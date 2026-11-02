use super::*;
use std::cell::{Cell, RefCell};

fn tear(message: &str) -> Error {
    Error::new("torn_read", message)
}

#[test]
fn all_three_raw_producer_phases_recover_before_one_downstream_acceptance() {
    for phase in [
        Phase::DestinationInitial,
        Phase::DestinationAfterDeparture,
        Phase::SelectedIdentity,
    ] {
        let start = Instant::now();
        let at = Cell::new(start);
        let calls = Cell::new(0);
        let downstream = Cell::new(0);
        let sleeps = RefCell::new(Vec::new());
        let mut history = History::default();
        let value = history
            .observe_with(
                phase,
                start + Duration::from_secs(5),
                || {
                    calls.set(calls.get() + 1);
                    if calls.get() < 3 {
                        Err(tear(&format!("original {}", calls.get())))
                    } else {
                        Ok("one complete witnessed snapshot")
                    }
                },
                || at.get(),
                |d| {
                    sleeps.borrow_mut().push(d);
                    at.set(at.get() + d);
                },
            )
            .unwrap();
        downstream.set(downstream.get() + 1); // Guard/pin/commit happens only here.
        assert_eq!(value, "one complete witnessed snapshot");
        assert_eq!(calls.get(), 3);
        assert_eq!(downstream.get(), 1);
        assert_eq!(*sleeps.borrow(), vec![super::super::INTERVAL; 2]);
        let summary = history.summary().unwrap();
        assert_eq!(summary["rejected_reads"], 2);
        assert_eq!(summary["recovered_observations"], 1);
        assert_eq!(summary["failed_observations"], 0);
        assert_eq!(summary["latest_rejection_recovered"], true);
        assert_eq!(summary["first_failure"]["phase"], phase.name());
        assert_eq!(summary["first_failure"]["producer"], phase.producer());
        assert_eq!(summary["first_failure"]["reason"], "original 1");
        assert_eq!(summary["latest_failure"]["reason"], "original 2");
        assert_eq!(summary["historical"], true);
        assert_eq!(summary["input_authorized"], false);
        assert_eq!(summary["current_state_verified"], false);
    }
}

#[test]
fn first_success_is_one_read_no_sleep_no_history_and_does_not_erase_previous_recovery() {
    let mut history = History::default();
    let start = Instant::now();
    let mut calls = 0;
    assert_eq!(
        history
            .observe_with(
                Phase::DestinationInitial,
                start + Duration::from_secs(1),
                || {
                    calls += 1;
                    Ok(17)
                },
                || start,
                |_| panic!("Clean read must never sleep")
            )
            .unwrap(),
        17
    );
    assert_eq!(calls, 1);
    assert!(history.summary().is_none());
    let mut attempts = 0;
    history
        .observe_with(
            Phase::SelectedIdentity,
            start + Duration::from_secs(1),
            || {
                attempts += 1;
                if attempts == 1 {
                    Err(tear("one rejected identity"))
                } else {
                    Ok(())
                }
            },
            || start,
            |_| {},
        )
        .unwrap();
    let before = history.summary();
    history
        .observe_with(
            Phase::DestinationAfterDeparture,
            start + Duration::from_secs(1),
            || Ok(()),
            || start,
            |_| panic!("Clean read must not sleep"),
        )
        .unwrap();
    assert_eq!(history.summary(), before);
}

#[test]
fn stable_changed_identity_is_compared_once_outside_recovery_and_never_rebound() {
    let start = Instant::now();
    let mut h = History::default();
    let mut reads = 0;
    let current = h
        .observe_with(
            Phase::SelectedIdentity,
            start + Duration::from_secs(1),
            || {
                reads += 1;
                if reads == 1 {
                    Err(tear("unaccepted partial snapshot"))
                } else {
                    Ok("changed root")
                }
            },
            || start,
            |_| {},
        )
        .unwrap();
    let mut comparisons = 0;
    let compared = (|| -> Result<()> {
        comparisons += 1;
        if current != "original root" {
            return Err(Error::new("ownership_changed", "Original root changed"));
        }
        Ok(())
    })();
    assert_eq!(compared.unwrap_err().code, "ownership_changed");
    assert_eq!(reads, 2);
    assert_eq!(comparisons, 1);
    assert_eq!(h.summary().unwrap()["input_authorized"], false);
    // The read itself recovered; this flag never means its predicate passed.
    assert_eq!(h.summary().unwrap()["latest_rejection_recovered"], true);
}

