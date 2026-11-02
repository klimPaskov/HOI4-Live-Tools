use super::*;
use std::cell::{Cell, RefCell};

fn torn() -> Error {
    Error::new(
        "torn_read",
        "Snapshot witness changed; section=mac_construction_topbar_opener; read=431; size=16; changed_byte=12",
    )
}

#[test]
fn first_success_has_no_wait_and_checks_both_boundaries() {
    let start = Instant::now();
    let reads = Cell::new(0);
    let checks = Cell::new(0);
    let value = recover(
        start + Duration::from_secs(1),
        "initial_binding",
        || {
            reads.set(reads.get() + 1);
            Ok(17)
        },
        || {
            checks.set(checks.get() + 1);
            Ok(())
        },
        || start,
        |_| panic!("A coherent first proof must not wait"),
    )
    .unwrap();
    assert_eq!((value, reads.get(), checks.get()), (17, 1, 2));
}

#[test]
fn third_complete_observation_can_succeed_without_changing_the_bound() {
    let start = Instant::now();
    let until = start + Duration::from_millis(20);
    let clock = Cell::new(start);
    let reads = Cell::new(0);
    let checks = Cell::new(0);
    let waits = RefCell::new(Vec::new());
    let value = recover(
        until,
        "before_key",
        || {
            reads.set(reads.get() + 1);
            if reads.get() < 3 {
                Err(torn())
            } else {
                Ok("original binding")
            }
        },
        || {
            checks.set(checks.get() + 1);
            Ok(())
        },
        || clock.get(),
        |duration| {
            waits.borrow_mut().push(duration);
            clock.set(clock.get() + duration);
        },
    )
    .unwrap();
    assert_eq!(value, "original binding");
    assert_eq!((reads.get(), checks.get()), (3, 10));
    assert_eq!(*waits.borrow(), [INTERVAL, INTERVAL]);
    assert_eq!(clock.get(), start + Duration::from_millis(4));
    assert_eq!(until, start + Duration::from_millis(20));
}

#[test]
fn four_torn_observations_preserve_the_last_cause_and_diagnostics() {
    let start = Instant::now();
    let clock = Cell::new(start);
    let reads = Cell::new(0);
    let waits = Cell::new(0);
    let error = recover::<()>(
        start + Duration::from_secs(1),
        "before_key",
        || {
            reads.set(reads.get() + 1);
            Err(torn())
        },
        || Ok(()),
        || clock.get(),
        |duration| {
            waits.set(waits.get() + 1);
            clock.set(clock.get() + duration);
        },
    )
    .unwrap_err();
    assert_eq!((reads.get(), waits.get()), (4, 3));
    assert_eq!(error.code, "torn_read");
    assert_eq!(
        error.message,
        "phase=before_key; read_attempts=4; read_elapsed_ms=6.000; Snapshot witness changed; section=mac_construction_topbar_opener; read=431; size=16; changed_byte=12"
    );
}

#[test]
fn coherent_refusal_never_retries_toward_a_later_restored_binding() {
    for code in [
        "construction_opener_changed",
        "ownership_changed",
        "invalid_construction",
        "short_read",
    ] {
        let start = Instant::now();
        let reads = Cell::new(0);
        let error = recover(
            start + Duration::from_secs(1),
            "original_binding",
            || {
                reads.set(reads.get() + 1);
                if reads.get() == 1 {
                    Err(Error::new(code, "Coherent changed binding"))
                } else {
                    Ok("restored binding must remain unread")
                }
            },
            || Ok(()),
            || start,
            |_| panic!("Coherent refusals cannot wait"),
        )
        .unwrap_err();
        assert_eq!(reads.get(), 1);
        assert_eq!(error.code, code);
        assert!(error.message.contains("Coherent changed binding"));
        assert!(error.message.contains("read_attempts=1"));
    }
}

