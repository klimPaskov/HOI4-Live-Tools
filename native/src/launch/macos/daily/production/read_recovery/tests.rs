use super::super::proof_tests;
use super::*;
use crate::{construction_stage::Delivery, memory::ReadMemory};
use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

fn torn() -> Error {
    Error::new(
        "torn_read",
        "Snapshot witness changed; section=mac_production_geometry; read=7; size=8; changed_byte=0",
    )
}

#[test]
fn a_wholly_discarded_proof_can_recover_after_three_retries() {
    let started = Instant::now();
    let clock = Cell::new(started);
    let calls = Cell::new(0);
    let checks = Cell::new(0);
    let mut waits = Vec::new();
    let result = recover(
        started + Duration::from_secs(1),
        "dispatch_guard",
        || {
            calls.set(calls.get() + 1);
            if calls.get() <= 3 {
                Err(torn())
            } else {
                Ok(71)
            }
        },
        || {
            checks.set(checks.get() + 1);
            Ok(())
        },
        || clock.get(),
        |duration| {
            waits.push(duration);
            clock.set(clock.get() + duration);
        },
    )
    .unwrap();
    assert_eq!(result, 71);
    assert_eq!(calls.get(), 4);
    assert_eq!(waits, [Duration::from_millis(2); 3]);
    assert_eq!(checks.get(), 14); // Each attempt and both ends of each wait.
}

#[test]
fn exhaustion_keeps_the_last_underlying_cause_and_bounded_phase_diagnostic() {
    let started = Instant::now();
    let clock = Cell::new(started);
    let calls = Cell::new(0);
    let error = recover::<()>(
        started + Duration::from_secs(1),
        "dispatch_guard",
        || {
            calls.set(calls.get() + 1);
            Err(Error::new("torn_read", format!("sample={}", calls.get())))
        },
        || Ok(()),
        || clock.get(),
        |duration| clock.set(clock.get() + duration),
    )
    .unwrap_err();
    assert_eq!(calls.get(), 4);
    assert_eq!(error.code, "torn_read");
    assert_eq!(
        error.message,
        "phase=dispatch_guard; read_attempts=4; read_elapsed_ms=6.000; sample=4"
    );
}

#[test]
fn coherent_foreign_owner_scope_and_short_reads_are_never_retried() {
    for code in [
        "ownership_changed",
        "foreign_production_scope",
        "production_state_changed",
        "short_read",
        "read_bound",
        "permission_denied",
        "action_uncertain",
    ] {
        let started = Instant::now();
        let calls = Cell::new(0);
        let error = recover::<()>(
            started + Duration::from_secs(1),
            "dispatch_guard",
            || {
                calls.set(calls.get() + 1);
                Err(Error::new(code, "generated coherent refusal"))
            },
            || Ok(()),
            || started,
            |_| panic!("Only a discarded torn proof can wait"),
        )
        .unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(calls.get(), 1);
        assert!(error.message.contains("read_attempts=1"));
        assert!(error.message.ends_with("generated coherent refusal"));
    }
}

#[test]
fn an_expired_original_deadline_never_starts_a_proof() {
    let until = Instant::now();
    let error = recover::<()>(
        until,
        "dispatch_guard",
        || panic!("Expired proof must not read"),
        || panic!("Expired proof must not check a process"),
        || until,
        |_| panic!("Expired proof must not wait"),
    )
    .unwrap_err();
    assert_eq!(error.code, "production_deadline");
    assert!(error.message.contains("read_attempts=0"));
}

#[test]
fn successive_torn_samples_share_one_original_deadline() {
    let started = Instant::now();
    let until = started + Duration::from_millis(8);
    let clock = Cell::new(started);
    let calls = Cell::new(0);
    let mut waits = Vec::new();
    let error = recover::<()>(
        until,
        "dispatch_guard",
        || {
            calls.set(calls.get() + 1);
            clock.set(clock.get() + Duration::from_millis(1));
            Err(torn())
        },
        || Ok(()),
        || clock.get(),
        |duration| {
            waits.push(duration);
            clock.set(clock.get() + duration);
        },
    )
    .unwrap_err();
    assert_eq!(error.code, "production_deadline");
    assert_eq!(calls.get(), 3);
    assert_eq!(clock.get(), until);
    assert_eq!(
        waits,
        [
            Duration::from_millis(2),
            Duration::from_millis(2),
            Duration::from_millis(1),
        ]
    );
}

