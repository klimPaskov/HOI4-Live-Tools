//! Diplomacy with ordinary input: the diplomacy view (E) is pointed at a
//! foreign country by centring one of its states with the Find view and
//! clicking it; actions are its action rows. An action's dialog is the one
//! shown child of the action info window: its Send button is clicked, and
//! the war-goal dialog first gets a type and states. Verified when the
//! action rows change (e.g. "Justify War goal" becomes "Cancel
//! Justification") or political power is spent.
use super::ui::{accept_command, clear_notices, find_state, read, settled, wait};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::{Reader, StringAbi},
    platform::{
        macos_input::{Key, MacKeys},
        Process,
    },
};
use serde_json::{json, Value};

struct Ui {
    manager: u64,
    view: u64,
    view_root: u64,
    shown_tag: u64,
    action_glue: u64,
    info_window: u64,
    grid_items: u64,
    item_container: u64,
    text: u64,
    container_vtable: u64,
    container_base: u64,
    /// War-goal dialog rows relative to its Send button's top-left: type
    /// rows (35 px slots) and state rows (30 px slots), measured live.
    type_row: (f64, f64, f64),
    state_row: (f64, f64, f64),
    visible_state_rows: usize,
}
/// macOS Mach-O 089EA519, mapped live on 18 October 2026 (Germany test run):
/// the view's tag field followed Poland (10) and Czechoslovakia (9); the
/// action controller's info window holds one shown dialog per action; the
/// justify dialog took "Conquer" and "Poznań" by these row offsets and Send.
const MACOS: Ui = Ui {
    manager: 0x670,
    view: 0x110,
    view_root: 0x430,
    shown_tag: 0x1420,
    action_glue: 0x2d38,
    info_window: 0x818,
    grid_items: 0x190,
    item_container: 0x10,
    text: 0x100,
    container_vtable: 0x34ac0b0,
    container_base: 0x30,
    type_row: (-215.0, -216.0, 35.0),
    state_row: (30.0, -222.0, 30.0),
    visible_state_rows: 7,
};
const GAME: (u32, u32) = (1920, 1080);

