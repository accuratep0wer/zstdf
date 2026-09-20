use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error;
use std::fs::File;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use arrow::array::{
    Array, BooleanArray, Float32Array, Int16Array, StringArray, UInt16Array, UInt32Array,
    UInt64Array, UInt8Array,
};
use arrow::record_batch::RecordBatch;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

mod dataset;
pub use dataset::{generate_dataset_dashboard, DatasetLimits};

#[derive(Debug, Clone)]
pub struct DashboardOptions {
    pub title: String,
    pub max_correlation_tests: usize,
    pub max_items: usize,
    pub ftr_inputs: Vec<std::path::PathBuf>,
}

impl Default for DashboardOptions {
    fn default() -> Self {
        Self {
            title: "zstdf DataView".to_string(),
            max_correlation_tests: 16,
            max_items: 40,
            ftr_inputs: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DashboardSummary {
    pub rows: usize,
    pub parts: usize,
    pub yield_percent: f64,
}

#[derive(Debug, Clone)]
struct TestRow {
    part_sequence: u64,
    part_merge_key: Option<String>,
    lot_id: String,
    wafer_id: Option<String>,
    part_id: String,
    head_num: u8,
    site_num: u8,
    x_coord: Option<i16>,
    y_coord: Option<i16>,
    hard_bin: u16,
    soft_bin: u16,
    part_pass: bool,
    test_num: u32,
    test_txt: Option<String>,
    test_type: String,
    result: Option<f32>,
    test_pass: Option<bool>,
    lo_limit: Option<f32>,
    hi_limit: Option<f32>,
    units: Option<String>,
    test_time_ms: Option<u32>,
}

#[derive(Debug, Clone)]
struct DashboardData {
    title: String,
    generated_at_unix: u64,
    kpis: Kpis,
    pareto: Vec<TestAggregate>,
    wafer_yield: Vec<GroupAggregate>,
    site_yield: Vec<GroupAggregate>,
    hard_bins: Vec<GroupAggregate>,
    soft_bins: Vec<GroupAggregate>,
    commonality: Vec<CommonalityAggregate>,
    correlations: Vec<CorrelationPair>,
    heatmap_points: Vec<HeatmapPoint>,
    process_windows: Vec<ProcessWindow>,
    data_quality: Vec<GroupAggregate>,
}

#[derive(Debug, Clone)]
struct Kpis {
    rows: usize,
    parts: usize,
    passed_parts: usize,
    failed_parts: usize,
    yield_percent: f64,
    tests: usize,
    lots: usize,
    wafers: usize,
    sites: usize,
    failing_tests: usize,
}

#[derive(Debug, Clone)]
struct TestAggregate {
    label: String,
    test_num: u32,
    test_type: String,
    units: Option<String>,
    total: usize,
    pass: usize,
    fail: usize,
    unknown: usize,
    limit_fail: usize,
    mean: Option<f64>,
    sigma: Option<f64>,
    min: Option<f64>,
    max: Option<f64>,
    lo_limit: Option<f64>,
    hi_limit: Option<f64>,
    yield_percent: f64,
}

#[derive(Debug, Clone)]
struct GroupAggregate {
    label: String,
    total: usize,
    pass: usize,
    fail: usize,
    yield_percent: f64,
}

#[derive(Debug, Clone)]
struct CommonalityAggregate {
    dimension: String,
    label: String,
    count: usize,
}

#[derive(Debug, Clone)]
struct CorrelationPair {
    x: String,
    y: String,
    correlation: f64,
    pairs: usize,
}

#[derive(Debug, Clone)]
struct HeatmapPoint {
    x: i16,
    y: i16,
    total: usize,
    fail: usize,
    yield_percent: f64,
}

#[derive(Debug, Clone)]
struct ProcessWindow {
    label: String,
    mean: f64,
    sigma: f64,
    min: f64,
    max: f64,
    lo_limit: Option<f64>,
    hi_limit: Option<f64>,
    limit_fail: usize,
    cp: Option<f64>,
}

#[derive(Debug, Clone)]
struct PartInfo {
    wafer_id: Option<String>,
    part_pass: bool,
    head_num: u8,
    site_num: u8,
    hard_bin: u16,
    soft_bin: u16,
}

#[derive(Debug, Clone)]
struct RunningTest {
    label: String,
    test_num: u32,
    test_type: String,
    units: Option<String>,
    total: usize,
    pass: usize,
    fail: usize,
    unknown: usize,
    limit_fail: usize,
    count: usize,
    sum: f64,
    sum_sq: f64,
    min: Option<f64>,
    max: Option<f64>,
    lo_limit: Option<f64>,
    hi_limit: Option<f64>,
}

impl RunningTest {
    fn new(row: &TestRow) -> Self {
        Self {
            label: test_label(row),
            test_num: row.test_num,
            test_type: row.test_type.clone(),
            units: row.units.clone(),
            total: 0,
            pass: 0,
            fail: 0,
            unknown: 0,
            limit_fail: 0,
            count: 0,
            sum: 0.0,
            sum_sq: 0.0,
            min: None,
            max: None,
            lo_limit: row.lo_limit.map(f64::from),
            hi_limit: row.hi_limit.map(f64::from),
        }
    }

    fn push(&mut self, row: &TestRow) {
        self.total += 1;
        match row.test_pass {
            Some(true) => self.pass += 1,
            Some(false) => self.fail += 1,
            None => self.unknown += 1,
        }

        if let Some(value) = row.result.map(f64::from).filter(|value| value.is_finite()) {
            self.count += 1;
            self.sum += value;
            self.sum_sq += value * value;
            self.min = Some(self.min.map_or(value, |current| current.min(value)));
            self.max = Some(self.max.map_or(value, |current| current.max(value)));

            if row.lo_limit.is_some_and(|limit| value < f64::from(limit))
                || row.hi_limit.is_some_and(|limit| value > f64::from(limit))
            {
                self.limit_fail += 1;
            }
        }

        if self.units.is_none() {
            self.units = row.units.clone();
        }
        if self.lo_limit.is_none() {
            self.lo_limit = row.lo_limit.map(f64::from);
        }
        if self.hi_limit.is_none() {
            self.hi_limit = row.hi_limit.map(f64::from);
        }
    }

    fn finish(self) -> TestAggregate {
        let mean = (self.count > 0).then(|| self.sum / self.count as f64);
        let sigma = if self.count > 1 {
            let variance = ((self.sum_sq - self.sum * self.sum / self.count as f64)
                / (self.count - 1) as f64)
                .max(0.0);
            Some(variance.sqrt())
        } else {
            None
        };
        let known = self.pass + self.fail;
        TestAggregate {
            label: self.label,
            test_num: self.test_num,
            test_type: self.test_type,
            units: self.units,
            total: self.total,
            pass: self.pass,
            fail: self.fail,
            unknown: self.unknown,
            limit_fail: self.limit_fail,
            mean,
            sigma,
            min: self.min,
            max: self.max,
            lo_limit: self.lo_limit,
            hi_limit: self.hi_limit,
            yield_percent: percent(self.pass, known),
        }
    }
}

pub fn generate_dashboard(
    input: &Path,
    output: &Path,
    options: DashboardOptions,
) -> Result<DashboardSummary, Box<dyn Error>> {
    let data = analyze_parquet(input, &options)?;
    let summary = DashboardSummary {
        rows: data.kpis.rows,
        parts: data.kpis.parts,
        yield_percent: data.kpis.yield_percent,
    };
    if output.canonicalize().ok().as_ref() == Some(&input.canonicalize()?) {
        return Err(invalid_data("dashboard output must not overwrite input Parquet").into());
    }
    let html = crate::ftr_pareto::attach(render_html(&data), &options.ftr_inputs, output)?;
    let html = crate::report_ui::decorate(html);
    stdf_parquet::catalog::atomic_write(output, html.as_bytes())?;
    Ok(summary)
}

fn analyze_parquet(
    path: &Path,
    options: &DashboardOptions,
) -> Result<DashboardData, Box<dyn Error>> {
    let file = File::open(path)?;
    let reader = ParquetRecordBatchReaderBuilder::try_new(file)?;
    require_coordinate_schema(reader.schema().as_ref())?;
    let reader = reader.build()?;
    let mut analyzer = AnalysisAccumulator::default();
    for batch in reader {
        let batch = batch?;
        for row in rows_from_batch(&batch)? {
            analyzer.push(&row);
        }
    }
    Ok(analyzer.finish(options))
}

fn require_coordinate_schema(schema: &arrow::datatypes::Schema) -> Result<(), Box<dyn Error>> {
    if schema != stdf_arrow::eav_schema().as_ref() {
        return Err(invalid_data("dashboard requires eav-v2 coordinate identity columns; reconvert the source STDF to a new Parquet file/dataset").into());
    }
    Ok(())
}

fn rows_from_batch(batch: &RecordBatch) -> Result<Vec<TestRow>, Box<dyn Error>> {
    require_coordinate_schema(batch.schema().as_ref())?;
    let part_sequence = batch
        .column(stdf_arrow::schema::PART_SEQUENCE)
        .as_any()
        .downcast_ref::<UInt64Array>()
        .ok_or_else(|| invalid_data("part_sequence column is not UInt64"))?;
    let part_merge_key = string_col(batch, stdf_arrow::schema::PART_MERGE_KEY, "part_merge_key")?;
    let lot_id = string_col(batch, stdf_arrow::schema::LOT_ID, "lot_id")?;
    let wafer_id = string_col(batch, stdf_arrow::schema::WAFER_ID, "wafer_id")?;
    let part_id = string_col(batch, stdf_arrow::schema::PART_ID, "part_id")?;
    let head_num = u8_col(batch, stdf_arrow::schema::HEAD_NUM, "head_num")?;
    let site_num = u8_col(batch, stdf_arrow::schema::SITE_NUM, "site_num")?;
    let x_coord = i16_col(batch, stdf_arrow::schema::X_COORD, "x_coord")?;
    let y_coord = i16_col(batch, stdf_arrow::schema::Y_COORD, "y_coord")?;
    let hard_bin = u16_col(batch, stdf_arrow::schema::HARD_BIN, "hard_bin")?;
    let soft_bin = u16_col(batch, stdf_arrow::schema::SOFT_BIN, "soft_bin")?;
    let part_pass = bool_col(batch, stdf_arrow::schema::PART_PASS, "part_pass")?;
    let test_num = u32_col(batch, stdf_arrow::schema::TEST_NUM, "test_num")?;
    let test_txt = string_col(batch, stdf_arrow::schema::TEST_TXT, "test_txt")?;
    let test_type = string_col(batch, stdf_arrow::schema::TEST_TYPE, "test_type")?;
    let result = f32_col(batch, stdf_arrow::schema::RESULT, "result")?;
    let test_pass = bool_col(batch, stdf_arrow::schema::TEST_PASS, "test_pass")?;
    let lo_limit = f32_col(batch, stdf_arrow::schema::LO_LIMIT, "lo_limit")?;
    let hi_limit = f32_col(batch, stdf_arrow::schema::HI_LIMIT, "hi_limit")?;
    let units = string_col(batch, stdf_arrow::schema::UNITS, "units")?;
    let test_time_ms = u32_col(batch, stdf_arrow::schema::TEST_TIME_MS, "test_time_ms")?;

    let mut rows = Vec::with_capacity(batch.num_rows());
    for row in 0..batch.num_rows() {
        if part_sequence.is_null(row) {
            return Err(invalid_data("part_sequence must not be null").into());
        }
        rows.push(TestRow {
            part_sequence: part_sequence.value(row),
            part_merge_key: optional_string(part_merge_key, row),
            lot_id: required_string(lot_id, row, "lot_id")?,
            wafer_id: optional_string(wafer_id, row),
            part_id: required_string(part_id, row, "part_id")?,
            head_num: required_u8(head_num, row, "head_num")?,
            site_num: required_u8(site_num, row, "site_num")?,
            x_coord: optional_i16(x_coord, row),
            y_coord: optional_i16(y_coord, row),
            hard_bin: required_u16(hard_bin, row, "hard_bin")?,
            soft_bin: required_u16(soft_bin, row, "soft_bin")?,
            part_pass: required_bool(part_pass, row, "part_pass")?,
            test_num: required_u32(test_num, row, "test_num")?,
            test_txt: optional_string(test_txt, row),
            test_type: required_string(test_type, row, "test_type")?,
            result: optional_f32(result, row),
            test_pass: optional_bool(test_pass, row),
            lo_limit: optional_f32(lo_limit, row),
            hi_limit: optional_f32(hi_limit, row),
            units: optional_string(units, row),
            test_time_ms: optional_u32(test_time_ms, row),
        });
    }
    Ok(rows)
}

#[cfg(test)]
fn analyze_rows(rows: &[TestRow], options: &DashboardOptions) -> DashboardData {
    let mut analyzer = AnalysisAccumulator::default();
    for row in rows {
        analyzer.push(row);
    }
    analyzer.finish(options)
}

#[derive(Debug)]
struct AnalysisAccumulator {
    row_count: usize,
    parts: BTreeMap<String, PartInfo>,
    tests: BTreeMap<u32, RunningTest>,
    lots: BTreeSet<String>,
    wafers: BTreeSet<String>,
    commonality: BTreeMap<(String, String), usize>,
    part_results: BTreeMap<String, BTreeMap<u32, f64>>,
    heatmap: BTreeMap<(i16, i16), (usize, usize)>,
    data_quality: BTreeMap<String, (usize, usize)>,
}

impl Default for AnalysisAccumulator {
    fn default() -> Self {
        Self {
            row_count: 0,
            parts: BTreeMap::new(),
            tests: BTreeMap::new(),
            lots: BTreeSet::new(),
            wafers: BTreeSet::new(),
            commonality: BTreeMap::new(),
            part_results: BTreeMap::new(),
            heatmap: BTreeMap::new(),
            data_quality: quality_seed(),
        }
    }
}

impl AnalysisAccumulator {
    fn push(&mut self, row: &TestRow) {
        self.push_scoped(row, "");
    }

    fn push_scoped(&mut self, row: &TestRow, source: &str) {
        self.row_count += 1;
        self.lots.insert(row.lot_id.clone());
        if let Some(wafer) = &row.wafer_id {
            self.wafers.insert(wafer.clone());
        }

        let part_key = scoped_part_key(row, source);
        let entry = self
            .parts
            .entry(part_key.clone())
            .or_insert_with(|| PartInfo {
                wafer_id: row.wafer_id.clone(),
                part_pass: row.part_pass,
                head_num: row.head_num,
                site_num: row.site_num,
                hard_bin: row.hard_bin,
                soft_bin: row.soft_bin,
            });
        entry.part_pass &= row.part_pass;

        self.tests
            .entry(row.test_num)
            .or_insert_with(|| RunningTest::new(row))
            .push(row);

        if let Some(result) = row.result.map(f64::from).filter(|value| value.is_finite()) {
            self.part_results
                .entry(part_key)
                .or_default()
                .entry(row.test_num)
                .or_insert(result);
        }

        if let (Some(x), Some(y)) = (row.x_coord, row.y_coord) {
            let point = self.heatmap.entry((x, y)).or_insert((0, 0));
            point.0 += 1;
            if is_failure(row) {
                point.1 += 1;
            }
        }

        push_quality(&mut self.data_quality, row);
        if is_failure(row) {
            push_commonality(&mut self.commonality, "test", test_label(row));
            push_commonality(&mut self.commonality, "wafer", wafer_label(row));
            push_commonality(
                &mut self.commonality,
                "site",
                format!("H{} / S{}", row.head_num, row.site_num),
            );
            push_commonality(
                &mut self.commonality,
                "hard_bin",
                format!("HBIN {}", row.hard_bin),
            );
            push_commonality(
                &mut self.commonality,
                "soft_bin",
                format!("SBIN {}", row.soft_bin),
            );
            push_commonality(
                &mut self.commonality,
                "test_site",
                format!(
                    "{} @ H{} / S{}",
                    test_label(row),
                    row.head_num,
                    row.site_num
                ),
            );
            push_commonality(
                &mut self.commonality,
                "test_hard_bin",
                format!("{} @ HBIN {}", test_label(row), row.hard_bin),
            );
        }
    }

    fn finish(self, options: &DashboardOptions) -> DashboardData {
        let passed_parts = self.parts.values().filter(|part| part.part_pass).count();
        let failed_parts = self.parts.len().saturating_sub(passed_parts);
        let mut pareto: Vec<_> = self.tests.into_values().map(RunningTest::finish).collect();
        pareto.sort_by(|left, right| {
            right
                .fail
                .cmp(&left.fail)
                .then(right.limit_fail.cmp(&left.limit_fail))
                .then(right.total.cmp(&left.total))
                .then(left.label.cmp(&right.label))
        });

        let failing_tests = pareto.iter().filter(|test| test.fail > 0).count();
        let process_windows = process_windows(&pareto, options.max_items);
        let correlations = correlations(&self.part_results, &pareto, options.max_correlation_tests);
        let heatmap_points = self
            .heatmap
            .into_iter()
            .map(|((x, y), (total, fail))| HeatmapPoint {
                x,
                y,
                total,
                fail,
                yield_percent: percent(total.saturating_sub(fail), total),
            })
            .collect();

        DashboardData {
            title: options.title.clone(),
            generated_at_unix: unix_seconds(),
            kpis: Kpis {
                rows: self.row_count,
                parts: self.parts.len(),
                passed_parts,
                failed_parts,
                yield_percent: percent(passed_parts, self.parts.len()),
                tests: pareto.len(),
                lots: self.lots.len(),
                wafers: self.wafers.len(),
                sites: unique_site_count(&self.parts),
                failing_tests,
            },
            pareto: pareto.into_iter().take(options.max_items).collect(),
            wafer_yield: grouped_part_yield(self.parts.values(), |part| {
                part.wafer_id
                    .clone()
                    .unwrap_or_else(|| "(no wafer)".to_string())
            }),
            site_yield: grouped_part_yield(self.parts.values(), |part| {
                format!("H{} / S{}", part.head_num, part.site_num)
            }),
            hard_bins: grouped_part_yield(self.parts.values(), |part| {
                format!("HBIN {}", part.hard_bin)
            }),
            soft_bins: grouped_part_yield(self.parts.values(), |part| {
                format!("SBIN {}", part.soft_bin)
            }),
            commonality: finish_commonality(self.commonality, options.max_items),
            correlations,
            heatmap_points,
            process_windows,
            data_quality: finish_quality(self.data_quality),
        }
    }
}

fn grouped_part_yield<'a>(
    parts: impl Iterator<Item = &'a PartInfo>,
    label: impl Fn(&PartInfo) -> String,
) -> Vec<GroupAggregate> {
    let mut groups: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for part in parts {
        let entry = groups.entry(label(part)).or_default();
        entry.0 += 1;
        if part.part_pass {
            entry.1 += 1;
        }
    }
    let mut values: Vec<_> = groups
        .into_iter()
        .map(|(label, (total, pass))| GroupAggregate {
            label,
            total,
            pass,
            fail: total.saturating_sub(pass),
            yield_percent: percent(pass, total),
        })
        .collect();
    values.sort_by(|left, right| {
        right
            .fail
            .cmp(&left.fail)
            .then(left.yield_percent.total_cmp(&right.yield_percent))
            .then(left.label.cmp(&right.label))
    });
    values
}

fn process_windows(tests: &[TestAggregate], max_items: usize) -> Vec<ProcessWindow> {
    let mut windows: Vec<_> = tests
        .iter()
        .filter_map(|test| {
            let mean = test.mean?;
            let sigma = test.sigma?;
            let min = test.min?;
            let max = test.max?;
            let cp = match (test.lo_limit, test.hi_limit) {
                (Some(lo), Some(hi)) if sigma > 0.0 => {
                    Some(((hi - mean).min(mean - lo)) / (3.0 * sigma))
                }
                _ => None,
            };
            Some(ProcessWindow {
                label: test.label.clone(),
                mean,
                sigma,
                min,
                max,
                lo_limit: test.lo_limit,
                hi_limit: test.hi_limit,
                limit_fail: test.limit_fail,
                cp,
            })
        })
        .collect();
    windows.sort_by(|left, right| {
        right
            .limit_fail
            .cmp(&left.limit_fail)
            .then_with(|| {
                left.cp
                    .unwrap_or(f64::INFINITY)
                    .total_cmp(&right.cp.unwrap_or(f64::INFINITY))
            })
            .then(left.label.cmp(&right.label))
    });
    windows.truncate(max_items);
    windows
}

fn correlations(
    part_results: &BTreeMap<String, BTreeMap<u32, f64>>,
    tests: &[TestAggregate],
    max_tests: usize,
) -> Vec<CorrelationPair> {
    let selected: Vec<_> = tests
        .iter()
        .filter(|test| test.mean.is_some() && test.sigma.unwrap_or(0.0) > 0.0)
        .take(max_tests.max(2))
        .map(|test| (test.test_num, test.label.clone()))
        .collect();
    let labels: HashMap<_, _> = selected.iter().cloned().collect();
    let mut pairs = Vec::new();

    for left_index in 0..selected.len() {
        for right_index in (left_index + 1)..selected.len() {
            let left = selected[left_index].0;
            let right = selected[right_index].0;
            if let Some((correlation, count)) = pearson(part_results, left, right) {
                pairs.push(CorrelationPair {
                    x: labels
                        .get(&left)
                        .cloned()
                        .unwrap_or_else(|| left.to_string()),
                    y: labels
                        .get(&right)
                        .cloned()
                        .unwrap_or_else(|| right.to_string()),
                    correlation,
                    pairs: count,
                });
            }
        }
    }

    pairs.sort_by(|left, right| {
        right
            .correlation
            .abs()
            .total_cmp(&left.correlation.abs())
            .then(right.pairs.cmp(&left.pairs))
    });
    pairs.truncate(60);
    pairs
}

fn pearson(
    part_results: &BTreeMap<String, BTreeMap<u32, f64>>,
    left: u32,
    right: u32,
) -> Option<(f64, usize)> {
    let mut n = 0usize;
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_x2 = 0.0;
    let mut sum_y2 = 0.0;
    let mut sum_xy = 0.0;

    for values in part_results.values() {
        let (Some(x), Some(y)) = (values.get(&left), values.get(&right)) else {
            continue;
        };
        n += 1;
        sum_x += x;
        sum_y += y;
        sum_x2 += x * x;
        sum_y2 += y * y;
        sum_xy += x * y;
    }

    if n < 3 {
        return None;
    }
    let n_f = n as f64;
    let numerator = n_f * sum_xy - sum_x * sum_y;
    let denominator = ((n_f * sum_x2 - sum_x * sum_x) * (n_f * sum_y2 - sum_y * sum_y)).sqrt();
    (denominator > 0.0).then_some((numerator / denominator, n))
}

fn finish_commonality(
    groups: BTreeMap<(String, String), usize>,
    max_items: usize,
) -> Vec<CommonalityAggregate> {
    let mut values: Vec<_> = groups
        .into_iter()
        .map(|((dimension, label), count)| CommonalityAggregate {
            dimension,
            label,
            count,
        })
        .collect();
    values.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then(left.dimension.cmp(&right.dimension))
            .then(left.label.cmp(&right.label))
    });
    values.truncate(max_items);
    values
}

