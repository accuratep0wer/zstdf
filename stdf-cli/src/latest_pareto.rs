// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
//! Latest-device Pareto evidence. Independent of legacy all-attempt EAV statistics.
use crate::CliResult;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{Read, Seek};
use std::path::{Path, PathBuf};
use stdf_arrow::identity::CoordinateCandidates;
use stdf_core::StdfRecord;
use stdf_io::StreamingRecordReader;

#[derive(Clone, Debug, Serialize)]
pub struct Mode {
    key: String,
    label: String,
    passed: Option<bool>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Attempt {
    source: String,
    sequence: u64,
    run: usize,
    start_t: Option<u32>,
    offset: usize,
    lot: String,
    wafer: Option<String>,
    x: Option<i64>,
    y: Option<i64>,
    coordinate_source: &'static str,
    ecid: Option<String>,
    head: u8,
    site: u8,
    part_id: Option<String>,
    passed: Option<bool>,
    hard_bin: Option<u16>,
    soft_bin: Option<u16>,
    tests: Vec<Mode>,
    patterns: Vec<Mode>,
    is_latest: Option<bool>,
    issue: Option<&'static str>,
    #[serde(skip)]
    wafer_slot: Option<usize>,
}
#[derive(Serialize)]
pub struct Report {
    schema: &'static str,
    ordering: &'static str,
    sources: BTreeMap<String, Vec<PathBuf>>,
    unavailable: Option<String>,
    attempts: Vec<Attempt>,
}
struct Active {
    lot: String,
    wafer_slot: Option<usize>,
    wafer_ambiguous: bool,
    run: usize,
    start: Option<u32>,
    coordinates: CoordinateCandidates,
    tests: BTreeMap<String, Mode>,
    patterns: BTreeMap<String, Mode>,
}
struct Hashed<R> {
    inner: R,
    digest: Sha256,
}
impl<R: Read> Read for Hashed<R> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(bytes)?;
        self.digest.update(&bytes[..n]);
        Ok(n)
    }
}
struct Budget {
    used: usize,
    limit: usize,
}
impl Budget {
    fn charge(&mut self, bytes: usize) -> CliResult<()> {
        self.used = self.used.saturating_add(bytes);
        if self.used > self.limit {
            return Err("latest Pareto evidence exceeds memory budget; reduce the input population or raise --memory-limit-mib".into());
        }
        Ok(())
    }
}
fn clean(value: Option<String>) -> Option<String> {
    value.filter(|s| !s.trim().is_empty() && !s.chars().any(|c| c.is_control() || c == '\u{fffd}'))
}
fn verdict(flag: u8) -> Option<bool> {
    (flag & 0x7e == 0).then_some(flag & 0x80 == 0)
}
fn mode(kind: &str, number: u32, name: Option<&str>, flag: u8) -> Mode {
    Mode {
        key: serde_json::json!([kind, number, name]).to_string(),
        label: format!("{kind} {number} {}", name.unwrap_or("")),
        passed: verdict(flag),
    }
}
fn collapse_patterns(modes: impl Iterator<Item = Mode>) -> Vec<Mode> {
    let mut patterns: BTreeMap<String, Mode> = BTreeMap::new();
    for m in modes {
        patterns
            .entry(m.key.clone())
            .and_modify(|old| {
                old.passed = match (old.passed, m.passed) {
                    (Some(false), _) | (_, Some(false)) => Some(false),
                    (Some(true), Some(true)) => Some(true),
                    _ => None,
                };
            })
            .or_insert(m);
    }
    patterns.into_values().collect()
}

