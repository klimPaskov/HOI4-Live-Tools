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
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
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
    path: PathBuf,
    stamp: StorageStamp,
    digest: Sha256,
    records: BTreeMap<String, Record>,
    poisoned: bool,
}

#[derive(PartialEq, Eq)]
struct StorageStamp {
    len: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    unix: (u64, u64, u64, u32, u32, i64, i64),
}
impl StorageStamp {
    fn from(metadata: &std::fs::Metadata) -> Result<Self> {
        if !metadata.is_file() || metadata.len() > LIMIT {
            return Err(error(
                "invalid_journal",
                "Bounded regular production journal required",
            ));
        }
        #[cfg(unix)]
        let unix = {
            use std::os::unix::fs::MetadataExt;
            if metadata.uid() != unsafe { libc::geteuid() }
                || metadata.mode() & 0o077 != 0
                || metadata.nlink() != 1
            {
                return Err(error(
                    "invalid_journal",
                    "Private owned single-link production journal required",
                ));
            }
            (
                metadata.dev(),
                metadata.ino(),
                metadata.nlink(),
                metadata.mode(),
                metadata.uid(),
                metadata.ctime(),
                metadata.ctime_nsec(),
            )
        };
        Ok(Self {
            len: metadata.len(),
            modified: metadata.modified()?,
            #[cfg(unix)]
            unix,
        })
    }
}
fn storage_stamp(file: &File, path: &Path) -> Result<StorageStamp> {
    let opened = StorageStamp::from(&file.metadata()?)?;
    let named = StorageStamp::from(&path.symlink_metadata()?)?;
    if opened != named {
        return Err(error(
            "journal_changed",
            "Retained production journal path or identity changed",
        ));
    }
    Ok(opened)
}
fn storage_digest(file: &File) -> Result<Sha256> {
    let mut reader = file.try_clone()?;
    reader.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 16 << 10];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > LIMIT {
            return Err(error("invalid_journal", "Production journal exceeds bound"));
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest)
}
fn same_digest(a: &Sha256, b: &Sha256) -> bool {
    a.clone().finalize() == b.clone().finalize()
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
        let stamp = storage_stamp(&file, path)?;
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
        let digest = storage_digest(&file)?;
        if storage_stamp(&file, path)? != stamp {
            return Err(error(
                "journal_changed",
                "Production journal changed during open",
            ));
        }
        Ok(Self {
            file,
            path: path.to_owned(),
            stamp,
            digest,
            records,
            poisoned: false,
        })
    }
    /// Retain the original handle/path and exact bytes across every owner gate.
    /// Restoring a changed file never clears this owner's uncertainty.
    pub(crate) fn check_storage(&mut self) -> Result<()> {
        if self.poisoned {
            return Err(error(
                "action_uncertain",
                "Production journal storage is uncertain",
            ));
        }
        let result = (|| {
            if storage_stamp(&self.file, &self.path)? != self.stamp
                || !same_digest(&storage_digest(&self.file)?, &self.digest)
                || storage_stamp(&self.file, &self.path)? != self.stamp
            {
                return Err(error(
                    "journal_changed",
                    "Retained production journal bytes or identity changed",
                ));
            }
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    #[cfg(any(target_os = "macos", all(test, unix)))]
    pub(crate) fn check_owner(&mut self, epoch: &Epoch) -> Result<()> {
        self.check_storage()?;
        if self.records.values().any(|r| r.target.epoch() != epoch) {
            self.poisoned = true;
            return Err(error(
                "ownership_changed",
                "Production journal contains a foreign controller epoch",
            ));
        }
        Ok(())
    }
    #[cfg(any(target_os = "macos", all(test, unix)))]
    pub(crate) fn unresolved(&self) -> bool {
        self.poisoned
            || self
                .records
                .values()
                .any(|r| matches!(r.status, Status::Intent | Status::Uncertain))
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
        self.check_storage()?;
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
        let mut digest = self.digest.clone();
        digest.update(&bytes);
        self.file.write_all(&bytes)?;
        self.file.sync_all()?;
        let stamp = storage_stamp(&self.file, &self.path)?;
        if stamp.len != self.stamp.len + bytes.len() as u64
            || !same_digest(&storage_digest(&self.file)?, &digest)
            || storage_stamp(&self.file, &self.path)? != stamp
        {
            return Err(error(
                "journal_changed",
                "Production journal changed across its own append",
            ));
        }
        self.stamp = stamp;
        self.digest = digest;
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
        self.check_storage()?;
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

#[cfg(all(test, unix))]
mod retained_storage_tests {
    use super::*;
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicU64, Ordering},
    };
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "private-production-storage-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
        fn path(&self) -> PathBuf {
            self.0.join("production-actions.jsonl")
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn epoch() -> Epoch {
        Epoch {
            game: Identity {
                pid: 71,
                start_ticks: "123.000456".into(),
                exe: "/generated/hoi4".into(),
                uid: 501,
            },
            campaign: "original:fixture".into(),
            player: "GER".into(),
        }
    }
    fn intent(id: &str) -> Record {
        let target = FactoryTarget {
            epoch: epoch(),
            line_handle: "0x0000000100000038".into(),
            equipment_id: "infantry_equipment_1".into(),
            factories: 11,
        };
        let before = State::Factories(FactoryState {
            epoch: epoch(),
            line: LineIdentity {
                country: 0x10000,
                production: 0x20000,
                line: 0x30000,
                variant: 0x40000,
                base_type: 0x50000,
                handle: 0x0000000100000038,
                equipment_id: target.equipment_id.clone(),
            },
            requested: 10,
        });
        Record {
            version: 1,
            id: id.into(),
            target: Target::Factories(target),
            before: before.clone(),
            last: before,
            status: Status::Intent,
            delivery: None,
            cause: None,
        }
    }
    #[test]
    fn own_append_updates_fingerprint_and_reopen_keeps_original_owner_uncertainty() {
        let temp = Temp::new();
        let mut journal = Journal::open(&temp.path()).unwrap();
        journal.check_storage().unwrap();
        journal.append(intent("first")).unwrap();
        journal.check_owner(&epoch()).unwrap();
        assert!(journal.unresolved());
        drop(journal);
        let mut reopened = Journal::open(&temp.path()).unwrap();
        reopened.check_owner(&epoch()).unwrap();
        assert!(reopened.unresolved());
    }
    #[test]
    fn byte_identical_path_replacement_is_not_a_retained_writer_and_poison_is_sticky() {
        let temp = Temp::new();
        let mut journal = Journal::open(&temp.path()).unwrap();
        journal.append(intent("first")).unwrap();
        let bytes = fs::read(temp.path()).unwrap();
        let original = temp.0.join("original");
        fs::rename(temp.path(), &original).unwrap();
        fs::write(temp.path(), &bytes).unwrap();
        fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(journal.check_storage().unwrap_err().code, "journal_changed");
        fs::remove_file(temp.path()).unwrap();
        fs::rename(original, temp.path()).unwrap();
        assert_eq!(
            journal.check_storage().unwrap_err().code,
            "action_uncertain"
        );
        assert!(journal.unresolved());
    }
    #[test]
    fn edited_or_appended_bytes_are_never_adopted_and_restoration_cannot_rescue_them() {
        for append in [false, true] {
            let temp = Temp::new();
            let mut journal = Journal::open(&temp.path()).unwrap();
            journal.append(intent("first")).unwrap();
            let bytes = fs::read(temp.path()).unwrap();
            let mut edited = bytes.clone();
            if append {
                edited.push(b'\n');
            } else {
                edited[0] ^= 1;
            }
            fs::write(temp.path(), edited).unwrap();
            assert!(journal.check_storage().is_err());
            fs::write(temp.path(), bytes).unwrap();
            assert_eq!(
                journal.append(intent("replacement")).unwrap_err().code,
                "action_uncertain"
            );
        }
    }
    #[test]
    fn private_link_and_mode_changes_refuse_before_new_records() {
        for hardlink in [false, true] {
            let temp = Temp::new();
            let mut journal = Journal::open(&temp.path()).unwrap();
            if hardlink {
                fs::hard_link(temp.path(), temp.0.join("alias")).unwrap();
            } else {
                fs::set_permissions(temp.path(), fs::Permissions::from_mode(0o644)).unwrap();
            }
            assert_eq!(journal.check_storage().unwrap_err().code, "invalid_journal");
            assert_eq!(
                journal.append(intent("replacement")).unwrap_err().code,
                "action_uncertain"
            );
        }
    }
    #[test]
    fn foreign_unresolved_epoch_is_never_hidden_by_game_filtered_checks() {
        let temp = Temp::new();
        let mut journal = Journal::open(&temp.path()).unwrap();
        journal.append(intent("first")).unwrap();
        drop(journal);
        let mut journal = Journal::open(&temp.path()).unwrap();
        let mut foreign = epoch();
        foreign.game.start_ticks = "124.000456".into();
        assert!(!journal.blocks_game(&foreign.game));
        assert!(journal.unresolved());
        assert_eq!(
            journal.check_owner(&foreign).unwrap_err().code,
            "ownership_changed"
        );
        assert_eq!(
            journal.check_owner(&epoch()).unwrap_err().code,
            "action_uncertain"
        );
    }
}
