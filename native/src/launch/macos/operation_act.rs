//! `hoi4_operation_start`: prepare an intelligence operation against a target with ordinary
//! input, the way a player does it. Route (1.19.4.1, mapped live 14 Nov 2026): Shift+W agency
//! view, the Operations tab, the operation's row (`operation_view_entry`, its name from the
//! localisation) and its `btn_select_button` (Prepare); the overview window
//! (`operation_overview_window`) then shows one `operatives_grid` slot per required operative.
//! Each empty slot's `portrait_btn` opens the selection list (`operativeleaderwindow` of
//! `COperativeLeaderForOperationWindow`), whose rows carry the operative's `name`; a click on the
//! row's portrait assigns them. `cb_auto_commence` makes the game commence the operation as soon
//! as its resources are gathered; `btn_start_button` (Prepare) starts the preparation.
//!
//! The operations list is virtual (a handful of row windows reused while it scrolls), so a row
//! is found by its name and the overview's own memory names the operation and target it was
//! opened for (window +0x13c0 `COperation*` with the key at +0x450, +0x1380 the target tag id):
//! a wrong row is closed again before any state-changing click.
use super::{
    intel_live,
    journal::Outcome,
    loc, mgmt, panelkit as pk, states,
    ui::{accept_command, clear_notices, read, settled, wait},
    ui_action::{self, refuse, Driven, Job},
    Guard, InputAuthority,
};
use crate::{
    error::{Error, Result},
    gui_tree::{self, Rect},
    memory::Reader,
    platform::{game_input::GameInput, Process},
};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::{thread, time::Duration};

/// ANSI virtual key code of `w` (Shift+W opens the intelligence agency view).
const KEY_W: u16 = 13;
/// Overview window fields (macOS 1.19.4.1, live 14 Nov 2026).
const OVERVIEW_TARGET: u64 = 0x1380;
const OVERVIEW_OPERATION: u64 = 0x13c0;
/// `COperation` key string.
const OPERATION_KEY: u64 = 0x450;
/// Wheel steps tried while the wanted row is not on screen.
const SCROLL_TRIES: usize = 8;

fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}

/// What the request names and what the plan resolved from memory and the files.
#[derive(Clone, Debug)]
pub(super) struct StartPlan {
    pub target: String,
    pub operation: String,
    /// The operation's display name (`operation_name_text` of its row).
    pub display: String,
    /// Names of the operatives to assign, in slot order.
    pub operatives: Vec<String>,
    pub auto_commence: bool,
}

/// (operation key, target tag) the open overview window was opened for.
pub(super) fn overview_identity(r: &Reader<'_>, i: u64) -> Result<Option<(String, String)>> {
    if !pk::is_open(r, i, &pk::OPERATION_OVERVIEW)? {
        return Ok(None);
    }
    let Some(window) = pk::child_object(r, i, &pk::OPERATION_OVERVIEW)? else {
        return Ok(None);
    };
    let op = mgmt::pointer(r, window + OVERVIEW_OPERATION)?;
    let key = mgmt::text(r, op + OPERATION_KEY, 128)?.unwrap_or_default();
    let target = states::tag_name(r, i, r.i32(window + OVERVIEW_TARGET)?)?;
    Ok(Some((key, target)))
}

fn open_agency(process: &dyn Process, keys: &dyn GameInput) -> Result<()> {
    if pk::open_now(process, &pk::AGENCY_VIEW) {
        return Ok(());
    }
    keys.press_shifted(KEY_W)?;
    wait(process, "open intelligence agency view", |r, i| pk::is_open(r, i, &pk::AGENCY_VIEW))?;
    pause(400);
    Ok(())
}

/// The operations grid and its visible clip (the grid container's rectangle).
fn grid(r: &Reader<'_>, i: u64) -> Result<(u64, Rect)> {
    let root = pk::open_root(r, i, &pk::AGENCY_VIEW)?;
    let holder = pk::need(r, i, root, "operations_grid_container", 6)?;
    let window = gui_tree::container(r, holder, i)?
        .ok_or_else(|| Error::new("invalid_gui", "operations_grid_container is not a window"))?;
    // The container has no renderer of its own: the visible list runs from below the
    // "Show unavailable" checkbox above it to the bottom of the screen.
    let top = pk::rect_of(r, i, root, "show_unavailable_operations", 4)?;
    let clip = Rect {
        x: top.x,
        y: top.y + top.h,
        w: 1,
        h: pk::GAME.1 as i32 - (top.y + top.h),
    };
    let grid = pk::need(r, i, window, "operations_grid", 2)?;
    Ok((grid, clip))
}

