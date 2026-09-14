//! Bounded full-field text output for shell automation; previews never select its rows.
use super::*;
use arrow::array::{StringArray, UInt64Array};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

fn append(output: &mut Vec<u8>, budget: &mut Budget, line: &str) -> CliResult<()> {
    budget.charge(line.len())?;
    output.extend_from_slice(line.as_bytes());
    Ok(())
}

pub(super) fn render(
    stage: &Path,
    report: &Report,
    budget: &mut Budget,
) -> CliResult<(Vec<u8>, u64)> {
    let mut output = Vec::new();
    append(&mut output, budget, &format!(
        "zstdf sanity-text-v1\nrule_version={}\nprofile_hash={}\nsources={} runs={} units={}\nvalidation_failed={}\nfail_on_missing={}\n",
        report.rule_version, report.profile_hash, report.sources.len(), report.runs.len(),
        report.units.len(), report.validation_failed, budget.args.fail_on_missing
    ))?;
    append(&mut output, budget, "ATR, CDR, ATER, CTSR and CTRR are extracted but not sanity checked; excluded from field totals. Framing/decode errors remain diagnostics. Missing optional fields do not imply validation failure. Unknown is not valid. Offsets refer to decompressed bytes. Values and paths are JSON-escaped.\n")?;
    for source in &report.sources {
        append(
            &mut output,
            budget,
            &format!(
                "SOURCE {} bytes={} scan_complete={} paths={}\n",
                source.id,
                source.bytes,
                source.scan_complete,
                serde_json::to_string(&source.paths)?
            ),
        )?;
    }
    append(&mut output, budget, "\nFIELDS (all invalid/missing/unknown fields, including records outside previews)\nsource_id\trecord_offset\trecord\tfield\tstatus\tfield_byte_offset\tpresence\torigin\traw_json\teffective_json\tissues_json\n")?;
    let reader =
        ParquetRecordBatchReaderBuilder::try_new(File::open(stage.join("record_fields.parquet"))?)?
            .with_batch_size(128)
            .build()?;
    let (mut invalid, mut missing, mut unknown) = (0u64, 0u64, 0u64);
    for batch in reader {
        budget.check()?;
        let batch = batch?;
        let strings = |i: usize| {
            batch
                .column(i)
                .as_any()
                .downcast_ref::<StringArray>()
                .expect("evidence schema")
        };
        let offsets = batch
            .column(1)
            .as_any()
            .downcast_ref::<UInt64Array>()
            .expect("evidence offsets");
        for i in 0..batch.num_rows() {
            let field: Field = serde_json::from_str(strings(7).value(i))?;
            match field.status.as_str() {
                "valid" | "not_checked" => continue,
                "invalid" => invalid += 1,
                "missing" => missing += 1,
                _ => unknown += 1,
            }
            append(
                &mut output,
                budget,
                &format!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                    strings(0).value(i),
                    offsets.value(i),
                    strings(2).value(i),
                    field.name,
                    field.status,
                    offsets.value(i) + field.byte_start as u64,
                    field.presence,
                    field.origin,
                    serde_json::to_string(&field.raw)?,
                    serde_json::to_string(&field.effective)?,
                    serde_json::to_string(&field.issues)?
                ),
            )?;
        }
    }
    append(&mut output, budget, &format!("\nFIELD_TOTALS invalid={invalid} missing={missing} unknown={unknown}\n\nDIAGNOSTICS (including structural and product-profile failures)\n"))?;
    for finding in &report.findings {
        append(
            &mut output,
            budget,
            &format!("{}\n", serde_json::to_string(finding)?),
        )?;
    }
    append(
        &mut output,
        budget,
        &format!(
            "\nEND scan_complete={} diagnostic_count={}\n",
            report.sources.iter().all(|s| s.scan_complete),
            report.findings.len()
        ),
    )?;
    Ok((output, missing))
}
