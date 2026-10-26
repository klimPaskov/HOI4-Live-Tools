//! `hoi4_diplomacy_state` and the Stage B war data of `hoi4_wars`: the
//! player's wars, faction, guarantees, subjects, pacts, military access and
//! the world tension, from validated read-only memory. Pure read: no input, no
//! lease, no cursor, no pause. Only what the player's own diplomacy and war
//! screens would show; nothing about other countries beyond who is related to
//! whom (public relations) is read.
//!
//! Provenance (macOS 1.19.3.0, LC_UUID 089EA519, discovered 25 Oct 2026 with
//! read-only memory scans on a live Germany 1936 campaign; every offset was
//! found by scanning for vtables, none was copied from another platform):
//! - `CCountry + 0xd30` -> `CDiplomacyStatus` (vtable `+0x3342770`). Self
//!   checking: the pointee's first word must be the vtable.
//! - `CDiplomacyStatus + 0x20`: array `{ptr, cap i32, size i32}` of pointers
//!   to relation objects, every relation is listed by BOTH parties (same
//!   pointer in both lists). Seen live with vtables CWarRelation `0x3347ca8`,
//!   CGuaranteeRelation `0x3348258`, CSubjectRelation `0x3348598`,
//!   CNonAggressionPactRelation `0x3348328`, CMilitaryAccessRelation
//!   `0x3347e48`. The other relation classes are recognised by vtable only
//!   (never seen populated) and reported as `other`.
//! - Relation object: `+0x0xc` i32 first party tag index, `+0x10` i32 second
//!   party, `+0x20` i32 start date (game hours), `+0x38` i32 end date
//!   (`43808760` means none). Guarantee: first guarantees second; subject:
//!   first is overlord; military access: first may enter second (history
//!   `give_military_access` in the second's file); non-aggression pact is
//!   symmetric. War relation: the two tag fields are NOT reliable (civil war
//!   countries), so the other party is found by pointer identity only.
//! - Subject relation `+0x50` -> `CAutonomousState` (vtable `+0x3342550`),
//!   libc++ string key at `+0x8` (for example `autonomy_integrated_puppet`).
//! - `GS + 0x3b8` -> `CFactionSystem` (vtable `+0x333e920`), array of
//!   `CFaction*` at `+0x20` (vtable `+0x333ccd0`, string name at `+0x18`,
//!   array of `CCountry*` members at `+0x48`, first element the founder; the
//!   faction leader is NOT located, so `leader` is the first member and is
//!   marked unvalidated).
//! - `GS + 0x650` -> `CWorldThreat` (vtable `+0x3349148`), `+0x8` i32
//!   fixed-point world tension (`/100000`).
//! - `CWarGoal` (vtable `+0x3325720`): `+0x38`/`+0x3c` i32 attacker/target tag
//!   index, `+0x40` -> `CWarGoalType` (vtable `+0x3385390`) with libc++ key at
//!   `+0x10`. Which array of the diplomacy status holds which role of goal is
//!   not established, so goals are reported unclassified with their raw array
//!   offset, and justification progress/expiry are `offset_not_validated`.
//! - date: game hours; `60759371` is 1936-01-01 12:00 (the start date of the
//!   campaign at 12:00, and the start of every historical relation); no leap
//!   years (screen 24:00 12 Feb 1937 vs raw 60769153 and 17:00 26 Aug 1937 vs
//!   raw 60773825, within the 1-6 h latency of a screen capture).
//!
//! Live validation notes are in the `validation` object of the result.
use super::ui::read;
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    time::Instant,
};

const DIPLOMACY: u64 = 0xd30;
const DIPLOMACY_VTABLE: u64 = 0x3342770;
const DS_RELATIONS: u64 = 0x20;
/// Arrays of the diplomacy status that hold `CWarGoal*` (seen live).
const DS_GOAL_ARRAYS: [u64; 4] = [0x50, 0x68, 0x80, 0x98];
const REL_A: u64 = 0xc;
const REL_B: u64 = 0x10;
const REL_START: u64 = 0x20;
const REL_END: u64 = 0x38;
const REL_AUTONOMY: u64 = 0x50;
const NO_DATE: i32 = 43_808_760;
const AUTONOMY_VTABLE: u64 = 0x3342550;
const GS_FACTIONS: u64 = 0x3b8;
const FACTION_SYSTEM_VTABLE: u64 = 0x333e920;
const FS_LIST: u64 = 0x20;
const FACTION_VTABLE: u64 = 0x333ccd0;
const FACTION_NAME: u64 = 0x18;
const FACTION_MEMBERS: u64 = 0x48;
const GS_THREAT: u64 = 0x650;
const THREAT_VTABLE: u64 = 0x3349148;
const THREAT_VALUE: u64 = 0x8;
const WARGOAL_VTABLE: u64 = 0x3325720;
const WG_ATTACKER: u64 = 0x38;
const WG_TARGET: u64 = 0x3c;
const WG_TYPE: u64 = 0x40;
const WG_TYPE_VTABLE: u64 = 0x3385390;
const WG_TYPE_KEY: u64 = 0x10;
const START_DATE_RAW: i32 = 60_759_371;
const TAG_BUDGET: usize = 1024;
const RELATION_BUDGET: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    War,
    Guarantee,
    Subject,
    NonAggression,
    MilitaryAccess,
    /// A relation class we recognise by vtable but have never seen populated.
    Other(&'static str),
}

