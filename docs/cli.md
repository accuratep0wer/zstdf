# zstdf CLI

`zstdf-cli` provides the first packaged command-line workflow for inspecting
and converting STDF files.

## Commands

Trace device test flows and retest differences directly from STDF:

```powershell
.\target\release\zstdf-cli.exe traceability path\to\inputs --flow-config flow.json --output trace.html
```

Optional `--flow-closures closures.json` and `--identity-map identities.json`
provide explicit flow closure and cross-namespace identity links. See the
[configuration, limits, evidence schema and runnable demo](traceability.md).

Print decode summary:

```powershell
cargo run -p stdf-cli --bin zstdf-cli -- info path\to\input.stdf
```

Dump records as stable text lines:

```powershell
cargo run -p stdf-cli --bin zstdf-cli -- dump path\to\input.stdf --limit 20
```

Convert STDF to long-format EAV Parquet:

```powershell
cargo run -p stdf-cli --bin zstdf-cli -- convert path\to\input.stdf path\to\output.parquet --batch-size 65536
```

Conversion emits Arrow batches into Parquet. Batch size is a target, not a hard
memory limit: unfinished parts, input decoding, and writer buffers also consume
memory. Use catalog layout for accounted resource budgets. Successful
conversions are committed atomically through a same-directory
temporary file and write a manifest at `output.parquet.json`.

Use `--no-overwrite` for retry-safe jobs. If the output Parquet file and
manifest already exist, the command returns the committed row summary instead
of rewriting the file:

```powershell
cargo run -p stdf-cli --bin zstdf-cli -- convert path\to\input.stdf path\to\output.parquet --no-overwrite
```

Gzip input is auto-detected by `.gz` extension or gzip magic bytes.

Generate a self-contained interactive HTML dashboard from EAV Parquet:

```powershell
cargo run -p stdf-cli --bin zstdf-cli -- dashboard path\to\output.parquet path\to\dashboard.html --title "Lot DataView"
```

The dashboard reads Parquet batch-by-batch and embeds aggregate analysis rather
than raw rows, avoiding both CLI-side and browser-side memory blowups on large
files. It includes yield KPIs, failure Pareto, failure commonality, test
correlation, wafer/XY maps, bin distributions, process window statistics, and
data-completeness checks.

## Unified Conversion

Use `convert input.stdf output.parquet` for a single output file, or
`convert <inputs...> --output-dir dataset` for one output per source. Add
`--layout catalog` for bounded row fragments, catalog verification, and
`dashboard-dir`. The old `convert-many` and `convert-partitioned` subcommands
are removed: replace them with `convert` and `convert --layout catalog`,
respectively. Existing single-file syntax is unchanged.

With `--output-dir`, all positional paths are inputs. Without it, exactly two
paths are required; directory paths and STDF output filenames are rejected.
`--partition-by` requires `--output-dir`. Catalog resource options and
`--continue-on-error` require `--layout catalog`. Catalog layout rejects
`--batch-size` and `--no-overwrite`; use `--row-group-rows`, and let the catalog
verify and reuse successful source generations. Invalid option combinations
fail before input/output processing.

`dump` prints compact record summaries to stdout. `to-ascii` remains available
for detailed field-by-field text export, including formatted timestamps and
inherited PTR scale/unit information.

## Multi-file Conversion (Phase 10B)

Convert files or recursively scan directories:

```powershell
cargo run -p stdf-cli -- convert --output-dir dataset --partition-by lot-id,wafer-id input1.stdf input2.stdf
cargo run -p stdf-cli -- convert --output-dir dataset --no-overwrite path\to\inputs
```

The default partition key is `input-file`. Other keys are `lot-id` and
`wafer-id`, comma-separated in the desired directory order. Directory scans
include `.std`, `.stdf`, `.std.gz`, and `.stdf.gz` (case-insensitive) and skip
symbolic links. Explicit files may use any extension. Duplicate canonical
paths are converted only once. The command prints file, batch, and row totals
and the destination of each source.

