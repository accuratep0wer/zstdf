use super::*;
use arrow::record_batch::RecordBatch;
use parquet::arrow::{arrow_reader::ParquetRecordBatchReaderBuilder, ArrowWriter};
use parquet::file::properties::WriterProperties;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::Component;
use stdf_arrow::{bounded_record_batches, BatchLimits};

#[derive(Debug, Clone)]
pub struct FragmentOptions {
    pub max_memory_bytes: usize,
    pub max_pending_tests: usize,
    pub max_open_writers: usize,
    pub row_group_rows: usize,
    pub max_output_files: usize,
}
impl Default for FragmentOptions {
    fn default() -> Self {
        Self {
            max_memory_bytes: 256 * 1024 * 1024,
            max_pending_tests: 100_000,
            max_open_writers: 4,
            row_group_rows: 65_536,
            max_output_files: 10_000,
        }
    }
}
impl FragmentOptions {
    pub(crate) fn budget(&self) -> Result<usize> {
        if self.max_pending_tests == 0
            || self.max_open_writers == 0
            || self.row_group_rows == 0
            || self.max_output_files == 0
        {
            return Err(invalid("fragment limits must be positive"));
        }
        // Reserve summary/receipt paths plus input-plan metadata, independent of row data.
        let metadata = self
            .max_output_files
            .checked_mul(8192)
            .ok_or_else(|| invalid("metadata limit overflow"))?;
        let available = self.max_memory_bytes.checked_sub(metadata).ok_or_else(|| {
            invalid("memory budget cannot hold the configured output-file metadata limit")
        })?;
        let budget = available / 3;
        limit("minimum working bytes", 1024 * 1024, budget)?;
        limit(
            "writer slots",
            self.max_open_writers.saturating_mul(64 * 1024),
            budget,
        )?;
        Ok(budget)
    }
    fn batch_limits(&self, budget: usize) -> BatchLimits {
        BatchLimits {
            max_pending_tests: self.max_pending_tests,
            max_memory_bytes: budget,
        }
    }
}

#[derive(Debug, Default)]
pub struct FragmentSummary {
    pub conversion: MultiFileConversionSummary,
    pub peak_open_writers: usize,
    pub peak_writer_bytes: usize,
    pub peak_pending_bytes: usize,
}

/// Bounded PTR EAV conversion with immutable, numbered, row-partitioned fragments.
/// Publishes a directory per source only after all fragments and its receipt close.
/// Existing source generations are validated and reused; they are never overwritten.
pub fn files_to_partitioned_fragments(
    inputs: &[PathBuf],
    output_dir: &Path,
    keys: &[PartitionKey],
    options: &FragmentOptions,
) -> Result<FragmentSummary> {
    files_to_partitioned_fragments_with_limit(
        inputs,
        output_dir,
        keys,
        options,
        options.max_output_files,
    )
}

