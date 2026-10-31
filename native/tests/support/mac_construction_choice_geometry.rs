use super::*;

const SECTION: u64 = 0x450000;
const SPRITE: u64 = 0xd20000;
const SPRITE_TYPE: u64 = 0xd30000;
const GRAPHICS: u64 = 0xe00000;
const PARENT: u64 = 0xe40000;
const SOURCES: u64 = 0xe50000;
const ROUTE: u64 = 0xe60000;

fn pair(f: &mut MacFixture, address: u64, values: [f32; 2]) {
    f.put(address, &values[0].to_le_bytes());
    f.put(address + 4, &values[1].to_le_bytes());
}

fn rectangle(f: &mut MacFixture, x: i32, y: i32, width: i32, height: i32) {
    f.int(SPRITE_TYPE + 0x78, 1);
    f.int(SPRITE_TYPE + 0x8c, width);
    f.int(SPRITE_TYPE + 0x90, height);
    pair(
        f,
        SPRITE + 0xb0,
        [(x - 960) as f32, (540 - y - height) as f32],
    );
}

fn geometric(count: usize, callback: bool) -> MacFixture {
    let mut f = if callback {
        bound(count)
    } else {
        fixture(true, count)
    };
    f.int(MANAGER + 0x3a8, 17);
    for ancestor in [ROOT, POSSIBLE, SECTION, FRAME] {
        f.int(ancestor + 0x13d8, 0);
        f.int(ancestor + 0x1390, 0);
    }
    for (field, value) in [
        (REGISTRY + 0x98, GRAPHICS),
        (GRAPHICS, BASE + 0x35e5b58),
        (BUTTON + 0xd8, GRAPHICS),
        (BUTTON + 0xe0, SPRITE),
        (BUTTON + 0xc8, 0),
        (BUTTON + 0x180, PARENT),
        (SPRITE, BASE + 0x36cc528),
        (SPRITE + 8, SPRITE_TYPE),
        (SPRITE + 0x108, PARENT),
        (SPRITE + 0x138, BUTTON),
        (FRAME + 0x1358, PARENT),
        (PARENT, BASE + 0x36bea88),
        (BASE + 0x37ed910, 0),
    ] {
        f.word(field, value);
    }
    for (offset, value) in [(0x170, 1920), (0x174, 1080), (0x178, 1920), (0x17c, 1080)] {
        f.int(GRAPHICS + offset, value);
    }
    f.put(GRAPHICS + 0x2c5b0, &1_f32.to_le_bytes());
    f.put(BASE + 0x372b2b8, &[0]);
    f.put(BUTTON + 0xd4, &[8]);
    f.put(BUTTON + 0x1d9, &[0]);
    f.put(PARENT + 0x128, &[1]);
    rectangle(&mut f, 300, 200, 120, 40);
    f
}

fn with_routes(f: &mut MacFixture, sources: &[u64], targets: &[u64]) {
    f.array(GRAPHICS + 0x50, SOURCES, sources);
    f.word(GRAPHICS + 0x28, ROUTE);
    f.int(GRAPHICS + 0x30, targets.len() as i32);
    f.int(GRAPHICS + 0x34, targets.len() as i32);
    for (i, target) in targets.iter().enumerate() {
        f.word(ROUTE + i as u64 * 16, BASE + 0x36bf490);
        f.word(ROUTE + i as u64 * 16 + 8, *target);
    }
    for (field, value) in [(REGISTRY + 0x120, BUTTON), (REGISTRY + 0x128, BUTTON)] {
        f.word(field, value);
    }
    f.put(REGISTRY + 0x130, &[0]);
    f.put(REGISTRY + 0x12, &[0]);
    f.int(BUTTON + 0x64, 0x802); // Includes the independently read shown byte.
    pair(f, GRAPHICS + 0x40, [360.0, 220.0]);
    pair(f, GRAPHICS + 0x48, [360.0, 220.0]);
    pair(f, GRAPHICS + 0x1c0, [1.0, 1.0]);
    pair(f, GRAPHICS + 0x1c8, [1.0, 1.0]);
}

fn geometry(f: &MacFixture, callback: bool, routing: bool) -> Result<Value> {
    snapshot_with_options(
        f,
        "arms_factory",
        1,
        Options {
            callback,
            geometry: true,
            routing,
        },
    )
}

