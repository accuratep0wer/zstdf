# Single-file STDF engineering viewer

Status: implemented as the local `view` command. This workspace is independent of the existing Dashboard and traceability reports. It does not claim exact SmarTest product parity.

The primary interaction reference is the user-supplied **SmarTest 8.8.2 Detail data views** HTML saved under `examples`. The DataView manual excerpt, `examples/propose function spec.pdf`, is a supplementary analysis reference. These reference documents are not bundled with the viewer assets.

## Run the viewer

```powershell
cargo build --release -p stdf-cli

# Open one source in the default browser. Keep this terminal open.
.\target\release\zstdf-cli.exe view C:\data\run.stdf

# Gzip input and explicit field-validation domain.
.\target\release\zstdf-cli.exe view C:\data\run.stdf.gz --flow CP

# Verify and reuse a catalog's matching source fragments.
.\target\release\zstdf-cli.exe view C:\data\run.stdf --dataset C:\data\dataset

# An attachment needs no service, STDF, Parquet, network, or installation to open.
.\target\release\zstdf-cli.exe view C:\data\run.stdf --export-html run-view.html
.\target\release\zstdf-cli.exe view C:\data\run.stdf --export-html summary.html --export-scope summary

# Explicit budgets, cache location, and cancellation signal.
.\target\release\zstdf-cli.exe view C:\data\run.stdf --cache-dir C:\data\viewer-cache `
  --memory-limit-mib 256 --disk-limit-mib 10240 --export-size-mib 20 `
  --cancel-file C:\data\viewer.cancel --no-open
```

Remove a previously created cancellation flag before starting a new invocation. Creating the flag cancels indexing, queries, or the service. Ctrl+C also stops the process. `--no-open` prints the loopback URL; `--port 0` chooses an available port. No external assets or CDN are used.

The default cache lives in the OS temporary directory under `zstdf-viewer`. A source hash and viewer schema version identify its immutable generation. Plain and gzip forms have the same decompressed-content hash but separate source/cache hashes. A modified source gets a new generation. Existing cache checksums must verify; use a fresh cache directory if a cache is damaged.

## Workspace interactions

Start in **Tests** with a linked **Data Log**. The left navigation contains Information, Tests, Devices, Wafers, Hard Bins, and Soft Bins. Information lists source hashes, record inventory, and the optional CP/FT validation results. The toolbar chooses run, wafer, and population. Style and language controls provide the existing five themes and English, Chinese, Japanese, Korean, and German.

**Show In** opens an additional table or plot. Panes coexist; the navigation width and pane height are resizable. In Properties, assign the same **Tab group** name to multiple panes to put them into a resizable tab group. Leave the group empty to display the pane independently. Up to 16 panes can be open.

| Control | Behavior |
|---|---|
| Link | Follow the current navigation selection. |
| Pin | Capture the selected tests/devices/bins; subsequent navigation does not change this pane. Global population, run/wafer, and exclusion policies still apply. |
| New | Copy the pane type, selection, link state, and analytical options into another pane. |
| Statistics | Show exact numerical statistics underlying a plot. |
| Properties | Choose columns, sites, bins, percentile whiskers, grouping, title, legend, map color/statistic, and an alert rule. |
| Image | Download the visible plot as SVG or PNG. |
| Save / Load session | Export or restore a source-hash-bound JSON layout and selection. The local service also saves the session in browser storage. |

Drag a rectangle inside a numerical plot to zoom. Double-click resets its range. Click a plotted point to inspect its original record and select its attempt in linked Data Logs. Click a wafer cell to open the contributing Devices. Spatial overlays show explicitly ordered wafers without merging their device identities; controls rotate, flip Y, and reverse the wafer order. The map tooltip reports coverage and per-wafer verdicts. Missing coverage remains visible.

### Data Log and record inspection

Rows are 24 px. The table reads bounded pages and virtualizes the scroll surface. Previous/Next also navigate pages, including beyond the browser's practical scroll-height limit. Header sorting cycles ascending, descending, then original source-record order. MPR channels retain distinct row identities even when they share a source record. The first identity column is frozen; column headers can be resized.

The visible filter row supports substring matching and `=`, `!=`, `>`, `>=`, `<`, `<=`. Each header's dropdown lists values with counts, search, select-all, and checkbox selection. Value lists exceeding 10,000 distinct entries require a text/numeric filter instead of truncating silently. Navigation search runs across the complete summary, not only its visible page.

Click, Ctrl-click, and Shift-click select table rows. Enter inspects the focused row; up/down arrows move between visible rows. Double-click opens the record inspector. Right-click offers Include, Exclude, Analyze Selection, Show In, and copy. Copy produces tab-separated values; CSV/JSON exports use the full filtered scope, subject to the attachment budget.

Failure and alarm icons reflect recorded flags. Saved user alerts use the same text/numeric comparisons as filters. The right overview ruler previews up to 2,000 markers and explicitly labels this limit; previous/next annotation navigation searches the full result. Select failure, alarm, exclusion, alert, or all annotations.