/// Shown rows wholly inside the clip: (name text, Prepare button rectangle when shown and enabled).
fn rows(r: &Reader<'_>, i: u64) -> Result<Vec<(String, Option<Rect>)>> {
    let (grid, clip) = grid(r, i)?;
    let mut out = Vec::new();
    for item in pk::items(r, i, grid)? {
        let Some(name) = pk::text(r, i, item.frame, "operation_name_text", 3)? else {
            continue;
        };
        let Some(button) = pk::find(r, i, item.frame, "btn_select_button", 3)? else {
            continue;
        };
        if !gui_tree::shown(r, button)? {
            continue;
        }
        let Ok(rect) = pk::rect(r, i, button) else {
            continue;
        };
        let inside = rect.y >= clip.y && rect.y + rect.h <= clip.y + clip.h;
        if !inside {
            continue;
        }
        let enabled = !gui_tree::disabled(r, button)?;
        out.push((name.trim().to_string(), enabled.then_some(rect)));
    }
    Ok(out)
}

/// Rows of the operative selection list: (name, portrait rectangle).
fn leader_rows(r: &Reader<'_>, i: u64) -> Result<Vec<(String, Rect)>> {
    let root = pk::open_root(r, i, &pk::OPERATIVE_FOR_OPERATION)?;
    let mut out = Vec::new();
    let Some(list) = pk::find(r, i, root, "leaders", 2)? else {
        return Ok(out);
    };
    let Some(inner) = gui_tree::container(r, list, i)? else {
        return Ok(out);
    };
    let Some(grid) = gui_tree::child(r, i, inner, "leaders_grid")? else {
        return Ok(out);
    };
    for item in super::navalui::grid_items(r, grid)?.into_iter().take(64) {
        let window = super::navalui::item_window(r, item)?;
        let Some(frame) = gui_tree::container(r, window, i)? else {
            continue;
        };
        if !gui_tree::visible(r, frame)? {
            continue;
        }
        let name = pk::text(r, i, frame, "name", 2)?.unwrap_or_default();
        let icon = match gui_tree::child(r, i, frame, "portrait_overlay_bg")? {
            Some(w) => Some(w),
            None => gui_tree::child(r, i, frame, "operative_portrait_pos")?,
        };
        if let Some(icon) = icon {
            if let Ok(rect) = gui_tree::rect_unclipped(r, i, icon, pk::GAME) {
                out.push((name.trim().to_string(), rect));
            }
        }
    }
    Ok(out)
}

/// Portrait buttons of the overview's operative slots, in order.
fn slot_buttons(r: &Reader<'_>, i: u64) -> Result<Vec<Rect>> {
    let root = pk::open_root(r, i, &pk::OPERATION_OVERVIEW)?;
    let grid = pk::need(r, i, root, "operatives_grid", 4)?;
    let mut out = Vec::new();
    // operation_operative_view_entry: the slot is clicked where its portrait background lies.
    // The background itself is flagged hidden (the game draws the empty portrait over it) but
    // keeps its laid-out place (live 14 Nov 2026), so the entry window's visibility decides.
    for item in pk::items(r, i, grid)? {
        if !gui_tree::visible(r, item.frame)? {
            continue;
        }
        if let Some(b) = pk::find(r, i, item.frame, "portrait_overlay_bg", 3)? {
            // operationoverview.gui: the overlay sits at x=50 of the entry, the empty portrait
            // (operative_portrait_pos x=15, a 54 px wide portrait window) is centred 8 px to its
            // left at the same height (checked against the drawn slots, 14 Nov 2026).
            let o = pk::rect(r, i, b)?;
            out.push(Rect { x: o.x - 8 - o.w / 2, y: o.y, w: o.w, h: o.h });
        }
    }
    Ok(out)
}

