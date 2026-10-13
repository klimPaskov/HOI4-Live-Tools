use crate::{
    error::{Error, Result},
    memory::{ReadMemory, Reader},
    technology_nodes::current_nodes,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};
const BASE: u64 = 0x10000000;
const TREE: u64 = 0x20000000;
const STATUS: u64 = 0x21000000;
const ITEM: u64 = 0x22000000;
const TECH: u64 = 0x23000000;
const TEMPLATE: u64 = 0x24000000;
const DB: u64 = 0x25000000;
const RECORDS: u64 = 0x26000000;
#[derive(Default)]
struct Memory {
    bytes: BTreeMap<u64, u8>,
    change: Cell<bool>,
}
impl Memory {
    fn put(&mut self, a: u64, b: &[u8]) {
        for (i, v) in b.iter().enumerate() {
            self.bytes.insert(a + i as u64, *v);
        }
    }
    fn u64(&mut self, a: u64, v: u64) {
        self.put(a, &v.to_le_bytes());
    }
    fn i32(&mut self, a: u64, v: i32) {
        self.put(a, &v.to_le_bytes());
    }
    fn header(&mut self, a: u64, data: u64, count: i32) {
        self.u64(a, data);
        self.i32(a + 8, count);
        self.i32(a + 12, count);
    }
    fn records(&mut self, ids: &[u64]) {
        self.header(TREE + 0xdd8, RECORDS, ids.len() as i32);
        self.put(RECORDS, &vec![0; 40 * ids.len()]);
        for (i, item) in ids.iter().enumerate() {
            self.u64(RECORDS + i as u64 * 40 + 32, *item);
        }
    }
    fn unknown_gui(&mut self) -> u64 {
        let object = 0x29000000;
        let table = BASE + 0x4250000;
        let info = BASE + 0x4260000;
        let name = BASE + 0x3600000;
        self.u64(object, table);
        self.u64(table - 16, 0);
        self.u64(table - 8, info);
        self.u64(info + 8, name);
        self.put(name, &[0; 128]);
        self.put(name, b"12UnknownWidget");
        object
    }
    fn fixture() -> Self {
        let mut m = Self::default();
        for (p, v) in [
            (TREE, 0x424de40),
            (STATUS, 0x42108f8),
            (ITEM, 0x424e2e0),
            (TECH, 0x42107f8),
            (TEMPLATE, 0x4211038),
            (DB, 0x42110c8),
        ] {
            m.u64(p, BASE + v);
        }
        m.records(&[ITEM]);
        m.header(TREE + 0xdf0, 0, 0);
        m.u64(ITEM + 0x30, TREE);
        m.u64(ITEM + 0xc8, TECH);
        m.u64(ITEM + 0x10, 0);
        m.u64(TECH + 0x158, STATUS);
        m.u64(TECH + 0x160, TEMPLATE);
        m.i32(TEMPLATE + 0x30, 1);
        m.put(TEMPLATE + 8, &[0; 32]);
        m.u64(TEMPLATE + 8, TEMPLATE + 24);
        m.u64(TEMPLATE + 16, 11);
        m.put(TEMPLATE + 24, b"basic_tools");
        m.u64(BASE + 0x439f450, DB);
        m.header(DB + 0x48, 0x27000000, 2);
        m.u64(0x27000000, 0);
        m.u64(0x27000008, TEMPLATE);
        m.header(STATUS + 0x88, 0x28000000, 1);
        m.u64(0x28000000, TECH);
        m
    }
}
impl ReadMemory for Memory {
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        if self.change.get() && a == ITEM + 0xc8 {
            return Ok(vec![0; n]);
        }
        (0..n)
            .map(|i| {
                self.bytes.get(&(a + i as u64)).copied().ok_or_else(|| {
                    Error::new("unexpected_read", format!("Undeclared fixture field {a:x}"))
                })
            })
            .collect()
    }
}
// Mirrors caller Witnesses contract; tests do not relax unknown-field reads.
struct Witnesses<'a> {
    source: &'a Memory,
    reads: RefCell<BTreeMap<(u64, usize), Vec<u8>>>,
}
impl ReadMemory for Witnesses<'_> {
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        let bytes = self.source.read(a, n)?;
        let mut reads = self.reads.borrow_mut();
        if reads.get(&(a, n)).is_some_and(|p| p != &bytes) {
            return Err(Error::new("torn_read", "changed"));
        }
        reads.insert((a, n), bytes.clone());
        Ok(bytes)
    }
}
impl Witnesses<'_> {
    fn finish(&self) -> Result<()> {
        for ((a, n), v) in self.reads.borrow().iter() {
            if self.source.read(*a, *n)? != *v {
                return Err(Error::new("torn_read", "changed"));
            }
        }
        Ok(())
    }
}
fn observe(m: &Memory) -> Result<serde_json::Value> {
    current_nodes(&Reader::new(m), BASE, TREE, STATUS, 0)
}
#[test]
fn both_vectors_deduplicate_and_output_no_pointers() {
    let mut m = Memory::fixture();
    m.header(TREE + 0xdf0, RECORDS, 1);
    let v = observe(&m).unwrap();
    assert_eq!(v["record_count"], 2);
    assert_eq!(v["node_count"], 1);
    assert_eq!(v["nodes"][0]["definition_index"], 1);
    assert!(v["nodes"][0]["gui_type_encoding"].is_null());
    assert_eq!(v["input_authorized"], false);
    assert!(!v.to_string().contains(&ITEM.to_string()));
}
#[test]
fn foreign_owner_refused_before_missing_template() {
    let mut m = Memory::fixture();
    m.u64(TECH + 0x158, STATUS + 8);
    for i in 0..8 {
        m.bytes.remove(&(TECH + 0x160 + i));
    }
    assert_eq!(observe(&m).unwrap_err().code, "invalid_research_owner");
}
#[test]
fn canonical_join_does_not_use_derived_id() {
    let mut m = Memory::fixture();
    m.i32(TEMPLATE + 0x30, 2);
    assert_eq!(observe(&m).unwrap_err().code, "invalid_research_join");
}
#[test]
fn wrong_item_owner_refused() {
    let mut m = Memory::fixture();
    m.u64(ITEM + 0x30, TREE + 8);
    assert_eq!(observe(&m).unwrap_err().code, "invalid_gui_owner");
}
#[test]
fn current_records_have_40_byte_stride() {
    let mut m = Memory::fixture();
    m.records(&[ITEM, ITEM]);
    assert_eq!(observe(&m).unwrap()["record_count"], 2);
}
#[test]
fn negative_count_and_aggregate_overflow_refused() {
    let mut m = Memory::fixture();
    m.i32(TREE + 0xdd8 + 12, -1);
    assert_eq!(observe(&m).unwrap_err().code, "invalid_array");
    let mut m = Memory::fixture();
    m.header(TREE + 0xdd8, RECORDS, 513);
    assert_eq!(observe(&m).unwrap_err().code, "invalid_array");
}
#[test]
fn changed_node_path_is_rejected_by_caller_finish() {
    let m = Memory::fixture();
    let w = Witnesses {
        source: &m,
        reads: RefCell::new(BTreeMap::new()),
    };
    let r = Reader::new(&w);
    current_nodes(&r, BASE, TREE, STATUS, 0).unwrap();
    m.change.set(true);
    assert_eq!(w.finish().unwrap_err().code, "torn_read");
}
#[test]
fn null_node_is_not_silently_skipped() {
    let mut m = Memory::fixture();
    m.records(&[0]);
    assert!(observe(&m).is_err());
}
#[test]
fn unknown_gui_reads_metadata_only() {
    let mut m = Memory::fixture();
    let gui = m.unknown_gui();
    m.u64(ITEM + 0x10, gui);
    let v = observe(&m).unwrap();
    assert_eq!(v["nodes"][0]["gui_type_encoding"], "12UnknownWidget");
    assert_eq!(v["nodes"][0]["visibility"], "unavailable");
}
#[test]
fn conflicting_gui_for_same_canonical_id_refused() {
    let mut m = Memory::fixture();
    let other = ITEM + 0x1000;
    let gui = m.unknown_gui();
    m.u64(other, BASE + 0x424e2e0);
    m.u64(other + 0x30, TREE);
    m.u64(other + 0xc8, TECH);
    m.u64(other + 0x10, gui);
    m.records(&[ITEM, other]);
    assert_eq!(observe(&m).unwrap_err().code, "ambiguous_gui_node");
}

