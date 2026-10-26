//! Fleet and task force structure tools (UNVALIDATED, built offline from `interface/naviesview.gui`
//! and the game's localisation): merge, split, balance, reserve, delete and assign, all through
//! the Strategic Navy map mode (F2), the fleet panel and its `tool_bar_window`.
//!
//! Buttons (names from `naviesview.gui`, rectangles read from the live tree, never copied):
//! * `btn_merge` (shortcut G; tooltip "Merge the selected task forces", state 3 while fewer than two
//!   task forces are in the selection);
//! * `btn_split_in_half` (shortcut D; "Split the selected task force in half");
//! * `btn_taskforce_o_matic` (no tooltip key in the files; assumed to be the game's automatic
//!   rebalancing of the selection: THIS MAPPING IS A GUESS, see the validation queue);
//! * `btn_exclude` (sprite `GFX_NV_btn_set_as_reserve`: sends the selection to the reserve fleet);
//! * `btn_delete` ("Delete task force" confirmation: the ships are DESTROYED);
//! * moving a task force into another fleet: the localisation has "Are you sure you want to assign
//!   all taskforces of $FLEETS$ to $PARENT$ and Disband empty fleets?" (`REASSIGN_ALL_TASKFORCES`),
//!   so a fleet card is a drop/assign target. The route tried is the army cards' convention (with the
//!   task force selected, a RIGHT click on the target fleet card); `route:"drag"` drags the task
//!   force's entry onto the card instead.
//!
//! The selection model is the game's: with no task force selected a toolbar button works on all of the
//! panel's fleet ("Missions - All Taskforces"), with one selected on that one. Each tool therefore
//! deselects first (fleet-level merge and balance) or selects exactly one entry (the others).
//! Verification is the pure `settle` rule over the fleet list in memory; nothing is retried.
use super::{
    fleetmem,
    kit::{self, same_name, Env},
    naval::{Fleet, TaskForce},
    navalact, navalui as nui,
    settle::Verdict,
    ui::{clear_notices, settled, wait},
};
use crate::{
    economy,
    error::{Error, Result},
    gui_tree::{self, Rect},
    platform::{macos_input::MacKeys, Process},
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{collections::BTreeSet, thread, time::Duration};

pub(super) const TOOLS: [&str; 6] = [
    "hoi4_fleet_merge",
    "hoi4_fleet_split",
    "hoi4_fleet_balance",
    "hoi4_fleet_reserve",
    "hoi4_fleet_delete",
    "hoi4_fleet_assign",
];
pub(super) fn handles(name: &str) -> bool {
    TOOLS.contains(&name)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Op {
    Merge,
    Split,
    Balance,
    Reserve,
    Delete,
    Assign,
}
impl Op {
    fn from_tool(name: &str) -> Option<Op> {
        Some(match name {
            "hoi4_fleet_merge" => Op::Merge,
            "hoi4_fleet_split" => Op::Split,
            "hoi4_fleet_balance" => Op::Balance,
            "hoi4_fleet_reserve" => Op::Reserve,
            "hoi4_fleet_delete" => Op::Delete,
            "hoi4_fleet_assign" => Op::Assign,
            _ => return None,
        })
    }
    fn label(self) -> &'static str {
        match self {
            Op::Merge => "merge",
            Op::Split => "split",
            Op::Balance => "balance",
            Op::Reserve => "reserve",
            Op::Delete => "delete",
            Op::Assign => "assign",
        }
    }
    /// Toolbar button of the operation (assign uses the target fleet's card instead).
    fn button(self) -> Option<&'static str> {
        match self {
            Op::Merge => Some("btn_merge"),
            Op::Split => Some("btn_split_in_half"),
            Op::Balance => Some("btn_taskforce_o_matic"),
            Op::Reserve => Some("btn_exclude"),
            Op::Delete => Some("btn_delete"),
            Op::Assign => None,
        }
    }
    /// Fleet-level operations act on the whole fleet (no task force selected).
    fn fleet_level(self) -> bool {
        matches!(self, Op::Merge | Op::Balance)
    }
}

