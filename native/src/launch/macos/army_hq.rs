//! Army headquarters (deployable generals, HOI4 1.20): whether the mechanic is in force in the running game, and
//! what it is worth, read from the game's own files (engineer 110, 28 Nov 2026).
//!
//! The game's rule (1.20.1.0, `common/defines/00_defines.lua` NMilitary): an army's planning cap and planning speed
//! are scaled by how far its deployed army HQ stands behind the front (`PLANNING_CAP_COMMS_SCALING`,
//! `PLANNING_SPEED_COMMS_SCALING`, index = provinces behind the front, the last value repeated past
//! `COMMS_MAX_DISTANCE`), and by `*_NO_HQ_SCALING` when the army has no deployed HQ in the same front order; leader
//! modifiers and active abilities get `LEADER_MOD_*` / `ABILITY_*` the same way. The HQ keeps the distance its
//! proximity setting asks for (`GENERAL_PROXIMITY_CLOSE/MEDIUM/FAR`). Deploying reserves command power (the HQ
//! template's battalions' `deployment_cost`), takes days scaled by the HQ template's manpower
//! (`UNIT_LEADER_MODIFIER_COOLDOWN_ON_DEPLOY*`), and requisitions men and equipment from the army's divisions down
//! to `ARMY_HQ_REQUISITION_MINIMUM_REMAINING_PERCENTAGE`.
//!
//! THE GATE (read from the executable, 1.20.0.0 = 1.20.1.0 code): the routine that computes every one of these
//! scalings (1.20.0.0 0x10008ab90, called with the planning-speed table by 0x1000926e0) first asks the game's DLC
//! feature test (0x1006b6bc0, feature 0x39 = bit 0x2 of the feature byte set at start-up only when the DLC
//! "Thunder at Our Gates" is enabled, 0x1006b6ac2) and returns 1.0 without it; it also returns 1.0 for an army with
//! no front order. The HQ battalions (`common/units`, `allow_in_army_hq = yes`) and the default HQ templates
//! (`history/general`) carry the same DLC. So without that DLC no army is scaled down for want of an HQ, and no HQ
//! can be deployed: the mechanic is not part of that game. With the DLC every number below is in force.
//!
//! The DLC name of the engine's gate is the executable's (a mod cannot change it); the script-side DLC is read from
//! the HQ battalions so a mod that gates them otherwise is reported as it is. Numbers come only from the files: a
//! define the files do not carry is left out, never guessed.
use crate::error::{Error, Result};
use crate::focus_rules::live::Dlcs;
use crate::plan_predicate::Truth;
use crate::platform::Process;
use crate::script::{self, Block, Val};
use serde_json::{json, Map, Value};
use std::path::{Path, PathBuf};

pub(super) const TOOL: &str = "hoi4_army_hq";

/// The DLC whose feature bit the executable's scaling routine tests (see the module comment).
pub(super) const ENGINE_GATE_DLC: &str = "Thunder at Our Gates";

/// The NMilitary defines of the mechanic, in the order they are reported.
const DEFINES: [&str; 21] = [
    "COMMS_MAX_DISTANCE",
    "PLANNING_CAP_COMMS_SCALING",
    "PLANNING_CAP_NO_HQ_SCALING",
    "PLANNING_SPEED_COMMS_SCALING",
    "PLANNING_SPEED_NO_HQ_SCALING",
    "LEADER_MOD_COMMS_SCALING",
    "LEADER_MOD_NO_HQ_SCALING",
    "ABILITY_COMMS_SCALING",
    "ABILITY_NO_HQ_SCALING",
    "GENERAL_PROXIMITY_CLOSE",
    "GENERAL_PROXIMITY_MEDIUM",
    "GENERAL_PROXIMITY_FAR",
    "GENERAL_PROXIMITY_DEFAULT",
    "UNIT_LEADER_MODIFIER_COOLDOWN_ON_DEPLOY",
    "UNIT_LEADER_MODIFIER_COOLDOWN_ON_DEPLOY_MIN",
    "UNIT_LEADER_MODIFIER_COOLDOWN_ON_WITHDRAW",
    "UNIT_LEADER_MODIFIER_COOLDOWN_ON_WITHDRAW_MIN",
    "UNIT_LEADER_MODIFIER_COOLDOWN_REFERENCE_MANPOWER",
    "UNIT_LEADER_MODIFIER_COOLDOWN_MANPOWER_EXPONENT",
    "ARMY_HQ_REQUISITION_MINIMUM_REMAINING_PERCENTAGE",
    "PLANNING_MAX",
];

