//! Portable unit fixtures included by the crate-private selection observer.
//! Every byte is generated; neither this host nor a game process is opened.
use super::*;
use crate::{
    construction::mac_ui::{observe_choice, Options},
    memory::StringAbi,
    own_focus::assignment_fixture::{Fixture, BASE, COUNTRY, DATE, FOREIGN, GS, IDLER},
};

const PRODUCTION: u64 = 0x900000;
const DATABASE: u64 = 0x910000;
const TEMPLATE: u64 = 0x920000;
const STATE: u64 = 0x930000;
const LINE: u64 = 0x940000;
const BUILDING: u64 = 0x950000;
const NULL: u64 = 0x960000;
const OTHER: u64 = 0x970000;
const GUI: u64 = 0x1000000;
const MANAGER: u64 = 0x1010000;
const VIEW: u64 = 0x1020000;
const ROOT: u64 = 0x1030000;
const POSSIBLE: u64 = 0x1040000;
const SECTION: u64 = 0x1050000;
const GRID: u64 = 0x10a0000;
const ITEM: u64 = 0x1100000;
const FRAME: u64 = 0x1200000;
const BUTTON: u64 = 0x1210000;
const NODE: u64 = 0x1220000;
const FUNCTION: u64 = 0x1230000;
const GRAPHICS: u64 = 0x2000000;
const SPRITE: u64 = 0x2100000;
const SPRITE_TYPE: u64 = 0x2110000;

fn template(f: &mut Fixture, pointer: u64, ordinal: i32, key: &str) {
    f.word(pointer, BASE + 0x332e908);
    f.put(pointer + 0x18, &[1]);
    f.int(pointer + 0x290, ordinal);
    f.string(pointer + 0x228, key);
}

fn fixture() -> Fixture {
    let mut f = Fixture::new(StringAbi::LibCxx);
    for (field, value) in [
        (COUNTRY + 0xd10, PRODUCTION),
        (PRODUCTION, BASE + 0x338b678),
        (PRODUCTION + 0x290, COUNTRY),
        (BASE + 0x34d0ac0, DATABASE),
        (DATABASE, BASE + 0x332dd28),
        (NULL, BASE + 0x332def8),
        (STATE, BASE + 0x337caf0),
        (IDLER + 0x4c0, GUI),
        (GUI, BASE + 0x34974d0),
        (IDLER + 0x670, MANAGER),
        (MANAGER + 0x180, IDLER),
    ] {
        f.word(field, value);
    }
    f.put(NULL + 0x18, &[0]);
    template(&mut f, TEMPLATE, 1, "arms_factory");
    f.array(DATABASE + 0x40, &[NULL, TEMPLATE]);
    f.array(PRODUCTION + 0x70, &[]);
    f.array(GS + 0x290, &[0, STATE]);
    f.int(STATE + 0x40, 1);
    f.int(STATE + 0xc0, 1);
    f.int(STATE + 0xc4, 1);
    f.int(MANAGER + 0x3c0, 0);
    f
}

fn selected(f: &mut Fixture, pointer: u64) {
    f.int(MANAGER + 0x3c0, 1);
    f.word(MANAGER + 0x3b8, pointer);
    f.word(DATABASE + 0x3a8, NULL);
}

fn queue(f: &mut Fixture) {
    for (field, value) in [
        (LINE, BASE + 0x3389c40),
        (LINE + 8, 0x10000003b),
        (LINE + 0x20, PRODUCTION),
        (LINE + 0x28, 100000),
        (LINE + 0x30, 100),
        (LINE + 0x38, 0),
        (LINE + 0x70, BUILDING),
        (BUILDING, BASE + 0x332dcb0),
        (BUILDING + 0x1a8, STATE + 0x110),
        (BUILDING + 0x1b0, TEMPLATE),
        (STATE + 0x110, BASE + 0x332e6a8),
    ] {
        f.word(field, value);
    }
    for (field, value) in [
        (LINE + 0x14, 1),
        (LINE + 0x18, 0),
        (LINE + 0x40, 1),
        (LINE + 0x44, 0),
        (LINE + 0x48, 2),
        (LINE + 0x78, 0),
        (STATE + 0x178, 2),
        (STATE + 0x17c, 1),
        (STATE + 0x180, 0),
    ] {
        f.int(field, value);
    }
    f.put(LINE + 0x8c, &[0]);
    f.put(LINE + 0x99, &[0]);
    f.array(PRODUCTION + 0x70, &[LINE]);
    // State building ordinal map: index1 -> local building index0.
    f.array(STATE + 0x130, &[0]);
    let index = f.get_word(STATE + 0x130);
    f.int(STATE + 0x138, 2);
    f.int(STATE + 0x13c, 2);
    f.int(index + 4, 0);
    f.array(STATE + 0x148, &[BUILDING]);
}

