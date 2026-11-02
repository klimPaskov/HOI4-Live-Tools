//! Typed departure reservations. Each attempt consumes its original day's
//! budget before the ordinary adapter runs; no automatic replay or settlement.
use super::*;
use crate::{
    construction_stage::{Diagnostic, FailureEvidence},
    production_step as production, research_stage as research,
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "domain", deny_unknown_fields)]
#[allow(clippy::large_enum_variant)] // Two private typed snapshot alternatives; no per-loop allocation.
pub(crate) enum Source {
    Production { state: PanelState },
    Research { state: OwnState },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "domain", deny_unknown_fields)]
enum Proof {
    Production {
        request_id: String,
        target: production::PanelTarget,
        before: PanelState,
        after: PanelState,
    },
    Research {
        receipt: research::Receipt,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "status", deny_unknown_fields)]
#[allow(clippy::large_enum_variant)] // Bounded durable record; preserve the typed destination.
enum Status {
    Intent {
        failure: Option<Diagnostic>,
    },
    NotQueued {
        cause: Diagnostic,
        evidence: FailureEvidence,
    },
    Verified {
        proof: Box<Proof>,
        destination: Destination,
        date: i32,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Step {
    id: String,
    selection: usize,
    source: Source,
    date: i32,
    day: i64,
    status: Status,
}
fn research_target(scope: &Scope) -> research::Target {
    research::Target {
        epoch: scope.epoch.clone(),
        stage: research::Stage::ClosePanel,
        technology: String::new(),
        folder_id: None,
        slot: 0,
    }
}
impl Source {
    fn validate(&self, selection: &Selection, scope: &Scope) -> Result<()> {
        match (self, &selection.original) {
            (Self::Production { state }, Destination::Research { state: destination }) => {
                let target =
                    production::Target::Panel(production::PanelTarget::closed(scope.epoch.clone()));
                production::State::Panel(state.clone()).validate(&target)?;
                let b = destination
                    .panel_binding
                    .as_ref()
                    .ok_or_else(|| invalid("Destination roots unavailable"))?;
                if state.managed_mode != 2
                    || !state.panel_shown
                    || destination.active_mode != Some(2)
                    || destination.panel_visible != Some(false)
                    || destination.tree_visible
                    || !destination.visible_info.is_empty()
                    || state.panel_identity[0] != scope.own_country
                    || state.panel_identity[2] != scope.anchors[1]
                    || state.panel_identity[3..5] != b.anchors[2..4]
                {
                    return Err(invalid("Exact shared shown production departure required"));
                }
            }
            (Self::Research { state }, Destination::Production { panel, .. }) => {
                let target = research_target(scope);
                state.validate(&target)?;
                state.preflight(&target)?;
                let b = state
                    .panel_binding
                    .as_ref()
                    .ok_or_else(|| invalid("Source Research roots unavailable"))?;
                if panel.managed_mode != 5
                    || panel.panel_shown
                    || b.anchors[..2] != [scope.own_country, scope.own_status]
                    || panel.panel_identity[3..5] != b.anchors[2..4]
                {
                    return Err(invalid("Exact shared shown Research departure required"));
                }
            }
            _ => return Err(invalid("Departure must leave the opposite domain")),
        }
        Ok(())
    }
    fn closed(&self) -> Self {
        match self {
            Self::Production { state } => Self::Production {
                state: PanelState {
                    managed_mode: 23,
                    panel_shown: false,
                    ..state.clone()
                },
            },
            Self::Research { state } => Self::Research {
                state: OwnState {
                    active_mode: Some(23),
                    panel_visible: Some(false),
                    ..state.clone()
                },
            },
        }
    }
}
fn destination_after_close(original: &Destination) -> Destination {
    match original {
        Destination::Research { state } => Destination::Research {
            state: OwnState {
                active_mode: Some(23),
                ..state.clone()
            },
        },
        Destination::Production { state, panel } => Destination::Production {
            state: state.clone(),
            panel: PanelState {
                managed_mode: 23,
                ..panel.clone()
            },
        },
    }
}
impl Step {
    fn id(&self, data: &Data, ordinal: usize) -> Result<String> {
        let selection = data
            .selections
            .get(self.selection)
            .ok_or_else(|| invalid("Departure selection absent"))?;
        digest(&(
            "panel-departure-v1",
            &selection.id,
            ordinal,
            &self.source,
            self.date,
            self.day,
        ))
    }
    fn verify(
        &self,
        data: &Data,
        proof: &Proof,
        destination: &Destination,
        date: i32,
    ) -> Result<()> {
        let selection = &data.selections[self.selection];
        let scope = &data.policies[selection.policy].plan.scope;
        if day(date)? < self.day
            || date < self.date
            || *destination != destination_after_close(&selection.original)
        {
            return Err(invalid(
                "Departure changed original destination or chronology",
            ));
        }
        match (proof, &self.source) {
            (
                Proof::Production {
                    request_id,
                    target,
                    before,
                    after,
                },
                Source::Production { state },
            ) => {
                if request_id != &self.id
                    || target != &production::PanelTarget::closed(scope.epoch.clone())
                    || before != state
                    || (Source::Production {
                        state: after.clone(),
                    }) != self.source.closed()
                {
                    return Err(invalid(
                        "Exact fresh original production close receipt required",
                    ));
                }
            }
            (Proof::Research { receipt }, Source::Research { state }) => {
                if receipt.version != 1
                    || receipt.request_id != self.id
                    || receipt.target != research_target(scope)
                    || receipt.status != research::Status::Verified
                    || !matches!(
                        receipt.delivery,
                        Some(research::Delivery::ServerConfirmed | research::Delivery::Unknown)
                    )
                    || (Source::Research {
                        state: receipt.observed.clone(),
                    }) != self.source.closed()
                {
                    return Err(invalid(
                        "Exact fresh original Research close receipt required",
                    ));
                }
                receipt.observed.validate(&receipt.target)?;
                receipt.observed.preserves_close(state)?;
            }
            _ => return Err(invalid("Departure receipt domain differs")),
        }
        Ok(())
    }
}
pub(super) fn validate(data: &Data) -> Result<()> {
    if data.departures.len() > 4096 {
        return Err(invalid("Departure history exceeds bound"));
    }
    let mut selected = std::collections::BTreeSet::new();
    let mut counts = std::collections::BTreeMap::new();
    let mut latest = None;
    for (index, step) in data.departures.iter().enumerate() {
        let selection = data
            .selections
            .get(step.selection)
            .ok_or_else(|| invalid("Departure has no destination"))?;
        let policy = &data.policies[selection.policy];
        let count = counts.entry(step.day).or_insert(0usize);
        step.source.validate(selection, &policy.plan.scope)?;
        if step.id != step.id(data, index)?
            || !selected.insert(step.selection)
            || step.day != day(step.date)?
            || step.date < selection.date
            || latest.is_some_and(|d| step.date < d)
            || *count >= usize::from(policy.limit)
        {
            return Err(invalid("Departure identity, chronology or budget changed"));
        }
        *count += 1;
        latest = Some(step.date);
        match &step.status {
            Status::Intent { failure } => {
                if failure.as_ref().is_some_and(|d| !d.validate())
                    || index + 1 != data.departures.len()
                    || step.selection + 1 != data.selections.len()
                    || selection.terminal.is_some()
                {
                    return Err(invalid("Unresolved departure cannot be bypassed"));
                }
            }
            Status::NotQueued { cause, evidence } => {
                if !cause.validate()
                    || *evidence == FailureEvidence::Unknown
                    || step.selection + 1 != data.selections.len()
                    || selection.terminal.is_some()
                {
                    return Err(invalid("Known departure refusal cannot be replayed"));
                }
            }
            Status::Verified {
                proof,
                destination,
                date,
            } => {
                step.verify(data, proof, destination, *date)?;
                latest = Some(*date);
                if let Some(Terminal::Completed { observed, .. }) = &selection.terminal {
                    if observed.date_raw < *date {
                        return Err(invalid("Goal predates its departure"));
                    }
                }
            }
        }
    }
    Ok(())
}
impl Navigator {
    pub(crate) fn blocked(&self) -> bool {
        self.poisoned
            || self
                .data
                .departures
                .iter()
                .any(|s| matches!(s.status, Status::Intent { .. }))
    }
    pub(super) fn ensure_ready(&mut self) -> Result<()> {
        self.check()?;
        if self.blocked() {
            return Err(Error::new(
                "panel_navigation_blocked",
                "Unresolved departure cannot be replayed or hidden by permission changes",
            ));
        }
        if self
            .data
            .departures
            .last()
            .is_some_and(|s| matches!(s.status, Status::NotQueued { .. }))
        {
            return Err(invalid(
                "Known departure refusal is retained; no automatic new attempt",
            ));
        }
        Ok(())
    }
    pub(crate) fn used_steps(&self, day: i64) -> usize {
        self.data.departures.iter().filter(|s| s.day == day).count()
    }
    /// Reserve exactly one close for this selection. Caller must hold the shared
    /// controller and publish blocked() before releasing it on any error.
    pub(crate) fn begin_departure(
        &mut self,
        source: Source,
        fresh_destination: &Destination,
        date: i32,
    ) -> Result<String> {
        self.ensure_ready()?;
        let selection = self
            .data
            .selections
            .len()
            .checked_sub(1)
            .ok_or_else(|| invalid("No selected destination"))?;
        let selected = &self.data.selections[selection];
        if selected.terminal.is_some()
            || self
                .data
                .departures
                .iter()
                .any(|s| s.selection == selection)
            || &selected.original != fresh_destination
        {
            return Err(invalid(
                "Destination changed or departure already attempted",
            ));
        }
        let mut step = Step {
            id: String::new(),
            selection,
            source,
            date,
            day: day(date)?,
            status: Status::Intent { failure: None },
        };
        step.id = step.id(&self.data, self.data.departures.len())?;
        let id = step.id.clone();
        let mut next = self.data.clone();
        next.departures.push(step);
        next.validate()?;
        self.data = next;
        self.save()?;
        Ok(id)
    }
    fn complete_departure(
        &mut self,
        id: &str,
        proof: Proof,
        destination: Destination,
        date: i32,
    ) -> Result<()> {
        self.check()?;
        let current = self
            .data
            .departures
            .last()
            .ok_or_else(|| invalid("No pending departure"))?;
        if current.id != id || !matches!(current.status, Status::Intent { failure: None }) {
            return Err(invalid(
                "Original pending departure without failure required",
            ));
        }
        current.verify(&self.data, &proof, &destination, date)?;
        let mut next = self.data.clone();
        next.departures.last_mut().expect("pending").status = Status::Verified {
            proof: Box::new(proof),
            destination,
            date,
        };
        next.validate()?;
        self.data = next;
        self.save()
    }
    pub(crate) fn verified_research(
        &mut self,
        id: &str,
        execution: &research::Execution,
        destination: Destination,
        date: i32,
    ) -> Result<()> {
        if execution.historical || !execution.current_state_verified {
            return Err(invalid(
                "Historical Research closing cannot advance navigation",
            ));
        }
        self.complete_departure(
            id,
            Proof::Research {
                receipt: execution.receipt.clone(),
            },
            destination,
            date,
        )
    }
    pub(crate) fn verified_production(
        &mut self,
        id: &str,
        execution: &production::Execution,
        destination: Destination,
        date: i32,
    ) -> Result<()> {
        let (actual, target, before, after) = execution.verified_step()?;
        let (
            production::Target::Panel(target),
            production::State::Panel(before),
            production::State::Panel(after),
        ) = (target, before, after)
        else {
            return Err(invalid("Production closing proof required"));
        };
        self.complete_departure(
            id,
            Proof::Production {
                request_id: actual.into(),
                target: target.clone(),
                before: before.clone(),
                after: after.clone(),
            },
            destination,
            date,
        )
    }
    pub(crate) fn failed_departure(
        &mut self,
        id: &str,
        cause: &Error,
        evidence: FailureEvidence,
        token: &str,
    ) -> Result<()> {
        self.check()?;
        let current = self
            .data
            .departures
            .last()
            .ok_or_else(|| invalid("No pending departure"))?;
        if current.id != id || !matches!(current.status, Status::Intent { failure: None }) {
            return Err(invalid("Original pending departure required"));
        }
        let cause = Diagnostic::from_error(cause, token);
        let mut next = self.data.clone();
        next.departures.last_mut().expect("pending").status =
            if evidence == FailureEvidence::Unknown {
                Status::Intent {
                    failure: Some(cause),
                }
            } else {
                Status::NotQueued { cause, evidence }
            };
        next.validate()?;
        self.data = next;
        self.save()
    }
    pub(super) fn departure_summary(&self) -> Value {
        let latest=self.data.departures.last().map(|s|{let (status,cause)=match &s.status {Status::Intent{failure}=>("Intent",failure.as_ref()),Status::NotQueued{cause,..}=>("NotQueued",Some(cause)),Status::Verified{..}=>("Verified",None)};json!({"day":s.day,"request_id":s.id,"status":status,"cause":cause,"used_steps":self.used_steps(s.day),"limit":self.data.policies[self.data.selections[s.selection].policy].limit})});
        json!({"historical":true,"latest":latest})
    }
}
