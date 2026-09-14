use stdf_core::{
    records::{decode_record, Gdr},
    types::VarData,
    ByteOrder, RecordHeader, RecordType, StdfRecord,
};

fn cn(s: &str) -> Vec<u8> {
    let mut b = vec![s.len() as u8];
    b.extend(s.as_bytes());
    b
}
fn u2(n: u16, order: ByteOrder) -> [u8; 2] {
    match order {
        ByteOrder::LittleEndian => n.to_le_bytes(),
        ByteOrder::BigEndian => n.to_be_bytes(),
    }
}
fn u4(n: u32, order: ByteOrder) -> [u8; 4] {
    match order {
        ByteOrder::LittleEndian => n.to_le_bytes(),
        ByteOrder::BigEndian => n.to_be_bytes(),
    }
}
fn decode(typ: u8, sub: u8, b: &[u8], order: ByteOrder) -> stdf_core::error::Result<StdfRecord> {
    decode_record(
        &RecordHeader {
            rec_len: b.len() as u16,
            rec_typ: typ,
            rec_sub: sub,
        },
        b,
        order,
    )
}

#[test]
fn ater_site_and_binary_activity_are_preserved() {
    let mut b = cn("ACTIVITY_TRACE_LOG");
    b.extend(cn("test.method"));
    b.extend([2, 7, 3, 0, 0xff, 0x80]);
    for order in [ByteOrder::LittleEndian, ByteOrder::BigEndian] {
        let StdfRecord::Ater(r) = decode(137, 10, &b, order).unwrap() else {
            panic!()
        };
        assert_eq!((r.head_num, r.site_num), (2, 7));
        assert_eq!(r.evt_src, "test.method");
        assert_eq!(r.activity_bytes, [0, 0xff, 0x80]);
        for end in 0..b.len() {
            assert!(
                decode(137, 10, &b[..end], order).is_err(),
                "truncation {end}"
            );
        }
    }
    assert_eq!(RecordType::Ater.to_type_sub(), (137, 10));
}

#[test]
fn cdr_endianness_counts_and_long_cell_names() {
    for order in [ByteOrder::LittleEndian, ByteOrder::BigEndian] {
        let mut b = vec![1];
        b.extend(u2(0x1234, order));
        b.extend(cn("chain"));
        b.extend(u4(123456, order));
        b.extend(u2(25, order));
        b.extend(u2(26, order));
        b.push(2);
        b.extend(u2(40, order));
        b.extend(u2(41, order));
        b.push(1);
        b.extend(u2(42, order));
        b.push(255);
        b.extend(u2(2, order));
        for name in ["cell".to_string(), "a".repeat(300)] {
            b.extend(u2(name.len() as u16, order));
            b.extend(name.as_bytes());
        }
        let StdfRecord::Cdr(r) = decode(1, 94, &b, order).unwrap() else {
            panic!()
        };
        assert_eq!(r.cdr_indx, 0x1234);
        assert_eq!(r.chn_len, 123456);
        assert_eq!(r.cont_flg, 1);
        assert_eq!(r.m_clks, [40, 41]);
        assert_eq!(r.s_clks, [42]);
        assert_eq!(r.cell_lst[1].len(), 300);
        for end in 0..b.len() {
            assert!(decode(1, 94, &b[..end], order).is_err(), "truncation {end}");
        }
    }
    assert_eq!(RecordType::Cdr.to_type_sub(), (1, 94));
}

fn gdr(values: Vec<VarData>) -> Gdr {
    Gdr {
        fld_cnt: values.len() as u16,
        gen_data: values,
    }
}
fn s(v: &str) -> VarData {
    VarData::Cn(v.into())
}
fn setup(shmoo: bool, table_form: bool) -> Gdr {
    use VarData::*;
    let mut v = vec![
        Padding,
        s(if shmoo { "SHMOO" } else { "MARGIN" }),
        s("0x59"),
        s("char"),
        s("suite"),
        s("plot"),
        s("mode"),
        s("false"),
        U1(2),
    ];
    for id in [1, 2] {
        v.extend([
            U1(id),
            s("vcc"),
            s(""),
            s("specVariable"),
            s("vcc"),
            s("(ALL,1:5.0)"),
            s("absolute"),
            s("(1,6)"),
            R8(1.0000000000000002),
            U4(1),
        ]);
        if shmoo {
            v.extend([U4(0), s("linear")]);
        } else {
            v.push(s("serial"));
        }
        if table_form {
            v.push(R8(f64::NAN));
        }
        v.push(U1(1));
        v.extend([U1(1), s("tracking"), s(""), s("specVariable"), s("tracked")]);
        if table_form {
            v.push(s("initial"));
        }
        v.extend([s("relative"), s("(-10%,6%)")]);
        if table_form {
            v.push(R8(f64::INFINITY));
        }
    }
    gdr(v)
}

