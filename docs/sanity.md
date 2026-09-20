# CP/FT source sanity

`sanity` reads STDF files, directories and gzip inputs directly. It scans every
record, checks CSV-selected fields, displays important run metadata, and offers compact per-unit previews.
It does not change Dashboard yield or the `traceability` command.

## Optional records and default exclusions

ATR, CDR, ATER, CTSR and CTRR are optional records. If absent, they have no
placeholder cells or missing-record findings. If present, their values are
extracted and displayed. Their CSV rows are commented out by default, so their
fields appear as **Not checked** and stay out of TXT field totals. Uncomment
individual rows to enable checks. Framing and structural decoding errors always
remain diagnostics. ATER/CTRR retain explicit head/site ownership.
See [record formats, compatibility and demo](vendor-records.md).

## CSV field checklist

[config/sanity-checks.csv](../config/sanity-checks.csv) lists the supported body
fields in wire order, including disabled fields as commented rows. A copy is
embedded at build time for execution from any working directory. Pass an edited
copy explicitly; files in the current directory are never loaded implicitly.

```powershell
Copy-Item .\config\sanity-checks.csv .\my-checks.csv
.\target\release\zstdf-cli.exe sanity C:\data\sample.stdf.gz `
  --test-domain ft --checks-csv .\my-checks.csv `
  --output-dir .\reports\sample
```

The four columns, in order, are `record,field,flow,format`:

```csv
record,field,flow,format
MRR,FINISH_T,CP|FT,U*4
# MRR,DISP_COD,CP|FT,C*1
# MRR,USR_DESC,CP|FT,C*n
# MRR,EXC_DESC,CP|FT,C*n
# ATR,MOD_TIM,CP|FT,U*4
# CDR,CHN_NAM,CP|FT,C*n
# CTSR,CHAR_NAM,CP|FT,C*n
# CTSR,AXES[].RNG_RESO,CP|FT,R*8
```

- Active rows require a usable effective value whenever the record appears.
  Missing, blank, invalid or unresolved selected values produce a finding and
  exit 1. A valid inherited value satisfies the check and retains its provenance.
  Selecting a result also requires a usable result from definition-only or
  not-executed PTR/MPR records; disable that field if such records are expected.
- A `#` at the start of a line disables it. Disabled fields still retain their
  raw/effective evidence, but have status `not_checked` and no field or profile
  findings. An empty checklist (header only) disables semantic field checks.
- `CP`, `FT`, and `CP|FT` select flows. Mixed-input runs use the profile selected
  for each MIR. FAR/ATR precede MIR and must use `CP|FT`. Before a run is known,
  only shared `CP|FT` checks execute; unresolved routing is itself an error.
- Formats are STDF type labels: `U*1`, `U*2`, `U*4`, `I*1`, `I*2`, `I*4`, `R*4`,
  `R*8`, `C*1`, `C*n`, `B*1`, `B*n`, `D*n`, `N*1`, `V*n`, plus extension `S*n`.
  Arrays use `kx`, e.g. `kxR*4` for MPR.RTN_RSLT. Counts come from the decoded
  wire layout; an encoded zero-count array is present and may be empty. The format must match that layout; it is not a cast or regex.
- CTSR paths use `AXES[]` and `TRACKING[]` for all actual members. Zero-member
  arrays do not invent a member. Enabling a conditional field such as EXE_ORDER,
  RSC_MOD or MARGIN_VAL requires it on every applicable record/member; select
  only fields expected in your shmoo/margin logging variant. Formats for vendor
  fields follow the supplied vendor layout, not a claim of base-v4 membership.
- Active JSON profile rules add naming/range constraints only to CSV-selected
  fields. CSV selection takes precedence. `important_fields` only changes the
  displayed run-field subset; it does not limit checking or Parquet evidence.
- Structural checks, decoding, run/unit closure, configured CP wafer requirements
  and identity resolution remain independent of the CSV. Disabling a field check
  does not make invalid coordinates safe for merging.

