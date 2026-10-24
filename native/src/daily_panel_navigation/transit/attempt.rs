//! One ordinary Idle -> Research navigation, retaining a different final goal.
//! Historical facts never reconstruct a current Anchor/AtResearch. A restarted
//! controller quarantines even a recorded input-bearing success: this internal
//! stage has no qualified cross-process recovery admission yet. Stdio reconnects
//! share the original controller and do not discard its in-process admission.
use super::{AtResearch, Goal, ManagedPanel, Progress, Snapshot, Transit};
use crate::{
    daily_governor::{store::Store, Entry, Scope},
    error::{Error, Result},
    input_lease::InputAuthority,
    platform::Identity,
    research_stage::{Delivery, OwnState},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::Path,
    sync::RwLock,
    time::{Duration, Instant},
};

const KIND: &str = "owned_research_transits";
const VERSION: u8 = 1;
const MAX_RECORDS: usize = 64;
const RECORD_LIMIT: usize = 32 << 10;
const FILE_LIMIT: usize = 1 << 20;
const BUDGET: Duration = Duration::from_secs(5);

fn invalid(message: &str) -> Error {
    Error::new("invalid_transit_journal", message)
}
fn uncertain(message: &str) -> Error {
    Error::new("action_uncertain", message)
}
fn permit(gate: &RwLock<bool>) -> Result<std::sync::RwLockReadGuard<'_, bool>> {
    let p = gate
        .try_read()
        .map_err(|_| Error::new("session_closed", "Owned input gate unavailable"))?;
    if !*p {
        return Err(Error::new("session_closed", "Owned input session closed"));
    }
    Ok(p)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Request {
    pub(crate) id: String,
    pub(crate) scope: Scope,
    pub(crate) entry: Entry,
}
impl Request {
    fn validate(&self) -> Result<()> {
        if self.id.is_empty()
            || self.id.len() > 128
            || !self
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.:/".contains(&c))
        {
            return Err(invalid("Bounded stable transit request ID required"));
        }
        self.scope.validate()?;
        Goal::new(self.scope.clone(), self.entry.clone())?;
        Ok(())
    }
}

