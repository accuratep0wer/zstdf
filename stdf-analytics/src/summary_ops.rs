// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
//! ANA-002 site / lot / retest summaries over physical PRR attempts.
//!
//! These reducers reuse the attempt-yield request envelope and
//! [`YieldAccumulator`] counting rules. Dashboard `coordinate-v1` merging is
//! intentionally out of scope.

use crate::attempt_yield::{AttemptDisposition, Ratio, Verdict, YieldAccumulator, YieldSummary};
use crate::attempt_yield_op::{
    encode_counts, encode_yields, in_population, in_scope, population_of, u64_str, valid_id,
    AttemptRecord, CompiledScope, Coverage, Population, Request, IDENTITY_LATEST, MAX_EVIDENCE_IDS,
    MAX_REQUEST_BYTES, SOURCE_POLICY,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};
use std::io;

pub const SITE_REQUEST_VERSION: &str = "site-variation-request-v1";
pub const SITE_RESULT_VERSION: &str = "site-variation-result-v1";
pub const SITE_OPERATION: &str = "prr-site-variation-v1";

pub const LOT_REQUEST_VERSION: &str = "lot-comparison-request-v1";
pub const LOT_RESULT_VERSION: &str = "lot-comparison-result-v1";
pub const LOT_OPERATION: &str = "prr-lot-comparison-v1";

pub const RETEST_REQUEST_VERSION: &str = "retest-impact-request-v1";
pub const RETEST_RESULT_VERSION: &str = "retest-impact-result-v1";
pub const RETEST_OPERATION: &str = "prr-retest-impact-v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SummaryKind {
    SiteVariation,
    LotComparison,
    RetestImpact,
}

impl SummaryKind {
    pub fn request_version(self) -> &'static str {
        match self {
            Self::SiteVariation => SITE_REQUEST_VERSION,
            Self::LotComparison => LOT_REQUEST_VERSION,
            Self::RetestImpact => RETEST_REQUEST_VERSION,
        }
    }

    pub fn result_version(self) -> &'static str {
        match self {
            Self::SiteVariation => SITE_RESULT_VERSION,
            Self::LotComparison => LOT_RESULT_VERSION,
            Self::RetestImpact => RETEST_RESULT_VERSION,
        }
    }

    pub fn operation(self) -> &'static str {
        match self {
            Self::SiteVariation => SITE_OPERATION,
            Self::LotComparison => LOT_OPERATION,
            Self::RetestImpact => RETEST_OPERATION,
        }
    }
}

pub fn kind_of_version(version: &str) -> Option<SummaryKind> {
    match version {
        SITE_REQUEST_VERSION => Some(SummaryKind::SiteVariation),
        LOT_REQUEST_VERSION => Some(SummaryKind::LotComparison),
        RETEST_REQUEST_VERSION => Some(SummaryKind::RetestImpact),
        _ => None,
    }
}

pub fn parse_summary_request(bytes: &[u8]) -> Result<(SummaryKind, Request), String> {
    if bytes.len() as u64 > MAX_REQUEST_BYTES {
        return Err("summary request exceeds 1 MiB".into());
    }
    let request: Request =
        serde_json::from_slice(bytes).map_err(|e| format!("invalid summary request: {e}"))?;
    let kind = kind_of_version(&request.version)
        .ok_or_else(|| "unsupported summary request version".to_string())?;
    validate_summary_request(kind, &request)?;
    Ok((kind, request))
}

