use super::*;
use hoi4_live_native::construction::controls::snapshot as observe_controls;
#[path = "controls/names.rs"]
mod names;
#[path = "controls/optional_gate.rs"]
mod optional_gate;
const REG: u64 = 0x200000;
const MAN: u64 = 0x210000;
const PANEL: u64 = 0x220000;
const TOP: u64 = 0x230000;
const ROOT: u64 = 0x240000;
const MENU: u64 = 0x250000;
const FACTION: u64 = 0x260000;
const INTEL: u64 = 0x270000;
const CW: u64 = 0x280000;
const OPEN: u64 = 0x290000;
const CHOOSE: u64 = 0x2a0000;
const ITEM: u64 = 0x2b0000;
const ITEMS: u64 = 0x2c0000;
const FOCUS: u64 = 0x2d0000;
const POLITICS: u64 = 0x2e0000;
const FROOT: u64 = 0x2f0000;
const PROOT: u64 = 0x300000;
const RESEARCH: u64 = 0x310000;
const TREE: u64 = 0x320000;
const RROOT: u64 = 0x330000;
const TROOT: u64 = 0x340000;
const CONSTRUCTION_ROOT: u64 = 0x350000;
const GRAPHICS: u64 = 0x360000;
fn text(f: &mut Fixture, address: u64, value: &str) {
    if value.len() <= 15 {
        f.string(address, value);
        return;
    }
    f.region(address, 32);
    f.word(address, address + 0x10000000);
    f.word(address + 8, value.len() as u64);
    f.put(address + 0x10000000, value.as_bytes());
}
fn named(f: &mut Fixture, window: u64, children: bool, name: &str, value: u64) {
    let (a, b) = if children {
        (0xbb0, 0xbc8)
    } else {
        (0x9a0, 0x9b8)
    };
    f.array(window + a, window + 0x3000, 1);
    f.word(window + 0x3000, value);
    f.array(window + b, window + 0x4000, 1);
    text(f, window + 0x4000, name);
}
fn window(f: &mut Fixture, p: u64, shown: bool) {
    f.region(p, 0xf00);
    f.word(p, BASE + 0x4347d58);
    f.word(p + 0x98, REG);
    f.put(p + 0xa5, &[if shown { 8 } else { 0 }]);
}
fn control(f: &mut Fixture, button: u64, owner: u64, choice: bool) {
    f.region(button, 0x208);
    f.word(button, BASE + 0x43470c8);
    f.word(button + 0x68, REG);
    f.number(button + 0x74, 0x800);
    f.number(button + 0xac, 2);
    f.word(button + 0x80, BASE + 0x4347488);
    let link = button + 0x3000;
    f.word(button + 0x88, link);
    f.word(button + 0x90, link);
    f.number(button + 0x98, 1);
    let (off, primary, manager, invoker, member) = if choice {
        (0x28, 0x43104c0, 0x297df40, 0x297df10, 0x296f810)
    } else {
        (0x21f0, 0x426c518, 0x1886560, 0x1886530, 0x186d470)
    };
    let glue = owner + off;
    f.region(glue, 0x288);
    f.word(glue, BASE + primary);
    f.word(glue + 0x18, BASE + manager);
    f.word(glue + 0x20, BASE + invoker);
    let capture = button + 0x4000;
    f.region(capture, 24);
    f.word(capture, BASE + member);
    f.word(capture + 0x10, owner);
    f.word(glue + 8, capture);
    f.region(link, 32);
    f.word(link, glue);
    // DC is one byte; never authorize reads of its unproved trailing bytes.
    for a in button + 0xdd..button + 0xe0 {
        f.data.remove(&a);
    }
}
fn controls_fixture() -> Fixture {
    let mut f = fixture();
    f.region(REG, 0x240);
    f.word(REG, BASE + 0x42736f8);
    f.word(BASE + 0x45778e0, REG);
    f.word(0x324f8, REG);
    f.region(MAN, 0x500);
    f.word(0x326b8, MAN);
    f.word(MAN + 0x180, 0x32000);
    f.number(MAN + 0x3d8, 17);
    f.word(MAN + 0x150, PANEL);
    f.word(MAN + 0x188, TOP);
    f.region(PANEL, 0x340);
    f.word(PANEL, BASE + 0x43101d8);
    f.word(PANEL + 0x2f0, 0x32000);
    f.word(PANEL + 0x2f8, CONSTRUCTION_ROOT);
    f.region(TOP, 0x5000);
    f.word(TOP, BASE + 0x426c3d0);
    f.word(TOP + 0x68, 0x32000);
    f.word(TOP + 0x70, MAN);
    f.word(TOP + 0x78, ROOT);
    f.word(TOP + 0x98, MENU);
    f.word(TOP + 0x4db8, OPEN);
    for p in [ROOT, MENU, FACTION, INTEL, CW, CONSTRUCTION_ROOT] {
        window(&mut f, p, true);
    }
    for p in [FROOT, PROOT, RROOT, TROOT] {
        window(&mut f, p, false);
    }
    for (parent, name, child) in [
        (ROOT, "country_menu_window", MENU),
        (MENU, "after_faction_button", FACTION),
        (FACTION, "after_intel_agency_button", INTEL),
    ] {
        named(&mut f, parent, true, name, child);
    }
    named(&mut f, INTEL, false, "construction_button", OPEN);
    f.array(PANEL + 0x320, ITEMS, 1);
    f.word(ITEMS, ITEM);
    f.region(ITEM, 0x300);
    f.word(ITEM, BASE + 0x43100f8);
    f.word(ITEM + 0x20, TEMPLATE);
    f.word(ITEM + 0x10, CW);
    f.number(TEMPLATE + 0x2b8, 1);
    named(&mut f, CW, false, "start_construction_button", CHOOSE);
    control(&mut f, OPEN, TOP, false);
    control(&mut f, CHOOSE, ITEM, true);
    f.word(MAN + 0x128, POLITICS);
    f.word(MAN + 0x130, FOCUS);
    f.region(FOCUS, 0x1200);
    f.word(FOCUS, BASE + 0x4249c70);
    f.word(FOCUS + 0x2f0, 0x32000);
    f.word(FOCUS + 0x2f8, FROOT);
    f.word(FOCUS + 0x11d8, 0x20000);
    f.region(POLITICS, 0x300);
    f.word(POLITICS, BASE + 0x42546e8);
    f.word(POLITICS + 0x2f0, 0x32000);
    f.word(POLITICS + 0x38, MAN);
    f.word(POLITICS + 0x2f8, PROOT);
    f.word(MAN + 0xf0, RESEARCH);
    f.region(RESEARCH, 0x300);
    f.word(RESEARCH, BASE + 0x424dc38);
    f.word(RESEARCH + 0x2f0, 0x32000);
    f.word(RESEARCH + 0x2f8, RROOT);
    f.word(MAN + 0xf8, TREE);
    f.region(TREE, 0xe28);
    f.word(TREE, BASE + 0x424de40);
    f.word(TREE + 0x2f0, 0x32000);
    f.word(TREE + 0x2f8, TROOT);
    f.number(TREE + 0xe20, 1);
    f.number(TREE + 0xe24, 6);
    f.put(0x30a59, &[1]);
    f.put(0x328f0, &[0]);
    f.word(0x327b0, 0);
    f
}
fn observe(f: &Fixture) -> Result<serde_json::Value> {
    observe_controls(f, "infrastructure", false)
}