fn close_overview(process: &dyn Process, keys: &dyn GameInput) {
    if pk::open_now(process, &pk::OPERATIVE_FOR_OPERATION) {
        let _ = pk::click(process, keys, "close operative list", |r, i| {
            let root = pk::open_root(r, i, &pk::OPERATIVE_FOR_OPERATION)?;
            pk::rect_of(r, i, root, "close_button", 3)
        });
        let _ = wait(process, "operative list closed", |r, i| {
            Ok(!pk::is_open(r, i, &pk::OPERATIVE_FOR_OPERATION)?)
        });
    }
    if pk::open_now(process, &pk::OPERATION_OVERVIEW) {
        let _ = pk::click(process, keys, "close operation overview", |r, i| {
            let root = pk::open_root(r, i, &pk::OPERATION_OVERVIEW)?;
            pk::rect_of(r, i, root, "btn_close", 3)
        });
        let _ = wait(process, "operation overview closed", |r, i| {
            Ok(!pk::is_open(r, i, &pk::OPERATION_OVERVIEW)?)
        });
    }
}

fn leave(process: &dyn Process, keys: &dyn GameInput) {
    close_overview(process, keys);
    pk::dismiss_window(process, keys, &pk::AGENCY_VIEW);
}

/// Open the overview for (operation, target): rows with the operation's name are tried in turn
/// (one per target with a network) until the window's memory names the wanted target.
fn open_overview(process: &dyn Process, keys: &dyn GameInput, plan: &StartPlan) -> Driven {
    let mut tried: Vec<(String, String)> = Vec::new();
    let mut seen_names: Vec<String> = Vec::new();
    let mut disabled = false;
    for attempt in 0..=SCROLL_TRIES {
        let visible = settled(process, "operation rows", rows)?;
        for (name, _) in &visible {
            if !seen_names.contains(name) {
                seen_names.push(name.clone());
            }
        }
        let candidates: Vec<Rect> = visible
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(&plan.display))
            .filter_map(|(_, rect)| {
                if rect.is_none() {
                    disabled = true;
                }
                *rect
            })
            .collect();
        for rect in candidates {
            let (x, y) = rect.center();
            keys.click(pk::GAME, x, y)?;
            wait(process, "open operation overview", |r, i| {
                Ok(overview_identity(r, i)?.is_some())
            })?;
            // The window slides in for 300 ms (animation_time of operation_overview_window).
            pause(600);
            let who = read(process, overview_identity)?.unwrap_or_default();
            if who.0 == plan.operation && who.1 == plan.target {
                return Ok(json!({"overview_for": [who.0, who.1]}));
            }
            tried.push(who);
            close_overview(process, keys);
        }
        if attempt == SCROLL_TRIES {
            break;
        }
        // Scroll the list one wheel step down and look again; stop when nothing moves.
        let before: Vec<String> = visible.iter().map(|(n, _)| n.clone()).collect();
        let (_, clip) = read(process, grid)?;
        let (cx, cy) = (f64::from(clip.x) + 120.0, f64::from(clip.y) + f64::from(clip.h) / 2.0);
        keys.scroll(pk::GAME, (cx, cy), -120)?;
        pause(250);
        let after: Vec<String> = read(process, rows)?.into_iter().map(|(n, _)| n).collect();
        if after == before {
            break;
        }
    }
    if disabled && tried.is_empty() {
        return Err(refuse(
            "operation_unavailable",
            format!(
                "The game disables Prepare on {}; its tooltip names the missing piece (hoi4_operations lists them); nothing was started",
                plan.display
            ),
        ));
    }
    Err(refuse(
        "operation_not_listed",
        format!(
            "No row of {} against {} in the agency's operations list (rows seen: {}; overviews opened: {:?}); nothing was started",
            plan.display,
            plan.target,
            seen_names.join(", "),
            tried
        ),
    ))
}

