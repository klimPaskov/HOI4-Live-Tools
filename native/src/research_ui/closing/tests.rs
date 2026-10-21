use super::*;
use crate::memory::ReadMemory;
use std::collections::BTreeMap;
#[derive(Default)]
struct Bytes(BTreeMap<u64, u8>);
impl Bytes {
    fn put(&mut self, p: u64, bytes: &[u8]) {
        for (i, b) in bytes.iter().enumerate() {
            self.0.insert(p + i as u64, *b);
        }
    }
    fn word(&mut self, p: u64, v: u64) {
        self.put(p, &v.to_le_bytes());
    }
}
impl ReadMemory for Bytes {
    fn read(&self, p: u64, n: usize) -> Result<Vec<u8>> {
        (p..p + n as u64)
            .map(|a| {
                self.0
                    .get(&a)
                    .copied()
                    .ok_or_else(|| Error::new("fixture_unmapped", "Unmapped byte"))
            })
            .collect()
    }
}
#[test]
fn closing_window_requires_literal_settled_owned_container() {
    let base = 0x10000;
    let root = 0x20000;
    let registry = 0x30000;
    let mut m = Bytes::default();
    m.word(root, base + 0x4347d58);
    m.word(root + 0x98, registry);
    m.put(root + 0xa5, &[8]);
    m.put(root + 0xeb0, &0i32.to_le_bytes());
    m.put(root + 0xef8, &0i32.to_le_bytes());
    assert!(settled(&Reader::new(&m), base, root, registry).unwrap());
    m.put(root + 0xa5, &[16]);
    assert!(!settled(&Reader::new(&m), base, root, registry).unwrap());
    for field in [0xeb0, 0xef8] {
        for state in [-1i32, 1, 2, 3] {
            m.put(root + field, &state.to_le_bytes());
            assert!(settled(&Reader::new(&m), base, root, registry).is_err());
        }
        m.put(root + field, &0i32.to_le_bytes());
    }
    m.word(root + 0x98, registry + 8);
    assert!(settled(&Reader::new(&m), base, root, registry).is_err());
    m.word(root + 0x98, registry);
    m.word(root, base + 0x42f2708); // embedded view must never masquerade as window
    assert!(settled(&Reader::new(&m), base, root, registry).is_err());
    assert!(settled(&Reader::new(&m), base, 0, registry).is_err());
}
