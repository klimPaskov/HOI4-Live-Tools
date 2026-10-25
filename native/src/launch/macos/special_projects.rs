//! Special projects (HOI4 1.14+): own research facilities, scientists and
//! prototype projects.
//!
//! * `hoi4_special_projects` is a pure memory read (no input, no lease, no
//!   cursor, no pause): the own CProgramStatus (facilities and visible project
//!   pool), the own character collection's scientist list and the breakthrough
//!   accounts.
//! * `hoi4_special_projects_panel` opens the Research panel's Experimental
//!   Facilities tab with ordinary input and reads what only the screen
//!   carries (state names, speeds, a facility's project list with times).
//! * `hoi4_special_project_assign` puts a scientist and/or a project into one
//!   facility through the same panel; success is read back from the facility's
//!   own scientist and project slots in memory.
//!
//! Every offset below was mapped live on macOS 089EA519 on 26 October 2026
//! (Germany, January 1938) by diffing the facility object across ordinary UI
//! actions and by joining against what the screen showed; see the
//! `docs/PLAYBOOK_TOOL_GAPS.md` section "Special projects". Nothing is copied
//! from another platform.
use super::commanders::closest_names;
use super::special_project_rules::{self as rules, Rules};
use super::ui::{clear_notices, read, settled, wait};
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
use std::{collections::BTreeMap, thread, time::Duration};

const GAME: (u32, u32) = (1920, 1080);

// ---- memory layout (image-relative vtables) ---------------------------------
const MANAGER: u64 = 0x670;
const TECH_VIEW: u64 = 0xf0;
const TREE_VIEW: u64 = 0xf8;
const VIEW_ROOT: u64 = 0x430;
/// CFacilitiesTabView pointer inside the technology (Research) view.
const TAB_VIEW: u64 = 0xcf0;
const VT_TAB_VIEW: u64 = 0x3398798;
const TAB_ROSTER: u64 = 0x70;
const TAB_PROGRAM_VIEW: u64 = 0x8b0;
const ROSTER_ROOT: u64 = 0x20;
/// The roster keeps the Recruit Scientist window it created here.
const ROSTER_RECRUIT: u64 = 0xd68;
const VT_RECRUIT_WINDOW: u64 = 0x339bda0;
const RECRUIT_CONFIRM: u64 = 0x980;
const PROGRAM_VIEW_ROOT: u64 = 0x28;
const VT_PROGRAM_VIEW: u64 = 0x339a150;
const VT_ROSTER: u64 = 0x3399fa0;
/// Own country: program status (facilities, projects, breakthroughs) and the
/// character collection (scientist list).
const COUNTRY_PROGRAMS: u64 = 0xd50;
const COUNTRY_CHARACTERS: u64 = 0xd98;
const VT_PROGRAM_STATUS: u64 = 0x3398248;
const VT_PROGRAM: u64 = 0x3394d20;
const VT_PROJECT_POOL: u64 = 0x3397d48;
const VT_PROJECT: u64 = 0x3394e10;
const VT_PROJECT_TEMPLATE: u64 = 0x3395c90;
const VT_STATE_MACHINE: u64 = 0x3397ef8;
const VT_SIMPLE_STATE: u64 = 0x3397e28;
const VT_COUNTRY_CHARACTERS: u64 = 0x332f5b8;
const VT_SCIENTIST: u64 = 0x332fdb0;
const VT_CHARACTER: u64 = 0x332efe0;
const VT_TRAIT: u64 = 0x3330120;
/// Global specialization database (4 templates, ids consecutive).
const SPEC_DB_GLOBAL: u64 = 0x34d0b50;
const SPEC_DB_TEMPLATES: u64 = 0x58;
const PROJECT_STRIDE: u64 = 0x1b8;
/// Scientist list inside the character collection; the recruitment pool is an
/// embedded object at +0xe8 whose first array holds the offered candidates.
const CHARACTERS_SCIENTISTS: u64 = 0xa0;
const CHARACTERS_POOL: u64 = 0xe8;
const POOL_CANDIDATES: u64 = 0x8;
const MAX_PROGRAMS: usize = 64;
const MAX_PROJECTS: usize = 128;
const MAX_SCIENTISTS: usize = 128;
// GUI item classes.
const VT_PROGRAM_ITEM: u64 = 0x33994b0;
const VT_SCIENTIST_ITEM: u64 = 0x3399e08;
const VT_PROJECT_ITEM: u64 = 0x339ada8;
const GRID_ITEMS: u64 = 0x190;
const ITEM_CONTAINER: u64 = 0x10;
const PROGRAM_ITEM_HANDLE: u64 = 0x38;
const PROJECT_ITEM_PROJECT: u64 = 0x28;
const TEXT: u64 = 0x100;

/// Specialization order of `specializations.txt`; the game GUI calls nuclear
/// "Physics".
pub(super) const SPECS: [&str; 4] = ["nuclear", "naval", "air", "land"];
const SPEC_TITLES: [&str; 4] = ["Physics", "Naval", "Air", "Land"];

pub(super) fn spec_of_building(key: &str) -> Option<usize> {
    let base = key.strip_suffix("_facility")?;
    SPECS.iter().position(|s| *s == base)
}

// ---- snapshot -----------------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Facility {
    pub id: i32,
    pub handle: u64,
    pub building: String,
    pub spec: Option<usize>,
    pub scientist: Option<u32>,
    pub project: Option<u32>,
    /// Count of iteration reports not yet opened (program +0x170: 0 to 1 when
    /// an iteration finished, back to 0 once the facility window was opened).
    pub unread_reports: i32,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Scientist {
    pub character: u32,
    pub name: String,
    pub key: String,
    pub skills: [i32; 4],
    pub traits: Vec<(String, String)>,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Project {
    pub id: u32,
    pub key: String,
    pub spec: Option<usize>,
    pub days: u32,
    /// Program handle of the facility running it.
    pub running: u64,
    /// Days left of a stop in progress (the state machine left its prototype
    /// state for the stopping state); None while not stopping.
    pub stopping_days: Option<i32>,
    /// Prototype progress in whole percent (screen: "Project progress: 21%").
    pub progress_percent: i32,
    /// Completed prototype iterations.
    pub iterations: i32,
    /// Progress one iteration adds, minimum and maximum percent (screen:
    /// "gains 15% to 25% each iteration").
    pub gain: (i32, i32),
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Breakthrough {
    pub spec: usize,
    /// Fixed point, 10000 = one point; the fraction is the progress bar.
    pub raw: i64,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Snapshot {
    pub facilities: Vec<Facility>,
    pub scientists: Vec<Scientist>,
    /// Characters the Recruit Scientist window offers (not yet own scientists).
    pub candidates: Vec<Scientist>,
    pub projects: Vec<Project>,
    pub breakthroughs: Vec<Breakthrough>,
}

fn libcxx(r: &Reader<'_>) -> Result<()> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(()),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "Special projects are not reviewed for this build",
        )),
    }
}
fn own_country(r: &Reader<'_>, image: u64) -> Result<u64> {
    let g = game::layout(r);
    let gs = r.u64(image + g.gs_global)?;
    let tag = r.i32(gs + g.player_tag)?;
    let index = i32::from_le_bytes(
        r.array_entry(gs + g.tag_country_index, tag as usize, 4)?
            .try_into()
            .unwrap(),
    );
    let country = u64::from_le_bytes(
        r.array_entry(gs + g.countries, index as usize, 8)?
            .try_into()
            .unwrap(),
    );
    if r.u64(country)? != image + g.country_vtable {
        return Err(Error::new("invalid_type", "Own country required"));
    }
    Ok(country)
}
fn typed(r: &Reader<'_>, image: u64, object: u64, vtable: u64, what: &str) -> Result<()> {
    if object == 0 || r.u64(object)? != image + vtable {
        return Err(Error::new(
            "invalid_type",
            format!("Exact {what} object required"),
        ));
    }
    Ok(())
}
fn hi(handle: u64) -> u32 {
    (handle >> 32) as u32
}
fn lo(handle: u64) -> u32 {
    handle as u32
}
fn spec_index(ids: &[i32; 4], id: i32) -> Option<usize> {
    ids.iter().position(|v| *v == id)
}

/// One bracketed read of everything the memory tool reports.
pub(super) fn snapshot(process: &dyn Process) -> Result<Snapshot> {
    read(process, snapshot_from)
}
fn snapshot_from(r: &Reader<'_>, image: u64) -> Result<Snapshot> {
    libcxx(r)?;
    let country = own_country(r, image)?;
    // specialization ids
    let db = r.u64(image + SPEC_DB_GLOBAL)?;
    let templates = r.pointer_array(db + SPEC_DB_TEMPLATES)?;
    if templates.len() != 4 {
        return Err(Error::new(
            "invalid_array",
            "Four specialization templates required",
        ));
    }
    let mut ids = [0i32; 4];
    for (i, t) in templates.iter().enumerate() {
        ids[i] = r.i32(t + 8)?;
    }
    if (1..4).any(|i| ids[i] != ids[0] + i as i32) {
        return Err(Error::new(
            "invalid_array",
            "Specialization ids are not consecutive",
        ));
    }
    let status = r.u64(country + COUNTRY_PROGRAMS)?;
    typed(r, image, status, VT_PROGRAM_STATUS, "program status")?;
    let mut out = Snapshot::default();
    // facilities
    let (_, data, count) = r.array_header(status + 0x20)?;
    for index in 0..count.min(MAX_PROGRAMS) as u64 {
        let program = r.u64(data + index * 8)?;
        typed(r, image, program, VT_PROGRAM, "program")?;
        let building = r.string_bounded(program + 0x20, 64)?;
        let project = r.u64(program + 0x48)?;
        let scientist = r.u64(program + 0x50)?;
        out.facilities.push(Facility {
            id: r.i32(program + 0x18)?,
            handle: r.u64(program + 8)?,
            spec: spec_of_building(&building),
            building,
            scientist: (scientist != 0).then(|| hi(scientist)),
            project: (project != 0).then(|| lo(project)),
            unread_reports: r.i32(program + 0x170)?,
        });
    }
    // visible project pool
    let pool = r.u64(status + 0x18)?;
    typed(r, image, pool, VT_PROJECT_POOL, "project pool")?;
    let (_, data, count) = r.array_header(pool + 0x10)?;
    for index in 0..count.min(MAX_PROJECTS) as u64 {
        let project = data + index * PROJECT_STRIDE;
        typed(r, image, project, VT_PROJECT, "project")?;
        let template = r.u64(project + 0x20)?.wrapping_sub(0x10);
        typed(r, image, template, VT_PROJECT_TEMPLATE, "project template")?;
        out.projects.push(Project {
            id: r.i32(template + 8)? as u32,
            key: r.string_bounded(template + 0x208, 128)?,
            spec: spec_index(&ids, r.i32(template + 0x10)?),
            days: r.i32(template + 0xc)? as u32,
            running: r.u64(project + 0x60)?,
            stopping_days: stopping_days(r, image, project)?,
            progress_percent: r.i32(r.u64(project + 0x28)? + 0x94)?,
            iterations: r.i32(r.u64(project + 0x28)? + 0x98)?,
            gain: (
                r.i32(r.u64(project + 0x28)? + 0x8c)?,
                r.i32(r.u64(project + 0x28)? + 0x90)?,
            ),
        });
    }
    // breakthrough accounts: libc++ tree, root at +0x60, key high dword = spec id
    let mut stack = vec![r.u64(status + 0x60)?];
    let mut seen = 0;
    while let Some(node) = stack.pop() {
        if node == 0 {
            continue;
        }
        seen += 1;
        if seen > 16 {
            return Err(Error::new("invalid_array", "Breakthrough tree too large"));
        }
        stack.push(r.u64(node)?);
        stack.push(r.u64(node + 8)?);
        let key = r.u64(node + 0x18)?;
        if let Some(spec) = spec_index(&ids, hi(key) as i32) {
            out.breakthroughs.push(Breakthrough {
                spec,
                raw: r.u64(node + 0x20)? as i64,
            });
        }
    }
    out.breakthroughs.sort_by_key(|b| b.spec);
    // scientists
    let characters = r.u64(country + COUNTRY_CHARACTERS)?;
    typed(r, image, characters, VT_COUNTRY_CHARACTERS, "country characters")?;
    let (_, data, count) = r.array_header(characters + CHARACTERS_SCIENTISTS)?;
    for index in 0..count.min(MAX_SCIENTISTS) as u64 {
        let scientist = r.u64(data + index * 8)?;
        if scientist == 0 {
            continue;
        }
        out.scientists
            .push(read_scientist(r, image, &ids, scientist)?);
    }
    // recruitment pool: the characters the Recruit Scientist window offers
    let (_, data, count) = r.array_header(characters + CHARACTERS_POOL + POOL_CANDIDATES)?;
    for index in 0..count.min(16) as u64 {
        let scientist = r.u64(data + index * 8)?;
        if scientist == 0 {
            continue;
        }
        out.candidates
            .push(read_scientist(r, image, &ids, scientist)?);
    }
    Ok(out)
}

