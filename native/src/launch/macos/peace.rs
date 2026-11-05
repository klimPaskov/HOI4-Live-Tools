//! Peace conference actions with ordinary input. NOT validated live: no
//! conference could be arranged before the task-port authorization expired
//! (the qualification game was at peace, a 250-day war goal needs more game
//! time than the session had). Everything here follows the route documented
//! in docs/PLAYBOOK_TOOL_GAPS.md 13a (live on 26 Oct 2026 with Luxembourg:
//! "Take State" preselected, a click on the state on the map adds the demand
//! and lowers the score, "Submit Demands", then "Confirm and Exit" and OK) and
//! the element names of `interface/peaceconferencewindow.gui`. Every output
//! carries `validated:false`; the live procedure is in the PLAYBOOK section.
//!
//! Reads never move the cursor. Actions: choose the demand kind (action
//! button), centre each state with the Find view and click it, verify through
//! the window's own score text; submit with `end_turn_button`; leave with
//! `done_button`. No input is retried after a possible input.
use super::diplomacy::{clean, find};
use super::events::peace_root;
use super::panelkit as pk;
use super::ui::{dismiss_notices, find_state, read, wait};
use crate::{
    error::{Error, Result},
    game,
    gui_tree::{self, Rect},
    memory::Reader,
    platform::{game_input::GameInput, Process},
    xlayout::{self, fld},
};
use serde_json::{json, Map, Value};
use std::time::{Duration, Instant};

const GAME: (u32, u32) = (1920, 1080);
const TEXT: u64 = 0x100;
/// Owner of the text field (no reviewed text-box vtable).
const TEXT_BOX: xlayout::Class = xlayout::Class::new("gui.text", 0);
/// Texts of the window's header (current_participant_window, bidphase_panel).
const HEADER: [&str; 6] = [
    "current_player",
    "current_score",
    "tension",
    "bidphase_text",
    "scorethisturn_text",
    "remainingturns_text",
];
/// Texts inside list entries (participants, aggregated actions, bids).
const ENTRY_TEXTS: [&str; 14] = [
    "country_name",
    "score",
    "number_of_bids",
    "number_of_contested_bids",
    "peacescore_ratio",
    "action_name",
    "cost",
    "number_of_available_states",
    "number_of_wargoals",
    "number_of_contested_states",
    "nb_biddings",
    "underlying_resource_name",
    "label",
    "number_of_items",
];
/// Lists whose entries are read.
const LISTS: [&str; 3] = [
    "country_biddings_list",
    "available_actions_list",
    "biddings_grid",
];
const BUTTONS: [&str; 17] = [
    "take_state_action",
    "liberate_action",
    "puppet_action",
    "force_government_action",
    "take_navy_action",
    "demilitarize_zone_action",
    "resource_rights_action",
    "war_reparation_action",
    "dismantle_industry_action",
    "end_turn_button",
    "done_button",
    "reset_button",
    "victors_tab",
    "beneficiaries_tab",
    "bid_button",
    "forfeit_button",
    "select_all_button",
];

/// What a take call demands from the states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Kind {
    Take,
    Puppet,
    Liberate,
}
impl Kind {
    pub(super) fn parse(puppet: bool, release: bool) -> Result<Kind> {
        match (puppet, release) {
            (false, false) => Ok(Kind::Take),
            (true, false) => Ok(Kind::Puppet),
            (false, true) => Ok(Kind::Liberate),
            (true, true) => Err(Error::new(
                "invalid_arguments",
                "puppet and release are exclusive",
            )),
        }
    }
    fn button(self) -> &'static str {
        match self {
            Kind::Take => "take_state_action",
            Kind::Puppet => "puppet_action",
            Kind::Liberate => "liberate_action",
        }
    }
    pub(super) fn name(self) -> &'static str {
        match self {
            Kind::Take => "take_state",
            Kind::Puppet => "puppet",
            Kind::Liberate => "liberate",
        }
    }
}