pub fn validate_summary_request(kind: SummaryKind, request: &Request) -> Result<(), String> {
    if request.version != kind.request_version() {
        return Err("unsupported summary request version".into());
    }
    if !valid_id(&request.snapshot_id) {
        return Err("invalid snapshot_id".into());
    }
    let population = population_of(request)?;
    if request.source_policy != SOURCE_POLICY {
        return Err("unsupported source_policy".into());
    }
    match population {
        Population::AllAttempts => {
            if request.identity_policy.is_some() {
                return Err("identity_policy is not used for all_attempts".into());
            }
        }
        Population::LatestDevice => match request.identity_policy.as_deref() {
            Some(IDENTITY_LATEST) => {}
            Some(_) => return Err("unsupported identity_policy".into()),
            None => {
                return Err("latest_device requires identity_policy latest-ecid-start-t-v1".into())
            }
        },
    }
    if request.evidence_ids.is_empty() || request.evidence_ids.len() > MAX_EVIDENCE_IDS {
        return Err("evidence_ids length must be 1..=256".into());
    }
    let mut seen = HashSet::new();
    for id in &request.evidence_ids {
        if !valid_id(id) {
            return Err("invalid evidence_id".into());
        }
        if !seen.insert(id.as_str()) {
            return Err("duplicate evidence_id".into());
        }
    }
    let _ = kind;
    Ok(())
}

fn site_key(row: &AttemptRecord) -> String {
    format!("H{} / S{}", row.head, row.site)
}

fn lot_key(row: &AttemptRecord) -> String {
    row.lot.clone()
}

fn disposition(row: &AttemptRecord, excludes: Option<&HashSet<u64>>) -> AttemptDisposition {
    AttemptDisposition {
        verdict: Verdict::from_prr_flags(row.flags),
        identity_resolved: row.ecid.is_some(),
        excluded: excludes.is_some_and(|set| set.contains(&row.attempt)),
    }
}

/// Streaming site or lot aggregation. Caller owns cancel checks between rows.
pub fn accumulate_groups(
    kind: SummaryKind,
    rows: impl IntoIterator<Item = AttemptRecord>,
    source_id: &str,
    population: Population,
    scope: &CompiledScope,
    excludes: Option<&HashSet<u64>>,
) -> io::Result<(YieldAccumulator, BTreeMap<String, YieldAccumulator>)> {
    let mut total = YieldAccumulator::default();
    let mut groups: BTreeMap<String, YieldAccumulator> = BTreeMap::new();
    for row in rows {
        if !row_in_summary(kind, &row, population) || !in_scope(&row, source_id, scope) {
            continue;
        }
        let key = match kind {
            SummaryKind::SiteVariation => site_key(&row),
            SummaryKind::LotComparison => lot_key(&row),
            SummaryKind::RetestImpact => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "retest impact uses accumulate_retest",
                ))
            }
        };
        let disp = disposition(&row, excludes);
        total.add(disp)?;
        groups.entry(key).or_default().add(disp)?;
    }
    Ok((total, groups))
}

