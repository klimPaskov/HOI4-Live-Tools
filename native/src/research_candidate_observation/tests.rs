use super::*;
#[test]
fn incomplete_closure_diagnostic_does_not_echo_unbounded_or_untrusted_values() {
    let h = serde_json::json!({"status":"injected arbitrary status /private/path","graphics_root_closed":false,
        "observed_ancestors":99999,"payload_hops":18446744073709551615u64,
        "chain":[{"type_encoding":"/Users/private/unknown"}],
        "policy_boundary":{"stage":"arbitrary /private/path","policy_scope":"unknown","count":99999999,"capacity":99999999}});
    let e = common(&serde_json::Value::Null, "w", "a", "s", &h).unwrap_err();
    assert_eq!(e.code, "research_geometry_unavailable");
    assert!(e.message.contains("status=missing_or_invalid"));
    assert!(e.message.contains("ancestors=unknown"));
    assert!(
        !e.message.contains("/Users/")
            && !e.message.contains("/private/")
            && !e.message.contains("99999")
    );
    assert!(e.message.len() < 600);
}
use serde_json::json;
#[test]
fn policy_closure_refusal_retains_each_boundary_quantity_within_durable_limit() {
    for (stage, expected_stage) in [
        ("container_position_membership", "container"),
        ("ancestor_membership", "ancestor"),
        ("arbitrary/path", "unknown"),
    ] {
        for (scope, expected_scope) in [
            ("exact_owned_graphics_root", "owned_root"),
            ("ordinary_ancestor", "ordinary"),
            ("own_focus_position_ancestor", "focus_position"),
            ("arbitrary/path", "unknown"),
        ] {
            for count in [json!(20000), Value::Null, json!(20001), json!(-1)] {
                let h = json!({"status":"unavailable_policy_bound","graphics_root_closed":false,
                    "payload_hops":16,"chain":[{"type_encoding":"A".repeat(128)}],
                    "policy_boundary":{"stage":stage,"policy_scope":scope,"count":count,"capacity":count,
                        "capacity_limit":count,"inspected_entries_before_attempt":count,"aggregate_entry_limit":count}});
                let e = common(&Value::Null, "w", "a", "s", &h).unwrap_err();
                let n = if count == 20000 { "20000" } else { "unknown" };
                assert_eq!(e.code, "research_geometry_unavailable");
                assert_eq!(e.message, format!("Root closure policy bound: stage={expected_stage} scope={expected_scope} count={n} cap={n} cap_max={n} prior={n} sum_max={n} hops=16"));
                assert!(e.message.is_ascii() && e.message.len() <= 160);
                assert!(!e.message.contains("arbitrary") && !e.message.contains(&"A".repeat(128)));
            }
        }
    }
}
#[test]
fn malformed_or_contradictory_policy_closure_never_produces_a_candidate() {
    for boundary in [
        Value::Null,
        json!("untrusted/path"),
        json!({"count":u64::MAX,"stage":[],"policy_scope":{},"capacity":-1}),
    ] {
        for closed in [true, false] {
            let h = json!({"status":"unavailable_policy_bound","graphics_root_closed":closed,
                "policy_boundary":boundary,"payload_hops":u64::MAX});
            let e = common(&Value::Null, "w", "a", "s", &h).unwrap_err();
            assert_eq!(e.message,"Root closure policy bound: stage=unknown scope=unknown count=unknown cap=unknown cap_max=unknown prior=unknown sum_max=unknown hops=unknown");
            assert!(e.message.is_ascii() && e.message.len() <= 160);
        }
    }
}
fn profile() -> OwnedClientProfile {
    OwnedClientProfile {
        window: 42,
        client: [512, 288],
        logical: [2560, 1440],
        os_binding_verified: true,
        reviewed_builtin_client_origin: true,
    }
}
fn geometry() -> Value {
    json!({"build_id":game::BUILD_ID,"physical_dimensions":[512,288],"logical_dimensions":[2560,1440],
      "logical_per_physical":[5.0,5.0],"physical_per_logical":[0.2,0.2],
      "input_mapping_verified":false,"input_source":{"point_diagnostic":{"getter_source_layout_verified":true,
        "window_association":{"association_available":true,"normal_absolute_mode":true,"graphics_dimensions_match":true,
          "native_window_id":42,"window_dimensions":[512,288],"relative_mode_raw":[0,0],"owned_os_window_verified":false,"client_origin_verified":false}}}})
}
fn hierarchy() -> Value {
    json!({"graphics_root_closed":true,"status":"closed_to_owned_graphics_root","chain":[{"role":"owned_graphics_root_1f0"}],
      "unit_cache_diagnostic":{"cache_consistent_under_observed_unit_branch":true}})
}
#[test]
fn instant_text_parser_requires_exact_join_and_uses_its_own_hit_operands() {
    let raw = json!({"render_type_encoding":"18CInstantTextObject",
        "field_layout_verified":true,"renderer_control_join_verified":true,
        "flags228_raw":128,"cache_translation_raw":[100.75,-200.25],
        "scalar_raw":1,"position_raw":[11,13],"alignment_offset_raw":[-21,31],
        "dimensions_raw":[80,40],"alignment_raw":4,"parent_chain":hierarchy()});
    let c = codec(&geometry(), &profile()).unwrap();
    let p = research_attempt::point_candidate(&control_surface(&raw).unwrap(), &c).unwrap();
    assert_eq!(p.ideal_logical, [141, 248]);
    assert_eq!(p.client_pixel, [28, 50]);
    assert!(!p.input_authorized && !p.effective_routing_verified);
    for (path, value) in [
        ("/field_layout_verified", json!(false)),
        ("/renderer_control_join_verified", json!(false)),
        ("/flags228_raw", json!(256)),
        ("/flags228_raw", json!(-1)),
        ("/flags228_raw", json!(129)),
        ("/alignment_raw", json!(7)),
        (
            "/parent_chain/unit_cache_diagnostic/cache_consistent_under_observed_unit_branch",
            json!(false),
        ),
        (
            "/parent_chain/chain/0/role",
            json!("owned_graphics_root_350"),
        ),
        ("/render_type_encoding", json!("14CInstantSprite")),
    ] {
        let mut bad = raw.clone();
        *bad.pointer_mut(path).unwrap() = value;
        assert!(
            control_surface(&bad)
                .and_then(|s| research_attempt::point_candidate(&s, &c))
                .is_err(),
            "{path}"
        );
    }
}
#[test]
fn a_closed_unknown_or_350_scope_cannot_fabricate_root1f0_coordinate_authority() {
    for role in [
        serde_json::Value::Null,
        json!("owned_graphics_root_350"),
        json!("arbitrary_root"),
    ] {
        let mut h = hierarchy();
        h["chain"][0]["role"] = role;
        let e = common(&serde_json::Value::Null, "w", "a", "s", &h).unwrap_err();
        assert!(e.message.contains("root1f0 coordinate scope required"));
    }
}
fn detail(research: bool) -> Value {
    json!({"owned":true,"build_id":game::BUILD_ID,"id":"support_weapons","selected_slot":0,
      "detail":{"id":"support_weapons","distinct_bound_items":1,"render_diagnostic":{
        "field_layout_verified":true,"render_type_encoding":"7CSprite","resource_type_encoding":"11CSpriteType",
        "raw":{"visibility_flag":1,"cached_world_translation":[-106,-241],"pivot_dimensions":[512,288],"local_scalar":1,
          "physical_scaled_clip":[0,0,512,288]},"resource_cache":{"cache_equivalent":true,"no_mask_rejection_branch":true,
          "texture_dimensions":[72,72],"grid":[1,1],"orientation_raw":0},"parent_chain":hierarchy()}},
      "own_info_postcondition":{"matching_locally_visible_count":u32::from(research),"ordinary_research_button":{
        "control_layout_verified":true,"ordinary_alias_matches":true,"callback_binding_verified":true,"renderer_control_join_verified":true,
        "disabled_raw":0,"flags_raw":0,"renderer_type_encoding":"11CTextSprite","text_raw_diagnostic":{
          "resource_layout_verified":true,"raw_fields_available":true,"cache_translation_raw":[634,-47],"anchor_raw":[512,288],"scalar_raw":1,
          "renderer_width_raw":123,"renderer_height_raw":36,"resource_count_raw":1,"hit_width_divisor_raw":1,"transform_110_raw":0,
          "parent_chain":hierarchy()}}}})
}
#[test]
fn internally_typed_sprite_and_text_match_independent_expected_points() {
    let c = codec(&geometry(), &profile()).unwrap();
    let view = research_attempt::point_candidate(
        &surface(&detail(false), "support_weapons", 0, Stage::View).unwrap(),
        &c,
    )
    .unwrap();
    assert_eq!(view.client_pixel, [37, 70]);
    let research = research_attempt::point_candidate(
        &surface(&detail(true), "support_weapons", 0, Stage::Research).unwrap(),
        &c,
    )
    .unwrap();
    assert_eq!(research.client_pixel, [190, 35]);
    assert!(
        !research.input_authorized
            && !research.current_frame_verified
            && !research.effective_routing_verified
    );
}

