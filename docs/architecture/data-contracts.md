# Snapshot and analysis contracts

Status: mixed. Existing `eav-v2`, catalog v1 and `viewer-v5` remain supported.
Shipped attempt-yield (`attempt-yield-request-v1` / `attempt-yield-result-v1`) and
`analysis-receipt-v1` publication are documented here and in [cli.md](../cli.md);
broader items such as full `dataset-snapshot-v1` inventory remain proposed and are
not aliases for currently emitted catalog-snapshot metadata.

## Source identities and coverage

Keep these concepts separate:

| Identity | Meaning and rule |
| --- | --- |
| Catalog logical source ID | Existing SHA-256 of canonical source path; preserve for compatibility |
| Raw object hash | SHA-256 of stored input bytes, including gzip container bytes when compressed |
| Decompressed content hash | SHA-256 of the decoded byte stream; required when content-equivalent gzip/plain dedup is requested |
| Conversion generation | Exact source revision plus converter build/schema/options and output object inventory |
| Run | Generation plus source-local MIR occurrence, not job name alone |
| Attempt | Generation plus source-local PRR sequence; preserve PRR-only devices |
| Execution | Generation plus source record ordinal; multiple executions of the same test remain distinct |
| Measurement | Execution plus explicit result/channel index; never test number or PART_ID alone |
| Device association | Policy-versioned identity or explicit mapping, separate from the physical attempt |

Content equivalence is not automatically device equivalence. The first snapshot
wrapper preserves catalog path semantics and records `catalog-path-v1` as its
source-inclusion policy. It reports content aliases when known but does not
silently deduplicate them. A content-deduplicating adapter requires its own
policy/version and gzip/plain, copied-source and input-order tests. Retain alias
paths for forensic use without making them keys for result ordering.

Existing catalog verification checks converted fragments, not raw-file replay.
A snapshot must declare raw retention as `retained`, `external-required` or
`unavailable`. Retained raw bytes are verified and managed as immutable objects.
External paths are locators, not proof of availability. A moved, missing or changed
external source cannot silently substitute for its frozen hash. Declare replay
capabilities per operation: analytics-only results may remain usable while raw
record inspection or source-derived Pareto is unavailable.

## Snapshot publication and selection

A proposed `dataset-snapshot-v1` manifest contains:

- Snapshot format version and dataset lineage ID.
- Exact sorted source-generation references and conversion policy/build hashes.
- Object inventory: relative object ID, hash, size, schema version and row count.
- Companion coverage: runs, attempts, PTR/MPR/FTR executions, provenance and raw
  replay availability. Unknown coverage is distinct from zero records.
- Expected input inventory and selected source outcomes: succeeded, failed,
  pending, or explicitly carried-forward previous generation.
- Captured validation profile/hash and findings, identity/dedup policy versions,
  and content-alias information available at publication.

Hash a canonical manifest payload with a domain/version prefix to obtain
`snapshot_id`. Exclude the hash itself, creation timestamp and mutable display
locators from that payload; exact generation refs, object hashes, inclusion
policy and completeness evidence are part of it. Keep the canonical payload
alongside any descriptive metadata. Dataset-root relocation must not change the
identity; changing its referenced content must.

1. The writer stages objects, closes them, verifies hashes/schema/counts and
   coverage, then publishes an immutable manifest with create-if-absent semantics.
2. A writer lock serializes local publication. Verify an existing manifest with
   the same ID instead of overwriting it. After publication, atomically replace
   a small current-snapshot pointer. A crash may leave an unreferenced complete
   snapshot; it must never leave a pointer to incomplete objects.
3. Readers resolve the pointer once, verify the selected manifest and its exact
   object set, then retain that snapshot handle for the entire analysis. Do not
   query `objects/**/*.parquet` or reread a changing catalog between pages.
4. Retain all published snapshots and referenced generations in the initial
   implementation. Deletion/GC is disabled until snapshot, receipt and active
   reader reachability rules are implemented and tested.
5. Failed publication preserves previously published snapshots. This does not
   change the current converter's existing per-source commit behavior.

Publication policy defaults to requiring complete inputs for the requested import
inventory. An explicit partial snapshot must retain failures, pending sources
and stale carried-forward revisions, and every dependent result must expose them.
`catalog.run.status == complete` alone cannot prove completeness of a wafer,
manufacturing flow, dataset history or an unspecified expected inventory.

Imported legacy catalogs may initially have `expected_inventory_unknown`. Permit
status/inspection with that warning; never relabel them complete automatically.
A complete snapshot may still contain unknown verdicts or unresolved identities;
those are separate quality dimensions with counts.

## Reuse companions before creating ART

The proposed logical tables are an API model, not a second competing storage
system. Map existing viewer companions and EAV provenance first. Before publishing
a retained snapshot, copy/import verified companions and any retained raw spool
into the managed object store (or an explicitly durable equivalent). The default
viewer cache may live in OS temporary storage; a reader pin cannot prevent an
external cache cleanup. Never publish a retention guarantee based solely on a
path into that disposable cache. Missing companions make dependent analytics
unavailable; missing raw bytes affect raw replay as declared separately. If a reusable
physical schema is justified, version and migrate it separately; do not announce
`art-v1` until its mappings and compatibility tests exist.

Required coverage includes source/run metadata, every PRR attempt, repeated PTR
and MPR executions with result indices, FTR record verdicts and VECT_NAM patterns,
bin definitions scoped by run/head/site, and source record ordinals/offsets.
Gzip offsets refer to decompressed content. Preserve raw flags and raw/effective
value distinctions. Unexpanded vendor records remain inspectable as evidence
without inventing interpretation. Do not reconstruct lost metadata from an EAV
row count or repeated PART_ID; return unsupported/coverage-incomplete instead.

