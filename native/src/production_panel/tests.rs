use super::*;
use crate::memory::ReadMemory;
use std::{cell::RefCell, collections::BTreeMap};
const BASE: u64 = 0x10000;
const GS: u64 = 0x100000;
const IDLER: u64 = 0x200000;
const MANAGER: u64 = 0x300000;
const REG: u64 = 0x400000;
const SETTINGS: u64 = 0x500000;
const MENU: u64 = 0x600000;
const OVERVIEW: u64 = 0x700000;
const ROOT: u64 = 0x800000;
const TOPBAR: u64 = 0x900000;
const BUTTON: u64 = 0xa00000;
const LINK: u64 = 0xb00000;
const CAPTURE: u64 = 0xc00000;
#[test]
fn opening_diagnostics_preserve_each_existing_gate() {
    let ready = json!({"build_id":game::BUILD_ID,"new_click_preconditions_clear":true,
        "opener":{"local_dispatch_ready":true}});
    assert!(require_open_candidate(&ready, true).is_ok());
    for (pointer, replacement, expected) in [
        ("/build_id", json!("unknown"), "build"),
        (
            "/new_click_preconditions_clear",
            json!(false),
            "preconditions",
        ),
        ("/opener/local_dispatch_ready", Value::Null, "opener"),
    ] {
        let mut value = ready.clone();
        *value.pointer_mut(pointer).unwrap() = replacement;
        let error = require_open_candidate(&value, true).unwrap_err();
        assert_eq!(error.code, "production_precondition");
        assert!(error.message.contains(&format!("failed={expected};")));
    }
    assert!(require_open_candidate(&ready, false)
        .unwrap_err()
        .message
        .contains("failed=geometry;"));
    let error = require_open_candidate(&Value::Null, false).unwrap_err();
    assert!(error
        .message
        .contains("failed=build,preconditions,opener,geometry;"));
}

#[test]
fn opening_refusal_distinguishes_known_blockers_from_unknown_without_extra_reads() {
    let value = json!({"mode_raw":23,"root":{"locally_hidden_settled":true},
        "engine_gates":{"known_gates_clear":true,"conservative_engine_clear":false,
            "auxiliary_present":true,"manager_transition_bytes":[0,0]},
        "common_blockers":{"focus_detail_visible_animating_counts":[1,0]},
        "opener":{"local":{"optional_gate":{"clear":false}}}});
    let error = require_open_candidate(&value, false).unwrap_err();
    let details: Value =
        serde_json::from_str(error.message.split_once("; observed=").unwrap().1).unwrap();
    assert_eq!(details["mode"], 23);
    assert_eq!(details["auxiliary_present"], true);
    assert_eq!(details["engine_known"], true);
    assert_eq!(details["engine"], false);
    assert_eq!(details["manager_transition"], json!([0, 0]));
    assert_eq!(details["focus_details"], json!([1, 0]));
    assert_eq!(details["optional_gate"], false);
    assert!(details["callback"].is_null());
}

