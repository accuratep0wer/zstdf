use crate::{error::Result, fields::FieldReader};

/// CDR — V4-2007 Chain Description Record (1, 94).
/// Continuation fragments are retained individually; no chain merging is inferred.
#[derive(Debug, Clone)]
pub struct Cdr {
    pub cont_flg: u8,
    pub cdr_indx: u16,
    pub chn_nam: String,
    pub chn_len: u32,
    pub sin_pin: u16,
    pub sout_pin: u16,
    pub mstr_cnt: u8,
    pub m_clks: Vec<u16>,
    pub slav_cnt: u8,
    pub s_clks: Vec<u16>,
    pub inv_val: u8,
    pub lst_cnt: u16,
    pub cell_lst: Vec<String>,
}

impl Cdr {
    pub fn parse(r: &mut FieldReader) -> Result<Self> {
        let cont_flg = r.read_u1()?;
        let cdr_indx = r.read_u2()?;
        let chn_nam = r.read_cn()?;
        let chn_len = r.read_u4()?;
        let sin_pin = r.read_u2()?;
        let sout_pin = r.read_u2()?;
        let mstr_cnt = r.read_u1()?;
        let m_clks = (0..mstr_cnt).map(|_| r.read_u2()).collect::<Result<_>>()?;
        let slav_cnt = r.read_u1()?;
        let s_clks = (0..slav_cnt).map(|_| r.read_u2()).collect::<Result<_>>()?;
        let inv_val = r.read_u1()?;
        let lst_cnt = r.read_u2()?;
        let cell_lst = (0..lst_cnt).map(|_| r.read_sn()).collect::<Result<_>>()?;
        Ok(Self {
            cont_flg,
            cdr_indx,
            chn_nam,
            chn_len,
            sin_pin,
            sout_pin,
            mstr_cnt,
            m_clks,
            slav_cnt,
            s_clks,
            inv_val,
            lst_cnt,
            cell_lst,
        })
    }
}