#[test]
fn count_exhaustion_and_strict_failure_keep_original_error_and_failed_history() {
    let start = Instant::now();
    let mut history = History::default();
    let mut calls = 0;
    let failure = history
        .observe_with::<()>(
            Phase::DestinationAfterDeparture,
            start + Duration::from_secs(5),
            || {
                calls += 1;
                Err(tear(&format!("original {calls}")))
            },
            || start,
            |_| {},
        )
        .unwrap_err();
    assert_eq!(calls, 4);
    assert_eq!(failure.code, "torn_read");
    assert_eq!(failure.message, "original 4");
    let s = history.summary().unwrap();
    assert_eq!(s["failed_observations"], 1);
    assert_eq!(s["rejected_reads"], 4);
    assert_eq!(s["first_failure"]["reason"], "original 1");
    assert_eq!(s["latest_failure"]["reason"], "original 4");
    assert_eq!(s["latest_rejection_recovered"], false);
    for code in [
        "ownership_changed",
        "short_read",
        "permission_denied",
        "research_precondition",
        "research_panel_transition",
        "research_identity_transition",
        "invalid_gui_type",
        "action_uncertain",
    ] {
        for after_tear in [false, true] {
            let mut h = History::default();
            let mut reads = 0;
            let e = h
                .observe_with::<()>(
                    Phase::DestinationInitial,
                    start + Duration::from_secs(5),
                    || {
                        reads += 1;
                        if after_tear && reads == 1 {
                            Err(tear("rejected whole snapshot"))
                        } else {
                            Err(Error::new(code, "strict original refusal"))
                        }
                    },
                    || start,
                    |_| {},
                )
                .unwrap_err();
            assert_eq!(e.code, code);
            assert_eq!(e.message, "strict original refusal");
            assert_eq!(reads, if after_tear { 2 } else { 1 });
            if after_tear {
                let s = h.summary().unwrap();
                assert_eq!(s["rejected_reads"], 1);
                assert_eq!(s["failed_observations"], 1);
                assert_eq!(s["latest_rejection_recovered"], false);
            } else {
                assert!(h.summary().is_none());
            }
        }
    }
}

#[test]
fn unchanged_original_deadline_and_one_second_window_reject_late_success() {
    for outer in [Duration::from_millis(10), Duration::from_secs(5)] {
        let start = Instant::now();
        let at = Cell::new(start);
        let mut history = History::default();
        let mut reads = 0;
        let mut sleeps = Vec::new();
        let e = history
            .observe_with(
                Phase::DestinationAfterDeparture,
                start + outer,
                || {
                    reads += 1;
                    if reads == 1 {
                        Err(tear("original before deadline"))
                    } else {
                        at.set(start + Duration::from_secs(1));
                        Ok("too late")
                    }
                },
                || at.get(),
                |d| {
                    sleeps.push(d);
                    at.set(at.get() + d);
                },
            )
            .unwrap_err();
        assert_eq!(e.code, "research_deadline");
        assert!(e
            .message
            .contains("phase=mixed_destination_after_departure"));
        assert_eq!(reads, if outer < Duration::from_secs(1) { 1 } else { 2 });
        assert_eq!(sleeps[0], outer.min(super::super::INTERVAL));
        let s = history.summary().unwrap();
        assert_eq!(s["failed_observations"], 1);
        assert_eq!(s["recovered_observations"], 0);
        assert_eq!(s["latest_failure"]["reason"], "original before deadline");
    }
}

#[test]
fn lifetime_keeps_only_bounded_first_latest_and_exact_recovery_counts() {
    let mut history = History::default();
    let start = Instant::now();
    for index in 0..128 {
        let mut reads = 0;
        let result = history.observe_with(
            Phase::SelectedIdentity,
            start + Duration::from_secs(5),
            || {
                reads += 1;
                if index % 2 == 0 && reads == 2 {
                    Ok(())
                } else {
                    Err(tear(&format!("{index}:{reads}\n{}", "x".repeat(10000))))
                }
            },
            || start,
            |_| {},
        );
        assert_eq!(result.is_ok(), index % 2 == 0);
    }
    let s = history.summary().unwrap();
    assert_eq!(s["rejected_reads"], 64 + 64 * 4);
    assert_eq!(s["recovered_observations"], 64);
    assert_eq!(s["failed_observations"], 64);
    assert_eq!(s["latest_rejection_recovered"], false);
    assert!(s["first_failure"]["reason"]
        .as_str()
        .unwrap()
        .starts_with("0:1x"));
    assert!(s["latest_failure"]["reason"]
        .as_str()
        .unwrap()
        .starts_with("127:4x"));
    for key in ["first_failure", "latest_failure"] {
        let reason = s[key]["reason"].as_str().unwrap();
        assert_eq!(reason.chars().count(), 192);
        assert!(!reason.chars().any(char::is_control));
    }
    assert!(serde_json::to_vec(&s).unwrap().len() < 1200);
    assert!(s.get("events").is_none());
}

#[test]
fn exhausted_history_capacity_preserves_terminal_error_and_refuses_later_reads() {
    let mut h = History {
        rejected_reads: u64::MAX,
        ..Default::default()
    };
    let start = Instant::now();
    let e = h
        .observe_with::<()>(
            Phase::DestinationInitial,
            start + Duration::from_secs(1),
            || Err(tear("actual terminal cause")),
            || start,
            |_| {},
        )
        .unwrap_err();
    assert_eq!(e.code, "torn_read");
    assert_eq!(e.message, "actual terminal cause");
    assert_eq!(h.summary().unwrap()["unavailable"], true);
    let e = h
        .observe_with::<()>(
            Phase::SelectedIdentity,
            start + Duration::from_secs(1),
            || panic!("Poisoned history must not read"),
            || start,
            |_| {},
        )
        .unwrap_err();
    assert_eq!(e.code, "research_history_full");
}