Each source produces one Parquet file. Lot/wafer partitioning requires a single
value for each selected key across that source's EAV rows. For a mixed-wafer
file, use `--partition-by input-file`, or use the Phase 10C.1 command below
for row-level splitting. The existing dashboard command accepts one emitted Parquet file.

Sources are streamed, including gzip, and validated before any output is
created. Conversion reads each source twice. Only one source and Parquet writer
are processed at a time; active-part state and Parquet row groups still consume
memory, so `--batch-size` is not a hard RAM limit.

Directory values are percent-encoded, including lowercase letters to prevent
Windows case collisions. Null is `__HIVE_DEFAULT_PARTITION__`; empty string is
`__empty`. Literal sentinel values are escaped. Encoded values above 180 bytes
are rejected before output creation. EAV columns retain the original values;
read those columns rather than inferring null/empty strings from folder names.

Output names depend on canonical source path and content. Reordering inputs
or retrying a subset preserves names. Changing bytes or moving a source creates
a new output and retains the old version; do not blindly combine old and new
versions when calculating yield. Catalog layout selects current versions for dataset dashboards.

`--no-overwrite` validates the current source and checks the existing Parquet
footer, schema, and row count against the manifest before returning its summary.
It requires the original source to remain available. Fingerprints are local
identifiers, not cryptographic checksums, and footer checks do not validate all
data pages. Rerun without `--no-overwrite` to repair a damaged output.

A `.zstdf-convert.lock` in the output root prevents overlapping multi-file
conversions. Normal completion or handled errors remove it. After a crash,
verify no conversion is still running before removing that specific lock and
retrying. Do not run a single-file conversion against a dataset's output names
while a multi-file job owns the lock. If writing fails partway through, earlier
committed files remain; rerun with `--no-overwrite` after resolving the error.

## Bounded Row Partitions (Phase 10C.1)

Split mixed-lot or mixed-wafer PTR results into immutable Parquet fragments:

```powershell
cargo run -p stdf-cli -- convert --layout catalog --output-dir dataset --partition-by lot-id,wafer-id .\inputs
```

This command defaults to lot/wafer partitioning and accepts the same file and
directory inputs as `convert`. It prints total rows/fragments, each output
path, peak open writers, and accounted pending/writer bytes. Repeated runs with
the same inputs and options validate and reuse completed source generations.

Resource options:

| Option | Default | Meaning |
| --- | ---: | --- |
| `--memory-limit-mib` | 256 | Accounted conversion memory budget |
| `--max-pending-tests` | 100000 | Tests retained across all unfinished parts |
| `--max-open-writers` | 4 | Concurrent Parquet writers |
| `--row-group-rows` | 65536 | Maximum rows per row group and fragment |
| `--max-output-files` | 10000 | Total fragments per invocation, including reused ones |

For a smaller workload and budget:

```powershell
cargo run -p stdf-cli -- convert --layout catalog --output-dir dataset-small --memory-limit-mib 16 --max-output-files 1000 --max-open-writers 2 --row-group-rows 1024 .\inputs
```

The file limit reserves 8 KiB per possible output for paths/receipts/plan
metadata; an unaffordable combination is rejected before output creation.
Pending state and Parquet buffers have additional limits. A large unfinished
part fails with a resource-limit error before further tests are retained.
The memory option is not an operating-system RSS ceiling; runtime, allocator,
decoder, and encoding overhead still need headroom. Disk spilling is not enabled.

PTR results are supported. MPR/FTR expansion is not yet supported by this
bounded workflow and produces an explicit error. Malformed part/test records,
duplicate PIR, and missing PRR also fail instead of dropping measurements.

Each source is published under `objects/<generation>/source-...`, containing
partition folders and `_SUCCESS.json`. The root `_catalog.json` selects its
current generation. Files are numbered fragments with at most one row group.
A handled error removes owned staging data while completed sources remain
reusable. Failed source updates retain their last successful catalog version.