fn name(f: &mut Fixture, widget: u64, key: &str) {
    let kind = widget + 0x3000000;
    f.word(widget + 0x50, kind);
    f.word(widget + 0x58, GUI);
    f.word(kind, BASE + 0x34b0e18);
    f.string(kind + 0x28, key);
}
fn window(f: &mut Fixture, object: u64, key: &str, children: &[u64]) {
    f.word(object, BASE + 0x34ac0b0);
    f.word(object + 0x30, 0);
    f.word(object + 0x88, GUI);
    f.put(object + 0x95, &[8]);
    f.int(object + 0x12b8, 0);
    f.int(object + 0x1270, 0);
    name(f, object + 0x30, key);
    f.array(object + 0xd30, children);
}

fn ui(f: &mut Fixture) {
    f.int(MANAGER + 0x3a8, 17);
    for (field, value) in [
        (MANAGER + 0x150, VIEW),
        (VIEW, BASE + 0x343aa90),
        (VIEW + 0x428, IDLER),
        (VIEW + 0x30, MANAGER),
        (VIEW + 0x430, ROOT),
    ] {
        f.word(field, value);
    }
    window(f, ROOT, "country_constructions", &[POSSIBLE + 0x30]);
    window(
        f,
        POSSIBLE,
        "possible_constructions",
        &(0..4)
            .map(|i| SECTION + i * 0x10000 + 0x30)
            .collect::<Vec<_>>(),
    );
    for (i, (section_name, grid_name)) in super::super::GRIDS.into_iter().enumerate() {
        let section = SECTION + i as u64 * 0x10000;
        let grid = GRID + i as u64 * 0x10000;
        window(f, section, section_name, &[grid]);
        f.word(grid, BASE + 0x34be068);
        name(f, grid, grid_name);
        f.array(grid + 0x190, if i == 0 { &[ITEM] } else { &[] });
    }
    f.word(ITEM, BASE + 0x343a9b0);
    f.word(ITEM + 0x20, TEMPLATE);
    f.word(ITEM + 0x10, FRAME);
    window(f, FRAME, "construction_choice", &[BUTTON]);
    name(f, BUTTON, "start_construction_button");
    let glue = ITEM + 0x30;
    for (field, value) in [
        (BUTTON, BASE + 0x34aafa0),
        (BUTTON + 0x70, BASE + 0x34ab360),
        (BUTTON + 0x78, NODE),
        (BUTTON + 0x80, NODE),
        (BUTTON + 0x90, 0),
        (BUTTON + 0xc0, 0),
        (BUTTON + 0x178, 0),
        (NODE, glue),
        (NODE + 8, 0),
        (NODE + 0x10, 0),
        (glue, BASE + 0x343ade8),
        (glue + 0x30, FUNCTION),
        (FUNCTION, BASE + 0x343ae78),
        (FUNCTION + 8, BASE + 0x1ed5640),
        (FUNCTION + 0x10, 0),
        (FUNCTION + 0x18, ITEM),
        (GUI + 0x98, GRAPHICS),
        (GRAPHICS, BASE + 0x33dafa8),
        (BUTTON + 0xd0, GRAPHICS),
        (BUTTON + 0xd8, SPRITE),
        (SPRITE, BASE + 0x34bdb10),
        (SPRITE + 8, SPRITE_TYPE),
        (SPRITE + 0x138, BUTTON),
        (BASE + 0x35dbf60, 0),
    ] {
        f.word(field, value);
    }
    f.int(BUTTON + 0x88, 1);
    f.put(BUTTON + 0x8c, &[0]);
    f.put(NODE + 0x18, &[0]);
    f.put(BUTTON + 0x65, &[8]);
    f.int(BUTTON + 0x9c, 0);
    f.put(BUTTON + 0xcc, &[8]);
    f.put(BUTTON + 0x1d1, &[0]);
    f.put(BASE + 0x3519990, &[0]);
    for index in 2..=20 {
        f.word(glue + index * 0x30, 0);
    }
    for (offset, value) in [(0x170, 1920), (0x174, 1080), (0x178, 1920), (0x17c, 1080)] {
        f.int(GRAPHICS + offset, value);
    }
    f.put(GRAPHICS + 0x2c5b0, &1_f32.to_le_bytes());
    f.int(SPRITE_TYPE + 0x78, 1);
    f.int(SPRITE_TYPE + 0x8c, 120);
    f.int(SPRITE_TYPE + 0x90, 40);
    f.put(SPRITE + 0xb0, &(-660_f32).to_le_bytes());
    f.put(SPRITE + 0xb4, &300_f32.to_le_bytes());
}

