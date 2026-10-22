//! Join/stop ownership only; no game input, reader or strategy.
#[cfg(any(target_os = "linux", all(test, unix)))]
mod clock_probe;
use crate::{
    daily_governor::{WakeReason, Wakeup},
    error::{Error, Result},
};
#[cfg(target_os = "linux")]
pub(crate) use clock_probe::ClockProbeHistory;
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
};
/// Recheck uncertainty after controller acquisition. A publication between the
/// optimistic check and try_lock must not authorize a manual input operation.
pub(crate) fn input_state<'a, T>(
    state: &'a std::sync::Mutex<T>,
    mut check: impl FnMut() -> Result<()>,
) -> Result<std::sync::MutexGuard<'a, T>> {
    check()?;
    let guard = state.try_lock().map_err(|e| match e {
        std::sync::TryLockError::WouldBlock => {
            Error::new("input_busy", "Shared controller is in use")
        }
        std::sync::TryLockError::Poisoned(_) => {
            Error::new("action_uncertain", "Shared controller is poisoned")
        }
    })?;
    check()?;
    Ok(guard)
}
pub(crate) struct Lifecycle {
    stop: Arc<AtomicBool>,
    wake: Arc<Wakeup>,
    handle: Option<JoinHandle<()>>,
}
impl Lifecycle {
    pub(crate) fn new(stop: Arc<AtomicBool>, wake: Arc<Wakeup>, handle: JoinHandle<()>) -> Self {
        Self {
            stop,
            wake,
            handle: Some(handle),
        }
    }
    pub(crate) fn running(&self) -> bool {
        !self.stop.load(Ordering::Acquire) && self.handle.as_ref().is_some_and(|h| !h.is_finished())
    }
    pub(crate) fn stop(&mut self) -> Result<()> {
        self.stop.store(true, Ordering::Release);
        self.wake.notify(WakeReason::Shutdown)?;
        if let Some(h) = self.handle.take() {
            if h.thread().id() != thread::current().id() {
                h.join().map_err(|_| {
                    Error::new(
                        "daily_quarantined",
                        "Worker panic; retain input uncertainty",
                    )
                })?;
            }
        }
        Ok(())
    }
}
impl Drop for Lifecycle {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn quarantine_published_between_precheck_and_lock_refuses_manual_work() {
        let state = Arc::new(std::sync::Mutex::new(0));
        let blocked = Arc::new(AtomicBool::new(false));
        let (send, recv) = std::sync::mpsc::channel();
        let (released, waited) = std::sync::mpsc::channel();
        let s = state.clone();
        let b = blocked.clone();
        let worker = thread::spawn(move || {
            recv.recv().unwrap();
            let guard = s.lock().unwrap();
            b.store(true, Ordering::Release);
            drop(guard);
            released.send(()).unwrap();
        });
        let mut first = true;
        let result = input_state(&state, || {
            if blocked.load(Ordering::Acquire) {
                return Err(Error::new("action_uncertain", "Generated quarantine"));
            }
            if first {
                first = false;
                send.send(()).unwrap();
                waited.recv_timeout(Duration::from_secs(1)).unwrap();
            }
            Ok(())
        });
        assert_eq!(result.unwrap_err().code, "action_uncertain");
        worker.join().unwrap();
        assert_eq!(*state.lock().unwrap(), 0);
    }
    #[test]
    fn manual_input_lock_is_nonblocking_and_poison_is_uncertain() {
        let state = std::sync::Mutex::new(0);
        let guard = state.lock().unwrap();
        assert_eq!(
            input_state(&state, || Ok(())).unwrap_err().code,
            "input_busy"
        );
        drop(guard);
        let _ = std::panic::catch_unwind(|| {
            let _guard = state.lock().unwrap();
            panic!("Generated controller panic");
        });
        assert_eq!(
            input_state(&state, || Ok(())).unwrap_err().code,
            "action_uncertain"
        );
    }
    #[test]
    fn condvar_shutdown_joins_without_controller_lock() {
        let stop = Arc::new(AtomicBool::new(false));
        let wake = Arc::new(Wakeup::default());
        let (send, recv) = std::sync::mpsc::channel();
        let s = stop.clone();
        let w = wake.clone();
        let h = thread::spawn(move || {
            let mut n = 0;
            while !s.load(Ordering::Acquire) {
                n = w.wait_after(n, Duration::from_secs(60)).unwrap().generation;
            }
            send.send(()).unwrap();
        });
        let mut life = Lifecycle::new(stop, wake, h);
        life.stop().unwrap();
        recv.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(!life.running());
    }
    #[test]
    fn final_owner_drop_on_worker_thread_never_joins_itself() {
        let slot = Arc::new(std::sync::Mutex::new(None));
        let last = slot.clone();
        let (go, wait) = std::sync::mpsc::channel();
        let (send, recv) = std::sync::mpsc::channel();
        let h = thread::spawn(move || {
            wait.recv().unwrap();
            drop(last);
            send.send(()).unwrap();
        });
        *slot.lock().unwrap() = Some(Lifecycle::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(Wakeup::default()),
            h,
        ));
        drop(slot);
        go.send(()).unwrap();
        recv.recv_timeout(Duration::from_secs(1)).unwrap();
    }
    #[test]
    fn worker_panic_is_reported_and_not_restarted() {
        let h = thread::spawn(|| panic!("generated worker panic"));
        let mut life = Lifecycle::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(Wakeup::default()),
            h,
        );
        assert_eq!(life.stop().unwrap_err().code, "daily_quarantined");
        assert!(!life.running());
    }
}