fn quality_seed() -> BTreeMap<String, (usize, usize)> {
    [
        "missing wafer_id",
        "missing part_id",
        "missing x/y coordinate",
        "missing test_pass",
        "missing result",
        "missing limits",
        "missing test_time_ms",
        "unresolved coordinate identity",
    ]
    .into_iter()
    .map(|label| (label.to_string(), (0, 0)))
    .collect()
}

fn push_quality(groups: &mut BTreeMap<String, (usize, usize)>, row: &TestRow) {
    for value in groups.values_mut() {
        value.0 += 1;
    }
    if row.part_merge_key.is_none() {
        groups.get_mut("unresolved coordinate identity").unwrap().1 += 1;
    }
    if row.wafer_id.is_none() {
        groups.get_mut("missing wafer_id").unwrap().1 += 1;
    }
    if row.part_id.is_empty() {
        groups.get_mut("missing part_id").unwrap().1 += 1;
    }
    if row.x_coord.is_none() || row.y_coord.is_none() {
        groups.get_mut("missing x/y coordinate").unwrap().1 += 1;
    }
    if row.test_pass.is_none() {
        groups.get_mut("missing test_pass").unwrap().1 += 1;
    }
    if row.result.is_none() {
        groups.get_mut("missing result").unwrap().1 += 1;
    }
    if row.lo_limit.is_none() || row.hi_limit.is_none() {
        groups.get_mut("missing limits").unwrap().1 += 1;
    }
    if row.test_time_ms.is_none() {
        groups.get_mut("missing test_time_ms").unwrap().1 += 1;
    }
}

