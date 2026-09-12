use std::collections::HashMap;

pub mod fields;

use stdf_core::{raw_records, records::decode_record, StdfError, StdfRecord};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Fatal,
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone)]
pub struct Finding {
    pub severity: Severity,
    pub rule: &'static str,
    pub record_type: Option<&'static str>,
    pub file_offset: Option<u64>,
    pub message: String,
}

impl Finding {
    pub fn new(
        severity: Severity,
        rule: &'static str,
        record_type: Option<&'static str>,
        file_offset: Option<u64>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity,
            rule,
            record_type,
            file_offset,
            message: message.into(),
        }
    }
}

#[derive(Debug, Default)]
pub struct ValidationReport {
    pub findings: Vec<Finding>,
    pub decode_errors: Vec<String>,
}

impl ValidationReport {
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty() && self.decode_errors.is_empty()
    }

    pub fn has_severity(&self, severity: &Severity) -> bool {
        self.findings
            .iter()
            .any(|finding| &finding.severity == severity)
    }

    pub fn has_errors(&self) -> bool {
        !self.decode_errors.is_empty()
            || self.has_severity(&Severity::Fatal)
            || self.has_severity(&Severity::Error)
    }

    pub fn by_rule<'a>(&'a self, rule: &'static str) -> impl Iterator<Item = &'a Finding> {
        self.findings
            .iter()
            .filter(move |finding| finding.rule == rule)
    }

    pub fn likely_aborted(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| finding.rule.starts_with("ABORT"))
    }

    pub fn summary(&self) -> String {
        let count = |severity: &Severity| {
            self.findings
                .iter()
                .filter(|finding| &finding.severity == severity)
                .count()
        };
        format!(
            "{} fatal  {} error  {} warning  {} info  {} decode_error",
            count(&Severity::Fatal),
            count(&Severity::Error),
            count(&Severity::Warning),
            count(&Severity::Info),
            self.decode_errors.len()
        )
    }
}

#[derive(Debug, Default)]
pub struct ValidationContext {
    pub record_count: u64,
    pub current_offset: u64,
    pub far_seen: bool,
    pub mir_count: u32,
    pub mrr_seen: bool,
    pub open_wir_offsets: Vec<u64>,
    pub open_pir_offset: Option<u64>,
    pub bps_depth: u32,
    pub test_registry: HashMap<u32, (String, u64)>,
    pub hb_counts_prr: HashMap<u16, u64>,
    pub hb_counts_hbr: HashMap<(u8, u8, u16), u64>,
    pub mir_start_t: Option<u32>,
    pub part_count_prr: u64,
}

impl ValidationContext {
    pub fn update(&mut self, record: &StdfRecord, offset: u64) {
        self.record_count += 1;
        self.current_offset = offset;
        match record {
            StdfRecord::Far(_) => self.far_seen = true,
            StdfRecord::Mir(m) => {
                self.mir_count += 1;
                self.mir_start_t = Some(m.start_t);
            }
            StdfRecord::Mrr(_) => self.mrr_seen = true,
            StdfRecord::Wir(_) => self.open_wir_offsets.push(offset),
            StdfRecord::Wrr(_) => {
                self.open_wir_offsets.pop();
            }
            StdfRecord::Pir(_) => self.open_pir_offset = Some(offset),
            StdfRecord::Prr(p) => {
                self.open_pir_offset = None;
                self.part_count_prr += 1;
                *self.hb_counts_prr.entry(p.hard_bin).or_default() += 1;
            }
            StdfRecord::Hbr(h) => {
                *self
                    .hb_counts_hbr
                    .entry((h.head_num, h.site_num, h.hbin_num))
                    .or_default() += h.hbin_cnt as u64;
            }
            StdfRecord::Bps(_) => self.bps_depth += 1,
            StdfRecord::Eps(_) => self.bps_depth = self.bps_depth.saturating_sub(1),
            _ => {}
        }
    }
}

pub trait RecordRule: Send + Sync {
    fn name(&self) -> &'static str;

    fn on_record(
        &mut self,
        record: &StdfRecord,
        context: &ValidationContext,
        offset: u64,
    ) -> Vec<Finding>;

