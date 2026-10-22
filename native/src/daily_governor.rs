//! Private prerequisite for durable daily routine. No process reader, pause,
//! engine callback, input transport, MCP command or strategic chooser lives here.
mod production;
#[cfg(unix)]
pub(crate) mod store;
#[cfg(not(unix))]
#[path = "daily_governor/store_unsupported.rs"]
pub(crate) mod store;
mod types;
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Condvar, Mutex},
    time::{Duration, Instant},
};
pub use types::*;

/// Owner integration must implement this only by verified ordinary adapters,
/// checking shared lease/gates, process/epoch/build and final scoped witnesses.
/// `observe` must emit a fresh sequence across restart, never cached payloads.
/// Readiness is read-only; Unknown prerequisites map to Deferred, absent verified
/// adapters to Unavailable. attempt must recheck actual ordinary preconditions
/// under the shared input gate/lease immediately before transport.
/// NotQueued is allowed only for positively known zero-input delivery; every
/// other transport error/outcome is uncertain. This trait is a trusted owner
/// boundary, not a JSON receipt parser or a strategy/authority constructor.
/// `unresolved_input` must check ALL shared input journals, not only this plan.
pub trait Backend {
    fn unresolved_input(&mut self) -> Result<bool>;
    fn observe(&mut self) -> Result<Observation>;
    fn readiness(&mut self, entry: &Entry, observed: &Observation) -> Result<Readiness>;
    fn attempt(
        &mut self,
        attempt: &Attempt,
        observed: &Observation,
        deadline: Instant,
    ) -> Result<Outcome>;
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", deny_unknown_fields)]
pub enum EntryState {
    Pending {
        attempts: u64,
        reason: Option<String>,
        reason_day: Option<i64>,
    },
    Intent {
        attempt: Attempt,
    },
    Complete {
        attempt: Attempt,
        observed: Box<Observation>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct JournalRecord {
    request_id: String,
    entry_id: String,
    day: i64,
    before_sequence: u64,
    terminal: Option<bool>,
    after_sequence: Option<u64>,
    // Retain the original zero-input refusal after Pending becomes Complete.
    // Absent on older records: never reconstruct a historical cause.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    not_queued_reason: Option<String>,
    // Legacy records bind to the then-immutable plan revision. Updates pin it
    // before changing the plan so historical request identities stay valid.
    #[serde(default)]
    plan_revision: Option<u64>,
    #[serde(default)]
    day_bound: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u8,
    plan: Plan,
    entries: BTreeMap<String, EntryState>,
    last_date: Option<i32>,
    last_day: Option<i64>,
    last_sequence: Option<u64>,
    quarantine: Option<String>,
    journal: Vec<JournalRecord>,
}
pub struct Governor {
    store: store::Store,
    state: State,
    poisoned: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct DailyResult {
    pub advanced: bool,
    pub verified: Vec<String>,
    pub retained: Vec<String>,
    pub blocking_decision: Option<String>,
    pub digest: Value,
}
fn reason(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(160).collect()
}
const RESEARCH_SLOT_WAIT: &str = "Own research slot occupied; awaiting fresh empty-slot evidence";
const FOCUS_WAIT: &str = "Own different active focus; awaiting fresh idle state";
fn request_id_day(
    plan: &Plan,
    entry: &Entry,
    number: u64,
    revision: u64,
    day: i64,
) -> Result<String> {
    let encoded = serde_json::to_vec(&(
        "daily-day-bound-v1",
        &plan.scope,
        revision,
        entry,
        number,
        day,
    ))
    .map_err(|_| types::invalid("Request encoding failed"))?;
    Ok(format!("{:x}", Sha256::digest(encoded)))
}
fn request_id_revision(plan: &Plan, entry: &Entry, number: u64, revision: u64) -> Result<String> {
    let encoded = serde_json::to_vec(&(&plan.scope, revision, entry, number))
        .map_err(|e| types::invalid(&e.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(encoded)))
}
impl State {
    fn decode(bytes: &[u8], expected: Option<&Plan>) -> Result<Self> {
        let state: State = serde_json::from_slice(bytes)
            .map_err(|e| Error::new("invalid_plan_state", e.to_string()))?;
        let plan = &state.plan;
        plan.validate()?;
        if state.version != 1
            || expected.is_some_and(|expected| &state.plan != expected)
            || state.entries.len() != plan.entries.len()
        {
            return Err(Error::new(
                "plan_conflict",
                "Persisted plan/version/scope differs; no reset or new-token rescue",
            ));
        }
        if state.journal.len() > 4096 || state.quarantine.as_ref().is_some_and(|s| s.len() > 640) {
            return Err(types::invalid("Journal/quarantine exceeds bound"));
        }
        let mut intents = BTreeMap::new();
        let mut latest = BTreeMap::new();
        let mut counts: BTreeMap<&String, u64> = BTreeMap::new();
        let mut seen = std::collections::BTreeSet::new();
        let mut previous_day = None;
        let mut previous_sequence = 0;
        for record in &state.journal {
            let revision = record.plan_revision.unwrap_or(plan.revision);
            if record.request_id.len() != 64
                || !record.request_id.bytes().all(|b| b.is_ascii_hexdigit())
                || !state.entries.contains_key(&record.entry_id)
                || record.before_sequence == 0
                || !(1936 * 365..2001 * 365).contains(&record.day)
                || revision == 0
                || revision > plan.revision
                || record.not_queued_reason.as_ref().is_some_and(|s| {
                    record.terminal != Some(false)
                        || s.len() > 640
                        || s.chars().count() > 160
                        || s.chars().any(char::is_control)
                })
            {
                return Err(types::invalid("Invalid journal record binding"));
            }
            if record.terminal.is_none() {
                if previous_day.is_some_and(|d| record.day < d)
                    || record.before_sequence <= previous_sequence
                {
                    return Err(types::invalid("Journal chronology regressed"));
                }
                previous_day = Some(record.day);
                previous_sequence = record.before_sequence;
                if !intents.is_empty()
                    || latest
                        .get(&record.entry_id)
                        .is_some_and(|r: &&JournalRecord| r.terminal == Some(true))
                {
                    return Err(types::invalid(
                        "Input intent overlaps unresolved or completed work",
                    ));
                }
                let number = counts.entry(&record.entry_id).or_default();
                *number += 1;
                let entry = plan
                    .entries
                    .iter()
                    .find(|e| e.id == record.entry_id)
                    .ok_or_else(|| types::invalid("Journal entry absent from plan"))?;
                if !seen.insert(&record.request_id)
                    || record.request_id
                        != if record.day_bound {
                            request_id_day(plan, entry, *number, revision, record.day)?
                        } else {
                            request_id_revision(plan, entry, *number, revision)?
                        }
                {
                    return Err(types::invalid("Reused or nonmatching request identity"));
                }
                if record.after_sequence.is_some()
                    || intents.insert(&record.request_id, record).is_some()
                {
                    return Err(types::invalid("Duplicate or malformed intent journal"));
                }
            } else {
                if let Some(after) = record.after_sequence {
                    previous_sequence = after;
                }
                let previous = intents
                    .remove(&record.request_id)
                    .ok_or_else(|| types::invalid("Terminal journal lacks unique intent"))?;
                if previous.entry_id != record.entry_id
                    || previous.day != record.day
                    || previous.before_sequence != record.before_sequence
                    || previous.day_bound != record.day_bound
                    || previous.plan_revision.unwrap_or(plan.revision) != revision
                    || (record.terminal == Some(true)
                        && !record
                            .after_sequence
                            .is_some_and(|n| n > record.before_sequence))
                    || (record.terminal == Some(false) && record.after_sequence.is_some())
                {
                    return Err(types::invalid("Journal transition differs"));
                }
            }
            latest.insert(&record.entry_id, record);
        }
        for e in &plan.entries {
            match state
                .entries
                .get(&e.id)
                .ok_or_else(|| types::invalid("Missing durable entry"))?
            {
                EntryState::Pending {
                    reason, reason_day, ..
                } => {
                    if reason.as_ref().is_some_and(|s| {
                        s.len() > 640 || s.chars().count() > 160 || s.chars().any(char::is_control)
                    }) {
                        return Err(types::invalid("Invalid bounded pending reason"));
                    }
                    match (reason.as_deref(), reason_day) {
                        (None, None) => {}
                        (Some(_), Some(day))
                            if (1936 * 365..2001 * 365).contains(day)
                                && state.last_day.is_some_and(|last| *day <= last) => {}
                        (Some(message), None)
                            if state.last_day.is_some()
                                && matches!(
                                    (&e.intent, message),
                                    (Intent::Research { .. }, RESEARCH_SLOT_WAIT)
                                        | (Intent::Focus { .. }, FOCUS_WAIT)
                                ) => {}
                        _ => {
                            return Err(types::invalid(
                                "Pending refusal/wait lacks matching saved chronology",
                            ))
                        }
                    }
                }
                EntryState::Intent { attempt } | EntryState::Complete { attempt, .. } => {
                    if attempt.entry != *e
                        || attempt.scope != plan.scope
                        || attempt.request_id.len() != 64
                        || !attempt.request_id.bytes().all(|b| b.is_ascii_hexdigit())
                        || attempt.before_sequence == 0
                    {
                        return Err(types::invalid("Durable attempt binding differs"));
                    }
                }
            }
            let saved = state.entries.get(&e.id).unwrap();
            match (saved, latest.get(&e.id)) {
                (EntryState::Pending { attempts, .. }, None) if *attempts == 0 => {}
                (EntryState::Pending { attempts, .. }, Some(r))
                    if r.terminal == Some(false) && counts.get(&e.id) == Some(attempts) => {}
                (EntryState::Intent { attempt }, Some(r))
                    if r.request_id == attempt.request_id
                        && r.day == attempt.day
                        && r.before_sequence == attempt.before_sequence
                        && r.terminal.is_none() => {}
                (EntryState::Complete { attempt, observed }, Some(r))
                    if r.request_id == attempt.request_id
                        && r.day == attempt.day
                        && r.before_sequence == attempt.before_sequence
                        && r.terminal == Some(true)
                        && r.after_sequence == Some(observed.sequence) => {}
                _ => return Err(types::invalid("Journal and durable entry differ")),
            }
            if let Some(EntryState::Complete { attempt, observed }) = state.entries.get(&e.id) {
                observed.validate(plan, observed.observed_unix_ms)?;
                if observed.sequence <= attempt.before_sequence
                    || (i64::from(observed.date_raw) - 43_800_000).div_euclid(24) < attempt.day
                {
                    return Err(types::invalid("Completed receipt chronology invalid"));
                }
            }
        }
        if !state.journal.is_empty() && state.last_date.is_none() {
            return Err(types::invalid("Journal lacks saved chronology"));
        }
        if previous_day.is_some_and(|day| state.last_day.is_none_or(|last| day > last))
            || state
                .last_sequence
                .is_some_and(|last| previous_sequence > last)
        {
            return Err(types::invalid("Journal exceeds saved chronology"));
        }
        for saved in state.entries.values() {
            match saved {
                EntryState::Complete { attempt, observed }
                    if state.last_sequence.is_none_or(|n| n < observed.sequence)
                        || state.last_date.is_none_or(|d| d < observed.date_raw)
                        || state.last_day.is_none_or(|d| d < attempt.day) =>
                {
                    return Err(types::invalid(
                        "Completed evidence exceeds saved chronology",
                    ))
                }
                EntryState::Intent { attempt }
                    if state
                        .last_sequence
                        .is_none_or(|n| n < attempt.before_sequence)
                        || state.last_day.is_none_or(|d| d < attempt.day) =>
                {
                    return Err(types::invalid("Intent exceeds saved chronology"))
                }
                _ => {}
            }
        }
        if state.last_date.is_some() != state.last_day.is_some()
            || state.last_sequence.is_some() != state.last_day.is_some()
        {
            return Err(types::invalid("Incomplete chronology state"));
        }
        if let Some(raw) = state.last_date {
            let day = (i64::from(raw) - 43_800_000).div_euclid(24);
            crate::calendar::decode(raw)?;
            if state.last_day != Some(day) || state.last_sequence == Some(0) {
                return Err(types::invalid("Invalid saved chronology"));
            }
        }
        Ok(state)
    }
}

impl Governor {
    /// Load the original private plan before any owner input is enabled.
    /// Existing bytes, quarantine and unresolved intents are never rewritten.
    /// A present result retains the same exclusive Store lock as `open`.
    #[cfg(any(target_os = "macos", all(test, unix)))]
    pub(crate) fn load_existing(path: &Path) -> Result<Option<Self>> {
        let (store, bytes) = store::Store::open(path)?;
        let Some(bytes) = bytes else {
            return Ok(None);
        };
        let state = State::decode(&bytes, None)?;
        store.check()?;
        Ok(Some(Self {
            store,
            state,
            poisoned: false,
        }))
    }
    pub fn open(path: &Path, plan: Plan) -> Result<Self> {
        plan.validate()?;
        let (store, bytes) = store::Store::open(path)?;
        let state = if let Some(bytes) = bytes {
            State::decode(&bytes, Some(&plan))?
        } else {
            State {
                version: 1,
                entries: plan
                    .entries
                    .iter()
                    .map(|e| {
                        (
                            e.id.clone(),
                            EntryState::Pending {
                                attempts: 0,
                                reason: None,
                                reason_day: None,
                            },
                        )
                    })
                    .collect(),
                plan,
                last_date: None,
                last_day: None,
                last_sequence: None,
                quarantine: None,
                journal: Vec::new(),
            }
        };
        let mut g = Self {
            store,
            state,
            poisoned: false,
        };
        if g.state
            .entries
            .values()
            .any(|e| matches!(e, EntryState::Intent { .. }))
        {
            g.state.quarantine = Some("Unresolved durable input intent after restart".into());
        }
        g.persist()?;
        Ok(g)
    }
    fn persist(&mut self) -> Result<()> {
        let bytes = serde_json::to_vec(&self.state)
            .map_err(|e| Error::new("invalid_plan_state", e.to_string()))?;
        if let Err(e) = self.store.replace(&bytes) {
            self.poisoned = true;
            return Err(e);
        }
        Ok(())
    }
    fn check(&mut self) -> Result<()> {
        if self.poisoned {
            return Err(Error::new(
                "governor_quarantined",
                "Persistence uncertainty blocks all work",
            ));
        }
        if let Err(e) = self.store.check() {
            self.poisoned = true;
            return Err(e);
        }
        if self.state.quarantine.is_some()
            || self
                .state
                .entries
                .values()
                .any(|e| matches!(e, EntryState::Intent { .. }))
        {
            return Err(Error::new(
                "governor_quarantined",
                "Unresolved possible input blocks all work and restart",
            ));
        }
        Ok(())
    }
    fn quarantine(&mut self, message: &str) -> Result<()> {
        self.state.quarantine = Some(reason(message));
        self.persist()
    }
    fn validate_observation(&self, o: &Observation, now: u64) -> Result<i64> {
        let day = o.validate(&self.state.plan, now)?;
        if self.state.last_date.is_some_and(|raw| o.date_raw < raw) {
            return Err(Error::new(
                "date_regressed",
                "Observed date moved backwards",
            ));
        }
        if self.state.last_sequence.is_some_and(|s| o.sequence <= s) {
            return Err(Error::new(
                "stale_observation",
                "Observation sequence did not advance",
            ));
        }
        Ok(day)
    }
    #[cfg(any(target_os = "linux", target_os = "macos", all(test, unix)))]
    pub(crate) fn blocks_work(&self) -> bool {
        self.poisoned
            || self.state.quarantine.is_some()
            || self
                .state
                .entries
                .values()
                .any(|e| matches!(e, EntryState::Intent { .. }))
    }
    #[cfg(any(target_os = "linux", target_os = "macos", all(test, unix)))]
    pub(crate) fn last_sequence(&self) -> u64 {
        self.state.last_sequence.unwrap_or(0)
    }
    #[cfg(any(target_os = "linux", target_os = "macos", all(test, unix)))]
    pub(crate) fn plan(&self) -> Plan {
        self.state.plan.clone()
    }
    #[cfg(any(target_os = "linux", target_os = "macos", all(test, unix)))]
    pub(crate) fn preparation_check(&mut self) -> Result<()> {
        self.check()
    }
    #[cfg(target_os = "macos")]
    pub(crate) fn verify_live_scope(&mut self, scope: &Scope, date_raw: i32) -> Result<()> {
        self.check()?;
        crate::calendar::decode(date_raw)?;
        if self.state.plan.scope != *scope
            || self.state.last_date.is_some_and(|date| date_raw < date)
        {
            self.poisoned = true;
            return Err(Error::new(
                "ownership_changed",
                "Retained plan scope or chronology differs from the live campaign",
            ));
        }
        Ok(())
    }
    /// Historical key hint only. A completed assignment proves a past Start,
    /// not the current active focus or panel state; the owner must join both
    /// from fresh scoped observations before any navigation.
    #[cfg(any(target_os = "linux", target_os = "macos", all(test, unix)))]
    pub(crate) fn completed_focus_hint(&mut self) -> Result<Option<Entry>> {
        self.check()?;
        Ok(self
            .state
            .plan
            .entries
            .iter()
            .filter_map(|entry| match self.state.entries.get(&entry.id) {
                Some(EntryState::Complete { observed, .. })
                    if matches!(entry.intent, Intent::Focus { .. }) =>
                {
                    Some((observed.sequence, entry))
                }
                _ => None,
            })
            .max_by_key(|(sequence, _)| *sequence)
            .map(|(_, entry)| entry.clone()))
    }
    #[cfg(any(target_os = "linux", target_os = "macos", all(test, unix)))]
    pub(crate) fn pending_research(&self, day: i64) -> Vec<Entry> {
        let mut slots = std::collections::BTreeSet::new();
        self.state
            .plan
            .entries
            .iter()
            .filter(|entry| {
                let Intent::Research { slot, .. } = &entry.intent else {
                    return false;
                };
                let Some(EntryState::Pending { reason_day, .. }) =
                    self.state.entries.get(&entry.id)
                else {
                    return false;
                };
                // A refused/deferred head still blocks later entries for this slot.
                slots.insert(*slot) && *reason_day != Some(day)
            })
            .cloned()
            .collect()
    }
    #[cfg(any(target_os = "linux", target_os = "macos", all(test, unix)))]
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) fn pending_focus(&self, day: i64) -> Option<Entry> {
        let head = self.state.plan.entries.iter().find(|e| {
            matches!(e.intent, Intent::Focus { .. })
                && !matches!(
                    self.state.entries.get(&e.id),
                    Some(EntryState::Complete { .. })
                )
        })?;
        match self.state.entries.get(&head.id) {
            Some(EntryState::Pending { reason_day, .. }) if *reason_day != Some(day) => {
                Some(head.clone())
            }
            _ => None,
        }
    }
    /// Pending production heads are ordered independently for each exact line.
    /// A refused head still blocks later targets for that line; unrelated lines
    /// may proceed. This is scheduling only, never input/completion authority.
    pub fn pending_production(&self, day: i64) -> Vec<Entry> {
        self.state
            .plan
            .entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.intent,
                    Intent::Economy {
                        target: EconomyTarget::Production { .. }
                    }
                ) && self.queue_head(entry)
                    && matches!(self.state.entries.get(&entry.id),
                    Some(EntryState::Pending { reason_day, .. }) if *reason_day != Some(day))
            })
            .cloned()
            .collect()
    }
    fn queue_head(&self, entry: &Entry) -> bool {
        if let Intent::Economy {
            target: EconomyTarget::Production { line_id, .. },
        } = &entry.intent
        {
            return self
                .state
                .plan
                .entries
                .iter()
                .find(|candidate| {
                    matches!(&candidate.intent,
                    Intent::Economy { target: EconomyTarget::Production { line_id: other, .. } }
                        if other == line_id)
                        && !matches!(
                            self.state.entries.get(&candidate.id),
                            Some(EntryState::Complete { .. })
                        )
                })
                .is_some_and(|head| head.id == entry.id);
        }
        let Intent::Research { slot, .. } = &entry.intent else {
            if matches!(entry.intent, Intent::Focus { .. }) {
                return self
                    .state
                    .plan
                    .entries
                    .iter()
                    .find(|e| {
                        matches!(e.intent, Intent::Focus { .. })
                            && !matches!(
                                self.state.entries.get(&e.id),
                                Some(EntryState::Complete { .. })
                            )
                    })
                    .is_some_and(|head| head.id == entry.id);
            }
            return true;
        };
        self.state.plan.entries.iter().find(|candidate| {
            matches!(&candidate.intent, Intent::Research { slot: candidate_slot, .. } if candidate_slot == slot)
                && !matches!(self.state.entries.get(&candidate.id), Some(EntryState::Complete { .. }))
        }).is_some_and(|head| head.id == entry.id)
    }
    /// Count persisted unique intent reservations, including NotQueued and unresolved.
    pub(crate) fn used_actions(&self, day: i64) -> usize {
        self.state
            .journal
            .iter()
            .filter(|r| r.terminal.is_none() && r.day == day)
            .count()
    }
    #[cfg(any(target_os = "linux", target_os = "macos", all(test, unix)))]
    pub(crate) fn budget_available(&self, day: i64) -> bool {
        !self
            .state
            .journal
            .iter()
            .any(|r| r.day == day && !r.day_bound)
            && self.used_actions(day) < usize::from(self.state.plan.max_actions_per_day)
    }
    pub fn entry_state(&self, id: &str) -> Option<&EntryState> {
        self.state.entries.get(id)
    }
    /// Durable assignment history, independent of the latest batch and current
    /// game state. Caller holds the governor lock; this never authorizes input.
    #[cfg(any(target_os = "linux", target_os = "macos", all(test, unix)))]
    pub(crate) fn assignment_summary(&mut self) -> Result<Value> {
        self.check_history()?;
        self.assignment_history()
    }
    /// STOP has no fresh game observation. Derive its budget from the same
    /// checked durable state as assignments, never from a worker report cache.
    #[cfg(any(target_os = "linux", target_os = "macos", all(test, unix)))]
    pub(crate) fn stopped_history(&mut self) -> Result<(Value, Value)> {
        self.check_history()?;
        let day = self.state.last_day;
        let used = day.map(|day| self.used_actions(day));
        let remaining = day.map(|day| {
            if self.budget_available(day) {
                usize::from(self.state.plan.max_actions_per_day)
                    .saturating_sub(self.used_actions(day))
            } else {
                0
            }
        });
        let budget = json!({"available":true,"source":"durable_plan",
            "historical":true,"current_state_verified":false,"input_authorized":false,
            "plan_revision":self.state.plan.revision,"date_raw":self.state.last_date,
            "observed_sequence":self.state.last_sequence,"day":day,
            "used_assignment_attempts":used,"limit":self.state.plan.max_actions_per_day,
            "remaining":remaining,"navigation_spends_assignment_budget":false});
        Ok((self.assignment_history()?, budget))
    }
    #[cfg(any(target_os = "linux", target_os = "macos", all(test, unix)))]
    fn check_history(&mut self) -> Result<()> {
        if self.poisoned {
            return Err(Error::new(
                "governor_quarantined",
                "Daily persistence is unverified",
            ));
        }
        if let Err(error) = self.store.check() {
            self.poisoned = true;
            return Err(error);
        }
        Ok(())
    }
    #[cfg(any(target_os = "linux", target_os = "macos", all(test, unix)))]
    fn assignment_history(&self) -> Result<Value> {
        let mut complete = Vec::new();
        let mut pending = 0;
        let mut in_flight = 0;
        for entry in &self.state.plan.entries {
            match self.state.entries.get(&entry.id) {
                Some(EntryState::Complete { attempt, observed }) => {
                    complete.push((entry, attempt, observed));
                }
                Some(EntryState::Pending { .. }) => pending += 1,
                Some(EntryState::Intent { .. }) => in_flight += 1,
                None => return Err(Error::new("invalid_plan_state", "Assignment state missing")),
            }
        }
        complete.sort_by(|a, b| {
            b.2.sequence
                .cmp(&a.2.sequence)
                .then_with(|| a.0.id.cmp(&b.0.id))
        });
        let latest = complete.first().map(|(entry, attempt, observed)| {
            json!({"entry_id":entry.id,"state":"Complete","request_id":attempt.request_id,
                "observed_sequence":observed.sequence,"date_raw":observed.date_raw})
        });
        let ids: Vec<_> = complete
            .iter()
            .take(4)
            .map(|(entry, _, _)| entry.id.as_str())
            .collect();
        let refusals: Vec<_> = self
            .state
            .journal
            .iter()
            .filter(|record| record.terminal == Some(false))
            .collect();
        let latest_refusal = refusals.last().map(|record| {
            json!({"entry_id":record.entry_id,"request_id":record.request_id,
                "day":record.day,"before_sequence":record.before_sequence,
                "plan_revision":record.plan_revision.unwrap_or(self.state.plan.revision),
                "reason":record.not_queued_reason})
        });
        Ok(
            json!({"available":true,"source":"durable_plan","plan_revision":self.state.plan.revision,
            "historical":true,"current_state_verified":false,"input_authorized":false,
            "total":self.state.plan.entries.len(),"completed":complete.len(),"pending":pending,"in_flight":in_flight,
            "completed_ids":ids,"completed_ids_truncated":complete.len()>ids.len(),"latest_completion":latest,
            "not_queued_attempts":refusals.len(),"latest_not_queued":latest_refusal}),
        )
    }
    /// Trusted owner-only plan update. The caller checks the shared controller,
    /// lease, fresh scope and ALL input journals before exposing this through MCP.
    /// Existing work/specifications and evidence cannot be removed or replaced;
    /// additions, ordering and bounded policy limits may change. This performs
    /// no input and does not reset a consumed day, sequence or quarantine.
    /// Returns false for an exact idempotent retry of the current plan.
    pub fn update_plan(&mut self, next: Plan) -> Result<bool> {
        self.check()?;
        next.validate()?;
        if next == self.state.plan {
            return Ok(false);
        }
        if next.scope != self.state.plan.scope {
            return Err(Error::new("ownership_changed", "Plan update scope differs"));
        }
        if next.revision <= self.state.plan.revision {
            return Err(Error::new("plan_conflict", "Plan revision must advance"));
        }
        for old in &self.state.plan.entries {
            if !next.entries.iter().any(|entry| entry == old) {
                return Err(Error::new(
                    "plan_conflict",
                    "Plan update must retain every existing entry and its specification",
                ));
            }
        }
        // Existing per-slot order is immutable; additions append to that slot.
        // Other domains and relative ordering across different slots may move.
        let mut slots = std::collections::BTreeSet::new();
        for old in &self.state.plan.entries {
            if let Intent::Research { slot, .. } = old.intent {
                slots.insert(slot);
            }
        }
        for slot in slots {
            let old: Vec<_> = self
                .state
                .plan
                .entries
                .iter()
                .filter(|e| matches!(e.intent, Intent::Research { slot: s, .. } if s == slot))
                .collect();
            let updated: Vec<_> = next
                .entries
                .iter()
                .filter(|e| matches!(e.intent, Intent::Research { slot: s, .. } if s == slot))
                .collect();
            if !updated.starts_with(&old) {
                return Err(Error::new(
                    "plan_conflict",
                    "Existing research slot queue order must remain an unchanged prefix",
                ));
            }
        }
        let old_focus: Vec<_> = self
            .state
            .plan
            .entries
            .iter()
            .filter(|e| matches!(e.intent, Intent::Focus { .. }))
            .collect();
        let next_focus: Vec<_> = next
            .entries
            .iter()
            .filter(|e| matches!(e.intent, Intent::Focus { .. }))
            .collect();
        if !next_focus.starts_with(&old_focus) {
            return Err(Error::new(
                "plan_conflict",
                "Existing focus queue order must remain an unchanged prefix",
            ));
        }
        let previous_revision = self.state.plan.revision;
        let lines: std::collections::BTreeSet<_> = self
            .state
            .plan
            .entries
            .iter()
            .filter_map(|e| match &e.intent {
                Intent::Economy {
                    target: EconomyTarget::Production { line_id, .. },
                } => Some(line_id),
                _ => None,
            })
            .collect();
        for line in lines {
            let belongs = |entry: &&Entry| {
                matches!(&entry.intent,
                Intent::Economy { target: EconomyTarget::Production { line_id, .. } } if line_id == line)
            };
            let old: Vec<_> = self.state.plan.entries.iter().filter(belongs).collect();
            let updated: Vec<_> = next.entries.iter().filter(belongs).collect();
            if !updated.starts_with(&old) {
                return Err(Error::new(
                    "plan_conflict",
                    "Existing production line queue order must remain an unchanged prefix",
                ));
            }
        }
        for record in &mut self.state.journal {
            record.plan_revision.get_or_insert(previous_revision);
        }
        for entry in &next.entries {
            self.state
                .entries
                .entry(entry.id.clone())
                .or_insert(EntryState::Pending {
                    attempts: 0,
                    reason: None,
                    reason_day: None,
                });
        }
        self.state.plan = next;
        self.persist()?;
        Ok(true)
    }
    /// One bounded routine batch per newly observed day. A clock notification
    /// should call this; callers must NOT manufacture daily model polling.
    /// `now_unix_ms` comes from the owner clock, never from snapshot timestamps.
    pub fn run_day(
        &mut self,
        backend: &mut dyn Backend,
        now_unix_ms: u64,
        deadline: Instant,
    ) -> Result<DailyResult> {
        self.run_batch(backend, now_unix_ms, deadline, false, None)
    }
    #[cfg(any(target_os = "linux", target_os = "macos", all(test, unix)))]
    pub(crate) fn continue_entry(
        &mut self,
        backend: &mut dyn Backend,
        entry_id: &str,
        now_unix_ms: u64,
        deadline: Instant,
    ) -> Result<DailyResult> {
        if !self.state.plan.entries.iter().any(|e| e.id == entry_id) {
            return Err(types::invalid("Unknown continuation entry"));
        }
        self.run_batch(backend, now_unix_ms, deadline, true, Some(entry_id))
    }
    fn run_batch(
        &mut self,
        backend: &mut dyn Backend,
        now_unix_ms: u64,
        deadline: Instant,
        continuation: bool,
        only_entry: Option<&str>,
    ) -> Result<DailyResult> {
        let started = Instant::now();
        let now = || {
            now_unix_ms
                .saturating_add(started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64)
        };
        self.check()?;
        if backend.unresolved_input()? {
            self.quarantine("Shared input journal has unresolved possible input")?;
            return Err(Error::new(
                "governor_quarantined",
                "Shared unresolved input blocks all work",
            ));
        }
        let mut observed = backend.observe()?;
        let day = self.validate_observation(&observed, now())?;
        let advanced = self.state.last_day != Some(day);
        self.state.last_date = Some(observed.date_raw);
        self.state.last_sequence = Some(observed.sequence);
        if advanced {
            self.state.last_day = Some(day);
        }
        // Persist day consumption before any attempt. Crash without an intent
        // retains every unfinished entry for a later advancing day.
        self.persist()?;
        let mut verified = Vec::new();
        let mut retained = Vec::new();
        let mut blocking = None;
        if advanced || continuation {
            let entries = self.state.plan.entries.clone();
            let mut attempts = if self
                .state
                .journal
                .iter()
                .any(|r| r.day == day && !r.day_bound)
            {
                usize::from(self.state.plan.max_actions_per_day)
            } else {
                self.used_actions(day)
            };
            for entry in entries {
                if !self.queue_head(&entry) {
                    retained.push(entry.id.clone());
                    continue;
                }
                if only_entry.is_some_and(|id| id != entry.id) {
                    continue;
                }
                if continuation
                    && matches!(self.state.entries.get(&entry.id),Some(EntryState::Pending {reason_day,..}) if *reason_day==Some(day))
                {
                    continue;
                }
                let count = match self.state.entries.get(&entry.id) {
                    Some(EntryState::Pending { attempts, .. }) => *attempts,
                    _ => continue,
                };
                if attempts >= usize::from(self.state.plan.max_actions_per_day)
                    || Instant::now() >= deadline
                {
                    retained.push(entry.id.clone());
                    continue;
                }
                // Recheck shared uncertainty and reacquire a fresh complete own
                // observation before each entry; never reuse a prior action's UI.
                if backend.unresolved_input()? {
                    self.quarantine("Shared input became uncertain")?;
                    return Err(Error::new(
                        "governor_quarantined",
                        "Shared unresolved input",
                    ));
                }
                observed = backend.observe()?;
                let entry_day = self.validate_observation(&observed, now())?;
                if entry_day != day {
                    break;
                }
                self.state.last_date = Some(observed.date_raw);
                self.state.last_sequence = Some(observed.sequence);
                let ready = backend.readiness(&entry, &observed)?;
                match ready {
                    Readiness::WaitingForResearchSlot => {
                        if !matches!(entry.intent, Intent::Research { .. }) {
                            return Err(types::invalid(
                                "Research slot wait belongs only to Research",
                            ));
                        }
                        self.state.entries.insert(
                            entry.id.clone(),
                            EntryState::Pending {
                                attempts: count,
                                reason: Some(RESEARCH_SLOT_WAIT.into()),
                                reason_day: None,
                            },
                        );
                        retained.push(entry.id);
                        self.persist()?;
                        continue;
                    }
                    Readiness::WaitingForFocus => {
                        if !matches!(entry.intent, Intent::Focus { .. }) {
                            return Err(types::invalid("Focus wait belongs only to Focus"));
                        }
                        self.state.entries.insert(
                            entry.id.clone(),
                            EntryState::Pending {
                                attempts: count,
                                reason: Some(FOCUS_WAIT.into()),
                                reason_day: None,
                            },
                        );
                        retained.push(entry.id);
                        self.persist()?;
                        continue;
                    }
                    Readiness::Deferred(s) | Readiness::Unavailable(s) => {
                        self.state.entries.insert(
                            entry.id.clone(),
                            EntryState::Pending {
                                attempts: count,
                                reason: Some(reason(&s)),
                                reason_day: Some(day),
                            },
                        );
                        retained.push(entry.id);
                        self.persist()?;
                        continue;
                    }
                    Readiness::BlockingDecision(s) => {
                        blocking = Some(reason(&s));
                        self.state.entries.insert(
                            entry.id.clone(),
                            EntryState::Pending {
                                attempts: count,
                                reason: blocking.clone(),
                                reason_day: Some(day),
                            },
                        );
                        self.persist()?;
                        break;
                    }
                    Readiness::Ready => {}
                }
                observed = backend.observe()?;
                let fresh_day = self.validate_observation(&observed, now())?;
                if fresh_day != day {
                    break;
                }
                self.state.last_date = Some(observed.date_raw);
                self.state.last_sequence = Some(observed.sequence);
                self.check()?;
                if Instant::now() >= deadline {
                    retained.push(entry.id);
                    break;
                }
                if self.state.journal.len() > 4094 {
                    return Err(Error::new(
                        "journal_full",
                        "Bounded journal full; retain work without input",
                    ));
                }
                if backend.unresolved_input()? {
                    self.quarantine("Shared input became uncertain before intent")?;
                    return Err(Error::new(
                        "governor_quarantined",
                        "Shared unresolved input",
                    ));
                }
                let number = count
                    .checked_add(1)
                    .ok_or_else(|| types::invalid("Attempt count exhausted"))?;
                let attempt = Attempt {
                    request_id: request_id_day(
                        &self.state.plan,
                        &entry,
                        number,
                        self.state.plan.revision,
                        day,
                    )?,
                    entry: entry.clone(),
                    scope: self.state.plan.scope.clone(),
                    day,
                    before_sequence: observed.sequence,
                };
                self.state.journal.push(JournalRecord {
                    request_id: attempt.request_id.clone(),
                    entry_id: entry.id.clone(),
                    day,
                    before_sequence: attempt.before_sequence,
                    terminal: None,
                    after_sequence: None,
                    not_queued_reason: None,
                    plan_revision: Some(self.state.plan.revision),
                    day_bound: true,
                });
                self.state.entries.insert(
                    entry.id.clone(),
                    EntryState::Intent {
                        attempt: attempt.clone(),
                    },
                );
                self.persist()?;
                attempts += 1;
                // From here, errors/panics leave a durable unresolved intent.
                let action_deadline = deadline.min(Instant::now() + Duration::from_secs(5));
                let outcome = match backend.attempt(&attempt, &observed, action_deadline) {
                    Ok(v) => v,
                    Err(e) => {
                        self.quarantine(&format!("Adapter outcome uncertain: {}", e.code))?;
                        return Err(e);
                    }
                };
                self.store.check().inspect_err(|_| self.poisoned = true)?;
                match backend.unresolved_input() {
                    Ok(false) => {}
                    Ok(true) => {
                        self.quarantine("Shared unresolved input after adapter return")?;
                        return Err(Error::new(
                            "action_uncertain",
                            "Shared uncertainty blocks receipt consumption",
                        ));
                    }
                    Err(e) => {
                        self.quarantine(
                            "Shared input journal cannot be verified after adapter return",
                        )?;
                        return Err(e);
                    }
                }
                match outcome {
                    Outcome::NotQueued(s) => {
                        self.state.journal.push(JournalRecord {
                            request_id: attempt.request_id.clone(),
                            entry_id: entry.id.clone(),
                            day,
                            before_sequence: attempt.before_sequence,
                            terminal: Some(false),
                            after_sequence: None,
                            not_queued_reason: Some(reason(&s)),
                            plan_revision: Some(self.state.plan.revision),
                            day_bound: true,
                        });
                        self.state.entries.insert(
                            entry.id.clone(),
                            EntryState::Pending {
                                attempts: number,
                                reason: Some(reason(&s)),
                                reason_day: Some(day),
                            },
                        );
                        retained.push(entry.id);
                        self.persist()?;
                    }
                    Outcome::Uncertain(s) => {
                        self.quarantine(&s)?;
                        return Err(Error::new(
                            "action_uncertain",
                            "Possible input was not verified; all routine work quarantined",
                        ));
                    }
                    Outcome::Verified(receipt) => {
                        let valid = receipt.current
                            && receipt.attempt == attempt
                            && receipt.canonical_intent == entry.intent
                            && receipt.observed.scope == attempt.scope
                            && receipt.observed.sequence > attempt.before_sequence
                            && receipt.observed.date_raw >= observed.date_raw
                            && receipt.observed.observed_unix_ms >= observed.observed_unix_ms
                            && receipt.observed.validate(&self.state.plan, now()).is_ok()
                            && Instant::now() < action_deadline;
                        if !valid {
                            self.quarantine("Nonmatching/stale/late canonical receipt")?;
                            return Err(Error::new(
                                "action_uncertain",
                                "Current matching canonical receipt required",
                            ));
                        }
                        observed = receipt.observed;
                        self.state.journal.push(JournalRecord {
                            request_id: attempt.request_id.clone(),
                            entry_id: entry.id.clone(),
                            day,
                            before_sequence: attempt.before_sequence,
                            terminal: Some(true),
                            after_sequence: Some(observed.sequence),
                            not_queued_reason: None,
                            plan_revision: Some(self.state.plan.revision),
                            day_bound: true,
                        });
                        self.state.last_date = Some(observed.date_raw);
                        self.state.last_sequence = Some(observed.sequence);
                        self.state.last_day =
                            Some((i64::from(observed.date_raw) - 43_800_000).div_euclid(24));
                        self.state.entries.insert(
                            entry.id.clone(),
                            EntryState::Complete {
                                attempt,
                                observed: Box::new(observed.clone()),
                            },
                        );
                        self.persist()?;
                        verified.push(entry.id);
                    }
                }
            }
        }
        let digest = self.digest(&observed, now(), &verified, blocking.as_deref())?;
        Ok(DailyResult {
            advanced,
            verified,
            retained,
            blocking_decision: blocking,
            digest,
        })
    }
    fn digest(
        &self,
        o: &Observation,
        now: u64,
        verified: &[String],
        blocking: Option<&str>,
    ) -> Result<Value> {
        let total = self
            .state
            .entries
            .values()
            .filter(|s| !matches!(s, EntryState::Complete { .. }))
            .count();
        let mut out = json!({"date_raw":o.date_raw,"observed_unix_ms":o.observed_unix_ms,"age_ms":now.saturating_sub(o.observed_unix_ms),"source":o.source,"country":o.scope.epoch.player,"plan_revision":self.state.plan.revision,"counts":o.counts,"counts_scope":"observed own categories; null is unknown","review_due":self.state.plan.review_day.is_some_and(|d|self.state.last_day.is_some_and(|day|day>=d)),"verified":[],"retained":[],"blocking_decision":blocking,"input_authorized":false});
        // Reserve essential counts before admitting optional IDs; otherwise a
        // full verified-ID list can leave no room for the count added later.
        out["retained_count"] = json!(total);
        for id in verified {
            let mut candidate = out.clone();
            candidate["verified"]
                .as_array_mut()
                .unwrap()
                .push(json!(id));
            if serde_json::to_vec(&candidate)
                .map_err(|e| types::invalid(&e.to_string()))?
                .len()
                <= 2048
            {
                out = candidate;
            }
        }
        for entry in &self.state.plan.entries {
            if let Some(EntryState::Pending {
                reason, reason_day, ..
            }) = self.state.entries.get(&entry.id)
            {
                let item = json!({"id":entry.id,"reason":reason,"last_checked_game_day":reason_day,"fix":{"entry_id":entry.id,"plan_revision":self.state.plan.revision,"capability":entry.intent.capability()}});
                let mut candidate = out.clone();
                candidate["retained"].as_array_mut().unwrap().push(item);
                if serde_json::to_vec(&candidate)
                    .map_err(|e| types::invalid(&e.to_string()))?
                    .len()
                    <= 2048
                {
                    out = candidate;
                }
            }
        }
        if serde_json::to_vec(&out)
            .map_err(|e| types::invalid(&e.to_string()))?
            .len()
            > 2048
        {
            return Err(types::invalid("Digest essential fields exceed bound"));
        }
        Ok(out)
    }
}
/// Event/clock notification primitive. Controller owns the worker and complete
/// snapshots. Bounded Condvar waiting never pauses the game or invokes a model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeReason {
    GameDay,
    StateChange,
    ReviewDeadline,
    Shutdown,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Notification {
    pub generation: u64,
    pub reason: Option<WakeReason>,
}
#[derive(Default)]
pub struct Wakeup {
    notification: Mutex<Notification>,
    changed: Condvar,
}
impl Wakeup {
    #[cfg(test)]
    pub(crate) fn poison_for_test(&self) {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = self.notification.lock().unwrap();
            panic!("Generated notification poison");
        }));
        self.changed.notify_all();
    }
    pub fn notify(&self, reason: WakeReason) -> Result<Notification> {
        let mut n = self
            .notification
            .lock()
            .map_err(|_| Error::new("worker_poisoned", "Notification lock poisoned"))?;
        n.generation = n
            .generation
            .checked_add(1)
            .ok_or_else(|| types::invalid("Notification counter exhausted"))?;
        n.reason = Some(reason);
        self.changed.notify_all();
        Ok(*n)
    }
    pub fn wait_after(&self, previous: u64, timeout: Duration) -> Result<Notification> {
        if timeout > Duration::from_secs(60) {
            return Err(types::invalid("Worker wait exceeds 60 seconds"));
        }
        let guard = self
            .notification
            .lock()
            .map_err(|_| Error::new("worker_poisoned", "Notification lock poisoned"))?;
        let (guard, _) = self
            .changed
            .wait_timeout_while(guard, timeout, |n| n.generation <= previous)
            .map_err(|_| Error::new("worker_poisoned", "Notification lock poisoned"))?;
        Ok(*guard)
    }
}
#[cfg(test)]
mod tests;

