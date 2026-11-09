//! FAIR executor guards: refuse an action that breaks a FAIR multiplayer rule
//! (skills/hoi4-campaign-playbook/references/rules-and-modes.md) before any input, in the one
//! place every action passes (`protocol::Server::call`), so the plan runner and the player's
//! direct tool calls are guarded alike. The plan lint (scripts/hoi4facts/lint_fair.py) checks
//! what a plan can break; these guards check the live action.
//!
//! Mode: the first of `_validator.mode`, `_meta.mode`, `run_spec.mode`, `_run_spec.mode` in the
//! standing plan (`Commands::plan_path`), as the lint reads it; anything but "meta" is FAIR, and
//! no plan or no mode is FAIR. META skips every guard. A host-approved exception
//! `_fair.exceptions` {"fair.<rule>": "<reason>"} lets that rule's actions through. Nation and
//! side rules use the plan's own tag (`_validator.tag`, else `_meta.tag`); without one they are
//! not checked.
//!
//! A refusal is the error `fair_rule_refused` whose message starts with the rule id
//! ("fair.naval_mines: ..."), names what was refused and ends with the rule in plain words.
//!
//! Guarded (decided from the action's arguments, the plan and the game's content files):
//!   hoi4_fleet_mission          mine_laying (fair.naval_mines)
//!   hoi4_air_mission            naval_mines (fair.naval_mines)
//!   hoi4_diplomacy_act          asking or offering military access, docking rights or airbase
//!                               access (fair.access; the side exceptions need the corridor, which
//!                               no read verifies: a host exception lets them through); dismantling
//!                               or disbanding a faction (fair.factions); Japan's side joining or
//!                               inviting the European Axis or the reverse (fair.factions); a
//!                               war-goal justification on the Soviet Union by Germany or Japan's
//!                               side (fair.timing: the focus war goal only); volunteers outside
//!                               Spain, the USA to the Allies, the Soviet Union or China and the
//!                               Soviet Union to China (fair.volunteers)
//!   hoi4_faction_leave          when the plan creates the faction it leaves (fair.factions)
//!   hoi4_template_create/_edit  space marines (fair.space_marines) and paratrooper templates
//!                               below 20 width (fair.paratroopers), from the unit files
//!   hoi4_research_start         a technology that unlocks a submarine above Submarine II or an
//!                               AIP or coating module (fair.submarines)
//!   hoi4_special_project_assign a project that unlocks them (fair.submarines)
//!   hoi4_design_create          a submarine hull above II as the base, an AIP or coating module
//!                               (fair.submarines), a mine-laying module (fair.naval_mines)
//!   hoi4_production_add, hoi4_design_set_production  a submarine above II named by its
//!                               equipment key (fair.submarines)
//!   hoi4_operative_assign       an Axis collaboration-government operation on the Soviet Union
//!                               (fair.collaboration)
//!
//! Not guarded (needs live state the action's arguments do not carry; the player and the plan
//! lint own them): convoy_raiding at peace and submarine task forces below 5 (hoi4_fleet_mission),
//! the Italian fleet in port, always engage (no tool sets it), flicker bombing and refinery
//! targets (hoi4_air_mission has no target option), justifications before Danzig, declarations
//! and joins against the timing rules, France's lend-lease before capitulation, expeditionary
//! forces into Europe, leaving a faction the plan does not create, disbanding, editing or moving
//! encircled divisions (hoi4_division_disband, hoi4_template_edit allow_in_use, hoi4_army_*),
//! tanks parked in ports, widths below 10 at war, special forces added to fielded templates,
//! hoi4_train (paratrooper count, ghost divisions), licensed plane designs, French ports and air
//! bases, the invasion prerequisites (hoi4_naval_invasion) and demolition (no tool demolishes).
use crate::error::{Error, Result};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

/// The European Axis, Japan's side and the Allies as the rules name them (historical tags).
const EURO_AXIS: &[&str] = &["GER", "ITA", "HUN", "ROM", "BUL", "FIN", "SLO", "CRO"];
const JAPAN_SIDE: &[&str] = &["JAP", "MAN", "SIA", "MEN"];
const ALLIES: &[&str] = &[
    "ENG", "FRA", "USA", "CAN", "AST", "NZL", "SAF", "RAJ", "HOL", "BEL", "LUX", "PHI",
];
const SPAIN: &[&str] = &["SPR", "SPA", "SPB", "SPC", "SPD"];
const PARA_WIDTH: f64 = 20.0;

