use std::collections::HashMap;

use stdf_core::{records::Prr, StdfRecord};

#[derive(Debug, Clone, PartialEq)]
pub struct TestResult {
    pub test_num: u32,
    pub test_txt: Option<String>,
    pub test_type: String,
    pub result: Option<f32>,
    pub test_pass: Option<bool>,
    pub lo_limit: Option<f32>,
    pub hi_limit: Option<f32>,
    pub units: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PartResult {
    /// Source-local test attempt sequence; independent of a reused PART_ID.
    pub part_sequence: u64,
    pub lot_id: String,
    pub wafer_id: Option<String>,
    pub part_id: String,
    pub head_num: u8,
    pub site_num: u8,
    pub x_coord: Option<i16>,
    pub y_coord: Option<i16>,
    pub hard_bin: u16,
    pub soft_bin: u16,
    pub part_pass: bool,
    pub test_time_ms: Option<u32>,
    pub tests: Vec<TestResult>,
}

#[derive(Debug, Clone)]
struct ActivePart {
    part_seq: u64,
    lot_id: String,
    wafer_id: Option<String>,
    tests: Vec<TestResult>,
}

#[derive(Debug, Clone)]
pub struct StdfContext {
    lot_id: String,
    wafer_ids: HashMap<(u8, u8), Option<String>>,
    site_groups: HashMap<(u8, u8), u8>,
    active_parts: HashMap<(u8, u8), ActivePart>,
    part_seq: u64,
}

impl Default for StdfContext {
    fn default() -> Self {
        Self {
            lot_id: String::new(),
            wafer_ids: HashMap::new(),
            site_groups: HashMap::new(),
            active_parts: HashMap::new(),
            part_seq: 0,
        }
    }
}

impl StdfContext {
    pub fn new() -> Self {
        Self::default()
    }

    fn wafer_for(&self, head: u8, site: u8) -> Option<String> {
        if let Some(group) = self.site_groups.get(&(head, site)) {
            return self
                .wafer_ids
                .get(&(head, *group))
                .or_else(|| self.wafer_ids.get(&(head, 255)))
                .cloned()
                .flatten();
        }
        if let Some(wafer) = self.wafer_ids.get(&(head, 255)) {
            return wafer.clone();
        }
        let mut matches = self.wafer_ids.iter().filter(|((h, _), _)| *h == head);
        let first = matches.next()?;
        if matches.next().is_some() {
            None
        } else {
            first.1.clone()
        }
    }

    fn active(&mut self, head: u8, site: u8) -> &mut ActivePart {
        let key = (head, site);
        let wafer_id = self.wafer_for(head, site);
        self.active_parts.entry(key).or_insert_with(|| {
            self.part_seq += 1;
            ActivePart {
                part_seq: self.part_seq,
                lot_id: self.lot_id.clone(),
                wafer_id,
                tests: Vec::new(),
            }
        })
    }

