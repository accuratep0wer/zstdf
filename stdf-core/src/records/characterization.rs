//! Advantest CTSR/CTRR views over ordinary GDR values.
//! GDR's count, V*n type tags and padding remain intact in the original record.
use super::Gdr;
use crate::{
    error::{Result, StdfError},
    types::VarData,
};

#[derive(Debug, Clone)]
pub struct NamedValue {
    /// Nested axis/tracking fields use zero-based paths, e.g. AXES[0].AXIS_ID.
    pub name: String,
    /// Index in the original GEN_DATA, including padding entries.
    pub index: usize,
    pub value: VarData,
}

#[derive(Debug, Clone)]
pub struct Characterization {
    pub record_name: &'static str,
    pub fields: Vec<NamedValue>,
}

impl Characterization {
    pub fn get(&self, name: &str) -> Option<&VarData> {
        self.fields
            .iter()
            .find(|f| f.name == name)
            .map(|f| &f.value)
    }
}

impl Gdr {
    /// Identify only the documented discriminator, never arbitrary text matches.
    pub fn custom_record_name(&self) -> Option<&'static str> {
        match self
            .gen_data
            .iter()
            .find(|v| !matches!(v, VarData::Padding))
        {
            Some(VarData::Cn(s)) if matches!(s.as_str(), "SHMOO" | "MARGIN") => Some("CTSR"),
            Some(VarData::Cn(s)) if matches!(s.as_str(), "SHMOO_RESULT" | "MARGIN_RESULT") => {
                Some("CTRR")
            }
            _ => None,
        }
    }

    /// Decode field structure only. No enum, range, pass/fail or axis sanity checks.
    pub fn characterization(&self) -> Result<Option<Characterization>> {
        let Some(record_name) = self.custom_record_name() else {
            return Ok(None);
        };
        let mut c = Cursor {
            record: record_name,
            data: &self.gen_data,
            pos: 0,
            fields: Vec::new(),
        };
        let kind = c.cn("REC_CUSTM")?;
        if record_name == "CTRR" {
            c.take("CHAR_ID_REF", "U4")?;
            c.take("HEAD_NUM", "U1")?;
            c.take("SITE_NUM", "U4")?;
            for name in ["TEST_NUM_REF", "CELL_COORD", "RSLT_TAGT_INS", "CELL_RSLT"] {
                c.cn(name)?;
            }
        } else {
            for name in ["CHAR_ID", "CHAR_NAM", "TAGT_INS", "RSLT_TITLE"] {
                c.cn(name)?;
            }
            let shmoo = kind == "SHMOO";
            c.cn(if shmoo { "EXE_ORDER" } else { "RSC_MOD" })?;
            c.cn("BYPASS_FLG")?;
            let axes = c.u1("AXIS_CNT")?;
            for a in 0..axes {
                let p = format!("AXES[{a}].");
                c.take(&(p.clone() + "AXIS_ID"), "U1")?;
                for name in [
                    "AXIS_NAM",
                    "SETUP_SIG",
                    "RSC_TYP",
                    "RSC_NAM",
                    "INIT_VAL",
                    "RNG_FMT",
                    "RNG_VAL",
                ] {
                    c.cn(&(p.clone() + name))?;
                }
                c.take(&(p.clone() + "RNG_RESO"), "R8")?;
                c.take(&(p.clone() + "RNG_STEP"), "U4")?;
                if shmoo {
                    c.take(&(p.clone() + "RNG_FSTEP"), "U4")?;
                    c.cn(&(p.clone() + "RNG_SCAL"))?;
                } else {
                    c.cn(&(p.clone() + "SIG_MOD"))?;
                }
                // The supplied shmoo example omits margin values; type tags
                // disambiguate an R8 value from the following U1 count.
                if matches!(c.peek(), Some(VarData::R8(_))) {
                    c.take(&(p.clone() + "MARGIN_VAL"), "R8")?;
                }
                let tracks = c.u1(&(p.clone() + "TRACK_CNT"))?;
                for t in 0..tracks {
                    let q = format!("{p}TRACKING[{t}].");
                    c.take(&(q.clone() + "TRACK_ID"), "U1")?;
                    for name in [
                        "TRACK_NAM",
                        "TRACK_SETUP_SIG",
                        "TRACK_RSC_TYP",
                        "TRACK_RSC_NAM",
                    ] {
                        c.cn(&(q.clone() + name))?;
                    }
                    // Table form has INIT_VAL/FMT/VAL; supplied example has FMT/VAL.
                    let strings = c.data[c.pos..]
                        .iter()
                        .filter(|v| !matches!(v, VarData::Padding))
                        .take_while(|v| matches!(v, VarData::Cn(_)))
                        .count();
                    if strings == 3 {
                        c.cn(&(q.clone() + "TRACK_INIT_VAL"))?;
                    } else if strings != 2 {
                        return Err(c.error("tracking parameter string layout is ambiguous"));
                    }
                    c.cn(&(q.clone() + "TRACK_RNG_FMT"))?;
                    c.cn(&(q.clone() + "TRACK_RNG_VAL"))?;
                    if matches!(c.peek(), Some(VarData::R8(_))) {
                        c.take(&(q + "TRACK_MARGIN_VAL"), "R8")?;
                    }
                }
            }
        }
        if c.peek().is_some() {
            return Err(c.error("unconsumed custom GDR values"));
        }
        Ok(Some(Characterization {
            record_name,
            fields: c.fields,
        }))
    }
}

struct Cursor<'a> {
    record: &'static str,
    data: &'a [VarData],
    pos: usize,
    fields: Vec<NamedValue>,
}
impl Cursor<'_> {
    fn error(&self, msg: &str) -> StdfError {
        StdfError::InvalidField {
            record: self.record,
            field: "GEN_DATA",
            msg: format!("value {}: {msg}", self.pos),
        }
    }
    fn peek(&mut self) -> Option<&VarData> {
        while matches!(self.data.get(self.pos), Some(VarData::Padding)) {
            self.pos += 1;
        }
        self.data.get(self.pos)
    }
    fn take(&mut self, name: &str, kind: &str) -> Result<VarData> {
        let v = self
            .peek()
            .cloned()
            .ok_or_else(|| self.error(&format!("missing {name}:{kind}")))?;
        if !matches!(
            (kind, &v),
            ("Cn", VarData::Cn(_))
                | ("U1", VarData::U1(_))
                | ("U4", VarData::U4(_))
                | ("R8", VarData::R8(_))
        ) {
            return Err(self.error(&format!("expected {name}:{kind}, got {v:?}")));
        }
        self.fields.push(NamedValue {
            name: name.into(),
            index: self.pos,
            value: v.clone(),
        });
        self.pos += 1;
        Ok(v)
    }
    fn cn(&mut self, name: &str) -> Result<String> {
        let VarData::Cn(v) = self.take(name, "Cn")? else {
            unreachable!()
        };
        Ok(v)
    }
    fn u1(&mut self, name: &str) -> Result<u8> {
        let VarData::U1(v) = self.take(name, "U1")? else {
            unreachable!()
        };
        Ok(v)
    }
}
