use std::io::{BufReader, Read};
use std::path::Path;

use crate::error::{IoError, IoResult};
use stdf_core::{
    detect_byte_order_result, records::decode_record, ByteOrder, RecordHeader, StdfError,
    StdfRecord,
};

/// A complete wire record, including decode failures for known record types.
/// Offsets refer to the decompressed input. Framing failures are returned separately.
pub struct RecordEvent {
    pub offset: usize,
    pub header: RecordHeader,
    pub body: Vec<u8>,
    pub decoded: Result<StdfRecord, StdfError>,
}

/// Incremental STDF decoder over any `Read` source.
pub struct StreamingRecordReader<R: Read> {
    reader: BufReader<R>,
    byte_order: ByteOrder,
    offset: usize,
    first_body: Option<Vec<u8>>,
    finished: bool,
}

impl<R: Read> StreamingRecordReader<R> {
    /// Build a streaming reader and consume only the FAR header/body needed
    /// for byte-order detection. The FAR record is still yielded first.
    pub fn new(inner: R) -> IoResult<Self> {
        let mut reader = BufReader::new(inner);
        let mut first = [0u8; 6];
        reader.read_exact(&mut first)?;

        let byte_order = detect_byte_order_result(&first)?;
        Ok(Self {
            reader,
            byte_order,
            offset: 0,
            first_body: Some(first[4..].to_vec()),
            finished: false,
        })
    }

    pub fn byte_order(&self) -> ByteOrder {
        self.byte_order
    }

    pub fn bytes_consumed(&self) -> usize {
        self.offset
    }

    pub fn next_event(&mut self) -> IoResult<Option<RecordEvent>> {
        if self.finished {
            return Ok(None);
        }

        let (header, body) = if let Some(body) = self.first_body.take() {
            (
                RecordHeader {
                    rec_len: body.len() as u16,
                    rec_typ: 0,
                    rec_sub: 10,
                },
                body,
            )
        } else {
            let mut header_bytes = [0u8; 4];
            let mut read = 0;
            while read < header_bytes.len() {
                match self.reader.read(&mut header_bytes[read..]) {
                    Ok(0) if read == 0 => {
                        self.finished = true;
                        return Ok(None);
                    }
                    Ok(0) => {
                        self.finished = true;
                        return Err(IoError::Decode(StdfError::UnexpectedEof {
                            position: self.offset,
                            expected: 4,
                        }));
                    }
                    Ok(n) => read += n,
                    Err(err) => return Err(err.into()),
                }
            }

            let header = RecordHeader::from_bytes(&header_bytes, self.byte_order);
            let mut body = vec![0u8; header.rec_len as usize];
            self.reader.read_exact(&mut body).map_err(|err| {
                self.finished = true;
                if err.kind() == std::io::ErrorKind::UnexpectedEof {
                    IoError::Decode(StdfError::UnexpectedEof {
                        position: self.offset + 4,
                        expected: header.rec_len as usize,
                    })
                } else {
                    IoError::Io(err)
                }
            })?;
            (header, body)
        };

        let offset = self.offset;
        self.offset += 4 + header.rec_len as usize;
        let decoded = decode_record(&header, &body, self.byte_order);
        Ok(Some(RecordEvent {
            offset,
            header,
            body,
            decoded,
        }))
    }
}

impl StreamingRecordReader<std::fs::File> {
    pub fn open<P: AsRef<Path>>(path: P) -> IoResult<Self> {
        Self::new(std::fs::File::open(path)?)
    }
}

impl<R: Read> Iterator for StreamingRecordReader<R> {
    type Item = IoResult<StdfRecord>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_event()
            .map(|event| {
                event.map(|e| {
                    e.decoded.unwrap_or_else(|_| StdfRecord::Unknown {
                        typ: e.header.rec_typ,
                        sub: e.header.rec_sub,
                        data: e.body,
                    })
                })
            })
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use stdf_core::{decode_with_summary, RecordType};

