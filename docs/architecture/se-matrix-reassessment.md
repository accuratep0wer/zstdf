# SE matrix reassessment (post low-score fixes)

Status: evidence-based rescore after TDD residuals on branch
`feature/se-tdd-residuals` (baseline main includes `#1`–`#6`).

Companion canvas:
`canvases/stdf-se-matrix-reassessment.canvas.tsx` (Cursor project canvases).

## Method

Same 1–5 rubric as the prior review. Scores move only when shipped code/tests/docs
changed the evidence. Dashboard `coordinate-v1` yield math was not modified.
Pareto ANA-002 and dashboard SpillStore rewrite remain open (documented skip with
spike evidence). Identity policies stay distinct (attempt-yield / latest-device vs
Dashboard merge).

## Before → prior → latest

| Dimension | Original | Prior (~corner) | Latest | Why (latest delta) |
| --- | ---: | ---: | ---: | --- |
| Layering & modularity | 4.0 | 4.0 | 4.0 | Shared reducers in `stdf-analytics`; CLI still I/O host |
| Cohesion of concerns | 3.0 | 3.5 | 3.5 | `summary_ops` + `attempt_yield_op` host analytics |
| Extensibility seams | 3.0 | 3.5 | 4.0 | Wafer comparison joins versioned analyze dispatch |
| Domain fidelity | 4.5 | 4.5 | 4.5 | Missing vs empty wafer labels kept distinct |
| Schema & versioning | 4.0 | 4.5 | 4.5 | `wafer-comparison-*-v1` + existing ANA-002 versions |
| Provenance & reproducibility | 3.5 | 4.5 | 4.5 | Wafer receipt replay stable |
| Reliability & failure model | 3.5 | 4.5 | 4.5 | Stream scan still fails corrupt empty-scope evidence |
| Performance engineering | 3.5 | 3.5 | 4.0 | Per-source stream reduce; no multi-source attempt Vec |
| Trust boundary | 3.5 | 4.0 | 4.0 | Path-traversal denial tests retained |
| Observability | 2.0 | 3.0 | 3.0 | `run-telemetry-v1` on convert/view/analyze |
| Operability & portability | 3.0 | 3.5 | 3.5 | Linux CI job (Rust/clippy/test; no Edge smoke) |
| Test strategy | 4.0 | 4.5 | 4.5 | Wafer + streaming corrupt fixtures; goldens held |
| CI & automation | 3.0 | 3.5 | 3.5 | Clippy -D warnings + Linux job + cargo-deny |
| Process & governance | 4.5 | 4.5 | 4.5 | Unchanged multi-agent / review policy |
| Maintainability | 3.0 | 4.0 | 4.0 | Adapter + dashboard render split retained |
| Analytics completeness | 2.5 | 3.5 | 4.0 | Site/lot/wafer/retest + latest; Pareto still deferred |
| API / release stability | 3.0 | 3.5 | 3.5 | Public `snapshot analyze` + CHANGELOG contracts |

Approximate weighted maturity: **~3.3 → ~4.0 → ~4.1** (same weights as prior canvas).

## Residuals

1. Pareto / FTR pattern-attempt population still deferred (needs execution evidence).
2. Dashboard SpillStore rewrite skipped again after spike: SpillStore is an
   external-sort store, not a map backend; no bitwise KPI golden suite yet.
3. Linux CI omits Playwright/Edge viewer smoke by design.
4. Loopback-only multi-user trust model unchanged.
5. Independent semantic review of this delta still required before calling the
   review gate verified.

## Verification recorded here

```text
cargo test -p stdf-analytics --locked -- summary_ops attempt_yield
cargo test -p stdf-cli --locked -- attempt_yield analysis_receipt latest_device site_variation lot_comparison wafer_comparison retest_impact streaming_scan analysis_path
cargo fmt --all
cargo clippy -p stdf-analytics -p stdf-cli --locked -- -D warnings
```

This pass added: `wafer-comparison-request-v1` reducer + CLI adapter coverage,
streaming inventory→scan reduce (inventory parity enforced), and documented
Dashboard spill skip spike evidence.
