use super::*;
use stdf_arrow::identity::{wafer_key, CoordinateCandidates};
use stdf_io::StreamingRecordReader;
use stdf_validate::fields::{field, number, text};

type Site = (u8, u8);
struct Active {
    index: usize,
    coordinates: CoordinateCandidates,
}

fn selected(profile: &Profile, record: &str, fields: &[Field]) -> Vec<Field> {
    if let Some(names) = profile.important_fields.get(record) {
        return fields
            .iter()
            .filter(|f| names.contains(&f.name))
            .cloned()
            .collect();
    }
    let names=match record {
        "MIR"=>"LOT_ID PART_TYP JOB_NAM JOB_REV SBLOT_ID TEST_COD OPER_NAM FLOW_ID TST_TEMP SETUP_T START_T STAT_NUM MODE_COD RTST_COD NODE_NAM TSTR_TYP DATE_COD FACIL_ID FLOOR_ID PROC_ID USER_TXT",
        "PRR"=>"HEAD_NUM SITE_NUM PART_ID PART_FLG HARD_BIN SOFT_BIN X_COORD Y_COORD",
        "SDR"=>"HEAD_NUM SITE_GRP SITE_CNT SITE_NUM HAND_TYP HAND_ID CARD_TYP CARD_ID LOAD_TYP LOAD_ID DIB_TYP DIB_ID CONT_TYP CONT_ID EXTR_ID",
        _=>"",
    };
    fields
        .iter()
        .filter(|f| names.is_empty() || names.split_whitespace().any(|n| n == f.name))
        .cloned()
        .collect()
}
fn preview(record: &str, offset: usize, fields: &[Field]) -> Value {
    let name = match record {
        "PTR" => "RESULT",
        "MPR" => "RTN_RSLT",
        "DTR" => "TEXT_DAT",
        "ATER" => "ACTIVITY",
        "CTRR" => "CELL_RSLT",
        "CTSR" => "CHAR_NAM",
        "GDR" => "GEN_DATA",
        "FTR" => "TEST_FLG",
        _ => "",
    };
    let f = field(fields, name);
    let raw = f.map(|f| f.raw.clone()).unwrap_or(Value::Null);
    let (value, ordinal, count) = match record {
        "MPR" => (
            raw.get(0).cloned().unwrap_or(Value::Null),
            Some(0),
            raw.as_array().map(Vec::len),
        ),
        "GDR" => {
            let a = raw.as_array();
            let first = a.and_then(|a| a.iter().enumerate().find(|(_, v)| v["tag"] != 0));
            (
                first.map(|(_, v)| v.clone()).unwrap_or(Value::Null),
                first.map(|(i, _)| i),
                a.map(Vec::len),
            )
        }
        "FTR" => {
            let flag = number(fields, "TEST_FLG");
            (
                json!(flag.and_then(|f| (f & 0x7e == 0).then_some(f & 0x80 == 0))),
                None,
                None,
            )
        }
        _ => (raw, None, None),
    };
    let status = if f.is_some_and(|f| f.status == "not_checked") {
        "not_checked"
    } else if value.is_null() {
        if count == Some(0) {
            "empty"
        } else {
            "unknown"
        }
    } else {
        f.map(|f| f.status.as_str()).unwrap_or("unknown")
    };
    let preview_raw = if record == "FTR" {
        f.map(|f| f.raw.clone()).unwrap_or(Value::Null)
    } else {
        value.clone()
    };
    json!({"raw":preview_raw,"type":record,"offset":offset.to_string(),"test_num":number(fields,"TEST_NUM"),"test_name":text(fields,"TEST_TXT"),"units":field(fields,"UNITS"),"field":name,"value":value,"ordinal":ordinal,"element_count":count,"status":status,"origin":f.map(|f|f.origin.as_str()),"inherited_from":f.and_then(|f|f.inherited_from.clone())})
}
fn apply_profile(
    profile: &Profile,
    record: &str,
    fields: &[Field],
    report: &mut Report,
    budget: &mut Budget,
    source: &str,
    offset: usize,
) -> CliResult<()> {
    for (rule, f) in profile
        .rules
        .iter()
        .filter(|r| r.record == record)
        .flat_map(|rule| {
            fields
                .iter()
                .filter(move |f| checks::canonical(&f.name) == rule.field)
                .map(move |f| (rule, f))
        })
    {
        if f.status == "not_checked" {
            continue;
        }
        let v = &f.effective;
        let num = v.as_f64().or_else(|| {
            v.get("value")
                .and_then(Value::as_str)
                .and_then(|s| s.parse::<f64>().ok())
        });
        let invalid = rule.required && (v.is_null() || f.status != "valid")
            || rule
                .compiled_pattern
                .as_ref()
                .is_some_and(|p| v.as_str().is_none_or(|s| !p.is_match(s)))
            || rule.allowed.as_ref().is_some_and(|a| !a.contains(v))
            || rule
                .min
                .is_some_and(|min| num.is_none_or(|n| !n.is_finite() || n < min))
            || rule
                .max
                .is_some_and(|max| num.is_none_or(|n| !n.is_finite() || n > max));
        if invalid {
            issue(
                report,
                budget,
                source,
                offset,
                "error",
                "profile",
                format!(
                    "{record}.{} does not satisfy profile {}",
                    f.name, profile.id
                ),
            )?;
            let finding = report
                .findings
                .last_mut()
                .expect("just added profile finding");
            finding["record"] = json!(record);
            finding["field"] = json!(f.name);
            budget.charge(record.len() + rule.field.len() + 32)?;
        }
    }
    Ok(())
}

