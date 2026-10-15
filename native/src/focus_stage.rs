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
use sha2::{Digest, Sha256};
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
    /// Distinct research-panel-to-Politics Q; never reinterprets old receipts.
    OpenPoliticsFromResearch,
    /// Guarded mode12-to23 Q; recorded service validation in run045.
    ClosePolitics,
    OpenFocus,
    /// One ordinary click in the exact empty own search field; no text input.
    SearchField,
    View,
    Start,
}
impl Stage {
    pub(crate) fn preserves_focus(self) -> bool {
        matches!(
            self,
            Self::ClosePolitics | Self::OpenPoliticsFromResearch | Self::SearchField
        )
    }
    pub(crate) fn is_shortcut(self) -> bool {
        matches!(
            self,
            Self::OpenPolitics | Self::ClosePolitics | Self::OpenPoliticsFromResearch
        )
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Delivery {
    NotQueued,
    Unknown,
    ServerConfirmed,
}

/// Keep the terminal classification while reporting the actual local cause.
/// In particular, a post-Intent NotQueued must never become retryable torn_read.
#[cfg(any(target_os = "linux", test))]
pub(crate) fn with_transport_cause(
    mut error: Error,
    transport: Option<&serde_json::Value>,
) -> Error {
    if let Some(cause) = transport.and_then(|v| v.get("error")) {
        if let (Some(code), Some(message)) = (cause["code"].as_str(), cause["message"].as_str()) {
            error.message = format!("{code}: {message}; {}", error.message);
        }
    }
    error
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
/// Producer-derived identities, not caller-supplied input permission.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SearchControl {
    pub locally_ready: bool,
    pub empty_query: bool,
    pub enabled: bool,
    pub keyboard_focus_verified: bool,
    pub text_receiver_verified: bool,
}
impl SearchControl {
    /// Projection of a witnessed producer response; not a command or public MCP input.
    #[doc(hidden)]
    pub fn from_owned_observation(d: &serde_json::Value, key: &str) -> Result<Self> {
        if d["owned"] != true
            || d["build_id"] != crate::game::BUILD_ID
            || d["command_observation"]["requested_key"] != key
            || d["command_observation"]["same_reader_clock_context"] != true
            || !d["focuses"]
                .as_array()
                .is_some_and(|a| a.len() == 1 && a[0]["key"] == key)
            || d["search"]["available"] != true
            || d["search"]["requested_key"] != key
        {
            return Err(search_error(
                "Same-reader exact own search command required",
            ));
        }
        let c = &d["search"]["query_control"];
        if [
            "available",
            "named_container_binding_verified",
            "ordinary_listener_binding_verified",
            "renderer_control_join_verified",
        ]
        .iter()
        .any(|k| c[k] != true)
            || c["query_encoding"] != "bounded_utf32le"
        {
            return Err(search_error(
                "Exact named own search control/listener/renderer required",
            ));
        }
        let r = &c["routing_diagnostic"];
        if [
            "available",
            "gui_global_binding_verified",
            "receiver_layout_verified",
        ]
        .iter()
        .any(|k| r[k] != true)
        {
            return Err(search_error(
                "Exact own GUI and separate search receivers required",
            ));
        }
        let flag = |v: &serde_json::Value| {
            v.as_bool()
                .ok_or_else(|| search_error("Known search control state required"))
        };
        Ok(Self {
            locally_ready: flag(&c["container_locally_ready"])?,
            empty_query: c["query_text"]
                .as_str()
                .ok_or_else(|| search_error("Exact search query required"))?
                .is_empty(),
            enabled: flag(&r["control_enabled_flag"])?,
            keyboard_focus_verified: flag(&r["keyboard_focus_verified"])?,
            text_receiver_verified: flag(&r["text_receiver_verified"])?,
        })
    }
}
fn search_error(message: &str) -> Error {
    Error::new("focus_precondition", message)
}
impl LocalRoot {
    fn is_unknown(&self) -> bool {
        *self == Self::Unknown
    }
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
    // Omit unknown defaults to preserve serialized old receipts/checksums.
    #[serde(default, skip_serializing_if = "LocalRoot::is_unknown")]
    pub research_panel_root: LocalRoot,
    #[serde(default, skip_serializing_if = "LocalRoot::is_unknown")]
    pub research_tree_root: LocalRoot,
    pub selected_focus: Option<String>,
    pub requested_detail_ready: Option<bool>,
    /// Absent on older stages/receipts; retain their serialized hashes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_control: Option<SearchControl>,
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
    pub(crate) fn reached(&self, t: &Target) -> bool {
        if !self.base_known() {
            return false;
        }
        match t.stage {
            Stage::OpenPoliticsFromResearch => {
                self.managed_mode == Some(12)
                    && self.politics_root == LocalRoot::Ready
                    && self.focus_root == LocalRoot::Hidden
                    && self.research_panel_root == LocalRoot::Hidden
                    && self.research_tree_root == LocalRoot::Hidden
                    && self.blockers_clear()
            }
            Stage::ClosePolitics => {
                matches!(&self.active_focus, ActiveFocus::Active(id) if id == &t.focus_id)
                    && self.managed_mode == Some(23)
                    && self.politics_root == LocalRoot::Hidden
                    && self.focus_root == LocalRoot::Hidden
                    && self.blockers_clear()
            }
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
            Stage::SearchField => {
                self.search_base_ready()
                    && self
                        .search_control
                        .as_ref()
                        .is_some_and(|s| s.keyboard_focus_verified && s.text_receiver_verified)
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
            Stage::OpenPoliticsFromResearch => {
                self.managed_mode == Some(5)
                    && self.politics_root == LocalRoot::Hidden
                    && self.focus_root == LocalRoot::Hidden
                    && self.research_panel_root == LocalRoot::Ready
                    && self.research_tree_root == LocalRoot::Hidden
                    && self.blockers_clear()
            }
            Stage::ClosePolitics => {
                matches!(&self.active_focus, ActiveFocus::Active(id) if id == &t.focus_id)
                    && self.managed_mode == Some(12)
                    && self.politics_root == LocalRoot::Ready
                    && self.focus_root == LocalRoot::Hidden
                    && self.blockers_clear()
            }
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
            Stage::SearchField => {
                self.search_base_ready()
                    && self
                        .search_control
                        .as_ref()
                        .is_some_and(|s| !s.keyboard_focus_verified && !s.text_receiver_verified)
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
    fn search_base_ready(&self) -> bool {
        self.managed_mode == Some(13)
            && self.focus_root == LocalRoot::Ready
            && self.politics_root == LocalRoot::Hidden
            && self.research_panel_root == LocalRoot::Hidden
            && self.research_tree_root == LocalRoot::Hidden
            && self.blockers_clear()
            && self
                .search_control
                .as_ref()
                .is_some_and(|s| s.locally_ready && s.empty_query && s.enabled)
    }
}
#[derive(Clone, Debug)]
pub enum Candidate {
    /// Distinct fixed Q candidate, never a generic panel escape.
    ClosePolitics,
    /// Fixed ordinary shortcut; no coordinates or arbitrary key arguments.
    OpenPolitics,
    OpenPoliticsFromResearch,
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
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) enum FailureEvidence {
    BeforeIntent,
    ConfirmedNotQueued,
    Unknown,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct TerminalQuarantine {
    version: u8,
    kind: String,
    request_id: String,
    target: Target,
    code: String,
    reason: String,
    receipt_hash: String,
    checksum: String,
}
impl TerminalQuarantine {
    fn checksum(&self) -> Result<String> {
        let bytes = serde_json::to_vec(&(
            "focus-terminal-quarantine-v1",
            self.version,
            &self.kind,
            &self.request_id,
            &self.target,
            &self.code,
            &self.reason,
            &self.receipt_hash,
        ))
        .map_err(|_| error("invalid_journal", "Cannot encode quarantine checksum"))?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}
fn receipt_hash(receipt: &Receipt) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(
            serde_json::to_vec(receipt)
                .map_err(|_| error("invalid_journal", "Cannot encode focus witness"))?
        )
    ))
}
pub struct Journal {
    file: File,
    receipts: BTreeMap<String, Receipt>,
    quarantines: BTreeMap<String, TerminalQuarantine>,
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
        let mut quarantines: BTreeMap<String, TerminalQuarantine> = BTreeMap::new();
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
            if let Ok(marker) = serde_json::from_slice::<TerminalQuarantine>(&line) {
                let previous = receipts.get(&marker.request_id).ok_or_else(|| {
                    error(
                        "invalid_journal",
                        "Quarantine requires original verified receipt",
                    )
                })?;
                if marker.version != 1
                    || marker.kind != "FocusTerminalQuarantine"
                    || marker.code != "action_uncertain"
                    || marker.reason.is_empty()
                    || marker.reason.len() > 256
                    || marker.reason.chars().any(char::is_control)
                    || previous.status != Status::Verified
                    || previous.target != marker.target
                    || receipt_hash(previous)? != marker.receipt_hash
                    || marker.checksum()? != marker.checksum
                    || quarantines.contains_key(&marker.request_id)
                {
                    return Err(error(
                        "invalid_journal",
                        "Invalid terminal focus quarantine",
                    ));
                }
                quarantines.insert(marker.request_id.clone(), marker);
                continue;
            }
            let receipt: Receipt = serde_json::from_slice(&line)
                .map_err(|_| error("invalid_journal", "Malformed focus receipt"))?;
            if quarantines
                .values()
                .any(|q| q.target.epoch.game == receipt.target.epoch.game)
            {
                return Err(error(
                    "invalid_journal",
                    "Receipt follows terminal focus quarantine",
                ));
            }
            request_id(&receipt.request_id)?;
            receipt.target.validate_key()?;
            receipt.observed.validate(&receipt.target)?;
            if (receipt.target.stage.preserves_focus()
                && receipt.status == Status::Intent
                && receipt.observed.preflight(&receipt.target).is_err())
                || receipt.version != 1
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
                    && receipt.target.stage.preserves_focus()
                    && receipt.observed.active_focus != previous.observed.active_focus)
                    || (receipt.status == Status::Verified
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
            } else if (receipt.target.stage.preserves_focus() && receipt.status == Status::Verified)
                || receipt.status == Status::NotQueued
                || (receipt.status == Status::Verified && receipt.delivery.is_some())
            {
                return Err(error("invalid_journal", "Missing durable focus intent"));
            }
            receipts.insert(receipt.request_id.clone(), receipt);
        }
        Ok(Self {
            file,
            receipts,
            quarantines,
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
    /// Preserve actual terminal evidence while prohibiting every subsequent input.
    /// A failed append poisons this owner; it cannot promise an uncommitted marker
    /// will survive a crash or independently reopening an unchanged old file.
    #[cfg(any(target_os = "linux", test))]
    pub(crate) fn finish_before_deadline(
        &mut self,
        execution: &Execution,
        until: Instant,
    ) -> Result<()> {
        if Instant::now() < until {
            return Ok(());
        }
        let reason = "Focus result arrived after original absolute deadline";
        if let Err(e) = self.quarantine_verified(&execution.receipt, reason) {
            return Err(error(
                "action_uncertain",
                &format!("{reason}; quarantine persistence failed: {e}"),
            ));
        }
        Err(error("action_uncertain", reason))
    }
    #[cfg(any(target_os = "linux", test))]
    fn quarantine_verified(&mut self, receipt: &Receipt, reason: &str) -> Result<()> {
        if self.poisoned {
            return Err(error("journal_failed", "Focus journal requires recovery"));
        }
        self.poisoned = true;
        if receipt.status != Status::Verified
            || self.receipts.get(&receipt.request_id) != Some(receipt)
            || reason.is_empty()
            || reason.len() > 256
            || reason.chars().any(char::is_control)
        {
            return Err(error(
                "invalid_journal",
                "Actual verified focus evidence required",
            ));
        }
        let mut marker = TerminalQuarantine {
            version: 1,
            kind: "FocusTerminalQuarantine".into(),
            request_id: receipt.request_id.clone(),
            target: receipt.target.clone(),
            code: "action_uncertain".into(),
            reason: reason.into(),
            receipt_hash: receipt_hash(receipt)?,
            checksum: String::new(),
        };
        marker.checksum = marker.checksum()?;
        if self.quarantines.get(&marker.request_id) == Some(&marker) {
            return Ok(());
        }
        self.quarantines
            .insert(marker.request_id.clone(), marker.clone());
        let mut line = serde_json::to_vec(&marker)
            .map_err(|_| error("invalid_journal", "Cannot encode quarantine"))?;
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
                "No durable focus quarantine capacity",
            ));
        }
        self.file.write_all(&line)?;
        self.file.sync_all()?;
        self.poisoned = false;
        Ok(())
    }
    /// Classify only immediately after execute returns an error, never from
    /// absence in a historic file alone. Poison/possible input always dominates.
    #[allow(dead_code)] // Future daily caller must classify immediately, not historical absence.
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
    pub fn blocks_game(&self, game: &Identity) -> bool {
        self.poisoned
            || self
                .quarantines
                .values()
                .any(|q| &q.target.epoch.game == game)
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
        if self.blocks_game(&target.epoch.game) {
            return Err(error(
                "action_uncertain",
                "Focus journal quarantines this game; never replay",
            ));
        }
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
        if before.reached(&target) && !target.stage.preserves_focus() {
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
            Candidate::OpenPoliticsFromResearch
                if target.stage == Stage::OpenPoliticsFromResearch => {}
            Candidate::ClosePolitics if target.stage == Stage::ClosePolitics => {}
            Candidate::Point(point) if !target.stage.is_shortcut() => {
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
        if target.stage.preserves_focus() && fresh.active_focus != before.active_focus {
            return Err(error(
                "torn_read",
                "Canonical active focus changed before focus navigation input",
            ));
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
                    if target.stage.preserves_focus()
                        && state.active_focus != intent.observed.active_focus
                    {
                        return Err(error("action_uncertain", "Canonical active focus changed after focus navigation input; never replay"));
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

#[cfg(all(test, unix))]
mod daily_failure_classifier_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    #[test]
    fn immediate_classifier_never_converts_possible_input_to_preintent() {
        static N: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "private-focus-classifier-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut j = Journal::open(&root.join("focus.jsonl")).unwrap();
        let epoch = Epoch {
            game: Identity {
                pid: 123,
                start_ticks: "456".into(),
                exe: "/generated/game".into(),
                uid: 1000,
            },
            campaign: "generated".into(),
            player: "GER".into(),
        };
        let target = Target {
            epoch: epoch.clone(),
            stage: Stage::View,
            focus_id: "GER_rhineland".into(),
        };
        assert_eq!(
            j.failure_evidence("id", &target, &error("input_busy", "before")),
            FailureEvidence::BeforeIntent
        );
        assert_eq!(
            j.failure_evidence("id", &target, &error("action_uncertain", "unknown")),
            FailureEvidence::Unknown
        );
        let mut receipt = Receipt {
            version: 1,
            request_id: "id".into(),
            target: target.clone(),
            status: Status::Intent,
            delivery: None,
            observed: OwnState {
                epoch,
                active_focus: ActiveFocus::Idle,
                canonical_focus_id: Some(target.focus_id.clone()),
                managed_mode: Some(13),
                politics_root: LocalRoot::Hidden,
                focus_root: LocalRoot::Ready,
                own_detail_counts: Some([0, 0]),
                own_research_info_counts: Some([0, 0]),
                research_panel_root: LocalRoot::Unknown,
                research_tree_root: LocalRoot::Unknown,
                selected_focus: None,
                requested_detail_ready: Some(false),
                search_control: None,
            },
        };
        j.receipts.insert("id".into(), receipt.clone());
        assert_eq!(
            j.failure_evidence("id", &target, &error("input_busy", "after intent")),
            FailureEvidence::Unknown
        );
        receipt.status = Status::NotQueued;
        receipt.delivery = Some(Delivery::NotQueued);
        j.receipts.insert("id".into(), receipt);
        assert_eq!(
            j.failure_evidence("id", &target, &error("input_not_queued", "confirmed")),
            FailureEvidence::ConfirmedNotQueued
        );
        j.poisoned = true;
        assert_eq!(
            j.failure_evidence("id", &target, &error("input_busy", "poison")),
            FailureEvidence::Unknown
        );
        drop(j);
        std::fs::remove_dir_all(root).unwrap();
    }
}
