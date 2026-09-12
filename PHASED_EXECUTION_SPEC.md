# zstdf phased execution spec

Status: execution plan for the current repo state

This document is the working implementation spec for the code in this
repository. It narrows the broader `ZSTDF_SPEC.md` and
`implementation_plan.md` into phases that can be implemented and validated
incrementally.

## Current progress

- Coordinate identity update: `eav-v2` adds `part_sequence` and nullable
  `part_merge_key`. `coordinate-v1` supersedes the legacy identity contract for
  both dashboard commands. See `docs/coordinate_identity.md`; original wafer,
  PRR coordinates and PART_ID remain unchanged. Existing eav-v1 data requires
  reconversion into a new output/dataset.

- Phase 1: complete in the active top-level `stdf-core` crate.
- Phase 2: complete in the active top-level `stdf-core` crate as of this pass.
- Phase 3: complete in the active top-level `stdf-core` crate as of this pass.
- Phase 4: complete in the active top-level `stdf-io` crate as of this pass.
- Phase 5: complete in the active top-level `stdf-arrow` crate as of this pass.
- Phase 6: complete in the active top-level `stdf-parquet` and `stdf-py` crates as of this pass.
- Phase 7A: complete as a hardening pass across `stdf-core`, `stdf-io`, `stdf-arrow`, `stdf-parquet`, and `stdf-py`.
- Phase 7B: complete with checked-in fixture corpus policy, deterministic fuzz smoke tooling, and performance baseline tooling.
- Phase 8: complete with CLI workflows, Python packaging metadata fix, CI workflow, and user docs.
- Phase 9A: complete with Parquet-specific performance attribution and profiler workflow.
- Phase 9B: complete with streaming Parquet conversion, atomic output commits, retry-safe manifests, orphan-temp cleanup, and no-overwrite idempotency.
- Phase 10A: complete with self-contained interactive HTML dashboard generation from EAV Parquet.
- Phase 10B: implemented with file-granularity partitioned output, multi-file CLI/Python conversion, streaming gzip, and validated retries. See the scope boundary below.
- Phase 10C.1: implemented for PTR EAV rows with row-level partitioning, bounded state/writers, per-source staged publication, and validated retries.
- Phase 10C.2: implemented with SHA-256 catalog snapshots, explicit recovery, per-file failure reporting, and lot-selectable dataset dashboards.
- Phase 10C.2 validation baseline: 242 workspace tests, desktop/mobile browser smoke, and Windows RSS stress checks. Phase 10C.2 adds 24 regression tests.
- Phase 10D.1: implemented as the `stdf-analytics` bounded external-sort foundation; not yet connected to dashboard commands.
- Next implementation milestone: Phase 10D.2, disk-backed dashboard reducers and small-data analytics parity. Full Phase 10D remains incomplete.
- Direct-STDF traceability is implemented independently of dashboard yield:
  `traceability-v1` retains PRR attempts, coordinate identities, flow states and
  verdict/bin differences. See [traceability](docs/traceability.md).
- Phase 10D.4 has an initial `sanity` CLI implementation; full acceptance remains pending.
  See [sanity usage and coverage](docs/sanity.md). Phases 10D.4–10D.5 below specify CP/FT source sanity, field provenance,
  run-level summaries, compact per-unit record previews, and lossless typed storage optimization.
  Existing validation and EAV types are starting points, not completion of these gates.
- Phases 10E–10I below specify the proposed evidence dataset, retest yield,
  hardware Change Review, capability/MSA, and reliability/stage-gate pipeline.
  These are planned work, not implemented commands or completed validation.

## Why this spec differs

The active workspace now contains top-level crates:

- `stdf-core`
- `stdf-io`
- `stdf-arrow`
- `stdf-parquet`
- `stdf-py`
- `stdf-cli`
- `stdf-validate`
- `stdf-ascii`
- `stdf-analytics` (Phase 10D scratch-storage foundation)

So the practical order is:

1. finish a stable decode core
2. add whole-buffer and streaming APIs on top of that
3. introduce Arrow/Parquet crates only after the decoded record model is stable

This also tightens one correctness point from the existing docs:

- `FAR.CPU_TYPE=1` is treated as big-endian
- `FAR.CPU_TYPE=2` is treated as little-endian
- other values are rejected explicitly in v0.1 instead of being silently
  interpreted as big-endian

## Phase 1: decoder foundation

Goal: make `stdf-core` useful for real metadata and test-result parsing before
building more crates.

Milestone:

- reusable binary field reader
- strict FAR validation
- recoverable whole-buffer decode
- implemented record set:
  `FAR`, `MIR`, `MRR`, `WIR`, `WRR`, `PIR`, `PRR`, `PTR`, `PMR`, `TSR`,
  `HBR`, `SBR`
- unimplemented records preserved as raw payloads

Validation:

- `cargo test -p stdf-core`
- confirm:
  - little-endian and big-endian FAR decode
  - metadata records decode into typed structs
  - PTR optional fields and pass/fail extraction work
  - truncated trailing record returns prior decoded records plus a trailing error

## Phase 2: remaining high-value record coverage

Goal: finish the record family needed for text-dump parity and EAV conversion.

Milestone:

- implement `MPR`, `FTR`, `PCR`, `DTR`, `GDR`
- add shared helpers for nibble arrays, bit arrays, and generic data values
- add record display helpers for plain-text comparison output

Validation:

- `cargo test -p stdf-core`
- add golden byte fixtures for each new record type
- add one mixed-record sample covering `PIR -> PTR/MPR/FTR -> PRR`

## Phase 3: stable core APIs for downstream crates

Goal: expose a downstream-friendly API before introducing I/O and Arrow.

Milestone:

- add iterator-oriented decode entry points
- add decode summary/statistics
- stabilize record structs and error model
- keep graceful truncation behavior

Validation:

- `cargo test -p stdf-core`
- doc-tests or unit tests for:
  - one-shot decode
  - incremental decode
  - summary/error reporting

## Phase 4: I/O crate

Goal: introduce `stdf-io` only after the core binary contract is stable.

Milestone:

- local file reader
- buffered streaming parser
- large-file iteration without whole-file buffering

Validation:

- `cargo test -p stdf-io`
- integration test reading a temp STDF file from disk
- verify output parity against `stdf-core::decode_all`

## Phase 5: Arrow crate

Goal: produce long-format EAV batches from decoded records.

Milestone:

- `stdf-arrow` crate
- context tracking across MIR/WIR/PIR/PRR
- `PTR` EAV rows with room for later `MPR` and `FTR` expansion
- batch flush by configurable row count

Validation:

- `cargo test -p stdf-arrow`
- synthetic flow:
  `FAR -> MIR -> WIR -> PIR -> PTR* -> PRR`
- verify row counts, lot/wafer propagation, and nullable optional fields

Implemented:

- long-format EAV schema where each row is one test result with part context
- `StdfContext` state machine for `MIR`, `WIR`, `PIR`, `PTR`, and `PRR`
- configurable `BatchBuilder` row flushing
- `records_to_batches` convenience API for decoded record iterators

## Phase 6: Parquet + Python

Goal: surface the first end-to-end user workflow.

Milestone:

- `stdf-parquet` crate
- `stdf-py` one-shot file read as PyArrow `RecordBatch` list
- `stdf-py` file-to-Parquet conversion

Validation:

- `cargo test --workspace`
- Rust-level Python wrapper smoke test through `cargo test -p stdf-py`
- verify a converted dataset loads through the Rust Parquet Arrow reader

Implemented:

- `stdf_parquet::write_record_batches`
- `stdf_parquet::write_record_batches_to_path`
- `stdf_parquet::records_to_parquet_path`
- `_zstdf.read_batches(path, batch_size=None)` returning PyArrow `RecordBatch` objects
- `_zstdf.write_parquet(input_path, output_path, batch_size=None, overwrite=None)` returning written row count

## Phase 7: hardening

Goal: make the decoder trustworthy on damaged files and vendor variance.

Milestone:

- fuzz target for `stdf-core`
- golden tests with real vendor files
- performance baseline

Validation:

- `cargo test --workspace`
- fuzz corpus runs without panics
- benchmark results checked into docs

Phase 7A implemented:

- strict `FAR.CPU_TYPE` validation instead of silently guessing byte order
- strict `StdfReader` initialization through `detect_byte_order_result`
- streaming reader error reporting for partial trailing headers
- deterministic malformed-input/noise tests for decode APIs
- Arrow context hardening for interleaved sites, missing `PIR`, empty parts, wafer updates, pass/fail nullability, and batch-size normalization
- Parquet hardening for in-memory output, empty schema files, invalid output paths, decode-error propagation, and multi-batch row preservation
- Python wrapper hardening for invalid paths, empty FAR-only input, zero batch size, and module registration

Phase 7B implemented:

- checked-in synthetic golden corpus under `fixtures/stdf/golden`
- documented fixture and private vendor corpus policy in `fixtures/stdf/README.md`
- golden corpus integration tests, including valid LE/BE, PTR/PRR flow, unknown vendor record, and truncation cases
- deterministic 1,500-case fuzz regression test seeded by checked-in fixtures
- runnable fuzz smoke:
  `cargo run -p stdf-core --bin fuzz_decode -- --iterations 1500 --max-len 1024`
- runnable performance baseline:
  `cargo run -p stdf-core --bin perf_baseline -- --parts 1000 --tests-per-part 5`
- performance baseline documentation and 15% regression policy in `docs/performance_baseline.md`

## Phase 8: packaging, CLI workflows, CI, and docs

Goal: make the current implementation usable through stable commands and
repeatable automation.

Milestone:

- useful `zstdf-cli` commands for inspect/dump/convert workflows
- active `pyproject.toml` points to the top-level Python crate
- CI workflow captures the same quality gates used locally
- user-facing docs explain CLI and Python entry points

Validation:

- `cargo test -p stdf-cli`
- `cargo check --workspace`
- `cargo test --workspace`
- fuzz smoke command from CI
- performance smoke command from CI

Implemented:

- `zstdf-cli info <input>`
- `zstdf-cli dump <input> [--limit N]`
- `zstdf-cli convert <input> <output.parquet> [--batch-size N] [--no-overwrite]`
- `zstdf-cli dashboard <input.parquet> <output.html> [--title TITLE] [--max-correlation-tests N]`
- CLI tests for summary output, dump limit, Parquet conversion, and invalid input
- `pyproject.toml` updated to `stdf-py/Cargo.toml` and `_zstdf`
- GitHub Actions workflow in `.github/workflows/ci.yml`
- CLI/Python usage docs in `docs/cli.md`

## Phase 9A: Parquet performance attribution and profiling workflow

Goal: identify STDF-to-Parquet bottlenecks with a repeatable runner and make
industry-standard profiler workflows straightforward.

Milestone:

- Parquet-specific macro benchmark runner
- stage timing for parse, Arrow build, Parquet write, and Parquet close
- profiler workflow docs for Windows and Linux/WSL
- CI smoke command for the Parquet runner

Validation:

- `cargo test -p stdf-parquet`
- `cargo run -p stdf-parquet --bin parquet_perf -- --parts 1000 --tests-per-part 5 --batch-size 4096`
- `cargo check --workspace`
- `cargo test --workspace`

Implemented:

- `cargo run -p stdf-parquet --bin parquet_perf -- --parts N --tests-per-part N --batch-size N [--output path.parquet]`
- stage metrics: generation, parse, Arrow EAV construction, Parquet write, Parquet close, total rows/sec
- PowerShell helper in `scripts/profile_parquet.ps1`
- profiler guide in `docs/parquet_profiling.md`
- CI Parquet performance smoke step

## Phase 9B: robustness for large files and failed jobs

Goal: make STDF -> Parquet conversion safe under memory pressure, process
crashes, and repeated server jobs.

Milestone:

- stream decoded STDF records into Arrow batches instead of collecting all
  batches before Parquet writing
- write Parquet through a same-directory temporary file and atomically rename
  only after successful writer close/fsync
- write a compact JSON manifest with rows, batches, batch size, and schema
  version after successful commit
- support retry-safe no-overwrite/idempotent conversion paths in CLI and Python
- remove orphaned temporary files from prior crashed attempts before retry

Validation:

- `cargo test -p stdf-arrow`
- `cargo test -p stdf-parquet`
- `cargo test -p stdf-cli`
- `cargo test -p stdf-py`
- `cargo check --workspace`
- `cargo test --workspace`

Implemented:

- `stdf_arrow::record_batches(...)` streaming batch iterator
- `stdf_parquet::write_record_batch_iter(...)` streaming writer
- `stdf_parquet::records_to_parquet_path_atomic(...)`
- `stdf_parquet::AtomicWriteOptions`
- manifest helper `stdf_parquet::manifest_path(...)`
- CLI `--no-overwrite` manifest fast path
- Python `write_parquet(..., overwrite=False)` manifest fast path
- tests for manifest creation, retry idempotency, decode-error cleanup, orphan
  temp cleanup, and no-overwrite behavior with missing original input