pub(crate) fn files_to_partitioned_fragments_with_limit(
    inputs: &[PathBuf],
    output_dir: &Path,
    keys: &[PartitionKey],
    options: &FragmentOptions,
    file_limit: usize,
) -> Result<FragmentSummary> {
    let budget = options.budget()?;
    limit(
        "output root path bytes",
        output_dir.as_os_str().as_encoded_bytes().len(),
        1024,
    )?;
    if inputs.is_empty() || keys.iter().collect::<BTreeSet<_>>().len() != keys.len() {
        return Err(invalid("provide input files and unique partition keys"));
    }
    limit("input files", inputs.len(), options.max_output_files)?;
    let paths = inputs
        .iter()
        .map(fs::canonicalize)
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    let config = format!(
        "fragment-v3|eav-v2|coordinate-v1|mpr-ftr-v1|{:?}|{}|{}|{}|{}|{}",
        keys,
        options.max_memory_bytes,
        options.max_pending_tests,
        options.max_open_writers,
        options.row_group_rows,
        options.max_output_files
    );
    let config_id = hash_bytes(HASH_INIT, config.as_bytes());
    let mut plan = Vec::new();
    let mut summary = FragmentSummary::default();
    for path in paths {
        limit(
            "source path bytes",
            path.as_os_str().as_encoded_bytes().len(),
            1024,
        )?;
        let (records, digest) = open_records(&path).map_err(|e| source_error(&path, e))?;
        let mut batches = bounded_record_batches(records, options.batch_limits(budget))?;
        let mut rows = 0usize;
        for batch in batches.by_ref() {
            let batch = batch.map_err(|e| source_error(&path, e.into()))?;
            partition_dir(&batch, &path, keys)?;
            rows += batch.num_rows();
        }
        summary.peak_pending_bytes = summary
            .peak_pending_bytes
            .max(batches.peak_reserved_bytes());
        let id = hash_bytes(
            hash_bytes(config_id, path.as_os_str().as_encoded_bytes()),
            &digest.get().to_le_bytes(),
        );
        plan.push((
            path,
            output_dir.join(format!("source-{id:032x}")),
            digest.get(),
            rows,
        ));
    }
    fs::create_dir_all(output_dir)?;
    let _lock = DatasetLock::acquire(output_dir)?;
    for (input, destination, digest, expected_rows) in plan {
        let remaining = file_limit.min(options.max_output_files) - summary.conversion.outputs.len();
        let outputs = if destination.exists() {
            read_receipt(&input, &destination, expected_rows, remaining)?
        } else {
            let stage_path = output_dir.join(format!(
                ".staging-{}-{}",
                std::process::id(),
                crate::monotonic_nanos()
            ));
            fs::create_dir(&stage_path)?;
            let mut stage = Stage {
                path: stage_path,
                published: false,
            };
            let mut cache = WriterCache::new(
                &stage.path,
                &destination,
                &input,
                options,
                budget,
                remaining,
            );
            let records = checked_records(&input, digest)?;
            let mut batches = bounded_record_batches(records, options.batch_limits(budget))?;
            for batch in batches.by_ref() {
                let batch = batch.map_err(|e| source_error(&input, e.into()))?;
                let partition = partition_dir(&batch, &input, keys)?;
                cache.write(partition, &batch)?;
            }
            summary.peak_pending_bytes = summary
                .peak_pending_bytes
                .max(batches.peak_reserved_bytes());
            cache.close_all()?;
            summary.peak_open_writers = summary.peak_open_writers.max(cache.peak_open);
            summary.peak_writer_bytes = summary.peak_writer_bytes.max(cache.peak_bytes);
            let mut outputs = std::mem::take(&mut cache.outputs);
            drop(cache);
            outputs.sort_by(|a, b| a.output_path.cmp(&b.output_path));
            if outputs.iter().map(|o| o.rows).sum::<usize>() != expected_rows {
                return Err(invalid("source row count changed during conversion"));
            }
            write_receipt(&stage.path, &destination, &outputs, expected_rows)?;
            fs::rename(&stage.path, &destination)?;
            stage.published = true;
            outputs
        };
        summary.conversion.files += 1;
        summary.conversion.rows += outputs.iter().map(|o| o.rows).sum::<usize>();
        summary.conversion.batches += outputs.iter().map(|o| o.batches).sum::<usize>();
        summary.conversion.outputs.extend(outputs);
    }
    Ok(summary)
}

fn source_error(path: &Path, error: crate::StdfParquetError) -> crate::StdfParquetError {
    match error {
        crate::StdfParquetError::Stdf(StdfError::ResourceLimit { .. }) => error,
        error => invalid(format!("{}: {error}", path.display())),
    }
}
fn limit(resource: &'static str, requested: usize, maximum: usize) -> Result<()> {
    if requested > maximum {
        return Err(StdfError::ResourceLimit {
            resource,
            requested,
            limit: maximum,
        }
        .into());
    }
    Ok(())
}

fn partition_dir(batch: &RecordBatch, input: &Path, keys: &[PartitionKey]) -> Result<PathBuf> {
    let mut path = PathBuf::new();
    for key in keys {
        let value = match key {
            PartitionKey::InputFile => input.file_name().and_then(|s| s.to_str()),
            key => {
                let column = if *key == PartitionKey::LotId { 0 } else { 1 };
                let values = batch
                    .column(column)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .expect("EAV strings");
                (!values.is_null(0)).then(|| values.value(0))
            }
        };
        path.push(format!("{}={}", key.name(), partition_value(value)?));
    }
    Ok(path)
}