Defaults enable core run/part/test identifiers, result fields and relevant CP
wafer fields. MRR enables **FINISH_T only**. The CSV is a configurable engineering
checklist, not a list of every mandatory record in the STDF standard or full
conformance certification. Unknown records/fields, wrong types, bad flows,
duplicate overlapping rows and malformed CSV fail before output replacement.
Files are UTF-8 (including a BOM), LF or CRLF; standard CSV quoting is supported.
Comments must begin in column one. CSV size is limited to 1 MiB.

The HTML **Checks CSV** download and bundle `checks.csv` contain the exact input
snapshot. JSON includes the snapshot, active rows and SHA-256; the manifest and
TXT header include the checklist hash. Raw values and timestamps retain the
existing hover behavior (`[null]` for null; UTC date/time for valid timestamps).

## Installation and execution

Build from the repository root:

```powershell
cargo build -p stdf-cli --release --locked
.\target\release\zstdf-cli.exe sanity --help
```

Use `--test-domain` for a single CP or FT domain and `--run-profiles` for mixed
inputs. Omitting `--profile` selects the built-in base profile. Naming rules in
example profiles apply to synthetic data; adapt them to your actual product.

```powershell
.\target\release\zstdf-cli.exe sanity C:\data\cp `
  --test-domain cp --profile examples\sanity\cp-profile.json `
  --output-dir C:\reports\cp-sanity

.\target\release\zstdf-cli.exe sanity C:\data\ft.stdf.gz `
  --test-domain ft --profile examples\sanity\ft-profile.json `
  --output-dir C:\reports\ft-sanity

Start-Process C:\reports\cp-sanity\report.html
```

On Linux/macOS, use `target/release/zstdf-cli` with the same arguments.

## Text summaries for scripts

Check one file by name and write a UTF-8 text summary without publishing an HTML bundle:

```powershell
.\target\release\zstdf-cli.exe sanity "C:\data\ft\lot-001.stdf.gz" `
  --test-domain ft --text-summary "C:\reports\lot-001.sanity.txt"
