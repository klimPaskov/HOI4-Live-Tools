//! Durable single-stage ordinary research attempts with borrowed input authority.
//! The adapter supplies fresh own state/candidates and one ordinary dispatcher.
//! No MCP registration, engine invocation or automatic input replay occurs here.
use crate::{
    actions::Epoch,
    error::{Error, Result},
    input_lease::InputAuthority,
    platform::Identity,
    research_attempt::PointCandidate,
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

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Stage {
    OpenPanel,
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
        if self.stage == Stage::SelectCategory {
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
        Ok(())
    }
    pub(crate) fn reached(&self, target: &Target) -> bool {
        match target.stage {
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
            Stage::OpenPanel | Stage::SelectSlot | Stage::SelectCategory => {
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
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub version: u8,
    pub request_id: String,
    pub target: Target,
    pub status: Status,
    pub delivery: Option<Delivery>,
    pub observed: OwnState,
}
#[derive(Debug, Serialize)]
pub struct Execution {
    pub receipt: Receipt,
    pub historical: bool,
    pub current_state_verified: bool,
    pub event_causality_verified: bool,
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
            {
                return Err(error("invalid_journal", "Invalid research receipt witness"));
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
                    || (receipt.status == Status::NotQueued
                        && receipt.observed != previous.observed)
                {
                    return Err(error(
                        "invalid_journal",
                        "Research terminal record contradicts durable intent",
                    ));
                }
            } else if receipt.status == Status::NotQueued
                || (receipt.status == Status::Verified && receipt.delivery.is_some())
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
    pub fn blocks_game(&self, game: &Identity) -> bool {
        self.poisoned
            || self
                .receipts
                .values()
                .any(|r| r.status == Status::Intent && &r.target.epoch.game == game)
    }
    /// Only an immediate error from execute may use this classification.
    /// execute calls dispatch_once only after a durable Intent. Post-dispatch
    /// observation errors become action_uncertain and keep that Intent.
    #[cfg(any(target_os = "linux", test))]
    pub(crate) fn failure_evidence(
        &self,
        id: &str,
        target: &Target,
        error: &Error,
    ) -> FailureEvidence {
        if self.poisoned || self.blocks_game(&target.epoch.game) || error.code == "action_uncertain"
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
        if let Some(r) = self.receipts.get(id) {
            if r.target != target {
                return Err(error(
                    "request_id_conflict",
                    "Request token is bound to another stage/target",
                ));
            }
            return match r.status {
                Status::Verified => Ok(Execution {
                    receipt: r.clone(),
                    historical: true,
                    current_state_verified: false,
                    event_causality_verified: false,
                }),
                Status::NotQueued => Err(error(
                    "input_not_queued",
                    "Historical local refusal; preserved plan needs a fresh attempt token",
                )),
                Status::Intent => Err(error(
                    "action_uncertain",
                    "Unresolved research attempt; never replay",
                )),
            };
        }
        if self.blocks_game(&target.epoch.game) {
            return Err(error(
                "action_uncertain",
                "Unresolved input blocks new IDs and stages for this owned game",
            ));
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
            Candidate::Point(point) if target.stage != Stage::OpenPanel => {
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
        })?;
        // A known local refusal after intent is durable too; uncertain delivery
        // retains Intent and never authorizes a second event.
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
        loop {
            live(authority, token, &target, deadline, gate).map_err(|cause| {
                Error::after_possible_input(
                    "Readback deadline/authority ended; never replay",
                    cause,
                )
            })?;
            match backend.observe() {
                Err(e) if e.code == "torn_read" => {}
                Err(cause) => {
                    return Err(Error::after_possible_input(
                        "Own readback unavailable; never replay",
                        cause,
                    ))
                }
                Ok(state) => {
                    state.validate(&target).map_err(|cause| {
                        Error::after_possible_input(
                            "Own context changed after possible input",
                            cause,
                        )
                    })?;
                    if Instant::now() >= deadline {
                        return Err(error(
                            "action_uncertain",
                            "Readback completed after bounded deadline",
                        ));
                    }
                    if state.reached(&target) {
                        backend.confirm_input_idle(&candidate,deadline).map_err(|cause|Error::after_possible_input(
                            "Canonical result observed but input idle remains unverified; never replay",cause))?;
                        live(authority, token, &target, deadline, gate).map_err(|cause| {
                            Error::after_possible_input(
                                "Input-idle check exceeded attempt authority/deadline",
                                cause,
                            )
                        })?;
                        let receipt=self.append(Receipt{status:Status::Verified,delivery:Some(delivery),observed:state,..intent})
                            .map_err(|cause|Error::after_possible_input("Canonical result observed but terminal receipt could not be committed",cause))?;
                        return Ok(Execution {
                            receipt,
                            historical: false,
                            current_state_verified: true,
                            event_causality_verified: false,
                        });
                    }
                }
            }
            thread::sleep(
                Duration::from_millis(20).min(deadline.saturating_duration_since(Instant::now())),
            );
        }
    }
}

#[cfg(test)]
mod tests;
