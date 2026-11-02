//! Two durable ordinary construction stages with an independently opted-in adapter.
//! The backend supplies witnessed own state, a stable private identity scope,
//! an admitted PointCandidate, and at most one ordinary pointer dispatch.
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

pub mod read_recovery;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Stage {
    EnsureOpen,
    SelectBuilding,
    /// Open the game's own map Find view with its fixed `f` shortcut.
    FindOpen,
    /// Type the decimal own state ID into the focused, empty Find query.
    FindQuery,
    /// Press Return to activate the verified first state result.
    FindActivate,
    /// One left click at the owned client centre with the canonical building
    /// selected and the camera centred on the own state: ensure its queued total.
    Place,
}
impl Stage {
    pub fn is_find(self) -> bool {
        matches!(self, Self::FindOpen | Self::FindQuery | Self::FindActivate)
    }
    /// Stages that observe the own Find view and camera.
    pub fn uses_find_view(self) -> bool {
        self.is_find() || self == Self::Place
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Delivery {
    NotQueued,
    Unknown,
    ServerConfirmed,
}
/// A positive no-input refusal must carry its actual producer reason.
/// Unknown(Some(error)) is a failed/ambiguous transport and stops readback;
/// Unknown(None) permits bounded canonical readback without assuming delivery.
#[derive(Debug)]
pub enum DispatchOutcome {
    NotQueued(Error),
    Unknown(Option<Error>),
    ServerConfirmed,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub epoch: Epoch,
    pub stage: Stage,
    /// Required only for SelectBuilding. Never a localized label or icon ID.
    pub building_id: Option<String>,
    /// Own state ID for FindQuery/FindActivate; absent (and unserialized) otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_id: Option<i32>,
    /// Place only: desired total queued amount of building_id in state_id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<u32>,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum PanelState {
    Unmaterialized,
    Unknown,
    Transitional,
    Hidden,
    Shown,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Coverage {
    NotChecked,
    Unavailable,
    Complete,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct OwnState {
    pub epoch: Epoch,
    pub managed_mode: Option<i32>,
    /// Only a settled exact owned panel can be Shown or Hidden.
    pub panel: PanelState,
    pub selected_mode: Option<i32>,
    /// Ordinary mode1 only, joined to the fresh public template catalog.
    /// Railway/unsupported modes must not expose template payload here.
    pub selected_building: Option<String>,
    /// Complete bounded unique catalog pointer membership, with the selected or
    /// requested template's exact ordinal/key join; not all catalog key strings.
    pub catalog_coverage: Coverage,
    pub choice_coverage: Coverage,
    pub requested_choice_present: bool,
    /// Known own scoped local blockers, not global-modal or current-frame proof.
    pub scoped_blockers_clear: Option<bool>,
    /// Requested named control's exact type/flags/listener/full-tail gates.
    /// Geometry and renderer/profile admission are still backend-required.
    pub local_dispatch_ready: Option<bool>,
    /// Same-witness own Find view facts; only for Find stages, never serialized otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub find: Option<FindState>,
    /// Place only: same-witness queued amount of the requested building.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<PlacementState>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlacementState {
    /// Sum of line amounts of the requested building located in the own state.
    pub in_state: i64,
    /// Sum for the same building located elsewhere.
    pub elsewhere: i64,
    /// Every line of that building has a proven own location and valid amount.
    pub complete: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FindState {
    /// Exact own Find window local visible flag; None when unmaterialized.
    pub visible: Option<bool>,
    /// The GUI text receiver is exactly the Find text box.
    pub text_focused: Option<bool>,
    /// Bounded printable ASCII query, or None when unsupported/unavailable.
    pub query: Option<String>,
    /// First result row's literal kind (2 = state), when a first row exists.
    #[serde(default)]
    pub first_kind: Option<i32>,
    /// First result row's catalog-joined state ID, when it is a state row.
    pub first_state_id: Option<i32>,
    /// Requested state is own owned and controlled (Find query/activate only).
    pub target_owned: Option<bool>,
    /// Camera look-at equals the requested state's template map position.
    pub camera_centered: Option<bool>,
}
pub fn state_digits(id: i32) -> Result<String> {
    if !(1..4096).contains(&id) {
        return Err(error("invalid_arguments", "Bounded own state ID required"));
    }
    Ok(id.to_string())
}
fn error(code: &'static str, message: &str) -> Error {
    Error::new(code, message)
}
fn key(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(error(
            "invalid_arguments",
            "Bounded canonical building key required",
        ));
    }
    Ok(())
}
impl Target {
    pub fn validate(&self) -> Result<()> {
        match (self.stage, self.building_id.as_deref()) {
            (Stage::FindOpen, None) if self.state_id.is_none() && self.amount.is_none() => Ok(()),
            (Stage::FindQuery | Stage::FindActivate, None) if self.amount.is_none() => {
                state_digits(self.state_id.unwrap_or(0)).map(|_| ())
            }
            (Stage::Place, Some(id)) => {
                key(id)?;
                state_digits(self.state_id.unwrap_or(0))?;
                if id == "rail_way" || !self.amount.is_some_and(|n| (1..=1000).contains(&n)) {
                    return Err(error(
                        "invalid_arguments",
                        "Place requires an ordinary building and a bounded target amount",
                    ));
                }
                Ok(())
            }
            (Stage::EnsureOpen, None) if self.state_id.is_none() && self.amount.is_none() => Ok(()),
            (Stage::SelectBuilding, Some(_))
                if self.state_id.is_some() || self.amount.is_some() =>
            {
                Err(error(
                    "invalid_arguments",
                    "Stage requires its exact canonical target shape",
                ))
            }
            (Stage::SelectBuilding, Some(id)) => {
                key(id)?;
                if id == "rail_way" {
                    return Err(error(
                        "invalid_arguments",
                        "Railway selection is outside this stage",
                    ));
                }
                Ok(())
            }
            _ => Err(error(
                "invalid_arguments",
                "Stage requires its exact canonical target shape",
            )),
        }
    }
}
impl OwnState {
    pub fn validate(&self, target: &Target) -> Result<()> {
        target.validate()?;
        if self.epoch != target.epoch {
            return Err(error("ownership_changed", "Construction own epoch changed"));
        }
        if let Some(id) = &self.selected_building {
            key(id)?;
            if self.selected_mode != Some(1)
                || id == "rail_way"
                || self.catalog_coverage != Coverage::Complete
            {
                return Err(error(
                    "invalid_construction_state",
                    "Only fresh canonical ordinary selection payload is supported",
                ));
            }
        }
        if self.requested_choice_present
            && (target.stage != Stage::SelectBuilding
                || self.catalog_coverage != Coverage::Complete
                || self.choice_coverage != Coverage::Complete)
        {
            return Err(error(
                "invalid_construction_state",
                "Choice membership requires complete catalog and choice coverage",
            ));
        }
        if self.placement.is_some() != (target.stage == Stage::Place) {
            return Err(error(
                "invalid_construction_state",
                "Placement facts belong exactly to Place",
            ));
        }
        match (&self.find, target.stage.uses_find_view()) {
            (Some(find), true) => {
                if find
                    .query
                    .as_ref()
                    .is_some_and(|q| q.len() > 64 || !q.bytes().all(|b| (0x20..0x7f).contains(&b)))
                    || find.first_state_id.is_some_and(|v| !(1..4096).contains(&v))
                {
                    return Err(error(
                        "invalid_construction_state",
                        "Bounded own Find view facts required",
                    ));
                }
            }
            (None, false) => {}
            _ => {
                return Err(error(
                    "invalid_construction_state",
                    "Find view facts belong exactly to Find stages",
                ))
            }
        }
        Ok(())
    }
    pub fn reached(&self, target: &Target) -> bool {
        if target.stage == Stage::Place {
            return self.placement.as_ref().is_some_and(|p| {
                p.complete && target.amount.is_some_and(|n| p.in_state == i64::from(n))
            });
        }
        if target.stage.is_find() {
            let Some(f) = &self.find else {
                return false;
            };
            let digits = target.state_id.and_then(|v| state_digits(v).ok());
            let open = f.visible == Some(true) && f.text_focused == Some(true);
            return match target.stage {
                Stage::FindOpen => open,
                // Typing is verified by its own effect: the exact query and a
                // refreshed first row. Which row comes first is Activate's precondition.
                Stage::FindQuery => {
                    open && digits.is_some() && f.query == digits && f.first_kind.is_some()
                }
                Stage::FindActivate => {
                    f.visible == Some(false)
                        && f.target_owned == Some(true)
                        && f.camera_centered == Some(true)
                }
                _ => false,
            };
        }
        if self.managed_mode != Some(17) || self.panel != PanelState::Shown {
            return false;
        }
        match target.stage {
            Stage::EnsureOpen => true,
            Stage::FindOpen | Stage::FindQuery | Stage::FindActivate | Stage::Place => false,
            Stage::SelectBuilding => {
                self.selected_mode == Some(1)
                    && self.selected_building.as_deref() == target.building_id.as_deref()
                    && self.catalog_coverage == Coverage::Complete
                    && self.choice_coverage == Coverage::Complete
                    && self.requested_choice_present
            }
        }
    }
    pub fn preflight(&self, target: &Target) -> Result<()> {
        self.validate(target)?;
        let require = |ok: bool, why: &'static str| -> Result<()> {
            if ok {
                Ok(())
            } else {
                Err(error("construction_precondition", why))
            }
        };
        let find = || {
            self.find
                .as_ref()
                .ok_or_else(|| error("construction_precondition", "Find view facts required"))
        };
        match target.stage {
            Stage::Place => {
                require(
                    self.managed_mode == Some(17) && self.panel == PanelState::Shown,
                    "Place requires the positively shown construction panel",
                )?;
                require(
                    self.selected_mode == Some(1)
                        && self.selected_building.as_deref() == target.building_id.as_deref()
                        && self.catalog_coverage == Coverage::Complete,
                    "Place requires the requested canonical ordinary building selected",
                )?;
                let f = find()?;
                require(
                    f.visible == Some(false)
                        && f.target_owned == Some(true)
                        && f.camera_centered == Some(true),
                    "Place requires the closed Find view and the camera on the own state",
                )?;
                let p = self.placement.as_ref().ok_or_else(|| {
                    error("construction_precondition", "Placement facts required")
                })?;
                require(
                    p.complete
                        && target
                            .amount
                            .is_some_and(|n| p.in_state + 1 == i64::from(n)),
                    "Place requires a complete queue exactly one below the target amount",
                )?;
            }
            Stage::FindOpen => {
                require(
                    find()?.visible == Some(false),
                    "FindOpen requires the materialized Find view positively hidden",
                )?;
            }
            Stage::FindQuery | Stage::FindActivate => {
                let f = find()?;
                require(
                    f.visible == Some(true) && f.text_focused == Some(true),
                    "Find input requires the visible Find view with its focused text box",
                )?;
                require(
                    f.target_owned == Some(true),
                    "Requested state must be own owned and controlled",
                )?;
                if target.stage == Stage::FindQuery {
                    require(
                        f.query.as_deref() == Some(""),
                        "FindQuery requires an empty query",
                    )?;
                } else {
                    require(
                        f.query.is_some()
                            && f.query == target.state_id.and_then(|v| state_digits(v).ok()),
                        "FindActivate requires the exact typed query",
                    )?;
                    require(
                        f.first_kind == Some(2) && f.first_state_id == target.state_id,
                        "FindActivate requires the requested own state as the first result row",
                    )?;
                }
            }
            Stage::EnsureOpen => {
                require(
                    self.managed_mode == Some(23),
                    "EnsureOpen requires managed mode23",
                )?;
                require(
                    self.panel == PanelState::Hidden,
                    "EnsureOpen requires positively settled hidden panel",
                )?;
                require(
                    self.selected_mode == Some(0),
                    "EnsureOpen requires inactive selected mode0",
                )?;
            }
            Stage::SelectBuilding => {
                require(
                    self.managed_mode == Some(17),
                    "SelectBuilding requires managed mode17",
                )?;
                require(
                    self.panel == PanelState::Shown,
                    "SelectBuilding requires positively settled shown panel",
                )?;
                require(
                    self.selected_mode == Some(0)
                        || (self.selected_mode == Some(1) && self.selected_building.is_some()),
                    "SelectBuilding requires known inactive or canonical ordinary selection",
                )?;
                require(
                    self.catalog_coverage == Coverage::Complete,
                    "Complete unique catalog membership and canonical target join required",
                )?;
                require(
                    self.choice_coverage == Coverage::Complete,
                    "Complete unique construction choice coverage required",
                )?;
                require(
                    self.requested_choice_present,
                    "Requested canonical building must occur in complete choice collection",
                )?;
            }
        }
        require(
            self.scoped_blockers_clear == Some(true),
            "Known own scoped local blockers must be clear",
        )?;
        require(
            self.local_dispatch_ready == Some(true),
            "Requested named control local dispatch predicates must all pass",
        )?;
        Ok(())
    }
}

/// Scope is a backend-private typed identity tuple, not JSON/scalar equality.
/// It must retain actual own context/registry/manager/panel plus requested
/// owner/window/button/callback capture/renderer/template identities relevant
/// to this stage. Exclude the semantic selection fields expected to change.
/// Construction controls observation must obtain this alongside OwnState in
/// the encompassing Witnesses transaction; no additional un-witnessed reads.
#[derive(Clone, Debug)]
pub struct Observation<S> {
    pub state: OwnState,
    pub scope: S,
}
/// Exactly one ordinary input, derived by the backend from witnessed state.
/// Keys and digits are fixed by stage/target; callers never supply keys.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    /// EnsureOpen/SelectBuilding: one admitted left click.
    Point(PointCandidate),
    /// FindOpen: the game's own `f` shortcut for the Find view.
    FindKey,
    /// FindQuery: the requested state's decimal digits, unshifted only.
    Digits(String),
    /// FindActivate: one Return, which activates the first result row.
    Return,
}
#[derive(Clone, Debug)]
pub struct Candidate<S> {
    pub input: Input,
    pub scope: S,
}
pub trait Backend {
    type Scope: Clone + Eq;
    /// Ownership/positive player checks must precede any state payload read.
    /// Foreign/unknown roots must fail without traversing their payload.
    fn observe(&mut self, target: &Target) -> Result<Observation<Self::Scope>>;
    /// A read-only pre-input phase, bounded by the original lease/call deadline.
    /// Implementations may discard torn producer snapshots, never retry context
    /// acceptance, state/scope decisions, journal execution or possible input.
    fn observe_before_input(
        &mut self,
        target: &Target,
        _phase: read_recovery::Phase,
        _deadline: Instant,
    ) -> Result<Observation<Self::Scope>> {
        self.observe(target)
    }
    /// Actual named renderer/resource/hierarchy and reviewed owned client codec
    /// must admit the point. No fallback caller coordinates or keyboard toggle.
    fn candidate(
        &mut self,
        target: &Target,
        scope: &Self::Scope,
        deadline: Instant,
    ) -> Result<Candidate<Self::Scope>>;
    /// Exactly one ordinary left-click attempt, with the ORIGINAL deadline.
    /// Must prepare through the owned input adapter and immediately recheck
    /// positive state, exact pinned scope AND original point derivation before
    /// dispatch_once_checked. Changed geometry must refuse, not replace the
    /// candidate or dispatch to a recomputed point under the same intent.
    /// NotQueued means positive proof no input was queued; any possible event,
    /// transport loss, or ambiguous refusal MUST return Unknown/ServerConfirmed.
    fn dispatch_once(
        &mut self,
        target: &Target,
        candidate: &Candidate<Self::Scope>,
        deadline: Instant,
    ) -> DispatchOutcome;
    /// Read-only acknowledgement, never automatic release/parking/compensation.
    fn confirm_input_idle(
        &mut self,
        candidate: &Candidate<Self::Scope>,
        deadline: Instant,
    ) -> Result<()>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DomainBlocks {
    pub clock: bool,
    pub research: bool,
    pub focus: bool,
    /// Other construction journals; this Journal checks its own unresolved IDs.
    pub construction: bool,
    pub production: bool,
}
pub trait UnresolvedInputs {
    /// Caller must borrow ALL shared domain journals under the existing single
    /// controller/service mutex for the whole attempt; booleans are not a lock.
    /// A missing/unreadable domain is an error, never implicitly clear.
    fn observe(&self, game: &Identity) -> Result<DomainBlocks>;
}
fn domains_clear(domains: &dyn UnresolvedInputs, game: &Identity) -> Result<()> {
    let d = domains.observe(game)?;
    if d.clock || d.research || d.focus || d.construction || d.production {
        return Err(error(
            "action_uncertain",
            "Unresolved shared input domain blocks construction",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Status {
    Intent,
    NotQueued,
    Verified,
    /// Durable actual own readback when possible input was not verified.
    /// This remains blocking and can NEVER authorize replay.
    Uncertain,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Uncertainty {
    Dispatch,
    DeadlineOrAuthority,
    Observation,
    Ownership,
    Scope,
    InputIdle,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
}
fn bounded(value: &str, limit: usize) -> String {
    let mut out = String::new();
    for c in value.chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > limit {
            break;
        }
        out.push(c);
    }
    out
}
fn safe_message(value: &str, token: &str) -> String {
    // Start with the established one-line/UTF8 byte bound, but redact before
    // truncation so a split token/address can never leave a secret prefix.
    let filtered: String = value.chars().filter(|c| !c.is_control()).collect();
    let value = filtered.as_str();
    let lower = value.to_ascii_lowercase();
    let needle = token.to_ascii_lowercase();
    let mut scrubbed = String::new();
    let mut copied = 0;
    if !needle.is_empty() {
        for (index, _) in lower.match_indices(&needle) {
            scrubbed.push_str(&value[copied..index]);
            scrubbed.push_str("[redacted]");
            copied = index + needle.len();
        }
    }
    scrubbed.push_str(&value[copied..]);
    let mut out = String::new();
    let mut word = String::new();
    fn emit(out: &mut String, word: &mut String, token: &str) {
        if word.is_empty() {
            return;
        }
        let hex_address = word
            .as_bytes()
            .windows(3)
            .any(|v| v[0] == b'0' && matches!(v[1], b'x' | b'X') && v[2].is_ascii_hexdigit());
        let opaque_hex = word.len() >= 8 && word.bytes().all(|b| b.is_ascii_hexdigit());
        if hex_address || opaque_hex || (!token.is_empty() && word.eq_ignore_ascii_case(token)) {
            out.push_str("[redacted]");
        } else {
            out.push_str(word);
        }
        word.clear();
    }
    for c in scrubbed.chars() {
        if c.is_ascii_alphanumeric() {
            word.push(c);
        } else {
            emit(&mut out, &mut word, token);
            if !c.is_control() {
                out.push(c);
            }
        }
    }
    emit(&mut out, &mut word, token);
    bounded(&out, 512)
}
impl Diagnostic {
    pub(crate) fn from_error(cause: &Error, token: &str) -> Self {
        let code = if cause.code.len() <= 128
            && !cause.code.is_empty()
            && cause
                .code
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && safe_message(cause.code, token) == cause.code
        {
            cause.code.into()
        } else {
            "diagnostic_code_redacted".into()
        };
        Self {
            code,
            message: safe_message(&cause.message, token),
        }
    }
    pub(crate) fn validate(&self) -> bool {
        !self.code.is_empty()
            && self.code.len() <= 128
            && self
                .code
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
            && safe_message(&self.code, "") == self.code
            && self.message.len() <= 512
            && safe_message(&self.message, "") == self.message
    }
    pub(crate) fn as_error(&self, code: &'static str, context: &str) -> Error {
        Error::new(
            code,
            bounded(
                &format!("{context}; cause={}: {}", self.code, self.message),
                512,
            ),
        )
    }
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
    pub uncertainty: Option<Uncertainty>,
    pub cause: Option<Diagnostic>,
}
#[derive(Debug, Serialize)]
pub struct Execution {
    pub receipt: Receipt,
    pub historical: bool,
    pub current_state_verified: bool,
    pub event_causality_verified: bool,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FailureEvidence {
    BeforeIntent,
    ConfirmedNotQueued,
    Unknown,
}
impl Receipt {
    fn validate(&self) -> Result<()> {
        request_id(&self.request_id)?;
        self.observed.validate(&self.target)?;
        let valid = self.version == 1
            && self.cause.as_ref().is_none_or(Diagnostic::validate)
            && match self.status {
                Status::Intent => {
                    self.delivery.is_none()
                        && self.uncertainty.is_none()
                        && self.cause.is_none()
                        && self.observed.preflight(&self.target).is_ok()
                }
                Status::NotQueued => {
                    self.delivery == Some(Delivery::NotQueued)
                        && self.uncertainty.is_none()
                        && self.cause.is_some()
                }
                Status::Verified => {
                    self.observed.reached(&self.target)
                        && self.uncertainty.is_none()
                        && self.cause.is_none()
                        && self.delivery != Some(Delivery::NotQueued)
                }
                Status::Uncertain => {
                    matches!(
                        self.delivery,
                        Some(Delivery::Unknown | Delivery::ServerConfirmed)
                    ) && self.uncertainty.is_some()
                        && self.cause.is_some()
                }
            };
        if !valid {
            return Err(error(
                "invalid_journal",
                "Invalid construction receipt witness",
            ));
        }
        Ok(())
    }
}

// Durable file policy below is reused from repository research_stage.rs:
// fs2 single writer, private owned single-link regular file, no-follow flags,
// bounded complete JSONL, intent fsync before input, parent directory fsync,
// capacity reservation and poisoned writer on ambiguous write failure.

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
            "construction_deadline",
            "Construction attempt deadline reached",
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
            .map_err(|_| error("journal_busy", "One construction journal writer required"))?;
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
                        "Construction journal line exceeds bound",
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
                .map_err(|_| error("invalid_journal", "Malformed construction receipt"))?;
            receipt
                .validate()
                .map_err(|_| error("invalid_journal", "Invalid construction receipt"))?;
            if let Some(previous) = receipts.get(&receipt.request_id) {
                if previous.status != Status::Intent
                    || receipt.status == Status::Intent
                    || previous.target != receipt.target
                {
                    return Err(error(
                        "invalid_journal",
                        "Invalid construction receipt transition",
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
                        "Terminal record contradicts durable intent",
                    ));
                }
            } else if receipt.status == Status::NotQueued
                || receipt.status == Status::Uncertain
                || (receipt.status == Status::Verified && receipt.delivery.is_some())
            {
                return Err(error(
                    "invalid_journal",
                    "Missing durable construction intent",
                ));
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
                "Construction journal requires recovery",
            ));
        }
        receipt.validate()?;
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
                "No construction attempt without durable capacity",
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
            || self.receipts.values().any(|r| {
                matches!(r.status, Status::Intent | Status::Uncertain)
                    && &r.target.epoch.game == game
            })
    }
    /// Classify only the immediate error from execute. An unresolved durable
    /// record or poisoned writer always defeats a retryable producer category.
    pub fn failure_evidence(&self, id: &str, target: &Target, cause: &Error) -> FailureEvidence {
        if self.poisoned || self.blocks_game(&target.epoch.game) || cause.code == "action_uncertain"
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
    fn noop(&mut self, id: &str, target: Target, state: OwnState) -> Result<Execution> {
        let receipt = self.append(Receipt {
            version: 1,
            request_id: id.into(),
            target,
            status: Status::Verified,
            delivery: None,
            observed: state,
            uncertainty: None,
            cause: None,
        })?;
        Ok(Execution {
            receipt,
            historical: false,
            current_state_verified: true,
            event_causality_verified: false,
        })
    }
    fn not_queued(&mut self, intent: Receipt, cause: Error, token: &str) -> Result<Execution> {
        let diagnostic = Diagnostic::from_error(&cause, token);
        self.append(Receipt {
            status: Status::NotQueued,
            delivery: Some(Delivery::NotQueued),
            cause: Some(diagnostic.clone()),
            ..intent
        })
        .map_err(|write| {
            diagnostic.as_error(
                "action_uncertain",
                &format!(
                    "Refusal could not be committed; preserve intent; journal_cause={}",
                    Diagnostic::from_error(&write, token).code
                ),
            )
        })?;
        Err(diagnostic.as_error(
            "input_not_queued",
            "Ordinary attempt proved not queued; never replay this ID",
        ))
    }
    #[allow(clippy::too_many_arguments)]
    fn uncertain(
        &mut self,
        token: &str,
        intent: &Receipt,
        delivery: Delivery,
        last: OwnState,
        reason: Uncertainty,
        cause: Error,
    ) -> Error {
        let diagnostic = Diagnostic::from_error(&cause, token);
        // A failed terminal append retains the original durable intent (or
        // poisons the writer), both blocking. Preserve the actual producer
        // cause in the returned error even if terminal persistence fails.
        let result = self.append(Receipt {
            status: Status::Uncertain,
            delivery: Some(delivery),
            observed: last,
            uncertainty: Some(reason),
            cause: Some(diagnostic.clone()),
            ..intent.clone()
        });
        match result {
            Ok(_) => diagnostic.as_error(
                "action_uncertain",
                "Construction outcome unverified; never replay",
            ),
            Err(write) => diagnostic.as_error(
                "action_uncertain",
                &format!(
                    "Uncertainty could not be committed; preserve intent; journal_cause={}",
                    Diagnostic::from_error(&write, token).code
                ),
            ),
        }
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
        let result = (|| -> Result<Execution> {
            request_id(id)?;
            target.validate()?;
            if timeout.is_zero() || timeout > Duration::from_secs(10) {
                return Err(error(
                    "invalid_arguments",
                    "Stage timeout must be positive and at most ten seconds",
                ));
            }
            let deadline =
                (Instant::now() + timeout).min(authority.authorize(token, &target.epoch)?);
            live(authority, token, &target, deadline, gate)?;
            if let Some(receipt) = self.receipts.get(id) {
                if receipt.target != target {
                    return Err(error(
                        "request_id_conflict",
                        "Request ID already binds another stage or target",
                    ));
                }
                return match receipt.status {
                    Status::Verified => Ok(Execution {
                        receipt: receipt.clone(),
                        historical: true,
                        current_state_verified: false,
                        event_causality_verified: false,
                    }),
                    Status::NotQueued => Err(receipt
                        .cause
                        .as_ref()
                        .unwrap()
                        .as_error("input_not_queued", "Historical local refusal; never replay")),
                    Status::Uncertain => Err(receipt.cause.as_ref().unwrap().as_error(
                        "action_uncertain",
                        "Historical uncertain outcome; never replay",
                    )),
                    Status::Intent => Err(error(
                        "action_uncertain",
                        "Unresolved construction intent; never replay",
                    )),
                };
            }
            if self.blocks_game(&target.epoch.game) {
                return Err(error(
                    "action_uncertain",
                    "Unresolved construction input blocks new IDs and stages",
                ));
            }
            domains_clear(domains, &target.epoch.game)?;
            live(authority, token, &target, deadline, gate)?;
            let before =
                backend.observe_before_input(&target, read_recovery::Phase::Initial, deadline)?;
            live(authority, token, &target, deadline, gate)?;
            before.state.validate(&target)?;
            let scope = before.scope;
            if before.state.reached(&target) {
                return self.noop(id, target, before.state);
            }
            before.state.preflight(&target)?;
            let candidate = backend.candidate(&target, &scope, deadline)?;
            live(authority, token, &target, deadline, gate)?;
            if candidate.scope != scope {
                return Err(error(
                    "ownership_changed",
                    "Candidate replaced the pinned construction scope",
                ));
            }
            let admitted = match (&candidate.input, target.stage) {
                (Input::Point(point), Stage::EnsureOpen | Stage::SelectBuilding | Stage::Place) => {
                    !(point.input_authorized
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
                            .any(|(v, n)| *v < 0 || *v >= i32::from(n)))
                }
                (Input::FindKey, Stage::FindOpen) | (Input::Return, Stage::FindActivate) => true,
                (Input::Digits(digits), Stage::FindQuery) => {
                    target.state_id.and_then(|v| state_digits(v).ok()).as_ref() == Some(digits)
                }
                _ => false,
            };
            if !admitted {
                return Err(error(
                    "construction_precondition",
                    "Bounded conditional candidate cannot assert frame/routing/input authority",
                ));
            }
            let fresh = backend.observe_before_input(
                &target,
                read_recovery::Phase::BeforeIntent,
                deadline,
            )?;
            fresh.state.validate(&target)?;
            if fresh.scope != scope {
                return Err(error(
                    "ownership_changed",
                    "Fresh observation replaced pinned construction scope",
                ));
            }
            live(authority, token, &target, deadline, gate)?;
            domains_clear(domains, &target.epoch.game)?;
            if fresh.state.reached(&target) {
                return self.noop(id, target, fresh.state);
            }
            fresh.state.preflight(&target)?;
            if self
                .file
                .metadata()?
                .len()
                .saturating_add((2 * LINE) as u64)
                > LIMIT
            {
                return Err(error(
                    "journal_full",
                    "Reserve intent and bounded terminal record before ordinary input",
                ));
            }
            let intent = self.append(Receipt {
                version: 1,
                request_id: id.into(),
                target: target.clone(),
                status: Status::Intent,
                delivery: None,
                observed: fresh.state,
                uncertainty: None,
                cause: None,
            })?;
            // Read-only final checks AFTER fsync, so a scope replacement during
            // durable intent cannot slip into dispatch. Backend additionally checks
            // the exact point/scope immediately inside the ordinary dispatcher.
            let final_check = (|| -> Result<()> {
                live(authority, token, &target, deadline, gate)?;
                domains_clear(domains, &target.epoch.game)?;
                let state = backend.observe_before_input(
                    &target,
                    read_recovery::Phase::AfterIntent,
                    deadline,
                )?;
                state.state.validate(&target)?;
                if state.scope != scope {
                    return Err(error(
                        "ownership_changed",
                        "Pinned construction identity scope changed before dispatch",
                    ));
                }
                if state.state.reached(&target) {
                    return Err(error(
                        "construction_precondition",
                        "Stage became complete before dispatch; no toggle or selection queued",
                    ));
                }
                state.state.preflight(&target)?;
                live(authority, token, &target, deadline, gate)
            })();
            if let Err(cause) = final_check {
                return self.not_queued(intent, cause, token);
            }
            let outcome = match live(authority, token, &target, deadline, gate)
                .and_then(|()| domains_clear(domains, &target.epoch.game))
                .and_then(|()| permit(gate))
            {
                Ok(permit) => {
                    let delivery = backend.dispatch_once(&target, &candidate, deadline);
                    drop(permit);
                    delivery
                }
                Err(cause) => DispatchOutcome::NotQueued(cause),
            };
            let delivery = match outcome {
                DispatchOutcome::NotQueued(cause) => return self.not_queued(intent, cause, token),
                DispatchOutcome::Unknown(Some(cause)) => {
                    return Err(self.uncertain(
                        token,
                        &intent,
                        Delivery::Unknown,
                        intent.observed.clone(),
                        Uncertainty::Dispatch,
                        cause,
                    ))
                }
                DispatchOutcome::Unknown(None) => Delivery::Unknown,
                DispatchOutcome::ServerConfirmed => Delivery::ServerConfirmed,
            };
            let mut last = intent.observed.clone();
            loop {
                if let Err(cause) = live(authority, token, &target, deadline, gate) {
                    return Err(self.uncertain(
                        token,
                        &intent,
                        delivery,
                        last,
                        Uncertainty::DeadlineOrAuthority,
                        cause,
                    ));
                }
                if let Err(cause) = domains_clear(domains, &target.epoch.game) {
                    return Err(self.uncertain(
                        token,
                        &intent,
                        delivery,
                        last,
                        Uncertainty::Observation,
                        cause,
                    ));
                }
                match backend.observe(&target) {
                    Err(e) if e.code == "torn_read" => {}
                    Err(cause) => {
                        return Err(self.uncertain(
                            token,
                            &intent,
                            delivery,
                            last,
                            Uncertainty::Observation,
                            cause,
                        ))
                    }
                    Ok(observed) => {
                        if let Err(cause) = observed.state.validate(&target) {
                            return Err(self.uncertain(
                                token,
                                &intent,
                                delivery,
                                last,
                                Uncertainty::Ownership,
                                cause,
                            ));
                        }
                        if observed.scope != scope {
                            return Err(self.uncertain(
                                token,
                                &intent,
                                delivery,
                                last,
                                Uncertainty::Scope,
                                error(
                                    "ownership_changed",
                                    "Own identity scope changed after possible input",
                                ),
                            ));
                        }
                        last = observed.state;
                        if let Err(cause) = live(authority, token, &target, deadline, gate) {
                            return Err(self.uncertain(
                                token,
                                &intent,
                                delivery,
                                last,
                                Uncertainty::DeadlineOrAuthority,
                                cause,
                            ));
                        }
                        if last.reached(&target) {
                            if let Err(cause) = backend.confirm_input_idle(&candidate, deadline) {
                                return Err(self.uncertain(
                                    token,
                                    &intent,
                                    delivery,
                                    last,
                                    Uncertainty::InputIdle,
                                    cause,
                                ));
                            }
                            if let Err(cause) = live(authority, token, &target, deadline, gate) {
                                return Err(self.uncertain(
                                    token,
                                    &intent,
                                    delivery,
                                    last,
                                    Uncertainty::DeadlineOrAuthority,
                                    cause,
                                ));
                            }
                            // Idle acknowledgement is not a canonical state read.
                            // Require another same-scope witnessed result afterwards.
                            let terminal =
                                match backend.observe(&target) {
                                    Ok(v) => v,
                                    // A torn terminal witness proves nothing either way
                                    // (Run090 tore in the panel's choice list right after
                                    // the place click). Re-observe and re-confirm idle
                                    // from the top within the same original deadline.
                                    Err(e) if e.code == "torn_read" => {
                                        thread::sleep(Duration::from_millis(20).min(
                                            deadline.saturating_duration_since(Instant::now()),
                                        ));
                                        continue;
                                    }
                                    Err(cause) => {
                                        return Err(self.uncertain(
                                            token,
                                            &intent,
                                            delivery,
                                            last,
                                            Uncertainty::Observation,
                                            cause,
                                        ))
                                    }
                                };
                            if let Err(cause) = terminal.state.validate(&target) {
                                return Err(self.uncertain(
                                    token,
                                    &intent,
                                    delivery,
                                    last,
                                    Uncertainty::Ownership,
                                    cause,
                                ));
                            }
                            if terminal.scope != scope {
                                return Err(self.uncertain(
                                    token,
                                    &intent,
                                    delivery,
                                    last,
                                    Uncertainty::Scope,
                                    error(
                                        "ownership_changed",
                                        "Identity scope changed during input-idle confirmation",
                                    ),
                                ));
                            }
                            last = terminal.state;
                            if let Err(cause) = live(authority, token, &target, deadline, gate) {
                                return Err(self.uncertain(
                                    token,
                                    &intent,
                                    delivery,
                                    last,
                                    Uncertainty::DeadlineOrAuthority,
                                    cause,
                                ));
                            }
                            if last.reached(&target) {
                                let receipt = self
                                    .append(Receipt {
                                        status: Status::Verified,
                                        delivery: Some(delivery),
                                        observed: last,
                                        ..intent
                                    })
                                    .map_err(|cause| {
                                        Error::after_possible_input(
                                        "Verified state could not be committed; preserve intent",
                                        cause,
                                    )
                                    })?;
                                return Ok(Execution {
                                    receipt,
                                    historical: false,
                                    current_state_verified: true,
                                    event_causality_verified: false,
                                });
                            }
                        }
                    }
                }
                thread::sleep(
                    Duration::from_millis(20)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
        })();
        result.map_err(|cause| {
            let diagnostic = Diagnostic::from_error(&cause, token);
            let code = if diagnostic.code == cause.code {
                cause.code
            } else {
                "diagnostic_code_redacted"
            };
            Error::new(code, diagnostic.message)
        })
    }
}

#[cfg(test)]
mod tests;
