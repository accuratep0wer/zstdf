// Copyright 2026 zstdf contributors. SPDX-License-Identifier: Apache-2.0
use super::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Query {
    pub pane: u64,
    pub table: String,
    pub population: String,
    pub run: Option<u64>,
    pub wafer: Option<String>,
    pub sites: Vec<u8>,
    pub tests: Vec<String>,
    pub attempts: Vec<u64>,
    pub excluded_measurements: Vec<u64>,
    pub excluded_devices: Vec<u64>,
    pub filters: BTreeMap<String, String>,
    pub sort: String,
    pub descending: bool,
    pub offset: usize,
    pub limit: usize,
    pub plot: String,
    pub bins: usize,
    pub low_whisker: f64,
    pub high_whisker: f64,
    pub color: String,
    pub level: String,
    pub show_pass: bool,
    pub statistic: String,
    pub annotation: String,
    pub search: String,
    pub values: BTreeMap<String, Vec<Value>>,
    pub hard_bins: Vec<u16>,
    pub soft_bins: Vec<u16>,
    pub alerts: BTreeMap<String, String>,
}
impl Default for Query {
    fn default() -> Self {
        Self {
            pane: 0,
            table: "measurements".into(),
            population: "latest".into(),
            run: None,
            wafer: None,
            sites: vec![],
            tests: vec![],
            attempts: vec![],
            excluded_measurements: vec![],
            excluded_devices: vec![],
            filters: BTreeMap::new(),
            sort: String::new(),
            descending: false,
            offset: 0,
            limit: 100,
            plot: "histogram".into(),
            bins: 20,
            low_whisker: 2.,
            high_whisker: 98.,
            color: "site".into(),
            level: "hard_bin".into(),
            show_pass: false,
            statistic: "mean".into(),
            annotation: "all".into(),
            search: String::new(),
            values: BTreeMap::new(),
            hard_bins: vec![],
            soft_bins: vec![],
            alerts: BTreeMap::new(),
        }
    }
}
impl Query {
    pub fn validate(&self) -> CliResult<()> {
        if !["latest", "first", "all"].contains(&self.population.as_str())
            || ![
                "measurements",
                "devices",
                "records",
                "tests",
                "wafers",
                "hard_bin",
                "soft_bin",
            ]
            .contains(&self.table.as_str())
        {
            return Err("invalid query population/table".into());
        }
        if self.limit > 1000
            || self.bins == 0
            || self.bins > 1024
            || !self.low_whisker.is_finite()
            || !self.high_whisker.is_finite()
            || self.low_whisker < 0.
            || self.high_whisker > 100.
            || self.low_whisker > self.high_whisker
        {
            return Err("invalid query limits".into());
        }
        if self.tests.len() > 256
            || self.attempts.len() > 10000
            || self.excluded_devices.len() + self.excluded_measurements.len() > 100000
        {
            return Err("selection exceeds query budget".into());
        }
        Ok(())
    }
    pub fn population(&self, r: &Row) -> bool {
        match self.population.as_str() {
            "first" => r.first == Some(true),
            "latest" => r.latest == Some(true),
            _ => true,
        }
    }
    fn scope(&self, r: &Row) -> bool {
        self.run.is_none_or(|v| v == r.run)
            && self
                .wafer
                .as_ref()
                .is_none_or(|w| r.wafer.as_ref() == Some(w))
            && (self.sites.is_empty() || self.sites.contains(&r.site))
            && (self.attempts.is_empty() || self.attempts.contains(&r.attempt))
            && (self.hard_bins.is_empty()
                || r.hard_bin.is_some_and(|b| self.hard_bins.contains(&b)))
            && (self.soft_bins.is_empty()
                || r.soft_bin.is_some_and(|b| self.soft_bins.contains(&b)))
    }
    pub fn included(&self, r: &Row, measurements: bool) -> bool {
        self.population(r)
            && !self.excluded_devices.contains(&r.attempt)
            && (!measurements || !self.excluded_measurements.contains(&r.id))
    }
}
pub fn field(row: &Row, key: &str) -> Value {
    serde_json::to_value(row)
        .unwrap_or(Value::Null)
        .get(key)
        .cloned()
        .unwrap_or(Value::Null)
}
fn text(v: &Value) -> String {
    if v.is_null() {
        "[null]".into()
    } else if let Some(s) = v.as_str() {
        s.into()
    } else {
        v.to_string()
    }
}
pub fn matches(row: &Row, filters: &BTreeMap<String, String>) -> bool {
    filters.iter().all(|(k, filter)| {
        let v = field(row, k);
        let f = filter.trim();
        if f.is_empty() {
            return true;
        }
        for op in [">=", "<=", "!=", ">", "<", "="] {
            if let Some(rhs) = f.strip_prefix(op) {
                if let (Some(n), Ok(t)) = (v.as_f64(), rhs.trim().parse::<f64>()) {
                    return match op {
                        ">=" => n >= t,
                        "<=" => n <= t,
                        ">" => n > t,
                        "<" => n < t,
                        "!=" => n != t,
                        _ => n == t,
                    };
                }
                return if op == "!=" {
                    text(&v) != rhs.trim()
                } else if op == "=" {
                    text(&v) == rhs.trim()
                } else {
                    false
                };
            }
        }
        text(&v).to_lowercase().contains(&f.to_lowercase())
    })
}
fn sort_key(r: &Row, q: &Query) -> Vec<u8> {
    let mut bytes = if q.sort.is_empty() {
        r.record.to_be_bytes().to_vec()
    } else {
        let v = field(r, &q.sort);
        if let Some(n) = v.as_f64() {
            float_key(n).to_vec()
        } else {
            let mut b = text(&v).to_lowercase().into_bytes();
            b.push(0);
            b
        }
    };
    if q.descending {
        for b in &mut bytes {
            *b = !*b;
        }
    }
    bytes.extend_from_slice(&r.id.to_be_bytes());
    bytes
}
fn float_key(n: f64) -> [u8; 8] {
    let b = n.to_bits();
    (if b >> 63 == 1 { !b } else { b ^ (1 << 63) }).to_be_bytes()
}