pub(super) fn scan(
    path: &Path,
    source: &str,
    profiles: &Profiles,
    checks: &checks::Checks,
    report: &mut Report,
    evidence: &mut storage::EvidenceWriter,
    budget: &mut Budget,
) -> CliResult<()> {
    let unresolved = Profile {
        version: 1,
        id: "unresolved".into(),
        domain: "unknown".into(),
        require_wafer: false,
        rules: Vec::new(),
        important_fields: BTreeMap::new(),
    };
    let mut profile = profiles.default_profile().unwrap_or(&unresolved);
    let mut reader = match StreamingRecordReader::new(File::open(path)?) {
        Ok(r) => r,
        Err(e) => {
            issue(report, budget, source, 0, "error", "framing", e.to_string())?;
            return Ok(());
        }
    };
    let order = reader.byte_order();
    let mut active: BTreeMap<Site, Active> = BTreeMap::new();
    let mut run: Option<usize> = None;
    let mut wafers: BTreeMap<Site, (Option<String>, String)> = BTreeMap::new();
    let mut sites: BTreeMap<Site, (u8, String)> = BTreeMap::new();
    let mut defaults: BTreeMap<(String, u64), (usize, Vec<Field>)> = BTreeMap::new();
    let mut sequence = 0u64;
    let mut setups: BTreeMap<(u64, usize), Vec<String>> = BTreeMap::new();
    let mut lot = String::new();
    let mut complete = true;
    let mut mirs = 0;
    let mut mrrs = 0;
    let mut bps = 0;
    let mut far_metadata: Option<Value> = None;
    loop {
        budget.check()?;
        let offset = reader.bytes_consumed();
        let e = match reader.next_event() {
            Ok(Some(e)) => e,
            Ok(None) => break,
            Err(e) => {
                issue(
                    report,
                    budget,
                    source,
                    offset,
                    "error",
                    "framing",
                    e.to_string(),
                )?;
                complete = false;
                break;
            }
        };
        let mut record = format!("{:?}", e.header.record_type()).to_uppercase();
        if let Ok(stdf_core::StdfRecord::Gdr(gdr)) = &e.decoded {
            if let Some(name) = gdr.custom_record_name() {
                record = name.into();
            }
        }
        let unchecked = fields::sanity_exempt(&record);
        if record == "FAR" && offset != 0 {
            issue(
                report,
                budget,
                source,
                offset,
                "error",
                "far_position",
                "FAR must be the first record, not repeated",
            )?;
        }
        *report.records.entry(record.clone()).or_default() += 1;
        let mut fields = if matches!(record.as_str(), "CTSR" | "CTRR") {
            match fields::characterization_fields(&e.body, order) {
                Ok(f) => f,
                Err(error) => {
                    issue(
                        report,
                        budget,
                        source,
                        offset,
                        "error",
                        "decode",
                        format!("{record}: {error}"),
                    )?;
                    fields::unchecked_gdr_fields(&e.body, order)
                }
            }
        } else {
            fields::inspect(&record, &e.body, order)
        };
        if let Ok(stdf_core::StdfRecord::Ater(ater)) = &e.decoded {
            if let Some(activity) = field(&fields, "ACTIVITY") {
                let mut bytes = activity.clone();
                bytes.name = "ACTIVITY_BYTES".into();
                bytes.kind = "Binary".into();
                bytes.byte_start += 1;
                bytes.byte_len = ater.activity_bytes.len();
                bytes.raw = json!(ater.activity_bytes);
                bytes.effective = bytes.raw.clone();
                fields.push(bytes);
            }
        }
        if record == "MIR" {
            profile = match profiles.select(source, offset, &fields) {
                Ok(p) => p,
                Err(message) => {
                    issue(
                        report,
                        budget,
                        source,
                        offset,
                        "error",
                        "run_profile",
                        message,
                    )?;
                    &unresolved
                }
            };
        }
        if serde_json::to_vec(&fields)?.len() > budget.max {
            return Err("single record exceeds report working budget".into());
        }
        if fields::layout(&record).is_none() && !unchecked {
            issue(
                report,
                budget,
                source,
                offset,
                "error",
                "unsupported_record",
                format!("{record}: raw bytes retained, field validation unavailable"),
            )?;
        }
        if let Err(error) = &e.decoded {
            issue(
                report,
                budget,
                source,
                offset,
                "error",
                "decode",
                format!("{record}: {error}"),
            )?;
        }
        // Definitions are immutable and scoped to this source and MIR run.
        let mut definition_only = false;
        if matches!(record.as_str(), "PTR" | "MPR") {
            if let Some(n) = number(&fields, "TEST_NUM") {
                let key = (record.clone(), n);
                let first = !defaults.contains_key(&key);
                let head_site = number(&fields, "HEAD_NUM")
                    .zip(number(&fields, "SITE_NUM"))
                    .map(|(h, s)| (h as u8, s as u8));
                definition_only = first
                    && run.is_some()
                    && number(&fields, "TEST_FLG").is_some_and(|f| f & 0x10 != 0)
                    && number(&fields, "PARM_FLG") == Some(0)
                    && head_site.is_some_and(|s| !active.contains_key(&s));
                if let Some((origin, prior)) = defaults.get(&key) {
                    fields::resolve_defaults(
                        &record,
                        &mut fields,
                        Some((&format!("{source}:{origin}"), prior)),
                    );
                } else {
                    fields::resolve_defaults(&record, &mut fields, None);
                    let retained = fields::definition_fields(&fields);
                    budget.charge(serde_json::to_vec(&retained)?.len() + 256)?;
                    defaults.insert(key, (offset, retained));
                }
            }
        }
        // Keep internal effective values for identity/default resolution unchanged.
        let checked_source = if unchecked && !matches!(record.as_str(), "CTSR" | "CTRR") {
            let mut decoded = fields.clone();
            for checked in fields::inspect_checked(&record, &e.body, order) {
                if let Some(f) = decoded.iter_mut().find(|f| f.name == checked.name) {
                    *f = checked;
                }
            }
            decoded
        } else {
            fields.clone()
        };
        let mut checked_fields = checks.project(&record, &profile.domain, &checked_source);
        for f in &mut checked_fields {
            if f.status == "not_checked" {
                if let Some(original) = field(&fields, &f.name) {
                    *f = original.clone();
                    f.status = "not_checked".into();
                    f.issues.clear();
                }
            }
        }
        let check_status = if checked_fields.iter().any(|f| f.status != "not_checked") {
            "checked"
        } else {
            "not_checked"
        };
        let structural = fields::inspect_structure(
            if matches!(record.as_str(), "CTSR" | "CTRR") {
                "GDR"
            } else {
                &record
            },
            &e.body,
            order,
        );
        for f in structural.iter().chain(checked_fields.iter()) {
            for message in &f.issues {
                issue(
                    report,
                    budget,
                    source,
                    offset,
                    "error",
                    "field",
                    format!(
                        "{record}.{} at byte {}: {message}",
                        f.name,
                        offset + 4 + f.byte_start
                    ),
                )?;
            }
        }
        apply_profile(
            profile,
            &record,
            &checked_fields,
            report,
            budget,
            source,
            offset,
        )?;
        if record == "FAR" && offset == 0 {
            far_metadata = Some(
                json!({"type":"FAR","offset":"0","fields":selected(profile,"FAR",&checked_fields)}),
            );
        }
        let mut wire_fields = checked_fields.clone();
        for f in &mut wire_fields {
            f.byte_start += 4;
        }
        for (name, kind, start, len, value) in [
            ("REC_LEN", "U2", 0, 2, e.header.rec_len as u64),
            ("REC_TYP", "U1", 2, 1, e.header.rec_typ as u64),
            ("REC_SUB", "U1", 3, 1, e.header.rec_sub as u64),
        ] {
            wire_fields.push(Field {
                name: name.into(),
                kind: kind.into(),
                byte_start: start,
                byte_len: len,
                presence: "present".into(),
                origin: "explicit".into(),
                raw: json!(value),
                effective: json!(value),
                status: "not_checked".into(),
                matches_default: None,
                inherited_from: None,
                issues: Vec::new(),
            });
        }
        evidence.append(source, offset, &record, &wire_fields)?;
        let value_ref = format!("{source}:{offset}");
        if record == "MIR" {
            mirs += 1;
            if run.is_some() || !active.is_empty() {
                issue(
                    report,
                    budget,
                    source,
                    offset,
                    "error",
                    "run_overlap",
                    "MIR before prior run/units closed",
                )?;
                active.clear();
            }
            defaults.clear();
            setups.clear();
            sites.clear();
            wafers.clear();
            lot = text(&fields, "LOT_ID").unwrap_or("").into();
            let id = value_ref.clone();
            budget.charge(512)?;
            if let Some(v) = &far_metadata {
                budget.charge(serde_json::to_vec(v)?.len() + 512)?;
            }
            report.runs.push(Run {
                id,
                source: source.into(),
                domain: profile.domain.clone(),
                profile_id: (profile.domain != "unknown").then(|| profile.id.clone()),
                profile_hash: (profile.domain != "unknown").then(|| {
                    format!(
                        "{:x}",
                        Sha256::digest(serde_json::to_vec(profile).expect("serializable profile"))
                    )
                }),
                metadata: far_metadata.clone().into_iter().collect(),
                unassigned: Vec::new(),
                closed: false,
            });
            run = Some(report.runs.len() - 1);
        }
        if matches!(
            record.as_str(),
            "MIR" | "SDR" | "WIR" | "WRR" | "WCR" | "MRR" | "PCR" | "SBR" | "HBR"
        ) {
            if let Some(i) = run {
                let v = json!({"type":record,"offset":offset.to_string(),"fields":selected(profile,&record,&checked_fields)});
                budget.charge(serde_json::to_vec(&v)?.len() + 512)?;
                report.runs[i].metadata.push(v);
            } else if record != "PCR" {
                issue(
                    report,
                    budget,
                    source,
                    offset,
                    "error",
                    "run_context",
                    format!("{record} outside MIR/MRR"),
                )?;
            }
        }
        if matches!(record.as_str(), "ATR" | "CDR" | "CTSR") {
            let v = json!({"source":source,"type":record,"offset":offset.to_string(),"fields":checked_fields,"sanity":check_status});
            budget.charge(serde_json::to_vec(&v)?.len() + 512)?;
            if let Some(i) = run {
                report.runs[i].metadata.push(v);
            } else {
                report.file_records.push(v);
            }
            if record == "CTSR" {
                if let Some(id) = text(&fields, "CHAR_ID").and_then(characterization_id) {
                    for a in active.values() {
                        budget.charge(128 + value_ref.len())?;
                        setups
                            .entry((id, a.index))
                            .or_default()
                            .push(value_ref.clone());
                    }
                }
            }
        }
        if record == "SDR" {
            if let (Some(head), Some(group), Some(nums)) = (
                number(&fields, "HEAD_NUM"),
                number(&fields, "SITE_GRP"),
                field(&fields, "SITE_NUM").and_then(|f| f.effective.as_array()),
            ) {
                for site in nums {
                    if let Some(s) = site.as_u64() {
                        sites.insert((head as u8, s as u8), (group as u8, value_ref.clone()));
                        budget.charge(256)?;
                    }
                }
            }
        }
        if record == "WIR" {
            if let Some(head) = number(&fields, "HEAD_NUM") {
                let group = number(&fields, "SITE_GRP").unwrap_or(255) as u8;
                if wafers
                    .insert(
                        (head as u8, group),
                        (
                            text(&fields, "WAFER_ID").map(str::to_owned),
                            value_ref.clone(),
                        ),
                    )
                    .is_some()
                {
                    issue(
                        report,
                        budget,
                        source,
                        offset,
                        "error",
                        "wafer_overlap",
                        "WIR replaces an unclosed wafer",
                    )?;
                }
                budget.charge(512)?;
            }
        }
        if record == "WRR" {
            if let (Some(h), Some(g)) = (number(&fields, "HEAD_NUM"), number(&fields, "SITE_GRP")) {
                if wafers.remove(&(h as u8, g as u8)).is_none() {
                    issue(
                        report,
                        budget,
                        source,
                        offset,
                        "error",
                        "wafer_pair",
                        "WRR without matching WIR",
                    )?;
                }
            }
        }
        let site = number(&fields, "HEAD_NUM")
            .zip(number(&fields, "SITE_NUM"))
            .map(|(h, s)| (h as u8, s as u8));
        if matches!(record.as_str(), "PIR" | "PRR") {
            if let (Some(site), Some(ri)) = (site, run) {
                if record == "PIR" || !active.contains_key(&site) {
                    if record == "PRR" || active.contains_key(&site) {
                        issue(
                            report,
                            budget,
                            source,
                            offset,
                            "error",
                            "part_pair",
                            format!("{record} unmatched at head/site {site:?}"),
                        )?;
                    }
                    if report.units.len() >= budget.args.max_units {
                        return Err("unit count limit exceeded".into());
                    }
                    sequence += 1;
                    let group = sites.get(&site).map(|s| s.0).unwrap_or(255);
                    let wafer = wafers
                        .get(&(site.0, group))
                        .or_else(|| wafers.get(&(site.0, 255)));
                    let mut context = vec![report.runs[ri].id.clone()];
                    if let Some((_, id)) = sites.get(&site) {
                        context.push(id.clone());
                    }
                    if let Some((_, id)) = wafer {
                        context.push(id.clone());
                    }
                    let wafer = wafer.and_then(|(w, _)| w.clone());
                    if profile.require_wafer && wafer.as_ref().is_none_or(|w| w.is_empty()) {
                        issue(
                            report,
                            budget,
                            source,
                            offset,
                            "error",
                            "cp_wafer",
                            "CP profile requires unambiguous wafer context",
                        )?;
                    }
                    budget.charge(1024)?;
                    report.units.push(Unit {
                        id: format!("{source}:unit:{sequence}"),
                        run: report.runs[ri].id.clone(),
                        head: site.0,
                        site: site.1,
                        sequence,
                        wafer,
                        context,
                        ..Default::default()
                    });
                    active.insert(
                        site,
                        Active {
                            index: report.units.len() - 1,
                            coordinates: CoordinateCandidates::default(),
                        },
                    );
                }
                if record == "PRR" {
                    let a = active.remove(&site).unwrap();
                    let unit = &mut report.units[a.index];
                    let x = field(&fields, "X_COORD")
                        .and_then(|f| f.effective.as_i64())
                        .and_then(|n| i16::try_from(n).ok());
                    let y = field(&fields, "Y_COORD")
                        .and_then(|f| f.effective.as_i64())
                        .and_then(|n| i16::try_from(n).ok());
                    unit.merge_key = wafer_key(unit.wafer.as_deref(), x, y)
                        .or_else(|| a.coordinates.lot_key(&lot));
                    unit.part_id = text(&fields, "PART_ID").map(str::to_owned);
                    unit.closed = true;
                    let flag = number(&fields, "PART_FLG");
                    let summary = json!({"fields":selected(profile,"PRR",&checked_fields),"offset":offset.to_string(),"x":x,"y":y,"part_flg":flag,"passed":flag.and_then(|f|(f&0x14==0).then_some(f&8==0)),"hard_bin":number(&fields,"HARD_BIN"),"soft_bin":number(&fields,"SOFT_BIN")});
                    budget.charge(serde_json::to_vec(&summary)?.len() + 256)?;
                    unit.prr = Some(summary);
                    if unit.merge_key.is_none() {
                        issue(
                            report,
                            budget,
                            source,
                            offset,
                            "warning",
                            "identity",
                            "Coordinate identity unresolved; unit remains independent",
                        )?;
                    }
                }
            } else {
                issue(
                    report,
                    budget,
                    source,
                    offset,
                    "error",
                    "part_context",
                    format!("{record} lacks valid head/site or run"),
                )?;
            }
        }
        if matches!(
            record.as_str(),
            "PTR" | "MPR" | "FTR" | "DTR" | "GDR" | "ATER" | "CTRR"
        ) {
            let target = if matches!(record.as_str(), "DTR" | "GDR") {
                if active.len() == 1 {
                    active.keys().next().copied()
                } else {
                    None
                }
            } else if record == "CTRR" {
                number(&fields, "HEAD_NUM")
                    .and_then(|h| u8::try_from(h).ok())
                    .zip(number(&fields, "SITE_NUM").and_then(|s| u8::try_from(s).ok()))
            } else {
                site
            };
            let mut p = preview(&record, offset, &checked_fields);
            if unchecked {
                p["head_num"] = json!(number(&fields, "HEAD_NUM"));
                p["site_num"] = json!(number(&fields, "SITE_NUM"));
                p["fields"] = json!(checked_fields);
                p["ownership"] = json!("unresolved");
                if record == "CTRR" {
                    let refs = number(&fields, "CHAR_ID_REF")
                        .zip(target.and_then(|s| active.get(&s)))
                        .and_then(|(id, a)| setups.get(&(id, a.index)));
                    p["setup_refs"] = json!(refs.cloned().unwrap_or_default());
                    p["setup_association"] = json!(match refs.map(Vec::len) {
                        Some(1) => "resolved",
                        Some(n) if n > 1 => "ambiguous",
                        _ => "unresolved",
                    });
                }
            }
            if definition_only {
                p["definition_only"] = json!(true);
            }
            if let Some(a) = target.and_then(|s| active.get_mut(&s)) {
                if record == "PTR" {
                    if let (Some(name), Some(v)) = (
                        text(&fields, "TEST_TXT"),
                        field(&fields, "RESULT")
                            .and_then(|f| f.effective.get("value"))
                            .and_then(Value::as_str)
                            .and_then(|v| v.parse::<f32>().ok()),
                    ) {
                        a.coordinates.observe(name, Some(v));
                    }
                }
                if unchecked {
                    p["ownership"] = json!("explicit_head_site");
                }
                let unit = &mut report.units[a.index];
                *unit.counts.entry(record.clone()).or_default() += 1;
                let values = unit.previews.entry(record.clone()).or_default();
                if values.len() < budget.args.preview_records_per_type {
                    budget.charge(serde_json::to_vec(&p)?.len() + 512)?;
                    values.push(p);
                }
            } else {
                if let Some(i) = run {
                    budget.charge(serde_json::to_vec(&p)?.len() + 256)?;
                    report.runs[i].unassigned.push(p);
                } else if unchecked {
                    budget.charge(serde_json::to_vec(&p)?.len() + 256)?;
                    report.file_records.push(json!({"source":source,"type":record,"offset":offset.to_string(),"fields":checked_fields,"sanity":check_status}));
                }
                let severity = if matches!(record.as_str(), "DTR" | "GDR") {
                    "warning"
                } else {
                    "error"
                };
                if !definition_only && !unchecked {
                    issue(
                        report,
                        budget,
                        source,
                        offset,
                        severity,
                        "record_ownership",
                        format!("{record} unit association unresolved; retained at run level"),
                    )?;
                }
            }
        }
        if record == "BPS" {
            bps += 1;
        }
        if record == "EPS" {
            if bps == 0 {
                issue(
                    report,
                    budget,
                    source,
                    offset,
                    "error",
                    "program_section",
                    "EPS without BPS",
                )?;
            } else {
                bps -= 1;
            }
        }
        if record == "MRR" {
            mrrs += 1;
            if !active.is_empty() {
                issue(
                    report,
                    budget,
                    source,
                    offset,
                    "error",
                    "unclosed_unit",
                    "MRR with unclosed unit(s)",
                )?;
                active.clear();
            }
            if let Some(i) = run.take() {
                report.runs[i].closed = true;
            }
        }
    }
    let offset = reader.bytes_consumed();
    if mirs == 0 || mrrs == 0 || run.is_some() {
        issue(
            report,
            budget,
            source,
            offset,
            "error",
            "run_closure",
            "missing MIR/MRR or unclosed run",
        )?;
    }
    if !active.is_empty() {
        issue(
            report,
            budget,
            source,
            offset,
            "error",
            "unclosed_unit",
            "EOF with unclosed unit(s)",
        )?;
    }
    if !wafers.is_empty() {
        issue(
            report,
            budget,
            source,
            offset,
            "error",
            "wafer_closure",
            "EOF with WIR lacking WRR",
        )?;
    }
    if bps != 0 {
        issue(
            report,
            budget,
            source,
            offset,
            "error",
            "program_section",
            "EOF with unclosed BPS",
        )?;
    }
    report
        .sources
        .iter_mut()
        .find(|s| s.id == source)
        .unwrap()
        .scan_complete = complete;
    Ok(())
}

// The vendor table uses C*n for CHAR_ID; examples display hexadecimal IDs.
fn characterization_id(value: &str) -> Option<u64> {
    value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .map(|v| u64::from_str_radix(v, 16).ok())
        .unwrap_or_else(|| value.parse().ok())
}