The inspector retains byte offset, raw field presence, raw value, effective value, inherited/default origin, and original hex bytes in the local workspace. Null raw values display `[null]`. STDF date fields show UTC conversion beside the integer. Gzip offsets refer to decompressed bytes. Unassigned DTR/GDR/vendor records remain unassigned; a site is never guessed.

Test method and foreground/background timing fields are explicitly unavailable when an ordinary STDF has no corresponding evidence. The viewer never derives them from test names.

### Selection semantics

| Action | Effect |
|---|---|
| Display filter | Changes rows visible in that pane, leaving analytics unchanged. |
| Measurement Include/Exclude | Changes test statistics and test/pattern Pareto selection; never rewrites recorded PRR yield or bins. |
| Device Include/Exclude | Separately changes the device population used for PRR bin analysis. |
| Analyze Selection | Includes selected rows and excludes other rows in the pane's represented, filtered scope. Oversized scope exports fail explicitly. |

Latest is the default. First and All attempts are explicit alternatives. Whole attempts are selected **before** site, verdict, and display filters, so hiding a newer pass cannot resurrect an earlier fail.

ECID follows the established latest-device identity: lot, wafer scribe/wafer ID when available, and a complete positive coordinate pair. PRR X/Y take precedence; when the pair is unusable, both coordinates come from the shared PTR coordinate resolver. Conflicting/missing PTR candidates remain unresolved. Repeated PART_ID and site do not merge distinct identities. MIR START_T orders runs; PRR source order breaks ties within the same run. Equal or missing timestamps across relevant runs do not fabricate a latest winner. Devices and Records preserve unresolved cases; deduplicated analytics omit them and the source status shows their count.

Raw Data Log retains repeated executions. Analytics select the last execution for each test/channel (and recorded FTR pattern) within the selected attempt. The existing EAV identity columns and legacy Dashboard population policies are unchanged.

## Analytical views

| View | Implementation |
|---|---|
| Histogram | 20 default bins, 10/20/30 presets, half/double controls, shared bin edges across comparable series, stacked/grouped bars, count and population-percentage axes, mean/limits, optional ±2σ/±3σ. |
| Box | Median/quartiles with 2nd/98th-percentile default whiskers; configurable percentiles; displayed outliers remain included. |
| Probability | Empirical CDF; all ties receive their full cumulative rank. Reduced plot points retain exact population ranks. |
| Trend | Source record sequence, site series, and available changing limits; no invented measurement timestamps. |
| Scatter / Range | Two tests pair within an attempt using the last execution; report unpaired attempts and exact correlation. Range displays min/mean/max; incompatible units get separate plots. |
| Pareto | Hard bin, soft bin, test, and FTR pattern levels; stacked/grouped bars and optional passing bins. Categories page in groups of 25. |
| Bin wafer map | Verdict or selected bin color; spatial overlay, missing-coordinate counts, per-cell device navigation. |
| Parametric wafer map | Explicit min/mean/max aggregation; missing values remain unmeasured. |
| Timing | Recorded PRR duration and MIR start time, separate from unavailable per-test execution timing. |

Statistics use all included values, sample standard deviation, and linearly interpolated quantiles. Invalid, nonfinite, missing, and nonnumeric entries have separate counters. Ppk is labeled as an **overall-sigma** estimate and suppressed for variable limits; no within-process Cpk is invented.

Plot point budgets depend on the memory budget, with at most about 2,000 points per series. Reduced rendering is labeled; exact statistics are unaffected. Scatter renders at most 20,000 pairs and labels reduction. Wafer-map selections exceeding 20,000 entries fail and require a narrower scope. More than 256 plot series also requires narrowing the selection.

## Storage, provenance, and budgets

The viewer retains `eav-v2` and uses a separately versioned viewer manifest. It creates a quota-limited seekable source spool, measurement EAV fragments, and companion Parquet tables for record locations, device attempts, and executions. Run and wafer metadata are in the manifest; field evidence is a disk-indexed companion stream. The companion tables expose typed ID, attempt, run, test, and numeric value columns alongside serialized row evidence.

Companion Parquet uses ZSTD compression. Indexed evidence and query rows use individually compressed blocks, so inspection and paging do not require decompressing the complete source population.

The bounded converter's optional `RowProvenance` API maps every emitted EAV row to the exact record ordinal and MPR expansion index. The viewer validates source-local attempt sequences. Each execution records its EAV fragment/row address. Repeated tests never join solely on test number or PART_ID.

`--dataset` verifies catalog hashes, source identity, schema, and the complete multiset of typed EAV rows against reconstruction from the original source. Validated catalog fragment bytes are copied unchanged into the immutable cache. Reconstructed companion references address those copies. Copies deliberately avoid hard links so changing an external catalog cannot mutate an already published viewer cache. An already verified viewer cache can be reused without rebuilding it. Incomplete or incompatible source coverage fails.

