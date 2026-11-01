# Revised implementation backlog

Status: proposed work packages, not seven completed development days. The supplied
50-60-hour estimate is unvalidated. The first week should target the smallest
complete reproducibility slice; if a dependency fails, carry its work forward
instead of skipping correctness, review or resource limits.

## Initial slice and non-goals

Deliver a verified, historically pinned dataset selection, an attempt-complete
adapter, one PRR yield operation, and an analysis receipt. Prefer single-source
viewer companion reuse first, then compose verified multiple sources without
inventing cross-source device relationships. Keep all existing CLI/report paths
compatible. Later lot/site/retest operations depend on this contract.

No mandatory DuckDB, LLM, cloud account, server/client deployment, new UI toolkit,
`art-v1` migration or HA promise is included. Existing MPR/FTR/raw-record inspection
and no-measurement attempts remain supported. Existing development multi-agents
continue coordinating work; runtime AI is a separate, deferred feature.

## Seven ordered work packages

| Order / issue | Implementation and owner | Depends on | Acceptance |
| --- | --- | --- | --- |
| 1 / ARCH-001 | Coordinator: finalize [contracts](data-contracts.md), policy matrix, fixtures and [failure model](failure-model.md); independent reviewer checks semantic separation | Source audit | Complete-vs-partial, dedup, raw retention, ordering, result hash and compatibility decisions explicit |
| 2 / SNAP-001 | Storage worker: immutable manifests, resolver, local writer lock/current pointer and pins; extend `stdf-parquet` catalog interfaces without changing converter commit semantics | ARCH-001 | Resolve S1 after S2 publication; injected boundary failures never expose an incomplete snapshot; no GC |
| 3 / DATA-001 | Backend worker: map existing viewer companions/provenance into the snapshot contract; version any extracted shared adapter | ARCH-001 and SNAP-001 contract | Import companions into retained storage; survive temporary cache removal; reconcile every PRR and PTR/MPR/FTR execution; PRR-only units and repeated channels/patterns retained; incompatible/missing coverage rejected |
| 4 / ANA-001 | Analysis worker: typed public adapter for exact all-attempt PRR yield; coordinator owns shared interface changes | SNAP-001, DATA-001 | Hand-checked pass/fail/unknown counts, explicit two denominators and zero-denominator null; no EAV row-count surrogate |
| 5 / AUD-001 | Backend worker: canonical request/result encoder and append-only receipt bundle publication | ANA-001 | Repeat hash under pinned build; result/receipt crash consistency; source/policy/config/build provenance; raw replay limits visible |
| 6 / QA-001 | Independent evaluator: integration, adversarial and publication-fault fixtures; coordinator serializes runs | 2-5 integrated | Failure matrix passes for changed paths, no regression to existing CLI and live/offline reports |
| 7 / REL-001 | Coordinator: CLI docs and synthetic end-to-end demo; incremental CI support; independent final review | QA-001 | Import, pin S1, summarize+receipt, publish S2, replay S1; declared platform/test evidence attached; unresolved gates remain pending |

ARCH-001 receives a documentation draft in this review. Its executable schemas,
policy registry and fixtures remain implementation work. The other packages are
not implemented by adding these documents.

A reasonable week-one checkpoint is contracts plus the pinned-snapshot vertical
slice. Do not promise all seven packages finish within seven calendar days. Scope
must follow measured effort and acceptance results.

## Multi-agent ownership and sequencing

The coordinator freezes request/response and manifest contracts first. Backend
and analysis implementation can parallelize only once shared fields agree. A
read-only evaluator can design fixture expectations concurrently. Treat
`stdf-cli/src/viewer/mod.rs`, `tests.rs`, shared translations, root manifests,
README and generated demo outputs as single-writer files. The reviewer does not
quietly edit the author's code to make it pass.

Use [task/handoff/review templates](../development/multi-agent.md), explicit
writable paths and a single shared build/test slot. For bounded optimization,
freeze the evaluator and acceptance rules using the [improvement workflow](../development/improvement.md).
Optimization scores cannot waive semantic or snapshot-integrity failures.

## Existing checks to reuse

The existing Windows CI includes workspace tests, including `stdf-py`; it does
not currently run Linux or browser suites. Local non-extension test commands
are not proof that the CI job or Python runtime integration passed.

```powershell
cargo fmt --all -- --check
cargo test --workspace --exclude stdf-py --locked
cargo check -p stdf-py --locked
python scripts/check_licenses.py
cargo build --release -p stdf-cli --locked
```

For viewer-affecting changes, generate the synthetic fixture and execute the
existing browser suites after the build, using documented Playwright/Edge prerequisites:

```powershell
python examples/viewer/generate_demo.py
node scripts/smoke_viewer.cjs
node scripts/smoke_viewer_axes.cjs
```

Add targeted snapshot/replay tests before implementation. Choose other report
suites by affected behavior. Add Linux CI as a separate verified port: provision
Python/PyO3 dependencies, replace Windows-only launch assumptions and select a
supported browser explicitly. A matrix declaration is not passing portability
or installer evidence. Do not alter current CI merely to hide extension failures.

## Follow-on packages

