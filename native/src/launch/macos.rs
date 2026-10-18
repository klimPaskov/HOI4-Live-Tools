//! Ordinary input for one owned macOS game: exclusive lease, durable
//! journals, CGEvent keys and clicks only while the owned window is front-most,
//! and fresh memory readback for every step. The clock keeps the Linux tool
//! contract; research starts are driven by live GUI geometry.
mod air;
mod armies;
mod construction;
mod decisions;
mod deployment;
mod diplomacy;
mod events;
mod focus;
mod journal;
mod mapdata;
mod politics;
mod production;
mod research;
mod states;
mod ui;
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
    sync::Mutex,
    thread,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    controller: String,
    player: String,
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
}
impl Guard {
    /// Fresh own clock state; refuses another game, campaign or a rewound date.
    fn observe(&mut self, process: &dyn Process) -> Result<State> {
        if process.identity() != Some(&self.epoch.game) {
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
        if context.anchors != self.anchors
            || context.playthrough_id != self.playthrough
            || context.player != self.epoch.player
            || context.date_raw < self.latest_date
        {
            return Err(Error::new(
                "ownership_changed",
                "Campaign context changed; a fresh owned run is required",
            ));
        }
        self.latest_date = context.date_raw;
        Ok(State {
            epoch: self.epoch.clone(),
            paused: context.paused,
            speed: context.speed,
        })
    }
}

struct Control {
    guard: Guard,
    authority: InputAuthority,
    journal: Journal,
    actions: journal::GameJournal,
}

pub struct MacClock {
    keys: MacKeys,
    state: Mutex<Control>,
}
impl MacClock {
    /// Bind to the owned process already opened from the session manifest.
    pub fn bind(process: &dyn Process, directory: &Path) -> Result<Self> {
        let config: Config = serde_json::from_slice(&std::fs::read(directory.join("input.json"))?)
            .map_err(|e| Error::new("invalid_input_binding", e.to_string()))?;
        let game_identity = process
            .identity()
            .cloned()
            .ok_or_else(|| Error::new("input_unavailable", "Owned process identity required"))?;
        let context = game::clock_context(process)?;
        if context.player != config.player {
            return Err(Error::new(
                "invalid_input_binding",
                "Configured player differs from the owned campaign",
            ));
        }
        let epoch = Epoch {
            game: game_identity.clone(),
            campaign: format!("{}:{}", nonce()?, context.playthrough_id),
            player: config.player,
        };
        Ok(Self {
            keys: MacKeys::new(game_identity.pid),
            state: Mutex::new(Control {
                authority: InputAuthority::new(config.controller, epoch.clone())?,
                journal: Journal::open(&directory.join("clock-actions.jsonl"))?,
                actions: journal::GameJournal::open(&directory.join("game-actions.jsonl"))?,
                guard: Guard {
                    epoch,
                    anchors: context.anchors,
                    playthrough: context.playthrough_id,
                    latest_date: context.date_raw,
                },
            }),
        })
    }
}

struct ClockBackend<'a> {
    process: &'a dyn Process,
    keys: &'a MacKeys,
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

impl Commands for MacClock {
    fn tools(&self) -> Vec<Value> {
        vec![
            json!({"name":"hoi4_input_lease","description":"Exclusive configured-controller lease for ordinary macOS input. Acquire uses holder; renew/release use only lease_token. Status takes only op. Observations do not need a lease.","inputSchema":{"type":"object","oneOf":[
                {"properties":{"op":{"const":"acquire"},"holder":{"type":"string"},"seconds":{"type":"integer","minimum":30,"maximum":900}},"required":["op","holder"],"additionalProperties":false},
                {"properties":{"op":{"const":"renew"},"lease_token":{"type":"string"},"seconds":{"type":"integer","minimum":30,"maximum":900}},"required":["op","lease_token"],"additionalProperties":false},
                {"properties":{"op":{"const":"release"},"lease_token":{"type":"string"}},"required":["op","lease_token"],"additionalProperties":false},
                {"properties":{"op":{"const":"status"}},"required":["op"],"additionalProperties":false}]}}),
            json!({"name":"hoi4_research_start","description":"Start an own technology in an empty research slot with ordinary input (research panel, slot, folder tab, tile, Research), each click located from the live GUI and each stage verified from memory; succeeds only when the slot holds the technology. Refuses researched, already-assigned or busy-slot targets before any input. A tile scrolled out of view is refused. Stable request_id; an unresolved dispatch is settled only by retrying its own request_id.","inputSchema":{"type":"object","properties":{"technology":{"type":"string","maxLength":128},"slot":{"type":"integer","minimum":0,"maximum":15},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["technology","slot","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_focus_start","description":"Start an own national focus with ordinary input (politics panel, focus button, drag-panning the tree until the node is in view, the node, Start), each target located from the live GUI and each stage verified from memory; succeeds only when own focus progress names the focus as active. Refuses completed or already-active focuses, or any while another focus is active, before input. Stable request_id; an unresolved dispatch is settled only by retrying its own request_id.","inputSchema":{"type":"object","properties":{"focus":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["focus","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_production_add","description":"Add one own military production line for a base equipment type (e.g. infantry_equipment, artillery_equipment, light_tank_equipment, fighter_equipment) with ordinary input: production panel, category button, the design in the selection window, then +/- until the line requests the given factories. Verified from the own line collection. Refuses equipment with no producible design. Naval lines are not supported yet.","inputSchema":{"type":"object","properties":{"equipment":{"type":"string","maxLength":128},"factories":{"type":"integer","minimum":1,"maximum":150},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["equipment","factories","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_production_set","description":"Set the requested factories of an own production line, addressed by its entry_index and equipment_id from hoi4_production_lines, with the line's ordinary +/- buttons; each click is verified from memory. 0 keeps the line with no factories.","inputSchema":{"type":"object","properties":{"line":{"type":"integer","minimum":0,"maximum":2047},"equipment":{"type":"string","maxLength":128},"factories":{"type":"integer","minimum":0,"maximum":150},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["line","equipment","factories","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_construction_add","description":"Queue more of a building in an own state with ordinary input: construction panel, the building button, the game's Find view to centre the state (F, state_id, Return), then clicks at the screen centre. Verified from the own construction queue (the building located in that state); placement mode is switched off afterwards. Building keys: arms_factory, industrial_complex, dockyard, infrastructure, air_base, anti_air_building, radar_station, synthetic_refinery, fuel_silo, rocket_site, ... (state buildings only).","inputSchema":{"type":"object","properties":{"building":{"type":"string","maxLength":64},"state_id":{"type":"integer","minimum":1,"maximum":4095},"count":{"type":"integer","minimum":1,"maximum":10,"default":1},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["building","state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_politics_options","description":"List the ideas offered for one politics slot (opens the slot's selection window and closes it again; changes nothing). category: government (slots 0 mobilization law, 1 trade law, 2 economy law, 3-5 political advisors), research_production (designers) or military_staff. Returns each idea key with the game's enabled state and the slot's current idea.","inputSchema":{"type":"object","properties":{"category":{"type":"string","enum":["government","research_production","military_staff"]},"slot":{"type":"integer","minimum":0,"maximum":7},"lease_token":{"type":"string"}},"required":["category","slot","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_politics_choose","description":"Take an idea (law, advisor, designer) for one politics slot with ordinary input; verified when the slot holds the idea in memory. Refuses ideas the game disables. Same category/slot numbering as hoi4_politics_options.","inputSchema":{"type":"object","properties":{"category":{"type":"string","enum":["government","research_production","military_staff"]},"slot":{"type":"integer","minimum":0,"maximum":7},"idea":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["category","slot","idea","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_decisions_list","description":"Open the decisions panel and list offered decisions (key, enabled by the game, whether a timer for it is running). Changes nothing.","inputSchema":{"type":"object","properties":{"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_decision_take","description":"Take an own decision with ordinary input (decisions panel, its row, select; the game's confirmation for exactly this decision is accepted). Verified when the decision view shows it taken (timer started, entry gone or disabled). Refuses decisions the game disables.","inputSchema":{"type":"object","properties":{"decision":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["decision","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_division_templates","description":"Own division template names (for hoi4_train) and how many training lines each has queued. Pure read; no lease needed.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
            json!({"name":"hoi4_train","description":"Queue divisions of an own template with ordinary input (deployment panel, the template's Train button once per division) and give every training line without a location the own state deploy_state_id, since a line without a location never deploys. Each click is verified by one more training line; each location by the line's deployment province.","inputSchema":{"type":"object","properties":{"template":{"type":"string","maxLength":128},"count":{"type":"integer","minimum":1,"maximum":24},"deploy_state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["template","count","deploy_state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_deploy_locations","description":"Give every training line without a deployment location the own state state_id (Find view, the line's location button, one click on the state). Lines without a location never deploy. Verified by the lines' deployment provinces.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_armies","description":"Own armies (name, division ids) and unassigned division ids. Pure read; no lease needed.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
            json!({"name":"hoi4_army_front","description":"Give an own army a front-line order along the border nearest a state: army card, front-line drawing (Z), Find view centres the state, one click on the nearest highlighted border. Verified by one more battle-plan order on the army.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_army_plan","description":"Execute or stop an own army's battle plan with the army card's play or stop button. Verified by the card's executing state.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"execute":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","execute","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_army_offensive","description":"Draw an offensive line for an own army that already holds a front line, into a target state: army card, Find view centres the state, offensive-line drawing (X), one right-button drag from the army's side to the state. Verified by new battle-plan orders on the army; execute the plan afterwards.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_states","description":"Find states by English name (accent-insensitive substring, or the exact ID), or list every state a country owns or controls by its tag (e.g. GER), with owner and controller as the political map shows them. Pure read.","inputSchema":{"type":"object","properties":{"query":{"type":"string"}},"required":["query"],"additionalProperties":false}}),
            json!({"name":"hoi4_events","description":"Open event windows in order (the last is on top) with their title and option texts. Pure read; no lease needed.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
            json!({"name":"hoi4_event_choose","description":"Choose an option (0-based, in listed order) of the open event on top by clicking it. Verified when exactly that event window closes.","inputSchema":{"type":"object","properties":{"event":{"type":"string"},"option":{"type":"integer","minimum":0,"maximum":7},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["event","option","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_diplomacy","description":"Open the diplomacy view for the owner of a state (Find view centres the state, one click selects the country) and list the offered actions with enabled state, cost and whether the AI would refuse. Changes nothing in the game.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"lease_token":{"type":"string"}},"required":["state_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_diplomacy_act","description":"Take one diplomatic action (exact listed name, e.g. Declare war, Justify War goal, Guarantee independence, Improve relations) toward the owner of a state. Justify War goal also needs war_goal (e.g. Conquer) and 1..5 states by name. Verified when the offered actions change or political power is spent.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"action":{"type":"string"},"war_goal":{"type":"string"},"states":{"type":"array","items":{"type":"string"},"maxItems":5},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["state_id","action","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_army_commander","description":"Assign a general (exact name as listed) to an own army from the army panel's commander list. Verified when the army's commander is that general.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"general":{"type":"string"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","general","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_air_wings","description":"Own air wings with air base, assigned region and missions, from the Air Forces Overview (opens it). Changes nothing in the game.","inputSchema":{"type":"object","properties":{"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_air_mission","description":"Give an own air wing (exact name) a mission over the strategic region containing a state: wing row, mission button, Find view centres the state, right click assigns. Missions: air_superiority, cas, interception, strategic_bombing, naval_strike, kamikaze, port_strike, logistics_strike, air_supply, training, naval_mines, naval_minesweeping, recon, naval_patrol. Verified when the wing shows a changed region or mission.","inputSchema":{"type":"object","properties":{"wing":{"type":"string"},"mission":{"type":"string"},"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["wing","mission","state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_army_create","description":"Create one army from the own divisions shown around a state: Find view centres the state, a box selection picks own divisions there, the new-army card creates the army. Verified by a new army in memory; returns its divisions.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["state_id","request_id","lease_token"],"additionalProperties":false}}),
            json!({"name":"hoi4_time","description":"Set desired pause or internal speed 0..4 with ordinary keys (Space, keypad +/-) sent only while the owned game window is front-most, each verified by fresh memory readback. Supply exactly one target and a stable request_id; uncertain dispatches are never replayed.","inputSchema":{"type":"object","properties":{"paused":{"type":"boolean"},"speed":{"type":"integer","minimum":0,"maximum":4},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}}),
        ]
    }
    fn supports(&self, name: &str) -> bool {
        matches!(
            name,
            "hoi4_input_lease"
                | "hoi4_time"
                | "hoi4_research_start"
                | "hoi4_focus_start"
                | "hoi4_production_add"
                | "hoi4_production_set"
                | "hoi4_construction_add"
                | "hoi4_politics_options"
                | "hoi4_politics_choose"
                | "hoi4_decisions_list"
                | "hoi4_decision_take"
                | "hoi4_division_templates"
                | "hoi4_train"
                | "hoi4_deploy_locations"
                | "hoi4_armies"
                | "hoi4_states"
                | "hoi4_events"
                | "hoi4_event_choose"
                | "hoi4_diplomacy"
                | "hoi4_diplomacy_act"
                | "hoi4_army_create"
                | "hoi4_army_front"
                | "hoi4_army_plan"
                | "hoi4_army_offensive"
                | "hoi4_army_commander"
                | "hoi4_air_wings"
                | "hoi4_air_mission"
        )
    }
    fn capabilities(&self) -> Value {
        json!({"transport":"ordinary macOS CGEvent keys to the front-most owned window","actions":["set_paused","set_speed","research_start","focus_start","production_add","production_set","construction_add","politics_options","politics_choose","decisions_list","decision_take","train","deploy_locations","army_create","army_front","army_plan","army_offensive","event_choose","diplomacy","diplomacy_act","army_commander","air_wings","air_mission"],"lease":"exclusive configured controller","native_posting":false,"full_campaign_acceptance":false})
    }
    fn call(&self, process: &dyn Process, name: &str, arguments: &Value) -> Result<Value> {
        let args = arguments
            .as_object()
            .ok_or_else(|| Error::new("invalid_arguments", "Input arguments must be an object"))?;
        if name == "hoi4_armies" {
            allowed(args, &[])?;
            return armies::armies(process);
        }
        if name == "hoi4_events" {
            allowed(args, &[])?;
            return events::list(process);
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
        let mut state = self
            .state
            .try_lock()
            .map_err(|_| Error::new("input_busy", "Another input operation is in progress"))?;
        let Control {
            guard,
            authority,
            journal,
            actions,
        } = &mut *state;
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
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let action = json!({"kind":"research_start","technology":key,"slot":slot});
            let before = research::assignments(process)?;
            if actions.receipt_exists(id) {
                // A retry only settles or replays its receipt; it never refuses on new state.
            } else {
                research::check(&before, key, slot)?;
                self.keys.preflight()?;
            }
            let keys = &self.keys;
            let mut settle = || {
                let now = research::assignments(process)?;
                let target = now.slots.get(slot).cloned().flatten();
                let others_kept = now
                    .slots
                    .iter()
                    .enumerate()
                    .all(|(i, s)| i == slot || before.slots.get(i) == Some(s));
                let detail = json!({"slots":now.slots});
                Ok(if target.as_deref() == Some(key) {
                    journal::Outcome::Reached(detail)
                } else if target.is_none() && others_kept {
                    journal::Outcome::Unchanged(detail)
                } else {
                    journal::Outcome::Unknown
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                research::start(process, keys, key, slot)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
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
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
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
                self.keys.preflight()?;
            }
            let keys = &self.keys;
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
            let retry = actions.receipt_exists(id);
            let action = json!({"kind":"diplomacy_act","state_id":state,"action":action_name,"war_goal":kind,"states":states});
            let keys = &self.keys;
            // The dispatch verifies the change itself; its result decides.
            let verified = std::cell::Cell::new(false);
            let mut settle = || {
                let now = diplomacy::current(process)?;
                Ok(if verified.get() {
                    journal::Outcome::Reached(json!({"actions":now}))
                } else if retry {
                    journal::Outcome::Diverged(json!({"actions":now}))
                } else {
                    journal::Outcome::Unchanged(json!({"actions":now}))
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
                } else if retry {
                    journal::Outcome::Diverged(json!({"wing":now}))
                } else {
                    journal::Outcome::Unchanged(json!({"wing":now}))
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
        if name == "hoi4_army_commander" {
            allowed(args, &["army", "general", "request_id", "lease_token"])?;
            let army = string(args, "army")?;
            let general = string(args, "general")?;
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
                } else if now > before {
                    journal::Outcome::Reached(detail)
                } else {
                    journal::Outcome::Unchanged(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                armies::offensive(process, keys, &game_dir, army, state)
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
            let game_dir = mapdata::game_dir(&epoch.game.exe)?;
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
                } else if now > before {
                    journal::Outcome::Reached(detail)
                } else {
                    journal::Outcome::Unchanged(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                armies::front(process, keys, &game_dir, army, state)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_army_create" {
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
            let before = armies::armies(process)?;
            let count = |v: &Value| v["armies"].as_array().map_or(0, Vec::len);
            let retry = actions.receipt_exists(id);
            if !retry {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"army_create","state_id":state});
            let keys = &self.keys;
            let mut settle = || {
                let now = armies::armies(process)?;
                Ok(if retry {
                    journal::Outcome::Diverged(now)
                } else if count(&now) > count(&before) {
                    journal::Outcome::Reached(now)
                } else {
                    journal::Outcome::Unchanged(now)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                armies::create(process, keys, state)
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
                } else if now >= start + count {
                    journal::Outcome::Reached(detail)
                } else if now == start {
                    journal::Outcome::Unchanged(detail)
                } else {
                    journal::Outcome::Diverged(detail)
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
                Ok(if now == 0 {
                    journal::Outcome::Reached(detail)
                } else if retry || now < start {
                    journal::Outcome::Diverged(detail)
                } else {
                    journal::Outcome::Unchanged(detail)
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
                } else if pp < pp_before || timer {
                    journal::Outcome::Reached(detail)
                } else {
                    journal::Outcome::Unchanged(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                decisions::take(process, keys, key)
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
                } else if now >= start + count {
                    journal::Outcome::Reached(detail)
                } else if now == start {
                    journal::Outcome::Unchanged(detail)
                } else {
                    journal::Outcome::Diverged(detail)
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
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let action = json!({"kind":"focus_start","focus":key});
            if !actions.receipt_exists(id) {
                focus::check(&focus::progress(process)?, key)?;
                self.keys.preflight()?;
            }
            let keys = &self.keys;
            let mut settle = || {
                let now = focus::progress(process)?;
                let detail = json!({"active":now.active});
                Ok(match now.active.as_deref() {
                    Some(active) if active == key => journal::Outcome::Reached(detail),
                    None => journal::Outcome::Unchanged(detail),
                    Some(_) => journal::Outcome::Unknown,
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                focus::start(process, keys, key)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
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