    fn push_record(buf: &mut Vec<u8>, typ: u8, sub: u8, body: &[u8]) {
        buf.extend_from_slice(&(body.len() as u16).to_le_bytes());
        buf.push(typ);
        buf.push(sub);
        buf.extend_from_slice(body);
    }

    fn build_stream_test_stdf() -> Vec<u8> {
        let mut buf = Vec::new();
        push_record(&mut buf, 0, 10, &[2, 4]);
        push_record(&mut buf, 5, 10, &[1, 0]);

        let mut ptr = Vec::new();
        ptr.extend_from_slice(&100u32.to_le_bytes());
        ptr.push(1);
        ptr.push(0);
        ptr.push(0);
        ptr.push(0);
        ptr.extend_from_slice(&1.5f32.to_le_bytes());
        push_record(&mut buf, 15, 10, &ptr);

        let mut prr = Vec::new();
        prr.push(1);
        prr.push(0);
        prr.push(0);
        prr.extend_from_slice(&1u16.to_le_bytes());
        prr.extend_from_slice(&1u16.to_le_bytes());
        prr.extend_from_slice(&1u16.to_le_bytes());
        push_record(&mut buf, 5, 20, &prr);

        buf
    }

    #[test]
    fn streaming_reader_decodes_from_read_source() {
        let data = build_stream_test_stdf();
        let reader = StreamingRecordReader::new(Cursor::new(data.clone())).unwrap();
        let records = reader.collect::<IoResult<Vec<_>>>().unwrap();
        let core = decode_with_summary(&data);

        assert_eq!(records.len(), core.records.len());
        assert_eq!(records[0].record_type(), RecordType::Far);
        assert_eq!(records[2].record_type(), RecordType::Ptr);
    }

    #[test]
    fn streaming_reader_reports_bytes_consumed() {
        let data = build_stream_test_stdf();
        let mut reader = StreamingRecordReader::new(Cursor::new(data.clone())).unwrap();

        assert_eq!(reader.bytes_consumed(), 0);
        assert!(reader.next().unwrap().is_ok());
        assert_eq!(reader.bytes_consumed(), 6);
        while let Some(record) = reader.next() {
            record.unwrap();
        }
        assert_eq!(reader.bytes_consumed(), data.len());
    }

    #[test]
    fn streaming_reader_opens_plain_file() {
        let dir = std::env::temp_dir().join("zstdf_streaming_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("stream.stdf");
        let data = build_stream_test_stdf();
        std::fs::write(&path, &data).unwrap();

        let reader = StreamingRecordReader::open(&path).unwrap();
        let records = reader.collect::<IoResult<Vec<_>>>().unwrap();

        assert_eq!(records.len(), 4);
        assert_eq!(records[0].record_type(), RecordType::Far);

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn streaming_reader_reports_truncated_body() {
        let data = build_stream_test_stdf();
        let truncated = &data[..data.len() - 2];
        let mut reader = StreamingRecordReader::new(Cursor::new(truncated.to_vec())).unwrap();

        assert!(reader.next().unwrap().is_ok());
        assert!(reader.next().unwrap().is_ok());
        assert!(reader.next().unwrap().is_ok());
        assert!(reader.next().unwrap().is_err());
        assert!(reader.next().is_none());
    }

    #[test]
    fn event_retains_known_decode_error_and_raw_offset() {
        let mut bytes = vec![2, 0, 0, 10, 2, 4];
        push_record(&mut bytes, 5, 20, &[1]);
        let mut reader = StreamingRecordReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(reader.next_event().unwrap().unwrap().offset, 0);
        let event = reader.next_event().unwrap().unwrap();
        assert_eq!(event.offset, 6);
        assert_eq!(event.body, [1]);
        assert!(event.decoded.is_err());
        assert!(reader.next_event().unwrap().is_none());
    }
}