fn route(f: &mut Fixture, targets: &[u64]) {
    // Exact source may be first rather than tail: only hit order selects down.
    f.array(GRAPHICS + 0x50, &[SPRITE, FOREIGN]);
    let words: Vec<_> = targets
        .iter()
        .flat_map(|target| [BASE + 0x34b0ab8, *target])
        .collect();
    f.array(GRAPHICS + 0x28, &words);
    f.int(GRAPHICS + 0x30, targets.len() as i32);
    f.int(GRAPHICS + 0x34, targets.len() as i32);
    for field in [GUI + 0x120, GUI + 0x128] {
        f.word(field, BUTTON);
    }
    for field in [GUI + 0x130, GUI + 0x12] {
        f.put(field, &[0]);
    }
    f.int(BUTTON + 0x64, 0x2802); // Shown, mouse-eligible, hover notification.
    for (offset, x, y) in [
        (0x40, 360_f32, 220_f32),
        (0x48, 360.0, 220.0),
        (0x1c0, 1.0, 1.0),
        (0x1c8, 1.0, 1.0),
    ] {
        f.put(GRAPHICS + offset, &x.to_le_bytes());
        f.put(GRAPHICS + offset + 4, &y.to_le_bytes());
    }
}

#[test]
fn inactive_bind_and_readback_never_read_retained_default_or_old_ui() {
    let f = fixture(); // Every view, default-pointer and renderer byte is absent.
    let a = bind_selection(&f, "arms_factory", 1).unwrap();
    assert!(!a.initially_selected());
    let state = a.observe(&f).unwrap();
    a.same_scope_and_queue(&state).unwrap();
    assert!(!a.is_selected(&state));
    for field in [
        MANAGER + 0x3b8,
        DATABASE + 0x3a8,
        MANAGER + 0x150,
        VIEW,
        BUTTON,
        GUI + 0x98,
    ] {
        assert!(f.untouched(field, 8), "{field:x}");
    }
}

#[test]
fn initially_selected_target_needs_no_surviving_gui_control() {
    let mut f = fixture();
    selected(&mut f, TEMPLATE);
    let a = bind_selection(&f, "arms_factory", 1).unwrap();
    assert!(a.initially_selected());
    assert!(a.is_selected(&a.observe(&f).unwrap()));
    assert!(f.untouched(MANAGER + 0x150, 8));
}

#[test]
fn clearing_retains_nonempty_queue_and_ignores_stale_inactive_pointer() {
    let mut f = fixture();
    queue(&mut f);
    selected(&mut f, TEMPLATE);
    let a = bind_selection(&f, "arms_factory", 1)
        .unwrap()
        .clear_selection_anchor()
        .unwrap();
    assert!(!a.initially_inactive());
    assert_eq!(a.private_baseline().unwrap()["queue_member_count"], 1);
    assert_eq!(a.observe(&f).unwrap(), ClearSelectionReadback::Pending);
    f.int(MANAGER + 0x3c0, 0);
    f.word(MANAGER + 0x3b8, FOREIGN);
    assert_eq!(a.observe(&f).unwrap(), ClearSelectionReadback::Cleared);
    for field in [FOREIGN, MANAGER + 0x150, VIEW, BUTTON, GUI + 0x98] {
        assert!(f.untouched(field, 8), "{field:x}");
    }
}

#[test]
fn clearing_inactive_original_is_no_input_evidence_and_rejects_later_selection() {
    let mut f = fixture();
    let a = bind_selection(&f, "arms_factory", 1)
        .unwrap()
        .clear_selection_anchor()
        .unwrap();
    assert!(a.initially_inactive());
    assert_eq!(a.observe(&f).unwrap(), ClearSelectionReadback::Cleared);
    selected(&mut f, TEMPLATE);
    assert_eq!(
        a.observe(&f).unwrap_err().code,
        "construction_state_changed"
    );
}

#[test]
fn clearing_never_accepts_queue_delta_as_selection_success() {
    for remove_line in [false, true] {
        let mut f = fixture();
        queue(&mut f);
        selected(&mut f, TEMPLATE);
        let a = bind_selection(&f, "arms_factory", 1)
            .unwrap()
            .clear_selection_anchor()
            .unwrap();
        f.int(MANAGER + 0x3c0, 0);
        if remove_line {
            f.array(PRODUCTION + 0x70, &[]);
        } else {
            f.int(LINE + 0x48, 3);
        }
        assert_eq!(
            a.observe(&f).unwrap_err().code,
            "construction_state_changed"
        );
    }
}

