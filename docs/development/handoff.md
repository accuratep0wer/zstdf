# Worker handoff template

- Task ID / role / status:
- Objective achieved, or concrete blocker:
- Files changed and why:
- Interface/schema/cache changes:
- Domain invariants checked:
- Commands executed, exit codes and evidence paths:
- Checks not run and why:
- Processes started and stopped; remaining owned processes:
- Known limitations and follow-up dependencies:
- Suggested reviewer focus:

In a shared checkout, list paths and summarize the diff; workers do not create
commits. In an isolated worktree, include the baseline revision and exact patch
requested for integration. The coordinator owns commit and publish operations.
