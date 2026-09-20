use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zstdf-ftr-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn record(typ: u8, sub: u8, body: &[u8], be: bool) -> Vec<u8> {
    let len = body.len() as u16;
    let mut bytes = if be {
        len.to_be_bytes()
    } else {
        len.to_le_bytes()
    }
    .to_vec();
    bytes.extend([typ, sub]);
    bytes.extend(body);
    bytes
}
fn ftr(pattern: Option<&str>, flag: u8, site: u8, be: bool) -> Vec<u8> {
    let mut body = if be {
        42u32.to_be_bytes()
    } else {
        42u32.to_le_bytes()
    }
    .to_vec();
    body.extend([1, site, flag]);
    if let Some(pattern) = pattern {
        body.push(0xc0);
        body.extend([0; 30]); // Counters, addresses, offset, two zero array counts.
        body.extend([0, 0]); // FAIL_PIN D*n has zero bits.
        body.push(pattern.len() as u8);
        body.extend(pattern.as_bytes());
    }
    record(15, 20, &body, be)
}
fn sample(be: bool) -> Vec<u8> {
    let mut bytes = record(0, 10, &[if be { 1 } else { 2 }, 4], be);
    for (pattern, flag, site) in [
        (Some("SCAN_A"), 0, 1),
        (Some("SCAN_A"), 128, 2),
        (Some("SCAN_A"), 0, 1),
        (Some("SCAN_B"), 0, 1),
        (Some("SCAN_B"), 64, 1),
        (Some("SCAN_B"), 16, 1),
        (None, 128, 1),
        (Some(""), 0, 1),
    ] {
        bytes.extend(ftr(pattern, flag, site, be));
    }
    bytes
}
#[test]
fn flag_classification_excludes_unknown_and_not_executed() {
    let mut c = Counts::default();
    for flag in [0, 1, 128, 129, 2, 4, 8, 32, 64, 192, 16, 144] {
        c.push(flag);
    }
    assert_eq!(
        c,
        Counts {
            attempts: 12,
            pass: 2,
            fail: 2,
            unknown: 6,
            not_executed: 2,
            alarms: 2
        }
    );
}
#[test]
fn pattern_grouping_endianness_missing_and_multisite() {
    let f = Fixture::new();
    for be in [false, true] {
        let source = scan(&f.write("a.stdf", &sample(be)), 100).unwrap();
        assert_eq!(source.groups.len(), 4);
        let a: Vec<_> = source
            .groups
            .iter()
            .filter(|g| g.scope.pattern.as_deref() == Some("SCAN_A"))
            .collect();
        assert_eq!(a.len(), 2);
        assert_eq!(a.iter().map(|g| g.counts.attempts).sum::<u64>(), 3);
        assert_eq!(a.iter().map(|g| g.counts.fail).sum::<u64>(), 1);
        let b = source
            .groups
            .iter()
            .find(|g| g.scope.pattern.as_deref() == Some("SCAN_B"))
            .unwrap();
        assert_eq!(
            (b.counts.pass, b.counts.unknown, b.counts.not_executed),
            (1, 1, 1)
        );
        let missing = source
            .groups
            .iter()
            .find(|g| g.scope.pattern.is_none())
            .unwrap();
        assert_eq!(missing.counts.attempts, 2); // Never inherit SCAN_B.
        assert!(b.last_offset > b.first_offset);
    }
}
#[test]
fn decompressed_duplicates_deduplicate_and_order_is_stable() {
    let f = Fixture::new();
    let bytes = sample(false);
    let a = f.write("a.stdf", &bytes);
    let b = f.write("copy.std", &bytes);
    let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    gz.write_all(&bytes).unwrap();
    let c = f.write("a.stdf.gz", &gz.finish().unwrap());
    let output = f.0.join("report.html");
    let one = analyze(&[a.clone(), b.clone(), c.clone()], &output, 100).unwrap();
    let two = analyze(&[c, b, a], &output, 100).unwrap();
    assert_eq!(one.sources.len(), 1);
    assert_eq!(one.sources[0].paths.len(), 3);
    assert_eq!(
        serde_json::to_string(&one).unwrap(),
        serde_json::to_string(&two).unwrap()
    );
}
#[test]
fn names_are_exact_and_not_split_or_conflated_with_null() {
    let f = Fixture::new();
    let mut bytes = record(0, 10, &[2, 4], false);
    for p in ["A,B", "a,b", "[null]", "A,B ", ""] {
        bytes.extend(ftr(Some(p), 128, 1, false));
    }
    let s = scan(&f.write("p.stdf", &bytes), 10).unwrap();
    assert_eq!(s.groups.len(), 5);
}
#[test]
fn malformed_truncated_and_limits_preserve_existing_output() {
    let f = Fixture::new();
    let good = f.write("good.stdf", &sample(false));
    let output = f.write("report.html", b"previous report");
    for (bytes, groups, report_mib) in [
        (vec![1, 2], 10, 1),
        (sample(false), 1, 1),
        (sample(false), 10, 0),
    ] {
        let bad = f.write("bad.stdf", &bytes);
        assert!(execute(
            Arguments {
                inputs: vec![good.clone(), bad],
                output: output.clone(),
                max_groups: groups,
                max_report_mib: report_mib
            },
            &mut Vec::new()
        )
        .is_err());
        assert_eq!(std::fs::read(&output).unwrap(), b"previous report");
    }
    let mut partial = ftr(Some("ABC"), 0, 1, false);
    partial.pop();
    let len = (partial.len() - 4) as u16;
    partial[..2].copy_from_slice(&len.to_le_bytes());
    let mut bytes = record(0, 10, &[2, 4], false);
    bytes.extend(partial);
    assert!(scan(&f.write("partial.stdf", &bytes), 10).is_err());
    assert!(analyze(std::slice::from_ref(&good), &good, 10).is_err());
}
#[test]
fn no_ftr_is_explicit_and_html_evidence_is_script_safe() {
    let f = Fixture::new();
    let far = f.write("empty.stdf", &record(0, 10, &[2, 4], false));
    let r = analyze(&[far], &f.0.join("report.html"), 10).unwrap();
    assert!(r.sources[0].groups.is_empty());
    let mut bytes = record(0, 10, &[2, 4], false);
    bytes.extend(ftr(Some("</script><img src=x>"), 128, 1, false));
    let r = analyze(
        &[f.write("evil.stdf", &bytes)],
        &f.0.join("report.html"),
        10,
    )
    .unwrap();
    let html = fragment(&r).unwrap();
    assert!(!html.contains("</script><img"));
    assert!(html.contains("\\u003c/script"));
}