/// Every tool a guard looks at (reads and the other actions pass untouched).
pub const GUARDED_TOOLS: &[&str] = &[
    "hoi4_fleet_mission",
    "hoi4_air_mission",
    "hoi4_diplomacy_act",
    "hoi4_faction_leave",
    "hoi4_template_create",
    "hoi4_template_edit",
    "hoi4_research_start",
    "hoi4_special_project_assign",
    "hoi4_design_create",
    "hoi4_production_add",
    "hoi4_design_set_production",
    "hoi4_operative_assign",
];

/// The rules in plain words (the end of every refusal).
fn rule_text(rule: &str) -> &'static str {
    match rule {
        "fair.naval_mines" => "naval mining is banned",
        "fair.access" => {
            "no military access, docking rights or airbase access (only for civil-war supplies)"
        }
        "fair.factions" => {
            "no disbanding factions; Japan's side never joins the Axis and no European Axis nation joins Japan's faction"
        }
        "fair.timing" => "Barbarossa and Japan's war on the Soviet Union start on the focus war goal, never a manual justification",
        "fair.volunteers" => {
            "no volunteers outside Spain (the USA may send them to China, the Allies and the Soviet Union; the Soviet Union to China)"
        }
        "fair.space_marines" => {
            "no space marines: no infantry or cavalry battalions with tank battalions in one division (tank support companies are fine)"
        }
        "fair.paratroopers" => {
            "paratrooper divisions are at least 20 width and a nation has at most 18 of them"
        }
        "fair.submarines" => {
            "submarines: nothing above Submarine II, no special submarine modules (AIP, coating), at least 5 per submarine fleet, no always engage"
        }
        "fair.collaboration" => "no Axis collaboration governments in the Soviet Union",
        _ => "a FAIR rule",
    }
}

/// The run's mode and what the FAIR rules need from the plan.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Run {
    pub fair: bool,
    /// The plan's own tag (`_validator.tag`, else `_meta.tag`).
    pub tag: Option<String>,
    /// `_fair.exceptions`: rule id -> the host's reason.
    pub exceptions: BTreeMap<String, String>,
    /// The plan creates (leads) a faction (`faction.create`).
    pub leads_faction: bool,
}