/// Fill every operative slot with the planned operatives (by name), in order. The selection
/// list stays open after a pick and offers the remaining operatives for the next empty slot
/// (live 14 Nov 2026), so a pick is proven by its row leaving the list.
fn assign_operatives(process: &dyn Process, keys: &dyn GameInput, plan: &StartPlan) -> Driven {
    let slots = settled(process, "operative slots", slot_buttons)?;
    if slots.len() < plan.operatives.len() {
        return Err(refuse(
            "operative_slots_unavailable",
            format!(
                "The overview shows {} operative slots, the plan names {}; nothing was started",
                slots.len(),
                plan.operatives.len()
            ),
        ));
    }
    let mut picked = Vec::new();
    for (n, name) in plan.operatives.iter().enumerate() {
        if !pk::open_now(process, &pk::OPERATIVE_FOR_OPERATION) {
            let (x, y) = slots[n].center();
            keys.click(pk::GAME, x, y)?;
            wait(process, "open operative list", |r, i| {
                pk::is_open(r, i, &pk::OPERATIVE_FOR_OPERATION)
            })?;
            pause(300);
        }
        let offered = settled(process, "operative rows", leader_rows)?;
        let Some((_, row)) = offered.iter().find(|(n, _)| n == name) else {
            return Err(refuse(
                "operative_not_offered",
                format!(
                    "{name} is not in the game's list for this operation ({}); nothing was started",
                    offered.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ")
                ),
            ));
        };
        let (x, y) = row.center();
        keys.click(pk::GAME, x, y)?;
        let _ = accept_command(process, keys);
        let taken = wait(process, "operative taken from the list", |r, i| {
            if !pk::is_open(r, i, &pk::OPERATIVE_FOR_OPERATION)? {
                return Ok(true);
            }
            Ok(!leader_rows(r, i)?.iter().any(|(n, _)| n == name))
        })
        .is_ok();
        if !taken {
            return Err(refuse(
                "operative_pick_failed",
                format!("{name} stayed in the operative list after the click; nothing was started"),
            ));
        }
        picked.push(name.clone());
        pause(250);
    }
    if pk::open_now(process, &pk::OPERATIVE_FOR_OPERATION) {
        pk::click(process, keys, "close operative list", |r, i| {
            let root = pk::open_root(r, i, &pk::OPERATIVE_FOR_OPERATION)?;
            pk::rect_of(r, i, root, "close_button", 3)
        })?;
        wait(process, "operative list closed", |r, i| {
            Ok(!pk::is_open(r, i, &pk::OPERATIVE_FOR_OPERATION)?)
        })?;
        pause(200);
    }
    Ok(json!({"picked": picked}))
}

pub(super) fn start(process: &dyn Process, keys: &dyn GameInput, plan: &StartPlan) -> Driven {
    clear_notices(process, keys)?;
    let result: Driven = (|| {
        open_agency(process, keys)?;
        // The Operations tab (the Cryptology tab hides the operations grid).
        if read(process, |r, i| Ok(rows(r, i).is_err()))? {
            pk::click(process, keys, "operations tab", |r, i| {
                let root = pk::open_root(r, i, &pk::AGENCY_VIEW)?;
                pk::rect_of(r, i, root, "operations_tab_button", 3)
            })?;
            pause(300);
        }
        let opened = open_overview(process, keys, plan)?;
        let picked = assign_operatives(process, keys, plan)?;
        // Auto-commence: the checkbox's state word also carries hover, so it is clicked only
        // when asked and the memory readback of the operation decides.
        if plan.auto_commence {
            pk::click(process, keys, "auto commence", |r, i| {
                let root = pk::open_root(r, i, &pk::OPERATION_OVERVIEW)?;
                pk::rect_of(r, i, root, "cb_auto_commence", 4)
            })?;
            pause(200);
        }
        // The button's state word reads 3 on an enabled Prepare too (live 14 Nov 2026), so it is
        // not taken as disabled: the memory readback of the operation decides.
        pk::click(process, keys, "prepare operation", |r, i| {
            let root = pk::open_root(r, i, &pk::OPERATION_OVERVIEW)?;
            pk::rect_of(r, i, root, "btn_start_button", 4)
        })?;
        let _ = accept_command(process, keys);
        pause(400);
        Ok(json!({"opened": opened, "operatives": picked, "auto_commence_clicked": plan.auto_commence}))
    })();
    leave(process, keys);
    result
}

/// Names of free operatives for the plan: the named positions, else the first free ones.
pub(super) fn pick_operatives(
    agency: &intel_live::Agency,
    positions: Option<&[usize]>,
    needed: usize,
) -> Result<Vec<String>> {
    let free: Vec<(usize, &intel_live::Operative)> = agency
        .operatives
        .iter()
        .enumerate()
        .filter(|(_, o)| o.operation.is_none())
        .collect();
    match positions {
        Some(list) => {
            let mut out = Vec::new();
            for p in list {
                let Some(o) = agency.operatives.get(*p) else {
                    return Err(Error::new(
                        "operative_unknown",
                        format!("{} operatives are recruited, not position {p}", agency.operatives.len()),
                    ));
                };
                if let Some(op) = &o.operation {
                    return Err(Error::new("operative_busy", format!("{} works on {op}", o.name)));
                }
                out.push(o.name.clone());
            }
            if out.len() < needed {
                return Err(Error::new(
                    "not_enough_operatives",
                    format!("The operation needs {needed} operatives; {} named", out.len()),
                ));
            }
            Ok(out.into_iter().take(needed).collect())
        }
        None => {
            if free.len() < needed {
                return Err(Error::new(
                    "not_enough_operatives",
                    format!(
                        "The operation needs {needed} operatives; {} of {} recruited are free",
                        free.len(),
                        agency.operatives.len()
                    ),
                ));
            }
            Ok(free.into_iter().take(needed).map(|(_, o)| o.name.clone()).collect())
        }
    }
}

