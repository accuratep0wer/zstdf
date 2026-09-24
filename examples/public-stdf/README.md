# Public STDF samples for local experiments

Three distinct public files have been downloaded and checked with zstdf. Their original bytes, source commits, URLs, and SHA-256 hashes are recorded in [sources.json](sources.json). Downloads and generated reports are ignored by Git.

## Samples and verified coverage

| File under `downloads/` | Size | PRR attempts | PTR records | FTR records | Suggested experiment |
| --- | ---: | ---: | ---: | ---: | --- |
| `pystdf/lot2.stdf` | 4.21 MiB | 1,569 | 52,403 | 0 | Parametric distributions, limits, test/bin comparison |
| `pystdf/lot3.stdf` | 4.35 MiB | 1,619 | 54,123 | 0 | A second lot/wafer sample with the same test program |
| `stdfreader/a595.stdf` | 2.00 MiB | 336 | 31,605 | 4,649 | FTR pattern Pareto, three wafers, DTR/GDR inspection |
| `rust-stdf/lot2.stdf.gz` | 0.20 MiB | Same as lot2 | Same as lot2 | 0 | Gzip input; decompressed bytes verified identical to lot2 |

All are big-endian files. `info` consumed every byte with `truncated=false`; `view` built the three distinct caches and exported summary reports successfully. This verifies local decoder/viewer compatibility, not the correctness of the original tester measurements or conformance to every CP/FT sanity rule. PRR counts are test attempts, not deduplicated device counts. None of these files exercises MPR or CTSR/CTRR.

**Use Population = All attempts for the initial exploration.** Under the viewer's established positive-coordinate identity rule, lot2 has 1,569 unresolved latest identities and lot3 has 1,619. Their default Latest plots are therefore empty. The a595 sample has 270 eligible Latest attempts and 66 unresolved attempts. Original coordinates and identities have not been changed to force these samples into the current policy. Devices/Records remain available for inspection.

## Run locally

From the repository root:

```powershell
# Re-download pinned bytes if needed; verified existing files are reused.
python examples/public-stdf/download.py

# Recommended first sample: includes FTR patterns and multiple wafers.
.\target\release\zstdf-cli.exe view examples/public-stdf/downloads/stdfreader/a595.stdf `
  --cache-dir target/public-stdf-cache --memory-limit-mib 1024 --export-size-mib 100

# In the browser, choose Population = All attempts.
.\target\release\zstdf-cli.exe view examples/public-stdf/downloads/pystdf/lot2.stdf `
  --cache-dir target/public-stdf-cache --memory-limit-mib 1024 --export-size-mib 100

# Compressed input uses the same viewer entry point.
.\target\release\zstdf-cli.exe view examples/public-stdf/downloads/rust-stdf/lot2.stdf.gz `
  --cache-dir target/public-stdf-cache --memory-limit-mib 1024

# Recreate compact offline reports using Node.js with built-in Fetch.
node examples/public-stdf/prepare_reports.cjs
Start-Process examples/public-stdf/generated/a595-selection.html
```

In the live viewer, select tests in Tests navigation and use Show In for Histogram, Box, Trend, or Pareto. For a595, set Pareto Properties to the FTR pattern level. The live workspace can query the complete input; each prepared offline selection deliberately captures 24 attempts (prioritizing up to 12 failures, including failures of the selected FTR pattern) and up to three varying PTR tests plus one named FTR pattern test. All attempts is the export policy, so these snapshots do not deduplicate retests. The exact selection is saved in `generated/selected-reports.json` and embedded in each HTML file.

The prepared selection reports are approximately 4.93 MiB (lot2), 4.59 MiB (lot3), and 12.78 MiB (a595). The a595 selection includes the `amsdsm` pattern with six failing attempts. Associated raw records and run context remain inspectable. Full-file evidence exports can be much larger than the STDF source because they embed decoded field provenance; narrowing devices as well as tests reduces their size.

## Sources and reuse status

- [PySTDF sample directory](https://github.com/cmars/pystdf/tree/2215079c00e2f7b4c0be133baef4bf2a70d25859/data): its sample-specific README says these files originally came from Galaxy Semiconductor's public tool demonstrations. The PySTDF repository declares GPL-2.0, but this does not establish a separate open-data license from the original dataset owner. `demofile.stdf` and `lot3.stdf` have the same Git blob, so only lot3 was downloaded.
- [STDFReader sample directory](https://github.com/showjim/STDFReader/tree/6ebaa2608c81e77f921e65010433d2dcde82f160/sample_stdf): supplies a595; the repository declares GPL-2.0. No separate dataset license was identified in that directory.
- [rust-stdf compressed samples](https://github.com/noonchen/rust-stdf/tree/5160dc3859ea1ed19c94f008a1a6133299715993/demo_stdf): supplies the gzip copy. The code repository declares MIT, but lot2 is the same Galaxy-origin data as the PySTDF copy; it is not treated as newly MIT-licensed data.

These are **public samples hosted by open-source projects**, not confirmed Apache-2.0/open-data datasets. Upstream license texts and the sample-specific README are retained with the downloads. They are not included in zstdf's source distribution or relicensed under zstdf's Apache-2.0 license. Use the existing [synthetic viewer generator](../viewer/generate_demo.py) when a project-owned demonstration is needed for redistribution.