fn ui(r: &Reader<'_>) -> Result<&'static Ui> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(&MACOS),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "Diplomacy input is not reviewed for this build",
        )),
    }
}
fn view(r: &Reader<'_>, image: u64) -> Result<u64> {
    let u = ui(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    r.u64(r.u64(idler + u.manager)? + u.view)
}
fn open(r: &Reader<'_>, image: u64) -> Result<bool> {
    let root = gui_tree::container(r, r.u64(view(r, image)? + ui(r)?.view_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Diplomacy view root unavailable"))?;
    gui_tree::visible(r, root)
}
fn shown_tag(r: &Reader<'_>, image: u64) -> Result<i32> {
    r.i32(view(r, image)? + ui(r)?.shown_tag)
}
fn player_tag(r: &Reader<'_>, image: u64) -> Result<i32> {
    let g = game::layout(r);
    r.i32(r.u64(image + g.gs_global)? + g.player_tag)
}
fn state_owner(r: &Reader<'_>, image: u64, state: i32) -> Result<i32> {
    let g = game::layout(r);
    let l = crate::construction::layout(r);
    let gs = r.u64(image + g.gs_global)?;
    let object = u64::from_le_bytes(
        r.array_entry(gs + l.gs_states, state as usize, 8)?
            .try_into()
            .unwrap(),
    );
    if object == 0
        || r.u64(object)? != image + l.state_vtable
        || r.i32(object + l.state_id)? != state
    {
        return Err(Error::new("invalid_target", "State not in the catalog"));
    }
    r.i32(object + l.state_owner)
}
/// Text without the engine's colour and icon markup.
fn clean(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            '\u{11}' => {
                chars.next();
            }
            '\u{13}' => {}
            c => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn find(r: &Reader<'_>, image: u64, object: u64, name: &str, depth: u32) -> Result<Option<u64>> {
    for child in gui_tree::children(r, object)? {
        if gui_tree::name(r, child, image)?.as_deref() == Some(name) {
            return Ok(Some(child));
        }
        if depth < 6 {
            if let Some(inner) = gui_tree::container(r, child, image)? {
                if let Some(found) = find(r, image, inner, name, depth + 1)? {
                    return Ok(Some(found));
                }
            }
        }
    }
    Ok(None)
}
/// Texts of the `label` widgets of a grid's entries, in grid order.
fn grid_texts(r: &Reader<'_>, image: u64, grid: u64, label: &str) -> Result<Vec<String>> {
    let u = ui(r)?;
    let (_, data, count) = r.array_header(grid + u.grid_items)?;
    let mut out = Vec::new();
    for index in 0..count.min(128) as u64 {
        let item = r.u64(data + index * 8)?;
        let entry = r.u64(item + u.item_container)?;
        let base = if r.u64(entry)? == image + u.container_vtable {
            entry + u.container_base
        } else {
            entry
        };
        let text = match gui_tree::container(r, base, image)? {
            Some(frame) => match gui_tree::child(r, image, frame, label)? {
                Some(t) => clean(&r.string_bounded(t + u.text, 256)?),
                None => String::new(),
            },
            None => String::new(),
        };
        out.push(text);
    }
    Ok(out)
}

#[derive(Clone, Debug, PartialEq)]
struct Row {
    name: String,
    enabled: bool,
    cost: String,
    ai_refuses: bool,
    rect: Option<Rect>,
}
fn rows(r: &Reader<'_>, image: u64) -> Result<Vec<Row>> {
    let u = ui(r)?;
    let root = gui_tree::container(r, r.u64(view(r, image)? + u.view_root)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Diplomacy view root unavailable"))?;
    let grid = find(r, image, root, "actions_grid", 0)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Diplomatic actions not found"))?;
    let (_, data, count) = r.array_header(grid + u.grid_items)?;
    let mut out = Vec::new();
    for index in 0..count.min(64) as u64 {
        let item = r.u64(data + index * 8)?;
        let Some(frame) = gui_tree::container(r, r.u64(item + u.item_container)?, image)? else {
            continue;
        };
        let text = |name: &str| -> Result<String> {
            Ok(match gui_tree::child(r, image, frame, name)? {
                Some(t) => clean(&r.string_bounded(t + u.text, 256)?),
                None => String::new(),
            })
        };
        let name = text("name")?;
        if name.is_empty() {
            continue;
        }
        let (enabled, rect) = match gui_tree::child(r, image, frame, "diplo_actions_entry_bg")? {
            Some(button) => (
                !gui_tree::disabled(r, button)?,
                gui_tree::rect(r, image, button, GAME).ok(),
            ),
            None => (false, None),
        };
        let ai_refuses = match gui_tree::child(r, image, frame, "accept_icon")? {
            Some(icon) => gui_tree::shown(r, icon)?,
            None => false,
        };
        out.push(Row {
            name,
            enabled,
            cost: text("cost")?,
            ai_refuses,
            rect,
        });
    }
    Ok(out)
}
/// The action dialog shown now: (name, GUI object).
fn dialog(r: &Reader<'_>, image: u64) -> Result<Option<(String, u64)>> {
    let u = ui(r)?;
    let glue = r.u64(view(r, image)? + u.action_glue)?;
    let info = r.u64(glue + u.info_window)?;
    if r.u64(info)? != image + u.container_vtable {
        return Err(Error::new("invalid_gui", "Action info window required"));
    }
    let info = info + u.container_base;
    let Some(window) = gui_tree::container(r, info, image)? else {
        return Ok(None);
    };
    if !gui_tree::visible(r, window)? {
        return Ok(None);
    }
    for child in gui_tree::children(r, window)? {
        if !gui_tree::shown(r, child)? {
            continue;
        }
        if let Some(inner) = gui_tree::container(r, child, image)? {
            if gui_tree::visible(r, inner)? {
                let name = gui_tree::name(r, child, image)?.unwrap_or_default();
                return Ok(Some((name, inner)));
            }
        }
    }
    Ok(None)
}
fn dialog_button(r: &Reader<'_>, image: u64, name: &str) -> Result<(Rect, bool)> {
    let (_, window) =
        dialog(r, image)?.ok_or_else(|| Error::new("input_readback_failed", "Dialog closed"))?;
    let button = gui_tree::child(r, image, window, name)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Dialog button not found"))?;
    Ok((
        gui_tree::rect(r, image, button, GAME)?,
        !gui_tree::disabled(r, button)?,
    ))
}
fn dialog_text(r: &Reader<'_>, image: u64, name: &str) -> Result<String> {
    let (_, window) =
        dialog(r, image)?.ok_or_else(|| Error::new("input_readback_failed", "Dialog closed"))?;
    match find(r, image, window, name, 0)? {
        Some(t) => Ok(clean(&r.string_bounded(t + ui(r)?.text, 256)?)),
        None => Ok(String::new()),
    }
}
fn dialog_grid(r: &Reader<'_>, image: u64, grid: &str) -> Result<Vec<String>> {
    let (_, window) =
        dialog(r, image)?.ok_or_else(|| Error::new("input_readback_failed", "Dialog closed"))?;
    match find(r, image, window, grid, 0)? {
        Some(g) => grid_texts(r, image, g, "name"),
        None => Ok(Vec::new()),
    }
}
fn cancel_dialog(process: &dyn Process, keys: &MacKeys) {
    if let Ok((rect, true)) = read(process, |r, i| dialog_button(r, i, "cancel_button")) {
        let (x, y) = rect.center();
        let _ = keys.click(GAME, x, y);
        let _ = wait(process, "dialog closed", |r, i| Ok(dialog(r, i)?.is_none()));
    }
}

/// Point the diplomacy view at the owner of state `state`.
fn show(process: &dyn Process, keys: &MacKeys, state: i32) -> Result<i32> {
    let (owner, own) = read(process, |r, i| {
        Ok((state_owner(r, i, state)?, player_tag(r, i)?))
    })?;
    if owner == own {
        return Err(Error::new(
            "invalid_target",
            "This state is own; choose a state of the other country",
        ));
    }
    if read(process, |r, i| Ok(open(r, i)? && shown_tag(r, i)? == owner))? {
        return Ok(owner);
    }
    if !read(process, open)? {
        keys.press(Key::letter('e')?)?;
        wait(process, "open diplomacy", open)?;
    }
    find_state(process, keys, state)?;
    keys.click(GAME, 960.0, 540.0)?;
    wait(process, "diplomacy shows the country", |r, i| {
        Ok(open(r, i)? && shown_tag(r, i)? == owner)
    })?;
    std::thread::sleep(std::time::Duration::from_millis(300));
    Ok(owner)
}
fn row_json(row: &Row) -> Value {
    json!({"action":row.name,"enabled":row.enabled,"cost":row.cost,"ai_would_refuse":row.ai_refuses})
}

/// Diplomatic actions offered toward the owner of state `state`.
pub(super) fn actions(process: &dyn Process, keys: &MacKeys, state: i32) -> Result<Value> {
    clear_notices(process, keys)?;
    show(process, keys, state)?;
    let found = settled(process, "diplomatic actions", rows)?;
    Ok(json!({"state_id":state,"actions":found.iter().map(row_json).collect::<Vec<_>>()}))
}
/// Action rows toward the shown country (for settling receipts).
pub(super) fn current(process: &dyn Process) -> Result<Vec<Value>> {
    let found = read(process, |r, i| {
        if open(r, i)? {
            rows(r, i)
        } else {
            Ok(Vec::new())
        }
    })?;
    Ok(found.iter().map(row_json).collect())
}

fn click_and_check(
    process: &dyn Process,
    keys: &MacKeys,
    at: (f64, f64),
    stage: &str,
    check: impl Fn(&Reader<'_>, u64) -> Result<bool>,
) -> Result<()> {
    keys.click(GAME, at.0, at.1)?;
    wait(process, stage, check)
}
/// War-goal dialog: choose the type, then the states, by their rows.
fn war_goal(process: &dyn Process, keys: &MacKeys, kind: &str, states: &[String]) -> Result<()> {
    let u = &MACOS;
    let (send, _) = settled(process, "send button", |r, i| {
        dialog_button(r, i, "send_button")
    })?;
    let types = read(process, |r, i| dialog_grid(r, i, "wargoals_grid"))?;
    let index = types
        .iter()
        .position(|t| t.eq_ignore_ascii_case(kind))
        .ok_or_else(|| {
            Error::new(
                "invalid_arguments",
                format!("War goal type must be one of: {}", types.join(", ")),
            )
        })?;
    let before = read(process, |r, i| dialog_text(r, i, "wargoal_name"))?;
    let (x0, y0) = (f64::from(send.x), f64::from(send.y));
    click_and_check(
        process,
        keys,
        (
            x0 + u.type_row.0,
            y0 + u.type_row.1 + u.type_row.2 * index as f64,
        ),
        "war goal type chosen",
        |r, i| Ok(dialog_text(r, i, "wargoal_name")? != before),
    )?;
    let offered = settled(process, "war goal states", |r, i| {
        dialog_grid(r, i, "additionals_grid")
    })?;
    for state in states {
        let index = offered
            .iter()
            .position(|s| s.eq_ignore_ascii_case(state))
            .ok_or_else(|| {
                Error::new(
                    "invalid_arguments",
                    format!("States offered: {}", offered.join(", ")),
                )
            })?;
        if index >= u.visible_state_rows {
            return Err(Error::new(
                "gui_target_unavailable",
                format!(
                    "Only the first {} offered states are reachable: {}",
                    u.visible_state_rows,
                    offered[..u.visible_state_rows.min(offered.len())].join(", ")
                ),
            ));
        }
        let lower = state.to_lowercase();
        click_and_check(
            process,
            keys,
            (
                x0 + u.state_row.0,
                y0 + u.state_row.1 + u.state_row.2 * index as f64,
            ),
            "war goal state chosen",
            |r, i| {
                Ok(dialog_text(r, i, "wargoal_name")?
                    .to_lowercase()
                    .contains(&lower))
            },
        )?;
    }
    Ok(())
}

/// Take diplomatic action `action` toward the owner of state `state`; the
/// caller has journaled intent.
pub(super) fn act(
    process: &dyn Process,
    keys: &MacKeys,
    state: i32,
    action: &str,
    kind: Option<&str>,
    states: &[String],
) -> Result<()> {
    clear_notices(process, keys)?;
    if read(process, |r, i| Ok(dialog(r, i)?.is_some()))? {
        cancel_dialog(process, keys);
    }
    show(process, keys, state)?;
    let before = settled(process, "diplomatic actions", rows)?;
    let row = before
        .iter()
        .find(|r| r.name.eq_ignore_ascii_case(action))
        .cloned()
        .ok_or_else(|| Error::new("invalid_target", "This action is not offered"))?;
    if !row.enabled {
        return Err(Error::new(
            "invalid_target",
            "The game disables this action now",
        ));
    }
    let rect = row
        .rect
        .ok_or_else(|| Error::new("gui_target_unavailable", "Action row not on screen"))?;
    let pp_before = super::decisions::political_power(process)?;
    let (x, y) = rect.center();
    keys.click(GAME, x, y)?;
    std::thread::sleep(std::time::Duration::from_millis(400));
    if let Some((name, _)) = read(process, dialog)? {
        let flow = (|| -> Result<()> {
            if name == "generate_wargoal" {
                let kind = kind.ok_or_else(|| {
                    Error::new(
                        "invalid_arguments",
                        "war_goal (e.g. Conquer) and states are required for this action",
                    )
                })?;
                if states.is_empty() {
                    return Err(Error::new("invalid_arguments", "Choose 1 or more states"));
                }
                war_goal(process, keys, kind, states)?;
            }
            let (send, enabled) = settled(process, "send button", |r, i| {
                dialog_button(r, i, "send_button")
            })?;
            if !enabled {
                return Err(Error::new(
                    "invalid_target",
                    format!(
                        "The game does not allow sending: {}",
                        read(process, |r, i| dialog_text(r, i, "wargoal_warning"))
                            .unwrap_or_default()
                    ),
                ));
            }
            let (x, y) = send.center();
            keys.click(GAME, x, y)?;
            wait(process, "dialog sent", |r, i| Ok(dialog(r, i)?.is_none()))
        })();
        if let Err(error) = flow {
            cancel_dialog(process, keys);
            return Err(error);
        }
    }
    accept_command(process, keys)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        let now = read(process, |r, i| {
            if open(r, i)? {
                rows(r, i)
            } else {
                Ok(Vec::new())
            }
        })?;
        let changed = |a: &[Row], b: &[Row]| {
            a.len() != b.len()
                || a.iter()
                    .zip(b)
                    .any(|(x, y)| x.name != y.name || x.enabled != y.enabled)
        };
        if changed(&now, &before) || super::decisions::political_power(process)? < pp_before {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err(Error::new(
                "input_readback_failed",
                "diplomatic action: no change in the offered actions or political power",
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(60));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn markup_is_removed_from_texts() {
        assert_eq!(clean("\u{13}pol_power  \u{11}Y15\u{11}! "), "pol_power 15");
        assert_eq!(
            clean("Select \u{11}Y1\u{11}! - \u{11}Y5\u{11}! state(s)"),
            "Select 1 - 5 state(s)"
        );
    }
}