/// 1..=8 distinct state ids in 1..=4095.
pub(super) fn validate_states(ids: &[i64]) -> Result<Vec<i32>> {
    if ids.is_empty() || ids.len() > 8 {
        return Err(Error::new("invalid_arguments", "states must hold 1..8 ids"));
    }
    let mut out: Vec<i32> = Vec::new();
    for &id in ids {
        if !(1..4096).contains(&id) {
            return Err(Error::new("invalid_arguments", "state ids must be 1..4095"));
        }
        if out.contains(&(id as i32)) {
            return Err(Error::new("invalid_arguments", "duplicate state id"));
        }
        out.push(id as i32);
    }
    Ok(out)
}

/// First number in a window text ("95", "-3.5", "Score: 68 / 95"), markup
/// already removed.
pub(super) fn parse_number(text: &str) -> Option<f64> {
    let b: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let sign = c == '-' && b.get(i + 1).is_some_and(char::is_ascii_digit);
        if c.is_ascii_digit() || sign {
            let mut j = i + 1;
            while j < b.len()
                && (b[j].is_ascii_digit()
                    || (b[j] == '.' && b.get(j + 1).is_some_and(char::is_ascii_digit)))
            {
                j += 1;
            }
            return b[i..j].iter().collect::<String>().parse().ok();
        }
        i += 1;
    }
    None
}

/// A button the player can press: shown and not disabled.
pub(super) fn usable(shown: bool, disabled: bool) -> bool {
    shown && !disabled
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Settle {
    Reached,
    Unchanged,
    Diverged,
    Unknown,
}
/// Take settle: compare only the score the window shows. A successful take
/// lowers it; unchanged without a sent click is a clean no-op; an equal score
/// after a click, or a rise, is a divergence.
pub(super) fn take_settle(
    before: Option<f64>,
    now: Option<f64>,
    sent: bool,
    retry: bool,
) -> Settle {
    match (before, now) {
        (Some(b), Some(n)) if n < b - 1e-9 => Settle::Reached,
        (Some(b), Some(n)) if (n - b).abs() <= 1e-9 && !sent && !retry => Settle::Unchanged,
        (None, Some(_)) if retry => Settle::Unknown,
        (_, Some(_)) => Settle::Diverged,
        (_, None) => Settle::Unknown,
    }
}
/// Submit settle: the header texts (phase, turn, score of the turn) changed or
/// the window closed.
pub(super) fn submit_settle(before: &str, now: Option<&str>, sent: bool) -> Settle {
    match now {
        None => Settle::Reached,
        Some(n) if n != before => Settle::Reached,
        Some(_) if !sent => Settle::Unchanged,
        Some(_) => Settle::Diverged,
    }
}

fn text_at(r: &Reader<'_>, node: u64) -> Option<String> {
    let text = fld(r.string_abi(), &TEXT_BOX, TEXT).ok()?;
    r.string_bounded(node + text, 256).ok().map(|t| clean(&t))
}
fn flat_texts(
    r: &Reader<'_>,
    image: u64,
    object: u64,
    depth: u32,
    out: &mut Map<String, Value>,
) -> Result<()> {
    for child in gui_tree::children(r, object)? {
        let Some(name) = gui_tree::name(r, child, image)? else {
            continue;
        };
        if ENTRY_TEXTS.contains(&name.as_str()) {
            if let Some(t) = text_at(r, child) {
                let mut key = name.clone();
                let mut n = 2;
                while out.contains_key(&key) {
                    key = format!("{name}#{n}");
                    n += 1;
                }
                out.insert(key, json!(t));
            }
        }
        if depth < 6 {
            if let Some(inner) = gui_tree::container(r, child, image)? {
                flat_texts(r, image, inner, depth + 1, out)?;
            }
        }
    }
    Ok(())
}
fn root(r: &Reader<'_>, image: u64) -> Result<u64> {
    peace_root(r, image)?
        .ok_or_else(|| Error::new("no_conference_open", "No peace conference is open"))
}
fn header(r: &Reader<'_>, image: u64, root: u64) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    for name in HEADER {
        if let Some(t) = find(r, image, root, name, 0)? {
            if let Some(text) = text_at(r, t) {
                out.insert(name.to_string(), json!(text));
            }
        }
    }
    Ok(out)
}
#[derive(Clone, Debug, PartialEq)]
struct Button {
    shown: bool,
    disabled: bool,
    selected: bool,
    rect: Option<Rect>,
}
fn button(r: &Reader<'_>, image: u64, root: u64, name: &str) -> Result<Option<Button>> {
    let Some(node) = find(r, image, root, name, 0)? else {
        return Ok(None);
    };
    Ok(Some(Button {
        shown: gui_tree::shown(r, node)?,
        disabled: gui_tree::disabled(r, node)?,
        selected: gui_tree::button_state(r, node)? == 2,
        rect: gui_tree::rect(r, image, node, GAME).ok(),
    }))
}
fn need_button(r: &Reader<'_>, image: u64, root: u64, name: &str) -> Result<Button> {
    button(r, image, root, name)?.ok_or_else(|| {
        Error::new(
            "gui_target_unavailable",
            format!("Conference control {name} not found"),
        )
    })
}
fn score(r: &Reader<'_>, image: u64) -> Result<Option<f64>> {
    let root = root(r, image)?;
    Ok(header(r, image, root)?
        .get("current_score")
        .and_then(Value::as_str)
        .and_then(parse_number))
}
fn header_key(r: &Reader<'_>, image: u64) -> Result<Option<String>> {
    let Some(root) = peace_root(r, image)? else {
        return Ok(None);
    };
    let h = header(r, image, root)?;
    Ok(Some(
        ["bidphase_text", "scorethisturn_text", "remainingturns_text"]
            .iter()
            .map(|k| h.get(*k).and_then(Value::as_str).unwrap_or(""))
            .collect::<Vec<_>>()
            .join("|"),
    ))
}
pub(super) fn score_now(process: &dyn Process) -> Result<Option<f64>> {
    // A closed window (conference over) has no score.
    match read(process, score) {
        Err(e) if e.code == "no_conference_open" => Ok(None),
        other => other,
    }
}
pub(super) fn is_open(process: &dyn Process) -> Result<bool> {
    read(process, |r, i| Ok(peace_root(r, i)?.is_some()))
}

