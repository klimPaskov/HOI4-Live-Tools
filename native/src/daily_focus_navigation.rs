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

mod readiness;
#[cfg(target_os = "linux")]
pub(crate) use readiness::observed_ready;

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
/// Scheduling hint only; already-assigned research and pending retries bypass handoff.
pub(crate) fn needs_research_handoff(
    permission: bool,
    pending_retry: bool,
    entry: &Entry,
    research: &crate::research_stage::OwnState,
) -> bool {
    permission
        && !pending_retry
        && research.active_mode == Some(12)
        && matches!(&entry.intent, Intent::Research { slot, .. }
            if research.researching.get(*slot as usize) == Some(&None))
}
/// The durable Focus entry is only a key hint. Both independent own-state
/// producers must agree on the epoch and a closed research UI, and the fresh
/// Focus state must satisfy the same strict ClosePolitics preflight as manual
/// input. This function supplies no permission, lease or transport authority.
pub(crate) fn research_handoff_target(
    focus_entry: &Entry,
    research_entry: &Entry,
    focus: &OwnState,
    research: &crate::research_stage::OwnState,
) -> Result<Target> {
    focus_entry.validate()?;
    research_entry.validate()?;
    let Intent::Focus { focus_id } = &focus_entry.intent else {
        return Err(invalid());
    };
    let Intent::Research { slot, .. } = &research_entry.intent else {
        return Err(invalid());
    };
    if research.epoch != focus.epoch
        || research.active_mode != Some(12)
        || research.panel_visible != Some(false)
        || research.tree_visible
        || !research.visible_info.is_empty()
        || research.researching.get(*slot as usize) != Some(&None)
    {
        return Err(Error::new(
            "focus_precondition",
            "Fresh same-epoch empty research slot and closed research UI required for handoff",
        ));
    }
    let target = Target {
        epoch: focus.epoch.clone(),
        stage: Stage::ClosePolitics,
        focus_id: focus_id.clone(),
    };
    focus.validate(&target)?;
    focus.preflight(&target)?;
    Ok(target)
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
struct StartFailure {
    attempt: crate::daily_governor::Attempt,
    revision: u64,
    index: u8,
    code: String,
    reason: String,
    evidence: crate::focus_stage::FailureEvidence,
    retry_planned: bool,
    checksum: String,
}
fn start_failure_hash(f: &StartFailure) -> Result<String> {
    hash(&(
        "focus-start-failure-v1",
        &f.attempt,
        f.revision,
        f.index,
        &f.code,
        &f.reason,
        f.evidence,
        f.retry_planned,
    ))
}
pub(crate) fn retry_before_intent(
    error: &Error,
    evidence: crate::focus_stage::FailureEvidence,
    failures: u8,
) -> bool {
    error.code == "torn_read"
        && evidence == crate::focus_stage::FailureEvidence::BeforeIntent
        && failures < 3
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    start_failures: Vec<StartFailure>,
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
            || !f
                .code
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
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
                start_failures: vec![],
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
        let mut failures: std::collections::BTreeMap<&str, &StartFailure> =
            std::collections::BTreeMap::new();
        if data.start_failures.len() > 4096 {
            return Err(invalid());
        }
        for f in &data.start_failures {
            f.attempt.entry.validate()?;
            if !entries.contains(&f.attempt.entry)
                || f.attempt.scope.epoch != epoch
                || !matches!(f.attempt.entry.intent, Intent::Focus { .. })
                || f.attempt.request_id.len() != 64
                || !f.attempt.request_id.bytes().all(|b| b.is_ascii_hexdigit())
                || f.attempt.before_sequence == 0
                || f.attempt
                    .scope
                    .anchors
                    .iter()
                    .chain([&f.attempt.scope.own_country, &f.attempt.scope.own_status])
                    .any(|p| !(0x10000..=0x7fff_ffff_0000).contains(p))
                || f.revision == 0
                || !(1936 * 365..2001 * 365).contains(&f.attempt.day)
                || !(1..=3).contains(&f.index)
                || f.code.is_empty()
                || f.code.len() > 64
                || !f
                    .code
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                || f.reason.len() > 160
                || f.reason.chars().any(char::is_control)
                || f.checksum != start_failure_hash(f)?
                || (f.retry_planned
                    && !(f.code == "torn_read"
                        && f.evidence == crate::focus_stage::FailureEvidence::BeforeIntent
                        && f.index < 3))
            {
                return Err(invalid());
            }
            match failures.get(f.attempt.request_id.as_str()) {
                Some(previous)
                    if previous.retry_planned
                        && previous.attempt == f.attempt
                        && previous.revision == f.revision
                        && f.index == previous.index + 1 => {}
                None if f.index == 1 => {}
                _ => return Err(invalid()),
            }
            failures.insert(&f.attempt.request_id, f);
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
            || self
                .data
                .start_failures
                .iter()
                .any(|f| f.evidence == crate::focus_stage::FailureEvidence::Unknown)
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
    pub(crate) fn record_start_failure(
        &mut self,
        attempt: &crate::daily_governor::Attempt,
        revision: u64,
        error: &Error,
        evidence: crate::focus_stage::FailureEvidence,
        index: u8,
        retry_planned: bool,
    ) -> Result<()> {
        self.store.check().inspect_err(|_| self.poisoned = true)?;
        if self.blocked()
            || self.data.start_failures.len() >= 4096
            || attempt.scope.epoch != self.data.epoch
            || !(1..=3).contains(&index)
            || (retry_planned && !retry_before_intent(error, evidence, index))
        {
            return Err(invalid());
        }
        let previous = self
            .data
            .start_failures
            .iter()
            .rev()
            .find(|f| f.attempt.request_id == attempt.request_id);
        if !match previous {
            None => index == 1,
            Some(p) => {
                p.retry_planned
                    && p.attempt == *attempt
                    && p.revision == revision
                    && p.index + 1 == index
            }
        } {
            return Err(invalid());
        }
        let mut f = StartFailure {
            attempt: attempt.clone(),
            revision,
            index,
            code: error.code.into(),
            reason: crate::daily_navigation::bounded_reason(&error.message),
            evidence,
            retry_planned,
            checksum: String::new(),
        };
        f.checksum = start_failure_hash(&f)?;
        self.data.start_failures.push(f);
        self.save()
    }
    pub(crate) fn latest(&self) -> serde_json::Value {
        let start_failure = self.data.start_failures.last().map(|last| {
            let first = self.data.start_failures.iter().find(|f| f.attempt.request_id == last.attempt.request_id).expect("validated failure");
            serde_json::json!({"entry_id":last.attempt.entry.id,"request_id":last.attempt.request_id,"day":last.attempt.day,"original_code":first.code,"original_reason":first.reason,"code":last.code,"reason":last.reason,"evidence":last.evidence,"failures":last.index,"retry_planned_historical":last.retry_planned,"historical":true,"retry_authority":false})
        });
        let preparation=self.data.steps.last().map(|s|serde_json::json!({"entry_id":s.entry.id,"stage":s.target.stage,"request_id":s.id,"verified":s.receipt.is_some(),"goal_completed":false,"historical":true}));
        let failure=self.data.steps.iter().rev().find_map(|s|s.failure.as_ref().map(|f|serde_json::json!({"entry_id":s.entry.id,"stage":s.target.stage,"code":f.code,"reason":f.reason,"day":s.day,"historical":true})));
        let refusal=self.data.refusals.last().map(|r|serde_json::json!({"entry_id":r.entry.id,"code":r.code,"reason":r.reason,"day":r.day,"historical":true}));
        let start=self.data.goals.last().map(|g|serde_json::json!({"entry_id":g.attempt.entry.id,"request_id":g.attempt.request_id,"stage":"Start","historical":true}));
        let wait=self.data.wait.as_ref().map(|w|serde_json::json!({"entry_id":w.entry.id,"active_focus_id":w.active,"date_raw":w.date_raw,"observed_unix_ms":w.observed_unix_ms,"source":"native witnessed own focus","historical":true,"navigation_attempted":false,"assignment_budget_consumed":false}));
        serde_json::json!({"start_failure":start_failure,"preparation":preparation,"failure":failure,"refusal":refusal,"start":start,"wait":wait,"uncertainty":self.data.uncertainty.as_ref().map(|u|serde_json::json!({"code":u.code,"reason":u.reason,"request_id":u.step_id,"historical":true,"blocks_all_input":true}))})
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
    pub(crate) fn check_handoff(&mut self, entry: &Entry, day: i64) -> Result<()> {
        self.store.check().inspect_err(|_| self.poisoned = true)?;
        if self.blocked() {
            return Err(Error::new(
                "action_uncertain",
                "Unresolved Focus navigation blocks handoff",
            ));
        }
        if self.refused_today(entry, day)
            || self
                .data
                .steps
                .iter()
                .any(|s| s.entry == *entry && s.target.stage == Stage::ClosePolitics)
        {
            return Err(Error::new(
                "research_precondition",
                "Prior Focus handoff/refusal is retained; no replay",
            ));
        }
        Ok(())
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
            // A completed Focus has one automatic handoff. Even a later fresh
            // Politics panel must not reopen a prior ClosePolitics attempt.
            || (target.stage == Stage::ClosePolitics
                && self.data.steps.iter().any(|s| {
                    s.entry == entry && s.target.stage == Stage::ClosePolitics
                }))
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
pub(crate) fn preparation_permission(stage: Stage, research_permission: bool) -> Result<()> {
    if stage == Stage::OpenPoliticsFromResearch && !research_permission {
        Err(Error::new(
            "capability_unavailable",
            "Research-to-Politics requires both worker domain permissions",
        ))
    } else {
        Ok(())
    }
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
        Stage::OpenPoliticsFromResearch,
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
    // Keep the actual accepted observation's failing facts, not a later read.
    // Static enums/counts fit the durable 160-byte reason limit. No keys,
    // addresses, geometry or new reads enter this diagnostic.
    fn counts(value: Option<[usize; 2]>) -> String {
        value.map_or_else(|| "unknown".into(), |v| format!("{}/{}", v[0], v[1]))
    }
    let mode = state
        .managed_mode
        .map_or_else(|| "unknown".into(), |v| v.to_string());
    let selected = match state.selected_focus.as_deref() {
        None => "none",
        Some(id) if id == target.focus_id => "target",
        Some(_) => "other",
    };
    let ready = match state.requested_detail_ready {
        Some(true) => "true",
        Some(false) => "false",
        None => "unknown",
    };
    Err(Error::new("focus_precondition", format!(
        "UI precondition: mode={mode} politics={:?} focus={:?} panel={:?} tree={:?} details={} info={} selected={selected} ready={ready}",
        state.politics_root, state.focus_root, state.research_panel_root, state.research_tree_root,
        counts(state.own_detail_counts), counts(state.own_research_info_counts)
    )))
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
    pub(super) fn target(stage: Stage) -> Target {
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
    pub(super) fn state() -> OwnState {
        OwnState {
            epoch: epoch(),
            active_focus: ActiveFocus::Idle,
            canonical_focus_id: Some("GER_rhineland".into()),
            managed_mode: Some(23),
            politics_root: LocalRoot::Hidden,
            focus_root: LocalRoot::Hidden,
            own_detail_counts: Some([0, 0]),
            own_research_info_counts: Some([0, 0]),
            research_panel_root: LocalRoot::Unknown,
            research_tree_root: LocalRoot::Unknown,
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
    fn handoff_research() -> (Entry, crate::research_stage::OwnState) {
        (
            Entry {
                id: "research-head".into(),
                intent: Intent::Research {
                    technology_id: "support_weapons".into(),
                    slot: 0,
                },
                navigation: None,
            },
            crate::research_stage::OwnState {
                epoch: epoch(),
                selected_slot: None,
                selected_folder: None,
                tree_visible: false,
                panel_visible: Some(false),
                active_mode: Some(12),
                researching: vec![None; 4],
                visible_info: vec![],
            },
        )
    }
    fn active_politics() -> OwnState {
        let mut s = state();
        s.active_focus = ActiveFocus::Active("GER_rhineland".into());
        s.managed_mode = Some(12);
        s.politics_root = LocalRoot::Ready;
        s
    }
    #[test]
    fn handoff_scheduling_skips_occupied_slots_missing_permission_and_pending_retries() {
        let (e, r) = handoff_research();
        assert!(needs_research_handoff(true, false, &e, &r));
        assert!(!needs_research_handoff(false, false, &e, &r));
        assert!(!needs_research_handoff(true, true, &e, &r));
        assert!(!needs_research_handoff(true, false, &entry(), &r));
        for current in ["support_weapons", "different_technology"] {
            let mut own = r.clone();
            own.researching[0] = Some(current.into());
            assert!(!needs_research_handoff(true, false, &e, &own));
        }
        for mode in [None, Some(5), Some(6), Some(13), Some(23)] {
            let mut own = r.clone();
            own.active_mode = mode;
            assert!(!needs_research_handoff(true, false, &e, &own));
        }
        let mut own = r;
        own.researching.clear();
        assert!(!needs_research_handoff(true, false, &e, &own));
    }
    #[test]
    fn handoff_hint_needs_exact_current_focus_and_two_consistent_own_ui_states() {
        let (entry_r, research) = handoff_research();
        let own = active_politics();
        assert_eq!(
            research_handoff_target(&entry(), &entry_r, &own, &research).unwrap(),
            target(Stage::ClosePolitics)
        );
        for index in 0..10 {
            let mut s = own.clone();
            match index {
                0 => s.active_focus = ActiveFocus::Idle,
                1 => s.active_focus = ActiveFocus::Unknown,
                2 => s.active_focus = ActiveFocus::Active("GER_other".into()),
                3 => s.canonical_focus_id = Some("GER_other".into()),
                4 => s.managed_mode = Some(23),
                5 => s.politics_root = LocalRoot::Hidden,
                6 => s.focus_root = LocalRoot::Ready,
                7 => s.own_detail_counts = None,
                8 => s.own_detail_counts = Some([0, 1]),
                9 => s.own_research_info_counts = Some([1, 0]),
                _ => unreachable!(),
            }
            assert!(research_handoff_target(&entry(), &entry_r, &s, &research).is_err());
        }
        for index in 0..8 {
            let mut s = research.clone();
            match index {
                0 => s.epoch.campaign = "other".into(),
                1 => s.active_mode = Some(23),
                2 => s.panel_visible = Some(true),
                3 => s.panel_visible = None,
                4 => s.tree_visible = true,
                5 => s.visible_info.push(None),
                6 => s.researching[0] = Some("other_technology".into()),
                7 => s.researching.clear(),
                _ => unreachable!(),
            }
            assert!(research_handoff_target(&entry(), &entry_r, &own, &s).is_err());
        }
        assert!(research_handoff_target(&entry_r, &entry_r, &own, &research).is_err());
        assert!(research_handoff_target(&entry(), &entry(), &own, &research).is_err());
    }
    #[test]
    fn handoff_close_receipt_survives_restart_and_cannot_create_another_attempt() {
        let t = Scratch::new();
        let e = entry();
        let day = 1936 * 365;
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        n.check_handoff(&e, day).unwrap();
        let id = n
            .begin(e.clone(), target(Stage::ClosePolitics), day, 1, binding())
            .unwrap();
        let mut execution = receipt(id);
        execution.receipt.target = target(Stage::ClosePolitics);
        execution.receipt.observed = state();
        execution.receipt.observed.active_focus = ActiveFocus::Active("GER_rhineland".into());
        n.verified(&execution, &binding()).unwrap();
        assert!(!n.blocked());
        assert!(n.data.goals.is_empty());
        let bytes = std::fs::read(t.path()).unwrap();
        drop(n);
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        assert_eq!(n.latest()["preparation"]["stage"], "ClosePolitics");
        assert_eq!(n.latest()["preparation"]["goal_completed"], false);
        for when in [day, day + 1] {
            assert!(n.check_handoff(&e, when).is_err());
            assert!(n
                .begin(e.clone(), target(Stage::ClosePolitics), when, 2, binding())
                .is_err());
        }
        assert_eq!(std::fs::read(t.path()).unwrap(), bytes);
        assert_eq!(n.data.steps.len(), 1);
        assert!(n.data.goals.is_empty());
    }
    #[test]
    fn handoff_refusal_unresolved_input_and_changed_storage_never_gain_retry_authority() {
        for evidence in [
            None,
            Some(crate::focus_stage::FailureEvidence::BeforeIntent),
            Some(crate::focus_stage::FailureEvidence::ConfirmedNotQueued),
            Some(crate::focus_stage::FailureEvidence::Unknown),
        ] {
            let t = Scratch::new();
            let e = entry();
            let day = 1936 * 365;
            let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
            n.begin(e.clone(), target(Stage::ClosePolitics), day, 1, binding())
                .unwrap();
            if let Some(evidence) = evidence {
                n.failed(
                    &Error::new("input_not_queued", "generated refusal"),
                    evidence,
                )
                .unwrap();
            }
            drop(n);
            let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
            assert!(n.check_handoff(&e, day).is_err());
            assert!(n.check_handoff(&e, day + 1).is_err());
            assert_eq!(n.data.steps.len(), 1);
        }
        let t = Scratch::new();
        let e = entry();
        let day = 1936 * 365;
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        std::fs::write(t.path(), "external change").unwrap();
        assert!(n.check_handoff(&e, day).is_err());
        assert!(n.blocked());
    }
    #[test]
    fn research_return_selection_permissions_and_durable_preparation() {
        let mut own = state();
        own.managed_mode = Some(5);
        assert!(next(&own, &target(Stage::Start)).is_err());
        own.research_panel_root = LocalRoot::Ready;
        own.research_tree_root = LocalRoot::Hidden;
        assert!(matches!(
            next(&own, &target(Stage::Start)).unwrap(),
            Next::Stage(Stage::OpenPoliticsFromResearch)
        ));
        assert_eq!(
            preparation_permission(Stage::OpenPoliticsFromResearch, false)
                .unwrap_err()
                .code,
            "capability_unavailable"
        );
        preparation_permission(Stage::OpenPoliticsFromResearch, true).unwrap();
        preparation_permission(Stage::OpenPolitics, false).unwrap();
        let t = Scratch::new();
        let e = entry();
        let day = 1936 * 365;
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        let id = n
            .begin(
                e.clone(),
                target(Stage::OpenPoliticsFromResearch),
                day,
                1,
                binding(),
            )
            .unwrap();
        let mut execution = receipt(id);
        execution.receipt.target = target(Stage::OpenPoliticsFromResearch);
        execution.receipt.observed.research_panel_root = LocalRoot::Hidden;
        execution.receipt.observed.research_tree_root = LocalRoot::Hidden;
        n.verified(&execution, &binding()).unwrap();
        assert!(n.data.goals.is_empty());
        drop(n);
        let n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        assert!(!n.blocked());
        assert_eq!(
            n.latest()["preparation"]["stage"],
            "OpenPoliticsFromResearch"
        );
        assert_eq!(n.latest()["preparation"]["goal_completed"], false);
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
    fn navigation_refusal_preserves_exact_facts_without_keys_across_restart() {
        let mut own = state();
        own.managed_mode = Some(5);
        own.research_panel_root = LocalRoot::Unknown;
        own.research_tree_root = LocalRoot::Hidden;
        own.selected_focus = Some("PRIVATE_SELECTED_KEY".into());
        let error = next(&own, &target(Stage::Start)).err().unwrap();
        assert_eq!(error.code, "focus_precondition");
        assert_eq!(error.message, "UI precondition: mode=5 politics=Hidden focus=Hidden panel=Unknown tree=Hidden details=0/0 info=0/0 selected=other ready=false");
        assert!(!error.message.contains("PRIVATE_SELECTED_KEY"));
        assert_eq!(
            crate::daily_navigation::bounded_reason(&error.message),
            error.message
        );
        let t = Scratch::new();
        let e = entry();
        let day = 1936 * 365;
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        n.refuse_read_only(&e, day, &error).unwrap();
        assert!(n.data.steps.is_empty() && n.data.goals.is_empty());
        drop(n);
        let n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        assert_eq!(n.latest()["refusal"]["reason"], error.message);
        assert!(n.refused_today(&e, day) && !n.blocked());
    }
    #[test]
    fn navigation_refusal_worst_case_facts_fit_the_existing_durable_bound() {
        let mut own = state();
        own.managed_mode = None;
        own.politics_root = LocalRoot::Animating;
        own.focus_root = LocalRoot::Animating;
        own.research_panel_root = LocalRoot::Animating;
        own.research_tree_root = LocalRoot::Animating;
        own.own_detail_counts = None;
        own.own_research_info_counts = None;
        own.selected_focus = Some(target(Stage::Start).focus_id);
        own.requested_detail_ready = None;
        let error = next(&own, &target(Stage::Start)).err().unwrap();
        assert_eq!(error.message, "UI precondition: mode=unknown politics=Animating focus=Animating panel=Animating tree=Animating details=unknown info=unknown selected=target ready=unknown");
        assert!(error.message.len() <= 160 && error.message.is_ascii());
        assert_eq!(
            crate::daily_navigation::bounded_reason(&error.message),
            error.message
        );
    }
    #[test]
    fn geometry_boundary_survives_durable_before_intent_failure_and_restart() {
        // Generated boundary, not a reconstruction of run054's lost numbers.
        let render = serde_json::json!({"render_type_encoding":"20CFrameAnimatedSprite",
            "field_layout_verified":true,"resource_layout_verified":true,"resource_alias_matches":true,
            "resource_type_encoding":"24CFrameAnimatedSpriteType",
            "resource_cache":{"getter_cache_equivalent":true},
            "frame_raw_geometry":{"no_mask_rejection_branch":true,"parent_chain":{
                "graphics_root_closed":false,"status":"unavailable_policy_bound","payload_hops":2,
                "policy_boundary":{"stage":"ancestor_membership","policy_scope":"own_focus_position_ancestor",
                    "count":9001,"capacity":10000,"capacity_limit":8192,"inspected_entries_before_attempt":8,"aggregate_entry_limit":16384}}}});
        let error = crate::research_candidate_observation::control_surface(&render).unwrap_err();
        assert_eq!(error.message,"Root closure policy bound: stage=ancestor scope=focus_position count=9001 cap=10000 cap_max=8192 prior=8 sum_max=16384 hops=2");
        let t = Scratch::new();
        let e = entry();
        let day = 1936 * 365;
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        n.begin(e.clone(), target(Stage::View), day, 1, binding())
            .unwrap();
        n.failed(&error, FailureEvidence::BeforeIntent).unwrap();
        assert!(n.data.goals.is_empty() && !n.blocked());
        drop(n);
        let n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        assert_eq!(n.latest()["failure"]["reason"], error.message);
        assert_eq!(
            n.latest()["failure"]["code"],
            "research_geometry_unavailable"
        );
        assert_eq!(n.latest()["failure"]["historical"], true);
        assert!(n.data.steps[0].receipt.is_none() && n.data.goals.is_empty());
        assert!(!n.blocked());
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
    fn retry_attempt() -> crate::daily_governor::Attempt {
        crate::daily_governor::Attempt {
            request_id: "a".repeat(64),
            entry: entry(),
            scope: crate::daily_governor::Scope {
                epoch: epoch(),
                anchors: [0x10000; 3],
                own_country: 0x20000,
                own_status: 0x30000,
            },
            day: 1936 * 365,
            before_sequence: 5,
        }
    }
    #[test]
    fn start_failure_history_retains_original_cause_and_never_authorizes_reopen_retry() {
        let t = Scratch::new();
        let e = entry();
        let a = retry_attempt();
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        for index in 1..=3 {
            let err = Error::new("torn_read", format!("original own witness {index}"));
            let retry = retry_before_intent(&err, FailureEvidence::BeforeIntent, index);
            assert_eq!(retry, index < 3);
            n.record_start_failure(&a, 1, &err, FailureEvidence::BeforeIntent, index, retry)
                .unwrap();
        }
        assert_eq!(
            n.latest()["start_failure"]["original_reason"],
            "original own witness 1"
        );
        assert_eq!(n.latest()["start_failure"]["failures"], 3);
        assert_eq!(n.latest()["start_failure"]["retry_authority"], false);
        assert!(n
            .record_start_failure(
                &a,
                1,
                &Error::new("torn_read", "fourth"),
                FailureEvidence::BeforeIntent,
                3,
                true
            )
            .is_err());
        drop(n);
        let n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        assert_eq!(
            n.latest()["start_failure"]["original_reason"],
            "original own witness 1"
        );
        drop(n);
        let mut d: Data = serde_json::from_slice(&std::fs::read(t.path()).unwrap()).unwrap();
        d.start_failures[1].attempt.day += 1;
        std::fs::write(t.path(), serde_json::to_vec(&d).unwrap()).unwrap();
        assert!(Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).is_err());
    }
    #[test]
    fn only_beforeintent_torn_retries_unknown_is_durable_block_and_savefailure_poison() {
        for code in [
            "input_busy",
            "ownership_changed",
            "focus_deadline",
            "focus_precondition",
            "action_uncertain",
        ] {
            assert!(!retry_before_intent(
                &Error::new(code, "refuse"),
                FailureEvidence::BeforeIntent,
                1
            ));
        }
        for proof in [
            FailureEvidence::ConfirmedNotQueued,
            FailureEvidence::Unknown,
        ] {
            assert!(!retry_before_intent(
                &Error::new("torn_read", "refuse"),
                proof,
                1
            ));
        }
        let t = Scratch::new();
        let e = entry();
        let a = retry_attempt();
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        n.record_start_failure(
            &a,
            1,
            &Error::new("torn_read", "possible input"),
            FailureEvidence::Unknown,
            1,
            false,
        )
        .unwrap();
        assert!(n.blocked());
        drop(n);
        assert!(
            Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e))
                .unwrap()
                .blocked()
        );
        let t = Scratch::new();
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        std::fs::write(t.path(), b"external mutation").unwrap();
        assert!(n
            .record_start_failure(
                &a,
                1,
                &Error::new("torn_read", "original"),
                FailureEvidence::BeforeIntent,
                1,
                true
            )
            .is_err());
        assert!(n.blocked());
    }
    #[test]
    fn two_inflight_preintent_failures_complete_one_original_core_budget_reservation() {
        use crate::daily_governor as c;
        struct B {
            obs: c::Observation,
            n: Navigator,
            sends: usize,
            failures: u8,
            exhaust: bool,
        }
        impl c::Backend for B {
            fn unresolved_input(&mut self) -> Result<bool> {
                Ok(self.n.blocked())
            }
            fn observe(&mut self) -> Result<c::Observation> {
                self.obs.sequence += 1;
                Ok(self.obs.clone())
            }
            fn readiness(&mut self, _: &Entry, _: &c::Observation) -> Result<c::Readiness> {
                Ok(c::Readiness::Ready)
            }
            fn attempt(
                &mut self,
                a: &c::Attempt,
                _: &c::Observation,
                _: std::time::Instant,
            ) -> Result<c::Outcome> {
                for index in 1..=if self.exhaust { 3 } else { 2 } {
                    self.obs.sequence += 1;
                    let e = Error::new("torn_read", format!("own witness {index}"));
                    let retry = retry_before_intent(&e, FailureEvidence::BeforeIntent, index);
                    self.n.record_start_failure(
                        a,
                        1,
                        &e,
                        FailureEvidence::BeforeIntent,
                        index,
                        retry,
                    )?;
                    self.failures += 1;
                    if !retry {
                        return Ok(c::Outcome::NotQueued(e.code.into()));
                    }
                }
                self.sends += 1;
                self.obs.sequence += 1;
                Ok(c::Outcome::Verified(Box::new(c::VerifiedReceipt {
                    attempt: a.clone(),
                    current: true,
                    canonical_intent: a.entry.intent.clone(),
                    observed: self.obs.clone(),
                })))
            }
        }
        for exhaust in [false, true] {
            let t = Scratch::new();
            let a = retry_attempt();
            let e = a.entry.clone();
            let p = c::Plan {
                version: 1,
                revision: 1,
                scope: a.scope.clone(),
                entries: vec![e.clone()],
                max_actions_per_day: 1,
                max_observation_age_ms: 5000,
                review_day: None,
            };
            let mut g = c::Governor::open(&t.0.join("plan"), p.clone()).unwrap();
            let n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
            let mut b = B {
                obs: c::Observation {
                    scope: a.scope,
                    date_raw: crate::calendar::encode(1936, 1, 1).unwrap(),
                    sequence: 1,
                    observed_unix_ms: 9999,
                    source: "generated fresh owned state".into(),
                    counts: Default::default(),
                },
                n,
                sends: 0,
                failures: 0,
                exhaust,
            };
            let result = g
                .run_day(
                    &mut b,
                    10000,
                    std::time::Instant::now() + std::time::Duration::from_secs(2),
                )
                .unwrap();
            assert_eq!(g.used_actions(1936 * 365), 1);
            assert_eq!(b.sends, usize::from(!exhaust));
            assert_eq!(result.verified.len(), usize::from(!exhaust));
            let original = std::fs::read(t.0.join("plan")).unwrap();
            drop(g);
            let mut g = c::Governor::open(&t.0.join("plan"), p).unwrap();
            assert_eq!(g.used_actions(1936 * 365), 1);
            if exhaust {
                assert!(g.pending_focus(1936 * 365).is_none());
            }
            b.obs.sequence += 10;
            g.continue_entry(
                &mut b,
                &e.id,
                10000,
                std::time::Instant::now() + std::time::Duration::from_secs(1),
            )
            .unwrap();
            assert_eq!(b.sends, usize::from(!exhaust));
            let before: serde_json::Value = serde_json::from_slice(&original).unwrap();
            let after: serde_json::Value =
                serde_json::from_slice(&std::fs::read(t.0.join("plan")).unwrap()).unwrap();
            assert_eq!(before["journal"], after["journal"]);
        }
    }
}
