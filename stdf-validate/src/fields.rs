//! Wire-level field evidence. This supplements, rather than changes, the legacy decoder.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use stdf_core::{fields::FieldReader, ByteOrder};

mod defaults;
mod schema;
pub use defaults::{definition_fields, resolve_defaults};
pub use schema::layout;
pub const RULE_VERSION: &str = "sanity-fields-v2";

/// These records are extracted, but excluded from field and profile sanity checks.
pub fn sanity_exempt(record: &str) -> bool {
    matches!(record, "ATR" | "CDR" | "ATER" | "CTSR" | "CTRR")
}

/// Extract named characterization fields without semantic validation. Offsets
/// still address the original GDR body, including its count and V*n tags.
pub fn characterization_fields(body: &[u8], order: ByteOrder) -> Result<Vec<Field>, String> {
    let mut r = FieldReader::new(body, order);
    let gdr = stdf_core::records::Gdr::parse(&mut r).map_err(|e| e.to_string())?;
    if r.remaining() != 0 {
        return Err("unparsed bytes after GDR values".into());
    }
    let custom = gdr
        .characterization()
        .map_err(|e| e.to_string())?
        .ok_or("not a characterization GDR")?;
    let mut r = FieldReader::new(body, order);
    r.read_u2().map_err(|e| e.to_string())?;
    let mut wire = Vec::new();
    for _ in 0..gdr.fld_cnt {
        let start = r.position();
        let v = scalar(&mut r, "Vn")?;
        wire.push((start, r.position() - start, v));
    }
    let mut fields = unchecked_gdr_fields(body, order);
    fields.extend(custom.fields.into_iter().map(|f| {
        let (start, len, v) = &wire[f.index];
        Field {
            name: f.name,
            kind: v["kind"].as_str().unwrap().into(),
            byte_start: *start,
            byte_len: *len,
            presence: "present".into(),
            origin: "explicit".into(),
            raw: v["value"].clone(),
            effective: v["value"].clone(),
            status: "not_checked".into(),
            matches_default: None,
            inherited_from: None,
            issues: Vec::new(),
        }
    }));
    Ok(fields)
}

/// Raw typed evidence fallback when a recognized custom layout cannot be decoded.
pub fn unchecked_gdr_fields(body: &[u8], order: ByteOrder) -> Vec<Field> {
    inspect_impl("GDR", body, order, true)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub kind: String,
    pub byte_start: usize,
    pub byte_len: usize,
    pub presence: String,
    pub origin: String,
    pub raw: Value,
    pub effective: Value,
    pub status: String,
    pub matches_default: Option<bool>,
    pub inherited_from: Option<String>,
    pub issues: Vec<String>,
}

fn scalar(r: &mut FieldReader<'_>, kind: &str) -> Result<Value, String> {
    macro_rules! read {
        ($f:ident) => {
            json!(r.$f().map_err(|e| e.to_string())?)
        };
    }
    Ok(match kind {
        "U1" | "B1" => read!(read_u1),
        "U2" => read!(read_u2),
        "U4" => read!(read_u4),
        "I1" => read!(read_i1),
        "I2" => read!(read_i2),
        "I4" => read!(read_i4),
        "C1" => json!(char::from(r.read_u1().map_err(|e| e.to_string())?).to_string()),
        "Cn" => read!(read_cn),
        "Sn" => read!(read_sn),
        "Bn" => read!(read_bn),
        "Dn" => {
            let (bits, bytes) = r.read_dn().map_err(|e| e.to_string())?;
            json!({"bit_count":bits,"bytes":bytes})
        }
        "R4" => {
            let v = r.read_r4().map_err(|e| e.to_string())?;
            json!({"value":v.to_string(),"bits":format!("{:08x}",v.to_bits())})
        }
        "R8" => {
            let v = r.read_r8().map_err(|e| e.to_string())?;
            json!({"value":v.to_string(),"bits":format!("{:016x}",v.to_bits())})
        }
        "Vn" => {
            let tag = r.read_u1().map_err(|e| e.to_string())?;
            let kind = match tag {
                0 => "PAD",
                1 => "U1",
                2 => "U2",
                3 => "U4",
                4 => "I1",
                5 => "I2",
                6 => "I4",
                7 => "R4",
                8 => "R8",
                10 => "Cn",
                11 => "Bn",
                12 => "Dn",
                13 => "N1",
                _ => return Err(format!("unknown V*n tag {tag}")),
            };
            let value = if tag == 0 {
                Value::Null
            } else if tag == 13 {
                let n = r.read_u1().map_err(|e| e.to_string())?;
                if n > 15 {
                    return Err("nonzero nibble padding".into());
                }
                json!(n)
            } else {
                scalar(r, kind)?
            };
            json!({"tag":tag,"kind":kind,"value":value})
        }
        _ => return Err(format!("unsupported field type {kind}")),
    })
}

