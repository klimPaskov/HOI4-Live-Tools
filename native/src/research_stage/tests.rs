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
            "hoi4-stage-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn journal(&self) -> std::path::PathBuf {
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
        campaign: "fixture-epoch".into(),
        player: "GER".into(),
    }
}
fn target(stage: Stage) -> Target {
    Target {
        epoch: epoch(),
        stage,
        technology: "support_weapons".into(),
        slot: 0,
    }
}
fn authority() -> InputAuthority {
    let mut a = InputAuthority::new("luna".into(), epoch()).unwrap();
    a.acquire("luna", "a".repeat(32), 300, &epoch()).unwrap();
    a
}
fn state() -> OwnState {
    OwnState {
        epoch: epoch(),
        selected_slot: Some(0),
        tree_visible: true,
        researching: vec![None, None, None, None],
        visible_info: vec![Some("support_weapons".into())],
    }
}
fn reached() -> OwnState {
    let mut s = state();
    s.researching[0] = Some("support_weapons".into());
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
    observations: VecDeque<Result<OwnState>>,
    last: OwnState,
    calls: usize,
    dispatches: usize,
    delivery: Delivery,
    candidate: PointCandidate,
    crash: bool,
    slow: Duration,
    input_idle: bool,
}
impl Fake {
    fn new() -> Self {
        Self {
            observations: VecDeque::new(),
            last: state(),
            calls: 0,
            dispatches: 0,
            delivery: Delivery::Unknown,
            candidate: point(),
            crash: false,
            slow: Duration::ZERO,
            input_idle: true,
        }
    }
    fn successful() -> Self {
        let mut f = Self::new();
        f.observations = VecDeque::from([
            Ok(state()),
            Ok(state()),
            Ok(state()),
            Ok(state()),
            Ok(reached()),
        ]);
        f
    }
}
impl Backend for Fake {
    fn confirm_input_idle(&mut self, _: &PointCandidate, _: Instant) -> Result<()> {
        if self.input_idle {
            Ok(())
        } else {
            Err(error("input_unavailable", "Generated held input"))
        }
    }
    fn observe(&mut self) -> Result<OwnState> {
        self.calls += 1;
        thread::sleep(self.slow);
        if let Some(result) = self.observations.pop_front() {
            if let Ok(s) = &result {
                self.last = s.clone();
            }
            result
        } else {
            Ok(self.last.clone())
        }
    }
    fn candidate(&mut self, _: &Target) -> Result<PointCandidate> {
        Ok(self.candidate.clone())
    }
    fn dispatch_once(&mut self, _: &PointCandidate, _: Instant) -> Delivery {
        self.dispatches += 1;
        if self.crash {
            panic!("simulated loss after possible input");
        }
        self.delivery
    }
}
fn execute(
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
fn unknown_delivery_three_readbacks_verify_once_and_replay_is_historical() {
    let scratch = Scratch::new();
    let a = authority();
    let g = RwLock::new(true);
    let mut f = Fake::successful();
    let mut j = Journal::open(&scratch.journal()).unwrap();
    let r = execute(
        &mut j,
        "research-1",
        target(Stage::Research),
        &a,
        &g,
        &mut f,
        Duration::from_secs(1),
    )
    .unwrap();
    assert!(r.current_state_verified && !r.historical && !r.event_causality_verified);
    assert_eq!(f.dispatches, 1);
    assert_eq!(f.calls, 5);
    assert_eq!(r.receipt.delivery, Some(Delivery::Unknown));
    drop(j);
    let mut j = Journal::open(&scratch.journal()).unwrap();
    let replay = execute(
        &mut j,
        "research-1",
        target(Stage::Research),
        &a,
        &g,
        &mut f,
        Duration::from_secs(1),
    )
    .unwrap();
    assert!(replay.historical && !replay.current_state_verified);
    assert_eq!(f.calls, 5);
    assert_eq!(f.dispatches, 1);
    assert_eq!(
        execute(
            &mut j,
            "research-1",
            target(Stage::View),
            &a,
            &g,
            &mut f,
            Duration::from_secs(1)
        )
        .unwrap_err()
        .code,
        "request_id_conflict"
    );
}
#[test]
fn crash_intent_survives_restart_and_blocks_new_id_and_stage() {
    let scratch = Scratch::new();
    let a = authority();
    let g = RwLock::new(true);
    let mut f = Fake::new();
    f.crash = true;
    let mut j = Journal::open(&scratch.journal()).unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| execute(
            &mut j,
            "view-1",
            target(Stage::Research),
            &a,
            &g,
            &mut f,
            Duration::from_secs(1)
        )))
        .is_err()
    );
    drop(j);
    let mut j = Journal::open(&scratch.journal()).unwrap();
    f.crash = false;
    for (id, stage) in [
        ("view-1", Stage::Research),
        ("fresh", Stage::Research),
        ("other-stage", Stage::View),
    ] {
        assert_eq!(
            execute(
                &mut j,
                id,
                target(stage),
                &a,
                &g,
                &mut f,
                Duration::from_secs(1)
            )
            .unwrap_err()
            .code,
            "action_uncertain"
        );
    }
    assert_eq!(f.dispatches, 1);
}
#[test]
fn timeout_and_foreign_readback_quarantine_without_input_replay() {
    for foreign in [false, true] {
        let scratch = Scratch::new();
        let a = authority();
        let g = RwLock::new(true);
        let mut f = Fake::new();
        if foreign {
            let mut s = state();
            s.epoch.player = "CHI".into();
            f.observations = VecDeque::from([Ok(state()), Ok(state()), Ok(s)]);
        }
        let mut j = Journal::open(&scratch.journal()).unwrap();
        assert_eq!(
            execute(
                &mut j,
                "attempt",
                target(Stage::Research),
                &a,
                &g,
                &mut f,
                Duration::from_millis(45)
            )
            .unwrap_err()
            .code,
            "action_uncertain"
        );
        assert!(j.blocks_game(&epoch().game));
        assert_eq!(f.dispatches, 1);
        assert_eq!(
            execute(
                &mut j,
                "fresh",
                target(Stage::Research),
                &a,
                &g,
                &mut f,
                Duration::from_secs(1)
            )
            .unwrap_err()
            .code,
            "action_uncertain"
        );
        assert_eq!(f.dispatches, 1);
    }
}
#[test]
fn preflight_gate_lease_and_invalid_candidate_leave_plan_retryable() {
    for case in 0..5 {
        let scratch = Scratch::new();
        let mut a = authority();
        let g = RwLock::new(case != 0);
        let mut f = Fake::new();
        if case == 1 {
            a.release(&"a".repeat(32), &epoch()).unwrap();
        }
        if case == 2 {
            f.candidate.client_pixel = [-1, 35];
        }
        if case == 3 {
            f.candidate.expected_client = [0, 288];
        }
        if case == 4 {
            f.last.visible_info = vec![Some("other_technology".into())];
        }
        let mut j = Journal::open(&scratch.journal()).unwrap();
        assert!(execute(
            &mut j,
            "attempt",
            target(Stage::Research),
            &a,
            &g,
            &mut f,
            Duration::from_secs(1)
        )
        .is_err());
        assert_eq!(f.dispatches, 0);
        assert!(!j.blocks_game(&epoch().game));
        assert_eq!(j.file.metadata().unwrap().len(), 0);
    }
}
#[test]
fn known_not_queued_is_durable_and_new_token_can_retry() {
    let scratch = Scratch::new();
    let a = authority();
    let g = RwLock::new(true);
    let mut f = Fake::new();
    f.delivery = Delivery::NotQueued;
    let mut j = Journal::open(&scratch.journal()).unwrap();
    assert_eq!(
        execute(
            &mut j,
            "first",
            target(Stage::Research),
            &a,
            &g,
            &mut f,
            Duration::from_secs(1)
        )
        .unwrap_err()
        .code,
        "input_not_queued"
    );
    drop(j);
    let mut j = Journal::open(&scratch.journal()).unwrap();
    assert!(!j.blocks_game(&epoch().game));
    assert_eq!(
        execute(
            &mut j,
            "first",
            target(Stage::Research),
            &a,
            &g,
            &mut f,
            Duration::from_secs(1)
        )
        .unwrap_err()
        .code,
        "input_not_queued"
    );
    assert_eq!(f.dispatches, 1);
    f = Fake::successful();
    execute(
        &mut j,
        "fresh",
        target(Stage::Research),
        &a,
        &g,
        &mut f,
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(f.dispatches, 1);
}
#[test]
fn late_noop_cannot_claim_fresh_success_and_terminal_capacity_is_reserved() {
    let scratch = Scratch::new();
    let a = authority();
    let g = RwLock::new(true);
    let mut f = Fake::new();
    f.last = reached();
    f.slow = Duration::from_millis(30);
    let mut j = Journal::open(&scratch.journal()).unwrap();
    assert_eq!(
        execute(
            &mut j,
            "late",
            target(Stage::Research),
            &a,
            &g,
            &mut f,
            Duration::from_millis(5)
        )
        .unwrap_err()
        .code,
        "research_deadline"
    );
    assert_eq!(j.file.metadata().unwrap().len(), 0);
    f = Fake::new();
    j.file.set_len(LIMIT - LINE as u64).unwrap();
    assert_eq!(
        execute(
            &mut j,
            "full",
            target(Stage::Research),
            &a,
            &g,
            &mut f,
            Duration::from_secs(1)
        )
        .unwrap_err()
        .code,
        "journal_full"
    );
    assert_eq!(f.dispatches, 0);
}
#[test]
fn malformed_transitions_tail_and_exclusive_writer_refuse() {
    for delivery in [None, Some(Delivery::NotQueued)] {
        let scratch = Scratch::new();
        let mut j = Journal::open(&scratch.journal()).unwrap();
        let intent = Receipt {
            version: 1,
            request_id: "one".into(),
            target: target(Stage::Research),
            status: Status::Intent,
            delivery: None,
            observed: state(),
        };
        j.append(intent.clone()).unwrap();
        j.append(Receipt {
            status: Status::Verified,
            delivery,
            observed: reached(),
            ..intent
        })
        .unwrap();
        drop(j);
        assert!(matches!(Journal::open(&scratch.journal()),Err(e) if e.code=="invalid_journal"));
    }
    let scratch = Scratch::new();
    let j = Journal::open(&scratch.journal()).unwrap();
    assert!(matches!(Journal::open(&scratch.journal()),Err(e) if e.code=="journal_busy"));
    drop(j);
    OpenOptions::new()
        .append(true)
        .open(scratch.journal())
        .unwrap()
        .write_all(b"unfinished")
        .unwrap();
    assert!(matches!(Journal::open(&scratch.journal()),Err(e) if e.code=="invalid_journal"));
}
#[cfg(unix)]
#[test]
fn symlink_and_unprotected_existing_journal_refuse() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let scratch = Scratch::new();
    let p = scratch.0.join("actual");
    fs::write(&p, b"").unwrap();
    symlink(&p, scratch.journal()).unwrap();
    assert!(Journal::open(&scratch.journal()).is_err());
    fs::remove_file(scratch.journal()).unwrap();
    fs::write(scratch.journal(), b"").unwrap();
    fs::set_permissions(scratch.journal(), fs::Permissions::from_mode(0o644)).unwrap();
    assert!(matches!(Journal::open(&scratch.journal()),Err(e) if e.code=="invalid_journal"));
}

#[test]
fn canonical_assignment_without_input_idle_stays_quarantined() {
    let scratch = Scratch::new();
    let a = authority();
    let gate = RwLock::new(true);
    let mut f = Fake::successful();
    f.input_idle = false;
    let mut journal = Journal::open(&scratch.journal()).unwrap();
    assert_eq!(
        execute(
            &mut journal,
            "held",
            target(Stage::Research),
            &a,
            &gate,
            &mut f,
            Duration::from_secs(1)
        )
        .unwrap_err()
        .code,
        "action_uncertain"
    );
    assert_eq!(f.dispatches, 1);
    assert!(journal.blocks_game(&epoch().game));
    assert_eq!(
        execute(
            &mut journal,
            "new",
            target(Stage::Research),
            &a,
            &gate,
            &mut f,
            Duration::from_secs(1)
        )
        .unwrap_err()
        .code,
        "action_uncertain"
    );
    assert_eq!(f.dispatches, 1);
}
