use super::*;
use hoi4_live_native::{construction::mac_panel::snapshot, protocol::Server};
use serde_json::json;

const GUI: u64 = 0x400000;
const MANAGER: u64 = 0x410000;
const VIEW: u64 = 0x420000;
const ROOT: u64 = 0x430000;
const TOPBAR: u64 = 0x440000;
const TOPROOT: u64 = 0x450000;
const MENU: u64 = 0x460000;
const FACTION: u64 = 0x470000;
const AGENCY: u64 = 0x480000;
const BUTTON: u64 = 0x490000;
const NODE: u64 = 0x4a0000;
const FUNCTOR: u64 = 0x4b0000;
const SPRITE: u64 = 0x4c0000;
const TYPE: u64 = 0x4d0000;
const GRAPHICS: u64 = 0x4e0000;

fn name(f: &mut MacFixture, widget: u64, text: &str) {
    let kind = widget + 0x1000000;
    f.word(widget + 0x50, kind);
    f.word(kind, BASE + 0x34b0e18);
    if text.len() <= 22 {
        f.string(kind + 0x28, text);
    } else {
        f.word(kind + 0x28, (text.len() as u64 + 1) | 1);
        f.word(kind + 0x30, text.len() as u64);
        f.word(kind + 0x38, kind + 0x100);
        f.put(kind + 0x100, text.as_bytes());
    }
}
fn local(f: &mut MacFixture, object: u64, shown: bool) {
    f.word(object, BASE + 0x34ac0b0);
    f.word(object + 0x30, 0);
    f.word(object + 0x88, GUI);
    f.put(object + 0x95, &[if shown { 8 } else { 0 }]);
    f.int(object + 0x12b8, 0);
    f.int(object + 0x1270, 0);
}
fn fixture(mode: i32) -> MacFixture {
    let mut f = MacFixture::new();
    for (field, value) in [
        (IDLER + 0x4c0, GUI),
        (GUI, BASE + 0x34974d0),
        (IDLER + 0x670, MANAGER),
        (MANAGER + 0x180, IDLER),
        (MANAGER + 0x150, VIEW),
        (VIEW, BASE + 0x343aa90),
        (VIEW + 0x428, IDLER),
        (VIEW + 0x30, MANAGER),
        (VIEW + 0x430, ROOT),
    ] {
        f.word(field, value);
    }
    f.int(MANAGER + 0x3a8, mode);
    f.int(MANAGER + 0x3c0, 0);
    local(&mut f, ROOT, mode == 0x11);
    if mode == 0x11 {
        return f;
    } // Opener intentionally unmapped for no-toggle path.
    for (field, value) in [
        (MANAGER + 0x188, TOPBAR),
        (TOPBAR, BASE + 0x3489640),
        (TOPBAR + 0x58, IDLER),
        (TOPBAR + 0x60, MANAGER),
        (TOPBAR + 0x30, MANAGER),
        (TOPBAR + 0x68, TOPROOT),
        (TOPBAR + 0x88, MENU + 0x30),
        (TOPBAR + 0x7428, BUTTON),
    ] {
        f.word(field, value);
    }
    f.string(TOPBAR + 0x40, "topbar");
    for (object, child, key) in [
        (TOPROOT, MENU + 0x30, "country_menu_window"),
        (MENU, FACTION + 0x30, "after_faction_button"),
        (FACTION, AGENCY + 0x30, "after_intel_agency_button"),
        (AGENCY, BUTTON, "construction_button"),
    ] {
        local(&mut f, object, true);
        f.array(object + 0xd30, object + 0x4000, &[child]);
        name(&mut f, child, key);
    }
    for (field, value) in [
        (BUTTON, BASE + 0x34aafa0),
        (BUTTON + 0x58, GUI),
        (BUTTON + 0x70, BASE + 0x34ab360),
        (BUTTON + 0x78, NODE),
        (BUTTON + 0x80, NODE),
        (BUTTON + 0x90, 0),
        (BUTTON + 0xc0, 0),
        (NODE, TOPBAR + 0x3290),
        (NODE + 8, 0),
        (NODE + 0x10, 0),
        (TOPBAR + 0x3290, BASE + 0x3489788),
        (TOPBAR + 0x3290 + 0x30, FUNCTOR),
        (FUNCTOR, BASE + 0x3489818),
        (FUNCTOR + 8, BASE + 0x22f27d0),
        (FUNCTOR + 0x10, 0),
        (FUNCTOR + 0x18, TOPBAR),
    ] {
        f.word(field, value);
    }
    f.int(BUTTON + 0x88, 1);
    f.put(BUTTON + 0x8c, &[0]);
    for offset in [0x60, 0x150, 0x180, 0x1b0, 0x1e0, 0x2d0, 0x3c0] {
        f.word(TOPBAR + 0x3290 + offset, 0);
    }
    f.put(NODE + 0x18, &[0]);
    f.put(BUTTON + 0x65, &[8]);
    f.int(BUTTON + 0x9c, 0);
    f
}
fn geometry(f: &mut MacFixture) {
    f.word(GUI + 0x98, GRAPHICS);
    f.word(GRAPHICS, BASE + 0x33dafa8);
    for (offset, dimension) in [(0x170, 1920), (0x174, 1080), (0x178, 1920), (0x17c, 1080)] {
        f.int(GRAPHICS + offset, dimension);
    }
    f.put(GRAPHICS + 0x2c5b0, &1_f32.to_le_bytes());
    f.word(BUTTON + 0xd8, SPRITE);
    f.word(SPRITE, BASE + 0x34bdb10);
    f.word(SPRITE + 8, TYPE);
    f.int(TYPE + 0x78, 1);
    f.int(TYPE + 0x8c, 34);
    f.int(TYPE + 0x90, 32);
    f.put(SPRITE + 0xb0, &(-740_f32).to_le_bytes());
    f.put(SPRITE + 0xb4, &498_f32.to_le_bytes());
}
fn not_read(f: &MacFixture, fields: &[u64]) {
    assert!(
        !f.reads
            .borrow()
            .iter()
            .any(|(a, n)| fields.iter().any(|x| (*a..*a + *n as u64).contains(x))),
        "unexpected reads: {fields:x?}"
    );
}