fn geometry_fixture() -> Fixture {
    let mut f = controls_fixture();
    f.region(GRAPHICS, 0x200);
    f.word(GRAPHICS, BASE + 0x42417f8);
    f.word(BASE + 0x4577890, GRAPHICS);
    f.word(REG + 0xb8, GRAPHICS);
    for (i, n) in [512, 288, 2560, 1440].into_iter().enumerate() {
        f.number(GRAPHICS + 0x178 + i as u64 * 4, n);
    }
    for (i, n) in [5.0f32, 5.0, 0.2, 0.2].into_iter().enumerate() {
        f.put(GRAPHICS + 0x1d0 + i as u64 * 4, &n.to_le_bytes());
    }
    f
}

fn metadata(f: &mut Fixture, object: u64, primary: u64, name: &str) {
    let primary = BASE + primary;
    let info = BASE + 0x435a800;
    let string = BASE + 0x3516000;
    f.word(object, primary);
    f.word(primary - 16, 0);
    f.word(primary - 8, info);
    f.word(info + 8, string);
    f.region(string, 128);
    f.put(string, name.as_bytes());
}

#[test]
fn combined_optional_geometry_uses_one_process_bracket_and_final_witness_set() {
    let f = geometry_fixture();
    let v = observe_controls(&f, "infrastructure", true).unwrap();
    assert_eq!(
        v["input_geometry"]["physical_dimensions"],
        json!([512, 288])
    );
    assert_eq!(
        v["opener"]["render_diagnostic"]["reason"],
        "renderer_unmaterialized"
    );
    assert_eq!(
        v["choice"]["render_diagnostic"]["reason"],
        "renderer_unmaterialized"
    );
    assert_eq!(v["input_geometry"]["input_source"]["materialized"], false);
    assert_eq!(f.checks.get(), 2);
    let a = GRAPHICS + 0x178;
    let mut bytes = (0..16).map(|i| f.data[&(a + i)]).collect::<Vec<_>>();
    bytes[0] ^= 1;
    let mut changed = geometry_fixture();
    changed.fault = Some((a, f.tracked_reads.borrow()[&a] - 1, bytes));
    assert_eq!(
        observe_controls(&changed, "infrastructure", true)
            .unwrap_err()
            .code,
        "torn_read"
    );
}

