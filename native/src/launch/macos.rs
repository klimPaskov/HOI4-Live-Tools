//! Ordinary input for one owned macOS game: exclusive lease, durable
//! journals, CGEvent keys and clicks only while the owned window is front-most,
//! and fresh memory readback for every step. The clock keeps the Linux tool
//! contract; research starts are driven by live GUI geometry.
mod action_fence;
mod agency_act;
mod air;
mod airact;
mod airread;
mod airui;
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
mod design_edit;
mod design_ui;
mod designer;
mod designs;
mod diplomacy;
mod diplomacy_keys;
mod diplomacy_state;
mod trade;
mod peace;
mod doctrine_act;
mod doctrines;
mod events;
mod faction;
mod fleetmem;
mod fleetops;
mod focus;
mod front_plan;
pub(crate) use focus::input_probe as focus_input_probe;
mod input_gate;
mod intelligence;
mod invasion;
mod journal;
mod kit;
#[cfg(test)]
mod live_reads_navalair;
mod mapdata;
mod naval;
mod panelkit;
mod navalact;
mod navalui;
mod politics;
mod production;
mod research;
mod economy_detail;
mod resources;
#[cfg(test)]
mod live_reads_mgmt;
mod mgmt;
mod loc;
mod mio_act;
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
mod template_edit;
mod template_get;
mod units;
#[cfg(test)]
mod live_designer;
#[cfg(test)]
mod live_designs;
#[cfg(test)]
mod live_reads;
#[cfg(test)]
mod testmem;
mod ui;
mod ui_action;
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
            diplomacy_state::countries_tool(),
            json!({"name":"hoi4_peace_status","description":"Read-only: whether the peace conference window is open and, when it is, the texts it shows (current player, score, bidding phase, turns left). Without a conference it returns the route an ordinary player takes (actions list, states on the conference map, End turn). NOT validated live while a conference is open (no war was fought in the qualification game); no peace action tool exists yet.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
            trade::tool(),
            trade::set_tool(),
            naval::fleets_schema(),
            json!({"name":"hoi4_fleet_mission","description":"Give one own task force a mission with the game's ordinary mission buttons (Strategic Navy map mode F2, fleet card, the task force entry's Select button, the mission button; the map mode is restored afterwards). Missions: patrol, strike_force, convoy_raiding, convoy_escort, mine_laying, mine_sweeping, training, invasion_support (all operate in the fleet's assigned sea regions; add regions with hoi4_fleet_region first) and hold (cancel the mission and sail to the closest port). Typed refusals before any input: task_force_unknown, task_force_ambiguous, nothing_to_change, no_assigned_region, reserve_fleet; found while driving: mission_unavailable (the game disables the button, e.g. mine missions without the technology). Verified from the task force's mission code in memory (hold 0, patrol 1, strike force 2, convoy raiding 3, convoy escort 4, training 7, invasion support 9 read back live). One mission per call; the ships sail when the game runs.","inputSchema":{"type":"object","properties":{"task_force":{"type":"string","maxLength":128},"mission":{"type":"string"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["task_force","mission","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_fleet_region","description":"Add a sea region to a fleet's area of operation (or remove it with remove:true) with ordinary input: Strategic Navy map mode, the Find view centres `state_id` (a coastal state next to the sea), left clicks on the map read the sea region names from the game's region view until `region` (exact display name, e.g. 'Eastern North Sea', 'Danish Belts') is found, then the fleet card and a right click on that region (remove: the remove-region button first). Typed refusals: fleet_unknown, already_assigned, region_not_assigned, reserve_fleet; region_not_found lists the sea regions seen (nothing is ordered then); mission_unavailable when the game disables the remove button. The fleet's task forces patrol/escort/raid in this area. Verified from the fleet's region list in memory (hoi4_fleets assigned_regions).","inputSchema":{"type":"object","properties":{"fleet":{"type":"string","maxLength":128},"region":{"type":"string","maxLength":96},"state_id":{"type":"integer","minimum":1,"maximum":4095},"remove":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["fleet","region","state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_fleet_admiral","description":"Put an admiral in command of an own fleet (or unassign with unassign:true) through the fleet panel's 'Select Commander' list: Strategic Navy map mode, the fleet card, the commander portrait, the admiral's row (scrolled into view), a confirmation when he leads another fleet. With list:true it only opens the list and returns the admirals it offers (name, skill) without assigning. Typed refusals before input: fleet_unknown, reserve_fleet, nothing_to_change; unknown_commander lists the names offered. Verified from the fleet's leader in memory. The admirals' skills, traits and rank are the list's own; an admiral who leads another fleet is moved (the game's confirmation is accepted).","inputSchema":{"type":"object","properties":{"fleet":{"type":"string","maxLength":128},"admiral":{"type":"string","maxLength":96},"unassign":{"type":"boolean"},"list":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["fleet","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_naval_production_options","description":"Read-only (memory, no input): the ship and convoy designs the Production panel can build (not decommissioned) with their equipment keys and existing lines, and the own naval production lines with their line index and dockyard count. Use the equipment key and optional design id with hoi4_production_add; ship and convoy lines are added through the Production panel's naval category.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
            json!({"name":"hoi4_fleet_create","description":"Found a new fleet from one own task force with the game's ordinary controls: Strategic Navy map mode, the task force's fleet card, its Select button, then the bottom bar's '+' (new fleet) card; the new fleet holds exactly that task force and gets the game's default name (see hoi4_fleets). Typed refusals before input: task_force_unknown, task_force_ambiguous, nothing_to_change (already alone in its fleet), reserve_fleet. Verified from the fleet list in memory (one more fleet, holding only that task force). Use hoi4_fleet_mission / hoi4_fleet_region / hoi4_fleet_admiral on the new fleet afterwards.","inputSchema":{"type":"object","properties":{"task_force":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["task_force","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_navy_ui","description":"DEBUG ONLY (never listed to a player): dump the naval UI tree or send one ordinary input.","inputSchema":{"type":"object","properties":{"op":{"type":"string"},"root":{"type":"string"},"path":{"type":"string"},"depth":{"type":"integer"},"index":{"type":"integer"},"items":{"type":"boolean"},"x":{"type":"number"},"y":{"type":"number"},"code":{"type":"integer"},"letter":{"type":"string"},"pixels":{"type":"integer"},"lease_token":{"type":"string"}},"required":["op","lease_token"],"additionalProperties":false}}),
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
            json!({"name":"hoi4_peace_options","description":"Read the open peace conference from its window: header (current player, own remaining points = current_score, world tension, bidding phase, turn), which controls are usable (demand kinds take_state/liberate/puppet/force_government/take_navy, end turn, done), participant, available-action and bid lists as the window shows them, and for the asked `states` their owner and occupier. Costs are not stored: a state's tooltip shows them and hoi4_peace_take reports each state's cost as the score drop. No lease, no input. validated:false (no conference was arranged live; procedure in docs/PLAYBOOK_TOOL_GAPS.md).","inputSchema":{"type":"object","properties":{"states":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4095},"maxItems":16}},"additionalProperties":false}}),
            json!({"name":"hoi4_peace_take","description":"In the open peace conference, choose the demand kind (default Take State; puppet:true = Puppet; release:true = Liberate) and add each listed state by centring it with Find and clicking it, verifying each through the drop of the window's score; reports each state's cost. Refusals before input: no_conference_open, round_not_ready, action_not_available, unknown_state, state_not_offered (own state), cost_exceeds_points. `max_cost` stops after a state dearer than it. Nothing is submitted. validated:false.","inputSchema":{"type":"object","properties":{"states":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4095},"minItems":1,"maxItems":8},"puppet":{"type":"boolean"},"release":{"type":"boolean"},"max_cost":{"type":"number","minimum":0},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["states","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_peace_submit","description":"Submit Demands for the round (the conference's end-turn button; one submission per round under the blind-bid rules). Refused before input with round_not_ready or no_conference_open. Verified when the header (phase, turn, score of the turn) changes or the window closes. validated:false.","inputSchema":{"type":"object","properties":{"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_peace_exit","description":"Leave the peace conference (the Confirm and Exit / done button and the OK of its confirmation); accepted bids persist. Refused before input with exit_not_available or no_conference_open. Verified when the conference window closes. validated:false.","inputSchema":{"type":"object","properties":{"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_plan_run","description":"Carry out the standing plan for up to `seconds` (default 60): answer news and single-option events, fill empty research slots and the focus slot from the plan's lists, make planned politics picks and decisions, keep the training queue, put idle military factories on the priority lines and spend idle civilian factories on planned construction. Every step is an ordinary verified action under the given lease; the game is never paused. Returns early when a choice event needs the model or a peace conference opens. Use it instead of hoi4_wait_for while nothing else needs attention.","inputSchema":{"type":"object","properties":{"seconds":{"type":"integer","minimum":1,"maximum":300},"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_input_lease","description":"Exclusive configured-controller lease for ordinary macOS input. Acquire uses holder; renew/release use only lease_token. Status takes only op. Observations do not need a lease.","inputSchema":{"type":"object","oneOf":[
                {"properties":{"op":{"const":"acquire"},"holder":{"type":"string"},"seconds":{"type":"integer","minimum":30,"maximum":900}},"required":["op","holder"],"additionalProperties":false},
                {"properties":{"op":{"const":"renew"},"lease_token":{"type":"string"},"seconds":{"type":"integer","minimum":30,"maximum":900}},"required":["op","lease_token"],"additionalProperties":false},
                {"properties":{"op":{"const":"release"},"lease_token":{"type":"string"}},"required":["op","lease_token"],"additionalProperties":false},
                {"properties":{"op":{"const":"status"}},"required":["op"],"additionalProperties":false}]}}),
            json!({"name":"hoi4_research_start","description":"Start an own technology in an empty research slot with ordinary input (research panel, slot, folder tab, tile, Research), each click located from the live GUI and each stage verified from memory; succeeds only when the slot holds the technology. Refuses researched (code already_researched), already-assigned or busy-slot targets before any input. A tile scrolled out of view is refused. Stable request_id; an unresolved dispatch is settled only by retrying its own request_id.","inputSchema":{"type":"object","properties":{"technology":{"type":"string","maxLength":128},"slot":{"type":"integer","minimum":0,"maximum":15},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["technology","slot","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_focus_start","description":"Start an own national focus with ordinary input (politics panel, focus button, drag-panning the tree until the node is in view, the node, Start), each target located from the live GUI and each stage verified from memory; succeeds only when own focus progress names the focus as active. Refuses completed or already-active focuses, or any while another focus is active, before input. Typed errors: focus_not_available (the detail window opened but the game disables Start: requirements unmet or an exclusive focus taken; Start was not clicked), focus_detail_not_opened (the node click was sent, no detail window read; re-observe before retrying), ui_blocked (names the notice pop-up classes and open events still shown). Stable request_id; an unresolved dispatch is settled only by retrying its own request_id.","inputSchema":{"type":"object","properties":{"focus":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["focus","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_production_add","description":"Add one own military or naval production line for a base equipment type (e.g. infantry_equipment, artillery_equipment, light_tank_equipment, fighter_equipment) with ordinary input: production panel, category button, the design in the selection window, then +/- until the line requests the given factories. Verified from the own line collection. Refuses equipment with no producible design. Optional `design` (a design id from hoi4_designs / hoi4_design_create, same equipment type) picks that exact variant in the selection window instead of the first listed one. Ship hulls (ship_hull_*) and convoys (convoy_*) are added through the naval category; see hoi4_naval_production_options.","inputSchema":{"type":"object","properties":{"equipment":{"type":"string","maxLength":128},"factories":{"type":"integer","minimum":1,"maximum":150},"design":{"type":"integer","minimum":1},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["equipment","factories","request_id","lease_token"],"additionalProperties":false}}),
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
            json!({"name":"hoi4_diplomacy","description":"Open the diplomacy view for a country and list the offered actions with canonical key, enabled state, cost and whether the AI would refuse (changes nothing in the game). Address the country by `country` (three-letter tag such as POL; the view is pointed at one of its states with the Find view) or by `state_id` of one of its states; `countries` (up to 6 tags) lists several. With `action` (a canonical key such as justify_war_goal, declare_war, guarantee, or an exact row name) it previews what the action's dialog shows (the cost, justification time and options the game displays) and then cancels without sending: for justify_war_goal / add_war_goal add `war_goal` (a type from war_goal_types, e.g. Conquer) and `states` (up to 5 names from states_offered) to read the game's cost text for that exact goal. Read the countries and their relations with hoi4_diplomacy_countries.","inputSchema":{"type":"object","properties":{"country":{"type":"string","pattern":"^[A-Z0-9]{3}$"},"countries":{"type":"array","items":{"type":"string","pattern":"^[A-Z0-9]{3}$"},"maxItems":6},"state_id":{"type":"integer","minimum":1,"maximum":4095},"action":{"type":"string","maxLength":128},"war_goal":{"type":"string","maxLength":128},"states":{"type":"array","items":{"type":"string"},"maxItems":5},"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_diplomacy_act","description":"Take ONE diplomatic action toward a country with the ordinary diplomacy view. Address it by `country` (tag, e.g. POL) or `state_id`. `action` is a canonical key (improve_relations, guarantee, revoke_guarantee, non_aggression_pact, ask_military_access, offer_military_access, revoke_military_access, ask_docking_rights, offer_docking_rights, send_attache, invite_to_faction, join_faction, kick_from_faction, request_licence_production, start_lend_lease, request_lend_lease, send_volunteers, expeditionary_force, return_expeditions, boost_party_popularity, stage_coup, embargo, justify_war_goal, add_war_goal, cancel_war_goal, declare_war, ...) or an exact row name from hoi4_diplomacy. justify_war_goal / add_war_goal also need `war_goal` (type, e.g. Conquer) and 1..5 `states` by name (preview them first with hoi4_diplomacy). declare_war needs `confirm:\"declare_war\"` and a completed war goal; without one the call is refused with war_goal_required and nothing is sent. Typed refusals before any input: action_not_offered (lists the offered rows), action_disabled, war_goal_required, ai_would_refuse (the game's red cross on the row; pass try_anyway:true to send anyway), nothing_to_change, confirmation_required, dialog_unsupported (dialogs that need choices, e.g. volunteers groups). The receipt is settled from memory by the one relation, war goal, war or faction membership the action can create or remove (see hoi4_diplomacy_state); an AI refusal is a NotApplied receipt.","inputSchema":{"type":"object","properties":{"country":{"type":"string","pattern":"^[A-Z0-9]{3}$"},"state_id":{"type":"integer","minimum":1,"maximum":4095},"action":{"type":"string"},"war_goal":{"type":"string"},"states":{"type":"array","items":{"type":"string"},"maxItems":5},"try_anyway":{"type":"boolean"},"confirm":{"type":"string"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["action","request_id","lease_token"],"additionalProperties":false}}),
            commanders::tool(),
            supply::tool(),
            template_get::tool(),
            template_edit::create_tool(),
            template_edit::edit_tool(),
            template_edit::rename_tool(),
            template_edit::delete_tool(),
            template_edit::options_tool(),
            design_edit::options_tool(),
            design_edit::create_tool(),
            design_edit::obsolete_tool(),
            design_edit::set_production_tool(),
            special_projects::tool(),
            special_projects::panel_tool(),
            special_projects::assign_tool(),
            special_projects::stop_tool(),
            special_projects::unassign_tool(),
            special_projects::recruit_tool(),
            doctrine_act::tool(),
            json!({"name":"hoi4_army_commander","description":"Assign a general (exact name as listed by hoi4_commanders) to an own army, or a field marshal to an own army group (use the army group's name), from the panel's commander list. Verified when the army's commander is that general. Refused before any click with army_unknown (lists valid armies/groups), unknown_commander (5 closest names) or commander_unavailable (already leads another army/group; pass reassign: true to move them).","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"general":{"type":"string"},"reassign":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","general","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_air_wings","description":"Own air wings with air base, assigned region and missions, from the Air Forces Overview (opens it). Changes nothing in the game.","inputSchema":{"type":"object","properties":{"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_air_mission","description":"Give an own air wing (exact name) a mission over a strategic air region with the game's ordinary controls: wing row of the Air Forces Overview (L), the mission button, the Find view centres `state_id` (any state inside the wanted region) and a right click assigns. Pass `region` (region id or display name, e.g. 'Eastern Germany') instead of or with `state_id`: the state is then chosen from the game's files and the result is verified to be that region. Missions: air_superiority, cas, interception, strategic_bombing, naval_strike, kamikaze, port_strike, logistics_strike, air_supply, training, naval_mines, naval_minesweeping, recon, naval_patrol. Typed refusals before input: wing_unknown, wing_ambiguous, invalid_arguments (unknown mission), region_unknown, region_state_mismatch; the game's own disabling (mission not offered / needs more planes) surfaces as invalid_target. Verified from the wing's mission mask and region id in memory (hoi4_air). No percent or intensity control exists in the game's air UI; aggressiveness and day/night are per-wing cycle buttons not driven by this tool.","inputSchema":{"type":"object","properties":{"wing":{"type":"string","maxLength":128},"mission":{"type":"string"},"state_id":{"type":"integer","minimum":1,"maximum":4095},"region":{"type":"string","maxLength":96},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["wing","mission","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_air_rebase","description":"Rebase one own air wing to the air base of a state with ordinary input: strategic air map (F3), the wing's row of the Air Forces Overview, the Find view centres `state_id`, the base's circle is located from the game's own map icon objects and right clicked. The wing flies a transfer (status 'To <base>'); verified from the wing's transfer destination in memory (hoi4_air `transfer_to_base_id`), the base id changes on arrival. Typed refusals before any input: wing_unknown, wing_ambiguous, air_base_unknown (state without an own air base; lists own base states), nothing_to_change (already based or already flying there), air_base_full (capacity minus wings stationed or incoming). The game set no range limit on a transfer in the test (a 570 km fighter wing was accepted 207 map units away); if it refuses anyway the tool reports rebase_refused. Map mode is restored afterwards.","inputSchema":{"type":"object","properties":{"wing":{"type":"string","maxLength":128},"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["wing","state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_air_wing_create","description":"Found a new air wing from reserve aircraft at an own air base: strategic air map, Find view centres `state_id`, left click on the base circle, the N shortcut ('Add new Air Wing'), the reserve row of `aircraft` (variant name such as 'Ju 86' or class name such as 'Inter-War Fighter', case-insensitive) moves up to 100 planes of that variant into the wing (the game's own limit per click), Ok. Reserve aircraft come from production or from disbanding a wing. Typed refusals: air_base_unknown, air_base_full (before input); no_reserve_aircraft (the game refuses the window when the reserve is empty), aircraft_not_in_reserve (lists what is in reserve), aircraft_ambiguous, create_unavailable (Ok disabled, e.g. no manpower) while driving; nothing is created in those cases. Verified from the new wing's name appearing at that base in memory (hoi4_air). The wing gets the game's default name and 'Deploying from Reserves' status.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"aircraft":{"type":"string","maxLength":96},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["state_id","aircraft","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_air_wing_disband","description":"Disband one own air wing (row of the Air Forces Overview, toolbar Disband, OK of the confirmation); all its equipment returns to the reserve. Typed refusals before input: wing_unknown, wing_ambiguous; disband_unavailable if the game disables the button. Verified from the wing's absence in memory (hoi4_air).","inputSchema":{"type":"object","properties":{"wing":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["wing","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_air_wing_split","description":"Split one own air wing in two halves with the toolbar's Split button (shortcut D) after selecting it in the Air Forces Overview: the original keeps half of its aircraft, a new wing of the same type, base and orders takes the rest and gets the game's default name. Typed refusals before input: wing_unknown, wing_ambiguous, wing_in_transfer, nothing_to_change (fewer than 2 aircraft); split_unavailable if the game disables the button. Verified from memory: one more wing at the base and fewer aircraft in the original. This and hoi4_air_wing_create (up to 100 aircraft per click) are the ways to size wings; hoi4_air_wing_merge joins them again.","inputSchema":{"type":"object","properties":{"wing":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["wing","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_air_wing_merge","description":"Merge own air wings of one aircraft type at one air base with the toolbar's Merge button. The game's map stack and merge work on every wing of that type at the base, so `wing` is joined with all others of its type there; give `with` to name the single expected partner (refused with merge_group_larger when more than two wings of the type are there). The first wing is selected through its Air Forces Overview row, the stack item of the type beside the base circle joins with a Shift click, the game's selection array decides that exactly the group is selected, then Merge; the first wing survives with the aircraft (a wing holds at most 100: merging 40+80+40 bombers left wings of 100 and 60). Typed refusals before input: wing_unknown, wing_ambiguous, invalid_arguments (same wing twice), merge_different_base, merge_different_type, no_merge_partner, merge_group_larger, wing_in_transfer; merge_unavailable if the game disables the button, selection_lost if the group could not be selected (nothing is merged then). Verified from memory: fewer wings of the group left holding all their aircraft.","inputSchema":{"type":"object","properties":{"wing":{"type":"string","maxLength":128},"with":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["wing","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_air_hold","description":"Stop an own air wing's mission (the toolbar's Hold Air Wings button, shortcut H): the wing keeps its base and stands by (its panel row reads 'Standing by', the Air Forces Overview's Area column empties); its assigned region is cleared, the last mission selection stays in memory. Typed refusals before input: wing_unknown, wing_ambiguous, nothing_to_change (already standing by); hold_unavailable if the game disables the button. Verified from the wing's assigned region pointer in memory.","inputSchema":{"type":"object","properties":{"wing":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["wing","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_air_region_probe","description":"What the player's air map shows for one strategic air region: Find view centres `state_id`, a left click on the region opens the game's region window, which is read (region name, active aircraft by type for us and the enemy: fighters, bombers, close air support, anti-air guns; detection and efficiency percentages) and closed again. Orders nothing; moves the cursor and clicks, so it needs the input lease. Only what the air map displays; no hidden enemy information.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"lease_token":{"type":"string"}},"required":["state_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_air_regions","description":"Own wings and air bases grouped by strategic air region, read from memory and the game's map files (no input, never pauses): per region its name, own wings assigned (aircraft, type, missions, base state), aircraft total and the own air bases inside with capacity, aircraft stationed and free capacity; plus the wings that have no region. Enemy presence is not in memory: use hoi4_air_region_probe for the regions that matter.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
            json!({"name":"hoi4_army_group_create","description":"Group two or more own armies into a new army group (army cards with Shift, then the new-army-group card). Give it a field marshal with hoi4_army_commander using the army group's name. Verified when all the armies belong to one new army group.","inputSchema":{"type":"object","properties":{"armies":{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":12},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["armies","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_army_split","description":"Split the first N divisions (list order, N 1..19) of an own army into a new army: the army's card, the panel's first row, Shift on row N, then the new-army card. Use it to bring an oversized army (more than 24 divisions) under its general's cap. Verified by a new army holding exactly those divisions.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"count":{"type":"integer","minimum":1,"maximum":19},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","count","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_state_slots","description":"Free building slots of own states (unlocked, max, used, free) read from each state's view (Find view centres the state, one click opens it). Use it to choose where a factory can still be queued before hoi4_construction_add. Changes nothing in the game; up to 12 states per call.","inputSchema":{"type":"object","properties":{"state_ids":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4095},"minItems":1,"maxItems":12},"lease_token":{"type":"string"}},"required":["state_ids","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_army_create","description":"(optional absorb_existing: true folds armies standing there into the new one) Create one army from the own divisions shown around a state: Find view centres the state, a box selection picks own divisions there, the new-army card creates the army. Verified by a new army in memory; returns its divisions. Refused before any input with already_in_army (every own division in that state belongs to a named army; use absorb_existing or hoi4_army_split) or no_own_divisions (none stand there per hoi4_armies); input_readback_failed after the click makes no claim about the cause.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"absorb_existing":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_army_assign_plan","description":"Pure read: where the unassigned divisions standing in a state would go (into the standing army with the most room under 24, or the named one; a new army when none stands there), how many, the overflow and the split needed afterwards. Typed refusals: no_unassigned_here, no_such_army, target_not_in_state, army_full.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"army":{"type":"string","maxLength":128}},"required":["state_id"],"additionalProperties":false}}),
            json!({"name":"hoi4_army_assign","description":"Fold the unassigned divisions standing in a state into an existing army there without disturbing other armies, or create a new army when none stands there. Same refusals as hoi4_army_assign_plan, all before any input. Folding into a standing army uses the game's own control (box-select the divisions, RIGHT-click the army's card); it refuses with selection_mismatch, before the click, when the box also took divisions of another army. Creating a new army uses the verified new-army path.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"army":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_time","description":"Set desired pause or internal speed 0..4 with ordinary keys (Space, keypad +/-) sent only while the owned game window is front-most, each verified by fresh memory readback. Supply exactly one target and a stable request_id; uncertain dispatches are never replayed.","inputSchema":{"type":"object","properties":{"paused":{"type":"boolean"},"speed":{"type":"integer","minimum":0,"maximum":4},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}}),
        ])
        .chain(fleetops::tools())
        .chain(std::iter::once(invasion::tool()))
        .chain(faction::tools())
        .chain(agency_act::tools())
        .chain(mio_act::tools())
        .collect()
    }
    // Broker allowlist note: `hoi4_country_status` is a pure read (no lease,
    // no input, no cursor); an external broker allowlist that names the other
    // read-only tools (hoi4_armies, hoi4_states, hoi4_events) should list it too.
    fn supports(&self, name: &str) -> bool {
        fleetops::handles(name)
            || faction::handles(name)
            || name == invasion::TOOL
            || matches!(
            name,
            "hoi4_daily_governor"
                | "hoi4_navy_ui"
                | "hoi4_fleet_mission"
                | "hoi4_fleet_region"
                | "hoi4_fleet_create"
                | "hoi4_naval_production_options"
                | "hoi4_fleet_admiral"
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
                | "hoi4_diplomacy_countries"
                | "hoi4_peace_status"
                | "hoi4_peace_options"
                | "hoi4_peace_take"
                | "hoi4_peace_submit"
                | "hoi4_peace_exit"
                | "hoi4_trade"
                | "hoi4_trade_set"
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
                | "hoi4_template_create"
                | "hoi4_template_edit"
                | "hoi4_template_rename"
                | "hoi4_template_delete"
                | "hoi4_template_options"
                | "hoi4_design_options"
                | "hoi4_design_create"
                | "hoi4_design_obsolete"
                | "hoi4_design_set_production"
                | "hoi4_special_projects"
                | "hoi4_special_projects_panel"
                | "hoi4_special_project_assign"
                | "hoi4_special_project_stop"
                | "hoi4_special_project_unassign"
                | "hoi4_special_project_recruit"
                | "hoi4_doctrine_adopt"
                | "hoi4_mio_design_team"
                | "hoi4_mio_policy"
                | "hoi4_mio_trait"
                | "hoi4_agency_create"
                | "hoi4_agency_upgrade"
                | "hoi4_operative_recruit"
                | "hoi4_operative_assign"
                | "hoi4_army_group_create"
                | "hoi4_army_split"
                | "hoi4_state_slots"
                | "hoi4_air_wings"
                | "hoi4_air_mission"
                | "hoi4_air_rebase"
                | "hoi4_air_wing_create"
                | "hoi4_air_wing_disband"
                | "hoi4_air_hold"
                | "hoi4_air_wing_split"
                | "hoi4_air_wing_merge"
                | "hoi4_air_region_probe"
                | "hoi4_air_regions"
        )
    }
    fn wait_events(&self) -> Option<Arc<crate::protocol::worker_events::Events>> {
        Some(self.daily.events())
    }
    fn plan_path(&self) -> Option<std::path::PathBuf> {
        Some(self.directory.join("plan.json"))
    }
    fn date_stop_status(&self) -> Option<Value> {
        Some(self.daily.date_stop_status())
    }
    fn capabilities(&self) -> Value {
        json!({"transport":"ordinary macOS CGEvent keys to the front-most owned window","actions":["set_paused","set_speed","research_start","focus_start","production_add","production_set","construction_add","politics_options","politics_choose","decisions_list","decision_take","train","deploy_locations","army_create","army_assign","army_front","army_plan","army_offensive","event_choose","diplomacy","diplomacy_act","army_commander","army_group_create","army_split","template_create","template_edit","template_rename","template_delete","template_options","design_options","design_create","design_obsolete","design_set_production","state_slots","air_wings","air_mission","plan_run"],"lease":"exclusive configured controller","input_profile":{"name":"macos_cocoa_absolute_1920x1080_v1","checked_at_bind":true,"rechecked_before_each_input":true,"diagnostic_alone_authorizes_input":false},"construction_panel":{"enabled":self.construction_panel,"qualification_only":true,"ordinary_key":"t","live_delivery_verified":false,"input_retry":false},"construction_selection":{"enabled":self.construction_selection,"qualification_only":true,"ordinary_clicks_max":1,"input_retry":false,"placement":false},"construction_placement":{"enabled":self.construction_placement,"qualification_only":true,"ordinary_clicks_max":1,"full_picker_verified":false,"atomic_event_verified":false},"construction_cancellation":{"enabled":self.construction_cancellation,"qualification_only":true,"ordinary_key":"escape","ordinary_key_presses_max":1,"input_retry":false,"worker":false},"construction_find_open":{"enabled":self.construction_find_open,"qualification_only":true,"transport":"ordinary_left_click","ordinary_clicks_max":1,"cursor_moves_max":1,"ordinary_key_presses":0,"input_retry":false,"worker":false},"construction_navigation":{"enabled":self.construction_navigation,"qualification_only":true,"already_open_only":true,"input_retry":false,"placement":false},"daily_governor":self.daily.capabilities(),"native_posting":false,"full_campaign_acceptance":false})
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
        if name == faction::INCOMING && !args.contains_key("answer") {
            allowed(args, &["which", "index"])?;
            return faction::incoming_read(process);
        }
        if name == "hoi4_peace_status" {
            allowed(args, &[])?;
            return events::peace_status(process);
        }
        if name == "hoi4_diplomacy_countries" {
            allowed(
                args,
                &["query", "relation", "faction", "only_with_states", "limit", "offset"],
            )?;
            return diplomacy_state::countries(process, args);
        }
        if name == "hoi4_fleets" {
            allowed(args, &["ships", "fleet"])?;
            return naval::fleets_tool(process, args);
        }
        if name == "hoi4_naval_production_options" {
            allowed(args, &[])?;
            return navalact::production_options(process);
        }
        if name == "hoi4_convoys" {
            allowed(args, &[])?;
            return naval::convoys_tool(process, args);
        }
        if name == "hoi4_air" {
            allowed(args, &["wing"])?;
            return airread::tool(process, args);
        }
        if name == "hoi4_air_regions" {
            allowed(args, &[])?;
            return airact::regions(process);
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
        // Tools built offline from the GUI definitions (all validated:false); see kit.rs.
        if fleetops::handles(name) || faction::handles(name) || name == invasion::TOOL {
            let env = kit::Env {
                process,
                keys: &self.keys,
                guard,
                authority: &*authority,
                actions,
            };
            return if fleetops::handles(name) {
                fleetops::call(name, args, env)
            } else if name == invasion::TOOL {
                invasion::call(args, env)
            } else {
                faction::call(name, args, env)
            };
        }
        if mio_act::TOOLS.contains(&name) {
            return mio_act::call(name, args, process, &self.keys, guard, authority, actions);
        }
        if agency_act::TOOLS.contains(&name) {
            return agency_act::call(name, args, process, &self.keys, guard, authority, actions);
        }
        if name == "hoi4_doctrine_adopt" {
            return doctrine_act::call(args, process, &self.keys, guard, authority, actions);
        }
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
                    // The server's own date watcher acts only under this lease.
                    self.daily.arm_date_stop(self, &token, &epoch);
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
                    self.daily.disarm_date_stop();
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
        if name == "hoi4_design_options" {
            allowed(args, &["kind", "base", "slot", "measure_costs", "lease_token"])?;
            let kind = design_edit::kind_arg(args)?;
            let token = string(args, "lease_token")?;
            let expected = guard.observe_context(process)?;
            let epoch = guard.epoch.clone();
            let lease_end = authority.authorize(token, &epoch)?;
            let Some(base) = args.get("base") else {
                return design_edit::options_listing(process, kind);
            };
            let base = design_edit::base_arg(base)?;
            let slot = match args.get("slot") {
                None => None,
                Some(v) => Some(
                    v.as_str()
                        .filter(|s| !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'))
                        .ok_or_else(|| Error::new("invalid_arguments", "Invalid slot name"))?,
                ),
            };
            let measure = args.get("measure_costs").and_then(Value::as_bool).unwrap_or(true);
            let fence = action_fence::Fence {
                process,
                epoch: &epoch,
                expected: &expected,
                authority,
                token,
                until: (Instant::now() + Duration::from_secs(240)).min(lease_end),
                stop: None,
            };
            let check_input = || fence.input_check();
            let bounded_keys = self.keys.guarded(&fence, &check_input)?;
            let process: &dyn Process = &fence;
            bounded_keys.preflight()?;
            return design_edit::options(process, &bounded_keys, kind, &base, slot, measure);
        }
        if name == "hoi4_design_obsolete" || name == "hoi4_design_set_production" {
            let obsolete = name == "hoi4_design_obsolete";
            if obsolete {
                allowed(args, &["design", "obsolete", "request_id", "lease_token"])?;
            } else {
                allowed(
                    args,
                    &["line", "design", "accept_progress_loss", "request_id", "lease_token"],
                )?;
            }
            let design = args
                .get("design")
                .and_then(Value::as_i64)
                .and_then(|d| i32::try_from(d).ok())
                .filter(|d| *d > 0)
                .ok_or_else(|| Error::new("invalid_arguments", "design must be a positive design id"))?;
            let want_obsolete = match args.get("obsolete") {
                None => true,
                Some(v) if obsolete => v
                    .as_bool()
                    .ok_or_else(|| Error::new("invalid_arguments", "obsolete must be a boolean"))?,
                Some(_) => return Err(Error::new("invalid_arguments", "obsolete applies to hoi4_design_obsolete")),
            };
            let line = if obsolete {
                0
            } else {
                args.get("line")
                    .and_then(Value::as_u64)
                    .filter(|l| *l > 0)
                    .ok_or_else(|| Error::new("invalid_arguments", "line must be a production line handle"))?
            };
            let accept = args
                .get("accept_progress_loss")
                .and_then(Value::as_bool)
                .unwrap_or(false);
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
                until: (Instant::now() + Duration::from_secs(90)).min(lease_end),
                stop: None,
            };
            let check_input = || fence.input_check();
            let bounded_keys = self.keys.guarded(&fence, &check_input)?;
            let process: &dyn Process = &fence;
            let retry = actions.receipt_exists(id);
            let keys = &bounded_keys;
            if obsolete {
                let all = designs::read_all(process)?;
                let before = all.iter().find(|d| d.variant.id == design).map(|d| d.variant.obsolete);
                let found = if retry {
                    all.iter().find(|d| d.variant.id == design).cloned()
                } else {
                    let d = design_edit::plan_obsolete(&all, design, want_obsolete)?.clone();
                    bounded_keys.preflight()?;
                    Some(d)
                };
                let Some(found) = found else {
                    return Err(Error::new("design_unknown", format!("No own design {design}")));
                };
                let action = json!({"kind":"design_obsolete","design":design,"obsolete":want_obsolete});
                let info = std::cell::RefCell::new(Value::Null);
                let mut settle = || {
                    let now = design_edit::obsolete_flag(process, design)?;
                    let detail = json!({"design":design,"obsolete":now});
                    let verdict = design_edit::settle_obsolete(before.unwrap_or(!want_obsolete), now, want_obsolete);
                    Ok(verdict.outcome(detail))
                };
                let mut dispatch = || {
                    authority.authorize(token, &epoch)?;
                    if retry {
                        return Err(Error::new("action_uncertain", "A retried request is never dispatched again"));
                    }
                    *info.borrow_mut() = design_edit::obsolete_apply(process, keys, &found, want_obsolete)?;
                    Ok(())
                };
                let receipt = actions.execute_with(id, &epoch, action, &mut settle, &mut dispatch, true)?;
                return Ok(json!({"receipt":receipt,"designer":*info.borrow()}));
            }
            let all = designs::read_all(process)?;
            let lines = production::lines(process)?;
            let change = if retry {
                design_edit::LineChange {
                    handle: line,
                    design,
                    from_design: None,
                    naval: false,
                }
            } else {
                let c = design_edit::plan_set_production(&lines, &all, line, design, accept)?;
                bounded_keys.preflight()?;
                c
            };
            let action = json!({"kind":"design_set_production","line":line,"design":design});
            let info = std::cell::RefCell::new(Value::Null);
            let mut settle = || {
                let all = designs::read_all(process)?;
                let detail = json!({"line":line,"design":design});
                Ok(design_edit::settle_set_production(&all, &change).outcome(detail))
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                if retry {
                    return Err(Error::new("action_uncertain", "A retried request is never dispatched again"));
                }
                *info.borrow_mut() = design_edit::set_production_apply(process, keys, &change)?;
                Ok(())
            };
            let receipt = actions.execute_with(id, &epoch, action, &mut settle, &mut dispatch, true)?;
            return Ok(json!({"receipt":receipt,"change":*info.borrow()}));
        }
        if name == "hoi4_design_create" {
            allowed(
                args,
                &["kind", "base", "upgrades", "modules", "name", "request_id", "lease_token"],
            )?;
            let input = design_edit::Input {
                kind: design_edit::kind_arg(args)?,
                base: design_edit::base_arg(
                    args.get("base")
                        .ok_or_else(|| Error::new("invalid_arguments", "base is required"))?,
                )?,
                upgrades: match args.get("upgrades") {
                    Some(v) => design_edit::upgrades_arg(v)?,
                    None => Default::default(),
                },
                modules: match args.get("modules") {
                    Some(v) => design_edit::modules_arg(v)?,
                    None => Default::default(),
                },
                name: design_edit::name_arg(args.get("name"))?,
            };
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let expected = guard.observe_context(process)?;
            let epoch = guard.epoch.clone();
            let lease_end = authority.authorize(token, &epoch)?;
            // A design with several steps takes about a minute of ordinary clicks; the
            // deadline is fixed up front and never renewed.
            let fence = action_fence::Fence {
                process,
                epoch: &epoch,
                expected: &expected,
                authority,
                token,
                until: (Instant::now() + Duration::from_secs(240)).min(lease_end),
                stop: None,
            };
            let check_input = || fence.input_check();
            let bounded_keys = self.keys.guarded(&fence, &check_input)?;
            let process: &dyn Process = &fence;
            let retry = actions.receipt_exists(id);
            let catalogue = design_edit::catalogue(process)?;
            let planned = design_edit::plan(&input, &catalogue, id, retry)?;
            if !retry {
                bounded_keys.preflight()?;
            }
            let action = json!({
                "kind": "design_create",
                "design_kind": input.kind,
                "base": planned.base_id,
                "upgrades": input.upgrades,
                "modules": input.modules,
                "name": planned.want.name,
            });
            let info = std::cell::RefCell::new(Value::Null);
            let keys = &bounded_keys;
            let mut settle = || {
                let now = designs::read_all(process)?;
                let verdict = if retry {
                    design_edit::settle_retry(&now, &planned.want)
                } else {
                    design_edit::settle_create(&planned.before_ids, &now, &planned.want)
                };
                let detail = json!({"design_name":planned.want.name,"base":planned.base_id});
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
                *info.borrow_mut() = design_edit::apply(process, keys, &planned, input.kind)?;
                Ok(())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            let all = designs::read_all(process)?;
            let saved = all
                .iter()
                .find(|d| !planned.before_ids.contains(&d.variant.id) && design_edit::matches_want(d, &planned.want))
                .or_else(|| all.iter().find(|d| design_edit::matches_want(d, &planned.want)));
            let row = saved.map_or(Value::Null, |d| design_edit::design_row(&all, d.variant.id));
            return Ok(json!({"receipt":receipt,"designer":*info.borrow(),"design":row}));
        }
        if name == "hoi4_template_options" {
            allowed(args, &["lease_token"])?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            self.keys.preflight()?;
            return template_edit::options(process, &self.keys);
        }
        if matches!(
            name,
            "hoi4_template_create"
                | "hoi4_template_edit"
                | "hoi4_template_rename"
                | "hoi4_template_delete"
        ) {
            let kind = &name["hoi4_template_".len()..];
            match kind {
                "create" => allowed(
                    args,
                    &["name", "regiments", "support", "priority", "request_id", "lease_token"],
                )?,
                "edit" => allowed(
                    args,
                    &[
                        "template",
                        "regiments",
                        "support",
                        "priority",
                        "new_name",
                        "allow_in_use",
                        "request_id",
                        "lease_token",
                    ],
                )?,
                "rename" => allowed(args, &["template", "new_name", "request_id", "lease_token"])?,
                _ => allowed(args, &["template", "request_id", "lease_token"])?,
            }
            let existing = if kind == "create" {
                None
            } else {
                Some(string(args, "template")?)
            };
            if existing.is_some_and(|t| t.is_empty() || t.len() > 128 || t.chars().any(char::is_control)) {
                return Err(Error::new("invalid_arguments", "Invalid template name"));
            }
            let input = template_edit::Input {
                existing,
                name: match kind {
                    "create" => Some(
                        template_edit::name_arg(args.get("name"), "name")?
                            .ok_or_else(|| Error::new("invalid_arguments", "name is required"))?,
                    ),
                    "edit" | "rename" => template_edit::name_arg(args.get("new_name"), "new_name")?,
                    _ => None,
                },
                regiments: match args.get("regiments") {
                    Some(v) => Some(template_edit::parse_regiments(v)?),
                    None if kind == "create" => {
                        return Err(Error::new("invalid_arguments", "regiments is required"))
                    }
                    None => None,
                },
                support: match args.get("support") {
                    Some(v) => Some(template_edit::parse_support(v)?),
                    None if kind == "create" => Some(Vec::new()),
                    None => None,
                },
                priority: template_edit::priority_arg(args.get("priority"))?,
                allow_in_use: args
                    .get("allow_in_use")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            };
            if kind == "rename" && input.name.is_none() {
                return Err(Error::new("invalid_arguments", "new_name is required"));
            }
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let expected = guard.observe_context(process)?;
            let epoch = guard.epoch.clone();
            let lease_end = authority.authorize(token, &epoch)?;
            // A template with ten battalions takes about a minute of ordinary clicks; the
            // deadline is fixed up front and never renewed.
            let fence = action_fence::Fence {
                process,
                epoch: &epoch,
                expected: &expected,
                authority,
                token,
                until: (Instant::now() + Duration::from_secs(180)).min(lease_end),
                stop: None,
            };
            let check_input = || fence.input_check();
            let bounded_keys = self.keys.guarded(&fence, &check_input)?;
            let process: &dyn Process = &fence;
            let retry = actions.receipt_exists(id);
            if kind == "delete" {
                let template = existing.unwrap_or_default();
                let before = if retry {
                    template_edit::layouts(process)?
                        .into_iter()
                        .find(|l| l.name == template)
                } else {
                    let (before, _) = template_edit::plan_delete(process, template)?;
                    bounded_keys.preflight()?;
                    Some(before)
                };
                let action = json!({"kind":"template_delete","template":template});
                let keys = &bounded_keys;
                let mut settle = || {
                    let now = template_edit::layouts(process)?
                        .into_iter()
                        .find(|l| l.name == template);
                    let detail = json!({"template":template,"still_listed":now.is_some()});
                    let verdict = match &before {
                        Some(b) => template_edit::settle_delete(b, now.as_ref()),
                        None if now.is_none() => settle::Verdict::Reached,
                        None => settle::Verdict::Unknown,
                    };
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
                    template_edit::delete(process, keys, template)
                };
                let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
                return Ok(json!({"receipt":receipt}));
            }
            let list = template_edit::layouts(process)?;
            let (want, before, _) = template_edit::build_want(&input, &list)?;
            let planned = if retry {
                None
            } else {
                let defs = template_edit::catalog(process)?;
                let usage = match existing {
                    Some(n) => Some(template_edit::usage(process, n)?),
                    None => None,
                };
                let planned = template_edit::plan_request(&input, &list, &defs, usage.as_ref())?;
                bounded_keys.preflight()?;
                Some(planned)
            };
            let action = json!({
                "kind": format!("template_{kind}"),
                "template": existing,
                "layout": {
                    "name": want.name,
                    "regiments": want.columns,
                    "support": want.support,
                    "priority": want.priority,
                },
            });
            let info = std::cell::RefCell::new(Value::Null);
            let keys = &bounded_keys;
            let mut settle = || {
                let now_list = template_edit::layouts(process)?;
                let now = now_list
                    .iter()
                    .find(|l| l.name == want.name)
                    .or_else(|| before.as_ref().and_then(|b| now_list.iter().find(|l| l.name == b.name)))
                    .cloned();
                let detail = json!({"template":want.name,"layout_saved":now.as_ref() == Some(&want)});
                let verdict = template_edit::settle_layout(before.as_ref(), now.as_ref(), &want);
                Ok(if retry && verdict == settle::Verdict::Unchanged {
                    journal::Outcome::Diverged(detail)
                } else {
                    verdict.outcome(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let planned = planned.as_ref().ok_or_else(|| {
                    Error::new("action_uncertain", "A retried request is never dispatched again")
                })?;
                *info.borrow_mut() = template_edit::apply(process, keys, planned)?;
                Ok(())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            let row = template_edit::readback(process, &want.name).ok();
            return Ok(json!({"receipt":receipt,"designer":*info.borrow(),"template":row}));
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
                    &["equipment", "factories", "design", "request_id", "lease_token"],
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
            let design = match args.get("design") {
                None => None,
                Some(v) if adding => Some(
                    v.as_i64()
                        .and_then(|d| i32::try_from(d).ok())
                        .filter(|d| *d > 0)
                        .ok_or_else(|| Error::new("invalid_arguments", "design must be a positive design id"))?,
                ),
                Some(_) => {
                    return Err(Error::new("invalid_arguments", "design applies to adding a line"))
                }
            };
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
            let action = if design.is_some() {
                json!({"kind":name.trim_start_matches("hoi4_"),"equipment":key,"factories":factories,"line":index,"design":design})
            } else {
                json!({"kind":name.trim_start_matches("hoi4_"),"equipment":key,"factories":factories,"line":index})
            };
            let before = production::lines(process)?;
            let retry = actions.receipt_exists(id);
            if !retry {
                if let Some(wanted) = design {
                    design_edit::check_production_design(&designs::read_all(process)?, wanted, key)?;
                }
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
                        let design_ok = match design {
                            None => true,
                            Some(wanted) => designs::read_all(process)?.iter().any(|d| {
                                d.variant.id == wanted
                                    && d.lines.iter().any(|(h, _)| added.iter().any(|l| l.handle == *h))
                            }),
                        };
                        if design_ok
                            && added
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
                    None => production::add(process, keys, key, factories, design),
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
                        "country",
                        "action",
                        "war_goal",
                        "states",
                        "try_anyway",
                        "confirm",
                        "request_id",
                        "lease_token",
                    ],
                )?;
            } else {
                allowed(
                    args,
                    &["state_id", "country", "countries", "action", "war_goal", "states", "lease_token"],
                )?;
            }
            let text_ok =
                |v: &str| !v.is_empty() && v.len() <= 128 && !v.chars().any(char::is_control);
            let tag_ok = |v: &str| v.len() == 3 && v.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
            let tag_of = |v: &Value| -> Result<String> {
                v.as_str()
                    .filter(|t| tag_ok(t))
                    .map(str::to_string)
                    .ok_or_else(|| Error::new("invalid_arguments", "country must be a three-letter tag such as POL"))
            };
            let state_arg = match args.get("state_id") {
                None => None,
                Some(v) => Some(
                    v.as_i64()
                        .filter(|v| (1..4096).contains(v))
                        .ok_or_else(|| Error::new("invalid_arguments", "state_id must be 1..4095"))?
                        as i32,
                ),
            };
            let country_arg = args.get("country").map(tag_of).transpose()?;
            let many: Vec<String> = match args.get("countries") {
                None => Vec::new(),
                Some(v) => v
                    .as_array()
                    .filter(|a| a.len() <= 6 && !a.is_empty())
                    .ok_or_else(|| Error::new("invalid_arguments", "countries must list 1..6 tags"))?
                    .iter()
                    .map(&tag_of)
                    .collect::<Result<Vec<_>>>()?,
            };
            if state_arg.is_some() && country_arg.is_some() {
                return Err(Error::new("invalid_arguments", "Give country or state_id, not both"));
            }
            let token = string(args, "lease_token")?;
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
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let target = match (state_arg, country_arg) {
                (Some(state), None) => Some(diplomacy::Target::State(state)),
                (None, Some(tag)) => Some(diplomacy::Target::Country(tag)),
                _ => None,
            };
            if !act {
                self.keys.preflight()?;
                if !many.is_empty() {
                    if target.is_some() || args.contains_key("action") {
                        return Err(Error::new(
                            "invalid_arguments",
                            "countries cannot be combined with country, state_id or action",
                        ));
                    }
                    let mut out = Vec::new();
                    for tag in many {
                        out.push(
                            match diplomacy::actions(process, &self.keys, &diplomacy::Target::Country(tag.clone())) {
                                Ok(v) => v,
                                Err(e) => json!({"country":tag,"error":{"code":e.code,"message":e.message}}),
                            },
                        );
                    }
                    return Ok(json!({"countries":out}));
                }
                let target = target.ok_or_else(|| {
                    Error::new("invalid_arguments", "country (tag) or state_id is required")
                })?;
                return match args.get("action") {
                    None => diplomacy::actions(process, &self.keys, &target),
                    Some(a) => {
                        let a = a
                            .as_str()
                            .filter(|v| text_ok(v))
                            .ok_or_else(|| Error::new("invalid_arguments", "Invalid action name"))?;
                        diplomacy::preview(process, &self.keys, &target, a, kind, &states)
                    }
                };
            }
            let target = target
                .ok_or_else(|| Error::new("invalid_arguments", "country (tag) or state_id is required"))?;
            let action_name = string(args, "action")?;
            if !text_ok(action_name) {
                return Err(Error::new("invalid_arguments", "Invalid action name"));
            }
            let flag = |key: &str| -> Result<bool> {
                match args.get(key) {
                    None => Ok(false),
                    Some(v) => v
                        .as_bool()
                        .ok_or_else(|| Error::new("invalid_arguments", format!("{key} must be boolean"))),
                }
            };
            let try_anyway = flag("try_anyway")?;
            let confirm = match args.get("confirm") {
                None => None,
                Some(v) => Some(
                    v.as_str()
                        .filter(|v| text_ok(v))
                        .ok_or_else(|| Error::new("invalid_arguments", "Invalid confirm"))?,
                ),
            };
            let id = string(args, "request_id")?;
            let (state_json, country_json) = match &target {
                diplomacy::Target::State(s) => (json!(s), Value::Null),
                diplomacy::Target::Country(t) => (Value::Null, json!(t)),
            };
            let action = json!({"kind":"diplomacy_act","state_id":state_json,"country":country_json,"action":action_name,"war_goal":kind,"states":states,"try_anyway":try_anyway});
            let retry = actions.receipt_exists(id);
            let keys = &self.keys;
            let plan = if retry {
                None
            } else {
                keys.preflight()?;
                let plan = diplomacy::plan(process, keys, &target, action_name, try_anyway)?;
                if plan.spec.map(|s| s.key) == Some("declare_war") && confirm != Some("declare_war") {
                    return Err(Error::new(
                        "confirmation_required",
                        "Declaring war is one explicit step: repeat the call with confirm:\"declare_war\" (the game accepts it only with a completed war goal)",
                    ));
                }
                Some(plan)
            };
            let tag = match &plan {
                Some(p) => p.tag.clone(),
                None => diplomacy::target_tag(process, &target)?,
            };
            let spec = match &plan {
                Some(p) => p.spec,
                None => diplomacy_keys::spec_of_key(action_name)
                    .or_else(|| diplomacy_keys::spec_of_row(action_name)),
            };
            let sent = std::cell::Cell::new(false);
            let proven = std::cell::Cell::new(false);
            let dialog = std::cell::RefCell::new(Value::Null);
            let mut settle = || {
                let snap = ui::read(process, |r, i| diplomacy_state::snapshot(r, i, &tag))?;
                let mut verdict = diplomacy_keys::verdict(
                    spec,
                    plan.as_ref().map(|p| &p.before),
                    &snap,
                    proven.get(),
                );
                if verdict == settle::Verdict::Unknown && !retry && !sent.get() {
                    verdict = settle::Verdict::Unchanged;
                }
                // An AI-decided proposal is answered when the game runs: while it
                // is paused a proposal the AI accepts leaves no relation yet. It
                // is recorded as sent and never retried (Diverged), not as unchanged.
                let reply = dialog
                    .borrow()
                    .get("diplo_reply")
                    .and_then(Value::as_str)
                    .map(str::to_lowercase)
                    .unwrap_or_default();
                let awaiting = verdict == settle::Verdict::Unchanged
                    && spec.is_some_and(|s| s.ai_decides)
                    && sent.get()
                    && reply.contains("will accept")
                    && !reply.contains("not accept");
                if awaiting {
                    verdict = settle::Verdict::Diverged;
                }
                let detail = json!({"country":tag,"outcome":if awaiting {"sent_awaiting_ai_reply: the country accepts when the game runs; check hoi4_diplomacy_state"} else {""},"ai_reply":reply,
                    "relations":snap.rels.iter().map(|(k, f)| json!({"kind":k,"from_player":f})).collect::<Vec<_>>(),
                    "own_war_goals_against":snap.goals,"at_war":snap.at_war,"in_player_faction":snap.same_faction,
                    "proof":proven.get(),"dialog":dialog.borrow().clone()});
                Ok(verdict.outcome(detail))
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let plan = plan
                    .as_ref()
                    .ok_or_else(|| Error::new("invalid_state", "No plan for this dispatch"))?;
                let proof = diplomacy::execute(process, keys, plan, kind, &states, &sent)?;
                proven.set(proof.rows_changed || proof.pp_spent || proof.relation);
                *dialog.borrow_mut() = proof.dialog;
                Ok(())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_trade" {
            allowed(args, &["resource", "limit", "lease_token"])?;
            let resource = match args.get("resource") {
                None => None,
                Some(v) => Some(
                    v.as_str()
                        .filter(|v| !v.is_empty() && v.len() <= 24 && !v.chars().any(char::is_control))
                        .ok_or_else(|| Error::new("invalid_arguments", "Invalid resource"))?,
                ),
            };
            let limit = match args.get("limit") {
                None => 30,
                Some(v) => v
                    .as_u64()
                    .filter(|v| (1..=60).contains(v))
                    .ok_or_else(|| Error::new("invalid_arguments", "limit must be 1..60"))?
                    as usize,
            };
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            self.keys.preflight()?;
            return trade::view(process, &self.keys, resource, limit);
        }
        if name == "hoi4_trade_set" {
            allowed(
                args,
                &["resource", "country", "amount", "request_id", "lease_token"],
            )?;
            let resource = string(args, "resource")?;
            let country = string(args, "country")?;
            for (k, v, max) in [("resource", resource, 24), ("country", country, 64)] {
                if v.trim().is_empty() || v.len() > max || v.chars().any(char::is_control) {
                    return Err(Error::new("invalid_arguments", format!("Invalid {k}")));
                }
            }
            let amount = match args.get("amount") {
                Some(Value::String(s)) if s == "max" => trade::Amount::Max,
                Some(Value::String(s)) if s == "needed" => trade::Amount::Needed,
                Some(v) => trade::Amount::Exact(
                    v.as_i64()
                        .filter(|v| (0..=100_000).contains(v))
                        .ok_or_else(|| {
                            Error::new("invalid_arguments", "amount must be 0..100000, \"max\" or \"needed\"")
                        })?,
                ),
                None => return Err(Error::new("invalid_arguments", "amount is required")),
            };
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let retry = actions.receipt_exists(id);
            let keys = &self.keys;
            let action = json!({"kind":"trade_set","resource":resource.to_lowercase(),"country":country.trim().to_lowercase(),"amount":format!("{amount:?}")});
            let plan = if retry {
                None
            } else {
                keys.preflight()?;
                Some(trade::plan(process, keys, resource, country)?)
            };
            let sent = std::cell::Cell::new(false);
            let chosen = std::cell::Cell::new(None::<i64>);
            let target_country = plan.as_ref().map(|p| p.country.clone());
            let before = plan
                .as_ref()
                .and_then(|p| trade::contracted(&p.partner.bought));
            let want_resource = plan.as_ref().map(|p| p.resource);
            let mut settle = || {
                let Some(name) = target_country.clone().or_else(|| Some(country.to_string())) else {
                    return Ok(journal::Outcome::Unknown);
                };
                let index = want_resource.or_else(|| {
                    (0..7).find(|i| trade::resource_name(*i) == resource.to_lowercase())
                });
                let now = trade::bought_now(process, &name)?;
                let expect = match &amount {
                    trade::Amount::Exact(n) => Some(*n),
                    _ => chosen.get(),
                };
                let detail = json!({"country":name,"resource":index.map(trade::resource_name),"units_bought_now":now,"expected":expect});
                Ok(match (now, expect) {
                    (Some(n), Some(e)) if n == e && (before != Some(n) || sent.get()) => {
                        journal::Outcome::Reached(detail)
                    }
                    (Some(n), _) if Some(n) == before && !sent.get() => journal::Outcome::Unchanged(detail),
                    (Some(n), Some(_)) if Some(n) == before => journal::Outcome::Unchanged(detail),
                    (Some(_), _) => journal::Outcome::Diverged(detail),
                    (None, _) if !sent.get() && !retry => journal::Outcome::Unchanged(detail),
                    _ => journal::Outcome::Unknown,
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let plan = plan
                    .as_ref()
                    .ok_or_else(|| Error::new("invalid_state", "No plan for this dispatch"))?;
                let v = trade::execute(process, keys, plan, &amount, &sent)?;
                chosen.set(v["chosen"].as_i64());
                Ok(())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_peace_options" {
            allowed(args, &["states"])?;
            let ids: Vec<i64> = match args.get("states") {
                None => Vec::new(),
                Some(v) => v
                    .as_array()
                    .filter(|a| a.len() <= 16)
                    .and_then(|a| a.iter().map(Value::as_i64).collect::<Option<Vec<_>>>())
                    .ok_or_else(|| Error::new("invalid_arguments", "states must be up to 16 ids"))?,
            };
            if ids.iter().any(|v| !(1..4096).contains(v)) {
                return Err(Error::new("invalid_arguments", "state ids must be 1..4095"));
            }
            let ids: Vec<i32> = ids.into_iter().map(|v| v as i32).collect();
            return peace::options(process, &ids);
        }
        if matches!(name, "hoi4_peace_take" | "hoi4_peace_submit" | "hoi4_peace_exit") {
            let take = name == "hoi4_peace_take";
            if take {
                allowed(
                    args,
                    &["states", "puppet", "release", "max_cost", "request_id", "lease_token"],
                )?;
            } else {
                allowed(args, &["request_id", "lease_token"])?;
            }
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let (kind, states, max_cost) = if take {
                let ids = args
                    .get("states")
                    .and_then(Value::as_array)
                    .and_then(|a| a.iter().map(Value::as_i64).collect::<Option<Vec<_>>>())
                    .ok_or_else(|| Error::new("invalid_arguments", "states must be state ids"))?;
                let max_cost = match args.get("max_cost") {
                    None => None,
                    Some(v) => Some(v.as_f64().filter(|v| *v >= 0.0 && v.is_finite()).ok_or_else(
                        || Error::new("invalid_arguments", "max_cost must be a non-negative number"),
                    )?),
                };
                (
                    peace::Kind::parse(mgmt::flag(args, "puppet")?, mgmt::flag(args, "release")?)?,
                    peace::validate_states(&ids)?,
                    max_cost,
                )
            } else {
                (peace::Kind::Take, Vec::new(), None)
            };
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let retry = actions.receipt_exists(id);
            let keys = &self.keys;
            let sent = std::cell::Cell::new(false);
            let (plan, before_key) = if retry {
                (None, String::new())
            } else {
                keys.preflight()?;
                match name {
                    "hoi4_peace_take" => (Some(peace::plan_take(process, kind, &states, max_cost)?), String::new()),
                    "hoi4_peace_submit" => (None, peace::plan_submit(process)?),
                    _ => {
                        peace::plan_exit(process)?;
                        (None, String::new())
                    }
                }
            };
            let score_before = plan.as_ref().and_then(|p| p.score_before);
            let action = match name {
                "hoi4_peace_take" => json!({"kind":"peace_take","demand":kind.name(),"states":states}),
                "hoi4_peace_submit" => json!({"kind":"peace_submit"}),
                _ => json!({"kind":"peace_exit"}),
            };
            let tool = name;
            let mut settle = || {
                use peace::Settle;
                let (verdict, detail) = match tool {
                    "hoi4_peace_take" => {
                        let now = peace::score_now(process)?;
                        (
                            peace::take_settle(score_before, now, sent.get(), retry),
                            json!({"score_before":score_before,"score_now":now}),
                        )
                    }
                    "hoi4_peace_submit" => {
                        let now = peace::header_snapshot(process)?;
                        (
                            peace::submit_settle(&before_key, now.as_deref(), sent.get() || retry),
                            json!({"header_now":now}),
                        )
                    }
                    _ => {
                        let open = peace::is_open(process)?;
                        (
                            if !open {
                                Settle::Reached
                            } else if sent.get() || retry {
                                Settle::Diverged
                            } else {
                                Settle::Unchanged
                            },
                            json!({"conference_open":open}),
                        )
                    }
                };
                Ok(match verdict {
                    Settle::Reached => journal::Outcome::Reached(detail),
                    Settle::Unchanged => journal::Outcome::Unchanged(detail),
                    Settle::Diverged => journal::Outcome::Diverged(detail),
                    Settle::Unknown => journal::Outcome::Unknown,
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                match tool {
                    "hoi4_peace_take" => {
                        let plan = plan
                            .as_ref()
                            .ok_or_else(|| Error::new("invalid_state", "No plan for this dispatch"))?;
                        peace::take(process, keys, plan, &sent).map(|_| ())
                    }
                    "hoi4_peace_submit" => peace::submit(process, keys, &before_key, &sent).map(|_| ()),
                    _ => peace::exit(process, keys, &sent).map(|_| ()),
                }
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt,"validated":false}));
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
        if name == "hoi4_fleet_mission" || name == "hoi4_fleet_region" {
            let by_mission = name == "hoi4_fleet_mission";
            if by_mission {
                allowed(args, &["task_force", "mission", "request_id", "lease_token"])?;
            } else {
                allowed(
                    args,
                    &["fleet", "region", "state_id", "remove", "request_id", "lease_token"],
                )?;
            }
            let text_arg = |key: &str, max: usize| -> Result<String> {
                let v = string(args, key)?;
                if v.trim().is_empty() || v.len() > max || v.chars().any(char::is_control) {
                    return Err(Error::new("invalid_arguments", format!("Invalid {key}")));
                }
                Ok(v.to_string())
            };
            let (subject, detail, state, remove) = if by_mission {
                (text_arg("task_force", 128)?, text_arg("mission", 40)?, 0, false)
            } else {
                let state = args
                    .get("state_id")
                    .and_then(Value::as_i64)
                    .filter(|v| (1..4096).contains(v))
                    .ok_or_else(|| Error::new("invalid_arguments", "state_id must be 1..4095"))?
                    as i32;
                (
                    text_arg("fleet", 128)?,
                    text_arg("region", 96)?,
                    state,
                    mgmt::flag(args, "remove")?,
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
                until: (Instant::now() + Duration::from_secs(if by_mission { 60 } else { 120 })).min(lease_end),
                stop: None,
            };
            let check_input = || fence.input_check();
            let bounded_keys = self.keys.guarded(&fence, &check_input)?;
            let process: &dyn Process = &fence;
            let retry = actions.receipt_exists(id);
            let keys = &bounded_keys;
            let fleets = navalact::own_fleets(process)?;
            let mission_plan = if by_mission && !retry {
                Some(navalact::plan_mission(&fleets, &subject, &detail)?)
            } else {
                None
            };
            let region_plan = if !by_mission && !retry {
                Some(navalact::plan_region(&fleets, &subject, &detail, state, remove)?)
            } else {
                None
            };
            if !retry {
                bounded_keys.preflight()?;
            }
            let action = if by_mission {
                json!({"kind":"fleet_mission","task_force":subject,"mission":detail})
            } else {
                json!({"kind":"fleet_region","fleet":subject,"region":detail,"state_id":state,"remove":remove})
            };
            let target_code = navalact::MISSIONS.iter().find(|m| m.0 == detail).map(|m| m.2).unwrap_or(-1);
            let before_code = mission_plan.as_ref().map(|p| p.before_code);
            let had_region = region_plan.as_ref().map(|p| p.remove);
            let info = std::cell::RefCell::new(Value::Null);
            let mut settle = || {
                if by_mission {
                    let now = navalact::mission_code_now(process, &subject)?;
                    let verdict = match (retry, now) {
                        (true, Some(c)) if c == target_code => settle::Verdict::Reached,
                        (true, Some(_)) => settle::Verdict::Unchanged,
                        _ => navalact::settle_mission(before_code.unwrap_or(-1), now, target_code),
                    };
                    Ok(verdict.outcome(json!({"task_force":subject,"mission_code":now})))
                } else {
                    let now = navalact::region_present_now(process, &subject, &detail)?;
                    let want_present = !remove;
                    let verdict = match (retry, now) {
                        (true, Some(h)) if h == want_present => settle::Verdict::Reached,
                        (true, Some(_)) => settle::Verdict::Unchanged,
                        _ => navalact::settle_region(had_region.unwrap_or(false), now, want_present),
                    };
                    Ok(verdict.outcome(json!({"fleet":subject,"region":detail,"present":now})))
                }
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                if retry {
                    return Err(Error::new("action_uncertain", "A retried request is never dispatched again"));
                }
                *info.borrow_mut() = match (&mission_plan, &region_plan) {
                    (Some(plan), _) => navalact::set_mission(process, keys, plan)?,
                    (_, Some(plan)) => navalact::region_op(process, keys, plan)?,
                    _ => return Err(Error::new("invalid_arguments", "No plan")),
                };
                Ok(())
            };
            let receipt = actions.execute_with(id, &epoch, action, &mut settle, &mut dispatch, true)?;
            let after = navalact::own_fleets(process).ok();
            return Ok(json!({"receipt":receipt,"result":*info.borrow(),
                "fleets_after":after.map(|f| f.iter().map(|f| json!({"fleet":f.name.trim(),
                    "regions":f.regions.iter().map(|r| &r.1).collect::<Vec<_>>(),
                    "task_forces":f.task_forces.iter().map(|t| json!({"task_force":t.name,"mission":naval::mission_name(t.mission_code)})).collect::<Vec<_>>()})).collect::<Vec<_>>())}));
        }
        if name == "hoi4_fleet_admiral" {
            allowed(
                args,
                &["fleet", "admiral", "unassign", "list", "request_id", "lease_token"],
            )?;
            let text_arg = |key: &str, max: usize| -> Result<String> {
                let v = string(args, key)?;
                if v.trim().is_empty() || v.len() > max || v.chars().any(char::is_control) {
                    return Err(Error::new("invalid_arguments", format!("Invalid {key}")));
                }
                Ok(v.to_string())
            };
            let fleet = text_arg("fleet", 128)?;
            let list = mgmt::flag(args, "list")?;
            let unassign = mgmt::flag(args, "unassign")?;
            let admiral = match args.get("admiral") {
                Some(_) => Some(text_arg("admiral", 96)?),
                None => None,
            };
            if list && (unassign || admiral.is_some()) {
                return Err(Error::new("invalid_arguments", "list takes no admiral or unassign"));
            }
            if !list && unassign == admiral.is_some() {
                return Err(Error::new(
                    "invalid_arguments",
                    "Give exactly one of admiral or unassign:true",
                ));
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
                until: (Instant::now() + Duration::from_secs(90)).min(lease_end),
                stop: None,
            };
            let check_input = || fence.input_check();
            let bounded_keys = self.keys.guarded(&fence, &check_input)?;
            let process: &dyn Process = &fence;
            let keys = &bounded_keys;
            let fleets = navalact::own_fleets(process)?;
            if list {
                let Some(found) = fleets.iter().find(|f| f.name.trim().eq_ignore_ascii_case(fleet.trim())) else {
                    return Err(Error::new("fleet_unknown", "No own fleet with this name"));
                };
                bounded_keys.preflight()?;
                let order: Vec<String> = fleets.iter().map(|f| f.name.clone()).collect();
                let rows = navalact::list_admirals(process, keys, &found.name, &order)?;
                return Ok(json!({"fleet":found.name.trim(),"current":found.admiral,
                    "admirals":rows.iter().map(|a| json!({"name":a.name,"skill":a.skill})).collect::<Vec<_>>()}));
            }
            let retry = actions.receipt_exists(id);
            let plan = if retry {
                None
            } else {
                Some(navalact::plan_admiral(&fleets, &fleet, admiral.as_deref())?)
            };
            if !retry {
                bounded_keys.preflight()?;
            }
            let before = fleets
                .iter()
                .find(|f| f.name.trim().eq_ignore_ascii_case(fleet.trim()))
                .and_then(|f| f.admiral.clone());
            let want = admiral.clone();
            let action = json!({"kind":"fleet_admiral","fleet":fleet,"admiral":admiral,"unassign":unassign});
            let info = std::cell::RefCell::new(Value::Null);
            let mut settle = || {
                let now = navalact::admiral_now(process, &fleet)?;
                let verdict = if retry {
                    match &now {
                        Some(n) if n.as_deref().map(str::trim).map(str::to_lowercase) == want.as_deref().map(str::trim).map(str::to_lowercase) => settle::Verdict::Reached,
                        Some(_) => settle::Verdict::Unchanged,
                        None => settle::Verdict::Unknown,
                    }
                } else {
                    navalact::settle_admiral(&before, now.as_ref(), &want)
                };
                Ok(verdict.outcome(json!({"fleet":fleet,"admiral":now})))
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let Some(plan) = &plan else {
                    return Err(Error::new("action_uncertain", "A retried request is never dispatched again"));
                };
                *info.borrow_mut() = navalact::assign_admiral(process, keys, plan)?;
                Ok(())
            };
            let receipt = actions.execute_with(id, &epoch, action, &mut settle, &mut dispatch, true)?;
            return Ok(json!({"receipt":receipt,"result":*info.borrow()}));
        }
        if name == "hoi4_fleet_create" {
            allowed(args, &["task_force", "request_id", "lease_token"])?;
            let tf = string(args, "task_force")?;
            if tf.trim().is_empty() || tf.len() > 128 || tf.chars().any(char::is_control) {
                return Err(Error::new("invalid_arguments", "Invalid task_force"));
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
                until: (Instant::now() + Duration::from_secs(60)).min(lease_end),
                stop: None,
            };
            let check_input = || fence.input_check();
            let bounded_keys = self.keys.guarded(&fence, &check_input)?;
            let process: &dyn Process = &fence;
            let keys = &bounded_keys;
            let retry = actions.receipt_exists(id);
            let fleets = navalact::own_fleets(process)?;
            let plan = if retry { None } else { Some(navalact::plan_create(&fleets, tf)?) };
            if !retry {
                bounded_keys.preflight()?;
            }
            let action = json!({"kind":"fleet_create","task_force":tf});
            let info = std::cell::RefCell::new(Value::Null);
            let count_before = fleets.len();
            let mut settle = || {
                let now = navalact::own_fleets(process)?;
                let verdict = match &plan {
                    Some(p) => navalact::settle_create(&now, p),
                    None => {
                        let alone = now.iter().any(|f| f.task_forces.len() == 1 && f.task_forces[0].name.trim().eq_ignore_ascii_case(tf.trim()));
                        if alone && now.len() > 1 { settle::Verdict::Reached } else { settle::Verdict::Unchanged }
                    }
                };
                let _ = count_before;
                Ok(verdict.outcome(json!({"fleets":now.iter().map(|f| f.name.trim()).collect::<Vec<_>>()})))
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let Some(plan) = &plan else {
                    return Err(Error::new("action_uncertain", "A retried request is never dispatched again"));
                };
                *info.borrow_mut() = navalact::create_fleet(process, keys, plan)?;
                Ok(())
            };
            let receipt = actions.execute_with(id, &epoch, action, &mut settle, &mut dispatch, true)?;
            return Ok(json!({"receipt":receipt,"result":*info.borrow()}));
        }
        if name == "hoi4_navy_ui" {
            allowed(
                args,
                &["op", "root", "path", "depth", "index", "items", "x", "y", "code", "letter", "pixels", "lease_token"],
            )?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let op = string(args, "op")?;
            let num = |k: &str| args.get(k).and_then(Value::as_f64).unwrap_or(0.0);
            return match op {
                "dump" => navalui::dump(process, args),
                "click" => {
                    self.keys.preflight()?;
                    self.keys.click(navalui::GAME, num("x"), num("y")).map(|p| json!({"clicked":format!("{p:?}")}))
                }
                "shiftclick" => {
                    self.keys.preflight()?;
                    self.keys.shift_click(navalui::GAME, num("x"), num("y")).map(|p| json!({"shift_clicked":format!("{p:?}")}))
                }
                "rclick" => {
                    self.keys.preflight()?;
                    self.keys.right_click(navalui::GAME, num("x"), num("y")).map(|p| json!({"rclicked":format!("{p:?}")}))
                }
                "code" => {
                    self.keys.preflight()?;
                    self.keys.press_code(num("code") as u16).map(|_| json!({"pressed":num("code")}))
                }
                "letter" => {
                    self.keys.preflight()?;
                    let c = string(args, "letter")?.chars().next().unwrap_or('z');
                    self.keys.press(Key::letter(c)?).map(|_| json!({"pressed":c.to_string()}))
                }
                "escape" => {
                    self.keys.preflight()?;
                    self.keys.press(Key::Escape).map(|_| json!({"pressed":"escape"}))
                }
                "find" => {
                    self.keys.preflight()?;
                    ui::find_state(process, &self.keys, num("x") as i32).map(|_| json!({"found":num("x")}))
                }
                "scroll" => {
                    self.keys.preflight()?;
                    self.keys.scroll(navalui::GAME, (num("x"), num("y")), num("pixels") as i32).map(|_| json!({"scrolled":num("pixels")}))
                }
                _ => Err(Error::new("invalid_arguments", "Unknown op")),
            };
        }
        if name == "hoi4_air_wings" {
            allowed(args, &["lease_token"])?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            self.keys.preflight()?;
            return air::list(process, &self.keys);
        }
        if name == "hoi4_air_region_probe" {
            allowed(args, &["state_id", "lease_token"])?;
            let state = airact::state_arg(args, "state_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            self.keys.preflight()?;
            return airact::probe_region(process, &self.keys, state);
        }
        if matches!(
            name,
            "hoi4_air_mission"
                | "hoi4_air_rebase"
                | "hoi4_air_wing_create"
                | "hoi4_air_wing_disband"
                | "hoi4_air_hold"
                | "hoi4_air_wing_split"
                | "hoi4_air_wing_merge"
        ) {
            enum Air {
                Mission { wing: String, mission: String, state: i32, region: Option<i32>, mask: i32 },
                Rebase { wing: String, state: i32 },
                Create { state: i32, aircraft: String },
                Disband { wing: String },
                Hold { wing: String },
                Split { wing: String },
                Merge { wing: String, with: Option<String> },
            }
            let op = match name {
                "hoi4_air_mission" => {
                    allowed(args, &["wing", "mission", "state_id", "region", "request_id", "lease_token"])?;
                    let wing = airact::text_arg(args, "wing", 128)?;
                    let mission = airact::text_arg(args, "mission", 40)?;
                    let Some(&(_, frame)) = air::MISSIONS.iter().find(|(n, _)| *n == mission) else {
                        return Err(Error::new(
                            "invalid_arguments",
                            format!(
                                "mission must be one of {}",
                                air::MISSIONS.iter().map(|m| m.0).collect::<Vec<_>>().join(", ")
                            ),
                        ));
                    };
                    let (state, region) = airact::resolve_region(
                        process,
                        args.get("state_id").map(|_| airact::state_arg(args, "state_id")).transpose()?,
                        args.get("region").map(|_| airact::text_arg(args, "region", 96)).transpose()?,
                    )?;
                    Air::Mission { wing, mission, state, region, mask: 1 << (frame - 1) }
                }
                "hoi4_air_rebase" => {
                    allowed(args, &["wing", "state_id", "request_id", "lease_token"])?;
                    Air::Rebase { wing: airact::text_arg(args, "wing", 128)?, state: airact::state_arg(args, "state_id")? }
                }
                "hoi4_air_wing_create" => {
                    allowed(args, &["state_id", "aircraft", "request_id", "lease_token"])?;
                    Air::Create { state: airact::state_arg(args, "state_id")?, aircraft: airact::text_arg(args, "aircraft", 96)? }
                }
                "hoi4_air_wing_disband" => {
                    allowed(args, &["wing", "request_id", "lease_token"])?;
                    Air::Disband { wing: airact::text_arg(args, "wing", 128)? }
                }
                "hoi4_air_wing_split" => {
                    allowed(args, &["wing", "request_id", "lease_token"])?;
                    Air::Split { wing: airact::text_arg(args, "wing", 128)? }
                }
                "hoi4_air_wing_merge" => {
                    allowed(args, &["wing", "with", "request_id", "lease_token"])?;
                    Air::Merge {
                        wing: airact::text_arg(args, "wing", 128)?,
                        with: args.get("with").map(|_| airact::text_arg(args, "with", 128)).transpose()?,
                    }
                }
                _ => {
                    allowed(args, &["wing", "request_id", "lease_token"])?;
                    Air::Hold { wing: airact::text_arg(args, "wing", 128)? }
                }
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
                until: (Instant::now() + Duration::from_secs(120)).min(lease_end),
                stop: None,
            };
            let check_input = || fence.input_check();
            let bounded_keys = self.keys.guarded(&fence, &check_input)?;
            let process: &dyn Process = &fence;
            let keys = &bounded_keys;
            let retry = actions.receipt_exists(id);
            let (bases, wings) = airact::own(process)?;
            // Pure refusals first; the plan of an already journaled request is never rebuilt.
            let mut rebase_plan = None;
            let mut create_plan = None;
            let mut split_plan = None;
            let mut merge_plan = None;
            let mut before: Option<airact::WingSnap> = None;
            if !retry {
                match &op {
                    Air::Mission { wing, .. } => before = Some(airact::plan_disband(&wings, wing)?),
                    Air::Rebase { wing, state } => {
                        rebase_plan = Some(airact::plan_rebase(&bases, &wings, wing, *state)?);
                    }
                    Air::Create { state, aircraft } => {
                        create_plan = Some(airact::plan_create(&bases, &wings, *state, aircraft)?);
                    }
                    Air::Disband { wing } => before = Some(airact::plan_disband(&wings, wing)?),
                    Air::Hold { wing } => before = Some(airact::plan_hold(&wings, wing)?),
                    Air::Split { wing } => split_plan = Some(airact::plan_split(&wings, wing)?),
                    Air::Merge { wing, with } => merge_plan = Some(airact::plan_merge(&wings, wing, with.as_deref())?),
                }
                bounded_keys.preflight()?;
            }
            let action = match &op {
                Air::Mission { wing, mission, state, region, .. } => {
                    json!({"kind":"air_mission","wing":wing,"mission":mission,"state_id":state,"region":region})
                }
                Air::Rebase { wing, state } => json!({"kind":"air_rebase","wing":wing,"state_id":state}),
                Air::Create { state, aircraft } => json!({"kind":"air_wing_create","state_id":state,"aircraft":aircraft}),
                Air::Disband { wing } => json!({"kind":"air_wing_disband","wing":wing}),
                Air::Hold { wing } => json!({"kind":"air_hold","wing":wing}),
                Air::Split { wing } => json!({"kind":"air_wing_split","wing":wing}),
                Air::Merge { wing, with } => json!({"kind":"air_wing_merge","wing":wing,"with":with}),
            };
            let info = std::cell::RefCell::new(Value::Null);
            let mut settle = || {
                let (bases_now, wings_now) = airact::own(process)?;
                let snap_of = |n: &str| wings_now.iter().find(|w| w.name == n).map(airact::snap);
                Ok(match &op {
                    Air::Mission { wing, mask, region, .. } => {
                        let now = snap_of(wing);
                        let verdict = match (&before, retry) {
                            (Some(b), false) => airact::settle_mission(b, now.as_ref(), *mask, *region),
                            _ => match &now {
                                Some(n) if n.mission_mask & mask == *mask && region.is_none_or(|r| n.region_id == r) => settle::Verdict::Reached,
                                Some(_) => settle::Verdict::Unchanged,
                                None => settle::Verdict::Unknown,
                            },
                        };
                        verdict.outcome(json!({"wing":now.map(|n| json!({"mission_mask":n.mission_mask,"region_id":n.region_id}))}))
                    }
                    Air::Rebase { wing, state } => {
                        let now = snap_of(wing);
                        let to = bases_now.iter().find(|b| b.state == Some(*state)).map(|b| b.id).unwrap_or(-1);
                        let verdict = match (&rebase_plan, retry) {
                            (Some(p), false) => airact::settle_rebase(&p.before, now.as_ref(), p.to_base_id),
                            _ => match &now {
                                Some(n) if n.base_id == to || n.transfer_base_id == to => settle::Verdict::Reached,
                                Some(_) => settle::Verdict::Unchanged,
                                None => settle::Verdict::Unknown,
                            },
                        };
                        verdict.outcome(json!({"wing":now.map(|n| json!({"base_id":n.base_id,"transfer_to_base_id":n.transfer_base_id}))}))
                    }
                    Air::Create { .. } => match &create_plan {
                        Some(p) => {
                            let (verdict, added) = airact::settle_create(p, &wings_now);
                            verdict.outcome(json!({"new_wings":added}))
                        }
                        None => journal::Outcome::Unknown,
                    },
                    Air::Disband { wing } => match &before {
                        Some(b) => airact::settle_disband(b, &wings_now).outcome(json!({"wing_present":snap_of(wing).is_some()})),
                        None => {
                            if snap_of(wing).is_none() { journal::Outcome::Reached(json!({"wing_present":false})) } else { journal::Outcome::Unchanged(json!({"wing_present":true})) }
                        }
                    },
                    Air::Split { .. } => match &split_plan {
                        Some(p) => {
                            let (verdict, added) = airact::settle_split(p, &wings_now);
                            verdict.outcome(json!({"new_wings":added}))
                        }
                        None => journal::Outcome::Unknown,
                    },
                    Air::Merge { wing, .. } => match &merge_plan {
                        Some(p) => airact::settle_merge(p, &wings_now)
                            .outcome(json!({"wing_present":snap_of(wing).is_some(),"group_left":p.group.iter().filter(|n| snap_of(n).is_some()).count()})),
                        None => journal::Outcome::Unknown,
                    },
                    Air::Hold { wing } => {
                        let now = snap_of(wing);
                        let verdict = match (&before, retry) {
                            (Some(b), false) => airact::settle_hold(b, now.as_ref()),
                            _ => match &now {
                                Some(n) if n.region_id <= 0 => settle::Verdict::Reached,
                                Some(_) => settle::Verdict::Unchanged,
                                None => settle::Verdict::Unknown,
                            },
                        };
                        verdict.outcome(json!({"wing":now.map(|n| json!({"mission_mask":n.mission_mask,"region_id":n.region_id}))}))
                    }
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                if retry {
                    return Err(Error::new("action_uncertain", "A retried request is never dispatched again"));
                }
                *info.borrow_mut() = match &op {
                    Air::Mission { wing, mission, state, .. } => {
                        airact::mission(process, keys, wing, mission, *state)?;
                        json!({"wing":wing,"mission":mission,"state_id":state})
                    }
                    Air::Rebase { .. } => airact::rebase(process, keys, rebase_plan.as_ref().ok_or_else(|| Error::new("invalid_arguments", "No plan"))?)?,
                    Air::Create { .. } => airact::create_wing(process, keys, create_plan.as_ref().ok_or_else(|| Error::new("invalid_arguments", "No plan"))?)?,
                    Air::Disband { wing } => airact::disband(process, keys, wing)?,
                    Air::Hold { wing } => airact::hold(process, keys, wing)?,
                    Air::Split { .. } => airact::split(process, keys, split_plan.as_ref().ok_or_else(|| Error::new("invalid_arguments", "No plan"))?)?,
                    Air::Merge { .. } => airact::merge(process, keys, merge_plan.as_ref().ok_or_else(|| Error::new("invalid_arguments", "No plan"))?)?,
                };
                Ok(())
            };
            let receipt = actions.execute_with(id, &epoch, action, &mut settle, &mut dispatch, true)?;
            let (_, after) = airact::own(process)?;
            let after_rows: Vec<Value> = match &op {
                Air::Mission { wing, .. } | Air::Rebase { wing, .. } | Air::Hold { wing } => after
                    .iter()
                    .filter(|w| w.name == *wing)
                    .map(|w| json!({"wing":w.name,"aircraft":w.planes,"air_base_state":w.base_state,
                        "transfer_to_base_id":(w.transfer_base_id > 0).then_some(w.transfer_base_id),
                        "missions":if w.region_id > 0 { airread::missions_in(w.mission_mask) } else { Vec::new() },
                        "standing_by":w.region_id <= 0,"region_id":(w.region_id > 0).then_some(w.region_id),
                        "region":w.region_name}))
                    .collect(),
                Air::Create { state, .. } => after
                    .iter()
                    .filter(|w| w.base_state == Some(*state))
                    .map(|w| json!({"wing":w.name,"aircraft":w.planes,"aircraft_type":w.equipment,"variant":w.variant}))
                    .collect(),
                Air::Disband { .. } => Vec::new(),
                Air::Split { wing } | Air::Merge { wing, .. } => {
                    let base = after.iter().find(|w| w.name == *wing).map(|w| w.base_id);
                    after
                        .iter()
                        .filter(|w| Some(w.base_id) == base)
                        .map(|w| json!({"wing":w.name,"aircraft":w.planes,"aircraft_type":w.equipment}))
                        .collect()
                }
            };
            return Ok(json!({"receipt":receipt,"result":*info.borrow(),"wings_after":after_rows}));
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
