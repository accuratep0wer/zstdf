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

The initial implementation provides `sanity --test-domain cp|ft`, basic v4 field-layout checks,
raw evidence preservation, important run fields, first-value previews for the first two records
of each type per unit, profile field rules, and atomic HTML publication.
See [docs/sanity.md](docs/sanity.md) for runnable interfaces and limitations. The complete target
contract remains below. Unique mixed CP/FT run-profile matching and numeric/string inheritance
from initial PTR/MPR definitions are implemented. Complete standard rules/count reconciliation,
FTR definition inheritance, separate inventory/findings Parquet tables, and large-scale resource
acceptance remain incomplete; the whole of 10D.4 must not be marked accepted.

**Goal:** Before data enters analytics, check the entire STDF file's records, fields, context,
and product formats. Show important fields for each test run and the first value of each of the
first two records per type within each unit to help engineers inspect tester output. Report
previews separately from full-file validation; displaying two complete test instances is no longer required.

#### 10D.4.1 Field evidence and decision contract

Checks must use STDF v4 raw records and bytes, not only existing EAV/PTR rows.
Maintain field descriptors from the standard for each record: field order, STDF type, length/count
source, permitted omission conditions, missing sentinels, validity flags, valid enumerations,
default/inheritance rules, and standard clauses. Pin the rule version and hash at release.
Declare v4-2007 extension coverage separately; its fields and omission rules must not replace base v4 rules.

Generate `FieldEvidence` for every field, keyed by
`(source_id, record_offset, field_path, element_ordinal)`; retain array element ordinals.
Save record type, field type, raw byte range, raw value, effective value, context references,
and rule results. Missing fields still produce evidence rows; absence of a value must not suppress output.

| Dimension | Values and decision requirements |
| --- | --- |
| `presence` | `present / omitted / truncated`; distinguish legal omission of an entire trailing field from a field that starts but lacks sufficient bytes |
| `value_origin` | `explicit / standard_default / inherited / unresolved`; explicit means a value exists in the bytes, not that it was entered manually |
| `matches_default` | `true / false / unknown`; an explicit value equal to the standard default remains explicit |
| `semantic_status` | `valid / missing / invalid / unknown`; use that field's sentinels, enumerations, and flags, rather than treating all zeros, blanks, or -1 values as invalid |
| `profile_status` | `pass / fail / not_applicable / not_evaluated / unknown`; evaluate CP/FT product formats, association conditions, and standard formats separately |
| `effective_value` | Generate only when a rule determines it; attach the default rule ID or inherited field's source/offset, without overwriting the raw value |

STDF alone cannot establish whether an operator entered a value or the tester filled it automatically.
Such provenance requires explicit links to tester configuration/operation logs. Reports must not label
non-default values as confirmed manual input. A standard missing marker is not a normal default
numeric value suitable for statistics.

#### 10D.4.2 Coverage and CP/FT profiles

Scan every base v4 FAR, ATR, MIR, MRR, PCR, HBR, SBR, PMR, PGR, PLR, RDR, SDR,
WIR, WRR, WCR, PIR, PRR, TSR, PTR, MPR, FTR, BPS, EPS, GDR, and DTR. Extract all defined
fields, including headers, flags, arrays, text, limits, scales, units, and information unused by the current Dashboard.
Maintain a standard-record/field-to-decoding-and-validation coverage inventory. Uncovered items must
show unsupported, never pass. Preserve unknown vendor records' type/subtype, length, offset, and raw
payload. Report known-record decoding failures separately from unknown extensions. Unknown records
prevent completeness claims for affected analyses; the profile determines whether other analyses may proceed.

| Check layer | CP | FT |
| --- | --- | --- |
| Bytes and records | Check FAR/version/byte order, REC_LEN, field boundaries, array counts, validity flags, record order, and closure relationships | Same as CP; FT does not relax decoding errors |
| Run information | MIR lot, product, program name/version, step, temperature; SDR tester/head/site and applicable hardware identifiers | Same common information; profile specifies handler, load board, socket, and other requirements |
| Wafer context | Check WIR/WRR/WCR association with head/site groups, wafer ID, coordinates, and applicable counts; production CP profiles may require wafer information | Absent wafer records may be not applicable; missing WIR/WRR does not automatically fail, but present records are still validated |
| Device association | Maintain PIR–test-record–PRR state per head/site; check whether coordinate identity resolves | Retain every PRR similarly; configure PART_ID/serial-number formats per product, and require explicit reliable identity for CP/FT association |
| Product format | Lot/wafer, program/version, test names, units, limits, and other fields satisfy configured rules | Lot/package/serial number, program/version, test names, units, limits, and other fields satisfy configured rules |

Specify CP/FT through explicit `test_domain`. Mixed inputs require exact source/run configuration,
with exactly one match per run. WIR presence, filenames, or MIR text may provide hints but cannot
establish profile compliance automatically. Standard-valid negative I*2 coordinates and the positive-integer
requirement of `coordinate-v1` belong to different layers. Display the missing meaning of -32768,
valid negative coordinates, and unmet product requirements separately. FT attempts with unresolved
coordinates remain independent instances with association limitations.

Format rules include explicit full-string matching, allowed value sets, numeric ranges, required fields,
cross-record equality, test definitions, and permitted definition changes. Select rules by program/version/step.
Bound regex length and execution resources. Equal preview values indicate observed consistency only;
they do not create product-wide rules. Unconfigured product formats are `not_evaluated`; samples cannot
establish format compliance. Save each rule's ID, scope, and severity.
PCR/WRR/PRR/TSR count reconciliation must first account for head/site, retests, and standard count
semantics. Do not simply equate PRR.NUM_TEST with PTR count or count each MPR result as an independent test.

#### 10D.4.3 Important run fields and unit first-value previews

Use two display levels and deduplicate uploads by source content. Continue checking and persisting
complete records and fields. The default HTML summary below does not expand two complete device histories.

**Per test run:** Show important values from MIR and other records for each run, including raw and
effective values, origin status, standard/product rule results, and record offsets. A versioned CP/FT
profile configures important fields; defaults include:

| Record/context | Important fields shown by default |
| --- | --- |
| FAR | CPU_TYPE, STDF_VER |
| MIR | LOT_ID, PART_TYP, JOB_NAM, JOB_REV, SBLOT_ID, TEST_COD, OPER_NAM, FLOW_ID, TST_TEMP, SETUP_T, START_T, STAT_NUM, MODE_COD, RTST_COD, NODE_NAM, TSTR_TYP, DATE_COD, FACIL_ID, FLOOR_ID, PROC_ID |
| SDR | HEAD_NUM, SITE_GRP, SITE_NUM, SITE_CNT, HAND_TYP, HAND_ID, EXTR_ID |
| WIR/WRR/WCR (when applicable) | Wafer ID, head/site group, start/end times, counts, coordinate system/wafer configuration; identify the source record for each field |
| SBR | HEAD_NUM, SITE_NUM, SBIN_NUM, SBIN_CNT, SBIN_PF, SBIN_NAM |
| MRR/PCR | Run end time/status, scoped part/retest/good/abort counts, and completeness diagnostics |
| PRR (unit summary) | HEAD_NUM, SITE_NUM, PART_ID, PART_FLG, HARD_BIN, SOFT_BIN, coordinates, and closure status |

Display multiple wafers, site groups, or context versions within a run in separate rows. Do not retain
only the first or last value and overwrite the rest. Do not repeat expanded run information beneath
every unit; each unit references its own effective context.

**Per unit:** A unit here means an independent test instance identified by source/run/head/site/part_sequence.
Retests of the same die and repeated PART_ID values remain separate. Support unit filtering/pagination
with compact identity, PRR verdict, and bin summaries. After selection, take the first two records by
raw offset separately for each DTR, PTR, MPR, FTR, GDR, and other record type. Show only each record's
first value and validity as defined below, not its complete test history. “First two” does not mean two
units or two records per test number; do not merge repeated records with the same number.

| Type | First value per record | Minimum accompanying context |
| --- | --- | --- |
| DTR | TEXT value; retain the entire text, not its first character | Offset, association basis, character/format status |
| PTR | RESULT scalar | TEST_NUM, TEST_TXT, UNITS, validity, and default/inheritance provenance |
| MPR | RTN_RSLT[0] | TEST_NUM, TEST_TXT, UNITS, result count, validity; label an empty array empty rather than selecting another field |
| FTR | Valid Pass/Fail/Unknown derived from TEST_FLG | TEST_NUM, TEST_TXT, raw flag; do not invent a number when no floating-point result exists |
| GDR | Typed value of the first non-padding GEN_DATA element | Element ordinal, type tag, element count; label empty data empty, and never present padding as a data value |

