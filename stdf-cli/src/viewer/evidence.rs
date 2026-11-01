// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
//! Immutable, source-scoped native viewer evidence. No Dashboard policy changes.
use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::OpenOptions;
use stdf_parquet::snapshot::{verify_snapshot, Snapshot};

const EVIDENCE_VERSION: &str = "viewer-evidence-v1";
const MIB: u64 = 1024 * 1024;
const MAX_METADATA: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct EvidenceLimits {
    pub memory_limit_mib: usize,
    pub disk_limit_mib: u64,
    pub cancel_file: Option<PathBuf>,
}
impl Default for EvidenceLimits {
    fn default() -> Self {
        Self {
            memory_limit_mib: 256,
            disk_limit_mib: 10240,
            cancel_file: None,
        }
    }
}
impl EvidenceLimits {
    fn check(&self) -> CliResult<()> {
        if self.cancel_file.as_ref().is_some_and(|p| p.exists()) {
            return Err("evidence operation cancelled".into());
        }
        Ok(())
    }
    fn validate(&self) -> CliResult<()> {
        if self.memory_limit_mib < 64 || self.disk_limit_mib < 64 {
            return Err("evidence needs at least 64 MiB memory and disk".into());
        }
        self.memory_limit_mib
            .checked_mul(MIB as usize)
            .ok_or("memory budget overflow")?;
        self.disk()?;
        self.check()
    }
    fn disk(&self) -> CliResult<u64> {
        self.disk_limit_mib
            .checked_mul(MIB)
            .ok_or_else(|| "disk budget overflow".into())
    }
    fn metadata(&self) -> usize {
        MAX_METADATA.min(self.memory_limit_mib.saturating_mul(MIB as usize) / 8)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct EvidenceFile {
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct EvidenceBundle {
    pub version: String,
    pub evidence_id: String,
    pub snapshot_id: String,
    pub source_id: String,
    pub source_sha256: String,
    pub content_sha256: String,
    pub viewer_version: String,
    pub coverage_scope: String,
    pub catalog_match: String,
    pub raw_retention: String,
    pub original_container_retention: String,
    pub snapshot_source_count: usize,
    pub catalog_run_status: String,
    pub catalog_failures: usize,
    pub records: u64,
    pub attempts: u64,
    pub measurements: u64,
    pub executions: u64,
    pub inventory: BTreeMap<String, u64>,
    pub files: BTreeMap<String, EvidenceFile>,
}

fn valid_id(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}
fn reject_link(path: &Path) -> CliResult<fs::Metadata> {
    let m = fs::symlink_metadata(path)?;
    if m.file_type().is_symlink() {
        return Err("evidence path must not be a symlink".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if m.file_attributes() & 0x400 != 0 {
            return Err("evidence path must not be a reparse point".into());
        }
    }
    Ok(m)
}
fn evidence_dir(dataset: &Path, create: bool) -> CliResult<PathBuf> {
    let dataset = dataset.canonicalize()?;
    let path = dataset.join("evidence");
    if create {
        match fs::create_dir(&path) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(e) => return Err(e.into()),
        }
    }
    if !reject_link(&path)?.is_dir() || !path.canonicalize()?.starts_with(&dataset) {
        return Err("unsafe evidence directory".into());
    }
    Ok(path)
}
fn bundle_dir(dataset: &Path, id: &str) -> CliResult<PathBuf> {
    if !valid_id(id) {
        return Err("evidence ID must be 64 lowercase hexadecimal characters".into());
    }
    let root = evidence_dir(dataset, false)?;
    let path = root.join(id);
    if !reject_link(&path)?.is_dir() || !path.canonicalize()?.starts_with(root.canonicalize()?) {
        return Err("unsafe evidence bundle directory".into());
    }
    Ok(path)
}
struct Guard(File);
impl Guard {
    fn acquire(dataset: &Path) -> CliResult<Self> {
        let path = dataset.join(".evidence.guard");
        if fs::symlink_metadata(&path).is_ok() && !reject_link(&path)?.is_file() {
            return Err("evidence guard must be a regular file".into());
        }
        let f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        f.try_lock()
            .map_err(|e| format!("evidence workspace is locked: {e}"))?;
        Ok(Self(f))
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

struct Stage(PathBuf);
impl Stage {
    fn new(root: &Path) -> CliResult<Self> {
        let path = root.join(format!(
            ".building-evidence-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        // This path was created by this invocation, beneath its canonical managed root.
        if reject_link(&self.0).is_ok_and(|m| m.is_dir()) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}
fn managed_bytes(root: &Path, limits: &EvidenceLimits) -> CliResult<u64> {
    fn walk(
        path: &Path,
        depth: usize,
        entries: &mut usize,
        limits: &EvidenceLimits,
    ) -> CliResult<u64> {
        if depth > 4 {
            return Err("evidence workspace nesting exceeds limit".into());
        }
        let mut bytes = 0u64;
        for entry in fs::read_dir(path)? {
            limits.check()?;
            *entries += 1;
            if *entries > 100_000 {
                return Err("evidence workspace entry limit exceeded".into());
            }
            let entry = entry?;
            let m = reject_link(&entry.path())?;
            let size = if m.is_file() {
                m.len()
            } else if m.is_dir() {
                walk(&entry.path(), depth + 1, entries, limits)?
            } else {
                return Err("non-regular evidence object".into());
            };
            bytes = bytes
                .checked_add(size)
                .ok_or("evidence byte count overflow")?;
            if bytes > limits.disk()? {
                return Err("evidence disk budget exceeded".into());
            }
        }
        Ok(bytes)
    }
    walk(root, 0, &mut 0, limits)
}
fn hash_file(path: &Path, limits: &EvidenceLimits) -> CliResult<EvidenceFile> {
    if !reject_link(path)?.is_file() {
        return Err("evidence object must be a regular file".into());
    }
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut bytes = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        limits.check()?;
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        bytes = bytes
            .checked_add(n as u64)
            .ok_or("evidence byte count overflow")?;
        if bytes > limits.disk()? {
            return Err("evidence object exceeds disk budget".into());
        }
        hash.update(&buffer[..n]);
    }
    Ok(EvidenceFile {
        sha256: format!("{:x}", hash.finalize()),
        bytes,
    })
}
fn read_json<T: serde::de::DeserializeOwned + Serialize>(
    path: &Path,
    limit: usize,
) -> CliResult<T> {
    if !reject_link(path)?.is_file() {
        return Err("metadata must be a regular file".into());
    }
    let file = File::open(path)?;
    if file.metadata()?.len() > limit as u64 {
        return Err("evidence metadata budget exceeded".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("evidence metadata budget exceeded".into());
    }
    let parsed: T = serde_json::from_slice(&bytes)?;
    let raw: Value = serde_json::from_slice(&bytes)?;
    if raw != serde_json::to_value(&parsed)? {
        return Err("unknown or noncanonical evidence metadata".into());
    }
    Ok(parsed)
}
fn identity(bundle: &EvidenceBundle) -> CliResult<String> {
    let mut value = serde_json::to_value(bundle)?;
    value.as_object_mut().unwrap().remove("evidence_id");
    let mut hash = Sha256::new();
    hash.update(b"zstdf:viewer-evidence-v1\0");
    hash.update(serde_json::to_vec(&value)?);
    Ok(format!("{:x}", hash.finalize()))
}
fn executions(inventory: &BTreeMap<String, u64>) -> CliResult<u64> {
    ["PTR", "MPR", "FTR"].iter().try_fold(0u64, |n, k| {
        n.checked_add(*inventory.get(*k).unwrap_or(&0))
            .ok_or_else(|| "execution count overflow".into())
    })
}
fn check_manifest(m: &Manifest) -> CliResult<()> {
    if m.version != VERSION
        || !valid_id(&m.source_hash)
        || !valid_id(&m.content_hash)
        || m.eav_files != ["measurements.parquet"]
    {
        return Err("incompatible native viewer evidence".into());
    }
    let mut required: BTreeSet<String> = [
        "source.stdf",
        "records.bin",
        "records.idx",
        "raw_attempts.bin",
        "raw_attempts.idx",
        "attempts.bin",
        "attempts.idx",
        "selection.bin",
        "measurements.parquet",
    ]
    .iter()
    .map(|s| (*s).into())
    .collect();
    if m.measurements > 0 {
        required.extend([
            "measurement_lookup.bin".into(),
            "measurement_lookup.idx".into(),
        ]);
    }
    if m.fragments.keys().map(String::as_str).collect::<Vec<_>>()
        != ["devices", "measurements", "records"]
    {
        return Err("incomplete native companion inventory".into());
    }
    for (name, fragments) in &m.fragments {
        let mut rows = 0u64;
        for f in fragments {
            if !safe_name(&f.path)
                || m.files.get(&f.path) != Some(&f.sha256)
                || !required.insert(f.path.clone())
            {
                return Err("invalid companion fragment inventory".into());
            }
            rows = rows
                .checked_add(f.rows as u64)
                .ok_or("companion row overflow")?;
        }
        let expected = match name.as_str() {
            "devices" => m.attempts,
            "measurements" => m.measurements,
            _ => m.records,
        };
        if rows != expected {
            return Err("companion count mismatch".into());
        }
    }
    if required != m.files.keys().cloned().collect()
        || m.files.keys().any(|n| !safe_name(n))
        || m.files.values().any(|h| !valid_id(h))
        || m.files.get("source.stdf") != Some(&m.content_hash)
        || m.inventory.get("PRR").copied().unwrap_or(0) != m.attempts
        || m.inventory
            .values()
            .try_fold(0u64, |n, v| n.checked_add(*v))
            != Some(m.records)
    {
        return Err("incomplete native evidence coverage".into());
    }
    Ok(())
}
fn files_for(
    root: &Path,
    m: &Manifest,
    limits: &EvidenceLimits,
) -> CliResult<BTreeMap<String, EvidenceFile>> {
    let stored: Manifest = read_json(&root.join("manifest.json"), limits.metadata())?;
    if serde_json::to_value(&stored)? != serde_json::to_value(m)? {
        return Err("native manifest changed while retaining evidence".into());
    }
    check_manifest(m)?;
    let mut files = BTreeMap::new();
    for name in m
        .files
        .keys()
        .map(String::as_str)
        .chain(std::iter::once("manifest.json"))
    {
        let meta = hash_file(&root.join(name), limits)?;
        if let Some(hash) = m.files.get(name) {
            if hash != &meta.sha256 {
                return Err("native evidence hash mismatch".into());
            }
        }
        files.insert(name.to_owned(), meta);
    }
    Ok(files)
}
fn canonical_native(m: &Manifest) -> CliResult<Value> {
    let mut m = m.clone();
    m.source.clear();
    m.source_hash = m.content_hash.clone();
    Ok(serde_json::to_value(m)?)
}
fn native(source: &Path, stage: &Stage, root: &Path, limits: &EvidenceLimits) -> CliResult<Cache> {
    let remaining = limits
        .disk()?
        .checked_sub(managed_bytes(root, limits)?)
        .and_then(|n| n.checked_sub((limits.metadata() as u64) * 2))
        .ok_or("evidence disk budget exceeded (including manifest publication reserve)")?;
    if remaining < MIB {
        return Err("insufficient evidence disk budget for indexing scratch".into());
    }
    ingest::prepare(&Arguments {
        input: source.into(),
        dataset: None,
        cache_dir: Some(stage.0.clone()),
        flow: None,
        export_html: None,
        export_scope: "selection".into(),
        export_size_mib: 20,
        memory_limit_mib: limits.memory_limit_mib,
        disk_limit_mib: remaining / MIB,
        cancel_file: limits.cancel_file.clone(),
        port: 0,
        no_open: true,
        telemetry_json: None,
    })
}
fn compare_catalog(
    dataset: &Path,
    snapshot: &Snapshot,
    source_id: &str,
    cache: &Cache,
) -> CliResult<()> {
    let source = snapshot
        .catalog
        .sources
        .get(source_id)
        .ok_or("snapshot has no requested source ID")?;
    let expected = source
        .fragments
        .iter()
        .map(|f| stdf_parquet::catalog::verified_path(dataset, &f.path))
        .collect::<Result<Vec<_>, _>>()?;
    if ingest::digest_parquet(&expected, cache)?
        != ingest::digest_parquet(&[cache.root.join("measurements.parquet")], cache)?
    {
        return Err("snapshot measurement coverage differs from native source evidence".into());
    }
    Ok(())
}

fn envelope(
    dataset: &Path,
    id: &str,
    limits: &EvidenceLimits,
) -> CliResult<(EvidenceBundle, Manifest, Snapshot, PathBuf)> {
    let root = bundle_dir(dataset, id)?;
    let b: EvidenceBundle = read_json(&root.join("bundle.json"), limits.metadata())?;
    if b.version != EVIDENCE_VERSION
        || b.evidence_id != id
        || identity(&b)? != id
        || b.viewer_version != VERSION
        || b.coverage_scope != "one-catalog-source"
        || b.catalog_match != "typed-row-multiset-v1"
        || b.raw_retention != "decompressed-content-retained"
        || b.original_container_retention != "external-required"
        || !valid_id(&b.source_id)
        || !valid_id(&b.source_sha256)
        || !valid_id(&b.content_sha256)
    {
        return Err("invalid evidence identity, version or coverage declaration".into());
    }
    let snapshot = verify_snapshot(dataset, &b.snapshot_id)?;
    limits.check()?;
    let source = snapshot
        .catalog
        .sources
        .get(&b.source_id)
        .ok_or("snapshot has no evidence source ID")?;
    if source.source_sha256 != b.source_sha256
        || snapshot.catalog.sources.len() != b.snapshot_source_count
        || snapshot.catalog.run.status != b.catalog_run_status
        || snapshot.catalog.run.failures.len() != b.catalog_failures
    {
        return Err("evidence snapshot/source binding mismatch".into());
    }
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(&root)? {
        limits.check()?;
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "non-ASCII evidence filename")?;
        if !safe_name(&name) || !reject_link(&entry.path())?.is_file() {
            return Err("unsafe retained evidence object".into());
        }
        names.insert(name);
        if names.len() > 100_000 {
            return Err("evidence object count exceeds limit".into());
        }
    }
    let mut expected: BTreeSet<_> = b.files.keys().cloned().collect();
    if expected.contains("bundle.json") || b.files.keys().any(|name| !safe_name(name)) {
        return Err("unsafe evidence inventory".into());
    }
    expected.insert("bundle.json".into());
    if names != expected {
        return Err("retained evidence inventory differs from descriptor".into());
    }
    for (name, meta) in &b.files {
        if !valid_id(&meta.sha256) || hash_file(&root.join(name), limits)? != *meta {
            return Err("retained evidence checksum or length mismatch".into());
        }
    }
    let m: Manifest = read_json(&root.join("manifest.json"), limits.metadata())?;
    check_manifest(&m)?;
    let mut declared: BTreeSet<_> = m.files.keys().cloned().collect();
    declared.insert("manifest.json".into());
    if declared != b.files.keys().cloned().collect()
        || m.files
            .iter()
            .any(|(n, h)| b.files.get(n).is_none_or(|f| &f.sha256 != h))
        || m.source_hash != b.source_sha256
        || m.content_hash != b.content_sha256
        || m.records != b.records
        || m.attempts != b.attempts
        || m.measurements != b.measurements
        || m.inventory != b.inventory
        || executions(&m.inventory)? != b.executions
    {
        return Err("retained evidence manifest/coverage mismatch".into());
    }
    Ok((b, m, snapshot, root))
}

#[cfg(test)]
thread_local! { static FAIL_PUBLISH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(test)]
pub(super) fn fail_next_publication() {
    FAIL_PUBLISH.with(|v| v.set(true));
}

pub(crate) fn retain(
    dataset: &Path,
    snapshot_id: &str,
    source_id: &str,
    source: &Path,
    limits: &EvidenceLimits,
) -> CliResult<EvidenceBundle> {
    limits.validate()?;
    let dataset = dataset.canonicalize()?;
    let snapshot = verify_snapshot(&dataset, snapshot_id)?;
    limits.check()?;
    let selected = snapshot
        .catalog
        .sources
        .get(source_id)
        .ok_or("snapshot has no requested source ID")?;
    let source = source.canonicalize()?;
    if hash_file(&source, limits)?.sha256 != selected.source_sha256 {
        return Err("input source hash does not match snapshot source ID".into());
    }
    let _guard = Guard::acquire(&dataset)?;
    let root = evidence_dir(&dataset, true)?;
    managed_bytes(&root, limits)?;
    let stage = Stage::new(&root)?;
    let cache = native(&source, &stage, &root, limits)?;
    if cache.manifest.source_hash != selected.source_sha256 {
        return Err("source changed while retaining evidence".into());
    }
    compare_catalog(&dataset, &snapshot, source_id, &cache)?;
    let files = files_for(&cache.root, &cache.manifest, limits)?;
    let mut bundle = EvidenceBundle {
        version: EVIDENCE_VERSION.into(),
        evidence_id: String::new(),
        snapshot_id: snapshot.snapshot_id.clone(),
        source_id: source_id.into(),
        source_sha256: cache.manifest.source_hash.clone(),
        content_sha256: cache.manifest.content_hash.clone(),
        viewer_version: VERSION.into(),
        coverage_scope: "one-catalog-source".into(),
        catalog_match: "typed-row-multiset-v1".into(),
        raw_retention: "decompressed-content-retained".into(),
        original_container_retention: "external-required".into(),
        snapshot_source_count: snapshot.catalog.sources.len(),
        catalog_run_status: snapshot.catalog.run.status.clone(),
        catalog_failures: snapshot.catalog.run.failures.len(),
        records: cache.manifest.records,
        attempts: cache.manifest.attempts,
        measurements: cache.manifest.measurements,
        executions: executions(&cache.manifest.inventory)?,
        inventory: cache.manifest.inventory.clone(),
        files,
    };
    bundle.evidence_id = identity(&bundle)?;
    let bytes = serde_json::to_vec(&bundle)?;
    if bytes.len() > limits.metadata() {
        return Err("evidence descriptor exceeds metadata budget".into());
    }
    let target = root.join(&bundle.evidence_id);
    if fs::symlink_metadata(&target).is_ok() {
        // Fresh source reconstruction above supplies the independent native validation.
        // Avoid another simultaneous reindex copy during an idempotent retention.
        let (existing, manifest, _, _) = envelope(&dataset, &bundle.evidence_id, limits)?;
        if existing != bundle || canonical_native(&manifest)? != canonical_native(&cache.manifest)?
        {
            return Err("existing evidence bundle differs; it will not be overwritten".into());
        }
        managed_bytes(&root, limits)?;
        return Ok(existing);
    }
    if managed_bytes(&root, limits)?
        .checked_add(bytes.len() as u64)
        .is_none_or(|n| n > limits.disk().unwrap_or(0))
    {
        return Err("evidence descriptor exceeds remaining disk budget".into());
    }
    let mut descriptor = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(cache.root.join("bundle.json"))?;
    descriptor.write_all(&bytes)?;
    descriptor.sync_all()?;
    drop(descriptor);
    for name in bundle.files.keys() {
        limits.check()?;
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(cache.root.join(name))?
            .sync_all()?;
    }
    managed_bytes(&root, limits)?;
    if hash_file(&source, limits)?.sha256 != selected.source_sha256 {
        return Err("source changed before evidence publication".into());
    }
    limits.check()?;
    #[cfg(test)]
    if FAIL_PUBLISH.with(|v| v.replace(false)) {
        return Err("injected failure before evidence publication".into());
    }
    // Guard serializes cooperating writers. Complete published directories are
    // nonempty and cannot be replaced by rename on either supported platform.
    if fs::symlink_metadata(&target).is_ok() {
        return Err("evidence publication conflict".into());
    }
    fs::rename(&cache.root, &target)?;
    #[cfg(unix)]
    File::open(&root)?.sync_all()?;
    Ok(bundle)
}

pub(crate) fn verify(
    dataset: &Path,
    id: &str,
    limits: &EvidenceLimits,
) -> CliResult<EvidenceBundle> {
    Ok(open_attempts(dataset, id, limits)?.bundle)
}

/// A bounded native reader whose local evidence lock lives through the scan.
/// Kept internal: public analysis types do not depend on the viewer's Row schema.
pub(super) struct VerifiedAttempts {
    pub bundle: EvidenceBundle,
    pub run_ids: BTreeSet<u64>,
    reader: store::IndexedReader,
    next: u64,
    limits: EvidenceLimits,
    _guard: Guard,
}
impl VerifiedAttempts {
    pub fn next(&mut self) -> CliResult<Option<Row>> {
        self.limits.check()?;
        if self.next > self.bundle.attempts {
            return Ok(None);
        }
        let row: Row = self.reader.get(self.next)?;
        if row.id != self.next || row.attempt != self.next || row.kind != "PRR" {
            return Err("retained attempt index returned a different PRR".into());
        }
        self.next = self.next.checked_add(1).ok_or("attempt sequence overflow")?;
        Ok(Some(row))
    }
}

pub(super) fn open_attempts(
    dataset: &Path,
    id: &str,
    limits: &EvidenceLimits,
) -> CliResult<VerifiedAttempts> {
    limits.validate()?;
    let dataset = dataset.canonicalize()?;
    // Fail before creating scratch/guard state for an invalid bundle path.
    bundle_dir(&dataset, id)?;
    let guard = Guard::acquire(&dataset)?;
    let root = evidence_dir(&dataset, false)?;
    managed_bytes(&root, limits)?;
    let (bundle, manifest, snapshot, path) = envelope(&dataset, id, limits)?;
    let stage = Stage::new(&root)?;
    let regenerated = native(&path.join("source.stdf"), &stage, &root, limits)?;
    files_for(&regenerated.root, &regenerated.manifest, limits)?;
    if canonical_native(&regenerated.manifest)? != canonical_native(&manifest)? {
        return Err("retained native evidence does not match source reconstruction; incompatible serializer or incomplete provenance".into());
    }
    compare_catalog(&dataset, &snapshot, &bundle.source_id, &regenerated)?;
    limits.check()?;
    managed_bytes(&root, limits)?;
    let run_ids = manifest
        .runs
        .iter()
        .map(|run| run["id"].as_u64().ok_or("invalid retained run ID"))
        .collect::<Result<BTreeSet<_>, _>>()?;
    let reader = store::IndexedReader::open(&path, "attempts")?;
    Ok(VerifiedAttempts {
        bundle,
        run_ids,
        reader,
        next: 1,
        limits: limits.clone(),
        _guard: guard,
    })
}

/// Names only; no data-integrity or completeness assertion.
pub(crate) fn list(dataset: &Path) -> CliResult<Vec<String>> {
    let dataset = dataset.canonicalize()?;
    match fs::symlink_metadata(dataset.join("evidence")) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e.into()),
        Ok(_) => (),
    }
    let mut ids = Vec::new();
    for (i, entry) in fs::read_dir(evidence_dir(&dataset, false)?)?.enumerate() {
        if i >= 1000 {
            return Err("evidence listing exceeds 1000 directory entry budget".into());
        }
        let entry = entry?;
        if let Some(name) = entry.file_name().to_str().filter(|n| valid_id(n)) {
            if !reject_link(&entry.path())?.is_dir() {
                return Err("evidence ID is not a directory".into());
            }
            ids.push(name.to_owned());
        }
    }
    ids.sort();
    Ok(ids)
}
pub(crate) fn record(
    dataset: &Path,
    id: &str,
    record_id: u64,
    limits: &EvidenceLimits,
) -> CliResult<Value> {
    let bundle = verify(dataset, id, limits)?;
    if record_id == 0 || record_id > bundle.records {
        return Err("record ID outside retained coverage".into());
    }
    let record: Evidence = store::lookup(&bundle_dir(dataset, id)?, "records", record_id)?;
    Ok(serde_json::to_value(record)?)
}
pub(crate) fn attempt(
    dataset: &Path,
    id: &str,
    attempt_id: u64,
    limits: &EvidenceLimits,
) -> CliResult<Value> {
    let bundle = verify(dataset, id, limits)?;
    if attempt_id == 0 || attempt_id > bundle.attempts {
        return Err("attempt ID outside retained coverage".into());
    }
    let row: Row = store::lookup(&bundle_dir(dataset, id)?, "attempts", attempt_id)?;
    Ok(serde_json::to_value(row)?)
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    #[test]
    fn native_manifest_budget_is_checked_before_retention_inventory() {
        let root = crate::tests::temp_path("evidence_manifest_limit");
        fs::create_dir(&root).unwrap();
        let limits = EvidenceLimits {
            memory_limit_mib: 64,
            ..Default::default()
        };
        let manifest: Manifest = serde_json::from_value(json!({
            "version": VERSION, "source":"test", "source_hash":"a".repeat(64),
            "content_hash":"b".repeat(64), "records":0,"attempts":0,"measurements":0,
            "inventory":{},"runs":[],"bin_definitions":[],"wafers":[],"fragments":{},
            "files":{},"eav_files":[],"population_counts":{}
        }))
        .unwrap();
        let path = root.join("manifest.json");
        let file = File::create(&path).unwrap();
        file.set_len(limits.metadata() as u64 + 1).unwrap();
        drop(file);
        assert!(files_for(&root, &manifest, &limits)
            .unwrap_err()
            .to_string()
            .contains("metadata budget"));
        fs::remove_file(path).unwrap();
        fs::remove_dir(root).unwrap();
    }
}
