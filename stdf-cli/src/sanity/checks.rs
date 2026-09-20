//! Explicit field selection, independent of extraction and device identity.
use super::*;
use std::collections::BTreeSet;

pub const DEFAULT: &str = include_str!("../../../config/sanity-checks.csv");
const CTSR: &str = "REC_CUSTM:Cn CHAR_ID:Cn CHAR_NAM:Cn TAGT_INS:Cn RSLT_TITLE:Cn EXE_ORDER:Cn RSC_MOD:Cn BYPASS_FLG:Cn AXIS_CNT:U1 AXES[].AXIS_ID:U1 AXES[].AXIS_NAM:Cn AXES[].SETUP_SIG:Cn AXES[].RSC_TYP:Cn AXES[].RSC_NAM:Cn AXES[].INIT_VAL:Cn AXES[].RNG_FMT:Cn AXES[].RNG_VAL:Cn AXES[].RNG_RESO:R8 AXES[].RNG_STEP:U4 AXES[].RNG_FSTEP:U4 AXES[].RNG_SCAL:Cn AXES[].SIG_MOD:Cn AXES[].MARGIN_VAL:R8 AXES[].TRACK_CNT:U1 AXES[].TRACKING[].TRACK_ID:U1 AXES[].TRACKING[].TRACK_NAM:Cn AXES[].TRACKING[].TRACK_SETUP_SIG:Cn AXES[].TRACKING[].TRACK_RSC_TYP:Cn AXES[].TRACKING[].TRACK_RSC_NAM:Cn AXES[].TRACKING[].TRACK_INIT_VAL:Cn AXES[].TRACKING[].TRACK_RNG_FMT:Cn AXES[].TRACKING[].TRACK_RNG_VAL:Cn AXES[].TRACKING[].TRACK_MARGIN_VAL:R8";
const CTRR: &str = "REC_CUSTM:Cn CHAR_ID_REF:U4 HEAD_NUM:U1 SITE_NUM:U4 TEST_NUM_REF:Cn CELL_COORD:Cn RSLT_TAGT_INS:Cn CELL_RSLT:Cn";

#[derive(Serialize)]
pub(super) struct Checks {
    pub hash: String,
    pub csv: String,
    rows: Vec<Row>,
}
#[derive(Serialize)]
struct Row {
    record: String,
    field: String,
    flows: BTreeSet<String>,
    format: String,
}

