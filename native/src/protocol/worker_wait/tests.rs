use super::*;
use crate::protocol::worker_events::Terminal;
use std::{cell::Cell, sync::mpsc, thread};
fn run(
    events: Option<Arc<Events>>,
    timeout: Duration,
    target: Option<i32>,
    assignments: Option<assignment_wait::Target>,
    producer: impl FnMut() -> Result<(Value, game::ClockContext)>,
) -> Result<Value> {
    run_from(
        Instant::now(),
        events,
        timeout,
        target,
        assignments,
        producer,
    )
}
fn context() -> game::ClockContext {
    game::ClockContext {
        playthrough_id: "owned".into(),
        player: "GER".into(),
        anchors: [1, 2, 3],
        date_raw: 100,
        difficulty: 1,
        historical_raw: 1,
        paused: false,
        speed: 5,
    }
}
fn sample() -> (Value, game::ClockContext) {
    (
        json!({"owned":true,"date":{"raw":100},"focus":{"key":"old"},"research":[]}),
        context(),
    )
}
fn terminal(kind: Disposition) -> Terminal {
    Terminal::new(
        kind,
        "research_precondition",
        "Original bounded own root refusal",
    )
}
fn target() -> assignment_wait::Target {
    assignment_wait::Target::parse(&json!({"focus_id":"wanted"})).unwrap()
}
#[test]
fn already_terminal_is_level_triggered_before_any_game_read_for_both_target_families() {
    for kind in [
        Disposition::Stopped,
        Disposition::Quarantined,
        Disposition::RequestedStop,
    ] {
        for assignments in [false, true] {
            let events = Arc::new(Events::default());
            let instance = events.start(9).unwrap();
            events.finish(instance, 9, terminal(kind)).unwrap();
            let out = run(
                Some(events),
                Duration::from_secs(50),
                Some(200),
                assignments.then(target),
                || panic!("terminal wait must not read game"),
            )
            .unwrap();
            assert_eq!(out["state"], Value::Null);
            assert_eq!(out["state_role"], "none");
            assert_eq!(
                out["worker_event"]["original_error"]["code"],
                "research_precondition"
            );
            assert_eq!(out["worker_event"]["quiescent"], true);
            assert_eq!(
                out["worker_event"]["actionable_error"],
                kind != Disposition::RequestedStop
            );
            assert_eq!(out["completion_proven"], false);
            assert_eq!(out["input_authorized"], false);
        }
    }
}
#[test]
fn actual_terminal_notification_wakes_fifty_second_wait_with_only_historical_sample() {
    for assignments in [false, true] {
        let events = Arc::new(Events::default());
        let instance = events.start(2).unwrap();
        let e = events.clone();
        let (tx, rx) = mpsc::channel();
        let h = thread::spawn(move || {
            let mut reads = 0;
            let out = run(
                Some(e),
                Duration::from_secs(50),
                (!assignments).then_some(200),
                assignments.then(target),
                || {
                    reads += 1;
                    if reads == 1 {
                        tx.send(()).unwrap();
                    }
                    Ok(sample())
                },
            )
            .unwrap();
            (out, reads)
        });
        rx.recv().unwrap();
        thread::sleep(Duration::from_millis(10));
        let start = Instant::now();
        events
            .finish(instance, 2, terminal(Disposition::Quarantined))
            .unwrap();
        let (out, reads) = h.join().unwrap();
        assert!(start.elapsed() < Duration::from_millis(200));
        assert_eq!(reads, 1);
        assert_eq!(out["reasons"], json!(["daily_worker_quarantined"]));
        assert_eq!(out["state"]["focus"]["key"], "old");
        assert_eq!(out["state_historical"], true);
        assert_eq!(out["current_state_verified"], false);
        assert_eq!(
            out["state_role"],
            "last_snapshot_accepted_before_worker_interruption"
        );
    }
}
#[test]
fn terminal_during_torn_read_interrupts_without_another_read_or_full_budget_sleep() {
    let events = Arc::new(Events::default());
    let instance = events.start(1).unwrap();
    let mut calls = 0;
    let out = run(
        Some(events.clone()),
        Duration::from_secs(50),
        Some(200),
        None,
        || {
            calls += 1;
            events
                .finish(instance, 1, terminal(Disposition::Stopped))
                .unwrap();
            Err(Error::new("torn_read", "Discard snapshot"))
        },
    )
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(out["rejected_torn_reads"], 1);
    assert_eq!(out["state"], Value::Null);
}
#[test]
fn same_sample_terminal_precedes_target_but_real_identity_error_remains_strict() {
    for fail in [false, true] {
        let events = Arc::new(Events::default());
        let instance = events.start(1).unwrap();
        let result = run(
            Some(events.clone()),
            Duration::from_secs(50),
            Some(100),
            None,
            || {
                events
                    .finish(instance, 1, terminal(Disposition::Stopped))
                    .unwrap();
                if fail {
                    Err(Error::new(
                        "ownership_changed",
                        "Actual original process changed",
                    ))
                } else {
                    Ok(sample())
                }
            },
        );
        if fail {
            assert_eq!(result.unwrap_err().code, "ownership_changed");
        } else {
            assert_eq!(result.unwrap()["reasons"], json!(["daily_worker_stopped"]));
        }
    }
}
#[test]
fn replacement_never_follows_new_worker_and_wait_without_worker_does_not_attach() {
    let events = Arc::new(Events::default());
    let first = events.start(1).unwrap();
    let mut watch = Watch::new(Some(events.clone())).unwrap();
    events
        .finish(first, 1, terminal(Disposition::Stopped))
        .unwrap();
    events.start(2).unwrap();
    assert_eq!(
        watch.check().unwrap().unwrap()["reason"],
        "daily_worker_replaced"
    );
    let events = Arc::new(Events::default());
    let mut watch = Watch::new(Some(events.clone())).unwrap();
    let instance = events.start(1).unwrap();
    events
        .finish(instance, 1, terminal(Disposition::Quarantined))
        .unwrap();
    let replacement = events.start(2).unwrap();
    events
        .finish(replacement, 2, terminal(Disposition::Stopped))
        .unwrap();
    assert!(watch.check().unwrap().is_none());
    events.retire().unwrap();
    assert_eq!(watch.check().unwrap_err().code, "session_closed");
}
#[test]
fn first_assignment_sample_matches_and_changed_followup_context_never_hides_behind_event() {
    let events = Arc::new(Events::default());
    let instance = events.start(1).unwrap();
    let mut calls = 0;
    let out = run(
        Some(events.clone()),
        Duration::from_secs(2),
        None,
        Some(target()),
        || {
            calls += 1;
            let mut s = sample();
            s.0["focus"]["key"] = json!("wanted");
            Ok(s)
        },
    )
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(out["reasons"], json!(["assignments_reached"]));
    let mut calls = 0;
    let result = run(
        Some(events.clone()),
        Duration::from_secs(2),
        Some(200),
        None,
        || {
            calls += 1;
            let mut s = sample();
            if calls == 2 {
                events
                    .finish(instance, 1, terminal(Disposition::Stopped))
                    .unwrap();
                s.1.anchors[0] = 99;
            }
            Ok(s)
        },
    );
    assert_eq!(calls, 2);
    assert_eq!(result.unwrap_err().code, "wait_context_changed");
}
#[test]
fn late_event_or_snapshot_never_becomes_timely_target_and_retirement_prevents_next_read() {
    let events = Arc::new(Events::default());
    let instance = events.start(1).unwrap();
    let mut watch = Watch::new(Some(events.clone())).unwrap();
    let mut rejected = 0;
    let out = read(
        &mut watch,
        Instant::now() + Duration::from_millis(5),
        &mut rejected,
        || {
            thread::sleep(Duration::from_millis(10));
            events
                .finish(instance, 1, terminal(Disposition::Stopped))
                .unwrap();
            Ok(sample())
        },
    )
    .unwrap();
    assert!(matches!(out, Read::Deadline(WaitDeadline::LateCompletion)));
    let events = Arc::new(Events::default());
    events.start(1).unwrap();
    events.retire().unwrap();
    let out = run(
        Some(events),
        Duration::from_secs(50),
        Some(100),
        None,
        || panic!("retired owner cannot read"),
    );
    assert_eq!(out.unwrap_err().code, "session_closed");
}
#[test]
fn timeout_keeps_original_deadline_and_only_historical_terminal_cause_and_earlier_state() {
    let events = Arc::new(Events::default());
    let instance = events.start(1).unwrap();
    let start = Instant::now();
    let mut watch = Watch::new(Some(events.clone())).unwrap();
    let mut rejected = 0;
    let mut calls = 0;
    let earlier = match read(
        &mut watch,
        start + Duration::from_secs(50),
        &mut rejected,
        || {
            calls += 1;
            Ok(sample())
        },
    )
    .unwrap()
    {
        Read::Fresh((state, _)) => state,
        _ => panic!("Earlier own snapshot must be accepted before terminal publication"),
    };
    let received = Instant::now();
    let (publish_tx, publish_rx) = mpsc::channel();
    let publisher = thread::spawn(move || {
        publish_rx.recv().unwrap();
        events
            .finish(instance, 1, terminal(Disposition::Quarantined))
            .unwrap();
    });
    assert!(watch.check().unwrap().is_none());
    publish_tx.send(()).unwrap();
    publisher.join().unwrap();

    // The separate delayed-read test covers LateCompletion classification.
    // Compose that original refusal directly: a second 250ms poll must not
    // depend on the CI scheduler fitting inside a 30ms remainder.
    let out = timed_out(
        &mut watch,
        earlier.clone(),
        start,
        received,
        rejected,
        Some(WaitDeadline::LateCompletion),
    )
    .unwrap();
    assert_eq!(calls, 1);
    assert_eq!(out["reasons"], json!(["wall_timeout"]));
    assert_eq!(out["state"], earlier);
    assert_eq!(out["state_historical"], true);
    assert_eq!(out["current_state_verified"], false);
    assert_eq!(
        out["deadline_refusal"]["reason"],
        "snapshot_completed_after_deadline"
    );
    assert_eq!(out["deadline_refusal"]["late_target_accepted"], false);
    assert_eq!(out["input_authorized"], false);
    assert_eq!(
        out["worker_event_historical"]["original_error"]["code"],
        "research_precondition"
    );
    assert_eq!(out["worker_event_timely"], false);
}
#[test]
fn unsupported_and_running_workers_preserve_first_sample_success_and_recovered_tears() {
    for supported in [false, true] {
        let events = Arc::new(Events::default());
        events.start(1).unwrap();
        let calls = Cell::new(0);
        let out = run(
            supported.then_some(events),
            Duration::from_secs(2),
            Some(100),
            None,
            || {
                calls.set(calls.get() + 1);
                if calls.get() == 1 {
                    Err(Error::new("torn_read", "Recovered read diagnostic"))
                } else {
                    Ok(sample())
                }
            },
        )
        .unwrap();
        assert_eq!(out["reasons"], json!(["date_reached"]));
        assert_eq!(out["rejected_torn_reads"], 1);
        assert_eq!(calls.get(), 2);
        assert!(out.get("worker_event").is_none());
    }
}
#[test]
fn protocol_route_returns_structured_terminal_without_decorative_process_reads() {
    struct Adapter(Arc<Events>);
    impl Commands for Adapter {
        fn tools(&self) -> Vec<Value> {
            vec![]
        }
        fn supports(&self, _: &str) -> bool {
            false
        }
        fn call(&self, _: &dyn crate::platform::Process, _: &str, _: &Value) -> Result<Value> {
            panic!("no input")
        }
        fn capabilities(&self) -> Value {
            json!({})
        }
        fn wait_events(&self) -> Option<Arc<Events>> {
            Some(self.0.clone())
        }
    }
    let events = Arc::new(Events::default());
    let instance = events.start(1).unwrap();
    events
        .finish(instance, 1, terminal(Disposition::Quarantined))
        .unwrap();
    let server = crate::protocol::Server {
        process: None,
        unavailable: None,
    };
    let adapter = Adapter(events);
    for args in [
        json!({"until_date":[1936,1,1],"timeout_seconds":50}),
        json!({"until_assignments":{"focus_id":"wanted"},"timeout_seconds":50}),
    ] {
        let out = server.call("hoi4_wait_for", &args, Some(&adapter)).unwrap();
        assert_eq!(out["reasons"], json!(["daily_worker_quarantined"]));
        assert_eq!(out["state"], Value::Null);
    }
}