#[test]
fn late_success_is_discarded_even_after_a_discarded_torn_sample() {
    let started = Instant::now();
    let until = started + Duration::from_millis(10);
    let clock = Cell::new(started);
    let calls = Cell::new(0);
    let error = recover(
        until,
        "dispatch_guard",
        || {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                Err(torn())
            } else {
                clock.set(until);
                Ok(71)
            }
        },
        || Ok(()),
        || clock.get(),
        |duration| clock.set(clock.get() + duration),
    )
    .unwrap_err();
    assert_eq!(error.code, "production_deadline");
    assert_eq!(calls.get(), 2);
}

#[test]
fn cancellation_lease_or_identity_loss_at_read_or_wait_boundaries_stops_recovery() {
    for code in [
        "session_closed",
        "input_lease_required",
        "ownership_changed",
    ] {
        for during_read in [false, true] {
            let started = Instant::now();
            let failed = Cell::new(false);
            let calls = Cell::new(0);
            let waits = Cell::new(0);
            let error = recover::<()>(
                started + Duration::from_secs(1),
                "dispatch_guard",
                || {
                    calls.set(calls.get() + 1);
                    if during_read {
                        failed.set(true);
                    }
                    Err(torn())
                },
                || {
                    if failed.get() {
                        Err(Error::new(code, "generated original Fence refusal"))
                    } else {
                        Ok(())
                    }
                },
                || started,
                |_| {
                    waits.set(waits.get() + 1);
                    failed.set(true);
                },
            )
            .unwrap_err();
            assert_eq!(error.code, code);
            assert_eq!(calls.get(), 1);
            assert_eq!(waits.get(), usize::from(!during_read));
        }
    }
}

struct FixtureProcess {
    identity: Identity,
    present: AtomicBool,
    checks: AtomicUsize,
}
impl ReadMemory for FixtureProcess {
    fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
        panic!("This generated boundary fixture must not read a process")
    }
}
impl Process for FixtureProcess {
    fn check(&self) -> Result<()> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn image_base(&self) -> u64 {
        0
    }
    fn identity(&self) -> Option<&Identity> {
        self.present
            .load(Ordering::SeqCst)
            .then_some(&self.identity)
    }
}

#[test]
fn default_observer_retains_the_actual_action_fence_after_each_proof() {
    use crate::launch::macos::action_fence::Fence;
    for stopped in [false, true] {
        let original = proof_tests::snapshot();
        let epoch = &original.state.epoch;
        let process = FixtureProcess {
            identity: epoch.game.clone(),
            present: AtomicBool::new(true),
            checks: AtomicUsize::new(0),
        };
        let mut authority = InputAuthority::new("fixture".into(), epoch.clone()).unwrap();
        authority
            .acquire("fixture", TOKEN.into(), 30, epoch)
            .unwrap();
        let stop = AtomicBool::new(false);
        let until = Instant::now() + Duration::from_secs(1);
        let fence = Fence {
            process: &process,
            epoch,
            expected: &original.context,
            authority: &authority,
            token: TOKEN,
            until,
            stop: Some(&stop),
        };
        let calls = Cell::new(0);
        let error = observe(&fence, until, "dispatch_guard", || {
            calls.set(calls.get() + 1);
            if stopped {
                stop.store(true, Ordering::SeqCst);
                Ok(71)
            } else {
                process.present.store(false, Ordering::SeqCst);
                Err(torn())
            }
        })
        .unwrap_err();
        assert_eq!(
            error.code,
            if stopped {
                "session_closed"
            } else {
                "ownership_changed"
            }
        );
        assert_eq!(calls.get(), 1);
        assert_eq!(process.checks.load(Ordering::SeqCst), 1);
    }
}