fn classify(vtable: u64) -> Option<Kind> {
    Some(match vtable {
        0x3347ca8 => Kind::War,
        0x3348258 => Kind::Guarantee,
        0x3348598 => Kind::Subject,
        0x3348328 => Kind::NonAggression,
        0x3347e48 => Kind::MilitaryAccess,
        0x3341dc0 => Kind::Other("air_base_access"),
        0x3341e90 => Kind::Other("offer_air_base_access"),
        0x3342018 => Kind::Other("docking_rights"),
        0x33420e8 => Kind::Other("offer_docking_rights"),
        0x33421e8 => Kind::Other("naval_blockade"),
        0x3347d78 => Kind::Other("licence_production_access"),
        0x3347f18 => Kind::Other("offer_military_access"),
        0x3347fe8 => Kind::Other("improve_relations"),
        0x33480b8 => Kind::Other("attache"),
        0x3348188 => Kind::Other("boost_party_popularity"),
        0x33483f8 => Kind::Other("lend_lease"),
        0x33486f8 => Kind::Other("embargo"),
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Pure helpers.

const MONTHS: [(&str, i32); 12] = [
    ("01", 31),
    ("02", 28),
    ("03", 31),
    ("04", 30),
    ("05", 31),
    ("06", 30),
    ("07", 31),
    ("08", 31),
    ("09", 30),
    ("10", 31),
    ("11", 30),
    ("12", 31),
];

/// Game hours (no leap years) as `YYYY-MM-DD HH:00`, relative to the
/// 1936-01-01 start date. `None` for the "no date" sentinel or a date before
/// the anchor.
fn iso(raw: i32) -> Option<String> {
    if raw == NO_DATE || raw < START_DATE_RAW {
        return None;
    }
    // the campaign starts at 12:00 on 1 January
    let hours = i64::from(raw - START_DATE_RAW) + 12;
    let (mut day, hour) = (hours / 24, hours % 24);
    let mut year = 1936 + day / 365;
    day %= 365;
    if year > 9999 {
        year = 9999;
    }
    for (month, len) in MONTHS {
        if day < i64::from(len) {
            return Some(format!("{year}-{month}-{:02} {hour:02}:00", day + 1));
        }
        day -= i64::from(len);
    }
    None
}

fn date_json(raw: i32) -> Value {
    if raw == NO_DATE {
        return json!({"raw":raw,"iso":null,"none":true});
    }
    json!({"raw":raw,"iso":iso(raw)})
}

/// Relation parties found by pointer identity: relation pointer -> holders.
type Holders = BTreeMap<u64, Vec<i32>>;

#[derive(Debug, PartialEq, Eq)]
struct WarPicture {
    /// (relation pointer, enemy tag index)
    enemies: Vec<(u64, i32)>,
    /// countries fighting an enemy of the player, not enemies themselves
    allies: BTreeSet<i32>,
}

/// Pure pairing step. `player_wars` are the player's war relation pointers,
/// `war_holders` every war relation pointer and the countries holding it.
/// A war relation must be held by exactly two countries, one of them the
/// player for the player's own relations.
fn war_picture(player: i32, player_wars: &[u64], war_holders: &Holders) -> Result<WarPicture> {
    let mut enemies = Vec::new();
    for ptr in player_wars {
        let holders = war_holders.get(ptr).map(Vec::as_slice).unwrap_or(&[]);
        let others: Vec<i32> = holders.iter().copied().filter(|t| *t != player).collect();
        if holders.len() != 2 || others.len() != 1 {
            return Err(Error::new(
                "war_relation_inconsistent",
                "A war relation must be held by exactly two countries",
            ));
        }
        enemies.push((*ptr, others[0]));
    }
    let enemy_set: BTreeSet<i32> = enemies.iter().map(|(_, t)| *t).collect();
    let mut allies = BTreeSet::new();
    for holders in war_holders.values() {
        if holders.len() != 2 || holders.contains(&player) {
            continue;
        }
        for (mine, theirs) in [(holders[0], holders[1]), (holders[1], holders[0])] {
            if enemy_set.contains(&theirs) && !enemy_set.contains(&mine) && mine != player {
                allies.insert(mine);
            }
        }
    }
    Ok(WarPicture { enemies, allies })
}

// ---------------------------------------------------------------------------
// Memory reads.

#[derive(Clone, Debug)]
struct Rel {
    ptr: u64,
    kind: Kind,
    a: i32,
    b: i32,
    start: i32,
    end: i32,
    autonomy: Option<String>,
}

#[derive(Clone, Debug)]
struct Goal {
    array: u64,
    attacker: i32,
    target: i32,
    kind: Option<String>,
}

struct World {
    gs: u64,
    player_tag: i32,
    player_country: u64,
    /// tag index -> country pointer
    country_of: BTreeMap<i32, u64>,
    /// country pointer -> tag index
    tag_of: HashMap<u64, i32>,
}

fn world(r: &Reader<'_>, image: u64) -> Result<World> {
    let g = game::layout(r);
    let gs = r.u64(image + g.gs_global)?;
    let player_tag = r.i32(gs + g.player_tag)?;
    let countries = r.pointer_array(gs + g.countries)?;
    let (header, data, size) = r.array_header(gs + g.tag_country_index)?;
    let _ = header;
    if size == 0 || size > TAG_BUDGET {
        return Err(Error::new("invalid_array", "Tag index table size"));
    }
    let raw = r.read(data, size * 4)?;
    let mut country_of = BTreeMap::new();
    let mut tag_of = HashMap::new();
    for tag in 1..size {
        let index = i32::from_le_bytes(raw[tag * 4..tag * 4 + 4].try_into().unwrap());
        if index < 0 {
            continue;
        }
        let Some(&country) = countries.get(index as usize) else {
            continue;
        };
        if country != 0 {
            country_of.insert(tag as i32, country);
            tag_of.insert(country, tag as i32);
        }
    }
    let player_country = *country_of
        .get(&player_tag)
        .ok_or_else(|| Error::new("campaign_unavailable", "Player country not found"))?;
    Ok(World {
        gs,
        player_tag,
        player_country,
        country_of,
        tag_of,
    })
}

fn diplomacy_status(r: &Reader<'_>, image: u64, country: u64) -> Result<u64> {
    let ds = r.u64(country + DIPLOMACY)?;
    if ds == 0 || r.u64(ds)? != image + DIPLOMACY_VTABLE {
        return Err(Error::new(
            "invalid_type",
            "Country has no CDiplomacyStatus at the expected offset",
        ));
    }
    Ok(ds)
}

fn tag_index(value: i32) -> Result<i32> {
    if (0..1024).contains(&value) {
        Ok(value)
    } else {
        Err(Error::new("invalid_relation", "Relation party out of range"))
    }
}

fn read_relation(r: &Reader<'_>, image: u64, ptr: u64) -> Result<Option<Rel>> {
    let vtable = r.u64(ptr)?;
    let Some(kind) = vtable.checked_sub(image).and_then(classify) else {
        return Ok(None);
    };
    let a = tag_index(r.i32(ptr + REL_A)?)?;
    let b = tag_index(r.i32(ptr + REL_B)?)?;
    let start = r.i32(ptr + REL_START)?;
    let end = r.i32(ptr + REL_END)?;
    let autonomy = if kind == Kind::Subject {
        let state = r.u64(ptr + REL_AUTONOMY)?;
        if state != 0 && r.u64(state)? == image + AUTONOMY_VTABLE {
            r.string_bounded(state + 8, 96).ok()
        } else {
            None
        }
    } else {
        None
    };
    Ok(Some(Rel {
        ptr,
        kind,
        a,
        b,
        start,
        end,
        autonomy,
    }))
}

/// Relations of one country plus how many entries had an unknown class.
fn relations_of(r: &Reader<'_>, image: u64, country: u64) -> Result<(Vec<Rel>, usize)> {
    let ds = diplomacy_status(r, image, country)?;
    let list = r.pointer_array(ds + DS_RELATIONS)?;
    if list.len() > RELATION_BUDGET {
        return Err(Error::new("invalid_array", "Relation list too long"));
    }
    let mut out = Vec::new();
    let mut unknown = 0;
    for ptr in list {
        if ptr == 0 {
            continue;
        }
        match read_relation(r, image, ptr)? {
            Some(rel) => out.push(rel),
            None => unknown += 1,
        }
    }
    Ok((out, unknown))
}

fn goals_of(r: &Reader<'_>, image: u64, country: u64) -> Result<Vec<Goal>> {
    let ds = diplomacy_status(r, image, country)?;
    let mut out = Vec::new();
    for array in DS_GOAL_ARRAYS {
        for ptr in r.pointer_array(ds + array)? {
            // One of these arrays holds plain tag indexes once a war is on
            // (value 6 seen live on 20 Dec 1936 with Luxembourg as the
            // target): anything that is not a heap pointer is skipped.
            if ptr < 0x10000 || ptr % 8 != 0 {
                continue;
            }
            let vtable = match r.u64(ptr) {
                Ok(v) => v,
                Err(e) if e.code == "read_bound" => continue,
                Err(e) => return Err(e),
            };
            if vtable != image + WARGOAL_VTABLE {
                continue;
            }
            let ty = r.u64(ptr + WG_TYPE)?;
            let kind = if ty != 0 && r.u64(ty)? == image + WG_TYPE_VTABLE {
                r.string_bounded(ty + WG_TYPE_KEY, 96).ok()
            } else {
                None
            };
            out.push(Goal {
                array,
                attacker: r.i32(ptr + WG_ATTACKER)?,
                target: r.i32(ptr + WG_TARGET)?,
                kind,
            });
        }
    }
    Ok(out)
}

#[derive(Clone, Debug)]
struct Faction {
    name: String,
    members: Vec<i32>,
    unresolved_members: usize,
}

fn factions(r: &Reader<'_>, image: u64, w: &World) -> Result<Vec<Faction>> {
    let system = r.u64(w.gs + GS_FACTIONS)?;
    if system == 0 || r.u64(system)? != image + FACTION_SYSTEM_VTABLE {
        return Err(Error::new("invalid_type", "No CFactionSystem at GS+0x3b8"));
    }
    let mut out = Vec::new();
    for ptr in r.pointer_array(system + FS_LIST)? {
        if ptr == 0 || r.u64(ptr)? != image + FACTION_VTABLE {
            continue;
        }
        let name = r.string_bounded(ptr + FACTION_NAME, 96)?;
        let mut members = Vec::new();
        let mut unresolved = 0;
        for member in r.pointer_array(ptr + FACTION_MEMBERS)? {
            if member == 0 {
                continue;
            }
            match w.tag_of.get(&member) {
                Some(tag) => members.push(*tag),
                None => unresolved += 1,
            }
        }
        out.push(Faction {
            name,
            members,
            unresolved_members: unresolved,
        });
    }
    Ok(out)
}

fn world_tension(r: &Reader<'_>, image: u64, w: &World) -> Result<f64> {
    let t = r.u64(w.gs + GS_THREAT)?;
    if t == 0 || r.u64(t)? != image + THREAT_VTABLE {
        return Err(Error::new("invalid_type", "No CWorldThreat at GS+0x650"));
    }
    Ok(f64::from(r.i32(t + THREAT_VALUE)?) / 100_000.0)
}

/// Every war relation of every country, grouped by pointer identity.
fn all_war_holders(r: &Reader<'_>, image: u64, w: &World) -> Result<Holders> {
    let mut out: Holders = BTreeMap::new();
    for (tag, country) in &w.country_of {
        let ds = diplomacy_status(r, image, *country)?;
        for ptr in r.pointer_array(ds + DS_RELATIONS)? {
            if ptr != 0 && r.u64(ptr)? == image + 0x3347ca8 {
                let holders = out.entry(ptr).or_default();
                if !holders.contains(tag) {
                    holders.push(*tag);
                }
            }
        }
    }
    Ok(out)
}

struct Names<'a> {
    r: &'a Reader<'a>,
    image: u64,
    w: &'a World,
    cache: HashMap<i32, (String, Option<String>)>,
}

impl<'a> Names<'a> {
    fn new(r: &'a Reader<'a>, image: u64, w: &'a World) -> Self {
        Self {
            r,
            image,
            w,
            cache: HashMap::new(),
        }
    }
    fn entry(&mut self, tag: i32) -> (String, Option<String>) {
        if let Some(found) = self.cache.get(&tag) {
            return found.clone();
        }
        let code = super::states::tag_name(self.r, self.image, tag)
            .unwrap_or_else(|_| format!("#{tag}"));
        let name = self
            .w
            .country_of
            .get(&tag)
            .and_then(|c| self.r.string_bounded(c + 0x10, 96).ok());
        self.cache.insert(tag, (code.clone(), name.clone()));
        (code, name)
    }
    fn who(&mut self, tag: i32) -> Value {
        let (code, name) = self.entry(tag);
        json!({"tag":code,"name":name})
    }
    fn code(&mut self, tag: i32) -> String {
        self.entry(tag).0
    }
}

// ---------------------------------------------------------------------------
// Public data for hoi4_wars.

/// What `hoi4_wars` adds in Stage B.
pub(super) struct PlayerWars {
    pub enemies: BTreeSet<String>,
    pub json: Value,
}

fn goal_json(names: &mut Names<'_>, g: &Goal) -> Value {
    json!({"type":g.kind,"attacker":names.who(g.attacker),"target":names.who(g.target),
        "diplomacy_array_offset":format!("0x{:x}",g.array),
        "role":"unclassified","justification_progress":null,
        "expires":null,"timers_reason":"offset_not_validated","validated":false})
}

fn wars_value(r: &Reader<'_>, image: u64, w: &World) -> Result<(Value, BTreeSet<String>)> {
    let (own, _) = relations_of(r, image, w.player_country)?;
    let own_wars: Vec<&Rel> = own.iter().filter(|x| x.kind == Kind::War).collect();
    let mut names = Names::new(r, image, w);
    if own_wars.is_empty() {
        return Ok((json!({"at_war":false,"count":0,"enemies":[],"wars":[],
            "allies_at_war":[],"validated":true,
            "validation":"empty list matches the screen at peace (no war overview shield, no war icon)"}),
            BTreeSet::new()));
    }
    let ptrs: Vec<u64> = own_wars.iter().map(|x| x.ptr).collect();
    let holders = all_war_holders(r, image, w)?;
    let picture = war_picture(w.player_tag, &ptrs, &holders)?;
    let goals = goals_of(r, image, w.player_country)?;
    let mut enemy_codes = BTreeSet::new();
    let mut wars = Vec::new();
    for (rel, enemy) in own_wars.iter().zip(picture.enemies.iter().map(|(_, t)| *t)) {
        let code = names.code(enemy);
        enemy_codes.insert(code.clone());
        let related: Vec<Value> = goals
            .iter()
            .filter(|g| g.target == enemy || g.attacker == enemy)
            .map(|g| goal_json(&mut names, g))
            .collect();
        wars.push(json!({"enemy":names.who(enemy),"started":date_json(rel.start),
            "relation_id":format!("0x{:x}",rel.ptr),
            "war_goals_involving_enemy":related,
            "attacker":null,"defender":null,"sides_reason":"offset_not_validated",
            "war_score":null,"war_score_reason":"offset_not_validated"}));
    }
    let allies: Vec<Value> = picture.allies.iter().map(|t| names.who(*t)).collect();
    Ok((json!({"at_war":true,"count":wars.len(),
        "enemies":enemy_codes.iter().collect::<Vec<_>>(),"wars":wars,
        "allies_at_war":allies,"leaders":null,"leaders_reason":"offset_not_validated",
        "validated":"enemies and start date by pointer identity (structure live-validated on a civil war); not yet compared with a player war on screen",
        "validation_note":"the player's own war has not been observed live"}),
        enemy_codes))
}

pub(super) fn player_wars(process: &dyn Process) -> Result<PlayerWars> {
    let (json, enemies) = read(process, |r, image| {
        let w = world(r, image)?;
        wars_value(r, image, &w)
    })?;
    Ok(PlayerWars { enemies, json })
}

// ---------------------------------------------------------------------------
// hoi4_diplomacy_state.

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_diplomacy_state","description":"Own diplomatic situation from memory, no input, no lease, no pause: wars with enemies and allies at war, own faction (name and members; leader is the first member and unvalidated; goals and rules null), guarantees given and received, non-aggression pacts, military access, overlord and subjects with autonomy level, world tension. Foreign opinion, faction goals and rules, justification timers, war score, volunteers and the list of available diplomatic actions are null with reason offset_not_validated (use hoi4_diplomacy for the action rows of the open panel).","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})
}