#[test]
fn clearing_rejects_original_manager_replacement_before_its_payload() {
    let mut f = fixture();
    selected(&mut f, TEMPLATE);
    let a = bind_selection(&f, "arms_factory", 1)
        .unwrap()
        .clear_selection_anchor()
        .unwrap();
    f.word(IDLER + 0x670, FOREIGN);
    assert_eq!(
        a.observe(&f).unwrap_err().code,
        "construction_state_changed"
    );
    assert!(f.untouched(FOREIGN, 8));
}

#[test]
fn clearing_rejects_other_canonical_template_at_bind_and_after_original_bind() {
    for switch_after_bind in [false, true] {
        let mut f = fixture();
        template(&mut f, OTHER, 2, "industrial_complex");
        f.array(DATABASE + 0x40, &[NULL, TEMPLATE, OTHER]);
        selected(&mut f, if switch_after_bind { TEMPLATE } else { OTHER });
        let original = bind_selection(&f, "arms_factory", 1).unwrap();
        if switch_after_bind {
            let a = original.clear_selection_anchor().unwrap();
            selected(&mut f, OTHER);
            assert_eq!(
                a.observe(&f).unwrap_err().code,
                "construction_state_changed"
            );
        } else {
            assert_eq!(
                original.clear_selection_anchor().unwrap_err().code,
                "construction_state_changed"
            );
        }
    }
}

#[test]
fn clearing_keeps_original_campaign_and_monotonic_chronology() {
    for rewind in [false, true] {
        let mut f = fixture();
        selected(&mut f, TEMPLATE);
        let a = bind_selection(&f, "arms_factory", 1)
            .unwrap()
            .clear_selection_anchor()
            .unwrap();
        f.int(MANAGER + 0x3c0, 0);
        if rewind {
            f.set_date(DATE - 1);
        } else {
            f.int(GS + 0x4d8, 2);
        }
        assert!(a.observe(&f).is_err());
    }
}

#[test]
fn clearing_allows_forward_time_without_rebinding_the_original_queue() {
    let mut f = fixture();
    queue(&mut f);
    selected(&mut f, TEMPLATE);
    let a = bind_selection(&f, "arms_factory", 1)
        .unwrap()
        .clear_selection_anchor()
        .unwrap();
    let original = a.private_baseline().unwrap();
    f.set_date(DATE + 1);
    f.int(MANAGER + 0x3c0, 0);
    assert_eq!(a.observe(&f).unwrap(), ClearSelectionReadback::Cleared);
    assert_eq!(a.private_baseline().unwrap(), original);
}

#[test]
fn clearing_unknown_mode_never_follows_its_selected_pointer() {
    let mut f = fixture();
    selected(&mut f, TEMPLATE);
    let a = bind_selection(&f, "arms_factory", 1)
        .unwrap()
        .clear_selection_anchor()
        .unwrap();
    f.int(MANAGER + 0x3c0, 2);
    f.word(MANAGER + 0x3b8, FOREIGN);
    assert!(a.observe(&f).is_err());
    assert!(f.untouched(FOREIGN, 8));
}

#[test]
fn idle_to_exact_target_readback_ignores_obsolete_gui_callback_and_geometry() {
    let mut f = fixture();
    ui(&mut f);
    let before = observe_choice(&f, "arms_factory", 1, false).unwrap();
    let a = before.selection_anchor().unwrap();
    selected(&mut f, TEMPLATE);
    f.word(VIEW + 0x430, FOREIGN);
    f.word(BUTTON + 0xd8, FOREIGN);
    f.word(FUNCTION + 0x18, FOREIGN);
    let state = a.observe(&f).unwrap();
    a.same_scope_and_queue(&state).unwrap();
    assert!(a.is_selected(&state));
    assert!(f.untouched(VIEW, 0x500));
    assert!(f.untouched(BUTTON, 0x200));
    assert!(f.untouched(FUNCTION, 0x20));
    assert!(f.untouched(FOREIGN, 0x1000));
}

#[test]
fn canonical_other_selection_is_not_target_success() {
    let mut f = fixture();
    template(&mut f, OTHER, 2, "industrial_complex");
    f.array(DATABASE + 0x40, &[NULL, TEMPLATE, OTHER]);
    let a = bind_selection(&f, "arms_factory", 1).unwrap();
    selected(&mut f, OTHER);
    let now = a.observe(&f).unwrap();
    a.same_scope_and_queue(&now).unwrap();
    assert!(!a.is_selected(&now));
    assert!(!bind_selection(&f, "arms_factory", 1)
        .unwrap()
        .initially_selected());
}

