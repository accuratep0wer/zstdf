// Copyright 2026 zstdf contributors
// SPDX-License-Identifier: Apache-2.0

use super::{
    CommonalityAggregate, CorrelationPair, DashboardData, DiePoint, GroupAggregate, HeatmapPoint,
    Kpis, ProcessWindow, TestAggregate, TestHistogram,
};

pub(crate) fn render_html(data: &DashboardData) -> String {
    DASHBOARD_HTML
        .replace("__TITLE__", &html_escape(&data.title))
        .replace("__DATA__", &dashboard_json(data))
}

pub(crate) fn dashboard_json(data: &DashboardData) -> String {
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
            "\"die_points\":{},",
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
        die_points_json(&data.die_points),
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
                    "\"yield_percent\":{},\"histogram\":{}",
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
                json_f64(item.yield_percent),
                histogram_json(item.histogram.as_ref())
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

fn histogram_json(item: Option<&TestHistogram>) -> String {
    let Some(item) = item else {
        return "null".to_string();
    };
    let counts = item
        .counts
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "{{",
            "\"bin_lo\":{},\"bin_width\":{},\"counts\":[{}],",
            "\"below\":{},\"above\":{},\"observed\":{},",
            "\"sampled\":{},\"spec\":{}",
            "}}"
        ),
        json_f64(item.bin_lo),
        json_f64(item.bin_width),
        counts,
        item.below,
        item.above,
        item.observed,
        if item.sampled { "true" } else { "false" },
        if item.spec { "true" } else { "false" }
    )
}

