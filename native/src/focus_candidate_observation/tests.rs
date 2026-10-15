use super::*;
use serde_json::json;

struct UiMemory(std::collections::BTreeMap<u64, u8>);
impl UiMemory {
    fn put(&mut self, a: u64, b: &[u8]) {
        for (i, v) in b.iter().enumerate() {
            self.0.insert(a + i as u64, *v);
        }
    }
    fn zero(&mut self, a: u64, n: usize) {
        self.put(a, &vec![0; n]);
    }
    fn word(&mut self, a: u64, v: u64) {
        self.put(a, &v.to_le_bytes());
    }
    fn number(&mut self, a: u64, v: i32) {
        self.put(a, &v.to_le_bytes());
    }
    fn array(&mut self, a: u64, p: u64, n: i32) {
        self.zero(a, 16);
        self.word(a, p);
        self.number(a + 8, n);
        self.number(a + 12, n);
    }
    fn new(start: bool) -> Self {
        let mut m = Self(Default::default());
        m.number(0x303d8, 13);
        m.word(0x402f8, 0x60000);
        m.zero(0x60000, 0xf00);
        m.word(0x60000, 0x10000 + 0x4347d58);
        m.word(0x60098, 0x50000);
        m.put(0x600a5, &[8]);
        m.array(0x41180, 0x90000, i32::from(start));
        if start {
            m.word(0x90000, 0x80000);
            m.zero(0x80000, 0x800);
            m.word(0x80000, 0x10000 + 0x43023f8);
            m.word(0x807b8, 0x20000);
            m.word(0x807c0, 0x40000);
            m.word(0x807c8, 0x81000);
            m.zero(0x81000, 0xf00);
            m.word(0x81000, 0x10000 + 0x4347d58);
            m.word(0x81098, 0x50000);
            m.put(0x810a5, &[8]);
        }
        m.word(0x300f8, 0x70000);
        m.word(0x300f0, 0);
        m.zero(0x70000, 0xe30);
        m.word(0x70000, 0x10000 + 0x424de40);
        m.word(0x702f0, 0x20000);
        m.number(0x70e24, 6);
        m.array(0x70dc0, 0, 0);
        m
    }
    fn facts(&self) -> crate::focus_ui_facts::FocusUiFacts {
        let r = crate::memory::Reader::new(self);
        crate::focus_ui_facts::observe(&r, 0x10000, 0x20000, 0x30000, 0x40000, 0x50000, 1).unwrap()
    }
}
impl crate::memory::ReadMemory for UiMemory {
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        (0..n)
            .map(|i| {
                self.0
                    .get(&(a + i as u64))
                    .copied()
                    .ok_or_else(|| unavailable("Unmapped UI producer fixture"))
            })
            .collect()
    }
}
fn facts(start: bool) -> crate::focus_ui_facts::FocusUiFacts {
    UiMemory::new(start).facts()
}
fn render() -> Value {
    json!({"field_layout_verified":true,"render_type_encoding":"7CSprite","resource_type_encoding":"11CSpriteType","raw":{"visibility_flag":1,"cached_world_translation":[-106,-241],"pivot_dimensions":[512,288],"local_scalar":1,"physical_scaled_clip":[0,0,512,288]},"resource_cache":{"cache_equivalent":true,"no_mask_rejection_branch":true,"texture_dimensions":[72,72],"grid":[1,1],"orientation_raw":0},"parent_chain":{"graphics_root_closed":true,"status":"closed_to_owned_graphics_root","chain":[{"role":"owned_graphics_root_1f0"}],"unit_cache_diagnostic":{"cache_consistent_under_observed_unit_branch":true}}})
}
fn detail(start: bool) -> Value {
    let binding = json!({"selected_own_key":"army_effort","available":true,"callback_route_verified":true,"renderer_control_join_verified":true,"render_diagnostic":render()});
    let details = if start {
        json!([{"selected_key":"army_effort","window":{"materialized":true,"conservative_container_visibility":true,"start":{"materialized":true,"disabled":false,"disabled_state_raw":0,"flags74_raw":2048,"ordinary_binding":binding}}}])
    } else {
        json!([])
    };
    json!({"owned":true,"build_id":game::BUILD_ID,"command_observation":{"requested_key":"army_effort","same_reader_clock_context":true,"active_key":null},"focuses":[{"key":"army_effort"}],"details":details,"node_association":{"requested_key":"army_effort","materialized":true,"controls":[{"name":"symbol","type_encoding":"15CButtonStandard","binding_verified":true,"disabled_state3":false,"disabled_state_raw":0,"flags74_raw":2048,"renderer_control_join_verified":true,"render_diagnostic":render()}]}})
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
    json!({"build_id":game::BUILD_ID,"physical_dimensions":[512,288],"logical_dimensions":[2560,1440],"logical_per_physical":[5.,5.],"physical_per_logical":[0.2,0.2],"input_source":{"point_diagnostic":{"getter_source_layout_verified":true,"window_association":{"association_available":true,"normal_absolute_mode":true,"graphics_dimensions_match":true,"native_window_id":42,"window_dimensions":[512,288],"relative_mode_raw":[0,0]}}}})
}
fn text_render() -> Value {
    json!({"render_type_encoding":"11CTextSprite","resource_layout_verified":true,"raw_fields_available":true,
        "cache_translation_raw":[634,-47],"anchor_raw":[512,288],"scalar_raw":1,
        "renderer_width_raw":123,"renderer_height_raw":36,"resource_count_raw":1,"hit_width_divisor_raw":1,"transform_110_raw":0,
        "parent_chain":render()["parent_chain"],"hit_projection_available":false,"input_authorized":false})
}
fn frame_render() -> Value {
    json!({"render_type_encoding":"20CFrameAnimatedSprite","field_layout_verified":true,"resource_layout_verified":true,
        "resource_alias_matches":true,"resource_type_encoding":"24CFrameAnimatedSpriteType",
        "resource_cache":{"getter_cache_equivalent":true,"frame_count_raw":3,"grid_other_raw":1,"orientation_raw":0,"texture_width_raw":217,"texture_height_raw":72},
        "frame_raw_geometry":{"cache_translation_raw":[-106,-241],"anchor_raw":[512,288],"scalar_raw":1,"no_mask_rejection_branch":true,"parent_chain":render()["parent_chain"]},
        "current_frame_verified":false,"effective_routing_verified":false,"input_authorized":false})
}
fn with_render(r: Value, start: bool) -> Result<PointCandidate> {
    let mut d = detail(start);
    if start {
        d["details"][0]["window"]["start"]["ordinary_binding"]["render_diagnostic"] = r;
    } else {
        d["node_association"]["controls"][0]["render_diagnostic"] = r;
    }
    let s = surface(
        &d,
        "army_effort",
        if start { Stage::Start } else { Stage::Node },
        Some(&facts(start)),
    )?;
    research_attempt::point_candidate(&s, &codec(&geometry(), &profile())?)
}
#[test]
fn exact_zero_angle_text_and_frame_bindings_prepare_conditional_points_for_both_stages() {
    for start in [false, true] {
        let p = with_render(text_render(), start).unwrap();
        assert_eq!(p.ideal_logical, [951, 173]);
        assert_eq!(p.client_pixel, [190, 35]);
        assert_eq!(p.quantized_logical, [950, 175]);
        assert!(!p.current_frame_verified && !p.effective_routing_verified && !p.input_authorized);
        let p = with_render(frame_render(), start).unwrap();
        assert_eq!(p.client_pixel, [37, 70]);
        assert_eq!(p.ideal_logical, [186, 349]); // fractional grid bounds, integer dimension origin
        assert!(!p.current_frame_verified && !p.effective_routing_verified && !p.input_authorized);
    }
}
#[test]
fn text_rotation_divisor_unsettled_cache_hierarchy_and_quantization_never_fall_back_to_sprite() {
    for (path, value) in [
        ("/transform_110_raw", json!(0.01)),
        ("/hit_width_divisor_raw", json!(2)),
        ("/raw_fields_available", json!(false)),
        ("/resource_layout_verified", json!(false)),
        ("/scalar_raw", json!(2)),
        ("/cache_translation_raw/0", json!(1000000)),
        (
            "/parent_chain/unit_cache_diagnostic/cache_consistent_under_observed_unit_branch",
            json!(false),
        ),
        ("/renderer_width_raw", json!(2)),
    ] {
        let mut r = text_render();
        *r.pointer_mut(path).unwrap() = value;
        assert!(with_render(r, true).is_err(), "{path}");
    }
}
#[test]
fn frame_alias_resource_mask_orientation_and_cache_refuse_without_text_unwrapping() {
    for (path, value) in [
        ("/resource_alias_matches", json!(false)),
        ("/resource_type_encoding", json!("15CTextSpriteType")),
        ("/resource_cache/orientation_raw", json!(0x100)),
        ("/resource_cache/getter_cache_equivalent", json!(false)),
        ("/frame_raw_geometry/no_mask_rejection_branch", json!(false)),
        ("/resource_cache/frame_count_raw", json!(0)),
        (
            "/frame_raw_geometry/parent_chain/graphics_root_closed",
            json!(false),
        ),
    ] {
        let mut r = frame_render();
        *r.pointer_mut(path).unwrap() = value;
        assert!(with_render(r, true).is_err(), "{path}");
    }
    let mut r = frame_render();
    r["render_type_encoding"] = json!("14CInstantSprite");
    assert!(with_render(r, true).is_err());
}
#[test]
fn exact_bound_sprite_node_and_start_reuse_independent_point() {
    for stage in [Stage::Node, Stage::Start] {
        let d = detail(matches!(stage, Stage::Start));
        let s = surface(
            &d,
            "army_effort",
            stage,
            Some(&facts(matches!(stage, Stage::Start))),
        )
        .unwrap();
        let p = research_attempt::point_candidate(&s, &codec(&geometry(), &profile()).unwrap())
            .unwrap();
        assert_eq!(p.client_pixel, [37, 70]);
        assert!(!p.input_authorized && !p.current_frame_verified && !p.effective_routing_verified);
    }
}
#[test]
fn unknown_scoped_ui_visibility_mode_and_profile_refuse() {
    assert!(surface(&detail(false), "army_effort", Stage::Node, None).is_err());
    for (a, bytes) in [
        (0x303d8, 23i32.to_le_bytes().to_vec()),
        (0x600a5, vec![0]),
        (0x60eb0, 2i32.to_le_bytes().to_vec()),
        (0x70e20, 2i32.to_le_bytes().to_vec()),
    ] {
        let mut m = UiMemory::new(false);
        m.put(a, &bytes);
        let u = m.facts();
        assert!(surface(&detail(false), "army_effort", Stage::Node, Some(&u)).is_err());
    }
    let s = surface(
        &detail(false),
        "army_effort",
        Stage::Node,
        Some(&facts(false)),
    )
    .unwrap();
    let mut p = profile();
    p.os_binding_verified = false;
    assert!(research_attempt::point_candidate(&s, &codec(&geometry(), &p).unwrap()).is_err());
}
#[test]
fn node_missing_binding_disabled_text_or_unknown_hierarchy_refuse() {
    for (path, v) in [
        ("/owned", json!(false)),
        ("/focuses/0/key", json!("navy_effort")),
        (
            "/node_association/controls/0/binding_verified",
            json!(false),
        ),
        ("/node_association/controls/0/disabled_state_raw", json!(3)),
        (
            "/node_association/controls/0/render_diagnostic/render_type_encoding",
            json!("11CTextSprite"),
        ),
        (
            "/node_association/controls/0/render_diagnostic/parent_chain/status",
            json!("unknown_ancestor"),
        ),
        (
            "/node_association/controls/0/render_diagnostic/raw/visibility_flag",
            json!(0),
        ),
    ] {
        let mut d = detail(false);
        *d.pointer_mut(path).unwrap() = v;
        assert!(
            surface(&d, "army_effort", Stage::Node, Some(&facts(false))).is_err(),
            "{path}"
        );
    }
}
#[test]
fn start_wrong_selection_callback_visibility_and_unknown_state_refuse() {
    for (path, v) in [
        ("/details/0/selected_key", json!("navy_effort")),
        (
            "/details/0/window/conservative_container_visibility",
            Value::Null,
        ),
        ("/details/0/window/start/disabled_state_raw", json!(4)),
        (
            "/details/0/window/start/ordinary_binding/callback_route_verified",
            json!(false),
        ),
        (
            "/details/0/window/start/ordinary_binding/renderer_control_join_verified",
            json!(false),
        ),
    ] {
        let mut d = detail(true);
        *d.pointer_mut(path).unwrap() = v;
        assert!(
            surface(&d, "army_effort", Stage::Start, Some(&facts(true))).is_err(),
            "{path}"
        );
    }
}
#[test]
fn ambiguity_and_visible_detail_block_node() {
    let mut d = detail(false);
    let c = d["node_association"]["controls"][0].clone();
    d["node_association"]["controls"]
        .as_array_mut()
        .unwrap()
        .push(c);
    assert!(surface(&d, "army_effort", Stage::Node, Some(&facts(false))).is_err());
    assert!(surface(
        &detail(true),
        "army_effort",
        Stage::Node,
        Some(&facts(false))
    )
    .is_err());
    let mut d = detail(true);
    let x = d["details"][0].clone();
    d["details"].as_array_mut().unwrap().push(x);
    assert!(surface(&d, "army_effort", Stage::Start, Some(&facts(true))).is_err());
}

