use crate::error::Result;
use crate::header::RecordHeader;
use crate::records::{decode_record, StdfRecord};
use crate::types::ByteOrder;

/// A raw (header + body bytes) record, before decode.
/// Used by the iterator to decouple framing from parsing.
#[derive(Debug, Clone)]
pub struct RawRecord<'a> {
    pub header: RecordHeader,
    pub body: &'a [u8],
    /// Byte offset of this record's header in the source buffer.
    pub offset: usize,
}

/// Iterator that walks a byte slice, yielding one `RawRecord` at a time.
/// Does NOT decode — just frames. Decoding is a separate step so callers
/// can filter by record type before paying the parse cost.
pub struct RawRecordIter<'a> {
    data: &'a [u8],
    pos: usize,
    byte_order: ByteOrder,
}

impl<'a> RawRecordIter<'a> {
    /// Create a new iterator. Caller must supply the byte order
    /// (typically detected from FAR beforehand).
    pub fn new(data: &'a [u8], byte_order: ByteOrder) -> Self {
        RawRecordIter {
            data,
            pos: 0,
            byte_order,
        }
    }

    /// Current byte position in the buffer.
    pub fn position(&self) -> usize {
        self.pos
    }
}

impl<'a> Iterator for RawRecordIter<'a> {
    type Item = Result<RawRecord<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.pos + 4 > self.data.len() {
            if self.pos < self.data.len() {
                // Trailing bytes that can't form a header — report as truncation
                let err = crate::error::StdfError::UnexpectedEof {
                    position: self.pos,
                    expected: 4,
                };
                self.pos = self.data.len(); // consume to stop iteration
                return Some(Err(err));
            }
            return None; // clean end
        }

        let offset = self.pos;
        let header = RecordHeader::from_bytes(
            &[
                self.data[self.pos],
                self.data[self.pos + 1],
                self.data[self.pos + 2],
                self.data[self.pos + 3],
            ],
            self.byte_order,
        );
        self.pos += 4;

        let body_len = header.rec_len as usize;
        if self.pos + body_len > self.data.len() {
            let err = crate::error::StdfError::UnexpectedEof {
                position: self.pos,
                expected: body_len,
            };
            self.pos = self.data.len();
            return Some(Err(err));
        }

        let body = &self.data[self.pos..self.pos + body_len];
        self.pos += body_len;

        Some(Ok(RawRecord {
            header,
            body,
            offset,
        }))
    }
}

/// Iterator that decodes raw records into typed `StdfRecord` values.
/// On decode failure, yields `StdfRecord::Unknown` (graceful degradation).
pub struct RecordIter<'a> {
    inner: RawRecordIter<'a>,
}

impl<'a> RecordIter<'a> {
    pub fn new(data: &'a [u8], byte_order: ByteOrder) -> Self {
        RecordIter {
            inner: RawRecordIter::new(data, byte_order),
        }
    }
}

impl<'a> Iterator for RecordIter<'a> {
    type Item = Result<StdfRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        let raw = match self.inner.next()? {
            Ok(raw) => raw,
            Err(e) => return Some(Err(e)),
        };

        let record = match decode_record(&raw.header, raw.body, self.inner.byte_order) {
            Ok(rec) => rec,
            Err(_) => StdfRecord::Unknown {
                typ: raw.header.rec_typ,
                sub: raw.header.rec_sub,
                data: raw.body.to_vec(),
            },
        };

        Some(Ok(record))
    }
}

/// Detect byte order from the first 6 bytes of an STDF buffer (the FAR record).
/// Returns `None` if the buffer is too short or doesn't start with a valid FAR.
pub fn detect_byte_order(data: &[u8]) -> Option<ByteOrder> {
    if data.len() < 6 {
        return None;
    }

    // Try little-endian first (most common — Teradyne)
    let header_le = RecordHeader::from_bytes(
        &[data[0], data[1], data[2], data[3]],
        ByteOrder::LittleEndian,
    );
    if header_le.rec_typ == 0 && header_le.rec_sub == 10 && header_le.rec_len == 2 && data[4] == 2 {
        return Some(ByteOrder::LittleEndian);
    }

    // Try big-endian (Advantest / Sun)
    let header_be =
        RecordHeader::from_bytes(&[data[0], data[1], data[2], data[3]], ByteOrder::BigEndian);
    if header_be.rec_typ == 0 && header_be.rec_sub == 10 && header_be.rec_len == 2 && data[4] == 1 {
        return Some(ByteOrder::BigEndian);
    }

    None
}

