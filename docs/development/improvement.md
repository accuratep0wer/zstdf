# Bounded recursive improvement

## What executes

The multi-agent coordinator proposes and implements candidate changes. The local
`scripts/improvement_runner.py` evaluates those candidates against a frozen
contract and records acceptance, rejection and stopping decisions. It does not
call a model, edit product code, schedule future runs, merge changes or push Git.

This first implementation improves the development loop, not model weights.
Run it only within an assigned task. A successful evaluation is evidence for
integration, not permission to publish. Changes to prompts and role definitions
are a separate experiment; do not rewrite active instructions to make a failed
product candidate pass.

## Roles and iteration

Keep one coordinator, one implementation worker, one evaluator and one reviewer
within the host's available slots. The evaluator may design tests before the
experiment starts, then freezes them. During evaluation the harness and expected
results are read-only to the implementation worker. One writer owns each file;
the coordinator serializes builds and evaluations.

1. Select one measurable improvement and define its correctness gates.
2. Prepare baseline and candidate checkouts or directories. Include permitted
   uncommitted changes in the baseline; a Git commit ID alone is insufficient.
3. Freeze the contract, source hashes, protected evaluation files and runner.
4. The implementation worker produces one scoped candidate.
5. The coordinator runs evaluation and the independent reviewer checks evidence.
6. Keep an accepted candidate as an artifact; reject other candidates without
   changing the working tree. Record the reason and propose the next hypothesis.
7. Continue only while the recorded budget and stopping policy allow it.

The runner's acceptance decision does not replace independent semantic review.
The coordinator must still check the final diff against the task contract before
integration. A worktree provides editing isolation, not a security sandbox.

## Run the synthetic demonstration

From the repository root, with Python 3.11+:

```powershell
python examples/improvement/run_demo.py
python scripts/improvement_runner.py status --state target/improvement-demo/experiment
```

The demo creates a new output directory and refuses to overwrite one. For another
run, pass `--output-dir target/improvement-demo-2`. It demonstrates a correct
candidate with lower simulated cost, a worse candidate, and an incorrect
candidate whose attractive score cannot override a failed correctness gate.
These numbers are synthetic and do not establish zstdf performance improvement.

## Create an experiment

Start from [the example contract](../../examples/improvement/contract.json).
Commands are token arrays, never shell strings. `{python}` resolves to the
current interpreter. Commands execute with the corresponding checkout as their
working directory. Do not put secrets in arguments or emitted logs.

```powershell
python scripts/improvement_runner.py init --contract experiment.json --baseline C:\work\baseline --state C:\work\evidence\experiment-01
python scripts/improvement_runner.py evaluate --state C:\work\evidence\experiment-01 --candidate C:\work\candidate-01
python scripts/improvement_runner.py status --state C:\work\evidence\experiment-01
```

Initialization fingerprints the configured source scope without running commands.
The state directory must be new and outside source scopes. Evaluation verifies
the frozen baseline and candidate changes, runs gates on both roots, and then
alternates paired baseline/candidate measurements. Both sides use the same
command and metric definition. Source hashes are checked again after execution.

| Field | Meaning |
| --- | --- |
| `schema_version` | Contract schema, currently `1` |
| `objective` | One concrete goal |
| `source_paths` | Relative files/directories forming the complete experimental scope |
| `allowed_changes` | Relative file/directory prefixes a candidate may change |
| `protected_paths` | Evaluation/fixture paths inside the source scope that must not change |
| `gates` | Named commands; every exit code must be zero |
| `benchmark.argv` | Command emitting a JSON object on stdout |
| `benchmark.metric` | Top-level JSON field containing a finite positive number |
| `benchmark.direction` | `lower` or `higher` |
| `benchmark.min_relative_gain` | Minimum relative improvement over the baseline median |
| `benchmark.repeats` | At least three paired measurements |
| `limits.max_rounds` | Maximum evaluations; start with three |
| `limits.max_no_gain` | Consecutive unsuccessful rounds before stopping; start with two |
| `limits.wall_seconds` | Cumulative active evaluation budget; idle time between commands is excluded |
| `limits.command_seconds` | Timeout for each gate/measurement |
| `limits.output_bytes` | Output cap per command |

The original baseline remains fixed across rounds. Once a candidate is accepted,
a later candidate must also improve on the incumbent metric; it cannot replace
the accepted candidate merely by beating the original baseline. Baseline drift,
protected source changes, invalid evidence or interrupted evaluation cannot
produce a successful result. The last accepted candidate is recorded by location
and fingerprint; the runner does not retain a full copy of its source tree.
Keep that checkout available and recheck its fingerprint before integration.

## Integrity and recovery limits

The contract, state and evidence have integrity checks. These catch accidental
editing/corruption; they are not signatures and do not defend against a malicious
actor who can rewrite both data and checksums. The filesystem lock serializes
runner writers. Interruption is recorded as incomplete/stopped rather than
silently retried as a successful evaluation. Do not repair state manually to
continue; preserve evidence and start a new reviewed experiment when necessary.

Commands are trusted code with the caller's permissions. They can access files
outside their working directory. Use OS sandboxing for untrusted candidates;
neither `protected_paths` nor separate worktrees provide that enforcement.
Hash checks detect covered mutations after the fact and do not undo them.
The runner never resets Git, deletes a candidate or replaces reports/caches.
Use foreground commands that wait for their children; detached/background processes
are unsupported. Forced termination of the coordinator can leave a child process
alive, especially on Windows. Inspect owned processes before resuming. Process
cleanup and final source hashing can extend beyond a command timeout; this is not
an OS-enforced wall-time ceiling. Final hashing is charged before acceptance.

Only configured source paths are fingerprinted. Include all code, lockfiles,
fixtures and local evaluator scripts relevant to the experiment. Pin external
dependencies and record compiler/browser/tool versions separately. Do not claim
that the runner's Python/platform signature captures the entire toolchain.
Use fixture-generated data and keep proprietary references outside public artifacts.

If the evaluator itself is wrong, stop the experiment, independently review and
version its correction, then rerun baseline and candidate under the same new
contract. Never reduce thresholds after observing a candidate fail.

## Applying this to the viewer

Use the existing Rust and browser gates described in [the collaboration guide](multi-agent.md).
Freeze identity, Latest selection, source provenance, quota and atomic-output
fixtures before optimizing. CI/browser checks that were not run remain pending.

`scripts/measure_viewer.py` is useful evidence, but each query currently executes
once and includes Node startup overhead. Its disk measurement is a final snapshot,
not peak scratch use. A real performance experiment needs a dedicated adapter
that emits the selected metric as JSON, controls cold/warm cache behavior, and
measures latency at the intended boundary. Choose improvement thresholds above
observed baseline noise. Repeated medians alone are not statistical proof.

Separate correctness from speed: lost attempts, incorrect identities, wrong
statistics or broken offline behavior are failures regardless of score. Evaluate
workflow changes on representative tasks using rework and escaped defects, not
lines changed, test count or agent self-ratings.

## Verification

```powershell
python -m unittest discover -s scripts -p test_improvement_runner.py
```

Tests use temporary synthetic scopes and subprocess commands. They establish
runner behavior, not product performance or hostile-process containment.
