use super::*;
use hoi4_live_native::{
    construction::mac_panel::{snapshot, snapshot_with_routing},
    protocol::Server,
};
use serde_json::json;

#[path = "mac_construction_shortcut.rs"]
mod shortcut;

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
    f.word(kind, BASE + 0x36bf800);
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
    f.word(object, BASE + 0x36ba978);
    f.word(object + 0x30, 0);
    f.word(object + 0x88, GUI);
    f.put(object + 0x95, &[if shown { 8 } else { 0 }]);
    f.int(object + 0x13d8, 0);
    f.int(object + 0x1390, 0);
}
fn fixture(mode: i32) -> MacFixture {
    let mut f = MacFixture::new();
    for (field, value) in [
        (IDLER + 0x4c0, GUI),
        (GUI, BASE + 0x36a5810),
        (IDLER + 0x670, MANAGER),
        (MANAGER + 0x180, IDLER),
        (MANAGER + 0x150, VIEW),
        (VIEW, BASE + 0x3647478),
        (VIEW + 0x488, IDLER),
        (VIEW + 0x30, MANAGER),
        (VIEW + 0x490, ROOT),
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
        (TOPBAR, BASE + 0x3697918),
        (TOPBAR + 0x58, IDLER),
        (TOPBAR + 0x60, MANAGER),
        (TOPBAR + 0x30, MANAGER),
        (TOPBAR + 0x68, TOPROOT),
        (TOPBAR + 0x88, MENU),
        (TOPBAR + 0x8c38, BUTTON),
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
        f.array(object + 0xe50, object + 0x4000, &[child]);
        name(&mut f, child, key);
    }
    for (field, value) in [
        (BUTTON, BASE + 0x36b9738),
        (BUTTON + 0x58, GUI),
        (BUTTON + 0x70, BASE + 0x36b9b18),
        (BUTTON + 0x78, NODE),
        (BUTTON + 0x80, NODE),
        (BUTTON + 0x90, 0),
        (BUTTON + 0xc8, 0),
        (NODE, TOPBAR + 0x4440),
        (NODE + 8, 0),
        (NODE + 0x10, 0),
        (TOPBAR + 0x4440, BASE + 0x3697a60),
        (TOPBAR + 0x4440 + 0x30, FUNCTOR),
        (FUNCTOR, BASE + 0x3697af8),
        (FUNCTOR + 8, BASE + 0x2460fb0),
        (FUNCTOR + 0x10, 0),
        (FUNCTOR + 0x18, TOPBAR),
    ] {
        f.word(field, value);
    }
    f.int(BUTTON + 0x88, 1);
    f.put(BUTTON + 0x8c, &[0]);
    for offset in [0x60, 0x150, 0x180, 0x1b0, 0x1e0, 0x2d0, 0x3c0] {
        f.word(TOPBAR + 0x4440 + offset, 0);
    }
    f.put(NODE + 0x18, &[0]);
    f.put(BUTTON + 0x65, &[8]);
    f.int(BUTTON + 0x9c, 0);
    f
}
fn geometry(f: &mut MacFixture) {
    f.word(GUI + 0x98, GRAPHICS);
    f.word(GRAPHICS, BASE + 0x35e5b58);
    for (offset, dimension) in [(0x170, 1920), (0x174, 1080), (0x178, 1920), (0x17c, 1080)] {
        f.int(GRAPHICS + offset, dimension);
    }
    f.put(GRAPHICS + 0x2c5b0, &1_f32.to_le_bytes());
    f.word(BUTTON + 0xe0, SPRITE);
    f.word(SPRITE, BASE + 0x36cc528);
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
    not_read(&f, &[BUTTON + 0xe0, GUI + 0x98]);
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
        not_read(&f, &[BUTTON + 0xe0, SPRITE]);
        assert!(snapshot(&f, false).is_ok());
    }
    let mut f = fixture(2);
    geometry(&mut f);
    f.put(GRAPHICS + 0x2c5b0, &1.5_f32.to_le_bytes());
    assert_eq!(
        snapshot(&f, true).unwrap_err().code,
        "gui_geometry_unavailable"
    );
    not_read(&f, &[BUTTON + 0xe0, SPRITE]);
}
#[test]
fn foreign_or_malformed_graphics_cannot_supply_opener_geometry() {
    for field in [GUI + 0x98, GRAPHICS] {
        let mut f = fixture(2);
        geometry(&mut f);
        f.word(FOREIGN, 0);
        f.word(field, FOREIGN);
        assert_eq!(snapshot(&f, true).unwrap_err().code, "invalid_gui_geometry");
        not_read(&f, &[BUTTON + 0xe0, SPRITE]);
    }
    for dimension in [0, -1, 16385] {
        let mut f = fixture(2);
        geometry(&mut f);
        f.int(GRAPHICS + 0x170, dimension);
        assert_eq!(snapshot(&f, true).unwrap_err().code, "invalid_gui_geometry");
        not_read(&f, &[BUTTON + 0xe0]);
    }
    for scale in [f32::NAN, f32::INFINITY, 0.0, 8.5] {
        let mut f = fixture(2);
        geometry(&mut f);
        f.put(GRAPHICS + 0x2c5b0, &scale.to_le_bytes());
        assert_eq!(snapshot(&f, true).unwrap_err().code, "invalid_gui_geometry");
        not_read(&f, &[BUTTON + 0xe0]);
    }
}
#[test]
fn already_selected_mode_never_reads_toggle_control_even_with_pending_visibility() {
    for (shown, state) in [(0, 0), (8, 1), (8, 2), (8, 0)] {
        let mut f = fixture(0x11);
        f.put(ROOT + 0x95, &[shown]);
        f.int(ROOT + 0x13d8, state);
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
        (ROOT + 0x1390, 1),
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
        TOPBAR + 0x8c38,
        BUTTON + 0x58,
    ] {
        let mut f = fixture(2);
        f.word(field, FOREIGN);
        assert_eq!(snapshot(&f, true).unwrap_err().code, "invalid_construction");
        not_read(&f, &[FUNCTOR, BUTTON + 0xe0, FOREIGN]);
    }
}
#[test]
fn country_menu_alias_requires_canonical_window_not_raw_membership() {
    // The live topbar stores TWindow, while its parent's membership stores
    // TWindow's CGuiObject at +0x30. Raw pointer equality rejected the real UI.
    let f = fixture(2);
    assert_eq!(snapshot(&f, false).unwrap()["reason"], "opener_observed");
    for alias in [MENU + 0x30, MENU + 8, FACTION, 0] {
        let mut f = fixture(2);
        f.word(TOPBAR + 0x88, alias);
        assert_eq!(
            snapshot(&f, false).unwrap_err().code,
            "invalid_construction"
        );
        not_read(&f, &[MENU + 0xe50, BUTTON, FUNCTOR]);
    }
    let mut f = fixture(2);
    f.word(MENU + 0x88, FOREIGN);
    assert_eq!(
        snapshot(&f, false).unwrap_err().code,
        "invalid_construction"
    );
    not_read(&f, &[TOPBAR + 0x88, MENU + 0xe50, FOREIGN]);
}
#[test]
fn inactive_override_or_ambiguous_observers_refuse_before_functor_payload() {
    for (field, value) in [
        (BUTTON + 0x90, FOREIGN),
        (BUTTON + 0xc8, FOREIGN),
        (BUTTON + 0x80, 0),
        (NODE + 8, NODE),
        (NODE + 0x10, NODE),
        (NODE, FOREIGN),
    ] {
        let mut f = fixture(2);
        f.word(field, value);
        assert_eq!(snapshot(&f, true).unwrap_err().code, "invalid_construction");
        not_read(&f, &[FUNCTOR, FOREIGN, BUTTON + 0xe0]);
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
        f.word(TOPBAR + 0x4440 + offset, FOREIGN);
        assert_eq!(snapshot(&f, true).unwrap_err().code, "invalid_construction");
        not_read(&f, &[FUNCTOR, FOREIGN, BUTTON + 0xe0]);
    }
}
#[test]
fn wrong_callback_tuple_never_yields_a_rendered_opener() {
    for (field, value) in [
        (FUNCTOR, BASE + 0x3697b00),
        (FUNCTOR + 8, BASE + 0x22f27d8),
        (FUNCTOR + 0x10, 8),
        (FUNCTOR + 0x18, FOREIGN),
    ] {
        let mut f = fixture(2);
        f.word(field, value);
        assert_eq!(snapshot(&f, true).unwrap_err().code, "invalid_construction");
        not_read(&f, &[BUTTON + 0xe0, FOREIGN]);
    }
}
#[test]
fn hidden_disabled_or_transitioning_topbar_omits_renderer_payload() {
    for (field, value) in [
        (TOPROOT + 0x1390, 1),
        (MENU + 0x13d8, 2),
        (BUTTON + 0x9c, 3),
    ] {
        let mut f = fixture(2);
        f.int(field, value);
        let v = snapshot(&f, true).unwrap();
        assert!(v["opener"]["rect"].is_null());
        not_read(&f, &[BUTTON + 0xe0]);
    }
    let mut f = fixture(2);
    f.put(MENU + 0x95, &[0]);
    let v = snapshot(&f, true).unwrap();
    assert!(v["opener"]["rect"].is_null());
    not_read(&f, &[BUTTON + 0xe0]);
}
#[test]
fn all_opener_fields_are_rechecked_in_the_final_witness() {
    for field in [
        MANAGER + 0x3a8,
        MANAGER + 0x3c0,
        TOPBAR + 0x58,
        TOPBAR + 0x88,
        TOPBAR + 0x8c38,
        TOPROOT + 0xe50,
        MENU + 0x88,
        NODE + 0x18,
        FUNCTOR + 8,
        FUNCTOR + 0x18,
        BUTTON + 0x90,
        BUTTON + 0x8c,
        BUTTON + 0xc8,
        TOPBAR + 0x4440 + 0x1e0,
        TOPBAR + 0x4440 + 0x3c0,
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
        json!({"include_geometry":true,"include_routing":1}),
        json!({"include_geometry":true,"include_routing":null}),
        json!({"include_routing":true}),
        json!({"include_geometry":false,"include_routing":true}),
        json!({"click":true}),
        json!({"building_id":"arms_factory"}),
    ] {
        let v=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_construction_panel_probe","arguments":args}})).unwrap();
        assert!(v.to_string().contains("invalid_arguments"));
    }
}