fn rel_json(names: &mut Names<'_>, rel: &Rel, me: i32) -> Value {
    let other = if rel.a == me { rel.b } else { rel.a };
    json!({"with":names.who(other),"from":names.who(rel.a),"to":names.who(rel.b),
        "since":date_json(rel.start),"until":date_json(rel.end)})
}

fn faction_json(names: &mut Names<'_>, list: &[Faction], me: i32) -> Value {
    let Some(f) = list.iter().find(|f| f.members.contains(&me)) else {
        return json!({"in_faction":false,"faction":null,"validated":true,
            "factions_in_game":list.iter().map(|f| f.name.clone()).collect::<Vec<_>>()});
    };
    let members: Vec<Value> = f.members.iter().map(|t| names.who(*t)).collect();
    json!({"in_faction":true,
        "faction":{"name":f.name,"member_count":f.members.len(),"members":members,
            "leader":f.members.first().map(|t| names.who(*t)),
            "leader_validated":false,
            "leader_note":"first member (the founder); the leader field itself is not located",
            "unresolved_members":f.unresolved_members,
            "goals":null,"rules":null,"goals_rules_reason":"offset_not_validated"},
        "validated":"membership matches history for the 1936 start (Axis with Germany); not compared with the faction screen"})
}

pub(super) fn state(process: &dyn Process) -> Result<Value> {
    let started = Instant::now();
    let mut out = read(process, |r, image| {
        let w = world(r, image)?;
        let me = w.player_tag;
        let mut names = Names::new(r, image, &w);
        let (rels, unknown) = relations_of(r, image, w.player_country)?;
        let list = factions(r, image, &w)?;
        let tension = world_tension(r, image, &w)?;
        let date = r.i32(w.gs + game::layout(r).date)?;
        let (wars, _) = wars_value(r, image, &w)?;

        let by = |kind: Kind| rels.iter().filter(move |x| x.kind == kind);
        let given: Vec<Value> = by(Kind::Guarantee)
            .filter(|x| x.a == me)
            .map(|x| rel_json(&mut names, x, me))
            .collect();
        let received: Vec<Value> = by(Kind::Guarantee)
            .filter(|x| x.b == me)
            .map(|x| rel_json(&mut names, x, me))
            .collect();
        let pacts: Vec<Value> = by(Kind::NonAggression)
            .map(|x| rel_json(&mut names, x, me))
            .collect();
        let access_to_them: Vec<Value> = by(Kind::MilitaryAccess)
            .filter(|x| x.a == me)
            .map(|x| rel_json(&mut names, x, me))
            .collect();
        let access_to_us: Vec<Value> = by(Kind::MilitaryAccess)
            .filter(|x| x.b == me)
            .map(|x| rel_json(&mut names, x, me))
            .collect();
        let overlord: Vec<Value> = by(Kind::Subject)
            .filter(|x| x.b == me)
            .map(|x| {
                let mut v = rel_json(&mut names, x, me);
                v["autonomy"] = json!(x.autonomy);
                v
            })
            .collect();
        let subjects: Vec<Value> = by(Kind::Subject)
            .filter(|x| x.a == me)
            .map(|x| {
                let mut v = rel_json(&mut names, x, me);
                v["autonomy"] = json!(x.autonomy);
                v
            })
            .collect();
        let others: Vec<Value> = rels
            .iter()
            .filter_map(|x| match x.kind {
                Kind::Other(name) => Some(json!({"class":name,"with":names.who(if x.a == me { x.b } else { x.a })})),
                _ => None,
            })
            .collect();
        let goals: Vec<Value> = goals_of(r, image, w.player_country)?
            .iter()
            .map(|g| goal_json(&mut names, g))
            .collect();
        Ok(json!({
            "source":"native own diplomacy state (memory)",
            "player":names.who(me),
            "date":date_json(date),
            "world_tension":{"value":tension,"percent":tension*100.0,"validated":true,
                "note":"screen 4% vs field 4.78%/4.84%, screen 8% vs field 8.541% (same instant): consistent with the screen flooring the field"},
            "wars":wars,
            "faction":faction_json(&mut names, &list, me),
            "guarantees_given":given,
            "guarantees_received":received,
            "non_aggression_pacts":pacts,
            "military_access_player_may_use":access_to_them,
            "military_access_granted_by_player":access_to_us,
            "overlord":overlord,
            "subjects":subjects,
            "other_relations":others,
            "unrecognised_relation_entries":unknown,
            "own_war_goals":goals,
            "volunteers":null,"volunteers_reason":"offset_not_validated",
            "opinion_of_countries":null,"opinion_reason":"offset_not_validated",
            "available_actions":null,"available_actions_reason":"offset_not_validated",
            "available_actions_hint":"hoi4_diplomacy lists the rows of the open diplomacy view for one target",
            "validation":{
                "diplomacy_status_pointer":"live validated: CCountry+0xd30 holds CDiplomacyStatus for all 440 countries",
                "relations":"live validated against history: ITA guarantees ALB, FRA guarantees CZE/YUG/ROM, CZE guarantees YUG/ROM, ENG overlord of the dominions and puppets, FRA of SYR/LEB, HOL of INS, JAP of MAN, DEN of ICE, USA of PHI, ENG and IRQ military access (IRQ gives, ENG may use)",
                "player_peace":"live: Germany has no relations and no faction relations other than Axis membership; screen shows no war",
                "war_relation_sides":"not trusted (civil war ids); pairing by pointer identity",
                "gaps":["war leaders and sides","war score","justification timers","faction goals/rules/leader","opinion","volunteers","action list"]},
            "observation_paused_game":false,
            "input_authorized":false,
        }))
    })?;
    out["freshness"] = json!({"elapsed_ms": started.elapsed().as_secs_f64() * 1000.0});
    Ok(out)
}