#[test]
fn invalid_key_refuses_before_process() {
    struct Spy(std::cell::Cell<usize>);
    impl crate::memory::ReadMemory for Spy {
        fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
            self.0.set(self.0.get() + 1);
            Err(unavailable("Unexpected read"))
        }
    }
    impl Process for Spy {
        fn image_base(&self) -> u64 {
            self.0.set(self.0.get() + 1);
            0x10000
        }
        fn check(&self) -> Result<()> {
            self.0.set(self.0.get() + 1);
            Err(unavailable("Unexpected check"))
        }
    }
    let s = Spy(std::cell::Cell::new(0));
    assert!(observe(&s, "../invalid", Stage::Node, &profile()).is_err());
    assert_eq!(s.0.get(), 0);
}
#[test]
fn positively_observed_own_info_and_detail_animation_blockers_refuse() {
    for (visible, animation) in [(8, 0), (0, 2)] {
        let mut m = UiMemory::new(false);
        m.array(0x70dc0, 0xa0000, 1);
        m.word(0xa0000, 0xa1000);
        m.zero(0xa1000, 0xb00);
        m.word(0xa1000, 0x10000 + 0x430bbf0);
        m.word(0xa1a78, 0x20000);
        m.word(0xa1a80, 0x70000);
        m.word(0xa1a88, 0xa2000);
        m.zero(0xa2000, 0xf00);
        m.word(0xa2000, 0x10000 + 0x4347d58);
        m.word(0xa2098, 0x50000);
        m.put(0xa20a5, &[visible]);
        m.number(0xa2eb0, animation);
        let u = m.facts();
        assert!(surface(&detail(false), "army_effort", Stage::Node, Some(&u)).is_err());
    }
    let mut m = UiMemory::new(true);
    m.number(0x81ef8, 2);
    let u = m.facts();
    assert!(surface(&detail(true), "army_effort", Stage::Start, Some(&u)).is_err());
    let mut m = UiMemory::new(false);
    m.word(0x402f8, 0);
    let u = m.facts();
    assert!(surface(&detail(false), "army_effort", Stage::Node, Some(&u)).is_err());
}
fn retained_other(m: &mut UiMemory, start: bool, visible: u8, animation: i32) {
    let n = if start { 2 } else { 1 };
    m.array(0x41180, 0x90000, n);
    let index = u64::from(start);
    m.word(0x90000 + index * 8, 0xb0000);
    m.zero(0xb0000, 0x800);
    m.word(0xb0000, 0x10000 + 0x43023f8);
    m.word(0xb07b8, 0x20000);
    m.word(0xb07c0, 0x40000);
    m.word(0xb07c8, 0xb1000);
    m.zero(0xb1000, 0xf00);
    m.word(0xb1000, 0x10000 + 0x4347d58);
    m.word(0xb1098, 0x50000);
    m.put(0xb10a5, &[visible]);
    m.number(0xb1eb0, animation);
}
#[test]
fn hidden_nonmatching_retained_rows_do_not_require_omitted_payload() {
    for stage in [Stage::Node, Stage::Start] {
        let start = matches!(stage, Stage::Start);
        let mut m = UiMemory::new(start);
        retained_other(&mut m, start, 0, 0);
        let u = m.facts();
        let mut d = detail(start);
        d["details"]
            .as_array_mut()
            .unwrap()
            .push(json!({"selected_key":"other_focus","window_payload_read":false}));
        assert!(surface(&d, "army_effort", stage, Some(&u)).is_ok());
    }
}
#[test]
fn nonmatching_visible_or_animating_retained_rows_still_block() {
    for stage in [Stage::Node, Stage::Start] {
        for (v, a) in [(8, 0), (0, 2)] {
            let start = matches!(stage, Stage::Start);
            let mut m = UiMemory::new(start);
            retained_other(&mut m, start, v, a);
            let u = m.facts();
            let mut d = detail(start);
            d["details"]
                .as_array_mut()
                .unwrap()
                .push(json!({"selected_key":"other_focus","window_payload_read":false}));
            assert!(surface(&d, "army_effort", stage, Some(&u)).is_err());
        }
    }
}
#[test]
fn unknown_nonmatching_producer_window_still_refuses() {
    for stage in [Stage::Node, Stage::Start] {
        let start = matches!(stage, Stage::Start);
        let mut m = UiMemory::new(start);
        retained_other(&mut m, start, 0, 0);
        m.word(0xb07c8, 0);
        let u = m.facts();
        assert_eq!(u.focus_detail_local_counts(), None);
        let mut d = detail(start);
        d["details"]
            .as_array_mut()
            .unwrap()
            .push(json!({"selected_key":"other_focus","window_payload_read":false}));
        assert!(surface(&d, "army_effort", stage, Some(&u)).is_err());
    }
}

