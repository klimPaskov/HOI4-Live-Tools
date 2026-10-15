use super::*;
use std::{
    collections::VecDeque,
    fs,
    sync::atomic::{AtomicUsize, Ordering},
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
        selected_focus: None,
        requested_detail_ready: Some(false),
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
}
impl Fake {
    fn new(stage: Stage) -> Self {
        Self {
            queue: VecDeque::new(),
            last: state(23),
            candidate: if stage == Stage::ClosePolitics {
                Candidate::ClosePolitics
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
        if let Some(r) = self.queue.pop_front() {
            if let Ok(s) = &r {
                self.last = s.clone();
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
    let transport = serde_json::json!({"delivery":"not_queued","error":{"code":"torn_read","message":"Focus witness changed at final queue check"}});
    let detailed = with_transport_cause(original, Some(&transport));
    assert_eq!(detailed.code, "input_not_queued");
    assert!(detailed
        .message
        .starts_with("torn_read: Focus witness changed at final queue check;"));
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
#[test]
fn crashed_intent_reopens_and_blocks_all_stage_ids_and_game() {
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let a = authority();
    let g = RwLock::new(true);
    let mut f = Fake::new(Stage::View);
    f.last = state(13);
    f.crash = true;
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = run(
            &mut j,
            "crashed",
            target(Stage::View),
            &a,
            &g,
            &mut f,
            Duration::from_secs(1),
        );
    }))
    .is_err());
    drop(j);
    let mut j = Journal::open(&sc.path()).unwrap();
    assert!(j.blocks_game(&epoch().game));
    f.crash = false;
    for stage in [
        Stage::OpenPolitics,
        Stage::OpenFocus,
        Stage::View,
        Stage::Start,
    ] {
        assert_eq!(
            run(
                &mut j,
                "new-id",
                target(stage),
                &a,
                &g,
                &mut f,
                Duration::from_millis(30)
            )
            .unwrap_err()
            .code,
            "action_uncertain"
        );
    }
    assert_eq!(f.dispatches, 1);
}
#[test]
fn postdispatch_input_idle_or_expired_readback_stays_quarantined() {
    for idle in [false, true] {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let a = authority();
        let g = RwLock::new(true);
        let mut f = Fake::sequence(Stage::Start, view(), started());
        f.idle = idle;
        f.expire_readback = idle;
        let timeout = Duration::from_secs(5);
        assert!(run(
            &mut j,
            "uncertain",
            target(Stage::Start),
            &a,
            &g,
            &mut f,
            timeout
        )
        .is_err());
        assert_eq!(f.dispatches, 1);
        assert!(j.blocks_game(&epoch().game));
    }
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

#[test]
fn late_verified_terminal_quarantine_retains_receipt_and_survives_reopen() {
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
    j.finish_before_deadline(&execution, Instant::now() + Duration::from_secs(1))
        .unwrap();
    assert!(!j.blocks_game(&epoch().game));
    assert_eq!(
        j.finish_before_deadline(&execution, Instant::now())
            .unwrap_err()
            .code,
        "action_uncertain"
    );
    assert_eq!(j.receipts.get("late"), Some(&execution.receipt));
    assert!(j.blocks_game(&epoch().game));
    drop(j);
    let mut j = Journal::open(&sc.path()).unwrap();
    assert!(j.blocks_game(&epoch().game));
    for id in ["late", "new-id"] {
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
    assert_eq!(f.dispatches, 1);
    assert_eq!(
        j.failure_evidence(
            "new-id",
            &target(Stage::OpenPolitics),
            &error("input_busy", "busy")
        ),
        FailureEvidence::Unknown
    );
    drop(j);
    let text = std::fs::read_to_string(sc.path())
        .unwrap()
        .replace("original absolute", "forged absolute");
    // Both bounded reason and original receipt are checksum-bound.
    let mut records: Vec<serde_json::Value> = text
        .lines()
        .map(|v| serde_json::from_str(v).unwrap())
        .collect();
    records.last_mut().unwrap()["receipt_hash"] = serde_json::json!("0".repeat(64));
    std::fs::write(
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
fn late_verified_marker_write_failure_poison_retains_actual_evidence() {
    let sc = Scratch::new();
    let mut j = Journal::open(&sc.path()).unwrap();
    let a = authority();
    let gate = RwLock::new(true);
    let mut f = Fake::sequence(Stage::OpenPolitics, state(23), state(12));
    let execution = run(
        &mut j,
        "write-failure",
        target(Stage::OpenPolitics),
        &a,
        &gate,
        &mut f,
        Duration::from_secs(2),
    )
    .unwrap();
    j.file = File::open(sc.path()).unwrap();
    let e = j
        .finish_before_deadline(&execution, Instant::now())
        .unwrap_err();
    assert_eq!(e.code, "action_uncertain");
    assert!(e.to_string().contains("persistence failed"));
    assert!(j.poisoned && j.blocks_game(&epoch().game));
    assert_eq!(j.receipts.get("write-failure"), Some(&execution.receipt));
    assert_eq!(
        run(
            &mut j,
            "new",
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
    assert_eq!(f.dispatches, 1);
    // No committed marker: do not claim independent reopen knows the failure.
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
                "action_uncertain"
            } else {
                "focus_precondition"
            }
        );
        assert_eq!(f.dispatches, usize::from(after_input));
        drop(j);
        let j = Journal::open(&sc.path()).unwrap();
        assert_eq!(j.blocks_game(&epoch().game), after_input);
    }
}
#[test]
fn close_politics_unknown_delivery_missing_idle_and_late_read_quarantine_no_replay() {
    for failure in 0..3 {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let mut f = Fake::sequence(Stage::ClosePolitics, close_state(12), close_state(23));
        match failure {
            0 => {
                f.delivery = Delivery::Unknown;
                f.queue.pop_back();
            }
            1 => f.idle = false,
            _ => f.expire_readback = true,
        };
        // Unknown delivery with unchanged mode12 has no successful postcondition.
        let e = run(
            &mut j,
            "uncertain",
            target(Stage::ClosePolitics),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_millis(300),
        )
        .unwrap_err();
        assert_eq!(e.code, "action_uncertain");
        assert_eq!(f.dispatches, 1);
        drop(j);
        let mut j = Journal::open(&sc.path()).unwrap();
        assert!(j.blocks_game(&epoch().game));
        assert!(run(
            &mut j,
            "other",
            target(Stage::OpenPolitics),
            &authority(),
            &RwLock::new(true),
            &mut f,
            Duration::from_secs(1)
        )
        .is_err());
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
fn detailed_postinput_refusal_retains_intent_and_blocks_all_ids_after_restart() {
    for case in 0..3 {
        let sc = Scratch::new();
        let mut j = Journal::open(&sc.path()).unwrap();
        let a = authority();
        let g = RwLock::new(true);
        let t = target(Stage::OpenFocus);
        let mut f = Fake::sequence(Stage::OpenFocus, state(12), state(13));
        let cause = match case {
            0 => {
                f.queue[2] = Err(error("invalid_focus_owner", "Generated own root mismatch"));
                "invalid_focus_owner: Generated own root mismatch"
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
            "uncertain",
            t.clone(),
            &a,
            &g,
            &mut f,
            Duration::from_secs(2),
        )
        .unwrap_err();
        assert_eq!(e.code, "action_uncertain");
        assert!(e.message.contains(cause), "{}", e.message);
        assert_eq!(
            j.failure_evidence("uncertain", &t, &e),
            FailureEvidence::Unknown
        );
        assert_eq!(f.dispatches, 1);
        let bytes = fs::read(sc.path()).unwrap();
        assert_eq!(bytes.iter().filter(|b| **b == b'\n').count(), 1);
        drop(j);
        let mut j = Journal::open(&sc.path()).unwrap();
        assert!(j.blocks_game(&epoch().game));
        for id in ["uncertain", "different-id"] {
            assert_eq!(
                run(
                    &mut j,
                    id,
                    t.clone(),
                    &a,
                    &g,
                    &mut f,
                    Duration::from_secs(2)
                )
                .unwrap_err()
                .code,
                "action_uncertain"
            );
        }
        assert_eq!(f.dispatches, 1);
        assert_eq!(fs::read(sc.path()).unwrap(), bytes);
    }
}
