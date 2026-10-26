//! Intelligence agency actions with ordinary input: `hoi4_agency_create`,
//! `hoi4_agency_upgrade`, `hoi4_operative_recruit`, `hoi4_operative_assign`.
//! UNVALIDATED: written from `interface/countryintelligenceagencyview.gui`,
//! `operativeleader.gui` and the shortcut table of `topbar.gui`; no live run
//! has located these windows yet.
//!
//! Nothing about the agency (created flag, upgrade levels, operative roster)
//! is mapped in game memory (`hoi4_intelligence` reports `state_read:false`),
//! so every refusal that needs that state comes from the window itself after
//! it is opened (before any state-changing click), and every success is read
//! back from the window too. That evidence is recorded in the receipt's
//! `observed` and marked `verification: "gui_only"`.
//!
//! Route: Shift+W opens the agency view (`intel_agency_button`). Creation:
//! `creation_button`, the name/logo window's `create_button`. Upgrades: the
//! branch's `background_button` in `upgrades_list`, then the entry's
//! `upgrade_button` in the upgrades window. Operatives: the bottom bar's empty
//! slot `portrait_btn` opens the recruitment window; a recruited operative's
//! `portrait_btn` opens the order bar whose `btn_mission_N` (N = the mission
//! icon frame: 1 build intel network, 2 quiet intel network, 3 counter
//! intelligence, 4 root out resistance, 5 boost ideology, 6 control trade,
//! 7 diplomatic pressure, 8 propaganda, from the localisation's
//! `operative_mission_icons_small|N` markers) arms the mission and a click on
//! the map target (the Find view centres a state of the target) starts it.
use super::{
    diplomacy_state, intelligence, loc, mgmt, panelkit as pk,
    ui::{accept_command, clear_notices, find_state, read, settled, wait},
    ui_action::{self, refuse, Driven, Fail, Job},
    Guard, InputAuthority,
};
use crate::{
    error::{Error, Result},
    gui_tree,
    memory::Reader,
    platform::{macos_input::MacKeys, Process},
};
use serde_json::{json, Value};
use std::{cell::RefCell, thread, time::Duration};

/// ANSI virtual key code of `w` (Shift+W opens the intelligence agency view).
const KEY_W: u16 = 13;

fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}

// ------------------------------------------------------------ window state

/// What the agency view shows (the only source for the created flag).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct AgencyGui {
    pub created: bool,
    pub creating: bool,
    pub creation_button_enabled: bool,
    /// `progressbar_desc` text while an upgrade runs.
    pub upgrading: Option<String>,
    /// An operative is being recruited (`operatives_progressbar` shown).
    pub recruiting: bool,
}

fn read_agency(r: &Reader<'_>, i: u64) -> Result<AgencyGui> {
    let root = pk::open_root(r, i, &pk::AGENCY_VIEW)?;
    let before = pk::need(r, i, root, "before_creation_panel", 2)?;
    let created = !gui_tree::shown(r, before)?;
    let (creating, creation_button_enabled) = match gui_tree::container(r, before, i)? {
        Some(panel) if !created => (
            pk::shown(r, i, panel, "creation_progressbar", 2)?,
            match pk::find(r, i, panel, "creation_button", 2)? {
                Some(button) => gui_tree::shown(r, button)? && !gui_tree::disabled(r, button)?,
                None => false,
            },
        ),
        _ => (false, false),
    };
    let upgrading = match pk::find(r, i, root, "upgrade_progressbar", 3)? {
        Some(bar) if gui_tree::shown(r, bar)? => match gui_tree::container(r, bar, i)? {
            Some(window) => Some(pk::text(r, i, window, "progressbar_desc", 2)?.unwrap_or_default()),
            None => Some(String::new()),
        },
        _ => None,
    };
    let recruiting = pk::shown(r, i, root, "operatives_progressbar", 3)?;
    Ok(AgencyGui {
        created,
        creating,
        creation_button_enabled,
        upgrading,
        recruiting,
    })
}

/// Refusal for `hoi4_agency_create`, from the window.
pub(super) fn create_refusal(g: &AgencyGui) -> Option<(&'static str, String)> {
    if g.created {
        return Some(("agency_exists", "The intelligence agency already exists".into()));
    }
    if g.creating {
        return Some((
            "agency_creation_in_progress",
            "The agency is already being created".into(),
        ));
    }
    if !g.creation_button_enabled {
        return Some((
            "agency_creation_unavailable",
            "The game disables Create Agency (civilian factories or the focus requirement are missing)".into(),
        ));
    }
    None
}
/// Refusal for the agency actions that need an existing agency.
pub(super) fn needs_agency_refusal(g: &AgencyGui) -> Option<(&'static str, String)> {
    if !g.created {
        return Some((
            "agency_not_created",
            "Create the intelligence agency first (hoi4_agency_create)".into(),
        ));
    }
    None
}
pub(super) fn upgrade_refusal(g: &AgencyGui) -> Option<(&'static str, String)> {
    needs_agency_refusal(g).or_else(|| {
        g.upgrading.as_ref().map(|d| {
            (
                "upgrade_in_progress",
                format!("An upgrade is already running ({d}); the agency runs one at a time"),
            )
        })
    })
}

