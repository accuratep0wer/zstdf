// Copyright 2026 zstdf contributors
// SPDX-License-Identifier: Apache-2.0

//! Immutable selections of existing managed EAV objects, not raw STDF archives.
use crate::catalog::{load_catalog, verify_catalog_data, CatalogGuard, DatasetCatalog};
use crate::Result;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

const VERSION: &str = "catalog-snapshot-v1";
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 1000;
const DOMAIN: &[u8] = b"zstdf:catalog-snapshot-v1\0";

#[cfg(test)]
thread_local! {
    static FAIL_BEFORE_PUBLICATION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub version: String,
    pub snapshot_id: String,
    pub expected_inventory: String,
    pub raw_retention: String,
    pub catalog: DatasetCatalog,
}

#[derive(Serialize)]
struct Payload<'a> {
    version: &'a str,
    expected_inventory: &'a str,
    raw_retention: &'a str,
    catalog: &'a DatasetCatalog,
}

fn invalid(message: impl Into<String>) -> crate::StdfParquetError {
    std::io::Error::new(std::io::ErrorKind::InvalidData, message.into()).into()
}

struct Limited(Vec<u8>);
impl Write for Limited {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_BYTES.saturating_sub(self.0.len()) {
            return Err(std::io::Error::other(
                "snapshot exceeds 16 MiB metadata budget",
            ));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    let mut out = Limited(Vec::new());
    serde_json::to_writer(&mut out, value).map_err(|e| invalid(e.to_string()))?;
    Ok(out.0)
}
fn identity(snapshot: &Snapshot) -> Result<String> {
    let payload = Payload {
        version: &snapshot.version,
        expected_inventory: &snapshot.expected_inventory,
        raw_retention: &snapshot.raw_retention,
        catalog: &snapshot.catalog,
    };
    let mut hash = Sha256::new();
    hash.update(DOMAIN);
    hash.update(encode(&payload)?);
    Ok(format!("{:x}", hash.finalize()))
}
fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn reject_link(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(invalid("snapshot path must not be a symlink"));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid("snapshot path must not be a reparse point"));
        }
    }
    Ok(())
}
fn directory(root: &Path, create: bool) -> Result<PathBuf> {
    let root = fs::canonicalize(root)?;
    let dir = root.join("snapshots");
    if create {
        match fs::create_dir(&dir) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.into()),
        }
    }
    reject_link(&dir)?;
    if !dir.is_dir() || !fs::canonicalize(&dir)?.starts_with(&root) {
        return Err(invalid(
            "snapshot directory escapes dataset or is not a directory",
        ));
    }
    Ok(dir)
}
fn validate_selection(catalog: &DatasetCatalog, allow_incomplete: bool) -> Result<()> {
    match catalog.run.status.as_str() {
        "complete" => (),
        "partial" | "failed" | "interrupted" if allow_incomplete => (),
        "partial" | "failed" | "interrupted" => {
            return Err(invalid("incomplete run requires --allow-incomplete"))
        }
        "running" => return Err(invalid("cannot snapshot a running catalog")),
        _ => return Err(invalid("unknown catalog run status")),
    }
    if !allow_incomplete && !catalog.run.failures.is_empty() {
        return Err(invalid(
            "catalog contains failures; capture requires --allow-incomplete",
        ));
    }
    if catalog.sources.is_empty() {
        return Err(invalid("cannot snapshot an empty source selection"));
    }
    for source in catalog.sources.values() {
        for fragment in &source.fragments {
            let path = Path::new(&fragment.path);
            let mut components = path.components();
            if components.next() != Some(Component::Normal(std::ffi::OsStr::new("objects")))
                || components.clone().next().is_none()
                || components.any(|p| !matches!(p, Component::Normal(_)))
            {
                return Err(invalid(
                    "snapshot fragments must use managed objects/ paths",
                ));
            }
        }
    }
    Ok(())
}

fn verify_managed_paths(root: &Path, catalog: &DatasetCatalog) -> Result<()> {
    let objects = root.join("objects");
    reject_link(&objects)?;
    let objects = fs::canonicalize(objects)?;
    if !objects.starts_with(fs::canonicalize(root)?) {
        return Err(invalid("managed object directory escapes dataset"));
    }
    for source in catalog.sources.values() {
        for fragment in &source.fragments {
            let path = crate::catalog::verified_path(root, &fragment.path)?;
            if !path.starts_with(&objects) {
                return Err(invalid("snapshot fragment escapes managed objects"));
            }
        }
    }
    Ok(())
}

