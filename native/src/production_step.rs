//! One durable ordinary factory-allocation step. Original identity and count
//! survive restart; uncertain input is observed only and is never replayed.
use crate::{
    actions::Epoch,
    construction_stage::{Delivery, Diagnostic, DispatchOutcome},
    error::{Error, Result},
    input_lease::InputAuthority,
    platform::Identity,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
    sync::RwLock,
    thread,
    time::{Duration, Instant},
};

const LIMIT: u64 = 64 << 20;
const LINE: usize = 16 << 10;
fn error(code: &'static str, message: &str) -> Error {
    Error::new(code, message)
}

mod model;
pub use model::{
    FactoryState, FactoryTarget, LineIdentity, PanelStage, PanelState, PanelTarget, State, Target,
};

pub trait Backend {
    type Candidate;
    /// Same-snapshot canonical own line only; this may run after the UI closes.
    fn observe(&mut self, target: &Target, deadline: Instant) -> Result<State>;
    /// Exact named callback/local gates, geometry and owned client profile.
    /// Must compare its fresh canonical state to `before`.
    fn candidate(
        &mut self,
        target: &Target,
        before: &State,
        deadline: Instant,
    ) -> Result<Self::Candidate>;
    /// At most one ordinary input. Recheck original state, UI identities and
    /// original point inside the transport's final before-queue callback.
    fn dispatch_once(
        &mut self,
        target: &Target,
        before: &State,
        candidate: &Self::Candidate,
        deadline: Instant,
    ) -> DispatchOutcome;
    /// Pure owned-window/key/button check, also available after journal restart.
    fn confirm_input_idle(&mut self, deadline: Instant) -> Result<()>;
}
pub trait UnresolvedInputs {
    /// Read every other shared journal under the same controller mutex.
    fn clear(&self, game: &Identity) -> Result<()>;
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Status {
    Intent,
    NotQueued,
    Uncertain,
    Verified,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    id: String,
    target: Target,
    before: State,
    last: State,
    status: Status,
    delivery: Option<Delivery>,
    cause: Option<Diagnostic>,
}
fn request_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b))
    {
        return Err(error(
            "invalid_arguments",
            "Bounded stable request_id required",
        ));
    }
    Ok(())
}
impl Record {
    fn validate(&self) -> Result<()> {
        request_id(&self.id)?;
        self.before.validate(&self.target)?;
        self.last.validate(&self.target)?;
        let possible = matches!(
            self.delivery,
            Some(Delivery::Unknown | Delivery::ServerConfirmed)
        );
        let valid = self.version == 1
            && self.before.same_identity(&self.last)
            && self.cause.as_ref().is_none_or(Diagnostic::validate)
            && match self.status {
                Status::Intent => {
                    self.before.step(&self.target).is_ok()
                        && self.last == self.before
                        && self.delivery.is_none()
                        && self.cause.is_none()
                }
                Status::NotQueued => {
                    self.before.step(&self.target).is_ok()
                        && self.last == self.before
                        && self.delivery == Some(Delivery::NotQueued)
                        && self.cause.is_some()
                }
                Status::Uncertain => {
                    self.before.step(&self.target).is_ok() && possible && self.cause.is_some()
                }
                Status::Verified => {
                    self.last.reached(&self.target)
                        && self.cause.is_none()
                        && if self.delivery.is_none() {
                            self.last == self.before
                        } else {
                            possible && self.before.step(&self.target).is_ok()
                        }
                }
            };
        if !valid {
            return Err(error("invalid_journal", "Invalid production step record"));
        }
        Ok(())
    }
}
#[derive(Debug, Serialize)]
pub struct Execution {
    // Retain the typed journal proof for trusted owner-side plan completion.
    // Never publish process pointers or accept a public JSON summary as proof.
    #[serde(skip)]
    proof: Record,
    #[serde(skip)]
    proof_historical: bool,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub equipment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_requested: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requested_factories: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<&'static str>,
    pub status: Status,
    pub delivery: Option<Delivery>,
    pub historical: bool,
    pub current_state_verified: bool,
    pub event_causality_verified: bool,
}
impl Execution {
    fn new(r: &Record, historical: bool) -> Self {
        Self {
            proof: r.clone(),
            proof_historical: historical,
            request_id: r.id.clone(),
            line_handle: r.target.factory().ok().map(|t| t.line_handle.clone()),
            equipment_id: r.target.factory().ok().map(|t| t.equipment_id.clone()),
            original_requested: r.before.factory().ok().map(|s| s.requested),
            requested_factories: r.target.factory().ok().map(|t| t.factories),
            stage: match &r.target {
                Target::Panel(t) => Some(t.stage.name()),
                _ => None,
            },
            status: r.status,
            delivery: r.delivery,
            historical,
            current_state_verified: !historical,
            event_causality_verified: false,
        }
    }

