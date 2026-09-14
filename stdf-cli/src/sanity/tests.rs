use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let p = std::env::temp_dir().join(format!(
            "zstdf-sanity-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn write(&self, name: &str, b: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        fs::write(&p, b).unwrap();
        p
    }
    fn args(&self, b: &[u8]) -> Arguments {
        Arguments {
            inputs: vec![self.write("input.stdf", b)],
            test_domain: Some("ft".into()),
            profile: None,
            run_profiles: None,
            output_dir: Some(self.0.join("report")),
            text_summary: None,
            fail_on_missing: false,
            preview_records_per_type: 2,
            max_report_mib: 8,
            disk_limit_mib: 16,
            max_units: 100,
            max_sources: 100,
            cancel_file: None,
        }
    }
    fn report(&self, a: &Arguments, failed: bool) -> Value {
        let result = generate(a, &mut Vec::new());
        if failed {
            assert!(result.is_err());
        } else {
            result.unwrap();
        }
        let html = fs::read_to_string(a.output_dir.as_ref().unwrap().join("report.html")).unwrap();
        let data = html
            .split("id=\"sanity-data\" type=\"application/json\">")
            .nth(1)
            .unwrap()
            .split("</script>")
            .next()
            .unwrap();
        serde_json::from_str(data).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn rec(typ: u8, sub: u8, b: &[u8]) -> Vec<u8> {
    let mut v = (b.len() as u16).to_le_bytes().to_vec();
    v.extend([typ, sub]);
    v.extend(b);
    v
}
fn cn(s: &str) -> Vec<u8> {
    let mut v = vec![s.len() as u8];
    v.extend(s.as_bytes());
    v
}
fn start() -> Vec<u8> {
    let mut v = rec(0, 10, &[2, 4]);
    let mut m = vec![0; 15];
    m[0..4].copy_from_slice(&10u32.to_le_bytes());
    m[4..8].copy_from_slice(&11u32.to_le_bytes());
    m[9] = b'P';
    m[10] = b' ';
    m[11] = b' ';
    m[14] = b' ';
    for s in ["LOT1", "DIE", "NODE", "TESTER", "JOB", "v1"] {
        m.extend(cn(s));
    }
    v.extend(rec(1, 10, &m));
    v
}
fn pir(site: u8) -> Vec<u8> {
    rec(5, 10, &[1, site])
}
fn prr(site: u8) -> Vec<u8> {
    let mut b = vec![1, site, 0];
    b.extend(0u16.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(1i16.to_le_bytes());
    b.extend(2i16.to_le_bytes());
    b.extend(0u32.to_le_bytes());
    b.extend(cn("SAME"));
    rec(5, 20, &b)
}
fn ptr(site: u8, n: f32) -> Vec<u8> {
    let mut b = 1u32.to_le_bytes().to_vec();
    b.extend([1, site, 0, 0]);
    b.extend(n.to_le_bytes());
    b.extend(cn("VDD"));
    rec(15, 10, &b)
}
fn mpr(site: u8, values: &[f32]) -> Vec<u8> {
    let mut b = 2u32.to_le_bytes().to_vec();
    b.extend([1, site, 0, 0]);
    b.extend(0u16.to_le_bytes());
    b.extend((values.len() as u16).to_le_bytes());
    for v in values {
        b.extend(v.to_le_bytes());
    }
    rec(15, 15, &b)
}
fn finish(b: &mut Vec<u8>) {
    b.extend(rec(1, 20, &20u32.to_le_bytes()));
}
fn simple() -> Vec<u8> {
    let mut b = start();
    b.extend(pir(1));
    b.extend(prr(1));
    finish(&mut b);
    b
}

fn custom_gdr(values: &[stdf_core::types::VarData]) -> Vec<u8> {
    use stdf_core::types::VarData::*;
    let mut b = (values.len() as u16).to_le_bytes().to_vec();
    for v in values {
        match v {
            Padding => b.push(0),
            Cn(s) => {
                b.push(10);
                b.extend(cn(s));
            }
            U1(v) => b.extend([1, *v]),
            U4(v) => {
                b.push(3);
                b.extend(v.to_le_bytes());
            }
            R8(v) => {
                b.push(8);
                b.extend(v.to_le_bytes());
            }
            _ => panic!("fixture type"),
        }
    }
    rec(50, 10, &b)
}
fn char_setup() -> Vec<u8> {
    use stdf_core::types::VarData::*;
    custom_gdr(&[
        Cn("SHMOO".into()),
        Cn("0x59".into()),
        Cn("plot".into()),
        Cn("suite".into()),
        Cn("title".into()),
        Cn("snakeVertical".into()),
        Cn("false".into()),
        U1(0),
    ])
}
fn char_result(site: u32) -> Vec<u8> {
    use stdf_core::types::VarData::*;
    custom_gdr(&[
        Cn("SHMOO_RESULT".into()),
        Padding,
        U4(89),
        U1(1),
        U4(site),
        Cn("110".into()),
        Cn("(0,1)".into()),
        Cn("suite".into()),
        Cn("pass".into()),
    ])
}
fn activity(site: u8, text: &str) -> Vec<u8> {
    let mut b = cn("ACTIVITY_TRACE_LOG");
    b.extend(cn("source"));
    b.extend([1, site]);
    b.extend(cn(text));
    rec(137, 10, &b)
}

#[test]
fn custom_records_are_extracted_without_sanity_and_sites_do_not_cross() {
    let f = Fixture::new();
    let mut b = start();
    // File-level ATR before MIR; omitted command line is not a missing finding.
    b.splice(6..6, rec(0, 20, &0u32.to_le_bytes()));
    b.extend(pir(1));
    b.extend(pir(2));
    b.extend(char_setup());
    b.extend(activity(2, "site two"));
    b.extend(activity(1, "site one"));
    b.extend(char_result(2));
    b.extend(char_result(1));
    b.extend(char_result(257));
    b.extend(prr(1));
    b.extend(pir(1));
    b.extend(activity(1, "retest"));
    b.extend(char_result(1));
    b.extend(prr(2));
    b.extend(prr(1));
    finish(&mut b);
    let a = f.args(&b);
    let v = f.report(&a, false);
    assert_eq!(v["records"]["CTSR"], 1);
    assert_eq!(v["records"]["CTRR"], 4);
    assert_eq!(v["units"][0]["previews"]["ATER"][0]["value"], "site one");
    assert_eq!(v["units"][1]["previews"]["ATER"][0]["value"], "site two");
    assert_eq!(v["units"][2]["previews"]["ATER"][0]["value"], "retest");
    assert_eq!(
        v["units"][0]["previews"]["CTRR"][0]["setup_association"],
        "resolved"
    );
    assert_eq!(
        v["units"][2]["previews"]["CTRR"][0]["setup_association"],
        "unresolved"
    );
    assert_eq!(v["units"][0]["counts"]["CTRR"], 1);
    assert_eq!(v["runs"][0]["unassigned"][0]["site_num"], 257);
    assert_eq!(v["runs"][0]["unassigned"][0]["ownership"], "unresolved");
    assert_eq!(v["file_records"][0]["fields"][0]["status"], "not_checked");
    assert_eq!(
        v["units"][0]["previews"]["ATER"][0]["status"],
        "not_checked"
    );
    assert!(!v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["rule"] == "unsupported_record" || v["rule"] == "record_ownership"));
    // Field evidence keeps names, full U4 site values, GDR tags and skipped status.
    let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
        File::open(
            fs::read_dir(a.output_dir.as_ref().unwrap())
                .unwrap()
                .map(|e| e.unwrap().path())
                .find(|p| p.is_dir())
                .unwrap()
                .join("record_fields.parquet"),
        )
        .unwrap(),
    )
    .unwrap()
    .build()
    .unwrap();
    let mut wide_site = false;
    let mut raw_gdr = false;
    for batch in reader {
        let batch = batch.unwrap();
        let names = batch
            .column(2)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .unwrap();
        let values = batch
            .column(7)
            .as_any()
            .downcast_ref::<arrow::array::StringArray>()
            .unwrap();
        for i in 0..batch.num_rows() {
            if fields::sanity_exempt(names.value(i)) {
                let field: Field = serde_json::from_str(values.value(i)).unwrap();
                assert_eq!(field.status, "not_checked");
                if field.name == "SITE_NUM" && field.raw == json!(257) {
                    wide_site = true;
                }
                if field.name == "GEN_DATA" {
                    raw_gdr = true;
                }
            }
        }
    }
    assert!(wide_site && raw_gdr);
}

#[test]
fn unchecked_cdr_atr_and_ater_skip_semantic_and_profile_failures() {
    let f = Fixture::new();
    let mut b = start();
    let mut atr = 0u32.to_le_bytes().to_vec();
    atr.extend([1, 0xff]);
    b.extend(rec(0, 20, &atr));
    // Structurally readable CDR: unusual inversion byte, no referenced PMRs.
    b.extend(rec(
        1,
        94,
        &[0, 1, 0, 0, 0, 0, 0, 0, 255, 255, 255, 255, 0, 0, 99, 0, 0],
    ));
    b.extend(pir(1));
    b.extend(activity(1, ""));
    b.extend(prr(1));
    finish(&mut b);
    let mut a = f.args(&b);
    a.profile=Some(f.write("profile.json",br#"{"version":1,"id":"skip","domain":"ft","require_wafer":false,"rules":[{"record":"ATR","field":"CMD_LINE","required":true,"pattern":"never"},{"record":"ATER","field":"ACTIVITY","required":true}]}"#));
    let v = f.report(&a, false);
    assert_eq!(v["records"]["CDR"], 1);
    assert!(!v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["severity"] == "error"));
}

#[test]
fn skipped_fields_do_not_change_text_missing_or_unknown_totals() {
    let f = Fixture::new();
    let mut b = simple();
    let base = f.args(&b);
    let mut plain = base;
    plain.output_dir = None;
    plain.text_summary = Some(f.0.join("base.txt"));
    generate(&plain, &mut Vec::new()).unwrap();
    b.splice(6..6, rec(0, 20, &0u32.to_le_bytes()));
    let mut a = f.args(&b);
    a.output_dir = None;
    a.text_summary = Some(f.0.join("extended.txt"));
    generate(&a, &mut Vec::new()).unwrap();
    let before = fs::read_to_string(plain.text_summary.unwrap()).unwrap();
    let after = fs::read_to_string(a.text_summary.unwrap()).unwrap();
    assert_eq!(
        before.lines().find(|l| l.starts_with("FIELD_TOTALS")),
        after.lines().find(|l| l.starts_with("FIELD_TOTALS"))
    );
    assert!(!after.contains("\tATR\t"));
}

#[test]
fn exempt_records_still_report_structural_decode_and_framing_errors() {
    for record in [
        rec(137, 10, &[22, b'x']),
        rec(1, 94, &[0]),
        custom_gdr(&[stdf_core::types::VarData::Cn("SHMOO_RESULT".into())]),
    ] {
        let f = Fixture::new();
        let mut b = start();
        b.extend(record);
        finish(&mut b);
        let a = f.args(&b);
        let v = f.report(&a, true);
        assert!(v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["rule"] == "decode"));
    }
    let f = Fixture::new();
    let mut b = simple();
    b.extend([20, 0, 137, 10, 1]);
    let a = f.args(&b);
    let v = f.report(&a, true);
    assert_eq!(v["sources"][0]["scan_complete"], false);
}

#[test]
fn ft_run_fields_and_prr_without_ptr_are_retained() {
    let f = Fixture::new();
    let a = f.args(&simple());
    let v = f.report(&a, false);
    assert_eq!(v["units"][0]["prr"]["passed"], true);
    assert_eq!(v["units"][0]["counts"], json!({}));
    assert_eq!(v["runs"][0]["metadata"][0]["fields"][0]["name"], "CPU_TYPE");
    assert_eq!(v["runs"][0]["metadata"][1]["fields"][0]["name"], "SETUP_T");
    assert_eq!(v["sources"][0]["scan_complete"], true);
}
#[test]
fn preview_is_first_two_records_per_type_not_two_units() {
    let f = Fixture::new();
    let mut b = start();
    for s in 1..=3 {
        b.extend(pir(s));
        for n in [1.25, 2.5, 3.75] {
            b.extend(ptr(s, n));
            b.extend(mpr(s, &[n, 99.0]));
        }
        b.extend(prr(s));
    }
    finish(&mut b);
    let a = f.args(&b);
    let v = f.report(&a, false);
    assert_eq!(v["units"].as_array().unwrap().len(), 3);
    for u in v["units"].as_array().unwrap() {
        assert_eq!(u["counts"]["PTR"], 3);
        assert_eq!(u["previews"]["PTR"].as_array().unwrap().len(), 2);
        assert_eq!(u["previews"]["MPR"][1]["value"]["value"], "2.5");
        assert_eq!(u["previews"]["MPR"][1]["element_count"], 2);
    }
}
#[test]
fn multisite_values_do_not_cross_and_dtr_is_unassigned() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(pir(1));
    b.extend(pir(2));
    b.extend(ptr(2, 22.0));
    b.extend(rec(50, 30, &cn("shared text")));
    b.extend(ptr(1, 11.0));
    b.extend(prr(2));
    b.extend(prr(1));
    finish(&mut b);
    let a = f.args(&b);
    let v = f.report(&a, false);
    assert_eq!(v["units"][0]["previews"]["PTR"][0]["value"]["value"], "11");
    assert_eq!(v["units"][1]["previews"]["PTR"][0]["value"]["value"], "22");
    assert_eq!(v["runs"][0]["unassigned"][0]["value"], "shared text");
}
#[test]
fn third_record_and_later_mpr_element_are_still_checked() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(pir(1));
    b.extend(ptr(1, 1.0));
    b.extend(ptr(1, 2.0));
    b.extend(ptr(1, f32::NAN));
    b.extend(mpr(1, &[1.0, f32::INFINITY]));
    b.extend(prr(1));
    finish(&mut b);
    let a = f.args(&b);
    let v = f.report(&a, true);
    assert_eq!(
        v["units"][0]["previews"]["PTR"].as_array().unwrap().len(),
        2
    );
    assert!(v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["message"].as_str().unwrap().contains("RTN_RSLT")));
}
#[test]
fn duplicate_gzip_source_does_not_duplicate_units() {
    let f = Fixture::new();
    let b = simple();
    let mut a = f.args(&b);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&b).unwrap();
    a.inputs.push(f.write("copy.gz", &gz.finish().unwrap()));
    let v = f.report(&a, false);
    assert_eq!(v["sources"].as_array().unwrap().len(), 1);
    assert_eq!(v["sources"][0]["paths"].as_array().unwrap().len(), 2);
    assert_eq!(v["units"].as_array().unwrap().len(), 1);
}
#[test]
fn repeated_part_id_keeps_independent_sequences() {
    let f = Fixture::new();
    let mut b = start();
    for _ in 0..2 {
        b.extend(pir(1));
        b.extend(prr(1));
    }
    finish(&mut b);
    let a = f.args(&b);
    let v = f.report(&a, false);
    assert_ne!(v["units"][0]["id"], v["units"][1]["id"]);
    assert_eq!(v["units"][0]["part_id"], v["units"][1]["part_id"]);
}
#[test]
fn cp_requires_wafer_but_ft_does_not() {
    let f = Fixture::new();
    let mut a = f.args(&simple());
    a.test_domain = Some("cp".into());
    let v = f.report(&a, true);
    assert!(v["findings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["rule"] == "cp_wafer"));
}
#[test]
fn malformed_and_unclosed_input_publishes_diagnostic() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(pir(1));
    b.extend([9, 0, 5]);
    let a = f.args(&b);
    let v = f.report(&a, true);
    assert_eq!(v["sources"][0]["scan_complete"], false);
    assert_eq!(v["units"][0]["closed"], false);
}
#[test]
fn cancelled_and_over_limit_runs_preserve_previous_report() {
    let f = Fixture::new();
    let mut a = f.args(&simple());
    f.report(&a, false);
    let old = fs::read(a.output_dir.as_ref().unwrap().join("report.html")).unwrap();
    a.cancel_file = Some(f.write("cancel", b""));
    assert!(generate(&a, &mut Vec::new()).is_err());
    assert_eq!(
        fs::read(a.output_dir.as_ref().unwrap().join("report.html")).unwrap(),
        old
    );
    a.cancel_file = None;
    a.max_units = 1;
    let mut b = start();
    for _ in 0..2 {
        b.extend(pir(1));
        b.extend(prr(1));
    }
    finish(&mut b);
    fs::write(&a.inputs[0], b).unwrap();
    assert!(generate(&a, &mut Vec::new()).is_err());
    assert_eq!(
        fs::read(a.output_dir.as_ref().unwrap().join("report.html")).unwrap(),
        old
    );
}
#[test]
fn text_export_is_safe_and_first_record_is_not_skipped() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(pir(1));
    b.extend(rec(50, 30, &cn("</script><script>alert(1)</script>")));
    b.extend(rec(50, 30, &cn("__GENERATION__ __SANITY_DATA__")));
    b.extend(mpr(1, &[]));
    b.extend(mpr(1, &[3.0]));
    b.extend(prr(1));
    finish(&mut b);
    let a = f.args(&b);
    let v = f.report(&a, false);
    assert!(v["units"][0]["previews"]["MPR"][0]["value"].is_null());
    assert_eq!(v["units"][0]["previews"]["MPR"][1]["value"]["value"], "3");
    let html = fs::read_to_string(a.output_dir.as_ref().unwrap().join("report.html")).unwrap();
    assert!(!html.contains("</script><script>alert(1)"));
    assert_eq!(
        v["units"][0]["previews"]["DTR"][1]["value"],
        "__GENERATION__ __SANITY_DATA__"
    );
}
#[test]
fn profile_rejects_unknown_fields_and_enforces_full_match() {
    let f = Fixture::new();
    let mut a = f.args(&simple());
    a.profile=Some(f.write("profile.json",br#"{"version":1,"id":"strict","domain":"ft","rules":[{"record":"MIR","field":"LOT_ID","pattern":"LOT"}]}"#));
    f.report(&a, true);
    a.profile=Some(f.write("bad.json",br#"{"version":1,"id":"bad","domain":"ft","rules":[{"record":"MIR","field":"NO_SUCH_FIELD"}]}"#));
    assert!(generate(&a, &mut Vec::new())
        .unwrap_err()
        .to_string()
        .contains("unknown profile field"));
}
#[test]
fn parquet_field_evidence_and_source_hash_are_readable() {
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    let f = Fixture::new();
    let a = f.args(&simple());
    let v = f.report(&a, false);
    let dir = fs::read_dir(a.output_dir.as_ref().unwrap())
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.is_dir())
        .unwrap();
    let batches = ParquetRecordBatchReaderBuilder::try_new(
        File::open(dir.join("record_fields.parquet")).unwrap(),
    )
    .unwrap()
    .build()
    .unwrap();
    let count: usize = batches.map(|b| b.unwrap().num_rows()).sum();
    assert!(count > 40);
    let raw =
        fs::read(dir.join(format!("{}.stdf", v["sources"][0]["id"].as_str().unwrap()))).unwrap();
    assert_eq!(raw, simple());
}

#[test]
fn disk_exhaustion_preserves_old_report_and_removes_only_new_generation() {
    let f = Fixture::new();
    let mut a = f.args(&simple());
    f.report(&a, false);
    let old = fs::read(a.output_dir.as_ref().unwrap().join("report.html")).unwrap();
    let before = fs::read_dir(a.output_dir.as_ref().unwrap())
        .unwrap()
        .count();
    a.disk_limit_mib = 1;
    let mut data = simple();
    data.resize(2 * 1024 * 1024, 0);
    fs::write(&a.inputs[0], data).unwrap();
    assert!(generate(&a, &mut Vec::new())
        .unwrap_err()
        .to_string()
        .contains("disk limit"));
    assert_eq!(
        fs::read(a.output_dir.as_ref().unwrap().join("report.html")).unwrap(),
        old
    );
    assert_eq!(
        fs::read_dir(a.output_dir.as_ref().unwrap())
            .unwrap()
            .count(),
        before
    );
}

#[test]
fn unknown_ftr_and_empty_mpr_are_not_displayed_as_valid_results() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(pir(1));
    b.extend(rec(15, 20, &[1, 0, 0, 0, 1, 1, 0x40]));
    b.extend(mpr(1, &[]));
    b.extend(prr(1));
    finish(&mut b);
    let a = f.args(&b);
    let v = f.report(&a, false);
    assert_eq!(v["units"][0]["previews"]["FTR"][0]["status"], "unknown");
    assert_eq!(v["units"][0]["previews"]["FTR"][0]["raw"], 0x40);
    assert!(v["units"][0]["previews"]["FTR"][0]["value"].is_null());
    assert_eq!(v["units"][0]["previews"]["MPR"][0]["status"], "empty");
}

#[test]
fn input_order_does_not_change_summary_and_profile_hash() {
    let f = Fixture::new();
    let b = simple();
    let mut other = simple();
    other[16] = 2;
    let mut a = f.args(&b);
    a.inputs.push(f.write("other.stdf", &other));
    let first = f.report(&a, false);
    a.inputs.reverse();
    let second = f.report(&a, false);
    assert_eq!(first, second);
}

fn routed(f: &Fixture, a: &mut Arguments, routes: Value) {
    a.test_domain = None;
    a.run_profiles = Some(
        f.write(
            "routes.json",
            &serde_json::to_vec(&json!({
                "version":1,"id":"mixed-demo","routes":routes
            }))
            .unwrap(),
        ),
    );
}
fn route(id: &str, domain: &str, matches: Value) -> Value {
    json!({"matches":matches,"profile":{"version":1,"id":id,"domain":domain}})
}
#[test]
fn mixed_run_profiles_use_and_or_selectors_and_are_order_independent() {
    let f = Fixture::new();
    let mut a = f.args(&simple());
    let mut other = simple();
    let at = other.windows(3).position(|s| s == b"JOB").unwrap();
    other[at..at + 3].copy_from_slice(b"FT2");
    a.inputs.push(f.write("other.stdf", &other));
    routed(
        &f,
        &mut a,
        json!([
            route(
                "cp-v1",
                "cp",
                json!([{"mir":{"JOB_NAM":"JOB","JOB_REV":"v1"}}])
            ),
            route(
                "ft-v1",
                "ft",
                json!([{"mir":{"JOB_NAM":"unused"}},{"mir":{"JOB_NAM":"FT2"}}])
            )
        ]),
    );
    let v = f.report(&a, false);
    let domains: std::collections::BTreeSet<_> = v["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["domain"].as_str().unwrap())
        .collect();
    assert_eq!(domains, ["cp", "ft"].into_iter().collect());
    assert!(v["runs"]
        .as_array()
        .unwrap()
        .iter()
        .all(|r| r["profile_hash"].as_str().unwrap().len() == 64));
    a.inputs.reverse();
    assert_eq!(v, f.report(&a, false));
}
#[test]
fn ambiguous_and_unmatched_profiles_preserve_unknown_run_and_units() {
    let f = Fixture::new();
    let mut a = f.args(&simple());
    for routes in [
        json!([route("none", "ft", json!([{"mir":{"JOB_NAM":"OTHER"}}]))]),
        json!([
            route("one", "ft", json!([{"mir":{"JOB_NAM":"JOB"}}])),
            route("two", "cp", json!([{"mir":{"JOB_NAM":"JOB"}}]))
        ]),
    ] {
        routed(&f, &mut a, routes);
        let v = f.report(&a, true);
        assert_eq!(v["runs"][0]["domain"], "unknown");
        assert!(v["runs"][0]["profile_id"].is_null());
        assert_eq!(v["units"].as_array().unwrap().len(), 1);
        assert!(v["findings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|x| x["rule"] == "run_profile"));
    }
}
#[test]
fn exact_source_and_mir_offset_routing_and_invalid_config_preserve_report() {
    let f = Fixture::new();
    let mut a = f.args(&simple());
    let hash = format!("{:x}", Sha256::digest(simple()));
    routed(
        &f,
        &mut a,
        json!([route(
            "exact",
            "ft",
            json!([{"source_id":hash,"mir_offset":"6","mir":{"JOB_NAM":"JOB"}}])
        )]),
    );
    f.report(&a, false);
    let old = fs::read(a.output_dir.as_ref().unwrap().join("report.html")).unwrap();
    for selectors in [
        json!([{}]),
        json!([{"mir_offset":"06"}]),
        json!([{"mir":{"UNKNOWN":"x"}}]),
    ] {
        routed(&f, &mut a, json!([route("bad", "ft", selectors)]));
        assert!(generate(&a, &mut Vec::new()).is_err());
        assert_eq!(
            fs::read(a.output_dir.as_ref().unwrap().join("report.html")).unwrap(),
            old
        );
    }
}
#[test]
fn definition_only_record_is_run_evidence_and_defaults_reset_at_mir() {
    let f = Fixture::new();
    let mut b = start();
    let mut definition = ptr(1, 0.0);
    definition[10] = 0x10;
    // Remaining optional fields: alarm, opt, three scales, two limits, units.
    definition.extend([0, 2, 0, 0, 0]);
    definition.extend(0f32.to_le_bytes());
    definition.extend(10f32.to_le_bytes());
    definition.extend(cn("V"));
    let len = (definition.len() - 4) as u16;
    definition[..2].copy_from_slice(&len.to_le_bytes());
    b.extend(definition);
    b.extend(pir(1));
    b.extend(ptr(1, 1.0));
    b.extend(prr(1));
    finish(&mut b);
    // A second MIR in the same source must not reuse first-run definitions.
    b.extend(&start()[6..]);
    b.extend(pir(1));
    b.extend(ptr(1, 2.0));
    b.extend(prr(1));
    finish(&mut b);
    let a = f.args(&b);
    let v = f.report(&a, false);
    assert_eq!(v["runs"][0]["unassigned"][0]["definition_only"], true);
    assert_eq!(v["units"].as_array().unwrap().len(), 2);
    assert_eq!(
        v["units"][0]["previews"]["PTR"][0]["units"]["effective"],
        "V"
    );
    assert_eq!(
        v["units"][0]["previews"]["PTR"][0]["units"]["origin"],
        "inherited"
    );
    assert!(v["units"][1]["previews"]["PTR"][0]["units"]["effective"].is_null());
}

#[test]
fn text_summary_scans_beyond_previews_and_retains_missing_and_source_locations() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(pir(1));
    b.extend(ptr(1, 1.0));
    b.extend(ptr(1, 2.0));
    let invalid_offset = b.len();
    b.extend(ptr(1, f32::NAN));
    b.extend(prr(1));
    finish(&mut b);
    let mut a = f.args(&b);
    a.output_dir = None;
    a.text_summary = Some(f.0.join("issues.txt"));
    assert!(generate(&a, &mut Vec::new())
        .unwrap_err()
        .to_string()
        .contains("diagnostic text summary published"));
    let txt = fs::read_to_string(a.text_summary.as_ref().unwrap()).unwrap();
    assert!(txt.contains("sanity-text-v1"));
    assert!(txt.contains("input.stdf"));
    assert!(txt.contains(&format!("\t{invalid_offset}\tPTR\tRESULT\tinvalid\t")));
    assert!(txt.contains("\tMIR\tUSER_TXT\tmissing\t"));
    assert!(txt.contains("NaN"));
    assert!(txt.contains("scan_complete=true"));
    assert!(!f.0.join("report").exists());
    assert!(!fs::read_dir(&f.0).unwrap().any(|e| e
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with("sanity-")));
}

#[test]
fn text_summary_optional_missing_policy_and_failure_preservation() {
    let f = Fixture::new();
    let mut a = f.args(&simple());
    a.output_dir = None;
    let output = f.0.join("issues.txt");
    a.text_summary = Some(output.clone());
    generate(&a, &mut Vec::new()).unwrap();
    a.fail_on_missing = true;
    assert!(generate(&a, &mut Vec::new())
        .unwrap_err()
        .to_string()
        .contains("missing fields"));
    let old = fs::read(&output).unwrap();
    a.cancel_file = Some(f.write("cancel", b"stop"));
    assert!(generate(&a, &mut Vec::new()).is_err());
    assert_eq!(fs::read(&output).unwrap(), old);
    a.cancel_file = None;
    a.profile = Some(f.write("bad-profile.json", b"{}"));
    assert!(generate(&a, &mut Vec::new()).is_err());
    assert_eq!(fs::read(&output).unwrap(), old);
    a.profile = None;
    a.text_summary = Some(a.inputs[0].clone());
    assert!(generate(&a, &mut Vec::new())
        .unwrap_err()
        .to_string()
        .contains("input"));
    assert_eq!(fs::read(&a.inputs[0]).unwrap(), simple());
}

#[test]
fn text_summary_escapes_control_text_and_includes_profile_failures_and_aliases() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(pir(1));
    b.extend(rec(50, 30, &cn("line\n\t\x1b</script>")));
    b.extend(prr(1));
    finish(&mut b);
    let mut a = f.args(&b);
    a.inputs.push(f.write("copy.stdf", &b));
    a.output_dir = None;
    a.text_summary = Some(f.0.join("issues.txt"));
    a.profile = Some(f.write("profile.json", br#"{"version":1,"id":"p","domain":"ft","rules":[{"record":"MIR","field":"JOB_NAM","pattern":"OTHER"}]}"#));
    assert!(generate(&a, &mut Vec::new()).is_err());
    let txt = fs::read_to_string(a.text_summary.as_ref().unwrap()).unwrap();
    assert!(txt.contains("copy.stdf") && txt.contains("input.stdf"));
    assert!(txt.contains("MIR.JOB_NAM does not satisfy profile p"));
    assert!(!txt.contains('\x1b'));
    assert!(txt.contains("sources=1"));
}

#[test]
fn text_summary_cli_requires_exactly_one_output_mode() {
    use clap::Parser;
    let base = ["zstdf-cli", "sanity", "input.stdf", "--test-domain", "ft"];
    assert!(crate::Cli::try_parse_from(base).is_err());
    assert!(
        crate::Cli::try_parse_from(base.into_iter().chain(["--text-summary", "issues.txt"]))
            .is_ok()
    );
    assert!(crate::Cli::try_parse_from(base.into_iter().chain([
        "--text-summary",
        "issues.txt",
        "--output-dir",
        "report"
    ]))
    .is_err());
    assert!(crate::Cli::try_parse_from(base.into_iter().chain([
        "--output-dir",
        "report",
        "--fail-on-missing"
    ]))
    .is_err());
}

#[test]
fn text_summary_limits_and_incomplete_scan_preserve_publication_contract() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(pir(1));
    for _ in 0..1500 {
        b.extend(ptr(1, 1.0));
    }
    b.extend(prr(1));
    finish(&mut b);
    let mut a = f.args(&b);
    a.output_dir = None;
    let output = f.write("issues.txt", b"previous summary");
    a.text_summary = Some(output.clone());
    a.max_report_mib = 1;
    let error = generate(&a, &mut Vec::new()).unwrap_err().to_string();
    assert!(error.contains("budget"), "{error}");
    assert_eq!(fs::read(&output).unwrap(), b"previous summary");
    a.inputs = vec![f.write("truncated.stdf", &[2, 0, 0, 10, 2, 4, 5])];
    a.max_report_mib = 8;
    assert!(generate(&a, &mut Vec::new()).is_err());
    let txt = fs::read_to_string(&output).unwrap();
    assert!(txt.contains("scan_complete=false"));
    assert!(txt.contains("framing"));
}

#[test]
fn text_summary_final_commit_cancellation_preserves_existing_file() {
    let f = Fixture::new();
    let output = f.write("issues.txt", b"previous summary");
    let stage = storage::Stage::new(&f.0, 4096).unwrap();
    assert!(stage
        .publish_text(&output, b"new summary", || Err("cancel at commit".into()))
        .is_err());
    assert_eq!(fs::read(&output).unwrap(), b"previous summary");
    let limited = storage::Stage::new(&f.0, 1).unwrap();
    assert!(limited
        .publish_text(&output, b"new summary", || Ok(()))
        .is_err());
    assert_eq!(fs::read(&output).unwrap(), b"previous summary");
}
