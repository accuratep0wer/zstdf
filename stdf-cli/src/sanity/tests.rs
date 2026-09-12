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
            output_dir: self.0.join("report"),
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
        let html = fs::read_to_string(a.output_dir.join("report.html")).unwrap();
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
    let old = fs::read(a.output_dir.join("report.html")).unwrap();
    a.cancel_file = Some(f.write("cancel", b""));
    assert!(generate(&a, &mut Vec::new()).is_err());
    assert_eq!(fs::read(a.output_dir.join("report.html")).unwrap(), old);
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
    assert_eq!(fs::read(a.output_dir.join("report.html")).unwrap(), old);
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
    let html = fs::read_to_string(a.output_dir.join("report.html")).unwrap();
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
    let dir = fs::read_dir(&a.output_dir)
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
    let old = fs::read(a.output_dir.join("report.html")).unwrap();
    let before = fs::read_dir(&a.output_dir).unwrap().count();
    a.disk_limit_mib = 1;
    let mut data = simple();
    data.resize(2 * 1024 * 1024, 0);
    fs::write(&a.inputs[0], data).unwrap();
    assert!(generate(&a, &mut Vec::new())
        .unwrap_err()
        .to_string()
        .contains("disk limit"));
    assert_eq!(fs::read(a.output_dir.join("report.html")).unwrap(), old);
    assert_eq!(fs::read_dir(&a.output_dir).unwrap().count(), before);
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
    let old = fs::read(a.output_dir.join("report.html")).unwrap();
    for selectors in [
        json!([{}]),
        json!([{"mir_offset":"06"}]),
        json!([{"mir":{"UNKNOWN":"x"}}]),
    ] {
        routed(&f, &mut a, json!([route("bad", "ft", selectors)]));
        assert!(generate(&a, &mut Vec::new()).is_err());
        assert_eq!(fs::read(a.output_dir.join("report.html")).unwrap(), old);
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
