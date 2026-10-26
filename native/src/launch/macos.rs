//! Ordinary input for one owned macOS game: exclusive lease, durable
//! journals, CGEvent keys and clicks only while the owned window is front-most,
//! and fresh memory readback for every step. The clock keeps the Linux tool
//! contract; research starts are driven by live GUI geometry.
mod action_fence;
mod air;
mod airread;
mod armies;
mod commanders;
mod construction;
mod construction_find;
// Internal prospective click opener; no tool/configuration registration.
mod construction_find_open;
// Default-off single ordinary Escape qualification.
mod construction_cancel;
mod construction_open;
// Default-off one-action qualification; full campaign acceptance remains false.
mod construction_place;
mod construction_select;
mod controller_epoch;
mod country_status;
mod daily;
mod decisions;
mod deployment;
mod designs;
mod diplomacy;
mod diplomacy_state;
mod doctrines;
mod events;
mod fleetmem;
mod focus;
mod front_plan;
pub(crate) use focus::input_probe as focus_input_probe;
mod input_gate;
mod intelligence;
mod journal;
#[cfg(test)]
mod live_reads_navalair;
mod mapdata;
mod naval;
mod politics;
mod production;
mod research;
mod economy_detail;
mod resources;
#[cfg(test)]
mod live_reads_mgmt;
mod mgmt;
mod mios;
mod research_extras;
mod script;
mod settle;
mod special_project_rules;
mod special_projects;
mod states;
mod stateslots;
mod supply;
mod supplymem;
mod template_get;
mod units;
#[cfg(test)]
mod live_reads;
#[cfg(test)]
mod testmem;
mod ui;
mod wars;
use crate::{
    actions::{Action, Backend, Epoch, Journal, State},
    error::{Error, Result},
    game,
    input_lease::InputAuthority,
    platform::{
        macos_input::{Key, MacKeys},
        Process,
    },
    protocol::Commands,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs::File,
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    controller: String,
    player: String,
    #[serde(default)]
    daily_research: bool,
    #[serde(default)]
    daily_production: bool,
    #[serde(default)]
    daily_focus: bool,
    /// Qualification only: the ordinary T opener is not campaign accepted.
    #[serde(default)]
    construction_panel: bool,
    /// Qualification only: ordinary template selection does not place a building.
    #[serde(default)]
    construction_selection: bool,
    /// Qualification only: type a state ID into an already-open Find view.
    #[serde(default)]
    construction_navigation: bool,
    /// Qualification only: one exact selected template at the calibrated center.
    #[serde(default)]
    construction_placement: bool,
    #[serde(default)]
    placement_deadline_epoch: Option<f64>,
    /// Qualification only: one original ordinary Escape; no worker integration.
    #[serde(default)]
    construction_cancellation: bool,
    #[serde(default)]
    cancellation_deadline_epoch: Option<f64>,
    /// Qualification only: one ordinary named Find button click; no worker.
    #[serde(default)]
    construction_find_open: bool,
    #[serde(default)]
    find_open_deadline_epoch: Option<f64>,
}

// Convert the caller's absolute qualification bound once;
// never renew it on a lease, request, retry or delayed server response.
fn qualification_remaining(
    enabled: bool,
    epoch: Option<f64>,
    now: f64,
) -> Result<Option<Duration>> {
    if !enabled {
        return Ok(None);
    }
    let end = epoch.ok_or_else(|| {
        Error::new(
            "invalid_input_binding",
            "Action needs its immutable qualification deadline",
        )
    })?;
    let remaining = end - now;
    if !now.is_finite()
        || !end.is_finite()
        || !remaining.is_finite()
        || remaining <= 0.
        || remaining > 180.
    {
        return Err(Error::new(
            "invalid_input_binding",
            "Original qualification deadline must be within 180 seconds",
        ));
    }
    Ok(Some(Duration::from_secs_f64(remaining)))
}
fn nonce() -> Result<String> {
    let mut bytes = [0; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

struct Guard {
    epoch: Epoch,
    anchors: [u64; 3],
    playthrough: String,
    latest_date: i32,
    persisted: controller_epoch::ControllerEpoch,
    poisoned: bool,
}
impl Guard {
    /// Fresh own clock state; refuses another game, campaign or a rewound date.
    fn observe(&mut self, process: &dyn Process) -> Result<State> {
        let context = self.observe_context(process)?;
        Ok(State {
            epoch: self.epoch.clone(),
            paused: context.paused,
            speed: context.speed,
        })
    }
    fn observe_context(&mut self, process: &dyn Process) -> Result<game::ClockContext> {
        if self.poisoned || self.persisted.blocked() {
            return Err(Error::new(
                "action_uncertain",
                "Controller identity is quarantined",
            ));
        }
        if process.identity() != Some(&self.epoch.game) {
            self.poisoned = true;
            return Err(Error::new("ownership_changed", "Clock process differs"));
        }
        let mut retries = 0;
        let context = loop {
            match game::clock_context(process) {
                Err(e) if e.code == "torn_read" && retries < 3 => {
                    retries += 1;
                    thread::sleep(Duration::from_millis(2));
                }
                result => break result?,
            }
        };
        // Persisted validation is sticky, including a later restored context.
        self.persisted.observe(&self.epoch, &context)?;
        if context.anchors != self.anchors
            || context.playthrough_id != self.playthrough
            || context.player != self.epoch.player
            || context.date_raw < self.latest_date
        {
            self.poisoned = true;
            return Err(Error::new(
                "ownership_changed",
                "Campaign context changed; a fresh owned run is required",
            ));
        }
        self.latest_date = context.date_raw;
        Ok(context)
    }
}

struct Control {
    guard: Guard,
    authority: InputAuthority,
    journal: Journal,
    actions: journal::GameJournal,
}

pub struct MacClock {
    construction_panel: bool,
    construction_selection: bool,
    construction_navigation: bool,
    construction_placement: bool,
    placement_until: Option<Instant>,
    construction_cancellation: bool,
    cancellation_until: Option<Instant>,
    construction_find_open: bool,
    find_open_until: Option<Instant>,
    directory: std::path::PathBuf,
    daily: daily::Controller,
    keys: Arc<MacKeys<'static>>,
    state: Arc<Mutex<Control>>,
    process: Arc<dyn Process + Send + Sync>,
}
impl MacClock {
    /// Bind to the owned process already opened from the session manifest.
    pub fn bind(process: Arc<dyn Process + Send + Sync>, directory: &Path) -> Result<Self> {
        let config: Config = serde_json::from_slice(&std::fs::read(directory.join("input.json"))?)
            .map_err(|e| Error::new("invalid_input_binding", e.to_string()))?;
        let qualification_clock_origin = Instant::now();
        let wall = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| Error::new("invalid_input_binding", e.to_string()))?
            .as_secs_f64();
        let placement_until = qualification_remaining(
            config.construction_placement,
            config.placement_deadline_epoch,
            wall,
        )?
        .map(|duration| qualification_clock_origin + duration);
        let cancellation_until = qualification_remaining(
            config.construction_cancellation,
            config.cancellation_deadline_epoch,
            wall,
        )?
        .map(|duration| qualification_clock_origin + duration);
        let find_open_until = qualification_remaining(
            config.construction_find_open,
            config.find_open_deadline_epoch,
            wall,
        )?
        .map(|duration| qualification_clock_origin + duration);
        let game_identity = process
            .identity()
            .cloned()
            .ok_or_else(|| Error::new("input_unavailable", "Owned process identity required"))?;
        let context = game::clock_context(process.as_ref())?;
        if context.player != config.player {
            return Err(Error::new(
                "invalid_input_binding",
                "Configured player differs from the owned campaign",
            ));
        }
        let proposed = Epoch {
            game: game_identity.clone(),
            campaign: format!("{}:{}", nonce()?, context.playthrough_id),
            player: config.player.clone(),
        };
        let keys = Arc::new(MacKeys::bind_owned(process.clone())?);
        let mut persisted = controller_epoch::ControllerEpoch::open(
            &directory.join("controller-epoch.json"),
            proposed,
            &context,
            true,
        )?;
        let epoch = persisted.epoch()?;
        let journal = Journal::open(&directory.join("clock-actions.jsonl"))?;
        let actions = journal::GameJournal::open(&directory.join("game-actions.jsonl"))?;
        let (_, own) = crate::research::owned_snapshot(process.as_ref())?;
        persisted.observe(&epoch, &own.campaign)?;
        let scope = crate::daily_governor::Scope {
            epoch: epoch.clone(),
            anchors: own.campaign.anchors,
            own_country: own.own[0],
            own_status: own.own[1],
        };
        let daily = daily::Controller::bind(directory, &config, scope, own.campaign.date_raw)?;
        Ok(Self {
            construction_panel: config.construction_panel,
            construction_selection: config.construction_selection,
            construction_navigation: config.construction_navigation,
            construction_placement: config.construction_placement,
            placement_until,
            construction_cancellation: config.construction_cancellation,
            cancellation_until,
            construction_find_open: config.construction_find_open,
            find_open_until,
            daily,
            directory: directory.to_path_buf(),
            keys,
            process,
            state: Arc::new(Mutex::new(Control {
                authority: InputAuthority::new(config.controller, epoch.clone())?,
                journal,
                actions,
                guard: Guard {
                    epoch,
                    anchors: context.anchors,
                    playthrough: context.playthrough_id,
                    latest_date: context.date_raw,
                    persisted,
                    poisoned: false,
                },
            })),
        })
    }
}

struct ClockBackend<'a> {
    process: &'a dyn Process,
    keys: &'a MacKeys<'a>,
    guard: &'a mut Guard,
    authority: &'a InputAuthority,
    token: &'a str,
}
impl ClockBackend<'_> {
    fn live(&self, epoch: &Epoch) -> Result<()> {
        self.authority.authorize(self.token, epoch).map(|_| ())
    }
    fn plan(action: &Action, state: &State) -> Option<Key> {
        match action {
            Action::SetPaused(target) if *target != state.paused => Some(Key::Pause),
            Action::SetSpeed(target) if *target > state.speed => Some(Key::Faster),
            Action::SetSpeed(target) if *target < state.speed => Some(Key::Slower),
            _ => None,
        }
    }
    /// One key, then a bounded wait for the matching memory change.
    fn step(&mut self, action: &Action) -> Result<bool> {
        let before = self.guard.observe(self.process)?;
        self.live(&before.epoch)?;
        let Some(key) = Self::plan(action, &before) else {
            return Ok(true);
        };
        self.keys.press(key)?;
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let now = self.guard.observe(self.process)?;
            if (now.paused, now.speed) != (before.paused, before.speed) {
                return Ok(Self::plan(action, &now).is_none());
            }
            if Instant::now() >= deadline {
                return Err(Error::new(
                    "input_readback_failed",
                    "Clock state did not change after one ordinary key",
                ));
            }
            thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Backend for ClockBackend<'_> {
    fn observe(&mut self) -> Result<State> {
        self.guard.observe(self.process)
    }
    fn preflight(&mut self, _action: &Action, before: &State) -> Result<()> {
        self.live(&before.epoch)?;
        self.keys.preflight()?;
        // The game menu pauses the game and swallows the clock keys.
        ui::close_menu(self.process, self.keys)
    }
    fn dispatch(&mut self, action: &Action, _before: &State) -> Result<()> {
        for _ in 0..4 {
            if self.step(action)? {
                return Ok(());
            }
        }
        Err(Error::new(
            "input_readback_failed",
            "Clock target not reached within four ordinary keys",
        ))
    }
}