struct OpenFragment {
    writer: ArrowWriter<File>,
    relative: PathBuf,
    rows: usize,
    batches: usize,
    tick: u64,
}
struct WriterCache<'a> {
    root: &'a Path,
    destination: &'a Path,
    input: &'a Path,
    options: &'a FragmentOptions,
    budget: usize,
    max_files: usize,
    created: usize,
    tick: u64,
    open: BTreeMap<PathBuf, OpenFragment>,
    outputs: Vec<FileConversionSummary>,
    peak_open: usize,
    peak_bytes: usize,
}
impl<'a> WriterCache<'a> {
    fn new(
        root: &'a Path,
        destination: &'a Path,
        input: &'a Path,
        options: &'a FragmentOptions,
        budget: usize,
        max_files: usize,
    ) -> Self {
        Self {
            root,
            destination,
            input,
            options,
            budget,
            max_files,
            created: 0,
            tick: 0,
            open: BTreeMap::new(),
            outputs: Vec::new(),
            peak_open: 0,
            peak_bytes: 0,
        }
    }
    fn memory(&self) -> usize {
        self.open
            .values()
            .map(|o| o.writer.memory_size().saturating_add(64 * 1024))
            .sum()
    }
    fn evict(&mut self) -> Result<()> {
        let key = self
            .open
            .iter()
            .min_by_key(|(_, value)| value.tick)
            .map(|(key, _)| key.clone())
            .ok_or_else(|| invalid("no writer to evict"))?;
        self.close(&key)
    }
    fn close(&mut self, key: &Path) -> Result<()> {
        let Some(mut item) = self.open.remove(key) else {
            return Ok(());
        };
        crate::catalog::inject_failure(3)?;
        item.writer.finish()?;
        item.writer.inner().sync_all()?;
        self.outputs.push(FileConversionSummary {
            input_path: self.input.into(),
            output_path: self.destination.join(item.relative),
            rows: item.rows,
            batches: item.batches,
        });
        Ok(())
    }
    fn close_all(&mut self) -> Result<()> {
        while !self.open.is_empty() {
            self.evict()?;
        }
        Ok(())
    }
    fn write(&mut self, key: PathBuf, batch: &RecordBatch) -> Result<()> {
        limit(
            "Arrow batch bytes",
            batch.get_array_memory_size(),
            self.budget,
        )?;
        let mut offset = 0;
        while offset < batch.num_rows() {
            // Reserve encoding/copy space before writing. Fragments hold at most one row group.
            let max_chunk = (self.budget / 16384).clamp(1, 256);
            let mut count = (batch.num_rows() - offset)
                .min(max_chunk)
                .min(self.options.row_group_rows);
            let reserve = count.saturating_mul(8192).saturating_add(64 * 1024);
            limit("writer reservation bytes", reserve, self.budget)?;
            while !self.open.is_empty() && self.memory().saturating_add(reserve) > self.budget {
                self.evict()?;
            }
            if !self.open.contains_key(&key) {
                if self.open.len() == self.options.max_open_writers {
                    self.evict()?;
                }
                limit("output files", self.created + 1, self.max_files)?;
                let relative = key.join(format!("part-{:08}.parquet", self.created));
                fs::create_dir_all(self.root.join(&key))?;
                let file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(self.root.join(&relative))?;
                let props = WriterProperties::builder()
                    .set_max_row_group_size(self.options.row_group_rows)
                    .set_dictionary_enabled(false)
                    .build();
                let writer = ArrowWriter::try_new(file, stdf_arrow::eav_schema(), Some(props))?;
                self.open.insert(
                    key.clone(),
                    OpenFragment {
                        writer,
                        relative,
                        rows: 0,
                        batches: 0,
                        tick: self.tick,
                    },
                );
                self.created += 1;
                self.peak_open = self.peak_open.max(self.open.len());
            }
            let item = self.open.get_mut(&key).unwrap();
            count = count.min(self.options.row_group_rows - item.rows);
            item.writer.write(&batch.slice(offset, count))?;
            item.rows += count;
            item.batches += 1;
            self.tick += 1;
            item.tick = self.tick;
            let full = item.rows == self.options.row_group_rows;
            self.peak_bytes = self.peak_bytes.max(self.memory());
            limit("writer bytes", self.memory(), self.budget)?;
            if full {
                self.close(&key)?;
            }
            offset += count;
        }
        Ok(())
    }
}