fn finish_quality(groups: BTreeMap<String, (usize, usize)>) -> Vec<GroupAggregate> {
    groups
        .into_iter()
        .map(|(label, (total, fail))| GroupAggregate {
            label,
            total,
            pass: total.saturating_sub(fail),
            fail,
            yield_percent: percent(total.saturating_sub(fail), total),
        })
        .collect()
}

fn push_commonality(
    groups: &mut BTreeMap<(String, String), usize>,
    dimension: impl Into<String>,
    label: impl Into<String>,
) {
    *groups.entry((dimension.into(), label.into())).or_default() += 1;
}

fn unique_site_count(parts: &BTreeMap<String, PartInfo>) -> usize {
    parts
        .values()
        .map(|part| (part.head_num, part.site_num))
        .collect::<BTreeSet<_>>()
        .len()
}

fn scoped_part_key(row: &TestRow, source: &str) -> String {
    // Resolved wafer/PRR or lot/PTR coordinates merge across files and sites.
    // An unresolved attempt is isolated by source and its PIR/PRR sequence,
    // never by a possibly empty/reused PART_ID.
    row.part_merge_key
        .clone()
        .unwrap_or_else(|| serde_json::json!(["unresolved", source, row.part_sequence]).to_string())
}

fn test_label(row: &TestRow) -> String {
    match row.test_txt.as_deref().filter(|value| !value.is_empty()) {
        Some(text) => format!("{} - {}", row.test_num, text),
        None => format!("TEST {}", row.test_num),
    }
}

