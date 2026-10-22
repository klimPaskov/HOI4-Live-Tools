use super::*;

fn close_target() -> Target {
    Target {
        technology: String::new(),
        ..target(Stage::ClosePanel)
    }
}
fn shown() -> OwnState {
    let mut state = panel_state();
    state.selected_slot = Some(1);
    state.selected_folder = Some("industry_folder".into());
    state.researching[2] = Some("support_weapons".into());
    state.panel_binding = Some(PanelBinding {
        anchors: [0x10000, 0x20000, 0x30000, 0x40000, 0x50000, 0x60000],
        slots: vec![0x70000, 0x80000, 0x90000, 0xa0000],
        roots: [0xb0000, 0xc0000, 0xd0000, 0xe0000],
        facilities_parent: 0x110000,
        selected_slot_pointer: 0x80000,
        folder_index: 1,
        folder_windows: vec![0xf0000, 0x100000],
        tab: 0,
    });
    state
}
fn closed() -> OwnState {
    OwnState {
        active_mode: Some(23),
        panel_visible: Some(false),
        ..shown()
    }
}
fn closer(states: Vec<OwnState>) -> Fake {
    let mut fake = Fake::new();
    fake.candidate = Candidate::ClosePanel;
    fake.delivery = Delivery::ServerConfirmed;
    fake.last = shown();
    fake.observations = states.into_iter().map(Ok).collect();
    fake
}

