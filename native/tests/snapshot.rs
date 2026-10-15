use hoi4_live_native::{
    error::{Error, Result},
    game, gui,
    memory::ReadMemory,
    platform::Process,
    research, research_folders, research_ui, technology,
};
use std::{cell::Cell, collections::BTreeMap};

// Own-politics diagnostics: synthetic named registries and generated observer
// captures, with ordinary overview/position fixtures reused independently.
const POLITICS: u64 = 0x210000;
const FOCUS: u64 = 0x220000;
const POLITICS_WINDOW: u64 = 0x230000;
const OPENER_CAPTURE: u64 = 0x241000;
const OPENER_LINK: u64 = 0x242000;
const CHILD_NAMES: u64 = 0x243000;
const CHILD_VALUES: u64 = 0x244000;
const OPENER_NAMES: u64 = 0x245000;
const OPENER_NAME_TEXT: u64 = 0x246000;
const OPENER_VALUES: u64 = 0x247000;

fn politics_focus_fixture() -> Fixture {
    let mut f = owned_node_fixture();
    let b = 0x10000;
    f.number(0x20008, 1);
    f.word(b + 0x42c9a88 + 0xc0, b + 0x20811a0);
    f.number(0x903d8, 12);
    f.word(0x90128, POLITICS);
    f.word(0x90130, FOCUS);
    f.region(POLITICS, 0x600);
    f.word(POLITICS, b + 0x42546e8);
    f.word(POLITICS + 0x38, 0x90000);
    f.word(POLITICS + 0x2f0, 0x32000);
    f.word(POLITICS + 0x2f8, POLITICS_WINDOW);
    f.region(FOCUS, 0x1200);
    f.word(FOCUS, b + 0x4249c70);
    f.word(FOCUS + 0x2f0, 0x32000);
    f.word(FOCUS + 0x11d8, 0x20000);
    f.region(POLITICS_WINDOW, 0xf00);
    f.word(POLITICS_WINDOW, b + 0x4347d58);
    f.word(POLITICS_WINDOW + 0x98, 0x70000);
    f.put(POLITICS_WINDOW + 0xa5, &[8]);
    f.array(POLITICS_WINDOW + 0xbb0, CHILD_VALUES, 1);
    f.word(CHILD_VALUES, 0xd4000);
    f.array(POLITICS_WINDOW + 0xbc8, CHILD_NAMES, 1);
    f.string(CHILD_NAMES, "active_goal");
    f.put(0xd40a5, &[8]);
    f.number(0xd4eb0, 0);
    f.number(0xd4ef8, 0);
    f.array(0xd49a0, OPENER_VALUES, 1);
    f.word(OPENER_VALUES, 0xd5000);
    f.array(0xd49b8, OPENER_NAMES, 1);
    let name = b"add_national_goal_button";
    f.region(OPENER_NAMES, 32);
    f.word(OPENER_NAMES, OPENER_NAME_TEXT);
    f.word(OPENER_NAMES + 8, name.len() as u64);
    f.word(OPENER_NAMES + 16, name.len() as u64);
    f.put(OPENER_NAME_TEXT, name);
    f.word(0xd5000, b + 0x43470c8);
    f.word(0xd5080, b + 0x4347488);
    f.word(0xd5088, OPENER_LINK);
    f.word(0xd5090, OPENER_LINK);
    f.number(0xd5098, 1);
    f.word(0xd50a0, 0);
    f.number(0xd50ac, 0);
    f.put(0xd5074, &0x8100_0000u32.to_le_bytes());
    f.word(0xd50e8, 0xf0000);
    f.region(OPENER_LINK, 32);
    f.word(OPENER_LINK, POLITICS + 0x300);
    f.word(POLITICS + 0x300, b + 0x4254880);
    f.word(POLITICS + 0x308, OPENER_CAPTURE);
    f.word(POLITICS + 0x318, b + 0x1693160);
    f.word(POLITICS + 0x320, b + 0x1693130);
    f.region(OPENER_CAPTURE, 24);
    f.word(OPENER_CAPTURE, b + 0x1686aa0);
    f.word(OPENER_CAPTURE + 0x10, POLITICS);
    // Standard association cannot depend on Drag renderer110 or tech data+8.
    for a in (0xd5008..0xd5010).chain(0xd5110..0xd5118) {
        f.data.remove(&a);
    }
    f
}

fn politics_forbid(f: &mut Fixture, start: u64, size: usize) {
    for a in start..start + size as u64 {
        f.data.remove(&a);
    }
}

fn politics_typed_geometry_fixture(text: bool) -> Fixture {
    let mut f = politics_focus_fixture();
    let b = 0x10000;
    // Retain the independently constructed owner/hierarchy fields, only expand
    // the exact renderer's observed object storage for Text-specific fields.
    f.region(0xf0180, 0x180);
    for (object, table, info, name, string) in [
        (
            0xf0000,
            if text { 0x43593d8 } else { 0x434b370 },
            0x435a800,
            if text {
                "11CTextSprite"
            } else {
                "20CFrameAnimatedSprite"
            },
            0x3516000,
        ),
        (
            0xf1000,
            if text { 0x4359740 } else { 0x434b668 },
            0x435a900,
            if text {
                "15CTextSpriteType"
            } else {
                "24CFrameAnimatedSpriteType"
            },
            0x3516100,
        ),
    ] {
        f.word(object, b + table);
        f.word(b + table - 16, 0);
        f.word(b + table - 8, b + info);
        f.region(b + info, 16);
        f.word(b + info + 8, b + string);
        f.region(b + string, 128);
        f.put(b + string, name.as_bytes());
    }
    if text {
        f.number(0xf108c, 1);
        f.number(0xf0228, 300);
        f.number(0xf022c, 60);
        f.number(0xf0238, 1);
        f.put(0xf0110, &0.0f32.to_le_bytes());
    } else {
        f.word(0xf01b0, 0xf1000); // separate hit pointer must equal dimension168
    }
    f.unit_chain();
    f
}
#[test]
fn politics_focus_exact_text_frame_renderer_inheritance_follows_named_own_callback() {
    for text in [false, true] {
        let f = politics_typed_geometry_fixture(text);
        let (value, _) = hoi4_live_native::politics_focus::owned_snapshot(&f, true).unwrap();
        assert_eq!(value["renderer_control_join_verified"], true);
        assert_eq!(value["callback_route_verified"], true);
        let g = &value["render_diagnostic"];
        if text {
            assert_eq!(g["raw_fields_available"], true);
            assert_eq!(g["renderer_width_raw"], 300);
            assert_eq!(g["hit_width_divisor_raw"], 1);
            assert_eq!(
                g["parent_chain"]["unit_cache_diagnostic"]
                    ["cache_consistent_under_observed_unit_branch"],
                true
            );
            f.assert_not_read(&[0xf01b0, 0xf1090, 0xf1094, 0xf10a4]);
        } else {
            assert_eq!(g["field_layout_verified"], true);
            assert_eq!(g["resource_alias_matches"], true);
            assert_eq!(
                g["frame_raw_geometry"]["parent_chain"]["unit_cache_diagnostic"]
                    ["cache_consistent_under_observed_unit_branch"],
                true
            );
            f.assert_not_read(&[0xf0228, 0xf0238]);
        }
        assert_eq!(g["current_frame_verified"], false);
        assert_eq!(g["input_authorized"], false);
        f.assert_not_read(&[0xdead0000, 0xd5110, 0xd5008]);
    }
}
#[test]
fn politics_focus_text_frame_foreign_owner_and_callback_refuse_before_renderer_resource() {
    for text in [false, true] {
        let mut f = politics_typed_geometry_fixture(text);
        f.word(0xf0138, 0xdead0000);
        assert_eq!(
            hoi4_live_native::politics_focus::owned_snapshot(&f, true)
                .unwrap_err()
                .code,
            "invalid_gui_owner"
        );
        f.assert_not_read(&[0xf0168, 0xf01b0, 0xf0228, 0xf1090, 0xdead0000]);
        let mut f = politics_typed_geometry_fixture(text);
        f.word(POLITICS + 0x320, 0x10000 + 0x1693138);
        let (value, _) = hoi4_live_native::politics_focus::owned_snapshot(&f, true).unwrap();
        assert_eq!(value["callback_route_verified"], false);
        f.assert_not_read(&[0xd50e8, 0xf0168, 0xf01b0, 0xf0228]);
    }
}

#[test]
fn politics_focus_positive_owned_route_is_metadata_not_input_authority() {
    let f = politics_focus_fixture();
    let (v, context) = hoi4_live_native::politics_focus::owned_snapshot(&f, true).unwrap();
    assert_eq!(context.campaign().player, "GER");
    assert_eq!(context.mode_raw(), 12);
    assert_eq!(
        context.anchors(),
        [0x20000, 0x90000, 0x70000, POLITICS, FOCUS, POLITICS_WINDOW]
    );
    assert_eq!(v["callback_route_verified"], true);
    assert_eq!(v["target_managed_mode"], 13);
    assert_eq!(v["flags_raw"], 0x8100_0000u32);
    assert_eq!(
        v["render_diagnostic"]["parent_chain"]["status"],
        "closed_to_owned_graphics_root"
    );
    assert_eq!(
        v["render_diagnostic"]["resource_cache"]["diagnostic_dimensions"],
        serde_json::json!([300, 60])
    );
    for flag in [
        "input_authorized",
        "current_frame_verified",
        "effective_routing_verified",
        "global_modal_clear_verified",
        "client_rectangle_available",
    ] {
        assert_eq!(v[flag], false, "{flag}");
    }
    assert_eq!(f.checks.get(), 2);
    assert!(!v.to_string().contains(&POLITICS.to_string()));
    assert!(!v.to_string().contains(&OPENER_CAPTURE.to_string()));
}

#[test]
fn politics_focus_no_geometry_and_disabled_state_never_claim_eligibility() {
    let mut f = politics_focus_fixture();
    f.number(0xd50ac, 3);
    politics_forbid(&mut f, 0xd50e8, 8);
    let (v, _) = hoi4_live_native::politics_focus::owned_snapshot(&f, false).unwrap();
    assert_eq!(v["available"], true);
    assert_eq!(v["disabled_raw"], 3);
    assert_eq!(v["input_authorized"], false);
    assert!(v.get("render_diagnostic").is_none());
}

#[test]
fn politics_focus_inactive_hidden_or_null_root_skips_named_payload() {
    for case in 0..5 {
        let mut f = politics_focus_fixture();
        match case {
            0 => f.number(0x903d8, 23),
            1 => f.number(0x903d8, 13),
            2 => f.put(POLITICS_WINDOW + 0xa5, &[0]),
            3 => f.number(POLITICS_WINDOW + 0xef8, 2),
            _ => f.word(POLITICS + 0x2f8, 0),
        }
        politics_forbid(&mut f, POLITICS_WINDOW + 0xbb0, 40);
        politics_forbid(&mut f, 0xd50e8, 8);
        let (v, _) = hoi4_live_native::politics_focus::owned_snapshot(&f, true).unwrap();
        assert_eq!(v["available"], false, "case{case}");
        assert!(!f.tracked_reads.borrow().contains_key(&CHILD_VALUES));
    }
}

#[test]
fn politics_focus_foreign_joins_refuse_before_names_or_renderer() {
    for (address, value, code, forbidden) in [
        (
            POLITICS + 0x2f0,
            0x32008,
            "invalid_gui_owner",
            POLITICS_WINDOW + 0xbb0,
        ),
        (
            POLITICS + 0x38,
            0x90008,
            "invalid_gui_owner",
            POLITICS_WINDOW + 0xbb0,
        ),
        (
            FOCUS + 0x2f0,
            0x32008,
            "foreign_focus_scope",
            POLITICS_WINDOW + 0xbb0,
        ),
        (
            FOCUS + 0x11d8,
            0xdead0000,
            "foreign_focus_scope",
            POLITICS_WINDOW + 0xbb0,
        ),
        (
            POLITICS_WINDOW + 0x98,
            0xdead0000,
            "invalid_gui_owner",
            POLITICS_WINDOW + 0xbb0,
        ),
        (0xd4098, 0xdead0000, "invalid_gui_owner", 0xd49a0),
        (0xd5068, 0xdead0000, "invalid_gui_owner", 0xd50e8),
        (
            OPENER_CAPTURE + 0x10,
            POLITICS + 8,
            "invalid_gui_owner",
            0xd50e8,
        ),
    ] {
        let mut f = politics_focus_fixture();
        f.word(address, value);
        politics_forbid(&mut f, forbidden, 8);
        assert_eq!(
            hoi4_live_native::politics_focus::owned_snapshot(&f, true)
                .unwrap_err()
                .code,
            code,
            "field{address:x}"
        );
        assert!(!f.tracked_reads.borrow().contains_key(&forbidden));
    }
}

#[test]
fn politics_focus_unverified_callback_variants_skip_renderer() {
    for (address, value) in [
        (0xd5080, 0x10000 + 0x4346ea8),
        (0xd50a0, OPENER_CAPTURE),
        (POLITICS + 0x300, 0x10000 + 0x424e0a8),
        (POLITICS + 0x318, 0x10000 + 0x1693168),
        (POLITICS + 0x320, 0x10000 + 0x1693138),
        (POLITICS + 0x328, OPENER_CAPTURE),
        (OPENER_CAPTURE, 0x10000 + 0x1686aa8),
        (OPENER_CAPTURE + 8, 8),
    ] {
        let mut f = politics_focus_fixture();
        f.word(address, value);
        politics_forbid(&mut f, 0xd50e8, 8);
        let (v, _) = hoi4_live_native::politics_focus::owned_snapshot(&f, true).unwrap();
        assert_eq!(
            v["reason"], "callback_binding_unavailable",
            "field{address:x}"
        );
        assert!(!f.tracked_reads.borrow().contains_key(&0xd50e8));
    }
}

#[test]
fn politics_focus_bad_lists_and_registry_bounds_stop_before_geometry() {
    for (address, value, expected) in [
        (0xd5098, -1, "invalid_gui_binding"),
        (0xd5098, 17, "invalid_gui_binding"),
        (POLITICS_WINDOW + 0xbb8, 257, "invalid_array"),
        (POLITICS_WINDOW + 0xbd0, 257, "invalid_gui_registry"),
        (POLITICS_WINDOW + 0xbd4, 0, "invalid_gui_registry"),
    ] {
        let mut f = politics_focus_fixture();
        f.number(address, value);
        politics_forbid(&mut f, 0xd50e8, 8);
        assert_eq!(
            hoi4_live_native::politics_focus::owned_snapshot(&f, true)
                .unwrap_err()
                .code,
            expected,
            "field{address:x}"
        );
        assert!(!f.tracked_reads.borrow().contains_key(&0xd50e8));
    }
    for address in [OPENER_LINK + 8, OPENER_LINK + 0x10, 0xd5090] {
        let mut f = politics_focus_fixture();
        f.word(address, OPENER_LINK);
        if address == 0xd5090 {
            f.word(address, OPENER_LINK + 8);
        }
        assert_eq!(
            hoi4_live_native::politics_focus::owned_snapshot(&f, false)
                .unwrap_err()
                .code,
            "invalid_gui_binding"
        );
    }
}

#[test]
fn politics_focus_complete_named_membership_and_no_global_fallback() {
    let mut f = politics_focus_fixture();
    f.array(POLITICS_WINDOW + 0xbb0, CHILD_VALUES, 2);
    f.word(CHILD_VALUES + 8, 0xdead0000);
    f.array(POLITICS_WINDOW + 0xbc8, CHILD_NAMES, 2);
    f.string(CHILD_NAMES + 32, "active_goal");
    assert_eq!(
        hoi4_live_native::politics_focus::owned_snapshot(&f, true)
            .unwrap_err()
            .code,
        "invalid_gui_registry"
    );

    let mut f = politics_focus_fixture();
    f.string(CHILD_NAMES, "unknown_child");
    politics_forbid(&mut f, 0xd49a0, 16);
    let (v, _) = hoi4_live_native::politics_focus::owned_snapshot(&f, true).unwrap();
    assert_eq!(v["reason"], "named_container_unavailable");
    // No guessed registry fallback or button-pair reuse for child containers.
    assert!(!f
        .tracked_reads
        .borrow()
        .contains_key(&(POLITICS_WINDOW + 0x9a0)));

    let mut f = politics_focus_fixture();
    f.array(0xd49a0, OPENER_VALUES, 2);
    f.word(OPENER_VALUES + 8, 0xdead0000);
    f.array(0xd49b8, OPENER_NAMES, 2);
    f.region(OPENER_NAMES + 32, 32);
    f.word(OPENER_NAMES + 32, 0xdead0000);
    f.word(OPENER_NAMES + 40, 20);
    f.word(OPENER_NAMES + 48, 20);
    // Even an unmatched later name must be observed; no early first-match exit.
    assert_eq!(
        hoi4_live_native::politics_focus::owned_snapshot(&f, false)
            .unwrap_err()
            .code,
        "unexpected_read"
    );
}

#[test]
fn politics_focus_unknown_types_never_read_guessed_payload() {
    for case in 0..3 {
        let mut f = politics_focus_fixture();
        match case {
            0 => {
                f.word(0xd5000, 0x10000 + 0x4346b10);
                politics_forbid(&mut f, 0xd5068, 8);
            }
            1 => {
                f.word(0xf0000, 0x10000 + 0x4346b10);
                politics_forbid(&mut f, 0xf0138, 8);
            }
            _ => {
                f.word(0xf1000, 0x10000 + 0x4346b10);
                politics_forbid(&mut f, 0xf1090, 8);
            }
        }
        let (v, _) = hoi4_live_native::politics_focus::owned_snapshot(&f, true).unwrap();
        match case {
            0 => assert_eq!(v["reason"], "standard_layout_unavailable"),
            1 => assert_eq!(
                v["render_diagnostic"]["reason"],
                "renderer_layout_unavailable"
            ),
            _ => assert_eq!(
                v["render_diagnostic"]["reason"],
                "resource_layout_unavailable"
            ),
        }
        assert_eq!(v["input_authorized"], false);
    }
}

#[test]
fn politics_focus_positive_tag_and_accessor_precede_all_ui_payload() {
    let mut f = politics_focus_fixture();
    f.number(0x30518, 0);
    politics_forbid(&mut f, 0x30340, 16);
    politics_forbid(&mut f, 0x326b8, 8);
    assert_eq!(
        hoi4_live_native::politics_focus::owned_snapshot(&f, true)
            .unwrap_err()
            .code,
        "player_unavailable"
    );
    let mut f = politics_focus_fixture();
    f.word(0x10000 + 0x42c9a88 + 0xc0, 0x10000 + 0x20811a8);
    politics_forbid(&mut f, 0x30340, 16);
    assert_eq!(
        hoi4_live_native::politics_focus::owned_snapshot(&f, true)
            .unwrap_err()
            .code,
        "invalid_gui_owner"
    );
}

#[test]
fn politics_focus_final_witnesses_cover_scope_names_glue_and_geometry() {
    for (a, n) in [
        (0x10000 + 0x44086a8, 8),
        (0x10000 + 0x42c9a88 + 0xc0, 8),
        (0x30518, 4),
        (0x3a010, 8),
        (0x326b8, 8),
        (0x324f8, 8),
        (0x90180, 8),
        (0x90128, 8),
        (0x90130, 8),
        (0x903d8, 4),
        (POLITICS, 8),
        (POLITICS + 0x38, 8),
        (POLITICS + 0x2f0, 8),
        (POLITICS + 0x2f8, 8),
        (FOCUS + 0x11d8, 8),
        (POLITICS_WINDOW + 0x98, 8),
        (POLITICS_WINDOW + 0xa5, 1),
        (POLITICS_WINDOW + 0xef8, 4),
        (POLITICS_WINDOW + 0xbb0, 16),
        (POLITICS_WINDOW + 0xbc8, 16),
        (CHILD_VALUES, 8),
        (CHILD_NAMES, 32),
        (0xd49a0, 16),
        (0xd49b8, 16),
        (OPENER_VALUES, 8),
        (OPENER_NAMES, 32),
        (OPENER_NAME_TEXT, 24),
        (0xd5080, 8),
        (0xd5088, 8),
        (0xd5098, 4),
        (0xd50a0, 8),
        (OPENER_LINK, 8),
        (OPENER_LINK + 8, 8),
        (OPENER_LINK + 0x10, 8),
        (OPENER_LINK + 0x18, 1),
        (POLITICS + 0x300, 8),
        (POLITICS + 0x308, 8),
        (POLITICS + 0x318, 8),
        (POLITICS + 0x320, 8),
        (POLITICS + 0x328, 0x260),
        (OPENER_CAPTURE, 8),
        (OPENER_CAPTURE + 8, 8),
        (OPENER_CAPTURE + 0x10, 8),
        (0xd50ac, 4),
        (0xd5074, 4),
        (0xd50e8, 8),
        (0xf0138, 8),
        (0xf0168, 8),
        (0xf00b0, 4),
        (0xf0114, 4),
        (0xf10a4, 4),
        (0xf110c, 4),
        (0xd4e70, 8),
        (0xf5100, 8),
    ] {
        let f = politics_focus_fixture();
        let mut changed: Vec<_> = (0..n).map(|i| f.data[&(a + i as u64)]).collect();
        // Inline payload is witnessed as part of its complete32-byte header.
        changed[if a == CHILD_NAMES { 16 } else { 0 }] ^= 1;
        let p = OwnedContextFixture {
            fixture: f,
            armed: Cell::new(false),
            context_started: Cell::new(false),
            tag_reads: Cell::new(0),
            final_mutation: Some((a, changed)),
            finishing: Cell::new(false),
        };
        let error = match hoi4_live_native::politics_focus::owned_snapshot(&p, true) {
            Err(error) => error,
            Ok(_) => panic!("Final field{a:x}/{n} mutation was not witnessed"),
        };
        assert_eq!(error.code, "torn_read", "final field{a:x}");
    }
}

#[test]
fn politics_focus_noncanonical_links_refuse_without_dereference() {
    for field in [0x90128, POLITICS + 0x2f8, OPENER_VALUES, POLITICS + 0x308] {
        for value in [0xffff, 0x7fff_ffff_0008, u64::MAX - 7] {
            let mut f = politics_focus_fixture();
            f.word(field, value);
            assert_eq!(
                hoi4_live_native::politics_focus::owned_snapshot(&f, true)
                    .unwrap_err()
                    .code,
                "invalid_gui_owner",
                "field{field:x}/value{value:x}"
            );
            assert!(!f.tracked_reads.borrow().contains_key(&value));
        }
    }
}

// The research context path also checks the game's ordinary overview types.
// Synthetic RTTI/name buffers live separately from the nearby primary tables.
fn owned_research_fixture() -> Fixture {
    let mut f = Fixture::new();
    f.own_research();
    let base = 0x10000;
    for (i, (table, name)) in [
        (0x42108f8, "CTechnologyStatus"),
        (0x4210898, "CResearchSlot"),
        (0x42107f8, "CTechnology"),
        (0x4211038, "CTechnologyTemplate"),
    ]
    .iter()
    .enumerate()
    {
        let info = base + 0x435d000 + i as u64 * 0x100;
        let text = base + 0x350d000 + i as u64 * 0x100;
        f.word(base + table - 8, info);
        f.region(info, 16);
        f.word(info + 8, text);
        f.region(text, 128);
        f.put(text, format!("{}{name}", name.len()).as_bytes());
    }
    f
}

struct OwnedContextFixture {
    fixture: Fixture,
    armed: Cell<bool>,
    context_started: Cell<bool>,
    tag_reads: Cell<usize>,
    final_mutation: Option<(u64, Vec<u8>)>,
    finishing: Cell<bool>,
}

fn owned_node_fixture() -> Fixture {
    let mut f = Fixture::new();
    f.node_render_detail();
    for (i, (table, name)) in [
        (0x42108f8, "CTechnologyStatus"),
        (0x4210898, "CResearchSlot"),
        (0x42107f8, "CTechnology"),
        (0x4211038, "CTechnologyTemplate"),
    ]
    .iter()
    .enumerate()
    {
        let info = 0x10000 + 0x435f000 + i as u64 * 0x100;
        let text = 0x10000 + 0x350f000 + i as u64 * 0x100;
        f.word(0x10000 + table - 8, info);
        f.region(info, 16);
        f.word(info + 8, text);
        f.region(text, 128);
        f.put(text, format!("{}{name}", name.len()).as_bytes());
    }
    f
}
#[test]
fn owned_node_detail_carries_same_requested_body_campaign_and_slot_ids() {
    let f = owned_node_fixture();
    let (value, context) = research_ui::node_detail_owned(&f, "tools1").unwrap();
    assert_eq!(value["id"], "tools1");
    assert_eq!(context.campaign.player, "GER");
    assert_eq!(context.anchors[..2], [0x20000, 0x28000]);
    assert_eq!(context.slots, [0x2c000, 0x2c100, 0x2c200, 0x2c300]);
    assert_eq!(f.checks.get(), 2);
}
#[test]
fn owned_node_detail_refuses_inner_tag_and_final_slot_mutations() {
    let mut f = OwnedContextFixture::new();
    f.fixture = owned_node_fixture();
    f.armed.set(true);
    assert_eq!(
        research_ui::node_detail_owned(&f, "tools1")
            .unwrap_err()
            .code,
        "torn_read"
    );
    let mut f = OwnedContextFixture::new();
    f.fixture = owned_node_fixture();
    f.final_mutation = Some((0x3b000, vec![0; 32]));
    assert_eq!(
        research_ui::node_detail_owned(&f, "tools1")
            .unwrap_err()
            .code,
        "torn_read"
    );
}
impl OwnedContextFixture {
    fn new() -> Self {
        Self {
            fixture: owned_research_fixture(),
            armed: Cell::new(false),
            context_started: Cell::new(false),
            tag_reads: Cell::new(0),
            final_mutation: None,
            finishing: Cell::new(false),
        }
    }
}
impl ReadMemory for OwnedContextFixture {
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        if self.armed.get() && address == 0x30718 {
            self.context_started.set(true);
        }
        if self.armed.get() && self.context_started.get() && address == 0x30518 {
            self.tag_reads.set(self.tag_reads.get() + 1);
            // Positive outer GER tag, then a foreign tag at the body/context
            // join, then GER again. No foreign-country payload is readable.
            if self.tag_reads.get() == 1 {
                assert_eq!(size, 4);
                return Ok(2i32.to_le_bytes().to_vec());
            }
        }
        if self.finishing.get() {
            if let Some((changed, bytes)) = &self.final_mutation {
                if address == *changed {
                    assert_eq!(size, bytes.len());
                    return Ok(bytes.clone());
                }
            }
        }
        self.fixture.read(address, size)
    }
    fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        hoi4_live_native::memory::validate_read_batch(requests)?;
        let previous = self.finishing.replace(true);
        let result = requests.iter().map(|&(a, n)| self.read(a, n)).collect();
        self.finishing.set(previous);
        result
    }
}
impl Process for OwnedContextFixture {
    fn image_base(&self) -> u64 {
        self.fixture.image_base()
    }
    fn check(&self) -> Result<()> {
        self.fixture.check()
    }
}

#[test]
fn owned_research_context_pairs_canonical_slots_with_same_body_ownership() {
    let f = owned_research_fixture();
    let (value, context) = research::owned_snapshot(&f).unwrap();
    assert_eq!(context.campaign.player, "GER");
    assert_eq!(context.campaign.anchors, [0x30000, 0x32000, 0x36000]);
    assert_eq!(context.own, [0x20000, 0x28000]);
    assert_eq!(context.slots, [0x2c000, 0x2c100, 0x2c200, 0x2c300]);
    assert_eq!(value["slots"][0]["researching"], "tools1");
    assert_eq!(value["slots"][0]["progress_raw"], -0x1234_5678_9abci64);
    assert_eq!(f.checks.get(), 2); // One owned process bracket, no nested begin.
}

#[test]
fn owned_research_refuses_inner_foreign_context_between_outer_ger_samples() {
    let f = OwnedContextFixture::new();
    assert_eq!(game::clock_context(&f).unwrap().player, "GER");
    f.armed.set(true);
    assert_eq!(research::owned_snapshot(&f).unwrap_err().code, "torn_read");
    assert_eq!(game::clock_context(&f).unwrap().player, "GER");
    assert!(!f.fixture.tracked_reads.borrow().contains_key(&0xdead0000));
}