const SOURCES: u64 = 0x520000;
const ROUTE: u64 = 0x530000;

fn routes(f: &mut MacFixture, sources: &[u64], targets: &[u64]) {
    f.array(GRAPHICS + 0x50, SOURCES, sources);
    f.word(GRAPHICS + 0x28, ROUTE);
    f.int(GRAPHICS + 0x30, targets.len() as i32);
    f.int(GRAPHICS + 0x34, targets.len() as i32);
    for (i, target) in targets.iter().enumerate() {
        f.word(ROUTE + i as u64 * 16, BASE + 0x36bf490);
        f.word(ROUTE + i as u64 * 16 + 8, *target);
    }
}

fn routing_fixture() -> MacFixture {
    let mut f = fixture(2);
    geometry(&mut f);
    for (field, value) in [
        (BUTTON + 0xd8, GRAPHICS),
        (SPRITE + 0x138, BUTTON),
        (GUI + 0x120, BUTTON),
        (GUI + 0x128, BUTTON),
        (BUTTON + 0x180, 0),
        (BASE + 0x37ed910, 0),
    ] {
        f.word(field, value);
    }
    for field in [GUI + 0x130, GUI + 0x12, BUTTON + 0x1d9] {
        f.put(field, &[0]);
    }
    f.int(BUTTON + 0x64, 0x802);
    for (offset, value) in [
        (0x40, 237_f32),
        (0x44, 26.0),
        (0x48, 237.0),
        (0x4c, 26.0),
        (0x1c0, 1.0),
        (0x1c4, 1.0),
        (0x1c8, 1.0),
        (0x1cc, 1.0),
    ] {
        f.put(GRAPHICS + offset, &value.to_le_bytes());
    }
    routes(&mut f, &[FOREIGN, SPRITE], &[BUTTON, FOREIGN]);
    f
}

