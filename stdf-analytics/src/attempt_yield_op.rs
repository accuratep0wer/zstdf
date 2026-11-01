// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
//! Typed attempt-yield request/result helpers shared by the CLI evidence adapter.
//!
//! Evidence I/O, snapshot verification and receipt publication remain in stdf-cli.
//! This module owns validation, scope compilation, population filtering and
//! canonical result encoding. Dashboard coordinate-v1 merging is intentionally
//! out of scope.

use crate::attempt_yield::{AttemptDisposition, Verdict, YieldAccumulator, YieldSummary};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io;

pub const REQUEST_VERSION: &str = "attempt-yield-request-v1";
pub const RESULT_VERSION: &str = "attempt-yield-result-v1";
pub const RECEIPT_VERSION: &str = "analysis-receipt-v1";
pub const OPERATION_ALL_ATTEMPTS: &str = "prr-attempt-yield-v1";
pub const OPERATION_LATEST_DEVICE: &str = "prr-latest-device-yield-v1";
pub const POPULATION_ALL_ATTEMPTS: &str = "all_attempts";
pub const POPULATION_LATEST_DEVICE: &str = "latest_device";
pub const SOURCE_POLICY: &str = "catalog-source-id-v1";
pub const IDENTITY_LATEST: &str = "latest-ecid-start-t-v1";
pub const MAX_REQUEST_BYTES: u64 = 1024 * 1024;
pub const MAX_EVIDENCE_IDS: usize = 256;

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: String,
    pub snapshot_id: String,
    pub evidence_ids: Vec<String>,
    pub population: String,
    pub source_policy: String,
    pub allow_partial: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity_policy: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<Scope>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exclude_attempts: Option<Vec<ExcludeAttempt>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lot_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wafer_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sites: Option<Vec<SiteFilter>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runs: Option<Vec<RunFilter>>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SiteFilter {
    pub head: u8,
    pub site: u8,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RunFilter {
    pub source_id: String,
    pub run_id: String,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ExcludeAttempt {
    pub source_id: String,
    pub attempt_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Population {
    AllAttempts,
    LatestDevice,
}

impl Population {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AllAttempts => POPULATION_ALL_ATTEMPTS,
            Self::LatestDevice => POPULATION_LATEST_DEVICE,
        }
    }

    pub fn operation(self) -> &'static str {
        match self {
            Self::AllAttempts => OPERATION_ALL_ATTEMPTS,
            Self::LatestDevice => OPERATION_LATEST_DEVICE,
        }
    }
}

/// Minimal PRR fields required by the yield adapter (not the viewer Row schema).
#[derive(Clone, Debug)]
pub struct AttemptRecord {
    pub attempt: u64,
    pub run: u64,
    pub lot: String,
    pub wafer: Option<String>,
    pub head: u8,
    pub site: u8,
    pub flags: u8,
    pub ecid: Option<String>,
    /// Viewer/retained latest flag: true selected, false earlier, None unresolved.
    pub latest: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct SourceMeta {
    pub source_id: String,
    pub evidence_id: String,
    pub source_sha256: String,
    pub content_sha256: String,
    pub available_attempts: u64,
}

#[derive(Debug)]
pub struct CompiledScope {
    lots: Option<HashSet<String>>,
    wafers: Option<HashSet<String>>,
    sites: Option<HashSet<(u8, u8)>>,
    runs: Option<HashSet<(String, u64)>>,
}

#[derive(Clone, Debug)]
pub struct Coverage {
    pub snapshot_sources: u64,
    pub selected_sources: u64,
    pub missing_source_ids: Vec<String>,
    pub partial: bool,
    pub expected_inventory: String,
    pub content_alias_source_count: u64,
    pub catalog_run_status: String,
    pub catalog_failure_count: u64,
}

#[derive(Clone, Debug)]
pub struct BuildIdentity {
    pub package_version: String,
    pub git_sha: Option<String>,
    pub target: String,
}

pub fn parse_request(bytes: &[u8]) -> Result<Request, String> {
    if bytes.len() as u64 > MAX_REQUEST_BYTES {
        return Err("attempt-yield request exceeds 1 MiB".into());
    }
    let request: Request = serde_json::from_slice(bytes)
        .map_err(|e| format!("invalid attempt-yield request: {e}"))?;
    validate_request(&request)?;
    Ok(request)
}

pub fn validate_request(request: &Request) -> Result<(), String> {
    if request.version != REQUEST_VERSION {
        return Err("unsupported attempt-yield request version".into());
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
            None => return Err("latest_device requires identity_policy latest-ecid-start-t-v1".into()),
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
    Ok(())
}

pub fn population_of(request: &Request) -> Result<Population, String> {
    match request.population.as_str() {
        POPULATION_ALL_ATTEMPTS => Ok(Population::AllAttempts),
        POPULATION_LATEST_DEVICE => Ok(Population::LatestDevice),
        _ => Err("unsupported population".into()),
    }
}

pub fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

pub fn parse_canonical_u64(raw: &str) -> Result<u64, String> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err("non-canonical decimal ID".into());
    }
    let value: u64 = raw.parse().map_err(|_| "non-canonical decimal ID")?;
    if value == 0 || value.to_string() != raw {
        return Err("non-canonical decimal ID".into());
    }
    Ok(value)
}