fn die_points_json(items: &[DiePoint]) -> String {
    let values: Vec<_> = items
        .iter()
        .map(|item| {
            format!(
                "{{\"wafer\":{},\"x\":{},\"y\":{},\"hard_bin\":{},\"soft_bin\":{},\"pass\":{}}}",
                json_string(&item.wafer),
                item.x,
                item.y,
                item.hard_bin,
                item.soft_bin,
                if item.pass { "true" } else { "false" }
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

pub(crate) fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(crate) const DASHBOARD_HTML: &str = r#"<!doctype html>
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
.panel-head { display: flex; justify-content: space-between; align-items: center; gap: 10px; flex-wrap: wrap; margin-bottom: 12px; }
.panel-head h2 { margin: 0; }
.hint { color: var(--muted); font-size: 12px; line-height: 1.45; margin: 8px 0 0; }
.cockpit { margin-top: 14px; }
.kpi.good .value { color: var(--green); }
.kpi.warn .value { color: var(--amber); }
.kpi.bad .value { color: var(--red); }
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
      <p id="analysis-policy" class="muted" data-i18n="Parts merge by coordinate identity; unresolved attempts remain separate. Device yield uses all-pass merging, not final-retest yield.">Parts merge by coordinate identity; unresolved attempts remain separate. Device yield uses all-pass merging, not final-retest yield.</p>
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
    <p class="hint" data-i18n="Yield cockpit compares bins, a die wafer map, and one parametric distribution. It uses the same merged-part population as the other tabs.">Yield cockpit compares bins, a die wafer map, and one parametric distribution. It uses the same merged-part population as the other tabs.</p>
    <div class="grid two cockpit">
      <div class="panel">
        <div class="panel-head">
          <h2 data-i18n="Bin Pareto">Bin Pareto</h2>
          <select id="bin-kind" aria-label="Bin Pareto" data-i18n-label="Bin Pareto">
            <option value="hard" data-i18n="Hard bin">Hard bin</option>
            <option value="soft" data-i18n="Soft bin">Soft bin</option>
          </select>
        </div>
        <canvas id="hard-bin-chart"></canvas>
        <p class="hint" data-i18n="Bars rank failure count. The line is cumulative share.">Bars rank failure count. The line is cumulative share.</p>
        <p id="bin-caption" class="hint"></p>
      </div>
      <div class="panel">
        <div class="panel-head">
          <h2 data-i18n="Wafer Map">Wafer Map</h2>
          <select id="wafer-select" aria-label="Wafer" data-i18n-label="Wafer"></select>
        </div>
        <canvas id="overview-wafer"></canvas>
        <p id="wafer-caption" class="hint"></p>
      </div>
    </div>
    <div class="grid two cockpit">
      <div class="panel">
        <div class="panel-head">
          <h2 data-i18n="Parametric Distribution">Parametric Distribution</h2>
          <select id="histogram-test" aria-label="Parametric test" data-i18n-label="Parametric test"></select>
        </div>
        <canvas id="param-histogram"></canvas>
        <p id="histogram-note" class="hint"></p>
      </div>
      <div class="panel"><h2 data-i18n="Yield By Site">Yield By Site</h2><canvas id="site-yield-chart"></canvas></div>
    </div>
  </section>

  <section id="pareto" class="section">
    <div class="grid two">
      <div class="panel"><h2 data-i18n="Failure Pareto">Failure Pareto</h2><canvas id="pareto-chart"></canvas></div>
      <div class="panel"><h2 data-i18n="Test Detail">Test Detail</h2><div id="test-detail" class="detail"><span data-i18n="Click a Pareto bar or row to inspect limits and distribution.">Click a Pareto bar or row to inspect limits and distribution.</span></div><canvas id="detail-histogram"></canvas></div>
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
      <div class="panel"><h2 data-i18n="Wafer Map">Wafer Map</h2><canvas id="xy-map"></canvas><p id="spatial-caption" class="hint"></p></div>
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
const state = { tab: 'overview', query: '', minFails: 0, binKind: 'hard', wafer: '', histogramLabel: '' };
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
document.getElementById('bin-kind').addEventListener('change', event => {
  state.binKind = event.target.value === 'soft' ? 'soft' : 'hard';
  renderBinPareto('hard-bin-chart', binSeries());
});
document.getElementById('wafer-select').addEventListener('change', event => {
  state.wafer = event.target.value;
  renderDieMap('overview-wafer', 'wafer-caption');
  renderDieMap('xy-map', 'spatial-caption');
});
document.getElementById('histogram-test').addEventListener('change', event => {
  state.histogramLabel = event.target.value;
  renderHistogram('param-histogram', (data.pareto || []).find(item => item.label === state.histogramLabel) || null, 'histogram-note');
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
  const policy=document.getElementById('analysis-policy');
  policy.dataset.i18n=state.tab==='pareto'?'One selected attempt per lot + wafer + X/Y device. Latest is determined by MIR START_T; source-local order breaks ties within a run.':'Parts merge by coordinate identity; unresolved attempts remain separate. Device yield uses all-pass merging, not final-retest yield.';
  policy.textContent=T(policy.dataset.i18n);
  renderKpis();
  renderCockpit();
  renderPareto();
  renderCommonality();
  renderCorrelation();
  renderSpatial();
  renderQuality();
  renderHistogram('detail-histogram', currentDetail, null);
}

function renderCockpit() {
  renderBinPareto('hard-bin-chart', binSeries());
  syncWaferSelect();
  renderDieMap('overview-wafer', 'wafer-caption');
  renderHistogram('param-histogram', syncHistogramSelect(), 'histogram-note');
  renderBar('site-yield-chart', data.site_yield.slice(0, 16), item => item.label, item => item.yield_percent, item => item.yield_percent < 90 ? C('bad-ink') : C('good-ink'), '%');
}

function binSeries() {
  const source = (state.binKind === 'soft' ? data.soft_bins : data.hard_bins) || [];
  const failing = source.filter(item => item.fail > 0);
  return (failing.length ? failing : source).slice(0, 12);
}

function renderKpis() {
  const k = data.kpis;
  const tone = k.yield_percent >= 98 ? 'good' : k.yield_percent >= 90 ? 'warn' : 'bad';
  document.getElementById('kpis').innerHTML = [
    card('Yield', fmtPct(k.yield_percent), T('{pass}/{total} parts passed',{pass:k.passed_parts,total:k.parts}), tone),
    card('Fail Parts', k.failed_parts.toLocaleString(), T('{n} failing tests',{n:k.failing_tests})),
    card('Rows', k.rows.toLocaleString(), T('EAV test result rows')),
    card('Tests', k.tests.toLocaleString(), T('unique test numbers')),
    card('Wafers', k.wafers.toLocaleString(), T('{n} lots',{n:k.lots})),
    card('Sites', k.sites.toLocaleString(), T('head / site combinations')),
  ].join('');
}

function renderPareto() {
  if(window.LatestPareto){window.LatestPareto.render({query:state.query,minFails:state.minFails,lot:snapshot.lots&&Number(lotSelect.value)>0?snapshot.lots[Number(lotSelect.value)-1].label:null});return;}

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
  renderDieMap('xy-map', 'spatial-caption');
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

function renderBar(id, items, label, value, color, suffix, onPick, overlay) {
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
  if (overlay) overlay({ctx, left, top, stride, barH, plotWidth});
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

function renderBinPareto(id, items) {
  const caption = document.getElementById('bin-caption');
  const metric = item => item.fail > 0 || items.some(entry => entry.fail > 0) ? item.fail : item.total;
  renderBar(id, items, item => item.label, metric, item => item.fail ? C('bad-ink') : C('good-ink'), ' '+T('parts'), item => {
    if (caption) caption.textContent = T('{label}: {fail} failing of {total}, yield {yield}.', {label: item.label, fail: item.fail, total: item.total, yield: fmtPct(item.yield_percent)});
  }, ({ctx, left, top, stride, barH, plotWidth}) => {
    const total = items.reduce((sum, item) => sum + metric(item), 0);
    if (!total) return;
    ctx.save();
    ctx.strokeStyle = C('accent') || '#14699a';
    ctx.lineWidth = 2;
    ctx.beginPath();
    let cumulative = 0;
    items.forEach((item, index) => {
      cumulative += metric(item);
      const x = left + plotWidth * cumulative / total;
      const y = top + index * stride + barH / 2;
      if (index === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    ctx.stroke();
    ctx.restore();
  });
  if (!caption) return;
  if (!items.length) caption.textContent = T('No bins in this population.');
  else caption.textContent = T('{label}: {fail} failing of {total}, yield {yield}.', {label: items[0].label, fail: items[0].fail, total: items[0].total, yield: fmtPct(items[0].yield_percent)});
}

function syncWaferSelect() {
  const select = document.getElementById('wafer-select');
  const wafers = [...new Set((data.die_points || []).map(die => die.wafer))];
  const previous = state.wafer;
  select.replaceChildren(new Option(T('All wafers'), ''));
  for (const wafer of wafers) select.appendChild(new Option(wafer, wafer));
  state.wafer = wafers.includes(previous) ? previous : '';
  select.value = state.wafer;
}

function syncHistogramSelect() {
  const select = document.getElementById('histogram-test');
  const items = (data.pareto || []).filter(item => item.histogram && item.histogram.counts && item.histogram.counts.length);
  const previous = state.histogramLabel;
  select.replaceChildren();
  for (const item of items) select.appendChild(new Option(item.label, item.label));
  let pick = items.find(item => item.label === previous);
  if (!pick) {
    pick = items.slice().sort((a, b) => (b.limit_fail - a.limit_fail) || (b.fail - a.fail) || a.label.localeCompare(b.label))[0] || null;
  }
  state.histogramLabel = pick ? pick.label : '';
  if (pick) select.value = pick.label;
  return pick;
}

function visibleDies() {
  const dies = data.die_points || [];
  return state.wafer ? dies.filter(die => die.wafer === state.wafer) : dies;
}

function stackDies(dies) {
  const map = new Map();
  for (const die of dies) {
    const key = `${die.x}:${die.y}`;
    const current = map.get(key);
    if (!current) {
      map.set(key, {x: die.x, y: die.y, hard_bin: die.hard_bin, soft_bin: die.soft_bin, pass: !!die.pass, total: 1, fail: die.pass ? 0 : 1});
    } else {
      current.total += 1;
      if (!die.pass) {
        current.fail += 1;
        current.hard_bin = die.hard_bin;
        current.soft_bin = die.soft_bin;
      }
      current.pass = current.pass && !!die.pass;
    }
  }
  return [...map.values()];
}

function dieColor(die) {
  if (die.pass) return C('good-ink') || '#1f8a4c';
  if (die.total > 1 && die.fail < die.total) return C('warn-ink') || '#b58105';
  const palette = [C('bad-ink'), C('warn-ink'), C('accent'), C('blue-ink')].filter(Boolean);
  return palette[(Number(die.hard_bin) || 0) % Math.max(palette.length, 1)] || '#963912';
}

function renderDieMap(id, captionId) {
  const canvas = setupCanvas(id);
  const ctx = canvas.getContext('2d');
  canvas.onclick = null;
  const dies = visibleDies();
  const caption = document.getElementById(captionId);
  if (!dies.length) {
    if (caption) caption.textContent = T('No die coordinates in this population.');
    return emptyCanvas(ctx, canvas, 'No die coordinates in this population.');
  }
  const rect = canvas.getBoundingClientRect();
  const width = rect.width || 320, height = rect.height || 300;
  ctx.setTransform(canvas.width / width, 0, 0, canvas.height / height, 0, 0);
  ctx.clearRect(0, 0, width, height);
  const stacked = stackDies(dies);
  const minX = Math.min(...stacked.map(die => die.x)), maxX = Math.max(...stacked.map(die => die.x));
  const minY = Math.min(...stacked.map(die => die.y)), maxY = Math.max(...stacked.map(die => die.y));
  const cols = maxX - minX + 1, rows = maxY - minY + 1;
  const pad = 12, legend = 16;
  const cellW = (width - pad * 2) / cols, cellH = (height - pad * 2 - legend) / rows;
  const hits = [];
  for (const die of stacked) {
    const x = pad + (die.x - minX) * cellW;
    const y = pad + (maxY - die.y) * cellH;
    const inset = Math.min(cellW, cellH) > 5 ? 0.6 : 0;
    ctx.fillStyle = dieColor(die);
    ctx.fillRect(x + inset, y + inset, Math.max(cellW - inset * 2, 0.4), Math.max(cellH - inset * 2, 0.4));
    hits.push({x, y, w: cellW, h: cellH, die});
  }
  ctx.fillStyle = C('muted') || '#526979';
  ctx.font = '11px Segoe UI';
  ctx.textAlign = 'left';
  ctx.textBaseline = 'alphabetic';
  ctx.fillText(`${T('Pass')} / ${T('Fail')}`, pad, height - 4);
  if (caption) caption.textContent = T('{parts} parts with coordinates. {cells} map cells. X {minx} to {maxx}, Y {miny} to {maxy}.', {parts: dies.length, cells: stacked.length, minx: minX, maxx: maxX, miny: minY, maxy: maxY});
  canvas.setAttribute('role', 'img');
  canvas.setAttribute('aria-label', caption ? caption.textContent : 'Wafer Map');
  canvas.onclick = event => {
    const bounds = canvas.getBoundingClientRect();
    const px = (event.clientX - bounds.left) * width / bounds.width;
    const py = (event.clientY - bounds.top) * height / bounds.height;
    const hit = hits.find(item => px >= item.x && px < item.x + item.w && py >= item.y && py < item.y + item.h);
    if (!hit || !caption) return;
    const die = hit.die;
    caption.textContent = T('X {x}, Y {y}, hard bin {hard}, soft bin {soft}, {verdict}.', {x: die.x, y: die.y, hard: die.hard_bin, soft: die.soft_bin, verdict: die.pass ? T('Pass') : die.total > 1 ? T('Mixed') : T('Fail')});
  };
}

function renderHistogram(id, item, noteId) {
  if (!document.getElementById(id)) return;
  const canvas = setupCanvas(id);
  if (!canvas) return;
  const ctx = canvas.getContext('2d');
  const note = noteId ? document.getElementById(noteId) : null;
  const histogram = item && item.histogram;
  if (!histogram || !histogram.counts || !histogram.counts.length) {
    if (note) note.textContent = '';
    return emptyCanvas(ctx, canvas, item ? 'No numeric distribution for this test.' : 'Click a Pareto bar or row to inspect limits and distribution.');
  }
  const rect = canvas.getBoundingClientRect();
  const width = rect.width || 320, height = rect.height || 300;
  ctx.setTransform(canvas.width / width, 0, 0, canvas.height / height, 0, 0);
  ctx.clearRect(0, 0, width, height);
  const bins = [];
  if (histogram.spec) bins.push({count: histogram.below, tail: true});
  histogram.counts.forEach(count => bins.push({count, tail: false}));
  if (histogram.spec) bins.push({count: histogram.above, tail: true});
  const max = Math.max(...bins.map(bin => bin.count), 1);
  const left = 8, right = 8, top = 12, bottom = 26;
  const plotW = Math.max(1, width - left - right), plotH = Math.max(1, height - top - bottom);
  bins.forEach((bin, index) => {
    const slot = plotW / bins.length;
    const barH = plotH * bin.count / max;
    ctx.fillStyle = bin.tail ? (C('bad-ink') || '#963912') : (C('accent') || '#14699a');
    ctx.fillRect(left + index * slot + 0.5, top + plotH - barH, Math.max(slot - 1, 0.4), Math.max(barH, bin.count ? 1 : 0));
  });
  ctx.strokeStyle = C('muted') || '#526979';
  ctx.beginPath();
  ctx.moveTo(left, top);
  ctx.lineTo(left, top + plotH);
  ctx.lineTo(left + plotW, top + plotH);
  ctx.stroke();
  ctx.fillStyle = C('muted') || '#526979';
  ctx.font = '11px Segoe UI';
  ctx.textBaseline = 'top';
  const hi = histogram.bin_lo + histogram.bin_width * histogram.counts.length;
  ctx.textAlign = 'left';
  ctx.fillText(fmt(histogram.bin_lo), left, top + plotH + 6);
  ctx.textAlign = 'right';
  ctx.fillText(fmt(hi), left + plotW, top + plotH + 6);
  if (note) {
    note.textContent = histogram.sampled
      ? T('Sampled distribution of {observed} results.', {observed: histogram.observed})
      : T('{observed} results. Below limit {below}. Above limit {above}.', {observed: histogram.observed, below: histogram.below, above: histogram.above});
  }
  canvas.setAttribute('role', 'img');
  canvas.setAttribute('aria-label', T('{label} distribution', {label: item.label}));
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
  renderHistogram('detail-histogram', item, null);
}

function filtered(items) {
  if (!state.query) return items;
  return items.filter(item => JSON.stringify(item).toLowerCase().includes(state.query));
}

function card(label, value, sub, tone) {
  return `<div class="kpi${tone ? ' ' + tone : ''}"><div class="label">${esc(T(label))}</div><div class="value">${esc(value)}</div><div class="sub">${esc(sub)}</div></div>`;
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