fn wafer_label(row: &TestRow) -> String {
    row.wafer_id
        .clone()
        .unwrap_or_else(|| "(no wafer)".to_string())
}

fn is_failure(row: &TestRow) -> bool {
    row.test_pass == Some(false)
        || row
            .result
            .zip(row.lo_limit)
            .is_some_and(|(value, limit)| value < limit)
        || row
            .result
            .zip(row.hi_limit)
            .is_some_and(|(value, limit)| value > limit)
}

fn percent(pass: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        pass as f64 * 100.0 / total as f64
    }
}

fn unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn string_col<'a>(
    batch: &'a RecordBatch,
    index: usize,
    name: &str,
) -> Result<&'a StringArray, Box<dyn Error>> {
    batch
        .column(index)
        .as_any()
        .downcast_ref::<StringArray>()
        .ok_or_else(|| invalid_data(format!("{name} column is not Utf8")).into())
}

fn u8_col<'a>(
    batch: &'a RecordBatch,
    index: usize,
    name: &str,
) -> Result<&'a UInt8Array, Box<dyn Error>> {
    batch
        .column(index)
        .as_any()
        .downcast_ref::<UInt8Array>()
        .ok_or_else(|| invalid_data(format!("{name} column is not UInt8")).into())
}

fn u16_col<'a>(
    batch: &'a RecordBatch,
    index: usize,
    name: &str,
) -> Result<&'a UInt16Array, Box<dyn Error>> {
    batch
        .column(index)
        .as_any()
        .downcast_ref::<UInt16Array>()
        .ok_or_else(|| invalid_data(format!("{name} column is not UInt16")).into())
}

fn u32_col<'a>(
    batch: &'a RecordBatch,
    index: usize,
    name: &str,
) -> Result<&'a UInt32Array, Box<dyn Error>> {
    batch
        .column(index)
        .as_any()
        .downcast_ref::<UInt32Array>()
        .ok_or_else(|| invalid_data(format!("{name} column is not UInt32")).into())
}

fn i16_col<'a>(
    batch: &'a RecordBatch,
    index: usize,
    name: &str,
) -> Result<&'a Int16Array, Box<dyn Error>> {
    batch
        .column(index)
        .as_any()
        .downcast_ref::<Int16Array>()
        .ok_or_else(|| invalid_data(format!("{name} column is not Int16")).into())
}

fn f32_col<'a>(
    batch: &'a RecordBatch,
    index: usize,
    name: &str,
) -> Result<&'a Float32Array, Box<dyn Error>> {
    batch
        .column(index)
        .as_any()
        .downcast_ref::<Float32Array>()
        .ok_or_else(|| invalid_data(format!("{name} column is not Float32")).into())
}

fn bool_col<'a>(
    batch: &'a RecordBatch,
    index: usize,
    name: &str,
) -> Result<&'a BooleanArray, Box<dyn Error>> {
    batch
        .column(index)
        .as_any()
        .downcast_ref::<BooleanArray>()
        .ok_or_else(|| invalid_data(format!("{name} column is not Boolean")).into())
}

fn required_string(array: &StringArray, row: usize, name: &str) -> Result<String, Box<dyn Error>> {
    optional_string(array, row)
        .ok_or_else(|| invalid_data(format!("{name} is null at row {row}")).into())
}

fn required_u8(array: &UInt8Array, row: usize, name: &str) -> Result<u8, Box<dyn Error>> {
    (!array.is_null(row))
        .then(|| array.value(row))
        .ok_or_else(|| invalid_data(format!("{name} is null at row {row}")).into())
}

fn required_u16(array: &UInt16Array, row: usize, name: &str) -> Result<u16, Box<dyn Error>> {
    (!array.is_null(row))
        .then(|| array.value(row))
        .ok_or_else(|| invalid_data(format!("{name} is null at row {row}")).into())
}

fn required_u32(array: &UInt32Array, row: usize, name: &str) -> Result<u32, Box<dyn Error>> {
    (!array.is_null(row))
        .then(|| array.value(row))
        .ok_or_else(|| invalid_data(format!("{name} is null at row {row}")).into())
}

fn required_bool(array: &BooleanArray, row: usize, name: &str) -> Result<bool, Box<dyn Error>> {
    (!array.is_null(row))
        .then(|| array.value(row))
        .ok_or_else(|| invalid_data(format!("{name} is null at row {row}")).into())
}

fn optional_string(array: &StringArray, row: usize) -> Option<String> {
    (!array.is_null(row)).then(|| array.value(row).to_string())
}

fn optional_i16(array: &Int16Array, row: usize) -> Option<i16> {
    (!array.is_null(row)).then(|| array.value(row))
}

fn optional_f32(array: &Float32Array, row: usize) -> Option<f32> {
    (!array.is_null(row)).then(|| array.value(row))
}

fn optional_u32(array: &UInt32Array, row: usize) -> Option<u32> {
    (!array.is_null(row)).then(|| array.value(row))
}

