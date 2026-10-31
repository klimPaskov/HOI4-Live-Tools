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
    view_root: 0x490,
    shown_tag: 0x1608,
    action_glue: 0x35e8,
    info_window: 0x8d8,
    grid_items: 0x190,
    item_container: 0x10,
    text: 0x100,
    container_vtable: 0x36ba978,
    container_base: 0x30,
    type_row: (-215.0, -216.0, 35.0),
    state_row: (30.0, -222.0, 30.0),
    visible_state_rows: 7,
};
const GAME: (u32, u32) = (1920, 1080);
/// Frame number of the diplomacy rows' `accept_icon` (1 decline, 2 accept).
const ICON_FRAME: u64 = 0x184;

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
pub(super) fn clean(text: &str) -> String {
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
pub(super) fn find(r: &Reader<'_>, image: u64, object: u64, name: &str, depth: u32) -> Result<Option<u64>> {
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
pub(super) struct Row {
    name: String,
    enabled: bool,
    cost: String,
    ai_refuses: bool,
    /// The row's accept icon shows the green check (the AI would accept).
    ai_accepts: bool,
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
        // The accept icon is shown for both answers; its frame tells them
        // apart (macOS 089EA519, read live 26 Oct 2026 against the screen:
        // 2 is the green check on Italy's non-aggression pact, 1 the red
        // cross on Poland's and on Italy's invitation to a faction).
        let (ai_refuses, ai_accepts) = match gui_tree::child(r, image, frame, "accept_icon")? {
            Some(icon) if gui_tree::shown(r, icon)? => match r.i32(icon + ICON_FRAME)? {
                2 => (false, true),
                _ => (true, false),
            },
            _ => (false, false),
        };
        out.push(Row {
            name,
            enabled,
            cost: text("cost")?,
            ai_refuses,
            ai_accepts,
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
/// Vertical scroll position (pixels, increasing as the list moves up) of a
/// dialog grid. macOS 089EA519: the grid object's float at +0x334 moved from
/// -496 to -481 for one wheel event, the list moving up by 15 screen pixels;
/// every event is one 15 px tick whatever its size, and the position persists
/// between dialogs (read live 26 October 2026 in the war-goal state list).
const GRID_SCROLL: u64 = 0x334;
fn grid_scroll(r: &Reader<'_>, image: u64, grid: &str) -> Result<f64> {
    let (_, window) =
        dialog(r, image)?.ok_or_else(|| Error::new("input_readback_failed", "Dialog closed"))?;
    let g = find(r, image, window, grid, 0)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Dialog list not found"))?;
    let value = f32::from_bits(r.i32(g + GRID_SCROLL)? as u32);
    if !value.is_finite() || value.abs() > 100_000.0 {
        return Err(Error::new("invalid_gui", "List scroll position unreadable"));
    }
    Ok(f64::from(value))
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

/// Who a diplomacy call is about: the owner of a state, or a country by tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Target {
    State(i32),
    Country(String),
}

/// Tag text of the target without touching the UI.
pub(super) fn target_tag(process: &dyn Process, target: &Target) -> Result<String> {
    match target {
        Target::Country(tag) => Ok(tag.clone()),
        Target::State(state) => read(process, |r, i| {
            super::states::tag_name(r, i, state_owner(r, i, *state)?)
        }),
    }
}

/// Point the diplomacy view at the target; returns its tag text. A country
/// is reached through one of its states (the first ones by id); the next
/// state is tried when the map click lands on another owner.
fn show_target(process: &dyn Process, keys: &MacKeys, target: &Target) -> Result<String> {
    match target {
        Target::State(state) => {
            let owner = show(process, keys, *state)?;
            read(process, |r, i| super::states::tag_name(r, i, owner))
        }
        Target::Country(tag) => {
            let (index, own, candidates) = read(process, |r, i| {
                Ok((
                    super::diplomacy_state::tag_index_of(r, i, tag)?,
                    player_tag(r, i)?,
                    super::diplomacy_state::states_owned_by(r, i, tag, 3)?,
                ))
            })?;
            if index == own {
                return Err(Error::new(
                    "invalid_target",
                    "This is the own country; name another country",
                ));
            }
            if read(process, |r, i| Ok(open(r, i)? && shown_tag(r, i)? == index))? {
                return Ok(tag.clone());
            }
            if candidates.is_empty() {
                return Err(Error::new(
                    "country_without_states",
                    format!("{tag} owns no state the diplomacy view can be pointed at"),
                ));
            }
            let mut last = None;
            for state in candidates {
                match show(process, keys, state) {
                    Ok(_) => return Ok(tag.clone()),
                    Err(e) if e.code == "input_readback_failed" => last = Some(e),
                    Err(e) => return Err(e),
                }
            }
            Err(last.unwrap_or_else(|| Error::new("gui_target_unavailable", "Country not shown")))
        }
    }
}
fn row_json(row: &Row) -> Value {
    json!({"action":row.name,"enabled":row.enabled,"cost":row.cost,"ai_would_refuse":row.ai_refuses,"ai_would_accept":row.ai_accepts,
        "key":super::diplomacy_keys::spec_of_row(&row.name).map(|s| s.key)})
}

/// Diplomatic actions offered toward the target.
pub(super) fn actions(process: &dyn Process, keys: &MacKeys, target: &Target) -> Result<Value> {
    clear_notices(process, keys)?;
    let tag = show_target(process, keys, target)?;
    let found = settled(process, "diplomatic actions", rows)?;
    let mut out = json!({"country":tag,"actions":found.iter().map(row_json).collect::<Vec<_>>()});
    if let Target::State(state) = target {
        out["state_id"] = json!(state);
    }
    Ok(out)
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
    let mut info = json!({});
    war_goal_choose(process, keys, kind, states, &mut info)
}
/// As `war_goal`, and note the states the game offers for the chosen type in
/// `info["states_offered"]`.
fn war_goal_choose(
    process: &dyn Process,
    keys: &MacKeys,
    kind: &str,
    states: &[String],
    info: &mut Value,
) -> Result<()> {
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
    info["states_offered"] = json!(offered);
    let mut baseline: Option<f64> = None;
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
        let lower = state.to_lowercase();
        // Rows are 30 px slots; the list is scrolled with the wheel until the
        // wanted row lies in the visible band, using the grid's own scroll
        // position, then the row is clicked.
        let slot = u.state_row.2;
        let band_lo = y0 + u.state_row.1 - 4.0;
        let band_hi = y0 + u.state_row.1 + slot * u.visible_state_rows as f64 - 14.0;
        if baseline.is_none() {
            // The list keeps its scroll position between dialogs and a wheel
            // event moves it one 15 px tick whatever its size: go to the top
            // (the position stops changing) so that row 0 sits at the first
            // slot; the position read there is the reference for every offset.
            let mut top = read(process, |r, i| grid_scroll(r, i, "additionals_grid"))?;
            for _ in 0..60 {
                keys.scroll(GAME, (x0 + u.state_row.0, y0 + u.state_row.1 + 90.0), 40)?;
                let now = read(process, |r, i| grid_scroll(r, i, "additionals_grid"))?;
                if (now - top).abs() < 0.5 {
                    break;
                }
                top = now;
            }
            baseline = Some(top);
        }
        let base_scroll = baseline.unwrap_or(0.0);
        let mut at = None;
        let mut seen = String::new();
        for _ in 0..48 {
            let now = read(process, |r, i| grid_scroll(r, i, "additionals_grid"))?;
            let y = y0 + u.state_row.1 + slot * index as f64 - (now - base_scroll);
            seen = format!("row at y={y:.0}, visible {band_lo:.0}..{band_hi:.0}");
            if y >= band_lo && y <= band_hi {
                at = Some((x0 + u.state_row.0, y));
                break;
            }
            // positive wheel moves the list down (towards the top)
            let wheel = if y > band_hi { -40 } else { 40 };
            keys.scroll(GAME, (x0 + u.state_row.0, (band_lo + band_hi) / 2.0), wheel)?;
        }
        let at = at.ok_or_else(|| {
            Error::new(
                "gui_target_unavailable",
                format!("The state {state} could not be scrolled into view ({seen})"),
            )
        })?;
        click_and_check(process, keys, at, "war goal state chosen", |r, i| {
            Ok(dialog_text(r, i, "wargoal_name")?
                .to_lowercase()
                .contains(&lower))
        })
        .map_err(|e| {
            Error::new(
                e.code,
                format!("{} (clicked x={:.0} y={:.0}; {seen}; list scroll base {base_scroll:.0})", e.message, at.0, at.1),
            )
        })?;
    }
    Ok(())
}

/// Dialog windows whose Send button is the whole interaction (or, for war
/// goals, after the type and states are chosen). Others need choices this
/// tool cannot make and are cancelled with `dialog_unsupported`.
const SENDABLE: [&str; 7] = [
    "generate_wargoal",
    "add_wargoal",
    "declare_war",
    "guarantee",
    "standard_diplomacy_controller",
    "invite_faction_diplomacy_controller",
    "naval_blockade",
];
const TEXT_FIELDS: [&str; 6] = [
    "title",
    "description",
    "diplo_reply",
    "wargoal_name",
    "wargoal_warning",
    "threat_value",
];

/// What the shown dialog says, field by field (empty fields left out).
fn dialog_texts(process: &dyn Process) -> Value {
    let mut out = serde_json::Map::new();
    for field in TEXT_FIELDS {
        if let Ok(text) = read(process, |r, i| dialog_text(r, i, field)) {
            if !text.is_empty() {
                out.insert(field.to_string(), json!(text));
            }
        }
    }
    Value::Object(out)
}

/// An action chosen and checked, with the state before it.
pub(super) struct Plan {
    pub tag: String,
    pub row: Row,
    pub spec: Option<&'static super::diplomacy_keys::Spec>,
    pub before: super::diplomacy_keys::Snapshot,
}

fn disabled_hint(key: Option<&str>) -> &'static str {
    match key {
        Some("declare_war") => {
            "Declaring war needs a completed war goal against this country (justify_war_goal, then wait for the justification to finish) or another valid reason (a call to arms, a guarantee to a country at war with it)"
        }
        Some("justify_war_goal") | Some("add_war_goal") => {
            "A war goal needs enough political power, no war goal already against this country, and a valid target (not an ally, not a subject of an ally)"
        }
        Some("guarantee") => "Guarantees need political power and a country that is not already guaranteed by you and not in your faction",
        Some("invite_to_faction") | Some("join_faction") => {
            "Faction invitations need a faction the player leads (see hoi4_faction_create), an ideology the faction rules allow, and no war against the country"
        }
        Some("send_volunteers") | Some("expeditionary_force") => "Needs the country to be at war and an ally or a faction-rule permission",
        _ => "The game disables this action now (its tooltip names the reason)",
    }
}

/// Open the view on the target and choose the row `action` names.
fn locate(
    process: &dyn Process,
    keys: &MacKeys,
    target: &Target,
    action: &str,
) -> Result<(String, Vec<Row>, Row, Option<&'static super::diplomacy_keys::Spec>)> {
    clear_notices(process, keys)?;
    let tag = show_target(process, keys, target)?;
    if read(process, |r, i| Ok(dialog(r, i)?.is_some()))? {
        cancel_dialog(process, keys);
    }
    let rows_now = settled(process, "diplomatic actions", rows)?;
    let names: Vec<String> = rows_now.iter().map(|r| r.name.clone()).collect();
    let (name, spec) = super::diplomacy_keys::resolve(action, &names)?;
    let row = rows_now
        .iter()
        .find(|r| r.name == name)
        .cloned()
        .ok_or_else(|| Error::new("action_not_offered", "Row vanished"))?;
    Ok((tag, rows_now, row, spec))
}

/// All typed refusals that need no input beyond opening the view.
pub(super) fn plan(
    process: &dyn Process,
    keys: &MacKeys,
    target: &Target,
    action: &str,
    try_anyway: bool,
) -> Result<Plan> {
    let (tag, _, row, spec) = locate(process, keys, target, action)?;
    let before = read(process, |r, i| {
        super::diplomacy_state::snapshot(r, i, &tag)
    })?;
    if !row.enabled {
        let mut message = format!(
            "The game disables '{}' toward {tag} now. {}.",
            row.name,
            disabled_hint(spec.map(|s| s.key))
        );
        let mut code = "action_disabled";
        if matches!(spec.map(|s| s.key), Some("declare_war")) {
            code = "war_goal_required";
            let goals: Vec<String> = read(process, super::diplomacy_state::own_goals)
                .unwrap_or_default()
                .into_iter()
                .filter(|(_, t, _)| *t == tag)
                .map(|(_, t, k)| format!("{} vs {t}", k.unwrap_or_else(|| "goal".into())))
                .collect();
            message.push_str(&if goals.is_empty() {
                " Own war goals against this country: none.".to_string()
            } else {
                format!(" Own war goals against this country (not yet completed or expired): {}.", goals.join("; "))
            });
        }
        return Err(Error::new(code, message));
    }
    if let Some(spec) = spec {
        if super::diplomacy_keys::already(spec.effect, &before) {
            return Err(Error::new(
                "nothing_to_change",
                format!("'{}' toward {tag} already holds", spec.key),
            ));
        }
        if spec.ai_decides && row.ai_refuses && !try_anyway {
            return Err(Error::new(
                "ai_would_refuse",
                format!(
                    "The game marks '{}' toward {tag} as one the country would refuse (the red cross on the row); improve relations first, or pass try_anyway:true to send it regardless",
                    row.name
                ),
            ));
        }
    }
    Ok(Plan { tag, row, spec, before })
}

/// The dispatch's own evidence after sending.
pub(super) struct Proof {
    pub rows_changed: bool,
    pub pp_spent: bool,
    pub relation: bool,
    pub dialog: Value,
}

fn rows_differ(a: &[Row], b: &[Row]) -> bool {
    a.len() != b.len()
        || a.iter()
            .zip(b)
            .any(|(x, y)| x.name != y.name || x.enabled != y.enabled)
}

/// Click the planned row, finish its dialog, and wait up to 3 s for the
/// evidence. `sent` is set just before the first input that can change the
/// game (the Send click, or the row click of an action without a dialog).
pub(super) fn execute(
    process: &dyn Process,
    keys: &MacKeys,
    plan: &Plan,
    kind: Option<&str>,
    states: &[String],
    sent: &std::cell::Cell<bool>,
) -> Result<Proof> {
    let before_rows = settled(process, "diplomatic actions", rows)?;
    let row = before_rows
        .iter()
        .find(|r| r.name == plan.row.name)
        .cloned()
        .ok_or_else(|| Error::new("action_not_offered", "The row is no longer offered"))?;
    if !row.enabled {
        return Err(Error::new("action_disabled", "The game disables this action now"));
    }
    let rect = row
        .rect
        .ok_or_else(|| Error::new("gui_target_unavailable", "Action row not on screen"))?;
    let pp_before = super::decisions::political_power(process)?;
    let known_dialog = plan.spec.map(|s| s.dialog).unwrap_or(false);
    if !known_dialog {
        sent.set(true);
    }
    let (x, y) = rect.center();
    keys.click(GAME, x, y)?;
    std::thread::sleep(std::time::Duration::from_millis(400));
    let mut texts = Value::Null;
    if let Some((name, _)) = read(process, dialog)? {
        let flow = (|| -> Result<Value> {
            if !SENDABLE.contains(&name.as_str()) {
                return Err(Error::new(
                    "dialog_unsupported",
                    format!("The action opened the dialog '{name}', which needs choices this tool does not make"),
                ));
            }
            let cancelling = name == "generate_wargoal"
                && read(process, |r, i| dialog_text(r, i, "title"))?
                    .to_lowercase()
                    .contains("cancel");
            if (name == "generate_wargoal" || name == "add_wargoal") && !cancelling {
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
            let texts = dialog_texts(process);
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
            sent.set(true);
            keys.click(GAME, x, y)?;
            wait(process, "dialog sent", |r, i| Ok(dialog(r, i)?.is_none()))?;
            Ok(texts)
        })();
        match flow {
            Ok(t) => texts = t,
            Err(error) => {
                cancel_dialog(process, keys);
                return Err(error);
            }
        }
    } else {
        sent.set(true);
    }
    accept_command(process, keys)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    let ai = plan.spec.map(|s| s.ai_decides).unwrap_or(false);
    loop {
        let rows_now = read(process, |r, i| {
            if open(r, i)? {
                rows(r, i)
            } else {
                Ok(Vec::new())
            }
        })?;
        let rows_changed = rows_differ(&rows_now, &before_rows);
        let pp_spent = super::decisions::political_power(process)? < pp_before;
        let snap = read(process, |r, i| super::diplomacy_state::snapshot(r, i, &plan.tag))?;
        let relation = super::diplomacy_keys::verdict(plan.spec, Some(&plan.before), &snap, false)
            == super::settle::Verdict::Reached
            && !matches!(plan.spec.map(|s| s.effect), Some(super::diplomacy_keys::Effect::RowsOnly));
        if relation || ((rows_changed || pp_spent) && !ai) || std::time::Instant::now() >= deadline {
            return Ok(Proof { rows_changed, pp_spent, relation, dialog: texts });
        }
        std::thread::sleep(std::time::Duration::from_millis(80));
    }
}

/// Read the dialog an action opens (cost, days and options the game shows)
/// without sending it. For a war goal the type and states are chosen first
/// when given; the dialog is always cancelled.
pub(super) fn preview(
    process: &dyn Process,
    keys: &MacKeys,
    target: &Target,
    action: &str,
    kind: Option<&str>,
    states: &[String],
) -> Result<Value> {
    let (tag, _, row, spec) = locate(process, keys, target, action)?;
    let mut out = json!({"country":tag,"row":row_json(&row),"opens_dialog":spec.map(|s| s.dialog)});
    if !row.enabled || !spec.map(|s| s.dialog).unwrap_or(false) {
        out["previewed"] = json!(false);
        out["reason"] = json!(if row.enabled {
            "this action acts when its row is clicked; there is no dialog to read"
        } else {
            disabled_hint(spec.map(|s| s.key))
        });
        return Ok(out);
    }
    let rect = row
        .rect
        .ok_or_else(|| Error::new("gui_target_unavailable", "Action row not on screen"))?;
    let (x, y) = rect.center();
    keys.click(GAME, x, y)?;
    std::thread::sleep(std::time::Duration::from_millis(400));
    let Some((name, _)) = read(process, dialog)? else {
        out["previewed"] = json!(false);
        out["reason"] = json!("no dialog opened");
        return Ok(out);
    };
    let flow = (|| -> Result<Value> {
        let mut info = json!({"dialog":name});
        let cancelling = name == "generate_wargoal"
            && read(process, |r, i| dialog_text(r, i, "title"))?
                .to_lowercase()
                .contains("cancel");
        if (name == "generate_wargoal" || name == "add_wargoal") && !cancelling {
            let types = read(process, |r, i| dialog_grid(r, i, "wargoals_grid"))?;
            info["war_goal_types"] = json!(types);
            if let Some(kind) = kind {
                war_goal_choose(process, keys, kind, states, &mut info)?;
            }
        }
        info["texts"] = dialog_texts(process);
        info["send_enabled"] = json!(
            read(process, |r, i| dialog_button(r, i, "send_button"))
                .map(|(_, e)| e)
                .unwrap_or(false)
        );
        Ok(info)
    })();
    cancel_dialog(process, keys);
    let info = flow?;
    out["previewed"] = json!(true);
    out["dialog_info"] = info;
    Ok(out)
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