// ---------------------------------------------------------------------------
// Target-scoped snapshot and the country list (memory only).

fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::War => "war",
        Kind::Guarantee => "guarantee",
        Kind::Subject => "subject",
        Kind::NonAggression => "non_aggression",
        Kind::MilitaryAccess => "military_access",
        Kind::Other(name) => name,
    }
}

/// Tag index of the three-letter tag `tag`, from the game's tag table.
pub(super) fn tag_index_of(r: &Reader<'_>, image: u64, tag: &str) -> Result<i32> {
    let g = game::layout(r);
    let db = r.u64(image + g.tag_db_global)?;
    let (_, data, count) = r.array_header(db)?;
    for index in 1..count.min(TAG_BUDGET) {
        if let Ok(name) = r.string(data + index as u64 * g.tag_stride) {
            if name == tag {
                return Ok(index as i32);
            }
        }
    }
    Err(Error::new(
        "unknown_country",
        format!("No country with the tag {tag}"),
    ))
}

/// (state id, owner tag index) of every state in the catalog.
fn state_owners(r: &Reader<'_>, image: u64) -> Result<Vec<(i32, i32)>> {
    let g = game::layout(r);
    let l = crate::construction::layout(r);
    let gs = r.u64(image + g.gs_global)?;
    let mut out = Vec::new();
    for state in r.pointer_array(gs + l.gs_states)? {
        if state == 0 || r.u64(state)? != image + l.state_vtable {
            continue;
        }
        out.push((r.i32(state + l.state_id)?, r.i32(state + l.state_owner)?));
    }
    Ok(out)
}

