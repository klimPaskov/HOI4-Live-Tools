use super::*;
use std::{
    sync::{mpsc, Arc},
    thread,
    time::Duration,
};
fn stopped() -> Terminal {
    Terminal::new(
        Disposition::Stopped,
        "input_lease_required",
        "Actual expired input lease",
    )
}
#[test]
fn bounded_current_instance_retains_first_terminal_and_replaces_only_on_start() {
    let events = Events::default();
    let first = events.start(4).unwrap();
    events.finish(first, 5, stopped()).unwrap();
    let snapshot = events.snapshot().unwrap();
    assert_eq!(snapshot.plan_revision, Some(5));
    events
        .finish(
            first,
            6,
            Terminal::new(Disposition::RequestedStop, "stop", "later STOP"),
        )
        .unwrap();
    assert_eq!(events.snapshot().unwrap().sequence, snapshot.sequence);
    assert_eq!(events.snapshot().unwrap().terminal, snapshot.terminal);
    let second = events.start(6).unwrap();
    assert!(second > first);
    assert!(events.snapshot().unwrap().terminal.is_none());
    assert_eq!(
        events.finish(first, 5, stopped()).unwrap_err().code,
        "daily_worker_replaced"
    );
}
#[test]
fn terminal_and_absorbing_retirement_wake_multiple_waiters_without_lost_edges() {
    for retire in [false, true] {
        let events = Arc::new(Events::default());
        let instance = events.start(1).unwrap();
        let sequence = events.snapshot().unwrap().sequence;
        let (ready_tx, ready_rx) = mpsc::channel();
        let handles = (0..3)
            .map(|_| {
                let e = events.clone();
                let tx = ready_tx.clone();
                thread::spawn(move || {
                    tx.send(()).unwrap();
                    e.wait_after(sequence, Instant::now() + Duration::from_secs(5))
                        .unwrap()
                })
            })
            .collect::<Vec<_>>();
        for _ in 0..3 {
            ready_rx.recv().unwrap();
        }
        let start = Instant::now();
        if retire {
            events.retire().unwrap();
        } else {
            events.finish(instance, 1, stopped()).unwrap();
        }
        for h in handles {
            let s = h.join().unwrap();
            assert!(s.retired || s.terminal.is_some());
        }
        assert!(start.elapsed() < Duration::from_secs(1));
        if retire {
            assert_eq!(events.start(2).unwrap_err().code, "session_closed");
            events.retire().unwrap();
            assert!(
                events
                    .wait_after(
                        events.snapshot().unwrap().sequence,
                        Instant::now() + Duration::from_secs(5)
                    )
                    .unwrap()
                    .retired
            );
        }
    }
}
#[test]
fn counter_exhaustion_and_poison_fail_explicitly_without_wrapping_or_old_success() {
    for instance_full in [false, true] {
        let events = Events::default();
        {
            let mut s = events.current.lock().unwrap();
            if instance_full {
                s.instance = Some(u64::MAX);
            } else {
                s.sequence = u64::MAX;
            }
        }
        assert_eq!(
            events.start(1).unwrap_err().code,
            "worker_events_unavailable"
        );
        assert_eq!(
            events.snapshot().unwrap_err().code,
            "worker_events_unavailable"
        );
        assert_eq!(
            events
                .wait_after(u64::MAX, Instant::now())
                .unwrap_err()
                .code,
            "worker_events_unavailable"
        );
    }
    let events = Events::default();
    let instance = events.start(1).unwrap();
    events.current.lock().unwrap().sequence = u64::MAX;
    assert_eq!(
        events.finish(instance, 1, stopped()).unwrap_err().code,
        "worker_events_unavailable"
    );
    assert_eq!(
        events.snapshot().unwrap_err().code,
        "worker_events_unavailable"
    );
    events.retire().unwrap();
    assert!(events.snapshot().unwrap().retired);
    let events = Arc::new(Events::default());
    let e = events.clone();
    let _ = thread::spawn(move || {
        let _guard = e.current.lock().unwrap();
        panic!("fixture poison");
    })
    .join();
    assert_eq!(
        events.snapshot().unwrap_err().code,
        "worker_events_unavailable"
    );
}
