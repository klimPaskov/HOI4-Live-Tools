//! Durable receipts for ordinary game actions (research, Focus, ...). Intent is
//! written and synced before the first input; a dispatch without a durable
//! outcome is never replayed.
//!
//! Every intent ends in a terminal receipt before its call returns. After the
//! dispatch the journal re-observes the game for a bounded settlement window
//! (`SETTLE_WINDOW`), reading past the action's own deadline (the window is
//! read-only: `settling_until`), and records what it sees: `Verified`,
//! `NotApplied`, `Diverged`, or `Unconfirmed` when the effect could not be
//! proven either way inside the window. An `Unconfirmed` request is closed: it
//! is never replayed, it blocks nothing, and the caller is told to read the
//! state again before sending a fresh request (each adapter's own preflight
//! refuses a duplicate: already active, already researched, target reached).
//! Only an intent left open by a server that stopped in the middle of an action
//! still blocks input, and only for `ORPHAN_MS`; the next action closes it.
//!
//! Settlement rule: compare only what the action can change. The game runs at
//! speed 3-5 while an action is journaled, so unrelated state (other research
//! slots, production lines, armies, queues, date, resources) drifts. Each
//! `settle` closure must read only the fields its action writes: `Reached` when
//! the exact target effect is present, `Unchanged` only when every possible
//! effect is provably absent, `Diverged` for a settled mismatch, `Unknown` for
//! partial or foreign effects (re-observed until the window ends).
//! The pure rules live in `settle.rs` (and `research::settle`) with tests.
use crate::{
    actions::Epoch,
    error::{Error, Result},
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    cell::Cell,
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const LINE_LIMIT: usize = 16 << 10;
const JOURNAL_LIMIT: u64 = 64 << 20;
/// How long one call keeps re-observing an action whose effect is not provable yet.
const SETTLE_WINDOW: Duration = Duration::from_millis(3_000);
const SETTLE_POLL: Duration = Duration::from_millis(100);
/// How long an intent left open by a stopped server blocks its own domain and global input.
/// After that the next action closes it as `Unconfirmed`.
const ORPHAN_MS: u64 = 10_000;

thread_local! {
    /// End of the read-only settlement window running on this thread, if any.
    static SETTLING: Cell<Option<Instant>> = const { Cell::new(None) };
}
/// While the journal re-observes an action on this thread: the end of that window. The action
/// fence lets memory reads through until then even though the action's own deadline (or lease)
/// has ended, and still refuses every input. Without this an action that ran out of time could
/// never be settled: its readback failed on the same expired deadline.
pub(super) fn settling_until() -> Option<Instant> {
    SETTLING.with(Cell::get)
}
pub(super) struct Settling(Option<Instant>);
impl Settling {
    pub(super) fn enter(until: Instant) -> Self {
        Self(SETTLING.with(|s| s.replace(Some(until))))
    }
}
impl Drop for Settling {
    fn drop(&mut self) {
        SETTLING.with(|s| s.set(self.0));
    }
}

#[cfg(test)]
#[path = "journal/strict_tests.rs"]
mod strict_tests;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Status {
    Dispatching,
    Verified,
    NotApplied,
    /// Fully observed outcome that differs from the target (e.g. a line was
    /// added but its factory count could not be set). Settled, not uncertain.
    Diverged,
    /// Input may have been sent and the settlement window ended without proof of the effect or
    /// of its absence. Closed: never replayed, blocks nothing. The caller reads the state again.
    Unconfirmed,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub version: u8,
    pub id: String,
    pub epoch: Epoch,
    pub action: Value,
    pub status: Status,
    pub detail: Value,
    pub unix_ms: u64,
}
/// What fresh game state says about an action whose dispatch is unresolved.
pub enum Outcome {
    Reached(Value),
    Unchanged(Value),
    /// A fresh read proves the action's target does not exist (e.g. no army with that name),
    /// so no input could have acted on it: settles even an unresolved retry as NotApplied.
    TargetAbsent(Value),
    Diverged(Value),
    /// Not provable yet: partial or foreign effects, or state still moving. Re-observed until
    /// the settlement window ends.
    Unknown,
    /// The adapter has nothing in the game to read for this action (its verdict comes only from
    /// what its own dispatch saw): re-observing cannot change it, so the request closes as
    /// `Unconfirmed` at once instead of holding the input lane for the whole window.
    Unobservable,
}

/// A new strict adapter supplies a result from this attempt only. It must
/// classify every failure after any possible input as Uncertain; unchanged
/// game state is not proof that no event was queued.
pub(super) enum StrictOutcome {
    Verified(Value),
    NotQueued(Error),
    Uncertain(Error),
}

pub struct GameJournal {
    file: File,
    receipts: BTreeMap<String, Receipt>,
    poisoned: bool,
    /// Detail of the next intent record (set only inside `execute_baselined`).
    baseline: Option<Value>,
    /// Settlement window and poll interval (shortened only by tests).
    window: Duration,
    poll: Duration,
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}
/// Same owned process, player and playthrough. The per-bind nonce before ':'
/// is ignored so a restarted server can replay or settle earlier receipts.
fn same_campaign(a: &Epoch, b: &Epoch) -> bool {
    let playthrough = |e: &Epoch| e.campaign.split_once(':').map(|(_, p)| p.to_string());
    a.game == b.game
        && a.player == b.player
        && playthrough(a).is_some()
        && playthrough(a) == playthrough(b)
}
fn valid_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 96
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
    {
        return Err(Error::new("invalid_arguments", "Invalid request_id"));
    }
    Ok(())
}
impl GameJournal {
    pub fn open(path: &Path) -> Result<Self> {
        if path.symlink_metadata().is_ok_and(|m| !m.is_file()) {
            return Err(Error::new(
                "invalid_journal",
                "Journal must be a regular file",
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
        if file.metadata()?.len() > JOURNAL_LIMIT {
            return Err(Error::new("invalid_journal", "Journal exceeds bound"));
        }
        let mut receipts: BTreeMap<String, Receipt> = BTreeMap::new();
        let mut reader = BufReader::new(file.try_clone()?);
        loop {
            let mut line = Vec::new();
            if reader
                .by_ref()
                .take((LINE_LIMIT + 1) as u64)
                .read_until(b'\n', &mut line)?
                == 0
            {
                break;
            }
            if line.len() > LINE_LIMIT {
                return Err(Error::new(
                    "invalid_journal",
                    "Journal record exceeds bound",
                ));
            }
            if line.last() != Some(&b'\n') {
                return Err(Error::new("invalid_journal", "Truncated journal record"));
            }
            line.pop();
            if line.is_empty() {
                continue;
            }
            if line.len() > LINE_LIMIT {
                return Err(Error::new(
                    "invalid_journal",
                    "Journal record exceeds bound",
                ));
            }
            let record: Receipt = serde_json::from_slice(&line)
                .map_err(|e| Error::new("invalid_journal", e.to_string()))?;
            if !matches!(record.version, 1 | 2)
                || (record.version == 2 && record.status == Status::Diverged)
            {
                return Err(Error::new(
                    "invalid_journal",
                    "Unsupported action receipt semantics",
                ));
            }
            valid_id(&record.id)?;
            if let Some(previous) = receipts.get(&record.id) {
                if previous.status != Status::Dispatching
                    || record.status == Status::Dispatching
                    || previous.epoch != record.epoch
                    || previous.action != record.action
                    || previous.version != record.version
                {
                    return Err(Error::new("invalid_journal", "Invalid receipt transition"));
                }
            } else if record.status != Status::Dispatching {
                return Err(Error::new("invalid_journal", "Outcome without intent"));
            }
            receipts.insert(record.id.clone(), record);
        }
        Ok(Self {
            file,
            receipts,
            poisoned: false,
            baseline: None,
            // Unit tests of every adapter run through this journal: keep their unknown
            // outcomes fast.
            window: if cfg!(test) {
                Duration::from_millis(60)
            } else {
                SETTLE_WINDOW
            },
            poll: if cfg!(test) {
                Duration::from_millis(5)
            } else {
                SETTLE_POLL
            },
        })
    }
    fn append(&mut self, record: Receipt) -> Result<Receipt> {
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
            return Err(Error::new("journal_full", "No durable receipt capacity"));
        }
        self.poisoned = true;
        self.file.write_all(&line)?;
        self.file.sync_all()?;
        self.receipts.insert(record.id.clone(), record.clone());
        self.poisoned = false;
        Ok(record)
    }
    pub fn receipt_exists(&self, id: &str) -> bool {
        self.receipts.contains_key(id)
    }
    #[cfg(test)]
    pub(super) fn unresolved(&self) -> Option<&Receipt> {
        self.receipts
            .values()
            .find(|r| r.status == Status::Dispatching)
    }
    /// An open intent that blocks new input in `domain`: one in the same domain, or any one
    /// whose domain is global (`*`), or any one at all when `domain` is global.
    ///
    /// Every call closes its own intent before it returns, so an open one was left by a server
    /// that stopped in the middle of an action. Its input may still be in flight, so it blocks
    /// for `ORPHAN_MS`; after that it blocks nothing and the next action closes it as
    /// `Unconfirmed` (`close_orphans`). One uncertain click can never freeze a live game (w25:
    /// one focus click blocked every event for months).
    pub(super) fn unresolved_in(&self, domain: &str) -> Option<&Receipt> {
        self.unresolved_at(domain, now())
    }
    fn unresolved_at(&self, domain: &str, now_ms: u64) -> Option<&Receipt> {
        self.receipts.values().find(|r| {
            r.status == Status::Dispatching && now_ms.saturating_sub(r.unix_ms) < ORPHAN_MS && {
                let theirs = domain_of(r.action["kind"].as_str().unwrap_or(""));
                theirs == domain || domain == "*" || theirs == "*"
            }
        })
    }
    /// Close every open intent older than `ORPHAN_MS`, except `keep` (the request being retried,
    /// which settles itself). Nothing is read and nothing is sent: the record only says that
    /// the outcome was never observed.
    fn close_orphans(&mut self, keep: &str) -> Result<()> {
        let now_ms = now();
        let old: Vec<Receipt> = self
            .receipts
            .values()
            .filter(|r| {
                r.status == Status::Dispatching
                    && r.id != keep
                    && now_ms.saturating_sub(r.unix_ms) >= ORPHAN_MS
            })
            .cloned()
            .collect();
        for intent in old {
            let age_ms = now_ms.saturating_sub(intent.unix_ms);
            self.append(Receipt {
                status: Status::Unconfirmed,
                detail: serde_json::json!({"closed":"orphaned","age_ms":age_ms}),
                unix_ms: now_ms,
                ..intent
            })?;
        }
        Ok(())
    }
    pub(super) fn blocked(&self) -> bool {
        self.poisoned || self.unresolved_in("*").is_some()
    }
    /// Blocked for new input in `domain` (a persistence failure blocks everything).
    pub(super) fn blocked_for(&self, domain: &str) -> bool {
        self.poisoned || self.unresolved_in(domain).is_some()
    }
    /// Only an existing request may take the read-only replay/settlement path.
    /// Persistence failure cannot be cleared by a new lease or request ID.
    pub(super) fn permits_recovery(&self, id: Option<&str>) -> bool {
        !self.poisoned && id.is_some_and(|id| self.receipt_exists(id))
    }
    /// Replay only a verified historical receipt. This path never observes the
    /// current UI to settle an old intent, and never dispatches a second time.
    pub(super) fn strict_receipt(
        &self,
        id: &str,
        epoch: &Epoch,
        action: &Value,
    ) -> Result<Option<Receipt>> {
        if self.poisoned {
            return Err(Error::new(
                "action_uncertain",
                "Game journal persistence is uncertain",
            ));
        }
        valid_id(id)?;
        if let Some(previous) = self.receipts.get(id) {
            if previous.version != 2
                || !same_campaign(&previous.epoch, epoch)
                || &previous.action != action
            {
                return Err(Error::new(
                    "idempotency_conflict",
                    "request_id belongs to another action or epoch",
                ));
            }
            return match previous.status {
                Status::Verified => Ok(Some(previous.clone())),
                Status::NotApplied => Err(Error::new("input_not_queued", "Historical positive refusal; use a fresh request after repairing its cause")),
                Status::Unconfirmed => Err(unconfirmed(previous, None)),
                Status::Dispatching | Status::Diverged => Err(Error::new("action_uncertain", "Historical strict action remains unresolved; current UI cannot settle or replay it")),
            };
        }
        if self.unresolved_in("*").is_some() {
            return Err(Error::new(
                "action_uncertain",
                "An action interrupted moments ago may still have input in flight; new strict input waits a few seconds",
            ));
        }
        Ok(None)
    }
    /// Persist the original typed adapter's private binding/baseline before
    /// invoking it once. Only its same-attempt verified readback can certify
    /// this intent; an uncertain attempt is closed as `Unconfirmed` at once
    /// (the current UI can neither settle nor replay it).
    pub(super) fn execute_strict(
        &mut self,
        id: &str,
        epoch: &Epoch,
        action: Value,
        before: Value,
        dispatch: &mut dyn FnMut() -> StrictOutcome,
    ) -> Result<Receipt> {
        if !self.poisoned {
            self.close_orphans(id)?;
        }
        if let Some(receipt) = self.strict_receipt(id, epoch, &action)? {
            return Ok(receipt);
        }
        let intent = self.append(Receipt {
            version: 2,
            id: id.into(),
            epoch: epoch.clone(),
            action,
            status: Status::Dispatching,
            detail: before,
            unix_ms: now(),
        })?;
        match dispatch() {
            StrictOutcome::Verified(detail) => self
                .append(Receipt {
                    status: Status::Verified,
                    detail,
                    unix_ms: now(),
                    ..intent
                })
                .map_err(|error| {
                    Error::after_possible_input("Strict action receipt was not durable", error)
                }),
            StrictOutcome::NotQueued(error) => {
                // Retain the positive producer classification without copying
                // arbitrary transport messages or lease tokens into a receipt.
                self.append(Receipt {
                    status: Status::NotApplied,
                    detail: serde_json::json!({"delivery":"NotQueued","cause_code":error.code}),
                    unix_ms: now(),
                    ..intent
                })?;
                Err(error)
            }
            StrictOutcome::Uncertain(error) => {
                let closed = self
                    .append(Receipt {
                        status: Status::Unconfirmed,
                        detail: serde_json::json!({"closed":"strict_uncertain","cause_code":error.code}),
                        unix_ms: now(),
                        ..intent
                    })
                    .map_err(|e| {
                        Error::after_possible_input("Strict action receipt was not durable", e)
                    })?;
                Err(unconfirmed(&closed, Some(&error)))
            }
        }
    }
    /// Run `dispatch` once under a durable intent record. `settle` reads fresh
    /// state and reports what the game shows about this action.
    /// Like `execute`, but the intent record stores `baseline` (the state read before any
    /// input) and `settle` is always given that original baseline. A retry of an open
    /// request id can then settle fully (reached, unchanged or diverged) from the durable
    /// baseline instead of a fresh, post-dispatch re-read. Intents written without a baseline
    /// keep the strict "only a reached target settles a retry" rule.
    pub fn execute_baselined(
        &mut self,
        id: &str,
        epoch: &Epoch,
        action: Value,
        baseline: Value,
        settle: &mut dyn FnMut(&Value) -> Result<Outcome>,
        dispatch: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Receipt> {
        if let Some(previous) = self.receipts.get(id).cloned() {
            if previous.version == 1
                && previous.status == Status::Dispatching
                && same_campaign(&previous.epoch, epoch)
                && previous.action == action
                && !self.poisoned
            {
                if let Some(original) = previous.detail.get("baseline").cloned() {
                    let mut stored = || settle(&original);
                    return self.conclude(previous, &mut stored, None);
                }
            }
            let mut legacy = || settle(&baseline);
            return self.execute_with(id, epoch, action, &mut legacy, dispatch, false);
        }
        self.baseline = Some(serde_json::json!({ "baseline": baseline.clone() }));
        let mut fresh = || settle(&baseline);
        let result = self.execute_with(id, epoch, action, &mut fresh, dispatch, false);
        self.baseline = None;
        result
    }
    pub fn execute(
        &mut self,
        id: &str,
        epoch: &Epoch,
        action: Value,
        settle: &mut dyn FnMut() -> Result<Outcome>,
        dispatch: &mut dyn FnMut() -> Result<()>,
    ) -> Result<Receipt> {
        self.execute_with(id, epoch, action, settle, dispatch, false)
    }
    /// Like `execute`. `retry_unchanged` is only for adapters whose `Unchanged`
    /// verdict is a pure function of fresh game state (no before-snapshot): then
    /// a retry of an open request id may also settle as not applied.
    /// Settlement re-reads for the bounded window (torn reads and read errors
    /// included: the game rewrites its memory while running); no input is ever
    /// re-sent.
    pub fn execute_with(
        &mut self,
        id: &str,
        epoch: &Epoch,
        action: Value,
        settle: &mut dyn FnMut() -> Result<Outcome>,
        dispatch: &mut dyn FnMut() -> Result<()>,
        retry_unchanged: bool,
    ) -> Result<Receipt> {
        if self.poisoned {
            return Err(Error::new(
                "action_uncertain",
                "Game journal persistence is uncertain",
            ));
        }
        valid_id(id)?;
        if let Some(previous) = self.receipts.get(id).cloned() {
            if previous.version == 2 {
                return Err(Error::new(
                    "action_uncertain",
                    "Strict action receipts cannot use legacy settlement",
                ));
            }
            if !same_campaign(&previous.epoch, epoch) || previous.action != action {
                return Err(Error::new(
                    "idempotency_conflict",
                    "request_id is bound to a different action or epoch",
                ));
            }
            match previous.status {
                Status::Dispatching => {}
                Status::Unconfirmed => return Err(unconfirmed(&previous, None)),
                _ => return Ok(previous),
            }
            // An intent left open by a stopped server. These adapters reconstruct their
            // before-state on each call, so a later Unchanged/Diverged observation cannot
            // prove the original dispatch failed: only a reached target (or a target proven
            // absent) settles it, unless the adapter's Unchanged verdict needs no baseline.
            // Anything else closes it as Unconfirmed when the window ends.
            let mut retry_settle = || match settle()? {
                reached @ Outcome::Reached(_) => Ok(reached),
                absent @ Outcome::TargetAbsent(_) => Ok(absent),
                unchanged @ Outcome::Unchanged(_) if retry_unchanged => Ok(unchanged),
                _ => Ok(Outcome::Unknown),
            };
            return self.conclude(previous, &mut retry_settle, None);
        }
        self.close_orphans(id)?;
        if let Some(open) = self.unresolved_in(domain_of(action["kind"].as_str().unwrap_or(""))) {
            return Err(Error::new(
                "action_uncertain",
                format!(
                    "Request {} was interrupted moments ago and its input may still be in flight; new input in its domain waits at most {} s",
                    open.id,
                    ORPHAN_MS / 1000
                ),
            ));
        }
        let intent = self.append(Receipt {
            version: 1,
            id: id.into(),
            epoch: epoch.clone(),
            action,
            status: Status::Dispatching,
            detail: self.baseline.clone().unwrap_or(Value::Null),
            unix_ms: now(),
        })?;
        match dispatch() {
            Ok(()) => self.conclude(intent, settle, None),
            Err(error) => {
                // What the game shows decides: the effect is there (the dispatch only failed
                // late), it is provably absent (the dispatch error is the answer), or neither
                // could be read inside the window (closed as Unconfirmed).
                let receipt = self.conclude(intent, settle, Some(&error))?;
                match receipt.status {
                    Status::Verified => Ok(receipt),
                    _ => Err(error),
                }
            }
        }
    }
    /// Settle `intent` and turn an `Unconfirmed` close into its typed error.
    fn conclude(
        &mut self,
        intent: Receipt,
        settle: &mut dyn FnMut() -> Result<Outcome>,
        cause: Option<&Error>,
    ) -> Result<Receipt> {
        let receipt = self.settle(intent, settle, cause).map_err(|error| {
            Error::after_possible_input("Action receipt was not durable", error)
        })?;
        if receipt.status == Status::Unconfirmed {
            return Err(unconfirmed(&receipt, cause));
        }
        Ok(receipt)
    }
    /// Re-observe until the game shows an outcome or the settlement window ends, then write the
    /// terminal receipt. Reads inside the window pass the action fence even after the action's
    /// own deadline. An outcome that stays unknown (or unreadable) closes the intent as
    /// `Unconfirmed`. Only a persistence failure is an error.
    fn settle(
        &mut self,
        intent: Receipt,
        settle: &mut dyn FnMut() -> Result<Outcome>,
        cause: Option<&Error>,
    ) -> Result<Receipt> {
        let started = Instant::now();
        let end = started + self.window;
        let mut polls = 0u32;
        let mut last = String::new();
        let outcome = {
            let _reading = Settling::enter(end);
            loop {
                polls += 1;
                match settle() {
                    Ok(Outcome::Unknown) => {
                        last = "state neither reached the target nor stayed unchanged".into();
                    }
                    Ok(Outcome::Unobservable) => {
                        last = "the action's effect cannot be read back".into();
                        break None;
                    }
                    Ok(outcome) => break Some(outcome),
                    Err(error) => {
                        last = bounded(&format!("{}: {}", error.code, error.message), 200);
                        // A stopped session reads nothing more.
                        if error.code == "session_closed" {
                            break None;
                        }
                    }
                }
                let now = Instant::now();
                if now >= end {
                    break None;
                }
                std::thread::sleep(self.poll.min(end - now));
            }
        };
        let (status, detail) = match outcome {
            Some(Outcome::Reached(v)) => (Status::Verified, v),
            Some(Outcome::Unchanged(v) | Outcome::TargetAbsent(v)) => (Status::NotApplied, v),
            Some(Outcome::Diverged(v)) => (Status::Diverged, v),
            Some(Outcome::Unknown | Outcome::Unobservable) | None => (
                Status::Unconfirmed,
                serde_json::json!({
                    "closed": "settlement_window",
                    "window_ms": started.elapsed().as_millis() as u64,
                    "polls": polls,
                    "readback": last,
                    "cause_code": cause.map(|c| c.code),
                }),
            ),
        };
        self.append(Receipt {
            status,
            detail,
            unix_ms: now(),
            ..intent
        })
    }
}
/// One bounded line without control characters.
fn bounded(text: &str, limit: usize) -> String {
    let mut out = String::new();
    for c in text.chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > limit {
            break;
        }
        out.push(c);
    }
    out
}
/// The typed error for a request closed as `Unconfirmed`: a clean failure, not a quarantine.
fn unconfirmed(receipt: &Receipt, cause: Option<&Error>) -> Error {
    let mut message = String::from(
        "Input may have been sent but its effect was not observed; the request is closed, is \
         never replayed and blocks nothing. Read the state again and send a fresh request only \
         if the effect is absent",
    );
    if let Some(cause) = cause {
        message.push_str(&bounded(
            &format!("; cause={}: {}", cause.code, cause.message),
            260,
        ));
    }
    if let Some(readback) = receipt.detail["readback"]
        .as_str()
        .filter(|r| !r.is_empty())
    {
        message.push_str(&bounded(&format!("; readback={readback}"), 220));
    }
    Error::new("action_unconfirmed", message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::Identity;
    use serde_json::json;

    pub(super) fn epoch() -> Epoch {
        Epoch {
            game: Identity {
                pid: 7,
                start_ticks: "1".into(),
                exe: "/hoi4".into(),
                uid: 501,
            },
            campaign: "n1:c".into(),
            player: "GER".into(),
        }
    }
    pub(super) fn dir(name: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("hoi4-game-journal-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    pub(super) fn journal(dir: &std::path::Path) -> GameJournal {
        GameJournal::open(&dir.join("game-actions.jsonl")).unwrap()
    }

    #[test]
    fn write_failure_poison_cannot_be_cleared_by_restore_or_new_request() {
        let dir = dir("sticky-write-failure");
        let mut j = journal(&dir);
        let good = std::mem::replace(
            &mut j.file,
            File::open(dir.join("game-actions.jsonl")).unwrap(),
        );
        let result = j.execute(
            "failed",
            &epoch(),
            json!({"kind":"fixture"}),
            &mut || panic!("Failed durable intent must not observe"),
            &mut || panic!("Failed durable intent must not dispatch"),
        );
        assert!(result.is_err());
        assert!(j.blocked());
        assert!(!j.permits_recovery(Some("failed")));
        j.file = good;
        for id in ["failed", "new"] {
            assert_eq!(
                j.execute(
                    id,
                    &epoch(),
                    json!({}),
                    &mut || panic!("Poisoned"),
                    &mut || panic!("Poisoned")
                )
                .unwrap_err()
                .code,
                "action_uncertain"
            );
        }
        drop(j);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn truncated_or_oversized_record_is_never_accepted_as_durable() {
        let dir = dir("incomplete-record");
        let path = dir.join("game-actions.jsonl");
        let record = Receipt {
            version: 1,
            id: "partial".into(),
            epoch: epoch(),
            action: json!({}),
            status: Status::Dispatching,
            detail: Value::Null,
            unix_ms: 1,
        };
        std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert_eq!(
            GameJournal::open(&path).err().unwrap().code,
            "invalid_journal"
        );
        std::fs::write(&path, vec![b' '; LINE_LIMIT + 1]).unwrap();
        assert_eq!(
            GameJournal::open(&path).err().unwrap().code,
            "invalid_journal"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn baselined_intent_stores_baseline_and_a_retry_settles_against_it() {
        let dir = dir("baselined");
        let e = epoch();
        let action = json!({"kind":"construction_add","state_id":64});
        {
            let mut j = journal(&dir);
            // The intent record carries the baseline read before any input.
            let mut seen = None;
            let r = j
                .execute_baselined(
                    "c0",
                    &e,
                    action.clone(),
                    json!(1),
                    &mut |b| {
                        seen = Some(b.clone());
                        Ok(Outcome::Reached(json!(2)))
                    },
                    &mut || Ok(()),
                )
                .unwrap();
            assert_eq!((r.status, seen), (Status::Verified, Some(json!(1))));
            // A server that stopped mid-action left this one open, baseline stored.
            orphan(&mut j, "c1", &action, json!({"baseline":1}));
        }
        // Restarted server: the fresh baseline would be 2 (the queue grew), the stored one is 1.
        let mut j = journal(&dir);
        let mut seen = None;
        let r = j
            .execute_baselined(
                "c1",
                &e,
                action,
                json!(2),
                &mut |b| {
                    seen = Some(b.clone());
                    Ok(if b == &json!(1) {
                        Outcome::Reached(json!(2))
                    } else {
                        Outcome::Unchanged(json!(2))
                    })
                },
                &mut || panic!("a retry never dispatches"),
            )
            .unwrap();
        assert_eq!(seen, Some(json!(1)));
        assert_eq!(r.status, Status::Verified);
    }
    #[test]
    fn baselined_retry_can_prove_not_applied_from_the_stored_baseline() {
        let dir = dir("baselined-unchanged");
        let e = epoch();
        let action = json!({"kind":"construction_add","state_id":7});
        let mut j = journal(&dir);
        orphan(&mut j, "c2", &action, json!({"baseline":3}));
        let r = j
            .execute_baselined(
                "c2",
                &e,
                action,
                json!(3),
                &mut |_| Ok(Outcome::Unchanged(json!(3))),
                &mut || panic!("no dispatch"),
            )
            .unwrap();
        assert_eq!(r.status, Status::NotApplied);
    }
    #[test]
    fn legacy_intent_without_baseline_keeps_the_strict_retry_rule() {
        let dir = dir("baselined-legacy");
        let e = epoch();
        let action = json!({"kind":"construction_add","state_id":9});
        let mut j = journal(&dir);
        orphan(&mut j, "c3", &action, Value::Null);
        let err = j
            .execute_baselined(
                "c3",
                &e,
                action,
                json!(0),
                &mut |_| Ok(Outcome::Unchanged(json!(0))),
                &mut || panic!("no dispatch"),
            )
            .unwrap_err();
        assert_eq!(err.code, "action_unconfirmed");
        assert_eq!(j.receipts["c3"].status, Status::Unconfirmed);
    }
    #[test]
    fn a_baselined_dispatch_that_cannot_be_read_back_is_closed_with_its_cause() {
        let dir = dir("baselined-unreadable");
        let mut j = journal(&dir);
        let err = j
            .execute_baselined(
                "c4",
                &epoch(),
                json!({"kind":"construction_add","state_id":64}),
                json!(1),
                &mut |_| Err(Error::new("read_failed", "torn")),
                &mut || Err(Error::new("input_readback_failed", "no change seen")),
            )
            .unwrap_err();
        assert_eq!(err.code, "action_unconfirmed");
        assert!(err.message.contains("cause=input_readback_failed"));
        assert!(err.message.contains("readback=read_failed"));
        assert_eq!(j.receipts["c4"].status, Status::Unconfirmed);
        assert_eq!(
            j.receipts["c4"].detail["cause_code"],
            "input_readback_failed"
        );
    }
    #[test]
    fn positively_observed_unchanged_state_settles_a_refusal() {
        let dir = dir("refusal");
        let mut j = journal(&dir);
        let e = epoch();
        let err = j
            .execute(
                "bad-target",
                &e,
                json!({"kind":"x"}),
                &mut || Ok(Outcome::Unchanged(json!({"verified_unchanged":true}))),
                &mut || Err(Error::new("invalid_target", "no such army")),
            )
            .unwrap_err();
        assert_eq!(err.code, "invalid_target");
        let next = j
            .execute(
                "next",
                &e,
                json!({"kind":"y"}),
                &mut || Ok(Outcome::Reached(json!({}))),
                &mut || Ok(()),
            )
            .unwrap();
        assert_eq!(next.status, Status::Verified);
    }

    #[test]
    fn verified_once_and_replayed_from_the_receipt() {
        let dir = dir("verified");
        let mut j = journal(&dir);
        let mut sent = 0;
        let action = json!({"research":"support_weapons","slot":0});
        let r = j
            .execute(
                "a1",
                &epoch(),
                action.clone(),
                &mut || Ok(Outcome::Reached(json!(1))),
                &mut || {
                    sent += 1;
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(r.status, Status::Verified);
        drop(j);
        let mut j = journal(&dir);
        let again = j
            .execute(
                "a1",
                &epoch(),
                action,
                &mut || unreachable!(),
                &mut || unreachable!(),
            )
            .unwrap();
        assert_eq!((again.status, sent), (Status::Verified, 1));
    }
    #[test]
    fn an_unprovable_dispatch_is_closed_unconfirmed_and_blocks_nothing() {
        let dir = dir("unconfirmed");
        let mut j = journal(&dir);
        let action = json!({"kind":"research_start","research":"radio","slot":1});
        let mut polls = 0;
        let err = j
            .execute(
                "a2",
                &epoch(),
                action.clone(),
                &mut || {
                    polls += 1;
                    Ok(Outcome::Unknown)
                },
                &mut || Err(Error::new("input_readback_failed", "x")),
            )
            .unwrap_err();
        assert_eq!(err.code, "action_unconfirmed");
        assert!(err.message.contains("cause=input_readback_failed"));
        // Re-observed more than once inside the window before giving up.
        assert!(polls > 1, "polls={polls}");
        assert_eq!(j.receipts["a2"].status, Status::Unconfirmed);
        assert_eq!(j.receipts["a2"].detail["closed"], "settlement_window");
        assert!(!j.blocked() && !j.blocked_for("research") && j.unresolved().is_none());
        // The same domain takes new input at once.
        let next = j
            .execute(
                "a3",
                &epoch(),
                json!({"kind":"research_start","research":"radio","slot":1}),
                &mut || Ok(Outcome::Reached(json!(1))),
                &mut || Ok(()),
            )
            .unwrap();
        assert_eq!(next.status, Status::Verified);
        // The closed request stays closed across a restart: never observed or sent again.
        drop(j);
        let mut j = journal(&dir);
        let again = j
            .execute(
                "a2",
                &epoch(),
                action,
                &mut || panic!("closed"),
                &mut || panic!("closed"),
            )
            .unwrap_err();
        assert_eq!(again.code, "action_unconfirmed");
    }
    #[test]
    fn an_effect_that_shows_late_inside_the_window_is_verified() {
        let dir = dir("late-effect");
        let mut j = journal(&dir);
        // The action ran out of time (dispatch error) and the first readbacks fail or see
        // nothing yet; the effect appears a moment later.
        let mut reads = 0;
        let r = j
            .execute(
                "late",
                &epoch(),
                json!({"kind":"focus_start","focus":"f"}),
                &mut || {
                    reads += 1;
                    match reads {
                        1 => Err(Error::new("torn_read", "day tick")),
                        2 => Ok(Outcome::Unknown),
                        _ => Ok(Outcome::Reached(json!({"active":"f"}))),
                    }
                },
                &mut || {
                    Err(Error::new(
                        "action_deadline",
                        "Original action deadline expired",
                    ))
                },
            )
            .unwrap();
        assert_eq!((r.status, reads), (Status::Verified, 3));
        // The opposite: a dispatch error whose effect is provably absent keeps its own code.
        let e = j
            .execute(
                "absent",
                &epoch(),
                json!({"kind":"focus_start","focus":"g"}),
                &mut || Ok(Outcome::Unchanged(json!({"active":null}))),
                &mut || {
                    Err(Error::new(
                        "action_deadline",
                        "Original action deadline expired",
                    ))
                },
            )
            .unwrap_err();
        assert_eq!(e.code, "action_deadline");
        assert_eq!(j.receipts["absent"].status, Status::NotApplied);
        assert!(j.unresolved().is_none());
    }
    #[test]
    fn an_unobservable_outcome_closes_at_once_without_polling() {
        let dir = dir("unobservable");
        let mut j = journal(&dir);
        j.window = Duration::from_secs(30);
        let started = Instant::now();
        let mut polls = 0;
        let err = j
            .execute(
                "u1",
                &epoch(),
                json!({"kind":"construction_order","line":1}),
                &mut || {
                    polls += 1;
                    Ok(Outcome::Unobservable)
                },
                &mut || Err(Error::new("input_readback_failed", "x")),
            )
            .unwrap_err();
        assert_eq!(err.code, "action_unconfirmed");
        assert_eq!(polls, 1);
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_eq!(j.receipts["u1"].status, Status::Unconfirmed);
        assert!(!j.blocked());
    }
    #[test]
    fn settlement_reads_run_inside_the_read_only_window() {
        let dir = dir("window-scope");
        let mut j = journal(&dir);
        assert!(settling_until().is_none());
        let mut inside = (false, false);
        j.execute(
            "w",
            &epoch(),
            json!({"kind":"x"}),
            &mut || {
                inside.1 = settling_until().is_some();
                Ok(Outcome::Reached(json!(1)))
            },
            &mut || {
                inside.0 = settling_until().is_some();
                Ok(())
            },
        )
        .unwrap();
        // The dispatch (input) never runs inside the window; the readback does.
        assert_eq!(inside, (false, true));
        assert!(settling_until().is_none());
    }
    #[test]
    fn a_restarted_server_replays_but_another_campaign_conflicts() {
        let dir = dir("restart");
        let mut j = journal(&dir);
        j.execute(
            "a5",
            &epoch(),
            json!(1),
            &mut || Ok(Outcome::Reached(json!(1))),
            &mut || Ok(()),
        )
        .unwrap();
        let mut restarted = epoch();
        restarted.campaign = "n2:c".into();
        assert!(j
            .execute(
                "a5",
                &restarted,
                json!(1),
                &mut || unreachable!(),
                &mut || unreachable!()
            )
            .is_ok());
        restarted.campaign = "n2:other".into();
        assert_eq!(
            j.execute(
                "a5",
                &restarted,
                json!(1),
                &mut || unreachable!(),
                &mut || unreachable!()
            )
            .unwrap_err()
            .code,
            "idempotency_conflict"
        );
    }
    #[test]
    fn a_request_id_cannot_change_its_action() {
        let dir = dir("conflict");
        let mut j = journal(&dir);
        j.execute(
            "a4",
            &epoch(),
            json!(1),
            &mut || Ok(Outcome::Reached(json!(1))),
            &mut || Ok(()),
        )
        .unwrap();
        let err = j
            .execute(
                "a4",
                &epoch(),
                json!(2),
                &mut || unreachable!(),
                &mut || unreachable!(),
            )
            .unwrap_err();
        assert_eq!(err.code, "idempotency_conflict");
    }

    #[test]
    fn unreadable_outcomes_after_possible_input_close_as_unconfirmed_not_as_a_quarantine() {
        for code in [
            "invalid_target",
            "invalid_arguments",
            "capability_unavailable",
            "gui_target_unavailable",
        ] {
            let dir = dir(code);
            let mut j = journal(&dir);
            let action = json!({"kind":"army_move","army":"Army 1","target":42});
            let mut sent = 0;
            let error = j
                .execute(
                    "partial",
                    &epoch(),
                    action.clone(),
                    &mut || Err(Error::new(code, "readback unavailable")),
                    &mut || {
                        sent += 1;
                        Err(Error::new(code, "lookup failed after click"))
                    },
                )
                .unwrap_err();
            assert_eq!(sent, 1);
            assert_eq!(error.code, "action_unconfirmed");
            assert!(error.message.contains(code));
            assert_eq!(j.receipts["partial"].status, Status::Unconfirmed);
            assert!(j.receipts["partial"].detail["readback"]
                .as_str()
                .unwrap()
                .contains(code));
            drop(j);
            let mut j = journal(&dir);
            // A retry of the closed id never dispatches and never claims an outcome.
            let error = j
                .execute(
                    "partial",
                    &epoch(),
                    action.clone(),
                    &mut || panic!("A closed request is not observed again"),
                    &mut || panic!("Retry must never dispatch"),
                )
                .unwrap_err();
            assert_eq!(error.code, "action_unconfirmed");
            // A fresh request in the same domain is not blocked.
            let fresh = j
                .execute(
                    "new-id",
                    &epoch(),
                    action,
                    &mut || Ok(Outcome::Reached(json!({"target":42}))),
                    &mut || {
                        sent += 1;
                        Ok(())
                    },
                )
                .unwrap();
            assert_eq!((fresh.status, sent), (Status::Verified, 2));
        }
    }

    #[test]
    fn successful_dispatch_with_unavailable_readback_is_closed_unconfirmed() {
        let dir = dir("readback-failed");
        let mut j = journal(&dir);
        let error = j
            .execute(
                "sent",
                &epoch(),
                json!({"army":"A"}),
                &mut || Err(Error::new("invalid_target", "army not observable")),
                &mut || Ok(()),
            )
            .unwrap_err();
        assert_eq!(error.code, "action_unconfirmed");
        assert!(error.message.contains("invalid_target"));
        assert_eq!(j.receipts["sent"].status, Status::Unconfirmed);
        assert!(!j.blocked());
    }

    /// An intent left open by a stopped server (written here as a bare record).
    pub(super) fn orphan(j: &mut GameJournal, id: &str, action: &Value, detail: Value) {
        j.append(Receipt {
            version: 1,
            id: id.into(),
            epoch: epoch(),
            action: action.clone(),
            status: Status::Dispatching,
            detail,
            unix_ms: now(),
        })
        .unwrap();
    }
    #[test]
    fn a_retry_without_the_original_baseline_cannot_prove_nonapplication() {
        for diverged in [false, true] {
            let dir = dir(if diverged {
                "retry-diverged"
            } else {
                "retry-unchanged"
            });
            let mut j = journal(&dir);
            let action = json!({"kind":"construct","state":64});
            orphan(&mut j, "lost", &action, Value::Null);
            drop(j);
            let mut j = journal(&dir);
            let error = j
                .execute(
                    "lost",
                    &epoch(),
                    action.clone(),
                    &mut || {
                        Ok(if diverged {
                            Outcome::Diverged(json!({"queued":1}))
                        } else {
                            Outcome::Unchanged(json!({"queued":1}))
                        })
                    },
                    &mut || panic!("Retry must never dispatch"),
                )
                .unwrap_err();
            // Not provable either way: closed, never recorded as NotApplied or Diverged.
            assert_eq!(error.code, "action_unconfirmed");
            assert_eq!(j.receipts["lost"].status, Status::Unconfirmed);
        }
    }
    #[test]
    fn a_retried_orphan_settles_when_its_target_is_reached() {
        let dir = dir("retry-reached");
        let mut j = journal(&dir);
        let action = json!({"kind":"research_start","research":"radio","slot":1});
        orphan(&mut j, "o1", &action, Value::Null);
        let settled = j
            .execute(
                "o1",
                &epoch(),
                action,
                &mut || Ok(Outcome::Reached(json!({"research":"radio","slot":1}))),
                &mut || unreachable!(),
            )
            .unwrap();
        assert_eq!(settled.status, Status::Verified);
        assert!(j.unresolved().is_none());
    }
}

/// The input domain of an action kind or tool name (`hoi4_` prefix ignored). An uncertain
/// action blocks new input only in its own domain: an army order whose effect is not yet
/// proven does not stop research or production. The clock, events, the peace screens and
/// anything unknown stay global (`*`): an uncertain one may have left a modal window open.
pub(super) fn domain_of(kind: &str) -> &'static str {
    let k = kind.strip_prefix("hoi4_").unwrap_or(kind);
    let first = k.split('_').next().unwrap_or("");
    match first {
        "army" | "armies" | "division" | "train" | "deploy" | "template" => "army",
        "air" => "air",
        "fleet" | "naval" | "navy" => "navy",
        "research" | "technology" | "doctrine" => "research",
        "focus" => "focus",
        "construction" | "railway" => "construction",
        "production" | "design" => "production",
        "politics" | "decision" | "occupation" | "mio" | "special" => "politics",
        "diplomacy" | "faction" | "trade" | "auto" => "diplomacy",
        "agency" | "operative" => "intelligence",
        _ => "*",
    }
}

#[cfg(test)]
mod domain_tests {
    use super::domain_of;
    use super::tests::{dir, epoch, journal};
    use super::*;
    #[test]
    fn an_orphaned_intent_blocks_briefly_and_the_next_action_closes_it() {
        let d = dir("orphan");
        let mut j = journal(&d);
        let t0 = now();
        // A server stopped in the middle of a focus action: its intent is still open.
        j.append(Receipt {
            version: 1,
            id: "f1".into(),
            epoch: epoch(),
            action: serde_json::json!({"kind": "focus_start", "focus": "x"}),
            status: Status::Dispatching,
            detail: Value::Null,
            unix_ms: t0,
        })
        .unwrap();
        assert!(j.unresolved_at("*", t0 + 1_000).is_some());
        assert!(j.unresolved_at("focus", t0 + 1_000).is_some());
        assert!(j.unresolved_at("army", t0 + 1_000).is_none());
        assert!(j.unresolved_at("*", t0 + ORPHAN_MS).is_none());
        assert!(j.unresolved_at("focus", t0 + ORPHAN_MS).is_none());
        // While its input may still be in flight a new focus action waits (typed, bounded).
        let early = j
            .execute(
                "f2",
                &epoch(),
                serde_json::json!({"kind": "focus_start", "focus": "y"}),
                &mut || panic!("blocked before any read"),
                &mut || panic!("blocked before any input"),
            )
            .unwrap_err();
        assert_eq!(early.code, "action_uncertain");
        assert!(early.message.contains("at most 10 s"));
        // Old enough: the next action of any domain closes it and runs.
        j.receipts.get_mut("f1").unwrap().unix_ms = t0 - ORPHAN_MS;
        let next = j
            .execute(
                "f3",
                &epoch(),
                serde_json::json!({"kind": "focus_start", "focus": "y"}),
                &mut || Ok(Outcome::Reached(serde_json::json!(1))),
                &mut || Ok(()),
            )
            .unwrap();
        assert_eq!(next.status, Status::Verified);
        assert_eq!(j.receipts["f1"].status, Status::Unconfirmed);
        assert_eq!(j.receipts["f1"].detail["closed"], "orphaned");
        assert!(j.unresolved().is_none());
        // Its own request id now answers with the closed receipt's typed error, without reads.
        let closed = j
            .execute(
                "f1",
                &epoch(),
                serde_json::json!({"kind": "focus_start", "focus": "x"}),
                &mut || panic!("a closed request is not observed again"),
                &mut || panic!("a closed request is never replayed"),
            )
            .unwrap_err();
        assert_eq!(closed.code, "action_unconfirmed");
        // The closed record survives a restart as a valid transition.
        drop(j);
        let j = journal(&d);
        assert_eq!(j.receipts["f1"].status, Status::Unconfirmed);
    }
    #[test]
    fn kinds_and_tools_map_to_the_same_domains() {
        assert_eq!(domain_of("army_front"), "army");
        assert_eq!(domain_of("hoi4_army_move"), "army");
        assert_eq!(domain_of("division_disband"), "army");
        assert_eq!(domain_of("research_start"), "research");
        assert_eq!(domain_of("hoi4_production_set"), "production");
        assert_eq!(domain_of("design_set_production"), "production");
        assert_eq!(domain_of("event_choose"), "*");
        assert_eq!(domain_of("peace_take"), "*");
        assert_eq!(domain_of("hoi4_time"), "*");
        assert_eq!(domain_of("something_new"), "*");
    }
}
