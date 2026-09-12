//! First-definition defaults are scoped by the caller to a source, MIR run,
//! record type and test number. Overrides never mutate the first definition.
use super::*;

fn is_default(name: &str) -> bool {
    matches!(
        name,
        "OPT_FLAG"
            | "RES_SCAL"
            | "LLM_SCAL"
            | "HLM_SCAL"
            | "LO_LIMIT"
            | "HI_LIMIT"
            | "START_IN"
            | "INCR_IN"
            | "RTN_INDX"
            | "UNITS"
            | "UNITS_IN"
            | "C_RESFMT"
            | "C_LLMFMT"
            | "C_HLMFMT"
            | "LO_SPEC"
            | "HI_SPEC"
    )
}
pub fn definition_fields(fields: &[Field]) -> Vec<Field> {
    fields
        .iter()
        .filter(|f| is_default(&f.name))
        .cloned()
        .collect()
}
fn inherit(f: &mut Field, prior: Option<(&str, &[Field])>) {
    if let Some((source, p)) = prior.and_then(|(s, p)| field(p, &f.name).map(|p| (s, p))) {
        if matches!(p.status.as_str(), "valid" | "missing") {
            f.effective = p.effective.clone();
            f.status = p.status.clone();
            f.origin = "inherited".into();
            f.inherited_from = Some(format!("{source}:{}", f.name));
            return;
        }
    }
    f.effective = Value::Null;
    f.status = "unknown".into();
    f.origin = "unresolved".into();
}

