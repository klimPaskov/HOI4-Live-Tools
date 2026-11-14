//! Durable single-stage ordinary research attempts with borrowed input authority.
//! The adapter supplies fresh own state/candidates and one ordinary dispatcher.
//! No MCP registration, engine invocation or automatic input replay occurs here.
//!
//! Every intent ends in a terminal receipt before `execute` returns (`stage_settle`): after
//! the one input the journal keeps observing, reading only, and records `Verified`,
//! `Diverged` or `Unconfirmed`. A request closed without proof answers `action_unconfirmed`,
//! is never replayed and holds nothing. Only an intent left open by a stopped server holds
//! its game, for a bounded time; the next request closes it.
use crate::{
    actions::Epoch,
    error::{Error, Result},
    input_lease::InputAuthority,
    platform::Identity,
    research_attempt::PointCandidate,
    stage_settle::{self, Seen, Settled},
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
    sync::RwLock,
    time::{Duration, Instant},
};

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) mod close_read_recovery;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Stage {
    OpenPanel,
    ClosePanel,
    SelectSlot,
    SelectCategory,
    View,
    Research,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Delivery {
    NotQueued,
    Unknown,
    ServerConfirmed,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub epoch: Epoch,
    pub stage: Stage,
    pub technology: String,
    #[serde(default)]
    pub folder_id: Option<String>,
    pub slot: u32,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OwnState {
    pub epoch: Epoch,
    pub selected_slot: Option<u32>,
    #[serde(default)]
    pub selected_folder: Option<String>,
    pub tree_visible: bool,
    /// Older View/Research receipts did not store this diagnostic; it is never
    /// inferred true when reopening their journal.
    #[serde(default)]
    pub panel_visible: Option<bool>,
    #[serde(default)]
    pub active_mode: Option<i32>,
    /// Complete own-slot canonical assignments, never inferred from UI labels.
    pub researching: Vec<Option<String>>,
    /// All locally visible own Info windows. Null means unknown selection.
    pub visible_info: Vec<Option<String>>,
    /// Private original identity proof; omitted for legacy stage receipts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel_binding: Option<PanelBinding>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PanelBinding {
    pub anchors: [u64; 6],
    pub slots: Vec<u64>,
    /// Research root, tree root, facilities view and facilities root, in order.
    pub roots: [u64; 4],
    pub facilities_parent: u64,
    pub selected_slot_pointer: u64,
    /// The empty tree constructor uses zero; populated arrays require an in-bounds index.
    pub folder_index: i32,
    pub folder_windows: Vec<u64>,
    pub tab: i32,
}
impl PanelBinding {
    fn validate(&self, state: &OwnState) -> Result<()> {
        fn pointer(p: u64) -> bool {
            (0x10000..=0x7fff_ffff_0000).contains(&p) && p % 8 == 0
        }
        let mut slots = std::collections::BTreeSet::new();
        let mut folders = std::collections::BTreeSet::new();
        if !(1..=32).contains(&state.researching.len())
            || self.slots.len() != state.researching.len()
            || !self
                .anchors
                .iter()
                .chain(self.roots.iter())
                .all(|p| pointer(*p))
            || !pointer(self.facilities_parent)
            || !self.slots.iter().all(|p| pointer(*p) && slots.insert(*p))
            || self.folder_windows.len() > 128
            || !self
                .folder_windows
                .iter()
                .all(|p| pointer(*p) && folders.insert(*p))
            || !(0..=1).contains(&self.tab)
            || match state.selected_slot {
                Some(index) => self.slots.get(index as usize) != Some(&self.selected_slot_pointer),
                None => self.selected_slot_pointer != 0,
            }
            || if self.folder_windows.is_empty() {
                self.folder_index != 0 || state.selected_folder.is_some()
            } else {
                self.folder_index < 0
                    || self.folder_index as usize >= self.folder_windows.len()
                    || state.selected_folder.is_none()
            }
        {
            return Err(error(
                "research_precondition",
                "Complete bounded own research panel binding required",
            ));
        }
        Ok(())
    }
}
fn error(code: &'static str, message: &str) -> Error {
    Error::new(code, message)
}
fn key(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(error(
            "invalid_arguments",
            "Bounded canonical technology key required",
        ));
    }
    Ok(())
}
impl Target {
    pub(crate) fn validate_key(&self) -> Result<()> {
        if self.stage == Stage::ClosePanel {
            if !self.technology.is_empty() || self.folder_id.is_some() || self.slot != 0 {
                return Err(error(
                    "invalid_arguments",
                    "ClosePanel requires empty technology, no folder and slot zero",
                ));
            }
        } else if self.stage == Stage::SelectCategory {
            if !self.technology.is_empty() {
                return Err(error(
                    "invalid_arguments",
                    "SelectCategory does not bind technology to folder",
                ));
            }
            key(self
                .folder_id
                .as_deref()
                .ok_or_else(|| error("invalid_arguments", "Explicit folder ID required"))?)?;
        } else {
            key(&self.technology)?;
            if self.folder_id.is_some() {
                return Err(error(
                    "invalid_arguments",
                    "Folder ID belongs only to SelectCategory",
                ));
            }
        }
        Ok(())
    }
}
impl OwnState {
    pub(crate) fn validate(&self, target: &Target) -> Result<()> {
        if self.epoch != target.epoch
            || self.researching.is_empty()
            || self.researching.len() > 32
            || target.slot as usize >= self.researching.len()
            || self.visible_info.len() > 64
            || self
                .selected_slot
                .is_some_and(|s| s as usize >= self.researching.len())
        {
            return Err(error(
                "ownership_changed",
                "Research own context or slot changed",
            ));
        }
        target.validate_key()?;
        self.validate_observed_panel()?;
        if self.panel_binding.is_none() && target.stage == Stage::ClosePanel {
            return Err(error(
                "research_precondition",
                "ClosePanel requires original panel identity proof",
            ));
        }
        Ok(())
    }
    /// Navigation-only validation: no technology, requested slot or assignment
    /// goal exists. Complete positive panel identity is still required.
    pub(crate) fn validate_navigation(&self, epoch: &Epoch) -> Result<()> {
        if &self.epoch != epoch
            || !(1..=32).contains(&self.researching.len())
            || self.visible_info.len() > 64
            || self
                .selected_slot
                .is_some_and(|s| s as usize >= self.researching.len())
        {
            return Err(error(
                "ownership_changed",
                "Research navigation own context changed",
            ));
        }
        self.validate_observed_panel()?;
        if self.panel_binding.is_none() {
            return Err(error(
                "research_precondition",
                "Research navigation requires original panel identity proof",
            ));
        }
        Ok(())
    }
    fn validate_observed_panel(&self) -> Result<()> {
        if let Some(folder) = &self.selected_folder {
            key(folder)?;
        }
        for id in self
            .researching
            .iter()
            .chain(self.visible_info.iter())
            .flatten()
        {
            key(id)?;
        }
        if let Some(binding) = &self.panel_binding {
            binding.validate(self)?;
        }
        Ok(())
    }
    pub(crate) fn reached(&self, target: &Target) -> bool {
        match target.stage {
            Stage::ClosePanel => {
                self.active_mode == Some(23)
                    && self.panel_visible == Some(false)
                    && !self.tree_visible
                    && self.visible_info.is_empty()
                    && self
                        .panel_binding
                        .as_ref()
                        .is_some_and(|b| b.tab == 0 && b.validate(self).is_ok())
            }
            Stage::OpenPanel => {
                self.active_mode == Some(5)
                    && self.panel_visible == Some(true)
                    && !self.tree_visible
                    && self.visible_info.is_empty()
                    && self.researching[target.slot as usize].is_none()
            }
            Stage::SelectSlot => {
                self.tree_visible
                    && self.panel_visible == Some(false)
                    && self.selected_slot == Some(target.slot)
                    && self.researching[target.slot as usize].is_none()
                    && self.visible_info.is_empty()
            }
            Stage::SelectCategory => {
                self.active_mode == Some(6)
                    && self.tree_visible
                    && self.panel_visible == Some(false)
                    && self.selected_slot == Some(target.slot)
                    && self.researching[target.slot as usize].is_none()
                    && self.visible_info.is_empty()
                    && self.selected_folder.as_deref() == target.folder_id.as_deref()
            }
            Stage::Research => {
                self.researching[target.slot as usize].as_deref() == Some(&target.technology)
            }
            Stage::View => {
                self.tree_visible
                    && self.selected_slot == Some(target.slot)
                    && self.visible_info.as_slice() == [Some(target.technology.clone())]
            }
        }
    }
    pub(crate) fn preflight(&self, target: &Target) -> Result<()> {
        if target.stage == Stage::ClosePanel {
            if self.active_mode != Some(5)
                || self.panel_visible != Some(true)
                || self.tree_visible
                || !self.visible_info.is_empty()
                || !self
                    .panel_binding
                    .as_ref()
                    .is_some_and(|b| b.tab == 0 && b.validate(self).is_ok())
            {
                return Err(error("research_precondition", "Shown own Research tab, no tree or Info and complete original panel proof required"));
            }
            return Ok(());
        }
        if target.stage == Stage::OpenPanel {
            if self.active_mode != Some(23)
                || self.panel_visible != Some(false)
                || self.tree_visible
                || !self.visible_info.is_empty()
                || self.researching[target.slot as usize].is_some()
            {
                return Err(error(
                    "research_precondition",
                    "Known closed own research views with no managed view selected required",
                ));
            }
            return Ok(());
        }
        if target.stage == Stage::SelectSlot {
            if self.panel_visible != Some(true)
                || self.tree_visible
                || self.researching[target.slot as usize].is_some()
                || !self.visible_info.is_empty()
            {
                return Err(error(
                    "research_precondition",
                    "Open own panel, empty requested slot and no Info required",
                ));
            }
            return Ok(());
        }
        if target.stage == Stage::SelectCategory {
            if self.active_mode != Some(6)
                || !self.tree_visible
                || self.panel_visible != Some(false)
                || self.selected_slot != Some(target.slot)
                || self.researching[target.slot as usize].is_some()
                || !self.visible_info.is_empty()
                || self.selected_folder.is_none()
            {
                return Err(error(
                    "research_precondition",
                    "Known own folder, selected empty own slot and no Info required",
                ));
            }
            return Ok(());
        }
        if !self.tree_visible
            || self.selected_slot != Some(target.slot)
            || self.researching[target.slot as usize].is_some()
        {
            return Err(error(
                "research_precondition",
                "Open own tree and selected empty slot required",
            ));
        }
        let info_ok = match target.stage {
            Stage::OpenPanel | Stage::ClosePanel | Stage::SelectSlot | Stage::SelectCategory => {
                unreachable!("Selection handled above")
            }
            Stage::View => self.visible_info.is_empty(),
            Stage::Research => self.visible_info.as_slice() == [Some(target.technology.clone())],
        };
        if !info_ok {
            return Err(error(
                "research_precondition",
                "Own Info state differs from requested stage",
            ));
        }
        Ok(())
    }
    /// Closing may change only the shown Research panel to the closed map mode.
    /// An unchanged shown state remains possible while waiting for readback.
    pub(crate) fn preserves_close(&self, original: &Self) -> Result<()> {
        for state in [original, self] {
            let binding = state.panel_binding.as_ref().ok_or_else(|| {
                error(
                    "research_precondition",
                    "Original research panel binding missing",
                )
            })?;
            binding.validate(state)?;
            if binding.tab != 0
                || state.tree_visible
                || !state.visible_info.is_empty()
                || !matches!(
                    (state.active_mode, state.panel_visible),
                    (Some(5), Some(true)) | (Some(23), Some(false))
                )
            {
                return Err(error(
                    "research_precondition",
                    "Research close panel state changed outside permitted transition",
                ));
            }
        }
        let mut normalized = self.clone();
        normalized.active_mode = original.active_mode;
        normalized.panel_visible = original.panel_visible;
        if normalized != *original
            || (original.active_mode == Some(23) && self.active_mode != Some(23))
        {
            return Err(error(
                "research_precondition",
                "Original research panel identity or own state changed",
            ));
        }
        Ok(())
    }
}
/// Reject contradictory newer command-body evidence before any ordinary input.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn join_category_body(
    old: &OwnState,
    mut latest: OwnState,
    target: &Target,
    selected_empty: bool,
    folder: &str,
) -> Result<OwnState> {
    old.validate(target)?;
    latest.validate(target)?;
    key(folder)?;
    if target.stage != Stage::SelectCategory
        || old.selected_slot != latest.selected_slot
        || old.researching != latest.researching
        || old.visible_info != latest.visible_info
        || selected_empty != old.researching[target.slot as usize].is_none()
    {
        return Err(error(
            "torn_read",
            "Newer category body contradicts canonical own state",
        ));
    }
    latest.selected_folder = Some(folder.to_owned());
    latest.preflight(target)?;
    Ok(latest)
}

