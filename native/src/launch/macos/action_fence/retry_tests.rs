//! Generated clock memory and private probe seams only; no native process/input.
use super::*;
use crate::own_focus::assignment_fixture::{Fixture, BASE, DATE};
use std::{
    cell::Cell,
    sync::{atomic::AtomicUsize, Mutex},
};

struct ClockProcess {
    generated: Mutex<Fixture>,
    game: Identity,
    replacement: Identity,
    replaced: AtomicBool,
    abi: StringAbi,
    attempts: AtomicUsize,
    torn_attempts: usize,
    wrong_first: bool,
    playthrough: String,
}
impl ReadMemory for ClockProcess {
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        self.generated.lock().unwrap().read(address, size)
    }
    fn string_abi(&self) -> StringAbi {
        self.abi
    }
}
impl Process for ClockProcess {
    fn begin_snapshot(&self) -> Result<()> {
        let attempt = self.attempts.fetch_add(1, Ordering::SeqCst) + 1;
        let mut generated = self.generated.lock().unwrap();
        // clock_context reads this root first, inside snapshot_fields, and at
        // snapshot_fields' final boundary. Tear only that complete readback.
        let root = BASE + generated.game_layout().gs_global;
        generated.tear(
            root,
            8,
            if attempt <= self.torn_attempts {
                3
            } else {
                usize::MAX
            },
        );
        generated.set_date(DATE + attempt as i32);
        generated.set_playthrough(if self.wrong_first && attempt == 1 {
            "11223344-5566-4788-99aa-bbccddeeff00"
        } else {
            &self.playthrough
        });
        generated.begin_snapshot()
    }
    fn check(&self) -> Result<()> {
        self.generated.lock().unwrap().check()
    }
    fn image_base(&self) -> u64 {
        BASE
    }
    fn identity(&self) -> Option<&Identity> {
        Some(if self.replaced.load(Ordering::Acquire) {
            &self.replacement
        } else {
            &self.game
        })
    }
}

struct Harness {
    process: ClockProcess,
    epoch: Epoch,
    expected: ClockContext,
    authority: InputAuthority,
    token: String,
    stop: AtomicBool,
}
impl Harness {
    fn new(abi: StringAbi, torn_attempts: usize, wrong_first: bool) -> Self {
        let generated = Fixture::new(abi);
        let expected = generated.context();
        let game = generated.game();
        let mut replacement = game.clone();
        replacement.start_ticks.push('9');
        let epoch = Epoch {
            game: game.clone(),
            campaign: "generated:clock-retry".into(),
            player: expected.player.clone(),
        };
        let token = "a".repeat(32);
        let mut authority = InputAuthority::new("fixture".into(), epoch.clone()).unwrap();
        authority
            .acquire("fixture", token.clone(), 30, &epoch)
            .unwrap();
        Self {
            process: ClockProcess {
                generated: Mutex::new(generated),
                game,
                replacement,
                replaced: AtomicBool::new(false),
                abi,
                attempts: AtomicUsize::new(0),
                torn_attempts,
                wrong_first,
                playthrough: expected.playthrough_id.clone(),
            },
            epoch,
            expected,
            authority,
            token,
            stop: AtomicBool::new(false),
        }
    }
    fn fence(&self, until: Instant) -> Fence<'_> {
        Fence {
            process: &self.process,
            epoch: &self.epoch,
            expected: &self.expected,
            authority: &self.authority,
            token: &self.token,
            until,
            stop: Some(&self.stop),
        }
    }
}

fn torn() -> Error {
    Error::new(
        "torn_read",
        "Generated discarded complete clock observation",
    )
}

#[test]
fn default_clock_path_discards_torn_observations_then_accepts_coherent_time_progress() {
    for abi in [StringAbi::LibCxx, StringAbi::LibStdCxx] {
        let h = Harness::new(abi, 2, false);
        let until = Instant::now() + Duration::from_secs(5);
        let fence = h.fence(until);
        fence.input_check().unwrap();
        assert_eq!(h.process.attempts.load(Ordering::SeqCst), 3);
        assert_eq!(fence.until, until);
        assert_eq!(
            h.process.generated.lock().unwrap().context().date_raw,
            h.expected.date_raw + 3
        );
    }
}

#[test]
fn default_clock_path_exhausts_three_retries_without_adopting_any_torn_context() {
    for abi in [StringAbi::LibCxx, StringAbi::LibStdCxx] {
        let h = Harness::new(abi, usize::MAX, false);
        let fence = h.fence(Instant::now() + Duration::from_secs(5));
        let error = fence.input_check().unwrap_err();
        assert_eq!(error.code, "torn_read");
        assert_eq!(error.message, "Campaign anchors changed during observation");
        assert_eq!(h.process.attempts.load(Ordering::SeqCst), 4);
    }
}

#[test]
fn default_clock_path_refuses_coherent_wrong_campaign_before_a_possible_restore() {
    for abi in [StringAbi::LibCxx, StringAbi::LibStdCxx] {
        let h = Harness::new(abi, 0, true);
        let fence = h.fence(Instant::now() + Duration::from_secs(5));
        assert_eq!(fence.input_check().unwrap_err().code, "ownership_changed");
        // The fixture would restore the original campaign on attempt two.
        assert_eq!(h.process.attempts.load(Ordering::SeqCst), 1);
    }
}