For example, if a unit's first three MPR results are `[1.25, 1.26]`, `[2.5, 2.6]`, and `[3.0]`,
preview only `1.25` and `2.5`, labeled “Showing 2/3 records, first value only”; sanity checks still cover
the remaining elements. If the first record/result is invalid, show invalid rather than skipping to a valid
value. With zero or one record, show the actual count; do not pad or borrow values from another unit.

Records without head/site, such as DTR/GDR/BPS/EPS, enter unit previews only when ownership can be
established reliably. With interleaved sites, do not infer ownership from proximity. Show unresolved
records in run context with association limitations. Every preview value links to field provenance and
diagnostics; retain all raw records/array details in the field-evidence export.

Also provide a full-file record inventory, field-status distributions, anomaly lists, and CP/FT, run,
record, field, and severity filters. Errors must drill down to source/record/field offsets.
Safely display control characters, invalid encodings, and `</script>` in HTML; export raw bytes separately as hex.

#### 10D.4.4 Implementation sequence, interfaces, and failure semantics

1. Add optional field-access tracing to `stdf-core::FieldReader`, recording consumed bytes, length
   prefixes, and types. Existing `from_utf8_lossy` display is not raw character evidence; retain raw
   bytes and check standard character constraints and decoding status independently. Test legal omission,
   partial I*2, and truncated C*n/arrays separately; insufficient `remaining()` must not always mean None.
2. Provide an event stream in `stdf-io` with offset/raw payload/decode status. Known-type parse failures
   must not become Unknown without diagnostics. Stop when the next record boundary is untrustworthy;
   do not guess byte resynchronization.
3. Extend versioned field rules and the profile evaluator in `stdf-validate`. Replace the single
   `open_pir_offset` state with run/head/site state. Resolve defaults/inheritance according to each
   record field's standard scope. PTR/MPR definitions must not leak across source/run/program,
   and generic forward-fill must not replace standard rules.
4. Stream full `record_inventory`, `record_fields`, and `findings` to disk. Bound status statistics
   and run/unit summaries too, using existing `stdf-analytics` spill facilities for large instances and
   high cardinality. Do not accumulate unlimited findings in a Vec or stop full-file validation once
   two preview records have been collected.
5. Add the proposed command `zstdf-cli sanity <inputs...> --test-domain cp|ft --profile <json>
   --preview-records-per-type 2 --output-dir <dir>`, supporting files, directories, and gzip. For mixed
   domains, use `--run-profiles <json>`, mutually exclusive with `--test-domain`.
   Preserve compatibility with existing validate/convert interfaces.
6. Produce a versioned bundle: `report.html`, `summary.json`, `record_inventory.parquet`,
   `record_fields.parquet`, `findings.parquet`, raw evidence blobs, and rule snapshots. Its manifest
   records hashes, scan scope, completeness, and results. Offline HTML embeds important run fields,
   unit first-value previews, and check summaries; full field tables remain separately readable in
   the bundle. Exceeding the report budget fails explicitly; silent truncation cannot produce a “complete report.”

`validation_failed` is a deliverable diagnostic outcome: publish a complete diagnostic bundle marked
failed and return a nonzero exit code. Corrupt records may produce diagnostics with `scan_complete=false`,
but unscanned content cannot be declared valid. Runtime write failures, resource-limit failures, and
cancellation do not publish; preserve the existing bundle. Publish through generation directories and
an atomic manifest pointer, not by individually overwriting files in use by readers.
10E ingest uses the same validation event stream and pinned profile; failed/incomplete input cannot
become a validated analysis snapshot. Sanity stores each raw record blob once, with field-range
references, rather than duplicating the whole record for every field.

#### 10D.4.5 Correctness validation and deliverables

| ID | Scenario | Required result |
| --- | --- | --- |
| S01 | All base v4 records with non-default fields; little/big-endian input | Every field/array matches independently annotated byte offsets, types, and values; no unexplained coverage gaps |
| S02 | Legal trailing omission, explicit defaults/empty strings, invalid flags, valid zeros | Accurate presence/origin/semantic states; equal effective values retain distinct provenance |
| S03 | Partial numbers, string prefixes exceeding remaining length, array-count mismatch, corrupt headers | Precise decoding-error locations; no false omitted/Unknown/pass status; stop after unsafe boundaries |
| S04 | Valid CP, valid FT without wafers, CP missing required wafer information, mixed files | Correct profile applies; no false FT errors for inapplicable records; ambiguous/unmapped runs do not pass |
| S05 | PIR A, PIR B, interleaved PTR/MPR/FTR, PRR B, PRR A | First values of the first two records per type per unit follow offset order without cross-site leakage; no guessed binding for records without site |
| S06 | Repeated PART_ID, no PTR, zero/one record of a type, orphan PIR | Independent instances; unit verdict summary exists without PTR; no padded previews; unclosed instances reported separately |
| S07 | PTR default definition, omission, legal override, then test-number reuse in a new run | Standard-scoped provenance resolution without cross-run leakage; raw and effective values reconstruct correctly |
| S08 | Invalid encoding/control characters, product regex mismatch, valid negative coordinates, conflicting fallback coordinates | Distinct standard, encoding, product, and identity diagnostics; no report injection or lost raw bytes |
| S09 | First two first-values valid, but third record/later MPR element invalid; unknown/corrupt records later in file | Full-file diagnostics cover undisplayed content; previews cannot establish a pass |
| S10 | High cardinality, huge single-unit test count, full disk, cancellation, publication failure | Resource gates respected, counts reconcile, old bundle intact, no silent truncation |
| S11 | Multiple runs, wafers/SDR contexts; explicit default or missing important MIR values | Correct important fields/states per run; units reference their own context, without global last-record overwrite |
| S12 | Multiple/empty/invalid-first MPR results, DTR text, GDR padding/type tags, unknown FTR verdict | Strict first-value display, retained record/element counts, no skipped invalid values or expanded complete instances |

Deliver versioned CP and FT profiles with synthetic STDF, complete field-type/rule inventories,
run/unit summary reports, independent byte-level expectations, CLI documentation, and browser checks
for filtering, drill-down, and export. Manually check all standard clauses against the original text.
An independent decoder may cross-check results, but another parser's error tolerance is not the standard.

### 10D.5: lossless STDF-to-Arrow/Parquet storage optimization (planned)

**Goal:** Optimize storage using actual field types and usage frequency while preserving values,
precision, missing states, flags, identity, and record provenance. Distinguish integer widths/signs;
“single/double precision” applies to R*4/R*8 floating-point values. Do not narrow an entire column or
convert it to integers merely because preview values are small or appear integral.

`stdf-arrow/src/schema.rs` already stores PTR result/limits as Float32, head/site as UInt8,
coordinates as Int16, bins as UInt16, and test numbers as UInt32; this is not a blanket Float64→Float32
migration. `stdf-parquet/src/dataset/fragments.rs` currently disables dictionaries explicitly.
Optimization must preserve bounded memory. Establish actual codec/encoding baselines from the
single-file and dataset writers' footers.

#### 10D.5.1 Type mapping and lossless contract

The table defines default logical types for new evidence/field evidence; public eav-v2 column types
remain unchanged. Generate/check schemas against field descriptors, not sampled values.

| STDF type/information | Arrow / Parquet target | Validation and fidelity requirements |
| --- | --- | --- |
| U*1 / U*2 / U*4 | UInt8 / UInt16 / UInt32; Parquet INT32 with corresponding unsigned logical annotations | Preserve zero through each upper bound; label sentinel states separately without deleting raw numbers |
| I*1 / I*2 / I*4 | Int8 / Int16 / Int32; INT32 with corresponding signed annotations | Preserve negative values and both bounds; do not drop negatives through unsigned conversion |
| R*4 / R*8 | Float32 / Float64; FLOAT / DOUBLE | Never narrow R*8 to R*4; integral-valued floats remain floats; separate raw bits from effective values |
| C*1 / C*n / fixed-length characters | Utf8/STRING for valid text; Binary evidence for invalid raw encoding | Byte-based lengths; preserve leading zeros, empty strings, spaces, and case; lossy strings cannot replace raw values |
| B*1 / fixed-length binary | UInt8 flags / FixedSizeBinary or Binary | Preserve every bit; derived booleans do not replace raw flags |
| B*n / D*n | Binary plus required length/bit_count | bit_count differs from byte_count; retain padding-bit checks and raw bytes separately |
| N*1, kxTYPE | UInt8 (0..15) or packed Binary with element count; other arrays use typed List/child tables | Preserve nibble order, odd trailing elements, array order, and counts; do not convert everything to JSON text |
| GDR V*n | Tag plus typed value/child table plus ordinal | Store R*8, integers, characters, and bit strings distinctly; do not convert all to Float64 or discard tags |
| STDF time U*4 | Raw UInt32 plus optional derived UTC timestamp | Preserve raw sentinels and conversion status; no INT96 or arbitrary local timezone |
| Generated offset/sequence/ID | UInt64 / fixed 32-byte hash / normalized identity dimension table | These are internal model types, not a claim that base v4 has U*8 fields; never truncate hashes |