#[test]
fn public_node_keys_refuse_control_bytes_and_ambiguous_canonical_definitions() {
    let mut m = Memory::fixture();
    m.put(TEMPLATE + 24, b"basic\ntools");
    assert_eq!(observe(&m).unwrap_err().code, "invalid_research_id");
    let mut m = Memory::fixture();
    let item = ITEM + 0x1000;
    let tech = TECH + 0x1000;
    let template = TEMPLATE + 0x1000;
    m.records(&[ITEM, item]);
    m.u64(item, BASE + 0x424e2e0);
    m.u64(item + 0x30, TREE);
    m.u64(item + 0xc8, tech);
    m.u64(item + 0x10, 0);
    m.u64(tech, BASE + 0x42107f8);
    m.u64(tech + 0x158, STATUS);
    m.u64(tech + 0x160, template);
    m.u64(template, BASE + 0x4211038);
    m.i32(template + 0x30, 2);
    m.put(template + 8, &[0; 32]);
    m.u64(template + 8, template + 24);
    m.u64(template + 16, 11);
    m.put(template + 24, b"basic_tools");
    m.header(DB + 0x48, 0x27000000, 3);
    m.u64(0x27000010, template);
    m.header(STATUS + 0x88, 0x28000000, 2);
    m.u64(0x28000008, tech);
    assert_eq!(observe(&m).unwrap_err().code, "invalid_research_id");
}