// Strict historical projection, deliberately distinct from the trusted Snapshot.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Facts {
    date_raw: i32,
    playthrough: String,
    own: OwnState,
}
impl Facts {
    fn observed(s: &Snapshot) -> Self {
        Self {
            date_raw: s.context.date_raw,
            playthrough: s.context.playthrough_id.clone(),
            own: s.research.clone(),
        }
    }
    fn validate(&self, scope: &Scope) -> Result<ManagedPanel> {
        crate::calendar::decode(self.date_raw)?;
        if self.playthrough.len() != 36
            || !scope
                .epoch
                .campaign
                .ends_with(&format!(":{}", self.playthrough))
        {
            return Err(invalid("Original transit campaign required"));
        }
        self.own.validate_navigation(&scope.epoch)?;
        let binding = self
            .own
            .panel_binding
            .as_ref()
            .ok_or_else(|| invalid("Original roots required"))?;
        if binding.anchors[..2] != [scope.own_country, scope.own_status] {
            return Err(invalid("Original transit own-country roots differ"));
        }
        let panel = ManagedPanel::from_state(&self.own)?;
        if !matches!(panel, ManagedPanel::Idle | ManagedPanel::Research) {
            return Err(invalid(
                "Ordinary transit requires settled Idle or Research",
            ));
        }
        Ok(panel)
    }
    fn follows(&self, before: &Self) -> Result<()> {
        let a = before
            .own
            .panel_binding
            .as_ref()
            .ok_or_else(|| invalid("Original roots absent"))?;
        let b = self
            .own
            .panel_binding
            .as_ref()
            .ok_or_else(|| invalid("Current roots absent"))?;
        if self.date_raw < before.date_raw
            || self.playthrough != before.playthrough
            || a.anchors != b.anchors
            || a.slots != b.slots
            || a.roots != b.roots
            || a.facilities_parent != b.facilities_parent
        {
            return Err(invalid(
                "Transit receipt changed retained roots/slots or chronology",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "status", deny_unknown_fields)]
enum Outcome {
    Intent,
    NotQueued,
    Verified {
        after: Box<Facts>,
        delivery: Option<Delivery>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Record {
    request: Request,
    before: Facts,
    outcome: Outcome,
}
impl Record {
    fn validate(&self) -> Result<()> {
        self.request.validate()?;
        let panel = self.before.validate(&self.request.scope)?;
        match &self.outcome {
            Outcome::Intent | Outcome::NotQueued if panel == ManagedPanel::Idle => {}
            Outcome::Verified { after, delivery } => {
                if after.validate(&self.request.scope)? != ManagedPanel::Research
                    || matches!(delivery, Some(Delivery::NotQueued))
                    || (delivery.is_some() && panel != ManagedPanel::Idle)
                    || (delivery.is_none() && panel != ManagedPanel::Research)
                {
                    return Err(invalid("Invalid transit terminal observation/delivery"));
                }
                after.follows(&self.before)?;
            }
            _ => return Err(invalid("Invalid transit outcome/source")),
        }
        if serde_json::to_vec(self)
            .map_err(|_| invalid("Cannot encode transit record"))?
            .len()
            > RECORD_LIMIT
        {
            return Err(invalid("Transit record exceeds bound"));
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Data {
    kind: String,
    version: u8,
    records: Vec<Record>,
}

const TRANSITION_ORIGINS: [&str; 7] = [
    "research_panel",
    "research_tree",
    "facilities_parent",
    "facilities_view",
    "research_info",
    "mode_visibility",
    "unknown",
];

// Decode only complete fixed internal messages. Unknown text stays unknown;
// diagnostic classification never affects retry/input/proof decisions.
fn transition_origin(message: &str) -> (usize, Option<[u8; 2]>) {
    if message.len() > 128 {
        return (6, None);
    }
    if message == "Research transit mode/literal visibility not settled" {
        return (5, None);
    }
    let Some(body) =
        message.strip_prefix("Closing root is in a known animation transition; transitions=")
    else {
        return (6, None);
    };
    let Some((pair, name)) = body.split_once("; root=") else {
        return (6, None);
    };
    let Some(origin) = TRANSITION_ORIGINS[..5].iter().position(|v| *v == name) else {
        return (6, None);
    };
    let pair = match pair {
        "[0, 1]" => [0, 1],
        "[0, 2]" => [0, 2],
        "[1, 0]" => [1, 0],
        "[1, 1]" => [1, 1],
        "[1, 2]" => [1, 2],
        "[2, 0]" => [2, 0],
        "[2, 1]" => [2, 1],
        "[2, 2]" => [2, 2],
        _ => return (6, None),
    };
    (origin, Some(pair))
}

/// Rejected samples and timing are historical diagnostics, never a current proof.
/// No reader error text, raw pointer, request goal or journal bytes are retained.
#[derive(Default)]
struct ReadbackDiagnostic {
    started: u32,
    pending_idle: u32,
    shown_research: u32,
    transition_errors: u32,
    torn_errors: u32,
    last_retryable: Option<&'static str>,
    transition_origins: [u32; 7],
    last_transition: Option<(usize, Option<[u8; 2]>)>,
    first_pending_idle_ms: Option<u128>,
    first_transition_ms: Option<u128>,
}
impl ReadbackDiagnostic {
    fn sample_started(&mut self) {
        self.started = self.started.saturating_add(1);
    }
    fn result(&mut self, result: &Result<Progress>, elapsed: Duration) {
        match result {
            Ok(Progress::Pending(ManagedPanel::Idle)) => {
                self.pending_idle = self.pending_idle.saturating_add(1);
                self.first_pending_idle_ms
                    .get_or_insert(elapsed.as_millis());
            }
            Ok(Progress::AtResearch(_)) => {
                self.shown_research = self.shown_research.saturating_add(1)
            }
            Err(error) if error.code == "research_panel_transition" => {
                self.transition_errors = self.transition_errors.saturating_add(1);
                self.last_retryable = Some("research_panel_transition");
                let transition = transition_origin(&error.message);
                self.transition_origins[transition.0] =
                    self.transition_origins[transition.0].saturating_add(1);
                self.last_transition = Some(transition);
                self.first_transition_ms.get_or_insert(elapsed.as_millis());
            }
            Err(error) if error.code == "torn_read" => {
                self.torn_errors = self.torn_errors.saturating_add(1);
                self.last_retryable = Some("torn_read");
            }
            _ => {}
        }
    }
    fn report(
        &self,
        dispatch: Option<(Duration, Duration, Duration)>,
        readback_elapsed: Duration,
    ) -> serde_json::Value {
        let origins: serde_json::Map<String, serde_json::Value> = TRANSITION_ORIGINS
            .iter()
            .zip(self.transition_origins)
            .map(|(name, count)| ((*name).into(), serde_json::json!(count)))
            .collect();
        serde_json::json!({
            "historical_diagnostics_only":true,
            "readback_started":self.started,
            "pending_idle":self.pending_idle,
            "shown_research":self.shown_research,
            "transition_errors":self.transition_errors,
            "torn_errors":self.torn_errors,
            "last_retryable_code":self.last_retryable,
            "transition_origins":origins,
            "last_transition":self.last_transition.map(|(origin,pair)|
                serde_json::json!({"origin":TRANSITION_ORIGINS[origin],"pair":pair})),
            // Result-return timestamps relative to attempt start, not engine
            // transition onset, dispatch processing or a settled root state.
            "first_pending_idle_ms":self.first_pending_idle_ms,
            "first_transition_ms":self.first_transition_ms,
            "pre_dispatch_ms":dispatch.map(|v|v.0.as_millis()),
            "remaining_at_dispatch_ms":dispatch.map(|v|v.1.as_millis()),
            "dispatch_ms":dispatch.map(|v|v.2.as_millis()),
            "readback_ms":readback_elapsed.as_millis()
        })
    }
}

pub(crate) struct Journal {
    store: Store,
    data: Data,
    poisoned: bool,
    // Never serialized. Only successful persistence + the original live bound
    // admit subsequent actions in this existing controller. A late write/crash
    // cannot create a restart permission from a historical Verified record.
    admitted: BTreeSet<String>,
}
#[derive(Debug)]
pub(crate) enum Execution {
    Current {
        proof: Box<AtResearch>,
        delivery: Option<Delivery>,
    },
    Historical(Box<Record>),
}
pub(crate) trait Backend {
    fn begin(&mut self, scope: &Scope, until: Instant) -> Result<Snapshot>;
    /// Uses the same original typed Anchor established by begin.
    fn observe(&mut self, until: Instant) -> Result<Snapshot>;
    fn preflight(&mut self, until: Instant) -> Result<()>;
    /// Positive NotQueued is permitted only before any input may be enqueued.
    /// The backend reruns its original-anchor predicate at the queue boundary.
    fn dispatch_once(&mut self, until: Instant) -> Delivery;
    fn confirm_input_idle(&mut self, until: Instant) -> Result<()>;
    fn wait_readback(&mut self, delay: Duration) {
        std::thread::sleep(delay);
    }
}
impl Journal {
    pub(crate) fn open(path: &Path) -> Result<Self> {
        let (store, bytes) = Store::open(path)?;
        let data: Data = match bytes {
            Some(bytes) => {
                serde_json::from_slice(&bytes).map_err(|_| invalid("Malformed transit journal"))?
            }
            None => Data {
                kind: KIND.into(),
                version: VERSION,
                records: Vec::new(),
            },
        };
        if data.kind != KIND || data.version != VERSION || data.records.len() > MAX_RECORDS {
            return Err(invalid("Unknown transit format/version or capacity"));
        }
        let mut ids = BTreeSet::new();
        for record in &data.records {
            record.validate()?;
            if !ids.insert(&record.request.id) {
                return Err(invalid("Duplicate transit request ID"));
            }
        }
        Ok(Self {
            store,
            data,
            poisoned: false,
            admitted: BTreeSet::new(),
        })
    }
    pub(crate) fn blocks_game(&self, game: &Identity) -> bool {
        self.poisoned
            || self.store.check().is_err()
            || self.data.records.iter().any(|r| {
                &r.request.scope.epoch.game == game
                    && match r.outcome {
                        Outcome::Intent => true,
                        Outcome::NotQueued | Outcome::Verified { delivery: None, .. } => false,
                        Outcome::Verified {
                            delivery: Some(_), ..
                        } => !self.admitted.contains(&r.request.id),
                    }
            })
    }
    fn checked(&mut self) -> Result<()> {
        if self.poisoned {
            return Err(uncertain("Transit storage is quarantined"));
        }
        if let Err(error) = self.store.check() {
            self.poisoned = true;
            return Err(error);
        }
        Ok(())
    }
    fn persist(&mut self, record: Record) -> Result<()> {
        record.validate()?;
        self.checked()?;
        let mut data = self.data.clone();
        if let Some(old) = data
            .records
            .iter_mut()
            .find(|r| r.request.id == record.request.id)
        {
            if old.request != record.request
                || old.before != record.before
                || !matches!(old.outcome, Outcome::Intent)
            {
                return Err(invalid(
                    "Transit cannot replace a terminal/different original attempt",
                ));
            }
            *old = record;
        } else {
            if data.records.len() >= MAX_RECORDS {
                return Err(invalid("Transit capacity exhausted"));
            }
            data.records.push(record);
        }
        let bytes =
            serde_json::to_vec(&data).map_err(|_| invalid("Cannot encode transit journal"))?;
        // Reserve a complete bounded terminal before an input-bearing Intent.
        let reserve = if matches!(
            data.records.last().map(|r| &r.outcome),
            Some(Outcome::Intent)
        ) {
            RECORD_LIMIT
        } else {
            0
        };
        if bytes.len().saturating_add(reserve) > FILE_LIMIT {
            return Err(invalid("Transit durable capacity exhausted"));
        }
        self.poisoned = true;
        self.store.replace(&bytes)?;
        self.data = data;
        self.poisoned = false;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn execute(
        &mut self,
        request: Request,
        token: &str,
        authority: &InputAuthority,
        gate: &RwLock<bool>,
        backend: &mut dyn Backend,
        caller_until: Instant,
    ) -> Result<Execution> {
        self.execute_with_clock(
            request,
            token,
            authority,
            gate,
            backend,
            caller_until,
            &Instant::now,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn execute_with_clock(
        &mut self,
        request: Request,
        token: &str,
        authority: &InputAuthority,
        gate: &RwLock<bool>,
        backend: &mut dyn Backend,
        caller_until: Instant,
        now: &dyn Fn() -> Instant,
    ) -> Result<Execution> {
        request.validate()?;
        self.checked()?;
        if let Some(old) = self
            .data
            .records
            .iter()
            .find(|r| r.request.id == request.id)
        {
            if old.request != request {
                return Err(Error::new(
                    "request_id_conflict",
                    "Transit ID retains its exact original scope/goal",
                ));
            }
            return match old.outcome {
                Outcome::Verified { .. } => Ok(Execution::Historical(Box::new(old.clone()))),
                Outcome::NotQueued => Err(Error::new(
                    "input_not_queued",
                    "Historical transit refusal; no replay",
                )),
                Outcome::Intent => Err(uncertain(
                    "Unresolved transit; no replay or late settlement",
                )),
            };
        }
        if self.blocks_game(&request.scope.epoch.game) {
            return Err(uncertain("Original-game transit quarantine blocks new IDs"));
        }
        if self.data.records.len() >= MAX_RECORDS {
            return Err(invalid(
                "Transit capacity exhausted before observation/input",
            ));
        }
        let started = now();
        let until = caller_until
            .min(started + BUDGET)
            .min(authority.authorize(token, &request.scope.epoch)?);
        let live = || -> Result<()> {
            authority.authorize(token, &request.scope.epoch)?;
            drop(permit(gate)?);
            if now() >= until {
                return Err(super::deadline());
            }
            Ok(())
        };
        live()?;
        let initial = backend.begin(&request.scope, until)?;
        live()?;
        let before = Facts::observed(&initial);
        if !matches!(
            before.validate(&request.scope)?,
            ManagedPanel::Idle | ManagedPanel::Research
        ) {
            return Err(invalid("Ordinary navigation source unsupported"));
        }
        let mut transit = Transit::bind_at(request.entry.clone(), initial, until, now())?;
        backend.preflight(until)?;
        live()?;
        let current = transit.observe_with(
            &request.scope,
            &request.entry,
            |_, _| backend.observe(until),
            now,
        )?;
        live()?;
        let mut dispatch_timing = None;
        let (record_before, delivery) = match current {
            Progress::AtResearch(shown) => (Facts::observed(shown.observed()), None),
            Progress::Pending(ManagedPanel::Idle) => {
                self.persist(Record {
                    request: request.clone(),
                    before: before.clone(),
                    outcome: Outcome::Intent,
                })?;
                live()?;
                let _permit = permit(gate)?;
                let dispatch_started = now();
                let delivery = backend.dispatch_once(until);
                dispatch_timing = Some((
                    dispatch_started.saturating_duration_since(started),
                    until.saturating_duration_since(dispatch_started),
                    now().saturating_duration_since(dispatch_started),
                ));
                drop(_permit);
                if delivery == Delivery::NotQueued {
                    // Known local refusal is safe to persist even after expiry.
                    self.persist(Record {
                        request: request.clone(),
                        before,
                        outcome: Outcome::NotQueued,
                    })?;
                    return Err(Error::new(
                        "input_not_queued",
                        "Ordinary transit positively refused before input",
                    ));
                }
                (before, Some(delivery))
            }
            _ => return Err(invalid("Original transit departure required")),
        };
        let readback_started = now();
        let mut diagnostic = ReadbackDiagnostic::default();
        let effect = (|| -> Result<Box<AtResearch>> {
            loop {
                live()?;
                diagnostic.sample_started();
                let reached = transit.observe_with(
                    &request.scope,
                    &request.entry,
                    |_, _| backend.observe(until),
                    now,
                );
                diagnostic.result(&reached, now().saturating_duration_since(started));
                match reached {
                    Ok(Progress::AtResearch(_)) => {
                        backend.confirm_input_idle(until)?;
                        live()?;
                        // Last game proof is AFTER platform idle, under the
                        // original Anchor/Scope/Entry and absolute deadline.
                        diagnostic.sample_started();
                        let final_sample = transit.observe_with(
                            &request.scope,
                            &request.entry,
                            |_, _| backend.observe(until),
                            now,
                        );
                        diagnostic.result(&final_sample, now().saturating_duration_since(started));
                        match final_sample? {
                            Progress::AtResearch(proof) => {
                                live()?;
                                return Ok(proof);
                            }
                            _ => {
                                return Err(invalid(
                                    "Research changed after input-idle acknowledgement",
                                ))
                            }
                        }
                    }
                    Ok(Progress::Pending(ManagedPanel::Idle)) if delivery.is_some() => {}
                    Err(error)
                        if delivery.is_some()
                            && matches!(error.code, "torn_read" | "research_panel_transition") => {}
                    Err(error) => return Err(error),
                    _ => return Err(invalid("Research panel no longer shown")),
                }
                backend.wait_readback(
                    Duration::from_millis(25).min(until.saturating_duration_since(now())),
                );
            }
        })();
        let proof = match effect {
            Ok(proof) => proof,
            Err(cause) if delivery.is_some() => {
                return Err(uncertain(&format!(
                    "Ordinary transit unverified; preserved Intent; cause={}; diagnostics={}",
                    cause.code,
                    diagnostic.report(
                        dispatch_timing,
                        now().saturating_duration_since(readback_started)
                    )
                )))
            }
            Err(cause) => return Err(cause),
        };
        live()?;
        self.persist(Record {
            request: request.clone(),
            before: record_before,
            outcome: Outcome::Verified {
                after: Box::new(Facts::observed(proof.observed())),
                delivery,
            },
        })?;
        live()?;
        if delivery.is_some() {
            self.admitted.insert(request.id);
        }
        Ok(Execution::Current { proof, delivery })
    }
}

#[cfg(all(test, unix))]
mod tests;

#[cfg(all(test, target_os = "linux"))]
pub(crate) fn fixture_readback_failure() -> Error {
    tests::readback_failure()
}

#[cfg(all(test, unix))]
pub(crate) fn fixture_intent(path: &Path) -> (Journal, Identity) {
    tests::fixture_intent(path)
}
