pub mod ater;
pub mod atr;
pub mod bps;
pub mod cdr;
pub mod characterization;
pub mod dtr;
pub mod eps;
pub mod far;
pub mod ftr;
pub mod gdr;
pub mod hbr;
pub mod mir;
pub mod mpr;
pub mod mrr;
pub mod pcr;
pub mod pgr;
pub mod pir;
pub mod plr;
pub mod pmr;
pub mod prr;
pub mod ptr;
pub mod rdr;
pub mod sbr;
pub mod sdr;
pub mod tsr;
pub mod wcr;
pub mod wir;
pub mod wrr;

pub use ater::Ater;
pub use atr::Atr;
pub use bps::Bps;
pub use cdr::Cdr;
pub use dtr::Dtr;
pub use eps::Eps;
pub use far::Far;
pub use ftr::Ftr;
pub use gdr::Gdr;
pub use hbr::Hbr;
pub use mir::Mir;
pub use mpr::Mpr;
pub use mrr::Mrr;
pub use pcr::Pcr;
pub use pgr::Pgr;
pub use pir::Pir;
pub use plr::Plr;
pub use pmr::Pmr;
pub use prr::Prr;
pub use ptr::Ptr;
pub use rdr::Rdr;
pub use sbr::Sbr;
pub use sdr::Sdr;
pub use tsr::Tsr;
pub use wcr::Wcr;
pub use wir::Wir;
pub use wrr::Wrr;

use crate::error::Result;
use crate::fields::FieldReader;
use crate::header::RecordHeader;
use crate::types::{ByteOrder, RecordType, VarData};

/// The decoded STDF record enum — one variant per known record, plus Unknown fallback.
#[derive(Debug, Clone)]
pub enum StdfRecord {
    Far(Far),
    Atr(Atr),
    Ater(Ater),
    Cdr(Cdr),
    Mir(Mir),
    Mrr(Mrr),
    Pcr(Pcr),
    Hbr(Hbr),
    Sbr(Sbr),
    Pmr(Pmr),
    Pgr(Pgr),
    Plr(Plr),
    Rdr(Rdr),
    Sdr(Sdr),
    Wir(Wir),
    Wrr(Wrr),
    Wcr(Wcr),
    Pir(Pir),
    Prr(Prr),
    Tsr(Tsr),
    Ptr(Ptr),
    Mpr(Mpr),
    Ftr(Ftr),
    Bps(Bps),
    Eps(Eps),
    Gdr(Gdr),
    Dtr(Dtr),
    /// Unknown or vendor-extension records — raw bytes preserved, never an error.
    Unknown {
        typ: u8,
        sub: u8,
        data: Vec<u8>,
    },
}

impl StdfRecord {
    /// Get the record type.
    pub fn record_type(&self) -> RecordType {
        match self {
            Self::Far(_) => RecordType::Far,
            Self::Atr(_) => RecordType::Atr,
            Self::Ater(_) => RecordType::Ater,
            Self::Cdr(_) => RecordType::Cdr,
            Self::Mir(_) => RecordType::Mir,
            Self::Mrr(_) => RecordType::Mrr,
            Self::Pcr(_) => RecordType::Pcr,
            Self::Hbr(_) => RecordType::Hbr,
            Self::Sbr(_) => RecordType::Sbr,
            Self::Pmr(_) => RecordType::Pmr,
            Self::Pgr(_) => RecordType::Pgr,
            Self::Plr(_) => RecordType::Plr,
            Self::Rdr(_) => RecordType::Rdr,
            Self::Sdr(_) => RecordType::Sdr,
            Self::Wir(_) => RecordType::Wir,
            Self::Wrr(_) => RecordType::Wrr,
            Self::Wcr(_) => RecordType::Wcr,
            Self::Pir(_) => RecordType::Pir,
            Self::Prr(_) => RecordType::Prr,
            Self::Tsr(_) => RecordType::Tsr,
            Self::Ptr(_) => RecordType::Ptr,
            Self::Mpr(_) => RecordType::Mpr,
            Self::Ftr(_) => RecordType::Ftr,
            Self::Bps(_) => RecordType::Bps,
            Self::Eps(_) => RecordType::Eps,
            Self::Gdr(_) => RecordType::Gdr,
            Self::Dtr(_) => RecordType::Dtr,
            Self::Unknown { typ, sub, .. } => RecordType::Unknown(*typ, *sub),
        }
    }

