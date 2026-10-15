//! Bounded live summary with exact, read-only historical detail retrieval.
//! This module has no process, input, lease or worker access.
use crate::error::{Error, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

pub(crate) const SECTIONS: &[&str] = &[
    "assignments",
    "focus",
    "core_pending_historical",
    "latest_preparation_historical",
    "latest_focus_wait_historical",
    "last_error",
    "last_preparation_refusal",
    "last_stage_failure",
    "retained",
    "latest_verified_ids_historical",
];

#[derive(Default)]
pub(crate) struct Details {
    saved: Option<(String, Value)>,
}
impl Details {
    pub(crate) fn retain(&mut self, snapshot: &Value) -> Result<String> {
        let bytes =
            serde_json::to_vec(snapshot).map_err(|e| Error::new("digest_error", e.to_string()))?;
        if bytes.len() > 1_048_576 {
            return Err(Error::new(
                "digest_error",
                "Historical detail bound exceeded",
            ));
        }
        let id = format!("{:x}", Sha256::digest(&bytes));
        self.saved = Some((id.clone(), snapshot.clone()));
        Ok(id)
    }
    pub(crate) fn get(&self, id: &str, section: &str) -> Result<Value> {
        if !SECTIONS.contains(&section) {
            return Err(Error::new(
                "invalid_arguments",
                "Unknown daily detail section",
            ));
        }
        let (saved_id, snapshot) = self
            .saved
            .as_ref()
            .filter(|(saved_id, _)| saved_id == id)
            .ok_or_else(|| {
                Error::new(
                    "digest_snapshot_expired",
                    "Obtain a new digest; the exact historical snapshot is unavailable",
                )
            })?;
        Ok(
            json!({"snapshot_id":saved_id,"section":section,"value":snapshot[section],
            "historical":true,"current_state_verified":false,"input_authorized":false}),
        )
    }
}

/// Overflow changes representation, never the worker state or original history.
/// Codes, uncertainty and budget stay inline; full causes/IDs use the exact
/// snapshot lookup. No successful completion is inferred from preparation.
pub(crate) fn overflow(full: &Value, snapshot_id: &str) -> Result<Value> {
    let mut out = json!({});
    for key in [
        "date_raw",
        "sequence",
        "observed_unix_ms",
        "age_ms",
        "source",
        "country",
        "plan_revision",
        "counts",
        "daily_budget",
        "running",
        "stop_reason",
        "quarantined",
        "focus_permission",
        "review_due",
        "blocking_decision",
        "retained_count",
    ] {
        if let Some(value) = full.get(key) {
            out[key] = value.clone();
        }
    }
    out["input_authorized"] = json!(false);
    out["running_is_input_authority"] = json!(false);
    out["verified"] = json!([]);
    out["history"] = json!({"compacted":true,"historical":true,"current_state_verified":false,
        "snapshot_id":snapshot_id,"tool":"hoi4_daily_governor","op":"details"});
    let assignments = &full["assignments"];
    let mut summary = json!({"historical":true,"current_state_verified":false,"input_authorized":false,
        "details_section":"assignments"});
    for key in [
        "available",
        "reason",
        "plan_revision",
        "total",
        "completed",
        "pending",
        "in_flight",
        "not_queued_attempts",
    ] {
        if let Some(value) = assignments.get(key) {
            summary[key] = value.clone();
        }
    }
    out["assignments"] = summary;
    // Every distinct error location stays discoverable, even if it is not the
    // latest error. Reasons/IDs are retained verbatim in the matching section.
    let mut failures = serde_json::Map::new();
    for (name, value) in [
        ("last_error", &full["last_error"]),
        (
            "last_preparation_refusal",
            &full["last_preparation_refusal"],
        ),
        ("last_stage_failure", &full["last_stage_failure"]),
        ("focus.failure", &full["focus"]["failure"]),
        ("focus.refusal", &full["focus"]["refusal"]),
        ("focus.uncertainty", &full["focus"]["uncertainty"]),
        ("focus.start_failure", &full["focus"]["start_failure"]),
    ] {
        if value.is_object() {
            failures.insert(name.into(), value["code"].clone());
        }
    }
    out["historical_failure_codes"] = Value::Object(failures);
    if full["focus"]["start_failure"].is_object() {
        let f = &full["focus"]["start_failure"];
        out["start_failure"] = json!({"original_code":f["original_code"],"evidence":f["evidence"],
            "failures":f["failures"],"retry_authority":false,"historical":true,"details_section":"focus"});
    }
    out["history"]["sections"] = json!(SECTIONS
        .iter()
        .filter(|key| full.get(**key).is_some_and(|v| !v.is_null()))
        .copied()
        .collect::<Vec<_>>());
    // The fixed schema is bounded for production field limits. Refuse malformed
    // caller-generated oversized essentials rather than silently falsifying them.
    if serde_json::to_vec(&out)
        .map_err(|e| Error::new("digest_error", e.to_string()))?
        .len()
        > 2048
    {
        return Err(Error::new(
            "digest_error",
            "Essential summary exceeds bound",
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn crowded() -> Value {
        let failure = json!({"entry_id":"x".repeat(128),"request_id":"a".repeat(64),
            "code":"input_not_queued","reason":"é".repeat(80),"original_code":"torn_read",
            "original_reason":"different original cause","evidence":"ConfirmedNotQueued",
            "failures":3,"retry_authority":false,"historical":true});
        json!({"date_raw":60759371,"sequence":u64::MAX,"observed_unix_ms":1791978764068u64,
            "age_ms":u64::MAX,"source":"native process_vm_readv","country":"GER","plan_revision":u64::MAX,
            "counts":{"idle_research_slots":4,"idle_civilian_factories":null,"idle_military_factories":null,"scoped_equipment_deficit":null},
            "daily_budget":{"day":706640,"used_assignment_attempts":1,"limit":1,"remaining":0,"navigation_spends_assignment_budget":false},
            "running":true,"stop_reason":null,"quarantined":true,"focus_permission":true,"review_due":false,"blocking_decision":null,"retained_count":256,
            "assignments":{"available":true,"plan_revision":u64::MAX,"total":256,"completed":255,"pending":1,"in_flight":0,
                "not_queued_attempts":2048,"latest_not_queued":failure,
                "latest_completion":{"entry_id":"q".repeat(128),"request_id":"b".repeat(64),"state":"Complete","observed_sequence":5}},
            "last_error":failure,"last_preparation_refusal":failure,"last_stage_failure":failure,
            "focus":{"failure":failure,"refusal":failure,"uncertainty":failure,"start_failure":failure,
                "preparation":{"verified":true,"goal_completed":false,"request_id":"c".repeat(64)}},
            "latest_preparation_historical":{"request_id":"d".repeat(64)},
            "latest_focus_wait_historical":{"active_focus_id":"GER_other"},
            "core_pending_historical":{"entry_id":"x".repeat(128),"attempts":1,"reason":"input_not_queued"},
            "retained":(0..256).map(|i|json!({"id":format!("{i}-{}", "x".repeat(120)),"reason":"é".repeat(80)})).collect::<Vec<_>>(),
            "latest_verified_ids_historical":["old-success"]})
    }
    #[test]
    fn simultaneous_long_histories_fit_and_roundtrip_every_cause_without_input_authority() {
        let mut details = Details::default();
        let full = crowded();
        let id = details.retain(&full).unwrap();
        let compact = overflow(&full, &id).unwrap();
        assert!(serde_json::to_vec(&compact).unwrap().len() <= 2048);
        assert_eq!(compact["assignments"]["completed"], 255);
        assert_eq!(compact["assignments"]["pending"], 1);
        assert_eq!(compact["assignments"]["not_queued_attempts"], 2048);
        assert_eq!(compact["daily_budget"], full["daily_budget"]);
        assert_eq!(compact["quarantined"], true);
        assert_eq!(compact["start_failure"]["evidence"], "ConfirmedNotQueued");
        assert_eq!(compact["start_failure"]["retry_authority"], false);
        assert_eq!(
            compact["historical_failure_codes"]
                .as_object()
                .unwrap()
                .len(),
            7
        );
        assert_eq!(compact["verified"], json!([]));
        for section in SECTIONS {
            let saved = details.get(&id, section).unwrap();
            assert_eq!(saved["value"], full[section]);
            assert_eq!(saved["historical"], true);
            assert_eq!(saved["current_state_verified"], false);
            assert_eq!(saved["input_authorized"], false);
        }
    }
    #[test]
    fn replacement_never_returns_a_different_snapshot_and_unknown_sections_refuse() {
        let mut details = Details::default();
        let mut full = crowded();
        let first = details.retain(&full).unwrap();
        assert_eq!(details.retain(&full).unwrap(), first);
        full["focus"]["start_failure"]["reason"] = json!("new failure");
        let second = details.retain(&full).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            details.get(&first, "focus").unwrap_err().code,
            "digest_snapshot_expired"
        );
        assert_eq!(
            details.get(&second, "../private").unwrap_err().code,
            "invalid_arguments"
        );
        assert_eq!(
            details.get(&second, "focus").unwrap()["value"]["start_failure"]["reason"],
            "new failure"
        );
        assert_eq!(
            Details::default().get(&second, "focus").unwrap_err().code,
            "digest_snapshot_expired"
        );
    }
    #[test]
    fn rejected_oversized_history_preserves_prior_exact_snapshot() {
        let mut details = Details::default();
        let id = details
            .retain(&json!({"focus":{"failure":"original"}}))
            .unwrap();
        assert_eq!(
            details
                .retain(&json!({"focus":"x".repeat(1_048_576)}))
                .unwrap_err()
                .code,
            "digest_error"
        );
        assert_eq!(
            details.get(&id, "focus").unwrap()["value"]["failure"],
            "original"
        );
    }
}
