# Latest-device Pareto

The Dashboard Pareto page provides one chart with four analysis levels:

| Level | Category | Count |
| --- | --- | --- |
| Hard bin | PRR HARD_BIN | Unique selected devices in that bin |
| Soft bin | PRR SOFT_BIN | Unique selected devices in that bin |
| Test | Record type, TEST_NUM, and TEST_TXT | Unique selected devices failing that test |
| FTR pattern | Exact complete VECT_NAM | Unique selected devices failing that pattern |

Each device contributes at most once to a category. A device can fail several
tests or patterns, so the sum across failure categories is not the failed-device
population. MPR uses the overall record verdict, not one failure per numeric
result. Numeric limit comparisons do not replace the recorded test verdict.
Unknown/not-executed tests are not counted as known failures or passes.

## Device identity and latest selection

The derived ECID is `[lot, wafer-or-null, x, y]`; it is not a manufacturer fuse ID.
Lot must be populated. Wafer uses WRR FABWF_ID when supplied, otherwise the wafer
identifier from WRR/WIR. SDR head/site mappings associate units with wafer groups.
WRR metadata is applied to its wafer's earlier completed units. Ambiguous wafer
group associations do not produce an identity.

Both PRR coordinates must be positive integers. Otherwise both coordinates come
from the existing PTR name matcher, with case-insensitive X/Y recognition,
priority rules, and conflict rejection. PTR values must be finite positive
integers. PRR and PTR axes are never mixed. The coordinate source is recorded but
does not separate otherwise identical ECIDs. Missing wafer information remains
separate from known wafer identities; no coordinate-only alias is guessed across
wafers or lots. Repeated PART_ID values do not determine identity.

Selection uses MIR START_T, not END_T, filesystem modification time, upload order,
or filename. The greatest known START_T wins even when run intervals overlap.
Within the same source run, the last PRR sequence wins. Distinct source runs tied
at the maximum timestamp are ambiguous. Missing START_T is zero/absent; if it
prevents determining a unique latest run, the device is excluded. A single run
can still order its own attempts by PRR sequence without a timestamp.

The complete selected attempt supplies bins, test verdicts, and patterns. An
older failure disappears after a passing retest. A test absent from the selected
attempt stays absent; older attempts never fill it in. Repeated instances of the
same PTR/MPR test within one attempt use the last recorded verdict. For FTR,
last occurrences are retained per test/pattern pair; different patterns within
one functional test combine with failure taking precedence. For a pattern shared
by multiple FTR test numbers, the last occurrence per test/pattern is retained,
then any known failure makes the device fail that pattern once. Without a known
failure, an unknown constituent keeps the combined pattern verdict unknown.

This selection spans the supplied population. For step-specific results, provide
only that step's files. It is not a configurable manufacturing-flow engine.

Every PRR produces an attempt, including units with no PTR/MPR/FTR records.
The evidence contains `is_latest: true` for selected attempts, `false` for earlier
attempts, and `null` for unresolved selection. Earlier attempts remain available
in the evidence export. Exact decompressed-content duplicates count once; their
source paths are retained. Different files with overlapping results are not
silently deduplicated beyond the device/latest-attempt rules.

## Chart controls

- **Analysis level:** Hard bin, Soft bin, Test, or FTR pattern.
- **Bar layout:** stacked or side by side, with a shared count axis.
- **Color by:** lot, wafer, head/site, or one combined series.
- **Show passing bins:** hidden by default. Bins containing failures or unknown
  device verdicts remain visible. Passing-only bins appear when enabled. The
  classification comes from selected PRR verdicts, not an assumption that bin 1
  always passes. Mixed pass/fail bins retain all their selected devices.
- **Filters:** the Dashboard's lot, search, and minimum-failure controls apply.
- **Details:** click a bar or table entry for selected ECIDs, source paths, UTC
  MIR times, offsets, verdicts, and the latest flags. Bars support Enter/Space.
- **Export:** JSON includes the entire selection evidence. CSV includes all
  currently filtered categories, independent of the visible page.

Categories default to descending count. Table headings change ordering. Chart and
table show 25 categories per page with explicit navigation. There is no silent
top-N truncation. Audit preview shows 100 entries and states the total; JSON
contains all attempts. Five report themes and five languages remain available.

## Generate a report

```powershell
# Use the catalog's unchanged source STDF files automatically.
.\target\release\zstdf-cli.exe dashboard-dir dataset dashboard.html

# Supply raw evidence for a single-Parquet dashboard.
.\target\release\zstdf-cli.exe dashboard measurements.parquet dashboard.html --stdf-input C:\data\inputs

# Multiple paths are repeatable; --ftr-input remains a compatibility alias.
.\target\release\zstdf-cli.exe dashboard measurements.parquet dashboard.html --stdf-input run1.stdf --stdf-input run2.stdf.gz
```

Files and recursive directories are supported, including gzip. When using
automatic catalog discovery, source bytes must match the committed catalog's
SHA-256. Missing/changed sources disable latest Pareto with an explicit message;
other pages can still display the verified older measurement snapshot. Source
changes during analysis abort publication. Explicit raw inputs define their own
Pareto population; use matching files when comparing with measurement panels.

Single-Parquet reports without raw evidence show an unavailable message rather
than infer latest results from EAV rows that lack MIR times and VECT_NAM.
Once generated, the HTML works offline and does not need the source files.
The Parquet schema and the other pages' all-pass measurement aggregation remain
unchanged. Standalone `ftr-pareto` still reports all FTR attempts.

Analysis streams source records and retains bounded attempt/mode evidence. Limits
are 4,096 discovered source paths, 100,000 discovery entries, and 100,000 attempts.
The single-file Dashboard uses a 256 MiB accounted budget. Dataset dashboards
use the remaining `--memory-limit-mib` budget after measurement analysis.
Charges are conservative, including duplicate reads; they are not process RSS
guarantees. Serialized evidence is limited to one quarter of that budget, and
the existing total report limit also applies. Exceeded limits fail explicitly.
Malformed relevant records, duplicate PIRs, and unclosed attempts fail; existing
HTML is replaced atomically only after successful generation.

## Demo and verification

```powershell
cargo build --release -p stdf-cli --locked
python examples/latest-pareto/generate_demo.py
Start-Process examples/latest-pareto/generated/dashboard.html
cargo test -p stdf-cli latest_pareto
node scripts/smoke_latest_pareto.cjs
```

The two-lot demo has 15 attempts, 7 selected devices, and 8 earlier attempts.
Selected results are 3 passes and 4 failures: HBIN 9 = 1, HBIN 10 = 2,
HBIN 11 = 1, and passing HBIN 1 = 3. Pattern failures are scan = 1, leak = 2,
timing = 1. All run intervals overlap intentionally to verify START_T selection.
The browser check covers levels, exact counts, stacking geometry, passing bins,
filters, keyboard drilldown, CSV/JSON, themes, languages, and mobile layout.
