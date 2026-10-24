use super::*;
const UPDATE_DATA: u64 = 0x3f0000;

fn pending() -> (Fixture, Anchor) {
    let mut f = Fixture::new(23);
    let (anchor, _) = f.bind(ManagedPanel::Idle).unwrap();
    f.mode(5);
    f.put(PANEL + 0xeb0, &[0; 0x80]);
    f.put(PANEL + 0xe58, &1.0_f32.to_le_bytes());
    f.put(PANEL + 0xeb4, &0.2_f32.to_le_bytes());
    f.put(PANEL + 0xec4, &0.3_f32.to_le_bytes());
    f.put(PANEL + 0xed0, &0.1_f32.to_le_bytes());
    f.number(PANEL + 0xef8, 1);
    f.number(PANEL + 0xefc, 2);
    f.array(REGISTRY + 0x188, UPDATE_DATA, &[PANEL + 0x30]);
    (f, anchor)
}

#[test]
fn witnessed_pending_diagnostic_is_neither_snapshot_nor_animation_completion() {
    let (f, mut anchor) = pending();
    assert_eq!(
        f.observe(&mut anchor).unwrap_err().code,
        "research_panel_transition"
    );
    f.reads.borrow_mut().clear();
    let date = anchor.last_date;
    let sample = anchor.animation_with_clock(&f, &|| f.clock.get()).unwrap();
    assert!(sample.literal_shown);
    assert_eq!(sample.update_registration.entries, 1);
    assert_eq!(sample.update_registration.original_root_matches, 1);
    assert_eq!(sample.mode_raw, 5);
    assert_eq!(sample.scalar.state, 0);
    assert_eq!(sample.position.state, 1);
    assert_eq!(sample.position.elapsed, 0.1);
    assert_eq!(sample.position.active_duration, 0.3);
    assert!(!sample.position.completed);
    assert_eq!(sample.position.setter_flags, [false, false]);
    assert_eq!(anchor.last_date, date);
    assert_eq!(anchor.deadline(), f.until);
    assert!(f
        .reads
        .borrow()
        .iter()
        .any(|(p, n)| *p == PANEL + 0xeb0 && *n == 0x80));
    assert!(!f
        .reads
        .borrow()
        .iter()
        .any(|(p, _)| [FACILITIES, TREE_ROOT].contains(p)));
    assert_eq!(
        f.observe(&mut anchor).unwrap_err().code,
        "research_panel_transition"
    );
}

#[test]
fn elapsed_and_snap_flag_can_change_without_clearing_the_original_pending_state() {
    let (mut f, mut anchor) = pending();
    for (elapsed, complete) in [(0.1, false), (0.5, true)] {
        f.put(PANEL + 0xed0, &f32::to_le_bytes(elapsed));
        f.put(PANEL + 0xf2c, &[u8::from(complete)]);
        let sample = anchor.animation_with_clock(&f, &|| f.clock.get()).unwrap();
        assert_eq!(sample.position.elapsed, elapsed);
        assert_eq!(sample.position.completed, complete);
        assert_eq!(sample.position.state, 1);
        assert_eq!(
            f.observe(&mut anchor).unwrap_err().code,
            "research_panel_transition"
        );
    }
}

#[test]
fn expired_original_bound_and_wrong_process_or_abi_read_no_memory() {
    for change in 0..4 {
        let (mut f, anchor) = pending();
        match change {
            0 => f.clock.set(f.until),
            1 => f.id.start_ticks = "999".into(),
            2 => f.image += 8,
            _ => f.abi = StringAbi::LibCxx,
        }
        f.reads.borrow_mut().clear();
        assert!(anchor.animation_with_clock(&f, &|| f.clock.get()).is_err());
        assert!(f.reads.borrow().is_empty());
    }
}

#[test]
fn replaced_owners_stop_before_original_or_foreign_animation_payload() {
    for address in [
        IDLER + 0x4f8,
        IDLER + 0x6b8,
        MANAGER + 0xf0,
        MANAGER + 0xf8,
        VIEW + 0x2f8,
    ] {
        let (mut f, anchor) = pending();
        f.word(address, POISON);
        f.reads.borrow_mut().clear();
        assert_eq!(
            anchor
                .animation_with_clock(&f, &|| f.clock.get())
                .unwrap_err()
                .code,
            "ownership_changed"
        );
        assert!(!f
            .reads
            .borrow()
            .iter()
            .any(|(p, _)| *p >= POISON && *p < POISON + 0x1000));
        assert!(!f.reads.borrow().iter().any(|(p, _)| *p == PANEL + 0xeb0));
    }
}

