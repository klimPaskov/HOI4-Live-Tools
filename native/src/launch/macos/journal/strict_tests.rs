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
fn strict_uncertainty_survives_reopen_new_ids_and_legacy_reached_settlement() {
    let d = dir("strict-uncertain");
    let mut j = journal(&d);
    assert_eq!(
        j.execute_strict(
            "lost",
            &epoch(),
            action(),
            json!({"binding":123}),
            &mut || StrictOutcome::Uncertain(Error::new("readback_failed", "Possible key event"))
        )
        .unwrap_err()
        .code,
        "action_uncertain"
    );
    drop(j);
    let mut j = journal(&d);
    let bytes = std::fs::read(d.join("game-actions.jsonl")).unwrap();
    for id in ["lost", "replacement"] {
        assert_eq!(
            j.execute_strict(
                id,
                &epoch(),
                action(),
                json!({"panel_open_now":true}),
                &mut || panic!("Uncertain intent cannot dispatch or settle")
            )
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
            &mut || panic!("Legacy reached-target observation must never run"),
            &mut || panic!("No redispatch")
        )
        .unwrap_err()
        .code,
        "action_uncertain"
    );
    assert_eq!(std::fs::read(d.join("game-actions.jsonl")).unwrap(), bytes);
    assert!(j.blocked());
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
    j.execute(
        "legacy",
        &epoch(),
        json!({"kind":"research_start"}),
        &mut || Ok(Outcome::Unknown),
        &mut || Ok(()),
    )
    .unwrap_err();
    assert_eq!(
        j.execute_strict("new", &epoch(), action(), Value::Null, &mut || panic!(
            "Shared legacy uncertainty"
        ))
        .unwrap_err()
        .code,
        "action_uncertain"
    );
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
