//! Digest-only contention wait; no read, input, cache, lease or renewal here.
use crate::error::{Error, Result};
use std::{
    sync::{Mutex, MutexGuard, TryLockError},
    time::{Duration, Instant},
};
/// The same gate used by the worker must be obtained before touching producer.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn governed<T, R>(
    governor: &Mutex<T>,
    body: impl FnOnce(&mut T) -> Result<R>,
) -> Result<R> {
    let mut guard = governor.try_lock().map_err(|e| match e {
        TryLockError::WouldBlock => Error::new("input_busy", "Daily worker owns governor"),
        TryLockError::Poisoned(_) => {
            Error::new("action_uncertain", "Daily governor mutex poisoned")
        }
    })?;
    body(&mut guard)
}
pub(crate) fn acquire<T>(mutex: &Mutex<T>, until: Option<Instant>) -> Result<MutexGuard<'_, T>> {
    acquire_with(mutex, until, Instant::now, std::thread::park_timeout)
}
fn acquire_with<T>(
    mutex: &Mutex<T>,
    until: Option<Instant>,
    mut now: impl FnMut() -> Instant,
    mut wait: impl FnMut(Duration),
) -> Result<MutexGuard<'_, T>> {
    loop {
        if until.is_some_and(|d| now() >= d) {
            return Err(Error::new(
                "daily_deadline",
                "Digest producer wait exhausted original deadline; no read",
            ));
        }
        match mutex.try_lock() {
            Ok(guard) => {
                if until.is_some_and(|d| now() >= d) {
                    return Err(Error::new(
                        "daily_deadline",
                        "Digest producer acquired after original deadline; no read",
                    ));
                }
                return Ok(guard);
            }
            Err(TryLockError::Poisoned(_)) => {
                return Err(Error::new(
                    "daily_quarantined",
                    "Daily producer mutex poisoned",
                ))
            }
            Err(TryLockError::WouldBlock) => {
                let Some(deadline) = until else {
                    return Err(Error::new("input_busy", "Daily producer currently reading"));
                };
                wait(
                    deadline
                        .saturating_duration_since(now())
                        .min(Duration::from_millis(2)),
                );
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ordinary_try_is_busy_and_poison_is_not_busy() {
        let m = Mutex::new(0);
        let guard = m.lock().unwrap();
        assert_eq!(acquire(&m, None).unwrap_err().code, "input_busy");
        drop(guard);
        let _ = std::panic::catch_unwind(|| {
            let _g = m.lock().unwrap();
            panic!("injected producer poison")
        });
        assert_eq!(acquire(&m, None).unwrap_err().code, "daily_quarantined");
    }
    #[test]
    fn wait_exhaustion_does_not_start_read_or_retry_model() {
        let m = Mutex::new(0);
        let _held = m.lock().unwrap();
        let start = Instant::now();
        let deadline = start + Duration::from_secs(2);
        let clock = std::cell::Cell::new(start);
        let waits = std::cell::Cell::new(0);
        let result = acquire_with(
            &m,
            Some(deadline),
            || clock.get(),
            |duration| {
                assert!(duration <= Duration::from_millis(2));
                waits.set(waits.get() + 1);
                clock.set(deadline);
            },
        );
        assert_eq!(result.unwrap_err().code, "daily_deadline");
        assert_eq!(waits.get(), 1);
    }
    #[test]
    fn actual_contention_release_acquires_current_value_once() {
        let m = std::sync::Arc::new(Mutex::new(1));
        let mut held = m.lock().unwrap();
        let (busy_tx, busy_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let copy = m.clone();
        let waiter = std::thread::spawn(move || {
            let stamp = Instant::now();
            let mut first = true;
            let value = acquire_with(
                &copy,
                Some(stamp + Duration::from_secs(2)),
                || stamp,
                |_| {
                    if first {
                        first = false;
                        busy_tx.send(()).unwrap();
                        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    }
                },
            )
            .unwrap();
            *value
        });
        busy_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        *held = 42;
        drop(held);
        release_tx.send(()).unwrap();
        assert_eq!(waiter.join().unwrap(), 42);
    }
    #[test]
    fn deadline_after_lock_acquisition_drops_guard_without_read() {
        let m = Mutex::new(9);
        let start = Instant::now();
        let deadline = start + Duration::from_secs(2);
        let calls = std::cell::Cell::new(0);
        let result = acquire_with(
            &m,
            Some(deadline),
            || {
                let n = calls.get();
                calls.set(n + 1);
                if n == 0 {
                    start
                } else {
                    deadline
                }
            },
            |_| panic!("uncontended mutex must not wait"),
        );
        assert_eq!(result.unwrap_err().code, "daily_deadline");
        assert_eq!(*m.try_lock().unwrap(), 9);
    }
    #[test]
    fn digest_cannot_make_inflight_postdispatch_witness_producer_busy() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        };
        let governor = Arc::new(Mutex::new(()));
        let counter = Arc::new(Mutex::new(7));
        let reads = Arc::new(AtomicUsize::new(0));
        let (inflight_tx, inflight_rx) = std::sync::mpsc::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let (g, c) = (governor.clone(), counter.clone());
        let worker = std::thread::spawn(move || {
            let _owned_stage = g.lock().unwrap();
            inflight_tx.send(()).unwrap();
            finish_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let mut witness =
                acquire(&c, None).expect("postdispatch witness must not be made busy by digest");
            *witness += 1;
        });
        inflight_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let result = governed(&governor, |_| {
            reads.fetch_add(1, Ordering::SeqCst);
            let value = acquire(&counter, Some(Instant::now() + Duration::from_secs(2)))?;
            Ok(*value)
        });
        assert_eq!(result.unwrap_err().code, "input_busy");
        assert_eq!(reads.load(Ordering::SeqCst), 0);
        finish_tx.send(()).unwrap();
        worker.join().unwrap();
        let fresh = governed(&governor, |_| {
            reads.fetch_add(1, Ordering::SeqCst);
            let value = acquire(&counter, Some(Instant::now() + Duration::from_secs(2)))?;
            Ok(*value)
        })
        .unwrap();
        assert_eq!(fresh, 8);
        assert_eq!(reads.load(Ordering::SeqCst), 1);
    }
}
