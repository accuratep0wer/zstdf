//! Source sanity is separate from historical yield and traceability aggregation.
use crate::CliResult;
use clap::Args;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use stdf_validate::fields::{self, Field};

mod profiles;
mod scan;
mod storage;
mod text;
use profiles::Profiles;
#[cfg(test)]
mod tests;

#[derive(Debug, Args)]
pub struct Arguments {
    #[arg(required = true)]
    inputs: Vec<PathBuf>,
    #[arg(long,value_parser=["cp","ft"],required_unless_present="run_profiles",conflicts_with="run_profiles")]
    test_domain: Option<String>,
    #[arg(long, conflicts_with = "run_profiles")]
    profile: Option<PathBuf>,
    #[arg(long)]
    run_profiles: Option<PathBuf>,
    /// Publish an offline HTML report and its evidence bundle.
    #[arg(
        long,
        required_unless_present = "text_summary",
        conflicts_with = "text_summary"
    )]
    output_dir: Option<PathBuf>,
    /// Write a full-file invalid/missing field summary as UTF-8 text, without an HTML bundle.
    #[arg(long, conflicts_with = "output_dir")]
    text_summary: Option<PathBuf>,
    /// Return a failure when the text summary contains missing fields, including optional fields.
    #[arg(long, requires = "text_summary", conflicts_with = "output_dir")]
    fail_on_missing: bool,
    #[arg(long, default_value_t = 2)]
    preview_records_per_type: usize,
    #[arg(long, default_value_t = 32)]
    max_report_mib: usize,
    #[arg(long, default_value_t = 1024)]
    disk_limit_mib: u64,
    #[arg(long, default_value_t = 100_000)]
    max_units: usize,
    #[arg(long, default_value_t = 10_000)]
    max_sources: usize,
    /// Stop cooperatively if this path exists, including immediately before publication.
    #[arg(long)]
    cancel_file: Option<PathBuf>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    version: u32,
    id: String,
    domain: String,
    #[serde(default)]
    require_wafer: bool,
    #[serde(default)]
    rules: Vec<Rule>,
    #[serde(default)]
    important_fields: BTreeMap<String, Vec<String>>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    record: String,
    field: String,
    #[serde(default)]
    required: bool,
    pattern: Option<String>,
    #[serde(skip)]
    compiled_pattern: Option<regex::Regex>,
    allowed: Option<Vec<Value>>,
    min: Option<f64>,
    max: Option<f64>,
}
impl Profile {
    fn load(args: &Arguments) -> CliResult<Self> {
        let domain = args
            .test_domain
            .as_deref()
            .ok_or("--test-domain or --run-profiles is required")?;
        let mut p: Self = if let Some(path) = &args.profile {
            if fs::metadata(path)?.len() > 1024 * 1024 {
                return Err("profile exceeds 1 MiB".into());
            }
            serde_json::from_reader(File::open(path)?)?
        } else {
            Self {
                version: 1,
                id: format!("builtin-{domain}-v1"),
                domain: domain.into(),
                require_wafer: domain == "cp",
                rules: vec![
                    Rule {
                        record: "MIR".into(),
                        field: "LOT_ID".into(),
                        required: true,
                        pattern: None,
                        compiled_pattern: None,
                        allowed: None,
                        min: None,
                        max: None,
                    },
                    Rule {
                        record: "MIR".into(),
                        field: "JOB_NAM".into(),
                        required: true,
                        pattern: None,
                        compiled_pattern: None,
                        allowed: None,
                        min: None,
                        max: None,
                    },
                ],
                important_fields: BTreeMap::new(),
            }
        };
        if p.domain != domain {
            return Err("profile version/domain mismatch or empty id".into());
        }
        p.validate()?;
        Ok(p)
    }
    fn validate(&mut self) -> CliResult<()> {
        if self.version != 1 || self.id.is_empty() || !matches!(self.domain.as_str(), "cp" | "ft") {
            return Err("profile version/domain mismatch or empty id".into());
        }
        for rule in &mut self.rules {
            if fields::sanity_exempt(&rule.record) {
                continue;
            }
            let layout = fields::layout(&rule.record).ok_or("unsupported profile record")?;
            if !layout
                .split_whitespace()
                .any(|s| s.split(':').next() == Some(rule.field.as_str()))
            {
                return Err(format!("unknown profile field {}.{}", rule.record, rule.field).into());
            }
            if rule.min.zip(rule.max).is_some_and(|(a, b)| a > b) {
                return Err("profile min exceeds max".into());
            }
            if let Some(pattern) = &rule.pattern {
                rule.compiled_pattern = Some(regex::Regex::new(&format!("\\A(?:{pattern})\\z"))?);
            }
        }
        for (record, names) in &self.important_fields {
            let layout = fields::layout(record).ok_or("unknown important_fields record")?;
            for name in names {
                if !layout
                    .split_whitespace()
                    .any(|t| t.split(':').next() == Some(name))
                {
                    return Err(format!("unknown important field {record}.{name}").into());
                }
            }
        }
        Ok(())
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Report {
    schema: String,
    rule_version: String,
    profile: Value,
    profile_hash: String,
    coverage: Vec<String>,
    sources: Vec<Source>,
    runs: Vec<Run>,
    units: Vec<Unit>,
    findings: Vec<Value>,
    records: BTreeMap<String, u64>,
    #[serde(default)]
    file_records: Vec<Value>,
    preview_records_per_type: usize,
    validation_failed: bool,
}
#[derive(Serialize, Deserialize)]
struct Source {
    id: String,
    paths: Vec<String>,
    bytes: u64,
    scan_complete: bool,
}
#[derive(Serialize, Deserialize)]
struct Run {
    id: String,
    source: String,
    domain: String,
    profile_id: Option<String>,
    profile_hash: Option<String>,
    metadata: Vec<Value>,
    unassigned: Vec<Value>,
    closed: bool,
}
#[derive(Default, Serialize, Deserialize)]
struct Unit {
    id: String,
    run: String,
    head: u8,
    site: u8,
    sequence: u64,
    part_id: Option<String>,
    wafer: Option<String>,
    merge_key: Option<String>,
    prr: Option<Value>,
    closed: bool,
    context: Vec<String>,
    counts: BTreeMap<String, u64>,
    previews: BTreeMap<String, Vec<Value>>,
}
struct Budget<'a> {
    args: &'a Arguments,
    retained: usize,
    max: usize,
}
impl Budget<'_> {
    fn check(&self) -> CliResult<()> {
        if self.args.cancel_file.as_ref().is_some_and(|p| p.exists()) {
            return Err("sanity cancelled; previous report retained".into());
        }
        Ok(())
    }
    fn charge(&mut self, n: usize) -> CliResult<()> {
        self.check()?;
        self.retained = self.retained.checked_add(n).ok_or("report size overflow")?;
        if self.retained > self.max {
            return Err("sanity report budget exceeded; no report published".into());
        }
        Ok(())
    }
}
fn issue(
    report: &mut Report,
    budget: &mut Budget,
    source: &str,
    offset: usize,
    severity: &str,
    rule: &str,
    message: impl Into<String>,
) -> CliResult<()> {
    let v = json!({"source":source,"offset":offset.to_string(),"severity":severity,"rule":rule,"message":message.into()});
    budget.charge(serde_json::to_vec(&v)?.len() + 256)?;
    if severity == "error" {
        report.validation_failed = true;
    }
    report.findings.push(v);
    Ok(())
}
fn open(path: &Path) -> CliResult<Box<dyn Read>> {
    let mut f = File::open(path)?;
    let mut magic = [0; 2];
    let n = f.read(&mut magic)?;
    f.seek(SeekFrom::Start(0))?;
    if n == 2 && magic == [0x1f, 0x8b] {
        Ok(Box::new(flate2::read::MultiGzDecoder::new(f)))
    } else {
        Ok(Box::new(f))
    }
}
fn inputs(args: &Arguments, budget: &Budget) -> CliResult<Vec<PathBuf>> {
    fn visit(p: &Path, paths: &mut Vec<PathBuf>, budget: &Budget) -> CliResult<()> {
        budget.check()?;
        let metadata = fs::symlink_metadata(p)?;
        if metadata.file_type().is_symlink() {
            return Err(format!("symlink input is not supported: {}", p.display()).into());
        }
        if metadata.is_dir() {
            for entry in fs::read_dir(p)? {
                let entry = entry?;
                let p = entry.path();
                if p.is_dir() {
                    visit(&p, paths, budget)?;
                } else {
                    let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                    if [".stdf", ".std", ".stdf.gz", ".std.gz"]
                        .iter()
                        .any(|s| name.ends_with(s))
                    {
                        visit(&p, paths, budget)?;
                    }
                }
            }
        } else {
            if paths.len() >= budget.args.max_sources {
                return Err("source count limit exceeded".into());
            }
            paths.push(p.to_path_buf());
        }
        Ok(())
    }
    let mut paths = Vec::new();
    for input in &args.inputs {
        visit(input, &mut paths, budget)?;
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}
pub fn execute(args: Arguments, out: &mut impl Write) -> CliResult<()> {
    generate(&args, out)
}
fn generate(args: &Arguments, out: &mut impl Write) -> CliResult<()> {
    if args.fail_on_missing && args.text_summary.is_none() {
        return Err("--fail-on-missing requires --text-summary".into());
    }
    if args.preview_records_per_type == 0
        || args.preview_records_per_type > 100
        || args.max_report_mib == 0
        || args.disk_limit_mib == 0
        || args.max_units == 0
        || args.max_sources == 0
    {
        return Err("sanity limits must be positive; preview count must be <=100".into());
    }
    let profile = Profiles::load(args)?;
    let mut budget = Budget {
        args,
        retained: 0,
        max: args
            .max_report_mib
            .checked_mul(1024 * 1024)
            .ok_or("report budget overflow")?,
    };
    budget.check()?;
    let paths = inputs(args, &budget)?;
    if paths.is_empty() {
        return Err("no STDF inputs".into());
    }
    if args.output_dir.is_some() == args.text_summary.is_some() {
        return Err("choose exactly one of --output-dir or --text-summary".into());
    }
    if let Some(output) = &args.text_summary {
        if output.exists() {
            let target = output.canonicalize()?;
            for path in paths
                .iter()
                .chain(args.profile.iter())
                .chain(args.run_profiles.iter())
            {
                if path.canonicalize()? == target {
                    return Err(
                        "text summary must not overwrite an input STDF or configuration".into(),
                    );
                }
            }
        }
    }
    let root = args.output_dir.as_deref().unwrap_or_else(|| {
        args.text_summary
            .as_ref()
            .unwrap()
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
    });
    let mut stage = storage::Stage::new(
        root,
        args.disk_limit_mib
            .checked_mul(1024 * 1024)
            .ok_or("disk budget overflow")?,
    )?;
    let mut report=Report{schema:"sanity-v1".into(),rule_version:fields::RULE_VERSION.into(),profile:serde_json::to_value(&profile)?,profile_hash:format!("{:x}",Sha256::digest(serde_json::to_vec(&profile)?)),coverage:vec!["Base-v4 field layouts plus extraction of ATR, CDR, ATER, CTSR and CTRR; these five record types skip field/profile sanity checks (not_checked). Framing/decode errors still fail; raw bytes preserved for every source".into(),"Not a complete STDF conformance certification: unlisted vendor extensions and some enum/count rules remain unsupported".into()],preview_records_per_type:args.preview_records_per_type,..Default::default()};
    let mut evidence = storage::EvidenceWriter::new(stage.file("record_fields.parquet")?)?;
    let mut hashes: BTreeMap<String, usize> = BTreeMap::new();
    for (index, path) in paths.iter().enumerate() {
        budget.check()?;
        let name = format!("source-{index}.stdf");
        let mut target = stage.file(&name)?;
        let mut source = open(path)?;
        let mut digest = Sha256::new();
        let mut buf = [0; 65536];
        let mut bytes = 0;
        loop {
            budget.check()?;
            let n = source.read(&mut buf)?;
            if n == 0 {
                break;
            }
            target.write_all(&buf[..n])?;
            digest.update(&buf[..n]);
            bytes += n as u64;
        }
        target.sync()?;
        drop(target);
        let hash = format!("{:x}", digest.finalize());
        if let Some(&existing) = hashes.get(&hash) {
            report.sources[existing]
                .paths
                .push(path.display().to_string());
            budget.charge(path.as_os_str().len() * 4 + 128)?;
            stage.remove(&name)?;
            continue;
        }
        hashes.insert(hash.clone(), report.sources.len());
        budget.charge(path.as_os_str().len() * 4 + 512)?;
        report.sources.push(Source {
            id: hash.clone(),
            paths: vec![path.display().to_string()],
            bytes,
            scan_complete: false,
        });
        let archived = format!("{hash}.stdf");
        stage.rename(&name, &archived)?;
        scan::scan(
            &stage.path.join(&archived),
            &hash,
            &profile,
            &mut report,
            &mut evidence,
            &mut budget,
        )?;
    }
    evidence.finish()?;
    if let Some(output) = &args.text_summary {
        let (summary, missing) = text::render(&stage.path, &report, &mut budget)?;
        stage.publish_text(output, &summary, || {
            budget.check().map_err(|e| e.to_string())
        })?;
        writeln!(
            out,
            "sources={} runs={} units={} validation_failed={} missing_fields={}\ntext_summary={}",
            report.sources.len(),
            report.runs.len(),
            report.units.len(),
            report.validation_failed,
            missing,
            output.display()
        )?;
        if report.validation_failed {
            return Err("sanity validation failed; diagnostic text summary published".into());
        }
        if args.fail_on_missing && missing > 0 {
            return Err("sanity missing fields found; diagnostic text summary published".into());
        }
        return Ok(());
    }
    let data = serde_json::to_vec(&report)?;
    if data.len() > budget.max {
        return Err("serialized report size limit exceeded".into());
    }
    stage.write("summary.json", &data)?;
    let safe = String::from_utf8(data)?
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    let html = include_str!("report.html")
        .replace("__GENERATION__", &stage.name)
        .replace("__SANITY_DATA__", &safe);
    if html.len() > budget.max {
        return Err("HTML report size limit exceeded".into());
    }
    stage.write("report.html", html.as_bytes())?;
    stage.manifest(&report.profile_hash)?;
    budget.check()?;
    stage.publish(html.as_bytes(), || {
        budget.check().map_err(|e| e.to_string())
    })?;
    writeln!(
        out,
        "sources={} runs={} units={} findings={} validation_failed={}\nreport={}",
        report.sources.len(),
        report.runs.len(),
        report.units.len(),
        report.findings.len(),
        report.validation_failed,
        args.output_dir
            .as_ref()
            .unwrap()
            .join("report.html")
            .display()
    )?;
    if report.validation_failed {
        return Err("sanity validation failed; diagnostic report published".into());
    }
    Ok(())
}