// ---- planning (pure, before any input) ------------------------------------------------------------
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Plan {
    pub op: Op,
    /// The fleet the operation starts in.
    pub fleet: String,
    pub task_force: Option<String>,
    /// Assign: the receiving fleet.
    pub target: Option<String>,
    /// Fleet names in card order.
    pub order: Vec<String>,
    pub task_forces_before: usize,
    /// Ship ids of the starting fleet.
    pub ship_ids: BTreeSet<i32>,
    /// The starting fleet's task forces as sorted ship id lists.
    pub partition: BTreeSet<Vec<i32>>,
}
fn ids(f: &Fleet) -> BTreeSet<i32> {
    f.task_forces.iter().flat_map(|t| t.ships.iter().map(|s| s.id)).collect()
}
fn partition(f: &Fleet) -> BTreeSet<Vec<i32>> {
    f.task_forces
        .iter()
        .map(|t| {
            let mut v: Vec<i32> = t.ships.iter().map(|s| s.id).collect();
            v.sort_unstable();
            v
        })
        .collect()
}
/// Index of the reserve fleet: the last fleet when it holds only reserve task forces (code 8).
pub(super) fn reserve_index(fleets: &[Fleet]) -> Option<usize> {
    let last = fleets.last()?;
    (fleets.len() > 1 && !last.task_forces.is_empty() && last.task_forces.iter().all(|t| t.mission_code == 8))
        .then(|| fleets.len() - 1)
}
fn fleet_index(fleets: &[Fleet], name: &str) -> Result<usize> {
    fleets.iter().position(|f| same_name(&f.name, name)).ok_or_else(|| {
        Error::new(
            "fleet_unknown",
            format!(
                "No own fleet named {name:?}; own fleets: {}",
                fleets.iter().map(|f| f.name.trim()).collect::<Vec<_>>().join(" | ")
            ),
        )
    })
}
fn base_plan(fleets: &[Fleet], op: Op, index: usize, task_force: Option<&TaskForce>) -> Plan {
    let f = &fleets[index];
    Plan {
        op,
        fleet: f.name.clone(),
        task_force: task_force.map(|t| t.name.clone()),
        target: None,
        order: fleets.iter().map(|f| f.name.clone()).collect(),
        task_forces_before: f.task_forces.len(),
        ship_ids: ids(f),
        partition: partition(f),
    }
}
/// Refusals decided from reads alone. `subject` is a fleet name (merge, balance) or a task force
/// name (split, reserve, delete, assign); `to` is the receiving fleet of an assignment.
pub(super) fn plan(fleets: &[Fleet], op: Op, subject: &str, to: Option<&str>) -> Result<Plan> {
    let reserve = reserve_index(fleets);
    if op.fleet_level() {
        let index = fleet_index(fleets, subject)?;
        if Some(index) == reserve {
            return Err(Error::new(
                "reserve_fleet",
                "The reserve fleet holds ships without orders; move them into a real fleet first",
            ));
        }
        if fleets[index].task_forces.len() < 2 {
            return Err(Error::new(
                "nothing_to_change",
                format!("Fleet {:?} has fewer than two task forces to {}", fleets[index].name.trim(), op.label()),
            ));
        }
        return Ok(base_plan(fleets, op, index, None));
    }
    let (index, _, tf) = navalact::find_task_force(fleets, subject)?;
    let home_is_reserve = Some(index) == reserve;
    match op {
        Op::Reserve if home_is_reserve => Err(Error::new(
            "nothing_to_change",
            format!("{} is already in the reserve fleet", tf.name),
        )),
        Op::Reserve | Op::Split | Op::Delete | Op::Assign if home_is_reserve => Err(Error::new(
            "reserve_fleet",
            "The reserve fleet's task forces are not driven; reserve ships are placed with the game's own reserve controls",
        )),
        Op::Split if tf.ships.len() < 2 => Err(Error::new(
            "task_force_too_small",
            format!("{} has {} ship(s); splitting in half needs at least two", tf.name, tf.ships.len()),
        )),
        Op::Assign => {
            let to = to.ok_or_else(|| Error::new("invalid_arguments", "to_fleet is required"))?;
            let target = fleet_index(fleets, to)?;
            if target == index {
                return Err(Error::new(
                    "nothing_to_change",
                    format!("{} is already in fleet {:?}", tf.name, fleets[index].name.trim()),
                ));
            }
            if Some(target) == reserve {
                return Err(Error::new(
                    "target_is_reserve",
                    "Use hoi4_fleet_reserve to send a task force to the reserve fleet",
                ));
            }
            let mut plan = base_plan(fleets, op, index, Some(tf));
            plan.target = Some(fleets[target].name.clone());
            Ok(plan)
        }
        _ => Ok(base_plan(fleets, op, index, Some(tf))),
    }
}

