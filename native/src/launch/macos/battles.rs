//! `hoi4_battles`: every land battle a country takes part in, from the game's
//! combat manager in memory. Pure read: no input, no lease, no pause.
//!
//! Provenance (macOS 1.19.4.1, mapped 23 November 2026 on a paused Germany
//! study session at war day 4 of the West, read-only peeks; 28 land combats
//! in the world, 9 with Germany):
//! - `CCombatManager` is EMBEDDED in `CCurrentGameState` at `+0x228` (its
//!   vtable sits there); `+0x08` of it is the array of `CLandCombat*`.
//! - `CLandCombat`: `+0x1c` i32 combat id (a running counter), `+0x28` the
//!   attacker and `+0x30` the defender `CLandCombatant`, `+0x38` the
//!   `CProvince` fought over, `+0x40` i32 hours since the battle began (two
//!   equal copies; +9 after nine game hours), `+0x48` the `CTerrainType`
//!   (`+0x10` key, `+0x58` combat width, `+0x60` support width per extra
//!   direction, 1/100000 fixed: forest 60/30, plains and hills 70/35 as in
//!   common/terrain).
//! - `CLandCombatant`: `+0x20` every division of the side in this battle,
//!   `+0xe0` the divisions in the combat line, `+0xf8` the reserves waiting
//!   for width (23 = 4 + 19 for an attack of 23 divisions into a forest);
//!   `+0x128` `CCombatTactic` (`+0x10` key, e.g. tactic_shock); `+0x170` and
//!   `+0x178` the organisation and strength damage the side has TAKEN since the
//!   battle began (both grew every hour; the opponent's `+0x218` repeats this
//!   side's `+0x170`).
//! - `CArmy` (division): `+0x1d8` i32 owner tag index, `+0x418` strength,
//!   `+0x420` organisation (units.rs), `+0x468` planning (armies.rs), `+0x40`
//!   supply ratio (supplymem.rs), `+0x1f0` location.
//!
//! Not mapped (the game computes them for the combat view on demand): the
//! modifier lines (air superiority, CAS, night, entrenchment, leader, stacking,
//! encirclement, experience). Terrain, river and the forts of the state history
//! come from the map files (terrain.rs); planning and supply from the divisions.
use super::{diplomacy_state::date_json, states, ui::read, units};
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
    xlayout::{fld, img, Class},
};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

