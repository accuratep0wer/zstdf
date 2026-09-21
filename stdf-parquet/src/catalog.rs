//! Atomic current-version catalog over immutable source generations.
use crate::{FragmentOptions, FragmentSummary, PartitionKey, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

const CATALOG: &str = "_catalog.json";
const MAX_CATALOG_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogFragment {
    pub path: String,
    pub rows: usize,
    pub sha256: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceVersion {
    pub source_path: String,
    pub source_sha256: String,
    pub options_sha256: String,
    pub rows: usize,
    pub fragments: Vec<CatalogFragment>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileFailure {
    pub path: String,
    pub error: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStatus {
    pub id: String,
    pub status: String,
    pub completed: usize,
    pub failures: Vec<FileFailure>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetCatalog {
    pub version: u32,
    pub schema_version: String,
    pub revision: u64,
    pub sources: BTreeMap<String, SourceVersion>,
    pub run: RunStatus,
}
impl Default for DatasetCatalog {
    fn default() -> Self {
        Self {
            version: 1,
            schema_version: stdf_arrow::schema::SCHEMA_VERSION.into(),
            revision: 0,
            sources: BTreeMap::new(),
            run: RunStatus {
                id: String::new(),
                status: "complete".into(),
                completed: 0,
                failures: Vec::new(),
            },
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum ErrorPolicy {
    FailFast,
    Continue,
}
#[derive(Debug)]
pub struct DatasetRun {
    pub summary: FragmentSummary,
    pub failures: Vec<FileFailure>,
}

pub fn sha256_file(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        digest.update(&buffer[..n]);
    }
    Ok(format!("{:x}", digest.finalize()))
}
fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn invalid(message: impl Into<String>) -> crate::StdfParquetError {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into()).into()
}

pub fn load_catalog(root: &Path) -> Result<DatasetCatalog> {
    read_catalog(root, MAX_CATALOG_BYTES)
}
fn read_catalog(root: &Path, max_bytes: usize) -> Result<DatasetCatalog> {
    let path = root.join(CATALOG);
    let file = File::open(&path)?;
    if file.metadata()?.len() > max_bytes as u64 {
        return Err(invalid("catalog exceeds configured metadata budget"));
    }
    let catalog: DatasetCatalog = serde_json::from_reader(file.take(max_bytes as u64 + 1))
        .map_err(|e| invalid(e.to_string()))?;
    if catalog.version != 1 || catalog.schema_version != stdf_arrow::schema::SCHEMA_VERSION {
        return Err(invalid("unsupported catalog/schema version; reconvert source STDF files into a new eav-v2 dataset directory"));
    }
    Ok(catalog)
}

pub fn verify_catalog(root: &Path) -> Result<DatasetCatalog> {
    let catalog = load_catalog(root)?;
    let mut seen = BTreeSet::new();
    for (id, source) in &catalog.sources {
        if id != &sha256(source.source_path.as_bytes()) {
            return Err(invalid("catalog source identity mismatch"));
        }
        verify_source(root, source, &mut seen)?;
    }
    Ok(catalog)
}
fn verify_source(root: &Path, source: &SourceVersion, seen: &mut BTreeSet<PathBuf>) -> Result<()> {
    for hash in [&source.source_sha256, &source.options_sha256] {
        validate_hash(hash)?;
    }
    let mut rows = 0usize;
    for fragment in &source.fragments {
        let path = verified_path(root, &fragment.path)?;
        if !seen.insert(path.clone()) {
            return Err(invalid("duplicate catalog fragment"));
        }
        validate_hash(&fragment.sha256)?;
        if sha256_file(&path)? != fragment.sha256 {
            return Err(invalid(format!("SHA-256 mismatch: {}", path.display())));
        }
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
            File::open(&path)?,
        )?;
        if reader.metadata().file_metadata().num_rows() != fragment.rows as i64
            || reader.schema().as_ref() != stdf_arrow::eav_schema().as_ref()
        {
            return Err(invalid("catalog fragment schema/row mismatch"));
        }
        rows = rows
            .checked_add(fragment.rows)
            .ok_or_else(|| invalid("row count overflow"))?;
    }
    if rows != source.rows {
        return Err(invalid("catalog source row total mismatch"));
    }
    Ok(())
}
fn validate_hash(hash: &str) -> Result<()> {
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return Err(invalid("invalid SHA-256 value"));
    }
    Ok(())
}
pub fn verified_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if relative.is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(invalid("unsafe catalog fragment path"));
    }
    let canonical = fs::canonicalize(root.join(path))?;
    if !canonical.starts_with(fs::canonicalize(root)?) {
        return Err(invalid("catalog fragment escapes dataset"));
    }
    Ok(canonical)
}

pub fn convert_dataset(
    inputs: &[PathBuf],
    root: &Path,
    keys: &[PartitionKey],
    options: &FragmentOptions,
    policy: ErrorPolicy,
) -> Result<DatasetRun> {
    options.budget()?;
    let mut writer_options = options.clone();
    writer_options.max_memory_bytes = options.max_memory_bytes / 4 * 3;
    writer_options.budget()?;
    if inputs.is_empty()
        || inputs.len() > options.max_output_files
        || keys.iter().collect::<BTreeSet<_>>().len() != keys.len()
    {
        return Err(invalid("invalid inputs or duplicate partition keys"));
    }
    let max_bytes = (options.max_memory_bytes / 64).min(MAX_CATALOG_BYTES);
    fs::create_dir_all(root)?;
    let _guard = CatalogGuard::acquire(root)?;
    let mut catalog = if root.join(CATALOG).exists() {
        read_catalog(root, max_bytes)?
    } else {
        DatasetCatalog::default()
    };
    catalog.run = RunStatus {
        id: format!("{}-{}", std::process::id(), crate::monotonic_nanos()),
        status: "running".into(),
        completed: 0,
        failures: Vec::new(),
    };
    publish(root, &mut catalog, max_bytes)?;
    let options_sha = sha256(
        format!("catalog-fragments-v3|eav-v2|coordinate-v1|mpr-ftr-v1|{keys:?}|{options:?}")
            .as_bytes(),
    );
    let objects = root.join("objects");
    fs::create_dir_all(&objects)?;
    if !fs::canonicalize(&objects)?.starts_with(fs::canonicalize(root)?) {
        return Err(invalid("object store escapes dataset"));
    }
    let mut summary = FragmentSummary::default();
    let mut seen = BTreeSet::new();
    for input in inputs {
        let attempt = (|| -> Result<()> {
            let path = fs::canonicalize(input)?;
            if !seen.insert(path.clone()) {
                return Ok(());
            }
            let source_path = path
                .to_str()
                .ok_or_else(|| invalid("source path must be UTF-8"))?
                .to_string();
            let source_id = sha256(source_path.as_bytes());
            let source_sha = sha256_file(&path)?;
            if let Some(previous) = catalog.sources.get(&source_id) {
                if previous.source_sha256 == source_sha && previous.options_sha256 == options_sha {
                    verify_source(root, previous, &mut BTreeSet::new())?;
                }
            }
            let generation = sha256(format!("{source_id}|{source_sha}|{options_sha}").as_bytes());
            let remaining = options
                .max_output_files
                .checked_sub(summary.conversion.outputs.len())
                .filter(|n| *n > 0)
                .ok_or_else(|| invalid("output file limit exhausted"))?;
            // Keep the writer configuration stable across input ordering; check the total separately.
            let result = crate::dataset::fragments::files_to_partitioned_fragments_with_limit(
                &[path.clone()],
                &objects.join(generation),
                keys,
                &writer_options,
                remaining,
            )?;
            if result.conversion.outputs.len() > remaining {
                return Err(invalid("output file limit exceeded"));
            }
            inject_failure(1)?;
            if sha256_file(&path)? != source_sha {
                return Err(invalid("source changed while converting"));
            }
            let mut fragments = Vec::new();
            for output in &result.conversion.outputs {
                let relative = output
                    .output_path
                    .strip_prefix(root)
                    .map_err(|_| invalid("fragment outside dataset"))?
                    .to_str()
                    .ok_or_else(|| invalid("non UTF-8 fragment path"))?
                    .to_string();
                fragments.push(CatalogFragment {
                    path: relative,
                    rows: output.rows,
                    sha256: sha256_file(&output.output_path)?,
                });
            }
            catalog.sources.insert(
                source_id,
                SourceVersion {
                    source_path,
                    source_sha256: source_sha,
                    options_sha256: options_sha.clone(),
                    rows: result.conversion.rows,
                    fragments,
                },
            );
            catalog.run.completed += 1;
            publish(root, &mut catalog, max_bytes)?;
            summary.peak_open_writers = summary.peak_open_writers.max(result.peak_open_writers);
            summary.peak_pending_bytes = summary.peak_pending_bytes.max(result.peak_pending_bytes);
            summary.peak_writer_bytes = summary.peak_writer_bytes.max(result.peak_writer_bytes);
            summary.conversion.files += result.conversion.files;
            summary.conversion.rows += result.conversion.rows;
            summary.conversion.batches += result.conversion.batches;
            summary.conversion.outputs.extend(result.conversion.outputs);
            Ok(())
        })();
        if let Err(error) = attempt {
            // Reload the last committed snapshot: a failed replacement must not publish pending changes.
            catalog = read_catalog(root, max_bytes)?;
            catalog.run.failures.push(FileFailure {
                path: input.to_string_lossy().chars().take(1024).collect(),
                error: error.to_string().chars().take(2048).collect(),
            });
            catalog.run.status = if matches!(policy, ErrorPolicy::FailFast) {
                "failed"
            } else {
                "running"
            }
            .into();
            publish(root, &mut catalog, max_bytes)?;
            if matches!(policy, ErrorPolicy::FailFast) {
                return Err(error);
            }
        }
    }
    catalog.run.status = if catalog.run.failures.is_empty() {
        "complete"
    } else {
        "partial"
    }
    .into();
    publish(root, &mut catalog, max_bytes)?;
    Ok(DatasetRun {
        summary,
        failures: catalog.run.failures,
    })
}

fn publish(root: &Path, catalog: &mut DatasetCatalog, max_bytes: usize) -> Result<()> {
    catalog.revision = catalog
        .revision
        .checked_add(1)
        .ok_or_else(|| invalid("catalog revision overflow"))?;
    let mut buffer = CappedBuffer {
        bytes: Vec::new(),
        limit: max_bytes,
    };
    serde_json::to_writer_pretty(&mut buffer, catalog).map_err(|e| invalid(e.to_string()))?;
    atomic_write(&root.join(CATALOG), &buffer.bytes)
}
struct CappedBuffer {
    bytes: Vec<u8>,
    limit: usize,
}
impl Write for CappedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other(
                "catalog exceeds configured metadata budget",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    atomic_write_checked(path, bytes, || Ok(()))
}

/// Stage and sync the complete output, then check cancellation before committing.
pub fn atomic_write_checked(
    path: &Path,
    bytes: &[u8],
    before_replace: impl FnOnce() -> std::io::Result<()>,
) -> Result<()> {
    let temp = crate::temp_output_path(path);
    let mut owned = false;
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        owned = true;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        inject_failure(2)?;
        before_replace()?;
        replace_file(&temp, path)
    })();
    if result.is_err() && owned {
        fs::remove_file(&temp).ok();
    }
    result
}
#[cfg(not(windows))]
fn replace_file(from: &Path, to: &Path) -> Result<()> {
    fs::rename(from, to)?;
    File::open(to.parent().unwrap_or(Path::new(".")))?.sync_all()?;
    Ok(())
}
#[cfg(windows)]
fn replace_file(from: &Path, to: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }
    let from: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
    // Same-directory replacement retains the previous snapshot if the operation fails.
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), 1 | 8) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}