#[test]
fn own_panel_slot_surface_requires_independent_binding_and_known_window_states() {
    let base = detail(false);
    let d = json!({"owned":true,"build_id":game::BUILD_ID,"slot":2,"active_mode_raw":5,
        "panel":{"materialized":true,"visible_predicate":true},
        "ordinary_tree":{"materialized":false,"visible_predicate":null},
        "descriptor":{"slot":2,"ordinary_selection_binding":{"binding_verified":true,
            "render_diagnostic":base["detail"]["render_diagnostic"]}}});
    let s = slot_surface(&d, 2).unwrap();
    let p =
        research_attempt::point_candidate(&s, &codec(&geometry(), &profile()).unwrap()).unwrap();
    assert_eq!(p.client_pixel, [37, 70]);
    for (path, value) in [
        ("/slot", json!(1)),
        ("/descriptor/slot", json!(1)),
        ("/active_mode_raw", json!(6)),
        ("/panel/materialized", json!(false)),
        ("/ordinary_tree/materialized", json!(true)),
        (
            "/descriptor/ordinary_selection_binding/binding_verified",
            json!(false),
        ),
    ] {
        let mut changed = d.clone();
        *changed.pointer_mut(path).unwrap() = value;
        assert!(slot_surface(&changed, 2).is_err(), "{path}");
    }
}
#[test]
fn own_key_slot_build_and_unknown_fields_fail_before_geometry() {
    for (path, value) in [
        ("/owned", json!(false)),
        ("/id", json!("basic_machine_tools")),
        ("/selected_slot", json!(1)),
        ("/build_id", json!("other")),
        ("/detail/id", json!("other")),
        ("/detail/distinct_bound_items", json!(2)),
        ("/detail/render_diagnostic/raw/visibility_flag", Value::Null),
        (
            "/detail/render_diagnostic/resource_type_encoding",
            json!("unknown"),
        ),
    ] {
        let mut d = detail(false);
        *d.pointer_mut(path).unwrap() = value;
        assert!(
            surface(&d, "support_weapons", 0, Stage::View).is_err(),
            "{path}"
        );
    }
    assert!(surface(&detail(true), "support_weapons", 0, Stage::View).is_err());
    assert!(surface(&detail(false), "support_weapons", 0, Stage::Research).is_err());
}
#[test]
fn callback_disabled_alias_and_unit_hierarchy_are_required() {
    for (path, value) in [
        ("/own_info_postcondition/ordinary_research_button/callback_binding_verified", json!(false)),
        ("/own_info_postcondition/ordinary_research_button/ordinary_alias_matches", Value::Null),
        ("/own_info_postcondition/ordinary_research_button/disabled_raw", json!(3)),
        ("/own_info_postcondition/ordinary_research_button/flags_raw", json!(4096)),
        ("/own_info_postcondition/ordinary_research_button/text_raw_diagnostic/parent_chain/graphics_root_closed", json!(false)),
        ("/own_info_postcondition/ordinary_research_button/text_raw_diagnostic/parent_chain/unit_cache_diagnostic/cache_consistent_under_observed_unit_branch", json!(false)),
    ] {
        let mut d = detail(true); *d.pointer_mut(path).unwrap() = value;
        assert!(surface(&d,"support_weapons",0,Stage::Research).is_err(),"{path}");
    }
}
#[test]
fn diagnostic_flags_never_replace_explicit_owner_profile_or_window_match() {
    let mut p = profile();
    p.reviewed_builtin_client_origin = false;
    let d = detail(true);
    let s = surface(&d, "support_weapons", 0, Stage::Research).unwrap();
    assert!(research_attempt::point_candidate(&s, &codec(&geometry(), &p).unwrap()).is_err());
    p = profile();
    p.window = 43;
    assert!(research_attempt::point_candidate(&s, &codec(&geometry(), &p).unwrap()).is_err());
    p = profile();
    p.os_binding_verified = false;
    assert!(research_attempt::point_candidate(&s, &codec(&geometry(), &p).unwrap()).is_err());
    let mut g = geometry();
    g["input_source"]["point_diagnostic"]["window_association"]["relative_mode_raw"] =
        json!([1, 0]);
    assert!(research_attempt::point_candidate(&s, &codec(&g, &profile()).unwrap()).is_err());
}
#[test]
fn stale_sizes_bad_pairs_and_relative_mode_refuse() {
    for (path, value) in [
        (
            "/input_source/point_diagnostic/window_association/window_dimensions",
            json!([513, 288]),
        ),
        (
            "/input_source/point_diagnostic/window_association/normal_absolute_mode",
            json!(false),
        ),
        ("/physical_dimensions", json!([512])),
        (
            "/input_source/point_diagnostic/getter_source_layout_verified",
            Value::Null,
        ),
    ] {
        let mut g = geometry();
        *g.pointer_mut(path).unwrap() = value;
        assert!(codec(&g, &profile()).is_err(), "{path}");
    }
}