/// A project's state machine (+0x28) points at its current state: the
/// CPrototypeProjectState inside the machine while the project is idle or
/// running, a CSimpleProjectState (name "STOPPING_STATE", days at +0x48) while
/// a stop is in progress. Mapped live on 26 October 2026 by stopping a project.
fn stopping_days(r: &Reader<'_>, image: u64, project: u64) -> Result<Option<i32>> {
    let machine = r.u64(project + 0x28)?;
    typed(r, image, machine, VT_STATE_MACHINE, "project state machine")?;
    let state = r.u64(machine + 8)?;
    if state != 0 && r.u64(state)? == image + VT_SIMPLE_STATE {
        return Ok(Some(r.i32(state + 0x48)?));
    }
    Ok(None)
}
fn read_scientist(r: &Reader<'_>, image: u64, ids: &[i32; 4], scientist: u64) -> Result<Scientist> {
    typed(r, image, scientist, VT_SCIENTIST, "scientist")?;
    let character = r.u64(scientist + 8)?;
    typed(r, image, character, VT_CHARACTER, "character")?;
    let (_, levels, n) = r.array_header(scientist + 0xf0)?;
    if n != 4 {
        return Err(Error::new("invalid_array", "Four skill levels required"));
    }
    let mut skills = [0i32; 4];
    for i in 0..4u64 {
        let entry = levels + i * 0x20;
        let id = r.i32(entry)?;
        let at = spec_index(ids, id)
            .ok_or_else(|| Error::new("invalid_array", "Unknown skill specialization"))?;
        skills[at] = r.i32(entry + 0x18)?;
    }
    let mut traits = Vec::new();
    for t in r.pointer_array(scientist + 0x28)? {
        typed(r, image, t, VT_TRAIT, "scientist trait")?;
        traits.push((
            r.string_bounded(t + 0x18, 96)?,
            r.string_bounded(t + 0x90, 96)?,
        ));
    }
    Ok(Scientist {
        character: hi(r.u64(character + 8)?),
        name: r.string_bounded(character + 0x20, 128)?,
        key: r.string_bounded(character + 0x38, 128)?,
        skills,
        traits,
    })
}

impl Snapshot {
    pub(super) fn facility(&self, id: i32) -> Option<&Facility> {
        self.facilities.iter().find(|f| f.id == id)
    }
    pub(super) fn scientist_by_id(&self, character: u32) -> Option<&Scientist> {
        self.scientists.iter().find(|s| s.character == character)
    }
    pub(super) fn scientist_named(&self, name: &str) -> Option<&Scientist> {
        let wanted = name.to_lowercase();
        self.scientists
            .iter()
            .find(|s| s.name.to_lowercase() == wanted || s.key.to_lowercase() == wanted)
    }
    pub(super) fn project_by_key(&self, key: &str) -> Option<&Project> {
        self.projects.iter().find(|p| p.key == key)
    }
    pub(super) fn project_by_id(&self, id: u32) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }
    /// Facility holding this scientist.
    pub(super) fn facility_of(&self, character: u32) -> Option<&Facility> {
        self.facilities
            .iter()
            .find(|f| f.scientist == Some(character))
    }
    pub(super) fn facility_running(&self, project: u32) -> Option<&Facility> {
        self.facilities.iter().find(|f| f.project == Some(project))
    }
    #[cfg(test)]
    pub(super) fn points(&self, spec: usize) -> i64 {
        self.breakthroughs
            .iter()
            .find(|b| b.spec == spec)
            .map_or(0, |b| b.raw.div_euclid(10_000))
    }
}

fn spec_name(spec: Option<usize>) -> Value {
    spec.map_or(Value::Null, |s| json!(SPECS[s]))
}
fn best_skill(skills: &[i32; 4]) -> Option<(usize, i32)> {
    skills
        .iter()
        .enumerate()
        .filter(|(_, l)| **l > 0)
        .max_by_key(|(_, l)| **l)
        .map(|(i, l)| (i, *l))
}
fn scientist_json(snap: &Snapshot, s: &Scientist) -> Value {
    let skills: serde_json::Map<String, Value> = SPECS
        .iter()
        .enumerate()
        .map(|(i, n)| (n.to_string(), json!(s.skills[i])))
        .collect();
    let best = best_skill(&s.skills);
    json!({
        "character_id": s.character,
        "name": s.name,
        "key": s.key,
        "specialization": best.map(|(i, _)| SPECS[i]),
        "level": best.map(|(_, l)| l),
        "skills": skills,
        "traits": s.traits.iter().map(|(k, _)| k).collect::<Vec<_>>(),
        "assigned_facility_id": snap.facility_of(s.character).map(|f| f.id),
    })
}
fn facility_json(snap: &Snapshot, f: &Facility) -> Value {
    let scientist = f
        .scientist
        .and_then(|c| snap.scientist_by_id(c).map(|s| json!({"character_id": s.character, "name": s.name})))
        .or_else(|| f.scientist.map(|c| json!({"character_id": c, "name": null})));
    let project = f.project.map(|id| match snap.project_by_id(id) {
        Some(p) => {
            json!({"key": p.key, "template_id": p.id, "base_days": p.days, "stopping": p.stopping_days.is_some(), "stop_duration_days": p.stopping_days, "progress_percent": p.progress_percent, "iterations_completed": p.iterations, "iteration_gain_percent": [p.gain.0, p.gain.1]})
        }
        None => json!({"key": null, "template_id": id}),
    });
    json!({
        "facility_id": f.id,
        "building": f.building,
        "specialization": spec_name(f.spec),
        "scientist": scientist,
        "project": project,
        "unread_iteration_reports": f.unread_reports,
    })
}
/// Political power a recruit costs: the Recruit Scientist tooltip lists
/// 25/50/75/100 for 0/1/2/3+ idle scientists (RECRUIT_SCIENTIST_COST). Seen live:
/// 100 with fourteen idle scientists.
pub(super) fn recruit_cost(snap: &Snapshot) -> i64 {
    let idle = snap
        .scientists
        .iter()
        .filter(|s| snap.facility_of(s.character).is_none())
        .count();
    25 * (idle.min(3) as i64 + 1)
}
/// Result of `hoi4_special_projects`.
pub(super) fn to_json(snap: &Snapshot) -> Value {
    let specs: Vec<Value> = (0..4)
        .map(|i| {
            let raw = snap
                .breakthroughs
                .iter()
                .find(|b| b.spec == i)
                .map(|b| b.raw);
            json!({
                "specialization": SPECS[i],
                "ui_title": SPEC_TITLES[i],
                "breakthrough_points": raw.map(|r| r.div_euclid(10_000)),
                "breakthrough_progress_percent": raw.map(|r| r.rem_euclid(10_000) as f64 / 100.0),
                "facilities": snap.facilities.iter().filter(|f| f.spec == Some(i)).count(),
                "visible_projects": snap.projects.iter().filter(|p| p.spec == Some(i)).count(),
            })
        })
        .collect();
    let projects: Vec<Value> = snap
        .projects
        .iter()
        .map(|p| {
            json!({
                "key": p.key,
                "specialization": spec_name(p.spec),
                "base_days": p.days,
                "running_in_facility_id": snap.facility_running(p.id).map(|f| f.id),
                "stopping": p.stopping_days.is_some(),
            })
        })
        .collect();
    json!({
        "specializations": specs,
        "facilities": snap.facilities.iter().map(|f| facility_json(snap, f)).collect::<Vec<_>>(),
        "scientists": snap.scientists.iter().map(|s| scientist_json(snap, s)).collect::<Vec<_>>(),
        "recruitable": {
            "cost_political_power": recruit_cost(snap),
            "candidates": snap.candidates.iter().map(|s| scientist_json(snap, s)).collect::<Vec<_>>(),
        },
        "projects": projects,
        "notes": {
            "projects": "The visible project pool. Whether one can start now (technology, breakthrough points, scientist) shows only on screen: hoi4_special_projects_panel with facility_id lists the offered ones (available_now); rules:true adds the vanilla gates.",
            "scientists": "Own scientist list in memory; the roster window may hide some (two level-3 land scientists were absent live). Assigning one it does not offer is refused before the click.",
            "breakthrough": "Points and progress percent; starting a project spends points of its specialization (cost per project in rules or the panel).",
            "state_names": "A facility's state name only shows on its card: hoi4_special_projects_panel.",
            "validated": {"facilities": true, "scientists": true, "projects": true, "breakthroughs": true, "project_progress": true, "iterations": true, "stop_state": true, "scientist_xp": false},
        },
    })
}
/// `hoi4_special_projects`: pure memory read.
pub(super) fn list(process: &dyn Process, with_rules: bool) -> Result<Value> {
    let mut out = to_json(&snapshot(process)?);
    if with_rules {
        attach_rules(&mut out, &game_rules(process));
    }
    Ok(out)
}
pub(super) fn rules_arg(v: Option<&Value>) -> Result<bool> {
    match v {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        _ => Err(Error::new("invalid_arguments", "rules must be a boolean")),
    }
}
/// Vanilla rules of every project from the owned install (empty when the
/// install cannot be found; the read then simply has no `rules`).
fn game_rules(process: &dyn Process) -> BTreeMap<String, Rules> {
    process
        .identity()
        .and_then(|id| super::mapdata::game_dir(&id.exe).ok())
        .map(|dir| rules::load(&dir))
        .unwrap_or_default()
}
/// Put `rules` on every project entry of a result that has a `projects` list.
fn attach_rules(out: &mut Value, rules: &BTreeMap<String, Rules>) {
    if let Some(projects) = out["projects"].as_array_mut() {
        for p in projects {
            if let Some(r) = p["key"].as_str().and_then(|k| rules.get(k)) {
                p["rules"] = r.to_json();
            }
        }
    }
}
pub(super) fn tool() -> Value {
    json!({"name":"hoi4_special_projects","description":"Own special-project state read from memory (no input, no lease, nothing paused): facilities (facility_id, building, specialization, assigned scientist, running project), scientists (name, skills per specialization nuclear/naval/air/land, traits, assigned_facility_id), the visible project pool (key, specialization, base_days, running_in_facility_id) and the four breakthrough accounts (points and progress percent). Specialization 'nuclear' is called Physics on screen. A new facility is built with hoi4_construction_add (building land_facility, air_facility, naval_facility or nuclear_facility). Pass rules:true to add each project's vanilla rules from the install (breakthrough cost, parent projects, required technologies, the available trigger, prototype resources, time and complexity). Use hoi4_special_projects_panel for state names and what can be started.","inputSchema":{"type":"object","properties":{"rules":{"type":"boolean"}},"additionalProperties":false}})
}

