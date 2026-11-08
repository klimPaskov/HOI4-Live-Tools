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
}

#[derive(Default)]
pub struct Autopilot {
    on: Mutex<Option<On>>,
    /// Set only while an executor step runs on the input lane.
    in_step: AtomicBool,
    /// Armies the player ordered directly, with when: the executor leaves them alone.
    claims: Mutex<HashMap<String, Instant>>,
}

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
        let on = self.on.lock().ok()?;
        let on = on.as_ref()?;
        (Instant::now() >= on.next).then(|| on.token.clone())
    }
    /// After a step: one with a successful action runs again at once (more may be waiting),
    /// a quiet one waits the interval, one whose actions all failed waits longer each time.
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
                let wait = if succeeded > 0 {
                    on.failing = 0;
                    Duration::ZERO
                } else if actions > 0 {
                    on.failing = (on.failing + 1).min(8);
                    (on.interval * (1 << on.failing.min(5))).min(Duration::from_secs(30))
                } else {
                    on.interval
                };
                on.next = Instant::now() + wait;
            }
        }
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
    #[test]
    fn quiet_steps_wait_for_the_interval_and_acting_steps_run_again() {
        let a = Autopilot::default();
        assert!(a.due().is_none());
        a.enable("t", Duration::from_millis(80));
        assert_eq!(a.due().as_deref(), Some("t"));
        a.ran(0, None);
        assert!(a.due().is_none());
        std::thread::sleep(Duration::from_millis(90));
        assert!(a.due().is_some());
        a.ran(1, Some("x".into()));
        assert!(a.due().is_some());
        assert_eq!(a.status()["actions"], 1);
        a.disable();
        assert!(a.due().is_none());
    }
    #[test]
    fn failing_steps_back_off() {
        let a = Autopilot::default();
        a.enable("t", Duration::from_millis(50));
        a.ran_with(1, 0, Some("x: false".into()));
        assert!(a.due().is_none(), "a failed step does not rerun at once");
        std::thread::sleep(Duration::from_millis(60));
        assert!(a.due().is_none(), "the wait doubled");
        std::thread::sleep(Duration::from_millis(60));
        assert!(a.due().is_some());
        a.ran_with(1, 1, None);
        assert!(a.due().is_some(), "a success reruns at once");
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
