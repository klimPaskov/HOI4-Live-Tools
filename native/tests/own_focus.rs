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
        s.word(BUTTON + 0x68, GUI);
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

impl Fake {
    fn start_binding(&mut self) {
        self.word(BUTTON + 0x80, 0x4347488);
        self.word(BUTTON + 0x88, 0x220000);
        self.word(BUTTON + 0x90, 0x220020);
        self.int(BUTTON + 0x98, 2);
        self.region(0x220000, 64);
        self.word(0x220000, PANEL + 0x2a8);
        self.word(0x220010, 0x220020);
        self.word(0x220020, PANEL + 0x530);
        self.word(0x220028, 0x220000);
        for (glue, off, cap, manager, invoker, member) in [
            (
                PANEL + 0x2a8,
                0x28,
                0x221000,
                0x283a060,
                0x283a030,
                0x282ffe0,
            ),
            (
                PANEL + 0x530,
                0x228,
                0x221020,
                0x283a100,
                0x283a0d0,
                0x2839080,
            ),
        ] {
            self.word(glue, 0x4302598);
            self.word(glue + off, cap);
            self.word(glue + off + 0x10, manager);
            self.word(glue + off + 0x18, invoker);
            self.region(cap, 24);
            self.word(cap, member);
            self.word(cap + 0x10, PANEL);
        }
        self.word(BUTTON + 0xe8, 0x222000);
        self.region(0x222000, 0x140);
        self.word(0x222000, 0x43593d8);
        self.word(0x222138, BUTTON);
        self.word(0x43593c8, 0);
        self.word(0x43593d0, 0x43596f8);
        self.region(0x43596f8, 16);
        self.word(0x4359700, 0x350e700);
        self.region(0x350e700, 128);
        self.put(0x350e700, b"11CTextSprite");
    }
    fn binding(&self) -> serde_json::Value {
        self.result().unwrap()["details"][0]["window"]["start"]["ordinary_binding"].clone()
    }
}
#[test]
fn focus_start_event_and_noop_two_listeners_verified_no_authority() {
    let mut f = Fake::new();
    f.start_binding();
    let v = f.binding();
    assert_eq!(v["available"], true);
    assert_eq!(v["callback_route_verified"], true);
    assert_eq!(v["selected_own_key"], "GER_rhineland");
    assert_eq!(v["input_authorized"], false);
    assert_eq!(v["renderer_type_encoding"], "11CTextSprite");
    assert!(f.untouched(0x222168, 8));
}
#[test]
fn focus_start_foreign_button_and_capture_owners_refused_before_payload() {
    let mut f = Fake::new();
    f.start_binding();
    f.word(BUTTON + 0x68, 0xdead0000);
    assert_eq!(f.result().unwrap_err().code, "invalid_focus_owner");
    assert!(f.untouched(BUTTON + 0x80, 8));
    assert!(f.untouched(BUTTON + 0xac, 4));
    let mut f = Fake::new();
    f.start_binding();
    f.word(0x221010, 0xdead0000);
    assert_eq!(f.result().unwrap_err().code, "invalid_focus_owner");
    assert!(f.untouched(0x221000, 8));
    assert!(f.untouched(BUTTON + 0xe8, 8));
}
#[test]
fn focus_start_wrong_event_variant_and_unknown_target_no_capture_reads() {
    let mut f = Fake::new();
    f.start_binding();
    f.word(PANEL + 0x2a8 + 0x38, 0x2916700);
    assert_eq!(f.binding()["reason"], "event_variant_unavailable");
    assert!(f.untouched(0x221000, 24));
    let mut f = Fake::new();
    f.start_binding();
    f.word(0x220020, 0xdead0000);
    assert_eq!(f.binding()["callback_route_verified"], false);
    assert!(f.untouched(0xdead0000, 8));
    assert!(f.untouched(PANEL + 0x2a8, 8));
}
#[test]
fn focus_start_listener_bounds_removed_override_and_member_variant() {
    let mut f = Fake::new();
    f.start_binding();
    f.int(BUTTON + 0x98, 17);
    assert_eq!(f.result().unwrap_err().code, "invalid_focus_binding");
    let mut f = Fake::new();
    f.start_binding();
    f.put(0x220018, &[1]);
    assert_eq!(f.result().unwrap_err().code, "invalid_focus_binding");
    let mut f = Fake::new();
    f.start_binding();
    f.word(BUTTON + 0xa0, 0xdead0000);
    assert_eq!(f.binding()["reason"], "observer_override_unavailable");
    assert!(f.untouched(0x220000, 32));
    let mut f = Fake::new();
    f.start_binding();
    f.word(0x221008, 8);
    assert_eq!(f.binding()["reason"], "event_member_unavailable");
    assert!(f.untouched(BUTTON + 0xe8, 8));
}
#[test]
fn focus_start_unknown_renderer_metadata_only_and_hidden_no_binding() {
    let mut f = Fake::new();
    f.start_binding();
    f.word(0x222000, 0x43593e0);
    f.word(0x43593d8, 0);
    f.word(0x43593d0 + 8, 0x43596f8);
    // malformed/unknown primary metadata explicitly unavailable, no layout payload.
    let v = f.binding();
    assert_eq!(v["callback_route_verified"], true);
    assert_eq!(v["renderer_control_join_verified"], false);
    assert!(f.untouched(0x222138, 8));
    let mut f = Fake::new();
    f.start_binding();
    f.put(WIN + 0xa5, &[0]);
    assert_eq!(f.binding()["reason"], "detail_not_locally_visible");
    assert!(f.untouched(BUTTON + 0x80, 8));
}
#[test]
fn focus_start_each_structural_capture_renderer_byte_final_witness() {
    for a in [
        BUTTON + 0x68,
        BUTTON + 0x80,
        BUTTON + 0xa0,
        BUTTON + 0x98,
        0x220000,
        0x220028,
        0x220018,
        PANEL + 0x2a8,
        PANEL + 0x2e0,
        PANEL + 0x2e8,
        0x221010,
        0x221000,
        PANEL + 0x768,
        PANEL + 0x770,
        0x221030,
        0x221020,
        BUTTON + 0xe8,
        0x222138,
    ] {
        let mut f = Fake::new();
        f.start_binding();
        f.torn = Some(a);
        assert_eq!(f.result().unwrap_err().code, "torn_read", "{a:x}");
    }
}
#[test]
fn focus_start_extra_callback_variant_unavailable_before_capture() {
    let mut f = Fake::new();
    f.start_binding();
    f.word(PANEL + 0x2a8 + 0x18, 0xdead0000);
    assert_eq!(
        f.binding()["reason"],
        "additional_event_variant_unavailable"
    );
    assert!(f.untouched(0x221000, 24));
    assert!(f.untouched(0xdead0000, 8));
}

