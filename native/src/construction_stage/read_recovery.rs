//! Bounded replacement of wholly discarded producer snapshots before input.
//! No context acceptance, journal operation or transport belongs in this loop.
use super::Diagnostic;
use crate::error::{Error, Result};
use serde::Serialize;
use std::time::{Duration, Instant};

const MAX_READS: usize = 4;
const WINDOW: Duration = Duration::from_secs(1);
const INTERVAL: Duration = Duration::from_millis(25);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Initial,
    Candidate,
    BeforeIntent,
    AfterIntent,
    DispatchCheck,
    /// A read-only tool snapshot; never part of an action's input authority.
    ReadOnly,
}

#[derive(Default)]
pub(crate) struct Report(Vec<(Phase, Error)>);
impl Report {
    pub(crate) fn value(&self, token: &str) -> serde_json::Value {
        serde_json::json!(self
            .0
            .iter()
            .map(|(phase, cause)| {
                serde_json::json!({"phase":phase,"cause":Diagnostic::from_error(cause,token),
                "historical":true,"input_authorized":false})
            })
            .collect::<Vec<_>>())
    }
    pub(crate) fn with_error(&self, error: Error, token: &str) -> Error {
        if self.0.is_empty() {
            return error;
        }
        // Keep the terminal category, including NotQueued/Uncertain. These
        // discarded read-only samples never create a retryable action result.
        Error::new(
            error.code,
            format!(
                "{}; discarded_preinput_reads={}",
                error.message,
                self.value(token)
            ),
        )
    }
}

pub(crate) fn observe<T>(
    phase: Phase,
    until: Instant,
    producer: impl FnMut() -> Result<T>,
    report: &mut Report,
) -> Result<T> {
    recover(
        phase,
        until,
        producer,
        Instant::now,
        std::thread::sleep,
        report,
    )
}

fn deadline() -> Error {
    Error::new(
        "construction_deadline",
        "Construction read recovery exceeded its original bounded deadline",
    )
}

