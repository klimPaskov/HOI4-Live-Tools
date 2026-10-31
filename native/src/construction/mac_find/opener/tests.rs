//! Sparse Mac layout fixtures; no OS API, input, process attachment or clock.
use super::*;
use crate::{
    construction::mac_ui::bind_selection,
    memory::StringAbi,
    own_focus::assignment_fixture::{Fixture, BASE, COUNTRY, DATE, FOREIGN, GS, IDLER},
};

const PRODUCTION: u64 = 0x900000;
const REGISTRY: u64 = 0x910000;
const MANAGER: u64 = 0x920000;
const FIND: u64 = 0x930000;
const WINDOW: u64 = 0x940000;
const ROW: u64 = 0x950000;
const ENTRIES: u64 = 0x960000;
const CAMERA: u64 = 0x970000;
const STATE: u64 = 0x980000;
const TEMPLATE: u64 = 0x990000;
const EDIT: u64 = 0x9a0000;
const KIND: u64 = 0x9b0000;

fn floats(f: &mut Fixture, address: u64, values: &[f32]) {
    for (index, value) in values.iter().enumerate() {
        f.put(address + index as u64 * 4, &value.to_le_bytes());
    }
}

fn find_fixture() -> Fixture {
    let mut f = Fixture::new(StringAbi::LibCxx);
    for (at, value) in [
        (COUNTRY + 0xd10, PRODUCTION),
        (PRODUCTION, BASE + 0x358e0c8),
        (PRODUCTION + 0x2a8, COUNTRY),
        (IDLER + 0x4c0, REGISTRY),
        (REGISTRY, BASE + 0x36a5810),
        (IDLER + 0x670, MANAGER),
        (MANAGER + 0x180, IDLER),
        (MANAGER + 0x190, FIND),
        (FIND, BASE + 0x3665308),
        (FIND + 0xd50, IDLER),
        (FIND + 0xd58, WINDOW),
        (WINDOW, BASE + 0x36ba978),
        (WINDOW + 0x88, REGISTRY),
        (FIND + 0xda8, ROW),
        (ROW, BASE + 0x36657b0),
        (ROW + 0x458, STATE),
        (IDLER + 0x718, CAMERA),
        (STATE, BASE + 0x357f410),
        (STATE + 0x20, TEMPLATE),
        (TEMPLATE, BASE + 0x3580798),
        (BASE + 0x37eda08, REGISTRY),
        (FIND + 0xd60, EDIT),
        (EDIT, BASE + 0x36bc6a0),
        (EDIT + 0x180, BASE + 0x36bc9e0),
        (EDIT + 0x58, REGISTRY),
        (EDIT + 0x50, KIND),
        (KIND, BASE + 0x36bca58),
        (REGISTRY + 0x138, EDIT),
        (REGISTRY + 0x210, EDIT + 0x180),
    ] {
        f.word(at, value);
    }
    f.put(WINDOW + 0x95, &[8]);
    f.int(WINDOW + 0x13d8, 0);
    f.int(WINDOW + 0x1390, 0);
    f.string(FIND + 0xd68, "");
    f.word(FIND + 0xdb0, ENTRIES);
    f.int(FIND + 0xdb8, 1);
    f.int(FIND + 0xdbc, 0);
    f.put(ENTRIES, &[0; 0x40]);
    f.word(ENTRIES + 0x30, STATE);
    f.int(ENTRIES + 0x38, 2);
    f.int(ROW + 0x468, 2);
    let mut states = vec![0; 65];
    states[64] = STATE;
    f.array(GS + 0x290, &states);
    f.int(STATE + 0x40, 64);
    f.int(STATE + 0xc0, 1);
    f.int(STATE + 0xc4, 1);
    f.int(TEMPLATE + 0x120, 3120);
    f.int(TEMPLATE + 0x124, 1480);
    floats(
        &mut f,
        CAMERA + 0x698,
        &[100.0, 300.0, 200.0, 100.0, 9.5, 200.0],
    );
    // Actual passed primitive-gate raw values, not inferred eligibility flags.
    f.put(EDIT + 0x64, &0x20805u32.to_le_bytes());
    f.put(EDIT + 0x239, &[0x91]);
    f.string(KIND + 0x28, "to_find_text");
    f.array(WINDOW + 0xe50, &[EDIT]);
    f
}

