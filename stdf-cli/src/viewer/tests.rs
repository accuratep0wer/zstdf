// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
use super::*;
#[test]
fn query_defaults_are_latest() {
    assert_eq!(query::Query::default().population, "latest");
}

#[test]
fn sampled_trend_keeps_source_endpoints_in_record_order() {
    let f = Fixture::new();
    let mut b = start();
    for x in 1..=2205 {
        b.extend(unit(1, x, x as f32 / 10., false));
    }
    b.extend(end());
    let c = f.cache(&b);
    let q = query::Query {
        plot: "trend".into(),
        color: "none".into(),
        ..Default::default()
    };
    let data = query::plot(&c, &q).unwrap();
    let s = data["series"].as_object().unwrap().values().next().unwrap();
    let points = s["points"].as_array().unwrap();
    assert_eq!(s["count"], 2205);
    assert_eq!(s["sampled"], true);
    assert_eq!(points.first().unwrap()["record"], 5);
    assert_eq!(points.last().unwrap()["record"], 6617);
    assert!(points
        .windows(2)
        .all(|w| w[0]["record"].as_u64() < w[1]["record"].as_u64()));
    assert!(points.len() <= data["points_limit_per_series"].as_u64().unwrap() as usize);
}

#[test]
fn scatter_reports_axis_identity_units_and_full_bounds() {
    let f = Fixture::new();
    let mut b = start();
    for x in 1..=3 {
        b.extend(pir(1));
        b.extend(ptr(1, 10, "Voltage", x as f32, 0));
        let mut current = ptr(1, 20, "Current", x as f32 / 100., 0);
        *current.last_mut().unwrap() = b'A';
        b.extend(current);
        b.extend(prr(1, x, false));
    }
    b.extend(end());
    let c = f.cache(&b);
    let q = query::Query {
        plot: "scatter".into(),
        tests: vec![
            json!(["PTR", 10, "Voltage", null]).to_string(),
            json!(["PTR", 20, "Current", null]).to_string(),
        ],
        ..Default::default()
    };
    let data = query::plot(&c, &q).unwrap();
    assert_eq!(data["x_test"], q.tests[0]);
    assert_eq!(data["y_test"], q.tests[1]);
    assert_eq!(data["x_units"], json!(["V"]));
    assert_eq!(data["y_units"], json!(["A"]));
    assert_eq!(data["x_min"], 1.);
    assert_eq!(data["x_max"], 3.);
    assert!((data["y_max"].as_f64().unwrap() - 0.03).abs() < 1e-8);
}

