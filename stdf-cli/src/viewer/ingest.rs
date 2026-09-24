// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
use super::*;
use arrow::array::{Array, UInt64Array};
use sha2::{Digest, Sha256};
use stdf_arrow::identity::CoordinateCandidates;
use stdf_core::StdfRecord;
use stdf_io::StreamingRecordReader;
use stdf_validate::fields::{self, definition_fields, resolve_defaults};

struct Active {
    id: u64,
    run: u64,
    lot: String,
    start: Option<u32>,
    slot: Option<usize>,
    coords: CoordinateCandidates,
}
fn clean(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.trim().is_empty() && !s.chars().any(|c| c.is_control() || c == '\u{fffd}'))
}
fn slot(
    head: u8,
    site: u8,
    groups: &BTreeMap<(u8, u8), u8>,
    slots: &BTreeMap<(u8, u8), usize>,
) -> Option<usize> {
    if let Some(g) = groups.get(&(head, site)) {
        return slots
            .get(&(head, *g))
            .or_else(|| slots.get(&(head, 255)))
            .copied();
    }
    if let Some(i) = slots.get(&(head, 255)) {
        return Some(*i);
    }
    let mut found = slots.iter().filter(|((h, _), _)| *h == head);
    let first = *found.next()?.1;
    if found.next().is_some() {
        None
    } else {
        Some(first)
    }
}
fn side(record: &StdfRecord) -> Option<(u8, u8)> {
    match record {
        StdfRecord::Pir(r) => Some((r.head_num, r.site_num)),
        StdfRecord::Prr(r) => Some((r.head_num, r.site_num)),
        StdfRecord::Ptr(r) => Some((r.head_num, r.site_num)),
        StdfRecord::Mpr(r) => Some((r.head_num, r.site_num)),
        StdfRecord::Ftr(r) => Some((r.head_num, r.site_num)),
        _ => None,
    }
}
fn value(fields: &[Field], name: &str) -> Value {
    fields
        .iter()
        .find(|f| f.name == name)
        .map(|f| f.effective.clone())
        .unwrap_or(Value::Null)
}
fn numeric(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| {
            v.get("value")
                .and_then(Value::as_str)
                .and_then(|s| s.parse().ok())
        })
        .filter(|n: &f64| n.is_finite())
}
fn string(fields: &[Field], name: &str) -> String {
    value(fields, name).as_str().unwrap_or("").into()
}
fn num(fields: &[Field], name: &str) -> Option<f64> {
    numeric(&value(fields, name))
}
fn raw_num(fields: &[Field], name: &str) -> Option<f64> {
    fields
        .iter()
        .find(|f| f.name == name)
        .and_then(|f| numeric(&f.raw))
}