#[test]
fn non_torn_clock_errors_never_wait_or_retry() {
    for code in [
        "short_read",
        "ownership_changed",
        "foreign_focus_scope",
        "invalid_pointer",
        "input_lease_required",
        "session_closed",
        "action_deadline",
    ] {
        let h = Harness::new(StringAbi::LibCxx, 0, false);
        let fence = h.fence(Instant::now() + Duration::from_secs(5));
        let attempts = Cell::new(0);
        let waits = Cell::new(0);
        let error = fence
            .check_context(
                || {
                    attempts.set(attempts.get() + 1);
                    Err(Error::new(code, "Generated complete clock refusal"))
                },
                || fence.live(),
                |_| waits.set(waits.get() + 1),
            )
            .unwrap_err();
        assert_eq!(error.code, code);
        assert_eq!(attempts.get(), 1);
        assert_eq!(waits.get(), 0);
    }
}

#[test]
fn retry_waits_are_bounded_and_cancellation_is_checked_after_probe_and_wait() {
    for stop_during_wait in [false, true] {
        let h = Harness::new(StringAbi::LibCxx, 0, false);
        let fence = h.fence(Instant::now() + Duration::from_secs(5));
        let attempts = Cell::new(0);
        let waits = Cell::new(0);
        let error = fence
            .check_context(
                || {
                    attempts.set(attempts.get() + 1);
                    if !stop_during_wait {
                        h.stop.store(true, Ordering::Release);
                    }
                    Err(torn())
                },
                || fence.live(),
                |duration| {
                    assert!(duration <= Duration::from_millis(2));
                    waits.set(waits.get() + 1);
                    h.stop.store(true, Ordering::Release);
                },
            )
            .unwrap_err();
        assert_eq!(error.code, "session_closed");
        assert_eq!(attempts.get(), 1);
        assert_eq!(waits.get(), usize::from(stop_during_wait));
    }
}

#[test]
fn lease_refusal_after_retry_wait_prevents_another_clock_probe() {
    let h = Harness::new(StringAbi::LibCxx, 0, false);
    let fence = h.fence(Instant::now() + Duration::from_secs(5));
    let expired = Cell::new(false);
    let attempts = Cell::new(0);
    let waits = Cell::new(0);
    let error = fence
        .check_context(
            || {
                attempts.set(attempts.get() + 1);
                Err(torn())
            },
            || {
                fence.live()?;
                // Synthetic lease expiry avoids sleeping for the real 30s
                // minimum. The default path always uses Fence::live itself.
                if expired.get() {
                    Err(Error::new(
                        "input_lease_required",
                        "Generated lease expiry during retry wait",
                    ))
                } else {
                    Ok(())
                }
            },
            |_| {
                waits.set(waits.get() + 1);
                expired.set(true);
            },
        )
        .unwrap_err();
    assert_eq!(error.code, "input_lease_required");
    assert_eq!(attempts.get(), 1);
    assert_eq!(waits.get(), 1);
}

#[test]
fn process_identity_loss_during_retry_wait_prevents_another_clock_probe() {
    let h = Harness::new(StringAbi::LibCxx, 0, false);
    let fence = h.fence(Instant::now() + Duration::from_secs(5));
    let attempts = Cell::new(0);
    let error = fence
        .check_context(
            || {
                attempts.set(attempts.get() + 1);
                Err(torn())
            },
            || fence.live(),
            |_| h.process.replaced.store(true, Ordering::Release),
        )
        .unwrap_err();
    assert_eq!(error.code, "ownership_changed");
    assert_eq!(attempts.get(), 1);
}

#[test]
fn original_deadline_expiring_during_retry_wait_prevents_another_probe() {
    let h = Harness::new(StringAbi::LibCxx, 0, false);
    let until = Instant::now() + Duration::from_millis(100);
    let fence = h.fence(until);
    let attempts = Cell::new(0);
    let waits = Cell::new(0);
    let error = fence
        .check_context(
            || {
                attempts.set(attempts.get() + 1);
                Err(torn())
            },
            || fence.live(),
            |duration| {
                assert!(duration <= Duration::from_millis(2));
                waits.set(waits.get() + 1);
                thread::sleep(
                    (until + Duration::from_millis(1)).saturating_duration_since(Instant::now()),
                );
            },
        )
        .unwrap_err();
    assert_eq!(error.code, "action_deadline");
    assert_eq!(attempts.get(), 1);
    assert_eq!(waits.get(), 1);
    assert_eq!(fence.until, until);
}

#[test]
fn late_coherent_or_torn_probe_is_discarded_before_success_or_retry() {
    for coherent in [false, true] {
        let h = Harness::new(StringAbi::LibCxx, 0, false);
        let until = Instant::now() + Duration::from_millis(100);
        let fence = h.fence(until);
        let attempts = Cell::new(0);
        let waits = Cell::new(0);
        let error = fence
            .check_context(
                || {
                    attempts.set(attempts.get() + 1);
                    thread::sleep(
                        (until + Duration::from_millis(1))
                            .saturating_duration_since(Instant::now()),
                    );
                    if coherent {
                        Ok(h.expected.clone())
                    } else {
                        Err(torn())
                    }
                },
                || fence.live(),
                |_| waits.set(waits.get() + 1),
            )
            .unwrap_err();
        assert_eq!(error.code, "action_deadline");
        assert_eq!(attempts.get(), 1);
        assert_eq!(waits.get(), 0);
    }
}
