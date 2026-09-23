// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
use super::*;
use arrow::{
    array::{Array, ArrayRef, Float64Array, StringArray, UInt64Array},
    datatypes::{DataType, Field as ArrowField, Schema},
    record_batch::RecordBatch,
};
use parquet::{
    arrow::{arrow_reader::ParquetRecordBatchReaderBuilder, ArrowWriter},
    file::properties::WriterProperties,
};

pub fn atomic_write(path: &Path, data: &[u8]) -> CliResult<()> {
    atomic_write_checked(path, data, || Ok(()))
}
pub fn atomic_write_checked(
    path: &Path,
    data: &[u8],
    check: impl FnOnce() -> std::io::Result<()>,
) -> CliResult<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    stdf_parquet::catalog::atomic_write_checked(path, data, check)?;
    Ok(())
}

pub struct IndexedWriter {
    data: File,
    index: File,
}
impl IndexedWriter {
    pub fn new(root: &Path, name: &str) -> CliResult<Self> {
        Ok(Self {
            data: File::create(root.join(format!("{name}.bin")))?,
            index: File::create(root.join(format!("{name}.idx")))?,
        })
    }
    pub fn push<T: Serialize>(&mut self, id: u64, v: &T) -> CliResult<()> {
        let bytes = serde_json::to_vec(v)?;
        if bytes.len() > 4 * 1024 * 1024 {
            return Err("record evidence exceeds 4 MiB".into());
        }
        let mut compressed =
            flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        compressed.write_all(&bytes)?;
        let compressed = compressed.finish()?;
        let (codec, body) = if compressed.len() < bytes.len() {
            (1u8, compressed.as_slice())
        } else {
            (0, bytes.as_slice())
        };
        let pos = self.data.stream_position()?;
        self.index.seek(SeekFrom::Start(id * 8))?;
        self.index.write_all(&(pos + 1).to_le_bytes())?;
        self.data
            .write_all(&((body.len() + 1) as u32).to_le_bytes())?;
        self.data.write_all(&[codec])?;
        self.data.write_all(body)?;
        Ok(())
    }
}
pub fn lookup<T: serde::de::DeserializeOwned>(root: &Path, name: &str, id: u64) -> CliResult<T> {
    IndexedReader::open(root, name)?.get(id)
}
pub struct IndexedReader {
    index: File,
    data: File,
}
impl IndexedReader {
    pub fn open(root: &Path, name: &str) -> CliResult<Self> {
        Ok(Self {
            index: File::open(root.join(format!("{name}.idx")))?,
            data: File::open(root.join(format!("{name}.bin")))?,
        })
    }
    pub fn get<T: serde::de::DeserializeOwned>(&mut self, id: u64) -> CliResult<T> {
        self.index
            .seek(SeekFrom::Start(id.checked_mul(8).ok_or("ID overflow")?))?;
        let mut p = [0u8; 8];
        self.index.read_exact(&mut p)?;
        let p = u64::from_le_bytes(p)
            .checked_sub(1)
            .ok_or("unknown record ID")?;
        let file = &mut self.data;
        file.seek(SeekFrom::Start(p))?;
        let mut n = [0u8; 4];
        file.read_exact(&mut n)?;
        let n = u32::from_le_bytes(n) as usize;
        if n == 0 || n > 4 * 1024 * 1024 + 1 {
            return Err("invalid evidence length".into());
        }
        let mut b = vec![0; n];
        file.read_exact(&mut b)?;
        match b[0] {
            0 => Ok(serde_json::from_slice(&b[1..])?),
            1 => {
                let mut decoded = Vec::new();
                flate2::read::ZlibDecoder::new(&b[1..])
                    .take(4 * 1024 * 1024 + 1)
                    .read_to_end(&mut decoded)?;
                if decoded.len() > 4 * 1024 * 1024 {
                    return Err("decompressed evidence exceeds limit".into());
                }
                Ok(serde_json::from_slice(&decoded)?)
            }
            _ => Err("unknown evidence codec".into()),
        }
    }
}

