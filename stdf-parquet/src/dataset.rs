use std::cell::Cell;
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use arrow::array::{Array, StringArray};
use stdf_core::{StdfError, StdfRecord};
use stdf_io::{IoError, StreamingRecordReader};

use crate::{records_to_parquet_path_atomic, AtomicWriteOptions, Result};
pub(crate) mod fragments;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PartitionKey {
    InputFile,
    LotId,
    WaferId,
}

impl PartitionKey {
    pub fn name(self) -> &'static str {
        match self {
            Self::InputFile => "input_file",
            Self::LotId => "lot_id",
            Self::WaferId => "wafer_id",
        }
    }
}

impl std::str::FromStr for PartitionKey {
    type Err = String;
    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "input-file" | "input_file" => Ok(Self::InputFile),
            "lot-id" | "lot_id" => Ok(Self::LotId),
            "wafer-id" | "wafer_id" => Ok(Self::WaferId),
            _ => Err(format!(
                "unknown partition key {value:?}; use input-file, lot-id, or wafer-id"
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileConversionSummary {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub rows: usize,
    pub batches: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MultiFileConversionSummary {
    pub files: usize,
    pub rows: usize,
    pub batches: usize,
    pub outputs: Vec<FileConversionSummary>,
}

/// Convert one source per Parquet file, sequentially, after validating all inputs.
/// Selected lot/wafer values must be homogeneous in each source's emitted rows.
/// Input paths must remain available for retries. Changed bytes get a new filename.
/// A failure during writing leaves earlier committed files available for retry.
pub fn files_to_partitioned_parquet_dir(
    input_paths: &[PathBuf],
    output_dir: &Path,
    batch_size: usize,
    options: &AtomicWriteOptions,
    partition_by: &[PartitionKey],
) -> Result<MultiFileConversionSummary> {
    if input_paths.is_empty() {
        return Err(invalid("no input files supplied"));
    }
    if partition_by.iter().collect::<BTreeSet<_>>().len() != partition_by.len() {
        return Err(invalid("duplicate partition keys"));
    }
    let paths = input_paths
        .iter()
        .map(fs::canonicalize)
        .collect::<std::io::Result<BTreeSet<_>>>()?;
    let mut plan = Vec::with_capacity(paths.len());
    let mut destinations = BTreeSet::new();
    for path in paths {
        let (records, digest) =
            open_records(&path).map_err(|error| invalid(format!("{}: {error}", path.display())))?;
        let mut values: Vec<Option<Option<String>>> = vec![None; partition_by.len()];
        let mut expected_rows = 0usize;
        for batch in stdf_arrow::record_batches(records, batch_size) {
            let batch = batch.map_err(|error| invalid(format!("{}: {error}", path.display())))?;
            expected_rows += batch.num_rows();
            for (index, key) in partition_by.iter().enumerate() {
                let column = match key {
                    PartitionKey::InputFile => continue,
                    PartitionKey::LotId => 0,
                    PartitionKey::WaferId => 1,
                };
                let array = batch
                    .column(column)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .expect("EAV string column");
                for row in 0..batch.num_rows() {
                    let value = (!array.is_null(row)).then(|| array.value(row));
                    match &values[index] {
                        Some(previous) if previous.as_deref() != value => return Err(invalid(format!(
                            "{} has multiple {} values; use input-file partitioning for this source", path.display(), key.name()))),
                        None => values[index] = Some(value.map(str::to_owned)),
                        _ => {}
                    }
                }
            }
        }
        let mut output = output_dir.to_path_buf();
        for (index, key) in partition_by.iter().enumerate() {
            let value = if *key == PartitionKey::InputFile {
                Some(
                    path.file_name()
                        .ok_or_else(|| invalid("input has no filename"))?
                        .to_string_lossy()
                        .into_owned(),
                )
            } else {
                values[index].take().flatten()
            };
            output.push(format!(
                "{}={}",
                key.name(),
                partition_value(value.as_deref())?
            ));
        }
        // A fixed FNV-1a 128-bit fingerprint is stable across Rust versions and run order.
        // It identifies local sources, and is not a cryptographic integrity guarantee.
        let path_hash = hash_bytes(
            hash_bytes(HASH_INIT, b"eav-v2|coordinate-v1|mpr-ftr-v1"),
            path.as_os_str().as_encoded_bytes(),
        );
        output.push(format!(
            "part-{path_hash:032x}-{:032x}.parquet",
            digest.get()
        ));
        if !destinations.insert(output.clone()) {
            return Err(invalid("output identity collision"));
        }
        plan.push((path, output, digest.get(), expected_rows));
    }

    fs::create_dir_all(output_dir)?;
    let _lock = DatasetLock::acquire(output_dir)?;
    let mut summary = MultiFileConversionSummary::default();
    for (input_path, output_path, expected_digest, expected_rows) in plan {
        let result = (|| {
            fs::create_dir_all(output_path.parent().expect("output parent"))?;
            if output_path.exists() && !options.overwrite {
                verify_existing_output(&output_path, expected_rows)?;
            }
            records_to_parquet_path_atomic(
                checked_records(&input_path, expected_digest)?,
                &output_path,
                batch_size,
                options,
            )
        })()
        .map_err(|error| {
            invalid(format!(
                "{} -> {}: {error}",
                input_path.display(),
                output_path.display()
            ))
        })?;
        summary.files += 1;
        summary.rows += result.rows;
        summary.batches += result.batches;
        summary.outputs.push(FileConversionSummary {
            input_path,
            output_path,
            rows: result.rows,
            batches: result.batches,
        });
    }
    Ok(summary)
}

fn invalid(message: impl Into<String>) -> crate::StdfParquetError {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message.into()).into()
}

fn checked_records(
    path: &Path,
    expected_digest: u128,
) -> Result<impl Iterator<Item = std::result::Result<StdfRecord, StdfError>>> {
    let (records, digest) = open_records(path)?;
    let mut checked = false;
    let check = std::iter::from_fn(move || {
        if checked {
            return None;
        }
        checked = true;
        (digest.get() != expected_digest).then(|| {
            Err(StdfError::Io(std::io::Error::other(
                "input changed during conversion; retry with a stable source",
            )))
        })
    });
    Ok(records.chain(check))
}

fn verify_existing_output(path: &Path, expected_rows: usize) -> Result<()> {
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    let reader = ParquetRecordBatchReaderBuilder::try_new(File::open(path)?)?;
    let manifest = crate::read_manifest_summary(&crate::manifest_path(path))?
        .ok_or_else(|| invalid("existing output has no readable manifest"))?;
    if reader.metadata().file_metadata().num_rows() != expected_rows as i64
        || manifest.rows != expected_rows
        || reader.schema().as_ref() != stdf_arrow::eav_schema().as_ref()
    {
        return Err(invalid("existing output metadata does not match the source; rerun without --no-overwrite to repair"));
    }
    Ok(())
}

fn partition_value(value: Option<&str>) -> Result<String> {
    let Some(value) = value else {
        return Ok("__HIVE_DEFAULT_PARTITION__".into());
    };
    if value.is_empty() {
        return Ok("__empty".into());
    }
    let mut encoded = String::new();
    for (index, byte) in value.bytes().enumerate() {
        let reserved = index == 0 && value.starts_with("__");
        // Escape lowercase too, so case-insensitive filesystems cannot merge distinct values.
        if (byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_' || byte == b'-')
            && !reserved
        {
            encoded.push(byte as char);
        } else {
            use std::fmt::Write;
            write!(&mut encoded, "%{byte:02X}").unwrap();
        }
    }
    if encoded.len() > 180 {
        return Err(invalid(
            "encoded partition value exceeds 180 bytes; use input-file partitioning or fewer keys",
        ));
    }
    Ok(encoded)
}

const HASH_INIT: u128 = 0x6c62272e07bb014262b821756295c58d;
fn hash_bytes(mut hash: u128, bytes: &[u8]) -> u128 {
    for byte in bytes {
        hash = (hash ^ u128::from(*byte)).wrapping_mul(0x0000000001000000000000000000013b);
    }
    hash
}
struct HashingReader {
    file: File,
    digest: Rc<Cell<u128>>,
}
impl Read for HashingReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let count = self.file.read(buffer)?;
        self.digest
            .set(hash_bytes(self.digest.get(), &buffer[..count]));
        Ok(count)
    }
}

fn open_records(
    path: &Path,
) -> Result<(
    impl Iterator<Item = std::result::Result<StdfRecord, StdfError>>,
    Rc<Cell<u128>>,
)> {
    let mut file = File::open(path)?;
    let mut magic = [0; 2];
    file.read_exact(&mut magic)?;
    file.seek(SeekFrom::Start(0))?;
    let gzip = magic == [0x1f, 0x8b]
        || path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("gz"));
    let digest = Rc::new(Cell::new(HASH_INIT));
    let raw = HashingReader {
        file,
        digest: digest.clone(),
    };
    let source: Box<dyn Read> = if gzip {
        Box::new(flate2::read::MultiGzDecoder::new(raw))
    } else {
        Box::new(raw)
    };
    let records = StreamingRecordReader::new(source)
        .map_err(io_error)?
        .map(|record| record.map_err(io_error));
    Ok((records, digest))
}
fn io_error(error: IoError) -> StdfError {
    match error {
        IoError::Decode(error) => error,
        IoError::Io(error) => StdfError::Io(error),
        error => StdfError::Io(std::io::Error::other(error.to_string())),
    }
}

pub(crate) struct DatasetLock {
    path: PathBuf,
    file: Option<File>,
}
impl DatasetLock {
    pub(crate) fn acquire(root: &Path) -> Result<Self> {
        let path = root.join(".zstdf-convert.lock");
        let file = fs::OpenOptions::new().write(true).create_new(true).open(&path).map_err(|error| invalid(format!(
            "cannot acquire {}: {error}; after a crash, remove this lock only after verifying no conversion is active", path.display())))?;
        let mut lock = Self {
            path,
            file: Some(file),
        };
        writeln!(lock.file.as_mut().unwrap(), "pid={}", std::process::id())?;
        Ok(lock)
    }
}
impl Drop for DatasetLock {
    fn drop(&mut self) {
        drop(self.file.take());
        fs::remove_file(&self.path).ok();
    }
}

#[cfg(test)]
mod tests;
