//! Durable Focus preparation/audit; permission and fresh checks are external.

use crate::{
    actions::Epoch,
    daily_governor::{Entry, Intent},
    error::{Error, Result},
    focus_stage::{ActiveFocus, Execution, OwnState, Receipt, Stage, Status, Target},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Explicit owner flag AND plan opt-in AND existing verified profile. These
/// permissions never stand in for fresh game state, lease or journal checks.
pub(crate) fn capability(owner: bool, plan: bool, profile: bool) -> Result<()> {
    if !plan {
        return Ok(());
    }
    if owner && profile {
        Ok(())
    } else {
        Err(Error::new(
            "capability_unavailable",
            "Daily Focus requires explicit owner binding, plan permission and Focus profile",
        ))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnBinding {
    focus: [u64; 7],
    politics: [u64; 6],
    slots: Vec<u64>,
}
impl OwnBinding {
    #[cfg(target_os = "linux")]
    pub(crate) fn from_context(
        focus: &crate::own_focus::FocusContext,
        politics: &crate::politics_focus::PoliticsContext,
    ) -> Result<Self> {
        let q = politics.anchors();
        let a = &focus.campaign;
        let b = politics.campaign();
        if a.player != b.player
            || a.playthrough_id != b.playthrough_id
            || a.anchors != b.anchors
            || a.difficulty != b.difficulty
            || a.historical_raw != b.historical_raw
            || a.date_raw > b.date_raw
            || [
                focus.anchors[0],
                focus.anchors[5],
                focus.anchors[4],
                focus.anchors[3],
            ] != [q[0], q[1], q[2], q[4]]
        {
            return Err(Error::new(
                "torn_read",
                "Own Focus/politics bindings disagree",
            ));
        }
        let value = Self {
            focus: focus.anchors,
            politics: q,
            slots: focus.slots.clone(),
        };
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<()> {
        if [self.focus[0], self.focus[5], self.focus[4], self.focus[3]]
            != [
                self.politics[0],
                self.politics[1],
                self.politics[2],
                self.politics[4],
            ]
        {
            return Err(invalid());
        }
        if self
            .focus
            .iter()
            .chain(self.politics.iter())
            .any(|p| *p == 0 || *p % 8 != 0)
            || self.slots.is_empty()
            || self.slots.len() > 32
            || self.slots.iter().any(|p| *p == 0 || *p % 8 != 0)
            || self
                .slots
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.slots.len()
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Failure {
    code: String,
    reason: String,
    evidence: crate::focus_stage::FailureEvidence,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    entry: Entry,
    target: Target,
    day: i64,
    revision: u64,
    index: u64,
    id: String,
    binding: OwnBinding,
    receipt: Option<Receipt>,
    failure: Option<Failure>,
    checksum: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Refusal {
    entry: Entry,
    day: i64,
    code: String,
    reason: String,
    checksum: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Goal {
    attempt: crate::daily_governor::Attempt,
    revision: u64,
    before: OwnBinding,
    after: OwnBinding,
    receipt: Receipt,
    observed: crate::daily_governor::Observation,
    checksum: String,
}
fn refusal_hash(r: &Refusal) -> Result<String> {
    hash(&(&r.entry, r.day, &r.code, &r.reason))
}
fn goal_hash(g: &Goal) -> Result<String> {
    hash(&(
        &g.attempt,
        g.revision,
        &g.before,
        &g.after,
        &g.receipt,
        &g.observed,
    ))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct WaitFact {
    entry: Entry,
    active: String,
    date_raw: i32,
    observed_unix_ms: u64,
    binding: OwnBinding,
    checksum: String,
}
fn wait_hash(w: &WaitFact) -> Result<String> {
    hash(&(
        &w.entry,
        &w.active,
        w.date_raw,
        w.observed_unix_ms,
        &w.binding,
    ))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalUncertainty {
    code: String,
    reason: String,
    step_id: String,
    receipt_hash: String,
    checksum: String,
}
fn uncertainty_hash(epoch: &Epoch, u: &TerminalUncertainty) -> Result<String> {
    hash(&(
        "focus-terminal-uncertainty-v1",
        epoch,
        &u.code,
        &u.reason,
        &u.step_id,
        &u.receipt_hash,
    ))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Data {
    version: u8,
    epoch: Epoch,
    steps: Vec<Step>,
    #[serde(default)]
    refusals: Vec<Refusal>,
    #[serde(default)]
    goals: Vec<Goal>,
    #[serde(default)]
    wait: Option<WaitFact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    uncertainty: Option<TerminalUncertainty>,
}
pub(crate) struct Navigator {
    store: crate::daily_governor::store::Store,
    data: Data,
    poisoned: bool,
}
fn invalid() -> Error {
    Error::new(
        "invalid_focus_navigation",
        "Bounded exact Focus preparation history required",
    )
}
fn hash<T: Serialize>(v: &T) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(v).map_err(|_| invalid())?)
    ))
}
fn id(s: &Step) -> Result<String> {
    hash(&(
        "daily-focus-preparation-v1",
        &s.entry,
        &s.target,
        s.day,
        s.revision,
        s.index,
        &s.binding,
    ))
}
fn checksum(s: &Step) -> Result<String> {
    hash(&(
        &s.entry, &s.target, s.day, s.revision, s.index, &s.id, &s.binding, &s.receipt, &s.failure,
    ))
}
fn validate_step(s: &Step, epoch: &Epoch, entries: &[Entry], index: usize) -> Result<()> {
    s.entry.validate()?;
    s.binding.validate()?;
    let Intent::Focus { focus_id } = &s.entry.intent else {
        return Err(invalid());
    };
    if !entries.contains(&s.entry)
        || s.target.epoch != *epoch
        || s.target.focus_id != *focus_id
        || s.target.stage == Stage::Start
        || !(1936 * 365..2001 * 365).contains(&s.day)
        || s.revision == 0
        || s.index != index as u64 + 1
        || s.id != id(s)?
        || s.checksum != checksum(s)?
    {
        return Err(invalid());
    }
    if let Some(r) = &s.receipt {
        r.observed.validate(&r.target)?;
        if s.failure.is_some()
            || r.version != 1
            || r.status != Status::Verified
            || r.request_id != s.id
            || r.target != s.target
            || !r.observed.reached(&s.target)
        {
            return Err(invalid());
        }
    }
    if let Some(f) = &s.failure {
        if f.code.is_empty()
            || f.code.len() > 64
            || !f.code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
            || f.reason.len() > 160
            || f.reason.chars().any(char::is_control)
        {
            return Err(invalid());
        }
    }
    Ok(())
}
fn validate_goal(g: &Goal, epoch: &Epoch, entries: &[Entry]) -> Result<()> {
    g.attempt.entry.validate()?;
    g.before.validate()?;
    g.after.validate()?;
    let Intent::Focus { focus_id } = &g.attempt.entry.intent else {
        return Err(invalid());
    };
    g.receipt.observed.validate(&g.receipt.target)?;
    if g.revision == 0
        || g.attempt.request_id.is_empty()
        || g.attempt.request_id.len() > 128
        || !entries.contains(&g.attempt.entry)
        || g.attempt.scope.epoch != *epoch
        || !(1936 * 365..2001 * 365).contains(&g.attempt.day)
        || g.before != g.after
        || g.before.focus[0] != g.attempt.scope.own_country
        || g.before.focus[6] != g.attempt.scope.own_status
        || crate::calendar::decode(g.observed.date_raw).is_err()
        || g.receipt.delivery == Some(crate::focus_stage::Delivery::NotQueued)
        || g.receipt.status != Status::Verified
        || g.receipt.version != 1
        || g.receipt.target.epoch != *epoch
        || g.receipt.target.stage != Stage::Start
        || g.receipt.target.focus_id != *focus_id
        || g.receipt.request_id != g.attempt.request_id
        || !matches!(&g.receipt.observed.active_focus,ActiveFocus::Active(key) if key==focus_id)
        || g.observed.scope != g.attempt.scope
        || g.observed.sequence <= g.attempt.before_sequence
        || g.checksum != goal_hash(g)?
    {
        return Err(invalid());
    }
    Ok(())
}
impl Navigator {
    pub(crate) fn open(path: &Path, epoch: Epoch, entries: &[Entry]) -> Result<Self> {
        let (store, bytes) = crate::daily_governor::store::Store::open(path)?;
        let data: Data = match bytes {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(|_| invalid())?,
            None => Data {
                version: 1,
                epoch: epoch.clone(),
                steps: vec![],
                refusals: vec![],
                goals: vec![],
                wait: None,
                uncertainty: None,
            },
        };
        if data.version != 1 || data.epoch != epoch || data.steps.len() > 4096 {
            return Err(invalid());
        }
        let mut previous = None;
        for (i, s) in data.steps.iter().enumerate() {
            validate_step(s, &epoch, entries, i)?;
            if previous.is_some_and(|day| s.day < day)
                || (i + 1 < data.steps.len()
                    && s.receipt.is_none()
                    && !s.failure.as_ref().is_some_and(|f| {
                        f.evidence != crate::focus_stage::FailureEvidence::Unknown
                    }))
            {
                return Err(invalid());
            }
            previous = Some(s.day);
        }
        if data.refusals.len() + data.goals.len() + data.steps.len() > 4096 {
            return Err(invalid());
        }
        let mut keys = std::collections::BTreeSet::new();
        for r in &data.refusals {
            r.entry.validate()?;
            if !entries.contains(&r.entry)
                || !matches!(r.entry.intent, Intent::Focus { .. })
                || !(1936 * 365..2001 * 365).contains(&r.day)
                || r.checksum != refusal_hash(r)?
                || !keys.insert((r.entry.id.clone(), r.day))
                || r.reason.len() > 160
                || r.reason.chars().any(char::is_control)
                || r.code.is_empty()
                || r.code.len() > 64
                || !r
                    .code
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            {
                return Err(invalid());
            }
        }
        let mut ids = std::collections::BTreeSet::new();
        let mut goals_entries = std::collections::BTreeSet::new();
        for g in &data.goals {
            validate_goal(g, &epoch, entries)?;
            if !ids.insert(&g.attempt.request_id) || !goals_entries.insert(&g.attempt.entry.id) {
                return Err(invalid());
            }
        }
        if let Some(w) = &data.wait {
            w.entry.validate()?;
            w.binding.validate()?;
            let Intent::Focus { focus_id } = &w.entry.intent else {
                return Err(invalid());
            };
            if !entries.contains(&w.entry)
                || w.active == *focus_id
                || w.active.is_empty()
                || w.active.len() > 256
                || !w
                    .active
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || crate::calendar::decode(w.date_raw).is_err()
                || w.observed_unix_ms == 0
                || w.checksum != wait_hash(w)?
            {
                return Err(invalid());
            }
        }
        if let Some(u) = &data.uncertainty {
            let step = data
                .steps
                .iter()
                .find(|s| s.id == u.step_id)
                .ok_or_else(invalid)?;
            let receipt = step.receipt.as_ref().ok_or_else(invalid)?;
            if u.code != "action_uncertain"
                || u.reason.len() > 160
                || u.reason.chars().any(char::is_control)
                || u.receipt_hash != hash(receipt)?
                || u.checksum != uncertainty_hash(&data.epoch, u)?
            {
                return Err(invalid());
            }
        }
        Ok(Self {
            store,
            data,
            poisoned: false,
        })
    }
    fn save(&mut self) -> Result<()> {
        if let Some(last) = self.data.steps.last_mut() {
            last.checksum = checksum(last)?;
        }
        self.store
            .replace(&serde_json::to_vec(&self.data).map_err(|_| invalid())?)
            .inspect_err(|_| self.poisoned = true)
    }
    pub(crate) fn blocked(&self) -> bool {
        self.poisoned
            || self.data.uncertainty.is_some()
            || self.data.steps.iter().any(|s| {
                s.receipt.is_none()
                    && !s
                        .failure
                        .as_ref()
                        .is_some_and(|f| f.evidence != crate::focus_stage::FailureEvidence::Unknown)
            })
    }
    pub(crate) fn refused_today(&self, entry: &Entry, day: i64) -> bool {
        self.data
            .refusals
            .iter()
            .any(|r| r.entry == *entry && r.day == day)
            || self
                .data
                .steps
                .iter()
                .any(|s| s.entry == *entry && s.day == day && s.failure.is_some())
    }
    pub(crate) fn latest(&self) -> serde_json::Value {
        let preparation=self.data.steps.last().map(|s|serde_json::json!({"entry_id":s.entry.id,"stage":s.target.stage,"request_id":s.id,"verified":s.receipt.is_some(),"goal_completed":false,"historical":true}));
        let failure=self.data.steps.iter().rev().find_map(|s|s.failure.as_ref().map(|f|serde_json::json!({"entry_id":s.entry.id,"stage":s.target.stage,"code":f.code,"reason":f.reason,"day":s.day,"historical":true})));
        let refusal=self.data.refusals.last().map(|r|serde_json::json!({"entry_id":r.entry.id,"code":r.code,"reason":r.reason,"day":r.day,"historical":true}));
        let start=self.data.goals.last().map(|g|serde_json::json!({"entry_id":g.attempt.entry.id,"request_id":g.attempt.request_id,"stage":"Start","historical":true}));
        let wait=self.data.wait.as_ref().map(|w|serde_json::json!({"entry_id":w.entry.id,"active_focus_id":w.active,"date_raw":w.date_raw,"observed_unix_ms":w.observed_unix_ms,"source":"native witnessed own focus","historical":true,"navigation_attempted":false,"assignment_budget_consumed":false}));
        serde_json::json!({"preparation":preparation,"failure":failure,"refusal":refusal,"start":start,"wait":wait,"uncertainty":self.data.uncertainty.as_ref().map(|u|serde_json::json!({"code":u.code,"reason":u.reason,"request_id":u.step_id,"historical":true,"blocks_all_input":true}))})
    }
    /// A verified packet never proves that no input occurred. Retain its exact
    /// receipt and persist a separate terminal marker for subsequent uncertainty.
    pub(crate) fn quarantine_after_verified(&mut self, error: &Error) -> Result<()> {
        // Poison before checking/writing: any failure keeps the live navigator
        // blocked, including failures before Store::replace begins.
        self.poisoned = true;
        let step = self.data.steps.last().ok_or_else(invalid)?;
        let receipt = step.receipt.as_ref().ok_or_else(invalid)?;
        if error.code != "action_uncertain"
            || receipt.status != Status::Verified
            || step.failure.is_some()
        {
            return Err(invalid());
        }
        let mut u = TerminalUncertainty {
            code: error.code.into(),
            reason: crate::daily_navigation::bounded_reason(&error.message),
            step_id: step.id.clone(),
            receipt_hash: hash(receipt)?,
            checksum: String::new(),
        };
        u.checksum = uncertainty_hash(&self.data.epoch, &u)?;
        if let Some(old) = &self.data.uncertainty {
            return if old.checksum == u.checksum {
                Ok(())
            } else {
                Err(invalid())
            };
        }
        self.data.uncertainty = Some(u);
        self.store.check()?;
        self.save()
    }
    pub(crate) fn record_wait(
        &mut self,
        entry: &Entry,
        active: &str,
        date_raw: i32,
        binding: OwnBinding,
    ) -> Result<()> {
        self.store.check().inspect_err(|_| self.poisoned = true)?;
        let Intent::Focus { focus_id } = &entry.intent else {
            return Err(invalid());
        };
        entry.validate()?;
        binding.validate()?;
        if self.blocked()
            || active == focus_id
            || active.is_empty()
            || active.len() > 256
            || !active
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            || crate::calendar::decode(date_raw).is_err()
        {
            return Err(invalid());
        }
        // Persist only an entry/active/day/binding change, never every clock probe.
        if self.data.wait.as_ref().is_some_and(|w| {
            w.entry == *entry
                && w.active == active
                && (i64::from(w.date_raw) - 43800000).div_euclid(24)
                    == (i64::from(date_raw) - 43800000).div_euclid(24)
                && w.binding == binding
        }) {
            return Ok(());
        }
        let mut w = WaitFact {
            entry: entry.clone(),
            active: active.into(),
            date_raw,
            observed_unix_ms: crate::daily_observation::unix_ms()?,
            binding,
            checksum: String::new(),
        };
        w.checksum = wait_hash(&w)?;
        self.data.wait = Some(w);
        self.save()
    }
    pub(crate) fn refuse_read_only(
        &mut self,
        entry: &Entry,
        day: i64,
        error: &Error,
    ) -> Result<()> {
        self.store.check().inspect_err(|_| self.poisoned = true)?;
        if self.blocked()
            || self.data.steps.len() + self.data.goals.len() + self.data.refusals.len() >= 4096
            || !(1936 * 365..2001 * 365).contains(&day)
        {
            return Err(invalid());
        }
        entry.validate()?;
        if !matches!(entry.intent, Intent::Focus { .. }) {
            return Err(invalid());
        }
        if self
            .data
            .refusals
            .iter()
            .any(|r| r.entry == *entry && r.day == day)
        {
            return Ok(());
        }
        let mut r = Refusal {
            entry: entry.clone(),
            day,
            code: error.code.into(),
            reason: crate::daily_navigation::bounded_reason(&error.message),
            checksum: String::new(),
        };
        r.checksum = refusal_hash(&r)?;
        self.data.refusals.push(r);
        self.save()
    }
    pub(crate) fn record_start(
        &mut self,
        attempt: &crate::daily_governor::Attempt,
        revision: u64,
        before: OwnBinding,
        after: OwnBinding,
        receipt: Receipt,
        observed: crate::daily_governor::Observation,
    ) -> Result<()> {
        self.store.check().inspect_err(|_| self.poisoned = true)?;
        if self.blocked()
            || self.data.steps.len() + self.data.goals.len() + self.data.refusals.len() >= 4096
        {
            return Err(invalid());
        }
        let mut g = Goal {
            attempt: attempt.clone(),
            revision,
            before,
            after,
            receipt,
            observed,
            checksum: String::new(),
        };
        g.checksum = goal_hash(&g)?;
        validate_goal(&g, &self.data.epoch, std::slice::from_ref(&attempt.entry))?;
        if let Some(old) = self
            .data
            .goals
            .iter()
            .find(|old| old.attempt.request_id == attempt.request_id)
        {
            return if old.checksum == g.checksum {
                Ok(())
            } else {
                Err(invalid())
            };
        }
        if self
            .data
            .goals
            .iter()
            .any(|old| old.attempt.entry.id == attempt.entry.id)
        {
            return Err(invalid());
        }
        self.data.goals.push(g);
        self.save()
    }
    pub(crate) fn begin(
        &mut self,
        entry: Entry,
        target: Target,
        day: i64,
        revision: u64,
        binding: OwnBinding,
    ) -> Result<String> {
        self.store.check().inspect_err(|_| self.poisoned = true)?;
        if self.blocked()
            || self.data.steps.len() + self.data.goals.len() + self.data.refusals.len() >= 4096
            || self.data.steps.last().is_some_and(|s| s.day > day)
            || self
                .data
                .steps
                .iter()
                .any(|s| s.entry == entry && s.day == day && s.failure.is_some())
        {
            return Err(invalid());
        }
        let mut step = Step {
            entry,
            target,
            day,
            revision,
            index: self.data.steps.len() as u64 + 1,
            id: String::new(),
            binding,
            receipt: None,
            failure: None,
            checksum: String::new(),
        };
        step.id = id(&step)?;
        step.checksum = checksum(&step)?;
        validate_step(
            &step,
            &self.data.epoch,
            std::slice::from_ref(&step.entry),
            self.data.steps.len(),
        )?;
        let id = step.id.clone();
        self.data.steps.push(step);
        self.save()?;
        Ok(id)
    }
    pub(crate) fn verified(&mut self, e: &Execution, binding: &OwnBinding) -> Result<()> {
        self.store.check().inspect_err(|_| self.poisoned = true)?;
        let last = self.data.steps.last_mut().ok_or_else(invalid)?;
        if last.receipt.is_some()
            || last.failure.is_some()
            || &last.binding != binding
            || e.historical
            || !e.current_state_verified
            || e.receipt.version != 1
            || e.receipt.status != Status::Verified
            || e.receipt.request_id != last.id
            || e.receipt.target != last.target
            || !e.receipt.observed.reached(&last.target)
        {
            return Err(Error::new(
                "action_uncertain",
                "Fresh exact Focus preparation receipt/binding required",
            ));
        }
        e.receipt.observed.validate(&last.target)?;
        last.receipt = Some(e.receipt.clone());
        self.save()
    }
    /// Only a trusted immediate adapter classification can mark known pre-input.
    /// Unknown/lost delivery leaves the ledger globally blocked; never retries.
    pub(crate) fn failed(
        &mut self,
        error: &Error,
        evidence: crate::focus_stage::FailureEvidence,
    ) -> Result<()> {
        self.store.check().inspect_err(|_| self.poisoned = true)?;
        let last = self.data.steps.last_mut().ok_or_else(invalid)?;
        if last.receipt.is_some()
            || last.failure.is_some()
            || (error.code == "action_uncertain"
                && evidence != crate::focus_stage::FailureEvidence::Unknown)
        {
            return Err(invalid());
        }
        last.failure = Some(Failure {
            code: error.code.into(),
            reason: crate::daily_navigation::bounded_reason(&error.message),
            evidence,
        });
        self.save()
    }
}
pub(crate) enum Next {
    Wait,
    Stage(Stage),
    Start,
}
pub(crate) fn next(state: &OwnState, target: &Target) -> Result<Next> {
    state.validate(target)?;
    match &state.active_focus {
        ActiveFocus::Unknown => {
            return Err(Error::new(
                "focus_unavailable",
                "Unknown active focus is not idle",
            ))
        }
        ActiveFocus::Active(key) if key != &target.focus_id => return Ok(Next::Wait),
        ActiveFocus::Active(_) => return Ok(Next::Start),
        ActiveFocus::Idle => {}
    }
    for stage in [
        Stage::OpenPolitics,
        Stage::OpenFocus,
        Stage::View,
        Stage::Start,
    ] {
        let target = Target {
            stage,
            ..target.clone()
        };
        if state.preflight(&target).is_ok() {
            return Ok(if stage == Stage::Start {
                Next::Start
            } else {
                Next::Stage(stage)
            });
        }
    }
    Err(Error::new(
        "focus_precondition",
        "Current own roots/blockers cannot be navigated; no blind transitions",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        focus_stage::{Delivery, FailureEvidence, LocalRoot},
        platform::Identity,
    };
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let p = std::env::temp_dir().join(format!(
                "private-focus-foundation-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&p).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self(p)
        }
        fn path(&self) -> std::path::PathBuf {
            self.0.join("focus-navigation.json")
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn epoch() -> Epoch {
        Epoch {
            game: Identity {
                pid: 123,
                start_ticks: "456".into(),
                exe: "/generated/game".into(),
                uid: 1000,
            },
            campaign: "generated".into(),
            player: "GER".into(),
        }
    }
    fn entry() -> Entry {
        Entry {
            id: "focus-head".into(),
            intent: Intent::Focus {
                focus_id: "GER_rhineland".into(),
            },
            navigation: None,
        }
    }
    fn target(stage: Stage) -> Target {
        Target {
            epoch: epoch(),
            stage,
            focus_id: "GER_rhineland".into(),
        }
    }
    fn binding() -> OwnBinding {
        OwnBinding {
            focus: [0x1000, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000, 0x7000],
            politics: [0x1000, 0x6000, 0x5000, 0x8000, 0x4000, 0x9000],
            slots: vec![0xa000],
        }
    }
    fn state() -> OwnState {
        OwnState {
            epoch: epoch(),
            active_focus: ActiveFocus::Idle,
            canonical_focus_id: Some("GER_rhineland".into()),
            managed_mode: Some(23),
            politics_root: LocalRoot::Hidden,
            focus_root: LocalRoot::Hidden,
            own_detail_counts: Some([0, 0]),
            own_research_info_counts: Some([0, 0]),
            selected_focus: None,
            requested_detail_ready: Some(false),
        }
    }
    fn receipt(id: String) -> Execution {
        let mut s = state();
        s.managed_mode = Some(12);
        s.politics_root = LocalRoot::Ready;
        Execution {
            receipt: Receipt {
                version: 1,
                request_id: id,
                target: target(Stage::OpenPolitics),
                status: Status::Verified,
                delivery: Some(Delivery::ServerConfirmed),
                observed: s,
            },
            historical: false,
            current_state_verified: true,
            event_causality_verified: false,
        }
    }
    #[test]
    fn idle_navigation_active_wait_unknown_refusal_and_hard_gate() {
        assert_eq!(
            capability(false, true, true).unwrap_err().code,
            "capability_unavailable"
        );
        let mut s = state();
        assert!(matches!(
            next(&s, &target(Stage::Start)).unwrap(),
            Next::Stage(Stage::OpenPolitics)
        ));
        s.active_focus = ActiveFocus::Active("GER_other".into());
        assert!(matches!(
            next(&s, &target(Stage::Start)).unwrap(),
            Next::Wait
        ));
        s.active_focus = ActiveFocus::Unknown;
        assert_eq!(
            next(&s, &target(Stage::Start)).err().unwrap().code,
            "focus_unavailable"
        );
        s.active_focus = ActiveFocus::Idle;
        s.managed_mode = Some(6);
        assert!(next(&s, &target(Stage::Start)).is_err());
        s = state();
        s.managed_mode = Some(12);
        s.politics_root = LocalRoot::Ready;
        assert!(matches!(
            next(&s, &target(Stage::Start)).unwrap(),
            Next::Stage(Stage::OpenFocus)
        ));
        s.managed_mode = Some(13);
        s.politics_root = LocalRoot::Hidden;
        s.focus_root = LocalRoot::Ready;
        assert!(matches!(
            next(&s, &target(Stage::Start)).unwrap(),
            Next::Stage(Stage::View)
        ));
        s.own_detail_counts = Some([1, 0]);
        s.selected_focus = Some("GER_rhineland".into());
        s.requested_detail_ready = Some(true);
        assert!(matches!(
            next(&s, &target(Stage::Start)).unwrap(),
            Next::Start
        ));
    }
    #[test]
    fn durable_preparation_receipt_is_fresh_exact_binding_and_start_excluded() {
        let t = Scratch::new();
        let e = entry();
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        let id = n
            .begin(
                e.clone(),
                target(Stage::OpenPolitics),
                1936 * 365,
                1,
                binding(),
            )
            .unwrap();
        assert!(n.blocked());
        let mut r = receipt(id);
        r.historical = true;
        assert!(n.verified(&r, &binding()).is_err());
        r.historical = false;
        let mut foreign = binding();
        foreign.focus[1] += 8;
        assert!(n.verified(&r, &foreign).is_err());
        n.verified(&r, &binding()).unwrap();
        assert!(!n.blocked());
        drop(n);
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        assert!(!n.blocked());
        assert!(n
            .begin(e, target(Stage::Start), 1936 * 365, 1, binding())
            .is_err());
    }
    #[test]
    fn unknown_delivery_blocks_restart_and_preinput_failure_retains_exact_cause() {
        for evidence in [
            FailureEvidence::Unknown,
            FailureEvidence::BeforeIntent,
            FailureEvidence::ConfirmedNotQueued,
        ] {
            let t = Scratch::new();
            let e = entry();
            let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
            n.begin(e.clone(), target(Stage::View), 1936 * 365, 1, binding())
                .unwrap();
            n.failed(
                &Error::new("torn_read", "Exact original own witness reason"),
                evidence,
            )
            .unwrap();
            drop(n);
            let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
            assert_eq!(n.blocked(), evidence == FailureEvidence::Unknown);
            assert_eq!(
                n.data.steps[0].failure.as_ref().unwrap().reason,
                "Exact original own witness reason"
            );
            assert!(n
                .begin(e, target(Stage::View), 1936 * 365, 1, binding())
                .is_err());
        }
    }
    #[test]
    fn forged_entry_target_day_binding_and_late_unknown_history_refuse() {
        for field in ["entry", "target", "day", "binding"] {
            let t = Scratch::new();
            let e = entry();
            let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
            let id = n
                .begin(
                    e.clone(),
                    target(Stage::OpenPolitics),
                    1936 * 365,
                    1,
                    binding(),
                )
                .unwrap();
            n.verified(&receipt(id), &binding()).unwrap();
            drop(n);
            let mut d: Data = serde_json::from_slice(&std::fs::read(t.path()).unwrap()).unwrap();
            let s = &mut d.steps[0];
            match field {
                "entry" => s.entry.id = "forged".into(),
                "target" => s.target.focus_id = "GER_other".into(),
                "day" => s.day += 1,
                _ => s.binding.focus[1] += 8,
            };
            s.checksum = checksum(s).unwrap();
            std::fs::write(t.path(), serde_json::to_vec(&d).unwrap()).unwrap();
            assert!(Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).is_err());
        }
    }
    #[test]
    fn lost_response_and_external_store_mutation_never_allow_another_stage() {
        let t = Scratch::new();
        let e = entry();
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        n.begin(
            e.clone(),
            target(Stage::OpenPolitics),
            1936 * 365,
            1,
            binding(),
        )
        .unwrap();
        drop(n);
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        assert!(n.blocked());
        assert!(n
            .begin(e, target(Stage::OpenFocus), 1936 * 365, 1, binding())
            .is_err());
        std::fs::write(t.path(), b"changed").unwrap();
        assert!(n
            .failed(
                &Error::new("input_busy", "preflight"),
                FailureEvidence::BeforeIntent
            )
            .is_err());
        assert!(n.blocked());
    }
    #[test]
    fn permission_requires_both_flags_and_existing_profile() {
        for (owner, plan, profile) in [
            (false, true, true),
            (true, true, false),
            (false, true, false),
        ] {
            assert_eq!(
                capability(owner, plan, profile).unwrap_err().code,
                "capability_unavailable"
            );
        }
        assert!(capability(false, false, false).is_ok());
        assert!(capability(true, true, true).is_ok());
    }
    #[test]
    fn active_wait_persists_without_preparation_refusal_and_unchanged_probe_rewrite() {
        let t = Scratch::new();
        let e = entry();
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        let date = crate::calendar::encode(1936, 1, 1).unwrap();
        n.record_wait(&e, "GER_other", date, binding()).unwrap();
        let bytes = std::fs::read(t.path()).unwrap();
        n.record_wait(&e, "GER_other", date + 1, binding()).unwrap();
        assert_eq!(std::fs::read(t.path()).unwrap(), bytes);
        assert!(!n.blocked());
        assert!(!n.refused_today(&e, 1936 * 365));
        assert!(n.data.steps.is_empty());
        assert!(n.data.goals.is_empty());
        drop(n);
        let n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        assert_eq!(n.latest()["wait"]["active_focus_id"], "GER_other");
        assert!(!n.refused_today(&e, 1936 * 365));
    }
    #[test]
    fn start_audit_retains_full_scope_and_does_not_supply_preparation_or_completion_authority() {
        let t = Scratch::new();
        let e = entry();
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        let scope = crate::daily_governor::Scope {
            epoch: epoch(),
            anchors: [0x10000, 0x20000, 0x30000],
            own_country: 0x1000,
            own_status: 0x7000,
        };
        let attempt = crate::daily_governor::Attempt {
            request_id: "a".repeat(64),
            entry: e.clone(),
            scope: scope.clone(),
            day: 1936 * 365,
            before_sequence: 5,
        };
        let mut r = receipt(attempt.request_id.clone()).receipt;
        r.target.stage = Stage::Start;
        r.observed.active_focus = ActiveFocus::Active("GER_rhineland".into());
        let observed = crate::daily_governor::Observation {
            scope,
            date_raw: crate::calendar::encode(1936, 1, 1).unwrap(),
            sequence: 6,
            observed_unix_ms: 10000,
            source: "generated own".into(),
            counts: crate::daily_governor::OwnCounts::default(),
        };
        let mut wrong = binding();
        wrong.focus[1] += 8;
        assert!(n
            .record_start(&attempt, 1, binding(), wrong, r.clone(), observed.clone())
            .is_err());
        n.record_start(
            &attempt,
            1,
            binding(),
            binding(),
            r.clone(),
            observed.clone(),
        )
        .unwrap();
        let bytes = std::fs::read(t.path()).unwrap();
        n.record_start(&attempt, 1, binding(), binding(), r, observed)
            .unwrap();
        assert_eq!(std::fs::read(t.path()).unwrap(), bytes);
        assert!(n.data.steps.is_empty());
        drop(n);
        let n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        assert_eq!(n.latest()["start"]["historical"], true);
        assert!(!n.blocked());
    }
    #[test]
    fn read_only_refusal_persists_and_does_not_turn_unknown_into_idle() {
        let t = Scratch::new();
        let e = entry();
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        n.refuse_read_only(
            &e,
            1936 * 365,
            &Error::new(
                "focus_precondition",
                "Unsupported cross-domain root; no UI escape",
            ),
        )
        .unwrap();
        drop(n);
        let n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        assert!(n.refused_today(&e, 1936 * 365));
        assert!(!n.refused_today(&e, 1936 * 365 + 1));
        assert!(!n.blocked());
        assert_eq!(
            n.latest()["refusal"]["reason"],
            "Unsupported cross-domain root; no UI escape"
        );
    }
    #[test]
    fn late_verified_preparation_retains_receipt_and_blocks_reopen_all_work() {
        let t = Scratch::new();
        let e = entry();
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        let id = n
            .begin(
                e.clone(),
                target(Stage::OpenPolitics),
                1936 * 365,
                1,
                binding(),
            )
            .unwrap();
        let execution = receipt(id);
        n.verified(&execution, &binding()).unwrap();
        assert!(!n.blocked());
        // Deterministic post-verification expiry; no filesystem timing assumption.
        let deadline = std::time::Instant::now() - std::time::Duration::from_secs(1);
        assert!(std::time::Instant::now() >= deadline);
        let error = Error::new(
            "action_uncertain",
            "Focus preparation receipt exceeded original deadline",
        );
        n.quarantine_after_verified(&error).unwrap();
        assert!(n.blocked());
        assert_eq!(
            n.data.steps[0].receipt.as_ref().unwrap(),
            &execution.receipt
        );
        assert!(n
            .failed(
                &Error::new("input_busy", "Before-input claim is invalid"),
                crate::focus_stage::FailureEvidence::BeforeIntent
            )
            .is_err());
        drop(n);
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        assert!(n.blocked());
        assert_eq!(n.latest()["uncertainty"]["reason"], error.message);
        assert!(n
            .begin(e, target(Stage::OpenFocus), 1936 * 365, 1, binding())
            .is_err());
        assert_eq!(
            crate::daily_worker::disposition(Some(&error), n.blocked()),
            crate::daily_worker::Disposition::Quarantine
        );
    }
    #[test]
    fn normal_verified_preparation_reopen_remains_usable() {
        let t = Scratch::new();
        let e = entry();
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        let id = n
            .begin(
                e.clone(),
                target(Stage::OpenPolitics),
                1936 * 365,
                1,
                binding(),
            )
            .unwrap();
        n.verified(&receipt(id), &binding()).unwrap();
        drop(n);
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        assert!(!n.blocked());
        assert!(n
            .begin(e, target(Stage::OpenFocus), 1936 * 365, 1, binding())
            .is_ok());
    }
    #[test]
    fn uncertainty_save_failure_poison_preserves_original_evidence_and_marker_tamper_refuses() {
        let t = Scratch::new();
        let e = entry();
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        let id = n
            .begin(
                e.clone(),
                target(Stage::OpenPolitics),
                1936 * 365,
                1,
                binding(),
            )
            .unwrap();
        n.verified(&receipt(id), &binding()).unwrap();
        std::fs::write(t.path(), b"external mutation").unwrap();
        let error = Error::new("action_uncertain", "Original late preparation reason");
        assert!(n.quarantine_after_verified(&error).is_err());
        assert!(n.blocked());
        assert_eq!(n.latest()["uncertainty"]["reason"], error.message);
        assert!(n
            .begin(
                e.clone(),
                target(Stage::OpenFocus),
                1936 * 365,
                1,
                binding()
            )
            .is_err());
        drop(n);
        assert!(Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).is_err());
        let t = Scratch::new();
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        let id = n
            .begin(
                e.clone(),
                target(Stage::OpenPolitics),
                1936 * 365,
                1,
                binding(),
            )
            .unwrap();
        n.verified(&receipt(id), &binding()).unwrap();
        n.quarantine_after_verified(&error).unwrap();
        drop(n);
        let mut data: Data = serde_json::from_slice(&std::fs::read(t.path()).unwrap()).unwrap();
        data.uncertainty.as_mut().unwrap().reason = "Forged".into();
        std::fs::write(t.path(), serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).is_err());
    }
}