const RESOURCE: u64 = 0xd30000;
const HOTKEYS: u64 = 0xd50000;
const DATABASE: u64 = 0xa00000;
const BUILDING_TEMPLATE: u64 = 0xa10000;
const NULL_TEMPLATE: u64 = 0xa20000;
const MAP: u64 = 0xb00000;
const MAP_ROOT: u64 = 0xb10000;
const BUTTON: u64 = 0xb20000;
const DESC: u64 = 0xb30000;
const SPRITE: u64 = 0xb40000;
const POSITION: u64 = 0xb50000;
const FUNCTION: u64 = 0xb60000;
const KEYS: u64 = 0xb70000;
const NODE: u64 = 0xb80000;
const GFX: u64 = 0xc00000;
const APP: u64 = 0xc10000;
const PUMP: u64 = 0xc20000;
const MENU: u64 = 0xc30000;
const SETTINGS: u64 = 0xc40000;
const LOBBY: u64 = 0xc50000;
fn fixture() -> Fixture {
    let mut f = find_fixture();
    for (at, v) in [
        (BASE + 0x3703ad0, DATABASE),
        (DATABASE, BASE + 0x352eea8),
        (NULL_TEMPLATE, BASE + 0x352f078),
        (BUILDING_TEMPLATE, BASE + 0x352fa88),
        (MANAGER + 0x228, MAP),
        (MAP, BASE + 0x36743d8),
        (MAP + 0x30, MANAGER),
        (MAP + 0x2608, MANAGER),
        (MAP + 0x2610, IDLER),
        (MAP + 0x2730, MAP_ROOT),
        (MAP_ROOT, BASE + 0x36ba978),
        (MAP_ROOT + 0x88, REGISTRY),
        (MAP_ROOT + 0x1358, POSITION),
        (BUTTON, BASE + 0x36b9738),
        (BUTTON + 0x50, DESC),
        (BUTTON + 0x58, REGISTRY),
        (DESC, BASE + 0x36b9f98),
        (BUTTON + 0x70, BASE + 0x36b9b18),
        (BUTTON + 0x78, NODE),
        (BUTTON + 0x80, NODE),
        (NODE, MAP + 0x1530),
        (NODE + 8, 0),
        (NODE + 0x10, 0),
        (BUTTON + 0x90, 0),
        (BUTTON + 0xc8, 0),
        (MAP + 0x1530, BASE + 0x36749f0),
        (MAP + 0x1560, FUNCTION),
        (FUNCTION, BASE + 0x3674a88),
        (FUNCTION + 8, BASE + 0x22b03c0),
        (FUNCTION + 0x10, 0),
        (FUNCTION + 0x18, MAP),
        (BUTTON + 0xd8, GFX),
        (BUTTON + 0xe0, SPRITE),
        (SPRITE, BASE + 0x36cc528),
        (SPRITE + 0x138, BUTTON),
        (SPRITE + 0x108, POSITION),
        (POSITION, BASE + 0x36bea88),
        (BUTTON + 0x180, POSITION),
        (REGISTRY + 0x98, GFX),
        (GFX, BASE + 0x35e5b58),
        (IDLER + 0x4f8, APP),
        (APP + 0x300, GFX),
        (APP + 0x308, REGISTRY),
        (APP + 0x38, IDLER),
        (BASE + 0x37ea778, 0),
        (BASE + 0x37ea608, PUMP),
        (PUMP, BASE + 0x36b5160),
        (BASE + 0x35089d8 + 0x20, BASE + 0x7bb2c0),
        (MANAGER + 0x218, MENU),
        (MENU, BASE + 0x366ebf8),
        (MENU + 0x2a28, IDLER),
        (MENU + 0x2a30, MANAGER),
        (MENU + 0x2a38, 0),
        (MENU + 0x2a50, 0),
        (IDLER + 0x778, SETTINGS),
        (SETTINGS, BASE + 0x3669110),
        (SETTINGS + 0x28, IDLER),
        (SETTINGS + 0x7e0, 0),
        (MANAGER + 0x248, LOBBY),
        (LOBBY, BASE + 0x367da48),
        (LOBBY + 0x48, IDLER),
        (LOBBY + 0x38, 0),
        (LOBBY + 0x40, 0),
    ] {
        f.word(at, v);
    }
    f.put(NULL_TEMPLATE + 0x18, &[0]);
    f.put(BUILDING_TEMPLATE + 0x18, &[1]);
    f.int(BUILDING_TEMPLATE + 0x290, 1);
    f.string(BUILDING_TEMPLATE + 0x228, "arms_factory");
    f.array(DATABASE + 0x40, &[NULL_TEMPLATE, BUILDING_TEMPLATE]);
    f.word(DATABASE + 0x3a8, NULL_TEMPLATE);
    f.array(PRODUCTION + 0x70, &[]);
    f.int(MANAGER + 0x3c0, 0);
    f.put(WINDOW + 0x95, &[0]);
    f.word(REGISTRY + 0x138, 0);
    f.word(REGISTRY + 0x210, 0);
    f.put(GS + 0x9b9, &[1]);
    f.put(IDLER + 0x8c8, &[0]);
    f.put(IDLER + 0x7e1, &[1]);
    f.put(BASE + 0x372b2b8, &[0]);
    f.put(PUMP + 0x44, &[0]);
    f.int(BASE + 0x384a928, 0x66);
    f.put(BASE + 0x384a70b, &[0]);
    f.put(BASE + 0x384a500, &[0, 0]);
    f.put(REGISTRY + 0x10, &[1, 0, 0]);
    f.put(REGISTRY + 0x130, &[0]);
    f.array(REGISTRY + 0xa70, &[]);
    f.array(REGISTRY + 0xa88, &[]);
    f.put(MAP_ROOT + 0x95, &[8]);
    f.int(MAP_ROOT + 0x13d8, 0);
    f.int(MAP_ROOT + 0x1390, 0);
    f.array(MAP_ROOT + 0xe50, &[BUTTON]);
    f.string(DESC + 0x28, "open_find_view_button");
    f.int(BUTTON + 0x88, 1);
    f.put(BUTTON + 0x8c, &[0]);
    f.put(NODE + 0x18, &[0]);
    for i in 2..=20 {
        f.word(MAP + 0x1530 + i * 0x30, 0);
    }
    f.int(BUTTON + 0x64, 0x804);
    f.int(BUTTON + 0x9c, 0);
    f.put(BUTTON + 0xd4, &[8]);
    f.put(BUTTON + 0x1d9, &[0]);
    f.put(POSITION + 0x128, &[1]);
    f.array(GFX + 0x68, &[SPRITE]);
    f.word(BUTTON + 0xa8, KEYS);
    f.int(BUTTON + 0xb0, 1);
    f.int(BUTTON + 0xb4, 1);
    f.put(KEYS, &[0; 40]);
    f.int(KEYS, 0x66);
    f.word(BASE + 0x372afc0, IDLER);
    f.word(BASE + 0x37ed910, 0);
    f.int(BUTTON + 0x64, 0x20805);
    f.word(SPRITE + 8, RESOURCE);
    f.word(RESOURCE, BASE + 0x36cc880);
    f.int(RESOURCE + 0x78, 1);
    f.int(RESOURCE + 0x8c, 100);
    f.int(RESOURCE + 0x90, 40);
    floats(&mut f, SPRITE + 0xb0, &[0.0, 0.0]);
    for (i, v) in [1920, 1080, 1920, 1080].iter().enumerate() {
        f.int(GFX + 0x170 + i as u64 * 4, *v);
    }
    floats(&mut f, GFX + 0x2c5b0, &[1.0]);
    set_route(&mut f);
    f
}
fn selected_anchor(f: &Fixture) -> SelectionAnchor {
    bind_selection(f, "arms_factory", 64).unwrap()
}
fn open(f: &mut Fixture) {
    f.put(WINDOW + 0x95, &[8]);
    f.word(REGISTRY + 0x138, EDIT);
    f.word(REGISTRY + 0x210, EDIT + 0x180);
}
#[test]
fn exact_hidden_callback_and_click_target_admit_only_the_fixed_target() {
    let f = fixture();
    let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
    assert!(!a.initially_ready());
    a.require_before_move(&f).unwrap();
    assert!(a.observe_ready(&f).unwrap().is_none());
    assert!(f.untouched(FIND + 0xd60, 8));
    assert!(f.untouched(FIND + 0xd68, 24));
    assert!(
        serde_json::to_vec(&a.private_baseline().unwrap())
            .unwrap()
            .len()
            < 16384
    );
}
#[test]
fn visible_ready_is_zero_click_and_does_not_read_opener_payload() {
    let mut f = fixture();
    open(&mut f);
    let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
    assert!(a.initially_ready());
    assert!(f.untouched(MANAGER + 0x228, 8));
    assert!(a.require_before_move(&f).is_err());
    let p = a.observe_ready(&f).unwrap().unwrap();
    p.reobserve(&f, &a).unwrap();
}
#[test]
fn visible_nonempty_or_wrong_focus_is_not_a_click_retry() {
    for which in 0..2 {
        let mut f = fixture();
        open(&mut f);
        if which == 0 {
            f.string(FIND + 0xd68, "x")
        } else {
            f.word(REGISTRY + 0x210, FOREIGN)
        }
        assert!(OpenAnchor::bind(&f, &selected_anchor(&f), 64).is_err());
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}
#[test]
fn callback_null_root_is_explicit_no_input_unsupported() {
    let mut f = fixture();
    f.word(FIND + 0xd58, 0);
    assert!(OpenAnchor::bind(&f, &selected_anchor(&f), 64).is_err());
    assert!(f.untouched(MANAGER + 0x228, 8));
}
#[test]
fn one_reached_root_then_final_replacement_is_not_adopted() {
    let mut f = fixture();
    let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
    open(&mut f);
    let p = a.observe_ready(&f).unwrap().unwrap();
    f.word(FIND + 0xd58, FOREIGN);
    assert!(p.reobserve(&f, &a).is_err());
    assert!(f.untouched(FOREIGN, 0x1000));
}
#[test]
fn first_reached_own_root_may_materialize_after_the_click() {
    let mut f = fixture();
    let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
    const NEW: u64 = 0xd00000;
    f.word(NEW, BASE + 0x36ba978);
    f.word(NEW + 0x88, REGISTRY);
    f.put(NEW + 0x95, &[8]);
    f.int(NEW + 0x13d8, 0);
    f.int(NEW + 0x1390, 0);
    f.array(NEW + 0xe50, &[EDIT]);
    f.word(FIND + 0xd58, NEW);
    f.word(REGISTRY + 0x138, EDIT);
    f.word(REGISTRY + 0x210, EDIT + 0x180);
    let p = a.observe_ready(&f).unwrap().unwrap();
    p.reobserve(&f, &a).unwrap();
}
#[test]
fn owned_edges_replaced_before_context_or_foreign_ui_payload_refuse() {
    for at in [
        IDLER + 0x670,
        MANAGER + 0x228,
        MAP + 0x2730,
        MAP + 0x2608,
        FUNCTION + 0x18,
    ] {
        let mut f = fixture();
        let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
        f.word(at, FOREIGN);
        assert!(a.require_before_move(&f).is_err(), "{at:x}");
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}
#[test]
fn changed_own_country_or_catalog_is_not_followed() {
    for at in [COUNTRY + 0xd10, BASE + 0x3703ad0] {
        let mut f = fixture();
        let s = selected_anchor(&f);
        f.word(at, FOREIGN);
        assert!(OpenAnchor::bind(&f, &s, 64).is_err());
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}
#[test]
fn replaced_button_membership_refuses_before_descriptor_payload() {
    let mut f = fixture();
    let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
    f.array(MAP_ROOT + 0xe50, &[FOREIGN]);
    assert!(a.require_before_move(&f).is_err());
    assert!(f.untouched(FOREIGN, 0x1000));
}
#[test]
fn supported_receiver_type_is_not_guessed_or_skipped() {
    for at in [SPRITE, BUTTON, MAP, SETTINGS, LOBBY] {
        let mut f = fixture();
        f.word(at, BASE + 0x123456);
        assert!(
            OpenAnchor::bind(&f, &selected_anchor(&f), 64).is_err(),
            "{at:x}"
        );
    }
}
#[test]
fn exact_blocker_owners_and_roots_are_checked_without_foreign_follow() {
    for at in [SETTINGS + 0x28, LOBBY + 0x48, MENU + 0x2a28] {
        let mut f = fixture();
        f.word(at, FOREIGN);
        assert!(OpenAnchor::bind(&f, &selected_anchor(&f), 64).is_err());
        assert!(f.untouched(FOREIGN, 0x1000));
    }
    for at in [GS + 0x9b9, IDLER + 0x8c8] {
        let mut f = fixture();
        f.put(at, &[if at == GS + 0x9b9 { 0 } else { 1 }]);
        assert!(OpenAnchor::bind(&f, &selected_anchor(&f), 64).is_err());
    }
}
#[test]
fn nonempty_queue_membership_or_active_selection_never_adopt() {
    let mut f = fixture();
    let s = selected_anchor(&f);
    f.array(PRODUCTION + 0x70, &[FOREIGN]);
    assert!(OpenAnchor::bind(&f, &s, 64).is_err());
    assert!(f.untouched(FOREIGN, 0x1000));
    let mut f = fixture();
    f.int(MANAGER + 0x3c0, 1);
    f.word(MANAGER + 0x3b8, BUILDING_TEMPLATE);
    let s = bind_selection(&f, "arms_factory", 64).unwrap();
    assert!(OpenAnchor::bind(&f, &s, 64).is_err());
}
#[test]
fn final_witness_tear_identity_loss_and_chronology_refuse() {
    let mut f = fixture();
    let s = selected_anchor(&f);
    f.tear(MAP + 0x2608, 8, 2);
    assert_eq!(OpenAnchor::bind(&f, &s, 64).unwrap_err().code, "torn_read");
    for which in 0..3 {
        let mut f = fixture();
        let s = selected_anchor(&f);
        if which == 0 {
            f.fail_final = true
        } else if which == 1 {
            f.swap_final_identity = true
        } else {
            f.int(GS + 0x428, DATE - 1)
        }
        assert!(OpenAnchor::bind(&f, &s, 64).is_err(), "case {which}");
    }
}
#[test]
fn replaced_array_renderer_or_parent_edges_refuse_before_new_payload() {
    for at in [
        MAP_ROOT + 0xe50,
        BUTTON + 0xe0,
        BUTTON + 0x180,
        MAP_ROOT + 0x1358,
    ] {
        let mut f = fixture();
        let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
        f.word(at, FOREIGN);
        assert!(a.require_before_move(&f).is_err(), "{at:x}");
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}
#[test]
fn reached_query_focus_or_named_edit_change_refuses_final_read() {
    for which in 0..3 {
        let mut f = fixture();
        let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
        open(&mut f);
        let p = a.observe_ready(&f).unwrap().unwrap();
        match which {
            0 => f.string(FIND + 0xd68, "64"),
            1 => f.word(REGISTRY + 0x210, FOREIGN),
            _ => f.word(FIND + 0xd60, FOREIGN),
        };
        assert!(p.reobserve(&f, &a).is_err());
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}
#[test]
fn wrong_identity_or_abi_refuses_before_any_fresh_memory_payload() {
    for abi in [false, true] {
        let mut f = fixture();
        let s = selected_anchor(&f);
        if abi {
            f.abi = StringAbi::LibStdCxx
        } else {
            f.id.start_ticks.push('9')
        };
        f.begin_snapshot().unwrap();
        assert!(OpenAnchor::bind(&f, &s, 64).is_err());
        assert!(f.read_counts().is_empty());
    }
}
#[test]
fn complete_collection_and_single_observer_bounds_precede_their_payload() {
    for which in 0..2 {
        let mut f = fixture();
        match which {
            0 => {
                f.word(BUTTON + 0x78, FOREIGN);
                f.int(BUTTON + 0x88, 2)
            }
            _ => {
                f.word(MAP_ROOT + 0xe50, FOREIGN);
                f.int(MAP_ROOT + 0xe58, 5000);
                f.int(MAP_ROOT + 0xe5c, 5000)
            }
        }
        assert!(OpenAnchor::bind(&f, &selected_anchor(&f), 64).is_err());
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}

#[test]
fn unknown_lifetime_collections_refuse_before_optional_payload() {
    for at in [REGISTRY + 0xa70, REGISTRY + 0xa88] {
        let mut f = fixture();
        let s = selected_anchor(&f);
        f.array(at, &[FOREIGN]);
        assert!(OpenAnchor::bind(&f, &s, 64).is_err());
        assert!(f.untouched(FOREIGN + 8, 0x1000));
    }
}

fn set_route(f: &mut Fixture) {
    // Mouse dispatch requires bit0x2000 in addition to keyboard-ready flags.
    f.int(BUTTON + 0x64, 0x22805);
    f.array(GFX + 0x50, &[SPRITE]);
    f.array(GFX + 0x28, &[BASE + 0x36bf490, BUTTON]);
    f.int(GFX + 0x30, 1);
    f.int(GFX + 0x34, 1);
    floats(f, GFX + 0x40, &[1010.0, 520.0, 1010.0, 520.0]);
    floats(f, GFX + 0x1c0, &[1.0, 1.0, 1.0, 1.0]);
    f.word(REGISTRY + 0x120, BUTTON);
    f.word(REGISTRY + 0x128, BUTTON);
}

#[test]
fn mouse_control_reads_no_printable_key_mapping_or_keyboard_prefix() {
    let mut f = fixture();
    f.int(BASE + 0x384a928, 0);
    f.array(GFX + 0x68, &[FOREIGN]);
    let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
    a.require_before_down(&f).unwrap();
    assert!(f.untouched(BASE + 0x384a928, 4));
    assert!(f.untouched(GFX + 0x68, 16));
    assert!(f.untouched(FOREIGN, 0x1000));
    assert_eq!(a.control_point().unwrap(), (1010.0, 520.0));
}
#[test]
fn current_mouse_route_is_fresh_while_original_target_is_immutable() {
    let mut f = fixture();
    let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
    f.array(GFX + 0x50, &[FOREIGN, SPRITE]);
    f.array(
        GFX + 0x28,
        &[BASE + 0x36bf490, BUTTON, BASE + 0x36bf490, FOREIGN],
    );
    f.int(GFX + 0x30, 2);
    f.int(GFX + 0x34, 2);
    a.require_before_down(&f).unwrap();
    assert!(f.untouched(FOREIGN, 0x1000));
    f.array(GFX + 0x50, &[SPRITE]);
    a.require_before_down(&f).unwrap();
}
#[test]
fn competing_first_duplicate_or_unknown_route_never_authorizes_down() {
    for which in 0..4 {
        let mut f = fixture();
        let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
        match which {
            0 => {
                f.array(
                    GFX + 0x28,
                    &[BASE + 0x36bf490, FOREIGN, BASE + 0x36bf490, BUTTON],
                );
                f.int(GFX + 0x30, 2);
                f.int(GFX + 0x34, 2);
            }
            1 => {
                f.array(GFX + 0x50, &[SPRITE, SPRITE]);
            }
            2 => {
                f.array(
                    GFX + 0x28,
                    &[BASE + 0x36bf490, BUTTON, BASE + 0x36bf490, BUTTON],
                );
                f.int(GFX + 0x30, 2);
                f.int(GFX + 0x34, 2);
            }
            _ => {
                f.array(GFX + 0x28, &[BASE + 0xdeadbe0, BUTTON]);
                f.int(GFX + 0x30, 1);
                f.int(GFX + 0x34, 1);
            }
        }
        a.require_before_move(&f).unwrap();
        assert!(a.require_before_down(&f).is_err(), "case{which}");
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}
#[test]
fn route_missing_cache_or_cursor_alias_refuses_down_without_target_adoption() {
    for at in [
        GFX + 0x40,
        GFX + 0x48,
        GFX + 0x1c0,
        REGISTRY + 0x120,
        REGISTRY + 0x128,
        REGISTRY + 0x130,
    ] {
        let mut f = fixture();
        let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
        if at == REGISTRY + 0x120 || at == REGISTRY + 0x128 {
            f.word(at, 0);
        } else if at == REGISTRY + 0x130 {
            f.put(at, &[1]);
        } else {
            floats(&mut f, at, &[2.0, 3.0]);
        }
        assert!(a.require_before_down(&f).is_err(), "{at:x}");
    }
}
#[test]
fn same_geometry_allows_hover_state_but_not_disabled_handler_or_movement() {
    let mut f = fixture();
    let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
    f.int(BUTTON + 0x9c, 2);
    a.require_before_down(&f).unwrap();
    for which in 0..6 {
        let mut f = fixture();
        let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
        match which {
            0 => f.int(BUTTON + 0x9c, 3),
            1 => f.put(BUTTON + 0xd4, &[0x48]),
            2 => f.put(BUTTON + 0x1d9, &[1]),
            3 => f.put(POSITION + 0x128, &[0]),
            4 => floats(&mut f, SPRITE + 0xb0, &[10.0, 0.0]),
            _ => f.word(BUTTON + 0xc8, FOREIGN),
        }
        assert!(a.require_before_move(&f).is_err(), "case{which}");
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}
#[test]
fn root_hidden_animating_foreign_or_offscreen_geometry_refuses() {
    for which in 0..8 {
        let mut f = fixture();
        match which {
            0 => f.put(MAP_ROOT + 0x95, &[0]),
            1 => f.int(MAP_ROOT + 0x1390, 1),
            2 => f.word(MAP_ROOT + 0x88, FOREIGN),
            3 => floats(&mut f, SPRITE + 0xb0, &[2000.0, 0.0]),
            4 => floats(&mut f, SPRITE + 0xb0, &[f32::NAN, 0.0]),
            5 => floats(&mut f, GFX + 0x2c5b0, &[0.8]),
            6 => f.int(RESOURCE + 0x78, 0),
            _ => f.word(RESOURCE, BASE + 0xdeadbe0),
        }
        assert!(
            OpenAnchor::bind(&f, &selected_anchor(&f), 64).is_err(),
            "case{which}"
        );
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}
#[test]
fn exact_frame_renderer_resource_and_null_parent_variants_are_typed() {
    let mut f = fixture();
    f.word(SPRITE, BASE + 0x36be530);
    f.word(RESOURCE, BASE + 0x36be828);
    OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
    let mut f = fixture();
    f.word(BUTTON + 0x180, 0);
    f.word(MAP_ROOT + 0x1358, 0);
    f.word(SPRITE + 0x108, 0);
    let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
    a.require_before_down(&f).unwrap();
    assert!(f.untouched(POSITION + 0x128, 1));
}
#[test]
fn original_owner_target_and_resource_refuse_before_new_payload_or_callback() {
    for at in [
        REGISTRY + 0x98,
        APP + 0x38,
        BUTTON + 0x50,
        BUTTON + 0xd8,
        BUTTON + 0xe0,
        BUTTON + 0x180,
        SPRITE + 8,
        SPRITE + 0x108,
        MAP_ROOT + 0x1358,
    ] {
        let mut f = fixture();
        let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
        f.word(at, FOREIGN);
        assert!(a.require_before_move(&f).is_err(), "{at:x}");
        assert!(f.untouched(FOREIGN, 0x1000));
        assert!(f.untouched(FUNCTION + 8, 8));
    }
}
#[test]
fn final_geometry_witness_and_current_route_tear_are_not_movement_authority() {
    for route in [false, true] {
        let mut f = fixture();
        let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
        if route {
            f.tear(GFX + 0x40, 8, 2);
        } else {
            f.tear(SPRITE + 0xb0, 8, 2);
        }
        let e = if route {
            a.require_before_down(&f)
        } else {
            a.require_before_move(&f)
        }
        .unwrap_err();
        assert_eq!(e.code, "torn_read");
    }
}
#[test]
fn exact_owned_idle_hotkey_wrapper_is_mouse_only_and_unknowns_refuse_early() {
    for flag in [0, 1] {
        let mut f = fixture();
        f.word(BASE + 0x37ea778, HOTKEYS);
        f.word(HOTKEYS, BASE + 0x36b6490);
        f.word(BASE + 0x384b758, APP);
        f.word(BASE + 0x36b6490 + 0x20, BASE + 0x2867050);
        f.put(BASE + 0x37ea780, &[flag]);
        f.int(HOTKEYS + 0x64, 0);
        let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
        a.require_before_down(&f).unwrap();
        assert!(f.untouched(HOTKEYS + 8, 0x10));
    }
    for which in 0..6 {
        let mut f = fixture();
        f.word(BASE + 0x37ea778, HOTKEYS);
        f.word(HOTKEYS, BASE + 0x36b6490);
        f.word(BASE + 0x384b758, APP);
        f.word(BASE + 0x36b6490 + 0x20, BASE + 0x2867050);
        f.put(BASE + 0x37ea780, &[1]);
        f.int(HOTKEYS + 0x64, 0);
        match which {
            0 => f.word(BASE + 0x37ea778, 1),
            1 => f.word(HOTKEYS, BASE + 0xdeadbe0),
            2 => f.word(BASE + 0x384b758, FOREIGN),
            3 => f.int(HOTKEYS + 0x64, 1),
            4 => f.put(BASE + 0x37ea780, &[2]),
            _ => f.word(BASE + 0x36b6490 + 0x20, FOREIGN),
        }
        assert!(
            OpenAnchor::bind(&f, &selected_anchor(&f), 64).is_err(),
            "case{which}"
        );
        assert!(f.untouched(BUTTON, 0x200));
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}
#[test]
fn typed_inert_hidden_lifetime_is_visibility_only_not_a_blanket_exemption() {
    const PLANE: u64 = 0xd60000;
    const POPUP: u64 = 0xd70000;
    for which in 0..6 {
        let mut f = fixture();
        f.array(REGISTRY + 0xa70, &[PLANE]);
        f.word(PLANE, BASE + 0x35f7af0);
        f.word(PLANE + 0x490, POPUP);
        f.put(PLANE + 0x498, &[0, 0]);
        f.word(POPUP, BASE + 0x36ba978);
        f.word(POPUP + 0x88, REGISTRY);
        f.put(POPUP + 0x95, &[16]);
        f.int(POPUP + 0x1390, 0);
        f.int(POPUP + 0x13d8, 0);
        match which {
            0 => {}
            1 => f.put(POPUP + 0x95, &[8]),
            2 => f.int(POPUP + 0x1390, 1),
            3 => f.word(POPUP + 0x88, FOREIGN),
            4 => f.put(PLANE + 0x499, &[1]),
            _ => f.array(REGISTRY + 0xa88, &[PLANE]),
        }
        let r = OpenAnchor::bind(&f, &selected_anchor(&f), 64);
        assert_eq!(r.is_ok(), which == 0, "case{which}");
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}
#[test]
fn original_owner_primary_change_refuses_before_old_layout_or_callback_payload() {
    for (object, payload) in [
        (MAP, MAP + 0x30),
        (GFX, GFX + 0x170),
        (MAP_ROOT, MAP_ROOT + 0x95),
        (SETTINGS, SETTINGS + 0x28),
        (MENU, MENU + 0x2a28),
    ] {
        let mut f = fixture();
        let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
        f.word(object, BASE + 0xdeadbe0);
        assert!(a.require_before_move(&f).is_err(), "{object:x}");
        assert!(f.untouched(payload, 1), "{payload:x}");
        assert!(f.untouched(FUNCTION + 8, 8));
    }
}

#[test]
fn initial_null_or_nonnull_attachment_mismatch_refuses_before_parent_payload() {
    for (button_parent, root_parent, sprite_parent) in [
        (0, FOREIGN, 0),
        (0, 0, FOREIGN),
        (POSITION, POSITION, 0),
        (FOREIGN, 0, FOREIGN),
    ] {
        let mut f = fixture();
        f.word(BUTTON + 0x180, button_parent);
        f.word(MAP_ROOT + 0x1358, root_parent);
        f.word(SPRITE + 0x108, sprite_parent);
        assert!(OpenAnchor::bind(&f, &selected_anchor(&f), 64).is_err());
        assert!(f.untouched(FOREIGN, 0x1000));
        assert!(f.untouched(POSITION + 0x128, 1));
        assert!(f.untouched(RESOURCE + 0x78, 4));
        assert!(f.untouched(FUNCTION + 8, 8));
    }
}
#[test]
fn original_null_attachment_edges_cannot_gain_a_foreign_parent_between_reads() {
    for at in [BUTTON + 0x180, MAP_ROOT + 0x1358, SPRITE + 0x108] {
        let mut f = fixture();
        f.word(BUTTON + 0x180, 0);
        f.word(MAP_ROOT + 0x1358, 0);
        f.word(SPRITE + 0x108, 0);
        let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
        f.word(at, FOREIGN);
        assert!(a.require_before_move(&f).is_err(), "{at:x}");
        assert!(f.untouched(FOREIGN, 0x1000));
        assert!(f.untouched(RESOURCE + 0x78, 4));
        assert!(f.untouched(FUNCTION + 8, 8));
    }
}
#[test]
fn original_nonnull_renderer_attachment_cannot_be_replaced_before_down() {
    let mut f = fixture();
    let a = OpenAnchor::bind(&f, &selected_anchor(&f), 64).unwrap();
    f.word(SPRITE + 0x108, FOREIGN);
    assert!(a.require_before_down(&f).is_err());
    assert!(f.untouched(FOREIGN, 0x1000));
    assert!(f.untouched(POSITION + 0x128, 1));
    assert!(f.untouched(FUNCTION + 8, 8));
}
#[test]
fn button_only_under_a_hidden_nested_container_is_not_a_direct_target() {
    const NESTED: u64 = 0xe00000;
    let mut f = fixture();
    f.word(NESTED, BASE + 0x36ba978);
    f.word(NESTED + 0x88, REGISTRY);
    f.put(NESTED + 0x95, &[0]);
    f.int(NESTED + 0x13d8, 0);
    f.int(NESTED + 0x1390, 0);
    // GUI-object child form; its missing name is not the requested button.
    f.word(NESTED + 0x80, 0);
    f.array(NESTED + 0xe50, &[BUTTON]);
    f.array(MAP_ROOT + 0xe50, &[NESTED + 0x30]);
    assert!(OpenAnchor::bind(&f, &selected_anchor(&f), 64).is_err());
    assert!(f.untouched(SPRITE + 8, 8));
    assert!(f.untouched(POSITION + 0x128, 1));
    assert!(f.untouched(FUNCTION + 8, 8));
}
#[test]
fn direct_membership_contract_allows_unrelated_paths_but_never_a_nested_substitute() {
    let valid = vec![(MAP_ROOT, vec![BUTTON, POSITION]), (POSITION, vec![EDIT])];
    button::require_direct_member(MAP_ROOT, BUTTON, &valid).unwrap();
    for paths in [
        vec![(MAP_ROOT, vec![POSITION]), (POSITION, vec![BUTTON])],
        vec![(POSITION, vec![BUTTON])],
        vec![(MAP_ROOT, vec![BUTTON, BUTTON])],
        vec![(MAP_ROOT, vec![BUTTON]), (MAP_ROOT, vec![BUTTON])],
    ] {
        assert!(button::require_direct_member(MAP_ROOT, BUTTON, &paths).is_err());
    }
}
