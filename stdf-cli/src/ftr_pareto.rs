// Copyright 2026 zstdf contributors
// SPDX-License-Identifier: Apache-2.0
//! FTR attempt metrics, independent of PTR EAV and device yield.
use crate::CliResult;
use clap::Args;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use stdf_core::StdfRecord;
use stdf_io::StreamingRecordReader;

#[derive(Debug, Args)]
pub struct Arguments {
    /// STDF files or directories, including gzip. Exact content copies count once.
    #[arg(required = true)]
    pub inputs: Vec<PathBuf>,
    #[arg(long)]
    pub output: PathBuf,
    /// Maximum source/scoped-pattern groups (not a process RSS limit).
    #[arg(long, default_value_t = 50_000)]
    pub max_groups: usize,
    #[arg(long, default_value_t = 256)]
    pub max_report_mib: usize,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
struct Counts {
    attempts: u64,
    pass: u64,
    fail: u64,
    unknown: u64,
    not_executed: u64,
    alarms: u64,
}
impl Counts {
    fn push(&mut self, flag: u8) {
        self.attempts += 1;
        self.alarms += u64::from(flag & 1 != 0);
        if flag & 0x10 != 0 {
            self.not_executed += 1;
        } else if flag & 0x6e != 0 {
            // Reserved, unreliable, timeout, abort, or invalid pass/fail flag.
            self.unknown += 1;
        } else if flag & 0x80 != 0 {
            self.fail += 1;
        } else {
            self.pass += 1;
        }
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct Scope {
    pattern: Option<String>,
    run: u64,
    lot: String,
    program: String,
    revision: String,
    head: u8,
    site: u8,
    test_num: u32,
}
#[derive(Debug, Serialize)]
struct Group {
    #[serde(flatten)]
    scope: Scope,
    #[serde(flatten)]
    counts: Counts,
    first_offset: usize,
    last_offset: usize,
}
#[derive(Debug, Serialize)]
struct Source {
    sha256: String,
    paths: Vec<PathBuf>,
    groups: Vec<Group>,
}
#[derive(Debug, Serialize)]
struct Report {
    schema: &'static str,
    sources: Vec<Source>,
}

struct HashReader<R> {
    inner: R,
    digest: Sha256,
}
impl<R: Read> Read for HashReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.digest.update(&buf[..n]);
        Ok(n)
    }
}

fn discover(inputs: &[PathBuf]) -> CliResult<Vec<PathBuf>> {
    let mut pending = inputs.to_vec();
    let mut visited = BTreeSet::new();
    let mut files = BTreeSet::new();
    let mut entries = 0;
    while let Some(path) = pending.pop() {
        let path = path.canonicalize()?;
        if !visited.insert(path.clone()) {
            continue;
        }
        if path.is_dir() {
            for entry in std::fs::read_dir(path)? {
                let entry = entry?;
                entries += 1;
                if entries > 100_000 {
                    return Err("FTR directory entry limit exceeded (100000)".into());
                }
                if entry.file_type()?.is_symlink() {
                    continue;
                }
                let p = entry.path();
                if p.is_dir() || crate::is_stdf_path(&p) {
                    pending.push(p);
                }
            }
        } else {
            files.insert(path);
            if files.len() > 4096 {
                return Err("FTR source limit exceeded (4096)".into());
            }
        }
    }
    if files.is_empty() {
        return Err("no STDF inputs found".into());
    }
    Ok(files.into_iter().collect())
}

fn scan(path: &Path, max_groups: usize) -> CliResult<Source> {
    let mut file = File::open(path)?;
    let mut magic = [0; 2];
    file.read_exact(&mut magic)?;
    file.rewind()?;
    let input: Box<dyn Read> = if magic == [0x1f, 0x8b] {
        Box::new(flate2::read::MultiGzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut hash = HashReader {
        inner: input,
        digest: Sha256::new(),
    };
    let mut groups: BTreeMap<Scope, Group> = BTreeMap::new();
    let (mut run, mut lot, mut program, mut revision) =
        (0, String::new(), String::new(), String::new());
    {
        let mut reader = StreamingRecordReader::new(&mut hash)?;
        while let Some(event) = reader.next_event()? {
            if (event.header.rec_typ, event.header.rec_sub) == (15, 20) {
                for field in stdf_validate::fields::inspect_structure(
                    "FTR",
                    &event.body,
                    reader.byte_order(),
                ) {
                    if !field.issues.is_empty() {
                        return Err(format!(
                            "{} offset {} FTR {}: {}",
                            path.display(),
                            event.offset,
                            field.name,
                            field.issues.join("; ")
                        )
                        .into());
                    }
                }
            }
            let record = event
                .decoded
                .map_err(|e| format!("{} offset {}: {e}", path.display(), event.offset))?;
            match record {
                StdfRecord::Mir(m) => {
                    run += 1;
                    lot = m.lot_id;
                    program = m.job_nam;
                    revision = m.job_rev.unwrap_or_default();
                }
                StdfRecord::Ftr(f) => {
                    // VECT_NAM is not a semi-static default field; never inherit it.
                    let scope = Scope {
                        pattern: f.vect_nam.filter(|s| !s.trim().is_empty()),
                        run,
                        lot: lot.clone(),
                        program: program.clone(),
                        revision: revision.clone(),
                        head: f.head_num,
                        site: f.site_num,
                        test_num: f.test_num,
                    };
                    if !groups.contains_key(&scope) && groups.len() >= max_groups {
                        return Err(
                            "FTR scoped-pattern group limit exceeded; raise --max-groups".into(),
                        );
                    }
                    let group = groups.entry(scope.clone()).or_insert_with(|| Group {
                        scope,
                        counts: Counts::default(),
                        first_offset: event.offset,
                        last_offset: event.offset,
                    });
                    group.counts.push(f.test_flg);
                    group.last_offset = event.offset;
                }
                _ => {}
            }
        }
    }
    Ok(Source {
        sha256: format!("{:x}", hash.digest.finalize()),
        paths: vec![path.to_path_buf()],
        groups: groups.into_values().collect(),
    })
}

fn analyze(inputs: &[PathBuf], output: &Path, max_groups: usize) -> CliResult<Report> {
    if max_groups == 0 {
        return Err("max-groups must be positive".into());
    }
    let files = discover(inputs)?;
    if let Ok(dest) = output.canonicalize() {
        if files.contains(&dest) {
            return Err("FTR report must not overwrite source STDF".into());
        }
    }
    let mut sources: BTreeMap<String, Source> = BTreeMap::new();
    let mut total = 0;
    for path in files {
        let source = scan(&path, max_groups)?;
        if let Some(existing) = sources.get_mut(&source.sha256) {
            existing.paths.extend(source.paths);
        } else {
            total += source.groups.len();
            if total > max_groups {
                return Err("FTR total scoped-pattern group limit exceeded".into());
            }
            sources.insert(source.sha256.clone(), source);
        }
    }
    Ok(Report {
        schema: "ftr-patterns-v1",
        sources: sources.into_values().collect(),
    })
}

fn fragment(report: &Report) -> CliResult<String> {
    let json = serde_json::to_string(report)?
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026");
    Ok(include_str!("ftr_pareto.html").replace("__FTR_DATA__", &json))
}

pub fn attach(html: String, inputs: &[PathBuf], output: &Path) -> CliResult<String> {
    if inputs.is_empty() {
        return Ok(html);
    }
    if !html.contains("<!-- FTR_PATTERN_ANALYSIS -->") {
        return Err("dashboard template is missing the FTR Pareto section".into());
    }
    let fragment = fragment(&analyze(inputs, output, 50_000)?)?;
    let html = html.replacen("<!-- FTR_PATTERN_ANALYSIS -->", &fragment, 1);
    if html.len() > 256 * 1024 * 1024 {
        return Err("FTR dashboard report exceeds 256 MiB".into());
    }
    Ok(html)
}

pub fn execute(args: Arguments, out: &mut impl Write) -> CliResult<()> {
    let limit = args
        .max_report_mib
        .checked_mul(1024 * 1024)
        .filter(|n| *n > 0)
        .ok_or("invalid report size limit")?;
    let report = analyze(&args.inputs, &args.output, args.max_groups)?;
    let html = format!("<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>FTR pattern Pareto</title><body>{}</body></html>", fragment(&report)?);
    if html.len() > limit {
        return Err("FTR report size limit exceeded".into());
    }
    let html = crate::report_ui::decorate(html);
    if html.len() > limit {
        return Err("FTR report size limit exceeded".into());
    }
    stdf_parquet::catalog::atomic_write(&args.output, html.as_bytes())?;
    writeln!(
        out,
        "unique_sources={}\nftr_attempts={}\nreport={}",
        report.sources.len(),
        report
            .sources
            .iter()
            .flat_map(|s| &s.groups)
            .map(|g| g.counts.attempts)
            .sum::<u64>(),
        args.output.display()
    )?;
    Ok(())
}

#[cfg(test)]
mod tests;