fn refusal(r: Option<(&'static str, String)>) -> std::result::Result<(), Fail> {
    match r {
        Some((code, message)) => Err(refuse(code, message)),
        None => Ok(()),
    }
}

fn open_agency(process: &dyn Process, keys: &MacKeys<'_>) -> Result<()> {
    if pk::open_now(process, &pk::AGENCY_VIEW) {
        return Ok(());
    }
    keys.press_shifted(KEY_W)?;
    wait(process, "open intelligence agency view", |r, i| {
        pk::is_open(r, i, &pk::AGENCY_VIEW)
    })?;
    pause(400);
    Ok(())
}
/// Close a pop-up window with its close button (these windows have no
/// Escape shortcut in their `.gui` definition).
fn close_with(process: &dyn Process, keys: &MacKeys<'_>, class: &pk::Class, button: &str) {
    if !pk::open_now(process, class) {
        return;
    }
    let clicked = pk::click(process, keys, "close window", |r, i| {
        let root = pk::open_root(r, i, class)?;
        pk::rect_of(r, i, root, button, 4)
    });
    if clicked.is_ok() {
        let _ = wait(process, "window closed", |r, i| Ok(!pk::is_open(r, i, class)?));
    }
}
fn leave(process: &dyn Process, keys: &MacKeys<'_>) {
    close_with(process, keys, &pk::AGENCY_LOGO, "btn_close");
    close_with(process, keys, &pk::AGENCY_UPGRADES, "btn_close");
    if pk::open_now(process, &pk::OPERATIVE_RECRUIT) {
        super::ui::dismiss(process, keys, |r, i| Ok(!pk::is_open(r, i, &pk::OPERATIVE_RECRUIT)?));
    }
    pk::dismiss_window(process, keys, &pk::AGENCY_VIEW);
}

/// Agency read for the plan: the branch and upgrade keys from the loaded
/// database (memory).
fn live_branches(process: &dyn Process) -> Result<intelligence::Live> {
    read(process, |r, i| {
        let own = mgmt::own(r, i)?;
        intelligence::read_live(r, i, own.country)
    })
}

// ---------------------------------------------------------------- creation

fn create(process: &dyn Process, keys: &MacKeys<'_>) -> Driven {
    clear_notices(process, keys)?;
    let result: Driven = (|| {
        open_agency(process, keys)?;
        let g = read(process, read_agency)?;
        refusal(create_refusal(&g))?;
        pk::click(process, keys, "creation button", |r, i| {
            let root = pk::open_root(r, i, &pk::AGENCY_VIEW)?;
            let panel = pk::need_window(r, i, root, "before_creation_panel", 2)?;
            pk::rect_of(r, i, panel, "creation_button", 2)
        })?;
        wait(process, "open agency name window", |r, i| {
            pk::is_open(r, i, &pk::AGENCY_LOGO)
        })?;
        pause(300);
        let create_enabled = |r: &Reader<'_>, i: u64| -> Result<bool> {
            let root = pk::open_root(r, i, &pk::AGENCY_LOGO)?;
            let button = pk::need(r, i, root, "create_button", 3)?;
            Ok(gui_tree::shown(r, button)? && !gui_tree::disabled(r, button)?)
        };
        let mut picked_logo = false;
        if !read(process, create_enabled)? {
            // The default name stays; a logo must be chosen first.
            pk::click(process, keys, "agency logo", |r, i| {
                let root = pk::open_root(r, i, &pk::AGENCY_LOGO)?;
                let grid = pk::need(r, i, root, "item_grid", 4)?;
                let item = pk::items(r, i, grid)?
                    .into_iter()
                    .next()
                    .ok_or_else(|| Error::new("gui_target_unavailable", "No agency logos offered"))?;
                pk::rect_of(r, i, item.frame, "button", 2)
            })?;
            picked_logo = true;
            pause(250);
            if !read(process, create_enabled)? {
                return Err(refuse(
                    "agency_creation_unavailable",
                    "Create stayed disabled after choosing a logo; nothing was created",
                ));
            }
        }
        pk::click(process, keys, "create button", |r, i| {
            let root = pk::open_root(r, i, &pk::AGENCY_LOGO)?;
            pk::rect_of(r, i, root, "create_button", 3)
        })?;
        let _ = accept_command(process, keys);
        let started = wait(process, "agency creation started", |r, i| {
            let g = read_agency(r, i)?;
            Ok(g.creating || g.created)
        })
        .is_ok();
        Ok(json!({"creation_started": started, "picked_logo": picked_logo, "verification": "gui_only"}))
    })();
    leave(process, keys);
    result
}

// ----------------------------------------------------------------- upgrades