#[test]
fn root_type_registry_view_owner_and_foreign_display_refuse_before_fields() {
    for address in [PANEL, PANEL + 0x98, VIEW + 0x2f0, TREE + 0x2f0] {
        let (mut f, anchor) = pending();
        f.word(address, POISON);
        f.reads.borrow_mut().clear();
        assert!(anchor.animation_with_clock(&f, &|| f.clock.get()).is_err());
        assert!(!f.reads.borrow().iter().any(|(p, _)| *p == PANEL + 0xeb0));
    }
    let (mut f, anchor) = pending();
    f.number(TREE + 0xe20, 999);
    f.reads.borrow_mut().clear();
    assert_eq!(
        anchor
            .animation_with_clock(&f, &|| f.clock.get())
            .unwrap_err()
            .code,
        "ownership_changed"
    );
    assert!(!f.reads.borrow().iter().any(|(p, _)| *p == PANEL + 0xeb0));
}

#[test]
fn campaign_player_date_and_setup_cannot_be_changed_for_diagnostics() {
    for change in 0..4 {
        let (mut f, anchor) = pending();
        match change {
            0 => f.string(GS + 0x718, "bbbbbbbb-1122-4344-8566-778899aabbcc", 0x50000),
            1 => f.number(GS + 0x460, anchor.last_date - 1),
            2 => f.number(GS + 0x620, 3),
            _ => f.number(GS + 0x628, 0),
        }
        f.reads.borrow_mut().clear();
        assert!(anchor.animation_with_clock(&f, &|| f.clock.get()).is_err());
        assert!(!f.reads.borrow().iter().any(|(p, _)| *p == PANEL + 0xeb0));
    }
}

#[test]
fn unknown_state_nonfinite_numeric_and_nonboolean_flags_never_produce_a_sample() {
    for address in [
        PANEL + 0xeb0,
        PANEL + 0xef8,
        PANEL + 0xf2c,
        PANEL + 0xf2d,
        PANEL + 0xf2e,
    ] {
        let (mut f, anchor) = pending();
        f.put(address, &[3]);
        assert_eq!(
            anchor
                .animation_with_clock(&f, &|| f.clock.get())
                .unwrap_err()
                .code,
            "research_animation_unknown"
        );
    }
    for address in [
        PANEL + 0xe58,
        PANEL + 0xeb4,
        PANEL + 0xeb8,
        PANEL + 0xec0,
        PANEL + 0xec4,
        PANEL + 0xed0,
    ] {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let (mut f, anchor) = pending();
            f.put(address, &value.to_le_bytes());
            assert_eq!(
                anchor
                    .animation_with_clock(&f, &|| f.clock.get())
                    .unwrap_err()
                    .code,
                "research_animation_unknown"
            );
        }
    }
}

#[test]
fn final_witness_covers_owners_visibility_scalar_and_every_position_field() {
    for address in [
        VIEW + 0x2f8,
        PANEL + 0x98,
        PANEL + 0xa5,
        PANEL + 0xe58,
        PANEL + 0xeb0,
        PANEL + 0xec4,
        PANEL + 0xed0,
        PANEL + 0xef8,
        PANEL + 0xf2c,
    ] {
        let (mut f, anchor) = pending();
        f.mutate_finish = Some(address);
        assert_eq!(
            anchor
                .animation_with_clock(&f, &|| f.clock.get())
                .unwrap_err()
                .code,
            "torn_read"
        );
    }
}

#[test]
fn original_deadline_covers_field_read_final_witness_and_process_check() {
    let (mut f, anchor) = pending();
    f.late_at = Some(PANEL + 0xeb0);
    assert_eq!(
        anchor
            .animation_with_clock(&f, &|| f.clock.get())
            .unwrap_err()
            .code,
        "panel_transit_deadline"
    );
    let (mut f, anchor) = pending();
    f.late_finish = true;
    assert_eq!(
        anchor
            .animation_with_clock(&f, &|| f.clock.get())
            .unwrap_err()
            .code,
        "panel_transit_deadline"
    );
    let (mut f, anchor) = pending();
    f.fail_after_check = Some(f.checks.get() + 1);
    assert_eq!(
        anchor
            .animation_with_clock(&f, &|| f.clock.get())
            .unwrap_err()
            .code,
        "ownership_changed"
    );
}

#[test]
fn concrete_read_failure_and_short_fields_never_return_partial_diagnostics() {
    let (mut f, anchor) = pending();
    f.error_at = Some(PANEL + 0xeb0);
    assert_eq!(
        anchor
            .animation_with_clock(&f, &|| f.clock.get())
            .unwrap_err()
            .code,
        "ownership_changed"
    );
    let (mut f, anchor) = pending();
    f.short_at = Some(PANEL + 0xeb0);
    assert_eq!(
        anchor
            .animation_with_clock(&f, &|| f.clock.get())
            .unwrap_err()
            .code,
        "short_read"
    );
}

