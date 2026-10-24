//! Sparse Mac layout fixtures; no OS API, input, process attachment or clock.
use super::*;
use crate::{
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

fn fixture() -> Fixture {
    let mut f = Fixture::new(StringAbi::LibCxx);
    for (at, value) in [
        (COUNTRY + 0xd10, PRODUCTION),
        (PRODUCTION, BASE + 0x338b678),
        (PRODUCTION + 0x290, COUNTRY),
        (IDLER + 0x4c0, REGISTRY),
        (REGISTRY, BASE + 0x34974d0),
        (IDLER + 0x670, MANAGER),
        (MANAGER + 0x180, IDLER),
        (MANAGER + 0x190, FIND),
        (FIND, BASE + 0x3457c38),
        (FIND + 0xc30, IDLER),
        (FIND + 0xc38, WINDOW),
        (WINDOW, BASE + 0x34ac0b0),
        (WINDOW + 0x88, REGISTRY),
        (FIND + 0xc88, ROW),
        (ROW, BASE + 0x34580d8),
        (ROW + 0x3f8, STATE),
        (IDLER + 0x708, CAMERA),
        (STATE, BASE + 0x337caf0),
        (STATE + 0x20, TEMPLATE),
        (TEMPLATE, BASE + 0x337de78),
        (BASE + 0x35dc058, REGISTRY),
        (FIND + 0xc40, EDIT),
        (EDIT, BASE + 0x34add98),
        (EDIT + 0x180, BASE + 0x34ae0c8),
        (EDIT + 0x58, REGISTRY),
        (EDIT + 0x50, KIND),
        (KIND, BASE + 0x34ae140),
        (REGISTRY + 0x138, EDIT),
        (REGISTRY + 0x210, EDIT + 0x180),
    ] {
        f.word(at, value);
    }
    // Navigation additionally retains the ordinary outer printable receiver.
    // Mouse opener/focus-only readers deliberately do not need these fields.
    for (at, value) in [
        (REGISTRY + 0x98, 0x9c0000),
        (0x9c0000, BASE + 0x33dafa8),
        (IDLER + 0x4f8, 0x9d0000),
        (BASE + 0x3639ad8, 0x9d0000),
        (0x9d0038, IDLER),
        (0x9d0300, 0x9c0000),
        (0x9d0308, REGISTRY),
        (BASE + 0x3308f90 + 0x20, BASE + 0x776fe0),
        (BASE + 0x35d8c60, 0x9e0000),
        (0x9e0000, BASE + 0x34a6760),
        (BASE + 0x35d8dd0, 0),
    ] {
        f.word(at, value);
    }
    f.put(IDLER + 0x7b5, &[1]);
    f.put(BASE + 0x3519990, &[0]);
    f.put(0x9e0044, &[0]);
    f.put(WINDOW + 0x95, &[8]);
    f.int(WINDOW + 0x12b8, 0);
    f.int(WINDOW + 0x1270, 0);
    f.string(FIND + 0xc48, "");
    f.word(FIND + 0xc90, ENTRIES);
    f.int(FIND + 0xc98, 1);
    f.int(FIND + 0xc9c, 0);
    f.put(ENTRIES, &[0; 0x40]);
    f.word(ENTRIES + 0x30, STATE);
    f.int(ENTRIES + 0x38, 2);
    f.int(ROW + 0x408, 2);
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
    f.array(WINDOW + 0xd30, &[EDIT]);
    f
}

fn full_query(f: &mut Fixture) {
    f.string(FIND + 0xc48, "64");
    f.int(FIND + 0xc9c, 1);
}
fn closed(f: &mut Fixture) {
    f.put(WINDOW + 0x95, &[0]);
    floats(f, CAMERA + 0x6a4, &[3120.0, 9.5, 1480.0]);
}

#[test]
fn bind_requires_visible_idle_empty_focused_own_find() {
    let f = fixture();
    let anchor = bind(&f, 64).unwrap();
    assert_eq!(anchor.decimal_query(), "64");
    assert_eq!(
        anchor
            .find_anchor()
            .binding()
            .own_state()
            .unwrap()
            .state_id(),
        64
    );
    assert_eq!(f.snapshots.get(), 1);
    let proof = anchor.observe_prefix(&f, "").unwrap().unwrap();
    assert_eq!(proof.expected_prefix(), "");
    assert_eq!(proof.next_digit(), Some('6'));
    assert_eq!(proof.observation().context().player, "GER");
}

#[test]
fn bind_never_opens_focuses_or_clears_an_unready_find() {
    for mode in 0..8 {
        let mut f = fixture();
        match mode {
            0 => f.word(FIND + 0xc38, 0),
            1 => f.put(WINDOW + 0x95, &[0]),
            2 => f.int(WINDOW + 0x12b8, 1),
            3 => f.word(REGISTRY + 0x138, 0),
            4 => f.word(REGISTRY + 0x210, 0),
            5 => f.string(FIND + 0xc48, "6"),
            6 => f.string(FIND + 0xc48, "\n"),
            _ => f.word(FIND + 0xc40, 0),
        }
        assert!(bind(&f, 64).is_err(), "mode {mode}");
    }
    let mut f = fixture();
    f.int(STATE + 0xc0, 7);
    assert_eq!(
        bind(&f, 64).unwrap_err().code,
        "construction_target_unavailable"
    );
    assert!(f.untouched(STATE + 0x20, 8));
}

#[test]
fn decimal_prefix_progression_exposes_only_the_one_next_digit() {
    let mut f = fixture();
    let anchor = bind(&f, 64).unwrap();
    assert!(anchor.observe_prefix(&f, "6").unwrap().is_none());
    f.string(FIND + 0xc48, "6");
    let proof = anchor.observe_prefix(&f, "6").unwrap().unwrap();
    assert_eq!(proof.next_digit(), Some('4'));
    assert!(anchor.observe_prefix(&f, "64").unwrap().is_none());
    f.string(FIND + 0xc48, "64");
    let proof = anchor.observe_prefix(&f, "64").unwrap().unwrap();
    assert_eq!(proof.expected_prefix(), "64");
    assert_eq!(proof.next_digit(), None);
}

#[test]
fn canonical_decimal_supports_single_digit_and_internal_zero_at_state_bound() {
    for id in [1usize, 4095] {
        let mut f = fixture();
        let mut states = vec![0; id + 1];
        states[id] = STATE;
        f.array(GS + 0x290, &states);
        f.int(STATE + 0x40, id as i32);
        let anchor = bind(&f, id as i32).unwrap();
        let decimal = id.to_string();
        for index in 0..decimal.len() {
            f.string(FIND + 0xc48, &decimal[..index]);
            let proof = anchor
                .observe_prefix(&f, &decimal[..index])
                .unwrap()
                .unwrap();
            assert_eq!(proof.next_digit(), decimal[index..].chars().next());
            assert!(anchor
                .observe_prefix(&f, &decimal[..index + 1])
                .unwrap()
                .is_none());
        }
        f.string(FIND + 0xc48, &decimal);
        f.int(FIND + 0xc9c, 1);
        assert!(anchor.observe_return(&f).unwrap().is_some());
    }
}

#[test]
fn skipped_repeated_reversed_or_unrelated_coherent_query_never_advances() {
    for (actual, expected) in [
        ("", "64"),
        ("64", "6"),
        ("66", "6"),
        ("064", "64"),
        ("6x", "64"),
        ("4", "64"),
        ("6", ""),
        ("\n", "6"),
    ] {
        let mut f = fixture();
        let anchor = bind(&f, 64).unwrap();
        f.string(FIND + 0xc48, actual);
        assert_eq!(
            anchor.observe_prefix(&f, expected).unwrap_err().code,
            "construction_state_changed",
            "{actual:?}/{expected:?}"
        );
    }
    let f = fixture();
    let anchor = bind(&f, 64).unwrap();
    let snapshots = f.snapshots.get();
    for expected in ["0", "065", "65", "644", "é"] {
        assert_eq!(
            anchor.observe_prefix(&f, expected).unwrap_err().code,
            "invalid_arguments"
        );
        assert_eq!(f.snapshots.get(), snapshots);
    }
}

#[test]
fn dynamic_raw_flags_are_not_mistaken_for_control_replacement() {
    let mut f = fixture();
    let anchor = bind(&f, 64).unwrap();
    f.put(EDIT + 0x64, &0x20804u32.to_le_bytes());
    f.put(EDIT + 0x239, &[0x90]);
    assert_eq!(
        anchor.observe_prefix(&f, "").unwrap().unwrap().next_digit(),
        Some('6')
    );
}

#[test]
fn every_digit_and_return_require_both_current_focus_aliases_and_idle_visibility() {
    for (field, value) in [
        (REGISTRY + 0x138, 0),
        (REGISTRY + 0x210, 0),
        (REGISTRY + 0x210, EDIT + 0x1a0),
        (REGISTRY + 0x138, FOREIGN),
    ] {
        let mut f = fixture();
        let anchor = bind(&f, 64).unwrap();
        f.word(field, value);
        assert_eq!(
            anchor.observe_prefix(&f, "").unwrap_err().code,
            "construction_find_unavailable"
        );
        full_query(&mut f);
        assert_eq!(
            anchor.observe_return(&f).unwrap_err().code,
            "construction_find_unavailable"
        );
        assert!(f.untouched(FOREIGN, 8));
    }
    for hide in [false, true] {
        let mut f = fixture();
        let anchor = bind(&f, 64).unwrap();
        if hide {
            f.put(WINDOW + 0x95, &[0]);
        } else {
            f.int(WINDOW + 0x1270, 2);
        }
        assert_eq!(
            anchor.observe_prefix(&f, "").unwrap_err().code,
            "construction_find_unavailable"
        );
    }
}

#[test]
fn changed_edit_pointer_or_child_membership_refuses_before_replacement_payload() {
    for edge in 0..4 {
        let mut f = fixture();
        let anchor = bind(&f, 64).unwrap();
        match edge {
            0 => f.word(FIND + 0xc40, FOREIGN),
            1 => f.array(WINDOW + 0xd30, &[EDIT, FOREIGN]),
            2 => f.array(WINDOW + 0xd30, &[EDIT, EDIT]),
            _ => f.array(WINDOW + 0xd30, &[]),
        }
        assert_eq!(
            anchor.observe_prefix(&f, "").unwrap_err().code,
            if edge == 2 {
                "invalid_construction"
            } else {
                "construction_state_changed"
            }
        );
        assert!(f.untouched(FOREIGN, 0x500));
        assert!(f.untouched(FIND + 0xc48, 24));
    }
}

#[test]
fn rebuilt_window_is_not_adopted_even_with_same_own_control_and_name() {
    let mut f = fixture();
    let anchor = bind(&f, 64).unwrap();
    f.word(FOREIGN, BASE + 0x34ac0b0);
    f.word(FOREIGN + 0x88, REGISTRY);
    f.put(FOREIGN + 0x95, &[8]);
    f.int(FOREIGN + 0x12b8, 0);
    f.int(FOREIGN + 0x1270, 0);
    f.array(FOREIGN + 0xd30, &[EDIT]);
    f.word(FIND + 0xc38, FOREIGN);
    assert_eq!(
        anchor.observe_prefix(&f, "").unwrap_err().code,
        "construction_state_changed"
    );
    assert!(f.untouched(FOREIGN, 0x1300));
    assert!(f.untouched(FIND + 0xc48, 24));
}

#[test]
fn original_scope_target_and_global_gui_refuse_before_new_payload() {
    let states = fixture().get_word(GS + 0x290);
    for field in [
        BASE + 0x3519740,
        BASE + 0x35196c0,
        BASE + 0x34d0de0,
        COUNTRY + 0xd10,
        IDLER + 0x4c0,
        IDLER + 0x670,
        MANAGER + 0x190,
        IDLER + 0x708,
        states + 64 * 8,
        STATE + 0x20,
        BASE + 0x35dc058,
    ] {
        let mut f = fixture();
        let anchor = bind(&f, 64).unwrap();
        f.word(field, FOREIGN);
        assert_eq!(
            anchor.observe_prefix(&f, "").unwrap_err().code,
            "construction_state_changed",
            "{field:x}"
        );
        assert!(f.untouched(FOREIGN, 0x1300));
        assert!(f.untouched(FIND + 0xc48, 24));
    }
}

#[test]
fn same_pointer_control_still_requires_own_type_owner_receiver_and_name() {
    for (field, value) in [
        (EDIT, BASE + 0x34add90),
        (EDIT + 0x58, FOREIGN),
        (EDIT + 0x180, BASE + 0x34ae0c0),
    ] {
        let mut f = fixture();
        let anchor = bind(&f, 64).unwrap();
        f.word(field, value);
        assert!(anchor.observe_prefix(&f, "").is_err());
        assert!(f.untouched(FOREIGN, 8));
    }
    let mut f = fixture();
    let anchor = bind(&f, 64).unwrap();
    f.string(KIND + 0x28, "other_text");
    assert!(anchor.observe_prefix(&f, "").is_err());
}

#[test]
fn own_date_can_advance_but_campaign_setup_or_chronology_cannot_change() {
    let mut f = fixture();
    let anchor = bind(&f, 64).unwrap();
    f.set_date(DATE + 1);
    f.int(GS + 0x47c, 3);
    f.put(IDLER + 0x679, &[0]);
    let proof = anchor.observe_prefix(&f, "").unwrap().unwrap();
    assert_eq!(proof.observation().context().date_raw, DATE + 1);
    assert_eq!(anchor.find_anchor().context().date_raw, DATE);
    for (field, value) in [
        (GS + 0x428, DATE - 1),
        (GS + 0x5d0, 1),
        (GS + 0x5d8, 0),
        (GS + 0x4d8, 2),
    ] {
        let mut f = fixture();
        let anchor = bind(&f, 64).unwrap();
        f.int(field, value);
        assert_eq!(
            anchor.observe_prefix(&f, "").unwrap_err().code,
            "construction_state_changed"
        );
    }
    let mut f = fixture();
    let anchor = bind(&f, 64).unwrap();
    f.set_playthrough("ccddccdd-1122-4344-8566-778899aabbcc");
    assert_eq!(
        anchor.observe_prefix(&f, "").unwrap_err().code,
        "construction_state_changed"
    );
}

#[test]
fn return_requires_complete_exact_query_and_original_first_state_object_and_id() {
    let mut f = fixture();
    let anchor = bind(&f, 64).unwrap();
    for query in ["", "6", "064", "65"] {
        f.string(FIND + 0xc48, query);
        assert_eq!(
            anchor.observe_return(&f).unwrap_err().code,
            "construction_state_changed"
        );
    }
    f.string(FIND + 0xc48, "64");
    assert!(anchor.observe_return(&f).unwrap().is_none());
    full_query(&mut f);
    f.word(FIND + 0xc88, 0);
    assert!(anchor.observe_return(&f).unwrap().is_none());
    f.word(FIND + 0xc88, ROW);
    let proof = anchor.observe_return(&f).unwrap().unwrap();
    assert!(proof.observation().first_row_is_requested_state());
    assert_eq!(
        proof.observation().first_result().unwrap().state_id(),
        Some(64)
    );
}

#[test]
fn other_kinds_or_foreign_state_result_never_prove_return() {
    for (kind, offset) in [(1, 0x3f0), (3, 0x400), (0, 0)] {
        let mut f = fixture();
        let anchor = bind(&f, 64).unwrap();
        full_query(&mut f);
        f.int(ROW + 0x408, kind);
        f.int(ENTRIES + 0x38, kind);
        if offset != 0 {
            f.word(ROW + offset, FOREIGN);
            f.word(ENTRIES + 0x30, FOREIGN);
        }
        assert!(anchor.observe_return(&f).unwrap().is_none());
        assert!(f.untouched(FOREIGN, 0x500));
    }
    let mut f = fixture();
    let anchor = bind(&f, 64).unwrap();
    full_query(&mut f);
    f.word(FOREIGN, BASE + 0x337caf0);
    f.int(FOREIGN + 0x40, 65);
    let mut states = vec![0; 66];
    states[64] = STATE;
    states[65] = FOREIGN;
    f.array(GS + 0x290, &states);
    f.word(ROW + 0x3f8, FOREIGN);
    f.word(ENTRIES + 0x30, FOREIGN);
    assert!(anchor.observe_return(&f).unwrap().is_none());
    assert!(f.untouched(FOREIGN + 0xc0, 8));
    assert!(f.untouched(FOREIGN + 0x20, 8));
}

#[test]
fn a_same_id_replacement_cannot_prove_return_or_finished() {
    let mut f = fixture();
    let anchor = bind(&f, 64).unwrap();
    full_query(&mut f);
    let mut states = vec![0; 65];
    states[64] = FOREIGN;
    f.array(GS + 0x290, &states);
    f.word(ROW + 0x3f8, FOREIGN);
    f.word(ENTRIES + 0x30, FOREIGN);
    assert_eq!(
        anchor.observe_return(&f).unwrap_err().code,
        "construction_state_changed"
    );
    assert!(f.untouched(FOREIGN, 0x1300));
    closed(&mut f);
    assert_eq!(
        anchor.observe_finished(&f).unwrap_err().code,
        "construction_state_changed"
    );
    assert!(f.untouched(FOREIGN, 0x1300));
}

#[test]
fn final_requires_closed_find_and_exact_map_coordinates_without_wrap_inference() {
    let mut f = fixture();
    let anchor = bind(&f, 64).unwrap();
    floats(&mut f, CAMERA + 0x6a4, &[3120.0, 9.5, 1480.0]);
    assert!(anchor.observe_finished(&f).unwrap().is_none());
    closed(&mut f);
    f.int(WINDOW + 0x1270, 2);
    assert!(anchor.observe_finished(&f).unwrap().is_none());
    f.int(WINDOW + 0x1270, 0);
    for look in [
        [3121.0, 9.5, 1480.0],
        [3120.0, 9.5, 1481.0],
        [13120.0, 9.5, 1480.0],
    ] {
        floats(&mut f, CAMERA + 0x6a4, &look);
        assert!(anchor.observe_finished(&f).unwrap().is_none());
    }
    floats(&mut f, CAMERA + 0x6a4, &[3120.0, 9.5, 1480.0]);
    assert_eq!(
        anchor
            .observe_finished(&f)
            .unwrap()
            .unwrap()
            .observation()
            .camera()
            .look_at(),
        [3120.0, 9.5, 1480.0]
    );
}

#[test]
fn final_hidden_or_teardown_skips_retained_edit_query_and_focus_payload() {
    for teardown in [false, true] {
        let mut f = fixture();
        let anchor = bind(&f, 64).unwrap();
        closed(&mut f);
        if teardown {
            f.word(FIND + 0xc38, 0);
        }
        f.word(FIND + 0xc40, FOREIGN);
        f.word(REGISTRY + 0x138, FOREIGN);
        f.word(REGISTRY + 0x210, FOREIGN);
        assert!(anchor.observe_finished(&f).unwrap().is_some());
        for (address, size) in [
            (FIND + 0xc40, 8),
            (FIND + 0xc48, 24),
            (FIND + 0xc88, 8),
            (REGISTRY + 0x138, 8),
            (REGISTRY + 0x210, 8),
            (EDIT, 0x300),
            (FOREIGN, 0x500),
        ] {
            assert!(
                f.untouched(address, size),
                "{address:x}, teardown {teardown}"
            );
        }
    }
}

#[test]
fn final_hidden_replacement_root_is_never_adopted_as_original_close() {
    let mut f = fixture();
    let anchor = bind(&f, 64).unwrap();
    closed(&mut f);
    f.word(FIND + 0xc38, FOREIGN);
    assert_eq!(
        anchor.observe_finished(&f).unwrap_err().code,
        "construction_state_changed"
    );
    assert!(f.untouched(FOREIGN, 0x1300));
}

#[test]
fn every_stage_retains_final_witness_and_process_guards() {
    for (field, size) in [
        (REGISTRY + 0x138, 8),
        (REGISTRY + 0x210, 8),
        (FIND + 0xc38, 8),
        (WINDOW + 0xd30, 16),
    ] {
        let mut f = fixture();
        let anchor = bind(&f, 64).unwrap();
        f.tear(field, size, 2);
        assert_eq!(
            anchor.observe_prefix(&f, "").unwrap_err().code,
            "torn_read",
            "{field:x}"
        );
    }
    let mut f = fixture();
    let anchor = bind(&f, 64).unwrap();
    full_query(&mut f);
    f.tear(ROW + 0x408, 4, 2);
    assert_eq!(anchor.observe_return(&f).unwrap_err().code, "torn_read");
    let mut f = fixture();
    let anchor = bind(&f, 64).unwrap();
    closed(&mut f);
    f.tear(CAMERA + 0x698, 24, 2);
    assert_eq!(anchor.observe_finished(&f).unwrap_err().code, "torn_read");
    for final_failure in [false, true] {
        let mut f = fixture();
        let anchor = bind(&f, 64).unwrap();
        f.fail_final = final_failure;
        f.swap_final_identity = !final_failure;
        assert!(anchor.observe_prefix(&f, "").is_err());
    }
}

#[test]
fn compact_baseline_is_private_evidence_and_not_navigation_authority() {
    let f = fixture();
    let anchor = bind(&f, 64).unwrap();
    let mut value = anchor.private_baseline().unwrap();
    assert!(serde_json::to_vec(&value).unwrap().len() < 8192);
    assert_eq!(value["state_id"], 64);
    assert_eq!(value["decimal_query"], "64");
    assert_eq!(value["window_pointer"], WINDOW);
    assert_eq!(value["edit_box"], EDIT);
    assert_eq!(value["member_count"], 1);
    assert_eq!(value["membership_sha256"].as_str().unwrap().len(), 64);
    assert_eq!(value["input_authorized"], false);
    assert_eq!(value["delivery_verified"], false);
    value["state_id"] = json!(65);
    value["input_authorized"] = json!(true);
    assert_eq!(anchor.decimal_query(), "64");
    assert_eq!(
        anchor.observe_prefix(&f, "").unwrap().unwrap().next_digit(),
        Some('6')
    );
}

#[test]
fn invalid_state_non_mac_abi_and_changed_process_never_read_control_payload() {
    let f = fixture();
    for id in [-1, 0, 4096] {
        assert_eq!(bind(&f, id).unwrap_err().code, "invalid_arguments");
        assert_eq!(f.snapshots.get(), 0);
    }
    let mut f = fixture();
    f.abi = StringAbi::LibStdCxx;
    assert_eq!(bind(&f, 64).unwrap_err().code, "unsupported_platform");
    assert_eq!(f.snapshots.get(), 0);
    let mut f = fixture();
    let anchor = bind(&f, 64).unwrap();
    f.id.start_ticks.push('9');
    assert_eq!(
        anchor.observe_prefix(&f, "").unwrap_err().code,
        "ownership_changed"
    );
    assert!(f.read_counts().is_empty());
}

#[path = "mac_construction_find_characters.rs"]
mod characters;
