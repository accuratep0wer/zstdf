# Changelog

All notable user-facing and schema/API changes for zstdf are recorded here.
The package version remains `0.1.0` while public contracts are still evolving
under explicit schema identifiers.

## 0.x — schema and API contracts

### `eav-v2`

Long-format measurement Parquet and catalog fragments use Arrow schema
`eav-v2` with dashboard identity rule `coordinate-v1`. New conversions append
`part_sequence` and nullable `part_merge_key`. Legacy `eav-v1` outputs are not
read by current dashboards; reconvert source STDF into a new Parquet or catalog
dataset before use. See [coordinate identity](docs/coordinate_identity.md).

### `viewer-v5`

The single-file engineering viewer (`view`) publishes native cache manifests as
`viewer-v5`, with attempt/execution companions, record evidence, and bounded
loopback export. Viewer identity and population policies remain separate from
Dashboard `coordinate-v1` merging. Optional
[`viewer-evidence-v1`](docs/retained-evidence.md) attachments reuse the same
native companions under a frozen catalog snapshot.

### `attempt-yield-v1`

Exact PRR yield over retained evidence uses:

- request `attempt-yield-request-v1`
- result `attempt-yield-result-v1`
- operation `prr-attempt-yield-v1` for population `all_attempts`
- operation `prr-latest-device-yield-v1` for population `latest_device`
  (requires `identity_policy: latest-ecid-start-t-v1`)

`all_attempts` counts every scoped physical PRR under `catalog-source-id-v1`.
`latest_device` uses retained latest flags (ECID + MIR START_T policy) and is
not Dashboard `coordinate-v1` merging. Request/result/scope/hash helpers live in
`stdf-analytics::attempt_yield_op`; the CLI publishes receipts and reads
evidence. Public entry: `zstdf-cli snapshot attempt-yield <dataset>
<request.json>` (optional `--telemetry-json`). Focused verification:

```powershell
cargo test -p stdf-cli attempt_yield --locked
```

### Receipts

Catalog conversion publishes per-source `_SUCCESS.json` fragment receipts used
for retry and verification. Shipped `analysis-receipt-v1` bundles publish under
`dataset/analysis/<execution_id>/` as paired `result.json` and `receipt.json`
(content-addressed execution id; create-if-absent / verify-on-replay). See
[data contracts](docs/architecture/data-contracts.md).

### Run telemetry

`convert`, `view`, and `snapshot attempt-yield` accept optional
`--telemetry-json <path>` to append `run-telemetry-v1` JSONL events. Omitting
the flag preserves prior stdout results; stderr phase lines may still appear
when the helper runs. This is local diagnostics, not OpenTelemetry export.
