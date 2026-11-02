use super::*;
use crate::memory::ReadMemory;
use std::{cell::RefCell, collections::BTreeMap};

const HEADER: u64 = 0x20_0000;
const DATA: u64 = 0x21_0000;
const HEAP: u64 = 0x22_0000;

#[derive(Default)]
struct Fake {
    bytes: BTreeMap<u64, u8>,
    reads: RefCell<Vec<(u64, usize)>>,
    change_on_repeat: Option<u64>,
    macos: bool,
}
impl Fake {
    fn put(&mut self, address: u64, bytes: &[u8]) {
        self.bytes.extend(
            bytes
                .iter()
                .enumerate()
                .map(|(i, byte)| (address + i as u64, *byte)),
        );
    }
    fn word(&mut self, address: u64, value: u64) {
        self.put(address, &value.to_le_bytes());
    }
    fn registry(names: &[&str]) -> Self {
        let mut m = Self::default();
        m.word(HEADER, DATA);
        m.put(HEADER + 8, &(names.len() as i32).to_le_bytes());
        m.put(HEADER + 12, &(names.len() as i32).to_le_bytes());
        for (i, name) in names.iter().enumerate() {
            let address = DATA + i as u64 * 32;
            m.put(address, &[0; 32]);
            let pointer = if name.len() <= 15 {
                address + 16
            } else {
                HEAP + i as u64 * 512
            };
            m.word(address, pointer);
            m.word(address + 8, name.len() as u64);
            m.put(pointer, name.as_bytes());
        }
        m
    }
}
impl ReadMemory for Fake {
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        let mut reads = self.reads.borrow_mut();
        let changed = self.change_on_repeat == Some(address)
            && reads.iter().any(|&(prior, _)| prior == address);
        reads.push((address, size));
        let mut bytes = (0..size)
            .map(|i| {
                self.bytes
                    .get(&(address + i as u64))
                    .copied()
                    .ok_or_else(|| Error::new("unmapped", "Fixture payload unavailable"))
            })
            .collect::<Result<Vec<_>>>()?;
        if changed {
            bytes[0] ^= 1;
        }
        Ok(bytes)
    }
    fn string_abi(&self) -> StringAbi {
        if self.macos {
            StringAbi::LibCxx
        } else {
            StringAbi::LibStdCxx
        }
    }
}

#[test]
fn batches_short_names_without_reading_widget_payloads() {
    let m = Fake::registry(&["first", "research", "last"]);
    let r = Reader::new(&m);
    assert_eq!(
        lookup(&r, HEADER, &[0x30000, 0x40000, 0x50000], "research", 128).unwrap(),
        Some(0x40000)
    );
    assert_eq!(
        *m.reads.borrow(),
        vec![(HEADER, 16), (DATA, 96), (DATA, 96), (HEADER, 16)]
    );
}

#[test]
fn long_names_read_only_their_own_payload() {
    let name = "start_construction_button";
    let m = Fake::registry(&["", name]);
    let r = Reader::new(&m);
    assert_eq!(lookup(&r, HEADER, &[1, 2], name, 128).unwrap(), Some(2));
    assert_eq!(r.calls.get(), 5);
    assert!(m.reads.borrow().contains(&(HEAP + 512, name.len())));
}

#[test]
fn duplicates_of_requested_key_refuse_even_with_the_same_pointer() {
    let m = Fake::registry(&["research", "research"]);
    assert_eq!(
        lookup(&Reader::new(&m), HEADER, &[1, 1], "research", 128)
            .unwrap_err()
            .code,
        "invalid_gui_registry"
    );
}

#[test]
fn unrelated_duplicate_names_and_missing_key_are_allowed() {
    let m = Fake::registry(&["", "", "research"]);
    assert_eq!(
        lookup(&Reader::new(&m), HEADER, &[1, 2, 3], "research", 128).unwrap(),
        Some(3)
    );
    assert_eq!(
        lookup(&Reader::new(&m), HEADER, &[1, 2, 3], "missing", 128).unwrap(),
        None
    );
}

#[test]
fn empty_registry_does_not_dereference_its_data_pointer() {
    let mut m = Fake::registry(&[]);
    m.word(HEADER, 0);
    assert_eq!(
        lookup(&Reader::new(&m), HEADER, &[], "x", 128).unwrap(),
        None
    );
    assert_eq!(*m.reads.borrow(), vec![(HEADER, 16), (HEADER, 16)]);
}

#[test]
fn array_and_string_header_changes_refuse_without_a_match() {
    for address in [HEADER, DATA] {
        let mut m = Fake::registry(&["research"]);
        m.change_on_repeat = Some(address);
        assert_eq!(
            lookup(&Reader::new(&m), HEADER, &[1], "missing", 128)
                .unwrap_err()
                .code,
            "torn_read"
        );
    }
}

#[test]
fn corruption_after_a_match_is_not_hidden_by_early_return() {
    let mut m = Fake::registry(&["research", "invalid"]);
    m.put(DATA + 32 + 16, &[0xff]);
    assert_eq!(
        lookup(&Reader::new(&m), HEADER, &[1, 2], "research", 128)
            .unwrap_err()
            .code,
        "invalid_string"
    );
}

#[test]
fn invalid_string_sizes_refuse_before_external_payload() {
    for (pointer, size) in [(DATA + 16, 16), (HEAP, 129), (HEAP, u64::MAX)] {
        let mut m = Fake::registry(&["research"]);
        m.word(DATA, pointer);
        m.word(DATA + 8, size);
        assert_eq!(
            lookup(&Reader::new(&m), HEADER, &[1], "research", 128)
                .unwrap_err()
                .code,
            "invalid_string"
        );
        assert_eq!(m.reads.borrow().len(), 2);
    }
}

#[test]
fn name_array_count_capacity_and_alignment_are_checked_before_payload() {
    for (pointer, capacity, count) in [(DATA, 2, 2), (DATA, 257, 1), (DATA + 1, 1, 1)] {
        let mut m = Fake::registry(&["research"]);
        m.word(HEADER, pointer);
        m.put(HEADER + 8, &i32::to_le_bytes(capacity));
        m.put(HEADER + 12, &i32::to_le_bytes(count));
        assert_eq!(
            lookup(&Reader::new(&m), HEADER, &[1], "research", 128)
                .unwrap_err()
                .code,
            "invalid_gui_registry"
        );
        assert_eq!(*m.reads.borrow(), vec![(HEADER, 16)]);
    }
}

#[test]
fn complete_registry_bound_and_abi_are_checked_before_reading() {
    let m = Fake::default();
    assert!(lookup(&Reader::new(&m), HEADER, &[1; 257], "x", 128).is_err());
    assert!(lookup(&Reader::new(&m), HEADER, &[], "x", 257).is_err());
    assert!(m.reads.borrow().is_empty());
    let m = Fake {
        macos: true,
        ..Fake::default()
    };
    assert_eq!(
        lookup(&Reader::new(&m), HEADER, &[], "x", 128)
            .unwrap_err()
            .code,
        "capability_unavailable"
    );
    assert!(m.reads.borrow().is_empty());
}

#[test]
fn all_256_short_names_still_use_four_reads() {
    let names = vec!["unused"; 256];
    let m = Fake::registry(&names);
    let r = Reader::new(&m);
    assert_eq!(lookup(&r, HEADER, &[1; 256], "missing", 128).unwrap(), None);
    assert_eq!(r.calls.get(), 4);
}
