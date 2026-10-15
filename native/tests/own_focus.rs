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
#[path = "support/focus_completion_cases.rs"]
mod completion_cases;
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

#[test]
fn changed_focus_fields_report_bounded_labels_without_addresses_or_payloads() {
    for (address, field) in [
        (PANEL + 0x7d0, "detail_panel+0x7d0 size=8"),
        (WIN + 0xa5, "detail_window+0xa5 size=1"),
        (BUTTON + 0xac, "start_control+0xac size=4"),
    ] {
        let mut f = Fake::new();
        f.torn = Some(address);
        let e = f.result().unwrap_err();
        assert_eq!(e.code, "torn_read");
        assert!(e.message.contains("detail;"), "{}", e.message);
        assert!(e.message.contains(field), "{}", e.message);
        assert!(e.message.len() < 160);
        for pointer in [address, PANEL, WIN, BUTTON] {
            assert!(!e.message.contains(&format!("{pointer:x}")));
            assert!(!e.message.contains(&pointer.to_string()));
        }
    }
    // An existing out-of-object key payload still refuses. Diagnostics must
    // not invent an object owner or leak its address to explain the failure.
    let mut f = Fake::new();
    f.torn = Some(F + 0x28);
    let e = f.result().unwrap_err();
    assert_eq!(e.code, "torn_read");
    assert!(
        e.message.contains("catalog; unclassified size="),
        "{}",
        e.message
    );
    assert!(!e.message.contains(&format!("{:x}", F + 0x28)));
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
impl Fake {
    fn focus_geometry_fixture(&mut self, window: u64, button: u64, text: bool) {
        let render = 0x244000;
        let resource = 0x245000;
        let graphics = 0x240000;
        self.region(render, 0x300);
        self.region(resource, 0x200);
        self.word(button + 0xe8, render);
        self.word(render + 0x138, button);
        self.word(render + 0x168, resource);
        for (obj, table, ti, name, strp) in [
            (
                render,
                if text { 0x43593d8 } else { 0x4356e08 },
                0x4358000,
                if text { "11CTextSprite" } else { "7CSprite" },
                0x350f300,
            ),
            (
                resource,
                if text { 0x4359740 } else { 0x4357160 },
                0x4358100,
                if text {
                    "15CTextSpriteType"
                } else {
                    "11CSpriteType"
                },
                0x350f400,
            ),
        ] {
            self.word(obj, table);
            self.word(table - 16, 0);
            self.word(table - 8, ti);
            self.region(ti, 16);
            self.word(ti + 8, strp);
            self.region(strp, 128);
            self.put(strp, name.as_bytes());
        }
        self.put(render + 0x114, &1.0f32.to_le_bytes());
        self.put(render + 0xb0, &10.0f32.to_le_bytes());
        self.put(render + 0xb4, &(-20.0f32).to_le_bytes());
        self.word(0x4577890, graphics);
        self.region(graphics, 0x500);
        self.word(graphics, 0x42417f8);
        self.word(GUI + 0xb8, graphics);
        self.word(graphics + 0x1f0, 0x43484f0);
        self.word(graphics + 0x350, 0x43484f0);
        for a in [0x241000, 0x242000] {
            self.region(a, 0x160);
            self.word(a, 0x43484f0);
        }
        self.word(window + 0xe70, 0x241000);
        self.word(window + 0xe78, 0x242000);
        self.word(0x242108, 0x241000);
        self.array(0x2410c0, 0x243000, &[0x242000]);
        if text {
            self.int(resource + 0x8c, 3);
            self.int(render + 0x228, 369);
            self.int(render + 0x22c, 34);
            self.int(render + 0x238, 3);
        } else {
            self.int(resource + 0x90, 3);
            self.int(resource + 0x94, 1);
            self.int(resource + 0xa4, 369);
            self.int(resource + 0xa8, 34);
            self.word(resource + 0x1a0, u64::MAX);
            self.word(resource + 0x170, 0xdead0000);
        }
    }
}
#[test]
fn focus_geometry_sprite_standard_node_and_start_share_only_renderer_semantics() {
    for start in [false, true] {
        let mut f = Fake::new();
        let v = if start {
            f.start_binding();
            f.focus_geometry_fixture(WIN, BUTTON, false);
            f.geometry_result().unwrap()["details"][0]["window"]["start"]["ordinary_binding"]
                ["render_diagnostic"]
                .clone()
        } else {
            f.focus_nodes();
            f.focus_geometry_fixture(0x232000, 0x234000, false);
            f.geometry_result().unwrap()["node_association"]["controls"][0]["render_diagnostic"]
                .clone()
        };
        assert_eq!(v["field_layout_verified"], true);
        assert_eq!(
            v["resource_cache"]["diagnostic_dimensions"],
            serde_json::json!([123, 34])
        );
        assert_eq!(v["input_authorized"], false);
        assert_eq!(v["client_rectangle_available"], false);
        assert_eq!(v["parent_chain"]["status"], "detached_ancestor");
        assert!(f.untouched(0xdead0000, 8));
        assert!(f.untouched(if start { BUTTON + 0x110 } else { 0x234110 }, 8));
    }
}
#[test]
fn focus_geometry_text_raw_only_no_frame_resource_aliases() {
    let mut f = Fake::new();
    f.focus_nodes();
    f.focus_geometry_fixture(0x232000, 0x234000, true);
    let v = f.geometry_result().unwrap();
    let g = &v["node_association"]["controls"][0]["render_diagnostic"];
    assert_eq!(g["raw_fields_available"], true);
    assert_eq!(g["hit_projection_available"], false);
    assert_eq!(g["input_authorized"], false);
    assert!(f.untouched(0x2441b0, 8));
    assert!(f.untouched(0x245090, 8));
}
#[test]
fn focus_geometry_wrong_resource_before_scalar_payload_and_owner_refusal() {
    let mut f = Fake::new();
    f.focus_nodes();
    f.focus_geometry_fixture(0x232000, 0x234000, false);
    f.word(0x245000, 0x4359740);
    f.word(0x4359730, 0);
    f.word(0x4359738, 0x4358200);
    f.region(0x4358200, 16);
    f.word(0x4358208, 0x350f500);
    f.region(0x350f500, 128);
    f.put(0x350f500, b"15CTextSpriteType");
    let v = f.geometry_result().unwrap();
    assert_eq!(
        v["node_association"]["controls"][0]["render_diagnostic"]["reason"],
        "resource_layout_unavailable"
    );
    assert!(f.untouched(0x244114, 4));
    assert!(f.untouched(0x245090, 8));
    let mut f = Fake::new();
    f.focus_nodes();
    f.focus_geometry_fixture(0x232000, 0x234000, false);
    f.word(0x244138, BUTTON);
    assert_eq!(f.geometry_result().unwrap_err().code, "invalid_focus_owner");
    assert!(f.untouched(0x244168, 8));
}
#[test]
fn focus_geometry_callback_gate_before_all_renderer_payload_and_final_mutations() {
    let mut f = Fake::new();
    f.focus_nodes();
    f.focus_geometry_fixture(0x232000, 0x234000, false);
    f.word(0x231040, 0xdead0000);
    assert_eq!(
        f.geometry_result().unwrap()["node_association"]["controls"][0]["binding_verified"],
        false
    );
    assert!(f.untouched(0x2340e8, 8));
    for a in [
        0x244168, 0x245000, 0x244114, 0x2440b0, 0x244120, 0x24510f, 0x2451a7, 0x232e70, 0x242108,
    ] {
        let mut f = Fake::new();
        f.focus_nodes();
        f.focus_geometry_fixture(0x232000, 0x234000, false);
        f.torn = Some(a);
        assert_eq!(f.geometry_result().unwrap_err().code, "torn_read", "{a:x}");
    }
}

#[test]
fn focus_geometry_instant_metadata_does_not_borrow_sprite_or_text_payload() {
    let mut f = Fake::new();
    f.focus_nodes();
    f.focus_geometry_fixture(0x232000, 0x234000, false);
    f.word(0x244000, 0x434c5b0);
    f.word(0x434c5a0, 0);
    f.word(0x434c5a8, 0x4358300);
    f.region(0x4358300, 16);
    f.word(0x4358308, 0x350f600);
    f.region(0x350f600, 128);
    f.put(0x350f600, b"14CInstantSprite");
    let v = f.geometry_result().unwrap();
    assert_eq!(
        v["node_association"]["controls"][0]["render_diagnostic"]["reason"],
        "focus_geometry_renderer_unavailable"
    );
    assert!(f.untouched(0x244168, 8));
    assert!(f.untouched(0x2441b0, 8));
    assert!(f.untouched(0x244228, 4));
}

impl Fake {
    fn focus_frame_fixture(&mut self, window: u64, button: u64) {
        self.focus_geometry_fixture(window, button, false);
        self.word(0x244000, 0x434b370);
        self.word(0x2441b0, 0x245000);
        self.word(0x245000, 0x434b668);
        for (table, ti, name, strp) in [
            (0x434b370, 0x4358300, "20CFrameAnimatedSprite", 0x350f500),
            (
                0x434b668,
                0x4358400,
                "24CFrameAnimatedSpriteType",
                0x350f600,
            ),
        ] {
            self.word(table - 16, 0);
            self.word(table - 8, ti);
            self.region(ti, 16);
            self.word(ti + 8, strp);
            self.region(strp, 128);
            self.put(strp, name.as_bytes());
        }
    }
}
#[test]
fn focus_geometry_frame_node_and_start_require_exact_resource_alias_and_no_texture_payload() {
    for start in [false, true] {
        let mut f = Fake::new();
        if start {
            f.start_binding();
            f.focus_frame_fixture(WIN, BUTTON);
        } else {
            f.focus_nodes();
            f.focus_frame_fixture(0x232000, 0x234000);
        }
        let v = f.geometry_result().unwrap();
        let g = if start {
            &v["details"][0]["window"]["start"]["ordinary_binding"]["render_diagnostic"]
        } else {
            &v["node_association"]["controls"][0]["render_diagnostic"]
        };
        assert_eq!(g["field_layout_verified"], true);
        assert_eq!(g["resource_alias_matches"], true);
        assert_eq!(g["resource_cache"]["frame_count_raw"], 3);
        assert_eq!(g["resource_cache"]["texture_width_raw"], 369);
        assert_eq!(g["frame_raw_geometry"]["no_mask_rejection_branch"], true);
        assert_eq!(g["client_rectangle_available"], false);
        assert_eq!(g["input_authorized"], false);
        assert!(f.untouched(0xdead0000, 8));
        assert!(f.untouched(0x244228, 4));
        assert!(f.untouched(if start { BUTTON + 0x110 } else { 0x234110 }, 8));
    }
    let mut f = Fake::new();
    f.focus_nodes();
    f.focus_frame_fixture(0x232000, 0x234000);
    f.word(0x244168, 0xdead0000);
    let v = f.geometry_result().unwrap();
    assert_eq!(
        v["node_association"]["controls"][0]["render_diagnostic"]["reason"],
        "frame_resource_alias_unavailable"
    );
    assert!(f.untouched(0x245090, 4));
    assert!(f.untouched(0xdead0000, 8));
}
#[test]
fn focus_geometry_frame_full_orientation_and_cold_cache_refuse_before_dimension_payload() {
    for (a, b, reason) in [
        (
            0x24510c,
            0x100u32.to_le_bytes().to_vec(),
            "frame_resource_grid_orientation_unavailable",
        ),
        (0x2451d3, vec![1], "frame_resource_cache_unavailable"),
    ] {
        let mut f = Fake::new();
        f.focus_nodes();
        f.focus_frame_fixture(0x232000, 0x234000);
        f.put(a, &b);
        let v = f.geometry_result().unwrap();
        let g = &v["node_association"]["controls"][0]["render_diagnostic"];
        assert_eq!(g["reason"], reason);
        assert_eq!(g["field_layout_verified"], false);
        assert!(f.untouched(0x2450a4, 4));
        assert!(f.untouched(0x244114, 4));
    }
}
#[test]
fn focus_geometry_frame_and_text_new_fields_are_final_snapshot_witnesses() {
    for (text, a) in [
        (false, 0x2441b0),
        (false, 0x24510f),
        (false, 0x2451a7),
        (false, 0x2451d3),
        (false, 0x2450a4),
        (false, 0x245111),
        (false, 0x245170),
        (false, 0x2451d0),
        (false, 0x2440b0),
        (false, 0x244124),
        (false, 0x232e70),
        (true, 0x24508c),
        (true, 0x244228),
        (true, 0x24422c),
        (true, 0x244238),
        (true, 0x244110),
        (true, 0x244114),
        (true, 0x244120),
        (true, 0x242108),
    ] {
        let mut f = Fake::new();
        f.focus_nodes();
        if text {
            f.focus_geometry_fixture(0x232000, 0x234000, true);
        } else {
            f.focus_frame_fixture(0x232000, 0x234000);
        }
        f.torn = Some(a);
        assert_eq!(f.geometry_result().unwrap_err().code, "torn_read", "{a:x}");
    }
}
#[test]
fn focus_geometry_inherits_exact_sprite_and_text_numeric_refusals() {
    let mut f = Fake::new();
    f.focus_nodes();
    f.focus_geometry_fixture(0x232000, 0x234000, false);
    f.put(0x244114, &f32::NAN.to_le_bytes());
    assert_eq!(
        f.geometry_result().unwrap_err().code,
        "invalid_gui_geometry"
    );
    let mut f = Fake::new();
    f.focus_nodes();
    f.focus_geometry_fixture(0x232000, 0x234000, true);
    f.int(0x244238, 0);
    assert_eq!(
        f.geometry_result().unwrap()["node_association"]["controls"][0]["render_diagnostic"]
            ["raw_fields_available"],
        false
    );
    assert!(f.untouched(0x232e70, 8));
}

impl Fake {
    fn geometry_result(&self) -> Result<serde_json::Value> {
        own_focus::node_detail_with_geometry(self, "GER_rhineland", true)
    }
}
#[test]
fn focus_geometry_default_list_and_node_keep_metadata_shape_and_reads() {
    let mut f = Fake::new();
    f.start_binding();
    f.focus_nodes();
    f.focus_geometry_fixture(0x232000, 0x234000, false);
    let v = f.node_result().unwrap();
    assert!(v["node_association"]["controls"][0]
        .get("render_diagnostic")
        .is_none());
    assert!(v["details"][0]["window"]["start"]["ordinary_binding"]
        .get("render_diagnostic")
        .is_none());
    assert!(f.untouched(0x244168, 8));
    assert!(f.untouched(0x232e70, 8));
    let v = f.result().unwrap();
    assert!(v["details"][0]["window"]["start"]["ordinary_binding"]
        .get("render_diagnostic")
        .is_none());
}
impl Fake {
    fn command_context_fixture(&mut self) {
        self.focus_nodes();
        self.word(0x439f450, 0x251000);
        self.word(0x43f3eb8, 0x252000);
        self.array(0x252000, 0x253000, &[0, 0]);
        self.string(0x253020, "GER");
        self.string(CO + 0x10, "Germany");
        self.region(GS + 0x718, 32);
        self.word(GS + 0x718, 0x250000);
        self.word(GS + 0x720, 36);
        self.region(0x250000, 64);
        self.put(0x250000, b"00000000-0000-0000-0000-000000000001");
        self.int(GS + 0x460, 60759371);
        self.int(GS + 0x4b4, 0);
        self.put(ID + 0x6c1, &[1]);
        self.int(GS + 0x620, 1);
        self.int(GS + 0x628, 1);
        self.region(0x254000, 0x100);
        self.word(0x254000, 0x4202638);
        self.word(0x254008, CO);
        self.word(CO + 0x1348, 0x254000);
        self.region(0x255000, 0x200);
        self.word(0x255000, 0x42108f8);
        self.word(0x255130, CO);
        self.int(0x255138, 1);
        self.word(CO + 0xf38, 0x255000);
        for (table, ti, text, name) in [
            (0x4202638, 0x256000, 0x257000, "CNationalFocusProgress"),
            (0x42108f8, 0x256020, 0x257100, "CTechnologyStatus"),
            (0x42024d0, 0x256040, 0x257200, "CNationalFocus"),
        ] {
            self.word(table - 8, ti);
            self.region(ti, 16);
            self.word(ti + 8, text);
            self.region(text, 128);
            self.put(text, format!("{}{name}", name.len()).as_bytes());
        }
        self.string(F + 0x60, "Rhineland");
    }
}
#[test]
fn focus_command_same_reader_context_and_default_paths_unchanged() {
    let mut f = Fake::new();
    f.command_context_fixture();
    let (body, c) = own_focus::command_owned(&f, "GER_rhineland", false).unwrap();
    assert_eq!(c.anchors, [CO, 0x254000, TREE, VIEW, GUI, MAN, 0x255000]);
    assert_eq!(c.campaign.player, "GER");
    assert_eq!(c.selected, vec![[PANEL, F]]);
    assert_eq!(c.active_key, None);
    assert_eq!(body["command_observation"]["input_authorized"], false);
    assert!(f.untouched(0x232e70, 8));
    let f = Fake::new();
    assert!(f.result().unwrap().get("command_observation").is_none());
    assert!(f.untouched(CO + 0x1348, 8));
    assert!(f.untouched(GS + 0x718, 32));
}
#[test]
fn focus_command_active_focus_own_catalog_and_progress_owner_before_payload() {
    let mut f = Fake::new();
    f.command_context_fixture();
    f.word(0x254010, F);
    let (_, c) = own_focus::command_owned(&f, "GER_rhineland", false).unwrap();
    assert_eq!(c.active_key.as_deref(), Some("GER_rhineland"));
    let mut f = Fake::new();
    f.command_context_fixture();
    f.word(0x254008, 0xdead0000);
    assert_eq!(
        own_focus::command_owned(&f, "GER_rhineland", false)
            .unwrap_err()
            .code,
        "invalid_focus_owner"
    );
    assert!(f.untouched(0x254010, 8));
    let mut f = Fake::new();
    f.command_context_fixture();
    f.word(0x254010, 0xdead0000);
    assert_eq!(
        own_focus::command_owned(&f, "GER_rhineland", false)
            .unwrap_err()
            .code,
        "invalid_focus_owner"
    );
    assert!(f.untouched(0xdead0000, 8));
    assert!(f.untouched(GS + 0x718, 32));
}
#[test]
fn focus_command_late_progress_selection_type_registry_and_clock_witnesses() {
    for a in [
        CO + 0x1348,
        0x254000,
        0x254008,
        0x254010,
        PANEL + 0x7d0,
        PANEL + 0x7b8,
        VIEW + 0x2f0,
        WIN + 0x98,
        BUTTON + 0x68,
        GS + 0x718,
        GS + 0x460,
        GS + 0x628,
        CO + 0x1338,
    ] {
        let mut f = Fake::new();
        f.command_context_fixture();
        f.torn = Some(a);
        assert_eq!(
            own_focus::command_owned(&f, "GER_rhineland", false)
                .unwrap_err()
                .code,
            "torn_read",
            "{a:x}"
        );
    }
}
#[test]
fn focus_command_positive_tag_scope_before_payload_and_wrong_progress_type() {
    let mut f = Fake::new();
    f.command_context_fixture();
    f.int(GS + 0x518, 0);
    assert_eq!(
        own_focus::command_owned(&f, "GER_rhineland", false)
            .unwrap_err()
            .code,
        "player_unavailable"
    );
    assert!(f.untouched(CO, 8));
    assert!(f.untouched(CO + 0x1348, 8));
    let mut f = Fake::new();
    f.command_context_fixture();
    f.word(0x254000, 0x42024d0);
    assert_eq!(
        own_focus::command_owned(&f, "GER_rhineland", false)
            .unwrap_err()
            .code,
        "invalid_focus_type"
    );
    assert!(f.untouched(0x254008, 8));
}

#[test]
fn focus_command_matching_detail_scope_anchor_and_ambiguity() {
    let mut f = Fake::new();
    f.command_context_fixture();
    let (_, c) = own_focus::command_owned(&f, "GER_rhineland", false).unwrap();
    assert_eq!(c.matching_details, vec![[PANEL, F, WIN]]);
    let bytes: Vec<_> = (0..0x800).map(|i| f.data[&(PANEL + i)]).collect();
    f.put(0x258000, &bytes);
    f.array(VIEW + 0x1180, 0x214000, &[PANEL, 0x258000]);
    assert_eq!(
        own_focus::command_owned(&f, "GER_rhineland", false)
            .unwrap_err()
            .code,
        "ambiguous_focus_detail"
    );
}
#[test]
fn focus_command_exact_key_before_progress_and_owned_slot_scope() {
    let mut f = Fake::new();
    f.command_context_fixture();
    assert_eq!(
        own_focus::command_owned(&f, "GER_unknown", false)
            .unwrap_err()
            .code,
        "focus_unavailable"
    );
    assert!(f.untouched(CO + 0x1348, 8));
    let mut f = Fake::new();
    f.command_context_fixture();
    f.region(0x259000, 0x60);
    f.word(0x259000, 0x4210898);
    f.word(0x259010, 0x255000);
    f.array(0x2550a0, 0x25a000, &[0x259000]);
    f.word(0x4210890, 0x256060);
    f.region(0x256060, 16);
    f.word(0x256068, 0x257300);
    f.region(0x257300, 128);
    f.put(0x257300, b"13CResearchSlot");
    let (_, c) = own_focus::command_owned(&f, "GER_rhineland", false).unwrap();
    assert_eq!(c.slots, vec![0x259000]);
    f.reads.borrow_mut().clear();
    f.torn = Some(0x259000);
    assert_eq!(
        own_focus::command_owned(&f, "GER_rhineland", false)
            .unwrap_err()
            .code,
        "torn_read"
    );
}
#[test]
fn focus_command_optin_geometry_shares_context_final_witnesses() {
    let mut f = Fake::new();
    f.command_context_fixture();
    f.focus_geometry_fixture(0x232000, 0x234000, false);
    let (v, c) = own_focus::command_owned(&f, "GER_rhineland", true).unwrap();
    assert_eq!(
        v["node_association"]["controls"][0]["render_diagnostic"]["field_layout_verified"],
        true
    );
    assert!(c.campaign.paused);
    assert_eq!(v["input_authorized"], false);
    let mut f = Fake::new();
    f.command_context_fixture();
    f.focus_geometry_fixture(0x232000, 0x234000, false);
    f.torn = Some(0x2451a7);
    assert_eq!(
        own_focus::command_owned(&f, "GER_rhineland", true)
            .unwrap_err()
            .code,
        "torn_read"
    );
}

#[test]
fn focus_command_duplicate_slot_scope_refused_before_slot_payload() {
    let mut f = Fake::new();
    f.command_context_fixture();
    f.array(0x2550a0, 0x25a000, &[0x259000, 0x259000]);
    // Deliberately unmapped slot. Neither direct context nor ClockContext may
    // dereference it before the duplicate-pointer scope check.
    assert_eq!(
        own_focus::command_owned(&f, "GER_rhineland", false)
            .unwrap_err()
            .code,
        "invalid_focus_context"
    );
    assert!(f.untouched(0x259000, 0x60));
    assert!(f.untouched(GS + 0x718, 32));
}
#[test]
fn focus_command_duplicate_panel_pointer_refused_before_selected_payload() {
    let mut f = Fake::new();
    f.command_context_fixture();
    f.array(VIEW + 0x1180, 0x214000, &[PANEL, PANEL]);
    assert_eq!(
        own_focus::command_owned(&f, "GER_rhineland", false)
            .unwrap_err()
            .code,
        "invalid_focus_catalog"
    );
    assert!(f.untouched(PANEL + 0x7b8, 8));
    assert!(f.untouched(PANEL + 0x7d0, 8));
}

impl Fake {
    fn own_focus_ui_fixture(&mut self) {
        self.command_context_fixture();
        self.int(MAN + 0x3d8, 13);
        self.region(0x260000, 0xf00);
        self.word(0x260000, 0x4347d58);
        self.word(0x260098, GUI);
        self.put(0x2600a5, &[8]);
        self.word(VIEW + 0x2f8, 0x260000);
        self.region(0x261000, 0xe00);
        self.word(0x261000, 0x424de40);
        self.word(0x2612f0, ID);
        self.int(0x261e20, 1);
        self.int(0x261e24, 6);
        self.word(MAN + 0xf8, 0x261000);
    }
    fn ui(&self) -> hoi4_live_native::own_focus::FocusContext {
        own_focus::command_owned(self, "GER_rhineland", false)
            .unwrap()
            .1
    }
    fn research_info_fixture(&mut self) {
        self.array(0x261dc0, 0x263000, &[0x262000]);
        self.region(0x262000, 0xb00);
        self.word(0x262000, 0x430bbf0);
        self.word(0x262a78, ID);
        self.word(0x262a80, 0x261000);
        self.word(0x262a88, 0x264000);
        self.region(0x264000, 0xf00);
        self.word(0x264000, 0x4347d58);
        self.word(0x264098, GUI);
        // Poison selected technology. GUI blocker reads must not touch it or payload.
        self.word(0x262a98, 0xdead0000);
    }
}
#[test]
fn focus_ui_producer_local_mode_visibility_and_complete_counts_not_modal_claim() {
    let mut f = Fake::new();
    f.own_focus_ui_fixture();
    let c = f.ui();
    let u = c.ui_facts.unwrap();
    assert_eq!(u.focus_mode_selected(), Some(true));
    assert_eq!(u.focus_view_local_ready(), Some(true));
    assert_eq!(u.focus_detail_local_counts(), Some([1, 0]));
    assert_eq!(u.own_research_info_local_counts(), Some([0, 0]));
    assert_eq!(u.local_summary()["global_modal_clear_available"], false);
    let f = Fake::new();
    f.result().unwrap();
    assert!(f.untouched(MAN + 0x3d8, 4));
    assert!(f.untouched(VIEW + 0x2f8, 8));
    assert!(f.untouched(MAN + 0xf8, 8));
}
#[test]
fn focus_ui_animation_and_null_are_explicitly_distinct_from_clear() {
    let mut f = Fake::new();
    f.own_focus_ui_fixture();
    f.int(0x260ef8, 2);
    assert_eq!(f.ui().ui_facts.unwrap().focus_view_local_ready(), None);
    let mut f = Fake::new();
    f.own_focus_ui_fixture();
    f.word(VIEW + 0x2f8, 0);
    assert_eq!(f.ui().ui_facts.unwrap().focus_view_local_ready(), None);
    let mut f = Fake::new();
    f.own_focus_ui_fixture();
    f.word(PANEL + 0x7c8, 0);
    assert_eq!(f.ui().ui_facts.unwrap().focus_detail_local_counts(), None);
    let mut f = Fake::new();
    f.own_focus_ui_fixture();
    f.int(MAN + 0x3d8, 99);
    assert_eq!(f.ui().ui_facts.unwrap().focus_mode_selected(), None);
}
#[test]
fn focus_ui_complete_nonmatching_details_are_inspected_for_local_flags_only() {
    let mut f = Fake::new();
    f.own_focus_ui_fixture();
    f.region(0x265000, 0x1800);
    f.word(0x265000, 0x42024d0);
    f.word(0x265160, TREE);
    f.string(0x265018, "GER_other");
    f.array(TREE + 0x30, 0x212000, &[F, 0x265000]);
    f.array(DB + 0x58, 0x213000, &[F, 0x265000]);
    f.region(0x266000, 0x800);
    f.word(0x266000, 0x43023f8);
    f.word(0x2667b8, ID);
    f.word(0x2667c0, VIEW);
    f.word(0x2667d0, 0x265000);
    f.word(0x2667c8, 0x267000);
    f.region(0x267000, 0xf00);
    f.word(0x267000, 0x4347d58);
    f.word(0x267098, GUI);
    f.put(0x2670a5, &[8]);
    f.array(VIEW + 0x1180, 0x214000, &[PANEL, 0x266000]);
    assert_eq!(
        f.ui().ui_facts.unwrap().focus_detail_local_counts(),
        Some([2, 0])
    );
    assert!(f.untouched(0x2679a0, 16));
}
#[test]
fn focus_ui_research_info_owns_scope_before_flags_no_selected_tech_payload() {
    let mut f = Fake::new();
    f.own_focus_ui_fixture();
    f.research_info_fixture();
    assert_eq!(
        f.ui().ui_facts.unwrap().own_research_info_local_counts(),
        Some([0, 0])
    );
    assert!(f.untouched(0x262a98, 8));
    assert!(f.untouched(0xdead0000, 8));
    let mut f = Fake::new();
    f.own_focus_ui_fixture();
    f.research_info_fixture();
    f.put(0x2640a5, &[8]);
    assert_eq!(
        f.ui().ui_facts.unwrap().own_research_info_local_counts(),
        Some([1, 0])
    );
    let mut f = Fake::new();
    f.own_focus_ui_fixture();
    f.research_info_fixture();
    f.word(0x262a78, 0xdead0000);
    assert_eq!(
        own_focus::command_owned(&f, "GER_rhineland", false)
            .unwrap_err()
            .code,
        "invalid_focus_owner"
    );
    assert!(f.untouched(0x262a88, 8));
    assert!(f.untouched(0x2640a5, 1));
    let mut f = Fake::new();
    f.own_focus_ui_fixture();
    f.research_info_fixture();
    f.int(0x261e20, 2);
    assert_eq!(
        f.ui().ui_facts.unwrap().own_research_info_local_counts(),
        None
    );
    assert!(f.untouched(0x261dc0, 16));
}
#[test]
fn focus_ui_own_registry_type_and_late_flags_are_final_witnesses() {
    let mut f = Fake::new();
    f.own_focus_ui_fixture();
    f.word(0x260098, 0xdead0000);
    assert_eq!(
        own_focus::command_owned(&f, "GER_rhineland", false)
            .unwrap_err()
            .code,
        "invalid_focus_owner"
    );
    assert!(f.untouched(0x2600a5, 1));
    for a in [
        MAN + 0x3d8,
        VIEW + 0x2f8,
        0x260000,
        0x260098,
        0x2600a5,
        0x260eb0,
        0x260ef8,
        MAN + 0xf8,
        0x2612f0,
        0x261e20,
        0x261e24,
        0x261dc0,
        0x262a78,
        0x262a88,
        0x2640a5,
    ] {
        let mut f = Fake::new();
        f.own_focus_ui_fixture();
        f.research_info_fixture();
        f.torn = Some(a);
        assert_eq!(
            own_focus::command_owned(&f, "GER_rhineland", false)
                .unwrap_err()
                .code,
            "torn_read",
            "{a:x}"
        );
    }
}

#[test]
fn focus_node_geometry_state_flags_are_bounded_and_final_witnesses() {
    let mut f = Fake::new();
    f.focus_nodes();
    f.focus_geometry_fixture(0x232000, 0x234000, false);
    f.int(0x2340ac, 4);
    assert_eq!(f.geometry_result().unwrap_err().code, "invalid_focus_node");
    assert!(f.untouched(0x2340e8, 8));
    for a in [0x2340ac, 0x234074] {
        let mut f = Fake::new();
        f.focus_nodes();
        f.focus_geometry_fixture(0x232000, 0x234000, false);
        f.torn = Some(a);
        assert_eq!(f.geometry_result().unwrap_err().code, "torn_read");
    }
}

#[test]
fn focus_ui_transition_one_and_unknown_never_supply_ready_or_clear_counts() {
    for state in [1, -1, 3] {
        for offset in [0xeb0, 0xef8] {
            let mut f = Fake::new();
            f.own_focus_ui_fixture();
            f.int(0x260000 + offset, state);
            let u = f.ui().ui_facts.unwrap();
            assert_eq!(u.focus_view_local_ready(), None);
            assert_eq!(u.focus_view_local_hidden(), None);
            let mut f = Fake::new();
            f.own_focus_ui_fixture();
            f.put(WIN + 0xa5, &[0]);
            f.int(WIN + offset, state);
            assert_eq!(f.ui().ui_facts.unwrap().focus_detail_local_counts(), None);
            let mut f = Fake::new();
            f.own_focus_ui_fixture();
            f.research_info_fixture();
            f.int(0x264000 + offset, state);
            assert_eq!(
                f.ui().ui_facts.unwrap().own_research_info_local_counts(),
                None
            );
            assert!(f.untouched(0x262a98, 8));
        }
    }
}
#[test]
fn focus_ui_positive_literal_hidden_and_transition_witnesses() {
    let mut f = Fake::new();
    f.own_focus_ui_fixture();
    f.put(0x2600a5, &[0]);
    let u = f.ui().ui_facts.unwrap();
    assert_eq!(u.focus_view_local_hidden(), Some(true));
    assert_eq!(u.focus_view_local_ready(), Some(false));
    for at in [0x2600a5, 0x260eb0, 0x260ef8] {
        let mut f = Fake::new();
        f.own_focus_ui_fixture();
        f.torn = Some(at);
        assert_eq!(
            own_focus::command_owned(&f, "GER_rhineland", false)
                .unwrap_err()
                .code,
            "torn_read"
        );
    }
}

// Exercise the production focus-state join on every host with generated own maps.
use hoi4_live_native::{actions, error, focus_stage, game, platform, politics_focus};
#[allow(dead_code)]
#[path = "../src/launch/linux/clock/focus/state.rs"]
mod state;
include!("support/focus_service_cases.rs");

#[test]
fn focus_start_alternate_event_manager_refuses_before_capture_and_geometry() {
    for (glue, known_capture) in [(PANEL + 0x2a8, 0x221000), (PANEL + 0x530, 0x221020)] {
        for geometry in [false, true] {
            let mut f = Fake::new();
            f.start_binding();
            f.word(glue + 0x1d8, 0xdead0000);
            f.word(glue + 0x1c8, 0xdead1000);
            f.word(glue + 0x1e0, 0xdead2000);
            let v = if geometry {
                f.geometry_result().unwrap()["details"][0]["window"]["start"]["ordinary_binding"]
                    .clone()
            } else {
                f.binding()
            };
            assert_eq!(v["available"], false);
            assert_eq!(v["reason"], "additional_event_variant_unavailable");
            assert_eq!(v["callback_route_verified"], false);
            assert!(f.untouched(glue + 0x1c8, 8));
            assert!(f.untouched(glue + 0x1e0, 8));
            assert!(f.untouched(0xdead0000, 8));
            assert!(f.untouched(0xdead1000, 24));
            assert!(f.untouched(known_capture, 24));
            assert!(f.untouched(BUTTON + 0xe8, 8));
            assert!(f.untouched(0x222000, 0x140));
        }
    }
}
#[test]
fn focus_start_zero_alternate_manager_preserves_known_event_branch() {
    let mut f = Fake::new();
    f.start_binding();
    let v = f.binding();
    assert_eq!(v["available"], true);
    assert_eq!(v["callback_route_verified"], true);
    assert_eq!(
        v["reason"],
        "own_selected_event_start_and_noop_listener_verified"
    );
    assert!(f.reads.borrow().contains(&(PANEL + 0x2a8 + 0x1d8, 8)));
    assert!(f.reads.borrow().contains(&(PANEL + 0x530 + 0x1d8, 8)));
    assert_eq!(v["input_authorized"], false);
}
#[test]
fn focus_start_alternate_manager_is_final_witness_on_valid_and_refused_paths() {
    for glue in [PANEL + 0x2a8, PANEL + 0x530] {
        for initial in [0, 1] {
            let mut f = Fake::new();
            f.start_binding();
            f.word(glue + 0x1d8, initial);
            f.torn = Some(glue + 0x1d8);
            assert_eq!(f.result().unwrap_err().code, "torn_read");
        }
    }
}

impl Fake {
    fn start_listener_targets(&mut self, targets: &[u64]) {
        self.word(BUTTON + 0x88, if targets.is_empty() { 0 } else { 0x220000 });
        self.word(
            BUTTON + 0x90,
            if targets.is_empty() {
                0
            } else {
                0x220000 + (targets.len() as u64 - 1) * 32
            },
        );
        self.int(BUTTON + 0x98, targets.len() as i32);
        self.region(0x220000, targets.len() * 32);
        for (i, target) in targets.iter().enumerate() {
            let node = 0x220000 + i as u64 * 32;
            self.word(node, *target);
            self.word(node + 8, if i == 0 { 0 } else { node - 32 });
            self.word(
                node + 0x10,
                if i + 1 == targets.len() { 0 } else { node + 32 },
            );
        }
    }
}
#[test]
fn focus_start_topology_zero_noop_only_and_unknown_remain_unavailable_without_target_reads() {
    for (targets, event, noop, other) in [
        (vec![], false, false, false),
        (vec![PANEL + 0x530], false, true, false),
        (vec![PANEL + 0x2a8, 0xdead0000], true, false, true),
        (
            vec![PANEL + 0x2a8, PANEL + 0x530, 0xdead0000],
            true,
            true,
            true,
        ),
    ] {
        let mut f = Fake::new();
        f.start_binding();
        f.start_listener_targets(&targets);
        let v = f.binding();
        let t = &v["listener_topology"];
        assert_eq!(v["available"], false);
        assert_eq!(v["reason"], "event_binding_unavailable");
        assert_eq!(v["callback_route_verified"], false);
        assert_eq!(v["input_authorized"], false);
        assert_eq!(t["complete_bounded_list"], true);
        assert_eq!(t["listener_count"], targets.len());
        assert_eq!(t["selected_event_target_present"], event);
        assert_eq!(t["noop_target_present"], noop);
        assert_eq!(t["other_target_present"], other);
        assert_eq!(t["matches_admitted_topology"], false);
        assert!(f.untouched(PANEL + 0x2a8, 8));
        assert!(f.untouched(PANEL + 0x530, 8));
        assert!(f.untouched(0xdead0000, 24));
        assert!(f.untouched(BUTTON + 0xe8, 8));
    }
}
#[test]
fn focus_start_topology_existing_two_target_acceptance_and_early_unknown_are_preserved() {
    let mut f = Fake::new();
    f.start_binding();
    let v = f.binding();
    assert_eq!(v["available"], true);
    assert_eq!(v["callback_route_verified"], true);
    assert_eq!(v["listener_topology"]["matches_admitted_topology"], true);
    assert_eq!(v["listener_topology"]["count_matches_two"], true);
    assert_eq!(v["listener_topology"]["other_target_present"], false);
    let mut f = Fake::new();
    f.start_binding();
    f.word(BUTTON + 0xa0, 0xdead0000);
    let v = f.binding();
    assert!(v.get("listener_topology").is_none());
    assert!(f.untouched(BUTTON + 0x98, 4));
}
#[test]
fn focus_start_topology_malformed_links_duplicates_and_bounds_still_refuse() {
    for kind in 0..4 {
        let mut f = Fake::new();
        f.start_binding();
        match kind {
            0 => f.start_listener_targets(&[PANEL + 0x2a8, PANEL + 0x2a8]),
            1 => f.word(0x220010, 0x220000),
            2 => f.put(0x220018, &[1]),
            _ => f.int(BUTTON + 0x98, 17),
        }
        assert_eq!(f.result().unwrap_err().code, "invalid_focus_binding");
        assert!(f.untouched(0x221000, 24));
        assert!(f.untouched(BUTTON + 0xe8, 8));
    }
}
#[test]
fn focus_start_topology_count_and_target_mutations_are_final_witnesses_on_unavailable_path() {
    for at in [BUTTON + 0x98, 0x220000, 0x220010, BUTTON + 0x90] {
        let mut f = Fake::new();
        f.start_binding();
        f.start_listener_targets(&[PANEL + 0x530]);
        f.torn = Some(at);
        assert_eq!(f.result().unwrap_err().code, "torn_read");
    }
}

#[test]
fn focus_start_selected_singleton_proves_route_without_absent_noop_payload() {
    let mut f = Fake::new();
    f.start_binding();
    f.start_listener_targets(&[PANEL + 0x2a8]);
    let v = f.binding();
    assert_eq!(v["available"], true);
    assert_eq!(v["callback_route_verified"], true);
    assert_eq!(v["listener_topology"]["selected_event_only"], true);
    assert_eq!(v["reason"], "own_selected_event_start_listener_verified");
    assert_eq!(v["input_authorized"], false);
    assert_eq!(v["effective_routing_verified"], false);
    assert!(f.untouched(PANEL + 0x530, 0x270));
    assert!(f.untouched(0x221020, 24));
}
#[test]
fn focus_start_selected_singleton_keeps_capture_alternate_and_renderer_guards() {
    for kind in 0..4 {
        let mut f = Fake::new();
        f.start_binding();
        f.start_listener_targets(&[PANEL + 0x2a8]);
        match kind {
            0 => f.word(PANEL + 0x2a8 + 0x1d8, 0xdead0000),
            1 => f.word(0x221000 + 0x10, PANEL + 8),
            2 => f.word(0x221000, 0x2833390),
            _ => f.word(0x222138, BUTTON + 8),
        }
        let result = f.result();
        match kind {
            0 | 2 => assert_eq!(
                result.unwrap()["details"][0]["window"]["start"]["ordinary_binding"]["available"],
                false
            ),
            _ => assert_eq!(result.unwrap_err().code, "invalid_focus_owner"),
        }
        assert!(f.untouched(PANEL + 0x530, 0x270));
        if kind == 0 {
            assert!(f.untouched(0x221000, 24));
            assert!(f.untouched(BUTTON + 0xe8, 8));
        }
    }
}
#[test]
fn focus_start_selected_route_selector_zero_required_for_both_topologies() {
    for singleton in [false, true] {
        for selector in [-1, 1, 2] {
            let mut f = Fake::new();
            f.start_binding();
            if singleton {
                f.start_listener_targets(&[PANEL + 0x2a8]);
            }
            f.int(BUTTON + 8, selector);
            let v = f.binding();
            assert_eq!(v["reason"], "selected_event_selector_unavailable");
            assert_eq!(v["available"], false);
            assert!(f.untouched(0x221000, 24));
            assert!(f.untouched(BUTTON + 0xe8, 8));
        }
    }
}
#[test]
fn focus_start_selected_singleton_all_route_reads_remain_final_witnesses() {
    for at in [
        BUTTON + 8,
        BUTTON + 0x98,
        0x220000,
        0x220010,
        BUTTON + 0x90,
        PANEL + 0x2a8 + 0x1d8,
        0x221000,
        0x221010,
        0x222138,
    ] {
        let mut f = Fake::new();
        f.start_binding();
        f.start_listener_targets(&[PANEL + 0x2a8]);
        f.torn = Some(at);
        assert_eq!(f.result().unwrap_err().code, "torn_read");
    }
}

#[test]
fn selected_detail_exposes_start_window_even_when_node_symbol_route_is_unavailable() {
    let mut f = Fake::new();
    f.start_binding();
    f.start_listener_targets(&[PANEL + 0x2a8]);
    f.focus_nodes();
    f.word(0x234000, 0x4346b10); // Node symbol is not the detail Start button.
    let original = f.node_result().unwrap();
    assert_eq!(
        original["node_association"]["controls"][0]["binding_verified"],
        false
    );
    let value = own_focus::selected_detail(&f, "GER_rhineland", false).unwrap();
    assert_eq!(value["selected_key"], "GER_rhineland");
    assert_eq!(value["selected_detail_count"], 1);
    assert_eq!(value["scope"], "own_selected_focus_detail_only");
    assert_eq!(value["window"], original["details"][0]["window"]);
    let binding = &value["window"]["start"]["ordinary_binding"];
    assert_eq!(binding["available"], true);
    assert_eq!(binding["callback_route_verified"], true);
    assert_eq!(binding["renderer_control_join_verified"], true);
    assert_eq!(binding["renderer_type_encoding"], "11CTextSprite");
    assert_eq!(
        binding["reason"],
        "own_selected_event_start_listener_verified"
    );
    assert_eq!(binding["listener_topology"]["noop_target_present"], false);
    assert_eq!(value["input_authorized"], false);
    assert_eq!(binding["input_authorized"], false);
    assert!(value.get("node_association").is_none());
    assert!(value.get("focuses").is_none());
    assert!(f.untouched(PANEL + 0x530, 0x270));
    assert!(f.untouched(0x221020, 24));
}

#[test]
fn selected_detail_keeps_window_witnesses_without_tree_node_reads() {
    let mut original = Fake::new();
    original.start_binding();
    original.focus_nodes();
    let full = original.node_result().unwrap();
    let mut compact = Fake::new();
    compact.start_binding();
    compact.focus_nodes();
    let value = own_focus::selected_detail(&compact, "GER_rhineland", false).unwrap();
    assert_eq!(value["window"], full["details"][0]["window"]);
    assert_eq!(compact.checks.get(), 2);
    assert!(compact.untouched(VIEW + 0x1138, 16));
    assert!(compact.untouched(0x230000, 0x7010));
    assert!(
        value["freshness"]["read_calls"].as_u64().unwrap()
            < full["freshness"]["read_calls"].as_u64().unwrap()
    );
    assert!(
        value["freshness"]["read_bytes"].as_u64().unwrap()
            < full["freshness"]["read_bytes"].as_u64().unwrap()
    );
}

#[test]
fn selected_detail_refuses_missing_selection_and_missing_window_explicitly() {
    for case in 0..3 {
        let mut f = Fake::new();
        match case {
            0 => f.array(VIEW + 0x1180, 0, &[]),
            1 => f.word(PANEL + 0x7d0, 0),
            _ => f.word(PANEL + 0x7c8, 0),
        }
        assert_eq!(
            own_focus::selected_detail(&f, "GER_rhineland", false)
                .unwrap_err()
                .code,
            "focus_detail_unavailable"
        );
        assert!(f.untouched(WIN, 0x1800));
        assert_eq!(f.checks.get(), 2);
    }
}

#[test]
fn selected_detail_refuses_wrong_key_or_multiple_selected_details_without_guessing() {
    let mut f = Fake::new();
    f.region(0x238000, 0x1800);
    f.word(0x238000, 0x42024d0);
    f.word(0x238160, TREE);
    f.string(0x238018, "GER_other");
    f.array(TREE + 0x30, 0x212000, &[F, 0x238000]);
    f.array(DB + 0x58, 0x213000, &[F, 0x238000]);
    f.word(PANEL + 0x7d0, 0x238000);
    assert_eq!(
        own_focus::selected_detail(&f, "GER_rhineland", false)
            .unwrap_err()
            .code,
        "focus_detail_not_selected"
    );
    assert!(f.untouched(WIN, 0x1800));
    let mut f = Fake::new();
    let second = 0x270000;
    f.region(second, 0x800);
    f.word(second, 0x43023f8);
    f.word(second + 0x7b8, ID);
    f.word(second + 0x7c0, VIEW);
    f.word(second + 0x7d0, F);
    f.word(second + 0x7c8, 0);
    f.array(VIEW + 0x1180, 0x214000, &[PANEL, second]);
    assert_eq!(
        own_focus::selected_detail(&f, "GER_rhineland", false)
            .unwrap_err()
            .code,
        "ambiguous_focus_detail"
    );
    assert_eq!(f.checks.get(), 2);
}

#[test]
fn selected_detail_preserves_actual_unavailable_and_absent_start_fields() {
    let mut f = Fake::new();
    f.start_binding();
    f.start_listener_targets(&[PANEL + 0x530]);
    let value = own_focus::selected_detail(&f, "GER_rhineland", false).unwrap();
    let binding = &value["window"]["start"]["ordinary_binding"];
    assert_eq!(binding["available"], false);
    assert_eq!(binding["reason"], "event_binding_unavailable");
    assert_eq!(
        binding["listener_topology"]["selected_event_target_present"],
        false
    );
    assert!(f.untouched(BUTTON + 0xe8, 8));
    let mut f = Fake::new();
    f.string(0x216000, "other");
    let value = own_focus::selected_detail(&f, "GER_rhineland", false).unwrap();
    assert_eq!(value["window"]["start"]["materialized"], false);
    assert!(value["window"]["start"]["ordinary_binding"].is_null());
    assert!(value["window"]["start"].get("ordinary_binding").is_none());
    assert!(f.untouched(BUTTON, 0x1800));
}

#[test]
fn selected_detail_geometry_is_optional_and_preserves_only_existing_diagnostics() {
    let mut f = Fake::new();
    f.start_binding();
    f.focus_geometry_fixture(WIN, BUTTON, true);
    let value = own_focus::selected_detail(&f, "GER_rhineland", false).unwrap();
    assert!(value["window"]["start"]["ordinary_binding"]
        .get("render_diagnostic")
        .is_none());
    assert!(f.untouched(0x244168, 8));
    let value = own_focus::selected_detail(&f, "GER_rhineland", true).unwrap();
    let binding = &value["window"]["start"]["ordinary_binding"];
    assert_eq!(binding["available"], true);
    assert_eq!(binding["render_diagnostic"]["raw_fields_available"], true);
    assert_eq!(
        binding["render_diagnostic"]["hit_projection_available"],
        false
    );
    assert_eq!(binding["render_diagnostic"]["input_authorized"], false);
    assert!(value.get("client_point").is_none());
    assert!(value.get("coordinates").is_none());
    assert!(f.untouched(0x2441b0, 8));
}

#[test]
fn selected_detail_keeps_original_torn_and_process_refusals() {
    for at in [
        GS + 0x518,
        CO + 0x1338,
        F + 0x18,
        VIEW + 0x1180,
        PANEL + 0x7d0,
        WIN + 0x98,
        BUTTON + 0x80,
        0x220000,
        PANEL + 0x2a8 + 0x1d8,
        BUTTON + 0xe8,
        0x222138,
    ] {
        let mut f = Fake::new();
        f.start_binding();
        f.focus_nodes();
        f.torn = Some(at);
        assert_eq!(
            own_focus::selected_detail(&f, "GER_rhineland", false)
                .unwrap_err()
                .code,
            "torn_read",
            "{at:x}"
        );
    }
    let mut f = Fake::new();
    f.fail_final = true;
    assert_eq!(
        own_focus::selected_detail(&f, "GER_rhineland", false)
            .unwrap_err()
            .code,
        "ownership_changed"
    );
    let f = Fake::new();
    assert_eq!(
        own_focus::selected_detail(&f, "", false).unwrap_err().code,
        "invalid_argument"
    );
    assert_eq!(f.checks.get(), 0);
    assert!(f.reads.borrow().is_empty());
}

#[path = "support/focus_research_root_cases.rs"]
mod focus_research_root_cases;

#[test]
fn selected_detail_skips_unmapped_or_torn_tree_nodes_but_node_query_refuses() {
    for unmapped in [false, true] {
        let mut f = Fake::new();
        f.start_binding();
        f.focus_nodes();
        if unmapped {
            f.data.remove(&(VIEW + 0x1138));
        } else {
            f.torn = Some(0x2340ac); // Unrelated node disabled state.
        }
        let v = own_focus::selected_detail(&f, "GER_rhineland", false).unwrap();
        assert_eq!(v["window"]["start"]["ordinary_binding"]["available"], true);
        assert!(f.untouched(VIEW + 0x1138, 16));
        assert!(f.untouched(0x234000, 0x110));
        f.reads.borrow_mut().clear();
        assert_eq!(
            f.node_result().unwrap_err().code,
            if unmapped {
                "fixture_unmapped"
            } else {
                "torn_read"
            }
        );
    }
}

#[test]
fn selected_command_retains_full_context_without_node_payload() {
    let mut full = Fake::new();
    full.own_focus_ui_fixture();
    full.start_binding();
    let (original, context) = own_focus::command_owned(&full, "GER_rhineland", false).unwrap();
    let mut narrow = Fake::new();
    narrow.own_focus_ui_fixture();
    narrow.start_binding();
    narrow.data.remove(&(VIEW + 0x1138));
    let (observed, selected_context) =
        own_focus::command_selected_detail(&narrow, "GER_rhineland", false).unwrap();
    assert_eq!(selected_context, context);
    assert_eq!(observed["details"], original["details"]);
    assert_eq!(
        observed["command_observation"],
        original["command_observation"]
    );
    assert!(observed["node_association"].is_null());
    assert!(narrow.untouched(VIEW + 0x1138, 16));
    assert!(narrow.untouched(0x230000, 0x7010));
    assert_eq!(narrow.checks.get(), 2);
}

#[test]
fn selected_command_still_refuses_changed_context_start_and_geometry() {
    for at in [
        GS + 0x460,
        GS + 0x718,
        CO + 0x1348,
        CO + 0x1338,
        0x254008,
        0x254010,
        PANEL + 0x7b8,
        PANEL + 0x7d0,
        WIN + 0x98,
        BUTTON + 0x68,
        BUTTON + 0xac,
        BUTTON + 0x74,
        BUTTON + 0x80,
        MAN + 0x3d8,
        0x2600a5,
        0x261e20,
        0x244238,
    ] {
        let mut f = Fake::new();
        f.own_focus_ui_fixture();
        f.start_binding();
        f.focus_geometry_fixture(WIN, BUTTON, true);
        f.torn = Some(at);
        assert_eq!(
            own_focus::command_selected_detail(&f, "GER_rhineland", true)
                .unwrap_err()
                .code,
            "torn_read",
            "{at:x}"
        );
        assert!(f.untouched(VIEW + 0x1138, 16));
    }
    let mut f = Fake::new();
    f.own_focus_ui_fixture();
    f.fail_final = true;
    assert_eq!(
        own_focus::command_selected_detail(&f, "GER_rhineland", false)
            .unwrap_err()
            .code,
        "ownership_changed"
    );
    let f = Fake::new();
    assert_eq!(
        own_focus::command_selected_detail(&f, "", false)
            .unwrap_err()
            .code,
        "invalid_argument"
    );
    assert!(f.reads.borrow().is_empty());
}