fn category_detail() -> Value {
    let d = detail(false);
    json!({"owned":true,"build_id":game::BUILD_ID,"selected_slot":0,"category":{"folder_key":"industry_folder","selected_slot_empty":true,"binding_available":true,"control_type_encoding":"11CButtonDrag","raw_flags":2048,"sprite_diagnostic":d["detail"]["render_diagnostic"]}})
}
#[test]
fn category_reuses_sprite_and_codec_without_authority() {
    let d = category_detail();
    let p = research_attempt::point_candidate(
        &category_surface(&d, "industry_folder", 0).unwrap(),
        &codec(&geometry(), &profile()).unwrap(),
    )
    .unwrap();
    assert_eq!(p.client_pixel, [37, 70]);
    assert!(!p.input_authorized && !p.current_frame_verified && !p.effective_routing_verified);
}
#[test]
fn category_scope_type_emptiness_and_raw_flags_fail_closed() {
    for (path, value) in [
        ("/owned", json!(false)),
        ("/build_id", json!("other")),
        ("/selected_slot", json!(1)),
        ("/category/folder_key", json!("electronics_folder")),
        ("/category/binding_available", json!(false)),
        ("/category/selected_slot_empty", json!(false)),
        (
            "/category/control_type_encoding",
            json!("15CButtonStandard"),
        ),
        ("/category/raw_flags", json!(0)),
        ("/category/raw_flags", json!(6144)),
    ] {
        let mut d = category_detail();
        *d.pointer_mut(path).unwrap() = value;
        assert!(
            category_surface(&d, "industry_folder", 0).is_err(),
            "{path}"
        );
    }
}
#[test]
fn category_rejects_unknown_or_incomplete_hierarchy_and_mask_branch() {
    for(path,value)in[("/category/sprite_diagnostic/parent_chain/graphics_root_closed",json!(false)),("/category/sprite_diagnostic/parent_chain/status",json!("unknown_ancestor")),("/category/sprite_diagnostic/parent_chain/unit_cache_diagnostic/cache_consistent_under_observed_unit_branch",json!(false)),("/category/sprite_diagnostic/resource_cache/no_mask_rejection_branch",json!(false)),("/category/sprite_diagnostic/raw/visibility_flag",json!(0))]{let mut d=category_detail();*d.pointer_mut(path).unwrap()=value;assert!(category_surface(&d,"industry_folder",0).is_err(),"{path}");}
}
#[test]
fn category_owner_profile_is_not_inferred_from_sprite() {
    let d = category_detail();
    let s = category_surface(&d, "industry_folder", 0).unwrap();
    let mut p = profile();
    p.os_binding_verified = false;
    assert!(research_attempt::point_candidate(&s, &codec(&geometry(), &p).unwrap()).is_err());
    let mut p = profile();
    p.reviewed_builtin_client_origin = false;
    assert!(research_attempt::point_candidate(&s, &codec(&geometry(), &p).unwrap()).is_err());
}

