//! History for rejected scheduling reads, never input or completion authority.
use super::Disposition;
use crate::error::Error;
use serde_json::{json, Value};

#[derive(Default)]
pub(crate) struct ClockProbeHistory {
    rejected: u64,
    consecutive: u64,
    first: Option<Value>,
    latest: Option<Value>,
    last_success: Option<i32>,
}
impl ClockProbeHistory {
    pub(crate) fn verified(&mut self, date: i32) {
        self.last_success = Some(date);
        self.consecutive = 0;
    }
    /// Only a torn clock probe before scheduling may use this separate channel.
    /// A quarantine, terminal loss or any action-phase error remains last_error.
    /// The caller still ends this batch and uses the normal worker wake interval.
    pub(crate) fn rejected(
        &mut self,
        error: &Error,
        at_clock_probe: bool,
        disposition: Disposition,
    ) -> bool {
        if !at_clock_probe || error.code != "torn_read" || disposition != Disposition::Continue {
            return false;
        }
        self.rejected = self.rejected.saturating_add(1);
        self.consecutive = self.consecutive.saturating_add(1);
        let failure = json!({"code":error.code,
            "reason":crate::daily_navigation::bounded_reason(&error.message),
            "phase":"clock_probe","historical":true,"input_attempted":false});
        self.first.get_or_insert_with(|| failure.clone());
        self.latest = Some(failure);
        true
    }
    pub(crate) fn summary(&self) -> Value {
        json!({"rejected_reads":self.rejected,"consecutive_rejections":self.consecutive,
            "first_failure":self.first,"last_failure":self.latest,
            "last_success_date_raw":self.last_success,
            "recovered":self.rejected>0 && self.consecutive==0 && self.last_success.is_some(),
            "scope":"worker_lifetime","historical":true,
            "current_state_verified":false,"input_authorized":false})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn idle_recovery_retains_original_cause_and_never_becomes_fresh_evidence() {
        let mut h = ClockProbeHistory::default();
        h.verified(100);
        assert!(h.rejected(
            &Error::new("torn_read", "first witness"),
            true,
            Disposition::Continue
        ));
        assert!(h.rejected(
            &Error::new("torn_read", "later witness"),
            true,
            Disposition::Continue
        ));
        assert_eq!(h.summary()["consecutive_rejections"], 2);
        assert_eq!(h.summary()["recovered"], false);
        h.verified(101); // Idle success, no completed assignment is required.
        let s = h.summary();
        assert_eq!(s["rejected_reads"], 2);
        assert_eq!(s["consecutive_rejections"], 0);
        assert_eq!(s["recovered"], true);
        assert_eq!(s["first_failure"]["reason"], "first witness");
        assert_eq!(s["last_failure"]["reason"], "later witness");
        assert_eq!(s["last_success_date_raw"], 101);
        assert_eq!(s["current_state_verified"], false);
        assert_eq!(s["input_authorized"], false);
        assert!(h.rejected(
            &Error::new("torn_read", "new witness"),
            true,
            Disposition::Continue
        ));
        assert_eq!(h.summary()["recovered"], false);
        assert_eq!(h.summary()["consecutive_rejections"], 1);
    }
    #[test]
    fn action_errors_and_uncertainty_cannot_enter_probe_history() {
        let mut h = ClockProbeHistory::default();
        for (code, at_probe, possible_input) in [
            ("torn_read", false, false),
            ("torn_read", true, true),
            ("ownership_changed", true, false),
            ("input_lease_required", true, false),
            ("action_uncertain", true, false),
            ("io_error", true, false),
            ("input_busy", true, false),
        ] {
            let e = Error::new(code, "original failure");
            assert!(!h.rejected(
                &e,
                at_probe,
                super::super::disposition(Some(&e), possible_input)
            ));
        }
        assert_eq!(h.summary()["rejected_reads"], 0);
        assert!(h.summary()["first_failure"].is_null());
        assert_eq!(h.summary()["recovered"], false);
    }
}