/// State ids owned by tag `tag`, lowest id first (at most `limit`).
pub(super) fn states_owned_by(r: &Reader<'_>, image: u64, tag: &str, limit: usize) -> Result<Vec<i32>> {
    let owner = tag_index_of(r, image, tag)?;
    let mut ids: Vec<i32> = state_owners(r, image)?
        .into_iter()
        .filter(|(_, o)| *o == owner)
        .map(|(id, _)| id)
        .collect();
    ids.sort_unstable();
    ids.truncate(limit);
    Ok(ids)
}

/// What the player's relations with country `tag` look like now.
pub(super) fn snapshot(r: &Reader<'_>, image: u64, tag: &str) -> Result<super::diplomacy_keys::Snapshot> {
    let w = world(r, image)?;
    let me = w.player_tag;
    let target = tag_index_of(r, image, tag)?;
    let target_country = *w
        .country_of
        .get(&target)
        .ok_or_else(|| Error::new("unknown_country", "Country not in the game"))?;
    let theirs: BTreeSet<u64> = r
        .pointer_array(diplomacy_status(r, image, target_country)? + DS_RELATIONS)?
        .into_iter()
        .collect();
    let (mine, _) = relations_of(r, image, w.player_country)?;
    let mut snap = super::diplomacy_keys::Snapshot::default();
    for rel in mine.iter().filter(|x| theirs.contains(&x.ptr)) {
        snap.rels.insert((kind_label(rel.kind).to_string(), rel.a == me));
        if rel.kind == Kind::War {
            snap.at_war = true;
        }
    }
    snap.goals = goals_of(r, image, w.player_country)?
        .iter()
        .filter(|g| g.attacker == me && g.target == target)
        .count();
    let list = factions(r, image, &w)?;
    snap.same_faction = list
        .iter()
        .any(|f| f.members.contains(&me) && f.members.contains(&target));
    Ok(snap)
}

