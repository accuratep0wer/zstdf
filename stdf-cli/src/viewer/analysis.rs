// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
//! Shared retained-evidence analysis adapter (not Dashboard coordinate-v1 merge).
//!
//! Dispatches versioned request envelopes for attempt-yield, site / lot / wafer
//! comparison and retest impact. Inventory is collected without buffering all
//! attempts; each evidence source is then stream-scanned with cooperative cancel
//! checks. Corrupt selected evidence always fails before a complete result is
//! published. Retest still materializes one source at a time for ECID history.
use super::evidence::{self, EvidenceBundle, EvidenceLimits};
use super::{CliResult, Row};
use crate::telemetry::RunTelemetry;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use stdf_analytics::attempt_yield::{AttemptDisposition, Verdict, YieldAccumulator};
use stdf_analytics::attempt_yield_op::{
    self, build_receipt, build_result, compile_excludes, compile_scope, content_alias_count,
    execution_id, in_population, in_scope, parse_request, population_of, request_fingerprint,
    result_hash, AttemptRecord, BuildIdentity, Coverage, Population, ReceiptMeta, ReceiptParts,
    Request, SourceMeta, REQUEST_VERSION,
};
use stdf_analytics::summary_ops::{
    self, accumulate_groups, accumulate_retest, build_grouped_result, build_retest_result,
    merge_group_maps, summaries_of, SummaryKind,
};
use stdf_parquet::snapshot::verify_snapshot;

#[cfg(test)]
thread_local! {
    static FAIL_AFTER_RESULT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static FAIL_BEFORE_RENAME: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    /// After this many successful `limits_check` calls, the next check fails as cancelled.
    static CANCEL_AFTER_CHECKS: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
    static CHECKS_SEEN: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn inject_fail_after_result() {
    FAIL_AFTER_RESULT.with(|v| v.set(true));
}

#[cfg(test)]
pub(crate) fn inject_fail_before_rename() {
    FAIL_BEFORE_RENAME.with(|v| v.set(true));
}

/// Fail the next `limits_check` after `n` successful checks (mid-scan cancel).
#[cfg(test)]
pub(crate) fn inject_cancel_after_checks(n: u32) {
    CHECKS_SEEN.with(|v| v.set(0));
    CANCEL_AFTER_CHECKS.with(|v| v.set(Some(n)));
}

#[derive(Clone, Copy, Debug)]
enum OpKind {
    AttemptYield,
    Summary(SummaryKind),
}

impl OpKind {
    fn receipt_meta(self, population: Population) -> ReceiptMeta<'static> {
        match self {
            Self::AttemptYield => ReceiptMeta::attempt_yield(population),
            Self::Summary(kind) => ReceiptMeta {
                operation: kind.operation(),
                request_version: kind.request_version(),
                result_version: kind.result_version(),
            },
        }
    }
}

struct EvidenceMeta {
    bundle: EvidenceBundle,
    run_ids: BTreeSet<u64>,
}