// ---- settlement (pure) ----------------------------------------------------------------------------
fn holder<'a>(fleets: &'a [Fleet], task_force: &str) -> Option<(usize, &'a Fleet)> {
    fleets
        .iter()
        .enumerate()
        .find(|(_, f)| f.task_forces.iter().any(|t| same_name(&t.name, task_force)))
}
/// Compare only what the operation can change: the starting fleet's ships and grouping, the
/// holder of the named task force.
pub(super) fn settle(plan: &Plan, now: &[Fleet]) -> Verdict {
    let fleet = now.iter().find(|f| same_name(&f.name, &plan.fleet));
    let tf = plan.task_force.as_deref();
    match plan.op {
        Op::Merge | Op::Split | Op::Balance => {
            let Some(f) = fleet else {
                return Verdict::Unknown;
            };
            if ids(f) != plan.ship_ids {
                return Verdict::Diverged;
            }
            let count = f.task_forces.len();
            match plan.op {
                Op::Merge if count == 1 => Verdict::Reached,
                Op::Split if count == plan.task_forces_before + 1 => Verdict::Reached,
                Op::Balance if partition(f) != plan.partition => Verdict::Reached,
                _ if count == plan.task_forces_before && partition(f) == plan.partition => Verdict::Unchanged,
                _ => Verdict::Diverged,
            }
        }
        Op::Reserve => match tf.and_then(|t| holder(now, t)) {
            None => Verdict::Unknown,
            Some((i, _)) if Some(i) == reserve_index(now) => Verdict::Reached,
            Some((_, f)) if same_name(&f.name, &plan.fleet) => Verdict::Unchanged,
            Some(_) => Verdict::Diverged,
        },
        Op::Delete => match tf {
            Some(t) if holder(now, t).is_none() => Verdict::Reached,
            Some(_) => Verdict::Unchanged,
            None => Verdict::Unknown,
        },
        Op::Assign => match tf.and_then(|t| holder(now, t)) {
            None => Verdict::Unknown,
            Some((_, f)) if plan.target.as_deref().is_some_and(|t| same_name(&f.name, t)) => Verdict::Reached,
            Some((_, f)) if same_name(&f.name, &plan.fleet) => Verdict::Unchanged,
            Some(_) => Verdict::Diverged,
        },
    }
}
/// A retry whose baseline is gone settles only by what the arguments imply.
fn settle_without_baseline(op: Op, subject: &str, to: Option<&str>, now: &[Fleet]) -> Verdict {
    match op {
        Op::Merge => match now.iter().find(|f| same_name(&f.name, subject)) {
            Some(f) if f.task_forces.len() == 1 => Verdict::Reached,
            Some(_) => Verdict::Unchanged,
            None => Verdict::Unknown,
        },
        Op::Reserve => match holder(now, subject) {
            Some((i, _)) if Some(i) == reserve_index(now) => Verdict::Reached,
            Some(_) => Verdict::Unchanged,
            None => Verdict::Unknown,
        },
        Op::Delete => {
            if holder(now, subject).is_none() {
                Verdict::Reached
            } else {
                Verdict::Unchanged
            }
        }
        Op::Assign => match (holder(now, subject), to) {
            (Some((_, f)), Some(t)) if same_name(&f.name, t) => Verdict::Reached,
            (Some(_), _) => Verdict::Unchanged,
            (None, _) => Verdict::Unknown,
        },
        Op::Split | Op::Balance => Verdict::Unknown,
    }
}