#[test]
fn owned_research_rechecks_slot_identity_header_and_assignment_at_finish() {
    for (address, changed) in [
        (0x2c018, 0u64.to_le_bytes().to_vec()),
        (
            0x3b000,
            [0x2c100u64, 0x2c000, 0x2c200, 0x2c300]
                .into_iter()
                .flat_map(u64::to_le_bytes)
                .collect(),
        ),
        (
            0x280a0,
            [
                0x85000u64.to_le_bytes().as_slice(),
                4i32.to_le_bytes().as_slice(),
                4i32.to_le_bytes().as_slice(),
            ]
            .concat(),
        ),
    ] {
        let mut f = OwnedContextFixture::new();
        // A relocated array has identical slot values. Header identity must
        // still refuse the relocation; payload equality alone is insufficient.
        let original = (0..32)
            .map(|i| f.fixture.data[&(0x3b000 + i)])
            .collect::<Vec<_>>();
        f.fixture.put(0x85000, &original);
        f.final_mutation = Some((address, changed));
        assert_eq!(
            research::owned_snapshot(&f).unwrap_err().code,
            "torn_read",
            "field {address:x}"
        );
    }
}

#[test]
fn owned_research_checks_positive_scope_before_broader_context_payload() {
    let mut f = owned_research_fixture();
    f.number(0x30518, 0);
    for address in 0x39000..0x3900c {
        f.data.remove(&address);
    }
    assert_eq!(
        research::owned_snapshot(&f).unwrap_err().code,
        "player_unavailable"
    );
    assert!(!f.tracked_reads.borrow().contains_key(&0x39000));
    assert!(!f.tracked_reads.borrow().contains_key(&0x30718));
}

#[test]
fn owned_research_checks_technology_owner_before_broader_context_payload() {
    let mut f = owned_research_fixture();
    f.word(0x80158, 0xdead0000);
    // A foreign own-slot pointer must stop at owner+158, before either the
    // template pointer or the broader clock context's technology-name path.
    for address in (0x80160..0x80168).chain(0x30718..0x30738) {
        f.data.remove(&address);
    }
    assert_eq!(
        research::owned_snapshot(&f).unwrap_err().code,
        "invalid_research_owner"
    );
    assert!(!f.tracked_reads.borrow().contains_key(&0x80160));
    assert!(!f.tracked_reads.borrow().contains_key(&0x30718));
}

#[test]
fn research_public_page_does_not_require_new_context_payloads() {
    let mut f = Fixture::new();
    f.own_research();
    for (address, size) in [
        (0x30718, 32),
        (0x60000, 36),
        (0x30620, 4),
        (0x30628, 4),
        (0x30460, 4),
        (0x10000 + 0x43f3eb8, 8),
    ] {
        for a in address..address + size {
            f.data.remove(&a);
        }
    }
    let value = research::page(&f, 0, 1).unwrap();
    assert_eq!(value["researched"][0]["id"], "tools2");
    assert_eq!(value["slots"][0]["researching"], "tools1");
    assert!(!f.tracked_reads.borrow().contains_key(&0x30718));
    assert_eq!(f.checks.get(), 2);
}

#[test]
fn game_context_refuses_foreign_status_before_slot_array_payload() {
    for tag_fault in [false, true] {
        let mut f = Fixture::new();
        if tag_fault {
            f.number(0x28138, 2);
        } else {
            f.word(0x28130, 0xdead0000);
        }
        for address in 0x280a0..0x280b0 {
            f.data.remove(&address);
        }
        assert_eq!(
            game::clock_context(&f).unwrap_err().code,
            "invalid_research_owner"
        );
        assert!(!f.tracked_reads.borrow().contains_key(&0x280a0));
    }
}

#[test]
fn game_context_refuses_foreign_slot_before_active_technology_pointer() {
    let mut f = Fixture::new();
    f.word(0x2c010, 0xdead0000);
    for address in 0x2c018..0x2c020 {
        f.data.remove(&address);
    }
    assert_eq!(
        game::snapshot(&f).unwrap_err().code,
        "invalid_research_owner"
    );
    assert!(!f.tracked_reads.borrow().contains_key(&0x2c018));
}

#[test]
fn game_context_refuses_foreign_active_tech_before_unmapped_template_pointer() {
    let mut f = owned_research_fixture();
    f.word(0x80158, 0xdead0000);
    for address in 0x80160..0x80168 {
        f.data.remove(&address);
    }
    assert_eq!(
        game::clock_context(&f).unwrap_err().code,
        "invalid_research_owner"
    );
    assert!(!f.tracked_reads.borrow().contains_key(&0x80160));
}

#[test]
fn game_context_bounds_active_key_before_forbidden_heap_payload() {
    let mut f = owned_research_fixture();
    f.region(0x63008, 32);
    f.word(0x63008, 0xfe000);
    f.word(0x63010, 257);
    assert_eq!(game::snapshot(&f).unwrap_err().code, "invalid_string");
    assert!(!f.tracked_reads.borrow().contains_key(&0xfe000));
}

#[test]
fn game_context_rechecks_new_owner_witnesses_at_snapshot_boundary() {
    for (address, changed) in [
        (0x28130, 0xdead0000u64.to_le_bytes().to_vec()),
        (0x28138, 2i32.to_le_bytes().to_vec()),
        (0x2c010, 0xdead0000u64.to_le_bytes().to_vec()),
        (0x80158, 0xdead0000u64.to_le_bytes().to_vec()),
    ] {
        let mut f = owned_research_fixture();
        f.fault = Some((address, 1, changed));
        assert_eq!(
            game::snapshot(&f).unwrap_err().code,
            "torn_read",
            "owner field {address:x}"
        );
    }
}

#[test]
fn game_context_requires_positive_own_tag_before_country_mapping() {
    let mut f = Fixture::new();
    f.number(0x30518, 0);
    for address in 0x30340..0x30350 {
        f.data.remove(&address);
    }
    assert_eq!(game::snapshot(&f).unwrap_err().code, "player_unavailable");
    assert!(!f.tracked_reads.borrow().contains_key(&0x30340));
}

struct Fixture {
    tracked_reads: std::cell::RefCell<BTreeMap<u64, usize>>,
    data: BTreeMap<u64, u8>,
    clock_reads: Cell<u32>,
    torn: bool,
    checks: Cell<u32>,
    replaced: bool,
    id_reads: Cell<u32>,
    changed_id: bool,
    pause_reads: Cell<u32>,
    invalid_final_pause: bool,
    catalog_reads: Cell<u32>,
    changed_catalog: bool,
    geometry_reads: Cell<u32>,
    changed_geometry: bool,
    catalog_name_reads: Cell<u32>,
    changed_catalog_name: bool,
    catalog_template_reads: Cell<u32>,
    relocated_catalog: bool,
    fault: Option<(u64, usize, Vec<u8>)>,
    fault_reads: Cell<usize>,
    historical_reads: Cell<usize>,
    inconsistent_historical_body: bool,
    root_reads: Cell<usize>,
    split_context_roots: bool,
}
impl Fixture {
    fn put(&mut self, a: u64, data: &[u8]) {
        for (i, v) in data.iter().enumerate() {
            self.data.insert(a + i as u64, *v);
        }
    }
    fn region(&mut self, a: u64, n: usize) {
        self.put(a, &vec![0; n]);
    }
    fn word(&mut self, a: u64, n: u64) {
        self.put(a, &n.to_le_bytes());
    }
    fn number(&mut self, a: u64, n: i32) {
        self.put(a, &n.to_le_bytes());
    }
    fn string(&mut self, a: u64, text: &str) {
        assert!(text.len() <= 15);
        self.region(a, 32);
        self.word(a, a + 16);
        self.word(a + 8, text.len() as u64);
        self.put(a + 16, text.as_bytes());
    }
    fn object(&mut self, a: u64, name: &str, vt: u64) {
        self.region(vt - 8, 32);
        self.region(vt + 32, 128);
        self.word(a, vt);
        self.word(vt - 8, vt + 16);
        self.word(vt + 24, vt + 32);
        self.put(vt + 32, format!("{}{name}\0", name.len()).as_bytes());
    }
    fn array(&mut self, a: u64, data: u64, count: i32) {
        self.region(a, 16);
        self.word(a, data);
        self.number(a + 8, count);
        self.number(a + 12, count);
    }
    fn new() -> Self {
        let mut f = Self {
            tracked_reads: std::cell::RefCell::new(BTreeMap::new()),
            data: BTreeMap::new(),
            clock_reads: Cell::new(0),
            torn: false,
            checks: Cell::new(0),
            replaced: false,
            id_reads: Cell::new(0),
            changed_id: false,
            pause_reads: Cell::new(0),
            invalid_final_pause: false,
            catalog_reads: Cell::new(0),
            changed_catalog: false,
            geometry_reads: Cell::new(0),
            changed_geometry: false,
            catalog_name_reads: Cell::new(0),
            changed_catalog_name: false,
            catalog_template_reads: Cell::new(0),
            relocated_catalog: false,
            fault: None,
            fault_reads: Cell::new(0),
            historical_reads: Cell::new(0),
            inconsistent_historical_body: false,
            root_reads: Cell::new(0),
            split_context_roots: false,
        };
        let base = 0x10000;
        f.word(base + 0x44086a8, 0x30000);
        f.word(base + 0x44085f8, 0x32000);
        f.word(base + 0x439f450, 0x36000);
        f.word(base + 0x43f3eb8, 0x37000);
        for (a, n) in [
            (0x30000, 0x700),
            (0x32000, 0x700),
            (0x20000, 0x1400),
            (0x28000, 0x200),
            (0x2a000, 0x50),
        ] {
            f.region(a, n);
        }
        f.word(0x30000, base + 0x42c7598);
        f.word(0x32000, base + 0x42c9a88);
        f.number(0x30460, 60759371);
        f.number(0x304b4, 4);
        f.number(0x30518, 1);
        f.number(0x30620, 2);
        f.number(0x30628, 1);
        let id = b"aabbccdd-1122-4344-8566-778899aabbcc";
        f.region(0x30718, 32);
        f.word(0x30718, 0x60000);
        f.word(0x30720, id.len() as u64);
        f.put(0x60000, id);
        f.put(0x326c1, &[1]);
        f.array(0x30340, 0x39000, 3);
        f.region(0x39000, 12);
        f.number(0x39004, 2);
        f.array(0x30310, 0x3a000, 3);
        f.region(0x3a000, 24);
        f.word(0x3a000, 0xdead0000);
        f.word(0x3a008, 0xbeef0000);
        f.word(0x3a010, 0x20000);
        f.array(0x37000, 0x38000, 2);
        f.string(0x38000, "---");
        f.string(0x38020, "GER");
        f.string(0x20010, "German Reich");
        f.object(0x20000, "CCountry", 0x50000);
        f.word(0x21348, 0x2a000);
        f.object(0x2a000, "CNationalFocusProgress", 0x50100);
        f.word(0x20f38, 0x28000);
        f.object(0x28000, "CTechnologyStatus", 0x50200);
        f.word(0x28130, 0x20000);
        f.number(0x28138, 1);
        f.array(0x280a0, 0x3b000, 4);
        f.region(0x3b000, 32);
        for i in 0..4 {
            let a = 0x2c000 + i * 0x100;
            f.region(a, 64);
            f.object(a, "CResearchSlot", 0x50300);
            f.word(a + 0x10, 0x28000);
            f.word(0x3b000 + i * 8, a);
        }
        f
    }
    fn catalog(&mut self) {
        let base = 0x10000;
        self.region(0x36000, 0x100);
        self.word(0x36000, base + 0x42110c8);
        self.array(0x36048, 0x62000, 4);
        self.region(0x62000, 32);
        // Actual constructor: slot zero is a typed nonnull reserved singleton,
        // not a zero pointer or positive technology definition.
        self.word(0x62000, 0x62500);
        self.word(base + 0x439f560, 0x62500);
        self.word(0x62500, base + 0x4201c78);
        // Canonical indices are independent of copied template+0x34 values.
        for (i, name) in ["tools1", "tools2", "construction1"].iter().enumerate() {
            let ptr = 0x63000 + i as u64 * 0x100;
            self.region(ptr, 0x80);
            self.word(0x62008 + i as u64 * 8, ptr);
            self.word(ptr, base + 0x4211038);
            self.string(ptr + 8, name);
            self.number(ptr + 0x30, i as i32 + 1);
            self.number(ptr + 0x34, 999);
        }
    }
    fn geometry(&mut self) {
        let base = 0x10000;
        self.word(0x324f8, 0x70000);
        self.region(0x70000, 0xc0);
        self.word(0x70000, base + 0x42736f8);
        self.word(0x700b8, 0x71000);
        self.word(base + 0x4577890, 0x71000);
        self.region(0x71000, 0x200);
        self.word(0x71000, base + 0x42417f8);
        for (i, v) in [512, 288, 1280, 720].iter().enumerate() {
            self.number(0x71178 + i as u64 * 4, *v);
        }
        for (i, v) in [2.5f32, 2.5, 0.4, 0.4].iter().enumerate() {
            self.put(0x711d0 + i as u64 * 4, &v.to_le_bytes());
        }
        self.put(0x71040, &640f32.to_le_bytes());
        self.put(0x71044, &20f32.to_le_bytes());
    }
    fn own_research(&mut self) {
        self.catalog();
        let base = 0x10000;
        self.word(0x28000, base + 0x42108f8);
        self.word(0x28130, 0x20000);
        self.number(0x28138, 1);
        self.array(0x28088, 0x82000, 3);
        self.region(0x82000, 24);
        self.array(0x28040, 0x83000, 1);
        self.word(0x83000, 0x81000);
        for (i, (level, threshold)) in [(1, 2), (3, 3)].iter().enumerate() {
            let pointer = 0x80000 + i as u64 * 0x1000;
            let template = 0x63000 + i as u64 * 0x100;
            self.region(pointer, 0x1a0);
            self.word(pointer, base + 0x42107f8);
            self.word(pointer + 0x158, 0x28000);
            self.word(pointer + 0x160, template);
            self.number(pointer + 0x174, *level);
            self.number(template + 0x3cc, *threshold);
            self.word(0x82000 + i as u64 * 8, pointer);
        }
        self.put(0x80198, &(-0x1234_5678_9abci64).to_le_bytes());
        for i in 0..4 {
            let slot = 0x2c000 + i * 0x100;
            self.word(slot, base + 0x4210898);
            self.word(slot + 0x10, 0x28000);
            self.word(slot + 0x20, 999999);
        }
        self.word(0x2c018, 0x80000);
    }
    fn research_ui(&mut self) {
        self.own_research();
        self.geometry();
        let base = 0x10000;
        self.word(0x326b8, 0x90000);
        self.region(0x90000, 0x400);
        self.word(0x90180, 0x32000);
        self.word(0x900f0, 0x91000);
        self.word(0x900f8, 0x92000);
        self.number(0x903d8, 5);
        self.region(0x91000, 0x400);
        self.region(0x92000, 0xe30);
        self.word(0x91000, base + 0x424dc38);
        self.word(0x92000, base + 0x424de40);
        self.word(0x912f0, 0x32000);
        self.word(0x922f0, 0x32000);
        self.number(0x92e24, 6);
        self.word(0x92e08, 0x2c100);
        self.array(0x91308, 0x93000, 4);
        self.region(0x93000, 32);
        for i in 0..4 {
            let widget = 0x94000 + i * 0x1000;
            let container = 0xa0000 + i * 0x2000;
            self.region(widget, 0x300);
            self.word(widget, base + 0x424e7d8);
            self.word(widget + 0x2a8, 0x70000);
            self.word(widget + 0x2b0, 0x91000);
            self.word(widget + 0x2b8, 0x2c000 + i * 0x100);
            self.word(widget + 0x10, container);
            self.word(0x93000 + i * 8, widget);
            self.region(container, 0xf00);
            self.word(container, base + 0x4347d58);
            self.word(container + 0x98, 0x70000);
            self.put(container + 0xa5, &[8]);
        }
        self.word(0x912f8, 0xa0000);
        self.array(0xa0958, 0xb0000, 1);
        self.word(0xb0000, 0xb1000);
        self.region(0xb1000, 0x100);
        self.word(0xb10e8, 0xb2000);
        self.region(0xb2000, 0x1c0);
        self.number(0xb2034, 3);
        self.word(0xb2168, 0xb3000);
        self.word(0xb21b0, 0xb3000);
        self.region(0xb3000, 8);
        for (object, name, vptr, info, text) in [
            (
                0xb1000,
                "CButtonStandard",
                base + 0x43470c8,
                base + 0x4350000,
                base + 0x38afd74,
            ),
            (
                0xb2000,
                "CFrameAnimatedSprite",
                base + 0x434b370,
                base + 0x4351000,
                base + 0x38b4750,
            ),
            (
                0xb3000,
                "CFrameAnimatedSpriteType",
                base + 0x434b668,
                base + 0x4352000,
                base + 0x3507000,
            ),
        ] {
            self.word(object, vptr);
            self.word(vptr - 16, 0);
            self.word(vptr - 8, info);
            self.region(info, 16);
            self.word(info + 8, text);
            self.region(text, 128);
            self.put(text, format!("{}{name}", name.len()).as_bytes());
        }
    }
    fn research_drag(&mut self) {
        self.research_ui();
        let base = 0x10000;
        self.region(0xb1000, 0x120);
        self.word(0xb1000, base + 0x4346b10);
        self.word(base + 0x4346b10 - 16, 0);
        self.word(base + 0x4346b10 - 8, base + 0x4350000);
        self.region(base + 0x38afd74, 128);
        self.put(base + 0x38afd74, b"11CButtonDrag");
        self.word(0xb1110, 0xb2000);
        self.number(0xb10ac, 4);
        // Position data is deliberately unreadable as a pointer. The exact
        // Drag layout must not read the Standard renderer field at +e8.
        for address in 0xb10e8..0xb10f0 {
            self.data.remove(&address);
        }
    }
    fn research_plain_sprite(&mut self) {
        self.research_drag();
        let base = 0x10000;
        for (object, table, info, text, name) in [
            (0xb2000, 0x4356e08, 0x4351000, 0x38b4750, "7CSprite"),
            (0xb3000, 0x4357160, 0x4352000, 0x3507000, "11CSpriteType"),
        ] {
            self.word(object, base + table);
            self.word(base + table - 16, 0);
            self.word(base + table - 8, base + info);
            self.region(base + text, 128);
            self.put(base + text, name.as_bytes());
        }
        // The plain primary uses its direct resource at +168. Neither the
        // Instant +160 nor Frame +1b0 field exists in this fixture.
        for address in (0xb2160..0xb2168).chain(0xb21b0..0xb21b8) {
            self.data.remove(&address);
        }
    }
    fn research_info(&mut self) {
        self.research_ui();
        let base = 0x10000;
        self.array(0x92dc0, 0xc0000, 1);
        self.word(0xc0000, 0xc1000);
        self.region(0xc1000, 0xb00);
        self.word(0xc1000, base + 0x430bbf0);
        self.word(0xc1a78, 0x32000);
        self.word(0xc1a80, 0x92000);
        self.word(0xc1a98, 0x80000);
        self.word(0xc1a88, 0xc3000);
        self.word(0xc1ac0, 0xc5000);
        self.word(0xc1ab0, 0xc5000);
        self.region(0xc3000, 0xf00);
        self.word(0xc3000, base + 0x4347d58);
        self.word(0xc3098, 0x70000);
        self.put(0xc30a5, &[8]);
        self.array(0xc39a0, 0xc7000, 2);
        self.array(0xc39b8, 0xc6000, 2);
        self.word(0xc7000, 0xc8000);
        self.word(0xc7008, 0xc5000);
        self.string(0xc6000, "close");
        self.string(0xc6020, "research");
        self.region(0xc5000, 0x120);
        self.word(0xc5000, base + 0x43470c8);
        self.word(0xc5068, 0x70000); // Exact Standard constructor owner.
        self.number(0xc50ac, 3);
        self.put(0xc5074, &0x1234_0800u32.to_le_bytes());
        // Another named registry has no readable payload here. It must not
        // be used in place of the actual virtual +88 button lookup.
        for address in (0xc3a60..0xc3a70).chain(0xc3a78..0xc3a88) {
            self.data.remove(&address);
        }
    }
    fn current_research_nodes(&mut self) {
        self.research_ui();
        let base = 0x10000;
        self.number(0x903d8, 6);
        self.put(0xa00a5, &[0]);
        self.word(0x922f8, 0xd0000);
        self.region(0xd0000, 0xf00);
        self.word(0xd0000, base + 0x4347d58);
        self.word(0xd0098, 0x70000);
        self.put(0xd00a5, &[8]);
        self.array(0x92dd8, 0xd2000, 2);
        self.region(0xd2000, 80);
        self.word(0xd2010, 0xd3000);
        self.word(0xd2020, 0x92000); // Context, not the item.
        self.word(0xd2038, 0xd3000);
        self.word(0xd2048, 0xdead0000); // Unreadable context, never dereferenced.
        self.array(0x92df0, 0, 0);
        self.region(0xd3000, 0xd0);
        self.word(0xd3000, base + 0x424e2e0);
        self.word(0xd3030, 0x92000);
        self.word(0xd30c8, 0x80000);
    }
    fn research_wait(&mut self) {
        self.research_drag();
        let base = 0x10000;
        // Real tables are near each other. Keep synthetic RTTI/names separate
        // rather than placing fake name buffers inside another real table.
        for (i, (a, name, vt)) in [
            (0x28000, "CTechnologyStatus", 0x42108f8),
            (0x2c000, "CResearchSlot", 0x4210898),
            (0x2c100, "CResearchSlot", 0x4210898),
            (0x2c200, "CResearchSlot", 0x4210898),
            (0x2c300, "CResearchSlot", 0x4210898),
            (0x80000, "CTechnology", 0x42107f8),
            (0x63000, "CTechnologyTemplate", 0x4211038),
        ]
        .iter()
        .enumerate()
        {
            let info = base + 0x4353000 + i as u64 * 0x100;
            let text = base + 0x3508000 + i as u64 * 0x100;
            self.word(*a, base + vt);
            self.word(base + vt - 16, 0);
            self.word(base + vt - 8, info);
            self.region(info, 16);
            self.word(info + 8, text);
            self.region(text, 128);
            self.put(text, format!("{}{name}", name.len()).as_bytes());
        }
    }
}
impl ReadMemory for Fixture {
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        *self.tracked_reads.borrow_mut().entry(a).or_default() += 1;
        if a == 0x10000 + 0x44086a8 {
            self.root_reads.set(self.root_reads.get() + 1);
            if self.split_context_roots && (2..=3).contains(&self.root_reads.get()) {
                return Ok(0x90000u64.to_le_bytes().to_vec());
            }
        }
        if a == 0x30628 {
            self.historical_reads.set(self.historical_reads.get() + 1);
            if self.inconsistent_historical_body && (2..=3).contains(&self.historical_reads.get()) {
                return Ok(0i32.to_le_bytes().to_vec());
            }
        }
        if let Some((address, after, replacement)) = &self.fault {
            if a == *address {
                self.fault_reads.set(self.fault_reads.get() + 1);
                if self.fault_reads.get() > *after {
                    assert_eq!(replacement.len(), n);
                    return Ok(replacement.clone());
                }
            }
        }
        if a == 0x63000 {
            self.catalog_template_reads
                .set(self.catalog_template_reads.get() + 1);
        }
        if a == 0x36048 && self.relocated_catalog && self.catalog_template_reads.get() > 0 {
            let mut bytes = 0x62100u64.to_le_bytes().to_vec();
            bytes.extend(4i32.to_le_bytes());
            bytes.extend(4i32.to_le_bytes());
            assert_eq!(bytes.len(), n);
            return Ok(bytes);
        }
        if a == 0x64000 {
            self.catalog_name_reads
                .set(self.catalog_name_reads.get() + 1);
            if self.changed_catalog_name && self.catalog_name_reads.get() > 1 {
                return Ok(vec![b'z'; n]);
            }
        }
        if a == 0x71040 {
            self.geometry_reads.set(self.geometry_reads.get() + 1);
            if self.changed_geometry && self.geometry_reads.get() > 1 {
                return Ok(vec![0; n]);
            }
        }
        if a == 0x62000 {
            self.catalog_reads.set(self.catalog_reads.get() + 1);
            if self.changed_catalog && self.catalog_reads.get() > 1 {
                return Ok(vec![0; n]);
            }
        }
        if a == 0x326c1 {
            self.pause_reads.set(self.pause_reads.get() + 1);
            if self.invalid_final_pause && self.pause_reads.get() > 2 {
                return Ok(vec![2]);
            }
        }
        if a == 0x60000 {
            self.id_reads.set(self.id_reads.get() + 1);
            if self.changed_id && self.id_reads.get() > 1 {
                return Ok(b"aabbccdd-1122-4344-8566-778899aabbcd".to_vec());
            }
        }
        if a == 0x30460 {
            self.clock_reads.set(self.clock_reads.get() + 1);
            if self.torn && self.clock_reads.get() > 1 {
                return Ok(60759372i32.to_le_bytes().to_vec());
            }
        }
        (0..n)
            .map(|i| {
                self.data.get(&(a + i as u64)).copied().ok_or_else(|| {
                    Error::new("unexpected_read", format!("Fixture forbids reading {a:x}"))
                })
            })
            .collect()
    }
}
impl Process for Fixture {
    fn image_base(&self) -> u64 {
        0x10000
    }
    fn check(&self) -> Result<()> {
        self.checks.set(self.checks.get() + 1);
        if self.replaced && self.checks.get() > 1 {
            Err(Error::new("ownership_changed", "Fixture replacement"))
        } else {
            Ok(())
        }
    }
}
#[test]
fn own_research_uses_completed_membership_and_active_i64_counter() {
    let mut f = Fixture::new();
    f.own_research();
    let value = research::snapshot(&f).unwrap();
    assert_eq!(value["researched"][0]["id"], "tools2");
    assert_eq!(value["researched"][0]["level_raw"], 3);
    assert_eq!(value["researched"][0]["completion_threshold_raw"], 3);
    assert_eq!(value["researched"].as_array().unwrap().len(), 1);
    assert_eq!(value["slots"][0]["researching"], "tools1");
    assert_eq!(value["slots"][0]["progress_raw"], -0x1234_5678_9abci64);
    assert!(value["slots"][0]["progress"].is_null());
    assert!(value["slots"][1]["progress_raw"].is_null());
    assert_eq!(f.checks.get(), 2);
}
#[test]
fn research_pages_sorted_membership_and_distinguishes_known_empty() {
    let mut f = Fixture::new();
    f.own_research();
    f.number(0x80174, 2);
    f.array(0x28040, 0x83000, 2);
    f.word(0x83008, 0x80000);
    let first = research::page(&f, 0, 1).unwrap();
    assert_eq!(first["researched_count"], 2);
    assert_eq!(first["researched"][0]["id"], "tools1");
    assert_eq!(first["next_offset"], 1);
    let next = research::page(&f, 1, 1).unwrap();
    assert_eq!(next["researched"][0]["id"], "tools2");
    assert!(next["next_offset"].is_null());
    assert_eq!(
        research::page(&f, 2, 1).unwrap()["researched"],
        serde_json::json!([])
    );
    f.array(0x28040, 0, 0);
    assert_eq!(research::snapshot(&f).unwrap()["researched_count"], 0);
}
#[test]
fn research_refuses_foreign_owners_and_inconsistent_canonical_joins() {
    for address in [0x28130, 0x2c010, 0x80158, 0x80160, 0x82000] {
        let mut f = Fixture::new();
        f.own_research();
        f.word(address, 0xdead0000);
        assert!(
            research::snapshot(&f).is_err(),
            "accepted changed field {address:x}"
        );
    }
    let mut f = Fixture::new();
    f.own_research();
    f.number(0x63030, 2);
    assert_eq!(
        research::snapshot(&f).unwrap_err().code,
        "invalid_research_join"
    );
}
#[test]
fn research_refuses_duplicate_members_partial_completion_and_uninitialized_arrays() {
    let mut f = Fixture::new();
    f.own_research();
    f.array(0x28040, 0x83000, 2);
    f.word(0x83008, 0x81000);
    assert_eq!(
        research::snapshot(&f).unwrap_err().code,
        "invalid_completed_research"
    );
    let mut f = Fixture::new();
    f.own_research();
    f.number(0x81174, 2);
    assert_eq!(
        research::snapshot(&f).unwrap_err().code,
        "invalid_completed_research"
    );
    let mut f = Fixture::new();
    f.own_research();
    f.array(0x28088, 0x82000, 2);
    assert_eq!(
        research::snapshot(&f).unwrap_err().code,
        "research_unavailable"
    );
    let mut f = Fixture::new();
    f.own_research();
    f.number(0x28090, 8193);
    assert_eq!(research::snapshot(&f).unwrap_err().code, "invalid_array");
}
#[test]
fn research_refuses_changed_counters_payloads_slots_and_relocated_lists() {
    for (address, replacement) in [
        (0x80198, 17i64.to_le_bytes().to_vec()),
        (0x83000, 0x80000u64.to_le_bytes().to_vec()),
        (
            0x82000,
            [0u64, 0x81000, 0]
                .into_iter()
                .flat_map(u64::to_le_bytes)
                .collect(),
        ),
        (0x2c018, 0u64.to_le_bytes().to_vec()),
        (0x28130, 0xdead0000u64.to_le_bytes().to_vec()),
    ] {
        let mut f = Fixture::new();
        f.own_research();
        f.fault = Some((address, 1, replacement));
        assert!(
            research::snapshot(&f).is_err(),
            "accepted torn field {address:x}"
        );
    }
    let mut f = Fixture::new();
    f.own_research();
    f.region(0x84000, 24);
    f.word(0x84000, 0x80000);
    f.word(0x84008, 0x81000);
    let mut relocated = 0x84000u64.to_le_bytes().to_vec();
    relocated.extend(3i32.to_le_bytes());
    relocated.extend(3i32.to_le_bytes());
    f.fault = Some((0x28088, 2, relocated));
    assert_eq!(research::snapshot(&f).unwrap_err().code, "torn_read");
}
#[test]
fn research_retains_final_process_check_and_refuses_wrong_primary_type() {
    let mut f = Fixture::new();
    f.own_research();
    f.replaced = true;
    assert_eq!(
        research::snapshot(&f).unwrap_err().code,
        "ownership_changed"
    );
    let mut f = Fixture::new();
    f.own_research();
    f.word(0x80000, 0x10000 + 0x4210868);
    assert_eq!(research::snapshot(&f).unwrap_err().code, "invalid_type");
}
#[test]
fn player_tag_index_uses_mapping_not_country_index() {
    let f = Fixture::new();
    let value = game::snapshot(&f).unwrap();
    assert_eq!(value["player"], "GER");
    assert_eq!(value["country"], "German Reich");
    assert_eq!(value["research"].as_array().unwrap().len(), 4);
    assert!(value["focus"].is_null());
    assert_eq!(f.checks.get(), 2);
}
#[test]
fn changed_clock_refuses_snapshot() {
    let mut f = Fixture::new();
    f.torn = true;
    assert_eq!(game::snapshot(&f).unwrap_err().code, "torn_read");
}
#[test]
fn replaced_process_refuses_snapshot() {
    let mut f = Fixture::new();
    f.replaced = true;
    assert_eq!(game::snapshot(&f).unwrap_err().code, "ownership_changed");
}