pub fn prepare(args: &Arguments) -> CliResult<Cache> {
    let source = args.input.canonicalize()?;
    if !source.is_file() {
        return Err("view accepts exactly one STDF file".into());
    }
    if args.cancel_file.as_ref().is_some_and(|p| p.exists()) {
        return Err("viewer operation cancelled".into());
    }
    let hash = stdf_parquet::catalog::sha256_file(&source)?;
    let base = args
        .cache_dir
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join("zstdf-viewer"));
    fs::create_dir_all(&base)?;
    let root = base.join(format!("{VERSION}-{}", &hash));
    if root.exists() {
        let manifest_file = File::open(root.join("manifest.json"))?;
        if manifest_file.metadata()?.len() > args.memory_limit_mib as u64 * 1024 * 1024 / 8 {
            return Err("viewer manifest exceeds metadata budget".into());
        }
        let mut m: Manifest = serde_json::from_reader(manifest_file)?;
        if m.version != VERSION || m.source_hash != hash {
            return Err("incompatible viewer cache".into());
        }
        for f in m.fragments.values().flatten() {
            if m.files.get(&f.path) != Some(&f.sha256) {
                return Err("unverified companion fragment".into());
            }
        }
        for p in &m.eav_files {
            if !m.files.contains_key(p) {
                return Err("unverified measurement fragment".into());
            }
        }
        for (p, h) in &m.files {
            let path = Path::new(p);
            if path
                .components()
                .any(|c| !matches!(c, std::path::Component::Normal(_)))
            {
                return Err("unsafe cache path".into());
            }
            if stdf_parquet::catalog::sha256_file(&root.join(p))? != *h {
                return Err("viewer cache checksum mismatch; use a fresh --cache-dir".into());
            }
        }
        // Identical copies share immutable data, but show the path opened this session.
        m.source = source.to_string_lossy().into_owned();
        let c = Cache {
            root,
            manifest: m,
            memory: args.memory_limit_mib * 1024 * 1024,
            disk: args.disk_limit_mib * 1024 * 1024,
            cancel: args.cancel_file.clone(),
            cancellation: Arc::new(AtomicBool::new(false)),
            stop_watch: Arc::new(AtomicBool::new(false)),
            query_pages: std::cell::RefCell::new(BTreeMap::new()),
        };
        c.watch();
        if let Some(dataset) = &args.dataset {
            validate_dataset(&c, dataset)?;
        }
        return Ok(c);
    }
    let stage = base.join(format!(
        ".building-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    fs::create_dir(&stage)?;
    let mut cache = Cache {
        root: stage.clone(),
        manifest: Manifest {
            version: VERSION.into(),
            source: source.to_string_lossy().into_owned(),
            source_hash: hash.clone(),
            content_hash: String::new(),
            records: 0,
            attempts: 0,
            measurements: 0,
            inventory: BTreeMap::new(),
            runs: Vec::new(),
            bin_definitions: Vec::new(),
            wafers: Vec::new(),
            fragments: BTreeMap::new(),
            files: BTreeMap::new(),
            eav_files: vec!["measurements.parquet".into()],
            population_counts: BTreeMap::new(),
        },
        memory: args.memory_limit_mib * 1024 * 1024,
        disk: args.disk_limit_mib * 1024 * 1024,
        cancel: args.cancel_file.clone(),
        cancellation: Arc::new(AtomicBool::new(false)),
        stop_watch: Arc::new(AtomicBool::new(false)),
        query_pages: std::cell::RefCell::new(BTreeMap::new()),
    };
    cache.watch();
    let result = (|| -> CliResult<()> {
        spool(&source, &cache)?;
        cache.manifest.content_hash =
            stdf_parquet::catalog::sha256_file(&cache.root.join("source.stdf"))?;
        scan(&mut cache)?;
        build_attempts(&mut cache)?;
        convert(&mut cache)?;
        if let Some(dataset) = &args.dataset {
            reuse_dataset(&mut cache, dataset)?;
        }
        cache.check()?;
        if stdf_parquet::catalog::sha256_file(&source)? != hash {
            return Err("source changed during indexing".into());
        }
        for p in fs::read_dir(&stage)? {
            let p = p?;
            if p.file_type()?.is_file() {
                let name = p.file_name().to_string_lossy().into_owned();
                cache
                    .manifest
                    .files
                    .insert(name, stdf_parquet::catalog::sha256_file(&p.path())?);
            }
        }
        if store::disk_bytes(&stage)? > cache.disk {
            return Err("viewer disk budget exceeded".into());
        }
        store::atomic_write(
            &stage.join("manifest.json"),
            &serde_json::to_vec(&cache.manifest)?,
        )?;
        fs::rename(&stage, &root)?;
        Ok(())
    })();
    if let Err(e) = result {
        // Only remove this invocation's newly-created staging directory.
        if stage.parent() == Some(base.as_path())
            && stage
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(".building-"))
        {
            let _ = fs::remove_dir_all(&stage);
        }
        return Err(e);
    }
    cache.root = root;
    Ok(cache)
}
fn spool(source: &Path, cache: &Cache) -> CliResult<()> {
    let mut f = File::open(source)?;
    let mut magic = [0; 2];
    f.read_exact(&mut magic)?;
    f.rewind()?;
    let mut input: Box<dyn Read> = if magic == [0x1f, 0x8b] {
        Box::new(flate2::read::MultiGzDecoder::new(f))
    } else {
        Box::new(f)
    };
    let mut out = File::create(cache.root.join("source.stdf"))?;
    let mut b = [0u8; 65536];
    let mut bytes = 0u64;
    loop {
        cache.check()?;
        let n = input.read(&mut b)?;
        if n == 0 {
            break;
        }
        bytes += n as u64;
        if bytes > cache.disk / 2 {
            return Err("decompressed source exceeds viewer disk budget".into());
        }
        out.write_all(&b[..n])?;
    }
    out.sync_all()?;
    Ok(())
}
fn scan(cache: &mut Cache) -> CliResult<()> {
    let mut reader = StreamingRecordReader::open(cache.root.join("source.stdf"))?;
    let order = reader.byte_order();
    let mut evidence = store::IndexedWriter::new(&cache.root, "records")?;
    let mut attempts = store::IndexedWriter::new(&cache.root, "raw_attempts")?;
    let mut records = store::TableWriter::new(&cache.root, "records", cache.memory / 16);
    let (mut run, mut next, mut lot, mut start) = (0, 0, String::new(), None);
    let mut active: BTreeMap<(u8, u8), Active> = BTreeMap::new();
    let mut groups = BTreeMap::new();
    let mut slots = BTreeMap::new();
    let mut defaults: BTreeMap<(u64, String, u32), (String, Vec<Field>)> = BTreeMap::new();
    let mut definition_bytes = 0;
    while let Some(event) = reader.next_event()? {
        cache.check()?;
        cache.manifest.records += 1;
        let id = cache.manifest.records;
        let record = event
            .decoded
            .map_err(|e| format!("invalid record at {}: {e}", event.offset))?;
        let kind = record.record_type().mnemonic().to_string();
        let mut fields = fields::inspect(&kind, &event.body, order);
        let test_num = match &record {
            StdfRecord::Ptr(r) => Some(r.test_num),
            StdfRecord::Mpr(r) => Some(r.test_num),
            _ => None,
        };
        if let Some(n) = test_num {
            let k = (run, kind.clone(), n);
            let prior = defaults.get(&k);
            resolve_defaults(
                &kind,
                &mut fields,
                prior.map(|(s, f)| (s.as_str(), f.as_slice())),
            );
            if prior.is_none() {
                let d = definition_fields(&fields);
                definition_bytes += serde_json::to_vec(&d)?.len() * 4;
                if definition_bytes > cache.memory / 4 {
                    return Err("test definition index exceeds memory budget".into());
                }
                defaults.insert(k, (format!("record:{id}"), d));
            }
        }
        match &record {
            StdfRecord::Mir(r) => {
                if !active.is_empty() {
                    return Err("MIR before PRR".into());
                }
                run += 1;
                lot = r.lot_id.clone();
                start = (r.start_t > 0).then_some(r.start_t);
                groups.clear();
                slots.clear();
                defaults.clear();
                definition_bytes = 0;
                cache
                    .manifest
                    .runs
                    .push(json!({"id":run,"lot":lot,"start":start,"record":id,"job":r.job_nam}));
            }
            StdfRecord::Mrr(_) => {
                if !active.is_empty() {
                    return Err("MRR with unfinished units".into());
                }
            }
            StdfRecord::Hbr(r) => {
                cache.manifest.bin_definitions.push(json!({"level":"hard_bin","run":run,"head":r.head_num,"site":r.site_num,"number":r.hbin_num,"name":r.hbin_nam}));
            }
            StdfRecord::Sbr(r) => {
                cache.manifest.bin_definitions.push(json!({"level":"soft_bin","run":run,"head":r.head_num,"site":r.site_num,"number":r.sbin_num,"name":r.sbin_nam}));
            }
            StdfRecord::Sdr(r) => {
                for s in &r.site_num {
                    groups.insert((r.head_num, *s), r.site_grp);
                }
            }
            StdfRecord::Wir(r) => {
                cache.manifest.wafers.push(clean(r.wafer_id.clone()));
                slots.insert(
                    (r.head_num, r.site_grp.unwrap_or(255)),
                    cache.manifest.wafers.len() - 1,
                );
            }
            StdfRecord::Wrr(r) => {
                if let Some(i) = slots
                    .remove(&(r.head_num, r.site_grp))
                    .or_else(|| slots.remove(&(r.head_num, 255)))
                {
                    cache.manifest.wafers[i] = clean(r.fabwf_id.clone())
                        .or_else(|| clean(r.wafer_id.clone()))
                        .or_else(|| cache.manifest.wafers[i].clone());
                }
            }
            _ => {}
        }
        let site = side(&record);
        let mut attempt = 0;
        if let Some(s) = site {
            if matches!(record, StdfRecord::Pir(_)) && active.contains_key(&s) {
                return Err("duplicate PIR".into());
            }
            if slot(s.0, s.1, &groups, &slots).is_none()
                && (groups.contains_key(&s) || !slots.keys().any(|(h, _)| *h == s.0))
            {
                cache.manifest.wafers.push(None);
                slots.insert(
                    (s.0, groups.get(&s).copied().unwrap_or(255)),
                    cache.manifest.wafers.len() - 1,
                );
            }
            let a = active.entry(s).or_insert_with(|| {
                next += 1;
                Active {
                    id: next,
                    run,
                    lot: lot.clone(),
                    start,
                    slot: slot(s.0, s.1, &groups, &slots),
                    coords: CoordinateCandidates::default(),
                }
            });
            attempt = a.id;
            if let StdfRecord::Ptr(r) = &record {
                if let Some(n) = &r.test_txt {
                    a.coords.observe(n, Some(r.result));
                }
            }
            if let StdfRecord::Prr(r) = &record {
                let a = active.remove(&s).unwrap();
                let xy = r
                    .x_coord
                    .zip(r.y_coord)
                    .filter(|(x, y)| *x > 0 && *y > 0)
                    .map(|(x, y)| (i64::from(x), i64::from(y)))
                    .or_else(|| {
                        a.coords.lot_key("coordinates").and_then(|k| {
                            let v: Value = serde_json::from_str(&k).ok()?;
                            Some((v[2].as_i64()?, v[3].as_i64()?))
                        })
                    });
                let row = Row {
                    id: a.id,
                    record: id,
                    attempt: a.id,
                    run: a.run,
                    kind: "PRR".into(),
                    head: s.0,
                    site: s.1,
                    lot: a.lot,
                    start: a.start,
                    x: xy.map(|v| v.0),
                    y: xy.map(|v| v.1),
                    part: r.part_id.clone(),
                    passed: (r.part_flg & 0x14 == 0).then_some(r.part_flg & 8 == 0),
                    hard_bin: (r.hard_bin <= 32767).then_some(r.hard_bin),
                    soft_bin: (r.soft_bin <= 32767).then_some(r.soft_bin),
                    flags: r.part_flg,
                    duration: r.test_t,
                    offset: event.offset as u64,
                    issue: a.slot.is_none().then(|| "ambiguous_wafer".into()),
                    ..Row::default()
                };
                attempts.push(a.id, &json!({"row":row,"slot":a.slot}))?;
            }
        } else if let StdfRecord::Ater(r) = &record {
            attempt = active.get(&(r.head_num, r.site_num)).map_or(0, |a| a.id);
        }
        let e = Evidence {
            id,
            offset: event.offset as u64,
            len: event.header.rec_len,
            typ: event.header.rec_typ,
            sub: event.header.rec_sub,
            kind: kind.clone(),
            run,
            attempt,
            fields,
        };
        evidence.push(id, &e)?;
        records.push(Row {
            id,
            record: id,
            attempt,
            run,
            kind: kind.clone(),
            offset: event.offset as u64,
            head: site.map_or(0, |s| s.0),
            site: site.map_or(0, |s| s.1),
            ..Row::default()
        })?;
        *cache.manifest.inventory.entry(kind).or_default() += 1;
        if id % 1024 == 0 {
            if store::disk_bytes(&cache.root)? > cache.disk / 2 {
                return Err("viewer indexing disk budget exceeded".into());
            }
            if serde_json::to_vec(&cache.manifest.runs)?.len()
                + serde_json::to_vec(&cache.manifest.wafers)?.len()
                + serde_json::to_vec(&cache.manifest.bin_definitions)?.len()
                > cache.memory / 16
            {
                return Err("run/wafer metadata exceeds budget".into());
            }
        }
    }
    if !active.is_empty() {
        return Err("incomplete unit: missing PRR".into());
    }
    cache.manifest.attempts = next;
    records.flush()?;
    cache
        .manifest
        .fragments
        .insert("records".into(), records.fragments);
    Ok(())
}