// ---- driving the navy UI --------------------------------------------------------------------------
/// A toolbar button: rectangle and raw state (0 normal, 3 disabled).
fn toolbar_button(r: &crate::memory::Reader<'_>, image: u64, name: &str) -> Result<(Rect, i32)> {
    let bar = nui::tool_bar(r, image)?;
    let button = nui::child_of(r, image, bar, name)?;
    if !gui_tree::shown(r, button)? {
        return Err(Error::new("gui_target_unavailable", format!("{name} is not shown")));
    }
    Ok((gui_tree::rect(r, image, button, nui::GAME)?, gui_tree::button_state(r, button)?))
}
fn wait_fleets(process: &dyn Process, stage: &str, ok: impl Fn(&[Fleet]) -> bool) -> Result<()> {
    wait(process, stage, |r, i| {
        fleetmem::require_libcxx(r)?;
        let (country, _) = economy::own_production(r, i)?;
        Ok(ok(&super::naval::fleets(r, i, country)?))
    })
}
fn press_toolbar(process: &dyn Process, keys: &MacKeys<'_>, op: Op) -> Result<()> {
    let name = op.button().ok_or_else(|| Error::new("invalid_arguments", "No toolbar button"))?;
    let (rect, state) = settled(process, "toolbar button", |r, i| toolbar_button(r, i, name))?;
    if state == 3 {
        return Err(Error::new(
            "button_disabled",
            format!("The game disables the {} button ({name}) for this selection", op.label()),
        ));
    }
    kit::click(keys, rect)
}
/// Make sure no task force of the open panel is selected (a selected entry shows "Back").
fn deselect_all(process: &dyn Process, keys: &MacKeys<'_>) -> Result<()> {
    for _ in 0..6 {
        let entries = settled(process, "task force entries", nui::entries)?;
        let Some(selected) = entries.iter().find(|e| e.selected) else {
            return Ok(());
        };
        navalact::click_at(
            keys,
            f64::from(selected.select.x) + 30.0,
            f64::from(selected.select.y + selected.select.h / 2),
        )?;
        thread::sleep(Duration::from_millis(600));
    }
    Err(Error::new("ui_blocked", "A task force stayed selected"))
}
fn with_navy_ui<T>(
    process: &dyn Process,
    keys: &MacKeys<'_>,
    f: impl FnOnce() -> Result<T>,
) -> Result<T> {
    clear_notices(process, keys)?;
    navalact::enter_navy_mode(process, keys)?;
    let result = (|| {
        navalact::close_region_view(process, keys)?;
        f()
    })();
    navalact::release(process, keys);
    result
}
/// Summary of the fleets after the action, for the result.
fn summary(fleets: &[Fleet]) -> Value {
    Value::Array(
        fleets
            .iter()
            .map(|f| json!({"fleet":f.name.trim(),"task_forces":f.task_forces.iter().map(|t| json!({"name":t.name,"ships":t.ships.len()})).collect::<Vec<_>>()}))
            .collect(),
    )
}
pub(super) fn drive(process: &dyn Process, keys: &MacKeys<'_>, plan: &Plan, drag: bool) -> Result<Value> {
    with_navy_ui(process, keys, || {
        navalact::select_fleet(process, keys, &plan.fleet, &plan.order)?;
        if plan.op.fleet_level() {
            deselect_all(process, keys)?;
        } else if let Some(tf) = &plan.task_force {
            navalact::select_task_force(process, keys, tf)?;
        }
        let mut answered = false;
        if plan.op == Op::Assign {
            let target = plan.target.as_deref().unwrap_or_default();
            let index = plan
                .order
                .iter()
                .position(|n| same_name(n, target))
                .ok_or_else(|| Error::new("fleet_unknown", "The receiving fleet is not among the own fleets"))?;
            let cards = settled(process, "fleet cards", nui::cards)?;
            let card = cards
                .get(index)
                .ok_or_else(|| Error::new("gui_target_unavailable", "The receiving fleet's card is not shown"))?;
            let x = f64::from(card.frame.x) + f64::from(card.frame.w) / 2.0;
            let y = (f64::from(card.frame.y) + f64::from(card.frame.h) / 2.0).min(1040.0);
            if drag {
                let tf = plan.task_force.as_deref().unwrap_or_default();
                let entries = settled(process, "task force entries", nui::entries)?;
                let entry = entries
                    .iter()
                    .find(|e| same_name(&e.name, tf))
                    .ok_or_else(|| Error::new("gui_target_unavailable", "The task force entry is not shown"))?;
                keys.drag(
                    nui::GAME,
                    (f64::from(entry.select.x) + 30.0, f64::from(entry.select.y + entry.select.h / 2)),
                    (x, y),
                )?;
            } else {
                keys.right_click(nui::GAME, x, y)?;
            }
            answered = kit::answer_confirmation(process, keys, true)?;
        } else {
            press_toolbar(process, keys, plan.op)?;
            if matches!(plan.op, Op::Reserve | Op::Delete | Op::Merge | Op::Balance) {
                answered = kit::answer_confirmation(process, keys, true)?;
            }
        }
        let expect = plan.clone();
        wait_fleets(process, "fleet structure changed", move |f| settle(&expect, f) == Verdict::Reached).map_err(|_| {
            Error::new(
                "input_readback_failed",
                format!(
                    "The {} did not show in the fleet list (the game may have refused it; no input was repeated)",
                    plan.op.label()
                ),
            )
        })?;
        let after = navalact::own_fleets(process)?;
        Ok(json!({"op":plan.op.label(),"fleet":plan.fleet.trim(),"task_force":plan.task_force,"target":plan.target,
            "confirmation_answered":answered,"route":if plan.op == Op::Assign { if drag {"drag"} else {"right_click"} } else {"toolbar"},
            "fleets_after":summary(&after)}))
    })
}

