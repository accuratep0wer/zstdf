# Shared report display settings

Generated HTML reports share a top-right **Page style** and **Language** selector:

- Measurement Dashboard, including all six existing tabs and dataset dashboards.
- FTR pattern analysis below the existing Failure Pareto chart and test table.
- Sanity reports, including CP, FT, mixed-run, and vendor-record reports.
- Traceability reports, including the device matrix, anomalies, and attempt details.
- The optional standalone FTR export.

The styles are **Excel classic**, **MES industrial blue**, **Night shift dark**,
**Quality review**, and **High-contrast monitor**. All retain green/red/amber/gray
status meanings with visible text or symbols. Canvas labels redraw for the theme.

Languages are **Chinese**, **Japanese**, **Korean**, **English**, and **German**.
The selectors translate report controls, headings, legends, generated status
labels, summaries, and drill-down interface text. Source identifiers, pattern
names, test names, field names, measurements, raw diagnostic messages, embedded
configuration, and exported evidence are intentionally preserved verbatim.
An identifier that happens to equal a translated word is still source data.

Filter values, table sorting, current page, selected unit, and selected-device
details survive style/language changes. Raw JSON payloads are not modified.
Preferences are stored in browser local storage when available. File URL storage
scope is browser-dependent; URL parameters provide explicit, portable selection:

```text
dashboard.html?theme=quality&lang=zh&tab=pareto
report.html?theme=dark&lang=ja
trace.html?theme=excel&lang=de
```

Valid theme IDs: `excel`, `mes`, `dark`, `quality`, `contrast`.
Language IDs: `zh`, `ja`, `ko`, `en`, `de`.
Unknown values fall back to saved settings or the defaults, **MES / English**.
`tab` applies to the measurement Dashboard and accepts `overview`, `pareto`,
`commonality`, `correlation`, `spatial`, or `quality`.

## Regenerate reports

The controls, styles, and translation dictionary are embedded in generated HTML;
there are no network requests or external assets. Previously generated reports
need regeneration with the updated CLI.

```powershell
cargo build --release -p stdf-cli --locked

# FTR is part of the existing Pareto page; raw STDF is needed for VECT_NAM.
.\target\release\zstdf-cli.exe dashboard measurements.parquet dashboard.html --ftr-input C:\data
.\target\release\zstdf-cli.exe dashboard-dir dataset dashboard.html --ftr-input C:\data

.\target\release\zstdf-cli.exe sanity C:\data\cp.stdf --test-domain cp --output-dir sanity-report
.\target\release\zstdf-cli.exe traceability C:\data --flow-config flow.json --output trace.html
```

FTR inputs still have their own labeled filters within the Pareto page because
existing measurement Parquet does not carry FTR pattern names. Device-yield
semantics and report evidence schemas are unchanged. Theme/language support does
not promote the separate compact Traceability design preview or its experimental
column-filter layout into the production template.

## Verification

`scripts/smoke_report_ui.cjs` checks five report routes against all five languages
and styles, existing Pareto placement, retained filters/selections, unchanged
evidence, responsive layout, offline operation, and browser preference persistence.
Generate all required fixtures and reports with one command. The generator uses
only synthetic inputs, prepares a PTR-only dataset for the bounded converter,
and rebuilds the Dashboard, CP/FT/mixed/vendor Sanity, and Traceability demos.
It replaces named generated outputs and preserves unrelated files. It runs from
any working directory; `--cli` can select a different built executable.

```powershell
cargo build --release -p stdf-cli --locked
python scripts/generate_report_demos.py
cargo test --workspace --exclude stdf-py --locked
# With Playwright installed and Microsoft Edge available:
node scripts/smoke_report_ui.cjs
node scripts/smoke_ftr_patterns.cjs
```