fn state_owner_controller(r: &Reader<'_>, image: u64, state: i32) -> Result<(i32, i32)> {
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
        return Err(Error::new(
            "unknown_state",
            format!("State {state} is not in the catalog"),
        ));
    }
    Ok((
        r.i32(object + l.state_owner)?,
        r.i32(object + l.state_controller)?,
    ))
}
fn player(r: &Reader<'_>, image: u64) -> Result<i32> {
    let g = game::layout(r);
    r.i32(r.u64(image + g.gs_global)? + g.player_tag)
}

/// The open conference as the window shows it, plus owner and occupier of
/// the asked states. Costs are not stored in the window; they are shown in the
/// tooltip of a state and as the drop of `current_score` after a take.
pub(super) fn options(process: &dyn Process, states: &[i32]) -> Result<Value> {
    let (open, header_map, buttons, lists, state_rows) = read(process, |r, image| {
        let Some(root) = peace_root(r, image)? else {
            return Ok((false, Map::new(), Map::new(), Map::new(), Vec::new()));
        };
        let header_map = header(r, image, root)?;
        let mut buttons = Map::new();
        for name in BUTTONS {
            if let Some(b) = button(r, image, root, name)? {
                buttons.insert(
                    name.to_string(),
                    json!({"shown":b.shown,"enabled":b.shown && !b.disabled,"selected":b.selected}),
                );
            }
        }
        let mut lists = Map::new();
        for list in LISTS {
            if let Some(node) = find(r, image, root, list, 0)? {
                let mut entries = Vec::new();
                if let Some(inner) = gui_tree::container(r, node, image)? {
                    for child in gui_tree::children(r, inner)?.into_iter().take(64) {
                        let mut texts = Map::new();
                        if let Some(name) = gui_tree::name(r, child, image)? {
                            if ENTRY_TEXTS.contains(&name.as_str()) {
                                if let Some(t) = text_at(r, child) {
                                    texts.insert(name, json!(t));
                                }
                            }
                        }
                        if let Some(item) = gui_tree::container(r, child, image)? {
                            flat_texts(r, image, item, 0, &mut texts)?;
                        }
                        if !texts.is_empty() {
                            entries.push(Value::Object(texts));
                        }
                    }
                }
                lists.insert(list.to_string(), json!(entries));
            }
        }
        let own = player(r, image)?;
        let mut rows = Vec::new();
        for &state in states {
            rows.push(match state_owner_controller(r, image, state) {
                Ok((owner, controller)) => json!({
                    "state_id": state,
                    "owner": super::states::tag_name(r, image, owner)?,
                    "controller": super::states::tag_name(r, image, controller)?,
                    "own": owner == own,
                    "occupied_by_us": controller == own && owner != own,
                }),
                Err(e) => json!({"state_id": state, "error": e.code}),
            });
        }
        Ok((true, header_map, buttons, lists, rows))
    })?;
    if !open {
        return Ok(json!({"peace_conference_open":false,"validated":false,
            "next":"no conference; one opens when a war you fight ends (hoi4_events reports peace_conference_open)"}));
    }
    let remaining = header_map
        .get("current_score")
        .and_then(Value::as_str)
        .and_then(parse_number);
    Ok(json!({
        "peace_conference_open": true,
        "validated": false,
        "header": header_map,
        "own_remaining_points": remaining,
        "controls": buttons,
        "lists": lists,
        "states": state_rows,
        "cost_note": "State costs are not in the window; the tooltip of a state on the map shows them and hoi4_peace_take reports each state's cost as the drop of the score. hoi4_war_goal_targets ranks the states.",
        "actions": ["hoi4_peace_take {states, puppet?, release?}", "hoi4_peace_submit", "hoi4_peace_exit"],
    }))
}

