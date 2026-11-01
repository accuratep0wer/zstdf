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
mod render;
pub use dataset::{generate_dataset_dashboard, DatasetLimits};
pub(crate) use render::{dashboard_json, html_escape, render_html, DASHBOARD_HTML};

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
pub(super) struct DashboardData {
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
pub(super) struct Kpis {
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
pub(super) struct TestAggregate {
    key: String,
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
pub(super) struct GroupAggregate {
    label: String,
    total: usize,
    pass: usize,
    fail: usize,
    yield_percent: f64,
}

#[derive(Debug, Clone)]
pub(super) struct CommonalityAggregate {
    dimension: String,
    label: String,
    count: usize,
}

#[derive(Debug, Clone)]
pub(super) struct CorrelationPair {
    x: String,
    y: String,
    correlation: f64,
    pairs: usize,
}

#[derive(Debug, Clone)]
pub(super) struct HeatmapPoint {
    x: i16,
    y: i16,
    total: usize,
    fail: usize,
    yield_percent: f64,
}

#[derive(Debug, Clone)]
pub(super) struct ProcessWindow {
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
    key: String,
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
            key: test_key(row),
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
            key: self.key,
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
    let html = crate::latest_pareto::attach(
        render_html(&data),
        &options.ftr_inputs,
        output,
        256 * 1024 * 1024,
    )?;
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
    tests: BTreeMap<String, RunningTest>,
    lots: BTreeSet<String>,
    wafers: BTreeSet<String>,
    commonality: BTreeMap<(String, String), usize>,
    part_results: BTreeMap<String, BTreeMap<String, f64>>,
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
            .entry(test_key(row))
            .or_insert_with(|| RunningTest::new(row))
            .push(row);

        if let Some(result) = row.result.map(f64::from).filter(|value| value.is_finite()) {
            self.part_results
                .entry(part_key)
                .or_default()
                .entry(test_key(row))
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
    part_results: &BTreeMap<String, BTreeMap<String, f64>>,
    tests: &[TestAggregate],
    max_tests: usize,
) -> Vec<CorrelationPair> {
    let selected: Vec<_> = tests
        .iter()
        .filter(|test| test.mean.is_some() && test.sigma.unwrap_or(0.0) > 0.0)
        .take(max_tests.max(2))
        .map(|test| (test.key.clone(), test.label.clone()))
        .collect();
    let labels: HashMap<_, _> = selected.iter().cloned().collect();
    let mut pairs = Vec::new();

    for left_index in 0..selected.len() {
        for right_index in (left_index + 1)..selected.len() {
            let left = &selected[left_index].0;
            let right = &selected[right_index].0;
            if let Some((correlation, count)) = pearson(part_results, left, right) {
                pairs.push(CorrelationPair {
                    x: labels
                        .get(left)
                        .cloned()
                        .unwrap_or_else(|| left.to_string()),
                    y: labels
                        .get(right)
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
    part_results: &BTreeMap<String, BTreeMap<String, f64>>,
    left: &str,
    right: &str,
) -> Option<(f64, usize)> {
    let mut n = 0usize;
    let mut sum_x = 0.0;
    let mut sum_y = 0.0;
    let mut sum_x2 = 0.0;
    let mut sum_y2 = 0.0;
    let mut sum_xy = 0.0;

    for values in part_results.values() {
        let (Some(x), Some(y)) = (values.get(left), values.get(right)) else {
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
        bump_quality(groups, "unresolved coordinate identity");
    }
    if row.wafer_id.is_none() {
        bump_quality(groups, "missing wafer_id");
    }
    if row.part_id.is_empty() {
        bump_quality(groups, "missing part_id");
    }
    if row.x_coord.is_none() || row.y_coord.is_none() {
        bump_quality(groups, "missing x/y coordinate");
    }
    if row.test_pass.is_none() {
        bump_quality(groups, "missing test_pass");
    }
    if row.result.is_none() {
        bump_quality(groups, "missing result");
    }
    if row.lo_limit.is_none() || row.hi_limit.is_none() {
        bump_quality(groups, "missing limits");
    }
    if row.test_time_ms.is_none() {
        bump_quality(groups, "missing test_time_ms");
    }
}

fn bump_quality(groups: &mut BTreeMap<String, (usize, usize)>, label: &str) {
    // Labels are inserted by quality_seed. A missing label is a programming
    // mistake; skip rather than abort an otherwise complete report.
    if let Some(slot) = groups.get_mut(label) {
        slot.1 += 1;
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

fn test_key(row: &TestRow) -> String {
    // MPR labels include a stable zero-based result position. Never mix channels
    // or combine a functional result with a parametric test using the same number.
    serde_json::json!([
        row.test_num,
        row.test_type,
        if row.test_type == "MPR" {
            row.test_txt.as_deref()
        } else {
            None
        }
    ])
    .to_string()
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

    #[test]
    fn mpr_channels_have_independent_statistics_and_correlations() {
        let mut rows = Vec::new();
        for part in 1..=3 {
            for index in 0..2 {
                let mut r = row(
                    &format!("P{part}"),
                    0,
                    true,
                    100,
                    (part * (index + 1)) as f32,
                    true,
                );
                r.test_type = "MPR".into();
                r.test_txt = Some(format!("Voltage [MPR result {index}]"));
                r.test_pass = None;
                rows.push(r);
            }
            let mut overall = row(&format!("P{part}"), 0, true, 100, 0.0, false);
            overall.test_type = "MPR".into();
            overall.test_txt = Some("Voltage [MPR overall]".into());
            overall.result = None;
            rows.push(overall);
            let mut ftr = row(&format!("P{part}"), 0, true, 100, 0.0, true);
            ftr.test_type = "FTR".into();
            ftr.result = None;
            rows.push(ftr);
        }
        let data = analyze_rows(&rows, &DashboardOptions::default());
        assert_eq!(data.kpis.tests, 4);
        let first = data
            .pareto
            .iter()
            .find(|t| t.label.ends_with("[MPR result 0]"))
            .unwrap();
        let second = data
            .pareto
            .iter()
            .find(|t| t.label.ends_with("[MPR result 1]"))
            .unwrap();
        assert_eq!(first.mean, Some(2.0));
        assert_eq!(second.mean, Some(4.0));
        assert_eq!(first.fail, 0);
        assert_eq!(first.unknown, 3);
        let overall = data
            .pareto
            .iter()
            .find(|t| t.label.ends_with("[MPR overall]"))
            .unwrap();
        assert_eq!(overall.fail, 3);
        assert_eq!(data.correlations.len(), 1);
        assert_eq!(data.correlations[0].pairs, 3);
        assert!((data.correlations[0].correlation - 1.0).abs() < 1e-12);
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
