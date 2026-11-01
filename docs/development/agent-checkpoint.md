# Agent checkpoint and publication

The stop hook `.cursor/hooks/auto-commit-push.ps1` runs when an agent session
ends. It does **not** publish. `AGENTS.md` allows only the coordinator to
integrate and publish, and only with user authorization. An unconditional
`git add -A`, commit, and `git push` contradicted that rule: it could publish
`main`, and it could sweep untracked local files into history.

## What the stop hook does

1. If `git status --porcelain` is empty, it exits without changing Git state.
2. Otherwise it writes a local checkpoint under
   `target/agent-work/checkpoints/<timestamp>/`:
   - `tracked.diff` — `git diff HEAD` (tracked changes only)
   - `staged.diff` — `git diff --cached`
   - `untracked.txt` — names of untracked, non-ignored paths, not their bytes
   - `status.txt` — porcelain status
3. It does not run `git add`, `git commit`, or `git push`.
4. It never updates `main` or `master`.
5. `ZSTDF_AGENT_PUBLISH` is ignored. The hook cannot honestly gate on
   `cargo fmt`, Clippy, and the workspace tests: its timeout is short and the
   script is fail-open so a failed check must not become a push.
6. Stdout is `{}` so the agent is not blocked. Warnings go to stderr,
   including a pull-request size warning when the local diff exceeds the
   limit in `scripts/check_pr_size.py`.

Restore tracked edits with `git apply target/agent-work/checkpoints/<timestamp>/tracked.diff`
from a clean worktree. Untracked files stay where they are; the checkpoint
lists their names only, so a downloaded STDF sample is not copied into the
checkpoint.

## Explicit coordinator publish

`scripts/publish_agent_branch.ps1` is a manual command, not a hook.

- The destination must match `agent/<name>` (lowercase). `main`, `master`,
  and any other destination are refused.
- It runs `cargo fmt --all -- --check`, `cargo clippy --workspace --locked --all-targets -- -D warnings`,
  and `cargo test --workspace --locked` before any commit or push.
- A failed check does not commit or push.
- It stages tracked updates and untracked non-ignored files, skipping STDF
  sample names and `examples/public-stdf/downloads/`. It does not use
  `git add -A` as a blind sweep of ignored paths (Git already excludes those;
  the extra denylist covers sample filenames that are not ignored).
- The push refspec is `HEAD:refs/heads/agent/...`. It is never a force-push
  and never `refs/heads/main`.

## Pull request size

`scripts/check_pr_size.py` warns when a diff changes more than 800
non-generated lines and is not entirely deletions or entirely generated
files (`Cargo.lock` and paths under a `generated/` directory). The warning
does not fail CI. Split the change when it fires.
