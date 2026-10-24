use super::*;
use crate::{
    actions::Epoch,
    daily_governor::{Attempt, Observation, OwnCounts, VerifiedReceipt},
    platform::Identity,
    research_stage::{Delivery, Execution, PanelBinding, Receipt, Stage, Status, Target},
};
use std::{cell::Cell, time::Duration};

const PLAYTHROUGH: &str = "11111111-1111-1111-1111-111111111111";
fn scope() -> Scope {
    Scope {
        epoch: Epoch {
            game: Identity {
                pid: 123,
                start_ticks: "456".into(),
                exe: "/fixture/owned-game".into(),
                uid: 1000,
            },
            campaign: format!("fixture:{PLAYTHROUGH}"),
            player: "GER".into(),
        },
        anchors: [0x10000, 0x20000, 0x30000],
        own_country: 0x40000,
        own_status: 0x50000,
    }
}
fn date() -> i32 {
    crate::calendar::encode(1936, 1, 1).unwrap()
}
fn focus() -> Entry {
    Entry {
        id: "focus-goal".into(),
        intent: Intent::Focus {
            focus_id: "GER_rhineland".into(),
        },
        navigation: None,
        focus_search: None,
    }
}
fn production() -> Entry {
    Entry {
        id: "production-goal".into(),
        intent: Intent::Economy {
            target: EconomyTarget::Production {
                line_id: "0x0000000100000038".into(),
                equipment_id: "infantry_equipment_1".into(),
                total_factories: 14,
            },
        },
        navigation: None,
        focus_search: None,
    }
}
fn busy() -> Vec<Option<String>> {
    [
        "electronic_mechanical_engineering",
        "basic_machine_tools",
        "construction1",
        "support_weapons",
    ]
    .into_iter()
    .map(|s| Some(s.into()))
    .collect()
}
fn observed(mode: i32, when: i32, researching: Vec<Option<String>>) -> (OwnState, UiContext) {
    let s = scope();
    let binding = PanelBinding {
        anchors: [
            s.own_country,
            s.own_status,
            0x60000,
            0x70000,
            0x80000,
            0x90000,
        ],
        slots: vec![0xa0000, 0xb0000, 0xc0000, 0xd0000],
        roots: [0xe0000, 0xf0000, 0x100000, 0x110000],
        facilities_parent: 0x120000,
        selected_slot_pointer: 0,
        folder_index: 0,
        folder_windows: vec![],
        tab: 0,
    };
    let ui = UiContext {
        campaign: ClockContext {
            playthrough_id: PLAYTHROUGH.into(),
            player: "GER".into(),
            anchors: s.anchors,
            date_raw: when,
            difficulty: 2,
            historical_raw: 1,
            paused: true,
            speed: 0,
        },
        anchors: binding.anchors,
        slots: binding.slots.clone(),
    };
    let own = OwnState {
        epoch: s.epoch,
        selected_slot: None,
        selected_folder: None,
        tree_visible: false,
        panel_visible: Some(mode == 5),
        active_mode: Some(mode),
        researching,
        visible_info: vec![],
        panel_binding: Some(binding),
    };
    (own, ui)
}
fn snapshot(mode: i32, when: i32, researching: Vec<Option<String>>) -> Snapshot {
    let (own, ui) = observed(mode, when, researching);
    Snapshot::from_observed(&scope(), own, &ui).unwrap()
}
fn bound(entry: Entry, start: Instant) -> Transit {
    Transit::bind_at(
        entry,
        snapshot(2, date(), busy()),
        start + Duration::from_secs(5),
        start,
    )
    .unwrap()
}
fn reach(transit: &mut Transit, entry: &Entry, mode: i32, when: i32, now: Instant) -> Progress {
    transit
        .observe_with(
            &scope(),
            entry,
            |_, _| Ok(snapshot(mode, when, busy())),
            || now,
        )
        .unwrap()
}

