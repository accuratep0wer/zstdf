# SE matrix reassessment (post low-score fixes)

Status: evidence-based rescore after Waves 1–4 remainders on branch
`feature/se-low-score-remain-and-rescore` (baseline main `bfa9649`).

Companion canvas:
`canvases/stdf-se-matrix-reassessment.canvas.tsx` (Cursor project canvases).

## Method

Same 1–5 rubric as the prior review. Scores move only when shipped code/tests/docs
changed the evidence. Dashboard `coordinate-v1` yield math was not modified.
Pareto ANA-002 and dashboard SpillStore rewrite remain open (documented skip).

## Before → after

| Dimension | Before | After | Delta | Why |
| --- | ---: | ---: | ---: | --- |
| Layering & modularity | 4.0 | 4.0 | 0 | Shared reducers moved into `stdf-analytics`; CLI still hosts I/O |
| Cohesion of concerns | 3.0 | 3.5 | +0.5 | `summary_ops` + `attempt_yield_op` host analytics; CLI adapter thin |
| Extensibility seams | 3.0 | 3.5 | +0.5 | Versioned request dispatch for site/lot/retest/yield |
| Domain fidelity | 4.5 | 4.5 | 0 | Distinct identity policies preserved |
| Schema & versioning | 4.0 | 4.5 | +0.5 | `analysis-receipt-v1` + ANA-002 result versions shipped |
| Provenance & reproducibility | 3.5 | 4.0 | +0.5 | Receipts + replay + publication-fault injection tests |
| Reliability & failure model | 3.5 | 4.0 | +0.5 | Mid-publish fault + cancel-aware scan + corrupt-evidence fail |
| Performance engineering | 3.5 | 3.5 | 0 | SpillStore not wired into dashboard (documented skip) |
| Trust boundary | 3.5 | 4.0 | +0.5 | Path-traversal denial tests for evidence/analysis IDs |
| Observability | 2.0 | 3.0 | +1.0 | `run-telemetry-v1` on convert/view/analyze |
| Operability & portability | 3.0 | 3.5 | +0.5 | Linux CI job (Rust/clippy/test; no Edge smoke) |
| Test strategy | 4.0 | 4.0 | 0 | More synthetic fixtures; browser still Windows-local |
| CI & automation | 3.0 | 3.5 | +0.5 | Clippy -D warnings + Linux job + existing Windows suite |
| Process & governance | 4.5 | 4.5 | 0 | Unchanged multi-agent / review policy |
| Maintainability | 3.0 | 3.5 | +0.5 | Analysis adapter present; dashboard.rs render split deferred |
| Analytics completeness | 2.5 | 3.5 | +1.0 | Latest yield + site/lot/retest; Pareto still deferred |
| API / release stability | 3.0 | 3.5 | +0.5 | Public `snapshot analyze` + CHANGELOG contracts |

Approximate weighted maturity: **~3.3 → ~3.8** (same weights as prior canvas).

## Residuals

1. Pareto / full 10F step analytics still deferred.
2. Dashboard SpillStore rewrite skipped to protect yield math.
3. `dashboard.rs` HTML/render split not done (optional Wave 3).
4. Linux CI omits Playwright/Edge viewer smoke by design.
5. Loopback-only multi-user trust model unchanged.
6. Independent semantic review of this delta still required before calling the
   review gate verified.

## Verification recorded here

```text
cargo test -p stdf-analytics --locked -- summary_ops attempt_yield
cargo test -p stdf-cli --locked -- attempt_yield analysis_receipt latest_device site_variation lot_comparison retest_impact analysis_path analysis_receipt_publication
cargo clippy -p stdf-analytics -p stdf-cli --locked -- -D warnings
```
