//! Bounded fresh observations before any navigation intent or assignment.
use super::{next, Next, OwnState, Target};
use crate::error::{Error, Result};
use std::time::{Duration, Instant};

const MAX_READS: usize = 4;
const RETRY_WINDOW: Duration = Duration::from_secs(1);
const INTERVAL: Duration = Duration::from_millis(100);

pub(crate) struct Ready<T> {
    pub next: Next,
    pub own: OwnState,
    pub observed: T,
}

#[cfg(target_os = "linux")]
pub(crate) fn observed_ready<T>(
    target: &Target,
    until: Instant,
    observe: impl FnMut() -> Result<Option<(OwnState, T)>>,
) -> Result<Option<Ready<T>>> {
    observed_ready_with(target, until, observe, Instant::now, std::thread::sleep)
}

fn deadline() -> Error {
    Error::new(
        "focus_deadline",
        "Focus readiness observation exceeded its bounded deadline",
    )
}

fn observed_ready_with<T>(
    target: &Target,
    until: Instant,
    mut observe: impl FnMut() -> Result<Option<(OwnState, T)>>,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
) -> Result<Option<Ready<T>>> {
    let mut end = until;
    for index in 0..MAX_READS {
        if now() >= end {
            return Err(deadline());
        }
        // The caller rechecks lease/gate/scope/day on every invocation. None
        // means cancellation or day change; neither is a navigation refusal.
        let sample = observe()?;
        if now() >= end {
            return Err(deadline());
        }
        let Some((own, observed)) = sample else {
            return Ok(None);
        };
        // Never turn invalid canonical producer data into UI readiness retries.
        own.validate(target)?;
        match next(&own, target) {
            Ok(next) => {
                return Ok(Some(Ready {
                    next,
                    own,
                    observed,
                }))
            }
            Err(error) if error.code == "focus_precondition" => {
                if index + 1 == MAX_READS {
                    return Err(error);
                }
                if index == 0 {
                    end = end.min(now() + RETRY_WINDOW);
                }
                let at = now();
                if at >= end {
                    return Err(deadline());
                }
                sleep(INTERVAL.min(end - at));
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!("Last rejected read returns its actual precondition")
}

#[cfg(test)]
mod tests {
    use super::super::tests::{state, target};
    use super::*;
    use crate::focus_stage::{ActiveFocus, LocalRoot, Stage};
    use std::cell::Cell;

    fn ready() -> OwnState {
        let mut own = state();
        own.managed_mode = Some(5);
        own.research_panel_root = LocalRoot::Ready;
        own.research_tree_root = LocalRoot::Hidden;
        own
    }

    #[test]
    fn fresh_readiness_returns_only_the_latest_positive_state_and_binding() {
        let start = Instant::now();
        let time = Cell::new(start);
        let reads = Cell::new(0);
        let result = observed_ready_with(
            &target(Stage::Start),
            start + Duration::from_secs(10),
            || {
                reads.set(reads.get() + 1);
                let mut own = ready();
                if reads.get() < 3 {
                    own.research_panel_root = LocalRoot::Unknown;
                }
                Ok(Some((own, reads.get())))
            },
            || time.get(),
            |delay| time.set(time.get() + delay),
        )
        .unwrap()
        .unwrap();
        assert_eq!(reads.get(), 3);
        assert_eq!(result.observed, 3);
        assert_eq!(result.own.research_panel_root, LocalRoot::Ready);
        assert!(matches!(
            result.next,
            Next::Stage(Stage::OpenPoliticsFromResearch)
        ));
        assert_eq!(time.get() - start, INTERVAL * 2);
    }

    #[test]
    fn ready_or_active_focus_returns_once_without_sleep_or_new_authority() {
        for active in [
            ActiveFocus::Idle,
            ActiveFocus::Active("GER_other".into()),
            ActiveFocus::Active(target(Stage::Start).focus_id),
        ] {
            let reads = Cell::new(0);
            let start = Instant::now();
            let result = observed_ready_with(
                &target(Stage::Start),
                start + RETRY_WINDOW,
                || {
                    reads.set(reads.get() + 1);
                    let mut own = ready();
                    own.active_focus = active.clone();
                    Ok(Some((own, ())))
                },
                || start,
                |_| panic!("Ready/active state never delays"),
            )
            .unwrap()
            .unwrap();
            assert_eq!(reads.get(), 1);
            match active {
                ActiveFocus::Idle => assert!(matches!(
                    result.next,
                    Next::Stage(Stage::OpenPoliticsFromResearch)
                )),
                ActiveFocus::Active(key) if key == target(Stage::Start).focus_id => {
                    assert!(matches!(result.next, Next::Start))
                }
                _ => assert!(matches!(result.next, Next::Wait)),
            }
        }
    }

    #[test]
    fn initial_valid_read_keeps_the_original_batch_deadline() {
        let start = Instant::now();
        let time = Cell::new(start);
        let result = observed_ready_with(
            &target(Stage::Start),
            start + Duration::from_secs(10),
            || {
                time.set(start + Duration::from_secs(2));
                Ok(Some((ready(), ())))
            },
            || time.get(),
            |_| panic!("Positive initial read never receives a retry limit"),
        )
        .unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn error_on_a_fresh_followup_is_not_replaced_by_the_earlier_ui_refusal() {
        let start = Instant::now();
        let time = Cell::new(start);
        let reads = Cell::new(0);
        let error = observed_ready_with(
            &target(Stage::Start),
            start + Duration::from_secs(10),
            || {
                reads.set(reads.get() + 1);
                if reads.get() == 1 {
                    let mut own = ready();
                    own.research_tree_root = LocalRoot::Unknown;
                    Ok(Some((own, ())))
                } else {
                    Err(Error::new(
                        "ownership_changed",
                        "Actual followup scope failure",
                    ))
                }
            },
            || time.get(),
            |delay| time.set(time.get() + delay),
        )
        .err()
        .unwrap();
        assert_eq!(reads.get(), 2);
        assert_eq!(error.code, "ownership_changed");
        assert_eq!(error.message, "Actual followup scope failure");
    }

    #[test]
    fn persistent_unreadiness_has_four_reads_and_keeps_last_exact_refusal() {
        let start = Instant::now();
        let time = Cell::new(start);
        let reads = Cell::new(0);
        let error = observed_ready_with(
            &target(Stage::Start),
            start + Duration::from_secs(10),
            || {
                reads.set(reads.get() + 1);
                let mut own = ready();
                own.own_research_info_counts = Some([reads.get(), 0]);
                Ok(Some((own, ())))
            },
            || time.get(),
            |delay| time.set(time.get() + delay),
        )
        .err()
        .unwrap();
        assert_eq!(reads.get(), MAX_READS);
        assert_eq!(error.code, "focus_precondition");
        assert!(error.message.contains("info=4/0"));
        assert_eq!(time.get() - start, INTERVAL * 3);
    }

    #[test]
    fn original_deadline_blocks_new_reads_and_discards_late_success() {
        for late in [Duration::ZERO, Duration::from_millis(100)] {
            let start = Instant::now();
            let time = Cell::new(start);
            let reads = Cell::new(0);
            let error = observed_ready_with(
                &target(Stage::Start),
                start + late,
                || {
                    reads.set(reads.get() + 1);
                    time.set(start + late);
                    Ok(Some((ready(), ())))
                },
                || time.get(),
                |_| panic!("No delay after deadline"),
            )
            .err()
            .unwrap();
            assert_eq!(error.code, "focus_deadline");
            assert_eq!(reads.get(), usize::from(late != Duration::ZERO));
        }
    }

    #[test]
    fn retry_window_caps_sleep_and_rejects_payload_after_its_boundary() {
        let start = Instant::now();
        let time = Cell::new(start);
        let reads = Cell::new(0);
        let error = observed_ready_with(
            &target(Stage::Start),
            start + Duration::from_secs(10),
            || {
                reads.set(reads.get() + 1);
                let mut own = ready();
                if reads.get() == 1 {
                    own.research_tree_root = LocalRoot::Unknown;
                } else {
                    time.set(start + RETRY_WINDOW);
                }
                Ok(Some((own, ())))
            },
            || time.get(),
            |delay| time.set(time.get() + delay),
        )
        .err()
        .unwrap();
        assert_eq!(reads.get(), 2);
        assert_eq!(error.code, "focus_deadline");
        let time = Cell::new(start);
        let reads = Cell::new(0);
        let error = observed_ready_with(
            &target(Stage::Start),
            start + Duration::from_millis(30),
            || {
                reads.set(reads.get() + 1);
                {
                    let mut own = ready();
                    own.research_panel_root = LocalRoot::Unknown;
                    Ok(Some((own, ())))
                }
            },
            || time.get(),
            |delay| {
                assert_eq!(delay, Duration::from_millis(30));
                time.set(time.get() + delay)
            },
        )
        .err()
        .unwrap();
        assert_eq!(reads.get(), 1);
        assert_eq!(error.code, "focus_deadline");
    }

    #[test]
    fn reader_scope_validation_and_unknown_focus_errors_never_retry() {
        for code in [
            "torn_read",
            "ownership_changed",
            "action_uncertain",
            "lease_expired",
        ] {
            let reads = Cell::new(0);
            let start = Instant::now();
            let error = observed_ready_with::<()>(
                &target(Stage::Start),
                start + RETRY_WINDOW,
                || {
                    reads.set(reads.get() + 1);
                    Err(Error::new(code, "Original observer failure"))
                },
                || start,
                |_| panic!("Observer errors never retry"),
            )
            .err()
            .unwrap();
            assert_eq!(error.code, code);
            assert_eq!(reads.get(), 1);
        }
        for invalid in 0..3 {
            let start = Instant::now();
            let reads = Cell::new(0);
            let mut own = ready();
            match invalid {
                0 => own.canonical_focus_id = None,
                1 => own.own_detail_counts = Some([33, 0]),
                _ => own.active_focus = ActiveFocus::Unknown,
            }
            assert!(observed_ready_with(
                &target(Stage::Start),
                start + RETRY_WINDOW,
                || {
                    reads.set(reads.get() + 1);
                    Ok(Some((own.clone(), ())))
                },
                || start,
                |_| panic!("Invalid producer is not UI readiness")
            )
            .is_err());
            assert_eq!(reads.get(), 1);
        }
    }

    #[test]
    fn cancellation_or_day_change_during_readiness_returns_without_a_stage() {
        let start = Instant::now();
        let time = Cell::new(start);
        let reads = Cell::new(0);
        let result = observed_ready_with(
            &target(Stage::Start),
            start + RETRY_WINDOW,
            || {
                reads.set(reads.get() + 1);
                if reads.get() == 1 {
                    let mut own = ready();
                    own.research_tree_root = LocalRoot::Unknown;
                    Ok(Some((own, ())))
                } else {
                    Ok(None)
                }
            },
            || time.get(),
            |delay| time.set(time.get() + delay),
        )
        .unwrap();
        assert!(result.is_none());
        assert_eq!(reads.get(), 2);
    }
}
