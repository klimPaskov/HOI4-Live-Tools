//! Whole read-only Research snapshots only. Never wrap transport, scope
//! acceptance, predicates, journal execution or the original-state binding.
use crate::error::{Error, Result};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

const MAX_READS: usize = 4;
const WINDOW: Duration = Duration::from_secs(1);
const INTERVAL: Duration = Duration::from_millis(25);

pub(crate) mod history;

pub(crate) fn observe<T>(
    phase: &'static str,
    until: Instant,
    read: impl FnMut() -> Result<T>,
    history: &mut Vec<Value>,
) -> Result<T> {
    let mut rejected = Vec::new();
    let result = recover(until, read, Instant::now, std::thread::sleep, &mut rejected)
        .map_err(|cause| at_phase(cause, phase));
    if !rejected.is_empty() {
        history.push(json!({"phase":phase,"rejected_reads":rejected,
            "historical":true,"input_authorized":false}));
    }
    result
}
fn at_phase(mut cause: Error, phase: &'static str) -> Error {
    if cause.code == "research_deadline" {
        cause.message.push_str(&format!("; phase={phase}"));
    }
    cause
}
fn deadline() -> Error {
    Error::new(
        "research_deadline",
        "Research snapshot recovery exceeded its original deadline",
    )
}
fn recover<T>(
    until: Instant,
    mut read: impl FnMut() -> Result<T>,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
    rejected: &mut Vec<Error>,
) -> Result<T> {
    let mut end = until;
    for index in 0..MAX_READS {
        if now() >= end {
            return Err(deadline());
        }
        let sample = read();
        if let Err(error) = &sample {
            if error.code == "torn_read" {
                let reason: String = error
                    .message
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(192)
                    .collect();
                rejected.push(Error::new(error.code, reason));
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
    unreachable!("Final rejected snapshot returns its actual cause")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    #[test]
    fn fast_success_has_no_delay_or_rejected_history() {
        let mut errors = Vec::new();
        let start = Instant::now();
        assert_eq!(
            recover(
                start + WINDOW,
                || Ok(17),
                || start,
                |_| panic!("Successful read must not wait"),
                &mut errors
            )
            .unwrap(),
            17
        );
        assert!(errors.is_empty());
    }
    #[test]
    fn complete_snapshots_retry_with_fixed_bound_and_preserve_original_error() {
        let start = Instant::now();
        let at = Cell::new(start);
        let reads = Cell::new(0);
        for succeed in [false, true] {
            reads.set(0);
            at.set(start);
            let mut errors = Vec::new();
            let result = recover(
                start + WINDOW,
                || {
                    reads.set(reads.get() + 1);
                    if succeed && reads.get() == 3 {
                        Ok("fresh whole snapshot")
                    } else {
                        Err(Error::new("torn_read", format!("original {}", reads.get())))
                    }
                },
                || at.get(),
                |d| at.set(at.get() + d),
                &mut errors,
            );
            if succeed {
                assert_eq!(result.unwrap(), "fresh whole snapshot");
                assert_eq!(reads.get(), 3);
            } else {
                assert_eq!(result.unwrap_err().message, "original 4");
                assert_eq!(reads.get(), 4);
            }
            assert_eq!(errors[0].message, "original 1");
            assert_eq!(at.get() - start, INTERVAL * (reads.get() - 1));
        }
    }
    #[test]
    fn permanent_ownership_precondition_and_io_errors_never_retry() {
        for code in [
            "ownership_changed",
            "short_read",
            "research_precondition",
            "research_panel_transition",
            "action_uncertain",
        ] {
            let start = Instant::now();
            let mut rejected = Vec::new();
            let reads = Cell::new(0);
            let error = recover::<()>(
                start + WINDOW,
                || {
                    reads.set(reads.get() + 1);
                    Err(Error::new(code, "original"))
                },
                || start,
                |_| panic!("Permanent errors must not wait"),
                &mut rejected,
            )
            .unwrap_err();
            assert_eq!(error.code, code);
            assert_eq!(reads.get(), 1);
            assert!(rejected.is_empty());
        }
    }
    #[test]
    fn original_deadline_and_late_success_never_gain_more_time() {
        for late in [false, true] {
            let start = Instant::now();
            let at = Cell::new(start);
            let reads = Cell::new(0);
            let mut rejected = Vec::new();
            let result = recover(
                start + Duration::from_millis(10),
                || {
                    reads.set(reads.get() + 1);
                    if late {
                        at.set(start + Duration::from_millis(10));
                        Ok(())
                    } else {
                        Err(Error::new("torn_read", "original"))
                    }
                },
                || at.get(),
                |d| at.set(at.get() + d),
                &mut rejected,
            );
            assert_eq!(result.unwrap_err().code, "research_deadline");
            assert_eq!(reads.get(), 1);
        }
    }
    #[test]
    fn phases_and_rejections_remain_diagnostic_not_input_authority() {
        let mut history = Vec::new();
        for phase in ["initial", "candidate", "final", "pre_queue"] {
            let mut calls = 0;
            let result = observe(
                phase,
                Instant::now() + WINDOW,
                || {
                    calls += 1;
                    if calls == 1 {
                        Err(Error::new("torn_read", "retained witness"))
                    } else {
                        Ok(calls)
                    }
                },
                &mut history,
            );
            assert_eq!(result.unwrap(), 2);
        }
        assert_eq!(history.len(), 4);
        assert_eq!(history[3]["phase"], "pre_queue");
        assert_eq!(
            history[0]["rejected_reads"][0]["message"],
            "retained witness"
        );
        assert_eq!(history[0]["input_authorized"], false);
    }
}