pub fn visit(
    cache: &Cache,
    q: &Query,
    analysis: bool,
    mut f: impl FnMut(Row) -> CliResult<()>,
) -> CliResult<()> {
    let table = if q.table == "records" {
        "records"
    } else if q.table == "devices" {
        "devices"
    } else {
        "measurements"
    };
    let mut attempt_index = if table == "records" {
        Some(store::IndexedReader::open(&cache.root, "attempts")?)
    } else {
        None
    };
    store::scan_selected(
        cache,
        table,
        if table == "measurements" {
            &q.tests
        } else {
            &[]
        },
        q.run,
        &q.attempts,
        |mut r| {
            if table == "records" && r.attempt > 0 {
                let a: Row = attempt_index.as_mut().unwrap().get(r.attempt)?;
                r.site = a.site;
                r.head = a.head;
                r.wafer = a.wafer;
                r.lot = a.lot;
                r.part = a.part;
                r.hard_bin = a.hard_bin;
                r.soft_bin = a.soft_bin;
                r.latest = a.latest;
                r.first = a.first;
            }
            if !q.scope(&r) {
                return Ok(());
            }
            if analysis
                && (!q.included(&r, table == "measurements")
                    || (table == "measurements" && !r.last_execution))
            {
                return Ok(());
            }
            // Devices and Records remain forensic views, including unresolved attempts.
            if !analysis && table == "measurements" && !q.population(&r) {
                return Ok(());
            }
            f(r)
        },
    )
}
fn display(r: &Row, q: &Query) -> bool {
    matches(r, &q.filters)
        && q.values
            .iter()
            .all(|(key, values)| values.contains(&field(r, key)))
        && (q.search.is_empty()
            || serde_json::to_string(r)
                .unwrap_or_default()
                .to_lowercase()
                .contains(&q.search.to_lowercase()))
}
pub fn rows(cache: &Cache, q: &Query) -> CliResult<Value> {
    q.validate()?;
    if ["tests", "wafers", "hard_bin", "soft_bin"].contains(&q.table.as_str()) {
        return summaries(cache, q);
    }
    use sha2::{Digest, Sha256};
    let mut normalized = q.clone();
    normalized.offset = 0;
    normalized.limit = 0;
    normalized.pane = 0;
    let key = format!("{:x}", Sha256::digest(serde_json::to_vec(&normalized)?));
    if let Some(snapshot) = cache.query_pages.borrow().get(&key) {
        return snapshot.page(cache, q);
    }
    if cache.query_pages.borrow().len() >= 2 {
        let oldest = cache.query_pages.borrow().keys().next().cloned();
        if let Some(k) = oldest {
            cache.query_pages.borrow_mut().remove(&k);
        }
    }
    let mut sorted = cache.spill()?;
    let mut total = 0;
    let mut included = 0;
    let (mut duration_sum, mut duration_count) = (0u64, 0u64);
    visit(cache, q, false, |r| {
        if q.plot == "timing" && !q.included(&r, false) {
            return Ok(());
        }
        if q.plot == "timing" {
            if let Some(ms) = r.duration {
                duration_sum += u64::from(ms);
                duration_count += 1;
            }
        }
        if q.included(&r, q.table == "measurements") {
            included += 1;
        }
        if display(&r, q) {
            sorted.push(&sort_key(&r, q), &serde_json::to_vec(&r)?)?;
            total += 1;
        }
        Ok(())
    })?;
    let root = cache.root.join(format!(
        ".viewer-query-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos()
    ));
    fs::create_dir(&root)?;
    let mut snapshot = PageSnapshot {
        root: root.canonicalize()?,
        parent: cache.root.canonicalize()?,
        metadata: json!({"total":total,"included":included,"population":q.population,"timing_summary":{"count":duration_count,"sum_ms":duration_sum,"missing":if q.plot=="timing"{included as u64-duration_count}else{0}}}),
    };
    let mut writer = store::IndexedWriter::new(&root, "rows")?;
    let mut marks = std::io::BufWriter::new(File::create(root.join("marks.bin"))?);
    let mut bytes = 0u64;
    let disk = (cache.disk.saturating_sub(store::disk_bytes(&cache.root)?)) / 8;
    let mut it = sorted.finish()?;
    let mut index = 0;
    let mut annotation_count = 0;
    while let Some(r) = it.next_record()? {
        cache.check()?;
        bytes += r.value.len() as u64 + 13;
        if bytes > disk {
            return Err("paged query index exceeds scratch budget; narrow the scope".into());
        }
        let row: Row = serde_json::from_slice(&r.value)?;
        let alert = !q.alerts.is_empty() && matches(&row, &q.alerts);
        let failed = row.passed == Some(false);
        let excluded = if q.table == "devices" {
            q.excluded_devices.contains(&row.id)
        } else {
            q.excluded_measurements.contains(&row.id)
        };
        let marked = match q.annotation.as_str() {
            "failure" => failed,
            "alarm" => row.alarm,
            "exclusion" => excluded,
            "alert" => alert,
            _ => failed || row.alarm || excluded || alert,
        };
        let bits = if marked {
            annotation_count += 1;
            1 | u8::from(failed) * 2
                | u8::from(row.alarm) * 4
                | u8::from(excluded) * 8
                | u8::from(alert) * 16
        } else {
            0
        };
        marks.write_all(&[bits])?;
        writer.push(index + 1, &row)?;
        index += 1;
    }
    marks.flush()?;
    drop(marks);
    drop(writer);
    snapshot.metadata["annotation_count"] = json!(annotation_count);
    let result = snapshot.page(cache, q)?;
    cache.query_pages.borrow_mut().insert(key, snapshot);
    Ok(result)
}

