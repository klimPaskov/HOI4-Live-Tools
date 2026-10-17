use super::*;
fn research_state(mode: i32) -> OwnState {
    let mut s = state(mode);
    s.research_panel_root = if mode == 5 {
        LocalRoot::Ready
    } else {
        LocalRoot::Hidden
    };
    s.research_tree_root = LocalRoot::Hidden;
    s
}
fn attempt(j: &mut Journal, id: &str, f: &mut Fake) -> Result<Execution> {
    run(
        j,
        id,
        target(Stage::OpenPoliticsFromResearch),
        &authority(),
        &RwLock::new(true),
        f,
        Duration::from_secs(1),
    )
}
#[test]
fn research_return_distinct_receipt_reopens_without_replay_or_new_noop() {
    for active in [ActiveFocus::Idle, ActiveFocus::Active("GER_other".into())] {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let mut before = research_state(5);
        before.active_focus = active.clone();
        let mut after = research_state(12);
        after.active_focus = active.clone();
        let mut f = Fake::sequence(Stage::OpenPoliticsFromResearch, before, after);
        let e = attempt(&mut j, "return", &mut f).unwrap();
        assert_eq!(e.receipt.observed.active_focus, active);
        assert_eq!(e.receipt.target.stage, Stage::OpenPoliticsFromResearch);
        assert!(e.current_state_verified && !e.historical);
        assert_eq!(f.dispatches, 1);
        drop(j);
        let mut j = Journal::open(&sc.path()).unwrap();
        let again = attempt(&mut j, "return", &mut f).unwrap();
        assert!(again.historical && !again.current_state_verified);
        assert_eq!(again.receipt, e.receipt);
        assert_eq!(
            attempt(&mut j, "new", &mut f).unwrap_err().code,
            "focus_precondition"
        );
        assert_eq!(f.dispatches, 1);
        drop(j);
        assert_eq!(fs::read_to_string(sc.path()).unwrap().lines().count(), 2);
    }
}
#[test]
fn research_return_requires_every_positive_panel_tree_root_and_blocker_fact() {
    let mut bad = vec![
        state(5),
        research_state(6),
        research_state(12),
        research_state(23),
    ];
    for root in [LocalRoot::Unknown, LocalRoot::Animating, LocalRoot::Hidden] {
        let mut s = research_state(5);
        s.research_panel_root = root;
        bad.push(s);
    }
    for root in [LocalRoot::Unknown, LocalRoot::Animating, LocalRoot::Ready] {
        for field in 0..3 {
            let mut s = research_state(5);
            match field {
                0 => s.research_tree_root = root,
                1 => s.focus_root = root,
                _ => s.politics_root = root,
            };
            bad.push(s);
        }
    }
    for counts in [None, Some([1, 0]), Some([0, 1])] {
        let mut s = research_state(5);
        s.own_detail_counts = counts;
        bad.push(s);
        let mut s = research_state(5);
        s.own_research_info_counts = counts;
        bad.push(s);
    }
    let mut unknown = research_state(5);
    unknown.active_focus = ActiveFocus::Unknown;
    bad.push(unknown);
    for before in bad {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let mut f = Fake::sequence(Stage::OpenPoliticsFromResearch, before, research_state(12));
        assert!(attempt(&mut j, "bad", &mut f).is_err());
        assert_eq!(f.dispatches, 0);
        drop(j);
        assert!(fs::read(sc.path()).unwrap().is_empty());
    }
    // The old stage is still restricted to mode23 even with these new facts.
    assert!(research_state(5)
        .preflight(&target(Stage::OpenPolitics))
        .is_err());
}
#[test]
fn research_return_cannot_substitute_old_q_or_pointer_candidate() {
    for candidate in [
        Candidate::OpenPolitics,
        Candidate::ClosePolitics,
        Candidate::Point(point()),
    ] {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let mut f = Fake::sequence(
            Stage::OpenPoliticsFromResearch,
            research_state(5),
            research_state(12),
        );
        f.candidate = candidate;
        assert_eq!(
            attempt(&mut j, "wrong", &mut f).unwrap_err().code,
            "focus_precondition"
        );
        assert_eq!(f.dispatches, 0);
    }
}
#[test]
fn research_return_changed_focus_after_input_quarantines_across_restart() {
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let mut after = research_state(12);
    after.active_focus = ActiveFocus::Active("GER_other".into());
    let mut f = Fake::sequence(Stage::OpenPoliticsFromResearch, research_state(5), after);
    assert_eq!(
        attempt(&mut j, "uncertain", &mut f).unwrap_err().code,
        "action_uncertain"
    );
    assert_eq!(f.dispatches, 1);
    drop(j);
    let mut j = Journal::open(&sc.path()).unwrap();
    assert!(j.blocks_game(&epoch().game));
    for id in ["uncertain", "new"] {
        assert_eq!(
            attempt(&mut j, id, &mut f).unwrap_err().code,
            "action_uncertain"
        );
    }
    assert_eq!(f.dispatches, 1);
    drop(j);
    assert_eq!(fs::read_to_string(sc.path()).unwrap().lines().count(), 1);
}
#[test]
fn research_return_unsettled_post_root_never_becomes_verified() {
    for root in [LocalRoot::Unknown, LocalRoot::Ready, LocalRoot::Animating] {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let mut after = research_state(12);
        after.research_panel_root = root;
        let mut f = Fake::sequence(Stage::OpenPoliticsFromResearch, research_state(5), after);
        assert_eq!(
            attempt(&mut j, "uncertain", &mut f).unwrap_err().code,
            "action_uncertain"
        );
        assert_eq!(f.dispatches, 1);
        drop(j);
        assert!(Journal::open(&sc.path())
            .unwrap()
            .blocks_game(&epoch().game));
    }
}
#[test]
fn research_return_optional_facts_preserve_old_serialization_and_receipts() {
    let old = state(23);
    let serialized = serde_json::to_vec(&old).unwrap();
    let text = String::from_utf8(serialized.clone()).unwrap();
    assert!(!text.contains("research_panel_root") && !text.contains("research_tree_root"));
    let restored: OwnState = serde_json::from_slice(&serialized).unwrap();
    assert_eq!(restored.research_panel_root, LocalRoot::Unknown);
    assert_eq!(serde_json::to_vec(&restored).unwrap(), serialized);
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let mut f = Fake::sequence(Stage::OpenPolitics, state(23), state(12));
    let receipt = run(
        &mut j,
        "old",
        target(Stage::OpenPolitics),
        &authority(),
        &RwLock::new(true),
        &mut f,
        Duration::from_secs(1),
    )
    .unwrap();
    drop(j);
    let bytes = fs::read(sc.path()).unwrap();
    assert!(!String::from_utf8(bytes.clone())
        .unwrap()
        .contains("research_panel_root"));
    let mut j = Journal::open(&sc.path()).unwrap();
    let replay = run(
        &mut j,
        "old",
        target(Stage::OpenPolitics),
        &authority(),
        &RwLock::new(true),
        &mut f,
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(replay.receipt, receipt.receipt);
    assert!(replay.historical);
    drop(j);
    assert_eq!(fs::read(sc.path()).unwrap(), bytes);
}

#[test]
fn research_return_journal_rejects_missing_positive_before_or_after_facts() {
    for record_index in [0, 1] {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let mut f = Fake::sequence(
            Stage::OpenPoliticsFromResearch,
            research_state(5),
            research_state(12),
        );
        attempt(&mut j, "return", &mut f).unwrap();
        drop(j);
        let mut rows: Vec<serde_json::Value> = fs::read_to_string(sc.path())
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        rows[record_index]["observed"]
            .as_object_mut()
            .unwrap()
            .remove("research_panel_root");
        let text = rows
            .iter()
            .map(|row| format!("{}\n", serde_json::to_string(row).unwrap()))
            .collect::<String>();
        fs::write(sc.path(), text).unwrap();
        assert_eq!(
            Journal::open(&sc.path()).err().unwrap().code,
            "invalid_journal"
        );
    }
}