fn row_in_summary(kind: SummaryKind, row: &AttemptRecord, population: Population) -> bool {
    match kind {
        SummaryKind::SiteVariation | SummaryKind::LotComparison => in_population(row, population),
        // Retest needs full attempt history for selected devices; handled separately.
        SummaryKind::RetestImpact => true,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RetestImpactSummary {
    pub unresolved_identity_attempts: u64,
    pub devices: u64,
    pub retested_devices: u64,
    pub extra_attempts: u64,
    pub ever_verdict_changed: u64,
    pub first: YieldSummary,
    pub latest: YieldSummary,
    pub recovery_rate_known: Option<Ratio>,
    pub degradation_rate_known: Option<Ratio>,
    pub first_fail_known: u64,
    pub recovered: u64,
    pub first_pass_known: u64,
    pub degraded: u64,
}

#[derive(Clone, Debug)]
struct DeviceHistory {
    attempts: Vec<(u64, AttemptDisposition)>,
}

/// Retest impact over scoped attempts.
///
/// - `all_attempts`: every scoped non-population-filtered attempt participates.
/// - `latest_device`: only ECIDs with a selected `latest == true` attempt are kept;
///   for those devices, all scoped attempts contribute to first/last history.
pub fn accumulate_retest(
    rows: impl IntoIterator<Item = AttemptRecord>,
    source_id: &str,
    population: Population,
    scope: &CompiledScope,
    excludes: Option<&HashSet<u64>>,
) -> io::Result<RetestImpactSummary> {
    let mut unresolved = 0u64;
    let mut by_ecid: BTreeMap<String, DeviceHistory> = BTreeMap::new();
    let mut selected_latest: HashSet<String> = HashSet::new();

    for row in rows {
        if !in_scope(&row, source_id, scope) {
            continue;
        }
        let disp = disposition(&row, excludes);
        if disp.excluded {
            // Excluded attempts leave the population entirely for retest impact.
            continue;
        }
        let Some(ecid) = row.ecid.as_ref() else {
            unresolved = unresolved
                .checked_add(1)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "count overflow"))?;
            continue;
        };
        if row.latest == Some(true) {
            selected_latest.insert(ecid.clone());
        }
        // Collect full scoped history; latest_device membership is filtered after.
        by_ecid
            .entry(ecid.clone())
            .or_insert_with(|| DeviceHistory {
                attempts: Vec::new(),
            })
            .attempts
            .push((row.attempt, disp));
    }

    if population == Population::LatestDevice {
        by_ecid.retain(|ecid, _| selected_latest.contains(ecid));
    }

    let mut first_acc = YieldAccumulator::default();
    let mut latest_acc = YieldAccumulator::default();
    let mut retested = 0u64;
    let mut extra = 0u64;
    let mut changed = 0u64;
    let mut first_fail_known = 0u64;
    let mut recovered = 0u64;
    let mut first_pass_known = 0u64;
    let mut degraded = 0u64;

    for history in by_ecid.values_mut() {
        history.attempts.sort_by_key(|(attempt, _)| *attempt);
        let n = history.attempts.len() as u64;
        if n == 0 {
            continue;
        }
        if n > 1 {
            retested = retested
                .checked_add(1)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "count overflow"))?;
            extra = extra
                .checked_add(n - 1)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "count overflow"))?;
        }
        let Some((_, first)) = history.attempts.first() else {
            continue;
        };
        let Some((_, last)) = history.attempts.last() else {
            continue;
        };
        first_acc.add(AttemptDisposition {
            verdict: first.verdict,
            identity_resolved: true,
            excluded: false,
        })?;
        latest_acc.add(AttemptDisposition {
            verdict: last.verdict,
            identity_resolved: true,
            excluded: false,
        })?;

        let mut saw_pass = false;
        let mut saw_fail = false;
        for (_, d) in &history.attempts {
            match d.verdict {
                Verdict::Pass => saw_pass = true,
                Verdict::Fail => saw_fail = true,
                Verdict::Unknown => {}
            }
        }
        if saw_pass && saw_fail {
            changed = changed
                .checked_add(1)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "count overflow"))?;
        }

        match (first.verdict, last.verdict) {
            (Verdict::Fail, _) => {
                first_fail_known = first_fail_known
                    .checked_add(1)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "count overflow"))?;
                if last.verdict == Verdict::Pass {
                    recovered = recovered.checked_add(1).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "count overflow")
                    })?;
                }
            }
            (Verdict::Pass, _) => {
                first_pass_known = first_pass_known
                    .checked_add(1)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "count overflow"))?;
                if last.verdict == Verdict::Fail {
                    degraded = degraded.checked_add(1).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidData, "count overflow")
                    })?;
                }
            }
            _ => {}
        }
    }

    let ratio = |num, den| {
        (den != 0).then_some(Ratio {
            numerator: num,
            denominator: den,
        })
    };

    Ok(RetestImpactSummary {
        unresolved_identity_attempts: unresolved,
        devices: by_ecid.len() as u64,
        retested_devices: retested,
        extra_attempts: extra,
        ever_verdict_changed: changed,
        first: first_acc.summary(),
        latest: latest_acc.summary(),
        recovery_rate_known: ratio(recovered, first_fail_known),
        degradation_rate_known: ratio(degraded, first_pass_known),
        first_fail_known,
        recovered,
        first_pass_known,
        degraded,
    })
}