#[test]
fn public_wrapper_passes_original_snapshot_and_fixed_deadline_to_owned_reader() {
    // Headroom avoids scheduler noise; this test has no sleep, input or I/O.
    let entry = focus();
    let until = Instant::now() + Duration::from_secs(30);
    let mut transit = Transit::bind(entry.clone(), snapshot(2, date(), busy()), until).unwrap();
    assert_eq!(transit.goal().entry(), &entry);
    assert_eq!(transit.goal().scope(), &scope());
    let progress = transit
        .observe(&scope(), &entry, |deadline, original| {
            assert_eq!(deadline, until);
            assert_eq!(original.scope(), &scope());
            assert_eq!(original.panel(), ManagedPanel::Production);
            assert_eq!(original.context().date_raw, date());
            assert_eq!(
                original.research().panel_binding.as_ref().unwrap().roots,
                [0xe0000, 0xf0000, 0x100000, 0x110000]
            );
            Ok(snapshot(5, date() + 1, busy()))
        })
        .unwrap();
    assert!(matches!(progress, Progress::AtResearch(_)));
    assert_eq!(transit.deadline(), until);
}

#[test]
fn all_busy_slots_allow_both_final_goals_without_a_research_target() {
    for entry in [focus(), production()] {
        let start = Instant::now();
        let mut transit = bound(entry.clone(), start);
        assert_eq!(transit.original_panel(), ManagedPanel::Production);
        assert!(matches!(
            reach(&mut transit, &entry, 23, date(), start),
            Progress::Pending(ManagedPanel::Idle)
        ));
        let Progress::AtResearch(proof) = reach(&mut transit, &entry, 5, date(), start) else {
            panic!("Shown Research expected")
        };
        assert_eq!(proof.goal().entry(), &entry);
        assert_eq!(proof.goal().scope(), &scope());
        assert!(proof
            .observed()
            .research()
            .researching
            .iter()
            .all(Option::is_some));
        assert_eq!(proof.deadline(), start + Duration::from_secs(5));
    }
}

#[test]
fn natural_research_completion_and_new_current_assignments_are_not_goal_dependencies() {
    let start = Instant::now();
    let entry = focus();
    let mut transit = bound(entry.clone(), start);
    let mut slots = busy();
    slots[0] = None;
    let Progress::AtResearch(first) = transit
        .observe_with(
            &scope(),
            &entry,
            |_, _| Ok(snapshot(5, date() + 24, slots.clone())),
            || start,
        )
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(first.observed().research().researching[0], None);
    slots[0] = Some("radio".into());
    slots[1] = None;
    let Progress::AtResearch(second) = transit
        .observe_with(
            &scope(),
            &entry,
            |_, _| Ok(snapshot(5, date() + 48, slots)),
            || start,
        )
        .unwrap()
    else {
        panic!()
    };
    assert!(Arc::ptr_eq(&first.goal, &second.goal));
    assert_eq!(second.goal().entry(), &entry);
    assert_eq!(second.observed().context().date_raw, date() + 48);
}

#[test]
fn original_entry_and_process_scope_are_checked_before_observer_calls() {
    let start = Instant::now();
    let entry = focus();
    let reads = Cell::new(0);
    let mut changes = Vec::new();
    for edit in 0..6 {
        let mut changed = scope();
        match edit {
            0 => changed.epoch.game.start_ticks = "457".into(),
            1 => changed.epoch.game.uid = 2,
            2 => changed.epoch.player = "SOV".into(),
            3 => changed.own_country += 8,
            4 => changed.own_status += 8,
            _ => changed.anchors[1] += 8,
        }
        changes.push(changed);
    }
    for changed in changes {
        let mut transit = bound(entry.clone(), start);
        assert_eq!(
            transit
                .observe_with(
                    &changed,
                    &entry,
                    |_, _| {
                        reads.set(reads.get() + 1);
                        Ok(snapshot(5, date(), busy()))
                    },
                    || start
                )
                .unwrap_err()
                .code,
            "ownership_changed"
        );
    }
    for changed in [
        production(),
        Entry {
            id: "different".into(),
            ..entry.clone()
        },
        Entry {
            intent: Intent::Focus {
                focus_id: "GER_autobahn".into(),
            },
            ..entry.clone()
        },
    ] {
        let mut transit = bound(entry.clone(), start);
        assert!(transit
            .observe_with(
                &scope(),
                &changed,
                |_, _| {
                    reads.set(reads.get() + 1);
                    Ok(snapshot(5, date(), busy()))
                },
                || start
            )
            .is_err());
    }
    assert_eq!(reads.get(), 0);
}

