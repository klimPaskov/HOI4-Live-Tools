use super::*;
use crate::{memory::ReadMemory, platform::Process, research_ui::Witnesses};
use std::{cell::RefCell, collections::BTreeMap};

const B: u64 = 0x100000000;
const GS: u64 = 0x20000;
const COUNTRY: u64 = 0x30000;
const ARMY: u64 = 0x40000;
const FLEET: u64 = 0x50000;
const TASK: u64 = 0x60000;
const OTHER: u64 = 0x70000;
const SCENE_DATA: u64 = 0x80000;
const ARMY_DATA: u64 = 0x81000;
const FLEET_DATA: u64 = 0x82000;
const TASK_DATA: u64 = 0x83000;
const INDEX_DATA: u64 = 0x84000;
const COUNTRY_DATA: u64 = 0x85000;

struct Fixture {
    data: BTreeMap<u64, u8>,
    reads: RefCell<Vec<(u64, usize)>>,
    abi: StringAbi,
    changed_at_finish: Option<u64>,
}
impl Fixture {
    fn put(&mut self, address: u64, bytes: &[u8]) {
        self.data.extend(
            bytes
                .iter()
                .enumerate()
                .map(|(i, b)| (address + i as u64, *b)),
        );
    }
    fn word(&mut self, a: u64, v: u64) {
        self.put(a, &v.to_le_bytes());
    }
    fn number(&mut self, a: u64, v: i32) {
        self.put(a, &v.to_le_bytes());
    }
    fn list(&mut self, address: u64, data: u64, values: &[u64]) {
        self.word(address, data);
        self.number(address + 8, values.len() as i32);
        self.number(address + 12, values.len() as i32);
        for (i, value) in values.iter().enumerate() {
            self.word(data + i as u64 * 8, *value);
        }
    }
    fn new() -> Self {
        let mut f = Self {
            data: BTreeMap::new(),
            reads: RefCell::new(vec![]),
            abi: StringAbi::LibCxx,
            changed_at_finish: None,
        };
        f.word(B + game::MACOS.gs_global, GS);
        f.word(GS, B + game::MACOS.gs_vtable);
        f.number(GS + game::MACOS.player_tag, 1);
        f.word(GS + game::MACOS.tag_country_index, INDEX_DATA);
        f.number(GS + game::MACOS.tag_country_index + 8, 2);
        f.number(GS + game::MACOS.tag_country_index + 12, 2);
        f.number(INDEX_DATA + 4, 0);
        f.list(GS + game::MACOS.countries, COUNTRY_DATA, &[COUNTRY]);
        f.word(COUNTRY, B + game::MACOS.country_vtable);
        f.number(COUNTRY + 8, 1);
        f.list(COUNTRY + 0x250, ARMY_DATA, &[ARMY]);
        f.list(COUNTRY + 0x238, FLEET_DATA, &[FLEET]);
        f.list(FLEET + 0xb0, TASK_DATA, &[TASK]);
        f.list(GS + 0x5e0, SCENE_DATA, &[ARMY, TASK]);
        f.word(FLEET, B + 0x34fc2e8);
        f.word(ARMY, B + Kind::Army.primary());
        f.word(TASK, B + Kind::TaskForce.primary());
        f.number(ARMY + 8, 0);
        f.number(TASK + 8, 1);
        f.number(ARMY + 0x1d8, 1);
        f.number(TASK + 0x1d8, 1);
        f
    }
    fn observe(&self, original: Option<&Binding>) -> Result<Binding> {
        let w = Witnesses::new(self);
        let value = from_reader(&Reader::new(&w), B, original)?;
        w.finish()?;
        Ok(value)
    }
    fn untouched(&self, a: u64, n: u64) -> bool {
        self.reads
            .borrow()
            .iter()
            .all(|(b, s)| a + n <= *b || *b + *s as u64 <= a)
    }
    fn clear_reads(&self) {
        self.reads.borrow_mut().clear();
    }
}
impl ReadMemory for Fixture {
    fn string_abi(&self) -> StringAbi {
        self.abi
    }
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        self.reads.borrow_mut().push((a, n));
        (0..n)
            .map(|i| {
                self.data
                    .get(&(a + i as u64))
                    .copied()
                    .ok_or_else(|| Error::new("short_read", "Missing scene fixture byte"))
            })
            .collect()
    }
    fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        requests
            .iter()
            .map(|&(a, n)| {
                let mut bytes = self.read(a, n)?;
                if let Some(at) = self.changed_at_finish {
                    if (a..a + n as u64).contains(&at) {
                        bytes[(at - a) as usize] ^= 1;
                    }
                }
                Ok(bytes)
            })
            .collect()
    }
}
impl Process for Fixture {
    fn check(&self) -> Result<()> {
        Ok(())
    }
    fn image_base(&self) -> u64 {
        B
    }
}