    /// Stable one-line text representation for dump/parity workflows.
    pub fn to_text_line(&self) -> String {
        match self {
            Self::Far(r) => format!("FAR CPU_TYPE={} STDF_VER={}", r.cpu_type, r.stdf_ver),
            Self::Mir(r) => format!(
                "MIR LOT_ID={} PART_TYP={} JOB_NAM={} START_T={}",
                r.lot_id, r.part_typ, r.job_nam, r.start_t
            ),
            Self::Mrr(r) => format!("MRR FINISH_T={} DISP_COD={}", r.finish_t, fmt_opt_u8(r.disp_cod)),
            Self::Pcr(r) => format!(
                "PCR HEAD_NUM={} SITE_NUM={} PART_CNT={} RTST_CNT={} ABRT_CNT={} GOOD_CNT={} FUNC_CNT={}",
                r.head_num,
                r.site_num,
                r.part_cnt,
                fmt_opt_u32(r.rtst_cnt),
                fmt_opt_u32(r.abrt_cnt),
                fmt_opt_u32(r.good_cnt),
                fmt_opt_u32(r.func_cnt)
            ),
            Self::Hbr(r) => format!(
                "HBR HEAD_NUM={} SITE_NUM={} HBIN_NUM={} HBIN_CNT={} HBIN_NAM={}",
                r.head_num,
                r.site_num,
                r.hbin_num,
                r.hbin_cnt,
                fmt_opt_str(r.hbin_nam.as_deref())
            ),
            Self::Sbr(r) => format!(
                "SBR HEAD_NUM={} SITE_NUM={} SBIN_NUM={} SBIN_CNT={} SBIN_NAM={}",
                r.head_num,
                r.site_num,
                r.sbin_num,
                r.sbin_cnt,
                fmt_opt_str(r.sbin_nam.as_deref())
            ),
            Self::Pmr(r) => format!(
                "PMR PMR_INDX={} CHAN_NAM={} PHY_NAM={} LOG_NAM={}",
                r.pmr_indx,
                fmt_opt_str(r.chan_nam.as_deref()),
                fmt_opt_str(r.phy_nam.as_deref()),
                fmt_opt_str(r.log_nam.as_deref())
            ),
            Self::Wir(r) => format!(
                "WIR HEAD_NUM={} SITE_GRP={} START_T={} WAFER_ID={}",
                r.head_num,
                fmt_opt_u8(r.site_grp),
                fmt_opt_u32(r.start_t),
                fmt_opt_str(r.wafer_id.as_deref())
            ),
            Self::Wrr(r) => format!(
                "WRR HEAD_NUM={} SITE_GRP={} FINISH_T={} PART_CNT={} WAFER_ID={}",
                r.head_num,
                r.site_grp,
                r.finish_t,
                r.part_cnt,
                fmt_opt_str(r.wafer_id.as_deref())
            ),
            Self::Pir(r) => format!("PIR HEAD_NUM={} SITE_NUM={}", r.head_num, r.site_num),
            Self::Prr(r) => format!(
                "PRR HEAD_NUM={} SITE_NUM={} NUM_TEST={} HARD_BIN={} SOFT_BIN={} PART_ID={} PASSED={}",
                r.head_num,
                r.site_num,
                r.num_test,
                r.hard_bin,
                r.soft_bin,
                fmt_opt_str(r.part_id.as_deref()),
                r.passed()
            ),
            Self::Tsr(r) => format!(
                "TSR HEAD_NUM={} SITE_NUM={} TEST_NUM={} EXEC_CNT={} FAIL_CNT={} TEST_NAM={}",
                r.head_num,
                r.site_num,
                r.test_num,
                fmt_opt_u32(r.exec_cnt),
                fmt_opt_u32(r.fail_cnt),
                fmt_opt_str(r.test_nam.as_deref())
            ),
            Self::Ptr(r) => format!(
                "PTR TEST_NUM={} HEAD_NUM={} SITE_NUM={} RESULT={} TEST_TXT={} PASSED={}",
                r.test_num,
                r.head_num,
                r.site_num,
                r.result,
                fmt_opt_str(r.test_txt.as_deref()),
                r.passed()
            ),
            Self::Mpr(r) => format!(
                "MPR TEST_NUM={} HEAD_NUM={} SITE_NUM={} RTN_ICNT={} RSLT_CNT={} TEST_TXT={}",
                r.test_num,
                r.head_num,
                r.site_num,
                r.rtn_icnt,
                r.rslt_cnt,
                fmt_opt_str(r.test_txt.as_deref())
            ),
            Self::Ftr(r) => format!(
                "FTR TEST_NUM={} HEAD_NUM={} SITE_NUM={} TEST_TXT={} PASSED={}",
                r.test_num,
                r.head_num,
                r.site_num,
                fmt_opt_str(r.test_txt.as_deref()),
                r.passed()
            ),
            Self::Gdr(r) => {
                let custom = match r.characterization() {
                    Ok(Some(c)) => format!(" CUSTOM_RECORD={} {}", c.record_name,
                        c.fields.iter().map(|f| format!("{}={}",f.name,format_var_data(&f.value))).collect::<Vec<_>>().join(" ")),
                    Err(e) => format!(" CUSTOM_DECODE_ERROR={e}"),
                    _ => String::new(),
                };
                format!("GDR FLD_CNT={} GEN_DATA={}{}", r.fld_cnt,
                    r.gen_data.iter().map(format_var_data).collect::<Vec<_>>().join(","), custom)
            },
            Self::Dtr(r) => format!("DTR TEXT_DAT={}", r.text_dat),
            Self::Unknown { typ, sub, data } => {
                format!("UNKNOWN REC_TYP={} REC_SUB={} LEN={}", typ, sub, data.len())
            }
            _ => format!("{} {:?}", self.record_type().mnemonic(), self),
        }
    }
}