#[test]
fn production_line_equipment_and_allocation_goal_cannot_change_in_transit() {
    let start = Instant::now();
    let entry = production();
    for change in 0..3 {
        let mut current = entry.clone();
        if let Intent::Economy {
            target:
                EconomyTarget::Production {
                    line_id,
                    equipment_id,
                    total_factories,
                },
        } = &mut current.intent
        {
            match change {
                0 => *line_id = "0x0000000100000040".into(),
                1 => *equipment_id = "support_equipment_1".into(),
                _ => *total_factories = 15,
            }
        }
        let mut transit = bound(entry.clone(), start);
        let called = Cell::new(false);
        assert!(transit
            .observe_with(
                &scope(),
                &current,
                |_, _| {
                    called.set(true);
                    Ok(snapshot(5, date(), busy()))
                },
                || start
            )
            .is_err());
        assert!(!called.get());
    }
}

#[test]
fn unknown_or_inconsistent_source_and_visible_foreign_views_refuse() {
    for mode in [-1, 0, 1, 3, 6, 14, 24] {
        let (own, ui) = observed(mode, date(), busy());
        assert_eq!(
            Snapshot::from_observed(&scope(), own, &ui)
                .unwrap_err()
                .code,
            "invalid_panel_transit"
        );
    }
    for change in 0..5 {
        let (mut own, ui) = observed(23, date(), busy());
        match change {
            0 => own.active_mode = None,
            1 => own.panel_visible = Some(true),
            2 => own.tree_visible = true,
            3 => own.visible_info.push(None),
            _ => own.panel_binding.as_mut().unwrap().tab = 1,
        }
        assert!(Snapshot::from_observed(&scope(), own, &ui).is_err());
    }
}

#[test]
fn known_managed_panels_are_explicit_without_claiming_their_visibility() {
    for (mode, panel) in [
        (23, ManagedPanel::Idle),
        (2, ManagedPanel::Production),
        (12, ManagedPanel::Politics),
        (13, ManagedPanel::Focus),
        (5, ManagedPanel::Research),
    ] {
        assert_eq!(snapshot(mode, date(), busy()).panel(), panel);
    }
}

#[test]
fn complete_roots_and_slot_identities_refuse_replacement_or_reordering() {
    let start = Instant::now();
    let entry = focus();
    for change in 0..5 {
        let mut transit = bound(entry.clone(), start);
        let (mut own, mut ui) = observed(5, date(), busy());
        let b = own.panel_binding.as_mut().unwrap();
        match change {
            0 => {
                b.anchors[2] += 8;
                ui.anchors = b.anchors;
            }
            1 => b.roots[0] += 8,
            2 => {
                b.slots.swap(0, 1);
                ui.slots = b.slots.clone();
            }
            3 => {
                b.slots[0] += 8;
                ui.slots = b.slots.clone();
            }
            _ => b.facilities_parent += 8,
        }
        let current = Snapshot::from_observed(&scope(), own, &ui).unwrap();
        assert_eq!(
            transit
                .observe_with(&scope(), &entry, |_, _| Ok(current), || start)
                .unwrap_err()
                .code,
            "ownership_changed"
        );
    }
}

#[test]
fn mismatched_typed_pair_scope_and_incomplete_slot_sets_are_not_current_snapshots() {
    for change in 0..6 {
        let (mut own, mut ui) = observed(5, date(), busy());
        match change {
            0 => ui.anchors[0] += 8,
            1 => ui.slots.swap(0, 1),
            2 => own.panel_binding = None,
            3 => {
                own.researching.pop();
            }
            4 => own.epoch.game.pid += 1,
            _ => own.panel_binding.as_mut().unwrap().slots[0] = 0,
        }
        assert!(Snapshot::from_observed(&scope(), own, &ui).is_err());
    }
}

