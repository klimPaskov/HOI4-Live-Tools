//! Bounded owner-lifetime worker notifications, independent of game snapshots.
//! These records never authorize input or prove an assignment complete.
use crate::error::{Error, Result};
use serde::Serialize;
use std::sync::{Condvar, Mutex};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Stopped,
    Quarantined,
    RequestedStop,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Terminal {
    pub disposition: Disposition,
    pub code: String,
    pub reason: String,
    pub entry_id: Option<String>,
    pub phase: Option<&'static str>,
}
impl Terminal {
    pub fn new(disposition: Disposition, code: &str, reason: &str) -> Self {
        Self {
            disposition,
            code: code.chars().filter(|c| !c.is_control()).take(80).collect(),
            reason: reason
                .chars()
                .filter(|c| !c.is_control())
                .take(256)
                .collect(),
            entry_id: None,
            phase: None,
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub sequence: u64,
    pub instance: Option<u64>,
    pub plan_revision: Option<u64>,
    pub terminal: Option<Terminal>,
    pub retired: bool,
    unavailable: bool,
}
#[derive(Default)]
pub struct Events {
    current: Mutex<Snapshot>,
    changed: Condvar,
}
fn unavailable() -> Error {
    Error::new(
        "worker_events_unavailable",
        "Owned worker lifecycle notification unavailable",
    )
}
impl Events {
    pub fn snapshot(&self) -> Result<Snapshot> {
        let s = self.current.lock().map_err(|_| unavailable())?;
        if s.unavailable && !s.retired {
            return Err(unavailable());
        }
        Ok(s.clone())
    }
    pub fn start(&self, revision: u64) -> Result<u64> {
        let mut s = self.current.lock().map_err(|_| unavailable())?;
        if s.unavailable {
            return Err(unavailable());
        }
        if s.retired {
            return Err(Error::new(
                "session_closed",
                "Owned worker lifecycle retired",
            ));
        }
        let next = s
            .instance
            .unwrap_or(0)
            .checked_add(1)
            .zip(s.sequence.checked_add(1));
        let Some((instance, sequence)) = next else {
            s.unavailable = true;
            self.changed.notify_all();
            return Err(unavailable());
        };
        *s = Snapshot {
            sequence,
            instance: Some(instance),
            plan_revision: Some(revision),
            terminal: None,
            retired: false,
            unavailable: false,
        };
        self.changed.notify_all();
        Ok(instance)
    }
    /// Publish only after the worker has returned and input quarantine is retained.
    /// Repeated publication cannot replace the first terminal cause.
    pub fn finish(&self, instance: u64, revision: u64, terminal: Terminal) -> Result<()> {
        let mut s = self.current.lock().map_err(|_| unavailable())?;
        if s.unavailable {
            return Err(unavailable());
        }
        if s.instance != Some(instance) {
            return Err(Error::new(
                "daily_worker_replaced",
                "Terminal worker instance differs",
            ));
        }
        if s.terminal.is_none() {
            let Some(sequence) = s.sequence.checked_add(1) else {
                s.unavailable = true;
                self.changed.notify_all();
                return Err(unavailable());
            };
            s.sequence = sequence;
            s.plan_revision = Some(revision);
            s.terminal = Some(terminal);
            self.changed.notify_all();
        }
        Ok(())
    }
    /// Absorbing retirement wakes clients before the owner's read-gate drainage.
    pub fn retire(&self) -> Result<()> {
        let mut s = self.current.lock().map_err(|_| unavailable())?;
        if !s.retired {
            // Retirement remains visible even if the notification counter fills.
            s.retired = true;
            self.changed.notify_all();
            // Retired is checked before the counter by consumers.
            s.sequence = s.sequence.saturating_add(1);
        }
        Ok(())
    }
    pub fn wait_after(&self, sequence: u64, until: Instant) -> Result<Snapshot> {
        let mut s = self.current.lock().map_err(|_| unavailable())?;
        while s.sequence == sequence && !s.retired && !s.unavailable {
            let remaining = until.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            s = self
                .changed
                .wait_timeout(s, remaining)
                .map_err(|_| unavailable())?
                .0;
        }
        if s.unavailable && !s.retired {
            return Err(unavailable());
        }
        Ok(s.clone())
    }
}

#[cfg(test)]
mod tests;
