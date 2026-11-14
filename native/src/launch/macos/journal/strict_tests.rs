use super::{
    tests::{dir, epoch, journal},
    *,
};
use serde_json::json;

fn action() -> Value {
    json!({"kind":"construction_panel_open","transport":"ordinary_t"})
}

#[test]
fn original_binding_is_durable_before_dispatch_and_verified_replay_is_historical() {
    let d = dir("strict-verified");
    let mut j = journal(&d);
    let before = json!({"binding":[11,22,33],"managed_mode":2});
    let result = j
        .execute_strict("open", &epoch(), action(), before.clone(), &mut || {
            let rows = std::fs::read_to_string(d.join("game-actions.jsonl")).unwrap();
            let intent: Receipt = serde_json::from_str(rows.trim()).unwrap();
            assert_eq!(intent.version, 2);
            assert_eq!(intent.status, Status::Dispatching);
            assert_eq!(intent.detail, before);
            StrictOutcome::Verified(json!({"managed_mode":17}))
        })
        .unwrap();
    drop(j);
    let mut j = journal(&d);
    assert_eq!(
        j.execute_strict(
            "open",
            &epoch(),
            action(),
            json!({"different_current_ui":true}),
            &mut || panic!("Historical receipt cannot dispatch or inspect current UI")
        )
        .unwrap(),
        result
    );
    drop(j);
    std::fs::remove_dir_all(d).unwrap();
}

#[test]
fn strict_uncertainty_is_closed_at_once_and_never_settled_or_replayed() {
    let d = dir("strict-uncertain");
    let mut j = journal(&d);
    let error = j
        .execute_strict(
            "lost",
            &epoch(),
            action(),
            json!({"binding":123}),
            &mut || StrictOutcome::Uncertain(Error::new("readback_failed", "Possible key event")),
        )
        .unwrap_err();
    assert_eq!(error.code, "action_unconfirmed");
    assert!(error.message.contains("cause=readback_failed"));
    assert_eq!(j.receipts["lost"].status, Status::Unconfirmed);
    assert!(!j.blocked());
    drop(j);
    let mut j = journal(&d);
    let bytes = std::fs::read(d.join("game-actions.jsonl")).unwrap();
    // The closed id answers from its record: no dispatch, no settlement from the current UI.
    assert_eq!(
        j.execute_strict(
            "lost",
            &epoch(),
            action(),
            json!({"panel_open_now":true}),
            &mut || panic!("A closed strict intent cannot dispatch or settle")
        )
        .unwrap_err()
        .code,
        "action_unconfirmed"
    );
    assert_eq!(
        j.execute(
            "lost",
            &epoch(),
            action(),
            &mut || panic!("Legacy reached-target observation must never run"),
            &mut || panic!("No redispatch")
        )
        .unwrap_err()
        .code,
        "action_uncertain"
    );
    assert_eq!(std::fs::read(d.join("game-actions.jsonl")).unwrap(), bytes);
    // A fresh strict request is not blocked by it.
    let fresh = j
        .execute_strict(
            "replacement",
            &epoch(),
            action(),
            json!({"binding":124}),
            &mut || StrictOutcome::Verified(json!({"done":true})),
        )
        .unwrap();
    assert_eq!(fresh.status, Status::Verified);
    drop(j);
    std::fs::remove_dir_all(d).unwrap();
}

#[test]
fn positive_not_queued_refusal_is_retained_and_allows_only_a_fresh_request() {
    let d = dir("strict-not-queued");
    let mut j = journal(&d);
    assert_eq!(
        j.execute_strict("refused", &epoch(), action(), Value::Null, &mut || {
            StrictOutcome::NotQueued(Error::new(
                "key_target_changed",
                "lease secret not persisted",
            ))
        })
        .unwrap_err()
        .code,
        "key_target_changed"
    );
    assert!(!j.blocked());
    drop(j);
    let mut j = journal(&d);
    assert_eq!(
        j.execute_strict("refused", &epoch(), action(), Value::Null, &mut || panic!(
            "No automatic retry"
        ))
        .unwrap_err()
        .code,
        "input_not_queued"
    );
    j.execute_strict("fresh", &epoch(), action(), Value::Null, &mut || {
        StrictOutcome::Verified(json!({"opened":true}))
    })
    .unwrap();
    let rows = std::fs::read_to_string(d.join("game-actions.jsonl")).unwrap();
    assert!(rows.contains("key_target_changed"));
    assert!(!rows.contains("lease secret"));
    assert_eq!(rows.lines().count(), 4);
    drop(j);
    std::fs::remove_dir_all(d).unwrap();
}

