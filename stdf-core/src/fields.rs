use crate::error::{Result, StdfError};
use crate::types::{ByteOrder, VarData};

/// Cursor-based reader over a byte slice with endianness-aware reads.
/// Every record parser uses this to decode fields from the record body.
pub struct FieldReader<'a> {
    data: &'a [u8],
    pos: usize,
    byte_order: ByteOrder,
}

impl<'a> FieldReader<'a> {
    pub fn new(data: &'a [u8], byte_order: ByteOrder) -> Self {
        FieldReader {
            data,
            pos: 0,
            byte_order,
        }
    }

    /// Bytes remaining in the buffer.
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    /// Current cursor position.
    pub fn position(&self) -> usize {
        self.pos
    }

    fn ensure(&self, n: usize) -> Result<()> {
        if self.remaining() < n {
            Err(StdfError::UnexpectedEof {
                position: self.pos,
                expected: n,
            })
        } else {
            Ok(())
        }
    }

    fn read_bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        self.ensure(n)?;
        let slice = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(slice)
    }

    // ── Unsigned integers ──────────────────────────────────────────────

    /// U*1 — unsigned 1-byte integer
    pub fn read_u1(&mut self) -> Result<u8> {
        self.ensure(1)?;
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    /// U*2 — unsigned 2-byte integer, endian-aware
    pub fn read_u2(&mut self) -> Result<u16> {
        let b = self.read_bytes(2)?;
        Ok(match self.byte_order {
            ByteOrder::LittleEndian => u16::from_le_bytes([b[0], b[1]]),
            ByteOrder::BigEndian => u16::from_be_bytes([b[0], b[1]]),
        })
    }

    /// U*4 — unsigned 4-byte integer, endian-aware
    pub fn read_u4(&mut self) -> Result<u32> {
        let b = self.read_bytes(4)?;
        Ok(match self.byte_order {
            ByteOrder::LittleEndian => u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            ByteOrder::BigEndian => u32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        })
    }

    // ── Signed integers ────────────────────────────────────────────────

    /// I*1 — signed 1-byte integer
    pub fn read_i1(&mut self) -> Result<i8> {
        Ok(self.read_u1()? as i8)
    }

    /// I*2 — signed 2-byte integer, endian-aware
    pub fn read_i2(&mut self) -> Result<i16> {
        let b = self.read_bytes(2)?;
        Ok(match self.byte_order {
            ByteOrder::LittleEndian => i16::from_le_bytes([b[0], b[1]]),
            ByteOrder::BigEndian => i16::from_be_bytes([b[0], b[1]]),
        })
    }

    /// I*4 — signed 4-byte integer, endian-aware
    pub fn read_i4(&mut self) -> Result<i32> {
        let b = self.read_bytes(4)?;
        Ok(match self.byte_order {
            ByteOrder::LittleEndian => i32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            ByteOrder::BigEndian => i32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        })
    }

    // ── Floating point ─────────────────────────────────────────────────

    /// R*4 — IEEE 754 single-precision float, endian-aware
    pub fn read_r4(&mut self) -> Result<f32> {
        let b = self.read_bytes(4)?;
        Ok(match self.byte_order {
            ByteOrder::LittleEndian => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            ByteOrder::BigEndian => f32::from_be_bytes([b[0], b[1], b[2], b[3]]),
        })
    }

    /// R*8 — IEEE 754 double-precision float, endian-aware
    pub fn read_r8(&mut self) -> Result<f64> {
        let b = self.read_bytes(8)?;
        let mut arr = [0u8; 8];
        arr.copy_from_slice(b);
        Ok(match self.byte_order {
            ByteOrder::LittleEndian => f64::from_le_bytes(arr),
            ByteOrder::BigEndian => f64::from_be_bytes(arr),
        })
    }

    // ── Character / String types ───────────────────────────────────────

    /// C*1 — single character byte
    pub fn read_c1(&mut self) -> Result<u8> {
        self.read_u1()
    }

    /// C*n — variable-length string: first byte = length, then ASCII bytes
    pub fn read_cn(&mut self) -> Result<String> {
        let len = self.read_u1()? as usize;
        if len == 0 {
            return Ok(String::new());
        }
        let bytes = self.read_bytes(len)?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }

    /// S*n — V4-2007 string with an endian-aware U*2 byte length.
    pub fn read_sn(&mut self) -> Result<String> {
        let len = self.read_u2()? as usize;
        Ok(String::from_utf8_lossy(self.read_bytes(len)?).into_owned())
    }

    /// C*f — fixed-length string
    pub fn read_cf(&mut self, len: usize) -> Result<String> {
        let bytes = self.read_bytes(len)?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }

    // ── Binary types ───────────────────────────────────────────────────

    /// B*1 — single byte of bit flags
    pub fn read_b1(&mut self) -> Result<u8> {
        self.read_u1()
    }

    /// B*n — variable-length binary: first byte = byte count, then bytes
    pub fn read_bn(&mut self) -> Result<Vec<u8>> {
        let len = self.read_u1()? as usize;
        if len == 0 {
            return Ok(Vec::new());
        }
        Ok(self.read_bytes(len)?.to_vec())
    }

    /// D*n — bit-encoded data: first 2 bytes (U*2) = bit count, then ⌈bits/8⌉ bytes
    pub fn read_dn(&mut self) -> Result<(u16, Vec<u8>)> {
        let bit_count = self.read_u2()?;
        if bit_count == 0 {
            return Ok((0, Vec::new()));
        }
        let byte_count = (bit_count as usize + 7) / 8;
        let bytes = self.read_bytes(byte_count)?;
        Ok((bit_count, bytes.to_vec()))
    }

    // ── Nibble types ───────────────────────────────────────────────────

    /// N*1 — single nibble (4 bits). Reads one full byte, returns low nibble.
    pub fn read_n1(&mut self) -> Result<u8> {
        Ok(self.read_u1()? & 0x0F)
    }

    /// Read `count` nibbles packed 2 per byte (low nibble first).
    /// For odd count, high nibble of last byte is padding.
    pub fn read_nibble_array(&mut self, count: usize) -> Result<Vec<u8>> {
        if count == 0 {
            return Ok(Vec::new());
        }
        let byte_count = (count + 1) / 2;
        let bytes = self.read_bytes(byte_count)?;
        let mut result = Vec::with_capacity(count);
        for i in 0..count {
            let byte_idx = i / 2;
            let nibble = if i % 2 == 0 {
                bytes[byte_idx] & 0x0F // low nibble
            } else {
                (bytes[byte_idx] >> 4) & 0x0F // high nibble
            };
            result.push(nibble);
        }
        Ok(result)
    }

    // ── Array types ────────────────────────────────────────────────────

    pub fn read_u1_array(&mut self, count: usize) -> Result<Vec<u8>> {
        Ok(self.read_bytes(count)?.to_vec())
    }

    pub fn read_u2_array(&mut self, count: usize) -> Result<Vec<u16>> {
        let mut v = Vec::with_capacity(count);
        for _ in 0..count {
            v.push(self.read_u2()?);
        }
        Ok(v)
    }

    pub fn read_u4_array(&mut self, count: usize) -> Result<Vec<u32>> {
        let mut v = Vec::with_capacity(count);
        for _ in 0..count {
            v.push(self.read_u4()?);
        }
        Ok(v)
    }

    pub fn read_r4_array(&mut self, count: usize) -> Result<Vec<f32>> {
        let mut v = Vec::with_capacity(count);
        for _ in 0..count {
            v.push(self.read_r4()?);
        }
        Ok(v)
    }

    pub fn read_cn_array(&mut self, count: usize) -> Result<Vec<String>> {
        let mut v = Vec::with_capacity(count);
        for _ in 0..count {
            v.push(self.read_cn()?);
        }
        Ok(v)
    }

    // ── V*n variable type (for GDR) ────────────────────────────────────

    /// Read a single V*n typed value: type-code byte followed by value.
    pub fn read_vn(&mut self) -> Result<VarData> {
        let type_code = self.read_u1()?;
        match type_code {
            0 => Ok(VarData::Padding),
            1 => Ok(VarData::U1(self.read_u1()?)),
            2 => Ok(VarData::U2(self.read_u2()?)),
            3 => Ok(VarData::U4(self.read_u4()?)),
            4 => Ok(VarData::I1(self.read_i1()?)),
            5 => Ok(VarData::I2(self.read_i2()?)),
            6 => Ok(VarData::I4(self.read_i4()?)),
            7 => Ok(VarData::R4(self.read_r4()?)),
            8 => Ok(VarData::R8(self.read_r8()?)),
            10 => Ok(VarData::Cn(self.read_cn()?)),
            11 => Ok(VarData::Bn(self.read_bn()?)),
            12 => {
                let (bit_count, data) = self.read_dn()?;
                Ok(VarData::Dn { bit_count, data })
            }
            13 => Ok(VarData::N1(self.read_n1()?)),
            _ => Err(StdfError::InvalidField {
                record: "GDR",
                field: "GEN_DATA",
                msg: format!("unknown V*n type code: {}", type_code),
            }),
        }
    }

    /// Skip `n` bytes.
    pub fn skip(&mut self, n: usize) -> Result<()> {
        self.ensure(n)?;
        self.pos += n;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_u1() {
        let mut r = FieldReader::new(&[0x42], ByteOrder::LittleEndian);
        assert_eq!(r.read_u1().unwrap(), 0x42);
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn test_u2_le() {
        let mut r = FieldReader::new(&[0x34, 0x12], ByteOrder::LittleEndian);
        assert_eq!(r.read_u2().unwrap(), 0x1234);
    }

    #[test]
    fn test_u2_be() {
        let mut r = FieldReader::new(&[0x12, 0x34], ByteOrder::BigEndian);
        assert_eq!(r.read_u2().unwrap(), 0x1234);
    }

    #[test]
    fn test_u4_le() {
        let mut r = FieldReader::new(&[0x78, 0x56, 0x34, 0x12], ByteOrder::LittleEndian);
        assert_eq!(r.read_u4().unwrap(), 0x12345678);
    }

    #[test]
    fn test_u4_be() {
        let mut r = FieldReader::new(&[0x12, 0x34, 0x56, 0x78], ByteOrder::BigEndian);
        assert_eq!(r.read_u4().unwrap(), 0x12345678);
    }

    #[test]
    fn test_i2_negative_le() {
        let mut r = FieldReader::new(&[0xFE, 0xFF], ByteOrder::LittleEndian);
        assert_eq!(r.read_i2().unwrap(), -2);
    }

    #[test]
    fn test_i4_le() {
        let val: i32 = -12345;
        let bytes = val.to_le_bytes();
        let mut r = FieldReader::new(&bytes, ByteOrder::LittleEndian);
        assert_eq!(r.read_i4().unwrap(), -12345);
    }

    #[test]
    fn test_r4_le() {
        let val: f32 = 3.14;
        let bytes = val.to_le_bytes();
        let mut r = FieldReader::new(&bytes, ByteOrder::LittleEndian);
        assert!((r.read_r4().unwrap() - 3.14).abs() < 0.001);
    }

    #[test]
    fn test_r4_be() {
        let val: f32 = 3.14;
        let bytes = val.to_be_bytes();
        let mut r = FieldReader::new(&bytes, ByteOrder::BigEndian);
        assert!((r.read_r4().unwrap() - 3.14).abs() < 0.001);
    }

    #[test]
    fn test_r8_le() {
        let val: f64 = 2.718281828;
        let bytes = val.to_le_bytes();
        let mut r = FieldReader::new(&bytes, ByteOrder::LittleEndian);
        assert!((r.read_r8().unwrap() - 2.718281828).abs() < 1e-6);
    }

    #[test]
    fn test_cn() {
        let data = [0x05, b'H', b'e', b'l', b'l', b'o'];
        let mut r = FieldReader::new(&data, ByteOrder::LittleEndian);
        assert_eq!(r.read_cn().unwrap(), "Hello");
    }

    #[test]
    fn test_cn_empty() {
        let mut r = FieldReader::new(&[0x00], ByteOrder::LittleEndian);
        assert_eq!(r.read_cn().unwrap(), "");
    }

    #[test]
    fn test_bn() {
        let data = [0x03, 0xAA, 0xBB, 0xCC];
        let mut r = FieldReader::new(&data, ByteOrder::LittleEndian);
        assert_eq!(r.read_bn().unwrap(), vec![0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn test_dn() {
        let data = [0x0C, 0x00, 0xAB, 0xCD]; // 12 bits LE
        let mut r = FieldReader::new(&data, ByteOrder::LittleEndian);
        let (bits, bytes) = r.read_dn().unwrap();
        assert_eq!(bits, 12);
        assert_eq!(bytes, vec![0xAB, 0xCD]);
    }

    #[test]
    fn test_nibble_array_odd() {
        // 3 nibbles: byte0 = 0xBA → low=0xA, high=0xB; byte1 = 0x0C → low=0xC
        let data = [0xBA, 0x0C];
        let mut r = FieldReader::new(&data, ByteOrder::LittleEndian);
        assert_eq!(r.read_nibble_array(3).unwrap(), vec![0x0A, 0x0B, 0x0C]);
    }

    #[test]
    fn test_nibble_array_even() {
        let data = [0x21, 0x43]; // low=1,high=2, low=3,high=4
        let mut r = FieldReader::new(&data, ByteOrder::LittleEndian);
        assert_eq!(r.read_nibble_array(4).unwrap(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_vn_types() {
        // U1
        let mut r = FieldReader::new(&[1, 0x42], ByteOrder::LittleEndian);
        assert_eq!(r.read_vn().unwrap(), VarData::U1(0x42));

        // Cn
        let mut r = FieldReader::new(&[10, 3, b'A', b'B', b'C'], ByteOrder::LittleEndian);
        assert_eq!(r.read_vn().unwrap(), VarData::Cn("ABC".to_string()));

        // Padding
        let mut r = FieldReader::new(&[0], ByteOrder::LittleEndian);
        assert_eq!(r.read_vn().unwrap(), VarData::Padding);
    }

    #[test]
    fn test_eof_error() {
        let mut r = FieldReader::new(&[0x01], ByteOrder::LittleEndian);
        assert!(r.read_u2().is_err());
    }

    #[test]
    fn test_sequential_reads() {
        let mut data = Vec::new();
        data.extend_from_slice(&42u16.to_le_bytes());
        data.push(3);
        data.extend_from_slice(b"ABC");
        data.extend_from_slice(&3.14f32.to_le_bytes());

        let mut r = FieldReader::new(&data, ByteOrder::LittleEndian);
        assert_eq!(r.read_u2().unwrap(), 42);
        assert_eq!(r.read_cn().unwrap(), "ABC");
        assert!((r.read_r4().unwrap() - 3.14).abs() < 0.001);
        assert_eq!(r.remaining(), 0);
    }

    #[test]
    fn test_skip() {
        let mut r = FieldReader::new(&[1, 2, 3, 4, 5], ByteOrder::LittleEndian);
        r.skip(3).unwrap();
        assert_eq!(r.read_u1().unwrap(), 4);
    }
}
