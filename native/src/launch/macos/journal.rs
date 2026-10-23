//! Durable receipts for ordinary game actions (research, Focus, ...). Intent is
//! written and synced before the first input; a dispatch without a durable
//! outcome is never replayed. Legacy version-1 adapters retain their read-only
//! settlement contract. Strict version-2 intents never settle after an uncertain
//! attempt: only that original attempt may certify delivery or positive no-input.
//! Every unresolved dispatch quarantines shared input.
use crate::{
    actions::Epoch,
    error::{Error, Result},
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const LINE_LIMIT: usize = 16 << 10;
const JOURNAL_LIMIT: u64 = 64 << 20;

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
    Diverged(Value),
    Unknown,
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
    pub(super) fn unresolved(&self) -> Option<&Receipt> {
        self.receipts
            .values()
            .find(|r| r.status == Status::Dispatching)
    }
    pub(super) fn blocked(&self) -> bool {
        self.poisoned || self.unresolved().is_some()
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
                Status::Dispatching | Status::Diverged => Err(Error::new("action_uncertain", "Historical strict action remains unresolved; current UI cannot settle or replay it")),
            };
        }
        if self.unresolved().is_some() {
            return Err(Error::new(
                "action_uncertain",
                "Unresolved shared game input blocks a new strict action",
            ));
        }
        Ok(None)
    }
    /// Persist the original typed adapter's private binding/baseline before
    /// invoking it once. Only its same-attempt verified readback can complete
    /// this intent. Existing legacy adapters retain their separate contract.
    pub(super) fn execute_strict(
        &mut self,
        id: &str,
        epoch: &Epoch,
        action: Value,
        before: Value,
        dispatch: &mut dyn FnMut() -> StrictOutcome,
    ) -> Result<Receipt> {
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
            StrictOutcome::Uncertain(error) => Err(Error::after_possible_input(
                "Strict action outcome is uncertain; preserve original intent without replay",
                error,
            )),
        }
    }
    /// Run `dispatch` once under a durable intent record. `settle` reads fresh
    /// state and reports what the game shows about this action.
    pub fn execute(
        &mut self,
        id: &str,
        epoch: &Epoch,
        action: Value,
        settle: &mut dyn FnMut() -> Result<Outcome>,
        dispatch: &mut dyn FnMut() -> Result<()>,
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
            if previous.status != Status::Dispatching {
                return Ok(previous);
            }
            // A missing target/UI during readback says nothing about whether
            // the earlier dispatch sent input. Only a successful observation
            // may settle it; read errors must leave the intent unresolved.
            // These adapters reconstruct their before-state on each call.
            // A later Unchanged/Diverged observation therefore cannot prove
            // the original dispatch failed. Only a reached target can settle
            // an unresolved retry until original baselines are durable.
            let mut retry_settle = || match settle()? {
                reached @ Outcome::Reached(_) => Ok(reached),
                _ => Ok(Outcome::Unknown),
            };
            return self.settle(previous, &mut retry_settle).map_err(|error| {
                Error::after_possible_input(
                    "Previous dispatch remains unresolved; readback failed",
                    error,
                )
            });
        }
        if let Some(open) = self.unresolved() {
            return Err(Error::new(
                "action_uncertain",
                format!(
                    "Unresolved dispatch {} blocks input; retry that request_id to settle it",
                    open.id
                ),
            ));
        }
        let intent = self.append(Receipt {
            version: 1,
            id: id.into(),
            epoch: epoch.clone(),
            action,
            status: Status::Dispatching,
            detail: Value::Null,
            unix_ms: now(),
        })?;
        match dispatch() {
            Ok(()) => self.settle(intent, settle).map_err(|error| {
                Error::after_possible_input(
                    "Input completed but readback remains unresolved",
                    error,
                )
            }),
            Err(error) => {
                // Settle from fresh state; an unprovable outcome stays quarantined.
                let settled = self.settle(intent.clone(), settle);
                match settled {
                    Ok(receipt) if receipt.status == Status::Verified => Ok(receipt),
                    Ok(_) => Err(error),
                    Err(readback) => Err(Error::after_possible_input(
                        "Dispatch outcome unresolved; preserve intent",
                        Error::new(
                            error.code,
                            format!(
                                "{}; readback_cause={}: {}",
                                error.message, readback.code, readback.message,
                            ),
                        ),
                    )),
                }
            }
        }
    }
    fn settle(
        &mut self,
        intent: Receipt,
        settle: &mut dyn FnMut() -> Result<Outcome>,
    ) -> Result<Receipt> {
        let (status, detail) = match settle()? {
            Outcome::Reached(v) => (Status::Verified, v),
            Outcome::Unchanged(v) => (Status::NotApplied, v),
            Outcome::Diverged(v) => (Status::Diverged, v),
            Outcome::Unknown => {
                return Err(Error::new(
                    "action_uncertain",
                    "Game state neither reached the target nor stayed unchanged",
                ))
            }
        };
        self.append(Receipt {
            status,
            detail,
            unix_ms: now(),
            ..intent
        })
    }
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
    fn unprovable_dispatch_quarantines_until_settled_by_its_own_id() {
        let dir = dir("quarantine");
        let mut j = journal(&dir);
        let action = json!({"research":"radio","slot":1});
        let err = j
            .execute(
                "a2",
                &epoch(),
                action.clone(),
                &mut || Ok(Outcome::Unknown),
                &mut || Err(Error::new("input_readback_failed", "x")),
            )
            .unwrap_err();
        assert_eq!(err.code, "action_uncertain");
        assert!(err.message.contains("input_readback_failed"));
        let blocked = j
            .execute(
                "a3",
                &epoch(),
                json!({}),
                &mut || unreachable!(),
                &mut || unreachable!(),
            )
            .unwrap_err();
        assert_eq!(blocked.code, "action_uncertain");
        drop(j);
        let mut j = journal(&dir);
        let settled = j
            .execute(
                "a2",
                &epoch(),
                action,
                &mut || Ok(Outcome::Reached(json!({"research":"radio","slot":1}))),
                &mut || unreachable!(),
            )
            .unwrap();
        assert_eq!(settled.status, Status::Verified);
        assert!(j
            .execute(
                "a3",
                &epoch(),
                json!({}),
                &mut || Ok(Outcome::Reached(json!(1))),
                &mut || Ok(())
            )
            .is_ok());
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
    fn lookup_errors_after_possible_input_never_clear_the_intent_even_after_restart() {
        for code in [
            "invalid_target",
            "invalid_arguments",
            "capability_unavailable",
            "gui_target_unavailable",
        ] {
            let dir = dir(code);
            let mut j = journal(&dir);
            let action = json!({"army":"Army 1","target":42});
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
            assert_eq!(error.code, "action_uncertain");
            assert!(error.message.contains(code));
            assert_eq!(j.receipts["partial"].status, Status::Dispatching);
            drop(j);
            let mut j = journal(&dir);
            let error = j
                .execute(
                    "partial",
                    &epoch(),
                    action.clone(),
                    &mut || Err(Error::new(code, "target no longer observable")),
                    &mut || panic!("Retry must never dispatch"),
                )
                .unwrap_err();
            assert_eq!(error.code, "action_uncertain");
            assert_eq!(j.receipts["partial"].status, Status::Dispatching);
            let blocked = j
                .execute(
                    "new-id",
                    &epoch(),
                    json!({}),
                    &mut || panic!("Unresolved input blocks new observations"),
                    &mut || panic!("Unresolved input blocks new input"),
                )
                .unwrap_err();
            assert_eq!(blocked.code, "action_uncertain");
            let settled = j
                .execute(
                    "partial",
                    &epoch(),
                    action,
                    &mut || Ok(Outcome::Reached(json!({"target":42}))),
                    &mut || panic!("Settlement is read-only"),
                )
                .unwrap();
            assert_eq!(settled.status, Status::Verified);
            assert_eq!(sent, 1);
        }
    }

    #[test]
    fn successful_dispatch_with_unavailable_readback_is_still_uncertain() {
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
        assert_eq!(error.code, "action_uncertain");
        assert!(error.message.contains("invalid_target"));
        assert_eq!(j.receipts["sent"].status, Status::Dispatching);
    }

    #[test]
    fn a_retry_without_the_original_baseline_cannot_prove_nonapplication() {
        let dir = dir("retry-baseline");
        let mut j = journal(&dir);
        let action = json!({"kind":"construct","state":64});
        j.execute(
            "lost",
            &epoch(),
            action.clone(),
            &mut || Ok(Outcome::Unknown),
            &mut || Ok(()),
        )
        .unwrap_err();
        drop(j);
        let mut j = journal(&dir);
        for diverged in [false, true] {
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
            assert_eq!(error.code, "action_uncertain");
            assert_eq!(j.receipts["lost"].status, Status::Dispatching);
        }
        let error = j
            .execute(
                "replacement",
                &epoch(),
                action,
                &mut || panic!("Must not attempt a replacement"),
                &mut || panic!("Must not dispatch a replacement"),
            )
            .unwrap_err();
        assert_eq!(error.code, "action_uncertain");
    }
}