pub fn compile_scope(
    scope: Option<&Scope>,
    run_inventory: &HashMap<&str, &HashSet<u64>>,
) -> Result<CompiledScope, String> {
    let Some(scope) = scope else {
        return Ok(CompiledScope {
            lots: None,
            wafers: None,
            sites: None,
            runs: None,
        });
    };
    let lots = scope.lot_ids.as_ref().and_then(|ids| {
        if ids.is_empty() {
            None
        } else {
            Some(ids.iter().cloned().collect())
        }
    });
    let wafers = scope.wafer_ids.as_ref().and_then(|ids| {
        if ids.is_empty() {
            None
        } else {
            Some(ids.iter().cloned().collect())
        }
    });
    let sites = scope.sites.as_ref().and_then(|ids| {
        if ids.is_empty() {
            None
        } else {
            Some(ids.iter().map(|s| (s.head, s.site)).collect())
        }
    });
    let runs = match scope.runs.as_ref() {
        None => None,
        Some(ids) if ids.is_empty() => None,
        Some(ids) => {
            let mut set = HashSet::new();
            for run in ids {
                if !valid_id(&run.source_id) {
                    return Err("invalid run source_id".into());
                }
                let Some(run_ids) = run_inventory.get(run.source_id.as_str()) else {
                    return Err("run source_id is not among selected evidence".into());
                };
                let run_id = parse_canonical_u64(&run.run_id)?;
                if !run_ids.contains(&run_id) {
                    return Err("run_id is outside retained run inventory".into());
                }
                set.insert((run.source_id.clone(), run_id));
            }
            Some(set)
        }
    };
    Ok(CompiledScope {
        lots,
        wafers,
        sites,
        runs,
    })
}

pub fn compile_excludes(
    excludes: Option<&Vec<ExcludeAttempt>>,
    attempt_inventory: &HashMap<&str, u64>,
) -> Result<HashMap<String, HashSet<u64>>, String> {
    let mut out: HashMap<String, HashSet<u64>> = HashMap::new();
    let Some(excludes) = excludes else {
        return Ok(out);
    };
    for item in excludes {
        if !valid_id(&item.source_id) {
            return Err("invalid exclude source_id".into());
        }
        let Some(&available) = attempt_inventory.get(item.source_id.as_str()) else {
            return Err("exclude source_id is not among selected evidence".into());
        };
        let attempt_id = parse_canonical_u64(&item.attempt_id)?;
        if attempt_id > available {
            return Err("attempt_id is outside retained attempt inventory".into());
        }
        out.entry(item.source_id.clone())
            .or_default()
            .insert(attempt_id);
    }
    Ok(out)
}

pub fn in_scope(row: &AttemptRecord, source_id: &str, scope: &CompiledScope) -> bool {
    if let Some(lots) = &scope.lots {
        if !lots.contains(&row.lot) {
            return false;
        }
    }
    if let Some(wafers) = &scope.wafers {
        match &row.wafer {
            Some(wafer) if wafers.contains(wafer) => {}
            _ => return false,
        }
    }
    if let Some(sites) = &scope.sites {
        if !sites.contains(&(row.head, row.site)) {
            return false;
        }
    }
    if let Some(runs) = &scope.runs {
        if !runs.contains(&(source_id.to_owned(), row.run)) {
            return false;
        }
    }
    true
}

/// Returns true when the attempt belongs to the requested population before exclusions.
pub fn in_population(row: &AttemptRecord, population: Population) -> bool {
    match population {
        Population::AllAttempts => true,
        // Latest uses retained ECID/START_T selection flags; unresolved/earlier stay out.
        Population::LatestDevice => row.latest == Some(true),
    }
}