#[test]
fn scatter_rejects_multiple_units_on_the_same_axis() {
    let f = Fixture::new();
    let mut b = start();
    for x in 1..=2 {
        b.extend(pir(1));
        b.extend(ptr(1, 10, "Voltage", x as f32, 0));
        let mut other = ptr(1, 20, "Other", x as f32, 0);
        if x == 1 {
            *other.last_mut().unwrap() = b'A';
        }
        b.extend(other);
        b.extend(prr(1, x, false));
    }
    b.extend(end());
    let c = f.cache(&b);
    let q = query::Query {
        plot: "scatter".into(),
        tests: vec![
            json!(["PTR", 10, "Voltage", null]).to_string(),
            json!(["PTR", 20, "Other", null]).to_string(),
        ],
        ..Default::default()
    };
    let data = query::plot(&c, &q).unwrap();
    assert!(data["unavailable"]
        .as_str()
        .unwrap()
        .contains("incompatible units"));
    assert!(data.get("correlation").is_none());
    assert_eq!(data["y_units"], json!(["A", "V"]));
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let p = crate::tests::temp_path("viewer");
        fs::create_dir_all(&p).unwrap();
        Self(p)
    }
    fn args(&self, bytes: &[u8]) -> Arguments {
        let input = self.0.join("input.stdf");
        fs::write(&input, bytes).unwrap();
        Arguments {
            input,
            dataset: None,
            cache_dir: Some(self.0.join("cache")),
            flow: Some("CP".into()),
            export_html: None,
            export_scope: "selection".into(),
            export_size_mib: 20,
            memory_limit_mib: 64,
            disk_limit_mib: 128,
            cancel_file: None,
            port: 0,
            no_open: true,
        }
    }
    fn cache(&self, bytes: &[u8]) -> Cache {
        ingest::prepare(&self.args(bytes)).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn cn(s: &str) -> Vec<u8> {
    let mut v = vec![s.len() as u8];
    v.extend(s.as_bytes());
    v
}
fn rec(t: u8, s: u8, b: Vec<u8>) -> Vec<u8> {
    let mut v = (b.len() as u16).to_le_bytes().to_vec();
    v.extend([t, s]);
    v.extend(b);
    v
}
fn mir(time: u32) -> Vec<u8> {
    let mut b = vec![0; 15];
    b[4..8].copy_from_slice(&time.to_le_bytes());
    for s in ["LOT", "DEVICE", "NODE", "TESTER", "JOB"] {
        b.extend(cn(s));
    }
    rec(1, 10, b)
}
fn start() -> Vec<u8> {
    let mut b = rec(0, 10, vec![2, 4]);
    b.extend(mir(100));
    let mut w = vec![1, 1];
    w.extend(100u32.to_le_bytes());
    w.extend(cn("W1"));
    b.extend(rec(2, 10, w));
    b
}
fn pir(site: u8) -> Vec<u8> {
    rec(5, 10, vec![1, site])
}
fn ptr(site: u8, number: u32, name: &str, n: f32, flag: u8) -> Vec<u8> {
    let mut b = number.to_le_bytes().to_vec();
    b.extend([1, site, flag, 0]);
    b.extend(n.to_le_bytes());
    b.extend(cn(name));
    b.extend(cn(""));
    b.extend([0, 0, 0, 0]);
    b.extend(0f32.to_le_bytes());
    b.extend(10f32.to_le_bytes());
    b.extend(cn("V"));
    rec(15, 10, b)
}
fn prr(site: u8, x: i16, fail: bool) -> Vec<u8> {
    let mut b = vec![1, site, if fail { 8 } else { 0 }];
    for n in [1u16, if fail { 9 } else { 1 }, if fail { 90 } else { 10 }] {
        b.extend(n.to_le_bytes());
    }
    b.extend(x.to_le_bytes());
    b.extend(1i16.to_le_bytes());
    b.extend(10u32.to_le_bytes());
    b.extend(cn("SAME_PART"));
    rec(5, 20, b)
}
fn unit(site: u8, x: i16, n: f32, fail: bool) -> Vec<u8> {
    let mut b = pir(site);
    b.extend(ptr(site, 10, "Voltage", n, if fail { 128 } else { 0 }));
    b.extend(prr(site, x, fail));
    b
}
fn end() -> Vec<u8> {
    rec(1, 20, 1000u32.to_le_bytes().to_vec())
}
fn sample() -> Vec<u8> {
    let mut b = start();
    for x in 1..=4 {
        b.extend(unit(x as u8 % 2 + 1, x, x as f32, false));
    }
    b.extend(end());
    b
}
fn rows(cache: &Cache, q: query::Query) -> Vec<Row> {
    serde_json::from_value(query::rows(cache, &q).unwrap()["rows"].clone()).unwrap()
}

#[test]
fn latest_first_and_all_use_whole_attempts_and_preserve_raw_executions() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(unit(1, 1, 9., true));
    b.extend(pir(2));
    b.extend(ptr(2, 11, "Other", 2., 0));
    b.extend(prr(2, 1, false));
    b.extend(end());
    let c = f.cache(&b);
    let q = query::Query::default();
    let selected = rows(&c, q.clone());
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].number, Some(11));
    assert_eq!(selected[0].site, 2);
    let mut first = q.clone();
    first.population = "first".into();
    assert_eq!(rows(&c, first)[0].number, Some(10));
    let mut all = q.clone();
    all.population = "all".into();
    assert_eq!(rows(&c, all).len(), 2);
    let mut filtered = q.clone();
    filtered.sites = vec![1];
    assert!(rows(&c, filtered).is_empty());
    let devices = rows(
        &c,
        query::Query {
            table: "devices".into(),
            ..q
        },
    );
    assert_eq!(devices.len(), 2);
    assert_eq!(devices.iter().filter(|r| r.latest == Some(true)).count(), 1);
}
#[test]
fn interleaved_sites_and_mpr_expansion_have_exact_provenance() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(pir(1));
    b.extend(pir(2));
    b.extend(ptr(2, 10, "Voltage", 8., 0));
    let mut m = 20u32.to_le_bytes().to_vec();
    m.extend([1, 1, 0, 0]);
    m.extend(0u16.to_le_bytes());
    m.extend(2u16.to_le_bytes());
    m.extend(1f32.to_le_bytes());
    m.extend(2f32.to_le_bytes());
    m.extend(cn("Channels"));
    b.extend(rec(15, 15, m));
    b.extend(prr(2, 2, false));
    b.extend(prr(1, 1, false));
    b.extend(end());
    let c = f.cache(&b);
    let data = rows(&c, query::Query::default());
    assert_eq!(data.len(), 4);
    for r in &data {
        let e = c.evidence(r.record).unwrap();
        assert_eq!(e.attempt, r.attempt);
        let a = c.attempt(r.attempt).unwrap();
        assert_eq!(a.site, r.site);
    }
    let m: Vec<_> = data.iter().filter(|r| r.kind == "MPR").collect();
    assert_eq!(m.len(), 3);
    assert_eq!(m.iter().filter(|r| r.channel.is_some()).count(), 2);
    assert_eq!(m[1].record, m[2].record);
    assert_ne!(m[1].eav_row, m[2].eav_row);
}
#[test]
fn numeric_aggregates_are_exact_and_ignore_display_filters() {
    let f = Fixture::new();
    let c = f.cache(&sample());
    let mut q = query::Query {
        color: "none".into(),
        bins: 2,
        ..Default::default()
    };
    q.filters.insert("value".into(), ">3".into());
    assert_eq!(rows(&c, q.clone()).len(), 1);
    let p = query::plot(&c, &q).unwrap();
    let s = p["series"].as_object().unwrap().values().next().unwrap();
    assert_eq!(s["count"], 4);
    assert_eq!(s["mean"], 2.5);
    assert_eq!(s["histogram"], json!([2, 2]));
    assert_eq!(s["quantiles"][2], 2.5);
    assert!((s["sigma"].as_f64().unwrap() - (5f64 / 3.).sqrt()).abs() < 1e-12);
    assert_eq!(s["low"], 0.);
    assert_eq!(s["high"], 10.);
}
#[test]
fn measurement_exclusions_do_not_change_bins() {
    let f = Fixture::new();
    let c = f.cache(&sample());
    let mut q = query::Query {
        plot: "pareto".into(),
        show_pass: true,
        ..Default::default()
    };
    let before = query::plot(&c, &q).unwrap();
    q.excluded_measurements = vec![1, 2];
    assert_eq!(before, query::plot(&c, &q).unwrap());
    q.excluded_devices = vec![1];
    assert_eq!(query::plot(&c, &q).unwrap()["modes"][0]["count"], 3);
}
#[test]
fn repeated_tests_keep_history_but_analytics_use_last() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(pir(1));
    b.extend(ptr(1, 10, "Voltage", 1., 0));
    b.extend(ptr(1, 10, "Voltage", 7., 0));
    b.extend(prr(1, 1, false));
    b.extend(end());
    let c = f.cache(&b);
    let q = query::Query::default();
    assert_eq!(rows(&c, q.clone()).len(), 2);
    let p = query::plot(&c, &q).unwrap();
    let s = p["series"].as_object().unwrap().values().next().unwrap();
    assert_eq!(s["count"], 1);
    assert_eq!(s["mean"], 7.);
}
#[test]
fn tied_runs_and_missing_identity_stay_forensic() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(unit(1, 1, 1., false));
    b.extend(end());
    b.extend(mir(100));
    let mut w = vec![1, 1];
    w.extend(100u32.to_le_bytes());
    w.extend(cn("W1"));
    b.extend(rec(2, 10, w));
    b.extend(unit(1, 1, 2., false));
    b.extend(unit(1, 0, 3., false));
    b.extend(end());
    let c = f.cache(&b);
    assert!(rows(&c, query::Query::default()).is_empty());
    let devices = rows(
        &c,
        query::Query {
            table: "devices".into(),
            ..Default::default()
        },
    );
    assert_eq!(devices.len(), 3);
    assert!(devices.iter().all(|r| r.latest.is_none()));
}
#[test]
fn no_measurement_prr_is_retained() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(pir(1));
    b.extend(prr(1, 1, false));
    b.extend(end());
    let c = f.cache(&b);
    assert_eq!(c.manifest.attempts, 1);
    assert_eq!(c.manifest.measurements, 0);
    assert_eq!(
        rows(
            &c,
            query::Query {
                table: "devices".into(),
                ..Default::default()
            }
        )
        .len(),
        1
    );
}
#[test]
fn gzip_matches_plain_and_cache_detects_tampering() {
    let f = Fixture::new();
    let mut args = f.args(&sample());
    let plain = ingest::prepare(&args).unwrap();
    let copy = f.0.join("renamed.stdf");
    fs::copy(&args.input, &copy).unwrap();
    args.input = copy.clone();
    let reused = ingest::prepare(&args).unwrap();
    assert_eq!(plain.root, reused.root);
    assert_eq!(
        reused.manifest.source,
        copy.canonicalize().unwrap().to_string_lossy()
    );
    let gz = f.0.join("input.gz");
    let mut w =
        flate2::write::GzEncoder::new(File::create(&gz).unwrap(), flate2::Compression::default());
    w.write_all(&sample()).unwrap();
    w.finish().unwrap();
    args.input = gz;
    let compressed = ingest::prepare(&args).unwrap();
    assert_eq!(
        plain.manifest.content_hash,
        compressed.manifest.content_hash
    );
    assert_eq!(
        query::rows(&plain, &Default::default()).unwrap(),
        query::rows(&compressed, &Default::default()).unwrap()
    );
    fs::write(compressed.root.join("selection.bin"), b"tampered").unwrap();
    assert!(ingest::prepare(&args).is_err());
}
#[test]
fn malformed_cancelled_and_oversized_exports_preserve_destination() {
    let f = Fixture::new();
    let c = f.cache(&sample());
    let output = f.0.join("keep.html");
    fs::write(&output, b"original").unwrap();
    assert!(web::export(&c, &Default::default(), "selection", 100, None).is_err());
    let mut b = start();
    b.extend(pir(1));
    assert!(ingest::prepare(&f.args(&b)).is_err());
    let cancel = f.0.join("cancel");
    fs::write(&cancel, b"").unwrap();
    let mut args = f.args(&sample());
    args.cancel_file = Some(cancel);
    assert!(ingest::prepare(&args).is_err());
    assert_eq!(fs::read(output).unwrap(), b"original");
}
#[test]
fn field_defaults_retain_raw_and_inheritance() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(unit(1, 1, 1., false));
    b.extend(pir(1));
    let mut p = 10u32.to_le_bytes().to_vec();
    p.extend([1, 1, 0, 0]);
    p.extend(2f32.to_le_bytes());
    p.extend(cn("Voltage"));
    b.extend(rec(15, 10, p));
    b.extend(prr(1, 2, false));
    b.extend(end());
    let c = f.cache(&b);
    let data = rows(&c, Default::default());
    let e = c.evidence(data[1].record).unwrap();
    let low = e.fields.iter().find(|f| f.name == "LO_LIMIT").unwrap();
    assert!(low.raw.is_null());
    assert_eq!(low.origin, "inherited");
    assert_eq!(data[1].low, Some(0.));
}
#[test]
fn offline_html_escapes_source_labels_and_contains_no_api() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(pir(1));
    b.extend(ptr(1, 10, "</script><script>evil()</script>", 1., 0));
    b.extend(prr(1, 1, false));
    b.extend(end());
    let c = f.cache(&b);
    let html = web::export(&c, &Default::default(), "selection", 20 * 1024 * 1024, None).unwrap();
    assert!(!html.contains("</script><script>evil()"));
    assert!(html.contains("window.VIEWER_API=''"));
    assert!(html.contains("\"offline\":true"));
}

