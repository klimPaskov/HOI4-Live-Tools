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
    observe_error: Option<&'static str>,
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
            observe_error: None,
        }
    }
}
impl Backend for Fake {
    fn observe(&mut self) -> Result<State> {
        if let Some(code) = self.observe_error {
            return Err(Error::new(code, "Clock not observable"));
        }
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
/// An open request left by a server that stopped right after it sent the key.
fn stopped_mid_dispatch(temp: &Temp, backend: &Fake, action: &str) {
    let epoch = serde_json::to_value(&backend.state.epoch).unwrap();
    let mut before = serde_json::to_value(&backend.state).unwrap();
    before["paused"] = serde_json::json!(false);
    let line = serde_json::json!({
        "version": 1, "id": "pause", "epoch": epoch,
        "action": {"kind": action, "value": true},
        "status": "Dispatching", "dispatched": true, "observed": before,
    });
    fs::write(temp.path(), format!("{line}\n")).unwrap();
}
#[test]
fn a_dispatch_error_whose_key_took_effect_is_verified_from_fresh_state() {
    let temp = Temp::new("late-effect");
    let mut backend = Fake::new();
    backend.dispatch_error = true; // the transport reports failure, the key still applied
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
    assert!(backend.state.paused);
    assert_eq!(backend.dispatches, 1);
}
#[test]
fn a_request_left_open_by_a_stopped_server_settles_by_reading_and_blocks_nothing() {
    // The key applied before the server stopped: the next request of any id settles the open
    // one as Verified without sending it again, then runs normally.
    let temp = Temp::new("crash");
    let mut backend = Fake::new();
    stopped_mid_dispatch(&temp, &backend, "SetPaused");
    backend.state.paused = true;
    let epoch = backend.state.epoch.clone();
    let mut reopened = Journal::open(&temp.path()).unwrap();
    assert!(reopened.blocks_game_for_test(&epoch));
    let other = reopened
        .execute("speed", &epoch, Action::SetSpeed(3), &mut backend, TIMEOUT)
        .unwrap();
    assert_eq!(other.status, Status::Verified);
    assert_eq!(reopened.receipt("pause").unwrap().status, Status::Verified);
    assert!(!reopened.blocks_game_for_test(&epoch));
    assert_eq!(backend.dispatches, 1, "only the new speed key was sent");
    // Its own id replays the settled receipt; a different action on it still conflicts.
    assert_eq!(
        reopened
            .execute(
                "pause",
                &epoch,
                Action::SetPaused(true),
                &mut backend,
                TIMEOUT
            )
            .unwrap()
            .status,
        Status::Verified
    );
    assert_eq!(backend.dispatches, 1);
}
#[test]
fn an_open_request_whose_target_is_not_shown_closes_as_not_applied() {
    let temp = Temp::new("notapplied");
    let mut backend = Fake::new();
    stopped_mid_dispatch(&temp, &backend, "SetPaused");
    // Fresh state does not show the target (the key never applied, or it was undone).
    let epoch = backend.state.epoch.clone();
    let mut journal = Journal::open(&temp.path()).unwrap();
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
    assert_eq!(
        backend.dispatches, 0,
        "settlement never sends the key again"
    );
    // Closed for good and valid after a restart; a fresh id pauses normally.
    drop(journal);
    let mut reopened = Journal::open(&temp.path()).unwrap();
    assert_eq!(
        reopened.receipt("pause").unwrap().status,
        Status::NotApplied
    );
    let fresh = reopened
        .execute(
            "replacement",
            &epoch,
            Action::SetPaused(true),
            &mut backend,
            TIMEOUT,
        )
        .unwrap();
    assert_eq!(fresh.status, Status::Verified);
    assert_eq!(backend.dispatches, 1);
}
#[test]
fn an_unreadable_game_keeps_the_request_open_until_the_next_readable_call() {
    for code in [
        "invalid_target",
        "permission_denied",
        "ownership_changed",
        "torn_read",
    ] {
        let temp = Temp::new(code);
        let mut backend = Fake::new();
        stopped_mid_dispatch(&temp, &backend, "SetPaused");
        backend.state.paused = true;
        let epoch = backend.state.epoch.clone();
        let mut journal = Journal::open(&temp.path()).unwrap();
        backend.observe_error = Some(code);
        for id in ["pause", "another"] {
            let error = journal
                .execute(id, &epoch, Action::SetPaused(true), &mut backend, TIMEOUT)
                .unwrap_err();
            assert_eq!(error.code, "action_uncertain");
            assert!(error.message.contains(code));
        }
        assert_eq!(
            journal.receipt("pause").unwrap().status,
            Status::Dispatching
        );
        // The moment the game reads again, any request settles it.
        backend.observe_error = None;
        let settled = journal
            .execute(
                "another",
                &epoch,
                Action::SetPaused(true),
                &mut backend,
                TIMEOUT,
            )
            .unwrap();
        assert_eq!(settled.status, Status::Verified);
        assert_eq!(journal.receipt("pause").unwrap().status, Status::Verified);
        assert_eq!(backend.dispatches, 0);
    }
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
fn a_key_without_effect_closes_as_not_applied_and_a_lost_campaign_stays_open() {
    // The key was sent and the clock never changed: a clean, terminal failure.
    let temp = Temp::new("no-change");
    let mut backend = Fake::new();
    backend.no_change = true;
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
    assert_eq!(receipt.status, Status::NotApplied);
    assert!(receipt.dispatched);
    assert_eq!(backend.dispatches, 1);
    assert!(!journal.blocks_game_for_test(&epoch));
    backend.no_change = false;
    assert_eq!(
        journal
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
    // The campaign changed under the dispatch: nothing of this campaign can be read, so the
    // request stays open (and is never judged by another campaign's clock).
    let temp = Temp::new("epoch");
    let mut backend = Fake::new();
    backend.lose_epoch = true;
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