pub struct TableWriter {
    root: PathBuf,
    name: String,
    rows: Vec<Row>,
    bytes: usize,
    metadata_bytes: usize,
    budget: usize,
    lookup: Option<IndexedWriter>,
    lookup_attempt: u64,
    lookup_fragments: BTreeSet<String>,
    pub fragments: Vec<Fragment>,
}
impl TableWriter {
    pub fn new(root: &Path, name: &str, budget: usize) -> Self {
        Self {
            root: root.into(),
            name: name.into(),
            rows: Vec::new(),
            bytes: 0,
            metadata_bytes: 0,
            budget,
            lookup: None,
            lookup_attempt: 0,
            lookup_fragments: BTreeSet::new(),
            fragments: Vec::new(),
        }
    }
    pub fn push(&mut self, row: Row) -> CliResult<()> {
        if self.name.ends_with("measurements") {
            if self.lookup.is_none() {
                self.lookup = Some(IndexedWriter::new(&self.root, "measurement_lookup")?);
            }
            if self.lookup_attempt != row.attempt {
                if self.lookup_attempt != 0 {
                    self.lookup
                        .as_mut()
                        .unwrap()
                        .push(self.lookup_attempt, &self.lookup_fragments)?;
                }
                self.lookup_attempt = row.attempt;
                self.lookup_fragments.clear();
            }
            self.lookup_fragments.insert(format!(
                "{}-{:06}.parquet",
                self.name,
                self.fragments.len()
            ));
        }
        self.bytes += serde_json::to_vec(&row)?.len();
        self.rows.push(row);
        if self.rows.len() >= 1024 || self.bytes >= 1024 * 1024 {
            self.flush()?;
        }
        Ok(())
    }
    pub fn flush(&mut self) -> CliResult<()> {
        if let Some(lookup) = &mut self.lookup {
            if self.lookup_attempt != 0 {
                lookup.push(self.lookup_attempt, &self.lookup_fragments)?;
            }
        }
        if self.rows.is_empty() {
            return Ok(());
        }
        if self.fragments.len() >= 100_000 {
            return Err("viewer fragment limit exceeded".into());
        }
        let schema = Arc::new(Schema::new(vec![
            ArrowField::new("id", DataType::UInt64, false),
            ArrowField::new("attempt", DataType::UInt64, false),
            ArrowField::new("run", DataType::UInt64, false),
            ArrowField::new("test", DataType::Utf8, false),
            ArrowField::new("value", DataType::Float64, true),
            ArrowField::new("evidence", DataType::Utf8, false),
        ]));
        let payload: Vec<String> = self
            .rows
            .iter()
            .map(serde_json::to_string)
            .collect::<Result<_, _>>()?;
        let cols: Vec<ArrayRef> = vec![
            Arc::new(UInt64Array::from_iter_values(
                self.rows.iter().map(|r| r.id),
            )),
            Arc::new(UInt64Array::from_iter_values(
                self.rows.iter().map(|r| r.attempt),
            )),
            Arc::new(UInt64Array::from_iter_values(
                self.rows.iter().map(|r| r.run),
            )),
            Arc::new(StringArray::from_iter_values(
                self.rows.iter().map(|r| r.test.as_str()),
            )),
            Arc::new(Float64Array::from(
                self.rows.iter().map(|r| r.value).collect::<Vec<_>>(),
            )),
            Arc::new(StringArray::from(payload)),
        ];
        let batch = RecordBatch::try_new(schema.clone(), cols)?;
        let path = format!("{}-{:06}.parquet", self.name, self.fragments.len());
        let props = WriterProperties::builder()
            .set_compression(parquet::basic::Compression::ZSTD(Default::default()))
            .set_max_row_group_size(1024)
            .build();
        let mut writer =
            ArrowWriter::try_new(File::create(self.root.join(&path))?, schema, Some(props))?;
        writer.write(&batch)?;
        writer.close()?;
        self.fragments.push(Fragment {
            sha256: stdf_parquet::catalog::sha256_file(&self.root.join(&path))?,
            path,
            rows: self.rows.len(),
            first: self.rows.iter().map(|r| r.id).min().unwrap(),
            last: self.rows.iter().map(|r| r.id).max().unwrap(),
            tests: self
                .rows
                .iter()
                .map(|r| r.test.clone())
                .filter(|s| !s.is_empty())
                .collect(),
            runs: self.rows.iter().map(|r| r.run).collect(),
        });
        self.metadata_bytes += serde_json::to_vec(self.fragments.last().unwrap())?.len() * 4;
        if self.metadata_bytes > self.budget {
            return Err(
                "companion index metadata exceeds memory budget; increase --memory-limit-mib"
                    .into(),
            );
        }
        self.rows.clear();
        self.bytes = 0;
        Ok(())
    }
}
pub fn scan(
    cache: &Cache,
    table: &str,
    tests: &[String],
    run: Option<u64>,
    f: impl FnMut(Row) -> CliResult<()>,
) -> CliResult<()> {
    scan_selected(cache, table, tests, run, &[], f)
}
pub fn scan_selected(
    cache: &Cache,
    table: &str,
    tests: &[String],
    run: Option<u64>,
    attempts: &[u64],
    mut f: impl FnMut(Row) -> CliResult<()>,
) -> CliResult<()> {
    let mut selected = BTreeSet::new();
    if table == "measurements" && !attempts.is_empty() {
        let path = cache.root.join("measurement_lookup.idx");
        if !path.exists() {
            return Ok(());
        }
        let mut index = File::open(path)?;
        let length = index.metadata()?.len();
        for &id in attempts {
            let offset = id.checked_mul(8).ok_or("attempt index overflow")?;
            if offset.saturating_add(8) > length {
                continue;
            }
            index.seek(SeekFrom::Start(offset))?;
            let mut pos = [0; 8];
            index.read_exact(&mut pos)?;
            if u64::from_le_bytes(pos) != 0 {
                let fragments: BTreeSet<String> = lookup(&cache.root, "measurement_lookup", id)?;
                selected.extend(fragments);
            }
        }
    }
    if let Some(fragments) = cache.manifest.fragments.get(table) {
        for frag in fragments {
            cache.check()?;
            if table == "measurements" && !attempts.is_empty() && !selected.contains(&frag.path) {
                continue;
            }
            if run.is_some_and(|r| !frag.runs.contains(&r))
                || (!tests.is_empty() && !tests.iter().any(|t| frag.tests.contains(t)))
            {
                continue;
            }
            let builder =
                ParquetRecordBatchReaderBuilder::try_new(File::open(cache.root.join(&frag.path))?)?;
            let mask = parquet::arrow::ProjectionMask::leaves(builder.parquet_schema(), [2, 3, 5]);
            for b in builder.with_projection(mask).with_batch_size(256).build()? {
                let b = b?;
                let runs = b
                    .column(0)
                    .as_any()
                    .downcast_ref::<UInt64Array>()
                    .ok_or("invalid run column")?;
                let keys = b
                    .column(1)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .ok_or("invalid test column")?;
                let data = b
                    .column(2)
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .ok_or("invalid evidence column")?;
                for i in 0..b.num_rows() {
                    if run.is_some_and(|r| r != runs.value(i))
                        || (!tests.is_empty() && !tests.iter().any(|t| t == keys.value(i)))
                    {
                        continue;
                    }
                    f(serde_json::from_str(data.value(i))?)?;
                }
            }
        }
    }
    Ok(())
}
pub fn disk_bytes(root: &Path) -> CliResult<u64> {
    let mut n = 0;
    for p in fs::read_dir(root)? {
        let p = p?;
        let kind = p.file_type()?;
        if kind.is_file() {
            n += p.metadata()?.len();
        } else if kind.is_dir() && !kind.is_symlink() {
            n += disk_bytes(&p.path())?;
        }
    }
    Ok(n)
}
