// Copyright 2026 zstdf contributors
// SPDX-License-Identifier: Apache-2.0

//! Evidence for the SHA-256 pinned public samples.
//!
//! Missing downloads skip the test unless `ZSTDF_REQUIRE_PUBLIC=1`, which CI sets
//! after `examples/public-stdf/download.py`. Downloaded bytes are not committed.

use super::{ingest, Arguments};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use stdf_core::{summarize, ByteOrder, RecordType};
use stdf_io::StdfReader;

fn repo_public_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/public-stdf")
}

fn sha256_file(path: &Path) -> String {
    let bytes = fs::read(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn sources_by_file(root: &Path) -> BTreeMap<String, serde_json::Value> {
    let text = fs::read_to_string(root.join("sources.json")).expect("sources.json");
    let entries: Vec<serde_json::Value> = serde_json::from_str(&text).expect("sources.json parses");
    entries
        .into_iter()
        .map(|entry| {
            let file = entry["file"].as_str().expect("source file").to_string();
            (file, entry)
        })
        .collect()
}

fn require_download(path: &Path) -> bool {
    if path.is_file() {
        return true;
    }
    if std::env::var("ZSTDF_REQUIRE_PUBLIC").ok().as_deref() == Some("1") {
        panic!(
            "missing pinned public STDF file {} (CI must run examples/public-stdf/download.py)",
            path.display()
        );
    }
    eprintln!(
        "skipping public STDF evidence; download with examples/public-stdf/download.py ({})",
        path.display()
    );
    false
}

fn record_count(summary: &stdf_core::DecodeSummary, kind: RecordType) -> u64 {
    summary.record_counts.get(&kind).copied().unwrap_or(0) as u64
}

#[test]
fn pinned_public_files_match_expected_counts() {
    let root = repo_public_dir();
    let expected: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(root.join("expected.json")).expect("expected.json"),
    )
    .expect("expected.json parses");
    assert_eq!(expected["schema_version"], 1);
    let sources = sources_by_file(&root);
    let samples = expected["samples"].as_array().expect("samples array");

    for sample in samples {
        let file = sample["file"].as_str().expect("sample file");
        let path = root.join("downloads").join(file);
        if !require_download(&path) {
            return;
        }
        let source = sources
            .get(file)
            .unwrap_or_else(|| panic!("{file} missing from sources.json"));
        let actual_sha = sha256_file(&path);
        assert_eq!(
            actual_sha,
            source["sha256"].as_str().unwrap(),
            "{file} sha256"
        );
        assert_eq!(
            fs::metadata(&path).unwrap().len(),
            source["bytes"].as_u64().unwrap(),
            "{file} size"
        );

        let bytes = fs::read(&path).unwrap();
        let summary = summarize(&bytes);
        assert_eq!(
            summary.byte_order,
            Some(ByteOrder::BigEndian),
            "{file} byte order"
        );
        assert!(!summary.truncated, "{file} truncated");
        assert_eq!(summary.bytes_consumed, bytes.len(), "{file} consumed bytes");
        assert!(
            summary.errors.is_empty(),
            "{file} errors: {:?}",
            summary.errors
        );
        for (name, kind) in [
            ("PRR", RecordType::Prr),
            ("PTR", RecordType::Ptr),
            ("FTR", RecordType::Ftr),
            ("MPR", RecordType::Mpr),
        ] {
            assert_eq!(
                record_count(&summary, kind),
                sample["record_counts"][name].as_u64().unwrap(),
                "{file} {name}"
            );
        }

        let cache_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../target/public-stdf-evidence")
            .join(file.replace('/', "_"));
        let _ = fs::remove_dir_all(&cache_dir);
        let cache = ingest::prepare(&Arguments {
            input: path,
            dataset: None,
            cache_dir: Some(cache_dir),
            flow: None,
            export_html: None,
            export_scope: "summary".into(),
            export_size_mib: 20,
            memory_limit_mib: 1024,
            disk_limit_mib: 10240,
            cancel_file: None,
            port: 0,
            no_open: true,
            telemetry_json: None,
        })
        .unwrap_or_else(|err| panic!("viewer prepare {file}: {err}"));
        assert_eq!(
            cache.manifest.attempts,
            sample["record_counts"]["PRR"].as_u64().unwrap(),
            "{file} viewer attempts"
        );
        for (name, kind) in [("PRR", "PRR"), ("PTR", "PTR"), ("FTR", "FTR")] {
            assert_eq!(
                cache.manifest.inventory.get(kind).copied().unwrap_or(0),
                sample["record_counts"][name].as_u64().unwrap(),
                "{file} viewer inventory {name}"
            );
        }
        assert_eq!(
            cache
                .manifest
                .population_counts
                .get("latest")
                .copied()
                .unwrap_or(0),
            sample["latest_attempts"].as_u64().unwrap(),
            "{file} latest population {:?}",
            cache.manifest.population_counts
        );
        assert_eq!(
            cache
                .manifest
                .population_counts
                .get("unresolved_latest")
                .copied()
                .unwrap_or(0),
            sample["unresolved_latest_attempts"].as_u64().unwrap(),
            "{file} unresolved latest {:?}",
            cache.manifest.population_counts
        );
    }

    for pair in expected["byte_identical_pairs"].as_array().expect("pairs") {
        let compressed = pair["compressed"].as_str().unwrap();
        let plain = pair["plain"].as_str().unwrap();
        let compressed_path = root.join("downloads").join(compressed);
        let plain_path = root.join("downloads").join(plain);
        if !require_download(&compressed_path) || !require_download(&plain_path) {
            return;
        }
        let source = sources
            .get(compressed)
            .unwrap_or_else(|| panic!("{compressed} missing from sources.json"));
        assert_eq!(
            sha256_file(&compressed_path),
            source["sha256"].as_str().unwrap()
        );
        let plain_bytes = fs::read(&plain_path).unwrap();
        let decoded = StdfReader::open(&compressed_path)
            .unwrap_or_else(|err| panic!("open {compressed}: {err}"));
        assert_eq!(
            decoded.data(),
            plain_bytes.as_slice(),
            "{compressed} decompressed bytes differ from {plain}"
        );
    }
}
