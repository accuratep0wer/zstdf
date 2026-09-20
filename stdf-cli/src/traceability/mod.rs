//! Direct STDF evidence pipeline, independent of dashboard yield aggregation.
use crate::CliResult;
use clap::Args;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use stdf_analytics::{Limits, SpillStore};

mod analysis;
mod config;
mod ingest;
#[cfg(test)]
mod tests;
use analysis::{Attempt, Run};
use config::{Closure, Flow, Identity, Mapping, Selector};

#[derive(Debug, Args)]
pub struct Arguments {
    #[arg(required = true)]
    inputs: Vec<PathBuf>,
    #[arg(long)]
    flow_config: PathBuf,
    #[arg(long)]
    output: PathBuf,
    #[arg(long)]
    flow_closures: Option<PathBuf>,
    #[arg(long)]
    identity_map: Option<PathBuf>,
    /// Accounted working data budget, not a hard process RSS ceiling.
    #[arg(long, default_value_t = 256)]
    memory_limit_mib: usize,
    #[arg(long, default_value_t = 1024)]
    disk_limit_mib: u64,
    #[arg(long, default_value_t = 32)]
    max_report_mib: usize,
    #[arg(long, default_value_t = 100_000)]
    max_devices: usize,
    #[arg(long, default_value_t = 10_000)]
    max_sources: usize,
    #[arg(long)]
    temp_dir: Option<PathBuf>,
    /// Cooperative cancellation: stop if this file exists (also checked before publication).
    #[arg(long)]
    cancel_file: Option<PathBuf>,
}
struct Budget {
    metadata: usize,
    metadata_limit: usize,
    group_limit: usize,
    cancel: Arc<AtomicBool>,
    cancel_file: Option<PathBuf>,
}
impl Budget {
    fn check(&self) -> CliResult<()> {
        if self.cancel_file.as_ref().is_some_and(|p| p.exists()) {
            self.cancel.store(true, Ordering::Relaxed);
        }
        if self.cancel.load(Ordering::Relaxed) {
            return Err("traceability cancelled; report not replaced".into());
        }
        Ok(())
    }
    fn charge(&mut self, bytes: usize) -> CliResult<()> {
        self.check()?;
        self.metadata = self
            .metadata
            .checked_add(bytes)
            .ok_or("metadata size overflow")?;
        if self.metadata > self.metadata_limit {
            return Err("traceability metadata memory limit exceeded".into());
        }
        Ok(())
    }
}
#[derive(Serialize)]
struct Evidence<'a> {
    schema: &'static str,
    identity_version: &'static str,
    flow: &'a Flow,
    closures: &'a [Closure],
    identity_map: &'a [Mapping],
    sources: &'a BTreeMap<String, Vec<String>>,
    runs: &'a BTreeMap<String, Run>,
}
pub fn execute(args: Arguments, out: &mut impl Write) -> CliResult<()> {
    generate(&args, out, Arc::new(AtomicBool::new(false)))
}
fn read_config<T: serde::de::DeserializeOwned>(path: &Path, budget: &mut Budget) -> CliResult<T> {
    let length = usize::try_from(fs::metadata(path)?.len())?;
    budget.charge(length.checked_mul(8).ok_or("config size overflow")?)?;
    let mut bytes = Vec::new();
    File::open(path)?
        .take(length as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > length {
        return Err("config changed while reading".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}
fn normalized(path: &Path) -> CliResult<PathBuf> {
    if path.exists() {
        return Ok(fs::canonicalize(path)?);
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok(fs::canonicalize(parent)?.join(path.file_name().ok_or("output has no filename")?))
}
fn collect(
    path: &Path,
    found: &mut BTreeSet<PathBuf>,
    args: &Arguments,
    budget: &mut Budget,
) -> CliResult<()> {
    budget.check()?;
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Err("symlink inputs are not supported".into());
    }
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            if entry.file_type()?.is_symlink() {
                continue;
            }
            let child = entry.path();
            if child.is_dir() || crate::is_stdf_path(&child) {
                collect(&child, found, args, budget)?;
            }
        }
    } else {
        let path = fs::canonicalize(path)?;
        if found.insert(path.clone()) {
            budget.charge(path.as_os_str().len() * 8 + 256)?;
            if found.len() > args.max_sources {
                return Err("maximum source paths exceeded".into());
            }
        }
    }
    Ok(())
}
fn generate(args: &Arguments, out: &mut impl Write, cancel: Arc<AtomicBool>) -> CliResult<()> {
    let memory = args
        .memory_limit_mib
        .checked_mul(1024 * 1024)
        .ok_or("memory limit overflow")?;
    let report_limit = args
        .max_report_mib
        .checked_mul(1024 * 1024)
        .ok_or("report limit overflow")?;
    if memory < 16 * 1024 * 1024
        || report_limit == 0
        || report_limit > memory / 4
        || args.max_devices == 0
        || args.max_sources == 0
    {
        return Err("limits require memory >= 16 MiB, 0 < report <= memory/4, and positive device/source caps".into());
    }
    let mut budget = Budget {
        metadata: 0,
        metadata_limit: memory / 8,
        group_limit: memory / 8,
        cancel: cancel.clone(),
        cancel_file: args.cancel_file.clone(),
    };
    budget.check()?;
    let flow: Flow = read_config(&args.flow_config, &mut budget)?;
    flow.validate()?;
    let mappings: Vec<Mapping> = args
        .identity_map
        .as_ref()
        .map(|p| read_config(p, &mut budget))
        .transpose()?
        .unwrap_or_default();
    let aliases = config::mappings(&mappings)?;
    let closures: Vec<Closure> = args
        .flow_closures
        .as_ref()
        .map(|p| read_config(p, &mut budget))
        .transpose()?
        .unwrap_or_default();
    config::validate_closures(&closures, &flow)?;
    let output = normalized(&args.output)?;
    let mut paths = BTreeSet::new();
    for input in &args.inputs {
        collect(input, &mut paths, args, &mut budget)?;
    }
    if paths.is_empty() {
        return Err("no STDF inputs found".into());
    }
    for path in paths
        .iter()
        .chain(std::iter::once(&args.flow_config))
        .chain(args.identity_map.iter())
        .chain(args.flow_closures.iter())
    {
        if normalized(path)? == output {
            return Err("report output must not overwrite an input or configuration".into());
        }
    }
    let mut sources: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for path in paths {
        let hash = ingest::hash(&path, &budget)?;
        sources
            .entry(hash)
            .or_default()
            .push(path.to_string_lossy().into_owned());
        budget.charge(512)?;
    }
    for paths in sources.values_mut() {
        paths.sort();
        paths.dedup();
    }
    let scratch = args.temp_dir.clone().unwrap_or_else(std::env::temp_dir);
    // Reserve space for the atomic HTML staging file as well as sorted records.
    let disk = args
        .disk_limit_mib
        .checked_mul(1024 * 1024)
        .ok_or("disk limit overflow")?;
    let scratch_disk = disk
        .checked_sub(report_limit as u64)
        .filter(|n| *n > 0)
        .ok_or("disk limit must exceed the maximum report size")?;
    let mut store = SpillStore::new(
        &scratch,
        Limits {
            memory_bytes: memory / 8,
            disk_bytes: scratch_disk,
            max_record_bytes: 128 * 1024,
            merge_fan_in: 8,
        },
        cancel,
    )?;
    let mut runs = BTreeMap::new();
    for (hash, paths) in &sources {
        ingest::read(
            Path::new(&paths[0]),
            hash,
            &flow,
            &aliases,
            &mut runs,
            &mut store,
            &mut budget,
        )?;
    }
    budget.check()?;
    let mut sorted = store.finish()?;
    let evidence = Evidence {
        schema: "traceability-v1",
        identity_version: stdf_arrow::identity::IDENTITY_VERSION,
        flow: &flow,
        closures: &closures,
        identity_map: &mappings,
        sources: &sources,
        runs: &runs,
    };
    let mut json = serde_json::to_vec(&evidence)?;
    json.pop();
    json.extend_from_slice(b",\"devices\":[");
    let mut key: Option<String> = None;
    let mut attempts = Vec::new();
    let mut group_bytes = 0usize;
    let mut count = 0;
    let mut emit = |key: String, attempts: Vec<Attempt>, json: &mut Vec<u8>| -> CliResult<()> {
        budget.check()?;
        count += 1;
        if count > args.max_devices {
            return Err("maximum report devices exceeded".into());
        }
        let device = analysis::reduce(key, attempts, &flow, &closures, &aliases, &runs);
        if count > 1 {
            json.push(b',');
        }
        serde_json::to_writer(
            LimitedWriter {
                bytes: json,
                limit: report_limit,
            },
            &device,
        )?;
        Ok(())
    };
    while let Some(record) = sorted.next_record()? {
        budget.check()?;
        let next = String::from_utf8(record.key)?;
        if key.as_ref().is_some_and(|k| *k != next) {
            emit(
                key.take().unwrap(),
                std::mem::take(&mut attempts),
                &mut json,
            )?;
            group_bytes = 0;
        }
        key = Some(next);
        group_bytes = group_bytes
            .checked_add(record.value.len() * 8 + 1024)
            .ok_or("group size overflow")?;
        if group_bytes > budget.group_limit {
            return Err("device group memory limit exceeded".into());
        }
        attempts.push(serde_json::from_slice(&record.value)?);
    }
    if let Some(key) = key {
        emit(key, attempts, &mut json)?;
    }
    json.extend_from_slice(b"]}");
    // Bound escaping too: adversarial '<' strings can expand sixfold.
    let template = crate::report_ui::decorate(include_str!("report.html").to_owned());
    let (prefix, suffix) = template.split_once("__TRACE_DATA__").unwrap();
    let mut html = Vec::new();
    let mut writer = LimitedWriter {
        bytes: &mut html,
        limit: report_limit,
    };
    writer.write_all(prefix.as_bytes())?;
    for byte in &json {
        match byte {
            b'&' => writer.write_all(b"\\u0026")?,
            b'<' => writer.write_all(b"\\u003c")?,
            b'>' => writer.write_all(b"\\u003e")?,
            _ => writer.write_all(std::slice::from_ref(byte))?,
        }
    }
    writer.write_all(suffix.as_bytes())?;
    drop(json);
    budget.check()?;
    stdf_parquet::catalog::atomic_write_checked(&output, &html, || {
        budget
            .check()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Interrupted, e.to_string()))
    })?;
    writeln!(
        out,
        "Traceability: {count} devices, {} unique sources; {}",
        sources.len(),
        output.display()
    )?;
    Ok(())
}
struct LimitedWriter<'a> {
    bytes: &'a mut Vec<u8>,
    limit: usize,
}
impl Write for LimitedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) > self.limit {
            return Err(std::io::Error::other("report size limit exceeded"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
