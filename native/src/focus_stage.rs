//! Durable single-stage ordinary focus attempts with borrowed input authority.
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
    OpenPolitics,
    OpenFocus,
    View,
    Start,
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
    pub focus_id: String,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "state", content = "focus_id")]
pub enum ActiveFocus {
    #[default]
    Unknown,
    Idle,
    Active(String),
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub enum LocalRoot {
    #[default]
    Unknown,
    Hidden,
    Ready,
    Animating,
}
/// Future Backend must derive every field from positively typed own producer
/// contexts. This library exposes no MCP Boolean/coordinate argument surface.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OwnState {
    pub epoch: Epoch,
    #[serde(default)]
    pub active_focus: ActiveFocus,
    /// Exact requested key resolved from the bounded own canonical focus tree.
    #[serde(default)]
    pub canonical_focus_id: Option<String>,
    pub managed_mode: Option<i32>,
    pub politics_root: LocalRoot,
    pub focus_root: LocalRoot,
    pub own_detail_counts: Option<[usize; 2]>,
    pub own_research_info_counts: Option<[usize; 2]>,
    pub selected_focus: Option<String>,
    pub requested_detail_ready: Option<bool>,
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
            "Bounded canonical own focus ID required",
        ));
    }
    Ok(())
}
impl Target {
    fn validate_key(&self) -> Result<()> {
        key(&self.focus_id)
    }
}
impl OwnState {
    pub(crate) fn validate(&self, target: &Target) -> Result<()> {
        target.validate_key()?;
        if self.epoch != target.epoch {
            return Err(error("ownership_changed", "Focus own epoch changed"));
        }
        if self.canonical_focus_id.as_deref() != Some(target.focus_id.as_str()) {
            return Err(error(
                "focus_precondition",
                "Fresh canonical own focus join required",
            ));
        }
        if self.managed_mode.is_some_and(|m| !(0..=23).contains(&m))
            || self
                .own_detail_counts
                .is_some_and(|v| v.iter().any(|n| *n > 32))
            || self
                .own_research_info_counts
                .is_some_and(|v| v.iter().any(|n| *n > 64))
        {
            return Err(error(
                "focus_precondition",
                "Bounded own UI producer fields required",
            ));
        }
        if let ActiveFocus::Active(id) = &self.active_focus {
            key(id)?;
        }
        if let Some(id) = &self.selected_focus {
            key(id)?;
        }
        Ok(())
    }
    fn base_known(&self) -> bool {
        self.managed_mode.is_some() && !matches!(self.active_focus, ActiveFocus::Unknown)
    }
    fn blockers_clear(&self) -> bool {
        self.own_detail_counts == Some([0, 0]) && self.own_research_info_counts == Some([0, 0])
    }
    fn reached(&self, t: &Target) -> bool {
        if !self.base_known() {
            return false;
        }
        match t.stage {
            Stage::OpenPolitics => {
                self.managed_mode == Some(12)
                    && self.politics_root == LocalRoot::Ready
                    && self.focus_root == LocalRoot::Hidden
                    && self.blockers_clear()
            }
            Stage::OpenFocus => {
                self.managed_mode == Some(13)
                    && self.focus_root == LocalRoot::Ready
                    && self.politics_root == LocalRoot::Hidden
                    && self.blockers_clear()
            }
            Stage::View => {
                self.managed_mode == Some(13)
                    && self.focus_root == LocalRoot::Ready
                    && self.politics_root == LocalRoot::Hidden
                    && self.own_research_info_counts == Some([0, 0])
                    && self.own_detail_counts == Some([1, 0])
                    && self.selected_focus.as_deref() == Some(&t.focus_id)
                    && self.requested_detail_ready == Some(true)
            }
            Stage::Start => matches!(&self.active_focus,ActiveFocus::Active(id) if id==&t.focus_id),
        }
    }
    pub(crate) fn preflight(&self, t: &Target) -> Result<()> {
        if !self.base_known() {
            return Err(error(
                "focus_precondition",
                "Known managed mode and explicit canonical active-focus state required",
            ));
        }
        let ok = match t.stage {
            Stage::OpenPolitics => {
                self.managed_mode == Some(23)
                    && self.politics_root == LocalRoot::Hidden
                    && self.focus_root == LocalRoot::Hidden
                    && self.blockers_clear()
            }
            Stage::OpenFocus => {
                self.managed_mode == Some(12)
                    && self.politics_root == LocalRoot::Ready
                    && self.focus_root == LocalRoot::Hidden
                    && self.blockers_clear()
            }
            Stage::View => {
                self.managed_mode == Some(13)
                    && self.focus_root == LocalRoot::Ready
                    && self.politics_root == LocalRoot::Hidden
                    && self.blockers_clear()
            }
            Stage::Start => {
                self.active_focus == ActiveFocus::Idle
                    && self.managed_mode == Some(13)
                    && self.focus_root == LocalRoot::Ready
                    && self.politics_root == LocalRoot::Hidden
                    && self.own_research_info_counts == Some([0, 0])
                    && self.own_detail_counts == Some([1, 0])
                    && self.selected_focus.as_deref() == Some(&t.focus_id)
                    && self.requested_detail_ready == Some(true)
            }
        };
        if !ok {
            return Err(error(
                "focus_precondition",
                "Exact own roots, scoped blockers and requested stage selection required",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub enum Candidate {
    /// Fixed ordinary shortcut; no coordinates or arbitrary key arguments.
    OpenPolitics,
    Point(PointCandidate),
}
/// Trusted internal adapter contract, not a caller-supplied state/input surface.
/// The adapter must derive roots/counts/canonical IDs from witnessed own readers;
/// keep candidate anchors and revalidate them in the final observe and dispatch.
/// A false local visible predicate alone cannot produce LocalRoot::Hidden.
/// This library does not prove global modal clearance or effective event routing.
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
        return Err(error("focus_deadline", "Focus attempt deadline reached"));
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
            .map_err(|_| error("journal_busy", "One focus journal writer required"))?;
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
                    return Err(error("invalid_journal", "Focus journal line exceeds bound"));
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
                .map_err(|_| error("invalid_journal", "Malformed focus receipt"))?;
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
                return Err(error("invalid_journal", "Invalid focus receipt witness"));
            }
            if let Some(previous) = receipts.get(&receipt.request_id) {
                if previous.status != Status::Intent
                    || receipt.status == Status::Intent
                    || previous.target != receipt.target
                {
                    return Err(error("invalid_journal", "Invalid focus receipt transition"));
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
                        "Focus terminal record contradicts durable intent",
                    ));
                }
            } else if receipt.status == Status::NotQueued
                || (receipt.status == Status::Verified && receipt.delivery.is_some())
            {
                return Err(error("invalid_journal", "Missing durable focus intent"));
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
            return Err(error("journal_failed", "Focus journal requires recovery"));
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
                "No focus attempt without durable capacity",
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
        if timeout.is_zero() || timeout > Duration::from_secs(10) {
            return Err(error("invalid_arguments", "Bounded stage timeout required"));
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
                    "Unresolved focus attempt; never replay",
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
            Candidate::OpenPolitics if target.stage == Stage::OpenPolitics => {}
            Candidate::Point(point) if target.stage != Stage::OpenPolitics => {
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
                        "focus_precondition",
                        "Candidate cannot assert frame/routing/input authority",
                    ));
                }
            }
            _ => {
                return Err(error(
                    "focus_precondition",
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
            live(authority, token, &target, deadline, gate).map_err(|_| {
                error(
                    "action_uncertain",
                    "Readback deadline/authority ended; never replay",
                )
            })?;
            match backend.observe() {
                Err(e) if e.code == "torn_read" => {}
                Err(_) => {
                    return Err(error(
                        "action_uncertain",
                        "Own readback unavailable; never replay",
                    ))
                }
                Ok(state) => {
                    state.validate(&target).map_err(|_| {
                        error(
                            "action_uncertain",
                            "Own context changed after possible input",
                        )
                    })?;
                    if Instant::now() >= deadline {
                        return Err(error(
                            "action_uncertain",
                            "Readback completed after bounded deadline",
                        ));
                    }
                    if state.reached(&target) {
                        backend.confirm_input_idle(&candidate,deadline).map_err(|_|error(
                            "action_uncertain","Canonical result observed but input idle remains unverified; never replay"))?;
                        live(authority, token, &target, deadline, gate).map_err(|_| {
                            error(
                                "action_uncertain",
                                "Input-idle check exceeded attempt authority/deadline",
                            )
                        })?;
                        let receipt=self.append(Receipt{status:Status::Verified,delivery:Some(delivery),observed:state,..intent})
                            .map_err(|_|error("action_uncertain","Canonical result observed but terminal receipt could not be committed"))?;
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
