# FTR pattern failure Pareto and yield

FTR pattern metrics appear **below the existing Failure Pareto chart and test
table**, within the Dashboard's existing **Pareto** page. `dashboard` and
`dashboard-dir` accept repeatable `--ftr-input` paths for raw STDF files,
directories, and gzip files. There is no separate FTR navigation tab.

The existing `eav-v2` conversion stores PTR measurements and does not carry
`VECT_NAM`, so FTR metrics read the supplied STDF inputs directly. The optional
`ftr-pareto` command remains available for a standalone HTML export.

## Build and run

Run from the repository root in PowerShell:

```powershell
cargo build --release -p stdf-cli --locked

# One file, multiple files, or recursively discovered .stdf/.std/.stdf.gz/.std.gz.
.\target\release\zstdf-cli.exe ftr-pareto C:\data\run.stdf --output patterns.html
.\target\release\zstdf-cli.exe ftr-pareto C:\data\CP C:\data\FT --output patterns.html

# Add FTR metrics under the existing Failure Pareto section.
.\target\release\zstdf-cli.exe dashboard measurements.parquet dashboard.html --ftr-input C:\data\CP --ftr-input C:\data\FT
.\target\release\zstdf-cli.exe dashboard-dir dataset dashboard.html --ftr-input C:\data
```

On Linux/macOS use `./target/release/zstdf-cli` and platform-appropriate paths.
The output parent directory must already exist. Open the HTML in a browser;
there is no server or external JavaScript dependency.

## Calculation policy

| Metric | Definition |
| --- | --- |
| Pattern | Exact complete `VECT_NAM`, case-sensitive; no inferred splitting |
| Attempts | One per FTR, including repeated tests and not-executed records |
| Pass / Fail | Reliable executed FTR with valid `TEST_FLG` pass/fail indication |
| Unknown | Reserved bit 1, unreliable bit 2, timeout bit 3, abort bit 5, or no-indication bit 6 |
| Not executed | Bit 4 is set; takes precedence over the other classifications |
| Alarms | Bit 0 is set; counted independently and may overlap other outcomes |
| Yield | `100 * Pass / (Pass + Fail)`; N/A when the denominator is zero |
| Fail rate | `100 * Fail / (Pass + Fail)` |
| Failure share | Pattern failures / all failures in the selected FTR scope |
| Cumulative share | Running failure share in descending failure-count order |

`NUM_FAIL` is the number of pins with failures, not the number of failed test
attempts. It does not determine the report's fail count. Missing, empty, or
whitespace-only names are grouped separately as `[null] (missing VECT_NAM)`.
Nonblank names retain their original whitespace. No previous pattern name is
inherited: the STDF V4 FTR semi-static defaults start at `PATG_NUM`, after
`VECT_NAM`. See the [STDF V4 specification, FTR pages 55–57](https://storage.googleapis.com/google-code-archive-downloads/v2/code.google.com/stdf-eclipse/Stdf-V4-spec.pdf).

All attempts remain counted; this is neither first-pass nor final-device yield.
Each file is streamed and hashed after decompression. Exact content copies count
once, with all discovered paths retained. Different file contents count separately,
even if they may contain overlapping subsets of results. MIR occurrences separate
run scopes within each source. Head/site and test number remain available in details.

## Report controls and evidence

All report pages use the same [five styles and five languages](report-ui.md).
The integrated Dashboard view is the normal entry point; `ftr-pareto` remains an
optional export utility when a measurement Parquet file is not available.


- Search patterns and filter by lot, program, revision, head/site, and test number.
- The Pareto chart shows the first 30 failing patterns with an explicit count;
  the sortable table lists **all** patterns, including zero failures, 100 per page.
- Click a pattern for per-source/run/program/head/site/test-number counts and
  first/last decompressed record offsets. These offsets bound the group, not a
  contiguous range containing only that pattern.
- Export filtered CSV includes every matching pattern, independent of pagination.
  Cumulative share retains its failure-rank meaning when sorting other columns.
  Potential spreadsheet formula names receive an apostrophe prefix in CSV;
  evidence JSON retains exact names.
- Export evidence JSON includes all source hashes, paths, scoped counts, and
  schema `ftr-patterns-v1`, independent of filters. It is aggregated evidence;
  individual raw FTR records remain in the source STDF.
- Dashboard FTR filters are independent of measurement/dashboard filters. Supply
  the matching STDF population explicitly when comparing these two views.

## Limits and failure behavior

Input records are streamed; raw measurements are not accumulated. Aggregation is
bounded by 50,000 source/run/pattern/lot/program/revision/head/site/test-number
groups by default. Discovery is limited to 4,096 unique paths and 100,000 directory
entries; directory symlinks are not followed during recursive discovery.
Standalone options `--max-groups` and `--max-report-mib` (default 256) control
aggregation cardinality and serialized report size. These are not process RSS
limits; serialization and browser rendering need additional memory. Embedded
dashboards use the default FTR limits, and dataset dashboards also enforce their
existing report-size budget.

Framing/decode errors, malformed FTR fields, or exceeded limits fail the command.
Existing reports are replaced atomically only after successful analysis and
rendering. The report may inspect partial manufacturing runs: it does not perform
PIR/PRR closure, process-step completeness, or source-field sanity validation;
use `sanity` and `traceability` for those checks. A file with no FTR records produces
an explicit empty report, not an inferred 100% yield.

## Synthetic demo and verification

For all reports and browser-test inputs, run `python scripts/generate_report_demos.py`
after building the release CLI. This also prepares the PTR-only dataset needed
by `smoke_report_ui.cjs`. The commands below generate only the FTR example and
its companion single-file Dashboard.

```powershell
python examples/ftr-patterns/generate_demo.py
.\target\release\zstdf-cli.exe ftr-pareto examples/ftr-patterns/generated --output examples/ftr-patterns/generated/report.html
Start-Process examples/ftr-patterns/generated/report.html

cargo test -p stdf-cli ftr_pareto --locked
# Prepare the companion measurement dashboard for browser integration checks:
python examples/sanity/generate_demo.py
.\target\release\zstdf-cli.exe convert examples/sanity/generated/cp.stdf examples/ftr-patterns/generated/cp.parquet
.\target\release\zstdf-cli.exe dashboard examples/ftr-patterns/generated/cp.parquet examples/ftr-patterns/generated/dashboard.html --ftr-input examples/ftr-patterns/generated/patterns.stdf --ftr-input examples/ftr-patterns/generated/patterns.stdf.gz
# Requires Playwright and an installed Microsoft Edge browser.
node scripts/smoke_ftr_patterns.cjs
```

The demo includes an STDF and its gzip copy: expect **one unique source, 43 FTR
attempts, 9 failures, 1 unknown, 1 not executed, and 78.05% valid yield**. The
`scan/core_at_speed` pattern has 15 attempts, 10 passes, and 5 failures (66.67%
yield). `memory/mbist` has 13 passes, including one alarm. `io/loopback` has
9 passes, 3 failures, and 1 unknown. The not-executed-only pattern has N/A yield;
the missing-name group contains one failure. These are synthetic fixtures, not
production characterization results.