fn build_attempts(cache: &mut Cache) -> CliResult<()> {
    let mut sorted = cache.spill()?;
    let mut raw_index = store::IndexedReader::open(&cache.root, "raw_attempts")?;
    for id in 1..=cache.manifest.attempts {
        cache.check()?;
        let v: Value = raw_index.get(id)?;
        let mut row: Row = serde_json::from_value(v["row"].clone())?;
        row.wafer = v["slot"]
            .as_u64()
            .and_then(|i| cache.manifest.wafers[i as usize].clone());
        if row.issue.is_none() && clean(Some(row.lot.clone())).is_some() {
            if let Some((x, y)) = row.x.zip(row.y) {
                row.ecid = Some(json!(["lot-wafer-xy", row.lot, row.wafer, x, y]).to_string());
            }
        }
        if row.ecid.is_none() {
            row.issue = Some("unresolved_identity".into());
        }
        let key = row
            .ecid
            .clone()
            .unwrap_or_else(|| format!("unresolved:{id}"));
        sorted.push(key.as_bytes(), &serde_json::to_vec(&row)?)?;
    }
    let mut source = sorted.finish()?;
    let mut joined = cache.spill()?;
    let mut key = Vec::new();
    let mut first: Option<Row> = None;
    let mut last: Option<Row> = None;
    let mut run0 = 0;
    let mut multirun = false;
    let mut missing = false;
    let mut tf = false;
    let mut tl = false;
    fn publish(
        joined: &mut stdf_analytics::SpillStore,
        key: &[u8],
        first: &Option<Row>,
        last: &Option<Row>,
        ambiguous: bool,
        tf: bool,
        tl: bool,
    ) -> CliResult<()> {
        if key.is_empty() {
            return Ok(());
        }
        let f = first
            .as_ref()
            .filter(|r| r.ecid.is_some() && !ambiguous && !tf)
            .map(|r| r.id);
        let l = last
            .as_ref()
            .filter(|r| r.ecid.is_some() && !ambiguous && !tl)
            .map(|r| r.id);
        let mut key = key.to_vec();
        key.push(0);
        joined.push(&key, &serde_json::to_vec(&(f, l))?)?;
        Ok(())
    }
    while let Some(item) = source.next_record()? {
        cache.check()?;
        let row: Row = serde_json::from_slice(&item.value)?;
        if key != item.key {
            publish(
                &mut joined,
                &key,
                &first,
                &last,
                multirun && missing,
                tf,
                tl,
            )?;
            key = item.key;
            first = None;
            last = None;
            run0 = row.run;
            multirun = false;
            missing = false;
            tf = false;
            tl = false;
        }
        multirun |= row.run != run0;
        missing |= row.start.is_none();
        if let Some(f) = &first {
            if row.start < f.start {
                first = Some(row.clone());
                tf = false;
            } else if row.start == f.start {
                if row.run != f.run {
                    tf = true;
                } else if row.record < f.record {
                    first = Some(row.clone());
                }
            }
        } else {
            first = Some(row.clone());
        }
        if let Some(l) = &last {
            if row.start > l.start {
                last = Some(row.clone());
                tl = false;
            } else if row.start == l.start {
                if row.run != l.run {
                    tl = true;
                } else if row.record > l.record {
                    last = Some(row.clone());
                }
            }
        } else {
            last = Some(row.clone());
        }
        let mut data_key = key.clone();
        data_key.push(1);
        joined.push(&data_key, &item.value)?;
    }
    publish(
        &mut joined,
        &key,
        &first,
        &last,
        multirun && missing,
        tf,
        tl,
    )?;
    drop(source);
    let mut stream = joined.finish()?;
    let mut table = store::TableWriter::new(&cache.root, "devices", cache.memory / 16);
    let mut index = store::IndexedWriter::new(&cache.root, "attempts")?;
    let mut flags = File::create(cache.root.join("selection.bin"))?;
    flags.set_len(cache.manifest.attempts + 1)?;
    let (mut first, mut last): (Option<u64>, Option<u64>) = (None, None);
    while let Some(item) = stream.next_record()? {
        cache.check()?;
        if item.key.last() == Some(&0) {
            (first, last) = serde_json::from_slice(&item.value)?;
        } else {
            let mut row: Row = serde_json::from_slice(&item.value)?;
            row.first = first.map(|id| id == row.id);
            row.latest = last.map(|id| id == row.id);
            if row.ecid.is_some() && last.is_none() {
                row.issue = Some("ambiguous_latest".into());
            }
            flags.seek(SeekFrom::Start(row.id))?;
            flags.write_all(&[
                u8::from(row.first == Some(true)) | u8::from(row.latest == Some(true)) * 2
            ])?;
            for name in [
                if row.latest == Some(true) {
                    "latest"
                } else if row.latest.is_none() {
                    "unresolved_latest"
                } else {
                    "earlier"
                },
                if row.first == Some(true) {
                    "first"
                } else {
                    "not_first"
                },
            ] {
                *cache
                    .manifest
                    .population_counts
                    .entry(name.into())
                    .or_default() += 1;
            }
            index.push(row.id, &row)?;
            table.push(row)?;
        }
    }
    table.flush()?;
    cache
        .manifest
        .fragments
        .insert("devices".into(), table.fragments);
    Ok(())
}