/// A define's value: a number or a `{ a, b, c }` table of numbers.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Def {
    Num(f64),
    Table(Vec<f64>),
}

impl Def {
    fn json(&self) -> Value {
        match self {
            Def::Num(v) => json!(v),
            Def::Table(t) => json!(t),
        }
    }
    fn num(&self) -> Option<f64> {
        match self {
            Def::Num(v) => Some(*v),
            Def::Table(_) => None,
        }
    }
    fn table(&self) -> Option<&[f64]> {
        match self {
            Def::Table(t) => Some(t),
            Def::Num(_) => None,
        }
    }
}

/// The last assignment of `name` in a defines text (`NAME = 0.8,` or `NAME = { 1.0, 0.95 },` inside its block, or
/// `NDefines.NMilitary.NAME = ...` from a mod), comments cut. A table must close on its own line.
pub(super) fn define_in(text: &str, name: &str) -> Option<Def> {
    let dotted = format!(".{name}");
    text.lines()
        .filter_map(|l| {
            let l = l.split("--").next()?.trim();
            let (lhs, rhs) = l.split_once('=')?;
            let lhs = lhs.trim();
            if lhs != name && !lhs.ends_with(&dotted) {
                return None;
            }
            let rhs = rhs.trim().trim_end_matches([',', ';']).trim();
            if let Some(inner) = rhs.strip_prefix('{') {
                let inner = inner.strip_suffix('}')?;
                let t: Vec<f64> = inner
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.parse().ok())
                    .collect::<Option<_>>()?;
                return (!t.is_empty()).then_some(Def::Table(t));
            }
            rhs.parse().ok().map(Def::Num)
        })
        .next_back()
}

/// The mechanic's defines from a content folder: `common/defines/*.lua` in name order, a later file's assignment wins
/// (the way the game loads a mod's define overrides).
pub(super) fn defines(content: &Path) -> Vec<(&'static str, Def)> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(content.join("common/defines"))
        .map(|r| {
            r.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "lua"))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    let texts: Vec<String> = files
        .iter()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .collect();
    DEFINES
        .iter()
        .filter_map(|name| {
            texts
                .iter()
                .filter_map(|t| define_in(t, name))
                .next_back()
                .map(|d| (*name, d))
        })
        .collect()
}

/// The HQ battalions of `common/units` (`allow_in_army_hq = yes`): key, the DLCs they require, command power each.
pub(super) fn hq_battalions(entries: &[script::Entry]) -> Vec<(String, Vec<String>, Option<f64>)> {
    let mut out = Vec::new();
    for top in entries.iter().filter(|e| e.key == "sub_units") {
        for unit in top.val.block() {
            let Val::Block(body) = &unit.val else {
                continue;
            };
            if body.text("allow_in_army_hq") != Some("yes") {
                continue;
            }
            let dlcs = body
                .all("required_dlc")
                .flat_map(|v| match v {
                    Val::Scalar(s) => vec![s.trim_matches('"').to_string()],
                    Val::Block(b) => b
                        .iter()
                        .filter_map(|e| {
                            let s = if e.key.is_empty() {
                                e.val.scalar()?
                            } else {
                                e.key.as_str()
                            };
                            Some(s.trim_matches('"').to_string())
                        })
                        .collect(),
                })
                .collect();
            out.push((unit.key.clone(), dlcs, body.number("deployment_cost")));
        }
    }
    out
}

/// The scaling an army gets with its HQ `distance` provinces behind the front (the table's last value past its end).
pub(super) fn scaling_at(table: &[f64], distance: usize) -> Option<f64> {
    table
        .get(distance.min(table.len().checked_sub(1)?))
        .copied()
}

