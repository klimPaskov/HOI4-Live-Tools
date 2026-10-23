//! One preallocated ordinary key pair. Delivery knowledge comes from the
//! producer's position relative to down, never from an error code or UI state.
use crate::error::{Error, Result};
use std::time::Duration;

#[derive(Debug)]
pub(crate) enum Dispatch {
    /// No down operation was entered. The producer positively knows no input
    /// was queued; the original refusal may be retained without quarantine.
    NotQueued(Error),
    /// Down may have been queued. None permits same-attempt bounded readback;
    /// Some(error) stops it. Neither variant is delivery acknowledgement.
    Unknown(Option<Error>),
}

/// Both native events must be allocated and configured before this is returned.
/// Creating/dropping an unused pair must never post input. Dropping releases its
/// native resources; the sequence separately owns key-up after entering down.
pub(crate) trait Pair {
    fn down(&mut self) -> Result<()>;
    fn up(&mut self) -> Result<()>;
}

struct Release<P: Pair> {
    pair: P,
    needed: bool,
}
impl<P: Pair> Release<P> {
    fn up(&mut self) -> Result<()> {
        let result = self.pair.up();
        self.needed = false;
        result
    }
}
impl<P: Pair> Drop for Release<P> {
    fn drop(&mut self) {
        if self.needed {
            // Cleanup after a possible down is unconditional, including unwind;
            // no lease/deadline/foreground check can strand a held key.
            let _ = self.up();
        }
    }
}