/// Display name of an operation key from the localisation.
pub(super) fn display_name(dir: &std::path::Path, key: &str) -> Result<String> {
    loc::lookup(dir, &[key])
        .get(key)
        .cloned()
        .ok_or_else(|| Error::new("display_name_unknown", format!("No English name for {key}")))
}

fn mem(process: &dyn Process) -> Result<intel_live::Agency> {
    read(process, |r, i| {
        let own = mgmt::own(r, i)?;
        intel_live::agency(r, i, own.country)
    })
}

/// Operation definitions of the running game's content (the install with its mods).
fn catalog(
    process: &dyn Process,
) -> Result<(std::sync::Arc<crate::operation_rules::Catalog>, std::path::PathBuf)> {
    let exe = process
        .identity()
        .map(|i| i.exe.clone())
        .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?;
    let (content, _) = crate::protocol::focus_content(&exe)?;
    Ok((crate::operation_rules::Catalog::load(&content), content))
}

#[allow(clippy::too_many_arguments)]
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
    if name != "hoi4_operation_start" {
        return Err(Error::new("capability_unavailable", "Unknown operation tool"));
    }
    super::allowed(
        args,
        &["target", "operation", "operatives", "auto_commence", "request_id", "lease_token"],
    )?;
    let target = super::string(args, "target")?;
    if target.len() != 3 || !target.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()) {
        return Err(Error::new("invalid_arguments", "target must be a three-letter tag"));
    }
    let operation = super::string(args, "operation")?;
    if operation.is_empty()
        || operation.len() > 128
        || !operation.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(Error::new("invalid_arguments", "Invalid operation key"));
    }
    let positions: Option<Vec<usize>> = match args.get("operatives") {
        None => None,
        Some(Value::Array(list)) if list.len() <= 8 => Some(
            list.iter()
                .map(|v| v.as_u64().filter(|n| *n < 32).map(|n| n as usize))
                .collect::<Option<_>>()
                .ok_or_else(|| Error::new("invalid_arguments", "operatives lists positions 0..31"))?,
        ),
        Some(_) => return Err(Error::new("invalid_arguments", "operatives lists up to 8 positions")),
    };
    let auto_commence = match args.get("auto_commence") {
        None => true,
        Some(Value::Bool(b)) => *b,
        Some(_) => return Err(Error::new("invalid_arguments", "auto_commence must be boolean")),
    };
    let plan: RefCell<Option<StartPlan>> = RefCell::new(None);
    let before: RefCell<Option<usize>> = RefCell::new(None);
    let job = Job {
        kind: "operation_start",
        action: json!({"kind":"operation_start","target":target,"operation":operation,
                       "operatives":positions,"auto_commence":auto_commence}),
        plan: Box::new(|process| {
            let (catalog, content) = catalog(process)?;
            let def = catalog.operation(operation).ok_or_else(|| {
                Error::new("unknown_operation", format!("No operation {operation} in the game content"))
            })?;
            if def.picks_location {
                return Err(Error::new(
                    "operation_picks_location",
                    format!("{operation} also picks a state or region in its window; hoi4_operation_start does not choose one yet"),
                ));
            }
            let a = mem(process)?;
            if !a.created {
                return Err(Error::new("agency_not_created", "Create the intelligence agency first (hoi4_agency_create)"));
            }
            let needed = def.operatives.max(1);
            let operatives = pick_operatives(&a, positions.as_deref(), needed)?;
            let display = display_name(&content, operation)
                .or_else(|_| display_name(&mgmt::game_dir(process)?, operation))?;
            *before.borrow_mut() = Some(running_count(process, operation, target));
            *plan.borrow_mut() = Some(StartPlan {
                target: target.to_string(),
                operation: operation.to_string(),
                display,
                operatives,
                auto_commence,
            });
            Ok(())
        }),
        dispatch: Box::new(|process, keys| {
            let held = plan.borrow();
            let p = held.as_ref().ok_or_else(|| Error::new("action_uncertain", "Plan missing at dispatch"))?;
            start(process, keys, p)
        }),
        settle: Box::new(|process, seen| {
            let now = running_count(process, operation, target);
            let had = *before.borrow();
            let detail = json!({"tool":"operation_start","operation":operation,"target":target,
                                "running":now,"before":had,"verification":"memory",
                                "operations": running(process).unwrap_or_default()});
            if had.is_some_and(|n| now > n) {
                return Ok(Outcome::Reached(detail));
            }
            // Memory names no new operation: the dispatch's own evidence decides nothing.
            let _ = seen;
            Ok(Outcome::Unchanged(detail))
        }),
    };
    ui_action::run_bounded(job, 30, id, token, process, keys, guard, authority, actions)
}