## Phase 10A: interactive HTML data dashboard

Goal: turn generated EAV Parquet into an offline, interactive dashboard for
rapid lot/test debug without requiring a Python notebook or web service.

Milestone:

- add CLI dashboard generation from EAV Parquet
- read Parquet batch-by-batch and compute aggregate-only payloads so large raw
  Parquet rows are not retained by the CLI or embedded into the browser
- include yield KPIs, site/wafer yield, hard/soft bin distribution, failure
  Pareto, failure commonality, test correlation, XY failure map, process
  windows, and data-completeness checks
- render a self-contained HTML file with interactive tabs, search, minimum
  failure threshold, commonality dimension filtering, charts, and drilldown

Validation:

- `cargo test -p stdf-cli`
- `cargo check --workspace`
- `cargo test --workspace`

Implemented:

- `stdf-cli/src/dashboard.rs`
- CLI command:
  `zstdf-cli dashboard <input.parquet> <output.html> [--title TITLE] [--max-correlation-tests N]`
- tests for dashboard analytics, correlation detection, safe HTML/JSON
  rendering, and end-to-end CLI generation from a converted Parquet file

## Phase 10B: multi-file conversion and file-granularity partitions

Goal: convert collections of local STDF inputs into predictable Parquet outputs
without buffering all inputs or opening one Parquet writer per source at once.

Milestone implemented:

- Rust `files_to_partitioned_parquet_dir` with per-file and total summaries.
- CLI `convert-many --output-dir DIR [--partition-by lot-id,wafer-id] INPUT...`.
- Python `write_parquet_many(...)` returning `(files, rows)` and releasing the GIL.
- Recursive CLI discovery of `.std`, `.stdf`, `.std.gz`, and `.stdf.gz`; explicit
  files can have other extensions. Canonical paths are sorted and deduplicated;
  directory traversal skips symbolic links to avoid cycles.
- Sequential streaming of both plain and gzip inputs, including gzip magic detection.
- Preflight reads every input before output creation and checks partition values
  against emitted EAV rows. Missing or truncated input prevents the run from writing.
- One output per source. Stable names contain fingerprints of canonical source
  path and raw file contents; basename collisions and reordered/subset retries
  do not reassign outputs. Source changes detected before commit abort the write.
- `input-file`, `lot-id`, and `wafer-id` keys, percent-encoded directory values,
  null/empty sentinels, and Windows case-collision protection.
- Atomic per-file writing and manifests from Phase 9B. Multi-file no-overwrite
  also validates the existing footer, EAV schema, and source/manifest row counts.
- An exclusive dataset lock prevents cooperating multi-file jobs from sharing
  writers or cleaning each other's temporary files. Recovery is explicit after
  a process crash; a lock is never automatically assumed stale.

Scope and tradeoffs:

- This is file-granularity partitioning. A source with more than one selected
  lot/wafer value is rejected with guidance to use `input-file`. Automatic row
  splitting is the proposed Phase 10C milestone, not silently approximated here.
- Conversion reads each source twice (preflight and write). Retry still needs
  the source for validation. This trades throughput for predictable destinations
  and validation before writes.
- Input fingerprints use fixed FNV-1a 128-bit for local identity, not cryptographic
  integrity. Footer validation does not verify every Parquet data page.
- Changed source bytes create a new output identity and retain the previous
  version. Moving a source also changes its identity. There is no automatic
  version selection when globbing every Parquet file in the output directory.
- Writes are atomic per file, not a transaction over the whole dataset. If an
  I/O failure occurs during writing, earlier completed outputs remain reusable.
- Memory still includes active-part test state, Arrow batches, and Parquet row
  groups. `batch_size` is a target row count, not a hard process memory limit.
  Phase 10B does not claim protection from every malformed-file memory exhaustion.

Validation:

- `cargo fmt --all --check`
- `cargo check --workspace --offline`
- `cargo test --workspace --offline`
- 19 dataset tests cover readable totals, partitions, duplicate paths and names,
  reorder/subset stability, retries, changed sources, source mutation before
  commit, invalid/missing/truncated inputs, mixed wafers, escaped/reserved/case
  variants, gzip and damaged trailers, empty data, locks, damaged Parquet, and
  mismatched manifests.
- Three CLI tests cover recursive discovery/deduplication, argument validation,
  and empty directories. Two Python tests cover conversion/retry and invalid
  requests; the existing module registration test checks the new function.
- Smoke workflow: convert two fixture files, rerun with `--no-overwrite`, and
  generate the existing interactive dashboard from an emitted Parquet file.

## Phase 10C: row partitions, memory limits, and dataset versions

Status: 10C.1 and 10C.2 implemented for PTR EAV conversion.

Goal: support multi-wafer/multi-lot sources under a configured resource budget,
with resumable dataset versions that downstream dashboards can consume safely.

Milestone 10C.1: bounded row-level partition writer (implemented)

- Route EAV rows by their actual lot/wafer context, including interleaved sites.
- Add configurable memory, pending-test, open-writer, row-group, and output-file
  limits. Check limits before retaining data; either spill to bounded disk
  storage or return a specific resource-limit error.
- Use a bounded writer cache and numbered immutable fragments. Reopened
  partitions start a new fragment; do not attempt to append to closed Parquet.
- Preserve every completed part's rows and prevent a missing PRR from retaining
  unlimited test results. Report incomplete parts explicitly.

Implementation and scope:

- `bounded_record_batches` applies pending-test and conservative byte reservations
  before retaining tests. It emits one completed part per batch, avoiding an
  unbounded list of completed zero-row parts. Missing PRR, duplicate PIR, malformed
  metadata/test records, and exceeded limits return explicit errors.
- Active parts snapshot lot/wafer metadata at PIR (or their first PTR for an
  implicit part). SDR mappings select the wafer group for each head/site;
  interleaved parts are not reassigned when another wafer starts or finishes.
- The bounded converter currently supports PTR EAV output. MPR and FTR return
  an unsupported-expansion error instead of silently losing measurements; large
  MPR result counts are checked against the pending-test limit first.
- CLI: `convert-partitioned --output-dir DIR [resource limits] INPUT...`.
  Python: `write_parquet_partitioned(...)` returns `(files, rows, fragments)`.
  The older `convert`/`convert-many` entry points retain their existing behavior.
- The writer cache is limited by both open-file count and measured Parquet
  buffer size. Eviction closes an immutable fragment; a later visit creates a
  new numbered fragment. Each fragment contains at most one bounded row group.
- Preflight validates all inputs using bounded conversion. Each source is then
  written under a fresh staging directory, synced, and published by directory
  rename after all fragments and `_SUCCESS.json` close. Handled failures remove
  only the staging directory owned by that invocation.
- Source generation IDs include path, source fingerprint, partition keys, and
  resource options. Repeated invocations validate receipts, safe relative paths,
  fragment schemas, and row counts before reusing a generation. These local
  fingerprints and footer checks are not cryptographic data-page verification.
- Metadata, pending parts, Arrow copies, writer buffers, and encoding reservations
  have budget allocations. Limits govern accounted state, not an OS-enforced RSS
  ceiling. Decoder scratch space and allocator/runtime overhead still exist.
  Oversized parts fail; disk spilling is not implemented. Input/root paths are
  capped at 1024 encoded bytes to bound retained path metadata.
- Interrupted processes may leave a root lock and unpublished `.staging-*`
  directories. Do not glob those fragments into a dataset; explicit recovery
  and a catalog selecting current generations belong to 10C.2.

Validation for 10C.1:

- Mixed lots/wafers, interleaved sites, and repeated partition visits must have
  exact row/value parity with the unpartitioned EAV output.
- Generate many more partitions than the writer limit; verify open handles and
  buffered bytes stay within configured limits and all fragments remain readable.
- Stress a single oversized part, missing PRR, huge MPR arrays, tiny budgets,
  and highly compressed gzip. Measure peak RSS against a documented allowance
  for runtime/allocator overhead; require bounded failure or spill, never OOM.

Validation executed:

- `cargo test --workspace --offline` (218 tests, including 18 added in 10C.1).
- New cases cover pending tests across sites, tiny budgets, duplicate PIR,
  incomplete parts, lot/wafer snapshots, same-head concurrent wafer groups,
  row/value parity, writer eviction/revisits, capped row groups, fragment-limit
  cleanup, safe retry receipts, gzip missing PRR, malformed PTR, oversized MPR,
  empty sources, and CLI/Python conversion and retries.
- `scripts/measure_partition_memory.ps1` generates gzip inputs incrementally and
  measures the actual CLI process. On the local Windows debug build, a 16 MiB
  accounted budget used peak RSS of 11.62 MiB for 2,000 completed parts and
  11.37 MiB for 20,000. A 1,000,000-PTR input without PRR failed at 129 pending
  tests with a limit of 128, used 7.33 MiB peak RSS, and created no output.
- The repeatable stress gate allows baseline RSS plus 16 MiB budget and 64 MiB
  runtime/allocator variation. These synthetic results are a regression baseline,
  not proof of a hard RSS ceiling on all vendor inputs.

Milestone 10C.2: versioned catalog and recoverable runs (implemented)

- Atomically publish a dataset catalog with cryptographic source/output hashes,
  schema/options version, source identity, fragment paths, rows, and run status.
- Record which output version is current per source. Keep older files available
  but exclude them from the catalog's current snapshot to prevent double counting.
- Add explicit resume/retry and stale-lock recovery with owner/process checks.
  Track per-file errors and define fail-fast versus continue-on-error behavior.
- Let dashboards consume the catalog snapshot, followed by a `dashboard-dir`
  workflow with lot selection. Part keys now follow `coordinate-v1`; only
  unresolved attempts retain source scoping.

Validation for 10C.2:

- Inject failures before close, after Parquet commit, during catalog publication,
  and during retry. Resume must produce one current version without lost rows.
- Test concurrent jobs, stale versus active locks, changed inputs/options/schema,
  corrupt output pages, disk-full errors, and missing fragments.
- Dashboard totals from the catalog must equal the sum of current source versions;
  identical part IDs across sources and superseded outputs must not merge or
  double-count parts.

Implementation and scope for 10C.2:

- CLI `convert-partitioned` and Python `write_parquet_partitioned` now write
  `_catalog.json` above immutable `objects/<generation>/source-...` fragments.
  Python signatures and tuple results remain unchanged. The lower-level Rust
  `files_to_partitioned_fragments` API remains available without a catalog.
- Catalog version 1 records EAV schema version, options hash, canonical source
  identity, SHA-256 source/output hashes, current fragments, rows, revision, and
  latest run status. Source updates replace one catalog entry, not old files.
- Per-source publication is atomic, not a whole-run transaction. Fail-fast is
  default. `--continue-on-error` records failures, converts remaining inputs,
  and still exits nonzero. Failed updates retain the last successful version;
  the dashboard explicitly warns when the latest run is incomplete.
- Repeating conversion resumes valid immutable generations. Version-2 fragment
  receipts verify entire output files, not just footers. Older raw generations
  are not automatically imported: rerun conversion to build a managed catalog.
- `.catalog.guard` uses a kernel file lock that releases on process exit. Keep
  the persistent guard file; deleting it can break locking. `recover-dataset`
  holds this lock, refuses active/unknown legacy owners, removes abandoned
  staging/catalog temp files, and marks a running catalog as interrupted.
  Published generations are never removed by recovery.
- `verify-dataset` validates SHA-256, paths, unique fragments, EAV schemas, and
  row totals. It reports run status separately from snapshot integrity.
- `dashboard-dir` verifies the snapshot, scopes part IDs by source, aggregates
  across fragments, and embeds All-lot and per-lot views. Switching lots refreshes
  every chart. Memory/lot/part limits fail before replacing existing HTML.
- Conversion reserves one quarter of its accounted budget for catalog work;
  catalog JSON is capped at min(memory/64, 16 MiB), with capped serialization.
  Dashboard analysis uses conservative cumulative row/string reservations.
  Neither is an OS-enforced RSS ceiling. Parquet reader scratch/allocator
  overhead requires headroom; dashboards reject oversized workloads rather
  than spilling. A 256 MiB dashboard budget fits roughly 30,000 short rows.
- This targets cooperating local writers on ordinary local filesystems. Network
  filesystem lock/durability guarantees and physical power-loss behavior are
  not certified. Hashes detect accidental corruption; an attacker able to rewrite
  both data and catalog is outside this integrity model. Retained generations
  require disk capacity; automatic garbage collection is not implemented.
- Existing dashboard analytics still group tests by test number and aggregate XY
  across the selected lots/wafers. Part merging now uses `coordinate-v1`.
  They are not yet a retest-aware or test-program-version-aware analysis model.
- Tested with Rust 1.97.1 on Windows; native file locks require a sufficiently
  recent Rust toolchain (the workspace does not yet advertise a tested MSRV).

Validation executed for 10C.2:

