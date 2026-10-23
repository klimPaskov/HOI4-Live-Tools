//! Sparse Mac memory on any unit-test host; no process, UI or input APIs.
use super::*;
use crate::{
    construction::mac_ui::bind_selection,
    memory::StringAbi,
    own_focus::assignment_fixture::{Fixture, BASE, COUNTRY, DATE, FOREIGN, GS, IDLER},
};

const PRODUCTION: u64 = 0x900000;
const DATABASE: u64 = 0x910000;
const TEMPLATE: u64 = 0x920000;
const OTHER: u64 = 0x921000;
const NULL: u64 = 0x922000;
const STATE: u64 = 0x930000;
const STATE2: u64 = 0x931000;
const BUILDING: u64 = 0x950000;
const OTHER_BUILDING: u64 = 0x951000;
const STATE2_BUILDING: u64 = 0x952000;
const LINE: u64 = 0x960000;
const SECOND: u64 = 0x961000;
const NEW: u64 = 0x962000;
const GUI: u64 = 0x1000000;
const MANAGER: u64 = 0x1010000;

fn template(f: &mut Fixture, pointer: u64, ordinal: i32, key: &str) {
    f.word(pointer, BASE + 0x332e908);
    f.put(pointer + 0x18, &[1]);
    f.int(pointer + 0x290, ordinal);
    f.string(pointer + 0x228, key);
}

fn state(f: &mut Fixture, pointer: u64, id: i32, buildings: &[u64]) {
    f.word(pointer, BASE + 0x337caf0);
    f.int(pointer + 0x40, id);
    f.int(pointer + 0xc0, 1);
    f.int(pointer + 0xc4, 1);
    f.word(pointer + 0x110, BASE + 0x332e6a8);
    f.int(pointer + 0x178, 2);
    f.int(pointer + 0x17c, id);
    f.int(pointer + 0x180, 0);
    f.array(pointer + 0x130, &[0, 0]);
    let map = f.get_word(pointer + 0x130);
    f.int(pointer + 0x138, 3);
    f.int(pointer + 0x13c, 3);
    f.int(map, -1);
    f.int(map + 4, 0);
    f.int(map + 8, if buildings.len() > 1 { 1 } else { -1 });
    f.array(pointer + 0x148, buildings);
}

fn building(f: &mut Fixture, pointer: u64, state: u64, template: u64) {
    f.word(pointer, BASE + 0x332dcb0);
    f.word(pointer + 0x1a8, state + 0x110);
    f.word(pointer + 0x1b0, template);
}

fn line(f: &mut Fixture, pointer: u64, handle: u64, building: u64, amount: i32) {
    for (field, value) in [
        (pointer, BASE + 0x3389c40),
        (pointer + 8, handle),
        (pointer + 0x20, PRODUCTION),
        (pointer + 0x28, 100000),
        (pointer + 0x30, 100),
        (pointer + 0x38, 0),
        (pointer + 0x70, building),
    ] {
        f.word(field, value);
    }
    for (offset, value) in [
        (0x14, 1),
        (0x18, 0),
        (0x40, 1),
        (0x44, 0),
        (0x48, amount),
        (0x78, 0),
    ] {
        f.int(pointer + offset, value);
    }
    f.put(pointer + 0x8c, &[0]);
    f.put(pointer + 0x99, &[0]);
}