#[test]
fn own_clock_context_has_a_bounded_campaign_identity() {
    let fixture = Fixture::new();
    let context = game::clock_context(&fixture).unwrap();
    assert_eq!(context.player, "GER");
    assert_eq!(
        context.playthrough_id,
        "aabbccdd-1122-4344-8566-778899aabbcc"
    );
    assert_eq!(context.historical_raw, 1);
    assert!(context.paused);
    assert_eq!(fixture.checks.get(), 2);
}
#[test]
fn wait_payload_and_campaign_witness_share_one_owned_bracket() {
    let fixture = Fixture::new();
    let (value, context) = game::wait_snapshot(&fixture).unwrap();
    assert_eq!(value["player"], context.player);
    assert_eq!(value["date"]["raw"], context.date_raw);
    assert_eq!(value["paused"], context.paused);
    assert_eq!(fixture.checks.get(), 2);
    let mut changed = Fixture::new();
    changed.inconsistent_historical_body = true;
    assert_eq!(game::wait_snapshot(&changed).unwrap_err().code, "torn_read");
    let mut changed = Fixture::new();
    let copy: Vec<_> = (0..0x800)
        .map(|i| changed.data.get(&(0x30000 + i)).copied().unwrap_or(0))
        .collect();
    changed.put(0x90000, &copy);
    changed.split_context_roots = true;
    assert_eq!(game::wait_snapshot(&changed).unwrap_err().code, "torn_read");
    let mut changed = Fixture::new();
    changed.changed_id = true;
    assert_eq!(game::wait_snapshot(&changed).unwrap_err().code, "torn_read");
    let mut changed = Fixture::new();
    changed.replaced = true;
    assert_eq!(
        game::wait_snapshot(&changed).unwrap_err().code,
        "ownership_changed"
    );
}

#[test]
fn clock_context_retains_final_ownership_and_clock_readback() {
    let mut fixture = Fixture::new();
    fixture.replaced = true;
    assert_eq!(
        game::clock_context(&fixture).unwrap_err().code,
        "ownership_changed"
    );
    assert_eq!(fixture.checks.get(), 2);
    let mut fixture = Fixture::new();
    fixture.torn = true;
    assert_eq!(game::clock_context(&fixture).unwrap_err().code, "torn_read");
}

#[test]
fn invalid_final_pause_byte_cannot_masquerade_as_false() {
    let mut fixture = Fixture::new();
    fixture.put(0x326c1, &[0]);
    fixture.invalid_final_pause = true;
    assert_eq!(game::clock_context(&fixture).unwrap_err().code, "torn_read");
}

#[test]
fn unchanged_string_header_does_not_hide_changed_campaign_contents() {
    let mut fixture = Fixture::new();
    fixture.changed_id = true;
    assert_eq!(game::clock_context(&fixture).unwrap_err().code, "torn_read");
    let mut fixture = Fixture::new();
    fixture.number(0x30720, 4097);
    assert_eq!(
        game::clock_context(&fixture).unwrap_err().code,
        "invalid_string"
    );
}