fn convert(cache: &mut Cache) -> CliResult<()> {
    let reader = StreamingRecordReader::open(cache.root.join("source.stdf"))?;
    let records = reader.map(|r| {
        r.map_err(|e| stdf_core::StdfError::InvalidField {
            record: "viewer",
            field: "source",
            msg: e.to_string(),
        })
    });
    let mut batches = stdf_arrow::bounded_record_batches(
        records,
        stdf_arrow::BatchLimits {
            max_pending_tests: 100_000,
            max_memory_bytes: cache.memory / 4,
        },
    )?
    .with_provenance();
    let mut eav = parquet::arrow::ArrowWriter::try_new(
        File::create(cache.root.join("measurements.parquet"))?,
        stdf_arrow::eav_schema(),
        None,
    )?;
    let mut rows = store::TableWriter::new(&cache.root, "measurements", cache.memory / 16);
    let mut evidence_index = store::IndexedReader::open(&cache.root, "records")?;
    let mut attempt_index = store::IndexedReader::open(&cache.root, "attempts")?;
    let mut eav_index = 0u64;
    while let Some(batch) = batches.next() {
        cache.check()?;
        let batch = batch?;
        let origins = batches.last_provenance();
        if origins.len() != batch.num_rows() {
            return Err("EAV provenance count mismatch".into());
        }
        eav.write(&batch)?;
        if eav.in_progress_rows() >= 4096 {
            eav.flush()?;
        }
        let parts = batch
            .column(stdf_arrow::schema::PART_SEQUENCE)
            .as_any()
            .downcast_ref::<UInt64Array>()
            .ok_or("invalid EAV part sequence")?;
        let mut last = BTreeMap::new();
        let mut pending = Vec::new();
        for (i, origin) in origins.iter().enumerate() {
            eav_index += 1;
            let e: Evidence = evidence_index.get(origin.record_sequence)?;
            let a: Row = attempt_index.get(parts.value(i))?;
            if e.attempt != a.id {
                return Err("EAV provenance attempt mismatch".into());
            }
            let f = &e.fields;
            let flag = value(f, "TEST_FLG").as_u64().unwrap_or(0) as u8;
            let name = string(f, "TEST_TXT");
            let number = value(f, "TEST_NUM").as_u64().map(|n| n as u32);
            let channel = (e.kind == "MPR" && origin.expansion_index > 0)
                .then_some(origin.expansion_index.saturating_sub(1));
            let val = if e.kind == "PTR" {
                num(f, "RESULT")
            } else if let Some(c) = channel {
                value(f, "RTN_RSLT").get(c as usize).and_then(numeric)
            } else {
                None
            };
            let raw = if e.kind == "PTR" {
                raw_num(f, "RESULT")
            } else if let Some(c) = channel {
                f.iter()
                    .find(|f| f.name == "RTN_RSLT")
                    .and_then(|f| f.raw.get(c as usize))
                    .and_then(numeric)
            } else {
                None
            };
            let test = json!([e.kind, number, name, channel]).to_string();
            let pattern = (e.kind == "FTR")
                .then(|| string(f, "VECT_NAM"))
                .filter(|s| !s.is_empty());
            let key = json!([test, pattern]).to_string();
            last.insert(key.clone(), pending.len());
            let result_field = f.iter().find(|f| {
                f.name
                    == if e.kind == "PTR" {
                        "RESULT"
                    } else {
                        "RTN_RSLT"
                    }
            });
            let value_state = if val.is_some() {
                "valid"
            } else if e.kind == "FTR" || (e.kind == "MPR" && channel.is_none()) {
                "not_numeric"
            } else if flag & 0x3f != 0 {
                "invalid"
            } else if result_field.is_some_and(|f| {
                !f.raw.is_null() && f.raw.to_string().to_lowercase().contains("nan")
                    || !f.raw.is_null() && f.raw.to_string().to_lowercase().contains("inf")
            }) {
                "nonfinite"
            } else {
                result_field.map_or("missing", |f| f.status.as_str())
            }
            .to_string();
            let row = Row {
                id: eav_index,
                value_state,
                record: e.id,
                kind: e.kind.clone(),
                test,
                name,
                number,
                channel,
                pattern,
                value: val,
                raw,
                low: num(f, "LO_LIMIT"),
                high: num(f, "HI_LIMIT"),
                units: string(f, "UNITS"),
                passed: if channel.is_some() {
                    None
                } else {
                    (flag & 0x7e == 0).then_some(flag & 0x80 == 0)
                },
                flags: flag,
                alarm: flag & 1 != 0 || !string(f, "ALARM_ID").is_empty(),
                offset: e.offset,
                eav_fragment: Some("measurements.parquet".into()),
                eav_row: Some(eav_index - 1),
                ..a
            };
            pending.push((key, row));
        }
        for (i, (key, mut row)) in pending.into_iter().enumerate() {
            row.last_execution = last.get(&key) == Some(&i);
            rows.push(row)?;
        }
        if eav_index % 1024 < batch.num_rows() as u64
            && store::disk_bytes(&cache.root)? > cache.disk / 2
        {
            return Err("viewer measurement disk budget exceeded".into());
        }
    }
    eav.close()?;
    rows.flush()?;
    cache.manifest.measurements = eav_index;
    cache
        .manifest
        .fragments
        .insert("measurements".into(), rows.fragments);
    Ok(())
}