pub fn accumulate_source(
    rows: &[AttemptRecord],
    source_id: &str,
    population: Population,
    scope: &CompiledScope,
    excludes: Option<&HashSet<u64>>,
) -> io::Result<YieldAccumulator> {
    let mut acc = YieldAccumulator::default();
    for row in rows {
        if !in_population(row, population) || !in_scope(row, source_id, scope) {
            continue;
        }
        let disposition = AttemptDisposition {
            verdict: Verdict::from_prr_flags(row.flags),
            identity_resolved: row.ecid.is_some(),
            excluded: excludes.is_some_and(|set| set.contains(&row.attempt)),
        };
        acc.add(disposition)?;
    }
    Ok(acc)
}

pub fn content_alias_count<'a, I>(hashes: I) -> u64
where
    I: IntoIterator<Item = &'a str>,
{
    let mut counts: HashMap<&str, u64> = HashMap::new();
    for hash in hashes {
        *counts.entry(hash).or_default() += 1;
    }
    counts.values().filter(|&&n| n > 1).count() as u64
}

pub fn build_result(
    population: Population,
    total: &YieldSummary,
    per_source: &BTreeMap<String, (SourceMeta, YieldSummary)>,
    coverage: &Coverage,
) -> Value {
    let mut sources = Vec::with_capacity(per_source.len());
    for (source_id, (meta, summary)) in per_source {
        sources.push(json!({
            "source_id": source_id,
            "evidence_id": meta.evidence_id,
            "source_sha256": meta.source_sha256,
            "content_sha256": meta.content_sha256,
            "available_attempts": u64_str(meta.available_attempts),
            "counts": encode_counts(summary),
            "yields": encode_yields(summary),
        }));
    }
    json!({
        "version": RESULT_VERSION,
        "operation": population.operation(),
        "population": population.as_str(),
        "counts": encode_counts(total),
        "yields": encode_yields(total),
        "sources": sources,
        "coverage": {
            "snapshot_sources": u64_str(coverage.snapshot_sources),
            "selected_sources": u64_str(coverage.selected_sources),
            "missing_source_ids": coverage.missing_source_ids,
            "partial": coverage.partial,
            "expected_inventory": coverage.expected_inventory,
            "content_alias_source_count": u64_str(coverage.content_alias_source_count),
            "catalog_run_status": coverage.catalog_run_status,
            "catalog_failure_count": u64_str(coverage.catalog_failure_count),
        }
    })
}

pub fn encode_counts(summary: &YieldSummary) -> Value {
    let mut map = Map::new();
    map.insert("scoped_attempts".into(), json!(u64_str(summary.scoped_attempts)));
    map.insert("eligible".into(), json!(u64_str(summary.eligible)));
    map.insert("excluded".into(), json!(u64_str(summary.excluded)));
    map.insert("pass".into(), json!(u64_str(summary.pass)));
    map.insert("fail".into(), json!(u64_str(summary.fail)));
    map.insert("unknown".into(), json!(u64_str(summary.unknown)));
    map.insert("unresolved".into(), json!(u64_str(summary.unresolved)));
    Value::Object(map)
}

pub fn encode_yields(summary: &YieldSummary) -> Value {
    json!({
        "all_attempts": {
            "ratio": summary.all_attempts_yield.map(|r| json!({
                "numerator": u64_str(r.numerator),
                "denominator": u64_str(r.denominator),
            })),
            "null_reason": if summary.all_attempts_yield.is_none() {
                Value::String("zero-eligible-attempts".into())
            } else {
                Value::Null
            }
        },
        "known_verdict": {
            "ratio": summary.known_verdict_yield.map(|r| json!({
                "numerator": u64_str(r.numerator),
                "denominator": u64_str(r.denominator),
            })),
            "null_reason": if summary.known_verdict_yield.is_none() {
                Value::String("no-known-verdicts".into())
            } else {
                Value::Null
            }
        }
    })
}

pub fn u64_str(value: u64) -> String {
    value.to_string()
}

/// Canonical JSON bytes: object keys sorted recursively; arrays preserve order.
pub fn canonical_json(value: &Value) -> Result<Vec<u8>, String> {
    let normalized = normalize(value);
    serde_json::to_vec(&normalized).map_err(|e| e.to_string())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_encode(&hasher.finalize())
}

pub fn request_fingerprint(request: &Request) -> Result<String, String> {
    // Re-encode through Value so key order is canonical and defaults are explicit.
    let value = serde_json::to_value(request).map_err(|e| e.to_string())?;
    Ok(sha256_hex(&canonical_json(&value)?))
}

pub fn result_hash(result: &Value) -> Result<String, String> {
    Ok(sha256_hex(&canonical_json(result)?))
}

