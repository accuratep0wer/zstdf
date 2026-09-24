// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
//! Single-source engineering viewer. Legacy dashboard semantics are independent.
use crate::CliResult;
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{atomic::AtomicBool, Arc};
use stdf_validate::fields::Field;

mod ingest;
mod query;
mod store;
#[cfg(test)]
mod tests;
mod web;

const VERSION: &str = "viewer-v5";
#[derive(Debug, Args)]
pub struct Arguments {
    input: PathBuf,
    #[arg(long)]
    dataset: Option<PathBuf>,
    #[arg(long)]
    cache_dir: Option<PathBuf>,
    #[arg(long, value_parser=["CP", "FT", "cp", "ft"])]
    flow: Option<String>,
    #[arg(long)]
    export_html: Option<PathBuf>,
    #[arg(long, default_value="selection",value_parser=["selection","summary"])]
    export_scope: String,
    #[arg(long, default_value_t = 20)]
    export_size_mib: usize,
    #[arg(long, default_value_t = 256)]
    memory_limit_mib: usize,
    #[arg(long, default_value_t = 10240)]
    disk_limit_mib: u64,
    #[arg(long)]
    cancel_file: Option<PathBuf>,
    #[arg(long, default_value_t = 0)]
    port: u16,
    #[arg(long)]
    no_open: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct Row {
    id: u64,
    record: u64,
    attempt: u64,
    run: u64,
    kind: String,
    test: String,
    number: Option<u32>,
    name: String,
    channel: Option<u32>,
    pattern: Option<String>,
    head: u8,
    site: u8,
    lot: String,
    wafer: Option<String>,
    x: Option<i64>,
    y: Option<i64>,
    part: Option<String>,
    ecid: Option<String>,
    start: Option<u32>,
    passed: Option<bool>,
    hard_bin: Option<u16>,
    soft_bin: Option<u16>,
    duration: Option<u32>,
    value: Option<f64>,
    raw: Option<f64>,
    value_state: String,
    low: Option<f64>,
    high: Option<f64>,
    units: String,
    flags: u8,
    alarm: bool,
    offset: u64,
    latest: Option<bool>,
    first: Option<bool>,
    last_execution: bool,
    issue: Option<String>,
    eav_fragment: Option<String>,
    eav_row: Option<u64>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Evidence {
    id: u64,
    offset: u64,
    len: u16,
    typ: u8,
    sub: u8,
    kind: String,
    run: u64,
    attempt: u64,
    fields: Vec<Field>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Fragment {
    path: String,
    rows: usize,
    sha256: String,
    first: u64,
    last: u64,
    tests: BTreeSet<String>,
    runs: BTreeSet<u64>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Manifest {
    version: String,
    source: String,
    source_hash: String,
    content_hash: String,
    records: u64,
    attempts: u64,
    measurements: u64,
    inventory: BTreeMap<String, u64>,
    runs: Vec<Value>,
    #[serde(default)]
    bin_definitions: Vec<Value>,
    wafers: Vec<Option<String>>,
    fragments: BTreeMap<String, Vec<Fragment>>,
    files: BTreeMap<String, String>,
    eav_files: Vec<String>,
    population_counts: BTreeMap<String, u64>,
}
struct Cache {
    root: PathBuf,
    manifest: Manifest,
    memory: usize,
    disk: u64,
    cancel: Option<PathBuf>,
    cancellation: Arc<AtomicBool>,
    stop_watch: Arc<AtomicBool>,
    query_pages: std::cell::RefCell<BTreeMap<String, query::PageSnapshot>>,
}
impl Drop for Cache {
    fn drop(&mut self) {
        self.stop_watch
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
impl Cache {
    fn watch(&self) {
        if let Some(path) = self.cancel.clone() {
            let cancelled = self.cancellation.clone();
            let stop = self.stop_watch.clone();
            std::thread::spawn(move || {
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    if path.exists() {
                        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            });
        }
    }

    fn check(&self) -> CliResult<()> {
        if self.cancel.as_ref().is_some_and(|p| p.exists()) {
            return Err("viewer operation cancelled".into());
        }
        Ok(())
    }
    fn spill(&self) -> CliResult<stdf_analytics::SpillStore> {
        self.check()?;
        Ok(stdf_analytics::SpillStore::new(
            &self.root,
            stdf_analytics::Limits {
                memory_bytes: self.memory / 8,
                disk_bytes: (self.disk.saturating_sub(store::disk_bytes(&self.root)?)) / 8,
                max_record_bytes: 1024 * 1024,
                merge_fan_in: 4,
            },
            self.cancellation.clone(),
        )?)
    }
    fn evidence(&self, id: u64) -> CliResult<Evidence> {
        store::lookup(&self.root, "records", id)
    }
    #[cfg(test)]
    fn attempt(&self, id: u64) -> CliResult<Row> {
        store::lookup(&self.root, "attempts", id)
    }
}

pub fn execute(args: Arguments, out: &mut impl Write) -> CliResult<()> {
    if args.memory_limit_mib < 64 || args.disk_limit_mib < 64 || args.export_size_mib == 0 {
        return Err("viewer needs at least 64 MiB memory/disk and a positive export limit".into());
    }
    let cache = ingest::prepare(&args)?;
    writeln!(
        out,
        "Viewer: {} attempts, {} measurements; cache {}",
        cache.manifest.attempts,
        cache.manifest.measurements,
        cache.root.display()
    )?;
    if let Some(path) = &args.export_html {
        let q = query::Query::default();
        let html = web::export(
            &cache,
            &q,
            &args.export_scope,
            args.export_size_mib * 1024 * 1024,
            args.flow.as_deref(),
        )?;
        if path.canonicalize().ok() == args.input.canonicalize().ok() {
            return Err("report cannot replace source".into());
        }
        store::atomic_write_checked(path, html.as_bytes(), || {
            cache
                .check()
                .map_err(|e| std::io::Error::other(e.to_string()))
        })?;
        writeln!(out, "Offline viewer: {}", path.display())?;
        return Ok(());
    }
    web::serve(cache, &args, out)
}