#[test]
fn setup_playthrough_player_and_campaign_roots_must_match_original_scope() {
    for change in 0..5 {
        let (own, mut ui) = observed(5, date(), busy());
        match change {
            0 => ui.campaign.difficulty = 3,
            1 => ui.campaign.historical_raw = 0,
            2 => ui.campaign.playthrough_id = "different".into(),
            3 => ui.campaign.player = "SOV".into(),
            _ => ui.campaign.anchors[0] += 8,
        }
        assert_eq!(
            Snapshot::from_observed(&scope(), own, &ui)
                .unwrap_err()
                .code,
            "ownership_changed"
        );
    }
}

#[test]
fn dates_may_advance_and_pause_speed_change_but_never_regress() {
    let start = Instant::now();
    let entry = focus();
    let mut transit = bound(entry.clone(), start);
    let (own, mut ui) = observed(5, date() + 48, busy());
    ui.campaign.paused = false;
    ui.campaign.speed = 5;
    let sample = Snapshot::from_observed(&scope(), own, &ui).unwrap();
    assert!(matches!(
        transit
            .observe_with(&scope(), &entry, |_, _| Ok(sample), || start)
            .unwrap(),
        Progress::AtResearch(_)
    ));
    assert_eq!(
        transit
            .observe_with(
                &scope(),
                &entry,
                |_, _| Ok(snapshot(5, date() + 24, busy())),
                || start
            )
            .unwrap_err()
            .code,
        "ownership_changed"
    );
}

#[test]
fn no_research_or_construction_placeholder_can_be_bound_as_final_goal() {
    let start = Instant::now();
    let research = Entry {
        intent: Intent::Research {
            technology_id: "basic_machine_tools".into(),
            slot: 0,
        },
        ..focus()
    };
    let construction = Entry {
        intent: Intent::Economy {
            target: EconomyTarget::ConstructionQueue {
                building: "arms_factory".into(),
                state_id: 64,
                minimum_queued: 1,
            },
        },
        ..focus()
    };
    for entry in [research, construction] {
        assert_eq!(
            Transit::bind_at(
                entry,
                snapshot(23, date(), busy()),
                start + Duration::from_secs(5),
                start
            )
            .unwrap_err()
            .code,
            "invalid_panel_transit"
        );
    }
}

#[test]
fn original_assignment_opener_still_refuses_busy_slot_while_transit_accepts_it() {
    let (own, ui) = observed(23, date(), busy());
    let target = Target {
        epoch: scope().epoch,
        stage: Stage::OpenPanel,
        technology: "unrelated_assignment".into(),
        folder_id: None,
        slot: 0,
    };
    own.validate(&target).unwrap();
    assert_eq!(
        own.preflight(&target).unwrap_err().code,
        "research_precondition"
    );
    Snapshot::from_observed(&scope(), own, &ui).unwrap();
}

#[test]
fn expired_bind_and_expired_followup_never_invoke_a_reader() {
    let start = Instant::now();
    let entry = focus();
    assert_eq!(
        Transit::bind_at(entry.clone(), snapshot(23, date(), busy()), start, start)
            .unwrap_err()
            .code,
        "panel_transit_deadline"
    );
    let mut transit = bound(entry.clone(), start);
    let calls = Cell::new(0);
    assert_eq!(
        transit
            .observe_with(
                &scope(),
                &entry,
                |_, _| {
                    calls.set(calls.get() + 1);
                    Ok(snapshot(5, date(), busy()))
                },
                || start + Duration::from_secs(5)
            )
            .unwrap_err()
            .code,
        "panel_transit_deadline"
    );
    assert_eq!(calls.get(), 0);
}

