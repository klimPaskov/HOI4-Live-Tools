//! Synthetic process memory for the naval, air and design read tests.
use crate::{
    error::{Error, Result},
    memory::{ReadMemory, StringAbi},
};
use std::cell::RefCell;

pub(super) const IMAGE: u64 = 0x1_0000_0000;
/// Sparse, zero-filled heap made of 64 KiB pages.
pub(super) struct Mem {
    pages: RefCell<Vec<(u64, Vec<u8>)>>,
    next: RefCell<u64>,
}
impl Mem {
    pub fn new() -> Self {
        Self {
            pages: RefCell::new(Vec::new()),
            next: RefCell::new(0x7ff0_0000_0000),
        }
    }
    /// Allocate `size` zeroed bytes aligned to 16.
    pub fn alloc(&self, size: u64) -> u64 {
        let mut next = self.next.borrow_mut();
        let address = *next;
        *next += (size + 15) & !15;
        *next += 16;
        self.pages
            .borrow_mut()
            .push((address, vec![0; size as usize + 8]));
        address
    }
    fn with<T>(&self, address: u64, size: usize, f: impl FnOnce(&mut [u8]) -> T) -> Option<T> {
        let mut pages = self.pages.borrow_mut();
        for (start, data) in pages.iter_mut() {
            if address >= *start && address + size as u64 <= *start + data.len() as u64 {
                let off = (address - *start) as usize;
                return Some(f(&mut data[off..off + size]));
            }
        }
        None
    }
    pub fn u64(&self, address: u64, value: u64) {
        self.with(address, 8, |b| b.copy_from_slice(&value.to_le_bytes()))
            .expect("mapped");
    }
    pub fn u64_at(&self, address: u64) -> u64 {
        self.with(address, 8, |b| u64::from_le_bytes(b.try_into().unwrap()))
            .expect("mapped")
    }
    pub fn i32(&self, address: u64, value: i32) {
        self.with(address, 4, |b| b.copy_from_slice(&value.to_le_bytes()))
            .expect("mapped");
    }
    /// libc++ string (short form up to 22 bytes, else a separate long buffer).
    pub fn string(&self, address: u64, text: &str) {
        if text.len() <= 22 {
            self.with(address, 24, |b| {
                b.fill(0);
                b[0] = (text.len() as u8) << 1;
                b[1..1 + text.len()].copy_from_slice(text.as_bytes());
            })
            .expect("mapped");
        } else {
            let buffer = self.alloc(text.len() as u64 + 1);
            self.with(buffer, text.len(), |b| b.copy_from_slice(text.as_bytes()))
                .expect("mapped");
            self.u64(address, (text.len() as u64 + 16) | 1);
            self.u64(address + 8, text.len() as u64);
            self.u64(address + 16, buffer);
        }
    }
    /// Array header {data, capacity, size} over freshly allocated pointers.
    pub fn pointer_array(&self, header: u64, items: &[u64]) {
        let data = self.alloc(8 * items.len().max(1) as u64);
        for (i, p) in items.iter().enumerate() {
            self.u64(data + 8 * i as u64, *p);
        }
        self.u64(header, data);
        self.i32(header + 8, items.len() as i32 + 1);
        self.i32(header + 12, items.len() as i32);
    }
}
impl ReadMemory for Mem {
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        self.with(address, size, |b| b.to_vec())
            .ok_or_else(|| Error::new("short_read", "unmapped synthetic read"))
    }
    fn string_abi(&self) -> StringAbi {
        StringAbi::LibCxx
    }
}