#[test]
fn closed_panel_binds_exact_own_opener_without_default_geometry() {
    let f = fixture(2);
    let v = snapshot(&f, false).unwrap();
    assert_eq!(v["managed_mode_raw"], 2);
    assert_eq!(v["opener"]["callback_binding_verified"], true);
    assert_eq!(v["opener"]["observer_count"], 1);
    assert_eq!(v["input_authorized"], false);
    assert_eq!(v["global_modal_clear_verified"], false);
    assert!(v["opener"]["rect"].is_null());
    not_read(&f, &[BUTTON + 0xd8, GUI + 0x98]);
}
#[test]
fn explicitly_requested_rect_comes_from_the_bound_button_renderer() {
    let mut f = fixture(0x17);
    geometry(&mut f);
    let v = snapshot(&f, true).unwrap();
    assert_eq!(v["opener"]["rect"], json!({"x":220,"y":10,"w":34,"h":32}));
    assert_eq!(v["settled_frame_verified"], false);
    assert_eq!(v["input_authorized"], false);
}
#[test]
fn unsupported_canvas_refuses_before_renderer_payload() {
    for (field, dimension) in [
        (GRAPHICS + 0x170, 2560),
        (GRAPHICS + 0x174, 1440),
        (GRAPHICS + 0x178, 960),
        (GRAPHICS + 0x17c, 540),
    ] {
        let mut f = fixture(2);
        geometry(&mut f);
        f.int(field, dimension);
        assert_eq!(
            snapshot(&f, true).unwrap_err().code,
            "gui_geometry_unavailable"
        );
        not_read(&f, &[BUTTON + 0xd8, SPRITE]);
        assert!(snapshot(&f, false).is_ok());
    }
    let mut f = fixture(2);
    geometry(&mut f);
    f.put(GRAPHICS + 0x2c5b0, &1.5_f32.to_le_bytes());
    assert_eq!(
        snapshot(&f, true).unwrap_err().code,
        "gui_geometry_unavailable"
    );
    not_read(&f, &[BUTTON + 0xd8, SPRITE]);
}
#[test]
fn foreign_or_malformed_graphics_cannot_supply_opener_geometry() {
    for field in [GUI + 0x98, GRAPHICS] {
        let mut f = fixture(2);
        geometry(&mut f);
        f.word(FOREIGN, 0);
        f.word(field, FOREIGN);
        assert_eq!(snapshot(&f, true).unwrap_err().code, "invalid_gui_geometry");
        not_read(&f, &[BUTTON + 0xd8, SPRITE]);
    }
    for dimension in [0, -1, 16385] {
        let mut f = fixture(2);
        geometry(&mut f);
        f.int(GRAPHICS + 0x170, dimension);
        assert_eq!(snapshot(&f, true).unwrap_err().code, "invalid_gui_geometry");
        not_read(&f, &[BUTTON + 0xd8]);
    }
    for scale in [f32::NAN, f32::INFINITY, 0.0, 8.5] {
        let mut f = fixture(2);
        geometry(&mut f);
        f.put(GRAPHICS + 0x2c5b0, &scale.to_le_bytes());
        assert_eq!(snapshot(&f, true).unwrap_err().code, "invalid_gui_geometry");
        not_read(&f, &[BUTTON + 0xd8]);
    }
}
#[test]
fn already_selected_mode_never_reads_toggle_control_even_with_pending_visibility() {
    for (shown, state) in [(0, 0), (8, 1), (8, 2), (8, 0)] {
        let mut f = fixture(0x11);
        f.put(ROOT + 0x95, &[shown]);
        f.int(ROOT + 0x12b8, state);
        let v = snapshot(&f, true).unwrap();
        assert_eq!(v["reason"], "already_selected_no_toggle");
        assert!(v["opener"].is_null());
        not_read(&f, &[MANAGER + 0x188, TOPBAR, BUTTON]);
    }
}
#[test]
fn active_or_unknown_placement_and_inconsistent_panels_never_reach_topbar() {
    for (field, value) in [
        (MANAGER + 0x3c0, 1),
        (MANAGER + 0x3c0, 3),
        (MANAGER + 0x3a8, -1),
        (MANAGER + 0x3a8, 24),
        (ROOT + 0x1270, 1),
    ] {
        let mut f = fixture(2);
        f.int(field, value);
        let v = snapshot(&f, true).unwrap();
        assert!(v["opener"].is_null());
        not_read(&f, &[MANAGER + 0x188, TOPBAR, BUTTON]);
    }
    let mut f = fixture(2);
    f.put(ROOT + 0x95, &[8]);
    let v = snapshot(&f, true).unwrap();
    assert!(v["opener"].is_null());
    not_read(&f, &[TOPBAR]);
}
#[test]
fn foreign_topbar_or_alias_mismatch_refuses_before_callback_or_geometry() {
    for field in [
        TOPBAR + 0x30,
        TOPBAR + 0x58,
        TOPBAR + 0x60,
        TOPROOT + 0x88,
        TOPBAR + 0x88,
        TOPBAR + 0x7428,
        BUTTON + 0x58,
    ] {
        let mut f = fixture(2);
        f.word(field, FOREIGN);
        assert_eq!(snapshot(&f, true).unwrap_err().code, "invalid_construction");
        not_read(&f, &[FUNCTOR, BUTTON + 0xd8, FOREIGN]);
    }
}
#[test]
fn inactive_override_or_ambiguous_observers_refuse_before_functor_payload() {
    for (field, value) in [
        (BUTTON + 0x90, FOREIGN),
        (BUTTON + 0xc0, FOREIGN),
        (BUTTON + 0x80, 0),
        (NODE + 8, NODE),
        (NODE + 0x10, NODE),
        (NODE, FOREIGN),
    ] {
        let mut f = fixture(2);
        f.word(field, value);
        assert_eq!(snapshot(&f, true).unwrap_err().code, "invalid_construction");
        not_read(&f, &[FUNCTOR, FOREIGN, BUTTON + 0xd8]);
    }
    for field in [NODE + 0x18, BUTTON + 0x8c] {
        let mut f = fixture(2);
        f.put(field, &[1]);
        assert_eq!(
            snapshot(&f, false).unwrap_err().code,
            "invalid_construction"
        );
        not_read(&f, &[FUNCTOR]);
    }
}
#[test]
fn competing_click_callbacks_refuse_before_member_or_geometry_reads() {
    for offset in [0x60, 0x150, 0x180, 0x1b0, 0x1e0, 0x2d0, 0x3c0] {
        let mut f = fixture(2);
        f.word(TOPBAR + 0x3290 + offset, FOREIGN);
        assert_eq!(snapshot(&f, true).unwrap_err().code, "invalid_construction");
        not_read(&f, &[FUNCTOR, FOREIGN, BUTTON + 0xd8]);
    }
}
#[test]
fn wrong_callback_tuple_never_yields_a_rendered_opener() {
    for (field, value) in [
        (FUNCTOR, BASE + 0x3489820),
        (FUNCTOR + 8, BASE + 0x22f27d8),
        (FUNCTOR + 0x10, 8),
        (FUNCTOR + 0x18, FOREIGN),
    ] {
        let mut f = fixture(2);
        f.word(field, value);
        assert_eq!(snapshot(&f, true).unwrap_err().code, "invalid_construction");
        not_read(&f, &[BUTTON + 0xd8, FOREIGN]);
    }
}
#[test]
fn hidden_disabled_or_transitioning_topbar_omits_renderer_payload() {
    for (field, value) in [
        (TOPROOT + 0x1270, 1),
        (MENU + 0x12b8, 2),
        (BUTTON + 0x9c, 3),
    ] {
        let mut f = fixture(2);
        f.int(field, value);
        let v = snapshot(&f, true).unwrap();
        assert!(v["opener"]["rect"].is_null());
        not_read(&f, &[BUTTON + 0xd8]);
    }
    let mut f = fixture(2);
    f.put(MENU + 0x95, &[0]);
    let v = snapshot(&f, true).unwrap();
    assert!(v["opener"]["rect"].is_null());
    not_read(&f, &[BUTTON + 0xd8]);
}
#[test]
fn all_opener_fields_are_rechecked_in_the_final_witness() {
    for field in [
        MANAGER + 0x3a8,
        MANAGER + 0x3c0,
        TOPBAR + 0x58,
        TOPBAR + 0x88,
        TOPBAR + 0x7428,
        TOPROOT + 0xd30,
        MENU + 0x88,
        NODE + 0x18,
        FUNCTOR + 8,
        FUNCTOR + 0x18,
        BUTTON + 0x90,
        BUTTON + 0x8c,
        BUTTON + 0xc0,
        TOPBAR + 0x3290 + 0x1e0,
        TOPBAR + 0x3290 + 0x3c0,
        SPRITE + 0xb0,
        GUI + 0x98,
        GRAPHICS,
        GRAPHICS + 0x170,
        GRAPHICS + 0x178,
        GRAPHICS + 0x2c5b0,
    ] {
        let mut f = fixture(2);
        geometry(&mut f);
        f.final_fault.set(Some(field));
        assert_eq!(
            snapshot(&f, true).unwrap_err().code,
            "torn_read",
            "{field:x}"
        );
    }
}
#[test]
fn missing_opener_does_not_claim_a_verified_binding() {
    let mut f = fixture(2);
    f.word(MANAGER + 0x188, 0);
    let v = snapshot(&f, true).unwrap();
    assert_eq!(v["reason"], "opener_not_materialized");
    assert!(v["opener"].is_null());
    assert_eq!(v["input_authorized"], false);
}
#[test]
fn panel_probe_protocol_rejects_unknown_or_mistyped_options() {
    let server = Server {
        process: None,
        unavailable: None,
    };
    for args in [
        json!({"include_geometry":1}),
        json!({"include_geometry":null}),
        json!({"click":true}),
        json!({"building_id":"arms_factory"}),
    ] {
        let v=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_construction_panel_probe","arguments":args}})).unwrap();
        assert!(v.to_string().contains("invalid_arguments"));
    }
}