impl std::fmt::Display for StdfRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.to_text_line())
    }
}

fn fmt_opt_str(value: Option<&str>) -> &str {
    value.unwrap_or("")
}

fn fmt_opt_u8(value: Option<u8>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

fn fmt_opt_u32(value: Option<u32>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

fn format_var_data(value: &VarData) -> String {
    match value {
        VarData::Padding => "PAD".to_string(),
        VarData::U1(v) => format!("U1({v})"),
        VarData::U2(v) => format!("U2({v})"),
        VarData::U4(v) => format!("U4({v})"),
        VarData::I1(v) => format!("I1({v})"),
        VarData::I2(v) => format!("I2({v})"),
        VarData::I4(v) => format!("I4({v})"),
        VarData::R4(v) => format!("R4({v})"),
        VarData::R8(v) => format!("R8({v})"),
        VarData::Cn(v) => format!("CN({v})"),
        VarData::Bn(v) => format!("BN({} bytes)", v.len()),
        VarData::Dn { bit_count, data } => format!("DN({bit_count} bits, {} bytes)", data.len()),
        VarData::N1(v) => format!("N1({v})"),
    }
}

/// Decode a record body given its header and raw body bytes.
pub fn decode_record(
    header: &RecordHeader,
    data: &[u8],
    byte_order: ByteOrder,
) -> Result<StdfRecord> {
    let reader = &mut FieldReader::new(data, byte_order);
    match header.record_type() {
        RecordType::Far => Ok(StdfRecord::Far(Far::parse(reader)?)),
        RecordType::Atr => Ok(StdfRecord::Atr(Atr::parse(reader)?)),
        RecordType::Ater => Ok(StdfRecord::Ater(Ater::parse(reader)?)),
        RecordType::Cdr => Ok(StdfRecord::Cdr(Cdr::parse(reader)?)),
        RecordType::Mir => Ok(StdfRecord::Mir(Mir::parse(reader)?)),
        RecordType::Mrr => Ok(StdfRecord::Mrr(Mrr::parse(reader)?)),
        RecordType::Pcr => Ok(StdfRecord::Pcr(Pcr::parse(reader)?)),
        RecordType::Hbr => Ok(StdfRecord::Hbr(Hbr::parse(reader)?)),
        RecordType::Sbr => Ok(StdfRecord::Sbr(Sbr::parse(reader)?)),
        RecordType::Pmr => Ok(StdfRecord::Pmr(Pmr::parse(reader)?)),
        RecordType::Pgr => Ok(StdfRecord::Pgr(Pgr::parse(reader)?)),
        RecordType::Plr => Ok(StdfRecord::Plr(Plr::parse(reader)?)),
        RecordType::Rdr => Ok(StdfRecord::Rdr(Rdr::parse(reader)?)),
        RecordType::Sdr => Ok(StdfRecord::Sdr(Sdr::parse(reader)?)),
        RecordType::Wir => Ok(StdfRecord::Wir(Wir::parse(reader)?)),
        RecordType::Wrr => Ok(StdfRecord::Wrr(Wrr::parse(reader)?)),
        RecordType::Wcr => Ok(StdfRecord::Wcr(Wcr::parse(reader)?)),
        RecordType::Pir => Ok(StdfRecord::Pir(Pir::parse(reader)?)),
        RecordType::Prr => Ok(StdfRecord::Prr(Prr::parse(reader)?)),
        RecordType::Tsr => Ok(StdfRecord::Tsr(Tsr::parse(reader)?)),
        RecordType::Ptr => Ok(StdfRecord::Ptr(Ptr::parse(reader)?)),
        RecordType::Mpr => Ok(StdfRecord::Mpr(Mpr::parse(reader)?)),
        RecordType::Ftr => Ok(StdfRecord::Ftr(Ftr::parse(reader)?)),
        RecordType::Bps => Ok(StdfRecord::Bps(Bps::parse(reader)?)),
        RecordType::Eps => Ok(StdfRecord::Eps(Eps::parse(reader)?)),
        RecordType::Gdr => Ok(StdfRecord::Gdr(Gdr::parse(reader)?)),
        RecordType::Dtr => Ok(StdfRecord::Dtr(Dtr::parse(reader)?)),
        RecordType::Unknown(t, s) => Ok(StdfRecord::Unknown {
            typ: t,
            sub: s,
            data: data.to_vec(),
        }),
    }
}
