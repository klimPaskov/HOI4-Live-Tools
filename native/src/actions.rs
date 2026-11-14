//! Durable ordinary-action transactions. A dispatch whose result is unknown is
//! never replayed, including after a server crash. No native call implementation
//! or arbitrary command surface is provided here.
//!
//! The clock's state (paused, speed) is fully readable, so every clock request
//! ends in a terminal receipt: `Verified` when fresh state shows the target,
//! `NotApplied` when it does not once the verification window has passed. A
//! request stays open only while the game cannot be read at all; the next call
//! that can read settles it (`settle_open`). Nothing blocks input for good.
use crate::{
    error::{Error, Result},
    platform::Identity,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
    time::{Duration, Instant},
};

const JOURNAL_LIMIT: u64 = 64 << 20;
const LINE_LIMIT: usize = 16 << 10;
/// Observation window after a transport that already reported failure.
const DISPATCH_FAILED_WINDOW: Duration = Duration::from_millis(1_000);

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Epoch {
    pub game: Identity,
    /// Verified owned-run context supplied by the adapter. A persisted game
    /// identifier alone does not prove a fresh load/new-game generation.
    pub campaign: String,
    pub player: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", content = "value", deny_unknown_fields)]
pub enum Action {
    SetPaused(bool),
    SetSpeed(u8),
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub epoch: Epoch,
    pub paused: bool,
    pub speed: u8,
}
impl Action {
    fn validate(&self) -> Result<()> {
        if matches!(self, Self::SetSpeed(v) if *v > 4) {
            return Err(Error::new(
                "invalid_action",
                "Speed must use validated internal range 0..4",
            ));
        }
        Ok(())
    }
    fn reached(&self, state: &State) -> bool {
        match self {
            Self::SetPaused(v) => state.paused == *v,
            Self::SetSpeed(v) => state.speed == *v,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Status {
    Dispatching,
    Verified,
    /// A dispatch whose target fresh state does not show after the verification window;
    /// settled by reading only, never by sending the key again.
    NotApplied,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub version: u8,
    pub id: String,
    pub epoch: Epoch,
    pub action: Action,
    pub status: Status,
    pub dispatched: bool,
    pub observed: State,
}
/// Implementations must check exclusive input ownership, build, campaign epoch
/// and ordinary-action eligibility in preflight and immediately before sending.
/// Dispatch errors are uncertain: the caller cannot assume no side effects.
pub trait Backend {
    fn observe(&mut self) -> Result<State>;
    fn preflight(&mut self, action: &Action, before: &State) -> Result<()>;
    fn dispatch(&mut self, action: &Action, before: &State) -> Result<()>;
}
pub struct Journal {
    file: File,
    receipts: BTreeMap<String, Receipt>,
    poisoned: bool,
}
impl Journal {
    /// `blocks_game` for the integration tests of this crate.
    #[doc(hidden)]
    pub fn blocks_game_for_test(&self, epoch: &Epoch) -> bool {
        self.blocks_game(&epoch.game)
    }
    pub(crate) fn blocks_game(&self, game: &Identity) -> bool {
        self.poisoned
            || self
                .receipts
                .values()
                .any(|r| r.status == Status::Dispatching && &r.epoch.game == game)
    }
    /// Private per-session journal. The caller owns its parent directory. File
    /// locking gives a single writer, with automatic release on process exit.
    pub fn open(path: &Path) -> Result<Self> {
        if path.symlink_metadata().is_ok_and(|m| !m.is_file()) {
            return Err(Error::new(
                "invalid_journal",
                "Journal must be a regular file, not a link or device",
            ));
        }
        let mut options = OpenOptions::new();
        options.read(true).append(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path)?;
        file.try_lock_exclusive()
            .map_err(|e| Error::new("journal_busy", e.to_string()))?;
        if !file.metadata()?.is_file() || file.metadata()?.len() > JOURNAL_LIMIT {
            return Err(Error::new(
                "invalid_journal",
                "Journal exceeds bound or is not a file",
            ));
        }
        // Persist a newly created journal's directory entry on Unix as well as
        // its contents. Windows receipt writes use FlushFileBuffers via sync_all.
        #[cfg(unix)]
        {
            file.sync_all()?;
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            File::open(parent)?.sync_all()?;
        }
        let mut receipts: BTreeMap<String, Receipt> = BTreeMap::new();
        let mut reader = BufReader::new(file.try_clone()?);
        // Read with a bound before allocating, including malformed long lines.
        loop {
            let mut line = Vec::new();
            loop {
                let buf = reader.fill_buf()?;
                if buf.is_empty() {
                    break;
                }
                let end = buf
                    .iter()
                    .position(|b| *b == b'\n')
                    .map_or(buf.len(), |n| n + 1);
                if line.len() + end > LINE_LIMIT {
                    return Err(Error::new(
                        "invalid_journal",
                        "Journal record exceeds bound",
                    ));
                }
                line.extend_from_slice(&buf[..end]);
                let complete = buf[end - 1] == b'\n';
                reader.consume(end);
                if complete {
                    break;
                }
            }
            if line.is_empty() {
                break;
            }
            if line.last() != Some(&b'\n') {
                return Err(Error::new(
                    "invalid_journal",
                    "Incomplete journal tail; preserve for recovery",
                ));
            }
            let record: Receipt = serde_json::from_slice(&line)
                .map_err(|e| Error::new("invalid_journal", e.to_string()))?;
            validate_id(&record.id)?;
            record.action.validate()?;
            if record.version != 1
                || record.observed.epoch != record.epoch
                || record.observed.speed > 4
                || (record.status == Status::Dispatching && !record.dispatched)
            {
                return Err(Error::new(
                    "invalid_journal",
                    "Invalid receipt binding/version",
                ));
            }
            if record.status == Status::Verified && !record.action.reached(&record.observed) {
                return Err(Error::new(
                    "invalid_journal",
                    "Verified receipt has no target witness",
                ));
            }
            if record.status == Status::NotApplied && record.action.reached(&record.observed) {
                return Err(Error::new(
                    "invalid_journal",
                    "Not-applied receipt shows its target",
                ));
            }
            if let Some(previous) = receipts.get(&record.id) {
                if previous.status != Status::Dispatching
                    || record.status == Status::Dispatching
                    || !same_campaign(&previous.epoch, &record.epoch)
                    || previous.action != record.action
                    || previous.dispatched != record.dispatched
                {
                    return Err(Error::new("invalid_journal", "Invalid receipt transition"));
                }
            } else if (record.status == Status::Verified && record.dispatched)
                || record.status == Status::NotApplied
            {
                return Err(Error::new("invalid_journal", "Dispatch witness missing"));
            }
            receipts.insert(record.id.clone(), record);
        }
        Ok(Self {
            file,
            receipts,
            poisoned: false,
        })
    }
    fn append(&mut self, record: Receipt) -> Result<Receipt> {
        if self.poisoned {
            return Err(Error::new(
                "journal_failed",
                "Journal requires recovery before further actions",
            ));
        }
        let mut line = serde_json::to_vec(&record)
            .map_err(|e| Error::new("invalid_journal", e.to_string()))?;
        line.push(b'\n');
        if line.len() > LINE_LIMIT
            || self
                .file
                .metadata()?
                .len()
                .saturating_add(line.len() as u64)
                > JOURNAL_LIMIT
        {
            return Err(Error::new(
                "journal_full",
                "No action may dispatch without durable receipt capacity",
            ));
        }
        // Poison before writing. Partial writes/fsync failures must not permit a
        // second command on this server, even if a caller ignores the error.
        self.poisoned = true;
        self.file.write_all(&line)?;
        self.file.sync_all()?;
        self.receipts.insert(record.id.clone(), record.clone());
        self.poisoned = false;
        Ok(record)
    }
    pub fn receipt(&self, id: &str) -> Option<&Receipt> {
        self.receipts.get(id)
    }
    pub fn execute(
        &mut self,
        id: &str,
        epoch: &Epoch,
        action: Action,
        backend: &mut dyn Backend,
        timeout: Duration,
    ) -> Result<Receipt> {
        validate_id(id)?;
        action.validate()?;
        if timeout.is_zero() || timeout > Duration::from_secs(10) {
            return Err(Error::new(
                "invalid_action",
                "Verification timeout must be >0 and <=10 seconds",
            ));
        }
        if self.poisoned {
            return Err(Error::new("journal_failed", "Journal requires recovery"));
        }
        if let Some(previous) = self.receipts.get(id) {
            if !same_campaign(&previous.epoch, epoch) || previous.action != action {
                return Err(Error::new(
                    "idempotency_conflict",
                    "Request ID is already bound to a different action or epoch",
                ));
            }
            if previous.status == Status::Dispatching {
                // An open request (the game could not be read when it ran, or the server
                // stopped): settle it from fresh state without sending anything again.
                self.settle_open(epoch, &mut || backend.observe())
                    .map_err(|error| {
                        Error::after_possible_input("Clock settlement readback failed", error)
                    })?;
            }
            return self.receipts.get(id).cloned().ok_or_else(|| {
                Error::new(
                    "journal_failed",
                    "Clock receipt disappeared during settlement",
                )
            });
        }
        // An open request of this game is settled from fresh state before anything new is
        // sent. Only an unreadable game keeps it open (and then nothing can be verified
        // anyway); it can never block input for good.
        self.settle_open(epoch, &mut || backend.observe())
            .map_err(|error| {
                Error::new(
                    "action_uncertain",
                    format!(
                        "An earlier clock request is still open and the game could not be read to settle it: {error}"
                    ),
                )
            })?;
        let before = backend.observe()?;
        check_state(epoch, &before)?;
        backend.preflight(&action, &before)?;
        // Preflight may wait for transport or read a changing game. Verify the
        // current state before recording a no-op or a dispatch decision.
        let before = backend.observe()?;
        check_state(epoch, &before)?;
        let mut record = Receipt {
            version: 1,
            id: id.into(),
            epoch: epoch.clone(),
            action,
            status: Status::Verified,
            dispatched: false,
            observed: before,
        };
        if record.action.reached(&record.observed) {
            return self.append(record);
        }
        record.status = Status::Dispatching;
        record.dispatched = true;
        if self
            .file
            .metadata()?
            .len()
            .saturating_add((2 * LINE_LIMIT) as u64)
            > JOURNAL_LIMIT
        {
            return Err(Error::new(
                "journal_full",
                "Reserve both dispatch and verification receipts before sending",
            ));
        }
        let mut record = self.append(record)?;
        let dispatched = backend.dispatch(&record.action, &record.observed);
        // Whatever the transport reported, fresh state decides. A transport that already gave
        // up (it waits for the change itself) gets a short window; a completed one the full
        // verification timeout.
        let window = match &dispatched {
            Ok(()) => timeout,
            Err(_) => timeout.min(DISPATCH_FAILED_WINDOW),
        };
        let start = Instant::now();
        let mut seen: Option<State> = None;
        let mut unreadable: Option<Error> = None;
        loop {
            match backend.observe().and_then(|current| {
                check_state(epoch, &current)?;
                Ok(current)
            }) {
                Ok(current) => {
                    if record.action.reached(&current) {
                        record.status = Status::Verified;
                        record.observed = current;
                        return self.append(record).map_err(|e| {
                            Error::new(
                                "action_uncertain",
                                format!(
                                    "Target observed but verification receipt was not committed: {e}"
                                ),
                            )
                        });
                    }
                    seen = Some(current);
                }
                Err(error) => unreadable = Some(error),
            }
            if start.elapsed() >= window {
                break;
            }
            std::thread::sleep(
                Duration::from_millis(20).min(window.saturating_sub(start.elapsed())),
            );
        }
        let Some(current) = seen else {
            // Never read once after the dispatch: the request stays open and the next call
            // that can read the game settles it. It blocks nothing by itself.
            let cause = unreadable
                .map(|e| e.to_string())
                .unwrap_or_else(|| "no observation".into());
            return Err(Error::new(
                "action_uncertain",
                format!(
                    "Clock could not be read after the key; the request settles on the next readable call: {cause}"
                ),
            ));
        };
        // Read, and the target is not there: a clean failure, closed for good.
        record.status = Status::NotApplied;
        record.observed = current;
        let receipt = self.append(record).map_err(|e| {
            Error::new(
                "action_uncertain",
                format!("Clock outcome observed but its receipt was not committed: {e}"),
            )
        })?;
        match dispatched {
            Ok(()) => Ok(receipt),
            Err(error) => Err(Error::new(
                error.code,
                format!(
                    "{}; the clock did not reach the target and the request is closed as NotApplied",
                    error.message
                ),
            )),
        }
    }
    /// Settle every open request of `epoch`'s game from one fresh observation: `Verified`
    /// when the state shows its target, `NotApplied` otherwise. Reads only. An error means the
    /// game could not be read (or is not this campaign) and the requests stay open.
    pub(crate) fn settle_open(
        &mut self,
        epoch: &Epoch,
        observe: &mut dyn FnMut() -> Result<State>,
    ) -> Result<()> {
        if self.poisoned {
            return Err(Error::new("journal_failed", "Journal requires recovery"));
        }
        let open: Vec<Receipt> = self
            .receipts
            .values()
            .filter(|r| r.status == Status::Dispatching && r.epoch.game == epoch.game)
            .cloned()
            .collect();
        if open.is_empty() {
            return Ok(());
        }
        let current = observe()?;
        check_state(epoch, &current)?;
        for mut record in open {
            if same_campaign(&record.epoch, epoch) {
                record.status = if record.action.reached(&current) {
                    Status::Verified
                } else {
                    Status::NotApplied
                };
                record.epoch = epoch.clone();
                record.observed = current.clone();
            } else {
                // Another campaign of the same process: this campaign's clock says nothing
                // about it and its key is long since handled. Closed with its own
                // pre-dispatch observation (which does not show the target).
                record.status = Status::NotApplied;
            }
            self.append(record)?;
        }
        Ok(())
    }
}
fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || b"-_.:/".contains(&v))
    {
        return Err(Error::new(
            "invalid_action",
            "Request ID must be 1..128 ASCII identifier characters",
        ));
    }
    Ok(())
}
/// The same owned process, player and playthrough; the campaign prefix is
/// recomputed by each server start, so a restarted server can still settle
/// its own earlier receipt.
fn same_campaign(a: &Epoch, b: &Epoch) -> bool {
    let playthrough = |e: &Epoch| match e.campaign.split_once(':') {
        Some((_, p)) => p.to_string(),
        None => e.campaign.clone(),
    };
    a.game == b.game && a.player == b.player && playthrough(a) == playthrough(b)
}
fn check_state(epoch: &Epoch, state: &State) -> Result<()> {
    if state.epoch != *epoch || state.speed > 4 {
        return Err(Error::new(
            "ownership_changed",
            "Action campaign/player/process binding changed",
        ));
    }
    Ok(())
}
