use super::*;
use crate::{launch::macos::journal::Status, platform::Identity};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Fake {
    selected: bool,
    fail: u8,
    clicks: usize,
    finishes: usize,
}
fn error() -> Error {
    Error::new("torn_read", "fixture")
}
impl Attempt for Fake {
    fn prepare(&mut self) -> Result<bool> {
        if self.fail == 1 {
            Err(error())
        } else {
            Ok(self.selected)
        }
    }
    fn click_once(&mut self) -> Result<()> {
        self.clicks += 1;
        if self.fail == 2 {
            Err(error())
        } else {
            Ok(())
        }
    }
    fn finish(&mut self, clicked: bool) -> Result<Value> {
        self.finishes += 1;
        assert_eq!(clicked, !self.selected);
        if self.fail == 3 {
            Err(error())
        } else {
            Ok(json!({"selected":true,"ordinary_clicks":usize::from(clicked)}))
        }
    }
}
fn fake(selected: bool, fail: u8) -> Fake {
    Fake {
        selected,
        fail,
        clicks: 0,
        finishes: 0,
    }
}
fn directory() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "hoi4-selection-attempt-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    path
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
fn request() -> Request<'static> {
    Request {
        id: "select-once",
        key: "arms_factory",
        state_id: 64,
    }
}

#[test]
fn only_pre_input_or_zero_input_failure_can_be_a_positive_refusal() {
    for selected in [false, true] {
        for fail in 0..=3 {
            let mut a = fake(selected, fail);
            let result = dispatch(&mut a);
            assert_eq!(a.clicks, usize::from(!selected && fail != 1));
            assert_eq!(
                a.finishes,
                usize::from(fail != 1 && (selected || fail != 2))
            );
            match result {
                StrictOutcome::Verified(_) => assert!(fail == 0 || (selected && fail == 2)),
                StrictOutcome::NotQueued(_) => assert!(fail == 1 || (selected && fail == 3)),
                StrictOutcome::Uncertain(_) => assert!(!selected && [2, 3].contains(&fail)),
            }
        }
    }
}

#[test]
fn post_move_and_post_click_errors_remain_quarantined_after_reopen() {
    for failure in [2, 3] {
        let dir = directory();
        let path = dir.join("actions");
        let mut j = GameJournal::open(&path).unwrap();
        let mut a = fake(false, failure);
        assert!(j
            .execute_strict(
                request().id,
                &epoch(),
                request().action(),
                json!({"original":true}),
                &mut || dispatch(&mut a)
            )
            .is_err());
        assert_eq!(a.clicks, 1);
        assert!(j.blocked());
        drop(j);
        let before = std::fs::read(&path).unwrap();
        let mut j = GameJournal::open(&path).unwrap();
        assert!(j.blocked());
        assert!(j
            .execute_strict(
                request().id,
                &epoch(),
                request().action(),
                json!({"later_selected":true}),
                &mut || panic!("uncertain selection must not be replayed")
            )
            .is_err());
        assert_eq!(before, std::fs::read(&path).unwrap());
        drop(j);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn intent_is_durable_before_the_once_only_input_producer_and_replay_adds_nothing() {
    let dir = directory();
    let path = dir.join("actions");
    let mut j = GameJournal::open(&path).unwrap();
    let mut a = fake(false, 0);
    let receipt = j
        .execute_strict(
            request().id,
            &epoch(),
            request().action(),
            json!({"original":true}),
            &mut || {
                let text = std::fs::read_to_string(&path).unwrap();
                let rows: Vec<Value> = text
                    .lines()
                    .map(|l| serde_json::from_str(l).unwrap())
                    .collect();
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0]["status"], "Dispatching");
                dispatch(&mut a)
            },
        )
        .unwrap();
    assert_eq!(receipt.status, Status::Verified);
    assert_eq!(a.clicks, 1);
    drop(j);
    let before = std::fs::read(&path).unwrap();
    let j = GameJournal::open(&path).unwrap();
    assert_eq!(
        j.strict_receipt(request().id, &epoch(), &request().action())
            .unwrap()
            .unwrap(),
        receipt
    );
    let changed = Request {
        key: "industrial_complex",
        ..request()
    };
    assert!(j
        .strict_receipt(request().id, &epoch(), &changed.action())
        .is_err());
    assert_eq!(before, std::fs::read(&path).unwrap());
    drop(j);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn selected_target_and_early_refusals_never_reach_the_mouse_producer() {
    for (selected, fail) in [(true, 0), (true, 3), (false, 1)] {
        let dir = directory();
        let path = dir.join("actions");
        let mut j = GameJournal::open(&path).unwrap();
        let mut a = fake(selected, fail);
        let result = j.execute_strict(
            request().id,
            &epoch(),
            request().action(),
            json!({"original":true}),
            &mut || dispatch(&mut a),
        );
        assert_eq!(a.clicks, 0);
        assert!(!j.blocked());
        assert_eq!(result.is_ok(), fail == 0);
        if let Ok(receipt) = result {
            assert_eq!(receipt.detail["ordinary_clicks"], 0);
        }
        drop(j);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn selection_permission_is_distinct_and_defaults_off() {
    for value in [
        json!({"controller":"fixture","player":"GER"}),
        json!({"controller":"fixture","player":"GER","construction_panel":true}),
    ] {
        let config: super::super::Config = serde_json::from_value(value).unwrap();
        assert!(!config.construction_selection);
    }
    let schema = tool()["inputSchema"].clone();
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(
        schema["required"],
        json!(["building_id", "state_id", "request_id", "lease_token"])
    );
    for value in [json!(null), json!(1), json!("true")] {
        assert!(serde_json::from_value::<super::super::Config>(
            json!({"controller":"fixture","player":"GER","construction_selection":value})
        )
        .is_err());
    }
}