// ---- pure pre-checks and settlement -------------------------------------------
/// Everything one assign call will do, decided before any input.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Plan {
    pub facility: i32,
    pub spec: usize,
    pub scientist: Option<Scientist>,
    /// The scientist is not yet in the facility.
    pub assign_scientist: bool,
    pub project: Option<Project>,
}
fn names(snap: &Snapshot) -> Vec<String> {
    snap.scientists.iter().map(|s| s.name.clone()).collect()
}
pub(super) fn plan(
    snap: &Snapshot,
    facility: i32,
    scientist: Option<&str>,
    project: Option<&str>,
) -> Result<Plan> {
    if scientist.is_none() && project.is_none() {
        return Err(Error::new(
            "invalid_arguments",
            "Give a scientist, a project or both",
        ));
    }
    let Some(f) = snap.facility(facility) else {
        return Err(unknown_facility(snap, facility));
    };
    let spec = f
        .spec
        .ok_or_else(|| Error::new("facility_unknown", "Facility specialization unknown"))?;
    let mut chosen = None;
    let mut assign_scientist = false;
    if let Some(name) = scientist {
        let Some(s) = snap.scientist_named(name) else {
            return Err(Error::new(
                "unknown_scientist",
                format!(
                    "No own scientist named {name:?}. Closest: {}. All: {}",
                    closest_names(name, &names(snap), 5).join(", "),
                    names(snap).join(", ")
                ),
            ));
        };
        if s.skills[spec] <= 0 {
            let fits: Vec<String> = snap
                .scientists
                .iter()
                .filter(|o| o.skills[spec] > 0)
                .map(|o| o.name.clone())
                .collect();
            return Err(Error::new(
                "scientist_wrong_specialization",
                format!(
                    "{} has no {} skill; this is a {} facility. Scientists with that skill: {}",
                    s.name,
                    SPECS[spec],
                    SPECS[spec],
                    fits.join(", ")
                ),
            ));
        }
        match snap.facility_of(s.character) {
            Some(other) if other.id == facility => {}
            Some(other) => {
                return Err(Error::new(
                    "scientist_busy",
                    format!(
                        "{} already works in facility {} ({}); unassign them there first",
                        s.name, other.id, other.building
                    ),
                ))
            }
            None => assign_scientist = true,
        }
        if f.scientist.is_some() && assign_scientist && f.project.is_some() {
            return Err(Error::new(
                "facility_has_project",
                "The facility is running a project with another scientist; stop the project or unassign first",
            ));
        }
        chosen = Some(s.clone());
    }
    let mut wanted_project = None;
    if let Some(key) = project {
        let Some(p) = snap.project_by_key(key) else {
            let valid: Vec<&str> = snap
                .projects
                .iter()
                .filter(|p| p.spec == Some(spec))
                .map(|p| p.key.as_str())
                .collect();
            return Err(Error::new(
                "unknown_project",
                format!(
                    "No visible project {key:?}. Visible {} projects: {}",
                    SPECS[spec],
                    valid.join(", ")
                ),
            ));
        };
        if p.spec != Some(spec) {
            return Err(Error::new(
                "project_wrong_specialization",
                format!(
                    "{} is a {} project; this is a {} facility",
                    p.key,
                    p.spec.map_or("unknown", |s| SPECS[s]),
                    SPECS[spec]
                ),
            ));
        }
        if let Some(other) = snap.facility_running(p.id) {
            if other.id != facility {
                return Err(Error::new(
                    "project_busy",
                    format!("{} is already running in facility {}", p.key, other.id),
                ));
            }
            return Err(Error::new(
                "already_assigned",
                format!("{} is already running in facility {facility}", p.key),
            ));
        }
        if f.project.is_some() {
            return Err(Error::new(
                "facility_has_project",
                "The facility already runs a project; stop it first (hoi4_special_project_stop)",
            ));
        }
        if chosen.is_none() && f.scientist.is_none() {
            return Err(Error::new(
                "scientist_required",
                "A project needs a scientist in the facility: pass scientist too",
            ));
        }
        wanted_project = Some(p.clone());
    } else if scientist.is_some() && !assign_scientist {
        return Err(Error::new(
            "already_assigned",
            "That scientist already works in this facility",
        ));
    }
    Ok(Plan {
        facility,
        spec,
        scientist: chosen,
        assign_scientist,
        project: wanted_project,
    })
}