/// Capture one verified current catalog under the existing writer lock.
/// Does not copy or retain raw STDF, viewer caches, or unreferenced objects.
pub fn create_snapshot(root: &Path, allow_incomplete: bool) -> Result<Snapshot> {
    let _guard = CatalogGuard::acquire(root)?;
    let catalog = load_catalog(root)?;
    validate_selection(&catalog, allow_incomplete)?;
    verify_managed_paths(root, &catalog)?;
    verify_catalog_data(root, &catalog)?;
    let mut snapshot = Snapshot {
        version: VERSION.into(),
        snapshot_id: String::new(),
        expected_inventory: "unknown".into(),
        raw_retention: "external-required".into(),
        catalog,
    };
    snapshot.snapshot_id = identity(&snapshot)?;
    let bytes = encode(&snapshot)?;
    let dir = directory(root, true)?;
    let target = dir.join(format!("{}.json", snapshot.snapshot_id));
    if fs::symlink_metadata(&target).is_ok() {
        let existing = verify_snapshot(root, &snapshot.snapshot_id)?;
        if encode(&existing)? != bytes {
            return Err(invalid("existing snapshot content mismatch"));
        }
        return Ok(existing);
    }
    let temp = dir.join(format!(
        ".snapshot-{}-{}.tmp",
        std::process::id(),
        crate::monotonic_nanos()
    ));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temp)?;
    let result = (|| -> Result<()> {
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        #[cfg(test)]
        if FAIL_BEFORE_PUBLICATION.with(|flag| flag.replace(false)) {
            return Err(invalid("injected failure before snapshot publication"));
        }
        // Hard-link publication is atomic and must never replace an existing ID.
        match fs::hard_link(&temp, &target) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = verify_snapshot(root, &snapshot.snapshot_id)?;
                if encode(&existing)? != bytes {
                    return Err(invalid("snapshot publication conflict"));
                }
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    })();
    // Remove only the temporary descriptor created by this invocation.
    let cleanup = fs::remove_file(&temp);
    result?;
    cleanup?;
    #[cfg(unix)]
    File::open(&dir)?.sync_all()?;
    Ok(snapshot)
}