fn digest_parquet(paths: &[PathBuf], cache: &Cache) -> CliResult<(String, u64)> {
    let mut sort = cache.spill()?;
    let mut count = 0;
    for path in paths {
        let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
            File::open(path)?,
        )?;
        if reader.schema().as_ref() != stdf_arrow::eav_schema().as_ref() {
            return Err("incompatible EAV schema".into());
        }
        for b in reader.with_batch_size(256).build()? {
            let b = b?;
            for i in 0..b.num_rows() {
                let v: Vec<Option<String>> = b
                    .columns()
                    .iter()
                    .map(|a| {
                        if a.is_null(i) {
                            Ok(None)
                        } else {
                            arrow::util::display::array_value_to_string(a, i).map(Some)
                        }
                    })
                    .collect::<Result<_, _>>()?;
                sort.push(&Sha256::digest(serde_json::to_vec(&v)?), &[])?;
                count += 1;
            }
            cache.check()?;
        }
    }
    let mut h = Sha256::new();
    let mut records = sort.finish()?;
    while let Some(r) = records.next_record()? {
        h.update(r.key);
    }
    Ok((format!("{:x}", h.finalize()), count))
}
fn validate_dataset(cache: &Cache, root: &Path) -> CliResult<()> {
    let catalog = stdf_parquet::catalog::verify_catalog(root)?;
    let source = catalog
        .sources
        .values()
        .find(|s| s.source_sha256 == cache.manifest.source_hash)
        .ok_or("catalog has no matching source hash")?;
    let paths: Vec<_> = source
        .fragments
        .iter()
        .map(|f| root.join(&f.path))
        .collect();
    if digest_parquet(&paths, cache)?
        != digest_parquet(
            &cache
                .manifest
                .eav_files
                .iter()
                .map(|p| cache.root.join(p))
                .collect::<Vec<_>>(),
            cache,
        )?
    {
        return Err("catalog measurement coverage differs from source; reconvert".into());
    }
    Ok(())
}