// ---- GUI ------------------------------------------------------------------------
fn tech_view(r: &Reader<'_>, image: u64) -> Result<u64> {
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let manager = r.u64(idler + MANAGER)?;
    r.u64(manager + TECH_VIEW)
}
fn root(r: &Reader<'_>, image: u64) -> Result<u64> {
    libcxx(r)?;
    let view = tech_view(r, image)?;
    gui_tree::container(r, r.u64(view + VIEW_ROOT)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Research view root unavailable"))
}
fn named(r: &Reader<'_>, image: u64, object: u64, name: &str) -> Result<u64> {
    let child = gui_tree::child(r, image, object, name)?
        .ok_or_else(|| Error::new("gui_target_unavailable", format!("{name} not found")))?;
    gui_tree::container(r, child, image)?
        .ok_or_else(|| Error::new("invalid_gui", format!("{name} is not a window")))
}
fn panel_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    gui_tree::visible(r, root(r, image)?)
}
/// The Experimental Facilities tab is on screen. The `facilities` container
/// keeps a stale shown flag while the panel reopens on the research slots
/// tab (seen live), so the embedded `facilities_view` is checked too.
fn facilities_shown(r: &Reader<'_>, image: u64) -> Result<bool> {
    let root = root(r, image)?;
    if !gui_tree::visible(r, root)? {
        return Ok(false);
    }
    let facilities = named(r, image, root, "facilities")?;
    if !gui_tree::visible(r, facilities)? {
        return Ok(false);
    }
    gui_tree::visible(r, named(r, image, facilities, "facilities_view")?)
}
/// The technology tree (a separate view, opened by a research slot click)
/// covers the panel; Escape closes it.
fn tree_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    libcxx(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let view = r.u64(r.u64(idler + MANAGER)? + TREE_VIEW)?;
    match gui_tree::container(r, r.u64(view + VIEW_ROOT)?, image)? {
        Some(root) => gui_tree::visible(r, root),
        None => Ok(false),
    }
}
fn tab_rect(r: &Reader<'_>, image: u64) -> Result<Rect> {
    let root = root(r, image)?;
    let tabs = named(r, image, root, "tabs")?;
    let tab = gui_tree::child(r, image, tabs, "facilities_tab")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Facilities tab not found"))?;
    gui_tree::rect(r, image, tab, GAME)
}
fn tab_view(r: &Reader<'_>, image: u64) -> Result<u64> {
    let tab = r.u64(tech_view(r, image)? + TAB_VIEW)?;
    typed(r, image, tab, VT_TAB_VIEW, "facilities tab view")?;
    Ok(tab)
}
fn items(r: &Reader<'_>, grid: u64) -> Result<Vec<u64>> {
    let (_, data, count) = r.array_header(grid + GRID_ITEMS)?;
    if count > 256 {
        return Err(Error::new("invalid_gui", "Grid item list exceeds bound"));
    }
    let bytes = if count == 0 {
        Vec::new()
    } else {
        r.read(data, count * 8)?
    };
    Ok(bytes
        .chunks_exact(8)
        .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
        .collect())
}
fn item_frame(r: &Reader<'_>, image: u64, item: u64) -> Result<u64> {
    gui_tree::container(r, r.u64(item + ITEM_CONTAINER)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Grid item window required"))
}
fn text(r: &Reader<'_>, image: u64, parent: u64, name: &str) -> Result<Option<String>> {
    match gui_tree::child(r, image, parent, name)? {
        Some(t) => Ok(Some(r.string_bounded(t + TEXT, 256)?)),
        None => Ok(None),
    }
}
/// Game text with its colour markers (0x11 + letter ... 0x11 !) and icon markers removed.
pub(super) fn plain(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{11}' => {
                // colour: followed by one code character ('!' closes)
                chars.next();
            }
            '\u{13}' => {
                // icon token up to '!' or space
                let mut token = String::new();
                for n in chars.by_ref() {
                    if n == '!' || n == ' ' {
                        break;
                    }
                    token.push(n);
                }
                if token.starts_with("infinity") {
                    out.push_str("unknown");
                }
            }
            c if c.is_control() && c != '\n' => {}
            c => out.push(c),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The facilities list (program_list grid).
fn program_grid(r: &Reader<'_>, image: u64) -> Result<u64> {
    let root = root(r, image)?;
    let facilities = named(r, image, root, "facilities")?;
    let view = named(r, image, facilities, "facilities_view")?;
    let list = named(r, image, view, "program_list_window")?;
    gui_tree::child(r, image, list, "program_list")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Facility list not found"))
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Card {
    pub handle: u64,
    pub facility: String,
    pub state: String,
    pub speed: String,
    pub scientist: String,
    pub project: String,
    pub status: String,
    pub scientist_shown: bool,
    /// Click point of the facility icon (opens the project window).
    pub open: Rect,
    /// Click point of the scientist portrait (assign or change).
    pub portrait: Rect,
    pub unassign: Option<Rect>,
}
fn cards(r: &Reader<'_>, image: u64) -> Result<Vec<Card>> {
    let grid = program_grid(r, image)?;
    let mut out = Vec::new();
    for item in items(r, grid)? {
        if r.u64(item)? != image + VT_PROGRAM_ITEM {
            continue;
        }
        let frame = item_frame(r, image, item)?;
        let part = named(r, image, frame, "facility_part_window")?;
        let icons = named(r, image, part, "facility_icon_window")?;
        let open = gui_tree::child(r, image, icons, "build_slot_bg")?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Facility icon not found"))?;
        let sci = named(r, image, frame, "scientist_window")?;
        let portrait = gui_tree::child(r, image, sci, "shadow_for_scientist_card")?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Scientist card not found"))?;
        let assign = gui_tree::child(r, image, sci, "assign_scientist_button")?;
        let change = gui_tree::child(r, image, sci, "change_scientist_button")?;
        let unassign = gui_tree::child(r, image, sci, "unassign_scientist_button")?;
        let scientist_shown = match change {
            Some(b) => gui_tree::shown(r, b)?,
            None => false,
        };
        if !scientist_shown && assign.is_none() {
            return Err(Error::new("invalid_gui", "Scientist buttons not found"));
        }
        let project_window = named(r, image, frame, "ongoing_project_window")?;
        let project = match text(r, image, project_window, "ongoing_project_default")? {
            Some(t) if !t.is_empty() => t,
            _ => text(r, image, project_window, "ongoing_project_display")?.unwrap_or_default(),
        };
        let unassign_rect = match unassign {
            Some(b) if gui_tree::shown(r, b)? => gui_tree::rect(r, image, b, GAME).ok(),
            _ => None,
        };
        out.push(Card {
            handle: r.u64(item + PROGRAM_ITEM_HANDLE)?,
            facility: text(r, image, part, "facility_name")?.unwrap_or_default(),
            state: text(r, image, part, "state_name")?.unwrap_or_default(),
            speed: named(r, image, part, "research_speed_window")
                .and_then(|w| text(r, image, w, "research_speed_text").map(Option::unwrap_or_default))
                .unwrap_or_default(),
            scientist: text(r, image, sci, "scientist_name")?.unwrap_or_default(),
            project,
            status: text(r, image, frame, "status_info_text")?.unwrap_or_default(),
            scientist_shown,
            open: gui_tree::rect_unclipped(r, image, open, GAME)?,
            portrait: gui_tree::rect_unclipped(r, image, portrait, GAME)?,
            unassign: unassign_rect,
        });
    }
    Ok(out)
}
fn card_of(r: &Reader<'_>, image: u64, handle: u64) -> Result<Card> {
    cards(r, image)?
        .into_iter()
        .find(|c| c.handle == handle)
        .ok_or_else(|| Error::new("gui_target_unavailable", "Facility card not in the list"))
}

/// Bring the research panel's Experimental Facilities tab up.
fn open_tab(process: &dyn Process, keys: &MacKeys) -> Result<()> {
    // A technology tree left open by an earlier input hides the panel.
    super::ui::dismiss(process, keys, |r, i| Ok(!tree_open(r, i)?));
    if read(process, tree_open)? {
        return Err(Error::new(
            "ui_blocked",
            "The technology tree stayed open; close it before using special projects",
        ));
    }
    if read(process, facilities_shown)? {
        return Ok(());
    }
    if !read(process, panel_open)? {
        keys.press(Key::letter('w')?)?;
        wait(process, "open research panel", panel_open)?;
        // The panel reopens on the research slots tab; let that settle.
        thread::sleep(Duration::from_millis(300));
    }
    if !read(process, facilities_shown)? {
        let rect = settled(process, "facilities tab", tab_rect)?;
        let (x, y) = rect.center();
        keys.click(GAME, x, y)?;
        wait(process, "open facilities tab", facilities_shown)?;
    }
    Ok(())
}
/// Escape the open sub-windows, then the research panel itself.
fn leave(process: &dyn Process, keys: &MacKeys) {
    super::ui::dismiss(process, keys, |r, i| {
        Ok(!program_view_open(r, i).unwrap_or(false)
            && !roster_open(r, i).unwrap_or(false)
            && !recruit_open(r, i).unwrap_or(false))
    });
    super::ui::dismiss(process, keys, |r, i| Ok(!panel_open(r, i)?));
}
fn program_view_root(r: &Reader<'_>, image: u64) -> Result<Option<u64>> {
    let tab = tab_view(r, image)?;
    let view = r.u64(tab + TAB_PROGRAM_VIEW)?;
    if view == 0 || r.u64(view)? != image + VT_PROGRAM_VIEW {
        return Ok(None);
    }
    gui_tree::container(r, r.u64(view + PROGRAM_VIEW_ROOT)?, image)
}
fn program_view_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    match program_view_root(r, image)? {
        Some(root) => gui_tree::visible(r, root),
        None => Ok(false),
    }
}
fn roster_root(r: &Reader<'_>, image: u64) -> Result<u64> {
    let tab = tab_view(r, image)?;
    let roster = r.u64(tab + TAB_ROSTER)?;
    typed(r, image, roster, VT_ROSTER, "scientist roster")?;
    gui_tree::container(r, r.u64(roster + ROSTER_ROOT)?, image)?
        .ok_or_else(|| Error::new("invalid_gui", "Scientist roster window required"))
}
fn roster_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    gui_tree::visible(r, roster_root(r, image)?)
}
/// (scientist name, portrait click rectangle) of each roster row.
fn roster_rows(r: &Reader<'_>, image: u64) -> Result<Vec<(String, Rect)>> {
    let root = roster_root(r, image)?;
    let scientists = named(r, image, root, "scientists")?;
    let container = named(r, image, scientists, "scientist_list_container")?;
    let grid = gui_tree::child(r, image, container, "scientist_list")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Scientist list not found"))?;
    let mut out = Vec::new();
    for item in items(r, grid)? {
        if r.u64(item)? != image + VT_SCIENTIST_ITEM {
            continue;
        }
        let frame = item_frame(r, image, item)?;
        let name = text(r, image, frame, "name")?.unwrap_or_default();
        let card = gui_tree::child(r, image, frame, "shadow_for_scientist_card")?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Scientist card not found"))?;
        out.push((name, gui_tree::rect_unclipped(r, image, card, GAME)?));
    }
    Ok(out)
}
/// Visible band of the roster list in game pixels.
fn roster_band(r: &Reader<'_>, image: u64) -> (i32, i32) {
    let found = (|| -> Result<(i32, i32)> {
        let root = roster_root(r, image)?;
        let top = gui_tree::child(r, image, root, "scientist_text_search_bg")?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Search box not found"))?;
        let bottom = gui_tree::child(r, image, root, "recruit_scientist_button")?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Recruit button not found"))?;
        let top = gui_tree::rect_unclipped(r, image, top, GAME)?;
        let bottom = gui_tree::rect_unclipped(r, image, bottom, GAME)?;
        Ok((top.y + top.h, bottom.y))
    })();
    found.unwrap_or((170, 700))
}
fn roster_names(r: &Reader<'_>, image: u64) -> Result<Vec<String>> {
    Ok(roster_rows(r, image)?.into_iter().map(|(n, _)| n).collect())
}

/// One project entry of an open facility window.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Entry {
    pub key: String,
    pub name: String,
    pub kind: String,
    pub time: String,
    pub requirement: String,
    pub speed: String,
    /// The game offers it now (its time and speed show; greyed entries show a
    /// question mark blueprint and hide both).
    pub available: bool,
    /// Breakthrough points the entry lists (text of each cost item).
    pub cost: Vec<String>,
    pub select: Rect,
}
fn project_entries(r: &Reader<'_>, image: u64) -> Result<Vec<Entry>> {
    let root = program_view_root(r, image)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Facility window not open"))?;
    let projects = named(r, image, root, "projects")?;
    let available = named(r, image, projects, "available_projects")?;
    let list = named(r, image, available, "available_projects_list")?;
    let grid = gui_tree::child(r, image, list, "project_entries")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Project list not found"))?;
    let mut out = Vec::new();
    for item in items(r, grid)? {
        if r.u64(item)? != image + VT_PROJECT_ITEM {
            continue;
        }
        let project = r.u64(item + PROJECT_ITEM_PROJECT)?;
        typed(r, image, project, VT_PROJECT, "project")?;
        let template = r.u64(project + 0x20)?.wrapping_sub(0x10);
        typed(r, image, template, VT_PROJECT_TEMPLATE, "project template")?;
        let frame = item_frame(r, image, item)?;
        let button = gui_tree::child(r, image, frame, "selectable_button")?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Project button not found"))?;
        let name = match text(r, image, frame, "project_name")? {
            Some(n) if !n.is_empty() => n,
            _ => text(r, image, frame, "project_name_default")?.unwrap_or_default(),
        };
        let shown_text = |name: &str| -> Result<String> {
            match gui_tree::child(r, image, frame, name)? {
                Some(t) if gui_tree::shown(r, t)? => Ok(plain(&r.string_bounded(t + TEXT, 256)?)),
                _ => Ok(String::new()),
            }
        };
        let available = match gui_tree::child(r, image, frame, "project_time")? {
            Some(t) => gui_tree::shown(r, t)?,
            None => false,
        };
        let mut cost = Vec::new();
        if let Some(tab) = gui_tree::child(r, image, frame, "breakthrough_cost_tab")? {
            for entry in items(r, tab).unwrap_or_default() {
                if let Ok(window) = item_frame(r, image, entry) {
                    if let Some(t) = text(r, image, window, "cost_amount")? {
                        cost.push(plain(&t));
                    }
                }
            }
        }
        out.push(Entry {
            key: r.string_bounded(template + 0x208, 128)?,
            name: plain(&name),
            kind: plain(&text(r, image, frame, "project_type")?.unwrap_or_default()),
            time: shown_text("project_time")?,
            requirement: shown_text("scientist_info")?,
            speed: plain(&speed_if_shown(r, image, frame)?),
            available,
            cost,
            select: gui_tree::rect_unclipped(r, image, button, GAME)?,
        });
    }
    Ok(out)
}
fn speed_if_shown(r: &Reader<'_>, image: u64, frame: u64) -> Result<String> {
    let Ok(window) = named(r, image, frame, "research_speed_window") else {
        return Ok(String::new());
    };
    match gui_tree::child(r, image, window, "research_speed_text")? {
        Some(t) if gui_tree::shown(r, t)? => r.string_bounded(t + TEXT, 64),
        _ => Ok(String::new()),
    }
}
/// Visible band of the project rows in game pixels (the dialog's paper).
const PROJECT_BAND: (i32, i32) = (275, 735);
/// Screen text of a facility window that shows a running project.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Active {
    pub name: String,
    pub progress: String,
    pub time: String,
    pub phase: String,
    pub stopping: String,
}
fn active_info(r: &Reader<'_>, image: u64) -> Result<Option<Active>> {
    let Some(root) = program_view_root(r, image)? else {
        return Ok(None);
    };
    let projects = named(r, image, root, "projects")?;
    let active = named(r, image, projects, "active_research")?;
    if !gui_tree::visible(r, active)? {
        return Ok(None);
    }
    let info = named(r, image, active, "project_progress_info")?;
    let ongoing = named(r, image, active, "ongoing_project_info_window")?;
    let phase = named(r, image, ongoing, "phase_progress_window")?;
    let shown_text = |parent: u64, name: &str| -> Result<String> {
        match gui_tree::child(r, image, parent, name)? {
            Some(t) if gui_tree::shown(r, t)? => Ok(plain(&r.string_bounded(t + TEXT, 256)?)),
            _ => Ok(String::new()),
        }
    };
    Ok(Some(Active {
        name: shown_text(active, "project_name")?,
        progress: shown_text(info, "project_progress")?,
        time: shown_text(info, "approx_research_time")?,
        phase: shown_text(phase, "phase_name_text")?,
        stopping: shown_text(phase, "stop_time_text")?,
    }))
}
fn start_button(r: &Reader<'_>, image: u64) -> Result<Option<(Rect, bool)>> {
    let Some(root) = program_view_root(r, image)? else {
        return Ok(None);
    };
    let projects = named(r, image, root, "projects")?;
    let active = named(r, image, projects, "active_research")?;
    let flavor = named(r, image, active, "flavor_window")?;
    let start = named(r, image, flavor, "start_project_window")?;
    if !gui_tree::visible(r, start)? {
        return Ok(None);
    }
    let button = gui_tree::child(r, image, start, "start_research_button")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Start button not found"))?;
    Ok(Some((
        gui_tree::rect(r, image, button, GAME)?,
        gui_tree::disabled(r, button)?,
    )))
}