const GAME_STATE: Class = Class::new("CCurrentGameState", 0x35044e8);
const MANAGER: Class = Class::new("CCombatManager", 0x34b1930);
const COMBAT: Class = Class::new("CLandCombat", 0x34b0bd8);
const COMBATANT: Class = Class::new("CLandCombatant", 0x34b0ab0);
const TERRAIN: Class = Class::new("CTerrainType", 0x351fe78);
const TACTIC: Class = Class::new("CCombatTactic", 0x34b1b90);
const ARMY: Class = Class::new("CArmy", 0x34ace60);
const GS_COMBAT_MANAGER: u64 = 0x228;
const MANAGER_COMBATS: u64 = 0x08;
const COMBAT_ID: u64 = 0x1c;
const COMBAT_ATTACKER: u64 = 0x28;
const COMBAT_DEFENDER: u64 = 0x30;
const COMBAT_PROVINCE: u64 = 0x38;
const COMBAT_HOURS: u64 = 0x40;
const COMBAT_TERRAIN: u64 = 0x48;
const SIDE_ALL: u64 = 0x20;
const SIDE_ENGAGED: u64 = 0xe0;
const SIDE_RESERVE: u64 = 0xf8;
const SIDE_TACTIC: u64 = 0x128;
const SIDE_ORG_TAKEN: u64 = 0x170;
const SIDE_STRENGTH_TAKEN: u64 = 0x178;
const TERRAIN_KEY: u64 = 0x10;
const TERRAIN_WIDTH: u64 = 0x58;
const TERRAIN_SUPPORT_WIDTH: u64 = 0x60;
const TACTIC_KEY: u64 = 0x10;
const ARMY_TAG: u64 = 0x1d8;
const ARMY_STRENGTH: u64 = 0x418;
const ARMY_ORG: u64 = 0x420;
const ARMY_PLANNING: u64 = 0x468;
const ARMY_SUPPLY: u64 = 0x40;
const ARMY_LOCATION: u64 = 0x1f0;
const ARMY_HANDLE: u64 = 0x18;
const ARMY_TEMPLATE: u64 = 0x3b0;
const FIXED: f64 = 100_000.0;
/// More combats than this in one manager is not a combat array.
const COMBAT_LIMIT: usize = 4000;
const SIDE_LIMIT: usize = 400;

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Division {
    pub id: String,
    pub tag: String,
    pub template: Option<String>,
    pub width: Option<f64>,
    pub strength: f64,
    pub org: f64,
    pub planning: f64,
    pub supply: f64,
    pub province: Option<i32>,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Side {
    pub tactic: Option<String>,
    pub all: usize,
    pub engaged: Vec<Division>,
    pub reserve: Vec<Division>,
    pub org_taken: f64,
    pub strength_taken: f64,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Battle {
    pub id: i32,
    pub province: i32,
    pub state: Option<i32>,
    pub hours: i32,
    pub terrain: Option<String>,
    pub width: f64,
    pub support_width: f64,
    /// attacker, defender
    pub sides: [Side; 2],
}
impl Battle {
    fn tags(&self, side: usize) -> BTreeSet<&str> {
        let s = &self.sides[side];
        s.engaged
            .iter()
            .chain(&s.reserve)
            .map(|d| d.tag.as_str())
            .collect()
    }
    fn involves(&self, tag: &str) -> Option<usize> {
        (0..2).find(|&i| self.tags(i).contains(tag))
    }
}

/// What the map files add to a battle (terrain.rs); every lookup may be empty.
#[derive(Default)]
pub(super) struct MapFacts {
    /// terrain key -> attack modifier of common/terrain
    pub terrain_attack: BTreeMap<String, f64>,
    /// (from, to) province pair (low, high) -> ("small"|"large", penalty)
    pub rivers: BTreeMap<(i32, i32), (&'static str, f64)>,
    /// province -> highest bunker level of the state history
    pub forts: BTreeMap<i32, f64>,
}

fn fx(r: &Reader<'_>, a: u64) -> Result<f64> {
    Ok(i64::from_le_bytes(r.read(a, 8)?.try_into().unwrap()) as f64 / FIXED)
}
fn is_ptr(p: u64) -> bool {
    (0x10000..=0x7fff_ffff_0000).contains(&p) && p % 8 == 0
}
fn is(r: &Reader<'_>, image: u64, p: u64, class: &Class) -> Result<bool> {
    Ok(is_ptr(p) && r.u64(p)? == image + img(r.string_abi(), class.vtable)?)
}
fn round(v: f64, digits: i32) -> f64 {
    let m = 10f64.powi(digits);
    (v * m).round() / m
}

struct Ctx<'a> {
    r: &'a Reader<'a>,
    image: u64,
    tags: BTreeMap<i32, String>,
    templates: BTreeMap<u64, (Option<String>, Option<f64>)>,
    widths: Option<BTreeMap<String, f64>>,
}
impl Ctx<'_> {
    fn tag(&mut self, index: i32) -> String {
        if let Some(t) = self.tags.get(&index) {
            return t.clone();
        }
        let t = states::tag_name(self.r, self.image, index).unwrap_or_else(|_| format!("#{index}"));
        self.tags.insert(index, t.clone());
        t
    }
    fn template(&mut self, p: u64) -> (Option<String>, Option<f64>) {
        if let Some(t) = self.templates.get(&p) {
            return t.clone();
        }
        if self.widths.is_none() {
            self.widths = Some(
                units::catalog(self.r, self.image)
                    .map(|defs| defs.into_iter().map(|d| (d.key, d.width)).collect())
                    .unwrap_or_default(),
            );
        }
        let widths = self.widths.as_ref().unwrap();
        let found = match units::division_template(self.r, self.image, p) {
            Ok(t) => {
                let cells: Vec<&String> = t.columns.iter().flatten().collect();
                let width = cells
                    .iter()
                    .map(|k| widths.get(*k).copied())
                    .sum::<Option<f64>>()
                    .filter(|_| !cells.is_empty());
                (Some(t.name), width)
            }
            Err(_) => (None, None),
        };
        self.templates.insert(p, found.clone());
        found
    }
    fn division(&mut self, p: u64) -> Result<Division> {
        let (r, image) = (self.r, self.image);
        let abi = r.string_abi();
        if !is(r, image, p, &ARMY)? {
            return Err(Error::new("invalid_type", "Exact division required"));
        }
        let f = |o: u64| fld(abi, &ARMY, o);
        let c = crate::construction::layout(r);
        let location = r.u64(p + f(ARMY_LOCATION)?)?;
        let province = if location != 0 && r.u64(location)? == image + c.province_vtable {
            Some(r.i32(location + c.province_id)?)
        } else {
            None
        };
        let template = r.u64(p + f(ARMY_TEMPLATE)?)?;
        let (template, width) = self.template(template);
        let tag = r.i32(p + f(ARMY_TAG)?)?;
        Ok(Division {
            id: units::division_id(r, p, f(ARMY_HANDLE)?)?,
            tag: self.tag(tag),
            template,
            width,
            strength: fx(r, p + f(ARMY_STRENGTH)?)?,
            org: fx(r, p + f(ARMY_ORG)?)?,
            planning: fx(r, p + f(ARMY_PLANNING)?)?,
            supply: fx(r, p + f(ARMY_SUPPLY)?)?,
            province,
        })
    }
    fn divisions(&mut self, a: u64) -> Result<Vec<Division>> {
        let ptrs = self.r.pointer_array(a)?;
        if ptrs.len() > SIDE_LIMIT {
            return Err(Error::new(
                "invalid_array",
                "Too many divisions on one side",
            ));
        }
        ptrs.into_iter().map(|p| self.division(p)).collect()
    }
    fn side(&mut self, p: u64) -> Result<Side> {
        let (r, image) = (self.r, self.image);
        let abi = r.string_abi();
        if !is(r, image, p, &COMBATANT)? {
            return Err(Error::new("invalid_type", "Exact combat side required"));
        }
        let f = |o: u64| fld(abi, &COMBATANT, o);
        let tactic = r.u64(p + f(SIDE_TACTIC)?)?;
        let tactic = if is(r, image, tactic, &TACTIC)? {
            r.string_bounded(tactic + fld(abi, &TACTIC, TACTIC_KEY)?, 96)
                .ok()
        } else {
            None
        };
        let (_, _, all) = r.array_header(p + f(SIDE_ALL)?)?;
        Ok(Side {
            tactic,
            all,
            engaged: self.divisions(p + f(SIDE_ENGAGED)?)?,
            reserve: self.divisions(p + f(SIDE_RESERVE)?)?,
            org_taken: fx(r, p + f(SIDE_ORG_TAKEN)?)?,
            strength_taken: fx(r, p + f(SIDE_STRENGTH_TAKEN)?)?,
        })
    }
    fn battle(&mut self, p: u64) -> Result<Battle> {
        let (r, image) = (self.r, self.image);
        let abi = r.string_abi();
        if !is(r, image, p, &COMBAT)? {
            return Err(Error::new("invalid_type", "Exact land combat required"));
        }
        let f = |o: u64| fld(abi, &COMBAT, o);
        let c = crate::construction::layout(r);
        let province = r.u64(p + f(COMBAT_PROVINCE)?)?;
        if !is_ptr(province) || r.u64(province)? != image + c.province_vtable {
            return Err(Error::new("invalid_type", "Battle province required"));
        }
        let state = r.u64(province + c.province_state)?;
        let state = if state != 0 && r.u64(state)? == image + c.state_vtable {
            Some(r.i32(state + c.state_id)?)
        } else {
            None
        };
        let terrain = r.u64(p + f(COMBAT_TERRAIN)?)?;
        let (key, width, support) = if is(r, image, terrain, &TERRAIN)? {
            let t = |o: u64| fld(abi, &TERRAIN, o);
            (
                r.string_bounded(terrain + t(TERRAIN_KEY)?, 64).ok(),
                fx(r, terrain + t(TERRAIN_WIDTH)?)?,
                fx(r, terrain + t(TERRAIN_SUPPORT_WIDTH)?)?,
            )
        } else {
            (None, 0.0, 0.0)
        };
        Ok(Battle {
            id: r.i32(p + f(COMBAT_ID)?)?,
            province: r.i32(province + c.province_id)?,
            state,
            hours: r.i32(p + f(COMBAT_HOURS)?)?,
            terrain: key,
            width,
            support_width: support,
            sides: [
                self.side(r.u64(p + f(COMBAT_ATTACKER)?)?)?,
                self.side(r.u64(p + f(COMBAT_DEFENDER)?)?)?,
            ],
        })
    }
}

/// Every land combat in the world; a combat that does not read whole is skipped
/// (counted) instead of failing the read.
pub(super) fn combats(r: &Reader<'_>, image: u64, gs: u64) -> Result<(Vec<Battle>, usize)> {
    let abi = r.string_abi();
    let manager = gs + fld(abi, &GAME_STATE, GS_COMBAT_MANAGER)?;
    if !is_vtable_at(r, image, manager, &MANAGER)? {
        return Err(Error::new(
            "invalid_type",
            "Combat manager not found in the game state",
        ));
    }
    let ptrs = r.pointer_array(manager + fld(abi, &MANAGER, MANAGER_COMBATS)?)?;
    if ptrs.len() > COMBAT_LIMIT {
        return Err(Error::new("invalid_array", "Too many combats"));
    }
    let mut ctx = Ctx {
        r,
        image,
        tags: BTreeMap::new(),
        templates: BTreeMap::new(),
        widths: None,
    };
    let mut out = Vec::new();
    let mut skipped = 0;
    for p in ptrs {
        match ctx.battle(p) {
            Ok(b) => out.push(b),
            Err(e) if e.code == "torn_read" => return Err(e),
            Err(_) => skipped += 1,
        }
    }
    Ok((out, skipped))
}
fn is_vtable_at(r: &Reader<'_>, image: u64, a: u64, class: &Class) -> Result<bool> {
    Ok(r.u64(a)? == image + img(r.string_abi(), class.vtable)?)
}

fn mean(v: impl Iterator<Item = f64>) -> Option<f64> {
    let (mut s, mut n) = (0.0, 0.0);
    for x in v {
        s += x;
        n += 1.0;
    }
    (n > 0.0).then(|| s / n)
}

/// One side as the read shows it. `from` = the provinces the side's divisions
/// stand in (an attacker's directions).
fn side_json(side: &Side, role: &str, hours: i32, rows: bool) -> Value {
    let org_now: f64 = side.engaged.iter().map(|d| d.org).sum();
    let rate = if hours > 0 {
        side.org_taken / f64::from(hours)
    } else {
        0.0
    };
    let mut templates: BTreeMap<String, usize> = BTreeMap::new();
    for d in &side.reserve {
        *templates
            .entry(d.template.clone().unwrap_or_else(|| "?".into()))
            .or_default() += 1;
    }
    let tags: BTreeSet<&str> = side
        .engaged
        .iter()
        .chain(&side.reserve)
        .map(|d| d.tag.as_str())
        .collect();
    let row = |d: &Division| {
        json!({"id":d.id,"tag":d.tag,"template":d.template,"width":d.width,
        "strength":round(d.strength,1),"org":round(d.org,1),"planning":round(d.planning,2),
        "supply":round(d.supply,2),"province":d.province})
    };
    let mut v = json!({
        "role": role,
        "tags": tags,
        "tactic": side.tactic,
        "divisions": side.all,
        "engaged": side.engaged.len(),
        "reserve": side.reserve.len(),
        "width_engaged": side.engaged.iter().map(|d| d.width).sum::<Option<f64>>().map(|w| round(w, 1)),
        "org_engaged_mean": mean(side.engaged.iter().map(|d| d.org)).map(|v| round(v, 1)),
        "strength_engaged_mean": mean(side.engaged.iter().map(|d| d.strength)).map(|v| round(v, 1)),
        "planning_mean": mean(side.engaged.iter().chain(&side.reserve).map(|d| d.planning)).map(|v| round(v, 2)),
        "supply_mean": mean(side.engaged.iter().chain(&side.reserve).map(|d| d.supply)).map(|v| round(v, 2)),
        "org_damage_taken": round(side.org_taken, 1),
        "strength_damage_taken": round(side.strength_taken, 1),
        "org_damage_per_hour": round(rate, 2),
        "hours_until_broken": if rate > 0.0 { Some(round(org_now / rate, 1)) } else { None },
        "reserve_templates": templates,
    });
    if rows {
        v["engaged_divisions"] = side.engaged.iter().map(row).collect();
        v["reserve_divisions"] = side.reserve.iter().take(12).map(row).collect();
    }
    v
}

/// The battle as the caller reads it: sides, width, the lines the read can name,
/// who is winning by the organisation each side still has against the rate it loses it.
pub(super) fn battle_json(
    b: &Battle,
    date_raw: i32,
    me: Option<usize>,
    map: &MapFacts,
    rows: bool,
) -> Value {
    let attacker_from: BTreeSet<i32> = b.sides[0]
        .engaged
        .iter()
        .chain(&b.sides[0].reserve)
        .filter_map(|d| d.province)
        .filter(|p| *p != b.province)
        .collect();
    let directions = attacker_from.len().max(1);
    let width_max = b.width + b.support_width * (directions as f64 - 1.0);
    let used = b.sides[0]
        .engaged
        .iter()
        .map(|d| d.width)
        .sum::<Option<f64>>();
    let mut lines = Vec::new();
    if let Some(t) = b.terrain.as_ref() {
        if let Some(a) = map.terrain_attack.get(t).filter(|a| **a != 0.0) {
            lines.push(
                json!({"line":"terrain","side":"attacker","value":a,"source":"common/terrain"}),
            );
        }
    }
    let mut rivers = Vec::new();
    for from in &attacker_from {
        let key = if *from < b.province {
            (*from, b.province)
        } else {
            (b.province, *from)
        };
        if let Some((kind, penalty)) = map.rivers.get(&key) {
            rivers.push(json!({"from":from,"river":kind,"value":penalty}));
        }
    }
    if !rivers.is_empty() {
        let crossing = b.sides[0]
            .engaged
            .iter()
            .chain(&b.sides[0].reserve)
            .filter(|d| {
                d.province.is_some_and(|p| {
                    let k = if p < b.province {
                        (p, b.province)
                    } else {
                        (b.province, p)
                    };
                    map.rivers.contains_key(&k)
                })
            })
            .count();
        lines.push(json!({"line":"river","side":"attacker","crossings":rivers,
            "divisions_crossing":crossing,"source":"map rivers + defines"}));
    }
    if let Some(level) = map.forts.get(&b.province).filter(|l| **l > 0.0) {
        lines.push(json!({"line":"fort","side":"defender","level":level,"source":"state history (live level not read)"}));
    }
    for (i, role) in [(0, "attacker"), (1, "defender")] {
        if let Some(p) = mean(
            b.sides[i]
                .engaged
                .iter()
                .chain(&b.sides[i].reserve)
                .map(|d| d.planning),
        )
        .filter(|p| *p > 0.0)
        {
            lines.push(json!({"line":"planning","side":role,"value":round(p,2),"source":"division planning"}));
        }
        if let Some(s) = mean(b.sides[i].engaged.iter().map(|d| d.supply)).filter(|s| *s < 0.99) {
            lines.push(json!({"line":"supply","side":role,"value":round(s,2),"source":"division supply ratio"}));
        }
    }
    let last = |i: usize| -> Option<f64> {
        let s = &b.sides[i];
        let org: f64 = s.engaged.iter().map(|d| d.org).sum();
        (b.hours > 0 && s.org_taken > 0.0).then(|| org / (s.org_taken / f64::from(b.hours)))
    };
    let winning = match (last(0), last(1)) {
        (Some(a), Some(d)) if a > d * 1.25 => "attacker",
        (Some(a), Some(d)) if d > a * 1.25 => "defender",
        (Some(_), Some(_)) => "even",
        (None, Some(_)) => "attacker",
        (Some(_), None) => "defender",
        (None, None) => "unknown",
    };
    json!({
        "id": b.id,
        "province": b.province,
        "state": b.state,
        "terrain": b.terrain,
        "hours": b.hours,
        "since": date_json(date_raw - b.hours),
        "our_side": me.map(|i| if i == 0 { "attacker" } else { "defender" }),
        "width": {"terrain": b.width, "per_extra_direction": b.support_width,
                  "directions": directions, "attacker_max": width_max,
                  "attacker_engaged": used.map(|w| round(w, 1)),
                  "full": used.is_some_and(|w| w + 1e-6 >= width_max * 0.85)},
        "winning": winning,
        "lines": lines,
        "attacker": side_json(&b.sides[0], "attacker", b.hours, rows),
        "defender": side_json(&b.sides[1], "defender", b.hours, rows),
        "not_read": ["air superiority and CAS", "night", "entrenchment", "leader", "stacking and over-width penalties",
                     "encirclement", "experience", "the live fort level"],
    })
}

fn map_facts(process: &dyn Process, battles: &[Battle]) -> MapFacts {
    let mut out = MapFacts::default();
    let Some(identity) = process.identity() else {
        return out;
    };
    let Ok(dir) = super::mapdata::game_dir(&identity.exe) else {
        return out;
    };
    let Ok(t) = super::terrain::map_terrain(&dir) else {
        return out;
    };
    for (key, c) in &t.categories {
        if let Some(a) = c.attack {
            out.terrain_attack.insert(key.clone(), a);
        }
    }
    for b in battles {
        for d in b.sides[0].engaged.iter().chain(&b.sides[0].reserve) {
            let Some(p) = d.province.filter(|p| *p != b.province) else {
                continue;
            };
            let key = if p < b.province {
                (p, b.province)
            } else {
                (b.province, p)
            };
            if let Some(border) = t.borders.get(&key) {
                match border.river(&t.rivers) {
                    Some("small") => {
                        out.rivers.insert(key, ("small", t.rivers.penalty_small));
                    }
                    Some("large") => {
                        out.rivers.insert(key, ("large", t.rivers.penalty_large));
                    }
                    _ => {}
                }
            }
        }
        if let Some(level) = t.forts.get(&b.province).and_then(|f| {
            f.iter()
                .filter(|f| f.building == "bunker")
                .map(|f| f.level)
                .reduce(f64::max)
        }) {
            out.forts.insert(b.province, level);
        }
    }
    out
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_battles","description":"Read-only (memory, no input, no pause): every land battle a country fights (default the player's; country:TAG for another one, all:true for every battle in the world). Per battle: province, state, terrain, hours since it began; attacker and defender with their countries, tactic, divisions in the combat line and in reserve waiting for width (template, width, strength, organisation, planning, supply, the province they attack from), width used against the attacker's maximum (terrain width + support width per extra direction), the organisation and strength damage each side has taken and its rate, and who is winning (organisation left against the rate it is lost). Lines named: terrain attack (common/terrain), river crossings (map rivers), forts of the state history, planning and supply; the game's other modifier lines (air, night, entrenchment, leader, stacking) are listed under not_read. Sorted by the number of own divisions in the battle.","inputSchema":{"type":"object","properties":{"country":{"type":"string","pattern":"^[A-Z0-9]{3}$"},"all":{"type":"boolean"},"limit":{"type":"integer","minimum":1,"maximum":200},"divisions":{"type":"boolean","description":"list the divisions of each side (default true)"}},"additionalProperties":false}})
}

pub(super) fn battles(process: &dyn Process, args: &Map<String, Value>) -> Result<Value> {
    let all = args.get("all").and_then(Value::as_bool).unwrap_or(false);
    let rows = args
        .get("divisions")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(40) as usize;
    let wanted = args
        .get("country")
        .and_then(Value::as_str)
        .map(str::to_string);
    let (list, skipped, date, me) = read(process, |r, image| {
        let g = game::layout(r);
        let gs = r.u64(image + g.gs_global)?;
        let (list, skipped) = combats(r, image, gs)?;
        let date = r.i32(gs + g.date)?;
        let me = states::tag_name(r, image, r.i32(gs + g.player_tag)?)?;
        Ok((list, skipped, date, me))
    })?;
    let country = wanted.unwrap_or(me.clone());
    let mut chosen: Vec<(&Battle, Option<usize>, usize)> = list
        .iter()
        .filter_map(|b| {
            let side = b.involves(&country);
            if !all && side.is_none() {
                return None;
            }
            let n = side
                .map(|i| b.sides[i].engaged.len() + b.sides[i].reserve.len())
                .unwrap_or(0);
            Some((b, side, n))
        })
        .collect();
    chosen.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.province.cmp(&b.0.province)));
    let total = chosen.len();
    chosen.truncate(limit);
    let picked: Vec<Battle> = chosen.iter().map(|(b, _, _)| (*b).clone()).collect();
    let map = map_facts(process, &picked);
    let out: Vec<Value> = chosen
        .iter()
        .map(|(b, side, _)| battle_json(b, date, *side, &map, rows))
        .collect();
    let summary = summary(&chosen.iter().map(|(b, s, _)| (*b, *s)).collect::<Vec<_>>());
    Ok(json!({
        "country": country,
        "player": me,
        "date": date_json(date),
        "battles_in_world": list.len(),
        "battles": total,
        "shown": out.len(),
        "unreadable_combats": skipped,
        "summary": summary,
        "list": out,
    }))
}

