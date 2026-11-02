use super::super::journal::Status;
use super::*;
use crate::platform::Identity;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Copy, Default)]
enum Failure {
    #[default]
    None,
    Prepare,
    Before(usize),
    NotQueued(usize),
    PossibleError(usize),
    Finish,
    LateQueue,
}
struct Fake {
    query: &'static str,
    fail: Failure,
    before: usize,
    attempts: usize,
    posted: Vec<(String, Step)>,
    finished: usize,
}
fn error() -> Error {
    Error::new(
        "torn_read",
        "Same refusal code before or after possible delivery",
    )
}
impl Fake {
    fn new(fail: Failure) -> Self {
        Self {
            query: "64",
            fail,
            before: 0,
            attempts: 0,
            posted: vec![],
            finished: 0,
        }
    }
}
impl Attempt for Fake {
    fn query(&self) -> &str {
        self.query
    }
    fn prepare(&mut self) -> Result<()> {
        if matches!(self.fail, Failure::Prepare) {
            Err(error())
        } else {
            Ok(())
        }
    }
    fn before(&mut self, prefix: &str, step: Step) -> Result<()> {
        let n = self.before;
        self.before += 1;
        assert_eq!(
            (prefix, step),
            match n {
                0 => ("", Step::Digit('6')),
                1 => ("6", Step::Digit('4')),
                _ => ("64", Step::Return),
            }
        );
        if matches!(self.fail,Failure::Before(index) if index==n) {
            Err(error())
        } else {
            Ok(())
        }
    }
    fn post_once(&mut self, prefix: &str, step: Step) -> Dispatch {
        let n = self.attempts;
        self.attempts += 1;
        if matches!(self.fail,Failure::NotQueued(index) if index==n) {
            return Dispatch::NotQueued(error());
        }
        self.posted.push((prefix.to_string(), step));
        if matches!(self.fail,Failure::PossibleError(index) if index==n) {
            Dispatch::Unknown(Some(error()))
        } else {
            Dispatch::Unknown(None)
        }
    }
    fn finish(&mut self) -> Result<Value> {
        self.finished += 1;
        if matches!(self.fail, Failure::LateQueue) {
            let changed = std::cell::Cell::new(false);
            return guarded_read(
                || if changed.get() { Err(error()) } else { Ok(()) },
                || {
                    changed.set(true);
                    Ok(json!({"camera_centered_state_id":64}))
                },
            );
        }
        if matches!(self.fail, Failure::Finish) {
            Err(error())
        } else {
            Ok(json!({"camera_centered_state_id":64}))
        }
    }
}

#[test]
fn successful_typing_posts_each_digit_and_return_once_in_order() {
    let mut f = Fake::new(Failure::None);
    assert!(matches!(dispatch(&mut f), StrictOutcome::Verified(_)));
    assert_eq!(
        f.posted,
        vec![
            ("".into(), Step::Digit('6')),
            ("6".into(), Step::Digit('4')),
            ("64".into(), Step::Return)
        ]
    );
    assert_eq!(f.finished, 1);
}

#[test]
fn positive_refusal_before_first_key_does_not_quarantine() {
    for failure in [Failure::Prepare, Failure::Before(0), Failure::NotQueued(0)] {
        let mut f = Fake::new(failure);
        assert!(matches!(dispatch(&mut f), StrictOutcome::NotQueued(_)));
        assert!(f.posted.is_empty());
        assert_eq!(f.finished, 0);
    }
}

#[test]
fn any_refusal_after_an_earlier_digit_remains_uncertain() {
    for (failure, keys) in [
        (Failure::Before(1), 1),
        (Failure::Before(2), 2),
        (Failure::NotQueued(1), 1),
        (Failure::NotQueued(2), 2),
    ] {
        let mut f = Fake::new(failure);
        assert!(matches!(dispatch(&mut f), StrictOutcome::Uncertain(_)));
        assert_eq!(f.posted.len(), keys);
        assert_eq!(f.finished, 0);
    }
}

#[test]
fn possible_delivery_errors_never_post_the_following_key_or_retry() {
    for index in 0..3 {
        let mut f = Fake::new(Failure::PossibleError(index));
        assert!(matches!(dispatch(&mut f), StrictOutcome::Uncertain(_)));
        assert_eq!(f.posted.len(), index + 1);
        assert_eq!(f.attempts, index + 1);
        assert_eq!(f.finished, 0);
    }
}

#[test]
fn failed_camera_readback_after_return_preserves_uncertainty() {
    let mut f = Fake::new(Failure::Finish);
    assert!(matches!(dispatch(&mut f), StrictOutcome::Uncertain(_)));
    assert_eq!(f.posted.len(), 3);
    assert_eq!(f.finished, 1);
}