#[test]
fn technology_catalog_filters_pages_and_uses_primary_ids() {
    let mut fixture = Fixture::new();
    fixture.catalog();
    let first = technology::catalog(&fixture, "TOOLS", 0, 1).unwrap();
    assert_eq!(first["definition_count"], 3);
    assert_eq!(first["count"], 2);
    assert_eq!(first["technologies"], serde_json::json!(["tools1"]));
    assert_eq!(first["next_offset"], 1);
    assert_eq!(fixture.checks.get(), 2);
    let next = technology::catalog(&fixture, "tools", 1, 1).unwrap();
    assert_eq!(next["technologies"], serde_json::json!(["tools2"]));
    assert!(next["next_offset"].is_null());
    assert!(
        technology::catalog(&fixture, "no_match", 0, 100).unwrap()["technologies"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn malformed_and_changed_catalogs_return_no_page() {
    let mut fixture = Fixture::new();
    fixture.catalog();
    fixture.number(0x63030, 2);
    assert_eq!(
        technology::catalog(&fixture, "", 0, 100).unwrap_err().code,
        "invalid_catalog"
    );
    let mut fixture = Fixture::new();
    fixture.catalog();
    fixture.string(0x63108, "tools1");
    assert_eq!(
        technology::catalog(&fixture, "", 0, 100).unwrap_err().code,
        "invalid_catalog"
    );
    let mut fixture = Fixture::new();
    fixture.catalog();
    fixture.changed_catalog = true;
    assert_eq!(
        technology::catalog(&fixture, "", 0, 100).unwrap_err().code,
        "torn_read"
    );
    let mut fixture = Fixture::new();
    fixture.catalog();
    let name = b"long_technology_identifier";
    fixture.region(0x63008, 32);
    fixture.word(0x63008, 0x64000);
    fixture.word(0x63010, name.len() as u64);
    fixture.put(0x64000, name);
    fixture.changed_catalog_name = true;
    assert_eq!(
        technology::catalog(&fixture, "", 0, 100).unwrap_err().code,
        "torn_read"
    );
    let mut fixture = Fixture::new();
    fixture.catalog();
    fixture.replaced = true;
    assert_eq!(
        technology::catalog(&fixture, "", 0, 100).unwrap_err().code,
        "ownership_changed"
    );
}

#[test]
fn oversized_catalog_is_refused_before_pointer_payload() {
    let mut fixture = Fixture::new();
    fixture.catalog();
    fixture.array(0x36048, 0x62000, 8193);
    assert_eq!(
        technology::catalog(&fixture, "", 0, 100).unwrap_err().code,
        "invalid_array"
    );
    assert_eq!(fixture.catalog_reads.get(), 0);
    let mut fixture = Fixture::new();
    fixture.catalog();
    fixture.region(0x63008, 32);
    fixture.word(0x63008, 0x64000);
    fixture.word(0x63010, 257);
    assert_eq!(
        technology::catalog(&fixture, "", 0, 100).unwrap_err().code,
        "invalid_string"
    );
    assert_eq!(fixture.catalog_name_reads.get(), 0);
}

#[test]
fn relocated_catalog_with_identical_entries_is_a_changed_snapshot() {
    let mut fixture = Fixture::new();
    fixture.catalog();
    let bytes: Vec<_> = (0..32).map(|i| fixture.data[&(0x62000 + i)]).collect();
    fixture.put(0x62100, &bytes);
    fixture.relocated_catalog = true;
    assert_eq!(
        technology::catalog(&fixture, "", 0, 100).unwrap_err().code,
        "torn_read"
    );
}

#[test]
fn input_geometry_is_diagnostic_and_uses_finalized_ratios() {
    let mut fixture = Fixture::new();
    fixture.geometry();
    let value = gui::input_geometry(&fixture).unwrap();
    assert_eq!(value["physical_dimensions"], serde_json::json!([512, 288]));
    assert_eq!(value["logical_dimensions"], serde_json::json!([1280, 720]));
    assert_eq!(value["logical_pointer"], serde_json::json!([640.0, 20.0]));
    assert_eq!(value["input_mapping_verified"], false);
    assert_eq!(fixture.checks.get(), 2);
}

#[test]
fn input_source_metadata_never_implies_receiver_payload_layout() {
    let mut fixture = Fixture::new();
    fixture.geometry();
    let base = 0x10000;
    fixture.word(0x32500, 0xe0000);
    fixture.word(0xe0000, base + 0x435cee0);
    fixture.word(base + 0x435cee0 - 16, 0);
    fixture.word(base + 0x435cee0 - 8, base + 0x435c100);
    fixture.word(base + 0x435c108, base + 0x350c000);
    fixture.region(base + 0x350c000, 128);
    fixture.put(base + 0x350c000, b"9CPdxMouse");
    fixture.word(base + 0x435cee0 + 0xc8, 0); // explicitly unsupported getter
    let value = gui::input_geometry(&fixture).unwrap();
    assert_eq!(value["input_source"]["type_encoding"], "9CPdxMouse");
    assert_eq!(value["input_source"]["field_layout_verified"], false);
    assert_eq!(value["input_source"]["client_origin_verified"], false);
    fixture.assert_not_read(&[0xe0008, 0xe00c8, 0xe014a]);
    fixture.word(base + 0x435cee0 - 16, 8); // secondary primary is not accepted
    let value = gui::input_geometry(&fixture).unwrap();
    assert_eq!(value["input_source"]["type_metadata_available"], false);
    assert_eq!(
        value["input_source"]["type_encoding"],
        serde_json::Value::Null
    );
    assert_eq!(value["input_mapping_verified"], false);
}

#[test]
fn input_source_identity_and_metadata_are_final_snapshot_witnesses() {
    for address in [0x32500, 0xe0000, 0x10000 + 0x350c000] {
        let mut fixture = Fixture::new();
        fixture.geometry();
        let base = 0x10000;
        fixture.word(0x32500, 0xe0000);
        fixture.word(0xe0000, base + 0x435cee0);
        fixture.word(base + 0x435cee0 - 16, 0);
        fixture.word(base + 0x435cee0 - 8, base + 0x435c100);
        fixture.word(base + 0x435c108, base + 0x350c000);
        fixture.region(base + 0x350c000, 128);
        fixture.put(base + 0x350c000, b"9CPdxMouse");
        fixture.word(base + 0x435cee0 + 0xc8, 0); // explicitly unsupported getter
        let mut bytes: Vec<_> = (0..if address == base + 0x350c000 { 128 } else { 8 })
            .map(|i| fixture.data[&(address + i)])
            .collect();
        bytes[0] ^= 1;
        fixture.fault = Some((address, 1, bytes));
        assert_eq!(gui::input_geometry(&fixture).unwrap_err().code, "torn_read");
    }
}

impl Fixture {
    fn builtin_mouse_point(&mut self) {
        self.geometry();
        let base = 0x10000;
        self.word(0x32500, 0xe0000);
        self.word(0xe0000, base + 0x435cee0);
        self.word(base + 0x435cee0 - 16, 0);
        self.word(base + 0x435cee0 - 8, base + 0x435c100);
        self.word(base + 0x435c108, base + 0x350c000);
        self.region(base + 0x350c000, 128);
        self.put(base + 0x350c000, b"9CPdxMouse");
        self.word(base + 0x435cee0 + 0xc8, base + 0x2fad780);
        self.word(base + 0x4391098, base + 0x301a3a0);
        self.number(base + 0x463b960, 256);
        self.number(base + 0x463b964, 8);
        self.word(base + 0x463bbf8, 0); // no inferred window association
    }

    fn builtin_mouse_window(&mut self) {
        self.builtin_mouse_point();
        // Constructor/accessor-proved fields only. Sparse objects intentionally
        // have no SDL display, list links or unrelated window payload.
        let base = 0x10000;
        self.word(base + 0x463bbf8, MOUSE_VIDEO);
        self.word(MOUSE_VIDEO + 0x48, base + 0x307d430);
        self.word(MOUSE_VIDEO + 0x58, base + 0x307e0d0);
        self.word(MOUSE_VIDEO + 0x228, base + 0x30f5d30);
        self.word(base + 0x463b958, MOUSE_WINDOW);
        self.word(MOUSE_WINDOW, MOUSE_VIDEO + 0x2d8);
        self.number(MOUSE_WINDOW + 0x28, 512);
        self.number(MOUSE_WINDOW + 0x2c, 288);
        self.word(MOUSE_VIDEO + 0x5a8, MOUSE_DRIVER);
        self.word(MOUSE_WINDOW + 0xd8, MOUSE_DATA);
        self.word(MOUSE_DATA, MOUSE_WINDOW);
        self.word(MOUSE_DATA + 0xe0, MOUSE_DRIVER);
        self.word(MOUSE_DATA + 8, 0x4a0007);
        self.number(MOUSE_DRIVER + 0x2c, 3);
        self.word(MOUSE_DRIVER + 0x30, MOUSE_ENTRIES);
        self.number(MOUSE_DRIVER + 0x38, 3);
        self.word(MOUSE_ENTRIES, 0xdead0000);
        self.word(MOUSE_ENTRIES + 8, MOUSE_DATA);
        self.word(MOUSE_ENTRIES + 16, u64::MAX);
        self.number(base + 0x463b984, 0);
        self.number(base + 0x463b988, 0);
    }

    fn forbid_bytes(&mut self, address: u64, size: usize) {
        for offset in 0..size {
            self.data.remove(&(address + offset as u64));
        }
    }
}

const MOUSE_VIDEO: u64 = 0x150000;
const MOUSE_WINDOW: u64 = 0x160000;
const MOUSE_DRIVER: u64 = 0x170000;
const MOUSE_DATA: u64 = 0x180000;
const MOUSE_ENTRIES: u64 = 0x190000;

#[test]
fn builtin_mouse_window_reports_selected_metadata_without_input_authority() {
    let mut f = Fixture::new();
    f.builtin_mouse_window();
    let v = gui::input_geometry(&f).unwrap();
    let point = &v["input_source"]["point_diagnostic"];
    let window = &point["window_association"];
    assert_eq!(window["association_available"], true);
    assert_eq!(window["native_window_id"], 0x4a0007);
    assert_eq!(window["window_dimensions"], serde_json::json!([512, 288]));
    assert_eq!(window["graphics_dimensions_match"], true);
    assert_eq!(window["relative_mode_raw"], serde_json::json!([0, 0]));
    assert_eq!(window["normal_absolute_mode"], true);
    for key in [
        "owned_os_window_verified",
        "client_origin_verified",
        "fresh_event_verified",
    ] {
        assert_eq!(window[key], false, "Association cannot grant {key}");
    }
    assert_eq!(point["fresh_event_verified"], false);
    assert_eq!(v["input_source"]["field_layout_verified"], false);
    assert_eq!(v["input_source"]["client_origin_verified"], false);
    assert_eq!(v["input_mapping_verified"], false);
    assert_eq!(v["observation_paused_game"], false);
    assert!(window.get("input_authorized").is_none());
    assert_eq!(f.checks.get(), 2);
    // A complete membership scan is allowed; neither opaque display nor the
    // poisoned foreign/noncanonical entries may become a payload traversal.
    f.assert_not_read(&[
        MOUSE_DRIVER,
        MOUSE_VIDEO + 0x2c8,
        MOUSE_WINDOW + 0xe0,
        MOUSE_WINDOW + 0xe8,
        MOUSE_DATA + 0x10,
        0xdead0000,
        u64::MAX,
        0xe014a,
    ]);
}

#[test]
fn builtin_mouse_window_preserves_raw_relative_flags_and_dimension_mismatch() {
    for flags in [[1, 0], [0, 1], [-7, i32::MAX]] {
        let mut f = Fixture::new();
        f.builtin_mouse_window();
        f.number(0x10000 + 0x463b984, flags[0]);
        f.number(0x10000 + 0x463b988, flags[1]);
        f.number(MOUSE_WINDOW + 0x28, 513);
        let v = gui::input_geometry(&f).unwrap();
        let window = &v["input_source"]["point_diagnostic"]["window_association"];
        assert_eq!(window["association_available"], true);
        assert_eq!(window["relative_mode_raw"], serde_json::json!(flags));
        assert_eq!(window["normal_absolute_mode"], false);
        assert_eq!(window["graphics_dimensions_match"], false);
        assert_eq!(window["owned_os_window_verified"], false);
        assert_eq!(v["input_mapping_verified"], false);
    }
}

#[test]
fn builtin_mouse_window_is_not_read_for_an_unaccepted_point_getter() {
    for address in [0x10000 + 0x435cee0 + 0xc8, 0x10000 + 0x4391098] {
        let mut f = Fixture::new();
        f.builtin_mouse_window();
        f.word(address, 0xdead0000);
        f.forbid_bytes(0x10000 + 0x463bbf8, 8);
        let v = gui::input_geometry(&f).unwrap();
        let point = &v["input_source"]["point_diagnostic"];
        assert_eq!(point["getter_source_layout_verified"], false);
        assert!(point.get("window_association").is_none());
        f.assert_not_read(&[
            0x10000 + 0x463bbf8,
            0x10000 + 0x463b958,
            MOUSE_VIDEO + 0x48,
            MOUSE_WINDOW,
            MOUSE_DRIVER + 0x2c,
            MOUSE_DATA,
            MOUSE_ENTRIES,
        ]);
    }
}

#[test]
fn unsupported_mouse_window_driver_does_not_read_window_or_driver_payload() {
    for offset in [0x48, 0x58, 0x228] {
        let mut f = Fixture::new();
        f.builtin_mouse_window();
        f.word(MOUSE_VIDEO + offset, 0xdead0000);
        f.forbid_bytes(0x10000 + 0x463b958, 8);
        f.forbid_bytes(MOUSE_VIDEO + 0x5a8, 8);
        let v = gui::input_geometry(&f).unwrap();
        let window = &v["input_source"]["point_diagnostic"]["window_association"];
        assert_eq!(window["association_available"], false);
        assert_eq!(window["reason"], "x11_driver_layout_unavailable");
        assert_eq!(window["owned_os_window_verified"], false);
        assert!(window.get("native_window_id").is_none());
        f.assert_not_read(&[
            0x10000 + 0x463b958,
            MOUSE_VIDEO + 0x5a8,
            MOUSE_WINDOW,
            MOUSE_DATA,
            MOUSE_ENTRIES,
        ]);
    }
}

#[test]
fn absent_mouse_window_focus_or_video_is_unavailable_without_payload_reads() {
    for (address, reason) in [
        (0x10000 + 0x463bbf8, "active_video_unavailable"),
        (0x10000 + 0x463b958, "input_window_focus_unavailable"),
    ] {
        let mut f = Fixture::new();
        f.builtin_mouse_window();
        f.word(address, 0);
        f.forbid_bytes(MOUSE_WINDOW, 8);
        let v = gui::input_geometry(&f).unwrap();
        let window = &v["input_source"]["point_diagnostic"]["window_association"];
        assert_eq!(window["association_available"], false);
        assert_eq!(window["reason"], reason);
        f.assert_not_read(&[MOUSE_WINDOW, MOUSE_DATA, MOUSE_ENTRIES]);
        if address == 0x10000 + 0x463bbf8 {
            f.assert_not_read(&[MOUSE_VIDEO + 0x48, 0x10000 + 0x463b958]);
        }
    }
}

#[test]
fn mouse_window_magic_and_reciprocal_links_refuse_before_registry_payload() {
    for address in [MOUSE_WINDOW, MOUSE_DATA, MOUSE_DATA + 0xe0] {
        let mut f = Fixture::new();
        f.builtin_mouse_window();
        f.word(address, 0xdead0000);
        f.forbid_bytes(MOUSE_DRIVER + 0x2c, 4);
        assert_eq!(
            gui::input_geometry(&f).unwrap_err().code,
            "invalid_input_source",
            "Broken association at {address:x}"
        );
        f.assert_not_read(&[MOUSE_DRIVER + 0x2c, MOUSE_ENTRIES, MOUSE_DATA + 8]);
        if address == MOUSE_WINDOW {
            f.assert_not_read(&[
                MOUSE_WINDOW + 0x28,
                MOUSE_WINDOW + 0x2c,
                MOUSE_WINDOW + 0xd8,
                MOUSE_VIDEO + 0x5a8,
            ]);
        }
    }
}

#[test]
fn mouse_window_registry_accepts_the_full_128_entry_policy_boundary() {
    let mut f = Fixture::new();
    f.builtin_mouse_window();
    f.number(MOUSE_DRIVER + 0x2c, 128);
    f.number(MOUSE_DRIVER + 0x38, 128);
    for index in 0..128 {
        f.word(
            MOUSE_ENTRIES + index * 8,
            if index == 127 { MOUSE_DATA } else { u64::MAX },
        );
    }
    let v = gui::input_geometry(&f).unwrap();
    let window = &v["input_source"]["point_diagnostic"]["window_association"];
    assert_eq!(window["association_available"], true);
    assert_eq!(window["native_window_id"], 0x4a0007);
    f.assert_not_read(&[u64::MAX]);
}

#[test]
fn mouse_window_registry_oversize_is_unavailable_without_array_or_id_reads() {
    for count in [1, 129, i32::MAX] {
        let mut f = Fixture::new();
        f.builtin_mouse_window();
        f.number(MOUSE_DRIVER + 0x2c, count);
        f.number(MOUSE_DRIVER + 0x38, count.max(129));
        f.forbid_bytes(MOUSE_ENTRIES, 24);
        f.forbid_bytes(MOUSE_DATA + 8, 8);
        let v = gui::input_geometry(&f).unwrap();
        let window = &v["input_source"]["point_diagnostic"]["window_association"];
        assert_eq!(window["association_available"], false);
        assert_eq!(window["reason"], "input_window_registry_policy_bound");
        assert!(window.get("native_window_id").is_none());
        f.assert_not_read(&[
            MOUSE_ENTRIES,
            MOUSE_DATA + 8,
            0x10000 + 0x463b984,
            0x10000 + 0x463b988,
        ]);
    }
}

#[test]
fn mouse_window_registry_rejects_malformed_headers_before_oversize_policy() {
    for (count, capacity, entries) in [
        (0, 129, MOUSE_ENTRIES),
        (-1, 129, MOUSE_ENTRIES),
        (130, 129, MOUSE_ENTRIES),
        (1, -1, MOUSE_ENTRIES),
        (1, 129, 0),
        (1, 129, 0xffff),
        (1, 129, u64::MAX),
        (1, 129, 0x7fff_ffff_0008),
        (1, 129, MOUSE_ENTRIES + 1),
    ] {
        let mut f = Fixture::new();
        f.builtin_mouse_window();
        f.number(MOUSE_DRIVER + 0x2c, count);
        f.number(MOUSE_DRIVER + 0x38, capacity);
        f.word(MOUSE_DRIVER + 0x30, entries);
        f.forbid_bytes(MOUSE_ENTRIES, 24);
        assert_eq!(
            gui::input_geometry(&f).unwrap_err().code,
            "invalid_input_source",
            "Malformed registry ({count}, {capacity}, {entries:x})"
        );
        f.assert_not_read(&[entries, MOUSE_ENTRIES, MOUSE_DATA + 8]);
    }
}

#[test]
fn mouse_window_registry_requires_exactly_one_selected_entry_before_native_id() {
    for entries in [
        [0xdead0000, u64::MAX, 0xdead0008],
        [MOUSE_DATA, 0xdead0000, MOUSE_DATA],
    ] {
        let mut f = Fixture::new();
        f.builtin_mouse_window();
        for (index, entry) in entries.iter().enumerate() {
            f.word(MOUSE_ENTRIES + index as u64 * 8, *entry);
        }
        f.forbid_bytes(MOUSE_DATA + 8, 8);
        assert_eq!(
            gui::input_geometry(&f).unwrap_err().code,
            "invalid_input_source"
        );
        f.assert_not_read(&[MOUSE_DATA + 8, 0xdead0000, u64::MAX, 0xdead0008]);
    }
    let mut f = Fixture::new();
    f.builtin_mouse_window();
    // A readable prefix containing the selected member cannot substitute for
    // the required complete array, even when its own reciprocal links match.
    f.word(MOUSE_ENTRIES, MOUSE_DATA);
    f.forbid_bytes(MOUSE_ENTRIES + 16, 8);
    assert_eq!(gui::input_geometry(&f).unwrap_err().code, "unexpected_read");
    f.assert_not_read(&[MOUSE_DATA + 8]);
}

#[test]
fn mouse_window_dimensions_and_native_xid_enforce_their_observation_bounds() {
    for dimension in [0, -1, 16385, i32::MIN, i32::MAX] {
        for offset in [0x28, 0x2c] {
            let mut f = Fixture::new();
            f.builtin_mouse_window();
            f.number(MOUSE_WINDOW + offset, dimension);
            f.forbid_bytes(MOUSE_VIDEO + 0x5a8, 8);
            assert_eq!(
                gui::input_geometry(&f).unwrap_err().code,
                "invalid_input_source"
            );
            f.assert_not_read(&[MOUSE_VIDEO + 0x5a8, MOUSE_ENTRIES, MOUSE_DATA + 8]);
        }
    }
    for xid in [0, u32::MAX as u64 + 1, u64::MAX] {
        let mut f = Fixture::new();
        f.builtin_mouse_window();
        f.word(MOUSE_DATA + 8, xid);
        f.forbid_bytes(0x10000 + 0x463b984, 4);
        assert_eq!(
            gui::input_geometry(&f).unwrap_err().code,
            "invalid_input_source"
        );
        f.assert_not_read(&[0x10000 + 0x463b984, 0x10000 + 0x463b988]);
    }
    for dimension in [1, 16384] {
        let mut f = Fixture::new();
        f.builtin_mouse_window();
        f.number(MOUSE_WINDOW + 0x28, dimension);
        f.number(MOUSE_WINDOW + 0x2c, dimension);
        f.word(MOUSE_DATA + 8, u32::MAX as u64);
        let v = gui::input_geometry(&f).unwrap();
        let window = &v["input_source"]["point_diagnostic"]["window_association"];
        assert_eq!(window["association_available"], true);
        assert_eq!(window["native_window_id"], u32::MAX);
        assert_eq!(window["graphics_dimensions_match"], false);
        assert_eq!(window["owned_os_window_verified"], false);
    }
}

#[test]
fn mouse_window_pointers_refuse_noncanonical_and_overflowing_dereferences() {
    for address in [
        0x10000 + 0x463bbf8,
        0x10000 + 0x463b958,
        MOUSE_VIDEO + 0x5a8,
        MOUSE_WINDOW + 0xd8,
        MOUSE_DRIVER + 0x30,
    ] {
        for pointer in [0xffff, 0x7fff_ffff_0008, u64::MAX - 7, u64::MAX] {
            let mut f = Fixture::new();
            f.builtin_mouse_window();
            f.word(address, pointer);
            assert_eq!(
                gui::input_geometry(&f).unwrap_err().code,
                "invalid_input_source",
                "Pointer {pointer:x} from field {address:x}"
            );
            f.assert_not_read(&[pointer]);
        }
    }
}

#[test]
fn mouse_window_revalidates_every_required_association_field_and_array_byte() {
    for (address, size) in [
        (0x10000 + 0x463bbf8, 8),
        (MOUSE_VIDEO + 0x48, 8),
        (MOUSE_VIDEO + 0x58, 8),
        (MOUSE_VIDEO + 0x228, 8),
        (0x10000 + 0x463b958, 8),
        (MOUSE_WINDOW, 8),
        (MOUSE_WINDOW + 0x28, 4),
        (MOUSE_WINDOW + 0x2c, 4),
        (MOUSE_VIDEO + 0x5a8, 8),
        (MOUSE_WINDOW + 0xd8, 8),
        (MOUSE_DATA, 8),
        (MOUSE_DATA + 0xe0, 8),
        (MOUSE_DRIVER + 0x2c, 4),
        (MOUSE_DRIVER + 0x30, 8),
        (MOUSE_DRIVER + 0x38, 4),
        (MOUSE_DATA + 8, 8),
        (0x10000 + 0x463b984, 4),
        (0x10000 + 0x463b988, 4),
    ] {
        let mut f = Fixture::new();
        f.builtin_mouse_window();
        let mut bytes: Vec<_> = (0..size).map(|i| f.data[&(address + i)]).collect();
        bytes[0] ^= 1;
        f.fault = Some((address, 1, bytes));
        assert_eq!(
            gui::input_geometry(&f).unwrap_err().code,
            "torn_read",
            "Changed required association field {address:x}"
        );
        assert!(f.fault_reads.get() >= 2);
    }
    // Mutating an unrelated member changes the complete membership witness
    // even while the selected pointer remains present exactly once.
    for byte in [0, 8, 16, 23] {
        let mut f = Fixture::new();
        f.builtin_mouse_window();
        let mut bytes: Vec<_> = (0..24).map(|i| f.data[&(MOUSE_ENTRIES + i)]).collect();
        bytes[byte] ^= 1;
        f.fault = Some((MOUSE_ENTRIES, 1, bytes));
        assert_eq!(gui::input_geometry(&f).unwrap_err().code, "torn_read");
    }
}

#[test]
fn mouse_window_metadata_still_requires_the_final_process_ownership_check() {
    let mut f = Fixture::new();
    f.builtin_mouse_window();
    f.replaced = true;
    assert_eq!(
        gui::input_geometry(&f).unwrap_err().code,
        "ownership_changed"
    );
    assert_eq!(f.checks.get(), 2);
}

#[test]
fn builtin_point_source_proves_arithmetic_but_never_origin_or_fresh_event() {
    let mut f = Fixture::new();
    f.builtin_mouse_point();
    let v = gui::input_geometry(&f).unwrap();
    let p = &v["input_source"]["point_diagnostic"];
    assert_eq!(p["getter_source_layout_verified"], true);
    assert_eq!(p["source_point_raw"], serde_json::json!([256, 8]));
    assert_eq!(p["source_logical_point"], serde_json::json!([640.0, 20.0]));
    assert_eq!(p["logical_cache_equivalent"], true);
    assert_eq!(p["fresh_event_verified"], false);
    assert_eq!(v["input_source"]["client_origin_verified"], false);
    assert_eq!(v["input_mapping_verified"], false);
    f.assert_not_read(&[0xe014a, 0xe00c8, 0x10000 + 0x463b958]);
    // Signed source coordinates and float32 truncation, rather than integer
    // products or config-only scale, follow the independently decoded consumer.
    f.number(0x10000 + 0x463b960, -3);
    f.number(0x10000 + 0x463b964, 7);
    let v = gui::input_geometry(&f).unwrap();
    assert_eq!(
        v["input_source"]["point_diagnostic"]["source_logical_point"],
        serde_json::json!([-7.0, 17.0])
    );
    assert_eq!(
        v["input_source"]["point_diagnostic"]["logical_cache_equivalent"],
        false
    );
}
#[test]
fn unsupported_point_getter_or_dynamic_replacement_never_reads_global_payload() {
    let base = 0x10000;
    for (address, reason) in [
        (base + 0x435cee0 + 0xc8, "receiver_getter_unavailable"),
        (base + 0x4391098, "builtin_point_source_unavailable"),
    ] {
        let mut f = Fixture::new();
        f.builtin_mouse_point();
        f.word(address, 0xdead0000);
        let v = gui::input_geometry(&f).unwrap();
        assert_eq!(
            v["input_source"]["point_diagnostic"]["getter_source_layout_verified"],
            false
        );
        assert_eq!(v["input_source"]["point_diagnostic"]["reason"], reason);
        f.assert_not_read(&[base + 0x463b960, base + 0x463b964]);
    }
}
#[test]
fn builtin_point_source_witnesses_slot_and_both_coordinates() {
    for (address, length) in [
        (0x10000 + 0x4391098, 8),
        (0x10000 + 0x463b960, 4),
        (0x10000 + 0x463b964, 4),
    ] {
        let mut f = Fixture::new();
        f.builtin_mouse_point();
        let mut replacement: Vec<_> = (0..length).map(|i| f.data[&(address + i)]).collect();
        replacement[0] ^= 1;
        f.fault = Some((address, 1, replacement));
        assert_eq!(gui::input_geometry(&f).unwrap_err().code, "torn_read");
    }
    let mut f = Fixture::new();
    f.builtin_mouse_point();
    f.number(0x10000 + 0x463b960, i32::MIN);
    assert_eq!(
        gui::input_geometry(&f).unwrap_err().code,
        "invalid_gui_geometry"
    );
}

#[test]
fn wrong_gui_type_inconsistent_scales_and_torn_geometry_fail() {
    let mut fixture = Fixture::new();
    fixture.geometry();
    fixture.word(0x70000, 0x10000 + 0x434c068);
    let refusal = gui::input_geometry(&fixture).unwrap_err();
    assert_eq!(refusal.code, "gui_unavailable");
    assert!(refusal.message.contains("CGameGui"));
    let mut fixture = Fixture::new();
    fixture.geometry();
    fixture.word(0x71000, 0x10000 + 0x43608e0);
    assert_eq!(
        gui::input_geometry(&fixture).unwrap_err().code,
        "gui_unavailable"
    );
    let mut fixture = Fixture::new();
    fixture.geometry();
    fixture.put(0x711d0, &3.0f32.to_le_bytes());
    assert_eq!(
        gui::input_geometry(&fixture).unwrap_err().code,
        "invalid_gui_geometry"
    );
    let mut fixture = Fixture::new();
    fixture.geometry();
    fixture.changed_geometry = true;
    assert_eq!(gui::input_geometry(&fixture).unwrap_err().code, "torn_read");
    let mut fixture = Fixture::new();
    fixture.geometry();
    fixture.replaced = true;
    assert_eq!(
        gui::input_geometry(&fixture).unwrap_err().code,
        "ownership_changed"
    );
}

#[test]
fn research_ui_reports_owned_widgets_and_diagnostic_types_without_input() {
    let mut f = Fixture::new();
    f.research_ui();
    let v = research_ui::snapshot(&f).unwrap();
    assert_eq!(v["own_slot_count"], 4);
    assert_eq!(v["widget_count"], 4);
    assert_eq!(v["selected_slot"], 1);
    assert_eq!(v["active_mode_raw"], 5);
    assert_eq!(v["panel"]["visible_predicate"], true);
    assert_eq!(v["ordinary_tree"]["materialized"], false);
    assert_eq!(
        v["slots"][0]["buttons_observed"][0]["type_encoding"],
        "15CButtonStandard"
    );
    assert_eq!(
        v["slots"][0]["buttons_observed"][0]["render_type_encoding"],
        "20CFrameAnimatedSprite"
    );
    assert_eq!(
        v["slots"][0]["buttons_observed"][0]["geometry_verified"],
        false
    );
    assert_eq!(v["input_authorized"], false);
    let renderer = &v["slots"][0]["buttons_observed"][0]["render_details"];
    assert_eq!(renderer["current_frame_raw"], 3);
    assert_eq!(renderer["resources_same"], true);
    assert_eq!(
        renderer["hit_resource_type_encoding"],
        "24CFrameAnimatedSpriteType"
    );
    assert_eq!(f.checks.get(), 2);
}
#[test]
fn research_ui_distinguishes_hidden_unmaterialized_and_incomplete_widgets() {
    let mut f = Fixture::new();
    f.research_ui();
    f.number(0xa0ef8, 2);
    assert_eq!(
        research_ui::snapshot(&f).unwrap()["panel"]["visible_predicate"],
        false
    );
    f.array(0x91308, 0, 0);
    f.word(0x92e08, 0);
    let v = research_ui::snapshot(&f).unwrap();
    assert_eq!(v["widget_count"], 0);
    assert_eq!(v["widgets_complete"], false);
    assert!(v["selected_slot"].is_null());
    assert_eq!(v["own_slot_count"], 4);
}
#[test]
fn research_ui_refuses_foreign_and_wrong_slot_or_view_joins() {
    for (address, value, code) in [
        (0x92e20, 2, "foreign_ui_context"),
        (0x90180, 0xdead0000, "invalid_gui_owner"),
        (0x922f0, 0xdead0000, "invalid_gui_owner"),
        (0x92e24, 7, "invalid_gui_owner"),
        (0x92e08, 0xdead0000, "invalid_research_owner"),
        (0x942b8, 0x2c100, "invalid_research_join"),
        (0x942b0, 0x92000, "invalid_research_join"),
        (0x942a8, 0xdead0000, "invalid_research_join"),
        (0xa0098, 0xdead0000, "invalid_gui_owner"),
        (0x93008, 0x94000, "invalid_research_join"),
        (0x2c010, 0xdead0000, "invalid_research_owner"),
    ] {
        let mut f = Fixture::new();
        f.research_ui();
        if [0x92e20, 0x92e24].contains(&address) {
            f.number(address, value as i32);
        } else {
            f.word(address, value);
        }
        assert_eq!(research_ui::snapshot(&f).unwrap_err().code, code);
    }
}
#[test]
fn research_ui_checks_counts_before_payload_and_rejects_secondary_metadata() {
    for (address, value, code) in [
        (0x91310, 33, "invalid_array"),
        (0xa0960, 65, "invalid_array"),
        (0x10000 + 0x43470c8 - 16, 8, "invalid_gui_type"),
        (0x10000 + 0x4350000 + 8, 0x90000, "invalid_gui_type"),
        (
            0x10000 + 0x43470c8 - 8,
            0x10000 + 0x439e7a0,
            "invalid_gui_type",
        ),
        (
            0x10000 + 0x4350000 + 8,
            0x10000 + 0x436f150,
            "invalid_gui_type",
        ),
    ] {
        let mut f = Fixture::new();
        f.research_ui();
        if [0x91310, 0xa0960].contains(&address) {
            f.number(address, value as i32);
        } else {
            f.word(address, value);
        }
        assert_eq!(research_ui::snapshot(&f).unwrap_err().code, code);
    }
}
#[test]
fn research_ui_checks_final_headers_payload_owners_visibility_and_types() {
    for (address, size, value) in [
        (0x326b8, 8, 0xdead0000u64.to_le_bytes().to_vec()),
        (0x90180, 8, 0xdead0000u64.to_le_bytes().to_vec()),
        (0x903d8, 4, 6i32.to_le_bytes().to_vec()),
        (0x93000, 32, vec![0; 32]),
        (0x942b8, 8, 0x2c100u64.to_le_bytes().to_vec()),
        (0x942a8, 8, 0xdead0000u64.to_le_bytes().to_vec()),
        (0xa0ef8, 4, 2i32.to_le_bytes().to_vec()),
        (0xb10ac, 4, 3i32.to_le_bytes().to_vec()),
        (0xb10e8, 8, 0u64.to_le_bytes().to_vec()),
        (0xb2034, 4, 1u32.to_le_bytes().to_vec()),
        (0xb2168, 8, 0u64.to_le_bytes().to_vec()),
        (0xb21b0, 8, 0u64.to_le_bytes().to_vec()),
        (0x10000 + 0x38b4750, 128, vec![0; 128]),
    ] {
        let mut f = Fixture::new();
        f.research_ui();
        f.fault = Some((address, if address == 0xa0ef8 { 2 } else { 1 }, value));
        assert!(size > 0);
        assert_eq!(
            research_ui::snapshot(&f).unwrap_err().code,
            "torn_read",
            "changed field {address:x}"
        );
    }
    let mut f = Fixture::new();
    f.research_ui();
    f.replaced = true;
    assert_eq!(
        research_ui::snapshot(&f).unwrap_err().code,
        "ownership_changed"
    );
}

#[test]
fn research_ui_unknown_primary_table_never_implies_button_layout() {
    let mut f = Fixture::new();
    f.research_ui();
    let table = 0x10000 + 0x4347400;
    f.word(0xb1000, table);
    f.word(table - 16, 0);
    f.word(table - 8, 0x10000 + 0x4350000);
    for (address, size) in [(0xb10ac, 4), (0xb10e8, 8)] {
        for n in 0..size {
            f.data.remove(&(address + n));
        }
    }
    let v = research_ui::snapshot(&f).unwrap();
    let button = &v["slots"][0]["buttons_observed"][0];
    assert_eq!(button["type_encoding"], "15CButtonStandard");
    assert!(button.get("disabled_raw").is_none());
    assert!(button.get("render_type_encoding").is_none());
}

#[test]
fn research_ui_exact_drag_uses_its_renderer_and_reports_state_without_enable_claim() {
    let mut f = Fixture::new();
    f.research_drag();
    let v = research_ui::snapshot(&f).unwrap();
    let button = &v["slots"][0]["buttons_observed"][0];
    assert_eq!(button["type_encoding"], "11CButtonDrag");
    assert_eq!(button["state_raw"], 4);
    assert!(button.get("disabled_raw").is_none());
    assert_eq!(button["render_type_encoding"], "20CFrameAnimatedSprite");
    assert_eq!(button["geometry_verified"], false);
    assert_eq!(v["input_authorized"], false);
    f.word(0xb1110, 0);
    assert!(
        research_ui::snapshot(&f).unwrap()["slots"][0]["buttons_observed"][0]
            ["render_type_encoding"]
            .is_null()
    );
}

#[test]
fn research_ui_drag_end_witnesses_refuse_state_or_renderer_changes() {
    for (address, value) in [
        (0xb10ac, 2i32.to_le_bytes().to_vec()),
        (0xb1110, 0u64.to_le_bytes().to_vec()),
    ] {
        let mut f = Fixture::new();
        f.research_drag();
        f.fault = Some((address, 1, value));
        assert_eq!(research_ui::snapshot(&f).unwrap_err().code, "torn_read");
    }
    let mut f = Fixture::new();
    f.research_drag();
    let table = 0x10000 + 0x4347400;
    f.word(0xb1000, table);
    f.word(table - 16, 0);
    f.word(table - 8, 0x10000 + 0x4350000);
    for (address, size) in [(0xb10ac, 4), (0xb1110, 8)] {
        for n in 0..size {
            f.data.remove(&(address + n));
        }
    }
    let v = research_ui::snapshot(&f).unwrap();
    let button = &v["slots"][0]["buttons_observed"][0];
    assert_eq!(button["type_encoding"], "11CButtonDrag");
    assert!(button.get("state_raw").is_none());
    assert!(button.get("render_type_encoding").is_none());
}

#[test]
fn research_ui_instant_and_unknown_renderers_do_not_borrow_frame_layout() {
    let mut f = Fixture::new();
    f.research_drag();
    let base = 0x10000;
    let table = base + 0x434c5b0;
    f.word(0xb2000, table);
    f.word(table - 16, 0);
    f.word(table - 8, base + 0x4351000);
    f.word(0xb2160, 0xb3000);
    for address in (0xb2168..0xb2170).chain(0xb21b0..0xb21b8) {
        f.data.remove(&address);
    }
    let v = research_ui::snapshot(&f).unwrap();
    let details = &v["slots"][0]["buttons_observed"][0]["render_details"];
    assert_eq!(details["current_frame_raw"], 3);
    assert_eq!(
        details["hit_resource_type_encoding"],
        "24CFrameAnimatedSpriteType"
    );
    assert!(details.get("resources_same").is_none());
    let table = base + 0x434b400;
    f.word(0xb2000, table);
    f.word(table - 16, 0);
    f.word(table - 8, base + 0x4351000);
    for (address, size) in [(0xb2034, 4), (0xb2160, 8)] {
        for n in 0..size {
            f.data.remove(&(address + n));
        }
    }
    let v = research_ui::snapshot(&f).unwrap();
    assert_eq!(
        v["slots"][0]["buttons_observed"][0]["render_details"]["field_layout_verified"],
        false
    );
}

#[test]
fn research_ui_plain_sprite_uses_direct_resource_without_other_renderer_fields() {
    let mut f = Fixture::new();
    f.research_plain_sprite();
    let v = research_ui::snapshot(&f).unwrap();
    let button = &v["slots"][0]["buttons_observed"][0];
    assert_eq!(button["render_type_encoding"], "7CSprite");
    let details = &button["render_details"];
    assert_eq!(details["current_frame_raw"], 3);
    assert_eq!(details["hit_resource_type_encoding"], "11CSpriteType");
    assert!(details.get("resources_same").is_none());
    assert_eq!(details["geometry_verified"], false);
    assert_eq!(v["input_authorized"], false);
    f.word(0xb2168, 0);
    assert!(
        research_ui::snapshot(&f).unwrap()["slots"][0]["buttons_observed"][0]["render_details"]
            ["hit_resource_type_encoding"]
            .is_null()
    );
}

#[test]
fn research_ui_plain_sprite_witnesses_changes_and_rejects_name_only_layouts() {
    for (address, value) in [
        (0xb2034, 4u32.to_le_bytes().to_vec()),
        (0xb2168, 0u64.to_le_bytes().to_vec()),
        (0xb3000, 0u64.to_le_bytes().to_vec()),
    ] {
        let mut f = Fixture::new();
        f.research_plain_sprite();
        f.fault = Some((address, 1, value));
        assert_eq!(research_ui::snapshot(&f).unwrap_err().code, "torn_read");
    }
    let mut f = Fixture::new();
    f.research_plain_sprite();
    let table = 0x10000 + 0x4356e80;
    f.word(0xb2000, table);
    f.word(table - 16, 0);
    f.word(table - 8, 0x10000 + 0x4351000);
    for (address, size) in [(0xb2034, 4), (0xb2168, 8)] {
        for n in 0..size {
            f.data.remove(&(address + n));
        }
    }
    let v = research_ui::snapshot(&f).unwrap();
    let button = &v["slots"][0]["buttons_observed"][0];
    assert_eq!(button["render_type_encoding"], "7CSprite");
    assert_eq!(button["render_details"]["field_layout_verified"], false);
}

#[test]
fn research_info_reports_canonical_selection_and_ordinary_button_without_eligibility() {
    let mut f = Fixture::new();
    f.research_info();
    let v = research_ui::snapshot(&f).unwrap();
    let infos = &v["technology_info"];
    assert_eq!(infos["count"], 1);
    assert_eq!(infos["visible_count"], 1);
    let info = &infos["windows"][0];
    assert_eq!(info["technology"], "tools1"); // Template+34 deliberately is 999.
    assert_eq!(info["eligibility"], "unavailable");
    let button = &info["ordinary_research_button"];
    assert_eq!(button["disabled_predicate"], true);
    assert_eq!(button["flags_raw"], 0x1234_0800u32);
    assert_eq!(button["selected_control_alias_matches"], true);
    assert_eq!(button["input_authorized"], false);
    f.word(0xc1ab0, 0xc9000); // Special branch alias is not ordinary control.
    f.number(0xc50ac, 0);
    f.number(0xc3eb0, 2);
    let v = research_ui::snapshot(&f).unwrap();
    assert_eq!(v["technology_info"]["visible_count"], 0);
    let button = &v["technology_info"]["windows"][0]["ordinary_research_button"];
    assert_eq!(button["disabled_predicate"], false);
    assert_eq!(button["selected_control_alias_matches"], false);
    assert_eq!(button["input_authorized"], false);
}

#[test]
fn research_info_retained_empty_selection_and_unmaterialized_window_are_explicit() {
    let mut f = Fixture::new();
    f.research_info();
    f.word(0xc1a88, 0);
    let v = research_ui::snapshot(&f).unwrap();
    assert_eq!(v["technology_info"]["windows"][0]["technology"], "tools1");
    assert_eq!(
        v["technology_info"]["windows"][0]["window"]["materialized"],
        false
    );
    assert_eq!(
        v["technology_info"]["windows"][0]["ordinary_research_button"]["materialized"],
        false
    );
    f.word(0xc1a98, 0);
    // Empty retained objects require neither a database nor a guessed control.
    for address in (0xc1ab0..0xc1ab8)
        .chain(0xc1ac0..0xc1ac8)
        .chain((0x10000 + 0x439f450)..(0x10000 + 0x439f458))
    {
        f.data.remove(&address);
    }
    let v = research_ui::snapshot(&f).unwrap();
    let info = &v["technology_info"]["windows"][0];
    assert!(info["technology"].is_null());
    assert!(info["ordinary_research_button"].is_null());
    assert_eq!(info["eligibility"], "unavailable");
}

#[test]
fn research_info_refuses_foreign_objects_wrong_catalog_and_misbound_controls() {
    for (address, value, code) in [
        (0xc1a78, 0xdead0000, "invalid_gui_owner"),
        (0xc1a80, 0xdead0000, "invalid_gui_owner"),
        (0xc3098, 0xdead0000, "invalid_gui_owner"),
        (0x80158, 0xdead0000, "invalid_research_owner"),
        (0x62008, 0x63100, "invalid_research_join"),
        (0x82000, 0x81000, "invalid_research_join"),
        (0xc1a98, 0x63000, "invalid_gui_type"),
        (0xc7008, 0xc8000, "invalid_gui_registry"),
        (0xc1ac0, 0xc8000, "invalid_gui_registry"),
        (0xc5000, 0x10000 + 0x4346b10, "invalid_gui_type"),
    ] {
        let mut f = Fixture::new();
        f.research_info();
        f.word(address, value);
        if address == 0x80158 {
            for a in 0x80160..0x80168 {
                f.data.remove(&a);
            }
        }
        if address == 0xc3098 {
            for a in 0xc39a0..0xc39c8 {
                f.data.remove(&a);
            }
        }
        assert_eq!(
            research_ui::snapshot(&f).unwrap_err().code,
            code,
            "{address:x}"
        );
    }
}

#[test]
fn research_info_bounds_and_unique_named_lookup_precede_untrusted_payloads() {
    let mut f = Fixture::new();
    f.research_info();
    f.array(0x92dc0, 0xdead0000, 65);
    assert_eq!(research_ui::snapshot(&f).unwrap_err().code, "invalid_array");
    let mut f = Fixture::new();
    f.research_info();
    f.array(0x92dc0, 0xc0000, 2);
    f.word(0xc0008, 0xc1000);
    assert_eq!(
        research_ui::snapshot(&f).unwrap_err().code,
        "invalid_gui_owner"
    );
    let mut f = Fixture::new();
    f.research_info();
    f.array(0xc39b8, 0xdead0000, 257);
    assert_eq!(
        research_ui::snapshot(&f).unwrap_err().code,
        "invalid_gui_registry"
    );
    let mut f = Fixture::new();
    f.research_info();
    f.string(0xc6000, "research");
    assert_eq!(
        research_ui::snapshot(&f).unwrap_err().code,
        "invalid_gui_registry"
    );
    let mut f = Fixture::new();
    f.research_info();
    f.string(0xc6020, "other");
    assert_eq!(
        research_ui::snapshot(&f).unwrap_err().code,
        "gui_unavailable"
    );
}

#[test]
fn research_info_end_witnesses_cover_selection_registry_controls_and_catalog() {
    for (address, value) in [
        (
            0x92dc0,
            [
                0u64.to_le_bytes().as_slice(),
                1i32.to_le_bytes().as_slice(),
                1i32.to_le_bytes().as_slice(),
            ]
            .concat(),
        ),
        (0xc0000, 0u64.to_le_bytes().to_vec()),
        (0xc1a78, 0u64.to_le_bytes().to_vec()),
        (0xc1a98, 0u64.to_le_bytes().to_vec()),
        (0xc1a88, 0u64.to_le_bytes().to_vec()),
        (0xc3098, 0u64.to_le_bytes().to_vec()),
        (
            0xc39b8,
            [
                0u64.to_le_bytes().as_slice(),
                2i32.to_le_bytes().as_slice(),
                2i32.to_le_bytes().as_slice(),
            ]
            .concat(),
        ),
        (0xc1ab0, 0u64.to_le_bytes().to_vec()),
        (0xc50ac, 0i32.to_le_bytes().to_vec()),
        (0xc5074, 0u32.to_le_bytes().to_vec()),
        (0x80158, 0u64.to_le_bytes().to_vec()),
        (0x80160, 0u64.to_le_bytes().to_vec()),
        (0x63030, 2i32.to_le_bytes().to_vec()),
    ] {
        let mut f = Fixture::new();
        f.research_info();
        f.fault = Some((address, 1, value));
        assert_eq!(
            research_ui::snapshot(&f).unwrap_err().code,
            "torn_read",
            "{address:x}"
        );
    }
}

struct UiWaitFixture {
    fixture: std::sync::Mutex<Fixture>,
    snapshots: std::sync::atomic::AtomicUsize,
    second_snapshot_changes: Vec<(u64, Vec<u8>)>,
}
impl ReadMemory for UiWaitFixture {
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        self.fixture.lock().unwrap().read(a, n)
    }
}
impl Process for UiWaitFixture {
    fn image_base(&self) -> u64 {
        0x10000
    }
    fn check(&self) -> Result<()> {
        self.fixture.lock().unwrap().check()
    }
    fn begin_snapshot(&self) -> Result<()> {
        let n = self
            .snapshots
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut f = self.fixture.lock().unwrap();
        if n == 1 {
            for (address, bytes) in &self.second_snapshot_changes {
                f.put(*address, bytes);
            }
        }
        f.check()
    }
}
fn ui_wait_reply(
    f: Fixture,
    changes: Vec<(u64, Vec<u8>)>,
    target: serde_json::Value,
    timeout: u64,
) -> serde_json::Value {
    let server = hoi4_live_native::protocol::Server {
        process: Some(Box::new(UiWaitFixture {
            fixture: std::sync::Mutex::new(f),
            snapshots: std::sync::atomic::AtomicUsize::new(0),
            second_snapshot_changes: changes,
        })),
        unavailable: None,
    };
    let reply = server.handle(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_wait_for","arguments":{"timeout_seconds":timeout,"until_research_ui":target}}})).unwrap();
    serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn ui_wait_observes_a_delayed_view_transition_without_input_or_process_pause() {
    let mut f = Fixture::new();
    f.research_wait();
    let changes = vec![
        (0x903d8, 6i32.to_le_bytes().to_vec()),
        (0xa00a5, vec![0]),
        (0x922f8, 0xa2000u64.to_le_bytes().to_vec()),
    ];
    let v = ui_wait_reply(
        f,
        changes,
        serde_json::json!({"view":"tree","selected_slot":1}),
        10,
    );
    assert_eq!(v["reasons"][0], "research_ui_reached");
    assert_eq!(v["state"]["selected_slot"], 1);
    assert_eq!(v["state"]["ordinary_tree"]["visible_predicate"], true);
    assert_eq!(v["state"]["input_authorized"], false);
    assert_eq!(v["state"]["observation_paused_game"], false);
}

#[test]
fn ui_wait_refuses_changed_slot_identity_even_when_every_own_join_is_valid() {
    let mut f = Fixture::new();
    f.research_wait();
    let mut slots = Vec::new();
    for slot in [0x2c100u64, 0x2c000, 0x2c200, 0x2c300] {
        slots.extend(slot.to_le_bytes());
    }
    let changes = vec![
        (0x3b000, slots),
        (0x942b8, 0x2c100u64.to_le_bytes().to_vec()),
        (0x952b8, 0x2c000u64.to_le_bytes().to_vec()),
    ];
    let v = ui_wait_reply(f, changes, serde_json::json!({"view":"tree"}), 10);
    assert_eq!(v["error"]["code"], "wait_context_changed");
    assert!(v.get("state").is_none());
}

#[test]
fn ui_wait_distinguishes_reached_predicates_unavailable_slots_and_wall_timeout() {
    let mut f = Fixture::new();
    f.research_wait();
    let v = ui_wait_reply(
        f,
        Vec::new(),
        serde_json::json!({"view":"panel","selected_slot":1}),
        10,
    );
    assert_eq!(v["reasons"][0], "research_ui_reached");
    let mut f = Fixture::new();
    f.research_wait();
    let v = ui_wait_reply(
        f,
        Vec::new(),
        serde_json::json!({"view":"panel","selected_slot":4}),
        10,
    );
    assert_eq!(v["error"]["code"], "invalid_arguments");
    let mut f = Fixture::new();
    f.research_wait();
    let v = ui_wait_reply(f, Vec::new(), serde_json::json!({"view":"tree"}), 1);
    assert_eq!(v["reasons"][0], "wall_timeout");
    assert_eq!(v["state"]["panel"]["visible_predicate"], true);
    assert!(v["state_age_ms"].as_u64().is_some());
    assert_eq!(v["state_historical"], true);
    assert_eq!(v["current_state_verified"], false);
    assert_eq!(v["input_authorized"], false);
    assert_eq!(
        v["state_role"],
        "last_snapshot_accepted_before_wait_deadline"
    );
}

#[test]
fn ui_wait_requires_positive_own_tag_before_any_country_payload_read() {
    let mut f = Fixture::new();
    f.research_wait();
    f.number(0x30518, 0);
    for address in 0x39000..0x3900c {
        f.data.remove(&address);
    }
    let v = ui_wait_reply(f, Vec::new(), serde_json::json!({"view":"tree"}), 10);
    assert_eq!(v["error"]["code"], "player_unavailable");
    assert!(v.get("state").is_none());
}

#[test]
fn game_wait_normal_timeout_labels_earlier_accepted_state_historical() {
    let server = hoi4_live_native::protocol::Server {
        process: Some(Box::new(UiWaitFixture {
            fixture: std::sync::Mutex::new(Fixture::new()),
            snapshots: std::sync::atomic::AtomicUsize::new(0),
            second_snapshot_changes: Vec::new(),
        })),
        unavailable: None,
    };
    let reply = server.handle(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":"hoi4_wait_for","arguments":{"timeout_seconds":1,"until_date":[1940,1,1]}}})).unwrap();
    let value: serde_json::Value =
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(value["reasons"], serde_json::json!(["wall_timeout"]));
    assert_eq!(value["state_historical"], true);
    assert_eq!(value["current_state_verified"], false);
    assert_eq!(value["input_authorized"], false);
    assert_eq!(value["state"]["owned"], true);
    assert!(value["state_age_ms"].as_u64().is_some());
    assert_eq!(value["state_age_basis"], "time_since_accepted_snapshot");
    assert_eq!(
        value["state_role"],
        "last_snapshot_accepted_before_wait_deadline"
    );
}

#[test]
fn current_node_tool_uses_canonical_public_keys_and_actual_snapshot_boundary() {
    let mut f = Fixture::new();
    f.current_research_nodes();
    let v = research_ui::nodes(&f).unwrap();
    assert_eq!(v["current_nodes"]["record_count"], 2);
    assert_eq!(v["current_nodes"]["node_count"], 1);
    assert_eq!(v["current_nodes"]["nodes"][0]["id"], "tools1");
    assert_eq!(v["current_nodes"]["nodes"][0]["definition_index"], 1);
    assert_eq!(v["selected_slot"], 1);
    assert_eq!(v["input_authorized"], false);
    assert_eq!(f.checks.get(), 2);
    for (address, size) in [
        (0xd2000, 80),
        (0xd3030, 8),
        (0xd30c8, 8),
        (0x63030, 4),
        (0x63008, 32),
    ] {
        let mut f = Fixture::new();
        f.current_research_nodes();
        let mut replacement: Vec<_> = (0..size).map(|i| f.data[&(address + i as u64)]).collect();
        replacement[0] ^= 1;
        f.fault = Some((address, 1, replacement));
        assert_eq!(research_ui::nodes(&f).unwrap_err().code, "torn_read");
    }
}

#[test]
fn current_node_tool_requires_open_own_tree_before_node_payload() {
    for mode in [5, 23] {
        let mut f = Fixture::new();
        f.current_research_nodes();
        f.number(0x903d8, mode);
        for address in 0x92dd8..0x92de8 {
            f.data.remove(&address);
        }
        assert_eq!(research_ui::nodes(&f).unwrap_err().code, "gui_unavailable");
    }
    let mut f = Fixture::new();
    f.current_research_nodes();
    f.word(0x80158, 0xdead0000);
    for address in 0x80160..0x80168 {
        f.data.remove(&address);
    }
    assert_eq!(
        research_ui::nodes(&f).unwrap_err().code,
        "invalid_research_owner"
    );
}

#[test]
fn current_node_pages_bound_output_without_skipping_full_validation() {
    let mut f = Fixture::new();
    f.current_research_nodes();
    let page = research_ui::nodes_page(&f, 0, 1, Some("tool")).unwrap();
    assert_eq!(page["current_nodes"]["node_count"], 1);
    assert_eq!(page["current_nodes"]["filtered_count"], 1);
    assert_eq!(page["current_nodes"]["nodes"][0]["id"], "tools1");
    let empty = research_ui::nodes_page(&f, 1, 1, None).unwrap();
    assert_eq!(empty["current_nodes"]["nodes"].as_array().unwrap().len(), 0);
    assert!(empty["current_nodes"]["next_offset"].is_null());
    assert_eq!(empty["current_nodes"]["node_count"], 1);
    f.word(0xd3030, 0x92008);
    assert_eq!(
        research_ui::nodes_page(&f, 1, 1, Some("absent"))
            .unwrap_err()
            .code,
        "invalid_gui_owner"
    );
}
#[test]
fn invalid_current_node_page_never_enters_process() {
    let f = Fixture::new();
    for (offset, limit, prefix) in [
        (513, 1, None),
        (0, 0, None),
        (0, 101, None),
        (0, 1, Some("bad;key")),
    ] {
        assert_eq!(
            research_ui::nodes_page(&f, offset, limit, prefix)
                .unwrap_err()
                .code,
            "invalid_arguments"
        );
    }
    assert_eq!(f.checks.get(), 0);
}

impl Fixture {
    fn node_binding(&mut self) {
        self.current_research_nodes();
        let base = 0x10000;
        self.word(0xd3010, 0xd4000);
        self.region(0xd4000, 0x980);
        self.word(0xd4000, base + 0x4347d58);
        self.word(0xd4098, 0x70000);
        self.word(base + 0x4347d58 - 16, 0);
        self.word(base + 0x4347d58 - 8, base + 0x4355100);
        self.region(base + 0x4355100, 16);
        self.word(base + 0x4355108, base + 0x350d100);
        self.region(base + 0x350d100, 128);
        self.put(base + 0x350d100, b"16CContainerWindow");
        self.array(0xd4958, 0xd4800, 1);
        self.word(0xd4800, 0xd5000);
        self.region(0xd5000, 0x120);
        self.word(0xd5000, base + 0x4346b10);
        self.word(base + 0x4346b10 - 16, 0);
        self.word(base + 0x4346b10 - 8, base + 0x4355000);
        self.region(base + 0x4355000, 16);
        self.word(base + 0x4355008, base + 0x350d000);
        self.region(base + 0x350d000, 128);
        self.put(base + 0x350d000, b"11CButtonDrag");
        self.word(0xd5008, 0x80000);
        self.word(0xd5068, 0x70000);
        self.word(0xd5080, base + 0x4346ea8);
        self.word(0xd5088, 0xd6000);
        self.word(0xd5090, 0xd6000);
        self.number(0xd5098, 1);
        self.region(0xd6000, 32);
        self.word(0xd6000, 0x925e0);
        self.word(0x925e0, base + 0x424e0a8);
        self.word(0x92608, 0xd7000);
        self.word(0x92618, base + 0x15a5710);
        self.word(0x92620, base + 0x15a56e0);
        self.region(0xd7000, 24);
        self.word(0xd7000, base + 0x1599290);
        self.word(0xd7010, 0x92000);
    }
}
#[test]
fn node_control_binding_is_conditional_and_pointer_free() {
    let mut f = Fixture::new();
    f.node_binding();
    let v = research_ui::nodes(&f).unwrap();
    let b = &v["current_nodes"]["nodes"][0]["ordinary_control_bindings"][0];
    assert_eq!(b["binding_verified"], true);
    assert_eq!(b["input_authorized"], false);
    assert_eq!(b["coordinates"], "unavailable");
    assert!(!v.to_string().contains(&0xd5000u64.to_string()));
}
#[test]
fn node_wrong_control_data_refuses_before_callback() {
    let mut f = Fixture::new();
    f.node_binding();
    f.word(0xd5008, 0x80008);
    f.data.remove(&0xd5080);
    assert_eq!(
        research_ui::nodes(&f).unwrap_err().code,
        "invalid_research_join"
    );
}
#[test]
fn node_override_and_wrong_callback_never_claim_binding() {
    for (a, v) in [
        (0xd50a0, 0xdead0000),
        (0x92620, 0x10000 + 0x1599290),
        (0xd7010, 0x92008),
    ] {
        let mut f = Fixture::new();
        f.node_binding();
        f.word(a, v);
        let v = research_ui::nodes(&f).unwrap();
        assert_eq!(
            v["current_nodes"]["nodes"][0]["ordinary_control_bindings"][0]["binding_verified"],
            false
        );
    }
}
#[test]
fn node_null_and_unknown_layout_are_diagnostic_only() {
    let mut f = Fixture::new();
    f.node_binding();
    f.word(0xd3010, 0);
    let v = research_ui::nodes(&f).unwrap();
    assert_eq!(
        v["current_nodes"]["nodes"][0]["ordinary_control_bindings"][0]["reason"],
        "gui_unmaterialized"
    );
    let mut f = Fixture::new();
    f.node_binding();
    f.word(0xd4000, 0x10000 + 0x4346b10);
    for a in 0xd4958..0xd4968 {
        f.data.remove(&a);
    }
    let v = research_ui::nodes(&f).unwrap();
    assert_eq!(
        v["current_nodes"]["nodes"][0]["ordinary_control_bindings"][0]["binding_verified"],
        false
    );
}
#[test]
fn node_binding_fields_use_production_final_witnesses() {
    for (a, n) in [
        (0xd4098, 8),
        (0xd5068, 8),
        (0xd5008, 8),
        (0xd5088, 8),
        (0xd5098, 4),
        (0xd6010, 8),
        (0x92620, 8),
        (0xd7010, 8),
    ] {
        let mut f = Fixture::new();
        f.node_binding();
        let mut replacement: Vec<_> = (0..n).map(|i| f.data[&(a + i)]).collect();
        replacement[0] ^= 1;
        f.fault = Some((a, 1, replacement));
        assert_eq!(research_ui::nodes(&f).unwrap_err().code, "torn_read");
    }
}
#[test]
fn node_malformed_listener_is_refused() {
    let mut f = Fixture::new();
    f.node_binding();
    f.word(0xd6010, 0xd6000);
    assert_eq!(
        research_ui::nodes(&f).unwrap_err().code,
        "invalid_gui_binding"
    );
}
#[test]
fn repeated_key_controls_retain_separate_binding_observations() {
    let mut f = Fixture::new();
    f.node_binding();
    f.region(0xd8000, 0xd0);
    f.word(0xd8000, 0x10000 + 0x424e2e0);
    f.word(0xd8030, 0x92000);
    f.word(0xd80c8, 0x80000);
    f.word(0xd8010, 0xd4000);
    f.word(0xd2038, 0xd8000);
    let v = research_ui::nodes(&f).unwrap();
    assert_eq!(v["current_nodes"]["node_count"], 1);
    assert_eq!(
        v["current_nodes"]["nodes"][0]["ordinary_control_bindings"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn binding_never_relaxes_key_or_node_owner() {
    let mut f = Fixture::new();
    f.node_binding();
    f.string(0x63008, "bad\nkey");
    assert_eq!(
        research_ui::nodes(&f).unwrap_err().code,
        "invalid_research_id"
    );
    let mut f = Fixture::new();
    f.node_binding();
    f.word(0xd3030, 0x92008);
    assert_eq!(
        research_ui::nodes(&f).unwrap_err().code,
        "invalid_gui_owner"
    );
}

#[test]
fn corrected_record_offset_and_stage_diagnostics_do_not_relax_types() {
    let mut f = Fixture::new();
    f.node_binding();
    // +32 decoys are deliberately unreadable objects; +16 is actual item.
    assert_eq!(
        research_ui::nodes(&f).unwrap()["current_nodes"]["node_count"],
        1
    );
    f.word(0xd3000, 0x10000 + 0x4347d58);
    let error = research_ui::nodes(&f).unwrap_err();
    assert_eq!(error.code, "node_item_type_unavailable");
    assert!(error.message.contains("CContainerWindow"));
    assert!(!error.message.contains("0xd3000"));
    let mut f = Fixture::new();
    f.node_binding();
    f.word(0x80000, 0x10000 + 0x4347d58);
    // Existing Info observation may refuse first; isolate current node helper
    // by an empty Info list, preserving production owner/snapshot guards.
    f.array(0x92dc0, 0, 0);
    assert_eq!(
        research_ui::nodes(&f).unwrap_err().code,
        "node_technology_type_unavailable"
    );
}

#[test]
fn node_foreign_registry_refused_before_unmapped_control_array() {
    let mut f = Fixture::new();
    f.node_binding();
    f.word(0xd4098, 0xdead0000);
    for address in 0xd4958..0xd4968 {
        f.data.remove(&address);
    }
    assert_eq!(
        research_ui::nodes(&f).unwrap_err().code,
        "invalid_gui_owner"
    );
}

#[test]
fn node_foreign_control_registry_refuses_before_data_render_or_callback() {
    let mut f = Fixture::new();
    f.node_binding();
    f.word(0xd5068, 0xdead0000);
    for (start, length) in [(0xd5008, 8), (0xd5080, 8), (0xd5110, 8)] {
        for address in start..start + length {
            f.data.remove(&address);
        }
    }
    assert_eq!(
        research_ui::nodes(&f).unwrap_err().code,
        "invalid_gui_owner"
    );
}

#[test]
fn request_reuse_backend_read_measurement() {
    let mut f = Fixture::new();
    f.node_binding();
    f.region(0xd8000, 0xd0);
    f.word(0xd8000, 0x10000 + 0x424e2e0);
    f.word(0xd8030, 0x92000);
    f.word(0xd80c8, 0x80000);
    f.word(0xd8010, 0xd4000);
    f.word(0xd2038, 0xd8000);
    let value = research_ui::nodes(&f).unwrap();
    assert_eq!(value["current_nodes"]["node_count"], 1);
    let reads = f.tracked_reads.borrow();
    println!("metadata_table_reads={} callback_manager_reads={} object_control_reads={} rpc_read_calls={}",reads[&(0x10000+0x4346b10-16)],reads[&0x92618],reads[&0xd5000],value["freshness"]["read_calls"]);
    // Measured unmodified baseline on this fixture: 3/3/5/404.
    assert_eq!(reads[&(0x10000 + 0x4346b10 - 16)], 2);
    assert_eq!(reads[&0x92618], 2);
    assert_eq!(reads[&0xd5000], 5);
    assert!(value["freshness"]["read_calls"].as_u64().unwrap() < 404);
}

impl Fixture {
    fn reused_two_controls(&mut self) {
        self.node_binding();
        for (from, to, size) in [
            (0xd4000, 0xd9000, 0x980),
            (0xd5000, 0xda000, 0x120),
            (0xd6000, 0xdb000, 32),
        ] {
            let bytes: Vec<_> = (0..size).map(|i| self.data[&(from + i)]).collect();
            self.put(to, &bytes);
        }
        self.array(0xd9958, 0xd9800, 1);
        self.word(0xd9800, 0xda000);
        self.word(0xda088, 0xdb000);
        self.word(0xda090, 0xdb000);
        self.region(0xd8000, 0xd0);
        self.word(0xd8000, 0x10000 + 0x424e2e0);
        self.word(0xd8030, 0x92000);
        self.word(0xd80c8, 0x80000);
        self.word(0xd8010, 0xd9000);
        self.word(0xd2038, 0xd8000);
    }
}
#[test]
fn reused_metadata_and_capture_keep_final_production_witnesses() {
    for (a, n) in [
        (0x10000 + 0x4346b10 - 16, 8),
        (0x10000 + 0x4355008, 8),
        (0x10000 + 0x350d000, 128),
        (0x92618, 8),
        (0x92620, 8),
        (0x92608, 8),
        (0xd7000, 8),
        (0xd7008, 8),
        (0xd7010, 8),
    ] {
        let mut f = Fixture::new();
        f.reused_two_controls();
        let mut replacement: Vec<_> = (0..n).map(|i| f.data[&(a + i)]).collect();
        replacement[0] ^= 1;
        f.fault = Some((a, 1, replacement));
        assert_eq!(
            research_ui::nodes(&f).unwrap_err().code,
            "torn_read",
            "field {a:x}"
        );
    }
}
#[test]
fn reused_proof_never_reuses_later_object_owner_or_control_data() {
    for (a, v, code) in [
        (0xd8030, 0x92008, "invalid_gui_owner"),
        (0xd9098, 0xdead0000, "invalid_gui_owner"),
        (0xda068, 0xdead0000, "invalid_gui_owner"),
        (0xda008, 0x80008, "invalid_research_join"),
        (0xd8000, 0x10000 + 0x4347d58, "node_item_type_unavailable"),
    ] {
        let mut f = Fixture::new();
        f.reused_two_controls();
        f.word(a, v);
        assert_eq!(research_ui::nodes(&f).unwrap_err().code, code);
    }
    let mut f = Fixture::new();
    f.reused_two_controls();
    f.word(0xda000, 0x10000 + 0x4347d58);
    let v = research_ui::nodes(&f).unwrap();
    assert_eq!(
        v["current_nodes"]["nodes"][0]["ordinary_control_bindings"][1]["binding_verified"],
        false
    );
}
#[test]
fn request_local_reuse_does_not_survive_requests() {
    let mut f = Fixture::new();
    f.reused_two_controls();
    research_ui::nodes(&f).unwrap();
    f.word(0xd7010, 0x92008);
    let v = research_ui::nodes(&f).unwrap();
    for b in v["current_nodes"]["nodes"][0]["ordinary_control_bindings"]
        .as_array()
        .unwrap()
    {
        assert_eq!(b["binding_verified"], false);
    }
}

impl Fixture {
    fn node_render_detail(&mut self) {
        self.node_binding();
        let base = 0x10000;
        self.word(0xd5110, 0xf0000);
        self.region(0xf0000, 0x180);
        self.region(0xf1000, 0x1f0);
        for (object, table, info, name, text) in [
            (0xf0000, 0x4356e08, 0x4356000, "7CSprite", 0x350e000),
            (0xf1000, 0x4357160, 0x4356100, "11CSpriteType", 0x350e100),
        ] {
            self.word(object, base + table);
            self.word(base + table - 16, 0);
            self.word(base + table - 8, base + info);
            self.region(base + info, 16);
            self.word(base + info + 8, base + text);
            self.region(base + text, 128);
            self.put(base + text, name.as_bytes());
        }
        self.word(0xf0138, 0xd5000);
        self.word(0xf0168, 0xf1000);
        for (off, v) in [
            (0x70, 12.0f32),
            (0x74, 18.0),
            (0xb0, 100.0),
            (0xb4, -80.0),
            (0x114, 1.0),
        ] {
            self.put(0xf0000 + off, &v.to_le_bytes());
        }
        self.number(0xf0120, 300);
        self.number(0xf0124, 60);
        self.put(0xf0144, &[1, 1]);
        self.number(0xf0150, 1024);
        self.number(0xf0154, 768);
        self.number(0xf1090, 2);
        self.number(0xf1094, 3);
        self.number(0xf10a4, 600);
        self.number(0xf10a8, 180);
        self.word(0xf11a0, u64::MAX);
        self.word(0xf1170, 0xdead0000);
        self.hierarchy_fixture();
    }
}
#[test]
fn one_exact_own_node_raw_geometry_never_exposes_client_rectangle() {
    let mut f = Fixture::new();
    f.node_render_detail();
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    assert_eq!(v["detail"]["distinct_bound_items"], 1);
    assert_eq!(
        v["detail"]["render_diagnostic"]["resource_cache"]["diagnostic_dimensions"],
        serde_json::json!([300, 60])
    );
    assert_eq!(
        v["detail"]["render_diagnostic"]["parent_chain"]["payload_hops"],
        3
    );
    assert_eq!(v["client_rectangle_available"], false);
    assert_eq!(v["input_authorized"], false);
    assert_eq!(
        v["own_info_postcondition"]["fresh_transition_verified"],
        false
    );
    assert!(!v.to_string().contains(&0xf0000u64.to_string()));
}
#[test]
fn full_own_model_checked_before_requested_geometry() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.region(0xd8000, 0xd0);
    f.word(0xd8000, 0x10000 + 0x424e2e0);
    f.word(0xd8030, 0x92008);
    f.word(0xd2038, 0xd8000);
    for address in 0xf0138..0xf0140 {
        f.data.remove(&address);
    }
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_owner"
    );
}
#[test]
fn distinct_bound_items_are_ambiguous_even_for_same_key() {
    let mut f = Fixture::new();
    f.reused_two_controls();
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "ambiguous_gui_node"
    );
}
#[test]
fn detail_exact_key_and_unavailable_branch_do_not_guess() {
    let mut f = Fixture::new();
    f.node_render_detail();
    assert_eq!(
        research_ui::node_detail(&f, "tools").unwrap_err().code,
        "node_unavailable"
    );
    f.word(0xf0000, 0x10000 + 0x4346b10);
    for a in 0xf0138..0xf0180 {
        f.data.remove(&a);
    }
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap()["detail"]["render_diagnostic"]
            ["field_layout_verified"],
        false
    );
}
#[test]
fn detail_geometry_final_witness_and_owned_renderer_are_required() {
    for (a, n) in [
        (0xf0138, 8),
        (0xf0168, 8),
        (0xf00b0, 4),
        (0xf0114, 4),
        (0xf0145, 1),
        (0xf10a4, 4),
        (0xf11d3, 1),
        (0xf1170, 8),
    ] {
        let mut f = Fixture::new();
        f.node_render_detail();
        let mut replacement: Vec<_> = (0..n).map(|i| f.data[&(a + i)]).collect();
        replacement[0] ^= 1;
        f.fault = Some((a, 1, replacement));
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            "torn_read"
        );
    }
    let mut f = Fixture::new();
    f.node_render_detail();
    f.word(0xf0138, 0xd5008);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_owner"
    );
}
#[test]
fn lazy_or_masked_resource_reports_limits_without_texture_reads() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.put(0xf11d3, &[1]);
    f.put(0xf11d0, &[1]);
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    let c = &v["detail"]["render_diagnostic"]["resource_cache"];
    assert_eq!(c["cache_equivalent"], false);
    assert!(c["diagnostic_dimensions"].is_null());
    assert_eq!(c["mask_sampling_required"], true);
}
#[test]
fn typed_own_info_detail_is_state_postcondition_only() {
    let mut f = Fixture::new();
    f.research_info();
    f.node_render_detail();
    f.array(0x92dc0, 0xc0000, 1);
    f.word(0xc0000, 0xc1000);
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    assert_eq!(
        v["own_info_postcondition"]["requested_detail_locally_visible"],
        true
    );
    assert_eq!(
        v["own_info_postcondition"]["fresh_transition_verified"],
        false
    );
}

#[test]
fn ancestor_metadata_never_supplies_ownership_or_payload_layout() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.word(0xf0108, 0xf2000);
    f.word(0xf2000, 0x10000 + 0x4346b10);
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    assert_eq!(
        v["detail"]["render_diagnostic"]["parent_chain"]["status"],
        "unknown_ancestor"
    );
    assert_eq!(
        v["detail"]["render_diagnostic"]["parent_chain"]["payload_hops"],
        0
    );
    f.word(0xf2000, 0);
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    assert_eq!(
        v["detail"]["render_diagnostic"]["parent_chain"]["status"],
        "unknown_primary_metadata"
    );
}
#[test]
fn cached_dimension_two_rounds_and_finite_policy_are_explicit() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.number(0xf110c, 1);
    f.number(0xf10a4, 601);
    f.put(0xf0114, &3.0f32.to_le_bytes());
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap()["detail"]["render_diagnostic"]
            ["resource_cache"]["diagnostic_dimensions"],
        serde_json::json!([600, 270])
    );
    f.put(0xf0114, &f32::NAN.to_le_bytes());
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_geometry"
    );
}