```

`--text-summary` and `--output-dir` are mutually exclusive. Text mode accepts the same
files, directories, gzip, profiles, and mixed-run configuration as HTML mode. It checks
**all CSV-selected fields in all scanned records**, not only important fields or the preview's first
two records. The summary lists invalid, missing, and unknown field values, source hashes
and all filename aliases, decompressed record/field offsets, raw/effective values,
presence/origin, and structural/product-profile diagnostics. Values and paths are
JSON-escaped so embedded tabs, newlines, and control characters cannot forge text rows.
A completed file ends with `END scan_complete=...`; incomplete scans never claim full coverage.

Exit code 0 means the configured checks passed. Exit code 1 indicates selected
missing/invalid/unresolved fields, structural validation failure or an operational
error. Argument errors return 2. `--fail-on-missing` remains accepted for existing
scripts, but selected missing fields already fail and disabled fields remain
excluded. Missing fields and product-rule failures are separately identified.

The summary is atomically replaced only after scanning and rendering complete. Corrupt
input can publish an explicitly incomplete diagnostic summary and exit 1. Configuration,
I/O, cancellation, or resource failures preserve the previous summary; a previous file's
existence alone does not establish that the current invocation succeeded. Input STDF
cannot be used as the output path. Text mode uses temporary raw/Parquet evidence beneath
the output parent and removes it on normal completion/failure. Existing disk and report
budgets apply; exceeding them fails rather than truncating the summary.

For a directory, this PowerShell example produces one text file per input and preserves
relative subdirectories to avoid duplicate-basename collisions:

```powershell
$inputRoot = (Resolve-Path "C:\data\ft").Path.TrimEnd('\')
$outputRoot = "C:\reports\ft-sanity"
$failed = 0
Get-ChildItem -LiteralPath $inputRoot -Recurse -File |
  Where-Object { $_.Name -match '\.(stdf|std)(\.gz)?$' } |
  ForEach-Object {
    $relative = $_.FullName.Substring($inputRoot.Length + 1)
    $summary = Join-Path $outputRoot ($relative + ".sanity.txt")
    & .\target\release\zstdf-cli.exe sanity $_.FullName `
      --test-domain ft --text-summary $summary
    if ($LASTEXITCODE -ne 0) {
      $failed++
      Write-Warning "Sanity check failed: $($_.FullName)"
    }
  }
if ($failed -gt 0) { throw "$failed STDF checks failed; review summaries and command errors." }
```

Use `--test-domain cp` for CP data or the appropriate `--run-profiles` configuration
for mixed runs. On Linux/macOS, use `target/release/zstdf-cli` with the same arguments.

## Runnable synthetic examples

```powershell
python examples\sanity\generate_demo.py
.\target\release\zstdf-cli.exe sanity examples\sanity\generated\cp.stdf `
  --test-domain cp --profile examples\sanity\cp-profile.json `
  --output-dir examples\sanity\generated\cp-report
.\target\release\zstdf-cli.exe sanity examples\sanity\generated\ft.stdf.gz `
  --test-domain ft --profile examples\sanity\ft-profile.json `
  --output-dir examples\sanity\generated\ft-report
Start-Process examples\sanity\generated\cp-report\report.html
```

`ft-invalid.stdf` contains NaN in the third PTR of the third unit. It falls outside
the first-two-record preview, but full-file validation must report the error and
return exit code 1. This verifies that previews do not hide later anomalies.

## Report contents

Section 1 groups entries by record type and record offset. Each field or first-value
preview has a compact colored cell: **green valid**, **red invalid**, **grey missing**.
Unknown/empty entries use a dashed grey cell and their own labels; they are never
presented as valid. Hover or keyboard-focus a cell to see only its raw value (UTC date/time for timestamps). Click or press Enter to scroll to and focus the matching row in
section 2, opening its raw evidence. Repeated records retain separate targets.

Section 2 contains the detailed run-field tables, run-level test records, unit selector,
PRR fields, and compact per-type previews. Selecting a unit updates its cells in
section 1. Section 3 retains full-file diagnostics. Product-profile failures on valid
fields appear red with the underlying semantic status preserved. Green describes
field validity under implemented checks, not the device Pass/Fail outcome or complete
standards certification. Entry names sit inside flat, background-colored cells in a compact spreadsheet-style grid.
Record types label the rows; cells wrap onto additional rows as needed.
The default MIR summary includes USER_TXT, SETUP_T, and START_T. Valid timestamp fields
show their original integer alongside `YYYY-MM-DD HH:mm:ss UTC` in details.
Section 1 tooltips show only the raw value, with timestamps converted to UTC;
they do not include status, processed values, or the full evidence JSON.
A null raw value displays as `[null]`, even when an effective value was inherited.
STDF v4 calls the run/wafer end timestamp FINISH_T; MOD_TIM is also recognized, as is
END_T if supplied by a supported record. Missing/invalid timestamps show no fabricated
epoch date. Durations such as TEST_T and TEST_TIM are never converted to calendar dates.
Raw JSON and Parquet values remain unchanged.

- **Per run**: FAR CPU_TYPE/STDF_VER; MIR lot/sublot, product, program/revision,
  step, operator, temperature, times, STAT_NUM, MODE_COD, RTST_COD, DATE_COD,
  facility/floor/process; SDR head/site, SITE_CNT, and hardware/EXTR_ID; applicable
  WIR/WRR/WCR, SBR/HBR, and MRR/PCR. Preserve every context record within the run.
- **Per unit**: independently list PART_ID, wafer, coordinate merge key, and PRR
  verdict/bins for each test attempt. Take the first two records of each type:
  DTR text, PTR.RESULT, MPR.RTN_RSLT[0], valid FTR verdict, and the first non-padding
  GDR element. Do not pad zero/one-record cases or skip invalid first values.
- **Full-file diagnostics**: field boundaries, array lengths, nonfinite values,
  character encoding, selected missing-value/validity rules, CP wafer association,
  run/unit/wafer/program-section closure, and configured product formats.
  Interleaved sites are tracked independently.
- **Offline interaction**: run selection, unit search/pagination/selection,
  diagnostic filtering, and complete summary JSON export.

First values display their numbers and known units directly; FTR shows
PASS/FAIL/Unknown. Floating-point bits, source offsets, and inheritance information
are available under expandable **Raw evidence**. R*8 displays preserve the original
decimal string without conversion to JavaScript Number and subsequent rounding.
Display formatting does not affect exported JSON or Parquet evidence. The run
selector shows CP/FT and profile ID; the header reports warnings to review.

`explicit` means a value was written into the file; it does not prove manual entry.
`standard_default`, `inherited`, and `unresolved` are stored separately from validity.
The status beside an MPR first value applies to the entire result field, so invalid
later elements also flag it. DTR/GDR are associated with a unit only when exactly
one unit is clearly open; ambiguous multisite records stay at run level.

## Evidence and publication

The root `report.html` is the only publication entry point. It pins an immutable
`sanity-*` generation. Each generation contains:

| File | Contents |
| --- | --- |
| `summary.json` | Complete run/unit summaries, diagnostics, profile/hash, and scan completeness |
| `record_fields.parquet` | One row per field: source, record offset/type, field name/type, byte range, and `evidence_json` |
| `<sha256>.stdf` | Decompressed original evidence; identical contents stored once, with all input paths retained in the summary |
| `manifest.json` | SHA-256 and size of files in the same generation |
| `report.html` | Offline report copy for that generation |

Parquet `byte_start` is relative to the **start of the record header**; add
`record_offset` to obtain the original file position. `evidence_json` stores raw
and effective values, field states, and provenance. Arrays retain every element;
R*4/R*8 retain hexadecimal raw bits to avoid JSON/JavaScript precision loss.
Recover exact malformed-character bytes from the original evidence, not from
replacement characters in decoded text. Header fields are also included.

Completed diagnostics that identify data problems publish a failed-status report
and return exit code 1. Corrupt inputs may produce `scan_complete=false`; unscanned
trailing data is not declared valid. File I/O failures, resource limits, and
cancellation preserve the existing report. Older generations remain available for
review. The disk budget covers only the new generation and its atomic report copy,
not previously published generations.

## Profiles and resource parameters

Profiles use a strict JSON schema: `version: 1`, a nonempty `id`, and
`domain: cp|ft`, with configurable `require_wafer`, `rules`, and `important_fields`.
Misspelled fields, unknown parameters, and invalid regular expressions are rejected.
Each rule has `record`, `field`, and optional `required`, `pattern`, `allowed`,
`min`, and `max`. `pattern` matches the entire string; ranges apply to effective
numeric values. Rules evaluate records that actually occur; they do not require
arbitrary record types to exist. Structural rules handle run/MIR/MRR and unit
closure, and CP wafer requirements.

Enable `MIR,JOB_REV,CP|FT,C*n` in the CSV for the rule below.
For example, display selected MIR fields without changing full-field inspection/export:

```json
{
  "version": 1,
  "id": "my-ft-v1",
  "domain": "ft",
  "important_fields": {"MIR": ["LOT_ID", "JOB_NAM", "JOB_REV", "TST_TEMP"]},
  "rules": [{"record": "MIR", "field": "JOB_REV", "required": true}]
}
```

| Parameter | Default | Purpose |
| --- | --- | --- |
| `--checks-csv` | Embedded default CSV | Select required fields by CP/FT flow |
| `--preview-records-per-type` | 2 | Preview records per type in each unit; range 1–100 |
| `--max-report-mib` | 32 | Retained-summary budget and serialized-report limit; not a hard process RSS limit |
| `--disk-limit-mib` | 1024 | Write budget for new raw evidence, Parquet, summary, and publication copies |
| `--max-units` | 100000 | Test-attempt limit; exceeding it fails without truncation |
| `--max-sources` | 10000 | Input discovery limit, applied before content deduplication |
| `--cancel-file` | None | File presence requests cooperative cancellation; checked again before publication |

## Mixed CP/FT and per-run configuration

```powershell
.\target\release\zstdf-cli.exe sanity `
  examples\sanity\generated\cp.stdf examples\sanity\generated\ft.stdf `
  --run-profiles examples\sanity\run-profiles.json `
  --output-dir examples\sanity\generated\mixed-report
```

`--run-profiles` conflicts with `--test-domain` and `--profile`. The configuration
contains `version: 1`, `id`, and `routes`. Each route embeds a complete CP/FT
`profile`; `matches` lists candidate conditions. Fields within a condition use
AND; multiple conditions use OR. Each MIR must match exactly one route.
The example matches MIR.JOB_NAM and JOB_REV exactly; it does not infer CP/FT from
filenames or the presence of WIR.

Conditions may include a `mir` field-value object, `source_id` (lowercase SHA-256
of decompressed content), and `mir_offset` (a decimal **string** representing the
decompressed MIR record offset). Each run saves its actual profile ID/hash; the
report also stores the routing configuration and hash. Unmatched or multiply
matched runs retain raw evidence and units but show domain unknown, skip product
rules, and publish an explicitly failed diagnostic report with a nonzero exit code.
Malformed configuration is rejected before publication, preserving the old report.

## Current coverage boundary

This is a runnable **initial 10D.4 implementation**, not complete standards
conformance certification or the completed 10D.5 storage optimization. It covers
25 base-v4 record layouts, raw evidence, important-field previews, and the rules
above. Unknown/vendor/v4-2007 records retain their raw bytes and mark the check
failed/unsupported. Complete standard enums/count reconciliation, FTR definition
inheritance, the full rule-coverage inventory, and large-scale RSS/recovery acceptance
remain unimplemented.

PTR/MPR defaults are scoped by source + MIR run + record type + TEST_NUM and use
the first definition; subsequent overrides affect only the current record.
Scales, limits, MPR input parameters/indices, units, and display formats preserve
provenance. Invalid or absent first definitions remain unknown. Explicit absence
of a lower/upper limit takes precedence over a default request; a one-byte NUL
string explicitly clears default text. First definitions marked not executed can
remain at run level without counting as unit test records.

Field values currently reside in the Parquet JSON evidence column and original
STDF, with bounded batches, Snappy, and dictionary encoding disabled. Existing
EAV column types are unchanged; this does not establish completed per-column
compression benchmarks or measured space savings.

## Validation

```powershell
cargo fmt --all --check
cargo test --workspace --exclude stdf-py --locked --offline
cargo check -p stdf-py --locked --offline
cargo build -p stdf-cli --release --locked --offline
# With Playwright and Edge already installed locally:
node scripts\smoke_sanity.cjs examples\sanity\generated\cp-report\report.html
```

Rule reference: [Teradyne STDF v4 specification](https://storage.googleapis.com/google-code-archive-downloads/v2/code.google.com/stdf-eclipse/Stdf-V4-spec.pdf).
See [Phase 10D.4–10D.5](../PHASED_EXECUTION_SPEC.md) for the complete remaining deliverables.


Raw value is the value decoded from the source bytes (or null when omitted).
Effective value is the value after defaults/inheritance and validity rules have
been applied. They usually agree for ordinary explicit valid fields. An omitted
limit may have a null raw value but inherit an effective limit from its earlier
definition; a sentinel or invalid result can retain its raw value while its
effective value is null. The section 1 tooltip uses the raw value in both cases.
Full evidence remains available by clicking a cell to open its detail row.

## Display settings

The report includes top-right selectors for five styles and five languages.
Switching preserves filters and selected-device details; original field values,
identifiers, diagnostic evidence, and JSON exports remain unchanged. See
[shared report display settings](report-ui.md). Regenerate older HTML reports
with the updated CLI to receive these controls.