// Reuse immutable catalog bytes after validating the full multiset of typed rows.
// Equal repeated rows are assigned separate addresses in stable fragment/row order.
fn reuse_dataset(cache: &mut Cache, root: &Path) -> CliResult<()> {
    validate_dataset(cache, root)?;
    let catalog = stdf_parquet::catalog::verify_catalog(root)?;
    let source = catalog
        .sources
        .values()
        .find(|s| s.source_sha256 == cache.manifest.source_hash)
        .ok_or("missing matching catalog source")?;
    let mut expected = cache.spill()?;
    let mut actual = cache.spill()?;
    fn index(
        paths: &[PathBuf],
        sort: &mut stdf_analytics::SpillStore,
        cache: &Cache,
    ) -> CliResult<()> {
        let mut sequence = 0u64;
        for (fragment, path) in paths.iter().enumerate() {
            let mut offset = 0u64;
            let reader = parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder::try_new(
                File::open(path)?,
            )?
            .with_batch_size(256)
            .build()?;
            for b in reader {
                cache.check()?;
                let b = b?;
                for i in 0..b.num_rows() {
                    let values: Vec<Option<String>> = b
                        .columns()
                        .iter()
                        .map(|a| {
                            if a.is_null(i) {
                                Ok(None)
                            } else {
                                arrow::util::display::array_value_to_string(a, i).map(Some)
                            }
                        })
                        .collect::<Result<_, _>>()?;
                    sequence += 1;
                    sort.push(
                        &Sha256::digest(serde_json::to_vec(&values)?),
                        &serde_json::to_vec(&(sequence, fragment, offset))?,
                    )?;
                    offset += 1;
                }
            }
        }
        Ok(())
    }
    index(
        &[cache.root.join("measurements.parquet")],
        &mut expected,
        cache,
    )?;
    let paths: Vec<_> = source
        .fragments
        .iter()
        .map(|f| root.join(&f.path))
        .collect();
    index(&paths, &mut actual, cache)?;
    let mut expected = expected.finish()?;
    let mut actual = actual.finish()?;
    let mut mapping = store::IndexedWriter::new(&cache.root, "catalog_mapping")?;
    loop {
        match (expected.next_record()?, actual.next_record()?) {
            (None, None) => break,
            (Some(a), Some(b)) if a.key == b.key => {
                let (id, _, _): (u64, usize, u64) = serde_json::from_slice(&a.value)?;
                let (_, fragment, row): (u64, usize, u64) = serde_json::from_slice(&b.value)?;
                mapping.push(id, &(fragment, row))?;
            }
            _ => return Err("incomplete catalog provenance coverage".into()),
        }
    }
    drop(mapping);
    drop(actual);
    drop(expected);
    let mut names = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        cache.check()?;
        let name = format!("catalog-eav-{i:06}.parquet");
        // Copy preserves immutability even if the user later replaces a catalog file.
        if store::disk_bytes(&cache.root)? + fs::metadata(path)?.len() > cache.disk / 2 {
            return Err("catalog reuse exceeds disk budget".into());
        }
        fs::copy(path, cache.root.join(&name))?;
        names.push(name);
    }
    let mut output = store::TableWriter::new(&cache.root, "mapped-measurements", cache.memory / 16);
    store::scan(cache, "measurements", &[], None, |mut r| {
        let (fragment, row): (usize, u64) = store::lookup(&cache.root, "catalog_mapping", r.id)?;
        r.eav_fragment = Some(names[fragment].clone());
        r.eav_row = Some(row);
        output.push(r)
    })?;
    output.flush()?;
    let previous = cache
        .manifest
        .fragments
        .insert("measurements".into(), output.fragments)
        .unwrap_or_default();
    for f in previous {
        fs::remove_file(cache.root.join(f.path))?;
    }
    fs::remove_file(cache.root.join("measurements.parquet"))?;
    cache.manifest.eav_files = names;
    Ok(())
}