/// Field names/types are in wire order; `?` denotes an omittable trailing field.
/// All original bytes remain in the source evidence; offsets include length prefixes.
pub fn inspect(record: &str, body: &[u8], order: ByteOrder) -> Vec<Field> {
    inspect_impl(record, body, order, sanity_exempt(record))
}

/// Inspect even normally exempt records when explicitly selected by a caller.
pub fn inspect_checked(record: &str, body: &[u8], order: ByteOrder) -> Vec<Field> {
    inspect_impl(record, body, order, false)
}

/// Decode-only evidence for unconditional structural diagnostics.
pub fn inspect_structure(record: &str, body: &[u8], order: ByteOrder) -> Vec<Field> {
    inspect_impl(record, body, order, true)
}

/// Basic value checks for named fields extracted from custom GDR layouts.
pub fn check_extracted(f: &mut Field) {
    f.status = "valid".into();
    check_value(f);
}

fn inspect_impl(record: &str, body: &[u8], order: ByteOrder, unchecked: bool) -> Vec<Field> {
    let Some(spec) = layout(record) else {
        return Vec::new();
    };
    let mut r = FieldReader::new(body, order);
    let mut fields: Vec<Field> = Vec::new();
    let mut broken = false;
    for token in spec.split_whitespace() {
        let (name, t) = token.split_once(':').expect("static field descriptor");
        let optional = t.ends_with('?');
        let t = t.trim_end_matches('?');
        let (kind, count) = if let Some((kind, count_name)) = t.split_once('@') {
            (
                kind,
                fields
                    .iter()
                    .find(|f| f.name == count_name)
                    .and_then(|f| f.raw.as_u64())
                    .map(|n| n as usize),
            )
        } else {
            (t, None)
        };
        let array = t.contains('@');
        let start = r.position();
        let omitted = r.remaining() == 0 && optional && !(array && count == Some(0));
        let mut f = Field {
            name: name.into(),
            kind: t.into(),
            byte_start: start,
            byte_len: 0,
            presence: "present".into(),
            origin: "explicit".into(),
            raw: Value::Null,
            effective: Value::Null,
            status: "valid".into(),
            matches_default: None,
            inherited_from: None,
            issues: Vec::new(),
        };
        if broken || omitted {
            f.presence = if broken { "unread" } else { "omitted" }.into();
            f.origin = "unresolved".into();
            f.status = "unknown".into();
        } else {
            let value = if array {
                match count {
                    None => Err("array count missing".into()),
                    Some(n) if kind == "N1" => r
                        .read_nibble_array(n)
                        .map(|v| json!(v))
                        .map_err(|e| e.to_string()),
                    Some(n) => (0..n)
                        .map(|_| scalar(&mut r, kind))
                        .collect::<Result<Vec<_>, _>>()
                        .map(Value::Array),
                }
            } else {
                scalar(&mut r, kind)
            };
            match value {
                Ok(v) => {
                    f.raw = v.clone();
                    f.effective = v;
                }
                Err(e) => {
                    f.presence = if e.starts_with("unexpected EOF") {
                        "truncated"
                    } else {
                        "present"
                    }
                    .into();
                    f.status = "invalid".into();
                    f.issues.push(e);
                    broken = true;
                }
            }
            f.byte_len = if broken {
                body.len() - start
            } else {
                r.position() - start
            };
            if !unchecked && !broken && (kind == "Cn" || kind == "C1") {
                let bytes = &body[start + usize::from(kind == "Cn")..r.position()];
                if bytes.iter().any(|b| *b >= 128) {
                    f.status = "invalid".into();
                    f.issues
                        .push("non-ASCII character bytes; original bytes retained".into());
                    f.effective = Value::Null;
                }
            }
            if !unchecked
                && !broken
                && kind == "N1"
                && count.unwrap_or(0) % 2 == 1
                && body[r.position() - 1] & 0xf0 != 0
            {
                f.status = "invalid".into();
                f.issues.push("nonzero nibble padding".into());
            }
        }
        if unchecked {
            f.status = "not_checked".into();
            fields.push(f);
            continue;
        }
        let default = missing_default(record, name, kind);
        if let Some(d) = default {
            if f.presence == "omitted" {
                f.origin = "standard_default".into();
                f.status = "missing".into();
                f.matches_default = Some(true);
            } else if f.presence == "present" {
                f.matches_default = Some(f.raw == d);
                if f.raw == d && f.status == "valid" {
                    f.status = "missing".into();
                    f.effective = Value::Null;
                }
            }
        }
        check_value(&mut f);
        if matches!(
            f.name.as_str(),
            "HARD_BIN" | "SOFT_BIN" | "HBIN_NUM" | "SBIN_NUM"
        ) && f.status == "valid"
            && f.raw.as_u64().is_some_and(|n| n > 32767)
        {
            f.status = "invalid".into();
            f.effective = Value::Null;
            f.issues.push("bin number exceeds 32767".into());
        }
        fields.push(f);
    }
    if !broken && r.remaining() > 0 {
        fields.push(Field {
            name: "__TRAILING_BYTES".into(),
            kind: "Binary".into(),
            byte_start: r.position(),
            byte_len: r.remaining(),
            presence: "present".into(),
            origin: "explicit".into(),
            raw: Value::Null,
            effective: Value::Null,
            status: "invalid".into(),
            matches_default: None,
            inherited_from: None,
            issues: vec!["unparsed bytes after known record layout".into()],
        });
    }
    if unchecked {
        for f in &mut fields {
            f.status = "not_checked".into();
        }
        return fields;
    }
    let flag = number(&fields, "TEST_FLG").unwrap_or(0);
    let opt = number(&fields, "OPT_FLAG");
    for f in &mut fields {
        if matches!(f.name.as_str(), "RESULT" | "RTN_RSLT") && flag & 0x3f != 0 {
            f.status = "invalid".into();
            f.effective = Value::Null;
            f.issues
                .push("TEST_FLG marks result unreliable/not executed".into());
        }
        if matches!(record, "PTR" | "MPR") {
            let mask = match f.name.as_str() {
                "RES_SCAL" => 1,
                "LLM_SCAL" | "LO_LIMIT" => 0x50,
                "HLM_SCAL" | "HI_LIMIT" => 0xa0,
                "LO_SPEC" => 4,
                "HI_SPEC" => 8,
                _ => 0,
            };
            if mask != 0 {
                if opt.is_some_and(|v| v & mask != 0) {
                    f.status = "missing".into();
                    f.effective = Value::Null;
                } else if opt.is_none() && f.presence == "present" {
                    f.status = "unknown".into();
                    f.effective = Value::Null;
                }
            }
        }
    }
    fields
}