#[derive(Clone, Debug)]
pub enum Candidate {
    /// Fixed ordinary shortcut; no coordinates or arbitrary key arguments.
    OpenPanel,
    ClosePanel,
    Point(PointCandidate),
}
pub trait Backend {
    fn observe(&mut self) -> Result<OwnState>;
    fn candidate(&mut self, target: &Target) -> Result<Candidate>;
    /// Exactly one fixed shortcut or motion/left-button attempt; no activation or replay.
    fn dispatch_once(&mut self, candidate: &Candidate, deadline: Instant) -> Delivery;
    /// Read-only acknowledgement of clear input; never compensate/release.
    fn confirm_input_idle(&mut self, candidate: &Candidate, deadline: Instant) -> Result<()>;
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Status {
    Intent,
    NotQueued,
    Verified,
    /// Input may have been sent and a settled state contradicts the stage (the close key
    /// changed what it must preserve). Closed like `Unconfirmed`.
    Diverged,
    /// Input may have been sent and the observation ended without proof of the target.
    /// Closed: never replayed, blocks nothing. The caller reads the state again.
    Unconfirmed,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub version: u8,
    pub request_id: String,
    pub target: Target,
    pub status: Status,
    pub delivery: Option<Delivery>,
    /// For `Diverged` and `Unconfirmed` this stays the state the intent recorded before the
    /// input; `closed` says what was seen afterwards.
    pub observed: OwnState,
    /// When the intent was written; bounds how long an interrupted intent holds its game.
    /// Absent on records of older builds (their serialized shape is kept).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unix_ms: Option<u64>,
    /// Why a `Diverged` or `Unconfirmed` record closed (one bounded line).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed: Option<String>,
}
#[derive(Debug, Serialize)]
pub struct Execution {
    #[serde(serialize_with = "serialize_public_receipt")]
    pub receipt: Receipt,
    pub historical: bool,
    pub current_state_verified: bool,
    pub event_causality_verified: bool,
}
fn serialize_public_receipt<S: serde::Serializer>(
    receipt: &Receipt,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error> {
    let mut public = receipt.clone();
    public.observed.panel_binding = None;
    public.serialize(serializer)
}
#[cfg(any(target_os = "linux", test))]
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum FailureEvidence {
    BeforeIntent,
    ConfirmedNotQueued,
    Unknown,
}
pub struct Journal {
    file: File,
    receipts: BTreeMap<String, Receipt>,
    poisoned: bool,
}
const LIMIT: u64 = 64 << 20;
const LINE: usize = 16 << 10;
fn request_id(id: &str) -> Result<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.:/".contains(&b))
    {
        return Err(error(
            "invalid_arguments",
            "Bounded stable request ID required",
        ));
    }
    Ok(())
}
fn permit(gate: &RwLock<bool>) -> Result<std::sync::RwLockReadGuard<'_, bool>> {
    let p = gate
        .try_read()
        .map_err(|_| error("session_closed", "Input gate is closing or unavailable"))?;
    if !*p {
        return Err(error("session_closed", "Owned input session closed"));
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
    authority.authorize(token, &target.epoch)?;
    drop(permit(gate)?);
    if Instant::now() >= deadline {
        return Err(error(
            "research_deadline",
            "Research attempt deadline reached",
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
                "Private regular bounded journal required",
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
            .map_err(|_| error("journal_busy", "One research journal writer required"))?;
        file.sync_all()?;
        #[cfg(unix)]
        File::open(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new(".")),
        )?
        .sync_all()?;
        let mut reader = BufReader::new(file.try_clone()?);
        let mut receipts: BTreeMap<String, Receipt> = BTreeMap::new();
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
                        "Research journal line exceeds bound",
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
            let receipt: Receipt = serde_json::from_slice(&line)
                .map_err(|_| error("invalid_journal", "Malformed research receipt"))?;
            request_id(&receipt.request_id)?;
            receipt.target.validate_key()?;
            receipt.observed.validate(&receipt.target)?;
            if receipt.version != 1
                || (receipt.status == Status::Verified
                    && !receipt.observed.reached(&receipt.target))
                || (receipt.status == Status::Intent && receipt.delivery.is_some())
                || (receipt.status == Status::NotQueued
                    && receipt.delivery != Some(Delivery::NotQueued))
                || !closed_shape(&receipt)
            {
                return Err(error("invalid_journal", "Invalid research receipt witness"));
            }
            if receipt.target.stage == Stage::ClosePanel {
                receipt
                    .observed
                    .preserves_close(&receipt.observed)
                    .map_err(|_| {
                        error("invalid_journal", "Invalid original research close proof")
                    })?;
                if receipt.status == Status::Intent {
                    receipt.observed.preflight(&receipt.target).map_err(|_| {
                        error(
                            "invalid_journal",
                            "Research close intent requires shown original panel",
                        )
                    })?;
                }
            }
            if let Some(previous) = receipts.get(&receipt.request_id) {
                if previous.status != Status::Intent
                    || receipt.status == Status::Intent
                    || previous.target != receipt.target
                {
                    return Err(error(
                        "invalid_journal",
                        "Invalid research receipt transition",
                    ));
                }
                if (receipt.status == Status::Verified
                    && !matches!(
                        receipt.delivery,
                        Some(Delivery::Unknown | Delivery::ServerConfirmed)
                    ))
                    || (matches!(
                        receipt.status,
                        Status::NotQueued | Status::Diverged | Status::Unconfirmed
                    ) && receipt.observed != previous.observed)
                {
                    return Err(error(
                        "invalid_journal",
                        "Research terminal record contradicts durable intent",
                    ));
                }
                if receipt.target.stage == Stage::ClosePanel {
                    receipt
                        .observed
                        .preserves_close(&previous.observed)
                        .map_err(|_| {
                            error(
                                "invalid_journal",
                                "Research close terminal contradicts original panel proof",
                            )
                        })?;
                }
            } else if matches!(
                receipt.status,
                Status::NotQueued | Status::Diverged | Status::Unconfirmed
            ) || (receipt.status == Status::Verified && receipt.delivery.is_some())
            {
                return Err(error("invalid_journal", "Missing durable research intent"));
            }
            receipts.insert(receipt.request_id.clone(), receipt);
        }
        Ok(Self {
            file,
            receipts,
            poisoned: false,
        })
    }
    fn append(&mut self, receipt: Receipt) -> Result<Receipt> {
        if self.poisoned {
            return Err(error(
                "journal_failed",
                "Research journal requires recovery",
            ));
        }
        let mut line = serde_json::to_vec(&receipt)
            .map_err(|_| error("invalid_journal", "Cannot serialize receipt"))?;
        line.push(b'\n');
        if line.len() > LINE
            || self
                .file
                .metadata()?
                .len()
                .saturating_add(line.len() as u64)
                > LIMIT
        {
            return Err(error(
                "journal_full",
                "No research attempt without durable capacity",
            ));
        }
        self.poisoned = true;
        self.file.write_all(&line)?;
        self.file.sync_all()?;
        self.receipts
            .insert(receipt.request_id.clone(), receipt.clone());
        self.poisoned = false;
        Ok(receipt)
    }
    /// A persistence failure holds everything. Otherwise only an intent an interrupted server
    /// left open holds its game, and only for `stage_settle::ORPHAN_MS`: every call closes
    /// its own intent before it returns.
    pub fn blocks_game(&self, game: &Identity) -> bool {
        let now = stage_settle::now_ms();
        self.poisoned
            || self.receipts.values().any(|r| {
                r.status == Status::Intent
                    && &r.target.epoch.game == game
                    && stage_settle::holds(r.unix_ms, now)
            })
    }
    /// Close every intent an interrupted server left open once it no longer holds its game.
    /// Nothing is read and nothing is sent: the record only says the outcome was never
    /// observed.
    fn close_orphans(&mut self) -> Result<()> {
        let now = stage_settle::now_ms();
        let stale: Vec<Receipt> = self
            .receipts
            .values()
            .filter(|r| r.status == Status::Intent && !stage_settle::holds(r.unix_ms, now))
            .cloned()
            .collect();
        for intent in stale {
            self.append(Receipt {
                status: Status::Unconfirmed,
                closed: Some("orphaned: the server stopped before the outcome was observed".into()),
                ..intent
            })?;
        }
        Ok(())
    }
    /// Only an immediate error from execute may use this classification.
    /// execute calls dispatch_once only after a durable Intent. A request closed without
    /// proof (`action_unconfirmed`) may have sent input: its evidence is Unknown.
    #[cfg(any(target_os = "linux", test))]
    pub(crate) fn failure_evidence(
        &self,
        id: &str,
        target: &Target,
        error: &Error,
    ) -> FailureEvidence {
        if self.poisoned
            || self.blocks_game(&target.epoch.game)
            || matches!(error.code, "action_uncertain" | "action_unconfirmed")
        {
            return FailureEvidence::Unknown;
        }
        match self.receipts.get(id) {
            None => FailureEvidence::BeforeIntent,
            Some(r)
                if r.target == *target
                    && r.status == Status::NotQueued
                    && r.delivery == Some(Delivery::NotQueued) =>
            {
                FailureEvidence::ConfirmedNotQueued
            }
            _ => FailureEvidence::Unknown,
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &mut self,
        id: &str,
        target: Target,
        token: &str,
        authority: &InputAuthority,
        gate: &RwLock<bool>,
        backend: &mut dyn Backend,
        timeout: Duration,
    ) -> Result<Execution> {
        request_id(id)?;
        target.validate_key()?;
        if timeout.is_zero() || timeout > Duration::from_secs(10) || target.slot >= 32 {
            return Err(error(
                "invalid_arguments",
                "Bounded stage timeout and slot required",
            ));
        }
        let deadline = (Instant::now() + timeout).min(authority.authorize(token, &target.epoch)?);
        live(authority, token, &target, deadline, gate)?;
        if self.poisoned {
            return Err(error(
                "action_uncertain",
                "Research journal requires recovery",
            ));
        }
        if let Some(r) = self.receipts.get(id).cloned() {
            if r.target != target {
                return Err(error(
                    "request_id_conflict",
                    "Request token is bound to another stage/target",
                ));
            }
            // A closed request answers from its receipt: no read, no input.
            return match r.status {
                Status::Verified => Ok(Execution {
                    receipt: r,
                    historical: true,
                    current_state_verified: false,
                    event_causality_verified: false,
                }),
                Status::NotQueued => Err(error(
                    "input_not_queued",
                    "Historical local refusal; preserved plan needs a fresh attempt token",
                )),
                Status::Diverged => Err(stage_settle::diverged(r.closed.as_deref().unwrap_or(""))),
                Status::Unconfirmed => {
                    Err(stage_settle::unconfirmed(r.closed.as_deref().unwrap_or("")))
                }
                Status::Intent if stage_settle::holds(r.unix_ms, stage_settle::now_ms()) => {
                    Err(stage_settle::interrupted("research"))
                }
                Status::Intent => {
                    self.close_orphans()?;
                    Err(stage_settle::unconfirmed(
                        self.receipts
                            .get(id)
                            .and_then(|r| r.closed.as_deref())
                            .unwrap_or(""),
                    ))
                }
            };
        }
        self.close_orphans()?;
        if self.blocks_game(&target.epoch.game) {
            return Err(stage_settle::interrupted("research"));
        }
        live(authority, token, &target, deadline, gate)?;
        let before = backend.observe()?;
        live(authority, token, &target, deadline, gate)?;
        before.validate(&target)?;
        if before.reached(&target) {
            let receipt = self.append(Receipt {
                version: 1,
                request_id: id.into(),
                target,
                status: Status::Verified,
                delivery: None,
                observed: before,
                unix_ms: None,
                closed: None,
            })?;
            return Ok(Execution {
                receipt,
                historical: false,
                current_state_verified: true,
                event_causality_verified: false,
            });
        }
        before.preflight(&target)?;
        live(authority, token, &target, deadline, gate)?;
        let candidate = backend.candidate(&target)?;
        match &candidate {
            Candidate::OpenPanel if target.stage == Stage::OpenPanel => {}
            Candidate::ClosePanel if target.stage == Stage::ClosePanel => {}
            Candidate::Point(point)
                if !matches!(target.stage, Stage::OpenPanel | Stage::ClosePanel) =>
            {
                if point.input_authorized
                    || point.current_frame_verified
                    || point.effective_routing_verified
                    || point
                        .expected_client
                        .iter()
                        .any(|v| !(1..=16384).contains(v))
                    || point
                        .client_pixel
                        .iter()
                        .zip(point.expected_client)
                        .any(|(v, n)| *v < 0 || *v >= i32::from(n))
                {
                    return Err(error(
                        "research_precondition",
                        "Candidate cannot assert frame/routing/input authority",
                    ));
                }
            }
            _ => {
                return Err(error(
                    "research_precondition",
                    "Candidate differs from stage input kind",
                ))
            }
        }
        live(authority, token, &target, deadline, gate)?;
        let fresh = backend.observe()?;
        fresh.validate(&target)?;
        fresh.preflight(&target)?;
        if target.stage == Stage::ClosePanel {
            fresh.preserves_close(&before)?;
        }
        live(authority, token, &target, deadline, gate)?;
        if self
            .file
            .metadata()?
            .len()
            .saturating_add((2 * LINE) as u64)
            > LIMIT
        {
            return Err(error(
                "journal_full",
                "Reserve intent and bounded terminal receipt before ordinary input",
            ));
        }
        let intent = self.append(Receipt {
            version: 1,
            request_id: id.into(),
            target: target.clone(),
            status: Status::Intent,
            delivery: None,
            observed: fresh,
            unix_ms: Some(stage_settle::now_ms()),
            closed: None,
        })?;
        // A known local refusal after intent is durable too. Any other delivery is observed
        // below and closed with a terminal receipt; it never authorizes a second event.
        let delivery =
            match live(authority, token, &target, deadline, gate).and_then(|()| permit(gate)) {
                Ok(p) => {
                    let delivered = backend.dispatch_once(&candidate, deadline);
                    drop(p);
                    delivered
                }
                Err(_) => Delivery::NotQueued,
            };
        if delivery == Delivery::NotQueued {
            self.append(Receipt {
                status: Status::NotQueued,
                delivery: Some(delivery),
                ..intent
            })
            .map_err(|_| {
                error(
                    "action_uncertain",
                    "Local refusal could not be committed; durable intent requires recovery",
                )
            })?;
            return Err(error(
                "input_not_queued",
                "Ordinary attempt was refused before possible input; plan remains unfinished",
            ));
        }
        // From here the request always closes: observe (reading only, no lease needed) until
        // the deadline or the settlement window ends, then write the terminal receipt.
        // Rejected transition reads are diagnostics for the close reason only; success still
        // needs the complete final own-state proof and input-idle confirmation.
        let mut identity_rejections: usize = 0;
        let mut open_rejections: usize = 0;
        let settled = stage_settle::settle(stage_settle::end(deadline, Instant::now()), |end| {
            if permit(gate).is_err() {
                return Seen::Stop("session_closed: owned input session closed".into());
            }
            let state = match backend.observe() {
                Err(cause)
                    if target.stage == Stage::OpenPanel
                        && cause.code == "research_open_transition" =>
                {
                    open_rejections = open_rejections.saturating_add(1);
                    return Seen::Pending(format!(
                        "rejected_open_reads={open_rejections}; last_open_transition={}",
                        stage_settle::bounded(&cause.message, 120)
                    ));
                }
                Err(cause) if cause.code == "research_identity_transition" => {
                    identity_rejections = identity_rejections.saturating_add(1);
                    return Seen::Pending(format!(
                        "rejected_identity_reads={identity_rejections}; last_identity_transition={}",
                        stage_settle::bounded(&cause.message, 120)
                    ));
                }
                Err(e)
                    if e.code == "torn_read"
                        || (target.stage == Stage::ClosePanel
                            && e.code == "research_panel_transition") =>
                {
                    return Seen::Pending(String::new())
                }
                Err(e) => return Seen::Unreadable(stage_settle::readback(&e)),
                Ok(state) => state,
            };
            if let Err(e) = state.validate(&target) {
                return Seen::Unreadable(format!(
                    "own context changed after possible input; {}",
                    stage_settle::readback(&e)
                ));
            }
            if target.stage == Stage::ClosePanel {
                if let Err(e) = state.preserves_close(&intent.observed) {
                    return Seen::Diverged(
                        state,
                        format!(
                            "original research panel proof changed after possible close input; {}",
                            stage_settle::readback(&e)
                        ),
                    );
                }
            }
            if !state.reached(&target) {
                return Seen::Pending(String::new());
            }
            // A target that shows while input idle stays unverified for a whole window closes
            // without proof: a held key or button does not release itself.
            match backend.confirm_input_idle(&candidate, end) {
                Ok(()) => Seen::Reached(state),
                Err(e) => Seen::Unreadable(format!(
                    "target observed but input idle is unverified; {}",
                    stage_settle::readback(&e)
                )),
            }
        });
        let committed = |cause| {
            Error::after_possible_input(
                "Observed outcome could not be committed to the research journal",
                cause,
            )
        };
        match settled {
            Settled::Reached(state) => {
                let receipt = self
                    .append(Receipt {
                        status: Status::Verified,
                        delivery: Some(delivery),
                        observed: state,
                        ..intent
                    })
                    .map_err(committed)?;
                Ok(Execution {
                    receipt,
                    historical: false,
                    current_state_verified: true,
                    event_causality_verified: false,
                })
            }
            Settled::Diverged(_, why) => {
                self.append(Receipt {
                    status: Status::Diverged,
                    delivery: Some(delivery),
                    closed: Some(why.clone()),
                    ..intent
                })
                .map_err(committed)?;
                Err(stage_settle::diverged(&why))
            }
            Settled::Unconfirmed(why) => {
                self.append(Receipt {
                    status: Status::Unconfirmed,
                    delivery: Some(delivery),
                    closed: Some(why.clone()),
                    ..intent
                })
                .map_err(committed)?;
                Err(stage_settle::unconfirmed(&why))
            }
        }
    }
}

/// `closed` is present exactly on the two closed-without-proof statuses, as one bounded line.
fn closed_shape(receipt: &Receipt) -> bool {
    let closed = matches!(receipt.status, Status::Diverged | Status::Unconfirmed);
    match &receipt.closed {
        None => !closed,
        Some(why) => {
            closed
                && !why.is_empty()
                && why.len() <= 256
                && !why.chars().any(char::is_control)
                && receipt.delivery != Some(Delivery::NotQueued)
        }
    }
}

#[cfg(test)]
mod tests;