/// Detect byte order from FAR and return a typed error when detection fails.
pub fn detect_byte_order_result(data: &[u8]) -> Result<ByteOrder> {
    if data.len() < 6 {
        return Err(crate::error::StdfError::UnexpectedEof {
            position: 0,
            expected: 6,
        });
    }

    let header_le = RecordHeader::from_bytes(
        &[data[0], data[1], data[2], data[3]],
        ByteOrder::LittleEndian,
    );
    let header_be =
        RecordHeader::from_bytes(&[data[0], data[1], data[2], data[3]], ByteOrder::BigEndian);
    let is_le_far = header_le.rec_typ == 0 && header_le.rec_sub == 10 && header_le.rec_len == 2;
    let is_be_far = header_be.rec_typ == 0 && header_be.rec_sub == 10 && header_be.rec_len == 2;

    let byte_order = if is_le_far && data[4] == 2 {
        ByteOrder::LittleEndian
    } else if is_be_far && data[4] == 1 {
        ByteOrder::BigEndian
    } else if is_le_far || is_be_far {
        return Err(crate::error::StdfError::InvalidField {
            record: "FAR",
            field: "CPU_TYPE",
            msg: format!("unsupported or inconsistent CPU_TYPE {}", data[4]),
        });
    } else {
        return Err(crate::error::StdfError::InvalidFar {
            typ: data[2],
            sub: data[3],
        });
    };

    if data[5] != 4 {
        return Err(crate::error::StdfError::UnsupportedVersion(data[5]));
    }

    Ok(byte_order)
}

/// Create a raw record iterator after detecting byte order from FAR.
pub fn raw_records(data: &[u8]) -> Result<RawRecordIter<'_>> {
    Ok(RawRecordIter::new(data, detect_byte_order_result(data)?))
}

/// Create a typed record iterator after detecting byte order from FAR.
pub fn records(data: &[u8]) -> Result<RecordIter<'_>> {
    Ok(RecordIter::new(data, detect_byte_order_result(data)?))
}

/// Summary statistics from a decoded STDF file.
#[derive(Debug, Clone, Default)]
pub struct DecodeSummary {
    pub total_records: usize,
    pub record_counts: std::collections::HashMap<crate::types::RecordType, usize>,
    pub byte_order: Option<ByteOrder>,
    pub bytes_consumed: usize,
    pub truncated: bool,
    pub errors: Vec<String>,
}

/// One-shot decode output with records and the same summary downstream crates use.
#[derive(Debug)]
pub struct DecodeResult {
    pub records: Vec<StdfRecord>,
    pub summary: DecodeSummary,
    pub error: Option<crate::error::StdfError>,
}

/// Iterate and collect summary statistics without materializing all records.
pub fn summarize(data: &[u8]) -> DecodeSummary {
    let mut summary = DecodeSummary::default();

    let byte_order = match detect_byte_order_result(data) {
        Ok(byte_order) => {
            summary.byte_order = Some(byte_order);
            byte_order
        }
        Err(err) => {
            summary.errors.push(err.to_string());
            return summary;
        }
    };

    let iter = RawRecordIter::new(data, byte_order);
    for item in iter {
        match item {
            Ok(raw) => {
                summary.total_records += 1;
                summary.bytes_consumed = raw.offset + 4 + raw.header.rec_len as usize;
                let rt = raw.header.record_type();
                *summary.record_counts.entry(rt).or_insert(0) += 1;
            }
            Err(e) => {
                summary.truncated = true;
                summary.errors.push(format!("{}", e));
            }
        }
    }

    summary
}