pub fn build_grouped_result(
    kind: SummaryKind,
    population: Population,
    total: &YieldSummary,
    groups: &BTreeMap<String, YieldSummary>,
    coverage: &Coverage,
) -> Value {
    let group_field = match kind {
        SummaryKind::SiteVariation => "sites",
        SummaryKind::LotComparison => "lots",
        SummaryKind::RetestImpact => "groups",
    };
    let mut items = Vec::with_capacity(groups.len());
    for (label, summary) in groups {
        items.push(json!({
            "label": label,
            "counts": encode_counts(summary),
            "yields": encode_yields(summary),
        }));
    }
    json!({
        "version": kind.result_version(),
        "operation": kind.operation(),
        "population": population.as_str(),
        "counts": encode_counts(total),
        "yields": encode_yields(total),
        group_field: items,
        "coverage": coverage_json(coverage),
    })
}

pub fn build_retest_result(
    population: Population,
    summary: &RetestImpactSummary,
    coverage: &Coverage,
) -> Value {
    let ratio_json = |r: Option<Ratio>, null_reason: &str| {
        json!({
            "ratio": r.map(|r| json!({
                "numerator": u64_str(r.numerator),
                "denominator": u64_str(r.denominator),
            })),
            "null_reason": if r.is_none() { Value::String(null_reason.into()) } else { Value::Null }
        })
    };
    json!({
        "version": RETEST_RESULT_VERSION,
        "operation": RETEST_OPERATION,
        "population": population.as_str(),
        "counts": {
            "unresolved_identity_attempts": u64_str(summary.unresolved_identity_attempts),
            "devices": u64_str(summary.devices),
            "retested_devices": u64_str(summary.retested_devices),
            "extra_attempts": u64_str(summary.extra_attempts),
            "ever_verdict_changed": u64_str(summary.ever_verdict_changed),
            "first_fail_known": u64_str(summary.first_fail_known),
            "recovered": u64_str(summary.recovered),
            "first_pass_known": u64_str(summary.first_pass_known),
            "degraded": u64_str(summary.degraded),
        },
        "first": {
            "counts": encode_counts(&summary.first),
            "yields": encode_yields(&summary.first),
        },
        "latest": {
            "counts": encode_counts(&summary.latest),
            "yields": encode_yields(&summary.latest),
        },
        "rates": {
            "retest_device_rate": ratio_json(
                (summary.devices != 0).then_some(Ratio {
                    numerator: summary.retested_devices,
                    denominator: summary.devices,
                }),
                "zero-devices",
            ),
            "recovery_rate_known": ratio_json(summary.recovery_rate_known, "no-first-fail-known"),
            "degradation_rate_known": ratio_json(
                summary.degradation_rate_known,
                "no-first-pass-known",
            ),
        },
        "coverage": coverage_json(coverage),
    })
}

fn coverage_json(coverage: &Coverage) -> Value {
    json!({
        "snapshot_sources": u64_str(coverage.snapshot_sources),
        "selected_sources": u64_str(coverage.selected_sources),
        "missing_source_ids": coverage.missing_source_ids,
        "partial": coverage.partial,
        "expected_inventory": coverage.expected_inventory,
        "content_alias_source_count": u64_str(coverage.content_alias_source_count),
        "catalog_run_status": coverage.catalog_run_status,
        "catalog_failure_count": u64_str(coverage.catalog_failure_count),
    })
}

/// Merge per-source group maps while preserving label order.
pub fn merge_group_maps(
    into: &mut BTreeMap<String, YieldAccumulator>,
    from: BTreeMap<String, YieldAccumulator>,
) -> io::Result<()> {
    for (key, acc) in from {
        into.entry(key).or_default().merge(&acc)?;
    }
    Ok(())
}