fn fixture(rows: &[u64]) -> Fixture {
    let mut f = Fixture::new(StringAbi::LibCxx);
    for (field, value) in [
        (COUNTRY + 0xd10, PRODUCTION),
        (PRODUCTION, BASE + 0x338b678),
        (PRODUCTION + 0x290, COUNTRY),
        (BASE + 0x34d0ac0, DATABASE),
        (DATABASE, BASE + 0x332dd28),
        (NULL, BASE + 0x332def8),
        (IDLER + 0x4c0, GUI),
        (GUI, BASE + 0x34974d0),
        (IDLER + 0x670, MANAGER),
        (MANAGER + 0x180, IDLER),
        (MANAGER + 0x3b8, TEMPLATE),
        (DATABASE + 0x3a8, NULL),
    ] {
        f.word(field, value);
    }
    f.int(MANAGER + 0x3c0, 1);
    f.put(NULL + 0x18, &[0]);
    template(&mut f, TEMPLATE, 1, "arms_factory");
    template(&mut f, OTHER, 2, "industrial_complex");
    f.array(DATABASE + 0x40, &[NULL, TEMPLATE, OTHER]);
    f.array(GS + 0x290, &[0, STATE, STATE2]);
    state(&mut f, STATE, 1, &[BUILDING, OTHER_BUILDING]);
    state(&mut f, STATE2, 2, &[STATE2_BUILDING]);
    building(&mut f, BUILDING, STATE, TEMPLATE);
    building(&mut f, OTHER_BUILDING, STATE, OTHER);
    building(&mut f, STATE2_BUILDING, STATE2, TEMPLATE);
    line(&mut f, LINE, 0x10000003b, BUILDING, 2);
    line(&mut f, SECOND, 0x20000003b, OTHER_BUILDING, 4);
    line(&mut f, NEW, 0x30000003b, BUILDING, 1);
    f.array(PRODUCTION + 0x70, rows);
    f
}

fn anchor(f: &Fixture) -> PlacementAnchor {
    bind_selection(f, "arms_factory", 1)
        .unwrap()
        .placement_anchor()
        .unwrap()
}

#[test]
fn unchanged_full_queue_and_live_progress_are_pending_never_success() {
    let mut f = fixture(&[LINE, SECOND]);
    let a = anchor(&f);
    let pending = a.observe(&f).unwrap();
    assert_eq!(pending.delta(), PlacementDelta::Pending);
    assert!(!pending.placed());
    f.word(LINE + 0x38, 50);
    f.word(SECOND + 0x30, 200);
    f.set_date(DATE + 1);
    let pending = a.observe(&f).unwrap();
    assert_eq!(pending.delta(), PlacementDelta::Pending);
    assert!(!pending.placed());
}

#[test]
fn exactly_one_existing_matching_line_increment_is_accepted() {
    let mut f = fixture(&[LINE, SECOND]);
    let a = anchor(&f);
    f.int(LINE + 0x48, 3);
    let now = a.observe(&f).unwrap();
    assert_eq!(now.delta(), PlacementDelta::ExistingLineIncremented);
    assert!(now.placed());
}

#[test]
fn exactly_one_new_matching_amount_one_line_is_accepted_from_empty_queue() {
    let mut f = fixture(&[]);
    let a = anchor(&f);
    f.array(PRODUCTION + 0x70, &[NEW]);
    let now = a.observe(&f).unwrap();
    assert_eq!(now.delta(), PlacementDelta::NewLineAdded);
    assert!(now.placed());
}

#[test]
fn one_inserted_line_preserves_all_original_rows_and_relative_order() {
    for rows in [
        [NEW, LINE, SECOND],
        [LINE, NEW, SECOND],
        [LINE, SECOND, NEW],
    ] {
        let mut f = fixture(&[LINE, SECOND]);
        let a = anchor(&f);
        f.array(PRODUCTION + 0x70, &rows);
        assert_eq!(a.observe(&f).unwrap().delta(), PlacementDelta::NewLineAdded);
    }
}

#[test]
fn target_amount_decreases_excess_and_multiple_increments_are_refused() {
    for amount in [0, 1, 4, i32::MAX] {
        let mut f = fixture(&[LINE, SECOND]);
        let a = anchor(&f);
        f.int(LINE + 0x48, amount);
        assert!(a.observe(&f).is_err(), "amount {amount}");
    }
    let mut f = fixture(&[LINE, NEW]);
    let a = anchor(&f);
    f.int(LINE + 0x48, 3);
    f.int(NEW + 0x48, 2);
    assert_eq!(
        a.observe(&f).unwrap_err().code,
        "construction_placement_changed"
    );
}

