use super::*;
fn pending(states: &str) -> Error {
    error("research_open_transition", &format!("Own Research opening readiness pending; root=research_panel; transitions={states}; literal_shown=true"))
}
fn opener(observations: Vec<Result<OwnState>>) -> Fake {
    let mut f = Fake::new();
    f.candidate = Candidate::OpenPanel;
    f.delivery = Delivery::ServerConfirmed;
    f.observations = observations.into();
    f
}
#[test]
fn known_opening_animation_waits_after_one_input_for_settled_final_proof() {
    let scratch = Scratch::new();
    let a = authority();
    let gate = RwLock::new(true);
    let mut j = Journal::open(&scratch.journal()).unwrap();
    let mut f = opener(vec![
        Ok(closed_map_state()),
        Ok(closed_map_state()),
        Err(pending("[1, 0]")),
        Err(pending("[0, 2]")),
        Ok(panel_state()),
    ]);
    let t = target(Stage::OpenPanel);
    let r = execute(
        &mut j,
        "open-settled",
        t.clone(),
        &a,
        &gate,
        &mut f,
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(r.current_state_verified && !r.historical);
    assert_eq!(r.receipt.observed, panel_state());
    assert_eq!(r.receipt.delivery, Some(Delivery::ServerConfirmed));
    assert_eq!(f.dispatches, 1);
    assert_eq!(f.calls, 5);
    assert!(!j.blocks_game(&epoch().game));
    let calls = f.calls;
    assert!(
        execute(
            &mut j,
            "open-settled",
            t,
            &a,
            &gate,
            &mut f,
            Duration::from_secs(2)
        )
        .unwrap()
        .historical
    );
    assert_eq!(f.dispatches, 1);
    assert_eq!(f.calls, calls);
}
#[test]
fn opening_pending_before_intent_never_sleeps_retries_or_dispatches() {
    for final_read in [false, true] {
        let scratch = Scratch::new();
        let mut j = Journal::open(&scratch.journal()).unwrap();
        let mut states = Vec::new();
        if final_read {
            states.push(Ok(closed_map_state()));
        }
        states.extend([Err(pending("[1, 0]")), Ok(panel_state())]);
        let mut f = opener(states);
        let t = target(Stage::OpenPanel);
        let e = execute(
            &mut j,
            "strict-before",
            t.clone(),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert_eq!(e.code, "research_open_transition");
        assert_eq!(f.dispatches, 0);
        assert_eq!(f.calls, if final_read { 2 } else { 1 });
        assert_eq!(j.file.metadata().unwrap().len(), 0);
        assert_eq!(
            j.failure_evidence("strict-before", &t, &e),
            FailureEvidence::BeforeIntent
        );
    }
}
#[test]
fn persistent_opening_animation_obeys_original_deadline_and_quarantines_without_replay() {
    struct PendingUntilDeadline {
        inner: Fake,
        dispatched_deadline: Option<Instant>,
    }
    impl Backend for PendingUntilDeadline {
        fn observe(&mut self) -> Result<OwnState> {
            let result = self.inner.observe();
            if result
                .as_ref()
                .is_err_and(|cause| cause.code == "research_open_transition")
            {
                // This is a post-input timeout fixture, not a journal-fsync
                // performance test. Consume the caller's original deadline only
                // after possible input and a known pending read. Never renew it.
                let deadline = self.dispatched_deadline.unwrap();
                thread::sleep(
                    deadline.saturating_duration_since(Instant::now()) + Duration::from_millis(1),
                );
            }
            result
        }
        fn candidate(&mut self, target: &Target) -> Result<Candidate> {
            self.inner.candidate(target)
        }
        fn dispatch_once(&mut self, candidate: &Candidate, deadline: Instant) -> Delivery {
            assert!(self.dispatched_deadline.replace(deadline).is_none());
            self.inner.dispatch_once(candidate, deadline)
        }
        fn confirm_input_idle(&mut self, candidate: &Candidate, deadline: Instant) -> Result<()> {
            self.inner.confirm_input_idle(candidate, deadline)
        }
    }
    let scratch = Scratch::new();
    let a = authority();
    let gate = RwLock::new(true);
    let mut j = Journal::open(&scratch.journal()).unwrap();
    let mut f = PendingUntilDeadline {
        inner: opener(vec![
            Ok(closed_map_state()),
            Ok(closed_map_state()),
            Err(pending("[1, 0]")),
        ]),
        dispatched_deadline: None,
    };
    let t = target(Stage::OpenPanel);
    let e = j
        .execute(
            "opening-timeout",
            t.clone(),
            &"a".repeat(32),
            &a,
            &gate,
            &mut f,
            Duration::from_secs(5),
        )
        .unwrap_err();
    assert_eq!(e.code, "action_uncertain");
    assert!(e.message.contains("cause=research_deadline"));
    assert!(e.message.contains("rejected_open_reads="));
    assert!(e
        .message
        .contains("root=research_panel; transitions=[1, 0]"));
    assert!(e.message.len() <= 512);
    assert_eq!(f.inner.dispatches, 1);
    assert_eq!(f.inner.calls, 3);
    assert!(Instant::now() >= f.dispatched_deadline.unwrap());
    assert!(j.blocks_game(&epoch().game));
    drop(j);
    let bytes = fs::read(scratch.journal()).unwrap();
    let mut j = Journal::open(&scratch.journal()).unwrap();
    let calls = f.inner.calls;
    for id in ["opening-timeout", "new-opening-id"] {
        assert_eq!(
            j.execute(
                id,
                t.clone(),
                &"a".repeat(32),
                &a,
                &gate,
                &mut f,
                Duration::from_secs(1)
            )
            .unwrap_err()
            .code,
            "action_uncertain"
        );
    }
    assert_eq!(f.inner.dispatches, 1);
    assert_eq!(f.inner.calls, calls);
    drop(j);
    assert_eq!(fs::read(scratch.journal()).unwrap(), bytes);
}
#[test]
fn unknown_animation_ownership_and_other_stages_never_gain_opening_wait_authority() {
    for code in [
        "research_precondition",
        "ownership_changed",
        "invalid_gui_type",
        "input_lease_required",
    ] {
        let scratch = Scratch::new();
        let mut j = Journal::open(&scratch.journal()).unwrap();
        let mut f = opener(vec![
            Ok(closed_map_state()),
            Ok(closed_map_state()),
            Err(pending("[1, 0]")),
            Err(error(
                code,
                "Unknown root state or changed original ownership",
            )),
            Ok(panel_state()),
        ]);
        let e = execute(
            &mut j,
            "strict-after",
            target(Stage::OpenPanel),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert_eq!(e.code, "action_uncertain");
        assert!(e.message.contains(&format!("cause={code}")));
        assert_eq!(f.calls, 4);
        assert_eq!(f.dispatches, 1);
        assert!(j.blocks_game(&epoch().game));
    }
    let scratch = Scratch::new();
    let mut j = Journal::open(&scratch.journal()).unwrap();
    let mut f = Fake::new();
    f.observations = VecDeque::from([
        Ok(state()),
        Ok(state()),
        Err(pending("[1, 0]")),
        Ok(reached()),
    ]);
    assert_eq!(
        execute(
            &mut j,
            "other-stage",
            target(Stage::Research),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(2)
        )
        .unwrap_err()
        .code,
        "action_uncertain"
    );
    assert_eq!(f.calls, 3);
    assert_eq!(f.dispatches, 1);
}
#[test]
fn input_authority_loss_during_opening_wait_stops_before_another_read_or_dispatch() {
    struct Revoked<'a> {
        inner: Fake,
        gate: &'a RwLock<bool>,
    }
    impl Backend for Revoked<'_> {
        fn observe(&mut self) -> Result<OwnState> {
            let result = self.inner.observe();
            if self.inner.calls == 3 {
                *self.gate.write().unwrap() = false;
            }
            result
        }
        fn candidate(&mut self, t: &Target) -> Result<Candidate> {
            self.inner.candidate(t)
        }
        fn dispatch_once(&mut self, c: &Candidate, d: Instant) -> Delivery {
            self.inner.dispatch_once(c, d)
        }
        fn confirm_input_idle(&mut self, c: &Candidate, d: Instant) -> Result<()> {
            self.inner.confirm_input_idle(c, d)
        }
    }
    let scratch = Scratch::new();
    let mut j = Journal::open(&scratch.journal()).unwrap();
    let a = authority();
    let gate = RwLock::new(true);
    let mut f = Revoked {
        inner: opener(vec![
            Ok(closed_map_state()),
            Ok(closed_map_state()),
            Err(pending("[1, 0]")),
            Ok(panel_state()),
        ]),
        gate: &gate,
    };
    let e = j
        .execute(
            "revoked-opening",
            target(Stage::OpenPanel),
            &"a".repeat(32),
            &a,
            &gate,
            &mut f,
            Duration::from_secs(2),
        )
        .unwrap_err();
    assert_eq!(e.code, "action_uncertain");
    assert!(e.message.contains("cause=session_closed"));
    assert_eq!(f.inner.calls, 3);
    assert_eq!(f.inner.dispatches, 1);
    assert!(j.blocks_game(&epoch().game));
}
