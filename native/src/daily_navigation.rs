//! Durable verified preparation progress, separate from research completion.
use crate::{
    actions::Epoch,
    daily_governor::Entry,
    error::{Error, Result},
    research_stage::{Execution, OwnState, Stage, Status, Target},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    entry: Entry,
    index: u64,
    target: Target,
    id: String,
    terminal: Option<bool>,
    day: i64,
    receipt: Option<crate::research_stage::Receipt>,
    checksum: String,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Refusal {
    entry: Entry,
    day: i64,
    code: String,
    reason: String,
    checksum: String,
}
pub(crate) fn known_preparation_refusal(code: &str) -> bool {
    matches!(
        code,
        "research_precondition"
            | "research_unavailable"
            | "capability_unavailable"
            | "input_not_queued"
    )
}
pub(crate) fn bounded_reason(message: &str) -> String {
    let mut out = String::new();
    for c in message.chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > 160 {
            break;
        }
        out.push(c);
    }
    out
}
fn refusal_checksum(r: &Refusal) -> Result<String> {
    digest(&(&r.entry, r.day, &r.code, &r.reason))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Data {
    version: u8,
    epoch: Epoch,
    steps: Vec<Step>,
    #[serde(default)]
    refusals: Vec<Refusal>,
}
pub(crate) struct Navigator {
    store: crate::daily_governor::store::Store,
    data: Data,
    poisoned: bool,
}
impl Navigator {
    pub(crate) fn open(path: &Path, epoch: Epoch, entries: &[Entry]) -> Result<Self> {
        let (store, bytes) = crate::daily_governor::store::Store::open(path)?;
        let data = if let Some(bytes) = bytes {
            serde_json::from_slice::<Data>(&bytes)
                .map_err(|e| Error::new("invalid_navigation", e.to_string()))?
        } else {
            Data {
                version: 2,
                epoch: epoch.clone(),
                steps: vec![],
                refusals: vec![],
            }
        };
        validate_history(&data, &epoch)?;
        if data
            .steps
            .iter()
            .any(|s| !entries.iter().any(|e| e == &s.entry))
        {
            return Err(invalid());
        }
        if data
            .refusals
            .iter()
            .any(|r| !entries.iter().any(|e| e == &r.entry))
        {
            return Err(invalid());
        }
        Ok(Self {
            store,
            data,
            poisoned: false,
        })
    }
    pub(crate) fn blocked(&self) -> bool {
        self.poisoned || self.data.steps.iter().any(|s| s.terminal.is_none())
    }
    fn save(&mut self) -> Result<()> {
        let outcome = (|| {
            for step in &mut self.data.steps {
                step.checksum = checksum(step)?;
            }
            validate_history(&self.data, &self.data.epoch)?;
            let bytes = serde_json::to_vec(&self.data).map_err(|_| invalid())?;
            self.store.replace(&bytes)
        })();
        if outcome.is_err() {
            self.poisoned = true;
        }
        outcome
    }
    pub(crate) fn refused_today(&self, entry_id: &str, day: i64) -> bool {
        self.data
            .refusals
            .iter()
            .any(|r| r.entry.id == entry_id && r.day == day)
            || self
                .data
                .steps
                .iter()
                .rev()
                .find(|s| s.entry.id == entry_id)
                .is_some_and(|s| s.terminal == Some(false) && s.day == day)
    }
    pub(crate) fn latest_refusal(&self) -> Option<serde_json::Value> {
        self.data.refusals.last().map(|r|serde_json::json!({"code":r.code,"entry_id":r.entry.id,"reason":r.reason,"observed_on_day":r.day,"historical":true}))
    }
    pub(crate) fn refuse_preparation(
        &mut self,
        entry: &Entry,
        day: i64,
        error: &Error,
    ) -> Result<()> {
        if let Err(e) = self.store.check() {
            self.poisoned = true;
            return Err(e);
        }
        if self.blocked() || !known_preparation_refusal(error.code) {
            return Err(invalid());
        }
        entry.validate()?;
        if !matches!(entry.intent, crate::daily_governor::Intent::Research { .. }) {
            return Err(invalid());
        }
        if !(1936 * 365..2001 * 365).contains(&day) {
            return Err(invalid());
        }
        if let Some(old) = self
            .data
            .refusals
            .iter()
            .find(|r| r.entry.id == entry.id && r.day == day)
        {
            return if old.entry == *entry {
                Ok(())
            } else {
                Err(invalid())
            };
        }
        if self.data.steps.len() + self.data.refusals.len() >= 4096 {
            return Err(Error::new(
                "journal_full",
                "Bounded preparation history full; no input",
            ));
        }
        let mut r = Refusal {
            entry: entry.clone(),
            day,
            code: error.code.into(),
            reason: bounded_reason(&error.message),
            checksum: String::new(),
        };
        r.checksum = refusal_checksum(&r)?;
        self.data.refusals.push(r);
        self.save()
    }
    pub(crate) fn begin(&mut self, entry: &Entry, target: Target, day: i64) -> Result<String> {
        self.store.check()?;
        if self.blocked() {
            return Err(Error::new(
                "action_uncertain",
                "Unresolved navigation cannot replay",
            ));
        }
        if self.data.steps.len() + self.data.refusals.len() >= 4096
            || target.epoch != self.data.epoch
            || target.stage == Stage::Research
        {
            return Err(Error::new(
                "invalid_navigation",
                "Bounded preparation stage required",
            ));
        }
        let index = self.data.steps.len() as u64 + 1;
        validate_binding(entry, &target, day)?;
        let id = request_id(entry, &target, index, day)?;
        self.data.steps.push(Step {
            entry: entry.clone(),
            index,
            target,
            id: id.clone(),
            terminal: None,
            day,
            receipt: None,
            checksum: String::new(),
        });
        self.save()?;
        Ok(id)
    }
    pub(crate) fn verified(&mut self, execution: &Execution) -> Result<()> {
        let last = self
            .data
            .steps
            .last_mut()
            .ok_or_else(|| Error::new("invalid_navigation", "No preparation intent"))?;
        if execution.receipt.version != 1
            || execution.historical
            || !execution.current_state_verified
            || execution.receipt.status != Status::Verified
            || execution.receipt.request_id != last.id
            || execution.receipt.target != last.target
        {
            return Err(Error::new(
                "action_uncertain",
                "Fresh matching preparation receipt required",
            ));
        }
        execution.receipt.observed.validate(&last.target)?;
        if !execution.receipt.observed.reached(&last.target) {
            return Err(Error::new(
                "action_uncertain",
                "Preparation postcondition not reached",
            ));
        }
        if last.terminal.is_some() {
            return Err(Error::new(
                "invalid_navigation",
                "Preparation already terminal",
            ));
        }
        last.receipt = Some(execution.receipt.clone());
        last.terminal = Some(true);
        self.save()
    }
    pub(crate) fn not_queued(&mut self) -> Result<()> {
        let last = self
            .data
            .steps
            .last_mut()
            .ok_or_else(|| Error::new("invalid_navigation", "Missing intent"))?;
        if last.terminal.is_some() {
            return Err(Error::new(
                "invalid_navigation",
                "Preparation already terminal",
            ));
        }
        last.terminal = Some(false);
        self.save()
    }
}
fn invalid() -> Error {
    Error::new(
        "invalid_navigation",
        "Stored navigation binding/history invalid",
    )
}
fn digest(value: &impl Serialize) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(value).map_err(|_| invalid())?)
    ))
}
fn request_id(entry: &Entry, target: &Target, index: u64, day: i64) -> Result<String> {
    digest(&(entry, target, index, day))
}
fn checksum(s: &Step) -> Result<String> {
    digest(&(
        &s.entry, s.index, &s.target, &s.id, s.terminal, s.day, &s.receipt,
    ))
}
fn validate_binding(entry: &Entry, target: &Target, day: i64) -> Result<()> {
    entry.validate()?;
    target.validate_key()?;
    let crate::daily_governor::Intent::Research {
        technology_id,
        slot,
    } = &entry.intent
    else {
        return Err(invalid());
    };
    if *slot > 31
        || target.slot != *slot
        || target.stage == Stage::Research
        || !(1936 * 365..2001 * 365).contains(&day)
    {
        return Err(invalid());
    }
    if target.stage == Stage::SelectCategory {
        if !target.technology.is_empty()
            || target.folder_id.as_deref()
                != entry.navigation.as_ref().map(|n| n.folder_id.as_str())
            || target.folder_id.is_none()
        {
            return Err(invalid());
        }
    } else if target.technology != *technology_id || target.folder_id.is_some() {
        return Err(invalid());
    }
    Ok(())
}
fn validate_history(data: &Data, epoch: &Epoch) -> Result<()> {
    if data.version != 2 || data.epoch != *epoch || data.steps.len() > 4096 {
        return Err(invalid());
    }
    if data.steps.len() + data.refusals.len() > 4096 {
        return Err(invalid());
    }
    let mut refusals = std::collections::BTreeSet::new();
    for r in &data.refusals {
        r.entry.validate()?;
        if !matches!(
            r.entry.intent,
            crate::daily_governor::Intent::Research { .. }
        ) {
            return Err(invalid());
        }
        if !(1936 * 365..2001 * 365).contains(&r.day)
            || !known_preparation_refusal(&r.code)
            || r.reason.len() > 160
            || r.reason.chars().any(char::is_control)
            || r.checksum != refusal_checksum(r)?
            || !refusals.insert((&r.entry.id, r.day))
        {
            return Err(invalid());
        }
    }
    let mut ids = std::collections::BTreeSet::new();
    let mut entries = std::collections::BTreeMap::new();
    let mut last_day = None;
    for (i, s) in data.steps.iter().enumerate() {
        validate_binding(&s.entry, &s.target, s.day)?;
        if s.index != i as u64 + 1
            || s.target.epoch != *epoch
            || s.id.len() != 64
            || !s.id.bytes().all(|b| b.is_ascii_hexdigit())
            || !ids.insert(&s.id)
            || s.id != request_id(&s.entry, &s.target, s.index, s.day)?
            || s.checksum != checksum(s)?
            || last_day.is_some_and(|d| s.day < d)
        {
            return Err(invalid());
        }
        if let Some(old) = entries.insert(&s.entry.id, &s.entry) {
            if old != &s.entry {
                return Err(invalid());
            }
        }
        match (s.terminal, &s.receipt) {
            (Some(true), Some(r)) => {
                if r.version != 1
                    || r.request_id != s.id
                    || r.target != s.target
                    || r.status != Status::Verified
                {
                    return Err(invalid());
                }
                r.observed.validate(&s.target)?;
                if !r.observed.reached(&s.target) {
                    return Err(invalid());
                }
            }
            (Some(false), None) => {}
            (None, None) if i + 1 == data.steps.len() => {}
            _ => return Err(invalid()),
        }
        last_day = Some(s.day);
    }
    Ok(())
}
/// Choose from current canonical/UI state, never from stored receipt freshness.
pub(crate) fn next(entry: &Entry, state: &OwnState) -> Result<Option<Target>> {
    let crate::daily_governor::Intent::Research {
        technology_id,
        slot,
    } = &entry.intent
    else {
        return Ok(None);
    };
    if state
        .researching
        .get(*slot as usize)
        .and_then(Option::as_deref)
        == Some(technology_id.as_str())
    {
        return Ok(None);
    }
    if state.researching.get(*slot as usize) != Some(&None) {
        return Err(Error::new(
            "research_precondition",
            "Own target slot occupied or unavailable",
        ));
    }
    let which = if state.active_mode == Some(23)
        && state.panel_visible == Some(false)
        && !state.tree_visible
        && state.visible_info.is_empty()
    {
        Stage::OpenPanel
    } else if state.active_mode == Some(5)
        && state.panel_visible == Some(true)
        && !state.tree_visible
        && state.visible_info.is_empty()
    {
        Stage::SelectSlot
    } else if state.tree_visible
        && state.selected_slot == Some(*slot)
        && state.visible_info.is_empty()
    {
        if entry
            .navigation
            .as_ref()
            .is_some_and(|n| state.selected_folder.as_deref() != Some(n.folder_id.as_str()))
        {
            Stage::SelectCategory
        } else {
            Stage::View
        }
    } else if state.tree_visible
        && state.selected_slot == Some(*slot)
        && state.visible_info.as_slice() == [Some(technology_id.clone())]
    {
        return Ok(None);
    } else {
        return Err(Error::new(
            "research_precondition",
            "Unknown/other own UI must be resolved; no blind navigation",
        ));
    };
    Ok(Some(Target {
        epoch: state.epoch.clone(),
        stage: which,
        technology: if which == Stage::SelectCategory {
            String::new()
        } else {
            technology_id.clone()
        },
        folder_id: if which == Stage::SelectCategory {
            entry.navigation.as_ref().map(|n| n.folder_id.clone())
        } else {
            None
        },
        slot: *slot,
    }))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::{
        daily_governor::{Intent, ResearchNavigation},
        platform::Identity,
    };
    use std::{
        fs,
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicU64, Ordering},
    };
    fn epoch() -> Epoch {
        Epoch {
            game: Identity {
                pid: 123,
                start_ticks: "456".into(),
                exe: "/owned/game".into(),
                uid: 1000,
            },
            campaign: "campaign".into(),
            player: "GER".into(),
        }
    }
    fn entry() -> Entry {
        Entry {
            id: "research_1".into(),
            intent: Intent::Research {
                technology_id: "support_weapons".into(),
                slot: 0,
            },
            navigation: Some(ResearchNavigation {
                folder_id: "infantry".into(),
            }),
        }
    }
    fn state() -> OwnState {
        OwnState {
            epoch: epoch(),
            selected_slot: None,
            selected_folder: None,
            tree_visible: false,
            panel_visible: Some(false),
            active_mode: Some(23),
            researching: vec![None, None],
            visible_info: vec![],
        }
    }
    struct Temp(std::path::PathBuf);
    impl Temp {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let p = std::env::temp_dir().join(format!(
                "private-navigation-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
            Self(p)
        }
        fn path(&self) -> std::path::PathBuf {
            self.0.join("navigation.json")
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn receipt(id: String, target: Target, observed: OwnState) -> Execution {
        Execution {
            receipt: crate::research_stage::Receipt {
                version: 1,
                request_id: id,
                target,
                status: Status::Verified,
                delivery: None,
                observed,
            },
            historical: false,
            current_state_verified: true,
            event_causality_verified: false,
        }
    }
    #[test]
    fn pipeline_uses_fresh_state_and_never_research_receipt() {
        let e = entry();
        let mut s = state();
        assert_eq!(next(&e, &s).unwrap().unwrap().stage, Stage::OpenPanel);
        s.active_mode = Some(5);
        s.panel_visible = Some(true);
        assert_eq!(next(&e, &s).unwrap().unwrap().stage, Stage::SelectSlot);
        s.active_mode = Some(6);
        s.panel_visible = Some(false);
        s.tree_visible = true;
        s.selected_slot = Some(0);
        assert_eq!(next(&e, &s).unwrap().unwrap().stage, Stage::SelectCategory);
        s.selected_folder = Some("infantry".into());
        assert_eq!(next(&e, &s).unwrap().unwrap().stage, Stage::View);
        s.visible_info = vec![Some("support_weapons".into())];
        assert!(next(&e, &s).unwrap().is_none());
        s.visible_info = vec![Some("other".into())];
        assert!(next(&e, &s).is_err());
        s.visible_info.clear();
        s.researching[0] = Some("other".into());
        assert!(next(&e, &s).is_err());
    }
    #[test]
    fn unresolved_restart_blocks_and_verified_restart_reobserves() {
        let t = Temp::new();
        let e = entry();
        let s = state();
        let target = next(&e, &s).unwrap().unwrap();
        let mut n = Navigator::open(&t.path(), epoch(), &[entry()]).unwrap();
        let id = n.begin(&e, target.clone(), 1936 * 365 + 12).unwrap();
        drop(n);
        let mut n = Navigator::open(&t.path(), epoch(), &[entry()]).unwrap();
        assert!(n.blocked());
        assert!(n.begin(&e, target.clone(), 1936 * 365 + 13).is_err());
        let mut post = s.clone();
        post.active_mode = Some(5);
        post.panel_visible = Some(true);
        n.verified(&receipt(id, target, post.clone())).unwrap();
        drop(n);
        let n = Navigator::open(&t.path(), epoch(), &[entry()]).unwrap();
        assert!(!n.blocked());
        assert_eq!(next(&e, &post).unwrap().unwrap().stage, Stage::SelectSlot);
        assert_eq!(next(&e, &s).unwrap().unwrap().stage, Stage::OpenPanel);
    }
    #[test]
    fn historical_wrong_target_and_unreached_cannot_resolve_intent() {
        let t = Temp::new();
        let e = entry();
        let s = state();
        let target = next(&e, &s).unwrap().unwrap();
        let mut n = Navigator::open(&t.path(), epoch(), &[entry()]).unwrap();
        let id = n.begin(&e, target.clone(), 1936 * 365 + 1).unwrap();
        let mut x = receipt(id, target, s);
        assert!(n.verified(&x).is_err());
        assert!(n.blocked());
        x.receipt.observed.active_mode = Some(5);
        x.receipt.observed.panel_visible = Some(true);
        x.historical = true;
        assert!(n.verified(&x).is_err());
        x.historical = false;
        x.receipt.request_id = "wrong".into();
        assert!(n.verified(&x).is_err());
        assert!(n.blocked());
    }
    #[test]
    fn local_refusal_backoff_and_revised_entry_keep_history() {
        let t = Temp::new();
        let e = entry();
        let target = next(&e, &state()).unwrap().unwrap();
        let mut n = Navigator::open(&t.path(), epoch(), &[entry()]).unwrap();
        let first = n.begin(&e, target.clone(), 1936 * 365 + 7).unwrap();
        n.not_queued().unwrap();
        assert!(n.refused_today("research_1", 1936 * 365 + 7));
        assert!(!n.refused_today("research_1", 1936 * 365 + 8));
        let second = n.begin(&e, target, 1936 * 365 + 8).unwrap();
        assert_ne!(first, second);
        assert_eq!(n.data.steps.len(), 2);
    }
    #[test]
    fn omitted_hint_preserves_old_serialized_request() {
        let old = r#"{"id":"research_1","intent":{"kind":"Research","technology_id":"support_weapons","slot":0}}"#;
        let mut e = entry();
        e.navigation = None;
        let bytes = serde_json::to_string(&e).unwrap();
        assert_eq!(bytes, old);
        assert_eq!(serde_json::from_str::<Entry>(old).unwrap(), e);
    }
    #[test]
    fn external_mutation_and_one_writer_refuse() {
        let t = Temp::new();
        let mut n = Navigator::open(&t.path(), epoch(), &[entry()]).unwrap();
        assert!(Navigator::open(&t.path(), epoch(), &[entry()]).is_err());
        let e = entry();
        let target = next(&e, &state()).unwrap().unwrap();
        n.begin(&e, target.clone(), 1936 * 365 + 1).unwrap();
        n.not_queued().unwrap();
        fs::write(t.path(), b"changed").unwrap();
        assert!(n.begin(&e, target, 1936 * 365 + 2).is_err());
    }
    fn altered_reopen(mut alter: impl FnMut(&mut Data)) {
        let t = Temp::new();
        let mut n = Navigator::open(&t.path(), epoch(), &[entry()]).unwrap();
        n.begin(
            &entry(),
            next(&entry(), &state()).unwrap().unwrap(),
            1936 * 365 + 7,
        )
        .unwrap();
        n.not_queued().unwrap();
        let bytes = fs::read(t.path()).unwrap();
        drop(n);
        let mut data: Data = serde_json::from_slice(&bytes).unwrap();
        alter(&mut data);
        fs::write(t.path(), serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(Navigator::open(&t.path(), epoch(), &[entry()]).is_err());
    }
    fn reseal(s: &mut Step) {
        s.id = request_id(&s.entry, &s.target, s.index, s.day).unwrap();
        s.checksum = checksum(s).unwrap();
    }
    #[test]
    fn forged_entry_recomputed_hash_still_refuses_current_plan_mismatch() {
        altered_reopen(|d| {
            let s = &mut d.steps[0];
            s.entry.id = "forged".into();
            reseal(s);
        });
    }
    #[test]
    fn forged_target_and_wrong_key_refuse_even_with_recomputed_hash() {
        altered_reopen(|d| {
            let s = &mut d.steps[0];
            s.target.slot = 1;
            reseal(s);
        });
        altered_reopen(|d| {
            let s = &mut d.steps[0];
            s.target.technology = "wrong".into();
            reseal(s);
        });
        altered_reopen(|d| {
            let s = &mut d.steps[0];
            s.entry.id = "".into();
            reseal(s);
        });
    }
    #[test]
    fn valid_range_day_hash_and_out_of_range_day_refuse() {
        altered_reopen(|d| d.steps[0].day += 1);
        altered_reopen(|d| {
            let s = &mut d.steps[0];
            s.day = 1;
            reseal(s);
        });
    }
    #[test]
    fn forged_hash_duplicate_index_and_nonregressing_day_refuse() {
        altered_reopen(|d| d.steps[0].id = "g".repeat(64));
        altered_reopen(|d| d.steps.push(d.steps[0].clone()));
        altered_reopen(|d| {
            let mut s = d.steps[0].clone();
            s.index = 2;
            s.day -= 1;
            reseal(&mut s);
            d.steps.push(s);
        });
    }
    #[test]
    fn terminal_flip_or_missing_unreached_verified_evidence_refuses() {
        altered_reopen(|d| d.steps[0].terminal = Some(true));
        altered_reopen(|d| {
            let s = &mut d.steps[0];
            s.terminal = Some(true);
            reseal(s);
        });
        altered_reopen(|d| {
            let s = &mut d.steps[0];
            s.terminal = Some(true);
            s.receipt = Some(receipt(s.id.clone(), s.target.clone(), state()).receipt);
            reseal(s);
        });
    }
    #[test]
    fn old_unbound_schema_and_terminal_rewrite_are_unavailable() {
        altered_reopen(|d| d.version = 1);
        let t = Temp::new();
        let mut n = Navigator::open(&t.path(), epoch(), &[entry()]).unwrap();
        n.begin(
            &entry(),
            next(&entry(), &state()).unwrap().unwrap(),
            1936 * 365,
        )
        .unwrap();
        n.not_queued().unwrap();
        assert!(n.not_queued().is_err());
    }
    #[test]
    fn failed_terminal_validation_keeps_process_quarantined() {
        let t = Temp::new();
        let mut n = Navigator::open(&t.path(), epoch(), &[entry()]).unwrap();
        n.begin(
            &entry(),
            next(&entry(), &state()).unwrap().unwrap(),
            1936 * 365,
        )
        .unwrap();
        n.data.steps[0].day = 1;
        assert!(n.not_queued().is_err());
        assert!(n.blocked());
        drop(n);
        assert!(Navigator::open(&t.path(), epoch(), &[entry()])
            .unwrap()
            .blocked());
    }
    #[test]
    fn local_navigation_refusal_blocks_only_its_entry_on_that_day() {
        let t = Temp::new();
        let a = entry();
        let mut b = a.clone();
        b.id = "research_2".into();
        b.intent = crate::daily_governor::Intent::Research {
            technology_id: "other_tech".into(),
            slot: 1,
        };
        let mut n = Navigator::open(&t.path(), epoch(), &[a.clone(), b.clone()]).unwrap();
        let day = 1936 * 365;
        n.begin(&a, next(&a, &state()).unwrap().unwrap(), day)
            .unwrap();
        n.not_queued().unwrap();
        assert!(n.refused_today(&a.id, day));
        assert!(!n.refused_today(&b.id, day));
        n.begin(&b, next(&b, &state()).unwrap().unwrap(), day)
            .unwrap();
        assert!(n.blocked());
    }
    #[test]
    fn pre_navigation_refusal_is_durable_per_entry_day_and_never_goal_input() {
        let t = Temp::new();
        let a = entry();
        let mut b = a.clone();
        b.id = "research_2".into();
        b.intent = crate::daily_governor::Intent::Research {
            technology_id: "other_tech".into(),
            slot: 1,
        };
        let entries = [a.clone(), b.clone()];
        let day = 1936 * 365;
        let mut n = Navigator::open(&t.path(), epoch(), &entries).unwrap();
        let error = Error::new(
            "research_precondition",
            "Actual bounded refusal\nno invented diagnosis",
        );
        n.refuse_preparation(&a, day, &error).unwrap();
        let saved = fs::read(t.path()).unwrap();
        n.refuse_preparation(
            &a,
            day,
            &Error::new("research_unavailable", "replacement reason"),
        )
        .unwrap();
        assert_eq!(fs::read(t.path()).unwrap(), saved);
        assert!(n.data.steps.is_empty());
        assert!(!n.blocked());
        drop(n);
        let n = Navigator::open(&t.path(), epoch(), &entries).unwrap();
        assert!(n.refused_today(&a.id, day));
        assert!(!n.refused_today(&b.id, day));
        assert!(!n.refused_today(&a.id, day + 1));
        assert_eq!(
            n.latest_refusal().unwrap()["reason"],
            "Actual bounded refusalno invented diagnosis"
        );
        let selected = entries
            .iter()
            .find(|e| !n.refused_today(&e.id, day))
            .unwrap();
        assert_eq!(selected.id, b.id);
    }
    #[test]
    fn unknown_error_and_forged_refusal_cannot_supply_backoff_history() {
        let t = Temp::new();
        let a = entry();
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&a)).unwrap();
        assert!(n
            .refuse_preparation(
                &a,
                1936 * 365,
                &Error::new("action_uncertain", "possible input")
            )
            .is_err());
        assert!(n.data.refusals.is_empty());
        n.refuse_preparation(
            &a,
            1936 * 365,
            &Error::new("research_precondition", "known no-input refusal"),
        )
        .unwrap();
        drop(n);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(t.path()).unwrap()).unwrap();
        value["refusals"][0]["day"] = serde_json::json!(1936 * 365 + 1);
        fs::write(t.path(), serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(Navigator::open(&t.path(), epoch(), &[a]).is_err());
    }
    #[test]
    fn reason_is_bounded_and_existing_proven_step_history_is_compatible() {
        assert!(bounded_reason(&"é".repeat(1000)).len() <= 160);
        let t = Temp::new();
        let a = entry();
        let mut n = Navigator::open(&t.path(), epoch(), std::slice::from_ref(&a)).unwrap();
        n.begin(&a, next(&a, &state()).unwrap().unwrap(), 1936 * 365)
            .unwrap();
        n.not_queued().unwrap();
        drop(n);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(t.path()).unwrap()).unwrap();
        value.as_object_mut().unwrap().remove("refusals");
        fs::write(t.path(), serde_json::to_vec(&value).unwrap()).unwrap();
        let n = Navigator::open(&t.path(), epoch(), &[a]).unwrap();
        assert_eq!(n.data.steps.len(), 1);
        assert!(n.data.refusals.is_empty());
    }
}