/// Branch/upgrade keys named in the request must exist in the loaded database.
pub(super) fn plan_upgrade(
    live: &intelligence::Live,
    branch: &str,
    upgrade: Option<&str>,
) -> Result<()> {
    let Some((_, upgrades)) = live.branches.iter().find(|(key, _)| key == branch) else {
        return Err(Error::new(
            "branch_unknown",
            format!(
                "No agency branch {branch:?}; branches: {}",
                live.branches
                    .iter()
                    .map(|b| b.0.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ));
    };
    if let Some(u) = upgrade {
        if !upgrades.iter().any(|k| k == u) {
            return Err(Error::new(
                "upgrade_unknown",
                format!("Branch {branch} has the upgrades {}", upgrades.join(", ")),
            ));
        }
    }
    Ok(())
}

type UpgradeRows = (Vec<(String, bool)>, Option<gui_tree::Rect>);

struct Names {
    branch: String,
    upgrade: Option<String>,
}

fn upgrade(process: &dyn Process, keys: &MacKeys<'_>, names: &Names) -> Driven {
    clear_notices(process, keys)?;
    let result: Driven = (|| {
        open_agency(process, keys)?;
        let g = read(process, read_agency)?;
        refusal(upgrade_refusal(&g))?;
        let branch = names.branch.clone();
        pk::click(process, keys, "branch button", |r, i| {
            let root = pk::open_root(r, i, &pk::AGENCY_VIEW)?;
            let grid = pk::need(r, i, root, "upgrades_list", 4)?;
            for item in pk::items(r, i, grid)? {
                let title = pk::text(r, i, item.frame, "agency_branches_title", 2)?.unwrap_or_default();
                if loc::same(&title, &branch) {
                    return pk::rect_of(r, i, item.frame, "background_button", 2);
                }
            }
            Err(Error::new(
                "gui_target_unavailable",
                format!("Branch {branch:?} is not in the agency view"),
            ))
        })?;
        wait(process, "open agency upgrades window", |r, i| {
            pk::is_open(r, i, &pk::AGENCY_UPGRADES)
        })?;
        pause(300);
        // (names with enabled flag, chosen rectangle) of the branch's entries.
        let wanted = names.upgrade.clone();
        let branch = names.branch.clone();
        let rows = |r: &Reader<'_>, i: u64| -> Result<UpgradeRows> {
            let root = pk::open_root(r, i, &pk::AGENCY_UPGRADES)?;
            let grid = pk::need(r, i, root, "branches_grid", 4)?;
            for entry in pk::items(r, i, grid)? {
                let title = pk::text(r, i, entry.frame, "upgrade_branch_name", 2)?.unwrap_or_default();
                if !loc::same(&title, &branch) {
                    continue;
                }
                let list = pk::need_window(r, i, entry.frame, "upgrades", 3)?;
                let upgrades = pk::need(r, i, list, "upgrades_grid", 3)?;
                let mut seen = Vec::new();
                let mut chosen = None;
                for item in pk::items(r, i, upgrades)? {
                    let name = pk::text(r, i, item.frame, "upgrade_name", 2)?.unwrap_or_default();
                    let button = pk::need(r, i, item.frame, "upgrade_button", 2)?;
                    let enabled = gui_tree::shown(r, button)? && !gui_tree::disabled(r, button)?;
                    let fits = match &wanted {
                        Some(w) => loc::same(&name, w),
                        None => enabled,
                    };
                    if chosen.is_none() && fits && enabled {
                        chosen = pk::rect(r, i, button).ok();
                    }
                    seen.push((name, enabled));
                }
                return Ok((seen, chosen));
            }
            Err(Error::new(
                "gui_target_unavailable",
                format!("Branch {branch:?} is not in the upgrades window"),
            ))
        };
        let (seen, chosen) = settled(process, "upgrade entries", |r, i| {
            let (s, c) = rows(r, i)?;
            Ok((s, c))
        })?;
        let Some(rect) = chosen else {
            return Err(refuse(
                "upgrade_unavailable",
                format!(
                    "No enabled matching upgrade; the branch shows {}",
                    seen.iter()
                        .map(|(n, e)| format!("{n}{}", if *e { "" } else { " (disabled)" }))
                        .collect::<Vec<_>>()
                        .join(" | ")
                ),
            ));
        };
        let (x, y) = rect.center();
        keys.click(pk::GAME, x, y)?;
        let confirmed = accept_command(process, keys)?;
        pause(400);
        close_with(process, keys, &pk::AGENCY_UPGRADES, "btn_close");
        let after = read(process, read_agency)?;
        let matches = match (&names.upgrade, &after.upgrading) {
            (Some(u), Some(text)) => Some(loc::same(text, u) || text.to_lowercase().contains(&u.to_lowercase())),
            _ => None,
        };
        Ok(json!({
            "upgrade_started": after.upgrading.is_some(),
            "progress_text": after.upgrading,
            "matches_requested": matches,
            "confirmation_accepted": confirmed,
            "verification": "gui_only",
        }))
    })();
    leave(process, keys);
    result
}

// ---------------------------------------------------------------- operatives

/// One entry of the operative bottom bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Slot {
    pub frame: u64,
    pub recruited: bool,
    pub on_mission: bool,
}