impl Fake {
    fn focus_nodes(&mut self) {
        self.array(VIEW + 0x1138, 0x230000, &[0x231000]);
        self.region(0x231000, 0x2d0);
        self.word(0x231000, 0x4249b90);
        self.word(0x2312b0, VIEW);
        self.word(0x231020, F);
        self.word(0x231010, 0x232000);
        self.region(0x232000, 0xf00);
        self.word(0x232000, 0x4347d58);
        self.word(0x232098, GUI);
        self.array(0x2329a0, 0x233000, &[0x234000]);
        self.region(0x2329b8, 16);
        self.word(0x2329b8, 0x233100);
        self.int(0x2329c0, 1);
        self.int(0x2329c4, 1);
        self.string(0x233100, "symbol");
        self.word(0x43470b8, 0);
        self.word(0x43470c0, 0x4357000);
        self.region(0x4357000, 16);
        self.word(0x4357008, 0x350f000);
        self.region(0x350f000, 128);
        self.put(0x350f000, b"15CButtonStandard");
        self.word(0x4346b00, 1);
        self.region(0x234000, 0x110);
        self.word(0x234000, 0x43470c8);
        self.word(0x234068, GUI);
        self.word(0x2312b8, 0x234000);
        self.word(0x234080, 0x4347488);
        self.region(0x235000, 32);
        self.word(0x234088, 0x235000);
        self.word(0x234090, 0x235000);
        self.int(0x234098, 1);
        self.word(0x235000, 0x231028);
        self.word(0x231028, 0x424a860);
        for (off, cap, member) in [(8, 0x236000, 0x150b440), (0x148, 0x236020, 0x150b5f0)] {
            self.region(cap, 24);
            self.word(cap, member);
            self.word(cap + 0x10, 0x231000);
            self.word(0x231028 + off, cap);
            self.word(0x231028 + off + 0x10, 0x1527740);
            self.word(0x231028 + off + 0x18, 0x1527710);
        }
    }
    fn node_result(&self) -> Result<serde_json::Value> {
        own_focus::node_detail(self, "GER_rhineland")
    }
}
#[test]
fn focus_exact_node_key_complete_join_and_ordinary_callbacks_no_input() {
    let mut f = Fake::new();
    f.focus_nodes();
    let v = f.node_result().unwrap();
    let n = &v["node_association"];
    assert_eq!(n["materialized"], true);
    assert_eq!(n["controls"][0]["binding_verified"], true);
    assert_eq!(n["input_authorized"], false);
    assert_eq!(n["requested_key"], "GER_rhineland");
    assert!(f.untouched(0x231030 + 0x1d0, 8));
}
#[test]
fn focus_nodes_wrong_later_owner_refuses_before_target_gui() {
    let mut f = Fake::new();
    f.focus_nodes();
    f.array(VIEW + 0x1138, 0x230000, &[0x231000, 0x237000]);
    f.region(0x237000, 0x2d0);
    f.word(0x237000, 0x4249b90);
    f.word(0x2372b0, 0xdead0000);
    assert_eq!(f.node_result().unwrap_err().code, "foreign_focus_scope");
    assert!(f.untouched(0x231010, 8));
    assert!(f.untouched(0x237020, 8));
}
#[test]
fn focus_nodes_duplicate_requested_and_foreign_definition_refuse() {
    let mut f = Fake::new();
    f.focus_nodes();
    f.array(VIEW + 0x1138, 0x230000, &[0x231000, 0x237000]);
    f.region(0x237000, 0x2d0);
    f.word(0x237000, 0x4249b90);
    f.word(0x2372b0, VIEW);
    f.word(0x237020, F);
    assert_eq!(f.node_result().unwrap_err().code, "ambiguous_focus_node");
    assert!(f.untouched(0x231010, 8));
    f.word(0x237020, 0xdead0000);
    assert_eq!(f.node_result().unwrap_err().code, "invalid_focus_node");
    assert!(f.untouched(0xdead0000, 8));
}
#[test]
fn focus_nodes_unsupported_control_and_callback_variants_metadata_only() {
    let mut f = Fake::new();
    f.focus_nodes();
    f.word(0x234000, 0x4346b10);
    assert_eq!(
        f.node_result().unwrap()["node_association"]["controls"][0]["binding_verified"],
        false
    );
    assert!(f.untouched(0x234068, 8));
    let mut f = Fake::new();
    f.focus_nodes();
    f.word(0x231040, 0x2916700);
    assert_eq!(
        f.node_result().unwrap()["node_association"]["controls"][0]["binding_verified"],
        false
    );
    assert!(f.untouched(0x236000, 24));
}
#[test]
fn focus_nodes_callback_owner_foreign_and_symbol_alias_refuse() {
    let mut f = Fake::new();
    f.focus_nodes();
    f.word(0x236010, 0xdead0000);
    assert_eq!(f.node_result().unwrap_err().code, "invalid_focus_owner");
    assert!(f.untouched(0x236000, 8));
    let mut f = Fake::new();
    f.focus_nodes();
    f.word(0x2312b8, 0xdead0000);
    assert_eq!(f.node_result().unwrap_err().code, "invalid_focus_node");
    assert!(f.untouched(0x235000, 32));
}
#[test]
fn focus_nodes_final_catalog_record_and_callback_witnesses() {
    for a in [
        VIEW + 0x1138,
        0x230000,
        0x231000,
        0x2312b0,
        0x231020,
        0x231010,
        0x232098,
        0x233000,
        0x2312b8,
        0x235000,
        0x231040,
        0x236010,
        F + 0x18,
    ] {
        let mut f = Fake::new();
        f.focus_nodes();
        f.torn = Some(a);
        assert_eq!(f.node_result().unwrap_err().code, "torn_read", "{a:x}");
    }
}
#[test]
fn focus_node_query_noop_snapshot_path_and_exact_key_refusal() {
    let f = Fake::new();
    assert!(f.result().unwrap()["node_association"].is_null());
    assert!(f.untouched(VIEW + 0x1138, 16));
    let f = Fake::new();
    assert_eq!(
        own_focus::node_detail(&f, "GER_rhine").unwrap_err().code,
        "focus_unavailable"
    );
    assert!(f.untouched(VIEW + 0x1138, 16));
}
#[test]
fn focus_nodes_valid_unrelated_item_never_reads_its_gui_payload() {
    let mut f = Fake::new();
    f.focus_nodes();
    f.region(0x238000, 0x1800);
    f.word(0x238000, 0x42024d0);
    f.word(0x238160, TREE);
    f.string(0x238018, "GER_other");
    f.array(TREE + 0x30, 0x212000, &[F, 0x238000]);
    f.array(DB + 0x58, 0x213000, &[F, 0x238000]);
    f.array(VIEW + 0x1138, 0x230000, &[0x231000, 0x237000]);
    f.region(0x237000, 0x2d0);
    f.word(0x237000, 0x4249b90);
    f.word(0x2372b0, VIEW);
    f.word(0x237020, 0x238000);
    f.word(0x237010, 0xdead0000);
    assert_eq!(
        f.node_result().unwrap()["node_association"]["current_item_count"],
        2
    );
    assert!(f.untouched(0x237010, 8));
    assert!(f.untouched(0xdead0000, 8));
}
