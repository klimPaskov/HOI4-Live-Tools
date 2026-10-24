//! Actual sparse canonical observers compose these proofs; no proof builders,
//! JSON receipt input, UI payload, process attachment, clocks or key dispatch.
use super::*;

fn inactive(f: &mut Fixture) {
    f.int(MANAGER + 0x3c0, 0);
}
fn select(f: &mut Fixture) {
    f.int(MANAGER + 0x3c0, 1);
    f.word(MANAGER + 0x3b8, TEMPLATE);
}
fn original(f: &mut Fixture) -> SelectionAnchor {
    inactive(f);
    bind_selection(f, "arms_factory", 1).unwrap()
}

#[test]
fn inactive_is_pending_then_exact_selection_promotes_the_original_full_queue() {
    let mut f = fixture(&[LINE, SECOND]);
    let before = original(&mut f);
    assert!(before.observe_selected(&f).unwrap().is_none());
    select(&mut f);
    f.set_date(DATE + 1);
    let proof = before.observe_selected(&f).unwrap().unwrap();
    assert_eq!(proof.original_anchor().context().date_raw, DATE);
    assert_eq!(proof.context().date_raw, DATE + 1);
    assert!(proof.original_anchor().require_inactive().is_ok());
    assert!(proof.selected_anchor().initially_selected());
    let current = proof.selected_anchor().observe(&f).unwrap();
    proof
        .original_anchor()
        .same_scope_and_queue(&current)
        .unwrap();
    assert!(before.require_unchanged(&current).is_err());
    assert!(proof.selected_anchor().placement_anchor().is_ok());
    assert!(f.untouched(MANAGER + 0x150, 8));
}

#[test]
fn selected_original_cannot_be_relabelled_as_an_inactive_to_selected_transition() {
    let f = fixture(&[LINE]);
    let before = bind_selection(&f, "arms_factory", 1).unwrap();
    let snapshots = f.snapshots.get();
    assert_eq!(
        before.observe_selected(&f).unwrap_err().code,
        "construction_selection_unavailable"
    );
    assert_eq!(f.snapshots.get(), snapshots);
}