#[test]
fn adding_a_line_and_changing_an_old_row_is_not_one_placement() {
    let mut f = fixture(&[LINE, SECOND]);
    let a = anchor(&f);
    f.array(PRODUCTION + 0x70, &[LINE, NEW, SECOND]);
    f.int(LINE + 0x48, 3);
    assert_eq!(
        a.observe(&f).unwrap_err().code,
        "construction_placement_changed"
    );
    for amount in [0, 2, 3] {
        let mut f = fixture(&[LINE, SECOND]);
        let a = anchor(&f);
        f.int(NEW + 0x48, amount);
        f.array(PRODUCTION + 0x70, &[LINE, SECOND, NEW]);
        assert_eq!(
            a.observe(&f).unwrap_err().code,
            "construction_placement_changed"
        );
    }
}

#[test]
fn other_building_or_other_own_state_additions_and_increments_are_refused() {
    let mut f = fixture(&[LINE, SECOND]);
    let a = anchor(&f);
    f.int(SECOND + 0x48, 5);
    assert_eq!(
        a.observe(&f).unwrap_err().code,
        "construction_placement_changed"
    );
    for building in [OTHER_BUILDING, STATE2_BUILDING] {
        let mut f = fixture(&[LINE, SECOND]);
        let a = anchor(&f);
        f.word(NEW + 0x70, building);
        f.array(PRODUCTION + 0x70, &[LINE, SECOND, NEW]);
        assert_eq!(
            a.observe(&f).unwrap_err().code,
            "construction_placement_changed"
        );
        let mut f = fixture(&[NEW]);
        f.word(NEW + 0x70, building);
        let a = anchor(&f);
        f.int(NEW + 0x48, 2);
        assert_eq!(
            a.observe(&f).unwrap_err().code,
            "construction_placement_changed"
        );
    }
}

#[test]
fn removed_reordered_or_replaced_old_rows_never_verify_success() {
    for rows in [
        vec![],
        vec![SECOND],
        vec![SECOND, LINE],
        vec![NEW, SECOND],
        vec![NEW, LINE, SECOND, NEW + 0x1000],
    ] {
        let mut f = fixture(&[LINE, SECOND]);
        let a = anchor(&f);
        // Replacement NEW can reuse the old handle only after LINE disappears.
        f.word(NEW + 8, 0x10000003b);
        if rows.last() == Some(&(NEW + 0x1000)) {
            line(&mut f, NEW + 0x1000, 0x40000003b, BUILDING, 1);
        }
        f.array(PRODUCTION + 0x70, &rows);
        assert!(a.observe(&f).is_err(), "{rows:x?}");
    }
}

#[test]
fn one_new_row_does_not_hide_reordering_of_old_rows() {
    let mut f = fixture(&[LINE, SECOND]);
    let a = anchor(&f);
    f.array(PRODUCTION + 0x70, &[SECOND, NEW, LINE]);
    assert_eq!(
        a.observe(&f).unwrap_err().code,
        "construction_placement_changed"
    );
}

#[test]
fn old_handle_and_same_named_building_identity_replacements_are_refused() {
    let mut f = fixture(&[LINE, SECOND]);
    let a = anchor(&f);
    f.word(LINE + 8, 0x50000003b);
    f.int(LINE + 0x48, 3);
    assert_eq!(
        a.observe(&f).unwrap_err().code,
        "construction_placement_changed"
    );
    let mut f = fixture(&[LINE, SECOND]);
    let a = anchor(&f);
    let replacement = BUILDING + 0x4000;
    building(&mut f, replacement, STATE, TEMPLATE);
    f.array(STATE + 0x148, &[replacement, OTHER_BUILDING]);
    f.word(LINE + 0x70, replacement);
    f.int(LINE + 0x48, 3);
    assert_eq!(
        a.observe(&f).unwrap_err().code,
        "construction_placement_changed"
    );
}

#[test]
fn unchanged_original_rows_in_other_states_are_preserved_as_typed_evidence() {
    let mut f = fixture(&[LINE, SECOND]);
    f.word(SECOND + 0x70, STATE2_BUILDING);
    let a = anchor(&f);
    f.int(LINE + 0x48, 3);
    assert!(a.observe(&f).unwrap().placed());
    f.int(SECOND + 0x48, 5);
    assert_eq!(
        a.observe(&f).unwrap_err().code,
        "construction_placement_changed"
    );
}

