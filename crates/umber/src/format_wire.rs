//! Checked fixed-width encoding shared by semantic format codecs.
use crate::{Error, Result};
pub(crate) const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_ITEMS: usize = 1 << 20;

#[derive(Default)]
pub(crate) struct Writer {
    bytes: Vec<u8>,
}
impl Writer {
    pub fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }
    pub fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }
    pub fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }
    pub fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }
    pub fn i32(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }
    pub fn bytes(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }
    pub fn count(&mut self, value: usize) -> Result<()> {
        if value > MAX_ITEMS {
            return Err(Error::Capacity);
        }
        self.u32(value as u32);
        Ok(())
    }
    pub fn blob(&mut self, value: &[u8]) -> Result<()> {
        self.count(value.len())?;
        self.bytes(value);
        Ok(())
    }
    pub fn section(
        &mut self,
        kind: u32,
        write: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<()> {
        self.u32(kind);
        let length = self.bytes.len();
        self.u64(0);
        let start = self.bytes.len();
        write(self)?;
        let size = (self.bytes.len() - start) as u64;
        self.bytes[length..length + 8].copy_from_slice(&size.to_le_bytes());
        if self.bytes.len() > MAX_BYTES {
            return Err(Error::Capacity);
        }
        Ok(())
    }
    pub fn finish(mut self) -> Result<Vec<u8>> {
        if self.bytes.len() > MAX_BYTES || self.bytes.len() < 24 {
            return Err(Error::Capacity);
        }
        let length = self.bytes.len() as u64;
        self.bytes[16..24].copy_from_slice(&length.to_le_bytes());
        Ok(self.bytes)
    }
}
pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Result<Self> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::Capacity);
        }
        Ok(Self { bytes, position: 0 })
    }
    pub fn bytes(&mut self, len: usize) -> Result<&'a [u8]> {
        let end = self.position.checked_add(len).ok_or(Error::Format)?;
        let bytes = self.bytes.get(self.position..end).ok_or(Error::Format)?;
        self.position = end;
        Ok(bytes)
    }
    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }
    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }
    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.bytes(8)?.try_into().unwrap()))
    }
    pub fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }
    pub fn count(&mut self, minimum: usize) -> Result<usize> {
        let count = self.u32()? as usize;
        if count > MAX_ITEMS {
            return Err(Error::Capacity);
        }
        if minimum == 0 || count > self.remaining() / minimum {
            return Err(Error::Format);
        }
        Ok(count)
    }
    pub fn blob(&mut self) -> Result<&'a [u8]> {
        let n = self.count(1)?;
        self.bytes(n)
    }
    pub fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }
    pub fn finish(&self) -> Result<()> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(Error::Format)
        }
    }
    pub fn section(&mut self, kind: u32) -> Result<Self> {
        if self.u32()? != kind {
            return Err(Error::Format);
        }
        let length = usize::try_from(self.u64()?).map_err(|_| Error::Format)?;
        Self::new(self.bytes(length)?)
    }
}