pub(crate) fn dispatch<P: Pair>(
    mut check: impl FnMut() -> Result<()>,
    allocate: impl FnOnce() -> Result<P>,
    mut wait: impl FnMut(Duration),
) -> Dispatch {
    if let Err(e) = check() {
        return Dispatch::NotQueued(e);
    }
    let pair = match allocate() {
        Ok(pair) => pair,
        Err(e) => return Dispatch::NotQueued(e),
    };
    let mut release = Release {
        pair,
        needed: false,
    };
    if let Err(e) = check() {
        return Dispatch::NotQueued(e);
    }
    // Set this before entering the producer: its error or unwind cannot prove
    // no delivery. Never retry down, and do not infer non-delivery from readback.
    release.needed = true;
    let down = release.pair.down();
    if down.is_ok() {
        wait(Duration::from_millis(40));
    }
    let up = release.up();
    if let Err(error) = down.and(up) {
        return Dispatch::Unknown(Some(error));
    }
    wait(Duration::from_millis(40));
    Dispatch::Unknown(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };

    #[derive(Default)]
    struct Trace {
        calls: RefCell<Vec<&'static str>>,
        checks: Cell<usize>,
        waits: RefCell<Vec<Duration>>,
    }
    struct Fake {
        trace: Rc<Trace>,
        down_error: bool,
        up_error: bool,
        panic_down: bool,
    }
    fn error() -> Error {
        Error::new("torn_read", "generated producer failure")
    }
    impl Pair for Fake {
        fn down(&mut self) -> Result<()> {
            self.trace.calls.borrow_mut().push("down");
            assert!(!self.panic_down, "generated down unwind");
            if self.down_error {
                Err(error())
            } else {
                Ok(())
            }
        }
        fn up(&mut self) -> Result<()> {
            self.trace.calls.borrow_mut().push("up");
            if self.up_error {
                Err(error())
            } else {
                Ok(())
            }
        }
    }
    impl Drop for Fake {
        fn drop(&mut self) {
            self.trace.calls.borrow_mut().push("free_pair");
        }
    }
    fn pair(trace: &Rc<Trace>) -> Fake {
        Fake {
            trace: trace.clone(),
            down_error: false,
            up_error: false,
            panic_down: false,
        }
    }
    #[test]
    fn initial_refusal_never_allocates_or_posts_even_with_a_transient_error_code() {
        let out = dispatch::<Fake>(
            || Err(error()),
            || panic!("allocation after refusal"),
            |_| panic!("wait after refusal"),
        );
        assert!(matches!(out,Dispatch::NotQueued(e) if e.code=="torn_read"));
    }
    #[test]
    fn allocation_failure_never_reaches_second_check_or_posts() {
        let checks = Cell::new(0);
        let out = dispatch::<Fake>(
            || {
                checks.set(checks.get() + 1);
                Ok(())
            },
            || Err(error()),
            |_| panic!("wait after allocation failure"),
        );
        assert!(matches!(out, Dispatch::NotQueued(_)));
        assert_eq!(checks.get(), 1);
    }
    #[test]
    fn final_pre_down_refusal_frees_unused_pair_without_sending_up() {
        let trace = Rc::new(Trace::default());
        let out = dispatch(
            || {
                trace.checks.set(trace.checks.get() + 1);
                if trace.checks.get() == 2 {
                    Err(error())
                } else {
                    Ok(())
                }
            },
            || {
                trace.calls.borrow_mut().push("allocate");
                Ok(pair(&trace))
            },
            |_| panic!("no wait"),
        );
        assert!(matches!(out, Dispatch::NotQueued(_)));
        assert_eq!(*trace.calls.borrow(), ["allocate", "free_pair"]);
    }
    #[test]
    fn possible_down_failure_stays_unknown_and_releases_once_without_retry() {
        let trace = Rc::new(Trace::default());
        let out = dispatch(
            || Ok(()),
            || {
                let mut p = pair(&trace);
                p.down_error = true;
                Ok(p)
            },
            |_| panic!("failed down does not wait"),
        );
        assert!(matches!(out,Dispatch::Unknown(Some(e)) if e.code=="torn_read"));
        assert_eq!(*trace.calls.borrow(), ["down", "up", "free_pair"]);
    }
    #[test]
    fn failed_release_is_unknown_and_never_retries_the_key() {
        let trace = Rc::new(Trace::default());
        let out = dispatch(
            || Ok(()),
            || {
                let mut p = pair(&trace);
                p.up_error = true;
                Ok(p)
            },
            |d| trace.waits.borrow_mut().push(d),
        );
        assert!(matches!(out, Dispatch::Unknown(Some(_))));
        assert_eq!(*trace.calls.borrow(), ["down", "up", "free_pair"]);
        assert_eq!(*trace.waits.borrow(), [Duration::from_millis(40)]);
    }
    #[test]
    fn release_failure_does_not_replace_the_first_possible_delivery_failure() {
        struct Both;
        impl Pair for Both {
            fn down(&mut self) -> Result<()> {
                Err(Error::new("down_failed", "first"))
            }
            fn up(&mut self) -> Result<()> {
                Err(Error::new("up_failed", "second"))
            }
        }
        assert!(
            matches!(dispatch(||Ok(()),||Ok(Both),|_|panic!("no waits")),
            Dispatch::Unknown(Some(e)) if e.code=="down_failed")
        );
    }
    #[test]
    fn complete_posting_is_still_unacknowledged_and_release_ignores_expired_checks() {
        let trace = Rc::new(Trace::default());
        let expired = Cell::new(false);
        let out = dispatch(
            || {
                assert!(!expired.get(), "must release without new checks");
                trace.checks.set(trace.checks.get() + 1);
                Ok(())
            },
            || Ok(pair(&trace)),
            |d| {
                expired.set(true);
                trace.waits.borrow_mut().push(d);
            },
        );
        assert!(matches!(out, Dispatch::Unknown(None)));
        assert_eq!(trace.checks.get(), 2);
        assert_eq!(*trace.calls.borrow(), ["down", "up", "free_pair"]);
        assert_eq!(*trace.waits.borrow(), [Duration::from_millis(40); 2]);
    }
    #[test]
    fn unwind_in_down_or_hold_releases_before_freeing_the_pair() {
        for panic_down in [false, true] {
            let trace = Rc::new(Trace::default());
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                dispatch(
                    || Ok(()),
                    || {
                        let mut p = pair(&trace);
                        p.panic_down = panic_down;
                        Ok(p)
                    },
                    |_| panic!("generated hold unwind"),
                )
            }));
            assert!(result.is_err());
            assert_eq!(*trace.calls.borrow(), ["down", "up", "free_pair"]);
        }
    }
    #[test]
    fn unwind_after_release_never_sends_an_extra_up() {
        let trace = Rc::new(Trace::default());
        let waits = Cell::new(0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dispatch(
                || Ok(()),
                || Ok(pair(&trace)),
                |_| {
                    waits.set(waits.get() + 1);
                    assert_ne!(waits.get(), 2, "generated post-up unwind");
                },
            )
        }));
        assert!(result.is_err());
        assert_eq!(*trace.calls.borrow(), ["down", "up", "free_pair"]);
    }
}
