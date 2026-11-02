# SE matrix reassessment (post low-score fixes)

Status: evidence-based rescore after Waves 1–4 remainders plus adversarial
corner-case adapter tests on branch `feature/corner-case-tests-and-rescore`
(baseline main includes `#1`–`#5`).

Companion canvas:
`canvases/stdf-se-matrix-reassessment.canvas.tsx` (Cursor project canvases).

## Method

Same 1–5 rubric as the prior review. Scores move only when shipped code/tests/docs
changed the evidence. Dashboard `coordinate-v1` yield math was not modified.
Pareto ANA-002 and dashboard SpillStore rewrite remain open (documented skip).
Identity policies stay distinct (attempt-yield / latest-device vs Dashboard merge).

## Before → prior → latest

| Dimension | Original | Prior (~Waves 1–4) | Latest | Why (latest delta) |
| --- | ---: | ---: | ---: | --- |
| Layering & modularity | 4.0 | 4.0 | 4.0 | Shared reducers in `stdf-analytics`; CLI still I/O host |
| Cohesion of concerns | 3.0 | 3.5 | 3.5 | `summary_ops` + `attempt_yield_op` host analytics |
| Extensibility seams | 3.0 | 3.5 | 3.5 | Versioned request dispatch for site/lot/retest/yield |
| Domain fidelity | 4.5 | 4.5 | 4.5 | Tied/missing START_T + unresolved overlap asserted on adapter |
| Schema & versioning | 4.0 | 4.5 | 4.5 | `analysis-receipt-v1` + ANA-002 result versions |
| Provenance & reproducibility | 3.5 | 4.0 | 4.5 | `result_hash` stable under reorder; ANA-002 receipt replay |
| Reliability & failure model | 3.5 | 4.0 | 4.5 | Mid-scan cancel inject + corrupt ANA-002 empty-scope fail |
| Performance engineering | 3.5 | 3.5 | 3.5 | SpillStore not wired into dashboard (documented skip) |
| Trust boundary | 3.5 | 4.0 | 4.0 | Path-traversal denial tests retained |
| Observability | 2.0 | 3.0 | 3.0 | `run-telemetry-v1` on convert/view/analyze |
| Operability & portability | 3.0 | 3.5 | 3.5 | Linux CI job (Rust/clippy/test; no Edge smoke) |
| Test strategy | 4.0 | 4.0 | 4.5 | 12 adversarial CLI corner cases over shipped adapter |
| CI & automation | 3.0 | 3.5 | 3.5 | Clippy -D warnings + Linux job + cargo-deny |
| Process & governance | 4.5 | 4.5 | 4.5 | Unchanged multi-agent / review policy |
| Maintainability | 3.0 | 3.5 | 4.0 | Analysis adapter + dashboard HTML/JSON render split (`#5`) |
| Analytics completeness | 2.5 | 3.5 | 3.5 | Latest yield + site/lot/retest; Pareto still deferred |
| API / release stability | 3.0 | 3.5 | 3.5 | Public `snapshot analyze` + CHANGELOG contracts |

Approximate weighted maturity: **~3.3 → ~3.8 → ~4.0** (same weights as prior canvas).

## Residuals

1. Pareto / full 10F step analytics still deferred.
2. Dashboard SpillStore rewrite skipped to protect yield math.
3. Linux CI omits Playwright/Edge viewer smoke by design.
4. Loopback-only multi-user trust model unchanged.
5. Independent semantic review of this delta still required before calling the
   review gate verified.

## Verification recorded here

```text
cargo test -p stdf-analytics --locked -- summary_ops attempt_yield
cargo test -p stdf-cli --locked -- attempt_yield analysis_receipt latest_device site_variation lot_comparison retest_impact analysis_path
cargo fmt --all
cargo clippy -p stdf-analytics -p stdf-cli --locked -- -D warnings
```

Corner cases added in this pass (CLI `analysis_tests.rs`): absent vs empty scope
`result_hash`, unresolved pass/fail/unknown overlap, tied/missing START_T latest,
reorder-stable `result_hash`, mid-scan cancel, site scope/excludes + zero-denom +
corrupt empty scope, multi-lot comparison, lot `latest_device`, retest receipt replay.
