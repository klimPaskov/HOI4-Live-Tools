//! `hoi4_naval_invasion` (UNVALIDATED, built offline from `interface/battleplantools.gui` and the
//! localisation): one deliberate naval invasion order for land units, drawn with the ordinary
//! battle plan tools.
//!
//! The game's own text describes the route (`theater_l_english.yml`): the battle plan toolbar has
//! `btn_order_draw_naval_inv` (and `btn_order_draw_naval_inv_with_harbor`); then "Click to select the
//! invasion source" (a coastal province where the units start), "Right-click to select the invasion
//! destination" (a marked coastal province), "RETURN to confirm order", "ESCAPE to delete order and
//! cancel". The game needs convoys for the transports (`NAVAL_INVASION_LACK_TRANSPORT`), time to
//! prepare (`NAVAL_INVASION_LACK_PREPARATION`) and naval dominance on the sea route; none of that
//! is decided here, the game reports it on the army card and the plan stays a plan until prepared.
//!
//! Aiming: the Find view centres the source state and the destination state; each click goes to the
//! screen centre plus an optional pixel offset (`source_offset`, `landing_offset`, within 350 px). A state
//! whose centre province is not a valid port or coast needs an offset; with the wrong point the
//! order does not appear, the drawing is cancelled with Escape and `input_readback_failed` says so.
//! One call places one order: no probing and no repetition.
//!
//! Readback (memory): the army's battle plan order count (`armies::plan_nodes`) must grow, or at least one
//! of the selected divisions must head for a province of the destination state (division +0x2ac).
use super::{
    armies::{self, division_states},
    diplomacy_state, front_plan, kit,
    journal::Outcome,
    mapdata, naval,
    settle::Verdict,
    ui::{clear_notices, find_state, read, settled, wait},
    wars,
};
use crate::{
    error::{Error, Result},
    gui_tree::{self, Rect},
    memory::Reader,
    platform::{
        macos_input::{Key, MacKeys},
        Process,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{collections::BTreeSet, thread, time::Duration};

pub(super) const TOOL: &str = "hoi4_naval_invasion";
/// `CBattlePlansTools` (the view that owns `battleplantools_window`), image offset from the RTTI map.
const BATTLE_PLAN_TOOLS: u64 = 0x36424c8;
const OFFSET_LIMIT: i64 = 350;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) enum Selector {
    Army(String),
    /// Division ids that are exactly the own divisions standing in `state`.
    Divisions { state: i32, ids: Vec<String> },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Plan {
    pub selector: Selector,
    pub source_state: i32,
    pub target_state: i32,
    pub target_provinces: Vec<i32>,
    pub harbor: bool,
    pub source_offset: (i32, i32),
    pub landing_offset: (i32, i32),
    /// Battle plan order nodes of the army before (army selector).
    pub nodes_before: usize,
}
/// Everything the refusals are computed from (all reads, no input).
pub(super) struct Facts {
    pub player: String,
    pub armies: Value,
    pub division_states: std::collections::BTreeMap<String, Option<i32>>,
    /// (owner, controller) of the target state.
    pub target_holder: Option<(String, String)>,
    pub enemies: BTreeSet<String>,
    pub target_coastal: Option<bool>,
    pub source_coastal: Option<bool>,
    pub target_provinces: Vec<i32>,
    pub convoys: i64,
    pub nodes_before: usize,
}
pub(super) struct Request {
    pub army: Option<String>,
    pub divisions: Vec<String>,
    pub target_state: i32,
    pub source_state: Option<i32>,
    pub harbor: bool,
    pub source_offset: (i32, i32),
    pub landing_offset: (i32, i32),
}

fn army_row<'a>(view: &'a Value, name: &str) -> Option<&'a Value> {
    view["armies"]
        .as_array()?
        .iter()
        .find(|a| a["army"].as_str().is_some_and(|n| kit::same_name(n, name)))
}
/// Typed refusals before any input.
pub(super) fn plan(req: &Request, facts: &Facts) -> Result<Plan> {
    // The destination first: it is the same for both selectors.
    let Some((owner, controller)) = &facts.target_holder else {
        return Err(Error::new("invalid_target", format!("State {} is not known", req.target_state)));
    };
    if *owner == facts.player || *controller == facts.player {
        return Err(Error::new(
            "own_state",
            format!("State {} is held by the player; an invasion lands on enemy coast (use hoi4_army_move)", req.target_state),
        ));
    }
    if !facts.enemies.contains(owner) && !facts.enemies.contains(controller) {
        return Err(Error::new(
            "enemy_required",
            format!(
                "State {} is held by {owner}/{controller}, neither at war with the player (enemies: {}); declare a war first",
                req.target_state,
                if facts.enemies.is_empty() { "none".to_string() } else { facts.enemies.iter().cloned().collect::<Vec<_>>().join(", ") }
            ),
        ));
    }
    if facts.target_coastal == Some(false) {
        return Err(Error::new("not_coastal", format!("State {} has no coast to land on", req.target_state)));
    }
    if facts.convoys <= 0 {
        return Err(Error::new(
            "no_transports",
            "No convoys: the invasion needs convoys as transports (hoi4_convoys); build convoy_1 lines first",
        ));
    }
    let (selector, source_state) = match (&req.army, req.divisions.is_empty()) {
        (Some(_), false) | (None, true) => {
            return Err(Error::new("invalid_arguments", "Give exactly one of army or divisions"));
        }
        (Some(name), true) => {
            let Some(row) = army_row(&facts.armies, name) else {
                return Err(Error::new("no_such_army", format!("No own army named {name:?} (hoi4_armies)")));
            };
            if row["division_count"].as_u64().unwrap_or(0) == 0 {
                return Err(Error::new("no_own_divisions", format!("{name} has no divisions")));
            }
            let source = req.source_state.or_else(|| row["main_state"].as_i64().map(|s| s as i32));
            let Some(source) = source else {
                return Err(Error::new(
                    "army_location_unknown",
                    format!("{name} is on the move; wait until it stands somewhere or pass source_state"),
                ));
            };
            (Selector::Army(row["army"].as_str().unwrap_or(name).to_string()), source)
        }
        (None, false) => {
            let mut states = BTreeSet::new();
            for id in &req.divisions {
                match facts.division_states.get(id) {
                    None => return Err(Error::new("no_such_division", format!("No own division {id} (hoi4_armies)"))),
                    Some(None) => {
                        return Err(Error::new("army_location_unknown", format!("Division {id} is on the move")))
                    }
                    Some(Some(s)) => {
                        states.insert(*s);
                    }
                }
            }
            if states.len() != 1 {
                return Err(Error::new(
                    "divisions_not_together",
                    format!("The divisions stand in several states ({states:?}); invade with the ones in one port state"),
                ));
            }
            let state = *states.iter().next().unwrap();
            let there: Vec<&String> = facts
                .division_states
                .iter()
                .filter(|(_, s)| **s == Some(state))
                .map(|(id, _)| id)
                .collect();
            if there.len() != req.divisions.len() {
                return Err(Error::new(
                    "divisions_not_isolated",
                    format!(
                        "{} own divisions stand in state {state} but {} were named; the box selection would take the others too. \
                         Name them all, or order an army instead",
                        there.len(),
                        req.divisions.len()
                    ),
                ));
            }
            (Selector::Divisions { state, ids: req.divisions.clone() }, req.source_state.unwrap_or(state))
        }
    };
    if source_state == req.target_state {
        return Err(Error::new("invalid_target", "Source and destination are the same state"));
    }
    if facts.source_coastal == Some(false) {
        return Err(Error::new(
            "source_not_coastal",
            format!("State {source_state} has no port to start from; give a coastal source_state"),
        ));
    }
    Ok(Plan {
        selector,
        source_state,
        target_state: req.target_state,
        target_provinces: facts.target_provinces.clone(),
        harbor: req.harbor,
        source_offset: req.source_offset,
        landing_offset: req.landing_offset,
        nodes_before: facts.nodes_before,
    })
}

