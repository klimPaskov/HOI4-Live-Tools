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
            candidate: if stage == Stage::OpenPolitics {
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