fn unavailable(v: &Value) {
    assert_eq!(v["geometry_available"], false);
    let target = &v["choices"]["target"];
    if !target.is_null() {
        assert_eq!(target["geometry"]["available"], false);
        assert!(target["geometry"]["rect"].is_null());
        assert!(target["geometry"]["routing"].is_null());
        assert_eq!(target["geometry"]["input_authorized"], false);
    }
}

#[test]
fn defaults_keep_geometry_and_routing_payload_unmapped_and_unread() {
    let f = fixture(true, 1);
    let default = observe(&f, "arms_factory", 1).unwrap();
    let reads = f.reads.borrow().clone();
    f.reads.borrow_mut().clear();
    let explicit = snapshot_with_options(&f, "arms_factory", 1, Options::default()).unwrap();
    assert_eq!(*f.reads.borrow(), reads);
    assert_eq!(default["choices"], explicit["choices"]);
    assert!(explicit["choices"]["target"]["geometry"].is_null());
    assert_eq!(explicit["geometry_available"], false);
    not_read(
        &f,
        &[
            MANAGER + 0x3a8,
            REGISTRY + 0x98,
            BUTTON + 0xd8,
            BUTTON + 0xe0,
            BUTTON + 0xd4,
            BUTTON + 0x180,
            FRAME + 0x1358,
            POSSIBLE + 0x13d8,
            SECTION + 0x1390,
            BASE + 0x372b2b8,
            BASE + 0x37ed910,
        ],
    );
}

#[test]
fn geometry_without_callback_reads_only_requested_geometry_and_grants_no_authority() {
    let f = geometric(70, false); // Callback nodes and glue remain unmapped.
    let v = geometry(&f, false, false).unwrap();
    assert_eq!(v["choices"]["item_count"], 70);
    assert_eq!(v["geometry_available"], true);
    assert_eq!(v["callback_binding_verified"], false);
    let target = &v["choices"]["target"];
    assert!(target["callback"].is_null());
    let g = &target["geometry"];
    assert_eq!(g["available"], true);
    assert_eq!(g["reason"], "requested_geometry_observed");
    assert_eq!(g["rect"], json!({"x":300,"y":200,"width":120,"height":40}));
    assert!(g["routing"].is_null());
    assert_eq!(g["ancestor_visibility_verified"], true);
    assert_eq!(g["graphical_parent_verified"], true);
    for field in [
        "input_authorized",
        "settled_frame_verified",
        "delivery_verified",
        "prerequisites_evaluated",
    ] {
        assert_eq!(g[field], false, "{field}");
    }
    for i in 0..69 {
        not_read(&f, &[ITEMS + i * 0x400 + 0x10, ITEMS + i * 0x400 + 0x30]);
    }
    not_read(
        &f,
        &[
            BUTTON + 0x70,
            NODE,
            FUNCTION,
            GRAPHICS + 0x28,
            GRAPHICS + 0x50,
            REGISTRY + 0x120,
            REGISTRY + 0x130,
        ],
    );
    let rendered = v.to_string();
    for address in [SPRITE, GRAPHICS, PARENT, FRAME, ITEMS + 69 * 0x400] {
        assert!(!rendered.contains(&address.to_string()));
    }
}

#[test]
fn callback_geometry_and_routing_share_one_target_and_leave_conversion_opaque() {
    let mut f = geometric(1, true);
    add_conversion(&mut f);
    with_routes(&mut f, &[FOREIGN, SPRITE], &[BUTTON, FOREIGN]);
    let v = geometry(&f, true, true).unwrap();
    assert_eq!(v["choices"]["ordinary_building_item_count"], 1);
    assert_eq!(v["choices"]["conversion_item_count"], 1);
    assert_eq!(v["callback_binding_verified"], true);
    assert_eq!(v["geometry_available"], true);
    let r = &v["choices"]["target"]["geometry"]["routing"];
    for field in [
        "complete_membership_observed",
        "renderer_target_join_verified",
        "route_entry_types_verified",
        "source_tail_matches_renderer",
        "first_route_target_matches_button",
        "cached_cursor_matches_target",
    ] {
        assert_eq!(r[field], true, "{field}");
    }
    assert_eq!(r["source_count"], 2);
    assert_eq!(r["route_count"], 2);
    assert_eq!(r["target_client_point"], json!([360, 220]));
    assert_eq!(r["unsupported_optional_gate_present"], false);
    for field in [
        "cache_completion_verified",
        "os_cursor_verified",
        "input_authorized",
        "delivery_verified",
    ] {
        assert_eq!(r[field], false, "{field}");
    }
    not_read(&f, &[CONVERSION + 0x10, CONVERSION + 0x20, FOREIGN]);
}

