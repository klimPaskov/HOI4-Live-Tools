//! Private prerequisite for durable daily routine. No process reader, pause,
//! engine callback, input transport, MCP command or strategic chooser lives here.
#[cfg(unix)]
mod store;
#[cfg(not(unix))]
#[path = "daily_governor/store_unsupported.rs"]
mod store;
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
fn request_id(plan: &Plan, entry: &Entry, number: u64) -> Result<String> {
    let encoded = serde_json::to_vec(&(&plan.scope, plan.revision, entry, number))
        .map_err(|e| types::invalid(&e.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(encoded)))
}
impl Governor {
    pub fn open(path: &Path, plan: Plan) -> Result<Self> {
        plan.validate()?;
        let (store, bytes) = store::Store::open(path)?;
        let state = if let Some(bytes) = bytes {
            let state: State = serde_json::from_slice(&bytes)
                .map_err(|e| Error::new("invalid_plan_state", e.to_string()))?;
            state.plan.validate()?;
            if state.version != 1 || state.plan != plan || state.entries.len() != plan.entries.len()
            {
                return Err(Error::new(
                    "plan_conflict",
                    "Persisted plan/version/scope differs; no reset or new-token rescue",
                ));
            }
            if state.journal.len() > 4096
                || state.quarantine.as_ref().is_some_and(|s| s.len() > 640)
            {
                return Err(types::invalid("Journal/quarantine exceeds bound"));
            }
            let mut intents = BTreeMap::new();
            let mut latest = BTreeMap::new();
            let mut counts: BTreeMap<&String, u64> = BTreeMap::new();
            let mut seen = std::collections::BTreeSet::new();
            for record in &state.journal {
                if record.request_id.len() != 64
                    || !record.request_id.bytes().all(|b| b.is_ascii_hexdigit())
                    || !state.entries.contains_key(&record.entry_id)
                    || record.before_sequence == 0
                    || !(1936 * 365..2001 * 365).contains(&record.day)
                {
                    return Err(types::invalid("Invalid journal record binding"));
                }
                if record.terminal.is_none() {
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
                        .unwrap();
                    if !seen.insert(&record.request_id)
                        || record.request_id != request_id(&plan, entry, *number)?
                    {
                        return Err(types::invalid("Reused or nonmatching request identity"));
                    }
                    if record.after_sequence.is_some()
                        || intents.insert(&record.request_id, record).is_some()
                    {
                        return Err(types::invalid("Duplicate or malformed intent journal"));
                    }
                } else {
                    let previous = intents
                        .remove(&record.request_id)
                        .ok_or_else(|| types::invalid("Terminal journal lacks unique intent"))?;
                    if previous.entry_id != record.entry_id
                        || previous.day != record.day
                        || previous.before_sequence != record.before_sequence
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
                        if reason.as_ref().is_some_and(|s| s.len() > 640)
                            || reason.is_some() != reason_day.is_some()
                            || reason_day
                                .is_some_and(|d| state.last_day.is_none_or(|last| d > last))
                        {
                            return Err(types::invalid("Reason exceeds bound"));
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
                    observed.validate(&plan, observed.observed_unix_ms)?;
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
            state
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
    pub fn entry_state(&self, id: &str) -> Option<&EntryState> {
        self.state.entries.get(id)
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
        if advanced {
            let entries = self.state.plan.entries.clone();
            let mut attempts = 0;
            for entry in entries {
                let count = match self.state.entries.get(&entry.id) {
                    Some(EntryState::Pending { attempts, .. }) => *attempts,
                    _ => continue,
                };
                if attempts >= self.state.plan.max_actions_per_day || Instant::now() >= deadline {
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
                    request_id: request_id(&self.state.plan, &entry, number)?,
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
        let mut out = json!({"date_raw":o.date_raw,"observed_unix_ms":o.observed_unix_ms,"age_ms":now.saturating_sub(o.observed_unix_ms),"source":o.source,"country":o.scope.epoch.player,"plan_revision":self.state.plan.revision,"counts":o.counts,"counts_scope":"observed own categories; null is unknown","review_due":self.state.plan.review_day.is_some_and(|d|self.state.last_day.is_some_and(|day|day>=d)),"verified":[],"retained":[],"blocking_decision":blocking,"input_authorized":false});
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
        let total = self
            .state
            .entries
            .values()
            .filter(|s| !matches!(s, EntryState::Complete { .. }))
            .count();
        out["retained_count"] = json!(total);
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