#[test]
fn route_correlation_is_opt_in_and_never_reads_unrelated_target_payloads() {
    let mut f = fixture(2);
    geometry(&mut f);
    assert!(snapshot(&f, true).unwrap()["opener"]["routing"].is_null());
    not_read(
        &f,
        &[
            GRAPHICS + 0x28,
            GRAPHICS + 0x50,
            SPRITE + 0x138,
            BUTTON + 0xd8,
        ],
    );
    let f = routing_fixture();
    let v = snapshot_with_routing(&f, true, true).unwrap();
    let r = &v["opener"]["routing"];
    assert_eq!(r["source_count"], 2);
    assert_eq!(r["route_count"], 2);
    for key in [
        "renderer_target_join_verified",
        "source_tail_matches_renderer",
        "first_route_target_matches_button",
        "cached_cursor_matches_target",
        "route_entry_types_verified",
    ] {
        assert_eq!(r[key], true, "{key}");
    }
    for key in [
        "cache_completion_verified",
        "os_cursor_verified",
        "input_authorized",
        "delivery_verified",
    ] {
        assert_eq!(r[key], false, "{key}");
    }
    assert_eq!(r["gui_route_aliases_match_button"], json!([true, true]));
    not_read(&f, &[FOREIGN]);
    assert!(!v.to_string().contains(&FOREIGN.to_string()));
}

