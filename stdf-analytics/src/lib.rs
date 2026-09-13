// Copyright 2026 zstdf contributors
// SPDX-License-Identifier: Apache-2.0

//! Bounded scratch storage for the forthcoming disk-backed dashboard engine.
//! This crate does not change the existing dashboard's counting semantics.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

static NEXT: AtomicU64 = AtomicU64::new(0);
const OVERHEAD: usize = 192;
const HEADER: u64 = 24;

/// Accounted payload/working-state budgets, not an operating-system RSS limit.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub memory_bytes: usize,
    /// Maximum live scratch-file bytes, including simultaneous merge inputs/output.
    pub disk_bytes: u64,
    pub max_record_bytes: usize,
    /// Number of input files in a merge; at most this many plus one output are open.
    pub merge_fan_in: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Record {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
    sequence: u64,
}
impl Record {
    fn bytes(&self) -> u64 {
        HEADER + self.key.len() as u64 + self.value.len() as u64
    }
}

struct Workspace(PathBuf);
impl Workspace {
    fn path(&self, run: u64) -> PathBuf {
        self.0.join(format!("run-{run}.bin"))
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        // Only this invocation's atomically created directory is owned here.
        // A killed process can leave it behind; never scan/delete sibling jobs.
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Stable external sort over opaque binary keys and values.
///
/// Equal keys retain ingestion order, allowing the future analytics reducer to
/// preserve first-observation behavior without retaining every part in RAM.
/// Any I/O/quota error poisons the store; discard it rather than retrying a push.
pub struct SpillStore {
    workspace: Workspace,
    limits: Limits,
    cancel: Arc<AtomicBool>,
    chunk: Vec<Record>,
    charged: usize,
    sequence: u64,
    next_run: u64,
    disk: u64,
    peak_disk: u64,
    poisoned: bool,
}

impl SpillStore {
    pub fn new(parent: &Path, limits: Limits, cancel: Arc<AtomicBool>) -> io::Result<Self> {
        let working = limits
            .max_record_bytes
            .checked_add(OVERHEAD)
            .and_then(|n| n.checked_mul(limits.merge_fan_in.checked_add(1)?));
        if limits.disk_bytes == 0
            || limits.max_record_bytes == 0
            || !(2..=64).contains(&limits.merge_fan_in)
            || working.is_none_or(|bytes| bytes > limits.memory_bytes)
        {
            return Err(invalid(
                "invalid spill limits: memory must fit merge fronts and one output record",
            ));
        }
        let parent = fs::canonicalize(parent)?;
        let workspace = loop {
            let name = format!(
                ".zstdf-analytics-{}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            );
            let path = parent.join(name);
            match fs::create_dir(&path) {
                Ok(()) => break Workspace(path),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        };
        Ok(Self {
            workspace,
            limits,
            cancel,
            chunk: Vec::new(),
            charged: 0,
            sequence: 0,
            next_run: 0,
            disk: 0,
            peak_disk: 0,
            poisoned: false,
        })
    }

    pub fn push(&mut self, key: &[u8], value: &[u8]) -> io::Result<()> {
        let result = self.push_inner(key, value);
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }

    fn push_inner(&mut self, key: &[u8], value: &[u8]) -> io::Result<()> {
        self.check()?;
        let bytes = key
            .len()
            .checked_add(value.len())
            .ok_or_else(|| invalid("record limit overflow"))?;
        if bytes > self.limits.max_record_bytes {
            return Err(invalid("spill record limit exceeded"));
        }
        let charge = bytes + OVERHEAD;
        if self.charged > self.limits.memory_bytes - charge {
            self.flush()?;
        }
        let sequence = self.sequence;
        self.sequence = sequence
            .checked_add(1)
            .ok_or_else(|| invalid("record sequence overflow"))?;
        self.chunk.push(Record {
            key: key.to_vec(),
            value: value.to_vec(),
            sequence,
        });
        self.charged += charge;
        Ok(())
    }

    fn check(&self) -> io::Result<()> {
        cancelled(&self.cancel)?;
        if self.poisoned {
            return Err(invalid("spill store is poisoned by an earlier error"));
        }
        Ok(())
    }

    fn reserve_disk(&mut self, bytes: u64) -> io::Result<()> {
        if bytes > self.limits.disk_bytes.saturating_sub(self.disk) {
            return Err(io::Error::other("spill disk budget exceeded"));
        }
        self.disk += bytes;
        self.peak_disk = self.peak_disk.max(self.disk);
        Ok(())
    }

    fn create_run(&mut self) -> io::Result<(u64, File)> {
        let id = self.next_run;
        self.next_run = id
            .checked_add(1)
            .ok_or_else(|| invalid("run count overflow"))?;
        Ok((
            id,
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(self.workspace.path(id))?,
        ))
    }

    fn flush(&mut self) -> io::Result<()> {
        self.check()?;
        if self.chunk.is_empty() {
            return Ok(());
        }
        let mut chunk = std::mem::take(&mut self.chunk);
        chunk.sort_unstable_by(|a, b| a.key.cmp(&b.key).then(a.sequence.cmp(&b.sequence)));
        let (_, mut file) = self.create_run()?;
        for row in &chunk {
            self.check()?;
            self.reserve_disk(row.bytes())?;
            write_record(&mut file, row)?;
        }
        file.flush()?;
        self.charged = 0;
        Ok(())
    }

    /// Completes merging and transfers scratch ownership to the streaming reader.
    /// No dataset or dashboard destination is written by this API.
    pub fn finish(mut self) -> io::Result<SortedRecords> {
        self.flush()?;
        let mut start = 0;
        let mut end = self.next_run;
        // Run ranges, rather than a growing path list, bound metadata independently
        // of the number of input records/runs.
        while end - start > 1 {
            let next_start = self.next_run;
            for first in (start..end).step_by(self.limits.merge_fan_in) {
                let last = end.min(first.saturating_add(self.limits.merge_fan_in as u64));
                self.merge(first, last)?;
            }
            start = next_start;
            end = self.next_run;
        }
        self.check()?;
        let file = if start < end {
            Some(File::open(self.workspace.path(start))?)
        } else {
            None
        };
        Ok(SortedRecords {
            file,
            workspace: self.workspace,
            cancel: self.cancel,
            max_record_bytes: self.limits.max_record_bytes,
            peak_disk: self.peak_disk,
            failed: false,
            remaining: self.sequence,
        })
    }

    fn merge(&mut self, start: u64, end: u64) -> io::Result<()> {
        self.check()?;
        let mut readers = Vec::new();
        let mut fronts = Vec::new();
        let mut input_bytes = 0;
        for id in start..end {
            let mut file = File::open(self.workspace.path(id))?;
            input_bytes += file.metadata()?.len();
            fronts.push(read_record(&mut file, self.limits.max_record_bytes)?);
            readers.push(file);
        }
        let (_, mut output) = self.create_run()?;
        loop {
            self.check()?;
            let selected = fronts
                .iter()
                .enumerate()
                .filter_map(|(i, r)| r.as_ref().map(|r| (i, r)))
                .min_by(|(_, a), (_, b)| a.key.cmp(&b.key).then(a.sequence.cmp(&b.sequence)))
                .map(|(i, _)| i);
            let Some(i) = selected else {
                break;
            };
            let row = fronts[i].take().expect("selected nonempty front");
            self.reserve_disk(row.bytes())?;
            write_record(&mut output, &row)?;
            drop(row);
            fronts[i] = read_record(&mut readers[i], self.limits.max_record_bytes)?;
        }
        output.flush()?;
        drop(output);
        drop(readers);
        for id in start..end {
            fs::remove_file(self.workspace.path(id))?;
        }
        self.disk -= input_bytes;
        Ok(())
    }
}

/// Streaming replay. Drop closes the file before removing owned scratch files.
pub struct SortedRecords {
    file: Option<File>,
    workspace: Workspace,
    cancel: Arc<AtomicBool>,
    max_record_bytes: usize,
    peak_disk: u64,
    failed: bool,
    remaining: u64,
}
impl SortedRecords {
    pub fn next_record(&mut self) -> io::Result<Option<Record>> {
        cancelled(&self.cancel)?;
        if self.failed {
            return Err(invalid("scratch replay failed previously"));
        }
        let result = match self.file.as_mut() {
            Some(file) => read_record(file, self.max_record_bytes),
            None => Ok(None),
        }
        .and_then(|row| match (&row, self.remaining) {
            (None, 0) => Ok(row),
            (Some(_), 0) => Err(invalid("scratch has unexpected extra records")),
            (None, _) => Err(invalid("scratch record count mismatch")),
            (Some(_), _) => {
                self.remaining -= 1;
                Ok(row)
            }
        });
        self.failed = result.is_err();
        result
    }
    pub fn peak_disk_bytes(&self) -> u64 {
        self.peak_disk
    }
    /// Owned scratch path for diagnostics; callers must not modify its contents.
    pub fn scratch_path(&self) -> &Path {
        &self.workspace.0
    }
}

fn cancelled(flag: &AtomicBool) -> io::Result<()> {
    if flag.load(Ordering::Relaxed) {
        Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "analytics cancelled",
        ))
    } else {
        Ok(())
    }
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn write_record(file: &mut File, row: &Record) -> io::Result<()> {
    let mut header = [0; 24];
    header[..8].copy_from_slice(&(row.key.len() as u64).to_le_bytes());
    header[8..16].copy_from_slice(&(row.value.len() as u64).to_le_bytes());
    header[16..].copy_from_slice(&row.sequence.to_le_bytes());
    file.write_all(&header)?;
    file.write_all(&row.key)?;
    file.write_all(&row.value)
}
fn read_record(file: &mut File, max: usize) -> io::Result<Option<Record>> {
    let mut header = [0; 24];
    if file.read(&mut header[..1])? == 0 {
        return Ok(None);
    }
    file.read_exact(&mut header[1..])?;
    let key = u64::from_le_bytes(header[..8].try_into().unwrap());
    let value = u64::from_le_bytes(header[8..16].try_into().unwrap());
    if key.checked_add(value).is_none_or(|n| n > max as u64) {
        return Err(invalid("scratch record limit exceeded"));
    }
    let sequence = u64::from_le_bytes(header[16..].try_into().unwrap());
    let mut row = Record {
        key: vec![0; key as usize],
        value: vec![0; value as usize],
        sequence,
    };
    file.read_exact(&mut row.key)?;
    file.read_exact(&mut row.value)?;
    Ok(Some(row))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn store(disk_bytes: u64) -> SpillStore {
        SpillStore::new(
            &std::env::temp_dir(),
            Limits {
                memory_bytes: 4096,
                disk_bytes,
                max_record_bytes: 256,
                merge_fan_in: 2,
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
    }
    #[test]
    fn merge_quota_includes_live_inputs() {
        let mut s = store(4000);
        for _ in 0..20 {
            s.push(b"a", &[0; 100]).unwrap();
        }
        // 2500 bytes of input fit, but their merged copy does not.
        assert!(s
            .finish()
            .err()
            .unwrap()
            .to_string()
            .contains("disk budget"));
    }
    #[test]
    fn corrupt_length_is_rejected_before_allocation() {
        let mut s = store(10000);
        s.push(b"a", b"b").unwrap();
        s.flush().unwrap();
        let mut file = OpenOptions::new()
            .write(true)
            .open(s.workspace.path(0))
            .unwrap();
        file.write_all(&u64::MAX.to_le_bytes()).unwrap();
        drop(file);
        let mut r = s.finish().unwrap();
        assert!(r
            .next_record()
            .unwrap_err()
            .to_string()
            .contains("record limit"));
        assert!(r
            .next_record()
            .unwrap_err()
            .to_string()
            .contains("previously"));
    }
    #[test]
    fn partial_headers_and_payloads_are_not_eof() {
        for len in [1, 8, 23, 24, 25] {
            let mut s = store(10000);
            s.push(b"abc", b"def").unwrap();
            s.flush().unwrap();
            OpenOptions::new()
                .write(true)
                .open(s.workspace.path(0))
                .unwrap()
                .set_len(len)
                .unwrap();
            let mut r = s.finish().unwrap();
            assert_eq!(
                r.next_record().unwrap_err().kind(),
                io::ErrorKind::UnexpectedEof
            );
        }
    }
    #[test]
    fn prior_error_poisons_store() {
        let mut s = store(10000);
        assert!(s.push(&[0; 257], b"").is_err());
        assert!(s
            .push(b"a", b"b")
            .unwrap_err()
            .to_string()
            .contains("poisoned"));
        assert!(s.finish().is_err());
    }
    #[test]
    fn record_at_exact_limit_is_accepted() {
        let mut s = store(10000);
        s.push(&[1; 128], &[2; 128]).unwrap();
        assert_eq!(
            s.finish()
                .unwrap()
                .next_record()
                .unwrap()
                .unwrap()
                .value
                .len(),
            128
        );
    }
    #[test]
    fn missing_run_causes_error_and_owned_cleanup() {
        let mut s = store(10000);
        s.push(b"a", b"b").unwrap();
        s.flush().unwrap();
        let path = s.workspace.0.clone();
        fs::remove_file(s.workspace.path(0)).unwrap();
        assert!(s.finish().is_err());
        assert!(!path.exists());
    }

    #[test]
    fn whole_record_truncation_is_detected() {
        let mut s = store(10000);
        s.push(b"a", b"b").unwrap();
        s.flush().unwrap();
        OpenOptions::new()
            .write(true)
            .open(s.workspace.path(0))
            .unwrap()
            .set_len(0)
            .unwrap();
        let mut r = s.finish().unwrap();
        assert!(r
            .next_record()
            .unwrap_err()
            .to_string()
            .contains("count mismatch"));
    }

    #[test]
    fn unexpected_extra_record_is_detected() {
        let mut s = store(10000);
        s.push(b"a", b"b").unwrap();
        s.flush().unwrap();
        let mut file = OpenOptions::new()
            .append(true)
            .open(s.workspace.path(0))
            .unwrap();
        write_record(
            &mut file,
            &Record {
                key: vec![],
                value: vec![],
                sequence: 1,
            },
        )
        .unwrap();
        drop(file);
        let mut r = s.finish().unwrap();
        assert!(r.next_record().unwrap().is_some());
        assert!(r
            .next_record()
            .unwrap_err()
            .to_string()
            .contains("extra records"));
    }
}