Parquet's 8/16-bit integer logical annotations still use physical INT32. Do not claim each UInt8 value
necessarily occupies one disk byte; actual size depends on page encoding, dictionaries, compression,
and metadata. Verify bit-for-bit round trips for NaN payloads, positive/negative zero, Infinity, and
subnormal values. If an Arrow/Parquet path normalizes these representations, recover them through
verified raw-record references or a raw-bit exception table, counting that storage in total cost.
Effective analysis values may be null, but raw numbers and invalidity reasons must remain accessible.
JSON exports require typed, lossless representations for UInt64, large integers, and nonfinite floats;
browser Number conversion must not lose precision.

#### 10D.5.2 Implementation and measurement steps

1. **Establish a baseline.** Pin fixtures, dependency versions, schema, row groups, hardware, and run
   procedure. Report each column's type, null count, cardinality, compressed/uncompressed bytes,
   dictionary/page encoding, and total file size. Cover short files, large PTR volumes, MPR/FTR/GDR,
   R*8, low/high-cardinality strings, and unusual raw bytes.
2. **Reduce repeated metadata.** When implementing 10E tables, place repeated run, program,
   unit/limit definitions, and hardware snapshots in versioned dimension tables referenced by measurement
   IDs. Reconstruct each row's full context through these tables. Snapshot-local UInt32/UInt64 surrogates
   may reduce repeated wide hash foreign keys, but public logical IDs remain unchanged. Require unique,
   bounded, verifiable mappings in both directions. Never merge semantically different tests with equal numbers.
3. **Configure writers per column.** Compare available uncompressed, Snappy, and ZSTD implementations
   and configurations. Evaluate dictionaries for low-cardinality columns and supported appropriate encodings
   for numeric columns. Set explicit dictionary/page/row-group limits, with bounded fallback encodings for
   high cardinality. Account for dictionaries, page buffers, and concurrent writers. Do not apply lossy
   quantization, rounding, decimal truncation, or unagreed unit normalization to raw measurements.
4. **Unify configuration.** Centralize a versioned `StorageProfile` in `stdf-parquet`, sharing constraints
   across single-file, dataset, and new evidence writers. Record profile/hash, codec, encodings, writer version,
   and schema in the manifest. The initial 10D.4 field bundle may use a correct default layout first;
   verify equivalence after optimization.
5. **Read back and check downstream parity.** Fully compare old/new profiles for types, field states,
   raw bits, arrays, identities, all measurements, and PRR counts, then compare Dashboard/traceability
   outputs. Codec changes do not change logical schemas. Column-contract/table-layout changes require
   schema upgrades and new directories, never in-place modification of old data.
6. **Choose release settings.** Publish size, write/scan time, peak RSS, scratch peak, footer, and raw-byte
   verification reports for each data category. Deliver an explicit optional profile first; change defaults
   only after demonstrating benefits without resource regressions. Do not promise an unmeasured fixed compression ratio.

Storage totals include all Parquet files, dimension tables, indexes, catalogs, raw-bit exceptions, and
required evidence blobs. List optional raw STDF archives separately and also report archive-inclusive
totals. Compare equal evidence coverage; dropping MPR/FTR or raw fields cannot count as lossless
compression savings against complete storage. Repeat each benchmark group and report medians and
spread; measure cold and warm reads separately. Reuse 10D.3 resource limits and preregister compression
and read/write acceptance thresholds in benchmark configurations. Configurations below these gates
remain experimental and disabled by default. Equality after decimal rounding does not satisfy correctness.

#### 10D.5.3 Acceptance cases and deliverables

| ID | Scenario | Required result |
| --- | --- | --- |
| P01 | All type boundaries, both byte orders, maximum U*4, large generated UInt64 | STDF→Arrow→Parquet→export preserves types/values, correct footer signedness, no overflow |
| P02 | R*4/R*8 extremes, adjacent floats, NaN payloads, ±0, ±Infinity, subnormals | Bit-for-bit raw recovery, independent validity, no silent R*8 precision loss |
| P03 | Empty/omitted/space strings, leading zeros, non-ASCII/invalid encoding, explicit defaults/inheritance | Raw bytes and 10D.4 states agree; dictionaries do not merge distinct states |
| P04 | Odd nibble counts, non-byte-aligned D*n, empty arrays, mixed GDR types | Preserve element counts, order, tags, padding bits, and raw evidence |
| P05 | High-cardinality strings, dictionary limits, long single-unit runs, multiple writers | Normal fallback or explicit resource failure at thresholds; no unbounded dictionaries or silent record loss |
| P06 | Different row groups, encodings, codecs, threads, and input order | Same logical snapshots, identities, counts, and downstream results; physical hashes may differ |
| P07 | Coexisting old/new layouts, old manifests, unknown profiles/schemas, cancellation, full disk | Compatible settings read correctly; incompatible settings fail explicitly; old data/reports remain intact |
| P08 | Representative benchmarks include all required evidence objects | Publish total storage and read/write/RSS data; choose defaults using preset gates, not only the best-saving column |

Deliver a field mapping inventory, StorageProfile examples, independent bit-level fixtures,
per-column footer comparisons, repeatable benchmarks, full lossless/parity reports, and migration
instructions. Target types in documentation are not measured storage savings.

#### 10D.4–10D.5 Standards and implementation references

