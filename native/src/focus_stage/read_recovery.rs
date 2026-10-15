//! Discard wholly torn read-only snapshots before the one SearchQuery dispatch.
//! This helper never wraps transport, lease checks, input or journal execution.
use crate::error::{Error, Result};
use std::time::{Duration, Instant};

const MAX_READS: usize = 4;
const WINDOW: Duration = Duration::from_secs(1);
const INTERVAL: Duration = Duration::from_millis(25);

#[cfg(target_os = "linux")]
pub(crate) fn before_query<T>(
    until: Instant,
    observe: impl FnMut() -> Result<T>,
    rejected: &mut Vec<Error>,
) -> Result<T> {
    recover(until, observe, Instant::now, std::thread::sleep, rejected)
}

fn deadline() -> Error {
    Error::new(
        "focus_deadline",
        "Search query read recovery exceeded its original bounded deadline",
    )
}

// This observer is tested on every platform. Keep its diagnostic formatting
// independent of the Unix-only durable navigation implementation.
pub(super) fn bounded_reason(message: &str) -> String {
    let mut out = String::new();
    for c in message.chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > 160 {
            break;
        }
        out.push(c);
    }
    out
}

fn recover<T>(
    until: Instant,
    mut observe: impl FnMut() -> Result<T>,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
    rejected: &mut Vec<Error>,
) -> Result<T> {
    let mut end = until;
    for index in 0..MAX_READS {
        if now() >= end {
            return Err(deadline());
        }
        let sample = observe();
        if let Err(error) = &sample {
            if error.code == "torn_read" {
                rejected.push(Error::new(error.code, bounded_reason(&error.message)));
            }
        }
        if now() >= end {
            return Err(deadline());
        }
        let error = match sample {
            Ok(value) => return Ok(value),
            Err(error) if error.code == "torn_read" => error,
            Err(error) => return Err(error),
        };
        if index + 1 == MAX_READS {
            return Err(error);
        }
        if index == 0 {
            end = end.min(now() + WINDOW);
        }
        let at = now();
        if at >= end {
            return Err(deadline());
        }
        sleep(INTERVAL.min(end - at));
    }
    unreachable!("Last wholly discarded read returns its actual error")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn portable_diagnostic_bound_preserves_utf8_and_removes_control_characters() {
        assert_eq!(bounded_reason("original\n\t field\0"), "original field");
        assert_eq!(bounded_reason(&"é".repeat(100)), "é".repeat(80));
        assert_eq!(bounded_reason(&("a".repeat(159) + "é")), "a".repeat(159));
    }

    #[test]
    fn fresh_success_is_returned_once_and_torn_snapshots_are_wholly_discarded() {
        for failures in 0..4 {
            let start = Instant::now();
            let clock = Cell::new(start);
            let calls = Cell::new(0);
            let mut rejected = vec![];
            let value = recover(
                start + WINDOW,
                || {
                    calls.set(calls.get() + 1);
                    if calls.get() <= failures {
                        Err(Error::new("torn_read", format!("field {}", calls.get())))
                    } else {
                        Ok((calls.get(), "complete fresh snapshot"))
                    }
                },
                || clock.get(),
                |delay| clock.set(clock.get() + delay),
                &mut rejected,
            )
            .unwrap();
            assert_eq!(value, (failures + 1, "complete fresh snapshot"));
            assert_eq!(calls.get(), failures + 1);
            assert_eq!(rejected.len(), failures);
            assert_eq!(clock.get() - start, INTERVAL * failures as u32);
            for (i, error) in rejected.iter().enumerate() {
                assert_eq!(error.message, format!("field {}", i + 1));
            }
        }
    }

    #[test]
    fn persistent_torn_reads_stop_at_four_and_keep_original_order() {
        let start = Instant::now();
        let clock = Cell::new(start);
        let calls = Cell::new(0);
        let mut rejected = vec![];
        let error = recover::<()>(
            start + WINDOW,
            || {
                calls.set(calls.get() + 1);
                Err(Error::new("torn_read", format!("field {}", calls.get())))
            },
            || clock.get(),
            |delay| clock.set(clock.get() + delay),
            &mut rejected,
        )
        .unwrap_err();
        assert_eq!(error.code, "torn_read");
        assert_eq!(error.message, "field 4");
        assert_eq!(calls.get(), MAX_READS);
        assert_eq!(rejected.len(), MAX_READS);
        assert_eq!(rejected[0].message, "field 1");
    }

    #[test]
    fn permanent_errors_and_valid_but_unready_state_never_retry() {
        for code in [
            "ownership_changed",
            "focus_precondition",
            "invalid_focus_type",
            "permission_denied",
            "input_not_queued",
            "action_uncertain",
            "input_lease_required",
        ] {
            let start = Instant::now();
            let mut rejected = vec![];
            let error = recover::<()>(
                start + WINDOW,
                || Err(Error::new(code, "actual refusal")),
                || start,
                |_| panic!("No retry"),
                &mut rejected,
            )
            .unwrap_err();
            assert_eq!(error.code, code);
            assert!(rejected.is_empty());
        }
        let start = Instant::now();
        assert!(!recover(
            start + WINDOW,
            || Ok(false),
            || start,
            |_| panic!("No retry of a valid state"),
            &mut vec![]
        )
        .unwrap());
    }

    #[test]
    fn original_deadline_and_retry_window_reject_late_success() {
        for end_after in [
            Duration::ZERO,
            Duration::from_millis(10),
            Duration::from_secs(10),
        ] {
            let start = Instant::now();
            let clock = Cell::new(start);
            let calls = Cell::new(0);
            let mut rejected = vec![];
            let error = recover(
                start + end_after,
                || {
                    calls.set(calls.get() + 1);
                    if calls.get() == 1 {
                        Err(Error::new("torn_read", "original"))
                    } else {
                        clock.set(start + WINDOW);
                        Ok("too late")
                    }
                },
                || clock.get(),
                |delay| clock.set(clock.get() + delay),
                &mut rejected,
            )
            .unwrap_err();
            assert_eq!(error.code, "focus_deadline");
            assert_eq!(
                calls.get(),
                if end_after.is_zero() {
                    0
                } else if end_after < INTERVAL {
                    1
                } else {
                    2
                }
            );
            assert_eq!(rejected.len(), usize::from(!end_after.is_zero()));
        }
    }

    #[test]
    fn followup_scope_failure_is_not_hidden_by_original_torn_read() {
        let start = Instant::now();
        let clock = Cell::new(start);
        let calls = Cell::new(0);
        let mut rejected = vec![];
        let error = recover::<()>(
            start + WINDOW,
            || {
                calls.set(calls.get() + 1);
                Err(if calls.get() == 1 {
                    Error::new("torn_read", "discarded original")
                } else {
                    Error::new("ownership_changed", "fresh refusal")
                })
            },
            || clock.get(),
            |delay| clock.set(clock.get() + delay),
            &mut rejected,
        )
        .unwrap_err();
        assert_eq!(error.code, "ownership_changed");
        assert_eq!(calls.get(), 2);
        assert_eq!(rejected.len(), 1);
    }
}
