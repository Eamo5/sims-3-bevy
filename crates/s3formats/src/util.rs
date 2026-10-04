//! Little-endian byte cursor.

#[derive(Clone)]
pub struct Reader<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

#[derive(Debug)]
pub struct Eof;

impl std::fmt::Display for Eof {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unexpected end of data")
    }
}

impl std::error::Error for Eof {}

pub type R<T> = Result<T, Eof>;

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }
    pub fn at(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
    pub fn bytes(&mut self, n: usize) -> R<&'a [u8]> {
        let s = self.data.get(self.pos..self.pos + n).ok_or(Eof)?;
        self.pos += n;
        Ok(s)
    }
    pub fn skip(&mut self, n: usize) -> R<()> {
        self.bytes(n).map(|_| ())
    }
    pub fn u8(&mut self) -> R<u8> {
        Ok(self.bytes(1)?[0])
    }
    pub fn u16(&mut self) -> R<u16> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    pub fn i16(&mut self) -> R<i16> {
        Ok(i16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> R<u32> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    pub fn i32(&mut self) -> R<i32> {
        Ok(i32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    pub fn u64(&mut self) -> R<u64> {
        Ok(u64::from_le_bytes(self.bytes(8)?.try_into().unwrap()))
    }
    pub fn f32(&mut self) -> R<f32> {
        Ok(f32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    pub fn vec3(&mut self) -> R<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }
    pub fn fourcc(&mut self) -> R<[u8; 4]> {
        Ok(self.bytes(4)?.try_into().unwrap())
    }
    /// u32 char count followed by UTF-16LE.
    pub fn utf16_u32(&mut self) -> R<String> {
        let n = self.u32()? as usize;
        let b = self.bytes(n * 2)?;
        Ok(String::from_utf16_lossy(
            &b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>(),
        ))
    }
    /// The bytes of a 7-bit-length-prefixed string.
    pub fn string_bytes_7bit(&mut self) -> R<&'a [u8]> {
        let mut n = 0usize;
        let mut shift = 0;
        loop {
            let b = self.u8()?;
            n |= ((b & 0x7F) as usize) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        self.bytes(n)
    }
    /// 7-bit-encoded length prefix (.NET BinaryWriter style) followed by bytes.
    pub fn string_7bit(&mut self) -> R<String> {
        let mut n = 0usize;
        let mut shift = 0;
        loop {
            let b = self.u8()?;
            n |= ((b & 0x7F) as usize) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        Ok(String::from_utf8_lossy(self.bytes(n)?).into_owned())
    }
}
