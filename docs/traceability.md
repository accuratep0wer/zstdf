# Test flow traceability

`traceability` reads STDF directly and produces a self-contained, offline HTML
report. Each PRR is one attempt. Retests retain their individual verdicts and bins;
this command does not change Dashboard's all-pass aggregation, read measurement
Parquet, or require a new `eav-v2` migration.

```powershell
.\target\release\zstdf-cli.exe traceability C:\data\cp C:\data\ft\run.std.gz `
  --flow-config .\flow.json --flow-closures .\closures.json `
  --identity-map .\identities.json --output .\trace.html
```

The output's parent directory must already exist. Explicit files can have any
extension; directories recursively discover `.stdf`, `.std`, `.stdf.gz`, and
`.std.gz`. Gzip is detected by magic bytes. Symlink files supplied explicitly are
rejected; symlinks encountered during directory traversal are skipped.

## Configuration

See runnable [flow](../examples/traceability/flow.json),
[closures](../examples/traceability/closures.json), and
[identity mappings](../examples/traceability/identities.json).
The example stage names are arbitrary and are not a recommended product flow.

```json
{
  "flow_id": "my-flow",
  "version": "1",
  "steps": [
    {
      "id": "first-step",
      "required": true,
      "stop_on_fail": true,
      "matches": [
        {"job_nam": "PROGRAM_A", "job_rev": "1", "test_cod": "CP"},
        {"job_nam": "PROGRAM_A", "job_rev": "2", "test_cod": "CP"}
      ]
    }
  ]
}
```

Every step has a unique ID, explicit `required`, and one or more selectors.
`stop_on_fail` defaults to false. Selector fields are optional `job_nam`,
`job_rev`, `test_cod`, `flow_id`, and `tst_temp`. Comparisons are exact,
case-sensitive strings, including temperature; fields in one selector are ANDed,
selectors are ORed. Empty selectors, unknown fields, duplicate step IDs, and
invalid identities are errors. No matching step or multiple matching steps means
“step unidentified”; those attempts remain visible and required-step absence is
indeterminate. Several matching selectors inside a single step are not ambiguous.

Closure files contain an array of objects with `flow_id`, `version` and exactly
one of `lot` or `device`. ID/version must equal the report's configured flow.
A lot closure applies to devices observed in that lot, including explicitly
mapped devices. Device closures accept a complete identity tuple; mapped source
identities are normalized before comparison. MRR closes an STDF run only.

Mapping files contain an array of `{"from": [...], "to": [...]}` objects.
Tuples use `[namespace, identifier, positive_integer_x, positive_integer_y]`.
Only `lot-ptr` → `wafer-prr` is allowed. Multiple different targets for the same
source tuple are rejected. Equal duplicate mappings are harmless. Multiple
explicit source tuples may map to the same wafer identity. The report stores
both the original key and mapped key.

## Identity and attempts

The command shares the converter's [coordinate resolver](coordinate_identity.md).
Valid uppercase alphanumeric wafer ID plus positive PRR X/Y wins; otherwise the
entire tuple falls back to lot plus positive integer PTR X/Y. PTR coordinate
candidates are accumulated in constant space per active head/site, retaining the
existing priority and conflict rules. No PRR/PTR axis mixing or coordinate-only
cross-lot/wafer joining occurs.

Unmapped lot/PTR identities carry `fallback_unlinked`; absent required steps are
indeterminate because the other namespace may hold their history. Unresolved
coordinates are isolated by decompressed source hash and source-local sequence.
PART_ID and head/site do not split valid coordinate identities. Every PRR,
including a PRR without PTR or with only MPR/FTR, contributes an attempt. MPR/FTR
measurement details are not expanded. Repeated PART_IDs never collapse attempts.

Files are SHA-256 hashed over decompressed bytes and unique contents parsed once.
Byte-identical plain/gzip copies share evidence and preserve all source paths.
The parse pass checks the digest again to reject a changing source. Attempts
contain PRR record-start byte offsets in the **decompressed** stream, source-local
sequence (assigned on PIR, or on a standalone PRR), head/site, raw wafer/PRR
coordinates, lot, PART_ID, flags, bins, SDR equipment and all decoded MIR fields.