#[test]
fn coherent_refusal_survives_a_failed_post_read_boundary() {
    let start = Instant::now();
    let checks = Cell::new(0);
    let error = recover::<()>(
        start + Duration::from_secs(1),
        "original_binding",
        || {
            Err(Error::new(
                "construction_opener_changed",
                "Original target was coherently replaced",
            ))
        },
        || {
            checks.set(checks.get() + 1);
            if checks.get() == 2 {
                Err(Error::new("session_closed", "Stop after the proof"))
            } else {
                Ok(())
            }
        },
        || start,
        |_| panic!("No wait after a coherent refusal"),
    )
    .unwrap_err();
    assert_eq!(checks.get(), 2);
    assert_eq!(error.code, "construction_opener_changed");
    assert!(error
        .message
        .contains("Original target was coherently replaced"));
}

#[test]
fn coherent_late_refusal_keeps_its_cause_without_renewing_the_deadline() {
    let start = Instant::now();
    let until = start + INTERVAL;
    let clock = Cell::new(start);
    let reads = Cell::new(0);
    let error = recover::<()>(
        until,
        "original_binding",
        || {
            reads.set(reads.get() + 1);
            clock.set(until);
            Err(Error::new("ownership_changed", "Coherent foreign owner"))
        },
        || Ok(()),
        || clock.get(),
        |_| panic!("No wait after a coherent foreign owner"),
    )
    .unwrap_err();
    assert_eq!(reads.get(), 1);
    assert_eq!(error.code, "ownership_changed");
    assert!(error.message.contains("read_elapsed_ms=2.000"));
}

#[test]
fn expired_start_never_calls_boundary_or_producer() {
    let start = Instant::now();
    let error = recover::<()>(
        start,
        "initial_binding",
        || panic!("Expired proof must not run"),
        || panic!("Expired boundary must not read"),
        || start,
        |_| panic!("Expired proof must not wait"),
    )
    .unwrap_err();
    assert_eq!(error.code, "action_deadline");
    assert!(error.message.contains("read_attempts=0"));
}

#[test]
fn boundary_work_cannot_spend_the_budget_then_admit_a_producer() {
    let start = Instant::now();
    let until = start + INTERVAL;
    let clock = Cell::new(start);
    let error = recover::<()>(
        until,
        "initial_binding",
        || panic!("Boundary exhausted the original deadline"),
        || {
            clock.set(until);
            Ok(())
        },
        || clock.get(),
        |_| panic!("No waiting past the deadline"),
    )
    .unwrap_err();
    assert_eq!(error.code, "action_deadline");
    assert!(error.message.contains("read_attempts=0"));
}

#[test]
fn late_success_or_torn_read_is_discarded_without_wait_or_retry() {
    for is_torn in [false, true] {
        let start = Instant::now();
        let until = start + INTERVAL;
        let clock = Cell::new(start);
        let reads = Cell::new(0);
        let error = recover(
            until,
            "final_readback",
            || {
                reads.set(reads.get() + 1);
                clock.set(until);
                if is_torn {
                    Err(torn())
                } else {
                    Ok("Late successful panel")
                }
            },
            || Ok(()),
            || clock.get(),
            |_| panic!("A late read must not wait"),
        )
        .unwrap_err();
        assert_eq!(reads.get(), 1);
        assert_eq!(error.code, "action_deadline");
        assert!(error
            .message
            .contains("phase=final_readback; read_attempts=1"));
    }
}

#[test]
fn boundary_refusals_are_not_retried_even_when_the_boundary_code_is_torn() {
    for code in [
        "torn_read",
        "session_closed",
        "ownership_changed",
        "input_lease_expired",
    ] {
        let start = Instant::now();
        let checks = Cell::new(0);
        let error = recover::<()>(
            start + Duration::from_secs(1),
            "before_key",
            || panic!("No producer after refused boundary"),
            || {
                checks.set(checks.get() + 1);
                Err(Error::new(code, "Boundary refusal"))
            },
            || start,
            |_| panic!("A boundary refusal must not wait"),
        )
        .unwrap_err();
        assert_eq!(checks.get(), 1);
        assert_eq!(error.code, code);
        assert!(error.message.contains("read_attempts=0"));
    }
}