fn start_refusal(binding: Value) -> Error {
    let mut d = detail(true);
    d["details"][0]["window"]["start"]["ordinary_binding"] = binding;
    surface(&d, "army_effort", Stage::Start, Some(&facts(true))).unwrap_err()
}
#[test]
fn start_refusal_reports_known_observed_reason_flags_and_type_without_relaxing_guard() {
    let error = start_refusal(json!({"selected_own_key":"army_effort","available":false,
        "reason":"event_variant_unavailable","callback_route_verified":false,
        "renderer_control_join_verified":false,"renderer_type_encoding":"11CTextSprite"}));
    assert_eq!(error.code, "focus_geometry_unavailable");
    for field in [
        "Exact own selected start callback required",
        "reason=event_variant_unavailable",
        "available=false",
        "callback_route_verified=false",
        "renderer_control_join_verified=false",
        "renderer_type_encoding=11CTextSprite",
        "selected_key_matches=true",
    ] {
        assert!(error.message.contains(field), "{field}");
    }
    // A positively observed success reason cannot override any existing guard.
    let error = start_refusal(json!({"selected_own_key":"army_effort","available":true,
        "reason":"own_selected_event_start_and_noop_listener_verified",
        "callback_route_verified":false,"renderer_control_join_verified":true,
        "renderer_type_encoding":"7CSprite"}));
    assert!(error.message.contains("callback_route_verified=false"));
    assert_eq!(error.code, "focus_geometry_unavailable");
}
#[test]
fn start_refusal_missing_fields_and_non_boolean_values_are_unknown() {
    for binding in [
        json!({}),
        Value::Null,
        json!({"reason":null,"available":"true",
        "callback_route_verified":1,"renderer_control_join_verified":[],
        "renderer_type_encoding":false,"selected_own_key":123}),
    ] {
        let error = start_refusal(binding);
        assert_eq!(error.code, "focus_geometry_unavailable");
        for field in [
            "reason=unknown",
            "available=unknown",
            "callback_route_verified=unknown",
            "renderer_control_join_verified=unknown",
            "renderer_type_encoding=unknown",
            "selected_key_matches=unknown",
        ] {
            assert!(error.message.contains(field), "{field}");
        }
    }
}
#[test]
fn start_refusal_does_not_echo_unknown_private_long_or_control_character_strings() {
    for secret in [
        "opaque_unrecognized_value",
        "renderer_0x12345678",
        "event_variant_unavailable\nprivate-data",
        &"x".repeat(4096),
    ] {
        let error = start_refusal(json!({"reason":secret,"renderer_type_encoding":secret,
            "selected_own_key":secret,"available":false,"callback_route_verified":false,
            "renderer_control_join_verified":false,"address":"0x12345678"}));
        assert_eq!(error.code, "focus_geometry_unavailable");
        assert!(error.message.contains("reason=unknown"));
        assert!(error.message.contains("renderer_type_encoding=unknown"));
        assert!(error.message.contains("selected_key_matches=false"));
        assert!(!error.message.contains(secret));
        assert!(!error.message.contains("0x12345678"));
        assert!(error.message.len() < 400);
    }
}
#[test]
fn start_refusal_default_public_reason_is_retained_and_success_path_is_unchanged() {
    let error = start_refusal(json!({"selected_own_key":"army_effort","available":false,
        "reason":"event_binding_unavailable","callback_route_verified":false,
        "renderer_control_join_verified":false,"renderer_type_encoding":null}));
    assert!(error.message.contains("reason=event_binding_unavailable"));
    assert!(surface(
        &detail(true),
        "army_effort",
        Stage::Start,
        Some(&facts(true))
    )
    .is_ok());
}