/// Own war goals as plain data: (attacker tag, target tag, type key).
pub(super) fn own_goals(r: &Reader<'_>, image: u64) -> Result<Vec<(String, String, Option<String>)>> {
    let w = world(r, image)?;
    let mut names = Names::new(r, image, &w);
    Ok(goals_of(r, image, w.player_country)?
        .into_iter()
        .map(|g| (names.code(g.attacker), names.code(g.target), g.kind))
        .collect())
}

/// (faction name, member tags) of the player's faction, if any.
pub(super) fn player_faction(process: &dyn Process) -> Result<Option<(String, Vec<String>)>> {
    read(process, |r, image| {
        let w = world(r, image)?;
        let mut names = Names::new(r, image, &w);
        let list = factions(r, image, &w)?;
        Ok(list
            .iter()
            .find(|f| f.members.contains(&w.player_tag))
            .map(|f| (f.name.clone(), f.members.iter().map(|t| names.code(*t)).collect())))
    })
}

pub(super) fn countries_tool() -> Value {
    json!({"name":"hoi4_diplomacy_countries","description":"Read-only (memory, no input, no pause): the countries of the world with what the player's own diplomacy shows about them: tag, name, number of states owned, faction name and members' faction, overlord, and the relations between the player and the country (war, guarantee given/received, non-aggression pact, military access each way, subject, lend-lease, docking rights, embargo...). Filters: query (tag or name part), relation (kind such as guarantee, non_aggression, military_access, subject, war), faction (name part), only_with_states (default true), limit (default 50, max 200) and offset. The actions available toward one country are listed by hoi4_diplomacy {country}; enemy opinion, AI intentions and anything hidden are not read.","inputSchema":{"type":"object","properties":{"query":{"type":"string","maxLength":64},"relation":{"type":"string","maxLength":48},"faction":{"type":"string","maxLength":64},"only_with_states":{"type":"boolean"},"limit":{"type":"integer","minimum":1,"maximum":200},"offset":{"type":"integer","minimum":0}},"additionalProperties":false}})
}