fn selected_slot(
    head: u8,
    site: u8,
    groups: &BTreeMap<(u8, u8), u8>,
    slots: &BTreeMap<(u8, u8), usize>,
) -> Option<usize> {
    if let Some(group) = groups.get(&(head, site)) {
        return slots
            .get(&(head, *group))
            .or_else(|| slots.get(&(head, 255)))
            .copied();
    }
    if let Some(slot) = slots.get(&(head, 255)) {
        return Some(*slot);
    }
    let mut matching = slots.iter().filter(|((h, _), _)| *h == head);
    let first = *matching.next()?.1;
    matching.next().is_none().then_some(first)
}
fn scan(path: &Path, budget: &mut Budget) -> CliResult<(String, Vec<Attempt>)> {
    let mut file = File::open(path)?;
    let mut magic = [0; 2];
    file.read_exact(&mut magic)?;
    file.rewind()?;
    let input: Box<dyn Read> = if magic == [0x1f, 0x8b] {
        Box::new(flate2::read::MultiGzDecoder::new(file))
    } else {
        Box::new(file)
    };
    let mut input = Hashed {
        inner: input,
        digest: Sha256::new(),
    };
    let mut attempts = Vec::new();
    let mut wafers: Vec<Option<String>> = Vec::new();
    let mut slots = BTreeMap::new();
    let mut groups = BTreeMap::new();
    let mut active: BTreeMap<(u8, u8), Active> = BTreeMap::new();
    let (mut lot, mut start, mut run) = (String::new(), None, 0);
    {
        let mut reader = StreamingRecordReader::new(&mut input)?;
        loop {
            let offset = reader.bytes_consumed();
            let Some(record) = reader.next() else {
                break;
            };
            let record = record?;
            // Every retained record/label is charged conservatively, including duplicates.
            budget.charge(256)?;
            match &record {
                StdfRecord::Mir(mir) => {
                    if !active.is_empty() {
                        return Err("MIR with incomplete parts in latest Pareto input".into());
                    }
                    run += 1;
                    lot = mir.lot_id.clone();
                    start = (mir.start_t > 0).then_some(mir.start_t);
                    slots.clear();
                    groups.clear();
                }
                StdfRecord::Sdr(sdr) => {
                    for site in &sdr.site_num {
                        groups.insert((sdr.head_num, *site), sdr.site_grp);
                    }
                }
                StdfRecord::Wir(wir) => {
                    wafers.push(clean(wir.wafer_id.clone()));
                    slots.insert(
                        (wir.head_num, wir.site_grp.unwrap_or(255)),
                        wafers.len() - 1,
                    );
                }
                StdfRecord::Wrr(wrr) => {
                    let key = (wrr.head_num, wrr.site_grp);
                    if let Some(slot) = slots
                        .remove(&key)
                        .or_else(|| slots.remove(&(wrr.head_num, 255)))
                    {
                        // FABWF_ID is the fab's wafer identifier / scribe when supplied.
                        wafers[slot] = clean(wrr.fabwf_id.clone())
                            .or_else(|| clean(wrr.wafer_id.clone()))
                            .or_else(|| wafers[slot].clone());
                    }
                }
                StdfRecord::Mrr(_) if !active.is_empty() => {
                    return Err("MRR with incomplete parts in latest Pareto input".into())
                }
                StdfRecord::Unknown { typ, sub, .. }
                    if matches!(
                        (*typ, *sub),
                        (1, 10)
                            | (1, 20)
                            | (1, 80)
                            | (2, 10)
                            | (2, 20)
                            | (5, 10)
                            | (5, 20)
                            | (15, 10)
                            | (15, 15)
                            | (15, 20)
                    ) =>
                {
                    return Err(format!(
                        "malformed latest Pareto record ({typ},{sub}) at {}:{offset}",
                        path.display()
                    )
                    .into())
                }
                _ => {}
            }
            let site = match &record {
                StdfRecord::Pir(r) => Some((r.head_num, r.site_num)),
                StdfRecord::Ptr(r) => Some((r.head_num, r.site_num)),
                StdfRecord::Mpr(r) => Some((r.head_num, r.site_num)),
                StdfRecord::Ftr(r) => Some((r.head_num, r.site_num)),
                StdfRecord::Prr(r) => Some((r.head_num, r.site_num)),
                _ => None,
            };
            let Some(site) = site else {
                continue;
            };
            if matches!(record, StdfRecord::Pir(_)) && active.contains_key(&site) {
                return Err("duplicate PIR in latest Pareto input".into());
            }
            // A WRR may supply the wafer/scribe only after the units have closed.
            if selected_slot(site.0, site.1, &groups, &slots).is_none()
                && (groups.contains_key(&site) || !slots.keys().any(|(h, _)| *h == site.0))
            {
                wafers.push(None);
                slots.insert(
                    (site.0, groups.get(&site).copied().unwrap_or(255)),
                    wafers.len() - 1,
                );
            }
            let part = active.entry(site).or_insert_with(|| Active {
                lot: lot.clone(),
                wafer_slot: selected_slot(site.0, site.1, &groups, &slots),
                wafer_ambiguous: selected_slot(site.0, site.1, &groups, &slots).is_none(),
                run,
                start,
                coordinates: CoordinateCandidates::default(),
                tests: BTreeMap::new(),
                patterns: BTreeMap::new(),
            });
            let test = match &record {
                StdfRecord::Ptr(r) => {
                    if let Some(name) = &r.test_txt {
                        part.coordinates.observe(name, Some(r.result));
                    }
                    Some(mode("PTR", r.test_num, r.test_txt.as_deref(), r.test_flg))
                }
                StdfRecord::Mpr(r) => {
                    if r.rtn_rslt.as_ref().map_or(0, Vec::len) != usize::from(r.rslt_cnt)
                        || r.rtn_stat.as_ref().map_or(0, Vec::len) != usize::from(r.rtn_icnt)
                    {
                        return Err("malformed MPR arrays in latest Pareto evidence".into());
                    }
                    Some(mode("MPR", r.test_num, r.test_txt.as_deref(), r.test_flg))
                }
                StdfRecord::Ftr(r) => {
                    let name = r.vect_nam.clone().filter(|s| !s.trim().is_empty());
                    let key = serde_json::to_string(&name)?;
                    let pattern = Mode {
                        key: key.clone(),
                        label: name.unwrap_or_else(|| "[null] (missing VECT_NAM)".into()),
                        passed: verdict(r.test_flg),
                    };
                    budget.charge(1024 + pattern.label.len() * 8)?;
                    part.patterns
                        .insert(serde_json::json!([r.test_num, key]).to_string(), pattern);
                    Some(mode("FTR", r.test_num, r.test_txt.as_deref(), r.test_flg))
                }
                _ => None,
            };
            if let Some(test) = test {
                budget.charge(1024 + test.label.len() * 8)?;
                let key = if let StdfRecord::Ftr(r) = &record {
                    serde_json::json!([test.key, r.vect_nam]).to_string()
                } else {
                    test.key.clone()
                };
                part.tests.insert(key, test);
            }
            if let StdfRecord::Prr(prr) = &record {
                let part = active.remove(&site).unwrap();
                let prr_xy = prr
                    .x_coord
                    .zip(prr.y_coord)
                    .filter(|(x, y)| *x > 0 && *y > 0)
                    .map(|(x, y)| (i64::from(x), i64::from(y)));
                let ptr_xy = part.coordinates.lot_key("coordinate").and_then(|key| {
                    let v: serde_json::Value = serde_json::from_str(&key).ok()?;
                    Some((v[2].as_i64()?, v[3].as_i64()?))
                });
                let xy = prr_xy.or(ptr_xy);
                budget.charge(4096 + part.lot.len() * 8)?;
                attempts.push(Attempt {
                    source: String::new(),
                    sequence: attempts.len() as u64 + 1,
                    run: part.run,
                    start_t: part.start,
                    offset,
                    lot: part.lot,
                    wafer: None,
                    x: xy.map(|p| p.0),
                    y: xy.map(|p| p.1),
                    coordinate_source: if prr_xy.is_some() {
                        "PRR"
                    } else if ptr_xy.is_some() {
                        "PTR"
                    } else {
                        "unresolved"
                    },
                    ecid: None,
                    head: site.0,
                    site: site.1,
                    part_id: prr.part_id.clone(),
                    passed: (prr.part_flg & 0x14 == 0).then_some(prr.part_flg & 8 == 0),
                    hard_bin: (prr.hard_bin <= 32767).then_some(prr.hard_bin),
                    soft_bin: (prr.soft_bin <= 32767).then_some(prr.soft_bin),
                    tests: collapse_patterns(part.tests.into_values()),
                    patterns: collapse_patterns(part.patterns.into_values()),
                    is_latest: None,
                    issue: part.wafer_ambiguous.then_some("ambiguous_wafer"),
                    wafer_slot: part.wafer_slot,
                });
            }
        }
    }
    if !active.is_empty() {
        return Err("incomplete latest Pareto attempts: missing PRR".into());
    }
    let hash = format!("{:x}", input.digest.finalize());
    for attempt in &mut attempts {
        attempt.source = hash.clone();
        attempt.wafer = attempt.wafer_slot.and_then(|i| wafers[i].clone());
        if clean(Some(attempt.lot.clone())).is_some() && attempt.issue.is_none() {
            if let Some((x, y)) = attempt.x.zip(attempt.y) {
                attempt.ecid = Some(
                    serde_json::json!(["lot-wafer-xy", attempt.lot, attempt.wafer, x, y])
                        .to_string(),
                );
            }
        }
    }
    Ok((hash, attempts))
}