/// (scientist character id, project template id) of one facility, for settlement.
pub(super) fn slots(process: &dyn Process, facility: i32) -> Result<Option<Slots>> {
    Ok(snapshot(process)?.facility(facility).map(|f| Slots {
        scientist: f.scientist,
        project: f.project,
    }))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Slots {
    pub scientist: Option<u32>,
    pub project: Option<u32>,
}
impl Slots {
    pub(super) const EMPTY: Slots = Slots {
        scientist: None,
        project: None,
    };
    pub(super) fn of(snap: &Snapshot, facility: i32) -> Option<Slots> {
        snap.facility(facility).map(|f| Slots {
            scientist: f.scientist,
            project: f.project,
        })
    }
}
/// Character id and project template id the arguments name, for settlement.
pub(super) fn wanted(
    snap: &Snapshot,
    scientist: Option<&str>,
    project: Option<&str>,
) -> (Option<u32>, Option<u32>) {
    (
        scientist
            .and_then(|n| snap.scientist_named(n))
            .map(|s| s.character),
        project.and_then(|k| snap.project_by_key(k)).map(|p| p.id),
    )
}
pub(super) fn facility_arg(v: Option<&Value>) -> Result<Option<i32>> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_i64()
            .filter(|n| (1..=1_000_000).contains(n))
            .map(|n| Some(n as i32))
            .ok_or_else(|| Error::new("invalid_arguments", "facility_id must be a positive integer")),
    }
}
pub(super) fn name_arg<'a>(v: Option<&'a Value>, what: &str) -> Result<Option<&'a str>> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s))
            if !s.is_empty() && s.len() <= 128 && !s.chars().any(char::is_control) =>
        {
            Ok(Some(s))
        }
        _ => Err(Error::new(
            "invalid_arguments",
            format!("Invalid {what} name"),
        )),
    }
}
pub(super) fn key_arg(v: Option<&Value>) -> Result<Option<&str>> {
    match name_arg(v, "project")? {
        Some(k) if !k.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') => Err(
            Error::new("invalid_arguments", "Invalid project key"),
        ),
        other => Ok(other),
    }
}
pub(super) fn panel_tool() -> Value {
    json!({"name":"hoi4_special_projects_panel","description":"Open the Research panel's Experimental Facilities tab with ordinary input (W, the tab), read what only the screen shows and close the panel again: each facility card's state name, research speed, scientist and project text; with facility_id also that facility's project window: every project the game offers there now with display name, type, approximate time, requirement text ('Requires a Scientist to start') and speed. Starts nothing. Needs the lease; moves the cursor.","inputSchema":{"type":"object","properties":{"facility_id":{"type":"integer","minimum":1},"rules":{"type":"boolean"},"lease_token":{"type":"string"}},"required":["lease_token"],"additionalProperties":false}})
}
pub(super) fn assign_tool() -> Value {
    json!({"name":"hoi4_special_project_assign","description":"Put a scientist and/or a project into one own research facility with ordinary input: Research panel (W), Experimental Facilities tab, the facility card's scientist portrait and the roster row, then the facility icon, the project entry and Start. Give scientist (name as listed by hoi4_special_projects) and/or project (key, e.g. sp_air_radar); a project needs a scientist already in the facility or passed together. Verified from memory when the facility's own scientist and project slots hold them. Starting a project spends breakthrough points of its specialization. Refused before any input: facility_unknown, unknown_scientist (closest names), scientist_wrong_specialization, scientist_busy, unknown_project, project_wrong_specialization, project_busy, facility_has_project, scientist_required, already_assigned. Refused at the screen without starting: scientist_not_offered, project_not_offered, project_not_startable. Stable request_id; an unresolved dispatch is settled only by retrying its own request_id.","inputSchema":{"type":"object","properties":{"facility_id":{"type":"integer","minimum":1},"scientist":{"type":"string","maxLength":128},"project":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["facility_id","request_id","lease_token"],"additionalProperties":false}})
}
fn handle_of(snap: &Snapshot, facility: i32) -> Result<u64> {
    snap.facility(facility)
        .map(|f| f.handle)
        .ok_or_else(|| unknown_facility(snap, facility))
}

/// Read the panel: facility cards (state names, speeds), and with `facility`
/// that facility's project list. Opens the Research panel; closes it again.
pub(super) fn panel(
    process: &dyn Process,
    keys: &MacKeys,
    facility: Option<i32>,
    with_rules: bool,
) -> Result<Value> {
    let snap = snapshot(process)?;
    let handle = match facility {
        Some(id) => Some(handle_of(&snap, id)?),
        None => None,
    };
    clear_notices(process, keys)?;
    type Panel = (
        Vec<Card>,
        Option<Vec<Entry>>,
        Option<Active>,
        Option<Vec<String>>,
    );
    let result: Result<Panel> = (|| {
        open_tab(process, keys)?;
        let cards = settled(process, "facility cards", cards)?;
        let mut entries = None;
        let mut active = None;
        let mut roster = None;
        if handle.is_none() {
            // The Scientists window lists whom the game offers for assignment.
            let rect = settled(process, "scientists button", open_roster_rect)?;
            let (x, y) = rect.center();
            keys.click(GAME, x, y)?;
            wait(process, "open scientist roster", roster_open)?;
            thread::sleep(Duration::from_millis(250));
            roster = Some(settled(process, "roster rows", roster_names)?);
        }
        if let Some(handle) = handle {
            let card = cards
                .iter()
                .find(|c| c.handle == handle)
                .ok_or_else(|| Error::new("gui_target_unavailable", "Facility card not in the list"))?;
            let (x, y) = card.open.center();
            keys.click(GAME, x, y)?;
            wait(process, "open facility window", program_view_open)?;
            thread::sleep(Duration::from_millis(250));
            entries = Some(settled(process, "project list", |r, i| {
                // a running facility shows its project instead of a list
                Ok(project_entries(r, i).unwrap_or_default())
            })?);
            active = settled(process, "running project", active_info)?;
        }
        Ok((cards, entries, active, roster))
    })();
    leave(process, keys);
    let (cards, entries, active, roster) = result?;
    let facilities: Vec<Value> = snap
        .facilities
        .iter()
        .map(|f| {
            let mut v = facility_json(&snap, f);
            if let Some(c) = cards.iter().find(|c| c.handle == f.handle) {
                v["state"] = json!(plain(&c.state));
                v["card"] = json!({
                    "name": plain(&c.facility),
                    "research_speed": plain(&c.speed),
                    "scientist": plain(&c.scientist),
                    "project": plain(&c.project),
                    "status": plain(&c.status),
                });
            }
            v
        })
        .collect();
    let mut out = json!({"facilities": facilities, "source": "Research panel, Experimental Facilities tab (screen text); the panel is closed again"});
    if let Some(roster) = roster {
        let missing: Vec<&str> = snap
            .scientists
            .iter()
            .filter(|s| !roster.iter().any(|n| n.eq_ignore_ascii_case(&s.name)))
            .map(|s| s.name.as_str())
            .collect();
        out["scientist_roster"] = json!({
            "listed": roster,
            "in_memory_but_not_listed": missing,
            "note": "Scientists the game's roster window does not list cannot be assigned through it.",
        });
    }
    if let Some(entries) = entries {
        out["facility_id"] = json!(facility);
        out["projects"] = json!(entries
            .iter()
            .map(|e| json!({
                "key": e.key,
                "name": e.name,
                "type": e.kind,
                "available_now": e.available,
                "approximate_time": e.time,
                "requirement": e.requirement,
                "speed": e.speed,
                "breakthrough_cost": e.cost,
            }))
            .collect::<Vec<_>>());
        out["projects_note"] = json!("Entries the game offers for this facility right now. An empty list with a running project means the facility window shows its project instead.");
    }
    if let Some(a) = active {
        out["running_project"] = json!({
            "name": a.name,
            "progress": a.progress,
            "approximate_time": a.time,
            "phase": a.phase,
            "stopping": a.stopping,
        });
    }
    if with_rules {
        attach_rules(&mut out, &game_rules(process));
    }
    Ok(out)
}
fn unknown_facility(snap: &Snapshot, id: i32) -> Error {
    let valid: Vec<String> = snap
        .facilities
        .iter()
        .map(|f| format!("{} ({})", f.id, f.building))
        .collect();
    Error::new(
        "facility_unknown",
        format!(
            "No own facility with id {id}. Own facilities: {}. Build one with hoi4_construction_add (land_facility, air_facility, naval_facility, nuclear_facility).",
            if valid.is_empty() { "none".to_string() } else { valid.join(", ") }
        ),
    )
}

/// Ordinary input sequence for one assign. The caller has journaled intent.
pub(super) fn assign(process: &dyn Process, keys: &MacKeys, plan: &Plan) -> Result<()> {
    let snap = snapshot(process)?;
    let handle = handle_of(&snap, plan.facility)?;
    clear_notices(process, keys)?;
    let result: Result<()> = (|| {
        open_tab(process, keys)?;
        if plan.assign_scientist {
            let scientist = plan.scientist.as_ref().expect("scientist planned");
            let card = settled(process, "facility card", |r, i| card_of(r, i, handle))?;
            let (x, y) = card.portrait.center();
            keys.click(GAME, x, y)?;
            wait(process, "open scientist roster", roster_open)?;
            thread::sleep(Duration::from_millis(250));
            pick_scientist(process, keys, &scientist.name)?;
            wait(process, "scientist assigned", |r, i| {
                let snap = snapshot_from(r, i)?;
                Ok(snap
                    .facility(plan.facility)
                    .is_some_and(|f| f.scientist == Some(scientist.character)))
            })?;
            // The roster closes by itself on a pick.
            super::ui::dismiss(process, keys, |r, i| Ok(!roster_open(r, i)?));
        }
        if let Some(project) = &plan.project {
            let card = settled(process, "facility card", |r, i| card_of(r, i, handle))?;
            let (x, y) = card.open.center();
            keys.click(GAME, x, y)?;
            wait(process, "open facility window", program_view_open)?;
            thread::sleep(Duration::from_millis(250));
            let find_entry = |r: &Reader<'_>, i: u64| -> Result<Entry> {
                project_entries(r, i)?
                    .into_iter()
                    .find(|e| e.key == project.key)
                    .ok_or_else(|| {
                        Error::new(
                            "gui_target_unavailable",
                            "Project is not listed by the game for this facility right now",
                        )
                    })
            };
            let mut entry = settled(process, "project entry", find_entry).map_err(|e| {
                if e.code == "gui_target_unavailable" {
                    let offered = read(process, |r, i| {
                        Ok(project_entries(r, i)?
                            .into_iter()
                            .map(|e| e.key)
                            .collect::<Vec<_>>())
                    })
                    .unwrap_or_default();
                    Error::new(
                        "project_not_offered",
                        format!(
                            "The facility window does not list {}. Listed now: {}. Nothing was started.",
                            project.key,
                            if offered.is_empty() {
                                "none".to_string()
                            } else {
                                offered.join(", ")
                            }
                        ),
                    )
                } else {
                    e
                }
            })?;
            if !entry.available {
                return Err(Error::new(
                    "project_not_startable",
                    format!(
                        "The game shows {} greyed out (a prerequisite technology or project is missing); nothing was started. Entries available now: {}",
                        project.key,
                        read(process, |r, i| Ok(project_entries(r, i)?
                            .into_iter()
                            .filter(|e| e.available)
                            .map(|e| e.key)
                            .collect::<Vec<_>>()))
                        .unwrap_or_default()
                        .join(", ")
                    ),
                ));
            }
            for _ in 0..20 {
                if entry.select.y >= PROJECT_BAND.0
                    && entry.select.y + entry.select.h <= PROJECT_BAND.1
                {
                    break;
                }
                let pixels = if entry.select.y < PROJECT_BAND.0 { 240 } else { -240 };
                keys.scroll(GAME, (885.0, 500.0), pixels)?;
                thread::sleep(Duration::from_millis(150));
                entry = read(process, find_entry)?;
            }
            let entry = settled(process, "project entry", find_entry)?;
            let (x, y) = entry.select.center();
            keys.click(GAME, x, y)?;
            wait(process, "project details", |r, i| {
                Ok(start_button(r, i)?.is_some())
            })?;
            let (button, disabled) = settled(process, "start button", |r, i| {
                start_button(r, i)?
                    .ok_or_else(|| Error::new("input_readback_failed", "Project details closed"))
            })?;
            if disabled {
                return Err(Error::new(
                    "project_not_startable",
                    format!(
                        "The game disables Start for {} (not enough breakthrough points, missing resources or no free scientist); nothing was started",
                        project.key
                    ),
                ));
            }
            let (x, y) = button.center();
            keys.click(GAME, x, y)?;
            wait(process, "project started", |r, i| {
                let snap = snapshot_from(r, i)?;
                Ok(snap
                    .facility(plan.facility)
                    .is_some_and(|f| f.project == Some(project.id)))
            })?;
        }
        Ok(())
    })();
    leave(process, keys);
    result
}
/// Click the roster row of `name`, scrolling it into the list band.
fn pick_scientist(process: &dyn Process, keys: &MacKeys, name: &str) -> Result<()> {
    let wanted = name.to_lowercase();
    let find = |r: &Reader<'_>, i: u64| -> Result<Option<Rect>> {
        Ok(roster_rows(r, i)?
            .into_iter()
            .find(|(n, _)| n.to_lowercase() == wanted)
            .map(|(_, rect)| rect))
    };
    let Some(mut rect) = settled(process, "scientist row", find)? else {
        let listed = read(process, roster_names)?;
        return Err(Error::new(
            "scientist_not_offered",
            format!(
                "The roster does not offer {name} for this facility. It lists: {}. Nothing was assigned.",
                listed.join(", ")
            ),
        ));
    };
    let band = read(process, |r, i| Ok(roster_band(r, i)))?;
    for _ in 0..30 {
        if rect.y >= band.0 && rect.y + rect.h <= band.1 {
            break;
        }
        let pixels = if rect.y < band.0 { 240 } else { -240 };
        keys.scroll(GAME, (640.0, f64::from(band.0 + 120)), pixels)?;
        thread::sleep(Duration::from_millis(120));
        rect = read(process, find)?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Scientist left the list"))?;
    }
    let rect = settled(process, "scientist row", |r, i| {
        find(r, i)?.ok_or_else(|| Error::new("gui_target_unavailable", "Scientist left the list"))
    })?;
    if rect.y < band.0 || rect.y + rect.h > band.1 {
        return Err(Error::new(
            "gui_target_unavailable",
            "Scientist row could not be scrolled into the roster band",
        ));
    }
    let (x, y) = rect.center();
    keys.click(GAME, x, y)?;
    Ok(())
}

