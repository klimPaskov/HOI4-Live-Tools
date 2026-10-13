use hoi4_live_native::{
    error::{Error, Result},
    memory::ReadMemory,
    own_focus,
    platform::Process,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};
const GS: u64 = 0x100000;
const ID: u64 = 0x110000;
const CO: u64 = 0x120000;
const GUI: u64 = 0x130000;
const MAN: u64 = 0x140000;
const VIEW: u64 = 0x150000;
const TREE: u64 = 0x160000;
const DB: u64 = 0x170000;
const F: u64 = 0x180000;
const PANEL: u64 = 0x190000;
const WIN: u64 = 0x1a0000;
const BUTTON: u64 = 0x1b0000;
struct Fake {
    data: BTreeMap<u64, u8>,
    reads: RefCell<Vec<(u64, usize)>>,
    torn: Option<u64>,
    checks: Cell<usize>,
    fail_final: bool,
}
impl Fake {
    fn put(&mut self, a: u64, b: &[u8]) {
        for (i, v) in b.iter().enumerate() {
            self.data.insert(a + i as u64, *v);
        }
    }
    fn region(&mut self, a: u64, n: usize) {
        self.put(a, &vec![0; n]);
    }
    fn word(&mut self, a: u64, v: u64) {
        self.put(a, &v.to_le_bytes());
    }
    fn int(&mut self, a: u64, v: i32) {
        self.put(a, &v.to_le_bytes());
    }
    fn string(&mut self, a: u64, v: &str) {
        assert!(v.len() < 16);
        self.region(a, 32);
        self.word(a, a + 16);
        self.word(a + 8, v.len() as u64);
        self.put(a + 16, v.as_bytes());
    }
    fn array(&mut self, a: u64, ptr: u64, values: &[u64]) {
        self.region(a, 16);
        self.word(a, ptr);
        self.int(a + 8, values.len() as i32);
        self.int(a + 12, values.len() as i32);
        for (i, v) in values.iter().enumerate() {
            self.word(ptr + i as u64 * 8, *v);
        }
    }
    fn new() -> Self {
        let mut s = Self {
            data: BTreeMap::new(),
            reads: RefCell::new(vec![]),
            torn: None,
            checks: Cell::new(0),
            fail_final: false,
        };
        for (a, t) in [
            (GS, 0x42c7598),
            (ID, 0x42c9a88),
            (GUI, 0x42736f8),
            (VIEW, 0x4249c70),
            (TREE, 0x4202410),
            (DB, 0x4202590),
            (F, 0x42024d0),
            (PANEL, 0x43023f8),
            (WIN, 0x4347d58),
            (BUTTON, 0x43470c8),
        ] {
            s.region(a, 0x1800);
            s.word(a, t);
        }
        s.region(MAN, 0x400);
        s.region(CO, 0x1800);
        s.word(0x44086a8, GS);
        s.word(0x44085f8, ID);
        s.word(0x439faf0, DB);
        s.int(GS + 0x518, 1);
        s.int(CO + 8, 1);
        // Actual Reader::object_type on country, not bypassed.
        s.word(CO, 0x200000);
        s.word(0x1ffff8, 0x201000);
        s.word(0x201008, 0x202000);
        s.put(0x202000, b"8CCountry\0");
        s.array(GS + 0x310, 0x210000, &[CO]);
        s.region(GS + 0x340, 16);
        s.word(GS + 0x340, 0x211000);
        s.int(GS + 0x348, 2);
        s.int(GS + 0x34c, 2);
        s.int(0x211000, -1);
        s.int(0x211004, 0);
        s.word(ID + 0x4f8, GUI);
        s.word(ID + 0x6b8, MAN);
        s.word(MAN + 0x180, ID);
        s.word(MAN + 0x130, VIEW);
        s.word(VIEW + 0x2f0, ID);
        s.word(VIEW + 0x11d8, CO);
        s.word(CO + 0x1338, TREE);
        s.array(TREE + 0x30, 0x212000, &[F]);
        s.array(DB + 0x58, 0x213000, &[F]);
        s.word(F + 0x160, TREE);
        s.string(F + 0x18, "GER_rhineland");
        s.array(VIEW + 0x1180, 0x214000, &[PANEL]);
        s.word(PANEL + 0x7b8, ID);
        s.word(PANEL + 0x7c0, VIEW);
        s.word(PANEL + 0x7d0, F);
        s.word(PANEL + 0x7c8, WIN);
        s.word(WIN + 0x98, GUI);
        s.put(WIN + 0xa5, &[8]);
        s.array(WIN + 0x9a0, 0x215000, &[BUTTON]);
        s.region(WIN + 0x9b8, 16);
        s.word(WIN + 0x9b8, 0x216000);
        s.int(WIN + 0x9c0, 1);
        s.int(WIN + 0x9c4, 1);
        s.string(0x216000, "start");
        s
    }
    fn result(&self) -> Result<serde_json::Value> {
        own_focus::snapshot(self, 0, 64, None)
    }
    fn untouched(&self, a: u64, n: u64) -> bool {
        !self
            .reads
            .borrow()
            .iter()
            .any(|(p, l)| *p < a + n && *p + *l as u64 > a)
    }
}
impl ReadMemory for Fake {
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        let repeat = self.reads.borrow().contains(&(a, n));
        self.reads.borrow_mut().push((a, n));
        let mut out: Vec<u8> = (0..n)
            .map(|i| {
                self.data
                    .get(&(a + i as u64))
                    .copied()
                    .ok_or_else(|| Error::new("fixture_unmapped", "unmapped fixture byte"))
            })
            .collect::<Result<_>>()?;
        if repeat {
            if let Some(target) = self.torn {
                if target >= a && target < a + n as u64 {
                    out[(target - a) as usize] ^= 1;
                }
            }
        }
        Ok(out)
    }
}
impl Process for Fake {
    fn check(&self) -> Result<()> {
        self.checks.set(self.checks.get() + 1);
        if self.fail_final && self.checks.get() > 1 {
            Err(Error::new(
                "ownership_changed",
                "fixture ownership replaced",
            ))
        } else {
            Ok(())
        }
    }
    fn image_base(&self) -> u64 {
        0
    }
}
#[test]
fn valid_catalog_detail_has_no_addresses_or_actions() {
    let f = Fake::new();
    let v = f.result().unwrap();
    assert_eq!(v["focuses"][0]["key"], "GER_rhineland");
    assert_eq!(v["details"][0]["selected_key"], "GER_rhineland");
    assert_eq!(v["details"][0]["window"]["start"]["disabled"], false);
    assert_eq!(v["input_authorized"], false);
    assert_eq!(v["eligibility_available"], false);
    assert!(!v.to_string().contains("ptr"));
    assert_eq!(f.checks.get(), 2);
}
#[test]
fn nonpositive_tag_stops_before_country_payload() {
    let mut f = Fake::new();
    f.int(GS + 0x518, 0);
    assert_eq!(f.result().unwrap_err().code, "player_unavailable");
    assert!(f.untouched(CO, 0x1800));
    assert!(f.untouched(0x210000, 8));
}
#[test]
fn foreign_display_stops_before_catalog_payload() {
    let mut f = Fake::new();
    f.word(VIEW + 0x11d8, 0x300000);
    assert_eq!(f.result().unwrap_err().code, "foreign_focus_scope");
    assert!(f.untouched(0x212000, 8));
    assert!(f.untouched(F, 0x1800));
}
#[test]
fn own_display_fallback_and_nullable_selection() {
    let mut f = Fake::new();
    f.word(VIEW + 0x11d8, 0);
    f.word(PANEL + 0x7d0, 0);
    for address in (WIN + 0x9a0..WIN + 0x9b0).chain(WIN + 0x9b8..WIN + 0x9c8) {
        f.data.remove(&address);
    }
    let v = f.result().unwrap();
    assert!(v["details"][0]["selected_key"].is_null());
    assert_eq!(v["details"][0]["window"]["start"]["materialized"], false);
    assert!(f.untouched(BUTTON, 0x1800));
}

