// Copyright 2026 zstdf contributors
// SPDX-License-Identifier: Apache-2.0

//! Property tests for the decoder. The serializer below is a test oracle,
//! not a product encoder.

use proptest::prelude::*;
use stdf_core::{decoding_invariants, prefix_stable, ByteOrder, StdfRecord};

#[derive(Clone, Debug)]
struct FileSpec {
    order: ByteOrder,
    head: u8,
    site: u8,
    test_num: u32,
    test_flg: u8,
    parm_flg: u8,
    result_bits: u32,
    test_txt: String,
    part_flg: u8,
    num_test: u16,
    hard_bin: u16,
    soft_bin: u16,
    note: String,
    finish_t: u32,
    unknown: Vec<u8>,
}

fn encode(spec: &FileSpec) -> Vec<u8> {
    let mut data = Vec::new();
    let cpu = match spec.order {
        ByteOrder::LittleEndian => 2,
        ByteOrder::BigEndian => 1,
    };
    push(&mut data, spec.order, 0, 10, &[cpu, 4]);
    push(&mut data, spec.order, 5, 10, &[spec.head, spec.site]);

    let mut ptr = Vec::new();
    put_u32(&mut ptr, spec.order, spec.test_num);
    ptr.push(spec.head);
    ptr.push(spec.site);
    ptr.push(spec.test_flg);
    ptr.push(spec.parm_flg);
    put_u32(&mut ptr, spec.order, spec.result_bits);
    put_cn(&mut ptr, &spec.test_txt);
    push(&mut data, spec.order, 15, 10, &ptr);

    let mut prr = Vec::new();
    prr.push(spec.head);
    prr.push(spec.site);
    prr.push(spec.part_flg);
    put_u16(&mut prr, spec.order, spec.num_test);
    put_u16(&mut prr, spec.order, spec.hard_bin);
    put_u16(&mut prr, spec.order, spec.soft_bin);
    push(&mut data, spec.order, 5, 20, &prr);

    let mut dtr = Vec::new();
    put_cn(&mut dtr, &spec.note);
    push(&mut data, spec.order, 50, 30, &dtr);

    let mut mrr = Vec::new();
    put_u32(&mut mrr, spec.order, spec.finish_t);
    push(&mut data, spec.order, 1, 20, &mrr);

    let unknown: Vec<u8> = spec.unknown.iter().take(64).copied().collect();
    push(&mut data, spec.order, 99, 1, &unknown);
    data
}

fn push(data: &mut Vec<u8>, order: ByteOrder, typ: u8, sub: u8, body: &[u8]) {
    put_u16(
        data,
        order,
        u16::try_from(body.len()).expect("test body fits in u16"),
    );
    data.push(typ);
    data.push(sub);
    data.extend_from_slice(body);
}

fn put_u16(data: &mut Vec<u8>, order: ByteOrder, value: u16) {
    let bytes = match order {
        ByteOrder::LittleEndian => value.to_le_bytes(),
        ByteOrder::BigEndian => value.to_be_bytes(),
    };
    data.extend_from_slice(&bytes);
}

fn put_u32(data: &mut Vec<u8>, order: ByteOrder, value: u32) {
    let bytes = match order {
        ByteOrder::LittleEndian => value.to_le_bytes(),
        ByteOrder::BigEndian => value.to_be_bytes(),
    };
    data.extend_from_slice(&bytes);
}

fn put_cn(data: &mut Vec<u8>, text: &str) {
    let bytes = text.as_bytes();
    let len = bytes.len().min(255);
    data.push(u8::try_from(len).expect("cn length fits"));
    data.extend_from_slice(&bytes[..len]);
}

fn arb_text() -> impl Strategy<Value = String> {
    prop::string::string_regex("[ -~]{0,24}").expect("ascii pattern")
}