// ---- confirmation dialog -------------------------------------------------------
/// The game's generic confirmation (CGenericDefaultConfirmationPopUpWindow:
/// title, description, accept_button, decline_button), e.g. "Stop project".
const VT_CONFIRM: u64 = 0x338d640;
const GUI_FROM_IDLER: u64 = 0x4c0;
const POPUP_LIST: u64 = 0x9e0;
const POPUP_ROOT: u64 = 0x430;
/// One confirmation window (title, accept and decline rectangles) when it is
/// a visible generic confirmation.
fn confirm_in(
    r: &Reader<'_>,
    image: u64,
    window: u64,
) -> Result<Option<(String, Rect, Rect)>> {
    if window == 0 || r.u64(window)? != image + VT_CONFIRM {
        return Ok(None);
    }
    let Some(root) = gui_tree::container(r, r.u64(window + POPUP_ROOT)?, image)? else {
        return Ok(None);
    };
    if !gui_tree::visible(r, root)? {
        return Ok(None);
    }
    let title = text(r, image, root, "title")?.unwrap_or_default();
    let accept = gui_tree::child(r, image, root, "accept_button")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Accept button not found"))?;
    let decline = gui_tree::child(r, image, root, "decline_button")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Decline button not found"))?;
    Ok(Some((
        plain(&title),
        gui_tree::rect(r, image, accept, GAME)?,
        gui_tree::rect(r, image, decline, GAME)?,
    )))
}
/// The open confirmation: the game's popup list holds the Stop project one;
/// the recruit confirmation is only reachable through the Recruit Scientist
/// window (+0x980, seen live), which created it.
fn confirm(r: &Reader<'_>, image: u64) -> Result<Option<(String, Rect, Rect)>> {
    libcxx(r)?;
    let idler = r.u64(image + game::layout(r).idler_global)?;
    let gui = r.u64(idler + GUI_FROM_IDLER)?;
    let (_, data, count) = r.array_header(gui + POPUP_LIST)?;
    for index in 0..count.min(32) as u64 {
        if let Some(found) = confirm_in(r, image, r.u64(data + index * 8)?)? {
            return Ok(Some(found));
        }
    }
    let tab = tab_view(r, image)?;
    let roster = r.u64(tab + TAB_ROSTER)?;
    typed(r, image, roster, VT_ROSTER, "scientist roster")?;
    let window = r.u64(roster + ROSTER_RECRUIT)?;
    if window != 0 && r.u64(window)? == image + VT_RECRUIT_WINDOW {
        return confirm_in(r, image, r.u64(window + RECRUIT_CONFIRM)?);
    }
    Ok(None)
}
/// Accept the confirmation whose title contains `title` (case-insensitive).
fn accept_confirm(process: &dyn Process, keys: &MacKeys, title: &str) -> Result<()> {
    let wanted = title.to_lowercase();
    wait(process, "confirmation dialog", |r, i| {
        Ok(confirm(r, i)?.is_some())
    })?;
    let (found, accept, decline) = settled(process, "confirmation buttons", |r, i| {
        confirm(r, i)?.ok_or_else(|| Error::new("input_readback_failed", "Dialog closed"))
    })?;
    if !found.to_lowercase().contains(&wanted) {
        // Not ours: decline it and refuse.
        let (x, y) = decline.center();
        keys.click(GAME, x, y)?;
        return Err(Error::new(
            "unexpected_dialog",
            format!("A confirmation titled {found:?} appeared instead of {title:?}; it was declined"),
        ));
    }
    let (x, y) = accept.center();
    keys.click(GAME, x, y)?;
    wait(process, "confirmation closed", |r, i| {
        Ok(confirm(r, i)?.is_none())
    })
}