#[test]
fn database_membership_tolerates_null_and_repeated_entries_without_own_scope_expansion() {
    let mut f = Fake::new();
    f.array(DB + 0x58, 0x213000, &[0, F, F, 0x300000]);
    assert_eq!(f.result().unwrap()["catalog_count"], 1);
    assert!(f.untouched(0x300000, 0x1800));
}
#[test]
fn missing_canonical_membership_refuses_before_definition_payload() {
    let mut f = Fake::new();
    f.array(DB + 0x58, 0x213000, &[0x300000]);
    assert_eq!(f.result().unwrap_err().code, "invalid_focus_catalog");
    assert!(f.untouched(F, 0x1800));
}
#[test]
fn shared_null_foreign_owner_refused() {
    for owner in [0, 0x300000] {
        let mut f = Fake::new();
        f.word(F + 0x160, owner);
        assert_eq!(f.result().unwrap_err().code, "invalid_focus_owner");
        assert!(f.untouched(F + 0x18, 32));
    }
}
#[test]
fn duplicate_catalog_keys_refused() {
    let mut f = Fake::new();
    let second = 0x220000;
    f.region(second, 0x1800);
    f.word(second, 0x42024d0);
    f.word(second + 0x160, TREE);
    f.string(second + 0x18, "GER_rhineland");
    f.array(TREE + 0x30, 0x212000, &[F, second]);
    f.array(DB + 0x58, 0x213000, &[F, second]);
    assert_eq!(f.result().unwrap_err().code, "invalid_focus_catalog");
}
#[test]
fn malformed_header_never_reads_payload() {
    let mut f = Fake::new();
    f.int(TREE + 0x3c, 2);
    assert_eq!(f.result().unwrap_err().code, "invalid_array");
    assert!(f.untouched(0x212000, 16));
}
#[test]
fn torn_header_and_key_payload_refused() {
    for address in [TREE + 0x30, F + 0x18, F + 0x28] {
        let mut f = Fake::new();
        f.torn = Some(address);
        assert_eq!(f.result().unwrap_err().code, "torn_read");
    }
}
#[test]
fn selected_foreign_definition_refused() {
    let mut f = Fake::new();
    f.word(PANEL + 0x7d0, 0x300000);
    assert_eq!(f.result().unwrap_err().code, "invalid_focus_catalog");
}
#[test]
fn detail_and_container_owner_refusals() {
    for address in [PANEL + 0x7b8, PANEL + 0x7c0, WIN + 0x98] {
        let mut f = Fake::new();
        f.word(address, 0x300000);
        assert_eq!(f.result().unwrap_err().code, "invalid_focus_owner");
    }
}
#[test]
fn paired_control_registry_rejects_count_mismatch() {
    let mut f = Fake::new();
    f.int(WIN + 0x9c4, 0);
    assert_eq!(f.result().unwrap_err().code, "invalid_control_binding");
    assert!(f.untouched(BUTTON, 0x1800));
}
#[test]
fn alternate_registry_does_not_bind_start() {
    let mut f = Fake::new();
    f.string(0x216000, "close");
    f.array(WIN + 0xa60, 0x217000, &[BUTTON]);
    let v = f.result().unwrap();
    assert_eq!(v["details"][0]["window"]["start"]["materialized"], false);
    assert!(f.untouched(BUTTON, 0x1800));
}
#[test]
fn exact_control_type_and_duplicate_name_refused() {
    let mut f = Fake::new();
    f.word(BUTTON, 0x43470d0);
    assert_eq!(f.result().unwrap_err().code, "invalid_focus_type");
    let mut f = Fake::new();
    f.array(WIN + 0x9a0, 0x215000, &[BUTTON, BUTTON]);
    f.int(WIN + 0x9c0, 2);
    f.int(WIN + 0x9c4, 2);
    f.string(0x216020, "start");
    assert_eq!(f.result().unwrap_err().code, "invalid_control_binding");
}
#[test]
fn literal_visibility_is_separate_from_conservative_animation() {
    let mut f = Fake::new();
    f.int(WIN + 0xef8, 2);
    f.int(BUTTON + 0xac, 3);
    f.int(BUTTON + 0x74, i32::MIN);
    let v = f.result().unwrap();
    let w = &v["details"][0]["window"];
    assert_eq!(w["detail_visibility_flag"], true);
    assert_eq!(w["conservative_container_visibility"], false);
    assert_eq!(w["start"]["disabled"], true);
    assert_eq!(w["start"]["flags74_raw"], 2147483648u64);
}
#[test]
fn nullable_window_and_empty_catalog_are_known_states() {
    let mut f = Fake::new();
    f.word(PANEL + 0x7c8, 0);
    assert_eq!(
        f.result().unwrap()["details"][0]["window"]["materialized"],
        false
    );
    let mut f = Fake::new();
    f.array(TREE + 0x30, 0x212000, &[]);
    f.array(VIEW + 0x1180, 0x214000, &[]);
    assert_eq!(f.result().unwrap()["catalog_count"], 0);
}
#[test]
fn pagination_filter_validate_entire_catalog_and_no_payload_on_bad_query() {
    let f = Fake::new();
    let v = own_focus::snapshot(&f, 0, 1, Some("OTHER")).unwrap();
    assert_eq!(v["filtered_count"], 0);
    let f = Fake::new();
    assert_eq!(
        own_focus::snapshot(&f, 0, 101, None).unwrap_err().code,
        "invalid_query"
    );
    assert!(f.reads.borrow().is_empty());
}
#[test]
fn replaced_process_at_final_check_refused() {
    let mut f = Fake::new();
    f.fail_final = true;
    assert_eq!(f.result().unwrap_err().code, "ownership_changed");
}

