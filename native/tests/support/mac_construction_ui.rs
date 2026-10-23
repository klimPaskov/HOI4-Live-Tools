use super::*;
use hoi4_live_native::{construction::mac_ui::snapshot as observe, protocol::Server};
use serde_json::json;

const PRODUCTION: u64 = 0x300000;
const BUILDING_DB: u64 = 0x310000;
const TARGET_TEMPLATE: u64 = 0x320000;
const STATE: u64 = 0x330000;
const REGISTRY: u64 = 0x400000;
const MANAGER: u64 = 0x410000;
const VIEW: u64 = 0x420000;
const ROOT: u64 = 0x430000;
const POSSIBLE: u64 = 0x440000;
const ITEMS: u64 = 0x600000;
const FRAME: u64 = 0xc00000;
const BUTTON: u64 = 0xc10000;
const GRID: u64 = 0x4a0000;

fn text(f: &mut MacFixture, address: u64, value: &str) {
    if value.len() <= 22 {
        f.string(address, value);
        return;
    }
    f.word(address, (value.len() as u64 + 1) | 1);
    f.word(address + 8, value.len() as u64);
    f.word(address + 16, address + 0x2000);
    f.put(address + 0x2000, value.as_bytes());
}
fn named(f: &mut MacFixture, widget: u64, name: &str) {
    let kind = widget + 0x1000000;
    f.word(widget + 0x50, kind);
    f.word(widget + 0x58, REGISTRY);
    f.word(kind, BASE + 0x34b0e18);
    text(f, kind + 0x28, name);
}
fn window(f: &mut MacFixture, object: u64, name: &str, children: &[u64]) {
    f.word(object, BASE + 0x34ac0b0);
    // Generated non-primary GUI base; container() joins back to the exact
    // primary at -0x30, rather than requiring this header to be a primary.
    f.word(object + 0x30, 0);
    f.put(object + 0x95, &[8]);
    named(f, object + 0x30, name);
    f.array(object + 0xd30, object + 0x4000, children);
}
fn fixture(visible: bool, count: usize) -> MacFixture {
    let mut f = construction_target::construction();
    f.array(PRODUCTION + 0x70, 0, &[]);
    f.word(IDLER + 0x4c0, REGISTRY);
    f.word(REGISTRY, BASE + 0x34974d0);
    f.word(IDLER + 0x670, MANAGER);
    f.word(MANAGER + 0x180, IDLER);
    f.word(MANAGER + 0x150, VIEW);
    f.int(MANAGER + 0x3c0, 0);
    // Inactive selection pointer and hidden-panel children are deliberately
    // unmapped: even reading them would be an unexpected fixture access.
    f.word(VIEW, BASE + 0x343aa90);
    f.word(VIEW + 0x428, IDLER);
    f.word(VIEW + 0x30, MANAGER);
    f.word(VIEW + 0x430, ROOT);
    f.word(ROOT, BASE + 0x34ac0b0);
    f.put(ROOT + 0x95, &[if visible { 8 } else { 0 }]);
    f.word(ROOT + 0x88, REGISTRY);
    f.int(ROOT + 0x12b8, 0);
    f.int(ROOT + 0x1270, 0);
    if !visible {
        return f;
    }
    assert!(count > 0);
    let names = [
        ("possible_state_constructions", "non_shared_item_grid"),
        ("possible_shared_constructions", "shared_item_grid"),
        ("possible_province_constructions", "province_item_grid"),
        ("possible_facility_constructions", "facility_item_grid"),
    ];
    let sections: Vec<_> = (0..4).map(|i| 0x450000 + i * 0x10000 + 0x30).collect();
    window(&mut f, ROOT, "country_constructions", &[POSSIBLE + 0x30]);
    window(&mut f, POSSIBLE, "possible_constructions", &sections);
    for (i, (section_name, grid_name)) in names.into_iter().enumerate() {
        let section = 0x450000 + i as u64 * 0x10000;
        let grid = GRID + i as u64 * 0x10000;
        window(&mut f, section, section_name, &[grid]);
        f.word(grid, BASE + 0x34be068);
        named(&mut f, grid, grid_name);
        let items: Vec<_> = if i == 0 {
            (0..count).map(|i| ITEMS + i as u64 * 0x400).collect()
        } else {
            vec![]
        };
        f.array(grid + 0x190, grid + 0x4000, &items);
    }
    let mut templates = vec![0x31f000];
    for i in 0..count {
        let target = i == count - 1;
        let template = if target {
            TARGET_TEMPLATE
        } else {
            0xa00000 + i as u64 * 0x400
        };
        let key = if target {
            "arms_factory".to_string()
        } else {
            format!("building_{i}")
        };
        templates.push(template);
        f.word(template, BASE + 0x332e908);
        f.put(template + 0x18, &[1]);
        f.int(template + 0x290, i as i32 + 1);
        f.string(template + 0x228, &key);
        let item = ITEMS + i as u64 * 0x400;
        f.word(item, BASE + 0x343a9b0);
        f.word(item + 0x20, template);
        if target {
            f.word(item + 0x10, FRAME);
        }
    }
    f.array(BUILDING_DB + 0x40, 0xb00000, &templates);
    window(&mut f, FRAME, "construction_choice", &[BUTTON]);
    f.word(BUTTON, BASE + 0x34aafa0);
    named(&mut f, BUTTON, "start_construction_button");
    f.put(BUTTON + 0x65, &[8]);
    f.int(BUTTON + 0x9c, 0);
    f
}
fn not_read(f: &MacFixture, fields: &[u64]) {
    assert!(
        !f.reads.borrow().iter().any(|(address, size)| fields
            .iter()
            .any(|field| (*address..*address + *size as u64).contains(field))),
        "unexpected reads of {fields:x?}"
    );
}