pub(super) struct PageSnapshot {
    root: PathBuf,
    parent: PathBuf,
    metadata: Value,
}
impl Drop for PageSnapshot {
    fn drop(&mut self) {
        if let Ok(path) = self.root.canonicalize() {
            if path.parent() == Some(self.parent.as_path())
                && path
                    .file_name()
                    .is_some_and(|n| n.to_string_lossy().starts_with(".viewer-query-"))
            {
                let _ = fs::remove_dir_all(path);
            }
        }
    }
}
impl PageSnapshot {
    fn page(&self, cache: &Cache, q: &Query) -> CliResult<Value> {
        cache.check()?;
        let total = self.metadata["total"].as_u64().unwrap_or(0) as usize;
        let mut reader = store::IndexedReader::open(&self.root, "rows")?;
        let mut rows = Vec::<Row>::new();
        for i in q.offset..q.offset.saturating_add(q.limit).min(total) {
            rows.push(reader.get(i as u64 + 1)?);
        }
        let mut marks = File::open(self.root.join("marks.bin"))?;
        let mut buffer = [0; 16384];
        let mut index = 0;
        let mut next = None;
        let mut previous = None;
        let mut annotations = Vec::new();
        loop {
            cache.check()?;
            let n = marks.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            for &bits in &buffer[..n] {
                if bits & 1 != 0 {
                    if index > q.offset && next.is_none() {
                        next = Some(index);
                    }
                    if index < q.offset {
                        previous = Some(index);
                    }
                    if annotations.len() < 2000 {
                        annotations.push(json!({"index":index,"failed":bits&2!=0,"alarm":bits&4!=0,"excluded":bits&8!=0,"alert":bits&16!=0}));
                    }
                }
                index += 1;
            }
        }
        let mut result = self.metadata.clone();
        result["rows"] = json!(rows);
        result["offset"] = json!(q.offset);
        result["annotations"] = json!(annotations);
        result["next_annotation"] = json!(next);
        result["previous_annotation"] = json!(previous);
        result["annotation_limit"] = json!(2000);
        Ok(result)
    }
}