#[test]
fn unreviewed_control_and_renderer_types_never_reach_typed_payload() {
    let mut f = controls_fixture();
    metadata(&mut f, OPEN, 0x4346220, "7CButton");
    let v = observe(&f).unwrap();
    assert_eq!(v["opener"]["control_type_encoding"], "7CButton");
    assert_eq!(v["opener"]["available"], false);
    production_not_read(&f, &[OPEN + 0x68, OPEN + 0x74, OPEN + 0x80, OPEN + 0x1a8]);
    let mut f = geometry_fixture();
    f.word(OPEN + 0xe8, 0x370000);
    metadata(&mut f, 0x370000, 0x434c5b0, "22CUnsupportedTestSprite");
    let v = observe_controls(&f, "infrastructure", true).unwrap();
    assert_eq!(
        v["opener"]["render_diagnostic"]["reason"],
        "renderer_layout_unavailable"
    );
    production_not_read(&f, &[0x370138, 0x370160]);
    let mut f = geometry_fixture();
    f.word(OPEN + 0xe8, 0x370000);
    metadata(&mut f, 0x370000, 0x4356e08, "7CSprite");
    f.word(0x370138, CHOOSE);
    assert_eq!(
        observe_controls(&f, "infrastructure", true)
            .unwrap_err()
            .code,
        "invalid_construction"
    );
    production_not_read(&f, &[0x370160]);
}

#[test]
fn choice_coverage_duplicates_and_catalog_corruption_are_not_hidden_by_requested_match() {
    let mut f = controls_fixture();
    f.array(PANEL + 0x320, ITEMS, 2);
    f.word(ITEMS + 8, ITEM + 0x1000);
    f.word(ITEM + 0x1000, BASE + 0x43100f9);
    let v = observe(&f).unwrap();
    assert_eq!(v["choice"]["available"], true);
    assert_eq!(v["choice"]["complete_choice_coverage"], false);
    production_not_read(&f, &[ITEM + 0x1020, ITEM + 0x1010]);
    for duplicate_pointer in [false, true] {
        let mut f = controls_fixture();
        f.array(PANEL + 0x320, ITEMS, 2);
        let second = if duplicate_pointer {
            ITEM
        } else {
            ITEM + 0x1000
        };
        f.word(ITEMS + 8, second);
        f.word(second, BASE + 0x43100f8);
        f.word(second + 0x20, TEMPLATE);
        assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
    }
    let mut f = controls_fixture();
    f.number(TEMPLATE + 0x2b8, 2);
    assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
    production_not_read(&f, &[ITEM + 0x10]);
    let mut f = controls_fixture();
    f.number(PANEL + 0x328, 257);
    assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
    production_not_read(&f, &[ITEMS]);
    let mut f = controls_fixture();
    f.number(ROOT + 0xbd0, 257);
    assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
}