#[test]
fn complete_ordered_own_army_and_nested_task_scope_is_retained() {
    let f = Fixture::new();
    let b = f.observe(None).unwrap();
    assert_eq!(
        b.entries()
            .iter()
            .map(|e| (e.object(), e.kind()))
            .collect::<Vec<_>>(),
        vec![(ARMY, Kind::Army), (TASK, Kind::TaskForce)]
    );
    assert_eq!(f.observe(Some(&b)).unwrap(), b);
    assert!(f.untouched(ARMY + 0x1f0, 0x100)); // No province/model/unit details.
    assert!(f.untouched(TASK + 0x1f0, 0x100));
}
#[test]
fn empty_scene_does_not_require_candidate_payload() {
    let mut f = Fixture::new();
    f.list(GS + 0x5e0, 0, &[]);
    assert!(f.observe(None).unwrap().entries().is_empty());
    assert!(f.untouched(ARMY, 0x300));
    assert!(f.untouched(TASK, 0x300));
}
#[test]
fn unsupported_abi_and_original_image_refuse_before_reads() {
    let mut f = Fixture::new();
    let b = f.observe(None).unwrap();
    f.clear_reads();
    assert!(from_reader(&Reader::new(&f), B + 8, Some(&b)).is_err());
    assert!(f.reads.borrow().is_empty());
    f.abi = StringAbi::LibStdCxx;
    assert_eq!(f.observe(None).unwrap_err().code, "unsupported_platform");
    assert!(f.reads.borrow().is_empty());
}
#[test]
fn original_gs_replacement_is_rejected_before_replacement_header() {
    let mut f = Fixture::new();
    let b = f.observe(None).unwrap();
    f.clear_reads();
    f.word(B + game::MACOS.gs_global, OTHER);
    assert_eq!(
        f.observe(Some(&b)).unwrap_err().code,
        "construction_scene_changed"
    );
    assert!(f.untouched(OTHER, 0x1000));
}
#[test]
fn original_country_replacement_is_rejected_before_replacement_payload() {
    let mut f = Fixture::new();
    let b = f.observe(None).unwrap();
    f.clear_reads();
    f.word(COUNTRY_DATA, OTHER);
    assert!(f.observe(Some(&b)).is_err());
    assert!(f.untouched(OTHER, 0x1000));
}
#[test]
fn foreign_country_tag_refuses_before_any_force_list() {
    let mut f = Fixture::new();
    f.number(COUNTRY + 8, 2);
    assert!(f.observe(None).is_err());
    assert!(f.untouched(COUNTRY + 0x238, 0x28));
}
#[test]
fn original_fleet_replacement_is_rejected_before_new_fleet_header() {
    let mut f = Fixture::new();
    let b = f.observe(None).unwrap();
    f.clear_reads();
    f.word(FLEET_DATA, OTHER);
    assert!(f.observe(Some(&b)).is_err());
    assert!(f.untouched(OTHER, 0x200));
}
#[test]
fn original_task_replacement_refuses_before_task_type_or_owner() {
    let mut f = Fixture::new();
    let b = f.observe(None).unwrap();
    f.clear_reads();
    f.word(TASK_DATA, OTHER);
    assert!(f.observe(Some(&b)).is_err());
    assert!(f.untouched(OTHER, 0x300));
}
#[test]
fn unknown_last_scene_entry_refuses_before_any_candidate_payload() {
    let mut f = Fixture::new();
    f.list(GS + 0x5e0, SCENE_DATA, &[ARMY, TASK, OTHER]);
    assert!(f.observe(None).is_err());
    for ptr in [ARMY, TASK, OTHER] {
        assert!(f.untouched(ptr, 0x300));
    }
}
#[test]
fn duplicate_scene_entry_is_ambiguous_before_candidate_payload() {
    let mut f = Fixture::new();
    f.list(GS + 0x5e0, SCENE_DATA, &[ARMY, ARMY]);
    assert!(f.observe(None).is_err());
    assert!(f.untouched(ARMY, 0x300));
}
#[test]
fn overlapping_own_sources_refuse_before_candidate_payload() {
    let mut f = Fixture::new();
    f.list(FLEET + 0xb0, TASK_DATA, &[ARMY]);
    assert!(f.observe(None).is_err());
    assert!(f.untouched(ARMY, 0x300));
}
#[test]
fn own_fleet_type_must_match_before_nested_member_header() {
    let mut f = Fixture::new();
    f.word(FLEET, B + Kind::Army.primary());
    assert!(f.observe(None).is_err());
    assert!(f.untouched(FLEET + 0xb0, 16));
}
#[test]
fn candidate_type_must_match_before_owner_or_kind() {
    let mut f = Fixture::new();
    f.word(ARMY, B + Kind::TaskForce.primary());
    assert!(f.observe(None).is_err());
    assert!(f.untouched(ARMY + 0x1d8, 4));
    assert!(f.untouched(ARMY + 8, 4));
}
#[test]
fn candidate_owner_refuses_before_kind_even_with_own_source_membership() {
    let mut f = Fixture::new();
    f.number(ARMY + 0x1d8, 2);
    assert!(f.observe(None).is_err());
    assert!(f.untouched(ARMY + 8, 4));
}
#[test]
fn exact_type_does_not_substitute_for_matching_selectable_kind() {
    let mut f = Fixture::new();
    f.number(TASK + 8, 13);
    assert!(f.observe(None).is_err());
}
#[test]
fn original_scene_order_is_retained_before_candidate_payload() {
    let mut f = Fixture::new();
    let b = f.observe(None).unwrap();
    f.clear_reads();
    f.list(GS + 0x5e0, SCENE_DATA, &[TASK, ARMY]);
    assert!(f.observe(Some(&b)).is_err());
    assert!(f.untouched(ARMY, 0x300));
    assert!(f.untouched(TASK, 0x300));
}
#[test]
fn excessive_capacity_refuses_before_scene_vector_payload() {
    let mut f = Fixture::new();
    f.number(GS + 0x5e8, 4097);
    assert!(f.observe(None).is_err());
    assert!(f.untouched(SCENE_DATA, 16));
}
#[test]
fn unaligned_or_null_candidates_never_become_objects() {
    for ptr in [0, ARMY + 1] {
        let mut f = Fixture::new();
        f.list(GS + 0x5e0, SCENE_DATA, &[ptr]);
        assert!(f.observe(None).is_err());
        assert!(f.untouched(ARMY, 0x300));
    }
}
#[test]
fn final_membership_or_owner_tear_discards_the_whole_binding() {
    for at in [SCENE_DATA, TASK_DATA, COUNTRY_DATA, ARMY + 0x1d8] {
        let mut f = Fixture::new();
        f.changed_at_finish = Some(at);
        assert_eq!(f.observe(None).unwrap_err().code, "torn_read");
    }
}