#[test]
fn route_request_still_skips_already_selected_or_hidden_opener() {
    let f = fixture(0x11);
    assert!(snapshot_with_routing(&f, true, true).unwrap()["opener"].is_null());
    not_read(&f, &[MANAGER + 0x188, GUI + 0x98]);
    let mut f = fixture(2);
    f.put(MENU + 0x95, &[0]);
    assert!(snapshot_with_routing(&f, true, true).unwrap()["opener"]["routing"].is_null());
    not_read(&f, &[GUI + 0x98, GRAPHICS + 0x28]);
    let f = fixture(2);
    assert_eq!(
        snapshot_with_routing(&f, false, true).unwrap_err().code,
        "invalid_arguments"
    );
    assert!(f.reads.borrow().is_empty());
}

#[test]
fn route_membership_is_complete_and_selected_duplicates_remain_explicit() {
    let mut f = routing_fixture();
    let mut sources = vec![FOREIGN; 70];
    sources[1] = SPRITE;
    sources[69] = SPRITE;
    let mut targets = vec![FOREIGN; 70];
    targets[0] = BUTTON;
    targets[69] = BUTTON;
    routes(&mut f, &sources, &targets);
    let v = snapshot_with_routing(&f, true, true).unwrap();
    let r = &v["opener"]["routing"];
    assert_eq!(r["source_count"], 70);
    assert_eq!(r["route_count"], 70);
    assert_eq!(r["selected_source_occurrences"], 2);
    assert_eq!(r["selected_target_occurrences"], 2);
    not_read(&f, &[FOREIGN]);
    for field in [SOURCES + 69 * 8, ROUTE + 69 * 16 + 8] {
        let mut f = routing_fixture();
        routes(&mut f, &sources, &targets);
        f.final_fault.set(Some(field));
        assert_eq!(
            snapshot_with_routing(&f, true, true).unwrap_err().code,
            "torn_read"
        );
    }
}

#[test]
fn competing_or_empty_routes_do_not_follow_an_alternative_target() {
    for (sources, targets) in [
        (vec![SPRITE, FOREIGN], vec![FOREIGN, BUTTON]),
        (vec![], vec![]),
    ] {
        let mut f = routing_fixture();
        routes(&mut f, &sources, &targets);
        let v = snapshot_with_routing(&f, true, true).unwrap();
        assert_eq!(
            v["opener"]["routing"]["source_tail_matches_renderer"],
            false
        );
        assert_eq!(
            v["opener"]["routing"]["first_route_target_matches_button"],
            false
        );
        assert_eq!(v["input_authorized"], false);
        not_read(&f, &[FOREIGN]);
    }
}

#[test]
fn malformed_route_headers_refuse_without_truncation_or_payload_reads() {
    for offset in [0x28, 0x50] {
        for (capacity, count) in [(-1, 0), (1, -1), (1, 2), (4097, 4097), (20001, 2)] {
            let mut f = routing_fixture();
            f.int(GRAPHICS + offset + 8, capacity);
            f.int(GRAPHICS + offset + 12, count);
            assert!(snapshot_with_routing(&f, true, true).is_err());
            not_read(&f, &[if offset == 0x28 { ROUTE } else { SOURCES }]);
        }
        let mut f = routing_fixture();
        f.word(GRAPHICS + offset, FOREIGN + 1);
        assert!(snapshot_with_routing(&f, true, true).is_err());
        not_read(&f, &[FOREIGN]);
    }
}

#[test]
fn route_active_count_is_bounded_independently_of_unread_spare_capacity() {
    for offset in [0x28, 0x50] {
        for capacity in [4097, 10368, 20000] {
            let mut f = routing_fixture();
            f.int(GRAPHICS + offset + 8, capacity);
            let v = snapshot_with_routing(&f, true, true).unwrap();
            assert_eq!(v["opener"]["routing"]["source_count"], 2);
            assert_eq!(v["opener"]["routing"]["route_count"], 2);
            // Only two active entries exist. Spare allocation is unmapped.
            not_read(&f, &[SOURCES + 16, ROUTE + 32]);
        }
    }
}