/// Settlement: the order exists (more plan nodes, or divisions heading for the destination).
/// `nodes_now` and `heading` are `None` when unreadable.
pub(super) fn settle(plan: &Plan, nodes_now: Option<usize>, heading: Option<usize>) -> Verdict {
    let army = matches!(plan.selector, Selector::Army(_));
    match (army, nodes_now, heading) {
        (_, _, Some(h)) if h > 0 => Verdict::Reached,
        (true, Some(n), _) if n > plan.nodes_before => Verdict::Reached,
        (true, Some(n), _) if n < plan.nodes_before => Verdict::Diverged,
        (true, Some(_), Some(0)) | (false, _, Some(0)) => Verdict::Unchanged,
        (true, Some(_), None) => Verdict::Unchanged,
        _ => Verdict::Unknown,
    }
}

// ---- reads ----------------------------------------------------------------------------------------
fn gather(process: &dyn Process, req: &Request) -> Result<Facts> {
    let exe = &process
        .identity()
        .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
        .exe;
    let dir = mapdata::game_dir(exe)?;
    let map = front_plan::static_map(&dir)?;
    let snapshot = read(process, wars::snapshot)?;
    let target_holder = snapshot
        .states
        .iter()
        .find(|s| s.id == req.target_state)
        .map(|s| (s.owner.clone(), s.controller.clone()));
    let enemies = diplomacy_state::player_wars(process)?.enemies;
    let armies_view = armies::armies(process)?;
    let division_states = read(process, division_states)?;
    let convoys = naval::convoys_tool(process, &Map::new())?["convoys"]["total"].as_i64().unwrap_or(0);
    let source_hint = req.source_state.or_else(|| {
        req.army
            .as_deref()
            .and_then(|a| army_row(&armies_view, a))
            .and_then(|row| row["main_state"].as_i64())
            .map(|s| s as i32)
    });
    let nodes_before = match &req.army {
        Some(a) => read(process, |r, i| armies::plan_nodes(r, i, a)).unwrap_or(0),
        None => 0,
    };
    Ok(Facts {
        player: snapshot.player.clone(),
        armies: armies_view,
        division_states,
        target_holder,
        enemies,
        target_coastal: map.info.get(&req.target_state).map(|i| i.coastal),
        source_coastal: source_hint.and_then(|s| map.info.get(&s)).map(|i| i.coastal),
        target_provinces: mapdata::state_provinces(&dir, req.target_state).unwrap_or_default(),
        convoys,
        nodes_before,
    })
}
fn progress(process: &dyn Process, plan: &Plan) -> Result<(Option<usize>, Option<usize>)> {
    read(process, |r, i| {
        let nodes = match &plan.selector {
            Selector::Army(a) => armies::plan_nodes(r, i, a).ok(),
            Selector::Divisions { .. } => None,
        };
        let heading = match &plan.selector {
            Selector::Army(a) if !plan.target_provinces.is_empty() => {
                armies::heading_for(r, i, a, &plan.target_provinces).ok().map(|h| h.0)
            }
            Selector::Divisions { ids, .. } if !plan.target_provinces.is_empty() => {
                armies::divisions_heading_for(r, i, ids, &plan.target_provinces).ok().map(|h| h.0)
            }
            _ => None,
        };
        Ok((nodes, heading))
    })
}