fn summaries(cache: &Cache, q: &Query) -> CliResult<Value> {
    let mut groups: BTreeMap<String, Value> = BTreeMap::new();
    let mut input = q.clone();
    input.table = if q.table == "tests" {
        "measurements"
    } else {
        "devices"
    }
    .into();
    input.tests.clear();
    visit(cache, &input, true, |r| {
        let (key, label) = match q.table.as_str() {
            "wafers" => (text(&json!(r.wafer)), text(&json!(r.wafer))),
            "hard_bin" => (text(&json!(r.hard_bin)), text(&json!(r.hard_bin))),
            "soft_bin" => (text(&json!(r.soft_bin)), text(&json!(r.soft_bin))),
            _ => (
                r.test.clone(),
                format!(
                    "{} {} {}{}",
                    r.kind,
                    r.number.map_or(String::new(), |v| v.to_string()),
                    r.name,
                    r.channel
                        .map_or(String::new(), |v| format!(" [result {v}]"))
                ),
            ),
        };
        if groups.len() > cache.memory / 8192 {
            return Err("summary group budget exceeded; narrow the run/wafer scope".into());
        }
        let g=groups.entry(key.clone()).or_insert_with(||json!({"key":key,"name":label,"count":0,"pass":0,"fail":0,"unknown":0,"min":null,"max":null,"sum":0.,"numeric":0,"units":r.units}));
        for k in [
            "count",
            match r.passed {
                Some(true) => "pass",
                Some(false) => "fail",
                None => "unknown",
            },
        ] {
            g[k] = json!(g[k].as_u64().unwrap() + 1);
        }
        if let Some(v) = r.value {
            g["min"] = json!(g["min"].as_f64().map_or(v, |n| n.min(v)));
            g["max"] = json!(g["max"].as_f64().map_or(v, |n| n.max(v)));
            g["sum"] = json!(g["sum"].as_f64().unwrap() + v);
            g["numeric"] = json!(g["numeric"].as_u64().unwrap() + 1);
        }
        Ok(())
    })?;
    let mut values: Vec<_> = groups
        .into_values()
        .map(|mut v| {
            v["mean"] = json!(if v["numeric"] == 0 {
                None
            } else {
                Some(v["sum"].as_f64().unwrap() / v["numeric"].as_u64().unwrap() as f64)
            });
            v
        })
        .collect();
    values.sort_by(|a, b| {
        let key = if q.sort.is_empty() { "name" } else { &q.sort };
        let order = if let (Some(a), Some(b)) = (a[key].as_f64(), b[key].as_f64()) {
            a.total_cmp(&b)
        } else {
            text(&a[key]).cmp(&text(&b[key]))
        };
        if q.descending {
            order.reverse()
        } else {
            order
        }
    });
    values.retain(|v| {
        q.search.is_empty()
            || v.to_string()
                .to_lowercase()
                .contains(&q.search.to_lowercase())
    });
    let total = values.len();
    Ok(
        json!({"rows":values.into_iter().skip(q.offset).take(q.limit).collect::<Vec<_>>(),"total":total,"population":q.population}),
    )
}