An effective verdict is unknown when PART_FLG bit 4 invalidates it or bit 2 marks
an abnormal end; otherwise bit 3 determines Pass/Fail. This conservative handling
keeps aborted tests from becoming normal passes. Valid bins are 0–32767; other
values including 65535 are null for comparison, with raw bins retained. See the
[STDF V4 specification, PRR](https://www.roos.com/roos/documentation.nsf/3d6a93a7e05462cf85256a9c007dcaf3/92102f712ce51df48825783800832332/%24FILE/STDF%20Spec%20V4%202007.pdf).

## Ordering, differences and missing steps

For each device and recognized step, N independent attempts mean N−1 retests.
All valid values participate in verdict/hard-bin/soft-bin difference detection;
`Pass → Fail → Pass` is a difference. Unknown results are separately flagged.
Parameter measurement changes are outside this version's scope.

Within one content source, sequence determines order. Between sources, complete,
nonzero MIR start and MRR finish timestamps must form disjoint intervals with
`earlier.finish < later.start`. Overlap, touching boundaries, reversed intervals,
or missing times prevent a claim of total order. Attempts still have a stable
display order, but `order_unknown` is set and `latest_passed` is null. Filenames,
input argument order and hashes never imply chronology. Unknown final verdicts
also remain null rather than falling back to an older pass/fail.

| State | Meaning |
| --- | --- |
| Tested | One observed attempt with complete result |
| Retest consistent | Multiple attempts with matching valid verdict and bins |
| Retest different | Any valid verdict or bin changed; incomplete flags can coexist |
| Missing | Required absent step preceding an observed later step, or after explicit closure |
| Pending | Required future step on an open flow |
| Not applicable | Unexecuted optional step or established failure stop |
| Indeterminate | Identity/step attribution, result, or relevant stop decision is uncertain |

A stop is based on the latest determinable complete result of a configured
`stop_on_fail` step. Retesting that step to pass reopens downstream expectations.
Unknown order/result never establishes a stop; dependent absence is indeterminate.
Observed downstream tests always remain visible, even after a stopping failure.
Unrecognized attempts affect missing inference but are not assigned guessed
step-specific retest counts. Completely unobserved chips are outside the population.

## Report and evidence

The Chinese-language report includes a paginated device × step matrix, wafer/lot
and coordinate filters, step-column selection, anomaly filters, all-attempt
drilldown, side-by-side step comparison and anomaly links. Status text and symbols
accompany colors. Step detail distinguishes stable display order from chronology.
JSON export always includes **all** evidence, independent of active filters/pages.
Source paths are text, not fetched hyperlinks; the page makes no network requests.
Embedded data is script-escaped and rendered with DOM text nodes.

The embedded `traceability-v1` JSON contains `flow`, `closures`, `identity_map`,
`sources` (SHA-256 → source paths), `runs`, and `devices`. Each device has ordered
`steps`, plus `unidentified` attempts and `issues`. Each cell contains attempts,
test/retest counts, status, changes, completeness, ordering and nullable latest
verdict. Configuration snapshots and raw evidence allow independent review.

## Resource limits, errors and cancellation

| Option | Default |
| --- | --- |
| `--memory-limit-mib` | 256 |
| `--disk-limit-mib` | 1024 |
| `--max-report-mib` | 32 |
| `--max-devices` | 100000 |
| `--max-sources` | 10000 source paths, including aliases |
| `--temp-dir` | OS temporary directory |
| `--cancel-file` | None; creating this file requests cooperative cancellation |

Input records stream into `stdf-analytics::SpillStore`. Sorting groups all attempts
for one device; the reducer releases each completed device after appending its
JSON. The memory budget accounts for sort buffers, metadata, active contexts,
one device group and report construction; it is **not a hard process RSS cap**.
Minimum memory is 16 MiB, report limit must be positive and at most memory/4.
Sort records are capped at 128 KiB; unusually large hardware evidence fails
explicitly. Large individual device histories and metadata also have enforced
budgets. HTML escaping and complete rendered HTML count toward report size.
There is no silent record/device truncation.

The disk budget reserves maximum report size for atomic output staging, with the
remainder allocated to live sort scratch (including merge inputs/outputs). It
does not include the pre-existing report or input STDF files. Disk limit must
exceed maximum report size.

Decode errors, malformed supported records, orphan PTR/MPR/FTR, duplicate open
PIRs, unclosed test instances and missing MRR fail the command. Zero timestamps
are accepted as unknown. Missing coordinate data remains an isolated attempt,
not a decode failure. Vendor-extension records not understood by the decoder are
skipped. This is not a replacement for the separate `check` conformance command.

Processing errors, quota errors and cooperative cancellation leave an existing
report intact. The complete HTML is written and synced to a sibling staging file,
then cancellation is checked again before atomic replacement. Ordinary errors
clean up this invocation's scratch and staging files. Ctrl+C/process termination
can leave scratch files, but cannot publish partial HTML; a termination racing
the atomic commit leaves either the complete old or complete new report.

## Reproduce the synthetic demo

From the repository root:

```powershell
cargo build -p stdf-cli --release --locked --offline
python .\examples\traceability\generate_demo.py --cli .\target\release\zstdf-cli.exe
```

Open `examples/traceability/generated/demo.html`. The generator owns its named
fixture files and never removes unrelated files. It demonstrates ten devices,
including P/F flips, bin-only changes, missing/pending steps, failure stops,
recovery, explicit mapping, unresolved association, unknown steps and overlapping
run times. The gzip copy is deliberately deduplicated.

```powershell
cargo test --workspace --exclude stdf-py --locked --offline
cargo check -p stdf-py --locked --offline
cargo fmt --all --check
# With Playwright available on NODE_PATH and Edge installed:
node .\scripts\smoke_traceability.cjs .\examples\traceability\generated\demo.html
```

The browser smoke checks desktop/mobile filtering, drilldown, highlighter states,
unknown-order wording, all-evidence JSON download and zero external requests.
Only local synthetic evidence is covered; actual product flow configuration and
tester datasets must be supplied for product-specific validation.

Local verification on Windows, 2026-09-10: **291 workspace Rust tests passed**
(excluding Python extension tests), including **25 traceability regression tests**.
Rustfmt check, Python binding compile check and Release CLI build passed. The
Release-generated demo passed the Edge/Playwright desktop and mobile smoke with
no page errors or external network requests.

## Display settings

The report includes top-right selectors for five styles and five languages.
Switching preserves filters and selected-device details; original field values,
identifiers, diagnostic evidence, and JSON exports remain unchanged. See
[shared report display settings](report-ui.md). Regenerate older HTML reports
with the updated CLI to receive these controls.