/// Execute a versioned analysis request against retained evidence for `dataset`.
pub(crate) fn execute_with_telemetry(
    dataset: &Path,
    request_path: &Path,
    limits: &EvidenceLimits,
    mut telemetry: Option<&mut RunTelemetry>,
) -> CliResult<Value> {
    if let Some(t) = telemetry.as_mut() {
        t.phase("validate");
        t.budget("memory_mib", 0, limits.memory_limit_mib as u64);
        t.budget("disk_mib", 0, limits.disk_limit_mib);
    }
    let outcome = (|| -> CliResult<Value> {
        validate_limits(limits)?;
        let (op, request) = read_request(request_path)?;
        let population = population_of(&request).map_err(|e| e.to_string())?;
        let snapshot = verify_snapshot(dataset, &request.snapshot_id)?;
        let catalog_incomplete =
            snapshot.catalog.run.status != "complete" || !snapshot.catalog.run.failures.is_empty();
        if catalog_incomplete && !request.allow_partial {
            return Err("snapshot catalog run is incomplete; allow_partial required".into());
        }

        if let Some(t) = telemetry.as_mut() {
            t.phase("load_evidence");
            t.counter("evidence_ids", request.evidence_ids.len() as u64);
        }

        // Phase 1: open each evidence for inventory/run IDs only (release lock
        // before the next). Scope compiles from manifests; attempt bodies are
        // not buffered across sources.
        let mut metas = Vec::with_capacity(request.evidence_ids.len());
        let mut seen_sources = HashSet::new();
        for evidence_id in &request.evidence_ids {
            deny_traversal_id(evidence_id, "evidence_id")?;
            let attempts = evidence::open_attempts(dataset, evidence_id, limits)?;
            if attempts.bundle.snapshot_id != request.snapshot_id {
                return Err("evidence snapshot_id does not match request".into());
            }
            if !seen_sources.insert(attempts.bundle.source_id.clone()) {
                return Err("duplicate catalog source_id among selected evidence".into());
            }
            metas.push(EvidenceMeta {
                bundle: attempts.bundle.clone(),
                run_ids: attempts.run_ids.clone(),
            });
            drop(attempts);
        }

        let selected: BTreeSet<_> = metas
            .iter()
            .map(|item| item.bundle.source_id.clone())
            .collect();
        let mut missing: Vec<_> = snapshot
            .catalog
            .sources
            .keys()
            .filter(|id| !selected.contains(*id))
            .cloned()
            .collect();
        missing.sort();
        if !missing.is_empty() && !request.allow_partial {
            return Err("selected evidence does not cover every snapshot catalog source".into());
        }

        let run_sets: HashMap<String, HashSet<u64>> = metas
            .iter()
            .map(|item| {
                (
                    item.bundle.source_id.clone(),
                    item.run_ids.iter().copied().collect(),
                )
            })
            .collect();
        let run_refs: HashMap<&str, &HashSet<u64>> =
            run_sets.iter().map(|(k, v)| (k.as_str(), v)).collect();
        let compiled =
            compile_scope(request.scope.as_ref(), &run_refs).map_err(|e| e.to_string())?;
        let attempt_inventory: HashMap<&str, u64> = metas
            .iter()
            .map(|item| (item.bundle.source_id.as_str(), item.bundle.attempts))
            .collect();
        let excluded = compile_excludes(request.exclude_attempts.as_ref(), &attempt_inventory)
            .map_err(|e| e.to_string())?;

        if let Some(t) = telemetry.as_mut() {
            t.phase("reduce");
        }

        let bundles: Vec<_> = metas.iter().map(|i| i.bundle.clone()).collect();
        // Phase 2: re-open each evidence and stream-scan. Full scan always runs
        // so corrupt indexes fail even when scope matches zero rows. Scanned
        // count must equal retained attempt inventory.
        let mut result = match op {
            OpKind::AttemptYield => {
                let mut total = YieldAccumulator::default();
                let mut per_source = BTreeMap::new();
                for item in &metas {
                    let source_excludes = excluded.get(item.bundle.source_id.as_str());
                    let mut acc = YieldAccumulator::default();
                    stream_attempts(dataset, &item.bundle, limits, |row| {
                        if !in_population(&row, population)
                            || !in_scope(&row, &item.bundle.source_id, &compiled)
                        {
                            return Ok(());
                        }
                        acc.add(AttemptDisposition {
                            verdict: Verdict::from_prr_flags(row.flags),
                            identity_resolved: row.ecid.is_some(),
                            excluded: source_excludes.is_some_and(|set| set.contains(&row.attempt)),
                        })?;
                        Ok(())
                    })?;
                    total.merge(&acc)?;
                    per_source.insert(
                        item.bundle.source_id.clone(),
                        (
                            SourceMeta {
                                source_id: item.bundle.source_id.clone(),
                                evidence_id: item.bundle.evidence_id.clone(),
                                source_sha256: item.bundle.source_sha256.clone(),
                                content_sha256: item.bundle.content_sha256.clone(),
                                available_attempts: item.bundle.attempts,
                            },
                            acc.summary(),
                        ),
                    );
                }
                if let Some(t) = telemetry.as_mut() {
                    t.counter("scoped_attempts", total.summary().scoped_attempts);
                }
                let coverage =
                    coverage_of(&snapshot, &selected, missing, catalog_incomplete, &bundles);
                build_result(population, &total.summary(), &per_source, &coverage)
            }
            OpKind::Summary(SummaryKind::RetestImpact) => {
                let mut unresolved = 0u64;
                let mut devices = 0u64;
                let mut retested = 0u64;
                let mut extra = 0u64;
                let mut changed = 0u64;
                let mut first_fail = 0u64;
                let mut recovered = 0u64;
                let mut first_pass = 0u64;
                let mut degraded = 0u64;
                let mut first_total = YieldAccumulator::default();
                let mut latest_total = YieldAccumulator::default();
                for item in &metas {
                    limits_check(limits)?;
                    let source_excludes = excluded.get(item.bundle.source_id.as_str());
                    // One-source buffer only: ECID history needs the full source
                    // attempt set, but not every selected source at once.
                    let mut rows = Vec::with_capacity(item.bundle.attempts as usize);
                    stream_attempts(dataset, &item.bundle, limits, |row| {
                        rows.push(row);
                        Ok(())
                    })?;
                    let part = accumulate_retest(
                        rows,
                        &item.bundle.source_id,
                        population,
                        &compiled,
                        source_excludes,
                    )?;
                    unresolved = unresolved
                        .checked_add(part.unresolved_identity_attempts)
                        .ok_or("count overflow")?;
                    devices = devices.checked_add(part.devices).ok_or("count overflow")?;
                    retested = retested
                        .checked_add(part.retested_devices)
                        .ok_or("count overflow")?;
                    extra = extra
                        .checked_add(part.extra_attempts)
                        .ok_or("count overflow")?;
                    changed = changed
                        .checked_add(part.ever_verdict_changed)
                        .ok_or("count overflow")?;
                    first_fail = first_fail
                        .checked_add(part.first_fail_known)
                        .ok_or("count overflow")?;
                    recovered = recovered
                        .checked_add(part.recovered)
                        .ok_or("count overflow")?;
                    first_pass = first_pass
                        .checked_add(part.first_pass_known)
                        .ok_or("count overflow")?;
                    degraded = degraded
                        .checked_add(part.degraded)
                        .ok_or("count overflow")?;
                    merge_summary_into(&mut first_total, &part.first)?;
                    merge_summary_into(&mut latest_total, &part.latest)?;
                }
                let combined = summary_ops::RetestImpactSummary {
                    unresolved_identity_attempts: unresolved,
                    devices,
                    retested_devices: retested,
                    extra_attempts: extra,
                    ever_verdict_changed: changed,
                    first: first_total.summary(),
                    latest: latest_total.summary(),
                    recovery_rate_known: ratio(recovered, first_fail),
                    degradation_rate_known: ratio(degraded, first_pass),
                    first_fail_known: first_fail,
                    recovered,
                    first_pass_known: first_pass,
                    degraded,
                };
                let coverage =
                    coverage_of(&snapshot, &selected, missing, catalog_incomplete, &bundles);
                build_retest_result(population, &combined, &coverage)
            }
            OpKind::Summary(kind) => {
                let mut total = YieldAccumulator::default();
                let mut groups: BTreeMap<String, YieldAccumulator> = BTreeMap::new();
                for item in &metas {
                    limits_check(limits)?;
                    let source_excludes = excluded.get(item.bundle.source_id.as_str());
                    let mut part_total = YieldAccumulator::default();
                    let mut part_groups: BTreeMap<String, YieldAccumulator> = BTreeMap::new();
                    stream_attempts(dataset, &item.bundle, limits, |row| {
                        let (acc_total, acc_groups) = accumulate_groups(
                            kind,
                            std::iter::once(row),
                            &item.bundle.source_id,
                            population,
                            &compiled,
                            source_excludes,
                        )?;
                        part_total.merge(&acc_total)?;
                        merge_group_maps(&mut part_groups, acc_groups)?;
                        Ok(())
                    })?;
                    total.merge(&part_total)?;
                    merge_group_maps(&mut groups, part_groups)?;
                }
                if let Some(t) = telemetry.as_mut() {
                    t.counter("scoped_attempts", total.summary().scoped_attempts);
                }
                let coverage =
                    coverage_of(&snapshot, &selected, missing, catalog_incomplete, &bundles);
                build_grouped_result(
                    kind,
                    population,
                    &total.summary(),
                    &summaries_of(&groups),
                    &coverage,
                )
            }
        };

        let coverage_for_receipt = coverage_from_result(&result)?;
        if let Some(t) = telemetry.as_mut() {
            t.phase("publish_receipt");
        }
        let analysis = publish_receipt(
            dataset,
            &request,
            population,
            &result,
            &coverage_for_receipt,
            limits,
            op.receipt_meta(population),
        )?;
        if let Value::Object(map) = &mut result {
            map.insert("analysis".into(), analysis);
        }
        Ok(result)
    })();

    match (&outcome, telemetry) {
        (Ok(_), Some(t)) => t.finish_ok(),
        (Err(error), Some(t)) => t.finish_from_error(&error.to_string()),
        _ => {}
    }
    outcome
}