#[test]
fn standard_category_point_requires_exact_nondisabled_enum_and_profile() {
    let mut d = category_detail();
    d["category"]["control_type_encoding"] = json!("15CButtonStandard");
    d["category"]["standard_state_raw"] = json!(0);
    d["category"]["standard_disabled_predicate"] = json!(false);
    let s = category_surface(&d, "industry_folder", 0).unwrap();
    let p =
        research_attempt::point_candidate(&s, &codec(&geometry(), &profile()).unwrap()).unwrap();
    assert_eq!(p.client_pixel, [37, 70]);
    assert!(!p.input_authorized);
    for state in [-1, 3, 4] {
        let mut v = d.clone();
        v["category"]["standard_state_raw"] = json!(state);
        assert!(category_surface(&v, "industry_folder", 0).is_err());
    }
    let mut v = d.clone();
    v["category"]["standard_disabled_predicate"] = json!(true);
    assert!(category_surface(&v, "industry_folder", 0).is_err());
    let mut p = profile();
    p.os_binding_verified = false;
    assert!(research_attempt::point_candidate(&s, &codec(&geometry(), &p).unwrap()).is_err());
}

fn geometry_for(physical: [u16; 2], logical: [u16; 2]) -> Value {
    let mut g = geometry();
    let ratio = |a: u16, b: u16| f64::from(f32::from(a) / f32::from(b));
    g["physical_dimensions"] = json!(physical);
    g["logical_dimensions"] = json!(logical);
    g["logical_per_physical"] = json!([
        ratio(logical[0], physical[0]),
        ratio(logical[1], physical[1])
    ]);
    g["physical_per_logical"] = json!([
        ratio(physical[0], logical[0]),
        ratio(physical[1], logical[1])
    ]);
    g["input_source"]["point_diagnostic"]["window_association"]["window_dimensions"] =
        json!(physical);
    g
}
fn detail_1080(research: bool) -> Value {
    let mut d = detail(research);
    d["detail"]["render_diagnostic"]["raw"]["physical_scaled_clip"] = json!([0, 0, 1920, 1080]);
    d
}
#[test]
fn builtin_profiles_pin_exact_reviewed_client_and_logical_pairs() {
    let p = BuiltinClient::Absolute512x288.owned(42);
    let q = profile();
    assert_eq!(
        (p.window, p.client, p.logical),
        (q.window, q.client, q.logical)
    );
    assert!(p.os_binding_verified && p.reviewed_builtin_client_origin);
    let p = BuiltinClient::Absolute1920x1080.owned(42);
    assert_eq!((p.client, p.logical), ([1920, 1080], [1920, 1080]));
    assert!(p.os_binding_verified && p.reviewed_builtin_client_origin);
}
#[test]
fn each_builtin_profile_accepts_only_its_own_fresh_geometry() {
    let s = surface(&detail_1080(false), "support_weapons", 0, Stage::View).unwrap();
    let small = BuiltinClient::Absolute512x288;
    let full = BuiltinClient::Absolute1920x1080;
    for (client, physical, logical, accepted) in [
        (small, [512, 288], [2560, 1440], true),
        (full, [1920, 1080], [1920, 1080], true),
        // Another reviewed profile's geometry is never borrowed.
        (small, [1920, 1080], [1920, 1080], false),
        (full, [512, 288], [2560, 1440], false),
        // Same client with an unreviewed GUI scale.
        (full, [1920, 1080], [3840, 2160], false),
        (full, [1920, 1080], [2560, 1440], false),
        (small, [512, 288], [1280, 720], false),
        // Unknown sizes keep refusing.
        (full, [1280, 720], [1280, 720], false),
        (full, [1920, 1200], [1920, 1200], false),
        (small, [1024, 576], [2560, 1440], false),
    ] {
        let result = codec(&geometry_for(physical, logical), &client.owned(42))
            .and_then(|c| research_attempt::point_candidate(&s, &c));
        match result {
            Ok(_) => assert!(accepted, "{client:?} {physical:?} {logical:?}"),
            Err(e) => {
                assert!(!accepted, "{client:?} {physical:?} {logical:?}");
                assert_eq!(e.code, "research_geometry_unavailable");
            }
        }
    }
}
#[test]
fn full_hd_points_come_from_renderer_geometry_not_rescaled_small_points() {
    let full = codec(
        &geometry_for([1920, 1080], [1920, 1080]),
        &BuiltinClient::Absolute1920x1080.owned(42),
    )
    .unwrap();
    let small = codec(&geometry(), &profile()).unwrap();
    let view = |c: &ClientCodec, d: &Value| {
        research_attempt::point_candidate(
            &surface(d, "support_weapons", 0, Stage::View).unwrap(),
            c,
        )
        .unwrap()
    };
    let (a, b) = (
        view(&small, &detail(false)),
        view(&full, &detail_1080(false)),
    );
    assert_eq!(a.client_pixel, [37, 70]);
    // The same named renderer center at logical (186, 349) maps 1:1.
    assert_eq!(b.ideal_logical, a.ideal_logical);
    assert_eq!(
        (b.client_pixel, b.quantized_logical),
        ([186, 349], [186, 349])
    );
    assert_eq!(b.expected_client, [1920, 1080]);
    // A blind 3.75x resize of the small pixel would land elsewhere.
    assert_ne!(
        b.client_pixel,
        a.client_pixel.map(|n| (n as f32 * 3.75).round() as i32)
    );
    let research = research_attempt::point_candidate(
        &surface(&detail_1080(true), "support_weapons", 0, Stage::Research).unwrap(),
        &full,
    )
    .unwrap();
    assert_eq!(research.client_pixel, [951, 173]);
    assert!(
        !research.input_authorized
            && !research.current_frame_verified
            && !research.effective_routing_verified
    );
}
#[test]
fn full_hd_profile_refuses_points_outside_its_client_or_clip() {
    let full = codec(
        &geometry_for([1920, 1080], [1920, 1080]),
        &BuiltinClient::Absolute1920x1080.owned(42),
    )
    .unwrap();
    let mut d = detail_1080(false);
    d["detail"]["render_diagnostic"]["raw"]["cached_world_translation"] = json!([1800, -241]);
    let e = research_attempt::point_candidate(
        &surface(&d, "support_weapons", 0, Stage::View).unwrap(),
        &full,
    )
    .unwrap_err();
    assert_eq!(
        e.message,
        "Point outside client: axis=0 ideal=2092 pixel=2092 client=1920x1080"
    );
    // The small fixture's clip is not widened by the larger profile.
    let e = research_attempt::point_candidate(
        &surface(&detail(false), "support_weapons", 0, Stage::View).unwrap(),
        &full,
    )
    .unwrap_err();
    assert_eq!(e.message, "observed physical clip excludes candidate");
}