pub(super) struct Plan {
    pub kind: Kind,
    pub states: Vec<i32>,
    /// English names of `states` (same order), matched against the conference's state list.
    pub names: Vec<String>,
    pub max_cost: Option<f64>,
    pub score_before: Option<f64>,
}
/// Typed refusals before any input: no conference, round not ready, the
/// demand kind not available, a state unknown, own, or already ours.
pub(super) fn plan_take(
    process: &dyn Process,
    kind: Kind,
    states: &[i32],
    max_cost: Option<f64>,
) -> Result<Plan> {
    read(process, |r, image| {
        let root = root(r, image)?;
        let turn = need_button(r, image, root, "end_turn_button")?;
        if !usable(turn.shown, turn.disabled) {
            return Err(Error::new(
                "round_not_ready",
                "Demands cannot be changed now (end turn button hidden or disabled; wait for the other victors or the next round)",
            ));
        }
        let action = need_button(r, image, root, kind.button())?;
        if !usable(action.shown, action.disabled) {
            return Err(Error::new(
                "action_not_available",
                format!("{} is not available in this conference", kind.name()),
            ));
        }
        let own = player(r, image)?;
        for &state in states {
            let (owner, controller) = state_owner_controller(r, image, state)?;
            if owner == own {
                return Err(Error::new(
                    "state_not_offered",
                    format!("State {state} is already ours"),
                ));
            }
            if owner <= 0 {
                return Err(Error::new(
                    "state_not_offered",
                    format!("State {state} has no owner"),
                ));
            }
            let _ = controller;
        }
        let score_before = score(r, image)?;
        if score_before.is_some_and(|s| s <= 0.0) {
            return Err(Error::new(
                "cost_exceeds_points",
                "No points left this round",
            ));
        }
        Ok(Plan {
            names: Vec::new(),
            kind,
            states: states.to_vec(),
            max_cost,
            score_before,
        })
    })
}

