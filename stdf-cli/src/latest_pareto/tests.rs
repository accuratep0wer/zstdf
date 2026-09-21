// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
use super::*;
use std::io::Write;

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = crate::tests::temp_path("latest_pareto");
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn file(&self, name: &str, data: &[u8]) -> PathBuf {
        let p = self.0.join(name);
        std::fs::write(&p, data).unwrap();
        p
    }
    fn report(&self, inputs: &[PathBuf]) -> Report {
        analyze(inputs, &self.0.join("report.html"), 16 * 1024 * 1024).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}
fn cn(s: &str) -> Vec<u8> {
    let mut b = vec![s.len() as u8];
    b.extend(s.as_bytes());
    b
}
fn rec(typ: u8, sub: u8, body: Vec<u8>) -> Vec<u8> {
    let mut r = (body.len() as u16).to_le_bytes().to_vec();
    r.extend([typ, sub]);
    r.extend(body);
    r
}
fn mir(lot: &str, start: u32) -> Vec<u8> {
    let mut b = vec![0; 15];
    b[4..8].copy_from_slice(&start.to_le_bytes());
    b.extend(cn(lot));
    for s in ["DEVICE", "NODE", "TESTER", "JOB"] {
        b.extend(cn(s));
    }
    rec(1, 10, b)
}
fn start(lot: &str, wafer: Option<&str>, time: u32) -> Vec<u8> {
    let mut b = rec(0, 10, vec![2, 4]);
    b.extend(mir(lot, time));
    if let Some(w) = wafer {
        let mut wir = vec![1, 1];
        wir.extend(time.to_le_bytes());
        wir.extend(cn(w));
        b.extend(rec(2, 10, wir));
    }
    b
}
fn pir(site: u8) -> Vec<u8> {
    rec(5, 10, vec![1, site])
}
fn ptr(site: u8, num: u32, name: &str, value: f32, flag: u8) -> Vec<u8> {
    let mut b = num.to_le_bytes().to_vec();
    b.extend([1, site, flag, 0]);
    b.extend(value.to_le_bytes());
    b.extend(cn(name));
    rec(15, 10, b)
}
fn ftr(site: u8, num: u32, name: &str, flag: u8) -> Vec<u8> {
    let mut b = num.to_le_bytes().to_vec();
    b.extend([1, site, flag, 0xc0]);
    b.extend([0; 32]);
    b.extend(cn(name));
    rec(15, 20, b)
}
fn prr(site: u8, x: i16, y: i16, flag: u8, bin: u16) -> Vec<u8> {
    let mut b = vec![1, site, flag];
    b.extend(1u16.to_le_bytes());
    b.extend(bin.to_le_bytes());
    b.extend(bin.to_le_bytes());
    b.extend(x.to_le_bytes());
    b.extend(y.to_le_bytes());
    b.extend(1u32.to_le_bytes());
    b.extend(cn("REUSED_PART_ID"));
    rec(5, 20, b)
}
fn unit(x: i16, passed: bool) -> Vec<u8> {
    let mut b = pir(1);
    b.extend(ptr(1, 100, "IDD", 1.0, if passed { 0 } else { 128 }));
    b.extend(ftr(1, 200, "scan/core", if passed { 0 } else { 128 }));
    b.extend(prr(
        1,
        x,
        1,
        if passed { 0 } else { 8 },
        if passed { 1 } else { 9 },
    ));
    b
}
fn end() -> Vec<u8> {
    rec(1, 20, 9000u32.to_le_bytes().to_vec())
}
fn run(lot: &str, wafer: Option<&str>, time: u32, passed: bool) -> Vec<u8> {
    let mut b = start(lot, wafer, time);
    b.extend(unit(1, passed));
    b.extend(end());
    b
}

#[test]
fn start_time_selects_latest_independent_of_paths_and_input_order() {
    let f = Fixture::new();
    let old = f.file("z-old.stdf", &run("L1", Some("W1"), 100, false));
    let new = f.file("a-new.stdf", &run("L1", Some("W1"), 200, true));
    let a = f.report(&[old.clone(), new.clone()]);
    let b = f.report(&[new, old]);
    assert_eq!(
        serde_json::to_value(&a).unwrap(),
        serde_json::to_value(&b).unwrap()
    );
    let latest: Vec<_> = a
        .attempts
        .iter()
        .filter(|a| a.is_latest == Some(true))
        .collect();
    assert_eq!(latest.len(), 1);
    assert_eq!(latest[0].passed, Some(true));
    assert_eq!(latest[0].hard_bin, Some(1));
    assert_eq!(latest[0].start_t, Some(200));
    assert!(latest[0].patterns.iter().all(|m| m.passed == Some(true)));
    assert_eq!(
        a.attempts
            .iter()
            .filter(|a| a.is_latest == Some(false))
            .count(),
        1
    );
}