/// Totals over the country's battles: how many divisions fight and how many wait.
pub(super) fn summary(rows: &[(&Battle, Option<usize>)]) -> Value {
    let (mut attacking, mut defending, mut engaged, mut reserve) = (0, 0, 0, 0);
    let mut waiting = Vec::new();
    for (b, side) in rows {
        let Some(i) = side else { continue };
        if *i == 0 {
            attacking += 1;
        } else {
            defending += 1;
        }
        engaged += b.sides[*i].engaged.len();
        reserve += b.sides[*i].reserve.len();
        if b.sides[*i].reserve.len() >= 3 {
            waiting.push(
                json!({"province":b.province,"reserve":b.sides[*i].reserve.len(),
                "engaged":b.sides[*i].engaged.len(),"terrain":b.terrain}),
            );
        }
    }
    json!({"attacking":attacking,"defending":defending,"divisions_engaged":engaged,
           "divisions_in_reserve":reserve,"reserve_heavy_battles":waiting})
}

#[cfg(test)]
mod tests {
    use super::super::testmem::{Mem, IMAGE};
    use super::*;

    fn div(tag: &str, org: f64, province: i32, width: f64, planning: f64) -> Division {
        Division {
            id: format!("{tag}{province}{org}"),
            tag: tag.into(),
            template: Some(
                if tag == "GER" {
                    "Infanterie-Division"
                } else {
                    "Division d'Infanterie"
                }
                .into(),
            ),
            width: Some(width),
            strength: 214.0,
            org,
            planning,
            supply: 1.0,
            province: Some(province),
        }
    }
    /// The Belgian battle of the study session (province 3488, forest, 72 hours):
    /// Armee Pommern's 23 divisions, 4 in the line and 19 in reserve, against 5 Belgians.
    fn belgian() -> Battle {
        let mut attack = Side {
            tactic: Some("tactic_shock".into()),
            all: 23,
            org_taken: 82.4,
            strength_taken: 128.4,
            ..Side::default()
        };
        for (org, p) in [(38.2, 3448), (40.0, 3448), (22.2, 6583), (39.3, 3448)] {
            attack.engaged.push(div("GER", org, p, 18.0, 0.33));
        }
        for k in 0..19 {
            attack.reserve.push(div(
                "GER",
                66.4,
                if k % 2 == 0 { 3448 } else { 6583 },
                18.0,
                0.33,
            ));
        }
        let mut defend = Side {
            tactic: Some("tactic_basic_defend".into()),
            all: 5,
            org_taken: 57.9,
            strength_taken: 90.3,
            ..Side::default()
        };
        for org in [21.5, 31.6, 34.5, 41.3, 36.0] {
            defend.engaged.push(div("BEL", org, 3488, 18.0, 0.0));
        }
        Battle {
            id: 5346,
            province: 3488,
            state: Some(980),
            hours: 72,
            terrain: Some("forest".into()),
            width: 60.0,
            support_width: 30.0,
            sides: [attack, defend],
        }
    }

