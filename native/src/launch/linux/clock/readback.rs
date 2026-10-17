//! One fixed clock-operation budget and unchanged1.5s key-step readback.
//! Every observation is fresh; expiration never authorizes another key.
use crate::error::{Error, Result};
use std::time::{Duration, Instant};

pub(super) const OPERATION_TIMEOUT: Duration = Duration::from_secs(10);
const STEP_TIMEOUT: Duration = Duration::from_millis(1500);
const SAMPLE_INTERVAL: Duration = Duration::from_millis(20);

pub(super) fn operation_deadline(start: Instant, lease: Instant) -> Instant {
    lease.min(start + OPERATION_TIMEOUT)
}
pub(super) fn step_deadline(operation: Instant, now: Instant) -> Instant {
    operation.min(now + STEP_TIMEOUT)
}
fn authority_error() -> Error {
    Error::new(
        "input_lease_required",
        "Clock operation authority/deadline expired",
    )
}
fn readback_error() -> Error {
    Error::new(
        "input_readback_failed",
        "Delivered key had no timely target witness; never resend blindly",
    )
}
pub(super) fn ensure_live(deadline: Instant) -> Result<()> {
    if Instant::now() >= deadline {
        Err(authority_error())
    } else {
        Ok(())
    }
}

pub(super) fn observe_until<T>(
    deadline: Instant,
    observe: impl FnOnce() -> Result<T>,
) -> Result<T> {
    observe_until_with(deadline, observe, Instant::now)
}
#[cfg(test)]
pub(super) fn fixture_observe_until<T>(
    deadline: Instant,
    observe: impl FnOnce() -> Result<T>,
    now: impl FnMut() -> Instant,
) -> Result<T> {
    observe_until_with(deadline, observe, now)
}
fn observe_until_with<T>(
    deadline: Instant,
    observe: impl FnOnce() -> Result<T>,
    mut now: impl FnMut() -> Instant,
) -> Result<T> {
    if now() >= deadline {
        return Err(authority_error());
    }
    let value = observe()?;
    if now() >= deadline {
        return Err(authority_error());
    }
    Ok(value)
}