#[derive(Default, Serialize)]
struct Stats {
    count: u64,
    invalid: u64,
    missing: u64,
    nonfinite: u64,
    not_numeric: u64,
    mean: f64,
    m2: f64,
    min: Option<f64>,
    max: Option<f64>,
    sigma: Option<f64>,
    quantiles: Vec<f64>,
    histogram: Vec<u64>,
    low: Option<f64>,
    high: Option<f64>,
    variable_limits: bool,
    units: String,
    points: Vec<Row>,
    cdf: Vec<Value>,
    hist_min: f64,
    hist_max: f64,
    sampled: bool,
    ppk: Option<f64>,
}
impl Stats {
    fn push(&mut self, r: &Row) {
        if let Some(x) = r.value {
            self.count += 1;
            let delta = x - self.mean;
            self.mean += delta / self.count as f64;
            self.m2 += delta * (x - self.mean);
            self.min = Some(self.min.map_or(x, |v| v.min(x)));
            self.max = Some(self.max.map_or(x, |v| v.max(x)));
            if self.count == 1 {
                self.low = r.low;
                self.high = r.high;
                self.units = r.units.clone();
            } else {
                self.variable_limits |= self.low != r.low || self.high != r.high;
            }
        } else {
            match r.value_state.as_str() {
                "missing" => self.missing += 1,
                "nonfinite" => self.nonfinite += 1,
                "not_numeric" => self.not_numeric += 1,
                _ => self.invalid += 1,
            }
        }
    }
}
pub fn plot(cache: &Cache, q: &Query) -> CliResult<Value> {
    q.validate()?;
    let mut data = plot_data(cache, q)?;
    let mut input = q.clone();
    input.table = if q.plot == "binmap"
        || (q.plot == "pareto" && ["hard_bin", "soft_bin"].contains(&q.level.as_str()))
    {
        "devices"
    } else {
        "measurements"
    }
    .into();
    let mut included = 0;
    let mut excluded = 0;
    visit(cache, &input, false, |r| {
        if input.population(&r) && (input.table == "devices" || r.last_execution) {
            if input.included(&r, input.table == "measurements") {
                included += 1;
            } else {
                excluded += 1;
            }
        }
        Ok(())
    })?;
    data["included"] = json!(included);
    data["excluded"] = json!(excluded);
    Ok(data)
}
fn plot_data(cache: &Cache, q: &Query) -> CliResult<Value> {
    q.validate()?;
    if q.plot == "pareto" {
        return pareto(cache, q);
    }
    if q.plot == "binmap" || q.plot == "paramap" {
        return map(cache, q);
    }
    if q.plot == "scatter" {
        return scatter(cache, q);
    }
    let mut q = q.clone();
    q.table = "measurements".into();
    q.filters.clear();
    let mut groups: BTreeMap<String, Stats> = BTreeMap::new();
    let mut sort = cache.spill()?;
    let group_key = |r: &Row| {
        json!([
            r.test,
            if q.color == "site" {
                format!("{}/{}", r.head, r.site)
            } else {
                String::new()
            },
            r.units
        ])
        .to_string()
    };
    visit(cache, &q, true, |r| {
        let key = group_key(&r);
        if groups.len() > 256 && !groups.contains_key(&key) {
            return Err("plot exceeds 256 series; select fewer tests/sites".into());
        }
        groups.entry(key.clone()).or_default().push(&r);
        if let Some(n) = r.value {
            let mut k = key.into_bytes();
            k.push(0);
            k.extend_from_slice(&float_key(n));
            sort.push(&k, &serde_json::to_vec(&(group_key(&r), r))?)?;
        }
        Ok(())
    })?;
    let mut bounds: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for s in groups.values() {
        if let Some((lo, hi)) = s.min.zip(s.max) {
            let b = bounds.entry(s.units.clone()).or_insert((lo, hi));
            b.0 = b.0.min(lo);
            b.1 = b.1.max(hi);
        }
    }
    for s in groups.values_mut() {
        if let Some(&(lo, hi)) = bounds.get(&s.units) {
            s.hist_min = lo;
            if hi == lo {
                s.hist_min = lo - 0.5;
                s.hist_max = hi + 0.5;
            } else {
                s.hist_max = hi;
            }
        }
    }
    let point_limit = (cache.memory / 32 / groups.len().max(1) / 2048).clamp(1, 2000) as u64;
    let mut sorted = sort.finish()?;
    let mut indices: BTreeMap<String, u64> = BTreeMap::new();
    let percent = [q.low_whisker, 25., 50., 75., q.high_whisker];
    while let Some(item) = sorted.next_record()? {
        cache.check()?;
        let (key, row): (String, Row) = serde_json::from_slice(&item.value)?;
        let n = row.value.unwrap();
        let i = indices.entry(key.clone()).or_default();
        let s = groups.get_mut(&key).unwrap();
        if s.quantiles.is_empty() {
            s.quantiles = vec![0.; 5];
            s.histogram = vec![0; q.bins];
        }
        for (p, out) in percent.iter().zip(s.quantiles.iter_mut()) {
            let rank = (s.count - 1) as f64 * p / 100.;
            let floor = rank.floor() as u64;
            let ceil = rank.ceil() as u64;
            if *i == floor {
                *out += n * (if floor == ceil { 1. } else { 1. - rank.fract() });
            }
            if *i == ceil && ceil != floor {
                *out += n * rank.fract();
            }
        }
        let min = s.hist_min;
        let max = s.hist_max;
        let bin = if max == min {
            0
        } else {
            (((n - min) / (max - min) * q.bins as f64) as usize).min(q.bins - 1)
        };
        s.histogram[bin] += 1;
        *i += 1;
        let probability = *i as f64 / s.count as f64 * 100.;
        if s.cdf.last().is_some_and(|p| p["value"].as_f64() == Some(n)) {
            s.cdf.last_mut().unwrap()["probability"] = json!(probability);
        } else if (*i - 1) % s.count.div_ceil(point_limit).max(1) == 0 || *i == s.count {
            s.cdf.push(json!({"value":n,"probability":probability,"record":row.record,"attempt":row.attempt}));
        }
    }
    let mut seen: BTreeMap<String, u64> = BTreeMap::new();
    visit(cache, &q, true, |r| {
        let key = group_key(&r);
        let s = groups.get_mut(&key).unwrap();
        if r.value.is_some() {
            let n = seen.entry(key).or_default();
            let stride = s.count.div_ceil(point_limit).max(1);
            if *n % stride == 0 {
                s.points.push(r);
            }
            *n += 1;
            s.sampled = s.count > s.points.len() as u64;
        }
        Ok(())
    })?;
    for s in groups.values_mut() {
        s.sigma = (s.count > 1).then(|| (s.m2 / (s.count - 1) as f64).sqrt());
        if !s.variable_limits {
            if let (Some(l), Some(h), Some(sd)) = (s.low, s.high, s.sigma) {
                if sd > 0. && h > l {
                    s.ppk = Some(((h - s.mean) / (3. * sd)).min((s.mean - l) / (3. * sd)));
                }
            }
        }
    }
    Ok(
        json!({"series":groups,"population":q.population,"quantile_method":"linear interpolation","sigma_method":"sample standard deviation","points_limit_per_series":point_limit,"exact_statistics":true}),
    )
}
fn scatter(cache: &Cache, q: &Query) -> CliResult<Value> {
    if q.tests.len() != 2 {
        let mut range = q.clone();
        range.plot = "range".into();
        return plot(cache, &range);
    }
    let mut sort = cache.spill()?;
    let mut input = q.clone();
    input.table = "measurements".into();
    visit(cache, &input, true, |r| {
        if r.value.is_some() {
            sort.push(&r.attempt.to_be_bytes(), &serde_json::to_vec(&r)?)?;
        }
        Ok(())
    })?;
    let mut stream = sort.finish()?;
    let mut group: Vec<Row> = Vec::new();
    let mut id = 0;
    let mut count = 0u64;
    let mut unpaired = 0;
    let mut points = Vec::new();
    let (mut mx, mut my, mut xx, mut yy, mut xy) = (0., 0., 0., 0., 0.);
    let mut emit = |g: &[Row]| {
        let a = g
            .iter()
            .filter(|r| r.test == q.tests[0])
            .max_by_key(|r| r.record);
        let b = g
            .iter()
            .filter(|r| r.test == q.tests[1])
            .max_by_key(|r| r.record);
        if let (Some(a), Some(b)) = (a, b) {
            let x = a.value.unwrap();
            let y = b.value.unwrap();
            count += 1;
            let dx = x - mx;
            let dy = y - my;
            mx += dx / count as f64;
            my += dy / count as f64;
            xx += dx * (x - mx);
            yy += dy * (y - my);
            xy += dx * (y - my);
            if points.len() < 20000 {
                points.push(json!({"x":x,"y":y,"record":a.record,"other_record":b.record,"attempt":a.attempt,"site":a.site}));
            }
        } else if !g.is_empty() {
            unpaired += 1;
        }
    };
    while let Some(item) = stream.next_record()? {
        let r: Row = serde_json::from_slice(&item.value)?;
        if r.attempt != id {
            emit(&group);
            group.clear();
            id = r.attempt;
        }
        group.push(r);
        if group.len() > 10000 {
            return Err("scatter duplicate-series budget exceeded".into());
        }
    }
    emit(&group);
    Ok(
        json!({"points":points,"count":count,"unpaired":unpaired,"sampled":count>20000,"correlation":if count>1&&xx>0.&&yy>0.{Some(xy/(xx*yy).sqrt())}else{None},"population":q.population}),
    )
}
fn pareto(cache: &Cache, q: &Query) -> CliResult<Value> {
    let mut input = q.clone();
    let bins = q.level == "hard_bin" || q.level == "soft_bin";
    input.table = if bins { "devices" } else { "measurements" }.into();
    let mut sort = cache.spill()?;
    visit(cache, &input, true, |r| {
        if !bins && r.channel.is_some() {
            return Ok(());
        }
        if q.level == "pattern" && r.kind != "FTR" {
            return Ok(());
        }
        let label = match q.level.as_str() {
            "hard_bin" => text(&json!(r.hard_bin)),
            "soft_bin" => text(&json!(r.soft_bin)),
            "pattern" => r.pattern.clone().unwrap_or_else(|| "[null]".into()),
            _ => format!("{} {} {}", r.kind, r.number.unwrap_or(0), r.name),
        };
        let group = if q.color == "site" {
            format!("{}/{}", r.head, r.site)
        } else if q.color == "wafer" {
            text(&json!(r.wafer))
        } else if q.color == "lot" {
            r.lot.clone()
        } else {
            "All".into()
        };
        sort.push(
            json!([label, r.attempt]).to_string().as_bytes(),
            &serde_json::to_vec(&(label, group, r.passed))?,
        )?;
        Ok(())
    })?;
    let mut result: BTreeMap<String, Value> = BTreeMap::new();
    let mut stream = sort.finish()?;
    let mut key = Vec::new();
    let mut current: Option<(String, String, Option<bool>)> = None;
    let mut emit = |v: Option<(String, String, Option<bool>)>| -> CliResult<()> {
        if result.len() > cache.memory / 8192 {
            return Err("Pareto group budget exceeded".into());
        }
        if let Some((label, group, p)) = v {
            let e = result.entry(label.clone()).or_insert_with(
                || json!({"label":label,"count":0,"pass":0,"fail":0,"unknown":0,"groups":{}}),
            );
            let verdict = match p {
                Some(true) => "pass",
                Some(false) => "fail",
                None => "unknown",
            };
            e[verdict] = json!(e[verdict].as_u64().unwrap() + 1);
            if bins || p == Some(false) {
                e["count"] = json!(e["count"].as_u64().unwrap() + 1);
                e["groups"][&group] = json!(e["groups"][&group].as_u64().unwrap_or(0) + 1);
            }
        }
        Ok(())
    };
    while let Some(item) = stream.next_record()? {
        cache.check()?;
        let v: (String, String, Option<bool>) = serde_json::from_slice(&item.value)?;
        if item.key != key {
            emit(current.take())?;
            key = item.key;
            current = Some(v);
        } else if let Some(c) = &mut current {
            c.2 = match (c.2, v.2) {
                (Some(false), _) | (_, Some(false)) => Some(false),
                (Some(true), Some(true)) => Some(true),
                _ => None,
            };
        }
    }
    emit(current)?;
    let mut modes: Vec<_> = result
        .into_values()
        .filter(|v| {
            if bins {
                q.show_pass || v["fail"] != 0 || v["unknown"] != 0
            } else {
                v["count"] != 0
            }
        })
        .collect();
    modes.sort_by_key(|v| std::cmp::Reverse(v["count"].as_u64().unwrap()));
    if serde_json::to_vec(&modes)?.len() > cache.memory / 8 {
        return Err("Pareto response exceeds memory budget".into());
    }
    Ok(json!({"modes":modes,"population":q.population}))
}
fn map(cache: &Cache, q: &Query) -> CliResult<Value> {
    let mut input = q.clone();
    input.table = if q.plot == "binmap" {
        "devices"
    } else {
        "measurements"
    }
    .into();
    let mut cells = Vec::new();
    let mut missing = 0;
    visit(cache, &input, true, |r| {
        if r.x.is_some() && r.y.is_some() && r.wafer.is_some() {
            if cells.len() >= 20000 {
                return Err("map exceeds 20000 entries; select one wafer/test".into());
            }
            cells.push(r);
        } else {
            missing += 1;
        }
        Ok(())
    })?;
    Ok(json!({"cells":cells,"missing_coordinates":missing,"population":q.population}))
}