/// Decode all available records and return both records and structured summary.
///
/// A trailing truncated record stops decoding and is reported in `error` and
/// `summary.errors`; already decoded records are preserved.
pub fn decode_with_summary(data: &[u8]) -> DecodeResult {
    let mut summary = DecodeSummary::default();
    let byte_order = match detect_byte_order_result(data) {
        Ok(byte_order) => {
            summary.byte_order = Some(byte_order);
            byte_order
        }
        Err(err) => {
            summary.errors.push(err.to_string());
            return DecodeResult {
                records: Vec::new(),
                summary,
                error: Some(err),
            };
        }
    };

    let mut records = Vec::new();
    let iter = RawRecordIter::new(data, byte_order);
    for item in iter {
        match item {
            Ok(raw) => {
                summary.total_records += 1;
                summary.bytes_consumed = raw.offset + 4 + raw.header.rec_len as usize;
                *summary
                    .record_counts
                    .entry(raw.header.record_type())
                    .or_insert(0) += 1;

                let record = match decode_record(&raw.header, raw.body, byte_order) {
                    Ok(record) => record,
                    Err(_) => StdfRecord::Unknown {
                        typ: raw.header.rec_typ,
                        sub: raw.header.rec_sub,
                        data: raw.body.to_vec(),
                    },
                };
                records.push(record);
            }
            Err(err) => {
                summary.truncated = true;
                summary.errors.push(err.to_string());
                return DecodeResult {
                    records,
                    summary,
                    error: Some(err),
                };
            }
        }
    }

    DecodeResult {
        records,
        summary,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RecordType;

    /// Build a minimal LE STDF: FAR + PIR + PTR + PRR + MRR
    fn build_iter_test_stdf() -> Vec<u8> {
        let mut buf = Vec::new();

        // FAR
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.push(0);
        buf.push(10);
        buf.push(2);
        buf.push(4);

        // PIR: head=1, site=0
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.push(5);
        buf.push(10);
        buf.push(1);
        buf.push(0);

        // PTR: test_num=100, head=1, site=0, pass, result=1.5
        let mut ptr = Vec::new();
        ptr.extend_from_slice(&100u32.to_le_bytes());
        ptr.push(1);
        ptr.push(0);
        ptr.push(0x00);
        ptr.push(0x00);
        ptr.extend_from_slice(&1.5f32.to_le_bytes());
        buf.extend_from_slice(&(ptr.len() as u16).to_le_bytes());
        buf.push(15);
        buf.push(10);
        buf.extend_from_slice(&ptr);

        // PRR: head=1, site=0, pass
        let mut prr = Vec::new();
        prr.push(1);
        prr.push(0);
        prr.push(0x00);
        prr.extend_from_slice(&1u16.to_le_bytes());
        prr.extend_from_slice(&1u16.to_le_bytes());
        prr.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&(prr.len() as u16).to_le_bytes());
        buf.push(5);
        buf.push(20);
        buf.extend_from_slice(&prr);

        // MRR: finish_t=9999
        buf.extend_from_slice(&4u16.to_le_bytes());
        buf.push(1);
        buf.push(20);
        buf.extend_from_slice(&9999u32.to_le_bytes());

        buf
    }

    // ── Test: detect_byte_order ──────────────────────────────────────

    #[test]
    fn test_detect_byte_order_le() {
        let mut data = Vec::new();
        data.extend_from_slice(&2u16.to_le_bytes());
        data.push(0);
        data.push(10);
        data.push(2);
        data.push(4); // cpu_type=2 (LE)
        assert_eq!(detect_byte_order(&data), Some(ByteOrder::LittleEndian));
    }

    #[test]
    fn test_detect_byte_order_be() {
        let mut data = Vec::new();
        data.extend_from_slice(&2u16.to_be_bytes());
        data.push(0);
        data.push(10);
        data.push(1);
        data.push(4); // cpu_type=1 (BE)
        assert_eq!(detect_byte_order(&data), Some(ByteOrder::BigEndian));
    }

    #[test]
    fn test_detect_byte_order_too_short() {
        assert_eq!(detect_byte_order(&[0x02, 0x00]), None);
    }

    #[test]
    fn test_detect_byte_order_not_far() {
        let data = [0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(detect_byte_order(&data), None);
    }

    // ── Test: RawRecordIter ──────────────────────────────────────────

    #[test]
    fn test_raw_iter_counts() {
        let data = build_iter_test_stdf();
        let iter = RawRecordIter::new(&data, ByteOrder::LittleEndian);
        let records: Vec<_> = iter.collect::<Vec<_>>();
        assert_eq!(records.len(), 5); // FAR, PIR, PTR, PRR, MRR
        assert!(records.iter().all(|r| r.is_ok()));
    }

    #[test]
    fn test_raw_iter_record_types() {
        let data = build_iter_test_stdf();
        let types: Vec<RecordType> = RawRecordIter::new(&data, ByteOrder::LittleEndian)
            .filter_map(|r| r.ok())
            .map(|r| r.header.record_type())
            .collect();
        assert_eq!(
            types,
            vec![
                RecordType::Far,
                RecordType::Pir,
                RecordType::Ptr,
                RecordType::Prr,
                RecordType::Mrr,
            ]
        );
    }

    #[test]
    fn test_raw_iter_offsets_sequential() {
        let data = build_iter_test_stdf();
        let offsets: Vec<usize> = RawRecordIter::new(&data, ByteOrder::LittleEndian)
            .filter_map(|r| r.ok())
            .map(|r| r.offset)
            .collect();
        // First record at 0, each subsequent at previous + 4 + rec_len
        assert_eq!(offsets[0], 0);
        for w in offsets.windows(2) {
            assert!(w[1] > w[0], "offsets must be strictly increasing");
        }
    }

    #[test]
    fn test_raw_iter_truncated_body() {
        let data = build_iter_test_stdf();
        let truncated = &data[..data.len() - 2]; // chop 2 bytes off MRR body
        let results: Vec<_> = RawRecordIter::new(truncated, ByteOrder::LittleEndian).collect();
        // Should get 4 Ok records + 1 Err (truncated MRR)
        let ok_count = results.iter().filter(|r| r.is_ok()).count();
        let err_count = results.iter().filter(|r| r.is_err()).count();
        assert_eq!(ok_count, 4);
        assert_eq!(err_count, 1);
    }

    #[test]
    fn test_raw_iter_trailing_bytes() {
        let mut data = build_iter_test_stdf();
        data.push(0xFF);
        data.push(0xAA); // 2 trailing bytes — not enough for header
        let results: Vec<_> = RawRecordIter::new(&data, ByteOrder::LittleEndian).collect();
        let err_count = results.iter().filter(|r| r.is_err()).count();
        assert_eq!(err_count, 1); // trailing bytes → error
    }

    #[test]
    fn test_raw_iter_empty() {
        let results: Vec<_> = RawRecordIter::new(&[], ByteOrder::LittleEndian).collect();
        assert!(results.is_empty());
    }

    // ── Test: RecordIter (decoded) ───────────────────────────────────

    #[test]
    fn test_record_iter_decodes_all() {
        let data = build_iter_test_stdf();
        let records: Vec<StdfRecord> = RecordIter::new(&data, ByteOrder::LittleEndian)
            .filter_map(|r| r.ok())
            .collect();
        assert_eq!(records.len(), 5);
        assert!(matches!(records[0], StdfRecord::Far(_)));
        assert!(matches!(records[1], StdfRecord::Pir(_)));
        assert!(matches!(records[2], StdfRecord::Ptr(_)));
        assert!(matches!(records[3], StdfRecord::Prr(_)));
        assert!(matches!(records[4], StdfRecord::Mrr(_)));
    }

    #[test]
    fn test_record_iter_ptr_values() {
        let data = build_iter_test_stdf();
        let ptrs: Vec<_> = RecordIter::new(&data, ByteOrder::LittleEndian)
            .filter_map(|r| r.ok())
            .filter_map(|r| match r {
                StdfRecord::Ptr(p) => Some(p),
                _ => None,
            })
            .collect();
        assert_eq!(ptrs.len(), 1);
        assert_eq!(ptrs[0].test_num, 100);
        assert!((ptrs[0].result - 1.5).abs() < 0.001);
    }

    #[test]
    fn test_record_iter_filter_by_type() {
        let data = build_iter_test_stdf();
        let bo = ByteOrder::LittleEndian;
        // Use RawRecordIter + type filter to skip decoding non-PTR records
        let ptrs_raw: Vec<_> = RawRecordIter::new(&data, bo)
            .filter_map(|r| r.ok())
            .filter(|r| r.header.record_type() == RecordType::Ptr)
            .collect();
        assert_eq!(ptrs_raw.len(), 1);
        // Now decode only the filtered one
        let decoded = decode_record(&ptrs_raw[0].header, ptrs_raw[0].body, bo).unwrap();
        assert!(matches!(decoded, StdfRecord::Ptr(_)));
    }

    // ── Test: summarize ──────────────────────────────────────────────

    #[test]
    fn test_summarize_basic() {
        let data = build_iter_test_stdf();
        let summary = summarize(&data);
        assert_eq!(summary.total_records, 5);
        assert_eq!(summary.byte_order, Some(ByteOrder::LittleEndian));
        assert!(!summary.truncated);
        assert!(summary.errors.is_empty());
        assert_eq!(summary.record_counts[&RecordType::Far], 1);
        assert_eq!(summary.record_counts[&RecordType::Ptr], 1);
    }

    #[test]
    fn test_summarize_truncated() {
        let data = build_iter_test_stdf();
        let truncated = &data[..data.len() - 2];
        let summary = summarize(truncated);
        assert_eq!(summary.total_records, 4); // FAR + PIR + PTR + PRR (MRR truncated)
        assert!(summary.truncated);
        assert!(!summary.errors.is_empty());
    }

    #[test]
    fn test_summarize_empty() {
        let summary = summarize(&[]);
        assert_eq!(summary.total_records, 0);
        assert!(!summary.errors.is_empty()); // "cannot detect byte order"
    }

    #[test]
    fn test_raw_records_auto_detects_byte_order() {
        let data = build_iter_test_stdf();
        let mut iter = raw_records(&data).unwrap();
        let first = iter.next().unwrap().unwrap();

        assert_eq!(first.header.record_type(), RecordType::Far);
        assert_eq!(first.offset, 0);
    }

    #[test]
    fn test_records_auto_detects_byte_order() {
        let data = build_iter_test_stdf();
        let records: Vec<StdfRecord> = records(&data).unwrap().collect::<Result<Vec<_>>>().unwrap();

        assert_eq!(records.len(), 5);
        assert!(matches!(records[0], StdfRecord::Far(_)));
        assert!(matches!(records[2], StdfRecord::Ptr(_)));
    }

    #[test]
    fn test_detect_byte_order_result_rejects_unsupported_version() {
        let mut data = Vec::new();
        data.extend_from_slice(&2u16.to_le_bytes());
        data.push(0);
        data.push(10);
        data.push(2);
        data.push(3);

        let err = detect_byte_order_result(&data).unwrap_err();
        assert!(matches!(
            err,
            crate::error::StdfError::UnsupportedVersion(3)
        ));
    }

    #[test]
    fn test_decode_with_summary_clean_file() {
        let data = build_iter_test_stdf();
        let decoded = decode_with_summary(&data);

        assert!(decoded.error.is_none());
        assert_eq!(decoded.records.len(), 5);
        assert_eq!(decoded.summary.total_records, 5);
        assert_eq!(decoded.summary.byte_order, Some(ByteOrder::LittleEndian));
        assert_eq!(decoded.summary.bytes_consumed, data.len());
        assert!(!decoded.summary.truncated);
        assert_eq!(decoded.summary.record_counts[&RecordType::Ptr], 1);
    }

    #[test]
    fn test_decode_with_summary_truncated_file() {
        let data = build_iter_test_stdf();
        let truncated = &data[..data.len() - 2];
        let decoded = decode_with_summary(truncated);

        assert_eq!(decoded.records.len(), 4);
        assert!(decoded.error.is_some());
        assert!(decoded.summary.truncated);
        assert_eq!(decoded.summary.total_records, 4);
        assert!(!decoded.summary.errors.is_empty());
    }
}
