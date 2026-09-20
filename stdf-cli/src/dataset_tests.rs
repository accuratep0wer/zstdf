use super::*;

#[test]
fn conversion_help_exposes_only_the_unified_command() {
    use clap::CommandFactory;
    let mut command = Cli::command();
    let help = command.render_long_help().to_string();
    assert!(help.contains("convert "));
    assert!(!help.contains("convert-many"));
    assert!(!help.contains("convert-partitioned"));
    for removed in ["convert-many", "convert-partitioned"] {
        assert!(
            Cli::try_parse_from(["zstdf-cli", removed, "in.stdf", "--output-dir", "out"]).is_err()
        );
    }
}

#[test]
fn convert_rejects_incompatible_options_before_writing() {
    let dir = tests::temp_path("convert_invalid_options");
    fs::create_dir(&dir).unwrap();
    let input = dir.join("one.stdf");
    let output = dir.join("out");
    fs::write(&input, tests::build_test_stdf()).unwrap();
    for extra in [
        vec!["--memory-limit-mib", "256"],
        vec!["--max-pending-tests", "100"],
        vec!["--max-open-writers", "2"],
        vec!["--row-group-rows", "1"],
        vec!["--max-output-files", "100"],
        vec!["--continue-on-error"],
        vec!["--layout", "files", "--row-group-rows", "1"],
        vec!["--layout", "catalog", "--no-overwrite"],
        vec!["--layout", "catalog", "--batch-size", "1"],
    ] {
        let mut argv = vec![
            "zstdf-cli",
            "convert",
            input.to_str().unwrap(),
            "--output-dir",
            output.to_str().unwrap(),
        ];
        argv.extend(extra);
        let error = execute(Cli::try_parse_from(argv).unwrap(), &mut Vec::new()).unwrap_err();
        assert!(error.to_string().contains("--layout catalog"));
        assert!(!output.exists());
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn convert_requires_an_unambiguous_destination_and_preserves_source_files() {
    let dir = tests::temp_path("convert_destination");
    fs::create_dir(&dir).unwrap();
    let first = dir.join("one.stdf");
    let second = dir.join("two.STDF.GZ");
    let bytes = tests::build_test_stdf();
    fs::write(&first, &bytes).unwrap();
    fs::write(&second, &bytes).unwrap();
    for paths in [
        vec![first.to_str().unwrap()],
        vec![first.to_str().unwrap(), second.to_str().unwrap()],
        vec![
            first.to_str().unwrap(),
            second.to_str().unwrap(),
            "out.parquet",
        ],
        vec![dir.to_str().unwrap(), "out.parquet"],
        vec![first.to_str().unwrap(), dir.to_str().unwrap()],
    ] {
        let mut argv = vec!["zstdf-cli", "convert"];
        argv.extend(paths);
        assert!(execute(Cli::try_parse_from(argv).unwrap(), &mut Vec::new()).is_err());
    }
    assert_eq!(fs::read(&first).unwrap(), bytes);
    assert_eq!(fs::read(&second).unwrap(), bytes);
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 2);
    for extra in [
        vec!["--layout", "catalog"],
        vec!["--partition-by", "lot-id"],
    ] {
        let mut argv = vec!["zstdf-cli", "convert", "in.stdf", "out.parquet"];
        argv.extend(extra);
        assert!(Cli::try_parse_from(argv).is_err());
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn convert_directory_gzip_retries_preserve_output() {
    use std::io::Write;
    let dir = tests::temp_path("convert_gzip_retry");
    fs::create_dir(&dir).unwrap();
    let input = dir.join("one.stdf.gz");
    let output = dir.join("out");
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gzip.write_all(&tests::build_test_stdf()).unwrap();
    fs::write(&input, gzip.finish().unwrap()).unwrap();
    let argv = [
        "zstdf-cli",
        "convert",
        input.to_str().unwrap(),
        "--output-dir",
        output.to_str().unwrap(),
        "--layout",
        "files",
        "--batch-size",
        "1",
        "--no-overwrite",
    ];
    let mut first = Vec::new();
    execute(Cli::try_parse_from(argv).unwrap(), &mut first).unwrap();
    let mut retry = Vec::new();
    execute(Cli::try_parse_from(argv).unwrap(), &mut retry).unwrap();
    assert_eq!(first, retry);
    assert!(String::from_utf8(first).unwrap().contains("rows=1"));
    assert!(fs::read_dir(&output).unwrap().next().is_some());
    assert!(!output.join("_catalog.json").exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn catalog_cli_verifies_and_dashboard_keeps_source_identity() {
    let dir = tests::temp_path("catalog_dashboard");
    fs::create_dir(&dir).unwrap();
    let first = dir.join("one.stdf");
    let second = dir.join("two.stdf");
    let root = dir.join("out");
    let html = dir.join("dashboard.html");
    fs::write(&first, tests::build_test_stdf()).unwrap();
    fs::write(&second, tests::build_test_stdf()).unwrap();
    execute(
        Cli::try_parse_from([
            "zstdf-cli",
            "convert",
            "--layout",
            "catalog",
            "--output-dir",
            root.to_str().unwrap(),
            first.to_str().unwrap(),
            second.to_str().unwrap(),
        ])
        .unwrap(),
        &mut Vec::new(),
    )
    .unwrap();
    execute(
        Cli::try_parse_from(["zstdf-cli", "verify-dataset", root.to_str().unwrap()]).unwrap(),
        &mut Vec::new(),
    )
    .unwrap();
    let mut out = Vec::new();
    execute(
        Cli::try_parse_from([
            "zstdf-cli",
            "dashboard-dir",
            root.to_str().unwrap(),
            html.to_str().unwrap(),
        ])
        .unwrap(),
        &mut out,
    )
    .unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("parts=2"));
    assert!(text.contains("rows=2"));
    assert!(fs::read_to_string(&html).unwrap().contains("lot-select"));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn catalog_cli_partial_run_returns_failure_but_retains_good_sources() {
    let dir = tests::temp_path("catalog_partial");
    fs::create_dir(&dir).unwrap();
    let first = dir.join("one.stdf");
    let second = dir.join("missing.stdf");
    let root = dir.join("out");
    fs::write(&first, tests::build_test_stdf()).unwrap();
    let args = [
        "zstdf-cli",
        "convert",
        "--layout",
        "catalog",
        "--continue-on-error",
        "--output-dir",
        root.to_str().unwrap(),
        second.to_str().unwrap(),
        first.to_str().unwrap(),
    ];
    assert!(execute(Cli::try_parse_from(args).unwrap(), &mut Vec::new()).is_err());
    let catalog = stdf_parquet::catalog::verify_catalog(&root).unwrap();
    assert_eq!(catalog.sources.len(), 1);
    assert_eq!(catalog.run.status, "partial");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn convert_catalog_cli_converts_and_reports_resource_metrics() {
    let dir = tests::temp_path("fragment_cli");
    fs::create_dir(&dir).unwrap();
    let input = dir.join("one.stdf");
    let output = dir.join("out");
    fs::write(&input, tests::build_test_stdf()).unwrap();
    let args = [
        "zstdf-cli",
        "convert",
        "--layout",
        "catalog",
        "--output-dir",
        output.to_str().unwrap(),
        "--max-open-writers",
        "1",
        "--row-group-rows",
        "1",
        input.to_str().unwrap(),
    ];
    for _ in 0..2 {
        let mut out = Vec::new();
        execute(Cli::try_parse_from(args).unwrap(), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("rows=1"));
        assert!(text.contains("fragments=1"));
        assert!(text.contains("peak_pending_bytes="));
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn convert_catalog_cli_rejects_invalid_budget_before_output() {
    let dir = tests::temp_path("fragment_cli_budget");
    fs::create_dir(&dir).unwrap();
    let input = dir.join("one.stdf");
    let output = dir.join("out");
    fs::write(&input, tests::build_test_stdf()).unwrap();
    let cli = Cli::try_parse_from([
        "zstdf-cli",
        "convert",
        "--layout",
        "catalog",
        "--output-dir",
        output.to_str().unwrap(),
        "--memory-limit-mib",
        "0",
        input.to_str().unwrap(),
    ])
    .unwrap();
    assert!(execute(cli, &mut Vec::new()).is_err());
    assert!(!output.exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn convert_directory_expands_directories_and_deduplicates_inputs() {
    let dir = tests::temp_path("multi_cli");
    fs::create_dir_all(dir.join("nested")).unwrap();
    let input = dir.join("one.stdf");
    fs::write(&input, tests::build_test_stdf()).unwrap();
    fs::write(dir.join("nested/two.STD"), tests::build_test_stdf()).unwrap();
    fs::write(dir.join("ignore.txt"), b"not STDF").unwrap();
    let output = dir.join("dataset");
    let mut out = Vec::new();
    execute(
        Cli::try_parse_from([
            "zstdf-cli",
            "convert",
            "--output-dir",
            output.to_str().unwrap(),
            "--partition-by",
            "lot-id,wafer-id",
            dir.to_str().unwrap(),
            input.to_str().unwrap(),
        ])
        .unwrap(),
        &mut out,
    )
    .unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(text.contains("files=2"));
    assert!(text.contains("rows=2"));
    assert!(output
        .join("lot_id=__empty/wafer_id=__HIVE_DEFAULT_PARTITION__")
        .is_dir());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn convert_directory_rejects_unknown_keys_and_missing_inputs() {
    assert!(Cli::try_parse_from([
        "zstdf-cli",
        "convert",
        "--output-dir",
        "out",
        "--partition-by",
        "invalid",
        "in.stdf"
    ])
    .is_err());
    assert!(Cli::try_parse_from(["zstdf-cli", "convert", "--output-dir", "out"]).is_err());
}

#[test]
fn convert_directory_empty_directory_produces_no_output() {
    let dir = tests::temp_path("multi_empty");
    fs::create_dir(&dir).unwrap();
    let output = dir.join("out");
    let cli = Cli::try_parse_from([
        "zstdf-cli",
        "convert",
        "--output-dir",
        output.to_str().unwrap(),
        dir.to_str().unwrap(),
    ])
    .unwrap();
    assert!(execute(cli, &mut Vec::new()).is_err());
    assert!(!output.exists());
    fs::remove_dir(dir).unwrap();
}