#[test]
fn histogram_sites_share_edges_and_probability_ties_are_exact() {
    let f = Fixture::new();
    let mut b = start();
    for (i, v) in [1., 1., 4., 4.].iter().enumerate() {
        b.extend(unit(if i < 2 { 1 } else { 2 }, i as i16 + 1, *v, false));
    }
    b.extend(end());
    let c = f.cache(&b);
    let p = query::plot(
        &c,
        &query::Query {
            bins: 2,
            ..Default::default()
        },
    )
    .unwrap();
    let series: Vec<_> = p["series"].as_object().unwrap().values().collect();
    assert_eq!(series[0]["hist_min"], series[1]["hist_min"]);
    assert_eq!(series[0]["hist_max"], series[1]["hist_max"]);
    assert_eq!(series[0]["histogram"], json!([2, 0]));
    assert_eq!(series[1]["histogram"], json!([0, 2]));
    let p = query::plot(
        &c,
        &query::Query {
            plot: "probability".into(),
            color: "none".into(),
            ..Default::default()
        },
    )
    .unwrap();
    let s = p["series"].as_object().unwrap().values().next().unwrap();
    assert_eq!(s["cdf"].as_array().unwrap().len(), 2);
    assert_eq!(s["cdf"][0]["probability"], 50.);
    assert_eq!(s["cdf"][1]["probability"], 100.);
}
#[test]
fn original_record_order_survives_reverse_site_completion() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(pir(1));
    b.extend(pir(2));
    b.extend(ptr(1, 10, "Voltage", 1., 0));
    b.extend(ptr(2, 10, "Voltage", 2., 0));
    b.extend(prr(2, 2, false));
    b.extend(prr(1, 1, false));
    b.extend(end());
    let c = f.cache(&b);
    let r = rows(&c, Default::default());
    assert_eq!(r[0].site, 1);
    assert!(r[0].record < r[1].record);
    assert!(r[0].id > r[1].id);
}
#[test]
fn column_value_filters_only_change_display() {
    let f = Fixture::new();
    let c = f.cache(&sample());
    let mut q = query::Query::default();
    q.values.insert("site".into(), vec![json!(1)]);
    assert_eq!(rows(&c, q.clone()).len(), 2);
    assert_eq!(
        query::plot(&c, &q).unwrap(),
        query::plot(&c, &Default::default()).unwrap()
    );
    let choices = query::distinct(&c, &q, "site").unwrap();
    assert_eq!(choices["values"].as_array().unwrap().len(), 2);
    q.values.insert("site".into(), vec![]);
    assert_eq!(rows(&c, q).len(), 0);
}
#[test]
fn invalid_nonfinite_and_absent_values_are_separate() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(unit(1, 1, f32::NAN, false));
    b.extend(pir(1));
    b.extend(ptr(1, 10, "Voltage", 2., 2));
    b.extend(prr(1, 2, false));
    b.extend(end());
    let c = f.cache(&b);
    let r = rows(&c, Default::default());
    assert_eq!(r[0].value_state, "nonfinite");
    assert_eq!(r[1].value_state, "invalid");
    assert_eq!(r[1].raw, Some(2.));
    assert!(r.iter().all(|r| r.value.is_none()));
    let p = query::plot(&c, &Default::default()).unwrap();
    let s = p["series"].as_object().unwrap().values().next().unwrap();
    assert_eq!(s["nonfinite"], 1);
    assert_eq!(s["invalid"], 1);
    assert_eq!(s["count"], 0);
}
#[test]
fn selection_export_keeps_prr_and_run_evidence() {
    let f = Fixture::new();
    let c = f.cache(&sample());
    let html = web::export(
        &c,
        &Default::default(),
        "selection",
        20 * 1024 * 1024,
        Some("CP"),
    )
    .unwrap();
    let data = html
        .split("id=\"viewer-boot\">")
        .nth(1)
        .unwrap()
        .split("</script>")
        .next()
        .unwrap();
    let boot: Value = serde_json::from_str(data).unwrap();
    let kinds: BTreeSet<_> = boot["evidence"]
        .as_object()
        .unwrap()
        .values()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains("PRR") && kinds.contains("MIR") && kinds.contains("WIR"));
    assert_eq!(boot["validation"]["profile"], "CP");
}
#[test]
fn matching_catalog_bytes_are_reused_with_exact_row_addresses() {
    let f = Fixture::new();
    let mut args = f.args(&sample());
    let dataset = f.0.join("dataset");
    stdf_parquet::catalog::convert_dataset(
        &[args.input.clone()],
        &dataset,
        &[stdf_parquet::PartitionKey::WaferId],
        &stdf_parquet::FragmentOptions {
            row_group_rows: 2,
            ..Default::default()
        },
        stdf_parquet::catalog::ErrorPolicy::FailFast,
    )
    .unwrap();
    args.dataset = Some(dataset.clone());
    let c = ingest::prepare(&args).unwrap();
    assert!(c
        .manifest
        .eav_files
        .iter()
        .all(|n| n.starts_with("catalog-eav-")));
    assert!(!c.root.join("measurements.parquet").exists());
    let r = rows(&c, Default::default());
    for r in r {
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
            File::open(c.root.join(r.eav_fragment.unwrap())).unwrap(),
        )
        .unwrap()
        .build()
        .unwrap();
        let mut index = 0;
        let mut found = false;
        for b in reader {
            let b = b.unwrap();
            if r.eav_row.unwrap() >= index && r.eav_row.unwrap() < index + b.num_rows() as u64 {
                let n = (r.eav_row.unwrap() - index) as usize;
                use arrow::array::Array;
                let id = b
                    .column(stdf_arrow::schema::PART_SEQUENCE)
                    .as_any()
                    .downcast_ref::<arrow::array::UInt64Array>()
                    .unwrap()
                    .value(n);
                assert_eq!(id, r.attempt);
                found = true;
            }
            index += b.num_rows() as u64;
        }
        assert!(found);
    }
    assert!(ingest::prepare(&args).is_ok());
    let catalog = stdf_parquet::catalog::verify_catalog(&dataset).unwrap();
    let fragment = &catalog.sources.values().next().unwrap().fragments[0];
    fs::write(dataset.join(&fragment.path), b"damaged").unwrap();
    assert!(ingest::prepare(&args).is_err());
}
#[test]
fn cancellation_propagates_to_spill_sort() {
    let f = Fixture::new();
    let mut args = f.args(&sample());
    let cancel = f.0.join("cancel.flag");
    args.cancel_file = Some(cancel.clone());
    let c = ingest::prepare(&args).unwrap();
    let mut sort = c.spill().unwrap();
    sort.push(b"a", b"b").unwrap();
    fs::write(cancel, b"").unwrap();
    for _ in 0..100 {
        if c.cancellation.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(sort.finish().is_err());
}

#[test]
fn attempt_index_crosses_fragments_and_annotations_cover_all_rows() {
    let f = Fixture::new();
    let mut b = start();
    b.extend(pir(1));
    for _ in 0..2205 {
        b.extend(ptr(1, 10, "Voltage", 1., 128));
    }
    b.extend(prr(1, 1, true));
    b.extend(unit(2, 2, 2., false));
    b.extend(end());
    let c = f.cache(&b);
    let paths: BTreeSet<String> = store::lookup(&c.root, "measurement_lookup", 1).unwrap();
    assert!(paths.len() > 1);
    let q = query::Query {
        attempts: vec![1],
        offset: 2200,
        limit: 5,
        ..Default::default()
    };
    let data = query::rows(&c, &q).unwrap();
    assert_eq!(data["total"], 2205);
    assert_eq!(data["annotation_count"], 2205);
    assert_eq!(data["annotations"].as_array().unwrap().len(), 2000);
    assert_eq!(data["next_annotation"], 2201);
    assert_eq!(data["previous_annotation"], 2199);
    assert_eq!(data["rows"].as_array().unwrap().len(), 5);
    let q = query::Query {
        attempts: vec![2],
        ..Default::default()
    };
    assert_eq!(rows(&c, q).len(), 1);
}

#[test]
fn http_parser_waits_for_split_request_on_nonblocking_listener() {
    use std::net::{TcpListener, TcpStream};
    let f = Fixture::new();
    let args = f.args(&sample());
    let c = ingest::prepare(&args).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let host = listener.local_addr().unwrap().to_string();
    listener.set_nonblocking(true).unwrap();
    let host2 = host.clone();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = loop {
            match listener.accept() {
                Ok(v) => break v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(1))
                }
                Err(e) => panic!("{e}"),
            }
        };
        web::handle(&mut stream, &c, &args, "/test/", &host2)
            .map_err(|e| e.to_string())
            .unwrap();
    });
    let mut stream = TcpStream::connect(&host).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    write!(
        stream,
        "POST /test/rows HTTP/1.1\r\nHost: {host}\r\nContent-Length: 2\r\n\r\n"
    )
    .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(40));
    stream.write_all(b"{}").unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    assert!(response.starts_with("HTTP/1.1 200"));
    server.join().unwrap();
}
#[test]
fn cancelled_atomic_publish_keeps_existing_report() {
    let f = Fixture::new();
    let path = f.0.join("report.html");
    fs::write(&path, b"original").unwrap();
    assert!(
        store::atomic_write_checked(&path, b"new", || Err(std::io::Error::other("cancelled")))
            .is_err()
    );
    assert_eq!(fs::read(path).unwrap(), b"original");
}