    #[test]
    fn a_full_forest_attack_shows_its_reserve_and_width() {
        let b = belgian();
        let mut map = MapFacts::default();
        map.terrain_attack.insert("forest".into(), -0.15);
        map.rivers.insert((3448, 3488), ("small", -0.3));
        let v = battle_json(&b, 60797606, Some(0), &map, true);
        assert_eq!(v["our_side"], "attacker");
        assert_eq!(v["attacker"]["engaged"], 4);
        assert_eq!(v["attacker"]["reserve"], 19);
        assert_eq!(v["width"]["directions"], 2);
        assert_eq!(v["width"]["attacker_max"], 90.0);
        assert_eq!(v["width"]["attacker_engaged"], 72.0);
        assert_eq!(v["width"]["full"], false);
        assert_eq!(
            v["attacker"]["reserve_templates"]["Infanterie-Division"],
            19
        );
        let lines: Vec<&str> = v["lines"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l["line"].as_str().unwrap())
            .collect();
        assert_eq!(lines, ["terrain", "river", "planning"]);
        assert_eq!(v["lines"][1]["divisions_crossing"], 13);
        assert_eq!(v["since"]["raw"], 60797606 - 72);
        // attacker: 139.7 org left at 82.4/72 an hour = 122 h; defender 164.9 at 57.9/72 = 205 h
        assert_eq!(v["winning"], "defender");
    }

