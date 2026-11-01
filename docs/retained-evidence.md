# Retained viewer evidence

`viewer-evidence-v1` attaches native `viewer-v5` evidence to one explicit source
in an existing catalog snapshot. It preserves every record and PRR attempt,
including devices with no PTR/MPR/FTR results, repeated executions, site
association, raw/resolved fields, and offsets into retained decompressed STDF.
It also retains native measurement Parquet and its exact execution references.

The attachment is separate from `catalog-snapshot-v1`. It changes neither that
snapshot's identity or `raw_retention` declaration nor the selected snapshot
pointer. Existing dashboards and the viewer continue to use their existing inputs.

## Run it

This example converts exactly one source, so there is exactly one source ID:

```powershell
.\target\release\zstdf-cli.exe convert run.stdf.gz --output-dir dataset --layout catalog
$snapshot = .\target\release\zstdf-cli.exe snapshot create dataset | ConvertFrom-Json
$sourceId = $snapshot.catalog.sources.PSObject.Properties.Name
$evidence = .\target\release\zstdf-cli.exe snapshot retain-evidence dataset $snapshot.snapshot_id $sourceId run.stdf.gz | ConvertFrom-Json
.\target\release\zstdf-cli.exe snapshot list-evidence dataset
.\target\release\zstdf-cli.exe snapshot verify-evidence dataset $evidence.evidence_id
.\target\release\zstdf-cli.exe snapshot evidence-record dataset $evidence.evidence_id 1
.\target\release\zstdf-cli.exe snapshot evidence-attempt dataset $evidence.evidence_id 1
```

Check each command's exit status before using its JSON output. For a multi-source
snapshot, choose the intended source ID explicitly from `catalog.sources`.
Record IDs and attempt IDs are separate, one-based, source-local identifiers.
Out-of-range IDs fail; they do not select another device or a PART_ID match.

Retention verifies the supplied file's original-byte SHA-256 against the chosen
source. A moved copy with identical bytes is allowed. If two catalog paths have
the same bytes, their explicit source IDs remain distinct: the command never
picks the first hash match or merges their devices.

## What is stored and verified

Each immutable directory `dataset/evidence/<evidence_id>/` contains:

- `bundle.json`: versioned snapshot/source binding, original and decompressed
  hashes, coverage counts, import status, file lengths and hashes.
- The native viewer manifest, record/attempt indexes, companion Parquet tables,
  measurement lookup files where applicable, and native measurement Parquet.
- `source.stdf`: the complete decompressed content used for record inspection.

An evidence ID hashes the descriptor's canonical payload and object inventory.
It is an integrity check, not a signature. Repeating retention with unchanged
inputs is idempotent; damaged existing output is rejected, never overwritten.
The native manifest includes the source locator, so moving the input before a
new retention operation can produce a different evidence ID. Moving an already
retained dataset preserves its IDs and relative file references.

Native evidence is generated in a fresh private workspace using the existing
bounded viewer ingestion path. Arbitrary existing caches are not promoted into
archives. The catalog comparison checks the full multiset of typed EAV values,
including duplicate multiplicity, using disk-backed sorting. Equal row counts
alone are insufficient.

This comparison is labeled `typed-row-multiset-v1`. It does **not** prove which
original catalog row belonged to a particular execution when rows are identical.
Exact record/attempt/measurement references belong to the retained native copy,
which is reconstructed directly from the source. FTR pattern information remains
in that native evidence even when EAV cannot distinguish identical result rows.

Verification checks the immutable snapshot/source association, complete file
inventory, lengths, hashes and metadata. It then reconstructs fresh native
evidence from the retained decompressed source and compares the native manifest
and object hashes, normalizing only the source locator and original-container
hash. This validates coverage, indexes, counts and provenance without loading
the full population into memory. The current catalog, selected snapshot pointer,
original STDF and disposable viewer caches are not consulted.

Native regeneration requires a compatible serializer and validator. Changes in
native serialization must be versioned or migrated; a structural `viewer-v5`
label alone is not a promise of identical bytes across arbitrary future builds.
Verification fails explicitly on incompatible reconstruction. Back up the whole
dataset, including snapshot descriptors, catalog objects and evidence directories.

## Coverage and limits

`records`, `attempts`, `executions` and `measurements` are different counts.
Executions count PTR/MPR/FTR records; MPR expansion can produce multiple EAV rows,
while PRR-only attempts produce none. An attachment covers **one catalog source**,
not every source in the snapshot. `snapshot_source_count`, captured run status
and failure count remain visible. Expected manufacturing/input completeness
remains unknown. Copied viewer First/Latest flags retain their original
single-source meaning; they do not define a dataset-wide retest policy.

For gzip input, the original compressed-byte hash and retained decompressed hash
are distinct. `raw_retention: decompressed-content-retained` supports record
replay; `original_container_retention: external-required` means the original
gzip container is not archived. The original catalog snapshot continues to
declare external raw retention.

All retention, verification and inspection commands accept:

```text
--memory-limit-mib 256
--disk-limit-mib 10240
--cancel-file PATH
```

Minimum memory/disk settings are 64 MiB. Metadata is bounded to the smaller of
16 MiB and one eighth of the memory setting. These are application budgets, not
a hard operating-system RSS cap. Disk accounting conservatively includes the
**entire `dataset/evidence` tree plus operation scratch**; existing retained
bundles and abandoned staging consume budget. Reverification needs room to
reindex the retained source. Catalog objects outside this tree are not charged
against this evidence-storage limit. Listing examines at most 1,000 directory
entries and only reports names, without claiming data integrity.

Verification and record/attempt inspection are logically read-only but need a
writable evidence directory for temporary reindexing and a local operation lock.
The lock serializes these operations with retention. Cancellation is checked
during evidence hashing/indexing and before publication; existing catalog
snapshot verification has no mid-hash cancellation callback, so cancellation can
wait for that phase to finish. Nothing is silently truncated.

Private staging is cleaned on ordinary failures and cancellation; only the
invocation's own workspace is removed. A process crash can leave an unreferenced
staging directory. No automatic broad cleanup or garbage collection is added.
Failures before directory publication preserve prior bundles and snapshot state.
A durability error after publication is not a rollback guarantee. Symlink/reparse
paths and escaping object names are rejected, but no protection from hostile
concurrent filesystem mutation or power-loss guarantee is claimed.

## Synthetic demonstration

```powershell
python examples/snapshots/generate_evidence_demo.py
```

The script creates a fresh synthetic gzip source, retains its evidence, publishes
a newer conversion, removes the original inputs, and verifies/inspects the old
evidence while the live catalog is unavailable. It leaves the generated dataset
and a JSON result under a new `target/evidence-demo-*` directory.

This DATA-001 increment provides retained evidence and CLI inspection.
Cross-source policy evaluation and snapshot-backed dashboards remain subsequent
work. Attempt-yield and shipped `analysis-receipt-v1` publication are documented
in [cli.md](cli.md) and [CHANGELOG.md](../CHANGELOG.md).