fn bar_slots(r: &Reader<'_>, i: u64) -> Result<Vec<Slot>> {
    let root = pk::open_root(r, i, &pk::OPERATIVE_BAR)?;
    let grid = pk::need(r, i, root, "operatives_box", 4)?;
    let mut out = Vec::new();
    for item in pk::items(r, i, grid)? {
        // A recruited operative's portrait carries a skill level text.
        let recruited = pk::find(r, i, item.frame, "operative_skill_level", 2)?.is_some();
        let on_mission = recruited && pk::shown(r, i, item.frame, "on_mission_window", 2)?;
        out.push(Slot {
            frame: item.frame,
            recruited,
            on_mission,
        });
    }
    Ok(out)
}
pub(super) fn recruited(slots: &[Slot]) -> usize {
    slots.iter().filter(|s| s.recruited).count()
}

/// Refusal for `hoi4_operative_recruit` once the bar is read.
pub(super) fn recruit_refusal(slots: &[Slot]) -> Option<(&'static str, String)> {
    if !slots.iter().any(|s| !s.recruited) {
        return Some((
            "no_free_operative_slot",
            format!(
                "The operative bar shows {} operatives and no empty slot (agency upgrades add slots)",
                recruited(slots)
            ),
        ));
    }
    None
}

fn recruit(process: &dyn Process, keys: &MacKeys<'_>, index: usize) -> Driven {
    clear_notices(process, keys)?;
    let result: Driven = (|| {
        open_agency(process, keys)?;
        let g = read(process, read_agency)?;
        refusal(needs_agency_refusal(&g))?;
        if !pk::open_now(process, &pk::OPERATIVE_BAR) {
            return Err(refuse(
                "operative_bar_unavailable",
                "The operative bar is not shown with the agency view; nothing was clicked",
            ));
        }
        let slots = read(process, bar_slots)?;
        refusal(recruit_refusal(&slots))?;
        let before = recruited(&slots);
        pk::click(process, keys, "empty operative slot", |r, i| {
            let slot = bar_slots(r, i)?
                .into_iter()
                .find(|s| !s.recruited)
                .ok_or_else(|| Error::new("gui_target_unavailable", "No empty operative slot"))?;
            pk::rect_of(r, i, slot.frame, "portrait_btn", 2)
        })?;
        wait(process, "open operative recruitment window", |r, i| {
            pk::is_open(r, i, &pk::OPERATIVE_RECRUIT)
        })?;
        pause(300);
        let candidates = |r: &Reader<'_>, i: u64| -> Result<Vec<gui_tree::Rect>> {
            let root = pk::open_root(r, i, &pk::OPERATIVE_RECRUIT)?;
            let mut grids = Vec::new();
            pk::grids(r, i, root, 6, &mut grids)?;
            let mut out = Vec::new();
            for grid in grids {
                for item in pk::items(r, i, grid)? {
                    if let Some(button) = pk::find(r, i, item.frame, "portrait_btn", 2)? {
                        if let Ok(rect) = pk::rect(r, i, button) {
                            out.push(rect);
                        }
                    }
                }
            }
            Ok(out)
        };
        let offered = settled(process, "operative candidates", candidates)?;
        let Some(rect) = offered.get(index) else {
            return Err(refuse(
                "unknown_candidate",
                format!(
                    "The recruitment window offers {} operatives, not {}; nothing was recruited",
                    offered.len(),
                    index + 1
                ),
            ));
        };
        let (x, y) = rect.center();
        keys.click(pk::GAME, x, y)?;
        let confirmed = accept_command(process, keys)?;
        pause(500);
        let started = wait(process, "operative recruitment started", |r, i| {
            let recruiting = read_agency(r, i)?.recruiting;
            let now = recruited(&bar_slots(r, i).unwrap_or_default());
            Ok(recruiting || now > before)
        })
        .is_ok();
        Ok(json!({
            "recruit_started": started,
            "candidates_offered": offered.len(),
            "confirmation_accepted": confirmed,
            "verification": "gui_only",
        }))
    })();
    leave(process, keys);
    result
}