#[test]
fn strict_binding_conflicts_and_legacy_uncertainty_never_call_adapter() {
    let d = dir("strict-conflict");
    let mut j = journal(&d);
    j.execute_strict("one", &epoch(), action(), Value::Null, &mut || {
        StrictOutcome::Verified(Value::Null)
    })
    .unwrap();
    let mut other = epoch();
    other.game.start_ticks = "2".into();
    for (e, a) in [(other, action()), (epoch(), json!({"kind":"other"}))] {
        assert_eq!(
            j.execute_strict("one", &e, a, Value::Null, &mut || panic!("Conflict"))
                .unwrap_err()
                .code,
            "idempotency_conflict"
        );
    }
    // A legacy intent left open by a stopped server may still have input in flight: a new
    // strict action waits for it (bounded), then the orphan is closed and the action runs.
    super::tests::orphan(
        &mut j,
        "legacy",
        &json!({"kind":"event_choose"}),
        Value::Null,
    );
    assert_eq!(
        j.execute_strict("new", &epoch(), action(), Value::Null, &mut || panic!(
            "An interrupted action may still have input in flight"
        ))
        .unwrap_err()
        .code,
        "action_uncertain"
    );
    j.receipts.get_mut("legacy").unwrap().unix_ms -= ORPHAN_MS;
    let ran = j
        .execute_strict("new", &epoch(), action(), Value::Null, &mut || {
            StrictOutcome::Verified(Value::Null)
        })
        .unwrap();
    assert_eq!(ran.status, Status::Verified);
    assert_eq!(j.receipts["legacy"].status, Status::Unconfirmed);
    drop(j);
    std::fs::remove_dir_all(d).unwrap();
}

#[test]
fn strict_intent_and_result_persistence_failures_keep_quarantine() {
    for fail_intent in [true, false] {
        let d = dir(if fail_intent {
            "strict-intent-io"
        } else {
            "strict-result-io"
        });
        let mut j = journal(&d);
        if fail_intent {
            j.file = File::open(d.join("game-actions.jsonl")).unwrap();
        }
        let result = j.execute_strict("lost", &epoch(), action(), Value::Null, &mut || {
            assert!(!fail_intent, "Failed intent cannot dispatch");
            // Oversized final result refuses after the original intent is durable.
            StrictOutcome::Verified(json!({"result":"x".repeat(LINE_LIMIT)}))
        });
        assert!(result.is_err());
        assert!(j.blocked());
        assert_eq!(
            j.strict_receipt("fresh", &epoch(), &action())
                .unwrap_err()
                .code,
            "action_uncertain"
        );
        drop(j);
        std::fs::remove_dir_all(d).unwrap();
    }
}

#[test]
fn journal_refuses_cross_version_outcomes_and_unknown_receipt_semantics() {
    for (first_version, last_version, last_status) in [
        (2, 1, Status::Verified),
        (3, 3, Status::Verified),
        (2, 2, Status::Diverged),
    ] {
        let d = dir(&format!(
            "strict-invalid-{first_version}-{last_version}-{last_status:?}"
        ));
        let mut first = Receipt {
            version: first_version,
            id: "x".into(),
            epoch: epoch(),
            action: action(),
            status: Status::Dispatching,
            detail: Value::Null,
            unix_ms: 1,
        };
        let mut bytes = serde_json::to_string(&first).unwrap();
        bytes.push('\n');
        first.version = last_version;
        first.status = last_status;
        bytes.push_str(&serde_json::to_string(&first).unwrap());
        bytes.push('\n');
        std::fs::write(d.join("game-actions.jsonl"), bytes).unwrap();
        assert!(
            matches!(GameJournal::open(&d.join("game-actions.jsonl")),Err(e) if e.code=="invalid_journal")
        );
        std::fs::remove_dir_all(d).unwrap();
    }
}

#[cfg(test)]
mod retry_settle_tests {
    use super::super::tests::{dir, epoch, journal};
    use super::super::*;
    use serde_json::json;