Explicit cross-stage identity maps need a version/hash, source and target
namespace, conflict checks and recorded provenance. Matching coordinates alone
cannot invent CP-to-FT links across wafers/lots.

## Policy-bound analysis requests

The viewer already has an internal typed `Query`; the new adapter must version
its public subset and reject unknown fields or unsupported combinations. All
operations use a common request envelope, including site/retest/Pareto operations:

- API/operation version and one resolved snapshot ID.
- Identity, ordering, duplicate-source, repeated-execution and population policies.
- Lot/run/wafer/test/site/flow scope, explicitly scoped to the operation.
- Separate device exclusions, measurement exclusions and presentation filters.
- Partial-data acceptance and required coverage, numerator/denominator definition.
- Resource limits, precision/statistical definitions and result ordering.

Avoid a universal `identity_policy: coordinate-v1` default:

| Existing behavior | Contract to preserve |
| --- | --- |
| Dashboard identity | Wafer + positive PRR pair, or whole lot + PTR fallback; namespace separation; primary wafer key intentionally lacks lot |
| Latest Pareto / viewer | Lot + wafer/scribe when known + coordinate pair; latest MIR START_T, then within-run source sequence; unresolved cases visible |
| Traceability | Configured flow/step and explicit maps; cross-source chronology requires disjoint complete run intervals, unlike Latest ordering |
| Repeated measurements | Raw inspection retains all executions; reduction for analytics is explicit per operation/test/channel/pattern |

Use the detailed [coordinate](../coordinate_identity.md), [Latest](../latest-pareto.md),
[traceability](../traceability.md) and [viewer](../single-file-viewer-proposal.md)
contracts rather than inferring interchangeability. Freeze versioned registry
entries for these policies before publishing the new API.

For deduplicated Latest analytics, choose the complete attempt before result/site
presentation filters, and never fill missing tests from older attempts. Manufacturing
step scoping is an explicit population boundary, not a post-hoc rescue of an old
attempt. Measurement exclusion cannot rewrite PRR bins or verdicts. Reject an
operation if the required scope/order/identity evidence is unavailable.

## First exact operation

Shipped PRR attempt-yield uses retained evidence (not EAV row counts) via
`zstdf-cli snapshot attempt-yield`. Population `all_attempts`
(`prr-attempt-yield-v1`) counts every eligible PRR once, including devices with
no measurements. Population `latest_device` (`prr-latest-device-yield-v1`) is also
shipped and requires `identity_policy: latest-ecid-start-t-v1`; it is not
Dashboard `coordinate-v1` merging and does not change the all-attempts
denominator.

Return pass, fail, unknown, eligible, excluded and unresolved counts. `eligible`
is the scoped PRR population after device exclusions; `excluded` counts scoped
attempts removed by device exclusions. Measurement exclusions do not affect this
PRR yield operation. `unresolved` counts identity-unresolved attempts within the
eligible population and overlaps its verdict categories; it is not a sixth
mutually exclusive outcome. Require `eligible = pass + fail + unknown` and
`scoped_attempts = eligible + excluded`. Return exact integer numerator/denominator
pairs for both `pass / eligible` and `pass / (pass + fail)`, with clear labels;
the latter is known-verdict yield, not a silent removal of unknown tests. A zero
denominator gives null with a reason. Missing identities do not exclude physical
attempts under all-attempts, but affect deduplicated eligibility explicitly.

Lot comparison, site variation, retest impact and Pareto follow only after their
shared policy/coverage envelope is tested. Site differences alone do not prove
hardware causality or GR&R; qualification/reliability decisions need study metadata.

## Receipt and reproducibility

Shipped `analysis-receipt-v1` publishes under
`dataset/analysis/<execution_id>/` as paired `result.json` and `receipt.json`
(content-addressed execution ID; create-if-absent / verify-on-replay). No
`complete` receipt before both files stage and the bundle renames successfully.
Failed/cancelled attempts may have diagnostic records but not a complete result.
Receipts refer to retained snapshots; a hash is not a backup or a trusted signature.

A complete shipped receipt records:

- Unique `execution_id` plus deterministic `request_fingerprint` and `result_hash`.
- `snapshot_id`, operation/request/result versions, population, source/identity
  policies, `allow_partial` and selected `evidence_ids`.
- Coverage summary (selected/missing sources, partial flag, expected inventory,
  content-alias counts, catalog run status/failures) and result `counts`.
- Build identity (`package_version`, `git_sha`, `target`) and platform.
  Package version `0.1.0` alone is not a reproducibility identifier.

Still proposed (not fields of the current receipt schema): separate
validation/mapping/limits/exclusion hashes, numerical-settings capture on the
receipt, and diagnostic execution-time/resource measurements as receipt members.

Define a versioned canonical encoder before claiming identical result hashes:
UTF-8, sorted object keys, deterministic array/row order, no insignificant
whitespace, explicit null/reason tags. Encode UInt64 counts/IDs as decimal strings
to avoid JavaScript precision loss. For the first summary use integer ratios,
not rounded floating percentages, as canonical results. Human-readable times use
UTC; creation times, UI language, labels and elapsed time are outside result hashes.

For later floating calculations, specify reduction order, rounding and a canonical
finite-number representation; preserve raw nonfinite classifications using tags,
not invalid JSON NaN/Infinity. Linear quantiles and sample sigma already exist;
overall Ppk must not be relabeled within-process Cpk. Plot sampling must be stated
separately from exact statistics. Different engines/builds may require documented
numerical tolerances; exact byte reproducibility is initially scoped to the same
pinned engine/build. AI narrative wording is not covered by result reproducibility.