/// Mission table: (key, order bar button number, how the target is chosen).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Target {
    State,
    Country,
    /// Needs an ideology choice window that is not driven yet.
    Ideology,
}
pub(super) const MISSIONS: [(&str, u8, Target); 8] = [
    ("build_intel_network", 1, Target::State),
    ("quiet_intel_network", 2, Target::State),
    ("counter_intelligence", 3, Target::Country),
    ("root_out_resistance", 4, Target::State),
    ("boost_ideology", 5, Target::Ideology),
    ("control_trade", 6, Target::Country),
    ("diplomatic_pressure", 7, Target::Country),
    ("propaganda", 8, Target::Country),
];

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Aim {
    State(i32),
    Country(String),
    /// Counter intelligence at home: a right click starts it.
    Home,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct AssignPlan {
    pub operative: usize,
    pub mission: &'static str,
    pub button: u8,
    pub aim: Aim,
}

pub(super) fn plan_assign(
    operative: i64,
    mission: &str,
    state_id: Option<i64>,
    country: Option<&str>,
) -> Result<AssignPlan> {
    if !(0..=31).contains(&operative) {
        return Err(Error::new("invalid_arguments", "operative must be a bar position 0..31"));
    }
    let Some(&(key, button, kind)) = MISSIONS.iter().find(|m| m.0 == mission) else {
        return Err(Error::new(
            "invalid_arguments",
            format!(
                "mission must be one of {}",
                MISSIONS.iter().map(|m| m.0).collect::<Vec<_>>().join(", ")
            ),
        ));
    };
    let aim = match kind {
        Target::Ideology => {
            return Err(Error::new(
                "mission_not_built",
                "boost_ideology needs the ideology choice window, which this tool does not drive yet",
            ))
        }
        Target::State => {
            let state = state_id
                .filter(|s| (1..=4095).contains(s))
                .ok_or_else(|| {
                    Error::new("target_required", format!("{key} needs state_id (a state of the target network)"))
                })?;
            if country.is_some() {
                return Err(Error::new("invalid_arguments", format!("{key} takes state_id, not country")));
            }
            Aim::State(state as i32)
        }
        Target::Country => {
            if state_id.is_some() {
                return Err(Error::new("invalid_arguments", format!("{key} takes country, not state_id")));
            }
            match country {
                Some(tag) if tag.len() == 3 && tag.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) => {
                    Aim::Country(tag.to_string())
                }
                Some(_) => return Err(Error::new("invalid_arguments", "country must be a three-letter tag")),
                None if key == "counter_intelligence" => Aim::Home,
                None => {
                    return Err(Error::new("target_required", format!("{key} needs country (a tag such as ITA)")))
                }
            }
        }
    };
    Ok(AssignPlan {
        operative: operative as usize,
        mission: key,
        button,
        aim,
    })
}

/// A state of the target country to centre the camera on (lowest id).
fn target_state(process: &dyn Process, tag: &str) -> Result<i32> {
    let states = read(process, |r, i| diplomacy_state::states_owned_by(r, i, tag, 4))?;
    states.first().copied().ok_or_else(|| {
        Error::new("country_has_no_states", format!("{tag} owns no state to click on"))
    })
}

fn assign(process: &dyn Process, keys: &MacKeys<'_>, plan: &AssignPlan, state: Option<i32>) -> Driven {
    clear_notices(process, keys)?;
    let result: Driven = (|| {
        if let Some(state) = state {
            find_state(process, keys, state)?;
        }
        open_agency(process, keys)?;
        let g = read(process, read_agency)?;
        refusal(needs_agency_refusal(&g))?;
        if !pk::open_now(process, &pk::OPERATIVE_BAR) {
            return Err(refuse(
                "operative_bar_unavailable",
                "The operative bar is not shown with the agency view; nothing was clicked",
            ));
        }
        let slots = read(process, bar_slots)?;
        let nth = slots.iter().filter(|s| s.recruited).nth(plan.operative);
        let Some(slot) = nth else {
            return Err(refuse(
                "operative_unknown",
                format!(
                    "The bar shows {} recruited operatives (positions 0..), not position {}",
                    recruited(&slots),
                    plan.operative
                ),
            ));
        };
        if slot.on_mission {
            return Err(refuse(
                "operative_busy",
                "That operative is already on a mission or operation",
            ));
        }
        let position = plan.operative;
        pk::click(process, keys, "operative portrait", |r, i| {
            let slot = bar_slots(r, i)?
                .into_iter()
                .filter(|s| s.recruited)
                .nth(position)
                .ok_or_else(|| Error::new("gui_target_unavailable", "Operative left the bar"))?;
            pk::rect_of(r, i, slot.frame, "portrait_btn", 2)
        })?;
        wait(process, "open operative order bar", |r, i| {
            pk::is_open(r, i, &pk::OPERATIVE_ORDERS)
        })?;
        pause(300);
        let button = format!("btn_mission_{}", plan.button);
        let enabled = read(process, |r, i| {
            let root = pk::open_root(r, i, &pk::OPERATIVE_ORDERS)?;
            let b = pk::need(r, i, root, &button, 3)?;
            Ok(gui_tree::shown(r, b)? && !gui_tree::disabled(r, b)?)
        })?;
        if !enabled {
            return Err(refuse(
                "mission_unavailable",
                format!("The game disables the {} button for this operative", plan.mission),
            ));
        }
        pk::click(process, keys, "mission button", |r, i| {
            let root = pk::open_root(r, i, &pk::OPERATIVE_ORDERS)?;
            pk::rect_of(r, i, root, &button, 3)
        })?;
        pause(350);
        // The map now highlights targets; the camera is centred on the target
        // state, so the screen centre is the click.
        match plan.aim {
            Aim::Home => {
                keys.right_click(pk::GAME, 960.0, 540.0)?;
            }
            _ => {
                keys.click(pk::GAME, 960.0, 540.0)?;
            }
        }
        let confirmed = accept_command(process, keys)?;
        pause(500);
        let on_mission = read(process, |r, i| {
            Ok(bar_slots(r, i)?
                .into_iter()
                .filter(|s| s.recruited)
                .nth(position)
                .is_some_and(|s| s.on_mission))
        })
        .unwrap_or(false);
        Ok(json!({
            "on_mission": on_mission,
            "mission": plan.mission,
            "confirmation_accepted": confirmed,
            "verification": "gui_only",
        }))
    })();
    leave(process, keys);
    result
}