pub(super) fn wait_change<T>(
    deadline: Instant,
    observe: impl FnMut() -> Result<T>,
    reached: impl Fn(&T) -> bool,
) -> Result<T> {
    wait_change_with(deadline, observe, reached, Instant::now, std::thread::sleep)
}
#[cfg(test)]
pub(super) fn fixture_wait_change<T>(
    deadline: Instant,
    observe: impl FnMut() -> Result<T>,
    reached: impl Fn(&T) -> bool,
    now: impl FnMut() -> Instant,
    sleep: impl FnMut(Duration),
) -> Result<T> {
    wait_change_with(deadline, observe, reached, now, sleep)
}
fn wait_change_with<T>(
    deadline: Instant,
    mut observe: impl FnMut() -> Result<T>,
    reached: impl Fn(&T) -> bool,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
) -> Result<T> {
    loop {
        if now() >= deadline {
            return Err(readback_error());
        }
        let value = observe()?;
        // A target returned by a blocking read after the budget is not accepted.
        if now() >= deadline {
            return Err(readback_error());
        }
        if reached(&value) {
            return Ok(value);
        }
        let remaining = deadline.saturating_duration_since(now());
        if remaining.is_zero() {
            return Err(readback_error());
        }
        sleep(SAMPLE_INTERVAL.min(remaining));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        actions::{Action, Backend, Epoch, Journal, State, Status},
        platform::Identity,
    };
    use std::cell::Cell;

    #[test]
    fn operation_and_step_budget_are_capped_by_original_lease() {
        let start = Instant::now();
        assert_eq!(
            operation_deadline(start, start + Duration::from_secs(300)),
            start + Duration::from_secs(10)
        );
        let lease = start + Duration::from_millis(900);
        let until = operation_deadline(start, lease);
        assert_eq!(until, lease);
        assert_eq!(step_deadline(until, start), lease);
        assert_eq!(
            step_deadline(start + Duration::from_secs(10), start),
            start + Duration::from_millis(1500)
        );
        // Later steps do not reset or extend the original overall budget.
        assert_eq!(
            step_deadline(
                start + Duration::from_secs(10),
                start + Duration::from_millis(9600)
            ),
            start + Duration::from_secs(10)
        );
    }
    #[test]
    fn expired_readback_starts_no_observation() {
        let now = Instant::now();
        let reads = Cell::new(0);
        let e = wait_change_with(
            now,
            || {
                reads.set(reads.get() + 1);
                Ok(true)
            },
            |v| *v,
            || now,
            |_| panic!("No sleep after expiration"),
        )
        .unwrap_err();
        assert_eq!(e.code, "input_readback_failed");
        assert_eq!(reads.get(), 0);
    }
    #[test]
    fn late_target_from_one_blocking_read_is_refused() {
        let start = Instant::now();
        let now = Cell::new(start);
        let reads = Cell::new(0);
        let e = wait_change_with(
            start + STEP_TIMEOUT,
            || {
                reads.set(reads.get() + 1);
                now.set(start + STEP_TIMEOUT);
                Ok(true)
            },
            |v| *v,
            || now.get(),
            |_| panic!("No retry/sleep after late target"),
        )
        .unwrap_err();
        assert_eq!(e.code, "input_readback_failed");
        assert_eq!(reads.get(), 1);
    }
    #[test]
    fn remaining_budget_caps_sleep_and_prevents_an_extra_read() {
        let start = Instant::now();
        let now = Cell::new(start);
        let slept = Cell::new(Duration::ZERO);
        let reads = Cell::new(0);
        let e = wait_change_with(
            start + Duration::from_millis(25),
            || {
                reads.set(reads.get() + 1);
                now.set(start + Duration::from_millis(18));
                Ok(false)
            },
            |v| *v,
            || now.get(),
            |d| {
                slept.set(d);
                now.set(now.get() + d)
            },
        )
        .unwrap_err();
        assert_eq!(e.code, "input_readback_failed");
        assert_eq!(slept.get(), Duration::from_millis(7));
        assert_eq!(reads.get(), 1);
    }
    #[test]
    fn one_delivered_step_allows_three_fresh_samples_within_same_budget() {
        let start = Instant::now();
        let now = Cell::new(start);
        let reads = Cell::new(0);
        let next = wait_change_with(
            start + STEP_TIMEOUT,
            || {
                reads.set(reads.get() + 1);
                now.set(now.get() + Duration::from_millis(200));
                Ok(if reads.get() == 3 { 1u8 } else { 0 })
            },
            |v| *v == 1,
            || now.get(),
            |d| now.set(now.get() + d),
        )
        .unwrap();
        assert_eq!(next, 1);
        assert_eq!(reads.get(), 3);
    }
    #[test]
    fn wrong_final_speed_is_not_the_required_intermediate_step_witness() {
        let start = Instant::now();
        let now = Cell::new(start);
        let reads = Cell::new(0);
        let e = wait_change_with(
            start + STEP_TIMEOUT,
            || {
                reads.set(reads.get() + 1);
                now.set(now.get() + Duration::from_millis(800));
                Ok(4u8)
            },
            |v| *v == 1,
            || now.get(),
            |d| now.set(now.get() + d),
        )
        .unwrap_err();
        assert_eq!(e.code, "input_readback_failed");
        assert_eq!(reads.get(), 2);
    }
    #[test]
    fn observation_authority_is_checked_before_and_after_read() {
        let start = Instant::now();
        let until = start + Duration::from_secs(10);
        let reads = Cell::new(0);
        assert_eq!(
            observe_until_with(
                until,
                || {
                    reads.set(reads.get() + 1);
                    Ok(true)
                },
                || until
            )
            .unwrap_err()
            .code,
            "input_lease_required"
        );
        assert_eq!(reads.get(), 0);
        let now = Cell::new(start);
        assert_eq!(
            observe_until_with(
                until,
                || {
                    reads.set(reads.get() + 1);
                    now.set(until);
                    Ok(true)
                },
                || now.get()
            )
            .unwrap_err()
            .code,
            "input_lease_required"
        );
        assert_eq!(reads.get(), 1);
    }
    #[test]
    fn identity_or_permission_failure_is_never_retried_or_reclassified() {
        let start = Instant::now();
        let reads = Cell::new(0);
        let e = wait_change_with(
            start + STEP_TIMEOUT,
            || {
                reads.set(reads.get() + 1);
                Err::<bool, _>(Error::new("ownership_changed", "Generated exec change"))
            },
            |v| *v,
            || start,
            |_| panic!("No retry after ownership error"),
        )
        .unwrap_err();
        assert_eq!(e.code, "ownership_changed");
        assert_eq!(reads.get(), 1);
    }

    struct TempJournal(std::path::PathBuf);
    impl TempJournal {
        fn new() -> Self {
            static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let seq = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let time = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "hoi4-clock-budget-{}-{time}-{seq}",
                std::process::id()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn file(&self) -> std::path::PathBuf {
            self.0.join("clock-actions.jsonl")
        }
    }
    impl Drop for TempJournal {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
    #[derive(Clone, Copy)]
    enum Delay {
        Initial,
        Preflight,
        DeliveredLate,
        ThreeSamples,
    }
    struct GeneratedBackend {
        start: Instant,
        deadline: Instant,
        now: Cell<Instant>,
        state: State,
        delay: Delay,
        observations: usize,
        dispatches: usize,
        samples: usize,
    }
    impl GeneratedBackend {
        fn new(delay: Delay) -> Self {
            let start = Instant::now();
            Self {
                start,
                deadline: start + OPERATION_TIMEOUT,
                now: Cell::new(start),
                state: State {
                    epoch: Epoch {
                        game: Identity {
                            pid: 123,
                            start_ticks: "5".into(),
                            exe: "/generated-clock".into(),
                            uid: 1000,
                        },
                        campaign: "generated-deadline-campaign".into(),
                        player: "GER".into(),
                    },
                    paused: false,
                    speed: 0,
                },
                delay,
                observations: 0,
                dispatches: 0,
                samples: 0,
            }
        }
    }
    impl Backend for GeneratedBackend {
        fn observe(&mut self) -> Result<State> {
            self.observations += 1;
            observe_until_with(
                self.deadline,
                || {
                    if matches!(self.delay, Delay::Initial) {
                        self.now.set(self.deadline);
                    }
                    Ok(self.state.clone())
                },
                || self.now.get(),
            )
        }
        fn preflight(&mut self, _action: &Action, _before: &State) -> Result<()> {
            if matches!(self.delay, Delay::Preflight) {
                observe_until_with(
                    self.deadline,
                    || {
                        self.now.set(self.deadline);
                        Ok(())
                    },
                    || self.now.get(),
                )?;
            }
            Ok(())
        }
        fn dispatch(&mut self, _action: &Action, _before: &State) -> Result<()> {
            self.dispatches += 1;
            let next = wait_change_with(
                step_deadline(self.deadline, self.now.get()),
                || {
                    self.samples += 1;
                    if matches!(self.delay, Delay::DeliveredLate) {
                        self.now.set(self.start + STEP_TIMEOUT);
                        Ok(1u8)
                    } else {
                        self.now.set(self.now.get() + Duration::from_millis(400));
                        Ok(if self.samples == 3 { 1 } else { 0 })
                    }
                },
                |speed| *speed == 1,
                || self.now.get(),
                |d| self.now.set(self.now.get() + d),
            )?;
            self.state.speed = next;
            Ok(())
        }
    }
    #[test]
    fn late_noop_observation_does_not_create_receipt_or_dispatch() {
        let dir = TempJournal::new();
        let mut journal = Journal::open(&dir.file()).unwrap();
        let mut backend = GeneratedBackend::new(Delay::Initial);
        let epoch = backend.state.epoch.clone();
        let error = journal
            .execute(
                "late-noop",
                &epoch,
                Action::SetPaused(false),
                &mut backend,
                OPERATION_TIMEOUT,
            )
            .unwrap_err();
        assert_eq!(error.code, "input_lease_required");
        assert_eq!(backend.dispatches, 0);
        assert_eq!(backend.observations, 1);
        assert!(journal.receipt("late-noop").is_none());
    }
    #[test]
    fn late_preflight_does_not_mark_dispatching_or_send() {
        let dir = TempJournal::new();
        let mut journal = Journal::open(&dir.file()).unwrap();
        let mut backend = GeneratedBackend::new(Delay::Preflight);
        let epoch = backend.state.epoch.clone();
        let error = journal
            .execute(
                "late-preflight",
                &epoch,
                Action::SetSpeed(1),
                &mut backend,
                OPERATION_TIMEOUT,
            )
            .unwrap_err();
        assert_eq!(error.code, "input_lease_required");
        assert_eq!(backend.dispatches, 0);
        assert!(journal.receipt("late-preflight").is_none());
    }
    #[test]
    fn late_postdelivery_target_is_quarantined_across_journal_restart() {
        let dir = TempJournal::new();
        let mut journal = Journal::open(&dir.file()).unwrap();
        let mut backend = GeneratedBackend::new(Delay::DeliveredLate);
        let epoch = backend.state.epoch.clone();
        let error = journal
            .execute(
                "one-delivery",
                &epoch,
                Action::SetSpeed(1),
                &mut backend,
                OPERATION_TIMEOUT,
            )
            .unwrap_err();
        assert_eq!(error.code, "action_uncertain");
        assert_eq!(backend.dispatches, 1);
        assert_eq!(backend.samples, 1);
        assert_eq!(
            journal.receipt("one-delivery").unwrap().status,
            Status::Dispatching
        );
        drop(journal);
        let mut reopened = Journal::open(&dir.file()).unwrap();
        let reads = backend.observations;
        for id in ["one-delivery", "another-id"] {
            assert_eq!(
                reopened
                    .execute(
                        id,
                        &epoch,
                        Action::SetSpeed(1),
                        &mut backend,
                        OPERATION_TIMEOUT
                    )
                    .unwrap_err()
                    .code,
                "action_uncertain"
            );
        }
        assert_eq!(backend.dispatches, 1);
        assert_eq!(backend.observations, reads);
    }
    #[test]
    fn three_samples_verify_one_delivery_without_resetting_overall_budget() {
        let dir = TempJournal::new();
        let mut journal = Journal::open(&dir.file()).unwrap();
        let mut backend = GeneratedBackend::new(Delay::ThreeSamples);
        let epoch = backend.state.epoch.clone();
        let fixed = backend.deadline;
        let receipt = journal
            .execute(
                "one-delivery",
                &epoch,
                Action::SetSpeed(1),
                &mut backend,
                OPERATION_TIMEOUT,
            )
            .unwrap();
        assert_eq!(receipt.status, Status::Verified);
        assert_eq!(receipt.observed.speed, 1);
        assert_eq!(backend.dispatches, 1);
        assert_eq!(backend.samples, 3);
        assert_eq!(backend.deadline, fixed);
    }
}
