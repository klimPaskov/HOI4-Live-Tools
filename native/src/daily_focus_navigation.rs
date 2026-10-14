//! Prospective durable Focus preparation. No worker registration or authority.
#![allow(dead_code)] // Hard unavailable until independently reviewed live gate.
use crate::{
    actions::Epoch,
    daily_governor::{Entry, Intent},
    error::{Error, Result},
    focus_stage::{ActiveFocus, Execution, OwnState, Receipt, Stage, Status, Target},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub(crate) fn capability() -> Result<()> {
    Err(Error::new("capability_unavailable","Daily Focus input is not enabled; separate live Start acceptance and wiring review required"))
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OwnBinding {
    focus: [u64; 7],
    politics: [u64; 6],
    slots: Vec<u64>,
}
impl OwnBinding {
    pub(crate) fn from_context(
        focus: &crate::own_focus::FocusContext,
        politics: &crate::politics_focus::PoliticsContext,
    ) -> Result<Self> {
        let q = politics.anchors();
        let a = &focus.campaign;
        let b = politics.campaign();
        if a.player != b.player
            || a.playthrough_id != b.playthrough_id
            || a.anchors != b.anchors
            || a.difficulty != b.difficulty
            || a.historical_raw != b.historical_raw
            || a.date_raw > b.date_raw
            || [
                focus.anchors[0],
                focus.anchors[5],
                focus.anchors[4],
                focus.anchors[3],
            ] != [q[0], q[1], q[2], q[4]]
        {
            return Err(Error::new(
                "torn_read",
                "Own Focus/politics bindings disagree",
            ));
        }
        let value = Self {
            focus: focus.anchors,
            politics: q,
            slots: focus.slots.clone(),
        };
        value.validate()?;
        Ok(value)
    }
    fn validate(&self) -> Result<()> {
        if [self.focus[0], self.focus[5], self.focus[4], self.focus[3]]
            != [
                self.politics[0],
                self.politics[1],
                self.politics[2],
                self.politics[4],
            ]
        {
            return Err(invalid());
        }
        if self
            .focus
            .iter()
            .chain(self.politics.iter())
            .any(|p| *p == 0 || *p % 8 != 0)
            || self.slots.is_empty()
            || self.slots.len() > 32
            || self.slots.iter().any(|p| *p == 0 || *p % 8 != 0)
            || self
                .slots
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.slots.len()
        {
            return Err(invalid());
        }
        Ok(())
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Failure {
    code: String,
    reason: String,
    evidence: crate::focus_stage::FailureEvidence,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    entry: Entry,
    target: Target,
    day: i64,
    revision: u64,
    index: u64,
    id: String,
    binding: OwnBinding,
    receipt: Option<Receipt>,
    failure: Option<Failure>,
    checksum: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Data {
    version: u8,
    epoch: Epoch,
    steps: Vec<Step>,
}
pub(crate) struct Navigator {
    store: crate::daily_governor::store::Store,
    data: Data,
    poisoned: bool,
}
fn invalid() -> Error {
    Error::new(
        "invalid_focus_navigation",
        "Bounded exact Focus preparation history required",
    )
}
fn hash<T: Serialize>(v: &T) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(v).map_err(|_| invalid())?)
    ))
}
fn id(s: &Step) -> Result<String> {
    hash(&(
        "daily-focus-preparation-v1",
        &s.entry,
        &s.target,
        s.day,
        s.revision,
        s.index,
        &s.binding,
    ))
}
fn checksum(s: &Step) -> Result<String> {
    hash(&(
        &s.entry, &s.target, s.day, s.revision, s.index, &s.id, &s.binding, &s.receipt, &s.failure,
    ))
}
fn validate_step(s: &Step, epoch: &Epoch, entries: &[Entry], index: usize) -> Result<()> {
    s.entry.validate()?;
    s.binding.validate()?;
    let Intent::Focus { focus_id } = &s.entry.intent else {
        return Err(invalid());
    };
    if !entries.contains(&s.entry)
        || s.target.epoch != *epoch
        || s.target.focus_id != *focus_id
        || s.target.stage == Stage::Start
        || !(1936 * 365..2001 * 365).contains(&s.day)
        || s.revision == 0
        || s.index != index as u64 + 1
        || s.id != id(s)?
        || s.checksum != checksum(s)?
    {
        return Err(invalid());
    }
    if let Some(r) = &s.receipt {
        r.observed.validate(&r.target)?;
        if s.failure.is_some()
            || r.version != 1
            || r.status != Status::Verified
            || r.request_id != s.id
            || r.target != s.target
            || !r.observed.reached(&s.target)
        {
            return Err(invalid());
        }
    }
    if let Some(f) = &s.failure {
        if f.code.is_empty()
            || f.code.len() > 64
            || !f.code.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
            || f.reason.len() > 160
            || f.reason.chars().any(char::is_control)
        {
            return Err(invalid());
        }
    }
    Ok(())
}
impl Navigator {
    pub(crate) fn open(path: &Path, epoch: Epoch, entries: &[Entry]) -> Result<Self> {
        let (store, bytes) = crate::daily_governor::store::Store::open(path)?;
        let data: Data = match bytes {
            Some(bytes) => serde_json::from_slice(&bytes).map_err(|_| invalid())?,
            None => Data {
                version: 1,
                epoch: epoch.clone(),
                steps: vec![],
            },
        };
        if data.version != 1 || data.epoch != epoch || data.steps.len() > 4096 {
            return Err(invalid());
        }
        let mut previous = None;
        for (i, s) in data.steps.iter().enumerate() {
            validate_step(s, &epoch, entries, i)?;
            if previous.is_some_and(|day| s.day < day)
                || (i + 1 < data.steps.len()
                    && s.receipt.is_none()
                    && !s.failure.as_ref().is_some_and(|f| {
                        f.evidence != crate::focus_stage::FailureEvidence::Unknown
                    }))
            {
                return Err(invalid());
            }
            previous = Some(s.day);
        }
        Ok(Self {
            store,
            data,
            poisoned: false,
        })
    }
    fn save(&mut self) -> Result<()> {
        if let Some(last) = self.data.steps.last_mut() {
            last.checksum = checksum(last)?;
        }
        self.store
            .replace(&serde_json::to_vec(&self.data).map_err(|_| invalid())?)
            .inspect_err(|_| self.poisoned = true)
    }
    pub(crate) fn blocked(&self) -> bool {
        self.poisoned
            || self.data.steps.iter().any(|s| {
                s.receipt.is_none()
                    && !s
                        .failure
                        .as_ref()
                        .is_some_and(|f| f.evidence != crate::focus_stage::FailureEvidence::Unknown)
            })
    }
    pub(crate) fn begin(
        &mut self,
        entry: Entry,
        target: Target,
        day: i64,
        revision: u64,
        binding: OwnBinding,
    ) -> Result<String> {
        self.store.check().inspect_err(|_| self.poisoned = true)?;
        if self.blocked()
            || self.data.steps.len() >= 4096
            || self.data.steps.last().is_some_and(|s| s.day > day)
            || self
                .data
                .steps
                .iter()
                .any(|s| s.entry == entry && s.day == day && s.failure.is_some())
        {
            return Err(invalid());
        }
        let mut step = Step {
            entry,
            target,
            day,
            revision,
            index: self.data.steps.len() as u64 + 1,
            id: String::new(),
            binding,
            receipt: None,
            failure: None,
            checksum: String::new(),
        };
        step.id = id(&step)?;
        step.checksum = checksum(&step)?;
        validate_step(
            &step,
            &self.data.epoch,
            std::slice::from_ref(&step.entry),
            self.data.steps.len(),
        )?;
        let id = step.id.clone();
        self.data.steps.push(step);
        self.save()?;
        Ok(id)
    }
    pub(crate) fn verified(&mut self, e: &Execution, binding: &OwnBinding) -> Result<()> {
        self.store.check().inspect_err(|_| self.poisoned = true)?;
        let last = self.data.steps.last_mut().ok_or_else(invalid)?;
        if last.receipt.is_some()
            || last.failure.is_some()
            || &last.binding != binding
            || e.historical
            || !e.current_state_verified
            || e.receipt.version != 1
            || e.receipt.status != Status::Verified
            || e.receipt.request_id != last.id
            || e.receipt.target != last.target
            || !e.receipt.observed.reached(&last.target)
        {
            return Err(Error::new(
                "action_uncertain",
                "Fresh exact Focus preparation receipt/binding required",
            ));
        }
        e.receipt.observed.validate(&last.target)?;
        last.receipt = Some(e.receipt.clone());
        self.save()
    }
    /// Only a trusted immediate adapter classification can mark known pre-input.
    /// Unknown/lost delivery leaves the ledger globally blocked; never retries.
    pub(crate) fn failed(
        &mut self,
        error: &Error,
        evidence: crate::focus_stage::FailureEvidence,
    ) -> Result<()> {
        self.store.check().inspect_err(|_| self.poisoned = true)?;
        let last = self.data.steps.last_mut().ok_or_else(invalid)?;
        if last.receipt.is_some()
            || last.failure.is_some()
            || (error.code == "action_uncertain"
                && evidence != crate::focus_stage::FailureEvidence::Unknown)
        {
            return Err(invalid());
        }
        last.failure = Some(Failure {
            code: error.code.into(),
            reason: crate::daily_navigation::bounded_reason(&error.message),
            evidence,
        });
        self.save()
    }
}
pub(crate) enum Next {
    Wait,
    Stage(Stage),
    Start,
}
pub(crate) fn next(state: &OwnState, target: &Target) -> Result<Next> {
    state.validate(target)?;
    match &state.active_focus {
        ActiveFocus::Unknown => {
            return Err(Error::new(
                "focus_unavailable",
                "Unknown active focus is not idle",
            ))
        }
        ActiveFocus::Active(key) if key != &target.focus_id => return Ok(Next::Wait),
        ActiveFocus::Active(_) => return Ok(Next::Start),
        ActiveFocus::Idle => {}
    }
    for stage in [
        Stage::OpenPolitics,
        Stage::OpenFocus,
        Stage::View,
        Stage::Start,
    ] {
        let target = Target {
            stage,
            ..target.clone()
        };
        if state.preflight(&target).is_ok() {
            return Ok(if stage == Stage::Start {
                Next::Start
            } else {
                Next::Stage(stage)
            });
        }
    }
    Err(Error::new(
        "focus_precondition",
        "Current own roots/blockers cannot be navigated; no blind transitions",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        focus_stage::{Delivery, FailureEvidence, LocalRoot},
        platform::Identity,
    };
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let p = std::env::temp_dir().join(format!(
                "private-focus-foundation-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&p).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self(p)
        }
        fn path(&self) -> std::path::PathBuf {
            self.0.join("focus-navigation.json")
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn epoch() -> Epoch {
        Epoch {
            game: Identity {
                pid: 123,
                start_ticks: "456".into(),
                exe: "/generated/game".into(),
                uid: 1000,
            },
            campaign: "generated".into(),
            player: "GER".into(),
        }
    }
    fn entry() -> Entry {
        Entry {
            id: "focus-head".into(),
            intent: Intent::Focus {
                focus_id: "GER_rhineland".into(),
            },
            navigation: None,
        }
    }
    fn target(stage: Stage) -> Target {
        Target {
            epoch: epoch(),
            stage,
            focus_id: "GER_rhineland".into(),
        }
    }
    fn binding() -> OwnBinding {
        OwnBinding {
            focus: [0x1000, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000, 0x7000],
            politics: [0x1000, 0x6000, 0x5000, 0x8000, 0x4000, 0x9000],
            slots: vec![0xa000],
        }
    }
    fn state() -> OwnState {
        OwnState {
            epoch: epoch(),
            active_focus: ActiveFocus::Idle,
            canonical_focus_id: Some("GER_rhineland".into()),
            managed_mode: Some(23),
            politics_root: LocalRoot::Hidden,
            focus_root: LocalRoot::Hidden,
            own_detail_counts: Some([0, 0]),
            own_research_info_counts: Some([0, 0]),
            selected_focus: None,
            requested_detail_ready: Some(false),
        }
    }
    fn receipt(id: String) -> Execution {
        let mut s = state();
        s.managed_mode = Some(12);
        s.politics_root = LocalRoot::Ready;
        Execution {
            receipt: Receipt {
                version: 1,
                request_id: id,
                target: target(Stage::OpenPolitics),
                status: Status::Verified,
                delivery: Some(Delivery::ServerConfirmed),
                observed: s,
            },
            historical: false,
            current_state_verified: true,
            event_causality_verified: false,
        }
    }
    #[test]
    fn idle_navigation_active_wait_unknown_refusal_and_hard_gate() {
        assert_eq!(capability().unwrap_err().code, "capability_unavailable");
        let mut s = state();
        assert!(matches!(
            next(&s, &target(Stage::Start)).unwrap(),
            Next::Stage(Stage::OpenPolitics)
        ));
        s.active_focus = ActiveFocus::Active("GER_other".into());
        assert!(matches!(
            next(&s, &target(Stage::Start)).unwrap(),
            Next::Wait
        ));
        s.active_focus = ActiveFocus::Unknown;
        assert_eq!(
            next(&s, &target(Stage::Start)).err().unwrap().code,
            "focus_unavailable"
        );
        s.active_focus = ActiveFocus::Idle;
        s.managed_mode = Some(6);
        assert!(next(&s, &target(Stage::Start)).is_err());
        s = state();
        s.managed_mode = Some(12);
        s.politics_root = LocalRoot::Ready;
        assert!(matches!(
            next(&s, &target(Stage::Start)).unwrap(),
            Next::Stage(Stage::OpenFocus)
        ));
        s.managed_mode = Some(13);
        s.politics_root = LocalRoot::Hidden;
        s.focus_root = LocalRoot::Ready;
        assert!(matches!(
            next(&s, &target(Stage::Start)).unwrap(),
            Next::Stage(Stage::View)
        ));
        s.own_detail_counts = Some([1, 0]);
        s.selected_focus = Some("GER_rhineland".into());
        s.requested_detail_ready = Some(true);
        assert!(matches!(
            next(&s, &target(Stage::Start)).unwrap(),
            Next::Start
        ));
    }
    #[test]
    fn durable_preparation_receipt_is_fresh_exact_binding_and_start_excluded() {
        let t = Scratch::new();
        let e = entry();
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        let id = n
            .begin(
                e.clone(),
                target(Stage::OpenPolitics),
                1936 * 365,
                1,
                binding(),
            )
            .unwrap();
        assert!(n.blocked());
        let mut r = receipt(id);
        r.historical = true;
        assert!(n.verified(&r, &binding()).is_err());
        r.historical = false;
        let mut foreign = binding();
        foreign.focus[1] += 8;
        assert!(n.verified(&r, &foreign).is_err());
        n.verified(&r, &binding()).unwrap();
        assert!(!n.blocked());
        drop(n);
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        assert!(!n.blocked());
        assert!(n
            .begin(e, target(Stage::Start), 1936 * 365, 1, binding())
            .is_err());
    }
    #[test]
    fn unknown_delivery_blocks_restart_and_preinput_failure_retains_exact_cause() {
        for evidence in [
            FailureEvidence::Unknown,
            FailureEvidence::BeforeIntent,
            FailureEvidence::ConfirmedNotQueued,
        ] {
            let t = Scratch::new();
            let e = entry();
            let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
            n.begin(e.clone(), target(Stage::View), 1936 * 365, 1, binding())
                .unwrap();
            n.failed(
                &Error::new("torn_read", "Exact original own witness reason"),
                evidence,
            )
            .unwrap();
            drop(n);
            let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
            assert_eq!(n.blocked(), evidence == FailureEvidence::Unknown);
            assert_eq!(
                n.data.steps[0].failure.as_ref().unwrap().reason,
                "Exact original own witness reason"
            );
            assert!(n
                .begin(e, target(Stage::View), 1936 * 365, 1, binding())
                .is_err());
        }
    }
    #[test]
    fn forged_entry_target_day_binding_and_late_unknown_history_refuse() {
        for field in ["entry", "target", "day", "binding"] {
            let t = Scratch::new();
            let e = entry();
            let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
            let id = n
                .begin(
                    e.clone(),
                    target(Stage::OpenPolitics),
                    1936 * 365,
                    1,
                    binding(),
                )
                .unwrap();
            n.verified(&receipt(id), &binding()).unwrap();
            drop(n);
            let mut d: Data = serde_json::from_slice(&std::fs::read(t.path()).unwrap()).unwrap();
            let s = &mut d.steps[0];
            match field {
                "entry" => s.entry.id = "forged".into(),
                "target" => s.target.focus_id = "GER_other".into(),
                "day" => s.day += 1,
                _ => s.binding.focus[1] += 8,
            };
            s.checksum = checksum(s).unwrap();
            std::fs::write(t.path(), serde_json::to_vec(&d).unwrap()).unwrap();
            assert!(Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).is_err());
        }
    }
    #[test]
    fn lost_response_and_external_store_mutation_never_allow_another_stage() {
        let t = Scratch::new();
        let e = entry();
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        n.begin(
            e.clone(),
            target(Stage::OpenPolitics),
            1936 * 365,
            1,
            binding(),
        )
        .unwrap();
        drop(n);
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&e)).unwrap();
        assert!(n.blocked());
        assert!(n
            .begin(e, target(Stage::OpenFocus), 1936 * 365, 1, binding())
            .is_err());
        std::fs::write(t.path(), b"changed").unwrap();
        assert!(n
            .failed(
                &Error::new("input_busy", "preflight"),
                FailureEvidence::BeforeIntent
            )
            .is_err());
        assert!(n.blocked());
    }
}
