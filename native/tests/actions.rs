use hoi4_live_native::{
    actions::{Action, Backend, Epoch, Journal, State, Status},
    error::{Error, Result},
    platform::Identity,
};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

struct Temp(PathBuf);
impl Temp {
    fn new(name: &str) -> Self {
        let p = std::env::temp_dir().join(format!(
            "hoi4-native-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&p).unwrap();
        Self(p)
    }
    fn path(&self) -> PathBuf {
        self.0.join("receipts.jsonl")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
struct Fake {
    state: State,
    dispatches: usize,
    preflight_error: bool,
    lose_epoch: bool,
    dispatch_error: bool,
    no_change: bool,
    flip_pause_preflight: bool,
}
impl Fake {
    fn new() -> Self {
        Self {
            state: State {
                epoch: Epoch {
                    game: Identity {
                        pid: 123,
                        start_ticks: "55".into(),
                        exe: "owned-test-game".into(),
                        uid: 1000,
                    },
                    campaign: "fresh-run".into(),
                    player: "GER".into(),
                },
                paused: false,
                speed: 4,
            },
            dispatches: 0,
            preflight_error: false,
            lose_epoch: false,
            dispatch_error: false,
            no_change: false,
            flip_pause_preflight: false,
        }
    }
}
impl Backend for Fake {
    fn observe(&mut self) -> Result<State> {
        Ok(self.state.clone())
    }
    fn preflight(&mut self, _: &Action, _: &State) -> Result<()> {
        if self.flip_pause_preflight {
            self.state.paused = !self.state.paused;
        }
        if self.preflight_error {
            Err(Error::new("focus_lost", "Focused window unavailable"))
        } else {
            Ok(())
        }
    }
    fn dispatch(&mut self, action: &Action, _: &State) -> Result<()> {
        self.dispatches += 1;
        if self.lose_epoch {
            self.state.epoch.campaign = "loaded-another-campaign".into();
        }
        if !self.no_change {
            match action {
                Action::SetPaused(v) => self.state.paused = *v,
                Action::SetSpeed(v) => self.state.speed = *v,
            }
        }
        if self.dispatch_error {
            Err(Error::new("transport_closed", "Outcome unknown"))
        } else {
            Ok(())
        }
    }
}
const TIMEOUT: Duration = Duration::from_millis(1);
#[test]
fn preflight_state_change_cannot_commit_a_stale_noop() {
    let temp = Temp::new("preflight-change");
    let mut backend = Fake::new();
    backend.state.paused = true;
    backend.flip_pause_preflight = true;
    let epoch = backend.state.epoch.clone();
    let mut journal = Journal::open(&temp.path()).unwrap();
    let receipt = journal
        .execute(
            "pause",
            &epoch,
            Action::SetPaused(true),
            &mut backend,
            TIMEOUT,
        )
        .unwrap();
    assert_eq!(receipt.status, Status::Verified);
    assert!(receipt.dispatched);
    assert!(receipt.observed.paused);
    assert_eq!(backend.dispatches, 1);
}
#[test]
fn verified_receipt_survives_restart_without_second_dispatch() {
    let temp = Temp::new("verified");
    let mut backend = Fake::new();
    let epoch = backend.state.epoch.clone();
    let mut journal = Journal::open(&temp.path()).unwrap();
    let receipt = journal
        .execute(
            "plan/pause",
            &epoch,
            Action::SetPaused(true),
            &mut backend,
            TIMEOUT,
        )
        .unwrap();
    assert_eq!(receipt.status, Status::Verified);
    assert!(receipt.dispatched);
    assert_eq!(backend.dispatches, 1);
    drop(journal);
    // Game state changes later. Same request returns its original historical
    // receipt; it must not reinterpret the desired action against current state.
    backend.state.paused = false;
    let mut reopened = Journal::open(&temp.path()).unwrap();
    assert_eq!(
        reopened
            .execute(
                "plan/pause",
                &epoch,
                Action::SetPaused(true),
                &mut backend,
                TIMEOUT
            )
            .unwrap(),
        receipt
    );
    assert_eq!(backend.dispatches, 1);
    assert_eq!(
        reopened
            .execute(
                "plan/pause",
                &epoch,
                Action::SetPaused(false),
                &mut backend,
                TIMEOUT
            )
            .unwrap_err()
            .code,
        "idempotency_conflict"
    );
}
#[test]
fn crash_after_dispatch_cannot_be_replayed_or_bypassed_but_settles_by_reading() {
    let temp = Temp::new("crash");
    let mut backend = Fake::new();
    backend.dispatch_error = true;
    let epoch = backend.state.epoch.clone();
    let mut journal = Journal::open(&temp.path()).unwrap();
    assert_eq!(
        journal
            .execute(
                "pause",
                &epoch,
                Action::SetPaused(true),
                &mut backend,
                TIMEOUT
            )
            .unwrap_err()
            .code,
        "action_uncertain"
    );
    assert!(backend.state.paused);
    assert_eq!(backend.dispatches, 1);
    drop(journal);
    backend.dispatch_error = false;
    let mut reopened = Journal::open(&temp.path()).unwrap();
    assert_eq!(
        reopened
            .execute(
                "new-id",
                &epoch,
                Action::SetPaused(true),
                &mut backend,
                TIMEOUT
            )
            .unwrap_err()
            .code,
        "action_uncertain"
    );
    // A changed campaign identifier cannot evade quarantine for the same PID.
    backend.state.epoch.campaign = "new-campaign".into();
    let changed = backend.state.epoch.clone();
    assert_eq!(
        reopened
            .execute(
                "another-id",
                &changed,
                Action::SetPaused(true),
                &mut backend,
                TIMEOUT
            )
            .unwrap_err()
            .code,
        "action_uncertain"
    );
    assert_eq!(backend.dispatches, 1);
    // Retrying the uncertain request itself settles it from fresh state,
    // without sending the key again, and lifts the quarantine.
    backend.state.epoch = epoch.clone();
    let settled = reopened
        .execute(
            "pause",
            &epoch,
            Action::SetPaused(true),
            &mut backend,
            TIMEOUT,
        )
        .unwrap();
    assert_eq!(settled.status, Status::Verified);
    assert_eq!(backend.dispatches, 1);
    backend.state.paused = false;
    assert_eq!(
        reopened
            .execute(
                "pause-2",
                &epoch,
                Action::SetPaused(true),
                &mut backend,
                TIMEOUT
            )
            .unwrap()
            .status,
        Status::Verified
    );
    assert_eq!(backend.dispatches, 2);
}
#[test]
fn uncertain_dispatch_that_did_not_apply_settles_as_not_applied() {
    let temp = Temp::new("notapplied");
    let mut backend = Fake::new();
    backend.dispatch_error = true;
    let epoch = backend.state.epoch.clone();
    let mut journal = Journal::open(&temp.path()).unwrap();
    assert_eq!(
        journal
            .execute(
                "pause",
                &epoch,
                Action::SetPaused(true),
                &mut backend,
                TIMEOUT
            )
            .unwrap_err()
            .code,
        "action_uncertain"
    );
    backend.state.paused = false;
    backend.dispatch_error = false;
    let settled = journal
        .execute(
            "pause",
            &epoch,
            Action::SetPaused(true),
            &mut backend,
            TIMEOUT,
        )
        .unwrap();
    assert_eq!(settled.status, Status::NotApplied);
    assert_eq!(backend.dispatches, 1);
}
#[test]
fn failed_preflight_leaves_plan_entry_retryable_and_target_noop_is_recorded() {
    let temp = Temp::new("preflight");
    let mut backend = Fake::new();
    let epoch = backend.state.epoch.clone();
    let mut journal = Journal::open(&temp.path()).unwrap();
    backend.preflight_error = true;
    assert_eq!(
        journal
            .execute(
                "pause",
                &epoch,
                Action::SetPaused(true),
                &mut backend,
                TIMEOUT
            )
            .unwrap_err()
            .code,
        "focus_lost"
    );
    assert!(journal.receipt("pause").is_none());
    assert_eq!(backend.dispatches, 0);
    backend.preflight_error = false;
    backend.state.paused = true;
    let receipt = journal
        .execute(
            "pause",
            &epoch,
            Action::SetPaused(true),
            &mut backend,
            TIMEOUT,
        )
        .unwrap();
    assert_eq!(receipt.status, Status::Verified);
    assert!(!receipt.dispatched);
    assert_eq!(backend.dispatches, 0);
    drop(journal);
    assert_eq!(
        Journal::open(&temp.path()).unwrap().receipt("pause"),
        Some(&receipt)
    );
}
#[test]
fn changed_epoch_after_dispatch_and_unobserved_target_are_uncertain() {
    for lose_epoch in [false, true] {
        let temp = Temp::new("epoch");
        let mut backend = Fake::new();
        backend.lose_epoch = lose_epoch;
        backend.no_change = !lose_epoch;
        let epoch = backend.state.epoch.clone();
        let mut journal = Journal::open(&temp.path()).unwrap();
        assert_eq!(
            journal
                .execute(
                    "pause",
                    &epoch,
                    Action::SetPaused(true),
                    &mut backend,
                    TIMEOUT
                )
                .unwrap_err()
                .code,
            "action_uncertain"
        );
        assert_eq!(
            journal.receipt("pause").unwrap().status,
            Status::Dispatching
        );
        assert_eq!(backend.dispatches, 1);
    }
}
#[test]
fn exclusive_writer_and_corrupt_tail_fail_before_any_input() {
    let temp = Temp::new("lock");
    let journal = Journal::open(&temp.path()).unwrap();
    assert_eq!(
        Journal::open(&temp.path()).err().unwrap().code,
        "journal_busy"
    );
    drop(journal);
    for bytes in [b"{partial".to_vec(), b"{}\n".to_vec(), vec![b'x'; 16385]] {
        fs::write(temp.path(), bytes).unwrap();
        assert!(Journal::open(&temp.path()).is_err());
    }
}
#[test]
fn invalid_speed_and_reused_process_epoch_refuse_without_dispatch() {
    let temp = Temp::new("invalid");
    let mut backend = Fake::new();
    let mut epoch = backend.state.epoch.clone();
    let mut journal = Journal::open(&temp.path()).unwrap();
    assert_eq!(
        journal
            .execute("speed", &epoch, Action::SetSpeed(5), &mut backend, TIMEOUT)
            .unwrap_err()
            .code,
        "invalid_action"
    );
    epoch.game.start_ticks = "replacement-process".into();
    assert_eq!(
        journal
            .execute(
                "pause",
                &epoch,
                Action::SetPaused(true),
                &mut backend,
                TIMEOUT
            )
            .unwrap_err()
            .code,
        "ownership_changed"
    );
    assert_eq!(backend.dispatches, 0);
    assert!(journal.receipt("pause").is_none());
}