fn missing_default(record: &str, name: &str, kind: &str) -> Option<Value> {
    if kind == "Cn" {
        return Some(json!(""));
    }
    if kind == "C1" {
        return Some(json!(" "));
    }
    if matches!(name, "X_COORD" | "Y_COORD" | "CENTER_X" | "CENTER_Y") {
        return Some(json!(-32768));
    }
    if matches!(name, "SETUP_T" | "START_T" | "FINISH_T" | "MOD_TIM") {
        return Some(json!(0));
    }
    if name == "SOFT_BIN" || name == "BURN_TIM" {
        return Some(json!(65535));
    }
    if matches!(record, "PCR" | "WRR" | "TSR")
        && matches!(
            name,
            "RTST_CNT"
                | "ABRT_CNT"
                | "GOOD_CNT"
                | "FUNC_CNT"
                | "EXEC_CNT"
                | "FAIL_CNT"
                | "ALRM_CNT"
        )
    {
        return Some(json!(u32::MAX));
    }
    None
}

fn check_value(f: &mut Field) {
    fn invalid(v: &Value) -> bool {
        match v {
            Value::Array(v) => v.iter().any(invalid),
            Value::Object(m) => {
                m.get("bits").is_some()
                    && m.get("value")
                        .and_then(Value::as_str)
                        .and_then(|s| s.parse::<f64>().ok())
                        .is_some_and(|v| !v.is_finite())
                    || m.get("value").is_some_and(invalid)
                    || match (
                        m.get("bit_count").and_then(Value::as_u64),
                        m.get("bytes").and_then(Value::as_array),
                    ) {
                        (Some(n), Some(b)) if n % 8 != 0 => b
                            .last()
                            .and_then(Value::as_u64)
                            .is_some_and(|b| b >> (n % 8) != 0),
                        _ => false,
                    }
            }
            Value::String(s) => !s.is_ascii(),
            _ => false,
        }
    }
    if invalid(&f.raw) {
        f.status = "invalid".into();
        f.effective = Value::Null;
        f.issues
            .push("nonfinite number or nonzero bit padding".into());
    }
}
pub fn field<'a>(fields: &'a [Field], name: &str) -> Option<&'a Field> {
    fields.iter().find(|f| f.name == name)
}
pub fn number(fields: &[Field], name: &str) -> Option<u64> {
    field(fields, name)?.effective.as_u64()
}
pub fn text<'a>(fields: &'a [Field], name: &str) -> Option<&'a str> {
    field(fields, name)?.effective.as_str()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn partial_optional_integer_is_not_omitted() {
        let mut b = vec![1, 1, 0, 0, 0, 1, 0, 1, 0];
        b.push(12);
        let f = inspect("PRR", &b, ByteOrder::LittleEndian);
        assert_eq!(field(&f, "X_COORD").unwrap().status, "invalid");
        assert_eq!(field(&f, "Y_COORD").unwrap().presence, "unread");
    }
    #[test]
    fn explicit_empty_and_omitted_have_different_origins() {
        let a = inspect("BPS", &[], ByteOrder::LittleEndian);
        let b = inspect("BPS", &[0], ByteOrder::LittleEndian);
        assert_eq!(a[0].origin, "standard_default");
        assert_eq!(b[0].origin, "explicit");
        assert_eq!(b[0].status, "missing");
    }
    #[test]
    fn float_bits_and_invalid_later_array_element_survive() {
        let mut b = vec![1, 0, 0, 0, 1, 1, 0, 0, 0, 0, 2, 0];
        b.extend(1.25f32.to_le_bytes());
        b.extend(0x7fc00017u32.to_le_bytes());
        let f = inspect("MPR", &b, ByteOrder::LittleEndian);
        let v = field(&f, "RTN_RSLT").unwrap();
        assert_eq!(v.raw[0]["value"], "1.25");
        assert_eq!(v.raw[1]["bits"], "7fc00017");
        assert_eq!(v.status, "invalid");
    }
    #[test]
    fn gdr_r8_and_big_endian_are_lossless() {
        let mut b = vec![0, 1, 8];
        b.extend((-0.0f64).to_be_bytes());
        let f = inspect("GDR", &b, ByteOrder::BigEndian);
        assert_eq!(
            field(&f, "GEN_DATA").unwrap().raw[0]["value"]["bits"],
            "8000000000000000"
        );
    }

    #[test]
    fn long_ascii_string_length_is_not_an_encoding_error() {
        let mut b = vec![200];
        b.extend([b'A'; 200]);
        let f = inspect("DTR", &b, ByteOrder::LittleEndian);
        assert_eq!(f[0].status, "valid");
        assert_eq!(f[0].raw.as_str().unwrap().len(), 200);
    }

    #[test]
    fn opt_flag_invalid_limit_retains_raw_value() {
        let mut b = vec![1, 0, 0, 0, 1, 1, 0, 0];
        b.extend(1f32.to_le_bytes());
        b.extend([0, 0, 0x50, 0, 0, 0]);
        b.extend(2f32.to_le_bytes());
        let f = inspect("PTR", &b, ByteOrder::LittleEndian);
        let limit = field(&f, "LO_LIMIT").unwrap();
        assert_eq!(limit.raw["value"], "2");
        assert_eq!(limit.status, "missing");
        assert!(limit.effective.is_null());
    }

    #[test]
    fn invalid_bin_retains_raw_and_has_no_effective_value() {
        let f = inspect(
            "PRR",
            &[1, 1, 0, 0, 0, 255, 255, 1, 0],
            ByteOrder::LittleEndian,
        );
        let bin = field(&f, "HARD_BIN").unwrap();
        assert_eq!(bin.raw, 65535);
        assert_eq!(bin.status, "invalid");
        assert!(bin.effective.is_null());
    }
}