// ---- dispatch -------------------------------------------------------------------------------------
pub(super) fn call(name: &str, args: &Map<String, Value>, env: Env<'_>) -> Result<Value> {
    let op = Op::from_tool(name).ok_or_else(|| Error::new("capability_unavailable", "Unknown fleet tool"))?;
    let (subject_key, mut keys): (&str, Vec<&str>) = if op.fleet_level() {
        ("fleet", vec!["fleet"])
    } else {
        ("task_force", vec!["task_force"])
    };
    keys.extend(["request_id", "lease_token"]);
    match op {
        Op::Delete => keys.push("confirm"),
        Op::Assign => keys.extend(["to_fleet", "route"]),
        _ => {}
    }
    super::allowed(args, &keys)?;
    let subject = kit::text_arg(args, subject_key, 128)?;
    let to = if op == Op::Assign { Some(kit::text_arg(args, "to_fleet", 128)?) } else { None };
    let drag = match args.get("route").map(|v| v.as_str()) {
        None | Some(Some("right_click")) => false,
        Some(Some("drag")) => true,
        _ => return Err(Error::new("invalid_arguments", "route must be right_click or drag")),
    };
    if op == Op::Delete && args.get("confirm").and_then(Value::as_str) != Some("destroy_ships") {
        return Err(Error::new(
            "confirmation_required",
            "Deleting a task force DESTROYS its ships. Pass confirm:\"destroy_ships\" to do it; nothing was sent",
        ));
    }
    let id = super::string(args, "request_id")?.to_string();
    let token = super::string(args, "lease_token")?.to_string();
    let action = json!({"kind":format!("fleet_{}", op.label()),"subject":subject,"to_fleet":to,"route":if drag {"drag"} else {"right_click"}});
    kit::run(
        env,
        kit::Request { id: &id, token: &token, seconds: 120, action },
        &|p| {
            let fleets = navalact::own_fleets(p)?;
            let planned = plan(&fleets, op, &subject, to.as_deref())?;
            kit::remember(&id, &planned);
            Ok(planned)
        },
        &|p, planned| {
            let now = navalact::own_fleets(p)?;
            let verdict = match planned.cloned().or_else(|| kit::recall::<Plan>(&id)) {
                Some(pl) => settle(&pl, &now),
                None => settle_without_baseline(op, &subject, to.as_deref(), &now),
            };
            Ok(verdict.outcome(json!({"fleets":summary(&now)})))
        },
        &|p, keys, planned| drive(p, keys, planned, drag),
    )
}