// ---- stop, unassign, recruit: plans -----------------------------------------------
/// (project template id, in stopping state) of one facility, for settlement.
pub(super) fn stop_view(
    process: &dyn Process,
    facility: i32,
) -> Result<Option<(Option<u32>, bool)>> {
    let snap = snapshot(process)?;
    Ok(snap.facility(facility).map(|f| {
        let stopping = f
            .project
            .and_then(|id| snap.project_by_id(id))
            .is_some_and(|p| p.stopping_days.is_some());
        (f.project, stopping)
    }))
}
/// (owned now, still offered now) of a recruitment candidate by name.
pub(super) fn recruit_view(process: &dyn Process, name: &str) -> Result<(bool, bool)> {
    let snap = snapshot(process)?;
    Ok((
        snap.scientist_named(name).is_some(),
        candidate_named(&snap, name).is_some(),
    ))
}
pub(super) fn plan_stop(snap: &Snapshot, facility: i32) -> Result<()> {
    let f = snap
        .facility(facility)
        .ok_or_else(|| unknown_facility(snap, facility))?;
    let Some(id) = f.project else {
        return Err(Error::new(
            "no_project",
            format!("Facility {facility} runs no project; nothing to stop"),
        ));
    };
    if let Some(p) = snap.project_by_id(id) {
        if let Some(days) = p.stopping_days {
            return Err(Error::new(
                "already_stopping",
                format!("{} is already stopping (a stop takes {days} days)", p.key),
            ));
        }
    }
    Ok(())
}
pub(super) fn plan_unassign(snap: &Snapshot, facility: i32) -> Result<()> {
    let f = snap
        .facility(facility)
        .ok_or_else(|| unknown_facility(snap, facility))?;
    if f.scientist.is_none() {
        return Err(Error::new(
            "no_scientist",
            format!("Facility {facility} has no scientist"),
        ));
    }
    if f.project.is_some() {
        return Err(Error::new(
            "facility_has_project",
            "A project is running (or stopping); stop it with hoi4_special_project_stop and wait until the facility shows no project",
        ));
    }
    Ok(())
}
/// Pre-check for a recruit: the candidate must be in the pool and the own
/// political power must cover the cost. `power` is None when unreadable.
pub(super) fn plan_recruit<'a>(
    snap: &'a Snapshot,
    name: &str,
    power: Option<f64>,
) -> Result<&'a Scientist> {
    if let Some(own) = snap.scientist_named(name) {
        return Err(Error::new(
            "already_recruited",
            format!("{} is already an own scientist", own.name),
        ));
    }
    let wanted = name.to_lowercase();
    let Some(c) = snap
        .candidates
        .iter()
        .find(|c| c.name.to_lowercase() == wanted)
    else {
        let listed: Vec<String> = snap.candidates.iter().map(|c| c.name.clone()).collect();
        return Err(Error::new(
            "unknown_candidate",
            format!(
                "The Recruit Scientist window does not offer {name:?}. Offered now: {}",
                if listed.is_empty() {
                    "nobody".to_string()
                } else {
                    listed.join(", ")
                }
            ),
        ));
    };
    let cost = recruit_cost(snap);
    if let Some(power) = power {
        if power < cost as f64 {
            return Err(Error::new(
                "insufficient_political_power",
                format!("Recruiting costs {cost} political power; you have {power}"),
            ));
        }
    }
    Ok(c)
}
pub(super) fn candidate_named(snap: &Snapshot, name: &str) -> Option<u32> {
    let wanted = name.to_lowercase();
    snap.candidates
        .iter()
        .find(|c| c.name.to_lowercase() == wanted)
        .map(|c| c.character)
}