    fn on_eof(&mut self, _context: &ValidationContext) -> Vec<Finding> {
        Vec::new()
    }
}

pub struct ValidationEngine {
    rules: Vec<Box<dyn RecordRule>>,
    context: ValidationContext,
    report: ValidationReport,
}

impl ValidationEngine {
    pub fn new(rules: Vec<Box<dyn RecordRule>>) -> Self {
        Self {
            rules,
            context: ValidationContext::default(),
            report: ValidationReport::default(),
        }
    }

    pub fn feed(&mut self, record: &StdfRecord, offset: u64) {
        for rule in &mut self.rules {
            self.report
                .findings
                .extend(rule.on_record(record, &self.context, offset));
        }
        self.context.update(record, offset);
    }

    pub fn decode_error(&mut self, error: impl Into<String>) {
        self.report.decode_errors.push(error.into());
    }

    pub fn finish(mut self) -> ValidationReport {
        for rule in &mut self.rules {
            self.report.findings.extend(rule.on_eof(&self.context));
        }
        self.report
    }
}

pub fn full_engine() -> ValidationEngine {
    ValidationEngine::new(vec![
        Box::<PirPrrPairRule>::default(),
        Box::<WirWrrPairRule>::default(),
        Box::<BpsEpsBalanceRule>::default(),
        Box::<MirSingletonRule>::default(),
        Box::<AbortDetectionRule>::default(),
        Box::<PtrLimitOrderRule>::default(),
        Box::<PtrResultRule>::default(),
        Box::<MirLotIdRule>::default(),
        Box::<TestNameConsistencyRule>::default(),
        Box::<HbrBinCountRule>::default(),
    ])
}

pub fn validate_bytes(data: &[u8]) -> Result<ValidationReport, StdfError> {
    let mut engine = full_engine();
    let iter = raw_records(data)?;
    let byte_order = stdf_core::detect_byte_order_result(data)?;

    for item in iter {
        match item {
            Ok(raw) => {
                let record =
                    decode_record(&raw.header, raw.body, byte_order).unwrap_or_else(|_| {
                        StdfRecord::Unknown {
                            typ: raw.header.rec_typ,
                            sub: raw.header.rec_sub,
                            data: raw.body.to_vec(),
                        }
                    });
                engine.feed(&record, raw.offset as u64);
            }
            Err(error) => {
                engine.decode_error(error.to_string());
                break;
            }
        }
    }

    Ok(engine.finish())
}

#[derive(Default)]
pub struct AbortDetectionRule {
    mrr_seen: bool,
    open_wir_count: u32,
    open_pir: HashMap<(u8, u8), u64>,
    bps_depth: u32,
    last_offset: u64,
}

impl RecordRule for AbortDetectionRule {
    fn name(&self) -> &'static str {
        "ABORT"
    }

    fn on_record(
        &mut self,
        record: &StdfRecord,
        _ctx: &ValidationContext,
        offset: u64,
    ) -> Vec<Finding> {
        self.last_offset = offset;
        match record {
            StdfRecord::Mrr(_) => self.mrr_seen = true,
            StdfRecord::Wir(_) => self.open_wir_count += 1,
            StdfRecord::Wrr(_) => self.open_wir_count = self.open_wir_count.saturating_sub(1),
            StdfRecord::Pir(p) => {
                self.open_pir.insert((p.head_num, p.site_num), offset);
            }
            StdfRecord::Prr(p) => {
                self.open_pir.remove(&(p.head_num, p.site_num));
            }
            StdfRecord::Bps(_) => self.bps_depth += 1,
            StdfRecord::Eps(_) => self.bps_depth = self.bps_depth.saturating_sub(1),
            _ => {}
        }
        Vec::new()
    }

    fn on_eof(&mut self, _ctx: &ValidationContext) -> Vec<Finding> {
        let mut out = Vec::new();
        if !self.mrr_seen {
            out.push(Finding::new(
                Severity::Warning,
                "ABORT/NO_MRR",
                None,
                None,
                "No MRR found; writer likely aborted and bin/lot summary may be incomplete.",
            ));
        }
        if self.open_wir_count > 0 {
            out.push(Finding::new(
                Severity::Error,
                "ABORT/OPEN_WIR",
                Some("WIR"),
                Some(self.last_offset),
                format!(
                    "{} WIR(s) unclosed by WRR; aborted mid-wafer",
                    self.open_wir_count
                ),
            ));
        }
        let mut leftovers: Vec<_> = self.open_pir.iter().collect();
        leftovers.sort_by_key(|&(_, &offset)| offset);
        for (&(head, site), &offset) in leftovers {
            out.push(Finding::new(
                Severity::Error,
                "ABORT/OPEN_PIR",
                Some("PIR"),
                Some(offset),
                format!(
                    "PIR for head={head} site={site} never closed by PRR; aborted during part test"
                ),
            ));
        }
        if self.bps_depth > 0 {
            out.push(Finding::new(
                Severity::Warning,
                "ABORT/OPEN_BPS",
                Some("BPS"),
                None,
                format!("{} BPS section(s) not closed by EPS", self.bps_depth),
            ));
        }
        out
    }
}

