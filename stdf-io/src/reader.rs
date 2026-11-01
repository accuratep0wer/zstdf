use std::fs;
use std::io::Read;
use std::path::Path;

use flate2::read::GzDecoder;
use memmap2::Mmap;

use crate::error::{IoError, IoResult};
use stdf_core::{
    decode_all, detect_byte_order_result, summarize, ByteOrder, DecodeSummary, RawRecordIter,
    RecordIter, StdfRecord,
};

use crate::streaming::StreamingRecordReader;

/// How the file data is held in memory.
#[derive(Debug)]
pub enum StdfSource {
    /// Memory-mapped file (zero-copy, cross-platform via memmap2).
    Mmap(Mmap),
    /// Fully buffered in memory (used for gzip or small files).
    Buffer(Vec<u8>),
}

impl AsRef<[u8]> for StdfSource {
    fn as_ref(&self) -> &[u8] {
        match self {
            StdfSource::Mmap(m) => m.as_ref(),
            StdfSource::Buffer(b) => b.as_slice(),
        }
    }
}

/// High-level STDF file reader.
///
/// Opens a file (plain or gzip-compressed), detects byte order from FAR,
/// and provides iterator and batch access to records.
pub struct StdfReader {
    source: StdfSource,
    byte_order: ByteOrder,
}

impl StdfReader {
    /// Open an STDF file. Auto-detects gzip by `.gz` extension and
    /// magic bytes. Uses mmap for plain files, buffer for gzip.
    #[allow(unsafe_code)]
    pub fn open<P: AsRef<Path>>(path: P) -> IoResult<Self> {
        let path = path.as_ref();
        let source = if is_gzip(path)? {
            let file = fs::File::open(path)?;
            let mut decoder = GzDecoder::new(file);
            let mut buf = Vec::new();
            decoder
                .read_to_end(&mut buf)
                .map_err(|e| IoError::Gzip(e.to_string()))?;
            StdfSource::Buffer(buf)
        } else {
            let file = fs::File::open(path)?;
            // SAFETY: memmap2's read-only map keeps `file` alive with the map.
            // Callers use this for a local STDF file that is not truncated
            // while the reader exists. Gzip input does not take this path.
            let mmap = unsafe { Mmap::map(&file)? };
            StdfSource::Mmap(mmap)
        };

        let data = source.as_ref();
        let byte_order = detect_byte_order_result(data)?;

        Ok(StdfReader { source, byte_order })
    }

    /// Create a reader from an in-memory buffer (useful for testing).
    pub fn from_bytes(data: Vec<u8>) -> IoResult<Self> {
        let byte_order = detect_byte_order_result(&data)?;
        Ok(StdfReader {
            source: StdfSource::Buffer(data),
            byte_order,
        })
    }

    /// Detected byte order.
    pub fn byte_order(&self) -> ByteOrder {
        self.byte_order
    }

    /// Raw bytes of the loaded file.
    pub fn data(&self) -> &[u8] {
        self.source.as_ref()
    }

    /// File size in bytes.
    pub fn len(&self) -> usize {
        self.source.as_ref().len()
    }

    /// Is the file empty?
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Iterate over raw (undecoced) records — cheapest option, allows
    /// filtering by record type before paying the parse cost.
    pub fn raw_records(&self) -> RawRecordIter<'_> {
        RawRecordIter::new(self.data(), self.byte_order)
    }

    /// Iterate over decoded records.
    pub fn records(&self) -> RecordIter<'_> {
        RecordIter::new(self.data(), self.byte_order)
    }

    /// Decode all records at once.
    pub fn decode_all(&self) -> (Vec<StdfRecord>, Option<stdf_core::StdfError>) {
        decode_all(self.data())
    }

    /// Collect summary statistics without materializing records.
    pub fn summarize(&self) -> DecodeSummary {
        summarize(self.data())
    }

    /// Stream records from a plain local file without mapping or buffering the whole file.
    pub fn stream_file<P: AsRef<Path>>(path: P) -> IoResult<StreamingRecordReader<fs::File>> {
        StreamingRecordReader::open(path)
    }
}