fn select_latest(attempts: &mut [Attempt]) {
    let mut devices: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (i, a) in attempts.iter_mut().enumerate() {
        if let Some(key) = &a.ecid {
            devices.entry(key.clone()).or_default().push(i);
        } else {
            a.issue = Some(a.issue.unwrap_or("unresolved_identity"));
        }
    }
    for indices in devices.values() {
        // Linear candidate reduction: do not compare every retest pair.
        let all_timed = indices.iter().all(|&i| attempts[i].start_t.is_some());
        let max_start = indices.iter().filter_map(|&i| attempts[i].start_t).max();
        let candidates: Vec<_> = indices
            .iter()
            .copied()
            .filter(|&i| !all_timed || attempts[i].start_t == max_start)
            .collect();
        let scopes: BTreeSet<_> = candidates
            .iter()
            .map(|&i| (&attempts[i].source, attempts[i].run))
            .collect();
        let latest: Vec<_> = if scopes.len() == 1 {
            candidates
                .into_iter()
                .max_by_key(|&i| attempts[i].sequence)
                .into_iter()
                .collect()
        } else {
            Vec::new()
        };
        for &i in indices {
            if latest.len() == 1 {
                attempts[i].is_latest = Some(i == latest[0]);
            } else {
                attempts[i].issue = Some("ambiguous_latest");
            }
        }
    }
}

