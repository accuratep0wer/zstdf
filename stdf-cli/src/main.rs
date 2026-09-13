// Copyright 2026 zstdf contributors
// SPDX-License-Identifier: Apache-2.0

use std::error::Error;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use stdf_core::{RecordType, StdfError, StdfRecord};
use stdf_io::StdfReader;
use stdf_validate::{Severity, ValidationReport};

mod dashboard;
mod partitioned;
mod sanity;
mod traceability;

#[cfg(test)]
mod dataset_tests;

type CliResult<T> = Result<T, Box<dyn Error>>;

#[derive(Debug, Parser)]
#[command(name = "zstdf-cli")]
#[command(about = "Inspect and convert STDF files", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Audit CP/FT source fields and preview run metadata and per-unit first values.
    Sanity(sanity::Arguments),
    /// Trace device test steps and retest differences directly from STDF.
    Traceability(traceability::Arguments),
    /// Verify hashes, paths, schemas, and row counts of the current dataset snapshot.
    VerifyDataset { input: PathBuf },
    /// Remove abandoned staging files after checking that no writer is active.
    RecoverDataset { input: PathBuf },
    /// Generate a lot-selectable dashboard from current catalog versions only.
    DashboardDir {
        input: PathBuf,
        output: PathBuf,
        #[arg(long, default_value = "zstdf Dataset DataView")]
        title: String,
        /// Accounted analysis memory, not a hard process RSS limit.
        #[arg(long, default_value_t = 256)]
        memory_limit_mib: usize,
        #[arg(long, default_value_t = 32)]
        max_lots: usize,
        #[arg(long, default_value_t = 100_000)]
        max_parts: usize,
    },
    /// Convert PTR results to bounded, row-partitioned Parquet fragments.
    ConvertPartitioned(partitioned::Arguments),
    /// Print file-level decode summary.
    Info {
        /// Input STDF path. Gzip is auto-detected by extension or magic bytes.
        input: PathBuf,
    },
    /// Dump decoded records as stable text lines.
    Dump {
        /// Input STDF path. Gzip is auto-detected by extension or magic bytes.
        input: PathBuf,
        /// Stop after this many records.
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Run legacy-compatible STDF sanity checks.
    Check {
        /// Input STDF path. Gzip is auto-detected by extension or magic bytes.
        input: PathBuf,
    },
    /// Write a reference-style ASCII dump of decoded records.
    ToAscii {
        /// Input STDF path. Gzip is auto-detected by extension or magic bytes.
        input: PathBuf,
        /// Output text path. Defaults to input path with .txt extension.
        output: Option<PathBuf>,
        /// Accept legacy debug flag. Currently preserves output format.
        #[arg(long)]
        debug: bool,
    },
    /// Recursively sanity-check .stdf/.std files under a directory.
    BatchCheck {
        /// Root directory to scan.
        root_dir: PathBuf,
        /// Consolidated report path. Defaults to sanity_report.txt in root-dir.
        report_path: Option<PathBuf>,
        /// Worker thread count. Values below 1 are treated as 1.
        #[arg(long, default_value_t = 1)]
        threads: usize,
    },
    /// Convert STDF records to long-format EAV Parquet.
    Convert {
        /// Input STDF path. Gzip is auto-detected by extension or magic bytes.
        input: PathBuf,
        /// Output Parquet path.
        output: PathBuf,
        /// Target EAV rows per Arrow batch.
        #[arg(long, default_value_t = 65_536)]
        batch_size: usize,
        /// Return existing manifest summary instead of replacing an existing output.
        #[arg(long)]
        no_overwrite: bool,
    },
    /// Convert files or directories to a partitioned Parquet dataset.
    ConvertMany {
        /// STDF files or directories to scan recursively (including .stdf.gz/.std.gz).
        #[arg(required = true)]
        inputs: Vec<PathBuf>,
        #[arg(long)]
        output_dir: PathBuf,
        /// Comma-separated input-file, lot-id, wafer-id keys.
        #[arg(long, value_delimiter = ',', default_value = "input-file")]
        partition_by: Vec<stdf_parquet::PartitionKey>,
        #[arg(long, default_value_t = 65_536)]
        batch_size: usize,
        #[arg(long)]
        no_overwrite: bool,
    },
    /// Generate an interactive HTML dashboard from an EAV Parquet file.
    Dashboard {
        /// Input EAV Parquet path produced by `convert`.
        input: PathBuf,
        /// Output self-contained HTML path.
        output: PathBuf,
        /// Dashboard title.
        #[arg(long, default_value = "zstdf DataView")]
        title: String,
        /// Maximum numeric tests considered for correlation analysis.
        #[arg(long, default_value_t = 16)]
        max_correlation_tests: usize,
    },
}