fn ratio(num: u64, den: u64) -> Option<stdf_analytics::attempt_yield::Ratio> {
    (den != 0).then_some(stdf_analytics::attempt_yield::Ratio {
        numerator: num,
        denominator: den,
    })
}

fn merge_summary_into(
    acc: &mut YieldAccumulator,
    summary: &stdf_analytics::attempt_yield::YieldSummary,
) -> CliResult<()> {
    for _ in 0..summary.pass {
        acc.add(AttemptDisposition {
            verdict: Verdict::Pass,
            identity_resolved: true,
            excluded: false,
        })?;
    }
    for _ in 0..summary.fail {
        acc.add(AttemptDisposition {
            verdict: Verdict::Fail,
            identity_resolved: true,
            excluded: false,
        })?;
    }
    for _ in 0..summary.unknown {
        acc.add(AttemptDisposition {
            verdict: Verdict::Unknown,
            identity_resolved: true,
            excluded: false,
        })?;
    }
    for _ in 0..summary.excluded {
        acc.add(AttemptDisposition {
            verdict: Verdict::Pass,
            identity_resolved: true,
            excluded: true,
        })?;
    }
    Ok(())
}

fn coverage_of(
    snapshot: &stdf_parquet::snapshot::Snapshot,
    selected: &BTreeSet<String>,
    missing: Vec<String>,
    catalog_incomplete: bool,
    metas: &[EvidenceBundle],
) -> Coverage {
    let alias_count = content_alias_count(metas.iter().map(|b| b.content_sha256.as_str()));
    let partial = !missing.is_empty() || catalog_incomplete;
    Coverage {
        snapshot_sources: snapshot.catalog.sources.len() as u64,
        selected_sources: selected.len() as u64,
        missing_source_ids: missing,
        partial,
        expected_inventory: snapshot.expected_inventory.clone(),
        content_alias_source_count: alias_count,
        catalog_run_status: snapshot.catalog.run.status.clone(),
        catalog_failure_count: snapshot.catalog.run.failures.len() as u64,
    }
}

