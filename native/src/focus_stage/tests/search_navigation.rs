use super::*;

fn query_target(stage: Stage) -> Target {
    Target {
        query: Some("four year plan".into()),
        ..target(stage)
    }
}
fn matched(selected: &str) -> OwnState {
    let mut s = search_state(true);
    let c = s.search_control.as_mut().unwrap();
    c.empty_query = false;
    c.query_text = Some("four year plan".into());
    c.requested_selection_verified = Some(selected == "GER_rhineland");
    c.matches = Some(SearchMatches {
        ordered: vec![
            "GER_first".into(),
            "GER_rhineland".into(),
            "GER_last".into(),
        ],
        selected: selected.into(),
    });
    s
}
fn next_backend(before: OwnState, after: OwnState) -> Fake {
    let matches = before.search_matches().unwrap().clone();
    let mut f = Fake::sequence(Stage::SearchNext, before, after);
    f.candidate = Candidate::SearchNext(matches);
    f
}

#[test]
fn text_receipt_reports_matches_without_reinterpreting_strict_query() {
    let after = matched("GER_first");
    assert!(!after.reached(&query_target(Stage::SearchQuery)));
    assert!(after.reached(&query_target(Stage::SearchText)));
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let mut f = Fake::sequence(Stage::SearchText, search_state(true), after);
    f.candidate = Candidate::SearchQuery("four year plan".into());
    let t = query_target(Stage::SearchText);
    let r = run(
        &mut j,
        "text",
        t.clone(),
        &authority(),
        &RwLock::new(true),
        &mut f,
        Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(
        r.receipt.observed.search_matches().unwrap().selected,
        "GER_first"
    );
    assert_eq!(f.dispatches, 1);
    drop(j);
    let mut j = Journal::open(&sc.path()).unwrap();
    assert!(
        run(
            &mut j,
            "text",
            t,
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(5)
        )
        .unwrap()
        .historical
    );
    assert_eq!(f.dispatches, 1);
}

#[test]
fn next_match_preserves_order_and_active_focus_with_durable_replay() {
    for active in [ActiveFocus::Idle, ActiveFocus::Active("GER_active".into())] {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let t = query_target(Stage::SearchNext);
        let mut before = matched("GER_first");
        let mut after = matched("GER_rhineland");
        before.active_focus = active.clone();
        after.active_focus = active;
        let mut f = next_backend(before, after);
        let r = run(
            &mut j,
            "tab",
            t.clone(),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(r.receipt.status, Status::Verified);
        drop(j);
        let mut j = Journal::open(&sc.path()).unwrap();
        assert!(
            run(
                &mut j,
                "tab",
                t.clone(),
                &authority(),
                &RwLock::new(true),
                &mut f,
                Duration::from_secs(5)
            )
            .unwrap()
            .historical
        );
        assert_eq!(
            run(
                &mut j,
                "new-tab",
                t,
                &authority(),
                &RwLock::new(true),
                &mut f,
                Duration::from_secs(5)
            )
            .unwrap_err()
            .code,
            "focus_precondition"
        );
        assert_eq!(f.dispatches, 1);
    }
}

#[test]
fn next_is_exactly_one_step_with_wraparound_not_a_skip_or_arbitrary_key() {
    let mut t = query_target(Stage::SearchNext);
    let mut before = matched("GER_last");
    assert_eq!(
        before.search_matches().unwrap().next_key(),
        Some("GER_first")
    );
    assert!(before.preflight(&t).is_err());
    t.focus_id = "GER_first".into();
    before.canonical_focus_id = Some(t.focus_id.clone());
    assert!(before.preflight(&t).is_ok());
    let mut after = matched("GER_first");
    after.canonical_focus_id = Some(t.focus_id.clone());
    after
        .search_control
        .as_mut()
        .unwrap()
        .requested_selection_verified = Some(true);
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let mut f = next_backend(before, after);
    run(
        &mut j,
        "wrap",
        t,
        &authority(),
        &RwLock::new(true),
        &mut f,
        Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(f.dispatches, 1);
}

#[test]
fn search_step_rejects_changed_order_before_input_and_quarantines_after_input() {
    for post_input in [false, true] {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let t = query_target(Stage::SearchNext);
        let before = matched("GER_first");
        let mut f = next_backend(before.clone(), matched("GER_rhineland"));
        let index = if post_input { 2 } else { 1 };
        let mut changed = if post_input {
            matched("GER_rhineland")
        } else {
            before
        };
        changed
            .search_control
            .as_mut()
            .unwrap()
            .matches
            .as_mut()
            .unwrap()
            .ordered
            .push("GER_added".into());
        f.queue[index] = Ok(changed);
        let e = run(
            &mut j,
            "changed",
            t.clone(),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(
            e.code,
            if post_input {
                "action_uncertain"
            } else {
                "torn_read"
            }
        );
        assert_eq!(f.dispatches, usize::from(post_input));
        drop(j);
        let j = Journal::open(&sc.path()).unwrap();
        assert_eq!(j.blocks_game(&t.epoch.game), post_input);
    }
}

#[test]
fn partial_text_missing_match_or_lost_receiver_never_completes_text_stage() {
    for field in 0..4 {
        let mut s = matched("GER_first");
        let c = s.search_control.as_mut().unwrap();
        match field {
            0 => c.query_text = Some("four".into()),
            1 => c.matches = None,
            2 => c
                .matches
                .as_mut()
                .unwrap()
                .ordered
                .retain(|id| id != "GER_rhineland"),
            _ => c.keyboard_focus_verified = false,
        }
        assert!(!s.reached(&query_target(Stage::SearchText)));
    }
}

#[test]
fn canonical_match_projection_checks_complete_unique_bounded_order() {
    let body = serde_json::json!({"complete_current_item_coverage":true,"ordered_matches_available":true,
        "selected_in_matches":true,"match_count":3,"ordered_match_keys":["GER_first","GER_rhineland","GER_last"],"selected_key":"GER_last"});
    let m = SearchMatches::from_observation(&body).unwrap().unwrap();
    assert_eq!(m.next_key(), Some("GER_first"));
    for field in [
        "complete_current_item_coverage",
        "ordered_matches_available",
        "selected_in_matches",
    ] {
        let mut b = body.clone();
        b[field] = serde_json::json!(false);
        assert_eq!(SearchMatches::from_observation(&b).unwrap(), None);
    }
    for (field, value) in [
        ("match_count", serde_json::json!(2)),
        ("selected_key", serde_json::json!("GER_foreign")),
        (
            "ordered_match_keys",
            serde_json::json!(["GER_first", "GER_last", "GER_last"]),
        ),
        (
            "ordered_match_keys",
            serde_json::json!(["GER_first", "../foreign", "GER_last"]),
        ),
    ] {
        let mut b = body.clone();
        b[field] = value;
        assert!(SearchMatches::from_observation(&b).is_err());
    }
    let mut m = m;
    m.ordered = (0..65).map(|n| format!("GER_{n}")).collect();
    m.selected = "GER_0".into();
    assert!(m.validate().is_err());
    m.ordered.pop();
    assert!(m.validate().is_ok());
}

#[test]
fn reopened_next_receipt_cannot_change_its_durable_match_order() {
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let mut f = next_backend(matched("GER_first"), matched("GER_rhineland"));
    run(
        &mut j,
        "tab",
        query_target(Stage::SearchNext),
        &authority(),
        &RwLock::new(true),
        &mut f,
        Duration::from_secs(5),
    )
    .unwrap();
    drop(j);
    let mut rows: Vec<Receipt> = fs::read_to_string(sc.path())
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    rows[1]
        .observed
        .search_control
        .as_mut()
        .unwrap()
        .matches
        .as_mut()
        .unwrap()
        .ordered
        .push("GER_added".into());
    let content: String = rows
        .iter()
        .map(|r| serde_json::to_string(r).unwrap() + "\n")
        .collect();
    fs::write(sc.path(), content).unwrap();
    assert!(matches!(Journal::open(&sc.path()), Err(e) if e.code == "invalid_journal"));
}
