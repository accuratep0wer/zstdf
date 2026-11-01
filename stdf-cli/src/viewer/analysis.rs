// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
//! Attempt-complete PRR yield over retained viewer evidence (not Dashboard merge).
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
    result_hash, AttemptRecord, BuildIdentity, Coverage, Population, Request, SourceMeta,
};
use stdf_parquet::snapshot::verify_snapshot;

/// Execute an `attempt-yield-request-v1` against retained evidence for `dataset`.
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
        let request = read_request(request_path)?;
        let population = population_of(&request).map_err(|e| e.to_string())?;
        let snapshot = verify_snapshot(dataset, &request.snapshot_id)?;
        let catalog_incomplete = snapshot.catalog.run.status != "complete"
            || !snapshot.catalog.run.failures.is_empty();
        if catalog_incomplete && !request.allow_partial {
            return Err("snapshot catalog run is incomplete; allow_partial required".into());
        }

        if let Some(t) = telemetry.as_mut() {
            t.phase("load_evidence");
            t.counter("evidence_ids", request.evidence_ids.len() as u64);
        }

        let mut loaded = Vec::with_capacity(request.evidence_ids.len());
        let mut seen_sources = HashSet::new();
        for evidence_id in &request.evidence_ids {
            let mut attempts = evidence::open_attempts(dataset, evidence_id, limits)?;
            if attempts.bundle.snapshot_id != request.snapshot_id {
                return Err("evidence snapshot_id does not match request".into());
            }
            if !seen_sources.insert(attempts.bundle.source_id.clone()) {
                return Err("duplicate catalog source_id among selected evidence".into());
            }
            let mut rows = Vec::with_capacity(attempts.bundle.attempts as usize);
            while let Some(row) = attempts.next()? {
                // Cooperative cancel between retained attempt rows.
                limits_check(limits)?;
                rows.push(to_record(&row));
            }
            loaded.push(LoadedEvidence {
                bundle: attempts.bundle.clone(),
                run_ids: attempts.run_ids.clone(),
                rows,
            });
            drop(attempts);
        }

        let selected: BTreeSet<_> = loaded
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

        // compile_scope expects HashSet run inventories; retained evidence stores BTreeSet.
        let run_sets: HashMap<String, HashSet<u64>> = loaded
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
        let compiled = compile_scope(request.scope.as_ref(), &run_refs).map_err(|e| e.to_string())?;
        let attempt_inventory: HashMap<&str, u64> = loaded
            .iter()
            .map(|item| (item.bundle.source_id.as_str(), item.bundle.attempts))
            .collect();
        let excluded =
            compile_excludes(request.exclude_attempts.as_ref(), &attempt_inventory)
                .map_err(|e| e.to_string())?;

        if let Some(t) = telemetry.as_mut() {
            t.phase("reduce");
        }

        let mut total = YieldAccumulator::default();
        let mut per_source = BTreeMap::new();
        for item in &loaded {
            let mut acc = YieldAccumulator::default();
            let source_excludes = excluded.get(item.bundle.source_id.as_str());
            for row in &item.rows {
                limits_check(limits)?;
                if !in_population(row, population)
                    || !in_scope(row, &item.bundle.source_id, &compiled)
                {
                    continue;
                }
                acc.add(AttemptDisposition {
                    verdict: Verdict::from_prr_flags(row.flags),
                    identity_resolved: row.ecid.is_some(),
                    excluded: source_excludes.is_some_and(|set| set.contains(&row.attempt)),
                })?;
            }
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

        let alias_count = content_alias_count(loaded.iter().map(|i| i.bundle.content_sha256.as_str()));
        let partial = !missing.is_empty() || catalog_incomplete;
        let coverage = Coverage {
            snapshot_sources: snapshot.catalog.sources.len() as u64,
            selected_sources: selected.len() as u64,
            missing_source_ids: missing,
            partial,
            expected_inventory: snapshot.expected_inventory.clone(),
            content_alias_source_count: alias_count,
            catalog_run_status: snapshot.catalog.run.status.clone(),
            catalog_failure_count: snapshot.catalog.run.failures.len() as u64,
        };
        let mut result = build_result(population, &total.summary(), &per_source, &coverage);
        if let Some(t) = telemetry.as_mut() {
            t.counter(
                "scoped_attempts",
                total.summary().scoped_attempts,
            );
            t.phase("publish_receipt");
        }
        let analysis = publish_receipt(dataset, &request, population, &result, &coverage, limits)?;
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

fn validate_limits(limits: &EvidenceLimits) -> CliResult<()> {
    if limits.memory_limit_mib < 64 || limits.disk_limit_mib < 64 {
        return Err("evidence needs at least 64 MiB memory and disk".into());
    }
    limits_check(limits)
}

fn limits_check(limits: &EvidenceLimits) -> CliResult<()> {
    if limits
        .cancel_file
        .as_ref()
        .is_some_and(|path| path.exists())
    {
        return Err("evidence operation cancelled".into());
    }
    Ok(())
}

fn read_request(path: &Path) -> CliResult<Request> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    if len > attempt_yield_op::MAX_REQUEST_BYTES {
        return Err("attempt-yield request exceeds 1 MiB".into());
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    parse_request(&bytes).map_err(Into::into)
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

struct LoadedEvidence {
    bundle: EvidenceBundle,
    run_ids: BTreeSet<u64>,
    rows: Vec<AttemptRecord>,
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
) -> CliResult<Value> {
    limits_check(limits)?;
    let build = build_identity();
    let request_fp = request_fingerprint(request).map_err(|e| e.to_string())?;
    let result_fp = result_hash(result).map_err(|e| e.to_string())?;
    let id = execution_id(&request_fp, &result_fp, &build);
    let receipt = build_receipt(
        &id,
        request,
        population,
        result,
        coverage,
        &build,
        "complete",
    )
    .map_err(|e| e.to_string())?;

    let root = analysis_root(dataset)?;
    let target = root.join(&id);
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
        write_json(&stage.join("receipt.json"), &receipt)?;
        limits_check(limits)?;
        // create_new destination: complete bundles are immutable.
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
    let root = dataset.join("analysis");
    fs::create_dir_all(&root)?;
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