pub fn summaries_of(groups: &BTreeMap<String, YieldAccumulator>) -> BTreeMap<String, YieldSummary> {
    groups
        .iter()
        .map(|(k, v)| (k.clone(), v.summary()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attempt_yield_op::compile_scope;
    use std::collections::HashMap;

    fn row(
        attempt: u64,
        head: u8,
        site: u8,
        lot: &str,
        flags: u8,
        ecid: Option<&str>,
        latest: Option<bool>,
    ) -> AttemptRecord {
        AttemptRecord {
            attempt,
            run: 1,
            lot: lot.into(),
            wafer: Some("W1".into()),
            head,
            site,
            flags,
            ecid: ecid.map(str::to_owned),
            latest,
        }
    }

    #[test]
    fn site_variation_splits_by_head_site_and_keeps_unresolved() {
        let scope = compile_scope(None, &HashMap::new()).unwrap();
        let rows = [
            row(1, 1, 1, "L1", 0, Some("a"), Some(true)),
            row(2, 1, 2, "L1", 8, Some("b"), Some(true)),
            row(3, 1, 1, "L1", 0, None, None),
        ];
        let (total, groups) = accumulate_groups(
            SummaryKind::SiteVariation,
            rows,
            &"s".repeat(64),
            Population::AllAttempts,
            &scope,
            None,
        )
        .unwrap();
        let summary = total.summary();
        assert_eq!(summary.pass, 2);
        assert_eq!(summary.fail, 1);
        assert_eq!(summary.unresolved, 1);
        assert_eq!(groups["H1 / S1"].summary().pass, 2);
        assert_eq!(groups["H1 / S2"].summary().fail, 1);
    }

    #[test]
    fn lot_comparison_and_latest_population_filter() {
        let scope = compile_scope(None, &HashMap::new()).unwrap();
        let rows = [
            row(1, 1, 1, "L1", 0, Some("a"), Some(false)),
            row(2, 1, 1, "L2", 8, Some("b"), Some(true)),
            row(3, 1, 1, "L1", 0, Some("a"), Some(true)),
        ];
        let (total, groups) = accumulate_groups(
            SummaryKind::LotComparison,
            rows,
            &"s".repeat(64),
            Population::LatestDevice,
            &scope,
            None,
        )
        .unwrap();
        assert_eq!(total.summary().scoped_attempts, 2);
        assert_eq!(groups["L1"].summary().pass, 1);
        assert_eq!(groups["L2"].summary().fail, 1);
    }

    #[test]
    fn retest_impact_recovery_and_unresolved_are_visible() {
        let scope = compile_scope(None, &HashMap::new()).unwrap();
        let rows = [
            row(1, 1, 1, "L1", 8, Some("a"), Some(false)), // fail first
            row(2, 1, 1, "L1", 0, Some("a"), Some(true)),  // pass latest
            row(3, 1, 1, "L1", 0, None, None),             // unresolved
            row(4, 1, 1, "L1", 0, Some("b"), Some(true)),  // single pass
        ];
        let summary =
            accumulate_retest(rows, &"s".repeat(64), Population::AllAttempts, &scope, None)
                .unwrap();
        assert_eq!(summary.unresolved_identity_attempts, 1);
        assert_eq!(summary.devices, 2);
        assert_eq!(summary.retested_devices, 1);
        assert_eq!(summary.recovered, 1);
        assert_eq!(summary.first.fail, 1);
        assert_eq!(summary.first.pass, 1);
        assert_eq!(summary.latest.pass, 2);
        assert_eq!(
            summary.recovery_rate_known,
            Some(Ratio {
                numerator: 1,
                denominator: 1
            })
        );
    }

    #[test]
    fn retest_latest_device_keeps_history_only_for_selected_ecids() {
        let scope = compile_scope(None, &HashMap::new()).unwrap();
        let rows = [
            row(1, 1, 1, "L1", 8, Some("a"), Some(false)),
            row(2, 1, 1, "L1", 0, Some("a"), Some(true)),
            row(3, 1, 1, "L1", 8, Some("c"), Some(false)), // never selected latest
        ];
        let summary = accumulate_retest(
            rows,
            &"s".repeat(64),
            Population::LatestDevice,
            &scope,
            None,
        )
        .unwrap();
        assert_eq!(summary.devices, 1);
        assert_eq!(summary.retested_devices, 1);
        assert_eq!(summary.recovered, 1);
    }
}