    pub(crate) fn verified_factory(
        &self,
    ) -> Result<(&str, &FactoryTarget, &FactoryState, &FactoryState)> {
        let (id, target, before, last) = self.verified_step()?;
        Ok((id, target.factory()?, before.factory()?, last.factory()?))
    }
    pub(crate) fn verified_step(&self) -> Result<(&str, &Target, &State, &State)> {
        let r = &self.proof;
        r.validate()?;
        if self.proof_historical || r.status != Status::Verified {
            return Err(error(
                "receipt_mismatch",
                "Fresh verified production journal proof required",
            ));
        }
        Ok((&r.id, &r.target, &r.before, &r.last))
    }
}
pub struct Journal {
    file: File,
    records: BTreeMap<String, Record>,
    poisoned: bool,
}

fn permit(gate: &RwLock<bool>) -> Result<std::sync::RwLockReadGuard<'_, bool>> {
    let p = gate
        .try_read()
        .map_err(|_| error("session_closed", "Input session is closing"))?;
    if !*p {
        return Err(error("session_closed", "Input session closed"));
    }
    Ok(p)
}
fn live(
    authority: &InputAuthority,
    token: &str,
    target: &Target,
    deadline: Instant,
    gate: &RwLock<bool>,
) -> Result<()> {
    authority.authorize(token, target.epoch())?;
    drop(permit(gate)?);
    if Instant::now() >= deadline {
        return Err(error(
            "production_deadline",
            "Original production deadline ended",
        ));
    }
    Ok(())
}