- 242 workspace tests pass, including publication failure injection, failure
  before fragment close and after source commit, retry, active/dead/unknown
  owners, lock exclusion, metadata bounds, source/options changes, corruption,
  missing files, partial runs, scoped identities, and script-safe lot labels.
- `scripts/smoke_dataset_dashboard.cjs` passes in headless Edge at 1440x900 and
  390x844: lot yields, tab switching, search, no horizontal overflow, and no JS
  runtime errors. Requires Playwright on NODE_PATH and installed Edge.
- Updated `scripts/measure_partition_memory.ps1` passes with catalog conversion:
  2,000 parts used 11.41 MiB peak RSS; 20,000 used 11.77 MiB. A million PTRs
  without PRR failed at the pending-test bound, used 8.02 MiB, and published no
  source/fragments. Its diagnostic catalog correctly records failure.
- Fault injection models selected I/O/crash boundaries; it is not a physical
  disk-full or host-power-loss test.

## Phase 10D: scalable dataset analytics (in progress)

Milestone: replace cumulative in-memory part/result retention with a bounded
disk-backed aggregation stage, while preserving catalog snapshot semantics and
the self-contained HTML interface. Add an explicit disk budget and cancellation
cleanup; do not simply raise the current dashboard memory defaults.

Validation before implementation:

- Golden parity for yield, Pareto, commonality, correlations, and lot selection
  against the current small-data implementation.
- Million-row and high-cardinality fixtures must stay within measured memory
  allowances; disk-full, cancellation, and process-kill tests preserve the prior
  HTML/catalog and clean only owned temporary state.
- Define retest/part-instance and test-program identity in a versioned schema
  before changing counting semantics. Include repeated PART_ID, reused test
  numbers with different units/limits, and per-wafer XY selection regressions.
- Retain explicit PTR-only behavior in bounded conversion until a separately
  validated MPR/FTR expansion milestone is implemented.

### 10D.1: bounded scratch-store foundation (implemented)

The new `stdf-analytics` crate provides stable external sorting of binary
key/value records without introducing a database runtime. Memory limits bound
accounted chunk/front state, disk limits include live merge inputs and output,
and fan-in limits open input files. Run metadata uses numeric ranges rather than
an unbounded vector of paths. Identical keys retain ingestion order.

Cancellation is cooperative through `Arc<AtomicBool>` during ingestion, merge,
and replay. Handled failures and normal drop clean only the invocation's owned
scratch directory. Corrupt lengths are rejected before allocation, truncated
records fail rather than silently ending replay, and an errored store cannot
be reused. There is no publication or mutation of dataset/HTML paths.

Scope boundary: this is a storage API, not a dashboard engine. Existing
`dashboard-dir` memory/part limits and counting behavior are unchanged. Hard
process termination can leave scratch directories; automatic stale-job recovery,
OS signal wiring, real disk-full testing, and measured RSS gates remain pending.
Scratch bytes are not filesystem allocation/quota bytes, and memory accounting
is not a process RSS ceiling. Callers must stream replay rather than collect it.

Validation:

- `cargo test -p stdf-analytics --offline`: 19 tests, including multi-pass stable
  sorting, duplicate/binary keys, quotas during merge, cancellation, invalid
  configuration, corrupt/truncated records, missing runs, and isolated cleanup.
- `cargo run -p stdf-analytics --bin spill_stress -- 100000`: bounded scratch
  stress with row/order validation and cleanup verification.
- `scripts/measure_analytics_memory.ps1`: repeatable Windows stress/RSS sampling
  at 100,000 and 1,000,000 records, with configurable 64 MiB absolute/16 MiB
  growth regression allowances. These are runtime headroom gates, not memory
  guarantees for a future dashboard or arbitrary record distributions.
- Existing CLI tests remain the compatibility baseline; no analytics result
  parity claim is made until integration in 10D.2.

### 10D.2: disk-backed dashboard reducers (next)

Milestone: use the scratch store to group complete part identities across
catalog fragments, reduce part yield and correlation observations incrementally,
and retain only explicitly bounded aggregate/output state. Add CLI scratch-path
and disk-budget options only when the dashboard actually consumes them.

Current contract `coordinate-v1` uses eav-v2 `part_merge_key`: a valid uppercase
alphanumeric wafer and positive PRR X/Y, otherwise lot and unambiguous positive
integer PTR X/Y. Resolved identities merge across source/lot (for wafer keys),
PART_ID and head/site. Null merge keys use `(source, part_sequence)` and remain
separate. The prior `analytics-identity-v1` tuple contract is superseded by this
explicit schema/identity version; do not reproduce it in the new reducers.
Part pass remains a conjunction, metadata comes from the first row, correlation
uses the first finite result per test number, and tests remain grouped by number.
These policies are not first/final-retest or test-program-aware analytics.

Validation: compare every All-lot/per-lot payload field (excluding timestamps)
against the existing implementation for yield, Pareto, commonality, correlations,
quality, bins, process windows, and XY maps. Include fragment splits, repeated
PART_ID, identical IDs across sources, reused test numbers with different
units/limits, null/empty wafer IDs, missing/nonfinite results, and changed catalog
versions. Preserve existing HTML on every handled failure.

### 10D.3: operational qualification (pending)

Milestone: explicit stale scratch recovery with owner/lock checks, CLI cancellation,
disk exhaustion handling, and process-kill recovery. Validate million-row and
high-cardinality dashboards with measured RSS and disk peaks. Add per-wafer XY
selection with separate parity/UI tests. Do not remove the existing safeguards
or advertise unbounded-size dashboard support until these gates pass.

### 10D.4: CP/FT source sanity and field inspection (initial implementation; full acceptance pending)

当前首版已实现 `sanity --test-domain cp|ft`、基础 v4 字段布局检查、原始证据保存、
run 重要字段、unit 各类型前两条首值预览、profile 字段规则及原子 HTML 发布。
具体可运行接口和限制以 [docs/sanity.md](docs/sanity.md) 为准。以下仍保留完整目标合同；
已补齐混合 CP/FT run-profile 唯一匹配及 PTR/MPR 首次定义的数值/字符串继承；
剩余包括完整标准规则/计数对账、FTR 定义继承、独立 inventory/findings
Parquet 表以及大规模资源验收尚未完成，不能将整个 10D.4 标记为已验收。

**目标：** 在数据进入分析层前，检查完整 STDF 的记录、字段、上下文和产品格式；
展示每个 test run 的重要字段，以及每个 unit 内各类记录前两条的首值，帮助工程师检查
tester 输出。预览与全文件检查分别报告，不再要求展示前两个完整测试实例。

#### 10D.4.1 字段证据与判定合同

检查以 STDF v4 原始记录和字节为依据，不能只检查现有 EAV/PTR 行。
字段描述表由标准逐记录维护，包含字段顺序、STDF 类型、长度/计数来源、允许省略条件、
缺失哨兵、有效性 flag、合法枚举、默认/继承规则及标准条款。发布时固定规则版本和 hash。
v4-2007 扩展必须单独声明支持范围，不能用扩展的字段和省略规则替代基础 v4。

每个字段生成 `FieldEvidence`，主键为
`(source_id, record_offset, field_path, element_ordinal)`；数组保留元素序号。
保存记录类型、字段类型、原始字节范围、原始值、有效值、上下文引用和规则结果。
字段缺失仍有证据行，不能因为没有值就不输出。

| 维度 | 值与判定要求 |
| --- | --- |
| `presence` | `present / omitted / truncated`；区分合法省略整个尾部字段与字段已经开始但字节不足 |
| `value_origin` | `explicit / standard_default / inherited / unresolved`；explicit 仅指字节中有值，不代表人工输入 |
| `matches_default` | `true / false / unknown`；显式值恰好等于标准默认值，仍然是 explicit |
| `semantic_status` | `valid / missing / invalid / unknown`；根据该字段的哨兵、枚举和 flags 判断，不统一把 0、空白或 -1 当成坏值 |
| `profile_status` | `pass / fail / not_applicable / not_evaluated / unknown`；CP/FT 产品格式、关联条件与标准格式分别判断 |
| `effective_value` | 仅在规则可确定时生成；附默认规则 ID 或被继承字段的 source/offset，原始值不被覆盖 |

STDF 自身不能证明一个值由操作员输入还是 tester 自动填入。如需要这种来源，需要
tester 配置/操作日志的显式关联；报告不得将“非默认值”标成“已确认人工输入”。
标准的缺失标记也不是可用于统计的正常默认数值。

#### 10D.4.2 覆盖范围与 CP/FT profile

完整扫描基础 v4 的 FAR、ATR、MIR、MRR、PCR、HBR、SBR、PMR、PGR、PLR、RDR、SDR、
WIR、WRR、WCR、PIR、PRR、TSR、PTR、MPR、FTR、BPS、EPS、GDR、DTR；提取所有已定义
字段，含 header、flags、数组、文本、限值、scale、单位和不参与当前 Dashboard 的信息。
维护“标准记录/字段 → 解码与验证覆盖”清单；未覆盖项必须显示 unsupported，不能显示通过。
未知厂商记录保留 type/subtype、长度、offset 和原始 payload；已知记录解码失败单独报错，
不能与未知扩展混为一类。未知记录阻断相关分析的完整性声明，是否允许其他分析由 profile 决定。

| 检查层 | CP | FT |
| --- | --- | --- |
| 字节和记录 | 两者均检查 FAR/版本/字节序、REC_LEN、字段边界、数组计数、有效 flags、记录顺序和闭合关系 | 与 CP 相同，不因 FT 放宽解码错误 |
| run 信息 | MIR 的 lot、产品、程序名/版本、步骤、温度；SDR 的 tester/head/site 及适用硬件标识 | 相同公共信息；handler、load board、socket 等要求由 profile 指定 |
| wafer 上下文 | 检查 WIR/WRR/WCR 与 head/site group 的关联、wafer ID、坐标和适用计数；生产 CP profile 可要求 wafer 信息 | 无 wafer 记录可为不适用，不因为缺 WIR/WRR 自动判坏；有记录时仍检查其有效性 |
| 器件关联 | 分 head/site 维护 PIR–测试记录–PRR；检查坐标身份是否可解析 | 同样保留每次 PRR；PART_ID/序列号格式按产品配置，跨 CP/FT 关联需要显式可靠身份 |
| 产品格式 | lot/wafer、程序/版本、测试名称、单位、限值等满足配置的规则 | lot/package/序列号、程序/版本、测试名称、单位、限值等满足配置的规则 |

CP/FT 由显式 `test_domain` 指定；混合输入使用按 source/run 精确匹配的配置，每个 run
必须唯一匹配。WIR 是否存在、文件名或 MIR 文本只能提供提示，不能据此自动宣布 profile 通过。
标准允许的 I*2 负坐标与 `coordinate-v1` 要求正整数属于不同层次；例如 -32768 的缺失含义、
合法负坐标以及产品要求不满足要分别展示。FT 无可解析坐标时保留独立实例及关联限制。

格式规则包括显式全字符串匹配、允许值集合、数值范围、必需字段、跨记录相等关系、
测试定义及其变化范围；规则可按程序/版本/步骤选择。正则长度和执行资源受限。
预览值相同只显示 observed consistency，不自动生成全产品规则；没有配置的产品格式为
`not_evaluated`，不能从样本推断“符合格式”。所有规则都需保存规则 ID、适用范围和严重性。
PCR/WRR/PRR/TSR 的计数对账必须先解释 head/site、重测及标准计数语义；不简单令
PRR.NUM_TEST 等于 PTR 数，也不把一个 MPR 的每个结果都算作一次独立测试。

#### 10D.4.3 Run 重要字段与 unit 首值预览

展示分为两个层级，重复上传按 source 内容去重。完整记录和字段继续检查、落盘，HTML
默认使用下面的摘要，不展开两个完整器件测试实例。

**Per test run：** 每个 run 显示 MIR 等记录的重要值，列出原始值、有效值、来源状态、
标准/产品规则结果和 record offset。重要字段由版本化 CP/FT profile 配置，默认包括：

| 记录/上下文 | 默认展示的重要字段 |
| --- | --- |
| FAR | CPU_TYPE、STDF_VER |
| MIR | LOT_ID、PART_TYP、JOB_NAM、JOB_REV、SBLOT_ID、TEST_COD、OPER_NAM、FLOW_ID、TST_TEMP、SETUP_T、START_T、STAT_NUM、MODE_COD、RTST_COD、NODE_NAM、TSTR_TYP、DATE_COD、FACIL_ID、FLOOR_ID、PROC_ID |
| SDR | HEAD_NUM、SITE_GRP、SITE_NUM，SITE_CNT、HAND_TYP、HAND_ID、 EXTR_ID |
| WIR/WRR/WCR（适用时） | wafer ID、head/site group、开始/结束时间、计数及坐标系/wafer 配置信息；各字段注明所属记录 |
| SBR | HEAD_NUM、SITE_NUM、SBIN_NUM、SBIN_CNT、SBIN_PF、SBIN_NAM |
| MRR/PCR | run 结束时间/状态、适用范围内的 part/retest/good/abort 计数及完整性诊断 |
| PRR（unit 摘要） | HEAD_NUM、SITE_NUM、PART_ID、PART_FLG、HARD_BIN、SOFT_BIN、坐标及闭合状态 |

