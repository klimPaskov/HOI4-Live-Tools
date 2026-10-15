use super::*;

#[test]
fn typed_search_projection_needs_complete_exact_canonical_selection() {
    let key = "GER_rhineland";
    let body = serde_json::json!({"owned":true,"build_id":crate::game::BUILD_ID,
        "command_observation":{"requested_key":key,"same_reader_clock_context":true},
        "focuses":[{"key":key}],"search":{"available":true,"requested_key":key,
        "complete_current_item_coverage":true,"requested_match_count":1,"selected_key":key,
        "selected_matches_requested":true,"selected_in_matches":true,
        "query_control":{"available":true,"named_container_binding_verified":true,
        "ordinary_listener_binding_verified":true,"renderer_control_join_verified":true,
        "query_encoding":"bounded_utf32le","query_text":"rhineland","container_locally_ready":true,
        "routing_diagnostic":{"available":true,"gui_global_binding_verified":true,
        "receiver_layout_verified":true,"control_enabled_flag":true,"keyboard_focus_verified":true,"text_receiver_verified":true}}}});
    let c = SearchControl::from_owned_observation(&body, key).unwrap();
    assert_eq!(c.query_text.as_deref(), Some("rhineland"));
    assert_eq!(c.requested_selection_verified, Some(true));
    for (field, value) in [
        ("complete_current_item_coverage", serde_json::json!(false)),
        ("requested_match_count", serde_json::json!(0)),
        ("selected_key", serde_json::json!("GER_other")),
        ("selected_matches_requested", serde_json::json!(false)),
        ("selected_in_matches", serde_json::json!(false)),
    ] {
        let mut b = body.clone();
        b["search"][field] = value;
        assert_eq!(
            SearchControl::from_owned_observation(&b, key)
                .unwrap()
                .requested_selection_verified,
            Some(false)
        );
    }
    let mut b = body.clone();
    b["command_observation"]["same_reader_clock_context"] = serde_json::json!(false);
    assert!(SearchControl::from_owned_observation(&b, key).is_err());
    let mut b = body;
    b["search"]["query_control"]["query_text"] = serde_json::Value::Null;
    assert!(SearchControl::from_owned_observation(&b, key).is_err());
}

fn query_target() -> Target {
    Target {
        query: Some("four year plan".into()),
        ..target(Stage::SearchQuery)
    }
}
fn queried() -> OwnState {
    let mut s = search_state(true);
    let c = s.search_control.as_mut().unwrap();
    c.empty_query = false;
    c.query_text = Some("four year plan".into());
    c.requested_selection_verified = Some(true);
    s
}
fn backend(before: OwnState, after: OwnState) -> Fake {
    let mut f = Fake::sequence(Stage::SearchQuery, before, after);
    f.candidate = Candidate::SearchQuery("four year plan".into());
    f
}
#[test]
fn exact_query_selection_replay_and_conflicting_query_remain_durable() {
    for active in [ActiveFocus::Idle, ActiveFocus::Active("GER_other".into())] {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let a = authority();
        let g = RwLock::new(true);
        let t = query_target();
        let mut before = search_state(true);
        before.active_focus = active.clone();
        let mut after = queried();
        after.active_focus = active;
        let mut f = backend(before, after);
        let first = run(
            &mut j,
            "query",
            t.clone(),
            &a,
            &g,
            &mut f,
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(first.current_state_verified && !first.historical);
        assert_eq!(f.dispatches, 1);
        drop(j);
        let mut j = Journal::open(&sc.path()).unwrap();
        let replay = run(
            &mut j,
            "query",
            t.clone(),
            &a,
            &g,
            &mut f,
            Duration::from_secs(5),
        )
        .unwrap();
        assert!(replay.historical && !replay.current_state_verified);
        assert_eq!(replay.receipt, first.receipt);
        let mut changed = t.clone();
        changed.query = Some("rhineland".into());
        assert_eq!(
            run(
                &mut j,
                "query",
                changed,
                &a,
                &g,
                &mut f,
                Duration::from_secs(5)
            )
            .unwrap_err()
            .code,
            "request_id_conflict"
        );
        assert_eq!(
            run(&mut j, "new", t, &a, &g, &mut f, Duration::from_secs(5))
                .unwrap_err()
                .code,
            "focus_precondition"
        );
        assert_eq!(f.dispatches, 1);
    }
}
#[test]
fn query_requires_empty_text_both_receivers_and_target_bound_candidate() {
    for field in 0..7 {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let mut before = search_state(true);
        let c = before.search_control.as_mut().unwrap();
        match field {
            0 => c.query_text = None,
            1 => c.query_text = Some("old".into()),
            2 => c.empty_query = false,
            3 => c.keyboard_focus_verified = false,
            4 => c.text_receiver_verified = false,
            5 => c.enabled = false,
            _ => {}
        }
        let mut f = backend(before, queried());
        if field == 6 {
            f.candidate = Candidate::SearchQuery("rhineland".into());
        }
        assert!(run(
            &mut j,
            "bad",
            query_target(),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(5)
        )
        .is_err());
        assert_eq!(f.dispatches, 0);
        assert_eq!(fs::metadata(sc.path()).unwrap().len(), 0);
    }
}
#[test]
fn partial_query_wrong_selection_lost_receiver_and_changed_focus_quarantine() {
    for field in 0..5 {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let mut after = queried();
        let c = after.search_control.as_mut().unwrap();
        match field {
            0 => c.query_text = Some("four year".into()),
            1 => c.requested_selection_verified = Some(false),
            2 => c.text_receiver_verified = false,
            3 => c.keyboard_focus_verified = false,
            _ => after.active_focus = ActiveFocus::Active("GER_other".into()),
        }
        let mut f = backend(search_state(true), after);
        f.queue
            .push_back(Err(error("fixture_end", "End invalid post-state")));
        let t = query_target();
        assert_eq!(
            run(
                &mut j,
                "query",
                t.clone(),
                &authority(),
                &RwLock::new(true),
                &mut f,
                Duration::from_secs(5)
            )
            .unwrap_err()
            .code,
            "action_uncertain"
        );
        assert_eq!(f.dispatches, 1);
        drop(j);
        let j = Journal::open(&sc.path()).unwrap();
        assert!(j.blocks_game(&t.epoch.game));
    }
}
#[test]
fn query_shape_and_legacy_receipts_are_unambiguous() {
    for text in ["", " a", "a ", "A", "a\n", "a\t", "GER_focus", "日本"] {
        assert!(validate_query(text).is_err());
    }
    assert!(validate_query(&"a".repeat(49)).is_err());
    assert!(validate_query(&"a".repeat(48)).is_ok());
    let old = target(Stage::View);
    let v = serde_json::to_value(&old).unwrap();
    assert!(v.get("query").is_none());
    assert_eq!(serde_json::from_value::<Target>(v).unwrap(), old);
    let mut wrong = old;
    wrong.query = Some("a".into());
    assert!(wrong.validate_key().is_err());
    let mut missing = query_target();
    missing.query = None;
    assert!(missing.validate_key().is_err());
    let old = serde_json::json!({"locally_ready":true,"empty_query":true,"enabled":true,"keyboard_focus_verified":true,"text_receiver_verified":true});
    let c: SearchControl = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(serde_json::to_value(c).unwrap(), old);
}