#[test]
fn unknown_resource_is_metadata_only_without_geometry_payload_reads() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.word(0xf1000, 0x10000 + 0x4346b10);
    for a in 0xf1090..0xf11f0 {
        f.data.remove(&a);
    }
    for a in 0xf0070..0xf0078 {
        f.data.remove(&a);
    }
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    assert_eq!(
        v["detail"]["render_diagnostic"]["reason"],
        "resource_layout_unavailable"
    );
    assert_eq!(
        v["detail"]["render_diagnostic"]["field_layout_verified"],
        false
    );
}

impl Fixture {
    fn hierarchy_fixture(&mut self) {
        let base = 0x10000;
        let table = 0x43484f0;
        self.word(base + table - 16, 0);
        self.word(base + table - 8, base + 0x4356200);
        self.region(base + 0x4356200, 16);
        self.word(base + 0x4356208, base + 0x350e200);
        self.region(base + 0x350e200, 128);
        self.put(base + 0x350e200, b"17C2dPositionObject");
        for node in [0xf3000, 0xf4000, 0x711f0, 0x71350] {
            self.region(node, 0x160);
            self.word(node, base + table);
        }
        self.word(0xd4e70, 0xf3000);
        self.word(0xd4e78, 0xf4000);
        self.word(0xf4108, 0xf3000);
        self.array(0xf30c0, 0xf5000, 1);
        self.word(0xf5000, 0xf4000);
        self.word(0xf0108, 0xf4000);
        self.array(0xf40c0, 0xf5100, 1);
        self.word(0xf5100, 0xf0000);
        self.word(0xf3108, 0x711f0);
        self.array(0x712b0, 0xf5200, 1);
        self.word(0xf5200, 0xf3000);
        // Legitimate null138 is not a registry; forbid even reading it on positions.
        for node in [0xf3000, 0xf4000, 0x711f0, 0x71350] {
            for a in node + 0x138..node + 0x140 {
                self.data.remove(&a);
            }
        }
    }
}
#[test]
fn v2_typed_reciprocal_hierarchy_is_gui_provenance_only() {
    let mut f = Fixture::new();
    f.node_render_detail();
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    let h = &v["detail"]["render_diagnostic"]["parent_chain"];
    assert_eq!(h["status"], "closed_to_owned_graphics_root");
    assert_eq!(h["payload_hops"], 3);
    assert_eq!(h["ancestor_own_country_semantics"], "unproved");
    assert_eq!(h["client_rectangle_available"], false);
    assert_eq!(h["input_authorized"], false);
}
#[test]
fn v2_hierarchy_refuses_position_root_registry_and_reciprocal_defects() {
    for (address, value, code) in [
        (0xd4e70, 0, "invalid_gui_position"),
        (0xd4e78, 0xf3000, "invalid_gui_position"),
        (0xf4108, 0x71350, "invalid_gui_position"),
        (0x700b8, 0x71008, "invalid_gui_owner"),
        (0x711f0, 0x10000 + 0x4356e08, "invalid_gui_type"),
        (0xf5100, 0xdead0000, "invalid_gui_hierarchy"),
    ] {
        let mut f = Fixture::new();
        f.node_render_detail();
        f.word(address, value);
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            code
        );
    }
}
#[test]
fn v2_unknown_ancestor_stops_before_all_payload() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.word(0xf0108, 0xf2000);
    f.word(0xf2000, 0x10000 + 0x4346b10);
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    let h = &v["detail"]["render_diagnostic"]["parent_chain"];
    assert_eq!(h["status"], "unknown_ancestor");
    assert_eq!(h["payload_hops"], 0);
    assert_eq!(h["graphics_root_closed"], false);
}
#[test]
fn v2_cycle_and_bad_signed_child_headers_refuse() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.word(0xf3108, 0xf4000);
    f.array(0xf40c0, 0xf5100, 2);
    f.word(0xf5108, 0xf3000);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_hierarchy"
    );
    let mut f = Fixture::new();
    f.node_render_detail();
    f.number(0xf40cc, -1);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_array"
    );
}
#[test]
fn v2_hierarchy_end_witnesses_cover_all_typed_links_and_anchors() {
    for (a, n) in [
        (0xd4e70, 8),
        (0xd4e78, 8),
        (0xf4108, 8),
        (0xf40c0, 16),
        (0xf5100, 8),
        (0xf3108, 8),
        (0x712b0, 16),
        (0xf5200, 8),
        (0x10000 + 0x4577890, 8),
        (0x700b8, 8),
        (0x711f0, 8),
    ] {
        let mut f = Fixture::new();
        f.node_render_detail();
        let mut replacement: Vec<_> = (0..n).map(|i| f.data[&(a + i)]).collect();
        replacement[0] ^= 1;
        f.fault = Some((a, 1, replacement));
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            "torn_read",
            "field {a:x}"
        );
    }
}
#[test]
fn v2_depth_limit_never_projects_or_reads_seventeenth_payload() {
    let mut f = Fixture::new();
    f.node_render_detail();
    let mut child = 0xf0000;
    for i in 0..16u64 {
        let node = 0x100000 + i * 0x200;
        f.region(node, 0x160);
        f.word(node, 0x10000 + 0x43484f0);
        f.word(child + 0x108, node);
        let data = 0x110000 + i * 0x20;
        f.array(node + 0xc0, data, 1);
        f.word(data, child);
        child = node;
    }
    f.word(child + 0x108, 0xdead0000);
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    let h = &v["detail"]["render_diagnostic"]["parent_chain"];
    assert_eq!(h["status"], "depth_limit");
    assert_eq!(h["observed_ancestors"], 16);
    assert_eq!(h["graphics_root_closed"], false);
}