fn coverage_from_result(result: &Value) -> CliResult<Coverage> {
    let c = result
        .get("coverage")
        .ok_or("analysis result missing coverage")?;
    Ok(Coverage {
        snapshot_sources: parse_u64_field(c, "snapshot_sources")?,
        selected_sources: parse_u64_field(c, "selected_sources")?,
        missing_source_ids: c
            .get("missing_source_ids")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        partial: c.get("partial").and_then(|v| v.as_bool()).unwrap_or(false),
        expected_inventory: c
            .get("expected_inventory")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_owned(),
        content_alias_source_count: parse_u64_field(c, "content_alias_source_count")?,
        catalog_run_status: c
            .get("catalog_run_status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_owned(),
        catalog_failure_count: parse_u64_field(c, "catalog_failure_count")?,
    })
}

fn parse_u64_field(value: &Value, key: &str) -> CliResult<u64> {
    let raw = value
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("coverage.{key} missing"))?;
    raw.parse()
        .map_err(|_| format!("coverage.{key} is not a decimal u64").into())
}

fn validate_limits(limits: &EvidenceLimits) -> CliResult<()> {
    if limits.memory_limit_mib < 64 || limits.disk_limit_mib < 64 {
        return Err("evidence needs at least 64 MiB memory and disk".into());
    }
    limits_check(limits)
}