fn discover(inputs: &[PathBuf]) -> CliResult<Vec<PathBuf>> {
    let mut pending = inputs.to_vec();
    let mut seen = BTreeSet::new();
    let mut files = BTreeSet::new();
    while let Some(path) = pending.pop() {
        let path = path.canonicalize()?;
        if !seen.insert(path.clone()) {
            continue;
        }
        if seen.len() > 100_000 {
            return Err("latest Pareto directory limit exceeded".into());
        }
        if path.is_dir() {
            for item in std::fs::read_dir(path)? {
                let item = item?;
                if item.file_type()?.is_symlink() {
                    continue;
                }
                let p = item.path();
                if p.is_dir() || crate::is_stdf_path(&p) {
                    pending.push(p);
                }
                if pending.len() > 100_000 {
                    return Err("latest Pareto directory limit exceeded".into());
                }
            }
        } else {
            files.insert(path);
        }
        if files.len() > 4096 {
            return Err("latest Pareto source limit exceeded".into());
        }
    }
    Ok(files.into_iter().collect())
}
pub fn analyze(inputs: &[PathBuf], output: &Path, limit: usize) -> CliResult<Report> {
    let mut budget = Budget { used: 0, limit };
    let mut report = Report {
        schema: "latest-device-pareto-v1",
        ordering: "MIR.START_T; source-local PRR sequence within the same run",
        sources: BTreeMap::new(),
        unavailable: None,
        attempts: Vec::new(),
    };
    let output = output.canonicalize().ok();
    for path in discover(inputs)? {
        if output.as_ref() == Some(&path) {
            return Err("Pareto report must not overwrite source STDF".into());
        }
        let (hash, attempts) = scan(&path, &mut budget)?;
        let duplicate = report.sources.contains_key(&hash);
        report.sources.entry(hash).or_default().push(path);
        if !duplicate {
            report.attempts.extend(attempts);
        }
        if report.attempts.len() > 100_000 {
            return Err("latest Pareto attempt limit exceeded (100000)".into());
        }
    }
    report
        .attempts
        .sort_by(|a, b| (&a.source, a.sequence).cmp(&(&b.source, b.sequence)));
    select_latest(&mut report.attempts);
    Ok(report)
}

pub fn attach(html: String, inputs: &[PathBuf], output: &Path, limit: usize) -> CliResult<String> {
    let report = analyze(inputs, output, limit)?;
    render(html, report, limit)
}

pub fn unavailable(html: String, reason: &str, limit: usize) -> CliResult<String> {
    let report = Report {
        schema: "latest-device-pareto-v1",
        ordering: "MIR.START_T",
        sources: BTreeMap::new(),
        attempts: Vec::new(),
        unavailable: Some(reason.into()),
    };
    render(html, report, limit)
}
fn render(html: String, report: Report, limit: usize) -> CliResult<String> {
    let json = serde_json::to_string(&report)?;
    if json.len() > limit / 4 {
        return Err("latest Pareto report exceeds size budget".into());
    }
    let panel = include_str!("latest_pareto.html").replace(
        "__LATEST_DATA__",
        &json
            .replace('<', "\\u003c")
            .replace('>', "\\u003e")
            .replace('&', "\\u0026"),
    );
    let start = html
        .find("  <section id=\"pareto\"")
        .ok_or("missing Pareto section")?;
    let end = start
        + html[start..]
            .find("  <section id=\"commonality\"")
            .ok_or("missing commonality section")?;
    let mut result = html;
    result.replace_range(
        start..end,
        &format!("  <section id=\"pareto\" class=\"section\">{panel}</section>\n"),
    );
    if result.len() > limit {
        return Err("latest Pareto HTML exceeds size budget".into());
    }
    Ok(result)
}

#[cfg(test)]
mod tests;
