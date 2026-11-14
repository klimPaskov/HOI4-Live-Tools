use super::*;
mod search_navigation;
mod search_query;
use std::{
    collections::VecDeque,
    fs,
    sync::atomic::{AtomicUsize, Ordering},
    thread,
};
static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "focus-stage-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn path(&self) -> std::path::PathBuf {
        self.0.join("journal")
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn epoch() -> Epoch {
    Epoch {
        game: Identity {
            pid: 120,
            start_ticks: "456".into(),
            exe: "/fixture/game".into(),
            uid: 1000,
        },
        campaign: "campaign".into(),
        player: "GER".into(),
    }
}
fn target(stage: Stage) -> Target {
    Target {
        query: None,
        epoch: epoch(),
        stage,
        focus_id: "GER_rhineland".into(),
    }
}
fn authority() -> InputAuthority {
    let mut a = InputAuthority::new("luna".into(), epoch()).unwrap();
    a.acquire("luna", "a".repeat(32), 300, &epoch()).unwrap();
    a
}
fn state(mode: i32) -> OwnState {
    OwnState {
        epoch: epoch(),
        active_focus: ActiveFocus::Idle,
        canonical_focus_id: Some("GER_rhineland".into()),
        managed_mode: Some(mode),
        politics_root: if mode == 12 {
            LocalRoot::Ready
        } else {
            LocalRoot::Hidden
        },
        focus_root: if mode == 13 {
            LocalRoot::Ready
        } else {
            LocalRoot::Hidden
        },
        own_detail_counts: Some([0, 0]),
        own_research_info_counts: Some([0, 0]),
        research_panel_root: LocalRoot::Unknown,
        research_tree_root: LocalRoot::Unknown,
        selected_focus: None,
        requested_detail_ready: Some(false),
        search_control: None,
    }
}
fn view() -> OwnState {
    let mut s = state(13);
    s.own_detail_counts = Some([1, 0]);
    s.selected_focus = Some("GER_rhineland".into());
    s.requested_detail_ready = Some(true);
    s
}
fn started() -> OwnState {
    let mut s = state(23);
    s.active_focus = ActiveFocus::Active("GER_rhineland".into());
    s
}
fn search_state(focused: bool) -> OwnState {
    let mut s = state(13);
    s.research_panel_root = LocalRoot::Hidden;
    s.research_tree_root = LocalRoot::Hidden;
    s.search_control = Some(SearchControl {
        locally_ready: true,
        empty_query: true,
        enabled: true,
        keyboard_focus_verified: focused,
        text_receiver_verified: focused,
        query_text: Some(String::new()),
        requested_selection_verified: Some(false),
        matches: None,
    });
    s
}

#[test]
fn search_field_preserves_active_focus_and_replays_only_historical_receipt() {
    for active in [ActiveFocus::Idle, ActiveFocus::Active("GER_other".into())] {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let a = authority();
        let g = RwLock::new(true);
        let t = target(Stage::SearchField);
        let mut before = search_state(false);
        before.active_focus = active.clone();
        let mut after = search_state(true);
        after.active_focus = active;
        let mut f = Fake::sequence(t.stage, before, after);
        let r = run(
            &mut j,
            "field",
            t.clone(),
            &a,
            &g,
            &mut f,
            Duration::from_secs(1),
        )
        .unwrap();
        assert!(r.current_state_verified && !r.historical);
        assert_eq!(f.dispatches, 1);
        drop(j);
        let mut j = Journal::open(&sc.path()).unwrap();
        let r = run(
            &mut j,
            "field",
            t.clone(),
            &a,
            &g,
            &mut f,
            Duration::from_secs(1),
        )
        .unwrap();
        assert!(r.historical && !r.current_state_verified);
        assert_eq!(f.dispatches, 1);
        // Already focused is not permission for another click under a new ID.
        assert_eq!(
            run(
                &mut j,
                "new-field",
                t,
                &a,
                &g,
                &mut f,
                Duration::from_secs(1)
            )
            .unwrap_err()
            .code,
            "focus_precondition"
        );
        assert_eq!(f.dispatches, 1);
    }
}

#[test]
fn search_field_requires_empty_enabled_settled_control_and_both_receivers() {
    for field in 0..10 {
        let mut before = search_state(false);
        match field {
            0 => before.search_control = None,
            1 => before.search_control.as_mut().unwrap().empty_query = false,
            2 => before.search_control.as_mut().unwrap().enabled = false,
            3 => before.search_control.as_mut().unwrap().locally_ready = false,
            4 => {
                before
                    .search_control
                    .as_mut()
                    .unwrap()
                    .keyboard_focus_verified = true
            }
            5 => {
                before
                    .search_control
                    .as_mut()
                    .unwrap()
                    .text_receiver_verified = true
            }
            6 => before.research_panel_root = LocalRoot::Ready,
            7 => before.own_detail_counts = Some([1, 0]),
            8 => before.focus_root = LocalRoot::Animating,
            _ => before.research_tree_root = LocalRoot::Unknown,
        }
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let mut f = Fake::sequence(Stage::SearchField, before, search_state(true));
        assert_eq!(
            run(
                &mut j,
                "bad",
                target(Stage::SearchField),
                &authority(),
                &RwLock::new(true),
                &mut f,
                Duration::from_secs(1)
            )
            .unwrap_err()
            .code,
            "focus_precondition"
        );
        assert_eq!(f.dispatches, 0);
        assert_eq!(fs::metadata(sc.path()).unwrap().len(), 0);
    }
    for field in 0..4 {
        let mut after = search_state(true);
        match field {
            0 => {
                after
                    .search_control
                    .as_mut()
                    .unwrap()
                    .keyboard_focus_verified = false
            }
            1 => {
                after
                    .search_control
                    .as_mut()
                    .unwrap()
                    .text_receiver_verified = false
            }
            2 => after.search_control.as_mut().unwrap().empty_query = false,
            _ => after.active_focus = ActiveFocus::Active("GER_other".into()),
        }
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let a = authority();
        let g = RwLock::new(true);
        let t = target(Stage::SearchField);
        let mut f = Fake::sequence(t.stage, search_state(false), after);
        // End after one rejected post-state, independently of journal fsync or
        // scheduler latency. A 50 ms wall deadline can expire before dispatch.
        f.queue
            .push_back(Err(error("fixture_end", "End unchanged post-state")));
        assert_eq!(
            run(
                &mut j,
                "uncertain",
                t.clone(),
                &a,
                &g,
                &mut f,
                Duration::from_secs(5)
            )
            .unwrap_err()
            .code,
            "action_unconfirmed"
        );
        assert_eq!(f.dispatches, 1);
        drop(j);
        // Closed: the game is not held, and a new request reaches the backend again.
        let mut j = Journal::open(&sc.path()).unwrap();
        assert!(!j.blocks_game(&t.epoch.game));
        assert!(matches!(
            j.receipts["uncertain"].status,
            Status::Unconfirmed | Status::Diverged
        ));
        assert_eq!(
            run(&mut j, "new", t, &a, &g, &mut f, Duration::from_secs(1))
                .unwrap_err()
                .code,
            "fixture_end"
        );
        assert_eq!(f.dispatches, 1);
    }
}