#[test]
fn hidden_panel_observes_owned_scope_and_queue_without_choices_or_selection_payload() {
    let f = fixture(false, 0);
    let v = observe(&f, "arms_factory", 1).unwrap();
    assert_eq!(v["selection"]["kind"], "inactive");
    assert_eq!(v["panel"]["locally_visible"], false);
    assert_eq!(v["choices"]["complete_membership"], false);
    assert!(v["choices"]["target"].is_null());
    assert_eq!(v["queued"], json!({"in_state":0,"elsewhere":0}));
    for field in [
        "input_authorized",
        "geometry_available",
        "settled_frame_verified",
        "callback_binding_verified",
        "placement_eligibility_verified",
        "observation_paused_game",
    ] {
        assert_eq!(v[field], false, "{field}");
    }
    not_read(
        &f,
        &[
            MANAGER + 0x3b8,
            ROOT + 0xd30,
            BASE + 0x44085f8,
            IDLER + 0x6b8,
        ],
    );
    assert!(!v.to_string().contains("start_ticks"));
    assert!(!v.to_string().contains("/generated"));
}

#[test]
fn full_choice_membership_reaches_target_beyond_sixty_four_and_reads_only_its_button() {
    let f = fixture(true, 70);
    let v = observe(&f, "arms_factory", 1).unwrap();
    assert_eq!(v["choices"]["complete_membership"], true);
    assert_eq!(v["choices"]["item_count"], 70);
    assert_eq!(v["choices"]["grid_count"], 4);
    assert_eq!(v["grid_registry_backlink_verified"], true);
    assert_eq!(v["choices"]["target"]["building_id"], "arms_factory");
    assert_eq!(v["choices"]["target"]["disabled"], false);
    for i in 0..69 {
        not_read(&f, &[ITEMS + i * 0x400 + 0x10]);
    }
    not_read(&f, &[BUTTON + 0xd8]); // This slice never reads renderer geometry.
}

#[test]
fn duplicate_or_unknown_tail_items_refuse_before_requested_control_payload() {
    for (field, value) in [
        (ITEMS + 69 * 0x400, BASE + 0x343a9b8),
        (ITEMS + 69 * 0x400 + 0x20, 0xa00000),
        (ITEMS + 69 * 0x400 + 0x20, FOREIGN),
        (GRID + 0x4000 + 69 * 8, ITEMS),
    ] {
        let mut f = fixture(true, 70);
        f.word(field, value);
        assert_eq!(
            observe(&f, "arms_factory", 1).unwrap_err().code,
            "invalid_construction"
        );
        not_read(&f, &[FRAME, BUTTON, FOREIGN]);
    }
}