同一 run 内有多个 wafer、site group 或上下文版本时分行显示，不能只取第一条或最后一条
覆盖其他值。run 信息不在每个 unit 下重复展开；unit 仅引用自己的有效上下文。

**Per unit：** unit 在此指一次独立测试实例，使用 source/run/head/site/part_sequence
定位；同一芯片 retest、重复 PART_ID 均独立保留。unit 列表支持筛选/分页，显示简短的
身份、PRR 判定和 bin 摘要；选中 unit 后，DTR、PTR、MPR、FTR、GDR 等按记录类型分别
按原始 offset 取前两条记录，每条只展示下表定义的首值及其有效性，不展示完整测试历史。
“前两条”不是两个 unit，也不是每个 test number 各取两条；同号重复记录不合并。

| 类型 | 每条记录的首值 | 随值保留的最小上下文 |
| --- | --- | --- |
| DTR | TEXT 文本值，保留整条文本，不取第一个字符 | offset、归属依据、字符/格式状态 |
| PTR | RESULT 标量 | TEST_NUM、TEST_TXT、UNITS、有效性及默认/继承来源 |
| MPR | RTN_RSLT[0] | TEST_NUM、TEST_TXT、UNITS、结果个数、有效性；空数组标 empty，不改取其他字段 |
| FTR | TEST_FLG 派生的有效 Pass/Fail/Unknown | TEST_NUM、TEST_TXT、原始 flag；无浮点结果时不虚构数值 |
| GDR | GEN_DATA 首个非填充数据元素的带类型值 | 元素 ordinal、类型 tag、元素数；空数据标 empty，填充项不冒充数据值 |

例如某 unit 的前三条 MPR 结果分别为 `[1.25, 1.26]`、`[2.5, 2.6]`、`[3.0]`，
预览仅显示 `1.25` 和 `2.5`，并标明“展示 2/3 条记录，每条仅首值”；其余元素仍参与 sanity。
第一条记录/首个结果无效时直接显示 invalid，不跳过它再找一个有效值；只有零/一条时
显示实际数量，不能补齐或跨 unit 借值。

DTR/GDR/BPS/EPS 等没有 head/site 的记录只在归属能够可靠确定时进入 unit 预览；
多 site 交错时不能凭相邻位置猜归属，未确定的记录在 run 上下文中显示并注明关联限制。
每个预览值可查到字段来源和诊断，全部原始记录/数组明细保留在字段证据导出中。

报告另提供全文件记录清单、字段状态分布、异常列表及按 CP/FT、run、record、field、
严重性筛选；错误需能下钻到 source/record/field offset。
HTML 中的控制字符、无效编码、`</script>` 等必须安全显示；原始字节单独按 hex 导出。

#### 10D.4.4 实现顺序、接口和失败语义

1. 为 `stdf-core::FieldReader` 增加可选字段访问追踪，记录消费字节、长度前缀和类型。
   现有 `from_utf8_lossy` 展示不能作为原始字符证据；保留 raw bytes，并独立检查标准字符
   约束和解码状态。合法省略与半个 I*2、截断 C*n/数组分别测试，不以 `remaining()` 不足一律当 None。
2. 在 `stdf-io` 提供包含 offset/raw payload/decode status 的事件流。已知类型解析失败
   不得仅转换成没有诊断的 Unknown。无法信任下一条记录边界时停止，不猜测字节重同步。
3. 在 `stdf-validate` 扩展版本化字段规则和 profile evaluator；把当前单个
   `open_pir_offset` 状态改为按 run/head/site 维护。默认/继承按具体记录字段的标准作用域
   解析，PTR/MPR 定义不能跨 source/run/program 意外复用，也不能用通用 forward-fill 替代。
4. 全量 `record_inventory`、`record_fields`、`findings` 流式落盘；统计状态和 run/unit 摘要
   也受预算约束，使用已有 `stdf-analytics` spill 能力处理大实例和高基数。不能把全文件
   findings 无限制累积在 Vec，或因为已取到两条预览记录就提前结束全文件验证。
5. 新增拟定命令 `zstdf-cli sanity <inputs...> --test-domain cp|ft --profile <json>
   --preview-records-per-type 2 --output-dir <dir>`；文件、目录、gzip 均支持。混合 domain 时改用
   `--run-profiles <json>`，与 `--test-domain` 互斥。现有 validate/convert 接口保持兼容。
6. 输出版本化 bundle：`report.html`、`summary.json`、`record_inventory.parquet`、
   `record_fields.parquet`、`findings.parquet`、原始证据 blobs 和规则快照，由 manifest 记录
   hashes、扫描范围、完整性及结果。HTML 离线内嵌 run 重要字段、unit 首值预览和检查摘要，完整字段表在 bundle
   中可单独读取；超过报告预算明确失败，不靠静默截断得到“完整报告”。

`validation_failed` 是可交付的诊断结果：发布带失败状态的完整诊断 bundle 并返回非零退出码；
遇到损坏记录可以发布 `scan_complete=false` 的诊断，但不能声称未扫描部分有效。
运行层的写入失败、资源超限和取消不发布，已有 bundle 保持完整。通过 generation 目录+
原子 manifest 指针发布，不能逐个覆盖正在被读者使用的文件。
10E ingest 使用同一验证事件流和固定 profile；失败/不完整输入不能成为已验证分析快照。
sanity 的 raw blob 已按记录存一次，字段表引用范围，避免每个字段重复存整条记录。

#### 10D.4.5 正确性验证与交付

| 编号 | 场景 | 必须满足的结果 |
| --- | --- | --- |
| S01 | 各基础 v4 记录含非默认字段、大小端输入 | 所有字段/数组与手工标注字节 offset、类型和值一致；覆盖清单无未解释缺口 |
| S02 | 合法尾部省略、显式默认、显式空串、无效 flag、合法零值 | presence/origin/semantic 分别准确；相同 effective value 不抹去不同来源 |
| S03 | 半个数值、字符串前缀大于剩余长度、数组计数不符、损坏 header | 解码错误有精确位置；不能冒充 omitted/Unknown/通过；不安全边界后停止 |
| S04 | 合规 CP、无 wafer 的合规 FT、CP 缺必需 wafer 信息、混合文件 | 正确 profile 生效；FT 不因不适用记录报假错；歧义/未映射不通过 |
| S05 | PIR A、PIR B、交错 PTR/MPR/FTR、PRR B、PRR A | 各 unit 各类型前两条按 offset 取首值，值不串 site；无 site 记录不猜测绑定 |
| S06 | 同一 PART_ID 两次、无 PTR、某类型零/一条记录、孤立 PIR | 实例独立；无 PTR 仍显示 unit 判定摘要；预览不补齐，未闭合单独报告 |
| S07 | PTR 默认定义、省略、合法覆盖，随后新 run 重用 test number | 按标准定义来源解析，跨 run 不泄漏；raw/effective 都能还原 |
| S08 | 乱码/控制字符、产品 regex 不符、合法负坐标、坐标回退冲突 | 标准、编码、产品、身份四层诊断不混淆；报告无注入，原始字节不丢失 |
| S09 | 前两条首值正常，但第三条记录或 MPR 后续元素错误；后部未知/损坏记录 | 全文件诊断覆盖未展示部分，不能由预览宣布通过 |
| S10 | 高基数、单器件海量测试、磁盘满、取消、发布失败 | 遵守资源门槛，记录数可对账，旧 bundle 不损坏，无静默截断 |
| S11 | 多 run、多 wafer/SDR 上下文；MIR 重要值显式默认或缺失 | 各 run 重要字段及状态正确；unit 引用对应上下文，不被最后一条全局覆盖 |
| S12 | MPR 多结果/空数组/首值无效，DTR 文本，GDR 填充/类型 tag，FTR 未知判定 | 严格按首值合同展示，保留记录数/元素数；不跳过无效值，不展开完整实例 |

交付：CP/FT 各一份版本化 profile 和合成 STDF、完整字段类型/规则清单、run/unit 摘要报告、
逐字节独立预期、CLI 文档与筛选/下钻/导出浏览器验证。所有标准条款用原文人工核对；
独立解码器可交叉检查，但不能把另一个解析器的容错行为视为标准定义。

### 10D.5: lossless STDF-to-Arrow/Parquet storage optimization (planned)

**目标：** 以字段真实类型和使用频率优化存储，保持值、精度、缺失状态、flags、身份和
记录来源可追溯。整数按位宽/符号区分；“单精度/双精度”用于 R*4/R*8 浮点数。
不能因为预览中的数值小或看起来都是整数，就缩窄整列或改成整数。

当前 `stdf-arrow/src/schema.rs` 已将 PTR result/limits 存为 Float32，head/site 为 UInt8、
坐标为 Int16、bin 为 UInt16、test number 为 UInt32；因此不是一次统一 Float64→Float32
的改造。`stdf-parquet/src/dataset/fragments.rs` 当前明确关闭 dictionary，优化必须同时
满足原来的有界内存要求；单文件与 dataset writer 的实际 codec/encoding 从 footer 建基线。

#### 10D.5.1 类型映射与无损合同

下表定义新 evidence/字段证据的默认逻辑类型；eav-v2 对外列类型保持现状。
实际 schema 由字段描述表生成/核对，不用数据采样猜类型。

| STDF 类型/信息 | Arrow / Parquet 目标 | 验证与保真要求 |
| --- | --- | --- |
| U*1 / U*2 / U*4 | UInt8 / UInt16 / UInt32；Parquet INT32 + 对应 unsigned 逻辑注解 | 保留 0 到各自上界；哨兵另标状态，原始数值不删除 |
| I*1 / I*2 / I*4 | Int8 / Int16 / Int32；INT32 + 对应 signed 注解 | 保留负值和上下界；不能转成无符号省略负值 |
| R*4 / R*8 | Float32 / Float64；FLOAT / DOUBLE | R*8 不缩成 R*4；整值浮点仍是浮点；原始 bits 与有效值分离 |
| C*1 / C*n / 固定长度字符 | 可验证文本用 Utf8/STRING；原始编码不合规时保留 Binary 证据 | 长度按字节；保留前导 0、空串、空格、大小写，不用 lossy 字符串替代原值 |
| B*1 / 固定长度二进制 | UInt8 flags / FixedSizeBinary 或 Binary | 保存全部位，派生 bool 不替代原 flags |
| B*n / D*n | Binary + 必需长度/bit_count | bit_count 不等于 byte_count；尾部填充位检查与原始字节分别保留 |
| N*1、kxTYPE | UInt8（0..15）或带元素数的打包 Binary；其他数组为 typed List/子表 | 半字节顺序、奇数尾部、数组顺序与计数不变；不能统一转 JSON 文本 |
| GDR 的 V*n | tag + 对应类型值/子表 + ordinal | R*8、整数、字符、位串分别存储；不能全部转 Float64 或丢失 tag |
| STDF 时间 U*4 | 原始 UInt32 + 可选派生 UTC timestamp | 原始哨兵与转换状态保留；不使用 INT96，不擅自附加本地时区 |
| 生成的 offset/sequence/ID | UInt64 / 固定 32 字节 hash / 规范化身份维表 | 这是内部模型类型，不宣称基础 v4 存在 U*8 字段；不得截断 hash |

Parquet 的 8/16 位整数逻辑注解仍以 INT32 为物理类型，不能宣称每个 UInt8 值在磁盘上
必然只占一字节；实际大小取决于页编码、字典、压缩和元数据。
NaN payload、正负零、Infinity、subnormal 的原始位表示需逐位 round-trip 验证。
若某条 Arrow/Parquet 路径会规范化这些表示，用有校验的原始记录引用或 raw-bit 例外表恢复；
其空间必须计入总成本。有效分析值可以为 null，但原始数值和失效原因必须仍能查到。
JSON 导出用带类型的无损表示处理 UInt64、大整数和非有限浮点，不能经浏览器 Number 丢精度。

#### 10D.5.2 实现与测量步骤

1. **建立基线。** 固定 fixtures、依赖版本、schema、row group、硬件和运行方式；列出
   各列类型、null 数、基数、压缩前后字节、dictionary/page encoding 和总文件大小。
   覆盖短文件、大量 PTR、MPR/FTR/GDR、R*8、低/高基数字符串及异常 raw bytes。
2. **减少重复元数据。** 在 10E 表模型落地时将 run、程序、单位/限值定义、硬件快照等
   重复信息放入版本化维表，measurement 引用 ID；用维表重建每行的完整上下文。
   可使用快照内 UInt32/UInt64 surrogate 减少宽 hash 外键重复，但公开逻辑 ID 不变，
   映射必须双向唯一、有界并可校验；不能因优化把同号异义测试项合并。