    #[test]
    fn the_summary_counts_battles_with_divisions_waiting() {
        let b = belgian();
        let s = summary(&[(&b, Some(0))]);
        assert_eq!(s["attacking"], 1);
        assert_eq!(s["divisions_engaged"], 4);
        assert_eq!(s["divisions_in_reserve"], 19);
        assert_eq!(s["reserve_heavy_battles"][0]["province"], 3488);
    }

    #[test]
    fn a_side_that_takes_no_damage_wins() {
        let mut b = belgian();
        b.sides[0].org_taken = 0.0;
        let v = battle_json(&b, 60797606, Some(0), &MapFacts::default(), false);
        assert_eq!(v["winning"], "attacker");
        assert!(v["attacker"].get("engaged_divisions").is_none());
    }

    fn fx_bits(v: f64) -> u64 {
        (v * FIXED).round() as i64 as u64
    }

    /// Fake memory with the macOS layout: one battle, one division a side.
    #[test]
    fn reads_a_battle_from_the_combat_manager() {
        let mem = Mem::new();
        let r = Reader::new(&mem);
        let c = crate::construction::layout(&r);
        let gs = mem.alloc(0x500);
        let manager = gs + GS_COMBAT_MANAGER;
        mem.u64(manager, IMAGE + MANAGER.vtable);
        let combat = mem.alloc(0xd8);
        mem.u64(combat, IMAGE + COMBAT.vtable);
        mem.pointer_array(manager + MANAGER_COMBATS, &[combat]);
        mem.i32(combat + COMBAT_ID, 5346);
        mem.i32(combat + COMBAT_HOURS, 72);
        let state = mem.alloc(0x100);
        mem.u64(state, IMAGE + c.state_vtable);
        mem.i32(state + c.state_id, 980);
        let province = mem.alloc(0x100);
        mem.u64(province, IMAGE + c.province_vtable);
        mem.i32(province + c.province_id, 3488);
        mem.u64(province + c.province_state, state);
        mem.u64(combat + COMBAT_PROVINCE, province);
        let terrain = mem.alloc(0x100);
        mem.u64(terrain, IMAGE + TERRAIN.vtable);
        mem.string(terrain + TERRAIN_KEY, "forest");
        mem.u64(terrain + TERRAIN_WIDTH, fx_bits(60.0));
        mem.u64(terrain + TERRAIN_SUPPORT_WIDTH, fx_bits(30.0));
        mem.u64(combat + COMBAT_TERRAIN, terrain);
        let from = mem.alloc(0x100);
        mem.u64(from, IMAGE + c.province_vtable);
        mem.i32(from + c.province_id, 3448);
        for (slot, org, at, reserve) in [
            (COMBAT_ATTACKER, 38.2, from, true),
            (COMBAT_DEFENDER, 21.5, province, false),
        ] {
            let side = mem.alloc(0x240);
            mem.u64(side, IMAGE + COMBATANT.vtable);
            let army = mem.alloc(0x600);
            mem.u64(army, IMAGE + ARMY.vtable);
            mem.u64(army + ARMY_ORG, fx_bits(org));
            mem.u64(army + ARMY_STRENGTH, fx_bits(214.0));
            mem.u64(army + ARMY_PLANNING, fx_bits(0.33));
            mem.u64(army + ARMY_SUPPLY, fx_bits(1.0));
            mem.u64(army + ARMY_LOCATION, at);
            mem.pointer_array(side + SIDE_ENGAGED, &[army]);
            mem.pointer_array(side + SIDE_ALL, &[army]);
            if reserve {
                let spare = mem.alloc(0x600);
                mem.u64(spare, IMAGE + ARMY.vtable);
                mem.u64(spare + ARMY_LOCATION, at);
                mem.pointer_array(side + SIDE_RESERVE, &[spare]);
                mem.pointer_array(side + SIDE_ALL, &[army, spare]);
            }
            mem.u64(side + SIDE_ORG_TAKEN, fx_bits(82.4));
            mem.u64(combat + slot, side);
        }
        let (list, skipped) = combats(&r, IMAGE, gs).unwrap();
        assert_eq!(skipped, 0);
        let b = &list[0];
        assert_eq!(
            (b.id, b.province, b.state, b.hours),
            (5346, 3488, Some(980), 72)
        );
        assert_eq!(b.terrain.as_deref(), Some("forest"));
        assert_eq!((b.width, b.support_width), (60.0, 30.0));
        assert_eq!(b.sides[0].engaged.len(), 1);
        assert_eq!(b.sides[0].reserve.len(), 1);
        assert_eq!(b.sides[0].all, 2);
        assert_eq!(b.sides[0].engaged[0].province, Some(3448));
        assert!((b.sides[0].engaged[0].org - 38.2).abs() < 1e-9);
        assert!((b.sides[0].org_taken - 82.4).abs() < 1e-9);
        // a combat whose side is not a combatant is skipped, not fatal
        mem.u64(mem.u64_at(combat + COMBAT_DEFENDER), 0x1234);
        let (list, skipped) = combats(&r, IMAGE, gs).unwrap();
        assert_eq!((list.len(), skipped), (0, 1));
    }
}
