//! One generic runner for ordinary-UI actions whose success is read back from
//! the game: journaled intent, the fixed 12 s fence deadline, typed refusals
//! before any input, a single dispatch, and a settle rule that compares only
//! what the action can change. It exists so the doctrine, intelligence and
//! industrial-organisation tools share one control flow instead of repeating
//! the lease/fence/journal prologue of `macos.rs` per tool.
//!
//! Evidence the dispatch observes on screen before it closes its windows is
//! handed to the settle rule (`observed`); a dispatch that failed after
//! possible input leaves it empty and the rule may fall back to memory or
//! report `Diverged` ("unverified"), which is recorded but never freezes the
//! shared input journal.
use super::{
    action_fence::Fence,
    journal::{GameJournal, Outcome},
    Guard, InputAuthority,
};
use crate::{
    error::{Error, Result},
    platform::{macos_input::MacKeys, Process},
};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    time::{Duration, Instant},
};

/// How a dispatch ended. `Refused` is a typed refusal found while driving that
/// provably happened before the first state-changing click (the journal records
/// it as not applied); `Failed` is any error that may follow input.
pub(super) enum Fail {
    Refused(Error),
    Failed(Error),
}
impl From<Error> for Fail {
    fn from(e: Error) -> Self {
        Fail::Failed(e)
    }
}
pub(super) fn refuse(code: &'static str, message: impl Into<String>) -> Fail {
    Fail::Refused(Error::new(code, message))
}
pub(super) type Driven = std::result::Result<Value, Fail>;
type PlanFn<'j> = Box<dyn Fn(&dyn Process) -> Result<()> + 'j>;
type DispatchFn<'j> = Box<dyn Fn(&dyn Process, &MacKeys<'_>) -> Driven + 'j>;
type SettleFn<'j> = Box<dyn Fn(&dyn Process, Option<&Value>) -> Result<Outcome> + 'j>;

pub(super) struct Job<'j> {
    /// Short tool name for the result (`doctrine_adopt`, ...).
    pub kind: &'static str,
    /// The journaled intent: stable for retries of the same request id.
    pub action: Value,
    /// Typed refusals computed from existing reads; runs before any input and
    /// only on the first call of a request id.
    pub plan: PlanFn<'j>,
    /// The ordinary input. Returns what it observed on screen.
    pub dispatch: DispatchFn<'j>,
    /// Compare the fields the action can change. `observed` is the dispatch's
    /// own evidence when it ran to the end.
    pub settle: SettleFn<'j>,
}

/// Shape of every result: the receipt plus the standing unvalidated marker.
pub(super) fn result(kind: &str, receipt: impl serde::Serialize, observed: Option<Value>) -> Value {
    json!({
        "receipt": receipt,
        "tool": kind,
        "validated": false,
        "observed": observed,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run(
    job: Job<'_>,
    id: &str,
    token: &str,
    process: &(dyn Process + Send + Sync),
    keys: &MacKeys<'static>,
    guard: &mut Guard,
    authority: &InputAuthority,
    actions: &mut GameJournal,
) -> Result<Value> {
    let expected = guard.observe_context(process)?;
    let epoch = guard.epoch.clone();
    let lease_end = authority.authorize(token, &epoch)?;
    let fence = Fence {
        process,
        epoch: &epoch,
        expected: &expected,
        authority,
        token,
        until: (Instant::now() + Duration::from_secs(12)).min(lease_end),
        stop: None,
    };
    let check_input = || fence.input_check();
    let bounded_keys = keys.guarded(&fence, &check_input)?;
    let process: &dyn Process = &fence;
    let retry = actions.receipt_exists(id);
    if !retry {
        (job.plan)(process)?;
        bounded_keys.preflight()?;
    }
    let observed: RefCell<Option<Value>> = RefCell::new(None);
    let refused: RefCell<Option<Error>> = RefCell::new(None);
    let mut settle = || {
        let seen = observed.borrow().clone();
        if seen.as_ref().is_some_and(|v| v.get("refused").is_some()) {
            return Ok(Outcome::Unchanged(seen.unwrap_or(Value::Null)));
        }
        let outcome = (job.settle)(process, seen.as_ref())?;
        Ok(match outcome {
            // A retried request never proves the original was not sent.
            Outcome::Unchanged(detail) if retry => Outcome::Diverged(detail),
            other => other,
        })
    };
    let mut dispatch = || {
        authority.authorize(token, &epoch)?;
        if retry {
            return Err(Error::new(
                "action_uncertain",
                "A retried request is never dispatched again",
            ));
        }
        match (job.dispatch)(process, &bounded_keys) {
            Ok(seen) => {
                *observed.borrow_mut() = Some(seen);
                Ok(())
            }
            Err(Fail::Refused(error)) => {
                *observed.borrow_mut() =
                    Some(json!({"refused": error.code, "message": error.message}));
                *refused.borrow_mut() = Some(error);
                Ok(())
            }
            Err(Fail::Failed(error)) => Err(error),
        }
    };
    let receipt = actions.execute(id, &epoch, job.action.clone(), &mut settle, &mut dispatch)?;
    if let Some(error) = refused.into_inner() {
        return Err(error);
    }
    let seen = observed.borrow().clone();
    Ok(result(job.kind, receipt, seen))
}

/// Verdict for a single boolean effect that only this action can create
/// (a window state, a progress bar): present means reached, absent means
/// unchanged, unreadable evidence is recorded as unverified.
pub(super) fn effect(seen: Option<&Value>, key: &str, detail: Value) -> Outcome {
    match seen.and_then(|v| v.get(key)).and_then(Value::as_bool) {
        Some(true) => Outcome::Reached(detail),
        Some(false) => Outcome::Unchanged(detail),
        None => Outcome::Diverged(json!({"unverified":true,"detail":detail})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_maps_the_observed_flag() {
        let yes = json!({"started": true});
        let no = json!({"started": false});
        assert!(matches!(effect(Some(&yes), "started", json!({})), Outcome::Reached(_)));
        assert!(matches!(effect(Some(&no), "started", json!({})), Outcome::Unchanged(_)));
        assert!(matches!(effect(None, "started", json!({})), Outcome::Diverged(_)));
        assert!(matches!(
            effect(Some(&json!({"started": "yes"})), "started", json!({})),
            Outcome::Diverged(_)
        ));
    }

    #[test]
    fn refusals_convert_and_plain_errors_stay_failures() {
        let f: Fail = Error::new("io_error", "x").into();
        assert!(matches!(f, Fail::Failed(_)));
        assert!(
            matches!(refuse("doctrine_not_offered", "x"), Fail::Refused(e) if e.code == "doctrine_not_offered")
        );
    }

    #[test]
    fn results_always_carry_the_unvalidated_marker() {
        let v = result("x", json!({"status":"verified"}), Some(json!({"a":1})));
        assert_eq!(v["validated"], json!(false));
        assert_eq!(v["tool"], json!("x"));
        assert_eq!(v["observed"]["a"], json!(1));
    }
}