/// Verify a saved selection without reading the current catalog or raw sources.
pub fn verify_snapshot(root: &Path, id: &str) -> Result<Snapshot> {
    if !valid_id(id) {
        return Err(invalid(
            "snapshot ID must be 64 lowercase hexadecimal characters",
        ));
    }
    let dir = directory(root, false)?;
    let path = dir.join(format!("{id}.json"));
    reject_link(&path)?;
    let file = File::open(&path)?;
    if file.metadata()?.len() > MAX_BYTES as u64 {
        return Err(invalid("snapshot exceeds 16 MiB metadata budget"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err(invalid("snapshot exceeds 16 MiB metadata budget"));
    }
    let snapshot: Snapshot = serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
    // Comparing the decoded JSON to typed serialization rejects ignored nested
    // fields; the content address hashes typed canonical data, not whitespace.
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
    if value != serde_json::to_value(&snapshot).map_err(|e| invalid(e.to_string()))? {
        return Err(invalid("noncanonical snapshot payload"));
    }
    if snapshot.version != VERSION
        || snapshot.expected_inventory != "unknown"
        || snapshot.raw_retention != "external-required"
        || snapshot.snapshot_id != id
        || identity(&snapshot)? != id
    {
        return Err(invalid("snapshot version, coverage or identity mismatch"));
    }
    validate_selection(&snapshot.catalog, true)?;
    verify_managed_paths(root, &snapshot.catalog)?;
    verify_catalog_data(root, &snapshot.catalog)?;
    Ok(snapshot)
}

/// List descriptor names only. Call `verify_snapshot` to check their evidence.
pub fn list_snapshots(root: &Path) -> Result<Vec<String>> {
    let root = fs::canonicalize(root)?;
    match fs::symlink_metadata(root.join("snapshots")) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
        Ok(_) => (),
    }
    let dir = directory(&root, false)?;
    let mut result = Vec::new();
    for (count, entry) in fs::read_dir(dir)?.enumerate() {
        if count >= MAX_ENTRIES {
            return Err(invalid("snapshot listing exceeds 1000 entry budget"));
        }
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        if valid_id(id) {
            reject_link(&entry.path())?;
            if entry.file_type()?.is_file() {
                result.push(id.to_owned());
            }
        }
    }
    result.sort();
    Ok(result)
}

const POINTER: &str = "_current_snapshot.json";
const POINTER_VERSION: &str = "catalog-snapshot-pointer-v1";
const MAX_POINTER_BYTES: usize = 4096;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotPointer {
    version: String,
    snapshot_id: String,
}

/// One verified fragment in the captured catalog's original source/fragment order.
#[derive(Debug, Clone)]
pub struct ResolvedFragment {
    pub source_id: String,
    pub path: PathBuf,
    pub rows: usize,
    pub sha256: String,
}

/// Read-only metadata handle. It pins a generation choice, not file contents:
/// external mutation is still possible and no GC protection is promised.
#[derive(Debug)]
pub struct SnapshotSelection {
    snapshot: Snapshot,
    fragments: Vec<ResolvedFragment>,
}
impl SnapshotSelection {
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn fragments(&self) -> &[ResolvedFragment] {
        &self.fragments
    }
}

fn check_pointer_path(path: &Path, may_be_missing: bool) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(e) if may_be_missing && e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
        Ok(metadata) => {
            reject_link(path)?;
            if !metadata.is_file() {
                return Err(invalid("current snapshot pointer must be a regular file"));
            }
            Ok(())
        }
    }
}