#[test]
fn close_exact_original_state_is_durable_and_public_receipt_redacts_only_binding() {
    let scratch = Scratch::new();
    let a = authority();
    let gate = RwLock::new(true);
    let mut journal = Journal::open(&scratch.journal()).unwrap();
    let mut fake = closer(vec![shown(), shown(), shown(), closed()]);
    let execution = execute(
        &mut journal,
        "close",
        close_target(),
        &a,
        &gate,
        &mut fake,
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(fake.dispatches, 1);
    assert_eq!(execution.receipt.observed, closed());
    let public = serde_json::to_value(&execution).unwrap();
    assert!(public["receipt"]["observed"].get("panel_binding").is_none());
    let private = serde_json::to_value(&execution.receipt).unwrap();
    assert_eq!(private["observed"]["panel_binding"]["roots"][0], 0xb0000);
    drop(journal);
    let persisted = fs::read_to_string(scratch.journal()).unwrap();
    assert!(persisted.contains("panel_binding"));
    let mut journal = Journal::open(&scratch.journal()).unwrap();
    // A historical close must neither observe nor toggle a subsequently reopened panel.
    fake.last = shown();
    let observations = fake.calls;
    let replay = execute(
        &mut journal,
        "close",
        close_target(),
        &a,
        &gate,
        &mut fake,
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(replay.historical && !replay.current_state_verified);
    assert_eq!(fake.calls, observations);
    assert_eq!(fake.dispatches, 1);
}

#[test]
fn close_noop_requires_complete_closed_original_proof_and_does_not_dispatch() {
    let scratch = Scratch::new();
    let mut journal = Journal::open(&scratch.journal()).unwrap();
    let mut fake = closer(vec![closed()]);
    let execution = execute(
        &mut journal,
        "noop",
        close_target(),
        &authority(),
        &RwLock::new(true),
        &mut fake,
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(execution.current_state_verified && !execution.historical);
    assert_eq!(execution.receipt.delivery, None);
    assert_eq!(fake.dispatches, 0);
    drop(journal);
    Journal::open(&scratch.journal()).unwrap();
    let mut incomplete = closed();
    incomplete.panel_binding = None;
    assert!(incomplete.validate(&close_target()).is_err());
    assert!(!incomplete.reached(&close_target()));
}

#[test]
fn close_known_transition_retries_only_until_exact_readback_and_timeout_remains_quarantined() {
    for finish in [false, true] {
        let scratch = Scratch::new();
        let mut journal = Journal::open(&scratch.journal()).unwrap();
        let mut fake = closer(vec![]);
        fake.observations = VecDeque::from([
            Ok(shown()),
            Ok(shown()),
            Err(error(
                "research_panel_transition",
                "Known ordinary close animation",
            )),
        ]);
        if finish {
            fake.observations.push_back(Ok(closed()));
        } else {
            for _ in 0..20 {
                fake.observations.push_back(Err(error(
                    "research_panel_transition",
                    "Known ordinary close animation remains unfinished",
                )));
            }
        }
        let result = execute(
            &mut journal,
            "transition",
            close_target(),
            &authority(),
            &RwLock::new(true),
            &mut fake,
            Duration::from_millis(200),
        );
        if finish {
            assert!(result.unwrap().current_state_verified);
            assert!(!journal.blocks_game(&epoch().game));
        } else {
            assert_eq!(result.unwrap_err().code, "action_uncertain");
            assert!(journal.blocks_game(&epoch().game));
        }
        assert_eq!(fake.dispatches, 1);
    }
    let scratch = Scratch::new();
    let mut journal = Journal::open(&scratch.journal()).unwrap();
    let mut fake = Fake::new();
    fake.observations = VecDeque::from([
        Ok(state()),
        Ok(state()),
        Err(error(
            "research_panel_transition",
            "Only close stages may retry this",
        )),
        Ok(reached()),
    ]);
    assert_eq!(
        execute(
            &mut journal,
            "other-stage",
            target(Stage::Research),
            &authority(),
            &RwLock::new(true),
            &mut fake,
            Duration::from_secs(2)
        )
        .unwrap_err()
        .code,
        "action_uncertain"
    );
    assert_eq!(fake.calls, 3);
    assert!(journal.blocks_game(&epoch().game));
}

fn changed_states(mutator: impl Fn(&mut OwnState)) -> Vec<OwnState> {
    let mut state = shown();
    mutator(&mut state);
    vec![shown(), state]
}
#[test]
fn close_refuses_every_original_assignment_identity_selection_folder_and_tab_change_before_intent()
{
    type Mutation = Box<dyn Fn(&mut OwnState)>;
    let mut mutations: Vec<Mutation> = vec![
        Box::new(|s| s.researching[3] = Some("infantry_weapons2".into())),
        Box::new(|s| s.researching.swap(2, 3)),
        Box::new(|s| {
            s.selected_slot = Some(0);
            s.panel_binding.as_mut().unwrap().selected_slot_pointer = 0x70000;
        }),
        Box::new(|s| {
            s.selected_folder = Some("electronics_folder".into());
            s.panel_binding.as_mut().unwrap().folder_index = 0;
        }),
        Box::new(|s| s.panel_binding.as_mut().unwrap().slots[3] += 8),
        Box::new(|s| s.panel_binding.as_mut().unwrap().folder_windows[1] += 8),
        Box::new(|s| s.panel_binding.as_mut().unwrap().tab = 1),
        Box::new(|s| s.panel_binding.as_mut().unwrap().facilities_parent += 8),
    ];
    for index in 0..6 {
        mutations.push(Box::new(move |s| {
            s.panel_binding.as_mut().unwrap().anchors[index] += 8
        }));
    }
    for index in 0..4 {
        mutations.push(Box::new(move |s| {
            s.panel_binding.as_mut().unwrap().roots[index] += 8
        }));
    }
    for mutation in mutations {
        let scratch = Scratch::new();
        let mut journal = Journal::open(&scratch.journal()).unwrap();
        let mut fake = closer(changed_states(mutation));
        let result = execute(
            &mut journal,
            "changed",
            close_target(),
            &authority(),
            &RwLock::new(true),
            &mut fake,
            Duration::from_secs(2),
        );
        assert_eq!(result.unwrap_err().code, "research_precondition");
        assert_eq!(fake.dispatches, 0);
        assert_eq!(journal.file.metadata().unwrap().len(), 0);
    }
}

#[test]
fn close_after_delivery_mutation_quarantines_and_intent_replay_never_settles() {
    for case in 0..6 {
        let scratch = Scratch::new();
        let mut journal = Journal::open(&scratch.journal()).unwrap();
        let a = authority();
        let gate = RwLock::new(true);
        let mut after = closed();
        match case {
            0 => after.researching[2] = None,
            1 => after.panel_binding.as_mut().unwrap().roots[2] += 8,
            2 => after.panel_binding.as_mut().unwrap().tab = 1,
            3 => after.selected_folder = Some("electronics_folder".into()),
            4 => {
                after.selected_slot = Some(0);
                after.panel_binding.as_mut().unwrap().selected_slot_pointer = 0x70000;
            }
            _ => after.visible_info.push(Some("support_weapons".into())),
        }
        let mut fake = closer(vec![shown(), shown(), after]);
        assert_eq!(
            execute(
                &mut journal,
                "uncertain-close",
                close_target(),
                &a,
                &gate,
                &mut fake,
                Duration::from_secs(2)
            )
            .unwrap_err()
            .code,
            "action_uncertain"
        );
        assert_eq!(fake.dispatches, 1);
        assert!(journal.blocks_game(&epoch().game));
        drop(journal);
        let original_bytes = fs::read(scratch.journal()).unwrap();
        let mut journal = Journal::open(&scratch.journal()).unwrap();
        fake.last = closed();
        let calls = fake.calls;
        for id in ["uncertain-close", "fresh-close"] {
            assert_eq!(
                execute(
                    &mut journal,
                    id,
                    close_target(),
                    &a,
                    &gate,
                    &mut fake,
                    Duration::from_secs(2)
                )
                .unwrap_err()
                .code,
                "action_uncertain"
            );
        }
        assert_eq!(fake.calls, calls);
        assert_eq!(fake.dispatches, 1);
        drop(journal);
        assert_eq!(fs::read(scratch.journal()).unwrap(), original_bytes);
    }
}

#[test]
fn close_journal_rejects_terminal_assignment_identity_tab_and_selected_state_tampering() {
    for case in 0..6 {
        let scratch = Scratch::new();
        let mut journal = Journal::open(&scratch.journal()).unwrap();
        let original = Receipt {
            version: 1,
            request_id: "close".into(),
            target: close_target(),
            status: Status::Intent,
            delivery: None,
            observed: shown(),
        };
        journal.append(original.clone()).unwrap();
        let mut after = closed();
        match case {
            0 => after.researching[2] = None,
            1 => after.panel_binding.as_mut().unwrap().roots[1] += 8,
            2 => after.panel_binding.as_mut().unwrap().tab = 1,
            3 => after.selected_folder = Some("electronics_folder".into()),
            4 => {
                after.selected_slot = None;
                after.panel_binding.as_mut().unwrap().selected_slot_pointer = 0;
            }
            _ => after.panel_binding.as_mut().unwrap().slots[3] += 8,
        }
        journal
            .append(Receipt {
                status: Status::Verified,
                delivery: Some(Delivery::ServerConfirmed),
                observed: after,
                ..original
            })
            .unwrap();
        drop(journal);
        assert!(Journal::open(&scratch.journal()).is_err());
    }
}

#[test]
fn close_not_queued_retains_exact_original_proof_and_tampering_refuses_reopen() {
    for tamper in [false, true] {
        let scratch = Scratch::new();
        let mut journal = Journal::open(&scratch.journal()).unwrap();
        let original = Receipt {
            version: 1,
            request_id: "not-queued".into(),
            target: close_target(),
            status: Status::Intent,
            delivery: None,
            observed: shown(),
        };
        journal.append(original.clone()).unwrap();
        let mut terminal = original.clone();
        terminal.status = Status::NotQueued;
        terminal.delivery = Some(Delivery::NotQueued);
        if tamper {
            terminal
                .observed
                .panel_binding
                .as_mut()
                .unwrap()
                .facilities_parent += 8;
        }
        journal.append(terminal).unwrap();
        drop(journal);
        if tamper {
            assert!(Journal::open(&scratch.journal()).is_err());
        } else {
            let mut journal = Journal::open(&scratch.journal()).unwrap();
            assert!(!journal.blocks_game(&epoch().game));
            let mut fake = closer(vec![shown()]);
            assert_eq!(
                execute(
                    &mut journal,
                    "not-queued",
                    close_target(),
                    &authority(),
                    &RwLock::new(true),
                    &mut fake,
                    Duration::from_secs(2)
                )
                .unwrap_err()
                .code,
                "input_not_queued"
            );
            assert_eq!(fake.dispatches, 0);
            assert_eq!(fake.calls, 0);
        }
    }
}

#[test]
fn close_binding_validation_and_target_candidate_kinds_fail_closed() {
    let mut invalid = vec![];
    for case in 0..10 {
        let mut state = shown();
        let binding = state.panel_binding.as_mut().unwrap();
        match case {
            0 => binding.roots[0] = 0,
            1 => binding.anchors[5] = 0x10001,
            2 => binding.slots[1] = binding.slots[0],
            3 => {
                binding.slots.pop();
            }
            4 => binding.selected_slot_pointer = 0,
            5 => binding.folder_index = 2,
            6 => binding.folder_index = -1,
            7 => binding.folder_windows[0] = 0,
            8 => {
                binding.folder_windows.clear();
                state.selected_folder = None;
                binding.folder_index = -1;
            }
            _ => binding.tab = -1,
        }
        invalid.push(state);
    }
    for state in invalid {
        assert!(state.validate(&close_target()).is_err());
    }
    let mut empty = shown();
    let binding = empty.panel_binding.as_mut().unwrap();
    binding.folder_windows.clear();
    binding.folder_index = 0;
    empty.selected_folder = None;
    empty.validate(&close_target()).unwrap();
    empty.selected_slot = None;
    empty.panel_binding.as_mut().unwrap().selected_slot_pointer = 0;
    empty.validate(&close_target()).unwrap();
    for candidate in [Candidate::OpenPanel, Candidate::Point(point())] {
        let scratch = Scratch::new();
        let mut journal = Journal::open(&scratch.journal()).unwrap();
        let mut fake = closer(vec![shown()]);
        fake.candidate = candidate;
        assert_eq!(
            execute(
                &mut journal,
                "wrong",
                close_target(),
                &authority(),
                &RwLock::new(true),
                &mut fake,
                Duration::from_secs(2)
            )
            .unwrap_err()
            .code,
            "research_precondition"
        );
        assert_eq!(fake.dispatches, 0);
    }
    for case in 0..3 {
        let mut t = close_target();
        match case {
            0 => t.slot = 1,
            1 => t.technology = "support_weapons".into(),
            _ => t.folder_id = Some("industry_folder".into()),
        }
        assert_eq!(t.validate_key().unwrap_err().code, "invalid_arguments");
    }
}

#[test]
fn legacy_state_and_execution_json_remain_byte_identical_without_private_binding() {
    let original = state();
    let json = serde_json::to_string(&original).unwrap();
    assert!(!json.contains("panel_binding"));
    let restored: OwnState = serde_json::from_str(&json).unwrap();
    assert_eq!(restored.panel_binding, None);
    assert_eq!(serde_json::to_string(&restored).unwrap(), json);
    let receipt = Receipt {
        version: 1,
        request_id: "old".into(),
        target: target(Stage::Research),
        status: Status::Verified,
        delivery: None,
        observed: reached(),
    };
    let private = serde_json::to_value(&receipt).unwrap();
    let public = serde_json::to_value(Execution {
        receipt,
        historical: false,
        current_state_verified: true,
        event_causality_verified: false,
    })
    .unwrap();
    assert_eq!(public["receipt"], private);

    let scratch = Scratch::new();
    let mut journal = Journal::open(&scratch.journal()).unwrap();
    let mut fake = Fake::new();
    fake.last = reached();
    execute(
        &mut journal,
        "old-record",
        target(Stage::Research),
        &authority(),
        &RwLock::new(true),
        &mut fake,
        Duration::from_secs(2),
    )
    .unwrap();
    drop(journal);
    let bytes = fs::read(scratch.journal()).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("panel_binding"));
    let mut journal = Journal::open(&scratch.journal()).unwrap();
    let old = execute(
        &mut journal,
        "old-record",
        target(Stage::Research),
        &authority(),
        &RwLock::new(true),
        &mut fake,
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(old.historical);
    drop(journal);
    assert_eq!(fs::read(scratch.journal()).unwrap(), bytes);
}

fn identity_transition() -> Error {
    error(
        "research_identity_transition",
        "Known retained Research root animation",
    )
}
fn unopened() -> OwnState {
    OwnState {
        active_mode: Some(23),
        panel_visible: Some(false),
        ..panel_state()
    }
}

#[test]
fn identity_animation_recovers_each_research_stage_with_one_input_and_exact_terminal_proof() {
    for stage in [
        Stage::OpenPanel,
        Stage::SelectSlot,
        Stage::SelectCategory,
        Stage::View,
        Stage::Research,
    ] {
        let scratch = Scratch::new();
        let a = authority();
        let gate = RwLock::new(true);
        let mut t = target(stage);
        let (before, after) = match stage {
            Stage::OpenPanel => (unopened(), panel_state()),
            Stage::SelectSlot => (panel_state(), selected_empty_state()),
            Stage::SelectCategory => {
                t.technology.clear();
                t.folder_id = Some("industry_folder".into());
                let mut before = selected_empty_state();
                before.selected_folder = Some("infantry_folder".into());
                let mut after = before.clone();
                after.selected_folder = t.folder_id.clone();
                (before, after)
            }
            Stage::View => (selected_empty_state(), state()),
            Stage::Research => (state(), reached()),
            Stage::ClosePanel => unreachable!(),
        };
        let mut f = Fake::new();
        f.candidate = if stage == Stage::OpenPanel {
            Candidate::OpenPanel
        } else {
            Candidate::Point(point())
        };
        f.delivery = Delivery::ServerConfirmed;
        f.observations = VecDeque::from([
            Ok(before.clone()),
            Ok(before),
            Err(identity_transition()),
            Err(identity_transition()),
            Ok(after.clone()),
        ]);
        let mut journal = Journal::open(&scratch.journal()).unwrap();
        let result = execute(
            &mut journal,
            "animation",
            t.clone(),
            &a,
            &gate,
            &mut f,
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(result.receipt.observed, after, "{stage:?}");
        assert!(result.current_state_verified && !result.historical);
        assert_eq!(f.dispatches, 1);
        assert_eq!(f.calls, 5);
        assert!(!journal.blocks_game(&epoch().game));
        drop(journal);
        let bytes = fs::read(scratch.journal()).unwrap();
        let mut journal = Journal::open(&scratch.journal()).unwrap();
        let result = execute(
            &mut journal,
            "animation",
            t,
            &a,
            &gate,
            &mut f,
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(result.historical && !result.current_state_verified);
        assert_eq!(f.dispatches, 1);
        assert_eq!(f.calls, 5);
        drop(journal);
        assert_eq!(fs::read(scratch.journal()).unwrap(), bytes);
    }
}

#[test]
fn identity_animation_before_intent_is_not_retried_and_never_dispatches() {
    for final_read in [false, true] {
        let scratch = Scratch::new();
        let mut journal = Journal::open(&scratch.journal()).unwrap();
        let mut f = Fake::new();
        f.candidate = Candidate::OpenPanel;
        if final_read {
            f.observations.push_back(Ok(unopened()));
        }
        f.observations.push_back(Err(identity_transition()));
        f.observations.push_back(Ok(panel_state()));
        let t = target(Stage::OpenPanel);
        let failure = execute(
            &mut journal,
            "before",
            t.clone(),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert_eq!(failure.code, "research_identity_transition");
        assert_eq!(
            journal.failure_evidence("before", &t, &failure),
            FailureEvidence::BeforeIntent
        );
        assert_eq!(f.dispatches, 0);
        assert_eq!(f.calls, if final_read { 2 } else { 1 });
        assert_eq!(journal.file.metadata().unwrap().len(), 0);
        assert!(!journal.blocks_game(&epoch().game));
    }
}

#[test]
fn identity_animation_timeout_retains_cause_and_quarantine_across_reopen_without_replay() {
    let scratch = Scratch::new();
    let a = authority();
    let gate = RwLock::new(true);
    let mut journal = Journal::open(&scratch.journal()).unwrap();
    let mut f = Fake::new();
    f.candidate = Candidate::OpenPanel;
    f.delivery = Delivery::ServerConfirmed;
    f.observations = VecDeque::from([Ok(unopened()), Ok(unopened())]);
    for _ in 0..100 {
        f.observations.push_back(Err(identity_transition()));
    }
    let t = target(Stage::OpenPanel);
    let failure = execute(
        &mut journal,
        "timeout",
        t.clone(),
        &a,
        &gate,
        &mut f,
        Duration::from_millis(200),
    )
    .unwrap_err();
    assert_eq!(failure.code, "action_uncertain");
    assert!(failure.message.contains("cause=research_deadline"));
    assert!(failure.message.contains("rejected_identity_reads="));
    assert!(failure
        .message
        .contains("last_identity_transition=Known retained Research root animation"));
    assert!(failure.message.len() <= 512);
    assert!(f.calls > 2);
    assert_eq!(f.dispatches, 1);
    assert!(journal.blocks_game(&epoch().game));
    drop(journal);
    let bytes = fs::read(scratch.journal()).unwrap();
    let mut journal = Journal::open(&scratch.journal()).unwrap();
    let calls = f.calls;
    for id in ["timeout", "new-id"] {
        let failure = execute(
            &mut journal,
            id,
            t.clone(),
            &a,
            &gate,
            &mut f,
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(failure.code, "action_uncertain");
    }
    assert_eq!(f.calls, calls);
    assert_eq!(f.dispatches, 1);
    drop(journal);
    assert_eq!(fs::read(scratch.journal()).unwrap(), bytes);
}

#[test]
fn identity_animation_never_converts_unknown_or_changed_identity_into_recovery() {
    for code in [
        "research_precondition",
        "invalid_gui_type",
        "ownership_changed",
    ] {
        let scratch = Scratch::new();
        let mut journal = Journal::open(&scratch.journal()).unwrap();
        let mut f = Fake::new();
        f.candidate = Candidate::OpenPanel;
        f.delivery = Delivery::ServerConfirmed;
        f.observations = VecDeque::from([
            Ok(unopened()),
            Ok(unopened()),
            Err(identity_transition()),
            Err(error(
                code,
                "Unknown transition or original root/owner changed",
            )),
            Ok(panel_state()),
        ]);
        let failure = execute(
            &mut journal,
            "strict",
            target(Stage::OpenPanel),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert_eq!(failure.code, "action_uncertain");
        assert!(failure.message.contains(&format!("cause={code}")));
        assert_eq!(f.calls, 4);
        assert_eq!(f.dispatches, 1);
        assert!(journal.blocks_game(&epoch().game));
    }
}