#[test]
fn original_slot_header_and_membership_precede_canonical_slot_payload() {
    for address in [STATUS + 0xa0, SLOT_DATA] {
        let (mut f, anchor) = pending();
        f.word(address, POISON);
        f.reads.borrow_mut().clear();
        assert_eq!(
            anchor
                .animation_with_clock(&f, &|| f.clock.get())
                .unwrap_err()
                .code,
            "ownership_changed"
        );
        assert!(!f
            .reads
            .borrow()
            .iter()
            .any(|(p, _)| *p >= POISON && *p < POISON + 0x1000));
        assert!(!f
            .reads
            .borrow()
            .iter()
            .any(|(p, _)| *p == SLOT || *p == PANEL + 0xeb0));
    }
}

#[test]
fn literal_gui_flags_are_retained_without_admitting_an_update_or_input() {
    let (mut f, mut anchor) = pending();
    f.put(PANEL + 0xa5, &[0x18]);
    let sample = anchor.animation_with_clock(&f, &|| f.clock.get()).unwrap();
    assert_eq!(sample.gui_flags_raw, 0x18);
    assert!(sample.literal_shown);
    assert_eq!(sample.position.state, 1);
    assert_eq!(
        f.observe(&mut anchor).unwrap_err().code,
        "research_panel_transition"
    );
}

#[test]
fn own_model_membership_and_forward_owners_precede_canonical_payload() {
    for address in [
        DB + 0x48,
        STATUS + 0x88,
        SLOT + 0x18,
        TECHNOLOGY + 0x158,
        TECHNOLOGY + 0x160,
    ] {
        let (mut f, anchor) = pending();
        f.word(address, POISON);
        f.reads.borrow_mut().clear();
        assert_eq!(
            anchor
                .animation_with_clock(&f, &|| f.clock.get())
                .unwrap_err()
                .code,
            "ownership_changed"
        );
        assert!(!f
            .reads
            .borrow()
            .iter()
            .any(|(p, _)| *p >= POISON && *p < POISON + 0x1000));
        assert!(!f.reads.borrow().iter().any(|(p, _)| *p == PANEL + 0xeb0));
    }
}

#[test]
fn registry_membership_is_current_admission_only_and_no_other_entry_is_followed() {
    let (mut f, mut anchor) = pending();
    for pointers in [
        vec![],
        vec![PANEL + 0x30],
        vec![PANEL + 0x30, TREE_ROOT + 0x30, PANEL + 0x30],
    ] {
        f.array(REGISTRY + 0x188, UPDATE_DATA, &pointers);
        f.reads.borrow_mut().clear();
        let sample = anchor.animation_with_clock(&f, &|| f.clock.get()).unwrap();
        assert_eq!(sample.update_registration.entries, pointers.len());
        assert_eq!(
            sample.update_registration.original_root_matches,
            pointers.iter().filter(|p| **p == PANEL + 0x30).count()
        );
        assert_eq!(sample.position.state, 1);
        assert!(!f
            .reads
            .borrow()
            .iter()
            .any(|(p, _)| *p == TREE_ROOT + 0x30 || *p == TREE_ROOT + 0xa5));
        assert_eq!(
            f.observe(&mut anchor).unwrap_err().code,
            "research_panel_transition"
        );
    }
}

#[test]
fn overbound_or_malformed_update_census_is_unknown_not_zero_admission() {
    let (mut f, anchor) = pending();
    f.word(REGISTRY + 0x188, POISON);
    f.number(REGISTRY + 0x190, 4097);
    f.reads.borrow_mut().clear();
    assert!(anchor.animation_with_clock(&f, &|| f.clock.get()).is_err());
    assert!(!f
        .reads
        .borrow()
        .iter()
        .any(|(p, _)| *p >= POISON && *p < POISON + 0x1000));
    for value in [0, 0x10001] {
        let (mut f, anchor) = pending();
        f.word(UPDATE_DATA, value);
        assert!(anchor.animation_with_clock(&f, &|| f.clock.get()).is_err());
    }
}

#[test]
fn registry_header_and_pointer_census_are_retained_through_the_final_witness() {
    for address in [
        REGISTRY + 0x188,
        REGISTRY + 0x190,
        REGISTRY + 0x194,
        UPDATE_DATA,
    ] {
        let (mut f, anchor) = pending();
        f.mutate_finish = Some(address);
        assert_eq!(
            anchor
                .animation_with_clock(&f, &|| f.clock.get())
                .unwrap_err()
                .code,
            "torn_read"
        );
    }
}