fn main() {
    if let Err(error) = execute(Cli::parse(), &mut io::stdout()) {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn execute(cli: Cli, out: &mut impl Write) -> CliResult<()> {
    match cli.command {
        Command::Sanity(args) => sanity::execute(args, out),
        Command::Traceability(args) => traceability::execute(args, out),
        Command::VerifyDataset { input } => {
            let catalog = stdf_parquet::catalog::verify_catalog(&input)?;
            writeln!(
                out,
                "sources={} revision={} run_status={}",
                catalog.sources.len(),
                catalog.revision,
                catalog.run.status
            )?;
            Ok(())
        }
        Command::RecoverDataset { input } => {
            let removed = stdf_parquet::catalog::recover_dataset(&input)?;
            writeln!(out, "recovered_items={}", removed.len())?;
            Ok(())
        }
        Command::DashboardDir {
            input,
            output,
            title,
            memory_limit_mib,
            max_lots,
            max_parts,
        } => {
            let summary = dashboard::generate_dataset_dashboard(
                &input,
                &output,
                dashboard::DashboardOptions {
                    title,
                    ..Default::default()
                },
                dashboard::DatasetLimits {
                    max_memory_bytes: memory_limit_mib
                        .checked_mul(1024 * 1024)
                        .ok_or("memory budget overflow")?,
                    max_lots,
                    max_parts,
                },
            )?;
            writeln!(
                out,
                "rows={}\nparts={}\nyield_percent={:.2}",
                summary.rows, summary.parts, summary.yield_percent
            )?;
            Ok(())
        }
        Command::ConvertPartitioned(args) => partitioned::execute(args, out),
        Command::Info { input } => info(input, out),
        Command::Dump { input, limit } => dump(input, limit, out),
        Command::Check { input } => check(input, out),
        Command::ToAscii {
            input,
            output,
            debug,
        } => to_ascii(input, output, debug, out),
        Command::BatchCheck {
            root_dir,
            report_path,
            threads,
        } => batch_check(root_dir, report_path, threads, out),
        Command::Convert {
            input,
            output,
            batch_size,
            no_overwrite,
        } => convert(input, output, batch_size, no_overwrite, out),
        Command::ConvertMany {
            inputs,
            output_dir,
            partition_by,
            batch_size,
            no_overwrite,
        } => convert_many(
            inputs,
            output_dir,
            partition_by,
            batch_size,
            no_overwrite,
            out,
        ),
        Command::Dashboard {
            input,
            output,
            title,
            max_correlation_tests,
        } => dashboard_command(input, output, title, max_correlation_tests, out),
    }
}

fn info(input: PathBuf, out: &mut impl Write) -> CliResult<()> {
    let reader = StdfReader::open(input)?;
    let summary = reader.summarize();
    writeln!(out, "byte_order={:?}", summary.byte_order)?;
    writeln!(out, "total_records={}", summary.total_records)?;
    writeln!(out, "bytes_consumed={}", summary.bytes_consumed)?;
    writeln!(out, "truncated={}", summary.truncated)?;

    let mut counts: Vec<(String, usize)> = summary
        .record_counts
        .iter()
        .map(|(record_type, count)| (record_type_name(*record_type), *count))
        .collect();
    counts.sort_by(|left, right| left.0.cmp(&right.0));
    for (record_type, count) in counts {
        writeln!(out, "record_count.{record_type}={count}")?;
    }

    for error in summary.errors {
        writeln!(out, "error={error}")?;
    }
    Ok(())
}

fn convert_many(
    inputs: Vec<PathBuf>,
    output_dir: PathBuf,
    partition_by: Vec<stdf_parquet::PartitionKey>,
    batch_size: usize,
    no_overwrite: bool,
    out: &mut impl Write,
) -> CliResult<()> {
    let mut paths = Vec::new();
    for input in inputs {
        if input.is_dir() {
            paths.extend(find_stdf_files(&input)?);
        } else {
            paths.push(input);
        }
    }
    let summary = stdf_parquet::files_to_partitioned_parquet_dir(
        &paths,
        &output_dir,
        batch_size,
        &stdf_parquet::AtomicWriteOptions {
            overwrite: !no_overwrite,
            ..Default::default()
        },
        &partition_by,
    )?;
    writeln!(out, "files={}", summary.files)?;
    writeln!(out, "batches={}", summary.batches)?;
    writeln!(out, "rows={}", summary.rows)?;
    for file in summary.outputs {
        writeln!(
            out,
            "input={} output={} rows={} batches={}",
            file.input_path.display(),
            file.output_path.display(),
            file.rows,
            file.batches
        )?;
    }
    Ok(())
}

fn dashboard_command(
    input: PathBuf,
    output: PathBuf,
    title: String,
    max_correlation_tests: usize,
    out: &mut impl Write,
) -> CliResult<()> {
    let summary = dashboard::generate_dashboard(
        &input,
        &output,
        dashboard::DashboardOptions {
            title,
            max_correlation_tests: max_correlation_tests.max(2),
            ..dashboard::DashboardOptions::default()
        },
    )?;
    writeln!(out, "rows={}", summary.rows)?;
    writeln!(out, "parts={}", summary.parts)?;
    writeln!(out, "yield_percent={:.2}", summary.yield_percent)?;
    Ok(())
}

fn dump(input: PathBuf, limit: Option<usize>, out: &mut impl Write) -> CliResult<()> {
    let reader = StdfReader::open(input)?;
    let mut count = 0usize;
    for record in reader.records() {
        if limit.is_some_and(|limit| count >= limit) {
            break;
        }
        writeln!(out, "{}", record?)?;
        count += 1;
    }
    Ok(())
}

fn check(input: PathBuf, out: &mut impl Write) -> CliResult<()> {
    let reader = StdfReader::open(&input)?;
    let report = stdf_validate::validate_bytes(reader.data())?;
    write_validation_report(&input, &report, out)?;
    if report.has_errors() {
        return Err(format!("validation failed: {}", report.summary()).into());
    }
    Ok(())
}

fn to_ascii(
    input: PathBuf,
    output: Option<PathBuf>,
    _debug: bool,
    out: &mut impl Write,
) -> CliResult<()> {
    let output = output.unwrap_or_else(|| input.with_extension("txt"));
    let reader = StdfReader::open(&input)?;
    let mut dump = String::new();
    let display_name = input
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| input.to_string_lossy().into_owned());
    dump.push_str(&stdf_ascii::AsciiDumper::header(&display_name));
    let mut dumper = stdf_ascii::AsciiDumper::new();
    for record in reader.records() {
        dump.push_str(&dumper.render(&record?));
    }
    dump.push_str(stdf_ascii::AsciiDumper::footer());
    fs::write(&output, dump)?;
    writeln!(out, "output={}", output.display())?;
    Ok(())
}

fn batch_check(
    root_dir: PathBuf,
    report_path: Option<PathBuf>,
    threads: usize,
    out: &mut impl Write,
) -> CliResult<()> {
    let files = find_stdf_files(&root_dir)?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads.max(1))
        .build()?;
    let results = pool.install(|| {
        use rayon::prelude::*;
        files
            .par_iter()
            .map(|path| {
                let result = StdfReader::open(path)
                    .map_err(|error| error.to_string())
                    .and_then(|reader| {
                        stdf_validate::validate_bytes(reader.data())
                            .map_err(|error| error.to_string())
                    });
                (path.clone(), result)
            })
            .collect::<Vec<_>>()
    });

    let report_path = report_path.unwrap_or_else(|| root_dir.join("sanity_report.txt"));
    let mut report = String::new();
    let mut failed = 0usize;
    for (path, result) in &results {
        report.push_str(&format!("FILE {}\n", path.display()));
        match result {
            Ok(validation) => {
                report.push_str(&format!("SUMMARY {}\n", validation.summary()));
                if validation.has_errors() {
                    failed += 1;
                }
                for finding in &validation.findings {
                    report.push_str(&format_finding(finding));
                    report.push('\n');
                }
                for error in &validation.decode_errors {
                    report.push_str(&format!("DECODE_ERROR {error}\n"));
                }
            }
            Err(error) => {
                failed += 1;
                report.push_str(&format!("ERROR {error}\n"));
            }
        }
        report.push('\n');
    }
    fs::write(&report_path, report)?;
    writeln!(out, "files={}", results.len())?;
    writeln!(out, "failed={failed}")?;
    writeln!(out, "report={}", report_path.display())?;
    if failed > 0 {
        return Err(format!("{failed} file(s) failed validation").into());
    }
    Ok(())
}