Queries project companion Parquet columns and prune their bounded row-group fragments by run/test. An on-disk attempt-to-fragment lookup narrows device drilldown. Exact sorting/grouping/quantiles use the existing spill sorter. The browser receives bounded pages and bounded plot payloads, not a complete EAV dataset. The original in-memory Dashboard accumulator is not used.

Table queries retain at most two bounded disk indexes per session. Paging an unchanged query reuses its index; changing selection, exclusions, filters, or sorting builds a distinct index. Session shutdown removes these temporary indexes. Initial query and sort latency should be assessed separately from subsequent page latency.

Memory limits are allocation/buffer budgets rather than an OS-enforced RSS ceiling. Index metadata, pending test batches, group counts, query bodies, plot points, exports, and scratch stores have explicit limits. Disk headroom is reserved for simultaneous spill stores. Use the measurement script below to record actual indexing time, RSS, disk usage, and latency for the intended workload.

Cache generations and exports publish only after success. Malformed records, incomplete units, cancellation, or quota errors leave previous published reports intact. The service binds only to IPv4 loopback, uses a per-session unguessable URL, checks Host/Origin, bounds JSON request sizes, and serves bundled assets. It is a local single-user service, not a shared network deployment.

`--flow CP|FT` applies the bundled `config/sanity-checks.csv` field-selection policy to the record inspector and Information validation totals. Optional records remain optional. The standalone `sanity` command remains the full configurable run-profile/diagnostic report interface.

## Sharing

**Current analysis selection** includes the selected population's measurement executions, PRR-only devices, associated record fields, and run/unassigned context. It embeds source hashes and selection/exclusion settings. It can filter, inspect, and recalculate within captured evidence offline; it cannot expand to a population that was not exported.

**Summary snapshot** includes saved aggregate views and test summaries, with explicit unavailable reasons for views exceeding their limits. It has no record drilldown. Summary controls cannot imply a new source population. Source hashes identify the evidence; sending email is outside this command.

The default complete HTML size limit is 20 MiB. Size checks apply before writing the final file. Narrow a selection, use a summary snapshot, or explicitly increase `--export-size-mib`; no silent attachment truncation occurs.

## Reproduce and verify

```powershell
python examples/viewer/generate_demo.py
.\target\release\zstdf-cli.exe view examples/viewer/generated/demo.stdf
Start-Process examples/viewer/generated/selection.html

cargo fmt --all --check
cargo test --workspace --exclude stdf-py
cargo check -p stdf-py
cargo build --release -p stdf-cli
node scripts/smoke_viewer.cjs

# A larger source and measured indexing/query run; psutil is required by the monitor.
python examples/viewer/generate_demo.py --units 10000 --source-only --output-dir target/viewer-stress
python scripts/measure_viewer.py target/viewer-stress/demo.stdf
```

The browser script requires Playwright and Edge. It checks dense tables, pin/link/new, full-column selection, inspector, plot rendering, all theme/language combinations, session restore, loopback access restrictions, exact live/offline statistics, and disconnected attachment use. Rust regressions cover provenance, MPR expansion, multi-site ordering, repeated executions, latest/first/all, missing identities/timestamps, invalid values, gzip, catalog reuse, cancellation, and preservation on errors.

### Local performance observation

The synthetic 10,002-attempt source (2.50 MiB) was measured on Windows with a 64 MiB memory budget and a 2 GiB disk budget. Browser validation ran concurrently during part of this measurement, so these are workload observations rather than isolated performance guarantees:

| Measurement | Observed |
| --- | --- |
| Fresh source indexing | 38.2 s |
| Cold Data Log query | 22.2 s |
| Next page using the same disk query index | 0.17 s |
| New value sort | 25.1 s |
| Exact histogram/statistics query | 13.8 s |
| Pareto query | 1.19 s |
| Peak observed process working set | 23.8 MiB |
| Persistent cache after shutdown | 65.9 MiB |
| Cache plus retained query indexes before shutdown | 135.6 MiB |

Query timings include the local Fetch client startup. Disk sizes are end-of-phase observations, not peak scratch-disk measurements. Cold scans and new sorts remain noticeably slower than paging; larger production-file and million-device performance is not established by this fixture. Re-run `scripts/measure_viewer.py` for representative tester data.

## Explicit next release boundary

CTSR/CTRR shmoo/margin plots and dedicated ATER/GDR activity views are subsequent-release work; raw records remain inspectable now. FFC Pareto explains its unavailable status until a documented first-failing-cycle mapping exists. CYCL_CNT is not treated as FFC.

Live Result streaming, Pattern Debug, test-program editing, DC profiling files, and foreground/background execution timelines require inputs beyond one ordinary STDF and are outside this implementation. Manufacturing step completeness remains the independent traceability command's responsibility.
