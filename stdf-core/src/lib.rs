// Copyright 2026 zstdf contributors
// SPDX-License-Identifier: Apache-2.0

pub mod error;
pub mod fields;
pub mod header;
pub mod iter;
pub mod records;
pub mod types;

pub use error::{Result, StdfError};
pub use fields::FieldReader;
pub use header::RecordHeader;
pub use iter::{
    decode_with_summary, detect_byte_order, detect_byte_order_result, raw_records, records,
    summarize, DecodeResult, DecodeSummary, RawRecord, RawRecordIter, RecordIter,
};
pub use records::StdfRecord;
pub use types::*;

#[cfg(test)]
mod hardening_tests;

/// Decode all records from a complete STDF file byte buffer.
///
/// Returns `(records, optional_error)`. On truncation or invalid FAR,
/// returns partial results + the trailing error — never panics.
///
/// For streaming use, prefer [`RecordIter`] or [`RawRecordIter`].
pub fn decode_all(data: &[u8]) -> (Vec<StdfRecord>, Option<StdfError>) {
    let byte_order = match detect_byte_order(data) {
        Some(bo) => bo,
        None => {
            if data.len() < 6 {
                return (
                    Vec::new(),
                    Some(StdfError::UnexpectedEof {
                        position: 0,
                        expected: 6,
                    }),
                );
            }
            return (
                Vec::new(),
                Some(StdfError::InvalidFar {
                    typ: data[2],
                    sub: data[3],
                }),
            );
        }
    };

    let mut records = Vec::new();
    let mut last_error = None;

    for item in RecordIter::new(data, byte_order) {
        match item {
            Ok(record) => records.push(record),
            Err(e) => {
                last_error = Some(e);
                break; // truncation — stop
            }
        }
    }

    (records, last_error)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_record(buf: &mut Vec<u8>, typ: u8, sub: u8, body: &[u8]) {
        buf.extend_from_slice(&(body.len() as u16).to_le_bytes());
        buf.push(typ);
        buf.push(sub);
        buf.extend_from_slice(body);
    }

    /// Helper: build a minimal valid STDF file in memory (little-endian).
    fn build_test_stdf() -> Vec<u8> {
        let mut buf = Vec::new();

        // FAR: rec_len=2, typ=0, sub=10, cpu_type=2(LE), stdf_ver=4
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.push(0);
        buf.push(10);
        buf.push(2);
        buf.push(4);

        // MIR (mandatory fields only)
        let mut mir_body = Vec::new();
        mir_body.extend_from_slice(&1000u32.to_le_bytes()); // setup_t
        mir_body.extend_from_slice(&2000u32.to_le_bytes()); // start_t
        mir_body.push(1); // stat_num
        mir_body.push(b'P'); // mode_cod
        mir_body.push(b' '); // rtst_cod
        mir_body.push(b' '); // prot_cod
        mir_body.extend_from_slice(&0u16.to_le_bytes()); // burn_tim
        mir_body.push(b' '); // cmod_cod
        mir_body.push(6);
        mir_body.extend_from_slice(b"LOT001");
        mir_body.push(6);
        mir_body.extend_from_slice(b"CHIP_A");
        mir_body.push(4);
        mir_body.extend_from_slice(b"ND01");
        mir_body.push(2);
        mir_body.extend_from_slice(b"J7");
        mir_body.push(8);
        mir_body.extend_from_slice(b"TEST_001");
        buf.extend_from_slice(&(mir_body.len() as u16).to_le_bytes());
        buf.push(1);
        buf.push(10);
        buf.extend_from_slice(&mir_body);

        // PIR: head=1, site=0
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.push(5);
        buf.push(10);
        buf.push(1);
        buf.push(0);

        // PTR: test_num=1001, head=1, site=0, pass, result=3.14
        let mut ptr_body = Vec::new();
        ptr_body.extend_from_slice(&1001u32.to_le_bytes());
        ptr_body.push(1);
        ptr_body.push(0);
        ptr_body.push(0x00);
        ptr_body.push(0x00);
        ptr_body.extend_from_slice(&3.14f32.to_le_bytes());
        ptr_body.push(7);
        ptr_body.extend_from_slice(b"VDD_MIN");
        buf.extend_from_slice(&(ptr_body.len() as u16).to_le_bytes());
        buf.push(15);
        buf.push(10);
        buf.extend_from_slice(&ptr_body);

        // PRR: head=1, site=0, pass
        let mut prr_body = Vec::new();
        prr_body.push(1);
        prr_body.push(0);
        prr_body.push(0x00);
        prr_body.extend_from_slice(&1u16.to_le_bytes());
        prr_body.extend_from_slice(&1u16.to_le_bytes());
        prr_body.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&(prr_body.len() as u16).to_le_bytes());
        buf.push(5);
        buf.push(20);
        buf.extend_from_slice(&prr_body);

        // MRR: finish_t=3000
        let mut mrr_body = Vec::new();
        mrr_body.extend_from_slice(&3000u32.to_le_bytes());
        buf.extend_from_slice(&(mrr_body.len() as u16).to_le_bytes());
        buf.push(1);
        buf.push(20);
        buf.extend_from_slice(&mrr_body);

        buf
    }

    #[test]
    fn test_decode_all_basic() {
        let data = build_test_stdf();
        let (records, err) = decode_all(&data);
        assert!(err.is_none(), "unexpected error: {:?}", err);
        // FAR + MIR + PIR + PTR + PRR + MRR = 6
        assert_eq!(records.len(), 6);

        // Verify FAR
        match &records[0] {
            StdfRecord::Far(far) => {
                assert_eq!(far.cpu_type, 2);
                assert_eq!(far.stdf_ver, 4);
            }
            other => panic!("expected FAR, got {:?}", other.record_type()),
        }

        // Verify MIR
        match &records[1] {
            StdfRecord::Mir(mir) => {
                assert_eq!(mir.lot_id, "LOT001");
                assert_eq!(mir.part_typ, "CHIP_A");
                assert_eq!(mir.job_nam, "TEST_001");
            }
            other => panic!("expected MIR, got {:?}", other.record_type()),
        }

        // Verify PTR
        match &records[3] {
            StdfRecord::Ptr(ptr) => {
                assert_eq!(ptr.test_num, 1001);
                assert!((ptr.result - 3.14).abs() < 0.01);
                assert_eq!(ptr.test_txt, Some("VDD_MIN".to_string()));
                assert!(ptr.passed());
            }
            other => panic!("expected PTR, got {:?}", other.record_type()),
        }

        // Verify PRR
        match &records[4] {
            StdfRecord::Prr(prr) => {
                assert!(prr.passed());
                assert_eq!(prr.hard_bin, 1);
            }
            other => panic!("expected PRR, got {:?}", other.record_type()),
        }
    }

    #[test]
    fn test_decode_all_truncated_graceful() {
        let data = build_test_stdf();
        // Truncate in the middle of the last record
        let truncated = &data[..data.len() - 2];
        let (records, err) = decode_all(truncated);
        // Should have decoded at least FAR + MIR + PIR + PTR + PRR = 5
        assert!(records.len() >= 5);
        assert!(err.is_some()); // truncation error
    }

    #[test]
    fn test_decode_all_empty() {
        let (records, err) = decode_all(&[]);
        assert!(records.is_empty());
        assert!(err.is_some());
    }

    #[test]
    fn test_decode_all_unknown_record() {
        let mut data = Vec::new();
        // Valid FAR
        data.extend_from_slice(&2u16.to_le_bytes());
        data.push(0);
        data.push(10);
        data.push(2);
        data.push(4);
        // Unknown record: typ=99, sub=99, body="hello"
        data.extend_from_slice(&5u16.to_le_bytes());
        data.push(99);
        data.push(99);
        data.extend_from_slice(b"hello");

        let (records, err) = decode_all(&data);
        assert!(err.is_none());
        assert_eq!(records.len(), 2);
        match &records[1] {
            StdfRecord::Unknown { typ, sub, data } => {
                assert_eq!(*typ, 99);
                assert_eq!(*sub, 99);
                assert_eq!(data, b"hello");
            }
            _ => panic!("expected Unknown"),
        }
    }

    #[test]
    fn test_byte_order_detection_be() {
        let mut data = Vec::new();
        // FAR with big-endian header: rec_len=2 as BE → [0x00, 0x02]
        data.extend_from_slice(&2u16.to_be_bytes());
        data.push(0);
        data.push(10);
        data.push(1); // cpu_type=1 (BE/Sun)
        data.push(4); // stdf_ver=4

        let (records, err) = decode_all(&data);
        assert!(err.is_none());
        assert_eq!(records.len(), 1);
        match &records[0] {
            StdfRecord::Far(far) => {
                assert_eq!(far.cpu_type, 1);
                assert_eq!(far.stdf_ver, 4);
            }
            _ => panic!("expected FAR"),
        }
    }

    #[test]
    fn test_phase2_mixed_records_decode_and_text() {
        let mut data = Vec::new();
        push_record(&mut data, 0, 10, &[2, 4]);
        push_record(&mut data, 5, 10, &[1, 0]);

        let mut mpr = Vec::new();
        mpr.extend_from_slice(&3001u32.to_le_bytes());
        mpr.push(1);
        mpr.push(0);
        mpr.push(0x00);
        mpr.push(0x00);
        mpr.extend_from_slice(&2u16.to_le_bytes());
        mpr.extend_from_slice(&2u16.to_le_bytes());
        mpr.push(0x10);
        mpr.extend_from_slice(&1.0f32.to_le_bytes());
        mpr.extend_from_slice(&2.0f32.to_le_bytes());
        mpr.push(3);
        mpr.extend_from_slice(b"MPR");
        push_record(&mut data, 15, 15, &mpr);

        let mut ftr = Vec::new();
        ftr.extend_from_slice(&4001u32.to_le_bytes());
        ftr.push(1);
        ftr.push(0);
        ftr.push(0x80);
        ftr.push(0x00);
        ftr.extend_from_slice(&0u32.to_le_bytes());
        ftr.extend_from_slice(&0u32.to_le_bytes());
        ftr.extend_from_slice(&1u32.to_le_bytes());
        ftr.extend_from_slice(&1u32.to_le_bytes());
        ftr.extend_from_slice(&0i32.to_le_bytes());
        ftr.extend_from_slice(&0i32.to_le_bytes());
        ftr.extend_from_slice(&0i16.to_le_bytes());
        ftr.extend_from_slice(&1u16.to_le_bytes());
        ftr.extend_from_slice(&0u16.to_le_bytes());
        ftr.extend_from_slice(&11u16.to_le_bytes());
        ftr.push(0x01);
        ftr.extend_from_slice(&1u16.to_le_bytes());
        ftr.push(0x01);
        ftr.push(0);
        ftr.push(0);
        ftr.push(0);
        ftr.push(4);
        ftr.extend_from_slice(b"FTR1");
        push_record(&mut data, 15, 20, &ftr);

        let mut prr = Vec::new();
        prr.push(1);
        prr.push(0);
        prr.push(0x08);
        prr.extend_from_slice(&2u16.to_le_bytes());
        prr.extend_from_slice(&10u16.to_le_bytes());
        prr.extend_from_slice(&20u16.to_le_bytes());
        push_record(&mut data, 5, 20, &prr);

        let mut pcr = Vec::new();
        pcr.push(1);
        pcr.push(0);
        pcr.extend_from_slice(&1u32.to_le_bytes());
        pcr.extend_from_slice(&0u32.to_le_bytes());
        pcr.extend_from_slice(&0u32.to_le_bytes());
        pcr.extend_from_slice(&0u32.to_le_bytes());
        pcr.extend_from_slice(&1u32.to_le_bytes());
        push_record(&mut data, 1, 30, &pcr);

        let mut gdr = Vec::new();
        gdr.extend_from_slice(&2u16.to_le_bytes());
        gdr.push(1);
        gdr.push(42);
        gdr.push(10);
        gdr.push(3);
        gdr.extend_from_slice(b"ABC");
        push_record(&mut data, 50, 10, &gdr);

        let mut dtr = Vec::new();
        dtr.push(4);
        dtr.extend_from_slice(b"note");
        push_record(&mut data, 50, 30, &dtr);

        let (records, err) = decode_all(&data);
        assert!(err.is_none(), "unexpected decode error: {:?}", err);
        assert_eq!(records.len(), 8);
        assert!(matches!(records[2], StdfRecord::Mpr(_)));
        assert!(matches!(records[3], StdfRecord::Ftr(_)));
        assert!(matches!(records[5], StdfRecord::Pcr(_)));
        assert!(matches!(records[6], StdfRecord::Gdr(_)));
        assert!(matches!(records[7], StdfRecord::Dtr(_)));

        let text_lines: Vec<String> = records.iter().map(StdfRecord::to_text_line).collect();
        assert!(text_lines
            .iter()
            .any(|line| line.contains("MPR TEST_NUM=3001")));
        assert!(text_lines
            .iter()
            .any(|line| line.contains("FTR TEST_NUM=4001")));
        assert!(text_lines.iter().any(|line| line.contains("GDR FLD_CNT=2")));
        assert!(text_lines
            .iter()
            .any(|line| line.contains("DTR TEXT_DAT=note")));
    }
}