#[test]
fn a_foreign_new_line_is_refused_before_its_building_payload() {
    let mut f = fixture(&[LINE]);
    let a = anchor(&f);
    f.word(NEW + 0x20, FOREIGN);
    f.array(PRODUCTION + 0x70, &[LINE, NEW]);
    assert_eq!(a.observe(&f).unwrap_err().code, "invalid_construction");
    assert!(f.untouched(NEW + 0x70, 8));
    assert!(f.untouched(FOREIGN, 0x1000));
}

#[test]
fn new_foreign_state_location_is_incomplete_and_never_success() {
    let mut f = fixture(&[LINE]);
    let a = anchor(&f);
    f.word(NEW + 0x70, STATE2_BUILDING);
    f.int(STATE2 + 0xc0, 2);
    f.int(STATE2 + 0xc4, 2);
    f.array(PRODUCTION + 0x70, &[LINE, NEW]);
    assert_eq!(
        a.observe(&f).unwrap_err().code,
        "construction_queue_unavailable"
    );
    assert!(f.untouched(STATE2 + 0x40, 4));
}

#[test]
fn incomplete_or_anomalous_replacement_queues_do_not_become_pending() {
    for kind in 0..4 {
        let mut f = fixture(&[LINE]);
        let a = anchor(&f);
        f.array(PRODUCTION + 0x70, &[LINE, NEW]);
        match kind {
            0 => f.word(NEW, BASE + 0x1000),
            1 => f.word(NEW + 0x28, u64::MAX),
            2 => f.word(NEW + 0x70, 0),
            3 => f.word(BUILDING + 0x1a8, 0),
            _ => unreachable!(),
        }
        assert_eq!(
            a.observe(&f).unwrap_err().code,
            "construction_queue_unavailable"
        );
    }
}

#[test]
fn exact_selected_target_is_required_before_any_placement_anchor() {
    for (mode, pointer) in [(0, TEMPLATE), (1, OTHER)] {
        let mut f = fixture(&[]);
        f.int(MANAGER + 0x3c0, mode);
        f.word(MANAGER + 0x3b8, pointer);
        let selection = bind_selection(&f, "arms_factory", 1).unwrap();
        assert_eq!(
            selection.placement_anchor().unwrap_err().code,
            "construction_placement_unavailable"
        );
    }
}