#[test]
fn older_focus_receipts_keep_their_serialized_shape() {
    let s = state(13);
    let mut v = serde_json::to_value(&s).unwrap();
    assert!(v.get("search_control").is_none());
    assert_eq!(serde_json::from_value::<OwnState>(v.clone()).unwrap(), s);
    v["search_control"] = serde_json::json!({"empty_query":true});
    assert!(serde_json::from_value::<OwnState>(v).is_err());
}
fn point() -> PointCandidate {
    PointCandidate {
        ideal_logical: [951, 173],
        quantized_logical: [950, 175],
        client_pixel: [190, 35],
        expected_client: [512, 288],
        current_frame_verified: false,
        effective_routing_verified: false,
        input_authorized: false,
    }
}
struct Fake {
    queue: VecDeque<Result<OwnState>>,
    last: OwnState,
    candidate: Candidate,
    dispatches: usize,
    delivery: Delivery,
    idle: bool,
    crash: bool,
    slow: Duration,
    post_delay: Duration,
    expire_readback: bool,
    dispatched_deadline: Option<Instant>,
    /// A queued `fixture_end` error was reached: every later read fails the same way, so an
    /// outcome that stays unproven closes after one settlement window whatever the deadline.
    ended: bool,
}
impl Fake {
    fn new(stage: Stage) -> Self {
        Self {
            queue: VecDeque::new(),
            last: state(23),
            candidate: if stage == Stage::ClosePolitics {
                Candidate::ClosePolitics
            } else if stage == Stage::OpenPoliticsFromResearch {
                Candidate::OpenPoliticsFromResearch
            } else if stage == Stage::OpenPolitics {
                Candidate::OpenPolitics
            } else {
                Candidate::Point(point())
            },
            dispatches: 0,
            delivery: Delivery::ServerConfirmed,
            idle: true,
            crash: false,
            slow: Duration::ZERO,
            post_delay: Duration::ZERO,
            expire_readback: false,
            dispatched_deadline: None,
            ended: false,
        }
    }
    fn sequence(stage: Stage, a: OwnState, b: OwnState) -> Self {
        let mut f = Self::new(stage);
        f.queue = VecDeque::from([Ok(a.clone()), Ok(a), Ok(b)]);
        f
    }
}
impl Backend for Fake {
    fn observe(&mut self) -> Result<OwnState> {
        if self.expire_readback && self.dispatches > 0 {
            thread::sleep(
                self.dispatched_deadline
                    .unwrap()
                    .saturating_duration_since(Instant::now())
                    + Duration::from_millis(1),
            );
        }
        thread::sleep(
            self.slow
                + if self.dispatches > 0 {
                    self.post_delay
                } else {
                    Duration::ZERO
                },
        );
        if self.ended {
            return Err(error("fixture_end", "End of fixture"));
        }
        if let Some(r) = self.queue.pop_front() {
            match &r {
                Ok(s) => self.last = s.clone(),
                Err(e) if e.code == "fixture_end" => self.ended = true,
                Err(_) => {}
            }
            r
        } else {
            Ok(self.last.clone())
        }
    }
    fn candidate(&mut self, _: &Target) -> Result<Candidate> {
        Ok(self.candidate.clone())
    }
    fn dispatch_once(&mut self, _: &Candidate, deadline: Instant) -> Delivery {
        self.dispatches += 1;
        self.dispatched_deadline = Some(deadline);
        if self.crash {
            panic!("simulated crash after intent");
        }
        self.delivery
    }
    fn confirm_input_idle(&mut self, _: &Candidate, _: Instant) -> Result<()> {
        if self.idle {
            Ok(())
        } else {
            Err(error("input_unavailable", "Generated uncertain idle"))
        }
    }
}
fn run(
    j: &mut Journal,
    id: &str,
    t: Target,
    a: &InputAuthority,
    g: &RwLock<bool>,
    f: &mut Fake,
    timeout: Duration,
) -> Result<Execution> {
    j.execute(id, t, &"a".repeat(32), a, g, f, timeout)
}
#[test]
fn nonqueued_transport_cause_survives_without_reopening_terminal_attempt() {
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let a = authority();
    let g = RwLock::new(true);
    let t = target(Stage::Start);
    let mut f = Fake::sequence(Stage::Start, view(), view());
    f.delivery = Delivery::NotQueued;
    let original = run(
        &mut j,
        "refused",
        t.clone(),
        &a,
        &g,
        &mut f,
        Duration::from_secs(1),
    )
    .unwrap_err();
    let transport = serde_json::json!({"delivery":"not_queued","error":{"code":"torn_read","message":"Focus witness changed at final queue check"},"preinput_rejected_reads":[{"code":"torn_read","message":"original discarded search field"},{"code":"torn_read","message":"final discarded search field"}]});
    let detailed = with_transport_cause(original, Some(&transport));
    assert_eq!(detailed.code, "input_not_queued");
    assert!(detailed
        .message
        .starts_with("torn_read: Focus witness changed at final queue check;"));
    assert!(detailed.message.contains("original discarded search field"));
    assert!(detailed.message.contains("final discarded search field"));
    assert_eq!(
        j.failure_evidence("refused", &t, &detailed),
        FailureEvidence::ConfirmedNotQueued
    );
    #[cfg(unix)]
    assert!(!crate::daily_focus_navigation::retry_before_intent(
        &detailed,
        FailureEvidence::ConfirmedNotQueued,
        1
    ));
    assert_eq!(f.dispatches, 1);
    drop(j);
    let mut j = Journal::open(&sc.path()).unwrap();
    assert_eq!(
        run(&mut j, "refused", t, &a, &g, &mut f, Duration::from_secs(1))
            .unwrap_err()
            .code,
        "input_not_queued"
    );
    assert_eq!(f.dispatches, 1);
    let unchanged = with_transport_cause(error("action_uncertain", "original"), None);
    assert_eq!(
        (unchanged.code, unchanged.message.as_str()),
        ("action_uncertain", "original")
    );
}
#[test]
fn all_four_stages_use_distinct_canonical_postconditions_once() {
    for (stage, before, after) in [
        (Stage::OpenPolitics, state(23), state(12)),
        (Stage::OpenFocus, state(12), state(13)),
        (Stage::View, state(13), view()),
        (Stage::Start, view(), started()),
    ] {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let a = authority();
        let g = RwLock::new(true);
        let mut f = Fake::sequence(stage, before, after);
        let r = run(
            &mut j,
            "stage",
            target(stage),
            &a,
            &g,
            &mut f,
            Duration::from_secs(1),
        )
        .unwrap();
        assert!(r.current_state_verified && !r.historical && !r.event_causality_verified);
        assert_eq!(f.dispatches, 1);
    }
}
#[test]
fn verified_replay_is_historical_but_new_id_already_reached_is_fresh_noop() {
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let a = authority();
    let g = RwLock::new(true);
    let mut f = Fake::sequence(Stage::Start, view(), started());
    run(
        &mut j,
        "start",
        target(Stage::Start),
        &a,
        &g,
        &mut f,
        Duration::from_secs(1),
    )
    .unwrap();
    drop(j);
    let mut j = Journal::open(&sc.path()).unwrap();
    let r = run(
        &mut j,
        "start",
        target(Stage::Start),
        &a,
        &g,
        &mut f,
        Duration::from_secs(1),
    )
    .unwrap();
    assert!(r.historical && !r.current_state_verified);
    let r = run(
        &mut j,
        "distinct",
        target(Stage::Start),
        &a,
        &g,
        &mut f,
        Duration::from_secs(1),
    )
    .unwrap();
    assert!(!r.historical && r.current_state_verified);
    assert_eq!(r.receipt.delivery, None);
    assert_eq!(f.dispatches, 1);
}
#[test]
fn explicit_unknown_idle_active_serialization_and_late_unknown_refuse() {
    let unknown = serde_json::to_string(&ActiveFocus::Unknown).unwrap();
    let idle = serde_json::to_string(&ActiveFocus::Idle).unwrap();
    assert_ne!(unknown, idle);
    for mut late in [state(13), view()] {
        late.active_focus = ActiveFocus::Unknown;
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let a = authority();
        let g = RwLock::new(true);
        let mut f = Fake::new(Stage::Start);
        f.queue = VecDeque::from([Ok(view()), Ok(late)]);
        assert!(run(
            &mut j,
            "late",
            target(Stage::Start),
            &a,
            &g,
            &mut f,
            Duration::from_secs(1)
        )
        .is_err());
        assert_eq!(f.dispatches, 0);
        assert!(!j.blocks_game(&epoch().game));
    }
}
#[test]
fn local_unknown_blockers_selection_and_busy_focus_keep_plan_unfinished() {
    for change in 0..5 {
        let mut s = view();
        match change {
            0 => s.active_focus = ActiveFocus::Active("other_focus".into()),
            1 => s.own_research_info_counts = None,
            2 => s.own_detail_counts = Some([1, 1]),
            3 => s.selected_focus = Some("other_focus".into()),
            _ => s.focus_root = LocalRoot::Animating,
        };
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let a = authority();
        let g = RwLock::new(true);
        let mut f = Fake::new(Stage::Start);
        f.last = s;
        assert!(run(
            &mut j,
            "refused",
            target(Stage::Start),
            &a,
            &g,
            &mut f,
            Duration::from_millis(50)
        )
        .is_err());
        assert_eq!(f.dispatches, 0);
        assert!(!j.blocks_game(&epoch().game));
    }
}
/// Rewrite every record of the journal file through `edit` (as another build or time would
/// have left it).
fn rewrite_records(path: &std::path::Path, edit: impl Fn(&mut serde_json::Value)) {
    let text: String = fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| {
            let mut record: serde_json::Value = serde_json::from_str(line).unwrap();
            edit(&mut record);
            format!("{record}\n")
        })
        .collect();
    fs::write(path, text).unwrap();
}
fn crash_after_intent(sc: &Scratch) -> Fake {
    let mut j = Journal::open(&sc.path()).unwrap();
    let mut f = Fake::new(Stage::View);
    f.last = state(13);
    f.crash = true;
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = run(
            &mut j,
            "crashed",
            target(Stage::View),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(1),
        );
    }))
    .is_err());
    f.crash = false;
    f
}
#[test]
fn crashed_intent_holds_its_game_only_while_its_input_may_be_in_flight() {
    let sc = Scratch::new();
    let a = authority();
    let g = RwLock::new(true);
    let mut f = crash_after_intent(&sc);
    let mut j = Journal::open(&sc.path()).unwrap();
    assert_eq!(j.receipts["crashed"].status, Status::Intent);
    assert!(j.receipts["crashed"].unix_ms.is_some());
    assert!(j.blocks_game(&epoch().game));
    for (id, stage) in [
        ("new-id", Stage::OpenPolitics),
        ("new-id", Stage::OpenFocus),
        ("new-id", Stage::View),
        ("crashed", Stage::View),
    ] {
        let e = run(
            &mut j,
            id,
            target(stage),
            &a,
            &g,
            &mut f,
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(e.code, "action_uncertain");
        assert!(e.message.contains("at most 10 s"), "{}", e.message);
    }
    assert_eq!(f.dispatches, 1);
    assert_eq!(j.receipts["crashed"].status, Status::Intent);
}
#[test]
fn an_interrupted_intent_past_the_orphan_bound_is_closed_by_the_next_call() {
    // `aged`: written by this build, the bound has passed. Otherwise: written by an older
    // build, which recorded no time at all.
    for aged in [true, false] {
        let sc = Scratch::new();
        let a = authority();
        let g = RwLock::new(true);
        let mut f = crash_after_intent(&sc);
        rewrite_records(&sc.path(), |record| {
            if aged {
                record["unix_ms"] =
                    serde_json::json!(stage_settle::now_ms() - stage_settle::ORPHAN_MS);
            } else {
                record.as_object_mut().unwrap().remove("unix_ms");
            }
        });
        let mut j = Journal::open(&sc.path()).unwrap();
        assert_eq!(j.receipts["crashed"].status, Status::Intent);
        assert!(!j.blocks_game(&epoch().game));
        // A new request closes the orphan without reading for it and then runs normally.
        f.queue = VecDeque::from([Ok(state(13)), Ok(state(13)), Ok(view())]);
        let execution = run(
            &mut j,
            "next",
            target(Stage::View),
            &a,
            &g,
            &mut f,
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(execution.receipt.status, Status::Verified);
        assert_eq!(f.dispatches, 2);
        let closed = &j.receipts["crashed"];
        assert_eq!(closed.status, Status::Unconfirmed);
        assert_eq!(closed.delivery, None);
        assert!(closed.closed.as_deref().unwrap().starts_with("orphaned"));
        // The interrupted request itself answers from that receipt: no read, no input.
        f.ended = true;
        let e = run(
            &mut j,
            "crashed",
            target(Stage::View),
            &a,
            &g,
            &mut f,
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(e.code, "action_unconfirmed");
        assert_eq!(f.dispatches, 2);
        drop(j);
        let j = Journal::open(&sc.path()).unwrap();
        assert_eq!(j.receipts["crashed"].status, Status::Unconfirmed);
        assert!(!j.blocks_game(&epoch().game));
    }
}
#[test]
fn a_retry_of_an_interrupted_request_past_the_bound_closes_it_without_reading() {
    let sc = Scratch::new();
    let mut f = crash_after_intent(&sc);
    rewrite_records(&sc.path(), |record| {
        record.as_object_mut().unwrap().remove("unix_ms");
    });
    let mut j = Journal::open(&sc.path()).unwrap();
    f.ended = true;
    let e = run(
        &mut j,
        "crashed",
        target(Stage::View),
        &authority(),
        &RwLock::new(true),
        &mut f,
        Duration::from_secs(1),
    )
    .unwrap_err();
    assert_eq!(e.code, "action_unconfirmed");
    assert!(e.message.contains("orphaned"), "{}", e.message);
    assert_eq!(f.dispatches, 1);
    assert_eq!(j.receipts["crashed"].status, Status::Unconfirmed);
}
#[test]
fn an_unproven_start_closes_unconfirmed_and_the_next_request_is_accepted() {
    // The target shows but input idle never confirms; or nothing readable follows the click.
    for idle_missing in [true, false] {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let a = authority();
        let g = RwLock::new(true);
        let mut f = if idle_missing {
            Fake::sequence(Stage::Start, view(), started())
        } else {
            Fake::sequence(Stage::Start, view(), view())
        };
        f.idle = !idle_missing;
        if !idle_missing {
            f.queue.push_back(Err(error(
                "fixture_end",
                "Nothing readable after the click",
            )));
        }
        let e = run(
            &mut j,
            "unproven",
            target(Stage::Start),
            &a,
            &g,
            &mut f,
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(e.code, "action_unconfirmed");
        assert!(
            e.message.contains(if idle_missing {
                "input_unavailable: Generated uncertain idle"
            } else {
                "fixture_end"
            }),
            "{}",
            e.message
        );
        assert_eq!(f.dispatches, 1);
        assert!(!j.blocks_game(&epoch().game));
        let closed = j.receipts["unproven"].clone();
        assert_eq!(closed.status, Status::Unconfirmed);
        assert_eq!(closed.delivery, Some(Delivery::ServerConfirmed));
        assert_eq!(
            j.failure_evidence("unproven", &target(Stage::Start), &e),
            FailureEvidence::Unknown
        );
        // The same id answers from its receipt: no observation, no dispatch.
        f.ended = true;
        let replay = run(
            &mut j,
            "unproven",
            target(Stage::Start),
            &a,
            &g,
            &mut f,
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(replay.code, "action_unconfirmed");
        assert_eq!(f.dispatches, 1);
        assert_eq!(j.receipts["unproven"], closed);
        // A new id for the same game is accepted, also after a restart: fresh state decides.
        drop(j);
        let mut j = Journal::open(&sc.path()).unwrap();
        assert!(!j.blocks_game(&epoch().game));
        f.ended = false;
        f.idle = true;
        f.queue = VecDeque::from([Ok(view()), Ok(view()), Ok(started())]);
        let execution = run(
            &mut j,
            "fresh",
            target(Stage::Start),
            &a,
            &g,
            &mut f,
            Duration::from_secs(5),
        )
        .unwrap();
        assert_eq!(execution.receipt.status, Status::Verified);
        assert_eq!(f.dispatches, 2);
    }
}
#[test]
fn a_readback_that_arrives_after_the_deadline_is_still_verified() {
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let mut f = Fake::sequence(Stage::Start, view(), started());
    // The first read after the click returns only once the action's deadline has passed.
    f.expire_readback = true;
    let execution = run(
        &mut j,
        "late",
        target(Stage::Start),
        &authority(),
        &RwLock::new(true),
        &mut f,
        Duration::from_secs(1),
    )
    .unwrap();
    assert!(Instant::now() >= f.dispatched_deadline.unwrap());
    assert_eq!(execution.receipt.status, Status::Verified);
    assert!(execution.current_state_verified);
    assert_eq!(f.dispatches, 1);
    assert!(!j.blocks_game(&epoch().game));
}
#[test]
fn a_closing_session_ends_the_observation_at_once_and_the_request_still_closes() {
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let gate = RwLock::new(true);
    // The session closes while the first read after the click is under way.
    struct Closing<'a>(Fake, &'a RwLock<bool>);
    impl Backend for Closing<'_> {
        fn observe(&mut self) -> Result<OwnState> {
            if self.0.dispatches > 0 {
                *self.1.write().unwrap() = false;
                return Err(error("torn_read", "Session closing under the read"));
            }
            self.0.observe()
        }
        fn candidate(&mut self, target: &Target) -> Result<Candidate> {
            self.0.candidate(target)
        }
        fn dispatch_once(&mut self, candidate: &Candidate, deadline: Instant) -> Delivery {
            self.0.dispatch_once(candidate, deadline)
        }
        fn confirm_input_idle(&mut self, candidate: &Candidate, deadline: Instant) -> Result<()> {
            self.0.confirm_input_idle(candidate, deadline)
        }
    }
    let mut f = Closing(Fake::sequence(Stage::Start, view(), started()), &gate);
    let e = j
        .execute(
            "closing",
            target(Stage::Start),
            &"a".repeat(32),
            &authority(),
            &gate,
            &mut f,
            Duration::from_secs(5),
        )
        .unwrap_err();
    assert_eq!(e.code, "action_unconfirmed");
    assert!(e.message.contains("observation stopped"), "{}", e.message);
    assert_eq!(f.0.dispatches, 1);
    // Two reads before the click; the queued target state was never read.
    assert_eq!(f.0.queue.len(), 1);
    assert_eq!(j.receipts["closing"].status, Status::Unconfirmed);
}
#[test]
fn known_notqueued_is_terminal_nonreplay_and_new_id_can_retry() {
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let a = authority();
    let g = RwLock::new(true);
    let mut f = Fake::new(Stage::View);
    f.last = state(13);
    f.delivery = Delivery::NotQueued;
    assert_eq!(
        run(
            &mut j,
            "refused",
            target(Stage::View),
            &a,
            &g,
            &mut f,
            Duration::from_secs(1)
        )
        .unwrap_err()
        .code,
        "input_not_queued"
    );
    assert!(!j.blocks_game(&epoch().game));
    f = Fake::sequence(Stage::View, state(13), view());
    assert_eq!(
        run(
            &mut j,
            "refused",
            target(Stage::View),
            &a,
            &g,
            &mut f,
            Duration::from_secs(1)
        )
        .unwrap_err()
        .code,
        "input_not_queued"
    );
    assert_eq!(f.dispatches, 0);
    assert!(run(
        &mut j,
        "retry",
        target(Stage::View),
        &a,
        &g,
        &mut f,
        Duration::from_secs(1)
    )
    .is_ok());
    assert_eq!(f.dispatches, 1);
}
#[test]
fn wrong_kind_authority_gate_invalid_id_and_foreign_epoch_never_dispatch() {
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let a = authority();
    let mut f = Fake::new(Stage::View);
    f.last = state(13);
    f.candidate = Candidate::OpenPolitics;
    assert!(run(
        &mut j,
        "wrongkind",
        target(Stage::View),
        &a,
        &RwLock::new(true),
        &mut f,
        Duration::from_secs(1)
    )
    .is_err());
    assert!(run(
        &mut j,
        "closed",
        target(Stage::View),
        &a,
        &RwLock::new(false),
        &mut f,
        Duration::from_secs(1)
    )
    .is_err());
    let mut t = target(Stage::View);
    t.focus_id = "../invalid".into();
    assert!(run(
        &mut j,
        "invalid",
        t,
        &a,
        &RwLock::new(true),
        &mut f,
        Duration::from_secs(1)
    )
    .is_err());
    let mut t = target(Stage::View);
    t.epoch.game.start_ticks = "foreign".into();
    assert!(run(
        &mut j,
        "foreign",
        t,
        &a,
        &RwLock::new(true),
        &mut f,
        Duration::from_secs(1)
    )
    .is_err());
    assert_eq!(f.dispatches, 0);
}

#[test]
fn missing_wrong_and_late_canonical_join_never_dispatch() {
    for (n, late, id) in [
        (0, false, None),
        (1, false, Some("GER_other")),
        (2, true, None),
    ] {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let mut f = Fake::new(Stage::View);
        let good = state(13);
        let mut bad = good.clone();
        bad.canonical_focus_id = id.map(str::to_owned);
        f.queue = if late {
            VecDeque::from([Ok(good), Ok(bad)])
        } else {
            VecDeque::from([Ok(bad)])
        };
        let e = run(
            &mut j,
            &format!("canonical{n}"),
            target(Stage::View),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(1),
        )
        .unwrap_err();
        assert_eq!(e.code, "focus_precondition");
        assert_eq!(f.dispatches, 0);
        assert!(!j.blocks_game(&epoch().game));
    }
}

/// The marker an older build appended when a verified result arrived after its deadline.
fn old_quarantine_marker(receipt: &Receipt) -> TerminalQuarantine {
    let mut marker = TerminalQuarantine {
        version: 1,
        kind: "FocusTerminalQuarantine".into(),
        request_id: receipt.request_id.clone(),
        target: receipt.target.clone(),
        code: "action_uncertain".into(),
        reason: "Focus result arrived after original absolute deadline".into(),
        receipt_hash: receipt_hash(receipt).unwrap(),
        checksum: String::new(),
    };
    marker.checksum = marker.checksum().unwrap();
    marker
}
#[test]
fn a_journal_with_an_old_terminal_quarantine_marker_loads_and_no_longer_blocks() {
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let a = authority();
    let gate = RwLock::new(true);
    let mut f = Fake::sequence(Stage::OpenPolitics, state(23), state(12));
    let execution = run(
        &mut j,
        "late",
        target(Stage::OpenPolitics),
        &a,
        &gate,
        &mut f,
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(f.dispatches, 1);
    drop(j);
    // As the older build left the file: receipts without a time, then the marker.
    rewrite_records(&sc.path(), |record| {
        record.as_object_mut().unwrap().remove("unix_ms");
    });
    let mut old = execution.receipt.clone();
    old.unix_ms = None;
    let mut text = fs::read_to_string(sc.path()).unwrap();
    text.push_str(&serde_json::to_string(&old_quarantine_marker(&old)).unwrap());
    text.push('\n');
    fs::write(sc.path(), &text).unwrap();
    let mut j = Journal::open(&sc.path()).unwrap();
    assert!(!j.blocks_game(&epoch().game));
    // The late result is what it always was: verified. Its id replays the receipt.
    let replay = run(
        &mut j,
        "late",
        target(Stage::OpenPolitics),
        &a,
        &gate,
        &mut f,
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(replay.historical);
    assert_eq!(replay.receipt, old);
    // New work for the same game is accepted and its receipts follow the marker.
    f.queue = VecDeque::from([Ok(state(12)), Ok(state(12)), Ok(state(13))]);
    f.candidate = Candidate::Point(point());
    let next = run(
        &mut j,
        "new-id",
        target(Stage::OpenFocus),
        &a,
        &gate,
        &mut f,
        Duration::from_secs(5),
    )
    .unwrap();
    assert_eq!(next.receipt.status, Status::Verified);
    assert_eq!(f.dispatches, 2);
    drop(j);
    let j = Journal::open(&sc.path()).unwrap();
    assert_eq!(j.receipts["new-id"].status, Status::Verified);
    assert!(!j.blocks_game(&epoch().game));
    drop(j);
    // The marker stays checksum-bound to its receipt: a forged one is still refused.
    let mut records: Vec<serde_json::Value> = text
        .lines()
        .map(|v| serde_json::from_str(v).unwrap())
        .collect();
    records.last_mut().unwrap()["receipt_hash"] = serde_json::json!("0".repeat(64));
    fs::write(
        sc.path(),
        records
            .iter()
            .map(|v| format!("{}\n", v))
            .collect::<String>(),
    )
    .unwrap();
    assert_eq!(
        Journal::open(&sc.path()).err().unwrap().code,
        "invalid_journal"
    );
}

#[test]
fn a_receipt_that_cannot_be_written_still_blocks_everything() {
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let a = authority();
    let gate = RwLock::new(true);
    let mut f = Fake::sequence(Stage::OpenPolitics, state(23), state(12));
    // A handle that cannot append: the intent is never durable, so nothing is sent.
    j.file = File::open(sc.path()).unwrap();
    assert!(run(
        &mut j,
        "write-failure",
        target(Stage::OpenPolitics),
        &a,
        &gate,
        &mut f,
        Duration::from_secs(2),
    )
    .is_err());
    assert_eq!(f.dispatches, 0);
    assert!(j.poisoned && j.blocks_game(&epoch().game));
    // Not an uncertain action: no time bound and no new id clears a persistence failure.
    f.queue = VecDeque::from([Ok(state(23)), Ok(state(23)), Ok(state(12))]);
    for id in ["write-failure", "new"] {
        assert_eq!(
            run(
                &mut j,
                id,
                target(Stage::OpenPolitics),
                &a,
                &gate,
                &mut f,
                Duration::from_secs(2)
            )
            .unwrap_err()
            .code,
            "action_uncertain"
        );
    }
    assert_eq!(f.dispatches, 0);
    assert_eq!(
        j.failure_evidence(
            "new",
            &target(Stage::OpenPolitics),
            &error("input_busy", "busy")
        ),
        FailureEvidence::Unknown
    );
}
fn close_state(mode: i32) -> OwnState {
    let mut s = state(mode);
    s.active_focus = ActiveFocus::Active("GER_rhineland".into());
    s
}
#[test]
fn close_politics_one_typed_q_preserves_exact_active_and_reopens_without_replay() {
    {
        let active = ActiveFocus::Active("GER_rhineland".into());
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let mut before = close_state(12);
        before.active_focus = active.clone();
        let mut after = close_state(23);
        after.active_focus = active.clone();
        let mut f = Fake::sequence(Stage::ClosePolitics, before, after);
        let e = run(
            &mut j,
            "close",
            target(Stage::ClosePolitics),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(f.dispatches, 1);
        assert_eq!(e.receipt.observed.active_focus, active);
        assert_eq!(e.receipt.observed.managed_mode, Some(23));
        assert!(!e.event_causality_verified);
        drop(j);
        let mut j = Journal::open(&sc.path()).unwrap();
        let e = run(
            &mut j,
            "close",
            target(Stage::ClosePolitics),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(2),
        )
        .unwrap();
        assert!(e.historical && !e.current_state_verified);
        assert_eq!(f.dispatches, 1);
        // Already hidden mode23 cannot supply a new close attempt/no-op receipt.
        assert!(run(
            &mut j,
            "new",
            target(Stage::ClosePolitics),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(2)
        )
        .is_err());
        assert_eq!(f.dispatches, 1);
    }
}
#[test]
fn close_politics_unknown_wrong_mode_roots_and_complete_blockers_never_dispatch() {
    let mut bad = Vec::new();
    for mode in [6, 13, 23] {
        bad.push(close_state(mode));
    }
    let mut s = close_state(12);
    s.active_focus = ActiveFocus::Unknown;
    bad.push(s);
    for root in [LocalRoot::Unknown, LocalRoot::Animating, LocalRoot::Hidden] {
        let mut s = close_state(12);
        s.politics_root = root;
        bad.push(s);
    }
    for root in [LocalRoot::Unknown, LocalRoot::Animating, LocalRoot::Ready] {
        let mut s = close_state(12);
        s.focus_root = root;
        bad.push(s);
    }
    for counts in [None, Some([1, 0]), Some([0, 1])] {
        let mut s = close_state(12);
        s.own_detail_counts = counts;
        bad.push(s);
        let mut s = close_state(12);
        s.own_research_info_counts = counts;
        bad.push(s);
    }
    let mut s = close_state(12);
    s.canonical_focus_id = None;
    bad.push(s);
    let mut s = close_state(12);
    s.epoch.game.start_ticks = "foreign".into();
    bad.push(s);
    for (index, s) in bad.into_iter().enumerate() {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let mut f = Fake::sequence(Stage::ClosePolitics, s, close_state(23));
        assert!(run(
            &mut j,
            &format!("bad{index}"),
            target(Stage::ClosePolitics),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(2)
        )
        .is_err());
        assert_eq!(f.dispatches, 0);
        assert!(!j.blocks_game(&epoch().game));
    }
}
#[test]
fn close_politics_wrong_candidate_late_pre_change_and_post_change_are_distinct() {
    for candidate in [Candidate::OpenPolitics, Candidate::Point(point())] {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let mut f = Fake::sequence(Stage::ClosePolitics, close_state(12), close_state(23));
        f.candidate = candidate;
        assert!(run(
            &mut j,
            "wrong",
            target(Stage::ClosePolitics),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(2)
        )
        .is_err());
        assert_eq!(f.dispatches, 0);
    }
    for after_input in [false, true] {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let mut changed = close_state(if after_input { 23 } else { 12 });
        changed.active_focus = ActiveFocus::Active("GER_other".into());
        let mut f = Fake::new(Stage::ClosePolitics);
        f.queue = if after_input {
            VecDeque::from([Ok(close_state(12)), Ok(close_state(12)), Ok(changed)])
        } else {
            VecDeque::from([Ok(close_state(12)), Ok(changed)])
        };
        let e = run(
            &mut j,
            "changed",
            target(Stage::ClosePolitics),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert_eq!(
            e.code,
            if after_input {
                "action_unconfirmed"
            } else {
                "focus_precondition"
            }
        );
        assert_eq!(f.dispatches, usize::from(after_input));
        drop(j);
        // The focus changed under the close key: recorded as diverged, closed, not held.
        let j = Journal::open(&sc.path()).unwrap();
        assert!(!j.blocks_game(&epoch().game));
        assert_eq!(
            j.receipts.get("changed").map(|r| r.status),
            after_input.then_some(Status::Diverged)
        );
        if after_input {
            assert!(e.message.contains("diverged="), "{}", e.message);
        }
    }
}
#[test]
fn close_politics_unknown_delivery_or_missing_idle_closes_unconfirmed_without_replay() {
    for idle_missing in [false, true] {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let mut f = Fake::sequence(Stage::ClosePolitics, close_state(12), close_state(23));
        if idle_missing {
            f.idle = false;
        } else {
            // Unknown delivery and the panel stays open: no postcondition ever shows.
            f.delivery = Delivery::Unknown;
            f.queue.pop_back();
            f.queue
                .push_back(Err(error("fixture_end", "Nothing readable after the key")));
        }
        let e = run(
            &mut j,
            "unproven",
            target(Stage::ClosePolitics),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(e.code, "action_unconfirmed");
        assert_eq!(f.dispatches, 1);
        drop(j);
        let mut j = Journal::open(&sc.path()).unwrap();
        assert!(!j.blocks_game(&epoch().game));
        assert_eq!(j.receipts["unproven"].status, Status::Unconfirmed);
        // Never replayed: the same id answers from the receipt, without a read or a key.
        f.ended = true;
        assert_eq!(
            run(
                &mut j,
                "unproven",
                target(Stage::ClosePolitics),
                &authority(),
                &RwLock::new(true),
                &mut f,
                Duration::from_secs(1)
            )
            .unwrap_err()
            .code,
            "action_unconfirmed"
        );
        // Another stage is not refused by the journal: it gets as far as reading the game.
        assert_eq!(
            run(
                &mut j,
                "other",
                target(Stage::OpenPolitics),
                &authority(),
                &RwLock::new(true),
                &mut f,
                Duration::from_secs(1)
            )
            .unwrap_err()
            .code,
            "fixture_end"
        );
        assert_eq!(f.dispatches, 1);
    }
}
#[test]
fn close_politics_forged_terminal_active_focus_cannot_reopen() {
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let mut f = Fake::sequence(Stage::ClosePolitics, close_state(12), close_state(23));
    run(
        &mut j,
        "close",
        target(Stage::ClosePolitics),
        &authority(),
        &RwLock::new(true),
        &mut f,
        Duration::from_secs(2),
    )
    .unwrap();
    drop(j);
    let mut lines: Vec<Receipt> = std::fs::read_to_string(sc.path())
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    lines.last_mut().unwrap().observed.active_focus = ActiveFocus::Active("GER_other".into());
    std::fs::write(
        sc.path(),
        lines
            .into_iter()
            .map(|r| format!("{}\n", serde_json::to_string(&r).unwrap()))
            .collect::<String>(),
    )
    .unwrap();
    assert_eq!(
        Journal::open(&sc.path()).err().unwrap().code,
        "invalid_journal"
    );
}

#[test]
fn close_politics_idle_or_other_canonical_active_refuses_before_candidate_transport_and_intent() {
    for active in [
        ActiveFocus::Idle,
        ActiveFocus::Unknown,
        ActiveFocus::Active("GER_other".into()),
    ] {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let mut before = close_state(12);
        before.active_focus = active;
        assert!(!before.reached(&target(Stage::ClosePolitics)));
        let mut f = Fake::sequence(Stage::ClosePolitics, before, close_state(23));
        assert!(run(
            &mut j,
            "active-required",
            target(Stage::ClosePolitics),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(2)
        )
        .is_err());
        assert_eq!(f.dispatches, 0);
        assert!(j.receipts.is_empty());
        assert!(!j.blocks_game(&epoch().game));
        assert_eq!(std::fs::metadata(sc.path()).unwrap().len(), 0);
    }
}
#[test]
fn close_politics_forged_idle_intent_cannot_reopen_as_positive_active() {
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let mut f = Fake::sequence(Stage::ClosePolitics, close_state(12), close_state(23));
    run(
        &mut j,
        "close",
        target(Stage::ClosePolitics),
        &authority(),
        &RwLock::new(true),
        &mut f,
        Duration::from_secs(2),
    )
    .unwrap();
    drop(j);
    let mut lines: Vec<Receipt> = std::fs::read_to_string(sc.path())
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    lines[0].observed.active_focus = ActiveFocus::Idle;
    std::fs::write(
        sc.path(),
        lines
            .into_iter()
            .map(|r| format!("{}\n", serde_json::to_string(&r).unwrap()))
            .collect::<String>(),
    )
    .unwrap();
    assert_eq!(
        Journal::open(&sc.path()).err().unwrap().code,
        "invalid_journal"
    );
}

#[test]
fn detailed_postinput_refusal_closes_with_its_cause_and_is_never_replayed() {
    for case in 0..3 {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let a = authority();
        let g = RwLock::new(true);
        let t = target(Stage::OpenFocus);
        let mut f = Fake::sequence(Stage::OpenFocus, state(12), state(13));
        let cause = match case {
            0 => {
                f.queue[2] = Err(error("fixture_end", "Generated own root mismatch"));
                "fixture_end: Generated own root mismatch"
            }
            1 => {
                let mut wrong = state(13);
                wrong.canonical_focus_id = Some("other_focus".into());
                f.queue[2] = Ok(wrong);
                "focus_precondition: Fresh canonical own focus join required"
            }
            _ => {
                f.idle = false;
                "input_unavailable: Generated uncertain idle"
            }
        };
        let e = run(
            &mut j,
            "unproven",
            t.clone(),
            &a,
            &g,
            &mut f,
            Duration::from_secs(5),
        )
        .unwrap_err();
        assert_eq!(e.code, "action_unconfirmed");
        if case != 0 {
            assert!(e.message.contains(cause), "{}", e.message);
        }
        assert_eq!(
            j.failure_evidence("unproven", &t, &e),
            FailureEvidence::Unknown
        );
        assert_eq!(f.dispatches, 1);
        // Windows enforces the journal's byte-range lock on a second reader.
        // Inspect persisted bytes only after releasing the owning journal.
        drop(j);
        let bytes = fs::read(sc.path()).unwrap();
        assert_eq!(bytes.iter().filter(|b| **b == b'\n').count(), 2);
        let mut j = Journal::open(&sc.path()).unwrap();
        assert!(!j.blocks_game(&epoch().game));
        let closed = &j.receipts["unproven"];
        assert_eq!(closed.status, Status::Unconfirmed);
        assert!(
            closed
                .closed
                .as_deref()
                .unwrap()
                .contains(cause.split(": ").next().unwrap()),
            "{closed:?}"
        );
        f.ended = true;
        assert_eq!(
            run(
                &mut j,
                "unproven",
                t.clone(),
                &a,
                &g,
                &mut f,
                Duration::from_secs(2)
            )
            .unwrap_err()
            .code,
            "action_unconfirmed"
        );
        assert_eq!(f.dispatches, 1);
        drop(j);
        assert_eq!(fs::read(sc.path()).unwrap(), bytes);
    }
}

mod research_return;