    pub fn push_record(&mut self, record: &StdfRecord) -> Option<PartResult> {
        match record {
            StdfRecord::Mir(mir) => {
                self.lot_id = mir.lot_id.clone();
                None
            }
            StdfRecord::Wir(wir) => {
                self.wafer_ids.insert(
                    (wir.head_num, wir.site_grp.unwrap_or(255)),
                    wir.wafer_id.clone(),
                );
                None
            }
            StdfRecord::Sdr(sdr) => {
                for site in &sdr.site_num {
                    self.site_groups.insert((sdr.head_num, *site), sdr.site_grp);
                }
                None
            }
            StdfRecord::Wrr(wrr) => {
                self.wafer_ids.remove(&(wrr.head_num, wrr.site_grp));
                None
            }
            StdfRecord::Pir(pir) => {
                self.part_seq += 1;
                self.active_parts.insert(
                    (pir.head_num, pir.site_num),
                    ActivePart {
                        part_seq: self.part_seq,
                        lot_id: self.lot_id.clone(),
                        wafer_id: self.wafer_for(pir.head_num, pir.site_num),
                        tests: Vec::new(),
                    },
                );
                None
            }
            StdfRecord::Ptr(ptr) => {
                let key = (ptr.head_num, ptr.site_num);
                let next_seq = self.part_seq + 1;
                let wafer_id = self.wafer_for(ptr.head_num, ptr.site_num);
                let active = self.active_parts.entry(key).or_insert_with(|| {
                    self.part_seq = next_seq;
                    ActivePart {
                        part_seq: next_seq,
                        lot_id: self.lot_id.clone(),
                        wafer_id,
                        tests: Vec::new(),
                    }
                });

                active.tests.push(TestResult {
                    test_num: ptr.test_num,
                    test_txt: ptr.test_txt.clone(),
                    test_type: "PTR".to_string(),
                    result: Some(ptr.result),
                    test_pass: ptr.pass_fail_valid().then(|| ptr.passed()),
                    lo_limit: ptr.lo_limit,
                    hi_limit: ptr.hi_limit,
                    units: ptr.units.clone(),
                });
                None
            }
            StdfRecord::Mpr(mpr) => {
                let tests = &mut self.active(mpr.head_num, mpr.site_num).tests;
                // TEST_FLG describes the complete MPR, not each RTN_RSLT value.
                tests.push(TestResult {
                    test_num: mpr.test_num,
                    test_txt: Some(format!(
                        "{} [MPR overall]",
                        mpr.test_txt.as_deref().unwrap_or("")
                    )),
                    test_type: "MPR".into(),
                    result: None,
                    test_pass: reliable_test_pass(mpr.test_flg),
                    lo_limit: None,
                    hi_limit: None,
                    units: mpr.units.clone(),
                });
                for (index, value) in mpr.rtn_rslt.iter().flatten().enumerate() {
                    tests.push(TestResult {
                        test_num: mpr.test_num,
                        test_txt: Some(format!(
                            "{} [MPR result {}]",
                            mpr.test_txt.as_deref().unwrap_or(""),
                            index
                        )),
                        test_type: "MPR".into(),
                        result: Some(*value),
                        test_pass: None,
                        lo_limit: mpr.lo_limit,
                        hi_limit: mpr.hi_limit,
                        units: mpr.units.clone(),
                    });
                }
                None
            }
            StdfRecord::Ftr(ftr) => {
                self.active(ftr.head_num, ftr.site_num)
                    .tests
                    .push(TestResult {
                        test_num: ftr.test_num,
                        test_txt: ftr.test_txt.clone(),
                        test_type: "FTR".into(),
                        result: None,
                        test_pass: reliable_test_pass(ftr.test_flg),
                        lo_limit: None,
                        hi_limit: None,
                        units: None,
                    });
                None
            }
            StdfRecord::Prr(prr) => Some(self.finish_part(prr)),
            _ => None,
        }
    }

    fn finish_part(&mut self, prr: &Prr) -> PartResult {
        let key = (prr.head_num, prr.site_num);
        let active = self.active_parts.remove(&key).unwrap_or_else(|| {
            self.part_seq += 1;
            ActivePart {
                part_seq: self.part_seq,
                lot_id: self.lot_id.clone(),
                wafer_id: self.wafer_for(prr.head_num, prr.site_num),
                tests: Vec::new(),
            }
        });
        let part_id = prr
            .part_id
            .clone()
            .unwrap_or_else(|| format!("H{}_S{}_P{}", prr.head_num, prr.site_num, active.part_seq));

        PartResult {
            part_sequence: active.part_seq,
            lot_id: active.lot_id,
            wafer_id: active.wafer_id,
            part_id,
            head_num: prr.head_num,
            site_num: prr.site_num,
            x_coord: prr.x_coord,
            y_coord: prr.y_coord,
            hard_bin: prr.hard_bin,
            soft_bin: prr.soft_bin,
            part_pass: prr.passed(),
            test_time_ms: prr.test_t,
            tests: active.tests,
        }
    }
}

// Reserved/unreliable/timeout/not-executed/aborted/no-indication are unknown.
fn reliable_test_pass(flags: u8) -> Option<bool> {
    (flags & 0x7e == 0).then_some(flags & 0x80 == 0)
}

pub(crate) fn validate_expansion(record: &StdfRecord) -> Result<(), stdf_core::StdfError> {
    if let StdfRecord::Mpr(mpr) = record {
        if mpr.rtn_rslt.as_ref().map_or(0, Vec::len) != usize::from(mpr.rslt_cnt)
            || mpr.rtn_stat.as_ref().map_or(0, Vec::len) != usize::from(mpr.rtn_icnt)
        {
            return Err(stdf_core::StdfError::InvalidField {
                record: "MPR",
                field: "RTN_RSLT/RTN_STAT",
                msg: "array length does not match declared count".into(),
            });
        }
    }
    Ok(())
}