pub fn build_receipt(
    execution_id: &str,
    request: &Request,
    population: Population,
    result: &Value,
    coverage: &Coverage,
    build: &BuildIdentity,
    status: &str,
) -> Result<Value, String> {
    let request_fp = request_fingerprint(request)?;
    let result_fp = result_hash(result)?;
    Ok(json!({
        "version": RECEIPT_VERSION,
        "execution_id": execution_id,
        "status": status,
        "request_fingerprint": request_fp,
        "result_hash": result_fp,
        "snapshot_id": request.snapshot_id,
        "operation": population.operation(),
        "request_version": REQUEST_VERSION,
        "result_version": RESULT_VERSION,
        "population": population.as_str(),
        "source_policy": request.source_policy,
        "identity_policy": request.identity_policy,
        "allow_partial": request.allow_partial,
        "evidence_ids": request.evidence_ids,
        "coverage": {
            "snapshot_sources": u64_str(coverage.snapshot_sources),
            "selected_sources": u64_str(coverage.selected_sources),
            "missing_source_ids": coverage.missing_source_ids,
            "partial": coverage.partial,
            "expected_inventory": coverage.expected_inventory,
            "content_alias_source_count": u64_str(coverage.content_alias_source_count),
            "catalog_run_status": coverage.catalog_run_status,
            "catalog_failure_count": u64_str(coverage.catalog_failure_count),
        },
        "counts": result.get("counts").cloned().unwrap_or(Value::Null),
        "build": {
            "package_version": build.package_version,
            "git_sha": build.git_sha,
            "target": build.target,
        },
        "platform": std::env::consts::OS,
    }))
}

pub fn execution_id(request_fp: &str, result_fp: &str, build: &BuildIdentity) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"analysis-bundle-v1\0");
    hasher.update(request_fp.as_bytes());
    hasher.update(b"\0");
    hasher.update(result_fp.as_bytes());
    hasher.update(b"\0");
    hasher.update(build.package_version.as_bytes());
    hasher.update(b"\0");
    hasher.update(build.git_sha.as_deref().unwrap_or("unknown").as_bytes());
    hasher.update(b"\0");
    hasher.update(build.target.as_bytes());
    hex_encode(&hasher.finalize())
}

fn normalize(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut ordered = Map::new();
            let mut keys: Vec<_> = map.keys().cloned().collect();
            keys.sort();
            for key in keys {
                ordered.insert(key.clone(), normalize(&map[&key]));
            }
            Value::Object(ordered)
        }
        Value::Array(items) => Value::Array(items.iter().map(normalize).collect()),
        other => other.clone(),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0xf) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_population_requires_identity_policy() {
        let mut request = Request {
            version: REQUEST_VERSION.into(),
            snapshot_id: "a".repeat(64),
            evidence_ids: vec!["b".repeat(64)],
            population: POPULATION_LATEST_DEVICE.into(),
            source_policy: SOURCE_POLICY.into(),
            allow_partial: false,
            identity_policy: None,
            scope: None,
            exclude_attempts: None,
        };
        assert!(validate_request(&request).is_err());
        request.identity_policy = Some(IDENTITY_LATEST.into());
        assert!(validate_request(&request).is_ok());
    }

    #[test]
    fn canonical_hash_is_key_order_stable() {
        let a = json!({"z": 1, "a": {"y": 2, "b": 3}});
        let b = json!({"a": {"b": 3, "y": 2}, "z": 1});
        assert_eq!(result_hash(&a).unwrap(), result_hash(&b).unwrap());
    }

    #[test]
    fn latest_filter_keeps_only_selected_attempts() {
        let rows = [
            AttemptRecord {
                attempt: 1,
                run: 1,
                lot: "L".into(),
                wafer: Some("W".into()),
                head: 1,
                site: 1,
                flags: 0,
                ecid: Some("e1".into()),
                latest: Some(false),
            },
            AttemptRecord {
                attempt: 2,
                run: 1,
                lot: "L".into(),
                wafer: Some("W".into()),
                head: 1,
                site: 1,
                flags: 8,
                ecid: Some("e1".into()),
                latest: Some(true),
            },
            AttemptRecord {
                attempt: 3,
                run: 1,
                lot: "L".into(),
                wafer: Some("W".into()),
                head: 1,
                site: 2,
                flags: 0,
                ecid: Some("e2".into()),
                latest: None,
            },
        ];
        let scope = compile_scope(None, &HashMap::new()).unwrap();
        let summary = accumulate_source(
            &rows,
            &"s".repeat(64),
            Population::LatestDevice,
            &scope,
            None,
        )
        .unwrap()
        .summary();
        assert_eq!(summary.scoped_attempts, 1);
        assert_eq!(summary.fail, 1);
        assert_eq!(summary.pass, 0);
    }
}
