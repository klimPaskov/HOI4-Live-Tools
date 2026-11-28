//! Ordinary input for one owned macOS game: exclusive lease, durable
//! journals, CGEvent keys and clicks only while the owned window is front-most,
//! and fresh memory readback for every step. The clock keeps the Linux tool
//! contract; research starts are driven by live GUI geometry.
// Windows has no input backend yet, so nothing binds the management tools there: this code is
// compiled and unit-tested on Windows but not reachable (docs/PORTING_STATUS.md).
#![cfg_attr(windows, allow(dead_code))]
mod action_fence;
mod agency_act;
mod air;
mod airact;
mod airattach;
mod aircover;
mod airread;
mod airui;
mod alerts;
mod armies;
mod army_hq;
mod army_roles;
mod bop;
mod command_limit;
mod commanders;
mod construction;
mod construction_find;
mod fair_facts;
mod inproc;
mod inproc_construction;
mod inproc_diplomacy;
mod inproc_doctrine;
mod inproc_events;
mod inproc_holds;
mod inproc_mio;
mod inproc_moves;
mod inproc_navy;
mod inproc_orders;
mod inproc_picks;
mod inproc_politics;
mod inproc_projects;
mod inproc_research;
mod inproc_training;
mod inproc_units;
mod operation_act;
mod paradrop;
mod railway;
mod volunteers;
// Internal prospective click opener; no tool/configuration registration.
mod construction_find_open;
// Default-off single ordinary Escape qualification.
mod construction_cancel;
mod construction_open;
// Default-off one-action qualification; full campaign acceptance remains false.
mod abilities;
mod construction_place;
mod construction_select;
mod controller_epoch;
mod country_status;
mod daily;
mod daynight;
mod decisions;
mod deployment;
mod design_edit;
mod design_ui;
mod designer;
mod designs;
mod diplomacy;
mod diplomacy_keys;
mod diplomacy_state;
mod doctrine_act;
mod doctrines;
mod event_options;
mod events;
mod faction;
mod fleetmem;
mod fleetops;
mod weather;
// The input probe is used by the macOS main loop only.
#[cfg_attr(target_os = "linux", allow(dead_code, unused_imports))]
mod focus;
mod front_plan;
mod glide;
mod occlaw;
mod peace;
mod trade;
#[cfg(target_os = "macos")]
pub(crate) use focus::input_probe as focus_input_probe;
mod battles;
mod economy_detail;
mod input_gate;
mod intel_live;
mod intelligence;
mod invasion;
mod journal;
mod kit;
mod leader_act;
mod learn_tools;
#[cfg(test)]
mod live_designer;
#[cfg(test)]
mod live_designs;
#[cfg(test)]
mod live_reads;
#[cfg(test)]
mod live_reads_mgmt;
#[cfg(test)]
mod live_reads_navalair;
mod loc;
mod mapdata;
mod mgmt;
mod mio_act;
mod mios;
mod modlayer;
mod naval;
mod navalact;
mod navalui;
mod occupation;
mod panelkit;
mod politics;
mod production;
mod research;
mod research_extras;
mod resources;
mod screenmap;
mod settle;
mod special_project_rules;
mod special_projects;
mod state_table;
mod states;
mod stateslots;
mod stockpile;
mod supply;
mod supplymem;
mod template_edit;
mod template_get;
mod terrain;
#[cfg(test)]
mod testmem;
mod ui;
mod ui_action;
mod units;
mod war_stats;
mod wars;
use crate::{
    actions::{Action, Backend, Epoch, Journal, State},
    error::{Error, Result},
    game,
    input_lease::InputAuthority,
    platform::{
        game_input::{GameInput, Key},
        Process,
    },
    protocol::Commands,
    script,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    path::Path,
    sync::{Arc, Mutex},
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
                    crate::phase::sleep(Duration::from_millis(2));
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
    keys: Arc<dyn GameInput>,
    /// The pause watch on `keys`; every input lane brackets its actions with it.
    pause: Arc<crate::pause_guard::Watch>,
    state: Arc<Mutex<Control>>,
    process: Arc<dyn Process + Send + Sync>,
}
/// Where the game writes its saves: the run's own user folder for a game this runtime launched
/// (`-userdir` RUN/userdata), the game's user folder for a game the user started and the runtime
/// attached to (its `-userdir` argument, else Documents/Paradox Interactive/Hearts of Iron IV).
fn save_folder(directory: &Path) -> std::path::PathBuf {
    let own = directory.join("userdata").join("save games");
    let session = std::fs::read(directory.join("session.json"))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok());
    let Some(session) = session.filter(|s| s["attached"] == json!(true)) else {
        return own;
    };
    session["game"]["exe"]
        .as_str()
        .and_then(modlayer::userdir)
        .map(|d| d.join("save games"))
        .filter(|d| d.is_dir())
        .unwrap_or(own)
}
impl MacClock {
    /// Bind to the owned process already opened from the session manifest.
    #[cfg(target_os = "macos")]
    pub fn bind(process: Arc<dyn Process + Send + Sync>, directory: &Path) -> Result<Self> {
        let keys = Arc::new(crate::platform::macos_input::MacKeys::bind_owned(
            process.clone(),
        )?);
        Self::bind_with_input(process, directory, keys)
    }
    /// Bind with an already-verified platform input binding for the same process (macOS
    /// `MacKeys`, Linux `X11Input`; the entry point a Windows input backend will use).
    pub(crate) fn bind_with_input(
        process: Arc<dyn Process + Send + Sync>,
        directory: &Path,
        keys: Arc<dyn GameInput>,
    ) -> Result<Self> {
        let config: Config = serde_json::from_slice(&std::fs::read(directory.join("input.json"))?)
            .map_err(|e| Error::new("invalid_input_binding", e.to_string()))?;
        // Every key of every tool, the executor and the daily worker goes through the watch.
        let pause = Arc::new(crate::pause_guard::Watch::for_process(process.clone()));
        let keys: Arc<dyn GameInput> = Arc::new(crate::pause_guard::Watched::new(
            Box::new(keys),
            pause.clone(),
        ));
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
            pause,
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
    keys: &'a dyn crate::platform::game_input::GameInput,
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
        // An open focus tree's search box would take the key as text.
        let _ = panelkit::close_focus_tree(self.process, self.keys);
        self.keys.press_clock(key)?;
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
            crate::phase::sleep(Duration::from_millis(20));
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
/// The slot layout of the base game, said when a politics call names no slot or a wrong one
/// (players sent 48 such calls in the runs of 11-23 Nov).
const POLITICS_SLOT_HELP: &str = "slot must be 0..7: the slot's position in its category \
    (base game: government 0 mobilization law, 1 trade law, 2 economy law, 3-5 political advisors; \
    research_production 0-3 designers or design companies, 4 industrial concern, 5 theorist; \
    military_staff 0 chief of army, 1 chief of navy, 2 chief of air force, 3-5 high command); \
    the reply's `slots` lists every slot of the category with its holder";
/// The politics category of a call; words players used for one are taken as it, and a wrong
/// one is refused with the three categories and where officer corps spirits are chosen.
fn politics_category(args: &serde_json::Map<String, Value>) -> Result<&'static str> {
    let given = args.get("category").and_then(Value::as_str).unwrap_or("");
    let word = given.trim().to_ascii_lowercase().replace([' ', '-'], "_");
    Ok(match word.as_str() {
        "government" | "law" | "laws" | "advisor" | "advisors" | "political"
        | "political_advisor" | "political_advisors" => "government",
        "research_production" | "designer" | "designers" | "industry" | "theorist" => {
            "research_production"
        }
        "military_staff" | "military" | "staff" | "high_command" | "chiefs" => "military_staff",
        "officer_corps" => {
            return Err(Error::new(
                "invalid_arguments",
                "officer corps spirits are read and picked with hoi4_officer_corps (list:true reads them)",
            ))
        }
        _ => {
            return Err(Error::new(
                "invalid_arguments",
                format!(
                    "category {given:?} unknown: one of government, research_production, military_staff"
                ),
            ))
        }
    })
}
fn allowed(args: &serde_json::Map<String, Value>, keys: &[&str]) -> Result<()> {
    // A request id or lease token on a read is harmless: players add them by habit and a
    // refusal only costs a call (Ethiopia continuation, 6 Nov 2026).
    if let Some(bad) = args
        .keys()
        .find(|v| !keys.contains(&v.as_str()) && *v != "request_id" && *v != "lease_token")
    {
        return Err(Error::new(
            "invalid_arguments",
            format!(
                "Unexpected input argument {bad:?}; this tool takes: {}",
                keys.iter()
                    .filter(|k| !matches!(**k, "request_id" | "lease_token"))
                    .copied()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
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
        // Probe-only (macprobe): raw bytes of an own army object, for mapping fields.
        n if n.starts_with("dbg_army_bytes:") => armies::debug_bytes(process, &n[15..]),
        n if n.starts_with("dbg_rows:") => armies::debug_rows(process, &n[9..]),
        n if n.starts_with("dbg_wing:") => airact::debug_wing(process, &n[9..]),
        n if n.starts_with("dbg_card:") => armies::debug_card(process, &n[9..]),
        n if n.starts_with("dbg_hex:") => occupation::debug_hex(process, &n[8..]),
        n if n.starts_with("dbg_country:") => occupation::debug_country(process, &n[12..]),
        "dbg_states" => occupation::debug_states(process),
        n if n.starts_with("dbg_provinces:") => occupation::debug_provinces(process, &n[14..]),
        "dbg_event" => events::debug_event(process),
        "hoi4_war_stats" => war_stats::stats(process, &serde_json::Map::new()),
        "hoi4_alerts" => alerts::alerts(process),
        "hoi4_war_stats_all" => {
            let mut args = serde_json::Map::new();
            args.insert("all".into(), json!(true));
            war_stats::stats(process, &args)
        }
        "hoi4_occupation" | "hoi4_occupation_all" => {
            let exe = &process
                .identity()
                .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
                .exe;
            let mut args = serde_json::Map::new();
            if name.ends_with("_all") {
                args.insert("scope".into(), json!("all"));
            }
            occupation::occupation(process, &mapdata::game_dir(exe)?, &args)
        }
        n if n.starts_with("dbg_owner:") => panelkit::debug_owner(process, &n[10..]),
        n if n.starts_with("dbg_deep:") => panelkit::debug_deep(process, &n[9..]),
        n if n.starts_with("dbg_widget:") => panelkit::debug_widget_anywhere(process, &n[11..]),
        "dbg_alerts" => panelkit::debug_alerts(process),
        n if n.starts_with("dbg_widget_idler:") => panelkit::debug_widget_idler(process, &n[17..]),
        n if n.starts_with("dbg_image_save:") => panelkit::debug_image_save(process, &n[15..]),
        "dbg_popups" => panelkit::debug_popups(process),
        n if n.starts_with("dbg_container:") => {
            let a = u64::from_str_radix(n[14..].trim_start_matches("0x"), 16).unwrap_or(0);
            ui::read(process, |r, i| {
                let as_container = crate::gui_tree::container(r, a, i).ok().flatten();
                let target = as_container.unwrap_or(a);
                let name = crate::gui_tree::name(r, target, i)
                    .ok()
                    .flatten()
                    .or_else(|| crate::gui_tree::name(r, a, i).ok().flatten());
                let kids: Vec<serde_json::Value> = crate::gui_tree::children(r, target)
                    .unwrap_or_default()
                    .into_iter()
                    .take(30)
                    .map(|c| serde_json::json!(crate::gui_tree::name(r, c, i).ok().flatten()))
                    .collect();
                Ok(
                    serde_json::json!({"container": as_container.map(|c| format!("{c:#x}")), "name": name, "children": kids, "vtable": format!("{:#x}", r.u64(a).unwrap_or(0).wrapping_sub(i))}),
                )
            })
        }
        n if n.starts_with("dbg_scan_view:") => {
            // dbg_scan_view:SLOT[/HOP/HOP...]:NAME (hex)
            let (path, name) = n[14..].split_once(':').unwrap_or(("128", ""));
            let mut parts = path
                .split('/')
                .map(|p| u64::from_str_radix(p, 16).unwrap_or(0));
            let slot = parts.next().unwrap_or(0x128);
            let hops: Vec<u64> = parts.collect();
            ui::read(process, |r, i| {
                panelkit::scan_path_for_root(r, i, slot, &hops, name)
            })
            .map(|v| serde_json::json!(v))
        }
        n if n.starts_with("dbg_slot_object:") => panelkit::debug_slot_object(process, &n[16..]),
        "dbg_mio_list" => mio_act::debug_list(process),
        "dbg_line_roster" => mio_act::debug_line_roster(process),
        n if n.starts_with("dbg_find_text:") => mio_act::debug_find_text(process, &n[14..]),
        "dbg_factions" => diplomacy_state::debug_factions(process),
        "dbg_views" => panelkit::debug_views(process),
        n if n.starts_with("dbg_find_root:") => panelkit::debug_find_root(process, &n[14..]),
        n if n.starts_with("dbg_tree:") => panelkit::debug_tree(process, &n[9..]),
        n if n.starts_with("dbg_bytes:") => panelkit::debug_bytes(process, &n[10..]),
        "dbg_camera" => ui::read(process, |r, i| {
            let v = screenmap::view_matrix(r, i)?;
            Ok(json!({"height": screenmap::camera_height(r, i)?, "x": v[12], "z": v[14]}))
        }),
        _ => Err(Error::new(
            "capability_unavailable",
            "Unknown read-only tool",
        )),
    }
}

impl MacClock {
    /// Refuse a declaration of war when a country that would join the war borders the
    /// player and the shared border is not held: every own border state toward it needs a
    /// division and the border at least two divisions per state.
    fn check_war_widening(&self, process: &dyn Process, target: &str) -> Result<()> {
        let (joiners, weak) = Self::war_risk(process, target)?;
        if weak.is_empty() {
            return Ok(());
        }
        Err(Error::new(
            "war_would_widen",
            format!(
                "Declaring on {target} also brings in {} (faction, guarantors, overlord/subjects). Borders not held \
                 (need a division in every border state and two per state): {}. Nothing was sent. Cover these \
                 borders with fronts first (hoi4_front_plan, hoi4_army_front), or pass accept_widening:true \
                 to declare anyway.",
                if joiners.is_empty() { "nobody else".to_string() } else { joiners.join(", ") },
                weak.join("; ")
            ),
        ))
    }
    /// (countries that would join a war on `target`, borders with them that are not held).
    fn war_risk(process: &dyn Process, target: &str) -> Result<(Vec<String>, Vec<String>)> {
        let joiners = diplomacy_state::war_joiners(process, target)?;
        let exe = &process
            .identity()
            .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
            .exe;
        let report = wars::wars(process, &mapdata::game_dir(exe)?, &serde_json::Map::new())?;
        let mut weak = Vec::new();
        for row in report["borders"].as_array().into_iter().flatten() {
            let Some(tag) = row["neighbour_tag"].as_str() else {
                continue;
            };
            if tag != target && !joiners.iter().any(|j| j == tag) {
                continue;
            }
            let states = row["own_border_states"].as_array().map_or(0, Vec::len) as i64;
            let held = row["own_divisions_in_border_states"].as_i64().unwrap_or(0);
            let uncovered = row["uncovered_own_border_states"]
                .as_array()
                .map_or(0, Vec::len);
            if uncovered > 0 || held < 2 * states {
                weak.push(format!(
                    "{tag}: {held} divisions in {states} border states, {uncovered} empty"
                ));
            }
        }
        Ok((joiners, weak))
    }
}

/// Every macOS tool descriptor (name, description, input schema); no session needed, so the
/// catalogue can be listed offline (`hoi4-live-native --list-tools`).
/// Where the running game's files are: the install, the content folder the readers use (the
/// install, or the install with the enabled mods layered over it), and the DLCs the user
/// folder switches off (None when its `dlc_load.json` cannot be read).
pub(crate) struct GameContent {
    pub install: std::path::PathBuf,
    pub content: std::path::PathBuf,
    pub disabled_dlcs: Option<Vec<String>>,
}
/// Each law key with whether the game shows it and what could not be judged.
type LawsShown = Vec<(String, crate::plan_predicate::Truth, Vec<String>)>;

/// The occupation laws of the running game's content and whether the game shows each to the
/// player named in a hoi4_occupation read (tag, date).
fn occupation_laws(
    exe: &str,
    read: &Value,
) -> Result<(std::sync::Arc<Vec<occlaw::Law>>, LawsShown)> {
    let found = game_content(exe)?;
    let laws = occlaw::laws(&found.content);
    if laws.is_empty() {
        return Err(Error::new(
            "map_data_unavailable",
            "No occupation laws in common/occupation_laws",
        ));
    }
    let player = read["player"].as_str().unwrap_or("");
    // "1936-01-01 12:00" (the date part only)
    let date = read["date"]["iso"]
        .as_str()
        .and_then(|d| d.split(' ').next())
        .and_then(crate::focus_rules::date);
    let dlcs = crate::focus_rules::live::Dlcs::read(&found.install, found.disabled_dlcs.as_deref());
    let rules = crate::focus_rules::Catalog::load(&found.content);
    let known = occlaw::known(&found.content, player, date);
    let rows = occlaw::shown(&laws, &rules, known, Some(&dlcs));
    Ok((laws, rows))
}
pub(crate) fn game_content(exe: &str) -> Result<GameContent> {
    let install = mapdata::install_dir(exe)?;
    Ok(GameContent {
        content: modlayer::content_dir(&install, exe).unwrap_or_else(|| install.clone()),
        disabled_dlcs: modlayer::disabled_dlcs(exe),
        install,
    })
}
pub fn mac_tool_list() -> Vec<Value> {
    vec![
        country_status::tool(),
        diplomacy_state::tool(),
        diplomacy_state::countries_tool(),
        occupation::tool(),
        occlaw::tool(),
        war_stats::tool(),
        battles::tool(),
        alerts::tool(),
        json!({"name":"hoi4_peace_status","description":"Read-only: whether the peace conference window is open and, when it is, the texts it shows (current player, score, bidding phase, turns left). Without a conference it returns the route an ordinary player takes (actions list, states on the conference map, End turn). NOT validated live while a conference is open (no war was fought in the qualification game); no peace action tool exists yet.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        trade::tool(),
        trade::set_tool(),
        trade::auto_import_tool(),
        naval::fleets_schema(),
        json!({"name":"hoi4_fleet_mission","description":"Give one own task force a mission with the game's ordinary mission buttons (Strategic Navy map mode F2, fleet card, the task force entry's Select button, the mission button; the map mode is restored afterwards). Missions: patrol, strike_force, convoy_raiding, convoy_escort, mine_laying, mine_sweeping, training, invasion_support (all operate in the fleet's assigned sea regions; add regions with hoi4_fleet_region first) and hold (cancel the mission and sail to the closest port). Typed refusals before any input: task_force_unknown, task_force_ambiguous, nothing_to_change, no_assigned_region, reserve_fleet; found while driving: mission_unavailable (the game disables the button, e.g. mine missions without the technology). Verified from the task force's mission code in memory (hold 0, patrol 1, strike force 2, convoy raiding 3, convoy escort 4, training 7, invasion support 9 read back live). One mission per call; the ships sail when the game runs.","inputSchema":{"type":"object","properties":{"task_force":{"type":"string","maxLength":128},"mission":{"type":"string"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["task_force","mission","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_fleet_region","description":"Add a sea region to a fleet's area of operation (or remove it with remove:true) with ordinary input: Strategic Navy map mode, the Find view centres `state_id` (a coastal state next to the sea), left clicks on the map read the sea region names from the game's region view until `region` (exact display name, e.g. 'Eastern North Sea', 'Danish Belts') is found, then the fleet card and a right click on that region (remove: the remove-region button first). Typed refusals: fleet_unknown, already_assigned, region_not_assigned, reserve_fleet; region_not_found lists the sea regions seen (nothing is ordered then); mission_unavailable when the game disables the remove button. The fleet's task forces patrol/escort/raid in this area. Verified from the fleet's region list in memory (hoi4_fleets assigned_regions).","inputSchema":{"type":"object","properties":{"fleet":{"type":"string","maxLength":128},"region":{"type":"string","maxLength":96},"state_id":{"type":"integer","minimum":1,"maximum":4095},"remove":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["fleet","region","state_id","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_fleet_admiral","description":"Put an admiral in command of an own fleet (or unassign with unassign:true) through the fleet panel's 'Select Commander' list: Strategic Navy map mode, the fleet card, the commander portrait, the admiral's row (scrolled into view), a confirmation when he leads another fleet. With list:true it only opens the list and returns the admirals it offers (name, skill) without assigning. Typed refusals before input: fleet_unknown, reserve_fleet, nothing_to_change; unknown_commander lists the names offered. Verified from the fleet's leader in memory. The admirals' skills, traits and rank are the list's own; an admiral who leads another fleet is moved (the game's confirmation is accepted).","inputSchema":{"type":"object","properties":{"fleet":{"type":"string","maxLength":128},"admiral":{"type":"string","maxLength":96},"unassign":{"type":"boolean"},"list":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["fleet","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_naval_production_options","description":"Read-only (memory, no input): the ship and convoy designs the Production panel can build (not decommissioned) with their equipment keys and existing lines, and the own naval production lines with their line index and dockyard count; each design and line says whether it builds convoys (`convoy`). Use the equipment key and optional design id with hoi4_production_add; ship and convoy lines are added through the Production panel's naval category.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"hoi4_fleet_create","description":"Found a new fleet from one own task force with the game's ordinary controls: Strategic Navy map mode, the task force's fleet card, its Select button, then the bottom bar's '+' (new fleet) card; the new fleet holds exactly that task force and gets the game's default name (see hoi4_fleets). Typed refusals before input: task_force_unknown, task_force_ambiguous, nothing_to_change (already alone in its fleet), reserve_fleet. Verified from the fleet list in memory (one more fleet, holding only that task force). Use hoi4_fleet_mission / hoi4_fleet_region / hoi4_fleet_admiral on the new fleet afterwards.","inputSchema":{"type":"object","properties":{"task_force":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["task_force","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_navy_ui","description":"DEBUG ONLY (never listed to a player): dump the naval UI tree or send one ordinary input.","inputSchema":{"type":"object","properties":{"op":{"type":"string"},"root":{"type":"string"},"path":{"type":"string"},"depth":{"type":"integer"},"index":{"type":"integer"},"items":{"type":"boolean"},"x":{"type":"number"},"y":{"type":"number"},"code":{"type":"integer"},"letter":{"type":"string"},"pixels":{"type":"integer"},"lease_token":{"type":"string"}},"required":["op","lease_token"],"additionalProperties":false}}),
        naval::convoys_schema(),
        stockpile::tool(),
        airread::schema(),
        designs::schema(),
        paradrop::schema(),
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
        json!({"name":"hoi4_plan","description":"Read or change the standing plan (plan.json in the run directory): research priorities, focus path, construction, training, production, politics, decisions. {op:get} returns it; {op:set, plan:{...}} merges the given keys (null removes a key); {op:request_revision, reason} asks for a revised plan (written to the change feed as revision_requested; the plan in force stays until plan_revised). hoi4_plan_run carries it out.","inputSchema":{"type":"object","oneOf":[{"properties":{"op":{"const":"get"}},"required":["op"],"additionalProperties":false},{"properties":{"op":{"const":"set"},"plan":{"type":"object"}},"required":["op","plan"],"additionalProperties":false},{"properties":{"op":{"const":"request_revision"},"reason":{"type":"string","minLength":1,"maxLength":300}},"required":["op","reason"],"additionalProperties":false}]}}),
        json!({"name":"hoi4_peace_options","description":"Read the open peace conference from its window: header (current player, own remaining points = current_score, world tension, bidding phase, turn), which controls are usable (demand kinds take_state/liberate/puppet/force_government/take_navy, end turn, done), participant, available-action and bid lists as the window shows them, and for the asked `states` their owner and occupier. Costs are not stored: a state's tooltip shows them and hoi4_peace_take reports each state's cost as the score drop. No lease, no input. validated:false (no conference was arranged live; procedure in docs/PLAYBOOK_TOOL_GAPS.md).","inputSchema":{"type":"object","properties":{"states":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4095},"maxItems":16}},"additionalProperties":false}}),
        json!({"name":"hoi4_peace_take","description":"In the open peace conference, choose the demand kind (default Take State; puppet:true = Puppet; release:true = Liberate) and add each listed state from the conference's own state list (country rows expand into state rows; Find and a map click as fallback), verifying each through the drop of the window's score; reports each state's cost. Refusals before input: no_conference_open, round_not_ready, action_not_available, unknown_state, state_not_offered (own state), cost_exceeds_points. `max_cost` stops after a state dearer than it. Nothing is submitted. validated:false. `force_government` (the force-government demand on the country owning the state) is not yet built: refused with not_yet_built.","inputSchema":{"type":"object","properties":{"states":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4095},"minItems":1,"maxItems":8},"puppet":{"type":"boolean"},"release":{"type":"boolean"},"force_government":{"type":"boolean"},"max_cost":{"type":"number","minimum":0},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["states","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_peace_submit","description":"Submit Demands for the round (the conference's end-turn button; one submission per round under the blind-bid rules). Refused before input with round_not_ready or no_conference_open. Verified when the header (phase, turn, score of the turn) changes or the window closes. validated:false.","inputSchema":{"type":"object","properties":{"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_peace_exit","description":"Leave the peace conference (the Confirm and Exit / done button and the OK of its confirmation); accepted bids persist. Refused before input with exit_not_available or no_conference_open. Verified when the conference window closes. validated:false.","inputSchema":{"type":"object","properties":{"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_plan_run","description":"Carry out the standing plan for up to `seconds` (default 60): answer news and single-option events, fill empty research slots and the focus slot from the plan's lists, make planned politics picks and decisions, keep the training queue, put idle military factories on the priority lines and spend idle civilian factories on planned construction. Every step is an ordinary verified action under the given lease; the game is never paused. Returns early when a choice event needs the model or a peace conference opens. Use it instead of hoi4_wait_for while nothing else needs attention.","inputSchema":{"type":"object","properties":{"seconds":{"type":"integer","minimum":1,"maximum":300},"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_input_lease","description":"Exclusive configured-controller lease for ordinary macOS input. Acquire uses holder; renew/release use only lease_token. Status takes only op. Observations do not need a lease.","inputSchema":{"type":"object","oneOf":[
            {"properties":{"op":{"const":"acquire"},"holder":{"type":"string"},"seconds":{"type":"integer","minimum":30,"maximum":900}},"required":["op","holder"],"additionalProperties":false},
            {"properties":{"op":{"const":"renew"},"lease_token":{"type":"string"},"seconds":{"type":"integer","minimum":30,"maximum":900}},"required":["op","lease_token"],"additionalProperties":false},
            {"properties":{"op":{"const":"release"},"lease_token":{"type":"string"}},"required":["op","lease_token"],"additionalProperties":false},
            {"properties":{"op":{"const":"status"}},"required":["op"],"additionalProperties":false}]}}),
        json!({"name":"hoi4_research_start","description":"Start an own technology in a research slot (an empty one, or a busy one with replace:true, which switches that slot to this technology; without `slot` the first empty slot is used) with ordinary input (research panel, slot, folder tab, tile, Research), each click located from the live GUI and each stage verified from memory; succeeds only when the slot holds the technology. Refuses researched (code already_researched), already-assigned or busy-slot targets before any input. A tile scrolled out of view is refused. Stable request_id; every request ends in a terminal receipt (action_unconfirmed: the effect was not observed and the request is closed; read the state again before sending a fresh one).","inputSchema":{"type":"object","properties":{"technology":{"type":"string","maxLength":128},"slot":{"type":"integer","minimum":0,"maximum":15},"replace":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["technology","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_focus_start","description":"Start an own national focus with ordinary input (politics panel, focus button, drag-panning the tree until the node is in view, the node, Start), each target located from the live GUI and each stage verified from memory; succeeds only when own focus progress names the focus as active. Refuses completed or already-active focuses, or any while another focus is active, before input. Typed errors: focus_not_available (the detail window opened but the game disables Start: requirements unmet or an exclusive focus taken; Start was not clicked), focus_detail_not_opened (the node click was sent, no detail window read; re-observe before retrying), ui_blocked (names the notice pop-up classes and open events still shown). Stable request_id; every request ends in a terminal receipt (action_unconfirmed: the effect was not observed and the request is closed; read the state again before sending a fresh one).","inputSchema":{"type":"object","properties":{"focus":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["focus","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_production_add","description":"Add one own military or naval production line for a base equipment type (e.g. infantry_equipment, artillery_equipment, light_tank_equipment, fighter_equipment) with ordinary input: production panel, category button, the design in the selection window, then +/- until the line requests the given factories. Verified from the own line collection. Refuses equipment with no producible design. Optional `design` (a design id or design name from hoi4_designs / hoi4_design_create, same equipment type) picks that exact variant in the selection window instead of the first listed one. Ship hulls (ship_hull_*) and convoys (convoy_*) are added through the naval category; see hoi4_naval_production_options. The line gets no military-industrial organisation (the design is Shift+clicked past the organisation chooser); `organization` (the organisation's script key, as hoi4_mios lists it) is not yet built: it is refused with not_yet_built before any input.","inputSchema":{"type":"object","properties":{"equipment":{"type":"string","maxLength":128},"factories":{"type":"integer","minimum":1,"maximum":150},"design":{"anyOf":[{"type":"integer","minimum":1},{"type":"string","maxLength":128}]},"organization":{"type":"string","maxLength":128},"bulk":{"type":"boolean","description":"Factories by the game's Ctrl+Click (+5) and Shift+Click (+10) steps, each read back; a chord the game does not take falls back to single clicks. Off until its live probe passes."},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["equipment","factories","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_production_priority","description":"Move an own production line in the production view's priority order (a line higher in the list gets resources and factories first, so shortages and lost factories hit the bottom lines): the line's arrows (increase_priority, decrease_priority), Shift+click for to:top or to:bottom. The line is addressed like hoi4_production_set (line = entry_index from hoi4_production_lines, with its equipment). Verified by the line's place in the view's list (receipt detail: place before and after, the shown order).","inputSchema":{"type":"object","properties":{"line":{"type":"integer","minimum":0,"maximum":2047},"equipment":{"type":"string","maxLength":128},"to":{"type":"string","enum":["top","up","down","bottom"]},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["line","equipment","to","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_production_set","description":"factories 0 removes the line (its close button). Set the requested factories of an own production line, addressed by its entry_index and equipment_id from hoi4_production_lines, with the line's ordinary +/- buttons (bulk: Ctrl +5 and Shift +10 clicks); each click is verified from memory. 0 keeps the line with no factories.","inputSchema":{"type":"object","properties":{"line":{"type":"integer","minimum":0,"maximum":2047},"equipment":{"type":"string","maxLength":128},"factories":{"type":"integer","minimum":0,"maximum":150},"bulk":{"type":"boolean","description":"Factories by the game's Ctrl+Click (+5) and Shift+Click (+10) steps, each read back; a chord the game does not take falls back to single clicks. Off until its live probe passes."},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["line","equipment","factories","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_construction_add","description":"Queue more of a building in an own state with ordinary input: construction panel, the building button, the game's Find view to centre the state (F, state_id, Return), then clicks at the screen centre. Verified from the own construction queue (the building located in that state); placement mode is switched off afterwards. Typed errors: building_select_failed (the building button click was sent, the selection never named it, no map click was sent), no_free_slot_or_not_allowed (one centre click did not raise the queue in the state: likely no free slot or building not allowed, not proven; check hoi4_state_slots; never repeat blindly), ui_blocked (names the notice pop-up classes and open events). Building keys: arms_factory, industrial_complex, dockyard, infrastructure, air_base, anti_air_building, radar_station, synthetic_refinery, fuel_silo, rocket_site, ... Province buildings (supply_node = supply hub, naval_base, bunker, coastal_bunker) are clicked on a province: pass province_id, or the tool tries the state's provinces nearest the centre in turn until the game accepts one (a supply hub needs a province the game allows, usually on a railway). Railways (rail_way) are not supported.","inputSchema":{"type":"object","properties":{"building":{"type":"string","maxLength":64},"state_id":{"type":"integer","minimum":1,"maximum":4095},"province_id":{"type":"integer","minimum":1,"maximum":99999},"count":{"type":"integer","minimum":1,"maximum":10,"default":1},"fill":{"type":"boolean","description":"With bulk: count is every free slot of the building in the state: the first click is the game's Shift+Click (build the maximum amount), read back from the queue; plain clicks finish the count when the game took it as one."},"bulk":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["building","state_id","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_politics_options","description":"List the ideas offered for one politics slot (opens the slot's selection window and closes it again; changes nothing). category: government (slots 0 mobilization law, 1 trade law, 2 economy law, 3-5 political advisors), research_production (slots 0 tank, 1 naval, 2 aircraft and 3 materiel designer, 4 industrial concern, 5 theorist; with Arms Against Tyranny the designer slots hold Military Industrial Organizations: hoi4_mio_* tools) or military_staff (slots 0 chief of army, 1 chief of navy, 2 chief of air force, 3-5 high command). A slot index is the slot's position in its category, the order of the slot lines in the game's common/idea_tags (a mod can change it); an advisor is hired from a slot of its own kind, so a high command advisor needs slot 3, 4 or 5. Returns each idea key with the game's enabled state, the slot's current idea, and slots: every slot of the category with its index and what it holds now.","inputSchema":{"type":"object","properties":{"category":{"type":"string","enum":["government","research_production","military_staff"]},"slot":{"type":"integer","minimum":0,"maximum":7},"lease_token":{"type":"string"}},"required":["category","slot","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_politics_choose","description":"Take an idea (law, advisor, designer) for one politics slot with ordinary input; verified when the slot holds the idea in memory. Refuses ideas the game disables. Same category/slot numbering as hoi4_politics_options (military_staff: 0 chief of army, 1 chief of navy, 2 chief of air force, 3-5 high command; research_production: 5 theorist); the idea must be one the slot offers, so use a slot of the idea's own kind.","inputSchema":{"type":"object","properties":{"category":{"type":"string","enum":["government","research_production","military_staff"]},"slot":{"type":"integer","minimum":0,"maximum":7},"idea":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["category","slot","idea","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_decisions_list","description":"Open the decisions panel and list offered decisions (key, enabled by the game, whether a timer for it is running). Changes nothing.","inputSchema":{"type":"object","properties":{"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_decision_take","description":"Take an own decision with ordinary input (decisions panel, its row, select; the game's confirmation for exactly this decision is accepted). Verified when the decision view shows it taken (timer started, entry gone or disabled). Refuses decisions the game disables, and before input with insufficient_political_power a decision whose script cost (common/decisions, a plain number) is above the political power held: the game draws it enabled and a click does nothing. A targeted decision has one row per target: pass `target` (part of the row title as hoi4_decisions_list shows it, e.g. the target country's name) to pick that row.","inputSchema":{"type":"object","properties":{"decision":{"type":"string","maxLength":128},"target":{"type":"string","maxLength":96},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["decision","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_division_templates","description":"Own division template names (for hoi4_train) and how many training lines each has queued. Pure read; no lease needed.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"hoi4_train","description":"Queue `count` divisions of an own template in ONE training queue with ordinary input: the template's Train button creates the queue, its + sets each line to train one division (the game's default is infinite: a line that trains forever), then Add Unit adds the other lines in parallel; the queue gets the own state deploy_state_id as location (a queue without a location never deploys). Each step is verified on the queue in memory (amount, line count).","inputSchema":{"type":"object","properties":{"template":{"type":"string","maxLength":128},"count":{"type":"integer","minimum":1,"maximum":24},"deploy_state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["template","count","deploy_state_id","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_deploy_locations","description":"Give every training line without a deployment location the own state state_id (Find view, the line's location button, one click on the state). Lines without a location never deploy. Verified by the lines' deployment provinces.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["state_id","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_armies","description":"Own armies (name, division ids, division_limit: the divisions its commander leads without penalty, the game's base plus his traits; per army group army_limit: the armies its field marshal leads; threat: the share of the land around each army that others hold, encirclement_risk from 70%) and unassigned division ids (unassigned_by_state: per state the ids and shared_with, the armies standing in the same provinces and how many of them each shares). Per army `planning` {mean, min, max} is its divisions' planning bonus (0..1, 0.3 = 30%); it grows while the army holds a front or plan and stops at a cap the game computes from generals, technology and the HQ distance (not stored: a value that has stopped rising has reached it). threat:false skips the threat reads. Per army and per army group `plan` reads every battle-plan order from the game itself (what the battle-plan bar and the map arrows show): kind (front, offensive, fallback, area_defense), id, executing, divisions assigned; for a line its provinces count, its two end provinces, the states along it, the owner's divisions standing on it and the most in one province; per front its offensives with the arrow's states and the target line (provinces, ends, states, i.e. how deep and wide the advance goes); an area defence lists its states. An army inside an army group whose own plan is [] follows the group's orders. Pure read; no lease needed. Every army tool takes an army by its name or by its plan role (army_layout name_hint such as \"North_strike\", the army the executor bound to it; the reply names it in army_resolved).","inputSchema":{"type":"object","properties":{"threat":{"type":"boolean"}},"additionalProperties":false}}),
        json!({"name":"hoi4_army_front","description":"Give an own army a front-line order along the border nearest a state: army card, front-line drawing (Z), Find view centres the state, one click on the nearest highlighted border; with from_state (an own state touching state_id) a drag along that stretch of border only, a narrow front for a mobile army; with from_states (2-12 own states side by side) a drag along the border between those states and the land of state_id's controller only: one army group's own piece of a border that two groups share. Verified by one more battle-plan order on the army.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"state_id":{"type":"integer","minimum":1,"maximum":4095},"from_state":{"type":"integer","minimum":1,"maximum":4095},"from_states":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4095},"minItems":2,"maxItems":12},"provinces":{"type":"array","items":{"type":"integer","minimum":1,"maximum":99999},"minItems":1,"maxItems":1024,"description":"in-process only (family orders): the front's own provinces in drawing order; no drag. Each piece of the line on one theatre front section (the border against one enemy) becomes its own plan root (family orders2: one line per border for the whole group, the Shift+click form), all created in this call, each with its share of the group's divisions by length and place (the fronts the group already has included). Provinces no section holds are reported in in_process.coverage.uncovered and the receipt is Diverged (front_cut), never Verified"},"against":{"description":"in-process only: the front along the whole own border against this tag, or against each of several tags in one call (own_states narrows it): one plan root per border section, each with its share of the divisions; a tag with no border in the states is named in coverage.no_border","oneOf":[{"type":"string","minLength":3,"maxLength":3},{"type":"array","items":{"type":"string","minLength":3,"maxLength":3},"minItems":1,"maxItems":16}]},"own_states":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4095},"minItems":1,"maxItems":64},"parent_order":{"type":"integer","minimum":1,"description":"in-process only: the plan order the new front hangs under (default: the army's one top order)"},"replace_order":{"type":"integer","minimum":1,"description":"in-process only: give this existing front order the new province path instead of adding a front (keeps its id)"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","state_id","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_army_move","description":"Order an own army to move to the centre of a state with one right click (army card, Find view, right click); a plain move attacks enemy units there. Verified by the army's divisions heading for the state or stopping short; use for mopping up, battle plans remain the main tool. With province_id the order goes to that one province (the whole army gathers on it: a strike on one objective); it is sent again while any division neither marches on it nor stands in it. halt:true (no state_id) cancels every march and attack order of the army with the unit panel's halt control (H): the divisions stay where they stand and recover; verified by the divisions no longer marching (hoi4_supply shows each division's heading_province). divisions (with province_id) orders only those divisions of the army, one at a time (each selected from the army's division list, then one right click); the rest keep their orders: a strike sized to what the province feeds, a garrison left behind.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"halt":{"type":"boolean"},"divisions":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":48,"description":"Division ids as hoi4_armies or hoi4_supply list them; needs province_id"},"province_id":{"type":"integer","minimum":1,"maximum":29999,"description":"One province of the state to move to (an enemy-held province inside an own state, a port); hoi4_states with the state ID lists each province and its controller"},"state_id":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_army_plan","description":"Execute or stop an own army's battle plan with the army card's play or stop button (execute), and set how its battle plans are carried out (mode). Execute is verified by the card's executing state. mode sets the execution mode on the battle plan bar: careful (no attack on heavily fortified provinces, none that lengthens the front), balanced (the game's default) or rush (every province of the plan is attacked: it lengthens the front and can gather an army in one unsupplied province; it suits a weak or broken enemy line and is a plan decision). The army is selected, the bar's execution button opens the three choices, the wanted one is clicked and verified by the three buttons' frames (detail: frames_before, frames_after); the selection is cleared afterwards. Give execute, mode or both: with both the mode is set first (journalled as <request_id>:mode, returned as mode_receipt) and the execute or stop order follows only when the mode is verified. A failure of the mode step names the bar, the window, the rectangles and the frames it found.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"execute":{"type":"boolean","description":"true executes the plan, false stops it; may be left out only when mode is given"},"mode":{"type":"string","enum":["careful","balanced","rush"],"description":"execution mode of the army's battle plans; alone it only sets the mode"},"clear_orders":{"type":"boolean","description":"true deletes every battle-plan order of the army or army group (the bar's delete button, right click, confirmed); goes alone; verified by no order left"},"delete_order":{"type":"integer","minimum":1,"description":"deletes ONE battle-plan order by its id from the plan read-back (hoi4_armies plan: a front's offensives[].id): the bar's delete tool (left click), then a click on the order's arrow on the map; the front and every other order stay, so the front's divisions keep their planning bonus. Goes alone; verified by the id gone"},"assign":{"type":"object","description":"in-process only (family orders2: the plan bar's assign): divisions onto ONE order of the plan by its id (a front, or an offensive to put them in the attack): the listed division ids, or the count of the army's divisions standing nearest the order's line that it does not hold yet. Goes alone; verified by the order's division count read back","properties":{"order":{"type":"integer","minimum":1},"divisions":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":512},"count":{"type":"integer","minimum":1,"maximum":512}},"required":["order"],"additionalProperties":false},"distribute":{"description":"in-process only (family orders2): the army group's (or army's) divisions over its fronts (plan roots) by each front's weight (weights by front order id, default the front's length in provinces) and by place: each front keeps the divisions standing by it, the rest go to the nearest front with room, at least one per front while there are as many divisions as fronts; a front's new divisions go to its one offensive when it has one. The game puts every division of a new front on that front, so a group with two fronts drawn one after the other holds them all on the newest: distribute right after drawing fronts and whenever a front reads 0 divisions. Goes alone; verified by the division counts read back per order","oneOf":[{"type":"boolean"},{"type":"object","properties":{"weights":{"type":"object","additionalProperties":{"type":"number","minimum":0}}},"required":["weights"],"additionalProperties":false}]},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_army_offensive","description":"Draw an offensive line for an own army that already holds a front line, into a target state: army card, offensive-line drawing (X), one right-button drag across the state, parallel to the army's front, so the whole front advances to it. With province_id (a province of the state, e.g. its victory point) the line runs through that province's depth; without it through the middle of the state. Verified by new battle-plan orders on the army; the receipt's detail.plan reads the army's orders back from the game (the target line's provinces, ends and states). With states the line runs across several states at once, the way the game's AI draws its offensive (its 1939 Poland line: 22 provinces through eight states, one arrow, the whole army group on it). Execute the plan afterwards.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"state_id":{"type":"integer","minimum":1,"maximum":4095},"province_id":{"type":"integer","minimum":1,"maximum":29999,"description":"The objective inside the state: the line is drawn at its depth from the army"},"spearhead":{"type":"boolean","description":"Draw the game's spearhead order (battle-plan bar, Shift+X) instead of an offensive line: it takes the provinces as painted without adjusting, for encirclements and narrow thrusts (armour)"},"target_provinces":{"type":"array","items":{"type":"integer","minimum":1,"maximum":99999},"minItems":1,"maxItems":1024,"description":"in-process only: the exact target line in drawing order; with spearhead:true a new spearhead under parent_order (family orders2), with replace_order the new path of that existing order (family orders); a plain new offensive is still drawn by the stroke"},"parent_order":{"type":"integer","minimum":1},"replace_order":{"type":"integer","minimum":1},"states":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4095},"minItems":2,"maxItems":12,"description":"A deep line across these target states side by side (the phase line: e.g. every state along a river across the enemy country): one bending stroke through the states' centres in the order given (list them side by side from one end of the line to the other, e.g. along a river); state_id is then only the state the camera finds first. No province_id."},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","state_id","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_army_fallback","description":"Draw a fallback line for an own army through own states (a defender's line on rivers, mountains or forts behind the front): army card, fallback-line drawing (C), one right-button stroke across the states at their middle depth, perpendicular to the way to the army. Verified by new battle-plan orders on the army. The army moves to the line when its plan is executed (hoi4_army_plan execute): a defender executes it when the front breaks or goes unfed.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"states":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4095},"minItems":1,"maxItems":8,"description":"own states the line runs through, side by side"},"provinces":{"type":"array","items":{"type":"integer","minimum":1,"maximum":99999},"minItems":1,"maxItems":1024,"description":"in-process only (family orders2): the fallback line's own provinces in drawing order, posted as one fallback order; states is then the mouse path's line"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","states","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_army_area_defense","description":"Give an own army an area defence order over own states (the army spreads over them and holds what the order covers): army card, area defence (V), one right-click on each state (Find view first). Verified by new battle-plan orders on the army. Refused before input with no_commander when the army has no general (the game takes battle-plan orders only from a led army). `settings` (the order window's toggles: victory_points, ports, coastlines, airbases, borders, forts, rail, facility) is not yet built: refused with not_yet_built; the game's defaults apply.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"states":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4095},"minItems":1,"maxItems":8},"settings":{"type":"array","items":{"type":"string","enum":["victory_points","ports","coastlines","airbases","borders","forts","rail","facility"]}},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","states","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_army_exercise","description":"Start or stop army exercises for own armies: the divisions train toward the game's training maximum and the country earns army experience (doctrines, templates and designs are paid with it), at the cost of extra attrition (equipment) and fuel. Start: the army's card selects it and the battle plan bar's exercise button (btn_exercise) is clicked; stop_at_max (default true) Shift+clicks it so each army stops by itself at the maximum. Stop (on:false): the card's stop-training button. Verified per army by its card: training_marker shown while it exercises, icn_exercise_stop_at_max with the stop at the maximum (hoi4_armies reports both as exercising and exercise_stop_at_max). An army already as asked is left alone. A disabled button (no division that can train, all at the maximum) is refused with exercise_unavailable for that army; the receipt's detail lists every army's card state.","inputSchema":{"type":"object","properties":{"armies":{"type":"array","items":{"type":"string","minLength":1,"maxLength":128},"minItems":1,"maxItems":24},"on":{"type":"boolean"},"stop_at_max":{"type":"boolean","description":"start only: stop each army at the training maximum (default true)"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["armies","on","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_states","description":"States in one memory read (no interface input). A country tag (e.g. SOV, GER) lists every state it owns or controls: owner, controller, category, live building levels (buildings, factories), for the own country the shared building slots (unlocked, used, queued, free), resources, victory points with who holds them, the factories history/states gives (factories_start, an estimate for another country) and totals. Filter and sort in the same call instead of opening states one by one: owned_only, ids, min_free_slots, building with min_level/max_level, resource; sort by free_slots (default), factories, infrastructure, victory_points, level (of building), resource, id or name; limit (default 200) and offset. Large countries are read as fast as small ones: choose the few states to act on from this table, then act. A name (accent-insensitive substring) or one exact ID finds states by name instead. Pure read.","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"ids":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4095},"maxItems":1000},"owned_only":{"type":"boolean"},"min_free_slots":{"type":"integer","minimum":0,"maximum":25},"building":{"type":"string"},"min_level":{"type":"integer","minimum":0},"max_level":{"type":"integer","minimum":0},"resource":{"type":"string"},"sort":{"type":"string","enum":["free_slots","factories","infrastructure","victory_points","level","resource","id","name"]},"limit":{"type":"integer","minimum":0,"maximum":1000},"offset":{"type":"integer","minimum":0}},"required":["query"],"additionalProperties":false}}),
        json!({"name":"hoi4_wars","description":"Own land borders, occupied and threatened own states, from state owner/controller, own division positions and state adjacency derived from the install map files. Pure read; no lease. Stage B: wars lists the own wars (enemies, allies at war, start date) from war relations; sides, leaders, war score and justification timers are unvalidated; surrender progress (wars[].enemy_surrender_progress, own_surrender_progress) and the naval intel against each enemy (wars[].own_naval_intel) are not yet built and always null with a reason; hostility is the war enemies plus the occupation proxy and the optional enemies tags you pass; covered is a heuristic (own divisions stand in the border states), not a front-line-order read. Alerts: own_state_occupied, border_uncovered, threatened_state_undefended. capital_land: own-controlled states the capital reaches over own-controlled land (history capital, else the largest own area); a division elsewhere is overseas (disbanding it loses 75% of its equipment).","inputSchema":{"type":"object","properties":{"enemies":{"type":"array","items":{"type":"string","minLength":3,"maxLength":3},"maxItems":32},"max_states":{"type":"integer","minimum":1,"maximum":1100}},"additionalProperties":false}}),
        json!({"name":"hoi4_province_graph","description":"Land provinces of the given states and the provinces each one touches, from the install's map files (provinces.bmp, definition.csv, history/states): per province its id, its state and its land neighbours; its terrain with the terrain's combat width, support width, attack modifier, movement cost and attrition (common/terrain), whether it is coastal, and the bunker levels its state history sets (forts_in_history: start or the dated block; live levels are not read here); per border (borders[]) the river that draws it (small or large, with the attack penalty of the river defines; touches = a river meets the border only at a point, estimate) and special crossings of adjacencies.csv (sea strait with the sea province, impassable, canal rule). Use it to see which provinces an objective can be attacked from and to find a way over land; who holds a province is in hoi4_states (a state id query lists its provinces with their controllers). Pure read; no lease, no game memory.","inputSchema":{"type":"object","properties":{"states":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4095},"minItems":1,"maxItems":128}},"required":["states"],"additionalProperties":false}}),
        json!({"name":"hoi4_front_plan","description":"War plan for the named enemy tags (a tag, a comma list or an array): per contiguous border the own and enemy border states, ranked attack axes (cheapest route by terrain, infrastructure, urban and bunkers toward the enemy capital and valuable states, with the states along it), holding-line and second-line states for the other fronts, own divisions per front versus what is missing, and concrete hoi4_army_front / hoi4_army_offensive / hoi4_army_plan suggestions with state ids (add request_id and lease_token). Pure read; visible ownership and own division positions only, vanilla map files; no lease.","inputSchema":{"type":"object","properties":{"enemy":{"type":["string","array"],"items":{"type":"string"},"description":"enemy tag(s), e.g. POL or [\"FRA\",\"BEL\"]"},"max_axes":{"type":"integer","minimum":1,"maximum":6}},"required":["enemy"],"additionalProperties":false}}),
        json!({"name":"hoi4_encirclement_candidates","description":"Pockets of enemy-controlled states that taking at most max_gap neck states would cut off: the pocket, the neck, the two pincer arms (own state into neck state), divisions needed and available per arm, value (points, industry, manpower, resources) and a score; sea, impassable states and neutral neighbours count as walls. Pure read; no lease.","inputSchema":{"type":"object","properties":{"enemy":{"type":["string","array"],"items":{"type":"string"}},"max_gap":{"type":"integer","minimum":0,"maximum":3},"limit":{"type":"integer","minimum":1,"maximum":20}},"required":["enemy"],"additionalProperties":false}}),
        json!({"name":"hoi4_war_goal_targets","description":"Which enemy-owned states to demand in the peace conference, ranked by points, industry, manpower and resources with tiers (must_take, worth_taking, optional), resource prizes, contiguous blocks, own cores and which states we already occupy. A read, not an action; the conference cost itself is not modelled. No lease.","inputSchema":{"type":"object","properties":{"enemy":{"type":["string","array"],"items":{"type":"string"}},"limit":{"type":"integer","minimum":1,"maximum":100}},"required":["enemy"],"additionalProperties":false}}),
        json!({"name":"hoi4_events","description":"Open event windows in order (the last is on top) with their title and option texts. Pure read; no lease needed. Each event comes with its full text read from game memory (no screenshot): title, description (markup removed), the leader for leader/operative layouts, and every option text, so the event can be judged from the report alone. option_keys gives, per shown option, the name the event script uses for it (for example my_event.1.b; null when it cannot be told from the event files and the localisation), so a rule or a guide that names an option by its script name finds the right one even when an option is hidden.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"hoi4_event_choose","description":"Choose an option of an open event by clicking it: `option` is the 0-based index in listed order, or the option's script key as hoi4_events lists it in option_keys (\"germany.14.a\", or just \"a\"), or its shown text (the list order is not the stacking order: single-option windows over it are acknowledged first; a choice window over it is named in event_covered). Verified when exactly that event window closes.","inputSchema":{"type":"object","properties":{"event":{"type":"string"},"option":{"anyOf":[{"type":"integer","minimum":0,"maximum":7},{"type":"string","minLength":1,"maxLength":200}]},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["event","option","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_diplomacy","description":"Open the diplomacy view for a country and list the offered actions with canonical key, enabled state, cost and whether the AI would refuse (changes nothing in the game). Address the country by `country` (three-letter tag such as POL; the view is pointed at one of its states with the Find view) or by `state_id` of one of its states; `countries` (up to 6 tags) lists several. With `action` (a canonical key such as justify_war_goal, declare_war, guarantee, or an exact row name) it previews what the action's dialog shows (the cost, justification time and options the game displays) and then cancels without sending: for justify_war_goal / add_war_goal add `war_goal` (a type from war_goal_types, e.g. Conquer) and `states` (up to 5 names from states_offered) to read the game's cost text for that exact goal. Read the countries and their relations with hoi4_diplomacy_countries. With `catalog:true` and `country` (no view opened, nothing sent): EVERY diplomatic action the game itself has, from the in-process module's scan of the game's action classes plus the content's scripted actions (a mod's actions), each with its key, shape (simple, divisions, war_goal, war_goal_type, division_list, or unsupported:<shape>), the route that sends it (in_process, the view's row by mouse, or not postable yet) and can_do = the game's own check toward that country (the one that enables the view's Send button), asked without sending.","inputSchema":{"type":"object","properties":{"catalog":{"type":"boolean"},"country":{"type":"string","pattern":"^[A-Z0-9]{3}$"},"countries":{"type":"array","items":{"type":"string","pattern":"^[A-Z0-9]{3}$"},"maxItems":6},"state_id":{"type":"integer","minimum":1,"maximum":4095},"action":{"type":"string","maxLength":128},"war_goal":{"type":"string","maxLength":128},"states":{"type":"array","items":{"type":"string"},"maxItems":5},"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}}),
        railway::tool(),
        json!({"name":"hoi4_army_attack","description":"Front line, offensive line and execute for one army in one call (the same steps as hoi4_army_front, hoi4_army_offensive and hoi4_army_plan execute:true, each verified and journalled as <request_id>:front, :offensive, :execute). Skip a step with front:false (the army already holds a front), offensive:false or execute:false. Stops at the first failing step and says which steps were done.","inputSchema":{"type":"object","properties":{"army":{"type":"string","maxLength":128},"state_id":{"type":"integer","minimum":1,"maximum":4095},"front":{"type":"boolean"},"offensive":{"type":"boolean"},"execute":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","state_id","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_construction_order","description":"Reorder or remove one own construction line with its row buttons in the construction panel: action top (Shift+Click on the up arrow), up, down, bottom or remove (the row's close button). `line` is the 0-based index as hoi4_construction_queue lists the lines; the first lines get the civilian factories (15 per line), so put the supply hub or railway the front needs first. Verified by the line's new position in the own queue (or its absence after remove).","inputSchema":{"type":"object","properties":{"line":{"type":"integer","minimum":0,"maximum":127},"action":{"type":"string","enum":["top","up","down","bottom","remove"]},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["line","action","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_save","description":"Save the running game with the game's own Save dialog (Escape, Save, Save) under the name the game proposes (country and date), overwriting a save of the same name, and verify that a save file was written in the run's userdata/save games. Use it at the end of a campaign segment so the next launch continues exactly from this point (no turns are replayed). The game stays paused if it was paused; a game that was running is unpaused again after the save (resumed:true).","inputSchema":{"type":"object","properties":{"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_war_risk","description":"Read-only, before justifying or declaring war on `country`: which countries would join that war on its side (its faction, every country guaranteeing it, its overlord and subjects) and which of our borders with them are not held (need a division in every border state and two per state). hoi4_diplomacy_act declare_war refuses with war_would_widen in that case unless accept_widening:true. Memory and map files only; no input.","inputSchema":{"type":"object","properties":{"country":{"type":"string","pattern":"^[A-Z0-9]{3}$"}},"required":["country"],"additionalProperties":false}}),
        json!({"name":"hoi4_diplomacy_act","description":"Take ONE diplomatic action toward a country with the ordinary diplomacy view. Address it by `country` (tag, e.g. POL) or `state_id`. `action` is a canonical key (improve_relations, guarantee, revoke_guarantee, non_aggression_pact, ask_military_access, offer_military_access, revoke_military_access, ask_docking_rights, offer_docking_rights, send_attache, invite_to_faction, join_faction, kick_from_faction, call_to_arms, request_licence_production, start_lend_lease, request_lend_lease, send_volunteers, expeditionary_force, return_expeditions, boost_party_popularity, stage_coup, embargo, justify_war_goal, add_war_goal, cancel_war_goal, declare_war, ...) or an exact row name from hoi4_diplomacy. justify_war_goal / add_war_goal also need `war_goal` (type, e.g. Conquer) and 1..5 `states` by name (preview them first with hoi4_diplomacy). declare_war needs `confirm:\"declare_war\"` and a completed war goal; without one the call is refused with war_goal_required and nothing is sent. Typed refusals before any input: action_not_offered (lists the offered rows), action_disabled, war_goal_required, ai_would_refuse (the game's red cross on the row; pass try_anyway:true to send anyway), nothing_to_change, confirmation_required, dialog_unsupported (dialogs that need choices the tool cannot make yet), war_would_widen (declare_war: the target's faction, guarantors or overlord/subjects border us where the border is not held; lists them; accept_widening:true overrides). The receipt is settled from memory by the one relation, war goal, war or faction membership the action can create or remove (see hoi4_diplomacy_state); an AI refusal is a NotApplied receipt. send_volunteers / recall_volunteers carry `army` (the army whose divisions go) and optionally `division_ids` within it, and go through the in-process module's diplomacy family (the view's own action; HOI4_BACKEND=inprocess until S25-VOLUNTEERS): the game's own check decides (volunteer cap, divisions required, recall lock, war and ideology gates; refused_by_game posts nothing). Air volunteers are a rebase of wings to the receiver's air base (hoi4_air_rebase). Any other key that hoi4_diplomacy catalog:true lists with route in_process (a game action class with no view row the tool reads: air_base_access, offer_air_base_access, naval_blockade, peace_proposal, reduce_autonomy, increase_autonomy, request_access_to_license_production, cancel_licensed_production, or one a later game adds) takes `country` only and goes through the module's diplomacy2 family (HOI4_BACKEND=inprocess until S27-DIPLOMACY-ANY); a key whose shape the module cannot build is refused dialog_unsupported and nothing is sent. justify_war_goal may instead carry `war_goal_key` (the game's own key from common/wargoals, e.g. take_state, annex_everything, topple_government) and `state_ids` (the states a take-state goal names; none for a goal on the whole country): the module's diplomacy2 family then builds the justification by the action's own constructor and the game's own check decides (HOI4_BACKEND=inprocess until S27-DIPLOMACY-ANY). expeditionary_force with `army` (and optionally `division_ids`) goes the same way as send_volunteers, through the diplomacy2 family.","inputSchema":{"type":"object","properties":{"country":{"type":"string","pattern":"^[A-Z0-9]{3}$"},"army":{"type":"string"},"division_ids":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":64},"state_id":{"type":"integer","minimum":1,"maximum":4095},"action":{"type":"string"},"war_goal":{"type":"string"},"war_goal_key":{"type":"string","pattern":"^[A-Za-z0-9_]{1,39}$"},"state_ids":{"type":"array","items":{"type":"integer","minimum":1,"maximum":100000},"maxItems":64},"states":{"type":"array","items":{"type":"string"},"maxItems":5},"try_anyway":{"type":"boolean"},"accept_widening":{"type":"boolean"},"confirm":{"type":"string"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["action","request_id","lease_token"],"additionalProperties":false}}),
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
        json!({"name":"hoi4_army_commander","description":"Assign a general (by the full name hoi4_commanders lists, a part of it that names one commander only such as the surname, or the character id) to an own army, or a field marshal to an own army group (use the army group's name), from the panel's commander list. Verified when the army's commander is that general. Refused before any click with army_unknown (lists valid armies/groups), unknown_commander (5 closest names) or commander_unavailable (already leads another army/group; pass reassign: true to move them).","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"general":{"type":"string"},"reassign":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","general","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_air_wings","description":"Own air wings with air base, assigned region and missions, from the Air Forces Overview (opens it). Changes nothing in the game.","inputSchema":{"type":"object","properties":{"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_air_mission","description":"Give an own air wing (exact name) a mission over a strategic air region with the game's ordinary controls: wing row of the Air Forces Overview (L), the mission button, the Find view centres `state_id` (any state inside the wanted region) and a right click assigns. Pass `region` (region id or display name, e.g. 'Eastern Germany') instead of or with `state_id`: the state is then chosen from the game's files and the result is verified to be that region; a sea region (no state in it, e.g. 'English Channel') is reached by centring the nearest state and clicking the region's own sea provinces on screen. Missions: air_superiority, cas, interception, strategic_bombing, naval_strike, kamikaze, port_strike, logistics_strike, air_supply, training, naval_mines, naval_minesweeping, recon, naval_patrol. Typed refusals before input: wing_unknown, wing_ambiguous, invalid_arguments (unknown mission), region_unknown, region_state_mismatch; the game's own disabling (mission not offered / needs more planes) surfaces as invalid_target. Verified from the wing's mission mask and region id in memory (hoi4_air). No percent or intensity control exists in the game's air UI; aggressiveness and day/night are per-wing cycle buttons not driven by this tool.","inputSchema":{"type":"object","properties":{"wing":{"type":"string","maxLength":128},"wings":{"type":"array","items":{"type":"string","maxLength":128},"minItems":1,"maxItems":24,"description":"Several wings (instead of wing) given the same mission over the same region in one input sequence: selected together in the overview (a click, then Shift+clicks), one mission click, one region click; a wing the selection does not take is ordered alone. The receipt is Verified when all fly it, Diverged when some do (detail.wings names each, result.failed says why)"},"mission":{"type":"string"},"state_id":{"type":"integer","minimum":1,"maximum":4095},"region":{"anyOf":[{"type":"string","maxLength":96},{"type":"integer","minimum":1,"maximum":4095}]},"bulk":{"type":"boolean","description":"With wings: the bases whose every wing is listed are ordered by one click on the base circle (it selects all its wings), one mission or rebase order for all, read back at the end; the other wings and any the base order missed take the single path in the same call. Off until its live probe passes."},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["mission","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_air_rebase","description":"Rebase one own air wing to the air base of a state with ordinary input: strategic air map (F3), the wing's row of the Air Forces Overview, the Find view centres `state_id`, the base's circle is located from the game's own map icon objects and right clicked. The wing flies a transfer (status 'To <base>'); verified from the wing's transfer destination in memory (hoi4_air `transfer_to_base_id`), the base id changes on arrival. Typed refusals before any input: wing_unknown, wing_ambiguous, air_base_unknown (state without an own air base; lists own base states), nothing_to_change (already based or already flying there), air_base_full (capacity minus wings stationed or incoming). The game set no range limit on a transfer in the test (a 570 km fighter wing was accepted 207 map units away); if it refuses anyway the tool reports rebase_refused. Map mode is restored afterwards.","inputSchema":{"type":"object","properties":{"wings":{"type":"array","items":{"type":"string","maxLength":128},"minItems":1,"maxItems":24,"description":"Several wings (instead of wing) to the same base in one call: one air mode, each selected and right-clicked onto the base in turn (the camera stays on the base). Verified when all transfer, Diverged when some do (detail.wings, result.failed)"},"wing":{"type":"string","maxLength":128},"state_id":{"type":"integer","minimum":1,"maximum":4095},"bulk":{"type":"boolean","description":"With wings: the bases whose every wing is listed are ordered by one click on the base circle (it selects all its wings), one mission or rebase order for all, read back at the end; the other wings and any the base order missed take the single path in the same call. Off until its live probe passes."},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["state_id","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_air_wing_create","description":"Found a new air wing from reserve aircraft at an own air base: strategic air map, Find view centres `state_id`, left click on the base circle, the N shortcut ('Add new Air Wing'), the reserve row of `aircraft` (variant name such as 'Ju 86' or class name such as 'Inter-War Fighter', case-insensitive) moves up to 100 planes of that variant into the wing (the game's own limit per click), Ok. Reserve aircraft come from production or from disbanding a wing. Typed refusals: air_base_unknown, air_base_full (before input); no_reserve_aircraft (the game refuses the window when the reserve is empty), aircraft_not_in_reserve (lists what is in reserve), aircraft_ambiguous, create_unavailable (Ok disabled, e.g. no manpower) while driving; nothing is created in those cases. Verified from the new wing's name appearing at that base in memory (hoi4_air). The wing gets the game's default name and 'Deploying from Reserves' status.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"aircraft":{"type":"string","maxLength":96},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["state_id","aircraft","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_air_wing_disband","description":"Disband one own air wing (row of the Air Forces Overview, toolbar Disband, OK of the confirmation); all its equipment returns to the reserve. Typed refusals before input: wing_unknown, wing_ambiguous; disband_unavailable if the game disables the button. Verified from the wing's absence in memory (hoi4_air).","inputSchema":{"type":"object","properties":{"wing":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["wing","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_air_wing_split","description":"Split one own air wing in two halves with the toolbar's Split button (shortcut D) after selecting it in the Air Forces Overview: the original keeps half of its aircraft, a new wing of the same type, base and orders takes the rest and gets the game's default name. Typed refusals before input: wing_unknown, wing_ambiguous, wing_in_transfer, nothing_to_change (fewer than 2 aircraft); split_unavailable if the game disables the button. Verified from memory: one more wing at the base and fewer aircraft in the original. This and hoi4_air_wing_create (up to 100 aircraft per click) are the ways to size wings; hoi4_air_wing_merge joins them again.","inputSchema":{"type":"object","properties":{"wing":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["wing","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_air_wing_merge","description":"Merge own air wings of one aircraft type at one air base with the toolbar's Merge button. The game's map stack and merge work on every wing of that type at the base, so `wing` is joined with all others of its type there; give `with` to name the single expected partner (refused with merge_group_larger when more than two wings of the type are there). The first wing is selected through its Air Forces Overview row, the stack item of the type beside the base circle joins with a Shift click, the game's selection array decides that exactly the group is selected, then Merge; the first wing survives with the aircraft (a wing holds at most 100: merging 40+80+40 bombers left wings of 100 and 60). Typed refusals before input: wing_unknown, wing_ambiguous, invalid_arguments (same wing twice), merge_different_base, merge_different_type, no_merge_partner, merge_group_larger, wing_in_transfer; merge_unavailable if the game disables the button, selection_lost if the group could not be selected (nothing is merged then). Verified from memory: fewer wings of the group left holding all their aircraft.","inputSchema":{"type":"object","properties":{"wing":{"type":"string","maxLength":128},"with":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["wing","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_air_hold","description":"Stop an own air wing's mission (the toolbar's Hold Air Wings button, shortcut H): the wing keeps its base and stands by (its panel row reads 'Standing by', the Air Forces Overview's Area column empties); its assigned region is cleared, the last mission selection stays in memory. Typed refusals before input: wing_unknown, wing_ambiguous, nothing_to_change (already standing by); hold_unavailable if the game disables the button. Verified from the wing's assigned region pointer in memory.","inputSchema":{"type":"object","properties":{"wing":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["wing","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_air_attach","description":"Attach own air wings to an army the game's way: each wing is selected alone (its Air Forces Overview row) and the army's card is right-clicked (\"Right-Click to attach Airwings to Army\"). An attached wing flies for that army's battles and follows it. Fighters and close air support attach; naval and strategic bombers and wings in pilot exercises cannot. Read back from memory: the wing's attached-army link must name the army's object after the click (a wing already linked to it is left alone, already:true); the result lists per wing attached and ms. Attaching to another army moves the wing. A wing loaded attached from a save may read unlinked and is then attached again. Refused before input: an unknown army.","inputSchema":{"type":"object","properties":{"wings":{"type":"array","items":{"type":"string","maxLength":128},"minItems":1,"maxItems":24},"army":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["wings","army","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_air_region_probe","description":"What the player's air map shows for one strategic air region: Find view centres `state_id`, a left click on the region opens the game's region window, which is read (region name, active aircraft by type for us and the enemy: fighters, bombers, close air support, anti-air guns; detection and efficiency percentages) and closed again; `region` (id or display name) instead of `state_id` picks a state inside it from the game files. Adds enemy_air_superiority_estimate: the enemy's share of the active fighters weighted by each side's efficiency shown (theirs / (ours + theirs)), null with no fighter on either side; an estimate from what the window shows, not the game's own superiority figure (not located in memory). Orders nothing; moves the cursor and clicks, so it needs the input lease. Only what the air map displays; no hidden enemy information.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"region":{"anyOf":[{"type":"string","maxLength":96},{"type":"integer","minimum":1,"maximum":4095}]},"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_air_regions","description":"Own wings and air bases grouped by strategic air region, read from memory and the game's map files (no input, never pauses): per region its name, own wings assigned (aircraft, type, missions, base state), aircraft total and the own air bases inside with capacity, aircraft stationed and free capacity; plus the wings that have no region. Enemy presence is not in memory: use hoi4_air_region_probe for the regions that matter. regions[].enemy_air_superiority is not yet built: always null with enemy_air_superiority_reason.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}}),
        json!({"name":"hoi4_army_group_create","description":"Group two or more own armies into a new army group (army cards with Shift, then the new-army-group card). Give it a field marshal with hoi4_army_commander using the army group's name. Verified when all the armies belong to one new army group.","inputSchema":{"type":"object","properties":{"armies":{"type":"array","items":{"type":"string"},"minItems":2,"maxItems":12},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["armies","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_army_rename","description":"Rename an own army with the game's own control: the army's card selects it, its badge's name box takes a click, the old name is erased and the new one typed key by key (each key verified in the box), then Return. Verified when the army carries the new name. `army` may be the army's name or its plan role (army_layout name_hint). Refused before input with army_unknown, name_taken (another army or army group has it) or name_not_typable (letters, digits, space and - _ . , / ' ( ), at most 20 characters: the game keeps no more).","inputSchema":{"type":"object","properties":{"army":{"type":"string","maxLength":128},"name":{"type":"string","minLength":1,"maxLength":20},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","name","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_army_motorization","description":"Set an army's supply motorization: 0 none, 1 partial, 2 full. The supply hubs the army draws from then use trucks (about 40 per level per hub) to carry supply much farther into roadless land; it costs trucks from the stockpile (keep truck production running) and fuels nothing. With the game's ordinary control: the army's card selects it, then the motorization button on its badge is clicked until the level is reached, each click verified from memory. Read the current level in hoi4_armies (motorization). Refused before input with army_unknown or invalid_arguments.","inputSchema":{"type":"object","properties":{"army":{"type":"string","maxLength":128},"level":{"type":"integer","minimum":0,"maximum":2},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","level","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_division_disband","description":"Disband own divisions of one army (the division list's delete button; their equipment and manpower go back to the pool). Use it to cut useless divisions that only eat equipment, manpower and supply: tiny starting divisions (2-4 battalions), cavalry or irregulars a strong player would not keep, once the army has enough good divisions. `divisions` are ids as hoi4_armies / hoi4_supply list them; listing every division of the army disbands the whole army in one press. Each division is verified gone. Refused before input when a division is not in that army.","inputSchema":{"type":"object","properties":{"army":{"type":"string","maxLength":128},"divisions":{"type":"array","items":{"type":"string","maxLength":32},"minItems":1,"maxItems":24},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","divisions","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_map_mode","description":"Switch the map mode with the game's own F-key shortcut, as a human checks the map: political (F1, default; armies and fronts), naval (F2), air (F3, air regions and superiority), supply (F4), terrain (F5), resistance (F6), compliance (F7), resources (F8), air_raids (F9), factions (F10). A view change only (no game state changes, nothing to read back): switch to political again before giving army orders. Look at a mode when its domain matters: supply before an offensive, resistance and compliance for occupied states, air for superiority, naval for sea lanes.","inputSchema":{"type":"object","properties":{"mode":{"type":"string","enum":["political","naval","air","supply","terrain","resistance","compliance","resources","air_raids","factions"]},"lease_token":{"type":"string"}},"required":["mode","lease_token"],"additionalProperties":false}}),
        daynight::tool(),
        json!({"name":"hoi4_army_detach","description":"Move chosen divisions out of an army: `divisions` are ids as hoi4_armies / hoi4_supply list them (hoi4_supply shows each division's template, equipment fill, strength and supply, so you can pick weak or strong ones). Without `into` they form one new army; with `into` they join that own army. Uses the game's controls one division at a time (the army's card, the division's row in its list, then the new-army card or a right click on the target's card), each move verified from division membership. Use it to pull weak units (irregulars, cavalry, low equipment) out of a strike army into a rear army, or to send fresh divisions forward. Refused before input when a division is not in the army or the army would be emptied (use hoi4_army_merge).","inputSchema":{"type":"object","properties":{"army":{"type":"string","maxLength":128},"divisions":{"type":"array","items":{"type":"string","maxLength":32},"minItems":1,"maxItems":18},"into":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","divisions","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_army_merge","description":"Move every division of army `source` into army `target` with the game's own controls: a left click on the source's card selects exactly its divisions, a right click on the target's card transfers them (the empty source army disappears). Use it to fold many small armies into a few full ones (24 divisions each, one general each). Refused before input when either army is unknown, they are the same, or the merged army would exceed 24 divisions (split first). Verified from division membership.","inputSchema":{"type":"object","properties":{"source":{"type":"string"},"target":{"type":"string"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["source","target","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_army_gather","description":"Put EVERY own division that is outside an army into one army, wherever the divisions stand, with the game's own controls: Shift+click on the unassigned-divisions alert selects all of them (the alert's tooltip), a right click on the army's card assigns them (army), or a click on the new-army card makes one new army of them (no army). No state is opened and the camera does not travel: the cost is the same for 3 divisions or 300. Refused before input with no_unassigned, no_such_army, army_full (the army would pass max, default 24: name another army or leave army out and split afterwards) or gui_target_unavailable (the game shows the alert some days after a division is left outside an army). Verified from division membership.","inputSchema":{"type":"object","properties":{"army":{"type":"string","maxLength":128},"max":{"type":"integer","minimum":1,"maximum":1000},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_army_split","description":"Split the first N divisions (list order, N 1..19) of an own army into a new army: the army's card, the panel's first row, Shift on row N, then the new-army card. Use it to bring an oversized army (more than 24 divisions) under its general's cap. Verified by a new army holding exactly those divisions.","inputSchema":{"type":"object","properties":{"army":{"type":"string"},"count":{"type":"integer","minimum":1,"maximum":19},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","count","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_state_slots","description":"Free shared building slots of own states (unlocked, used, queued, free) from one memory read, no interface input; up to 12 states per call. For many states use hoi4_states with the own tag and min_free_slots / sort free_slots instead. view:true opens each state's view (Find view, one click) and reads the slot text the game shows: slow, only to check a doubtful count.","inputSchema":{"type":"object","properties":{"state_ids":{"type":"array","items":{"type":"integer","minimum":1,"maximum":4095},"minItems":1,"maxItems":12},"view":{"type":"boolean"},"lease_token":{"type":"string"}},"required":["state_ids","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_army_create","description":"(optional absorb_existing: true folds armies standing there into the new one) Create one army from the unassigned own divisions standing in a state with ONE selection: the camera frames them once (the Find view only when they are not on screen, then one zoom-out swoop at most), one box around all of them (one click per stack when another army stands among them) selects them, read back from memory (a division of another army refuses the action before the card is clicked; missed stacks get one Shift+click round), then the new-army card. Verified by a new army in memory; returns its divisions. Refused before any input with already_in_army (every own division in that state belongs to a named army; use absorb_existing or hoi4_army_split) or no_own_divisions (none stand there per hoi4_armies); input_readback_failed after the click makes no claim about the cause.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"absorb_existing":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["state_id","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_army_assign_plan","description":"Pure read: where the unassigned divisions standing in a state would go (into the standing army with the most room under 24, or the named one; a new army when none stands there), how many, the overflow and the split needed afterwards. Typed refusals: no_unassigned_here, no_such_army, target_not_in_state, army_full.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"army":{"type":"string","maxLength":128}},"required":["state_id"],"additionalProperties":false}}),
        json!({"name":"hoi4_army_assign","description":"Fold the unassigned divisions standing in a state into an existing army there without disturbing other armies, or create a new army when none stands there. Same refusals as hoi4_army_assign_plan, all before any input. Folding into a standing army uses the game's own control (box-select the divisions, RIGHT-click the army's card); it refuses with selection_mismatch, before the click, when the box also took divisions of another army. Without an army it picks the army standing in the same provinces as the unassigned divisions (hoi4_armies unassigned_by_state shared_with): a stack shared with an army can only go to that army; a refused selection names those armies. Creating a new army uses the verified new-army path.","inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},"army":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["state_id","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_time","description":"Set desired pause or internal speed 0..4 (the game's speeds 1 to 5: 2 is the game's speed 3) with ordinary keys (Space, keypad +/-) sent only while the owned game window is front-most, each verified by fresh memory readback. Supply exactly one target and a stable request_id; uncertain dispatches are never replayed.","inputSchema":{"type":"object","properties":{"paused":{"type":"boolean"},"speed":{"type":"integer","minimum":0,"maximum":4},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}}),
    ])
    .chain(fleetops::tools())
    .chain(std::iter::once(invasion::tool()))
    .chain(std::iter::once(weather::tool()))
    .chain(std::iter::once(abilities::tool()))
    .chain(faction::tools())
    .chain(agency_act::tools())
    .chain(operation_act::tools())
    .chain(mio_act::tools())
    .chain(leader_act::tools())
    .chain(learn_tools::tools())
    .chain(std::iter::once(doctrine_act::officer_tool()))
    .chain(std::iter::once(army_hq::tool()))
    .collect()
}

impl Commands for MacClock {
    fn tools(&self) -> Vec<Value> {
        mac_tool_list()
    }
    // Broker allowlist note: `hoi4_country_status` is a pure read (no lease,
    // no input, no cursor); an external broker allowlist that names the other
    // read-only tools (hoi4_armies, hoi4_states, hoi4_events) should list it too.
    fn supports(&self, name: &str) -> bool {
        fleetops::handles(name)
            || leader_act::TOOLS.contains(&name)
            || learn_tools::READS.contains(&name)
            || name == learn_tools::ACT
            || name == learn_tools::RUN
            || faction::handles(name)
            || name == invasion::TOOL
            || name == weather::TOOL
            || name == army_hq::TOOL
            || name == abilities::TOOL
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
                    | "hoi4_stockpile"
                    | "hoi4_air"
                    | "hoi4_designs"
                    | "hoi4_input_lease"
                    | "hoi4_time"
                    | "hoi4_research_start"
                    | "hoi4_focus_start"
                    | "hoi4_production_add"
                    | "hoi4_production_set"
                    | "hoi4_production_priority"
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
                    | "hoi4_auto_import"
                    | "hoi4_doctrines"
                    | "hoi4_mios"
                    | "hoi4_intelligence"
                    | "hoi4_research_extras"
                    | "hoi4_states"
                    | "hoi4_wars"
                    | "hoi4_war_stats"
                    | "hoi4_battles"
                    | "hoi4_alerts"
                    | "hoi4_occupation"
                    | "hoi4_war_risk"
                    | "hoi4_save"
                    | "hoi4_railway_build"
                    | "hoi4_construction_order"
                    | "hoi4_front_plan"
                    | "hoi4_province_graph"
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
                    | "hoi4_army_attack"
                    | "hoi4_army_plan"
                    | "hoi4_army_offensive"
                    | "hoi4_army_fallback"
                    | "hoi4_army_area_defense"
                    | "hoi4_paradrop"
                    | "hoi4_army_exercise"
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
                    | "hoi4_officer_corps"
                    | "hoi4_mio_design_team"
                    | "hoi4_mio_policy"
                    | "hoi4_mio_trait"
                    | "hoi4_mio_line"
                    | "hoi4_agency_create"
                    | "hoi4_agency_upgrade"
                    | "hoi4_operative_recruit"
                    | "hoi4_operative_assign"
                    | "hoi4_operation_start"
                    | "hoi4_army_group_create"
                    | "hoi4_army_split"
                    | "hoi4_army_merge"
                    | "hoi4_army_gather"
                    | "hoi4_army_motorization"
                    | "hoi4_army_rename"
                    | "hoi4_army_detach"
                    | "hoi4_map_mode"
                    | "hoi4_map_daynight"
                    | "hoi4_occupation_law"
                    | "hoi4_division_disband"
                    | "hoi4_state_slots"
                    | "hoi4_air_wings"
                    | "hoi4_air_mission"
                    | "hoi4_air_rebase"
                    | "hoi4_air_wing_create"
                    | "hoi4_air_wing_disband"
                    | "hoi4_air_hold"
                    | "hoi4_air_attach"
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
    fn fair_facts(
        &self,
        process: &dyn Process,
    ) -> Option<std::sync::Arc<crate::fair_guard::Facts>> {
        let exe = process.identity()?.exe.clone();
        mapdata::game_dir(&exe)
            .ok()
            .map(|dir| fair_facts::facts(&dir))
    }
    fn date_stop_status(&self) -> Option<Value> {
        Some(self.daily.date_stop_status())
    }
    fn capabilities(&self) -> Value {
        json!({"transport":"ordinary macOS CGEvent keys to the front-most owned window","actions":["set_paused","set_speed","research_start","focus_start","production_add","production_set","construction_add","politics_options","politics_choose","decisions_list","decision_take","train","deploy_locations","army_create","army_assign","army_front","army_plan","army_offensive","event_choose","diplomacy","diplomacy_act","army_commander","army_group_create","army_split","template_create","template_edit","template_rename","template_delete","template_options","design_options","design_create","design_obsolete","design_set_production","state_slots","air_wings","air_mission","plan_run"],"lease":"exclusive configured controller","input_profile":{"name":"macos_cocoa_absolute_1920x1080_v1","checked_at_bind":true,"rechecked_before_each_input":true,"diagnostic_alone_authorizes_input":false},"construction_panel":{"enabled":self.construction_panel,"qualification_only":true,"ordinary_key":"t","live_delivery_verified":false,"input_retry":false},"construction_selection":{"enabled":self.construction_selection,"qualification_only":true,"ordinary_clicks_max":1,"input_retry":false,"placement":false},"construction_placement":{"enabled":self.construction_placement,"qualification_only":true,"ordinary_clicks_max":1,"full_picker_verified":false,"atomic_event_verified":false},"construction_cancellation":{"enabled":self.construction_cancellation,"qualification_only":true,"ordinary_key":"escape","ordinary_key_presses_max":1,"input_retry":false,"worker":false},"construction_find_open":{"enabled":self.construction_find_open,"qualification_only":true,"transport":"ordinary_left_click","ordinary_clicks_max":1,"cursor_moves_max":1,"ordinary_key_presses":0,"input_retry":false,"worker":false},"construction_navigation":{"enabled":self.construction_navigation,"qualification_only":true,"already_open_only":true,"input_retry":false,"placement":false},"daily_governor":self.daily.capabilities(),"native_posting":false,"full_campaign_acceptance":false})
    }
    fn call(&self, process: &dyn Process, name: &str, arguments: &Value) -> Result<Value> {
        // A learned skill runs its steps through this same entry, one ordinary tool call each.
        if name == learn_tools::RUN {
            let listed = self.tools();
            return learn_tools::skill_run(
                &self.directory,
                process,
                &listed,
                &|tool, args| self.call(process, tool, args),
                arguments,
            );
        }
        // Plan role names (army_layout name_hint) stand for the army bound to the role; a
        // settlement of an earlier dispatch resolves the same way, so its action still matches.
        let (resolved_arguments, resolved) = self.resolve_army_roles(process, name, arguments);
        let arguments = resolved_arguments.as_ref().unwrap_or(arguments);
        // An order (a call with a request_id) writes where its seconds went (crate::phase).
        let order = arguments
            .get("request_id")
            .is_some()
            .then(crate::phase::begin);
        let result = self.call_resolved(process, name, arguments);
        if let Some(parts) = order.and_then(crate::phase::Order::finish) {
            eprintln!(
                "[order] {}",
                json!({"tool": name, "kind": order_kind(name, arguments),
                    "ok": result.is_ok(),
                    "status": result.as_ref().ok().and_then(|v| v.get("status")
                        .or_else(|| v.get("receipt").and_then(|r| r.get("status")))).cloned(),
                    "error": result.as_ref().err().map(|e| e.code),
                    "ms": parts})
            );
        }
        if resolved.is_empty() {
            return result;
        }
        let note: serde_json::Map<String, Value> = resolved
            .iter()
            .map(|(asked, army)| (asked.clone(), json!(army)))
            .collect();
        match result {
            Ok(mut value) => {
                if let Some(object) = value.as_object_mut() {
                    object.insert("army_resolved".into(), Value::Object(note));
                }
                Ok(value)
            }
            Err(mut e) => {
                let named: Vec<String> = resolved
                    .iter()
                    .map(|(asked, army)| format!("{asked} is {army}"))
                    .collect();
                e.message = format!("{} (plan role: {})", e.message, named.join(", "));
                Err(e)
            }
        }
    }
}
impl MacClock {
    fn call_resolved(&self, process: &dyn Process, name: &str, arguments: &Value) -> Result<Value> {
        // A request_id already in the journal is a settlement of an earlier dispatch: let it
        // through so a typo's open journal can still be settled.
        let journaled = arguments
            .get("request_id")
            .and_then(Value::as_str)
            .is_some_and(|id| {
                self.state
                    .lock()
                    .map(|c| c.actions.receipt_exists(id))
                    .unwrap_or(false)
            });
        // Real campaigns never pause: an action (not the clock tool, not the peace screens) that
        // changes the pause state through its own input gets the starting state put back (see
        // pause_guard; Ethiopia run 11: the game stood paused for 100 s after a plan-runner
        // tick with a focus and a front).
        // A read that drives the interface (hoi4_politics_options opens the seat's window) can
        // pause the game as surely as an action: every call is guarded, a memory read only costs
        // two clock reads (pm2, 17 Nov 2026: the game stood paused for four minutes after a
        // politics read sent without a request id).
        // A run that fixes the game speed (HOI4_SPEED_LOCK=<internal speed 0..4>, set by a
        // no-pause campaign) refuses a player's speed change to another value: pr1 (17 Nov 2026)
        // the player set speed 2 for a stretch of 1937 against a brief that says speed 3.
        if name == "hoi4_time" && !crate::autopilot::autopilot().in_step() {
            // the run's player is an agent: its speed change is not a human's (plan_speed)
            crate::plan_run::note_agent_speed();
            if let Some(Err(e)) = arguments
                .get("speed")
                .and_then(Value::as_i64)
                .map(|want| speed_lock(std::env::var("HOI4_SPEED_LOCK").ok().as_deref(), want))
            {
                return Err(e);
            }
        }
        let guard_pause = !journaled
            && name != "hoi4_time"
            && !name.starts_with("hoi4_peace")
            && process.identity() == self.process.identity();
        let scope = guard_pause.then(|| self.pause.begin());
        let menus_before = ui::menus_closed();
        let sent_before = crate::pause_guard::sent_on_this_thread();
        let started = std::time::Instant::now();
        // A direct player order on an army takes it from the executor's war rules for a while.
        if name.starts_with("hoi4_army_") && !crate::autopilot::autopilot().in_step() {
            if let Some(army) = arguments.get("army").and_then(Value::as_str) {
                if arguments.get("request_id").is_some() {
                    crate::autopilot::autopilot().claim(army);
                }
            }
        }
        // Army orders leave the camera at unit level so a spectator can follow the units
        // (maintainer, 8 Nov 2026); the strategic view comes back before the next action that
        // is not an army order.
        let army_tool = name.starts_with("hoi4_army_");
        if !army_tool
            && !journaled
            && arguments.get("request_id").is_some()
            && process.identity() == self.process.identity()
        {
            let _ = screenmap::restore_zoom(self.process.as_ref(), &*self.keys);
        }
        // An action that is not an army order starts with nothing selected: a selection left
        // by the last army order keeps the unit list and the battle-plan bar over the map and
        // the panels, and map clicks then land on them or become orders.
        if !army_tool
            && !journaled
            && name != "hoi4_time"
            && arguments.get("lease_token").is_some()
            && process.identity() == self.process.identity()
        {
            let _ = armies::clear_selection(self.process.as_ref(), &*self.keys);
        }
        let result = (if journaled {
            Ok(())
        } else {
            army_names_exist(process, name, arguments, &|| self.army_roles_text()).and_then(|()| {
                self.army_screen_ready(process, name, arguments.get("lease_token").is_some())
            })
        })
        .and_then(|()| self.call_inner(process, name, arguments))
        .and_then(not_applied_is_error);
        // Map actions zoom in to aim (Find, border clicks); give the player and the viewer the
        // strategic view back afterwards. Best effort: never changes the tool's result.
        if MAP_TOOLS.contains(&name) && !army_tool && process.identity() == self.process.identity()
        {
            let _ = screenmap::restore_zoom(self.process.as_ref(), &*self.keys);
        }
        let Some(scope) = scope else {
            return result;
        };
        let verdict = if crate::pause_guard::sent_on_this_thread() == sent_before {
            scope.untouched()
        } else {
            end_pause_scope(self.process.as_ref(), &*self.keys, scope, menus_before)
        };
        // One context line per action for the run report: which actions changed the pause
        // state, whether the change was our own input and whether it was put back.
        eprintln!(
            "[action] {}",
            json!({"unix_ms": std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0),
                "tool": name,
                "request_id": arguments.get("request_id"),
                "ms": started.elapsed().as_millis() as u64,
                "ok": result.is_ok(),
                "error": result.as_ref().err().map(|e| e.code),
                "paused_before": verdict.before,
                "paused_after": verdict.after,
                "self_pause": verdict.ours,
                "restored": verdict.restored,
                "executor": crate::autopilot::autopilot().in_step()})
        );
        if !verdict.ours {
            return result;
        }
        // The caller sees it too: a pause of ours is a defect to count even when undone.
        result.map(|mut value| {
            if let Some(object) = value.as_object_mut() {
                object.insert(
                    "pause_guard".into(),
                    json!({"changed_by_own_input": true,
                        "paused_before": verdict.before,
                        "restored": verdict.restored}),
                );
            }
            value
        })
    }
    /// Role bindings the executor keeps (plan_state.json `layout` next to plan.json) and the
    /// role names of the plan (`army_layout`); empty without a plan.
    fn army_roles(&self) -> (std::collections::BTreeMap<String, String>, Vec<String>) {
        let plan = self.directory.join("plan.json");
        let saved = std::fs::read(crate::plan_run::state_path(&plan))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .unwrap_or(Value::Null);
        let planned = crate::plan_run::read_plan(&plan)
            .map(|p| army_roles::planned(&p))
            .unwrap_or_default();
        (army_roles::bound(&saved), planned)
    }
    fn army_roles_text(&self) -> String {
        let (bound, planned) = self.army_roles();
        army_roles::describe(&bound, &planned)
    }
    /// The arguments with plan role names rewritten to their armies (None when nothing was),
    /// and each (asked, army). Reads the armies only when an army argument is given.
    #[allow(clippy::type_complexity)]
    fn resolve_army_roles(
        &self,
        process: &dyn Process,
        name: &str,
        arguments: &Value,
    ) -> (Option<Value>, Vec<(String, String)>) {
        if !army_roles::addressed(name) || process.identity() != self.process.identity() {
            return (None, Vec::new());
        }
        let asked = army_roles::asked(arguments);
        if asked.is_empty() {
            return (None, Vec::new());
        }
        let Ok(view) = armies::armies(process) else {
            return (None, Vec::new());
        };
        let names = army_and_group_names(&view);
        if asked.iter().all(|a| names.contains(a)) {
            return (None, Vec::new());
        }
        let (bound, _) = self.army_roles();
        let (out, done) = army_roles::rewrite(arguments, &names, &bound);
        if done.is_empty() {
            (None, done)
        } else {
            (Some(out), done)
        }
    }
    /// Army orders need the political map with no panel over it: a run left the map in naval
    /// mode with the diplomacy panel open and every army order failed until both were cleared
    /// by hand (Ethiopia run 10). F1 is the game's own political map-mode key.
    /// The game's paused flag, read like the clock tool reads it (None when unreadable).
    fn army_screen_ready(&self, process: &dyn Process, name: &str, leased: bool) -> Result<()> {
        // Input (panel closing, F1) only for actions: a lease-free read such as
        // hoi4_army_assign_plan runs on the read lane, beside an action, and must send nothing.
        let army_tool =
            (name.starts_with("hoi4_army_") || name == "hoi4_division_disband") && leased;
        if !army_tool || process.identity() != self.process.identity() {
            return Ok(());
        }
        let process = self.process.as_ref();
        panelkit::close_panels(process, &*self.keys)?;
        self.keys.press_code(122)?;
        crate::phase::sleep(std::time::Duration::from_millis(150));
        Ok(())
    }
}
/// Close one action's pause bracket for any input lane. The game menu is closed first (an
/// Escape with nothing left to close opens it; it pauses the game and closing it leaves the
/// game paused), then a pause state our own input changed is put back with the game's own
/// pause key. A pause by the player or a multiplayer host stays.
pub(super) fn end_pause_scope(
    process: &dyn Process,
    keys: &dyn GameInput,
    scope: crate::pause_guard::Scope<'_>,
    menus_before: u64,
) -> crate::pause_guard::Verdict {
    // A key sent as the action's last input has had its effect before the menu is looked for.
    scope.settle();
    let menu = ui::menu_shown(process);
    let _ = ui::close_menu(process, keys);
    scope.end(menu || ui::menus_closed() != menus_before, &mut |attempt| {
        // An open focus tree's search box would take the pause key as text.
        let _ = panelkit::close_focus_tree(process, keys);
        if attempt > 0 {
            // The first key changed nothing: a text box of an open panel may hold the keyboard.
            let _ = panelkit::close_panels(process, keys);
        }
        keys.press_clock(Key::Pause)
    })
}
/// Unpause a game the save left paused: one clock key, read back within 1.5 s. True when it
/// runs again (or was never paused by the save).
fn resume_after_save(process: &dyn Process, keys: &dyn GameInput) -> bool {
    let paused = || crate::game::clock_context(process).map(|c| c.paused).ok();
    let settle = std::time::Instant::now() + std::time::Duration::from_millis(600);
    while std::time::Instant::now() < settle {
        if paused() == Some(true) {
            break;
        }
        crate::phase::sleep(std::time::Duration::from_millis(50));
    }
    if paused() != Some(true) {
        return true;
    }
    if keys.press_clock(Key::Pause).is_err() {
        return false;
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
    while std::time::Instant::now() < deadline {
        if paused() == Some(false) {
            return true;
        }
        crate::phase::sleep(std::time::Duration::from_millis(50));
    }
    false
}
/// Army names are checked before anything is journaled: a typo used to become an uncertain
/// dispatch that blocked every later action (seen live: "Army 1 1 ").
/// Plan role names are resolved before this check; `roles` describes them for the refusal.
fn army_names_exist(
    process: &dyn Process,
    name: &str,
    arguments: &Value,
    roles: &dyn Fn() -> String,
) -> Result<()> {
    if !name.starts_with("hoi4_army_")
        || matches!(name, "hoi4_army_create" | "hoi4_army_assign_plan")
    {
        return Ok(());
    }
    let wanted = army_roles::asked(arguments);
    if wanted.is_empty() {
        return Ok(());
    }
    let view = armies::armies(process)?;
    let names = army_and_group_names(&view);
    for w in wanted {
        if !names.contains(&w) {
            let roles = roles();
            let roles = if roles.is_empty() {
                String::new()
            } else {
                format!(" Plan roles: {roles}.")
            };
            return Err(Error::new(
                "army_unknown",
                format!(
                    "No own army or army group is named {w:?}; nothing was done. Names: {}.{roles}",
                    names.join(", ")
                ),
            ));
        }
    }
    Ok(())
}
/// Own army and army group names of a `hoi4_armies` view.
fn army_and_group_names(view: &Value) -> Vec<String> {
    let mut names: Vec<String> = view["armies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| a["army"].as_str().map(str::to_string))
        .collect();
    names.extend(
        view["army_groups"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|g| g["army_group"].as_str().map(str::to_string)),
    );
    names
}
/// A settled receipt whose target did not happen (NotApplied) or happened only in part
/// (Diverged) is a failure for the caller, never a success with a buried status (seen live:
/// an army_commander retry settled with no general and was read as success).
fn not_applied_is_error(value: Value) -> Result<Value> {
    let status = value["receipt"]["status"].as_str().unwrap_or("");
    let code = match status {
        "NotApplied" => "action_not_applied",
        "Diverged" => "action_diverged",
        _ => return Ok(value),
    };
    let detail = value["receipt"]["detail"].to_string();
    Err(Error::new(
        code,
        format!(
            "The action did not reach its target ({status}); game state now: {detail}. \
             Read the state again and send a corrected action with a NEW request_id \
             (this id stays settled)."
        ),
    ))
}
/// `organization` on hoi4_production_add and hoi4_design_set_production: the organisation the
/// line is made with. With Arms Against Tyranny a plain click on a design opens the organisation
/// chooser (interface/military_industrial_organization/industrial_organization_list.gui:
/// `industrial_organisation_list_window` at `position_default`, rows `industrial_organisation_list_item`
/// in `organisation_list`, each with `name`, `task_capacity` and its `background_button`), and the
/// tools Shift+click past it today. Choosing one is not built yet, so the argument is accepted by
/// the schema and refused before any input (`not_yet_built`); plan_run::UNBUILT_ARGS keeps the
/// executor from sending it until the chooser is driven and verified live.
///
/// `state` on hoi4_occupation_law: the law of one occupied state instead of the default law. The
/// Occupied Territories view lists each state (`occupied_territory_state_entry`, with its own
/// `select_law_button` and `selected_law`) and the state view has the same button
/// (countrystateview.gui `law_and_template_container`); the tool drives the default-law button
/// only today, so the argument is refused the same way (gap register occupation.per_state_law).
///
/// `replace` on hoi4_doctrine_adopt: swapping the folder's grand doctrine or a track's chosen
/// subdoctrine for another (the officer corps view's slot, picked again while filled). The tool
/// adopts into empty slots only and refuses a filled one before input (grand_doctrine_set,
/// track_occupied), so the argument is refused (gap register doctrines.replace_doctrine).
///
/// `force_government` on hoi4_peace_take: the peace conference's force-government demand
/// (peace.rs reads its button, `force_government_action`, among the window's controls): picked
/// with one state of the loser, it turns the country's government to the demander's ideology. The
/// tool drives the take, puppet and release buttons only, so the argument is refused (gap
/// register peace.force_government).
pub(super) fn refuse_unbuilt_args(tool: &str, args: &serde_json::Map<String, Value>) -> Result<()> {
    for (t, arg) in crate::plan_run::UNBUILT_ARGS {
        if *t != tool {
            continue;
        }
        let Some(value) = args.get(*arg) else {
            continue;
        };
        let what = match *arg {
            "organization" => "choosing the military-industrial organisation (the line gets none)",
            "state" => "setting the law of one state (only the default law is set)",
            "replace" => "replacing a chosen doctrine (an occupied slot is refused)",
            "settings" => "the area order's settings toggles (the game's defaults apply)",
            "force_government" => {
                "the force-government demand (take, puppet and release are built)"
            }
            _ => "this argument",
        };
        return Err(Error::new(
            "not_yet_built",
            format!("{tool}: `{arg}` ({value}) is not built yet: {what}; send the call without it"),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod receipt_tools {
    /// Every tool the executor may send (plan_run::RECEIPT_TOOLS) is served here, so the list
    /// cannot name a tool this controller does not answer.
    #[test]
    fn every_executor_action_is_a_served_tool() {
        let served: Vec<String> = super::mac_tool_list()
            .iter()
            .filter_map(|t| t["name"].as_str().map(str::to_string))
            .collect();
        let missing: Vec<&&str> = crate::plan_run::RECEIPT_TOOLS
            .iter()
            .filter(|n| !served.iter().any(|s| s == **n))
            .collect();
        assert!(missing.is_empty(), "not served: {missing:?}");
    }

    /// Every argument the executor holds back as not built is in the served tool's schema and
    /// refused before input.
    #[test]
    fn every_unbuilt_argument_is_in_the_schema_and_refused() {
        let tools = super::mac_tool_list();
        for (tool, arg) in crate::plan_run::UNBUILT_ARGS {
            let t = tools
                .iter()
                .find(|t| t["name"] == *tool)
                .unwrap_or_else(|| panic!("{tool} not served"));
            assert!(
                t["inputSchema"]["properties"].get(*arg).is_some(),
                "{tool} schema lacks {arg}"
            );
            assert!(t["description"].as_str().unwrap().contains("not yet built"));
            let mut args = serde_json::Map::new();
            args.insert((*arg).to_string(), "x".into());
            let e = super::refuse_unbuilt_args(tool, &args).unwrap_err();
            assert_eq!(e.code, "not_yet_built");
            assert!(super::refuse_unbuilt_args(tool, &serde_json::Map::new()).is_ok());
        }
    }
}
/// Tools that move the map camera; the camera is zoomed back out after them (army tools
/// excepted: their camera stays at unit level until the next other action).
const MAP_TOOLS: &[&str] = &[
    "hoi4_occupation_law",
    "hoi4_army_detach",
    "hoi4_army_create",
    "hoi4_army_assign",
    "hoi4_army_front",
    "hoi4_army_attack",
    "hoi4_army_offensive",
    "hoi4_army_fallback",
    "hoi4_army_area_defense",
    "hoi4_army_move",
    "hoi4_army_split",
    "hoi4_army_merge",
    "hoi4_army_gather",
    "hoi4_construction_add",
    "hoi4_fleet_region",
    "hoi4_naval_invasion",
    "hoi4_air_mission",
    "hoi4_air_rebase",
    "hoi4_peace_take",
];
impl MacClock {
    fn call_inner(&self, process: &dyn Process, name: &str, arguments: &Value) -> Result<Value> {
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
        if learn_tools::READS.contains(&name) {
            return learn_tools::store_call(name, args, process, &self.directory);
        }
        if name == "hoi4_daily_governor" {
            return self.daily.call(self, args);
        }
        if name == "hoi4_army_attack" {
            // One call for front + offensive + execute: each step is its own journalled action
            // (request ids <id>:front, :offensive, :execute), the per-call panel reset and zoom
            // restore run once, and the first failing step ends the call with the steps so far.
            allowed(
                args,
                &[
                    "army",
                    "state_id",
                    "front",
                    "offensive",
                    "execute",
                    "request_id",
                    "lease_token",
                ],
            )?;
            let id = string(args, "request_id")?.to_string();
            let step = |flag: &str| args.get(flag).and_then(Value::as_bool).unwrap_or(true);
            let mut done = Vec::new();
            for (flag, tool) in [
                ("front", "hoi4_army_front"),
                ("offensive", "hoi4_army_offensive"),
                ("execute", "hoi4_army_plan"),
            ] {
                if !step(flag) {
                    continue;
                }
                let mut sub = serde_json::Map::new();
                sub.insert("army".into(), args["army"].clone());
                if flag == "execute" {
                    sub.insert("execute".into(), json!(true));
                } else {
                    sub.insert("state_id".into(), args["state_id"].clone());
                }
                sub.insert("request_id".into(), json!(format!("{id}:{flag}")));
                sub.insert("lease_token".into(), args["lease_token"].clone());
                match self.call_inner(process, tool, &Value::Object(sub)) {
                    Ok(v) => done.push(json!({"step": flag, "result": v})),
                    Err(e) => {
                        return Err(Error::new(
                            e.code,
                            format!(
                                "{} failed: {} (steps done before it: {})",
                                flag,
                                e.message,
                                serde_json::to_string(&done).unwrap_or_default()
                            ),
                        ))
                    }
                }
            }
            return Ok(json!({"army": args["army"], "state_id": args["state_id"], "steps": done}));
        }
        if name == "hoi4_country_status" {
            allowed(args, &[])?;
            return country_status::status(process);
        }
        if matches!(
            name,
            "hoi4_resources"
                | "hoi4_production_shortages"
                | "hoi4_industry"
                | "hoi4_economy_detail"
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
                &[
                    "query",
                    "relation",
                    "faction",
                    "only_with_states",
                    "limit",
                    "offset",
                ],
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
        if name == "hoi4_stockpile" {
            allowed(args, &["equipment"])?;
            return stockpile::stockpile(process, args);
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
                return Err(Error::new(
                    "invalid_arguments",
                    "tag must be three capital letters",
                ));
            }
            return mios::mios(process, tag, mgmt::flag(args, "include_all")?);
        }
        if name == weather::TOOL {
            return weather::call(process, args);
        }
        if name == army_hq::TOOL {
            return army_hq::call(process, args);
        }
        if name == abilities::TOOL {
            return abilities::call(process, args);
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
            allowed(args, &["strength_below", "summary", "army"])?;
            let below = match args.get("strength_below") {
                None => 0.7,
                Some(v) => v
                    .as_f64()
                    .filter(|v| (0.0..=1.0).contains(v))
                    .ok_or_else(|| {
                        Error::new("invalid_arguments", "strength_below must be 0..1")
                    })?,
            };
            let mut full = supply::report(process, below)?;
            // the hub defines as the running game (and its mods) load them: the executor sizes
            // trucks by these, never by a number of its own
            if let Ok(dir) = mgmt::game_dir(process) {
                let defines = crate::script::cached(&dir, "supply_defines", supply::defines_in);
                if let Some(o) = full.as_object_mut() {
                    o.insert("defines".into(), (*defines).clone());
                }
            }
            // summary:true keeps the per-army rows, totals and short lists (a few KB instead of
            // 40-60 KB of division rows); army:"X" keeps one army's divisions.
            if let Some(army) = args.get("army").and_then(Value::as_str) {
                for key in ["divisions", "below_70", "below_50", "shortages"] {
                    if let Some(list) = full[key].as_array_mut() {
                        list.retain(|d| d["army"] == army);
                    }
                }
                if let Some(list) = full["armies"].as_array_mut() {
                    list.retain(|d| d["army"] == army);
                }
            }
            if args.get("summary").and_then(Value::as_bool) == Some(true) {
                let count = |k: &str| full[k].as_array().map_or(0, Vec::len);
                let counts = json!({"divisions": count("divisions"), "below_70": count("below_70"),
                    "below_50": count("below_50"), "shortages": count("shortages"),
                    "overdrawn_provinces": count("overdrawn_provinces"), "provinces": count("provinces")});
                if let Some(o) = full.as_object_mut() {
                    for key in ["divisions", "provinces", "states", "supply_nodes"] {
                        o.remove(key);
                    }
                    for key in ["below_70", "below_50", "shortages", "overdrawn_provinces"] {
                        if let Some(list) = o.get_mut(key).and_then(Value::as_array_mut) {
                            list.truncate(10);
                        }
                    }
                    o.insert("counts".into(), counts);
                    o.insert(
                        "summary_note".into(),
                        json!("short lists cut to 10; call without summary (or with army) for every division"),
                    );
                }
            }
            return Ok(full);
        }
        if name == "hoi4_template_get" {
            allowed(args, &["template"])?;
            let only = match args.get("template") {
                None => None,
                Some(v) => Some(
                    v.as_str()
                        .filter(|s| {
                            !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control)
                        })
                        .ok_or_else(|| Error::new("invalid_arguments", "Invalid template name"))?,
                ),
            };
            return template_get::get(process, only);
        }
        if name == "hoi4_special_projects" {
            allowed(args, &["rules"])?;
            return special_projects::list(
                process,
                special_projects::rules_arg(args.get("rules"))?,
            );
        }
        if name == "hoi4_armies" {
            allowed(args, &["threat"])?;
            let mut view = armies::armies(process)?;
            if args.get("threat") == Some(&json!(false)) {
                return Ok(view);
            }
            // Pocket warning per army: the share of the land provinces around its divisions that
            // another country holds (w24: armies were encircled with no warning).
            if let Some(exe) = process.identity().map(|i| i.exe.clone()) {
                if let Ok(dir) = mapdata::game_dir(&exe) {
                    let _ = armies::add_threats(process, &dir, &mut view);
                }
            }
            return Ok(view);
        }
        if name == "hoi4_war_risk" {
            allowed(args, &["country"])?;
            let tag = string(args, "country")?;
            if tag.len() != 3
                || !tag
                    .bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            {
                return Err(Error::new(
                    "invalid_arguments",
                    "country must be a three-letter tag",
                ));
            }
            let (joiners, weak) = Self::war_risk(process, tag)?;
            return Ok(
                json!({"country":tag,"would_join":joiners,"borders_not_held":weak,
                "declare_would_be_refused":!weak.is_empty()}),
            );
        }
        if name == "hoi4_alerts" {
            allowed(args, &[])?;
            return alerts::alerts(process);
        }
        if name == "hoi4_war_stats" {
            allowed(args, &["all"])?;
            return war_stats::stats(process, args);
        }
        if name == "hoi4_battles" {
            allowed(args, &["country", "all", "limit", "divisions"])?;
            return battles::battles(process, args);
        }
        if name == "hoi4_occupation" {
            allowed(args, &["scope", "limit"])?;
            let exe = &process
                .identity()
                .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
                .exe;
            let mut out = occupation::occupation(process, &mapdata::game_dir(exe)?, args)?;
            if let Ok((laws, rows)) = occupation_laws(exe, &out) {
                out["laws"] = occlaw::law_rows(&laws, &rows);
                out["laws_source"] = json!("common/occupation_laws of the running game's content in the Default Law list's order; shown: whether the game shows the law to the player (null: its visible trigger reads something no tool reads; the ruling ideology is the history's)");
            }
            return Ok(out);
        }
        if name == "hoi4_wars" {
            allowed(args, &["enemies", "max_states"])?;
            let exe = &process
                .identity()
                .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
                .exe;
            return wars::wars(process, &mapdata::game_dir(exe)?, args);
        }
        if name == "hoi4_province_graph" {
            allowed(args, &["states"])?;
            let states: Vec<i32> = args
                .get("states")
                .and_then(Value::as_array)
                .map(|rows| {
                    rows.iter()
                        .filter_map(Value::as_i64)
                        .filter(|v| (1..4096).contains(v))
                        .map(|v| v as i32)
                        .collect()
                })
                .unwrap_or_default();
            if states.is_empty() || states.len() > 128 {
                return Err(Error::new(
                    "invalid_arguments",
                    "states must list 1..128 state ids",
                ));
            }
            let exe = &process
                .identity()
                .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
                .exe;
            let dir = mapdata::game_dir(exe)?;
            let neighbours = wars::province_neighbours(&dir)?;
            // terrain, rivers, forts and special crossings (map files); the graph stands without them
            let terrain = terrain::map_terrain(&dir);
            let mut rows = Vec::new();
            for state in states {
                // A state id the map does not know is left out, not an error for the others.
                let Ok(provinces) = mapdata::state_provinces(&dir, state) else {
                    continue;
                };
                for province in provinces {
                    // Lakes and sea provinces a state lists have no land edge.
                    let Some(next) = neighbours.get(&province) else {
                        continue;
                    };
                    let mut next = next.clone();
                    next.sort_unstable();
                    let mut row =
                        json!({"province": province, "state_id": state, "neighbours": next});
                    if let Ok(t) = &terrain {
                        if let (Some(o), Value::Object(extra)) =
                            (row.as_object_mut(), t.province(province))
                        {
                            o.extend(extra);
                        }
                        row["borders"] = json!(next
                            .iter()
                            .map(|n| t.border(province, *n))
                            .collect::<Vec<_>>());
                    }
                    rows.push(row);
                }
            }
            let mut out = json!({"provinces": rows, "source": "map files"});
            match &terrain {
                Ok(t) => out["rules"] = t.rules(),
                Err(e) => out["terrain_error"] = json!(e),
            }
            return Ok(out);
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
                "hoi4_encirclement_candidates" => {
                    front_plan::encirclement_tool(process, &dir, args)
                }
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
            allowed(
                args,
                &[
                    "query",
                    "ids",
                    "owned_only",
                    "min_free_slots",
                    "building",
                    "min_level",
                    "max_level",
                    "resource",
                    "sort",
                    "limit",
                    "offset",
                ],
            )?;
            let table = state_table::Query::from_args(args)?;
            let query = string(args, "query")?;
            if query.len() > 64 || query.chars().any(char::is_control) {
                return Err(Error::new("invalid_arguments", "Invalid state query"));
            }
            let exe = &process
                .identity()
                .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
                .exe;
            return states::lookup(process, &mapdata::game_dir(exe)?, query, table.as_ref());
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
        if input_gate::applies(name, args) {
            if state.journal.blocks_game(&state.guard.epoch.game) {
                // A clock request left open (the game was unreadable when it ran, or a server
                // stopped mid-key) settles from one fresh read before it can block anything.
                let Control { guard, journal, .. } = &mut *state;
                let epoch = guard.epoch.clone();
                let _ = journal.settle_open(&epoch, &mut || guard.observe(process));
            }
            input_gate::check_in(
                &state.journal,
                &state.actions,
                &state.guard.epoch.game,
                name == "hoi4_time",
                args.get("request_id").and_then(Value::as_str),
                journal::domain_of(name),
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
        if name == learn_tools::ACT {
            return learn_tools::act(args, process, &self.keys, guard, authority, actions);
        }
        if mio_act::TOOLS.contains(&name) {
            return mio_act::call(name, args, process, &self.keys, guard, authority, actions);
        }
        if leader_act::TOOLS.contains(&name) {
            return leader_act::call(name, args, process, &self.keys, guard, authority, actions);
        }
        if agency_act::TOOLS.contains(&name) {
            return agency_act::call(name, args, process, &self.keys, guard, authority, actions);
        }
        if operation_act::TOOLS.contains(&name) {
            return operation_act::call(name, args, process, &self.keys, guard, authority, actions);
        }
        if name == "hoi4_doctrine_adopt" {
            return doctrine_act::call(args, process, &self.keys, guard, authority, actions);
        }
        if name == "hoi4_officer_corps" {
            return doctrine_act::officer_call(
                args, process, &self.keys, guard, authority, actions,
            );
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
                Some(special_projects::plan(
                    &before, facility, scientist, project,
                )?)
            };
            // engineer 85: in process (family `projects`): the tab's own commands for the objects of the game's lists
            let mut sp_route = if retry {
                None
            } else {
                inproc::route("projects")
            };
            let sp_posts = match (sp_route.as_ref(), plan.as_ref()) {
                (Some(_), Some(p)) => special_projects::objects(process)
                    .ok()
                    .and_then(|o| inproc_projects::assign_posts(&o, p).ok().flatten()),
                _ => None,
            };
            if sp_posts.is_none() {
                sp_route = None;
            }
            if !retry && sp_route.is_none() {
                bounded_keys.preflight()?;
            }
            let sp_posted: std::cell::Cell<Option<Instant>> = std::cell::Cell::new(None);
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
            let mut sp_reply: Option<Value> = None;
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
                let early = sp_posted
                    .get()
                    .is_some_and(|t| t.elapsed() < inproc::IPC_SETTLE);
                Ok(if early && verdict != settle::Verdict::Reached {
                    journal::Outcome::Unknown
                } else if retry && verdict == settle::Verdict::Unchanged {
                    // A retry's own before-snapshot proves nothing about the first attempt.
                    journal::Outcome::Diverged(detail)
                } else {
                    verdict.outcome(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let plan = plan.as_ref().ok_or_else(|| {
                    Error::new(
                        "action_uncertain",
                        "A retried request is never dispatched again",
                    )
                })?;
                if let (Some(ch), Some(posts)) = (sp_route.as_mut(), sp_posts.as_ref()) {
                    if let Some(r) = inproc_projects::post_all(ch, posts)? {
                        sp_posted.set(Some(Instant::now()));
                        sp_reply = Some(json!(r));
                        return Ok(());
                    }
                    keys.preflight()?;
                }
                special_projects::assign(process, keys, plan)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(with_inproc(receipt, sp_reply));
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
            }
            // engineer 85: in process (family `projects`)
            let mut sp_route = if retry {
                None
            } else {
                inproc::route("projects")
            };
            let sp_post = match sp_route.as_ref() {
                Some(_) => special_projects::objects(process).ok().and_then(|o| {
                    if stopping {
                        inproc_projects::stop_post(&before, &o, facility)
                    } else {
                        inproc_projects::unassign_post(&before, &o, facility)
                    }
                    .ok()
                    .flatten()
                }),
                None => None,
            };
            if sp_post.is_none() {
                sp_route = None;
            }
            if !retry && sp_route.is_none() {
                bounded_keys.preflight()?;
            }
            let sp_posted: std::cell::Cell<Option<Instant>> = std::cell::Cell::new(None);
            let mut sp_reply: Option<Value> = None;
            let sp_early = || {
                sp_posted
                    .get()
                    .is_some_and(|t| t.elapsed() < inproc::IPC_SETTLE)
            };
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
                    Ok(if sp_early() && verdict != settle::Verdict::Reached {
                        journal::Outcome::Unknown
                    } else if retry && verdict == settle::Verdict::Unchanged {
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
                    Ok(if sp_early() && verdict != settle::Verdict::Reached {
                        journal::Outcome::Unknown
                    } else if retry && verdict == settle::Verdict::Unchanged {
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
                if let (Some(ch), Some(post)) = (sp_route.as_mut(), sp_post.as_ref()) {
                    if let Some(r) = inproc_projects::post_all(ch, std::slice::from_ref(post))? {
                        sp_posted.set(Some(Instant::now()));
                        sp_reply = Some(json!(r));
                        return Ok(());
                    }
                    keys.preflight()?;
                }
                if stopping {
                    special_projects::stop(process, keys, facility)
                } else {
                    special_projects::unassign(process, keys, facility)
                }
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(with_inproc(receipt, sp_reply));
        }
        if name == "hoi4_design_options" {
            allowed(
                args,
                &["kind", "base", "slot", "measure_costs", "lease_token"],
            )?;
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
                        .filter(|s| {
                            !s.is_empty()
                                && s.len() <= 64
                                && s.bytes().all(|b| {
                                    b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'
                                })
                        })
                        .ok_or_else(|| Error::new("invalid_arguments", "Invalid slot name"))?,
                ),
            };
            let measure = args
                .get("measure_costs")
                .and_then(Value::as_bool)
                .unwrap_or(true);
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
                    &[
                        "line",
                        "design",
                        "accept_progress_loss",
                        "organization",
                        "request_id",
                        "lease_token",
                    ],
                )?;
                refuse_unbuilt_args(name, args)?;
            }
            let design = design_arg(
                process,
                args.get("design").ok_or_else(|| {
                    Error::new("invalid_arguments", "design is required (id or name)")
                })?,
            )?;
            let want_obsolete = match args.get("obsolete") {
                None => true,
                Some(v) if obsolete => v
                    .as_bool()
                    .ok_or_else(|| Error::new("invalid_arguments", "obsolete must be a boolean"))?,
                Some(_) => {
                    return Err(Error::new(
                        "invalid_arguments",
                        "obsolete applies to hoi4_design_obsolete",
                    ))
                }
            };
            let line = if obsolete {
                0
            } else {
                let raw = args.get("line").and_then(Value::as_u64).ok_or_else(|| {
                    Error::new(
                        "invalid_arguments",
                        "line must be a line index (hoi4_production_lines) or a line handle",
                    )
                })?;
                // A small number is the line's index (as hoi4_production_lines numbers it):
                // resolved to its handle here so a retried request settles the same line.
                if raw < 4096 {
                    production::lines(process)?
                        .get(raw as usize)
                        .map(|l| l.handle)
                        .ok_or_else(|| {
                            Error::new("line_unknown", format!("No own production line {raw}"))
                        })?
                } else {
                    raw
                }
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
                let before = all
                    .iter()
                    .find(|d| d.variant.id == design)
                    .map(|d| d.variant.obsolete);
                let found = if retry {
                    all.iter().find(|d| d.variant.id == design).cloned()
                } else {
                    let d = design_edit::plan_obsolete(&all, design, want_obsolete)?.clone();
                    bounded_keys.preflight()?;
                    Some(d)
                };
                let Some(found) = found else {
                    return Err(Error::new(
                        "design_unknown",
                        format!("No own design {design}"),
                    ));
                };
                let action =
                    json!({"kind":"design_obsolete","design":design,"obsolete":want_obsolete});
                let info = std::cell::RefCell::new(Value::Null);
                let mut settle = || {
                    let now = design_edit::obsolete_flag(process, design)?;
                    let detail = json!({"design":design,"obsolete":now});
                    let verdict = design_edit::settle_obsolete(
                        before.unwrap_or(!want_obsolete),
                        now,
                        want_obsolete,
                    );
                    Ok(verdict.outcome(detail))
                };
                let mut dispatch = || {
                    authority.authorize(token, &epoch)?;
                    if retry {
                        return Err(Error::new(
                            "action_uncertain",
                            "A retried request is never dispatched again",
                        ));
                    }
                    *info.borrow_mut() =
                        design_edit::obsolete_apply(process, keys, &found, want_obsolete)?;
                    Ok(())
                };
                let receipt =
                    actions.execute_with(id, &epoch, action, &mut settle, &mut dispatch, true)?;
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
                    return Err(Error::new(
                        "action_uncertain",
                        "A retried request is never dispatched again",
                    ));
                }
                *info.borrow_mut() = design_edit::set_production_apply(process, keys, &change)?;
                Ok(())
            };
            let receipt =
                actions.execute_with(id, &epoch, action, &mut settle, &mut dispatch, true)?;
            return Ok(json!({"receipt":receipt,"change":*info.borrow()}));
        }
        if name == "hoi4_design_create" {
            allowed(
                args,
                &[
                    "kind",
                    "base",
                    "upgrades",
                    "modules",
                    "name",
                    "request_id",
                    "lease_token",
                ],
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
                .find(|d| {
                    !planned.before_ids.contains(&d.variant.id)
                        && design_edit::matches_want(d, &planned.want)
                })
                .or_else(|| {
                    all.iter()
                        .find(|d| design_edit::matches_want(d, &planned.want))
                });
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
                    &[
                        "name",
                        "regiments",
                        "support",
                        "regimental_support",
                        "priority",
                        "request_id",
                        "lease_token",
                    ],
                )?,
                "edit" => allowed(
                    args,
                    &[
                        "template",
                        "regiments",
                        "support",
                        "regimental_support",
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
            refuse_unbuilt_args(name, args)?;
            let existing = if kind == "create" {
                None
            } else {
                Some(string(args, "template")?)
            };
            if existing
                .is_some_and(|t| t.is_empty() || t.len() > 128 || t.chars().any(char::is_control))
            {
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
                regimental: args
                    .get("regimental_support")
                    .map(template_edit::parse_regimental)
                    .transpose()?,
                regimental_min: args
                    .get("regimental_support")
                    .and_then(|_| mgmt::game_dir(process).ok())
                    .and_then(|d| {
                        std::fs::read_to_string(d.join("common/defines/00_defines.lua")).ok()
                    })
                    .and_then(|t| template_edit::regimental_required(&t)),
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
                    .or_else(|| {
                        before
                            .as_ref()
                            .and_then(|b| now_list.iter().find(|l| l.name == b.name))
                    })
                    .cloned();
                let detail =
                    json!({"template":want.name,"layout_saved":now.as_ref() == Some(&want)});
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
                    Error::new(
                        "action_uncertain",
                        "A retried request is never dispatched again",
                    )
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
                let power = country_status::status(process)
                    .ok()
                    .and_then(|v| v["fields"]["political_power"]["value"].as_f64());
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
            allowed(
                args,
                &["technology", "slot", "replace", "request_id", "lease_token"],
            )?;
            let replace = args
                .get("replace")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let key = string(args, "technology")?;
            if key.is_empty()
                || key.len() > 128
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return Err(Error::new("invalid_arguments", "Invalid technology key"));
            }
            let slot = match args.get("slot") {
                // no slot named: the first empty one (or the one already holding it, for a retry)
                None | Some(Value::Null) if !replace => {
                    research::default_slot(&research::assignments(process)?, key)?
                }
                given => given
                    .and_then(Value::as_u64)
                    .filter(|v| *v <= 15)
                    .ok_or_else(|| {
                        Error::new(
                            "invalid_arguments",
                            "slot must be 0..15 (it may be left out without replace: the first empty slot)",
                        )
                    })? as usize,
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
            let action = json!({"kind":"research_start","technology":key,"slot":slot});
            let before = research::assignments(process)?;
            // engineer 79: in process (family `research`): SetResearch(own tag, the template, the slot) after the
            // game's own check; the template from the game's technology database, no view opened
            let mut research_route = if actions.receipt_exists(id) {
                None
            } else {
                inproc::route("research")
            };
            if actions.receipt_exists(id) {
                // A retry only settles or replays its receipt; it never refuses on new state.
            } else {
                research::check(&before, key, slot, replace)?;
                if research_route.is_none() {
                    bounded_keys.preflight()?;
                }
            }
            let had_research_route = research_route.is_some();
            let mut research_reply = None;
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
                research_reply = module_in_process(&mut research_route, |ch| {
                    let template =
                        ui::read(process, |r, i| crate::research::template_address(r, i, key))?;
                    Ok(inproc_research::start(ch, template, slot))
                })?;
                if research_reply.is_some() {
                    return Ok(());
                }
                if had_research_route {
                    keys.preflight()?;
                }
                research::start(process, keys, key, slot)
            };
            let receipt =
                actions.execute_with(id, &epoch, action, &mut settle, &mut dispatch, true)?;
            return Ok(with_inproc(receipt, research_reply));
        }
        if name == "hoi4_production_priority" {
            allowed(
                args,
                &["line", "equipment", "to", "request_id", "lease_token"],
            )?;
            let key = string(args, "equipment")?;
            let to = args
                .get("to")
                .and_then(Value::as_str)
                .and_then(production::Move::parse)
                .ok_or_else(|| {
                    Error::new("invalid_arguments", "to must be top, up, down or bottom")
                })?;
            let index = args
                .get("line")
                .and_then(Value::as_u64)
                .filter(|v| *v < 2048)
                .ok_or_else(|| Error::new("invalid_arguments", "line must be 0..2047"))?
                as usize;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let all_lines = production::lines(process)?;
            let (handle, address) = all_lines
                .get(index)
                .filter(|l| l.key.as_deref() == Some(key))
                .map(|l| (l.handle, l.address))
                .ok_or_else(|| {
                    Error::new(
                        "invalid_target",
                        "No own line with this index and equipment",
                    )
                })?;
            // in process (family production2): the arrows' command, ChangePriority(line, place) with the absolute
            // place; the order is then read from the country's own line list (no view needs to be open)
            let mut prio_route = if actions.receipt_exists(id) {
                None
            } else {
                inproc::route("production2")
            };
            let own_order = || {
                production::lines(process)
                    .ok()
                    .map(|v| v.iter().map(|l| l.handle).collect::<Vec<u64>>())
            };
            let in_process = prio_route.is_some();
            let order = || {
                if in_process {
                    own_order().ok_or_else(|| Error::new("read_failed", "own lines"))
                } else {
                    ui::read(process, production::shown_order)
                }
            };
            let before = order().ok();
            let place =
                |o: &Option<Vec<u64>>| o.as_ref().and_then(|o| o.iter().position(|h| *h == handle));
            let retry = actions.receipt_exists(id);
            if !retry && !in_process {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"production_priority","line":index,"equipment":key,
                "handle":format!("0x{handle:016x}"),"to":args["to"]});
            let mut settle = || {
                let now = order().ok();
                let (was, at) = (place(&before), place(&now));
                let count = now.as_ref().map_or(0, Vec::len);
                let wanted = was.map(|w| to.target(w, count));
                let detail = json!({"place_before":was,"place_after":at,"lines_shown":count});
                Ok(match (at, wanted) {
                    (Some(a), Some(w)) if a == w => journal::Outcome::Reached(detail),
                    (Some(a), _)
                        if retry
                            && (to == production::Move::Top && a == 0
                                || to == production::Move::Bottom && a + 1 == count) =>
                    {
                        journal::Outcome::Reached(detail)
                    }
                    (Some(a), Some(_)) if Some(a) != was => journal::Outcome::Diverged(detail),
                    _ if retry => journal::Outcome::Diverged(detail),
                    _ => journal::Outcome::Unchanged(detail),
                })
            };
            let keys = &self.keys;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                if let (Some(ch), Some(now)) = (prio_route.as_mut(), own_order()) {
                    if let Some(at) = now.iter().position(|h| *h == handle) {
                        let want = to.target(at, now.len());
                        if want == at {
                            return Ok(());
                        }
                        match inproc::priority_post(ch, address, want, &mut || {
                            own_order().map(|o| o.iter().position(|h| *h == handle) == Some(want))
                        }) {
                            inproc::Routed::Done(_) => return Ok(()),
                            inproc::Routed::Refused(what) => {
                                return Err(inproc::refused_error(&what))
                            }
                            inproc::Routed::Gui => {}
                        }
                    }
                    keys.preflight()?;
                }
                production::prioritize(process, keys, handle, to)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(
                json!({"receipt":receipt,"backend":if in_process { "inprocess" } else { "gui" }}),
            );
        }
        if name == "hoi4_production_add" || name == "hoi4_production_set" {
            let adding = name == "hoi4_production_add";
            if adding {
                allowed(
                    args,
                    &[
                        "equipment",
                        "factories",
                        "design",
                        "organization",
                        "bulk",
                        "request_id",
                        "lease_token",
                    ],
                )?;
                refuse_unbuilt_args(name, args)?;
            } else {
                allowed(
                    args,
                    &[
                        "line",
                        "equipment",
                        "factories",
                        "bulk",
                        "request_id",
                        "lease_token",
                    ],
                )?;
            }
            let chords = args.get("bulk").and_then(Value::as_bool).unwrap_or(false);
            let key = string(args, "equipment")?;
            if key.is_empty()
                || key.len() > 128
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return Err(Error::new("invalid_arguments", "Invalid equipment key"));
            }
            // A base key (infantry_equipment) names the newest own variant type of it
            // (infantry_equipment_2) when the exact key has no own design.
            let resolved;
            let key = if adding && !key.ends_with(|c: char| c.is_ascii_digit()) {
                let all = designs::read_all(process).unwrap_or_default();
                let exact = all.iter().any(|d| d.variant.type_key == key);
                let newest = all
                    .iter()
                    .filter(|d| !d.variant.obsolete)
                    .filter_map(|d| {
                        let rest = d.variant.type_key.strip_prefix(key)?.strip_prefix('_')?;
                        rest.parse::<u32>()
                            .ok()
                            .map(|n| (n, d.variant.type_key.clone()))
                    })
                    .max();
                match (exact, newest) {
                    (false, Some((_, k))) => {
                        resolved = k;
                        resolved.as_str()
                    }
                    _ => key,
                }
            } else {
                key
            };
            let factories = args
                .get("factories")
                .and_then(Value::as_i64)
                .filter(|v| (i64::from(adding)..=150).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "factories out of range"))?
                as i32;
            let design = match args.get("design") {
                None => None,
                Some(v) if adding => Some(design_arg(process, v)?),
                Some(_) => {
                    return Err(Error::new(
                        "invalid_arguments",
                        "design applies to adding a line",
                    ))
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
                    design_edit::check_production_design(
                        &designs::read_all(process)?,
                        wanted,
                        key,
                    )?;
                }
                if let Some(index) = index {
                    if before.get(index).and_then(|l| l.key.as_deref()) != Some(key) {
                        return Err(Error::new(
                            "invalid_target",
                            "No own line with this index and equipment",
                        ));
                    }
                }
            }
            // The in-process backend (HOI4_BACKEND): setting a line's factories is one command through the game's
            // own command path; no input, no focus. Adding a line stays on the GUI (not in the families yet).
            let mut inproc_route = match index {
                Some(_) if !retry => inproc::route("production"),
                // a NEW line (engineer 71, family production2): AddProductionLine(own tag, the named design, no
                // organisation), then the factories through the production family; only with an explicit design
                // (without one the GUI picks the selection window's first design, an order the reads do not hold)
                None if !retry && design.is_some() => inproc::route("production2")
                    .filter(|ch| inproc::family_usable(&ch.hello, "production", inproc::mode())),
                _ => None,
            };
            let add_variant = match (index, design, inproc_route.is_some()) {
                (None, Some(wanted), true) => designs::read_all(process)?
                    .iter()
                    .find(|d| d.variant.id == wanted && d.variant.address != 0)
                    .map(|d| d.variant.address),
                _ => None,
            };
            if index.is_none() && add_variant.is_none() {
                inproc_route = None;
            }
            if !retry && inproc_route.is_none() {
                bounded_keys.preflight()?;
            }
            let keys = &bounded_keys;
            // an order posted in process runs on the game's next frame: a read in the same
            // millisecond still shows the old line (s7n, 24 Nov 2026: a line closed in process read
            // back unchanged 4 ms later, NotApplied, and the balance rested two game days with 27
            // factories idle). Unchanged counts only after IPC_SETTLE.
            let ipc_posted: std::cell::Cell<Option<std::time::Instant>> =
                std::cell::Cell::new(None);
            let ipc_early = || {
                ipc_posted
                    .get()
                    .is_some_and(|t| t.elapsed() < inproc::IPC_SETTLE)
            };
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
                                    && d.lines
                                        .iter()
                                        .any(|(h, _)| added.iter().any(|l| l.handle == *h))
                            }),
                        };
                        if design_ok
                            && added
                                .iter()
                                .any(|l| l.key.as_deref() == Some(key) && l.requested == factories)
                        {
                            journal::Outcome::Reached(detail)
                        } else if added.is_empty() && ipc_early() {
                            journal::Outcome::Unknown
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
                            Some(l) if l.requested == line.requested && ipc_early() => {
                                journal::Outcome::Unknown
                            }
                            Some(l) if l.requested == line.requested => {
                                journal::Outcome::Unchanged(detail)
                            }
                            Some(_) => journal::Outcome::Diverged(detail),
                            // factories 0 removes the line.
                            None if factories == 0 => journal::Outcome::Reached(detail),
                            None => journal::Outcome::Unknown,
                        }
                    }
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                if let (None, Some(variant), Some(ch)) = (index, add_variant, inproc_route.as_mut())
                {
                    let known: Vec<u64> = before.iter().map(|l| l.handle).collect();
                    let mut new_line = || {
                        production::lines(process).ok().and_then(|now| {
                            now.into_iter()
                                .find(|l| {
                                    !known.contains(&l.handle) && l.key.as_deref() == Some(key)
                                })
                                .map(|l| (l.address, l.requested))
                        })
                    };
                    match inproc::add_line(ch, variant, factories, &mut new_line) {
                        inproc::Routed::Done(_) => {
                            ipc_posted.set(Some(std::time::Instant::now()));
                            return Ok(());
                        }
                        inproc::Routed::Refused(what) => return Err(inproc::refused_error(&what)),
                        inproc::Routed::Gui => bounded_keys.preflight()?,
                    }
                }
                if let (Some(index), Some(ch)) = (index, inproc_route.as_mut()) {
                    let line = &before[index];
                    match inproc::production_op(line.requested, factories) {
                        None => return Ok(()),
                        Some((op, delta)) => {
                            let vals = if delta == 0 { vec![] } else { vec![delta] };
                            match ch.post(op, &[line.address], &vals, None, None) {
                                Ok(reply) if inproc::all_posted(&reply) => {
                                    ipc_posted.set(Some(std::time::Instant::now()));
                                    return Ok(());
                                }
                                // the game's own check refused it: that is the answer (the GUI's
                                // buttons post the same refused command)
                                Ok(reply) if inproc::game_refused(&reply) => {
                                    return Err(inproc::refused_error(&format!(
                                        "{key} line to {factories} factories"
                                    )))
                                }
                                // not run or refused by the module: the GUI path decides, in this same call
                                Ok(_) | Err(_) => bounded_keys.preflight()?,
                            }
                        }
                    }
                }
                match index {
                    None => production::add(process, keys, key, factories, design, chords),
                    Some(index) => production::set(process, keys, index, key, factories, chords),
                }
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_diplomacy_act"
            && args
                .get("action")
                .and_then(Value::as_str)
                .and_then(volunteers::Action::from_key)
                // engineer 125: an expeditionary force takes this path when it names its army; without one the
                // view's row answers as before (dialog_unsupported)
                .is_some_and(|a| a != volunteers::Action::Expedition || args.get("army").is_some())
        {
            // engineer 118/120: send_volunteers and recall_volunteers, the diplomacy view's own actions through the
            // module's diplomacy family (HOI4_BACKEND=inprocess until S25-VOLUNTEERS); one diplomacy tool (owner, 28 Nov):
            // these keys of hoi4_diplomacy_act carry `army` / `division_ids`; no mouse path (the dialog is not driven)
            allowed(
                args,
                &[
                    "action",
                    "country",
                    "army",
                    "division_ids",
                    "try_anyway",
                    "request_id",
                    "lease_token",
                ],
            )?;
            volunteers::allowed_mode(inproc::mode())?;
            let action = volunteers::Action::from_key(string(args, "action")?)
                .ok_or_else(|| Error::new("invalid_arguments", "not a volunteers action"))?;
            let country = string(args, "country")?;
            if !volunteers::valid_tag(country) {
                return Err(Error::new(
                    "invalid_arguments",
                    "country must be a country tag such as SPR",
                ));
            }
            let army = args.get("army").and_then(Value::as_str).map(str::to_string);
            let ids: Option<Vec<String>> = match args.get("division_ids") {
                None | Some(Value::Null) => None,
                Some(v) => Some(
                    v.as_array()
                        .filter(|a| (1..=volunteers::MAX_DIVISIONS).contains(&a.len()))
                        .and_then(|a| {
                            a.iter()
                                .map(|x| {
                                    x.as_str()
                                        .filter(|s| !s.is_empty() && s.len() <= 64)
                                        .map(String::from)
                                })
                                .collect::<Option<Vec<String>>>()
                        })
                        .ok_or_else(|| {
                            Error::new("invalid_arguments", "division_ids: 1..64 division ids")
                        })?,
                ),
            };
            if action.takes_divisions() == army.is_none() {
                return Err(Error::new(
                    "invalid_arguments",
                    "send_volunteers, recall_volunteers and expeditionary_force name the army whose divisions go",
                ));
            }
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let observed = guard.observe(process)?;
            let epoch = observed.epoch;
            authority.authorize(token, &epoch)?;
            let mut route = inproc::route(action.family());
            if route.is_none() {
                return Err(Error::new(
                    "in_process_unavailable",
                    format!(
                        "the in-process module's {} family is not serving: nothing was posted",
                        action.family()
                    ),
                ));
            }
            let tag_index = ui::read(process, |r, i| diplomacy_state::tag_index_of(r, i, country))?;
            let divisions = match &army {
                None => Vec::new(),
                Some(army) => {
                    if army.is_empty() || army.len() > 128 || army.chars().any(char::is_control) {
                        return Err(Error::new("invalid_arguments", "Invalid army name"));
                    }
                    let group = ui::read(process, |r, i| armies::group_address(r, i, army))?;
                    let in_group =
                        ui::read(process, |r, i| armies::group_division_ptrs(r, i, group))?;
                    match &ids {
                        Some(ids) => {
                            let ptrs = ui::read(process, |r, i| armies::division_ptrs(r, i, ids))?;
                            if let Some(k) = ptrs.iter().position(|p| !in_group.contains(p)) {
                                return Err(Error::new(
                                    "invalid_arguments",
                                    format!("division {} is not in {army}", ids[k]),
                                ));
                            }
                            ptrs
                        }
                        None => in_group,
                    }
                }
            };
            if action.takes_divisions()
                && (divisions.is_empty() || divisions.len() > volunteers::MAX_DIVISIONS)
            {
                return Err(Error::new(
                    "invalid_arguments",
                    format!(
                        "1..{} divisions per call (found {})",
                        volunteers::MAX_DIVISIONS,
                        divisions.len()
                    ),
                ));
            }
            let what = json!({"kind":"volunteers","action":action.op(),"country":country,"army":army,
                "divisions":divisions.len()});
            let mut reply = Value::Null;
            // the read-back of volunteers abroad is mapped by S25-VOLUNTEERS; until then the module's posted count is
            // the verdict
            let mut settle = || -> Result<journal::Outcome> { Ok(journal::Outcome::Unobservable) };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let ch = route
                    .as_mut()
                    .ok_or_else(|| Error::new("in_process_unavailable", "no channel"))?;
                reply = volunteers::post(ch, action, tag_index, &divisions)?;
                Ok(())
            };
            let receipt = actions.execute(id, &epoch, what, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt,"in_process":reply}));
        }
        if name == "hoi4_diplomacy" && args.get("catalog").and_then(Value::as_bool) == Some(true) {
            // engineer 122 (the owner's rule: one dynamic diplomacy tool): every diplomatic action the game has, from the
            // module's catalog of the game's own action classes (and the content's scripted actions), each with the
            // game's own "can do" toward the country where the module can build it; no view is opened, nothing is sent
            allowed(args, &["catalog", "country", "lease_token"])?;
            let country = string(args, "country")?;
            if !volunteers::valid_tag(country) {
                return Err(Error::new(
                    "invalid_arguments",
                    "country must be a country tag such as POL",
                ));
            }
            let mut ch = match inproc::route("diplomacy") {
                Some(ch) => ch,
                None => {
                    return Err(Error::new(
                        "in_process_unavailable",
                        "the catalog is the in-process module's read: the module is not serving (HOI4_BACKEND=gui or the module is not loaded); hoi4_diplomacy without catalog reads the view's rows",
                    ))
                }
            };
            let cat = inproc_diplomacy::catalog(&mut ch)?;
            let tag_index = ui::read(process, |r, i| diplomacy_state::tag_index_of(r, i, country))?;
            let mut checks = Vec::new();
            for a in cat["actions"].as_array().into_iter().flatten() {
                let op = a["op"].as_str().unwrap_or("");
                // the check builds the action whole; division and war-goal shapes need their choices, so they are not asked
                if op.is_empty() || a["on"].as_bool() != Some(true) || a["shape"] != "simple" {
                    continue;
                }
                let fam = inproc_diplomacy::family_of(op);
                if !inproc::family_usable(&ch.hello, fam, inproc::Mode::InProcess) {
                    continue;
                }
                if let Ok(reply) = inproc_diplomacy::check(&mut ch, op, tag_index) {
                    checks.push((op.to_string(), reply));
                }
            }
            let scripted = process
                .identity()
                .map(|i| i.exe.clone())
                .and_then(|exe| crate::protocol::focus_content(&exe).ok())
                .map(|(content, _)| inproc_diplomacy::scripted_keys(&content))
                .unwrap_or_default();
            let mut out = inproc_diplomacy::listing(&cat, &checks, &scripted, |k| {
                diplomacy_keys::spec_of_key(k).is_some()
            });
            out["country"] = json!(country);
            out["note"] = json!("can_do is the game's own check (the one that enables the view's Send button), asked without sending; null = not asked (a shape that needs choices, or its family is not opted in). An action the game toggles (access, docking rights, embargo) revokes when already in force: read hoi4_diplomacy_state first. Send with hoi4_diplomacy_act and the row's key.");
            return Ok(out);
        }
        if name == "hoi4_diplomacy_act"
            && matches!(
                args.get("action").and_then(Value::as_str),
                Some("justify_war_goal" | "generate_war_goal")
            )
            && args.get("war_goal_key").is_some()
        {
            // engineer 125: justify by the war goal's own key (common/wargoals: take_state, annex_everything, ...) and the
            // states by id, through the module's diplomacy2 family (the catalog's generate_war_goal row; HOI4_BACKEND=
            // inprocess until S27-DIPLOMACY-ANY passes). Without war_goal_key the view's dialog is driven as before
            // (war_goal by display name, states by name).
            allowed(
                args,
                &[
                    "action",
                    "country",
                    "war_goal_key",
                    "state_ids",
                    "request_id",
                    "lease_token",
                ],
            )?;
            let country = string(args, "country")?;
            if !volunteers::valid_tag(country) {
                return Err(Error::new(
                    "invalid_arguments",
                    "country must be a country tag such as POL",
                ));
            }
            let goal = string(args, "war_goal_key")?.to_string();
            let states: Vec<i32> = match args.get("state_ids") {
                None | Some(Value::Null) => Vec::new(),
                Some(v) => v
                    .as_array()
                    .and_then(|a| {
                        a.iter()
                            .map(|x| x.as_i64().and_then(|n| i32::try_from(n).ok()))
                            .collect::<Option<Vec<i32>>>()
                    })
                    .ok_or_else(|| Error::new("invalid_arguments", "state_ids: state ids"))?,
            };
            inproc_diplomacy::justify_request(1, &goal, &states)?; // the arguments' own checks, before any read
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let observed = guard.observe(process)?;
            let epoch = observed.epoch;
            authority.authorize(token, &epoch)?;
            let mut route = inproc::route("diplomacy2");
            if route.is_none() {
                return Err(Error::new(
                    "in_process_unavailable",
                    "justify_war_goal with war_goal_key runs through the module's diplomacy2 family (HOI4_BACKEND=inprocess with diplomacy2 opted in, until S27-DIPLOMACY-ANY passes); pass war_goal (display name) and states (names) for the view's dialog instead; nothing was sent",
                ));
            }
            let tag_index = ui::read(process, |r, i| diplomacy_state::tag_index_of(r, i, country))?;
            let what = json!({"kind":"diplomacy_act","action":"justify_war_goal","op":inproc_diplomacy::JUSTIFY_OP,
                "country":country,"war_goal_key":goal,"state_ids":states});
            let mut reply = Value::Null;
            // the justification in progress is not read yet: the module's posted count after the game's own check is the
            // verdict (S27 maps the read-back: the country's war goal list gains the target)
            let mut settle = || -> Result<journal::Outcome> { Ok(journal::Outcome::Unobservable) };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let ch = route
                    .as_mut()
                    .ok_or_else(|| Error::new("in_process_unavailable", "no channel"))?;
                match inproc_diplomacy::justify(ch, tag_index, &goal, &states) {
                    inproc::Routed::Done(body) => {
                        reply = body;
                        Ok(())
                    }
                    inproc::Routed::Refused(what) => Err(inproc::refused_error(&what)),
                    inproc::Routed::Gui => Err(Error::new(
                        "in_process_unavailable",
                        "the module did not post the justification (see its log): nothing was sent",
                    )),
                }
            };
            let receipt = actions.execute(id, &epoch, what, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt,"in_process":reply}));
        }
        if name == "hoi4_diplomacy_act"
            && args.get("action").and_then(Value::as_str).is_some_and(|a| {
                !a.is_empty()
                    && a.len() <= 64
                    && a.bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                    && diplomacy_keys::spec_of_key(a).is_none()
                    && diplomacy_keys::spec_of_row(a).is_none()
                    && volunteers::Action::from_key(a).is_none()
            })
            && args.get("country").is_some()
        {
            // engineer 122: a catalog key with no view row the tool reads (a class the game has: air base access, naval
            // blockade, autonomy, peace proposal, licence access ...; or one a later game adds): the module's diplomacy2
            // family builds it by its shape; the game's own check decides. Off under `auto` until S27-DIPLOMACY-ANY.
            allowed(args, &["action", "country", "request_id", "lease_token"])?;
            let key = string(args, "action")?.to_string();
            let country = string(args, "country")?;
            if !volunteers::valid_tag(country) {
                return Err(Error::new(
                    "invalid_arguments",
                    "country must be a country tag such as POL",
                ));
            }
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let observed = guard.observe(process)?;
            let epoch = observed.epoch;
            authority.authorize(token, &epoch)?;
            let mut route = inproc::route("diplomacy2");
            let cat = match route.as_mut() {
                Some(ch) => inproc_diplomacy::catalog(ch)?,
                None => {
                    return Err(Error::new(
                        "in_process_unavailable",
                        format!("{key}: the module's diplomacy2 family is not serving (HOI4_BACKEND=inprocess with diplomacy2 opted in, until S27-DIPLOMACY-ANY passes); this action has no view row the tool reads, so nothing was sent"),
                    ))
                }
            };
            let e = inproc_diplomacy::entry(&cat, &key).ok_or_else(|| {
                Error::new(
                    "action_not_offered",
                    format!("{key} is not a diplomatic action of this game: hoi4_diplomacy with catalog:true lists them"),
                )
            })?;
            let op = e["op"].as_str().unwrap_or("").to_string();
            if op.is_empty() || e["on"].as_bool() != Some(true) || e["shape"] != "simple" {
                // engineer 125: the two shapes the module builds with their choices name the arguments they take
                let hint = match e["shape"].as_str() {
                    Some("war_goal_type") => " (pass action justify_war_goal with war_goal_key and state_ids)",
                    Some("division_list") => " (pass action expeditionary_force with army)",
                    _ => " (the module does not build it yet)",
                };
                return Err(Error::new(
                    "dialog_unsupported",
                    format!("{key}: shape {}{hint}; nothing was sent", e["shape"]),
                ));
            }
            let tag_index = ui::read(process, |r, i| diplomacy_state::tag_index_of(r, i, country))?;
            let what = json!({"kind":"diplomacy_act","action":key,"op":op,"country":country});
            let mut reply = Value::Null;
            // no relation read is mapped for these classes yet: the module's posted count after the game's own check is
            // the verdict (S27 maps the read-back per action)
            let mut settle = || -> Result<journal::Outcome> { Ok(journal::Outcome::Unobservable) };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let ch = route
                    .as_mut()
                    .ok_or_else(|| Error::new("in_process_unavailable", "no channel"))?;
                match inproc_diplomacy::act(ch, &op, tag_index) {
                    inproc::Routed::Done(body) => {
                        reply = body;
                        Ok(())
                    }
                    inproc::Routed::Refused(what) => Err(inproc::refused_error(&what)),
                    inproc::Routed::Gui => Err(Error::new(
                        "in_process_unavailable",
                        "the module did not post the action (see its log); this action has no view row, so the mouse sent nothing",
                    )),
                }
            };
            let receipt = actions.execute(id, &epoch, what, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt,"in_process":reply}));
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
                        "accept_widening",
                        "confirm",
                        "request_id",
                        "lease_token",
                    ],
                )?;
            } else {
                allowed(
                    args,
                    &[
                        "state_id",
                        "country",
                        "countries",
                        "action",
                        "war_goal",
                        "states",
                        "lease_token",
                    ],
                )?;
            }
            let text_ok =
                |v: &str| !v.is_empty() && v.len() <= 128 && !v.chars().any(char::is_control);
            let tag_ok = |v: &str| {
                v.len() == 3
                    && v.bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
            };
            let tag_of = |v: &Value| -> Result<String> {
                v.as_str()
                    .filter(|t| tag_ok(t))
                    .map(str::to_string)
                    .ok_or_else(|| {
                        Error::new(
                            "invalid_arguments",
                            "country must be a three-letter tag such as POL",
                        )
                    })
            };
            let state_arg =
                match args.get("state_id") {
                    None => None,
                    Some(v) => Some(v.as_i64().filter(|v| (1..4096).contains(v)).ok_or_else(
                        || Error::new("invalid_arguments", "state_id must be 1..4095"),
                    )? as i32),
                };
            let country_arg = args.get("country").map(tag_of).transpose()?;
            let many: Vec<String> = match args.get("countries") {
                None => Vec::new(),
                Some(v) => v
                    .as_array()
                    .filter(|a| a.len() <= 6 && !a.is_empty())
                    .ok_or_else(|| {
                        Error::new("invalid_arguments", "countries must list 1..6 tags")
                    })?
                    .iter()
                    .map(&tag_of)
                    .collect::<Result<Vec<_>>>()?,
            };
            if state_arg.is_some() && country_arg.is_some() {
                return Err(Error::new(
                    "invalid_arguments",
                    "Give country or state_id, not both",
                ));
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
                        let a = a.as_str().filter(|v| text_ok(v)).ok_or_else(|| {
                            Error::new("invalid_arguments", "Invalid action name")
                        })?;
                        diplomacy::preview(process, &self.keys, &target, a, kind, &states)
                    }
                };
            }
            let target = target.ok_or_else(|| {
                Error::new("invalid_arguments", "country (tag) or state_id is required")
            })?;
            let action_name = string(args, "action")?;
            if !text_ok(action_name) {
                return Err(Error::new("invalid_arguments", "Invalid action name"));
            }
            let flag = |key: &str| -> Result<bool> {
                match args.get(key) {
                    None => Ok(false),
                    Some(v) => v.as_bool().ok_or_else(|| {
                        Error::new("invalid_arguments", format!("{key} must be boolean"))
                    }),
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
            // engineer 78: an action the module sends as the view does (no view opened), for a country target; the
            // relation snapshot before it is the same read-back the mouse path uses. Off under `auto` until passed live.
            let inproc_op = match &target {
                diplomacy::Target::Country(_) if !retry => diplomacy_keys::spec_of_key(action_name)
                    .or_else(|| diplomacy_keys::spec_of_row(action_name))
                    .and_then(|s| inproc_diplomacy::op_of(s.key)),
                _ => None,
            };
            let diplo_route = std::cell::RefCell::new(
                inproc_op.and_then(|op| inproc::route(inproc_diplomacy::family_of(op))),
            );
            let war_checks = |key: Option<&str>, tag: &str| -> Result<()> {
                if key == Some("declare_war") && confirm != Some("declare_war") {
                    return Err(Error::new(
                        "confirmation_required",
                        "Declaring war is one explicit step: repeat the call with confirm:\"declare_war\" (the game accepts it only with a completed war goal)",
                    ));
                }
                if key == Some("declare_war") && !flag("accept_widening")? {
                    // Ger1 (1 Nov 2026) declared on Poland without reading its guarantors: Britain
                    // and France joined and walked through an empty Rhine. Refuse before input when
                    // a country that would join borders us without a held front.
                    self.check_war_widening(process, tag)?;
                }
                Ok(())
            };
            let mut inproc_before = None;
            let plan = if retry {
                None
            } else if diplo_route.borrow().is_some() {
                let tag = diplomacy::target_tag(process, &target)?;
                war_checks(
                    diplomacy_keys::spec_of_key(action_name)
                        .or_else(|| diplomacy_keys::spec_of_row(action_name))
                        .map(|s| s.key),
                    &tag,
                )?;
                inproc_before = Some(ui::read(process, |r, i| {
                    diplomacy_state::snapshot(r, i, &tag)
                })?);
                None
            } else {
                keys.preflight()?;
                let plan = diplomacy::plan(process, keys, &target, action_name, try_anyway)?;
                war_checks(plan.spec.map(|s| s.key), &plan.tag)?;
                Some(plan)
            };
            let inproc_ai_sent = std::cell::Cell::new(false);
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
                    plan.as_ref().map(|p| &p.before).or(inproc_before.as_ref()),
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
                // engineer 78: a proposal sent in process has no dialog text; the game's own check passed and the
                // command was posted, so the target's AI answers when the game runs (the same as a sent dialog)
                let awaiting = verdict == settle::Verdict::Unchanged
                    && spec.is_some_and(|s| s.ai_decides)
                    && sent.get()
                    && ((reply.contains("will accept") && !reply.contains("not accept"))
                        || inproc_ai_sent.get());
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
                if let (Some(op), Some(ch)) = (inproc_op, diplo_route.borrow_mut().as_mut()) {
                    let idx = ui::read(process, |r, i| diplomacy_state::tag_index_of(r, i, &tag))?;
                    let pp_before = decisions::political_power(process).ok();
                    match inproc_diplomacy::act(ch, op, idx) {
                        inproc::Routed::Done(body) => {
                            sent.set(true);
                            // read-back: the relation the action makes, or political power spent, within 3 s
                            let deadline =
                                std::time::Instant::now() + std::time::Duration::from_secs(3);
                            loop {
                                let snap = ui::read(process, |r, i| {
                                    diplomacy_state::snapshot(r, i, &tag)
                                })?;
                                let reached = diplomacy_keys::verdict(
                                    spec,
                                    inproc_before.as_ref(),
                                    &snap,
                                    false,
                                ) == settle::Verdict::Reached;
                                let pp_spent = matches!((pp_before, decisions::political_power(process).ok()), (Some(a), Some(b)) if b < a);
                                if reached || pp_spent || std::time::Instant::now() >= deadline {
                                    proven.set(reached || pp_spent);
                                    break;
                                }
                                crate::phase::sleep(std::time::Duration::from_millis(80));
                            }
                            inproc_ai_sent.set(spec.is_some_and(|s| s.ai_decides));
                            *dialog.borrow_mut() = body;
                            return Ok(());
                        }
                        inproc::Routed::Refused(what) => return Err(inproc::refused_error(&what)),
                        inproc::Routed::Gui => {}
                    }
                }
                let fallback;
                let plan = match plan.as_ref() {
                    Some(p) => p,
                    // the module handed the action to the mouse (not an unexplained refusal): the view's own path
                    None if inproc_op.is_some() && !retry => {
                        keys.preflight()?;
                        fallback =
                            diplomacy::plan(process, keys, &target, action_name, try_anyway)?;
                        &fallback
                    }
                    None => return Err(Error::new("invalid_state", "No plan for this dispatch")),
                };
                let proof = diplomacy::execute(process, keys, plan, kind, &states, &sent)?;
                proven.set(proof.rows_changed || proof.pp_spent || proof.relation);
                *dialog.borrow_mut() = proof.dialog;
                Ok(())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_construction_order" {
            allowed(args, &["line", "action", "request_id", "lease_token"])?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            // engineer 82: in process (family `construction`): the view's own move / the line's removal, no panel
            let order_retry = args
                .get("request_id")
                .and_then(Value::as_str)
                .is_some_and(|id| actions.receipt_exists(id));
            let mut order_route = if order_retry {
                None
            } else {
                inproc::route("construction")
            };
            let order_queue = match order_route {
                Some(_) => ui::read(process, crate::construction::inproc_queue).ok(),
                None => None,
            };
            if order_queue.is_none() {
                order_route = None;
            }
            if order_route.is_none() {
                self.keys.preflight()?;
            }
            let line = args
                .get("line")
                .and_then(Value::as_u64)
                .filter(|v| *v < 128)
                .ok_or_else(|| Error::new("invalid_arguments", "line must be 0..127"))?;
            let order = string(args, "action")?;
            let id = string(args, "request_id")?;
            // A journaled receipt like every other action: the executor settles an order by its
            // receipt, and an order without one was taken as uncertain and sent again
            // (shakedown sk2, 10 Nov 2026).
            let action = json!({"kind":"construction_order","line":line,"action":order});
            let done: std::cell::RefCell<Option<Value>> = std::cell::RefCell::new(None);
            let refused = std::cell::Cell::new(false);
            let mut settle = || {
                Ok(match (done.borrow().clone(), refused.get()) {
                    (Some(body), _) => journal::Outcome::Reached(body),
                    (None, true) => {
                        journal::Outcome::Unchanged(json!({"note": "refused before any click"}))
                    }
                    // Only this dispatch's own result says what happened to the order.
                    (None, false) => journal::Outcome::Unobservable,
                })
            };
            let keys = &self.keys;
            let mut order_reply = None;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                if let Some(queue) = order_queue.as_ref() {
                    match construction_order_in_process(
                        process,
                        &mut order_route,
                        queue,
                        line as usize,
                        order,
                    ) {
                        Ok(Some((body, reply))) => {
                            order_reply = Some(reply);
                            *done.borrow_mut() = Some(body);
                            return Ok(());
                        }
                        Ok(None) => keys.preflight()?,
                        Err(e) => {
                            refused.set(matches!(e.code, "invalid_arguments" | "invalid_target"));
                            return Err(e);
                        }
                    }
                }
                match construction::reorder(process, keys, line as usize, order) {
                    Ok(body) => {
                        *done.borrow_mut() = Some(body);
                        Ok(())
                    }
                    Err(e) => {
                        // These are raised before the row button is clicked.
                        refused.set(matches!(
                            e.code,
                            "invalid_arguments" | "invalid_target" | "gui_target_unavailable"
                        ));
                        Err(e)
                    }
                }
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(with_inproc(receipt, order_reply));
        }
        if name == "hoi4_railway_build" {
            allowed(
                args,
                &[
                    "from_province",
                    "from_state",
                    "to_province",
                    "to_state",
                    "from_nearest_node",
                    "leg",
                    "to_nearest_node",
                    "request_id",
                    "lease_token",
                ],
            )?;
            let token = string(args, "lease_token")?;
            let id = string(args, "request_id")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let int = |k: &str| {
                args.get(k)
                    .and_then(Value::as_i64)
                    .and_then(|v| i32::try_from(v).ok())
                    .filter(|v| *v > 0)
            };
            let flag = |k: &str| args.get(k).and_then(Value::as_bool).unwrap_or(false);
            let (nearest, to_node) = (flag("from_nearest_node"), flag("to_nearest_node"));
            let leg = flag("leg");
            // The action is the request as asked (a retry of the same id matches it without new
            // reads); the resolved ends go into the receipt detail.
            let action = json!({"kind":"railway_build",
                "from_province":int("from_province"),"from_state":int("from_state"),
                "to_province":int("to_province"),"to_state":int("to_state"),
                "from_nearest_node":nearest,"to_nearest_node":to_node,"leg":leg});
            // A retry of a journaled id only settles; it never resolves or clicks again (a
            // railway order without a receipt was re-sent until its path stood at the highest
            // level and every later order "queued nothing", shakedown sk2, 10 Nov 2026).
            let route = if actions.receipt_exists(id) {
                None
            } else {
                let route = railway::resolve(
                    process,
                    (int("from_province"), int("from_state")),
                    (int("to_province"), int("to_state")),
                    nearest,
                    to_node,
                    leg,
                )?;
                self.keys.preflight()?;
                Some(route)
            };
            let before = match &route {
                Some(_) => railway::queue_work_now(process)?,
                None => i64::MAX,
            };
            let done: std::cell::RefCell<Option<Value>> = std::cell::RefCell::new(None);
            let refused = std::cell::Cell::new(false);
            let mut settle = |baseline: &Value| {
                if let Some(body) = done.borrow().clone() {
                    return Ok(journal::Outcome::Reached(body));
                }
                let base = baseline["queue_work"].as_i64().unwrap_or(i64::MAX);
                let now = railway::queue_work_now(process)?;
                let body = json!({"queue_work_before": base, "queue_work_after": now,
                    "route": route.as_ref().map(railway::Route::json)});
                Ok(if now > base {
                    journal::Outcome::Reached(body)
                } else if refused.get() {
                    journal::Outcome::Unchanged(body)
                } else {
                    journal::Outcome::Unknown
                })
            };
            let keys = &self.keys;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let Some(route) = &route else {
                    return Err(Error::new(
                        "action_uncertain",
                        "A journaled railway order is only settled, never sent again",
                    ));
                };
                let clicked = std::cell::Cell::new(false);
                match railway::dispatch(process, keys, route, before, &clicked) {
                    Ok(body) => {
                        *done.borrow_mut() = Some(body);
                        Ok(())
                    }
                    Err(e) => {
                        // Settled as not applied: a failure before the first map click, or
                        // railway_not_queued (the queue was read for 5 s after the clicks and
                        // never grew: the game refused the path). A failure between the clicks
                        // and the readback may have left a railway behind and stays uncertain.
                        refused.set(!clicked.get() || e.code == "railway_not_queued");
                        Err(e)
                    }
                }
            };
            let receipt = actions.execute_baselined(
                id,
                &epoch,
                action,
                json!({"queue_work": before}),
                &mut settle,
                &mut dispatch,
            )?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_save" {
            allowed(args, &["request_id", "lease_token"])?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            self.keys.preflight()?;
            let running_before = crate::game::clock_context(process)
                .map(|c| !c.paused)
                .unwrap_or(false);
            let mut saved = ui::save_game(process, &self.keys, &save_folder(&self.directory))?;
            // The game's menu pauses it and the save leaves it paused (live 23 Nov 2026, s1b: a
            // running game read paused after every save); a game that was running runs again.
            if running_before {
                saved["resumed"] = json!(resume_after_save(process, &self.keys));
            }
            return Ok(saved);
        }
        if name == "hoi4_trade" {
            allowed(args, &["resource", "limit", "lease_token"])?;
            let resource = match args.get("resource") {
                None => None,
                Some(v) => Some(
                    v.as_str()
                        .filter(|v| {
                            !v.is_empty() && v.len() <= 24 && !v.chars().any(char::is_control)
                        })
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
        if name == "hoi4_auto_import" {
            allowed(args, &["enabled", "budget", "request_id", "lease_token"])?;
            let enabled = match args.get("enabled") {
                None => None,
                Some(Value::Bool(v)) => Some(*v),
                Some(_) => return Err(Error::new("invalid_arguments", "enabled must be boolean")),
            };
            let budget = match args.get("budget") {
                None => None,
                Some(v) => Some(
                    v.as_i64()
                        .filter(|v| (0..=500).contains(v))
                        .ok_or_else(|| Error::new("invalid_arguments", "budget must be 0..500"))?,
                ),
            };
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            if enabled.is_none() && budget.is_none() {
                self.keys.preflight()?;
                let state = trade::auto_import_read(process, &self.keys)?;
                return Ok(json!({"auto_import":state.json(),"changed":false}));
            }
            let id = string(args, "request_id")?;
            let retry = actions.receipt_exists(id);
            if !retry {
                self.keys.preflight()?;
            }
            let keys = &self.keys;
            let action = json!({"kind":"auto_import","enabled":enabled,"budget":budget});
            let before = if retry {
                None
            } else {
                Some(trade::auto_import_read(process, keys)?)
            };
            let mut settle = || {
                let state = trade::auto_import_read(process, keys)?;
                let reached = enabled.is_none_or(|e| state.enabled == Some(e))
                    && budget.is_none_or(|b| state.budget == Some(b));
                let detail = state.json();
                Ok(if reached {
                    journal::Outcome::Reached(detail)
                } else if before.as_ref() == Some(&state) {
                    journal::Outcome::Unchanged(detail)
                } else {
                    journal::Outcome::Diverged(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                trade::auto_import_set(process, keys, enabled, budget).map(|_| ())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
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
                            Error::new(
                                "invalid_arguments",
                                "amount must be 0..100000, \"max\" or \"needed\"",
                            )
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
                let Some(name) = target_country.clone().or_else(|| Some(country.to_string()))
                else {
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
                    (Some(n), _) if Some(n) == before && !sent.get() => {
                        journal::Outcome::Unchanged(detail)
                    }
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
                    .ok_or_else(|| {
                        Error::new("invalid_arguments", "states must be up to 16 ids")
                    })?,
            };
            if ids.iter().any(|v| !(1..4096).contains(v)) {
                return Err(Error::new("invalid_arguments", "state ids must be 1..4095"));
            }
            let ids: Vec<i32> = ids.into_iter().map(|v| v as i32).collect();
            return peace::options(process, &ids);
        }
        if matches!(
            name,
            "hoi4_peace_take" | "hoi4_peace_submit" | "hoi4_peace_exit"
        ) {
            let take = name == "hoi4_peace_take";
            if take {
                allowed(
                    args,
                    &[
                        "states",
                        "puppet",
                        "release",
                        "force_government",
                        "max_cost",
                        "request_id",
                        "lease_token",
                    ],
                )?;
                refuse_unbuilt_args(name, args)?;
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
                    Some(v) => Some(
                        v.as_f64()
                            .filter(|v| *v >= 0.0 && v.is_finite())
                            .ok_or_else(|| {
                                Error::new(
                                    "invalid_arguments",
                                    "max_cost must be a non-negative number",
                                )
                            })?,
                    ),
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
                    "hoi4_peace_take" => {
                        let mut plan = peace::plan_take(process, kind, &states, max_cost)?;
                        // English state names for the conference's own state list.
                        let all = states::names(&mapdata::game_dir(&epoch.game.exe)?)?;
                        plan.names = plan
                            .states
                            .iter()
                            .map(|s| {
                                all.iter()
                                    .find(|(id, _)| id == s)
                                    .map(|(_, n)| n.clone())
                                    .unwrap_or_default()
                            })
                            .collect();
                        (Some(plan), String::new())
                    }
                    "hoi4_peace_submit" => (None, peace::plan_submit(process)?),
                    _ => {
                        peace::plan_exit(process)?;
                        (None, String::new())
                    }
                }
            };
            let score_before = plan.as_ref().and_then(|p| p.score_before);
            let action = match name {
                "hoi4_peace_take" => {
                    json!({"kind":"peace_take","demand":kind.name(),"states":states})
                }
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
                        let plan = plan.as_ref().ok_or_else(|| {
                            Error::new("invalid_state", "No plan for this dispatch")
                        })?;
                        peace::take(process, keys, plan, &sent).map(|_| ())
                    }
                    "hoi4_peace_submit" => {
                        peace::submit(process, keys, &before_key, &sent).map(|_| ())
                    }
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
            let option = match args.get("option") {
                // a key ("germany.14.a"), its letter or the shown text: looked up in the open event
                Some(Value::String(wanted)) => {
                    events::option_index(&events::list(process)?, event, wanted)?
                }
                other => other
                    .and_then(Value::as_u64)
                    .filter(|v| *v < 8)
                    .ok_or_else(|| {
                        Error::new(
                            "invalid_arguments",
                            "option must be 0..7 or the option's key from hoi4_events option_keys",
                        )
                    })? as usize,
            };
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let retry = actions.receipt_exists(id);
            // engineer 76: the answer in process (family `events`), the mouse path as the fallback
            let mut events_route = inproc::route("events");
            let had_route = events_route.is_some();
            if !retry {
                if !events::is_open(process, event)? {
                    return Err(Error::new(
                        "invalid_target",
                        format!("Event {event} is not open"),
                    ));
                }
                if !had_route {
                    self.keys.preflight()?;
                }
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
            let mut events_reply = None;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                // the window's own option click as one command: the window and its shown option count from the
                // events read, the option by its shown place; the module checks the rest with the game's code
                events_reply = module_in_process(&mut events_route, |ch| {
                    Ok(match events::window_of(process, event)? {
                        Some((window, count)) => inproc_events::answer(ch, window, option, count),
                        None => inproc::Routed::Gui,
                    })
                })?;
                if events_reply.is_some() {
                    return Ok(());
                }
                if had_route {
                    keys.preflight()?;
                }
                events::choose(process, keys, event, option)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(with_inproc(receipt, events_reply));
        }
        if name == "hoi4_fleet_mission" || name == "hoi4_fleet_region" {
            let by_mission = name == "hoi4_fleet_mission";
            if by_mission {
                allowed(
                    args,
                    &["task_force", "mission", "request_id", "lease_token"],
                )?;
            } else {
                allowed(
                    args,
                    &[
                        "fleet",
                        "region",
                        "state_id",
                        "remove",
                        "request_id",
                        "lease_token",
                    ],
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
                (
                    text_arg("task_force", 128)?,
                    text_arg("mission", 40)?,
                    0,
                    false,
                )
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
                until: (Instant::now() + Duration::from_secs(if by_mission { 60 } else { 120 }))
                    .min(lease_end),
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
                Some(navalact::plan_region(
                    &fleets, &subject, &detail, state, remove,
                )?)
            } else {
                None
            };
            // engineer 85: in process (family `navy`): the mission bar's command for the task force, or the fleet's
            // region by strategic region id, after the module's checks and the game's own "can do"; the mouse path
            // serves the call when the module refuses for its own reasons
            let mut navy_route = if retry { None } else { inproc::route("navy") };
            let navy_target: Option<(u64, i32)> = if navy_route.is_some() {
                inproc_navy::objects(process).ok().and_then(|objs| {
                    if let Some(plan) = &mission_plan {
                        let (_, _, tf) =
                            navalact::find_task_force(&fleets, &plan.task_force).ok()?;
                        Some((
                            inproc_navy::task_force_object(&fleets, &objs, tf)?,
                            plan.code,
                        ))
                    } else {
                        let plan = region_plan.as_ref()?;
                        let f = fleets.iter().find(|f| f.name == plan.fleet)?;
                        Some((
                            inproc_navy::fleet_object(&fleets, &objs, f)?,
                            inproc_navy::region_id(process, &plan.region)?,
                        ))
                    }
                })
            } else {
                None
            };
            if navy_target.is_none() {
                navy_route = None;
            }
            let had_navy_route = navy_route.is_some();
            if !retry && !had_navy_route {
                bounded_keys.preflight()?;
            }
            let navy_posted: std::cell::Cell<Option<Instant>> = std::cell::Cell::new(None);
            let navy_reply: std::cell::RefCell<Option<Value>> = std::cell::RefCell::new(None);
            let action = if by_mission {
                json!({"kind":"fleet_mission","task_force":subject,"mission":detail})
            } else {
                json!({"kind":"fleet_region","fleet":subject,"region":detail,"state_id":state,"remove":remove})
            };
            let target_code = navalact::MISSIONS
                .iter()
                .find(|m| m.0 == detail)
                .map(|m| m.2)
                .unwrap_or(-1);
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
                    let early = navy_posted
                        .get()
                        .is_some_and(|t| t.elapsed() < inproc::IPC_SETTLE);
                    if early && verdict == settle::Verdict::Unchanged {
                        return Ok(journal::Outcome::Unknown);
                    }
                    Ok(verdict.outcome(json!({"task_force":subject,"mission_code":now})))
                } else {
                    let now = navalact::region_present_now(process, &subject, &detail)?;
                    let want_present = !remove;
                    let verdict = match (retry, now) {
                        (true, Some(h)) if h == want_present => settle::Verdict::Reached,
                        (true, Some(_)) => settle::Verdict::Unchanged,
                        _ => {
                            navalact::settle_region(had_region.unwrap_or(false), now, want_present)
                        }
                    };
                    let early = navy_posted
                        .get()
                        .is_some_and(|t| t.elapsed() < inproc::IPC_SETTLE);
                    if early && verdict == settle::Verdict::Unchanged {
                        return Ok(journal::Outcome::Unknown);
                    }
                    Ok(verdict.outcome(json!({"fleet":subject,"region":detail,"present":now})))
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
                if let Some((object, value)) = navy_target {
                    let reply = module_in_process(&mut navy_route, |ch| {
                        Ok(if by_mission {
                            inproc_navy::mission(ch, &[object], value)
                        } else {
                            inproc_navy::region(ch, object, value, remove)
                        })
                    })?;
                    if let Some(r) = reply {
                        navy_posted.set(Some(Instant::now()));
                        *navy_reply.borrow_mut() = Some(r);
                        return Ok(());
                    }
                }
                if had_navy_route {
                    keys.preflight()?;
                }
                *info.borrow_mut() = match (&mission_plan, &region_plan) {
                    (Some(plan), _) => navalact::set_mission(process, keys, plan)?,
                    (_, Some(plan)) => navalact::region_op(process, keys, plan)?,
                    _ => return Err(Error::new("invalid_arguments", "No plan")),
                };
                Ok(())
            };
            let receipt =
                actions.execute_with(id, &epoch, action, &mut settle, &mut dispatch, true)?;
            let after = navalact::own_fleets(process).ok();
            return Ok(
                json!({"receipt":receipt,"result":*info.borrow(),"in_process":navy_reply.into_inner(),
                "fleets_after":after.map(|f| f.iter().map(|f| json!({"fleet":f.name.trim(),
                    "regions":f.regions.iter().map(|r| &r.1).collect::<Vec<_>>(),
                    "task_forces":f.task_forces.iter().map(|t| json!({"task_force":t.name,"mission":naval::mission_name(t.mission_code)})).collect::<Vec<_>>()})).collect::<Vec<_>>())}),
            );
        }
        if name == "hoi4_fleet_admiral" {
            allowed(
                args,
                &[
                    "fleet",
                    "admiral",
                    "unassign",
                    "list",
                    "request_id",
                    "lease_token",
                ],
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
                return Err(Error::new(
                    "invalid_arguments",
                    "list takes no admiral or unassign",
                ));
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
                let Some(found) = fleets
                    .iter()
                    .find(|f| f.name.trim().eq_ignore_ascii_case(fleet.trim()))
                else {
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
            // engineer 85: in process (family `navy`): the commander list's command (the fleet, the admiral or none)
            let mut navy_route = if retry { None } else { inproc::route("navy") };
            let navy_target: Option<(u64, Option<u64>)> = match (&plan, navy_route.is_some()) {
                (Some(p), true) => inproc_navy::objects(process).ok().and_then(|objs| {
                    let f = fleets.iter().find(|f| f.name == p.fleet)?;
                    let fo = inproc_navy::fleet_object(&fleets, &objs, f)?;
                    match &p.admiral {
                        Some(a) => Some((fo, Some(inproc_navy::admiral_object(&objs, a)?))),
                        None => Some((fo, None)),
                    }
                }),
                _ => None,
            };
            if navy_target.is_none() {
                navy_route = None;
            }
            let had_navy_route = navy_route.is_some();
            let navy_posted: std::cell::Cell<Option<Instant>> = std::cell::Cell::new(None);
            let navy_reply: std::cell::RefCell<Option<Value>> = std::cell::RefCell::new(None);
            if !retry && !had_navy_route {
                bounded_keys.preflight()?;
            }
            let before = fleets
                .iter()
                .find(|f| f.name.trim().eq_ignore_ascii_case(fleet.trim()))
                .and_then(|f| f.admiral.clone());
            let want = admiral.clone();
            let action =
                json!({"kind":"fleet_admiral","fleet":fleet,"admiral":admiral,"unassign":unassign});
            let info = std::cell::RefCell::new(Value::Null);
            let mut settle = || {
                let now = navalact::admiral_now(process, &fleet)?;
                let verdict = if retry {
                    match &now {
                        Some(n)
                            if n.as_deref().map(str::trim).map(str::to_lowercase)
                                == want.as_deref().map(str::trim).map(str::to_lowercase) =>
                        {
                            settle::Verdict::Reached
                        }
                        Some(_) => settle::Verdict::Unchanged,
                        None => settle::Verdict::Unknown,
                    }
                } else {
                    navalact::settle_admiral(&before, now.as_ref(), &want)
                };
                let early = navy_posted
                    .get()
                    .is_some_and(|t| t.elapsed() < inproc::IPC_SETTLE);
                if early && verdict == settle::Verdict::Unchanged {
                    return Ok(journal::Outcome::Unknown);
                }
                Ok(verdict.outcome(json!({"fleet":fleet,"admiral":now})))
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let Some(plan) = &plan else {
                    return Err(Error::new(
                        "action_uncertain",
                        "A retried request is never dispatched again",
                    ));
                };
                if let Some((fo, admiral)) = navy_target {
                    let reply = module_in_process(&mut navy_route, |ch| {
                        Ok(inproc_navy::leader(ch, fo, admiral))
                    })?;
                    if let Some(r) = reply {
                        navy_posted.set(Some(Instant::now()));
                        *navy_reply.borrow_mut() = Some(r);
                        return Ok(());
                    }
                }
                if had_navy_route {
                    keys.preflight()?;
                }
                *info.borrow_mut() = navalact::assign_admiral(process, keys, plan)?;
                Ok(())
            };
            let receipt =
                actions.execute_with(id, &epoch, action, &mut settle, &mut dispatch, true)?;
            return Ok(
                json!({"receipt":receipt,"result":*info.borrow(),"in_process":navy_reply.into_inner()}),
            );
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
            let plan = if retry {
                None
            } else {
                Some(navalact::plan_create(&fleets, tf)?)
            };
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
                        let alone = now.iter().any(|f| {
                            f.task_forces.len() == 1
                                && f.task_forces[0].name.trim().eq_ignore_ascii_case(tf.trim())
                        });
                        if alone && now.len() > 1 {
                            settle::Verdict::Reached
                        } else {
                            settle::Verdict::Unchanged
                        }
                    }
                };
                let _ = count_before;
                Ok(verdict.outcome(
                    json!({"fleets":now.iter().map(|f| f.name.trim()).collect::<Vec<_>>()}),
                ))
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let Some(plan) = &plan else {
                    return Err(Error::new(
                        "action_uncertain",
                        "A retried request is never dispatched again",
                    ));
                };
                *info.borrow_mut() = navalact::create_fleet(process, keys, plan)?;
                Ok(())
            };
            let receipt =
                actions.execute_with(id, &epoch, action, &mut settle, &mut dispatch, true)?;
            return Ok(json!({"receipt":receipt,"result":*info.borrow()}));
        }
        if name == "hoi4_navy_ui" {
            allowed(
                args,
                &[
                    "op",
                    "root",
                    "path",
                    "depth",
                    "index",
                    "items",
                    "x",
                    "y",
                    "code",
                    "letter",
                    "pixels",
                    "lease_token",
                ],
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
                    self.keys
                        .click(navalui::GAME, num("x"), num("y"))
                        .map(|p| json!({"clicked":format!("{p:?}")}))
                }
                "shiftclick" => {
                    self.keys.preflight()?;
                    self.keys
                        .shift_click(navalui::GAME, num("x"), num("y"))
                        .map(|p| json!({"shift_clicked":format!("{p:?}")}))
                }
                "rclick" => {
                    self.keys.preflight()?;
                    self.keys
                        .right_click(navalui::GAME, num("x"), num("y"))
                        .map(|p| json!({"rclicked":format!("{p:?}")}))
                }
                "code" => {
                    self.keys.preflight()?;
                    self.keys
                        .press_code(num("code") as u16)
                        .map(|_| json!({"pressed":num("code")}))
                }
                "control" => {
                    let code = args.get("code").and_then(Value::as_u64).unwrap_or(0) as u16;
                    self.keys.press_control(code)?;
                    Ok(json!({"pressed_control": code}))
                }
                "letter" => {
                    self.keys.preflight()?;
                    let c = string(args, "letter")?.chars().next().unwrap_or('z');
                    self.keys
                        .press(Key::letter(c)?)
                        .map(|_| json!({"pressed":c.to_string()}))
                }
                "escape" => {
                    self.keys.preflight()?;
                    self.keys
                        .press(Key::Escape)
                        .map(|_| json!({"pressed":"escape"}))
                }
                // DEBUG: one wing (`root`) toward one army (`path`) in ONE call (a new call drops
                // the wing selection): `index` 0 left click the icon above the card, 1 right
                // click the portrait, 2 right click the icon; returns the words that changed.
                "attach" => {
                    self.keys.preflight()?;
                    let wing = string(args, "root")?.to_string();
                    let army = string(args, "path")?.to_string();
                    let how = args.get("index").and_then(Value::as_u64).unwrap_or(0);
                    let words = |p: &dyn Process| -> Result<(u64, Vec<u64>)> {
                        let w = ui::read(p, |r, i| airact::own_wing(r, i, &wing))?
                            .ok_or_else(|| Error::new("wing_unknown", "No such wing"))?;
                        let b = p.read(w.address, 0xa00)?;
                        Ok((
                            w.address,
                            b.chunks(8)
                                .map(|c| {
                                    let mut x = [0u8; 8];
                                    x[..c.len()].copy_from_slice(c);
                                    u64::from_le_bytes(x)
                                })
                                .collect(),
                        ))
                    };
                    let (wa, w0) = words(process)?;
                    let (ga, a0) = armies::debug_army_words(process, &army, 0x3000)?;
                    air::select_wing(process, &self.keys, &wing)?;
                    let card = armies::shown_card(process, &self.keys, &army)?;
                    let (cx, cy) = card.center();
                    let icon = (cx, f64::from(card.y) - 50.0);
                    crate::phase::sleep(Duration::from_millis(300));
                    match how {
                        1 => {
                            self.keys.right_click(navalui::GAME, cx, cy)?;
                        }
                        2 => {
                            self.keys.right_click(navalui::GAME, icon.0, icon.1)?;
                        }
                        _ => {
                            self.keys.click(navalui::GAME, icon.0, icon.1)?;
                        }
                    }
                    crate::phase::sleep(Duration::from_millis(900));
                    let (_, w1) = words(process)?;
                    let (_, a1) = armies::debug_army_words(process, &army, 0x3000)?;
                    let diff = |a: &[u64], b: &[u64]| -> Vec<String> {
                        a.iter()
                            .zip(b)
                            .enumerate()
                            .filter(|(_, (x, y))| x != y)
                            .map(|(k, (x, y))| format!("{:#x}: {x:#x} -> {y:#x}", k * 8))
                            .take(24)
                            .collect()
                    };
                    let refs: Vec<String> = a1
                        .iter()
                        .enumerate()
                        .filter(|(_, v)| **v == wa)
                        .map(|(k, _)| format!("{:#x}", k * 8))
                        .collect();
                    Ok(
                        json!({"wing_at": format!("{wa:#x}"), "army_at": format!("{ga:#x}"),
                        "clicked": [icon.0, icon.1, cx, cy, how], "wing_diff": diff(&w0, &w1),
                        "army_diff": diff(&a0, &a1), "army_refs_wing": refs}),
                    )
                }
                "find" => {
                    self.keys.preflight()?;
                    ui::find_state(process, &self.keys, num("x") as i32)
                        .map(|_| json!({"found":num("x")}))
                }
                "scroll" => {
                    self.keys.preflight()?;
                    self.keys
                        .scroll(navalui::GAME, (num("x"), num("y")), num("pixels") as i32)
                        .map(|_| json!({"scrolled":num("pixels")}))
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
        if name == "hoi4_air_attach" {
            allowed(args, &["wings", "army", "request_id", "lease_token"])?;
            let wings: Vec<String> = args
                .get("wings")
                .and_then(Value::as_array)
                .filter(|a| (1..=24).contains(&a.len()))
                .ok_or_else(|| Error::new("invalid_arguments", "wings must list 1..24 wing names"))?
                .iter()
                .map(|v| {
                    v.as_str()
                        .filter(|t| {
                            !t.is_empty() && t.len() <= 128 && !t.chars().any(char::is_control)
                        })
                        .map(str::to_string)
                        .ok_or_else(|| Error::new("invalid_arguments", "Invalid wing name"))
                })
                .collect::<Result<_>>()?;
            let army = airact::text_arg(args, "army", 128)?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            // in process (family airwing): the army card's command, Attach(wing, army), one per wing
            if let Some(mut ch) = inproc::route("airwing") {
                if let Some(v) = airattach::attach_inprocess(process, &mut ch, &wings, &army)? {
                    return Ok(v);
                }
            }
            self.keys.preflight()?;
            return airattach::attach(process, &self.keys, &wings, &army);
        }
        if name == "hoi4_air_region_probe" {
            allowed(args, &["state_id", "region", "lease_token"])?;
            let (state, _) = airact::resolve_region(
                process,
                args.get("state_id")
                    .map(|_| airact::state_arg(args, "state_id"))
                    .transpose()?,
                args.get("region")
                    .map(|_| airact::region_arg(args))
                    .transpose()?,
            )?;
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
                Mission {
                    // one wing, or several ordered in one input sequence (air::assign_many)
                    wings: Vec<String>,
                    mission: String,
                    state: i32,
                    region: Option<i32>,
                    mask: i32,
                },
                Rebase {
                    // one wing, or several to the same base in one call (airact::rebase_many)
                    wings: Vec<String>,
                    state: i32,
                },
                Create {
                    state: i32,
                    aircraft: String,
                },
                Disband {
                    wing: String,
                },
                Hold {
                    wing: String,
                },
                Split {
                    wing: String,
                },
                Merge {
                    wing: String,
                    with: Option<String>,
                },
            }
            // The bulk path (a base click selects every wing at the base; airact::missions_by_base,
            // airact::rebase_by_base), the single path its fallback in the same call.
            let bulk = args.get("bulk").and_then(Value::as_bool).unwrap_or(false);
            let op = match name {
                "hoi4_air_mission" => {
                    allowed(
                        args,
                        &[
                            "wing",
                            "wings",
                            "mission",
                            "state_id",
                            "region",
                            "bulk",
                            "request_id",
                            "lease_token",
                        ],
                    )?;
                    let wings: Vec<String> = match args.get("wings") {
                        Some(list) => {
                            let list = list
                                .as_array()
                                .filter(|l| (1..=24).contains(&l.len()))
                                .ok_or_else(|| {
                                    Error::new(
                                        "invalid_arguments",
                                        "wings must list 1..24 wing names",
                                    )
                                })?;
                            let mut out: Vec<String> = Vec::new();
                            for v in list {
                                let w = v
                                    .as_str()
                                    .filter(|w| !w.is_empty() && w.len() <= 128)
                                    .ok_or_else(|| {
                                        Error::new(
                                            "invalid_arguments",
                                            "a wing name must be 1..128 characters",
                                        )
                                    })?;
                                if !out.iter().any(|o| o == w) {
                                    out.push(w.to_string());
                                }
                            }
                            if args.get("wing").is_some() {
                                return Err(Error::new(
                                    "invalid_arguments",
                                    "give wing or wings, not both",
                                ));
                            }
                            out
                        }
                        None => vec![airact::text_arg(args, "wing", 128)?],
                    };
                    let mission = airact::text_arg(args, "mission", 40)?;
                    let Some(&(_, frame)) = air::MISSIONS.iter().find(|(n, _)| *n == mission)
                    else {
                        return Err(Error::new(
                            "invalid_arguments",
                            format!(
                                "mission must be one of {}",
                                air::MISSIONS
                                    .iter()
                                    .map(|m| m.0)
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ),
                        ));
                    };
                    let (state, region) = airact::resolve_region(
                        process,
                        args.get("state_id")
                            .map(|_| airact::state_arg(args, "state_id"))
                            .transpose()?,
                        args.get("region")
                            .map(|_| airact::region_arg(args))
                            .transpose()?,
                    )?;
                    Air::Mission {
                        wings,
                        // training has no region in memory (air::assign)
                        region: if mission == "training" { None } else { region },
                        mission,
                        state,
                        mask: 1 << (frame - 1),
                    }
                }
                "hoi4_air_rebase" => {
                    allowed(
                        args,
                        &[
                            "wing",
                            "wings",
                            "state_id",
                            "bulk",
                            "request_id",
                            "lease_token",
                        ],
                    )?;
                    let wings = match args.get("wings") {
                        Some(list) => {
                            if args.get("wing").is_some() {
                                return Err(Error::new(
                                    "invalid_arguments",
                                    "give wing or wings, not both",
                                ));
                            }
                            let list = list
                                .as_array()
                                .filter(|l| (1..=24).contains(&l.len()))
                                .ok_or_else(|| {
                                    Error::new(
                                        "invalid_arguments",
                                        "wings must list 1..24 wing names",
                                    )
                                })?;
                            let mut out: Vec<String> = Vec::new();
                            for v in list {
                                let w = v
                                    .as_str()
                                    .filter(|w| !w.is_empty() && w.len() <= 128)
                                    .ok_or_else(|| {
                                        Error::new(
                                            "invalid_arguments",
                                            "a wing name must be 1..128 characters",
                                        )
                                    })?;
                                if !out.iter().any(|o| o == w) {
                                    out.push(w.to_string());
                                }
                            }
                            out
                        }
                        None => vec![airact::text_arg(args, "wing", 128)?],
                    };
                    Air::Rebase {
                        wings,
                        state: airact::state_arg(args, "state_id")?,
                    }
                }
                "hoi4_air_wing_create" => {
                    allowed(args, &["state_id", "aircraft", "request_id", "lease_token"])?;
                    Air::Create {
                        state: airact::state_arg(args, "state_id")?,
                        aircraft: airact::text_arg(args, "aircraft", 96)?,
                    }
                }
                "hoi4_air_wing_disband" => {
                    allowed(args, &["wing", "request_id", "lease_token"])?;
                    Air::Disband {
                        wing: airact::text_arg(args, "wing", 128)?,
                    }
                }
                "hoi4_air_wing_split" => {
                    allowed(args, &["wing", "request_id", "lease_token"])?;
                    Air::Split {
                        wing: airact::text_arg(args, "wing", 128)?,
                    }
                }
                "hoi4_air_wing_merge" => {
                    allowed(args, &["wing", "with", "request_id", "lease_token"])?;
                    Air::Merge {
                        wing: airact::text_arg(args, "wing", 128)?,
                        with: args
                            .get("with")
                            .map(|_| airact::text_arg(args, "with", 128))
                            .transpose()?,
                    }
                }
                _ => {
                    allowed(args, &["wing", "request_id", "lease_token"])?;
                    Air::Hold {
                        wing: airact::text_arg(args, "wing", 128)?,
                    }
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
            let mut rebase_plans: Vec<airact::RebasePlan> = Vec::new();
            let mut before: Option<airact::WingSnap> = None;
            let mut befores: Vec<airact::WingSnap> = Vec::new();
            if !retry {
                match &op {
                    Air::Mission { wings: names, .. } => {
                        for wing in names {
                            befores.push(airact::plan_disband(&wings, wing)?);
                        }
                        before = befores.first().cloned();
                    }
                    Air::Rebase {
                        wings: names,
                        state,
                    } => {
                        for wing in names {
                            rebase_plans.push(airact::plan_rebase(&bases, &wings, wing, *state)?);
                        }
                        rebase_plan = rebase_plans.first().cloned();
                    }
                    Air::Create { state, aircraft } => {
                        create_plan = Some(airact::plan_create(&bases, &wings, *state, aircraft)?);
                    }
                    Air::Disband { wing } => before = Some(airact::plan_disband(&wings, wing)?),
                    Air::Hold { wing } => before = Some(airact::plan_hold(&wings, wing)?),
                    Air::Split { wing } => split_plan = Some(airact::plan_split(&wings, wing)?),
                    Air::Merge { wing, with } => {
                        merge_plan = Some(airact::plan_merge(&wings, wing, with.as_deref())?)
                    }
                }
            }
            // The in-process backend (HOI4_BACKEND) for missions and rebases: the UI's own commands, no input.
            let mut air_route = match &op {
                Air::Mission { .. } | Air::Rebase { .. } if !retry => inproc::route("air"),
                // wing structure (engineer 71): the airwing family, OFF under auto until its live session passes
                Air::Hold { .. } | Air::Split { .. } | Air::Merge { .. } | Air::Disband { .. }
                    if !retry =>
                {
                    inproc::route("airwing")
                }
                _ => None,
            };
            if !retry && air_route.is_none() {
                bounded_keys.preflight()?;
            }
            let action = match &op {
                Air::Mission {
                    wings,
                    mission,
                    state,
                    region,
                    ..
                } => match wings.as_slice() {
                    [wing] => {
                        json!({"kind":"air_mission","wing":wing,"mission":mission,"state_id":state,"region":region})
                    }
                    _ => {
                        json!({"kind":"air_mission","wings":wings,"mission":mission,"state_id":state,"region":region})
                    }
                },
                Air::Rebase { wings, state } => match wings.as_slice() {
                    [wing] => json!({"kind":"air_rebase","wing":wing,"state_id":state}),
                    _ => json!({"kind":"air_rebase","wings":wings,"state_id":state}),
                },
                Air::Create { state, aircraft } => {
                    json!({"kind":"air_wing_create","state_id":state,"aircraft":aircraft})
                }
                Air::Disband { wing } => json!({"kind":"air_wing_disband","wing":wing}),
                Air::Hold { wing } => json!({"kind":"air_hold","wing":wing}),
                Air::Split { wing } => json!({"kind":"air_wing_split","wing":wing}),
                Air::Merge { wing, with } => {
                    json!({"kind":"air_wing_merge","wing":wing,"with":with})
                }
            };
            let info = std::cell::RefCell::new(Value::Null);
            let mut settle = || {
                let (bases_now, wings_now) = airact::own(process)?;
                let snap_of = |n: &str| wings_now.iter().find(|w| w.name == n).map(airact::snap);
                Ok(match &op {
                    Air::Mission { wings, mask, region, .. } if wings.len() > 1 => {
                        // several wings: all on it = Reached, some = Diverged (each named), none
                        // = Unchanged; the dispatch already waited for each wing's answer
                        let on = |n: &airact::WingSnap| n.mission_mask & mask == *mask && region.is_none_or(|r| n.region_id == r);
                        let rows: Vec<(String, Option<airact::WingSnap>)> =
                            wings.iter().map(|w| (w.clone(), snap_of(w))).collect();
                        let reached = rows.iter().filter(|(_, n)| n.as_ref().is_some_and(on)).count();
                        let detail = json!({"changed": reached,
                            "unchanged": rows.iter().filter(|(_, n)| !n.as_ref().is_some_and(on)).map(|(w, _)| w).collect::<Vec<_>>(),
                            "wings": rows.iter().map(|(w, n)| json!({"wing": w,
                            "flying": n.as_ref().is_some_and(on),
                            "mission_mask": n.as_ref().map(|n| n.mission_mask),
                            "region_id": n.as_ref().map(|n| n.region_id)})).collect::<Vec<_>>()});
                        if reached == rows.len() {
                            journal::Outcome::Reached(detail)
                        } else if reached > 0 {
                            journal::Outcome::Diverged(detail)
                        } else {
                            journal::Outcome::Unchanged(detail)
                        }
                    }
                    Air::Mission { wings, mask, region, .. } => {
                        let wing = &wings[0];
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
                    Air::Rebase { wings, state } if wings.len() > 1 => {
                        let to = bases_now.iter().find(|b| b.state == Some(*state)).map(|b| b.id).unwrap_or(-1);
                        let rows: Vec<(String, Option<airact::WingSnap>)> =
                            wings.iter().map(|w| (w.clone(), snap_of(w))).collect();
                        let moving = |n: &airact::WingSnap| n.base_id == to || n.transfer_base_id == to;
                        let reached = rows.iter().filter(|(_, n)| n.as_ref().is_some_and(moving)).count();
                        let detail = json!({"changed": reached,
                            "unchanged": rows.iter().filter(|(_, n)| !n.as_ref().is_some_and(moving)).map(|(w, _)| w).collect::<Vec<_>>(),
                            "wings": rows.iter().map(|(w, n)| json!({"wing": w,
                            "moving": n.as_ref().is_some_and(moving),
                            "base_id": n.as_ref().map(|n| n.base_id),
                            "transfer_to_base_id": n.as_ref().map(|n| n.transfer_base_id)})).collect::<Vec<_>>()});
                        if reached == rows.len() {
                            journal::Outcome::Reached(detail)
                        } else if reached > 0 {
                            journal::Outcome::Diverged(detail)
                        } else {
                            journal::Outcome::Unchanged(detail)
                        }
                    }
                    Air::Rebase { wings, state } => {
                        let wing = &wings[0];
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
                    return Err(Error::new(
                        "action_uncertain",
                        "A retried request is never dispatched again",
                    ));
                }
                if let Some(ch) = air_route.as_mut() {
                    let mut reread = || airact::own(process).ok().map(|(_, w)| w);
                    let done = match &op {
                        Air::Mission {
                            wings: names,
                            region,
                            mask,
                            ..
                        } => inproc::air_mission(
                            ch,
                            &bases,
                            &wings,
                            names,
                            *region,
                            *mask,
                            &mut reread,
                        ),
                        Air::Rebase { .. } => {
                            inproc::air_rebase(ch, &bases, &wings, &rebase_plans, &mut reread)
                        }
                        Air::Hold { wing } => inproc::air_hold(ch, &wings, wing, &mut reread),
                        Air::Disband { wing } => inproc::air_disband(ch, &wings, wing, &mut reread),
                        Air::Split { .. } => match split_plan.as_ref() {
                            Some(p) => inproc::air_split(ch, &wings, p, &mut reread),
                            None => inproc::Routed::Gui,
                        },
                        Air::Merge { .. } => match merge_plan.as_ref() {
                            Some(p) => inproc::air_merge(ch, &wings, p, &mut reread),
                            None => inproc::Routed::Gui,
                        },
                        _ => inproc::Routed::Gui,
                    };
                    match done {
                        inproc::Routed::Done(v) => {
                            *info.borrow_mut() = v;
                            return Ok(());
                        }
                        inproc::Routed::Refused(what) => return Err(inproc::refused_error(&what)),
                        inproc::Routed::Gui => {}
                    }
                    // refused or not run: the GUI path decides, in this same call
                    bounded_keys.preflight()?;
                }
                *info.borrow_mut() = match &op {
                    Air::Mission {
                        wings,
                        mission,
                        state,
                        region,
                        ..
                    } if wings.len() > 1 => {
                        let (outcomes, by_base) = if bulk {
                            airact::missions_by_base(
                                process, keys, wings, mission, *state, *region,
                            )?
                        } else {
                            (
                                airact::missions(process, keys, wings, mission, *state, *region)?,
                                Vec::new(),
                            )
                        };
                        let failed: Vec<Value> = outcomes
                            .iter()
                            .filter_map(|(w, e)| {
                                e.as_ref().map(
                                    |e| json!({"wing": w, "code": e.code, "message": e.message}),
                                )
                            })
                            .collect();
                        if failed.len() == outcomes.len() {
                            if let Some((_, Some(e))) = outcomes.into_iter().next() {
                                return Err(e);
                            }
                        }
                        json!({"wings":wings,"mission":mission,"state_id":state,"failed":failed,"by_base":by_base})
                    }
                    Air::Mission {
                        wings,
                        mission,
                        state,
                        region,
                        ..
                    } => {
                        let wing = &wings[0];
                        airact::mission(process, keys, wing, mission, *state, *region)?;
                        json!({"wing":wing,"mission":mission,"state_id":state})
                    }
                    Air::Rebase { wings, state } if wings.len() > 1 => {
                        let (outcomes, by_base) = if bulk {
                            airact::rebase_by_base(process, keys, &rebase_plans)?
                        } else {
                            (
                                airact::rebase_many(process, keys, &rebase_plans)?,
                                Vec::new(),
                            )
                        };
                        let failed: Vec<Value> = outcomes
                            .iter()
                            .filter_map(|(w, e)| {
                                e.as_ref().map(
                                    |e| json!({"wing": w, "code": e.code, "message": e.message}),
                                )
                            })
                            .collect();
                        if failed.len() == outcomes.len() {
                            if let Some((_, Some(e))) = outcomes.into_iter().next() {
                                return Err(e);
                            }
                        }
                        json!({"wings":wings,"to_state":state,"failed":failed,"by_base":by_base})
                    }
                    Air::Rebase { .. } => airact::rebase(
                        process,
                        keys,
                        rebase_plan
                            .as_ref()
                            .ok_or_else(|| Error::new("invalid_arguments", "No plan"))?,
                    )?,
                    Air::Create { .. } => airact::create_wing(
                        process,
                        keys,
                        create_plan
                            .as_ref()
                            .ok_or_else(|| Error::new("invalid_arguments", "No plan"))?,
                    )?,
                    Air::Disband { wing } => airact::disband(process, keys, wing)?,
                    Air::Hold { wing } => airact::hold(process, keys, wing)?,
                    Air::Split { .. } => airact::split(
                        process,
                        keys,
                        split_plan
                            .as_ref()
                            .ok_or_else(|| Error::new("invalid_arguments", "No plan"))?,
                    )?,
                    Air::Merge { .. } => airact::merge(
                        process,
                        keys,
                        merge_plan
                            .as_ref()
                            .ok_or_else(|| Error::new("invalid_arguments", "No plan"))?,
                    )?,
                };
                Ok(())
            };
            let receipt =
                actions.execute_with(id, &epoch, action, &mut settle, &mut dispatch, true)?;
            let (_, after) = airact::own(process)?;
            let after_rows: Vec<Value> = match &op {
                Air::Mission { wings: names, .. } | Air::Rebase { wings: names, .. } => after
                    .iter()
                    .filter(|w| names.contains(&w.name))
                    .map(|w| json!({"wing":w.name,"aircraft":w.planes,"air_base_state":w.base_state,
                        "missions":if w.region_id > 0 { airread::missions_in(w.mission_mask) } else { Vec::new() },
                        "standing_by":w.region_id <= 0,"region_id":(w.region_id > 0).then_some(w.region_id),
                        "region":w.region_name}))
                    .collect(),
                Air::Hold { wing } => after
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
            allowed(args, &["state_ids", "view", "lease_token"])?;
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
            if args.get("view").and_then(Value::as_bool) != Some(true) {
                // The memory route: no state is opened (a large country is not walked).
                let exe = &process
                    .identity()
                    .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
                    .exe;
                return state_table::slots_of(process, &mapdata::game_dir(exe)?, &ids);
            }
            self.keys.preflight()?;
            return stateslots::read_states(process, &self.keys, &ids);
        }
        if name == "hoi4_occupation_law" {
            allowed(
                args,
                &["law", "via_state", "state", "request_id", "lease_token"],
            )?;
            refuse_unbuilt_args(name, args)?;
            let law = string(args, "law")?;
            if law.is_empty()
                || law.len() > 96
                || !law.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            {
                return Err(Error::new(
                    "invalid_arguments",
                    "Invalid occupation law key",
                ));
            }
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let exe = &process
                .identity()
                .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
                .exe;
            let dir = mapdata::game_dir(exe)?;
            // The law's row in the Default Law list, from the game files and the laws the game
            // shows the player (refused before any input when it cannot be placed).
            let row = {
                let read = occupation::occupation(process, &dir, &serde_json::Map::new())?;
                let (_, rows) = occupation_laws(exe, &read)?;
                occlaw::row_of(&rows, law)
            };
            let read_now = || -> Result<(Option<String>, Option<i32>)> {
                let v = occupation::occupation(process, &dir, &serde_json::Map::new())?;
                let current = v["player_occupation_law"]["key"]
                    .as_str()
                    .map(str::to_string);
                let state = v["states"].as_array().and_then(|a| {
                    a.iter()
                        .find(|s| s["resistance_active"].as_bool() == Some(true))
                        .and_then(|s| s["state_id"].as_i64())
                        .map(|x| x as i32)
                });
                Ok((current, state))
            };
            let (current, first_state) = read_now()?;
            let via = args
                .get("via_state")
                .and_then(Value::as_i64)
                .map(|x| x as i32)
                .or(first_state);
            if !actions.receipt_exists(id) {
                if current.as_deref() == Some(law) {
                    // A terminal receipt like every other action: a reply without one is taken
                    // by the executor as uncertain and sent again.
                    let action = json!({"kind":"occupation_law","law":law,"via_state":via});
                    return Ok(json!({"receipt": {"status": "Verified", "action": action,
                        "detail": {"law": law, "wanted": law,
                            "note": "already in force: nothing was sent"}},
                        "already": true, "law": law}));
                }
                if via.is_none() {
                    return Err(Error::new(
                        "invalid_target",
                        "No occupied or resisting state to open the occupation view from; pass via_state",
                    ));
                }
                if let Err(e) = &row {
                    return Err(Error::new(e.code, e.message.clone()));
                }
                self.keys.preflight()?;
            }
            let action = json!({"kind":"occupation_law","law":law,"via_state":via});
            let keys = &self.keys;
            let before = current.clone();
            let mut settle = || {
                let (now, _) = read_now()?;
                let detail = json!({"law":now,"wanted":law});
                Ok(if now.as_deref() == Some(law) {
                    journal::Outcome::Reached(detail)
                } else if now == before {
                    journal::Outcome::Unchanged(detail)
                } else {
                    journal::Outcome::Diverged(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let result = match &row {
                    Ok(row) => occlaw::set_default(process, keys, via.unwrap_or(0), *row),
                    Err(e) => Err(Error::new(e.code, e.message.clone())),
                };
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
                while std::time::Instant::now() < deadline
                    && read_now().ok().and_then(|x| x.0).as_deref() != Some(law)
                {
                    crate::phase::sleep(std::time::Duration::from_millis(150));
                }
                occlaw::close(process, keys);
                result
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == daynight::TOOL {
            allowed(args, &["on", "lease_token"])?;
            let want_on = args.get("on").and_then(Value::as_bool).unwrap_or(false);
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            return daynight::run(&self.keys, want_on);
        }
        if name == "hoi4_map_mode" {
            allowed(args, &["mode", "lease_token"])?;
            // macOS virtual key codes of F1..F10 (the game's map mode shortcuts, mapped live).
            let (key, code) = match string(args, "mode")? {
                "political" => ("F1", 122),
                "naval" => ("F2", 120),
                "air" => ("F3", 99),
                "supply" => ("F4", 118),
                "terrain" => ("F5", 96),
                "resistance" => ("F6", 97),
                "compliance" => ("F7", 98),
                "resources" => ("F8", 100),
                "air_raids" => ("F9", 101),
                "factions" => ("F10", 109),
                _ => return Err(Error::new("invalid_arguments", "Unknown map mode")),
            };
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            self.keys.preflight()?;
            self.keys.press_code(code)?;
            return Ok(json!({"mode": string(args, "mode")?, "key": key, "read_back": false}));
        }
        if name == "hoi4_division_disband" {
            allowed(args, &["army", "divisions", "request_id", "lease_token"])?;
            let army = string(args, "army")?;
            if army.is_empty() || army.len() > 128 || army.chars().any(char::is_control) {
                return Err(Error::new("invalid_arguments", "Invalid army name"));
            }
            let ids: Vec<String> = args
                .get("divisions")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .filter(|v: &Vec<String>| !v.is_empty() && v.len() <= 24)
                .ok_or_else(|| Error::new("invalid_arguments", "divisions: 1..24 division ids"))?;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            if !actions.receipt_exists(id) {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"division_disband","army":army,"divisions":ids});
            let keys = &self.keys;
            let mut settle = || {
                let view = armies::armies(process)?;
                let present: Vec<&String> = ids
                    .iter()
                    .filter(|d| {
                        view["armies"].as_array().into_iter().flatten().any(|a| {
                            a["divisions"]
                                .as_array()
                                .is_some_and(|v| v.iter().any(|x| x.as_str() == Some(d.as_str())))
                        }) || view["unassigned"]
                            .as_array()
                            .is_some_and(|v| v.iter().any(|x| x.as_str() == Some(d.as_str())))
                    })
                    .collect();
                let detail = json!({"army":army,"divisions":ids,"still_present":present});
                Ok(if present.is_empty() {
                    journal::Outcome::Reached(detail)
                } else if present.len() == ids.len() {
                    journal::Outcome::Unchanged(detail)
                } else {
                    journal::Outcome::Diverged(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                armies::disband(process, keys, army, &ids)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_army_detach" {
            allowed(
                args,
                &["army", "divisions", "into", "request_id", "lease_token"],
            )?;
            let army = string(args, "army")?;
            let into = args.get("into").and_then(Value::as_str);
            for v in std::iter::once(army).chain(into) {
                if v.is_empty() || v.len() > 128 || v.chars().any(char::is_control) {
                    return Err(Error::new("invalid_arguments", "Invalid army name"));
                }
            }
            if into == Some(army) {
                return Err(Error::new("invalid_arguments", "into is the same army"));
            }
            let ids: Vec<String> = args
                .get("divisions")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .filter(|v: &Vec<String>| !v.is_empty() && v.len() <= 18)
                .ok_or_else(|| Error::new("invalid_arguments", "divisions: 1..18 division ids"))?;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let mut units_route = inproc::route("units");
            let had_route = units_route.is_some();
            if !actions.receipt_exists(id) && !had_route {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"army_detach","army":army,"divisions":ids,"into":into});
            let keys = &self.keys;
            let moved_to = std::cell::RefCell::new(into.map(str::to_string));
            let mut settle = || {
                let view = armies::armies(process)?;
                let army_of = |d: &str| -> Option<String> {
                    view["armies"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find_map(|a| {
                            a["divisions"]
                                .as_array()
                                .is_some_and(|v| v.iter().any(|x| x.as_str() == Some(d)))
                                .then(|| a["army"].as_str().unwrap_or("").to_string())
                        })
                };
                let now: Vec<Option<String>> = ids.iter().map(|d| army_of(d)).collect();
                let detail = json!({"army":army,"divisions":ids,"now_in":now});
                let all_out = now.iter().all(|a| a.as_deref().is_some_and(|a| a != army));
                let same = now.windows(2).all(|w| w[0] == w[1]);
                let target_ok = moved_to
                    .borrow()
                    .as_deref()
                    .is_none_or(|t| now.first().and_then(|a| a.as_deref()) == Some(t));
                Ok(if all_out && same && target_ok {
                    journal::Outcome::Reached(detail)
                } else if now.iter().all(|a| a.as_deref() == Some(army)) {
                    journal::Outcome::Unchanged(detail)
                } else {
                    journal::Outcome::Diverged(detail)
                })
            };
            let mut units_reply = None;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                // engineer 75: in process = one command (a new army of these divisions, or Assign into `into`)
                units_reply = module_in_process(&mut units_route, |ch| {
                    let divs = ui::read(process, |r, i| armies::detach_targets(r, i, army, &ids))?;
                    Ok(match into {
                        Some(t) => {
                            let g = ui::read(process, |r, i| armies::group_address(r, i, t))?;
                            inproc_units::transfer(ch, g, &divs)
                        }
                        None => inproc_units::new_army(ch, &divs),
                    })
                })?;
                if units_reply.is_some() {
                    return Ok(());
                }
                if had_route {
                    keys.preflight()?;
                }
                let name = armies::detach(process, keys, army, &ids, into)?;
                *moved_to.borrow_mut() = Some(name);
                Ok(())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(with_inproc(receipt, units_reply));
        }
        if name == "hoi4_army_rename" {
            allowed(args, &["army", "name", "request_id", "lease_token"])?;
            let army = string(args, "army")?;
            let new_name = string(args, "name")?;
            if army.is_empty() || army.len() > 128 || army.chars().any(char::is_control) {
                return Err(Error::new("invalid_arguments", "Invalid army name"));
            }
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            if !actions.receipt_exists(id) {
                let view = armies::armies(process)?;
                if let Some(refusal) = armies::rename_refusal(&view, army, new_name) {
                    return Err(refusal);
                }
            }
            let mut units_route = inproc::route("units");
            let had_route = units_route.is_some();
            if !actions.receipt_exists(id) && !had_route {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"army_rename","army":army,"name":new_name});
            let keys = &self.keys;
            let mut settle = || {
                let view = armies::armies(process)?;
                let names = army_and_group_names(&view);
                let body = json!({"army":army,"name":new_name});
                Ok(if names.iter().any(|n| n == new_name) {
                    journal::Outcome::Reached(body)
                } else if names.iter().any(|n| n == army) {
                    journal::Outcome::Unchanged(body)
                } else {
                    journal::Outcome::Diverged(body)
                })
            };
            let mut units_reply = None;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                if army == new_name {
                    return Ok(());
                }
                units_reply = module_in_process(&mut units_route, |ch| {
                    let g = ui::read(process, |r, i| armies::group_address(r, i, army))?;
                    Ok(inproc_units::rename(ch, g, new_name))
                })?;
                if units_reply.is_some() {
                    return Ok(());
                }
                if had_route {
                    keys.preflight()?;
                }
                armies::rename(process, keys, army, new_name)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(with_inproc(receipt, units_reply));
        }
        if name == "hoi4_army_motorization" {
            allowed(args, &["army", "level", "request_id", "lease_token"])?;
            let army = string(args, "army")?;
            if army.is_empty() || army.len() > 128 || army.chars().any(char::is_control) {
                return Err(Error::new("invalid_arguments", "Invalid army name"));
            }
            let level = args
                .get("level")
                .and_then(Value::as_u64)
                .filter(|v| *v <= 2)
                .ok_or_else(|| Error::new("invalid_arguments", "level must be 0, 1 or 2"))?
                as u32;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            if !actions.receipt_exists(id) {
                ui::read(process, |r, i| armies::motorization(r, i, army)).map_err(|_| {
                    Error::new(
                        "army_unknown",
                        "No own army has this name (see hoi4_armies)",
                    )
                })?;
                self.keys.preflight()?;
            }
            let action = json!({"kind":"army_motorization","army":army,"level":level});
            let keys = &self.keys;
            let mut settle = || {
                let now = ui::read(process, |r, i| armies::motorization(r, i, army))?;
                let body = json!({"army":army,"motorization":now});
                Ok(if now == level {
                    journal::Outcome::Reached(body)
                } else {
                    journal::Outcome::Diverged(body)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                armies::set_motorization(process, keys, army, level)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_army_merge" {
            allowed(args, &["source", "target", "request_id", "lease_token"])?;
            let source = string(args, "source")?;
            let target = string(args, "target")?;
            for v in [source, target] {
                if v.is_empty() || v.len() > 128 || v.chars().any(char::is_control) {
                    return Err(Error::new("invalid_arguments", "Invalid army name"));
                }
            }
            if source == target {
                return Err(Error::new(
                    "invalid_arguments",
                    "source and target are the same army",
                ));
            }
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let action = json!({"kind":"army_merge","source":source,"target":target});
            let keys = &self.keys;
            let view = armies::armies(process)?;
            let before = settle::Roster::from_view(&view);
            if !actions.receipt_exists(id) {
                let size = |name: &str| {
                    view["armies"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .find(|a| a["army"].as_str() == Some(name))
                        .map(|a| a["division_count"].as_u64().unwrap_or(0))
                };
                let (Some(a), Some(b)) = (size(source), size(target)) else {
                    return Err(Error::new(
                        "army_unknown",
                        "Both armies must be own armies (names as hoi4_armies lists them)",
                    ));
                };
                if a + b > armies::ARMY_CAP as u64 {
                    return Err(Error::new(
                        "over_cap",
                        format!("{source} ({a}) + {target} ({b}) would exceed {} divisions; split first", armies::ARMY_CAP),
                    ));
                }
            }
            let mut units_route = inproc::route("units");
            let had_route = units_route.is_some();
            if !actions.receipt_exists(id) && !had_route {
                self.keys.preflight()?;
            }
            let mut settle = || {
                let now = armies::armies(process)?;
                Ok(
                    settle::army_merge(&before, &settle::Roster::from_view(&now), source, target)
                        .outcome(now),
                )
            };
            let mut units_reply = None;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                // engineer 75: every division of the source into the target (the right click on its card), one command
                units_reply = module_in_process(&mut units_route, |ch| {
                    let (s, t) = ui::read(process, |r, i| {
                        Ok((
                            armies::group_address(r, i, source)?,
                            armies::group_address(r, i, target)?,
                        ))
                    })?;
                    let divs = ui::read(process, |r, i| armies::group_division_ptrs(r, i, s))?;
                    Ok(inproc_units::transfer(ch, t, &divs))
                })?;
                if units_reply.is_some() {
                    return Ok(());
                }
                if had_route {
                    keys.preflight()?;
                }
                armies::merge(process, keys, source, target)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(with_inproc(receipt, units_reply));
        }
        if name == "hoi4_army_gather" {
            allowed(args, &["army", "max", "request_id", "lease_token"])?;
            let target = args.get("army").and_then(Value::as_str);
            if let Some(v) = target {
                if v.is_empty() || v.len() > 128 || v.chars().any(char::is_control) {
                    return Err(Error::new("invalid_arguments", "Invalid army name"));
                }
            }
            let max = args
                .get("max")
                .and_then(Value::as_u64)
                .unwrap_or(armies::ARMY_CAP as u64) as usize;
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let action = json!({"kind":"army_gather","army":target,"max":max});
            let keys = &self.keys;
            let view = armies::armies(process)?;
            let before = settle::Roster::from_view(&view);
            if !actions.receipt_exists(id) {
                self.keys.preflight()?;
            }
            let mut settle = || {
                let now = armies::armies(process)?;
                Ok(
                    settle::army_gather(&before, &settle::Roster::from_view(&now), target)
                        .outcome(now),
                )
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                armies::gather(process, keys, target, max).map(|_| ())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
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
            let mut units_route = inproc::route("units");
            let had_route = units_route.is_some();
            if !retry && !had_route {
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
            let mut units_reply = None;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                // engineer 75: the army's first `count` divisions (the roster's order) as one new army, one command
                units_reply = module_in_process(&mut units_route, |ch| {
                    let view = armies::armies(process)?;
                    let ids = armies::first_divisions(&view, army, count).ok_or_else(|| {
                        Error::new(
                            "army_unknown",
                            "No own army with this name and that many divisions",
                        )
                    })?;
                    let divs = ui::read(process, |r, i| armies::division_ptrs(r, i, &ids))?;
                    Ok(inproc_units::new_army(ch, &divs))
                })?;
                if units_reply.is_some() {
                    return Ok(());
                }
                if had_route {
                    keys.preflight()?;
                }
                armies::split(process, keys, army, count)?;
                verified.set(true);
                Ok(())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(with_inproc(receipt, units_reply));
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
            let mut units_route = inproc::route("units");
            let had_route = units_route.is_some();
            if !retry && !had_route {
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
            let mut units_reply = None;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                // engineer 75: one ArmyGroup command over the armies (they share a theatre; the module checks it)
                units_reply = module_in_process(&mut units_route, |ch| {
                    let groups = ui::read(process, |r, i| {
                        names
                            .iter()
                            .map(|n| armies::group_address(r, i, n))
                            .collect::<Result<Vec<_>>>()
                    })?;
                    Ok(inproc_units::army_group_new(ch, &groups))
                })?;
                if units_reply.is_some() {
                    return Ok(());
                }
                if had_route {
                    keys.preflight()?;
                }
                armies::create_group(process, keys, &names)?;
                verified.set(true);
                Ok(())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(with_inproc(receipt, units_reply));
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
            // a listed commander's surname, folded name or id names him (g92a: "Kesselring")
            let resolved = match commanders::canonical_name(process, general) {
                Ok(n) => n,
                Err(e) if matches!(e.code, "unknown_commander" | "ambiguous_commander") => {
                    return Err(e)
                }
                // an unreadable roster leaves the name to the commander list, as before
                Err(_) => general.to_string(),
            };
            let general = resolved.as_str();
            let mut units_route = inproc::route("units");
            let had_route = units_route.is_some();
            if !retry {
                if !had_route {
                    self.keys.preflight()?;
                }
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
            let mut units_reply = None;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                // engineer 75: SetArmyLeader(the leader by name, the army or army group), one command
                units_reply = module_in_process(&mut units_route, |ch| {
                    let (g, l) = ui::read(process, |r, i| {
                        Ok((
                            armies::group_address(r, i, army)?,
                            units::leader_address(r, i, general)?,
                        ))
                    })?;
                    Ok(inproc_units::leader(ch, g, Some(l)))
                })?;
                if units_reply.is_some() {
                    return Ok(());
                }
                if had_route {
                    keys.preflight()?;
                }
                armies::assign_commander(process, keys, army, general)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(with_inproc(receipt, units_reply));
        }
        if name == "hoi4_army_exercise" {
            allowed(
                args,
                &["armies", "on", "stop_at_max", "request_id", "lease_token"],
            )?;
            let names: Vec<String> = args
                .get("armies")
                .and_then(Value::as_array)
                .filter(|a| (1..=24).contains(&a.len()))
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .filter(|v: &Vec<String>| {
                    (1..=24).contains(&v.len())
                        && v.iter().all(|n| {
                            !n.is_empty() && n.len() <= 128 && !n.chars().any(char::is_control)
                        })
                })
                .ok_or_else(|| {
                    Error::new("invalid_arguments", "armies must be 1..24 army names")
                })?;
            let on = args
                .get("on")
                .and_then(Value::as_bool)
                .ok_or_else(|| Error::new("invalid_arguments", "on must be a boolean"))?;
            let at_max = on
                && match args.get("stop_at_max") {
                    None => true,
                    Some(v) => v.as_bool().ok_or_else(|| {
                        Error::new("invalid_arguments", "stop_at_max must be a boolean")
                    })?,
                };
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let cards = || -> Value {
                names
                    .iter()
                    .map(|a| {
                        let s = ui::read(process, |r, i| armies::exercise::state(r, i, a));
                        (
                            a.clone(),
                            s.map(|s| s.json())
                                .unwrap_or_else(|e| json!({"unreadable":e.code})),
                        )
                    })
                    .collect::<serde_json::Map<String, Value>>()
                    .into()
            };
            // armies started without the stop at the maximum (the game ignored the Shift variant)
            let plain: std::cell::RefCell<Vec<String>> = std::cell::RefCell::new(Vec::new());
            let reached = |cards: &Value| -> usize {
                names
                    .iter()
                    .filter(|a| {
                        let c = &cards[a.as_str()];
                        let plain_ok = on
                            && c["exercising"] == json!(true)
                            && plain.borrow().iter().any(|p| p == a.as_str());
                        plain_ok
                            || c["exercising"].as_bool().is_some_and(|x| {
                                armies::exercise::reached(
                                    armies::exercise::Shown {
                                        exercising: x,
                                        stop_at_max: c["stop_at_max"].as_bool().unwrap_or(false),
                                    },
                                    on,
                                    at_max,
                                )
                            })
                    })
                    .count()
            };
            let before = cards();
            let retry = actions.receipt_exists(id);
            if !retry {
                self.keys.preflight()?;
            }
            let action =
                json!({"kind":"army_exercise","armies":names,"on":on,"stop_at_max":at_max});
            let refused: std::cell::RefCell<Vec<Value>> = std::cell::RefCell::new(Vec::new());
            let mut settle = || {
                let now = cards();
                let done = reached(&now);
                let detail = json!({"before":before,"after":now,"refused":*refused.borrow(),
                    "stop_at_max_unavailable":*plain.borrow()});
                Ok(if done == names.len() {
                    journal::Outcome::Reached(detail)
                } else if done > reached(&before) || retry {
                    journal::Outcome::Diverged(detail)
                } else {
                    journal::Outcome::Unchanged(detail)
                })
            };
            let keys = &self.keys;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let mut last = None;
                for army in &names {
                    match armies::exercise::set(process, keys, army, on, at_max) {
                        Ok(true) => plain.borrow_mut().push(army.clone()),
                        Ok(false) => {}
                        Err(e) => {
                            refused
                                .borrow_mut()
                                .push(json!({"army":army,"code":e.code,"message":e.message}));
                            last = Some(e);
                        }
                    }
                }
                match last {
                    Some(e) if refused.borrow().len() == names.len() => Err(e),
                    _ => Ok(()),
                }
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt}));
        }
        if name == "hoi4_paradrop" {
            // engineer 106: the paradrop tool's source and target clicks and the plan bar's execute, by the game's own
            // commands through the module (family orders2; off in auto until S21-PARADROP); no mouse path
            allowed(
                args,
                &[
                    "army",
                    "from_state",
                    "target_province",
                    "division_ids",
                    "execute",
                    "request_id",
                    "lease_token",
                ],
            )?;
            paradrop::allowed_mode(inproc::mode())?;
            let army = string(args, "army")?;
            if army.is_empty() || army.len() > 128 || army.chars().any(char::is_control) {
                return Err(Error::new("invalid_arguments", "Invalid army name"));
            }
            let from = args
                .get("from_state")
                .and_then(Value::as_i64)
                .filter(|v| (1..4096).contains(v))
                .ok_or_else(|| {
                    Error::new(
                        "invalid_arguments",
                        "from_state must be a state id (1..4095)",
                    )
                })? as i32;
            let target = args
                .get("target_province")
                .and_then(Value::as_i64)
                .filter(|v| (1..100_000).contains(v))
                .ok_or_else(|| {
                    Error::new("invalid_arguments", "target_province must be a province id")
                })? as i32;
            let ids: Option<Vec<String>> = match args.get("division_ids") {
                None | Some(Value::Null) => None,
                Some(v) => Some(
                    v.as_array()
                        .filter(|a| (1..=64).contains(&a.len()))
                        .and_then(|a| {
                            a.iter()
                                .map(|x| {
                                    x.as_str()
                                        .filter(|s| !s.is_empty() && s.len() <= 64)
                                        .map(String::from)
                                })
                                .collect::<Option<Vec<String>>>()
                        })
                        .ok_or_else(|| {
                            Error::new("invalid_arguments", "division_ids: 1..64 division ids")
                        })?,
                ),
            };
            let execute = args.get("execute").and_then(Value::as_bool).unwrap_or(true);
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let observed = guard.observe(process)?;
            let epoch = observed.epoch;
            authority.authorize(token, &epoch)?;
            let mut route = inproc::route("orders2");
            if route.is_none() {
                return Err(Error::new(
                    "in_process_unavailable",
                    "the in-process module's orders2 family is not serving: nothing was posted",
                ));
            }
            let (bases, _) = airact::own(process)?;
            let source = paradrop::source_province(&bases, from)?;
            let group = ui::read(process, |r, i| armies::group_address(r, i, army))?;
            let in_group = ui::read(process, |r, i| armies::group_division_ptrs(r, i, group))?;
            let divisions = match &ids {
                Some(ids) => {
                    let ptrs = ui::read(process, |r, i| armies::division_ptrs(r, i, ids))?;
                    if let Some(k) = ptrs.iter().position(|p| !in_group.contains(p)) {
                        return Err(Error::new(
                            "invalid_arguments",
                            format!("division {} is not in {army}", ids[k]),
                        ));
                    }
                    ptrs
                }
                None => in_group,
            };
            let action = json!({"kind":"paradrop","army":army,"source_province":source,
                "target_province":target,"divisions":divisions.len(),"execute":execute});
            let order: std::cell::Cell<Option<i32>> = std::cell::Cell::new(None);
            let mut reply = Value::Null;
            let mut settle = || {
                let Some(o) = order.get() else {
                    return Ok(journal::Outcome::Unknown);
                };
                let tops: Vec<(i32, i32, bool)> =
                    ui::read(process, |r, i| armies::orders::group_orders(r, i, group))?
                        .into_iter()
                        .map(|(_, n)| (n.kind, n.id, n.executing))
                        .collect();
                let detail = json!({"army":army,"order_id":o,"orders":tops.len()});
                Ok(match paradrop::settled(&tops, o, execute) {
                    Some(true) => journal::Outcome::Reached(detail),
                    Some(false) => journal::Outcome::Diverged(detail),
                    None => journal::Outcome::Unchanged(detail),
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let ch = route
                    .as_mut()
                    .ok_or_else(|| Error::new("in_process_unavailable", "no channel"))?;
                let (o, r) = paradrop::drop(ch, group, source, target, &divisions, execute)?;
                order.set(Some(o));
                reply = r;
                Ok(())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(json!({"receipt":receipt,"in_process":reply}));
        }
        if name == "hoi4_army_fallback" || name == "hoi4_army_area_defense" {
            let fallback = name == "hoi4_army_fallback";
            if fallback {
                allowed(
                    args,
                    &["army", "states", "provinces", "request_id", "lease_token"],
                )?;
            } else {
                allowed(
                    args,
                    &["army", "states", "settings", "request_id", "lease_token"],
                )?;
                refuse_unbuilt_args(name, args)?;
            }
            let army = string(args, "army")?;
            if army.is_empty() || army.len() > 128 || army.chars().any(char::is_control) {
                return Err(Error::new("invalid_arguments", "Invalid army name"));
            }
            let states: Vec<i32> = args
                .get("states")
                .and_then(Value::as_array)
                .filter(|a| (1..=8).contains(&a.len()))
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_i64)
                        .filter(|v| (1..4096).contains(v))
                        .map(|v| v as i32)
                        .collect::<Vec<_>>()
                })
                .filter(|v: &Vec<i32>| (1..=8).contains(&v.len()))
                .ok_or_else(|| {
                    Error::new(
                        "invalid_arguments",
                        "states must be 1..8 state ids (1..4095)",
                    )
                })?;
            // engineer 74: the exact fallback line (provinces in drawing order) through the module's orders2 family
            // when it is usable; `states` stays the mouse path's line (it ignores the province list)
            let line: Option<Vec<i32>> = match args.get("provinces") {
                None | Some(Value::Null) => None,
                Some(v) => Some(
                    v.as_array()
                        .filter(|a| (1..=1024).contains(&a.len()))
                        .and_then(|a| {
                            a.iter()
                                .map(|x| {
                                    x.as_i64()
                                        .filter(|p| (1..100_000).contains(p))
                                        .map(|p| p as i32)
                                })
                                .collect::<Option<Vec<i32>>>()
                        })
                        .ok_or_else(|| {
                            Error::new(
                                "invalid_arguments",
                                "provinces: 1..1024 province ids in drawing order",
                            )
                        })?,
                ),
            };
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let observed = guard.observe(process)?;
            let epoch = observed.epoch;
            authority.authorize(token, &epoch)?;
            let game_dir = mapdata::game_dir(&epoch.game.exe)?;
            let nodes = || ui::read(process, |r, i| armies::plan_nodes(r, i, army));
            let before = nodes()?;
            let retry = actions.receipt_exists(id);
            // the game takes battle-plan orders only from an army led by a general: without one the area
            // tool's clicks select states and place nothing (s6v, 24 Nov 2026: Army 1 without a general
            // input_readback_failed on [68]; with Rommel the same order Verified at once)
            if !retry {
                let view = armies::armies(process)?;
                let leaderless = view["armies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|a| a["army"].as_str() == Some(army))
                    .is_some_and(|a| a["commander"].is_null());
                if leaderless {
                    return Err(Error::new(
                        "no_commander",
                        format!(
                            "{army} has no commander: the game takes battle-plan orders only from an army led by a \
                             general (hoi4_army_commander first); nothing was clicked"
                        ),
                    ));
                }
            }
            let mut inproc_route = line.as_ref().and_then(|_| inproc::route("orders2"));
            // engineer 79: an area-defence order through the module (family `holds`): one map click per state, the
            // first makes the order, the rest go into its id
            let mut area_existing: Option<(i32, Vec<i32>)> = None;
            let mut holds_route = if fallback || retry {
                None
            } else {
                inproc::route("holds")
            };
            if holds_route.is_some() {
                // the map tool takes battle-plan orders only from an army led by a general; a state the player does
                // not control is not a click the tool takes (refused before anything is posted)
                let view = armies::armies(process)?;
                let leaderless = view["armies"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .find(|a| a["army"].as_str() == Some(army))
                    .is_some_and(|a| a["commander"].is_null());
                if leaderless {
                    return Err(Error::new(
                        "no_commander",
                        format!(
                            "{army} has no commander: the game takes battle-plan orders only from an army led by a \
                             general (hoi4_army_commander first); nothing was posted"
                        ),
                    ));
                }
                let controlled = armies::controlled_state_ids(process)?;
                if let Some(bad) = states.iter().find(|s| !controlled.contains(s)) {
                    return Err(Error::new(
                        "invalid_arguments",
                        format!("state {bad} is not controlled by the player: an area order covers own states"),
                    ));
                }
                // ONE AREA ORDER PER ARMY: a new order replaces the army's old one in the game (s81b, 24 Nov 2026:
                // Army 2's order over 55 was dropped when [64,58] made a new one). The states go into the army's
                // standing area order by its id; those already in it are not clicked (a click removes a state)
                area_existing = inproc_holds::standing_area(&view, army);
                if let Some((_, held)) = &area_existing {
                    if states.iter().all(|s| held.contains(s)) {
                        return Err(Error::new(
                            "nothing_to_change",
                            format!("{army}'s area order already holds {states:?}"),
                        ));
                    }
                }
            }
            if !retry && inproc_route.is_none() && holds_route.is_none() {
                self.keys.preflight()?;
            }
            let mut inproc_reply = Value::Null;
            let kind = if fallback {
                "army_fallback"
            } else {
                "army_area_defense"
            };
            let action = json!({"kind":kind,"army":army,"states":states});
            let keys = &self.keys;
            let mut settle = || {
                let now = nodes()?;
                let detail = json!({"army":army,"plan_orders":now});
                // an area order is proven by its states: the army's area order holds every state asked for
                // (adding to a standing order does not change the order count)
                if !fallback {
                    let view = armies::armies(process)?;
                    if let Some((_, held)) = inproc_holds::standing_area(&view, army) {
                        if states.iter().all(|s| held.contains(s)) {
                            return Ok(journal::Outcome::Reached(
                                json!({"army":army,"area_states":held}),
                            ));
                        }
                    }
                }
                Ok(if retry {
                    journal::Outcome::Diverged(detail)
                } else {
                    settle::order_growth(before, now).outcome(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                if let (Some(ch), Some(line)) = (inproc_route.as_mut(), line.as_ref()) {
                    if let Some(bad) = front_plan::foreign_province(process, &game_dir, line)? {
                        return Err(Error::new(
                            "invalid_arguments",
                            format!("province {bad} is not in a state the player controls"),
                        ));
                    }
                    let group = ui::read(process, |r, i| armies::group_address(r, i, army))?;
                    let req =
                        inproc_orders::request2("orders2.fallback", group, &[], line, &[], None)?;
                    let reply = ch.orders_post(req)?;
                    if inproc::game_refused(&reply) {
                        return Err(inproc::refused_error("fallback line"));
                    }
                    if !inproc::all_posted(&reply) {
                        return Err(Error::new(
                            "inproc_refused",
                            format!(
                                "the module did not post the fallback line: {}",
                                reply["reason"]
                                    .as_str()
                                    .or(reply["error"].as_str())
                                    .unwrap_or("?")
                            ),
                        ));
                    }
                    inproc_reply = reply;
                    return Ok(());
                }
                if let Some(ch) = holds_route.as_mut() {
                    let group = ui::read(process, |r, i| armies::group_address(r, i, army))?;
                    let routed = match &area_existing {
                        Some((oid, held)) => {
                            let add: Vec<i32> = states
                                .iter()
                                .copied()
                                .filter(|s| !held.contains(s))
                                .collect();
                            inproc_holds::area_into(ch, group, *oid, &add)
                        }
                        None => inproc_holds::area(ch, group, &states),
                    };
                    match routed {
                        inproc::Routed::Done(v) => {
                            inproc_reply = v;
                            return Ok(());
                        }
                        inproc::Routed::Refused(what) => return Err(inproc::refused_error(&what)),
                        inproc::Routed::Gui => keys.preflight()?,
                    }
                }
                if fallback {
                    armies::fallback(process, keys, &game_dir, army, &states)
                } else {
                    armies::area_defense(process, keys, &game_dir, army, &states)
                }
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(if inproc_reply.is_null() {
                json!({"receipt":receipt})
            } else {
                json!({"receipt":receipt,"in_process":inproc_reply})
            });
        }
        if name == "hoi4_army_offensive" {
            allowed(
                args,
                &[
                    "army",
                    "state_id",
                    "province_id",
                    "states",
                    "spearhead",
                    "target_provinces",
                    "parent_order",
                    "replace_order",
                    "request_id",
                    "lease_token",
                ],
            )?;
            // engineer 74: the exact target line (provinces in drawing order) through the module: a spearhead under
            // parent_order (orders2.spearhead), or a new path for an existing offensive's target line (replace_order,
            // orders.set_path). A new plain offensive has no in-process command yet: the stroke below draws it.
            let target: Option<Vec<i32>> = match args.get("target_provinces") {
                None | Some(Value::Null) => None,
                Some(v) => Some(
                    v.as_array()
                        .filter(|a| (1..=1024).contains(&a.len()))
                        .and_then(|a| {
                            a.iter()
                                .map(|x| {
                                    x.as_i64()
                                        .filter(|p| (1..100_000).contains(p))
                                        .map(|p| p as i32)
                                })
                                .collect::<Option<Vec<i32>>>()
                        })
                        .ok_or_else(|| {
                            Error::new(
                                "invalid_arguments",
                                "target_provinces: 1..1024 province ids in drawing order",
                            )
                        })?,
                ),
            };
            let order_arg = |k: &str| -> Result<Option<i32>> {
                match args.get(k) {
                    None | Some(Value::Null) => Ok(None),
                    Some(v) => v
                        .as_i64()
                        .filter(|v| (1..=100_000_000).contains(v))
                        .map(|v| Some(v as i32))
                        .ok_or_else(|| {
                            Error::new("invalid_arguments", format!("{k}: an order id"))
                        }),
                }
            };
            let parent_order = order_arg("parent_order")?;
            let replace_order = order_arg("replace_order")?;
            if (parent_order.is_some() || replace_order.is_some()) && target.is_none() {
                return Err(Error::new(
                    "invalid_arguments",
                    "parent_order and replace_order need target_provinces",
                ));
            }
            if parent_order.is_some() && replace_order.is_some() {
                return Err(Error::new(
                    "invalid_arguments",
                    "give parent_order or replace_order, not both",
                ));
            }
            // The game's spearhead order (battle-plan bar) instead of an offensive line.
            let spearhead = match args.get("spearhead") {
                None => false,
                Some(v) => v.as_bool().ok_or_else(|| {
                    Error::new("invalid_arguments", "spearhead must be true or false")
                })?,
            };
            // A deep line across several states side by side (2..12 state ids).
            let across: Option<Vec<i32>> = match args.get("states") {
                None => None,
                Some(v) => Some(
                    v.as_array()
                        .filter(|a| (2..=12).contains(&a.len()))
                        .and_then(|a| {
                            a.iter()
                                .map(|x| {
                                    x.as_i64()
                                        .filter(|v| (1..4096).contains(v))
                                        .map(|v| v as i32)
                                })
                                .collect::<Option<Vec<i32>>>()
                        })
                        .ok_or_else(|| {
                            Error::new(
                                "invalid_arguments",
                                "states must be 2..12 state ids (1..4095)",
                            )
                        })?,
                ),
            };
            if across.is_some() && args.get("province_id").is_some() {
                return Err(Error::new(
                    "invalid_arguments",
                    "states draws one line across the states; it takes no province_id",
                ));
            }
            // The objective inside the state: the line is drawn through its depth.
            let through =
                match args.get("province_id") {
                    None => None,
                    Some(v) => Some(v.as_i64().filter(|v| (1..30000).contains(v)).ok_or_else(
                        || Error::new("invalid_arguments", "province_id must be 1..29999"),
                    )? as i32),
                };
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
            let read_plan = || {
                ui::read(process, |r, i| {
                    armies::orders::of_named(r, i, army, &Default::default())
                })
                .map(Value::from)
                .unwrap_or(Value::Null)
            };
            let plan_before = read_plan();
            // the states the new arrow must reach: the line's states, or the one target state
            let want: Vec<i32> = across.clone().unwrap_or_else(|| vec![state]);
            let retry = actions.receipt_exists(id);
            // engineer 89: a plain new offensive is orders.new_front (family orders): COrderNewFrontCommand makes the
            // arrow and its target line under the parent front, exactly as the X stroke does (s89a: the same tree)
            let family = if replace_order.is_some() || !spearhead {
                "orders"
            } else {
                "orders2"
            };
            let mut inproc_route = target.as_ref().and_then(|_| inproc::route(family));
            if !retry && inproc_route.is_none() {
                self.keys.preflight()?;
            }
            let mut inproc_reply = Value::Null;
            let mut action = json!({"kind":"army_offensive","army":army,"state_id":state});
            if let Some(p) = through {
                action["province_id"] = json!(p);
            }
            if let Some(st) = &across {
                action["states"] = json!(st);
            }
            if spearhead {
                action["spearhead"] = json!(true);
            }
            let keys = &self.keys;
            let mut settle = || {
                let now = nodes()?;
                // the plan read back from the game: the new line's target states and width
                let plan = read_plan();
                let miss = offensive_miss(&plan_before, &plan, &want, spearhead);
                let mut detail = json!({"army":army,"plan_orders":now,"plan":plan});
                if let Some(m) = &miss {
                    detail["missed"] = m.clone();
                }
                // engineer 95: a new path keeps the order (no order grows): verified by the same id standing with a
                // target line that changed (o96a, 28 Nov 2026: the path took, id 100 with its 11 new provinces, and the
                // receipt said NotApplied because no order was added)
                if let Some(rid) = replace_order.filter(|_| target.is_some()) {
                    let (was, is) = (
                        replaced_target(&plan_before, rid),
                        replaced_target(&plan, rid),
                    );
                    detail["replaced"] =
                        json!({"order": rid, "before": was.clone(), "after": is.clone()});
                    return Ok(match (&was, &is) {
                        _ if retry => journal::Outcome::Diverged(detail),
                        (_, None) => journal::Outcome::Diverged(detail),
                        // the game takes the path on a later frame: looked at again within the window
                        (Some(a), Some(b)) if a == b => journal::Outcome::Unknown,
                        _ => journal::Outcome::Reached(detail),
                    });
                }
                Ok(if retry {
                    journal::Outcome::Diverged(detail)
                } else if miss.is_some() && now > before {
                    // an arrow was drawn but the game took it elsewhere (s55a, 23 Nov 2026: four
                    // strokes into state 34 read back over 18 and 980 and were each called done,
                    // so the same stroke went out again and five arrows stood on one front)
                    journal::Outcome::Diverged(detail)
                } else {
                    settle::order_growth(before, now).outcome(detail)
                })
            };
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                if let (Some(ch), Some(line)) = (inproc_route.as_mut(), target.as_ref()) {
                    inproc_reply = offensive_in_process(
                        process,
                        ch,
                        army,
                        line,
                        parent_order,
                        replace_order,
                        spearhead,
                    )?;
                    return Ok(());
                }
                match &across {
                    Some(st) => {
                        armies::offensive_across(process, keys, &game_dir, army, st, spearhead)
                    }
                    None => {
                        armies::offensive(process, keys, &game_dir, army, state, through, spearhead)
                    }
                }
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(if inproc_reply.is_null() {
                json!({"receipt":receipt})
            } else {
                json!({"receipt":receipt,"in_process":inproc_reply})
            });
        }
        if name == "hoi4_army_move" {
            allowed(
                args,
                &[
                    "army",
                    "state_id",
                    "province_id",
                    "halt",
                    "divisions",
                    "request_id",
                    "lease_token",
                ],
            )?;
            let army = string(args, "army")?;
            // Some divisions of the army (ids as hoi4_armies lists them), not all of it.
            let only: Option<Vec<String>> = match args.get("divisions") {
                None => None,
                Some(v) => Some(
                    v.as_array()
                        .filter(|a| !a.is_empty() && a.len() <= 48)
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(str::to_string))
                                .collect::<Vec<_>>()
                        })
                        .filter(|ids| !ids.is_empty())
                        .ok_or_else(|| {
                            Error::new(
                                "invalid_arguments",
                                "divisions must be 1..48 division ids (as hoi4_armies lists them)",
                            )
                        })?,
                ),
            };
            if only.is_some()
                && (args.get("province_id").is_none()
                    || args.get("halt").and_then(Value::as_bool) == Some(true))
            {
                return Err(Error::new(
                    "invalid_arguments",
                    "divisions takes a province_id (and state_id) and no halt",
                ));
            }
            // halt: the unit panel's halt control for the whole army (no target).
            if args.get("halt").and_then(Value::as_bool) == Some(true) {
                if args.get("state_id").is_some() || args.get("province_id").is_some() {
                    return Err(Error::new(
                        "invalid_arguments",
                        "halt takes no state_id or province_id",
                    ));
                }
                let id = string(args, "request_id")?;
                let token = string(args, "lease_token")?;
                let observed = guard.observe(process)?;
                let epoch = observed.epoch;
                authority.authorize(token, &epoch)?;
                let marching = || -> Result<(usize, usize)> {
                    let ids = ui::read(process, |r, i| armies::army_division_ids(r, i, army))?;
                    let headings = ui::read(process, armies::division_headings)?;
                    let at = ui::read(process, armies::division_provinces)?;
                    Ok((armies::marching(&headings, &at, &ids), ids.len()))
                };
                let (before, total) = marching()?;
                let retry = actions.receipt_exists(id);
                let action = json!({"kind":"army_halt","army":army});
                if !retry && before == 0 {
                    return Ok(json!({"receipt": {"status": "Verified", "action": action,
                        "detail": {"army": army, "divisions": total, "marching_before": 0,
                            "marching": 0,
                            "note": "no division of the army holds a march order: nothing was sent"}}}));
                }
                if !retry {
                    self.keys.preflight()?;
                }
                let keys = &self.keys;
                // The halt button was clicked while shown: the game keeps each division's last
                // heading, so the click itself is the evidence (organisation recovers after it,
                // live 15 Nov 2026).
                let clicked = std::cell::Cell::new(false);
                let mut settle = || {
                    let (now, total) = marching()?;
                    let detail = json!({"army":army,"divisions":total,"marching_before":before,"marching":now,
                        "halt_button_clicked":clicked.get()});
                    Ok(if clicked.get() || now < before || now == 0 {
                        journal::Outcome::Reached(detail)
                    } else if retry {
                        journal::Outcome::Diverged(detail)
                    } else {
                        journal::Outcome::Unchanged(detail)
                    })
                };
                let mut dispatch = || {
                    authority.authorize(token, &epoch)?;
                    clicked.set(armies::halt(process, keys, army)?);
                    Ok(())
                };
                let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
                return Ok(json!({"receipt":receipt}));
            }
            let state = args
                .get("state_id")
                .and_then(Value::as_i64)
                .filter(|v| (1..4096).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "state_id must be 1..4095"))?
                as i32;
            // A single province of the state (an enemy-held one inside an own state, a port):
            // the order goes there and the readback watches that province.
            let province =
                match args.get("province_id") {
                    None => None,
                    Some(v) => Some(v.as_i64().filter(|v| (1..30000).contains(v)).ok_or_else(
                        || Error::new("invalid_arguments", "province_id must be 1..29999"),
                    )? as i32),
                };
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let observed = guard.observe(process)?;
            let epoch = observed.epoch;
            authority.authorize(token, &epoch)?;
            let game_dir = mapdata::game_dir(&epoch.game.exe)?;
            let wanted = match province {
                Some(p) => {
                    if !mapdata::state_provinces(&game_dir, state)?.contains(&p) {
                        return Err(Error::new(
                            "invalid_arguments",
                            format!("Province {p} is not in state {state}"),
                        ));
                    }
                    vec![p]
                }
                None => mapdata::state_provinces(&game_dir, state)?,
            };
            if let (Some(ids), Some(p)) = (&only, province) {
                let members = ui::read(process, |r, i| armies::army_division_ids(r, i, army))?;
                if let Some(stray) = ids.iter().find(|d| !members.contains(d)) {
                    return Err(Error::new(
                        "invalid_target",
                        format!(
                            "Division {stray} is not in {army} (ids as hoi4_armies lists them)"
                        ),
                    ));
                }
                let count = || -> Result<(usize, usize)> {
                    let headings = ui::read(process, armies::division_headings)?;
                    let at = ui::read(process, armies::division_provinces)?;
                    let on_way = ids.iter().filter(|d| headings.get(*d) == Some(&p)).count();
                    // standing there and not marching away (live 115, 28 Nov 2026)
                    let there = ids
                        .iter()
                        .filter(|d| {
                            at.get(*d).copied().flatten() == Some(p)
                                && headings.get(*d).is_none_or(|h| *h == 0 || *h == p)
                        })
                        .count();
                    Ok((on_way, there))
                };
                let retry = actions.receipt_exists(id);
                let action = json!({"kind":"army_move","army":army,"state_id":state,
                    "province_id":p,"divisions":ids});
                // engineer 105: each division's move by command (family move), the mouse path the fallback
                let mut move_route = inproc::route("move");
                let had_route = move_route.is_some();
                if !retry {
                    let (on_way, there) = count().unwrap_or((0, 0));
                    if on_way + there >= ids.len() {
                        return Ok(json!({"receipt": {"status": "Verified", "action": action,
                            "detail": {"army": army, "divisions": ids.len(), "divisions_heading": on_way,
                                "divisions_there": there,
                                "note": "already heading or standing there: nothing was sent"}}}));
                    }
                    if !had_route {
                        self.keys.preflight()?;
                    }
                }
                let keys = &self.keys;
                let mut settle = || {
                    let (on_way, there) = count()?;
                    let detail = json!({"army":army,"state_id":state,"province_id":p,
                        "divisions":ids.len(),"divisions_heading":on_way,"divisions_there":there});
                    // Most of them on the way counts: a division that takes no order is reported
                    // in the counts and does not hold the others back.
                    Ok(if (on_way + there) * 2 >= ids.len() && on_way + there > 0 {
                        journal::Outcome::Reached(detail)
                    } else if retry {
                        journal::Outcome::Diverged(detail)
                    } else {
                        journal::Outcome::Unchanged(detail)
                    })
                };
                let mut move_reply = None;
                let mut dispatch = || {
                    authority.authorize(token, &epoch)?;
                    let at = ui::read(process, armies::division_provinces).unwrap_or_default();
                    let away = ui::read(process, armies::division_headings).unwrap_or_default();
                    let pending: Vec<String> = ids
                        .iter()
                        .filter(|d| {
                            at.get(*d).copied().flatten() != Some(p)
                                || away.get(*d).is_some_and(|h| *h != 0 && *h != p)
                        })
                        .cloned()
                        .collect();
                    let mut posted = (Vec::new(), BTreeMap::new());
                    move_reply = module_in_process(&mut move_route, |ch| {
                        let headings = ui::read(process, armies::division_headings)?;
                        let left = inproc_moves::pending(&pending, &headings, &at, p, &[p]);
                        if left.is_empty() {
                            return Ok(inproc::Routed::Gui);
                        }
                        let ptrs = ui::read(process, |r, i| armies::division_ptrs(r, i, &left))?;
                        posted = (left, headings);
                        Ok(inproc_moves::move_divisions(ch, &ptrs, p))
                    })?;
                    if move_reply.is_some() {
                        inproc_moves::wait_started(
                            || ui::read(process, armies::division_headings),
                            &posted.1,
                            &posted.0,
                        );
                        return Ok(());
                    }
                    if had_route {
                        keys.preflight()?;
                    }
                    armies::move_divisions(process, keys, army, &pending, state, p)
                };
                let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
                return Ok(with_inproc(receipt, move_reply));
            }
            let heading = || ui::read(process, |r, i| armies::heading_for(r, i, army, &wanted));
            let retry = actions.receipt_exists(id);
            let action =
                json!({"kind":"army_move","army":army,"state_id":state,"province_id":province});
            // Already on its way there or already standing there, every division: no input at all
            // (w24: the same three Somaliland provinces were ordered 17 times, each a card click,
            // a Find view and a right click). While some division does neither, the order is
            // sent: half an army marching left the other half without it (live 15 Nov 2026).
            // engineer 105: the army's divisions ordered by command (family move), the mouse path the fallback
            let mut move_route = inproc::route("move");
            let had_route = move_route.is_some();
            if !retry {
                let (on_way, total) = heading().unwrap_or((0, 0));
                let ids = ui::read(process, |r, i| armies::army_division_ids(r, i, army))
                    .unwrap_or_default();
                let at = ui::read(process, armies::division_provinces).unwrap_or_default();
                let away = ui::read(process, armies::division_headings).unwrap_or_default();
                let standing = ids
                    .iter()
                    .filter(|d| {
                        at.get(*d)
                            .copied()
                            .flatten()
                            .is_some_and(|p| wanted.contains(&p))
                            && away.get(*d).is_none_or(|h| *h == 0 || wanted.contains(h))
                    })
                    .count();
                if total > 0 && on_way + standing >= total {
                    return Ok(json!({"receipt": {"status": "Verified", "action": action,
                        "detail": {"army": army, "divisions": total, "divisions_heading": on_way,
                            "divisions_there": standing,
                            "note": "already heading or standing there: nothing was sent"}}}));
                }
                if !had_route {
                    self.keys.preflight()?;
                }
            }
            let keys = &self.keys;
            // A move by sea or a long route first heads for the embarkation port or the next
            // leg, not the target state (seen live: home divisions sailed while the receipt said
            // NotApplied). New headings on the divisions count as the move having started.
            let headings_before =
                ui::read(process, |r, i| armies::headings(r, i, army)).unwrap_or_default();
            // Where each division stood: a division that stood in a province and no longer does
            // has started (live 15 Nov 2026, x3: a division ordered from its port to Eritrea was
            // at sea before the receipt read it, so the in-port check below missed it).
            let provinces_before =
                ui::read(process, armies::division_provinces).unwrap_or_default();
            let mut settle = || {
                let (now, total) = match heading() {
                    Ok(v) => v,
                    // No such army: nothing can have moved (settles a journal a typo left open).
                    Err(e) if e.code == "invalid_target" => {
                        return Ok(journal::Outcome::TargetAbsent(
                            json!({"army":army,"state_id":state,"error":e.message}),
                        ))
                    }
                    Err(e) => return Err(e),
                };
                let after = ui::read(process, |r, i| armies::headings(r, i, army))?;
                let left_province = || {
                    let at = ui::read(process, armies::division_provinces).unwrap_or_default();
                    let ids = ui::read(process, |r, i| armies::army_division_ids(r, i, army))
                        .unwrap_or_default();
                    ids.iter().any(|id| {
                        provinces_before
                            .get(id)
                            .copied()
                            .flatten()
                            .is_some_and(|p| at.get(id).copied().flatten() != Some(p))
                    })
                };
                let started = (after.len() == headings_before.len()
                    && after
                        .iter()
                        .zip(&headings_before)
                        .filter(|(a, b)| **a != 0 && a != b)
                        .count()
                        > 0)
                    || (now == 0 && left_province());
                // A sea transfer leaves a division standing in its port with its heading
                // unchanged until it lands (live 6 Nov 2026: Sicily to Tripoli sailed while every
                // read said nothing moved). With every division in an own port and the target
                // in another state, the order is reported as an unverified sea transfer.
                let in_ports = !started && now == 0 && {
                    let ports = ui::read(process, naval::naval_base_provinces).unwrap_or_default();
                    let at = ui::read(process, armies::division_provinces).unwrap_or_default();
                    let ids: Vec<String> =
                        ui::read(process, |r, i| armies::army_division_ids(r, i, army))
                            .unwrap_or_default();
                    !ids.is_empty()
                        && ids.iter().all(|id| {
                            at.get(id)
                                .copied()
                                .flatten()
                                .is_some_and(|p| ports.contains(&p) && !wanted.contains(&p))
                        })
                };
                let detail = json!({"army":army,"state_id":state,"divisions_heading":now,"divisions":total,
                    "under_way_by_other_route":now == 0 && started,
                    "sea_transfer_unverified":in_ports,
                    "note":if in_ports {"every division waits in an own port: a sea transfer shows no heading until it lands; check hoi4_armies after the crossing days instead of repeating the order"} else {""}});
                Ok(if now > 0 || started || in_ports {
                    journal::Outcome::Reached(detail)
                } else if retry {
                    journal::Outcome::Diverged(detail)
                } else {
                    journal::Outcome::Unchanged(detail)
                })
            };
            let mut move_reply = None;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let mut posted = (Vec::new(), BTreeMap::new());
                move_reply = module_in_process(&mut move_route, |ch| {
                    let to = match province {
                        Some(p) => Some(p),
                        None => {
                            let ports =
                                ui::read(process, naval::naval_base_provinces).unwrap_or_default();
                            let victory = mapdata::state_victory_points(&game_dir)?;
                            inproc_moves::state_target(
                                &wanted,
                                &ports,
                                victory.get(&state).map(Vec::as_slice).unwrap_or(&[]),
                            )
                        }
                    };
                    let Some(to) = to else {
                        return Ok(inproc::Routed::Gui);
                    };
                    let ids = ui::read(process, |r, i| armies::army_division_ids(r, i, army))?;
                    let headings = ui::read(process, armies::division_headings)?;
                    let at = ui::read(process, armies::division_provinces)?;
                    let left = inproc_moves::pending(&ids, &headings, &at, to, &wanted);
                    if left.is_empty() {
                        return Ok(inproc::Routed::Gui);
                    }
                    let ptrs = ui::read(process, |r, i| armies::division_ptrs(r, i, &left))?;
                    posted = (left, headings);
                    Ok(inproc_moves::move_divisions(ch, &ptrs, to))
                })?;
                if move_reply.is_some() {
                    inproc_moves::wait_started(
                        || ui::read(process, armies::division_headings),
                        &posted.1,
                        &posted.0,
                    );
                    return Ok(());
                }
                if had_route {
                    keys.preflight()?;
                }
                armies::move_to(process, keys, army, state, province)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(with_inproc(receipt, move_reply));
        }
        if name == "hoi4_army_plan" {
            allowed(
                args,
                &[
                    "army",
                    "execute",
                    "mode",
                    "clear_orders",
                    "delete_order",
                    "assign",
                    "distribute",
                    "request_id",
                    "lease_token",
                ],
            )?;
            let army = string(args, "army")?;
            if army.is_empty() || army.len() > 128 || army.chars().any(char::is_control) {
                return Err(Error::new("invalid_arguments", "Invalid army name"));
            }
            // engineer 95: divisions onto one order (assign) or the group's divisions over its fronts (distribute),
            // through the module (family orders2, the plan bar's assign); each goes alone. No mouse path: a refusal
            // says the family is not usable.
            let assign = args.get("assign").filter(|v| !v.is_null());
            let distribute = args
                .get("distribute")
                .filter(|v| !v.is_null() && **v != Value::Bool(false));
            if assign.is_some() || distribute.is_some() {
                if (assign.is_some() && distribute.is_some())
                    || ["execute", "mode", "clear_orders", "delete_order"]
                        .iter()
                        .any(|k| args.get(*k).is_some())
                {
                    return Err(Error::new(
                        "invalid_arguments",
                        "assign and distribute go alone (no execute, mode, clear_orders or delete_order)",
                    ));
                }
                let parsed = plan_assign_args(assign, distribute)?;
                let id = string(args, "request_id")?;
                let token = string(args, "lease_token")?;
                let epoch = guard.observe(process)?.epoch;
                authority.authorize(token, &epoch)?;
                let dir = mapdata::game_dir(&epoch.game.exe)?;
                let mut route = inproc::route("orders2").ok_or_else(|| {
                    Error::new(
                        "inproc_refused",
                        format!(
                            "assigning divisions to an order goes by command (in-process family orders2), which is not usable here{}",
                            inproc::os_unavailable().map(|r| format!(": {r}")).unwrap_or_default()
                        ),
                    )
                })?;
                let retry = actions.receipt_exists(id);
                let action =
                    json!({"kind":"army_plan","army":army,"assign":assign,"distribute":distribute});
                let expected: std::cell::RefCell<BTreeMap<i32, usize>> =
                    std::cell::RefCell::new(BTreeMap::new());
                let report: std::cell::RefCell<Value> = std::cell::RefCell::new(Value::Null);
                let mut settle = || {
                    let now = ui::read(process, |r, i| armies::order_divisions(r, i, army))?;
                    let want = expected.borrow();
                    let short: Vec<Value> = want
                        .iter()
                        .filter(|(o, n)| now.get(*o).copied().unwrap_or(0) < **n)
                        .map(|(o, n)| json!({"order":o,"wanted":n,"read":now.get(o)}))
                        .collect();
                    let detail = json!({"army":army,
                        "divisions_per_order":serde_json::to_value(&now).unwrap_or_default(),
                        "wanted":serde_json::to_value(&*want).unwrap_or_default(),
                        "short":short,"in_process":report.borrow().clone()});
                    Ok(if retry {
                        journal::Outcome::Diverged(detail)
                    } else if !want.is_empty() && short.is_empty() {
                        journal::Outcome::Reached(detail)
                    } else {
                        journal::Outcome::Unchanged(detail)
                    })
                };
                let mut dispatch = || {
                    authority.authorize(token, &epoch)?;
                    let (rep, want) = match &parsed {
                        PlanAssign::Order {
                            order,
                            divisions,
                            count,
                        } => assign_order_in_process(
                            process,
                            &mut route,
                            army,
                            &dir,
                            *order,
                            divisions.as_deref(),
                            *count,
                        )?,
                        PlanAssign::Distribute { weights } => {
                            distribute_in_process(process, &mut route, army, &dir, weights)?
                        }
                    };
                    *report.borrow_mut() = rep;
                    *expected.borrow_mut() = want;
                    Ok(())
                };
                let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
                let rep = report.borrow().clone();
                return Ok(json!({"receipt":receipt,"in_process":rep}));
            }
            // ONE order deleted (the bar's delete tool, then a click on the order's arrow): the
            // front and the other orders stay. Alone, it is the whole call.
            if let Some(order) = args.get("delete_order").filter(|v| !v.is_null()) {
                let order = order
                    .as_i64()
                    .and_then(|o| i32::try_from(o).ok())
                    .filter(|o| *o > 0)
                    .ok_or_else(|| {
                        Error::new(
                            "invalid_arguments",
                            "delete_order must be an order id from the army's plan read-back",
                        )
                    })?;
                if args.get("execute").is_some()
                    || args.get("mode").is_some()
                    || args.get("clear_orders").is_some()
                {
                    return Err(Error::new(
                        "invalid_arguments",
                        "delete_order goes alone (no execute, mode or clear_orders)",
                    ));
                }
                let id = string(args, "request_id")?;
                let token = string(args, "lease_token")?;
                let epoch = guard.observe(process)?.epoch;
                authority.authorize(token, &epoch)?;
                let game_dir = mapdata::game_dir(&epoch.game.exe)?;
                let keys = &self.keys;
                let retry = actions.receipt_exists(id);
                if !retry {
                    self.keys.preflight()?;
                }
                let action = json!({"kind":"army_plan","army":army,"delete_order":order});
                let sent: std::cell::RefCell<Option<Value>> = std::cell::RefCell::new(None);
                let mut settle = || {
                    let ids = ui::read(process, |r, i| armies::order_ids(r, i, army))?;
                    let mut detail = json!({"army":army,"order":order,"orders_left":ids.len()});
                    if let Some(d) = sent.borrow().as_ref() {
                        detail["delete"] = d.clone();
                    }
                    Ok(if !ids.contains(&order) {
                        journal::Outcome::Reached(detail)
                    } else if retry {
                        journal::Outcome::Diverged(detail)
                    } else {
                        journal::Outcome::Unchanged(detail)
                    })
                };
                let mut dispatch = || {
                    authority.authorize(token, &epoch)?;
                    let d = armies::delete_order(process, keys, &game_dir, army, order)?;
                    *sent.borrow_mut() = Some(d);
                    Ok(())
                };
                let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
                return Ok(json!({"receipt":receipt}));
            }
            // Every order of the army deleted (the bar's delete button, right click): alone,
            // it is the whole call.
            if args.get("clear_orders") == Some(&Value::Bool(true)) {
                if args.get("execute").is_some() || args.get("mode").is_some() {
                    return Err(Error::new(
                        "invalid_arguments",
                        "clear_orders goes alone (no execute, no mode)",
                    ));
                }
                let id = string(args, "request_id")?;
                let token = string(args, "lease_token")?;
                let epoch = guard.observe(process)?.epoch;
                authority.authorize(token, &epoch)?;
                let keys = &self.keys;
                let retry = actions.receipt_exists(id);
                if !retry {
                    self.keys.preflight()?;
                }
                let action = json!({"kind":"army_plan","army":army,"clear_orders":true});
                let mut settle = || {
                    let left = ui::read(process, |r, i| armies::plan_nodes(r, i, army))?;
                    let left_kinds = ui::read(process, |r, i| armies::orders_left(r, i, army))
                        .unwrap_or_default();
                    let detail = json!({"army":army,"orders_left":left,"left":left_kinds});
                    Ok(if left == 0 {
                        journal::Outcome::Reached(detail)
                    } else if retry {
                        journal::Outcome::Diverged(detail)
                    } else {
                        journal::Outcome::Unchanged(detail)
                    })
                };
                let mut dispatch = || {
                    authority.authorize(token, &epoch)?;
                    armies::mode::clear_orders(process, keys, army)
                };
                let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
                return Ok(json!({"receipt":receipt}));
            }
            // The execution mode of the army's battle plans (careful, balanced, rush).
            let mode = match args.get("mode") {
                None => None,
                Some(v) => Some(v.as_str().and_then(armies::mode::index).ok_or_else(|| {
                    Error::new(
                        "invalid_arguments",
                        "mode must be \"careful\", \"balanced\" or \"rush\"",
                    )
                })?),
            };
            // With a mode, execute is optional: the mode alone only sets the mode.
            let run = match args.get("execute") {
                None if mode.is_some() => None,
                v => Some(v.and_then(Value::as_bool).ok_or_else(|| {
                    Error::new(
                        "invalid_arguments",
                        "execute must be a boolean (it may be left out only when mode is given)",
                    )
                })?),
            };
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let keys = &self.keys;
            let mut mode_receipt = None;
            if let Some(wanted) = mode {
                // The mode is its own journalled action, sent before the execute or stop order
                // (request id <id>:mode when one follows). Only this dispatch's own readback
                // says what happened: the mode shows on the bar's buttons while the army is
                // selected, and no memory field of it is known.
                let mode_id = if run.is_some() {
                    format!("{id}:mode")
                } else {
                    id.to_string()
                };
                if !actions.receipt_exists(&mode_id) {
                    self.keys.preflight()?;
                }
                let action =
                    json!({"kind":"army_plan","army":army,"mode":armies::mode::MODES[wanted]});
                let attempt: std::cell::RefCell<Option<armies::mode::Attempt>> =
                    std::cell::RefCell::new(None);
                let mut settle = || {
                    Ok(match attempt.borrow().as_ref() {
                        Some(done) => done.outcome(),
                        None => journal::Outcome::Unobservable,
                    })
                };
                let mut dispatch = || {
                    authority.authorize(token, &epoch)?;
                    let mut done = armies::mode::set(process, keys, army, wanted);
                    let error = done.error.take();
                    *attempt.borrow_mut() = Some(done);
                    error.map_or(Ok(()), Err)
                };
                let receipt =
                    actions.execute(&mode_id, &epoch, action, &mut settle, &mut dispatch)?;
                if run.is_none() || receipt.status != journal::Status::Verified {
                    return Ok(json!({"receipt":receipt}));
                }
                mode_receipt = Some(receipt);
            }
            let Some(run) = run else {
                return Err(Error::new(
                    "invalid_arguments",
                    "execute or mode is required",
                ));
            };
            let state = || ui::read(process, |r, i| armies::executing(r, i, army));
            let retry = actions.receipt_exists(id);
            if !retry {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"army_plan","army":army,"execute":run});
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
            return Ok(match mode_receipt {
                Some(mode) => json!({"receipt":receipt,"mode_receipt":mode}),
                None => json!({"receipt":receipt}),
            });
        }
        if name == "hoi4_army_front" {
            allowed(
                args,
                &[
                    "army",
                    "state_id",
                    "from_state",
                    "from_states",
                    "provinces",
                    "against",
                    "own_states",
                    "parent_order",
                    "replace_order",
                    "request_id",
                    "lease_token",
                ],
            )?;
            let army = string(args, "army")?;
            let stated = front_stated(args)?;
            if army.is_empty() || army.len() > 128 || army.chars().any(char::is_control) {
                return Err(Error::new("invalid_arguments", "Invalid army name"));
            }
            let state = args
                .get("state_id")
                .and_then(Value::as_i64)
                .filter(|v| (1..4096).contains(v))
                .ok_or_else(|| Error::new("invalid_arguments", "state_id must be 1..4095"))?
                as i32;
            let one =
                match args.get("from_state") {
                    None | Some(Value::Null) => None,
                    Some(v) => Some(v.as_i64().filter(|v| (1..4096).contains(v)).ok_or_else(
                        || Error::new("invalid_arguments", "from_state must be 1..4095"),
                    )? as i32),
                };
            let many = armies::segment_states(args.get("from_states"))?;
            let listed = many.clone();
            let segment: Option<Vec<i32>> = match (one, many) {
                (Some(_), Some(_)) => {
                    return Err(Error::new(
                        "invalid_arguments",
                        "give from_state or from_states, not both",
                    ))
                }
                (Some(s), None) => Some(vec![s]),
                (None, many) => many,
            };
            let id = string(args, "request_id")?;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let orders = || ui::read(process, |r, i| armies::orders(r, i, army));
            let before = orders()?;
            let retry = actions.receipt_exists(id);
            // engineer 73: a front stated by provinces goes through the module's orders family when it is usable
            // (never under auto before its live pass); otherwise the drag below, which ignores the province list
            let inproc_route = stated.as_ref().and_then(|_| inproc::route("orders"));
            if !retry && inproc_route.is_none() {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"army_front","army":army,"state_id":state,"from_state":one});
            let mut action = action;
            if let Some(list) = &listed {
                action["from_states"] = json!(list);
            }
            let keys = &self.keys;
            // engineer 95: a stated line the game's sections did not hold whole is reported, never Verified
            let cut: std::cell::RefCell<Option<Value>> = std::cell::RefCell::new(None);
            // the coverage goes in the receipt itself too (o96a, 28 Nov 2026: read in the receipt, not found)
            let covered: std::cell::RefCell<Option<Value>> = std::cell::RefCell::new(None);
            let mut settle = || {
                let now = orders()?;
                let mut detail = json!({"army":army,"orders":now});
                if let Some(c) = covered.borrow().as_ref() {
                    detail["coverage"] = c.clone();
                }
                if let Some(c) = cut.borrow().as_ref() {
                    detail["front_cut"] = c.clone();
                    return Ok(journal::Outcome::Diverged(detail));
                }
                Ok(if retry {
                    journal::Outcome::Diverged(detail)
                } else {
                    settle::order_growth(before, now).outcome(detail)
                })
            };
            let mut inproc_route = inproc_route;
            let mut inproc_reply = Value::Null;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                if let (Some(ch), Some(st)) = (inproc_route.as_mut(), stated.as_ref()) {
                    inproc_reply = front_in_process(process, ch, army, st)?;
                    let c = &inproc_reply["coverage"];
                    *covered.borrow_mut() = Some(c.clone());
                    if c["uncovered"].as_array().is_some_and(|a| !a.is_empty()) {
                        *cut.borrow_mut() = Some(c.clone());
                    }
                    return Ok(());
                }
                armies::front(process, keys, army, state, segment.as_deref())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(if inproc_reply.is_null() {
                json!({"receipt":receipt})
            } else {
                json!({"receipt":receipt,"in_process":inproc_reply})
            });
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
            let action = json!({"kind":"army_assign","state_id":state,"army":target});
            let keys = &self.keys;
            if actions.receipt_exists(id) {
                // A retry settles from the baseline stored in the intent; the pre-checks
                // would refuse now that the divisions have moved.
                let mut settle = |baseline: &Value| {
                    let now = armies::armies(process)?;
                    let now_roster = settle::Roster::from_view(&now);
                    let (before_roster, pool) = settle::Roster::from_baseline(baseline);
                    Ok(
                        match (baseline["army"].as_str(), baseline["take"].as_u64()) {
                            (Some(army), Some(take)) => settle::army_assign(
                                &before_roster,
                                &now_roster,
                                army,
                                &pool,
                                take as usize,
                            )
                            .outcome(now),
                            _ => journal::Outcome::Diverged(now),
                        },
                    )
                };
                let receipt = actions.execute_baselined(
                    id,
                    &epoch,
                    action,
                    Value::Null,
                    &mut settle,
                    &mut || Err(Error::new("action_uncertain", "A retry never dispatches")),
                )?;
                return Ok(json!({"receipt":receipt}));
            }
            let before = armies::armies(process)?;
            let before_roster = settle::Roster::from_view(&before);
            let plan = armies::plan_assign(&before, state, target, armies::ARMY_CAP)?;
            armies::assign_preflight(&plan)?;
            armies::check_create(&before, state, false)?;
            self.keys.preflight()?;
            let pool = armies::unassigned_ids(&before, state);
            let mut baseline = before_roster.baseline(&pool);
            if let armies::AssignMode::Absorb { army } = &plan.mode {
                baseline["army"] = json!(army);
                baseline["take"] = json!(plan.take);
            }
            let mut settle = |_: &Value| {
                let now = armies::armies(process)?;
                let now_roster = settle::Roster::from_view(&now);
                Ok(if let armies::AssignMode::Absorb { army } = &plan.mode {
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
            let receipt = actions.execute_baselined(
                id,
                &epoch,
                action,
                baseline,
                &mut settle,
                &mut dispatch,
            )?;
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
            // The queues standing before: the verdict counts the lines of the queues added since,
            // not the template's total, which falls whenever an older queue deploys meanwhile.
            let standing = {
                process.begin_snapshot()?;
                let r = crate::memory::Reader::new(process);
                let ids = deployment::queue_ids(&r, process.image_base(), template)?;
                process.check()?;
                ids
            };
            let retry = actions.receipt_exists(id);
            // engineer 82: in process (family `training`): the Train button's command, then the new queue's series,
            // lines and location, no panel; the deploy state's first own province as the location click
            let mut train_route = if retry {
                None
            } else {
                inproc::route("training")
            };
            let train_province = match train_route {
                Some(_) => mapdata::game_dir(&epoch.game.exe)
                    .and_then(|d| mapdata::state_provinces(&d, deploy_state))
                    .ok()
                    .and_then(|p| p.first().copied()),
                None => None,
            };
            if train_province.is_none() {
                train_route = None;
            }
            if !retry && train_route.is_none() {
                self.keys.preflight()?;
            }
            let action = json!({"kind":"train","template":template,"count":count});
            let keys = &self.keys;
            let mut settle = || {
                let now = queued()?;
                process.begin_snapshot()?;
                let r = crate::memory::Reader::new(process);
                let added = deployment::added_lines(&r, process.image_base(), template, &standing)?;
                process.check()?;
                let detail = json!({"template":template,"queued_lines":now,"added_lines":added});
                Ok(if retry {
                    journal::Outcome::Diverged(detail)
                } else {
                    settle::lines_added_exactly(added as i64, count as i64).outcome(detail)
                })
            };
            let mut train_reply = None;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                if let Some(p) = train_province {
                    train_reply =
                        inproc_training::train(process, &mut train_route, template, count, p)?;
                    if train_reply.is_some() {
                        return Ok(());
                    }
                    keys.preflight()?;
                }
                deployment::train(process, keys, template, count)?;
                deployment::set_locations(process, keys, deploy_state).map(|_| ())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(with_inproc(receipt, train_reply));
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
            let retry = actions.receipt_exists(id);
            // engineer 82: in process (family `training`): the location command for every queue without one, the
            // state's first own province as the click; the queues' own province field is the read-back (no panel)
            let mut locate_route = if retry {
                None
            } else {
                inproc::route("training")
            };
            let locate_province = match locate_route {
                Some(_) => mapdata::game_dir(&epoch.game.exe)
                    .and_then(|d| mapdata::state_provinces(&d, state))
                    .ok()
                    .and_then(|p| p.first().copied()),
                None => None,
            };
            if locate_province.is_none() {
                locate_route = None;
            }
            let in_memory = locate_route.is_some();
            let unlocated = || {
                if in_memory {
                    inproc_training::unlocated(process)
                } else {
                    ui::read(process, deployment::unlocated)
                }
            };
            let start = unlocated()?;
            if !retry && !in_memory {
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
            let mut locate_reply = None;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                if let Some(p) = locate_province {
                    locate_reply = inproc_training::locate(process, &mut locate_route, p)?;
                    if locate_reply.is_some() {
                        return Ok(());
                    }
                    keys.preflight()?;
                }
                deployment::set_locations(process, keys, state).map(|_| ())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(with_inproc(receipt, locate_reply));
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
            allowed(args, &["decision", "target", "request_id", "lease_token"])?;
            let key = string(args, "decision")?;
            let target = match args.get("target") {
                None => None,
                Some(v) => Some(
                    v.as_str()
                        .filter(|s| !s.trim().is_empty() && s.len() <= 96)
                        .ok_or_else(|| {
                            Error::new(
                                "invalid_arguments",
                                "target must be the target's name as the row shows it",
                            )
                        })?,
                ),
            };
            if key.is_empty()
                || key.len() > 128
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
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
            let action = json!({"kind":"decision_take","decision":key,"target":target});
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
            // engineer 76: the decision in process (family `decisions`): the panel's row gives the decision object
            let mut decisions_route = inproc::route("decisions");
            let mut decisions_reply = None;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                let mut post = |decision: u64| -> Result<bool> {
                    decisions_reply = module_in_process(&mut decisions_route, |ch| {
                        Ok(inproc_picks::decision(ch, decision))
                    })?;
                    Ok(decisions_reply.is_some())
                };
                decisions::take(process, keys, key, target, &mut post)?;
                verified.set(true);
                Ok(())
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(with_inproc(receipt, decisions_reply));
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
            let category = politics_category(args)?;
            let index = args
                .get("slot")
                .and_then(Value::as_u64)
                .filter(|v| *v <= 7)
                .ok_or_else(|| Error::new("invalid_arguments", POLITICS_SLOT_HELP))?
                as usize;
            let token = string(args, "lease_token")?;
            let epoch = guard.observe(process)?.epoch;
            authority.authorize(token, &epoch)?;
            let before = politics::current(process, category, index)?;
            if !choosing {
                self.keys.preflight()?;
                let slots = politics::slots(process, category)?;
                let options = politics::list(process, &self.keys, category, index)?;
                return Ok(
                    json!({"category":category,"slot":index,"current":before,"options":options,"slots":slots}),
                );
            }
            let idea = string(args, "idea")?;
            if idea.is_empty()
                || idea.len() > 128
                || !idea
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            {
                return Err(Error::new("invalid_arguments", "Invalid idea key"));
            }
            let id = string(args, "request_id")?;
            let retry = actions.receipt_exists(id);
            // engineer 79: in process (family `politics`): a law (an idea of the game's idea database) by the politics
            // view's own command, the objects from the game's lists (the idea database, the own active ideas), no panel;
            // a key that is no idea (an advisor, staff: characters) goes to the mouse path
            let mut law_route = if retry {
                None
            } else {
                inproc::route("politics")
            };
            let law = match law_route {
                Some(_) => ui::read(process, |r, i| inproc_politics::law(r, i, idea))?,
                None => None,
            };
            if law.is_none() {
                law_route = None;
            }
            let law_before = law
                .as_ref()
                .and_then(|l| l.current.as_ref().map(|c| c.1.clone()));
            if !retry && law.is_none() {
                if before.as_deref() == Some(idea)
                    || politics::holder(process, category, idea)?.is_some()
                {
                    return Err(Error::new(
                        "invalid_target",
                        "The slot already holds this idea (or another slot of the category does)",
                    ));
                }
                self.keys.preflight()?;
            }
            let in_process = law.is_some();
            let action =
                json!({"kind":"politics_choose","category":category,"slot":index,"idea":idea});
            let keys = &self.keys;
            // a law posted in process takes effect on the game's next frame: a read 90 ms later still
            // showed the old law (s81b, 24 Nov 2026: limited_exports and service_by_requirement read
            // back NotApplied, PP spent and both active a moment later). Unchanged counts only after
            // IPC_SETTLE, as for production.
            let ipc_posted: std::cell::Cell<Option<std::time::Instant>> =
                std::cell::Cell::new(None);
            let mut settle = || {
                if in_process {
                    // the own active ideas: the law active = Reached; the replaced one still active = Unchanged
                    let own = ui::read(process, inproc_politics::active)?;
                    let keys_now: Vec<&str> = own.iter().map(|(_, k)| k.as_str()).collect();
                    let detail = json!({"active_ideas":keys_now,"replaced":law_before});
                    let early = ipc_posted
                        .get()
                        .is_some_and(|t| t.elapsed() < inproc::IPC_SETTLE);
                    return Ok(if keys_now.contains(&idea) {
                        journal::Outcome::Reached(detail)
                    } else if early {
                        journal::Outcome::Unknown
                    } else if law_before
                        .as_deref()
                        .is_none_or(|old| keys_now.contains(&old))
                    {
                        journal::Outcome::Unchanged(detail)
                    } else {
                        journal::Outcome::Diverged(detail)
                    });
                }
                let now = politics::current(process, category, index)?;
                let held = politics::holder(process, category, idea)?;
                let detail = json!({"current":now,"held_in_slot":held});
                Ok(if held.is_some() {
                    journal::Outcome::Reached(detail)
                } else if now == before {
                    journal::Outcome::Unchanged(detail)
                } else {
                    journal::Outcome::Diverged(detail)
                })
            };
            let mut law_reply = None;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                if let Some(l) = law.as_ref() {
                    law_reply =
                        module_in_process(&mut law_route, |ch| Ok(inproc_politics::take(ch, l)))?;
                    if law_reply.is_some() {
                        ipc_posted.set(Some(std::time::Instant::now()));
                        return Ok(());
                    }
                    keys.preflight()?;
                }
                politics::choose(process, keys, category, index, idea)
            };
            let receipt = actions.execute(id, &epoch, action, &mut settle, &mut dispatch)?;
            return Ok(with_inproc(receipt, law_reply));
        }
        if name == "hoi4_construction_add" {
            allowed(
                args,
                &[
                    "building",
                    "state_id",
                    "province_id",
                    "count",
                    "fill",
                    "bulk",
                    "request_id",
                    "lease_token",
                ],
            )?;
            let key = string(args, "building")?;
            // fill (with bulk): the state's free slots for the building in one Shift+Click
            let fill = args.get("bulk").and_then(Value::as_bool).unwrap_or(false)
                && args.get("fill").and_then(Value::as_bool).unwrap_or(false);
            let province = match args.get("province_id") {
                None => None,
                Some(v) => Some(v.as_i64().filter(|v| (1..100_000).contains(v)).ok_or_else(
                    || Error::new("invalid_arguments", "province_id must be 1..99999"),
                )? as i32),
            };
            if key.is_empty()
                || key.len() > 64
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
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
            let action = json!({"kind":"construction_add","building":key,"state_id":state,"province_id":province,"count":count});
            let start = construction::queued(process, key, state)?;
            let retry = actions.receipt_exists(id);
            // engineer 82: in process (family `construction`): the map click's own command, one per click, the building
            // reference as the game builds it (a province building on the named province, a state building in the
            // state); Shift's fill and a province building without a province stay on the mouse
            let mut build_route = if retry || fill {
                None
            } else {
                inproc::route("construction")
            };
            let placement = match build_route {
                Some(_) => ui::read(process, |r, i| {
                    crate::construction::template_placement(r, i, key)
                })
                .ok(),
                None => None,
            };
            let on_province = placement.is_some_and(|(_, p)| p);
            let template = placement
                .filter(|(_, p)| !*p || province.is_some())
                .map(|(t, _)| t);
            if template.is_none() {
                build_route = None;
            }
            let in_process = build_route.is_some();
            let location = if on_province {
                province.unwrap_or(0)
            } else {
                state
            };
            if !retry && !in_process {
                self.keys.preflight()?;
            }
            let keys = &self.keys;
            // a command posted in process runs on the game's next frame: an unchanged queue counts only after IPC_SETTLE
            let ipc_posted: std::cell::Cell<Option<Instant>> = std::cell::Cell::new(None);
            // The pre-input amount is stored in the intent, so a retry settles against it.
            let mut settle = |baseline: &Value| {
                let original = baseline.as_i64().unwrap_or(start);
                let now = construction::queued(process, key, state)?;
                let detail = json!({"building":key,"state_id":state,"queued_in_state":now,"queued_before":original});
                let early = ipc_posted
                    .get()
                    .is_some_and(|t| t.elapsed() < inproc::IPC_SETTLE);
                if early && now < original + count {
                    return Ok(journal::Outcome::Unknown);
                }
                Ok(settle::queue_growth(original, now, count).outcome(detail))
            };
            let mut build_reply = None;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                if let Some(t) = template {
                    build_reply = module_in_process(&mut build_route, |ch| {
                        Ok(inproc_construction::add(
                            ch,
                            t,
                            location,
                            on_province,
                            count,
                        ))
                    })?;
                    if build_reply.is_some() {
                        ipc_posted.set(Some(Instant::now()));
                        return Ok(());
                    }
                    keys.preflight()?;
                }
                construction::add(process, keys, key, state, count, province, fill)
            };
            let receipt = actions.execute_baselined(
                id,
                &epoch,
                action,
                json!(start),
                &mut settle,
                &mut dispatch,
            )?;
            return Ok(with_inproc(receipt, build_reply));
        }
        if name == "hoi4_focus_start" {
            allowed(args, &["focus", "request_id", "lease_token"])?;
            let key = string(args, "focus")?;
            if key.is_empty()
                || key.len() > 128
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
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
                // Fixed up front, never extended later. The budget covers what the action is
                // built to do: a cold tree needs ~3 s before the pan stage and the pan loop may
                // take its full twelve drags at about 1.2 s each (live: 12 s expired at the tenth
                // drag of a node that was still coming closer, so far nodes could never start).
                until: (Instant::now() + focus::START_BUDGET).min(lease_end),
                stop: None,
            };
            let check_input = || fence.input_check();
            let bounded_keys = self.keys.guarded(&fence, &check_input)?;
            let process: &(dyn Process + Send + Sync) = &fence;
            let action = json!({"kind":"focus_start","focus":key});
            let mut picks_route = inproc::route("picks");
            let had_route = picks_route.is_some();
            if !actions.receipt_exists(id) {
                focus::check(&focus::progress(process)?, key)?;
                if !had_route {
                    bounded_keys.preflight()?;
                }
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
            let mut picks_reply = None;
            let mut dispatch = || {
                authority.authorize(token, &epoch)?;
                // engineer 75: SetNationalFocus(own tag, the focus) after the game's own "can start", one command
                picks_reply = module_in_process(&mut picks_route, |ch| {
                    let f = crate::own_focus::focus_address(process, key)?;
                    Ok(inproc_picks::focus(ch, f))
                })?;
                if picks_reply.is_some() {
                    return Ok(());
                }
                if had_route {
                    keys.preflight()?;
                }
                focus::start(process, keys, key, fence.until)
            };
            let receipt =
                actions.execute_with(id, &epoch, action, &mut settle, &mut dispatch, true)?;
            return Ok(with_inproc(receipt, picks_reply));
        }
        allowed(args, &["paused", "speed", "request_id", "lease_token"])?;
        let id = string(args, "request_id")?;
        let token = string(args, "lease_token")?;
        // Both targets in one call (players write {"paused":false,"speed":3}): the speed goes
        // first under `<id>-speed`, then the pause state under the id itself.
        if let (Some(paused), Some(speed)) = (args.get("paused"), args.get("speed")) {
            let paused = paused
                .as_bool()
                .ok_or_else(|| Error::new("invalid_arguments", "Paused must be a boolean"))?;
            let speed = speed
                .as_u64()
                .filter(|v| *v <= 4)
                .ok_or_else(|| Error::new("invalid_arguments", "Speed must be integer 0..4"))?
                as u8;
            let epoch = guard.epoch.clone();
            let mut backend = ClockBackend {
                process,
                keys: &self.keys,
                guard,
                authority,
                token,
            };
            let first = journal.execute(
                &format!("{id}-speed"),
                &epoch,
                Action::SetSpeed(speed),
                &mut backend,
                Duration::from_secs(10),
            )?;
            let receipt = journal.execute(
                id,
                &epoch,
                Action::SetPaused(paused),
                &mut backend,
                Duration::from_secs(10),
            )?;
            return Ok(json!({"receipt":receipt,"speed_receipt":first}));
        }
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
                    "Supply a clock target: paused, speed or both",
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

/// The new offensive arrows of a plan read back after a draw (ids absent `before`) when none of
/// them reaches a wanted state: the arrow's target states (a spearhead's path as well). None
/// when no new arrow stands or one reaches the line.
/// The target line of offensive `id` in a plan read (`armies::orders::of_named`): its province count, ends and
/// states; None when no front holds that offensive. Pure.
fn replaced_target(plan: &Value, id: i32) -> Option<Value> {
    plan.as_array()
        .into_iter()
        .flatten()
        .flat_map(|front| front["offensives"].as_array().cloned().unwrap_or_default())
        .find(|o| o["id"].as_i64() == Some(i64::from(id)))
        .map(|o| o["target"].clone())
}

fn offensive_miss(before: &Value, now: &Value, want: &[i32], spearhead: bool) -> Option<Value> {
    let arrows = |plan: &Value| -> Vec<Value> {
        plan.as_array()
            .into_iter()
            .flatten()
            .flat_map(|front| front["offensives"].as_array().cloned().unwrap_or_default())
            .collect()
    };
    let old: std::collections::HashSet<i64> = arrows(before)
        .iter()
        .filter_map(|o| o["id"].as_i64())
        .collect();
    let new: Vec<Value> = arrows(now)
        .into_iter()
        .filter(|o| o["id"].as_i64().is_some_and(|id| !old.contains(&id)))
        .collect();
    if new.is_empty() {
        return None;
    }
    let states = |v: &Value| -> Vec<i32> {
        v.as_array()
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_i64().map(|n| n as i32))
            .collect()
    };
    let reach = |o: &Value| -> Vec<i32> {
        let mut r = states(&o["target"]["states"]);
        if spearhead {
            r.extend(states(&o["path_states"]));
        }
        r
    };
    if new
        .iter()
        .any(|o| reach(o).iter().any(|s| want.contains(s)))
    {
        return None;
    }
    Some(json!({
        "wanted_states": want,
        "arrows": new
            .iter()
            .map(|o| json!({"id": o["id"], "target_states": o["target"]["states"]}))
            .collect::<Vec<_>>(),
        "why": "the game drew the line to other states (the wanted ones are not reachable from this front as drawn)"
    }))
}

#[cfg(test)]
mod offensive_miss_tests {
    use super::*;

    fn plan(arrows: &[(i64, &[i64])]) -> Value {
        json!([{"kind": "front", "offensives": arrows
            .iter()
            .map(|(id, t)| json!({"id": id, "target": {"states": t}, "path_states": [42, 8]}))
            .collect::<Vec<_>>()}])
    }

    #[test]
    fn an_arrow_drawn_to_other_states_is_a_miss() {
        // s55a: a stroke into 34 read back over 18 (the front at 42-8)
        let before = plan(&[(115, &[18])]);
        let now = plan(&[(115, &[18]), (121, &[18]), (123, &[980])]);
        let m = offensive_miss(&before, &now, &[34], false).expect("a miss");
        assert_eq!(m["arrows"].as_array().unwrap().len(), 2);
        assert!(offensive_miss(&before, &now, &[34, 980], false).is_none());
        // no new arrow: nothing to judge
        assert!(offensive_miss(&now, &now, &[34], false).is_none());
        // a spearhead reaches through its path
        assert!(offensive_miss(&before, &now, &[8], true).is_none());
    }

    #[test]
    fn a_new_path_is_read_on_the_same_order() {
        // o96a: the path took on order 100 and no order was added
        let before = plan(&[(100, &[6, 34])]);
        let now = plan(&[(100, &[8, 6, 977, 34, 980])]);
        let (a, b) = (replaced_target(&before, 100), replaced_target(&now, 100));
        assert!(a.is_some() && b.is_some() && a != b);
        assert_eq!(replaced_target(&now, 101), None);
    }
}

#[cfg(test)]
mod argument_tests {
    use super::*;
    fn args(v: Value) -> serde_json::Map<String, Value> {
        v.as_object().unwrap().clone()
    }
    #[test]
    fn politics_categories_take_the_words_players_used_and_name_the_right_place() {
        assert_eq!(
            politics_category(&args(json!({"category": "government"}))).unwrap(),
            "government"
        );
        assert_eq!(
            politics_category(&args(json!({"category": "advisor"}))).unwrap(),
            "government"
        );
        assert_eq!(
            politics_category(&args(json!({"category": "High Command"}))).unwrap(),
            "military_staff"
        );
        let e = politics_category(&args(json!({"category": "officer_corps"}))).unwrap_err();
        assert!(e.message.contains("hoi4_officer_corps"), "{}", e.message);
        let e = politics_category(&args(json!({}))).unwrap_err();
        assert!(e.message.contains("research_production"), "{}", e.message);
        assert!(POLITICS_SLOT_HELP.contains("political advisors"));
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

/// A `design` argument: a positive design id, or a design's name as hoi4_designs lists it
/// (case-insensitive, exact), resolved from the own design list in memory.
fn design_arg(process: &dyn Process, value: &Value) -> Result<i32> {
    if let Some(id) = value
        .as_i64()
        .and_then(|d| i32::try_from(d).ok())
        .filter(|d| *d > 0)
    {
        return Ok(id);
    }
    let name = value
        .as_str()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .ok_or_else(|| {
            Error::new(
                "invalid_arguments",
                "design must be a design id or a design name from hoi4_designs",
            )
        })?;
    let all = designs::read_all(process)?;
    let found: Vec<i32> = all
        .iter()
        .filter(|d| d.variant.name.eq_ignore_ascii_case(name))
        .map(|d| d.variant.id)
        .collect();
    match found.as_slice() {
        [id] => Ok(*id),
        [] => Err(Error::new(
            "design_unknown",
            format!("No own design is named {name:?} (hoi4_designs lists the names and ids)"),
        )),
        _ => Err(Error::new(
            "design_ambiguous",
            format!("Several own designs are named {name:?}: pass the design id ({found:?})"),
        )),
    }
}

/// `HOI4_SPEED_LOCK` (internal speed 0..4): any other requested speed is refused.
fn speed_lock(lock: Option<&str>, want: i64) -> Result<()> {
    match lock.and_then(|l| l.trim().parse::<i64>().ok()) {
        Some(locked) if locked != want => Err(Error::new(
            "speed_locked",
            format!("This run holds the game at internal speed {locked} (HOI4_SPEED_LOCK); speed {want} is refused. Keep playing at the run's speed"),
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod speed_lock_tests {
    #[test]
    fn a_locked_run_refuses_other_speeds_only() {
        assert!(super::speed_lock(Some("2"), 2).is_ok());
        assert_eq!(
            super::speed_lock(Some("2"), 1).unwrap_err().code,
            "speed_locked"
        );
        assert!(super::speed_lock(None, 1).is_ok());
        assert!(super::speed_lock(Some(""), 4).is_ok());
    }
}

/// The order kind an `[order]` timing line is filed under: the tool without its prefix, with
/// the variant that changes its input path (a move of listed divisions, a halt, a plan mode).
fn order_kind(name: &str, arguments: &Value) -> String {
    let base = name.trim_start_matches("hoi4_");
    let has = |k: &str| arguments.get(k).is_some_and(|v| !v.is_null());
    let variant = match base {
        "army_move" if has("divisions") => "divisions",
        "army_move" if has("halt") => "halt",
        "army_plan" if has("mode") && !has("execute") => "mode",
        "army_plan" if has("clear_orders") => "clear",
        "army_plan" if has("delete_order") => "delete",
        _ => "",
    };
    if variant.is_empty() {
        base.to_string()
    } else {
        format!("{base}.{variant}")
    }
}

/// engineer 95: what hoi4_army_plan's `assign` or `distribute` asks.
enum PlanAssign {
    Order {
        order: i32,
        divisions: Option<Vec<String>>,
        count: Option<usize>,
    },
    Distribute {
        weights: BTreeMap<i32, f64>,
    },
}

fn plan_assign_args(assign: Option<&Value>, distribute: Option<&Value>) -> Result<PlanAssign> {
    let bad = |m: &str| Error::new("invalid_arguments", m.to_string());
    if let Some(a) = assign {
        let order = a["order"]
            .as_i64()
            .filter(|o| (1..=i64::from(i32::MAX)).contains(o))
            .ok_or_else(|| bad("assign.order: an order id from the plan read-back"))?
            as i32;
        let divisions = match a.get("divisions") {
            None | Some(Value::Null) => None,
            Some(Value::Array(l)) if !l.is_empty() && l.len() <= 512 => Some(
                l.iter()
                    .map(|d| {
                        d.as_str()
                            .filter(|s| !s.is_empty() && s.len() <= 64)
                            .map(str::to_string)
                            .ok_or_else(|| {
                                bad("assign.divisions: division ids as hoi4_armies lists them")
                            })
                    })
                    .collect::<Result<Vec<_>>>()?,
            ),
            _ => return Err(bad("assign.divisions: 1..512 division ids")),
        };
        let count = match a.get("count") {
            None | Some(Value::Null) => None,
            Some(v) => Some(
                v.as_u64()
                    .filter(|n| (1..=512).contains(n))
                    .ok_or_else(|| bad("assign.count: 1..512"))? as usize,
            ),
        };
        if divisions.is_some() == count.is_some() {
            return Err(bad("assign takes divisions or count, one of them"));
        }
        if let Some(o) = a.as_object() {
            if o.keys()
                .any(|k| !["order", "divisions", "count"].contains(&k.as_str()))
            {
                return Err(bad("assign: order with divisions or count"));
            }
        }
        return Ok(PlanAssign::Order {
            order,
            divisions,
            count,
        });
    }
    let mut weights = BTreeMap::new();
    match distribute {
        Some(Value::Bool(true)) => {}
        Some(Value::Object(o)) => {
            if o.keys().any(|k| k != "weights") {
                return Err(bad("distribute: true or {weights: {order id: weight}}"));
            }
            for (k, v) in o
                .get("weights")
                .and_then(Value::as_object)
                .ok_or_else(|| bad("distribute.weights: {order id: weight}"))?
            {
                let id: i32 = k
                    .parse()
                    .ok()
                    .filter(|i| *i > 0)
                    .ok_or_else(|| bad("distribute.weights: keys are front order ids"))?;
                let w = v
                    .as_f64()
                    .filter(|w| w.is_finite() && *w >= 0.0)
                    .ok_or_else(|| bad("distribute.weights: weights are numbers 0 or more"))?;
                weights.insert(id, w);
            }
        }
        _ => return Err(bad("distribute: true or {weights: {order id: weight}}")),
    }
    Ok(PlanAssign::Distribute { weights })
}

/// A front stated exactly (engineer 73): explicit provinces, or the own border against a tag, optionally a parent
/// order or an existing front order to re-path. None when the call states none of these (the drag decides).
struct FrontStated {
    provinces: Option<Vec<i32>>,
    /// engineer 95: one tag or several (every border of the group in one call, each piece its own root)
    against: Option<(Vec<String>, Option<Vec<i32>>)>,
    parent: Option<i32>,
    replace: Option<i32>,
}

fn front_stated(args: &serde_json::Map<String, Value>) -> Result<Option<FrontStated>> {
    let ints = |k: &str, lo: i64, hi: i64, max: usize| -> Result<Option<Vec<i32>>> {
        match args.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::Array(a)) if !a.is_empty() && a.len() <= max => a
                .iter()
                .map(|v| {
                    v.as_i64()
                        .filter(|x| (lo..=hi).contains(x))
                        .map(|x| x as i32)
                        .ok_or_else(|| {
                            Error::new("invalid_arguments", format!("{k}: integers {lo}..{hi}"))
                        })
                })
                .collect::<Result<Vec<_>>>()
                .map(Some),
            _ => Err(Error::new(
                "invalid_arguments",
                format!("{k}: 1..{max} integers"),
            )),
        }
    };
    let one = |k: &str| -> Result<Option<i32>> {
        match args.get(k) {
            None | Some(Value::Null) => Ok(None),
            Some(v) => v
                .as_i64()
                .filter(|x| (1..=i64::from(i32::MAX)).contains(x))
                .map(|x| Some(x as i32))
                .ok_or_else(|| {
                    Error::new(
                        "invalid_arguments",
                        format!("{k} must be a positive order id"),
                    )
                }),
        }
    };
    let provinces = ints("provinces", 1, 99_999, 1024)?;
    let own_states = ints("own_states", 1, 4095, 64)?;
    let tag = |v: &Value| -> Option<String> {
        v.as_str()
            .filter(|t| t.len() == 3 && t.chars().all(|c| c.is_ascii_alphanumeric()))
            .map(str::to_ascii_uppercase)
    };
    let against = match args.get("against") {
        None | Some(Value::Null) => None,
        Some(Value::Array(l)) if !l.is_empty() && l.len() <= 16 => {
            let mut tags = Vec::new();
            for v in l {
                let t = tag(v).ok_or_else(|| {
                    Error::new(
                        "invalid_arguments",
                        "against: a three-letter country tag or a list of them",
                    )
                })?;
                if !tags.contains(&t) {
                    tags.push(t);
                }
            }
            Some(tags)
        }
        Some(v) => Some(vec![tag(v).ok_or_else(|| {
            Error::new(
                "invalid_arguments",
                "against: a three-letter country tag or a list of them",
            )
        })?]),
    };
    let parent = one("parent_order")?;
    let replace = one("replace_order")?;
    if provinces.is_some() && against.is_some() {
        return Err(Error::new(
            "invalid_arguments",
            "give provinces or against, not both",
        ));
    }
    if own_states.is_some() && against.is_none() {
        return Err(Error::new(
            "invalid_arguments",
            "own_states narrows against",
        ));
    }
    if (parent.is_some() || replace.is_some()) && provinces.is_none() && against.is_none() {
        return Err(Error::new(
            "invalid_arguments",
            "parent_order and replace_order need provinces or against",
        ));
    }
    if parent.is_some() && replace.is_some() {
        return Err(Error::new(
            "invalid_arguments",
            "give parent_order or replace_order, not both",
        ));
    }
    if provinces.is_none() && against.is_none() {
        return Ok(None);
    }
    Ok(Some(FrontStated {
        provinces,
        against: against.map(|t| (t, own_states)),
        parent,
        replace,
    }))
}

/// engineer 74: the offensive's exact target line through the module: a new spearhead (arrow plus its line) under
/// `parent` (default: the army's one top order), or the new path of an existing order (`replace`).
fn offensive_in_process(
    process: &dyn Process,
    ch: &mut inproc::Channel,
    army: &str,
    line: &[i32],
    parent: Option<i32>,
    replace: Option<i32>,
    spearhead: bool,
) -> Result<Value> {
    let group = ui::read(process, |r, i| armies::group_address(r, i, army))?;
    let nodes = inproc_orders::tree_nodes(&ch.orders_tree(group)?)?;
    let ids = inproc_orders::all_ids(&nodes);
    let req = match replace {
        Some(order) if ids.contains(&order) => {
            // engineer 95: an arrow's own path is the game's (re-derived from its front to its target line on every
            // change), so the new line goes to the arrow's target line, which the arrow then follows
            let to = inproc_orders::path_target(&nodes, order);
            inproc_orders::request("orders.set_path", group, to, line, None, None)?
        }
        Some(order) => {
            return Err(Error::new(
                "invalid_arguments",
                format!("order {order} is not in {army}'s plan"),
            ))
        }
        None => {
            let p = parent.or_else(|| inproc_orders::default_parent(&nodes)).ok_or_else(|| {
                Error::new(
                    "plan_parent_unknown",
                    format!("{army}'s plan has no single top order: name parent_order (one of {ids:?})"),
                )
            })?;
            if !ids.contains(&p) {
                return Err(Error::new(
                    "invalid_arguments",
                    format!("order {p} is not in {army}'s plan"),
                ));
            }
            if spearhead {
                inproc_orders::request2("orders2.spearhead", group, &[p as i64], line, &[], None)?
            } else {
                // engineer 89: the plain offensive: NewFront under the front (the arrow + its target line), the
                // front's free divisions handed over as the X stroke hands them
                inproc_orders::request("orders.new_front", group, p, line, None, None)?
            }
        }
    };
    let reply = ch.orders_post(req)?;
    if inproc::game_refused(&reply) {
        return Err(inproc::refused_error("offensive"));
    }
    if !inproc::all_posted(&reply) {
        return Err(Error::new(
            "inproc_refused",
            format!(
                "the module did not post the offensive: {}",
                reply["reason"]
                    .as_str()
                    .or(reply["error"].as_str())
                    .unwrap_or("?")
            ),
        ));
    }
    Ok(reply)
}

/// engineer 75: one army-structure change through the module when the `units` family is usable (never under `auto`
/// before its live pass): Some(the module's reply) when every command was posted, None when the mouse path runs,
/// the typed refusal when the game's own check refused the change (the mouse would send the same refused order).
fn module_in_process(
    route: &mut Option<inproc::Channel>,
    f: impl FnOnce(&mut inproc::Channel) -> Result<inproc::Routed>,
) -> Result<Option<Value>> {
    let Some(ch) = route.as_mut() else {
        return Ok(None);
    };
    match f(ch)? {
        inproc::Routed::Done(v) => Ok(Some(v)),
        inproc::Routed::Refused(what) => Err(inproc::refused_error(&what)),
        inproc::Routed::Gui => Ok(None),
    }
}

/// The receipt with the module's reply when the change went in process.
fn with_inproc<R: serde::Serialize>(receipt: R, reply: Option<Value>) -> Value {
    match reply {
        Some(r) => json!({"receipt":receipt,"in_process":r}),
        None => json!({"receipt":receipt}),
    }
}

/// engineer 82: one construction-queue order in process: the line's removal or the view's move, then the queue read
/// back until the game ran it (it runs on the next frame; a paused game runs commands too). Ok(None): the mouse path
/// runs (no channel, or the module could not explain its answer); a game refusal is the typed error.
fn construction_order_in_process(
    process: &dyn Process,
    route: &mut Option<inproc::Channel>,
    queue: &[crate::construction::InprocLine],
    line: usize,
    action: &str,
) -> Result<Option<(Value, Value)>> {
    let n = queue.len();
    let Some(target) = queue.get(line) else {
        return Err(Error::new(
            "invalid_target",
            format!("No construction line {line}: the queue has {n} (hoi4_construction_queue lists them in order)"),
        ));
    };
    let address = target.address;
    let to = inproc_construction::target_index(action, line, n);
    if action != "remove" && to.is_none() {
        return Err(Error::new(
            "invalid_arguments",
            "action must be top, up, down, bottom or remove",
        ));
    }
    if to == Some(line) {
        return Ok(Some((
            json!({"position": line, "unchanged": "already there"}),
            json!({"backend":"inprocess","posted":0}),
        )));
    }
    let reply = module_in_process(route, |ch| {
        Ok(match to {
            None => inproc_construction::remove(ch, target),
            Some(to) => inproc_construction::move_line(ch, queue, line, to),
        })
    })?;
    let Some(reply) = reply else {
        return Ok(None);
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let now = ui::read(process, crate::construction::inproc_queue)?;
        let at = now.iter().position(|l| l.address == address);
        let body = match (to, at) {
            (None, None) => {
                Some(json!({"removed": true, "lines_before": n, "lines_after": now.len()}))
            }
            (Some(want), Some(at)) if at == want => Some(json!({"from": line, "to": want})),
            _ => None,
        };
        if let Some(body) = body {
            return Ok(Some((body, reply)));
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                "input_readback_failed",
                format!("The module posted the order, but the line is at {at:?} after 3 s (wanted {to:?}; None = removed)"),
            ));
        }
        crate::phase::sleep(Duration::from_millis(30));
    }
}

/// engineer 74: a plan root for an own army or army group through the module, bound to `section` of the theatre front
/// `front`, split 0. engineer 95: it carries `divisions` (its share), never the whole group: the game puts every
/// division a root is given on that root, so a second root given all of them empties the first (ctl92a, 28 Nov 2026:
/// Holland's root 0 divisions, Belgium's 68). Returns the new root's id (the top order not there before) and the reply.
fn new_root_in_process(
    ch: &mut inproc::Channel,
    group: u64,
    front: u64,
    section: i32,
    divisions: &[u64],
) -> Result<(i32, Value)> {
    let before: std::collections::BTreeSet<i32> = inproc_orders::root_rows(&ch.orders_tree(group)?)
        .iter()
        .map(|r| r.id)
        .collect();
    let req = inproc_orders::request2(
        "orders2.new_root",
        group,
        &[section as i64, 0, 100_000, 0],
        &[],
        divisions,
        Some(front),
    )?;
    let reply = ch.orders_post(req)?;
    if inproc::game_refused(&reply) {
        return Err(inproc::refused_error("plan root"));
    }
    if !inproc::all_posted(&reply) {
        return Err(Error::new(
            "inproc_refused",
            format!(
                "the module did not post the root: {}",
                reply["reason"]
                    .as_str()
                    .or(reply["error"].as_str())
                    .unwrap_or("?")
            ),
        ));
    }
    let root = inproc_orders::root_rows(&ch.orders_tree(group)?)
        .iter()
        .map(|r| r.id)
        .find(|id| !before.contains(id))
        .ok_or_else(|| {
            Error::new(
                "inproc_unverified",
                "the root was posted but no new top order reads back",
            )
        })?;
    Ok((root, reply))
}

/// engineer 95: the group's divisions over `targets` (front lines as provinces) by `weights`, by place: each target
/// keeps what stands by it, the rest go to the nearest target with room (`inproc_orders::split_by_place`). `holder`
/// maps a division to the target that holds it now.
fn shares_by_place(
    process: &dyn Process,
    dir: &std::path::Path,
    group: u64,
    targets: &[Vec<i32>],
    weights: &[f64],
    holder: &BTreeMap<u64, usize>,
) -> Result<Vec<Vec<u64>>> {
    let placed = ui::read(process, |r, i| armies::group_divisions_placed(r, i, group))?;
    let pos = mapdata::positions(dir)?;
    let divs: Vec<(u64, Option<(f64, f64)>)> = placed
        .iter()
        .map(|(d, p)| (*d, p.and_then(|p| pos.get(&p).copied())))
        .collect();
    let current: Vec<Option<usize>> = divs.iter().map(|(d, _)| holder.get(d).copied()).collect();
    let lines: Vec<Vec<(f64, f64)>> = targets
        .iter()
        .map(|t| t.iter().filter_map(|p| pos.get(p).copied()).collect())
        .collect();
    let q = inproc_orders::quotas(divs.len(), weights);
    Ok(inproc_orders::split_by_place(&divs, &current, &lines, &q))
}

/// engineer 95: the group's order tree with each order's divisions in the group's own division addresses (the ones
/// orders2 takes and `group_divisions_placed` reads), matched by `inproc_orders::match_divisions`: an order's list did
/// not compare equal to them (o96a, 28 Nov 2026), so a front's own divisions read as none of its own.
fn group_tree(process: &dyn Process, ch: &mut inproc::Channel, group: u64) -> Result<Value> {
    let mut tree = ch.orders_tree(group)?;
    // the module's reference handle offset (stage2.common.handle_off: 8 on 1.20.1.0)
    let handle_off = ch.hello["stage2"]["common"]["handle_off"]
        .as_u64()
        .unwrap_or(8);
    let group_divs = ui::read(process, |r, i| {
        let mut out = Vec::new();
        for d in armies::group_division_ptrs(r, i, group)? {
            out.push((d, r.u64(d + handle_off).unwrap_or(0)));
        }
        Ok(out)
    })?;
    let values = inproc_orders::tree_division_values(&tree);
    let matched = inproc_orders::match_divisions(&values, &group_divs);
    inproc_orders::translate_divisions(&mut tree, &matched);
    tree["division_match"] = json!({"values": values.len(), "matched": matched.len()});
    Ok(tree)
}

/// engineer 95: divisions onto one order through the module (orders2.assign: the plan bar's assign); nothing is posted
/// for none.
fn assign_in_process(
    ch: &mut inproc::Channel,
    group: u64,
    order: i32,
    divisions: &[u64],
) -> Result<Value> {
    if divisions.is_empty() {
        return Ok(json!({"moved":0}));
    }
    let mut replies = Vec::new();
    for part in divisions.chunks(512) {
        let req = inproc_orders::request2(
            "orders2.assign",
            group,
            &[i64::from(order)],
            &[],
            part,
            None,
        )?;
        let reply = ch.orders_post(req)?;
        if inproc::game_refused(&reply) {
            return Err(inproc::refused_error("assignment"));
        }
        if !inproc::all_posted(&reply) {
            return Err(Error::new(
                "inproc_refused",
                format!(
                    "the module did not post the assignment to order {order}: {}",
                    reply["reason"]
                        .as_str()
                        .or(reply["error"].as_str())
                        .unwrap_or("?")
                ),
            ));
        }
        replies.push(reply);
    }
    Ok(json!({"moved":divisions.len(),"replies":replies}))
}

/// engineer 95: the group's divisions over its fronts (plan roots), by each front's weight (`weights` by root id,
/// else the front's length in provinces) and by place; every front that has a line gets at least one while there
/// are as many divisions as fronts. Each front's new divisions go to its one offensive when it has one (the attack
/// carries them), else to the front. Returns the report and the divisions each order must hold after it.
fn distribute_in_process(
    process: &dyn Process,
    ch: &mut inproc::Channel,
    army: &str,
    dir: &std::path::Path,
    weights: &BTreeMap<i32, f64>,
) -> Result<(Value, BTreeMap<i32, usize>)> {
    let group = ui::read(process, |r, i| armies::group_address(r, i, army))?;
    let tree = group_tree(process, ch, group)?;
    let roots = inproc_orders::root_rows(&tree);
    if roots.is_empty() {
        return Err(Error::new(
            "plan_front_missing",
            format!("{army} has no front to distribute its divisions over"),
        ));
    }
    let mut holder = BTreeMap::new();
    for (t, r) in roots.iter().enumerate() {
        for d in r.all_divisions() {
            holder.insert(d, t);
        }
    }
    let targets: Vec<Vec<i32>> = roots.iter().map(|r| r.provinces.clone()).collect();
    let w: Vec<f64> = roots
        .iter()
        .map(|r| {
            weights
                .get(&r.id)
                .copied()
                .unwrap_or((r.provinces.len() as f64).max(1.0))
        })
        .collect();
    let shares = shares_by_place(process, dir, group, &targets, &w, &holder)?;
    let mut rows = Vec::new();
    let mut want = BTreeMap::new();
    for (t, r) in roots.iter().enumerate() {
        let has = r.all_divisions();
        let to = r.assign_order();
        let on_order: std::collections::BTreeSet<u64> = if to == r.id {
            r.divisions.iter().copied().collect()
        } else {
            r.arrows
                .iter()
                .find(|(id, _)| *id == to)
                .map(|(_, d)| d.iter().copied().collect())
                .unwrap_or_default()
        };
        let moving: Vec<u64> = shares[t]
            .iter()
            .copied()
            .filter(|d| !has.contains(d))
            .collect();
        let reply = assign_in_process(ch, group, to, &moving)?;
        let stays = shares[t].iter().filter(|d| on_order.contains(d)).count();
        want.insert(to, stays + moving.len());
        rows.push(
            json!({"front":r.id,"order":to,"provinces":r.provinces.len(),"weight":w[t],
            "share":shares[t].len(),"held_before":has.len(),"moved_in":moving.len(),"reply":reply}),
        );
    }
    Ok((
        json!({"fronts":rows,"division_match":tree["division_match"]}),
        want,
    ))
}

/// engineer 95: divisions onto one order of `army`: the listed ones, or the `count` of the group nearest the order's
/// line that it does not hold yet. Returns the report and the divisions the order must hold after it.
fn assign_order_in_process(
    process: &dyn Process,
    ch: &mut inproc::Channel,
    army: &str,
    dir: &std::path::Path,
    order: i32,
    listed: Option<&[String]>,
    count: Option<usize>,
) -> Result<(Value, BTreeMap<i32, usize>)> {
    let group = ui::read(process, |r, i| armies::group_address(r, i, army))?;
    let tree = group_tree(process, ch, group)?;
    let (line, on) = inproc_orders::order_row(&tree, order).ok_or_else(|| {
        Error::new(
            "invalid_arguments",
            format!("order {order} is not in {army}'s plan"),
        )
    })?;
    let divisions: Vec<u64> = match (listed, count) {
        (Some(ids), _) => ui::read(process, |r, i| armies::division_ptrs(r, i, ids))?
            .into_iter()
            .filter(|d| !on.contains(d))
            .collect(),
        (None, Some(n)) => {
            let placed = ui::read(process, |r, i| armies::group_divisions_placed(r, i, group))?;
            let pos = mapdata::positions(dir)?;
            let at: Vec<(f64, f64)> = line.iter().filter_map(|p| pos.get(p).copied()).collect();
            let mut free: Vec<(f64, u64)> = placed
                .iter()
                .filter(|(d, _)| !on.contains(d))
                .map(|(d, p)| {
                    let dist = p
                        .and_then(|p| pos.get(&p).copied())
                        .map(|(x, z)| {
                            at.iter()
                                .map(|(a, b)| (a - x).powi(2) + (b - z).powi(2))
                                .fold(f64::INFINITY, f64::min)
                        })
                        .unwrap_or(f64::INFINITY);
                    (dist, *d)
                })
                .collect();
            free.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
            free.into_iter().take(n).map(|(_, d)| d).collect()
        }
        _ => {
            return Err(Error::new(
                "invalid_arguments",
                "assign needs divisions or count",
            ))
        }
    };
    let reply = assign_in_process(ch, group, order, &divisions)?;
    let want = BTreeMap::from([(order, on.len() + divisions.len())]);
    Ok((
        json!({"order":order,"held_before":on.len(),"moved_in":divisions.len(),"reply":reply,
               "division_match":tree["division_match"]}),
        want,
    ))
}

/// The stated front through the module: read the army's plan, resolve the province chains, then one plan root
/// (orders2.new_root, bound to the theatre front section holding the chain) per chain (or one orders.set_path on
/// `replace_order`, the first chain).
/// The journal's settle (one more battle-plan order on the army, from the external read) stays the authority.
fn front_in_process(
    process: &dyn Process,
    ch: &mut inproc::Channel,
    army: &str,
    st: &FrontStated,
) -> Result<Value> {
    let group = ui::read(process, |r, i| armies::group_address(r, i, army))?;
    let tree = group_tree(process, ch, group)?;
    let nodes = inproc_orders::tree_nodes(&tree)?;
    let exe = &process
        .identity()
        .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
        .exe;
    let dir = mapdata::game_dir(exe)?;
    let mut no_border: Vec<String> = Vec::new();
    let chains: Vec<Vec<i32>> = match (&st.provinces, &st.against) {
        (Some(p), _) => {
            if let Some(bad) = front_plan::foreign_province(process, &dir, p)? {
                return Err(Error::new(
                    "invalid_arguments",
                    format!("province {bad} is not in a state the player controls"),
                ));
            }
            vec![p.clone()]
        }
        (None, Some((tags, own))) => {
            // every border asked: a tag without a border in the given states is named, the others go on
            let mut all = Vec::new();
            let mut none = Vec::new();
            for tag in tags {
                match front_plan::border_front_provinces(process, &dir, tag, own.as_deref()) {
                    Ok(c) => all.extend(c),
                    Err(e) if e.code == "no_border" && tags.len() > 1 => none.push(tag.clone()),
                    Err(e) => return Err(e),
                }
            }
            if all.is_empty() {
                return Err(Error::new(
                    "no_border",
                    format!("No own border against any of {tags:?} in the given states"),
                ));
            }
            no_border = none;
            all
        }
        _ => return Err(Error::new("invalid_arguments", "no front stated")),
    };
    let ids = inproc_orders::all_ids(&nodes);
    let mut replies = Vec::new();
    let mut coverage = Value::Null;
    if let Some(order) = st.replace {
        if !ids.contains(&order) {
            return Err(Error::new(
                "invalid_arguments",
                format!("order {order} is not in {army}'s plan"),
            ));
        }
        let req = inproc_orders::request("orders.set_path", group, order, &chains[0], None, None)?;
        replies.push(ch.orders_post(req)?);
    } else {
        // engineer 89 (s89a/s89b, 27 Nov): COrderNewFrontCommand is the OFFENSIVE (an arrow and its target line in
        // enemy land under the parent front; the game drops every own province), so a front line by command is a plan
        // root bound to the theatre front section holding the chain, one per chain (an army or army group may hold
        // several fronts). A root needs no parent: parent_order is ignored here. s89b: the root equals the Z click's
        // order field by field (kind, flags, provinces, divisions) in 0.48 s against 2.72 s
        let _ = (st.parent, &ids);
        if !inproc::family_usable(&ch.hello, "orders2", inproc::mode()) {
            return Err(Error::new(
                "inproc_refused",
                "a front line by command is a plan root (family orders2), which is not usable here",
            ));
        }
        // engineer 95: a root binds to ONE section (one enemy's border): the line's pieces by section, one root
        // each, created together; the group's divisions split over the fronts it has and the new ones by length and
        // place; the provinces no section holds are reported (the game would have cut them silently)
        let fronts = ch.fronts(group)?;
        let mut pieces: Vec<(u64, i32, Vec<i32>)> = Vec::new();
        let mut uncovered: Vec<i32> = Vec::new();
        for chain in &chains {
            let (found, left) = inproc_orders::section_pieces(&fronts, chain);
            for (addr, sec, provs) in found {
                match pieces
                    .iter_mut()
                    .find(|(a, s2, _)| *a == addr && *s2 == sec)
                {
                    Some(p) => p.2.extend(provs),
                    None => pieces.push((addr, sec, provs)),
                }
            }
            uncovered.extend(left);
        }
        if pieces.is_empty() {
            return Err(Error::new(
                "plan_front_missing",
                "no front of the group's theatre touches the stated line (a root binds to an existing front)",
            ));
        }
        let roots = inproc_orders::root_rows(&tree);
        let mut holder = BTreeMap::new();
        for (t, r) in roots.iter().enumerate() {
            for d in r.all_divisions() {
                holder.insert(d, t);
            }
        }
        let mut targets: Vec<Vec<i32>> = roots.iter().map(|r| r.provinces.clone()).collect();
        targets.extend(pieces.iter().map(|p| p.2.clone()));
        let weights: Vec<f64> = targets.iter().map(|t| (t.len() as f64).max(1.0)).collect();
        let shares = shares_by_place(process, &dir, group, &targets, &weights, &holder)?;
        let mut made = Vec::new();
        for (k, (addr, sec, provs)) in pieces.iter().enumerate() {
            let share = &shares[roots.len() + k];
            let (root, reply) = new_root_in_process(ch, group, *addr, *sec, share)?;
            made.push(
                json!({"root":root,"section":sec,"provinces":provs.len(),"divisions":share.len()}),
            );
            replies.push(reply);
        }
        // a division the split gives a standing front that it does not hold yet (one no front held) goes to it
        let mut assigned = Vec::new();
        for (t, r) in roots.iter().enumerate() {
            let has = r.all_divisions();
            let moving: Vec<u64> = shares[t]
                .iter()
                .copied()
                .filter(|d| !has.contains(d))
                .collect();
            if !moving.is_empty() {
                assigned.push(assign_in_process(ch, group, r.assign_order(), &moving)?);
            }
        }
        let asked: usize = chains.iter().map(Vec::len).sum();
        coverage = json!({"asked":asked,"on_roots":asked - uncovered.len(),"uncovered":uncovered,
            "roots":made,"standing_fronts":roots.len(),"assigned":assigned,"no_border":no_border});
    }
    for r in &replies {
        if inproc::game_refused(r) {
            return Err(inproc::refused_error("front line"));
        }
        if !inproc::all_posted(r) {
            return Err(Error::new(
                "inproc_refused",
                format!(
                    "the module did not post the front: {}",
                    r["reason"].as_str().or(r["error"].as_str()).unwrap_or("?")
                ),
            ));
        }
    }
    Ok(
        json!({"chains":chains.iter().map(Vec::len).collect::<Vec<_>>(),"coverage":coverage,"replies":replies}),
    )
}