fn recover<T>(
    phase: Phase,
    until: Instant,
    mut producer: impl FnMut() -> Result<T>,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
    report: &mut Report,
) -> Result<T> {
    let mut end = until;
    for index in 0..MAX_READS {
        if now() >= end {
            return Err(deadline());
        }
        let sample = producer();
        if let Err(error) = &sample {
            if error.code == "torn_read" {
                report.0.push((
                    phase,
                    // Redact before truncation in Report::value. Truncating
                    // here could leave an unmatched prefix of a token/address.
                    Error::new(error.code, error.message.clone()),
                ));
            }
        }
        if now() >= end {
            return Err(deadline());
        }
        let cause = match sample {
            Ok(value) => return Ok(value),
            Err(error) if error.code == "torn_read" => error,
            Err(error) => return Err(error),
        };
        if index + 1 == MAX_READS {
            return Err(cause);
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
    unreachable!("last discarded sample returns its actual cause")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn whole_samples_recover_with_bounded_ordered_phase_evidence() {
        for phase in [
            Phase::Initial,
            Phase::Candidate,
            Phase::BeforeIntent,
            Phase::AfterIntent,
            Phase::DispatchCheck,
        ] {
            for failures in 0..4 {
                let start = Instant::now();
                let clock = Cell::new(start);
                let calls = Cell::new(0);
                let mut report = Report::default();
                let value = recover(
                    phase,
                    start + WINDOW,
                    || {
                        calls.set(calls.get() + 1);
                        if calls.get() <= failures {
                            Err(Error::new("torn_read", format!("sample {}", calls.get())))
                        } else {
                            Ok((calls.get(), "whole fresh snapshot"))
                        }
                    },
                    || clock.get(),
                    |delay| clock.set(clock.get() + delay),
                    &mut report,
                )
                .unwrap();
                assert_eq!(value, (failures + 1, "whole fresh snapshot"));
                assert_eq!(calls.get(), failures + 1);
                assert_eq!(clock.get() - start, INTERVAL * failures as u32);
                assert_eq!(report.0.len(), failures);
                for (i, (actual, error)) in report.0.iter().enumerate() {
                    assert_eq!(*actual, phase);
                    assert_eq!(error.message, format!("sample {}", i + 1));
                }
            }
        }
    }

    #[test]
    fn persistent_torn_reads_stop_at_four_and_preserve_last_cause() {
        let start = Instant::now();
        let clock = Cell::new(start);
        let calls = Cell::new(0);
        let mut report = Report::default();
        let error = recover::<()>(
            Phase::Initial,
            start + WINDOW,
            || {
                calls.set(calls.get() + 1);
                Err(Error::new("torn_read", format!("sample {}", calls.get())))
            },
            || clock.get(),
            |delay| clock.set(clock.get() + delay),
            &mut report,
        )
        .unwrap_err();
        assert_eq!(error.code, "torn_read");
        assert_eq!(error.message, "sample 4");
        assert_eq!(calls.get(), 4);
        assert_eq!(report.0.len(), 4);
    }

    #[test]
    fn non_torn_errors_and_valid_but_unready_state_are_never_retried() {
        for code in [
            "ownership_changed",
            "permission_denied",
            "short_read",
            "invalid_construction_state",
            "construction_precondition",
            "input_not_queued",
            "action_uncertain",
            "input_lease_required",
        ] {
            let start = Instant::now();
            let mut report = Report::default();
            let error = recover::<()>(
                Phase::DispatchCheck,
                start + WINDOW,
                || Err(Error::new(code, "actual cause")),
                || start,
                |_| panic!("No retry"),
                &mut report,
            )
            .unwrap_err();
            assert_eq!(error.code, code);
            assert!(report.0.is_empty());
        }
        let start = Instant::now();
        assert!(!recover(
            Phase::Initial,
            start + WINDOW,
            || Ok(false),
            || start,
            |_| panic!("No retry"),
            &mut Report::default()
        )
        .unwrap());
    }

    #[test]
    fn original_deadline_and_one_second_window_reject_late_success() {
        for duration in [
            Duration::ZERO,
            Duration::from_millis(10),
            Duration::from_secs(10),
        ] {
            let start = Instant::now();
            let clock = Cell::new(start);
            let calls = Cell::new(0);
            let mut report = Report::default();
            let error = recover(
                Phase::AfterIntent,
                start + duration,
                || {
                    calls.set(calls.get() + 1);
                    if calls.get() == 1 {
                        Err(Error::new("torn_read", "original"))
                    } else {
                        clock.set(start + WINDOW);
                        Ok("late")
                    }
                },
                || clock.get(),
                |delay| clock.set(clock.get() + delay),
                &mut report,
            )
            .unwrap_err();
            assert_eq!(error.code, "construction_deadline");
            assert_eq!(
                calls.get(),
                if duration.is_zero() {
                    0
                } else if duration < INTERVAL {
                    1
                } else {
                    2
                }
            );
        }
    }

    #[test]
    fn fresh_permanent_refusal_wins_and_diagnostics_cannot_change_action_category() {
        let start = Instant::now();
        let clock = Cell::new(start);
        let calls = Cell::new(0);
        let mut report = Report::default();
        let error = recover::<()>(
            Phase::BeforeIntent,
            start + WINDOW,
            || {
                calls.set(calls.get() + 1);
                Err(if calls.get() == 1 {
                    Error::new("torn_read", "first")
                } else {
                    Error::new("ownership_changed", "fresh refusal")
                })
            },
            || clock.get(),
            |delay| clock.set(clock.get() + delay),
            &mut report,
        )
        .unwrap_err();
        assert_eq!(error.code, "ownership_changed");
        assert_eq!(calls.get(), 2);
        for code in ["input_not_queued", "action_uncertain", "ownership_changed"] {
            let error = report.with_error(Error::new(code, "terminal cause"), "");
            assert_eq!(error.code, code);
            assert!(error.message.starts_with("terminal cause;"));
            assert!(error.message.contains("before_intent"));
        }
        report.0.push((
            Phase::Candidate,
            Error::new("torn_read", "secret-token\n0x123456789abc"),
        ));
        let value = report.value("secret-token").to_string();
        assert!(!value.contains("secret-token"));
        assert!(!value.contains("0x123456789abc"));
        assert!(value.contains("input_authorized\":false"));
    }

    #[test]
    fn a_secret_crossing_the_output_bound_is_redacted_before_truncation() {
        let start = Instant::now();
        let token = "do-not-leak-this-token";
        let mut report = Report::default();
        let _ = recover::<()>(
            Phase::Initial,
            start + WINDOW,
            || {
                Err(Error::new(
                    "torn_read",
                    format!("{} {token} 0x123456789abcdef", "x ".repeat(250)),
                ))
            },
            || start,
            |_| {},
            &mut report,
        );
        let value = report.value(token);
        for row in value.as_array().unwrap() {
            let message = row["cause"]["message"].as_str().unwrap();
            assert!(message.len() <= 512);
            assert!(!message.contains("do-not"));
            assert!(!message.contains("0x123"));
        }
    }
}