#[test]
fn exact_animated_renderer_uses_per_frame_size() {
    let mut f = geometric(1, false);
    f.word(SPRITE, BASE + 0x36be530);
    f.int(SPRITE_TYPE + 0x78, 2);
    f.int(SPRITE_TYPE + 0x8c, 240);
    assert_eq!(
        geometry(&f, false, false).unwrap()["choices"]["target"]["geometry"]["rect"],
        json!({"x":300,"y":200,"width":120,"height":40})
    );
}

#[test]
fn null_graphical_parent_does_not_read_parent_or_attachment_payload() {
    let mut f = geometric(1, false);
    f.word(BUTTON + 0x180, 0);
    for (field, size) in [
        (SPRITE + 0x108, 8),
        (FRAME + 0x1358, 8),
        (PARENT, 8),
        (PARENT + 0x128, 1),
    ] {
        for offset in 0..size {
            f.data.remove(&(field + offset));
        }
    }
    let v = geometry(&f, false, false).unwrap();
    assert_eq!(v["geometry_available"], true);
    assert_eq!(
        v["choices"]["target"]["geometry"]["graphical_parent_verified"],
        true
    );
    not_read(
        &f,
        &[SPRITE + 0x108, FRAME + 0x1358, PARENT, PARENT + 0x128],
    );
}

#[test]
fn unrelated_hidden_sections_do_not_become_target_geometry_ancestors() {
    let mut f = geometric(1, false);
    for section in [0x460000, 0x470000, 0x480000] {
        f.put(section + 0x95, &[0]);
    }
    assert_eq!(
        geometry(&f, false, false).unwrap()["geometry_available"],
        true
    );
    for section in [0x460000, 0x470000, 0x480000] {
        not_read(&f, &[section + 0x13d8, section + 0x1390]);
    }
}

#[test]
fn every_hidden_target_ancestor_prevents_geometry_without_renderer_reads() {
    for ancestor in [ROOT, POSSIBLE, SECTION, FRAME] {
        let mut f = geometric(1, false);
        f.put(ancestor + 0x95, &[0]);
        unavailable(&geometry(&f, false, false).unwrap());
        not_read(&f, &[BUTTON + 0xe0, SPRITE, GRAPHICS]);
    }
}

#[test]
fn every_animating_target_ancestor_prevents_geometry_without_renderer_reads() {
    for ancestor in [ROOT, POSSIBLE, SECTION, FRAME] {
        for offset in [0x13d8, 0x1390] {
            for state in [1, 2] {
                let mut f = geometric(1, false);
                f.int(ancestor + offset, state);
                unavailable(&geometry(&f, false, false).unwrap());
                not_read(&f, &[BUTTON + 0xe0, SPRITE, GRAPHICS]);
            }
        }
    }
}

#[test]
fn unknown_target_ancestor_transition_is_a_structural_refusal() {
    for ancestor in [ROOT, POSSIBLE, SECTION, FRAME] {
        for offset in [0x13d8, 0x1390] {
            let mut f = geometric(1, false);
            f.int(ancestor + offset, 3);
            assert_eq!(
                geometry(&f, false, false).unwrap_err().code,
                "invalid_construction"
            );
            not_read(&f, &[BUTTON + 0xe0, SPRITE]);
        }
    }
}

#[test]
fn coherent_button_and_normal_path_gates_report_unavailability_without_callback_reads() {
    for (field, value) in [
        (BUTTON + 0x65, 0),
        (BUTTON + 0xd4, 0),
        (BUTTON + 0xd4, 0x48),
        (BUTTON + 0x1d9, 1),
        (PARENT + 0x128, 0),
        (BASE + 0x372b2b8, 1),
    ] {
        let mut f = geometric(1, false);
        f.put(field, &[value]);
        unavailable(&geometry(&f, false, false).unwrap());
        not_read(&f, &[NODE, FUNCTION, BUTTON + 0x70]);
    }
    for state in [1, 3] {
        let mut f = geometric(1, false);
        f.int(BUTTON + 0x9c, state);
        unavailable(&geometry(&f, false, false).unwrap());
    }
    for field in [BUTTON + 0xc8, BASE + 0x37ed910] {
        let mut f = geometric(1, false);
        f.word(field, FOREIGN);
        unavailable(&geometry(&f, false, false).unwrap());
        not_read(&f, &[FOREIGN, NODE, FUNCTION]);
    }
}

