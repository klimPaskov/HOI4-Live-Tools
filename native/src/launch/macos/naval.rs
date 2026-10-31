//! `hoi4_fleets` and `hoi4_convoys`: read-only views of the own navy from
//! macOS memory (089EA519). No input, no cursor, never pauses.
//!
//! Offsets mapped live on 26 October 2026 (Germany, April 1938) and checked
//! against `history/units/GER_1936_naval_mtg.txt`: fleet and task-force names,
//! ship names, hulls, locations (provinces 241, 6332, 6389 and their states
//! 56, 763, 58) and naval base levels (241 = 8, 6332 = 5) all matched.
//! Fields marked `validated:false` in the output have no independent check.
use super::fleetmem::{
    self, expect, fixed_at, is_object, pointers, require_libcxx, round3, Place,
};
use crate::{
    economy,
    error::{Error, Result},
    memory::Reader,
    platform::Process,
    production_lines,
};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;

const COUNTRY_FLEETS: u64 = 0x238;
const FLEET_VTABLE: u64 = 0x34fc2e8;
const FLEET_HOME_PORT: u64 = 0xd0;
const FLEET_NAME: u64 = 0xd8;
const FLEET_TASK_FORCES: u64 = 0xb0;
/// Fleet area of operation: Clausewitz array of `CStrategicRegion` at fleet +0x40. Matched the
/// fleet panel's "Active in Regions" and "Moving to assigned region <name>" on 26 October 2026
/// (one region added by a right click, a second added and removed again).
const FLEET_REGIONS: u64 = 0x40;
/// Fleet commander: a character handle {id i32, kind i32} at fleet +0xa8 (zero without admiral);
/// it equals +0x8/+0xc of the `CNavyLeader` (array at `CCountryCharacters` +0x88 of country
/// +0xd98; 7 admirals, matching the 7 rows of the game's list), whose name is at +0x38. The
/// handle appeared at +0xa8 when Alfred Saalwaechter was assigned and cleared on Unassign
/// (26 October 2026).
const FLEET_LEADER: u64 = 0xa8;
const COUNTRY_CHARACTERS: u64 = 0xd98;
const CHARACTERS_NAVY_LEADERS: u64 = 0x88;
const LEADER_HANDLE: u64 = 0x8;
const LEADER_NAME: u64 = 0x38;
const REGION_LIMIT: usize = 64;
const TASK_FORCE_VTABLE: u64 = 0x351f208;
const TF_NAME: u64 = 0x258;
const TF_PROVINCE: u64 = 0x1f0;
const TF_SHIPS: u64 = 0x340;
const TF_MISSION_CODE: u64 = 0x36c;
const SHIP_VTABLE: u64 = 0x3519a20;
const SHIP_ID: u64 = 0xc;
const SHIP_TYPE: u64 = 0x90;
const SHIP_VARIANTS: u64 = 0x60;
const SHIP_HP: u64 = 0x6c0;
const SHIP_MAX_ORG: u64 = 0x6c8;
const SHIP_XP: u64 = 0x6d0;
const SHIP_MORALE: u64 = 0x6d8;
const SHIP_XP_FACTOR: u64 = 0x6e8;
const SHIP_NAME: u64 = 0x7d0;
/// Own convoy count: the embedded CConvoys at country+0xf88, field +0x88
/// (194 in memory and on the top bar on 10 April 1938).
const COUNTRY_CONVOYS: u64 = 0xf88 + 0x88;
const STRATEGIC_NAVY_BASES: u64 = 0x18;
const NAVAL_BASE_VTABLE: u64 = 0x351cf18;
const NAVAL_BASE_PROVINCE: u64 = 0x10;
const NAVAL_BASE_LEVEL: u64 = 0x14;
const FLEET_LIMIT: usize = 64;
const TF_LIMIT: usize = 64;
const SHIP_LIMIT: usize = 512;