- [Teradyne STDF v4 specification](https://storage.googleapis.com/google-code-archive-downloads/v2/code.google.com/stdf-eclipse/Stdf-V4-spec.pdf): normative source for base records, field types, missing values, and default rules.
- [STDF v4-2007 extension specification](https://www.roos.com/roos/documentation.nsf/3d6a93a7e05462cf85256a9c007dcaf3/92102f712ce51df48825783800832332/%24FILE/STDF%20Spec%20V4%202007.pdf): defines explicit extension boundaries, not evidence of base v4 support for extensions.
- [PySTDF V4 record definitions](https://github.com/cmars/pystdf/blob/master/pystdf/V4.py): independent field-descriptor cross-check; pin its version for tests, and do not substitute it for the specification.
- [Parquet physical types](https://parquet.apache.org/docs/file-format/types/) and [logical types](https://parquet.apache.org/docs/file-format/types/logicaltypes/): references for physical widths, integer annotations, and text representation.
- [Parquet encodings](https://parquet.apache.org/docs/file-format/data-pages/encodings/) and [compression](https://parquet.apache.org/docs/file-format/data-pages/compression/): format references for candidate encodings/compression; verify availability against the repository's locked Rust libraries.

## Post-10D execution contract and dependencies

10D.4–10D.5 above add prerequisites for raw-data validation and lossless storage. The following
sections expand the five analytics priorities into Phase 10E–10I. Each phase specifies input/output
contracts, implementation steps, correctness validation, and delivery gates. New commands, schemas,
and directory layouts below are **proposed interfaces**, not necessarily supported by the current
CLI. Example thresholds are for automated tests, not product release criteria.

Execution dependencies:

```text
10D.2 Existing Dashboard disk-backed aggregation and metric parity
   + 10D.3 Resources, cancellation, and failure recovery
   + 10D.4 CP/FT full-file sanity, field provenance, and run/unit summaries
   + 10D.5 Type fidelity, storage baseline, and bounded encoding configuration
   -> 10E Persistent evidence datasets and traceability reconstruction
   -> 10F Step-level first/final results and retest yield
   -> 10G Paired hardware-change analysis and Change Review
   -> 10H.1 Cpk/Ppk/SPC + 10H.2 GR&R
   -> 10I Reliability data and development-stage release gates
```

10E model design and shared-code extraction can proceed during 10D.2; delivery for large datasets
must pass 10D.3 operational validation. Develop 10H analyzers independently after 10E/10F, then
integrate them into 10G evidence packages. Define the 10I stress-data model early, but formal stage
assessment depends on acceptance of the required analysis modules. Accept 10D.4 raw field evidence
and the 10D.5 type contract before releasing 10E ingest. Implement 10D.5 dimension-table optimization
with 10E.1–10E.2; retain the verified lossless baseline profile if performance gates are unmet.
“Operational qualification” in 10D refers to software operation, not chip qualification.

Shared constraints:

1. Store raw facts, revisable mapping/flow configurations, derived metrics, and human approvals
   separately. Mapping changes cannot overwrite raw PRR records, measurements, or published analyses.
   Use explicit IDs and versions for every association.
2. Pin each report to `dataset_snapshot_id + analysis_config_hash + engine_version`. Do not mix new
   catalog revisions into a running report; all tables, charts, and exports share one snapshot.
3. Reuse `coordinate-v1` device identity. Preserve existing `eav-v2` and Dashboard interfaces.
   Give the new evidence schema a separate namespace; old Parquet is not a complete test history.
4. Distinguish business decisions `pass/fail/insufficient_evidence/not_applicable` from runtime errors.
   Missing, unknown, invalid, or insufficient samples cannot become passes, zero failures, or zero variance.
5. Publish atomically by default and make repeated execution idempotent. Corruption, resource-limit
   failures, and cancellation must not create partial outputs accepted as complete downstream input.
   Preserve reasons, counts, and traceable details for every exclusion; never filter silently.
6. Configure statistical acceptance thresholds by product, silicon revision, development stage, flow
   version, and purpose, recording their source and version. Example minimum sample sizes, Cpk,
   GR&R, or correlation thresholds must not become universal release criteria.
7. Deliver configuration examples, synthetic fixtures, independent expectations, CLI documentation,
   and HTML/JSON schemas per phase. Write regression tests reproducing incorrect behavior before implementation.

Suggested component boundaries (new crate names are proposals):

| Component | Responsibility and implementation location | Boundary |
| --- | --- | --- |
| `stdf-core` / `stdf-io` | Decoding, streaming reads, record offsets, integrity errors | No yield or release policies |
| `stdf-validate` | Field provenance, standard rules, CP/FT profiles, sanity diagnostics | Report standard validity, product formats, and device-identity association separately |
| New `stdf-model` | Shared IDs, fact types, coordinate rules, configurations, enumerations | No HTML or Arrow dependency; re-exports can preserve old APIs |
| `stdf-arrow` | Explicit Arrow schemas and batch construction for new evidence tables | Preserve the existing `eav-v2` schema |
| `stdf-parquet::evidence` | Fragment writing, transactional catalog, snapshot reading/verification | Separate from old `_catalog.json`; reuse safe paths and atomic writes |
| `stdf-analytics` | External sorting, joins, deterministic reducers, statistics | No UI-filter state or implicit changes to sample scope |
| `stdf-cli` | Ingest/analysis commands and offline report rendering | Validate arguments, then call shared APIs; do not duplicate identity/statistical logic |
| Versioned external configuration | Product, stage, hardware versions, change requests, experiments, stress information | Supply information absent from STDF explicitly, not through filename inference |

Check Cargo dependencies for cycles when extracting shared models. Move coordinate candidate
accumulators into the shared layer while keeping existing `stdf-arrow` exports. Extract traceability
`Attempt/Run`, flow matching, and reducers from the CLI. All future reports consume shared models;
do not maintain three independent device-merging implementations.

## Phase 10E: versioned test evidence datasets (planned)

**Goal:** Import STDF once and persist sufficient run, attempt, test-definition, and measurement
facts to rebuild traceability reports from a specified snapshot after removing the original STDF,
using the same evidence for yield and change analysis.

### 10E.1 Data model and version contract

Name the new schema `evidence-v1`, independently of `eav-v2`. Represent optional values with null
and reason codes; do not collapse empty strings, missing values, zeros, NaN, Infinity, and STDF
invalid markers into one empty state. Specify integer, time, floating-point precision, and raw-byte
representations explicitly in Arrow schemas. Follow 10D.5 type and raw/effective field contracts.
Pin the 10D.4 sanity profile/rule version at ingest and preserve field-evidence/diagnostic-bundle
hashes and references. First-value previews alone cannot release an entire source.

| Table/object | Primary key and associations | Required content |
| --- | --- | --- |
| `sources` | `source_id = SHA256(complete decompressed bytes)` | All source paths, byte count, input compression information, content hash, optional archive location; path changes do not change source_id |
| `decode_generations` | source_id + decoder/schema/config digest | Decoder/compatibility versions, record counts, diagnostics, content verification status; select only one generation per source per snapshot |
| `runs` | `run_id = H("run-v1", source_id, MIR_offset)` | Raw MIR fields, MIR/MRR offsets, run start/end times and validity; a source may contain multiple runs |
| `attempts` | `attempt_id = H("attempt-v1", source_id, PRR_offset)` | run_id, source-local part_sequence, head/site, PIR/PRR offsets, PART_ID, raw wafer/PRR XY, raw/resolved coordinate keys, PRR flags, effective/raw verdicts and bins, hardware snapshot |
| `test_definitions` | Content-addressed definition ID | Program name/version, test type/num/name, units, limits, scale/validity, definition provenance and scope; retain missing fields as unknown rather than assuming equivalence |
| `measurements` | `measurement_id = H("measurement-v1", source_id, record_offset, element_ordinal)` | attempt_id, definition_id, raw test flags, raw Float32 bits, raw/effective measurements, units/conversion rules, record position, repeated-occurrence ordinal |
| `record_evidence` | source_id + record_offset | Unexpanded record types such as MPR/FTR, owning attempt, raw record bytes or verified persistent blob references, parsing coverage |
| `analysis_bindings` | snapshot_id + configuration hashes | Versioned associations for flow steps, identity mappings, and product/stage/hardware enrichment; never overwrite raw facts |

`H` applies SHA-256 to canonical encoding with domain separation and field lengths. Do not concatenate
potentially ambiguous strings directly. Provide cross-platform golden vectors specifying byte order,
null/empty strings, and floating-point raw bits. Device keys cannot serve as attempt_id, and measurement_id
cannot depend only on test_num. IDs exclude paths, read order, and ingest time. Retain existing traceability
rules for `part_sequence` to support parity; new stable primary keys use raw record offsets.

Treat definition equality separately from comparability. Only identical complete definition IDs imply
identical definitions by default. Cross-program-version semantic equivalence requires versioned
`test_equivalence` mappings. Unit conversions require explicit transformations, scope, and test evidence.
Unknown units, different limits, or equal numbers with different meanings must not be pooled by default.

Use explicit enrichment for development/hardware metadata: `product_id`, `silicon_revision`, `stage`
(engineering_samples / qualification / production), `test_domain` (CP / FT), `hardware_type/id/revision`,
and `change_id`. Save a program hash only when the actual program file is provided; MIR program
name/version is not a content hash. Record each field's provenance and matching scope; reject conflicts.
Do not infer stage from CP/FT, date, or temperature. Product/factory identifier domains restrict analysis
sample scope without altering established wafer/lot coordinate-merging rules. Forbid automatic
cross-product pairing when the identifier domain is uncertain.

### 10E.2 Streaming ingest, integrity, and transactional publication

Proposed interfaces:

```text
zstdf-cli ingest-evidence <inputs...> --output-dir <dataset>
  [--enrichment <json>] [--archive-source]
  [--memory-limit-mib N] [--disk-limit-mib N] [--temp-dir DIR]
  [--max-sources N] [--max-output-files N] [--cancel-file FILE]
zstdf-cli verify-evidence <dataset> [--snapshot <id>]
```

Implementation sequence:

1. Reuse streaming decoding and head/site tracking to emit unified RunStarted, MeasurementObserved,
   AttemptCompleted, and RunFinished events. Write one attempt per PRR, including devices without PTR.
2. Deduplicate sources by decompressed-content hash. Compute the hash before ingest and verify it
   again while parsing. Identical content at different paths adds aliases only. Directory sorting,
   thread counts, and fragment sizes must not change logical facts or primary keys.
3. PRR_offset is unknown when PTR arrives. Stream measurements to disk using an internal provisional
   ID based on source_id + PIR_offset. After PRR, write a provisional→attempt mapping and join through
   external sorting. Do not keep a unit's entire measurement history in memory while waiting for PRR.
   Intermediate join tables do not enter complete snapshots.
4. Initial numerical analysis covers PTR. Preserve PRR outcomes and record_evidence for MPR/FTR,
   marked unexpanded. Downstream requests for unsupported measurements return insufficient coverage,
   rather than silently omitting them and claiming every test passed.
5. Fully validate record formats, PIR/PRR pairing, run closure, foreign keys, definition conflicts,
   and counts by record type. Corrupt supported records, orphan measurements, duplicate open PIR,
   unclosed instances, or missing MRR fail by default.
6. Use `stdf-parquet::evidence` to write immutable generation fragments and manifests. Verify all tables
   and referenced blobs before atomically updating the current snapshot in `_evidence_catalog.json`.
7. Each invocation is all-or-nothing by default: any input failure leaves the current snapshot unchanged.
   Explicit recovery tools may reclaim completed but unreferenced generations. The initial version has
   no implicit partial-success mode.
8. Lock catalog writers and pin reader snapshots. New ingest during report generation does not affect
   existing readers. Create new generations for incompatible decoder/schema versions; never rewrite old files in place.

Suggested layout:

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
  sources/<source-id>.stdf                 # Archived only with --archive-source
  configs/<configuration-hash>.json
```

Manifests store schema/decoder/options digests, each object's hash/row count/byte count, record
coverage, parent snapshot, and source aliases. Derive snapshot IDs from canonical inventories,
excluding current time; save acquisition time and execution logs as separate provenance. Equal logical
facts need not produce identical Parquet bytes, but published objects must remain immutable.

Resource budgets cover batches, active contexts, external sorting, temporary join tables, archive
staging, open files, and final publication staging. The memory budget measures accounted memory,
not a hard process RSS limit; report both separately. Disk limits cover simultaneously live merge
inputs/outputs and staging, not just final files. Normal errors and cooperative cancellation clean up
only files owned by the invocation. Recovery after process termination follows 10D.3 ownership/lock rules.

### 10E.3 Rebuild traceability reports from datasets

Proposed extended interface:

```text
zstdf-cli traceability --dataset <evidence> --snapshot <id>
  --flow-config <json> --output <html>
  [--flow-closures <json>] [--identity-map <json>]
```

`--dataset` and direct STDF inputs are mutually exclusive; preserve the direct-input entry point.
Both use the same shared reducer. Raw facts do not freeze a particular flow match. New flow/identity
configurations create new analysis_bindings and analysis IDs. Reports preserve snapshot, configuration
digest, exclusion reasons, and engine version. Prefer archives when locating raw bytes. If the original
file is absent and unarchived, show “Raw bytes unavailable”; verified facts still support report reconstruction.

### 10E.4 Correctness validation and delivery gates

| ID | Scenario and validation method | Required result |
| --- | --- | --- |
| E01 | Repeated ingest of copies, renamed files, and gzip copies of one STDF | Unique sources content identity; no additional attempts/measurements; complete aliases |
| E02 | Interleaved sites, repeated PART_ID, standalone PRR, MPR/FTR-only input | Exactly one attempt per PRR, no cross-site measurement leakage, accurate coverage |
| E03 | Repeated same-device/test_num records; same number with different units/programs | Independent measurement IDs; no incorrect definition merging |
| E04 | One million PTR before PRR, tiny batches/fragments, different thread counts | Correct joins or explicit resource-limit failure; no requirement to retain all unit measurements in memory |
| E05 | Source changes between hashing/parsing, truncated records, corrupt references | Ingest/verify fails; old current snapshot and report bytes remain unchanged |
| E06 | Fault injection at every stage, cancellation, process termination, concurrent readers/writers | Readers see only complete old/new snapshots; no deletion of other tasks' objects |
| E07 | Generate direct traceability, then rebuild from evidence | Field-level equality for devices, attempts, step states, changes, order_unknown, and sources; only explicitly added provenance fields may differ |
| E08 | Remove original STDF; change flow or identity mapping | Original snapshot remains reconstructible; new configurations can reanalyze; raw facts and old reports remain unchanged |
| E09 | Different byte orders, NaN/Infinity, null, invalid flags, coordinate conflicts | Preserve raw evidence; derived validity follows existing coordinate rules; no fabricated normal values |
| E10 | Incompatible schema, changed decoder generation, old eav-v2 input | Explicit rejection of wrong formats; appropriate versions still read old snapshots; no silent migration |

Delivery gates: pass E01–E10; complete an end-to-end run of at least one million measurements within
resource gates; establish direct-STDF/dataset report parity on current synthetic and extended boundary
fixtures. Offline HTML filters, drill-down, and JSON export must reference the same snapshot.
**Use 10E as the standard input for later metrics only after completing this phase.**

## Phase 10F: step yield and retest analytics (planned)

**Goal:** Derive explicitly defined step yield and retest metrics from independent attempt histories,
locate losses, and retain unknown verdicts, identity limitations, and flow gaps. Depends on 10E;
does not change the legacy Dashboard's all-pass definition.

### 10F.1 Inputs and sample scope

Proposed command: `yield-report --dataset <dir> --snapshot <id> --config <yield.json> --output <html>`.
Configuration includes product, silicon revision, stage, CP/FT, flow ID/version, lot/wafer, analysis
time range, flow closures, and identity-map version. Initially, time filtering requires **the complete
run to fall within the window**. Report boundary overlaps and unknown times separately; do not infer
absolute timestamps from PRR relative durations.

A time window may exclude earlier attempts, so label results “first/latest within the selected observation
window.” “Complete-flow first/final result” requires an explicit, verified claim of complete step-history
coverage. Show known history outside the window in coverage; the earliest supplied file alone cannot
establish true first pass when complete history is unavailable.

The analysis unit is `(scope_id, device_identity, flow_id/version, step_id)`; scope fixes product and
identifier domain. Unresolved identities contribute to raw attempt counts without fabricated unique-device
counts. Stratify unlinked fallback devices separately and report association coverage. The population
consists of identified devices present in the input, not inferred dies that never appear.

### 10F.2 Metric contract

For a step, T is the number of reliably associated devices with an attempt in the observation window;
A is their attempt count. Also report all raw instances and identity exclusions so T is not mistaken
for the entire lot. Reuse conservative ordering: sequence within a source, a complete and noncontradictory
order across sources. Never choose first/final results by path, hash, or ingest order when chronology is unknown.

| Metric | Explicit definition |
| --- | --- |
| `first_known_devices` K1 | Devices in T with determinable order and a known effective verdict on their first attempt |
| `first_pass_devices` P1 | Devices in K1 whose first attempt passed |
| `first_pass_yield_known` | P1 / K1; null when K1=0; label as conditional yield among known first-attempt samples |
| `latest_known_devices` KL | Devices in T with determinable order and a known effective verdict on their last attempt; do not skip an unknown last result to choose an earlier one |
| `latest_pass_devices` PL | Devices in KL whose last attempt passed |
| `latest_yield_known` | PL / KL; null when KL=0; unclosed flows show latest-observed without claiming manufacturing completion |
| `retested_devices` | Devices in T with more than one attempt |
| `retest_device_rate` | retested_devices / T; null when T=0 |
| `extra_attempts` | A − T; unknown verdicts/order still count as actual repetitions |
| `recovery_rate_known` | Among devices with determinable first/last results: first Fail and last Pass count / first Fail count |
| `degradation_rate_known` | Among devices with determinable first/last results: first Pass and last Fail count / first Pass count |
| `ever_verdict_changed` | Devices with both valid Pass and Fail across all independent attempts, not only different endpoints |
| `bin_migration` | Valid hard/soft bin transitions for the same selected first/last attempts; invalid bins enter unknown categories |
| `step_coverage` | Tested, missing, pending, not-applicable, and indeterminate counts using existing flow/closure/stop rules |

Every ratio includes `numerator`, `denominator`, `excluded_by_reason`, `unknown_count`, and definition
version. For first/latest results, also report pass-fraction lower/upper bounds across T: `P/T` and
`(P + U)/T`, where U counts devices with undetermined corresponding first/latest verdicts. These are
**identification bounds for missing outcomes, not statistical confidence intervals**. Show coverage
alongside conditional yields so unknown proportions remain visible. Compute PRR verdict and bin
validity separately; missing bins cannot turn valid Pass into Fail. Failure stopping retains existing,
more conservative completeness rules.

Do not multiply step yields to obtain cross-step flow yield in the initial version. Any cohort-completion
metric must freeze the starting device set and verify all applicable required steps per device, reporting
missing/pending/stopped counts separately. Pareto attribution defaults to valid failed tests in the selected
attempt. PRR Fail without measurement details becomes `failure_cause_unavailable`; bin names cannot
establish parameter root causes.

### 10F.3 Implementation

1. Construct scope and flow bindings from a frozen snapshot; externally sort attempts by device/step.
2. The shared reducer emits first/latest selections, full-history changes, denominator ledgers, and
   step states, retaining selected attempt_id values.
3. A second disk-backed aggregation produces product/stage/lot/wafer/site/program/hardware strata.
   For cross-site retests, explicitly attribute first-site, latest-site, or attempt-site; do not mix definitions.
4. JSON metrics contain numerators, denominators, and evidence references. Derive HTML charts from
   that JSON, with all included samples and exclusions accessible by drill-down. Paginate/export
   high-cardinality details; fail at limits rather than silently truncating.

### 10F.4 Correctness validation and delivery gates

Create an independent hand-calculated fixture for one step: A=Pass, B=Fail→Pass, C=Pass→Fail,
D=Fail, E=Unknown, F=Pass→Fail→Pass, and G=overlapping cross-file Pass/Fail attempts.
H is missing, I is pending, and J is not_applicable after a preceding failure stop; H/I/J have no
attempt at this step.

Expect T=7, A=12, K1=KL=5, P1=PL=3, and conditional first/latest yields of 3/5. Retested devices=4,
extra_attempts=5, recovery=1/2, degradation=1/3. G contributes to change detection and retest counts
but not known first/latest denominators. F must show a historical verdict change despite Pass at both
endpoints. First/latest pass-fraction identification bounds are both [3/7, 5/7]. H/I/J enter the correct
flow states without increasing T.

Also test window truncation, unknown last results, bin-only changes, all-unknown results, zero
denominators, PRR without measurements, cross-site retests, unresolved identity, duplicate files,
input-order changes, and retest recovery after closure/stopping. Check count conservation in every
group and agreement between aggregate and filtered-detail numerators/denominators. Verify synthetic
data with an independent Python reference, not expectations derived from Rust output.

Delivery gates: exact integer/set equality for the hand-calculated fixture; ratios within declared
tolerances; dataset and direct-traceability attempt/flow parity; no fabricated final results; one million
measurements within 10D budgets; report filters and JSON denominators agree.

## Phase 10G: hardware correlation and Change Review (planned)

**Goal:** Build reproducible baseline/candidate comparisons for changes to probe cards, loadboards,
sockets, handlers, testers, sites, and other hardware, producing engineering review evidence packages.
Depends on 10E/10F. Ordinary Pearson correlation between test items does not replace paired agreement
analysis and acceptance decisions in this phase.

### 10G.1 Change configuration and pairing contract

Proposed interface:

```text
zstdf-cli change-review --dataset <dir> --snapshot <id>
  --change-config <change.json> --output <review.html>
```

`change.json` must specify change ID/version, purpose and hardware dimension, explicit baseline/candidate
selectors, product/revision/stage, the same step, program/test equivalence mappings, matching conditions
such as temperature/voltage, attempt-selection rules, minimum sample size/coverage, per-test acceptance
margins and units, and rule version. Baseline/candidate selections must not overlap; an attempt cannot
serve on both sides. Missing hardware revisions may be supplied by enrichment scoped precisely to
run/site/validity range, but conflicts must be rejected.

The default pairing key is `(scope, device_identity, step, test_equivalence_id, condition_stratum)`.
First select one attempt per side using the declared policy, then select measurement records. The
default is `unique`: multiple eligible attempts or repeated records for the same test on one side are
ambiguous, not averaged or reduced to the first row. Explicit first/latest selection is allowed only
with reliable order; preserve selection reasons and all excluded candidates. A measurement cannot
be reused across independent pairs. Sets without common devices may produce independent-cohort
descriptive comparisons only, not paired correlation or complete change-validation claims.

Flag confounding when program, limits, temperature, and hardware change together. Initially, only
explicit matching or configuration-approved strata enter hardware-effect analysis; do not attribute
observed differences to a single hardware factor. Match recipes/temperatures using declared numeric
normalization and tolerances. Preserve raw MIR strings; do not directly match Celsius/Fahrenheit or unknown voltages.

### 10G.2 Statistical outputs and acceptance rules

For each test, report baseline/candidate sample counts, pair count, unpaired/excluded reasons,
pairing coverage, means/standard deviations, scatter plots, difference distributions, and:

- Define paired differences consistently as `candidate − baseline`; report bias, difference standard
  deviation, and prespecified quantiles.
- Optionally provide paired-t bias CIs with confidence level, assumptions, and method. Engineering
  acceptance requires the entire CI within the predefined equivalence interval. Nonsignificant
  differences or a CI containing zero do not establish equivalence.
- Report Pearson r and OLS slope/intercept descriptively. Constant columns or insufficient samples
  return null/reason. Baseline measurements also contain error; OLS slope is not a validated causal calibration relationship.
- Use the same paired attempts for Pass→Fail, Fail→Pass, unknown, and bin transitions, with explicit denominators.
- Acceptance may jointly require bias CI, absolute-difference margins, coverage, verdict-flip limits,
  and design validity. Every test needs explicit thresholds or a descriptive-only, excluded-from-release label.

High correlation with a fixed offset must still be able to fail. Repeated measurements cannot inflate
independent sample size. If wafer/lot rather than die is the independent sampling unit, declare clusters
and use intervals matching the design, or return unsupported. The initial version may support only
explicit independent device pairs, marking other designs insufficient_evidence. Multi-test acceptance
must declare a `per_test` or `familywise` confidence strategy. A prespecified Bonferroni adjustment
may implement the latter; many individual intervals cannot be presented as an overall conclusion at
the same confidence level.

See [NIST confidence intervals for paired mean differences](https://itl.nist.gov/div898/handbook/prc/section3/prc312.htm)
for the statistical method. Equivalence margins, coverage, and joint acceptance strategies here are
preregistered product engineering rules.

### 10G.3 Implementation and evidence package

1. Parse and freeze selectors, test equivalence, unit transformations, and hardware enrichment.
2. Externally sort/join baseline and candidate data in `stdf-analytics`. Produce an auditable pair table
   containing both attempt_id and measurement_id values, matching conditions, exclusions, and differences.
3. Reducers compute stratified statistics without automatically pooling CP/FT or different measurement
   definitions. If bootstrap methods are added later, record seeds, resampling units, and software versions;
   do not enable them implicitly in the initial version.
4. Map each result to pass/fail/insufficient_evidence/not_applicable. Overall: any required failure
   means fail; otherwise any required insufficient evidence means insufficient; pass requires all
   applicable required items to pass and at least one required item to have been evaluated. Empty sets cannot pass vacuously.
5. HTML/JSON includes baseline/candidate snapshots, configuration digest, methods, thresholds, pair
   details, exclusion ledgers, and issues. New snapshots or rules create new review IDs; never rewrite old reviews.
6. Generate machine assessments and review materials only. Record human approvals separately;
   generating a report does not change production configuration.

### 10G.4 Correctness validation and delivery gates

| ID | Synthetic scenario | Acceptance expectation |
| --- | --- | --- |
| G01 | Candidate exactly equals baseline, finite with nonzero variance | All differences zero, r=1, slope=1; pass with sufficient coverage and sample policy |
| G02 | candidate = baseline + 0.2, bias margin ±0.1 | r=1 still does not pass; detect the fixed offset |
| G03 | candidate = 1.1 × baseline | Detect scale/slope and range-dependent differences; means alone cannot hide them |
| G04 | Zero variance, NaN, missing measurements, duplicate candidates, unknown order | Correct null metrics/diagnostics; no silent first-row selection or many-to-many pairing |
| G05 | Same coordinates but different wafers/lots or product domains; unmapped identities | No false pairs; unpaired counts reconcile |
| G06 | Convertible/unknown units; same number in different programs | Pair only through explicit, verified transformations/equivalence mappings |
| G07 | Program, temperature, and hardware all change, or cohorts contain different devices | Explicit confounded/unpaired status; no single-hardware-factor validation pass |
| G08 | Many repeated measurements, low pairing coverage, multi-test acceptance | No inflated independent n; apply configured coverage/confidence strategies |
| G09 | Changed input order/batch size or reimported copies | Same pair IDs, statistics, and decisions |
| G10 | Regenerate after changing snapshot or rule version | New review ID; old evidence package remains reproducible |

Freeze independent reference data/scripts: hand-calculated shift/scale cases plus Python/SciPy or R
paired-statistics references, with implementation versions. Require exact integer/pair-set equality;
compare floats/CIs using shared validation rules. Browser checks cover test/hardware/site filters,
anomaly location, pair drill-down, and full JSON export. Machine decisions must use the same thresholds
as the export. Complete at least one synthetic old-hardware→new-hardware review workflow before delivering 10G.

## Phase 10H: process capability, SPC and measurement systems (planned)

**Goal:** Validate process capability, temporal stability, and measurement systems. Use separate
configurations and method declarations for the two analysis categories, producing evidence objects
compatible with 10G for later reviews. Depends on 10E; reuse explicit 10F sample-selection policies.

### 10H.1 Cpk/Ppk and SPC specification

Proposed command: `capability-report --dataset <dir> --snapshot <id> --config <json> --output <html>`.
Configuration must specify product/stage/step/test definition, units and limit versions, lot/wafer/site/hardware
strata, attempt policy, observation window, rational subgroups, minimum groups/samples, distribution
assumptions, control charts/rules, control-limit sources, and a frozen baseline.

Select each device's measurement using the predeclared policy. Do not improve capability by choosing
only passing devices, best retests, or deleting out-of-specification points. Invalid measurements may
be excluded, but retain counts and provenance. Stratify different limit versions, program semantics,
temperatures, and equipment conditions by default; unknown definitions cannot be pooled directly.

Capability calculation contract:

```text
Cp  = (USL - LSL) / (6 * sigma_within)
Cpk = min(USL - mean, mean - LSL) / (3 * sigma_within)
Pp  = (USL - LSL) / (6 * s_overall)
Ppk = min(USL - mean, mean - LSL) / (3 * s_overall)
```

`s_overall` is the n−1 sample standard deviation of selected observations. Initially, `sigma_within`
may be restricted to pooled within-subgroup standard deviation from explicit rational subgroups,
with an estimator ID. Corrections such as c4 need separate method versions and reference validation.
Without valid subgroups, Cp/Cpk returns not_estimable; using overall standard deviation must not be
labeled Cpk. For one-sided specifications, output Cpu/Cpl or Ppu/Ppl; Cp/Pp are not applicable.
Explicitly report zero variance, insufficient n, LSL≥USL, and unknown units. Infinity does not establish
process qualification. Allow negative capability values when the mean is outside specification;
do not clip to zero. Capability interpretation assumes stability; see
[NIST process capability](https://www.itl.nist.gov/div898/handbook/pmc/section1/pmc16.htm).

Initially, implement I-MR charts with a frozen baseline and explicitly versioned rules. Without
reliable time order, time-series rules cannot run. Separate Phase-I baseline estimation from Phase-II
monitoring; new anomalies must not automatically recalculate limits. Show specification and control
limits separately. Unstable processes retain descriptive statistics but return insufficient_evidence
for capability release. Nonnormal transformations/distribution fitting are not implicit defaults;
unsupported initial methods return unsupported rather than extrapolating normal failure rates automatically.

Implementation: share selection/definition resolution, externally sort into subgroups/time series,
and use stable online moments for means/variances, retaining sample/group ledgers. The SPC engine
consumes ordered series and frozen limits, returning triggered rules, positions, and measurement_id
references. HTML presents values, methods, coverage, and validity together.

Validation:

- Inject an independent model with mean=0, sigma_within=1, LSL=−3, USL=3: Cp=Cpk=1;
  moving the mean to 1 gives Cpk=2/3. Also independently compute sigma from a real subgroup fixture;
  formula-only tests are insufficient.
- A fixture with identical within-group spread and shifting group means must preserve differences
  between within and overall variation, and between Cpk/Ppk.
- Regress one-sided limits, zero variance, negative Cpk, n=0/1, unknown units, mixed limit versions,
  and retest-selection bias.
- After freezing control limits, inject step shifts, drift, and out-of-control points; trigger the
  prespecified rules precisely without changing the training baseline.
- Compare estimators, limits, rule positions, and validity with independent NIST examples/reference
  scripts, not just final plots.

Delivery gates: reproducible capability values/methods; unstable/inapplicable cases cannot pass
capability gates; SPC alerts trace to original device measurements; batch/fragment changes do not
alter selection or decisions. Product thresholds require explicit configuration.

### 10H.2 GR&R specification

Proposed command: `grr-report --dataset <dir> --snapshot <id> --study-config <json> --output <html>`.
`study-config` defines study ID, characteristic/units, part list, appraiser factor, repeats, experiment
design/randomization order, condition/equipment/time blocks, measurement→experimental-unit mappings,
and acceptance methods. For automated testing, an appraiser may be an explicitly declared setup/equipment
factor; do not combine unknown operators into one person. Ordinary production retests have different
selection mechanisms and cannot automatically become GR&R study samples.

Initially support complete, balanced crossed studies with equal repeats per part × appraiser cell.
For example, 10 parts × 3 appraisers × 3 repeats = 90 independently registered measurements is a test
fixture only. At least 2 levels for factors/repeats is an estimability check, not a sufficient product
sample-size recommendation. Missing cells, reuse of one measurement, nested/unbalanced designs,
and drift confounding require explicit errors or insufficient evidence. Later unbalanced/REML support
requires separate method versions and validation, not a silent algorithm switch.

The model explicitly includes part, appraiser, part×appraiser interaction, and repeatability error.
Output ANOVA/variance components, repeatability, reproducibility with an explicit interaction statement,
part-to-part and total variation, %study variation, %contribution, and %tolerance when two-sided
specifications permit it. Distinguish standard-deviation ratios from variance ratios; retain multipliers
such as 6σ and denominators. Zero denominators return not estimable. If negative variance components
are truncated to zero, preserve raw estimates and boundary flags; boundary estimates do not prove
zero error. Retain interaction by default rather than removing it based on one p-value. Declare and
validate pooling rules separately. See [NIST Gauge R&R](https://www.itl.nist.gov/div898/handbook/mpc/section4/mpc4.htm)
for experimental-design requirements.

Implementation: assemble study matrices through evidence references, validate design completeness
first, then implement balanced two-way random-effects ANOVA in a separate analytics module.
Charts show measurements by part/appraiser/repeat, interactions, and residuals. Diagnose failed model
assumptions. Results provide independent evidence for 10G/10I; do not automatically apply generic
industry 10%/30% rules.

Validation fixtures cover pure repeatability, fixed appraiser shifts, known interactions, large
part-to-part differences, identical-reading boundary cases, missing cells, and duplicate measurements.
Use fixed seeds for supplementary data and independent reference ANOVA to check mean squares,
variance components, and percentages. Factor renaming and input order must not alter results.
With equal measurement error but greater part-to-part variation, changes in %study variation and
%tolerance must follow their respective denominators, preventing confusion between the two percentages.

### 10H.3 Integration and delivery gates

After separate acceptance of 10H.1 and 10H.2, 10G may reference capability, SPC, and GR&R artifact
IDs/summaries. Referenced artifacts must match product, conditions, hardware, and method versions.
The mere existence of a GR&R report does not establish coverage of current change conditions.
The browser must expose referenced coverage, invalidity/inapplicability reasons, and original
experimental units. Deliver only after numerical reference agreement, explicit failure for unsupported
designs, and traceable threshold configuration.

## Phase 10I: reliability evidence and lifecycle stage gates (planned)

**Goal:** Link CP/FT electrical results to stress, life, readpoints, and sample plans for engineering
samples, qualification, and production reviews. STDF Pass or zero observed failures alone cannot
establish reliability acceptance.

### 10I.1 Reliability data contract

Proposed command: `ingest-reliability --dataset <dir> --plan <json> --events <jsonl>`.
Initially use explicit JSON/JSONL schemas; later CSV/laboratory-system adapters must map to the
same model and be validated independently.

| Object | Required content and associations |
| --- | --- |
| `reliability_plan` | Plan ID/version, product/revision/stage, applicable standard/version or internal method, sample cohort, stress/readpoint plan, failure criteria, statistical assumptions, confidence level, acceptance targets |
| `sample_enrollment` | Complete device identity or explicit package-serial mapping, lot/wafer, sampling method, enrollment time; never infer unmapped samples from coordinates |
| `stress_event` | Sample/cohort, stress type, temperature/voltage/humidity or cycles, units, start/end, cumulative exposure, equipment/chamber, abnormal interruptions, provenance |
| `readpoint` | Planned/actual exposure point, baseline/post-stress label, linked electrical attempt_id, covered tests/definitions, completeness |
| `failure_observation` | Sample, failure criterion/mode, exact time or failure interval, exposure unit, evidence references, validity, disposition |
| `censor_observation` | Sample, censoring point/type such as right/interval, withdrawal reason, possible informative censoring; withdrawals cannot count directly as successes |
| `stage_assessment` | Assessment ID, policy version, all frozen evidence artifacts, per-item machine decisions and reasons |
| `review_decision` | Assessment ID, declared reviewer/role, decision, time, rationale, waiver references; separate from machine results |

Preserve source event IDs/hashes and make repeated ingest idempotent. Corrections use superseding
events and new snapshots, never overwriting reviewed events. Specify time basis, timezone, and
temperature/voltage/hour/cycle units. Cumulative exposure is not wall-clock time. Do not double-count
duplicate/overlapping intervals; label unknown intervals explicitly. Electrical retest recovery does not
automatically retract observed reliability failures; apply the plan's failure definition and engineering disposition.

### 10I.2 Analysis scope and implementation

Proposed command: `reliability-report --dataset <dir> --snapshot <id> --plan <json> --output <html>`.
The initial version has two independently acceptable analysis modes:

1. **Attribute validation at a fixed exposure endpoint:** At most one Pass/Fail outcome per sample.
   Report enrolled samples, endpoint completions, failures, not-yet-reached/missing/early-withdrawal
   counts, and binomial confidence bounds with explicit methods. Apply only to determinable outcomes
   at the same endpoint under independent, identically distributed sampling assumptions. Loss to follow-up
   is not success. Inadequate planned coverage yields insufficient_evidence overall even if the evaluable
   subset has zero failures.
2. **Life observations:** Implement Kaplan–Meier for exact failures plus noninformative right censoring,
   recording tie handling and interval-estimation methods. At a shared time, process failures in the risk
   set before censoring. Retain interval-censored failures as intervals with an unsupported-estimator
   message; do not fabricate exact times from midpoints.

Do not directly pool different stresses, failure modes, or exposure units. Initially, do not automatically
convert high-temperature test hours to service life or produce FIT/MTBF, Weibull, or accelerated-life
claims without model support. Later distribution/acceleration models require new model versions,
parameter sources, applicable failure mechanisms, fit diagnostics, sensitivity analysis, and independent
validation. See [NIST censoring](https://www.itl.nist.gov/div898/handbook/apr/section1/apr131.htm)
for treatment of censored and nonfailed samples.

Implementation: reuse 10E snapshot transactions to persist events. Externally sort by sample to
compute exposure/readpoint status. Electrical before/after comparisons reuse 10G definition mappings
and measurement pairing. Statistical modules consume only data passing plan/completeness checks,
reporting descriptions, assumptions, nonevaluable reasons, and traceable sample ledgers. Report
parameter drift separately from failure classification.

### 10I.3 Development-stage gates and human review

Proposed command: `stage-review --dataset <dir> --snapshot <id> --policy <json> --output <html>`.
Policies define product/revision/stage requirements; do not hard-code identical tests for every product.

| Stage | Configurable evidence categories | Required limitations |
| --- | --- | --- |
| Engineering samples | Identity/flow coverage, initial yield, parameter distributions, basic hardware comparisons | Small samples and selection bias; no unvalidated extrapolation to production capability |
| Qualification | Frozen programs/hardware, required reliability studies, GR&R, capability, open changes | Insufficient samples/readpoints, stress interruptions, unresolved failures, missing applicable standards/versions |
| Production | Approved baselines, SPC, yield/retest trends, equipment change reviews, planned ongoing reliability monitoring | Expired baselines, mismatched hardware versions, incomplete data windows, unapproved changes |

Policies specify required artifact types, coverage, freshness, revision matching, thresholds, and waiver
rules. Each item returns pass/fail/insufficient_evidence/not_applicable; overall aggregation follows
10G's nonempty-required-item rule. Ordinary configuration cannot arbitrarily mark mandatory items
not_applicable; satisfy policy applicability conditions or record a separately authorized waiver.
A passing report for an older revision does not automatically release a new revision.

Keep machine assessment separate from human approval. HTML generation does not create a formal
production release. Human decisions reference the assessment hash and record role, time, and rationale;
later evidence changes create a new assessment. Reviewer fields recorded by the offline CLI are
self-declared. Formal access control, signatures, and authentication integration require separate
validation; an editable JSON name is not authenticated approval. Initially deliver machine assessments
and offline review packages.

### 10I.4 Correctness validation and delivery gates

| ID | Scenario and validation method | Required result |
| --- | --- | --- |
| I01 | Fixed endpoint, zero failures, all samples complete exposure | Report sample size, target exposure, and confidence bounds; never claim proven 100% reliability |
| I02 | Equal independent sample counts, different early-withdrawal/missing-readpoint proportions | Withdrawals do not become Pass; insufficient planned coverage blocks overall acceptance |
| I03 | Hand-calculated small sample with exact failures/right censoring | Exact risk sets and KM products at each time; all-censored curves do not fabricate failure times or life guarantees |
| I04 | Interval censoring, informative withdrawal, mixed units/stresses | Preserve raw information and report inapplicable methods/conditions; no hidden conversion to ordinary KM |
| I05 | Unmapped readpoint/CP/FT identities, duplicate events, shuffled input | No false associations or duplicated exposure/samples; deterministic results independent of input order |
| I06 | Post-stress Fail followed by electrical retest Pass | Preserve plan-defined failure history; do not automatically clear reliability events |
| I07 | Missing/expired/wrong-revision required reports, empty policies, unauthorized N/A | No stage-gate pass; every blocker is traceable |
| I08 | All required evidence valid; snapshot changes before/after human records | New assessment requires separate review; old human decisions do not transfer implicitly |

Use a preselected exact one-sided binomial confidence bound for fixed endpoints and compare with
an independent reference. With zero failures and all n independent samples completing exposure,
the hand-calculated oracle is `R_lower = alpha^(1/n)`. At 95% one-sided confidence, the lower bound
is approximately 0.9505 for n=59 and 0.7411 for n=10. These verify computation, not prescribed sample
sizes for any chip product. Do not use the model for release if its assumptions fail.

KM fixture: 4 samples; 1 failure at t=1, 1 censor at t=2, 1 failure at t=3, and the last censor at t=4.
Expect S(1)=3/4 and S(3)=3/8. Also test failure/censor ties, zero time, negative exposure, and impossible
ordering. Reference scripts and Rust outputs must agree on risk sets/states.

Delivery gates: pass I01–I08 and model oracles. Every electrical/stress/stage conclusion traces to
input events and snapshots; unknown/unsupported conditions prevent improper automatic release.
Complete a synthetic demonstration from enrollment to stage review. Real product qualification claims
require separate applicable plans, engineering review, and validation evidence using actual data.

## Post-10D validation and release checklist

The following specifies acceptance for future implementation; these are not tests executed as part
of this documentation update.

### Independent expectations and numerical tolerances

- IDs, integer counts, selected sample sets, pair sets, states, and exclusion reasons must match exactly.
- Raw Float32/Float64 bits, field raw bytes, record offsets, and file hashes must match bit for bit.
  Derived Float64 statistics default to `abs(actual-ref) <= 1e-10 + 1e-8 * abs(ref)`.
  Algorithms such as inverse distributions/CIs needing different tolerances must document their
  numerical basis per test; do not relax tolerances afterward merely to pass.
- Each statistical module needs at least one hand-calculated oracle, an independent reference
  implementation, and boundary/rejection cases. Record reference versions and random seeds;
  do not generate expected values with the same Rust algorithm under test.
- Test values just below, equal to, and just above decision thresholds. Compare unrounded numbers
  and declare inclusive/exclusive boundaries. Test statistical significance, engineering equivalence,
  and acceptance decisions separately.

### Failures, scale, and UI

| Check | Validation requirement |
| --- | --- |
| Resources | Test 100,000/1,000,000 measurements with fixed memory/disk settings, including huge single-device histories, high-cardinality definitions, many sources/sites. Record accounted peak, RSS, live scratch peak, file count, and elapsed time separately |
| RSS gate | Initial end-to-end evidence baseline uses a 128 MiB accounted budget, with default peak RSS ≤256 MiB and growth ≤32 MiB from 100,000 to 1,000,000 measurements. This is a release gate awaiting measurement, not an achieved promise. Record each OS separately and justify gate changes |
| Limits | Explicitly fail when input exceeds configured limits; no silent measurement loss, sampling, or truncation; all unknowns/exclusions are enumerable |
| Fault injection | Inject errors at hash/parse, fragment writing, external sorting, cross-table joins, manifest/catalog publication, and HTML replacement boundaries |
| Cancellation/process termination | Check cooperative cancellation at the final commit boundary. After kill, readers see only complete snapshots; recovery cannot delete other owners' or active tasks' files. Killing exactly at atomic commit may leave the complete old or complete new version |
| Browser | 1440×900 and 390×844; filtering, pagination, drill-down, highlights, unknown states, complete JSON export, zero external requests, malicious-string safety, no JS errors |
| Reproducibility | Repeated runs with the same snapshot/configuration/engine agree. Input order, threads, and batch sizes do not alter logical results; snapshot/configuration changes produce new analysis IDs |
| Real data | Use explicitly authorized product data and independent engineering baselines; record product/version/coverage. Synthetic-test success does not establish real-product validation |

At each phase's delivery, run applicable existing checks and add phase-specific integration/reference tests:

```text
cargo fmt --all --check
cargo test --workspace --exclude stdf-py --locked --offline
cargo check -p stdf-py --locked --offline
cargo build -p stdf-cli --release --locked --offline
```

The first build needs network access if dependencies are uncached; failed downloads are not successful
validation. Linux/macOS require their own build/run records, not inherited Windows conclusions.
Delivery reports list executed commands, fixtures/snapshots, actual results, resource data, and untested
conditions. Change phase status from planned to implemented only after its exit gates pass.

Suggested incremental delivery order: **10D.4 CP/FT sanity and field evidence → 10D.5 type fidelity
and encoding baseline → 10E.1 shared model → 10E.2 ingest/snapshots, including dimension-table storage
optimization → 10E.3 traceability reconstruction and parity → 10F yield with explicit denominators →
10G one complete hardware-change case → 10H separate capability and GR&R validation → 10I reliability
readpoints and stage review**. The first runnable milestone for the new prerequisites is “CP/FT full-file
checks plus important run fields and first-value previews of each unit's first two records per type.”
The first evidence-dataset milestone remains “rebuild the same traceability report from a pinned
evidence snapshot after removing original STDF.” Empty tables or command placeholders are insufficient delivery.