3. **按列配置 writer。** 比较无压缩、Snappy、ZSTD 的可用实现与配置，针对低基数列
   评估 dictionary，针对数值评估库支持的适用 encoding。明确 dictionary/page/row-group
   上限；高基数自动退回可控编码，内存核算覆盖字典、页缓冲和同时打开的 writer。
   不对原始测量做有损量化、舍入、小数位截断或未经约定的单位归一化。
4. **统一配置路径。** 在 `stdf-parquet` 集中实现带版本的 `StorageProfile`，让单文件、
   dataset、新 evidence writer 共用约束；在 manifest 保存 profile/hash、codec、编码、
   writer 版本和 schema。10D.4 初版字段 bundle 可先用正确的默认布局，优化后验证等价。
5. **读回和下游 parity。** 新旧 profile 全量读回比较类型、字段状态、原始 bits、数组、
   身份、全部 measurement 与 PRR 次数，再比较 Dashboard/traceability 输出。更换 codec
   不改变逻辑 schema；改变列合同/表布局才升级 schema，并写新目录，不能原地改旧数据。
6. **选择发布配置。** 发布逐类数据的大小、写入/扫描时间、peak RSS、scratch 峰值、
   footer 和原始字节校验报告。先交付显式可选 profile，证明确有收益且没有资源回归后
   再变更默认值；不承诺一个未经实测的固定压缩比例。

统计空间时包括所有 Parquet、维表、索引、catalog、raw-bit 例外和必需证据 blob；
可选原始 STDF 归档另外列出，并报告含归档总量。对比必须是同一证据覆盖范围：不能通过
丢弃 MPR/FTR 或原始字段，再与完整存储比较，宣称获得无损压缩收益。
每组至少重复运行并公布中位数与离散范围；冷热读取分别测量，不混用缓存结果。
资源上限沿用 10D.3；压缩收益/读写耗时的验收门槛在 benchmark 配置中预先固定。
达不到门槛的配置保留为实验结果，不默认启用，也不以“小数四舍五入后相同”通过正确性门槛。

#### 10D.5.3 验收案例与交付

| 编号 | 场景 | 必须满足的结果 |
| --- | --- | --- |
| P01 | 全类型边界、大小端、U*4 最大值、生成的 UInt64 大值 | STDF→Arrow→Parquet→导出保持类型和值，footer signedness 正确，无溢出 |
| P02 | R*4/R*8 的极值、相邻浮点、NaN payload、±0、±Infinity、subnormal | 原始 bits 可逐位恢复；有效性独立；R*8 不被悄悄降精度 |
| P03 | 空串/省略/空格、前导 0、非 ASCII/坏编码、显式默认和继承 | raw bytes 与 10D.4 的状态一致，字典不把不同状态合并 |
| P04 | nibble 奇数个、非整字节 D*n、空数组、GDR 混合类型 | 元素数、顺序、tag、填充位和原始证据保真 |
| P05 | 高基数字符串、字典上限、长单器件、多个 writer | 达到阈值正常回退或明确超限；无无界字典、无静默记录丢失 |
| P06 | 不同 row group、编码、codec、线程及输入顺序 | 逻辑快照/身份/计数与下游结果一致，物理文件 hash 可不同 |
| P07 | 新旧布局并存、旧 manifest、未知 profile/schema、取消和磁盘满 | 兼容配置正确读取；不兼容明确拒绝；旧数据和报告保持完整 |
| P08 | 代表性基准含所有必需证据对象 | 公布总空间和读写/RSS数据，按预设门槛决定默认配置，不只挑收益最大的一列 |

交付：字段映射清单、StorageProfile 示例、独立 bit-level fixtures、逐列 footer 对比、
可重跑 benchmark、全量无损/parity 报告和迁移说明。文档中的目标类型不是已经测得的空间收益。

#### 10D.4–10D.5 标准与实现参考