#[test]
fn v2_known_renderer_ancestors_read_only_proved_base_links() {
    for (table, name) in [
        (0x4356e08, "7CSprite"),
        (0x434b370, "20CFrameAnimatedSprite"),
        (0x434c5b0, "14CInstantSprite"),
    ] {
        let mut f = Fixture::new();
        f.node_render_detail();
        let base = 0x10000;
        let node = 0xf8000;
        f.word(base + table - 16, 0);
        f.word(base + table - 8, base + 0x4356400);
        f.region(base + 0x4356400, 16);
        f.word(base + 0x4356408, base + 0x350e400);
        f.region(base + 0x350e400, 128);
        f.put(base + 0x350e400, name.as_bytes());
        f.region(node, 0x138);
        f.put(node + 0x144, &[0]);
        f.word(node, base + table);
        f.word(node + 0x108, 0xf4000);
        f.array(node + 0xc0, 0xf8200, 1);
        f.word(0xf8200, 0xf0000);
        f.word(0xf0108, node);
        f.word(0xf5100, node);
        let v = research_ui::node_detail(&f, "tools1").unwrap();
        let h = &v["detail"]["render_diagnostic"]["parent_chain"];
        assert_eq!(h["status"], "closed_to_owned_graphics_root");
        assert_eq!(h["payload_hops"], 4);
        assert_eq!(h["chain"][0]["type_encoding"], name);
    }
}
#[test]
fn v2_alternate_root_and_child_capacity_bound_are_explicit() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.word(0xf3108, 0x71350);
    f.array(0x71410, 0xf5200, 1);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap()["detail"]["render_diagnostic"]
            ["parent_chain"]["chain"][2]["role"],
        "owned_graphics_root_350"
    );
    let mut f = Fixture::new();
    f.node_render_detail();
    f.number(0xf40c8, 2049);
    f.data.remove(&0xf5100);
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    let h = &v["detail"]["render_diagnostic"]["parent_chain"];
    assert_eq!(h["status"], "unavailable_policy_bound");
    assert_eq!(h["container_position_joins_verified"], true);
    assert_eq!(h["payload_hops"], 0);
    assert_eq!(h["policy_boundary"]["capacity"], 2049);
    assert_eq!(h["policy_boundary"]["count"], 1);
    assert_eq!(h["policy_boundary"]["inspected_entries_before_attempt"], 1);
    f.assert_not_read(&[0xf5100]);
}

#[test]
fn hierarchy_container_policy_refusal_has_no_join_or_payload_claim() {
    for count in [0, 1] {
        let mut f = Fixture::new();
        f.node_render_detail();
        f.array(0xf30c0, 0x180000, count);
        f.number(0xf30c8, 2049);
        let v = research_ui::node_detail(&f, "tools1").unwrap();
        assert_eq!(
            v["validation_scope"],
            "requested_node_with_complete_current_record_ownership"
        );
        let h = &v["detail"]["render_diagnostic"]["parent_chain"];
        assert_eq!(h["status"], "unavailable_policy_bound");
        assert_eq!(h["container_position_joins_verified"], false);
        assert_eq!(h["observed_ancestors"], 0);
        assert_eq!(
            h["policy_boundary"]["stage"],
            "container_position_membership"
        );
        assert_eq!(h["policy_boundary"]["count"], count);
        assert_eq!(h["policy_boundary"]["inspected_entries_before_attempt"], 0);
        assert_eq!(h["graphics_root_closed"], false);
        assert_eq!(h["input_authorized"], false);
        f.assert_not_read(&[0x180000, 0xf0108]);
    }
}
#[test]
fn hierarchy_aggregate_policy_refusal_preserves_only_completed_ancestors() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.region(0x180000, 2048 * 8);
    f.word(0x180000, 0xf0000);
    f.array(0xf40c0, 0x180000, 2048);
    // This individually valid root header would exceed the aggregate budget;
    // its child payload is intentionally unmapped and must never be read.
    f.array(0x712b0, 0x190000, 6143);
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    let h = &v["detail"]["render_diagnostic"]["parent_chain"];
    assert_eq!(h["status"], "unavailable_policy_bound");
    assert_eq!(h["container_position_joins_verified"], true);
    assert_eq!(h["payload_hops"], 2);
    assert_eq!(h["policy_boundary"]["role"], "owned_graphics_root_1f0");
    assert_eq!(h["policy_boundary"]["capacity"], 6143);
    assert_eq!(h["policy_boundary"]["count"], 6143);
    assert_eq!(
        h["policy_boundary"]["inspected_entries_before_attempt"],
        2050
    );
    assert_eq!(h["graphics_root_closed"], false);
    assert_eq!(h["effective_routing_verified"], false);
    f.assert_not_read(&[0x190000]);
}
#[test]
fn oversized_hierarchy_headers_never_hide_alignment_or_structure_errors() {
    for (data, capacity, count, code) in [
        (0x180003, 2049, 1, "invalid_gui_hierarchy"),
        (0x180000, 20001, 1, "invalid_array"),
        (0x180000, 2049, -1, "invalid_array"),
        (0x180000, 2049, 2050, "invalid_array"),
    ] {
        let mut f = Fixture::new();
        f.node_render_detail();
        f.array(0xf30c0, data, count);
        f.number(0xf30c8, capacity);
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            code
        );
        f.assert_not_read(&[data]);
    }
}
#[test]
fn policy_unavailable_hierarchy_still_witnesses_the_refused_header_and_owner() {
    for address in [0xf30c0, 0xd4098] {
        let mut f = Fixture::new();
        f.node_render_detail();
        f.array(0xf30c0, 0x180000, 1);
        f.number(0xf30c8, 2049);
        let length = if address == 0xf30c0 { 16 } else { 8 };
        let mut replacement: Vec<_> = (0..length).map(|i| f.data[&(address + i)]).collect();
        replacement[0] ^= 1;
        f.fault = Some((address, 1, replacement));
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            "torn_read"
        );
        f.assert_not_read(&[0x180000]);
    }
}

impl Fixture {
    fn target_later_item(&mut self) {
        self.region(0xd8000, 0xd0);
        self.word(0xd8000, 0x10000 + 0x424e2e0);
        self.word(0xd8030, 0x92000);
        self.word(0xd80c8, 0x81000);
        self.word(0xd8010, 0xdead0000); // unrelated GUI intentionally unreadable
        self.word(0xd2038, 0xd8000);
    }
    fn target_info(&mut self, selected: u64) {
        self.array(0x92dc0, 0xc0000, 1);
        self.word(0xc0000, 0xc1000);
        self.region(0xc1000, 0xb00);
        self.word(0xc1000, 0x10000 + 0x430bbf0);
        self.word(0xc1a78, 0x32000);
        self.word(0xc1a80, 0x92000);
        self.word(0xc1a98, selected);
        self.word(0xc1a88, 0xc3000);
        self.region(0xc3000, 0xf00);
        self.word(0xc3000, 0x10000 + 0x4347d58);
        self.word(0xc3098, 0x70000);
        self.put(0xc30a5, &[8]);
    }
    fn assert_not_read(&self, addresses: &[u64]) {
        let reads = self.tracked_reads.borrow();
        for a in addresses {
            assert!(!reads.contains_key(a), "Forbidden read {a:x}");
        }
    }
}
#[test]
fn target_scope_skips_poisoned_unrelated_gui_and_slot_widgets() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.target_later_item();
    f.word(0x91308, 0xdead0000); // whole widget vector is outside target model
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    assert_eq!(
        v["validation_scope"],
        "requested_node_with_complete_current_record_ownership"
    );
    assert_eq!(v["full_gui_model_validated"], false);
    assert_eq!(v["detail"]["distinct_current_items"], 2);
    f.assert_not_read(&[0xd8010, 0xdead0000, 0x91308, 0x94000, 0xb1000]);
    assert!(research_ui::nodes(&f).is_err()); // full model is deliberately unchanged
}
#[test]
fn target_later_foreign_owner_refused_before_payload_and_geometry() {
    for (address, replacement, error, forbidden) in [
        (0xd8030, 0x92008, "invalid_gui_owner", 0xd80c8),
        (0x81158, 0x28008, "invalid_research_owner", 0x81160),
    ] {
        let mut f = Fixture::new();
        f.node_render_detail();
        f.target_later_item();
        f.word(address, replacement);
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            error
        );
        f.assert_not_read(&[forbidden, 0xd3010, 0xf0138, 0xd8010]);
    }
}
#[test]
fn target_later_canonical_mismatch_not_ignored() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.target_later_item();
    f.word(0x81160, 0x63000);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_research_join"
    );
    f.assert_not_read(&[0xd3010, 0xf0138]);
}
#[test]
fn target_global_duplicate_key_outside_current_records_refused() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.string(0x63208, "tools1");
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_research_id"
    );
    f.assert_not_read(&[0xd3010, 0xf0138]);
}
#[test]
fn target_catalog_sentinel_and_unrelated_definition_must_be_valid() {
    for (a, value, code) in [
        (0x62000, 0x63000, "research_unavailable"),
        (0x63200, 0x10000 + 0x42107f8, "invalid_gui_type"),
    ] {
        let mut f = Fixture::new();
        f.node_render_detail();
        f.word(a, value);
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            code
        );
    }
    let mut f = Fixture::new();
    f.node_render_detail();
    f.number(0x63230, 2);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_research_join"
    );
}

#[test]
fn target_reserved_singleton_is_typed_and_never_reads_template_payload() {
    let mut f = Fixture::new();
    f.node_render_detail();
    assert!(research_ui::node_detail(&f, "tools1").is_ok());
    f.assert_not_read(&[0x62508, 0x62530, 0x62538]);
    for (address, replacement, code) in [
        (0x62000, 0, "research_unavailable"),
        (0x10000 + 0x439f560, 0, "research_unavailable"),
        (0x10000 + 0x439f560, 0x63000, "research_unavailable"),
        (0x62500, 0x10000 + 0x4211038, "invalid_gui_type"),
    ] {
        let mut f = Fixture::new();
        f.node_render_detail();
        f.word(address, replacement);
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            code
        );
        f.assert_not_read(&[0x62508, 0x62530, 0x62538, 0xd3010, 0xf0138]);
    }
}

#[test]
fn target_reserved_global_pointer_and_primary_are_final_witnesses() {
    for address in [0x10000 + 0x439f560, 0x62500] {
        let mut f = Fixture::new();
        f.node_render_detail();
        let mut bytes: Vec<_> = (0..8).map(|i| f.data[&(address + i)]).collect();
        bytes[0] ^= 1;
        f.fault = Some((address, 1, bytes));
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            "torn_read"
        );
    }
}
#[test]
fn target_catalog_and_complete_record_mutations_are_witnessed() {
    for (a, n) in [
        (0x36048, 16),
        (0x62000, 32),
        (0x63208, 32),
        (0x63230, 4),
        (0x28088, 16),
        (0x82000, 24),
        (0x92dd8, 16),
        (0xd2000, 80),
        (0x92df0, 16),
    ] {
        let mut f = Fixture::new();
        f.node_render_detail();
        let mut bytes: Vec<_> = (0..n).map(|i| f.data[&(a + i)]).collect();
        bytes[0] ^= 1;
        f.fault = Some((a, 1, bytes));
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            "torn_read",
            "{a:x}"
        );
    }
}
#[test]
fn target_later_unrelated_owner_mutation_is_witnessed() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.target_later_item();
    f.fault = Some((0xd8030, 1, 0x92008u64.to_le_bytes().to_vec()));
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "torn_read"
    );
}
#[test]
fn target_in_second_vector_and_pointer_dedup_remain_complete() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.array(0x92dd8, 0, 0);
    f.array(0x92df0, 0xd2000, 2);
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    assert_eq!(v["detail"]["record_count"], 2);
    assert_eq!(v["detail"]["distinct_bound_items"], 1);
}
#[test]
fn target_info_skips_hidden_other_selection_control_and_container() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.target_info(0x81000);
    f.word(0xc1a88, 0xdead0000);
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    assert_eq!(
        v["own_info_postcondition"]["matching_locally_visible_count"],
        0
    );
    f.assert_not_read(&[0xc1a88, 0xdead0000, 0xc1ac0, 0xc1ab0]);
}
#[test]
fn target_info_null_and_visible_target_have_no_button_authority() {
    for (selected, expected) in [(0, 0), (0x80000, 1)] {
        let mut f = Fixture::new();
        f.node_render_detail();
        f.target_info(selected);
        let v = research_ui::node_detail(&f, "tools1").unwrap();
        assert_eq!(
            v["own_info_postcondition"]["matching_locally_visible_count"],
            expected
        );
        assert_eq!(
            v["own_info_postcondition"]["fresh_transition_verified"],
            false
        );
        f.assert_not_read(&[0xc1ac0, 0xc1ab0]);
        if selected == 0 {
            f.assert_not_read(&[0xc39a0, 0xc39b8]);
        } else {
            assert_eq!(
                v["own_info_postcondition"]["ordinary_research_button"]["reason"],
                "named_control_unavailable"
            );
            assert_eq!(
                v["own_info_postcondition"]["ordinary_research_button"]["input_authorized"],
                false
            );
        }
    }
}
#[test]
fn target_info_foreign_scope_refused_before_selected_payload() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.target_info(0x80000);
    f.word(0xc1a78, 0x32008);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_owner"
    );
    f.assert_not_read(&[0xc1a98, 0xc1a88]);
    let mut f = Fixture::new();
    f.node_render_detail();
    f.target_info(0x81000);
    f.word(0x81158, 0x28008);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_research_owner"
    );
    f.assert_not_read(&[0x81160, 0xc1a88]);
}
#[test]
fn target_info_ambiguity_and_mutation_refused() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.target_info(0x80000);
    let copy: Vec<_> = (0..0xb00).map(|i| f.data[&(0xc1000 + i)]).collect();
    f.put(0xc2000, &copy);
    f.array(0x92dc0, 0xc0000, 2);
    f.word(0xc0008, 0xc2000);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "ambiguous_gui_detail"
    );
    for (a, n) in [(0x92dc0, 16), (0xc1a98, 8), (0xc30a5, 1)] {
        let mut f = Fixture::new();
        f.node_render_detail();
        f.target_info(0x80000);
        let mut bytes: Vec<_> = (0..n).map(|i| f.data[&(a + i)]).collect();
        bytes[0] ^= 1;
        f.fault = Some((a, 1, bytes));
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            "torn_read"
        );
    }
}
#[test]
fn target_final_process_identity_check_is_retained() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.replaced = true;
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "ownership_changed"
    );
}
#[test]
fn target_differential_serialized_contract() {
    let mut values = Vec::new();
    for bound in [false, true] {
        let mut f = Fixture::new();
        f.node_render_detail();
        if !bound {
            f.word(0xd3010, 0);
        }
        let value = research_ui::node_detail(&f, "tools1").unwrap();
        println!(
            "DIFFERENTIAL_READS bound={bound} logical={}",
            f.tracked_reads.borrow().values().sum::<usize>()
        );
        let mut detail = value["detail"].clone();
        detail.as_object_mut().unwrap().remove("record_count");
        detail
            .as_object_mut()
            .unwrap()
            .remove("distinct_current_items");
        values.push(serde_json::json!({"detail":detail,"selected_slot":value["selected_slot"],"own_info_postcondition":value["own_info_postcondition"],"input_authorized":value["input_authorized"],"client_rectangle_available":value["client_rectangle_available"]}));
    }
    println!("DIFFERENTIAL={}", serde_json::Value::Array(values));
}
#[test]
fn target_context_gates_precede_catalog_and_current_payload() {
    for (a, v, code) in [
        (0x903d8, 5, "gui_unavailable"),
        (0x92e20, 2, "foreign_ui_context"),
        (0x92e20, -1, "gui_unavailable"),
        (0x30518, 0, "player_unavailable"),
    ] {
        let mut f = Fixture::new();
        f.node_render_detail();
        f.number(a, v);
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            code,
            "{a:x}"
        );
        f.assert_not_read(&[0x36048, 0x92dd8, 0xd3010]);
    }
    let mut f = Fixture::new();
    f.node_render_detail();
    f.word(0x92e08, 0xdead0000);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_research_owner"
    );
    f.assert_not_read(&[0xdead0000, 0x92dd8, 0xd3010]);
}
#[test]
fn target_later_exact_types_checked_before_ownership_payload() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.target_later_item();
    f.word(0xd8000, 0x10000 + 0x4347d58);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_type"
    );
    f.assert_not_read(&[0xd8030, 0xd80c8, 0xd3010]);
    let mut f = Fixture::new();
    f.node_render_detail();
    f.target_later_item();
    f.word(0x81000, 0x10000 + 0x4211038);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_type"
    );
    f.assert_not_read(&[0x81158, 0x81160, 0xd3010]);
}
#[test]
fn target_missing_current_membership_and_unbound_duplicate_policy() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.array(0x92dd8, 0, 0);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "node_unavailable"
    );
    let mut f = Fixture::new();
    f.node_render_detail();
    f.target_later_item();
    f.word(0xd80c8, 0x80000);
    f.word(0xd8010, 0xd4000); // same valid binding on distinct item
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "ambiguous_gui_node"
    );
    f.assert_not_read(&[0xf0138]);
    let mut f = Fixture::new();
    f.node_render_detail();
    f.target_later_item();
    f.word(0xd80c8, 0x80000);
    f.word(0xd8010, 0); // conflicting target GUI type, preserves old refusal
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "ambiguous_gui_node"
    );
    f.assert_not_read(&[0xf0138]);
}

#[test]
fn owned_root_accepts_actual_size_and_boundary_without_sibling_payloads() {
    for (cap, count) in [(5395i32, 4142), (8192, 8189)] {
        let mut f = Fixture::new();
        f.node_render_detail();
        f.region(0x180000, count * 8);
        f.word(0x180000, 0xf3000);
        for i in 1..count {
            f.word(0x180000 + i as u64 * 8, 0xdead0000);
        }
        f.array(0x712b0, 0x180000, count as i32);
        f.put(0x712b8, &cap.to_le_bytes());
        let v = research_ui::node_detail(&f, "tools1").unwrap();
        let h = &v["detail"]["render_diagnostic"]["parent_chain"];
        assert_eq!(h["graphics_root_closed"], true);
        assert_eq!(h["input_authorized"], false);
        f.assert_not_read(&[0xdead0000]);
    }
}
#[test]
fn root_policy_refuses_unmapped_payload_and_witnesses_header() {
    for (cap, count) in [(8193i32, 1), (8192, 8190)] {
        let mut f = Fixture::new();
        f.node_render_detail();
        f.array(0x712b0, 0x180000, count);
        f.put(0x712b8, &cap.to_le_bytes());
        let v = research_ui::node_detail(&f, "tools1").unwrap();
        let h = &v["detail"]["render_diagnostic"]["parent_chain"];
        assert_eq!(
            h["policy_boundary"]["policy_scope"],
            "exact_owned_graphics_root"
        );
        assert_eq!(h["graphics_root_closed"], false);
        f.assert_not_read(&[0x180000]);
        f.fault = Some((0x712b0, 1, vec![0; 16]));
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            "torn_read"
        );
    }
}
#[test]
fn root_array_all_bytes_are_final_witnesses() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.region(0x180000, 4142 * 8);
    f.word(0x180000, 0xf3000);
    f.array(0x712b0, 0x180000, 4142);
    let mut bytes = vec![0; 4142 * 8];
    bytes[..8].copy_from_slice(&0xf3000u64.to_le_bytes());
    bytes[4142 * 8 - 1] = 1;
    f.fault = Some((0x180000, 1, bytes));
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "torn_read"
    );
}
#[test]
fn same_primary_foreign_position_never_gets_root_policy() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.region(0x170000, 0x160);
    f.word(0x170000, 0x10000 + 0x43484f0);
    f.word(0xf3108, 0x170000);
    f.array(0x1700c0, 0x180000, 4142);
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    let h = &v["detail"]["render_diagnostic"]["parent_chain"];
    assert_eq!(h["policy_boundary"]["policy_scope"], "ordinary_ancestor");
    assert_eq!(h["policy_boundary"]["capacity_limit"], 2048);
    f.assert_not_read(&[0x180000]);
}

#[test]
fn larger_root_policy_preserves_unique_membership_and_alignment() {
    for duplicate in [false, true] {
        let mut f = Fixture::new();
        f.node_render_detail();
        f.region(0x180000, 4142 * 8);
        if duplicate {
            f.word(0x180000, 0xf3000);
            f.word(0x180008, 0xf3000);
        }
        f.array(0x712b0, 0x180000, 4142);
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            "invalid_gui_hierarchy"
        );
    }
    let mut f = Fixture::new();
    f.node_render_detail();
    f.array(0x712b0, 0x180003, 8193);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_hierarchy"
    );
    f.assert_not_read(&[0x180003]);
}

impl Fixture {
    fn unit_chain(&mut self) {
        for (i, node) in [0x711f0u64, 0xf3000, 0xf4000, 0xf0000]
            .into_iter()
            .enumerate()
        {
            self.put(node + 0x70, &1.0f32.to_le_bytes());
            self.put(node + 0x74, &(-2.0f32).to_le_bytes());
            self.put(node + 0x114, &1.0f32.to_le_bytes());
            self.put(node + 0x144, &[1]);
            self.put(node + 0x128, &[1]);
            self.put(node + 0x80, &1.0f32.to_le_bytes());
            self.put(node + 0x94, &1.0f32.to_le_bytes());
            self.put(node + 0xb0, &((i + 1) as f32).to_le_bytes());
            self.put(node + 0xb4, &(-2.0 * (i + 1) as f32).to_le_bytes());
        }
    }
}
#[test]
fn unit_cache_consistency_is_conditional_not_currency() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.unit_chain();
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    let d = &v["detail"]["render_diagnostic"]["parent_chain"]["unit_cache_diagnostic"];
    assert_eq!(d["status"], "conditional_cache_consistent");
    assert_eq!(d["current_frame_verified"], false);
    assert_eq!(d["input_authorized"], false);
    f.put(0xf00b0, &99.0f32.to_le_bytes());
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    assert_eq!(
        v["detail"]["render_diagnostic"]["parent_chain"]["unit_cache_diagnostic"]["status"],
        "cache_mismatch"
    );
}
#[test]
fn unit_cache_nonunit_nonfinite_and_skipped_root_are_unavailable() {
    for (a, bytes, status) in [
        (
            0xf3114,
            2.0f32.to_le_bytes(),
            "unavailable_nonunit_or_nonfinite",
        ),
        (
            0xf3070,
            f32::NAN.to_le_bytes(),
            "unavailable_nonunit_or_nonfinite",
        ),
    ] {
        let mut f = Fixture::new();
        f.node_render_detail();
        f.unit_chain();
        f.put(a, &bytes);
        let v = research_ui::node_detail(&f, "tools1").unwrap();
        assert_eq!(
            v["detail"]["render_diagnostic"]["parent_chain"]["unit_cache_diagnostic"]["status"],
            status
        );
    }
    let mut f = Fixture::new();
    f.node_render_detail();
    f.unit_chain();
    f.put(0x71334, &[0]);
    f.put(0x71318, &[0]);
    for a in 0x712a0..0x712a8 {
        f.data.remove(&a);
    }
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    assert_eq!(
        v["detail"]["render_diagnostic"]["parent_chain"]["unit_cache_diagnostic"]["status"],
        "unavailable_observed_traversal_flags"
    );
    f.assert_not_read(&[0x712a0]);
}
#[test]
fn unit_cache_fields_are_final_witnesses() {
    for (a, n) in [
        (0xf3070, 4),
        (0xf3114, 4),
        (0xf3128, 1),
        (0xf3134, 1),
        (0xf3144, 1),
        (0xf3080, 4),
        (0xf3094, 4),
        (0xf30b0, 4),
    ] {
        let mut f = Fixture::new();
        f.node_render_detail();
        f.unit_chain();
        let mut bytes = (0..n).map(|i| f.data[&(a + i)]).collect::<Vec<_>>();
        bytes[0] ^= 1;
        f.fault = Some((a, 1, bytes));
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            "torn_read"
        );
    }
}

#[test]
fn hidden_constructor_positions_with_children_still_publish_unit_cache() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.unit_chain();
    for node in [0x711f0u64, 0xf3000, 0xf4000] {
        f.put(node + 0x144, &[0]);
    }
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    assert_eq!(
        v["detail"]["render_diagnostic"]["parent_chain"]["unit_cache_diagnostic"]["status"],
        "conditional_cache_consistent"
    );
    // A negative count on the leaf is malformed, not a policy/unavailable branch.
    f.put(0xf00cc, &(-1i32).to_le_bytes());
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_hierarchy"
    );
}

