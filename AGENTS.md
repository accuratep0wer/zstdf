# zstdf development agents

This repository uses a coordinator-led development workflow. Read
[the collaboration guide](docs/development/multi-agent.md) before delegated work.
These are repository instructions, not an agent launcher or permission system.
User instructions and host permissions take precedence.

## Start and assignment

- Inspect the current checkout, Git status and root Cargo workspace first.
  Do not switch to an adjacent checkout or assume `crates/` is the active workspace.
- The user has authorized the multi-agent framework as the default for future
  zstdf development tasks. Delegate independent implementation, investigation and
  review work when supported by the host, without asking again for delegation
  permission. Later task-specific user instructions override this default.
- Small or strongly dependent changes may use one agent following the same
  workflow. If delegation is unavailable, perform the roles sequentially and
  disclose any independent-review gap. This default does not authorize unrelated
  background work, external messages, commits or publication.
- The coordinator records a task contract before spawning workers. Each worker
  receives an objective, exact writable paths, read-only references, dependencies,
  acceptance criteria and a verification command. Use the templates in
  `docs/development/`.
- One writer per file at a time, including tests and generated files. Read access
  is shared. A worker needing another path requests a coordinator handoff; it
  does not edit outside its assignment or spawn more agents on its own.
- Default to at most two implementation workers plus one read-only reviewer,
  subject to the host's actual limit. The coordinator is also an active agent.
- Workers report changed files, interface changes, exact checks/results, failures
  and unresolved risks. They do not stage, commit, push or revert another worker.
  Only the coordinator integrates and publishes, within the user's authorization.

## Preserve product contracts

- Preserve distinct identity/population policies. Read `docs/coordinate_identity.md`
  for Dashboard merging and `docs/latest-pareto.md` for latest-device ECID policy;
  read the viewer and traceability documents for their respective behavior.
  Do not silently unify these policies.
- Preserve every PRR attempt, per-head/site association and source provenance.
  Never infer identity from PART_ID alone or mix PRR and PTR coordinate axes.
- Latest selection uses MIR START_T and the documented within-run order. Display
  filters must not resurrect old attempts; measurement exclusions must not change
  recorded PRR verdicts. Keep unknown, missing and invalid distinct.
- Schema/cache changes require explicit versioning, compatibility behavior and
  regression evidence. Preserve bounded processing, quotas and atomic outputs.
- Keep five themes, five languages, raw-null tooltips and UTC dates. Browser
  behavior needs browser evidence; a Rust build alone is insufficient.
- Keep Markdown and reusable agent prompts in English. Preserve Apache-2.0
  declarations. Do not publish local proprietary references or downloaded STDF
  samples; use synthetic fixtures and documented download scripts.

## Verification and shared resources

- Choose checks from the collaboration guide. Record skipped/unavailable checks
  explicitly. Never treat a planned check or another agent's assertion as a pass.
- In a shared checkout, the coordinator serializes Cargo builds, demo generation,
  browser suites and release executable replacement. Workers request that slot.
- Use separate output directories for independent experiments. Do not kill an
  unknown process, overwrite another task's output, or run broad Git cleanup.
- Review the actual diff after all workers finish. The author cannot be the sole
  reviewer of a semantic identity, schema, population or query-contract change.
  If independent agent or human review is unavailable, deliver the implementation
  with review explicitly pending; do not label self-review as independent or mark
  the review gate verified.


## Bounded improvement tasks

- For iterative improvement work, follow `docs/development/improvement.md` and
  freeze a task-specific experiment contract before evaluating candidates.
- Implementation workers do not edit the active evaluator, protected fixtures,
  thresholds, budgets or these instructions to obtain acceptance. Propose workflow
  changes as a separate reviewed experiment.
- The coordinator owns experiment state and evaluation execution. A runner's
  accepted score is not independent review, a Git merge or publication permission.
- Stop at the recorded round, no-gain or resource limit. Preserve the last accepted
  candidate and evidence; never reset shared user work to discard a failed trial.