// ---- driving --------------------------------------------------------------------------------------
/// A battle plan toolbar button: rectangle and state (3 disabled).
fn plan_button(r: &Reader<'_>, image: u64, name: &str) -> Result<(Rect, i32)> {
    let view = kit::find_view(r, image, BATTLE_PLAN_TOOLS)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "The battle plan tools view was not found"))?;
    let window = kit::find_window(r, image, view, "battleplantools_window")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "The battle plan toolbar window was not found"))?;
    if !gui_tree::visible(r, window)? {
        return Err(Error::new("gui_target_unavailable", "The battle plan toolbar is not shown (no land units selected?)"));
    }
    kit::rect_deep(r, image, window, name)
}
fn pause(ms: u64) {
    thread::sleep(Duration::from_millis(ms));
}
fn aim(centre: (f64, f64), offset: (i32, i32)) -> (f64, f64) {
    (centre.0 + f64::from(offset.0), centre.1 + f64::from(offset.1))
}
/// Cancel the drawing mode (Escape deletes a drawn order) and close the game menu a stray Escape
/// may have opened.
fn cancel_drawing(process: &dyn Process, keys: &MacKeys<'_>) {
    let _ = keys.press(Key::Escape);
    pause(300);
    let _ = super::ui::close_menu(process, keys);
}
pub(super) fn drive(process: &dyn Process, keys: &MacKeys<'_>, plan: &Plan) -> Result<Value> {
    clear_notices(process, keys)?;
    match &plan.selector {
        Selector::Army(name) => {
            armies::select_army(process, keys, name)?;
        }
        Selector::Divisions { state, ids } => armies::box_select_exact(process, keys, *state, ids.len())?,
    }
    let button = if plan.harbor { "btn_order_draw_naval_inv_with_harbor" } else { "btn_order_draw_naval_inv" };
    let (rect, state) = settled(process, "invasion button", |r, i| plan_button(r, i, button))?;
    if state == 3 {
        return Err(Error::new(
            "button_disabled",
            "The game disables the naval invasion order for this selection (no coast reachable, or units not able to embark)",
        ));
    }
    let drawn = (|| -> Result<()> {
        kit::click(keys, rect)?;
        pause(400);
        // The source: a left click on the source state's centre.
        find_state(process, keys, plan.source_state)?;
        pause(300);
        let (sx, sy) = aim((960.0, 540.0), plan.source_offset);
        keys.click(kit::GAME, sx, sy)?;
        pause(500);
        // The destination: a right click on the destination state's centre.
        find_state(process, keys, plan.target_state)?;
        pause(300);
        let (tx, ty) = aim((960.0, 540.0), plan.landing_offset);
        keys.right_click(kit::GAME, tx, ty)?;
        pause(600);
        keys.press(Key::Return)?;
        pause(500);
        Ok(())
    })();
    if let Err(error) = drawn {
        cancel_drawing(process, keys);
        return Err(error);
    }
    let expect = plan.clone();
    let seen = wait(process, "invasion order", |r, i| {
        let nodes = match &expect.selector {
            Selector::Army(a) => armies::plan_nodes(r, i, a).ok(),
            _ => None,
        };
        let heading = match &expect.selector {
            Selector::Army(a) => armies::heading_for(r, i, a, &expect.target_provinces).ok().map(|h| h.0),
            Selector::Divisions { ids, .. } => armies::divisions_heading_for(r, i, ids, &expect.target_provinces).ok().map(|h| h.0),
        };
        Ok(settle(&expect, nodes, heading) == Verdict::Reached)
    });
    if seen.is_err() {
        cancel_drawing(process, keys);
        return Err(Error::new(
            "input_readback_failed",
            format!(
                "No invasion order appeared for state {} (source state {}). The aim points were the state centres plus the offsets; a centre that is \
                 not a valid port/coast needs source_offset / landing_offset. The drawing was cancelled with Escape; nothing was repeated",
                plan.target_state, plan.source_state
            ),
        ));
    }
    let (nodes, heading) = progress(process, plan)?;
    Ok(json!({"selector":plan.selector,"source_state":plan.source_state,"target_state":plan.target_state,"harbor":plan.harbor,
        "battle_plan_nodes_before":plan.nodes_before,"battle_plan_nodes_now":nodes,"divisions_heading_for_target":heading,
        "note":"An invasion needs convoys, preparation time and naval dominance on the route; read the army card and hoi4_armies for the preparation progress"}))
}