pub(super) fn canonical(name: &str) -> String {
    // Concrete array indices are evidence addresses; policy uses [] for every member.
    let mut result = String::new();
    let mut index = false;
    for c in name.chars() {
        if c == '[' {
            index = true;
            result.push(c);
        } else if c == ']' {
            index = false;
            result.push(c);
        } else if !index {
            result.push(c);
        }
    }
    result
}
pub(super) fn layout(record: &str) -> Option<&'static str> {
    match record {
        "CTSR" => Some(CTSR),
        "CTRR" => Some(CTRR),
        _ => fields::layout(record),
    }
}
fn format(kind: &str) -> String {
    let (kind, array) = (
        kind.split('@').next().unwrap().trim_end_matches('?'),
        kind.contains('@'),
    );
    format!(
        "{}{}*{}",
        if array { "kx" } else { "" },
        &kind[..1],
        &kind[1..]
    )
}
impl Checks {
    pub fn load(path: Option<&Path>) -> CliResult<Self> {
        let csv = if let Some(path) = path {
            if fs::metadata(path)?.len() > 1024 * 1024 {
                return Err("checks CSV exceeds 1 MiB".into());
            }
            fs::read_to_string(path)?
        } else {
            DEFAULT.into()
        };
        Self::parse(csv)
    }
    fn parse(csv: String) -> CliResult<Self> {
        let mut reader = csv::ReaderBuilder::new()
            .comment(Some(b'#'))
            .trim(csv::Trim::All)
            .from_reader(csv.as_bytes());
        if reader.headers()?.iter().collect::<Vec<_>>() != ["record", "field", "flow", "format"] {
            return Err("checks CSV header must be record,field,flow,format".into());
        }
        let mut seen = BTreeSet::new();
        let mut rows = Vec::new();
        for row in reader.records() {
            let row = row?;
            let (record, field, flow, fmt) = (&row[0], &row[1], &row[2], &row[3]);
            let expected = layout(record)
                .and_then(|s| {
                    s.split_whitespace().find_map(|t| {
                        let (n, k) = t.split_once(':')?;
                        (n == field).then(|| format(k))
                    })
                })
                .ok_or_else(|| format!("unknown checks CSV field {record}.{field}"))?;
            if fmt != expected {
                return Err(
                    format!("checks CSV {record}.{field}: expected {expected}, got {fmt}").into(),
                );
            }
            let mut flows = BTreeSet::new();
            for f in flow.split('|') {
                if !matches!(f, "CP" | "FT") || !flows.insert(f.to_lowercase()) {
                    return Err(format!("invalid checks CSV flow {flow}").into());
                }
                if !seen.insert((record.to_owned(), field.to_owned(), f.to_owned())) {
                    return Err(format!("duplicate checks CSV row {record}.{field} for {f}").into());
                }
            }
            // Before MIR the flow is unknown. Require common policy for file-level records.
            if matches!(record, "FAR" | "ATR") && flows.len() != 2 {
                return Err(format!("checks CSV {record} precedes MIR: use CP|FT").into());
            }
            rows.push(Row {
                record: record.into(),
                field: field.into(),
                flows,
                format: fmt.into(),
            });
        }
        Ok(Self {
            hash: format!("{:x}", Sha256::digest(csv.as_bytes())),
            csv,
            rows,
        })
    }
    pub fn enabled(&self, record: &str, field: &str, domain: &str) -> bool {
        let name = canonical(field);
        self.rows.iter().any(|r| {
            r.record == record
                && r.field == name
                && (r.flows.contains(domain) || domain == "unknown" && r.flows.len() == 2)
        })
    }
    pub fn project(&self, record: &str, domain: &str, fields: &[Field]) -> Vec<Field> {
        let mut result = fields.to_vec();
        for f in &mut result {
            if !self.enabled(record, &f.name, domain) {
                f.status = "not_checked".into();
                f.issues.clear();
                continue;
            }
            if f.status == "not_checked" {
                fields::check_extracted(f);
            }
            let blank =
                f.effective.is_null() || f.effective.as_str().is_some_and(|s| s.trim().is_empty());
            if blank && f.status != "invalid" {
                f.status = "missing".into();
            }
            if f.status != "valid" {
                f.issues
                    .push("required by checks CSV; usable value missing or invalid".into());
            }
        }
        // Expand selectors using the actual axis/tracking members, including
        // members whose selected optional field was omitted from the payload.
        for row in self
            .rows
            .iter()
            .filter(|r| r.record == record && self.enabled(record, &r.field, domain))
        {
            let mut names = vec![row.field.clone()];
            while names.iter().any(|n| n.contains("[]")) {
                let mut expanded = BTreeSet::new();
                for name in names {
                    if let Some((prefix, suffix)) = name.split_once("[]") {
                        for f in fields {
                            if let Some(tail) = f.name.strip_prefix(&format!("{prefix}[")) {
                                if let Some((index, _)) = tail.split_once(']') {
                                    if index.bytes().all(|b| b.is_ascii_digit()) {
                                        expanded.insert(format!("{prefix}[{index}]{suffix}"));
                                    }
                                }
                            }
                        }
                    } else {
                        expanded.insert(name);
                    }
                }
                names = expanded.into_iter().collect();
            }
            for name in names {
                if !result.iter().any(|f| f.name == name) {
                    result.push(Field {name, kind:row.format.clone(), byte_start:0,byte_len:0,
                        presence:"omitted".into(),origin:"unresolved".into(),raw:Value::Null,effective:Value::Null,
                        status:"missing".into(),matches_default:None,inherited_from:None,
                        issues:vec!["required by checks CSV; field absent from this record".into()]});
                }
            }
        }
        result
    }
}
