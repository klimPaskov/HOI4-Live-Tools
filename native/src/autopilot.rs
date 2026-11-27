//! Resident executor switch: while on, the input lane runs one standing-plan step
//! (`hoi4_plan_run` for about a second, at most one action in practice) whenever no request
//! waits, instead of an outside loop polling the server and holding its only lane (w17: plan runs
//! held the lane 63% of the wall time). A queued request always goes first.
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

struct On {
    token: String,
    interval: Duration,
    next: Instant,
    steps: u64,
    actions: u64,
    last: Option<String>,
    /// Consecutive steps whose actions all failed: the wait doubles (up to 30 s).
    failing: u32,
    /// The game hour of the last quiet step: the next step is due as soon as the game clock has
    /// passed it (see [`Autopilot::due_at`]), the interval being only the longest wait.
    quiet_hour: Option<i64>,
}

#[derive(Default)]
pub struct Autopilot {
    on: Mutex<Option<On>>,
    /// Set only while an executor step runs on the input lane.
    in_step: AtomicBool,
    /// Armies the player ordered directly, with when: the executor leaves them alone.
    claims: Mutex<HashMap<String, Instant>>,
}

/// The longest wait after a step whose order failed.
const FAILED_STEP_WAIT: Duration = Duration::from_secs(3);

pub fn autopilot() -> &'static Autopilot {
    static A: OnceLock<Autopilot> = OnceLock::new();
    A.get_or_init(Autopilot::default)
}

