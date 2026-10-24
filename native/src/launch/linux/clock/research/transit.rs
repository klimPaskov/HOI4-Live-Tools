//! Internal ordinary W adapter; no public tool or mixed-worker permission.
use super::*;
use crate::{
    daily_governor::Scope,
    daily_panel_navigation::transit::{ManagedPanel, Snapshot},
};

pub(super) fn execute(
    state: &mut ControlState,
    process: &dyn Process,
    gate: &RwLock<bool>,
    token: &str,
    request: transit_stage::Request,
    until: Instant,
) -> Result<(transit_stage::Execution, Option<Value>)> {
    if request.scope.epoch != state.guard.epoch || request.scope.anchors != state.guard.anchors {
        return Err(Error::new(
            "ownership_changed",
            "Original transit scope differs from controller",
        ));
    }
    let ControlState {
        controls,
        guard,
        authority,
        journal,
        research_transits,
        research,
        focus,
        construction,
        production,
    } = state;
    let service = research
        .as_ref()
        .ok_or_else(|| Error::new("capability_unavailable", "Owned Research profile required"))?;
    if service.profile.client != [1920, 1080]
        || service.profile.logical != [1920, 1080]
        || !service.profile.os_binding_verified
        || !service.profile.reviewed_builtin_client_origin
    {
        return Err(Error::new(
            "capability_unavailable",
            "Research transit requires the reviewed native 1080p profile",
        ));
    }
    let domains = Domains {
        clock: journal,
        research: service,
        focus: focus.as_ref(),
        construction: construction.as_ref(),
        production: production.as_ref(),
    };
    let mut backend = Backend {
        process,
        controls,
        guard,
        domains,
        anchor: None,
        until: None,
        last_transport: None,
    };
    let result = research_transits.execute(request, token, authority, gate, &mut backend, until);
    let execution = result.map_err(|mut error| {
        if let Some(code) = backend
            .last_transport
            .as_ref()
            .and_then(|v| v["error"]["code"].as_str())
        {
            error.message.push_str(&format!("; transport_error={code}"));
        }
        transport_failure(error, backend.last_transport.as_ref())
    })?;
    Ok((execution, backend.last_transport))
}
struct Domains<'a> {
    clock: &'a Journal,
    research: &'a Service,
    focus: Option<&'a focus::Service>,
    construction: Option<&'a construction::Service>,
    production: Option<&'a production::Service>,
}
impl Domains<'_> {
    fn clear(&self, game: &Identity) -> Result<()> {
        // The exact current transit Intent is owned by its executing Journal;
        // it must not self-block through the whole-controller predicate.
        if self.clock.blocks_game(game)
            || self.research.journal.blocks_game(game)
            || self.focus.is_some_and(|v| v.journal.blocks_game(game))
            || self
                .construction
                .is_some_and(|v| v.journal.blocks_game(game))
            || self.production.is_some_and(|v| v.journal.blocks_game(game))
        {
            return Err(Error::new(
                "action_uncertain",
                "Other-domain unresolved input blocks Research transit",
            ));
        }
        Ok(())
    }
}
struct Backend<'a> {
    process: &'a dyn Process,
    controls: &'a X11Controls,
    guard: &'a mut Guard,
    domains: Domains<'a>,
    anchor: Option<research_ui::transit::Anchor>,
    until: Option<Instant>,
    last_transport: Option<Value>,
}
impl Backend<'_> {
    fn boundary(&self, until: Instant) -> Result<()> {
        if self.until != Some(until) || Instant::now() >= until {
            return Err(Error::new(
                "panel_transit_deadline",
                "Original Research transit deadline changed/expired",
            ));
        }
        self.domains.clear(&self.guard.epoch.game)
    }
    fn fresh(&mut self, until: Instant) -> Result<Snapshot> {
        self.boundary(until)?;
        let snapshot = self
            .anchor
            .as_mut()
            .ok_or_else(|| Error::new("ownership_changed", "Original transit Anchor absent"))?
            .observe_for_input(self.process)?;
        self.guard
            .accept_context(self.process, snapshot.context())?;
        self.boundary(until)?;
        Ok(snapshot)
    }
}
impl transit_stage::Backend for Backend<'_> {
    fn begin(&mut self, scope: &Scope, until: Instant) -> Result<Snapshot> {
        if self.anchor.is_some() || self.until.is_some() {
            return Err(Error::new(
                "ownership_changed",
                "Transit Anchor cannot be rebound",
            ));
        }
        self.until = Some(until);
        self.boundary(until)?;
        let (anchor, snapshot) =
            research_ui::transit::Anchor::bind_current(self.process, scope, until)?;
        self.guard
            .accept_context(self.process, snapshot.context())?;
        self.anchor = Some(anchor);
        self.boundary(until)?;
        Ok(snapshot)
    }
    fn observe(&mut self, until: Instant) -> Result<Snapshot> {
        self.fresh(until)
    }
    fn preflight(&mut self, until: Instant) -> Result<()> {
        self.boundary(until)?;
        self.controls
            .preflight(self.process, &ControlKey::ResearchPanel)?;
        self.boundary(until)
    }
    fn dispatch_once(&mut self, until: Instant) -> stage::Delivery {
        if let Err(error) = self.boundary(until) {
            self.last_transport = Some(json!({"delivery":"not_queued","error":error}));
            return stage::Delivery::NotQueued;
        }
        let controls = self.controls;
        let process = self.process;
        let mut input_may_queue = false;
        let result = controls.send_until_checked(process, ControlKey::ResearchPanel, until, || {
            let fresh = self.fresh(until)?;
            if fresh.panel() == ManagedPanel::Research {
                return Ok(false);
            }
            if fresh.panel() != ManagedPanel::Idle {
                return Err(Error::new(
                    "research_transit_precondition",
                    "Ordinary transit source changed",
                ));
            }
            self.boundary(until)?;
            input_may_queue = true;
            Ok(true)
        });
        let (delivery, diagnostic) = match result {
            Ok(true) => (
                stage::Delivery::ServerConfirmed,
                json!({"delivery":"server_confirmed","ordinary_key":"W","game_action_verified":false,"event_causality_verified":false}),
            ),
            Ok(false) => (
                stage::Delivery::NotQueued,
                json!({"delivery":"not_queued","reason":"fresh_original_panel_already_shown"}),
            ),
            Err(error) if !input_may_queue => (
                stage::Delivery::NotQueued,
                json!({"delivery":"not_queued","error":error}),
            ),
            Err(error) => (
                stage::Delivery::Unknown,
                json!({"delivery":"unknown","error":error}),
            ),
        };
        self.last_transport = Some(diagnostic);
        delivery
    }
    fn confirm_input_idle(&mut self, until: Instant) -> Result<()> {
        self.preflight(until)
    }
}
