// Copyright 2026 zstdf contributors
// SPDX-License-Identifier: Apache-2.0

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::errors::ParquetError;
use stdf_core::{StdfError, StdfRecord};
use thiserror::Error;

pub mod catalog;
mod dataset;
pub use dataset::fragments::{files_to_partitioned_fragments, FragmentOptions, FragmentSummary};
pub use dataset::{
    files_to_partitioned_parquet_dir, FileConversionSummary, MultiFileConversionSummary,
    PartitionKey,
};

#[derive(Debug, Error)]
pub enum StdfParquetError {
    #[error(transparent)]
    Stdf(#[from] StdfError),

    #[error(transparent)]
    Parquet(#[from] ParquetError),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("output already exists without a readable manifest: {0}")]
    AlreadyExists(String),
}

pub type Result<T> = std::result::Result<T, StdfParquetError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteSummary {
    pub batches: usize,
    pub rows: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtomicWriteOptions {
    pub overwrite: bool,
    pub write_manifest: bool,
    pub cleanup_orphaned_temp: bool,
}

impl Default for AtomicWriteOptions {
    fn default() -> Self {
        Self {
            overwrite: true,
            write_manifest: true,
            cleanup_orphaned_temp: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionManifest {
    pub version: u32,
    pub output_path: String,
    pub rows: usize,
    pub batches: usize,
    pub batch_size: usize,
    pub schema_version: String,
}

pub fn write_record_batches<W>(writer: W, batches: &[RecordBatch]) -> Result<WriteSummary>
where
    W: Write + Send,
{
    let schema = batches
        .first()
        .map(RecordBatch::schema)
        .unwrap_or_else(stdf_arrow::eav_schema);
    let mut parquet_writer = ArrowWriter::try_new(writer, schema, None)?;
    let mut rows = 0;

    for batch in batches {
        rows += batch.num_rows();
        parquet_writer.write(batch)?;
    }

    parquet_writer.close()?;
    Ok(WriteSummary {
        batches: batches.len(),
        rows,
    })
}

pub fn write_record_batches_to_path<P: AsRef<Path>>(
    path: P,
    batches: &[RecordBatch],
) -> Result<WriteSummary> {
    let file = File::create(path)?;
    write_record_batches(file, batches)
}

pub fn write_record_batches_to_vec(batches: &[RecordBatch]) -> Result<(Vec<u8>, WriteSummary)> {
    let mut buffer = Vec::new();
    let summary = write_record_batches(&mut buffer, batches)?;
    Ok((buffer, summary))
}

pub fn write_record_batch_iter<W, I>(writer: W, batches: I) -> Result<WriteSummary>
where
    W: Write + Send,
    I: IntoIterator<Item = std::result::Result<RecordBatch, StdfError>>,
{
    let mut parquet_writer = ArrowWriter::try_new(writer, stdf_arrow::eav_schema(), None)?;
    let mut rows = 0;
    let mut batch_count = 0;

    for batch in batches {
        let batch = batch?;
        rows += batch.num_rows();
        batch_count += 1;
        parquet_writer.write(&batch)?;
    }

    parquet_writer.close()?;
    Ok(WriteSummary {
        batches: batch_count,
        rows,
    })
}

pub fn records_to_parquet_path<P>(
    records: impl IntoIterator<Item = std::result::Result<StdfRecord, StdfError>>,
    path: P,
    batch_size: usize,
) -> Result<WriteSummary>
where
    P: AsRef<Path>,
{
    records_to_parquet_path_atomic(records, path, batch_size, &AtomicWriteOptions::default())
}

pub fn records_to_parquet_path_atomic<P>(
    records: impl IntoIterator<Item = std::result::Result<StdfRecord, StdfError>>,
    path: P,
    batch_size: usize,
    options: &AtomicWriteOptions,
) -> Result<WriteSummary>
where
    P: AsRef<Path>,
{
    let path = path.as_ref();
    let manifest_path = manifest_path(path);

    if path.exists() && !options.overwrite {
        if let Some(summary) = read_manifest_summary(&manifest_path)? {
            return Ok(summary);
        }
        return Err(StdfParquetError::AlreadyExists(path.display().to_string()));
    }

    if options.cleanup_orphaned_temp {
        cleanup_orphaned_temp_files(path)?;
    }

    let tmp_path = temp_output_path(path);
    let mut tmp_file = File::create(&tmp_path)?;
    let batches = stdf_arrow::record_batches(records, batch_size);
    let write_result = write_record_batch_iter(&mut tmp_file, batches);
    match write_result {
        Ok(summary) => {
            tmp_file.sync_all()?;
            drop(tmp_file);
            commit_temp_file(&tmp_path, path, options.overwrite)?;
            if options.write_manifest {
                let manifest = ConversionManifest {
                    version: 1,
                    output_path: path.display().to_string(),
                    rows: summary.rows,
                    batches: summary.batches,
                    batch_size: batch_size.max(1),
                    schema_version: stdf_arrow::schema::SCHEMA_VERSION.to_string(),
                };
                write_manifest_atomic(&manifest_path, &manifest)?;
            }
            Ok(summary)
        }
        Err(error) => {
            drop(tmp_file);
            fs::remove_file(&tmp_path).ok();
            Err(error)
        }
    }
}

pub fn manifest_path(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".json");
    PathBuf::from(os)
}

fn commit_temp_file(tmp_path: &Path, output_path: &Path, overwrite: bool) -> Result<()> {
    if output_path.exists() {
        if overwrite {
            fs::remove_file(output_path)?;
        } else {
            return Err(StdfParquetError::AlreadyExists(
                output_path.display().to_string(),
            ));
        }
    }
    fs::rename(tmp_path, output_path)?;
    Ok(())
}

fn write_manifest_atomic(path: &Path, manifest: &ConversionManifest) -> Result<()> {
    let tmp_path = temp_output_path(path);
    {
        let mut file = File::create(&tmp_path)?;
        file.write_all(manifest.to_json().as_bytes())?;
        file.sync_all()?;
    }
    commit_temp_file(&tmp_path, path, true)
}

fn read_manifest_summary(path: &Path) -> Result<Option<WriteSummary>> {
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(path)?;
    let manifest: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    if manifest["schema_version"].as_str() != Some(stdf_arrow::schema::SCHEMA_VERSION) {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData,
            "existing output uses an incompatible schema; reconvert without --no-overwrite or use a new output path").into());
    }
    let rows = find_json_usize(&text, "rows");
    let batches = find_json_usize(&text, "batches");
    Ok(rows
        .zip(batches)
        .map(|(rows, batches)| WriteSummary { rows, batches }))
}

fn find_json_usize(text: &str, field: &str) -> Option<usize> {
    let needle = format!("\"{field}\":");
    let start = text.find(&needle)? + needle.len();
    let digits: String = text[start..]
        .chars()
        .skip_while(|ch| ch.is_whitespace())
        .take_while(|ch| ch.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn cleanup_orphaned_temp_files(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return Ok(());
    };
    let prefix = format!("{file_name}.tmp.");
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        let entry_path = entry.path();
        if entry_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(&prefix))
        {
            fs::remove_file(entry_path).ok();
        }
    }
    Ok(())
}

fn temp_output_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("output.parquet");
    parent.join(format!(
        "{file_name}.tmp.{}.{}",
        std::process::id(),
        monotonic_nanos()
    ))
}

fn monotonic_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

impl ConversionManifest {
    fn to_json(&self) -> String {
        format!(
            concat!(
                "{{\n",
                "  \"version\": {},\n",
                "  \"output_path\": \"{}\",\n",
                "  \"rows\": {},\n",
                "  \"batches\": {},\n",
                "  \"batch_size\": {},\n",
                "  \"schema_version\": \"{}\"\n",
                "}}\n"
            ),
            self.version,
            escape_json(&self.output_path),
            self.rows,
            self.batches,
            self.batch_size,
            escape_json(&self.schema_version)
        )
    }
}

fn escape_json(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use arrow::array::{Array, Float32Array, StringArray, UInt32Array};
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    use stdf_core::records::{Mir, Pir, Prr, Ptr, Wir};

    use super::*;

    #[test]
    fn writes_eav_batches_to_parquet_and_reads_them_back() {
        let path = temp_path("eav_roundtrip.parquet");
        let batches =
            stdf_arrow::records_to_batches(test_records().into_iter().map(Ok), 10).unwrap();

        let summary = write_record_batches_to_path(&path, &batches).unwrap();

        assert_eq!(
            summary,
            WriteSummary {
                batches: 1,
                rows: 2,
            }
        );

        let read_batch = read_single_batch(&path);
        assert_eq!(read_batch.num_rows(), 2);
        assert_eq!(read_batch.schema().field(0).name(), "lot_id");
        assert_eq!(as_string(&read_batch, 0).value(0), "LOT_PQ");
        assert_eq!(as_string(&read_batch, 2).value(1), "PART_1");
        assert_eq!(as_u32(&read_batch, 10).value(0), 7001);
        assert!((as_f32(&read_batch, 13).value(1) - 2.5).abs() < 0.001);

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn records_to_parquet_path_writes_from_decoded_record_iterator() {
        let path = temp_path("records_to_parquet.parquet");

        let summary =
            records_to_parquet_path(test_records().into_iter().map(Ok), &path, 1).unwrap();

        assert_eq!(summary.batches, 1);
        assert_eq!(summary.rows, 2);
        assert_eq!(read_single_batch(&path).num_rows(), 2);

        std::fs::remove_file(manifest_path(&path)).ok();
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn writes_empty_eav_parquet_file() {
        let path = temp_path("empty_eav.parquet");

        let summary = write_record_batches_to_path(&path, &[]).unwrap();

        assert_eq!(summary.rows, 0);
        assert_eq!(summary.batches, 0);
        let reader = ParquetRecordBatchReaderBuilder::try_new(File::open(&path).unwrap())
            .unwrap()
            .build()
            .unwrap();
        let batches: Vec<_> = reader.collect::<std::result::Result<Vec<_>, _>>().unwrap();
        assert!(batches.is_empty());

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn write_record_batches_to_vec_produces_parquet_magic() {
        let batches =
            stdf_arrow::records_to_batches(test_records().into_iter().map(Ok), 10).unwrap();

        let (buffer, summary) = write_record_batches_to_vec(&batches).unwrap();

        assert_eq!(summary.rows, 2);
        assert!(buffer.starts_with(b"PAR1"));
        assert!(buffer.ends_with(b"PAR1"));
    }

    #[test]
    fn multiple_batches_preserve_total_rows_when_read_back() {
        let path = temp_path("multi_batch.parquet");
        let mut records = test_records();
        records.extend(test_records());
        let batches = stdf_arrow::records_to_batches(records.into_iter().map(Ok), 2).unwrap();

        let summary = write_record_batches_to_path(&path, &batches).unwrap();
        let reader = ParquetRecordBatchReaderBuilder::try_new(File::open(&path).unwrap())
            .unwrap()
            .build()
            .unwrap();
        let rows: usize = reader
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap()
            .iter()
            .map(RecordBatch::num_rows)
            .sum();

        assert_eq!(summary.batches, 2);
        assert_eq!(summary.rows, 4);
        assert_eq!(rows, 4);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn records_to_parquet_path_propagates_decode_error() {
        let path = temp_path("decode_error.parquet");

        let err = records_to_parquet_path(vec![Err(StdfError::UnsupportedVersion(3))], &path, 10)
            .unwrap_err();

        assert!(matches!(
            err,
            StdfParquetError::Stdf(StdfError::UnsupportedVersion(3))
        ));
        assert!(!path.exists());
        assert!(!manifest_path(&path).exists());
    }

    #[test]
    fn write_to_directory_path_returns_io_error() {
        let dir = temp_path("as_dir");
        std::fs::create_dir_all(&dir).unwrap();

        let err = write_record_batches_to_path(&dir, &[]).unwrap_err();

        assert!(matches!(err, StdfParquetError::Io(_)));
        std::fs::remove_dir(dir).ok();
    }

    #[test]
    fn empty_parquet_file_preserves_eav_schema() {
        let path = temp_path("empty_schema.parquet");
        write_record_batches_to_path(&path, &[]).unwrap();

        let builder = ParquetRecordBatchReaderBuilder::try_new(File::open(&path).unwrap()).unwrap();

        assert_eq!(builder.schema().fields().len(), 21);
        assert_eq!(builder.schema().field(0).name(), "lot_id");
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn zero_batch_size_records_path_still_writes_rows() {
        let path = temp_path("zero_batch.parquet");

        let summary =
            records_to_parquet_path(test_records().into_iter().map(Ok), &path, 0).unwrap();

        assert_eq!(summary.rows, 2);
        assert_eq!(read_single_batch(&path).num_rows(), 2);
        std::fs::remove_file(manifest_path(&path)).ok();
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn atomic_records_writer_creates_manifest() {
        let path = temp_path("atomic_manifest.parquet");

        let summary = records_to_parquet_path_atomic(
            test_records().into_iter().map(Ok),
            &path,
            1,
            &AtomicWriteOptions::default(),
        )
        .unwrap();
        let manifest = std::fs::read_to_string(manifest_path(&path)).unwrap();

        assert_eq!(summary.rows, 2);
        assert!(path.exists());
        assert!(manifest.contains("\"rows\": 2"));
        assert!(manifest.contains("\"batches\": 1"));
        assert!(manifest.contains("\"schema_version\": \"eav-v2\""));

        std::fs::remove_file(manifest_path(&path)).ok();
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn atomic_records_writer_is_idempotent_when_output_and_manifest_exist() {
        let path = temp_path("idempotent.parquet");
        let summary = records_to_parquet_path_atomic(
            test_records().into_iter().map(Ok),
            &path,
            10,
            &AtomicWriteOptions::default(),
        )
        .unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();

        let repeat = records_to_parquet_path_atomic(
            vec![Err(StdfError::UnsupportedVersion(3))],
            &path,
            10,
            &AtomicWriteOptions {
                overwrite: false,
                write_manifest: true,
                cleanup_orphaned_temp: true,
            },
        )
        .unwrap();

        assert_eq!(repeat, summary);
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            modified
        );
        std::fs::remove_file(manifest_path(&path)).ok();
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn no_overwrite_rejects_legacy_manifest_without_modifying_existing_files() {
        let path = temp_path("legacy_manifest.parquet");
        records_to_parquet_path_atomic(
            test_records().into_iter().map(Ok),
            &path,
            10,
            &AtomicWriteOptions::default(),
        )
        .unwrap();
        let original = std::fs::read(&path).unwrap();
        let manifest = manifest_path(&path);
        let legacy = std::fs::read_to_string(&manifest)
            .unwrap()
            .replace("eav-v2", "eav-v1");
        std::fs::write(&manifest, &legacy).unwrap();
        let error = records_to_parquet_path_atomic(
            std::iter::empty::<std::result::Result<StdfRecord, StdfError>>(),
            &path,
            10,
            &AtomicWriteOptions {
                overwrite: false,
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("reconvert"));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        assert_eq!(std::fs::read_to_string(&manifest).unwrap(), legacy);
        assert_eq!(matching_temp_files(&path), 0);
        std::fs::remove_file(manifest).unwrap();
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn atomic_records_writer_removes_temp_file_on_decode_error() {
        let path = temp_path("decode_error_atomic.parquet");

        let result = records_to_parquet_path_atomic(
            vec![Err(StdfError::UnsupportedVersion(3))],
            &path,
            10,
            &AtomicWriteOptions::default(),
        );

        assert!(result.is_err());
        assert!(!path.exists());
        assert!(!manifest_path(&path).exists());
        assert_eq!(matching_temp_files(&path), 0);
    }

    #[test]
    fn cleanup_removes_orphaned_temp_files_before_retry() {
        let path = temp_path("cleanup.parquet");
        let orphan = path.with_file_name(format!(
            "{}.tmp.test-orphan",
            path.file_name().unwrap().to_string_lossy()
        ));
        std::fs::write(&orphan, b"partial").unwrap();

        records_to_parquet_path_atomic(
            test_records().into_iter().map(Ok),
            &path,
            10,
            &AtomicWriteOptions::default(),
        )
        .unwrap();

        assert!(!orphan.exists());
        assert_eq!(matching_temp_files(&path), 0);
        std::fs::remove_file(manifest_path(&path)).ok();
        std::fs::remove_file(path).ok();
    }

    fn read_single_batch(path: &Path) -> RecordBatch {
        let reader = ParquetRecordBatchReaderBuilder::try_new(File::open(path).unwrap())
            .unwrap()
            .build()
            .unwrap();
        let batches = reader.collect::<std::result::Result<Vec<_>, _>>().unwrap();
        assert_eq!(batches.len(), 1);
        batches.into_iter().next().unwrap()
    }

    fn as_string(batch: &RecordBatch, index: usize) -> &StringArray {
        batch
            .column(index)
            .as_any()
            .downcast_ref::<StringArray>()
            .unwrap()
    }

    fn as_u32(batch: &RecordBatch, index: usize) -> &UInt32Array {
        batch
            .column(index)
            .as_any()
            .downcast_ref::<UInt32Array>()
            .unwrap()
    }

    fn as_f32(batch: &RecordBatch, index: usize) -> &Float32Array {
        batch
            .column(index)
            .as_any()
            .downcast_ref::<Float32Array>()
            .unwrap()
    }

    fn temp_path(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("zstdf_{nanos}_{name}"))
    }

    fn matching_temp_files(path: &Path) -> usize {
        let parent = path.parent().unwrap();
        let prefix = format!("{}.tmp.", path.file_name().unwrap().to_string_lossy());
        std::fs::read_dir(parent)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with(&prefix))
            })
            .count()
    }

    fn test_records() -> Vec<StdfRecord> {
        vec![
            StdfRecord::Mir(mir("LOT_PQ")),
            StdfRecord::Wir(Wir {
                head_num: 1,
                site_grp: None,
                start_t: None,
                wafer_id: Some("WAFER_1".to_string()),
            }),
            StdfRecord::Pir(Pir {
                head_num: 1,
                site_num: 0,
            }),
            StdfRecord::Ptr(ptr(7001, 1.25, "VIL")),
            StdfRecord::Ptr(ptr(7002, 2.50, "VIH")),
            StdfRecord::Prr(Prr {
                head_num: 1,
                site_num: 0,
                part_flg: 0,
                num_test: 2,
                hard_bin: 1,
                soft_bin: 1,
                x_coord: Some(5),
                y_coord: Some(6),
                test_t: Some(77),
                part_id: Some("PART_1".to_string()),
                part_txt: None,
                part_fix: None,
            }),
        ]
    }

    fn mir(lot_id: &str) -> Mir {
        Mir {
            setup_t: 0,
            start_t: 0,
            stat_num: 0,
            mode_cod: b'P',
            rtst_cod: b' ',
            prot_cod: b' ',
            burn_tim: 0,
            cmod_cod: b' ',
            lot_id: lot_id.to_string(),
            part_typ: "DEVICE".to_string(),
            node_nam: "NODE".to_string(),
            tstr_typ: "T".to_string(),
            job_nam: "JOB".to_string(),
            job_rev: None,
            sblot_id: None,
            oper_nam: None,
            exec_typ: None,
            exec_ver: None,
            test_cod: None,
            tst_temp: None,
            user_txt: None,
            aux_file: None,
            pkg_typ: None,
            famly_id: None,
            date_cod: None,
            facil_id: None,
            floor_id: None,
            proc_id: None,
            oper_frq: None,
            spec_nam: None,
            spec_ver: None,
            flow_id: None,
            setup_id: None,
            dsgn_rev: None,
            eng_id: None,
            rom_cod: None,
            serl_num: None,
            supr_nam: None,
        }
    }

    fn ptr(test_num: u32, result: f32, name: &str) -> Ptr {
        Ptr {
            test_num,
            head_num: 1,
            site_num: 0,
            test_flg: 0,
            parm_flg: 0,
            result,
            test_txt: Some(name.to_string()),
            alarm_id: None,
            opt_flag: None,
            res_scal: None,
            llm_scal: None,
            hlm_scal: None,
            lo_limit: None,
            hi_limit: None,
            units: Some("V".to_string()),
            c_resfmt: None,
            c_llmfmt: None,
            c_hlmfmt: None,
            lo_spec: None,
            hi_spec: None,
        }
    }
}