/// Contention is a local busy state, not unresolved input. Poison is uncertain.
pub(crate) fn retained_quarantine<T>(
    state: &std::sync::Mutex<T>,
    check: impl FnOnce(&T) -> bool,
) -> bool {
    match state.try_lock() {
        Ok(value) => check(&value),
        Err(std::sync::TryLockError::WouldBlock) => false,
        Err(std::sync::TryLockError::Poisoned(_)) => true,
    }
}
#[cfg(test)]
mod contention_tests {
    #[test]
    fn update_contention_does_not_quarantine_or_mutate_worker() {
        let state = std::sync::Mutex::new(false);
        let guard = state.lock().unwrap();
        assert!(!super::retained_quarantine(&state, |blocked| *blocked));
        drop(guard);
        assert!(!super::retained_quarantine(&state, |blocked| *blocked));
        *state.lock().unwrap() = true;
        assert!(super::retained_quarantine(&state, |blocked| *blocked));
    }
}

/// Lifecycle-only classification; never authorize input or renew a lease here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Disposition {
    Continue,
    Stop,
    Quarantine,
}
pub(crate) fn disposition(error: Option<&Error>, possible_input: bool) -> Disposition {
    if possible_input
        || error.is_some_and(|e| {
            matches!(
                e.code,
                "action_uncertain" | "governor_quarantined" | "daily_quarantined"
            )
        })
    {
        Disposition::Quarantine
    } else if error.is_some_and(|e| {
        matches!(
            e.code,
            "ownership_changed" | "session_closed" | "input_lease_required"
        )
    }) {
        Disposition::Stop
    } else {
        Disposition::Continue
    }
}
/// Translate the actual final batch disposition, never a historical report.
pub(crate) fn terminal_event(
    disposition: Disposition,
    error: Option<&Error>,
) -> Option<crate::protocol::worker_events::Terminal> {
    use crate::protocol::worker_events::{Disposition as Kind, Terminal};
    let kind = match disposition {
        Disposition::Continue => return None,
        Disposition::Stop => Kind::Stopped,
        Disposition::Quarantine => Kind::Quarantined,
    };
    Some(
        error
            .map(|e| Terminal::new(kind, e.code, &e.message))
            .unwrap_or_else(|| {
                Terminal::new(
                    kind,
                    "action_uncertain",
                    "Worker terminal disposition; durable state requires review",
                )
            }),
    )
}
/// A failed scheduling wait is a real quarantine cause, not a batch diagnostic.
/// Retain the same bounded original cause in the report before final publication.
pub(crate) fn scheduling_failure(
    error: &Error,
    blocked: &AtomicBool,
    report: &std::sync::Mutex<serde_json::Value>,
) -> crate::protocol::worker_events::Terminal {
    blocked.store(true, Ordering::Release);
    let mut terminal = terminal_event(Disposition::Quarantine, Some(error)).expect("quarantine");
    terminal.phase = Some("scheduling_wait");
    if let Ok(mut report) = report.lock() {
        report["last_error"] = serde_json::json!({"code":terminal.code,"reason":terminal.reason,
            "phase":terminal.phase,"historical":true});
        report["stop_reason"] = serde_json::json!(terminal.code);
        report["running"] = serde_json::json!(false);
    }
    terminal
}
/// Retain quarantine/report state before notifying any read-only waiter.
/// The callback runs after the worker body returns and holds no event-cell lock.
pub(crate) fn publish_terminal(
    events: &crate::protocol::worker_events::Events,
    instance: u64,
    retain: impl FnOnce() -> (u64, crate::protocol::worker_events::Terminal),
) -> Result<()> {
    let (revision, terminal) = retain();
    events.finish(instance, revision, terminal)
}
#[cfg(test)]
mod terminal_tests {
    use super::*;
    #[test]
    fn scheduling_poison_retains_same_original_cause_in_report_and_terminal_event() {
        use crate::protocol::worker_events::{Disposition as Kind, Events};
        let events = Events::default();
        let instance = events.start(1).unwrap();
        let blocked = AtomicBool::new(false);
        let report = std::sync::Mutex::new(
            serde_json::json!({"running":true,"last_error":{"code":"old_historical_refusal"}}),
        );
        // Synthetic result of Wakeup::wait_after's poisoned notification lock.
        let scheduling = Error::new("worker_poisoned", "Notification lock poisoned");
        publish_terminal(&events, instance, || {
            let terminal = scheduling_failure(&scheduling, &blocked, &report);
            assert!(events.snapshot().unwrap().terminal.is_none());
            assert!(blocked.load(Ordering::Acquire));
            let retained = report.lock().unwrap();
            assert_eq!(retained["last_error"]["code"], terminal.code);
            assert_eq!(retained["last_error"]["reason"], terminal.reason);
            assert_eq!(retained["stop_reason"], terminal.code);
            assert_eq!(retained["running"], false);
            (1, terminal)
        })
        .unwrap();
        let terminal = events.snapshot().unwrap().terminal.unwrap();
        assert_eq!(terminal.disposition, Kind::Quarantined);
        assert_eq!(terminal.code, "worker_poisoned");
        assert_eq!(terminal.reason, "Notification lock poisoned");
        let retained = report.lock().unwrap();
        assert_eq!(retained["last_error"]["code"], terminal.code);
        assert_eq!(retained["last_error"]["reason"], terminal.reason);
        assert_eq!(retained["last_error"]["phase"], "scheduling_wait");
    }
    #[test]
    fn worker_events_publish_only_after_quarantine_and_report_are_retained() {
        use crate::protocol::worker_events::{Disposition as Kind, Events, Terminal};
        use std::sync::mpsc;
        let events = Arc::new(Events::default());
        let instance = events.start(1).unwrap();
        let blocked = Arc::new(AtomicBool::new(false));
        let report = Arc::new(std::sync::Mutex::new(false));
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let e = events.clone();
        let b = blocked.clone();
        let r = report.clone();
        let worker = thread::spawn(move || {
            publish_terminal(&e, instance, || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
                b.store(true, Ordering::Release);
                *r.lock().unwrap() = true;
                (
                    1,
                    Terminal::new(
                        Kind::Quarantined,
                        "action_uncertain",
                        "Original unresolved journal",
                    ),
                )
            })
            .unwrap()
        });
        entered_rx.recv().unwrap();
        assert!(events.snapshot().unwrap().terminal.is_none());
        let sequence = events.snapshot().unwrap().sequence;
        release_tx.send(()).unwrap();
        let snapshot = events
            .wait_after(
                sequence,
                std::time::Instant::now() + std::time::Duration::from_secs(2),
            )
            .unwrap();
        assert!(snapshot.terminal.is_some());
        assert!(blocked.load(Ordering::Acquire));
        assert!(*report.lock().unwrap());
        worker.join().unwrap();
    }
    #[test]
    fn worker_events_follow_final_disposition_not_continue_diagnostics() {
        use crate::protocol::worker_events::Disposition as Kind;
        for code in ["input_busy", "torn_read", "research_precondition"] {
            let error = Error::new(code, "Historical or recoverable refusal");
            assert!(terminal_event(disposition(Some(&error), false), Some(&error)).is_none());
            // A real production/panel failure is explicitly promoted by owner.
            let stopped = terminal_event(Disposition::Stop, Some(&error)).unwrap();
            assert_eq!(stopped.disposition, Kind::Stopped);
            assert_eq!(stopped.code, code);
            let quarantine = terminal_event(disposition(Some(&error), true), Some(&error)).unwrap();
            assert_eq!(quarantine.disposition, Kind::Quarantined);
            assert_eq!(quarantine.code, code);
        }
        let error = Error::new("input_lease_required", "Original lease expired");
        assert_eq!(
            terminal_event(disposition(Some(&error), false), Some(&error))
                .unwrap()
                .reason,
            "Original lease expired"
        );
    }
    #[test]
    fn terminal_preinput_loss_stops_without_false_quarantine() {
        for code in [
            "ownership_changed",
            "session_closed",
            "input_lease_required",
        ] {
            assert_eq!(
                disposition(Some(&Error::new(code, "injected before input")), false),
                Disposition::Stop
            );
        }
        for code in ["input_busy", "capability_unavailable", "torn_read"] {
            assert_eq!(
                disposition(Some(&Error::new(code, "known local refusal")), false),
                Disposition::Continue
            );
        }
    }
    #[test]
    fn possible_input_always_dominates_expiration_and_terminal_loss() {
        for code in [
            "ownership_changed",
            "session_closed",
            "input_lease_required",
            "input_busy",
        ] {
            assert_eq!(
                disposition(
                    Some(&Error::new(code, "injected after possible input")),
                    true
                ),
                Disposition::Quarantine
            );
        }
        assert_eq!(disposition(None, true), Disposition::Quarantine);
        assert_eq!(
            disposition(
                Some(&Error::new("action_uncertain", "unknown delivery")),
                false
            ),
            Disposition::Quarantine
        );
    }
    #[test]
    fn idle_expiration_join_has_no_probe_dispatch_or_automatic_restart() {
        use std::sync::atomic::AtomicUsize;
        let stop = Arc::new(AtomicBool::new(false));
        let wake = Arc::new(Wakeup::default());
        let cycles = Arc::new(AtomicUsize::new(0));
        let probes = Arc::new(AtomicUsize::new(0));
        let dispatches = Arc::new(AtomicUsize::new(0));
        let (s, w, c, p, d) = (
            stop.clone(),
            wake.clone(),
            cycles.clone(),
            probes.clone(),
            dispatches.clone(),
        );
        let h = thread::spawn(move || {
            c.fetch_add(1, Ordering::SeqCst);
            let error = Error::new("input_lease_required", "injected idle expiry");
            if disposition(Some(&error), false) == Disposition::Stop {
                s.store(true, Ordering::Release);
                return;
            }
            p.fetch_add(1, Ordering::SeqCst);
            d.fetch_add(1, Ordering::SeqCst);
            let _ = w.wait_after(0, std::time::Duration::from_millis(1));
        });
        let mut life = Lifecycle::new(stop, wake, h);
        life.stop().unwrap();
        assert!(!life.running());
        assert_eq!(cycles.load(Ordering::SeqCst), 1);
        assert_eq!(probes.load(Ordering::SeqCst), 0);
        assert_eq!(dispatches.load(Ordering::SeqCst), 0);
        life.stop().unwrap();
        assert_eq!(cycles.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn real_released_authority_cannot_be_renewed_or_mistaken_for_busy() {
        let epoch = crate::actions::Epoch {
            game: crate::platform::Identity {
                pid: 123,
                start_ticks: "5".into(),
                exe: "/fixture".into(),
                uid: 1000,
            },
            campaign: "fixture".into(),
            player: "GER".into(),
        };
        let token = "a".repeat(32);
        let mut authority =
            crate::input_lease::InputAuthority::new("holder".into(), epoch.clone()).unwrap();
        authority
            .acquire("holder", token.clone(), 30, &epoch)
            .unwrap();
        authority.release(&token, &epoch).unwrap();
        let error = authority.authorize(&token, &epoch).unwrap_err();
        assert_eq!(disposition(Some(&error), false), Disposition::Stop);
        assert!(authority.renew(&token, 30, &epoch).is_err());
        assert!(authority.status().1.is_zero());
        assert_eq!(disposition(Some(&error), true), Disposition::Quarantine);
    }
}
