// Derived typed/SSO/array semantics: Clausewitz-MCP calling/{mem,game}.py.
// Copyright (c) 2026 smerss, MIT; see ../LICENSE.clausewitz.
use crate::error::{Error, Result};
use std::cell::Cell;
pub trait ReadMemory {
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>>;
}
pub struct Reader<'a> {
    memory: &'a dyn ReadMemory,
    pub calls: Cell<u64>,
    pub bytes: Cell<usize>,
}
impl<'a> Reader<'a> {
    pub fn new(memory: &'a dyn ReadMemory) -> Self {
        Self {
            memory,
            calls: Cell::new(0),
            bytes: Cell::new(0),
        }
    }
    pub fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        let total = self
            .bytes
            .get()
            .checked_add(size)
            .ok_or_else(|| Error::new("read_bound", "Size overflow"))?;
        if size > 1 << 20
            || total > 32 << 20
            || !(0x10000..=0x7fff_ffff_0000).contains(&address)
            || address.checked_add(size as u64).is_none()
        {
            return Err(Error::new(
                "read_bound",
                "Invalid address or snapshot read budget",
            ));
        }
        self.calls.set(self.calls.get() + 1);
        self.bytes.set(total);
        let data = self.memory.read(address, size)?;
        if data.len() != size {
            return Err(Error::new("short_read", "Incomplete process read"));
        }
        Ok(data)
    }
    pub fn u64(&self, a: u64) -> Result<u64> {
        Ok(u64::from_le_bytes(self.read(a, 8)?.try_into().unwrap()))
    }
    pub fn i32(&self, a: u64) -> Result<i32> {
        Ok(i32::from_le_bytes(self.read(a, 4)?.try_into().unwrap()))
    }
    pub fn byte(&self, a: u64) -> Result<u8> {
        Ok(self.read(a, 1)?[0])
    }
    pub fn string(&self, a: u64) -> Result<String> {
        let header = self.read(a, 32)?;
        let ptr = u64::from_le_bytes(header[..8].try_into().unwrap());
        let size = u64::from_le_bytes(header[8..16].try_into().unwrap());
        if size > 4096 || (ptr == a + 16 && size > 15) {
            return Err(Error::new("invalid_string", "Invalid bounded SSO string"));
        }
        let bytes = if ptr == a + 16 {
            header[16..16 + size as usize].to_vec()
        } else if size == 0 {
            Vec::new()
        } else {
            self.read(ptr, size as usize)?
        };
        if self.read(a, 32)? != header {
            return Err(Error::new("torn_read", "String header changed"));
        }
        String::from_utf8(bytes).map_err(|_| Error::new("invalid_string", "Invalid UTF-8"))
    }
    pub fn array_header(&self, a: u64) -> Result<(Vec<u8>, u64, usize)> {
        let header = self.read(a, 16)?;
        let ptr = u64::from_le_bytes(header[..8].try_into().unwrap());
        let cap = i32::from_le_bytes(header[8..12].try_into().unwrap());
        let size = i32::from_le_bytes(header[12..16].try_into().unwrap());
        if !(0..=20_000).contains(&cap)
            || size < 0
            || size > cap
            || (size > 0 && !(0x10000..=0x7fff_ffff_0000).contains(&ptr))
        {
            return Err(Error::new(
                "invalid_array",
                "Invalid array count/capacity/data",
            ));
        }
        Ok((header, ptr, size as usize))
    }
    pub fn array_entry(&self, a: u64, index: usize, stride: usize) -> Result<Vec<u8>> {
        let (header, ptr, size) = self.array_header(a)?;
        if index >= size || !(1..=32).contains(&stride) {
            return Err(Error::new(
                "invalid_index",
                "Array index/stride unavailable",
            ));
        }
        let address = ptr
            .checked_add((index * stride) as u64)
            .ok_or_else(|| Error::new("read_bound", "Array address overflow"))?;
        let data = self.read(address, stride)?;
        if self.read(a, 16)? != header {
            return Err(Error::new("torn_read", "Array changed"));
        }
        Ok(data)
    }
    pub fn pointer_array(&self, a: u64) -> Result<Vec<u64>> {
        let (header, ptr, size) = self.array_header(a)?;
        let data = if size == 0 {
            Vec::new()
        } else {
            self.read(ptr, size * 8)?
        };
        if self.read(a, 16)? != header {
            return Err(Error::new("torn_read", "Array changed"));
        }
        Ok(data
            .chunks_exact(8)
            .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
            .collect())
    }
    pub fn object_type(&self, a: u64, name: &str) -> Result<()> {
        let vptr = self.u64(a)?;
        let info = self.u64(
            vptr.checked_sub(8)
                .ok_or_else(|| Error::new("invalid_type", "Invalid vtable"))?,
        )?;
        if !(0x10000..=0x7fff_ffff_0000).contains(&info) {
            return Err(Error::new(
                "invalid_type",
                "Invalid type information pointer",
            ));
        }
        let text = self.u64(info + 8)?;
        let encoded = format!("{}{name}", name.len());
        let bytes = self.read(text, encoded.len() + 1)?;
        if bytes[..encoded.len()] != *encoded.as_bytes() || bytes[encoded.len()] != 0 {
            return Err(Error::new("invalid_type", format!("Expected {name}")));
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fake {
        data: Vec<u8>,
        torn: bool,
        calls: Cell<u32>,
    }
    impl ReadMemory for Fake {
        fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
            self.calls.set(self.calls.get() + 1);
            let offset = (a - 0x10000) as usize;
            let mut d = self.data[offset..offset + n].to_vec();
            if self.torn && self.calls.get() > 1 && offset == 0 {
                d[0] ^= 1;
            }
            Ok(d)
        }
    }
    fn array(size: i32, cap: i32, torn: bool) -> Fake {
        let mut data = vec![0; 64];
        data[..8].copy_from_slice(&0x10020u64.to_le_bytes());
        data[8..12].copy_from_slice(&cap.to_le_bytes());
        data[12..16].copy_from_slice(&size.to_le_bytes());
        Fake {
            data,
            torn,
            calls: Cell::new(0),
        }
    }
    #[test]
    fn malformed_array_never_reads_payload() {
        for (n, c) in [(-1, 2), (3, 2), (1, 20001)] {
            let f = array(n, c, false);
            assert!(Reader::new(&f).pointer_array(0x10000).is_err());
            assert_eq!(f.calls.get(), 1);
        }
    }
    #[test]
    fn torn_array_rejected() {
        let f = array(1, 1, true);
        assert_eq!(
            Reader::new(&f).pointer_array(0x10000).unwrap_err().code,
            "torn_read"
        );
    }
    #[test]
    fn empty_array_is_known_empty() {
        let f = array(0, 0, false);
        assert!(Reader::new(&f).pointer_array(0x10000).unwrap().is_empty());
    }
}
