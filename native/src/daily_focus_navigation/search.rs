//! A single durable search chain per opted-in Focus entry. No input lives here.
use super::{invalid, Entry, Intent, Navigator, OwnBinding, OwnState, Result, Stage, Step, Target};
use crate::focus_stage::{ActiveFocus, SearchMatches, Status};

fn search_stage(stage: Stage) -> bool {
    matches!(
        stage,
        Stage::SearchField | Stage::SearchText | Stage::SearchNext
    )
}
fn chain_stage(stage: Stage) -> bool {
    search_stage(stage) || stage == Stage::View
}
impl OwnBinding {
    pub(crate) fn without_search(&self) -> Self {
        let mut value = self.clone();
        value.search = None;
        value
    }
}
pub(super) fn validate_target(entry: &Entry, target: &Target, binding: &OwnBinding) -> Result<()> {
    let hinted = entry.focus_search.as_deref();
    if target.stage == Stage::SearchQuery
        || (search_stage(target.stage) && hinted.is_none())
        || (search_stage(target.stage) != binding.search.is_some())
        || (target.stage.uses_query() && target.query.as_deref() != hinted)
        || (!target.stage.uses_query() && target.query.is_some())
    {
        return Err(invalid());
    }
    Ok(())
}
fn verified(step: &Step) -> Result<&crate::focus_stage::Receipt> {
    step.receipt
        .as_ref()
        .filter(|r| step.failure.is_none() && r.status == Status::Verified)
        .ok_or_else(invalid)
}
fn matches(step: &Step) -> Result<&SearchMatches> {
    verified(step)?
        .observed
        .search_matches()
        .ok_or_else(invalid)
}
/// Validate both new intent and reopened history against the preceding chain.
/// A failed or unresolved search stage can never start a second chain.
pub(super) fn validate_extension(prefix: &[Step], step: &Step) -> Result<()> {
    if step.entry.focus_search.is_none() || !chain_stage(step.target.stage) {
        return Ok(());
    }
    let prior: Vec<_> = prefix
        .iter()
        .filter(|s| s.entry == step.entry && chain_stage(s.target.stage))
        .collect();
    let Intent::Focus { focus_id } = &step.entry.intent else {
        return Err(invalid());
    };
    if let Some(receipt) = &step.receipt {
        if receipt.observed.active_focus != ActiveFocus::Idle {
            return Err(invalid());
        }
    }
    match step.target.stage {
        Stage::SearchField if prior.is_empty() => Ok(()),
        Stage::SearchText if prior.len() == 1 && prior[0].target.stage == Stage::SearchField => {
            verified(prior[0])?;
            if prior[0].binding != step.binding {
                return Err(invalid());
            }
            Ok(())
        }
        Stage::SearchNext | Stage::View => {
            let last = prior.last().ok_or_else(invalid)?;
            if !matches!(last.target.stage, Stage::SearchText | Stage::SearchNext) {
                return Err(invalid());
            }
            let before = matches(last)?;
            if step.binding.without_search() != last.binding.without_search() {
                return Err(invalid());
            }
            if step.target.stage == Stage::View {
                if before.selected != *focus_id {
                    return Err(invalid());
                }
            } else {
                let tabs = prior
                    .iter()
                    .filter(|s| s.target.stage == Stage::SearchNext)
                    .count();
                if step.binding != last.binding
                    || before.selected == *focus_id
                    || before.next_key() != Some(step.target.focus_id.as_str())
                    || tabs >= before.ordered.len().saturating_sub(1)
                {
                    return Err(invalid());
                }
                if let Some(receipt) = &step.receipt {
                    let after = receipt.observed.search_matches().ok_or_else(invalid)?;
                    if before.ordered != after.ordered || after.selected != step.target.focus_id {
                        return Err(invalid());
                    }
                }
            }
            Ok(())
        }
        _ => Err(invalid()),
    }
}
impl Navigator {
    /// Fresh own search state chooses one stage; history supplies bounds only.
    /// The caller still checks epoch/day/lease and executes the ordinary adapter.
    pub(crate) fn search_target(
        &self,
        entry: &Entry,
        own: &OwnState,
        binding: &OwnBinding,
    ) -> Result<Target> {
        entry.validate()?;
        let Intent::Focus { focus_id } = &entry.intent else {
            return Err(invalid());
        };
        let query = entry.focus_search.as_ref().ok_or_else(invalid)?;
        let mut target = Target {
            epoch: own.epoch.clone(),
            stage: Stage::View,
            focus_id: focus_id.clone(),
            query: None,
        };
        own.validate(&target)?;
        binding.validate()?;
        if self.blocked() || own.active_focus != ActiveFocus::Idle || binding.search.is_none() {
            return Err(invalid());
        }
        let control = own.search_control.as_ref().ok_or_else(invalid)?;
        let prior: Vec<_> = self
            .data
            .steps
            .iter()
            .filter(|s| s.entry == *entry && chain_stage(s.target.stage))
            .collect();
        match prior.last() {
            None => target.stage = Stage::SearchField,
            Some(last) if last.target.stage == Stage::SearchField => {
                verified(last)?;
                if &last.binding != binding {
                    return Err(invalid());
                }
                target.stage = Stage::SearchText;
                target.query = Some(query.clone());
            }
            Some(last) if matches!(last.target.stage, Stage::SearchText | Stage::SearchNext) => {
                let previous = matches(last)?;
                if &last.binding != binding
                    || control.matches.as_ref() != Some(previous)
                    || control.query_text.as_ref() != Some(query)
                    || !control.keyboard_focus_verified
                    || !control.text_receiver_verified
                    || !control.enabled
                    || !control.locally_ready
                {
                    return Err(invalid());
                }
                if previous.selected != *focus_id {
                    let tabs = prior
                        .iter()
                        .filter(|s| s.target.stage == Stage::SearchNext)
                        .count();
                    if tabs >= previous.ordered.len().saturating_sub(1) {
                        return Err(invalid());
                    }
                    target.stage = Stage::SearchNext;
                    target.query = Some(query.clone());
                    target.focus_id = previous.next_key().ok_or_else(invalid)?.into();
                }
            }
            _ => return Err(invalid()),
        }
        // The canonical producer joined the assignment target. A later adapter
        // independently joins an intermediate next key before any intent/input.
        own.preflight(&target)?;
        Ok(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::focus_stage::{
        Delivery, Execution, FailureEvidence, LocalRoot, Receipt, SearchControl,
    };
    use std::{
        os::unix::fs::PermissionsExt,
        sync::atomic::{AtomicU64, Ordering},
    };
    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "generated-focus-search-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&path).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self(path)
        }
        fn path(&self) -> std::path::PathBuf {
            self.0.join("navigation.json")
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn entry() -> Entry {
        Entry {
            id: "search-head".into(),
            intent: Intent::Focus {
                focus_id: "GER_rhineland".into(),
            },
            navigation: None,
            focus_search: Some("rhineland".into()),
        }
    }
    fn binding() -> OwnBinding {
        OwnBinding {
            focus: [0x1000, 0x2000, 0x3000, 0x4000, 0x5000, 0x6000, 0x7000],
            politics: [0x1000, 0x6000, 0x5000, 0x8000, 0x4000, 0x9000],
            slots: vec![0xa000],
            search: Some([0xb000, 0xc000, 0xd000]),
        }
    }
    fn own() -> OwnState {
        let mut own = super::super::tests::state();
        own.managed_mode = Some(13);
        own.focus_root = LocalRoot::Ready;
        own.research_panel_root = LocalRoot::Hidden;
        own.research_tree_root = LocalRoot::Hidden;
        own.search_control = Some(SearchControl {
            locally_ready: true,
            empty_query: true,
            enabled: true,
            keyboard_focus_verified: false,
            text_receiver_verified: false,
            query_text: Some("".into()),
            requested_selection_verified: Some(false),
            matches: None,
        });
        own
    }
    fn open(s: &Scratch, e: &Entry) -> Navigator {
        Navigator::open(&s.path(), own().epoch, std::slice::from_ref(e)).unwrap()
    }
    fn advance(n: &mut Navigator, e: &Entry, own: &mut OwnState, order: &[&str]) -> Stage {
        let mut bound = binding();
        let target = n.search_target(e, own, &bound).unwrap();
        let stage = target.stage;
        if stage == Stage::View {
            bound = bound.without_search();
        }
        let id = n
            .begin(e.clone(), target.clone(), 1936 * 365, 1, bound.clone())
            .unwrap();
        match stage {
            Stage::SearchField => {
                let c = own.search_control.as_mut().unwrap();
                c.keyboard_focus_verified = true;
                c.text_receiver_verified = true;
            }
            Stage::SearchText => {
                let c = own.search_control.as_mut().unwrap();
                c.empty_query = false;
                c.query_text = e.focus_search.clone();
                c.matches = Some(SearchMatches {
                    ordered: order.iter().map(|s| s.to_string()).collect(),
                    selected: order[0].into(),
                });
            }
            Stage::SearchNext => {
                own.search_control
                    .as_mut()
                    .unwrap()
                    .matches
                    .as_mut()
                    .unwrap()
                    .selected = target.focus_id.clone()
            }
            Stage::View => {
                own.selected_focus = Some(target.focus_id.clone());
                own.requested_detail_ready = Some(true);
                own.own_detail_counts = Some([1, 0]);
                own.search_control = None;
            }
            _ => panic!("unexpected preparation"),
        }
        own.canonical_focus_id = Some(target.focus_id.clone());
        if let Some(c) = &mut own.search_control {
            c.requested_selection_verified = Some(
                c.matches
                    .as_ref()
                    .is_some_and(|m| m.selected == target.focus_id),
            );
        }
        let execution = Execution {
            receipt: Receipt {
                version: 1,
                request_id: id,
                target,
                status: Status::Verified,
                delivery: Some(Delivery::ServerConfirmed),
                observed: own.clone(),
            },
            historical: false,
            current_state_verified: true,
            event_causality_verified: false,
        };
        n.verified(&execution, &bound).unwrap();
        own.canonical_focus_id = Some("GER_rhineland".into());
        if let Some(c) = &mut own.search_control {
            c.requested_selection_verified = Some(
                c.matches
                    .as_ref()
                    .is_some_and(|m| m.selected == "GER_rhineland"),
            );
        }
        stage
    }
    const ORDER: &[&str] = &["GER_other", "GER_middle", "GER_rhineland"];
    #[test]
    fn receipt_binding_uses_only_the_exact_current_observation_and_never_history() {
        let mut state = own();
        state
            .search_control
            .as_mut()
            .unwrap()
            .keyboard_focus_verified = true;
        state
            .search_control
            .as_mut()
            .unwrap()
            .text_receiver_verified = true;
        let execution = Execution {
            receipt: Receipt {
                version: 1,
                request_id: "generated".into(),
                target: Target {
                    epoch: state.epoch.clone(),
                    stage: Stage::SearchField,
                    focus_id: "GER_rhineland".into(),
                    query: None,
                },
                status: Status::Verified,
                delivery: Some(Delivery::ServerConfirmed),
                observed: state.clone(),
            },
            historical: false,
            current_state_verified: true,
            event_causality_verified: false,
        };
        let observation = (state.clone(), binding());
        assert_eq!(
            super::super::receipt_binding(&execution, Some(&observation)).unwrap(),
            Some(binding())
        );
        assert!(super::super::receipt_binding(&execution, None).is_err());
        state
            .search_control
            .as_mut()
            .unwrap()
            .text_receiver_verified = false;
        assert!(super::super::receipt_binding(&execution, Some(&(state, binding()))).is_err());
        let historical = Execution {
            historical: true,
            current_state_verified: false,
            ..execution
        };
        assert_eq!(
            super::super::receipt_binding(&historical, Some(&observation)).unwrap(),
            None
        );
        assert_eq!(
            super::super::receipt_binding(&historical, None).unwrap(),
            None
        );
    }

    #[test]
    fn complete_chain_survives_every_restart_and_does_not_restart_after_view() {
        let s = Scratch::new();
        let e = entry();
        let mut own = own();
        for expected in [
            Stage::SearchField,
            Stage::SearchText,
            Stage::SearchNext,
            Stage::SearchNext,
            Stage::View,
        ] {
            let mut n = open(&s, &e);
            assert_eq!(advance(&mut n, &e, &mut own, ORDER), expected);
        }
        let n = open(&s, &e);
        assert_eq!(n.data.steps.len(), 5);
        assert!(n.search_target(&e, &own, &binding()).is_err());
        assert_eq!(own.selected_focus.as_deref(), Some("GER_rhineland"));
        assert_eq!(own.active_focus, ActiveFocus::Idle);
    }
    #[test]
    fn current_order_selection_query_and_binding_must_match_the_verified_chain() {
        let s = Scratch::new();
        let e = entry();
        let mut own = own();
        let mut n = open(&s, &e);
        advance(&mut n, &e, &mut own, ORDER);
        advance(&mut n, &e, &mut own, ORDER);
        let bytes = std::fs::read(s.path()).unwrap();
        for kind in 0..5 {
            let mut changed = own.clone();
            let mut bound = binding();
            let c = changed.search_control.as_mut().unwrap();
            match kind {
                0 => c.matches.as_mut().unwrap().ordered.swap(0, 1),
                1 => c.matches.as_mut().unwrap().selected = "GER_middle".into(),
                2 => c.query_text = Some("different".into()),
                3 => bound.search = Some([0xe000, 0xc000, 0xd000]),
                _ => c.text_receiver_verified = false,
            }
            assert!(n.search_target(&e, &changed, &bound).is_err());
        }
        assert_eq!(std::fs::read(s.path()).unwrap(), bytes);
        let mut skipped = n.search_target(&e, &own, &binding()).unwrap();
        skipped.focus_id = "GER_rhineland".into();
        assert!(n.begin(e, skipped, 1936 * 365, 1, binding()).is_err());
        assert_eq!(std::fs::read(s.path()).unwrap(), bytes);
    }
    #[test]
    fn unresolved_and_failed_search_never_retype_even_after_reopen() {
        for failure in [false, true] {
            let s = Scratch::new();
            let e = entry();
            let own = own();
            let mut n = open(&s, &e);
            let target = n.search_target(&e, &own, &binding()).unwrap();
            n.begin(e.clone(), target, 1936 * 365, 1, binding())
                .unwrap();
            if failure {
                n.failed(
                    &crate::error::Error::new("torn_read", "generated"),
                    FailureEvidence::BeforeIntent,
                )
                .unwrap();
            }
            drop(n);
            let n = open(&s, &e);
            assert!(n.search_target(&e, &own, &binding()).is_err());
            assert_eq!(n.data.steps.len(), 1);
        }
    }
    #[test]
    fn reopened_individually_valid_receipt_cannot_skip_a_canonical_match() {
        let s = Scratch::new();
        let e = entry();
        let mut own = own();
        let mut n = open(&s, &e);
        for _ in 0..3 {
            advance(&mut n, &e, &mut own, ORDER);
        }
        let step = n.data.steps.last_mut().unwrap();
        step.target.focus_id = "GER_rhineland".into();
        step.id = super::super::id(step).unwrap();
        let r = step.receipt.as_mut().unwrap();
        r.request_id = step.id.clone();
        r.target = step.target.clone();
        r.observed.canonical_focus_id = Some("GER_rhineland".into());
        r.observed
            .search_control
            .as_mut()
            .unwrap()
            .matches
            .as_mut()
            .unwrap()
            .selected = "GER_rhineland".into();
        step.checksum = super::super::checksum(step).unwrap();
        // Recompute normal individual checksums; only ordered chain validation catches the skip.
        super::super::validate_step(step, &own.epoch, std::slice::from_ref(&e), 2).unwrap();
        let bytes = serde_json::to_vec(&n.data).unwrap();
        drop(n);
        std::fs::write(s.path(), bytes).unwrap();
        assert!(Navigator::open(&s.path(), own.epoch, std::slice::from_ref(&e)).is_err());
    }
    #[test]
    fn already_selected_query_needs_no_tab_and_completed_selection_cannot_cycle() {
        let s = Scratch::new();
        let e = entry();
        let mut own = own();
        let mut n = open(&s, &e);
        advance(&mut n, &e, &mut own, &["GER_rhineland", "GER_other"]);
        advance(&mut n, &e, &mut own, &["GER_rhineland", "GER_other"]);
        let target = n.search_target(&e, &own, &binding()).unwrap();
        assert_eq!(target.stage, Stage::View);
        let cycle = Target {
            stage: Stage::SearchNext,
            focus_id: "GER_other".into(),
            query: e.focus_search.clone(),
            ..target
        };
        assert!(n.begin(e, cycle, 1936 * 365, 1, binding()).is_err());
    }
}
