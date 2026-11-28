//! Intelligence agency actions with ordinary input: `hoi4_agency_create`,
//! `hoi4_agency_upgrade`, `hoi4_operative_recruit`, `hoi4_operative_assign`.
//! Live-verified on macOS 1.19.4.1 (Italy 1936, November 2026).
//!
//! The agency (created flag, upgrades, candidates, operatives, missions) is
//! read from game memory (`intel_live`). Every refusal that needs that state
//! is decided from a fresh memory read before any input, and every success is
//! read back from memory (`verification: "memory"`). Windows are only used to
//! carry the clicks.
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
    diplomacy_state, intel_live, intelligence,
    journal::Outcome,
    loc, mgmt, panelkit as pk,
    ui::{accept_command, clear_notices, find_state, read, settled, wait},
    ui_action::{self, refuse, Driven, Fail, Job},
    Guard, InputAuthority,
};
use crate::{
    error::{Error, Result},
    gui_tree,
    memory::Reader,
    platform::{game_input::GameInput, Process},
    xlayout::{fld, Class},
};
use serde_json::{json, Value};
use std::{cell::RefCell, time::Duration};

/// ANSI virtual key code of `w` (Shift+W opens the intelligence agency view).
const KEY_W: u16 = 13;

#[track_caller]
fn pause(ms: u64) {
    crate::phase::sleep(Duration::from_millis(ms));
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
    // The creation progress bar counts as shown under the Create Agency button even when
    // nothing runs (live 6 Nov 2026: 'already being created' with the button on screen), so a
    // creation is in progress only while the bar is shown and the button cannot be pressed.
    let (creating, creation_button_enabled) = match gui_tree::container(r, before, i)? {
        Some(panel) if !created => {
            let button = pk::find(r, i, panel, "creation_button", 2)?;
            let button_shown = match button {
                Some(b) => gui_tree::shown(r, b)?,
                None => false,
            };
            let enabled = match button {
                Some(b) => button_shown && !gui_tree::disabled(r, b)?,
                None => false,
            };
            // While the agency forms ('Agency being formed... N days') the button stays on
            // screen but disabled.
            // The cancel button (the X on the bar) is shown only while it forms.
            let cancel = pk::shown(r, i, panel, "cancel_creation_button", 2).unwrap_or(false);
            (
                cancel || (pk::shown(r, i, panel, "creation_progressbar", 2)? && !button_shown),
                enabled,
            )
        }
        _ => (false, false),
    };
    // The progress bars stay shown when idle, with STATUS_NO_AGENCY_UPGRADE /
    // STATUS_NO_OPERATIVE_RECRUITED as their text (seen live 1 November 2026).
    let progress = |name: &str, idle: &str| -> Result<Option<String>> {
        Ok(match pk::find(r, i, root, name, 3)? {
            Some(bar) if gui_tree::shown(r, bar)? => match gui_tree::container(r, bar, i)? {
                Some(window) => {
                    let text = pk::text(r, i, window, "progressbar_desc", 2)?.unwrap_or_default();
                    // A running bar counts days ('18 days'); a slot waiting for a pick shows
                    // 'Ready!' there with the description still 'Recruiting operative'.
                    let days = pk::text(r, i, window, "progressbar_days", 2)?.unwrap_or_default();
                    let counting = days.chars().any(|c| c.is_ascii_digit());
                    (counting && !idle_text(&text, idle)).then_some(text)
                }
                None => Some(String::new()),
            },
            _ => None,
        })
    };
    let upgrading = progress("upgrade_progressbar", "No upgrade in progress.")?;
    let recruiting = progress("operatives_progressbar", "No operative being recruited.")?.is_some();
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
        return Some((
            "agency_exists",
            "The intelligence agency already exists".into(),
        ));
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
/// A progress text that says nothing is running (English localisation; the run uses l_english).
fn idle_text(text: &str, idle: &str) -> bool {
    let t = text.trim();
    // 'Ready!' on the recruiting bar means a slot waits for a pick, not a recruitment running.
    t.is_empty()
        || t.eq_ignore_ascii_case(idle)
        || t.to_ascii_lowercase().starts_with("no ")
        || t.trim_end_matches('!').eq_ignore_ascii_case("ready")
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

/// The game queues an upgrade that needs more civilian factories than are free for projects
/// instead of running it.
pub(super) fn factories_refusal(need: f64, free: Option<f64>) -> Option<(&'static str, String)> {
    let free = free?;
    (free < need).then(|| {
        (
            "upgrade_needs_factories",
            format!(
                "The upgrade uses {need:.0} civilian factories while it runs and {free:.0} are free for projects: the game would only queue it"
            ),
        )
    })
}

fn refusal(r: Option<(&'static str, String)>) -> std::result::Result<(), Fail> {
    match r {
        Some((code, message)) => Err(refuse(code, message)),
        None => Ok(()),
    }
}

/// Whether the top bar shows its `intel_agency_button` (interface manager +0x670, top bar view
/// +0x188, root +0x68). The game hides it when the agency feature is not active (La Resistance
/// not enabled: seen live on 1.19.4.1, 31 October 2026, where Shift+W then opens nothing).
fn agency_button_shown(r: &Reader<'_>, i: u64) -> Result<Option<bool>> {
    const IDLER: Class = Class::new("CInGameIdler", 0x35089d8);
    const MANAGER: Class = Class::new("idler.manager", 0);
    const TOP_BAR: Class = Class::new("CTopBar", 0x3697918);
    let abi = r.string_abi();
    let idler = r.u64(i + crate::game::layout(r).idler_global)?;
    let manager = r.u64(idler + fld(abi, &IDLER, 0x670)?)?;
    let topbar = r.u64(manager + fld(abi, &MANAGER, 0x188)?)?;
    let root = r.u64(topbar + fld(abi, &TOP_BAR, 0x68)?)?;
    let Some(root) = gui_tree::container(r, root, i)? else {
        return Ok(None);
    };
    match pk::find(r, i, root, "intel_agency_button", 4)? {
        Some(button) => Ok(Some(gui_tree::shown(r, button)?)),
        None => Ok(None),
    }
}

fn open_agency(process: &dyn Process, keys: &dyn GameInput) -> Result<()> {
    if pk::open_now(process, &pk::AGENCY_VIEW) {
        return Ok(());
    }
    if read(process, agency_button_shown).ok().flatten() == Some(false) {
        return Err(Error::new(
            "agency_unavailable",
            "The game hides the intelligence agency button (the agency needs the La Resistance DLC, not active in this game); nothing was sent",
        ));
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
fn close_with(process: &dyn Process, keys: &dyn GameInput, class: &pk::Class, button: &str) {
    if !pk::open_now(process, class) {
        return;
    }
    let clicked = pk::click(process, keys, "close window", |r, i| {
        let root = pk::open_root(r, i, class)?;
        pk::rect_of(r, i, root, button, 4)
    });
    if clicked.is_ok() {
        let _ = wait(process, "window closed", |r, i| {
            Ok(!pk::is_open(r, i, class)?)
        });
    }
}
fn leave(process: &dyn Process, keys: &dyn GameInput) {
    close_with(process, keys, &pk::AGENCY_LOGO, "btn_close");
    close_with(process, keys, &pk::AGENCY_UPGRADES, "btn_close");
    if pk::open_now(process, &pk::OPERATIVE_RECRUIT) {
        super::ui::dismiss(process, keys, |r, i| {
            Ok(!pk::is_open(r, i, &pk::OPERATIVE_RECRUIT)?)
        });
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

/// The agency as memory holds it (created, upgrades, operatives, slots).
fn mem(process: &dyn Process) -> Result<intel_live::Agency> {
    read(process, |r, i| {
        let own = mgmt::own(r, i)?;
        intel_live::agency(r, i, own.country)
    })
}

fn no(code: &'static str, message: impl Into<String>) -> Error {
    Error::new(code, message)
}

/// Refusals of `hoi4_agency_create` from memory, before any input.
pub(super) fn create_check(a: &intel_live::Agency) -> Result<()> {
    if a.created {
        return Err(no(
            "agency_exists",
            "The intelligence agency already exists",
        ));
    }
    if a.creating {
        return Err(no(
            "agency_creation_in_progress",
            format!(
                "The agency is being created ({:.0} days done)",
                a.progress_days
            ),
        ));
    }
    Ok(())
}

/// Refusals of `hoi4_agency_upgrade` from memory: the agency, the running upgrade, a maxed
/// upgrade and missing prerequisite upgrades (levels and prerequisites from the scripts).
pub(super) fn upgrade_check(
    a: &intel_live::Agency,
    upgrade: Option<&str>,
    levels: Option<usize>,
    requires: &[String],
) -> Result<()> {
    if !a.created {
        return Err(no(
            "agency_not_created",
            "Create the intelligence agency first (hoi4_agency_create)",
        ));
    }
    if let Some(running) = &a.upgrading {
        return Err(no(
            "upgrade_in_progress",
            format!(
                "{running} is running ({:.0} days done); the agency runs one upgrade at a time",
                a.progress_days
            ),
        ));
    }
    if let Some(key) = upgrade {
        let have = a
            .upgrades
            .iter()
            .find(|(k, _)| k == key)
            .map_or(0, |(_, l)| *l);
        if levels.is_some_and(|max| have as usize >= max) {
            return Err(no(
                "upgrade_maxed",
                format!("{key} is at its last level ({have})"),
            ));
        }
        let missing: Vec<&String> = requires
            .iter()
            .filter(|r| !a.upgrades.iter().any(|(k, l)| k == *r && *l > 0))
            .collect();
        if !missing.is_empty() {
            return Err(no(
                "upgrade_locked",
                format!(
                    "{key} needs {} first",
                    missing
                        .iter()
                        .map(|m| m.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
    }
    Ok(())
}

/// Refusals of `hoi4_operative_recruit` from memory: no agency, every slot filled, a new slot
/// still waiting for its candidates (`wait_days` from the agency defines), a candidate beyond
/// the offered ones.
pub(super) fn recruit_check(
    a: &intel_live::Agency,
    candidate: usize,
    wait_days: i32,
) -> Result<()> {
    if !a.created {
        return Err(no(
            "agency_not_created",
            "Create the intelligence agency first (hoi4_agency_create)",
        ));
    }
    if a.operatives.len() as i32 >= a.slots {
        return Err(no(
            "no_free_operative_slot",
            format!(
                "{} of {} operative slots are filled; five agency upgrades open one more, some advisors, spirits and the spymaster others",
                a.operatives.len(),
                a.slots
            ),
        ));
    }
    if a.candidates.is_empty() {
        let left = (wait_days - a.slot_wait_days).max(0);
        return Err(no(
            "operative_slot_locked",
            format!("The new operative slot offers candidates in {left} days (waited {} of {wait_days})", a.slot_wait_days),
        ));
    }
    if candidate >= a.candidates.len() {
        return Err(no(
            "unknown_candidate",
            format!(
                "{} candidates are offered ({}), not position {candidate}",
                a.candidates.len(),
                a.candidates.join(", ")
            ),
        ));
    }
    Ok(())
}

/// Refusals of `hoi4_operative_assign` from memory.
/// `replace`: the caller means to end the operative's running mission (live 115, 28 Nov 2026: a
/// running mission was replaced silently). Without it, an operative on any mission other than the
/// starting counter-intelligence at home is refused with mission_running, naming the mission.
pub(super) fn assign_check(a: &intel_live::Agency, plan: &AssignPlan, replace: bool) -> Result<()> {
    if !a.created {
        return Err(no(
            "agency_not_created",
            "Create the intelligence agency first (hoi4_agency_create)",
        ));
    }
    let Some(o) = a.operatives.get(plan.operative) else {
        return Err(no(
            "operative_unknown",
            format!(
                "{} operatives are recruited (positions 0..), not position {}",
                a.operatives.len(),
                plan.operative
            ),
        ));
    };
    if let Some(op) = &o.operation {
        return Err(no(
            "operative_busy",
            format!(
                "{} works on {op}; it is free again when the operation ends",
                o.name
            ),
        ));
    }
    let same_target = match &plan.aim {
        Aim::State(s) => o.mission_state == Some(*s),
        Aim::Country(t) => o.mission_target.as_deref() == Some(t.as_str()),
        Aim::Home => true,
    };
    if o.mission.as_deref() == Some(plan.mission) && same_target {
        return Err(no(
            "already_on_mission",
            format!("{} already runs {} there", o.name, plan.mission),
        ));
    }
    if let Some(running) = o.mission.as_deref() {
        // free: no mission, or counter-intelligence at home (the target, when read, is one of
        // the operative's own nationalities)
        let home_default = running == "no_mission"
            || (running == "counter_intelligence"
                && o.mission_state.is_none()
                && o.mission_target
                    .as_ref()
                    .is_none_or(|t| o.nationalities.contains(t)));
        if !replace && !home_default {
            let at = o
                .mission_target
                .clone()
                .or(o.mission_state.map(|s| format!("state {s}")))
                .unwrap_or_default();
            return Err(no(
                "mission_running",
                format!(
                    "{} runs {running} {at}; pass replace:true to end it for {}",
                    o.name, plan.mission
                ),
            ));
        }
    }
    Ok(())
}

/// Settle rule shared by the agency actions: memory after the input against what the action
/// must have changed.
fn memory_outcome(detail: Value, reached: bool) -> Outcome {
    if reached {
        Outcome::Reached(detail)
    } else {
        Outcome::Unchanged(detail)
    }
}

// ---------------------------------------------------------------- creation

fn create(process: &dyn Process, keys: &dyn GameInput) -> Driven {
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
                let item = pk::items(r, i, grid)?.into_iter().next().ok_or_else(|| {
                    Error::new("gui_target_unavailable", "No agency logos offered")
                })?;
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
        Ok(json!({"creation_started": started, "picked_logo": picked_logo}))
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

fn upgrade(process: &dyn Process, keys: &dyn GameInput, names: &Names) -> Driven {
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
                let title =
                    pk::text(r, i, item.frame, "agency_branches_title", 2)?.unwrap_or_default();
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
                let title =
                    pk::text(r, i, entry.frame, "upgrade_branch_name", 2)?.unwrap_or_default();
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
            (Some(u), Some(text)) => {
                Some(loc::same(text, u) || text.to_lowercase().contains(&u.to_lowercase()))
            }
            _ => None,
        };
        Ok(json!({
            "upgrade_started": after.upgrading.is_some(),
            "progress_text": after.upgrading,
            "matches_requested": matches,
            "confirmation_accepted": confirmed,
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

/// `want` is the candidate's name from memory (hoi4_intelligence `agency.candidates`); the row
/// whose name text matches is clicked. The grid keeps recycled rows, so positions alone mislead.
fn recruit(
    process: &dyn Process,
    keys: &dyn GameInput,
    index: usize,
    want: Option<&str>,
) -> Driven {
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
        let candidates = |r: &Reader<'_>,
                          i: u64|
         -> Result<Vec<(Option<String>, gui_tree::Rect)>> {
            let root = pk::open_root(r, i, &pk::OPERATIVE_RECRUIT)?;
            let mut grids = Vec::new();
            pk::grids(r, i, root, 6, &mut grids)?;
            let mut out = Vec::new();
            // The shared leader-selection layout (leaders / leaders_grid, as the admiral list).
            if let Some(list) = pk::find(r, i, root, "leaders", 2)? {
                if let Some(inner) = gui_tree::container(r, list, i)? {
                    if let Some(grid) = gui_tree::child(r, i, inner, "leaders_grid")? {
                        let items = super::navalui::grid_items(r, grid)?;
                        for item in items.into_iter().take(64) {
                            let window = super::navalui::item_window(r, item)?;
                            let Some(frame) = gui_tree::container(r, window, i)? else {
                                continue;
                            };
                            if !gui_tree::visible(r, frame)? {
                                continue;
                            }
                            let name =
                                pk::text(r, i, frame, "name", 2)?.map(|t| t.trim().to_string());
                            // 1.19.4.1 rows: portrait_overlay_bg, title, name, skill, traits_grid
                            // (no button of their own; a click on the portrait picks the row).
                            let icon = match gui_tree::child(r, i, frame, "leader_icon")? {
                                Some(w) => Some(w),
                                None => gui_tree::child(r, i, frame, "portrait_overlay_bg")?,
                            };
                            if let Some(icon) = icon {
                                if let Ok(rect) = gui_tree::rect_unclipped(r, i, icon, pk::GAME) {
                                    out.push((name, rect));
                                }
                            }
                        }
                    }
                }
            }
            if !out.is_empty() {
                return Ok(out);
            }
            for grid in grids {
                for item in pk::items(r, i, grid)? {
                    // The candidate rows use the shared leader-row layout (leader_icon, as the
                    // admiral list) on 1.19.4.1; older layouts named it portrait_btn.
                    let button = match pk::find(r, i, item.frame, "portrait_btn", 2)? {
                        Some(b) => Some(b),
                        None => pk::find(r, i, item.frame, "leader_icon", 2)?,
                    };
                    if let Some(button) = button {
                        if let Ok(rect) = pk::rect(r, i, button) {
                            out.push((None, rect));
                        }
                    }
                }
            }
            Ok(out)
        };
        let offered = settled(process, "operative candidates", candidates)?;
        let named = offered
            .iter()
            .any(|(n, _)| n.as_deref().is_some_and(|n| !n.is_empty()));
        let pick = match (want, named) {
            (Some(w), true) => offered.iter().find(|(n, _)| n.as_deref() == Some(w)),
            _ => offered.get(index),
        };
        if pick.is_none() && want.is_some() && named {
            return Err(refuse(
                "candidate_not_shown",
                format!(
                    "{} is not among the shown candidates ({}); nothing was recruited",
                    want.unwrap_or_default(),
                    offered
                        .iter()
                        .filter_map(|(n, _)| n.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        }
        let Some((_, rect)) = pick else {
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
            "clicked": pick.and_then(|(n, _)| n.clone()),
            "confirmation_accepted": confirmed,
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
        return Err(Error::new(
            "invalid_arguments",
            "operative must be a bar position 0..31",
        ));
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
        Target::Ideology => return Err(Error::new(
            "mission_not_built",
            "boost_ideology needs the ideology choice window, which this tool does not drive yet",
        )),
        Target::State => {
            let state = state_id.filter(|s| (1..=4095).contains(s)).ok_or_else(|| {
                Error::new(
                    "target_required",
                    format!("{key} needs state_id (a state of the target network)"),
                )
            })?;
            if country.is_some() {
                return Err(Error::new(
                    "invalid_arguments",
                    format!("{key} takes state_id, not country"),
                ));
            }
            Aim::State(state as i32)
        }
        Target::Country => {
            if state_id.is_some() {
                return Err(Error::new(
                    "invalid_arguments",
                    format!("{key} takes country, not state_id"),
                ));
            }
            match country {
                Some(tag)
                    if tag.len() == 3
                        && tag
                            .bytes()
                            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) =>
                {
                    Aim::Country(tag.to_string())
                }
                Some(_) => {
                    return Err(Error::new(
                        "invalid_arguments",
                        "country must be a three-letter tag",
                    ))
                }
                None if key == "counter_intelligence" => Aim::Home,
                None => {
                    return Err(Error::new(
                        "target_required",
                        format!("{key} needs country (a tag such as ITA)"),
                    ))
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
    let states = read(process, |r, i| {
        diplomacy_state::states_owned_by(r, i, tag, 4)
    })?;
    states.first().copied().ok_or_else(|| {
        Error::new(
            "country_has_no_states",
            format!("{tag} owns no state to click on"),
        )
    })
}

fn assign(
    process: &dyn Process,
    keys: &dyn GameInput,
    plan: &AssignPlan,
    state: Option<i32>,
) -> Driven {
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
        // A new operative starts on counter-intelligence at home; the game lets the order bar
        // switch any operative to another mission, so a running mission is no refusal.
        let was_on_mission = slot.on_mission;
        let position = plan.operative;
        pk::click(process, keys, "operative portrait", |r, i| {
            let slot = bar_slots(r, i)?
                .into_iter()
                .filter(|s| s.recruited)
                .nth(position)
                .ok_or_else(|| Error::new("gui_target_unavailable", "Operative left the bar"))?;
            // portrait_btn is drawn at scale 0.4 inside an 84x150 slot window anchored to the
            // screen bottom (operativeleader.gui: origo center_down); its rectangle reads
            // unscaled with the bottom anchor missing (929,851 156x210 for a portrait drawn at
            // about 929..991 x 977..1061; live 6 Nov 2026). Click the drawn portrait's centre.
            let rect = pk::rect_of(r, i, slot.frame, "portrait_btn", 2)?;
            let w = (f64::from(rect.w) * 0.4).round() as i32;
            let h = (f64::from(rect.h) * 0.4).round() as i32;
            let top = pk::GAME.1 as i32 - 150 + 47;
            Ok(gui_tree::Rect {
                x: rect.x,
                y: top,
                w,
                h,
            })
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
                format!(
                    "The game disables the {} button for this operative",
                    plan.mission
                ),
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
        if !on_mission {
            // The click did not start the mission: the map still waits for a target (pm1/gr6:
            // the executor stood in the intelligence map pick and every later map click went
            // to it). Escape leaves the pick; the pause guard closes the game menu if the
            // Escape found nothing else to close.
            let _ = keys.press(crate::platform::game_input::Key::Escape);
            pause(300);
        }
        Ok(json!({
            "on_mission": on_mission,
            "replaced_running_mission": was_on_mission,
            "mission": plan.mission,
            "confirmation_accepted": confirmed,
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
    keys: &dyn GameInput,
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
                plan: Box::new(|process| create_check(&mem(process)?)),
                dispatch: Box::new(|process, keys| create(process, keys)),
                settle: Box::new(|process, _| {
                    let a = mem(process)?;
                    Ok(memory_outcome(
                        json!({"tool":"agency_create","creating":a.creating,"created":a.created,
                               "progress_days":a.progress_days,"verification":"memory"}),
                        a.creating || a.created,
                    ))
                }),
            };
            ui_action::run(job, id, token, process, keys, guard, authority, actions)
        }
        "hoi4_agency_upgrade" => {
            super::allowed(args, &["branch", "upgrade", "request_id", "lease_token"])?;
            let branch = super::string(args, "branch")?;
            let upgrade_key =
                match args.get("upgrade") {
                    None => None,
                    Some(v) => Some(v.as_str().ok_or_else(|| {
                        Error::new("invalid_arguments", "upgrade must be a string")
                    })?),
                };
            for key in std::iter::once(branch).chain(upgrade_key) {
                if key.is_empty()
                    || key.len() > 96
                    || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Invalid branch or upgrade key",
                    ));
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
                    let def = upgrade_key.and_then(|u| {
                        intelligence::catalog(&dir)
                            .upgrades
                            .iter()
                            .find(|(_, d)| d.key == u)
                            .map(|(_, d)| {
                                (d.levels, d.requires.clone(), d.factories_during_progress)
                            })
                    });
                    upgrade_check(
                        &mem(process)?,
                        upgrade_key,
                        def.as_ref().map(|d| d.0).filter(|l| *l > 0),
                        def.as_ref().map_or(&[][..], |d| &d.1[..]),
                    )?;
                    // An upgrade uses civilian factories while it runs; with fewer free for
                    // projects the game only queues it ("not enough Civilian Factories ...
                    // Queued", live 16 Nov 2026) and nothing runs.
                    if let Some(need) = def.as_ref().and_then(|d| d.2) {
                        let free = crate::economy::snapshot(process)?
                            ["civilian_factories_available_for_projects"]
                            .as_f64();
                        if let Some((code, message)) = factories_refusal(need, free) {
                            return Err(no(code, message));
                        }
                    }
                    let mut keys_wanted = vec![branch];
                    keys_wanted.extend(upgrade_key);
                    let names_map = loc::lookup(&dir, &keys_wanted);
                    let branch_name = names_map.get(branch).cloned().ok_or_else(|| {
                        Error::new(
                            "display_name_unknown",
                            format!("No English name for {branch}"),
                        )
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
                    let n = held.as_ref().ok_or_else(|| {
                        Error::new("action_uncertain", "Plan missing at dispatch")
                    })?;
                    upgrade(process, keys, n)
                }),
                settle: Box::new(|process, _| {
                    let a = mem(process)?;
                    let running = a.upgrading.clone();
                    let reached = match upgrade_key {
                        Some(u) => running.as_deref() == Some(u),
                        None => running.is_some(),
                    };
                    Ok(memory_outcome(
                        json!({"tool":"agency_upgrade","branch":branch,"upgrade":upgrade_key,
                               "running":running,"verification":"memory"}),
                        reached,
                    ))
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
                    .ok_or_else(|| Error::new("invalid_arguments", "candidate must be 0..31"))?
                    as usize,
            };
            let before: RefCell<Option<usize>> = RefCell::new(None);
            let wanted: RefCell<Option<String>> = RefCell::new(None);
            let job = Job {
                kind: "operative_recruit",
                action: json!({"kind":"operative_recruit","candidate":index}),
                plan: Box::new(|process| {
                    let a = mem(process)?;
                    let wait = intelligence::catalog(&mgmt::game_dir(process)?)
                        .defines
                        .iter()
                        .find(|(n, _)| *n == "AGENCY_OPERATIVE_RECRUITMENT_TIME")
                        .map_or(30, |(_, v)| v.round() as i32);
                    recruit_check(&a, index, wait)?;
                    *before.borrow_mut() = Some(a.operatives.len());
                    *wanted.borrow_mut() = a.candidates.get(index).cloned();
                    Ok(())
                }),
                dispatch: Box::new(|process, keys| {
                    recruit(process, keys, index, wanted.borrow().as_deref())
                }),
                settle: Box::new(|process, _| {
                    let a = mem(process)?;
                    let had = *before.borrow();
                    let newest = a.operatives.last().map(|o| json!({"name":o.name,"codename":o.codename,"nationalities":o.nationalities}));
                    let asked = wanted.borrow().clone();
                    let grew = had.is_some_and(|n| a.operatives.len() > n);
                    let who = a.operatives.last().map(|o| o.name.clone());
                    let detail = json!({"tool":"operative_recruit","recruited":a.operatives.len(),"before":had,
                               "asked":asked,"operative":newest,"verification":"memory"});
                    // A different candidate than the one asked for is a divergence, not success.
                    if grew && asked.is_some() && who != asked {
                        return Ok(Outcome::Diverged(detail));
                    }
                    Ok(memory_outcome(detail, grew))
                }),
            };
            ui_action::run(job, id, token, process, keys, guard, authority, actions)
        }
        "hoi4_operative_assign" => {
            super::allowed(args, OPERATIVE_ASSIGN_ARGS)?;
            if args.contains_key("operation") {
                return Err(Error::new(
                    "operation_not_built",
                    "hoi4_operative_assign does not run operations: call hoi4_operation_start {target: <tag>, operation: <key>} (it fills the operative slots itself), or plan intelligence.operations; hoi4_operations says first what each operation still needs",
                ));
            }
            let operative = args
                .get("operative")
                .and_then(Value::as_i64)
                .ok_or_else(|| {
                    Error::new(
                        "invalid_arguments",
                        "operative must be an integer bar position",
                    )
                })?;
            let mission = super::string(args, "mission")?;
            let state_id = match args.get("state_id") {
                None => None,
                Some(v) => Some(v.as_i64().ok_or_else(|| {
                    Error::new("invalid_arguments", "state_id must be an integer")
                })?),
            };
            let country =
                match args.get("country") {
                    None => None,
                    Some(v) => Some(v.as_str().ok_or_else(|| {
                        Error::new("invalid_arguments", "country must be a string")
                    })?),
                };
            let plan = plan_assign(operative, mission, state_id, country)?;
            let replace = match args.get("replace") {
                None => false,
                Some(v) => v.as_bool().ok_or_else(|| {
                    Error::new("invalid_arguments", "replace must be true or false")
                })?,
            };
            let chosen: RefCell<Option<i32>> = RefCell::new(None);
            let job = Job {
                kind: "operative_assign",
                action: json!({"kind":"operative_assign","operative":operative,"mission":mission,"state_id":state_id,"country":country}),
                plan: Box::new(|process| {
                    assign_check(&mem(process)?, &plan, replace)?;
                    if let Aim::Country(tag) = &plan.aim {
                        *chosen.borrow_mut() = Some(target_state(process, tag)?);
                    }
                    if let Aim::State(s) = plan.aim {
                        *chosen.borrow_mut() = Some(s);
                    }
                    Ok(())
                }),
                dispatch: Box::new(|process, keys| assign(process, keys, &plan, *chosen.borrow())),
                settle: Box::new(|process, _| {
                    let a = mem(process)?;
                    let Some(o) = a.operatives.get(plan.operative) else {
                        return Ok(Outcome::Diverged(
                            json!({"tool":"operative_assign","operative_missing":plan.operative}),
                        ));
                    };
                    let aimed = match &plan.aim {
                        Aim::State(_) => {
                            o.mission_state.is_some() && o.mission_state == *chosen.borrow()
                        }
                        Aim::Country(t) => o.mission_target.as_deref() == Some(t.as_str()),
                        Aim::Home => true,
                    };
                    Ok(memory_outcome(
                        json!({"tool":"operative_assign","operative":o.name,"mission":o.mission,
                               "target":o.mission_target,"state_id":o.mission_state,"verification":"memory"}),
                        o.mission.as_deref() == Some(plan.mission) && aimed,
                    ))
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

/// Every argument hoi4_operative_assign takes: the schema's properties and the dispatch's allow-list are
/// both checked against this list (g126b: the schema offered `replace`, the allow-list refused it, and the
/// executor's replace:true failed as invalid_arguments 4 times).
const OPERATIVE_ASSIGN_ARGS: &[&str] = &[
    "operative",
    "mission",
    "operation",
    "state_id",
    "country",
    "replace",
    "request_id",
    "lease_token",
];
pub(super) fn tools() -> Vec<Value> {
    vec![
        json!({"name":"hoi4_agency_create","description":"Create the intelligence agency with ordinary input: Shift+W agency view, Create Agency, the name/logo window (default name kept, the first logo chosen when Create is disabled), Create. Refusals from a fresh memory read before any input: agency_exists, agency_creation_in_progress; agency_creation_unavailable when the game disables the button. Verified by memory (creating or created). Live-verified on 1.19.4. Creation takes civilian factories for the define's number of days.","inputSchema":{"type":"object","properties":{"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_agency_upgrade","description":"Start an agency upgrade: Shift+W agency view, the branch button, the upgrade entry in the upgrades window (the named `upgrade`, else the first enabled one). Branch and upgrade keys are those of hoi4_intelligence (branch_intelligence, upgrade_army_department, ...). Typed refusals before input: branch_unknown, upgrade_unknown (loaded database); agency_not_created, upgrade_in_progress, upgrade_maxed, upgrade_locked (missing required upgrade) from memory; upgrade_needs_factories when the upgrade's civilian factory use (scripts) is above the factories free for projects (the game would only queue it); upgrade_unavailable when the game disables the entry. Verified by memory (the running upgrade equals the request). Live-verified on 1.19.4.","inputSchema":{"type":"object","properties":{"branch":{"type":"string","maxLength":96},"upgrade":{"type":"string","maxLength":96},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["branch","request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_operative_recruit","description":"Recruit an operative into a free slot of the operative bar: Shift+W agency view, the empty slot, then the candidate `candidate` (0-based index into hoi4_intelligence's candidate list, default 0), picked in the recruitment window by its name. Typed refusals before input, from memory: agency_not_created, no_free_operative_slot, operative_slot_locked (says in how many days candidates are offered: AGENCY_OPERATIVE_RECRUITMENT_TIME counts from creation), unknown_candidate; candidate_not_shown and operative_bar_unavailable from the window. Verified by memory: Reached when the operative count grew, Diverged when someone else was recruited. Live-verified on 1.19.4.","inputSchema":{"type":"object","properties":{"candidate":{"type":"integer","minimum":0,"maximum":31},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}}),
        json!({"name":"hoi4_operative_assign","description":"Send a recruited operative (`operative` = 0-based position among the recruited ones on the operative bar) on a mission: portrait, the order bar's mission button, then one click on the target. State missions (build_intel_network, quiet_intel_network, root_out_resistance) take `state_id`; country missions (counter_intelligence, control_trade, diplomatic_pressure, propaganda) take `country` (a tag; the Find view centres its lowest state id and that click picks the country; counter_intelligence without country starts at home with a right click). boost_ideology (needs the ideology window) and `operation` refuse with mission_not_built / operation_not_built (operations are started with hoi4_operation_start). Typed refusals before input: invalid_arguments, target_required, country_has_no_states, unknown_country; agency_not_created, operative_unknown, operative_busy (on an operation), already_on_mission, mission_running (the operative runs another mission than the starting counter-intelligence at home: pass replace:true to end it) from memory; operative_bar_unavailable, mission_unavailable (the game disables the button) from the windows. Verified by memory (mission and target). Live-verified on 1.19.4.","inputSchema":{"type":"object","properties":{"operative":{"type":"integer","minimum":0,"maximum":31},"mission":{"type":"string"},"state_id":{"type":"integer","minimum":1,"maximum":4095},"country":{"type":"string","minLength":3,"maxLength":3},"operation":{"type":"string"},"replace":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["operative","request_id","lease_token"],"additionalProperties":false}}),
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
    fn operative_assign_takes_every_schema_argument() {
        let defs = tools();
        let def = defs
            .iter()
            .find(|d| d["name"] == "hoi4_operative_assign")
            .expect("definition");
        let props = def["inputSchema"]["properties"]
            .as_object()
            .expect("properties");
        for key in props.keys() {
            assert!(
                OPERATIVE_ASSIGN_ARGS.contains(&key.as_str()),
                "schema offers {key} but the dispatch refuses it"
            );
        }
        for key in OPERATIVE_ASSIGN_ARGS {
            assert!(
                props.contains_key(*key),
                "{key} is allowed but not in the schema"
            );
        }
    }

    #[test]
    fn create_refuses_from_the_window_state() {
        assert_eq!(
            create_refusal(&gui(true, false, false, None)).unwrap().0,
            "agency_exists"
        );
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
        assert_eq!(
            upgrade_refusal(&gui(false, false, true, None)).unwrap().0,
            "agency_not_created"
        );
        assert_eq!(
            upgrade_refusal(&gui(true, false, false, Some("Army department")))
                .unwrap()
                .0,
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
                vec![
                    "upgrade_economy_civilian".into(),
                    "upgrade_army_department".into(),
                ],
            )],
        };
        assert!(plan_upgrade(&live, "branch_intelligence", None).is_ok());
        assert!(plan_upgrade(
            &live,
            "branch_intelligence",
            Some("upgrade_army_department")
        )
        .is_ok());
        assert_eq!(
            plan_upgrade(&live, "branch_x", None).unwrap_err().code,
            "branch_unknown"
        );
        assert_eq!(
            plan_upgrade(&live, "branch_intelligence", Some("nope"))
                .unwrap_err()
                .code,
            "upgrade_unknown"
        );
    }

    #[test]
    fn mission_plans_follow_the_target_kind() {
        let p = plan_assign(0, "build_intel_network", Some(64), None).unwrap();
        assert_eq!((p.button, p.aim), (1, Aim::State(64)));
        let p = plan_assign(2, "propaganda", None, Some("ITA")).unwrap();
        assert_eq!(
            (p.button, p.aim, p.operative),
            (8, Aim::Country("ITA".into()), 2)
        );
        assert_eq!(
            plan_assign(0, "counter_intelligence", None, None)
                .unwrap()
                .aim,
            Aim::Home
        );
        assert_eq!(
            plan_assign(0, "root_out_resistance", None, None)
                .unwrap_err()
                .code,
            "target_required"
        );
        assert_eq!(
            plan_assign(0, "control_trade", None, None)
                .unwrap_err()
                .code,
            "target_required"
        );
        assert_eq!(
            plan_assign(0, "control_trade", Some(3), Some("ITA"))
                .unwrap_err()
                .code,
            "invalid_arguments"
        );
        assert_eq!(
            plan_assign(0, "propaganda", None, Some("it"))
                .unwrap_err()
                .code,
            "invalid_arguments"
        );
        assert_eq!(
            plan_assign(0, "boost_ideology", None, Some("ITA"))
                .unwrap_err()
                .code,
            "mission_not_built"
        );
        assert_eq!(
            plan_assign(0, "sabotage", None, None).unwrap_err().code,
            "invalid_arguments"
        );
        assert_eq!(
            plan_assign(99, "propaganda", None, Some("ITA"))
                .unwrap_err()
                .code,
            "invalid_arguments"
        );
        // The table covers the eight mission classes of hoi4_intelligence.
        for key in intelligence::MISSIONS {
            assert!(MISSIONS.iter().any(|m| m.0 == *key), "{key}");
        }
    }

    #[test]
    fn an_upgrade_needs_its_factories_free_for_projects() {
        let (code, message) = factories_refusal(10.0, Some(7.0)).unwrap();
        assert_eq!(code, "upgrade_needs_factories");
        assert!(message.contains("10") && message.contains('7'), "{message}");
        assert!(factories_refusal(10.0, Some(10.0)).is_none());
        assert!(factories_refusal(10.0, None).is_none());
    }

    #[test]
    fn recruit_needs_a_free_slot() {
        let slot = |recruited| Slot {
            frame: 1,
            recruited,
            on_mission: false,
        };
        assert_eq!(
            recruit_refusal(&[slot(true), slot(true)]).unwrap().0,
            "no_free_operative_slot"
        );
        assert!(recruit_refusal(&[slot(true), slot(false)]).is_none());
        assert_eq!(recruited(&[slot(true), slot(false), slot(true)]), 2);
    }

    fn agency(
        created: bool,
        upgrading: Option<&str>,
        operatives: usize,
        slots: i32,
    ) -> intel_live::Agency {
        intel_live::Agency {
            created,
            upgrading: upgrading.map(str::to_string),
            upgrades: vec![("upgrade_training_centers".into(), 1)],
            operatives: (0..operatives)
                .map(|n| intel_live::Operative {
                    name: format!("op{n}"),
                    codename: String::new(),
                    nationalities: vec![],
                    mission: Some("build_intel_network".into()),
                    mission_target: Some("ETH".into()),
                    mission_state: Some(842),
                    address: 0,
                    id: n as i32,
                    operation: None,
                    operation_id: None,
                })
                .collect(),
            slots,
            ..Default::default()
        }
    }

    fn code(r: Result<()>) -> String {
        r.err().map(|e| e.code.to_string()).unwrap_or_default()
    }

    #[test]
    fn memory_refuses_before_any_input() {
        assert_eq!(
            code(create_check(&agency(true, None, 0, 1))),
            "agency_exists"
        );
        let mut a = agency(false, None, 0, 0);
        a.creating = true;
        assert_eq!(code(create_check(&a)), "agency_creation_in_progress");
        assert_eq!(code(create_check(&agency(false, None, 0, 0))), "");

        let up =
            |a: &intel_live::Agency, k, l, req: &[String]| code(upgrade_check(a, Some(k), l, req));
        assert_eq!(
            up(&agency(false, None, 0, 0), "upgrade_x", None, &[]),
            "agency_not_created"
        );
        assert_eq!(
            up(
                &agency(true, Some("upgrade_y"), 0, 1),
                "upgrade_x",
                None,
                &[]
            ),
            "upgrade_in_progress"
        );
        assert_eq!(
            up(
                &agency(true, None, 0, 1),
                "upgrade_training_centers",
                Some(1),
                &[]
            ),
            "upgrade_maxed"
        );
        assert_eq!(
            up(
                &agency(true, None, 0, 1),
                "upgrade_x",
                Some(1),
                &["upgrade_z".into()]
            ),
            "upgrade_locked"
        );
        assert_eq!(
            up(
                &agency(true, None, 0, 1),
                "upgrade_x",
                Some(1),
                &["upgrade_training_centers".into()]
            ),
            ""
        );

        assert_eq!(
            code(recruit_check(&agency(false, None, 0, 0), 0, 30)),
            "agency_not_created"
        );
        assert_eq!(
            code(recruit_check(&agency(true, None, 2, 2), 0, 30)),
            "no_free_operative_slot"
        );
        let mut a = agency(true, None, 1, 2);
        a.slot_wait_days = 3;
        let e = recruit_check(&a, 0, 30).unwrap_err();
        assert_eq!(
            (e.code, e.message.contains("in 27 days")),
            ("operative_slot_locked", true)
        );
        a.candidates = vec!["A".into(), "B".into()];
        assert_eq!(code(recruit_check(&a, 2, 30)), "unknown_candidate");
        assert_eq!(code(recruit_check(&a, 1, 30)), "");

        let plan = |operative, aim| AssignPlan {
            operative,
            mission: "build_intel_network",
            button: 1,
            aim,
        };
        let a = agency(true, None, 1, 2);
        assert_eq!(
            code(assign_check(&a, &plan(1, Aim::State(842)), false)),
            "operative_unknown"
        );
        assert_eq!(
            code(assign_check(&a, &plan(0, Aim::State(842)), false)),
            "already_on_mission"
        );
        assert_eq!(
            code(assign_check(&a, &plan(0, Aim::State(843)), false)),
            "mission_running"
        );
        assert_eq!(code(assign_check(&a, &plan(0, Aim::State(843)), true)), "");
        let mut busy = a.clone();
        busy.operatives[0].operation = Some("operation_collaboration_government".into());
        assert_eq!(
            code(assign_check(&busy, &plan(0, Aim::State(843)), true)),
            "operative_busy"
        );
    }

    #[test]
    fn descriptors_mark_validation_and_are_lease_gated() {
        for t in tools() {
            let d = t["description"].as_str().unwrap();
            // All four were live-verified (November 2026).
            assert!(!d.starts_with("UNVALIDATED"));
            assert!(t["inputSchema"]["required"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v == "lease_token"));
            assert!(TOOLS.contains(&t["name"].as_str().unwrap()));
        }
        assert_eq!(tools().len(), TOOLS.len());
    }
}
