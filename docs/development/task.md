# Task contract template

Copy into `target/agent-work/<feature>/task.md`. Replace every placeholder before
dispatch. Coordinator owns this file; do not treat this template as an active task.

- Task ID / status:
- User objective and acceptance criteria:
- Repository absolute path / baseline commit / existing uncommitted changes:
- Scope and explicit non-goals:
- Contract: interfaces, examples, null semantics, ordering and compatibility:
- Domain references:
- Resource limits and stop/resume conditions:

| Worker / role | Exact writable files | Read-only references | Depends on | Deliverable / checks |
| --- | --- | --- | --- | --- |
| Coordinator | Task state and integration paths | Entire repository | User objective | Integrated diff and evidence |
| Worker A | Fill before dispatch | Fill | Contract agreed | Fill |
| Worker B | Disjoint from A | Fill | Contract agreed | Fill |
| Reviewer | None during review | Diff, fixtures, logs | Integrated change | Findings with severity and evidence |

## Worker dispatch prompt

Implement the assigned objective within the listed writable files. Read the root
AGENTS.md and docs/development/multi-agent.md. Preserve unrelated work. Do not
spawn workers, stage, commit, push or edit shared interfaces outside this task.
Ask the coordinator for ownership transfer if an additional edit is necessary.
Request the shared-tool slot before Cargo, global formatting, demo generation or
browser suites. Report blocked dependencies early. Return the handoff template
with actual checks and limitations; do not claim unperformed verification.

## Coordinator ledger

- Active file owners and shared-tool owner:
- Agent identifiers and status:
- Interface decisions and dependency changes:
- Verification evidence:
- Review findings and dispositions:
- Final revision reviewed and delivered:
