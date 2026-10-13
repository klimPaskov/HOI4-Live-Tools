use super::*;
use serde_json::json;
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
    json!({"graphics_root_closed":true,"status":"closed_to_owned_graphics_root",
      "unit_cache_diagnostic":{"cache_consistent_under_observed_unit_branch":true}})
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
