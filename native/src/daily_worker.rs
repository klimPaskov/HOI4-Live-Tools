//! Join/stop ownership only; no game input, reader or strategy.
use crate::{
    daily_governor::{WakeReason, Wakeup},
    error::{Error, Result},
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
};
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