#[test]
fn unsupported_and_special_modes_cannot_create_or_complete_anchors() {
    for mode in [2, 3, -1, i32::MAX] {
        let mut f = fixture();
        let a = bind_selection(&f, "arms_factory", 1).unwrap();
        f.int(MANAGER + 0x3c0, mode);
        assert_eq!(
            a.observe(&f).unwrap_err().code,
            "construction_selection_unavailable"
        );
        assert!(f.untouched(MANAGER + 0x3b8, 8));
        assert_eq!(
            bind_selection(&f, "arms_factory", 1).unwrap_err().code,
            "construction_selection_unavailable"
        );
    }
    for pointer in [NULL, FOREIGN] {
        let mut f = fixture();
        selected(&mut f, pointer);
        assert!(bind_selection(&f, "arms_factory", 1).is_err());
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}

#[test]
fn same_key_replacement_and_catalog_membership_change_refuse_before_new_payload() {
    let mut f = fixture();
    let a = bind_selection(&f, "arms_factory", 1).unwrap();
    // No byte at replacement is mapped; the list mismatch must be sufficient.
    f.array(DATABASE + 0x40, &[NULL, FOREIGN]);
    assert_eq!(
        a.observe(&f).unwrap_err().code,
        "construction_state_changed"
    );
    assert!(f.untouched(FOREIGN, 0x1000));
    let mut f = fixture();
    let a = bind_selection(&f, "arms_factory", 1).unwrap();
    f.string(TEMPLATE + 0x228, "industrial_complex");
    assert!(a.observe(&f).is_err());
}

#[test]
fn original_ownership_edges_refuse_before_any_foreign_payload() {
    let countries = fixture().get_word(GS + 0x2d8);
    let states = fixture().get_word(GS + 0x290);
    for field in [
        BASE + 0x3519740,
        BASE + 0x35196c0,
        BASE + 0x34d0de0,
        countries,
        COUNTRY + 0xd10,
        PRODUCTION + 0x290,
        IDLER + 0x4c0,
        IDLER + 0x670,
        MANAGER + 0x180,
        BASE + 0x34d0ac0,
        states + 8,
    ] {
        let mut f = fixture();
        let a = bind_selection(&f, "arms_factory", 1).unwrap();
        f.word(field, FOREIGN);
        assert!(a.observe(&f).is_err(), "{field:x}");
        assert!(f.untouched(FOREIGN, 0x1000), "{field:x}");
    }
    for field in [STATE + 0xc0, STATE + 0xc4] {
        let mut f = fixture();
        let a = bind_selection(&f, "arms_factory", 1).unwrap();
        f.int(field, 2);
        assert_eq!(
            a.observe(&f).unwrap_err().code,
            "construction_target_unavailable"
        );
        assert!(f.untouched(DATABASE + 0x40, 16));
        assert!(f.untouched(PRODUCTION + 0x70, 16));
    }
}

#[test]
fn campaign_setup_playthrough_player_and_date_are_not_adopted() {
    for (field, value) in [
        (GS + 0x4d8, 2),
        (GS + 0x5d0, 1),
        (GS + 0x5d8, 0),
        (GS + 0x428, DATE - 1),
    ] {
        let mut f = fixture();
        let a = bind_selection(&f, "arms_factory", 1).unwrap();
        f.int(field, value);
        assert_eq!(
            a.observe(&f).unwrap_err().code,
            "construction_state_changed"
        );
    }
    let mut f = fixture();
    let a = bind_selection(&f, "arms_factory", 1).unwrap();
    f.set_playthrough(
        "aabbccdd-1122-4344-8566-778899aabbcc"
            .replace("aabb", "ccdd")
            .as_str(),
    );
    assert_eq!(
        a.observe(&f).unwrap_err().code,
        "construction_state_changed"
    );
}

#[test]
fn nonregressed_clock_controls_and_normal_queue_progress_do_not_change_scope() {
    let mut f = fixture();
    queue(&mut f);
    let a = bind_selection(&f, "arms_factory", 1).unwrap();
    f.set_date(DATE + 1);
    f.int(GS + 0x47c, 3);
    f.put(IDLER + 0x679, &[0]);
    f.word(LINE + 0x38, 50);
    selected(&mut f, TEMPLATE);
    let now = a.observe(&f).unwrap();
    a.same_scope_and_queue(&now).unwrap();
    assert!(a.is_selected(&now));
    assert_eq!(now.context().date_raw, DATE + 1);
    assert_eq!(a.context().date_raw, DATE);
}

#[test]
fn complete_queue_identity_and_amount_changes_cannot_verify_selection() {
    let mut f = fixture();
    queue(&mut f);
    let a = bind_selection(&f, "arms_factory", 1).unwrap();
    f.int(LINE + 0x48, 3);
    selected(&mut f, TEMPLATE);
    let now = a.observe(&f).unwrap();
    assert_eq!(
        a.same_scope_and_queue(&now).unwrap_err().code,
        "construction_state_changed"
    );
    assert!(!a.is_selected(&now));
    let mut f = fixture();
    let a = bind_selection(&f, "arms_factory", 1).unwrap();
    f.array(PRODUCTION + 0x70, &[FOREIGN]);
    assert_eq!(
        a.observe(&f).unwrap_err().code,
        "construction_state_changed"
    );
    assert!(f.untouched(FOREIGN, 0x100));
}

#[test]
fn full_queue_order_is_original_evidence_before_any_replacement_line_payload() {
    let mut f = fixture();
    queue(&mut f);
    let second = LINE + 0x1000;
    for offset in [0, 0x20, 0x28, 0x30, 0x38, 0x70] {
        f.word(second + offset, f.get_word(LINE + offset));
    }
    f.word(second + 8, 0x20000003b);
    for (offset, value) in [
        (0x14, 1),
        (0x18, 0),
        (0x40, 1),
        (0x44, 0),
        (0x48, 2),
        (0x78, 0),
    ] {
        f.int(second + offset, value);
    }
    f.put(second + 0x8c, &[0]);
    f.put(second + 0x99, &[0]);
    f.array(PRODUCTION + 0x70, &[LINE, second]);
    let a = bind_selection(&f, "arms_factory", 1).unwrap();
    f.array(PRODUCTION + 0x70, &[second, LINE]);
    assert_eq!(
        a.observe(&f).unwrap_err().code,
        "construction_state_changed"
    );
    assert!(f.untouched(LINE, 0xa0));
    assert!(f.untouched(second, 0xa0));
}

#[test]
fn initial_binding_requires_full_unique_catalog_keys_and_exact_ordinals() {
    let mut f = fixture();
    template(&mut f, OTHER, 2, "arms_factory");
    f.array(DATABASE + 0x40, &[NULL, TEMPLATE, OTHER]);
    assert_eq!(
        bind_selection(&f, "arms_factory", 1).unwrap_err().code,
        "invalid_construction"
    );
    let mut f = fixture();
    f.int(TEMPLATE + 0x290, 2);
    assert_eq!(
        bind_selection(&f, "arms_factory", 1).unwrap_err().code,
        "invalid_construction"
    );
}

#[test]
fn unsupported_anomalous_or_unlocated_other_rows_are_never_an_empty_queue() {
    for kind in [0, 1, 2, 3] {
        let mut f = fixture();
        queue(&mut f);
        match kind {
            0 => f.word(LINE, BASE + 0x1000), // No main-image type metadata/layout.
            1 => f.word(LINE, BASE + 0x338a7a8), // Railway location unproved.
            2 => f.word(LINE + 0x28, u64::MAX),
            3 => {
                template(&mut f, OTHER, 2, "industrial_complex");
                f.array(DATABASE + 0x40, &[NULL, TEMPLATE, OTHER]);
                f.word(BUILDING + 0x1b0, OTHER);
                f.word(BUILDING + 0x1a8, 0); // Unrelated key's missing location.
            }
            _ => unreachable!(),
        }
        assert_eq!(
            bind_selection(&f, "arms_factory", 1).unwrap_err().code,
            "construction_queue_unavailable"
        );
    }
}

#[test]
fn each_canonical_field_is_rechecked_in_the_same_readback_witness() {
    for (field, size) in [
        (GS + 0x428, 4),
        (STATE + 0xc4, 4),
        (DATABASE + 0x40, 16),
        (TEMPLATE + 0x228, 24),
        (IDLER + 0x670, 8),
        (GUI, 8),
        (COUNTRY + 0xd10, 8),
        (PRODUCTION + 0x70, 16),
        (LINE + 0x48, 4),
        (MANAGER + 0x3c0, 4),
        (MANAGER + 0x3b8, 8),
        (DATABASE + 0x3a8, 8),
    ] {
        let mut f = fixture();
        queue(&mut f);
        let a = bind_selection(&f, "arms_factory", 1).unwrap();
        selected(&mut f, TEMPLATE);
        a.observe(&f).unwrap();
        let last = f.read_counts()[&(field, size)];
        f.tear(field, size, last);
        assert_eq!(a.observe(&f).unwrap_err().code, "torn_read", "{field:x}");
    }
}

#[test]
fn exact_identity_is_checked_before_payload_and_after_final_witness() {
    let mut f = fixture();
    let a = bind_selection(&f, "arms_factory", 1).unwrap();
    f.id.start_ticks.push('9');
    assert_eq!(a.observe(&f).unwrap_err().code, "ownership_changed");
    assert!(f.untouched(GS, 0x1000));
    let mut f = fixture();
    let a = bind_selection(&f, "arms_factory", 1).unwrap();
    f.swap_final_identity = true;
    assert_eq!(a.observe(&f).unwrap_err().code, "ownership_changed");
    let mut f = fixture();
    let a = bind_selection(&f, "arms_factory", 1).unwrap();
    f.fail_final = true;
    assert_eq!(a.observe(&f).unwrap_err().code, "ownership_changed");
}

#[test]
fn lightweight_initial_anchor_and_fresh_control_must_keep_exact_selection() {
    let mut f = fixture();
    let initial = bind_selection(&f, "arms_factory", 1).unwrap();
    ui(&mut f);
    let first = observe_choice(&f, "arms_factory", 1, false).unwrap();
    initial
        .require_same_initial(&first.selection_anchor().unwrap())
        .unwrap();
    // Hover changes are allowed by the canonical-only comparison.
    f.int(BUTTON + 0x9c, 2);
    let hovered = observe_choice(&f, "arms_factory", 1, false).unwrap();
    first.selection_unchanged(&hovered).unwrap();
    selected(&mut f, TEMPLATE);
    let changed = observe_choice(&f, "arms_factory", 1, false).unwrap();
    assert_eq!(
        first.selection_unchanged(&changed).unwrap_err().code,
        "construction_state_changed"
    );
    assert!(initial
        .require_same_initial(&changed.selection_anchor().unwrap())
        .is_err());
}

#[test]
fn original_full_observation_refuses_incomplete_queue_and_unsupported_selection() {
    let mut f = fixture();
    ui(&mut f);
    f.int(MANAGER + 0x3c0, 2);
    let o = super::super::observe(&f, "arms_factory", 1, Options::default()).unwrap();
    assert_eq!(
        o.selection_anchor().unwrap_err().code,
        "construction_selection_unavailable"
    );
    f.int(MANAGER + 0x3c0, 0);
    queue(&mut f);
    f.word(LINE + 0x28, u64::MAX);
    let o = observe_choice(&f, "arms_factory", 1, false).unwrap();
    assert_eq!(
        o.selection_anchor().unwrap_err().code,
        "construction_queue_unavailable"
    );
}

#[test]
fn private_baseline_fingerprints_complete_vectors_with_bounded_output() {
    let mut f = fixture();
    let mut members = vec![NULL, TEMPLATE];
    for ordinal in 2..=100 {
        let pointer = OTHER + ordinal as u64 * 0x400;
        template(&mut f, pointer, ordinal, &format!("building_{ordinal}"));
        members.push(pointer);
    }
    f.array(DATABASE + 0x40, &members);
    queue(&mut f);
    let a = bind_selection(&f, "arms_factory", 1).unwrap();
    let baseline = a.private_baseline().unwrap();
    assert_eq!(baseline["catalog_member_count"], 101);
    assert_eq!(baseline["queue_member_count"], 1);
    assert_eq!(baseline["scope"]["target"]["pointer"], TEMPLATE);
    assert_eq!(baseline["binding_sha256"].as_str().unwrap().len(), 64);
    let length = serde_json::to_vec(&baseline).unwrap().len();
    assert!(length < 8192);
    assert!(baseline["binding_bytes"].as_u64().unwrap() > length as u64);
    f.word(LINE + 0x38, 100); // Progress is not an allocation/identity change.
    assert_eq!(
        bind_selection(&f, "arms_factory", 1)
            .unwrap()
            .private_baseline()
            .unwrap(),
        baseline
    );
    f.int(LINE + 0x48, 3);
    assert_ne!(
        bind_selection(&f, "arms_factory", 1)
            .unwrap()
            .private_baseline()
            .unwrap()["binding_sha256"],
        baseline["binding_sha256"]
    );
}

#[test]
fn public_rendering_still_exposes_no_original_pointer_or_private_fingerprint() {
    let mut f = fixture();
    ui(&mut f);
    let value = super::super::snapshot_with_options(
        &f,
        "arms_factory",
        1,
        Options {
            callback: true,
            geometry: true,
            routing: false,
        },
    )
    .unwrap();
    assert!(!value.to_string().contains("binding_sha256"));
    for pointer in [
        COUNTRY, PRODUCTION, DATABASE, TEMPLATE, STATE, GUI, MANAGER, ITEM, FRAME, BUTTON,
    ] {
        assert!(
            !value.to_string().contains(&pointer.to_string()),
            "{pointer:x}"
        );
    }
}

#[test]
fn typed_control_point_and_hover_route_preserve_original_identity_without_source_tail() {
    let mut f = fixture();
    ui(&mut f);
    let before = observe_choice(&f, "arms_factory", 1, false).unwrap();
    assert_eq!(before.selection_control_point().unwrap(), (360.0, 220.0));
    route(&mut f, &[BUTTON, FOREIGN]);
    f.int(BUTTON + 0x9c, 2);
    let after = observe_choice(&f, "arms_factory", 1, true).unwrap();
    before.require_same_choice_geometry(&after, true).unwrap();
    assert!(f.untouched(FOREIGN, 0x1000));
}

#[test]
fn typed_control_comparison_refuses_new_rect_callback_or_same_template_item() {
    for kind in 0..3 {
        let mut f = fixture();
        ui(&mut f);
        let before = observe_choice(&f, "arms_factory", 1, false).unwrap();
        match kind {
            0 => f.put(SPRITE + 0xb0, &(-659_f32).to_le_bytes()),
            1 => {
                let node = NODE + 0x1000;
                f.word(BUTTON + 0x78, node);
                f.word(BUTTON + 0x80, node);
                f.word(node, ITEM + 0x30);
                f.word(node + 8, 0);
                f.word(node + 0x10, 0);
                f.put(node + 0x18, &[0]);
            }
            2 => {
                let item = ITEM + 0x1000;
                f.array(GRID + 0x190, &[item]);
                f.word(item, BASE + 0x343a9b0);
                f.word(item + 0x10, FRAME);
                f.word(item + 0x20, TEMPLATE);
                f.word(item + 0x30, BASE + 0x343ade8);
                f.word(item + 0x60, FUNCTION);
                f.word(NODE, item + 0x30);
                f.word(FUNCTION + 0x18, item);
                for index in 2..=20 {
                    f.word(item + 0x30 + index * 0x30, 0);
                }
            }
            _ => unreachable!(),
        }
        let after = observe_choice(&f, "arms_factory", 1, false).unwrap();
        assert_eq!(
            before
                .require_same_choice_geometry(&after, false)
                .unwrap_err()
                .code,
            "construction_choice_changed"
        );
    }
}

#[test]
fn typed_before_down_comparison_refuses_mismatched_or_duplicate_route_targets() {
    for targets in [vec![FOREIGN, BUTTON], vec![BUTTON, BUTTON]] {
        let mut f = fixture();
        ui(&mut f);
        let before = observe_choice(&f, "arms_factory", 1, false).unwrap();
        route(&mut f, &targets);
        let after = observe_choice(&f, "arms_factory", 1, true).unwrap();
        assert_eq!(
            before
                .require_same_choice_geometry(&after, true)
                .unwrap_err()
                .code,
            "construction_route_unavailable"
        );
        assert!(f.untouched(FOREIGN, 0x1000));
    }
    let mut f = fixture();
    ui(&mut f);
    let before = observe_choice(&f, "arms_factory", 1, false).unwrap();
    assert_eq!(
        before
            .require_same_choice_geometry(&before, true)
            .unwrap_err()
            .code,
        "construction_choice_unavailable"
    );
}

#[test]
fn unsupported_abi_refuses_initial_binding_before_any_snapshot_or_payload() {
    let f = Fixture::new(StringAbi::LibStdCxx);
    assert_eq!(
        bind_selection(&f, "arms_factory", 1).unwrap_err().code,
        "unsupported_platform"
    );
    assert_eq!(f.snapshots.get(), 0);
}

#[test]
fn navigation_requires_initially_inactive_selection() {
    let mut f = fixture();
    let anchor = bind_selection(&f, "arms_factory", 1).unwrap();
    anchor.require_inactive().unwrap();
    selected(&mut f, TEMPLATE);
    assert_eq!(
        bind_selection(&f, "arms_factory", 1)
            .unwrap()
            .require_inactive()
            .unwrap_err()
            .code,
        "construction_selection_unavailable"
    );
}

#[test]
fn navigation_refuses_selection_changes_even_with_the_same_original_queue() {
    let mut f = fixture();
    let anchor = bind_selection(&f, "arms_factory", 1).unwrap();
    selected(&mut f, TEMPLATE);
    let now = anchor.observe(&f).unwrap();
    anchor.same_scope_and_queue(&now).unwrap();
    assert_eq!(
        anchor.require_unchanged(&now).unwrap_err().code,
        "construction_state_changed"
    );
}

#[test]
fn navigation_allows_unchanged_inactive_selection_without_any_panel_reads() {
    let f = fixture();
    let anchor = bind_selection(&f, "arms_factory", 1).unwrap();
    anchor
        .require_unchanged(&anchor.observe(&f).unwrap())
        .unwrap();
    assert!(f.untouched(MANAGER + 0x150, 8));
    assert!(f.untouched(MANAGER + 0x190, 8));
}