/// Choose the demand kind and add each state. `sent` is set before the first
/// click that can change the demands. Each state must lower the window's
/// score within 3 s, else the call stops with `state_not_added` (no retry,
/// a second click could toggle the demand off).
pub(super) fn take(
    process: &dyn Process,
    keys: &dyn GameInput,
    plan: &Plan,
    sent: &std::cell::Cell<bool>,
) -> Result<Value> {
    let (action, selected) = read(process, |r, image| {
        let root = root(r, image)?;
        let b = need_button(r, image, root, plan.kind.button())?;
        Ok((b.rect, b.selected))
    })?;
    if !selected {
        let rect = action
            .ok_or_else(|| Error::new("gui_target_unavailable", "Action button is off screen"))?;
        let (x, y) = rect.center();
        sent.set(true);
        keys.click(GAME, x, y)?;
        std::thread::sleep(Duration::from_millis(300));
    }
    let mut costs = Vec::new();
    let mut last = plan.score_before;
    for (n, &state) in plan.states.iter().enumerate() {
        sent.set(true);
        let name = plan.names.get(n).cloned().unwrap_or_default();
        // The conference hides the Find view: pick the state from the window's own list
        // (country rows expand into state rows), else fall back to centring it on the map.
        match click_state_row(process, keys, &name) {
            Ok(()) => {}
            Err(e)
                if e.code == "state_row_missing"
                    && std::env::var_os("HOI4_PEACE_LIST_DEBUG").is_none() =>
            {
                find_state(process, keys, state)?;
                keys.click(GAME, 960.0, 540.0)?;
            }
            Err(e) => return Err(e),
        }
        let before = last;
        let changed = wait(process, "demand added", |r, i| {
            let now = score(r, i)?;
            Ok(matches!((before, now), (Some(b), Some(n)) if n < b - 1e-9))
        });
        let now = read(process, score)?;
        if changed.is_err() {
            return Err(Error::new(
                "state_not_added",
                format!(
                    "Clicking state {state} did not lower the score (not offered, not selectable on the map, or already demanded); added so far: {costs:?}"
                ),
            ));
        }
        let cost = match (before, now) {
            (Some(b), Some(n)) => Some(((b - n) * 1000.0).round() / 1000.0),
            _ => None,
        };
        if let (Some(c), Some(max)) = (cost, plan.max_cost) {
            if c > max {
                return Err(Error::new(
                    "cost_exceeds_points",
                    format!("State {state} cost {c} above max_cost {max}; the demand is pending in the window (click the state again or reset to remove it)"),
                ));
            }
        }
        costs.push(json!({"state_id":state,"cost":cost,"score_after":now}));
        last = now;
    }
    Ok(
        json!({"kind":plan.kind.name(),"states":costs,"score_before":plan.score_before,"score_after":last}),
    )
}

/// Rows of the conference's available-actions list: (country rows: name and expand button,
/// state rows: name and a point on the row). peaceconferencewindow.gui: aggregated_action_item
/// (header with action_name and expand) holding available_actions_grid of available_action_item
/// (country_flag at x 18, action_name at x 110, 29 px high).
#[allow(clippy::type_complexity)]
fn list_rows(
    r: &crate::memory::Reader<'_>,
    image: u64,
) -> Result<(Vec<(String, Rect)>, Vec<(String, Rect)>)> {
    let root = root(r, image)?;
    let node = find(r, image, root, "available_actions_list", 0)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "available_actions_list not found"))?;
    let list = gui_tree::container(r, node, image)?.ok_or_else(|| {
        Error::new(
            "gui_target_unavailable",
            "available_actions_list is not a window",
        )
    })?;
    let (mut countries, mut states) = (Vec::new(), Vec::new());
    // Country rows are the list's child windows; their state rows ("Select All" first) sit in
    // an inner grid at y 54 with 29 px slots. The row icons have no reviewed renderer, so the
    // rows are placed from the country row's expand button at (266, 30).
    for child in gui_tree::children(r, list)?.into_iter().take(64) {
        let Some(item) = gui_tree::container(r, child, image)? else {
            continue;
        };
        let name = pk::text(r, image, item, "action_name", 3)?.unwrap_or_default();
        let Ok(expand) = pk::rect_of(r, image, item, "expand", 3) else {
            continue;
        };
        countries.push((name, expand));
        let (ox, oy) = (expand.x - 266, expand.y - 30);
        let mut grids = Vec::new();
        pk::grids(r, image, item, 3, &mut grids)?;
        for grid in grids {
            for (k, row) in pk::items(r, image, grid)?.into_iter().enumerate() {
                let state = pk::text(r, image, row.frame, "action_name", 2)?.unwrap_or_default();
                if state.is_empty() {
                    continue;
                }
                states.push((
                    state,
                    Rect {
                        x: ox + 140,
                        y: oy + 54 + 29 * k as i32 + 7,
                        w: 40,
                        h: 14,
                    },
                ));
            }
        }
    }
    Ok((countries, states))
}
/// Click the state row named `name` in the conference list, expanding country rows first.
fn click_state_row(process: &dyn Process, keys: &dyn GameInput, name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::new("state_row_missing", "No state name"));
    }
    let wanted = |rows: &[(String, Rect)]| {
        rows.iter()
            .find(|(n, _)| super::loc::same(n, name))
            .map(|(_, r)| *r)
    };
    let (countries, states) = read(process, list_rows)?;
    let mut target = wanted(&states);
    if target.is_none() {
        for (_, expand) in &countries {
            let (x, y) = expand.center();
            keys.click(GAME, x, y)?;
            std::thread::sleep(Duration::from_millis(350));
            target = wanted(&read(process, list_rows)?.1);
            if target.is_some() {
                break;
            }
        }
    }
    let rect = match target {
        Some(r) => r,
        None => {
            let (c, s) = read(process, list_rows).unwrap_or_default();
            return Err(Error::new(
                "state_row_missing",
                format!(
                    "{name} is not in the conference list (countries {:?}, states {:?})",
                    c.iter().map(|x| &x.0).collect::<Vec<_>>(),
                    s.iter().map(|x| &x.0).collect::<Vec<_>>()
                ),
            ));
        }
    };
    // Text-drawn buttons report their text sprite (864 px wide on 1.19.4.1), whose centre
    // can fall off screen; their left edge is the button's, so click 120 px in.
    let (mut x, y) = rect.center();
    if rect.w > 400 {
        x = f64::from(rect.x) + 120.0;
    }
    keys.click(GAME, x, y)
        .map_err(|e| Error::new(e.code, format!("{} ({name} at {rect:?})", e.message)))?;
    Ok(())
}