#[test]
fn paging_reuses_disk_query_index_and_cleans_its_own_scratch() {
    let f = Fixture::new();
    let c = f.cache(&sample());
    let root = c.root.clone();
    let q = query::Query {
        limit: 2,
        ..Default::default()
    };
    assert_eq!(rows(&c, q.clone()).len(), 2);
    let mut page = q;
    page.offset = 2;
    assert_eq!(rows(&c, page)[0].value, Some(3.));
    assert_eq!(c.query_pages.borrow().len(), 1);
    drop(c);
    assert!(!fs::read_dir(root).unwrap().any(|p| p
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".viewer-query-")));
}

#[test]
fn bin_descriptions_preserve_run_site_and_number() {
    let f = Fixture::new();
    let mut b = start();
    for (sub, site, number, name) in [(40, 1, 1u16, "Good"), (50, 255, 10, "Functional pass")] {
        let mut body = vec![1, site];
        body.extend(number.to_le_bytes());
        body.extend(1u32.to_le_bytes());
        body.push(b'P');
        body.extend(cn(name));
        b.extend(rec(1, sub, body));
    }
    b.extend(unit(1, 1, 1., false));
    b.extend(end());
    let c = f.cache(&b);
    assert_eq!(c.manifest.bin_definitions.len(), 2);
    assert_eq!(
        c.manifest.bin_definitions[0],
        json!({"level":"hard_bin","run":1,"head":1,"site":1,"number":1,"name":"Good"})
    );
    assert_eq!(c.manifest.bin_definitions[1]["site"], 255);
    assert_eq!(c.manifest.bin_definitions[1]["name"], "Functional pass");
}