fn limits_check(limits: &EvidenceLimits) -> CliResult<()> {
    #[cfg(test)]
    if let Some(n) = CANCEL_AFTER_CHECKS.with(|v| v.get()) {
        let seen = CHECKS_SEEN.with(|c| {
            let next = c.get().saturating_add(1);
            c.set(next);
            next
        });
        if seen > n {
            CANCEL_AFTER_CHECKS.with(|v| v.set(None));
            return Err("evidence operation cancelled".into());
        }
    }
    if limits
        .cancel_file
        .as_ref()
        .is_some_and(|path| path.exists())
    {
        return Err("evidence operation cancelled".into());
    }
    Ok(())
}

fn deny_traversal_id(id: &str, label: &str) -> CliResult<()> {
    if id.contains('/') || id.contains('\\') || id.contains("..") || id.contains('\0') {
        return Err(format!("{label} path traversal denied").into());
    }
    if !attempt_yield_op::valid_id(id) {
        return Err(format!("invalid {label}").into());
    }
    Ok(())
}

fn read_request(path: &Path) -> CliResult<(OpKind, Request)> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    if len > attempt_yield_op::MAX_REQUEST_BYTES {
        return Err("analysis request exceeds 1 MiB".into());
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let version = peek_version(&bytes)?;
    if version == REQUEST_VERSION {
        let request = parse_request(&bytes).map_err(|e| e.to_string())?;
        deny_traversal_id(&request.snapshot_id, "snapshot_id")?;
        for id in &request.evidence_ids {
            deny_traversal_id(id, "evidence_id")?;
        }
        Ok((OpKind::AttemptYield, request))
    } else if summary_ops::kind_of_version(&version).is_some() {
        let (parsed_kind, request) =
            summary_ops::parse_summary_request(&bytes).map_err(|e| e.to_string())?;
        deny_traversal_id(&request.snapshot_id, "snapshot_id")?;
        for id in &request.evidence_ids {
            deny_traversal_id(id, "evidence_id")?;
        }
        Ok((OpKind::Summary(parsed_kind), request))
    } else {
        Err("unsupported analysis request version".into())
    }
}

fn peek_version(bytes: &[u8]) -> CliResult<String> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|e| format!("invalid analysis request: {e}"))?;
    value
        .get("version")
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .ok_or_else(|| "analysis request missing version".into())
}

fn to_record(row: &Row) -> AttemptRecord {
    AttemptRecord {
        attempt: row.attempt,
        run: row.run,
        lot: row.lot.clone(),
        wafer: row.wafer.clone(),
        head: row.head,
        site: row.site,
        flags: row.flags,
        ecid: row.ecid.clone(),
        latest: row.latest,
    }
}

/// Re-open retained attempts, stream every row, and enforce inventory parity.
fn stream_attempts(
    dataset: &Path,
    bundle: &EvidenceBundle,
    limits: &EvidenceLimits,
    mut on_row: impl FnMut(AttemptRecord) -> CliResult<()>,
) -> CliResult<()> {
    let mut attempts = evidence::open_attempts(dataset, &bundle.evidence_id, limits)?;
    if attempts.bundle.evidence_id != bundle.evidence_id
        || attempts.bundle.source_id != bundle.source_id
        || attempts.bundle.attempts != bundle.attempts
    {
        return Err("evidence inventory changed between analysis phases".into());
    }
    let mut scanned = 0u64;
    while let Some(row) = attempts.next()? {
        limits_check(limits)?;
        scanned = scanned
            .checked_add(1)
            .ok_or_else(|| "attempt scan count overflow".to_string())?;
        on_row(to_record(&row))?;
    }
    if scanned != bundle.attempts {
        return Err("scanned attempt count does not match retained inventory".into());
    }
    Ok(())
}