#[test]
fn invalid_retained_query_is_refused_without_key_checks_or_input() {
    for query in ["", "4a", "12345", "１２"] {
        let mut f = Fake::new(Failure::None);
        f.query = query;
        assert!(matches!(dispatch(&mut f), StrictOutcome::NotQueued(_)));
        assert_eq!(f.before, 0);
        assert_eq!(f.attempts, 0);
    }
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
fn dir() -> std::path::PathBuf {
    static N: AtomicUsize = AtomicUsize::new(0);
    let p = std::env::temp_dir().join(format!(
        "hoi4-find-once-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&p).unwrap();
    p
}
fn request() -> Request<'static> {
    Request {
        id: "find64",
        key: "arms_factory",
        state_id: 64,
    }
}

#[test]
fn durable_verified_replay_needs_no_new_query_or_input() {
    let p = dir();
    let mut j = GameJournal::open(&p.join("game-actions.jsonl")).unwrap();
    let mut f = Fake::new(Failure::None);
    let receipt = j
        .execute_strict(
            "find64",
            &epoch(),
            request().action(),
            json!({"typed_baseline":"fixture"}),
            &mut || dispatch(&mut f),
        )
        .unwrap();
    assert_eq!(receipt.status, Status::Verified);
    assert_eq!(f.posted.len(), 3);
    drop(j);
    let j = GameJournal::open(&p.join("game-actions.jsonl")).unwrap();
    let replay = j
        .strict_receipt("find64", &epoch(), &request().action())
        .unwrap()
        .unwrap();
    assert_eq!(replay.status, Status::Verified);
    assert_eq!(f.posted.len(), 3);
    let changed = Request {
        id: "find64",
        key: "arms_factory",
        state_id: 65,
    };
    assert!(j
        .strict_receipt("find64", &epoch(), &changed.action())
        .is_err());
    drop(j);
    std::fs::remove_dir_all(p).unwrap();
}

#[test]
fn partial_query_uncertainty_survives_reopen_and_cannot_be_settled_later() {
    let p = dir();
    let mut j = GameJournal::open(&p.join("game-actions.jsonl")).unwrap();
    let mut f = Fake::new(Failure::Before(1));
    assert!(j
        .execute_strict(
            "find64",
            &epoch(),
            request().action(),
            json!({}),
            &mut || dispatch(&mut f)
        )
        .is_err());
    assert_eq!(f.posted.len(), 1);
    assert!(j.blocked());
    drop(j);
    let j = GameJournal::open(&p.join("game-actions.jsonl")).unwrap();
    assert!(j.blocked());
    assert!(j
        .strict_receipt("find64", &epoch(), &request().action())
        .is_err());
    drop(j);
    std::fs::remove_dir_all(p).unwrap();
}

#[test]
fn late_queue_or_selection_change_discards_positive_and_pending_find_reads() {
    use std::cell::Cell;
    for ready in [false, true] {
        let changed = Cell::new(false);
        let checked = Cell::new(0);
        let result = guarded_read(
            || {
                checked.set(checked.get() + 1);
                if changed.get() {
                    Err(error())
                } else {
                    Ok(())
                }
            },
            || {
                changed.set(true);
                Ok(ready.then_some("finished typed Find proof"))
            },
        );
        assert!(result.is_err());
        assert_eq!(checked.get(), 2);
    }
}

#[test]
fn failed_find_read_still_runs_the_post_queue_guard_without_overwriting_cause() {
    use std::cell::Cell;
    let checked = Cell::new(0);
    let result: Result<()> = guarded_read(
        || {
            checked.set(checked.get() + 1);
            if checked.get() == 2 {
                Err(error())
            } else {
                Ok(())
            }
        },
        || Err(Error::new("ownership_changed", "original Find refusal")),
    );
    assert_eq!(result.unwrap_err().code, "ownership_changed");
    assert_eq!(checked.get(), 2);
}

#[test]
fn initial_queue_refusal_never_runs_the_find_read() {
    let result: Result<()> = guarded_read(
        || Err(error()),
        || panic!("No Find payload after original queue refusal"),
    );
    assert!(result.is_err());
}

#[test]
fn late_queue_change_during_final_find_never_produces_a_verified_receipt() {
    let p = dir();
    let mut journal = GameJournal::open(&p.join("game-actions.jsonl")).unwrap();
    let mut attempt = Fake::new(Failure::LateQueue);
    assert!(journal
        .execute_strict(
            "find64",
            &epoch(),
            request().action(),
            json!({}),
            &mut || dispatch(&mut attempt)
        )
        .is_err());
    assert_eq!(attempt.posted.len(), 3);
    assert_eq!(attempt.finished, 1);
    assert!(journal.blocked());
    assert!(journal
        .strict_receipt("find64", &epoch(), &request().action())
        .is_err());
    let rows: Vec<Value> = std::fs::read_to_string(p.join("game-actions.jsonl"))
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["status"], "Dispatching");
    drop(journal);
    let journal = GameJournal::open(&p.join("game-actions.jsonl")).unwrap();
    assert!(journal.blocked());
    assert!(journal
        .strict_receipt("find64", &epoch(), &request().action())
        .is_err());
    drop(journal);
    std::fs::remove_dir_all(p).unwrap();
}
