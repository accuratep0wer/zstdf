# STDF ASCII highlighting for Notepad++

Import [zstdf-ascii.udl.xml](zstdf-ascii.udl.xml) to read zstdf `to-ascii`
exports with record types, field names and numbers highlighted. This is a
Notepad++ User Defined Language (UDL 2.1), not a binary STDF reader or a
pretty-printing plugin. It does not change file contents.

## Install and use

1. In Notepad++, open **Language > User Defined Language > Define your language**
   (some versions show **Language > Define your language**).
2. Click **Import**, select `zstdf-ascii.udl.xml`, and restart Notepad++ if
   `STDF ASCII` does not appear in the Language menu.
3. Generate text from the repository root:

   ```powershell
   .\target\release\zstdf-cli.exe to-ascii "C:\data\run.stdf" "C:\data\run.stdfascii"
   ```

4. Open the output in Notepad++. The `.stdfascii` and `.stdftext` extensions
   select this language automatically. For an existing `.txt` export, select
   **Language > STDF ASCII** manually.

Gzip input is also supported. The output directory must already exist, and
`to-ascii` overwrites an existing output file. The definition deliberately does
not associate ordinary `.txt` files or binary `.stdf` files with this language.

Try [sample.stdfascii](sample.stdfascii), an unmodified export of the project's
small synthetic viewer fixture. No production or third-party test data is included.

## Color key

| Appearance | Meaning |
| --- | --- |
| Bold blue | Record type and `Record` heading token |
| Slate | General field names |
| Blue | Device, site, wafer and test identity fields |
| Green | Measurement, units and limit field names |
| Amber | Flags, bins, alarms and failure-related field names |
| Purple | Time fields and numeric values |
| Teal | GDR value-type tags, such as `R4`, `R8` and `Cn` |
| Red | Literal `NaN` and `inf` tokens |

Colors are lexical categories, not validation results. In particular, amber
does not mean a device failed, and green does not mean a measurement passed.
The definition does not interpret flag bits, compare limits, or infer missing
fields. Free-text values can also contain matching keywords.

Timestamp fields retain the integer and converted date already emitted by
`to-ascii`; the editor does not perform date conversion. Dynamic vendor field
paths and arbitrary text remain readable as ordinary text; GDR type tags and
numbers are highlighted. Record mnemonics include ATR, CDR, ATER, CTSR and CTRR,
but highlighting does not add decoding support to the converter.

This draft uses a light background. Change the colors in the UDL dialog to suit
your editor theme. Strings and comments are intentionally not delimited because
STDF text can contain unmatched quotes, hashes and semicolons. Record folding
is not enabled: interleaved sites do not form safely nested PIR/PRR blocks.

## Verification and references

The XML was parsed and its field keywords checked against the current ASCII
formatter. The sample was generated using the release CLI. Interactive import
and rendering in Notepad++ have not been verified in this environment.

To regenerate the sample:

```powershell
python examples/viewer/generate_demo.py --units 2 --source-only --output-dir target/notepad-demo
.\target\release\zstdf-cli.exe to-ascii target/notepad-demo/demo.stdf config/notepad-plus-plus/sample.stdfascii
```

See the [official Notepad++ UDL documentation](https://github.com/notepad-plus-plus/npp-usermanual/blob/master/content/docs/user-defined-language-system.md)
for import and customization. These files use the repository's Apache-2.0 license.
