use super::*;
use arrow::array::{ArrayRef, StringArray, UInt64Array};
use arrow::datatypes::{DataType, Field as ArrowField, Schema};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::basic::Compression;
use parquet::file::properties::WriterProperties;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub struct LimitedFile {
    file: File,
    quota: Arc<Mutex<(u64, u64)>>,
}
impl Write for LimitedFile {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        let mut q = self.quota.lock().unwrap();
        if (b.len() as u64) > q.1.saturating_sub(q.0) {
            return Err(std::io::Error::other("sanity disk limit exceeded"));
        }
        let n = self.file.write(b)?;
        q.0 += n as u64;
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}
impl LimitedFile {
    pub fn sync(&mut self) -> CliResult<()> {
        self.flush()?;
        self.file.sync_all()?;
        Ok(())
    }
}

pub struct Stage {
    pub path: PathBuf,
    pub name: String,
    root: PathBuf,
    quota: Arc<Mutex<(u64, u64)>>,
    published: bool,
}
impl Stage {
    pub fn new(root: &Path, limit: u64) -> CliResult<Self> {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        fs::create_dir_all(root)?;
        let root = root.canonicalize()?;
        let name = format!(
            "sanity-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos(),
            SERIAL.fetch_add(1, Ordering::Relaxed)
        );
        let path = root.join(&name);
        fs::create_dir(&path)?;
        Ok(Self {
            path,
            name,
            root,
            quota: Arc::new(Mutex::new((0, limit))),
            published: false,
        })
    }
    pub fn file(&self, name: &str) -> CliResult<LimitedFile> {
        Ok(LimitedFile {
            file: File::options()
                .write(true)
                .create_new(true)
                .open(self.path.join(name))?,
            quota: self.quota.clone(),
        })
    }
    pub fn write(&self, name: &str, data: &[u8]) -> CliResult<()> {
        let mut f = self.file(name)?;
        f.write_all(data)?;
        f.sync()
    }
    pub fn rename(&self, old: &str, new: &str) -> CliResult<()> {
        fs::rename(self.path.join(old), self.path.join(new))?;
        Ok(())
    }
    pub fn remove(&self, name: &str) -> CliResult<()> {
        let p = self.path.join(name);
        let size = fs::metadata(&p)?.len();
        fs::remove_file(p)?;
        self.quota.lock().unwrap().0 -= size;
        Ok(())
    }
    pub fn manifest(&self, profile_hash: &str) -> CliResult<()> {
        let mut files = BTreeMap::new();
        for entry in fs::read_dir(&self.path)? {
            let entry = entry?;
            let mut reader = File::open(entry.path())?;
            let mut hash = Sha256::new();
            let mut b = [0; 65536];
            loop {
                let n = reader.read(&mut b)?;
                if n == 0 {
                    break;
                }
                hash.update(&b[..n]);
            }
            files.insert(
                entry.file_name().to_string_lossy().into_owned(),
                json!({"bytes":entry.metadata()?.len(),"sha256":format!("{:x}",hash.finalize())}),
            );
        }
        self.write(
            "manifest.json",
            &serde_json::to_vec_pretty(
                &json!({"schema":"sanity-bundle-v1","profile_hash":profile_hash,"files":files}),
            )?,
        )
    }
    pub fn publish(
        &mut self,
        html: &[u8],
        check: impl FnOnce() -> Result<(), String>,
    ) -> CliResult<()> {
        {
            let mut q = self.quota.lock().unwrap();
            if html.len() as u64 > q.1.saturating_sub(q.0) {
                return Err("disk budget cannot hold atomic report copy".into());
            }
            q.0 += html.len() as u64;
        }
        // The root HTML is the single commit point; it pins this immutable generation.
        stdf_parquet::catalog::atomic_write_checked(&self.root.join("report.html"), html, || {
            check().map_err(std::io::Error::other)
        })?;
        self.published = true;
        Ok(())
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        if !self.published
            && self.path.parent() == Some(self.root.as_path())
            && self.path.file_name().and_then(|s| s.to_str()) == Some(&self.name)
        {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

pub struct EvidenceWriter {
    writer: ArrowWriter<LimitedFile>,
    rows: Vec<(String, u64, String, Field)>,
    bytes: usize,
    schema: Arc<Schema>,
}
impl EvidenceWriter {
    pub fn new(file: LimitedFile) -> CliResult<Self> {
        let schema = Arc::new(Schema::new(vec![
            ArrowField::new("source_id", DataType::Utf8, false),
            ArrowField::new("record_offset", DataType::UInt64, false),
            ArrowField::new("record_type", DataType::Utf8, false),
            ArrowField::new("field_name", DataType::Utf8, false),
            ArrowField::new("field_type", DataType::Utf8, false),
            ArrowField::new("byte_start", DataType::UInt64, false),
            ArrowField::new("byte_len", DataType::UInt64, false),
            ArrowField::new("evidence_json", DataType::Utf8, false),
        ]));
        let props = WriterProperties::builder()
            .set_compression(Compression::SNAPPY)
            .set_dictionary_enabled(false)
            .set_max_row_group_size(128)
            .build();
        Ok(Self {
            writer: ArrowWriter::try_new(file, schema.clone(), Some(props))?,
            schema,
            rows: Vec::new(),
            bytes: 0,
        })
    }
    pub fn append(
        &mut self,
        source: &str,
        offset: usize,
        record: &str,
        fields: &[Field],
    ) -> CliResult<()> {
        for f in fields {
            self.bytes += serde_json::to_vec(f)?.len() + 256;
            self.rows
                .push((source.into(), offset as u64, record.into(), f.clone()));
            if self.bytes >= 256 * 1024 || self.rows.len() >= 128 {
                self.flush()?;
            }
        }
        Ok(())
    }
    fn flush(&mut self) -> CliResult<()> {
        if self.rows.is_empty() {
            return Ok(());
        }
        let evidence = self
            .rows
            .iter()
            .map(|r| serde_json::to_string(&r.3))
            .collect::<Result<Vec<_>, _>>()?;
        let arrays: Vec<ArrayRef> = vec![
            Arc::new(StringArray::from_iter_values(
                self.rows.iter().map(|r| r.0.as_str()),
            )),
            Arc::new(UInt64Array::from_iter_values(self.rows.iter().map(|r| r.1))),
            Arc::new(StringArray::from_iter_values(
                self.rows.iter().map(|r| r.2.as_str()),
            )),
            Arc::new(StringArray::from_iter_values(
                self.rows.iter().map(|r| r.3.name.as_str()),
            )),
            Arc::new(StringArray::from_iter_values(
                self.rows.iter().map(|r| r.3.kind.as_str()),
            )),
            Arc::new(UInt64Array::from_iter_values(
                self.rows.iter().map(|r| r.3.byte_start as u64),
            )),
            Arc::new(UInt64Array::from_iter_values(
                self.rows.iter().map(|r| r.3.byte_len as u64),
            )),
            Arc::new(StringArray::from(evidence)),
        ];
        self.writer
            .write(&RecordBatch::try_new(self.schema.clone(), arrays)?)?;
        self.writer.flush()?;
        self.rows.clear();
        self.bytes = 0;
        Ok(())
    }
    pub fn finish(mut self) -> CliResult<()> {
        self.flush()?;
        self.writer.close()?;
        Ok(())
    }
}