impl Journal {
    pub fn open(path: &Path) -> Result<Self> {
        let mut options = OpenOptions::new();
        options.read(true).append(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(0x00200000);
        }
        let file = options.open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.len() > LIMIT {
            return Err(error(
                "invalid_journal",
                "Private bounded regular journal required",
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.uid() != unsafe { libc::geteuid() }
                || metadata.mode() & 0o077 != 0
                || metadata.nlink() != 1
            {
                return Err(error(
                    "invalid_journal",
                    "Private owned single-link journal required",
                ));
            }
        }
        file.try_lock_exclusive()
            .map_err(|_| error("journal_busy", "One production journal writer required"))?;
        if file.metadata()?.len() > LIMIT {
            return Err(error(
                "invalid_journal",
                "Production journal grew beyond bound before lock",
            ));
        }
        file.sync_all()?;
        #[cfg(unix)]
        File::open(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new(".")),
        )?
        .sync_all()?;
        let mut reader = BufReader::new(file.try_clone()?);
        let mut records: BTreeMap<String, Record> = BTreeMap::new();
        loop {
            let mut line = Vec::new();
            loop {
                let b = reader.fill_buf()?;
                if b.is_empty() {
                    break;
                }
                let n = b
                    .iter()
                    .position(|v| *v == b'\n')
                    .map_or(b.len(), |v| v + 1);
                if line.len() + n > LINE {
                    return Err(error(
                        "invalid_journal",
                        "Production journal line exceeds bound",
                    ));
                }
                let complete = b[n - 1] == b'\n';
                line.extend_from_slice(&b[..n]);
                reader.consume(n);
                if complete {
                    break;
                }
            }
            if line.is_empty() {
                break;
            }
            if line.last() != Some(&b'\n') {
                return Err(error(
                    "invalid_journal",
                    "Incomplete journal tail; preserve for recovery",
                ));
            }
            let record: Record = serde_json::from_slice(&line)
                .map_err(|_| error("invalid_journal", "Malformed production journal record"))?;
            record
                .validate()
                .map_err(|_| error("invalid_journal", "Invalid production journal record"))?;
            if let Some(previous) = records.get(&record.id) {
                Self::transition(previous, &record)?;
            } else if record.status != Status::Intent
                && !(record.status == Status::Verified && record.delivery.is_none())
            {
                return Err(error(
                    "invalid_journal",
                    "Missing original production intent",
                ));
            }
            records.insert(record.id.clone(), record);
        }
        Ok(Self {
            file,
            records,
            poisoned: false,
        })
    }
    fn transition(previous: &Record, next: &Record) -> Result<()> {
        if previous.target != next.target
            || previous.before != next.before
            || !match previous.status {
                Status::Intent => matches!(
                    next.status,
                    Status::Verified | Status::NotQueued | Status::Uncertain
                ),
                Status::Uncertain => {
                    next.status == Status::Verified && next.delivery == previous.delivery
                }
                _ => false,
            }
            || (next.status == Status::Verified && next.delivery.is_none())
        {
            return Err(error(
                "invalid_journal",
                "Production transition must preserve original baseline and intent",
            ));
        }
        Ok(())
    }
    fn append(&mut self, r: Record) -> Result<Record> {
        if self.poisoned {
            return Err(error(
                "journal_failed",
                "Production writer requires recovery",
            ));
        }
        r.validate()?;
        if let Some(previous) = self.records.get(&r.id) {
            Self::transition(previous, &r)?;
        } else if r.status != Status::Intent
            && !(r.status == Status::Verified && r.delivery.is_none())
        {
            return Err(error(
                "invalid_journal",
                "Missing original production intent",
            ));
        }
        let mut bytes = serde_json::to_vec(&r)
            .map_err(|_| error("invalid_journal", "Cannot serialize production record"))?;
        bytes.push(b'\n');
        if bytes.len() > LINE
            || self
                .file
                .metadata()?
                .len()
                .saturating_add(bytes.len() as u64)
                > LIMIT
        {
            return Err(error(
                "journal_full",
                "Durable production capacity required",
            ));
        }
        self.poisoned = true;
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        self.records.insert(r.id.clone(), r.clone());
        self.poisoned = false;
        Ok(r)
    }
    pub fn blocks_game(&self, game: &Identity) -> bool {
        self.poisoned
            || self.records.values().any(|r| {
                r.target.epoch().game == *game
                    && matches!(r.status, Status::Intent | Status::Uncertain)
            })
    }
    /// Classify only an immediate execute error. Never infer no input merely
    /// from an error code or absence of a successful public receipt.
    pub fn failure_evidence(
        &self,
        id: &str,
        target: &Target,
        cause: &Error,
    ) -> crate::construction_stage::FailureEvidence {
        use crate::construction_stage::FailureEvidence as F;
        if self.poisoned
            || self.blocks_game(&target.epoch().game)
            || cause.code == "action_uncertain"
        {
            return F::Unknown;
        }
        match self.records.get(id) {
            None => F::BeforeIntent,
            Some(r)
                if r.target == *target
                    && r.status == Status::NotQueued
                    && r.delivery == Some(Delivery::NotQueued) =>
            {
                F::ConfirmedNotQueued
            }
            _ => F::Unknown,
        }
    }
    fn uncertain(
        &mut self,
        original: &Record,
        delivery: Delivery,
        last: State,
        cause: Error,
        token: &str,
    ) -> Error {
        let cause = Diagnostic::from_error(&cause, token);
        let saved = self.append(Record {
            status: Status::Uncertain,
            delivery: Some(delivery),
            last,
            cause: Some(cause.clone()),
            ..original.clone()
        });
        cause.as_error(
            "action_uncertain",
            if saved.is_ok() {
                "Production step unverified; never replay"
            } else {
                "Cannot persist outcome; preserve original intent"
            },
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn execute<B: Backend>(
        &mut self,
        id: &str,
        target: Target,
        token: &str,
        authority: &InputAuthority,
        gate: &RwLock<bool>,
        domains: &dyn UnresolvedInputs,
        backend: &mut B,
        timeout: Duration,
    ) -> Result<Execution> {
        request_id(id)?;
        target.validate()?;
        if timeout.is_zero() || timeout > Duration::from_secs(10) {
            return Err(error(
                "invalid_arguments",
                "Production timeout must be positive and at most ten seconds",
            ));
        }
        let deadline = (Instant::now() + timeout).min(authority.authorize(token, target.epoch())?);
        live(authority, token, &target, deadline, gate)?;
        if self.poisoned {
            return Err(error(
                "action_uncertain",
                "Production journal requires recovery",
            ));
        }
        if let Some(original) = self.records.get(id).cloned() {
            if original.target != target {
                return Err(error(
                    "request_id_conflict",
                    "Request ID already binds another production target",
                ));
            }
            match original.status {
                Status::Verified => return Ok(Execution::new(&original, true)),
                Status::NotQueued => {
                    return Err(original
                        .cause
                        .as_ref()
                        .unwrap()
                        .as_error("input_not_queued", "Historical refusal; never replay"))
                }
                Status::Intent | Status::Uncertain => {
                    // One settlement attempt, no new baseline, candidate or input.
                    let settled = (|| -> Result<State> {
                        domains.clear(&target.epoch().game)?;
                        let observed = backend.observe(&target, deadline)?;
                        observed.validate(&target)?;
                        if !observed.same_identity(&original.before) || !observed.reached(&target) {
                            return Err(error(
                                "production_target_unreached",
                                "Original production target not observed reached",
                            ));
                        }
                        backend.confirm_input_idle(deadline)?;
                        let confirmed = backend.observe(&target, deadline)?;
                        confirmed.validate(&target)?;
                        live(authority, token, &target, deadline, gate)?;
                        if confirmed != observed {
                            return Err(error(
                                "ownership_changed",
                                "Canonical production state changed during settlement",
                            ));
                        }
                        Ok(confirmed)
                    })();
                    let last = settled.map_err(|e| {
                        Diagnostic::from_error(&e, token).as_error(
                            "action_uncertain",
                            "Original production intent remains unresolved",
                        )
                    })?;
                    let record = self
                        .append(Record {
                            last,
                            status: Status::Verified,
                            delivery: Some(original.delivery.unwrap_or(Delivery::Unknown)),
                            cause: None,
                            ..original
                        })
                        .map_err(|e| {
                            Diagnostic::from_error(&e, token)
                                .as_error("action_uncertain", "Cannot persist read-only settlement")
                        })?;
                    return Ok(Execution::new(&record, false));
                }
            }
        }
        if self.blocks_game(&target.epoch().game) {
            return Err(error(
                "action_uncertain",
                "Unresolved production step blocks new IDs",
            ));
        }
        domains.clear(&target.epoch().game)?;
        let before = backend.observe(&target, deadline)?;
        before.validate(&target)?;
        live(authority, token, &target, deadline, gate)?;
        let initial = Record {
            version: 1,
            id: id.into(),
            target: target.clone(),
            before: before.clone(),
            last: before.clone(),
            status: Status::Verified,
            delivery: None,
            cause: None,
        };
        if before.reached(&target) {
            return self.append(initial).map(|r| Execution::new(&r, false));
        }
        before.step(&target)?;
        let candidate = backend.candidate(&target, &before, deadline)?;
        let fresh = backend.observe(&target, deadline)?;
        fresh.validate(&target)?;
        if fresh != before {
            return Err(error(
                "production_state_changed",
                "Original allocation changed before intent",
            ));
        }
        live(authority, token, &target, deadline, gate)?;
        domains.clear(&target.epoch().game)?;
        if self
            .file
            .metadata()?
            .len()
            .saturating_add((3 * LINE) as u64)
            > LIMIT
        {
            return Err(error(
                "journal_full",
                "Reserve intent, outcome and possible settlement capacity",
            ));
        }
        let intent = self.append(Record {
            status: Status::Intent,
            ..initial
        })?;
        let final_check = (|| -> Result<()> {
            live(authority, token, &target, deadline, gate)?;
            domains.clear(&target.epoch().game)?;
            let fresh = backend.observe(&target, deadline)?;
            fresh.validate(&target)?;
            if fresh != before {
                return Err(error(
                    "production_state_changed",
                    "Original allocation changed after intent",
                ));
            }
            live(authority, token, &target, deadline, gate)
        })();
        let outcome = match final_check.and_then(|()| permit(gate)) {
            Ok(p) => {
                let out = backend.dispatch_once(&target, &before, &candidate, deadline);
                drop(p);
                out
            }
            Err(e) => DispatchOutcome::NotQueued(e),
        };
        let delivery = match outcome {
            DispatchOutcome::NotQueued(e) => {
                let cause = Diagnostic::from_error(&e, token);
                self.append(Record {
                    status: Status::NotQueued,
                    delivery: Some(Delivery::NotQueued),
                    cause: Some(cause.clone()),
                    ..intent
                })
                .map_err(|e| {
                    Diagnostic::from_error(&e, token).as_error(
                        "action_uncertain",
                        "Refusal persistence failed; preserve intent",
                    )
                })?;
                return Err(cause.as_error(
                    "input_not_queued",
                    "Ordinary production click proved not queued; never replay",
                ));
            }
            DispatchOutcome::Unknown(Some(e)) => {
                return Err(self.uncertain(&intent, Delivery::Unknown, before, e, token))
            }
            DispatchOutcome::Unknown(None) => Delivery::Unknown,
            DispatchOutcome::ServerConfirmed => Delivery::ServerConfirmed,
        };
        let mut last = before;
        loop {
            let checked = (|| -> Result<Option<State>> {
                live(authority, token, &target, deadline, gate)?;
                domains.clear(&target.epoch().game)?;
                let observed = backend.observe(&target, deadline)?;
                observed.validate(&target)?;
                if !observed.same_identity(&intent.before) {
                    return Err(error(
                        "ownership_changed",
                        "Original production identity changed after input",
                    ));
                }
                live(authority, token, &target, deadline, gate)?;
                last = observed.clone();
                if !observed.reached(&target) {
                    return Ok(None);
                }
                backend.confirm_input_idle(deadline)?;
                let confirmed = backend.observe(&target, deadline)?;
                confirmed.validate(&target)?;
                live(authority, token, &target, deadline, gate)?;
                if confirmed != observed {
                    return Err(error(
                        "production_state_changed",
                        "Canonical allocation changed after idle check",
                    ));
                }
                Ok(Some(confirmed))
            })();
            match checked {
                Ok(Some(last)) => {
                    let saved = self
                        .append(Record {
                            status: Status::Verified,
                            delivery: Some(delivery),
                            last,
                            ..intent
                        })
                        .map_err(|e| {
                            Diagnostic::from_error(&e, token).as_error(
                                "action_uncertain",
                                "Cannot persist verified production result",
                            )
                        })?;
                    return Ok(Execution::new(&saved, false));
                }
                Ok(None) => {}
                Err(e)
                    if Instant::now() < deadline
                        && (e.code == "torn_read"
                            || (matches!(target, Target::Panel(_))
                                && e.code == "production_panel_transition")) => {}
                Err(e) => return Err(self.uncertain(&intent, delivery, last, e, token)),
            }
            thread::sleep(
                Duration::from_millis(20).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }
}

#[cfg(test)]
mod tests;