// ---- dispatch -------------------------------------------------------------------------------------
fn pair(args: &Map<String, Value>, key: &str) -> Result<(i32, i32)> {
    let Some(v) = args.get(key) else {
        return Ok((0, 0));
    };
    let a = v.as_array().filter(|a| a.len() == 2);
    let n = |i: usize| a.and_then(|a| a[i].as_i64()).filter(|n| n.abs() <= OFFSET_LIMIT);
    match (n(0), n(1)) {
        (Some(x), Some(y)) => Ok((x as i32, y as i32)),
        _ => Err(Error::new("invalid_arguments", format!("{key} must be [dx, dy] within +-{OFFSET_LIMIT} pixels"))),
    }
}
pub(super) fn call(args: &Map<String, Value>, env: kit::Env<'_>) -> Result<Value> {
    super::allowed(
        args,
        &["state_id", "army", "divisions", "source_state_id", "with_harbor", "source_offset", "landing_offset", "request_id", "lease_token"],
    )?;
    let target_state = args
        .get("state_id")
        .and_then(Value::as_i64)
        .filter(|s| (1..=4095).contains(s))
        .ok_or_else(|| Error::new("invalid_arguments", "state_id must be an integer 1..4095"))? as i32;
    let source_state = match args.get("source_state_id") {
        None => None,
        Some(v) => Some(
            v.as_i64()
                .filter(|s| (1..=4095).contains(s))
                .ok_or_else(|| Error::new("invalid_arguments", "source_state_id must be an integer 1..4095"))? as i32,
        ),
    };
    let divisions: Vec<String> = match args.get("divisions") {
        None => Vec::new(),
        Some(v) => v
            .as_array()
            .filter(|a| !a.is_empty() && a.len() <= 40)
            .and_then(|a| a.iter().map(|d| d.as_str().filter(|d| d.len() == 17).map(str::to_string)).collect())
            .ok_or_else(|| Error::new("invalid_arguments", "divisions must be 1..40 division ids from hoi4_armies"))?,
    };
    let req = Request {
        army: kit::opt_text_arg(args, "army", 128)?,
        divisions,
        target_state,
        source_state,
        harbor: args.get("with_harbor").and_then(Value::as_bool).unwrap_or(false),
        source_offset: pair(args, "source_offset")?,
        landing_offset: pair(args, "landing_offset")?,
    };
    let id = super::string(args, "request_id")?.to_string();
    let token = super::string(args, "lease_token")?.to_string();
    let action = json!({"kind":"naval_invasion","state_id":target_state,"army":req.army,"divisions":req.divisions,
        "source_state_id":source_state,"with_harbor":req.harbor,"source_offset":req.source_offset,"landing_offset":req.landing_offset});
    kit::run(
        env,
        kit::Request { id: &id, token: &token, seconds: 150, action },
        &|p| {
            let facts = gather(p, &req)?;
            let planned = plan(&req, &facts)?;
            kit::remember(&id, &planned);
            Ok(planned)
        },
        &|p, planned| {
            let Some(pl) = planned.cloned().or_else(|| kit::recall::<Plan>(&id)) else {
                return Ok(Outcome::Unknown);
            };
            let (nodes, heading) = progress(p, &pl)?;
            Ok(settle(&pl, nodes, heading).outcome(json!({"nodes":nodes,"heading":heading})))
        },
        &|p, keys, planned| drive(p, keys, planned),
    )
}

