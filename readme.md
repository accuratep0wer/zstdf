# zstdf

Licensed under [Apache-2.0](LICENSE). See [License and commercial use](#license-and-commercial-use).

Rust tools for reading semiconductor Standard Test Data Format (STDF) files,
exporting Parquet data, validating records, and generating interactive HTML
dashboards. An optional Python extension exposes the conversion APIs as `_zstdf`.

The independent `traceability` command reads STDF files/directories/gzip directly
and reports device test histories, retest verdict/bin differences, missing steps,
and source evidence in an offline HTML report. See [traceability usage and demo](docs/traceability.md).

The `sanity` command checks CP/FT source records and shows important run fields
plus the first value of the first two records per type in each unit. It exports
field evidence and an offline report. See [CP/FT sanity installation, execution, and examples](docs/sanity.md)
for runnable commands, profile configuration, and the current validation scope.
Quick start: [Run CP/FT Sanity Reports](#run-cpft-sanity-reports).

Dashboard part identity now uses **wafer + positive PRR X/Y**, with an all-or-nothing
fallback to **lot + positive integer PTR X/Y**. New conversions produce `eav-v2`;
reconvert older Parquet/catalog datasets from their source STDF before using the
updated dashboard. See [coordinate identity and migration](docs/coordinate_identity.md).

Build instructions: [Windows](#build-on-windows), [RHEL 9](#build-on-red-hat-enterprise-linux-9),
and [macOS](#build-on-macos). The Linux/macOS commands below are setup guidance;
they have not been build-tested on this Windows development machine.

For device histories and retests, follow [Install and Run Traceability Reports](#install-and-run-traceability-reports)
after building the CLI. This includes a ready-to-run synthetic example and
configuration for your own CP/FT data.

## Build on Windows

These instructions target **64-bit Windows 10/11**. Python is optional for the
command-line tools. Cursor or VS Code can be used as the editor; Microsoft C++
Build Tools are still required for compilation.

### 1. Install Git, Rust, and C++ Build Tools

Run in PowerShell:

```powershell
winget install --id Git.Git -e
winget install --id Rustlang.Rustup -e
```

Install [Microsoft C++ Build Tools](https://learn.microsoft.com/en-us/windows/dev-environment/rust/setup).
In Visual Studio Installer, select **Desktop development with C++** and include:

- MSVC x64/x86 C++ build tools.
- A Windows 10 or Windows 11 SDK.

You do not need to use Visual Studio as your editor. See the
[official Rust installation guide](https://rust-lang.org/tools/install/) for
the Rust installer and Windows prerequisites.

After installation, reopen **Developer PowerShell for Visual Studio** and verify:

```powershell
git --version
rustup default stable-x86_64-pc-windows-msvc
cargo --version
where.exe link
```

The linker path should point to the Microsoft MSVC tools. If no path is returned,
modify the Build Tools installation to add the C++ tools and Windows SDK, then
reopen Developer PowerShell.

### 2. Clone the Repository

If moving from another computer, ensure the changes you need have been committed
and pushed there first. Cloning only retrieves changes available on GitHub.

```powershell
git clone https://github.com/zefangzh/zstdf.git
cd zstdf
```

Run all following build commands from this folder, which contains the top-level
`Cargo.toml`. The first build needs internet access to download dependencies.

### 3. Test and Build the CLI

These commands exclude the optional Python binding from testing:

```powershell
cargo test --workspace --exclude stdf-py --locked
cargo build --release -p stdf-cli --locked
```

The executable is `target\release\zstdf-cli.exe`.

Check that the new command is available:

```powershell
.\target\release\zstdf-cli.exe traceability --help
```

To optionally install the executable into Cargo's bin directory, run
`cargo install --path .\stdf-cli --locked` from the repository root. With
`$HOME\.cargo\bin` on PATH, you can then use `zstdf-cli` from other folders.
Installing this way is optional; the examples below use the release executable
inside the repository.

## Build on Red Hat Enterprise Linux 9

Use Bash on an x86_64 or aarch64 RHEL 9 machine with access to its BaseOS and
AppStream repositories. Install the native compiler and build utilities:

```bash
sudo dnf install -y git gcc gcc-c++ make pkgconf-pkg-config ca-certificates
```

Check that `curl` is available (`curl --version`). If it is missing, install
`curl-minimal` with `sudo dnf install -y curl-minimal`. See Red Hat's
[C/C++ development guide](https://docs.redhat.com/en/documentation/red_hat_enterprise_linux/9/html-single/developing_c_and_cpp_applications_in_rhel_9/developing_c_and_cpp_applications_in_rhel_9)
for compiler setup.

Install current stable Rust for your native architecture using
[rustup](https://rust-lang.org/tools/install/), as your regular user:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/zstdf-rustup-init.sh
sh /tmp/zstdf-rustup-init.sh --default-toolchain stable
. "$HOME/.cargo/env"
rustup default stable
cargo --version
gcc --version
```

Clone, test, and build from the repository root:

```bash
git clone https://github.com/zefangzh/zstdf.git
cd zstdf
cargo test --workspace --exclude stdf-py --locked
cargo build --release -p stdf-cli --locked
./target/release/zstdf-cli --help
```

Python is not needed for these CLI build/test commands. The executable is
`target/release/zstdf-cli`, without an `.exe` suffix. Do not use the Windows
`stable-x86_64-pc-windows-msvc` toolchain on Linux.

## Build on macOS

Use Terminal's native Zsh or Bash on your current macOS release. On Apple
silicon, use an arm64 terminal and matching Python installation; avoid mixing
Rosetta/x86_64 tools with native arm64 tools. On an Intel Mac, use x86_64 tools.

Install [Apple's Command Line Tools](https://developer.apple.com/documentation/xcode/installing-the-command-line-tools/):

```bash
xcode-select --install
```

Finish the installer before continuing. If the tools are already installed,
verify them and update them through Software Update when needed:

```bash
xcode-select -p
clang --version
git --version
uname -m
```

Install Rust with [rustup](https://rust-lang.org/tools/install/). It selects the
native macOS toolchain; the Windows MSVC toolchain is not needed:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/zstdf-rustup-init.sh
sh /tmp/zstdf-rustup-init.sh --default-toolchain stable
. "$HOME/.cargo/env"
rustup default stable
cargo --version
```

Clone, test, and build:

```bash
git clone https://github.com/zefangzh/zstdf.git
cd zstdf
cargo test --workspace --exclude stdf-py --locked
cargo build --release -p stdf-cli --locked
./target/release/zstdf-cli --help
```

The executable is `target/release/zstdf-cli`. Homebrew and Python are optional
for the CLI; only the Python setup below uses Homebrew.

## Run the CLI

### Are features from before Phase 10D still available?

**Yes.** `sanity` and `traceability` are additional, independent entry points. The original CLI commands and Python
conversion interfaces remain available. The following features are implemented; this does not mean all of Phase 10D is complete.

| Feature | Current entry point |
| --- | --- |
| STDF decoding summary, record inspection, and ASCII export | `info`, `dump`, `to-ascii` |
| Existing single-file and batch validation | `check`, `batch-check` |
| Single-file Parquet conversion, manifests, and retry reuse | `convert`, `--no-overwrite` |
| Phase 10A single-file visualization | `dashboard` |
| Phase 10B multi-file conversion and file-level partitioning | `convert-many` |
| Phase 10C.1 row partitioning and conversion within resource budgets | `convert-partitioned` (currently supports PTR) |
| Phase 10C.2 dataset integrity, recovery, and multi-lot visualization | `verify-dataset`, `recover-dataset`, `dashboard-dir` |
| Optional Python conversion interface | `_zstdf`; see [Optional Python Binding](#optional-python-binding) |

The original `STDF → convert → dashboard` and
`STDF → convert-partitioned → verify-dataset → dashboard-dir` workflows remain available.
`sanity` reads STDF directly. Its `record_fields.parquet` output contains field-validation evidence and cannot serve as
the measurement EAV Parquet required by `dashboard`. It neither runs conversion automatically nor changes Dashboard yield definitions.

Legacy Parquet/catalog data must be reconverted from STDF into a new output directory to produce `eav-v2` before use
with the current Dashboard. This compatibility requirement follows the coordinate identity update. Phase 10D.2/10D.3 and 10D.5
remain incomplete; 10D.4 sanity has a runnable implementation but has not passed acceptance for every standard rule.
See the [phased specification](PHASED_EXECUTION_SPEC.md) for detailed status.

### Windows PowerShell

Replace the example paths below with files or directories that exist on your
computer. STDF input supports plain files and gzip compression.

Show available commands and inspect a file:

```powershell
.\target\release\zstdf-cli.exe --help
.\target\release\zstdf-cli.exe info C:\data\input.stdf
```

Convert to Parquet, then generate and open an interactive dashboard:

```powershell
.\target\release\zstdf-cli.exe convert C:\data\input.stdf output.parquet
.\target\release\zstdf-cli.exe dashboard output.parquet dashboard.html
Start-Process .\dashboard.html
```

The dashboard command requires an existing Parquet file. Run conversion
successfully before generating the dashboard.

For bounded, mixed-lot/wafer PTR conversion from a directory:

```powershell
.\target\release\zstdf-cli.exe convert-partitioned --output-dir dataset C:\data\inputs
```

This workflow writes multiple Parquet fragments and supports resource limits.
It currently supports PTR results; MPR/FTR expansion returns an explicit error.
Its memory budget applies to accounted conversion state, not an OS-enforced
resident-memory ceiling. Use `dashboard` for one Parquet file or `dashboard-dir`
for a catalog-backed dataset. See [CLI documentation](docs/cli.md) for resource options, retries, and
dataset-generation handling.

Quote paths containing spaces, for example `"C:\test data\input.stdf"`.

### RHEL 9 and macOS

Use forward-slash paths and the native executable without `.exe`. Replace
`/path/to/input.stdf` and `/path/to/inputs` with actual paths:

```bash
./target/release/zstdf-cli info /path/to/input.stdf
./target/release/zstdf-cli convert /path/to/input.stdf output.parquet
./target/release/zstdf-cli dashboard output.parquet dashboard.html
./target/release/zstdf-cli convert-partitioned --output-dir dataset /path/to/inputs
```

Open the generated dashboard on macOS:

```bash
open dashboard.html
```

On a RHEL graphical desktop with `xdg-open` installed:

```bash
xdg-open dashboard.html
```

On a headless RHEL server, transfer `dashboard.html` to your desktop computer
and open it in a browser. The generated HTML is self-contained; no web server
is required. The same PTR-only and memory-accounting limits described above apply.

## Run CP/FT Sanity Reports

Run these commands from the repository root. First install Rust and the build tools using the platform instructions above.
Reading actual STDF requires only the CLI; Python is used only to generate the synthetic examples below.

### 1. Build and verify the command

```powershell
cargo build --release -p stdf-cli --locked
.\target\release\zstdf-cli.exe sanity --help
```

### 2. Check actual CP or FT data

Replace the input paths with existing files or directories. Multiple inputs, recursive directories, and gzip are supported.
Without `--profile`, the built-in basic rules for the selected CP/FT domain apply.

```powershell
.\target\release\zstdf-cli.exe sanity "D:\STDF\CP" `
  --test-domain cp --output-dir .\reports\cp-sanity
Invoke-Item .\reports\cp-sanity\report.html

.\target\release\zstdf-cli.exe sanity "D:\STDF\FT\sample.stdf.gz" `
  --test-domain ft --output-dir .\reports\ft-sanity
Invoke-Item .\reports\ft-sanity\report.html
```

The basic CP rules require an unambiguous wafer context; FT does not automatically fail when wafer records are absent.
Configure product-specific formats with `--profile .\my-cp-profile.json`.
The lot and program names in the [CP example](examples/sanity/cp-profile.json) and [FT example](examples/sanity/ft-profile.json)
are for synthetic data. Adapt them before using the profiles with product data.

### 3. Run synthetic examples and a mixed CP/FT report

The following uses the Python 3 standard library to generate STDF; the Python binding is not required:

```powershell
python .\examples\sanity\generate_demo.py

.\target\release\zstdf-cli.exe sanity .\examples\sanity\generated\cp.stdf `
  --test-domain cp --profile .\examples\sanity\cp-profile.json `
  --output-dir .\examples\sanity\generated\cp-report

.\target\release\zstdf-cli.exe sanity `
  .\examples\sanity\generated\cp.stdf .\examples\sanity\generated\ft.stdf `
  --run-profiles .\examples\sanity\run-profiles.json `
  --output-dir .\examples\sanity\generated\mixed-report

Invoke-Item .\examples\sanity\generated\mixed-report\report.html
```

The mixed example contains **2 runs and 6 units**. Its 3 unresolved FT identity warnings are expected for this demonstration
and are not reported as format-validation errors. `--run-profiles` is mutually exclusive with `--test-domain` and `--profile`.
Each MIR must match exactly one route. The example uses exact `JOB_NAM` and `JOB_REV` matches;
adapt [run-profiles.json](examples/sanity/run-profiles.json) when using real data.

The upper section shows important MIR/SDR and other fields for each run. Selecting a unit shows the first value of each of
the first two records per DTR/PTR/MPR/FTR/GDR type. All records are checked; raw values, default/inheritance provenance, and diagnostics remain in the evidence.
The browser supports run selection, unit search, diagnostic filters, and complete JSON export without a web service.
First values appear as numbers, units, or PASS/FAIL. Click **Raw evidence** to inspect raw bits, field states, and provenance.
Display formatting does not change raw evidence or JSON exports. The run selector also shows CP/FT and the profile ID.

`--output-dir` is created automatically. Its root `report.html` references an immutable `sanity-*` subdirectory;
copy the entire output directory to share all evidence. Validation errors publish a diagnostic report clearly marked as failed and return exit code 1.
I/O errors, configuration errors, resource-limit failures, or cancellation preserve the previous report. The generated `ft-invalid.stdf` contains a
NaN outside the preview range, which can be used to verify full-file validation.

### 4. Linux/macOS

Follow the platform build instructions above, use the same arguments, and select the native executable path:

```bash
./target/release/zstdf-cli sanity /path/to/cp \
  --test-domain cp --output-dir reports/cp-sanity
```

On macOS, use `open reports/cp-sanity/report.html`; on a Linux desktop, use
`xdg-open reports/cp-sanity/report.html`. Local validation was performed on Windows;
the Linux/macOS commands are instructions for those platforms. See the [Sanity guide](docs/sanity.md) for resource limits and the complete field contract.

## Install and Run Traceability Reports

`traceability` reads multiple STDF files directly, links device steps and retests by wafer/coordinates, and generates an interactive HTML report
that opens offline. Files, directories, and gzip are supported. No prior Parquet conversion, Python installation,
or web service is required. Each PRR is retained as an independent test, and files with identical content are deduplicated automatically.

### 1. Install and build on Windows

First install Git, Rust, MSVC C++ Build Tools, and the Windows SDK following the
[Windows build instructions](#build-on-windows) above. Then run these commands in Developer PowerShell:

```powershell
git clone https://github.com/zefangzh/zstdf.git
cd zstdf
cargo test --workspace --exclude stdf-py --locked
cargo build --release -p stdf-cli --locked
.\target\release\zstdf-cli.exe traceability --help
```

If you have already cloned the repository, run the build command from its existing root; do not clone it again.
The first build needs network access to download dependencies. Once dependencies are cached, you can add `--offline` to Cargo commands.
Building the CLI from source does not require the Python binding.

### 2. Run the included example

Run these PowerShell commands from the repository root. The example STDF files are included, so Python is not required:

```powershell
.\target\release\zstdf-cli.exe traceability `
  .\examples\traceability\generated\inputs `
  --flow-config .\examples\traceability\flow.json `
  --flow-closures .\examples\traceability\closures.json `
  --identity-map .\examples\traceability\identities.json `
  --output .\trace.html

Invoke-Item .\trace.html
```

Each PowerShell backtick must be the last character on its line, with no trailing spaces. Alternatively, combine the command into
one line. A successful run produces **10 devices and 9 independent STDF files**; a gzip duplicate does not increase the retest count.
The example steps `stage-a`, `stage-b`, and `stage-c` are demonstration settings, not an actual product flow.

In the browser, filter by wafer/lot, X/Y coordinates, step, and anomaly type. Click a matrix cell to inspect
every attempt for each step, program/hardware details, source files, SHA-256 hashes, and record positions. Retest verdict/bin changes and
missing steps are highlighted with text, icons, and colors. The full JSON evidence export includes all data regardless of the current filters.

### 3. Configure a flow for actual CP/FT data

Copy and edit the example configuration first; choose any suitable destination filename:

```powershell
Copy-Item .\examples\traceability\flow.json .\flow.json
notepad .\flow.json
```

Edit the flow `flow_id`, `version`, ordered `steps`, and each step's `required`, `stop_on_fail`,
and `matches`. Matching fields come from STDF MIR: `job_nam`, `job_rev`, `test_cod`, `flow_id`, and
`tst_temp`. Fields use **case-sensitive exact matching**: fields within a condition are ANDed, and condition groups are ORed.
Different program versions can map to the same step. Unmatched records or records matching multiple steps are shown as unidentified steps.

To inspect the actual MIR fields, export the records as text first:

```powershell
.\target\release\zstdf-cli.exe to-ascii "D:\STDF\CP\sample.stdf" .\sample-records.txt
notepad .\sample-records.txt
```

Replace these paths with existing data directories or files, then generate the report:

```powershell
.\target\release\zstdf-cli.exe traceability `
  "D:\STDF\CP" "D:\STDF\FT" `
  --flow-config .\flow.json `
  --output .\trace.html

Invoke-Item .\trace.html
```

Inputs can mix multiple files and directories. Quote paths containing spaces. The output directory must already exist.
A subsequent successful run atomically replaces the report at the same path. Input errors, resource-limit failures, or cooperative cancellation preserve the existing report.

### 4. Optional: explicitly close flows and map device identities

- `--flow-closures closures.json`: specify a flow ID/version and a lot or complete device identity to declare the flow
  closed. Until then, required trailing steps not yet reached appear as pending; gaps in required earlier steps
  with observed later steps can be marked missing. An STDF MRR does not close the manufacturing flow.
- `--identity-map identities.json`: explicitly map complete lot/PTR keys to wafer/PRR keys.
  Equal coordinates alone never trigger cross-wafer/lot association. Unmapped fallback identities display an association limitation.

After editing these two optional configuration files, run:

```powershell
.\target\release\zstdf-cli.exe traceability `
  "D:\STDF\CP" "D:\STDF\FT" `
  --flow-config .\flow.json `
  --flow-closures .\closures.json `
  --identity-map .\identities.json `
  --output .\trace.html
```

See the configuration examples for [flows](examples/traceability/flow.json),
[closures](examples/traceability/closures.json), and [identity mappings](examples/traceability/identities.json).
Adapt them to the actual product; do not apply demonstration closures or mappings directly to real data.

### 5. Linux/macOS and regenerating synthetic data

After completing the platform build instructions above, run the example from the repository root:

```bash
./target/release/zstdf-cli traceability examples/traceability/generated/inputs \
  --flow-config examples/traceability/flow.json \
  --flow-closures examples/traceability/closures.json \
  --identity-map examples/traceability/identities.json \
  --output trace.html
```

On macOS, open it with `open trace.html`; on a Linux desktop, use `xdg-open trace.html`.
On a server without a desktop, copy the HTML to a local browser. These native command examples have not been validated on Linux/macOS;
the actual build and browser validation were performed on Windows.

To regenerate the synthetic STDF and demonstration report, use the optional Python 3 script:

```powershell
python .\examples\traceability\generate_demo.py --cli .\target\release\zstdf-cli.exe
Invoke-Item .\examples\traceability\generated\demo.html
```

See the [traceability report guide](docs/traceability.md) for complete decision rules, resource budgets, and evidence fields. Local validation included
291 Rust tests, a Release build, Python binding compilation, and browser interaction at desktop/mobile sizes.
These tests used synthetic STDF; actual product flows and tester data require separate validation.

## Command and Parameter Reference

Below, `zstdf-cli` means `.\target\release\zstdf-cli.exe` on Windows or
`./target/release/zstdf-cli` on Linux/macOS. Alternatively, run commands from
the repository root with `cargo run -p stdf-cli -- <command> <arguments>`.
Angle brackets mark required values; square brackets mark optional arguments.
Do not type the brackets. Quote paths and titles containing spaces.

Use `zstdf-cli --help` to list commands, `zstdf-cli --version` for the version,
and `zstdf-cli <command> --help` for that command's options. Boolean flags are
off unless supplied; use `--no-overwrite`, not `--no-overwrite true`.

### `sanity`: Inspect CP/FT Fields and Compact Unit Previews

```text
zstdf-cli sanity <inputs>... --test-domain cp|ft --output-dir DIR [--profile FILE]
zstdf-cli sanity <inputs>... --run-profiles FILE --output-dir DIR
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `inputs` | Required, one or more | STDF files/directories; gzip supported. |
| `--test-domain cp\|ft` | Required unless `--run-profiles` is supplied | One domain for all input runs. |
| `--profile FILE` | Built-in domain profile | Product rules for the selected domain. |
| `--run-profiles FILE` | None | Unique per-MIR route selection for mixed CP/FT; conflicts with the two options above. |
| `--output-dir DIR` | Required | Bundle directory; open its root `report.html`. |
| `--preview-records-per-type N` | `2` | First N records per type in each unit, range 1–100; does not limit full-file checking. |
| `--max-report-mib N` | `32` | Retained-summary budget and serialized report limit, not a hard RSS cap. |
| `--disk-limit-mib N` | `1024` | Write budget for this generation, including evidence and atomic report copy. |
| `--max-units N` | `100000` | Maximum independent unit attempts; exceeding it fails without truncation. |
| `--max-sources N` | `10000` | Input-path limit before content deduplication. |
| `--cancel-file FILE` | None | Creating this file requests cooperative cancellation before publication. |

### `traceability`: Trace Steps and Retest Differences

```text
zstdf-cli traceability <inputs>... --flow-config flow.json --output trace.html [options]
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `inputs` | Required, one or more | STDF files/directories; gzip supported. |
| `--flow-config FILE` | Required | Flow ID/version, ordered steps and exact MIR selectors. |
| `--output FILE` | Required | Self-contained HTML report; parent directory must exist. |
| `--flow-closures FILE` | None | Explicit lot/device closures for the configured flow ID/version. |
| `--identity-map FILE` | None | Full lot/PTR-to-wafer/PRR identity mappings; conflicts fail. |
| `--memory-limit-mib N` | `256` | Accounted working-data budget, minimum 16 MiB; not a hard RSS cap. |
| `--disk-limit-mib N` | `1024` | Live sort scratch plus reserved report staging; must exceed report limit. |
| `--max-report-mib N` | `32` | Maximum complete HTML size; positive and at most memory limit / 4. |
| `--max-devices N` | `100000` | Maximum reported devices; no silent truncation. |
| `--max-sources N` | `10000` | Maximum source paths, including duplicate-content aliases. |
| `--temp-dir DIR` | OS temporary directory | Existing directory for bounded sort scratch. |
| `--cancel-file FILE` | None | Creating this file requests cooperative cancellation; checked before commit. |

Unknown ordering does not imply a final result. The report retains every PRR
attempt and highlights verdict/bin differences across all attempts. See
[traceability rules and limitations](docs/traceability.md) for missing/pending,
failure stops, identity fallback and cancellation behavior.

### `info`: Inspect a File

```text
zstdf-cli info <input>
```

| Parameter | Meaning |
| --- | --- |
| `input` | Required STDF file path; plain or gzip-compressed. |

Prints byte order, record counts, bytes consumed, truncation status, and decode
errors. This is an inspection command: reported decode errors do not themselves
make its exit status fail. Use `check` for validation in automation.

### `dump`: Print Decoded Records

```text
zstdf-cli dump <input> [--limit N]
zstdf-cli dump input.stdf --limit 20
zstdf-cli dump input.stdf > records.txt
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `input` | Required | STDF input file. |
| `--limit N` | No limit | Maximum decoded records to print, not bytes or test results. `0` prints none. |

Writes compact record text to standard output. There is no output-path argument;
use shell redirection as shown, or `to-ascii` for reference-style formatting.

### `check`: Validate STDF Structure

```text
zstdf-cli check <input>
```

| Parameter | Meaning |
| --- | --- |
| `input` | Required STDF file to decode and validate. |

Prints a summary and findings, including severity, rule, record, and offset.
Checks include record pairing, lot metadata, abort detection, PTR limits/results,
test-name consistency, and hardware-bin counts. Validation errors produce a
nonzero exit status.

### `to-ascii`: Export Reference-Style Text

```text
zstdf-cli to-ascii <input> [output] [--debug]
zstdf-cli to-ascii input.stdf decoded.txt
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `input` | Required | STDF file to export. |
| `output` | Input path with its last extension replaced by `.txt` | Destination text file. `input.stdf` becomes `input.txt`; `input.stdf.gz` becomes `input.stdf.txt`. |
| `--debug` | Off | Accepted for legacy compatibility; currently does not change output or enable extra logging. |

Includes reference-style record formatting, header/footer, scales, timestamps,
and generic data formatting. Existing output is overwritten; its parent directory
must already exist. The complete text is assembled in memory before writing,
so this command is not a bounded-memory export.

### `batch-check`: Validate a Directory

```text
zstdf-cli batch-check <root-dir> [report-path] [--threads N]
zstdf-cli batch-check inputs reports.txt --threads 2
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `root-dir` | Required | Directory to scan recursively for `.std`, `.stdf`, `.std.gz`, and `.stdf.gz` files, case-insensitively. Symbolic-link entries are skipped. |
| `report-path` | `<root-dir>/sanity_report.txt` | Consolidated text report; an existing report is overwritten. |
| `--threads N` | `1` | Parallel validation workers. `0` is treated as `1`, not automatic CPU detection. |

Prints file/failure counts and the report path. Any failed input produces a
nonzero exit status. More workers can increase memory usage, especially for gzip
inputs; use one worker when diagnosing failures or working with limited RAM.

### `convert`: Produce One Parquet File

```text
zstdf-cli convert <input> <output> [--batch-size N] [--no-overwrite]
zstdf-cli convert input.stdf output.parquet --batch-size 4096 --no-overwrite
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `input` | Required | Source STDF file. |
| `output` | Required | Destination long-format EAV Parquet file. |
| `--batch-size N` | `65536` | Target EAV rows per Arrow batch; `0` becomes `1`. Part completion can exceed this target, so it is not a hard memory limit. |
| `--no-overwrite` | Off | If output exists, return its existing manifest summary instead of replacing it. |

Writes a companion `<output>.json` manifest and prints batch/row counts. By
default an existing output may be replaced. The no-overwrite fast path requires
a readable manifest but does not reopen the input or perform full integrity
verification. It is not proof that an existing output matches a changed source.

### `convert-many`: File-Level Partitioning

```text
zstdf-cli convert-many <inputs>... --output-dir <dir> [--partition-by KEYS] [--batch-size N] [--no-overwrite]
zstdf-cli convert-many inputs --output-dir dataset --partition-by lot-id,wafer-id
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `inputs` | Required, one or more | Space-separated files or recursively scanned directories. Directory scanning uses the STDF extensions listed under `batch-check`. |
| `--output-dir DIR` | Required | Root directory for generated Parquet files and manifests. |
| `--partition-by KEYS` | `input-file` | Comma-separated `input-file`, `lot-id`, or `wafer-id` keys, in directory nesting order. |
| `--batch-size N` | `65536` | Target EAV rows per batch, not a RAM cap. |
| `--no-overwrite` | Off | Reuse matching existing output only after source/manifest and Parquet metadata checks. The source must remain available. |

Canonical input paths are deduplicated. This produces one Parquet file per source;
selected lot/wafer keys must be consistent within each source. Use
`convert-partitioned` for a source containing multiple lots or wafers. Output names
include source identity information; changed source content can create new files.

### `convert-partitioned`: Bounded Dataset Conversion

```text
zstdf-cli convert-partitioned <inputs>... --output-dir <dir> [options]
zstdf-cli convert-partitioned inputs --output-dir dataset --memory-limit-mib 256 --row-group-rows 4096
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `inputs` | Required, one or more | STDF files or directories to scan recursively. |
| `--output-dir DIR` | Required | Catalog-backed dataset root containing Parquet fragments and `_catalog.json`. |
| `--partition-by KEYS` | `lot-id,wafer-id` | Comma-separated partition keys: `input-file`, `lot-id`, `wafer-id`. Routes rows rather than entire input files. |
| `--memory-limit-mib N` | `256` | Budget for accounted conversion state, in MiB (1,048,576 bytes). Not a hard process RSS limit. |
| `--max-pending-tests N` | `100000` | Maximum pending test results across unfinished parts. |
| `--max-open-writers N` | `4` | Maximum simultaneously open fragment writers. |
| `--row-group-rows N` | `65536` | Maximum rows per row group and fragment. Smaller values can create more output files. |
| `--max-output-files N` | `10000` | Maximum generated fragments for an invocation; also affects reserved metadata memory. |
| `--continue-on-error` | Off | Continue processing other inputs after an input fails, rather than stopping at the first failure. Any failures still result in a nonzero CLI exit status. |

Resource limits must be positive. Currently supports PTR results; unsupported
MPR/FTR expansion is an explicit error. Prints rows, fragments, writer/memory
peaks, and failures. Inspect `_catalog.json` for run status and failed inputs.
This command has no `--no-overwrite` option; dataset identity and current versions
are managed through its catalog.

### `verify-dataset`: Check Dataset Integrity

```text
zstdf-cli verify-dataset <input>
zstdf-cli verify-dataset dataset
```

| Parameter | Meaning |
| --- | --- |
| `input` | Required catalog-backed dataset root, not an individual Parquet file. |

Verifies current snapshot paths, hashes, schemas, and row counts. Prints source
count, catalog revision, and run status; verification failures return nonzero.

### `recover-dataset`: Clean Interrupted Work

```text
zstdf-cli recover-dataset <input>
zstdf-cli recover-dataset dataset
```

| Parameter | Meaning |
| --- | --- |
| `input` | Required dataset root to recover after an interrupted conversion. |

This command **modifies the dataset**: it removes abandoned `.staging-*`
directories, recoverable stale writer markers, and temporary catalog files under
the managed paths. It checks writer ownership/locks and refuses active writers.
A catalog run left as `running` is marked `interrupted`. It does not delete
completed generations or reconstruct missing/corrupt committed Parquet files.
Run `verify-dataset` afterward before consuming the data.

### `dashboard`: Visualize One Parquet File

```text
zstdf-cli dashboard <input> <output> [--title TEXT] [--max-correlation-tests N]
zstdf-cli dashboard output.parquet dashboard.html --title "Lot DataView" --max-correlation-tests 24
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `input` | Required | Existing EAV Parquet file, such as output from `convert`; not an STDF file or dataset directory. |
| `output` | Required | Destination self-contained interactive HTML file. |
| `--title TEXT` | `zstdf DataView` | Dashboard title; quote text containing spaces. |
| `--max-correlation-tests N` | `16` | Maximum numeric tests considered for correlation analysis. Values below `2` become `2`. Does not limit all input rows/tests loaded. |

Prints row/part counts and yield percentage. This single-file command does not
provide a memory-limit flag. Open the generated HTML in a browser as described
in the platform examples above.

### `dashboard-dir`: Visualize a Catalog Dataset

```text
zstdf-cli dashboard-dir <input> <output> [--title TEXT] [--memory-limit-mib N] [--max-lots N] [--max-parts N]
zstdf-cli dashboard-dir dataset dashboard.html --title "Dataset DataView" --max-lots 64
```

| Parameter | Default | Meaning |
| --- | --- | --- |
| `input` | Required | Catalog-backed dataset root from `convert-partitioned`, not an arbitrary folder of Parquet files. |
| `output` | Required | Destination self-contained HTML dashboard with lot selection. |
| `--title TEXT` | `zstdf Dataset DataView` | Dashboard title. |
| `--memory-limit-mib N` | `256` | Accounted analysis-memory budget in MiB; minimum `1`. Not a hard RSS cap. |
| `--max-lots N` | `32` | Maximum distinct lots allowed in the analysis; must be positive. |
| `--max-parts N` | `100000` | Maximum distinct parts across the analysis; must be positive. |

Reads current catalog versions rather than mixing historical generations.
Resource limits cause errors instead of silently truncating the analysis.
Unlike `dashboard`, this command has no `--max-correlation-tests` CLI option.

## Optional Python Binding

### Windows

Install **64-bit Python 3.12** with the Windows Python launcher (`py`). The Rust
and C++ build tools above are also required to build the extension from source.

From the repository root, create a virtual environment and install the package:

```powershell
py -3.12 -m venv .venv
$env:PYO3_PYTHON = (Resolve-Path .\.venv\Scripts\python.exe).Path
.\.venv\Scripts\python.exe -m pip install --upgrade pip
.\.venv\Scripts\python.exe -m pip install pyarrow .
.\.venv\Scripts\python.exe -c "import _zstdf; print(_zstdf.__version__)"
```

Using the virtual environment's Python directly avoids needing to activate it.
The installed distribution is named `stdf-rs`; its import name is `_zstdf`.

Example Python usage:

```python
import _zstdf

rows = _zstdf.write_parquet(r"C:\data\input.stdf", "output.parquet")
print(f"Converted {rows} rows")
```

To run the complete Rust workspace tests, including the Python binding, make
the base Python DLL directory available in the current shell:

```powershell
$env:PYO3_PYTHON = (Resolve-Path .\.venv\Scripts\python.exe).Path
$pythonBase = py -3.12 -c "import sys; print(sys.base_prefix)"
$env:Path = "$pythonBase;$env:Path"
cargo test --workspace --locked
```

### RHEL 9

Python 3.12 packages are available starting with **RHEL 9.4**. These optional
commands assume access to a RHEL 9 repository version providing those packages;
they are not required for the CLI on earlier RHEL 9 releases. See Red Hat's
[Python installation guide](https://developers.redhat.com/blog/install-python3-rhel).

```bash
sudo dnf install -y python3.12 python3.12-pip python3.12-devel
python3.12 -m venv .venv
export PYO3_PYTHON="$PWD/.venv/bin/python"
.venv/bin/python -m pip install --upgrade pip
.venv/bin/python -m pip install pyarrow .
.venv/bin/python -c "import _zstdf; print(_zstdf.__version__)"
```

Run these commands from the repository root. If the Python development package
is unavailable, ask your RHEL administrator to enable the appropriate approved
repositories; do not replace the OS-managed Python installation.

### macOS

If you do not already have Python 3.12, install [Homebrew](https://brew.sh/) and
follow its shell/PATH setup instructions. Then install its versioned
[Python 3.12 formula](https://formulae.brew.sh/formula/python%403.12):

```bash
brew install python@3.12
"$(brew --prefix python@3.12)/bin/python3.12" -m venv .venv
export PYO3_PYTHON="$PWD/.venv/bin/python"
.venv/bin/python -m pip install --upgrade pip
.venv/bin/python -m pip install pyarrow .
.venv/bin/python -c "import _zstdf; print(_zstdf.__version__)"
```

Run from the repository root. `brew --prefix` avoids hard-coding different
Homebrew locations on Apple silicon and Intel Macs. The virtual environment
keeps project dependencies separate from Homebrew's managed Python packages.

For Linux/macOS, verify the installed extension through the import smoke test
above and run the CLI-only Rust test command shown in the platform setup.
The binding currently enables PyO3's `extension-module` feature; full Rust
workspace test linking on Unix is not validated by these instructions.

Example conversion on either platform:

```bash
.venv/bin/python -c "import _zstdf; print(_zstdf.write_parquet('/path/to/input.stdf', 'output.parquet'))"
```

### Python Function Parameters

The Python module uses underscores in parameter names. File paths are strings;
`input_paths` and `partition_by` are lists of strings, not comma-separated text.
The multi-file Python functions take explicit file paths, unlike the CLI's
recursive directory expansion.

| Function | Parameters and defaults | Return value |
| --- | --- | --- |
| `read_batches(path, batch_size)` | `path`: STDF source. Pass `None` for the default batch size of `65536`; `0` becomes `1`. | List of PyArrow record batches. All batches are collected in memory; this is not a streaming iterator. |
| `write_parquet(input_path, output_path, batch_size=None, overwrite=None)` | Required source/destination paths. `batch_size=None` means `65536`. `overwrite=None` means `True`; `False` uses the same existing-manifest fast path as CLI `convert --no-overwrite`. | Number of rows. |
| `write_parquet_many(input_paths, output_dir, batch_size=None, overwrite=None, partition_by=None)` | Required source list/destination directory. Batch and overwrite defaults as above. `partition_by=None` means `["input-file"]`; `[]` disables partition directories. | `(files, rows)`. |
| `write_parquet_partitioned(input_paths, output_dir, partition_by=None, memory_limit_mib=256, max_pending_tests=100000, max_open_writers=4, row_group_rows=65536, max_output_files=10000)` | Required source list/dataset root. `partition_by=None` means `["lot-id", "wafer-id"]`; `[]` disables partition directories. Resource parameters have the same meanings as their hyphenated CLI counterparts above. No `overwrite` or `continue_on_error` parameter. | `(files, rows, fragments)`. |

Example with explicit parameters:

```python
import _zstdf

batches = _zstdf.read_batches("input.stdf", 4096)
files, rows, fragments = _zstdf.write_parquet_partitioned(
    ["input.stdf"],
    "dataset",
    partition_by=["lot-id", "wafer-id"],
    memory_limit_mib=256,
    max_open_writers=2,
    row_group_rows=4096,
)
```

## Troubleshooting

| Error | What to check |
| --- | --- |
| `cargo` is not recognized | Reopen the shell after Rust installation. Verify `$HOME\.cargo\bin\cargo.exe` exists and `$HOME\.cargo\bin` is on PATH. |
| `link.exe` not found | Install the MSVC C++ tools and Windows SDK, then use Developer PowerShell. Cursor/VS Code alone does not provide the linker. |
| Could not find `Cargo.toml` | Change directory to the cloned repository root before running Cargo. |
| Python/PyO3 build errors | For CLI-only use, exclude `stdf-py` from workspace tests. For Python use, select the 64-bit Python 3.12 environment with `PYO3_PYTHON` as shown above. |
| File not found when generating a dashboard | Confirm conversion succeeded and supply the actual generated Parquet path. |
| Unknown `convert-partitioned` command | Ensure your checkout includes Phase 10C.1 and rebuild the release CLI. |
| Unknown `traceability` command | Update the checkout to a revision containing traceability, then run `cargo build --release -p stdf-cli --locked`. Use the executable from that checkout. |
| Traceability report shows an unidentified step | Check actual MIR fields against the case, program version, and values in `matches`; avoid matching a record to multiple steps. |
| Trailing step appears as pending | This is normal for an open flow. Supply the corresponding closure list only after confirming that the flow is complete. |
| Fallback identity association is limited / identity is unresolved | Check wafer/PRR coordinates and PTR coordinate candidates; provide complete identity mappings when cross-namespace association is needed. |
| Traceability resource limit exceeded | Adjust the reported limit together with related memory/disk/report constraints, or split inputs by the intended flow population. No partial report is published. |
| `cargo: command not found` on Linux/macOS | Run `. "$HOME/.cargo/env"` or reopen your terminal after rustup installation. |
| `cc` or `clang` not found | On RHEL install GCC/build utilities; on macOS install the Command Line Tools and verify `xcode-select -p`. |
| RHEL cannot find Python 3.12 packages | Check that the enabled repositories provide RHEL 9.4 or later packages. Python is optional for the CLI. |
| Wrong architecture on macOS | Use matching native architectures for the terminal, Rust, Homebrew, and Python. Recreate the virtual environment if its Python architecture is wrong. |

## License and Commercial Use

Copyright 2026 zstdf contributors.

zstdf is licensed under the **Apache License, Version 2.0**. You may use, modify,
and distribute it commercially, including in proprietary products, subject to
the license terms. You do not have to publish your modifications merely because
you use this license. See the complete [LICENSE](LICENSE) and project [NOTICE](NOTICE).

When redistributing the software, include the license, retain applicable
copyright and attribution notices, preserve relevant NOTICE content, and mark
modified files as required by Section 4. The license includes the contributor
patent grant described in Section 3; it does not grant general trademark rights.
The software is provided without warranty, as stated in the license.

This declaration applies to this revision and subsequent revisions carrying it.
It does not revoke permissions already granted for earlier versions. Third-party
dependencies remain under their own licenses; distributing binaries requires
preserving their applicable licenses and notices as well. This repository's
license does not change ownership or licensing of user-supplied STDF/test data.

All active Rust crates declare `Apache-2.0` through workspace inheritance. Cargo
packages carry local copies of LICENSE and NOTICE. Both Python build entries
declare the SPDX expression and include those files using PEP 639; this requires
Maturin 1.9.3 or later. Contribution terms are in [CONTRIBUTING.md](CONTRIBUTING.md).

Validate the declarations and package notice copies with Python 3.11 or later:

```text
python scripts/check_licenses.py
```

## Project Documentation

### Phase 10D Development Tools

The `stdf-analytics` crate currently provides a bounded disk-backed sorting
foundation, not a replacement dashboard engine. Existing dashboard resource
limits are unchanged. Run its regression suite and scratch-storage stress tool:

```text
cargo test -p stdf-analytics --offline
cargo run -p stdf-analytics --bin spill_stress -- 100000
```

`spill_stress [rows]` takes an optional nonnegative record count (default
`100000`). It uses the OS temporary directory, a 1 MiB accounted memory budget,
a 256 MiB scratch-file budget, and merge fan-in 8. It checks stable ordering,
row counts, and cleanup, then prints peak scratch bytes and elapsed milliseconds.
It does not read STDF inputs or modify datasets. Forced termination may leave an
owned `.zstdf-analytics-*` scratch directory; automated recovery is not yet provided.
On Windows, run `powershell -File scripts/measure_analytics_memory.ps1` for the
100,000/1,000,000-record RSS regression check. It builds the runner, preserves logs
under `target`, and checks a 64 MiB peak/16 MiB growth allowance by default.
See the phased spec for dashboard integration and operational validation gates.

- [CLI and Python usage](docs/cli.md)
- [Phased execution spec and implementation status](PHASED_EXECUTION_SPEC.md)
- [STDF project specification](ZSTDF_SPEC.md)
- [Implementation plan](implementation_plan.md)
- [Parquet profiling guide](docs/parquet_profiling.md)

Build artifacts under `target` are generated locally and do not need to be
copied from the original computer.