- [Teradyne STDF v4 specification](https://storage.googleapis.com/google-code-archive-downloads/v2/code.google.com/stdf-eclipse/Stdf-V4-spec.pdf)：基础记录、字段类型、缺失和默认规则的规范依据。
- [STDF v4-2007 extension specification](https://www.roos.com/roos/documentation.nsf/3d6a93a7e05462cf85256a9c007dcaf3/92102f712ce51df48825783800832332/%24FILE/STDF%20Spec%20V4%202007.pdf)：仅用于明确扩展边界，不将扩展视为基础 v4 已支持。
- [PySTDF V4 record definitions](https://github.com/cmars/pystdf/blob/master/pystdf/V4.py)：独立字段描述的交叉检查参考；测试时固定版本，不能替代规范。
- [Parquet physical types](https://parquet.apache.org/docs/file-format/types/) 与 [logical types](https://parquet.apache.org/docs/file-format/types/logicaltypes/)：物理宽度、整数注解和文本表示依据。
- [Parquet encodings](https://parquet.apache.org/docs/file-format/data-pages/encodings/) 与 [compression](https://parquet.apache.org/docs/file-format/data-pages/compression/)：候选编码/压缩的格式依据；具体可用性由仓库锁定的 Rust 库验证。

## Post-10D execution contract and dependencies

以上 10D.4–10D.5 增补原始数据检查和无损存储前置要求；以下将五项分析优先级展开为
Phase 10E–10I。每一阶段包含输入/输出合同、
实现步骤、正确性验证和交付门槛。文中的新命令、schema 和目录布局均为**拟实现接口**，
当前 CLI 不一定支持；示例阈值仅用于自动化测试，不是产品放行标准。

执行依赖：

```text
10D.2 现有 Dashboard 落盘聚合与口径一致性
   + 10D.3 资源、取消、故障恢复
   + 10D.4 CP/FT 全文件 sanity、字段来源与 run/unit 摘要
   + 10D.5 类型保真、存储基线与有界编码配置
   -> 10E 持久化证据数据集与 traceability 重建
   -> 10F 按步骤的首测/最终结果与重测良率
   -> 10G 硬件变更配对分析与 Change Review
   -> 10H.1 Cpk/Ppk/SPC + 10H.2 GR&R
   -> 10I 可靠性数据与研发阶段放行
```

10E 的模型设计和共享代码提取可在 10D.2 期间推进；面向大规模数据的交付必须通过
10D.3 的运行验证。10H 的分析器可以在 10E/10F 完成后独立开发，之后接入 10G 的
证据包。10I 的应力数据模型可提前定义，正式阶段判定依赖所需分析模块通过验收。
10D.4 的原始字段证据和 10D.5 的类型合同必须在 10E ingest 发布前验收；10D.5 中的
维表优化与 10E.1–10E.2 一起落地，性能优化未达标时保留已经验证无损的基线 profile。
10D 中的“operational qualification”指软件运行验证，不代表芯片 qualification。

共同约束：

1. 原始事实、可修订的映射/流程配置、派生指标、人工审批分别保存。修改映射不能覆盖
   原始 PRR、测量值或已发布的分析结果；所有关联使用显式 ID 和版本。
2. 一份报告固定 `dataset_snapshot_id + analysis_config_hash + engine_version`。
   运行期间不能混入新的 catalog revision；所有表、图、导出共享同一快照。
3. 器件身份复用 `coordinate-v1`；`eav-v2` 和现有 Dashboard 继续保留其既有接口。
   新 evidence schema 使用独立命名空间，不把旧 Parquet 冒充完整测试历史。
4. 报告必须区分业务判定 `pass/fail/insufficient_evidence/not_applicable` 与运行错误。
   缺失、未知、无效值和样本不足不能转换成通过、零失效或零方差。
5. 默认原子发布，重复执行幂等；损坏、资源超限、取消不得产生可被下游当作完整输入的
   半成品。所有排除项保留原因、数量和可追溯明细，不静默过滤。
6. 统计验收阈值按产品、芯片 revision、研发阶段、流程版本和用途配置，记录来源与版本。
   不把示例中的最小样本量、Cpk、GR&R 或相关系数门槛写成通用放行标准。
7. 每阶段都交付配置示例、合成 fixtures、独立预期结果、CLI 文档、HTML/JSON schema，
   并先编写能复现错误行为的回归测试，再实现功能。

建议组件划分（新增 crate 名称为设计建议）：

| 组件 | 职责及实现位置 | 边界 |
| --- | --- | --- |
| `stdf-core` / `stdf-io` | 解码、流式读取、记录 offset、完整性错误 | 不包含良率和放行策略 |
| `stdf-validate` | 字段来源、标准规则、CP/FT profile、sanity 诊断 | 标准合法、产品格式与器件身份可关联性分别报告 |
| 新 `stdf-model` | 共享 ID、事实类型、坐标规则、配置与枚举 | 不依赖 HTML 或 Arrow；旧 API 可通过 re-export 兼容 |
| `stdf-arrow` | 新 evidence 表的显式 Arrow schema、批次构造 | 保留现有 `eav-v2` schema |
| `stdf-parquet::evidence` | 分片写入、事务 catalog、快照读取与校验 | 与旧 `_catalog.json` 分离；复用安全路径和原子写入能力 |
| `stdf-analytics` | 外部排序、关联、确定性 reducer、统计计算 | 不读取 UI 筛选状态，不隐式改变样本范围 |
| `stdf-cli` | ingest、分析命令和离线报告呈现 | 参数校验后调用公共 API；不复制身份/统计逻辑 |
| 版本化外部配置 | 产品、阶段、硬件版本、变更单、实验及应力信息 | STDF 没有的信息由显式配置补充，不从文件名推断 |

共享模型提取时必须检查 Cargo 依赖无环：坐标候选累积器移到共享层，`stdf-arrow`
保留现有导出入口；traceability 的 `Attempt/Run`、流程匹配和 reducer 从 CLI 中拆出。
后续报告都消费共享模型，不能分别维护三套器件合并规则。

## Phase 10E: versioned test evidence datasets (planned)

**目标：** 一次导入 STDF 后，持久化足够的 run、attempt、test definition 和 measurement
事实；移走原始 STDF 后仍能从指定快照重建追溯报告，并为良率及变更分析提供相同证据。

### 10E.1 数据模型与版本合同

新 schema 命名为 `evidence-v1`，独立于 `eav-v2`。所有非必需值使用 null 加原因码；
空字符串、缺失值、零值、NaN、Infinity 和 STDF 无效标记不得被合并成一个“空”。
整数、时间、浮点精度和原始字节表示须在 Arrow schema 中明确。
类型与 raw/effective 字段合同遵循 10D.5；导入固定 10D.4 sanity profile 及规则版本，
保留字段证据和诊断 bundle 的 hash/引用，不能仅凭首值预览通过放行整个 source。

| 表/对象 | 主键与关联 | 必须保存的内容 |
| --- | --- | --- |
| `sources` | `source_id = SHA256(解压后完整字节)` | 全部来源路径、字节数、输入压缩信息、内容 hash、可选归档位置；路径变化不改变 source_id |
| `decode_generations` | source_id + decoder/schema/config digest | 解码器版本、兼容性版本、记录计数、诊断、内容验证状态；一个快照每个 source 只选择一个 generation |
| `runs` | `run_id = H("run-v1", source_id, MIR_offset)` | 原始 MIR 字段、MIR/MRR offset、运行起止时间及有效性；一个 source 可含多个 run |
| `attempts` | `attempt_id = H("attempt-v1", source_id, PRR_offset)` | run_id、source-local part_sequence、head/site、PIR/PRR offset、PART_ID、原始 wafer/PRR XY、原始和解析后坐标键、PRR flags、有效/原始判定与 bin、设备快照 |
| `test_definitions` | 内容寻址的 definition ID | 程序名/版本、test type/num/name、单位、限值、scale/有效性、定义来源及有效范围；缺字段保留未知，不以空值假定等价 |
| `measurements` | `measurement_id = H("measurement-v1", source_id, record_offset, element_ordinal)` | attempt_id、definition_id、原始 test flags、原始 Float32 bits、原始/有效测量值、单位和转换规则、记录位置、重复出现序号 |
| `record_evidence` | source_id + record_offset | 尚未展开的 MPR/FTR 等记录类型、所属 attempt、原始记录字节或受校验的持久化 blob 引用、解析覆盖情况 |
| `analysis_bindings` | snapshot_id + configuration hashes | 流程步骤、身份映射、产品/阶段/硬件 enrichment 的版本化关联；不回写原始事实 |

`H` 使用带域分隔和字段长度的规范化编码再做 SHA-256，禁止直接拼接可能歧义的字符串。
编码必须有跨平台 golden vectors，规定字节序、null/空字符串和浮点 raw bits 表达。
器件键不能充当 attempt_id；measurement_id 不能只由 test_num 构成。ID 不包含文件路径、
读取顺序或导入时间。`part_sequence` 保留现有 traceability 的规则，便于 parity 验证；
新增稳定主键以原始记录 offset 为准。

定义相等与可比较分开处理：完整 definition ID 相同才默认属于同一定义；不同程序版本
之间的语义等价必须有版本化 `test_equivalence` 映射。单位转换需明确变换、适用范围和
测试证明。未知单位、不同限值或同编号异义默认不能直接混算。

研发和设备元数据采用显式 enrichment：`product_id`、`silicon_revision`、`stage`
（engineering_samples / qualification / production）、`test_domain`（CP / FT）、
`hardware_type/id/revision`、`change_id`。提供实际程序文件时才可保存其 program hash；
MIR 的程序名/版本不能冒充程序内容 hash。每个字段记录来源和匹配范围，重复冲突报错。
stage 不由 CP/FT、日期或温度自动推断。产品/工厂标识域用于限制分析样本范围；不能
擅自改变既定 wafer/lot 坐标合并规则，无法确定标识域时禁止跨产品自动配对。

### 10E.2 流式导入、完整性与事务发布

拟新增接口：

```text
zstdf-cli ingest-evidence <inputs...> --output-dir <dataset>
  [--enrichment <json>] [--archive-source]
  [--memory-limit-mib N] [--disk-limit-mib N] [--temp-dir DIR]
  [--max-sources N] [--max-output-files N] [--cancel-file FILE]
zstdf-cli verify-evidence <dataset> [--snapshot <id>]
```

实现顺序：

1. 复用现有流式解码和 head/site 跟踪，统一输出 RunStarted、MeasurementObserved、
   AttemptCompleted、RunFinished 事件。每条 PRR 写一条 attempt，包括无 PTR 的器件。
2. source 采用解压后 hash 去重。导入前确定 hash，解析时再次核对；同内容不同路径仅
   增加来源 alias。目录排序、线程数、分片大小变化不改变逻辑事实和主键。
3. PTR 到达时 PRR_offset 尚未知，使用 source_id + PIR_offset 的内部 provisional ID
   将测量流式落盘。PRR 完成后写 provisional→attempt 对照，用外部排序做关联；不能
   为等待 PRR 把该器件全部测量长期保留在内存。中间关联表不进入完整快照。
4. 首版数值分析覆盖 PTR；MPR/FTR 保留 PRR 结果和 record_evidence，标明未展开。
   下游请求未支持的测量项必须返回覆盖不足，不能默默忽略后声称全项通过。
5. 完整校验记录格式、PIR/PRR 配对、run 闭合、外键、definition 冲突和每类记录计数。
   支持的记录损坏、孤立测量、重复开放 PIR、未闭合实例或缺 MRR 默认失败。
6. 使用 `stdf-parquet::evidence` 写不可变 generation 分片和 manifest；所有表、引用
   blobs 校验完成后，最后原子更新 `_evidence_catalog.json` 的 current snapshot。
7. 单次调用默认 all-or-nothing：任一输入失败，current snapshot 不变；已完成但尚未
   被引用的 generation 可由显式恢复工具回收。首版不提供隐含 partial-success 模式。
8. 锁住 catalog 写者，读取者固定快照；报告运行期间的新导入不影响已有读取者。
   decoder/schema 不兼容时新建 generation，不原地改写旧文件。

建议布局：

```text
evidence/
  _evidence_catalog.json
  snapshots/<snapshot-id>.json
  objects/<source-id>/<decode-generation>/
    manifest.json
    runs/*.parquet
    attempts/*.parquet
    definitions/*.parquet
    measurements/*.parquet
    record-evidence/*
  sources/<source-id>.stdf                 # 仅 --archive-source 时归档
  configs/<configuration-hash>.json
```

manifest 保存 schema/decoder/options digest、每个对象的 hash/行数/字节数、记录覆盖、
父快照和来源 alias。snapshot ID 基于规范化清单计算，不包含当前时间；采集时间和
执行日志作为独立 provenance 保存。逻辑事实相同不要求 Parquet 文件字节完全相同，
但同一对象一经发布必须不可变。

资源预算覆盖批次、active contexts、外部排序、临时关联表、归档 staging、打开的文件
和最终发布暂存。内存预算是 accounted memory，不是进程 RSS 硬上限；分别报告两者。
对磁盘的限制必须覆盖同时存在的 merge 输入/输出和 staging，不只计算最终文件大小。
正常错误和协作取消清理本次拥有的文件；进程被杀后的恢复按 10D.3 的所有者/锁规则执行。

### 10E.3 从数据集重建追溯报告

拟扩展接口：

```text
zstdf-cli traceability --dataset <evidence> --snapshot <id>
  --flow-config <json> --output <html>
  [--flow-closures <json>] [--identity-map <json>]
```

`--dataset` 与直接 STDF inputs 互斥；现有直接读取入口保留。两者调用同一共享 reducer。
raw facts 中不固定某一流程匹配结果，新的 flow/identity 配置生成新的 analysis_bindings
和分析 ID。报告保存快照、配置摘要、排除原因和引擎版本；定位原始字节时优先用归档，
原文件不在且未归档时明确显示“原始字节不可访问”，不影响已验证事实的报告重建。

### 10E.4 正确性验证与交付门槛

| 编号 | 场景与验证方式 | 必须满足的结果 |
| --- | --- | --- |
| E01 | 同一 STDF 的复制、改名、gzip 副本多次导入 | sources 内容身份唯一，attempt/measurement 数量不增加；alias 完整 |
| E02 | 多 site 交错、重复 PART_ID、standalone PRR、仅 MPR/FTR | 每条 PRR 恰好一个 attempt；测量不串 site；覆盖信息准确 |
| E03 | 同一器件同一 test_num 重复记录、同编号异单位/异程序 | 每条测量有独立 ID，definition 不错误合并 |
| E04 | PRR 前百万 PTR、极小批次/分片、不同线程数 | 正确关联或明确超限；不依赖完整器件测量驻留内存 |
| E05 | source 内容在 hash 和 parse 之间变化、记录截断、引用损坏 | 导入/verify 失败，旧 current snapshot 及旧报告字节不变 |
| E06 | 所有阶段故障注入、取消、进程终止、并发读写 | 读者只能看到完整旧/新快照；不删除其他任务对象 |
| E07 | 先直接生成 traceability，再从 evidence 重建 | 对 device、attempt、步骤状态、changes、order_unknown、来源逐字段相等；仅明确新增 provenance 字段可不同 |
| E08 | 移走原 STDF；更换 flow 或 identity mapping | 原快照可重建，新配置可重新分析；原始事实及旧报告不变 |
| E09 | 不同端序、NaN/Infinity、null、无效 flags、坐标冲突 | 保留原始证据，派生有效性与现有坐标规则一致；不产生伪造正常值 |
| E10 | 不兼容 schema、decoder generation 更换、旧 eav-v2 输入 | 明确拒绝错误格式；旧快照可继续被相应版本读取，不静默迁移 |

交付门槛：E01–E10 全部通过；至少 100 万条 measurement 的端到端运行通过资源门槛；
direct-STDF 与 dataset 报告在当前合成样本和扩展边界样本上完成 parity；离线 HTML
筛选、下钻、JSON 导出均引用同一快照。**完成 10E 后才把它作为后续指标的标准输入。**

## Phase 10F: step yield and retest analytics (planned)

**目标：** 从独立 attempt 历史产生明确定义的按步骤良率和重测指标，解释损失发生位置，
同时保留未知判定、身份关联限制和流程缺口。依赖 10E，不改变旧 Dashboard 的 all-pass 口径。

### 10F.1 输入与样本范围

拟新增 `yield-report --dataset <dir> --snapshot <id> --config <yield.json> --output <html>`。
配置包含产品、silicon revision、stage、CP/FT、流程 ID/版本、lot/wafer、分析时间范围、
flow closure 和身份映射版本。首版默认以**运行完整落在时间范围内**作为时间筛选规则；
边界重叠或未知时间单列，不从 PRR 相对耗时反推绝对时间。

时间截取可能丢失早期尝试，因此报告明确为“所选观察窗口内首测/最新”，只有配置明确
声明并验证覆盖完整步骤历史时才允许称“完整流程首测/最终结果”。窗口外已知历史数量
显示在 coverage 中，缺完整历史不能凭最早输入文件声称真正 first pass。

分析单位为 `(scope_id, device_identity, flow_id/version, step_id)`，scope 固定产品和
标识域。身份未解析的实例参与原始 attempt 计数，但不伪造唯一器件数；回退未关联的器件
单独分层，报告关联覆盖率。器件总体来自输入中的已识别器件，不能推断完全未出现的芯片。

### 10F.2 指标合同

对某个步骤，定义 T 为观察窗口内有 attempt 且身份可可靠关联的器件数；A 为这些器件
的 attempt 数。所有结果同时给出全体原始实例数和身份排除数，避免 T 被误认为全批数量。
复用现有保守排序规则：同 source 按 sequence，跨 source 必须能建立无矛盾的完整顺序。
不能排序的器件不按路径、hash 或导入顺序选择首测/最终结果。

| 指标 | 明确定义 |
| --- | --- |
| `first_known_devices` K1 | T 中顺序可确定且第一次 attempt 的有效 verdict 已知的器件数 |
| `first_pass_devices` P1 | K1 中第一次为 Pass 的器件数 |
| `first_pass_yield_known` | P1 / K1；K1=0 时 null；标题明确“已知首测样本条件良率” |
| `latest_known_devices` KL | T 中顺序可确定且最后一次 attempt 的有效 verdict 已知的器件数；不跳过最后的未知结果去挑更早一次 |
| `latest_pass_devices` PL | KL 中最后一次为 Pass 的器件数 |
| `latest_yield_known` | PL / KL；KL=0 时 null；未关闭的流程显示 latest-observed，不宣称制造流程已最终结束 |
| `retested_devices` | T 中 attempt 数大于 1 的器件数 |
| `retest_device_rate` | retested_devices / T；T=0 时 null |
| `extra_attempts` | A − T；未知判定和顺序也计入真实重复次数 |
| `recovery_rate_known` | 首末结果均可确定的器件中，首 Fail、末 Pass 数 / 首 Fail 数 |
| `degradation_rate_known` | 首末结果均可确定的器件中，首 Pass、末 Fail 数 / 首 Pass 数 |
| `ever_verdict_changed` | 所有独立 attempts 中同时出现有效 Pass/Fail 的器件数，不只比较首末 |
| `bin_migration` | 同一被选中的首末 attempt 的有效 hard/soft bin 转移计数；无效 bin 进入 unknown 分类 |
| `step_coverage` | 已测、缺失、待测、不适用、无法判定数量，分别沿用 flow/closure/stop 规则 |

每个比例附 `numerator`、`denominator`、`excluded_by_reason`、`unknown_count` 和定义版本。
对于首测/末测，另给出 T 范围内的 pass fraction 下/上界：`P/T` 和 `(P + U)/T`，其中
U 是对应首/末 verdict 未确定的器件数。这是**缺失结果的识别区间，不是统计置信区间**。
主报告同时展示 coverage，不用条件良率掩盖未知比例。PRR verdict 与 bin 有效性分别计算，
不能因 bin 缺失自动把有效 Pass 改成 Fail；失败停流仍复用已有更保守的完整性规则。

首版不把各步骤良率直接相乘作为跨步骤流程良率。若交付 cohort completion 指标，必须
固定起始器件集合，并逐颗验证所有适用必测步骤；缺失/待测/停流数量分别显示。
Pareto 默认归因到选定 attempt 的有效失败测试；没有测量明细的 PRR Fail 列为
`failure_cause_unavailable`，不凭 bin 名称推断参数根因。

### 10F.3 实现方式

1. 从冻结快照构造 scope 和 flow bindings，外部排序按 device/step 聚合 attempt。
2. 共享 reducer 输出首末选择、全历史变化、分母台账和步骤状态；保持选择的 attempt_id。
3. 二次落盘聚合生成产品/阶段/lot/wafer/site/program/hardware 分层指标。跨 site 重测的
   归属必须显式标记为 first-site、latest-site 或 attempt-site，不能混成同一口径。
4. JSON 指标包含分子分母及 evidence 引用；HTML 图表从同一 JSON 派生，点击可查看所有
   构成样本和排除项。高基数明细分页/导出，超限失败，不静默截断。

### 10F.4 正确性验证与交付门槛

建立独立手算 fixture：同一步骤存在 A=Pass、B=Fail→Pass、C=Pass→Fail、D=Fail、
E=Unknown、F=Pass→Fail→Pass、G=跨文件重叠且含 Pass/Fail；另外 H 为 missing、I 为
pending、J 因前置失败 stop 为 not_applicable，三者在该步骤没有 attempt。

预期 T=7、A=12、K1=KL=5、P1=PL=3，首测/末测条件良率均为 3/5；重测器件数=4、
extra_attempts=5、recovery=1/2、degradation=1/3。G 参与变化检测和重测计数，但不参与
首末已知分母；F 首末都是 Pass，仍必须标记历史判定变化。首/末 pass fraction 的识别
区间均为 [3/7, 5/7]。H/I/J 分别落入正确的流程状态，不加进 T。

进一步验证：窗口截断、末次 Unknown、仅 bin 改变、全部未知、零分母、无测量 PRR、
同一器件跨 site、未解析身份、重复文件、改变输入顺序、关闭/停流后重测恢复。
每一分组都检查计数守恒，聚合层与明细筛选得到的分子/分母一致。用独立 Python
参考实现复核合成数据，不从 Rust 输出反推“预期值”。

交付门槛：上述手算 fixture 所有整数/集合精确相等；比例在声明的数值容差内；
新增数据集入口与 direct traceability 的 attempt/flow 结果一致；报告不展示伪造的最终
结果；百万 measurement 下按 10D 预算完成，所有报告筛选与 JSON 分母保持一致。

## Phase 10G: hardware correlation and Change Review (planned)

**目标：** 对 probe card、loadboard、socket、handler、tester、site 等变更，建立
可复现的 baseline/candidate 比较，形成工程审查证据包。依赖 10E/10F；普通测试项间
Pearson 相关性不能代替本阶段的配对一致性与验收判断。

### 10G.1 变更配置与配对合同

拟新增接口：

```text
zstdf-cli change-review --dataset <dir> --snapshot <id>
  --change-config <change.json> --output <review.html>
```

`change.json` 必须提供 change ID/版本、变更目的和硬件维度、baseline/candidate 的
明确选择条件、产品/revision/stage、同一步骤、程序/测试项等价映射、温度/电压等匹配
条件、attempt 选择规则、最小样本与覆盖率、每项验收 margin 及单位、规则版本。
baseline/candidate 选择范围不能重叠；不能把同一 attempt 同时用作两侧。
硬件 revision 缺失时可补充精确 run/site/有效范围的 enrichment，但冲突必须拒绝。

默认配对键为 `(scope, device_identity, step, test_equivalence_id, condition_stratum)`。
先按声明的 attempt policy 选择一侧一次测试，再选择测量记录。默认 `unique`：
一侧存在多个 eligible attempts/同一测试重复记录时列为 ambiguous，不取平均或第一条。
可显式选择有可靠顺序的 first/latest，必须保存选择理由和所有排除候选。
同一 measurement 不可被重复用于多个独立 pair。没有共同器件的集合只能输出独立
cohort 描述性比较，不能贴上 paired correlation 或完整变更验证的标签。

程序、限值、温度和硬件同时变化时标记混杂因素；首版只有显式匹配或配置批准的分层
可以进入硬件效果分析，不能声称观察差异由某个单独硬件引起。recipe/温度匹配采用
配置声明的数值规范化和容差；保留 MIR 原始字符串，不把摄氏/华氏或未知电压直接匹配。

### 10G.2 统计结果与验收规则

每个测试项输出 baseline/candidate 样本数、pair 数、未配对及排除原因、配对覆盖率、
均值/标准差、散点图、差值分布和以下指标：

- 逐对差值统一定义为 `candidate − baseline`；bias、差值标准差、预先指定的分位数。
- 可选 paired-t bias CI，输出置信水平、假设和方法。工程通过条件是整个 CI 落入
  预先定义的等价区间；“差异不显著”或 CI 包含 0 不是证明等价。
- Pearson r、OLS slope/intercept 作为描述性指标；常数列、样本不足返回 null/reason。
  baseline 也有测量误差，因此不得把 OLS 斜率当作已验证的校准因果关系。
- Pass→Fail、Fail→Pass、unknown 与 bin 迁移采用配对的相同 attempt，分母明确。
- 验收可同时要求 bias CI、绝对差值 margin、coverage、结果翻转限制和设计有效性。
  每个测试的阈值必须显式给出或标记“仅描述，不参加放行”。

高相关且有固定偏移必须能判 fail。重复测量不能人为增加独立样本数；若独立采样单位
是 wafer/lot，而不是 die，配置应声明 cluster，采用与实验设计匹配的区间估计或返回
不支持。首版可只支持明确的独立 device pairs，并将其他设计标为 insufficient_evidence。
多测试联合验收必须声明 `per_test` 或 `familywise` 置信策略；后者可用预先固定的
Bonferroni 调整，不能把很多逐项区间冒充相同置信度的整体结论。

统计方法参考 [NIST 配对均值差置信区间](https://itl.nist.gov/div898/handbook/prc/section3/prc312.htm)；
这里的等价 margin、覆盖率和联合验收策略属于本产品的预注册工程规则。

### 10G.3 实现方式与证据包

1. 解析配置并冻结 selectors、test equivalence、unit transformations 和硬件 enrichment。
2. 在 `stdf-analytics` 外部排序连接 baseline/candidate，输出可审计的 pair table，
   字段包含两侧 attempt_id、measurement_id、匹配条件、排除原因和差值。
3. reducer 计算分层统计；不自动合并 CP 与 FT 或不同测量定义。bootstrap 等方法如后续
   加入，必须记录随机种子、重采样单位和软件版本；首版不隐式启用。
4. 规则引擎将每项结果映射到 pass/fail/insufficient_evidence/not_applicable。
   overall：任何必需项 fail 则 fail；否则任一必需项证据不足则 insufficient；
   全部适用必需项通过且确有必需项被评估才 pass，空集合不能“真空通过”。
5. HTML/JSON 包含 baseline/candidate 快照、配置 digest、统计方法、阈值、pair 明细、
   排除台账和问题清单；新快照或规则变化生成新 review ID，旧 review 不回写。
6. 本阶段只生成机器评估和审查材料；人工批准单独记录，不因生成报告就更改生产配置。

### 10G.4 正确性验证与交付门槛

| 编号 | 合成场景 | 验收预期 |
| --- | --- | --- |
| G01 | candidate 完全等于 baseline，有限且有方差 | 差值全零，r=1，slope=1；在足够覆盖及样本策略下通过 |
| G02 | candidate = baseline + 0.2，bias margin 为 ±0.1 | r=1 仍不得通过，固定偏移被检出 |
| G03 | candidate = 1.1 × baseline | scale/slope 与差值随量程变化被检出，不能只看均值掩盖 |
| G04 | 零方差、NaN、缺测、重复候选、未知顺序 | 指标 null/诊断正确；禁止静默取首条和多对多配对 |
| G05 | 相同坐标但不同 wafer/lot 或产品域，未映射身份 | 不构造伪 pair，未配对计数守恒 |
| G06 | 单位可转换与单位未知、同编号不同程序 | 仅显式且验证的转换/等价映射可以配对 |
| G07 | 修改程序、温度与硬件，或仅有不同器件 cohort | 混杂/非配对状态明确，不能输出硬件单因素验证通过 |
| G08 | 大量重复测量、低配对覆盖、多个测试联合验收 | 不虚增独立 n；coverage 与置信策略按配置执行 |
| G09 | 换输入顺序、批次大小、重新导入副本 | pair IDs、统计结果与规则判定保持一致 |
| G10 | 修改快照或规则版本后重新生成 | 新 review ID；旧证据包仍可重现 |

冻结独立参考数据和计算脚本：手算平移/缩放案例，加上 Python/SciPy 或 R 的配对
统计参考，记录参考实现版本。整数/pair 集合精确一致；浮点及 CI 按共同验证规则比较。
浏览器检查按测试/硬件/site 筛选、异常定位、配对下钻和全量 JSON 导出；机器判定与
导出的同一阈值一致。至少完成一个“旧硬件→新硬件”的合成审查全流程后才交付 10G。

## Phase 10H: process capability, SPC and measurement systems (planned)

**目标：** 验证过程能力、时间稳定性与测量系统；两类分析使用独立配置和方法声明，
结果以 10G 的证据对象格式供后续审查引用。依赖 10E；样本选择复用 10F 的显式政策。

### 10H.1 Cpk/Ppk 与 SPC specification

拟新增 `capability-report --dataset <dir> --snapshot <id> --config <json> --output <html>`。
配置必须指定产品/stage/step/test definition、单位与限值版本、lot/wafer/site/hardware
分层、attempt policy、观察窗口、rational subgroup 定义、最小组数/样本数、分布假设，
以及要采用的控制图、规则、控制限来源和冻结基线。

每颗器件的测量选择按预声明政策执行；不能仅选择通过器件、最佳重测或删除越限点来
提高能力值。测量无效值可以排除，但必须计数并保持来源。不同限值版本、程序语义、
温度和设备条件默认分层；未知定义不能直接混算。

能力计算合同：

```text
Cp  = (USL - LSL) / (6 * sigma_within)
Cpk = min(USL - mean, mean - LSL) / (3 * sigma_within)
Pp  = (USL - LSL) / (6 * s_overall)
Ppk = min(USL - mean, mean - LSL) / (3 * s_overall)
```

`s_overall` 为所选样本的 n−1 样本标准差。首版 `sigma_within` 可限定为显式 rational
subgroups 的 pooled within-subgroup 标准差，保存 estimator ID；若采用 c4 等校正，
必须另设方法版本并提供参考验证。没有有效 subgroup 时，Cp/Cpk 返回 not_estimable，
不能用 overall 标准差计算后仍标成 Cpk。单侧规格输出 Cpu/Cpl 或 Ppu/Ppl，Cp/Pp 不适用。
零方差、n 不足、LSL≥USL、未知单位须有明确状态，不能把 Infinity 当成过程已经合格。
均值超出规格时允许产生负能力值，不裁剪为 0。能力解释以前提稳定性为基础，参见
[NIST process capability](https://www.itl.nist.gov/div898/handbook/pmc/section1/pmc16.htm)。

SPC 首版建议只实现有冻结 baseline 的 I-MR 控制图与明确版本的规则集；没有可靠时间
顺序则不能运行时间序列规则。Phase-I baseline 和 Phase-II monitoring 分开，不能让
新异常点自动重算控制限。规格限与控制限分别展示；不稳定过程保留描述性统计，但
能力放行结果为 insufficient_evidence。非正态转换或分布拟合不是默认隐式步骤，首版
不支持的方法应返回 unsupported，而不是默认套正态失效率外推。

实现：共享 selection/definition 解析，外部排序形成 subgroup/时间序列；用稳定在线
矩计算均值和方差，保存样本及组台账。SPC 引擎消费排序后的序列和冻结 limits，输出
触发规则、位置、对应 measurement_id。HTML 将数值、方法、coverage 和有效性一起呈现。

验证：

- 注入已知 mean=0、sigma_within=1、LSL=−3、USL=3 的独立模型，Cp=Cpk=1；
  均值移动到 1 时 Cpk=2/3。另用真实 subgroup fixture 独立计算 sigma，不能只测试公式。
- 相同组内散布、组间均值漂移的 fixture：within 与 overall 不相等，Cpk/Ppk 差异被保留。
- 单侧限、零方差、负 Cpk、n=0/1、未知单位、限值版本混合、重测选择偏差都需回归。
- 冻结控制限后注入 step shift、drift 和越限点，按预选规则准确触发；训练基线不变。
- 与独立 NIST 示例/参考脚本比较 estimator、限值、规则位置和有效性，不只比较最终图。

交付门槛：能力值及其方法可重现；不稳定/不适用时不能通过能力 gate；SPC 告警能定位
原始器件测量；批次/分片变化不改变选择与判断。产品阈值需显式配置。

### 10H.2 GR&R specification

拟新增 `grr-report --dataset <dir> --snapshot <id> --study-config <json> --output <html>`。
`study-config` 定义 study ID、被测特性/单位、parts 清单、appraiser 因子、重复次数、
实验设计与随机化顺序、条件/设备/时间 block、measurement→实验单元映射、验收方法。
自动测试时 appraiser 可以是明确声明的 setup/设备因素，不把未知操作员都归成一个人。
普通生产重测因选择机制不同，不能自动视为 GR&R 实验样本。

首版限定完整、平衡的 crossed study，每个 part × appraiser 单元具有相同重复次数。
例如 10 parts × 3 appraisers × 3 repeats = 90 条独立登记测量仅作为测试 fixture。
因子和 repeats 至少 2 个水平是可估计性检查，不代表足够的产品样本量。missing cells、
重复复用同一 measurement、嵌套设计、不平衡设计、漂移混杂必须明确报错或证据不足。
后续支持 unbalanced/REML 需要独立方法版本和验证，不能偷偷切换算法。

模型显式包含 part、appraiser、part×appraiser interaction 和 repeatability error。
输出 ANOVA/方差分量、repeatability、reproducibility（含交互项的声明）、part-to-part、
total variation、%study variation、%contribution，以及双侧规格下可计算的 %tolerance。
必须区分标准差比例与方差比例，保存乘数（如 6σ）和分母。零分母返回不可估计；
负方差分量如采用截零，保存原始估计和 boundary flag，不能把边界估计包装成确定的零误差。
默认保留交互项，不根据一次 p 值自动删项；池化规则必须独立声明和验证。
实验设计要求参考 [NIST Gauge R&R](https://www.itl.nist.gov/div898/handbook/mpc/section4/mpc4.htm)。

实现：通过 evidence 引用组装 study matrix，先验证设计完整性，再在独立 analytics
模块实现 balanced two-way random-effects ANOVA。图表提供各 part/appraiser/repeat
的测量值、交互作用和残差；模型假设失败时显示诊断。结果作为 10G/10I 的独立证据，
不自动套用某个行业的 10%/30% 通用规则。

验证：纯 repeatability、固定 appraiser shift、已知交互、巨大 part 间差异、相同
读数导致的边界、缺单元/重复 measurement 等 fixtures；使用固定随机种子生成附加数据，
用独立参考 ANOVA 核对 mean squares、方差分量和百分比。重命名因子、改变输入顺序
不改变结果；相同误差但扩大 part 间差异时，%study variation 与 %tolerance 的变化
应符合各自分母，防止把两种百分比混淆。

### 10H.3 集成与交付门槛

10H.1 和 10H.2 分别验收后，10G 可引用 capability、SPC、GR&R 的 artifact ID 和摘要。
引用 artifact 必须匹配产品、条件、硬件和方法版本；“有一份 GR&R 报告”不等于当前
变更条件已经被覆盖。浏览器应可查看引用的覆盖范围、失效/不适用原因与原始实验单元。
所有数值与独立参考一致、设计不支持时明确失败、阈值配置可追踪，才可交付此阶段。

## Phase 10I: reliability evidence and lifecycle stage gates (planned)

**目标：** 将 CP/FT 电测结果与应力、寿命、读点及样本计划关联，支持 engineering samples、
qualification、production 的阶段性审查。单独 STDF Pass 或零失效不能证明可靠性通过。

### 10I.1 可靠性数据合同

拟新增 `ingest-reliability --dataset <dir> --plan <json> --events <jsonl>`，首版采用
显式 JSON/JSONL schema；其他 CSV/实验系统适配器必须映射到相同模型并单独验证。

| 对象 | 必填内容与关联 |
| --- | --- |
| `reliability_plan` | plan ID/版本、产品/revision/stage、适用标准及版本或内部方法、sample cohort、应力/读点计划、失效判据、统计假设、置信水平与验收目标 |
| `sample_enrollment` | 完整器件身份或明确的封装序列号映射、lot/wafer、抽样方法、入组时间；未映射样本不得按坐标猜测 |
| `stress_event` | sample/cohort、应力类型、温度/电压/湿度或 cycles、单位、开始/结束、累计暴露、设备/腔体、异常中断及数据来源 |
| `readpoint` | 计划/实际暴露点、baseline/post-stress 标签、关联 electrical attempt_id、覆盖的测试项和定义、是否完整 |
| `failure_observation` | sample、失效判据/模式、exact time 或失效区间、暴露单位、证据引用、有效性、处置 |
| `censor_observation` | sample、删失时点、right/interval 等类型、退出原因与是否可能信息性删失；不能把退出样本直接当成功 |
| `stage_assessment` | assessment ID、policy 版本、冻结的全部 evidence artifacts、每项机器判定及理由 |
| `review_decision` | assessment ID、声明的审查人/角色、decision、时间、理由及 waiver 引用；与机器结果分开 |

事件保留 source event ID/hash，重复导入幂等。更正通过 superseding event 和新快照，
不改写已审查的事件。时间基准、时区、温度/电压/小时/cycles 单位明确；累积 exposure
不等同于壁钟时间。重复/重叠区间不能重复累计；未知间隔显式标记。
电测复测恢复并不自动撤销已观察的可靠性失效，需按计划的失效定义与工程处置记录判断。

### 10I.2 分析范围与实现

拟新增 `reliability-report --dataset <dir> --snapshot <id> --plan <json> --output <html>`。
第一版分两个可独立验收的分析模式：

1. **固定暴露终点的属性验证**：每颗样本最多一次通过/失败判定；输出入组数、达到
   终点数、失效数、未到读点/缺测/提前退出数，以及明确方法的二项置信界。只有同一
   终点、符合独立同分布抽样假设且结局可确定的样本才适用。失访不能当作成功；覆盖
   不满足计划时总体 gate 为 insufficient_evidence，即使可评估子集零失效。
2. **寿命观察**：对 exact failures + 非信息性 right censoring 实现 Kaplan–Meier，
   记录 ties 处理和区间估计方法。同一时点先处理风险集中的 failures，再处理 censoring。
   interval-censored 失效只保留区间并显示“不支持该估计”，不取中点伪造精确时间。

不把不同应力条件、失效模式或不同暴露单位直接合并。首版不将高温测试小时自动换算为
使用寿命，不输出缺模型依据的 FIT/MTBF、Weibull 或加速寿命结论。后续增加分布拟合/
加速模型时，必须新增模型版本、参数来源、适用失效机理、拟合诊断、敏感性分析和独立验证。
删失与未失效样本的处理参考 [NIST censoring](https://www.itl.nist.gov/div898/handbook/apr/section1/apr131.htm)。

实现：复用 10E 快照事务保存事件；按 sample 外部排序计算暴露与读点状态；电测前后
比较使用 10G 的定义映射和测量配对。统计模块只消费通过计划/完整性校验的数据，输出
描述、模型假设、不可评估原因和可追溯的 sample 台账。前后参数漂移与失效判定分别报告。

### 10I.3 研发阶段 gate 与人工审查

拟新增 `stage-review --dataset <dir> --snapshot <id> --policy <json> --output <html>`。
政策按产品/revision/stage 定义要求，而不是在代码里硬编码所有产品都必须经过相同试验。

| 阶段 | 可配置的证据类别 | 必须显示的限制 |
| --- | --- | --- |
| Engineering samples | 身份/流程覆盖、初始良率、参数分布、基础硬件比较 | 小样本和选择偏差；未经验证不能外推量产能力 |
| Qualification | 冻结程序/硬件、required reliability studies、GR&R、能力及未关闭变更 | 样本/读点不足、应力中断、失效未处置、适用标准/版本缺失 |
| Production | 已批准基线、SPC、良率/重测趋势、设备变更审查、按计划的持续可靠性监测 | 基线过期、硬件版本不匹配、数据窗口不完整、未批准变更 |

政策包含必需 artifact 类型、覆盖范围、时效、revision 匹配、阈值和 waiver 规则。
每项输出 pass/fail/insufficient_evidence/not_applicable，整体聚合沿用 10G 的非空
必需项规则；mandatory 项不能由普通配置随意置为 not_applicable，必须满足政策中的
适用条件或记录单独授权的 waiver。一个旧 revision 的通过报告不能自动放行新 revision。

机器评估与人类审批保持独立。生成 HTML 不产生正式生产 release；人工意见引用
assessment hash，保存角色、时间和原因，后续更改证据导致新 assessment。离线 CLI
记录的 reviewer 字段只是声明；正式权限控制、签名及身份认证接入需单独验证，不能把
一个可编辑 JSON 名字当成已认证审批。第一版可先交付机器评估与离线审查包。

### 10I.4 正确性验证与交付门槛

| 编号 | 场景与验证方式 | 必须满足的结果 |
| --- | --- | --- |
| I01 | 固定终点、零失效、所有样本完整到期 | 报告样本数、目标暴露和置信界，不报告“可靠性=100% 已证明” |
| I02 | 相同独立样本数、不同提前退出/缺读点比例 | 退出不变成 Pass，计划覆盖不足阻止总体 gate 通过 |
| I03 | 精确失效和 right-censored 的手算小样本 | 风险集及 KM 乘积逐时点相等；全删失曲线不伪造失效时间或寿命保证 |
| I04 | interval-censored、信息性退出、不同单位/应力混合 | 保留原始信息并返回方法/条件不适用，不暗中转为普通 KM |
| I05 | 读点与 CP/FT identity 未映射、重复事件、顺序打乱 | 不错误关联、不重复计暴露/样本；确定性结果不受输入顺序影响 |
| I06 | post-stress Fail 后电测 retest Pass | 按计划保留失效历史，不自动清除可靠性事件 |
| I07 | 必需报告缺失/过期/错误 revision、空政策、未经授权 N/A | 不能通过阶段 gate；每个阻断原因可追溯 |
| I08 | 所有必需证据有效、人工记录前后更换快照 | 新 assessment 需独立审查，旧人工决定不能隐式迁移 |

固定终点二项模型使用预先选定的精确单侧置信界，与独立参考实现核对；例如零失效且
所有 n 个独立样本完整到期时，下置信界的手算 oracle 为 `R_lower = alpha^(1/n)`。
在 95% 单侧置信水平下，n=59 时下界约 0.9505，n=10 时约 0.7411；这只验证计算，
不意味着 59 或 10 是任何芯片产品的规定样本量。原始假设不成立时此模型不得用于放行。

KM fixture 示例：4 个样本，t=1 有 1 failure，t=2 有 1 censor，t=3 有 1 failure，
t=4 最后 1 个 censor；S(1)=3/4、S(3)=3/8。另测 failure/censor 同时发生的 ties、
零时间、负 exposure 和不可能的先后关系。参考脚本与 Rust 输出的风险集/状态必须一致。

交付门槛：I01–I08 与模型 oracle 通过；每个电测/应力/阶段结论能回溯到输入事件和
快照；所有未知/不支持条件阻止不当自动放行；完成从样本入组到阶段审查包的合成演示。
真实产品 qualification 声明必须另有适用计划、工程审核及真实数据验证证据。

## Post-10D validation and release checklist

以下是未来实现的验收规范，不是本次文档更新已经执行的测试结果。

### 独立预期与数值容差

- ID、整数计数、所选样本集合、pair 集合、状态、排除原因必须精确一致。
- 原始 Float32/Float64 bits、字段 raw bytes、record offsets、文件 hashes 必须逐位一致；派生 Float64 统计
  默认采用 `abs(actual-ref) <= 1e-10 + 1e-8 * abs(ref)`。逆分布/CI 等算法如需不同
  容差，必须在测试清单逐项说明数值依据，不能为通过测试事后放宽。
- 每个统计模块至少有一个手算 oracle、一个独立参考实现和一组边界/拒绝案例。记录
  参考软件版本和随机种子；不能用同一 Rust 算法生成 expected 再与自己比较。
- 决策阈值附近的测试覆盖略低、相等、略高；判定比较使用未四舍五入的数值，并明确
  inclusive/exclusive。统计显著性、工程等价与验收通过分别测试。

### 故障、规模和 UI

| 检查 | 验证要求 |
| --- | --- |
| 资源 | 固定 memory/disk 配置测试 10万/100万 measurement；包括单器件大量记录、高基数定义、多 source 和多 site。分别记录 accounted peak、RSS、live scratch peak、文件数、耗时 |
| RSS 门槛 | 首个端到端证据基线使用 128 MiB accounted budget，默认允许 peak RSS ≤256 MiB、10万→100万增长 ≤32 MiB；这是待实测的发布 gate，不是已达成承诺。按操作系统单独记录，变更门槛需说明原因 |
| 超限 | 输入规模超过配置时明确失败；无静默丢测量、抽样或截断；未知项/排除项均可枚举 |
| 故障注入 | hash/parse、分片写入、外部排序、跨表关联、manifest、catalog 发布和 HTML 替换边界逐一注入错误 |
| 取消/进程终止 | 协作取消检查最终提交边界；kill 后读者只看完整快照，恢复不能删除他人/活跃任务文件。kill 恰在原子提交处允许完整旧或完整新版本 |
| 浏览器 | 1440×900 和 390×844；筛选、分页、下钻、高亮、unknown 状态、完整 JSON 导出、零外部请求、恶意字符串安全、无 JS 错误 |
| 重现性 | 同快照/配置/引擎重复运行结果一致；输入顺序、线程数和批次大小变化不改变逻辑结果；snapshot/config 改变时产出新分析标识 |
| 真数据 | 使用明确授权的产品数据和独立工程基准；记录产品/版本/覆盖范围。合成样本通过不等于真实产品验证通过 |

每个阶段交付时执行现有适用检查，并新增对应阶段的集成/参考测试：

```text
cargo fmt --all --check
cargo test --workspace --exclude stdf-py --locked --offline
cargo check -p stdf-py --locked --offline
cargo build -p stdf-cli --release --locked --offline
```

依赖尚未缓存时首次构建需要联网，不能把下载失败当成功验证；Linux/macOS 需独立编译/
运行记录，不能沿用 Windows 结论。报告交付列出已执行命令、fixture/快照、实际结果、
资源数据和未覆盖条件。阶段状态只在各自退出门槛通过后从 planned 改成 implemented。

建议分批交付顺序：**10D.4 CP/FT sanity 与字段证据 → 10D.5 类型保真与编码基线 →
10E.1 共享模型 → 10E.2 导入/快照（含维表存储优化）→ 10E.3 追溯重建与 parity →
10F 分母明确的良率 → 10G 一个完整硬件变更案例 → 10H 分别验证能力与 GR&R →
10I 可靠性读点和阶段审查**。新增前置要求的第一个可运行里程碑是“CP/FT 全文件检查+
run 重要字段及 unit 各类型前两条记录的首值预览”；证据数据集的第一个可运行里程碑仍是“原始 STDF 移走后
可从固定 evidence 快照重建相同追溯报告”，不能仅以空表或命令占位符交付。