// ---- tool list ------------------------------------------------------------------------------------
fn schema(name: &str, description: &str, props: Value, required: &[&str]) -> Value {
    let mut properties = props.as_object().cloned().unwrap_or_default();
    properties.insert("request_id".into(), json!({"type":"string"}));
    properties.insert("lease_token".into(), json!({"type":"string"}));
    let mut required: Vec<&str> = required.to_vec();
    required.extend(["request_id", "lease_token"]);
    json!({"name":name,"description":format!("{}{}", kit::UNVALIDATED, description),
        "inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false}})
}
pub(super) fn tools() -> Vec<Value> {
    let tf = json!({"type":"string","maxLength":128});
    let refusals = "Typed refusals before input: fleet_unknown / task_force_unknown / task_force_ambiguous, nothing_to_change, reserve_fleet; while driving button_disabled (the game greys the button), input_readback_failed (the fleet list did not change; nothing is repeated). Result has validated:false.";
    vec![
        schema("hoi4_fleet_merge", &format!("Merge all task forces of one own fleet into a single task force: Strategic Navy map mode (F2), the fleet's card, no task force selected, the fleet panel toolbar's Merge button (btn_merge, 'Merge the selected task forces'). Verified by the fleet holding exactly one task force with the same ships. {refusals}"), json!({"fleet":tf}), &["fleet"]),
        schema("hoi4_fleet_split", &format!("Split one own task force in half: fleet card, the entry's Select, toolbar 'Split the selected task force in half' (btn_split_in_half). Verified by the fleet gaining one task force with the same ships. Adds refusal task_force_too_small (fewer than two ships). {refusals}"), json!({"task_force":tf}), &["task_force"]),
        schema("hoi4_fleet_balance", &format!("Rebalance the ships of one own fleet's task forces with the toolbar's Task-force-o-matic button (btn_taskforce_o_matic; the button's meaning is inferred from its name, hover it first). Verified by the same ships grouped differently. {refusals}"), json!({"fleet":tf}), &["fleet"]),
        schema("hoi4_fleet_reserve", &format!("Send one own task force to the reserve fleet with the toolbar's set-as-reserve button (btn_exclude); a confirmation dialog is accepted. Verified by the task force being in the reserve fleet. {refusals}"), json!({"task_force":tf}), &["task_force"]),
        schema("hoi4_fleet_delete", &format!("DESTROY one own task force and its ships with the toolbar delete button (btn_delete) and the 'Delete task force' confirmation. Needs confirm:\"destroy_ships\" (else confirmation_required, nothing sent). Verified by the task force being gone. {refusals}"), json!({"task_force":tf,"confirm":{"type":"string","enum":["destroy_ships"]}}), &["task_force", "confirm"]),
        schema("hoi4_fleet_assign", &format!("Move one own task force into another own fleet: select the task force, then RIGHT-click the receiving fleet's card (route right_click, the army cards' convention) or drag the entry onto the card (route drag); a reassign confirmation is accepted. Verified by the task force being in the receiving fleet. Adds refusal target_is_reserve. {refusals}"), json!({"task_force":tf,"to_fleet":tf,"route":{"type":"string","enum":["right_click","drag"]}}), &["task_force", "to_fleet"]),
    ]
}

#[cfg(test)]
mod tests {
    use super::super::naval::Ship;
    use super::*;

    fn ship(id: i32) -> Ship {
        Ship {
            id,
            name: format!("S{id}"),
            unit_type: "destroyer".into(),
            hull: String::new(),
            variant: String::new(),
            strength: 1.0,
            max_strength: 1.0,
            max_organisation: 1.0,
            experience_points: 0.0,
            experience_factor: 0.0,
            morale: 1.0,
        }
    }
    fn tf(name: &str, code: i32, ships: &[i32]) -> TaskForce {
        TaskForce { name: name.into(), place: None, mission_code: code, ships: ships.iter().map(|i| ship(*i)).collect() }
    }
    fn fleet(name: &str, tfs: Vec<TaskForce>) -> Fleet {
        Fleet { name: name.into(), home_port: None, regions: Vec::new(), admiral: None, task_forces: tfs }
    }
    fn world() -> Vec<Fleet> {
        vec![
            fleet("Kriegsmarine", vec![tf("Hochseeflotte", 0, &[1, 2, 3, 4]), tf("Ostseeflotte", 1, &[5, 6])]),
            fleet("U-Flotte ", vec![tf("I. U-Boots", 0, &[7])]),
            fleet("Reserve", vec![tf("Reserve fleet 1", 8, &[8, 9])]),
        ]
    }

    #[test]
    fn plans_refuse_before_input() {
        let f = world();
        assert_eq!(plan(&f, Op::Merge, "kriegsmarine", None).unwrap().task_forces_before, 2);
        assert_eq!(plan(&f, Op::Merge, "U-Flotte", None).unwrap_err().code, "nothing_to_change");
        assert_eq!(plan(&f, Op::Merge, "Reserve", None).unwrap_err().code, "reserve_fleet");
        assert_eq!(plan(&f, Op::Merge, "Nope", None).unwrap_err().code, "fleet_unknown");
        assert_eq!(plan(&f, Op::Balance, "Kriegsmarine", None).unwrap().partition.len(), 2);
        assert_eq!(plan(&f, Op::Split, "Hochseeflotte", None).unwrap().ship_ids.len(), 6);
        assert_eq!(plan(&f, Op::Split, "I. U-Boots", None).unwrap_err().code, "task_force_too_small");
        assert_eq!(plan(&f, Op::Split, "Reserve fleet 1", None).unwrap_err().code, "reserve_fleet");
        assert_eq!(plan(&f, Op::Split, "Nope", None).unwrap_err().code, "task_force_unknown");
        assert_eq!(plan(&f, Op::Reserve, "Reserve fleet 1", None).unwrap_err().code, "nothing_to_change");
        assert_eq!(plan(&f, Op::Reserve, "Ostseeflotte", None).unwrap().fleet, "Kriegsmarine");
        assert_eq!(plan(&f, Op::Delete, "Reserve fleet 1", None).unwrap_err().code, "reserve_fleet");
        assert_eq!(plan(&f, Op::Delete, "I. U-Boots", None).unwrap().task_force.as_deref(), Some("I. U-Boots"));
    }
    #[test]
    fn assign_plans() {
        let f = world();
        let p = plan(&f, Op::Assign, "Ostseeflotte", Some("u-flotte")).unwrap();
        assert_eq!(p.target.as_deref(), Some("U-Flotte "));
        assert_eq!(p.order, vec!["Kriegsmarine", "U-Flotte ", "Reserve"]);
        assert_eq!(plan(&f, Op::Assign, "Ostseeflotte", Some("Kriegsmarine")).unwrap_err().code, "nothing_to_change");
        assert_eq!(plan(&f, Op::Assign, "Ostseeflotte", Some("Reserve")).unwrap_err().code, "target_is_reserve");
        assert_eq!(plan(&f, Op::Assign, "Ostseeflotte", Some("Nope")).unwrap_err().code, "fleet_unknown");
        assert_eq!(plan(&f, Op::Assign, "Ostseeflotte", None).unwrap_err().code, "invalid_arguments");
        assert_eq!(plan(&f, Op::Assign, "Reserve fleet 1", Some("Kriegsmarine")).unwrap_err().code, "reserve_fleet");
    }
    #[test]
    fn merge_split_balance_settlement() {
        let f = world();
        let merge = plan(&f, Op::Merge, "Kriegsmarine", None).unwrap();
        assert_eq!(settle(&merge, &f), Verdict::Unchanged);
        let mut after = world();
        let moved = after[0].task_forces.remove(1);
        after[0].task_forces[0].ships.extend(moved.ships);
        assert_eq!(settle(&merge, &after), Verdict::Reached);
        // A ship lost meanwhile is not a merge result.
        after[0].task_forces[0].ships.pop();
        assert_eq!(settle(&merge, &after), Verdict::Diverged);

        let split = plan(&f, Op::Split, "Hochseeflotte", None).unwrap();
        assert_eq!(settle(&split, &f), Verdict::Unchanged);
        let mut after = world();
        let half = after[0].task_forces[0].ships.split_off(2);
        after[0].task_forces.push(tf("New", 0, &[]));
        after[0].task_forces[2].ships = half;
        assert_eq!(settle(&split, &after), Verdict::Reached);

        let balance = plan(&f, Op::Balance, "Kriegsmarine", None).unwrap();
        assert_eq!(settle(&balance, &f), Verdict::Unchanged);
        let mut after = world();
        let s = after[0].task_forces[0].ships.pop().unwrap();
        after[0].task_forces[1].ships.push(s);
        assert_eq!(settle(&balance, &after), Verdict::Reached);
        assert_eq!(settle(&balance, &[]), Verdict::Unknown);
    }
    #[test]
    fn reserve_delete_assign_settlement() {
        let f = world();
        let reserve = plan(&f, Op::Reserve, "Ostseeflotte", None).unwrap();
        assert_eq!(settle(&reserve, &f), Verdict::Unchanged);
        let mut after = world();
        let moved = after[0].task_forces.remove(1);
        after[2].task_forces.push(TaskForce { mission_code: 8, ..moved });
        assert_eq!(settle(&reserve, &after), Verdict::Reached);
        let mut gone = world();
        gone[0].task_forces.remove(1);
        assert_eq!(settle(&reserve, &gone), Verdict::Unknown);

        let delete = plan(&f, Op::Delete, "Ostseeflotte", None).unwrap();
        assert_eq!(settle(&delete, &f), Verdict::Unchanged);
        assert_eq!(settle(&delete, &gone), Verdict::Reached);

        let assign = plan(&f, Op::Assign, "Ostseeflotte", Some("U-Flotte")).unwrap();
        assert_eq!(settle(&assign, &f), Verdict::Unchanged);
        let mut after = world();
        let moved = after[0].task_forces.remove(1);
        after[1].task_forces.push(moved);
        assert_eq!(settle(&assign, &after), Verdict::Reached);
        let mut elsewhere = world();
        let moved = elsewhere[0].task_forces.remove(1);
        elsewhere[2].task_forces.push(moved);
        assert_eq!(settle(&assign, &elsewhere), Verdict::Diverged);
        assert_eq!(settle(&assign, &gone), Verdict::Unknown);
    }
    #[test]
    fn retry_without_baseline_settles_by_arguments() {
        let f = world();
        assert_eq!(settle_without_baseline(Op::Merge, "Kriegsmarine", None, &f), Verdict::Unchanged);
        assert_eq!(settle_without_baseline(Op::Delete, "Nope", None, &f), Verdict::Reached);
        assert_eq!(settle_without_baseline(Op::Reserve, "Ostseeflotte", None, &f), Verdict::Unchanged);
        assert_eq!(settle_without_baseline(Op::Reserve, "Reserve fleet 1", None, &f), Verdict::Reached);
        assert_eq!(settle_without_baseline(Op::Assign, "I. U-Boots", Some("U-Flotte"), &f), Verdict::Reached);
        assert_eq!(settle_without_baseline(Op::Split, "Hochseeflotte", None, &f), Verdict::Unknown);
    }
    #[test]
    fn plans_survive_the_baseline_round_trip() {
        let p = plan(&world(), Op::Assign, "Ostseeflotte", Some("U-Flotte")).unwrap();
        kit::remember("fleetops-test-1", &p);
        assert_eq!(kit::recall::<Plan>("fleetops-test-1"), Some(p));
        assert_eq!(kit::recall::<Plan>("fleetops-test-missing"), None);
    }
    #[test]
    fn tool_list_has_one_entry_per_tool() {
        let list = tools();
        assert_eq!(list.len(), TOOLS.len());
        for (entry, name) in list.iter().zip(TOOLS) {
            assert_eq!(entry["name"], name);
            assert!(entry["description"].as_str().unwrap().starts_with("UNVALIDATED (no live test yet): "));
            let required = entry["inputSchema"]["required"].as_array().unwrap();
            assert!(required.iter().any(|v| v == "request_id") && required.iter().any(|v| v == "lease_token"));
        }
        assert!(handles("hoi4_fleet_delete") && !handles("hoi4_fleet_create"));
    }
}