Changing a source, its path, or conversion options creates a separate generation.
Older generations are retained. Do not glob every Parquet file into yield
calculations; use `dashboard-dir` to exclude superseded or unpublished files.
Retries verify whole-file SHA-256 hashes, schemas, and counts. Input/root paths
are limited to 1024 encoded bytes. Conversion reserves one quarter of its budget
for catalog work. Older pre-catalog outputs require rerunning conversion to
create a managed catalog; they are not automatically imported.

## Catalog Verification, Recovery, and Dashboards (Phase 10C.2)

```powershell
cargo run -p stdf-cli -- convert --layout catalog --output-dir dataset --continue-on-error .\inputs
cargo run -p stdf-cli -- verify-dataset dataset
cargo run -p stdf-cli -- dashboard-dir dataset dashboard.html --title "Lot DataView"
```

Fail-fast is the default. `--continue-on-error` attempts the remaining files but
still exits nonzero when any input fails. `_catalog.json` records per-file errors
and `complete`, `partial`, `failed`, `running`, or `interrupted` run status.
Successful sources remain committed independently. Verification checks snapshot
integrity, not whether every input in the latest run succeeded.

After a terminated process, explicitly recover and repeat the original conversion:

```powershell
cargo run -p stdf-cli -- recover-dataset dataset
cargo run -p stdf-cli -- convert --layout catalog --output-dir dataset .\inputs
```

Recovery refuses active or unverifiable legacy lock owners, removes only abandoned
staging/catalog temporary files, and preserves published generations. Do not delete
`.catalog.guard`; its kernel lock protects cooperating catalog writers. This is a
local-filesystem workflow, not a distributed transaction or automatic disk cleanup.

The HTML lot selector switches all charts between All lots and individual lots.
Parts merge by valid wafer + positive PRR X/Y, falling back as a complete tuple
to lot + positive integer PTR X/Y. Resolved identities merge across source files,
head/site and PART_ID; unresolved attempts remain source/sequence scoped. A run
status banner warns when a failed update leaves older successful data selected.
See [coordinate identity](coordinate_identity.md) for validation, name matching,
ambiguous candidates, and the required eav-v1 to eav-v2 reconversion.

`dashboard-dir` accepts `--memory-limit-mib` (256), `--max-lots` (32), and
`--max-parts` (100000). Its conservative budget includes both all-lot and per-lot
state; roughly 30,000 short rows fit the default budget. Exceeding a limit or
detecting corruption leaves existing HTML unchanged. These are accounted limits,
not hard RSS guarantees; disk-backed large-data analytics is the next milestone.
Repeated PART_IDs have independent source-local attempt sequences. Resolved
coordinate identities still use all-pass merging and the first finite result
per test number; first/final-retest policies and program identities are not
implemented. Test-row statistics are not deduplicated by the part merge key.

Repeat the Windows memory smoke test after building the CLI:

```powershell
cargo build -p stdf-cli
.\scripts\measure_partition_memory.ps1
```

## Python Packaging

The active Python extension crate is `stdf-py`, exposed as module `_zstdf`.

```powershell
maturin develop
```

Then in Python:

```python
import _zstdf

batches = _zstdf.read_batches("input.stdf")
rows = _zstdf.write_parquet("input.stdf", "output.parquet")
rows = _zstdf.write_parquet("input.stdf", "output.parquet", overwrite=False)

files, rows = _zstdf.write_parquet_many(
    ["input1.stdf", "input2.stdf"], "dataset",
    partition_by=["lot_id", "wafer_id"],
    batch_size=65536, overwrite=False,
)
```

`write_parquet_many` accepts file paths (directory discovery is a CLI feature),
releases the Python GIL during conversion, and returns `(files, rows)`.
Omitting `partition_by` uses `input-file`; an empty list writes without partition
directories. Existing Python functions keep their signatures.

For bounded row-level partitioning (also releases the GIL):

```python
files, rows, fragments = _zstdf.write_parquet_partitioned(
    ["input.stdf.gz"], "dataset",
    partition_by=["lot_id", "wafer_id"],
    memory_limit_mib=256, max_pending_tests=100000,
    max_open_writers=4, row_group_rows=65536, max_output_files=10000,
)
```