impl Fixture {
    fn ordinary_info_research(&mut self) {
        self.node_render_detail();
        self.target_info(0x80000);
        self.array(0xc39a0, 0xc7500, 1);
        self.word(0xc7500, 0xc5000);
        self.array(0xc39b8, 0xc7000, 1);
        self.string(0xc7000, "research");
        self.word(0xc1ac0, 0xc5000);
        self.word(0xc1ab0, 0xc5000);
        self.region(0xc5000, 0x110);
        self.word(0xc5000, 0x10000 + 0x43470c8);
        self.word(0xc5068, 0x70000);
        self.word(0xc5080, 0x10000 + 0x4347488);
        self.word(0xc5088, 0xc6000);
        self.word(0xc5090, 0xc6000);
        self.number(0xc5098, 1);
        self.region(0xc6000, 32);
        self.word(0xc6000, 0xc12c8);
        self.word(0xc12c8, 0x10000 + 0x430bd18);
        self.word(0xc12d0, 0xc9000); // FIRST VOID capture +8
        self.word(0xc12e0, 0x10000 + 0x2916700); // manager +18
        self.word(0xc12e8, 0x10000 + 0x29166d0); // invoker +20
        self.region(0xc9000, 24);
        self.word(0xc9000, 0x10000 + 0x290cce0);
        self.word(0xc9010, 0xc1000);
        self.word(0xc50e8, 0xc8000);
        self.word(0xc8000, 0x10000 + 0x4356e08);
        self.word(0xc8138, 0xc5000);
        // NO button+110, CSprite resource/local/geometry fields or Ctrl capture.
        self.word(0xc12f0, 0xdead0000); // Ctrl capture +28: never dereferenced
    }
}
fn ordinary_info_value(f: &Fixture) -> serde_json::Value {
    research_ui::node_detail(f, "tools1").unwrap()["own_info_postcondition"]
        ["ordinary_research_button"]
        .clone()
}
#[test]
fn ordinary_info_void_standard_renderer_metadata_without_unproved_fields() {
    let mut f = Fixture::new();
    f.ordinary_info_research();
    let v = ordinary_info_value(&f);
    assert_eq!(v["available"], true);
    assert_eq!(v["callback_binding_verified"], true);
    assert_eq!(v["renderer_type_encoding"], "7CSprite");
    assert_eq!(v["renderer_control_join_verified"], true);
    assert_eq!(v["input_authorized"], false);
    assert_eq!(v["eligibility"], "unavailable");
    assert_eq!(v["geometry_available"], false);
    f.assert_not_read(&[
        0xc5110, 0xc8168, 0xc8070, 0xc80b0, 0xc12f0, 0xdead0000, 0x92e18,
    ]);
    assert!(!v.to_string().contains(&0xc5000u64.to_string()));
}
#[test]
fn ordinary_info_hidden_or_other_selected_has_no_control_payload() {
    for other in [false, true] {
        let mut f = Fixture::new();
        f.ordinary_info_research();
        if other {
            f.word(0xc1a98, 0x81000);
        } else {
            f.put(0xc30a5, &[0]);
        }
        assert!(
            research_ui::node_detail(&f, "tools1").unwrap()["own_info_postcondition"]
                ["ordinary_research_button"]
                .is_null()
        );
        f.assert_not_read(&[0xc39a0, 0xc39b8, 0xc1ac0, 0xc5068]);
    }
}
#[test]
fn ordinary_info_wrong_alias_and_name_stop_before_control_payload() {
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.word(0xc1ab0, 0xdead0000);
    assert_eq!(
        ordinary_info_value(&f)["reason"],
        "ordinary_branch_unavailable"
    );
    f.assert_not_read(&[0xdead0000, 0xc5088, 0xc12d0, 0xc50e8]);
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.string(0xc7000, "research_MIO");
    assert_eq!(
        ordinary_info_value(&f)["reason"],
        "named_control_unavailable"
    );
    f.assert_not_read(&[0xc1ac0, 0xc5068, 0xc12d0]);
}
#[test]
fn ordinary_info_duplicate_name_and_wrong_ac0_are_structural_refusals() {
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.array(0xc39a0, 0xc7500, 2);
    f.word(0xc7508, 0xc5000);
    f.array(0xc39b8, 0xc7000, 2);
    f.string(0xc7020, "research");
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_registry"
    );
    f.assert_not_read(&[0xc5068, 0xc12d0]);
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.word(0xc1ac0, 0xdead0000);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_registry"
    );
    f.assert_not_read(&[0xc5068, 0xdead0000]);
}
#[test]
fn ordinary_info_unknown_control_does_not_borrow_drag_or_standard_fields() {
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.word(0xc5000, 0x10000 + 0x4346b10);
    let v = ordinary_info_value(&f);
    assert_eq!(v["reason"], "control_layout_unavailable");
    f.assert_not_read(&[0xc5068, 0xc50ac, 0xc50e8, 0xc5110, 0xc5088]);
}
#[test]
fn ordinary_info_registry_and_render_owner_before_payload() {
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.word(0xc5068, 0x70008);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_owner"
    );
    f.assert_not_read(&[0xc5088, 0xc12d0, 0xc50e8]);
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.word(0xc8138, 0xc5008);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_owner"
    );
    f.assert_not_read(&[0xc8168, 0xc8070]);
}
#[test]
fn ordinary_info_override_and_removed_listener_never_inspect_capture() {
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.word(0xc50a0, 0xdead0000);
    assert_eq!(
        ordinary_info_value(&f)["reason"],
        "single_observer_override"
    );
    f.assert_not_read(&[0xdead0000, 0xc5088, 0xc12d0, 0xc50e8]);
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.put(0xc6018, &[1]);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_binding"
    );
    f.assert_not_read(&[0xc12c8, 0xc12d0, 0xc50e8]);
}
#[test]
fn ordinary_info_listener_structure_and_unknown_target_stop_before_callback() {
    for (a, value) in [(0xc6008, 0xc6000), (0xc6010, 0xc6000), (0xc5090, 0xc6020)] {
        let mut f = Fixture::new();
        f.ordinary_info_research();
        f.word(a, value);
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            "invalid_gui_binding"
        );
        f.assert_not_read(&[0xc12c8, 0xc12d0, 0xc50e8]);
    }
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.word(0xc6000, 0xdead0000);
    assert_eq!(ordinary_info_value(&f)["reason"], "callback_unavailable");
    f.assert_not_read(&[0xdead0000, 0xc12c8, 0xc12d0]);
}
#[test]
fn ordinary_info_wrong_callback_primary_and_ctrl_variant_are_unavailable() {
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.word(0xc12c8, 0x10000 + 0x424e0a8);
    assert_eq!(
        ordinary_info_value(&f)["reason"],
        "callback_layout_unavailable"
    );
    f.assert_not_read(&[0xc12e0, 0xc12d0, 0xc9000, 0xc50e8]);
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.word(0xc12e0, 0x10000 + 0x2916840); // Ctrl manager, wrong first field
    f.word(0xc1300, 0x10000 + 0x2916840);
    f.word(0xc1308, 0x10000 + 0x2916810);
    assert_eq!(
        ordinary_info_value(&f)["reason"],
        "callback_variant_unavailable"
    );
    f.assert_not_read(&[0xc12d0, 0xc12f0, 0xc9000, 0xc50e8]);
}
#[test]
fn ordinary_info_foreign_capture_owner_refused_before_member() {
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.word(0xc9010, 0xc2000);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_owner"
    );
    f.assert_not_read(&[0xc9000, 0xc9008, 0xc2000, 0xc50e8]);
}
#[test]
fn ordinary_info_wrong_member_extra_callback_and_unknown_renderer_are_unavailable() {
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.word(0xc9000, 0x10000 + 0x290cd20);
    assert_eq!(
        ordinary_info_value(&f)["reason"],
        "callback_member_unavailable"
    );
    f.assert_not_read(&[0xc50e8]);
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.word(0xc14a0, 0x10000 + 0x2916700);
    assert_eq!(
        ordinary_info_value(&f)["reason"],
        "callback_variant_unavailable"
    );
    f.assert_not_read(&[0xc12d0, 0xc9010]);
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.word(0xc8000, 0x10000 + 0x4346b10);
    let v = ordinary_info_value(&f);
    assert_eq!(v["callback_binding_verified"], true);
    assert_eq!(v["reason"], "renderer_layout_unavailable");
    f.assert_not_read(&[0xc8138, 0xc8168, 0xc5110]);
}
#[test]
fn ordinary_info_io_failures_and_final_mutations_refuse_whole_result() {
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.data.remove(&0xc9010);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "unexpected_read"
    );
    for (a, n) in [
        (0xc39a0, 16),
        (0xc39b8, 16),
        (0xc7500, 8),
        (0xc7000, 32),
        (0xc1ac0, 8),
        (0xc1ab0, 8),
        (0xc5068, 8),
        (0xc5080, 8),
        (0xc5098, 4),
        (0xc6008, 8),
        (0xc6018, 1),
        (0xc12e0, 8),
        (0xc12e8, 8),
        (0xc12d0, 8),
        (0xc9010, 8),
        (0xc9000, 8),
        (0xc50e8, 8),
        (0xc8138, 8),
    ] {
        let mut f = Fixture::new();
        f.ordinary_info_research();
        let mut bytes: Vec<_> = (0..n).map(|i| f.data[&(a + i)]).collect();
        bytes[0] ^= 1;
        f.fault = Some((a, 1, bytes));
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            "torn_read",
            "{a:x}"
        );
    }
}
#[test]
fn ordinary_info_ambiguous_matching_windows_before_any_named_controls() {
    let mut f = Fixture::new();
    f.ordinary_info_research();
    let copy: Vec<_> = (0..0xb00).map(|i| f.data[&(0xc1000 + i)]).collect();
    f.put(0xca000, &copy);
    f.array(0x92dc0, 0xc0000, 2);
    f.word(0xc0008, 0xca000);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "ambiguous_gui_detail"
    );
    f.assert_not_read(&[0xc39a0, 0xc39b8, 0xc5068, 0xc12d0]);
}
#[test]
fn ordinary_info_multiple_valid_listeners_do_not_choose_one_callback() {
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.region(0xc6020, 32);
    f.word(0xc6010, 0xc6020);
    f.word(0xc6028, 0xc6000);
    f.word(0xc6020, 0xc12c8);
    f.word(0xc5090, 0xc6020);
    f.number(0xc5098, 2);
    assert_eq!(ordinary_info_value(&f)["reason"], "callback_unavailable");
    f.assert_not_read(&[0xc12c8, 0xc12d0, 0xc50e8]);
}
#[test]
fn ordinary_info_disabled_state_does_not_claim_eligibility_or_input() {
    let mut f = Fixture::new();
    f.ordinary_info_research();
    f.number(0xc50ac, 3);
    let v = ordinary_info_value(&f);
    assert_eq!(v["disabled_state3"], true);
    assert_eq!(v["callback_binding_verified"], true);
    assert_eq!(v["input_authorized"], false);
    assert_eq!(v["eligibility"], "unavailable");
    assert_eq!(v["effective_routing_verified"], false);
}

impl Fixture {
    fn ordinary_info_cache_v2(&mut self, instant: bool) {
        self.ordinary_info_research();
        self.word(
            0xc8000,
            0x10000 + if instant { 0x434c5b0 } else { 0x434b370 },
        );
        if instant {
            let table = 0x10000 + 0x434c5b0;
            self.word(table - 16, 0);
            self.word(table - 8, 0x10000 + 0x4356400);
            self.region(0x10000 + 0x4356400, 16);
            self.word(0x10000 + 0x4356408, 0x10000 + 0x350e400);
            self.region(0x10000 + 0x350e400, 128);
            self.put(0x10000 + 0x350e400, b"14CInstantSprite");
        }
        self.word(0xc8000 + if instant { 0x160 } else { 0x1b0 }, 0xcb000);
        self.word(0xc8000 + if instant { 8 } else { 0x168 }, 0xcb000);
        self.region(0xcb000, 0x1d4);
        self.word(0xcb000, 0x10000 + 0x434b668);
        self.number(0xcb090, 3);
        self.number(0xcb094, 1);
        self.number(0xcb0a4, 369);
        self.number(0xcb0a8, 34);
        self.word(0xcb1a0, u64::MAX);
        if !instant {
            self.ordinary_info_geometry_fixture();
        }
    }
}
#[test]
fn ordinary_info_v2_exact_frame_and_instant_local_cache_only() {
    for instant in [false, true] {
        let mut f = Fixture::new();
        f.ordinary_info_cache_v2(instant);
        let v = ordinary_info_value(&f);
        assert_eq!(v["resource_alias_matches"], true);
        assert_eq!(v["resource_layout_verified"], true);
        assert_eq!(v["resource_cache"]["handle_raw"], -1);
        assert_eq!(v["resource_cache"]["texture_width_raw"], 369);
        assert_eq!(v["geometry_available"], false);
        assert_eq!(v["input_authorized"], false);
        if instant {
            f.assert_not_read(&[0xc8070]);
        }
        if instant {
            f.assert_not_read(&[0xc8168, 0xc816c, 0xc81b0]);
        } else {
            f.assert_not_read(&[0xc8160]);
        }
    }
}
#[test]
fn ordinary_info_v2_alias_mismatch_stops_before_resource_metadata() {
    for instant in [false, true] {
        let mut f = Fixture::new();
        f.ordinary_info_cache_v2(instant);
        f.word(0xc8000 + if instant { 8 } else { 0x168 }, 0xdead0000);
        assert_eq!(
            ordinary_info_value(&f)["reason"],
            "resource_alias_unavailable"
        );
        f.assert_not_read(&[0xcb000, 0xcb090, 0xdead0000]);
    }
}
#[test]
fn ordinary_info_v2_foreign_control_precedes_resources() {
    let mut f = Fixture::new();
    f.ordinary_info_cache_v2(false);
    f.word(0xc8138, 0xdead0000);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_owner"
    );
    f.assert_not_read(&[0xc81b0, 0xc8168, 0xcb000]);
}
#[test]
fn ordinary_info_v2_other_resource_class_metadata_only() {
    let mut f = Fixture::new();
    f.ordinary_info_cache_v2(true);
    f.word(0xcb000, 0x10000 + 0x4357160);
    assert_eq!(
        ordinary_info_value(&f)["reason"],
        "resource_layout_unavailable"
    );
    f.assert_not_read(&[0xcb090, 0xcb094, 0xcb1a0, 0xcb0a4]);
}
#[test]
fn ordinary_info_v2_full_signed_handle_cache_gate() {
    for (handle, lazy, equivalent) in [
        (u64::MAX, 1, false),
        (0x00000000ffffffff, 1, true),
        (u64::MAX, 0, true),
    ] {
        let mut f = Fixture::new();
        f.ordinary_info_cache_v2(false);
        f.word(0xcb1a0, handle);
        f.put(0xcb1d3, &[lazy]);
        let v = ordinary_info_value(&f);
        assert_eq!(v["resource_cache"]["getter_cache_equivalent"], equivalent);
        if !equivalent {
            assert!(v["resource_cache"]["texture_width_raw"].is_null());
            f.assert_not_read(&[0xcb0a4, 0xcb0a8]);
        }
        if handle == 0xffffffff {
            assert_eq!(v["resource_cache"]["handle_raw"], 4294967295i64);
        }
    }
}
#[test]
fn ordinary_info_v2_invalid_grid_and_texture_bounded() {
    for (a, value, reason) in [
        (0xcb090, 0, "resource_grid_out_of_bounds"),
        (0xcb094, 4097, "resource_grid_out_of_bounds"),
        (0xcb0a4, 65537, "resource_texture_out_of_bounds"),
    ] {
        let mut f = Fixture::new();
        f.ordinary_info_cache_v2(false);
        f.number(a, value);
        assert_eq!(ordinary_info_value(&f)["reason"], reason);
        if a == 0xcb090 || a == 0xcb094 {
            f.assert_not_read(&[0xcb0a4, 0xcb0a8]);
        }
    }
}
#[test]
fn ordinary_info_v2_resource_io_and_final_mutation_refuse() {
    let mut f = Fixture::new();
    f.ordinary_info_cache_v2(false);
    f.data.remove(&0xcb1a7);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "unexpected_read"
    );
    for (a, n) in [
        (0xc81b0, 8),
        (0xc8168, 8),
        (0xcb000, 8),
        (0xcb090, 4),
        (0xcb094, 4),
        (0xcb10c, 4),
        (0xcb1a0, 8),
        (0xcb1d3, 1),
        (0xcb0a4, 4),
        (0xcb0a8, 4),
    ] {
        let mut f = Fixture::new();
        f.ordinary_info_cache_v2(false);
        let mut bytes: Vec<_> = (0..n).map(|i| f.data[&(a + i)]).collect();
        bytes[n as usize - 1] ^= 1;
        f.fault = Some((a, 1, bytes));
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            "torn_read",
            "{a:x}"
        );
    }
}

#[test]
fn raw_sprite_cache_handle_is_full_signed_width() {
    for (handle, lazy, equivalent) in [
        (-1i64, 1, false),
        (4294967295, 1, true),
        (-4294967296, 1, true),
        (-1, 0, true),
    ] {
        let mut f = Fixture::new();
        f.node_render_detail();
        f.put(0xf11a0, &handle.to_le_bytes());
        f.put(0xf11d3, &[lazy]);
        let v = research_ui::node_detail(&f, "tools1").unwrap();
        let cache = &v["detail"]["render_diagnostic"]["resource_cache"];
        assert_eq!(cache["handle_raw"], handle);
        assert_eq!(cache["cache_equivalent"], equivalent);
        assert_eq!(cache["diagnostic_dimensions"].is_null(), !equivalent);
        assert_eq!(v["input_authorized"], false);
    }
}

#[test]
fn raw_sprite_cache_handle_high_bytes_are_final_witnesses() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.put(0xf11a0, &4294967303i64.to_le_bytes());
    f.fault = Some((0xf11a0, 1, 8589934599i64.to_le_bytes().to_vec()));
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "torn_read"
    );
}

#[test]
fn raw_sprite_orientation_requires_full_word_for_dimensions() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.put(0xf110d, &[17, 22, 255]);
    let v = research_ui::node_detail(&f, "tools1").unwrap();
    let cache = &v["detail"]["render_diagnostic"]["resource_cache"];
    assert_eq!(cache["orientation_raw"], 0xff161100u32);
    assert!(cache["diagnostic_dimensions"].is_null());
    assert_eq!(v["input_authorized"], false);
}

#[test]
fn raw_sprite_orientation_high_bytes_are_final_witnesses() {
    let mut f = Fixture::new();
    f.node_render_detail();
    f.fault = Some((0xf110c, 1, 0x01000000u32.to_le_bytes().to_vec()));
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "torn_read"
    );
}

#[test]
fn ordinary_info_frame_orientation_preserves_full_word_metadata() {
    for instant in [false, true] {
        let mut f = Fixture::new();
        f.ordinary_info_cache_v2(instant);
        f.put(0xcb10c, &0xff161101u32.to_le_bytes());
        let v = ordinary_info_value(&f);
        assert_eq!(v["resource_cache"]["orientation_raw"], 0xff161101u32);
        assert_eq!(v["input_authorized"], false);
        assert_eq!(v["geometry_available"], false);
    }
}

impl Fixture {
    fn ordinary_info_geometry_fixture(&mut self) {
        for a in 0xc8000..0xc81d0 {
            self.data.entry(a).or_insert(0);
        }
        for a in [0xc8120, 0xc8124, 0xc812c, 0xc8130] {
            self.number(a, 0);
        }
        self.word(0xc3e70, 0xf3000);
        self.word(0xc3e78, 0xf4000);
        self.word(0xc8108, 0xf4000);
        self.array(0xf40c0, 0xf5100, 2);
        self.word(0xf5108, 0xc8000);
        self.put(0xc8114, &1.0f32.to_le_bytes());
        self.put(0xc80b0, &100.0f32.to_le_bytes());
        self.put(0xc80b4, &(-80.0f32).to_le_bytes());
        self.word(0xcb170, 0xdead0000);
    }
}
#[test]
fn ordinary_info_geometry_raw_exact_frame_no_input_or_origin_claim() {
    let mut f = Fixture::new();
    f.ordinary_info_cache_v2(false);
    let v = ordinary_info_value(&f);
    let g = &v["frame_raw_geometry"];
    assert_eq!(g["scalar_raw"], 1.0);
    assert_eq!(g["no_mask_rejection_branch"], true);
    assert_eq!(g["hit_origin_calculated"], false);
    assert_eq!(g["input_authorized"], false);
    assert_eq!(g["client_rectangle_available"], false);
    f.assert_not_read(&[0xdead0000, 0xc8034, 0xcb098]);
}
#[test]
fn ordinary_info_geometry_instant_and_csprite_do_not_borrow_frame_inputs() {
    let mut f = Fixture::new();
    f.ordinary_info_cache_v2(true);
    assert!(ordinary_info_value(&f)["frame_raw_geometry"].is_null());
    f.assert_not_read(&[
        0xc8114, 0xc80b0, 0xc8120, 0xcb111, 0xcb170, 0xcb1d0, 0xc3e70,
    ]);
    let mut f = Fixture::new();
    f.ordinary_info_research();
    assert!(ordinary_info_value(&f)["frame_raw_geometry"].is_null());
    f.assert_not_read(&[0xc8114, 0xc80b0, 0xc8120, 0xc3e70]);
}
#[test]
fn ordinary_info_geometry_bad_alias_and_resource_precede_raw_inputs() {
    for alias in [false, true] {
        let mut f = Fixture::new();
        f.ordinary_info_cache_v2(false);
        if alias {
            f.word(0xc8168, 0xdead0000);
        } else {
            f.word(0xcb000, 0x10000 + 0x4357160);
        }
        assert!(ordinary_info_value(&f)["frame_raw_geometry"].is_null());
        f.assert_not_read(&[0xc8114, 0xc80b0, 0xcb111, 0xc3e70]);
    }
}
#[test]
fn ordinary_info_geometry_nonfinite_bounds_and_owner_refused() {
    for value in [f32::NAN, f32::INFINITY, 1_000_001.0] {
        let mut f = Fixture::new();
        f.ordinary_info_cache_v2(false);
        f.put(0xc8114, &value.to_le_bytes());
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            "invalid_gui_geometry"
        );
    }
    let mut f = Fixture::new();
    f.ordinary_info_cache_v2(false);
    f.word(0xc8138, 0xdead0000);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_owner"
    );
    f.assert_not_read(&[0xc8114, 0xcb111]);
    let mut f = Fixture::new();
    f.ordinary_info_cache_v2(false);
    f.number(0xc8120, -1);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_geometry"
    );
}
#[test]
fn ordinary_info_geometry_orientation_high_bytes_and_mask_are_raw_not_hit_authority() {
    let mut f = Fixture::new();
    f.ordinary_info_cache_v2(false);
    f.number(0xcb10c, 256);
    f.put(0xcb1d0, &[1]);
    let v = ordinary_info_value(&f);
    assert_eq!(v["resource_cache"]["orientation_raw"], 256);
    assert_eq!(v["frame_raw_geometry"]["no_mask_rejection_branch"], false);
    assert_eq!(v["frame_raw_geometry"]["hit_origin_calculated"], false);
}
#[test]
fn ordinary_info_geometry_final_mutations_and_hierarchy_join_refuse() {
    for (a, n) in [
        (0xc8114, 4),
        (0xc80b0, 4),
        (0xc80b4, 4),
        (0xc8120, 4),
        (0xc8124, 4),
        (0xc812c, 4),
        (0xc8130, 4),
        (0xcb111, 1),
        (0xcb170, 8),
        (0xcb1d0, 1),
        (0xc3e70, 8),
    ] {
        let mut f = Fixture::new();
        f.ordinary_info_cache_v2(false);
        let mut bytes: Vec<_> = (0..n).map(|i| f.data[&(a + i)]).collect();
        bytes[n as usize - 1] ^= 1;
        f.fault = Some((a, 1, bytes));
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            "torn_read",
            "{a:x}"
        );
    }
    let mut f = Fixture::new();
    f.ordinary_info_cache_v2(false);
    f.word(0xf5108, 0xdead0000);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_hierarchy"
    );
}

impl Fixture {
    fn ordinary_info_text_raw(&mut self) {
        self.ordinary_info_research();
        for (object, table, ti, name, text) in [
            (0xc8000, 0x43593d8, 0x435a100, "11CTextSprite", 0x3511000),
            (
                0xcc000,
                0x4359740,
                0x435a200,
                "15CTextSpriteType",
                0x3511100,
            ),
        ] {
            self.word(object, 0x10000 + table);
            self.word(0x10000 + table - 16, 0);
            self.word(0x10000 + table - 8, 0x10000 + ti);
            self.region(0x10000 + ti, 16);
            self.word(0x10000 + ti + 8, 0x10000 + text);
            self.region(0x10000 + text, 128);
            self.put(0x10000 + text, name.as_bytes());
        }
        self.word(0xc8168, 0xcc000);
        self.number(0xcc08c, 3);
        self.number(0xc8228, 369);
        self.number(0xc822c, 34);
        self.number(0xc8238, 3);
        self.put(0xc8110, &0.0f32.to_le_bytes());
        self.ordinary_info_geometry_fixture();
    }
}
#[test]
fn ordinary_info_text_exact_raw_fields_no_frame_or_projection() {
    let mut f = Fixture::new();
    f.ordinary_info_text_raw();
    let v = ordinary_info_value(&f);
    let t = &v["text_raw_diagnostic"];
    assert_eq!(v["renderer_type_encoding"], "11CTextSprite");
    assert_eq!(t["raw_fields_available"], true);
    assert_eq!(t["resource_count_raw"], 3);
    assert_eq!(t["renderer_width_raw"], 369);
    assert_eq!(t["frame_unwrapping_verified"], false);
    assert_eq!(t["hit_projection_available"], false);
    assert_eq!(t["input_authorized"], false);
    f.assert_not_read(&[0xc8160, 0xc81b0, 0xc8034, 0xcc090, 0xcc170, 0xcc1d0]);
}
#[test]
fn ordinary_info_text_foreign_owner_stops_before_resource() {
    let mut f = Fixture::new();
    f.ordinary_info_text_raw();
    f.word(0xc8138, 0xdead0000);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "invalid_gui_owner"
    );
    f.assert_not_read(&[0xc8168, 0xcc08c, 0xc8228]);
}
#[test]
fn ordinary_info_text_wrong_resource_metadata_only() {
    let mut f = Fixture::new();
    f.ordinary_info_text_raw();
    f.word(0xcc000, 0x10000 + 0x434b668);
    assert_eq!(
        ordinary_info_value(&f)["text_raw_diagnostic"]["resource_layout_verified"],
        false
    );
    f.assert_not_read(&[0xcc08c, 0xc8228, 0xc822c, 0xc8238, 0xc8110]);
}
#[test]
fn ordinary_info_text_bounds_and_nonfinite_unavailable() {
    for (a, val) in [
        (0xcc08c, 0),
        (0xcc08c, -1),
        (0xc8228, -1),
        (0xc822c, 65537),
        (0xc8238, 0),
    ] {
        let mut f = Fixture::new();
        f.ordinary_info_text_raw();
        f.number(a, val);
        assert_eq!(
            ordinary_info_value(&f)["text_raw_diagnostic"]["reason"],
            "text_raw_fields_out_of_bounds"
        );
    }
    for val in [f32::NAN, f32::INFINITY, 1_000_001.0] {
        let mut f = Fixture::new();
        f.ordinary_info_text_raw();
        f.put(0xc8110, &val.to_le_bytes());
        assert_eq!(
            ordinary_info_value(&f)["text_raw_diagnostic"]["raw_fields_available"],
            false
        );
    }
}
#[test]
fn ordinary_info_text_every_new_field_final_mutation_refused() {
    for (a, n) in [
        (0xc8168, 8),
        (0xcc000, 8),
        (0xcc08c, 4),
        (0xc8228, 4),
        (0xc822c, 4),
        (0xc8238, 4),
        (0xc8110, 4),
    ] {
        let mut f = Fixture::new();
        f.ordinary_info_text_raw();
        let mut bytes: Vec<_> = (0..n).map(|i| f.data[&(a + i)]).collect();
        bytes[n as usize - 1] ^= 1;
        f.fault = Some((a, 1, bytes));
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            "torn_read",
            "{a:x}"
        );
    }
}
#[test]
fn ordinary_info_text_read_failure_refuses_and_other_branches_unchanged() {
    let mut f = Fixture::new();
    f.ordinary_info_text_raw();
    f.data.remove(&0xcc08f);
    assert_eq!(
        research_ui::node_detail(&f, "tools1").unwrap_err().code,
        "unexpected_read"
    );
    for kind in 0..3 {
        let mut f = Fixture::new();
        if kind == 0 {
            f.ordinary_info_research();
        } else {
            f.ordinary_info_cache_v2(kind == 2);
        }
        assert!(ordinary_info_value(&f)["text_raw_diagnostic"].is_null());
        f.assert_not_read(&[0xc8228, 0xc822c, 0xc8238, 0xcc08c]);
    }
}

#[test]
fn ordinary_info_text_v2_base_fields_and_typed_leaf_not_ancestor_whitelist() {
    let mut f = Fixture::new();
    f.ordinary_info_text_raw();
    let t = &ordinary_info_value(&f)["text_raw_diagnostic"];
    assert_eq!(t["scalar_raw"], 1.0);
    assert_eq!(
        t["cache_translation_raw"],
        serde_json::json!([100.0, -80.0])
    );
    assert_eq!(t["anchor_raw"], serde_json::json!([0, 0]));
    assert_eq!(t["hit_projection_available"], false);
    assert_eq!(t["parent_chain"]["status"], "closed_to_owned_graphics_root");
    let mut f = Fixture::new();
    f.ordinary_info_text_raw();
    f.word(0xc8108, 0xf6000);
    f.word(0xf6000, 0x10000 + 0x43593d8);
    let t = &ordinary_info_value(&f)["text_raw_diagnostic"];
    assert_eq!(t["parent_chain"]["status"], "unknown_ancestor");
    f.assert_not_read(&[0xf6070, 0xf6138, 0xf60c0]);
}
#[test]
fn ordinary_info_text_v2_common_bounds_nonfinite_and_high_unsigned_anchor() {
    for a in [0xc8114, 0xc80b0, 0xc80b4] {
        let mut f = Fixture::new();
        f.ordinary_info_text_raw();
        f.put(a, &f32::NAN.to_le_bytes());
        assert_eq!(
            ordinary_info_value(&f)["text_raw_diagnostic"]["reason"],
            "text_common_fields_out_of_bounds"
        );
        f.assert_not_read(&[0xc3e70]);
    }
    for a in [0xc8120, 0xc8124, 0xc812c, 0xc8130] {
        let mut f = Fixture::new();
        f.ordinary_info_text_raw();
        f.number(a, -1);
        assert_eq!(
            ordinary_info_value(&f)["text_raw_diagnostic"]["reason"],
            "text_common_fields_out_of_bounds"
        );
    }
}
#[test]
fn ordinary_info_text_v2_common_bytes_and_hierarchy_final_mutation_refuse() {
    for (a, n) in [
        (0xc8114, 4),
        (0xc80b0, 4),
        (0xc80b4, 4),
        (0xc8120, 4),
        (0xc8124, 4),
        (0xc812c, 4),
        (0xc8130, 4),
        (0xc3e70, 8),
        (0xc8108, 8),
    ] {
        let mut f = Fixture::new();
        f.ordinary_info_text_raw();
        let mut bytes: Vec<_> = (0..n).map(|i| f.data[&(a + i)]).collect();
        bytes[n as usize - 1] ^= 1;
        f.fault = Some((a, 1, bytes));
        assert_eq!(
            research_ui::node_detail(&f, "tools1").unwrap_err().code,
            "torn_read",
            "{a:x}"
        );
    }
}

