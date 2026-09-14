/// Byte order detected from FAR.CPU_TYPE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByteOrder {
    /// CPU_TYPE = 2 (x86) — Teradyne default, most common
    LittleEndian,
    /// CPU_TYPE = 1 (Sun/SPARC) — some Advantest platforms
    BigEndian,
}

/// Record type enumeration — maps (typ, sub) pairs to symbolic names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RecordType {
    Far,
    Atr,
    Cdr,
    Ater,
    Mir,
    Mrr,
    Pcr,
    Hbr,
    Sbr,
    Pmr,
    Pgr,
    Plr,
    Rdr,
    Sdr,
    Wir,
    Wrr,
    Wcr,
    Pir,
    Prr,
    Tsr,
    Ptr,
    Mpr,
    Ftr,
    Bps,
    Eps,
    Gdr,
    Dtr,
    Unknown(u8, u8),
}

impl RecordType {
    pub fn from_type_sub(typ: u8, sub: u8) -> Self {
        match (typ, sub) {
            (0, 10) => Self::Far,
            (0, 20) => Self::Atr,
            (1, 94) => Self::Cdr,
            (137, 10) => Self::Ater,
            (1, 10) => Self::Mir,
            (1, 20) => Self::Mrr,
            (1, 30) => Self::Pcr,
            (1, 40) => Self::Hbr,
            (1, 50) => Self::Sbr,
            (1, 60) => Self::Pmr,
            (1, 62) => Self::Pgr,
            (1, 63) => Self::Plr,
            (1, 70) => Self::Rdr,
            (1, 80) => Self::Sdr,
            (2, 10) => Self::Wir,
            (2, 20) => Self::Wrr,
            (2, 30) => Self::Wcr,
            (5, 10) => Self::Pir,
            (5, 20) => Self::Prr,
            (10, 30) => Self::Tsr,
            (15, 10) => Self::Ptr,
            (15, 15) => Self::Mpr,
            (15, 20) => Self::Ftr,
            (20, 10) => Self::Bps,
            (20, 20) => Self::Eps,
            (50, 10) => Self::Gdr,
            (50, 30) => Self::Dtr,
            _ => Self::Unknown(typ, sub),
        }
    }

    pub fn to_type_sub(&self) -> (u8, u8) {
        match self {
            Self::Far => (0, 10),
            Self::Atr => (0, 20),
            Self::Cdr => (1, 94),
            Self::Ater => (137, 10),
            Self::Mir => (1, 10),
            Self::Mrr => (1, 20),
            Self::Pcr => (1, 30),
            Self::Hbr => (1, 40),
            Self::Sbr => (1, 50),
            Self::Pmr => (1, 60),
            Self::Pgr => (1, 62),
            Self::Plr => (1, 63),
            Self::Rdr => (1, 70),
            Self::Sdr => (1, 80),
            Self::Wir => (2, 10),
            Self::Wrr => (2, 20),
            Self::Wcr => (2, 30),
            Self::Pir => (5, 10),
            Self::Prr => (5, 20),
            Self::Tsr => (10, 30),
            Self::Ptr => (15, 10),
            Self::Mpr => (15, 15),
            Self::Ftr => (15, 20),
            Self::Bps => (20, 10),
            Self::Eps => (20, 20),
            Self::Gdr => (50, 10),
            Self::Dtr => (50, 30),
            Self::Unknown(t, s) => (*t, *s),
        }
    }

    /// Human-readable record mnemonic.
    pub fn mnemonic(&self) -> &'static str {
        match self {
            Self::Far => "FAR",
            Self::Atr => "ATR",
            Self::Cdr => "CDR",
            Self::Ater => "ATER",
            Self::Mir => "MIR",
            Self::Mrr => "MRR",
            Self::Pcr => "PCR",
            Self::Hbr => "HBR",
            Self::Sbr => "SBR",
            Self::Pmr => "PMR",
            Self::Pgr => "PGR",
            Self::Plr => "PLR",
            Self::Rdr => "RDR",
            Self::Sdr => "SDR",
            Self::Wir => "WIR",
            Self::Wrr => "WRR",
            Self::Wcr => "WCR",
            Self::Pir => "PIR",
            Self::Prr => "PRR",
            Self::Tsr => "TSR",
            Self::Ptr => "PTR",
            Self::Mpr => "MPR",
            Self::Ftr => "FTR",
            Self::Bps => "BPS",
            Self::Eps => "EPS",
            Self::Gdr => "GDR",
            Self::Dtr => "DTR",
            Self::Unknown(_, _) => "???",
        }
    }
}

/// Variable-type data for GDR records (V*n).
#[derive(Debug, Clone, PartialEq)]
pub enum VarData {
    Padding,
    U1(u8),
    U2(u16),
    U4(u32),
    I1(i8),
    I2(i16),
    I4(i32),
    R4(f32),
    R8(f64),
    Cn(String),
    Bn(Vec<u8>),
    Dn { bit_count: u16, data: Vec<u8> },
    N1(u8),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_known_record_types() {
        let cases: Vec<(u8, u8, RecordType)> = vec![
            (0, 10, RecordType::Far),
            (0, 20, RecordType::Atr),
            (1, 10, RecordType::Mir),
            (1, 20, RecordType::Mrr),
            (1, 30, RecordType::Pcr),
            (1, 40, RecordType::Hbr),
            (1, 50, RecordType::Sbr),
            (1, 60, RecordType::Pmr),
            (1, 62, RecordType::Pgr),
            (1, 63, RecordType::Plr),
            (1, 70, RecordType::Rdr),
            (1, 80, RecordType::Sdr),
            (2, 10, RecordType::Wir),
            (2, 20, RecordType::Wrr),
            (2, 30, RecordType::Wcr),
            (5, 10, RecordType::Pir),
            (5, 20, RecordType::Prr),
            (10, 30, RecordType::Tsr),
            (15, 10, RecordType::Ptr),
            (15, 15, RecordType::Mpr),
            (15, 20, RecordType::Ftr),
            (20, 10, RecordType::Bps),
            (20, 20, RecordType::Eps),
            (50, 10, RecordType::Gdr),
            (50, 30, RecordType::Dtr),
        ];
        for (typ, sub, expected) in cases {
            assert_eq!(
                RecordType::from_type_sub(typ, sub),
                expected,
                "failed for ({}, {})",
                typ,
                sub
            );
        }
    }

    #[test]
    fn test_unknown_record_type() {
        assert_eq!(
            RecordType::from_type_sub(99, 99),
            RecordType::Unknown(99, 99)
        );
        assert_eq!(
            RecordType::from_type_sub(200, 1),
            RecordType::Unknown(200, 1)
        );
    }

    #[test]
    fn test_roundtrip_all() {
        let types = [
            RecordType::Far,
            RecordType::Atr,
            RecordType::Mir,
            RecordType::Mrr,
            RecordType::Pcr,
            RecordType::Hbr,
            RecordType::Sbr,
            RecordType::Pmr,
            RecordType::Pgr,
            RecordType::Plr,
            RecordType::Rdr,
            RecordType::Sdr,
            RecordType::Wir,
            RecordType::Wrr,
            RecordType::Wcr,
            RecordType::Pir,
            RecordType::Prr,
            RecordType::Tsr,
            RecordType::Ptr,
            RecordType::Mpr,
            RecordType::Ftr,
            RecordType::Bps,
            RecordType::Eps,
            RecordType::Gdr,
            RecordType::Dtr,
        ];
        for rt in &types {
            let (t, s) = rt.to_type_sub();
            assert_eq!(RecordType::from_type_sub(t, s), *rt);
        }
    }
}