// ---- stop, unassign, recruit: input -------------------------------------------------
fn stop_button(r: &Reader<'_>, image: u64) -> Result<Option<Rect>> {
    let Some(root) = program_view_root(r, image)? else {
        return Ok(None);
    };
    let projects = named(r, image, root, "projects")?;
    let active = named(r, image, projects, "active_research")?;
    if !gui_tree::visible(r, active)? {
        return Ok(None);
    }
    let info = named(r, image, active, "ongoing_project_info_window")?;
    let Some(button) = gui_tree::child(r, image, info, "stop_research")? else {
        return Ok(None);
    };
    if !gui_tree::shown(r, button)? {
        return Ok(None);
    }
    Ok(Some(gui_tree::rect(r, image, button, GAME)?))
}
fn open_card_window(process: &dyn Process, keys: &MacKeys, handle: u64) -> Result<()> {
    let card = settled(process, "facility card", |r, i| card_of(r, i, handle))?;
    let (x, y) = card.open.center();
    keys.click(GAME, x, y)?;
    wait(process, "open facility window", program_view_open)?;
    thread::sleep(Duration::from_millis(250));
    Ok(())
}
/// Stop the project of one facility: facility window, the stop button and
/// the game's confirmation. Verified when the project enters its stopping
/// state or the facility shows no project.
pub(super) fn stop(process: &dyn Process, keys: &MacKeys, facility: i32) -> Result<()> {
    let snap = snapshot(process)?;
    let handle = handle_of(&snap, facility)?;
    clear_notices(process, keys)?;
    let result: Result<()> = (|| {
        open_tab(process, keys)?;
        open_card_window(process, keys, handle)?;
        let rect = settled(process, "stop button", |r, i| {
            stop_button(r, i)?.ok_or_else(|| {
                Error::new(
                    "gui_target_unavailable",
                    "The facility window shows no running project with a stop button",
                )
            })
        })?;
        let (x, y) = rect.center();
        keys.click(GAME, x, y)?;
        accept_confirm(process, keys, "stop")?;
        wait(process, "project stopping", |r, i| {
            let snap = snapshot_from(r, i)?;
            Ok(match snap.facility(facility) {
                None => false,
                Some(f) => match f.project {
                    None => true,
                    Some(id) => snap
                        .project_by_id(id)
                        .is_some_and(|p| p.stopping_days.is_some()),
                },
            })
        })
    })();
    leave(process, keys);
    result
}
/// Take the scientist out of a facility (the small button on the portrait).
pub(super) fn unassign(process: &dyn Process, keys: &MacKeys, facility: i32) -> Result<()> {
    let snap = snapshot(process)?;
    let handle = handle_of(&snap, facility)?;
    clear_notices(process, keys)?;
    let result: Result<()> = (|| {
        open_tab(process, keys)?;
        let card = settled(process, "facility card", |r, i| card_of(r, i, handle))?;
        let rect = card.unassign.ok_or_else(|| {
            Error::new(
                "gui_target_unavailable",
                "The facility card shows no unassign button",
            )
        })?;
        let (x, y) = rect.center();
        keys.click(GAME, x, y)?;
        // The game may ask for confirmation; any confirmation is about this click.
        let deadline = std::time::Instant::now() + Duration::from_millis(1200);
        loop {
            if read(process, |r, i| Ok(confirm(r, i)?.is_some()))? {
                accept_confirm(process, keys, "")?;
                break;
            }
            if read(process, |r, i| {
                Ok(snapshot_from(r, i)?
                    .facility(facility)
                    .is_some_and(|f| f.scientist.is_none()))
            })? || std::time::Instant::now() >= deadline
            {
                break;
            }
            thread::sleep(Duration::from_millis(40));
        }
        wait(process, "scientist unassigned", |r, i| {
            Ok(snapshot_from(r, i)?
                .facility(facility)
                .is_some_and(|f| f.scientist.is_none()))
        })
    })();
    leave(process, keys);
    result
}
fn open_roster_rect(r: &Reader<'_>, image: u64) -> Result<Rect> {
    let root = root(r, image)?;
    let facilities = named(r, image, root, "facilities")?;
    let view = named(r, image, facilities, "facilities_view")?;
    let bottom = named(r, image, view, "facilities_overview_bottom_window")?;
    let button = gui_tree::child(r, image, bottom, "open_scientist_roster")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Scientists button not found"))?;
    gui_tree::rect(r, image, button, GAME)
}
fn recruit_window_root(r: &Reader<'_>, image: u64) -> Result<Option<u64>> {
    let tab = tab_view(r, image)?;
    let roster = r.u64(tab + TAB_ROSTER)?;
    typed(r, image, roster, VT_ROSTER, "scientist roster")?;
    let window = r.u64(roster + ROSTER_RECRUIT)?;
    if window == 0 || r.u64(window)? != image + VT_RECRUIT_WINDOW {
        return Ok(None);
    }
    gui_tree::container(r, r.u64(window + ROSTER_ROOT)?, image)
}
fn recruit_open(r: &Reader<'_>, image: u64) -> Result<bool> {
    match recruit_window_root(r, image)? {
        Some(root) => gui_tree::visible(r, root),
        None => Ok(false),
    }
}
fn recruit_button(r: &Reader<'_>, image: u64) -> Result<Rect> {
    let root = roster_root(r, image)?;
    let button = gui_tree::child(r, image, root, "recruit_scientist_button")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Recruit button not found"))?;
    gui_tree::rect(r, image, button, GAME)
}
/// (name, cost text, portrait rectangle) of each Recruit Scientist row.
fn recruit_rows(r: &Reader<'_>, image: u64) -> Result<Vec<(String, String, Rect)>> {
    let root = recruit_window_root(r, image)?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Recruit window not open"))?;
    let scientists = named(r, image, root, "scientists")?;
    let container = named(r, image, scientists, "scientist_list_container")?;
    let grid = gui_tree::child(r, image, container, "scientist_list")?
        .ok_or_else(|| Error::new("gui_target_unavailable", "Candidate list not found"))?;
    let mut out = Vec::new();
    for item in items(r, grid)? {
        if r.u64(item)? != image + VT_SCIENTIST_ITEM {
            continue;
        }
        let frame = item_frame(r, image, item)?;
        let card = gui_tree::child(r, image, frame, "shadow_for_scientist_card")?
            .ok_or_else(|| Error::new("gui_target_unavailable", "Candidate card not found"))?;
        out.push((
            text(r, image, frame, "name")?.unwrap_or_default(),
            plain(&text(r, image, frame, "recruit_cost")?.unwrap_or_default()),
            gui_tree::rect_unclipped(r, image, card, GAME)?,
        ));
    }
    Ok(out)
}
/// Recruit one candidate of the Recruit Scientist window by name.
pub(super) fn recruit(process: &dyn Process, keys: &MacKeys, name: &str) -> Result<()> {
    let wanted = name.to_lowercase();
    clear_notices(process, keys)?;
    let result: Result<()> = (|| {
        open_tab(process, keys)?;
        if !read(process, roster_open)? {
            let rect = settled(process, "scientists button", open_roster_rect)?;
            let (x, y) = rect.center();
            keys.click(GAME, x, y)?;
            wait(process, "open scientist roster", roster_open)?;
            thread::sleep(Duration::from_millis(250));
        }
        let rect = settled(process, "recruit button", recruit_button)?;
        let (x, y) = rect.center();
        keys.click(GAME, x, y)?;
        wait(process, "open recruit window", recruit_open)?;
        thread::sleep(Duration::from_millis(250));
        let row = settled(process, "candidate row", |r, i| {
            Ok(recruit_rows(r, i)?
                .into_iter()
                .find(|(n, _, _)| n.to_lowercase() == wanted))
        })?;
        let Some((_, _, rect)) = row else {
            let listed: Vec<String> = read(process, |r, i| {
                Ok(recruit_rows(r, i)?.into_iter().map(|(n, _, _)| n).collect())
            })?;
            return Err(Error::new(
                "unknown_candidate",
                format!(
                    "The Recruit Scientist window lists {}; nobody was recruited",
                    listed.join(", ")
                ),
            ));
        };
        let (x, y) = rect.center();
        keys.click(GAME, x, y)?;
        // "Spend <cost> to recruit <name>": the game asks before taking the power.
        accept_confirm(process, keys, "recruit")?;
        wait(process, "scientist recruited", |r, i| {
            Ok(snapshot_from(r, i)?
                .scientists
                .iter()
                .any(|s| s.name.to_lowercase() == wanted))
        })
        .map_err(|e| {
            Error::new(
                e.code,
                format!(
                    "{}. The click on the candidate was sent; the scientist did not join within 3 s (not enough political power, or the game refused). Re-read hoi4_special_projects before trying again.",
                    e.message
                ),
            )
        })
    })();
    leave(process, keys);
    result
}
pub(super) fn stop_tool() -> Value {
    json!({"name":"hoi4_special_project_stop","description":"Stop the running project of one own facility: facility window, the stop button and the game's 'Stop project' confirmation (accepted). Stopping takes about 10 game days (the scientist stays); canceling before an iteration completes refunds the breakthrough points once stopped. Verified when the project enters its stopping state (hoi4_special_projects shows stopping_days) or the facility shows no project. Refused before any input: facility_unknown, no_project, already_stopping.","inputSchema":{"type":"object","properties":{"facility_id":{"type":"integer","minimum":1},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["facility_id","request_id","lease_token"],"additionalProperties":false}})
}
pub(super) fn unassign_tool() -> Value {
    json!({"name":"hoi4_special_project_unassign","description":"Take the scientist out of an own facility (the small button on the portrait of the facility card). Refused before any input: facility_unknown, no_scientist, facility_has_project (stop the project first with hoi4_special_project_stop and wait for it to finish stopping). Verified when the facility's scientist slot is empty.","inputSchema":{"type":"object","properties":{"facility_id":{"type":"integer","minimum":1},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["facility_id","request_id","lease_token"],"additionalProperties":false}})
}
pub(super) fn recruit_tool() -> Value {
    json!({"name":"hoi4_special_project_recruit","description":"Recruit one scientist the Recruit Scientist window offers (names under recruitable.candidates of hoi4_special_projects) for political power (25/50/75/100 by how many scientists are idle). Scientists button, Recruit Scientist, the candidate's row. Verified when the character appears in the own scientist list. Refused before any input: unknown_candidate (lists who is offered), already_recruited, insufficient_political_power.","inputSchema":{"type":"object","properties":{"scientist":{"type":"string","maxLength":128},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["scientist","request_id","lease_token"],"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sci(id: u32, name: &str, skills: [i32; 4]) -> Scientist {
        Scientist {
            character: id,
            name: name.into(),
            key: format!("GER_{}", name.to_lowercase().replace(' ', "_")),
            skills,
            traits: vec![],
        }
    }
    fn project(id: u32, key: &str, spec: usize, running: u64) -> Project {
        Project {
            id,
            key: key.into(),
            spec: Some(spec),
            days: 60,
            running,
            stopping_days: None,
            progress_percent: 0,
            iterations: 0,
            gain: (15, 25),
        }
    }
    fn fac(id: i32, building: &str, scientist: Option<u32>, project: Option<u32>) -> Facility {
        Facility {
            id,
            handle: 0x100000053 + (id as u64) * 0x100000000,
            building: building.into(),
            spec: spec_of_building(building),
            scientist,
            project,
            unread_reports: 0,
        }
    }
    fn snap() -> Snapshot {
        Snapshot {
            facilities: vec![
                fac(10, "land_facility", None, None),
                fac(11, "air_facility", Some(3), Some(300)),
                fac(12, "land_facility", Some(2), None),
            ],
            scientists: vec![
                sci(1, "Eduard Grote", [0, 0, 0, 2]),
                sci(2, "Max Land", [0, 0, 0, 1]),
                sci(3, "Kurt Tank", [0, 0, 2, 0]),
                sci(4, "Wernher von Braun", [0, 0, 3, 0]),
            ],
            projects: vec![
                project(100, "sp_land_flamethrower_tank", 3, 0),
                project(101, "sp_land_military_engineering_vehicles", 3, 0),
                project(300, "sp_air_radar", 2, 0x100000053),
                project(301, "sp_air_jet_engine", 2, 0),
            ],
            candidates: vec![
                sci(5, "Hedwig Orth", [1, 0, 0, 0]),
                sci(6, "Helene Reuter", [0, 0, 0, 1]),
            ],
            breakthroughs: vec![
                Breakthrough { spec: 2, raw: 11624 },
                Breakthrough { spec: 3, raw: 3540 },
            ],
        }
    }
    #[test]
    fn stop_unassign_and_recruit_are_refused_before_input() {
        let mut s = snap();
        let code = |r: Result<()>| r.unwrap_err().code;
        assert!(plan_stop(&s, 11).is_ok());
        assert_eq!(code(plan_stop(&s, 10)), "no_project");
        assert_eq!(code(plan_stop(&s, 99)), "facility_unknown");
        s.projects[2].stopping_days = Some(10);
        assert_eq!(code(plan_stop(&s, 11)), "already_stopping");
        assert_eq!(code(plan_unassign(&s, 10)), "no_scientist");
        assert_eq!(code(plan_unassign(&s, 11)), "facility_has_project");
        assert!(plan_unassign(&s, 12).is_ok());
        let c = |r: Result<&Scientist>| r.map(|s| s.name.clone());
        assert_eq!(c(plan_recruit(&s, "hedwig orth", Some(318.0))).unwrap(), "Hedwig Orth");
        assert_eq!(
            c(plan_recruit(&s, "Hedwig Orth", Some(70.0))).unwrap_err().code,
            "insufficient_political_power"
        );
        // power unreadable: no refusal on it
        assert!(plan_recruit(&s, "Hedwig Orth", None).is_ok());
        assert_eq!(c(plan_recruit(&s, "Eduard Grote", None)).unwrap_err().code, "already_recruited");
        let e = plan_recruit(&s, "Nobody", None).unwrap_err();
        assert_eq!(e.code, "unknown_candidate");
        assert!(e.message.contains("Hedwig Orth") && e.message.contains("Helene Reuter"));
    }
    #[test]
    fn recruit_cost_follows_the_idle_scientists() {
        let mut s = snap();
        // idle: Max Land and Eduard Grote (Kurt Tank works in 11, Max Land in 12... see facilities)
        assert_eq!(recruit_cost(&s), 75);
        s.scientists.push(sci(7, "Idle A", [1, 0, 0, 0]));
        s.scientists.push(sci(8, "Idle B", [1, 0, 0, 0]));
        assert_eq!(recruit_cost(&s), 100);
        s.scientists.clear();
        assert_eq!(recruit_cost(&s), 25);
    }
    #[test]
    fn candidates_are_listed_with_the_cost() {
        let v = to_json(&snap());
        assert_eq!(v["recruitable"]["cost_political_power"], 75);
        assert_eq!(v["recruitable"]["candidates"][1]["name"], "Helene Reuter");
        assert_eq!(v["recruitable"]["candidates"][0]["specialization"], "nuclear");
    }
    #[test]
    fn facility_buildings_map_to_specializations() {
        assert_eq!(spec_of_building("land_facility"), Some(3));
        assert_eq!(spec_of_building("nuclear_facility"), Some(0));
        assert_eq!(spec_of_building("air_base"), None);
    }
    #[test]
    fn breakthrough_raw_is_points_and_percent() {
        let s = snap();
        assert_eq!(s.points(2), 1);
        assert_eq!(s.points(3), 0);
        let v = to_json(&s);
        assert_eq!(v["specializations"][2]["breakthrough_points"], 1);
        assert_eq!(v["specializations"][3]["breakthrough_progress_percent"], 35.4);
        assert_eq!(v["specializations"][0]["breakthrough_points"], Value::Null);
    }
    #[test]
    fn read_joins_scientist_and_project_to_facility() {
        let v = to_json(&snap());
        assert_eq!(v["facilities"][1]["scientist"]["name"], "Kurt Tank");
        assert_eq!(v["facilities"][1]["project"]["key"], "sp_air_radar");
        assert_eq!(v["scientists"][2]["assigned_facility_id"], 11);
        assert_eq!(v["scientists"][0]["assigned_facility_id"], Value::Null);
        assert_eq!(v["scientists"][0]["specialization"], "land");
        assert_eq!(v["projects"][2]["running_in_facility_id"], 11);
    }
    #[test]
    fn assign_scientist_and_project_plan() {
        let s = snap();
        let p = plan(&s, 10, Some("eduard grote"), Some("sp_land_flamethrower_tank")).unwrap();
        assert!(p.assign_scientist);
        assert_eq!(p.project.unwrap().id, 100);
        // a facility that already has the scientist only needs the project
        let p = plan(&s, 12, Some("Max Land"), Some("sp_land_flamethrower_tank")).unwrap();
        assert!(!p.assign_scientist);
        let p = plan(&s, 12, None, Some("sp_land_flamethrower_tank")).unwrap();
        assert!(p.scientist.is_none());
    }
    #[test]
    fn refusals_happen_before_any_input() {
        let s = snap();
        let code = |r: Result<Plan>| r.unwrap_err().code;
        assert_eq!(code(plan(&s, 99, Some("Eduard Grote"), None)), "facility_unknown");
        assert_eq!(code(plan(&s, 10, None, None)), "invalid_arguments");
        assert_eq!(code(plan(&s, 10, Some("Grotte"), None)), "unknown_scientist");
        assert_eq!(code(plan(&s, 10, Some("Kurt Tank"), None)), "scientist_wrong_specialization");
        assert_eq!(code(plan(&s, 10, Some("Max Land"), None)), "scientist_busy");
        assert_eq!(code(plan(&s, 10, None, Some("sp_land_flamethrower_tank"))), "scientist_required");
        assert_eq!(code(plan(&s, 10, Some("Eduard Grote"), Some("sp_air_radar"))), "project_wrong_specialization");
        assert_eq!(code(plan(&s, 10, Some("Eduard Grote"), Some("sp_nope"))), "unknown_project");
        assert_eq!(code(plan(&s, 11, Some("Wernher von Braun"), Some("sp_air_jet_engine"))), "facility_has_project");
        assert_eq!(code(plan(&s, 11, None, Some("sp_air_radar"))), "already_assigned");
        assert_eq!(code(plan(&s, 12, Some("Max Land"), None)), "already_assigned");
        // another facility already runs the project
        let mut other = snap();
        other.facilities.push(fac(13, "air_facility", None, None));
        assert_eq!(code(plan(&other, 13, Some("Wernher von Braun"), Some("sp_air_radar"))), "project_busy");
    }
    #[test]
    fn unknown_scientist_lists_closest_names() {
        let e = plan(&snap(), 10, Some("Eduard Grotte"), None).unwrap_err();
        assert!(e.message.contains("Eduard Grote"));
    }
    #[test]
    fn game_text_loses_colour_and_icon_markers() {
        assert_eq!(plain("\u{11}GSelect\u{11}! which project"), "Select which project");
        assert_eq!(plain("Approximate time: \u{13}infinity_icon! day(s)"), "Approximate time: unknown day(s)");
        assert_eq!(plain("\u{11}RRequires a \u{11}! \u{11}YScientist\u{11}! \u{11}Rto start\u{11}!"), "Requires a Scientist to start");
    }
}