pub(super) fn countries(process: &dyn Process, args: &serde_json::Map<String, Value>) -> Result<Value> {
    let text = |key: &str| -> Result<Option<String>> {
        match args.get(key) {
            None => Ok(None),
            Some(v) => v
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 64 && !s.chars().any(char::is_control))
                .map(|s| Some(s.to_lowercase()))
                .ok_or_else(|| Error::new("invalid_arguments", format!("Invalid {key}"))),
        }
    };
    let query = text("query")?;
    let relation = text("relation")?;
    let faction_filter = text("faction")?;
    let only_states = match args.get("only_with_states") {
        None => true,
        Some(v) => v
            .as_bool()
            .ok_or_else(|| Error::new("invalid_arguments", "only_with_states must be boolean"))?,
    };
    let limit = match args.get("limit") {
        None => 50,
        Some(v) => v
            .as_u64()
            .filter(|v| (1..=200).contains(v))
            .ok_or_else(|| Error::new("invalid_arguments", "limit must be 1..200"))? as usize,
    };
    let offset = match args.get("offset") {
        None => 0,
        Some(v) => v
            .as_u64()
            .ok_or_else(|| Error::new("invalid_arguments", "offset must be a number"))? as usize,
    };
    let started = Instant::now();
    let (rows, total) = read(process, |r, image| {
        let w = world(r, image)?;
        let me = w.player_tag;
        let mut names = Names::new(r, image, &w);
        let mut owned: HashMap<i32, usize> = HashMap::new();
        for (_, owner) in state_owners(r, image)? {
            *owned.entry(owner).or_default() += 1;
        }
        let list = factions(r, image, &w)?;
        let (mine, _) = relations_of(r, image, w.player_country)?;
        let mine_by_ptr: HashMap<u64, &Rel> = mine.iter().map(|x| (x.ptr, x)).collect();
        let mut rows = Vec::new();
        for (tag, country) in &w.country_of {
            let states = owned.get(tag).copied().unwrap_or(0);
            if only_states && states == 0 && *tag != me {
                continue;
            }
            let code = names.code(*tag);
            let name = names.entry(*tag).1;
            if let Some(q) = &query {
                if !code.to_lowercase().contains(q)
                    && !name.as_deref().unwrap_or("").to_lowercase().contains(q)
                {
                    continue;
                }
            }
            let faction = list.iter().find(|f| f.members.contains(tag)).map(|f| f.name.clone());
            if let Some(q) = &faction_filter {
                if !faction.as_deref().unwrap_or("").to_lowercase().contains(q) {
                    continue;
                }
            }
            let ptrs = r.pointer_array(diplomacy_status(r, image, *country)? + DS_RELATIONS)?;
            let mut overlord = None;
            let mut with_me: Vec<Value> = Vec::new();
            for ptr in ptrs {
                if ptr == 0 {
                    continue;
                }
                if let Some(rel) = read_relation(r, image, ptr)? {
                    if rel.kind == Kind::Subject && rel.b == *tag {
                        overlord = Some(names.code(rel.a));
                    }
                }
                if *tag != me {
                    if let Some(rel) = mine_by_ptr.get(&ptr) {
                        let label = kind_label(rel.kind);
                        let dir = if rel.kind == Kind::War || rel.kind == Kind::NonAggression {
                            "mutual"
                        } else if rel.a == me {
                            "from_player"
                        } else {
                            "to_player"
                        };
                        with_me.push(json!({"kind":label,"direction":dir}));
                    }
                }
            }
            if let Some(q) = &relation {
                if !with_me.iter().any(|v| {
                    v["kind"].as_str().is_some_and(|k| k.to_lowercase().contains(q))
                }) {
                    continue;
                }
            }
            rows.push(json!({"tag":code,"name":name,"states":states,"is_player":*tag == me,
                "faction":faction,"overlord":overlord,"relations_with_player":with_me}));
        }
        let total = rows.len();
        Ok((rows.into_iter().skip(offset).take(limit).collect::<Vec<_>>(), total))
    })?;
    Ok(json!({"source":"native own diplomacy state (memory)","count":rows.len(),"total_matching":total,
        "offset":offset,"countries":rows,
        "validation":{"relations":"same pointer in both countries' relation lists (validated live 25 Oct 2026); direction from the first party tag",
            "names":"country name string at CCountry+0x10",
            "gaps":["opinion","ideology","army strength (hidden information, never read)"]},
        "observation_paused_game":false,"input_authorized":false,
        "freshness":{"elapsed_ms":started.elapsed().as_secs_f64()*1000.0}}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_follow_the_no_leap_calendar() {
        assert_eq!(iso(60_759_371).as_deref(), Some("1936-01-01 12:00"));
        assert_eq!(iso(60_759_371 + 12).as_deref(), Some("1936-01-02 00:00"));
        assert_eq!(iso(60_759_371 + 24 * 59).as_deref(), Some("1936-03-01 12:00"));
        assert_eq!(iso(60_759_371 + 24 * 365).as_deref(), Some("1937-01-01 12:00"));
        // screen: 24:00 on 12 Feb 1937 for raw 60769153, and 17:00 on 26 Aug
        // 1937 for raw 60773824..60773827 (the capture lags the read by 1-6 h)
        assert_eq!(iso(60_769_153).as_deref(), Some("1937-02-13 02:00"));
        assert_eq!(iso(60_773_825).as_deref(), Some("1937-08-26 18:00"));
        assert_eq!(iso(NO_DATE), None);
        assert_eq!(iso(0), None);
        assert_eq!(date_json(NO_DATE)["none"], json!(true));
    }

    #[test]
    fn classifies_the_live_seen_relation_vtables() {
        assert_eq!(classify(0x3347ca8), Some(Kind::War));
        assert_eq!(classify(0x3348258), Some(Kind::Guarantee));
        assert_eq!(classify(0x3348598), Some(Kind::Subject));
        assert_eq!(classify(0x3348328), Some(Kind::NonAggression));
        assert_eq!(classify(0x3347e48), Some(Kind::MilitaryAccess));
        assert_eq!(classify(0x33486f8), Some(Kind::Other("embargo")));
        assert_eq!(classify(0x1234), None);
    }

    #[test]
    fn pairs_wars_by_pointer_identity() {
        let mut holders: Holders = BTreeMap::new();
        holders.insert(10, vec![1, 5]); // player 1 vs 5
        holders.insert(11, vec![1, 6]); // player 1 vs 6
        holders.insert(12, vec![7, 5]); // 7 fights enemy 5 -> ally
        holders.insert(13, vec![7, 9]); // unrelated war of the ally
        holders.insert(14, vec![5, 6]); // enemies fighting each other
        holders.insert(15, vec![8, 9]); // unrelated
        let pic = war_picture(1, &[10, 11], &holders).unwrap();
        assert_eq!(pic.enemies, vec![(10, 5), (11, 6)]);
        assert_eq!(pic.allies, BTreeSet::from([7]));
    }

    #[test]
    fn rejects_inconsistent_war_relations() {
        let mut holders: Holders = BTreeMap::new();
        holders.insert(10, vec![1]);
        assert!(war_picture(1, &[10], &holders).is_err());
        holders.insert(10, vec![1, 2, 3]);
        assert!(war_picture(1, &[10], &holders).is_err());
        holders.insert(10, vec![2, 3]);
        assert!(war_picture(1, &[10], &holders).is_err());
        assert!(war_picture(1, &[99], &BTreeMap::new()).is_err());
        let empty = war_picture(1, &[], &BTreeMap::new()).unwrap();
        assert!(empty.enemies.is_empty() && empty.allies.is_empty());
    }

    #[test]
    fn faction_lookup_by_membership() {
        let list = [
            Faction {
                name: "Axis".into(),
                members: vec![1],
                unresolved_members: 0,
            },
            Faction {
                name: "Comintern".into(),
                members: vec![3, 69],
                unresolved_members: 0,
            },
        ];
        assert_eq!(
            list.iter().find(|f| f.members.contains(&69)).unwrap().name,
            "Comintern"
        );
        assert!(list.iter().all(|f| !f.members.contains(&2)));
    }

    /// Manual read-only check against an owned running game (no input):
    /// `HOI4_SESSION=<run>/session.json cargo test --release live_state -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_state_prints_for_manual_comparison() {
        let path = std::env::var("HOI4_SESSION").expect("HOI4_SESSION");
        let process = crate::platform::open(std::path::Path::new(&path)).unwrap();
        let started = Instant::now();
        let value = state(process.as_ref()).unwrap();
        eprintln!("{} ms", started.elapsed().as_millis());
        eprintln!("{}", serde_json::to_string_pretty(&value).unwrap());
        let wars = player_wars(process.as_ref()).unwrap();
        eprintln!("wars: {}", wars.json);
    }
}