#[test]
fn ctsr_supports_both_test_modes_and_documented_tracking_forms() {
    for shmoo in [false, true] {
        for table in [false, true] {
            let g = setup(shmoo, table);
            let c = g.characterization().unwrap().unwrap();
            assert_eq!(c.record_name, "CTSR");
            assert_eq!(c.get("AXES[1].AXIS_ID"), Some(&VarData::U1(2)));
            let Some(VarData::R8(v)) = c.get("AXES[0].RNG_RESO") else {
                panic!()
            };
            assert_eq!(v.to_bits(), 1.0000000000000002f64.to_bits());
            assert_eq!(c.get("AXES[0].TRACKING[0].TRACK_INIT_VAL").is_some(), table);
            assert_eq!(c.get("EXE_ORDER").is_some(), shmoo);
            assert_eq!(c.get("RSC_MOD").is_some(), !shmoo);
            assert_eq!(c.fields[0].index, 1); // GDR padding is not a named value.
            assert_eq!(g.gen_data[0], VarData::Padding);
        }
    }
}

#[test]
fn ctrr_preserves_u4_site_text_results_and_reference_types() {
    use VarData::*;
    for name in ["SHMOO_RESULT", "MARGIN_RESULT"] {
        let c = gdr(vec![
            s(name),
            Padding,
            U4(89),
            U1(2),
            U4(65537),
            s("110"),
            s("(0,1,2)"),
            s("suite"),
            s("unusual result"),
        ])
        .characterization()
        .unwrap()
        .unwrap();
        assert_eq!(c.get("SITE_NUM"), Some(&U4(65537)));
        assert_eq!(c.get("CHAR_ID_REF"), Some(&U4(89)));
        assert_eq!(c.get("CELL_RSLT"), Some(&s("unusual result")));
    }
}

#[test]
fn custom_gdr_discriminator_is_exact_and_bad_structure_is_reported() {
    assert!(gdr(vec![s("prefix SHMOO")])
        .characterization()
        .unwrap()
        .is_none());
    assert!(gdr(vec![VarData::U1(1), s("SHMOO")])
        .characterization()
        .unwrap()
        .is_none());
    assert!(gdr(vec![s("SHMOO_RESULT"), VarData::U1(89)])
        .characterization()
        .is_err());
    let g = setup(true, false);
    for n in 1..g.gen_data.len() {
        let mut short = g.clone();
        short.gen_data.truncate(n);
        if short.custom_record_name().is_some() {
            assert!(short.characterization().is_err(), "truncated value {n}");
        }
    }
    let mut extra = g;
    extra.gen_data.push(VarData::U4(123));
    assert!(extra.characterization().is_err());
}

#[test]
fn characterization_wire_values_preserve_tags_and_precision_in_both_orders() {
    use VarData::*;
    for order in [ByteOrder::LittleEndian, ByteOrder::BigEndian] {
        let expected = setup(true, true);
        let mut b = u2(expected.fld_cnt, order).to_vec();
        for v in &expected.gen_data {
            match v {
                Padding => b.push(0),
                Cn(s) => {
                    b.push(10);
                    b.extend(cn(s));
                }
                U1(n) => b.extend([1, *n]),
                U4(n) => {
                    b.push(3);
                    b.extend(u4(*n, order));
                }
                R8(n) => {
                    b.push(8);
                    b.extend(match order {
                        ByteOrder::LittleEndian => n.to_le_bytes(),
                        ByteOrder::BigEndian => n.to_be_bytes(),
                    });
                }
                _ => unreachable!(),
            }
        }
        let StdfRecord::Gdr(g) = decode(50, 10, &b, order).unwrap() else {
            panic!()
        };
        let c = g.characterization().unwrap().unwrap();
        let Some(R8(n)) = c.get("AXES[0].RNG_RESO") else {
            panic!()
        };
        assert_eq!(n.to_bits(), 1.0000000000000002f64.to_bits());
        let Some(R8(n)) = c.get("AXES[0].MARGIN_VAL") else {
            panic!()
        };
        assert!(n.is_nan());
        for end in 0..b.len() {
            assert!(decode(50, 10, &b[..end], order).is_err());
        }
    }
}