impl Run {
    /// No plan: FAIR, no tag, no exceptions.
    pub fn default_fair() -> Self {
        Self {
            fair: true,
            ..Self::default()
        }
    }
    pub fn from_plan(plan: &Value) -> Self {
        let mode = ["_validator", "_meta", "run_spec", "_run_spec"]
            .iter()
            .find_map(|k| {
                plan[*k]["mode"]
                    .as_str()
                    .map(str::trim)
                    .filter(|m| !m.is_empty())
            });
        let fair = !mode.is_some_and(|m| m.eq_ignore_ascii_case("meta"));
        let tag = ["_validator", "_meta"]
            .iter()
            .find_map(|k| plan[*k]["tag"].as_str())
            .map(|t| t.trim().to_ascii_uppercase())
            .filter(|t| t.len() == 3);
        let exceptions = plan["_fair"]["exceptions"]
            .as_object()
            .map(|o| {
                o.iter()
                    .map(|(k, v)| {
                        let why = v.as_str().map_or_else(|| v.to_string(), str::to_string);
                        (k.clone(), why)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            fair,
            tag,
            exceptions,
            leads_faction: plan["faction"]["create"].is_object(),
        }
    }
    fn me(&self) -> &str {
        self.tag.as_deref().unwrap_or("")
    }
}

/// One land sub-unit as the FAIR rules classify it (from common/units).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Unit {
    pub width: f64,
    /// Foot infantry (type exactly infantry) or cavalry (category_cavalry).
    pub leg: bool,
    /// A tank battalion (type armor: line tanks, SPGs, tank destroyers, SPAA).
    pub armour: bool,
    pub para: bool,
}

/// What the guards need from the game's content (install plus mods); built by the platform
/// adapter (`Commands::fair_facts`). Without it the content guards are skipped.
#[derive(Clone, Debug, Default)]
pub struct Facts {
    /// Battalions and companies (support companies are left out).
    pub units: HashMap<String, Unit>,
    /// Submarine equipment above Submarine II -> why.
    pub sub_equipment: HashMap<String, String>,
    /// AIP and coating modules -> why.
    pub sub_modules: HashMap<String, String>,
    pub mine_modules: HashSet<String>,
    /// Technologies that unlock the banned equipment or modules -> what they unlock.
    pub techs: HashMap<String, String>,
    /// Special projects that unlock them -> what they unlock.
    pub projects: HashMap<String, String>,
}

fn has(list: &[&str], tag: &str) -> bool {
    list.contains(&tag)
}

/// Rule class of a diplomacy action key or row name (lint_fair.kind_of).
pub fn diplomacy_kind(action: &str) -> &'static str {
    let mut a = String::new();
    for c in action.to_ascii_lowercase().chars() {
        if c.is_ascii_lowercase() {
            a.push(c);
        } else if !a.ends_with('_') {
            a.push('_');
        }
    }
    let a = a.trim_matches('_');
    let words: Vec<&str> = a.split('_').collect();
    if [
        "revoke", "cancel", "end_", "return", "decline", "reject", "refuse", "kick",
    ]
    .iter()
    .any(|p| a.starts_with(p))
    {
        return "other";
    }
    if a.contains("justify") || a == "add_war_goal" || a == "war_goal" {
        return "justify";
    }
    if a.contains("declare") {
        return "declare";
    }
    if a.contains("faction")
        && (a.contains("leave") || a.contains("dismantle") || a.contains("disband"))
    {
        return "leave_faction";
    }
    if a == "join" || (a.contains("join") && a.contains("faction")) {
        return "join";
    }
    if a.contains("invite") {
        return "invite";
    }
    if a.contains("access")
        || a.contains("docking")
        || words.contains(&"dock")
        || a.contains("airbase")
        || a.contains("air_base")
    {
        return "access";
    }
    if a.contains("volunteer") {
        return "volunteers";
    }
    if a.contains("expedition") {
        return "expedition";
    }
    if a.contains("lend_lease") && !a.starts_with("request") {
        return "lend_lease";
    }
    if a.contains("licen") {
        return "licence";
    }
    "other"
}

fn text<'a>(args: &'a Value, key: &str) -> &'a str {
    args[key].as_str().map(str::trim).unwrap_or("")
}

/// The FAIR verdict on one action: Ok to go ahead, else `fair_rule_refused`.
pub fn check(name: &str, args: &Value, run: &Run, facts: Option<&Facts>) -> Result<()> {
    if !run.fair || !GUARDED_TOOLS.contains(&name) {
        return Ok(());
    }
    let Some((rule, what)) = breach(name, args, run, facts) else {
        return Ok(());
    };
    if run.exceptions.contains_key(rule) {
        return Ok(());
    }
    Err(Error::new(
        "fair_rule_refused",
        format!(
            "{rule}: {name} refused before any input: {what}. FAIR rule: {}. A host-approved exception goes in the plan as _fair.exceptions {{\"{rule}\": \"<reason>\"}}; META mode lifts it",
            rule_text(rule)
        ),
    ))
}