#[derive(Default)]
pub struct PtrLimitOrderRule;

impl RecordRule for PtrLimitOrderRule {
    fn name(&self) -> &'static str {
        "FIELD/PTR_LIMIT_ORDER"
    }

    fn on_record(
        &mut self,
        record: &StdfRecord,
        _ctx: &ValidationContext,
        offset: u64,
    ) -> Vec<Finding> {
        if let StdfRecord::Ptr(p) = record {
            if let (Some(lo), Some(hi)) = (p.lo_limit, p.hi_limit) {
                if !ptr_lo_limit_invalid(p.opt_flag) && !ptr_hi_limit_invalid(p.opt_flag) && lo > hi
                {
                    return vec![Finding::new(
                        Severity::Error,
                        self.name(),
                        Some("PTR"),
                        Some(offset),
                        format!("test {} lo_limit {lo} > hi_limit {hi}", p.test_num),
                    )];
                }
            }
        }
        Vec::new()
    }
}

#[derive(Default)]
pub struct PtrResultRule;

impl RecordRule for PtrResultRule {
    fn name(&self) -> &'static str {
        "FIELD/PTR_RESULT_PRESENT"
    }

    fn on_record(
        &mut self,
        _record: &StdfRecord,
        _ctx: &ValidationContext,
        _offset: u64,
    ) -> Vec<Finding> {
        Vec::new()
    }
}

#[derive(Default)]
pub struct MirLotIdRule;

impl RecordRule for MirLotIdRule {
    fn name(&self) -> &'static str {
        "FIELD/MIR_LOT_ID"
    }

    fn on_record(
        &mut self,
        record: &StdfRecord,
        _ctx: &ValidationContext,
        offset: u64,
    ) -> Vec<Finding> {
        if let StdfRecord::Mir(m) = record {
            if m.lot_id.trim().is_empty() {
                return vec![Finding::new(
                    Severity::Warning,
                    self.name(),
                    Some("MIR"),
                    Some(offset),
                    "MIR.LOT_ID is empty",
                )];
            }
        }
        Vec::new()
    }
}

#[derive(Default)]
pub struct TestNameConsistencyRule {
    seen: HashMap<u32, (String, u64)>,
}

impl RecordRule for TestNameConsistencyRule {
    fn name(&self) -> &'static str {
        "SEMANTIC/TEST_NAME_CONSISTENCY"
    }

    fn on_record(
        &mut self,
        record: &StdfRecord,
        _ctx: &ValidationContext,
        offset: u64,
    ) -> Vec<Finding> {
        if let StdfRecord::Ptr(p) = record {
            let name = p.test_txt.clone().unwrap_or_default();
            if let Some((prev_name, prev_offset)) = self.seen.get(&p.test_num) {
                if *prev_name != name {
                    return vec![Finding::new(
                        Severity::Warning,
                        self.name(),
                        Some("PTR"),
                        Some(offset),
                        format!(
                            "test_num {} has name {:?} here (prev {:?} at {prev_offset:#010x})",
                            p.test_num, name, prev_name
                        ),
                    )];
                }
            } else {
                self.seen.insert(p.test_num, (name, offset));
            }
        }
        Vec::new()
    }
}

