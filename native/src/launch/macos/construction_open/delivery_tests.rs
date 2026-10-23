//! Exercise the actual key producer, action classifier and durable journal
//! together. No native input API or live process is used.
use super::*;
use crate::{
    launch::macos::journal::{Outcome, Status},
    platform::{key_once, Identity},
};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fail {
    None,
    InitialCheck,
    Allocate,
    FinalCheck,
    Down,
    Up,
    Readback,
}
struct Pair {
    fail: Fail,
    trace: Rc<RefCell<Vec<&'static str>>>,
}
fn error() -> Error {
    Error::new("torn_read", "same code on both sides of possible delivery")
}
impl key_once::Pair for Pair {
    fn down(&mut self) -> Result<()> {
        self.trace.borrow_mut().push("down");
        if self.fail == Fail::Down {
            Err(error())
        } else {
            Ok(())
        }
    }
    fn up(&mut self) -> Result<()> {
        self.trace.borrow_mut().push("up");
        if self.fail == Fail::Up {
            Err(error())
        } else {
            Ok(())
        }
    }
}
struct Producer {
    fail: Fail,
    trace: Rc<RefCell<Vec<&'static str>>>,
}
impl Attempt for Producer {
    fn prepare(&mut self) -> Result<bool> {
        Ok(false)
    }
    fn press_once(&mut self) -> KeyDispatch {
        let checks = Cell::new(0);
        key_once::dispatch(
            || {
                checks.set(checks.get() + 1);
                if (checks.get() == 1 && self.fail == Fail::InitialCheck)
                    || (checks.get() == 2 && self.fail == Fail::FinalCheck)
                {
                    Err(error())
                } else {
                    Ok(())
                }
            },
            || {
                if self.fail == Fail::Allocate {
                    Err(error())
                } else {
                    Ok(Pair {
                        fail: self.fail,
                        trace: self.trace.clone(),
                    })
                }
            },
            |_| {},
        )
    }
    fn finish(&mut self, sent: bool) -> Result<Value> {
        assert!(sent);
        self.trace.borrow_mut().push("readback");
        if self.fail == Fail::Readback {
            Err(error())
        } else {
            Ok(json!({"panel_open":true}))
        }
    }
}
fn directory() -> PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let d = std::env::temp_dir().join(format!(
        "hoi4-strict-key-delivery-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&d).unwrap();
    d
}
fn epoch() -> Epoch {
    Epoch {
        game: Identity {
            pid: 7,
            start_ticks: "1.000001".into(),
            exe: "/fixture".into(),
            uid: 501,
        },
        campaign: "nonce:playthrough".into(),
        player: "GER".into(),
    }
}
fn action() -> Value {
    json!({"kind":"construction_panel_open","transport":"ordinary_t"})
}

#[test]
fn zero_input_mode_refusal_is_durable_when_the_prepared_panel_is_closed() {
    let d = directory();
    let path = d.join("actions");
    let mut j = GameJournal::open(&path).unwrap();
    let trace = Rc::new(RefCell::new(Vec::new()));
    let mut p = Producer {
        fail: Fail::None,
        trace: trace.clone(),
    };
    assert_eq!(
        j.execute_strict(
            "open-only",
            &epoch(),
            Mode::AlreadyOpenOnly.action(),
            json!({"initial_panel_open":true}),
            &mut || dispatch(&mut p, Mode::AlreadyOpenOnly)
        )
        .unwrap_err()
        .code,
        "construction_panel_not_open"
    );
    assert!(trace.borrow().is_empty());
    assert!(!j.blocked());
    drop(j);
    let prefix = std::fs::read(&path).unwrap();
    let j = GameJournal::open(&path).unwrap();
    assert_eq!(
        j.strict_receipt("open-only", &epoch(), &Mode::AlreadyOpenOnly.action())
            .unwrap_err()
            .code,
        "input_not_queued"
    );
    assert_eq!(
        j.strict_receipt("open-only", &epoch(), &Mode::Ordinary.action())
            .unwrap_err()
            .code,
        "idempotency_conflict"
    );
    let rows: Vec<Value> = String::from_utf8(prefix.clone())
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1]["status"], "NotApplied");
    assert_eq!(
        rows[1]["detail"],
        json!({"delivery":"NotQueued","cause_code":"construction_panel_not_open"})
    );
    assert_eq!(std::fs::read(&path).unwrap(), prefix);
    drop(j);
    std::fs::remove_dir_all(d).unwrap();
}