fn build_identity() -> BuildIdentity {
    BuildIdentity {
        package_version: env!("CARGO_PKG_VERSION").into(),
        git_sha: option_env!("VERGEN_GIT_SHA").map(str::to_owned),
        target: std::env::consts::ARCH.into(),
    }
}

fn publish_receipt(
    dataset: &Path,
    request: &Request,
    population: Population,
    result: &Value,
    coverage: &Coverage,
    limits: &EvidenceLimits,
    meta: ReceiptMeta<'_>,
) -> CliResult<Value> {
    limits_check(limits)?;
    let build = build_identity();
    let request_fp = request_fingerprint(request).map_err(|e| e.to_string())?;
    let result_fp = result_hash(result).map_err(|e| e.to_string())?;
    let id = execution_id(&request_fp, &result_fp, &build);
    deny_traversal_id(&id, "execution_id")?;
    let receipt = build_receipt(ReceiptParts {
        execution_id: &id,
        request,
        population,
        result,
        coverage,
        build: &build,
        status: "complete",
        meta,
    })
    .map_err(|e| e.to_string())?;

    let root = analysis_root(dataset)?;
    let target = root.join(&id);
    if !target.starts_with(&root) {
        return Err("analysis path escape denied".into());
    }
    if target.exists() {
        verify_existing_bundle(&target, result, &receipt)?;
        return Ok(analysis_ref(&id, &request_fp, &result_fp));
    }

    let stage = root.join(format!(
        ".staging-{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos(),
        &id[..16]
    ));
    fs::create_dir_all(&stage)?;
    let cleanup = |stage: &Path| {
        let _ = fs::remove_dir_all(stage);
    };
    let write = (|| -> CliResult<()> {
        write_json(&stage.join("result.json"), result)?;
        #[cfg(test)]
        if FAIL_AFTER_RESULT.with(|v| v.replace(false)) {
            return Err("injected failure after result.json".into());
        }
        write_json(&stage.join("receipt.json"), &receipt)?;
        limits_check(limits)?;
        #[cfg(test)]
        if FAIL_BEFORE_RENAME.with(|v| v.replace(false)) {
            return Err("injected failure before receipt rename".into());
        }
        match fs::rename(&stage, &target) {
            Ok(()) => Ok(()),
            Err(error) => {
                if target.exists() {
                    verify_existing_bundle(&target, result, &receipt)?;
                    Ok(())
                } else {
                    Err(error.into())
                }
            }
        }
    })();
    if write.is_err() {
        cleanup(&stage);
        return write.map(|_| unreachable!());
    }
    let _ = fs::remove_dir_all(&stage);
    Ok(analysis_ref(&id, &request_fp, &result_fp))
}

fn analysis_ref(id: &str, request_fp: &str, result_fp: &str) -> Value {
    json!({
        "execution_id": id,
        "receipt_version": attempt_yield_op::RECEIPT_VERSION,
        "status": "complete",
        "request_fingerprint": request_fp,
        "result_hash": result_fp,
        "relative_path": format!("analysis/{id}"),
    })
}

fn analysis_root(dataset: &Path) -> CliResult<PathBuf> {
    let dataset = dataset
        .canonicalize()
        .map_err(|e| format!("dataset path: {e}"))?;
    let root = dataset.join("analysis");
    fs::create_dir_all(&root)?;
    let root = root
        .canonicalize()
        .map_err(|e| format!("analysis root: {e}"))?;
    if !root.starts_with(&dataset) {
        return Err("analysis root escapes dataset".into());
    }
    Ok(root)
}

fn write_json(path: &Path, value: &Value) -> CliResult<()> {
    let bytes = serde_json::to_vec_pretty(value)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

fn verify_existing_bundle(target: &Path, result: &Value, receipt: &Value) -> CliResult<()> {
    let existing_result: Value = serde_json::from_reader(File::open(target.join("result.json"))?)?;
    let existing_receipt: Value =
        serde_json::from_reader(File::open(target.join("receipt.json"))?)?;
    if existing_result != *result {
        return Err("analysis bundle result mismatch for execution_id".into());
    }
    if existing_receipt != *receipt {
        return Err("analysis bundle receipt mismatch for execution_id".into());
    }
    Ok(())
}