#[test]
fn late_positive_read_is_discarded_without_advancing_chronology() {
    let start = Instant::now();
    let entry = focus();
    let mut transit = bound(entry.clone(), start);
    let now = Cell::new(start);
    let until = transit.deadline();
    assert_eq!(
        transit
            .observe_with(
                &scope(),
                &entry,
                |supplied, _| {
                    assert_eq!(supplied, until);
                    now.set(until);
                    Ok(snapshot(5, date() + 24, busy()))
                },
                || now.get()
            )
            .unwrap_err()
            .code,
        "panel_transit_deadline"
    );
    assert_eq!(transit.last_date, date());
    assert_eq!(transit.deadline(), until);
}

#[test]
fn late_callback_ownership_failure_keeps_its_concrete_cause() {
    let start = Instant::now();
    let entry = focus();
    let mut transit = bound(entry.clone(), start);
    let now = Cell::new(start);
    let calls = Cell::new(0);
    let until = transit.deadline();
    let error = transit
        .observe_with(
            &scope(),
            &entry,
            |_, _| {
                calls.set(calls.get() + 1);
                now.set(until);
                Err(Error::new(
                    "ownership_changed",
                    "original own status replaced before payload",
                ))
            },
            || now.get(),
        )
        .unwrap_err();
    assert_eq!(error.code, "ownership_changed");
    assert!(error.message.contains("before payload"));
    assert_eq!(calls.get(), 1);
    assert_eq!(transit.last_date, date());
    assert_eq!(transit.deadline(), until);
}

#[test]
fn repeated_reads_do_not_extend_the_deadline_or_retry_a_torn_observation() {
    let start = Instant::now();
    let entry = focus();
    let mut transit = bound(entry.clone(), start);
    let until = transit.deadline();
    let calls = Cell::new(0);
    let e = transit
        .observe_with(
            &scope(),
            &entry,
            |supplied, _| {
                calls.set(calls.get() + 1);
                assert_eq!(supplied, until);
                Err(Error::new("torn_read", "complete snapshot discarded"))
            },
            || start,
        )
        .unwrap_err();
    assert_eq!(e.code, "torn_read");
    assert_eq!(calls.get(), 1);
    assert_eq!(transit.last_date, date());
    for offset in 1..5 {
        assert!(matches!(
            transit
                .observe_with(
                    &scope(),
                    &entry,
                    |supplied, _| {
                        assert_eq!(supplied, until);
                        Ok(snapshot(5, date() + offset, busy()))
                    },
                    || start + Duration::from_secs(offset as u64)
                )
                .unwrap(),
            Progress::AtResearch(_)
        ));
    }
    assert_eq!(transit.deadline(), until);
}

#[test]
fn neither_current_nor_historical_research_panel_receipts_complete_the_final_goal() {
    for entry in [focus(), production()] {
        let start = Instant::now();
        let mut transit = bound(entry.clone(), start);
        let Progress::AtResearch(proof) = reach(&mut transit, &entry, 5, date(), start) else {
            panic!()
        };
        for historical in [false, true] {
            let epoch = scope().epoch;
            let execution = Execution {
                receipt: Receipt {
                    version: 1,
                    request_id: "goal-attempt".into(),
                    target: Target {
                        epoch,
                        stage: Stage::OpenPanel,
                        technology: "unrelated_assignment".into(),
                        folder_id: None,
                        slot: 0,
                    },
                    status: Status::Verified,
                    delivery: Some(Delivery::ServerConfirmed),
                    observed: proof.observed().research().clone(),
                },
                historical,
                current_state_verified: !historical,
                event_causality_verified: false,
            };
            let attempt = Attempt {
                request_id: "goal-attempt".into(),
                entry: proof.goal().entry().clone(),
                scope: scope(),
                day: 706640,
                before_sequence: 1,
            };
            let observed = Observation {
                scope: scope(),
                date_raw: date(),
                sequence: 2,
                observed_unix_ms: 10,
                source: "fixture current own producer".into(),
                counts: OwnCounts::default(),
            };
            assert_eq!(
                VerifiedReceipt::from_research(attempt, &execution, observed)
                    .unwrap_err()
                    .code,
                "receipt_mismatch"
            );
        }
        assert_eq!(proof.goal().entry(), &entry);
    }
}