fn string<'a>(args: &'a serde_json::Map<String, Value>, name: &str) -> Result<&'a str> {
    args.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::new("invalid_arguments", format!("{name} must be a string")))
}
fn allowed(args: &serde_json::Map<String, Value>, keys: &[&str]) -> Result<()> {
    if args.keys().any(|v| !keys.contains(&v.as_str())) {
        return Err(Error::new("invalid_arguments", "Unexpected input argument"));
    }
    Ok(())
}

/// Pure resource/industry reads; also reachable without any input binding
/// through the `macprobe` helper binary.
pub fn read_only_call(process: &dyn Process, name: &str) -> Result<Value> {
    match name {
        "hoi4_resources" => resources::resources(process),
        "hoi4_production_shortages" => resources::production_shortages(process),
        "hoi4_industry" => resources::industry(process),
        "hoi4_supply" => supply::report(process, 0.7),
        "hoi4_economy_detail" => economy_detail::detail(process),
        _ => Err(Error::new("capability_unavailable", "Unknown read-only tool")),
    }
}

impl Commands for MacClock {
    fn tools(&self) -> Vec<Value> {
        vec![
            country_status::tool(),
            diplomacy_state::tool(),
            naval::fleets_schema(),
            naval::convoys_schema(),
            airread::schema(),
            designs::schema(),
            doctrines::tool(),
            mios::tool(),
            intelligence::tool(),
            research_extras::tool(),
        ]
        .into_iter()
        .chain(resources::tools())
        .chain([economy_detail::tool()])
        .chain([
            construction_open::tool(),
            construction_select::tool(),
            construction_find::tool(),
            construction_place::tool(),
            construction_cancel::tool(),
            construction_find_open::tool(),
            daily::Controller::tool(),
            json!({"name":"hoi4_plan","description":"Read or change the standing plan (plan.json in the run directory): research priorities, focus path, construction, training, production, politics, decisions. {op:get} returns it; {op:set, plan:{...}} merges the given keys (null removes a key). hoi4_plan_run carries it out.","inputSchema":{"type":"object","oneOf":[{"properties":{"op":{"const":"get"}},"required":["op"],"additionalProperties":false},{"properties":{"op":{"const":"set"},"plan":{"type":"object"}},"required":["op","plan"],"additionalProperties":false}]}}),
            json!({"name":"hoi4_plan_run","description":"Carry out the standing plan for up to `seconds` (default 60): answer news and single-option events, fill empty research slots and the focus slot from the plan's lists, make planned politics picks and decisions, keep the training queue, put idle military factories on the priority lines and spend idle civilian factories on planned construction. Every step is an ordinary verified action under the given lease; the game is never paused. Returns early when a choice event needs the model or a peace conference opens. Use it instead of hoi4_wait_for while nothing else needs attention.","inputSchema":{"type":"object","properties":{"seconds":{"type":"integer","minimum":1,"maximum":300},"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_input_lease","description":"Exclusive configured-controller lease for ordinary macOS input. Acquire uses holder; renew/release use only lease_token. Status takes only op. Observations do not need a lease.","inputSchema":{"type":"object","oneOf":[
                {"properties":{"op":{"const":"acquire"},"holder":{"type":"string"},"seconds":{"type":"integer","minimum":30,"maximum":900}},"required":["op","holder"],"additionalProperties":false},
                {"properties":{"op":{"const":"renew"},"lease_token":{"type":"string"},"seconds":{"type":"integer","minimum":30,"maximum":900}},"required":["op","lease_token"],"additionalProperties":false},
                {"properties":{"op":{"const":"release"},"lease_token":{"type":"string"}},"required":["op","lease_token"],"additionalProperties":false},
                {"properties":{"op":{"const":"status"}},"required":["op"],"additionalProperties":false}]}}),
            json!({"name":"hoi4_research_start","description":"Start an own technology in an empty research slot with ordinary input (research panel, slot, folder tab, tile, Research), each click located from the live GUI and each stage verified from memory; succeeds only when the slot holds the technology. Refuses researched (code already_researched), already-assigned or busy-slot targets before any input. A tile scrolled out of view is refused. Stable request_id; an unresolved dispatch is settled only by retrying its own request_id.","inputSchema":{"type":"object","properties":{"technology":{"type":"string","maxLength":128},"slot":{"type":"integer","minimum":0,"maximum":15},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["technology","slot","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_focus_start","description":"Start an own national focus with ordinary input (politics panel, focus button, drag-panning the tree until the node is in view, the node, Start), each target located from the live GUI and each stage verified from memory; succeeds only when own focus progress names the focus as active. Refuses completed or already-active focuses, or any while another focus is active, before input. Typed errors: focus_not_available (the detail window opened but the game disables Start: requirements unmet or an exclusive focus taken; Start was not clicked), focus_detail_not_opened (the node click was sent, no detail window read; re-observe before retrying), ui_blocked (names the notice pop-up classes and open events still shown). Stable request_id; an unresolved dispatch is settled only by retrying its own request_id.","inputSchema":{"type":"object","properties":{"focus":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["focus","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_production_add","description":"Add one own military production line for a base equipment type (e.g. infantry_equipment, artillery_equipment, light_tank_equipment, fighter_equipment) with ordinary input: production panel, category button, the design in the selection window, then +/- until the line requests the given factories. Verified from the own line collection. Refuses equipment with no producible design. Naval lines are not supported yet.","inputSchema":{"type":"object","properties":{"equipment":{"type":"string","maxLength":128},"factories":{"type":"integer","minimum":1,"maximum":150},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["equipment","factories","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_production_set","description":"Set the requested factories of an own production line, addressed by its entry_index and equipment_id from hoi4_production_lines, with the line's ordinary +/- buttons; each click is verified from memory. 0 keeps the line with no factories.","inputSchema":{"type":"object","properties":{"line":{"type":"integer","minimum":0,"maximum":2047},"equipment":{"type":"string","maxLength":128},"factories":{"type":"integer","minimum":0,"maximum":150},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["line","equipment","factories","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_construction_add","description":"Queue more of a building in an own state with ordinary input: construction panel, the building button, the game's Find view to centre the state (F, state_id, Return), then clicks at the screen centre. Verified from the own construction queue (the building located in that state); placement mode is switched off afterwards. Typed errors: building_select_failed (the building button click was sent, the selection never named it, no map click was sent), no_free_slot_or_not_allowed (one centre click did not raise the queue in the state: likely no free slot or building not allowed, not proven; check hoi4_state_slots; never repeat blindly), ui_blocked (names the notice pop-up classes and open events). Building keys: arms_factory, industrial_complex, dockyard, infrastructure, air_base, anti_air_building, radar_station, synthetic_refinery, fuel_silo, rocket_site, ... (state buildings only).","inputSchema":{"type":"object","properties":{"building":{"type":"string","maxLength":64},"state_id":{"type":"integer","minimum":1,"maximum":4095},"count":{"type":"integer","minimum":1,"maximum":10,"default":1},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["building","state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_politics_options","description":"List the ideas offered for one politics slot (opens the slot's selection window and closes it again; changes nothing). category: government (slots 0 mobilization law, 1 trade law, 2 economy law, 3-5 political advisors), research_production (designers) or military_staff. Returns each idea key with the game's enabled state and the slot's current idea.","inputSchema":{"type":"object","properties":{"category":{"type":"string","enum":["government","research_production","military_staff"]},"slot":{"type":"integer","minimum":0,"maximum":7},"lease_token":{"type":"string"}},"required":["category","slot","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_politics_choose","description":"Take an idea (law, advisor, designer) for one politics slot with ordinary input; verified when the slot holds the idea in memory. Refuses ideas the game disables. Same category/slot numbering as hoi4_politics_options.","inputSchema":{"type":"object","properties":{"category":{"type":"string","enum":["government","research_production","military_staff"]},"slot":{"type":"integer","minimum":0,"maximum":7},"idea":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["category","slot","idea","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_decisions_list","description":"Open the decisions panel and list offered decisions (key, enabled by the game, whether a timer for it is running). Changes nothing.","inputSchema":{"type":"object","properties":{"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_decision_take","description":"Take an own decision with ordinary input (decisions panel, its row, select; the game's confirmation for exactly this decision is accepted). Verified when the decision view shows it taken (timer started, entry gone or disabled). Refuses decisions the game disables.","inputSchema":{"type":"object","properties":{"decision":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["decision","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_division_templates","description":"Own division template names (for hoi4_train) and how many training lines each has queued. Pure read; no lease needed.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
            json!({"name":"hoi4_train","description":"Queue divisions of an own template with ordinary input (deployment panel, the template's Train button once per division) and give every training line without a location the own state deploy_state_id, since a line without a location never deploys. Each click is verified by one more training line; each location by the line's deployment province.","inputSchema":{"type":"object","properties":{"template":{"type":"string","maxLength":128},"count":{"type":"integer","minimum":1,"maximum":24},"deploy_state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["template","count","deploy_state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_deploy_locations","description":"Give every training line without a deployment location the own state state_id (Find view, the line's location button, one click on the state). Lines without a location never deploy. Verified by the lines' deployment provinces.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_armies","description":"Own armies (name, division ids) and unassigned division ids. Pure read; no lease needed.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
            json!({"name":"hoi4_army_front","description":"Give an own army a front-line order along the border nearest a state: army card, front-line drawing (Z), Find view centres the state, one click on the nearest highlighted border. Verified by one more battle-plan order on the army.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_army_move","description":"Order an own army to move to the centre of a state with one right click (army card, Find view, right click); a plain move attacks enemy units there. Verified by the army's divisions heading for the state or stopping short; use for mopping up, battle plans remain the main tool.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_army_plan","description":"Execute or stop an own army's battle plan with the army card's play or stop button. Verified by the card's executing state.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"execute":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","execute","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_army_offensive","description":"Draw an offensive line for an own army that already holds a front line, into a target state: army card, Find view centres the state, offensive-line drawing (X), one right-button drag from the army's side to the state. Verified by new battle-plan orders on the army; execute the plan afterwards.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_states","description":"Find states by English name (accent-insensitive substring, or the exact ID), or list every state a country owns or controls by its tag (e.g. GER), with owner and controller as the political map shows them. Pure read.","inputSchema":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}}),
            json!({"name":"hoi4_wars","description":"Own land borders, occupied and threatened own states, from state owner/controller, own division positions and state adjacency derived from the install map files. Pure read; no lease. Stage B: wars lists the own wars (enemies, allies at war, start date) from war relations; sides, leaders, war score and justification timers are unvalidated; hostility is the war enemies plus the occupation proxy and the optional enemies tags you pass; covered is a heuristic (own divisions stand in the border states), not a front-line-order read. Alerts: own_state_occupied, border_uncovered, threatened_state_undefended.","inputSchema":{"type":"object","properties":{"enemies":{"type":"array","items":{"type":"string","minLength":3,"maxLength":3},"maxItems":32},"max_states":{"type":"integer","minimum":1,"maximum":1100}},"additionalProperties":false}}),
            json!({"name":"hoi4_front_plan","description":"War plan for the named enemy tags (a tag, a comma list or an array): per contiguous border the own and enemy border states, ranked attack axes (cheapest route by terrain, infrastructure, urban and bunkers toward the enemy capital and valuable states, with the states along it), holding-line and second-line states for the other fronts, own divisions per front versus what is missing, and concrete hoi4_army_front / hoi4_army_offensive / hoi4_army_plan suggestions with state ids (add request_id and lease_token). Pure read; visible ownership and own division positions only, vanilla map files; no lease.","inputSchema":{"type":"object","properties":{"enemy":{"type":["string","array"],"items":{"type":"string"},"description":"enemy tag(s), e.g. POL or [\"FRA\",\"BEL\"]"},"max_axes":{"type":"integer","minimum":1,"maximum":6}},"required":["enemy"],"additionalProperties":false}}),
            json!({"name":"hoi4_encirclement_candidates","description":"Pockets of enemy-controlled states that taking at most max_gap neck states would cut off: the pocket, the neck, the two pincer arms (own state into neck state), divisions needed and available per arm, value (points, industry, manpower, resources) and a score; sea, impassable states and neutral neighbours count as walls. Pure read; no lease.","inputSchema":{"type":"object","properties":{"enemy":{"type":["string","array"],"items":{"type":"string"}},"max_gap":{"type":"integer","minimum":0,"maximum":3},"limit":{"type":"integer","minimum":1,"maximum":20}},"required":["enemy"],"additionalProperties":false}}),
            json!({"name":"hoi4_war_goal_targets","description":"Which enemy-owned states to demand in the peace conference, ranked by points, industry, manpower and resources with tiers (must_take, worth_taking, optional), resource prizes, contiguous blocks, own cores and which states we already occupy. A read, not an action; the conference cost itself is not modelled. No lease.","inputSchema":{"type":"object","properties":{"enemy":{"type":["string","array"],"items":{"type":"string"}},"limit":{"type":"integer","minimum":1,"maximum":100}},"required":["enemy"],"additionalProperties":false}}),
            json!({"name":"hoi4_events","description":"Open event windows in order (the last is on top) with their title and option texts. Pure read; no lease needed.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
            json!({"name":"hoi4_event_choose","description":"Choose an option (0-based, in listed order) of the open event on top by clicking it. Verified when exactly that event window closes.","inputSchema":{"type":"object","properties":{"event":{"type":"string"},"option":{"type":"integer","minimum":0,"maximum":7},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["event","option","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_diplomacy","description":"Open the diplomacy view for the owner of a state (Find view centres the state, one click selects the country) and list the offered actions with enabled state, cost and whether the AI would refuse. Changes nothing in the game.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"lease_token":{"type":"string"}},"required":["state_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_diplomacy_act","description":"Take one diplomatic action (exact listed name, e.g. Declare war, Justify War goal, Guarantee independence, Improve relations) toward the owner of a state. Justify War goal also needs war_goal (e.g. Conquer) and 1..5 states by name. Verified when the offered actions change or political power is spent.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"action":{"type":"string"},"war_goal":{"type":"string"},"states":{"type":"array","items":{"type":"string"},"maxItems":5},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["state_id","action","request_id","lease_token"],"additionalProperties":false}}),
            commanders::tool(),
            supply::tool(),
            template_get::tool(),
            special_projects::tool(),
            special_projects::panel_tool(),
            special_projects::assign_tool(),
            special_projects::stop_tool(),
            special_projects::unassign_tool(),
            special_projects::recruit_tool(),
            json!({"name":"hoi4_army_commander","description":"Assign a general (exact name as listed by hoi4_commanders) to an own army, or a field marshal to an own army group (use the army group's name), from the panel's commander list. Verified when the army's commander is that general. Refused before any click with army_unknown (lists valid armies/groups), unknown_commander (5 closest names) or commander_unavailable (already leads another army/group; pass reassign: true to move them).","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"general":{"type":"string"},"reassign":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","general","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_air_wings","description":"Own air wings with air base, assigned region and missions, from the Air Forces Overview (opens it). Changes nothing in the game.","inputSchema":{"type":"object","properties":{"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_air_mission","description":"Give an own air wing (exact name) a mission over the strategic region containing a state: wing row, mission button, Find view centres the state, right click assigns. Missions: air_superiority, cas, interception, strategic_bombing, naval_strike, kamikaze, port_strike, logistics_strike, air_supply, training, naval_mines, naval_minesweeping, recon, naval_patrol. Verified when the wing shows a changed region or mission.","inputSchema":{"type":"object","properties":{"wing":{"type":"string"},"mission":{"type":"string"},"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["wing","mission","state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_army_group_create","description":"Group two or more own armies into a new army group (army cards with Shift, then the new-army-group card). Give it a field marshal with hoi4_army_commander using the army group's name. Verified when all the armies belong to one new army group.","inputSchema":{"type":"object","properties":{"armies":{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":12},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["armies","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_army_split","description":"Split the first N divisions (list order, N 1..19) of an own army into a new army: the army's card, the panel's first row, Shift on row N, then the new-army card. Use it to bring an oversized army (more than 24 divisions) under its general's cap. Verified by a new army holding exactly those divisions.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"count":{"type":"integer","minimum":1,"maximum":19},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","count","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_state_slots","description":"Free building slots of own states (unlocked, max, used, free) read from each state's view (Find view centres the state, one click opens it). Use it to choose where a factory can still be queued before hoi4_construction_add. Changes nothing in the game; up to 12 states per call.","inputSchema":{"type":"object","properties":{"state_ids":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4095},"minItems":1,"maxItems":12},"lease_token":{"type":"string"}},"required":["state_ids","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_army_create","description":"(optional absorb_existing: true folds armies standing there into the new one) Create one army from the own divisions shown around a state: Find view centres the state, a box selection picks own divisions there, the new-army card creates the army. Verified by a new army in memory; returns its divisions. Refused before any input with already_in_army (every own division in that state belongs to a named army; use absorb_existing or hoi4_army_split) or no_own_divisions (none stand there per hoi4_armies); input_readback_failed after the click makes no claim about the cause.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"absorb_existing":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_army_assign_plan","description":"Pure read: where the unassigned divisions standing in a state would go (into the standing army with the most room under 24, or the named one; a new army when none stands there), how many, the overflow and the split needed afterwards. Typed refusals: no_unassigned_here, no_such_army, target_not_in_state, army_full.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"army":{"type":"string","maxLength":128}},"required":["state_id"],"additionalProperties":false}}),
            json!({"name":"hoi4_army_assign","description":"Fold the unassigned divisions standing in a state into an existing army there without disturbing other armies, or create a new army when none stands there. Same refusals as hoi4_army_assign_plan, all before any input. Folding into a standing army uses the game's own control (box-select the divisions, RIGHT-click the army's card); it refuses with selection_mismatch, before the click, when the box also took divisions of another army. Creating a new army uses the verified new-army path.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"army":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_time","description":"Set desired pause or internal speed 0..4 with ordinary keys (Space, keypad +/-) sent only while the owned game window is front-most, each verified by fresh memory readback. Supply exactly one target and a stable request_id; uncertain dispatches are never replayed.","inputSchema":{"type":"object","properties":{"paused":{"type":"boolean"},"speed":{"type":"integer","minimum":0,"maximum":4},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}}),
        ])
        .collect()
    }
    // Broker allowlist note: `hoi4_country_status` is a pure read (no lease,
    // no input, no cursor); an external broker allowlist that names the other
    // read-only tools (hoi4_armies, hoi4_states, hoi4_events) should list it too.
    fn supports(&self, name: &str) -> bool {
        matches!(
            name,
            "hoi4_daily_governor"
                | "hoi4_fleets"
                | "hoi4_convoys"
                | "hoi4_air"
                | "hoi4_designs"
                | "hoi4_input_lease"
                | "hoi4_time"
                | "hoi4_research_start"
                | "hoi4_focus_start"
                | "hoi4_production_add"
                | "hoi4_production_set"
                | "hoi4_construction_add"
                | "hoi4_construction_open"
                | "hoi4_construction_select"
                | "hoi4_construction_find_state"
                | "hoi4_construction_place"
                | "hoi4_construction_clear_selection"
                | "hoi4_construction_find_open"
                | "hoi4_politics_options"
                | "hoi4_politics_choose"
                | "hoi4_decisions_list"
                | "hoi4_decision_take"
                | "hoi4_division_templates"
                | "hoi4_train"
                | "hoi4_deploy_locations"
                | "hoi4_armies"
                | "hoi4_country_status"
                | "hoi4_resources"
                | "hoi4_production_shortages"
                | "hoi4_industry"
                | "hoi4_economy_detail"
                | "hoi4_diplomacy_state"
                | "hoi4_doctrines"
                | "hoi4_mios"
                | "hoi4_intelligence"
                | "hoi4_research_extras"
                | "hoi4_states"
                | "hoi4_wars"
                | "hoi4_front_plan"
                | "hoi4_encirclement_candidates"
                | "hoi4_war_goal_targets"
                | "hoi4_events"
                | "hoi4_event_choose"
                | "hoi4_diplomacy"
                | "hoi4_diplomacy_act"
                | "hoi4_army_create"
                | "hoi4_army_assign_plan"
                | "hoi4_army_assign"
                | "hoi4_army_front"
                | "hoi4_army_plan"
                | "hoi4_army_offensive"
                | "hoi4_army_move"
                | "hoi4_army_commander"
                | "hoi4_commanders"
                | "hoi4_supply"
                | "hoi4_template_get"
                | "hoi4_special_projects"
                | "hoi4_special_projects_panel"
                | "hoi4_special_project_assign"
                | "hoi4_special_project_stop"
                | "hoi4_special_project_unassign"
                | "hoi4_special_project_recruit"
                | "hoi4_army_group_create"
                | "hoi4_army_split"
                | "hoi4_state_slots"
                | "hoi4_air_wings"
                | "hoi4_air_mission"
        )
    }
    fn wait_events(&self) -> Option<Arc<crate::protocol::worker_events::Events>> {
        Some(self.daily.events())
    }
    fn plan_path(&self) -> Option<std::path::PathBuf> {
        Some(self.directory.join("plan.json"))
    }
    fn capabilities(&self) -> Value {
        json!({"transport":"ordinary macOS CGEvent keys to the front-most owned window","actions":["set_paused","set_speed","research_start","focus_start","production_add","production_set","construction_add","politics_options","politics_choose","decisions_list","decision_take","train","deploy_locations","army_create","army_assign","army_front","army_plan","army_offensive","event_choose","diplomacy","diplomacy_act","army_commander","army_group_create","army_split","state_slots","air_wings","air_mission","plan_run"],"lease":"exclusive configured controller","input_profile":{"name":"macos_cocoa_absolute_1920x1080_v1","checked_at_bind":true,"rechecked_before_each_input":true,"diagnostic_alone_authorizes_input":false},"construction_panel":{"enabled":self.construction_panel,"qualification_only":true,"ordinary_key":"t","live_delivery_verified":false,"input_retry":false},"construction_selection":{"enabled":self.construction_selection,"qualification_only":true,"ordinary_clicks_max":1,"input_retry":false,"placement":false},"construction_placement":{"enabled":self.construction_placement,"qualification_only":true,"ordinary_clicks_max":1,"full_picker_verified":false,"atomic_event_verified":false},"construction_cancellation":{"enabled":self.construction_cancellation,"qualification_only":true,"ordinary_key":"escape","ordinary_key_presses_max":1,"input_retry":false,"worker":false},"construction_find_open":{"enabled":self.construction_find_open,"qualification_only":true,"transport":"ordinary_left_click","ordinary_clicks_max":1,"cursor_moves_max":1,"ordinary_key_presses":0,"input_retry":false,"worker":false},"construction_navigation":{"enabled":self.construction_navigation,"qualification_only":true,"already_open_only":true,"input_retry":false,"placement":false},"daily_governor":self.daily.capabilities(),"native_posting":false,"full_campaign_acceptance":false})
    }
    fn call(&self, process: &dyn Process, name: &str, arguments: &Value) -> Result<Value> {
        if process.identity() != self.process.identity() {
            return Err(Error::new(
                "ownership_changed",
                "Mac controller reader changed",
            ));
        }
        let process = self.process.as_ref();
        let args = arguments
            .as_object()
            .ok_or_else(|| Error::new("invalid_arguments", "Input arguments must be an object"))?;
        if name == "hoi4_daily_governor" {
            return self.daily.call(self, args);
        }
        if name == "hoi4_country_status" {
            allowed(args, &[])?;
            return country_status::status(process);
        }
        if matches!(
            name,
            "hoi4_resources" | "hoi4_production_shortages" | "hoi4_industry" | "hoi4_economy_detail"
        ) {
            allowed(args, &[])?;
            return read_only_call(process, name);
        }
        if name == "hoi4_diplomacy_state" {
            allowed(args, &[])?;
            return diplomacy_state::state(process);
        }
        if name == "hoi4_fleets" {
            allowed(args, &["ships", "fleet"])?;
            return naval::fleets_tool(process, args);
        }
        if name == "hoi4_convoys" {
            allowed(args, &[])?;
            return naval::convoys_tool(process, args);
        }
        if name == "hoi4_air" {
            allowed(args, &["wing"])?;
            return airread::tool(process, args);
        }
        if name == "hoi4_designs" {
            allowed(args, &["kind", "custom_only", "detail", "name"])?;
            return designs::tool(process, args);
        }
        if name == "hoi4_doctrines" {
            allowed(args, &["include_catalog"])?;
            return doctrines::doctrines(process, mgmt::flag(args, "include_catalog")?);
        }
        if name == "hoi4_mios" {
            allowed(args, &["tag", "include_all"])?;
            let tag = args.get("tag").map(|_| string(args, "tag")).transpose()?;
            if tag.is_some_and(|t| t.len() != 3 || !t.bytes().all(|b| b.is_ascii_uppercase())) {
                return Err(Error::new("invalid_arguments", "tag must be three capital letters"));
            }
            return mios::mios(process, tag, mgmt::flag(args, "include_all")?);
        }
        if name == "hoi4_intelligence" {
            allowed(args, &["include_operations"])?;
            return intelligence::intelligence(process, mgmt::flag(args, "include_operations")?);
        }
        if name == "hoi4_research_extras" {
            allowed(args, &[])?;
            return research_extras::research_extras(process);
        }
        if name == "hoi4_commanders" {
            allowed(args, &[])?;
            return commanders::list(process);
        }
        if name == "hoi4_supply" {
            allowed(args, &["strength_below"])?;
            let below = match args.get("strength_below") {
                None => 0.7,
                Some(v) => v
                    .as_f64()
                    .filter(|v| (0.0..=1.0).contains(v))
                    .ok_or_else(|| Error::new("invalid_arguments", "strength_below must be 0..1"))?,
            };
            return supply::report(process, below);
        }
        if name == "hoi4_template_get" {
            allowed(args, &["template"])?;
            let only = match args.get("template") {
                None => None,
                Some(v) => Some(
                    v.as_str()
                        .filter(|s| !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control))
                        .ok_or_else(|| Error::new("invalid_arguments", "Invalid template name"))?,
                ),
            };
            return template_get::get(process, only);
        }
        if name == "hoi4_special_projects" {
            allowed(args, &["rules"])?;
            return special_projects::list(process, special_projects::rules_arg(args.get("rules"))?);
        }
        if name == "hoi4_armies" {
            allowed(args, &[])?;
            return armies::armies(process);
        }
        if name == "hoi4_wars" {
            allowed(args, &["enemies", "max_states"])?;
            let exe = &process
                .identity()
                .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
                .exe;
            return wars::wars(process, &mapdata::game_dir(exe)?, args);
        }
        if matches!(
            name,
            "hoi4_front_plan" | "hoi4_encirclement_candidates" | "hoi4_war_goal_targets"
        ) {
            allowed(args, &["enemy", "enemies", "max_axes", "max_gap", "limit"])?;
            let exe = &process
                .identity()
                .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
                .exe;
            let dir = mapdata::game_dir(exe)?;
            return match name {
                "hoi4_front_plan" => front_plan::front_plan_tool(process, &dir, args),
                "hoi4_encirclement_candidates" => front_plan::encirclement_tool(process, &dir, args),
                _ => front_plan::war_goal_tool(process, &dir, args),
            };
        }
        if name == "hoi4_events" {
            allowed(args, &[])?;
            return events::list(process);
        }
        if name == "hoi4_army_assign_plan" {
            allowed(args, &["state_id", "army"])?;
            let state = args
                .get("state_id")
                .and_then(Value::as_i64)
                .filter(|v| (1..4096).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "state_id must be 1..4095"))?
                as i32;
            let army = args.get("army").and_then(Value::as_str);
            let view = armies::armies(process)?;
            return Ok(armies::plan_assign(&view, state, army, armies::ARMY_CAP)?.to_json());
        }
        if name == "hoi4_states" {
            allowed(args, &["query"])?;
            let query = string(args, "query")?;
            if query.len() > 64 || query.chars().any(char::is_control) {
                return Err(Error::new("invalid_arguments", "Invalid state query"));
            }
            let exe = &process
                .identity()
                .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
                .exe;
            return states::lookup(process, &mapdata::game_dir(exe)?, query);
        }
        if name == "hoi4_division_templates" {
            allowed(args, &[])?;
            process.begin_snapshot()?;
            let r = crate::memory::Reader::new(process);
            let image = process.image_base();
            let names = deployment::templates(&r, image)?;
            let rows = names
                .iter()
                .map(|n| Ok(json!({"template":n,"queued_lines":deployment::queued(&r, image, n)?})))
                .collect::<Result<Vec<_>>>()?;
            process.check()?;
            return Ok(json!({"templates":rows}));
        }
        let mut state = crate::daily_worker::input_state(&self.state, || self.daily.input_check())?;
        if name != "hoi4_input_lease" {
            input_gate::check(
                &state.journal,
                &state.actions,
                &state.guard.epoch.game,
                name == "hoi4_time",
                args.get("request_id").and_then(Value::as_str),
            )?;
        }
        let Control {
            guard,
            authority,
            journal,
            actions,
        } = &mut *state;
        if name == "hoi4_construction_find_open" {
            allowed(
                args,
                &["building_id", "state_id", "request_id", "lease_token"],
            )?;
            let key = string(args, "building_id")?;
            crate::construction::controls::validate_key(key)?;
            let state_id = crate::construction::find_view::validate_state_id(
                args.get("state_id")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| {
                        Error::new("invalid_arguments", "state_id must be an integer")
                    })?,
            )?;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.epoch.clone();
            let lease_end = authority.authorize(token, &epoch)?;
            let request = construction_find_open::Request { id, key, state_id };
            // A prior Verified receipt is historical: no fresh campaign payload
            // or input can settle or replace it. Lease and epoch remain required.
            if let Some(receipt) =
                construction_find_open::historical_receipt(&request, &epoch, actions)?
            {
                return Ok(receipt);
            }
            let expected = guard.observe_context(process)?;
            let fence = action_fence::Fence {
                process,
                epoch: &epoch,
                expected: &expected,
                authority,
                token,
                until: (Instant::now() + Duration::from_secs(12))
                    .min(lease_end)
                    .min(self.find_open_until.unwrap_or_else(Instant::now)),
                stop: None,
            };
            return construction_find_open::run(
                self.construction_find_open,
                request,
                &epoch,
                actions,
                &fence,
                &self.keys,
            );
        }
        if name == "hoi4_construction_clear_selection" {
            allowed(
                args,
                &["building_id", "state_id", "request_id", "lease_token"],
            )?;
            let key = string(args, "building_id")?;
            crate::construction::controls::validate_key(key)?;
            let state_id = crate::construction::find_view::validate_state_id(
                args.get("state_id")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| {
                        Error::new("invalid_arguments", "state_id must be an integer")
                    })?,
            )?;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.epoch.clone();
            let lease_end = authority.authorize(token, &epoch)?;
            let request = construction_cancel::Request { id, key, state_id };
            // A prior Verified receipt is historical: no fresh campaign payload
            // or input can settle or replace it. Lease and epoch remain required.
            if let Some(receipt) =
                construction_cancel::historical_receipt(&request, &epoch, actions)?
            {
                return Ok(receipt);
            }
            let expected = guard.observe_context(process)?;
            let fence = action_fence::Fence {
                process,
                epoch: &epoch,
                expected: &expected,
                authority,
                token,
                until: (Instant::now() + Duration::from_secs(12))
                    .min(lease_end)
                    .min(self.cancellation_until.unwrap_or_else(Instant::now)),
                stop: None,
            };
            return construction_cancel::run(
                self.construction_cancellation,
                request,
                &epoch,
                actions,
                &fence,
                &self.keys,
            );
        }
        if name == "hoi4_construction_place" {
            allowed(
                args,
                &["building_id", "state_id", "request_id", "lease_token"],
            )?;
            let key = string(args, "building_id")?;
            crate::construction::controls::validate_key(key)?;
            let state_id = crate::construction::find_view::validate_state_id(
                args.get("state_id")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| {
                        Error::new("invalid_arguments", "state_id must be an integer")
                    })?,
            )?;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let expected = guard.observe_context(process)?;
            let epoch = guard.epoch.clone();
            let lease_end = authority.authorize(token, &epoch)?;
            let fence = action_fence::Fence {
                process,
                epoch: &epoch,
                expected: &expected,
                authority,
                token,
                until: (Instant::now() + Duration::from_secs(12))
                    .min(lease_end)
                    .min(self.placement_until.unwrap_or_else(Instant::now)),
                stop: None,
            };
            return construction_place::run(
                self.construction_placement,
                construction_place::Request { id, key, state_id },
                &epoch,
                actions,
                &fence,
                &self.keys,
            );
        }
        if name == "hoi4_construction_find_state" {
            allowed(
                args,
                &["building_id", "state_id", "request_id", "lease_token"],
            )?;
            let key = string(args, "building_id")?;
            crate::construction::controls::validate_key(key)?;
            let state_id = crate::construction::find_view::validate_state_id(
                args.get("state_id")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| {
                        Error::new("invalid_arguments", "state_id must be an integer")
                    })?,
            )?;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let expected = guard.observe_context(process)?;
            let epoch = guard.epoch.clone();
            let lease_end = authority.authorize(token, &epoch)?;
            let fence = action_fence::Fence {
                process,
                epoch: &epoch,
                expected: &expected,
                authority,
                token,
                until: (Instant::now() + Duration::from_secs(12)).min(lease_end),
                stop: None,
            };
            return construction_find::run(
                self.construction_navigation,
                construction_find::Request { id, key, state_id },
                &epoch,
                actions,
                &fence,
                &self.keys,
            );
        }
        if name == "hoi4_construction_select" {
            allowed(
                args,
                &["building_id", "state_id", "request_id", "lease_token"],
            )?;
            let key = string(args, "building_id")?;
            crate::construction::controls::validate_key(key)?;
            let state_id = crate::construction::find_view::validate_state_id(
                args.get("state_id")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| {
                        Error::new("invalid_arguments", "state_id must be an integer")
                    })?,
            )?;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let expected = guard.observe_context(process)?;
            let epoch = guard.epoch.clone();
            let lease_end = authority.authorize(token, &epoch)?;
            let fence = action_fence::Fence {
                process,
                epoch: &epoch,
                expected: &expected,
                authority,
                token,
                until: (Instant::now() + Duration::from_secs(12)).min(lease_end),
                stop: None,
            };
            return construction_select::run(
                self.construction_selection,
                construction_select::Request { id, key, state_id },
                &epoch,
                actions,
                &fence,
                &self.keys,
            );
        }
        if name == "hoi4_construction_open" {
            allowed(args, &["request_id", "lease_token", "already_open_only"])?;
            let mode = construction_open::Mode::parse(args.get("already_open_only"))?;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let expected = guard.observe_context(process)?;
            let epoch = guard.epoch.clone();
            let lease_end = authority.authorize(token, &epoch)?;
            let fence = action_fence::Fence {
                process,
                epoch: &epoch,
                expected: &expected,
                authority,
                token,
                until: (Instant::now() + Duration::from_secs(12)).min(lease_end),
                stop: None,
            };
            return construction_open::run(
                self.construction_panel,
                mode,
                id,
                &epoch,
                actions,
                &fence,
                &self.keys,
            );
        }
        if name == "hoi4_input_lease" {
            let op = string(args, "op")?;
            let seconds = args.get("seconds").and_then(Value::as_u64).unwrap_or(300);
            if !(30..=900).contains(&seconds) {
                return Err(Error::new(
                    "invalid_arguments",
                    "Lease seconds must be 30..900",
                ));
            }
            let epoch = guard.observe(process)?.epoch;
            return match op {
                "acquire" => {
                    allowed(args, &["op", "holder", "seconds"])?;
                    let token = nonce()?;
                    authority.acquire(string(args, "holder")?, token.clone(), seconds, &epoch)?;
                    Ok(json!({"lease_token":token,"epoch":epoch,"seconds":seconds}))
                }
                "renew" => {
                    allowed(args, &["op", "lease_token", "seconds"])?;
                    authority.renew(string(args, "lease_token")?, seconds, &epoch)?;
                    Ok(json!({"renewed":true,"seconds":seconds}))
                }
                "release" => {
                    allowed(args, &["op", "lease_token"])?;
                    authority.release(string(args, "lease_token")?, &epoch)?;
                    Ok(json!({"released":true}))
                }
                "status" => {
                    allowed(args, &["op"])?;
                    let (holder, remaining) = authority.status();
                    Ok(
                        json!({"holder":holder,"remaining_seconds":remaining.as_secs_f64(),"epoch":epoch}),
                    )
                }
                _ => Err(Error::new("invalid_arguments", "Unknown lease operation")),
            };
        }
        if name == "hoi4_special_projects_panel" {
            allowed(args, &["facility_id", "rules", "lease_token"])?;
            let facility = special_projects::facility_arg(args.get("facility_id"))?;
            let with_rules = special_projects::rules_arg(args.get("rules"))?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            self.keys.preflight()?;
            return special_projects::panel(process, &self.keys, facility, with_rules);
        }
        if name == "hoi4_special_project_assign" {
            allowed(
                args,
                &[
                    "facility_id",
                    "scientist",
                    "project",
                    "request_id",
                    "lease_token",
                ],
            )?;
            let facility = special_projects::facility_arg(args.get("facility_id"))?
                .ok_or_else(|| Error::new("invalid_arguments", "facility_id is required"))?;
            let scientist = special_projects::name_arg(args.get("scientist"), "scientist")?;
            let project = special_projects::key_arg(args.get("project"))?;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let expected = guard.observe_context(process)?;
            let epoch = guard.epoch.clone();
            let lease_end = authority.authorize(token, &epoch)?;
            let fence = action_fence::Fence {
                process,
                epoch: &epoch,
                expected: &expected,
                authority,
                token,
                until: (Instant::now() + Duration::from_secs(12)).min(lease_end),
                stop: None,
            };
            let check_input = || fence.input_check();
            let bounded_keys = self.keys.guarded(&fence, &check_input)?;
            let process: &dyn Process = &fence;
            let retry = actions.receipt_exists(id);
            let before = special_projects::snapshot(process)?;
            let plan = if retry {
                // A retry only settles or replays its receipt; it never refuses on new state.
                special_projects::plan(&before, facility, scientist, project).ok()
            } else {
                let plan = special_projects::plan(&before, facility, scientist, project)?;
                bounded_keys.preflight()?;
                Some(plan)
            };
            let action = json!({
                "kind": "special_project_assign",
                "facility_id": facility,
                "scientist": scientist,
                "project": project,
            });
            let slots_before = special_projects::Slots::of(&before, facility);
            let (want_scientist, want_project) =
                special_projects::wanted(&before, scientist, project);
            let keys = &bounded_keys;
            let mut settle = || {
                let now = special_projects::slots(process, facility)?;
                let detail = json!({
                    "facility_id": facility,
                    "scientist_character_id": now.and_then(|s| s.scientist),
                    "project_template_id": now.and_then(|s| s.project),
                });
                let verdict = settle::special_project(
                    slots_before.unwrap_or(special_projects::Slots::EMPTY),
                    now,
                    want_scientist,
                    want_project,
                );
                Ok(if retry && verdict == settle::Verdict::Unchanged {
                    // A retry's own before-snapshot proves nothing about the first attempt.
                    journal::Outcome::Diverged(detail)
                } else {
                    verdict.outcome(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let plan = plan.as_ref().ok_or_else(|| {
                    Error::new("action_uncertain", "A retried request is never dispatched again")
                })?;
                special_projects::assign(process, keys, plan)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_special_project_stop" || name == "hoi4_special_project_unassign" {
            let stopping = name == "hoi4_special_project_stop";
            allowed(args, &["facility_id", "request_id", "lease_token"])?;
            let facility = special_projects::facility_arg(args.get("facility_id"))?
                .ok_or_else(|| Error::new("invalid_arguments", "facility_id is required"))?;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let expected = guard.observe_context(process)?;
            let epoch = guard.epoch.clone();
            let lease_end = authority.authorize(token, &epoch)?;
            let fence = action_fence::Fence {
                process,
                epoch: &epoch,
                expected: &expected,
                authority,
                token,
                until: (Instant::now() + Duration::from_secs(12)).min(lease_end),
                stop: None,
            };
            let check_input = || fence.input_check();
            let bounded_keys = self.keys.guarded(&fence, &check_input)?;
            let process: &dyn Process = &fence;
            let retry = actions.receipt_exists(id);
            let before = special_projects::snapshot(process)?;
            if !retry {
                if stopping {
                    special_projects::plan_stop(&before, facility)?;
                } else {
                    special_projects::plan_unassign(&before, facility)?;
                }
                bounded_keys.preflight()?;
            }
            let slots_before = special_projects::Slots::of(&before, facility);
            let action = json!({
                "kind": if stopping { "special_project_stop" } else { "special_project_unassign" },
                "facility_id": facility,
            });
            let keys = &bounded_keys;
            let mut settle = || {
                if stopping {
                    let now = special_projects::stop_view(process, facility)?;
                    let detail = json!({
                        "facility_id": facility,
                        "project_template_id": now.and_then(|n| n.0),
                        "stopping": now.map(|n| n.1),
                    });
                    let verdict = settle::special_stop(slots_before.and_then(|s| s.project), now);
                    Ok(if retry && verdict == settle::Verdict::Unchanged {
                        journal::Outcome::Diverged(detail)
                    } else {
                        verdict.outcome(detail)
                    })
                } else {
                    let now = special_projects::slots(process, facility)?;
                    let detail = json!({
                        "facility_id": facility,
                        "scientist_character_id": now.and_then(|s| s.scientist),
                    });
                    let verdict =
                        settle::special_unassign(slots_before.and_then(|s| s.scientist), now);
                    Ok(if retry && verdict == settle::Verdict::Unchanged {
                        journal::Outcome::Diverged(detail)
                    } else {
                        verdict.outcome(detail)
                    })
                }
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                if retry {
                    return Err(Error::new(
                        "action_uncertain",
                        "A retried request is never dispatched again",
                    ));
                }
                if stopping {
                    special_projects::stop(process, keys, facility)
                } else {
                    special_projects::unassign(process, keys, facility)
                }
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_special_project_recruit" {
            allowed(args, &["scientist", "request_id", "lease_token"])?;
            let scientist = special_projects::name_arg(args.get("scientist"), "scientist")?
                .ok_or_else(|| Error::new("invalid_arguments", "scientist is required"))?;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let expected = guard.observe_context(process)?;
            let epoch = guard.epoch.clone();
            let lease_end = authority.authorize(token, &epoch)?;
            let fence = action_fence::Fence {
                process,
                epoch: &epoch,
                expected: &expected,
                authority,
                token,
                until: (Instant::now() + Duration::from_secs(12)).min(lease_end),
                stop: None,
            };
            let check_input = || fence.input_check();
            let bounded_keys = self.keys.guarded(&fence, &check_input)?;
            let process: &dyn Process = &fence;
            let retry = actions.receipt_exists(id);
            if !retry {
                let before = special_projects::snapshot(process)?;
                // Political power is the top-bar text; unreadable means no pre-check on it.
                let power = country_status::status(process).ok().and_then(|v| {
                    v["fields"]["political_power"]["value"].as_f64()
                });
                special_projects::plan_recruit(&before, scientist, power)?;
                bounded_keys.preflight()?;
            }
            let action = json!({"kind": "special_project_recruit", "scientist": scientist});
            let keys = &bounded_keys;
            let mut settle = || {
                let (owned, offered) = special_projects::recruit_view(process, scientist)?;
                let detail = json!({"scientist": scientist, "owned": owned, "offered": offered});
                let verdict = settle::special_recruit(owned, offered);
                Ok(if retry && verdict == settle::Verdict::Unchanged {
                    journal::Outcome::Diverged(detail)
                } else {
                    verdict.outcome(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                if retry {
                    return Err(Error::new(
                        "action_uncertain",
                        "A retried request is never dispatched again",
                    ));
                }
                special_projects::recruit(process, keys, scientist)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_research_start" {
            allowed(args, &["technology", "slot", "request_id", "lease_token"])?;
            let key = string(args, "technology")?;
            if key.is_empty()
                || key.len() > 128
                || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(Error::new("invalid_arguments", "Invalid technology key"));
            }
            let slot = args
                .get("slot")
                .and_then(Value::as_u64)
                .filter(|v| *v <= 15)
                .ok_or_else(|| Error::new("invalid_arguments", "slot must be 0..15"))?
                as usize;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let expected = guard.observe_context(process)?;
            let epoch = guard.epoch.clone();
            let lease_end = authority.authorize(token, &epoch)?;
            let fence = action_fence::Fence {
                process,
                epoch: &epoch,
                expected: &expected,
                authority,
                token,
                until: (Instant::now() + Duration::from_secs(12)).min(lease_end),
                stop: None,
            };
            let check_input = || fence.input_check();
            let bounded_keys = self.keys.guarded(&fence, &check_input)?;
            let process: &dyn Process = &fence;
            let action = json!({"kind":"research_start","technology":key,"slot":slot});
            let before = research::assignments(process)?;
            if actions.receipt_exists(id) {
                // A retry only settles or replays its receipt; it never refuses on new state.
            } else {
                research::check(&before, key, slot)?;
                bounded_keys.preflight()?;
            }
            let keys = &bounded_keys;
            let mut settle = || {
                let now = research::assignments(process)?;
                let detail = json!({"slots":now.slots});
                Ok(
                    match research::settle(&before.slots, &now.slots, slot, key) {
                        research::Settled::Reached => journal::Outcome::Reached(detail),
                        research::Settled::Unchanged => journal::Outcome::Unchanged(detail),
                        research::Settled::Unknown => journal::Outcome::Unknown,
                    },
                )
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                research::start(process, keys, key, slot)
            };
            let receipt =
                actions.execute_with(id, &epoch, action, &mut settle, &mut dispatch, true)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_production_add" || name == "hoi4_production_set" {
            let adding = name == "hoi4_production_add";
            if adding {
                allowed(
                    args,
                    &["equipment", "factories", "request_id", "lease_token"],
                )?;
            } else {
                allowed(
                    args,
                    &[
                        "line",
                        "equipment",
                        "factories",
                        "request_id",
                        "lease_token",
                    ],
                )?;
            }
            let key = string(args, "equipment")?;
            if key.is_empty()
                || key.len() > 128
                || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(Error::new("invalid_arguments", "Invalid equipment key"));
            }
            let factories = args
                .get("factories")
                .and_then(Value::as_i64)
                .filter(|v| (i64::from(adding)..=150).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "factories out of range"))?
                as i32;
            let index = if adding {
                None
            } else {
                Some(
                    args.get("line")
                        .and_then(Value::as_u64)
                        .filter(|v| *v < 2048)
                        .ok_or_else(|| Error::new("invalid_arguments", "line must be 0..2047"))?
                        as usize,
                )
            };
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let expected = guard.observe_context(process)?;
            let epoch = guard.epoch.clone();
            let lease_end = authority.authorize(token, &epoch)?;
            let fence = action_fence::Fence {
                process,
                epoch: &epoch,
                expected: &expected,
                authority,
                token,
                until: (Instant::now() + Duration::from_secs(12)).min(lease_end),
                stop: None,
            };
            let check_input = || fence.input_check();
            let bounded_keys = self.keys.guarded(&fence, &check_input)?;
            let process: &dyn Process = &fence;
            let action = json!({"kind":name.trim_start_matches("hoi4_"),"equipment":key,"factories":factories,"line":index});
            let before = production::lines(process)?;
            let retry = actions.receipt_exists(id);
            if !retry {
                if let Some(index) = index {
                    if before.get(index).and_then(|l| l.key.as_deref()) != Some(key) {
                        return Err(Error::new(
                            "invalid_target",
                            "No own line with this index and equipment",
                        ));
                    }
                }
                bounded_keys.preflight()?;
            }
            let keys = &bounded_keys;
            let mut settle = || {
                let now = production::lines(process)?;
                let detail = json!({"lines":now.iter().map(|l| json!({"equipment":l.key,"factories":l.requested})).collect::<Vec<_>>()});
                let with_key: Vec<_> = now
                    .iter()
                    .filter(|l| l.key.as_deref() == Some(key))
                    .collect();
                if retry {
                    // No pre-dispatch snapshot: settle only from what is observable now.
                    return Ok(if with_key.iter().any(|l| l.requested == factories) {
                        journal::Outcome::Reached(detail)
                    } else if index.is_some() || !with_key.is_empty() {
                        journal::Outcome::Diverged(detail)
                    } else {
                        journal::Outcome::Unchanged(detail)
                    });
                }
                Ok(match index {
                    None => {
                        let added: Vec<_> = now
                            .iter()
                            .filter(|l| !before.iter().any(|b| b.handle == l.handle))
                            .collect();
                        if added
                            .iter()
                            .any(|l| l.key.as_deref() == Some(key) && l.requested == factories)
                        {
                            journal::Outcome::Reached(detail)
                        } else if added.is_empty() {
                            journal::Outcome::Unchanged(detail)
                        } else {
                            journal::Outcome::Diverged(detail)
                        }
                    }
                    Some(index) => {
                        let line = &before[index];
                        match now.iter().find(|l| l.handle == line.handle) {
                            Some(l) if l.requested == factories => {
                                journal::Outcome::Reached(detail)
                            }
                            Some(l) if l.requested == line.requested => {
                                journal::Outcome::Unchanged(detail)
                            }
                            Some(_) => journal::Outcome::Diverged(detail),
                            None => journal::Outcome::Unknown,
                        }
                    }
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                match index {
                    None => production::add(process, keys, key, factories),
                    Some(index) => production::set(process, keys, index, key, factories),
                }
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_diplomacy" || name == "hoi4_diplomacy_act" {
            let act = name == "hoi4_diplomacy_act";
            if act {
                allowed(
                    args,
                    &[
                        "state_id",
                        "action",
                        "war_goal",
                        "states",
                        "request_id",
                        "lease_token",
                    ],
                )?;
            } else {
                allowed(args, &["state_id", "lease_token"])?;
            }
            let state = args
                .get("state_id")
                .and_then(Value::as_i64)
                .filter(|v| (1..4096).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "state_id must be 1..4095"))?
                as i32;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            self.keys.preflight()?;
            if !act {
                return diplomacy::actions(process, &self.keys, state);
            }
            let action_name = string(args, "action")?;
            let text_ok =
                |v: &str| !v.is_empty() && v.len() <= 128 && !v.chars().any(char::is_control);
            if !text_ok(action_name) {
                return Err(Error::new("invalid_arguments", "Invalid action name"));
            }
            let kind = match args.get("war_goal") {
                None => None,
                Some(v) => Some(
                    v.as_str()
                        .filter(|v| text_ok(v))
                        .ok_or_else(|| Error::new("invalid_arguments", "Invalid war_goal"))?,
                ),
            };
            let states = match args.get("states") {
                None => Vec::new(),
                Some(v) => v
                    .as_array()
                    .filter(|a| a.len() <= 5)
                    .ok_or_else(|| {
                        Error::new("invalid_arguments", "states must list up to 5 names")
                    })?
                    .iter()
                    .map(|s| {
                        s.as_str()
                            .filter(|s| text_ok(s))
                            .map(str::to_string)
                            .ok_or_else(|| Error::new("invalid_arguments", "Invalid state name"))
                    })
                    .collect::<Result<Vec<_>>>()?,
            };
            let id = string(args, "request_id")?;
            let action = json!({"kind":"diplomacy_act","state_id":state,"action":action_name,"war_goal":kind,"states":states});
            let keys = &self.keys;
            // The dispatch verifies the change itself; its result decides.
            let verified = std::cell::Cell::new(false);
            let mut settle = || {
                let now = diplomacy::current(process)?;
                Ok(if verified.get() {
                    journal::Outcome::Reached(json!({"actions":now}))
                } else {
                    journal::Outcome::Unknown
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                diplomacy::act(process, keys, state, action_name, kind, &states)?;
                verified.set(true);
                Ok(())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_event_choose" {
            allowed(args, &["event", "option", "request_id", "lease_token"])?;
            let event = string(args, "event")?;
            if event.is_empty() || event.len() > 128 || event.chars().any(char::is_control) {
                return Err(Error::new("invalid_arguments", "Invalid event id"));
            }
            let option = args
                .get("option")
                .and_then(Value::as_u64)
                .filter(|v| *v < 8)
                .ok_or_else(|| Error::new("invalid_arguments", "option must be 0..7"))?
                as usize;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let retry = actions.receipt_exists(id);
            if !retry {
                if !events::is_open(process, event)? {
                    return Err(Error::new(
                        "invalid_target",
                        format!("Event {event} is not open"),
                    ));
                }
                self.keys.preflight()?;
            }
            let action = json!({"kind":"event_choose","event":event,"option":option});
            let keys = &self.keys;
            let mut settle = || {
                let open = events::is_open(process, event)?;
                let detail = json!({"event":event,"open":open});
                Ok(if !open {
                    journal::Outcome::Reached(detail)
                } else if retry {
                    journal::Outcome::Diverged(detail)
                } else {
                    journal::Outcome::Unchanged(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                events::choose(process, keys, event, option)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_air_wings" {
            allowed(args, &["lease_token"])?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            self.keys.preflight()?;
            return air::list(process, &self.keys);
        }
        if name == "hoi4_air_mission" {
            allowed(
                args,
                &["wing", "mission", "state_id", "request_id", "lease_token"],
            )?;
            let wing = string(args, "wing")?;
            if wing.is_empty() || wing.len() > 128 || wing.chars().any(char::is_control) {
                return Err(Error::new("invalid_arguments", "Invalid wing name"));
            }
            let mission = string(args, "mission")?;
            if !air::MISSIONS.iter().any(|(n, _)| *n == mission) {
                return Err(Error::new("invalid_arguments", "Unknown mission"));
            }
            let state = args
                .get("state_id")
                .and_then(Value::as_i64)
                .filter(|v| (1..4096).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "state_id must be 1..4095"))?
                as i32;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let retry = actions.receipt_exists(id);
            if !retry {
                self.keys.preflight()?;
            }
            let action =
                json!({"kind":"air_mission","wing":wing,"mission":mission,"state_id":state});
            let keys = &self.keys;
            let verified = std::cell::Cell::new(false);
            let mut settle = || {
                let now = air::wing_state(process, wing)?;
                Ok(if verified.get() {
                    journal::Outcome::Reached(json!({"wing":now}))
                } else {
                    journal::Outcome::Unknown
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                air::assign(process, keys, wing, mission, state)?;
                verified.set(true);
                Ok(())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_state_slots" {
            allowed(args, &["state_ids", "lease_token"])?;
            let ids: Vec<i32> = args
                .get("state_ids")
                .and_then(Value::as_array)
                .filter(|a| (1..=12).contains(&a.len()))
                .ok_or_else(|| Error::new("invalid_arguments", "state_ids must list 1..12 ids"))?
                .iter()
                .map(|v| {
                    v.as_i64()
                        .filter(|v| (1..4096).contains(v))
                        .map(|v| v as i32)
                        .ok_or_else(|| Error::new("invalid_arguments", "state id must be 1..4095"))
                })
                .collect::<Result<_>>()?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            self.keys.preflight()?;
            return stateslots::read_states(process, &self.keys, &ids);
        }
        if name == "hoi4_army_split" {
            allowed(args, &["army", "count", "request_id", "lease_token"])?;
            let army = string(args, "army")?;
            if army.is_empty() || army.len() > 128 || army.chars().any(char::is_control) {
                return Err(Error::new("invalid_arguments", "Invalid army name"));
            }
            let count = args
                .get("count")
                .and_then(Value::as_u64)
                .filter(|v| (1..=19).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "count must be 1..19"))?
                as usize;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let retry = actions.receipt_exists(id);
            if !retry {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"army_split","army":army,"count":count});
            let keys = &self.keys;
            let verified = std::cell::Cell::new(false);
            let before = settle::Roster::from_view(&armies::armies(process)?);
            let mut settle = || {
                let now = armies::armies(process)?;
                Ok(if verified.get() {
                    journal::Outcome::Reached(now)
                } else {
                    let verdict =
                        settle::army_split(&before, &settle::Roster::from_view(&now), army, count);
                    verdict.outcome(now)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                armies::split(process, keys, army, count)?;
                verified.set(true);
                Ok(())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_army_group_create" {
            allowed(args, &["armies", "request_id", "lease_token"])?;
            let names: Vec<String> = args
                .get("armies")
                .and_then(Value::as_array)
                .filter(|a| (2..=12).contains(&a.len()))
                .ok_or_else(|| Error::new("invalid_arguments", "armies must list 2..12 names"))?
                .iter()
                .map(|v| {
                    v.as_str()
                        .filter(|s| {
                            !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)
                        })
                        .map(str::to_string)
                        .ok_or_else(|| Error::new("invalid_arguments", "Invalid army name"))
                })
                .collect::<Result<_>>()?;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let retry = actions.receipt_exists(id);
            if !retry {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"army_group_create","armies":names});
            let keys = &self.keys;
            let verified = std::cell::Cell::new(false);
            let before = settle::Roster::from_view(&armies::armies(process)?);
            let mut settle = || {
                let now = armies::armies(process)?;
                Ok(if verified.get() {
                    journal::Outcome::Reached(now)
                } else {
                    let verdict =
                        settle::group_create(&before, &settle::Roster::from_view(&now), &names);
                    verdict.outcome(now)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                armies::create_group(process, keys, &names)?;
                verified.set(true);
                Ok(())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_army_commander" {
            allowed(
                args,
                &["army", "general", "reassign", "request_id", "lease_token"],
            )?;
            let army = string(args, "army")?;
            let general = string(args, "general")?;
            let reassign = args
                .get("reassign")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            for v in [army, general] {
                if v.is_empty() || v.len() > 128 || v.chars().any(char::is_control) {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Invalid army or general name",
                    ));
                }
            }
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let retry = actions.receipt_exists(id);
            if !retry {
                self.keys.preflight()?;
                commanders::precheck(process, army, general, reassign)?;
            }
            let action = json!({"kind":"army_commander","army":army,"general":general});
            let keys = &self.keys;
            let mut settle = || {
                let now = ui::read(process, |r, i| armies::commander(r, i, army))?;
                let detail = json!({"army":army,"commander":now});
                Ok(
                    if now
                        .as_deref()
                        .is_some_and(|c| c.eq_ignore_ascii_case(general))
                    {
                        journal::Outcome::Reached(detail)
                    } else if retry {
                        journal::Outcome::Diverged(detail)
                    } else {
                        journal::Outcome::Unchanged(detail)
                    },
                )
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                armies::assign_commander(process, keys, army, general)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_army_offensive" {
            allowed(args, &["army", "state_id", "request_id", "lease_token"])?;
            let army = string(args, "army")?;
            if army.is_empty() || army.len() > 128 || army.chars().any(char::is_control) {
                return Err(Error::new("invalid_arguments", "Invalid army name"));
            }
            let state = args
                .get("state_id")
                .and_then(Value::as_i64)
                .filter(|v| (1..4096).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "state_id must be 1..4095"))?
                as i32;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let observed = guard.observe(process)?;
            let epoch = observed.epoch;
            authority.authorize(token, &epoch)?;
            let game_dir = mapdata::game_dir(&epoch.game.exe)?;
            let nodes = || ui::read(process, |r, i| armies::plan_nodes(r, i, army));
            let before = nodes()?;
            let retry = actions.receipt_exists(id);
            if !retry {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"army_offensive","army":army,"state_id":state});
            let keys = &self.keys;
            let mut settle = || {
                let now = nodes()?;
                let detail = json!({"army":army,"plan_orders":now});
                Ok(if retry {
                    journal::Outcome::Diverged(detail)
                } else {
                    settle::order_growth(before, now).outcome(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                armies::offensive(process, keys, &game_dir, army, state)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_army_move" {
            allowed(args, &["army", "state_id", "request_id", "lease_token"])?;
            let army = string(args, "army")?;
            let state = args
                .get("state_id")
                .and_then(Value::as_i64)
                .filter(|v| (1..4096).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "state_id must be 1..4095"))?
                as i32;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let observed = guard.observe(process)?;
            let epoch = observed.epoch;
            authority.authorize(token, &epoch)?;
            let game_dir = mapdata::game_dir(&epoch.game.exe)?;
            let wanted = mapdata::state_provinces(&game_dir, state)?;
            let heading = || ui::read(process, |r, i| armies::heading_for(r, i, army, &wanted));
            let retry = actions.receipt_exists(id);
            if !retry {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"army_move","army":army,"state_id":state});
            let keys = &self.keys;
            let mut settle = || {
                let (now, total) = heading()?;
                let detail =
                    json!({"army":army,"state_id":state,"divisions_heading":now,"divisions":total});
                Ok(if retry {
                    journal::Outcome::Diverged(detail)
                } else if now > 0 {
                    journal::Outcome::Reached(detail)
                } else {
                    journal::Outcome::Unchanged(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                armies::move_to(process, keys, army, state)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_army_plan" {
            allowed(args, &["army", "execute", "request_id", "lease_token"])?;
            let army = string(args, "army")?;
            if army.is_empty() || army.len() > 128 || army.chars().any(char::is_control) {
                return Err(Error::new("invalid_arguments", "Invalid army name"));
            }
            let run = args
                .get("execute")
                .and_then(Value::as_bool)
                .ok_or_else(|| Error::new("invalid_arguments", "execute must be a boolean"))?;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let state = || ui::read(process, |r, i| armies::executing(r, i, army));
            let retry = actions.receipt_exists(id);
            if !retry {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"army_plan","army":army,"execute":run});
            let keys = &self.keys;
            let mut settle = || {
                let now = state()?;
                let detail = json!({"army":army,"executing":now});
                Ok(if now == run {
                    journal::Outcome::Reached(detail)
                } else if retry {
                    journal::Outcome::Diverged(detail)
                } else {
                    journal::Outcome::Unchanged(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                armies::execute(process, keys, army, run)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_army_front" {
            allowed(args, &["army", "state_id", "request_id", "lease_token"])?;
            let army = string(args, "army")?;
            if army.is_empty() || army.len() > 128 || army.chars().any(char::is_control) {
                return Err(Error::new("invalid_arguments", "Invalid army name"));
            }
            let state = args
                .get("state_id")
                .and_then(Value::as_i64)
                .filter(|v| (1..4096).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "state_id must be 1..4095"))?
                as i32;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let orders = || ui::read(process, |r, i| armies::orders(r, i, army));
            let before = orders()?;
            let retry = actions.receipt_exists(id);
            if !retry {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"army_front","army":army,"state_id":state});
            let keys = &self.keys;
            let mut settle = || {
                let now = orders()?;
                let detail = json!({"army":army,"orders":now});
                Ok(if retry {
                    journal::Outcome::Diverged(detail)
                } else {
                    settle::order_growth(before, now).outcome(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                armies::front(process, keys, army, state)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_army_create" {
            allowed(
                args,
                &["state_id", "absorb_existing", "request_id", "lease_token"],
            )?;
            let absorb = args
                .get("absorb_existing")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let state = args
                .get("state_id")
                .and_then(Value::as_i64)
                .filter(|v| (1..4096).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "state_id must be 1..4095"))?
                as i32;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let before = armies::armies(process)?;
            let before_roster = settle::Roster::from_view(&before);
            let retry = actions.receipt_exists(id);
            if !retry {
                armies::check_create(&before, state, absorb)?;
                self.keys.preflight()?;
            }
            let action = json!({"kind":"army_create","state_id":state});
            let keys = &self.keys;
            let mut settle = || {
                let now = armies::armies(process)?;
                Ok(if retry {
                    journal::Outcome::Diverged(now)
                } else {
                    // Identity only: movement, strength and plans drift at speed.
                    settle::army_create(&before_roster, &settle::Roster::from_view(&now))
                        .outcome(now)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                armies::create(process, keys, state, absorb)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_army_assign" {
            allowed(args, &["state_id", "army", "request_id", "lease_token"])?;
            let state = args
                .get("state_id")
                .and_then(Value::as_i64)
                .filter(|v| (1..4096).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "state_id must be 1..4095"))?
                as i32;
            let target = args.get("army").and_then(Value::as_str);
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let before = armies::armies(process)?;
            let before_roster = settle::Roster::from_view(&before);
            let retry = actions.receipt_exists(id);
            let plan = armies::plan_assign(&before, state, target, armies::ARMY_CAP)?;
            if !retry {
                armies::assign_preflight(&plan)?;
                armies::check_create(&before, state, false)?;
                self.keys.preflight()?;
            }
            let pool = armies::unassigned_ids(&before, state);
            let action = json!({"kind":"army_assign","state_id":state,"army":target});
            let keys = &self.keys;
            let mut settle = || {
                let now = armies::armies(process)?;
                let now_roster = settle::Roster::from_view(&now);
                Ok(if retry {
                    journal::Outcome::Diverged(now)
                } else if let armies::AssignMode::Absorb { army } = &plan.mode {
                    settle::army_assign(&before_roster, &now_roster, army, &pool, plan.take)
                        .outcome(now)
                } else {
                    settle::army_create(&before_roster, &now_roster).outcome(now)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                armies::assign(process, keys, &plan)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_train" {
            allowed(
                args,
                &[
                    "template",
                    "count",
                    "deploy_state_id",
                    "request_id",
                    "lease_token",
                ],
            )?;
            let deploy_state = args
                .get("deploy_state_id")
                .and_then(Value::as_i64)
                .filter(|v| (1..4096).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "deploy_state_id must be 1..4095"))?
                as i32;
            let template = string(args, "template")?;
            if template.is_empty() || template.len() > 128 || template.chars().any(char::is_control)
            {
                return Err(Error::new("invalid_arguments", "Invalid template name"));
            }
            let count = args
                .get("count")
                .and_then(Value::as_u64)
                .filter(|v| (1..=24).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "count must be 1..24"))?
                as usize;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let queued = || -> Result<usize> {
                process.begin_snapshot()?;
                let r = crate::memory::Reader::new(process);
                let n = deployment::queued(&r, process.image_base(), template)?;
                process.check()?;
                Ok(n)
            };
            let start = queued()?;
            let retry = actions.receipt_exists(id);
            if !retry {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"train","template":template,"count":count});
            let keys = &self.keys;
            let mut settle = || {
                let now = queued()?;
                let detail = json!({"template":template,"queued_lines":now});
                Ok(if retry {
                    journal::Outcome::Diverged(detail)
                } else {
                    settle::queue_growth(start as i64, now as i64, count as i64).outcome(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                deployment::train(process, keys, template, count)?;
                deployment::set_locations(process, keys, deploy_state).map(|_| ())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_deploy_locations" {
            allowed(args, &["state_id", "request_id", "lease_token"])?;
            let state = args
                .get("state_id")
                .and_then(Value::as_i64)
                .filter(|v| (1..4096).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "state_id must be 1..4095"))?
                as i32;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let unlocated = || ui::read(process, deployment::unlocated);
            let start = unlocated()?;
            let retry = actions.receipt_exists(id);
            if !retry {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"deploy_locations","state_id":state});
            let keys = &self.keys;
            let mut settle = || {
                let now = unlocated()?;
                let detail = json!({"lines_without_location":now});
                Ok(if retry && now != 0 {
                    journal::Outcome::Diverged(detail)
                } else {
                    settle::locations(start, now).outcome(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                deployment::set_locations(process, keys, state).map(|_| ())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_decisions_list" {
            allowed(args, &["lease_token"])?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            self.keys.preflight()?;
            return Ok(json!({"decisions":decisions::list(process, &self.keys)?}));
        }
        if name == "hoi4_decision_take" {
            allowed(args, &["decision", "request_id", "lease_token"])?;
            let key = string(args, "decision")?;
            if key.is_empty()
                || key.len() > 128
                || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(Error::new("invalid_arguments", "Invalid decision key"));
            }
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            if !actions.receipt_exists(id) {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"decision_take","decision":key});
            let keys = &self.keys;
            let pp_before = decisions::political_power(process)?;
            let retry = actions.receipt_exists(id);
            // The dispatch's own same-attempt proof (PP spent, timer or entry gone) decides:
            // political power rises on its own, so a later read cannot disprove it.
            let verified = std::cell::Cell::new(false);
            let mut settle = || {
                let rows = decisions::state(process)?;
                let mine: Vec<_> = rows.iter().filter(|r| r["decision"] == key).collect();
                let pp = decisions::political_power(process)?;
                let detail = json!({"decision_rows":mine,"political_power_raw":pp});
                let timer = mine.iter().any(|r| r["active_timer"] == true);
                Ok(if retry {
                    // No pre-dispatch evidence: report what is observable.
                    if timer {
                        journal::Outcome::Reached(detail)
                    } else {
                        journal::Outcome::Diverged(detail)
                    }
                } else {
                    settle::decision(verified.get(), timer, pp_before as i64, pp as i64)
                        .outcome(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                decisions::take(process, keys, key)?;
                verified.set(true);
                Ok(())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_politics_options" || name == "hoi4_politics_choose" {
            let choosing = name == "hoi4_politics_choose";
            if choosing {
                allowed(
                    args,
                    &["category", "slot", "idea", "request_id", "lease_token"],
                )?;
            } else {
                allowed(args, &["category", "slot", "lease_token"])?;
            }
            let category = string(args, "category")?;
            if !["government", "research_production", "military_staff"].contains(&category) {
                return Err(Error::new("invalid_arguments", "Unknown category"));
            }
            let index = args
                .get("slot")
                .and_then(Value::as_u64)
                .filter(|v| *v <= 7)
                .ok_or_else(|| Error::new("invalid_arguments", "slot must be 0..7"))?
                as usize;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let before = politics::current(process, category, index)?;
            if !choosing {
                self.keys.preflight()?;
                let options = politics::list(process, &self.keys, category, index)?;
                return Ok(
                    json!({"category":category,"slot":index,"current":before,"options":options}),
                );
            }
            let idea = string(args, "idea")?;
            if idea.is_empty()
                || idea.len() > 128
                || !idea.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(Error::new("invalid_arguments", "Invalid idea key"));
            }
            let id = string(args, "request_id")?;
            let retry = actions.receipt_exists(id);
            if !retry {
                if before.as_deref() == Some(idea) {
                    return Err(Error::new(
                        "invalid_target",
                        "The slot already holds this idea",
                    ));
                }
                self.keys.preflight()?;
            }
            let action =
                json!({"kind":"politics_choose","category":category,"slot":index,"idea":idea});
            let keys = &self.keys;
            let mut settle = || {
                let now = politics::current(process, category, index)?;
                let detail = json!({"current":now});
                Ok(if now.as_deref() == Some(idea) {
                    journal::Outcome::Reached(detail)
                } else if now == before {
                    journal::Outcome::Unchanged(detail)
                } else {
                    journal::Outcome::Diverged(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                politics::choose(process, keys, category, index, idea)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_construction_add" {
            allowed(
                args,
                &["building", "state_id", "count", "request_id", "lease_token"],
            )?;
            let key = string(args, "building")?;
            if key.is_empty()
                || key.len() > 64
                || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(Error::new("invalid_arguments", "Invalid building key"));
            }
            let state = args
                .get("state_id")
                .and_then(Value::as_i64)
                .filter(|v| (1..4096).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "state_id must be 1..4095"))?
                as i32;
            let count = args
                .get("count")
                .map(|v| v.as_i64().filter(|v| (1..=10).contains(v)))
                .unwrap_or(Some(1))
                .ok_or_else(|| Error::new("invalid_arguments", "count must be 1..10"))?;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let action =
                json!({"kind":"construction_add","building":key,"state_id":state,"count":count});
            let start = construction::queued(process, key, state)?;
            let retry = actions.receipt_exists(id);
            if !retry {
                self.keys.preflight()?;
            }
            let keys = &self.keys;
            let mut settle = || {
                let now = construction::queued(process, key, state)?;
                let detail = json!({"building":key,"state_id":state,"queued_in_state":now});
                Ok(if retry {
                    journal::Outcome::Diverged(detail)
                } else {
                    settle::queue_growth(start, now, count).outcome(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                construction::add(process, keys, key, state, count)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_focus_start" {
            allowed(args, &["focus", "request_id", "lease_token"])?;
            let key = string(args, "focus")?;
            if key.is_empty()
                || key.len() > 128
                || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(Error::new("invalid_arguments", "Invalid focus key"));
            }
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let expected = guard.observe_context(process)?;
            let epoch = guard.epoch.clone();
            let lease_end = authority.authorize(token, &epoch)?;
            let fence = action_fence::Fence {
                process,
                epoch: &epoch,
                expected: &expected,
                authority,
                token,
                // A cold focus tree needs ~3 s before the pan stage (live: 5 s expired mid-pan,
                // leaving an unsettleable Dispatching intent); fixed up front, never extended later.
                until: (Instant::now() + Duration::from_secs(12)).min(lease_end),
                stop: None,
            };
            let check_input = || fence.input_check();
            let bounded_keys = self.keys.guarded(&fence, &check_input)?;
            let process: &(dyn Process + Send + Sync) = &fence;
            let action = json!({"kind":"focus_start","focus":key});
            if !actions.receipt_exists(id) {
                focus::check(&focus::progress(process)?, key)?;
                bounded_keys.preflight()?;
            }
            let keys = &bounded_keys;
            let mut settle = || {
                let now = focus::progress(process)?;
                let detail = json!({"active":now.active});
                Ok(
                    settle::focus(now.active.as_deref(), now.completed().contains(key), key)
                        .outcome(detail),
                )
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                focus::start(process, keys, key, fence.until)
            };
            let receipt =
                actions.execute_with(id, &epoch, action, &mut settle, &mut dispatch, true)?;
            return Ok(json!({"receipt":receipt}));
        }
        allowed(args, &["paused", "speed", "request_id", "lease_token"])?;
        let id = string(args, "request_id")?;
        let token = string(args, "lease_token")?;
        let action = match (args.get("paused"), args.get("speed")) {
            (Some(v), None) => Action::SetPaused(
                v.as_bool()
                    .ok_or_else(|| Error::new("invalid_arguments", "Paused must be a boolean"))?,
            ),
            (None, Some(v)) => Action::SetSpeed(
                v.as_u64()
                    .filter(|v| *v <= 4)
                    .ok_or_else(|| Error::new("invalid_arguments", "Speed must be integer 0..4"))?
                    as u8,
            ),
            _ => {
                return Err(Error::new(
                    "invalid_arguments",
                    "Supply exactly one clock target",
                ))
            }
        };
        let epoch = guard.epoch.clone();
        let mut backend = ClockBackend {
            process,
            keys: &self.keys,
            guard,
            authority,
            token,
        };
        let receipt = journal.execute(id, &epoch, action, &mut backend, Duration::from_secs(10))?;
        Ok(json!({"receipt":receipt}))
    }
}

#[cfg(test)]
mod navigation_config_tests {
    use super::*;
    #[test]
    fn find_navigation_permission_is_explicit_and_disabled_by_default() {
        let minimal: Config =
            serde_json::from_value(json!({"controller":"fixture","player":"GER"})).unwrap();
        assert!(!minimal.construction_navigation);
        let enabled: Config = serde_json::from_value(
            json!({"controller":"fixture","player":"GER","construction_navigation":true}),
        )
        .unwrap();
        assert!(enabled.construction_navigation);
        assert!(serde_json::from_value::<Config>(
            json!({"controller":"fixture","player":"GER","construction_navigation":1})
        )
        .is_err());
    }
}

#[cfg(test)]
mod placement_deadline_tests {
    use super::*;
    #[test]
    fn generic_bound_accepts_later_epochs_but_rejects_invalid_wall_clocks() {
        let original = 4_000_000_000.;
        assert_eq!(
            qualification_remaining(true, Some(original), original - 180.).unwrap(),
            Some(Duration::from_secs(180))
        );
        assert!(qualification_remaining(true, Some(original), original - 181.).is_err());
        for wall in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert!(qualification_remaining(true, Some(original), wall).is_err());
        }
    }

    #[test]
    fn fixed_grant_bound_is_required_and_never_grows_with_a_later_bind() {
        let original = 1000.;
        assert_eq!(
            qualification_remaining(true, Some(original), original - 120.).unwrap(),
            Some(Duration::from_secs(120))
        );
        assert_eq!(
            qualification_remaining(true, Some(original), original - 1.).unwrap(),
            Some(Duration::from_secs(1))
        );
        for end in [
            None,
            Some(original - 1.),
            Some(f64::NAN),
            Some(f64::INFINITY),
            Some(original + 181.),
        ] {
            assert!(qualification_remaining(true, end, original).is_err());
        }
        assert!(qualification_remaining(true, Some(original), original).is_err());
        assert_eq!(
            qualification_remaining(false, None, original).unwrap(),
            None
        );
    }
}