const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
#[derive(Clone, Copy)]
enum Mode {
    Recover,
    AllocationChanged,
    ControlChanged,
    Exhaust,
}
/// Synthetic transport around the real recovery helper and original typed
/// comparisons. The real Journal proves its one-dispatch/no-replay boundary;
/// this is deliberately not evidence of native event delivery.
struct DispatchFixture {
    original: Canonical,
    current: FactoryState,
    mode: Mode,
    dispatches: usize,
    moves: usize,
    downs: usize,
    guard_samples: usize,
    path: PathBuf,
}
impl DispatchFixture {
    fn new(mode: Mode, path: PathBuf) -> Self {
        let original = proof_tests::snapshot();
        Self {
            current: original.state.clone(),
            original,
            mode,
            dispatches: 0,
            moves: 0,
            downs: 0,
            guard_samples: 0,
            path,
        }
    }
}
impl step::Backend for DispatchFixture {
    type Candidate = Button;
    fn observe(&mut self, _: &Target, _: Instant) -> Result<State> {
        Ok(State::Factories(self.current.clone()))
    }
    fn candidate(&mut self, _: &Target, before: &State, _: Instant) -> Result<Button> {
        assert_eq!(before.factory()?, &self.original.state);
        Ok(proof_tests::control())
    }
    fn dispatch_once(
        &mut self,
        _: &Target,
        before: &State,
        candidate: &Button,
        until: Instant,
    ) -> DispatchOutcome {
        assert_eq!(records(&self.path).last().unwrap()["status"], "Intent");
        assert_eq!(before.factory().unwrap(), &self.original.state);
        self.dispatches += 1;
        self.moves += 1;
        let clock = Cell::new(Instant::now());
        let calls = Cell::new(0);
        let proof = recover(
            until,
            "dispatch_guard",
            || {
                calls.set(calls.get() + 1);
                if calls.get() == 1 || matches!(self.mode, Mode::Exhaust) {
                    return stage("dispatch_button", Err(torn()));
                }
                // If a third read occurred it would restore the old state.
                // A coherent second sample must refuse without seeing it.
                let mut current = proof_tests::snapshot();
                let mut control = proof_tests::control();
                if calls.get() == 2 {
                    match self.mode {
                        Mode::AllocationChanged => current.allocations[1].1 += 1,
                        Mode::ControlChanged => control.widget += 8,
                        Mode::Recover | Mode::Exhaust => {}
                    }
                }
                stage("dispatch_allocation", same_before(&self.original, &current))?;
                stage(
                    "dispatch_fixed_control",
                    unchanged_button(candidate, &control),
                )
            },
            || Ok(()),
            || clock.get(),
            |duration| clock.set(clock.get() + duration),
        );
        self.guard_samples += calls.get();
        match proof {
            Ok(()) => {
                self.downs += 1;
                self.current.requested += 1;
                DispatchOutcome::Unknown(None)
            }
            Err(error) => DispatchOutcome::Unknown(Some(error)),
        }
    }
    fn confirm_input_idle(&mut self, _: Instant) -> Result<()> {
        Ok(())
    }
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "hoi4-mac-production-recovery-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn journal(&self) -> PathBuf {
        self.0.join("production-actions.jsonl")
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn records(path: &Path) -> Vec<serde_json::Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn authority(epoch: &crate::actions::Epoch) -> InputAuthority {
    let mut authority = InputAuthority::new("fixture".into(), epoch.clone()).unwrap();
    authority
        .acquire("fixture", TOKEN.into(), 30, epoch)
        .unwrap();
    authority
}

#[test]
fn recovered_guard_never_repeats_cursor_movement_or_journal_dispatch() {
    let temp = Scratch::new();
    let target = Target::Factories(proof_tests::target(&proof_tests::snapshot()));
    let mut journal = step::Journal::open(&temp.journal()).unwrap();
    let mut backend = DispatchFixture::new(Mode::Recover, temp.journal());
    let authority = authority(target.epoch());
    let gate = RwLock::new(true);
    let first = journal
        .execute(
            "step-1",
            target.clone(),
            TOKEN,
            &authority,
            &gate,
            &Cleared(&target.epoch().game),
            &mut backend,
            Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(first.status, step::Status::Verified);
    assert_eq!(first.delivery, Some(Delivery::Unknown));
    assert_eq!(
        (
            backend.dispatches,
            backend.moves,
            backend.downs,
            backend.guard_samples
        ),
        (1, 1, 1, 2)
    );
    let replay = journal
        .execute(
            "step-1",
            target.clone(),
            TOKEN,
            &authority,
            &gate,
            &Cleared(&target.epoch().game),
            &mut backend,
            Duration::from_secs(2),
        )
        .unwrap();
    assert!(replay.historical);
    assert_eq!(
        (
            backend.dispatches,
            backend.moves,
            backend.downs,
            backend.guard_samples
        ),
        (1, 1, 1, 2)
    );
    let saved = records(&temp.journal());
    assert_eq!(saved.len(), 2);
    assert_eq!(saved[0]["status"], "Intent");
    assert_eq!(saved[1]["status"], "Verified");
}

#[test]
fn coherent_changed_allocation_or_fixed_control_after_torn_read_refuses_immediately() {
    for mode in [Mode::AllocationChanged, Mode::ControlChanged] {
        uncertain_dispatch_is_retained(mode, "production_state_changed", 2);
    }
}

#[test]
fn exhausted_guard_retains_unknown_delivery_and_original_cause_without_replay() {
    uncertain_dispatch_is_retained(Mode::Exhaust, "torn_read", 4);
}

fn uncertain_dispatch_is_retained(mode: Mode, code: &str, samples: usize) {
    let temp = Scratch::new();
    let target = Target::Factories(proof_tests::target(&proof_tests::snapshot()));
    let mut journal = step::Journal::open(&temp.journal()).unwrap();
    let mut backend = DispatchFixture::new(mode, temp.journal());
    let authority = authority(target.epoch());
    let gate = RwLock::new(true);
    let error = journal
        .execute(
            "step-1",
            target.clone(),
            TOKEN,
            &authority,
            &gate,
            &Cleared(&target.epoch().game),
            &mut backend,
            Duration::from_secs(2),
        )
        .unwrap_err();
    assert_eq!(error.code, "action_uncertain");
    assert!(error.message.contains(code));
    assert_eq!(
        journal.failure_evidence("step-1", &target, &error),
        FailureEvidence::Unknown
    );
    assert_eq!(
        (
            backend.dispatches,
            backend.moves,
            backend.downs,
            backend.guard_samples
        ),
        (1, 1, 0, samples)
    );
    let saved = records(&temp.journal());
    assert_eq!(saved.len(), 2);
    assert_eq!(saved[0]["status"], "Intent");
    assert_eq!(saved[1]["status"], "Uncertain");
    assert_eq!(saved[1]["delivery"], "Unknown");
    assert_eq!(saved[1]["cause"]["code"], code);
    let diagnostic = saved[1]["cause"]["message"].as_str().unwrap();
    assert!(diagnostic.contains("phase=dispatch_guard"));
    assert!(diagnostic.contains(&format!("read_attempts={samples}")));
    assert!(diagnostic.contains("stage=dispatch_"));
    if code == "torn_read" {
        assert!(diagnostic.contains("section=mac_production_geometry"));
    }
    drop(journal);
    let mut reopened = step::Journal::open(&temp.journal()).unwrap();
    assert!(reopened.blocks_game(&target.epoch().game));
    // Same-ID recovery is observation only; a new ID also cannot rescue input.
    for id in ["step-1", "step-2"] {
        let refusal = reopened
            .execute(
                id,
                target.clone(),
                TOKEN,
                &authority,
                &gate,
                &Cleared(&target.epoch().game),
                &mut backend,
                Duration::from_secs(2),
            )
            .unwrap_err();
        assert_eq!(refusal.code, "action_uncertain");
        assert_eq!(
            (
                backend.dispatches,
                backend.moves,
                backend.downs,
                backend.guard_samples
            ),
            (1, 1, 0, samples)
        );
    }
    assert_eq!(records(&temp.journal()).len(), 2);
}