pub fn all_rows(cache: &Cache, q: &Query, limit: usize) -> CliResult<Vec<Row>> {
    let mut result = Vec::new();
    let mut bytes = 0;
    visit(cache, q, false, |r| {
        if display(&r, q) {
            bytes += serde_json::to_vec(&r)?.len();
            if bytes > limit {
                return Err(
                    "export exceeds size budget; narrow the selection or choose summary".into(),
                );
            }
            result.push(r);
        }
        Ok(())
    })?;
    Ok(result)
}

pub fn distinct(cache: &Cache, q: &Query, key: &str) -> CliResult<Value> {
    q.validate()?;
    let mut input = q.clone();
    input.values.remove(key);
    input.filters.remove(key);
    let mut values: BTreeMap<String, (Value, u64)> = BTreeMap::new();
    visit(cache, &input, false, |r| {
        if display(&r, &input) {
            let v = field(&r, key);
            if values.len() >= 10000 && !values.contains_key(&v.to_string()) {
                return Err(
                    "column has more than 10000 values; use a text or numeric filter".into(),
                );
            }
            let a = values.entry(v.to_string()).or_insert((v, 0));
            a.1 += 1;
        }
        Ok(())
    })?;
    Ok(
        json!({"values":values.into_values().map(|(value,count)|json!({"value":value,"count":count})).collect::<Vec<_>>()}),
    )
}