#[test]
fn foreign_state_or_view_backlinks_refuse_before_unrelated_payload() {
    let mut f = fixture(true, 1);
    f.int(STATE + 0xc4, 2);
    assert_eq!(
        observe(&f, "arms_factory", 1).unwrap_err().code,
        "construction_target_unavailable"
    );
    not_read(
        &f,
        &[
            STATE + 0x40,
            BUILDING_DB,
            REGISTRY,
            MANAGER,
            VIEW,
            ROOT,
            FRAME,
        ],
    );
    for (field, value) in [
        (MANAGER + 0x180, FOREIGN),
        (VIEW + 0x428, FOREIGN),
        (VIEW + 0x30, FOREIGN),
    ] {
        let mut f = fixture(true, 1);
        f.word(field, value);
        assert_eq!(
            observe(&f, "arms_factory", 1).unwrap_err().code,
            "invalid_construction"
        );
        not_read(&f, &[ROOT, FRAME, FOREIGN]);
    }
}

#[test]
fn ordinary_selection_joins_catalog_and_rejects_special_or_absent_pointer() {
    let mut f = fixture(false, 0);
    f.int(MANAGER + 0x3c0, 1);
    f.word(MANAGER + 0x3b8, TARGET_TEMPLATE);
    f.word(BUILDING_DB + 0x3a8, 0x31f000);
    let v = observe(&f, "arms_factory", 1).unwrap();
    assert_eq!(v["selection"]["building_id"], "arms_factory");
    for selected in [0x31f000, FOREIGN] {
        f.word(MANAGER + 0x3b8, selected);
        assert_eq!(
            observe(&f, "arms_factory", 1).unwrap_err().code,
            "invalid_construction"
        );
        not_read(&f, &[FOREIGN]);
    }
}

#[test]
fn unverified_selection_modes_never_read_the_selected_pointer_or_cache() {
    for mode in [2, 3, 4, -1, i32::MAX] {
        let mut f = fixture(false, 0);
        f.int(MANAGER + 0x3c0, mode);
        let v = observe(&f, "arms_factory", 1).unwrap();
        assert_eq!(v["selection"]["supported"], false);
        assert_eq!(v["selection"]["mode_raw"], mode);
        assert!(v["selection"]["building_id"].is_null());
        not_read(&f, &[MANAGER + 0x3b8, BUILDING_DB + 0x3a8]);
    }
}

#[test]
fn unknown_null_and_base_grid_primaries_refuse_before_list_payload() {
    for primary in [0x34b61f8, 0x34b0b70, 0x34be390, 0x34be070] {
        let mut f = fixture(true, 1);
        f.word(GRID, BASE + primary);
        let e = observe(&f, "arms_factory", 1).unwrap_err();
        assert_eq!(e.code, "invalid_construction");
        assert!(e.message.contains("expected rva_34be068"));
        not_read(&f, &[GRID + 0x190, ITEMS, FRAME]);
    }
}

#[test]
fn duplicate_named_sections_or_controls_do_not_select_the_first_match() {
    let mut f = fixture(true, 1);
    named(&mut f, 0x460030, "possible_state_constructions");
    assert_eq!(
        observe(&f, "arms_factory", 1).unwrap_err().code,
        "invalid_construction"
    );
    not_read(&f, &[GRID, FRAME]);
    let mut f = fixture(true, 1);
    named(&mut f, BUTTON + 0x1000, "start_construction_button");
    f.array(FRAME + 0xd30, FRAME + 0x4000, &[BUTTON, BUTTON + 0x1000]);
    assert_eq!(
        observe(&f, "arms_factory", 1).unwrap_err().code,
        "invalid_construction"
    );
    not_read(&f, &[BUTTON + 0x9c, BUTTON + 0xd8]);
}