/// Check if a file is gzip-compressed (by extension or magic bytes).
fn is_gzip(path: &Path) -> IoResult<bool> {
    // Check extension first
    if let Some(ext) = path.extension() {
        if ext.eq_ignore_ascii_case("gz") {
            return Ok(true);
        }
    }
    // Check magic bytes
    let mut file = fs::File::open(path)?;
    let mut magic = [0u8; 2];
    match file.read_exact(&mut magic) {
        Ok(()) => Ok(magic == [0x1f, 0x8b]),
        Err(_) => Ok(false), // too short to be gzip
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use stdf_core::types::RecordType;

    /// Build a minimal LE STDF in memory: FAR + MIR + PIR + PTR + PRR + MRR
    fn build_test_stdf() -> Vec<u8> {
        let mut buf = Vec::new();

        // FAR
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.push(0);
        buf.push(10);
        buf.push(2);
        buf.push(4);

        // MIR (minimal mandatory)
        let mut mir = Vec::new();
        mir.extend_from_slice(&1000u32.to_le_bytes()); // setup_t
        mir.extend_from_slice(&2000u32.to_le_bytes()); // start_t
        mir.push(1); // stat_num
        mir.push(b'P');
        mir.push(b' ');
        mir.push(b' '); // mode/rtst/prot
        mir.extend_from_slice(&0u16.to_le_bytes()); // burn_tim
        mir.push(b' '); // cmod_cod
        mir.push(5);
        mir.extend_from_slice(b"LOT42");
        mir.push(4);
        mir.extend_from_slice(b"IC_A");
        mir.push(4);
        mir.extend_from_slice(b"ND01");
        mir.push(2);
        mir.extend_from_slice(b"J7");
        mir.push(4);
        mir.extend_from_slice(b"JOB1");
        buf.extend_from_slice(&(mir.len() as u16).to_le_bytes());
        buf.push(1);
        buf.push(10);
        buf.extend_from_slice(&mir);

        // PIR
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.push(5);
        buf.push(10);
        buf.push(1);
        buf.push(0);

        // PTR: test_num=100, result=2.5
        let mut ptr = Vec::new();
        ptr.extend_from_slice(&100u32.to_le_bytes());
        ptr.push(1);
        ptr.push(0);
        ptr.push(0x00);
        ptr.push(0x00);
        ptr.extend_from_slice(&2.5f32.to_le_bytes());
        buf.extend_from_slice(&(ptr.len() as u16).to_le_bytes());
        buf.push(15);
        buf.push(10);
        buf.extend_from_slice(&ptr);

        // PRR
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

        // MRR
        buf.extend_from_slice(&4u16.to_le_bytes());
        buf.push(1);
        buf.push(20);
        buf.extend_from_slice(&9999u32.to_le_bytes());

        buf
    }

    // ── TDD: from_bytes ──────────────────────────────────────────────

    #[test]
    fn test_from_bytes_valid() {
        let data = build_test_stdf();
        let reader = StdfReader::from_bytes(data.clone()).unwrap();
        assert_eq!(reader.byte_order(), ByteOrder::LittleEndian);
        assert_eq!(reader.len(), data.len());
        assert!(!reader.is_empty());
    }

    #[test]
    fn test_from_bytes_invalid() {
        let result = StdfReader::from_bytes(vec![0xFF; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_from_bytes_empty() {
        let result = StdfReader::from_bytes(vec![]);
        assert!(result.is_err());
    }

    // ── TDD: record iteration ────────────────────────────────────────

    #[test]
    fn test_records_count() {
        let reader = StdfReader::from_bytes(build_test_stdf()).unwrap();
        let records: Vec<_> = reader.records().filter_map(|r| r.ok()).collect();
        assert_eq!(records.len(), 6); // FAR + MIR + PIR + PTR + PRR + MRR
    }

    #[test]
    fn test_records_types() {
        let reader = StdfReader::from_bytes(build_test_stdf()).unwrap();
        let types: Vec<RecordType> = reader
            .records()
            .filter_map(|r| r.ok())
            .map(|r| r.record_type())
            .collect();
        assert_eq!(
            types,
            vec![
                RecordType::Far,
                RecordType::Mir,
                RecordType::Pir,
                RecordType::Ptr,
                RecordType::Prr,
                RecordType::Mrr,
            ]
        );
    }

    #[test]
    fn test_raw_records_filter_ptrs() {
        let reader = StdfReader::from_bytes(build_test_stdf()).unwrap();
        let ptrs: Vec<_> = reader
            .raw_records()
            .filter_map(|r| r.ok())
            .filter(|r| r.header.record_type() == RecordType::Ptr)
            .collect();
        assert_eq!(ptrs.len(), 1);
    }

    // ── TDD: decode_all ──────────────────────────────────────────────

    #[test]
    fn test_decode_all_via_reader() {
        let reader = StdfReader::from_bytes(build_test_stdf()).unwrap();
        let (records, err) = reader.decode_all();
        assert!(err.is_none());
        assert_eq!(records.len(), 6);
    }

    // ── TDD: summarize ───────────────────────────────────────────────

    #[test]
    fn test_summarize_via_reader() {
        let reader = StdfReader::from_bytes(build_test_stdf()).unwrap();
        let summary = reader.summarize();
        assert_eq!(summary.total_records, 6);
        assert_eq!(summary.record_counts[&RecordType::Ptr], 1);
        assert!(!summary.truncated);
    }

    // ── TDD: file I/O (temp file) ────────────────────────────────────

    #[test]
    fn test_open_plain_file() {
        let dir = std::env::temp_dir().join("zstdf_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_open.stdf");
        std::fs::write(&path, build_test_stdf()).unwrap();

        let reader = StdfReader::open(&path).unwrap();
        let (records, err) = reader.decode_all();
        assert!(err.is_none());
        assert_eq!(records.len(), 6);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_open_gzip_file() {
        let dir = std::env::temp_dir().join("zstdf_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test_open.stdf.gz");

        // Compress the test STDF with gzip
        let raw = build_test_stdf();
        let file = std::fs::File::create(&path).unwrap();
        let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        encoder.write_all(&raw).unwrap();
        encoder.finish().unwrap();

        let reader = StdfReader::open(&path).unwrap();
        assert_eq!(reader.byte_order(), ByteOrder::LittleEndian);
        let (records, err) = reader.decode_all();
        assert!(err.is_none());
        assert_eq!(records.len(), 6);

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn test_open_nonexistent_file() {
        let result = StdfReader::open("/nonexistent/path/test.stdf");
        assert!(result.is_err());
    }

    // ── TDD: parity — StdfReader.decode_all == stdf_core::decode_all ─

    #[test]
    fn test_parity_with_core_decode_all() {
        let data = build_test_stdf();
        let (core_records, core_err) = stdf_core::decode_all(&data);
        let reader = StdfReader::from_bytes(data).unwrap();
        let (io_records, io_err) = reader.decode_all();

        assert_eq!(core_records.len(), io_records.len());
        assert!(core_err.is_none());
        assert!(io_err.is_none());

        // Verify same record types in same order
        for (c, i) in core_records.iter().zip(io_records.iter()) {
            assert_eq!(c.record_type(), i.record_type());
        }
    }
}