pub(super) fn tool() -> Value {
    json!({"name":TOOL,"description":format!("{}Order ONE naval invasion of an enemy coastal state for land units: the army's card selects it (or a box selection takes exactly the named `divisions` standing in one state), the battle plan toolbar's naval invasion button (with_harbor:true picks the harbour variant), a left click on the source state's centre, a right click on the destination state's centre, Return. One deliberate order per call, nothing is repeated. Aim: the Find view centres each state and the click goes to the centre plus an optional pixel offset ([dx,dy], within 350); a centre that is not a valid port or coast needs an offset. The game needs convoys, preparation days and naval dominance (read the army card). Typed refusals before input: own_state, enemy_required (the state's owner or controller is not at war with us), not_coastal, source_not_coastal, no_transports (no convoys), no_such_army, no_such_division, divisions_not_together, divisions_not_isolated, army_location_unknown, invalid_target; while driving selection_mismatch, button_disabled, input_readback_failed (the drawing is cancelled with Escape). Verified by more battle plan orders on the army or divisions heading for a province of the state. Result has validated:false.", kit::UNVALIDATED),
        "inputSchema":{"type":"object","properties":{"state_id":{"type":"integer","minimum":1,"maximum":4095},
            "army":{"type":"string","maxLength":128},
            "divisions":{"type":"array","items":{"type":"string"},"minItems":1,"maxItems":40},
            "source_state_id":{"type":"integer","minimum":1,"maximum":4095},
            "with_harbor":{"type":"boolean"},
            "source_offset":{"type":"array","items":{"type":"integer","minimum":-350,"maximum":350},"minItems":2,"maxItems":2},
            "landing_offset":{"type":"array","items":{"type":"integer","minimum":-350,"maximum":350},"minItems":2,"maxItems":2},
            "request_id":{"type":"string"},"lease_token":{"type":"string"}},
            "required":["state_id","request_id","lease_token"],"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn facts() -> Facts {
        Facts {
            player: "GER".into(),
            armies: json!({"armies":[
                {"army":"Landungsarmee","division_count":6,"main_state":64,"divisions":["00000001:00000001"]},
                {"army":"Reiter","division_count":0,"main_state":null}]}),
            division_states: BTreeMap::from([
                ("00000001:00000001".to_string(), Some(64)),
                ("00000001:00000002".to_string(), Some(64)),
                ("00000001:00000003".to_string(), Some(11)),
                ("00000001:00000004".to_string(), None),
            ]),
            target_holder: Some(("DEN".into(), "DEN".into())),
            enemies: BTreeSet::from(["DEN".to_string()]),
            target_coastal: Some(true),
            source_coastal: Some(true),
            target_provinces: vec![1, 2, 3],
            convoys: 40,
            nodes_before: 2,
        }
    }
    fn request() -> Request {
        Request {
            army: Some("landungsarmee".into()),
            divisions: Vec::new(),
            target_state: 12,
            source_state: None,
            harbor: false,
            source_offset: (0, 0),
            landing_offset: (10, -20),
        }
    }
    #[test]
    fn plans_an_army_invasion() {
        let p = plan(&request(), &facts()).unwrap();
        assert_eq!(p.selector, Selector::Army("Landungsarmee".into()));
        assert_eq!((p.source_state, p.target_state, p.nodes_before), (64, 12, 2));
        assert_eq!(p.landing_offset, (10, -20));
    }
    #[test]
    fn refuses_before_input() {
        let code = |req: Request, f: Facts| plan(&req, &f).unwrap_err().code;
        let mut f = facts();
        f.target_holder = Some(("GER".into(), "GER".into()));
        assert_eq!(code(request(), f), "own_state");
        let mut f = facts();
        f.target_holder = Some(("DEN".into(), "GER".into()));
        assert_eq!(code(request(), f), "own_state");
        let mut f = facts();
        f.enemies.clear();
        assert_eq!(code(request(), f), "enemy_required");
        let mut f = facts();
        f.target_coastal = Some(false);
        assert_eq!(code(request(), f), "not_coastal");
        let mut f = facts();
        f.convoys = 0;
        assert_eq!(code(request(), f), "no_transports");
        let mut f = facts();
        f.source_coastal = Some(false);
        assert_eq!(code(request(), f), "source_not_coastal");
        let mut f = facts();
        f.target_holder = None;
        assert_eq!(code(request(), f), "invalid_target");
        let mut r = request();
        r.army = Some("Nope".into());
        assert_eq!(code(r, facts()), "no_such_army");
        let mut r = request();
        r.army = Some("Reiter".into());
        assert_eq!(code(r, facts()), "no_own_divisions");
        let mut r = request();
        r.target_state = 64;
        assert_eq!(code(r, facts()), "invalid_target");
        let mut r = request();
        r.divisions = vec!["00000001:00000001".into()];
        assert_eq!(code(r, facts()), "invalid_arguments");
        let mut r = request();
        r.army = None;
        assert_eq!(code(r, facts()), "invalid_arguments");
    }
    #[test]
    fn army_on_the_move_needs_a_source() {
        let mut f = facts();
        f.armies = json!({"armies":[{"army":"Landungsarmee","division_count":3,"main_state":null}]});
        assert_eq!(plan(&request(), &f).unwrap_err().code, "army_location_unknown");
        let mut r = request();
        r.source_state = Some(64);
        assert_eq!(plan(&r, &f).unwrap().source_state, 64);
    }
    #[test]
    fn division_selection_must_be_exactly_those_standing_in_one_state() {
        let mut r = request();
        r.army = None;
        r.divisions = vec!["00000001:00000001".into(), "00000001:00000002".into()];
        let p = plan(&r, &facts()).unwrap();
        assert_eq!(p.selector, Selector::Divisions { state: 64, ids: r.divisions.clone() });
        r.divisions = vec!["00000001:00000001".into()];
        assert_eq!(plan(&r, &facts()).unwrap_err().code, "divisions_not_isolated");
        r.divisions = vec!["00000001:00000001".into(), "00000001:00000003".into()];
        assert_eq!(plan(&r, &facts()).unwrap_err().code, "divisions_not_together");
        r.divisions = vec!["00000001:00000004".into()];
        assert_eq!(plan(&r, &facts()).unwrap_err().code, "army_location_unknown");
        r.divisions = vec!["00000001:000000ff".into()];
        assert_eq!(plan(&r, &facts()).unwrap_err().code, "no_such_division");
    }
    #[test]
    fn settlement_reads_orders_and_heading() {
        let p = plan(&request(), &facts()).unwrap();
        assert_eq!(settle(&p, Some(3), None), Verdict::Reached);
        assert_eq!(settle(&p, Some(2), Some(0)), Verdict::Unchanged);
        assert_eq!(settle(&p, Some(2), None), Verdict::Unchanged);
        assert_eq!(settle(&p, Some(1), Some(0)), Verdict::Diverged);
        assert_eq!(settle(&p, Some(2), Some(4)), Verdict::Reached);
        assert_eq!(settle(&p, None, None), Verdict::Unknown);
        let mut r = request();
        r.army = None;
        r.divisions = vec!["00000001:00000001".into(), "00000001:00000002".into()];
        let d = plan(&r, &facts()).unwrap();
        assert_eq!(settle(&d, None, Some(2)), Verdict::Reached);
        assert_eq!(settle(&d, None, Some(0)), Verdict::Unchanged);
        assert_eq!(settle(&d, None, None), Verdict::Unknown);
    }
    #[test]
    fn offsets_are_bounded_pairs() {
        let args = json!({"a":[10,-20],"b":[400,0],"c":[1]});
        let args = args.as_object().unwrap();
        assert_eq!(pair(args, "a").unwrap(), (10, -20));
        assert_eq!(pair(args, "zz").unwrap(), (0, 0));
        assert_eq!(pair(args, "b").unwrap_err().code, "invalid_arguments");
        assert_eq!(pair(args, "c").unwrap_err().code, "invalid_arguments");
        assert_eq!(aim((960.0, 540.0), (10, -20)), (970.0, 520.0));
    }
    #[test]
    fn tool_schema_is_marked_unvalidated() {
        let t = tool();
        assert!(t["description"].as_str().unwrap().starts_with("UNVALIDATED (no live test yet): "));
        assert_eq!(t["name"], TOOL);
        assert_eq!(t["inputSchema"]["required"].as_array().unwrap().len(), 3);
    }
}