#[test]
fn changed_ownership_catalog_membership_visibility_or_button_fails_final_witnesses() {
    for field in [
        MANAGER + 0x180,
        VIEW + 0x30,
        VIEW + 0x428,
        VIEW + 0x430,
        ROOT + 0x95,
        ROOT + 0x88,
        ROOT + 0x12b8,
        ROOT + 0x1270,
        POSSIBLE + 0x88,
        GRID + 0x58,
        BUILDING_DB + 0x40,
        TARGET_TEMPLATE + 0x228,
        GRID + 0x4000,
        ITEMS + 0x20,
        BUTTON + 0x65,
        BUTTON + 0x58,
        BUTTON + 0x9c,
    ] {
        let f = fixture(true, 1);
        f.final_fault.set(Some(field));
        assert_eq!(
            observe(&f, "arms_factory", 1).unwrap_err().code,
            "torn_read",
            "{field:x}"
        );
    }
}

#[test]
fn every_requested_gui_owner_must_match_before_dependent_payload() {
    for (field, unread) in [
        (ROOT + 0x88, ROOT + 0x95),
        (POSSIBLE + 0x88, POSSIBLE + 0xd30),
        (0x450000 + 0x88, 0x450000 + 0xd30),
        (GRID + 0x58, GRID + 0x190),
        (GRID + 3 * 0x10000 + 0x58, ITEMS + 0x10),
        (FRAME + 0x88, FRAME + 0x95),
        (BUTTON + 0x58, BUTTON + 0x65),
    ] {
        let mut f = fixture(true, 1);
        f.word(field, FOREIGN);
        let error = observe(&f, "arms_factory", 1).unwrap_err();
        assert_eq!(error.code, "invalid_construction", "{field:x}");
        assert!(error.message.contains("own GUI registry"));
        not_read(&f, &[unread, FOREIGN]);
    }
}

#[test]
fn root_transition_two_skips_choice_payload_even_when_local_flag_is_shown() {
    for field in [ROOT + 0x12b8, ROOT + 0x1270] {
        let mut f = fixture(false, 0);
        f.put(ROOT + 0x95, &[8]);
        f.int(field, 2);
        let v = observe(&f, "arms_factory", 1).unwrap();
        assert_eq!(v["panel"]["locally_visible"], true);
        assert_eq!(v["panel"]["engine_visible"], false);
        assert_eq!(v["panel"]["local_transitions_idle"], false);
        assert_eq!(v["choices"]["reason"], "panel_transition_hiding");
        assert_eq!(v["grid_registry_backlink_verified"], false);
        not_read(&f, &[ROOT + 0xd30]);
    }
}

#[test]
fn root_transition_one_is_engine_visible_but_never_settled_or_input_ready() {
    for field in [ROOT + 0x12b8, ROOT + 0x1270] {
        let mut f = fixture(true, 1);
        f.int(field, 1);
        let v = observe(&f, "arms_factory", 1).unwrap();
        assert_eq!(v["panel"]["engine_visible"], true);
        assert_eq!(v["panel"]["local_transitions_idle"], false);
        assert_eq!(v["choices"]["complete_membership"], true);
        assert_eq!(v["settled_frame_verified"], false);
        assert_eq!(v["input_authorized"], false);
    }
}

#[test]
fn unknown_root_transition_refuses_before_choice_payload() {
    for field in [ROOT + 0x12b8, ROOT + 0x1270] {
        for state in [-1, 3, i32::MAX] {
            let mut f = fixture(false, 0);
            f.int(field, state);
            let error = observe(&f, "arms_factory", 1).unwrap_err();
            assert_eq!(error.code, "invalid_construction");
            assert!(error.message.contains("transition state"));
            not_read(&f, &[ROOT + 0xd30]);
        }
    }
}

#[test]
fn unmaterialized_panel_and_root_preserve_explicit_unavailability() {
    for field in [MANAGER + 0x150, VIEW + 0x430] {
        let mut f = fixture(false, 0);
        f.word(field, 0);
        let v = observe(&f, "arms_factory", 1).unwrap();
        assert_eq!(v["panel"]["root_materialized"], false);
        assert_eq!(v["choices"]["complete_membership"], false);
        not_read(&f, &[ROOT]);
    }
}

