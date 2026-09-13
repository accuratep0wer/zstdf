// Copyright 2026 zstdf contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::fmt::Write as _;

use stdf_core::{types::VarData, StdfRecord};

#[derive(Default)]
pub struct AsciiDumper {
    scale_cache: HashMap<u32, (i8, String)>,
}

impl AsciiDumper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn header(file_name: &str) -> String {
        format!(
            "Starting /opt/hp93000/soc/formatter/bin/STDFreader:\n\n  STDF is a trademark of Teradyne, Inc.\n\nData from file '{file_name}'\n"
        )
    }

    pub fn footer() -> &'static str {
        "End of file. Done!\n"
    }

    pub fn render(&mut self, record: &StdfRecord) -> String {
        let mut out = String::new();
        match record {
            StdfRecord::Far(r) => {
                out.push_str("FAR Record\n");
                field_u(&mut out, "CPU_TYPE", r.cpu_type);
                field_u(&mut out, "STDF_VER", r.stdf_ver);
            }
            StdfRecord::Atr(r) => {
                out.push_str("ATR Record\n");
                field_time(&mut out, "MOD_TIME", r.mod_tim);
                field_opt_str(&mut out, "CMD_LINE", r.cmd_line.as_deref());
            }
            StdfRecord::Mir(r) => {
                out.push_str("MIR Record\n");
                field_time(&mut out, "SETUP_T", r.setup_t);
                field_time(&mut out, "START_T", r.start_t);
                field_u(&mut out, "STAT_NUM", r.stat_num);
                field_char(&mut out, "MODE_COD", r.mode_cod);
                field_char(&mut out, "RTST_COD", r.rtst_cod);
                field_char(&mut out, "PROT_COD", r.prot_cod);
                field_u(&mut out, "BURN_TIM", r.burn_tim);
                field_char(&mut out, "CMOD_COD", r.cmod_cod);
                field_str(&mut out, "LOT_ID", &r.lot_id);
                field_str(&mut out, "PART_TYP", &r.part_typ);
                field_str(&mut out, "NODE_NAM", &r.node_nam);
                field_str(&mut out, "TSTR_TYP", &r.tstr_typ);
                field_str(&mut out, "JOB_NAM", &r.job_nam);
                field_opt_str(&mut out, "JOB_REV", r.job_rev.as_deref());
                field_opt_str(&mut out, "SBLOT_ID", r.sblot_id.as_deref());
                field_opt_str(&mut out, "OPER_NAM", r.oper_nam.as_deref());
                field_opt_str(&mut out, "EXEC_TYP", r.exec_typ.as_deref());
                field_opt_str(&mut out, "EXEC_VER", r.exec_ver.as_deref());
                field_opt_str(&mut out, "TEST_COD", r.test_cod.as_deref());
                field_opt_str(&mut out, "TST_TEMP", r.tst_temp.as_deref());
                field_opt_str(&mut out, "USER_TXT", r.user_txt.as_deref());
                field_opt_str(&mut out, "AUX_FILE", r.aux_file.as_deref());
                field_opt_str(&mut out, "PKG_TYP", r.pkg_typ.as_deref());
                field_opt_str(&mut out, "FAMLY_ID", r.famly_id.as_deref());
                field_opt_str(&mut out, "DATE_COD", r.date_cod.as_deref());
                field_opt_str(&mut out, "FACIL_ID", r.facil_id.as_deref());
                field_opt_str(&mut out, "FLOOR_ID", r.floor_id.as_deref());
                field_opt_str(&mut out, "PROC_ID", r.proc_id.as_deref());
                field_opt_str(&mut out, "OPER_FRQ", r.oper_frq.as_deref());
                field_opt_str(&mut out, "SPEC_NAM", r.spec_nam.as_deref());
                field_opt_str(&mut out, "SPEC_VER", r.spec_ver.as_deref());
                field_opt_str(&mut out, "FLOW_ID", r.flow_id.as_deref());
                field_opt_str(&mut out, "SETUP_ID", r.setup_id.as_deref());
                field_opt_str(&mut out, "DSGN_REV", r.dsgn_rev.as_deref());
                field_opt_str(&mut out, "ENG_ID", r.eng_id.as_deref());
                field_opt_str(&mut out, "ROM_COD", r.rom_cod.as_deref());
                field_opt_str(&mut out, "SERL_NUM", r.serl_num.as_deref());
                field_opt_str(&mut out, "SUPR_NAM", r.supr_nam.as_deref());
            }
            StdfRecord::Mrr(r) => {
                out.push_str("MRR Record\n");
                field_time(&mut out, "FINISH_T", r.finish_t);
                field_opt_char(&mut out, "DISP_COD", r.disp_cod);
                field_opt_str(&mut out, "USR_DESC", r.usr_desc.as_deref());
                field_opt_str(&mut out, "EXC_DESC", r.exc_desc.as_deref());
            }
            StdfRecord::Pcr(r) => {
                out.push_str("PCR Record\n");
                field_u(&mut out, "HEAD_NUM", r.head_num);
                field_u(&mut out, "SITE_NUM", r.site_num);
                field_u(&mut out, "PART_CNT", r.part_cnt);
                field_opt_u(&mut out, "RTST_CNT", r.rtst_cnt);
                field_opt_u(&mut out, "ABRT_CNT", r.abrt_cnt);
                field_opt_u(&mut out, "GOOD_CNT", r.good_cnt);
                field_opt_u(&mut out, "FUNC_CNT", r.func_cnt);
            }
            StdfRecord::Hbr(r) => {
                out.push_str("HBR Record\n");
                field_u(&mut out, "HEAD_NUM", r.head_num);
                field_u(&mut out, "SITE_NUM", r.site_num);
                field_u(&mut out, "HBIN_NUM", r.hbin_num);
                field_u(&mut out, "HBIN_CNT", r.hbin_cnt);
                field_opt_char(&mut out, "HBIN_PF", r.hbin_pf);
                field_opt_str(&mut out, "HBIN_NAM", r.hbin_nam.as_deref());
            }
            StdfRecord::Sbr(r) => {
                out.push_str("SBR Record\n");
                field_u(&mut out, "HEAD_NUM", r.head_num);
                field_u(&mut out, "SITE_NUM", r.site_num);
                field_u(&mut out, "SBIN_NUM", r.sbin_num);
                field_u(&mut out, "SBIN_CNT", r.sbin_cnt);
                field_opt_char(&mut out, "SBIN_PF", r.sbin_pf);
                field_opt_str(&mut out, "SBIN_NAM", r.sbin_nam.as_deref());
            }
            StdfRecord::Pmr(r) => {
                out.push_str("PMR Record\n");
                field_u(&mut out, "PMR_INDX", r.pmr_indx);
                field_opt_u(&mut out, "CHAN_TYP", r.chan_typ);
                field_opt_str(&mut out, "CHAN_NAM", r.chan_nam.as_deref());
                field_opt_str(&mut out, "PHY_NAM", r.phy_nam.as_deref());
                field_opt_str(&mut out, "LOG_NAM", r.log_nam.as_deref());
                field_opt_u(&mut out, "HEAD_NUM", r.head_num);
                field_opt_u(&mut out, "SITE_NUM", r.site_num);
            }
            StdfRecord::Sdr(r) => {
                out.push_str("SDR Record\n");
                field_u(&mut out, "HEAD_NUM", r.head_num);
                field_u(&mut out, "SITE_GRP", r.site_grp);
                field_u(&mut out, "SITE_CNT", r.site_cnt);
                field_str(&mut out, "SITE_NUM", &join_dec(&r.site_num));
            }
            StdfRecord::Wir(r) => {
                out.push_str("WIR Record\n");
                field_u(&mut out, "HEAD_NUM", r.head_num);
                field_opt_u(&mut out, "SITE_GRP", r.site_grp);
                field_opt_time(&mut out, "START_T", r.start_t);
                field_opt_str(&mut out, "WAFER_ID", r.wafer_id.as_deref());
            }
            StdfRecord::Wrr(r) => {
                out.push_str("WRR Record\n");
                field_u(&mut out, "HEAD_NUM", r.head_num);
                field_u(&mut out, "SITE_GRP", r.site_grp);
                field_time(&mut out, "FINISH_T", r.finish_t);
                field_u(&mut out, "PART_CNT", r.part_cnt);
                field_opt_u(&mut out, "RTST_CNT", r.rtst_cnt);
                field_opt_u(&mut out, "ABRT_CNT", r.abrt_cnt);
                field_opt_u(&mut out, "GOOD_CNT", r.good_cnt);
                field_opt_u(&mut out, "FUNC_CNT", r.func_cnt);
                field_opt_str(&mut out, "WAFER_ID", r.wafer_id.as_deref());
            }
            StdfRecord::Pir(r) => {
                out.push_str("PIR Record\n");
                field_u(&mut out, "HEAD_NUM", r.head_num);
                field_u(&mut out, "SITE_NUM", r.site_num);
            }
            StdfRecord::Prr(r) => {
                out.push_str("PRR Record\n");
                field_u(&mut out, "HEAD_NUM", r.head_num);
                field_u(&mut out, "SITE_NUM", r.site_num);
                field_hex(&mut out, "PART_FLG", r.part_flg);
                field_u(&mut out, "NUM_TEST", r.num_test);
                field_u(&mut out, "HARD_BIN", r.hard_bin);
                field_u(&mut out, "SOFT_BIN", r.soft_bin);
                field_opt_i(&mut out, "X_COORD", r.x_coord);
                field_opt_i(&mut out, "Y_COORD", r.y_coord);
                field_opt_u(&mut out, "TEST_T", r.test_t);
                field_opt_str(&mut out, "PART_ID", r.part_id.as_deref());
                field_opt_str(&mut out, "PART_TXT", r.part_txt.as_deref());
                field_opt_bn(&mut out, "PART_FIX", r.part_fix.as_deref());
            }
            StdfRecord::Tsr(r) => {
                out.push_str("TSR Record\n");
                field_u(&mut out, "HEAD_NUM", r.head_num);
                field_u(&mut out, "SITE_NUM", r.site_num);
                field_char(&mut out, "TEST_TYP", r.test_typ);
                field_u(&mut out, "TEST_NUM", r.test_num);
                field_opt_u(&mut out, "EXEC_CNT", r.exec_cnt);
                field_opt_u(&mut out, "FAIL_CNT", r.fail_cnt);
                field_opt_u(&mut out, "ALRM_CNT", r.alrm_cnt);
                field_opt_str(&mut out, "TEST_NAM", r.test_nam.as_deref());
                field_opt_str(&mut out, "SEQ_NAME", r.seq_name.as_deref());
                field_opt_str(&mut out, "TEST_LBL", r.test_lbl.as_deref());
                field_opt_hex(&mut out, "OPT_FLAG", r.opt_flag);
                if let Some(v) = r.test_tim {
                    field_str(&mut out, "TEST_TIM", &fmt_pad4(v as f64));
                }
                let cached = self.scale_cache.get(&r.test_num);
                if let Some(v) = r.test_min {
                    field_str(&mut out, "TEST_MIN", &fmt_tsr_result(v, cached));
                }
                if let Some(v) = r.test_max {
                    field_str(&mut out, "TEST_MAX", &fmt_tsr_result(v, cached));
                }
                if let Some(v) = r.tst_sums {
                    field_str(&mut out, "TEST_SUMS", &fmt_r4_18(v));
                }
                if let Some(v) = r.tst_sqrs {
                    field_str(&mut out, "TEST_SQRS", &fmt_r4_18(v));
                }
            }
            StdfRecord::Ptr(r) => {
                out.push_str("PTR Record\n");
                field_u(&mut out, "TEST_NUM", r.test_num);
                field_u(&mut out, "HEAD_NUM", r.head_num);
                field_u(&mut out, "SITE_NUM", r.site_num);
                field_hex(&mut out, "TEST_FLG", r.test_flg);
                field_hex(&mut out, "PARM_FLG", r.parm_flg);
                if let Some(scale) = r.res_scal {
                    self.scale_cache
                        .entry(r.test_num)
                        .or_insert_with(|| (scale, r.units.clone().unwrap_or_default()));
                }
                let res_scal = r.res_scal.unwrap_or(0);
                let llm_scal = r.llm_scal.unwrap_or(0);
                let hlm_scal = r.hlm_scal.unwrap_or(0);
                if r.test_flg & 0x02 == 0 {
                    field_str(
                        &mut out,
                        "RESULT",
                        &fmt_9(scale_result_f32(r.result, res_scal)),
                    );
                }
                field_opt_str(&mut out, "TEST_TXT", r.test_txt.as_deref());
                field_opt_str(&mut out, "ALARM_ID", r.alarm_id.as_deref());
                field_opt_hex(&mut out, "OPT_FLAG", r.opt_flag);
                if let Some(v) = r.lo_limit {
                    field_str(&mut out, "LO_LIMIT", &fmt_9(scale_f64(v, llm_scal)));
                }
                if let Some(v) = r.hi_limit {
                    field_str(&mut out, "HI_LIMIT", &fmt_9(scale_f64(v, hlm_scal)));
                }
                if let Some(units) = &r.units {
                    field_str(
                        &mut out,
                        "UNITS",
                        &units_with_prefix(res_scal as i32, units),
                    );
                }
                if let Some(v) = r.lo_spec {
                    field_str(&mut out, "LO_SPEC", &fmt_pad4(scale_f64(v, res_scal)));
                }
                if let Some(v) = r.hi_spec {
                    field_str(&mut out, "HI_SPEC", &fmt_pad4(scale_f64(v, res_scal)));
                }
            }
            StdfRecord::Mpr(r) => {
                out.push_str("MPR Record\n");
                field_u(&mut out, "TEST_NUM", r.test_num);
                field_u(&mut out, "HEAD_NUM", r.head_num);
                field_u(&mut out, "SITE_NUM", r.site_num);
                field_hex(&mut out, "TEST_FLG", r.test_flg);
                field_hex(&mut out, "PARM_FLG", r.parm_flg);
                field_u(&mut out, "RTN_ICNT", r.rtn_icnt);
                field_u(&mut out, "RSLT_CNT", r.rslt_cnt);
                if let Some(values) = &r.rtn_stat {
                    field_str(&mut out, "RTN_STAT", &join_hex(values));
                }
                if let Some(scale) = r.res_scal {
                    self.scale_cache
                        .entry(r.test_num)
                        .or_insert_with(|| (scale, r.units.clone().unwrap_or_default()));
                }
                let res_scal = r.res_scal.unwrap_or(0);
                let llm_scal = r.llm_scal.unwrap_or(0);
                let hlm_scal = r.hlm_scal.unwrap_or(0);
                if let Some(values) = &r.rtn_rslt {
                    let joined = values
                        .iter()
                        .map(|v| fmt_9(scale_f64(*v, res_scal)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    field_str(&mut out, "RTN_RSLT", &joined);
                }
                field_opt_str(&mut out, "TEST_TXT", r.test_txt.as_deref());
                field_opt_str(&mut out, "ALARM_ID", r.alarm_id.as_deref());
                field_opt_hex(&mut out, "OPT_FLAG", r.opt_flag);
                if let Some(v) = r.lo_limit {
                    field_str(&mut out, "LO_LIMIT", &fmt_9(scale_f64(v, llm_scal)));
                }
                if let Some(v) = r.hi_limit {
                    field_str(&mut out, "HI_LIMIT", &fmt_9(scale_f64(v, hlm_scal)));
                }
                if let Some(v) = r.start_in {
                    field_str(&mut out, "START_IN", &fmt_pad4(v as f64));
                }
                if let Some(v) = r.incr_in {
                    field_str(&mut out, "INCR_IN", &fmt_pad4(v as f64));
                }
                if let Some(values) = &r.rtn_indx {
                    field_str(&mut out, "RTN_INDX", &join_hex(values));
                }
                if let Some(units) = &r.units {
                    field_str(
                        &mut out,
                        "UNITS",
                        &units_with_prefix(res_scal as i32, units),
                    );
                }
                field_opt_str(&mut out, "UNITS_IN", r.units_in.as_deref());
                if let Some(v) = r.lo_spec {
                    field_str(&mut out, "LO_SPEC", &fmt_pad4(scale_f64(v, res_scal)));
                }
                if let Some(v) = r.hi_spec {
                    field_str(&mut out, "HI_SPEC", &fmt_pad4(scale_f64(v, res_scal)));
                }
            }
            StdfRecord::Ftr(r) => {
                out.push_str("FTR Record\n");
                field_u(&mut out, "TEST_NUM", r.test_num);
                field_u(&mut out, "HEAD_NUM", r.head_num);
                field_u(&mut out, "SITE_NUM", r.site_num);
                field_hex(&mut out, "TEST_FLG", r.test_flg);
                field_opt_hex(&mut out, "OPT_FLAG", r.opt_flag);
                field_opt_u(&mut out, "CYCL_CNT", r.cycl_cnt);
                field_opt_u(&mut out, "REL_VADR", r.rel_vadr);
                field_opt_u(&mut out, "REPT_CNT", r.rept_cnt);
                field_opt_u(&mut out, "NUM_FAIL", r.num_fail);
                field_opt_i(&mut out, "XFAIL_AD", r.xfail_ad);
                field_opt_i(&mut out, "YFAIL_AD", r.yfail_ad);
                field_opt_i(&mut out, "VECT_OFF", r.vect_off);
                field_opt_u(&mut out, "RTN_ICNT", r.rtn_icnt);
                field_opt_u(&mut out, "PGM_ICNT", r.pgm_icnt);
                if let Some(values) = &r.rtn_indx {
                    field_str(&mut out, "RTN_INDX", &join_hex(values));
                }
                if let Some(values) = &r.rtn_stat {
                    field_str(&mut out, "RTN_STAT", &join_hex(values));
                }
                if let Some(values) = &r.pgm_indx {
                    field_str(&mut out, "PGM_INDX", &join_hex(values));
                }
                if let Some(values) = &r.pgm_stat {
                    field_str(&mut out, "PGM_STAT", &join_hex(values));
                }
                field_opt_dn(
                    &mut out,
                    "FAIL_PIN",
                    r.fail_pin.as_ref().map(|(_, data)| data.as_slice()),
                );
                field_opt_str(&mut out, "VECT_NAM", r.vect_nam.as_deref());
                field_opt_str(&mut out, "TIME_SET", r.time_set.as_deref());
                field_opt_str(&mut out, "OP_CODE", r.op_code.as_deref());
                field_opt_str(&mut out, "TEST_TXT", r.test_txt.as_deref());
                field_opt_str(&mut out, "ALARM_ID", r.alarm_id.as_deref());
                field_opt_str(&mut out, "PROG_TXT", r.prog_txt.as_deref());
                field_opt_str(&mut out, "RSLT_TXT", r.rslt_txt.as_deref());
                field_opt_u(&mut out, "PATG_NUM", r.patg_num);
                field_opt_dn(
                    &mut out,
                    "SPIN_MAP",
                    r.spin_map.as_ref().map(|(_, data)| data.as_slice()),
                );
            }
            StdfRecord::Bps(r) => {
                out.push_str("BPS Record\n");
                field_opt_str(&mut out, "SEQ_NAME", r.seq_name.as_deref());
            }
            StdfRecord::Eps(_) => out.push_str("EPS Record\n"),
            StdfRecord::Gdr(r) => {
                out.push_str("GDR Record\n");
                field_u(&mut out, "FLD_CNT", r.fld_cnt);
                for (idx, field) in r.gen_data.iter().enumerate() {
                    let _ = writeln!(out, "  Field # {:>2}:    {}", idx + 1, fmt_gdr_field(field));
                }
            }
            StdfRecord::Dtr(r) => {
                out.push_str("DTR Record\n");
                field_str(&mut out, "TEXT_DAT", &r.text_dat);
            }
            StdfRecord::Unknown { typ, sub, .. } => {
                let _ = writeln!(out, "UNK Record (typ={typ}, sub={sub})");
            }
            _ => {
                let _ = writeln!(out, "{} Record", record.record_type().mnemonic());
            }
        }
        out
    }
}

fn field_line(out: &mut String, name: &str, value: &str) {
    let _ = writeln!(out, "  {:<15}{value}", format!("{name}:"));
}

fn field_str(out: &mut String, name: &str, value: &str) {
    field_line(out, name, value);
}

fn field_u(out: &mut String, name: &str, value: impl std::fmt::Display) {
    field_line(out, name, &value.to_string());
}

fn field_opt_u(out: &mut String, name: &str, value: Option<impl std::fmt::Display>) {
    if let Some(value) = value {
        field_line(out, name, &value.to_string());
    }
}

fn field_opt_i(out: &mut String, name: &str, value: Option<impl std::fmt::Display>) {
    if let Some(value) = value {
        field_line(out, name, &value.to_string());
    }
}

fn field_hex(out: &mut String, name: &str, value: u8) {
    field_line(out, name, &format!("0x{value:X}"));
}

fn field_opt_hex(out: &mut String, name: &str, value: Option<u8>) {
    if let Some(value) = value {
        field_hex(out, name, value);
    }
}

fn field_char(out: &mut String, name: &str, value: u8) {
    field_line(out, name, &format!("'{}'", value as char));
}

fn field_opt_char(out: &mut String, name: &str, value: Option<u8>) {
    if let Some(value) = value {
        field_char(out, name, value);
    }
}

fn field_opt_str(out: &mut String, name: &str, value: Option<&str>) {
    if let Some(value) = value {
        field_line(out, name, value);
    }
}

fn field_opt_bn(out: &mut String, name: &str, value: Option<&[u8]>) {
    if let Some(value) = value {
        field_line(out, name, &join_hex(value));
    }
}

fn field_opt_dn(out: &mut String, name: &str, value: Option<&[u8]>) {
    if let Some(value) = value {
        field_line(out, name, &join_bin(value));
    }
}

fn field_opt_time(out: &mut String, name: &str, value: Option<u32>) {
    if let Some(value) = value {
        field_time(out, name, value);
    }
}

fn join_dec(values: &[u8]) -> String {
    values
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

fn join_hex<T: Into<u32> + Copy>(values: &[T]) -> String {
    values
        .iter()
        .map(|value| format!("0x{:X}", (*value).into()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn join_bin(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:08b} ")).collect()
}

fn fmt_9(value: f64) -> String {
    format!("{value:.9}")
}

fn fmt_r4_18(value: f32) -> String {
    format!("{value:.18}")
}

fn fmt_pad4(value: f64) -> String {
    format!("{value:<10.4}")
}

fn scale_f64(raw: f32, scale: i8) -> f64 {
    raw as f64 * 10f64.powi(scale as i32)
}

fn scale_result_f32(raw: f32, scale: i8) -> f64 {
    let scaled = if scale >= 0 {
        raw * 10f32.powi(scale as i32)
    } else {
        raw / 10f32.powi(-(scale as i32))
    };
    scaled as f64
}

fn si_prefix(scale: i32) -> &'static str {
    match scale {
        -12 => "T",
        -9 => "G",
        -6 => "M",
        -3 => "K",
        -2 => "H",
        -1 => "D",
        0 => "",
        1 => "d",
        2 => "c",
        3 => "m",
        6 => "u",
        9 => "n",
        12 => "p",
        15 => "f",
        _ => "",
    }
}

fn units_with_prefix(scale: i32, raw_units: &str) -> String {
    match si_prefix(scale) {
        "" => format!(" {raw_units}"),
        prefix => format!("{prefix}{raw_units}"),
    }
}

fn fmt_tsr_result(value: f32, cached: Option<&(i8, String)>) -> String {
    let (scale, units) = cached
        .map(|(scale, units)| (*scale, units.as_str()))
        .unwrap_or((0, ""));
    let scaled = fmt_9(scale_result_f32(value, scale));
    if units.is_empty() {
        scaled
    } else {
        format!("{:<12} {}{units}", scaled, si_prefix(scale as i32))
    }
}

fn fmt_gdr_field(field: &VarData) -> String {
    match field {
        VarData::Padding => "PAD Field".to_string(),
        VarData::U1(value) => format!("U1: 0x{value:X}"),
        VarData::U2(value) => format!("U2: 0x{value:X}"),
        VarData::U4(value) => format!("U4: 0x{value:X}"),
        VarData::I1(value) => format!("I1: {value}"),
        VarData::I2(value) => format!("I2: {value}"),
        VarData::I4(value) => format!("I4: {value}"),
        VarData::R4(value) => format!("R4: {}", fmt_9(*value as f64)),
        VarData::R8(value) => format!("R8: {value:.9}"),
        VarData::Cn(value) => format!("Cn: {value}"),
        VarData::Bn(value) => format!("Bn: {}", join_hex(value)),
        VarData::Dn { data, .. } => format!("Dn: {}", join_hex(data)),
        VarData::N1(value) => format!("N1: 0x{value:X}"),
    }
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

fn field_time(out: &mut String, name: &str, epoch: u32) {
    field_line(out, name, &format!("{epoch} ({})", format_epoch(epoch)));
}

fn format_epoch(epoch: u32) -> String {
    let seconds = epoch as i64;
    let days = seconds.div_euclid(86_400);
    let time_of_day = seconds.rem_euclid(86_400);
    let (hh, mm, ss) = (
        time_of_day / 3600,
        (time_of_day % 3600) / 60,
        time_of_day % 60,
    );

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{d:02}-{}-{y:04} {hh:02}:{mm:02}:{ss:02}",
        MONTHS[(m - 1) as usize]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use stdf_core::records::*;

    #[test]
    fn renders_ptr_with_scaling_and_units() {
        let mut dumper = AsciiDumper::new();
        let text = dumper.render(&StdfRecord::Ptr(Ptr {
            test_num: 7,
            head_num: 1,
            site_num: 2,
            test_flg: 0,
            parm_flg: 0,
            result: 1.25,
            test_txt: Some("VDD".to_string()),
            alarm_id: None,
            opt_flag: Some(0),
            res_scal: Some(6),
            llm_scal: Some(0),
            hlm_scal: Some(0),
            lo_limit: Some(0.0),
            hi_limit: Some(2.0),
            units: Some("A".to_string()),
            c_resfmt: None,
            c_llmfmt: None,
            c_hlmfmt: None,
            lo_spec: Some(0.1),
            hi_spec: Some(1.9),
        }));

        assert!(text.contains("PTR Record"));
        assert!(text.contains("TEST_NUM:      7"));
        assert!(text.contains("RESULT:        1250000.000000000"));
        assert!(text.contains("UNITS:         uA"));
    }

    #[test]
    fn tsr_reuses_first_ptr_scale_cache() {
        let mut dumper = AsciiDumper::new();
        let _ = dumper.render(&StdfRecord::Ptr(Ptr {
            test_num: 9,
            head_num: 1,
            site_num: 0,
            test_flg: 0,
            parm_flg: 0,
            result: 1.0,
            test_txt: None,
            alarm_id: None,
            opt_flag: None,
            res_scal: Some(3),
            llm_scal: None,
            hlm_scal: None,
            lo_limit: None,
            hi_limit: None,
            units: Some("V".to_string()),
            c_resfmt: None,
            c_llmfmt: None,
            c_hlmfmt: None,
            lo_spec: None,
            hi_spec: None,
        }));
        let text = dumper.render(&StdfRecord::Tsr(Tsr {
            head_num: 1,
            site_num: 0,
            test_typ: b'P',
            test_num: 9,
            exec_cnt: Some(1),
            fail_cnt: Some(0),
            alrm_cnt: None,
            test_nam: None,
            seq_name: None,
            test_lbl: None,
            opt_flag: None,
            test_tim: None,
            test_min: Some(1.0),
            test_max: Some(2.0),
            tst_sums: None,
            tst_sqrs: None,
        }));

        assert!(text.contains("TEST_MIN:      1000.000000000 mV"));
        assert!(text.contains("TEST_MAX:      2000.000000000 mV"));
    }

    #[test]
    fn renders_gdr_var_data() {
        let mut dumper = AsciiDumper::new();
        let text = dumper.render(&StdfRecord::Gdr(Gdr {
            fld_cnt: 2,
            gen_data: vec![VarData::U1(42), VarData::Cn("ABC".to_string())],
        }));

        assert!(text.contains("FLD_CNT:       2"));
        assert!(text.contains("Field #  1:    U1: 0x2A"));
        assert!(text.contains("Field #  2:    Cn: ABC"));
    }
}