/// Operations of the player against `target` with this key that memory holds.
fn running_count(process: &dyn Process, operation: &str, target: &str) -> usize {
    running(process)
        .unwrap_or_default()
        .iter()
        .filter(|o| o["operation"] == json!(operation) && o["target"] == json!(target))
        .count()
}

/// The player's operations from memory (preparing or running).
fn running(process: &dyn Process) -> Result<Vec<Value>> {
    read(process, |r, i| {
        let own = mgmt::own(r, i)?;
        Ok(intel_live::operations(r, i, own.country)?
            .into_iter()
            .map(|o| intel_live::operation_json(&o))
            .collect())
    })
}

pub(super) const TOOLS: [&str; 1] = ["hoi4_operation_start"];

pub(super) fn tools() -> Vec<Value> {
    vec![json!({"name":"hoi4_operation_start","description":"Prepare an intelligence operation against a target with ordinary input, as a player does: agency view (Shift+W), Operations tab, the operation's row (scrolled into view; one row per target with a network, the opened window's memory must name the asked operation and target), Prepare, one free operative per slot picked by name in the game's list, optionally Commence when ready, then the window's Prepare. The game then gathers the equipment and civilian factories and, with auto_commence (default true), commences on its own; the operation's phases then run for its days. Refusals before input: unknown_operation, operation_picks_location (the operation also picks a state or region in its window, e.g. coup, sabotage, coordinated strike: not built), agency_not_created, operative_unknown, operative_busy, not_enough_operatives (from memory and the content's files); from the windows before the first state-changing click: operation_unavailable (the game disables Prepare), operation_not_listed, operative_slots_unavailable, operative_not_offered, operative_pick_failed. Live-verified on 1.19.4 (collaboration government, ITA on ETH, at war). Use hoi4_operations first: it lists every missing piece with a code. Verified from memory: the operation appears among the agency's operations (hoi4_intelligence operations). operatives = recruited positions (hoi4_intelligence operatives.list order); default the first free ones.","inputSchema":{"type":"object","properties":{"target":{"type":"string","minLength":3,"maxLength":3},"operation":{"type":"string","maxLength":128},"operatives":{"type":"array","items":{"type":"integer","minimum":0,"maximum":31},"maxItems":8},"auto_commence":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["target","operation","request_id","lease_token"],"additionalProperties":false}})]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agency(busy: &[bool]) -> intel_live::Agency {
        intel_live::Agency {
            created: true,
            operatives: busy
                .iter()
                .enumerate()
                .map(|(n, b)| intel_live::Operative {
                    name: format!("op{n}"),
                    codename: String::new(),
                    nationalities: vec![],
                    mission: None,
                    mission_target: None,
                    mission_state: None,
                    address: 0,
                    id: n as i32,
                    operation: b.then(|| "operation_x".to_string()),
                    operation_id: None,
                })
                .collect(),
            slots: busy.len() as i32,
            ..Default::default()
        }
    }

    #[test]
    fn operatives_are_picked_from_the_free_ones() {
        assert_eq!(pick_operatives(&agency(&[false, true, false]), None, 2).unwrap(), ["op0", "op2"]);
        let e = pick_operatives(&agency(&[false, true]), None, 2).unwrap_err();
        assert_eq!(e.code, "not_enough_operatives");
        assert_eq!(pick_operatives(&agency(&[false, true]), Some(&[1]), 1).unwrap_err().code, "operative_busy");
        assert_eq!(pick_operatives(&agency(&[false]), Some(&[3]), 1).unwrap_err().code, "operative_unknown");
        assert_eq!(pick_operatives(&agency(&[false, false]), Some(&[1, 0]), 2).unwrap(), ["op1", "op0"]);
    }
}