#[test]
fn hidden_requested_choice_and_disabled_button_remain_non_authorizing() {
    let mut f = fixture(true, 1);
    f.put(FRAME + 0x95, &[0]);
    let v = observe(&f, "arms_factory", 1).unwrap();
    assert_eq!(v["choices"]["reason"], "choice_window_hidden");
    assert!(v["choices"]["target"].is_null());
    not_read(&f, &[BUTTON]);
    let mut f = fixture(true, 1);
    f.int(BUTTON + 0x9c, 3);
    let v = observe(&f, "arms_factory", 1).unwrap();
    assert_eq!(v["choices"]["target"]["disabled"], true);
    assert_eq!(v["input_authorized"], false);
}

#[test]
fn probe_protocol_rejects_extra_fields_and_invalid_target_before_binding() {
    let server = Server {
        process: None,
        unavailable: None,
    };
    for args in [
        json!({}),
        json!({"building_id":"arms_factory","state_id":0}),
        json!({"building_id":"bad key","state_id":1}),
        json!({"building_id":"arms_factory","state_id":1,"click":true}),
        json!({"building_id":"arms_factory","state_id":1,"include_geometry":true}),
    ] {
        let reply=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_construction_ui_probe","arguments":args}})).unwrap();
        assert!(reply.to_string().contains("invalid_arguments"), "{reply}");
    }
}

#[test]
fn missing_later_grid_keeps_partial_membership_and_never_reads_an_early_target_control() {
    let mut f = fixture(true, 1);
    // First grid already contains the target; the fourth section has no grid.
    f.array(0x480000 + 0xd30, 0, &[]);
    let v = observe(&f, "arms_factory", 1).unwrap();
    assert_eq!(v["choices"]["complete_membership"], false);
    assert_eq!(v["choices"]["grid_count"], 3);
    assert_eq!(v["choices"]["item_count"], 1);
    assert!(v["choices"]["target"].is_null());
    not_read(&f, &[ITEMS + 0x10, FRAME, BUTTON]);
}

#[test]
fn unknown_tail_after_an_early_target_still_refuses_before_its_control() {
    let mut f = fixture(true, 70);
    f.string(0xa00000 + 0x228, "arms_factory");
    f.string(TARGET_TEMPLATE + 0x228, "building_69");
    f.word(ITEMS + 69 * 0x400, BASE + 0x343a9b8);
    assert_eq!(
        observe(&f, "arms_factory", 1).unwrap_err().code,
        "invalid_construction"
    );
    not_read(&f, &[ITEMS + 0x10, FRAME, BUTTON]);
}

#[test]
fn process_replacement_cannot_yield_a_completed_observation() {
    struct Replaced {
        fixture: MacFixture,
        replacement: Identity,
        calls: Cell<usize>,
    }
    impl ReadMemory for Replaced {
        fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
            self.fixture.read(address, size)
        }
        fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
            self.fixture.read_many(requests)
        }
        fn string_abi(&self) -> StringAbi {
            StringAbi::LibCxx
        }
    }
    impl Process for Replaced {
        fn begin_snapshot(&self) -> Result<()> {
            self.fixture.begin_snapshot()
        }
        fn check(&self) -> Result<()> {
            Ok(())
        }
        fn image_base(&self) -> u64 {
            BASE
        }
        fn identity(&self) -> Option<&Identity> {
            let first = self.calls.get() == 0;
            self.calls.set(self.calls.get() + 1);
            Some(if first {
                &self.fixture.id
            } else {
                &self.replacement
            })
        }
    }
    let f = fixture(false, 0);
    let mut replacement = f.id.clone();
    replacement.start_ticks.push('1');
    let replaced = Replaced {
        fixture: f,
        replacement,
        calls: Cell::new(0),
    };
    assert_eq!(
        observe(&replaced, "arms_factory", 1).unwrap_err().code,
        "ownership_changed"
    );
}