#[test]
fn supported_hover_state_and_input_enabled_byte_are_not_prerequisite_proofs() {
    let mut f = geometric(1, false);
    f.int(BUTTON + 0x9c, 2);
    let v = geometry(&f, false, false).unwrap();
    assert_eq!(v["geometry_available"], true);
    assert_eq!(
        v["choices"]["target"]["geometry"]["prerequisites_evaluated"],
        false
    );
    assert_eq!(v["placement_eligibility_verified"], false);
}

#[test]
fn panel_must_still_be_the_selected_managed_mode() {
    for mode in [0, 2, 16, 18] {
        let mut f = geometric(1, false);
        f.int(MANAGER + 0x3a8, mode);
        unavailable(&geometry(&f, false, false).unwrap());
        not_read(&f, &[REGISTRY + 0x98, SPRITE]);
    }
}

#[test]
fn partially_or_completely_offscreen_rectangles_are_unavailable_not_clipped() {
    for (x, y) in [
        (-20, 200),
        (1900, 200),
        (300, -10),
        (300, 1060),
        (-121, 200),
    ] {
        let mut f = geometric(1, false);
        rectangle(&mut f, x, y, 120, 40);
        unavailable(&geometry(&f, false, false).unwrap());
    }
}

#[test]
fn requested_geometry_never_uses_an_unsupported_canvas_profile() {
    for (field, value) in [
        (GRAPHICS + 0x170, 2560),
        (GRAPHICS + 0x174, 1440),
        (GRAPHICS + 0x178, 960),
        (GRAPHICS + 0x17c, 540),
    ] {
        let mut f = geometric(1, false);
        f.int(field, value);
        unavailable(&geometry(&f, false, false).unwrap());
        not_read(&f, &[BUTTON + 0xe0, SPRITE]);
    }
    let mut f = geometric(1, false);
    f.put(GRAPHICS + 0x2c5b0, &1.5_f32.to_le_bytes());
    unavailable(&geometry(&f, false, false).unwrap());
    not_read(&f, &[BUTTON + 0xe0, SPRITE]);
}

#[test]
fn foreign_graphics_renderer_target_or_parent_joins_cannot_be_adopted() {
    for (field, value) in [
        (BUTTON + 0xd8, FOREIGN),
        (SPRITE + 0x138, FOREIGN),
        (SPRITE + 0x108, FOREIGN),
        (FRAME + 0x1358, FOREIGN),
        (PARENT, BASE + 0x34b0430),
        (SPRITE, BASE + 0x36cee08),
    ] {
        let mut f = geometric(1, false);
        f.word(field, value);
        assert!(geometry(&f, false, false).is_err(), "{field:x}");
        not_read(&f, &[FOREIGN]);
    }
    for ancestor in [ROOT, POSSIBLE, SECTION, FRAME] {
        let mut f = geometric(1, false);
        f.word(ancestor + 0x88, FOREIGN);
        assert_eq!(
            geometry(&f, false, false).unwrap_err().code,
            "invalid_construction"
        );
        not_read(&f, &[FOREIGN, SPRITE]);
    }
}

#[test]
fn malformed_sprite_geometry_is_not_downgraded_to_coherent_unavailability() {
    for value in [f32::NAN, f32::INFINITY, 1_000_001.0] {
        let mut f = geometric(1, false);
        f.put(SPRITE + 0xb0, &value.to_le_bytes());
        assert!(geometry(&f, false, false).is_err());
    }
    for (field, value) in [
        (SPRITE_TYPE + 0x78, 0),
        (SPRITE_TYPE + 0x8c, -1),
        (SPRITE_TYPE + 0x90, 0),
    ] {
        let mut f = geometric(1, false);
        f.int(field, value);
        assert!(geometry(&f, false, false).is_err());
    }
}