#[test]
fn boundary_failure_after_a_torn_or_successful_producer_stops_recovery() {
    for successful in [false, true] {
        for code in ["torn_read", "session_closed", "ownership_changed"] {
            let start = Instant::now();
            let reads = Cell::new(0);
            let checks = Cell::new(0);
            let error = recover(
                start + Duration::from_secs(1),
                "before_key",
                || {
                    reads.set(reads.get() + 1);
                    if successful {
                        Ok("coherent")
                    } else {
                        Err(torn())
                    }
                },
                || {
                    checks.set(checks.get() + 1);
                    if checks.get() == 2 {
                        Err(Error::new(code, "Post-read boundary refused"))
                    } else {
                        Ok(())
                    }
                },
                || start,
                |_| panic!("Post-read refusal cannot retry"),
            )
            .unwrap_err();
            assert_eq!((reads.get(), checks.get()), (1, 2));
            assert_eq!(error.code, code);
            assert!(error.message.contains("Post-read boundary refused"));
        }
    }
}

#[test]
fn stop_identity_or_lease_loss_during_wait_prevents_the_next_producer() {
    for code in ["session_closed", "ownership_changed", "input_lease_expired"] {
        let start = Instant::now();
        let clock = Cell::new(start);
        let lost = Cell::new(false);
        let reads = Cell::new(0);
        let checks = Cell::new(0);
        let error = recover::<()>(
            start + Duration::from_secs(1),
            "before_key",
            || {
                reads.set(reads.get() + 1);
                Err(torn())
            },
            || {
                checks.set(checks.get() + 1);
                if lost.get() {
                    Err(Error::new(code, "Ownership lost during wait"))
                } else {
                    Ok(())
                }
            },
            || clock.get(),
            |duration| {
                clock.set(clock.get() + duration);
                lost.set(true);
            },
        )
        .unwrap_err();
        assert_eq!((reads.get(), checks.get()), (1, 4));
        assert_eq!(error.code, code);
    }
}

#[test]
fn wait_is_clipped_to_the_original_remaining_budget() {
    let start = Instant::now();
    let until = start + INTERVAL;
    let clock = Cell::new(start);
    let waits = RefCell::new(Vec::new());
    let reads = Cell::new(0);
    let error = recover::<()>(
        until,
        "before_key",
        || {
            reads.set(reads.get() + 1);
            clock.set(start + Duration::from_micros(1500));
            Err(torn())
        },
        || Ok(()),
        || clock.get(),
        |duration| {
            waits.borrow_mut().push(duration);
            clock.set(clock.get() + duration);
        },
    )
    .unwrap_err();
    assert_eq!(reads.get(), 1);
    assert_eq!(*waits.borrow(), [Duration::from_micros(500)]);
    assert_eq!(error.code, "action_deadline");
    assert_eq!(clock.get(), until);
}

#[test]
fn successive_torn_reads_do_not_receive_a_new_deadline() {
    let start = Instant::now();
    let until = start + Duration::from_millis(5);
    let clock = Cell::new(start);
    let reads = Cell::new(0);
    let waits = Cell::new(0);
    let error = recover::<()>(
        until,
        "before_key",
        || {
            reads.set(reads.get() + 1);
            clock.set(clock.get() + INTERVAL);
            Err(torn())
        },
        || Ok(()),
        || clock.get(),
        |duration| {
            waits.set(waits.get() + 1);
            clock.set(clock.get() + duration);
        },
    )
    .unwrap_err();
    assert_eq!((reads.get(), waits.get()), (2, 1));
    assert_eq!(error.code, "action_deadline");
    assert!(error.message.contains("read_elapsed_ms=6.000"));
    assert_eq!(until, start + Duration::from_millis(5));
}