// ------------------------------------------------------------------- tools

pub(super) fn call(
    name: &str,
    args: &serde_json::Map<String, Value>,
    process: &(dyn Process + Send + Sync),
    keys: &MacKeys<'static>,
    guard: &mut Guard,
    authority: &InputAuthority,
    actions: &mut super::journal::GameJournal,
) -> Result<Value> {
    let id = super::string(args, "request_id")?;
    let token = super::string(args, "lease_token")?;
    match name {
        "hoi4_agency_create" => {
            super::allowed(args, &["request_id", "lease_token"])?;
            let job = Job {
                kind: "agency_create",
                action: json!({"kind":"agency_create"}),
                plan: Box::new(|process| {
                    // The agency object must exist; whether it was created is
                    // only on screen, so the window refuses after it opens.
                    live_branches(process).map(|_| ())
                }),
                dispatch: Box::new(|process, keys| create(process, keys)),
                settle: Box::new(|_, seen| {
                    Ok(ui_action::effect(seen, "creation_started", json!({"tool":"agency_create"})))
                }),
            };
            ui_action::run(job, id, token, process, keys, guard, authority, actions)
        }
        "hoi4_agency_upgrade" => {
            super::allowed(args, &["branch", "upgrade", "request_id", "lease_token"])?;
            let branch = super::string(args, "branch")?;
            let upgrade_key = match args.get("upgrade") {
                None => None,
                Some(v) => Some(
                    v.as_str()
                        .ok_or_else(|| Error::new("invalid_arguments", "upgrade must be a string"))?,
                ),
            };
            for key in std::iter::once(branch).chain(upgrade_key) {
                if key.is_empty() || key.len() > 96 || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                    return Err(Error::new("invalid_arguments", "Invalid branch or upgrade key"));
                }
            }
            let names: RefCell<Option<Names>> = RefCell::new(None);
            let job = Job {
                kind: "agency_upgrade",
                action: json!({"kind":"agency_upgrade","branch":branch,"upgrade":upgrade_key}),
                plan: Box::new(|process| {
                    let live = live_branches(process)?;
                    plan_upgrade(&live, branch, upgrade_key)?;
                    let dir = mgmt::game_dir(process)?;
                    let mut keys_wanted = vec![branch];
                    keys_wanted.extend(upgrade_key);
                    let names_map = loc::lookup(&dir, &keys_wanted);
                    let branch_name = names_map.get(branch).cloned().ok_or_else(|| {
                        Error::new("display_name_unknown", format!("No English name for {branch}"))
                    })?;
                    let upgrade_name = match upgrade_key {
                        Some(u) => Some(names_map.get(u).cloned().ok_or_else(|| {
                            Error::new("display_name_unknown", format!("No English name for {u}"))
                        })?),
                        None => None,
                    };
                    *names.borrow_mut() = Some(Names {
                        branch: branch_name,
                        upgrade: upgrade_name,
                    });
                    Ok(())
                }),
                dispatch: Box::new(|process, keys| {
                    let held = names.borrow();
                    let n = held
                        .as_ref()
                        .ok_or_else(|| Error::new("action_uncertain", "Plan missing at dispatch"))?;
                    upgrade(process, keys, n)
                }),
                settle: Box::new(|_, seen| {
                    Ok(ui_action::effect(seen, "upgrade_started", json!({"tool":"agency_upgrade","branch":branch,"upgrade":upgrade_key})))
                }),
            };
            ui_action::run(job, id, token, process, keys, guard, authority, actions)
        }
        "hoi4_operative_recruit" => {
            super::allowed(args, &["candidate", "request_id", "lease_token"])?;
            let index = match args.get("candidate") {
                None => 0,
                Some(v) => v
                    .as_u64()
                    .filter(|n| *n <= 31)
                    .ok_or_else(|| Error::new("invalid_arguments", "candidate must be 0..31"))? as usize,
            };
            let job = Job {
                kind: "operative_recruit",
                action: json!({"kind":"operative_recruit","candidate":index}),
                plan: Box::new(|_| Ok(())),
                dispatch: Box::new(|process, keys| recruit(process, keys, index)),
                settle: Box::new(|_, seen| {
                    Ok(ui_action::effect(seen, "recruit_started", json!({"tool":"operative_recruit"})))
                }),
            };
            ui_action::run(job, id, token, process, keys, guard, authority, actions)
        }
        "hoi4_operative_assign" => {
            super::allowed(
                args,
                &["operative", "mission", "operation", "state_id", "country", "request_id", "lease_token"],
            )?;
            if args.contains_key("operation") {
                return Err(Error::new(
                    "operation_not_built",
                    "Launching operations (operation overview window, operative slots, target) is not driven yet; use mission",
                ));
            }
            let operative = args
                .get("operative")
                .and_then(Value::as_i64)
                .ok_or_else(|| Error::new("invalid_arguments", "operative must be an integer bar position"))?;
            let mission = super::string(args, "mission")?;
            let state_id = match args.get("state_id") {
                None => None,
                Some(v) => Some(
                    v.as_i64()
                        .ok_or_else(|| Error::new("invalid_arguments", "state_id must be an integer"))?,
                ),
            };
            let country = match args.get("country") {
                None => None,
                Some(v) => Some(
                    v.as_str()
                        .ok_or_else(|| Error::new("invalid_arguments", "country must be a string"))?,
                ),
            };
            let plan = plan_assign(operative, mission, state_id, country)?;
            let chosen: RefCell<Option<i32>> = RefCell::new(None);
            let job = Job {
                kind: "operative_assign",
                action: json!({"kind":"operative_assign","operative":operative,"mission":mission,"state_id":state_id,"country":country}),
                plan: Box::new(|process| {
                    if let Aim::Country(tag) = &plan.aim {
                        *chosen.borrow_mut() = Some(target_state(process, tag)?);
                    }
                    if let Aim::State(s) = plan.aim {
                        *chosen.borrow_mut() = Some(s);
                    }
                    Ok(())
                }),
                dispatch: Box::new(|process, keys| assign(process, keys, &plan, *chosen.borrow())),
                settle: Box::new(|_, seen| {
                    Ok(ui_action::effect(seen, "on_mission", json!({"tool":"operative_assign","mission":plan.mission})))
                }),
            };
            ui_action::run(job, id, token, process, keys, guard, authority, actions)
        }
        _ => Err(Error::new("capability_unavailable", "Unknown agency tool")),
    }
}