#[derive(Default)]
pub struct HbrBinCountRule;

impl RecordRule for HbrBinCountRule {
    fn name(&self) -> &'static str {
        "SEMANTIC/HBR_BIN_COUNT"
    }

    fn on_record(
        &mut self,
        _record: &StdfRecord,
        _ctx: &ValidationContext,
        _offset: u64,
    ) -> Vec<Finding> {
        Vec::new()
    }

    fn on_eof(&mut self, ctx: &ValidationContext) -> Vec<Finding> {
        let mut out = Vec::new();
        let mut granular_by_bin: HashMap<u16, u64> = HashMap::new();
        let mut aggregate_by_bin: HashMap<u16, u64> = HashMap::new();

        for (&(head, site, bin), &count) in &ctx.hb_counts_hbr {
            if head == 255 || site == 255 {
                let entry = aggregate_by_bin.entry(bin).or_default();
                *entry = (*entry).max(count);
            } else {
                *granular_by_bin.entry(bin).or_default() += count;
            }
        }

        let mut hbr_by_bin: HashMap<u16, u64> = HashMap::new();
        for &bin in granular_by_bin.keys().chain(aggregate_by_bin.keys()) {
            hbr_by_bin.entry(bin).or_insert_with(|| {
                granular_by_bin
                    .get(&bin)
                    .copied()
                    .unwrap_or_else(|| aggregate_by_bin.get(&bin).copied().unwrap_or(0))
            });
        }

        for (&bin, &hbr_count) in &hbr_by_bin {
            let prr_count = ctx.hb_counts_prr.get(&bin).copied().unwrap_or(0);
            if hbr_count != prr_count {
                out.push(Finding::new(
                    Severity::Warning,
                    self.name(),
                    Some("HBR"),
                    None,
                    format!("bin {bin} HBR says {hbr_count} but PRR count is {prr_count}"),
                ));
            }
        }

        for (&bin, &prr_count) in &ctx.hb_counts_prr {
            if !hbr_by_bin.contains_key(&bin) {
                out.push(Finding::new(
                    Severity::Warning,
                    self.name(),
                    Some("HBR"),
                    None,
                    format!("bin {bin} appears in PRRs ({prr_count} times) but has no HBR record"),
                ));
            }
        }
        out
    }
}

#[derive(Default)]
pub struct PirPrrPairRule {
    open: HashMap<(u8, u8), u64>,
}