#[cfg(all(test, unix))]
mod owner_reopen_tests {
    use super::*;
    use crate::{actions::Epoch, platform::Identity};
    use std::{
        fs,
        os::unix::fs::{MetadataExt, PermissionsExt},
        sync::atomic::{AtomicU64, Ordering},
    };

    struct Temp(std::path::PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let directory = std::env::temp_dir().join(format!(
                "private-daily-owner-reopen-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
            Self(directory)
        }
        fn path(&self) -> std::path::PathBuf {
            self.0.join("daily-plan.json")
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn plan() -> Plan {
        Plan {
            version: 1,
            revision: 1,
            scope: Scope {
                epoch: Epoch {
                    game: Identity {
                        pid: 321,
                        start_ticks: "123.000456".into(),
                        exe: "/generated/hoi4".into(),
                        uid: 501,
                    },
                    campaign: "original-controller:generated-playthrough".into(),
                    player: "GER".into(),
                },
                anchors: [0x10000, 0x20000, 0x30000],
                own_country: 0x40000,
                own_status: 0x50000,
            },
            entries: [
                ("first", "basic_machine_tools"),
                ("refill", "construction1"),
            ]
            .into_iter()
            .map(|(id, technology)| Entry {
                id: id.into(),
                intent: Intent::Research {
                    technology_id: technology.into(),
                    slot: 0,
                },
                navigation: None,
                focus_search: None,
            })
            .collect(),
            max_actions_per_day: 2,
            max_observation_age_ms: 5000,
            review_day: None,
        }
    }

    enum AttemptResult {
        Verified,
        NotQueued,
        Uncertain,
        Crash,
    }
    struct Generated {
        observation: Observation,
        waiting: Option<String>,
        result: AttemptResult,
        sends: usize,
    }
    impl Generated {
        fn new(plan: &Plan) -> Self {
            Self {
                observation: Observation {
                    scope: plan.scope.clone(),
                    date_raw: crate::calendar::encode(1936, 1, 1).unwrap(),
                    sequence: 1,
                    observed_unix_ms: 10_000,
                    source: "generated owned daily observation".into(),
                    counts: OwnCounts::default(),
                },
                waiting: None,
                result: AttemptResult::Verified,
                sends: 0,
            }
        }
    }
    impl Backend for Generated {
        fn unresolved_input(&mut self) -> Result<bool> {
            Ok(false)
        }
        fn observe(&mut self) -> Result<Observation> {
            self.observation.sequence += 1;
            Ok(self.observation.clone())
        }
        fn readiness(&mut self, entry: &Entry, _: &Observation) -> Result<Readiness> {
            if self.waiting.as_deref() == Some(&entry.id) {
                return Ok(match entry.intent {
                    Intent::Research { .. } => Readiness::WaitingForResearchSlot,
                    Intent::Focus { .. } => Readiness::WaitingForFocus,
                    _ => panic!("Generated waiting entry must be Research or Focus"),
                });
            }
            Ok(Readiness::Ready)
        }
        fn attempt(&mut self, attempt: &Attempt, _: &Observation, _: Instant) -> Result<Outcome> {
            self.sends += 1;
            match self.result {
                AttemptResult::NotQueued => {
                    Ok(Outcome::NotQueued("generated zero-input refusal".into()))
                }
                AttemptResult::Uncertain => {
                    Ok(Outcome::Uncertain("generated unresolved input".into()))
                }
                AttemptResult::Crash => panic!("Generated crash after persisted governor intent"),
                AttemptResult::Verified => Ok(Outcome::Verified(Box::new(VerifiedReceipt {
                    attempt: attempt.clone(),
                    current: true,
                    canonical_intent: attempt.entry.intent.clone(),
                    observed: self.observe()?,
                }))),
            }
        }
    }

    fn run(governor: &mut Governor, generated: &mut Generated) -> Result<DailyResult> {
        governor.run_day(generated, 10_000, Instant::now() + Duration::from_secs(2))
    }
    fn stamp(path: &Path) -> (u64, u64, u64, i64, i64) {
        let metadata = fs::metadata(path).unwrap();
        (
            metadata.dev(),
            metadata.ino(),
            metadata.len(),
            metadata.mtime(),
            metadata.mtime_nsec(),
        )
    }

    #[test]
    fn absent_owner_load_never_bootstraps_state_or_retains_a_lock() {
        let temp = Temp::new();
        assert!(Governor::load_existing(&temp.path()).unwrap().is_none());
        assert!(!temp.path().exists());
        let governor = Governor::open(&temp.path(), plan()).unwrap();
        assert!(!governor.blocks_work());
    }

    #[test]
    fn owner_load_preserves_bytes_plan_and_lock_and_open_still_requires_exact_plan() {
        let temp = Temp::new();
        let expected = plan();
        let governor = Governor::open(&temp.path(), expected.clone()).unwrap();
        let pretty = serde_json::to_vec_pretty(&governor.state).unwrap();
        drop(governor);
        fs::write(temp.path(), &pretty).unwrap();
        let before = stamp(&temp.path());
        let mut loaded = Governor::load_existing(&temp.path()).unwrap().unwrap();
        assert_eq!(loaded.plan(), expected);
        assert_eq!(loaded.last_sequence(), 0);
        loaded.preparation_check().unwrap();
        assert_eq!(fs::read(temp.path()).unwrap(), pretty);
        assert_eq!(stamp(&temp.path()), before);
        assert_eq!(
            Governor::load_existing(&temp.path()).err().unwrap().code,
            "plan_busy"
        );
        drop(loaded);
        let mut changed = expected.clone();
        changed.scope.epoch.campaign = "replacement-controller:generated-playthrough".into();
        assert_eq!(
            Governor::open(&temp.path(), changed).err().unwrap().code,
            "plan_conflict"
        );
        assert_eq!(fs::read(temp.path()).unwrap(), pretty);
        assert_eq!(stamp(&temp.path()), before);
        Governor::open(&temp.path(), expected).unwrap();
    }

    #[test]
    fn same_slot_wait_reopens_and_refills_without_resetting_completed_work_or_day_budget() {
        let temp = Temp::new();
        let expected = plan();
        let mut generated = Generated::new(&expected);
        generated.waiting = Some("refill".into());
        let mut governor = Governor::open(&temp.path(), expected.clone()).unwrap();
        assert_eq!(
            run(&mut governor, &mut generated).unwrap().verified,
            ["first"]
        );
        assert!(matches!(
            governor.entry_state("refill"),
            Some(EntryState::Pending { reason: Some(reason), reason_day: None, attempts: 0 })
                if reason == RESEARCH_SLOT_WAIT
        ));
        let original_sequence = governor.last_sequence();
        drop(governor);
        let bytes = fs::read(temp.path()).unwrap();
        let before = stamp(&temp.path());
        let mut loaded = Governor::load_existing(&temp.path()).unwrap().unwrap();
        let day = 1936 * 365;
        assert_eq!(loaded.last_sequence(), original_sequence);
        assert_eq!(loaded.used_actions(day), 1);
        assert!(loaded.budget_available(day));
        assert_eq!(
            loaded
                .pending_research(day)
                .iter()
                .map(|e| e.id.as_str())
                .collect::<Vec<_>>(),
            ["refill"]
        );
        assert_eq!(loaded.assignment_summary().unwrap()["completed"], 1);
        assert_eq!(loaded.stopped_history().unwrap().1["remaining"], 1);
        assert_eq!(fs::read(temp.path()).unwrap(), bytes);
        assert_eq!(stamp(&temp.path()), before);
        drop(loaded);
        // The exact-plan opener uses the identical validation, including waits.
        let mut loaded = Governor::open(&temp.path(), expected).unwrap();
        generated.waiting = None;
        generated.observation.sequence = loaded.last_sequence() + 10;
        assert_eq!(
            loaded
                .continue_entry(
                    &mut generated,
                    "refill",
                    10_000,
                    Instant::now() + Duration::from_secs(2)
                )
                .unwrap()
                .verified,
            ["refill"]
        );
        assert_eq!(generated.sends, 2);
        assert_eq!(loaded.used_actions(day), 2);
        assert!(!loaded.budget_available(day));
        assert!(loaded.pending_research(day).is_empty());
        drop(loaded);
        let mut loaded = Governor::load_existing(&temp.path()).unwrap().unwrap();
        assert_eq!(loaded.assignment_summary().unwrap()["completed"], 2);
        assert!(!loaded.budget_available(day));
    }

    #[test]
    fn focus_wait_is_also_valid_undated_history_on_owner_load() {
        let temp = Temp::new();
        let mut expected = plan();
        expected.entries.truncate(1);
        expected.entries[0].intent = Intent::Focus {
            focus_id: "GER_rhineland".into(),
        };
        let mut generated = Generated::new(&expected);
        generated.waiting = Some("first".into());
        let mut governor = Governor::open(&temp.path(), expected).unwrap();
        assert!(run(&mut governor, &mut generated)
            .unwrap()
            .verified
            .is_empty());
        drop(governor);
        let loaded = Governor::load_existing(&temp.path()).unwrap().unwrap();
        assert!(!loaded.blocks_work());
        assert!(matches!(loaded.entry_state("first"),
            Some(EntryState::Pending { reason: Some(reason), reason_day: None, .. }) if reason == FOCUS_WAIT));
    }

    #[test]
    fn owner_load_keeps_unfinished_intent_and_quarantine_without_rewriting_either() {
        for crash in [false, true] {
            let temp = Temp::new();
            let expected = plan();
            let mut generated = Generated::new(&expected);
            generated.result = if crash {
                AttemptResult::Crash
            } else {
                AttemptResult::Uncertain
            };
            let mut governor = Governor::open(&temp.path(), expected).unwrap();
            if crash {
                assert!(
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(
                        &mut governor,
                        &mut generated
                    )))
                    .is_err()
                );
                assert!(governor.state.quarantine.is_none());
            } else {
                assert!(run(&mut governor, &mut generated).is_err());
                assert!(governor.state.quarantine.is_some());
            }
            drop(governor);
            let bytes = fs::read(temp.path()).unwrap();
            let before = stamp(&temp.path());
            let mut loaded = Governor::load_existing(&temp.path()).unwrap().unwrap();
            assert!(loaded.blocks_work());
            assert!(matches!(
                loaded.entry_state("first"),
                Some(EntryState::Intent { .. })
            ));
            assert_eq!(
                loaded.preparation_check().unwrap_err().code,
                "governor_quarantined"
            );
            assert_eq!(loaded.state.quarantine.is_none(), crash);
            assert_eq!(loaded.assignment_summary().unwrap()["in_flight"], 1);
            assert_eq!(fs::read(temp.path()).unwrap(), bytes);
            assert_eq!(stamp(&temp.path()), before);
        }
    }

    fn rejected_by_both_without_rewrite(state: State, expected: &Plan) {
        let temp = Temp::new();
        let bytes = serde_json::to_vec(&state).unwrap();
        fs::write(temp.path(), &bytes).unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o600)).unwrap();
        let before = stamp(&temp.path());
        assert!(Governor::load_existing(&temp.path()).is_err());
        assert!(Governor::open(&temp.path(), expected.clone()).is_err());
        assert_eq!(fs::read(temp.path()).unwrap(), bytes);
        assert_eq!(stamp(&temp.path()), before);
    }

    #[test]
    fn malformed_wait_reasons_and_saved_chronology_refuse_both_openers() {
        let temp = Temp::new();
        let expected = plan();
        let mut generated = Generated::new(&expected);
        generated.waiting = Some("refill".into());
        let mut governor = Governor::open(&temp.path(), expected.clone()).unwrap();
        run(&mut governor, &mut generated).unwrap();
        let valid = governor.state.clone();
        drop(governor);
        for alter in [
            |s: &mut State| s.version = 2,
            |s: &mut State| s.last_date = None,
            |s: &mut State| s.last_sequence = Some(0),
            |s: &mut State| {
                if let EntryState::Pending { reason, .. } = s.entries.get_mut("refill").unwrap() {
                    *reason = Some("arbitrary undated refusal".into());
                }
            },
            |s: &mut State| {
                if let EntryState::Pending { reason_day, .. } = s.entries.get_mut("refill").unwrap()
                {
                    *reason_day = s.last_day.map(|day| day + 1);
                }
            },
            |s: &mut State| {
                if let EntryState::Pending { reason_day, .. } = s.entries.get_mut("refill").unwrap()
                {
                    *reason_day = Some(1935 * 365);
                }
            },
            |s: &mut State| {
                if let EntryState::Pending {
                    reason, reason_day, ..
                } = s.entries.get_mut("refill").unwrap()
                {
                    *reason = None;
                    *reason_day = s.last_day;
                }
            },
            |s: &mut State| {
                if let EntryState::Pending {
                    reason, reason_day, ..
                } = s.entries.get_mut("refill").unwrap()
                {
                    *reason = Some("invalid\nreason".into());
                    *reason_day = s.last_day;
                }
            },
            |s: &mut State| {
                let entry = s.entries.remove("first").unwrap();
                s.entries.insert("foreign".into(), entry);
                for record in &mut s.journal {
                    if record.entry_id == "first" {
                        record.entry_id = "foreign".into();
                    }
                }
            },
        ] {
            let mut malformed = valid.clone();
            alter(&mut malformed);
            rejected_by_both_without_rewrite(malformed, &expected);
        }
        let mut wrong_domain = valid;
        wrong_domain.plan.entries[1].intent = Intent::Focus {
            focus_id: "GER_rhineland".into(),
        };
        let changed_plan = wrong_domain.plan.clone();
        rejected_by_both_without_rewrite(wrong_domain, &changed_plan);
    }

    #[test]
    fn undated_wait_without_observed_chronology_is_not_an_initial_state() {
        let temp = Temp::new();
        let expected = plan();
        let governor = Governor::open(&temp.path(), expected.clone()).unwrap();
        let mut state = governor.state.clone();
        drop(governor);
        if let EntryState::Pending { reason, .. } = state.entries.get_mut("first").unwrap() {
            *reason = Some(RESEARCH_SLOT_WAIT.into());
        }
        rejected_by_both_without_rewrite(state, &expected);
    }

    #[test]
    fn terminal_notqueued_history_cannot_exceed_saved_day_or_sequence() {
        let temp = Temp::new();
        let mut expected = plan();
        expected.entries.truncate(1);
        let mut generated = Generated::new(&expected);
        generated.result = AttemptResult::NotQueued;
        let mut governor = Governor::open(&temp.path(), expected.clone()).unwrap();
        run(&mut governor, &mut generated).unwrap();
        let valid = governor.state.clone();
        drop(governor);
        let mut earlier_day = valid.clone();
        earlier_day.journal.iter_mut().for_each(|r| r.day += 1);
        let entry = &expected.entries[0];
        let id = request_id_day(&expected, entry, 1, expected.revision, 1936 * 365 + 1).unwrap();
        earlier_day
            .journal
            .iter_mut()
            .for_each(|r| r.request_id = id.clone());
        rejected_by_both_without_rewrite(earlier_day, &expected);
        let mut earlier_sequence = valid;
        earlier_sequence.last_sequence = Some(earlier_sequence.journal[0].before_sequence - 1);
        rejected_by_both_without_rewrite(earlier_sequence, &expected);
    }

    #[test]
    fn malformed_nonprivate_and_symlinked_existing_state_never_becomes_absence() {
        let temp = Temp::new();
        fs::write(temp.path(), b"{truncated").unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(
            Governor::load_existing(&temp.path()).err().unwrap().code,
            "invalid_plan_state"
        );
        assert_eq!(fs::read(temp.path()).unwrap(), b"{truncated");
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            Governor::load_existing(&temp.path()).err().unwrap().code,
            "unsafe_plan_path"
        );
        fs::remove_file(temp.path()).unwrap();
        let real = temp.0.join("real.json");
        fs::write(&real, b"preserve").unwrap();
        std::os::unix::fs::symlink(&real, temp.path()).unwrap();
        assert_eq!(
            Governor::load_existing(&temp.path()).err().unwrap().code,
            "unsafe_plan_path"
        );
        assert_eq!(fs::read(real).unwrap(), b"preserve");
    }
}
