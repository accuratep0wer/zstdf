use super::*;
use stdf_parquet::catalog::{atomic_write, verified_path, verify_catalog};

pub struct DatasetLimits {
    pub max_memory_bytes: usize,
    pub max_lots: usize,
    pub max_parts: usize,
}

pub fn generate_dataset_dashboard(
    root: &Path,
    output: &Path,
    options: DashboardOptions,
    limits: DatasetLimits,
) -> Result<DashboardSummary, Box<dyn Error>> {
    let canonical_root = std::fs::canonicalize(root)?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let destination = std::fs::canonicalize(parent)?.join(
        output
            .file_name()
            .ok_or_else(|| invalid_data("missing output filename"))?,
    );
    if destination.starts_with(canonical_root.join("objects"))
        || destination == canonical_root.join("_catalog.json")
        || destination == canonical_root.join(".catalog.guard")
    {
        return Err(invalid_data(
            "dashboard output must not overwrite dataset metadata or objects",
        )
        .into());
    }
    if limits.max_memory_bytes < 1024 * 1024 || limits.max_lots == 0 || limits.max_parts == 0 {
        return Err(invalid_data(
            "dashboard limits must be positive; memory must be at least 1 MiB",
        )
        .into());
    }
    // Reserve for parsed catalog, reader scratch, aggregate copies, and HTML serialization.
    let catalog_bytes = std::fs::metadata(root.join("_catalog.json"))?.len();
    let mut charged = usize::try_from(catalog_bytes)?
        .saturating_mul(8)
        .saturating_add(1024 * 1024);
    if charged > limits.max_memory_bytes {
        return Err(invalid_data("dashboard metadata exceeds memory budget").into());
    }
    let catalog = verify_catalog(root)?;
    let mut all = AnalysisAccumulator::default();
    let mut lots: BTreeMap<String, AnalysisAccumulator> = BTreeMap::new();
    for (source_id, source) in &catalog.sources {
        for fragment in &source.fragments {
            let path = verified_path(root, &fragment.path)?;
            let reader = ParquetRecordBatchReaderBuilder::try_new(File::open(path)?)?
                .with_batch_size(256)
                .build()?;
            for batch in reader {
                let batch = batch?;
                if batch.get_array_memory_size() > limits.max_memory_bytes / 8 {
                    return Err(invalid_data("dashboard Arrow batch exceeds memory budget").into());
                }
                for row in rows_from_batch(&batch)? {
                    let string_bytes = row.lot_id.len()
                        + row.wafer_id.as_ref().map_or(0, String::len)
                        + row.part_id.len()
                        + row.test_txt.as_ref().map_or(0, String::len)
                        + row.test_type.len()
                        + row.units.as_ref().map_or(0, String::len)
                        + row.part_merge_key.as_ref().map_or(0, String::len);
                    // Cumulative conservative reservation bounds both all-lot and per-lot maps.
                    charged = charged
                        .saturating_add(8192)
                        .saturating_add(string_bytes.saturating_mul(32));
                    if charged > limits.max_memory_bytes {
                        return Err(invalid_data("dashboard analysis exceeds memory budget; use a smaller dataset or raise --memory-limit-mib").into());
                    }
                    if !lots.contains_key(&row.lot_id) && lots.len() >= limits.max_lots {
                        return Err(invalid_data("dashboard lot limit exceeded").into());
                    }
                    if all.parts.len() >= limits.max_parts
                        && !all.parts.contains_key(&scoped_part_key(&row, source_id))
                    {
                        return Err(invalid_data("dashboard part limit exceeded").into());
                    }
                    all.push_scoped(&row, source_id);
                    lots.entry(row.lot_id.clone())
                        .or_default()
                        .push_scoped(&row, source_id);
                }
            }
        }
    }
    let data = all.finish(&options);
    let summary = DashboardSummary {
        rows: data.kpis.rows,
        parts: data.kpis.parts,
        yield_percent: data.kpis.yield_percent,
    };
    let lots: Vec<serde_json::Value> = lots
        .into_iter()
        .map(|(label, accumulator)| {
            let data: serde_json::Value =
                serde_json::from_str(&dashboard_json(&accumulator.finish(&options)))
                    .expect("dashboard JSON");
            serde_json::json!({"label":label, "data":data})
        })
        .collect();
    let all: serde_json::Value = serde_json::from_str(&dashboard_json(&data))?;
    let payload = serde_json::json!({"all":all, "lots":lots, "run_status":catalog.run.status, "revision":catalog.revision});
    let json = serde_json::to_string(&payload)?
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026");
    let html = DASHBOARD_HTML
        .replace("__TITLE__", &html_escape(&options.title))
        .replace("__DATA__", &json);
    let html = crate::ftr_pareto::attach(html, &options.ftr_inputs, output)?;
    let html = crate::report_ui::decorate(html);
    if html.len() > limits.max_memory_bytes / 2 {
        return Err(invalid_data("dashboard HTML exceeds memory budget").into());
    }
    atomic_write(output, html.as_bytes())?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use stdf_parquet::{
        catalog::{convert_dataset, ErrorPolicy},
        FragmentOptions,
    };

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = crate::tests::temp_path("dashboard_catalog");
            fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn input(&self, name: &str, lot: &str, rows: usize) -> PathBuf {
            let mut bytes = vec![2, 0, 0, 10, 2, 4];
            let mut mir = vec![0; 15];
            mir.push(lot.len() as u8);
            mir.extend_from_slice(lot.as_bytes());
            mir.extend_from_slice(&[0, 0, 0, 0]);
            append(&mut bytes, 1, 10, &mir);
            append(&mut bytes, 5, 10, &[1, 0]);
            for n in 0..rows {
                let mut ptr = (n as u32).to_le_bytes().to_vec();
                ptr.extend_from_slice(&[1, 0, 0, 0]);
                ptr.extend_from_slice(&(n as f32).to_le_bytes());
                append(&mut bytes, 15, 10, &ptr);
            }
            append(&mut bytes, 5, 20, &[1, 0, 0, 1, 0, 1, 0, 1, 0]);
            let path = self.0.join(name);
            fs::write(&path, bytes).unwrap();
            path
        }
        fn convert(&self, paths: &[PathBuf]) {
            convert_dataset(
                paths,
                &self.0.join("dataset"),
                &[],
                &FragmentOptions {
                    row_group_rows: 1,
                    ..Default::default()
                },
                ErrorPolicy::FailFast,
            )
            .unwrap();
        }
        fn coordinate_input(
            &self,
            name: &str,
            lot: &str,
            wafer: &str,
            site: u8,
            prr_xy: (i16, i16),
            samples: &[(f32, f32)],
        ) -> PathBuf {
            let path = self.input(name, lot, 0);
            let mut bytes = fs::read(&path).unwrap();
            // Keep FAR/MIR only; replace the helper's empty PIR/PRR.
            bytes.truncate(bytes.len() - 6 - 13);
            let mut wir = vec![1, 255, 0, 0, 0, 0, wafer.len() as u8];
            wir.extend_from_slice(wafer.as_bytes());
            append(&mut bytes, 2, 10, &wir);
            for &(x, y) in samples {
                append(&mut bytes, 5, 10, &[1, site]);
                // The regular measurement precedes X/Y. With one row per
                // fragment it must still carry the eventual resolved identity.
                for (number, name, value) in [
                    (1_u32, "Voltage", 1.0),
                    (2, "Y_INDEX", y),
                    (3, "coordinate_x", x),
                ] {
                    let mut ptr = number.to_le_bytes().to_vec();
                    ptr.extend_from_slice(&[1, site, 0, 0]);
                    ptr.extend_from_slice(&value.to_le_bytes());
                    ptr.push(name.len() as u8);
                    ptr.extend_from_slice(name.as_bytes());
                    append(&mut bytes, 15, 10, &ptr);
                }
                let mut prr = vec![1, site, 0, 3, 0, 1, 0, 1, 0];
                prr.extend_from_slice(&prr_xy.0.to_le_bytes());
                prr.extend_from_slice(&prr_xy.1.to_le_bytes());
                prr.extend_from_slice(&0_u32.to_le_bytes());
                prr.extend_from_slice(&[4, b'S', b'A', b'M', b'E']);
                append(&mut bytes, 5, 20, &prr);
            }
            fs::write(&path, bytes).unwrap();
            path
        }
        fn dashboard(&self, limits: DatasetLimits) -> Result<DashboardSummary, Box<dyn Error>> {
            generate_dataset_dashboard(
                &self.0.join("dataset"),
                &self.0.join("out.html"),
                DashboardOptions::default(),
                limits,
            )
        }
        fn payload(&self) -> serde_json::Value {
            let html = fs::read_to_string(self.0.join("out.html")).unwrap();
            let json = html
                .split("<script id=\"dashboard-data\" type=\"application/json\">")
                .nth(1)
                .unwrap()
                .split("</script>")
                .next()
                .unwrap();
            serde_json::from_str(json).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }
    fn append(bytes: &mut Vec<u8>, typ: u8, sub: u8, payload: &[u8]) {
        bytes.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        bytes.extend_from_slice(&[typ, sub]);
        bytes.extend_from_slice(payload);
    }
    fn limits() -> DatasetLimits {
        DatasetLimits {
            max_memory_bytes: 32 * 1024 * 1024,
            max_lots: 32,
            max_parts: 1000,
        }
    }

    #[test]
    fn dashboard_cannot_overwrite_its_catalog_or_fragments() {
        let f = Fixture::new();
        f.convert(&[f.input("one.stdf", "L1", 2)]);
        let root = f.0.join("dataset");
        let catalog = verify_catalog(&root).unwrap();
        for output in [
            root.join("_catalog.json"),
            root.join(".catalog.guard"),
            root.join(&catalog.sources.values().next().unwrap().fragments[0].path),
        ] {
            let old = fs::read(&output).unwrap();
            assert!(generate_dataset_dashboard(
                &root,
                &output,
                DashboardOptions::default(),
                limits()
            )
            .is_err());
            assert_eq!(fs::read(output).unwrap(), old);
        }
        verify_catalog(&root).unwrap();
    }

    #[test]
    fn lot_snapshots_merge_fragments_but_not_sources_or_old_versions() {
        let f = Fixture::new();
        let first = f.input("one.stdf", "L1", 2);
        let second = f.input("two.stdf", "L2", 2);
        f.convert(&[first, second]);
        let summary = f.dashboard(limits()).unwrap();
        assert_eq!((summary.rows, summary.parts), (4, 2));
        let payload = f.payload();
        assert_eq!(payload["lots"].as_array().unwrap().len(), 2);
        for lot in payload["lots"].as_array().unwrap() {
            assert_eq!(lot["data"]["kpis"]["rows"], 2);
            assert_eq!(lot["data"]["kpis"]["parts"], 1);
        }
        let first = f.input("one.stdf", "L1", 3);
        f.convert(&[first]);
        let summary = f.dashboard(limits()).unwrap();
        assert_eq!((summary.rows, summary.parts), (5, 2));
    }
    #[test]
    fn corrupted_or_missing_fragment_never_replaces_existing_html() {
        for missing in [false, true] {
            let f = Fixture::new();
            f.convert(&[f.input("one.stdf", "L1", 2)]);
            f.dashboard(limits()).unwrap();
            let old = fs::read(f.0.join("out.html")).unwrap();
            let root = f.0.join("dataset");
            let catalog = verify_catalog(&root).unwrap();
            let path = root.join(&catalog.sources.values().next().unwrap().fragments[0].path);
            if missing {
                fs::remove_file(path).unwrap();
            } else {
                let mut bytes = fs::read(&path).unwrap();
                bytes[8] ^= 1;
                fs::write(path, bytes).unwrap();
            }
            assert!(f.dashboard(limits()).is_err());
            assert_eq!(fs::read(f.0.join("out.html")).unwrap(), old);
        }
    }
    #[test]
    fn memory_lot_and_part_limits_leave_previous_html_unchanged() {
        let f = Fixture::new();
        f.convert(&[f.input("one.stdf", "L1", 2), f.input("two.stdf", "L2", 2)]);
        fs::write(f.0.join("out.html"), b"previous").unwrap();
        for bound in [
            DatasetLimits {
                max_memory_bytes: 1024 * 1024,
                ..limits()
            },
            DatasetLimits {
                max_lots: 1,
                ..limits()
            },
            DatasetLimits {
                max_parts: 1,
                ..limits()
            },
        ] {
            assert!(f.dashboard(bound).is_err());
            assert_eq!(fs::read(f.0.join("out.html")).unwrap(), b"previous");
        }
    }
    #[test]
    fn partial_runs_are_visible_and_lot_labels_are_script_safe() {
        let f = Fixture::new();
        let path = f.input("one.stdf", "</script><script>alert(1)</script>", 2);
        f.convert(&[path.clone()]);
        fs::write(&path, b"broken").unwrap();
        convert_dataset(
            &[path],
            &f.0.join("dataset"),
            &[],
            &FragmentOptions::default(),
            ErrorPolicy::Continue,
        )
        .unwrap();
        assert_eq!(f.dashboard(limits()).unwrap().rows, 2);
        assert_eq!(f.payload()["run_status"], "partial");
        assert!(!fs::read_to_string(f.0.join("out.html"))
            .unwrap()
            .contains("<script>alert(1)"));
    }

    #[test]
    fn ptr_fallback_survives_fragment_splits_repeated_ids_and_source_site_changes() {
        let f = Fixture::new();
        let samples = [(10.0, 20.0), (11.0, 20.0)];
        let first = f.coordinate_input("a.stdf", "L1", "bad-wafer", 0, (1, 2), &samples);
        let second = f.coordinate_input("b.stdf", "L1", "W2", 1, (0, 2), &samples);
        f.convert(&[first.clone(), second]);
        let summary = f
            .dashboard(DatasetLimits {
                max_parts: 2,
                ..limits()
            })
            .unwrap();
        assert_eq!((summary.rows, summary.parts), (12, 2));
        assert_eq!(f.payload()["all"]["identity_rule_version"], "coordinate-v1");
        let catalog = verify_catalog(&f.0.join("dataset")).unwrap();
        assert_eq!(catalog.schema_version, "eav-v2");
        assert_eq!(
            catalog
                .sources
                .values()
                .map(|s| s.fragments.len())
                .sum::<usize>(),
            12
        );

        let reader = stdf_io::StdfReader::open(&first).unwrap();
        let parquet = f.0.join("single.parquet");
        stdf_parquet::records_to_parquet_path(reader.records(), &parquet, 1).unwrap();
        let single = analyze_parquet(&parquet, &DashboardOptions::default()).unwrap();
        assert_eq!((single.kpis.rows, single.kpis.parts), (6, 2));

        // Equal PTR coordinates in a different lot must not merge.
        let third = f.coordinate_input("c.stdf", "L2", "bad-wafer", 2, (-1, 2), &samples);
        f.convert(&[third]);
        assert_eq!(f.dashboard(limits()).unwrap().parts, 4);
    }

    #[test]
    fn primary_coordinate_identity_ignores_source_lot_site_and_ptr_values() {
        let f = Fixture::new();
        let a = f.coordinate_input("a.stdf", "L1", "W1", 0, (3, 4), &[(10.0, 20.0)]);
        let b = f.coordinate_input("b.stdf", "L2", "W1", 1, (3, 4), &[(99.0, 88.0)]);
        let c = f.coordinate_input("c.stdf", "L1", "W2", 0, (3, 4), &[(10.0, 20.0)]);
        f.convert(&[a, b, c]);
        assert_eq!(f.dashboard(limits()).unwrap().parts, 2);
    }

    #[test]
    fn invalid_fallback_keeps_each_attempt_and_preserves_original_prr_fields() {
        let f = Fixture::new();
        let source = f.coordinate_input(
            "a.stdf",
            "L1",
            "bad-wafer",
            0,
            (-1, 0),
            &[(0.0, 20.0), (0.0, 20.0)],
        );
        f.convert(&[source]);
        assert_eq!(f.dashboard(limits()).unwrap().parts, 2);
        let payload = f.payload();
        let quality = payload["all"]["data_quality"]
            .as_array()
            .unwrap()
            .iter()
            .find(|q| q["label"] == "unresolved coordinate identity")
            .unwrap();
        assert_eq!(quality["fail"], 6);
        let catalog = verify_catalog(&f.0.join("dataset")).unwrap();
        for fragment in &catalog.sources.values().next().unwrap().fragments {
            let reader = ParquetRecordBatchReaderBuilder::try_new(
                File::open(f.0.join("dataset").join(&fragment.path)).unwrap(),
            )
            .unwrap()
            .build()
            .unwrap();
            for batch in reader {
                for row in rows_from_batch(&batch.unwrap()).unwrap() {
                    assert_eq!(row.wafer_id.as_deref(), Some("bad-wafer"));
                    assert_eq!((row.x_coord, row.y_coord), (Some(-1), Some(0)));
                    assert!(row.part_merge_key.is_none());
                }
            }
        }
    }

    #[test]
    fn empty_legacy_parquet_requires_reconversion_and_preserves_html() {
        let f = Fixture::new();
        let path = f.0.join("legacy.parquet");
        let schema = std::sync::Arc::new(arrow::datatypes::Schema::new(
            stdf_arrow::eav_schema().fields()[..19].to_vec(),
        ));
        parquet::arrow::ArrowWriter::try_new(File::create(&path).unwrap(), schema, None)
            .unwrap()
            .close()
            .unwrap();
        let original = fs::read(&path).unwrap();
        let output = f.0.join("out.html");
        fs::write(&output, "previous").unwrap();
        let error = generate_dashboard(&path, &output, DashboardOptions::default()).unwrap_err();
        assert!(error.to_string().contains("reconvert"));
        assert_eq!(fs::read_to_string(output).unwrap(), "previous");
        assert_eq!(fs::read(path).unwrap(), original);
    }

    #[test]
    fn legacy_catalog_is_rejected_without_changing_catalog_or_html() {
        let f = Fixture::new();
        let source = f.input("a.stdf", "L1", 1);
        f.convert(&[source.clone()]);
        let path = f.0.join("dataset/_catalog.json");
        let legacy = fs::read_to_string(&path)
            .unwrap()
            .replace("eav-v2", "eav-v1");
        fs::write(&path, &legacy).unwrap();
        fs::write(f.0.join("out.html"), "previous").unwrap();
        let error = f.dashboard(limits()).unwrap_err().to_string();
        assert!(error.contains("reconvert"));
        assert!(convert_dataset(
            &[source],
            &f.0.join("dataset"),
            &[],
            &FragmentOptions::default(),
            ErrorPolicy::FailFast
        )
        .is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), legacy);
        assert_eq!(
            fs::read_to_string(f.0.join("out.html")).unwrap(),
            "previous"
        );
    }
}
