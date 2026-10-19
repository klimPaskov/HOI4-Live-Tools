//! Durable receipts for ordinary game actions (research, Focus, ...). Intent is
//! written and synced before the first input; a dispatch without a durable
//! outcome is never replayed. A retry of that request may only settle it from
//! fresh game state: the target reached (Verified) or the game state provably
//! unchanged (NotApplied). Any other unresolved dispatch quarantines input.
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
    io::{BufRead, BufReader, Write},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

const LINE_LIMIT: usize = 16 << 10;
const JOURNAL_LIMIT: u64 = 64 << 20;

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

pub struct GameJournal {
    file: File,
    receipts: BTreeMap<String, Receipt>,
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
        for line in BufReader::new(file.try_clone()?).split(b'\n') {
            let line = line?;
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
            valid_id(&record.id)?;
            if let Some(previous) = receipts.get(&record.id) {
                if previous.status != Status::Dispatching
                    || record.status == Status::Dispatching
                    || previous.epoch != record.epoch
                    || previous.action != record.action
                {
                    return Err(Error::new("invalid_journal", "Invalid receipt transition"));
                }
            } else if record.status != Status::Dispatching {
                return Err(Error::new("invalid_journal", "Outcome without intent"));
            }
            receipts.insert(record.id.clone(), record);
        }
        Ok(Self { file, receipts })
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
        self.file.write_all(&line)?;
        self.file.sync_all()?;
        self.receipts.insert(record.id.clone(), record.clone());
        Ok(record)
    }
    pub fn receipt_exists(&self, id: &str) -> bool {
        self.receipts.contains_key(id)
    }
    fn unresolved(&self) -> Option<&Receipt> {
        self.receipts
            .values()
            .find(|r| r.status == Status::Dispatching)
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
        valid_id(id)?;
        if let Some(previous) = self.receipts.get(id).cloned() {
            if !same_campaign(&previous.epoch, epoch) || previous.action != action {
                return Err(Error::new(
                    "idempotency_conflict",
                    "request_id is bound to a different action or epoch",
                ));
            }
            if previous.status != Status::Dispatching {
                return Ok(previous);
            }
            return match self.settle(previous.clone(), settle) {
                // Its state cannot even be read (the target never existed):
                // nothing was sent, so settle it as not applied.
                Err(error)
                    if matches!(
                        error.code,
                        "invalid_target"
                            | "invalid_arguments"
                            | "capability_unavailable"
                            | "gui_target_unavailable"
                    ) =>
                {
                    self.append(Receipt {
                        status: Status::NotApplied,
                        detail: serde_json::json!({"refused": error.message}),
                        unix_ms: now(),
                        ..previous
                    })
                }
                other => other,
            };
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
            Ok(()) => self.settle(intent, settle),
            Err(error) => {
                // Settle from fresh state; an unprovable outcome stays quarantined.
                let settled = self.settle(intent.clone(), settle);
                match settled {
                    Ok(receipt) if receipt.status == Status::Verified => Ok(receipt),
                    Ok(_) => Err(error),
                    Err(_) => {
                        // A refusal raised before any input (unknown army, bad
                        // argument) proves nothing was sent; settling it as not
                        // applied keeps one bad call from quarantining input.
                        if matches!(
                            error.code,
                            "invalid_target"
                                | "invalid_arguments"
                                | "capability_unavailable"
                                | "gui_target_unavailable"
                        ) {
                            let _ = self.append(Receipt {
                                status: Status::NotApplied,
                                detail: serde_json::json!({"refused": error.message.clone()}),
                                unix_ms: now(),
                                ..intent
                            });
                        }
                        Err(error)
                    }
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

    fn epoch() -> Epoch {
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
    fn dir(name: &str) -> std::path::PathBuf {
        let d =
            std::env::temp_dir().join(format!("hoi4-game-journal-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }
    fn journal(dir: &std::path::Path) -> GameJournal {
        GameJournal::open(&dir.join("game-actions.jsonl")).unwrap()
    }

    #[test]
    fn refusal_before_input_does_not_quarantine() {
        let dir = dir("refusal");
        let mut j = journal(&dir);
        let e = epoch();
        let err = j
            .execute(
                "bad-target",
                &e,
                json!({"kind":"x"}),
                &mut || Err(Error::new("invalid_target", "no such army")),
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
        assert_eq!(err.code, "input_readback_failed");
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
                &mut || Ok(Outcome::Unchanged(json!(0))),
                &mut || unreachable!(),
            )
            .unwrap();
        assert_eq!(settled.status, Status::NotApplied);
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
}