/// The rule an action breaks and what breaks it.
fn breach(
    name: &str,
    args: &Value,
    run: &Run,
    facts: Option<&Facts>,
) -> Option<(&'static str, String)> {
    let me = run.me();
    match name {
        "hoi4_fleet_mission" => {
            let mission = text(args, "mission").to_ascii_lowercase();
            (mission.contains("mine_laying") || mission == "mines" || mission == "mine")
                .then(|| ("fair.naval_mines", format!("mission {mission}")))
        }
        "hoi4_air_mission" => {
            let mission = text(args, "mission").to_ascii_lowercase();
            (mission == "naval_mines" || mission == "naval_mine" || mission == "mine_laying")
                .then(|| ("fair.naval_mines", format!("mission {mission}")))
        }
        "hoi4_diplomacy_act" => diplomacy(args, run),
        "hoi4_faction_leave" => run.leads_faction.then(|| {
            (
                "fair.factions",
                "the plan creates this faction (faction.create): its leader leaving dismantles it"
                    .to_string(),
            )
        }),
        "hoi4_template_create" | "hoi4_template_edit" => template(args, facts?),
        "hoi4_research_start" => {
            let tech = text(args, "technology");
            facts?.techs.get(tech).map(|why| {
                (
                    "fair.submarines",
                    format!("technology {tech} unlocks {why}"),
                )
            })
        }
        "hoi4_special_project_assign" => {
            let project = text(args, "project");
            facts?.projects.get(project).map(|why| {
                (
                    "fair.submarines",
                    format!("project {project} unlocks {why}"),
                )
            })
        }
        "hoi4_design_create" => design(args, facts?),
        "hoi4_production_add" | "hoi4_design_set_production" => {
            let facts = facts?;
            ["equipment", "design"]
                .iter()
                .map(|k| text(args, k))
                .find_map(|key| facts.sub_equipment.get(key).map(|why| (key, why)))
                .map(|(key, why)| ("fair.submarines", format!("{key}: {why}")))
        }
        "hoi4_operative_assign" => {
            let op = format!("{} {}", text(args, "operation"), text(args, "mission"))
                .to_ascii_lowercase();
            let target = text(args, "country").to_ascii_uppercase();
            (op.contains("collaboration")
                && target == "SOV"
                && (has(EURO_AXIS, me) || has(JAPAN_SIDE, me)))
            .then(|| {
                (
                    "fair.collaboration",
                    format!("{me} prepares a collaboration government in the Soviet Union"),
                )
            })
        }
        _ => None,
    }
}

fn diplomacy(args: &Value, run: &Run) -> Option<(&'static str, String)> {
    let action = text(args, "action");
    let target = text(args, "country").to_ascii_uppercase();
    let me = run.me();
    let toward = if target.is_empty() {
        String::new()
    } else {
        format!(" toward {target}")
    };
    match diplomacy_kind(action) {
        "access" => Some((
            "fair.access",
            format!("{action}{toward} (the side exceptions need the corridor secured, which no read verifies)"),
        )),
        "leave_faction" => {
            let a = action.to_ascii_lowercase();
            (a.contains("dismantle") || a.contains("disband") || run.leads_faction)
                .then(|| ("fair.factions", format!("{action}: dismantles a faction")))
        }
        "join" | "invite" if !me.is_empty() && !target.is_empty() => {
            let across = (has(JAPAN_SIDE, me) && has(EURO_AXIS, &target))
                || (has(EURO_AXIS, me) && has(JAPAN_SIDE, &target));
            across.then(|| ("fair.factions", format!("{me} {action} {target}")))
        }
        "justify"
            if target == "SOV" && (me == "GER" || has(JAPAN_SIDE, me)) && !me.is_empty() =>
        {
            Some((
                "fair.timing",
                format!("{action} on the Soviet Union by hand: {me} uses the focus war goal"),
            ))
        }
        "volunteers" if !target.is_empty() => {
            let allowed = has(SPAIN, &target)
                || (me == "USA" && (has(ALLIES, &target) || target == "SOV" || target == "CHI"))
                || (me == "SOV" && target == "CHI");
            (!allowed).then(|| ("fair.volunteers", format!("volunteers to {target}")))
        }
        _ => None,
    }
}

fn template(args: &Value, facts: &Facts) -> Option<(&'static str, String)> {
    let columns = args["regiments"].as_array()?;
    let keys: Vec<&str> = columns
        .iter()
        .filter_map(Value::as_array)
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    // An unknown key is the tool's own refusal (unit_unknown); judge only a known layout.
    let units: Vec<&Unit> = keys
        .iter()
        .map(|k| facts.units.get(*k))
        .collect::<Option<_>>()?;
    let leg = units.iter().filter(|u| u.leg).count();
    let armour = units.iter().filter(|u| u.armour).count();
    let name = text(args, "name");
    let name = if name.is_empty() {
        text(args, "template")
    } else {
        name
    };
    if leg > 0 && armour > 0 {
        return Some((
            "fair.space_marines",
            format!("template {name:?} mixes {leg} infantry or cavalry battalions with {armour} tank battalions"),
        ));
    }
    let width: f64 = units.iter().map(|u| u.width).sum();
    if units.iter().any(|u| u.para) && width < PARA_WIDTH {
        return Some((
            "fair.paratroopers",
            format!("paratrooper template {name:?} is {width} wide (at least {PARA_WIDTH})"),
        ));
    }
    None
}