/// Deploy or withdraw days for an HQ template of `manpower`, as the defines' comments state the rule:
/// base x (manpower / reference) ^ exponent, at least the minimum (an estimate: the formula is the comments').
pub(super) fn cooldown_days(
    defs: &[(&str, Def)],
    base: &str,
    min: &str,
    manpower: f64,
) -> Option<f64> {
    let get = |n: &str| {
        defs.iter()
            .find(|(k, _)| *k == n)
            .and_then(|(_, d)| d.num())
    };
    let (b, m) = (get(base)?, get(min)?);
    let reference = get("UNIT_LEADER_MODIFIER_COOLDOWN_REFERENCE_MANPOWER")?;
    let exp = get("UNIT_LEADER_MODIFIER_COOLDOWN_MANPOWER_EXPONENT")?;
    (reference > 0.0).then(|| (b * (manpower / reference).powf(exp)).max(m))
}

/// What the mechanic is worth per proximity setting and without an HQ, from the defines (pure).
pub(super) fn worth(defs: &[(&'static str, Def)]) -> Value {
    let get = |n: &str| defs.iter().find(|(k, _)| *k == n).map(|(_, d)| d);
    let mut rows = Vec::new();
    for (setting, key) in [
        ("close", "GENERAL_PROXIMITY_CLOSE"),
        ("medium", "GENERAL_PROXIMITY_MEDIUM"),
        ("far", "GENERAL_PROXIMITY_FAR"),
    ] {
        let Some(d) = get(key).and_then(Def::num) else {
            continue;
        };
        let at = |t: &str| {
            get(t)
                .and_then(Def::table)
                .and_then(|tab| scaling_at(tab, d.max(0.0) as usize))
        };
        rows.push(json!({
            "proximity": setting, "provinces_behind_front": d,
            "planning_cap": at("PLANNING_CAP_COMMS_SCALING"),
            "planning_speed": at("PLANNING_SPEED_COMMS_SCALING"),
            "leader_modifiers": at("LEADER_MOD_COMMS_SCALING"),
            "abilities": at("ABILITY_COMMS_SCALING"),
        }));
    }
    let n = |k: &str| get(k).and_then(Def::num);
    json!({
        "with_hq": rows,
        "without_hq": {
            "planning_cap": n("PLANNING_CAP_NO_HQ_SCALING"),
            "planning_speed": n("PLANNING_SPEED_NO_HQ_SCALING"),
            "leader_modifiers": n("LEADER_MOD_NO_HQ_SCALING"),
            "abilities": n("ABILITY_NO_HQ_SCALING"),
        },
    })
}

/// The whole answer for a content folder and the DLC state (pure apart from reading files).
pub(super) fn mechanic(content: &Path, dlcs: &Dlcs) -> Value {
    let defs = defines(content);
    let battalions = hq_battalions(&script::parse_dir(&content.join("common/units")));
    let gate = dlcs.has(ENGINE_GATE_DLC);
    let active = match gate {
        Truth::True => json!(true),
        Truth::False => json!(false),
        Truth::Unknown => Value::Null,
    };
    let mut script_dlcs: Vec<String> = battalions.iter().flat_map(|(_, d, _)| d.clone()).collect();
    script_dlcs.sort();
    script_dlcs.dedup();
    let why = match gate {
        Truth::True => format!(
            "{ENGINE_GATE_DLC} is enabled: an army whose general has no deployed HQ in its front order plans at \
             the without_hq scaling; a deployed HQ at the proximity's distance gives the with_hq row"
        ),
        Truth::False => format!(
            "{ENGINE_GATE_DLC} is not enabled: the game's scaling routine returns 1.0 for every army, so no army \
             loses planning for want of an HQ, and no HQ can be deployed (its battalions and templates need the \
             DLC). Nothing to deploy or to judge in this game."
        ),
        Truth::Unknown => format!(
            "whether {ENGINE_GATE_DLC} is enabled could not be read (dlc_load.json unreadable): the mechanic may or \
             may not be in force"
        ),
    };
    let mut d = Map::new();
    for (k, v) in &defs {
        d.insert((*k).to_string(), v.json());
    }
    json!({
        "active": active,
        "engine_gate_dlc": ENGINE_GATE_DLC,
        "hq_battalion_dlcs": script_dlcs,
        "hq_battalions": battalions.iter().map(|(k, d, c)| json!({"key": k, "required_dlc": d, "deployment_cost": c})).collect::<Vec<_>>(),
        "why": why,
        "worth": worth(&defs),
        "deploy_days_at_reference": cooldown_days(&defs, "UNIT_LEADER_MODIFIER_COOLDOWN_ON_DEPLOY",
            "UNIT_LEADER_MODIFIER_COOLDOWN_ON_DEPLOY_MIN",
            defs.iter().find(|(k, _)| *k == "UNIT_LEADER_MODIFIER_COOLDOWN_REFERENCE_MANPOWER")
                .and_then(|(_, d)| d.num()).unwrap_or(0.0)),
        "defines": Value::Object(d),
        // The per-army state (HQ deployed, distance, the scaling in force) is not read from memory yet: on the
        // verified install the DLC is absent, so there is nothing to read; with the DLC the deploy, withdraw and
        // proximity commands (CDeployArmyHqCommand, CWithdrawArmyHqCommand, CSetOrderGroupLeaderProximityCommand)
        // are the route, each behind the game's own check, once a live session with the DLC can prove them.
        "per_army": if gate == Truth::False { json!("not applicable: the mechanic is not in this game") }
            else { json!("not read yet (needs a session with the DLC)") },
    })
}

pub(super) fn call(process: &dyn Process, args: &Map<String, Value>) -> Result<Value> {
    super::allowed(args, &[])?;
    let exe = &process
        .identity()
        .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
        .exe;
    let found = super::game_content(exe)?;
    let dlcs = Dlcs::read(&found.install, found.disabled_dlcs.as_deref());
    Ok(mechanic(&found.content, &dlcs))
}

pub(super) fn tool() -> Value {
    json!({"name": "hoi4_army_hq", "description": "Read-only, no input (files of the running game only): whether army headquarters (deployable generals, 1.20) are in force in this game and what they are worth. active:false when the DLC the executable gates the mechanic on (engine_gate_dlc, Thunder at Our Gates) is not enabled: the game then scales no army's planning for want of an HQ and no HQ can be deployed, so there is nothing to do. With the DLC: worth.with_hq per proximity setting (close/medium/far = provinces behind the front) and worth.without_hq give the planning cap, planning speed, leader-modifier and ability scaling from the defines (1.20.1.0: 0.95/0.9/0.85 vs 0.8 for planning, leader modifiers x1.04 at close); deploy and withdraw days from the HQ template's manpower; the HQ battalions and their command-power cost. Every number is read from common/defines and common/units at call time (a mod's override wins), never assumed.", "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}})
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFS: &str = "NDefines = { NMilitary = {\n\
        COMMS_MAX_DISTANCE = 4,\n\
        PLANNING_CAP_COMMS_SCALING = { 1.0, 0.95, 0.9, 0.85, 0.8 },\t\t-- index J\n\
        PLANNING_CAP_NO_HQ_SCALING = 0.8,\n\
        PLANNING_SPEED_COMMS_SCALING = { 1.0, 0.95, 0.9, 0.85, 0.8 },\n\
        PLANNING_SPEED_NO_HQ_SCALING = 0.8,\n\
        LEADER_MOD_COMMS_SCALING = { 1.06, 1.04, 1.02, 1.01, 1.0 },\n\
        LEADER_MOD_NO_HQ_SCALING = 1.0,\n\
        ABILITY_COMMS_SCALING = { 1.06, 1.04, 1.02, 1.01, 1.0 },\n\
        ABILITY_NO_HQ_SCALING = 1.0,\n\
        GENERAL_PROXIMITY_CLOSE = 1,\n\
        GENERAL_PROXIMITY_MEDIUM = 2,\n\
        GENERAL_PROXIMITY_FAR = 3,\n\
        UNIT_LEADER_MODIFIER_COOLDOWN_ON_DEPLOY = 3,\n\
        UNIT_LEADER_MODIFIER_COOLDOWN_ON_DEPLOY_MIN = 1,\n\
        UNIT_LEADER_MODIFIER_COOLDOWN_REFERENCE_MANPOWER = 2000,\n\
        UNIT_LEADER_MODIFIER_COOLDOWN_MANPOWER_EXPONENT = 1.4,\n\
        } }\n";

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("e110_hq_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("common/defines")).unwrap();
        std::fs::create_dir_all(d.join("common/units")).unwrap();
        d
    }

    #[test]
    fn defines_read_numbers_and_tables_and_a_later_mod_file_wins() {
        let d = tmp("defs");
        std::fs::write(d.join("common/defines/00_defines.lua"), DEFS).unwrap();
        std::fs::write(
            d.join("common/defines/zz_mod.lua"),
            "NDefines.NMilitary.PLANNING_CAP_NO_HQ_SCALING = 0.5\n",
        )
        .unwrap();
        let defs = defines(&d);
        let get = |n: &str| defs.iter().find(|(k, _)| *k == n).map(|(_, v)| v.clone());
        assert_eq!(get("PLANNING_CAP_NO_HQ_SCALING"), Some(Def::Num(0.5)));
        assert_eq!(
            get("PLANNING_CAP_COMMS_SCALING"),
            Some(Def::Table(vec![1.0, 0.95, 0.9, 0.85, 0.8]))
        );
        assert_eq!(
            get("PLANNING_MAX"),
            None,
            "a define the files lack is left out"
        );
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn worth_per_proximity_comes_from_the_tables_at_the_proximity_distance() {
        let defs: Vec<(&'static str, Def)> = DEFINES
            .iter()
            .filter_map(|n| define_in(DEFS, n).map(|d| (*n, d)))
            .collect();
        let w = worth(&defs);
        assert_eq!(w["with_hq"][0]["proximity"], "close");
        assert_eq!(w["with_hq"][0]["planning_cap"], 0.95);
        assert_eq!(w["with_hq"][2]["planning_speed"], 0.85);
        assert_eq!(w["with_hq"][0]["leader_modifiers"], 1.04);
        assert_eq!(w["without_hq"]["planning_cap"], 0.8);
        assert_eq!(
            scaling_at(&[1.0, 0.9], 7),
            Some(0.9),
            "the last value repeats past the end"
        );
        let days = cooldown_days(
            &defs,
            "UNIT_LEADER_MODIFIER_COOLDOWN_ON_DEPLOY",
            "UNIT_LEADER_MODIFIER_COOLDOWN_ON_DEPLOY_MIN",
            500.0,
        )
        .unwrap();
        assert!(
            (days - 1.0).abs() < 1e-9,
            "a small HQ is held at the minimum: {days}"
        );
    }

    #[test]
    fn without_the_gate_dlc_the_mechanic_is_reported_inactive_with_the_reason() {
        let d = tmp("off");
        std::fs::write(d.join("common/defines/00_defines.lua"), DEFS).unwrap();
        std::fs::write(
            d.join("common/units/hq.txt"),
            "sub_units = { hq_support_company = { allow_in_army_hq = yes required_dlc = { \"Thunder at Our Gates\" } deployment_cost = 1 } infantry = { manpower = 1000 } }",
        )
        .unwrap();
        // an install with a dlc folder listing another DLC only: the gate DLC is surely off
        let install = d.join("install");
        std::fs::create_dir_all(install.join("dlc/dlc001_x")).unwrap();
        std::fs::write(
            install.join("dlc/dlc001_x/dlc001.dlc"),
            "name = \"Other\"\n",
        )
        .unwrap();
        let dlcs = Dlcs::read(&install, Some(&[]));
        let m = mechanic(&d, &dlcs);
        assert_eq!(m["active"], false);
        assert!(m["why"].as_str().unwrap().contains("returns 1.0"));
        assert_eq!(m["hq_battalion_dlcs"][0], "Thunder at Our Gates");
        assert_eq!(
            m["hq_battalions"].as_array().unwrap().len(),
            1,
            "only HQ battalions are listed"
        );
        assert_eq!(m["worth"]["without_hq"]["planning_speed"], 0.8);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn the_tool_takes_no_arguments() {
        let t = tool();
        assert_eq!(t["name"], TOOL);
        assert_eq!(t["inputSchema"]["additionalProperties"], false);
    }
}