/// Pre-input check shared by submit and exit.
pub(super) fn plan_button(
    process: &dyn Process,
    name: &'static str,
    code: &'static str,
) -> Result<String> {
    read(process, |r, image| {
        let root = root(r, image)?;
        let b = need_button(r, image, root, name)?;
        if !usable(b.shown, b.disabled) {
            return Err(Error::new(code, format!("{name} is not usable now")));
        }
        Ok(header_key(r, image)?.unwrap_or_default())
    })
}
pub(super) fn plan_submit(process: &dyn Process) -> Result<String> {
    plan_button(process, "end_turn_button", "round_not_ready")
}
pub(super) fn plan_exit(process: &dyn Process) -> Result<String> {
    plan_button(process, "done_button", "exit_not_available")
}
pub(super) fn header_snapshot(process: &dyn Process) -> Result<Option<String>> {
    read(process, header_key)
}

fn click_button(process: &dyn Process, keys: &dyn GameInput, name: &str) -> Result<()> {
    let rect = read(process, |r, image| {
        let root = root(r, image)?;
        let b = need_button(r, image, root, name)?;
        if !usable(b.shown, b.disabled) {
            return Err(Error::new(
                "round_not_ready",
                format!("{name} is not usable now"),
            ));
        }
        b.rect
            .ok_or_else(|| Error::new("gui_target_unavailable", format!("{name} is off screen")))
    })?;
    // Text-drawn buttons report their text sprite (864 px wide on 1.19.4.1), whose centre
    // can fall off screen; their left edge is the button's, so click 120 px in.
    let (mut x, y) = rect.center();
    if rect.w > 400 {
        x = f64::from(rect.x) + 120.0;
    }
    keys.click(GAME, x, y)
        .map_err(|e| Error::new(e.code, format!("{} ({name} at {rect:?})", e.message)))?;
    Ok(())
}
/// One click on Submit Demands; verified when the header texts change or the
/// window closes. A wait window ("waiting for players") also changes them.
pub(super) fn submit(
    process: &dyn Process,
    keys: &dyn GameInput,
    before: &str,
    sent: &std::cell::Cell<bool>,
) -> Result<Value> {
    sent.set(true);
    click_button(process, keys, "end_turn_button")?;
    wait(process, "demands submitted", |r, i| {
        Ok(match header_key(r, i)? {
            None => true,
            Some(now) => now != before,
        })
    })?;
    Ok(json!({"submitted":true,"header":read(process, header_key)?}))
}
/// Click Confirm and Exit (done_button) and the OK of its confirmation; ends
/// when the conference window closed. The result notice that follows is left
/// for hoi4_events / the plan runner.
pub(super) fn exit(
    process: &dyn Process,
    keys: &dyn GameInput,
    sent: &std::cell::Cell<bool>,
) -> Result<Value> {
    sent.set(true);
    click_button(process, keys, "done_button")?;
    let deadline = Instant::now() + Duration::from_secs(6);
    let mut returned = false;
    loop {
        if !is_open(process)? {
            return Ok(json!({"exited":true}));
        }
        if Instant::now() >= deadline {
            return Err(Error::new(
                "input_readback_failed",
                "The conference stayed open after Confirm and Exit",
            ));
        }
        // The "Exit Conference" confirmation is a command dialog (OK/Cancel); older builds
        // showed a notice pop-up with one OK button.
        let accepted = super::ui::accept_command(process, keys).unwrap_or(false);
        let _ = dismiss_notices(process, keys);
        // 1.19.4.1: the "Exit Conference" popup (PEACE_DONE_POPUP_DESCRIPTION) is neither;
        // Return accepts it (verified live 1 November 2026). Pressed once.
        if !accepted && !returned {
            std::thread::sleep(Duration::from_millis(300));
            keys.press_code(crate::platform::game_input::vk::RETURN)?;
            returned = true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kind_flags_are_exclusive() {
        assert_eq!(Kind::parse(false, false).unwrap(), Kind::Take);
        assert_eq!(Kind::parse(true, false).unwrap(), Kind::Puppet);
        assert_eq!(Kind::parse(false, true).unwrap(), Kind::Liberate);
        assert_eq!(
            Kind::parse(true, true).unwrap_err().code,
            "invalid_arguments"
        );
        assert_eq!(Kind::Puppet.button(), "puppet_action");
    }
    #[test]
    fn states_are_bounded_and_unique() {
        assert_eq!(validate_states(&[8, 9]).unwrap(), vec![8, 9]);
        assert!(validate_states(&[]).is_err());
        assert!(validate_states(&[0]).is_err());
        assert!(validate_states(&[4096]).is_err());
        assert!(validate_states(&[3, 3]).is_err());
        assert!(validate_states(&(1..=9).collect::<Vec<_>>()).is_err());
    }
    #[test]
    fn numbers_come_from_window_text() {
        assert_eq!(parse_number("95"), Some(95.0));
        assert_eq!(parse_number("Score: 68.5 / 95"), Some(68.5));
        assert_eq!(parse_number("-3"), Some(-3.0));
        assert_eq!(parse_number("3."), Some(3.0));
        assert_eq!(parse_number("none"), None);
    }
    #[test]
    fn controls_must_be_shown_and_enabled() {
        assert!(usable(true, false));
        assert!(!usable(true, true));
        assert!(!usable(false, false));
    }
    #[test]
    fn take_settles_only_on_the_score() {
        assert_eq!(
            take_settle(Some(95.0), Some(68.0), true, false),
            Settle::Reached
        );
        assert_eq!(
            take_settle(Some(95.0), Some(95.0), false, false),
            Settle::Unchanged
        );
        assert_eq!(
            take_settle(Some(95.0), Some(95.0), true, false),
            Settle::Diverged
        );
        assert_eq!(
            take_settle(Some(95.0), Some(95.0), false, true),
            Settle::Diverged
        );
        assert_eq!(
            take_settle(Some(68.0), Some(95.0), true, false),
            Settle::Diverged
        );
        assert_eq!(take_settle(Some(95.0), None, true, false), Settle::Unknown);
    }
    #[test]
    fn submit_settles_on_header_change_or_close() {
        assert_eq!(submit_settle("a", None, true), Settle::Reached);
        assert_eq!(submit_settle("a", Some("b"), true), Settle::Reached);
        assert_eq!(submit_settle("a", Some("a"), false), Settle::Unchanged);
        assert_eq!(submit_settle("a", Some("a"), true), Settle::Diverged);
    }
}