#[test]
fn lost_or_other_selection_refuses_before_replacement_queue_payload() {
    for (mode, pointer) in [(0, FOREIGN), (1, OTHER), (2, FOREIGN)] {
        let mut f = fixture(&[LINE]);
        let a = anchor(&f);
        f.int(MANAGER + 0x3c0, mode);
        f.word(MANAGER + 0x3b8, pointer);
        f.array(PRODUCTION + 0x70, &[LINE, NEW]);
        assert!(a.observe(&f).is_err());
        assert!(f.untouched(PRODUCTION + 0x70, 16));
        assert!(f.untouched(NEW, 0xa0));
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}

#[test]
fn original_non_queue_edges_fail_before_new_queue_membership_or_payload() {
    let countries = fixture(&[]).get_word(GS + 0x2d8);
    let states = fixture(&[]).get_word(GS + 0x290);
    for field in [
        BASE + 0x3519740,
        BASE + 0x35196c0,
        BASE + 0x34d0de0,
        countries,
        COUNTRY + 0xd10,
        PRODUCTION + 0x290,
        IDLER + 0x670,
        MANAGER + 0x180,
        IDLER + 0x4c0,
        BASE + 0x34d0ac0,
        states + 8,
    ] {
        let mut f = fixture(&[LINE]);
        let a = anchor(&f);
        f.word(field, FOREIGN);
        f.array(PRODUCTION + 0x70, &[LINE, NEW]);
        assert!(a.observe(&f).is_err(), "{field:x}");
        assert!(f.untouched(PRODUCTION + 0x70, 16), "{field:x}");
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}

#[test]
fn requested_state_loss_and_target_catalog_replacement_precede_new_queue_reads() {
    for kind in 0..3 {
        let mut f = fixture(&[LINE]);
        let a = anchor(&f);
        match kind {
            0 => f.int(STATE + 0xc4, 2),
            1 => f.array(DATABASE + 0x40, &[NULL, FOREIGN, OTHER]),
            2 => f.string(TEMPLATE + 0x228, "naval_base"),
            _ => unreachable!(),
        }
        f.array(PRODUCTION + 0x70, &[LINE, NEW]);
        assert!(a.observe(&f).is_err());
        assert!(f.untouched(PRODUCTION + 0x70, 16));
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}

#[test]
fn regressed_or_changed_campaign_and_process_cannot_produce_delta_evidence() {
    for kind in 0..5 {
        let mut f = fixture(&[LINE]);
        let a = anchor(&f);
        match kind {
            0 => f.set_date(DATE - 1),
            1 => f.int(GS + 0x5d0, 1),
            2 => f.int(GS + 0x5d8, 0),
            3 => f.id.start_ticks.push('9'),
            4 => f.set_playthrough("11223344-1122-4344-8566-778899aabbcc"),
            _ => unreachable!(),
        }
        f.array(PRODUCTION + 0x70, &[LINE, NEW]);
        assert!(a.observe(&f).is_err());
        assert!(f.untouched(PRODUCTION + 0x70, 16));
    }
}

#[test]
fn torn_replacement_queue_or_scope_is_neither_pending_nor_success() {
    for (field, size) in [
        (PRODUCTION + 0x70, 16),
        (NEW + 0x48, 4),
        (LINE + 8, 8),
        (STATE + 0xc4, 4),
        (GS + 0x428, 4),
        (DATABASE + 0x40, 16),
        (TEMPLATE + 0x228, 24),
        (MANAGER + 0x3b8, 8),
    ] {
        let mut f = fixture(&[LINE]);
        let a = anchor(&f);
        f.array(PRODUCTION + 0x70, &[LINE, NEW]);
        assert!(a.observe(&f).unwrap().placed());
        let nth = f.read_counts()[&(field, size)];
        f.tear(field, size, nth);
        assert_eq!(a.observe(&f).unwrap_err().code, "torn_read", "{field:x}");
    }
    let mut f = fixture(&[LINE]);
    let a = anchor(&f);
    f.array(PRODUCTION + 0x70, &[LINE, NEW]);
    f.swap_final_identity = true;
    assert_eq!(a.observe(&f).unwrap_err().code, "ownership_changed");
}

#[test]
fn ordinary_selection_anchor_still_refuses_placement_queue_changes() {
    let mut f = fixture(&[LINE]);
    let selection = bind_selection(&f, "arms_factory", 1).unwrap();
    let placement = selection.placement_anchor().unwrap();
    f.array(PRODUCTION + 0x70, &[LINE, NEW]);
    assert_eq!(
        selection.observe(&f).unwrap_err().code,
        "construction_state_changed"
    );
    assert!(placement.observe(&f).unwrap().placed());
    let mut f = fixture(&[LINE]);
    let selection = bind_selection(&f, "arms_factory", 1).unwrap();
    f.int(LINE + 0x48, 3);
    let state = selection.observe(&f).unwrap();
    assert!(selection.same_scope_and_queue(&state).is_err());
    assert!(!selection.is_selected(&state));
}

#[test]
fn private_baseline_retains_original_selected_target_and_complete_queue_fingerprint() {
    let f = fixture(&[LINE, SECOND]);
    let selection = bind_selection(&f, "arms_factory", 1).unwrap();
    let placement = selection.placement_anchor().unwrap();
    let baseline = placement.private_baseline().unwrap();
    assert_eq!(baseline, selection.private_baseline().unwrap());
    assert_eq!(baseline["initial_selection"]["mode"], 1);
    assert_eq!(baseline["scope"]["target"]["pointer"], TEMPLATE);
    assert_eq!(baseline["queue_member_count"], 2);
    assert!(serde_json::to_vec(&baseline).unwrap().len() < 8192);
}