/// Mission names of the task force mission code (task force +0x36c). 0, 1, 2, 3, 4, 7 and 9 were
/// set by the game's own mission buttons on 26 October 2026 and read back (hold, patrol, strike
/// force, convoy raiding, convoy escort, training, naval invasion support); 8 is the code of the
/// reserve fleet's task force; 5 and 6 (mine laying and sweeping) follow the button numbering
/// but were disabled by the build state and never set.
pub(super) fn mission_name(code: i32) -> Option<&'static str> {
    match code {
        0 => Some("hold"),
        1 => Some("patrol"),
        2 => Some("strike_force"),
        3 => Some("convoy_raiding"),
        4 => Some("convoy_escort"),
        5 => Some("mine_laying"),
        6 => Some("mine_sweeping"),
        7 => Some("training"),
        8 => Some("reserve"),
        9 => Some("invasion_support"),
        _ => None,
    }
}
fn mission_validated(code: i32) -> bool {
    matches!(code, 0 | 1 | 2 | 3 | 4 | 7 | 8 | 9)
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Ship {
    pub id: i32,
    pub name: String,
    pub unit_type: String,
    pub hull: String,
    pub variant: String,
    pub strength: f64,
    pub max_strength: f64,
    pub max_organisation: f64,
    pub experience_points: f64,
    pub experience_factor: f64,
    pub morale: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct TaskForce {
    pub name: String,
    pub place: Option<Place>,
    pub mission_code: i32,
    pub ships: Vec<Ship>,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Fleet {
    pub name: String,
    pub home_port: Option<Place>,
    /// Assigned sea regions (key, display name).
    pub regions: Vec<(String, String)>,
    /// Assigned admiral (None: no commander).
    pub admiral: Option<String>,
    pub task_forces: Vec<TaskForce>,
}

fn ship(r: &Reader<'_>, image: u64, address: u64) -> Result<Ship> {
    expect(r, address, SHIP_VTABLE, image, "Ship")?;
    let variants = pointers(r, address + SHIP_VARIANTS, 8)?;
    let (variant, hull, max_strength) = match variants.first() {
        Some(v) if *v != 0 => {
            let v = fleetmem::variant(r, image, *v)?;
            (v.name, v.type_key, v.max_strength)
        }
        _ => (String::new(), String::new(), 0.0),
    };
    Ok(Ship {
        id: r.i32(address + SHIP_ID)?,
        name: r.string_bounded(address + SHIP_NAME, 128)?,
        unit_type: r.string_bounded(address + SHIP_TYPE, 64)?,
        hull,
        variant,
        strength: fixed_at(r, address + SHIP_HP)?,
        max_strength,
        max_organisation: fixed_at(r, address + SHIP_MAX_ORG)?,
        experience_points: fixed_at(r, address + SHIP_XP)?,
        experience_factor: fixed_at(r, address + SHIP_XP_FACTOR)?,
        morale: fixed_at(r, address + SHIP_MORALE)?,
    })
}
fn task_force(r: &Reader<'_>, image: u64, address: u64) -> Result<TaskForce> {
    expect(r, address, TASK_FORCE_VTABLE, image, "Task force")?;
    let mut ships = Vec::new();
    for s in pointers(r, address + TF_SHIPS, SHIP_LIMIT)? {
        if s != 0 {
            ships.push(ship(r, image, s)?);
        }
    }
    Ok(TaskForce {
        name: r.string_bounded(address + TF_NAME, 128)?,
        place: fleetmem::place(r, r.u64(address + TF_PROVINCE)?)?,
        mission_code: r.i32(address + TF_MISSION_CODE)?,
        ships,
    })
}
/// Name of the navy leader whose handle equals `handle` (`unknown` when none matches).
fn admiral_name(r: &Reader<'_>, country: u64, handle: &[u8]) -> Result<String> {
    let characters = r.u64(country + COUNTRY_CHARACTERS)?;
    if characters == 0 {
        return Ok("unknown".to_string());
    }
    for leader in pointers(r, characters + CHARACTERS_NAVY_LEADERS, 64)? {
        if leader != 0 && r.read(leader + LEADER_HANDLE, 8)? == handle {
            return r.string_bounded(leader + LEADER_NAME, 128);
        }
    }
    Ok("unknown".to_string())
}
/// Own fleets of `country` with their task forces and ships.
pub(super) fn fleets(r: &Reader<'_>, image: u64, country: u64) -> Result<Vec<Fleet>> {
    let mut out = Vec::new();
    for f in pointers(r, country + COUNTRY_FLEETS, FLEET_LIMIT)? {
        expect(r, f, FLEET_VTABLE, image, "Fleet")?;
        let mut task_forces = Vec::new();
        for tf in pointers(r, f + FLEET_TASK_FORCES, TF_LIMIT)? {
            task_forces.push(task_force(r, image, tf)?);
        }
        let mut regions = Vec::new();
        for region in pointers(r, f + FLEET_REGIONS, REGION_LIMIT)? {
            if region == 0 {
                continue;
            }
            let template = r.u64(region + fleetmem::REGION_TEMPLATE)?;
            if template == 0 {
                continue;
            }
            regions.push((
                r.string_bounded(template + fleetmem::TEMPLATE_KEY, 64)?,
                r.string_bounded(template + fleetmem::TEMPLATE_NAME, 128)?,
            ));
        }
        let handle = r.read(f + FLEET_LEADER, 8)?;
        let admiral = if handle.iter().all(|b| *b == 0) {
            None
        } else {
            Some(admiral_name(r, country, &handle)?)
        };
        out.push(Fleet {
            name: r.string_bounded(f + FLEET_NAME, 128)?,
            admiral,
            home_port: fleetmem::place(r, r.u64(f + FLEET_HOME_PORT)?)?,
            regions,
            task_forces,
        });
    }
    Ok(out)
}

fn percent(part: f64, whole: f64) -> Option<f64> {
    (whole > 0.0).then(|| round3(100.0 * part / whole))
}
fn ship_json(s: &Ship) -> Value {
    json!({"id":s.id,"name":s.name,"type":s.unit_type,"hull":s.hull,"variant":s.variant,
        "strength":s.strength,"max_strength":s.max_strength,
        "strength_percent":percent(s.strength,s.max_strength),
        "max_organisation":s.max_organisation,"experience_factor":s.experience_factor,
        "experience_points":s.experience_points,"morale":s.morale})
}
fn task_force_json(t: &TaskForce, with_ships: bool) -> Value {
    let mut by_type: BTreeMap<&str, u32> = BTreeMap::new();
    let (mut hp, mut max_hp) = (0.0, 0.0);
    for s in &t.ships {
        *by_type.entry(s.unit_type.as_str()).or_default() += 1;
        hp += s.strength;
        max_hp += s.max_strength;
    }
    let mut v = json!({"task_force":t.name,"location":fleetmem::place_json(&t.place),
        "ship_count":t.ships.len(),"ships_by_type":by_type,
        "strength":round3(hp),"max_strength":round3(max_hp),"strength_percent":percent(hp,max_hp),
        "mission":{"code":t.mission_code,"name":mission_name(t.mission_code),"validated":mission_validated(t.mission_code)},
        "admiral":null,"admiral_note":"the admiral leads the fleet: see the fleet's admiral field"});
    if with_ships {
        v["ships"] = Value::Array(t.ships.iter().map(ship_json).collect());
    }
    v
}
fn fleets_json(fleets: &[Fleet], with_ships: bool, filter: Option<&str>) -> Value {
    let wanted = filter.map(str::to_lowercase);
    let mut ships = 0;
    let mut by_type: BTreeMap<String, u32> = BTreeMap::new();
    let mut rows = Vec::new();
    for f in fleets {
        if let Some(w) = &wanted {
            let hit = f.name.to_lowercase().contains(w)
                || f.task_forces
                    .iter()
                    .any(|t| t.name.to_lowercase().contains(w));
            if !hit {
                continue;
            }
        }
        for t in &f.task_forces {
            ships += t.ships.len();
            for s in &t.ships {
                *by_type.entry(s.unit_type.clone()).or_default() += 1;
            }
        }
        rows.push(json!({"fleet":f.name,"home_port":fleetmem::place_json(&f.home_port),
            "admiral":f.admiral,"assigned_regions":f.regions.iter().map(|(k, n)| json!({"key":k,"name":n})).collect::<Vec<_>>(),
            "task_forces":f.task_forces.iter().map(|t| task_force_json(t, with_ships)).collect::<Vec<_>>()}));
    }
    json!({"fleets":rows,"totals":{"ships":ships,"ships_by_type":by_type},
        "validated":{"fleet_task_force_ship_names":true,"hulls_and_variants":true,"port_locations":true,
            "strength":"matches hull max_strength at full health; damaged values unverified",
            "mission":"codes 0 hold, 1 patrol, 2 strike_force, 3 convoy_raiding, 4 convoy_escort, 7 training, 8 reserve, 9 invasion_support were set with the game's buttons and read back; 5/6 (mines) inferred","assigned_regions":"matches the fleet panel's Active in Regions and the region names","morale":false,"experience_points":false},
        "unavailable":{"admiral_detail":"skills and traits come from hoi4_fleet_admiral list:true","fuel":"not mapped","supply":"not mapped",
            "organisation_current":"not mapped (max_organisation matches the unit definition)"}})
}

/// `hoi4_fleets`: own fleets, task forces and (optionally) every ship.
pub(super) fn fleets_tool(process: &dyn Process, args: &Map<String, Value>) -> Result<Value> {
    let with_ships = args.get("ships").and_then(Value::as_bool).unwrap_or(false);
    let filter = args.get("fleet").and_then(Value::as_str);
    if filter.is_some_and(|f| f.len() > 64 || f.chars().any(char::is_control)) {
        return Err(Error::new("invalid_arguments", "Invalid fleet filter"));
    }
    let found = super::ui::read(process, |r, image| {
        require_libcxx(r)?;
        let (country, _) = economy::own_production(r, image)?;
        fleets(r, image, country)
    })?;
    Ok(fleets_json(&found, with_ships, filter))
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct NavalBase {
    pub province: Option<Place>,
    pub level: i32,
}
/// Own naval bases from the strategic navy of the own country index.
fn naval_bases(r: &Reader<'_>, image: u64) -> Result<Vec<NavalBase>> {
    let navy = fleetmem::own_strategic(r, image, fleetmem::GS_NAVY_MANAGER, fleetmem::NAVY_BY_COUNTRY)?;
    let mut out = Vec::new();
    for base in pointers(r, navy + STRATEGIC_NAVY_BASES, 64)? {
        if !is_object(r, base, NAVAL_BASE_VTABLE, image)? {
            continue;
        }
        let province = r.i32(base + NAVAL_BASE_PROVINCE)?;
        out.push((province, r.i32(base + NAVAL_BASE_LEVEL)?));
    }
    Ok(out
        .into_iter()
        .map(|(province, level)| NavalBase {
            province: Some(Place {
                province,
                state: None,
                region: None,
            }),
            level,
        })
        .collect())
}
/// `hoi4_convoys`: convoy count, naval bases, convoy production lines.
pub(super) fn convoys_tool(process: &dyn Process, args: &Map<String, Value>) -> Result<Value> {
    if !args.is_empty() {
        return Err(Error::new("invalid_arguments", "Unexpected argument"));
    }
    let (total, bases, lines) = super::ui::read(process, |r, image| {
        require_libcxx(r)?;
        let (country, _) = economy::own_production(r, image)?;
        let total = r.i32(country + COUNTRY_CONVOYS)?;
        let bases = naval_bases(r, image)?;
        let lines = production_lines::own_lines(r, image)?;
        Ok((total, bases, lines))
    })?;
    let convoy_lines: Vec<Value> = lines
        .iter()
        .filter(|l| l.key.as_deref().is_some_and(|k| k.starts_with("convoy")))
        .map(|l| json!({"line_handle":l.handle,"equipment":l.key,"requested_factories":l.requested}))
        .collect();
    let mut v = convoys_json(total, &bases, convoy_lines);
    v["naval_lines_other"] = json!(lines.iter().filter(|l| l.naval && !l.key.as_deref().is_some_and(|k| k.starts_with("convoy"))).count());
    Ok(v)
}
fn convoys_json(total: i32, bases: &[NavalBase], lines: Vec<Value>) -> Value {
    json!({"convoys":{"total":total,"source":"memory:country+0xf88+0x88","validated":true,
            "evidence":"equal to the top-bar convoy figure (194) on 10 April 1938"},
        "naval_bases":bases.iter().map(|b| json!({"province":b.province.as_ref().map(|p| p.province),"level":b.level})).collect::<Vec<_>>(),
        "naval_bases_validated":"province and level matched history/states (241=8, 6332=5)",
        "convoy_production":lines,
        "unavailable":{"needed":"not mapped","escorts":"not mapped","in_use":"not mapped",
            "lost":"no stored counter found","routes":"SRegionalConvoyData is per region, layout not decoded"}})
}

pub(super) fn fleets_schema() -> Value {
    json!({"name":"hoi4_fleets","description":"Own navy read from game memory (no panel, no input, never pauses): fleets, task forces (location: port province, state and strategic region; ship count by type; summed strength and percent; mission code and name: hold, patrol, strike_force, convoy_raiding, convoy_escort, training, reserve, invasion_support; admiral is per fleet) and per fleet the admiral and the assigned sea regions (fleet area of operation) and, with ships:true, every ship (id, name, type, hull, design/variant, strength, max strength, max organisation, experience, morale). Filter by fleet or task-force name with `fleet`. Morale and experience fields are validated:false; fuel, supply and current organisation are not mapped.","inputSchema":{"type":"object","properties":{"ships":{"type":"boolean","default":false},"fleet":{"type":"string","maxLength":64}},"additionalProperties":false}})
}
pub(super) fn convoys_schema() -> Value {
    json!({"name":"hoi4_convoys","description":"Own convoy count (equal to the top bar), own naval bases with level, and the convoy production lines, read from game memory (no input, never pauses). Needed convoys, escorts, convoys in use, convoy routes and lost convoys are not mapped and are listed under unavailable.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::super::testmem::{Mem, IMAGE};
    use super::*;

    /// One fleet, one task force at a port, two ships of one variant.
    fn world(mem: &Mem) -> u64 {
        let country = mem.alloc(0x1100);
        let variant = mem.alloc(0x500);
        mem.u64(variant, IMAGE + fleetmem::VARIANT_VTABLE);
        mem.i32(variant + fleetmem::VARIANT_ID, 1347);
        mem.string(variant + fleetmem::VARIANT_NAME, "Deutschland");
        mem.string(variant + fleetmem::VARIANT_CLASS_SUFFIX, "Klasse");
        mem.i32(variant + fleetmem::VARIANT_MAX_STRENGTH, 22_000_000);
        let kind = mem.alloc(0x300);
        mem.string(kind + fleetmem::TYPE_KEY, "ship_hull_cruiser_panzerschiff");
        mem.u64(variant + fleetmem::VARIANT_TYPE, kind);
        let mut ships = Vec::new();
        for (id, name) in [(31, "Deutschland"), (32, "Admiral Scheer")] {
            let s = mem.alloc(0x900);
            mem.u64(s, IMAGE + SHIP_VTABLE);
            mem.i32(s + SHIP_ID, id);
            mem.string(s + SHIP_TYPE, "heavy_cruiser");
            mem.string(s + SHIP_NAME, name);
            mem.pointer_array(s + SHIP_VARIANTS, &[variant]);
            mem.i32(s + SHIP_HP, if id == 31 { 22_000_000 } else { 11_000_000 });
            mem.i32(s + SHIP_MAX_ORG, 4_000_000);
            mem.i32(s + SHIP_XP_FACTOR, 30_000);
            mem.i32(s + SHIP_MORALE, 10_000_000);
            ships.push(s);
        }
        let region = mem.alloc(0x100);
        let template = mem.alloc(0x100);
        mem.string(template + fleetmem::TEMPLATE_NAME, "North-West Germany");
        mem.string(template + fleetmem::TEMPLATE_KEY, "STRATEGICREGION_6");
        mem.u64(region + fleetmem::REGION_TEMPLATE, template);
        let state = mem.alloc(0x100);
        mem.i32(state + fleetmem::STATE_ID, 56);
        let province = mem.alloc(0x200);
        mem.i32(province + fleetmem::PROVINCE_ID, 241);
        mem.u64(province + fleetmem::PROVINCE_STATE, state);
        mem.u64(province + fleetmem::PROVINCE_REGION, region);
        let tf = mem.alloc(0x500);
        mem.u64(tf, IMAGE + TASK_FORCE_VTABLE);
        mem.string(tf + TF_NAME, "Hochseeflotte");
        mem.u64(tf + TF_PROVINCE, province);
        mem.pointer_array(tf + TF_SHIPS, &ships);
        let fleet = mem.alloc(0x300);
        mem.u64(fleet, IMAGE + FLEET_VTABLE);
        mem.string(fleet + FLEET_NAME, "Kriegsmarine");
        mem.u64(fleet + FLEET_HOME_PORT, province);
        mem.pointer_array(fleet + FLEET_TASK_FORCES, &[tf]);
        mem.pointer_array(country + COUNTRY_FLEETS, &[fleet]);
        country
    }

    #[test]
    fn reads_fleets_task_forces_and_ships() {
        let mem = Mem::new();
        let country = world(&mem);
        let r = Reader::new(&mem);
        let found = fleets(&r, IMAGE, country).unwrap();
        assert_eq!(found.len(), 1);
        let tf = &found[0].task_forces[0];
        assert_eq!(tf.name, "Hochseeflotte");
        assert_eq!(tf.place.as_ref().unwrap().state, Some(56));
        assert_eq!(tf.ships[1].name, "Admiral Scheer");
        assert_eq!(tf.ships[0].variant, "Deutschland");
        assert_eq!(tf.ships[0].hull, "ship_hull_cruiser_panzerschiff");
        assert_eq!(tf.ships[0].max_organisation, 40.0);
        let v = fleets_json(&found, true, None);
        let tf = &v["fleets"][0]["task_forces"][0];
        assert_eq!(tf["ship_count"], 2);
        assert_eq!(tf["ships_by_type"]["heavy_cruiser"], 2);
        assert_eq!(tf["strength_percent"], 75.0);
        assert_eq!(tf["location"]["region"], "North-West Germany");
        assert_eq!(tf["ships"][0]["name"], "Deutschland");
        assert_eq!(v["totals"]["ships"], 2);
        let none = fleets_json(&found, false, Some("zzz"));
        assert!(none["fleets"].as_array().unwrap().is_empty());
        assert!(fleets_json(&found, false, Some("hochsee"))["fleets"][0]["task_forces"][0]
            .get("ships")
            .is_none());
    }
    #[test]
    fn refuses_wrong_object_types() {
        let mem = Mem::new();
        let country = world(&mem);
        let fleet = mem.alloc(0x300);
        mem.pointer_array(country + COUNTRY_FLEETS, &[fleet]);
        let r = Reader::new(&mem);
        assert_eq!(fleets(&r, IMAGE, country).unwrap_err().code, "invalid_object");
    }
    #[test]
    fn convoy_json_reports_count_and_gaps() {
        let v = convoys_json(
            194,
            &[NavalBase {
                province: Some(Place {
                    province: 241,
                    state: None,
                    region: None,
                }),
                level: 8,
            }],
            vec![],
        );
        assert_eq!(v["convoys"]["total"], 194);
        assert_eq!(v["naval_bases"][0]["level"], 8);
        assert!(v["unavailable"]["escorts"].is_string());
    }
}