| Priority / issue | Scope | Gate |
| --- | --- | --- |
| P1 / ANA-002 | Latest yield, lot comparison, site variation, retest impact and Pareto | Every operation names population/order/reduction rules; matched Rust/report outcomes and explicit unsupported cases |
| P1 / API-001 | Read-only typed local tool adapter | Schema, authorization, cancellation, payload limits and denied operations tested; no LLM needed |
| Optional / SQL-001 | Embedded DuckDB query adapter | Pinned version, fixed operations, restricted object access, resource tests, semantic parity; no install required for ordinary users |
| Optional / AI-001 | Local or opted-in cloud explanation adapter | Authorized data scope, no arbitrary tool capability, provenance-grounded output, prompt-injection tests |
| P1 / UX-001 | Packaged local launcher and import flow | File picker, progress/cancel/retry, automatic local project creation and tested offline installation; reuse current UI |
| Later / HA-001 | Shared deployment adapters and operational design | Measured SLO/RPO/RTO, fenced concurrent publication, auth, backup/restore, fault/load/security evidence |

Do not call the local milestone "AI Analyst v0.1" before AI exists. A suitable
working milestone is "Reproducible local analytics slice"; release naming/version
is separate from implementation and does not change existing package versions.

## Evidence from the original architecture review

Reviewed baseline: `732870b` on 2026-10-31. Two independent read-only agents audited
storage and analytics against the pasted proposal. Source evidence is linked in
[overview.md](overview.md). That review revised documentation only. Subsequent
snapshot implementation is described below; no SQL adapter, runtime AI endpoint,
Linux CI or HA implementation is claimed.


### SNAP-001 implementation increment

[Catalog snapshots](../snapshots.md) implement immutable EAV descriptor publication
under the existing writer lock and historical descriptor/fragment verification.
The selection increment adds an atomic explicit current pointer and immutable
resolved metadata handles. The full SNAP-001 gate remains open: complete input
inventory and retention leases for long-lived analytics need further work.
DATA-001 remained a prerequisite for attempt-level yield and receipts; ANA-001 /
AUD-001 attempt-yield and `analysis-receipt-v1` publication have since shipped
(see [cli.md](../cli.md) and [CHANGELOG.md](../../CHANGELOG.md)). The original
source-audit statements above refer to baseline `732870b`.

Local Windows validation for the original capture increment (2026-10-31): 421 Rust workspace tests
passed, excluding Python extension linkage; the Python binding compile check,
format check, Apache-2.0 metadata check and release CLI build passed. The synthetic
demo captured 21 EAV rows, updated the same source to 35 rows, then verified the
original selection without the current catalog or original STDF. Independent
implementation review completed with no blocking findings after the native Windows
path and publication-failure coverage fixes.

The first restricted test run denied hard-link creation in both existing writers
and snapshot publication. The authorized rerun passed; this is not evidence that
unsupported filesystems can publish snapshots. Browser behavior is unchanged and
browser suites were not rerun. Unix-only symlink tests, Linux portability,
large-file resource measurements and power-loss durability remain unverified here.

The explicit selection increment passed 430 Rust workspace tests (Python
extension linkage excluded), the Python binding compile check, formatting,
license checks and release CLI build on the same Windows host. Its extended demo
kept S1 selected after capturing S2, changed selection only on request, and
reselected S1 without the live catalog or original STDF. Independent implementation
and documentation reviews completed with no blocking findings. Reader handles
pin metadata and generation choice; retention leases, attempt-complete companion
storage and snapshot-backed analytics remain future work.

### DATA-001 retained evidence increment

[Retained evidence](../retained-evidence.md) implements one-source immutable
attachments, bounded regeneration-based verification and CLI record/attempt
inspection. It reuses native viewer-v5 companions and exact source-derived
execution references, with explicit original/decompressed hashes. The original
catalog snapshot, current selection and existing reports remain unchanged.

The attached source's EAV values and duplicate multiplicity must match the frozen
catalog source. This is not an assertion that indistinguishable legacy catalog
rows have original execution provenance. Verification rejects incomplete or
incompatible native reconstruction. An explicit source ID prevents accidental
selection of the first same-hash catalog entry.

DATA-001 remains open for a shared public analytics adapter and composition of
multiple retained sources under explicit policies. ANA-001 yield and AUD-001
`analysis-receipt-v1` were not part of this evidence-storage increment; they
shipped later via `stdf-analytics::attempt_yield_op` and
`zstdf-cli snapshot attempt-yield`.

Local Windows validation (2026-11-01): all 444 Rust workspace tests passed,
excluding Python extension linkage. Formatting, Python binding compilation,
Apache-2.0 metadata checks and the release CLI build passed. The release demo
retained 48 records, six PRR attempts, 25 PTR/MPR/FTR executions and 35 measurement
rows. Verification and PRR-only attempt inspection passed after removing the
original plain/gzip inputs and temporarily removing the live catalog; the newer
snapshot remained selected. Independent review completed with no open findings.

The first full run exposed a parallel-test temporary-directory name collision in
an existing catalog fixture. A test-only atomic suffix fixed it; the 12 focused
catalog tests and the full workspace rerun passed. Browser suites were not rerun
because this increment changes no UI or existing viewer ingestion behavior.
Large-file performance, Linux execution, Windows reparse fixtures and cancellation
at an exact mid-copy boundary remain unverified; no such results are claimed.