fn optional_bool(array: &BooleanArray, row: usize) -> Option<bool> {
    (!array.is_null(row)).then(|| array.value(row))
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn render_html(data: &DashboardData) -> String {
    DASHBOARD_HTML
        .replace("__TITLE__", &html_escape(&data.title))
        .replace("__DATA__", &dashboard_json(data))
}

fn dashboard_json(data: &DashboardData) -> String {
    format!(
        concat!(
            "{{",
            "\"title\":{},",
            "\"generated_at_unix\":{},",
            "\"identity_rule_version\":{},",
            "\"kpis\":{},",
            "\"pareto\":{},",
            "\"wafer_yield\":{},",
            "\"site_yield\":{},",
            "\"hard_bins\":{},",
            "\"soft_bins\":{},",
            "\"commonality\":{},",
            "\"correlations\":{},",
            "\"heatmap_points\":{},",
            "\"process_windows\":{},",
            "\"data_quality\":{}",
            "}}"
        ),
        json_string(&data.title),
        data.generated_at_unix,
        json_string(stdf_arrow::identity::IDENTITY_VERSION),
        kpis_json(&data.kpis),
        test_aggregates_json(&data.pareto),
        group_aggregates_json(&data.wafer_yield),
        group_aggregates_json(&data.site_yield),
        group_aggregates_json(&data.hard_bins),
        group_aggregates_json(&data.soft_bins),
        commonality_json(&data.commonality),
        correlations_json(&data.correlations),
        heatmap_json(&data.heatmap_points),
        process_windows_json(&data.process_windows),
        group_aggregates_json(&data.data_quality)
    )
}

fn kpis_json(kpis: &Kpis) -> String {
    format!(
        concat!(
            "{{",
            "\"rows\":{},\"parts\":{},\"passed_parts\":{},\"failed_parts\":{},",
            "\"yield_percent\":{},\"tests\":{},\"lots\":{},\"wafers\":{},",
            "\"sites\":{},\"failing_tests\":{}",
            "}}"
        ),
        kpis.rows,
        kpis.parts,
        kpis.passed_parts,
        kpis.failed_parts,
        json_f64(kpis.yield_percent),
        kpis.tests,
        kpis.lots,
        kpis.wafers,
        kpis.sites,
        kpis.failing_tests
    )
}

fn test_aggregates_json(items: &[TestAggregate]) -> String {
    let values: Vec<_> = items
        .iter()
        .map(|item| {
            format!(
                concat!(
                    "{{",
                    "\"label\":{},\"test_num\":{},\"test_type\":{},\"units\":{},",
                    "\"total\":{},\"pass\":{},\"fail\":{},\"unknown\":{},",
                    "\"limit_fail\":{},\"mean\":{},\"sigma\":{},\"min\":{},\"max\":{},",
                    "\"lo_limit\":{},\"hi_limit\":{},",
                    "\"yield_percent\":{}",
                    "}}"
                ),
                json_string(&item.label),
                item.test_num,
                json_string(&item.test_type),
                json_option_string(item.units.as_deref()),
                item.total,
                item.pass,
                item.fail,
                item.unknown,
                item.limit_fail,
                json_option_f64(item.mean),
                json_option_f64(item.sigma),
                json_option_f64(item.min),
                json_option_f64(item.max),
                json_option_f64(item.lo_limit),
                json_option_f64(item.hi_limit),
                json_f64(item.yield_percent)
            )
        })
        .collect();
    format!("[{}]", values.join(","))
}

fn group_aggregates_json(items: &[GroupAggregate]) -> String {
    let values: Vec<_> = items
        .iter()
        .map(|item| {
            format!(
                "{{\"label\":{},\"total\":{},\"pass\":{},\"fail\":{},\"yield_percent\":{}}}",
                json_string(&item.label),
                item.total,
                item.pass,
                item.fail,
                json_f64(item.yield_percent)
            )
        })
        .collect();
    format!("[{}]", values.join(","))
}

fn commonality_json(items: &[CommonalityAggregate]) -> String {
    let values: Vec<_> = items
        .iter()
        .map(|item| {
            format!(
                "{{\"dimension\":{},\"label\":{},\"count\":{}}}",
                json_string(&item.dimension),
                json_string(&item.label),
                item.count
            )
        })
        .collect();
    format!("[{}]", values.join(","))
}

fn correlations_json(items: &[CorrelationPair]) -> String {
    let values: Vec<_> = items
        .iter()
        .map(|item| {
            format!(
                "{{\"x\":{},\"y\":{},\"correlation\":{},\"pairs\":{}}}",
                json_string(&item.x),
                json_string(&item.y),
                json_f64(item.correlation),
                item.pairs
            )
        })
        .collect();
    format!("[{}]", values.join(","))
}

fn heatmap_json(items: &[HeatmapPoint]) -> String {
    let values: Vec<_> = items
        .iter()
        .map(|item| {
            format!(
                "{{\"x\":{},\"y\":{},\"total\":{},\"fail\":{},\"yield_percent\":{}}}",
                item.x,
                item.y,
                item.total,
                item.fail,
                json_f64(item.yield_percent)
            )
        })
        .collect();
    format!("[{}]", values.join(","))
}

fn process_windows_json(items: &[ProcessWindow]) -> String {
    let values: Vec<_> = items
        .iter()
        .map(|item| {
            format!(
                concat!(
                    "{{",
                    "\"label\":{},\"mean\":{},\"sigma\":{},\"min\":{},\"max\":{},",
                    "\"lo_limit\":{},\"hi_limit\":{},\"limit_fail\":{},\"cp\":{}",
                    "}}"
                ),
                json_string(&item.label),
                json_f64(item.mean),
                json_f64(item.sigma),
                json_f64(item.min),
                json_f64(item.max),
                json_option_f64(item.lo_limit),
                json_option_f64(item.hi_limit),
                item.limit_fail,
                json_option_f64(item.cp)
            )
        })
        .collect();
    format!("[{}]", values.join(","))
}

fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

fn json_option_string(value: Option<&str>) -> String {
    value.map(json_string).unwrap_or_else(|| "null".to_string())
}

fn json_option_f64(value: Option<f64>) -> String {
    value.map(json_f64).unwrap_or_else(|| "null".to_string())
}

fn json_f64(value: f64) -> String {
    if value.is_finite() {
        format!("{value:.6}")
    } else {
        "null".to_string()
    }
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const DASHBOARD_HTML: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>__TITLE__</title>
<style>
:root {
  --bg: #07100f;
  --panel: rgba(13, 28, 26, 0.86);
  --panel-2: rgba(19, 44, 42, 0.9);
  --ink: #effff8;
  --muted: #8fb5ac;
  --line: rgba(156, 255, 223, 0.14);
  --cyan: #57f2d1;
  --amber: #ffbf5f;
  --red: #ff6b5f;
  --green: #7af28b;
  --blue: #6bb7ff;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  min-height: 100vh;
  color: var(--ink);
  background:
    radial-gradient(circle at 12% 8%, rgba(87, 242, 209, 0.16), transparent 30rem),
    radial-gradient(circle at 84% 12%, rgba(255, 191, 95, 0.15), transparent 28rem),
    linear-gradient(145deg, #06100f 0%, #0b151a 48%, #130f0a 100%);
  font-family: "Aptos Display", "Segoe UI", sans-serif;
}
body::before {
  content: "";
  position: fixed;
  inset: 0;
  pointer-events: none;
  background-image: linear-gradient(rgba(255,255,255,0.035) 1px, transparent 1px),
    linear-gradient(90deg, rgba(255,255,255,0.03) 1px, transparent 1px);
  background-size: 32px 32px;
  mask-image: linear-gradient(to bottom, black, transparent 82%);
}
.shell { width: min(1480px, calc(100vw - 32px)); margin: 0 auto; padding: 28px 0 46px; }
header { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 18px; align-items: end; margin-bottom: 18px; }
.eyebrow { color: var(--cyan); text-transform: uppercase; letter-spacing: .18em; font-size: 12px; font-weight: 800; }
h1 { margin: 8px 0 0; font-size: clamp(34px, 5vw, 78px); line-height: .9; letter-spacing: -.06em; }
.subtitle { color: var(--muted); max-width: 760px; margin: 12px 0 0; }
.controls { display: flex; gap: 10px; flex-wrap: wrap; justify-content: flex-end; }
input, select {
  color: var(--ink);
  background: rgba(255,255,255,0.06);
  border: 1px solid var(--line);
  border-radius: 14px;
  padding: 11px 12px;
  outline: none;
}
input:focus, select:focus { border-color: var(--cyan); box-shadow: 0 0 0 3px rgba(87,242,209,.12); }
.tabs { display: flex; gap: 8px; flex-wrap: wrap; margin: 16px 0 18px; }
.tab {
  border: 1px solid var(--line);
  color: var(--muted);
  background: rgba(255,255,255,0.04);
  border-radius: 999px;
  padding: 10px 14px;
  cursor: pointer;
}
.tab.active { color: #06100f; background: var(--cyan); border-color: var(--cyan); font-weight: 800; }
.grid { display: grid; gap: 14px; }
.kpis { grid-template-columns: repeat(6, minmax(0, 1fr)); }
.two { grid-template-columns: 1.18fr .82fr; }
.three { grid-template-columns: repeat(3, minmax(0, 1fr)); }
.panel, .kpi {
  background: linear-gradient(180deg, var(--panel), rgba(8, 18, 17, .92));
  border: 1px solid var(--line);
  border-radius: 24px;
  box-shadow: 0 18px 70px rgba(0,0,0,.32);
}
.panel { padding: 18px; min-height: 240px; min-width: 0; overflow-x: auto; }
.kpi { padding: 16px; position: relative; overflow: hidden; }
.kpi::after {
  content: "";
  position: absolute;
  right: -20px;
  bottom: -26px;
  width: 92px;
  height: 92px;
  border-radius: 999px;
  background: rgba(87, 242, 209, .08);
}
.label { color: var(--muted); font-size: 12px; text-transform: uppercase; letter-spacing: .12em; font-weight: 800; }
.value { font-size: clamp(26px, 3vw, 42px); font-weight: 900; letter-spacing: -.04em; margin-top: 8px; }
.sub { color: var(--muted); margin-top: 5px; font-size: 13px; }
.panel h2 { margin: 0 0 12px; font-size: 20px; letter-spacing: -.02em; }
canvas { width: 100%; height: 300px; display: block; }
table { width: 100%; border-collapse: collapse; font-size: 13px; }
th, td { padding: 9px 8px; border-bottom: 1px solid var(--line); text-align: left; }
th { color: var(--muted); font-size: 11px; text-transform: uppercase; letter-spacing: .08em; }
tr:hover td { background: rgba(87, 242, 209, .05); }
.section { display: none; animation: rise .35s ease both; }
.section.active { display: block; }
.pill { display: inline-flex; border-radius: 999px; padding: 4px 8px; font-weight: 800; font-size: 12px; }
.pass { background: rgba(122, 242, 139, .12); color: var(--green); }
.fail { background: rgba(255, 107, 95, .12); color: var(--red); }
.warn { background: rgba(255, 191, 95, .12); color: var(--amber); }
.empty { color: var(--muted); padding: 38px 0; text-align: center; border: 1px dashed var(--line); border-radius: 18px; }
.detail { color: var(--muted); line-height: 1.55; }
@keyframes rise { from { opacity: 0; transform: translateY(10px); } to { opacity: 1; transform: translateY(0); } }
@media (max-width: 980px) {
  header, .two, .three { grid-template-columns: 1fr; }
  .kpis { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .controls { justify-content: flex-start; }
}
@media (max-width: 560px) { .kpis { grid-template-columns: 1fr; } .shell { width: min(100% - 20px, 1480px); } }
</style>
</head>
<body>
<script id="dashboard-data" type="application/json">__DATA__</script>
<main class="shell">
  <header>
    <div>
      <div class="eyebrow">STDF spectrum dataview</div>
      <h1 id="title"></h1>
      <p class="muted" data-i18n="Parts merge by coordinate identity; unresolved attempts remain separate. Device yield uses all-pass merging, not final-retest yield.">Parts merge by coordinate identity; unresolved attempts remain separate. Device yield uses all-pass merging, not final-retest yield.</p>
    </div>
    <div class="controls">
      <input id="search" type="search" placeholder="Filter tests or groups" data-i18n-placeholder="Filter tests or groups">
      <select id="lot-select" aria-label="Lot selection" hidden data-i18n-label="Lot selection"></select>
      <select id="commonality-dimension" aria-label="Commonality dimension" data-i18n-label="Commonality dimension"></select>
      <input id="min-fails" type="number" min="0" value="0" aria-label="Minimum failures" data-i18n-label="Minimum failures">
    </div>
  </header>
  <p id="dataset-status" role="status" hidden></p>

  <nav class="tabs" aria-label="Dashboard sections" data-i18n-label="Dashboard sections">
    <button class="tab active" data-tab="overview" data-i18n="Overview">Overview</button>
    <button class="tab" data-tab="pareto" data-i18n="Pareto">Pareto</button>
    <button class="tab" data-tab="commonality" data-i18n="Commonality">Commonality</button>
    <button class="tab" data-tab="correlation" data-i18n="Correlation">Correlation</button>
    <button class="tab" data-tab="spatial" data-i18n="Spatial">Spatial</button>
    <button class="tab" data-tab="quality" data-i18n="Quality">Quality</button>
  </nav>

  <section id="overview" class="section active">
    <div id="kpis" class="grid kpis"></div>
    <div class="grid two" style="margin-top:14px">
      <div class="panel"><h2 data-i18n="Yield By Site">Yield By Site</h2><canvas id="site-yield-chart"></canvas></div>
      <div class="panel"><h2 data-i18n="Hard Bin Distribution">Hard Bin Distribution</h2><canvas id="hard-bin-chart"></canvas></div>
    </div>
  </section>

  <section id="pareto" class="section">
    <div class="grid two">
      <div class="panel"><h2 data-i18n="Failure Pareto">Failure Pareto</h2><canvas id="pareto-chart"></canvas></div>
      <div class="panel"><h2 data-i18n="Test Detail">Test Detail</h2><div id="test-detail" class="detail"><span data-i18n="Click a Pareto bar or row to inspect limits and distribution.">Click a Pareto bar or row to inspect limits and distribution.</span></div></div>
    </div>
    <div class="panel" style="margin-top:14px"><h2 data-i18n="Tests">Tests</h2><div id="pareto-table"></div></div>
    <!-- FTR_PATTERN_ANALYSIS -->
  </section>

  <section id="commonality" class="section" data-feature="failure-commonality">
    <div class="grid two">
      <div class="panel"><h2 data-i18n="Failure Commonality">Failure Commonality</h2><canvas id="commonality-chart"></canvas></div>
      <div class="panel"><h2 data-i18n="Interpretation">Interpretation</h2><div id="commonality-note" class="detail"></div></div>
    </div>
  </section>

  <section id="correlation" class="section">
    <div class="grid two">
      <div class="panel"><h2 data-i18n="Correlation Heatmap">Correlation Heatmap</h2><canvas id="correlation-heatmap"></canvas></div>
      <div class="panel"><h2 data-i18n="Strongest Relationships">Strongest Relationships</h2><div id="correlation-table"></div></div>
    </div>
  </section>

  <section id="spatial" class="section">
    <div class="grid two">
      <div class="panel"><h2 data-i18n="Wafer / XY Failure Map">Wafer / XY Failure Map</h2><canvas id="xy-map"></canvas></div>
      <div class="panel"><h2 data-i18n="Wafer Yield">Wafer Yield</h2><canvas id="wafer-yield-chart"></canvas></div>
    </div>
  </section>

  <section id="quality" class="section">
    <div class="grid two">
      <div class="panel"><h2 data-i18n="Data Completeness">Data Completeness</h2><canvas id="quality-chart"></canvas></div>
      <div class="panel"><h2 data-i18n="Process Windows">Process Windows</h2><div id="process-table"></div></div>
    </div>
  </section>
</main>
<script>
const T=window.ReportUI.t, C=name=>window.ReportUI.color(name);
let currentDetail=null;
const snapshot = JSON.parse(document.getElementById('dashboard-data').textContent);
let data = snapshot.all || snapshot;
const state = { tab: 'overview', query: '', minFails: 0 };
document.getElementById('title').textContent = data.title;

for (const button of document.querySelectorAll('.tab')) {
  button.addEventListener('click', () => {
    state.tab = button.dataset.tab;
    document.querySelectorAll('.tab').forEach(item => item.classList.toggle('active', item === button));
    document.querySelectorAll('.section').forEach(item => item.classList.toggle('active', item.id === state.tab));
    render();
  });
}
document.getElementById('search').addEventListener('input', event => {
  state.query = event.target.value.toLowerCase();
  render();
});
document.getElementById('min-fails').addEventListener('input', event => {
  state.minFails = Number(event.target.value || 0);
  render();
});

const dimensionSelect = document.getElementById('commonality-dimension');
const dimensions = [...new Set(data.commonality.map(item => item.dimension))];
dimensionSelect.innerHTML = '<option value="">all dimensions</option>' + dimensions.map(value => `<option value="${esc(value)}">${esc(value)}</option>`).join('');
dimensionSelect.addEventListener('change', render);

const lotSelect = document.getElementById('lot-select');
if (snapshot.lots) {
  lotSelect.hidden = false;
  [{label: 'All lots'}, ...snapshot.lots].forEach((lot, index) => {
    const option = document.createElement('option');
    option.value = String(index); option.textContent = lot.label || '(empty lot)';
    lotSelect.appendChild(option);
  });
  const status = document.getElementById('dataset-status');
  status.hidden = false;
  updateDatasetStatus();
  lotSelect.addEventListener('change', () => {
    data = Number(lotSelect.value) === 0 ? snapshot.all : snapshot.lots[Number(lotSelect.value) - 1].data;
    dimensionSelect.innerHTML = '<option value="">all dimensions</option>' +
      [...new Set(data.commonality.map(item => item.dimension))].map(value => `<option value="${esc(value)}">${esc(value)}</option>`).join('');
    document.getElementById('test-detail').innerHTML = '';currentDetail=null;
    render();
  });
}

function updateDatasetStatus(){if(!snapshot.lots)return;document.getElementById('dataset-status').textContent=T('Catalog revision {revision}; latest run: {status}.',{revision:snapshot.revision,status:snapshot.run_status})+' '+T(snapshot.run_status==='complete'?'Showing current successful source versions.':'Incomplete run: failed updates may retain older source versions.');}
function render() {
  renderKpis();
  renderBar('site-yield-chart', data.site_yield.slice(0, 16), item => item.label, item => item.yield_percent, item => item.yield_percent < 90 ? C('bad-ink') : C('good-ink'), '%');
  renderBar('hard-bin-chart', data.hard_bins.slice(0, 16), item => item.label, item => item.total, item => item.fail ? C('warn-ink') : C('good-ink'), ' '+T('parts'));
  renderPareto();
  renderCommonality();
  renderCorrelation();
  renderSpatial();
  renderQuality();
}

function renderKpis() {
  const k = data.kpis;
  document.getElementById('kpis').innerHTML = [
    card('Yield', fmtPct(k.yield_percent), T('{pass}/{total} parts passed',{pass:k.passed_parts,total:k.parts})),
    card('Fail Parts', k.failed_parts.toLocaleString(), T('{n} failing tests',{n:k.failing_tests})),
    card('Rows', k.rows.toLocaleString(), T('EAV test result rows')),
    card('Tests', k.tests.toLocaleString(), T('unique test numbers')),
    card('Wafers', k.wafers.toLocaleString(), T('{n} lots',{n:k.lots})),
    card('Sites', k.sites.toLocaleString(), T('head / site combinations')),
  ].join('');
}

function renderPareto() {
  const items = filtered(data.pareto).filter(item => item.fail >= state.minFails);
  renderBar('pareto-chart', items.slice(0, 18), item => item.label, item => item.fail, () => C('bad-ink'), ' '+T('fails'), showTestDetail);
  table('pareto-table', items, [
    ['Test', item => `<button class="tablerow" data-test="${esc(item.label)}">${esc(item.label)}</button>`],
    ['Type', item => esc(item.test_type)],
    ['Fail', item => pill(item.fail, item.fail ? 'fail' : 'pass')],
    ['Yield', item => fmtPct(item.yield_percent)],
    ['Mean', item => fmt(item.mean)],
    ['Sigma', item => fmt(item.sigma)],
    ['Limits', item => `${fmt(item.min)} / ${fmt(item.max)}`],
  ]);
  document.querySelectorAll('[data-test]').forEach(button => {
    button.addEventListener('click', () => showTestDetail(data.pareto.find(item => item.label === button.dataset.test)));
  });
}

function renderCommonality() {
  const selected = dimensionSelect.value;
  const items = filtered(data.commonality)
    .filter(item => !selected || item.dimension === selected)
    .filter(item => item.count >= state.minFails)
    .slice(0, 24);
  renderBar('commonality-chart', items, item => `${item.dimension}: ${item.label}`, item => item.count, () => C('warn-ink'), ' '+T('hits'));
  const top = items[0];
  document.getElementById('commonality-note').innerHTML = top
    ? esc(T('{dimension}: {label} · {n} failing observations.',{dimension:top.dimension,label:top.label,n:top.count.toLocaleString()}))
    : `<div class="empty">${esc(T('No failure commonality after current filters.'))}</div>`;
}

function renderCorrelation() {
  const items = filtered(data.correlations);
  renderCorrelationHeatmap(items.slice(0, 60));
  table('correlation-table', items.slice(0, 30), [
    ['Test A', item => esc(item.x)],
    ['Test B', item => esc(item.y)],
    ['r', item => `<span class="pill ${Math.abs(item.correlation) > .75 ? 'warn' : 'pass'}">${item.correlation.toFixed(3)}</span>`],
    ['Pairs', item => item.pairs.toLocaleString()],
  ]);
}

function renderSpatial() {
  renderXyMap(data.heatmap_points);
  renderBar('wafer-yield-chart', data.wafer_yield.slice(0, 18), item => item.label, item => item.yield_percent, item => item.yield_percent < 90 ? C('bad-ink') : C('good-ink'), '%');
}

function renderQuality() {
  renderBar('quality-chart', data.data_quality, item => T(item.label), item => item.fail, item => item.fail ? C('warn-ink') : C('good-ink'), ' '+T('missing'));
  table('process-table', data.process_windows.slice(0, 24), [
    ['Test', item => esc(item.label)],
    ['Mean', item => fmt(item.mean)],
    ['Sigma', item => fmt(item.sigma)],
    ['Range', item => `${fmt(item.min)} to ${fmt(item.max)}`],
    ['CP', item => item.cp == null ? '-' : item.cp.toFixed(2)],
    ['Limit Fails', item => pill(item.limit_fail, item.limit_fail ? 'fail' : 'pass')],
  ]);
}

function renderBar(id, items, label, value, color, suffix, onPick) {
  const canvas = setupCanvas(id);
  const ctx = canvas.getContext('2d');
  canvas.onclick = null;
  if (!items.length) return emptyCanvas(ctx, canvas, 'No data for current filters');
  // Layout in CSS pixels so labels and picking stay consistent at every DPR.
  const rect = canvas.getBoundingClientRect();
  const width = rect.width || 320, height = rect.height || 300;
  ctx.setTransform(canvas.width / width, 0, 0, canvas.height / height, 0, 0);
  ctx.clearRect(0, 0, width, height);
  const max = Math.max(...items.map(value), 1);
  ctx.font = '12px Segoe UI';
  const values = items.map(item => `${Number(value(item)).toLocaleString(undefined, {maximumFractionDigits: 1})}${suffix}`);
  const right = Math.max(...values.map(text => ctx.measureText(text).width)) + 24;
  const left = Math.min(130, width * .32), top = 18;
  const stride = Math.min(40, (height - top * 2) / items.length);
  const barH = Math.max(1, stride - 7), plotWidth = Math.max(1, width - left - right);
  ctx.textBaseline = 'middle';
  canvas.setAttribute('role', 'img');
  canvas.setAttribute('aria-label', items.map((item, i) => `${label(item)}: ${values[i]}`).join('; '));
  items.forEach((item, index) => {
    const y = top + index * stride;
    const v = value(item);
    const w = plotWidth * v / max;
    let text = String(label(item));
    if (ctx.measureText(text).width > left - 20) {
      while (text.length && ctx.measureText(text + '…').width > left - 20) text = text.slice(0, -1);
      text += '…';
    }
    ctx.textAlign = 'left';
    ctx.fillStyle = C('muted');
    ctx.fillText(text, 10, y + barH / 2);
    ctx.fillStyle = color(item);
    roundRect(ctx, left, y, Math.max(w, 2), barH, Math.min(6, barH / 2, Math.max(w, 2) / 2));
    ctx.fill();
    ctx.fillStyle = C('ink');
    ctx.textAlign = 'right';
    ctx.fillText(values[index], width - 10, y + barH / 2);
  });
  canvas.onclick = event => {
    if (!onPick) return;
    const rect = canvas.getBoundingClientRect();
    const y = (event.clientY - rect.top) * height / rect.height - top;
    const index = Math.floor(y / stride);
    if (items[index] && y - index * stride <= barH) onPick(items[index]);
  };
}

function renderCorrelationHeatmap(items) {
  const canvas = setupCanvas('correlation-heatmap');
  const ctx = canvas.getContext('2d');
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  if (!items.length) return emptyCanvas(ctx, canvas, 'Need at least 3 matched parts across 2 variable tests');
  const labels = [...new Set(items.flatMap(item => [item.x, item.y]))].slice(0, 16);
  const n = labels.length;
  const cell = Math.min((canvas.width - 160) / n, (canvas.height - 70) / n);
  const ox = 130, oy = 24;
  ctx.font = '10px Segoe UI';
  labels.forEach((a, i) => {
    labels.forEach((b, j) => {
      const hit = items.find(item => (item.x === a && item.y === b) || (item.x === b && item.y === a));
      const v = a === b ? 1 : (hit ? hit.correlation : 0);
      ctx.fillStyle = corrColor(v);
      ctx.fillRect(ox + i * cell, oy + j * cell, cell - 1, cell - 1);
    });
    ctx.fillStyle = C('muted');
    ctx.fillText(trim(a, 12), 4, oy + i * cell + cell * .7);
  });
}

function renderXyMap(points) {
  const canvas = setupCanvas('xy-map');
  const ctx = canvas.getContext('2d');
  ctx.clearRect(0, 0, canvas.width, canvas.height);
  if (!points.length) return emptyCanvas(ctx, canvas, 'No XY coordinates in Parquet rows');
  const xs = points.map(p => p.x), ys = points.map(p => p.y);
  const minX = Math.min(...xs), maxX = Math.max(...xs), minY = Math.min(...ys), maxY = Math.max(...ys);
  const pad = 34;
  for (const point of points) {
    const x = pad + (point.x - minX) / Math.max(1, maxX - minX) * (canvas.width - 2 * pad);
    const y = canvas.height - pad - (point.y - minY) / Math.max(1, maxY - minY) * (canvas.height - 2 * pad);
    ctx.fillStyle = point.fail ? C('bad-ink') : C('good-ink');
    ctx.globalAlpha = .45 + .55 * Math.min(1, point.fail / Math.max(1, point.total));
    ctx.beginPath();
    ctx.arc(x, y, 5 + Math.sqrt(point.total), 0, Math.PI * 2);
    ctx.fill();
  }
  ctx.globalAlpha = 1;
  ctx.fillStyle = C('muted');
  ctx.fillText(`X ${minX}..${maxX} / Y ${minY}..${maxY}`, 14, 22);
}

function setupCanvas(id) {
  const canvas = document.getElementById(id);
  const rect = canvas.getBoundingClientRect();
  const scale = window.devicePixelRatio || 1;
  canvas.width = Math.max(320, Math.floor(rect.width * scale));
  canvas.height = Math.max(240, Math.floor(rect.height * scale));
  return canvas;
}

function table(id, items, columns) {
  const element = document.getElementById(id);
  if (!items.length) {
    element.innerHTML = `<div class="empty">${esc(T('No data for current filters.'))}</div>`;
    return;
  }
  element.innerHTML = `<table><thead><tr>${columns.map(([h]) => `<th>${esc(T(h))}</th>`).join('')}</tr></thead><tbody>${items.map(item => `<tr>${columns.map(([, f]) => `<td>${f(item)}</td>`).join('')}</tr>`).join('')}</tbody></table>`;
}

function showTestDetail(item) {
  if (!item) return;
  currentDetail=item;
  document.getElementById('test-detail').innerHTML = `
    <div class="label">${esc(item.test_type)} ${esc(item.units || '')}</div>
    <div class="value">${esc(item.label)}</div>
    <p>${esc(T('Fail {fail} / total {total}; yield {yield}; limit failures {limits}.',{fail:item.fail,total:item.total,yield:fmtPct(item.yield_percent),limits:item.limit_fail}))}</p>
    <p>${esc(T('Mean {mean}; sigma {sigma}; range {min} to {max}.',{mean:fmt(item.mean),sigma:fmt(item.sigma),min:fmt(item.min),max:fmt(item.max)}))}</p>`;
}

function filtered(items) {
  if (!state.query) return items;
  return items.filter(item => JSON.stringify(item).toLowerCase().includes(state.query));
}

function card(label, value, sub) {
  return `<div class="kpi"><div class="label">${esc(T(label))}</div><div class="value">${esc(value)}</div><div class="sub">${esc(sub)}</div></div>`;
}

function pill(value, tone) { return `<span class="pill ${tone}">${Number(value).toLocaleString()}</span>`; }
function fmt(value) { return value == null ? '-' : Number(value).toLocaleString(undefined, {maximumFractionDigits: 4}); }
function fmtPct(value) { return `${Number(value || 0).toFixed(2)}%`; }
function trim(value, max) { value = String(value); return value.length > max ? value.slice(0, max - 1) + '...' : value; }
function esc(value) { return String(value).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c])); }
function corrColor(v) {
  const a = Math.min(1, Math.abs(v));
  return v < 0 ? `rgba(107,183,255,${0.15 + a * .85})` : `rgba(255,191,95,${0.15 + a * .85})`;
}
function emptyCanvas(ctx, canvas, text) {
  ctx.fillStyle = C('muted');
  ctx.font = '15px Segoe UI';
  ctx.textAlign = 'center';
  ctx.fillText(T(text), canvas.width / 2, canvas.height / 2);
  ctx.textAlign = 'left';
}
function roundRect(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}
const requestedTab=new URLSearchParams(location.search).get('tab');
if(['overview','pareto','commonality','correlation','spatial','quality'].includes(requestedTab))document.querySelector('[data-tab="'+requestedTab+'"]').click();
window.addEventListener('resize', render);
window.addEventListener('report-ui-change',()=>{dimensionSelect.options[0].textContent=T('all dimensions');if(snapshot.lots)lotSelect.options[0].textContent=T('All lots');render();if(currentDetail)showTestDetail(currentDetail);else document.getElementById('test-detail').textContent=T('Click a Pareto bar or row to inspect limits and distribution.');updateDatasetStatus();});
render();
</script>
</body>
</html>"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyze_rows_computes_yield_and_failure_pareto() {
        let rows = sample_rows();

        let data = analyze_rows(&rows, &DashboardOptions::default());

        assert_eq!(data.kpis.rows, 6);
        assert_eq!(data.kpis.parts, 3);
        assert_eq!(data.kpis.failed_parts, 1);
        assert!((data.kpis.yield_percent - 66.666).abs() < 0.01);
        assert_eq!(data.pareto[0].test_num, 200);
        assert_eq!(data.pareto[0].fail, 1);
        assert!(data
            .commonality
            .iter()
            .any(|item| item.dimension == "site" && item.label == "H1 / S1"));
    }

    #[test]
    fn analyze_rows_detects_correlations() {
        let rows = sample_rows();

        let data = analyze_rows(
            &rows,
            &DashboardOptions {
                max_correlation_tests: 4,
                ..DashboardOptions::default()
            },
        );

        assert_eq!(data.correlations.len(), 1);
        assert!(data.correlations[0].correlation > 0.99);
        assert_eq!(data.correlations[0].pairs, 3);
    }

    #[test]
    fn render_html_contains_interactive_sections_and_safe_json() {
        let rows = sample_rows();
        let data = analyze_rows(
            &rows,
            &DashboardOptions {
                title: "A <lot> dashboard".to_string(),
                ..DashboardOptions::default()
            },
        );

        let html = render_html(&data);

        assert!(html.contains("A &lt;lot&gt; dashboard"));
        assert!(html.contains("failure-commonality"));
        assert!(html.contains("correlation-heatmap"));
        assert!(html.contains("dashboard-data"));
        assert!(!html.contains("A <lot> dashboard"));
    }

    fn sample_rows() -> Vec<TestRow> {
        vec![
            row("P1", 0, true, 100, 1.0, true),
            row("P1", 0, true, 200, 2.0, true),
            row("P2", 1, false, 100, 2.0, true),
            row("P2", 1, false, 200, 4.0, false),
            row("P3", 1, true, 100, 3.0, true),
            row("P3", 1, true, 200, 6.0, true),
        ]
    }

    #[test]
    fn resolved_coordinates_merge_across_sources_and_sites() {
        let a = row("P1", 0, true, 1, 1.0, true);
        let mut b = a.clone();
        b.part_id = "DIFFERENT".into();
        b.head_num = 2;
        b.site_num = 9;
        b.lot_id = "ANOTHER".into();
        b.part_sequence = 20;
        assert_eq!(scoped_part_key(&a, "one"), scoped_part_key(&b, "two"));
        let mut all = AnalysisAccumulator::default();
        all.push_scoped(&a, "one");
        all.push_scoped(&b, "two");
        assert_eq!(all.parts.len(), 1);
    }

    #[test]
    fn unresolved_attempts_are_not_merged_by_repeated_part_id() {
        let mut a = row("P1", 0, true, 1, 1.0, true);
        a.part_merge_key = None;
        let mut b = a.clone();
        b.part_sequence += 1;
        assert_ne!(scoped_part_key(&a, "one"), scoped_part_key(&b, "one"));
        assert_ne!(scoped_part_key(&a, "one"), scoped_part_key(&a, "two"));
        let data = analyze_rows(&[a, b], &DashboardOptions::default());
        assert_eq!(data.kpis.parts, 2);
        assert_eq!(
            data.data_quality
                .iter()
                .find(|q| q.label == "unresolved coordinate identity")
                .unwrap()
                .fail,
            2
        );
    }

    fn row(
        part_id: &str,
        site_num: u8,
        part_pass: bool,
        test_num: u32,
        result: f32,
        test_pass: bool,
    ) -> TestRow {
        let coordinate = part_id.trim_start_matches('P').parse::<i16>().unwrap_or(1);
        TestRow {
            part_sequence: coordinate as u64,
            part_merge_key: stdf_arrow::identity::wafer_key(
                Some("W1"),
                Some(coordinate),
                Some(coordinate + 1),
            ),
            lot_id: "LOT1".to_string(),
            wafer_id: Some("W1".to_string()),
            part_id: part_id.to_string(),
            head_num: 1,
            site_num,
            x_coord: Some(coordinate),
            y_coord: Some(coordinate + 1),
            hard_bin: if part_pass { 1 } else { 9 },
            soft_bin: if part_pass { 1 } else { 99 },
            part_pass,
            test_num,
            test_txt: Some(format!("T{test_num}")),
            test_type: "PTR".to_string(),
            result: Some(result),
            test_pass: Some(test_pass),
            lo_limit: Some(0.0),
            hi_limit: Some(10.0),
            units: Some("V".to_string()),
            test_time_ms: Some(2),
        }
    }
}