struct Stage {
    path: PathBuf,
    published: bool,
}
impl Drop for Stage {
    fn drop(&mut self) {
        if !self.published {
            fs::remove_dir_all(&self.path).ok();
        }
    }
}

fn write_receipt(
    stage: &Path,
    destination: &Path,
    outputs: &[FileConversionSummary],
    rows: usize,
) -> Result<()> {
    let fragments = outputs.iter().map(|o| {
        let relative = o.output_path.strip_prefix(destination).unwrap();
        Ok(json!({ "path": relative.to_string_lossy(), "rows": o.rows, "batches": o.batches, "sha256": crate::catalog::sha256_file(&stage.join(relative))? }))
    }).collect::<Result<Vec<_>>>()?;
    let receipt = json!({ "version": 2, "rows": rows, "fragments": fragments });
    let mut file = File::create(stage.join("_SUCCESS.json"))?;
    serde_json::to_writer(&mut file, &receipt).map_err(|e| invalid(e.to_string()))?;
    file.sync_all()?;
    Ok(())
}
fn read_receipt(
    input: &Path,
    root: &Path,
    expected_rows: usize,
    max_files: usize,
) -> Result<Vec<FileConversionSummary>> {
    if fs::symlink_metadata(root)?.file_type().is_symlink() {
        return Err(invalid("source generation must not be a symlink"));
    }
    let receipt_path = root.join("_SUCCESS.json");
    limit(
        "receipt bytes",
        fs::metadata(&receipt_path)?.len() as usize,
        max_files.saturating_mul(2048).saturating_add(1024),
    )?;
    let receipt: Value =
        serde_json::from_reader(File::open(receipt_path)?).map_err(|e| invalid(e.to_string()))?;
    if receipt["version"].as_u64() != Some(2)
        || receipt["rows"].as_u64() != Some(expected_rows as u64)
    {
        return Err(invalid("source receipt mismatch"));
    }
    let fragments = receipt["fragments"]
        .as_array()
        .ok_or_else(|| invalid("invalid fragment list"))?;
    limit("output files", fragments.len(), max_files)?;
    let canonical = fs::canonicalize(root)?;
    let mut seen = BTreeSet::new();
    let mut outputs = Vec::new();
    for item in fragments {
        let relative = Path::new(
            item["path"]
                .as_str()
                .ok_or_else(|| invalid("missing fragment path"))?,
        );
        if relative
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
            || !seen.insert(relative.to_path_buf())
        {
            return Err(invalid("unsafe or duplicate fragment path"));
        }
        let output_path = root.join(relative);
        if !fs::canonicalize(&output_path)?.starts_with(&canonical) {
            return Err(invalid("fragment escapes source directory"));
        }
        if item["sha256"].as_str() != Some(crate::catalog::sha256_file(&output_path)?.as_str()) {
            return Err(invalid("fragment SHA-256 mismatch"));
        }
        let rows = item["rows"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| invalid("invalid fragment row count"))?;
        let batches = item["batches"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| invalid("invalid fragment batch count"))?;
        if rows == 0 || batches == 0 || batches > rows {
            return Err(invalid("invalid fragment counts"));
        }
        let reader = ParquetRecordBatchReaderBuilder::try_new(File::open(&output_path)?)?;
        if reader.metadata().file_metadata().num_rows() != rows as i64
            || reader.schema().as_ref() != stdf_arrow::eav_schema().as_ref()
        {
            return Err(invalid("fragment footer/schema mismatch"));
        }
        outputs.push(FileConversionSummary {
            input_path: input.into(),
            output_path,
            rows,
            batches,
        });
    }
    if outputs
        .iter()
        .try_fold(0usize, |n, o| n.checked_add(o.rows))
        != Some(expected_rows)
    {
        return Err(invalid("fragment row totals mismatch"));
    }
    Ok(outputs)
}
