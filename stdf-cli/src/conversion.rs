use super::{convert, convert_files, partitioned, CliResult};
use std::io::Write;
use std::path::PathBuf;
use stdf_parquet::PartitionKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum Layout {
    /// One Parquet file per source, with companion manifests.
    Files,
    /// Bounded row fragments and a versioned catalog for dashboard-dir.
    Catalog,
}

#[derive(Debug, clap::Args)]
#[command(
    after_help = "Examples:\n  zstdf-cli convert input.stdf output.parquet\n  zstdf-cli convert input.stdf inputs/ --output-dir dataset\n  zstdf-cli convert inputs/ --output-dir dataset --layout catalog"
)]
pub struct Arguments {
    /// Without --output-dir: INPUT OUTPUT. With it: files/directories to convert.
    #[arg(required = true, value_name = "PATHS")]
    paths: Vec<PathBuf>,
    /// Write multiple sources under this directory (recursive STDF/gzip scanning).
    #[arg(long)]
    output_dir: Option<PathBuf>,
    /// Directory layout; defaults to files. Supports PTR, MPR measurements, and FTR verdicts.
    #[arg(long, value_enum, requires = "output_dir")]
    layout: Option<Layout>,
    /// Comma-separated keys; defaults to input-file (files) or lot-id,wafer-id (catalog).
    #[arg(long, value_delimiter = ',', requires = "output_dir")]
    partition_by: Option<Vec<PartitionKey>>,
    /// Target EAV rows per batch in file output; default 65536, zero becomes one.
    #[arg(long)]
    batch_size: Option<usize>,
    /// Validate and reuse existing file output; unavailable for catalog layout.
    #[arg(long)]
    no_overwrite: bool,
    /// Catalog only: accounted memory budget in MiB, not process RSS; default 256.
    #[arg(long, requires = "output_dir")]
    memory_limit_mib: Option<usize>,
    /// Catalog only: maximum pending tests; default 100000.
    #[arg(long, requires = "output_dir")]
    max_pending_tests: Option<usize>,
    /// Catalog only: maximum open fragment writers; default 4.
    #[arg(long, requires = "output_dir")]
    max_open_writers: Option<usize>,
    /// Catalog only: maximum rows per row group and fragment; default 65536.
    #[arg(long, requires = "output_dir")]
    row_group_rows: Option<usize>,
    /// Catalog only: maximum fragments per invocation; default 10000.
    #[arg(long, requires = "output_dir")]
    max_output_files: Option<usize>,
    /// Catalog only: continue after failed inputs, but still exit nonzero.
    #[arg(long, requires = "output_dir")]
    continue_on_error: bool,
}

pub fn execute(mut args: Arguments, out: &mut impl Write) -> CliResult<()> {
    let catalog = args.layout == Some(Layout::Catalog);
    let catalog_options = args.memory_limit_mib.is_some()
        || args.max_pending_tests.is_some()
        || args.max_open_writers.is_some()
        || args.row_group_rows.is_some()
        || args.max_output_files.is_some()
        || args.continue_on_error;
    // Reject incompatible options before opening any input or output.
    if !catalog && catalog_options {
        return Err("resource limits and --continue-on-error require --layout catalog".into());
    }
    if catalog && (args.batch_size.is_some() || args.no_overwrite) {
        return Err("--layout catalog does not accept --batch-size or --no-overwrite; use --row-group-rows, and catalog retries automatically verify and reuse completed sources".into());
    }
    if let Some(output_dir) = args.output_dir {
        if catalog {
            partitioned::execute(
                partitioned::Arguments {
                    inputs: args.paths,
                    output_dir,
                    partition_by: args
                        .partition_by
                        .unwrap_or_else(|| vec![PartitionKey::LotId, PartitionKey::WaferId]),
                    memory_limit_mib: args.memory_limit_mib.unwrap_or(256),
                    max_pending_tests: args.max_pending_tests.unwrap_or(100_000),
                    max_open_writers: args.max_open_writers.unwrap_or(4),
                    row_group_rows: args.row_group_rows.unwrap_or(65_536),
                    max_output_files: args.max_output_files.unwrap_or(10_000),
                    continue_on_error: args.continue_on_error,
                },
                out,
            )
        } else {
            convert_files(
                args.paths,
                output_dir,
                args.partition_by
                    .unwrap_or_else(|| vec![PartitionKey::InputFile]),
                args.batch_size.unwrap_or(65_536).max(1),
                args.no_overwrite,
                out,
            )
        }
    } else {
        if args.paths.len() != 2 {
            return Err(
                "use convert <input> <output.parquet>, or convert <inputs...> --output-dir <dir>"
                    .into(),
            );
        }
        let output = args.paths.pop().unwrap();
        let input = args.paths.pop().unwrap();
        if input.is_dir() || output.is_dir() {
            return Err("directory conversion requires --output-dir <dir>".into());
        }
        // A pair of source files must not be mistaken for INPUT OUTPUT.
        if super::is_stdf_path(&output) {
            return Err(
                "output must not be an STDF input path; use --output-dir <dir> for multiple inputs"
                    .into(),
            );
        }
        convert(
            input,
            output,
            args.batch_size.unwrap_or(65_536),
            args.no_overwrite,
            out,
        )
    }
}