struct CatalogGuard {
    _file: File,
}
impl CatalogGuard {
    fn acquire(root: &Path) -> Result<Self> {
        if fs::symlink_metadata(root.join(".catalog.guard"))
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err(invalid("catalog guard must not be a symlink"));
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(root.join(".catalog.guard"))?;
        file.try_lock()
            .map_err(|e| invalid(format!("dataset is locked: {e}")))?;
        file.set_len(0)?;
        writeln!(file, "pid={}", std::process::id())?;
        file.sync_all()?;
        Ok(Self { _file: file })
    }
}

/// Recover only while holding the kernel lock. Legacy writer owners must be dead.
pub fn recover_dataset(root: &Path) -> Result<Vec<PathBuf>> {
    let _guard = CatalogGuard::acquire(root)?;
    let mut removed = Vec::new();
    let objects = root.join("objects");
    if objects.exists() {
        if !fs::canonicalize(&objects)?.starts_with(fs::canonicalize(root)?) {
            return Err(invalid("object store escapes dataset"));
        }
        for entry in fs::read_dir(&objects)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir()
                || validate_hash(&entry.file_name().to_string_lossy()).is_err()
            {
                continue;
            }
            let dir = entry.path();
            let marker = dir.join(".zstdf-convert.lock");
            if marker.exists() {
                let text = fs::read_to_string(&marker)?;
                let pid: u32 = text
                    .trim()
                    .strip_prefix("pid=")
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| invalid("cannot verify legacy lock owner"))?;
                if process_alive(pid)? {
                    return Err(invalid(format!(
                        "writer process {pid} is still active; refusing recovery"
                    )));
                }
                fs::remove_file(&marker)?;
                removed.push(marker);
            }
            let _writer_guard = crate::dataset::DatasetLock::acquire(&dir)?;
            for item in fs::read_dir(&dir)? {
                let item = item?;
                if item.file_type()?.is_dir()
                    && item.file_name().to_string_lossy().starts_with(".staging-")
                {
                    if !fs::canonicalize(item.path())?.starts_with(fs::canonicalize(&dir)?) {
                        return Err(invalid("staging path escapes object directory"));
                    }
                    fs::remove_dir_all(item.path())?;
                    removed.push(item.path());
                }
            }
        }
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_file()
            && entry
                .file_name()
                .to_string_lossy()
                .starts_with("_catalog.json.tmp.")
        {
            fs::remove_file(entry.path())?;
            removed.push(entry.path());
        }
    }
    if root.join(CATALOG).exists() {
        let mut catalog = load_catalog(root)?;
        if catalog.run.status == "running" {
            catalog.run.status = "interrupted".into();
            publish(root, &mut catalog, MAX_CATALOG_BYTES)?;
        }
    }
    Ok(removed)
}
#[cfg(windows)]
fn process_alive(pid: u32) -> Result<bool> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut std::ffi::c_void;
        fn GetExitCodeProcess(handle: *mut std::ffi::c_void, code: *mut u32) -> i32;
        fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
    }
    let handle = unsafe { OpenProcess(0x1000, 0, pid) };
    if handle.is_null() {
        let error = std::io::Error::last_os_error();
        return if error.raw_os_error() == Some(87) {
            Ok(false)
        } else {
            Err(error.into())
        };
    }
    let mut code = 0;
    let ok = unsafe { GetExitCodeProcess(handle, &mut code) };
    let error = std::io::Error::last_os_error();
    unsafe {
        CloseHandle(handle);
    }
    if ok == 0 {
        Err(error.into())
    } else {
        Ok(code == 259)
    }
}
#[cfg(unix)]
fn process_alive(pid: u32) -> Result<bool> {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    let pid = i32::try_from(pid)
        .ok()
        .filter(|p| *p > 0)
        .ok_or_else(|| invalid("invalid process id"))?;
    if unsafe { kill(pid, 0) } == 0 {
        return Ok(true);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(3) {
        Ok(false)
    } else {
        Err(error.into())
    }
}

#[cfg(test)]
thread_local! { static FAILURE: std::cell::Cell<u8> = const { std::cell::Cell::new(0) }; }
pub(crate) fn inject_failure(_point: u8) -> Result<()> {
    #[cfg(test)]
    if FAILURE.with(|failure| {
        if failure.get() == _point {
            failure.set(0);
            true
        } else {
            false
        }
    }) {
        return Err(std::io::Error::other("injected disk-full/crash failure").into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "catalog-{}-{}",
                std::process::id(),
                crate::monotonic_nanos()
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }
    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
    #[test]
    fn failure_before_fragment_close_publishes_nothing_and_can_retry() {
        let f = Fixture::new();
        let input = f.0.join("one.stdf");
        let root = f.0.join("out");
        fs::write(
            &input,
            [
                2, 0, 0, 10, 2, 4, 12, 0, 15, 10, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 128, 63, 9, 0, 5,
                20, 1, 0, 0, 1, 0, 1, 0, 1, 0,
            ],
        )
        .unwrap();
        FAILURE.with(|v| v.set(3));
        assert!(convert_dataset(
            &[input.clone()],
            &root,
            &[],
            &FragmentOptions::default(),
            ErrorPolicy::FailFast
        )
        .is_err());
        assert!(load_catalog(&root).unwrap().sources.is_empty());
        for object in fs::read_dir(root.join("objects")).unwrap() {
            assert_eq!(fs::read_dir(object.unwrap().path()).unwrap().count(), 0);
        }
        let run = convert_dataset(
            &[input],
            &root,
            &[],
            &FragmentOptions::default(),
            ErrorPolicy::FailFast,
        )
        .unwrap();
        assert_eq!(run.summary.conversion.rows, 1);
    }
    #[test]
    fn missing_fragment_and_metadata_budget_are_rejected() {
        let f = Fixture::new();
        let mut catalog = DatasetCatalog::default();
        assert!(publish(&f.0, &mut catalog, 1).is_err());
        assert!(!f.0.join(CATALOG).exists());
        assert!(verified_path(&f.0, "missing.parquet").is_err());
    }
    #[test]
    fn recovery_accepts_a_terminated_owner() {
        let f = Fixture::new();
        #[cfg(windows)]
        let mut child = std::process::Command::new("cmd")
            .args(["/C", "exit", "0"])
            .spawn()
            .unwrap();
        #[cfg(unix)]
        let mut child = std::process::Command::new("sh")
            .args(["-c", "exit 0"])
            .spawn()
            .unwrap();
        let pid = child.id();
        child.wait().unwrap();
        drop(child);
        let dir = f.0.join("objects").join(sha256(b"dead-owner"));
        fs::create_dir_all(dir.join(".staging-orphan")).unwrap();
        fs::write(dir.join(".zstdf-convert.lock"), format!("pid={pid}")).unwrap();
        assert_eq!(recover_dataset(&f.0).unwrap().len(), 2);
    }
    #[test]
    fn atomic_failure_preserves_previous_snapshot() {
        let f = Fixture::new();
        let path = f.0.join(CATALOG);
        atomic_write(&path, b"old").unwrap();
        FAILURE.with(|v| v.set(2));
        assert!(atomic_write(&path, b"new").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"old");
        assert_eq!(fs::read_dir(&f.0).unwrap().count(), 1);
    }
    #[test]
    fn kernel_lock_blocks_concurrent_writer_and_releases_on_drop() {
        let f = Fixture::new();
        let guard = CatalogGuard::acquire(&f.0).unwrap();
        assert!(CatalogGuard::acquire(&f.0).is_err());
        assert!(recover_dataset(&f.0).is_err());
        drop(guard);
        assert!(CatalogGuard::acquire(&f.0).is_ok());
    }
    #[test]
    fn recovery_refuses_active_legacy_owner() {
        let f = Fixture::new();
        let dir = f.0.join("objects").join(sha256(b"object"));
        let stage = dir.join(".staging-interrupted");
        fs::create_dir_all(&stage).unwrap();
        let marker = dir.join(".zstdf-convert.lock");
        fs::write(&marker, format!("pid={}", std::process::id())).unwrap();
        assert!(recover_dataset(&f.0)
            .unwrap_err()
            .to_string()
            .contains("still active"));
        assert!(marker.exists() && stage.exists());
    }
    #[test]
    fn recovery_removes_only_unpublished_staging_and_marks_interruption() {
        let f = Fixture::new();
        let dir = f.0.join("objects").join(sha256(b"object"));
        fs::create_dir_all(dir.join(".staging-interrupted")).unwrap();
        fs::create_dir(dir.join("source-published")).unwrap();
        fs::write(f.0.join("_catalog.json.tmp.orphan"), b"partial").unwrap();
        let mut catalog = DatasetCatalog::default();
        catalog.run.status = "running".into();
        publish(&f.0, &mut catalog, MAX_CATALOG_BYTES).unwrap();
        assert_eq!(recover_dataset(&f.0).unwrap().len(), 2);
        assert!(dir.join("source-published").exists());
        assert_eq!(load_catalog(&f.0).unwrap().run.status, "interrupted");
        assert!(recover_dataset(&f.0).unwrap().is_empty());
    }
    #[test]
    fn recovery_rejects_unknown_owner() {
        let f = Fixture::new();
        let dir = f.0.join("objects").join(sha256(b"object"));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(".zstdf-convert.lock"), b"not a pid").unwrap();
        assert!(recover_dataset(&f.0).is_err());
        assert!(dir.join(".zstdf-convert.lock").exists());
    }
    #[test]
    fn failfast_records_error_and_stops_later_sources() {
        let f = Fixture::new();
        let valid = f.0.join("valid.stdf");
        fs::write(&valid, [2, 0, 0, 10, 2, 4]).unwrap();
        let root = f.0.join("out");
        assert!(convert_dataset(
            &[f.0.join("missing"), valid],
            &root,
            &[],
            &FragmentOptions::default(),
            ErrorPolicy::FailFast
        )
        .is_err());
        let catalog = load_catalog(&root).unwrap();
        assert_eq!(catalog.run.status, "failed");
        assert!(catalog.sources.is_empty());
    }
    #[test]
    fn committed_source_can_resume_after_catalog_failure() {
        let f = Fixture::new();
        let valid = f.0.join("valid.stdf");
        fs::write(&valid, [2, 0, 0, 10, 2, 4]).unwrap();
        let root = f.0.join("out");
        FAILURE.with(|v| v.set(1));
        assert!(convert_dataset(
            &[valid.clone()],
            &root,
            &[],
            &FragmentOptions::default(),
            ErrorPolicy::FailFast
        )
        .is_err());
        assert!(load_catalog(&root).unwrap().sources.is_empty());
        convert_dataset(
            &[valid],
            &root,
            &[],
            &FragmentOptions::default(),
            ErrorPolicy::FailFast,
        )
        .unwrap();
        assert_eq!(verify_catalog(&root).unwrap().sources.len(), 1);
        assert_eq!(fs::read_dir(root.join("objects")).unwrap().count(), 1);
    }
    #[test]
    fn unsupported_schema_and_unsafe_paths_fail_closed() {
        let f = Fixture::new();
        let mut catalog = DatasetCatalog::default();
        catalog.schema_version = "future".into();
        publish(&f.0, &mut catalog, MAX_CATALOG_BYTES).unwrap();
        assert!(load_catalog(&f.0).is_err());
        for path in ["../outside", "/absolute", ""] {
            assert!(verified_path(&f.0, path).is_err());
        }
    }
}
