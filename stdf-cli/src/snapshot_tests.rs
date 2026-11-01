// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
use super::*;
use serde_json::Value;

fn invoke_snapshot(args: &[&str]) -> CliResult<Value> {
    let mut argv = vec!["zstdf-cli", "snapshot"];
    argv.extend_from_slice(args);
    let mut output = Vec::new();
    execute(Cli::try_parse_from(argv)?, &mut output)?;
    Ok(serde_json::from_slice(&output)?)
}

fn convert_snapshot_fixture(input: &Path, root: &Path) {
    execute(
        Cli::try_parse_from([
            "zstdf-cli",
            "convert",
            input.to_str().unwrap(),
            "--layout",
            "catalog",
            "--output-dir",
            root.to_str().unwrap(),
        ])
        .unwrap(),
        &mut Vec::new(),
    )
    .unwrap();
}

#[test]
fn snapshot_cli_roundtrip_is_idempotent_and_independent_of_raw_and_current_catalog() {
    let dir = tests::temp_path("snapshot_cli_roundtrip");
    fs::create_dir(&dir).unwrap();
    let input = dir.join("input.stdf");
    let root = dir.join("dataset");
    fs::write(&input, tests::build_test_stdf()).unwrap();
    convert_snapshot_fixture(&input, &root);
    let root_text = root.to_str().unwrap();
    let catalog_before = fs::read(root.join("_catalog.json")).unwrap();
    let first = invoke_snapshot(&["create", root_text]).unwrap();
    assert_eq!(first["version"], "catalog-snapshot-v1");
    assert_eq!(first["expected_inventory"], "unknown");
    assert_eq!(first["raw_retention"], "external-required");
    let id = first["snapshot_id"].as_str().unwrap();
    assert_eq!(id.len(), 64);
    assert!(id
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    assert_eq!(invoke_snapshot(&["create", root_text]).unwrap(), first);
    assert_eq!(
        fs::read(root.join("_catalog.json")).unwrap(),
        catalog_before
    );
    let listed = invoke_snapshot(&["list", root_text]).unwrap();
    assert_eq!(listed["snapshot_ids"], serde_json::json!([id]));
    assert!(listed["verification"]
        .as_str()
        .unwrap()
        .contains("manifest names only"));
    fs::remove_file(input).unwrap();
    fs::remove_file(root.join("_catalog.json")).unwrap();
    assert_eq!(invoke_snapshot(&["verify", root_text, id]).unwrap(), first);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn snapshot_cli_rejects_invalid_ids_and_preserves_manifest() {
    let dir = tests::temp_path("snapshot_cli_ids");
    fs::create_dir(&dir).unwrap();
    let input = dir.join("input.stdf");
    let root = dir.join("dataset");
    fs::write(&input, tests::build_test_stdf()).unwrap();
    convert_snapshot_fixture(&input, &root);
    let root_text = root.to_str().unwrap();
    let first = invoke_snapshot(&["create", root_text]).unwrap();
    let id = first["snapshot_id"].as_str().unwrap();
    let path = root.join("snapshots").join(format!("{id}.json"));
    let before = fs::read(&path).unwrap();
    for bad in [
        "../_catalog",
        "..\\_catalog",
        "not-a-hash",
        "",
        &"A".repeat(64),
    ] {
        assert!(invoke_snapshot(&["verify", root_text, bad]).is_err());
    }
    assert_eq!(fs::read(path).unwrap(), before);
    assert_eq!(invoke_snapshot(&["verify", root_text, id]).unwrap(), first);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn snapshot_cli_requires_explicit_incomplete_opt_in() {
    let dir = tests::temp_path("snapshot_cli_partial");
    fs::create_dir(&dir).unwrap();
    let input = dir.join("input.stdf");
    let broken = dir.join("broken.stdf");
    let root = dir.join("dataset");
    fs::write(&input, tests::build_test_stdf()).unwrap();
    fs::write(&broken, [0xff]).unwrap();
    let _ = execute(
        Cli::try_parse_from([
            "zstdf-cli",
            "convert",
            input.to_str().unwrap(),
            broken.to_str().unwrap(),
            "--layout",
            "catalog",
            "--output-dir",
            root.to_str().unwrap(),
            "--continue-on-error",
        ])
        .unwrap(),
        &mut Vec::new(),
    );
    let root_text = root.to_str().unwrap();
    let catalog_before = fs::read(root.join("_catalog.json")).unwrap();
    let catalog: Value = serde_json::from_slice(&catalog_before).unwrap();
    assert_eq!(catalog["run"]["status"], "partial");
    assert!(invoke_snapshot(&["create", root_text]).is_err());
    let partial = invoke_snapshot(&["create", root_text, "--allow-incomplete"]).unwrap();
    assert_eq!(partial["expected_inventory"], "unknown");
    assert_eq!(partial["catalog"], catalog);
    assert_eq!(
        partial["catalog"]["run"]["failures"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        fs::read(root.join("_catalog.json")).unwrap(),
        catalog_before
    );
    let id = partial["snapshot_id"].as_str().unwrap();
    assert_eq!(
        invoke_snapshot(&["verify", root_text, id]).unwrap(),
        partial
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn snapshot_cli_rejects_missing_datasets_without_creating_them() {
    let dir = tests::temp_path("snapshot_cli_missing");
    let root = dir.to_str().unwrap();
    for command in ["create", "list", "current"] {
        assert!(invoke_snapshot(&[command, root]).is_err());
        assert!(!dir.exists());
    }
    assert!(invoke_snapshot(&["verify", root, &"0".repeat(64)]).is_err());
    assert!(invoke_snapshot(&["select", root, &"0".repeat(64)]).is_err());
    assert!(!dir.exists());
}

#[test]
fn snapshot_cli_listing_does_not_claim_fragment_verification() {
    let dir = tests::temp_path("snapshot_cli_corruption");
    fs::create_dir(&dir).unwrap();
    let input = dir.join("input.stdf");
    let root = dir.join("dataset");
    fs::write(&input, tests::build_test_stdf()).unwrap();
    convert_snapshot_fixture(&input, &root);
    let root_text = root.to_str().unwrap();
    let snapshot = invoke_snapshot(&["create", root_text]).unwrap();
    let id = snapshot["snapshot_id"].as_str().unwrap();
    let source = snapshot["catalog"]["sources"]
        .as_object()
        .unwrap()
        .values()
        .next()
        .unwrap();
    let fragment = source["fragments"][0]["path"].as_str().unwrap();
    fs::write(root.join(fragment), b"corrupt parquet").unwrap();
    let listed = invoke_snapshot(&["list", root_text]).unwrap();
    assert_eq!(listed["snapshot_ids"], serde_json::json!([id]));
    assert!(listed["verification"]
        .as_str()
        .unwrap()
        .contains("manifest names only"));
    assert!(invoke_snapshot(&["verify", root_text, id]).is_err());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn snapshot_cli_selection_is_explicit_and_resolved_readers_remain_pinned() {
    use stdf_parquet::snapshot::resolve_snapshot;
    let dir = tests::temp_path("snapshot_cli_selection");
    fs::create_dir(&dir).unwrap();
    let input = dir.join("input.stdf");
    let another = dir.join("another.stdf");
    let root = dir.join("dataset");
    fs::write(&input, tests::build_test_stdf()).unwrap();
    convert_snapshot_fixture(&input, &root);
    let root_text = root.to_str().unwrap();
    let first = invoke_snapshot(&["create", root_text]).unwrap();
    assert!(invoke_snapshot(&["current", root_text]).is_err());
    assert!(!root.join("_current_snapshot.json").exists());
    let first_id = first["snapshot_id"].as_str().unwrap();
    assert_eq!(
        invoke_snapshot(&["select", root_text, first_id]).unwrap(),
        first
    );
    let selection = resolve_snapshot(&root, None).unwrap();
    let pointer_before = fs::read(root.join("_current_snapshot.json")).unwrap();
    let selected_paths: Vec<_> = selection
        .fragments()
        .iter()
        .map(|f| f.path.clone())
        .collect();

    fs::write(&another, tests::build_test_stdf()).unwrap();
    convert_snapshot_fixture(&another, &root);
    let second = invoke_snapshot(&["create", root_text]).unwrap();
    assert_ne!(first["snapshot_id"], second["snapshot_id"]);
    assert_eq!(second["catalog"]["sources"].as_object().unwrap().len(), 2);
    assert_eq!(invoke_snapshot(&["current", root_text]).unwrap(), first);
    assert_eq!(
        fs::read(root.join("_current_snapshot.json")).unwrap(),
        pointer_before
    );
    assert_eq!(
        invoke_snapshot(&["create", root_text, "--select"]).unwrap(),
        second
    );
    assert_eq!(invoke_snapshot(&["current", root_text]).unwrap(), second);
    assert_eq!(selection.snapshot().snapshot_id, first_id);
    assert_eq!(
        selection
            .fragments()
            .iter()
            .map(|f| f.path.clone())
            .collect::<Vec<_>>(),
        selected_paths
    );

    // Consume the captured inventory, never a glob or a newly read current catalog.
    let mut count = 0usize;
    for fragment in selection.fragments() {
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
            fs::File::open(&fragment.path).unwrap(),
        )
        .unwrap()
        .build()
        .unwrap();
        let rows: usize = reader.map(|batch| batch.unwrap().num_rows()).sum();
        assert_eq!(rows, fragment.rows);
        count += rows;
    }
    let expected: usize = first["catalog"]["sources"]
        .as_object()
        .unwrap()
        .values()
        .map(|source| source["rows"].as_u64().unwrap() as usize)
        .sum();
    assert_eq!(expected, 1); // The source fixture contains exactly one PTR result.
    assert_eq!(count, expected);
    fs::remove_file(root.join("_catalog.json")).unwrap();
    fs::remove_file(input).unwrap();
    fs::remove_file(another).unwrap();
    assert_eq!(invoke_snapshot(&["current", root_text]).unwrap(), second);
    assert_eq!(
        resolve_snapshot(&root, Some(first_id))
            .unwrap()
            .snapshot()
            .snapshot_id,
        first_id
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn snapshot_cli_select_rejects_corruption_without_changing_current() {
    let dir = tests::temp_path("snapshot_cli_bad_selection");
    fs::create_dir(&dir).unwrap();
    let input = dir.join("input.stdf");
    let root = dir.join("dataset");
    fs::write(&input, tests::build_test_stdf()).unwrap();
    convert_snapshot_fixture(&input, &root);
    let root_text = root.to_str().unwrap();
    let first = invoke_snapshot(&["create", root_text, "--select"]).unwrap();
    let id = first["snapshot_id"].as_str().unwrap();
    let pointer = fs::read(root.join("_current_snapshot.json")).unwrap();
    for bad in ["../_catalog", &"0".repeat(64)] {
        assert!(invoke_snapshot(&["select", root_text, bad]).is_err());
        assert_eq!(
            fs::read(root.join("_current_snapshot.json")).unwrap(),
            pointer
        );
    }
    let manifest = root.join("snapshots").join(format!("{id}.json"));
    let original = fs::read(&manifest).unwrap();
    fs::write(&manifest, b"corrupt").unwrap();
    assert!(invoke_snapshot(&["select", root_text, id]).is_err());
    assert!(invoke_snapshot(&["current", root_text]).is_err());
    assert_eq!(
        fs::read(root.join("_current_snapshot.json")).unwrap(),
        pointer
    );
    fs::write(manifest, original).unwrap();
    assert_eq!(invoke_snapshot(&["current", root_text]).unwrap(), first);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn snapshot_cli_select_requires_its_own_incomplete_opt_in() {
    let dir = tests::temp_path("snapshot_cli_partial_selection");
    fs::create_dir(&dir).unwrap();
    let input = dir.join("input.stdf");
    let root = dir.join("dataset");
    fs::write(&input, tests::build_test_stdf()).unwrap();
    convert_snapshot_fixture(&input, &root);
    let root_text = root.to_str().unwrap();
    let first = invoke_snapshot(&["create", root_text, "--select"]).unwrap();
    let mut catalog: Value =
        serde_json::from_slice(&fs::read(root.join("_catalog.json")).unwrap()).unwrap();
    catalog["run"]["status"] = "partial".into();
    catalog["run"]["failures"] = serde_json::json!([{"path":"missing.stdf","error":"unavailable"}]);
    fs::write(
        root.join("_catalog.json"),
        serde_json::to_vec(&catalog).unwrap(),
    )
    .unwrap();
    let partial = invoke_snapshot(&["create", root_text, "--allow-incomplete"]).unwrap();
    let id = partial["snapshot_id"].as_str().unwrap();
    assert!(invoke_snapshot(&["select", root_text, id]).is_err());
    assert_eq!(invoke_snapshot(&["current", root_text]).unwrap(), first);
    assert_eq!(
        invoke_snapshot(&["select", root_text, id, "--allow-incomplete"]).unwrap(),
        partial
    );
    assert_eq!(invoke_snapshot(&["current", root_text]).unwrap(), partial);
    assert_eq!(
        invoke_snapshot(&["create", root_text, "--select", "--allow-incomplete"]).unwrap(),
        partial
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn snapshot_cli_create_select_reports_saved_id_when_selection_fails() {
    let dir = tests::temp_path("snapshot_cli_select_publish_error");
    fs::create_dir(&dir).unwrap();
    let input = dir.join("input.stdf");
    let root = dir.join("dataset");
    fs::write(&input, tests::build_test_stdf()).unwrap();
    convert_snapshot_fixture(&input, &root);
    // A directory at the pointer path must not be replaced or removed.
    fs::create_dir(root.join("_current_snapshot.json")).unwrap();
    let error = invoke_snapshot(&["create", root.to_str().unwrap(), "--select"])
        .unwrap_err()
        .to_string();
    let ids = stdf_parquet::snapshot::list_snapshots(&root).unwrap();
    assert_eq!(ids.len(), 1);
    assert!(error.contains(&ids[0]));
    assert!(error.contains("was created, but selecting it failed"));
    assert!(root.join("_current_snapshot.json").is_dir());
    stdf_parquet::snapshot::verify_snapshot(&root, &ids[0]).unwrap();
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn snapshot_cli_retained_evidence_roundtrip_and_offline_inspection() {
    let dir = tests::temp_path("snapshot_cli_retained_evidence");
    fs::create_dir(&dir).unwrap();
    let input = dir.join("input.stdf");
    let root = dir.join("dataset");
    fs::write(&input, tests::build_test_stdf()).unwrap();
    convert_snapshot_fixture(&input, &root);
    let dataset = root.to_str().unwrap();
    let snapshot = invoke_snapshot(&["create", dataset]).unwrap();
    let snapshot_id = snapshot["snapshot_id"].as_str().unwrap();
    let source_id = snapshot["catalog"]["sources"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap();
    let bundle = invoke_snapshot(&[
        "retain-evidence",
        dataset,
        snapshot_id,
        source_id,
        input.to_str().unwrap(),
        "--memory-limit-mib",
        "64",
        "--disk-limit-mib",
        "128",
    ])
    .unwrap();
    assert_eq!(bundle["attempts"], 1);
    assert_eq!(bundle["measurements"], 1);
    let id = bundle["evidence_id"].as_str().unwrap();
    fs::remove_file(input).unwrap();
    fs::remove_file(root.join("_catalog.json")).unwrap();
    assert_eq!(
        invoke_snapshot(&["verify-evidence", dataset, id]).unwrap(),
        bundle
    );
    assert_eq!(
        invoke_snapshot(&["list-evidence", dataset]).unwrap()["evidence_ids"],
        serde_json::json!([id])
    );
    assert_eq!(
        invoke_snapshot(&["evidence-record", dataset, id, "1"]).unwrap()["kind"],
        "FAR"
    );
    assert_eq!(
        invoke_snapshot(&["evidence-attempt", dataset, id, "1"]).unwrap()["kind"],
        "PRR"
    );
    fs::remove_dir_all(dir).unwrap();
}