#[test]
fn same_run_uses_latest_whole_attempt_without_carrying_missing_tests() {
    let f = Fixture::new();
    let mut b = start("L1", Some("W1"), 100);
    b.extend(unit(1, false));
    b.extend(pir(2));
    b.extend(prr(2, 1, 1, 0, 1));
    b.extend(unit(2, false));
    b.extend(end());
    let r = f.report(&[f.file("runs.stdf", &b)]);
    let latest: Vec<_> = r
        .attempts
        .iter()
        .filter(|a| a.is_latest == Some(true))
        .collect();
    assert_eq!(latest.len(), 2);
    let a = latest.iter().find(|a| a.x == Some(1)).unwrap();
    assert_eq!(a.site, 2);
    assert!(a.tests.is_empty());
    assert!(a.patterns.is_empty());
    assert_eq!(a.sequence, 2);
}

#[test]
fn lot_and_wafer_are_both_part_of_device_identity() {
    let f = Fixture::new();
    let inputs = [
        ("a", "L1", Some("W1")),
        ("b", "L2", Some("W1")),
        ("c", "L1", Some("W2")),
        ("d", "L1", None),
    ]
    .map(|(name, l, w)| f.file(name, &run(l, w, 100, true)));
    let r = f.report(&inputs);
    assert_eq!(
        r.attempts
            .iter()
            .filter(|a| a.is_latest == Some(true))
            .count(),
        4
    );
    assert_eq!(
        r.attempts
            .iter()
            .filter_map(|a| a.ecid.as_ref())
            .collect::<BTreeSet<_>>()
            .len(),
        4
    );
}

#[test]
fn ties_and_missing_start_times_are_explicitly_ambiguous() {
    for time in [0, 100] {
        let f = Fixture::new();
        let a = f.file("a", &run("L", Some("W"), 100, true));
        let b = f.file("b", &run("L", Some("W"), time, false));
        let r = f.report(&[a, b]);
        assert!(r
            .attempts
            .iter()
            .all(|a| a.is_latest.is_none() && a.issue == Some("ambiguous_latest")));
    }
}

#[test]
fn identical_gzip_and_plain_sources_count_once() {
    let f = Fixture::new();
    let bytes = run("L", Some("W"), 100, true);
    let a = f.file("a.stdf", &bytes);
    let mut zip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    zip.write_all(&bytes).unwrap();
    let b = f.file("b.stdf.gz", &zip.finish().unwrap());
    let r = f.report(&[a, b]);
    assert_eq!(r.attempts.len(), 1);
    assert_eq!(r.sources.len(), 1);
    assert_eq!(r.sources.values().next().unwrap().len(), 2);
}

#[test]
fn coordinates_use_prr_or_complete_ptr_tuple_per_site() {
    let f = Fixture::new();
    let mut b = start("L", None, 100);
    b.extend(pir(1));
    b.extend(pir(2));
    b.extend(ptr(1, 10, "coordinate_X", 11.0, 0));
    b.extend(ptr(2, 10, "coordinate_X", 22.0, 0));
    b.extend(ptr(1, 11, "coordinate_Y", 33.0, 0));
    b.extend(ptr(2, 11, "coordinate_Y", 44.0, 0));
    b.extend(prr(2, 0, 2, 0, 1));
    b.extend(prr(1, -1, 3, 0, 1));
    b.extend(pir(1));
    b.extend(ptr(1, 10, "coordinate_X", 999.0, 0));
    b.extend(prr(1, 5, 6, 0, 1));
    b.extend(end());
    let r = f.report(&[f.file("sites", &b)]);
    assert_eq!((r.attempts[0].x, r.attempts[0].y), (Some(22), Some(44)));
    assert_eq!((r.attempts[1].x, r.attempts[1].y), (Some(11), Some(33)));
    assert_eq!(r.attempts[0].coordinate_source, "PTR");
    assert_eq!((r.attempts[2].x, r.attempts[2].y), (Some(5), Some(6)));
    assert_eq!(r.attempts[2].coordinate_source, "PRR");
}

#[test]
fn invalid_coordinates_do_not_merge_by_repeated_part_id() {
    let f = Fixture::new();
    let mut b = start("L", None, 100);
    for value in [0.0, -1.0, f32::NAN, f32::INFINITY, 1.5] {
        b.extend(pir(1));
        b.extend(ptr(1, 10, "X", value, 0));
        b.extend(ptr(1, 11, "Y", 2.0, 0));
        b.extend(prr(1, 0, 0, 0, 1));
    }
    b.extend(end());
    let r = f.report(&[f.file("invalid", &b)]);
    assert!(r
        .attempts
        .iter()
        .all(|a| a.ecid.is_none() && a.issue == Some("unresolved_identity")));
}