impl Autopilot {
    pub fn enable(&self, token: &str, interval: Duration) {
        if let Ok(mut on) = self.on.lock() {
            *on = Some(On {
                token: token.to_string(),
                interval,
                next: Instant::now(),
                steps: 0,
                actions: 0,
                last: None,
                failing: 0,
                quiet_hour: None,
            });
        }
    }
    pub fn disable(&self) {
        if let Ok(mut on) = self.on.lock() {
            *on = None;
        }
    }
    /// The lease to run a step with, when one is due.
    pub fn due(&self) -> Option<String> {
        self.due_at(|| None)
    }
    /// The lease to run a step with, when one is due: its wait is over, or the last step was quiet
    /// and the game clock (`clock`, read only then) has moved past that step's hour. EVENT-DRIVEN:
    /// the lanes' rests and the plan's dates are game hours, so a quiet executor looks again on the
    /// next game hour, not after a fixed wait (at speed 3 a game hour is about a quarter second;
    /// a second's wait let four of them pass).
    pub fn due_at(&self, clock: impl FnOnce() -> Option<i64>) -> Option<String> {
        let (token, quiet) = {
            let on = self.on.lock().ok()?;
            let on = on.as_ref()?;
            if Instant::now() >= on.next {
                return Some(on.token.clone());
            }
            (on.token.clone(), on.quiet_hour?)
        };
        // the clock is read outside the lock
        clock().is_some_and(|hour| hour > quiet).then_some(token)
    }
    /// After a step at game hour `hour` (see [`Autopilot::ran_with`]).
    pub fn ran_at(
        &self,
        actions: usize,
        succeeded: usize,
        last: Option<String>,
        hour: Option<i64>,
    ) {
        self.ran_with(actions, succeeded, last);
        if let Ok(mut on) = self.on.lock() {
            if let Some(on) = on.as_mut() {
                on.quiet_hour = hour.filter(|_| actions == 0);
            }
        }
    }
    /// After a step: one with a successful action runs again at once (more may be waiting),
    /// a quiet one waits the interval, one whose actions all failed waits twice the interval.
    ///
    /// The wait after failures does not grow: a step is one order of one domain, and the order
    /// that failed is held back by the failure ledger of the plan runner (its own waits, then
    /// given up). A growing wait here stopped every domain for up to 30 s after each failed or
    /// held order: events stayed unanswered, slots empty and armies without orders for a
    /// minute at a time while one air order kept being refused (14 Nov 2026).
    pub fn ran(&self, actions: usize, last: Option<String>) {
        self.ran_with(actions, actions, last)
    }
    pub fn ran_with(&self, actions: usize, succeeded: usize, last: Option<String>) {
        if let Ok(mut on) = self.on.lock() {
            if let Some(on) = on.as_mut() {
                on.steps += 1;
                on.actions += actions as u64;
                if last.is_some() {
                    on.last = last;
                }
                on.quiet_hour = None;
                let wait = if succeeded > 0 {
                    on.failing = 0;
                    Duration::ZERO
                } else if actions > 0 {
                    on.failing = (on.failing + 1).min(8);
                    (on.interval * 2).min(FAILED_STEP_WAIT)
                } else {
                    on.interval
                };
                on.next = Instant::now() + wait;
            }
        }
    }
    /// Time until the next step is due (tests).
    #[cfg(test)]
    fn wait_left(&self) -> Option<Duration> {
        let on = self.on.lock().ok()?;
        on.as_ref()
            .map(|on| on.next.saturating_duration_since(Instant::now()))
    }
    pub fn set_in_step(&self, on: bool) {
        self.in_step.store(on, Ordering::SeqCst);
    }
    pub fn in_step(&self) -> bool {
        self.in_step.load(Ordering::SeqCst)
    }
    /// A direct player order on an army: the executor's war rules skip it for a while.
    pub fn claim(&self, army: &str) {
        if let Ok(mut c) = self.claims.lock() {
            c.insert(army.to_string(), Instant::now());
        }
    }
    pub fn release(&self, armies: &[String]) {
        if let Ok(mut c) = self.claims.lock() {
            for a in armies {
                c.remove(a);
            }
        }
    }
    /// Armies claimed within `window`.
    pub fn claimed(&self, window: Duration) -> Vec<String> {
        self.claims
            .lock()
            .map(|c| {
                let mut v: Vec<String> = c
                    .iter()
                    .filter(|(_, at)| at.elapsed() < window)
                    .map(|(a, _)| a.clone())
                    .collect();
                v.sort();
                v
            })
            .unwrap_or_default()
    }
    pub fn status(&self) -> Value {
        match self.on.lock().ok().as_deref() {
            Some(Some(on)) => {
                json!({"enabled": true, "interval_ms": on.interval.as_millis() as u64,
                "steps": on.steps, "actions": on.actions, "last_action": on.last})
            }
            _ => json!({"enabled": false}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Waits are checked as durations, not by sleeping past them: a loaded CI runner overslept
    // a 60 ms sleep into the next window (8 Nov 2026).
    #[test]
    fn quiet_steps_wait_for_the_interval_and_acting_steps_run_again() {
        let a = Autopilot::default();
        assert!(a.due().is_none());
        a.enable("t", Duration::from_secs(10));
        assert_eq!(a.due().as_deref(), Some("t"));
        a.ran(0, None);
        assert!(a.due().is_none());
        let left = a.wait_left().unwrap();
        assert!(left > Duration::from_secs(9) && left <= Duration::from_secs(10));
        a.ran(1, Some("x".into()));
        assert!(a.due().is_some());
        assert_eq!(a.status()["actions"], 1);
        a.disable();
        assert!(a.due().is_none());
    }
    #[test]
    fn failing_steps_back_off() {
        let a = Autopilot::default();
        a.enable("t", Duration::from_secs(1));
        let around = |want: u64| {
            let left = a.wait_left().unwrap();
            left > Duration::from_millis(want * 1000 - 500) && left <= Duration::from_secs(want)
        };
        a.ran_with(1, 0, Some("x: false".into()));
        assert!(a.due().is_none(), "a failed step does not rerun at once");
        assert!(around(2), "a failed step waits twice the interval");
        // more failures do not stop the other domains for longer
        for _ in 0..10 {
            a.ran_with(1, 0, Some("x: false".into()));
        }
        assert!(around(2), "the wait does not grow");
        a.ran_with(1, 1, None);
        assert!(a.due().is_some(), "a success reruns at once");
    }
    #[test]
    fn a_quiet_step_runs_again_on_the_next_game_hour_not_after_the_interval() {
        let a = Autopilot::default();
        a.enable("t", Duration::from_secs(10));
        a.ran_at(0, 0, None, Some(100));
        assert!(a.due().is_none(), "no clock: the interval");
        assert!(a.due_at(|| Some(100)).is_none(), "the same game hour waits");
        assert_eq!(
            a.due_at(|| Some(101)).as_deref(),
            Some("t"),
            "the next hour is due"
        );
        // a step that acted has no quiet hour: it runs again at once anyway
        a.ran_at(1, 1, Some("x".into()), Some(101));
        assert!(a.due_at(|| None).is_some());
        // a failed step waits its back-off whatever the clock does
        a.ran_at(1, 0, Some("x: false".into()), Some(102));
        assert!(a.due_at(|| Some(500)).is_none());
        // the clock is read only while a quiet step waits
        a.ran_at(0, 0, None, Some(102));
        let read = std::cell::Cell::new(0);
        let _ = a.due_at(|| {
            read.set(read.get() + 1);
            Some(102)
        });
        assert_eq!(read.get(), 1);
        a.ran_at(1, 1, None, Some(103));
        let _ = a.due_at(|| {
            read.set(read.get() + 1);
            None
        });
        assert_eq!(read.get(), 1, "due at once: no clock read");
    }
    #[test]
    fn claimed_armies_expire_and_can_be_released() {
        let a = Autopilot::default();
        a.claim("Army 1");
        a.claim("Army 2");
        assert_eq!(a.claimed(Duration::from_secs(60)), vec!["Army 1", "Army 2"]);
        assert!(a.claimed(Duration::ZERO).is_empty());
        a.release(&["Army 1".to_string()]);
        assert_eq!(a.claimed(Duration::from_secs(60)), vec!["Army 2"]);
    }
}
