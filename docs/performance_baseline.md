# Performance Baseline

Phase 7B adds a repeatable synthetic benchmark runner that does not require
nightly Rust or external tooling.

## CLI run telemetry

`convert`, `view` (ingest/export/serve), and analysis `execute` emit local-first
`RunTelemetry` events: phase name, elapsed/phase milliseconds, counters, budget
use, and a terminal status (`ok`, `cancelled`, `quota`, or `error`). Stderr
receives one-line summaries. Pass `--telemetry-json <path>` to append JSONL
objects with `version: run-telemetry-v1`. Omitting the flag preserves prior
command results; only optional stderr/JSONL diagnostics are added. This is not
OpenTelemetry and does not export spans to a collector.

Synthetic acceptance sketch:

```powershell
zstdf-cli convert input.stdf --output-dir dataset --layout catalog --telemetry-json tel.jsonl
zstdf-cli view input.stdf --cache-dir cache --export-html out.html --telemetry-json tel.jsonl
```

Cancel/quota failures emit a matching terminal event before the process exits
nonzero. Analysis callers can pass the same helper around `analysis::execute`.

Run:

```powershell
cargo run -p stdf-core --bin perf_baseline -- --parts 10000 --tests-per-part 5
```

The runner reports:

- synthetic byte size
- decoded record count
- whole-buffer decode elapsed time
- iterator decode elapsed time
- records per second
- deterministic input checksum

## Regression Policy

Use the same machine, compiler, and command-line parameters when comparing
results. Treat a sustained records/sec drop above 15% as a regression candidate
unless the change intentionally adds validation or more complete decoding.

## Fuzz Smoke

Run:

```powershell
cargo run -p stdf-core --bin fuzz_decode -- --iterations 10000 --max-len 4096
```

This deterministic fuzz smoke mutates checked-in golden fixtures and exercises:

- byte-order detection
- `decode_all`
- `decode_with_summary`
- `summarize`

It is intended as a fast local/CI guard. It is not a replacement for coverage-
guided fuzzing with a tool such as `cargo-fuzz` once the project adds that
dependency explicitly.

## Parquet Pipeline Timing

For STDF -> Parquet attribution, use:

```powershell
cargo run -p stdf-parquet --bin parquet_perf -- --parts 10000 --tests-per-part 5 --batch-size 65536
```

See `docs/parquet_profiling.md` for profiler-specific workflows.

The production conversion path is streaming: decoded STDF records are flushed
into Arrow batches and written to Parquet without collecting all batches in a
`Vec`. Keep `--batch-size` large enough for writer efficiency but small enough
for worst-case server memory limits. For crash/retry robustness, CLI and Python
conversion write through same-directory temporary files, commit the final
Parquet atomically, and write a JSON manifest beside the output.