fn design(args: &Value, facts: &Facts) -> Option<(&'static str, String)> {
    if let Some(base) = args["base"].as_str() {
        if let Some(why) = facts.sub_equipment.get(base.trim()) {
            return Some(("fair.submarines", format!("base {base}: {why}")));
        }
    }
    for (slot, module) in args["modules"].as_object().into_iter().flatten() {
        let Some(module) = module.as_str() else {
            continue;
        };
        if let Some(why) = facts.sub_modules.get(module) {
            return Some(("fair.submarines", format!("slot {slot}: {why}")));
        }
        if facts.mine_modules.contains(module) {
            return Some((
                "fair.naval_mines",
                format!("slot {slot}: {module} lays mines"),
            ));
        }
    }
    None
}

/// The run read from the plan file, re-read only when the file changes.
pub fn run_of(path: Option<&std::path::Path>) -> Run {
    use std::sync::{Mutex, OnceLock};
    type Key = (std::path::PathBuf, Option<std::time::SystemTime>, u64);
    static CACHE: OnceLock<Mutex<Option<(Key, Run)>>> = OnceLock::new();
    let Some(path) = path else {
        return Run::default_fair();
    };
    let Ok(meta) = std::fs::metadata(path) else {
        return Run::default_fair();
    };
    let key: Key = (path.to_path_buf(), meta.modified().ok(), meta.len());
    let cache = CACHE.get_or_init(|| Mutex::new(None));
    let mut slot = cache.lock().unwrap_or_else(|p| p.into_inner());
    if let Some((k, run)) = slot.as_ref() {
        if *k == key {
            return run.clone();
        }
    }
    let run = std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .map_or_else(Run::default_fair, |plan| Run::from_plan(&plan));
    *slot = Some((key, run.clone()));
    run
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fair(tag: &str) -> Run {
        Run::from_plan(&json!({"_meta": {"tag": tag}}))
    }

    fn facts() -> Facts {
        let unit = |width: f64, leg: bool, armour: bool, para: bool| Unit {
            width,
            leg,
            armour,
            para,
        };
        let mut f = Facts::default();
        f.units
            .insert("infantry".into(), unit(2.0, true, false, false));
        f.units
            .insert("cavalry".into(), unit(2.0, true, false, false));
        f.units
            .insert("motorized".into(), unit(2.0, false, false, false));
        f.units
            .insert("light_armor".into(), unit(2.0, false, true, false));
        f.units
            .insert("paratrooper".into(), unit(2.0, false, false, true));
        f.units
            .insert("artillery_brigade".into(), unit(3.0, false, false, false));
        f.sub_equipment.insert(
            "ship_hull_submarine_3".into(),
            "ship_hull_submarine_3 is a tier-3 submarine".into(),
        );
        f.sub_modules.insert(
            "ship_anechoic_tiles".into(),
            "ship_anechoic_tiles is a special submarine module".into(),
        );
        f.mine_modules.insert("ship_mine_layer_sub".into());
        f.techs.insert(
            "improved_submarine".into(),
            "ship_hull_submarine_3 is a tier-3 submarine".into(),
        );
        f.projects.insert(
            "sp_naval_aip".into(),
            "ship_aip is a special submarine module".into(),
        );
        f
    }

    fn refused(name: &str, args: Value, run: &Run) -> String {
        let e = check(name, &args, run, Some(&facts())).expect_err(name);
        assert_eq!(e.code, "fair_rule_refused");
        e.message.split(':').next().unwrap().to_string()
    }

    fn allowed(name: &str, args: Value, run: &Run) {
        check(name, &args, run, Some(&facts())).expect(name);
    }

    #[test]
    fn mode_comes_from_the_plan_and_defaults_to_fair() {
        assert!(Run::from_plan(&json!({})).fair);
        assert!(Run::default_fair().fair);
        assert!(!Run::from_plan(&json!({"_validator": {"mode": "META"}})).fair);
        assert!(!Run::from_plan(&json!({"run_spec": {"mode": "meta"}})).fair);
        assert!(Run::from_plan(&json!({"_meta": {"mode": "fair"}})).fair);
        // the first key with a mode decides
        assert!(
            Run::from_plan(&json!({"_validator": {"mode": "fair"}, "_meta": {"mode": "meta"}}))
                .fair
        );
        let run = Run::from_plan(
            &json!({"_meta": {"tag": "ger"}, "_fair": {"exceptions": {"fair.access": "host ok"}}, "faction": {"create": {"name": "X"}}}),
        );
        assert_eq!(run.tag.as_deref(), Some("GER"));
        let run2 = Run::from_plan(&json!({"_validator": {"tag": "ITA", "mode": "fair"}}));
        assert_eq!(run2.tag.as_deref(), Some("ITA"));
        assert_eq!(run.exceptions["fair.access"], "host ok");
        assert!(run.leads_faction);
    }

    #[test]
    fn meta_and_exceptions_and_reads_pass() {
        let meta = Run::from_plan(&json!({"_meta": {"mode": "meta", "tag": "GER"}}));
        allowed(
            "hoi4_fleet_mission",
            json!({"task_force": "A", "mission": "mine_laying"}),
            &meta,
        );
        let ex = Run::from_plan(
            &json!({"_meta": {"tag": "GER"}, "_fair": {"exceptions": {"fair.naval_mines": "host"}}}),
        );
        allowed(
            "hoi4_fleet_mission",
            json!({"task_force": "A", "mission": "mine_laying"}),
            &ex,
        );
        allowed("hoi4_fleets", json!({}), &fair("GER"));
    }

    #[test]
    fn fleet_and_air_mines() {
        let run = fair("GER");
        assert_eq!(
            refused(
                "hoi4_fleet_mission",
                json!({"mission": "mine_laying"}),
                &run
            ),
            "fair.naval_mines"
        );
        allowed(
            "hoi4_fleet_mission",
            json!({"mission": "convoy_raiding"}),
            &run,
        );
        allowed(
            "hoi4_fleet_mission",
            json!({"mission": "mine_sweeping"}),
            &run,
        );
        assert_eq!(
            refused("hoi4_air_mission", json!({"mission": "naval_mines"}), &run),
            "fair.naval_mines"
        );
        allowed(
            "hoi4_air_mission",
            json!({"mission": "naval_minesweeping"}),
            &run,
        );
        allowed(
            "hoi4_air_mission",
            json!({"mission": "strategic_bombing"}),
            &run,
        );
    }

    #[test]
    fn diplomacy_rules() {
        let ger = fair("GER");
        for a in [
            "ask_military_access",
            "offer_docking_rights",
            "Request Airbase Access",
        ] {
            assert_eq!(
                refused(
                    "hoi4_diplomacy_act",
                    json!({"action": a, "country": "ITA"}),
                    &ger
                ),
                "fair.access",
                "{a}"
            );
        }
        allowed(
            "hoi4_diplomacy_act",
            json!({"action": "revoke_military_access", "country": "ITA"}),
            &ger,
        );
        assert_eq!(
            refused(
                "hoi4_diplomacy_act",
                json!({"action": "invite_to_faction", "country": "JAP"}),
                &ger
            ),
            "fair.factions"
        );
        allowed(
            "hoi4_diplomacy_act",
            json!({"action": "invite_to_faction", "country": "ITA"}),
            &ger,
        );
        assert_eq!(
            refused(
                "hoi4_diplomacy_act",
                json!({"action": "join_faction", "country": "GER"}),
                &fair("JAP")
            ),
            "fair.factions"
        );
        assert_eq!(
            refused(
                "hoi4_diplomacy_act",
                json!({"action": "justify_war_goal", "country": "SOV"}),
                &ger
            ),
            "fair.timing"
        );
        allowed(
            "hoi4_diplomacy_act",
            json!({"action": "justify_war_goal", "country": "POL"}),
            &ger,
        );
        // volunteers
        assert_eq!(
            refused(
                "hoi4_diplomacy_act",
                json!({"action": "send_volunteers", "country": "CHI"}),
                &ger
            ),
            "fair.volunteers"
        );
        allowed(
            "hoi4_diplomacy_act",
            json!({"action": "send_volunteers", "country": "SPR"}),
            &ger,
        );
        allowed(
            "hoi4_diplomacy_act",
            json!({"action": "send_volunteers", "country": "CHI"}),
            &fair("SOV"),
        );
        allowed(
            "hoi4_diplomacy_act",
            json!({"action": "send_volunteers", "country": "ENG"}),
            &fair("USA"),
        );
        assert_eq!(
            refused(
                "hoi4_diplomacy_act",
                json!({"action": "send_volunteers", "country": "ENG"}),
                &fair("SOV")
            ),
            "fair.volunteers"
        );
        // a faction the plan creates is not left (dismantled)
        assert_eq!(
            refused(
                "hoi4_diplomacy_act",
                json!({"action": "dismantle_faction"}),
                &ger
            ),
            "fair.factions"
        );
        // without a tag the nation rules are not judged
        allowed(
            "hoi4_diplomacy_act",
            json!({"action": "justify_war_goal", "country": "SOV"}),
            &Run::default_fair(),
        );
        allowed(
            "hoi4_diplomacy_act",
            json!({"action": "improve_relations", "country": "SOV"}),
            &ger,
        );
    }

    #[test]
    fn faction_leave_only_when_the_plan_leads() {
        allowed(
            "hoi4_faction_leave",
            json!({"confirm": "leave_faction"}),
            &fair("ITA"),
        );
        let leader = Run::from_plan(&json!({"_meta": {"tag": "ITA"}, "faction": {"create": {}}}));
        assert_eq!(
            refused(
                "hoi4_faction_leave",
                json!({"confirm": "leave_faction"}),
                &leader
            ),
            "fair.factions"
        );
    }

    #[test]
    fn templates_space_marines_and_paratroopers() {
        let run = fair("GER");
        assert_eq!(
            refused(
                "hoi4_template_create",
                json!({"name": "SM", "regiments": [["infantry", "infantry"], ["light_armor"]]}),
                &run
            ),
            "fair.space_marines"
        );
        assert_eq!(
            refused(
                "hoi4_template_edit",
                json!({"template": "Cav", "regiments": [["cavalry"], ["light_armor"]]}),
                &run
            ),
            "fair.space_marines"
        );
        allowed(
            "hoi4_template_create",
            json!({"name": "Tank", "regiments": [["light_armor", "light_armor"], ["motorized"]]}),
            &run,
        );
        assert_eq!(
            refused(
                "hoi4_template_create",
                json!({"name": "Para", "regiments": [["paratrooper", "paratrooper"]]}),
                &run
            ),
            "fair.paratroopers"
        );
        let wide: Vec<&str> = vec!["paratrooper"; 5];
        allowed(
            "hoi4_template_create",
            json!({"name": "Para", "regiments": [wide.clone(), wide]}),
            &run,
        );
        // only a known layout is judged; an edit without regiments is not a layout change
        allowed(
            "hoi4_template_create",
            json!({"name": "X", "regiments": [["infantry", "mystery_tank"]]}),
            &run,
        );
        allowed(
            "hoi4_template_edit",
            json!({"template": "X", "priority": "elite"}),
            &run,
        );
        // no content facts: the content guards are skipped
        check(
            "hoi4_template_create",
            &json!({"name": "SM", "regiments": [["infantry"], ["light_armor"]]}),
            &run,
            None,
        )
        .unwrap();
    }

    #[test]
    fn submarines_research_projects_designs_and_production() {
        let run = fair("GER");
        assert_eq!(
            refused(
                "hoi4_research_start",
                json!({"technology": "improved_submarine", "slot": 0}),
                &run
            ),
            "fair.submarines"
        );
        allowed(
            "hoi4_research_start",
            json!({"technology": "basic_submarine", "slot": 0}),
            &run,
        );
        assert_eq!(
            refused(
                "hoi4_special_project_assign",
                json!({"facility_id": 1, "project": "sp_naval_aip"}),
                &run
            ),
            "fair.submarines"
        );
        assert_eq!(
            refused(
                "hoi4_design_create",
                json!({"kind": "ship", "base": "ship_hull_submarine_3"}),
                &run
            ),
            "fair.submarines"
        );
        assert_eq!(
            refused(
                "hoi4_design_create",
                json!({"kind": "ship", "base": "ship_hull_submarine_2", "modules": {"fixed_ship_engine_slot": "ship_anechoic_tiles"}}),
                &run
            ),
            "fair.submarines"
        );
        assert_eq!(
            refused(
                "hoi4_design_create",
                json!({"kind": "ship", "base": 12, "modules": {"rear_1_custom_slot": "ship_mine_layer_sub"}}),
                &run
            ),
            "fair.naval_mines"
        );
        allowed(
            "hoi4_design_create",
            json!({"kind": "ship", "base": "ship_hull_submarine_2", "modules": {"a": null}}),
            &run,
        );
        assert_eq!(
            refused(
                "hoi4_production_add",
                json!({"equipment": "ship_hull_submarine_3", "factories": 1}),
                &run
            ),
            "fair.submarines"
        );
        assert_eq!(
            refused(
                "hoi4_design_set_production",
                json!({"line": 0, "design": "ship_hull_submarine_3"}),
                &run
            ),
            "fair.submarines"
        );
        allowed(
            "hoi4_production_add",
            json!({"equipment": "infantry_equipment", "factories": 1}),
            &run,
        );
    }

    #[test]
    fn collaboration_in_the_soviet_union() {
        assert_eq!(
            refused(
                "hoi4_operative_assign",
                json!({"operative": 0, "operation": "prepare_collaboration_government", "country": "SOV"}),
                &fair("GER")
            ),
            "fair.collaboration"
        );
        allowed(
            "hoi4_operative_assign",
            json!({"operative": 0, "operation": "prepare_collaboration_government", "country": "FRA"}),
            &fair("GER"),
        );
        allowed(
            "hoi4_operative_assign",
            json!({"operative": 0, "mission": "build_intel_network", "state_id": 219}),
            &fair("GER"),
        );
    }

    #[test]
    fn refusal_names_the_rule_and_the_exception_key() {
        let e = check(
            "hoi4_fleet_mission",
            &json!({"mission": "mine_laying"}),
            &fair("GER"),
            None,
        )
        .unwrap_err();
        assert!(e
            .message
            .starts_with("fair.naval_mines: hoi4_fleet_mission refused"));
        assert!(e.message.contains("_fair.exceptions"));
        assert!(e.message.contains("naval mining is banned"));
    }

    #[test]
    fn diplomacy_kinds_follow_the_lint() {
        for (a, k) in [
            ("ask_military_access", "access"),
            ("offer_docking_rights", "access"),
            ("revoke_military_access", "other"),
            ("justify_war_goal", "justify"),
            ("add_war_goal", "justify"),
            ("declare_war", "declare"),
            ("leave_faction", "leave_faction"),
            ("join_faction", "join"),
            ("invite_to_faction", "invite"),
            ("kick_from_faction", "other"),
            ("send_volunteers", "volunteers"),
            ("expeditionary_force", "expedition"),
            ("start_lend_lease", "lend_lease"),
            ("request_lend_lease", "other"),
            ("improve_relations", "other"),
        ] {
            assert_eq!(diplomacy_kind(a), k, "{a}");
        }
    }

    #[test]
    fn the_plan_file_is_read_and_cached() {
        let dir = std::env::temp_dir().join(format!("fair_guard_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plan.json");
        assert!(run_of(Some(&path)).fair, "no file: FAIR");
        std::fs::write(&path, r#"{"_meta": {"mode": "meta"}}"#).unwrap();
        assert!(!run_of(Some(&path)).fair);
        std::fs::write(&path, r#"{"_meta": {"mode": "fair", "tag": "ITA"}}"#).unwrap();
        let run = run_of(Some(&path));
        assert!(run.fair);
        assert_eq!(run.tag.as_deref(), Some("ITA"));
        std::fs::write(&path, "not json").unwrap();
        assert!(run_of(Some(&path)).fair, "unreadable plan: FAIR");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(run_of(None).fair);
    }
}