#[test]
fn zero_input_receipt_survives_reopen_without_becoming_an_ordinary_opener() {
    struct Open;
    impl Attempt for Open {
        fn prepare(&mut self) -> Result<bool> {
            Ok(true)
        }
        fn press_once(&mut self) -> KeyDispatch {
            panic!("No key producer allowed")
        }
        fn finish(&mut self, sent: bool) -> Result<Value> {
            assert!(!sent);
            Ok(json!({"panel_open":true,"ordinary_key_presses":0,"same_attempt_readback":true}))
        }
    }
    let d = directory();
    let path = d.join("actions");
    let mut j = GameJournal::open(&path).unwrap();
    let receipt = j
        .execute_strict(
            "open-only",
            &epoch(),
            Mode::AlreadyOpenOnly.action(),
            json!({"initial_panel_open":true}),
            &mut || dispatch(&mut Open, Mode::AlreadyOpenOnly),
        )
        .unwrap();
    drop(j);
    let bytes = std::fs::read(&path).unwrap();
    let mut j = GameJournal::open(&path).unwrap();
    assert_eq!(
        j.execute_strict(
            "open-only",
            &epoch(),
            Mode::AlreadyOpenOnly.action(),
            Value::Null,
            &mut || panic!("Historical receipt cannot run a new attempt")
        )
        .unwrap(),
        receipt
    );
    assert_eq!(
        j.strict_receipt("open-only", &epoch(), &Mode::Ordinary.action())
            .unwrap_err()
            .code,
        "idempotency_conflict"
    );
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    drop(j);
    std::fs::remove_dir_all(d).unwrap();
}
fn attempt(
    j: &mut GameJournal,
    file: &std::path::Path,
    id: &str,
    producer: &mut Producer,
) -> Result<Value> {
    let receipt = j.execute_strict(
        id,
        &epoch(),
        action(),
        json!({"original_binding":"retained"}),
        &mut || {
            let rows = std::fs::read_to_string(file).unwrap();
            let last: super::super::journal::Receipt =
                serde_json::from_str(rows.lines().last().unwrap()).unwrap();
            assert_eq!(last.status, Status::Dispatching);
            assert_eq!(last.id, id);
            dispatch(producer, Mode::Ordinary)
        },
    )?;
    Ok(serde_json::to_value(receipt).unwrap())
}

#[test]
fn producer_positive_no_queue_survives_reopen_and_preserves_refusal_on_fresh_attempt() {
    for fail in [Fail::InitialCheck, Fail::Allocate, Fail::FinalCheck] {
        let d = directory();
        let path = d.join("actions");
        let mut j = GameJournal::open(&path).unwrap();
        let trace = Rc::new(RefCell::new(Vec::new()));
        let mut p = Producer {
            fail,
            trace: trace.clone(),
        };
        assert_eq!(
            attempt(&mut j, &path, "refused", &mut p).unwrap_err().code,
            "torn_read"
        );
        assert!(trace.borrow().is_empty(), "{fail:?} posted or ran readback");
        assert!(!j.blocked());
        drop(j);
        let prefix = std::fs::read(&path).unwrap();
        let mut j = GameJournal::open(&path).unwrap();
        assert_eq!(
            j.strict_receipt("refused", &epoch(), &action())
                .unwrap_err()
                .code,
            "input_not_queued"
        );
        let records: Vec<Value> = String::from_utf8(prefix.clone())
            .unwrap()
            .lines()
            .map(|s| serde_json::from_str(s).unwrap())
            .collect();
        assert_eq!(records.len(), 2);
        assert_eq!(records[1]["status"], "NotApplied");
        assert_eq!(
            records[1]["detail"],
            json!({"delivery":"NotQueued","cause_code":"torn_read"})
        );
        p.fail = Fail::None;
        let receipt = attempt(&mut j, &path, "fresh", &mut p).unwrap();
        assert_eq!(receipt["status"], "Verified");
        assert!(!j.blocked());
        assert_eq!(*trace.borrow(), ["down", "up", "readback"]);
        assert!(std::fs::read(&path).unwrap().starts_with(&prefix));
        drop(j);
        std::fs::remove_dir_all(d).unwrap();
    }
}

#[test]
fn identical_error_after_possible_delivery_remains_quarantined_despite_new_producer_proof() {
    for fail in [Fail::Down, Fail::Up, Fail::Readback] {
        let d = directory();
        let path = d.join("actions");
        let mut j = GameJournal::open(&path).unwrap();
        let trace = Rc::new(RefCell::new(Vec::new()));
        let mut p = Producer {
            fail,
            trace: trace.clone(),
        };
        assert_eq!(
            attempt(&mut j, &path, "lost", &mut p).unwrap_err().code,
            "action_uncertain"
        );
        assert!(j.blocked());
        assert_eq!(&trace.borrow()[..2], ["down", "up"]);
        assert_eq!(
            trace.borrow().len(),
            if fail == Fail::Readback { 3 } else { 2 }
        );
        drop(j);
        let bytes = std::fs::read(&path).unwrap();
        let mut j = GameJournal::open(&path).unwrap();
        for id in ["lost", "replacement"] {
            assert_eq!(
                j.execute_strict(id, &epoch(), action(), Value::Null, &mut || {
                    panic!("A new producer classification cannot settle an old uncertain intent")
                })
                .unwrap_err()
                .code,
                "action_uncertain"
            );
        }
        assert_eq!(
            j.execute(
                "lost",
                &epoch(),
                action(),
                &mut || Ok(Outcome::Reached(json!({"panel_open":true}))),
                &mut || panic!("no replay")
            )
            .unwrap_err()
            .code,
            "action_uncertain"
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert!(j.blocked());
        drop(j);
        std::fs::remove_dir_all(d).unwrap();
    }
}