#[test]
fn wafer_scribe_from_wrr_backfills_completed_devices() {
    for wafer in [None, Some("TESTER_WAFER")] {
        let f = Fixture::new();
        let mut b = start("L", wafer, 100);
        b.extend(unit(1, true));
        let mut w = vec![1, 1];
        w.extend([0; 24]);
        w.extend(cn("TESTER_WAFER"));
        w.extend(cn("FAB-SCRIBE-42"));
        b.extend(rec(2, 20, w));
        b.extend(end());
        let r = f.report(&[f.file("scribe", &b)]);
        assert_eq!(r.attempts[0].wafer.as_deref(), Some("FAB-SCRIBE-42"));
        assert!(r.attempts[0]
            .ecid
            .as_ref()
            .unwrap()
            .contains("FAB-SCRIBE-42"));
    }
}

#[test]
fn patterns_deduplicate_per_device_and_keep_distinct_test_failures() {
    let f = Fixture::new();
    let mut b = start("L", Some("W"), 100);
    b.extend(pir(1));
    b.extend(ftr(1, 100, "scan", 128));
    b.extend(ftr(1, 100, "scan", 0));
    b.extend(ftr(1, 200, "scan", 128));
    b.extend(ftr(1, 300, "other", 64));
    b.extend(prr(1, 1, 1, 8, 9));
    b.extend(end());
    let r = f.report(&[f.file("patterns", &b)]);
    let a = &r.attempts[0];
    assert_eq!(a.patterns.len(), 2);
    assert_eq!(
        a.patterns
            .iter()
            .find(|m| m.label == "scan")
            .unwrap()
            .passed,
        Some(false)
    );
    assert_eq!(
        a.tests
            .iter()
            .find(|m| m.label.starts_with("FTR 100"))
            .unwrap()
            .passed,
        Some(true)
    );
    assert_eq!(
        a.patterns
            .iter()
            .find(|m| m.label == "other")
            .unwrap()
            .passed,
        None
    );
}

#[test]
fn malformed_incomplete_and_over_budget_inputs_fail_without_replacing_output() {
    let f = Fixture::new();
    let output = f.file("report.html", b"existing report");
    let mut incomplete = start("L", None, 100);
    incomplete.extend(pir(1));
    for (name, bytes, limit) in [
        ("broken", vec![1, 2, 3], 1024 * 1024),
        ("open", incomplete, 1024 * 1024),
        ("limited", run("L", None, 100, true), 1),
    ] {
        assert!(analyze(&[f.file(name, &bytes)], &output, limit).is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"existing report");
    }
    let input = f.file("source", &run("L", None, 100, true));
    assert!(analyze(&[input.clone()], &input, 1024 * 1024).is_err());
}

#[test]
fn report_escapes_metadata_and_marks_source_less_pareto_unavailable() {
    let f = Fixture::new();
    let input = f.file(
        "in",
        &run("</script><script>alert(1)</script>", Some("W"), 100, true),
    );
    let template =
        "  <section id=\"pareto\">old</section>\n  <section id=\"commonality\"></section>";
    let html = attach(
        template.into(),
        &[input],
        &f.0.join("out"),
        16 * 1024 * 1024,
    )
    .unwrap();
    assert!(!html.contains("</script><script>alert"));
    assert!(html.contains("\\u003c/script\\u003e"));
    assert!(html.contains("lp-level"));
    let html = unavailable(template.into(), "missing source", 16 * 1024 * 1024).unwrap();
    assert!(html.contains("\"unavailable\":\"missing source\""));
}

#[test]
fn functional_test_failure_is_not_hidden_by_a_different_passing_pattern() {
    let f = Fixture::new();
    let mut b = start("L", Some("W"), 100);
    b.extend(pir(1));
    b.extend(ftr(1, 100, "pattern-a", 128));
    b.extend(ftr(1, 100, "pattern-b", 0));
    b.extend(prr(1, 1, 1, 8, 9));
    b.extend(end());
    let r = f.report(&[f.file("functional", &b)]);
    assert_eq!(r.attempts[0].tests.len(), 1);
    assert_eq!(r.attempts[0].tests[0].passed, Some(false));
    assert_eq!(
        r.attempts[0]
            .patterns
            .iter()
            .filter(|m| m.passed == Some(false))
            .count(),
        1
    );
}

#[test]
fn ambiguous_wafer_groups_are_not_merged_as_missing_wafer() {
    let f = Fixture::new();
    let mut b = start("L", Some("W1"), 100);
    let mut w = vec![1, 2];
    w.extend(100u32.to_le_bytes());
    w.extend(cn("W2"));
    b.extend(rec(2, 10, w));
    b.extend(unit(1, true));
    b.extend(end());
    let r = f.report(&[f.file("wafer-groups", &b)]);
    assert_eq!(r.attempts[0].issue, Some("ambiguous_wafer"));
    assert!(r.attempts[0].ecid.is_none());
    assert!(r.attempts[0].is_latest.is_none());
}