#[test]
fn separate_sources_and_mir_runs_keep_independent_scope() {
    let f = Fixture::new();
    fn mir(lot: &str, revision: &str) -> Vec<u8> {
        let mut body = vec![0; 15];
        for value in [lot, "CHIP", "NODE", "TESTER", "JOB", revision] {
            body.push(value.len() as u8);
            body.extend(value.as_bytes());
        }
        record(1, 10, &body, false)
    }
    let mut bytes = record(0, 10, &[2, 4], false);
    bytes.extend(mir("LOT_A", "r1"));
    bytes.extend(ftr(Some("PAT"), 128, 1, false));
    bytes.extend(mir("LOT_B", "r2"));
    bytes.extend(ftr(Some("PAT"), 0, 1, false));
    let a = f.write("a.stdf", &bytes);
    let source = scan(&a, 10).unwrap();
    assert_eq!(source.groups.len(), 2);
    assert_eq!(source.groups[0].scope.lot, "LOT_A");
    assert_eq!(source.groups[1].scope.revision, "r2");
    assert_eq!(source.groups[1].scope.run, 2);
    // Distinct content, even with overlapping records, must not be deduplicated.
    bytes.extend(ftr(Some("PAT"), 128, 1, false));
    let b = f.write("b.stdf", &bytes);
    let output = f.0.join("report.html");
    let report = analyze(&[b.clone(), a.clone()], &output, 4).unwrap();
    assert_eq!(report.sources.len(), 2);
    assert_eq!(
        report
            .sources
            .iter()
            .flat_map(|s| &s.groups)
            .map(|g| g.counts.attempts)
            .sum::<u64>(),
        5
    );
    assert!(analyze(&[a, b], &output, 3).is_err());
}

#[test]
fn serialized_report_limit_preserves_previous_report() {
    let f = Fixture::new();
    let mut bytes = record(0, 10, &[2, 4], false);
    for i in 0..3000 {
        bytes.extend(ftr(
            Some(&format!("PAT_{i:04}_{}", "A".repeat(230))),
            128,
            1,
            false,
        ));
    }
    let input = f.write("large.stdf", &bytes);
    let output = f.write("report.html", b"previous report");
    let result = execute(
        Arguments {
            inputs: vec![input],
            output: output.clone(),
            max_groups: 3000,
            max_report_mib: 1,
        },
        &mut Vec::new(),
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("report size limit"));
    assert_eq!(std::fs::read(output).unwrap(), b"previous report");
}