impl RecordRule for PirPrrPairRule {
    fn name(&self) -> &'static str {
        "STRUCT/PIR_PRR_PAIR"
    }

    fn on_record(
        &mut self,
        record: &StdfRecord,
        _ctx: &ValidationContext,
        offset: u64,
    ) -> Vec<Finding> {
        match record {
            StdfRecord::Pir(p) => {
                let key = (p.head_num, p.site_num);
                if let Some(previous) = self.open.insert(key, offset) {
                    return vec![Finding::new(
                        Severity::Error,
                        self.name(),
                        Some("PIR"),
                        Some(offset),
                        format!(
                            "PIR at {offset:#010x} for head={} site={} opened while previous PIR at {previous:#010x} for the same site never closed",
                            p.head_num, p.site_num
                        ),
                    )];
                }
            }
            StdfRecord::Prr(p) => {
                let key = (p.head_num, p.site_num);
                if self.open.remove(&key).is_none() {
                    return vec![Finding::new(
                        Severity::Error,
                        self.name(),
                        Some("PRR"),
                        Some(offset),
                        format!(
                            "PRR at {offset:#010x} for head={} site={} has no matching PIR",
                            p.head_num, p.site_num
                        ),
                    )];
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn on_eof(&mut self, _ctx: &ValidationContext) -> Vec<Finding> {
        let mut leftovers: Vec<_> = self.open.iter().collect();
        leftovers.sort_by_key(|&(_, &offset)| offset);
        leftovers
            .into_iter()
            .map(|(&(head, site), &offset)| {
                Finding::new(
                    Severity::Error,
                    self.name(),
                    Some("PIR"),
                    Some(offset),
                    format!("PIR at {offset:#010x} for head={head} site={site} never closed; file likely truncated mid-part"),
                )
            })
            .collect()
    }
}

#[derive(Default)]
pub struct WirWrrPairRule {
    open: HashMap<(u8, u8), u64>,
}

impl RecordRule for WirWrrPairRule {
    fn name(&self) -> &'static str {
        "STRUCT/WIR_WRR_PAIR"
    }

    fn on_record(
        &mut self,
        record: &StdfRecord,
        _ctx: &ValidationContext,
        offset: u64,
    ) -> Vec<Finding> {
        match record {
            StdfRecord::Wir(w) => {
                let key = (w.head_num, w.site_grp.unwrap_or(255));
                if let Some(previous) = self.open.insert(key, offset) {
                    return vec![Finding::new(
                        Severity::Error,
                        self.name(),
                        Some("WIR"),
                        Some(offset),
                        format!(
                            "WIR at {offset:#010x} for head={} site_grp={} opened while previous WIR at {previous:#010x} for the same head/site_grp never closed",
                            w.head_num, key.1
                        ),
                    )];
                }
            }
            StdfRecord::Wrr(w) => {
                let key = (w.head_num, w.site_grp);
                if self.open.remove(&key).is_none() {
                    return vec![Finding::new(
                        Severity::Error,
                        self.name(),
                        Some("WRR"),
                        Some(offset),
                        format!(
                            "WRR at {offset:#010x} for head={} site_grp={} has no matching WIR",
                            w.head_num, w.site_grp
                        ),
                    )];
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn on_eof(&mut self, _ctx: &ValidationContext) -> Vec<Finding> {
        let mut leftovers: Vec<_> = self.open.iter().collect();
        leftovers.sort_by_key(|&(_, &offset)| offset);
        leftovers
            .into_iter()
            .map(|(&(head, site_grp), &offset)| {
                Finding::new(
                    Severity::Error,
                    self.name(),
                    Some("WIR"),
                    Some(offset),
                    format!("WIR at {offset:#010x} for head={head} site_grp={site_grp} never closed by WRR"),
                )
            })
            .collect()
    }
}

#[derive(Default)]
pub struct BpsEpsBalanceRule {
    depth: u32,
    first_open: Option<u64>,
}

impl RecordRule for BpsEpsBalanceRule {
    fn name(&self) -> &'static str {
        "STRUCT/BPS_EPS_BALANCE"
    }

    fn on_record(
        &mut self,
        record: &StdfRecord,
        _ctx: &ValidationContext,
        offset: u64,
    ) -> Vec<Finding> {
        match record {
            StdfRecord::Bps(_) => {
                self.depth += 1;
                self.first_open.get_or_insert(offset);
            }
            StdfRecord::Eps(_) => {
                if self.depth == 0 {
                    return vec![Finding::new(
                        Severity::Error,
                        self.name(),
                        Some("EPS"),
                        Some(offset),
                        "EPS without matching BPS",
                    )];
                }
                self.depth -= 1;
                if self.depth == 0 {
                    self.first_open = None;
                }
            }
            _ => {}
        }
        Vec::new()
    }

    fn on_eof(&mut self, _ctx: &ValidationContext) -> Vec<Finding> {
        if self.depth > 0 {
            return vec![Finding::new(
                Severity::Error,
                self.name(),
                Some("BPS"),
                self.first_open,
                format!("{} BPS record(s) never closed by EPS", self.depth),
            )];
        }
        Vec::new()
    }
}

#[derive(Default)]
pub struct MirSingletonRule {
    count: u32,
}

impl RecordRule for MirSingletonRule {
    fn name(&self) -> &'static str {
        "STRUCT/MIR_SINGLETON"
    }

    fn on_record(
        &mut self,
        record: &StdfRecord,
        _ctx: &ValidationContext,
        offset: u64,
    ) -> Vec<Finding> {
        if matches!(record, StdfRecord::Mir(_)) {
            self.count += 1;
            if self.count > 1 {
                return vec![Finding::new(
                    Severity::Error,
                    self.name(),
                    Some("MIR"),
                    Some(offset),
                    format!("MIR seen {} times; spec requires exactly one", self.count),
                )];
            }
        }
        Vec::new()
    }
}

fn ptr_lo_limit_invalid(opt_flag: Option<u8>) -> bool {
    opt_flag.map(|flag| flag & 0x50 != 0).unwrap_or(true)
}

fn ptr_hi_limit_invalid(opt_flag: Option<u8>) -> bool {
    opt_flag.map(|flag| flag & 0xA0 != 0).unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stdf_core::records::*;

    fn feed(records: Vec<StdfRecord>) -> ValidationReport {
        let mut engine = full_engine();
        for (idx, record) in records.iter().enumerate() {
            engine.feed(record, idx as u64 * 10);
        }
        engine.finish()
    }

    fn ptr(test_num: u32, name: &str, lo: f32, hi: f32) -> StdfRecord {
        StdfRecord::Ptr(Ptr {
            test_num,
            head_num: 1,
            site_num: 0,
            test_flg: 0,
            parm_flg: 0,
            result: 1.0,
            test_txt: Some(name.to_string()),
            alarm_id: None,
            opt_flag: Some(0),
            res_scal: Some(0),
            llm_scal: Some(0),
            hlm_scal: Some(0),
            lo_limit: Some(lo),
            hi_limit: Some(hi),
            units: None,
            c_resfmt: None,
            c_llmfmt: None,
            c_hlmfmt: None,
            lo_spec: None,
            hi_spec: None,
        })
    }

    fn prr(head: u8, site: u8, hbin: u16) -> StdfRecord {
        StdfRecord::Prr(Prr {
            head_num: head,
            site_num: site,
            part_flg: 0,
            num_test: 1,
            hard_bin: hbin,
            soft_bin: hbin,
            x_coord: None,
            y_coord: None,
            test_t: None,
            part_id: None,
            part_txt: None,
            part_fix: None,
        })
    }

    fn hbr(head: u8, site: u8, hbin: u16, count: u32) -> StdfRecord {
        StdfRecord::Hbr(Hbr {
            head_num: head,
            site_num: site,
            hbin_num: hbin,
            hbin_cnt: count,
            hbin_pf: Some(b'P'),
            hbin_nam: None,
        })
    }

    fn mir(lot_id: &str) -> StdfRecord {
        StdfRecord::Mir(Mir {
            setup_t: 0,
            start_t: 1,
            stat_num: 1,
            mode_cod: b' ',
            rtst_cod: b' ',
            prot_cod: b' ',
            burn_tim: 0,
            cmod_cod: b' ',
            lot_id: lot_id.to_string(),
            part_typ: String::new(),
            node_nam: String::new(),
            tstr_typ: String::new(),
            job_nam: String::new(),
            job_rev: None,
            sblot_id: None,
            oper_nam: None,
            exec_typ: None,
            exec_ver: None,
            test_cod: None,
            tst_temp: None,
            user_txt: None,
            aux_file: None,
            pkg_typ: None,
            famly_id: None,
            date_cod: None,
            facil_id: None,
            floor_id: None,
            proc_id: None,
            oper_frq: None,
            spec_nam: None,
            spec_ver: None,
            flow_id: None,
            setup_id: None,
            dsgn_rev: None,
            eng_id: None,
            rom_cod: None,
            serl_num: None,
            supr_nam: None,
        })
    }

    #[test]
    fn clean_multisite_file_has_no_pairing_findings() {
        let report = feed(vec![
            mir("LOT"),
            StdfRecord::Wir(Wir {
                head_num: 1,
                site_grp: Some(255),
                start_t: Some(0),
                wafer_id: None,
            }),
            StdfRecord::Pir(Pir {
                head_num: 1,
                site_num: 1,
            }),
            StdfRecord::Pir(Pir {
                head_num: 1,
                site_num: 2,
            }),
            prr(1, 1, 1),
            prr(1, 2, 1),
            StdfRecord::Wrr(Wrr {
                head_num: 1,
                site_grp: 255,
                finish_t: 1,
                part_cnt: 2,
                rtst_cnt: None,
                abrt_cnt: None,
                good_cnt: None,
                func_cnt: None,
                wafer_id: None,
                fabwf_id: None,
                frame_id: None,
                mask_id: None,
                usr_desc: None,
                exc_desc: None,
            }),
            hbr(1, 1, 1, 1),
            hbr(1, 2, 1, 1),
            StdfRecord::Mrr(Mrr {
                finish_t: 2,
                disp_cod: None,
                usr_desc: None,
                exc_desc: None,
            }),
        ]);
        assert_eq!(report.by_rule("STRUCT/PIR_PRR_PAIR").count(), 0);
        assert_eq!(report.by_rule("SEMANTIC/HBR_BIN_COUNT").count(), 0);
    }

    #[test]
    fn detects_pir_without_prr() {
        let report = feed(vec![StdfRecord::Pir(Pir {
            head_num: 1,
            site_num: 2,
        })]);
        assert_eq!(report.by_rule("STRUCT/PIR_PRR_PAIR").count(), 1);
        assert_eq!(report.by_rule("ABORT/OPEN_PIR").count(), 1);
    }

    #[test]
    fn detects_prr_without_pir() {
        let report = feed(vec![prr(1, 1, 1)]);
        assert_eq!(report.by_rule("STRUCT/PIR_PRR_PAIR").count(), 1);
    }

    #[test]
    fn wrr_does_not_close_different_head_wir() {
        let report = feed(vec![
            StdfRecord::Wir(Wir {
                head_num: 1,
                site_grp: Some(255),
                start_t: None,
                wafer_id: None,
            }),
            StdfRecord::Wir(Wir {
                head_num: 2,
                site_grp: Some(255),
                start_t: None,
                wafer_id: None,
            }),
            StdfRecord::Wrr(Wrr {
                head_num: 1,
                site_grp: 255,
                finish_t: 1,
                part_cnt: 0,
                rtst_cnt: None,
                abrt_cnt: None,
                good_cnt: None,
                func_cnt: None,
                wafer_id: None,
                fabwf_id: None,
                frame_id: None,
                mask_id: None,
                usr_desc: None,
                exc_desc: None,
            }),
        ]);
        let findings: Vec<_> = report.by_rule("STRUCT/WIR_WRR_PAIR").collect();
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("head=2"));
    }

    #[test]
    fn detects_bps_eps_imbalance() {
        let report = feed(vec![StdfRecord::Bps(Bps { seq_name: None })]);
        assert_eq!(report.by_rule("STRUCT/BPS_EPS_BALANCE").count(), 1);
    }

    #[test]
    fn detects_duplicate_mir() {
        let report = feed(vec![mir("A"), mir("B")]);
        assert_eq!(report.by_rule("STRUCT/MIR_SINGLETON").count(), 1);
    }

    #[test]
    fn detects_missing_mrr_as_abort_warning() {
        let report = feed(vec![mir("A")]);
        assert!(report.likely_aborted());
        assert_eq!(report.by_rule("ABORT/NO_MRR").count(), 1);
    }

    #[test]
    fn detects_inverted_ptr_limits() {
        let report = feed(vec![ptr(100, "INV", 5.0, 1.0)]);
        assert_eq!(report.by_rule("FIELD/PTR_LIMIT_ORDER").count(), 1);
    }

    #[test]
    fn detects_empty_lot_id() {
        let report = feed(vec![mir("")]);
        assert_eq!(report.by_rule("FIELD/MIR_LOT_ID").count(), 1);
    }

    #[test]
    fn detects_test_name_mismatch() {
        let report = feed(vec![ptr(500, "A", 0.0, 1.0), ptr(500, "B", 0.0, 1.0)]);
        assert_eq!(report.by_rule("SEMANTIC/TEST_NAME_CONSISTENCY").count(), 1);
    }

    #[test]
    fn detects_hbr_count_mismatch() {
        let report = feed(vec![prr(1, 1, 1), hbr(255, 255, 1, 9)]);
        assert_eq!(report.by_rule("SEMANTIC/HBR_BIN_COUNT").count(), 1);
    }

    #[test]
    fn hbr_rollup_is_not_double_counted_with_granular_records() {
        let report = feed(vec![prr(1, 1, 1), hbr(1, 1, 1, 1), hbr(255, 0, 1, 1)]);
        assert_eq!(report.by_rule("SEMANTIC/HBR_BIN_COUNT").count(), 0);
    }
}