#[test]
fn renderer_owner_target_and_persistent_route_type_must_match() {
    for (field, value) in [
        (BUTTON + 0xd8, FOREIGN),
        (SPRITE + 0x138, FOREIGN),
        (ROUTE, BASE + 0x36ba938),
    ] {
        let mut f = routing_fixture();
        f.word(field, value);
        assert_eq!(
            snapshot_with_routing(&f, true, true).unwrap_err().code,
            "invalid_construction"
        );
        not_read(&f, &[FOREIGN]);
    }
    let mut f = routing_fixture();
    f.word(SPRITE, BASE + 0x36be530);
    assert!(snapshot_with_routing(&f, true, true).is_ok());
}

#[test]
fn cursor_conversion_uses_both_factor_pairs_and_engine_truncation() {
    let mut f = routing_fixture();
    for offset in [0x40, 0x48] {
        f.put(GRAPHICS + offset, &118.0_f32.to_le_bytes());
    }
    f.put(GRAPHICS + 0x1c0, &0.5_f32.to_le_bytes());
    f.put(GRAPHICS + 0x1c8, &2.0_f32.to_le_bytes());
    let v = snapshot_with_routing(&f, true, true).unwrap();
    let r = &v["opener"]["routing"];
    assert_eq!(r["expected_graphics_stamp"], json!([118.0, 26.0]));
    assert_eq!(r["processed_event_point"], json!([236, 26]));
    assert_eq!(r["cached_cursor_matches_target"], false);
    let mut f = routing_fixture();
    f.put(GRAPHICS + 0x48, &236.0_f32.to_le_bytes());
    assert_eq!(
        snapshot_with_routing(&f, true, true).unwrap()["opener"]["routing"]
            ["cached_cursor_matches_target"],
        false
    );
}

#[test]
fn nonfinite_or_unbounded_cursor_conversions_refuse() {
    for (offset, value) in [
        (0x40, f32::NAN),
        (0x4c, f32::INFINITY),
        (0x1c0, 0.0),
        (0x1c4, -1.0),
        (0x1c8, 101.0),
        (0x48, 1_000_001.0),
    ] {
        let mut f = routing_fixture();
        f.put(GRAPHICS + offset, &value.to_le_bytes());
        assert_eq!(
            snapshot_with_routing(&f, true, true).unwrap_err().code,
            "invalid_construction"
        );
    }
    let mut f = routing_fixture();
    f.put(GRAPHICS + 0x48, &20_000.0_f32.to_le_bytes());
    f.put(GRAPHICS + 0x1c8, &100.0_f32.to_le_bytes());
    assert_eq!(
        snapshot_with_routing(&f, true, true).unwrap_err().code,
        "invalid_construction"
    );
}

#[test]
fn wrapper_and_unknown_hook_gates_are_reported_without_following_them() {
    let mut f = routing_fixture();
    f.put(GUI + 0x130, &[1]);
    f.put(GUI + 0x12, &[1]);
    f.int(BUTTON + 0x64, 0x801); // bit 0 does not satisfy virtual+18's bit 1.
    f.put(BUTTON + 0x1d9, &[1]);
    f.word(BUTTON + 0x180, FOREIGN);
    f.word(BASE + 0x37ed910, FOREIGN);
    let v = snapshot_with_routing(&f, true, true).unwrap();
    let r = &v["opener"]["routing"];
    assert_eq!(r["conditional_target_gate_clear"], false);
    assert_eq!(r["source_target_flags_eligible"], true);
    assert_eq!(r["dispatch_skip_raw"], 1);
    assert_eq!(r["standard_wrapper_gate_raw"], 1);
    assert_eq!(r["unsupported_optional_gate_present"], true);
    assert_eq!(r["unsupported_post_click_hook_present"], true);
    not_read(&f, &[FOREIGN]);
}

#[test]
fn routing_fields_share_the_full_final_witness_bracket() {
    for field in [
        BUTTON + 0xd8,
        SPRITE + 0x138,
        GRAPHICS + 0x50,
        GRAPHICS + 0x28,
        SOURCES + 8,
        ROUTE + 16,
        GRAPHICS + 0x40,
        GRAPHICS + 0x48,
        GRAPHICS + 0x1c0,
        GRAPHICS + 0x1cc,
        GUI + 0x120,
        GUI + 0x128,
        GUI + 0x130,
        GUI + 0x12,
        BUTTON + 0x64,
        BUTTON + 0x180,
        BUTTON + 0x1d9,
        BASE + 0x37ed910,
    ] {
        let f = routing_fixture();
        f.final_fault.set(Some(field));
        assert_eq!(
            snapshot_with_routing(&f, true, true).unwrap_err().code,
            "torn_read",
            "{field:x}"
        );
    }
}
