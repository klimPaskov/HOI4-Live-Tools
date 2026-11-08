//! Resident executor switch: while on, the input lane runs one standing-plan step
//! (`hoi4_plan_run` for about a second, at most one action in practice) whenever no request
//! waits, instead of an outside loop polling the server and holding its only lane (w17: plan runs
//! held the lane 63% of the wall time). A queued request always goes first.
use serde_json::{json, Value};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

struct On {
    token: String,
    interval: Duration,
    next: Instant,
    steps: u64,
    actions: u64,
    last: Option<String>,
}

#[derive(Default)]
pub struct Autopilot {
    on: Mutex<Option<On>>,
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
    /// After a step: acted steps run again at once (more may be waiting), quiet ones wait.
    pub fn ran(&self, actions: usize, last: Option<String>) {
        if let Ok(mut on) = self.on.lock() {
            if let Some(on) = on.as_mut() {
                on.steps += 1;
                on.actions += actions as u64;
                if last.is_some() {
                    on.last = last;
                }
                on.next = Instant::now()
                    + if actions > 0 {
                        Duration::ZERO
                    } else {
                        on.interval
                    };
            }
        }
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
}