#[test]
fn all_requested_geometry_fields_and_bindings_share_final_witnesses() {
    for field in [
        MANAGER + 0x3a8,
        BASE + 0x372afc0,
        BASE + 0x372b2b8,
        BASE + 0x37ed910,
        POSSIBLE + 0x95,
        SECTION + 0x95,
        FRAME + 0x95,
        POSSIBLE + 0x13d8,
        SECTION + 0x1390,
        FRAME + 0x1358,
        BUTTON + 0xd4,
        BUTTON + 0xc8,
        BUTTON + 0x1d9,
        BUTTON + 0x180,
        BUTTON + 0xd8,
        BUTTON + 0xe0,
        REGISTRY + 0x98,
        GRAPHICS,
        GRAPHICS + 0x170,
        GRAPHICS + 0x17c,
        GRAPHICS + 0x2c5b0,
        SPRITE,
        SPRITE + 8,
        SPRITE + 0xb0,
        SPRITE + 0x108,
        SPRITE + 0x138,
        SPRITE_TYPE + 0x78,
        SPRITE_TYPE + 0x8c,
        PARENT,
        PARENT + 0x128,
    ] {
        let f = geometric(1, false);
        f.final_fault.set(Some(field));
        assert_eq!(
            geometry(&f, false, false).unwrap_err().code,
            "torn_read",
            "{field:x}"
        );
    }
}

#[test]
fn routing_retains_complete_ordered_membership_but_never_follows_other_payloads() {
    let mut f = geometric(1, false);
    let mut sources = vec![FOREIGN; 70];
    let mut targets = vec![FOREIGN; 70];
    sources[0] = SPRITE;
    sources[69] = SPRITE;
    targets[1] = BUTTON;
    targets[69] = BUTTON;
    with_routes(&mut f, &sources, &targets);
    let v = geometry(&f, false, true).unwrap();
    let r = &v["choices"]["target"]["geometry"]["routing"];
    assert_eq!(r["source_count"], 70);
    assert_eq!(r["route_count"], 70);
    assert_eq!(r["selected_source_occurrences"], 2);
    assert_eq!(r["selected_target_occurrences"], 2);
    assert_eq!(r["source_tail_matches_renderer"], true);
    assert_eq!(r["first_route_target_matches_button"], false);
    assert_eq!(r["input_authorized"], false);
    not_read(&f, &[FOREIGN]);
    for field in [SOURCES + 69 * 8, ROUTE + 69 * 16 + 8] {
        let mut f = geometric(1, false);
        with_routes(&mut f, &sources, &targets);
        f.final_fault.set(Some(field));
        assert_eq!(geometry(&f, false, true).unwrap_err().code, "torn_read");
    }
}

#[test]
fn route_payload_and_geometry_use_one_final_witness_bracket() {
    for field in [
        GRAPHICS + 0x28,
        GRAPHICS + 0x50,
        GRAPHICS + 0x40,
        GRAPHICS + 0x48,
        GRAPHICS + 0x1c0,
        GRAPHICS + 0x1cc,
        SOURCES + 8,
        ROUTE + 16,
        REGISTRY + 0x120,
        REGISTRY + 0x128,
        REGISTRY + 0x130,
        REGISTRY + 0x12,
        BUTTON + 0x64,
    ] {
        let mut f = geometric(1, false);
        with_routes(&mut f, &[FOREIGN, SPRITE], &[BUTTON, FOREIGN]);
        f.final_fault.set(Some(field));
        assert_eq!(
            geometry(&f, false, true).unwrap_err().code,
            "torn_read",
            "{field:x}"
        );
    }
}

#[test]
fn route_dependency_is_checked_before_any_process_method() {
    struct NoProcessAccess;
    impl ReadMemory for NoProcessAccess {
        fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
            panic!("memory access before option validation")
        }
        fn string_abi(&self) -> StringAbi {
            panic!("ABI access before option validation")
        }
    }
    impl Process for NoProcessAccess {
        fn begin_snapshot(&self) -> Result<()> {
            panic!("snapshot before option validation")
        }
        fn check(&self) -> Result<()> {
            panic!("identity check before option validation")
        }
        fn image_base(&self) -> u64 {
            panic!("base access before option validation")
        }
        fn identity(&self) -> Option<&Identity> {
            panic!("identity access before option validation")
        }
    }
    for callback in [false, true] {
        let error = snapshot_with_options(
            &NoProcessAccess,
            "arms_factory",
            1,
            Options {
                callback,
                geometry: false,
                routing: true,
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "invalid_arguments");
    }
}
