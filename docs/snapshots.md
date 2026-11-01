# Catalog snapshots

`catalog-snapshot-v1` freezes the exact current EAV catalog value and its verified
managed Parquet references. It is an additive first step toward the broader
[local-first architecture](architecture/overview.md), not an attempt-complete
analytics schema or a raw-STDF archive.

## Commands

```powershell
.\target\release\zstdf-cli.exe convert C:\data\inputs --output-dir dataset --layout catalog
$captured = .\target\release\zstdf-cli.exe snapshot create dataset | ConvertFrom-Json
.\target\release\zstdf-cli.exe snapshot list dataset
.\target\release\zstdf-cli.exe snapshot verify dataset $captured.snapshot_id
.\target\release\zstdf-cli.exe snapshot select dataset $captured.snapshot_id
.\target\release\zstdf-cli.exe snapshot current dataset
```

Creation and verification output a JSON descriptor containing `snapshot_id`,
`version`, `expected_inventory`, `raw_retention` and the frozen `catalog`.
The ID is a lowercase SHA-256 value. Descriptors are stored under
`dataset/snapshots/<id>.json`. Treat creation errors as failures before using
`$captured`; command failures return nonzero.

The descriptor includes the captured catalog revision and run metadata, so
creating twice from the same unchanged catalog returns the same ID. A separate
conversion run can produce a new ID even when source membership is unchanged.
This is capture identity, not a deduplicated content-only dataset identity.

`snapshot verify` checks the saved descriptor identity plus referenced Parquet
hashes, schemas, row counts and confined paths. It does not read the current
`_catalog.json` or original STDF. Old selections remain verifiable after source
replacement or catalog deletion as long as their managed objects remain intact.
`snapshot list` lists valid descriptor filenames only; it is not an integrity
check. Verify each selected ID before use.

## Select a snapshot for subsequent work

`snapshot select dataset ID` verifies a saved snapshot and atomically replaces
`dataset/_current_snapshot.json`. `snapshot current dataset` resolves that
pointer once, verifies its referenced snapshot and returns the full JSON
descriptor. Both commands work without the original STDF or `_catalog.json`.
Missing, malformed, oversized or dangling pointers return errors; there is no
fallback to another snapshot or to the current conversion catalog.

Conversion and ordinary `snapshot create` leave this explicit selection alone.
To create and then select in one command:

```powershell
$captured = .\target\release\zstdf-cli.exe snapshot create dataset --select | ConvertFrom-Json
```

Creation and selection are two publication steps. If selection fails, the saved
snapshot remains; the error includes its ID so it can be inspected or selected
again. Failures before pointer replacement preserve the previous pointer. A
failure during durability confirmation after replacement is not a rollback
guarantee; inspect `snapshot current` before retrying.

Selecting an incomplete snapshot requires its own `--allow-incomplete` flag,
even if that flag was used when creating it. Reading an already selected partial
snapshot retains its status and failure details. No flag makes expected input
inventory known or complete.

| File | Purpose | Updated by |
| --- | --- | --- |
| `_catalog.json` | Current conversion results used by existing dashboards | `convert` and recovery |
| `_current_snapshot.json` | Explicitly selected saved EAV snapshot | `snapshot select`, or `snapshot create --select` |

The pointer uses `catalog-snapshot-pointer-v1`, accepts only `version` and
`snapshot_id`, and is bounded to 4 KiB. Selection uses the same local writer lock
as conversion and rejects symlink/reparse-point pointer paths. It does not
rewrite either `_catalog.json` or an immutable snapshot descriptor.

## Resolve a stable reader selection

The library exposes `resolve_snapshot(root, None)` for the selected snapshot and
`resolve_snapshot(root, Some(id))` for an explicit one. Explicit IDs work even if
the pointer is missing or invalid. The returned `SnapshotSelection` exposes
read-only `snapshot()` and `fragments()` accessors. Its exact fragment inventory
is ordered by captured source ID and then stored fragment order; each entry
contains its source ID, verified absolute object path, row count and hash.

Resolve once at the start of an operation and retain that handle. Selecting S2
after a reader resolves S1 does not change the reader's descriptor or file list.
It never discovers additional files with a wildcard or rereads a changing
catalog between batches. New resolution after moving the whole dataset rebuilds
absolute paths without changing snapshot IDs; an existing handle must be
resolved again after such a move.

This handle pins the metadata selection, not file contents: it holds no writer
lock or open-file lease and does not prevent external deletion or mutation. There
is no garbage collector or reader registry. A query engine and its own batch,
memory and cancellation limits remain separate implementation work.

## Completeness and retention

Creation rejects empty source sets, running/unknown states and incomplete runs by
default. To explicitly capture a partial, failed or interrupted catalog:

```powershell
.\target\release\zstdf-cli.exe snapshot create dataset --allow-incomplete
```

Recover abandoned conversion work before capturing a running catalog. The flag
does not suppress hash/schema/path failures, and never permits a running state.
The original status and failures remain in the frozen catalog. Recorded failures
also require this flag even if the catalog reports a `complete` status.

Every descriptor states `expected_inventory: "unknown"`: the legacy catalog
cannot prove all intended inputs or wafer devices are present. `run.status` is
not manufacturing-flow completeness. It also states
`raw_retention: "external-required"`: original STDF is not archived or verified by
this feature. EAV cannot reconstruct PRR-only attempts or all raw evidence.

Snapshots reference existing managed `objects/` generations in the dataset;
they do not depend on temporary viewer caches. The snapshot API never deletes
published objects or descriptors and adds no garbage collector. Back up the
whole dataset, including `snapshots/` and referenced `objects/`, to preserve
history. Manual deletion/corruption is detected, not prevented or repaired.
Hashes are integrity checks, not signatures or tamper-proof storage.

Creation holds the same local catalog writer lock as conversion/recovery and
publishes a complete descriptor without replacing an existing one. It requires
filesystem support for the create-only publication mechanism; unsupported
filesystems fail rather than fall back to unsafe overwrite. No power-loss or
network-filesystem durability guarantee is inferred from a successful local test.
Reads are bounded to 16 MiB per manifest; listing examines at most 1,000 directory
entries, including temporary or unrecognized names, and fails explicitly above
that limit. No silent truncation is used.

## Compatibility and current boundary

- `eav-v2`, `verify-dataset`, conversion commits and existing report policies are unchanged.
- `dashboard-dir` still reads the current catalog; it does not yet accept a snapshot ID.
- The library resolves a frozen, verified catalog and its exact fragment inventory.
- Snapshot selection does not yet change the viewer or Dashboard input path.
- Optional [retained evidence attachments](retained-evidence.md) preserve one
  source's native companions and decompressed STDF separately. The original
  snapshot's retention declaration and ID remain unchanged.
- Original compressed-container archival, retention/GC leases and
  snapshot-backed Dashboard queries remain future work. Attempt-yield and
  `analysis-receipt-v1` ship separately (see [cli.md](cli.md)).

## Synthetic demonstration

After building the release CLI:

```powershell
python examples/snapshots/generate_demo.py
```

The demo creates a fresh directory under `target/`, captures S1, updates the same
source and captures S2. It checks that capture leaves S1 selected, explicitly
selects S2, and then verifies and reselects S1 while the current catalog and raw
source are temporarily unavailable. It leaves S2 selected, checks different row
counts and preserves all outputs for inspection. It never overwrites an existing
demo directory. This validates historical EAV selection, not attempt-yield
receipt publication (see `zstdf-cli snapshot attempt-yield` in [cli.md](cli.md)).