#[test]
fn opening_refusal_never_copies_unbounded_text_objects_or_addresses() {
    let marker = "sensitive-arbitrary-data".repeat(10000);
    let value = json!({"mode_raw":marker,"root":{"locally_hidden_settled":{"address":marker}},
        "engine_gates":{"manager_transition_bytes":[marker,0],"settings":{"predicate_clear":marker}},
        "opener":{"reason":marker,"local":{"flags74_raw":marker},"render_diagnostic":{"address":marker}},
        "common_blockers":{"focus_detail_visible_animating_counts":[0,0,0]}});
    let error = require_open_candidate(&value, false).unwrap_err();
    assert!(error.message.len() < 2000);
    assert!(!error.message.contains("sensitive-arbitrary-data"));
    assert!(!error.message.contains("address"));
    let details: Value =
        serde_json::from_str(error.message.split_once("; observed=").unwrap().1).unwrap();
    for name in [
        "mode",
        "panel_hidden",
        "settings",
        "opener_flags",
        "manager_transition",
        "focus_details",
    ] {
        assert!(details[name].is_null(), "{name}");
    }
}
#[derive(Default)]
struct Fake {
    bytes: BTreeMap<u64, u8>,
    reads: RefCell<Vec<(u64, usize)>>,
}
impl Fake {
    fn put(&mut self, p: u64, b: &[u8]) {
        for (i, v) in b.iter().enumerate() {
            self.bytes.insert(p + i as u64, *v);
        }
    }
    fn word(&mut self, p: u64, n: u64) {
        self.put(p, &n.to_le_bytes());
    }
    fn number(&mut self, p: u64, n: i32) {
        self.put(p, &n.to_le_bytes());
    }
    fn zero(&mut self, p: u64, n: usize) {
        self.put(p, &vec![0; n]);
    }
    fn window(&mut self, p: u64, shown: bool) {
        self.word(p, BASE + 0x4347d58);
        self.word(p + 0x98, REG);
        self.put(p + 0xa5, &[if shown { 8 } else { 0 }]);
        self.number(p + 0xeb0, 0);
        self.number(p + 0xef8, 0);
    }
    fn engine() -> Self {
        let mut f = Self::default();
        f.put(MANAGER + 0x3e1, &[0, 0]);
        f.put(GS + 0xa59, &[1]);
        f.put(IDLER + 0x8f0, &[0]);
        f.put(IDLER + 0x5b2, &[1]);
        f.word(IDLER + 0x7b0, SETTINGS);
        f.word(SETTINGS, BASE + 0x431a2f0);
        f.word(SETTINGS + 0x30, IDLER);
        f.word(SETTINGS + 0x550, 0);
        f.word(MANAGER + 0x218, MENU);
        f.word(MENU, BASE + 0x425bf80);
        f.word(MENU + 0x19a0, IDLER);
        f.word(MENU + 0x19a8, MANAGER);
        f.word(MENU + 0x19b0, 0);
        f.word(MENU + 0x19c8, OVERVIEW);
        f.word(OVERVIEW, BASE + 0x4300540);
        f.word(OVERVIEW + 0x1078, IDLER);
        f.word(OVERVIEW + 0x1080, 0);
        f.word(MANAGER + 0x248, 0);
        f
    }
    fn callback() -> Self {
        let mut f = Self::default();
        f.word(BUTTON + 0x80, BASE + 0x4347488);
        f.word(BUTTON + 0xa0, 0);
        f.number(BUTTON + 0x98, 1);
        f.word(BUTTON + 0x88, LINK);
        f.word(BUTTON + 0x90, LINK);
        f.zero(LINK, 0x19);
        f.word(LINK, TOPBAR + 0xdb0);
        f.zero(TOPBAR + 0xdb0, 0x288);
        f.word(TOPBAR + 0xdb0, BASE + 0x426c518);
        f.word(TOPBAR + 0xdb0 + 8, CAPTURE);
        f.word(TOPBAR + 0xdb0 + 0x18, BASE + 0x1886560);
        f.word(TOPBAR + 0xdb0 + 0x20, BASE + 0x1886530);
        f.word(CAPTURE, BASE + 0x186d0d0);
        f.word(CAPTURE + 8, 0);
        f.word(CAPTURE + 16, TOPBAR);
        f
    }
    fn named(&mut self, p: u64, children: bool, key: &str, value: u64, storage: u64) {
        let (v, n) = if children {
            (0xbb0, 0xbc8)
        } else {
            (0x9a0, 0x9b8)
        };
        self.word(p + v, storage);
        self.number(p + v + 8, 1);
        self.number(p + v + 12, 1);
        self.word(storage, value);
        let names = storage + 0x100;
        self.word(p + n, names);
        self.number(p + n + 8, 1);
        self.number(p + n + 12, 1);
        self.zero(names, 32);
        self.word(names, storage + 0x200);
        self.word(names + 8, key.len() as u64);
        self.put(storage + 0x200, key.as_bytes());
    }
    fn opener() -> Self {
        let mut f = Self::callback();
        f.word(MANAGER + 0x188, TOPBAR);
        f.word(TOPBAR, BASE + 0x426c3d0);
        f.word(TOPBAR + 0x68, IDLER);
        f.word(TOPBAR + 0x70, MANAGER);
        f.word(TOPBAR + 0x78, ROOT);
        let children = [ROOT + 0x10000, ROOT + 0x20000, ROOT + 0x30000];
        f.word(TOPBAR + 0x98, children[0]);
        let mut p = ROOT;
        for (i, key) in [
            "country_menu_window",
            "after_faction_button",
            "after_intel_agency_button",
        ]
        .iter()
        .enumerate()
        {
            f.window(p, true);
            f.named(p, true, key, children[i], 0xd00000 + i as u64 * 0x1000);
            p = children[i];
        }
        f.window(p, true);
        f.named(p, false, "production_button", BUTTON, 0xd10000);
        f.word(TOPBAR + 0x4dc0, BUTTON);
        f.word(BUTTON, BASE + 0x43470c8);
        f.word(BUTTON + 0x68, REG);
        f.number(BUTTON + 0x74, 0x800);
        f.number(BUTTON + 0xac, 0);
        f.word(BUTTON + 0x1a8, 0);
        f.put(BUTTON + 0x201, &[0]);
        f.word(BUTTON + 0xd0, 0);
        f
    }
}
impl ReadMemory for Fake {
    fn read(&self, p: u64, n: usize) -> Result<Vec<u8>> {
        self.reads.borrow_mut().push((p, n));
        (0..n)
            .map(|i| {
                self.bytes
                    .get(&(p + i as u64))
                    .copied()
                    .ok_or_else(|| Error::new("unmapped", "Fixture unavailable"))
            })
            .collect()
    }
}
#[test]
fn engine_materialized_settings_and_overview_have_typed_owner_gates() {
    let f = Fake::engine();
    let v = engine(
        &Reader::new(&f),
        BASE,
        GS,
        IDLER,
        MANAGER,
        REG,
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(v["conservative_engine_clear"], true);
    assert_eq!(v["settings"]["reason"], "engine_false_by_null");
    assert!(v["settings"].get("locally_hidden_settled").is_none());
}
#[test]
fn gui_update_phase_is_diagnostic_and_never_bypasses_actual_blockers() {
    for phase in [0, 1] {
        for blocker in 0..5 {
            let mut f = Fake::engine();
            f.put(IDLER + 0x5b2, &[phase]);
            match blocker {
                1 => f.put(MANAGER + 0x3e1, &[1]),
                2 => f.put(GS + 0xa59, &[0]),
                3 => f.put(IDLER + 0x8f0, &[1]),
                4 => f.word(MANAGER + 0x248, 0xeeee00),
                _ => (),
            }
            let v = engine(
                &Reader::new(&f),
                BASE,
                GS,
                IDLER,
                MANAGER,
                REG,
                &mut Vec::new(),
            )
            .unwrap();
            assert_eq!(v["idler_can_update_raw"], phase);
            assert_eq!(
                v["idler_can_update_role"],
                "internal_gui_update_phase_observation_only"
            );
            assert_eq!(v["conservative_engine_clear"], blocker == 0);
            assert_eq!(v["complete_engine_eligibility_verified"], false);
        }
    }
}
#[test]
fn engine_foreign_settings_stops_before_root_payload() {
    let mut f = Fake::engine();
    f.word(SETTINGS + 0x30, IDLER + 8);
    assert_eq!(
        engine(
            &Reader::new(&f),
            BASE,
            GS,
            IDLER,
            MANAGER,
            REG,
            &mut Vec::new()
        )
        .unwrap_err()
        .code,
        "invalid_production_panel"
    );
    assert!(!f.reads.borrow().iter().any(|(p, _)| *p == SETTINGS + 0x550));
}
#[test]
fn engine_materialized_visibility_transitions_and_unknown_auxiliary_block() {
    for choice in 0..3 {
        let mut f = Fake::engine();
        if choice == 2 {
            f.word(MANAGER + 0x248, 0xeeee00);
        } else {
            f.word(SETTINGS + 0x550, ROOT);
            f.window(ROOT, choice == 0);
            if choice == 1 {
                f.number(ROOT + 0xeb0, 1);
            }
        }
        let v = engine(
            &Reader::new(&f),
            BASE,
            GS,
            IDLER,
            MANAGER,
            REG,
            &mut Vec::new(),
        )
        .unwrap();
        assert_eq!(v["conservative_engine_clear"], false);
        assert!(!f.reads.borrow().iter().any(|(p, _)| *p >= 0xeeee00));
    }
}
#[test]
fn first_void_tuple_is_distinct_from_equipment_and_construction_members() {
    for member in [0x186d0d0, 0x186d470, 0x17e0c60] {
        let mut f = Fake::callback();
        f.word(CAPTURE, BASE + member);
        let v = callback(&Reader::new(&f), BASE, TOPBAR, BUTTON, &mut Vec::new()).unwrap();
        assert_eq!(v["available"], member == 0x186d0d0);
    }
}
#[test]
fn every_unused_callback_bundle_including_kind9_refuses() {
    for off in [0x28, 0x38, 0x78, 0x98, 0x1d8, 0x278] {
        let mut f = Fake::callback();
        f.word(TOPBAR + 0xdb0 + off, 1);
        assert_eq!(
            callback(&Reader::new(&f), BASE, TOPBAR, BUTTON, &mut Vec::new()).unwrap()["available"],
            false
        );
    }
}
#[test]
fn exact_narrow_path_and_alias_prove_only_named_opener() {
    let f = Fake::opener();
    let v = opener(
        &Reader::new(&f),
        BASE,
        IDLER,
        MANAGER,
        REG,
        false,
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(v["local_dispatch_ready"], true);
    assert_eq!(v["input_authorized"], false);
    assert!(!f.reads.borrow().iter().any(|(p, _)| *p == BUTTON + 0xe8));
}
#[test]
fn alias_and_registry_mismatches_refuse() {
    for field in [TOPBAR + 0x4dc0, BUTTON + 0x68] {
        let mut f = Fake::opener();
        f.word(field, 0xeeee00);
        assert_eq!(
            opener(
                &Reader::new(&f),
                BASE,
                IDLER,
                MANAGER,
                REG,
                true,
                &mut Vec::new(),
                &mut Vec::new()
            )
            .unwrap_err()
            .code,
            "invalid_production_panel"
        );
    }
}
#[test]
fn unknown_optional_gate_never_dereferences_pointee_or_renderer() {
    let mut f = Fake::opener();
    f.word(BUTTON + 0x1a8, 0xeeee00);
    f.word(ROOT + 0x30000 + 0xe78, 0xf00000);
    let v = opener(
        &Reader::new(&f),
        BASE,
        IDLER,
        MANAGER,
        REG,
        true,
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(v["available"], true);
    assert_eq!(v["local_dispatch_ready"], false);
    assert!(!f
        .reads
        .borrow()
        .iter()
        .any(|(p, _)| *p >= 0xeeee00 || *p == BUTTON + 0xe8));
}
#[test]
fn missing_path_is_unavailable_without_borrowing_alias() {
    let mut f = Fake::opener();
    f.put(0xd00000 + 0x200, b"wrong");
    let v = opener(
        &Reader::new(&f),
        BASE,
        IDLER,
        MANAGER,
        REG,
        true,
        &mut Vec::new(),
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(v["available"], false);
    assert!(!f.reads.borrow().iter().any(|(p, _)| *p == TOPBAR + 0x4dc0));
}

#[test]
fn reviewed_named_parent_position_gate_is_supported_and_pinned() {
    let mut f = Fake::opener();
    let gate = 0xe00000;
    f.word(BUTTON + 0x1a8, gate);
    f.word(ROOT + 0x30000 + 0xe78, gate);
    f.word(gate, BASE + 0x43484f0);
    f.put(gate + 0x128, &[1]);
    let mut scope = Vec::new();
    let v = opener(
        &Reader::new(&f),
        BASE,
        IDLER,
        MANAGER,
        REG,
        false,
        &mut scope,
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(v["local_dispatch_ready"], true);
    assert!(scope.contains(&gate));
    f.put(gate + 0x128, &[0]);
    assert_eq!(
        opener(
            &Reader::new(&f),
            BASE,
            IDLER,
            MANAGER,
            REG,
            false,
            &mut Vec::new(),
            &mut Vec::new()
        )
        .unwrap()["local_dispatch_ready"],
        false
    );
    f.put(gate + 0x128, &[2]);
    assert!(opener(
        &Reader::new(&f),
        BASE,
        IDLER,
        MANAGER,
        REG,
        false,
        &mut Vec::new(),
        &mut Vec::new()
    )
    .is_err());
}
