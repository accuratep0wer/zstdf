# Failure model and verification plan

Status: proposed gates for the local-first extension. Existing mechanisms are
identified in [overview.md](overview.md); the scenarios below are not a claim
that historical snapshots, AI tools or HA have already passed tests.

## Failure domains

Distinguish malformed binary input, configured validation findings, unknown
engineering values, process interruption, storage corruption, resource exhaustion
and unauthorized operations. A missing optional vendor record is not corruption.
A hash-valid file with unknown verdicts is not fully interpretable test evidence.

A local application can provide tested crash recovery. It cannot guarantee
availability after loss of its only machine/disk, administrator tampering, or
arbitrary network-filesystem behavior. Atomic rename is not by itself proof of
power-loss durability: validate flush/order/recovery behavior on each supported
OS and filesystem. Checksums need trusted metadata; they are not signatures.

## Verification matrix

| Scenario | Required behavior and test oracle |
| --- | --- |
| Update S1 while an analysis is reading it | Reader uses S1's exact object list; a later S2 cannot alter counts, pages or hashes within that request |
| Crash after fragment close, generation rename, manifest publish or pointer update | Published S1 remains resolvable; pointer resolves only a complete manifest; orphan complete objects are harmless; retry has deterministic outcome |
| A succeeds, B fails in an import | Current converter may retain A's new generation. Snapshot wrapper records B failure and any carried-forward generation; no implicit complete snapshot |
| No expected input inventory | Completeness is unknown; not inferred from the latest catalog run's status |
| Same-path retry, copied path, gzip/plain equal content | Outcomes match the declared source-inclusion policy; aliases never silently become extra devices or disappear |
| Viewer temporary cache is removed after snapshot publication | Retained snapshots still resolve their managed companion/raw objects; no long-term guarantee depends on temporary cache paths |
| Raw source changes/disappears after conversion | Verified analytics may remain available where complete companions suffice; raw replay operations fail explicitly unless retained bytes match |
| Source changes during conversion or verified read | Reject affected publication/result; do not bless results from bytes different from the receipt's hashes |
| Corrupt fragment/footer/schema/count/manifest | Classify integrity failure before returning a complete analytical result; keep unrelated committed snapshots intact |
| Concurrent writers and pinned readers | Serialize writer publication; readers bind once. No deletion of retained/pinned objects; initial implementation has GC disabled |
| Path traversal, symlink/junction escape, renamed dataset root | Deny object-store escape; valid relocation preserves snapshot identity without recomputing source IDs from display locators |
| PRR-only, interleaved sites, repeated PART_ID, repeated MPR/FTR | Exact physical attempt count and record provenance; channels do not multiply devices; repeated executions preserved |
| Unknown verdict, missing identity, tied/missing START_T | Unknown/excluded counts explicit; no fabricated pass, identity or latest winner |
| Display filter after Latest selection | No resurrection of an older failing attempt; device and measurement exclusions stay separate |
| Cross-lot matching wafer coordinates | Existing Dashboard and latest-device policies intentionally remain different; explicit expected outputs for each |
| Cancel, disk quota, metadata limit, oversized result | Explicit failure; no silent truncation or complete receipt; remove only owned staging and preserve prior published outputs |
| Crash between result and receipt writes | No visible completed bundle until both exist and verify; retry cannot overwrite a different result under the same immutable ID |
| Repeat request under pinned build | Same normalized inputs produce canonical result bytes/hash; execution IDs/timestamps may differ |
| Different platform/engine | Use separately specified parity/tolerance checks; do not call differing floating bytes exactly reproducible |

Tests should inject failures at publication boundaries, not merely make a parser
return an error. Recovery currently cleans owned stale staging and marks an
interrupted catalog run; it does not reconstruct missing data or roll back a batch.
New snapshot recovery must build on that ownership discipline.

## Resource accounting

Set limits for compressed bytes, decompressed bytes, record length, input count,
manifest bytes, fragments, active readers, query duration, output bytes, scratch
space, concurrent operations and export size. Application buffer accounting is
not a hard RSS ceiling. Measure process RSS and peak scratch alongside latency
before claiming large-file support. Source hashing, verification and receipts
must also fit the budgets; a small result does not imply a bounded computation.

For a verified read, prefer stable handles to owned immutable objects; account
for mutation between path verification and opening. Rehash or fail if stability
cannot be established. This provides integrity under the stated storage model,
not protection from a privileged user actively replacing data and trusted hashes.

## Runtime tool boundary

The proposed runtime adapter accepts operation IDs, dataset/snapshot IDs and
validated parameters. It does not accept filesystem paths, free-form filter
expressions, SQL text, shell commands, plugin names or destinations from a model.
The application resolves allowed local objects. Dataset storage is read-only to
that capability; the recorder has a separate bounded analysis-output capability.
A model cannot alter schemas, validation findings, policies or disposition.

Test denials for traversal/aliases, unauthorized snapshots, arbitrary operation
names, oversized arguments/results, cancellation, external URLs, extension
loading and unexpected file/network writes. Treat text in STDF metadata as data,
including strings that look like agent instructions. Cloud/model adapters are
opt-in with explicit selection of shareable fields; engineering identifiers and
raw records must not be included by default. Keep local receipts free of secrets.

Optional SQL adapters require version-specific restrictions and OS boundaries
for untrusted execution. Database read-only mode is not a filesystem/network
sandbox; [DuckDB documents these distinctions](https://duckdb.org/docs/2.0/operations_manual/securing_duckdb/overview).
Do not claim the current loopback viewer or the development improvement runner
already enforces this proposed runtime-tool security model.

## What would establish enterprise readiness

After local contracts pass, specify SLO, RPO/RTO, dataset size and concurrency.
Then test transactional publication/CAS and fencing under worker failure,
replicated object durability, snapshot-aware retention, backup restore, tenant
isolation, audit protection and authorization on every request. A successful
PostgreSQL/S3 adapter or several replicas alone does not establish these properties.

## Review and evidence

Independent review is required for new snapshot, identity, policy and schema
semantics. Use synthetic fixtures with hand-computed expectations and record
commands, revisions, hashes, exit codes and platform. Preserve separate statuses
for local tests, CI, browser checks, fault injection and semantic review. Missing
evidence stays pending. The architecture review itself does not execute these gates.