#[test]
fn combined_named_bindings_selection_and_complete_scoped_blockers_without_queue_reads() {
    let f = controls_fixture();
    let v = observe(&f).unwrap();
    assert_eq!(v["opener"]["callback"]["verified"], true);
    assert_eq!(v["choice"]["callback"]["verified"], true);
    assert_eq!(v["choice"]["building_id"], "infrastructure");
    assert_eq!(v["managed_mode_raw"], 17);
    assert_eq!(v["scoped_blockers"]["conservative_scoped_clear"], true);
    assert_eq!(v["selection"]["active"], false);
    assert_eq!(v["opener"]["all_named_containers_settled_shown"], true);
    for k in [
        "input_authorized",
        "effective_routing_verified",
        "current_frame_verified",
        "global_modal_clear_verified",
        "observation_paused_game",
        "eligibility_available",
    ] {
        assert_eq!(v[k], false);
    }
    production_not_read(
        &f,
        &[
            0x40070,
            0x80000,
            OPEN + 0xe8,
            CHOOSE + 0xe8,
            BASE + 0x4577890,
            OTHER + 0x248,
        ],
    );
    assert_eq!(f.checks.get(), 2);
}
#[test]
fn exact_nested_path_alias_and_owner_guards_precede_payload() {
    for (a, v, blocked) in [
        (TOP + 0x68, 0xdead0000, TOP + 0x78),
        (TOP + 0x70, 0xdead0000, TOP + 0x78),
        (TOP + 0x98, INTEL, MENU + 0xbb0),
        (MENU + 0x98, 0xdead0000, MENU + 0xbb0),
        (TOP + 0x4db8, CHOOSE, OPEN + 0x68),
        (OPEN + 0x68, 0xdead0000, OPEN + 0x74),
        (ITEM + 0x20, 0xdead0000, ITEM + 0x10),
    ] {
        let mut f = controls_fixture();
        f.word(a, v);
        assert_eq!(
            observe(&f).unwrap_err().code,
            "invalid_construction",
            "{a:x}"
        );
        production_not_read(&f, &[blocked]);
    }
    let mut f = controls_fixture();
    text(&mut f, MENU + 0x4000, "absent");
    let v = observe(&f).unwrap();
    assert_eq!(v["opener"]["missing_name"], "after_faction_button");
    assert_eq!(v["choice"]["available"], true);
    production_not_read(&f, &[FACTION]);
}
#[test]
fn inactive_callback_tail_including_cancellation_manager_refuses_before_capture() {
    for glue in [TOP + 0x21f0, ITEM + 0x28] {
        for offset in [0x28, 0x38, 0x1d8, 0x258, 0x278, 0x280, 0x287] {
            let mut f = controls_fixture();
            f.put(glue + offset, &[1]);
            let v = observe(&f).unwrap();
            let key = if glue == ITEM + 0x28 {
                "choice"
            } else {
                "opener"
            };
            assert_eq!(v[key]["callback"]["reason"], "alternate_callback_bundle");
            assert_eq!(v[key]["available"], false);
            production_not_read(&f, &[glue + 8]);
        }
    }
}
#[test]
fn listeners_require_exact_singleton_full_tuple_and_receiver() {
    for button in [OPEN, CHOOSE] {
        let key = if button == OPEN { "opener" } else { "choice" };
        for (a, v) in [
            (button + 0xa0, 0xdead0000),
            (button + 0x3000, 0xdead0000),
            (button + 0x80, BASE + 0x4347489),
        ] {
            let mut f = controls_fixture();
            f.word(a, v);
            assert_eq!(observe(&f).unwrap()[key]["available"], false);
            production_not_read(&f, &[0xdead0000]);
        }
        let mut f = controls_fixture();
        f.word(button + 0x4000 + 0x10, 0xdead0000);
        assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
        let mut f = controls_fixture();
        f.word(button + 0x3008, 1);
        assert_eq!(observe(&f).unwrap_err().code, "invalid_construction");
        let mut f = controls_fixture();
        f.put(button + 0x3018, &[1]);
        assert_eq!(observe(&f).unwrap()[key]["callback"]["verified"], false);
    }
}
#[test]
fn all_local_dispatch_refusals_are_reported_without_optional_pointee_reads() {
    let mut f = controls_fixture();
    f.word(OPEN + 0x1a8, 0xdead0000);
    f.word(OPEN + 0xd0, 0xbeef0000);
    f.put(OPEN + 0x201, &[1]);
    f.number(OPEN + 0xac, 1);
    f.number(OPEN + 0x74, 0x1800);
    f.put(REG + 0x150, &[1]);
    f.word(REG + 0x158, 0xcafe0000);
    f.word(REG + 0x230, 0xabcd0000);
    let v = observe(&f).unwrap();
    assert_eq!(v["opener"]["local"]["conservative_dispatch_ready"], false);
    assert_eq!(v["local_gui_gates"]["conservative_local_gate_clear"], false);
    assert_eq!(v["choice"]["available"], true);
    production_not_read(
        &f,
        &[0xdead0000, 0xbeef0000, 0xcafe0000, 0xabcd0000, OPEN + 0xdd],
    );
}
#[test]
fn unknown_or_foreign_scoped_blockers_never_mean_clear_or_read_foreign_payload() {
    let mut f = controls_fixture();
    f.word(FOCUS + 0x11d8, 0xdead0000);
    assert_eq!(observe(&f).unwrap()["scoped_blockers"]["available"], false);
    production_not_read(&f, &[FOCUS + 0x1180, 0xdead0000]);
    for a in [FROOT + 0xeb0, PROOT + 0xef8, RROOT + 0xeb0, TROOT + 0xef8] {
        let mut f = controls_fixture();
        f.number(a, 1);
        assert_eq!(
            observe(&f).unwrap()["scoped_blockers"]["conservative_scoped_clear"],
            false
        );
    }
    let mut f = controls_fixture();
    f.number(TREE + 0xe20, 2);
    assert_eq!(
        observe(&f).unwrap()["scoped_blockers"]["conservative_scoped_clear"],
        false
    );
    production_not_read(&f, &[TREE + 0xdc0, TREE + 0x2f8]);
}
#[test]
fn absent_hidden_or_transitioning_construction_panel_does_not_enumerate_choices() {
    for a in [CONSTRUCTION_ROOT + 0xa5, CONSTRUCTION_ROOT + 0xeb0] {
        let mut f = controls_fixture();
        if a == CONSTRUCTION_ROOT + 0xa5 {
            f.put(a, &[0]);
        } else {
            f.number(a, 1);
        }
        let v = observe(&f).unwrap();
        assert_eq!(v["choice"]["available"], false);
        assert_eq!(v["opener"]["available"], true);
        production_not_read(&f, &[PANEL + 0x320, ITEMS, ITEM]);
    }
}
#[test]
fn every_consumed_ui_identity_callback_scalar_and_name_is_finally_witnessed() {
    let baseline = controls_fixture();
    observe(&baseline).unwrap();
    for (a, n) in [
        (BASE + 0x45778e0, 8),
        (REG + 0x150, 1),
        (REG + 0x158, 8),
        (MAN + 0x3d8, 4),
        (MAN + 0x3e1, 1),
        (TOP + 0x4db8, 8),
        (TOP + 0x98, 8),
        (ROOT + 0xbb0, 16),
        (MENU + 0x4000, 32),
        (OPEN + 0x74, 4),
        (OPEN + 0xdc, 1),
        (OPEN + 0x201, 1),
        (OPEN + 0x1a8, 8),
        (TOP + 0x2218, 0x260),
        (OPEN + 0x4000, 8),
        (ITEM + 0x20, 8),
        (TEMPLATE + 0x2b8, 4),
        (FOCUS + 0x11d8, 8),
        (TROOT + 0xa5, 1),
    ] {
        let mut bytes = (0..n).map(|i| baseline.data[&(a + i)]).collect::<Vec<_>>();
        bytes[0] ^= 1;
        let mut f = controls_fixture();
        f.fault = Some((a, baseline.tracked_reads.borrow()[&a] - 1, bytes));
        assert_eq!(observe(&f).unwrap_err().code, "torn_read", "{a:x}");
    }
}
#[test]
fn new_controls_schema_rejects_authority_coordinates_and_invalid_types_before_process() {
    let s = hoi4_live_native::protocol::Server {
        process: None,
        unavailable: None,
    };
    for args in [
        json!({}),
        json!({"building_id":""}),
        json!({"building_id":"dockyard","include_geometry":1}),
        json!({"building_id":"dockyard","x":3}),
        json!({"building_id":"dockyard","lease_token":"secret"}),
    ] {
        let v=s.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_construction_controls","arguments":args}})).unwrap();
        assert!(v.to_string().contains("invalid_arguments"));
    }
    let f = controls_fixture();
    assert_eq!(
        observe_controls(&f, "not a key", false).unwrap_err().code,
        "invalid_arguments"
    );
    assert!(f.tracked_reads.borrow().is_empty());
}
