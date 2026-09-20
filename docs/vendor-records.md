# Selected STDF and Advantest records

zstdf extracts ATR, CDR, ATER, CTSR and CTRR. These optional records are hidden
when absent and have no enabled field checks in the default CSV. When present,
they are displayed as `not_checked` and excluded from text-summary field totals.
Uncomment selected rows in [config/sanity-checks.csv](../config/sanity-checks.csv)
and pass `--checks-csv` to enable required-field/value checks. See
[CSV policy](sanity.md#csv-field-checklist) for type notation and conditional fields.
Record framing, truncated payloads and undecodable structures still produce
diagnostics, even when all semantic checks are disabled.

| Record | Wire identifier | Support |
| --- | --- | --- |
| ATR | 0 / 20 | Existing audit timestamp and command text decoder; extracted; checks disabled by default |
| CDR | 1 / 94 | V4-2007 chain index/name, length, pins, clocks, inversion, cell names and continuation flag |
| ATER | 137 / 10 | V93000 activity source, head/site and activity; original activity payload bytes retained |
| CTSR | GDR 50 / 10 | `REC_CUSTM` exactly `SHMOO` or `MARGIN`; summary, axes and tracking parameters |
| CTRR | GDR 50 / 10 | `REC_CUSTM` exactly `SHMOO_RESULT` or `MARGIN_RESULT`; cell result, coordinates and references |

## Run and inspect

From the repository root, in PowerShell:

```powershell
cargo build --release -p stdf-cli --locked
python -B examples/sanity/generate_vendor_demo.py
.\target\release\zstdf-cli.exe sanity examples\sanity\generated\vendor.stdf `
  --test-domain cp --output-dir examples\sanity\generated\vendor-report
Start-Process examples\sanity\generated\vendor-report\report.html

# Full record fields, including characterization values and their GDR tags:
.\target\release\zstdf-cli.exe to-ascii examples\sanity\generated\vendor.stdf `
  examples\sanity\generated\vendor.txt

# Invalid/missing summary for the other records; exempt fields are excluded:
.\target\release\zstdf-cli.exe sanity examples\sanity\generated\vendor.stdf `
  --test-domain cp --text-summary examples\sanity\generated\vendor.sanity.txt
```

Use your own `.stdf`, `.std`, gzip file, or directory in place of the synthetic
input. On Linux/macOS, use `target/release/zstdf-cli`. No extension flag is needed.
SmarTest must have characterization logging enabled to produce CTSR/CTRR; the
reader cannot reconstruct records that the tester did not write.

## Representation and association

CTSR/CTRR remain `StdfRecord::Gdr` at the core API's wire layer. Call
`Gdr::characterization()` for named fields and their original `GEN_DATA` indexes.
The original count, type tags, padding, strings and floating-point values remain
available. Named axis paths use zero-based indexes, for example
`AXES[0].TRACKING[0].TRACK_RNG_VAL`. R*8 values retain double precision; the
sanity evidence includes the original float bits. No unit conversion or parsing
of free-form range/result strings is applied.

ATER uses the supplied direct field layout: `Cn, Cn, U1, U1, Cn`. It does not
assume a GDR count/tag envelope for the vendor `(137,10)` record. Its decoded
`activity_bytes` and the `ACTIVITY_BYTES` evidence field preserve binary payloads,
even when text display needs replacement characters. Original source bytes are
also archived for all records.

ATER and CTRR use explicit head/site to select the active PIR/PRR test instance.
Repeated PART_ID values and reuse of a site do not merge text or cell results
between attempts. CTRR's U*4 site number is retained in full. A value outside the
ordinary U*1 PIR/PRR range stays unassigned; it is never truncated to another site.

CTSR is displayed as run metadata because it has no head/site fields. A CTRR
links to a preceding CTSR with the matching characterization ID while the same
unit is active. Decimal and `0x` hexadecimal text IDs are supported. Repeated
matching setups are marked ambiguous; a setup from a previous attempt is not
reused for a later retest. Missing setup/ownership links are displayed as
unresolved without association findings. CSV selection enables field checks,
not setup-link or ownership validation.

CDR continuation fragments are preserved as separate records. This release does
not assemble chains, validate PMR references, reconstruct shmoo plots, or change
PRR pass/fail, yield, coordinate identity or traceability calculations. The
measurement Parquet interface remains `eav-v2`; these records are available in
the source-sanity field evidence, HTML/JSON and text dumps, not as PTR rows.
HTML/JSON keeps the configured per-unit previews (two records per type by
default); the field Parquet, source archive and ASCII dump retain all records.

## Compatibility and verification

CTSR/CTRR and ATER layouts follow the SmarTest 8.8.2 documentation supplied with
this change. The CTSR table and shmoo example differ: the example omits margin
values and tracking initial values. The decoder accepts optional R*8 margin
values when their tags are present, and the two documented tracking string
forms (with or without initial value). It does not guess unknown layouts or
silently discard leftover values. The CDR field layout and `(1,94)` mapping were
cross-checked against the public
[rust-stdf record definitions](https://docs.rs/rust-stdf/0.3.1/src/rust_stdf/stdf_types.rs.html).

Tests cover both byte orders, truncated records, CDR S*n cell names longer than
255 bytes, shmoo/margin branches, both tracking forms, R*8 precision, GDR padding,
binary activities, multiple sites, repeated attempts, large CTRR site numbers,
and sanity-exclusion behavior. The demo is synthetic, not validation against
an actual V93000 capture. Existing reports must be regenerated to show the new
fields and exclusion status; no `eav-v2` migration is required.

## Enable selected optional field checks

The [vendor checklist example](../examples/sanity/vendor-checks.csv) checks
MRR.FINISH_T plus ATR.MOD_TIM, CDR.CHN_NAM, CTSR.CHAR_NAM and each axis's
RNG_RESO. It intentionally selects only these fields; use the full default CSV
as the starting point to retain the other core checks.

```powershell
.\target\release\zstdf-cli.exe sanity examples\sanity\generated\vendor.stdf `
  --test-domain cp --checks-csv examples\sanity\vendor-checks.csv `
  --output-dir examples\sanity\generated\vendor-checked-report
```

The same command succeeds without optional records: enabling their fields does
not require those record types to exist. When present, selected cells show their
validation status; remaining cells keep the neutral Not checked style.

Browser regression after generating the checked example (Playwright must be on
Node's module path and Microsoft Edge installed):

```powershell
node scripts/smoke_sanity_csv.cjs
```
