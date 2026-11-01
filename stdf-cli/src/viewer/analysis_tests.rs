// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
use super::analysis;
use super::evidence::{self, EvidenceLimits};
use super::*;
use stdf_parquet::catalog::{convert_dataset, ErrorPolicy};
use stdf_parquet::snapshot::{create_snapshot, select_snapshot};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = crate::tests::temp_path("attempt_yield");
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn dataset(&self) -> PathBuf {
        self.0.join("dataset")
    }

    fn source(&self) -> PathBuf {
        self.0.join("input.stdf")
    }

    fn convert(&self, inputs: &[PathBuf]) {
        convert_dataset(
            inputs,
            &self.dataset(),
            &[],
            &Default::default(),
            ErrorPolicy::FailFast,
        )
        .unwrap();
    }

    fn retain(&self, snapshot: &str, source: &str, input: &Path) -> Value {
        serde_json::to_value(
            evidence::retain(
                &self.dataset(),
                snapshot,
                source,
                input,
                &EvidenceLimits::default(),
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn setup(&self, bytes: &[u8]) -> (String, String, Value) {
        fs::write(self.source(), bytes).unwrap();
        self.convert(&[self.source()]);
        let snapshot = create_snapshot(&self.dataset(), false).unwrap();
        let source = snapshot.catalog.sources.keys().next().unwrap().clone();
        let bundle = self.retain(&snapshot.snapshot_id, &source, &self.source());
        (snapshot.snapshot_id, source, bundle)
    }

    fn aliases(&self) -> (String, Vec<Value>) {
        fs::write(self.source(), rich()).unwrap();
        let alias = self.0.join("copy.stdf");
        fs::write(&alias, rich()).unwrap();
        self.convert(&[self.source(), alias]);
        let snapshot = create_snapshot(&self.dataset(), false).unwrap();
        let bundles = snapshot
            .catalog
            .sources
            .keys()
            .map(|source| self.retain(&snapshot.snapshot_id, source, &self.source()))
            .collect();
        (snapshot.snapshot_id, bundles)
    }

    fn execute(&self, request: &Value) -> CliResult<Value> {
        self.execute_with(request, &EvidenceLimits::default())
    }

    fn execute_with(&self, request: &Value, limits: &EvidenceLimits) -> CliResult<Value> {
        let path = self.0.join("request.json");
        fs::write(&path, serde_json::to_vec(request).unwrap()).unwrap();
        analysis::execute_with_telemetry(&self.dataset(), &path, limits, None)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn request(snapshot: &str, bundles: &[Value]) -> Value {
    json!({
        "version": "attempt-yield-request-v1",
        "snapshot_id": snapshot,
        "evidence_ids": bundles.iter().map(|b| b["evidence_id"].clone()).collect::<Vec<_>>(),
        "population": "all_attempts",
        "source_policy": "catalog-source-id-v1",
        "allow_partial": false
    })
}

fn cn(s: &str) -> Vec<u8> {
    let mut out = vec![s.len() as u8];
    out.extend(s.as_bytes());
    out
}

fn rec(t: u8, s: u8, body: Vec<u8>) -> Vec<u8> {
    let mut out = (body.len() as u16).to_le_bytes().to_vec();
    out.extend([t, s]);
    out.extend(body);
    out
}

fn run(lot: &str, wafer: &str, start: u32) -> Vec<u8> {
    let mut mir = vec![0; 15];
    mir[4..8].copy_from_slice(&start.to_le_bytes());
    for s in [lot, "DEVICE", "NODE", "TESTER", "JOB"] {
        mir.extend(cn(s));
    }
    let mut out = rec(1, 10, mir);
    let mut wir = vec![1, 1];
    wir.extend(start.to_le_bytes());
    wir.extend(cn(wafer));
    out.extend(rec(2, 10, wir));
    out
}

fn start() -> Vec<u8> {
    let mut out = rec(0, 10, vec![2, 4]);
    out.extend(run("LOT-A", "W1", 100));
    out
}

fn finish(out: &mut Vec<u8>, time: u32) {
    out.extend(rec(1, 20, time.to_le_bytes().to_vec()));
}

fn ptr(site: u8, value: f32) -> Vec<u8> {
    let mut body = 10u32.to_le_bytes().to_vec();
    body.extend([1, site, 0, 0]);
    body.extend(value.to_le_bytes());
    body.extend(cn("Voltage"));
    body.extend(cn(""));
    body.extend([0, 0, 0, 0]);
    body.extend(0f32.to_le_bytes());
    body.extend(10f32.to_le_bytes());
    body.extend(cn("V"));
    rec(15, 10, body)
}

fn prr(head: u8, site: u8, flags: u8, x: i16) -> Vec<u8> {
    let mut body = vec![head, site, flags];
    for v in [3u16, 1, 10] {
        body.extend(v.to_le_bytes());
    }
    body.extend(x.to_le_bytes());
    body.extend(1i16.to_le_bytes());
    body.extend(10u32.to_le_bytes());
    body.extend(cn("REPEATED_PART"));
    rec(5, 20, body)
}

fn prr_only(out: &mut Vec<u8>, head: u8, site: u8, flags: u8, x: i16) {
    out.extend(rec(5, 10, vec![head, site]));
    out.extend(prr(head, site, flags, x));
}

fn rich() -> Vec<u8> {
    let mut out = start();
    // IDs follow PIR/first site activity, not PRR completion order.
    out.extend(rec(5, 10, vec![1, 1]));
    out.extend(rec(5, 10, vec![1, 2]));
    out.extend(ptr(2, 8.));
    out.extend(ptr(1, 1.));
    out.extend(ptr(1, 2.));
    let mut mpr = 20u32.to_le_bytes().to_vec();
    mpr.extend([1, 1, 0, 0]);
    mpr.extend(0u16.to_le_bytes());
    mpr.extend(2u16.to_le_bytes());
    mpr.extend(3f32.to_le_bytes());
    mpr.extend(4f32.to_le_bytes());
    mpr.extend(cn("Channels"));
    out.extend(rec(15, 15, mpr));
    for flag in [128, 0] {
        let mut ftr = 30u32.to_le_bytes().to_vec();
        ftr.extend([1, 2, flag, 0xc0]);
        ftr.extend([0; 32]);
        ftr.extend(cn("pattern/scan"));
        out.extend(rec(15, 20, ftr));
    }
    out.extend(prr(1, 2, 8, 2));
    out.extend(prr(1, 1, 0, 1));
    prr_only(&mut out, 1, 1, 4, 1);
    prr_only(&mut out, 1, 2, 0x18, 2);
    prr_only(&mut out, 1, 1, 0, i16::MIN);
    finish(&mut out, 200);
    out
}

fn counts(
    scoped: u64,
    eligible: u64,
    excluded: u64,
    pass: u64,
    fail: u64,
    unknown: u64,
    unresolved: u64,
) -> Value {
    json!({
        "scoped_attempts": scoped.to_string(),
        "eligible": eligible.to_string(),
        "excluded": excluded.to_string(),
        "pass": pass.to_string(),
        "fail": fail.to_string(),
        "unknown": unknown.to_string(),
        "unresolved": unresolved.to_string()
    })
}

fn yields(pass: u64, eligible: u64, known: u64) -> Value {
    let ratio = |denominator: u64| {
        (denominator > 0)
            .then(|| json!({"numerator": pass.to_string(), "denominator": denominator.to_string()}))
    };
    json!({
        "all_attempts": {
            "ratio": ratio(eligible),
            "null_reason": (eligible == 0).then_some("zero-eligible-attempts")
        },
        "known_verdict": {
            "ratio": ratio(known),
            "null_reason": (known == 0).then_some("no-known-verdicts")
        }
    })
}

#[test]
fn attempt_yield_counts_physical_prrs_not_measurements_latest_or_part_ids() {
    let f = Fixture::new();
    let (snapshot, source, bundle) = f.setup(&rich());
    assert_eq!(bundle["attempts"], 5);
    assert_eq!(bundle["executions"], 6);
    assert_eq!(bundle["measurements"], 8);
    let response = f.execute(&request(&snapshot, &[bundle.clone()])).unwrap();
    assert_eq!(response["version"], "attempt-yield-result-v1");
    assert_eq!(response["operation"], "prr-attempt-yield-v1");
    assert_eq!(response["population"], "all_attempts");
    assert_eq!(response["counts"], counts(5, 5, 0, 2, 1, 2, 1));
    assert_eq!(response["yields"], yields(2, 5, 3));
    assert_eq!(response["analysis"]["status"], "complete");
    assert_eq!(
        response["analysis"]["receipt_version"],
        "analysis-receipt-v1"
    );
    assert_eq!(
        response["analysis"]["request_fingerprint"]
            .as_str()
            .unwrap()
            .len(),
        64
    );
    assert_eq!(
        response["analysis"]["result_hash"].as_str().unwrap().len(),
        64
    );
    let published = f
        .dataset()
        .join(response["analysis"]["relative_path"].as_str().unwrap());
    assert!(published.join("result.json").is_file());
    assert!(published.join("receipt.json").is_file());
    assert_eq!(response["sources"].as_array().unwrap().len(), 1);
    assert_eq!(response["sources"][0]["source_id"], source);
    assert_eq!(response["sources"][0]["evidence_id"], bundle["evidence_id"]);
    assert_eq!(
        response["sources"][0]["source_sha256"],
        bundle["source_sha256"]
    );
    assert_eq!(
        response["sources"][0]["content_sha256"],
        bundle["content_sha256"]
    );
    assert_eq!(response["sources"][0]["available_attempts"], "5");
    assert_eq!(response["sources"][0]["counts"], response["counts"]);
    assert_eq!(response["sources"][0]["yields"], response["yields"]);
    assert_eq!(response["coverage"]["snapshot_sources"], "1");
    assert_eq!(response["coverage"]["selected_sources"], "1");
    assert_eq!(response["coverage"]["missing_source_ids"], json!([]));
    assert_eq!(response["coverage"]["partial"], false);
    assert_eq!(response["coverage"]["expected_inventory"], "unknown");
}

#[test]
fn attempt_yield_scopes_then_excludes_explicit_attempts() {
    let f = Fixture::new();
    let (snapshot, source, bundle) = f.setup(&rich());
    let mut req = request(&snapshot, &[bundle]);
    req["scope"] = json!({
        "lot_ids": ["LOT-A"], "wafer_ids": ["W1"],
        "sites": [{"head": 1, "site": 1}],
        "runs": [{"source_id": source, "run_id": "1"}]
    });
    req["exclude_attempts"] = json!([
        {"source_id": source, "attempt_id": "5"},
        {"source_id": source, "attempt_id": "2"}
    ]);
    let response = f.execute(&req).unwrap();
    // Attempt 2 is outside the site scope, so it is not an excluded scoped PRR.
    assert_eq!(response["counts"], counts(3, 2, 1, 1, 0, 1, 0));
    assert_eq!(response["yields"], yields(1, 2, 1));
    assert_eq!(response["sources"][0]["available_attempts"], "5");
}

#[test]
fn attempt_yield_prr_only_multirun_scope_keeps_head_and_site_separate() {
    let f = Fixture::new();
    let mut bytes = start();
    prr_only(&mut bytes, 1, 1, 0, 1);
    prr_only(&mut bytes, 2, 1, 8, 2);
    finish(&mut bytes, 200);
    bytes.extend(run("LOT-B", "W2", 300));
    prr_only(&mut bytes, 1, 1, 0x10, 3);
    prr_only(&mut bytes, 1, 2, 0, 4);
    finish(&mut bytes, 400);
    let (snapshot, source, bundle) = f.setup(&bytes);
    assert_eq!(bundle["measurements"], 0);
    let mut req = request(&snapshot, &[bundle]);
    assert_eq!(
        f.execute(&req).unwrap()["counts"],
        counts(4, 4, 0, 2, 1, 1, 0)
    );
    req["scope"] = json!({"sites": [{"head": 2, "site": 1}]});
    assert_eq!(
        f.execute(&req).unwrap()["counts"],
        counts(1, 1, 0, 0, 1, 0, 0)
    );
    req["scope"] = json!({
        "lot_ids": ["LOT-A", "LOT-B"], "wafer_ids": ["W2"],
        "sites": [{"head": 1, "site": 1}],
        "runs": [{"source_id": source, "run_id": "2"}]
    });
    let response = f.execute(&req).unwrap();
    assert_eq!(response["counts"], counts(1, 1, 0, 0, 0, 1, 0));
    assert_eq!(response["yields"], yields(0, 1, 0));
    req["scope"]["lot_ids"] = json!(["LOT-A"]);
    assert_eq!(
        f.execute(&req).unwrap()["counts"],
        counts(0, 0, 0, 0, 0, 0, 0)
    );
}

#[test]
fn attempt_yield_zero_denominators_and_empty_filter_lists_are_explicit() {
    let f = Fixture::new();
    let (snapshot, source, bundle) = f.setup(&rich());
    let mut req = request(&snapshot, &[bundle]);
    req["scope"] = json!({"lot_ids": [], "wafer_ids": [], "sites": [], "runs": []});
    assert_eq!(
        f.execute(&req).unwrap()["counts"],
        counts(5, 5, 0, 2, 1, 2, 1)
    );
    req["exclude_attempts"] =
        json!([1, 2, 5].map(|id| { json!({"source_id": source, "attempt_id": id.to_string()}) }));
    let unknown_only = f.execute(&req).unwrap();
    assert_eq!(unknown_only["counts"], counts(5, 2, 3, 0, 0, 2, 0));
    assert_eq!(unknown_only["yields"], yields(0, 2, 0));
    req["exclude_attempts"] =
        json!([1, 2, 3, 4, 5]
            .map(|id| { json!({"source_id": source, "attempt_id": id.to_string()}) }));
    let excluded = f.execute(&req).unwrap();
    assert_eq!(excluded["counts"], counts(5, 0, 5, 0, 0, 0, 0));
    assert_eq!(excluded["yields"], yields(0, 0, 0));
    req["scope"] = json!({"lot_ids": ["ABSENT"]});
    let empty = f.execute(&req).unwrap();
    assert_eq!(empty["counts"], counts(0, 0, 0, 0, 0, 0, 0));
    assert_eq!(empty["yields"], yields(0, 0, 0));
}

#[test]
fn attempt_yield_aliases_are_counted_by_source_and_request_order_is_canonical() {
    let f = Fixture::new();
    let (snapshot, bundles) = f.aliases();
    assert_eq!(bundles[0]["content_sha256"], bundles[1]["content_sha256"]);
    let mut req = request(&snapshot, &bundles);
    req["scope"] = json!({
        "lot_ids": ["LOT-A", "ABSENT"], "wafer_ids": ["W1", "ABSENT"],
        "sites": [{"head": 1, "site": 2}, {"head": 1, "site": 1}],
        "runs": bundles.iter().map(|b| json!({"source_id": b["source_id"], "run_id": "1"})).collect::<Vec<_>>()
    });
    let response = f.execute(&req).unwrap();
    assert_eq!(response["counts"], counts(10, 10, 0, 4, 2, 4, 2));
    assert_eq!(response["yields"], yields(4, 10, 6));
    assert_eq!(response["coverage"]["content_alias_source_count"], "1");
    assert_eq!(response["coverage"]["partial"], false);
    for (row, bundle) in response["sources"].as_array().unwrap().iter().zip(&bundles) {
        assert_eq!(row["source_id"], bundle["source_id"]);
        assert_eq!(row["counts"], counts(5, 5, 0, 2, 1, 2, 1));
    }
    req["evidence_ids"].as_array_mut().unwrap().reverse();
    for key in ["lot_ids", "wafer_ids", "sites", "runs"] {
        req["scope"][key].as_array_mut().unwrap().reverse();
    }
    let reordered = f.execute(&req).unwrap();
    // Yield math is request-order canonical; receipt fingerprints follow the raw request.
    assert_eq!(reordered["counts"], response["counts"]);
    assert_eq!(reordered["yields"], response["yields"]);
    assert_eq!(reordered["sources"], response["sources"]);
    assert_eq!(reordered["coverage"], response["coverage"]);
    assert_ne!(
        reordered["analysis"]["request_fingerprint"],
        response["analysis"]["request_fingerprint"]
    );
    req["exclude_attempts"] = json!([
        {"source_id": bundles[0]["source_id"], "attempt_id": "1"}
    ]);
    // A source-local exclusion must not remove the alias source's attempt 1.
    assert_eq!(
        f.execute(&req).unwrap()["counts"],
        counts(10, 9, 1, 3, 2, 4, 2)
    );
}

#[test]
fn attempt_yield_missing_sources_and_incomplete_catalog_require_partial_opt_in() {
    let f = Fixture::new();
    let (snapshot, bundles) = f.aliases();
    let mut req = request(&snapshot, &bundles[..1]);
    assert!(f.execute(&req).is_err());
    req["allow_partial"] = json!(true);
    let response = f.execute(&req).unwrap();
    assert_eq!(response["counts"], counts(5, 5, 0, 2, 1, 2, 1));
    assert_eq!(response["coverage"]["snapshot_sources"], "2");
    assert_eq!(response["coverage"]["selected_sources"], "1");
    assert_eq!(
        response["coverage"]["missing_source_ids"],
        json!([bundles[1]["source_id"]])
    );
    assert_eq!(response["coverage"]["partial"], true);

    let partial = Fixture::new();
    fs::write(partial.source(), rich()).unwrap();
    let broken = partial.0.join("broken.stdf");
    fs::write(&broken, [0xff]).unwrap();
    convert_dataset(
        &[partial.source(), broken],
        &partial.dataset(),
        &[],
        &Default::default(),
        ErrorPolicy::Continue,
    )
    .unwrap();
    let snap = create_snapshot(&partial.dataset(), true).unwrap();
    let source = snap.catalog.sources.keys().next().unwrap();
    let bundle = partial.retain(&snap.snapshot_id, source, &partial.source());
    let mut req = request(&snap.snapshot_id, &[bundle]);
    assert!(partial.execute(&req).is_err());
    req["allow_partial"] = json!(true);
    let response = partial.execute(&req).unwrap();
    assert_eq!(response["coverage"]["missing_source_ids"], json!([]));
    assert_eq!(response["coverage"]["catalog_run_status"], "partial");
    assert_eq!(response["coverage"]["catalog_failure_count"], "1");
    assert_eq!(response["coverage"]["partial"], true);
    assert_eq!(response["coverage"]["expected_inventory"], "unknown");
}

#[test]
fn attempt_yield_rejects_unknown_fields_policies_and_noncanonical_ids() {
    let f = Fixture::new();
    let (snapshot, source, bundle) = f.setup(&rich());
    let valid = request(&snapshot, &[bundle]);
    for (field, value) in [
        ("version", json!("attempt-yield-request-v2")),
        ("operation", json!("latest-yield")),
        ("population", json!("latest")),
        ("identity_policy", json!("latest-ecid-start-t-v1")),
        ("source_policy", json!("deduplicate-content")),
        ("measurement_exclusions", json!([])),
        ("presentation_filters", json!({})),
        ("allow_partial", json!("false")),
        ("snapshot_id", json!("../outside")),
        ("evidence_ids", json!(["A".repeat(64)])),
    ] {
        let mut req = valid.clone();
        req[field] = value;
        assert!(f.execute(&req).is_err(), "accepted invalid {field}");
    }
    for field in [
        "version",
        "snapshot_id",
        "evidence_ids",
        "population",
        "source_policy",
        "allow_partial",
    ] {
        let mut req = valid.clone();
        req.as_object_mut().unwrap().remove(field);
        assert!(f.execute(&req).is_err(), "accepted missing {field}");
    }
    for id in [
        json!(0),
        json!(1),
        json!("0"),
        json!("01"),
        json!("+1"),
        json!("-1"),
        json!("1.0"),
        json!("18446744073709551616"),
    ] {
        let mut req = valid.clone();
        req["exclude_attempts"] = json!([{"source_id": source, "attempt_id": id}]);
        assert!(f.execute(&req).is_err(), "accepted invalid attempt ID {id}");
        req.as_object_mut().unwrap().remove("exclude_attempts");
        req["scope"] = json!({"runs": [{"source_id": source, "run_id": id}]});
        assert!(f.execute(&req).is_err(), "accepted invalid run ID {id}");
    }
    for scope in [
        json!({"test_ids": [10]}),
        json!({"sites": [{"head": 1, "site": 1, "extra": true}]}),
        json!({"runs": [{"source_id": source, "run_id": "1", "extra": true}]}),
    ] {
        let mut req = valid.clone();
        req["scope"] = scope;
        assert!(f.execute(&req).is_err());
    }
}

#[test]
fn attempt_yield_rejects_duplicate_sources_wrong_snapshot_and_invalid_references() {
    let f = Fixture::new();
    let (snapshot, source, bundle) = f.setup(&rich());
    assert!(f
        .execute(&request(&snapshot, &[bundle.clone(), bundle.clone()]))
        .is_err());
    // A second valid bundle can share a catalog source but have a different locator.
    let relocated = f.0.join("relocated.stdf");
    fs::write(&relocated, rich()).unwrap();
    let second = f.retain(&snapshot, &source, &relocated);
    assert_ne!(second["evidence_id"], bundle["evidence_id"]);
    assert!(f
        .execute(&request(&snapshot, &[bundle.clone(), second]))
        .is_err());
    let valid = request(&snapshot, &[bundle.clone()]);
    for (key, value) in [
        (
            "exclude_attempts",
            json!([{"source_id": source, "attempt_id": "6"}]),
        ),
        (
            "exclude_attempts",
            json!([{"source_id": "f".repeat(64), "attempt_id": "1"}]),
        ),
        (
            "scope",
            json!({"runs": [{"source_id": source, "run_id": "2"}]}),
        ),
        (
            "scope",
            json!({"runs": [{"source_id": "f".repeat(64), "run_id": "1"}]}),
        ),
    ] {
        let mut req = valid.clone();
        req[key] = value;
        assert!(f.execute(&req).is_err(), "accepted invalid reference {req}");
    }
    f.convert(&[relocated]);
    let newer = create_snapshot(&f.dataset(), false).unwrap();
    assert_ne!(newer.snapshot_id, snapshot);
    let mut req = request(&newer.snapshot_id, &[bundle]);
    req["allow_partial"] = json!(true);
    assert!(f.execute(&req).is_err());
}

#[test]
fn attempt_yield_old_snapshot_survives_raw_catalog_and_current_pointer_removal() {
    let f = Fixture::new();
    let (snapshot, _, bundle) = f.setup(&rich());
    let req = request(&snapshot, &[bundle]);
    let expected = f.execute(&req).unwrap();
    let newer_input = f.0.join("newer.stdf");
    fs::write(&newer_input, rich()).unwrap();
    f.convert(&[newer_input.clone()]);
    let newer = create_snapshot(&f.dataset(), false).unwrap();
    select_snapshot(&f.dataset(), &newer.snapshot_id, false).unwrap();
    fs::remove_file(f.source()).unwrap();
    fs::remove_file(newer_input).unwrap();
    fs::remove_file(f.dataset().join("_catalog.json")).unwrap();
    fs::remove_file(f.dataset().join("_current_snapshot.json")).unwrap();
    assert_eq!(f.execute(&req).unwrap(), expected);
    assert!(!f.dataset().join("_catalog.json").exists());
    assert!(!f.dataset().join("_current_snapshot.json").exists());
}

#[test]
fn attempt_yield_corrupt_selected_evidence_fails_even_when_scope_is_empty() {
    let f = Fixture::new();
    let (snapshot, bundles) = f.aliases();
    let mut req = request(&snapshot, &bundles);
    req["scope"] = json!({"lot_ids": ["ABSENT"]});
    let corrupt = f
        .dataset()
        .join("evidence")
        .join(bundles[1]["evidence_id"].as_str().unwrap())
        .join("attempts.idx");
    fs::write(corrupt, b"corrupt index").unwrap();
    assert!(f.execute(&req).is_err());
    req["allow_partial"] = json!(true);
    assert!(f.execute(&req).is_err());
}

#[test]
fn attempt_yield_receipt_is_stable_for_identical_request_and_survives_replay() {
    let f = Fixture::new();
    let (snapshot, _, bundle) = f.setup(&rich());
    let req = request(&snapshot, &[bundle]);
    let first = f.execute(&req).unwrap();
    let second = f.execute(&req).unwrap();
    assert_eq!(first["analysis"], second["analysis"]);
    assert_eq!(first["counts"], second["counts"]);
    let receipt: Value = serde_json::from_slice(
        &fs::read(
            f.dataset()
                .join(first["analysis"]["relative_path"].as_str().unwrap())
                .join("receipt.json"),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["version"], "analysis-receipt-v1");
    assert_eq!(receipt["status"], "complete");
    assert_eq!(receipt["result_hash"], first["analysis"]["result_hash"]);
    assert_eq!(receipt["execution_id"], first["analysis"]["execution_id"]);
}

#[test]
fn latest_device_yield_counts_only_start_t_selected_attempts() {
    let f = Fixture::new();
    // Same ECID retested later: only the latest attempt is in latest_device.
    let mut bytes = start();
    prr_only(&mut bytes, 1, 1, 8, 1); // fail first
    finish(&mut bytes, 200);
    bytes.extend(run("LOT-A", "W1", 300));
    prr_only(&mut bytes, 1, 1, 0, 1); // pass later, same XY
    finish(&mut bytes, 400);
    let (snapshot, _, bundle) = f.setup(&bytes);
    let all = f.execute(&request(&snapshot, &[bundle.clone()])).unwrap();
    assert_eq!(all["counts"], counts(2, 2, 0, 1, 1, 0, 0));

    let mut req = request(&snapshot, &[bundle]);
    req["population"] = json!("latest_device");
    req["identity_policy"] = json!("latest-ecid-start-t-v1");
    let latest = f.execute(&req).unwrap();
    assert_eq!(latest["operation"], "prr-latest-device-yield-v1");
    assert_eq!(latest["population"], "latest_device");
    assert_eq!(latest["counts"], counts(1, 1, 0, 1, 0, 0, 0));
    assert_eq!(latest["yields"], yields(1, 1, 1));
}

#[test]
fn analysis_receipt_staging_failure_leaves_no_complete_bundle() {
    let f = Fixture::new();
    let (snapshot, _, bundle) = f.setup(&rich());
    let req = request(&snapshot, &[bundle]);
    let first = f.execute(&req).unwrap();
    let relative = first["analysis"]["relative_path"]
        .as_str()
        .unwrap()
        .to_string();
    let complete = f.dataset().join(&relative);
    assert!(complete.join("receipt.json").is_file());
    // Simulate a crash leaving only staging: no complete pointer under analysis/<id>.
    let stage = f.dataset().join("analysis").join(".staging-orphan-test");
    fs::create_dir_all(&stage).unwrap();
    fs::write(stage.join("result.json"), b"{}").unwrap();
    // Orphan staging must not be treated as a published complete receipt.
    assert!(!stage.join("receipt.json").is_file());
    assert!(complete.join("receipt.json").is_file());
    let replay = f.execute(&req).unwrap();
    assert_eq!(
        replay["analysis"]["execution_id"],
        first["analysis"]["execution_id"]
    );
    assert!(complete.join("receipt.json").is_file());
    assert!(stage.exists());
}

#[test]
fn attempt_yield_cancel_quota_and_malformed_request_preserve_dataset_state() {
    let f = Fixture::new();
    let (snapshot, _, bundle) = f.setup(&rich());
    select_snapshot(&f.dataset(), &snapshot, false).unwrap();
    let catalog = fs::read(f.dataset().join("_catalog.json")).unwrap();
    let current = fs::read(f.dataset().join("_current_snapshot.json")).unwrap();
    let before = evidence::list(&f.dataset()).unwrap();
    let req = request(&snapshot, &[bundle]);
    let cancel = f.0.join("cancel");
    fs::write(&cancel, b"cancel").unwrap();
    for limits in [
        EvidenceLimits {
            cancel_file: Some(cancel),
            ..Default::default()
        },
        EvidenceLimits {
            memory_limit_mib: 0,
            ..Default::default()
        },
        EvidenceLimits {
            disk_limit_mib: 0,
            ..Default::default()
        },
    ] {
        assert!(f.execute_with(&req, &limits).is_err());
    }
    let path = f.0.join("malformed.json");
    fs::write(&path, b"{\"version\":").unwrap();
    assert!(analysis::execute_with_telemetry(
        &f.dataset(),
        &path,
        &EvidenceLimits::default(),
        None
    )
    .is_err());
    fs::write(&path, vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert!(analysis::execute_with_telemetry(
        &f.dataset(),
        &path,
        &EvidenceLimits::default(),
        None
    )
    .is_err());
    let mut too_many = req;
    too_many["evidence_ids"] = json!(vec!["0".repeat(64); 257]);
    assert!(f.execute(&too_many).is_err());
    assert_eq!(
        fs::read(f.dataset().join("_catalog.json")).unwrap(),
        catalog
    );
    assert_eq!(
        fs::read(f.dataset().join("_current_snapshot.json")).unwrap(),
        current
    );
    assert_eq!(evidence::list(&f.dataset()).unwrap(), before);
}