pub(super) const TOOLS: [&str; 4] = [
    "hoi4_agency_create",
    "hoi4_agency_upgrade",
    "hoi4_operative_recruit",
    "hoi4_operative_assign",
];

pub(super) fn tools() -> Vec<Value> {
    vec![
        json!({"name":"hoi4_agency_create","description":"UNVALIDATED (no live test yet): create the intelligence agency with ordinary input: Shift+W agency view, Create Agency, the name/logo window (default name kept, the first logo chosen when Create is disabled), Create. Whether the agency exists is only on screen, so the refusals come from the opened window before the click: agency_exists, agency_creation_in_progress, agency_creation_unavailable (the game disables the button). Verified from the window only (the creation progress bar appears): verification gui_only, results carry validated:false. Creation takes civilian factories for the define's number of days.","inputSchema":{"type":"object","properties":{"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_agency_upgrade","description":"UNVALIDATED (no live test yet): start an agency upgrade: Shift+W agency view, the branch button, the upgrade entry in the upgrades window (the named `upgrade`, else the first enabled one). Branch and upgrade keys are those of hoi4_intelligence (branch_intelligence, upgrade_army_department, ...). Typed refusals: branch_unknown, upgrade_unknown (before input, from the loaded database); agency_not_created, upgrade_in_progress, upgrade_unavailable (lists the branch's entries with their enabled state), found from the windows before the click. Verified from the agency view's upgrade progress bar only (verification gui_only). Results carry validated:false.","inputSchema":{"type":"object","properties":{"branch":{"type":"string","maxLength":96},"upgrade":{"type":"string","maxLength":96},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["branch","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_operative_recruit","description":"UNVALIDATED (no live test yet): recruit an operative into a free slot of the operative bar: Shift+W agency view, the empty slot, the candidate at position `candidate` (0-based, default 0) of the recruitment window (names and traits are not readable, so the choice is by position). Typed refusals found from the windows before the click: agency_not_created, operative_bar_unavailable, no_free_operative_slot, unknown_candidate. Verified from the bar and the recruitment progress bar only (verification gui_only). Results carry validated:false.","inputSchema":{"type":"object","properties":{"candidate":{"type":"integer","minimum":0,"maximum":31},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_operative_assign","description":"UNVALIDATED (no live test yet): send a recruited operative (`operative` = 0-based position among the recruited ones on the operative bar) on a mission: portrait, the order bar's mission button, then one click on the target. State missions (build_intel_network, quiet_intel_network, root_out_resistance) take `state_id`; country missions (counter_intelligence, control_trade, diplomatic_pressure, propaganda) take `country` (a tag; the Find view centres its lowest state id and that click picks the country; counter_intelligence without country starts at home with a right click). boost_ideology (needs the ideology window) and `operation` (operation overview) are not built and refuse with mission_not_built / operation_not_built. Typed refusals: invalid_arguments, target_required, country_has_no_states, unknown_country (before input); agency_not_created, operative_bar_unavailable, operative_unknown, operative_busy, mission_unavailable (the game disables the button) from the windows before the first state-changing click. Verified from the operative's on-mission marker on the bar only (verification gui_only). Results carry validated:false.","inputSchema":{"type":"object","properties":{"operative":{"type":"integer","minimum":0,"maximum":31},"mission":{"type":"string"},"state_id":{"type":"integer","minimum":1,"maximum":4095},"country":{"type":"string","minLength":3,"maxLength":3},"operation":{"type":"string"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["operative","request_id","lease_token"],"additionalProperties":false}}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gui(created: bool, creating: bool, enabled: bool, upgrading: Option<&str>) -> AgencyGui {
        AgencyGui {
            created,
            creating,
            creation_button_enabled: enabled,
            upgrading: upgrading.map(str::to_string),
            recruiting: false,
        }
    }

    #[test]
    fn create_refuses_from_the_window_state() {
        assert_eq!(create_refusal(&gui(true, false, false, None)).unwrap().0, "agency_exists");
        assert_eq!(
            create_refusal(&gui(false, true, false, None)).unwrap().0,
            "agency_creation_in_progress"
        );
        assert_eq!(
            create_refusal(&gui(false, false, false, None)).unwrap().0,
            "agency_creation_unavailable"
        );
        assert!(create_refusal(&gui(false, false, true, None)).is_none());
    }

    #[test]
    fn upgrade_needs_an_agency_and_no_running_upgrade() {
        assert_eq!(upgrade_refusal(&gui(false, false, true, None)).unwrap().0, "agency_not_created");
        assert_eq!(
            upgrade_refusal(&gui(true, false, false, Some("Army department"))).unwrap().0,
            "upgrade_in_progress"
        );
        assert!(upgrade_refusal(&gui(true, false, false, None)).is_none());
    }

    #[test]
    fn upgrade_keys_are_checked_against_the_loaded_database() {
        let live = intelligence::Live {
            agency_name: Some("Abwehr".into()),
            branches: vec![(
                "branch_intelligence".into(),
                vec!["upgrade_economy_civilian".into(), "upgrade_army_department".into()],
            )],
        };
        assert!(plan_upgrade(&live, "branch_intelligence", None).is_ok());
        assert!(plan_upgrade(&live, "branch_intelligence", Some("upgrade_army_department")).is_ok());
        assert_eq!(plan_upgrade(&live, "branch_x", None).unwrap_err().code, "branch_unknown");
        assert_eq!(
            plan_upgrade(&live, "branch_intelligence", Some("nope")).unwrap_err().code,
            "upgrade_unknown"
        );
    }

    #[test]
    fn mission_plans_follow_the_target_kind() {
        let p = plan_assign(0, "build_intel_network", Some(64), None).unwrap();
        assert_eq!((p.button, p.aim), (1, Aim::State(64)));
        let p = plan_assign(2, "propaganda", None, Some("ITA")).unwrap();
        assert_eq!((p.button, p.aim, p.operative), (8, Aim::Country("ITA".into()), 2));
        assert_eq!(plan_assign(0, "counter_intelligence", None, None).unwrap().aim, Aim::Home);
        assert_eq!(plan_assign(0, "root_out_resistance", None, None).unwrap_err().code, "target_required");
        assert_eq!(plan_assign(0, "control_trade", None, None).unwrap_err().code, "target_required");
        assert_eq!(plan_assign(0, "control_trade", Some(3), Some("ITA")).unwrap_err().code, "invalid_arguments");
        assert_eq!(plan_assign(0, "propaganda", None, Some("it")).unwrap_err().code, "invalid_arguments");
        assert_eq!(plan_assign(0, "boost_ideology", None, Some("ITA")).unwrap_err().code, "mission_not_built");
        assert_eq!(plan_assign(0, "sabotage", None, None).unwrap_err().code, "invalid_arguments");
        assert_eq!(plan_assign(99, "propaganda", None, Some("ITA")).unwrap_err().code, "invalid_arguments");
        // The table covers the eight mission classes of hoi4_intelligence.
        for key in intelligence::MISSIONS {
            assert!(MISSIONS.iter().any(|m| m.0 == *key), "{key}");
        }
    }

    #[test]
    fn recruit_needs_a_free_slot() {
        let slot = |recruited| Slot { frame: 1, recruited, on_mission: false };
        assert_eq!(recruit_refusal(&[slot(true), slot(true)]).unwrap().0, "no_free_operative_slot");
        assert!(recruit_refusal(&[slot(true), slot(false)]).is_none());
        assert_eq!(recruited(&[slot(true), slot(false), slot(true)]), 2);
    }

    #[test]
    fn descriptors_are_unvalidated_and_lease_gated() {
        for t in tools() {
            assert!(t["description"].as_str().unwrap().starts_with("UNVALIDATED (no live test yet):"));
            assert!(t["inputSchema"]["required"].as_array().unwrap().iter().any(|v| v == "lease_token"));
            assert!(TOOLS.contains(&t["name"].as_str().unwrap()));
        }
        assert_eq!(tools().len(), TOOLS.len());
    }
}