    /// An intent left open by a server that stopped in the middle of the action.
    fn open_intent(j: &mut GameJournal, id: &str, kind: &str) {
        super::super::tests::orphan(j, id, &json!({"kind":kind}), Value::Null);
        assert!(j.unresolved().is_some());
    }
    #[test]
    fn an_interrupted_army_order_blocks_army_input_but_not_research() {
        let d = dir("domain-scope");
        let mut j = journal(&d);
        open_intent(&mut j, "a1", "army_move");
        assert!(j.blocked_for("army"));
        assert!(!j.blocked_for("research"));
        assert!(j.blocked_for("*"));
        // a new army order waits, a research start goes through
        let mut ok_settle = || Ok(Outcome::Reached(json!({})));
        let mut ok_dispatch = || Ok(());
        assert!(j
            .execute(
                "a2",
                &epoch(),
                json!({"kind":"army_front"}),
                &mut ok_settle,
                &mut ok_dispatch
            )
            .is_err());
        let mut ok_settle = || Ok(Outcome::Reached(json!({})));
        let mut ok_dispatch = || Ok(());
        assert!(j
            .execute(
                "r1",
                &epoch(),
                json!({"kind":"research_start"}),
                &mut ok_settle,
                &mut ok_dispatch
            )
            .is_ok());
    }
    #[test]
    fn an_interrupted_event_answer_blocks_everything_briefly() {
        let d = dir("domain-global");
        let mut j = journal(&d);
        open_intent(&mut j, "e1", "event_choose");
        assert!(j.blocked_for("research"));
        assert!(j.blocked_for("army"));
    }
    #[test]
    fn a_completed_call_never_leaves_an_open_intent_in_any_domain() {
        let d = dir("domain-closed");
        let mut j = journal(&d);
        for (id, kind) in [
            ("a1", "army_move"),
            ("e1", "event_choose"),
            ("f1", "focus_start"),
        ] {
            let mut settle = || Ok(Outcome::Unknown);
            let mut dispatch = || Err(Error::new("action_deadline", "late"));
            let error = j
                .execute(
                    id,
                    &epoch(),
                    json!({"kind":kind}),
                    &mut settle,
                    &mut dispatch,
                )
                .unwrap_err();
            assert_eq!(error.code, "action_unconfirmed");
            assert!(j.unresolved().is_none());
            for domain in ["army", "focus", "research", "*"] {
                assert!(!j.blocked_for(domain));
            }
        }
    }
    #[test]
    fn baseline_free_unchanged_settles_a_retry_and_unblocks_input() {
        let d = dir("retry-unchanged");
        let mut j = journal(&d);
        open_intent(&mut j, "a1", "k");
        let mut settle = || Ok(Outcome::Unchanged(json!({})));
        let mut dispatch = || panic!("a retry never re-sends input");
        let r = j
            .execute_with(
                "a1",
                &epoch(),
                json!({"kind":"k"}),
                &mut settle,
                &mut dispatch,
                true,
            )
            .unwrap();
        assert_eq!(r.status, Status::NotApplied);
        assert!(j.unresolved().is_none());
    }
    #[test]
    fn default_retry_still_refuses_unchanged_and_closes_unconfirmed() {
        let d = dir("retry-default");
        let mut j = journal(&d);
        open_intent(&mut j, "a1", "k");
        let mut settle = || Ok(Outcome::Unchanged(json!({})));
        let mut dispatch = || panic!("no input");
        let error = j
            .execute(
                "a1",
                &epoch(),
                json!({"kind":"k"}),
                &mut settle,
                &mut dispatch,
            )
            .unwrap_err();
        assert_eq!(error.code, "action_unconfirmed");
        assert_eq!(j.receipts["a1"].status, Status::Unconfirmed);
        assert!(j.unresolved().is_none());
    }
    #[test]
    fn torn_reads_in_settlement_are_read_again_until_the_outcome_shows() {
        let d = dir("torn");
        let mut j = journal(&d);
        let mut calls = 0;
        let mut settle = || {
            calls += 1;
            if calls < 3 {
                Err(Error::new("torn_read", "Snapshot witness changed"))
            } else {
                Ok(Outcome::Reached(json!({})))
            }
        };
        let mut dispatch = || Err(Error::new("action_deadline", "late"));
        let r = j.execute(
            "t1",
            &epoch(),
            json!({"kind":"k"}),
            &mut settle,
            &mut dispatch,
        );
        assert!(r.is_ok());
        assert_eq!(calls, 3);
    }
}

#[test]
fn compound_step_request_ids_are_valid() {
    // hoi4_army_attack journals its steps as <id>:front, :offensive, :execute
    // (a '/' separator was refused by every live call, 8 Nov 2026).
    for flag in ["front", "offensive", "execute"] {
        assert!(valid_id(&format!("player-17:{flag}")).is_ok());
    }
    assert!(valid_id("player-17/front").is_err());
}
