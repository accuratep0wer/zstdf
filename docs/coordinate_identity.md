# Coordinate-based part identity

New conversions use schema `eav-v2` and dashboard identity rule `coordinate-v1`.
The rule applies to both `dashboard` and `dashboard-dir`.

## Resolution

1. Use `(wafer_id, PRR.x_coord, PRR.y_coord)` if wafer is nonempty and contains
   only ASCII `A-Z` and `0-9`, and both PRR coordinates are strictly positive.
   Lowercase, whitespace, punctuation, Unicode/replacement characters, missing
   coordinates, zero and negative coordinates trigger fallback. IDs are not
   trimmed, repaired or case-folded.
2. If any primary component is invalid, replace the entire tuple with
   `(lot_id, PTR_X, PTR_Y)`. Both coordinates must come from PTR results for
   that same test attempt. Do not mix PRR and PTR coordinates. Lot must be nonempty.
3. If fallback is missing, invalid or ambiguous, leave the merge key null and
   isolate the attempt by source identity and `part_sequence`. A repeated or
   empty PART_ID never causes unresolved attempts to merge.

Wafer/PRR and lot/PTR keys have distinct namespaces. Within each namespace,
resolved identities merge across source files, PART_IDs, heads and sites.
The primary key deliberately does not include lot: identical wafer IDs and PRR
coordinates merge even across lots. PTR fallback includes lot as requested.

## PTR coordinate recognition

- Match test names case-insensitively. Prefer standalone X/Y tokens (including
  camelCase) and coordinate words, such as `coordinate_x`, `CoordinateY`,
  `xCoord`, `coordinatey`, `DieX`, `Y_INDEX`, and `waferx`.
- If no explicit candidate exists for an axis, use names containing only that
  axis letter (`x` or `y`), subject to the same value and ambiguity checks.
- Names that designate both axes without a unique axis are not selected.
- Values must be finite, strictly positive integers representable as signed
  64-bit integers. They are taken from the decoded PTR numeric result without
  rounding or truncation; zero, fractions, NaN and infinity are rejected.
- Multiple candidates at the highest matching priority must agree. Conflicting
  values, or invalid values at that priority, make that axis unresolved.
  Repeated identical coordinates are allowed. The outcome does not depend on
  candidate order. More explicit names take precedence over weak substring
  matches such as the `x` in `Vmax`.

Name recognition is heuristic because no fixed coordinate test names were
provided. Check `unresolved coordinate identity` in the Data Quality report and
inspect source PTR names/results when fallback cannot be resolved.

## Stored data and analysis semantics

`eav-v2` appends two columns to the existing 19 columns:

| Column | Type | Meaning |
| --- | --- | --- |
| `part_sequence` | UInt64, non-null | Source-local test attempt sequence, independent of PART_ID |
| `part_merge_key` | Utf8, nullable | JSON tuple `["wafer-prr", wafer, x, y]` or `["lot-ptr", lot, x, y]` |

Resolution happens after the attempt closes at PRR and all its PTR results are
available. Every measurement row carries the completed identity, even when X/Y
tests occur later or are stored in other Parquet fragments. Original wafer,
lot, PRR coordinates, PART_ID and measurement values are retained unchanged.

The key changes part counting and the grouping of observations used for test
correlations. Part pass still means every merged observation passed; metadata
comes from the first observation, and correlation retains the first finite
result per test number. This does not implement first/final-retest yield, and
does not remove measurement rows from parameter statistics. Raw-coordinate
spatial charts retain their existing semantics; fallback does not rewrite PRR.
The dashboard JSON includes `identity_rule_version`.

## Existing datasets

Do not combine eav-v1 and eav-v2 fragments. The new dashboard/catalog reader
rejects old schemas with a reconversion message, leaving existing artifacts
untouched. Old files do not contain attempt boundaries or completed identities,
so silently guessing those from repeated PART_IDs is not supported.

Reconvert from the original STDF to a new destination:

```powershell
cargo run -p stdf-cli -- convert input.stdf output-v2.parquet
cargo run -p stdf-cli -- dashboard output-v2.parquet dashboard-v2.html

cargo run -p stdf-cli -- convert --layout catalog --output-dir dataset-v2 .\inputs
cargo run -p stdf-cli -- dashboard-dir dataset-v2 dashboard-v2.html
```

Schema/identity changes are included in fragment generation fingerprints.
`convert --no-overwrite` refuses an old-schema manifest instead of reporting
that an incompatible result was successfully reused.

## Verification

Regression coverage includes invalid wafer characters, nonpositive PRR values,
missing/nonfinite/fractional PTR values, name variants and conflicting candidates,
PRR precedence, repeated PART_IDs, interleaved sites, and cross-source/fragment
merging. Both regular and bounded conversion paths are checked. Empty legacy
Parquet, legacy catalogs and old manifests must fail with a reconversion message
without modifying existing data or reports.

```powershell
cargo fmt --all -- --check
cargo test --workspace --exclude stdf-py --locked --offline
cargo check -p stdf-py --locked --offline
cargo build --release -p stdf-cli --locked --offline
```
