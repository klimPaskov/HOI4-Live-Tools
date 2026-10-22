//! Owner-held, separately budgeted departure from the opposite ordinary panel.
//! Selection remains durable until the exact governor goal finishes. This helper
//! owns no worker, lease, transport, screenshots, cursor operations or pauses.
use super::*;
use crate::{
    construction_stage::FailureEvidence,
    daily_panel_navigation::{Destination, Navigator, Source},
    production_step::{
        PanelState, PanelTarget, State as ProductionState, Target as ProductionTarget,
    },
    research_stage::{OwnState, Stage, Target as ResearchTarget},
    research_ui::{self, UiContext},
};

pub(super) struct PanelPlan {
    pub navigation: Navigator,
    pub limit: Option<u8>,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Run {
    Unchanged,
    Deferred,
    Departed,
}
pub(super) fn permission(
    research_enabled: bool,
    production_enabled: bool,
    production_permission: bool,
    focus_permission: bool,
    has_research: bool,
    has_production: bool,
    limit: Option<u8>,
) -> Result<()> {
    if let Some(limit) = limit {
        if !(1..=16).contains(&limit)
            || !research_enabled
            || !production_enabled
            || !production_permission
            || !has_research
            || !has_production
            || focus_permission
        {
            return Err(Error::new("invalid_plan", "Panel navigation requires both enabled owned Research/production profiles, production_permission, max_panel_navigation_steps_per_day in 1..16 and no Focus opt-in"));
        }
    }
    Ok(())
}
fn scope_check(context: &ClockContext, scope: &core::Scope) -> Result<()> {
    if context.anchors != scope.anchors || context.player != scope.epoch.player {
        return Err(Error::new(
            "ownership_changed",
            "Panel navigation left the original daily campaign scope",
        ));
    }
    crate::calendar::decode(context.date_raw)?;
    Ok(())
}
pub(super) fn check_research(
    own: &OwnState,
    context: &UiContext,
    scope: &core::Scope,
) -> Result<()> {
    scope_check(&context.campaign, scope)?;
    if own.epoch != scope.epoch
        || context.anchors[..2] != [scope.own_country, scope.own_status]
        || context.slots.len() != own.researching.len()
    {
        return Err(Error::new(
            "ownership_changed",
            "Panel navigation own Research identity changed",
        ));
    }
    Ok(())
}
fn research_target(scope: &core::Scope) -> ResearchTarget {
    ResearchTarget {
        epoch: scope.epoch.clone(),
        stage: Stage::ClosePanel,
        technology: String::new(),
        folder_id: None,
        slot: 0,
    }
}
fn day_of(raw: i32) -> i64 {
    (i64::from(raw) - 43_800_000).div_euclid(24)
}
fn research_selection(mode: i32, active: bool) -> Result<bool> {
    if active {
        return Ok(false);
    }
    match mode {
        5 | 23 => Ok(true),
        6 => Err(Error::new(
            "invalid_panel_navigation",
            "An unselected Research tree cannot establish its original destination",
        )),
        _ => Ok(false),
    }
}
fn production_destination(
    process: &dyn Process,
    state: &mut ControlState,
    entry: &core::Entry,
    scope: &core::Scope,
    until: Instant,
) -> Result<(Destination, ClockContext)> {
    let (own, before) = production_plan::observed(process, state, entry, scope, until)?;
    production_destination_from_own(process, state, own, before, scope, until)
}
fn production_destination_from_own(
    process: &dyn Process,
    state: &mut ControlState,
    own: crate::production_step::FactoryState,
    before: ClockContext,
    scope: &core::Scope,
    until: Instant,
) -> Result<(Destination, ClockContext)> {
    let (panel_context, panel) = production::panel_observed(
        process,
        &mut state.guard,
        &ProductionTarget::Panel(PanelTarget::new(scope.epoch.clone())),
        false,
        until,
    )?;
    scope_check(&panel_context.campaign, scope)?;
    if panel_context.campaign.date_raw < before.date_raw {
        return Err(Error::new(
            "torn_read",
            "Production destination chronology regressed",
        ));
    }
    let ProductionState::Panel(panel) = panel else {
        unreachable!()
    };
    if panel.panel_identity[..3] != [scope.own_country, own.line.production, scope.anchors[1]] {
        return Err(Error::new(
            "ownership_changed",
            "Canonical production line and panel roots differ",
        ));
    }
    Ok((
        Destination::Production { state: own, panel },
        panel_context.campaign,
    ))
}
impl PanelPlan {
    pub(super) fn active(&self) -> Option<core::Entry> {
        self.navigation.active().cloned()
    }
    pub(super) fn blocked(&self) -> bool {
        self.navigation.blocked()
    }
    pub(super) fn summary(&self, day: i64) -> Value {
        let mut summary = self.navigation.summary();
        summary["permission"] = json!(self.limit.is_some());
        summary["budget"] = json!({"day":day,"used_steps":self.navigation.used_steps(day),"limit":self.limit,"historical":true});
        summary
    }
    #[allow(clippy::too_many_arguments)]
    fn preflight(
        &self,
        state: &mut ControlState,
        process: &dyn Process,
        governor: &mut core::Governor,
        token: &str,
        until: Instant,
        stop: &AtomicBool,
        day: i64,
        date: i32,
        needs_departure: bool,
    ) -> Result<bool> {
        if stop.load(Ordering::Acquire) || Instant::now() >= until || day_of(date) != day {
            return Ok(false);
        }
        state.authority.authorize(token, &state.guard.epoch)?;
        governor.preparation_check()?;
        if self.blocked() || unresolved(state) {
            return Err(Error::new(
                "action_uncertain",
                "Shared input or panel departure is unresolved",
            ));
        }
        // Selection persistence can cross a game-day boundary. Re-prove the
        // original budget day immediately before a departure Intent is reserved.
        let current = game::clock_context(process)?;
        state.guard.accept_context(process, &current)?;
        scope_check(&current, &governor.plan().scope)?;
        if current.date_raw < date {
            return Err(Error::new("torn_read", "Panel departure clock regressed"));
        }
        if day_of(current.date_raw) != day
            || stop.load(Ordering::Acquire)
            || Instant::now() >= until
        {
            return Ok(false);
        }
        Ok(self
            .limit
            .is_some_and(|n| !needs_departure || self.navigation.used_steps(day) < usize::from(n))
            && governor.budget_available(day))
    }
    fn select(
        &mut self,
        governor: &mut core::Governor,
        entry: &core::Entry,
        destination: &Destination,
        date: i32,
    ) -> Result<()> {
        if let Some(active) = self.navigation.active() {
            if active != entry {
                return Err(Error::new(
                    "invalid_panel_navigation",
                    "Existing selected destination has priority",
                ));
            }
            self.navigation.verify_current(destination, date)
        } else {
            self.navigation
                .select(governor, entry, destination.clone(), date)
                .map(|_| ())
        }
    }
    /// Only mode2 causes a departure. Idle23/open Research5 also select the
    /// destination before preparation, retaining it without spending a step.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_to_research(
        &mut self,
        governor: &mut core::Governor,
        state: &mut ControlState,
        process: &dyn Process,
        gate: &RwLock<bool>,
        token: &str,
        until: Instant,
        stop: &AtomicBool,
        day: i64,
        entry: &core::Entry,
    ) -> Result<Run> {
        if self.limit.is_none() {
            return Ok(Run::Unchanged);
        }
        let core::Intent::Research {
            technology_id,
            slot,
        } = &entry.intent
        else {
            return Err(Error::new(
                "invalid_plan",
                "Research destination entry required",
            ));
        };
        if stop.load(Ordering::Acquire) || Instant::now() >= until {
            return Ok(Run::Deferred);
        }
        let scope = governor.plan().scope;
        // An exact already-assigned goal is a governor no-op, requiring no
        // opposite-panel departure or retained empty-slot destination proof.
        let (canonical, own_context) = crate::research::owned_snapshot(process)?;
        state.guard.accept_context(process, &own_context.campaign)?;
        scope_check(&own_context.campaign, &scope)?;
        if own_context.own != [scope.own_country, scope.own_status] {
            return Err(Error::new(
                "ownership_changed",
                "Research no-op probe left the original own status",
            ));
        }
        let assignment = canonical["slots"]
            .as_array()
            .and_then(|slots| slots.get(*slot as usize))
            .and_then(|slot| slot.get("researching"))
            .ok_or_else(|| {
                Error::new(
                    "research_unavailable",
                    "Exact own Research destination slot unavailable",
                )
            })?;
        if assignment.as_str() == Some(technology_id.as_str()) {
            return Ok(Run::Unchanged);
        }
        // Routing from an open Research panel or idle does not require a
        // materialized production root. The mode2 departure gets its exact
        // shown production identity from the combined retained reader below.
        let (queue_state, queue_context) = research::observed_state(process, &mut state.guard)?;
        check_research(&queue_state, &queue_context, &scope)?;
        let mode = queue_state.active_mode.ok_or_else(|| {
            Error::new(
                "research_precondition",
                "Known own managed view mode required",
            )
        })?;
        if mode != 2 {
            if research_selection(mode, self.navigation.active().is_some())? {
                let (own, context) = research::close_state(
                    process,
                    &mut state.guard,
                    research_ui::closing::observe(process)?,
                )?;
                check_research(&own, &context, &scope)?;
                if !matches!(own.active_mode, Some(5 | 23))
                    || own.tree_visible
                    || !own.visible_info.is_empty()
                {
                    return Err(Error::new("research_precondition", "Settled open Research or idle views required before original destination selection"));
                }
                let date = context.campaign.date_raw;
                if !self.preflight(
                    state, process, governor, token, until, stop, day, date, false,
                )? {
                    return Ok(Run::Deferred);
                }
                let destination = Destination::Research { state: own };
                drop(input_permit(gate)?);
                self.select(governor, entry, &destination, date)?;
                state.guard.selected_target = self.navigation.original()?;
                if !self.preflight(
                    state, process, governor, token, until, stop, day, date, false,
                )? {
                    return Ok(Run::Deferred);
                }
            }
            return Ok(Run::Unchanged);
        }
        let observation = research_ui::mixed_destination::observe(process, *slot)?;
        let source_identity = observation.production_identity;
        let (own, context) =
            research::close_state(process, &mut state.guard, observation.research)?;
        check_research(&own, &context, &scope)?;
        let date = context.campaign.date_raw;
        let destination = Destination::Research { state: own };
        let source = Source::Production {
            state: PanelState {
                epoch: scope.epoch.clone(),
                panel_identity: source_identity.to_vec(),
                managed_mode: 2,
                panel_shown: true,
            },
        };
        let Source::Production { state: expected } = &source else {
            unreachable!()
        };
        let expected = ProductionState::Panel(expected.clone());
        if !self.preflight(
            state, process, governor, token, until, stop, day, date, true,
        )? {
            return Ok(Run::Deferred);
        }
        drop(input_permit(gate)?);
        self.select(governor, entry, &destination, date)?;
        state.guard.selected_target = self.navigation.original()?;
        if !self.preflight(
            state, process, governor, token, until, stop, day, date, true,
        )? {
            return Ok(Run::Deferred);
        }
        let id = self
            .navigation
            .begin_departure(source, &destination, date)?;
        let target = ProductionTarget::Panel(PanelTarget::closed(scope.epoch.clone()));
        let execution = match production::execute_bound(
            state,
            process,
            gate,
            token,
            &id,
            target.clone(),
            until,
            Some(&expected),
        ) {
            Ok(execution) => execution,
            Err(error) => {
                let evidence = production_plan::failure(state, &id, &target, &error);
                self.navigation
                    .failed_departure(&id, &error, evidence, token)?;
                return Err(error);
            }
        };
        let verified = (|| {
            let after = research_ui::mixed_destination::observe(process, *slot)?;
            if after.production_identity != source_identity {
                return Err(Error::new(
                    "ownership_changed",
                    "Original production departure identity changed after close",
                ));
            }
            let (own, context) = research::close_state(process, &mut state.guard, after.research)?;
            check_research(&own, &context, &scope)?;
            if Instant::now() >= until {
                return Err(Error::new(
                    "daily_deadline",
                    "Panel departure readback exceeded original deadline",
                ));
            }
            self.navigation.verified_production(
                &id,
                &execution,
                Destination::Research { state: own },
                context.campaign.date_raw,
            )
        })();
        if let Err(error) = verified {
            if self.navigation.blocked() {
                self.navigation
                    .failed_departure(&id, &error, FailureEvidence::Unknown, token)?;
            }
            return Err(error);
        }
        Ok(Run::Departed)
    }
    /// Closing Research is preparation only; production's own opening and
    /// one-factory progression keep their independent persisted step budget.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_to_production(
        &mut self,
        governor: &mut core::Governor,
        state: &mut ControlState,
        process: &dyn Process,
        gate: &RwLock<bool>,
        token: &str,
        until: Instant,
        stop: &AtomicBool,
        day: i64,
        entry: &core::Entry,
    ) -> Result<Run> {
        if self.limit.is_none() {
            return Ok(Run::Unchanged);
        }
        if stop.load(Ordering::Acquire) || Instant::now() >= until {
            return Ok(Run::Deferred);
        }
        let scope = governor.plan().scope;
        // Matching-count finalization needs no panel observation or departure.
        let (own, context) = production_plan::observed(process, state, entry, &scope, until)?;
        if own.requested == production_plan::goal(entry, &scope)?.factory()?.factories {
            return Ok(Run::Unchanged);
        }
        let (destination, context) =
            production_destination_from_own(process, state, own, context, &scope, until)?;
        let Destination::Production { panel, .. } = &destination else {
            unreachable!()
        };
        if panel.managed_mode != 5 {
            return Ok(Run::Unchanged);
        }
        let target = research_target(&scope);
        let (source, source_context) =
            research::observed_target_state(process, &mut state.guard, Some(&target))?;
        check_research(&source, &source_context, &scope)?;
        let date = source_context.campaign.date_raw;
        if date < context.date_raw {
            return Err(Error::new(
                "torn_read",
                "Research departure chronology regressed",
            ));
        }
        if !self.preflight(
            state, process, governor, token, until, stop, day, date, true,
        )? {
            return Ok(Run::Deferred);
        }
        drop(input_permit(gate)?);
        self.select(governor, entry, &destination, date)?;
        state.guard.selected_target = self.navigation.original()?;
        if !self.preflight(
            state, process, governor, token, until, stop, day, date, true,
        )? {
            return Ok(Run::Deferred);
        }
        let id = self.navigation.begin_departure(
            Source::Research {
                state: source.clone(),
            },
            &destination,
            date,
        )?;
        let execution = match research::execute_bound(
            state,
            process,
            gate,
            token,
            &id,
            target.clone(),
            until,
            Some(&source),
        ) {
            Ok((execution, _)) => execution,
            Err(error) => {
                let evidence = match research::failure_evidence(state, &id, &target, &error) {
                    crate::research_stage::FailureEvidence::BeforeIntent => {
                        FailureEvidence::BeforeIntent
                    }
                    crate::research_stage::FailureEvidence::ConfirmedNotQueued => {
                        FailureEvidence::ConfirmedNotQueued
                    }
                    crate::research_stage::FailureEvidence::Unknown => FailureEvidence::Unknown,
                };
                self.navigation
                    .failed_departure(&id, &error, evidence, token)?;
                return Err(error);
            }
        };
        let verified = (|| {
            let (after, context) = production_destination(process, state, entry, &scope, until)?;
            self.navigation
                .verified_research(&id, &execution, after, context.date_raw)
        })();
        if let Err(error) = verified {
            if self.navigation.blocked() {
                self.navigation
                    .failed_departure(&id, &error, FailureEvidence::Unknown, token)?;
            }
            return Err(error);
        }
        Ok(Run::Departed)
    }
    /// Read-only identity joins precede the existing adapter's independent fresh
    /// readiness and final before-queue checks. They never authorize transport.
    pub(super) fn verify_selected(
        &mut self,
        governor: &mut core::Governor,
        state: &mut ControlState,
        process: &dyn Process,
        until: Instant,
        entry: &core::Entry,
    ) -> Result<()> {
        state.guard.selected_target = self.navigation.original()?;
        let Some(selected) = self.active() else {
            return Ok(());
        };
        if selected != *entry {
            return Err(Error::new(
                "invalid_panel_navigation",
                "Existing selected destination has priority",
            ));
        }
        governor.preparation_check()?;
        let scope = governor.plan().scope;
        match entry.intent {
            core::Intent::Research { .. } => {
                let identity = research_ui::mixed_destination::observe_identity(process)?;
                state
                    .guard
                    .accept_context(process, &identity.context.campaign)?;
                let (own, context) = research::observed_state(process, &mut state.guard)?;
                check_research(&own, &context, &scope)?;
                if identity.context.anchors != context.anchors
                    || identity.context.slots != context.slots
                    || identity.context.campaign.anchors != context.campaign.anchors
                    || identity.context.campaign.player != context.campaign.player
                    || context.campaign.date_raw < identity.context.campaign.date_raw
                {
                    return Err(Error::new(
                        "torn_read",
                        "Retained Research identity and current own UI differ",
                    ));
                }
                if Instant::now() >= until {
                    return Err(Error::new(
                        "daily_deadline",
                        "Research identity verification exceeded original deadline",
                    ));
                }
                self.navigation.verify_research_identity(
                    &own.epoch,
                    &identity.context.anchors,
                    &identity.context.slots,
                    &identity.roots,
                    identity.facilities_parent,
                    context.campaign.date_raw,
                )
            }
            _ => {
                let (current, context) =
                    production_destination(process, state, entry, &scope, until)?;
                self.navigation.verify_current(&current, context.date_raw)
            }
        }
    }
    pub(super) fn finish_selected(
        &mut self,
        governor: &mut core::Governor,
        state: &mut ControlState,
        process: &dyn Process,
        until: Instant,
    ) -> Result<bool> {
        state.guard.selected_target = self.navigation.original()?;
        let Some(entry) = self.active() else {
            return Ok(false);
        };
        governor.preparation_check()?;
        if !matches!(
            governor.entry_state(&entry.id),
            Some(core::EntryState::Complete { .. })
        ) {
            return Ok(false);
        }
        let scope = governor.plan().scope;
        let (current, date) = match entry.intent {
            core::Intent::Research { .. } => {
                let (own, context) = research::close_state(
                    process,
                    &mut state.guard,
                    research_ui::closing::observe(process)?,
                )?;
                check_research(&own, &context, &scope)?;
                (
                    Destination::Research { state: own },
                    context.campaign.date_raw,
                )
            }
            _ => {
                let (current, context) =
                    production_destination(process, state, &entry, &scope, until)?;
                (current, context.date_raw)
            }
        };
        if Instant::now() >= until {
            return Err(Error::new(
                "daily_deadline",
                "Selected destination completion exceeded original deadline",
            ));
        }
        self.navigation.finish(governor, &current, date)?;
        state.guard.selected_target = None;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn research_selection_retains_open_or_idle_goals_and_refuses_unbound_tree() {
        for mode in 0..=23 {
            assert!(!research_selection(mode, true).unwrap());
            match mode {
                5 | 23 => assert!(research_selection(mode, false).unwrap()),
                6 => assert!(research_selection(mode, false).is_err()),
                _ => assert!(!research_selection(mode, false).unwrap()),
            }
        }
    }

    #[test]
    fn panel_opt_in_requires_both_domains_profiles_and_no_focus_combination() {
        for mask in 0..64 {
            let flags: Vec<_> = (0..6).map(|bit| mask & (1 << bit) != 0).collect();
            let call = |limit| {
                permission(
                    flags[0], flags[1], flags[2], flags[3], flags[4], flags[5], limit,
                )
            };
            assert!(call(None).is_ok(), "legacy permissions mask {mask}");
            for limit in [1, 16] {
                assert_eq!(
                    call(Some(limit)).is_ok(),
                    mask == 55,
                    "mixed permissions mask {mask}, limit {limit}"
                );
            }
        }
        for limit in [0, 17, 255] {
            assert!(permission(true, true, true, false, true, true, Some(limit)).is_err());
        }
    }
}