#[test]
fn other_target_or_special_selection_never_becomes_pending_or_selected_proof() {
    for (mode, target) in [(1, OTHER), (1, FOREIGN), (2, FOREIGN)] {
        let mut f = fixture(&[LINE]);
        let before = original(&mut f);
        f.int(MANAGER + 0x3c0, mode);
        f.word(MANAGER + 0x3b8, target);
        assert!(before.observe_selected(&f).is_err());
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}

#[test]
fn original_queue_members_and_amounts_cannot_be_adopted_at_selection_promotion() {
    for change_members in [false, true] {
        let mut f = fixture(&[LINE, SECOND]);
        let before = original(&mut f);
        select(&mut f);
        if change_members {
            f.array(PRODUCTION + 0x70, &[LINE, NEW]);
        } else {
            f.int(LINE + 0x48, 3);
        }
        assert_eq!(
            before.observe_selected(&f).unwrap_err().code,
            "construction_state_changed"
        );
        if change_members {
            assert!(f.untouched(NEW, 0xa0));
        }
    }
}

#[test]
fn replacement_scope_or_same_named_catalog_target_refuses_before_new_payload() {
    for at in [IDLER + 0x670, COUNTRY + 0xd10, BASE + 0x34d0ac0] {
        let mut f = fixture(&[LINE]);
        let before = original(&mut f);
        select(&mut f);
        f.word(at, FOREIGN);
        assert!(before.observe_selected(&f).is_err());
        assert!(f.untouched(FOREIGN, 0x1000));
        assert!(f.untouched(PRODUCTION + 0x70, 16));
    }
    let mut f = fixture(&[LINE]);
    let before = original(&mut f);
    select(&mut f);
    template(&mut f, FOREIGN, 1, "arms_factory");
    f.array(DATABASE + 0x40, &[NULL, FOREIGN, OTHER]);
    f.word(MANAGER + 0x3b8, FOREIGN);
    assert!(before.observe_selected(&f).is_err());
    assert!(f.untouched(FOREIGN, 0x300));
}

#[test]
fn selected_transition_still_requires_final_witness_and_exact_process() {
    for final_identity in [false, true] {
        let mut f = fixture(&[LINE]);
        let before = original(&mut f);
        select(&mut f);
        if final_identity {
            f.swap_final_identity = true;
        } else {
            before.observe_selected(&f).unwrap().unwrap();
            let nth = f.read_counts()[&(MANAGER + 0x3c0, 4)];
            f.tear(MANAGER + 0x3c0, 4, nth);
        }
        assert_eq!(
            before.observe_selected(&f).unwrap_err().code,
            if final_identity {
                "ownership_changed"
            } else {
                "torn_read"
            }
        );
    }
}

#[test]
fn pending_placement_cannot_promote_and_proof_accessors_add_no_reads() {
    let f = fixture(&[LINE, SECOND]);
    let a = anchor(&f);
    let readback = a.observe(&f).unwrap();
    let reads = f.read_counts();
    let snapshots = f.snapshots.get();
    assert_eq!(readback.delta(), PlacementDelta::Pending);
    assert!(!readback.placed());
    assert!(readback.proof().is_none());
    assert_eq!(f.read_counts(), reads);
    assert_eq!(f.snapshots.get(), snapshots);
}

#[test]
fn each_exact_plus_one_form_retains_post_queue_and_seeds_clear_without_rebinding() {
    for new_line in [false, true] {
        let mut f = fixture(&[LINE, SECOND]);
        let a = anchor(&f);
        if new_line {
            f.array(PRODUCTION + 0x70, &[LINE, NEW, SECOND]);
        } else {
            f.int(LINE + 0x48, 3);
        }
        f.set_date(DATE + 2);
        let readback = a.observe(&f).unwrap();
        let reads = f.read_counts();
        let snapshots = f.snapshots.get();
        let proof = readback.proof().unwrap();
        assert_eq!(
            proof.delta(),
            if new_line {
                PlacementDelta::NewLineAdded
            } else {
                PlacementDelta::ExistingLineIncremented
            }
        );
        assert_eq!(proof.original_anchor().context().date_raw, DATE);
        assert_eq!(proof.context().date_raw, DATE + 2);
        assert!(proof.placed_anchor().initially_selected());
        let clear = proof.clear_anchor().unwrap();
        assert!(!clear.initially_inactive());
        assert_eq!(f.read_counts(), reads);
        assert_eq!(f.snapshots.get(), snapshots);
        assert!(clear.observe_proof(&f).unwrap().is_none());
        inactive(&mut f);
        let cleared = clear.observe_proof(&f).unwrap().unwrap();
        assert!(cleared.cleared_anchor().require_inactive().is_ok());
    }
}

#[test]
fn old_queue_anchor_cannot_clear_after_placement_but_exact_placement_proof_can() {
    let mut f = fixture(&[LINE, SECOND]);
    let selected = bind_selection(&f, "arms_factory", 1).unwrap();
    let old_clear = selected.clear_selection_anchor().unwrap();
    let a = selected.placement_anchor().unwrap();
    f.int(LINE + 0x48, 3);
    let proof = a.observe(&f).unwrap().proof().unwrap();
    inactive(&mut f);
    assert_eq!(
        old_clear.observe_proof(&f).unwrap_err().code,
        "construction_state_changed"
    );
    let cleared = proof
        .clear_anchor()
        .unwrap()
        .observe_proof(&f)
        .unwrap()
        .unwrap();
    let state = cleared.cleared_anchor().observe(&f).unwrap();
    proof.placed_anchor().same_scope_and_queue(&state).unwrap();
    assert!(proof
        .original_anchor()
        .same_scope_and_queue(&state)
        .is_err());
}

#[test]
fn cleared_proof_is_current_inactive_with_exact_proven_queue_and_context() {
    let mut f = fixture(&[LINE, SECOND]);
    let a = anchor(&f);
    f.array(PRODUCTION + 0x70, &[LINE, SECOND, NEW]);
    let placed = a.observe(&f).unwrap().proof().unwrap();
    let clear = placed.clear_anchor().unwrap();
    f.set_date(DATE + 3);
    inactive(&mut f);
    f.word(MANAGER + 0x3b8, FOREIGN);
    let proof = clear.observe_proof(&f).unwrap().unwrap();
    assert_eq!(proof.context().date_raw, DATE + 3);
    assert_eq!(proof.original_anchor().context().date_raw, DATE);
    assert!(proof.original_anchor().initially_selected());
    proof.cleared_anchor().require_inactive().unwrap();
    assert!(f.untouched(MANAGER + 0x3b8, 8));
    assert!(f.untouched(FOREIGN, 8));
    let snapshots = f.snapshots.get();
    let reads = f.read_counts();
    assert_eq!(proof.context().date_raw, DATE + 3);
    proof.cleared_anchor().require_inactive().unwrap();
    assert_eq!(f.read_counts(), reads);
    assert_eq!(f.snapshots.get(), snapshots);
}

#[test]
fn cleared_proof_refuses_lost_or_changed_expected_post_placement_rows() {
    for replace_members in [false, true] {
        let mut f = fixture(&[LINE, SECOND]);
        let a = anchor(&f);
        f.array(PRODUCTION + 0x70, &[LINE, SECOND, NEW]);
        let clear = a
            .observe(&f)
            .unwrap()
            .proof()
            .unwrap()
            .clear_anchor()
            .unwrap();
        inactive(&mut f);
        if replace_members {
            f.array(PRODUCTION + 0x70, &[LINE, SECOND]);
        } else {
            f.int(NEW + 0x48, 2);
        }
        assert_eq!(
            clear.observe_proof(&f).unwrap_err().code,
            "construction_state_changed"
        );
        if replace_members {
            assert!(f.untouched(NEW, 0xa0));
        }
    }
}

#[test]
fn changed_post_placement_selection_is_not_cleared_or_pending() {
    let mut f = fixture(&[LINE]);
    let a = anchor(&f);
    f.int(LINE + 0x48, 3);
    let clear = a
        .observe(&f)
        .unwrap()
        .proof()
        .unwrap()
        .clear_anchor()
        .unwrap();
    f.word(MANAGER + 0x3b8, OTHER);
    assert_eq!(
        clear.observe_proof(&f).unwrap_err().code,
        "construction_state_changed"
    );
}

#[test]
fn final_cleared_witness_and_identity_loss_discard_the_proof() {
    for final_identity in [false, true] {
        let mut f = fixture(&[LINE]);
        let a = anchor(&f);
        f.int(LINE + 0x48, 3);
        let clear = a
            .observe(&f)
            .unwrap()
            .proof()
            .unwrap()
            .clear_anchor()
            .unwrap();
        inactive(&mut f);
        if final_identity {
            f.swap_final_identity = true;
        } else {
            clear.observe_proof(&f).unwrap().unwrap();
            let nth = f.read_counts()[&(LINE + 0x48, 4)];
            f.tear(LINE + 0x48, 4, nth);
        }
        assert_eq!(
            clear.observe_proof(&f).unwrap_err().code,
            if final_identity {
                "ownership_changed"
            } else {
                "torn_read"
            }
        );
    }
}

#[test]
fn inactive_zero_input_clear_proof_still_rejects_later_selection() {
    let mut f = fixture(&[LINE]);
    let original = original(&mut f);
    let clear = original.clear_selection_anchor().unwrap();
    assert!(clear.initially_inactive());
    assert!(clear.observe_proof(&f).unwrap().is_some());
    select(&mut f);
    assert_eq!(
        clear.observe_proof(&f).unwrap_err().code,
        "construction_state_changed"
    );
}

#[test]
fn retained_handoffs_never_adopt_later_replacement_scope_or_regressed_context() {
    for which in 0..4 {
        let mut f = fixture(&[LINE]);
        let a = anchor(&f);
        f.int(LINE + 0x48, 3);
        let proof = a.observe(&f).unwrap().proof().unwrap();
        let clear = proof.clear_anchor().unwrap();
        inactive(&mut f);
        match which {
            0 => f.word(IDLER + 0x670, FOREIGN),
            1 => f.id.start_ticks.push('9'),
            2 => f.set_date(DATE - 1),
            _ => f.set_playthrough("11223344-1122-4344-8566-778899aabbcc"),
        }
        assert!(clear.observe_proof(&f).is_err());
        assert!(f.untouched(FOREIGN, 0x1000));
        if which < 2 {
            assert!(f.untouched(PRODUCTION + 0x70, 16));
        } else {
            // Existing original_edges checks known membership before context;
            // chronology still refuses before any queue row payload.
            assert!(f.untouched(LINE, 0xa0));
        }
        assert_eq!(proof.context().date_raw, DATE);
    }
}

#[test]
fn incomplete_or_excess_delta_never_exposes_a_handoff_proof() {
    for which in 0..3 {
        let mut f = fixture(&[LINE]);
        let a = anchor(&f);
        match which {
            0 => f.int(LINE + 0x48, 4),
            1 => {
                f.array(PRODUCTION + 0x70, &[LINE, NEW]);
                f.word(NEW + 0x70, 0);
            }
            _ => {
                f.array(PRODUCTION + 0x70, &[LINE, NEW]);
                f.int(NEW + 0x48, 2);
            }
        }
        assert!(a.observe(&f).is_err());
    }
}

#[test]
fn full_transition_chain_preserves_each_original_scope_and_only_the_proved_queue_delta() {
    let mut f = fixture(&[LINE, SECOND]);
    let original = original(&mut f);
    select(&mut f);
    f.set_date(DATE + 1);
    let selected = original.observe_selected(&f).unwrap().unwrap();
    let placement = selected.selected_anchor().placement_anchor().unwrap();
    f.int(LINE + 0x48, 3);
    f.set_date(DATE + 2);
    let placed = placement.observe(&f).unwrap().proof().unwrap();
    selected
        .selected_anchor()
        .require_same_initial(placed.original_anchor())
        .unwrap();
    let clear = placed.clear_anchor().unwrap();
    assert!(clear.observe_proof(&f).unwrap().is_none());
    inactive(&mut f);
    f.set_date(DATE + 3);
    let cleared = clear.observe_proof(&f).unwrap().unwrap();
    placed
        .placed_anchor()
        .require_same_initial(cleared.original_anchor())
        .unwrap();
    cleared.cleared_anchor().require_inactive().unwrap();
    let final_state = cleared.cleared_anchor().observe(&f).unwrap();
    assert!(original.same_scope_and_queue(&final_state).is_err());
    placed
        .placed_anchor()
        .same_scope_and_queue(&final_state)
        .unwrap();
    assert_eq!(selected.original_anchor().context().date_raw, DATE);
    assert_eq!(selected.context().date_raw, DATE + 1);
    assert_eq!(placed.context().date_raw, DATE + 2);
    assert_eq!(cleared.context().date_raw, DATE + 3);
}

#[test]
fn old_clear_enum_and_new_proof_keep_identical_reads_and_refusal_messages() {
    let mut f = fixture(&[LINE]);
    let clear = bind_selection(&f, "arms_factory", 1)
        .unwrap()
        .clear_selection_anchor()
        .unwrap();
    for pending in [true, false] {
        if pending {
            select(&mut f);
        } else {
            inactive(&mut f);
        }
        let old = clear.observe(&f).unwrap();
        let counts = f.read_counts();
        let proof = clear.observe_proof(&f).unwrap();
        assert_eq!(proof.is_some(), !pending);
        assert_eq!(
            old,
            if pending {
                super::super::super::ClearSelectionReadback::Pending
            } else {
                super::super::super::ClearSelectionReadback::Cleared
            }
        );
        assert_eq!(f.read_counts(), counts);
    }
    select(&mut f);
    f.word(MANAGER + 0x3b8, OTHER);
    let old = clear.observe(&f).unwrap_err();
    let counts = f.read_counts();
    let new = clear.observe_proof(&f).unwrap_err();
    assert_eq!((old.code, old.message), (new.code, new.message));
    assert_eq!(f.read_counts(), counts);
}