fn write_validation_report(
    input: &Path,
    report: &ValidationReport,
    out: &mut impl Write,
) -> CliResult<()> {
    writeln!(out, "file={}", input.display())?;
    writeln!(out, "summary={}", report.summary())?;
    for finding in &report.findings {
        writeln!(out, "{}", format_finding(finding))?;
    }
    for error in &report.decode_errors {
        writeln!(out, "decode_error={error}")?;
    }
    Ok(())
}

fn format_finding(finding: &stdf_validate::Finding) -> String {
    let severity = match finding.severity {
        Severity::Fatal => "fatal",
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    };
    let record_type = finding.record_type.unwrap_or("-");
    let offset = finding
        .file_offset
        .map(|offset| format!("0x{offset:08x}"))
        .unwrap_or_else(|| "-".to_string());
    format!(
        "{severity} rule={} record={record_type} offset={offset} message={}",
        finding.rule, finding.message
    )
}

fn find_stdf_files(root: &Path) -> CliResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_stdf_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_stdf_files(path: &Path, files: &mut Vec<PathBuf>) -> CliResult<()> {
    if path.is_file() {
        if is_stdf_path(path) {
            files.push(path.to_path_buf());
        }
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_symlink() {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            collect_stdf_files(&path, files)?;
        } else if is_stdf_path(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_stdf_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase();
    [".stdf", ".std", ".stdf.gz", ".std.gz"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
}
fn convert(
    input: PathBuf,
    output: PathBuf,
    batch_size: usize,
    no_overwrite: bool,
    out: &mut impl Write,
) -> CliResult<()> {
    let options = stdf_parquet::AtomicWriteOptions {
        overwrite: !no_overwrite,
        write_manifest: true,
        cleanup_orphaned_temp: true,
    };
    if no_overwrite && output.exists() {
        let summary = stdf_parquet::records_to_parquet_path_atomic(
            std::iter::empty::<std::result::Result<StdfRecord, StdfError>>(),
            &output,
            batch_size.max(1),
            &options,
        )?;
        writeln!(out, "batches={}", summary.batches)?;
        writeln!(out, "rows={}", summary.rows)?;
        return Ok(());
    }

    let reader = StdfReader::open(input)?;
    let summary = stdf_parquet::records_to_parquet_path_atomic(
        reader.records(),
        output,
        batch_size.max(1),
        &options,
    )?;
    writeln!(out, "batches={}", summary.batches)?;
    writeln!(out, "rows={}", summary.rows)?;
    Ok(())
}

fn record_type_name(record_type: RecordType) -> String {
    match record_type {
        RecordType::Unknown(typ, sub) => format!("UNKNOWN_{typ}_{sub}"),
        _ => record_type.mnemonic().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    use super::*;

    #[test]
    fn info_reports_summary_and_counts() {
        let input = temp_path("info.stdf");
        std::fs::write(&input, build_test_stdf()).unwrap();
        let mut out = Vec::new();

        execute(
            Cli::try_parse_from(["zstdf-cli", "info", input.to_str().unwrap()]).unwrap(),
            &mut out,
        )
        .unwrap();

        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("total_records=4"));
        assert!(output.contains("record_count.PTR=1"));
        assert!(output.contains("record_count.PRR=1"));

        std::fs::remove_file(input).ok();
    }

    #[test]
    fn dump_respects_limit() {
        let input = temp_path("dump.stdf");
        std::fs::write(&input, build_test_stdf()).unwrap();
        let mut out = Vec::new();

        execute(
            Cli::try_parse_from(["zstdf-cli", "dump", input.to_str().unwrap(), "--limit", "2"])
                .unwrap(),
            &mut out,
        )
        .unwrap();

        let lines = String::from_utf8(out).unwrap().lines().count();
        assert_eq!(lines, 2);

        std::fs::remove_file(input).ok();
    }

    #[test]
    fn convert_writes_readable_parquet() {
        let input = temp_path("convert.stdf");
        let output = temp_path("convert.parquet");
        std::fs::write(&input, build_test_stdf()).unwrap();
        let mut out = Vec::new();

        execute(
            Cli::try_parse_from([
                "zstdf-cli",
                "convert",
                input.to_str().unwrap(),
                output.to_str().unwrap(),
                "--batch-size",
                "1",
            ])
            .unwrap(),
            &mut out,
        )
        .unwrap();

        let output_text = String::from_utf8(out).unwrap();
        assert!(output_text.contains("rows=1"));
        let reader =
            ParquetRecordBatchReaderBuilder::try_new(std::fs::File::open(&output).unwrap())
                .unwrap()
                .build()
                .unwrap();
        let rows: usize = reader
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
            .iter()
            .map(arrow::record_batch::RecordBatch::num_rows)
            .sum();
        assert_eq!(rows, 1);

        std::fs::remove_file(input).ok();
        std::fs::remove_file(stdf_parquet::manifest_path(&output)).ok();
        std::fs::remove_file(output).ok();
    }

    #[test]
    fn convert_rejects_invalid_input() {
        let output = temp_path("missing.parquet");
        let mut out = Vec::new();

        let result = execute(
            Cli::try_parse_from([
                "zstdf-cli",
                "convert",
                "missing.stdf",
                output.to_str().unwrap(),
            ])
            .unwrap(),
            &mut out,
        );

        assert!(result.is_err());
        assert!(!output.exists());
    }

    #[test]
    fn convert_no_overwrite_returns_existing_manifest_summary() {
        let input = temp_path("convert_idempotent.stdf");
        let output = temp_path("convert_idempotent.parquet");
        std::fs::write(&input, build_test_stdf()).unwrap();
        let mut first_out = Vec::new();

        execute(
            Cli::try_parse_from([
                "zstdf-cli",
                "convert",
                input.to_str().unwrap(),
                output.to_str().unwrap(),
            ])
            .unwrap(),
            &mut first_out,
        )
        .unwrap();

        let mut second_out = Vec::new();
        execute(
            Cli::try_parse_from([
                "zstdf-cli",
                "convert",
                "missing.stdf",
                output.to_str().unwrap(),
                "--no-overwrite",
            ])
            .unwrap(),
            &mut second_out,
        )
        .unwrap();

        let output_text = String::from_utf8(second_out).unwrap();
        assert!(output_text.contains("rows=1"));
        std::fs::remove_file(input).ok();
        std::fs::remove_file(stdf_parquet::manifest_path(&output)).ok();
        std::fs::remove_file(output).ok();
    }

    #[test]
    fn dashboard_writes_interactive_html_from_parquet() {
        let input = temp_path("dashboard.stdf");
        let parquet = temp_path("dashboard.parquet");
        let html = temp_path("dashboard.html");
        std::fs::write(&input, build_test_stdf()).unwrap();

        execute(
            Cli::try_parse_from([
                "zstdf-cli",
                "convert",
                input.to_str().unwrap(),
                parquet.to_str().unwrap(),
            ])
            .unwrap(),
            &mut Vec::new(),
        )
        .unwrap();

        let mut out = Vec::new();
        execute(
            Cli::try_parse_from([
                "zstdf-cli",
                "dashboard",
                parquet.to_str().unwrap(),
                html.to_str().unwrap(),
                "--title",
                "Unit Dashboard",
            ])
            .unwrap(),
            &mut out,
        )
        .unwrap();

        let output_text = String::from_utf8(out).unwrap();
        let html_text = std::fs::read_to_string(&html).unwrap();
        assert!(output_text.contains("rows=1"));
        assert!(html_text.contains("Unit Dashboard"));
        assert!(html_text.contains("failure-commonality"));
        assert!(html_text.contains("correlation-heatmap"));

        std::fs::remove_file(input).ok();
        std::fs::remove_file(stdf_parquet::manifest_path(&parquet)).ok();
        std::fs::remove_file(parquet).ok();
        std::fs::remove_file(html).ok();
    }

    #[test]
    fn check_reports_validation_summary() {
        let input = temp_path("check.stdf");
        std::fs::write(&input, build_test_stdf()).unwrap();
        let mut out = Vec::new();

        execute(
            Cli::try_parse_from(["zstdf-cli", "check", input.to_str().unwrap()]).unwrap(),
            &mut out,
        )
        .unwrap();

        let output = String::from_utf8(out).unwrap();
        assert!(output.contains("summary="));
        assert!(output.contains("ABORT/NO_MRR"));

        std::fs::remove_file(input).ok();
    }

    #[test]
    fn to_ascii_writes_reference_style_text_file() {
        let input = temp_path("ascii.stdf");
        let output = temp_path("ascii.txt");
        std::fs::write(&input, build_test_stdf()).unwrap();
        let mut out = Vec::new();

        execute(
            Cli::try_parse_from([
                "zstdf-cli",
                "to-ascii",
                input.to_str().unwrap(),
                output.to_str().unwrap(),
            ])
            .unwrap(),
            &mut out,
        )
        .unwrap();

        let cli_output = String::from_utf8(out).unwrap();
        let ascii = std::fs::read_to_string(&output).unwrap();
        assert!(cli_output.contains("output="));
        assert!(ascii.contains("FAR Record"));
        assert!(ascii.contains("PTR Record"));
        assert!(ascii.contains("End of file. Done!"));

        std::fs::remove_file(input).ok();
        std::fs::remove_file(output).ok();
    }

    #[test]
    fn batch_check_writes_consolidated_report() {
        let dir = std::env::temp_dir().join(format!(
            "zstdf_cli_batch_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("one.stdf");
        let report = dir.join("report.txt");
        std::fs::write(&input, build_test_stdf()).unwrap();
        let mut out = Vec::new();

        execute(
            Cli::try_parse_from([
                "zstdf-cli",
                "batch-check",
                dir.to_str().unwrap(),
                report.to_str().unwrap(),
                "--threads",
                "1",
            ])
            .unwrap(),
            &mut out,
        )
        .unwrap();

        let cli_output = String::from_utf8(out).unwrap();
        let report_text = std::fs::read_to_string(&report).unwrap();
        assert!(cli_output.contains("files=1"));
        assert!(report_text.contains("FILE"));
        assert!(report_text.contains("SUMMARY"));

        std::fs::remove_file(input).ok();
        std::fs::remove_file(report).ok();
        std::fs::remove_dir(dir).ok();
    }
    pub(super) fn temp_path(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("zstdf_cli_{nanos}_{name}"))
    }

    pub(super) fn build_test_stdf() -> Vec<u8> {
        let mut buf = Vec::new();
        push_record(&mut buf, 0, 10, &[2, 4]);
        push_record(&mut buf, 5, 10, &[1, 0]);

        let mut ptr = Vec::new();
        ptr.extend_from_slice(&1001u32.to_le_bytes());
        ptr.push(1);
        ptr.push(0);
        ptr.push(0);
        ptr.push(0);
        ptr.extend_from_slice(&3.25f32.to_le_bytes());
        push_record(&mut buf, 15, 10, &ptr);

        let mut prr = Vec::new();
        prr.push(1);
        prr.push(0);
        prr.push(0);
        prr.extend_from_slice(&1u16.to_le_bytes());
        prr.extend_from_slice(&1u16.to_le_bytes());
        prr.extend_from_slice(&1u16.to_le_bytes());
        push_record(&mut buf, 5, 20, &prr);

        buf
    }

    fn push_record(buf: &mut Vec<u8>, typ: u8, sub: u8, body: &[u8]) {
        buf.extend_from_slice(&(body.len() as u16).to_le_bytes());
        buf.push(typ);
        buf.push(sub);
        buf.extend_from_slice(body);
    }
}