fn arb_file() -> impl Strategy<Value = FileSpec> {
    let header = (
        prop_oneof![Just(ByteOrder::LittleEndian), Just(ByteOrder::BigEndian)],
        any::<u8>(),
        any::<u8>(),
        any::<u32>(),
        any::<u8>(),
        any::<u8>(),
        any::<u32>(),
    );
    let rest = (
        arb_text(),
        any::<u8>(),
        any::<u16>(),
        any::<u16>(),
        any::<u16>(),
        arb_text(),
        any::<u32>(),
        prop::collection::vec(any::<u8>(), 0..32),
    );
    (header, rest).prop_map(
        |(
            (order, head, site, test_num, test_flg, parm_flg, result_bits),
            (test_txt, part_flg, num_test, hard_bin, soft_bin, note, finish_t, unknown),
        )| FileSpec {
            order,
            head,
            site,
            test_num,
            test_flg,
            parm_flg,
            result_bits,
            test_txt,
            part_flg,
            num_test,
            hard_bin,
            soft_bin,
            note,
            finish_t,
            unknown,
        },
    )
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    #[test]
    fn encoded_records_round_trip(spec in arb_file()) {
        let data = encode(&spec);
        decoding_invariants(&data).expect("encoded file invariants");
        let (records, error) = stdf_core::decode_all(&data);
        prop_assert!(error.is_none(), "encoded file stopped early: {error:?}");
        prop_assert_eq!(records.len(), 7);
        match &records[0] {
            StdfRecord::Far(far) => {
                prop_assert_eq!(far.stdf_ver, 4);
                prop_assert_eq!(
                    far.cpu_type,
                    match spec.order {
                        ByteOrder::LittleEndian => 2,
                        ByteOrder::BigEndian => 1,
                    }
                );
            }
            other => prop_assert!(false, "expected FAR, got {:?}", other.record_type()),
        }
        match &records[2] {
            StdfRecord::Ptr(ptr) => {
                prop_assert_eq!(ptr.test_num, spec.test_num);
                prop_assert_eq!(ptr.head_num, spec.head);
                prop_assert_eq!(ptr.site_num, spec.site);
                prop_assert_eq!(ptr.test_flg, spec.test_flg);
                prop_assert_eq!(ptr.parm_flg, spec.parm_flg);
                prop_assert_eq!(ptr.result.to_bits(), spec.result_bits);
                prop_assert_eq!(ptr.test_txt.as_deref(), Some(spec.test_txt.as_str()));
            }
            other => prop_assert!(false, "expected PTR, got {:?}", other.record_type()),
        }
        match &records[3] {
            StdfRecord::Prr(prr) => {
                prop_assert_eq!(prr.head_num, spec.head);
                prop_assert_eq!(prr.site_num, spec.site);
                prop_assert_eq!(prr.part_flg, spec.part_flg);
                prop_assert_eq!(prr.num_test, spec.num_test);
                prop_assert_eq!(prr.hard_bin, spec.hard_bin);
                prop_assert_eq!(prr.soft_bin, spec.soft_bin);
            }
            other => prop_assert!(false, "expected PRR, got {:?}", other.record_type()),
        }
        match &records[4] {
            StdfRecord::Dtr(dtr) => prop_assert_eq!(&dtr.text_dat, &spec.note),
            other => prop_assert!(false, "expected DTR, got {:?}", other.record_type()),
        }
        match &records[5] {
            StdfRecord::Mrr(mrr) => prop_assert_eq!(mrr.finish_t, spec.finish_t),
            other => prop_assert!(false, "expected MRR, got {:?}", other.record_type()),
        }
        match &records[6] {
            StdfRecord::Unknown { typ, sub, data } => {
                prop_assert_eq!(*typ, 99);
                prop_assert_eq!(*sub, 1);
                prop_assert_eq!(data.as_slice(), spec.unknown.as_slice());
            }
            other => prop_assert!(false, "expected unknown, got {:?}", other.record_type()),
        }
    }

    #[test]
    fn arbitrary_bytes_keep_decode_entry_points_aligned(data in prop::collection::vec(any::<u8>(), 0..512)) {
        decoding_invariants(&data).expect("arbitrary input invariants");
    }

    #[test]
    fn short_prefixes_do_not_change_decoded_records(data in prop::collection::vec(any::<u8>(), 0..96)) {
        prefix_stable(&data).expect("prefix stability");
    }
}

#[test]
fn golden_fixtures_satisfy_decode_invariants() {
    for (name, hex) in [
        (
            "minimal_le",
            include_str!("../../fixtures/stdf/golden/minimal_le.hex"),
        ),
        (
            "minimal_be",
            include_str!("../../fixtures/stdf/golden/minimal_be.hex"),
        ),
        (
            "ptr_prr_flow",
            include_str!("../../fixtures/stdf/golden/ptr_prr_flow.hex"),
        ),
        (
            "malformed_truncated",
            include_str!("../../fixtures/stdf/golden/malformed_truncated.hex"),
        ),
        (
            "unknown_vendor",
            include_str!("../../fixtures/stdf/golden/unknown_vendor.hex"),
        ),
    ] {
        let data = parse_hex(hex);
        decoding_invariants(&data).unwrap_or_else(|err| panic!("{name}: {err}"));
        prefix_stable(&data).unwrap_or_else(|err| panic!("{name} prefix: {err}"));
    }
}

fn parse_hex(input: &str) -> Vec<u8> {
    input
        .lines()
        .flat_map(|line| line.split('#').next().unwrap_or("").split_whitespace())
        .map(|token| u8::from_str_radix(token, 16).expect("fixture hex"))
        .collect()
}
