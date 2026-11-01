// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
use super::evidence::{self, EvidenceLimits};
use super::*;
use stdf_parquet::catalog::{convert_dataset, load_catalog, sha256_file, ErrorPolicy};
use stdf_parquet::snapshot::{create_snapshot, select_snapshot};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = crate::tests::temp_path("retained_evidence");
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
    fn setup(&self, bytes: &[u8]) -> (String, String) {
        fs::write(self.source(), bytes).unwrap();
        self.convert(&[self.source()]);
        let snapshot = create_snapshot(&self.dataset(), false).unwrap();
        let source_id = snapshot.catalog.sources.keys().next().unwrap().clone();
        (snapshot.snapshot_id, source_id)
    }
    fn retain(&self, snapshot: &str, source: &str) -> Value {
        serde_json::to_value(
            evidence::retain(
                &self.dataset(),
                snapshot,
                source,
                &self.source(),
                &EvidenceLimits::default(),
            )
            .unwrap(),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
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
fn start() -> Vec<u8> {
    let mut out = rec(0, 10, vec![2, 4]);
    let mut mir = vec![0; 15];
    mir[4..8].copy_from_slice(&100u32.to_le_bytes());
    for s in ["LOT", "DEVICE", "NODE", "TESTER", "JOB"] {
        mir.extend(cn(s));
    }
    out.extend(rec(1, 10, mir));
    let mut wir = vec![1, 1];
    wir.extend(100u32.to_le_bytes());
    wir.extend(cn("W1"));
    out.extend(rec(2, 10, wir));
    out
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
fn prr(site: u8, x: i16) -> Vec<u8> {
    let mut body = vec![1, site, 0];
    for v in [3u16, 1, 10] {
        body.extend(v.to_le_bytes());
    }
    body.extend(x.to_le_bytes());
    body.extend(1i16.to_le_bytes());
    body.extend(10u32.to_le_bytes());
    body.extend(cn("REPEATED_PART"));
    rec(5, 20, body)
}
fn rich() -> Vec<u8> {
    let mut out = start();
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
    out.extend(prr(2, 2));
    out.extend(prr(1, 1));
    out.extend(rec(5, 10, vec![1, 1]));
    out.extend(prr(1, 3));
    out.extend(rec(1, 20, 1000u32.to_le_bytes().to_vec()));
    out
}

#[test]
fn retained_evidence_preserves_interleaved_executions_and_prr_only_after_source_removal() {
    let f = Fixture::new();
    let (snapshot, source) = f.setup(&rich());
    let bundle = f.retain(&snapshot, &source);
    assert_eq!(bundle["coverage_scope"], "one-catalog-source");
    assert_eq!(bundle["raw_retention"], "decompressed-content-retained");
    assert_eq!(bundle["original_container_retention"], "external-required");
    assert_eq!(bundle["records"], 16);
    assert_eq!(bundle["attempts"], 3);
    assert_eq!(bundle["executions"], 6);
    assert_eq!(bundle["measurements"], 8);
    assert_eq!(bundle["inventory"]["PRR"], 3);
    assert_eq!(bundle["inventory"]["MPR"], 1);
    assert_eq!(bundle["inventory"]["FTR"], 2);
    let id = bundle["evidence_id"].as_str().unwrap();
    assert_eq!(f.retain(&snapshot, &source), bundle);
    fs::remove_file(f.source()).unwrap();
    fs::remove_file(f.dataset().join("_catalog.json")).unwrap();
    let limits = EvidenceLimits::default();
    assert_eq!(
        serde_json::to_value(evidence::verify(&f.dataset(), id, &limits).unwrap()).unwrap(),
        bundle
    );
    let mpr = evidence::record(&f.dataset(), id, 9, &limits).unwrap();
    assert_eq!(mpr["kind"], "MPR");
    assert_eq!(mpr["attempt"], 1);
    let ftr = evidence::record(&f.dataset(), id, 10, &limits).unwrap();
    assert_eq!(ftr["kind"], "FTR");
    assert_eq!(ftr["attempt"], 2);
    for (attempt, site, x) in [(1, 1, 1), (2, 2, 2), (3, 1, 3)] {
        let row = evidence::attempt(&f.dataset(), id, attempt, &limits).unwrap();
        assert_eq!(row["site"], site);
        assert_eq!(row["x"], x);
        assert_eq!(row["part"], "REPEATED_PART");
    }
    for invalid in [0, 17, u64::MAX] {
        assert!(evidence::record(&f.dataset(), id, invalid, &limits).is_err());
    }
    for invalid in [0, 4, u64::MAX] {
        assert!(evidence::attempt(&f.dataset(), id, invalid, &limits).is_err());
    }
}

#[test]
fn retained_evidence_gzip_preserves_container_and_content_hash_distinction() {
    let f = Fixture::new();
    let bytes = rich();
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&bytes).unwrap();
    let (snapshot, source) = f.setup(&gzip.finish().unwrap());
    let bundle = f.retain(&snapshot, &source);
    assert_eq!(bundle["source_sha256"], sha256_file(&f.source()).unwrap());
    assert_ne!(bundle["source_sha256"], bundle["content_sha256"]);
    let id = bundle["evidence_id"].as_str().unwrap();
    assert_eq!(
        fs::read(f.dataset().join("evidence").join(id).join("source.stdf")).unwrap(),
        bytes
    );
    fs::remove_file(f.source()).unwrap();
    assert!(evidence::verify(&f.dataset(), id, &EvidenceLimits::default()).is_ok());
}

#[test]
fn retained_evidence_explicit_alias_source_ids_do_not_collapse() {
    let f = Fixture::new();
    let bytes = rich();
    fs::write(f.source(), &bytes).unwrap();
    let alias = f.0.join("copy.stdf");
    fs::write(&alias, &bytes).unwrap();
    f.convert(&[f.source(), alias]);
    let snapshot = create_snapshot(&f.dataset(), false).unwrap();
    let ids: Vec<_> = snapshot.catalog.sources.keys().cloned().collect();
    let a = f.retain(&snapshot.snapshot_id, &ids[0]);
    let b = f.retain(&snapshot.snapshot_id, &ids[1]);
    assert_eq!(a["source_id"], ids[0]);
    assert_eq!(b["source_id"], ids[1]);
    assert_ne!(a["evidence_id"], b["evidence_id"]);
    assert_eq!(a["snapshot_source_count"], 2);
    assert!(evidence::retain(
        &f.dataset(),
        &snapshot.snapshot_id,
        &"f".repeat(64),
        &f.source(),
        &EvidenceLimits::default()
    )
    .is_err());
}

#[test]
fn retained_evidence_wrong_bytes_cancel_and_quota_preserve_existing_selection() {
    let f = Fixture::new();
    let bytes = rich();
    let (snapshot, source) = f.setup(&bytes);
    select_snapshot(&f.dataset(), &snapshot, false).unwrap();
    let pointer = fs::read(f.dataset().join("_current_snapshot.json")).unwrap();
    let catalog = fs::read(f.dataset().join("_catalog.json")).unwrap();
    fs::write(f.source(), b"not the selected source").unwrap();
    assert!(evidence::retain(
        &f.dataset(),
        &snapshot,
        &source,
        &f.source(),
        &EvidenceLimits::default()
    )
    .is_err());
    fs::write(f.source(), bytes).unwrap();
    let cancel = f.0.join("cancel");
    fs::write(&cancel, b"cancel").unwrap();
    for limits in [
        EvidenceLimits {
            cancel_file: Some(cancel),
            ..Default::default()
        },
        EvidenceLimits {
            disk_limit_mib: 0,
            ..Default::default()
        },
        EvidenceLimits {
            memory_limit_mib: 0,
            ..Default::default()
        },
    ] {
        assert!(evidence::retain(&f.dataset(), &snapshot, &source, &f.source(), &limits).is_err());
    }
    assert_eq!(
        fs::read(f.dataset().join("_current_snapshot.json")).unwrap(),
        pointer
    );
    assert_eq!(
        fs::read(f.dataset().join("_catalog.json")).unwrap(),
        catalog
    );
    let listed = evidence::list(&f.dataset()).unwrap();
    assert!(listed.is_empty());
}

#[test]
fn retained_evidence_detects_corruption_and_never_overwrites_existing_bundle() {
    let f = Fixture::new();
    let (snapshot, source) = f.setup(&rich());
    let bundle = f.retain(&snapshot, &source);
    let id = bundle["evidence_id"].as_str().unwrap();
    let root = f.dataset().join("evidence").join(id);
    let before = fs::read(root.join("bundle.json")).unwrap();
    fs::write(root.join("records.idx"), b"corrupt index").unwrap();
    assert!(evidence::verify(&f.dataset(), id, &EvidenceLimits::default()).is_err());
    assert!(evidence::retain(
        &f.dataset(),
        &snapshot,
        &source,
        &f.source(),
        &EvidenceLimits::default()
    )
    .is_err());
    assert_eq!(fs::read(root.join("bundle.json")).unwrap(), before);
    assert_eq!(
        fs::read(root.join("records.idx")).unwrap(),
        b"corrupt index"
    );
}

#[test]
fn retained_evidence_rejects_equal_count_but_different_eav_values() {
    use arrow::array::{Array, Float32Array};
    let f = Fixture::new();
    let (_, source_id) = f.setup(&rich());
    let mut catalog = load_catalog(&f.dataset()).unwrap();
    let source = catalog.sources.get_mut(&source_id).unwrap();
    let fragment = &mut source.fragments[0];
    let path = f.dataset().join(&fragment.path);
    let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
        File::open(&path).unwrap(),
    )
    .unwrap()
    .build()
    .unwrap();
    let mut batches = Vec::new();
    for batch in reader {
        let batch = batch.unwrap();
        let mut columns = batch.columns().to_vec();
        let values = columns[stdf_arrow::schema::RESULT]
            .as_any()
            .downcast_ref::<Float32Array>()
            .unwrap();
        let changed: Vec<Option<f32>> = (0..values.len())
            .map(|i| (!values.is_null(i)).then(|| values.value(i) + 100.))
            .collect();
        columns[stdf_arrow::schema::RESULT] = Arc::new(Float32Array::from(changed));
        batches.push(arrow::record_batch::RecordBatch::try_new(batch.schema(), columns).unwrap());
    }
    stdf_parquet::write_record_batches_to_path(&path, &batches).unwrap();
    fragment.sha256 = sha256_file(&path).unwrap();
    fs::write(
        f.dataset().join("_catalog.json"),
        serde_json::to_vec(&catalog).unwrap(),
    )
    .unwrap();
    let snapshot = create_snapshot(&f.dataset(), false).unwrap();
    assert!(evidence::retain(
        &f.dataset(),
        &snapshot.snapshot_id,
        &source_id,
        &f.source(),
        &EvidenceLimits::default()
    )
    .is_err());
}

#[test]
fn retained_evidence_prr_only_has_zero_executions_without_losing_attempt() {
    let f = Fixture::new();
    let mut bytes = start();
    bytes.extend(rec(5, 10, vec![1, 1]));
    bytes.extend(prr(1, 1));
    bytes.extend(rec(1, 20, 200u32.to_le_bytes().to_vec()));
    let (snapshot, source) = f.setup(&bytes);
    let bundle = f.retain(&snapshot, &source);
    assert_eq!(bundle["attempts"], 1);
    assert_eq!(bundle["executions"], 0);
    assert_eq!(bundle["measurements"], 0);
    let id = bundle["evidence_id"].as_str().unwrap();
    assert!(evidence::attempt(&f.dataset(), id, 1, &EvidenceLimits::default()).is_ok());
}

#[test]
fn retained_evidence_uses_explicit_old_snapshot_with_no_live_catalog() {
    let f = Fixture::new();
    let (old, source) = f.setup(&rich());
    let second_input = f.0.join("second.stdf");
    fs::write(&second_input, rich()).unwrap();
    f.convert(&[second_input]);
    let newer = create_snapshot(&f.dataset(), false).unwrap();
    select_snapshot(&f.dataset(), &newer.snapshot_id, false).unwrap();
    let pointer = fs::read(f.dataset().join("_current_snapshot.json")).unwrap();
    fs::remove_file(f.dataset().join("_catalog.json")).unwrap();
    let bundle = f.retain(&old, &source);
    assert_eq!(bundle["snapshot_id"], old);
    assert_eq!(bundle["snapshot_source_count"], 1);
    assert_eq!(
        fs::read(f.dataset().join("_current_snapshot.json")).unwrap(),
        pointer
    );
    assert!(!f.dataset().join("_catalog.json").exists());
}

#[test]
fn retained_evidence_relocation_and_missing_payload_are_explicit() {
    let f = Fixture::new();
    let (snapshot, source) = f.setup(&rich());
    let bundle = f.retain(&snapshot, &source);
    let id = bundle["evidence_id"].as_str().unwrap();
    let moved = f.0.join("moved-dataset");
    fs::rename(f.dataset(), &moved).unwrap();
    fs::remove_file(f.source()).unwrap();
    assert!(evidence::verify(&moved, id, &EvidenceLimits::default()).is_ok());
    fs::remove_file(moved.join("evidence").join(id).join("source.stdf")).unwrap();
    assert!(evidence::verify(&moved, id, &EvidenceLimits::default()).is_err());
    for invalid in ["../outside", "..\\outside", &"A".repeat(64)] {
        assert!(evidence::verify(&moved, invalid, &EvidenceLimits::default()).is_err());
    }
}

#[test]
fn retained_evidence_prepublication_failure_cleans_only_owned_stage() {
    let f = Fixture::new();
    let (snapshot, source) = f.setup(&rich());
    let old = f.retain(&snapshot, &source);
    let neighbor = f.dataset().join("evidence/.building-evidence-neighbor");
    fs::create_dir(&neighbor).unwrap();
    fs::write(neighbor.join("keep.txt"), b"other task").unwrap();
    let alias = f.0.join("alias.stdf");
    fs::write(&alias, rich()).unwrap();
    f.convert(&[alias]);
    let new = create_snapshot(&f.dataset(), false).unwrap();
    let before = fs::read(f.dataset().join("_catalog.json")).unwrap();
    evidence::fail_next_publication();
    let error = evidence::retain(
        &f.dataset(),
        &new.snapshot_id,
        &source,
        &f.source(),
        &EvidenceLimits::default(),
    )
    .unwrap_err();
    assert!(error.to_string().contains("injected failure"));
    assert_eq!(fs::read(f.dataset().join("_catalog.json")).unwrap(), before);
    assert_eq!(
        evidence::list(&f.dataset()).unwrap(),
        vec![old["evidence_id"].as_str().unwrap().to_owned()]
    );
    assert_eq!(fs::read(neighbor.join("keep.txt")).unwrap(), b"other task");
    assert_eq!(
        fs::read_dir(f.dataset().join("evidence")).unwrap().count(),
        2
    );
    assert!(evidence::verify(
        &f.dataset(),
        old["evidence_id"].as_str().unwrap(),
        &EvidenceLimits::default()
    )
    .is_ok());
}

#[test]
fn retained_evidence_budget_counts_existing_payload_and_preserves_it() {
    let f = Fixture::new();
    let (snapshot, source) = f.setup(&rich());
    let old = f.retain(&snapshot, &source);
    // A preexisting neighboring allocation leaves too little room for the
    // manifest reserve and reindexing, even though this source itself is tiny.
    let neighbor = f.dataset().join("evidence/.building-evidence-neighbor");
    fs::create_dir(&neighbor).unwrap();
    let file = File::create(neighbor.join("reserved.bin")).unwrap();
    file.set_len(63 * 1024 * 1024).unwrap();
    drop(file);
    let before = evidence::list(&f.dataset()).unwrap();
    let limits = EvidenceLimits {
        memory_limit_mib: 64,
        disk_limit_mib: 64,
        cancel_file: None,
    };
    assert!(evidence::verify(&f.dataset(), old["evidence_id"].as_str().unwrap(), &limits).is_err());
    assert!(evidence::retain(&f.dataset(), &snapshot, &source, &f.source(), &limits).is_err());
    assert_eq!(evidence::list(&f.dataset()).unwrap(), before);
    assert_eq!(
        fs::metadata(neighbor.join("reserved.bin")).unwrap().len(),
        63 * 1024 * 1024
    );
    assert_eq!(
        fs::read_dir(f.dataset().join("evidence")).unwrap().count(),
        2
    );
}

#[test]
fn retained_evidence_regeneration_rejects_corruption_even_with_updated_hashes() {
    use sha2::{Digest, Sha256};
    let f = Fixture::new();
    let (snapshot, source) = f.setup(&rich());
    let mut bundle = f.retain(&snapshot, &source);
    let old = f
        .dataset()
        .join("evidence")
        .join(bundle["evidence_id"].as_str().unwrap());
    fs::write(old.join("records.idx"), b"self-consistent but wrong index").unwrap();
    let mut manifest: Value =
        serde_json::from_slice(&fs::read(old.join("manifest.json")).unwrap()).unwrap();
    manifest["files"]["records.idx"] = sha256_file(&old.join("records.idx")).unwrap().into();
    fs::write(
        old.join("manifest.json"),
        serde_json::to_vec(&manifest).unwrap(),
    )
    .unwrap();
    for name in ["records.idx", "manifest.json"] {
        bundle["files"][name] = json!({"sha256":sha256_file(&old.join(name)).unwrap(),"bytes":fs::metadata(old.join(name)).unwrap().len()});
    }
    let mut payload = bundle.clone();
    payload.as_object_mut().unwrap().remove("evidence_id");
    let mut digest = Sha256::new();
    digest.update(b"zstdf:viewer-evidence-v1\0");
    digest.update(serde_json::to_vec(&payload).unwrap());
    let id = format!("{:x}", digest.finalize());
    bundle["evidence_id"] = id.clone().into();
    fs::write(
        old.join("bundle.json"),
        serde_json::to_vec(&bundle).unwrap(),
    )
    .unwrap();
    let changed = f.dataset().join("evidence").join(&id);
    fs::rename(old, changed).unwrap();
    let error = evidence::verify(&f.dataset(), &id, &EvidenceLimits::default()).unwrap_err();
    assert!(
        error.to_string().contains("source reconstruction"),
        "{error}"
    );
}