#[test]
fn wrong_primary_types_fail_closed() {
    for address in [TREE, DB, F, VIEW, PANEL, WIN, GUI] {
        let mut f = Fake::new();
        f.word(address, 0x300000);
        assert_eq!(f.result().unwrap_err().code, "invalid_focus_type");
    }
}
#[test]
fn duplicate_pointer_catalog_and_oversized_capacity_refused() {
    let mut f = Fake::new();
    f.array(TREE + 0x30, 0x212000, &[F, F]);
    assert_eq!(f.result().unwrap_err().code, "invalid_focus_catalog");
    let mut f = Fake::new();
    f.int(TREE + 0x38, 4097);
    assert_eq!(f.result().unwrap_err().code, "invalid_array");
    assert!(f.untouched(0x212000, 8));
}
#[test]
fn duplicate_keys_outside_page_still_rejected() {
    let mut f = Fake::new();
    let second = 0x220000;
    f.region(second, 0x1800);
    f.word(second, 0x42024d0);
    f.word(second + 0x160, TREE);
    f.string(second + 0x18, "GER_rhineland");
    f.array(TREE + 0x30, 0x212000, &[F, second]);
    f.array(DB + 0x58, 0x213000, &[F, second]);
    assert_eq!(
        own_focus::snapshot(&f, 0, 1, None).unwrap_err().code,
        "invalid_focus_catalog"
    );
}
#[test]
fn changed_selection_and_control_state_on_final_witness_refused() {
    for address in [PANEL + 0x7d0, BUTTON + 0xac, WIN + 0x98, VIEW + 0x11d8] {
        let mut f = Fake::new();
        f.torn = Some(address);
        assert_eq!(f.result().unwrap_err().code, "torn_read");
    }
}