impl Fixture {
    fn registered_slot_control(&mut self) {
        self.research_plain_sprite();
        let b = 0x10000;
        self.word(0xb1068, 0x70000);
        self.word(0xb1080, b + 0x4346ea8);
        self.word(0xb1088, 0xe0000);
        self.word(0xb1090, 0xe0000);
        self.number(0xb1098, 1);
        self.word(0xb10a0, 0);
        self.region(0xe0000, 0x20);
        self.word(0xe0000, 0x94020);
        self.word(0x94020, b + 0x424ea18);
        self.word(0x94028, 0xe1000);
        self.word(0x94038, b + 0x15b7be0);
        self.word(0x94040, b + 0x15b7bb0);
        self.region(0xe1000, 0x18);
        self.word(0xe1000, b + 0x15b35e0);
        self.word(0xe1010, 0x94000);
        self.word(0xb2138, 0xb1000);
    }
}
#[test]
fn targeted_slot_control_uses_same_production_own_model_and_witnesses() {
    let mut f = Fixture::new();
    f.registered_slot_control();
    let v = research_ui::slot_control_snapshot(&f, 0).unwrap();
    assert_eq!(
        v["descriptor"]["ordinary_selection_binding"]["binding_verified"],
        true
    );
    assert_eq!(v["selected_slot"], 1);
    assert_eq!(v["input_authorized"], false);
    assert!(!f.tracked_reads.borrow().contains_key(&0xb1008));
    assert!(!f.tracked_reads.borrow().contains_key(&0xb10ac));
    assert!(!f.tracked_reads.borrow().contains_key(&0xb10e8));
    f.fault = Some((0xe1000, 1, vec![0; 8]));
    f.fault_reads.set(0);
    assert_eq!(
        research_ui::slot_control_snapshot(&f, 0).unwrap_err().code,
        "torn_read"
    );
}
#[test]
fn targeted_slot_foreign_registry_refuses_before_renderer_payload() {
    let mut f = Fixture::new();
    f.registered_slot_control();
    f.word(0xb1068, 0x70008);
    for a in 0xb1110..0xb1118 {
        f.data.remove(&a);
    }
    assert_eq!(
        research_ui::slot_control_snapshot(&f, 0).unwrap_err().code,
        "invalid_gui_owner"
    );
    assert!(!f.tracked_reads.borrow().contains_key(&0xb1110));
    assert!(!f.tracked_reads.borrow().contains_key(&0xb10ac));
}
#[test]
fn targeted_slot_is_current_own_index_not_persistent_identity() {
    let mut f = Fixture::new();
    f.registered_slot_control();
    assert_eq!(
        research_ui::slot_control_snapshot(&f, 32).unwrap_err().code,
        "invalid_arguments"
    );
    assert!(f.tracked_reads.borrow().is_empty());
    assert_eq!(
        research_ui::slot_control_snapshot(&f, 4).unwrap_err().code,
        "invalid_index"
    );
    let server = hoi4_live_native::protocol::Server {
        process: None,
        unavailable: None,
    };
    for args in [
        serde_json::json!({}),
        serde_json::json!({"slot":-1}),
        serde_json::json!({"slot":32}),
        serde_json::json!({"slot":0,"other":true}),
        serde_json::json!({"slot":0,"include_geometry":1}),
    ] {
        let v=server.handle(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_research_slot_control","arguments":args}})).unwrap();
        assert!(v.to_string().contains("invalid_arguments"));
    }
}

fn owned_slot_geometry_fixture() -> Fixture {
    let mut f = owned_node_fixture();
    f.registered_slot_control();
    // Reuse supported Sprite semantics, with an independent slot callback and
    // hierarchy. The technology button-data field is deliberately unreadable.
    for (from, to, length) in [(0xf0000, 0xb2000, 0x180), (0xf1000, 0xb3000, 0x1f0)] {
        let bytes: Vec<_> = (0..length).map(|i| f.data[&(from + i)]).collect();
        f.put(to, &bytes);
    }
    f.word(0xb2138, 0xb1000);
    f.word(0xb2168, 0xb3000);
    f.hierarchy_fixture();
    f.word(0xa0e70, 0xf3000);
    f.word(0xa0e78, 0xf4000);
    f.word(0xf5100, 0xb2000);
    for address in 0xb1008..0xb1010 {
        f.data.remove(&address);
    }
    f
}

#[test]
fn slot_geometry_uses_slot_callback_and_same_owned_context_without_technology_data() {
    let f = owned_slot_geometry_fixture();
    let (v, context) = research_ui::slot_control_owned(&f, 0).unwrap();
    let binding = &v["descriptor"]["ordinary_selection_binding"];
    assert_eq!(binding["binding_verified"], true);
    assert_eq!(binding["render_diagnostic"]["field_layout_verified"], true);
    assert_eq!(
        binding["render_diagnostic"]["resource_cache"]["diagnostic_dimensions"],
        serde_json::json!([300, 60])
    );
    assert_eq!(
        binding["render_diagnostic"]["parent_chain"]["graphics_root_closed"],
        true
    );
    assert_eq!(context.campaign.player, "GER");
    assert_eq!(context.anchors[..2], [0x20000, 0x28000]);
    assert_eq!(context.slots, [0x2c000, 0x2c100, 0x2c200, 0x2c300]);
    assert_eq!(v["input_authorized"], false);
    assert_eq!(binding["input_authorized"], false);
    assert!(!f.tracked_reads.borrow().contains_key(&0xb1008));
}

#[test]
fn slot_geometry_requires_callback_scope_before_resource_or_hierarchy() {
    let mut f = owned_slot_geometry_fixture();
    f.word(0xe1010, 0x94008);
    let v = research_ui::slot_control_geometry(&f, 0).unwrap();
    assert_eq!(
        v["descriptor"]["ordinary_selection_binding"]["reason"],
        "different_slot_callback_capture"
    );
    f.assert_not_read(&[0xb2168, 0xb3090, 0xa0e70]);
    let mut f = owned_slot_geometry_fixture();
    f.word(0xb2138, 0xb1008);
    assert_eq!(
        research_ui::slot_control_geometry(&f, 0).unwrap_err().code,
        "invalid_gui_owner"
    );
    f.assert_not_read(&[0xb3090, 0xa0e70]);
}

#[test]
fn slot_geometry_full_orientation_and_final_witnesses_remain_required() {
    let mut f = owned_slot_geometry_fixture();
    f.number(0xb310c, 0x100);
    let v = research_ui::slot_control_geometry(&f, 0).unwrap();
    let cache =
        &v["descriptor"]["ordinary_selection_binding"]["render_diagnostic"]["resource_cache"];
    assert_eq!(cache["orientation_raw"], 0x100);
    assert!(cache["diagnostic_dimensions"].is_null());
    for (address, length) in [(0xb310c, 4), (0xb2138, 8), (0xf5100, 8), (0x3b000, 32)] {
        let mut f = owned_slot_geometry_fixture();
        let mut changed: Vec<_> = (0..length).map(|i| f.data[&(address + i)]).collect();
        changed[0] ^= 1;
        f.fault = Some((address, 1, changed));
        assert_eq!(
            research_ui::slot_control_owned(&f, 0).unwrap_err().code,
            "torn_read",
            "{address:x}"
        );
    }
}

fn owned_category_fixture() -> Fixture {
    let mut f = owned_node_fixture();
    let b = 0x10000;
    for (p, t, n) in [
        (0x210000, 0x42113f0, 0x10),
        (0x211000, 0x42112c0, 0xd0),
        (0x212000, 0x4347d58, 0xf00),
        (0x213000, 0x4347d58, 0xf00),
        (0x214000, 0x43470c8, 0x160),
    ] {
        f.region(p, n);
        f.word(p, b + t);
    }
    f.word(b + 0x43a40e0, 0x210000);
    f.array(0x360a8, 0x215000, 2);
    f.word(0x215000, 0x210000);
    f.word(0x215008, 0x211000);
    f.string(0x211008, "industry_folder");
    for p in [0x212000, 0x213000] {
        f.word(p + 0x98, 0x70000);
    }
    f.word(0x214068, 0x70000);
    f.word(0x214008, 0x213000);
    f.number(0x2140c8, 0);
    f.array(0xd0bb0, 0x216000, 2);
    f.word(0x216000, 0x212000);
    f.word(0x216008, 0x213000);
    f.array(0xd0bc8, 0x217000, 2);
    f.string(0x217000, "folder_tabs");
    f.string(0x217020, "industry_folder");
    f.array(0x2129a0, 0x218000, 1);
    f.word(0x218000, 0x214000);
    f.array(0x2129b8, 0x219000, 1);
    // The real named tab key exceeds libstdc++'s inline-string capacity.
    f.region(0x219000, 32);
    f.word(0x219000, 0x21e000);
    f.word(0x219008, 19);
    f.word(0x219010, 19);
    f.region(0x21e000, 20);
    f.put(0x21e000, b"industry_folder_tab");
    f.array(0x92d78, 0x21a000, 1);
    f.word(0x21a000, 0x213000);
    f.array(0x92d90, 0x21b000, 1);
    f.word(0x21b000, 0x214000);
    f.number(0x92e10, 0);
    f.word(0x214080, b + 0x4347488);
    f.word(0x214088, 0x21c000);
    f.word(0x214090, 0x21c000);
    f.number(0x214098, 1);
    f.region(0x21c000, 0x20);
    f.word(0x21c000, 0x92358);
    f.word(0x92358, b + 0x424e0a8);
    f.word(0x92380, 0x21d000);
    f.word(0x92390, b + 0x15a5710);
    f.word(0x92398, b + 0x15a56e0);
    f.region(0x21d000, 0x18);
    f.word(0x21d000, b + 0x1599270);
    f.word(0x21d010, 0x92000);
    f
}
#[test]
fn own_category_body_joins_same_witnessed_campaign_and_selected_slot() {
    let f = owned_category_fixture();
    let (v, c) = research_ui::category_owned(&f, "industry_folder").unwrap();
    assert_eq!(v["category"]["binding_available"], true);
    assert_eq!(v["category"]["selected_folder_key"], "industry_folder");
    assert_eq!(v["selected_slot"], 1);
    assert_eq!(c.campaign.player, "GER");
    assert_eq!(c.slots, [0x2c000, 0x2c100, 0x2c200, 0x2c300]);
    assert_eq!(v["input_authorized"], false);
    assert!(!v.to_string().contains(&0x21d000u64.to_string()));
}
#[test]
fn category_context_gates_and_late_capture_and_slot_changes_refuse() {
    let mut f = owned_category_fixture();
    f.number(0x903d8, 5);
    assert_eq!(
        research_ui::category_snapshot(&f, "industry_folder")
            .unwrap_err()
            .code,
        "gui_unavailable"
    );
    f.assert_not_read(&[0x360a8, 0x214068]);
    for (address, length) in [(0x21d000, 8), (0x3b000, 32), (0x213098, 8)] {
        let mut f = owned_category_fixture();
        let mut bytes: Vec<_> = (0..length).map(|i| f.data[&(address + i)]).collect();
        bytes[0] ^= 1;
        f.fault = Some((address, 1, bytes));
        assert_eq!(
            research_ui::category_owned(&f, "industry_folder")
                .unwrap_err()
                .code,
            "torn_read",
            "{address:x}"
        );
    }
}

#[test]
fn category_geometry_preserves_context_and_unknown_layout_limits() {
    let mut f = owned_category_fixture();
    let (v, c) = research_ui::category_with_geometry_owned(&f, "industry_folder", true).unwrap();
    assert_eq!(c.campaign.player, "GER");
    assert_eq!(c.slots, [0x2c000, 0x2c100, 0x2c200, 0x2c300]);
    assert_eq!(v["category"]["binding_available"], true);
    assert_eq!(
        v["category"]["sprite_diagnostic"]["reason"],
        "renderer_layout_unavailable"
    );
    f.assert_not_read(&[0x214110]);
    f.word(0x214000, 0x10000 + 0x4346b10);
    f.word(0x214080, 0x10000 + 0x4346ea8);
    let (v, _) = research_ui::category_with_geometry_owned(&f, "industry_folder", true).unwrap();
    assert_eq!(
        v["category"]["sprite_diagnostic"]["field_layout_verified"],
        false
    );
    assert_eq!(v["category"]["input_authorized"], false);
    assert_eq!(v["category"]["geometry_available"], false);
}
#[test]
fn category_geometry_final_witness_refuses_changed_renderer_identity() {
    let mut f = owned_category_fixture();
    f.word(0x214000, 0x10000 + 0x4346b10);
    f.word(0x214080, 0x10000 + 0x4346ea8);
    f.fault = Some((0x214110, 1, 0x21f000u64.to_le_bytes().to_vec()));
    assert_eq!(
        research_ui::category_with_geometry_owned(&f, "industry_folder", true)
            .unwrap_err()
            .code,
        "torn_read"
    );
}

#[test]
fn category_command_body_observes_new_info_and_selected_assignment_under_witnesses() {
    let mut f = owned_category_fixture();
    f.word(0x2c118, 0);
    let (old, c) = research_ui::category_command_owned(&f, "industry_folder").unwrap();
    assert_eq!(old["category"]["selected_slot_empty"], true);
    assert_eq!(old["technology_info"]["visible_count"], 0);
    f.target_info(0);
    let (new, n) = research_ui::category_command_owned(&f, "industry_folder").unwrap();
    assert_eq!(n, c);
    assert_eq!(new["technology_info"]["visible_count"], 1);
    assert_eq!(new["category"]["selected_slot_empty"], true);
    f.word(0x2c118, 0x81000);
    let (new, _) = research_ui::category_command_owned(&f, "industry_folder").unwrap();
    assert_eq!(new["category"]["selected_slot_empty"], false);
    let mut f = owned_category_fixture();
    f.word(0x2c118, 0);
    f.fault = Some((0x2c118, 1, 0x80000u64.to_le_bytes().to_vec()));
    assert_eq!(
        research_ui::category_command_owned(&f, "industry_folder")
            .unwrap_err()
            .code,
        "torn_read"
    );
}

#[test]
fn unified_category_preserves_complete_canonical_completed_and_slot_guards() {
    let mut f = owned_category_fixture();
    f.word(0x2c118, 0);
    let (v, c) = research_ui::category_command_owned(&f, "industry_folder").unwrap();
    assert_eq!(v["canonical_research"]["researched_count"], 1);
    assert_eq!(
        v["canonical_research"]["slots"][1]["researching"],
        serde_json::Value::Null
    );
    assert_eq!(c.slots.len(), 4);
    let mut f = owned_category_fixture();
    f.number(0x81174, 0);
    assert_eq!(
        research_ui::category_command_owned(&f, "industry_folder")
            .unwrap_err()
            .code,
        "invalid_completed_research"
    );
    f.assert_not_read(&[0x92dc0, 0x214068]);
    let mut f = owned_category_fixture();
    f.word(0x2c118, 0x80000);
    assert_eq!(
        research_ui::category_command_owned(&f, "industry_folder")
            .unwrap_err()
            .code,
        "invalid_research_join"
    );
}
#[test]
fn unified_category_late_info_slot_and_root_mutations_prevent_dispatch() {
    use hoi4_live_native::{
        actions::Epoch,
        input_lease::InputAuthority,
        platform::Identity,
        research_stage::{Backend, Candidate, Delivery, Journal, OwnState, Stage, Target},
    };
    use std::sync::RwLock;
    use std::time::{Duration, Instant};
    struct Gate<'a> {
        f: &'a Fixture,
        sends: usize,
    }
    impl Backend for Gate<'_> {
        fn observe(&mut self) -> Result<OwnState> {
            research_ui::category_command_owned(self.f, "industry_folder")?;
            panic!("Poisoned observation must refuse before stage state")
        }
        fn candidate(&mut self, _: &Target) -> Result<Candidate> {
            panic!("No candidate after refused observation")
        }
        fn dispatch_once(&mut self, _: &Candidate, _: Instant) -> Delivery {
            self.sends += 1;
            Delivery::Unknown
        }
        fn confirm_input_idle(&mut self, _: &Candidate, _: Instant) -> Result<()> {
            Ok(())
        }
    }
    for (i, (a, bytes)) in [
        (0xc1a78, 0xdead0000u64.to_le_bytes().to_vec()),
        (0x2c118, 0x81000u64.to_le_bytes().to_vec()),
        (0x10000 + 0x44085f8, 0x32008u64.to_le_bytes().to_vec()),
    ]
    .into_iter()
    .enumerate()
    {
        let mut f = owned_category_fixture();
        f.word(0x2c118, 0);
        f.target_info(0);
        f.fault = Some((a, 1, bytes));
        let epoch = Epoch {
            game: Identity {
                pid: 120,
                start_ticks: "456".into(),
                exe: "/fixture/game".into(),
                uid: 1000,
            },
            campaign: "fixture".into(),
            player: "GER".into(),
        };
        let mut authority = InputAuthority::new("test".into(), epoch.clone()).unwrap();
        let token = "a".repeat(32);
        authority
            .acquire("test", token.clone(), 300, &epoch)
            .unwrap();
        let directory = std::env::temp_dir().join(format!(
            "hoi4-category-mutation-{}-{i}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let path = directory.join("journal");
        let mut journal = Journal::open(&path).unwrap();
        let mut gate = Gate { f: &f, sends: 0 };
        let target = Target {
            epoch,
            stage: Stage::SelectCategory,
            technology: String::new(),
            folder_id: Some("industry_folder".into()),
            slot: 1,
        };
        assert_eq!(
            journal
                .execute(
                    "mutation",
                    target,
                    &token,
                    &authority,
                    &RwLock::new(true),
                    &mut gate,
                    Duration::from_secs(5)
                )
                .unwrap_err()
                .code,
            "torn_read"
        );
        assert_eq!(gate.sends, 0);
        drop(journal);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(directory).unwrap();
    }
}
#[test]
fn unified_category_read_count_measurement_per_verification_phase() {
    let f = owned_category_fixture();
    research_ui::category_command_owned(&f, "industry_folder").unwrap();
    let logical: usize = f.tracked_reads.borrow().values().sum();
    let reference = owned_category_fixture();
    // These public paths exclude the old UI wait's extra campaign reads and
    // selected-empty byte, so this conservatively undercounts the former work.
    research::owned_snapshot(&reference).unwrap();
    research_ui::snapshot(&reference).unwrap();
    research_ui::category_owned(&reference, "industry_folder").unwrap();
    research::owned_snapshot(&reference).unwrap();
    let former_lower_bound: usize = reference.tracked_reads.borrow().values().sum();
    println!("UNIFIED_CATEGORY_LOGICAL_READS={logical}; FORMER_LOWER_BOUND={former_lower_bound}");
    assert!(logical < former_lower_bound);
}

include!("support/daily_observation_cases.rs");

// A public folder catalog fixture with no GUI manager, registry, windows,
// buttons or callbacks mapped. Campaign/own-country fields use the independent
// overview fixture; reserved and folder primaries are constructed separately.
fn folder_catalog_key(f: &mut Fixture, pointer: u64, key: &str, heap: u64) {
    f.region(pointer, 40);
    f.word(pointer, 0x10000 + 0x42112c0);
    if key.len() <= 15 {
        f.string(pointer + 8, key);
    } else {
        f.word(pointer + 8, heap);
        f.word(pointer + 16, key.len() as u64);
        f.word(pointer + 24, key.len() as u64);
        f.put(heap, key.as_bytes());
    }
}
fn folder_catalog_fixture() -> Fixture {
    let mut f = Fixture::new();
    f.catalog();
    f.word(0x10000 + 0x43a40e0, 0x210000);
    f.word(0x210000, 0x10000 + 0x42113f0); // No readable sentinel payload.
    for (pointer, key, heap) in [
        (0x211000, "industry_folder", 0),
        (0x211100, "infantry_folder", 0),
        (0x211200, "electronics_folder", 0x216000),
    ] {
        folder_catalog_key(&mut f, pointer, key, heap);
    }
    f.array(0x360a8, 0x215000, 4);
    for (index, pointer) in [0x210000, 0x211000, 0x211100, 0x211200]
        .into_iter()
        .enumerate()
    {
        f.word(0x215000 + index as u64 * 8, pointer);
    }
    f
}

#[test]
fn research_folders_are_exact_sorted_and_independent_of_closed_or_unmapped_gui() {
    let f = folder_catalog_fixture();
    let (catalog, context) = research_folders::owned_snapshot(&f).unwrap();
    assert_eq!(
        catalog.ids,
        ["electronics_folder", "industry_folder", "infantry_folder"]
    );
    assert!(catalog.contains("industry_folder"));
    for alias in ["industry", "Industry_folder", "industry_folder "] {
        assert!(!catalog.contains(alias), "No invented alias: {alias}");
    }
    assert_eq!(context.player, "GER");
    assert_eq!(context.anchors, [0x30000, 0x32000, 0x36000]);
    assert_eq!(f.checks.get(), 2); // One begin/end bracket, no nested snapshot.
    f.assert_not_read(&[
        0x326b8, 0x324f8, 0x90000, 0x903d8, 0x91000, 0x92000, 0x70000, 0x210008, 0xdead0000,
        0xbeef0000,
    ]);
    let value = research_folders::snapshot(&f).unwrap();
    assert_eq!(value["ids"], serde_json::json!(catalog.ids));
    assert_eq!(value["count"], 3);
    assert_eq!(value["own_player"], "GER");
    assert_eq!(value["requires_open_research_view"], false);
    for field in [
        "technology_folder_mapping_available",
        "eligibility_available",
        "geometry_available",
        "input_authorized",
        "observation_paused_game",
    ] {
        assert_eq!(value[field], false, "{field}");
    }
    assert!(value["freshness"]["read_calls"].as_u64().unwrap() > 0);
    for address in [0x30000u64, 0x36000, 0x210000, 0x211000, 0x216000] {
        assert!(!value.to_string().contains(&address.to_string()));
    }
}

#[test]
fn research_folders_positive_own_country_and_status_precede_catalog_payload() {
    for (address, value, code) in [
        (0x30518, 0, "player_unavailable"),
        (0x28138, 2, "invalid_research_owner"),
    ] {
        let mut f = folder_catalog_fixture();
        f.number(address, value);
        assert_eq!(research_folders::owned_snapshot(&f).unwrap_err().code, code);
        f.assert_not_read(&[0x360a8, 0x215000, 0x211008, 0x216000]);
    }
    let mut f = folder_catalog_fixture();
    f.word(0x28130, 0xdead0000);
    assert_eq!(
        research_folders::owned_snapshot(&f).unwrap_err().code,
        "invalid_research_owner"
    );
    f.assert_not_read(&[0x360a8, 0xdead0000]);
}

#[test]
fn research_folders_capacity_bound_rejects_before_unreadable_array_payload() {
    for (capacity, count, data) in [
        (129, 4, 0xdead0000),
        (128, 129, 0xdead0000),
        (-1, 0, 0),
        (4, -1, 0x215000),
        (4, 4, 0x215001),
        (4, 4, 0),
    ] {
        let mut f = folder_catalog_fixture();
        f.word(0x360a8, data);
        f.number(0x360b0, capacity);
        f.number(0x360b4, count);
        assert_eq!(
            research_folders::owned_snapshot(&f).unwrap_err().code,
            "invalid_array",
            "capacity={capacity}, count={count}, data={data:x}"
        );
        f.assert_not_read(&[0x215000, 0x215001, 0xdead0000]);
    }
}

#[test]
fn research_folders_accept_complete_policy_boundary_and_known_empty_catalog() {
    let mut f = folder_catalog_fixture();
    f.array(0x360a8, 0x220000, 128);
    f.word(0x220000, 0x210000);
    for index in 0..127u64 {
        let pointer = 0x300000 + index * 0x100;
        folder_catalog_key(&mut f, pointer, &format!("folder_{index:03}"), 0);
        f.word(0x220008 + index * 8, pointer);
    }
    let (catalog, _) = research_folders::owned_snapshot(&f).unwrap();
    assert_eq!(catalog.ids.len(), 127); // 128 includes the reserved singleton.
    assert_eq!(catalog.ids.first().unwrap(), "folder_000");
    assert_eq!(catalog.ids.last().unwrap(), "folder_126");
    let mut f = folder_catalog_fixture();
    f.array(0x360a8, 0x215000, 1);
    f.number(0x360b0, 128);
    let value = research_folders::snapshot(&f).unwrap();
    assert_eq!(value["ids"], serde_json::json!([]));
    assert_eq!(value["count"], 0);
    assert_eq!(value["input_authorized"], false);
    f.assert_not_read(&[0x211000, 0x211008, 0x210008]);
}

#[test]
fn research_folders_require_exact_database_folder_and_first_reserved_singleton() {
    for (address, value, code, forbidden) in [
        (0x36000, 0x10000 + 0x42112c0, "invalid_gui_type", 0x215000),
        (0x210000, 0x10000 + 0x42112c0, "invalid_gui_type", 0x211008),
        (0x215000, 0x211000, "invalid_category_ui", 0x211008),
        (0x215008, 0x210000, "invalid_gui_type", 0x210008),
        (0x211000, 0x10000 + 0x42113f0, "invalid_gui_type", 0x211008),
        (0x215008, 0x211001, "invalid_gui_type", 0x211001),
        (0x215008, 0, "read_bound", 0),
    ] {
        let mut f = folder_catalog_fixture();
        f.word(address, value);
        assert_eq!(research_folders::owned_snapshot(&f).unwrap_err().code, code);
        f.assert_not_read(&[forbidden]);
    }
    let mut f = folder_catalog_fixture();
    f.array(0x360a8, 0, 0);
    assert_eq!(
        research_folders::owned_snapshot(&f).unwrap_err().code,
        "invalid_category_ui"
    );
    f.assert_not_read(&[0x211008, 0x210008]);
}

#[test]
fn research_folders_reject_duplicate_keys_and_pointers_empty_control_and_invalid_utf8() {
    for key in ["industry_folder", "", "bad\nfolder"] {
        let mut f = folder_catalog_fixture();
        f.string(0x211108, key);
        assert_eq!(
            research_folders::owned_snapshot(&f).unwrap_err().code,
            "invalid_category_ui",
            "{key:?}"
        );
    }
    let mut f = folder_catalog_fixture();
    f.word(0x215010, 0x211000);
    assert_eq!(
        research_folders::owned_snapshot(&f).unwrap_err().code,
        "invalid_category_ui"
    );
    let mut f = folder_catalog_fixture();
    f.put(0x211018, &[0xff]);
    assert_eq!(
        research_folders::owned_snapshot(&f).unwrap_err().code,
        "invalid_string"
    );
}

#[test]
fn research_folders_key_limit_is_checked_before_heap_payload() {
    let mut f = folder_catalog_fixture();
    folder_catalog_key(&mut f, 0x211000, &"x".repeat(256), 0x217000);
    assert!(research_folders::owned_snapshot(&f)
        .unwrap()
        .0
        .contains(&"x".repeat(256)));
    let mut f = folder_catalog_fixture();
    f.word(0x211008, 0xdead0000);
    f.word(0x211010, 257);
    assert_eq!(
        research_folders::owned_snapshot(&f).unwrap_err().code,
        "invalid_string"
    );
    f.assert_not_read(&[0xdead0000]);
}

#[test]
fn research_folders_final_witnesses_cover_catalog_and_bound_own_campaign() {
    for (address, width) in [
        (0x10000 + 0x439f450, 8),
        (0x36000, 8),
        (0x360a8, 16),
        (0x215000, 32),
        (0x10000 + 0x43a40e0, 8),
        (0x210000, 8),
        (0x211000, 8),
        (0x211008, 32),
        (0x216000, 18),
        (0x10000 + 0x44086a8, 8),
        (0x39004, 4),
        (0x28130, 8),
        (0x28138, 4),
        (0x3b000, 32),
        (0x60000, 36),
        (0x30518, 4),
        (0x30620, 4),
        (0x30628, 4),
        (0x326c1, 1),
    ] {
        let mut f = OwnedContextFixture::new();
        f.fixture = folder_catalog_fixture();
        let mut bytes: Vec<_> = (0..width)
            .map(|index| f.fixture.data[&(address + index as u64)])
            .collect();
        bytes[0] ^= 1;
        f.final_mutation = Some((address, bytes));
        assert_eq!(
            research_folders::owned_snapshot(&f).unwrap_err().code,
            "torn_read",
            "field {address:x}"
        );
    }
}

#[test]
fn research_folders_identical_list_relocation_is_not_a_stable_observation() {
    let mut f = OwnedContextFixture::new();
    f.fixture = folder_catalog_fixture();
    let identical: Vec<_> = (0..32)
        .map(|index| f.fixture.data[&(0x215000 + index)])
        .collect();
    f.fixture.put(0x218000, &identical);
    let mut header = 0x218000u64.to_le_bytes().to_vec();
    header.extend(4i32.to_le_bytes());
    header.extend(4i32.to_le_bytes());
    f.final_mutation = Some((0x360a8, header));
    assert_eq!(
        research_folders::owned_snapshot(&f).unwrap_err().code,
        "torn_read"
    );
}

#[test]
fn research_folders_identity_failure_does_not_publish_a_completed_catalog() {
    let mut f = folder_catalog_fixture();
    f.replaced = true;
    assert_eq!(
        research_folders::owned_snapshot(&f).unwrap_err().code,
        "ownership_changed"
    );
    assert_eq!(f.checks.get(), 2);
}

#[path = "support/assignment_wait_cases.rs"]
mod assignment_wait_cases;