#[cfg(test)]
thread_local! {
    static FAIL_POINTER_PUBLICATION: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Verify and atomically select an existing immutable snapshot. Conversion and
/// snapshot creation do not otherwise update this explicit pointer.
pub fn select_snapshot(root: &Path, id: &str, allow_incomplete: bool) -> Result<Snapshot> {
    let root = fs::canonicalize(root)?;
    let _guard = CatalogGuard::acquire(&root)?;
    let snapshot = verify_snapshot(&root, id)?;
    validate_selection(&snapshot.catalog, allow_incomplete)?;
    let path = root.join(POINTER);
    check_pointer_path(&path, true)?;
    let bytes = encode(&SnapshotPointer {
        version: POINTER_VERSION.into(),
        snapshot_id: snapshot.snapshot_id.clone(),
    })?;
    if bytes.len() > MAX_POINTER_BYTES {
        return Err(invalid("current snapshot pointer exceeds 4096 byte budget"));
    }
    crate::catalog::atomic_write_checked(&path, &bytes, || {
        check_pointer_path(&path, true).map_err(|e| std::io::Error::other(e.to_string()))?;
        #[cfg(test)]
        if FAIL_POINTER_PUBLICATION.with(|flag| flag.replace(false)) {
            return Err(std::io::Error::other(
                "injected failure before pointer publication",
            ));
        }
        Ok(())
    })?;
    Ok(snapshot)
}

/// Read the pointer once and verify that exact snapshot; never fall back to the
/// mutable catalog or the newest descriptor. Incomplete status stays visible.
pub fn current_snapshot(root: &Path) -> Result<Snapshot> {
    let root = fs::canonicalize(root)?;
    let path = root.join(POINTER);
    check_pointer_path(&path, false)?;
    let file = File::open(&path)?;
    if file.metadata()?.len() > MAX_POINTER_BYTES as u64 {
        return Err(invalid("current snapshot pointer exceeds 4096 byte budget"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_POINTER_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_POINTER_BYTES {
        return Err(invalid("current snapshot pointer exceeds 4096 byte budget"));
    }
    let pointer: SnapshotPointer =
        serde_json::from_slice(&bytes).map_err(|e| invalid(e.to_string()))?;
    if pointer.version != POINTER_VERSION || !valid_id(&pointer.snapshot_id) {
        return Err(invalid("invalid current snapshot pointer version or ID"));
    }
    verify_snapshot(&root, &pointer.snapshot_id)
}

/// Resolve an exact verified catalog inventory, without discovery or pointer
/// rereads. The returned handle exposes no mutable references.
pub fn resolve_snapshot(root: &Path, id: Option<&str>) -> Result<SnapshotSelection> {
    let root = fs::canonicalize(root)?;
    let snapshot = match id {
        Some(id) => verify_snapshot(&root, id)?,
        None => current_snapshot(&root)?,
    };
    let mut fragments = Vec::new();
    for (source_id, source) in &snapshot.catalog.sources {
        for fragment in &source.fragments {
            fragments.push(ResolvedFragment {
                source_id: source_id.clone(),
                path: crate::catalog::verified_path(&root, &fragment.path)?,
                rows: fragment.rows,
                sha256: fragment.sha256.clone(),
            });
        }
    }
    Ok(SnapshotSelection {
        snapshot,
        fragments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{CatalogFragment, SourceVersion};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "snapshot-{}-{}",
                std::process::id(),
                crate::monotonic_nanos()
            ));
            fs::create_dir_all(path.join("objects/one")).unwrap();
            let f = Self(path);
            f.catalog(0);
            f
        }
        fn catalog(&self, revision: u64) -> DatasetCatalog {
            let relative = Path::new("objects")
                .join("one")
                .join(format!("{revision}.parquet"))
                .to_str()
                .unwrap()
                .to_owned();
            use arrow::array::{
                ArrayRef, BooleanArray, Float32Array, Int16Array, StringArray, UInt16Array,
                UInt32Array, UInt64Array, UInt8Array,
            };
            use arrow::datatypes::DataType;
            use std::sync::Arc;
            let rows = revision as usize + 1;
            let schema = stdf_arrow::eav_schema();
            let arrays: Vec<ArrayRef> = schema
                .fields()
                .iter()
                .map(|field| -> ArrayRef {
                    match field.data_type() {
                        DataType::Utf8 => Arc::new(StringArray::from(vec!["synthetic"; rows])),
                        DataType::Boolean => Arc::new(BooleanArray::from(vec![true; rows])),
                        DataType::UInt8 => Arc::new(UInt8Array::from(vec![1; rows])),
                        DataType::UInt16 => Arc::new(UInt16Array::from(vec![1; rows])),
                        DataType::UInt32 => Arc::new(UInt32Array::from(vec![1; rows])),
                        DataType::UInt64 => Arc::new(UInt64Array::from(vec![1; rows])),
                        DataType::Int16 => Arc::new(Int16Array::from(vec![1; rows])),
                        DataType::Float32 => Arc::new(Float32Array::from(vec![1.0; rows])),
                        other => panic!("update synthetic fixture for {other:?}"),
                    }
                })
                .collect();
            let batch = arrow::record_batch::RecordBatch::try_new(schema, arrays).unwrap();
            crate::write_record_batches_to_path(self.0.join(&relative), &[batch]).unwrap();
            let mut catalog = DatasetCatalog::default();
            catalog.revision = revision;
            let source = format!("external-source-{revision}.stdf");
            catalog.sources.insert(
                format!("{:x}", Sha256::digest(source.as_bytes())),
                SourceVersion {
                    source_path: source,
                    source_sha256: "a".repeat(64),
                    options_sha256: "b".repeat(64),
                    rows,
                    fragments: vec![CatalogFragment {
                        sha256: crate::catalog::sha256_file(&self.0.join(&relative)).unwrap(),
                        path: relative,
                        rows,
                    }],
                },
            );
            self.save(&catalog);
            catalog
        }
        fn save(&self, catalog: &DatasetCatalog) {
            fs::write(
                self.0.join("_catalog.json"),
                serde_json::to_vec(catalog).unwrap(),
            )
            .unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }
    #[test]
    fn immutable_idempotent_selection_survives_current_replacement_and_removal() {
        let f = Fixture::new();
        let first = create_snapshot(&f.0, false).unwrap();
        assert_eq!(first.expected_inventory, "unknown");
        assert_eq!(first.raw_retention, "external-required");
        assert_eq!(
            first.snapshot_id,
            create_snapshot(&f.0, false).unwrap().snapshot_id
        );
        f.catalog(1);
        let second = create_snapshot(&f.0, false).unwrap();
        assert_ne!(first.snapshot_id, second.snapshot_id);
        assert_eq!(first.catalog.sources.values().next().unwrap().rows, 1);
        assert_eq!(second.catalog.sources.values().next().unwrap().rows, 2);
        fs::remove_file(f.0.join("_catalog.json")).unwrap();
        assert_eq!(
            verify_snapshot(&f.0, &first.snapshot_id)
                .unwrap()
                .catalog
                .revision,
            0
        );
        assert_eq!(list_snapshots(&f.0).unwrap().len(), 2);
    }
    #[test]
    fn status_empty_lock_and_managed_path_guards() {
        let f = Fixture::new();
        let mut c = load_catalog(&f.0).unwrap();
        for status in ["partial", "failed", "interrupted"] {
            c.run.status = status.into();
            f.save(&c);
            assert!(create_snapshot(&f.0, false).is_err());
            assert!(create_snapshot(&f.0, true).is_ok());
        }
        for status in ["running", "invented"] {
            c.run.status = status.into();
            f.save(&c);
            assert!(create_snapshot(&f.0, true).is_err());
        }
        c.run.status = "complete".into();
        f.save(&c);
        let guard = CatalogGuard::acquire(&f.0).unwrap();
        assert!(create_snapshot(&f.0, false).is_err());
        drop(guard);
        c.sources.values_mut().next().unwrap().fragments[0].path = "other.parquet".into();
        f.save(&c);
        assert!(create_snapshot(&f.0, false).is_err());
        c.sources.clear();
        f.save(&c);
        assert!(create_snapshot(&f.0, true).is_err());
    }
    #[test]
    fn corrupt_fragment_or_descriptor_cannot_replace_prior_snapshot() {
        let f = Fixture::new();
        let first = create_snapshot(&f.0, false).unwrap();
        let path =
            f.0.join("snapshots")
                .join(format!("{}.json", first.snapshot_id));
        let bytes = fs::read(&path).unwrap();
        let fragment = &first.catalog.sources.values().next().unwrap().fragments[0].path;
        fs::write(f.0.join(fragment), b"bad parquet").unwrap();
        assert!(verify_snapshot(&f.0, &first.snapshot_id).is_err());
        assert!(create_snapshot(&f.0, false).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        f.catalog(0);
        let mut tampered: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        tampered["catalog"]["ignored"] = true.into();
        fs::write(&path, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert!(verify_snapshot(&f.0, &first.snapshot_id).is_err());
        assert!(create_snapshot(&f.0, false).is_err());
        assert!(fs::read_dir(f.0.join("snapshots")).unwrap().all(|e| !e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")));
    }
    #[test]
    fn bounded_metadata_and_invalid_ids() {
        let f = Fixture::new();
        assert!(list_snapshots(&f.0).unwrap().is_empty());
        assert!(!f.0.join("snapshots").exists());
        for id in ["../escape", "ABC", "", &"A".repeat(64)] {
            assert!(verify_snapshot(&f.0, id).is_err());
        }
        let first = create_snapshot(&f.0, false).unwrap();
        let path =
            f.0.join("snapshots")
                .join(format!("{}.json", first.snapshot_id));
        OpenOptions::new()
            .write(true)
            .open(path)
            .unwrap()
            .set_len(MAX_BYTES as u64 + 1)
            .unwrap();
        assert!(verify_snapshot(&f.0, &first.snapshot_id).is_err());
        assert!(encode(&"x".repeat(MAX_BYTES)).is_err());
    }
    #[test]
    fn saved_status_version_paths_and_hash_changes_are_rejected() {
        let f = Fixture::new();
        let original = create_snapshot(&f.0, false).unwrap();
        for mode in 0..4 {
            let mut changed = original.clone();
            match mode {
                0 => changed.catalog.run.status = "running".into(),
                1 => changed.catalog.schema_version = "eav-old".into(),
                2 => {
                    changed
                        .catalog
                        .sources
                        .values_mut()
                        .next()
                        .unwrap()
                        .fragments[0]
                        .path = "objects/../../escape.parquet".into()
                }
                _ => changed.expected_inventory = "complete".into(),
            }
            changed.snapshot_id = identity(&changed).unwrap();
            fs::write(
                f.0.join("snapshots")
                    .join(format!("{}.json", changed.snapshot_id)),
                encode(&changed).unwrap(),
            )
            .unwrap();
            assert!(verify_snapshot(&f.0, &changed.snapshot_id).is_err());
        }
        assert!(verify_snapshot(&f.0, &original.snapshot_id).is_ok());
    }
    #[test]
    fn listing_has_a_hard_entry_budget() {
        let f = Fixture::new();
        fs::create_dir(f.0.join("snapshots")).unwrap();
        for i in 0..=MAX_ENTRIES {
            fs::write(f.0.join("snapshots").join(format!("{i:064x}.json")), b"{}").unwrap();
        }
        assert!(list_snapshots(&f.0).is_err());
    }
    #[cfg(unix)]
    #[test]
    fn symlinked_snapshot_directory_and_internal_object_escape_are_rejected() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        fs::create_dir(f.0.join("outside")).unwrap();
        symlink(f.0.join("outside"), f.0.join("snapshots")).unwrap();
        assert!(create_snapshot(&f.0, false).is_err());
        assert!(list_snapshots(&f.0).is_err());
        fs::remove_file(f.0.join("snapshots")).unwrap();
        let mut c = load_catalog(&f.0).unwrap();
        let fragment = &mut c.sources.values_mut().next().unwrap().fragments[0];
        fs::rename(f.0.join(&fragment.path), f.0.join("outside/data.parquet")).unwrap();
        symlink(f.0.join("outside/data.parquet"), f.0.join(&fragment.path)).unwrap();
        f.save(&c);
        assert!(create_snapshot(&f.0, false).is_err());
    }

    #[test]
    fn failure_after_temp_sync_preserves_prior_evidence_and_only_cleans_owned_temp() {
        let f = Fixture::new();
        let first = create_snapshot(&f.0, false).unwrap();
        let saved =
            f.0.join("snapshots")
                .join(format!("{}.json", first.snapshot_id));
        let saved_bytes = fs::read(&saved).unwrap();
        f.catalog(1);
        let catalog_before = fs::read(f.0.join("_catalog.json")).unwrap();
        let other_temp = f.0.join("snapshots/.snapshot-unrelated.tmp");
        fs::write(&other_temp, b"another task").unwrap();
        FAIL_BEFORE_PUBLICATION.with(|flag| flag.set(true));
        let error = create_snapshot(&f.0, false).unwrap_err();
        assert!(error.to_string().contains("injected failure"));
        assert_eq!(fs::read(f.0.join("_catalog.json")).unwrap(), catalog_before);
        assert_eq!(fs::read(&saved).unwrap(), saved_bytes);
        assert_eq!(fs::read(&other_temp).unwrap(), b"another task");
        assert_eq!(
            list_snapshots(&f.0).unwrap(),
            vec![first.snapshot_id.clone()]
        );
        assert_eq!(fs::read_dir(f.0.join("snapshots")).unwrap().count(), 2);
        verify_snapshot(&f.0, &first.snapshot_id).unwrap();
        assert!(create_snapshot(&f.0, false).is_ok());
    }

    #[test]
    fn complete_status_does_not_hide_recorded_failures() {
        let f = Fixture::new();
        let mut catalog = load_catalog(&f.0).unwrap();
        catalog.run.failures.push(crate::catalog::FileFailure {
            path: "missing-input.stdf".into(),
            error: "input unavailable".into(),
        });
        f.save(&catalog);
        assert!(create_snapshot(&f.0, false).is_err());
        assert!(!f.0.join("snapshots").exists());
        let snapshot = create_snapshot(&f.0, true).unwrap();
        assert_eq!(snapshot.catalog.run.status, "complete");
        assert_eq!(snapshot.catalog.run.failures.len(), 1);
        assert_eq!(
            verify_snapshot(&f.0, &snapshot.snapshot_id)
                .unwrap()
                .catalog
                .run
                .failures
                .len(),
            1
        );
    }
    #[test]
    fn selected_pointer_and_old_handle_retain_exact_generations() {
        let f = Fixture::new();
        let first = create_snapshot(&f.0, false).unwrap();
        select_snapshot(&f.0, &first.snapshot_id, false).unwrap();
        let handle = resolve_snapshot(&f.0, None).unwrap();
        f.catalog(1);
        let second = create_snapshot(&f.0, false).unwrap();
        assert_eq!(
            current_snapshot(&f.0).unwrap().snapshot_id,
            first.snapshot_id
        );
        select_snapshot(&f.0, &second.snapshot_id, false).unwrap();
        assert_eq!(
            current_snapshot(&f.0).unwrap().snapshot_id,
            second.snapshot_id
        );
        assert_eq!(handle.snapshot().snapshot_id, first.snapshot_id);
        assert_eq!(handle.fragments().len(), 1);
        assert_eq!(handle.fragments()[0].rows, 1);
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
            File::open(&handle.fragments()[0].path).unwrap(),
        )
        .unwrap();
        assert_eq!(reader.metadata().file_metadata().num_rows(), 1);
        assert_eq!(
            resolve_snapshot(&f.0, Some(&first.snapshot_id))
                .unwrap()
                .fragments()[0]
                .rows,
            1
        );
        fs::remove_file(f.0.join("_catalog.json")).unwrap();
        assert_eq!(resolve_snapshot(&f.0, None).unwrap().fragments()[0].rows, 2);
    }
    #[test]
    fn pointer_prepublication_failures_preserve_previous_selection() {
        let f = Fixture::new();
        let first = create_snapshot(&f.0, false).unwrap();
        select_snapshot(&f.0, &first.snapshot_id, false).unwrap();
        let before = fs::read(f.0.join(POINTER)).unwrap();
        f.catalog(1);
        let second = create_snapshot(&f.0, false).unwrap();
        let catalog_before = fs::read(f.0.join("_catalog.json")).unwrap();
        let entries_before = fs::read_dir(&f.0).unwrap().count();
        FAIL_POINTER_PUBLICATION.with(|flag| flag.set(true));
        assert!(select_snapshot(&f.0, &second.snapshot_id, false)
            .unwrap_err()
            .to_string()
            .contains("injected failure"));
        assert_eq!(fs::read(f.0.join(POINTER)).unwrap(), before);
        assert_eq!(fs::read(f.0.join("_catalog.json")).unwrap(), catalog_before);
        assert_eq!(fs::read_dir(&f.0).unwrap().count(), entries_before);
        assert!(select_snapshot(&f.0, &"f".repeat(64), false).is_err());
        let guard = CatalogGuard::acquire(&f.0).unwrap();
        assert!(select_snapshot(&f.0, &second.snapshot_id, false).is_err());
        drop(guard);
        assert_eq!(fs::read(f.0.join(POINTER)).unwrap(), before);
        verify_snapshot(&f.0, &first.snapshot_id).unwrap();
        verify_snapshot(&f.0, &second.snapshot_id).unwrap();
        let second_path =
            f.0.join("snapshots")
                .join(format!("{}.json", second.snapshot_id));
        fs::write(second_path, b"corrupted evidence").unwrap();
        assert!(select_snapshot(&f.0, &second.snapshot_id, false).is_err());
        assert_eq!(fs::read(f.0.join(POINTER)).unwrap(), before);
    }
    #[test]
    fn selecting_incomplete_evidence_requires_opt_in_but_reading_retains_status() {
        let f = Fixture::new();
        let first = create_snapshot(&f.0, false).unwrap();
        select_snapshot(&f.0, &first.snapshot_id, false).unwrap();
        let mut catalog = load_catalog(&f.0).unwrap();
        catalog.run.failures.push(crate::catalog::FileFailure {
            path: "missing.stdf".into(),
            error: "test failure".into(),
        });
        for status in ["complete", "partial"] {
            catalog.run.status = status.into();
            f.save(&catalog);
            let incomplete = create_snapshot(&f.0, true).unwrap();
            let before = fs::read(f.0.join(POINTER)).unwrap();
            assert!(select_snapshot(&f.0, &incomplete.snapshot_id, false).is_err());
            assert_eq!(fs::read(f.0.join(POINTER)).unwrap(), before);
            select_snapshot(&f.0, &incomplete.snapshot_id, true).unwrap();
            let read = current_snapshot(&f.0).unwrap();
            assert_eq!(read.catalog.run.status, status);
            assert_eq!(read.catalog.run.failures.len(), 1);
        }
    }
    #[test]
    fn invalid_pointer_never_falls_back_or_creates_state() {
        let f = Fixture::new();
        let first = create_snapshot(&f.0, false).unwrap();
        let entries_before = fs::read_dir(&f.0).unwrap().count();
        assert!(current_snapshot(&f.0).is_err());
        assert_eq!(fs::read_dir(&f.0).unwrap().count(), entries_before);
        let malformed = vec![
            "not JSON".to_owned(),
            format!(
                r#"{{"version":"old","snapshot_id":"{}"}}"#,
                first.snapshot_id
            ),
            format!(
                r#"{{"version":"{POINTER_VERSION}","snapshot_id":"{}","extra":1}}"#,
                first.snapshot_id
            ),
            format!(
                r#"{{"version":"{POINTER_VERSION}","snapshot_id":"{}","snapshot_id":"{}"}}"#,
                first.snapshot_id, first.snapshot_id
            ),
            format!(
                r#"{{"version":"{POINTER_VERSION}","snapshot_id":"{}"}}"#,
                "f".repeat(64)
            ),
            format!(r#"{{"version":"{POINTER_VERSION}","snapshot_id":"../escape"}}"#),
            " ".repeat(MAX_POINTER_BYTES + 1),
        ];
        for bytes in malformed {
            fs::write(f.0.join(POINTER), &bytes).unwrap();
            assert!(current_snapshot(&f.0).is_err());
            assert!(resolve_snapshot(&f.0, None).is_err());
            assert_eq!(fs::read_to_string(f.0.join(POINTER)).unwrap(), bytes);
            assert!(resolve_snapshot(&f.0, Some(&first.snapshot_id)).is_ok());
        }
        fs::remove_file(f.0.join(POINTER)).unwrap();
        fs::create_dir(f.0.join(POINTER)).unwrap();
        assert!(current_snapshot(&f.0).is_err());
        assert!(select_snapshot(&f.0, &first.snapshot_id, false).is_err());
    }
    #[test]
    fn resolved_inventory_follows_captured_order_and_survives_dataset_relocation() {
        let mut f = Fixture::new();
        let mut catalog = load_catalog(&f.0).unwrap();
        let extra = f.catalog(1);
        catalog.sources.extend(extra.sources);
        f.save(&catalog);
        let saved = create_snapshot(&f.0, false).unwrap();
        select_snapshot(&f.0, &saved.snapshot_id, false).unwrap();
        let moved = f.0.with_file_name(format!(
            "snapshot-relocated-{}-{}",
            std::process::id(),
            crate::monotonic_nanos()
        ));
        fs::rename(&f.0, &moved).unwrap();
        f.0 = moved;
        let handle = resolve_snapshot(&f.0, None).unwrap();
        assert_eq!(handle.snapshot().snapshot_id, saved.snapshot_id);
        let expected: Vec<_> = saved
            .catalog
            .sources
            .iter()
            .flat_map(|(id, source)| {
                source
                    .fragments
                    .iter()
                    .map(move |fragment| (id.as_str(), fragment.rows, fragment.sha256.as_str()))
            })
            .collect();
        let actual: Vec<_> = handle
            .fragments()
            .iter()
            .map(|fragment| {
                (
                    fragment.source_id.as_str(),
                    fragment.rows,
                    fragment.sha256.as_str(),
                )
            })
            .collect();
        assert_eq!(actual, expected);
        assert!(handle
            .fragments()
            .iter()
            .all(|fragment| fragment.path.starts_with(fs::canonicalize(&f.0).unwrap())));
    }
    #[cfg(unix)]
    #[test]
    fn pointer_symlinks_including_dangling_targets_are_rejected() {
        use std::os::unix::fs::symlink;
        let f = Fixture::new();
        let first = create_snapshot(&f.0, false).unwrap();
        symlink(f.0.join("absent.json"), f.0.join(POINTER)).unwrap();
        assert!(current_snapshot(&f.0).is_err());
        assert!(select_snapshot(&f.0, &first.snapshot_id, false).is_err());
        assert!(!f.0.join("absent.json").exists());
    }
}