/// Resolve only fields with specified PTR/MPR default semantics. Raw bytes,
/// presence and raw values never change. `prior` is the FIRST definition.
pub fn resolve_defaults(record: &str, fields: &mut [Field], prior: Option<(&str, &[Field])>) {
    if !matches!(record, "PTR" | "MPR") {
        return;
    }
    let raw_opt = number(fields, "OPT_FLAG");
    let opt = raw_opt.or_else(|| prior.and_then(|(_, p)| number(p, "OPT_FLAG")));
    let count = number(fields, "RTN_ICNT");
    let test_flag = number(fields, "TEST_FLG").unwrap_or(0);
    let parm_flag = number(fields, "PARM_FLG").unwrap_or(0);
    for f in fields {
        if matches!(f.name.as_str(), "RESULT" | "RTN_RSLT") {
            if test_flag & 0x3f != 0 || parm_flag & 7 != 0 {
                // A not-executed test is an encoded state, not malformed data.
                f.issues
                    .retain(|s| s != "TEST_FLG marks result unreliable/not executed");
                if f.issues.is_empty() {
                    f.status = "unknown".into();
                }
                f.effective = Value::Null;
            }
            continue;
        }
        if !is_default(&f.name)
            || !f.issues.is_empty()
            || matches!(f.presence.as_str(), "truncated" | "unread")
        {
            continue;
        }
        let omitted = f.presence == "omitted";
        if f.kind == "Cn" {
            if omitted || f.raw == json!("") {
                if prior.is_some() {
                    inherit(f, prior);
                }
            } else if f.raw == json!("\u{0}") {
                f.effective = json!("");
                f.status = "valid".into();
            }
            continue;
        }
        if f.name == "OPT_FLAG" {
            if record == "MPR" && omitted {
                inherit(f, prior);
            }
            continue;
        }
        let (default_mask, missing_mask) = match f.name.as_str() {
            "RES_SCAL" => (1, 0),
            "LLM_SCAL" | "LO_LIMIT" => (0x10, 0x40),
            "HLM_SCAL" | "HI_LIMIT" => (0x20, 0x80),
            "LO_SPEC" => (0, 4),
            "HI_SPEC" => (0, 8),
            "START_IN" | "INCR_IN" if record == "MPR" => (0, 2),
            _ => (0, 0),
        };
        // Explicit no-limit takes precedence over a request for default limits.
        if opt.is_some_and(|o| o & missing_mask != 0) {
            f.status = "missing".into();
            f.effective = Value::Null;
        } else if omitted || opt.is_some_and(|o| o & default_mask != 0) {
            inherit(f, prior);
        } else if opt.is_none() && (default_mask != 0 || missing_mask != 0) {
            f.status = "unknown".into();
            f.effective = Value::Null;
        } else {
            f.status = "valid".into();
            f.effective = f.raw.clone();
        }
        if f.name == "RTN_INDX"
            && f.status == "valid"
            && f.effective.as_array().map(|a| a.len() as u64) != count
        {
            f.status = "invalid".into();
            f.effective = Value::Null;
            f.issues
                .push("default RTN_INDX length differs from RTN_ICNT".into());
        }
        if matches!(f.name.as_str(), "LO_SPEC" | "HI_SPEC") && !omitted && f.status == "valid" {
            if let Some(p) = prior.and_then(|(_, p)| field(p, &f.name)) {
                if p.status == "valid" && p.effective != f.effective {
                    f.status = "invalid".into();
                    f.effective = Value::Null;
                    f.issues
                        .push("specification limit differs from first definition".into());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ptr(opt: u8, lo: f32, units: &[u8]) -> Vec<Field> {
        let mut b = vec![1, 0, 0, 0, 1, 1, 0, 0];
        b.extend(1.25f32.to_le_bytes());
        b.extend([0, 0, opt, 0, 0, 0]);
        b.extend(lo.to_le_bytes());
        b.extend(10f32.to_le_bytes());
        b.push(units.len() as u8);
        b.extend(units);
        inspect("PTR", &b, ByteOrder::LittleEndian)
    }
    #[test]
    fn numeric_requests_inherit_first_definition_and_no_limit_wins() {
        let mut first = ptr(2, 1.0, b"V");
        resolve_defaults("PTR", &mut first, None);
        let mut override_value = ptr(2, 2.0, b"mV");
        resolve_defaults("PTR", &mut override_value, Some(("source:10", &first)));
        assert_eq!(
            field(&override_value, "LO_LIMIT").unwrap().effective["value"],
            "2"
        );
        let mut next = ptr(0x12, 999.0, b"");
        resolve_defaults("PTR", &mut next, Some(("source:10", &first)));
        let f = field(&next, "LO_LIMIT").unwrap();
        assert_eq!(f.raw["value"], "999");
        assert_eq!(f.effective["value"], "1");
        assert_eq!(f.inherited_from.as_deref(), Some("source:10:LO_LIMIT"));
        assert_eq!(text(&next, "UNITS"), Some("V"));
        let mut missing = ptr(0x52, 999.0, b"V");
        resolve_defaults("PTR", &mut missing, Some(("source:10", &first)));
        assert_eq!(field(&missing, "LO_LIMIT").unwrap().status, "missing");
    }
    #[test]
    fn omitted_defaults_and_explicit_null_preserve_raw_presence() {
        let mut first = ptr(2, 1.0, b"V");
        resolve_defaults("PTR", &mut first, None);
        let mut short = inspect(
            "PTR",
            &[1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0],
            ByteOrder::LittleEndian,
        );
        resolve_defaults("PTR", &mut short, Some(("s:1", &first)));
        assert_eq!(field(&short, "LO_LIMIT").unwrap().presence, "omitted");
        assert_eq!(field(&short, "LO_LIMIT").unwrap().effective["value"], "1");
        let mut null = ptr(2, 1.0, b"\0");
        resolve_defaults("PTR", &mut null, Some(("s:1", &first)));
        assert_eq!(text(&null, "UNITS"), Some(""));
        assert_eq!(field(&null, "UNITS").unwrap().raw, json!("\0"));
        assert_eq!(field(&null, "UNITS").unwrap().origin, "explicit");
    }
    #[test]
    fn invalid_baseline_and_truncation_are_never_repaired_by_defaults() {
        let mut first = ptr(2, f32::NAN, b"V");
        resolve_defaults("PTR", &mut first, None);
        let mut next = ptr(0x12, 1.0, b"V");
        resolve_defaults("PTR", &mut next, Some(("s:1", &first)));
        assert_eq!(field(&next, "LO_LIMIT").unwrap().status, "unknown");
        let mut broken = inspect(
            "PTR",
            &[1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 1],
            ByteOrder::LittleEndian,
        );
        resolve_defaults("PTR", &mut broken, Some(("s:1", &first)));
        assert_eq!(field(&broken, "LO_LIMIT").unwrap().status, "invalid");
    }
    #[test]
    fn parm_flags_and_not_executed_results_are_not_valid_measurements() {
        for (test, parm) in [(1, 0), (0x10, 0), (0, 1), (0, 2), (0, 4)] {
            let mut b = vec![1, 0, 0, 0, 1, 1, test, parm];
            b.extend(1f32.to_le_bytes());
            let mut f = inspect("PTR", &b, ByteOrder::LittleEndian);
            resolve_defaults("PTR", &mut f, None);
            assert!(field(&f, "RESULT").unwrap().effective.is_null());
            assert_eq!(field(&f, "RESULT").unwrap().status, "unknown");
        }
    }

    fn mpr(count: u16, tail: bool, spec: f32) -> Vec<Field> {
        let mut b = vec![2, 0, 0, 0, 1, 1, 0, 0];
        b.extend(count.to_le_bytes());
        b.extend(1u16.to_le_bytes());
        b.extend(vec![0; (count as usize).div_ceil(2)]);
        b.extend(1f32.to_le_bytes());
        if tail {
            b.extend([0, 0, 0, 0, 0, 0]);
            for v in [0f32, 10.0, 2.0, 0.5] {
                b.extend(v.to_le_bytes());
            }
            for n in 0..count {
                b.extend((n + 10).to_le_bytes());
            }
            b.extend([1, b'V', 1, b'A', 0, 0, 0]);
            b.extend(spec.to_le_bytes());
            b.extend(10f32.to_le_bytes());
        }
        inspect("MPR", &b, ByteOrder::LittleEndian)
    }
    #[test]
    fn mpr_inherits_input_parameters_flags_and_count_checked_indices() {
        let mut first = mpr(1, true, 0.0);
        resolve_defaults("MPR", &mut first, None);
        let mut next = mpr(1, false, 0.0);
        resolve_defaults("MPR", &mut next, Some(("s:1", &first)));
        assert_eq!(number(&next, "OPT_FLAG"), Some(0));
        assert_eq!(field(&next, "START_IN").unwrap().effective["value"], "2");
        assert_eq!(field(&next, "INCR_IN").unwrap().effective["value"], "0.5");
        assert_eq!(field(&next, "RTN_INDX").unwrap().effective, json!([10]));
        let mut changed = mpr(2, false, 0.0);
        resolve_defaults("MPR", &mut changed, Some(("s:1", &first)));
        assert_eq!(field(&changed, "RTN_INDX").unwrap().status, "invalid");
    }
    #[test]
    fn later_specification_changes_are_rejected_and_first_definition_stays_valid() {
        let mut first = mpr(1, true, 0.0);
        resolve_defaults("MPR", &mut first, None);
        let mut changed = mpr(1, true, 1.0);
        resolve_defaults("MPR", &mut changed, Some(("s:1", &first)));
        assert_eq!(field(&changed, "LO_SPEC").unwrap().status, "invalid");
        let mut next = mpr(1, false, 0.0);
        resolve_defaults("MPR", &mut next, Some(("s:1", &first)));
        assert_eq!(field(&next, "LO_SPEC").unwrap().effective["value"], "0");
    }
}
