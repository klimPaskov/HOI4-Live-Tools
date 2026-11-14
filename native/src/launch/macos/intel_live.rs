//! Live espionage state read from game memory: whether the agency exists, its upgrades and the
//! one in progress, the recruited operatives with their missions, the offered candidates, the
//! wait for a locked operative slot, and the own intel networks per target country with the
//! strength of every covered state. Pure read: no input, never pauses.
//!
//! Mapped live on macOS 1.19.4.1 (14 November 2026, Italy 1936) by watching the objects change
//! while the agency was created, upgraded, an operative recruited and sent to build a network:
//!
//! * `CIntelligenceAgency` (country `+0xd68`): candidates array `+0x18`, finished upgrades
//!   `+0x60` (`{CAgencyUpgrade*, level}`), created flag `+0xb0`, creating flag `+0xb1`, progress
//!   of the creation or the running upgrade `+0xb8` (script fixed point, one unit a day),
//!   running upgrade `+0xc0`, recruited operatives `+0xe0`, operative slots `+0xf8`, days a
//!   newly opened slot has waited for its candidates `+0x100`.
//! * `COperativeLeader`: name `+0x20`, nationalities `+0xd88` (tag ids), owner `+0xe20`, code
//!   name `+0xe48`, embedded `COperativeMission` `+0xea0` whose `+0x10` is the mission object
//!   (`NOperativeMissions::C*`: target tag id `+0x10`, `CState*` `+0x18` for state missions).
//! * The networks hang off the game state: `+0x640` `CStrategicOperativeManager`, `+0x8` one
//!   `CStrategicOperative` per tag id, its `+0x10` the `CCountryIntelNetwork`s (target tag id
//!   `+0x8`, operative locations `+0x60` as `{id, type, state}`, sub-networks `+0x78` stored by
//!   value, 0xd8 bytes each). `CSubIntelNetwork`: strongest state `+0x20`, sum `+0x28`, target
//!   strength `+0x40`, covered states `+0x90` as `{CState*, strength}`.
//!
//! Every object is checked by its RTTI name and every array by its bounds; anything else fails
//! closed with a typed error.
use super::{mgmt, states};
use crate::{
    error::{Error, Result},
    memory::Reader,
    xlayout::{fld, Class},
};
use serde_json::{json, Value};

const COUNTRY: Class = Class::new("CCountry", 0x357e388);
const AGENCY_CLASS: Class = Class::new("CIntelligenceAgency", 0);
const LEADER_CLASS: Class = Class::new("COperativeLeader", 0);
const NETWORK_CLASS: Class = Class::new("CCountryIntelNetwork", 0);
const SUB_CLASS: Class = Class::new("CSubIntelNetwork", 0);
const GS_CLASS: Class = Class::new("CCurrentGameState", 0x35044e8);

const AGENCY: u64 = 0xd68;
const AGENCY_TYPE: &str = "19CIntelligenceAgency";
const CANDIDATES: u64 = 0x18;
const UPGRADES_DONE: u64 = 0x60;
const CREATED: u64 = 0xb0;
const CREATING: u64 = 0xb1;
const PROGRESS: u64 = 0xb8;
const UPGRADING: u64 = 0xc0;
const OPERATIVES: u64 = 0xe0;
const SLOTS: u64 = 0xf8;
const SLOT_WAIT: u64 = 0x100;
const UPGRADE_TYPE: &str = "14CAgencyUpgrade";
const UPGRADE_KEY: u64 = 0x10;

const LEADER_TYPE: &str = "16COperativeLeader";
const LEADER_NAME: u64 = 0x20;
const LEADER_NATIONALITIES: u64 = 0xd88;
const LEADER_CODENAME: u64 = 0xe48;
const LEADER_MISSION: u64 = 0xea0;
/// Index of the operation instance id the operative is assigned to (live 14 Nov 2026).
const LEADER_OPERATION: u64 = 0xda4;
const MISSION_TYPE: &str = "17COperativeMission";
const MISSION_OBJECT: u64 = 0x10;
const MISSION_TARGET: u64 = 0x10;
const MISSION_STATE: u64 = 0x18;
const MISSION_PREFIX: &str = "N18NOperativeMissions";

const GS_OPERATIVES: u64 = 0x640;
const MANAGER_TYPE: &str = "26CStrategicOperativeManager";
const MANAGER_BY_TAG: u64 = 0x8;
const STRATEGIC_TYPE: &str = "19CStrategicOperative";
const STRATEGIC_NETWORKS: u64 = 0x10;
const NETWORK_TYPE: &str = "20CCountryIntelNetwork";
const NETWORK_TARGET: u64 = 0x8;
const NETWORK_LOCATIONS: u64 = 0x60;
const NETWORK_SUBS: u64 = 0x78;
const SUB_TYPE: &str = "16CSubIntelNetwork";
const SUB_SIZE: u64 = 0xd8;
const SUB_MAX: u64 = 0x20;
const SUB_TOTAL: u64 = 0x28;
const SUB_TARGET: u64 = 0x40;
const SUB_STATES: u64 = 0x90;

const LIMIT: usize = 64;
const STATE_LIMIT: usize = 512;

fn at(r: &Reader<'_>, class: &Class, mac: u64) -> Result<u64> {
    fld(r.string_abi(), class, mac)
}

/// Fixed-point script value (`/ 100000`) rounded to 2 places.
fn fixed(r: &Reader<'_>, address: u64) -> Result<f64> {
    let raw = i64::from_le_bytes(r.read(address, 8)?.try_into().unwrap());
    Ok(mgmt::round(raw as f64 / mgmt::FIXED_SCALE, 2))
}

/// `{data, capacity, size}` array of `stride`-byte entries, bounded.
fn entries(r: &Reader<'_>, address: u64, stride: u64, limit: usize) -> Result<Vec<u64>> {
    let (_, data, size) = r.array_header(address)?;
    if size > limit {
        return Err(Error::new("invalid_array", "Implausible intelligence array"));
    }
    if size > 0 && !mgmt::is_pointer(data) {
        return Err(Error::new("invalid_pointer", "Stale intelligence array"));
    }
    Ok((0..size as u64).map(|i| data + i * stride).collect())
}

fn object(r: &Reader<'_>, pointer: u64, mangled: &str) -> Result<u64> {
    if !mgmt::is_pointer(pointer) {
        return Err(Error::new("invalid_pointer", "Stale intelligence object"));
    }
    mgmt::expect_type(r, pointer, mangled)?;
    Ok(pointer)
}

fn text(r: &Reader<'_>, address: u64) -> Result<String> {
    Ok(mgmt::text(r, address, 128)?.unwrap_or_default())
}

/// `build_intel_network` from `N18NOperativeMissions18CBuildIntelNetworkE`.
pub(super) fn mission_key(mangled: &str) -> Option<String> {
    let rest = mangled.strip_prefix(MISSION_PREFIX)?.strip_suffix('E')?;
    let name = rest.trim_start_matches(|c: char| c.is_ascii_digit());
    let name = name.strip_prefix('C')?;
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    (!out.is_empty()).then_some(out)
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Operative {
    pub name: String,
    pub codename: String,
    pub nationalities: Vec<String>,
    /// Mission key (`build_intel_network`, ...); `no_mission` when idle.
    pub mission: Option<String>,
    pub mission_target: Option<String>,
    pub mission_state: Option<i32>,
    /// The object's address, to join operations and locations.
    pub address: u64,
    /// The game's character id of the operative (operative locations name it).
    pub id: i32,
    /// Key of the operation the operative works on.
    pub operation: Option<String>,
    /// Index of the operation instance the operative is assigned to (leader +0xda4; the
    /// operative may still be travelling to the target).
    pub operation_id: Option<i32>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct SubNetwork {
    pub strongest: f64,
    pub total: f64,
    pub target: f64,
    pub states: Vec<(i32, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Network {
    pub target: String,
    pub subs: Vec<SubNetwork>,
    /// (operative character id, state) of every operative working the network.
    pub operatives: Vec<(i32, i32)>,
}

impl Network {
    /// Strength of the strongest covered state (what operations compare with their
    /// `network_strength`).
    pub fn strength(&self) -> f64 {
        self.subs.iter().map(|s| s.strongest).fold(0.0, f64::max)
    }
    #[cfg(test)]
    pub fn state(&self, id: i32) -> Option<f64> {
        self.subs
            .iter()
            .flat_map(|s| s.states.iter())
            .filter(|(s, _)| *s == id)
            .map(|(_, v)| *v)
            .reduce(f64::max)
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub(super) struct Agency {
    pub created: bool,
    pub creating: bool,
    pub progress_days: f64,
    pub upgrading: Option<String>,
    pub upgrades: Vec<(String, i32)>,
    pub operatives: Vec<Operative>,
    pub candidates: Vec<String>,
    /// Operative slots the agency has (`+0xf8`: 1 at creation, 2 after five upgrades).
    pub slots: i32,
    pub slot_wait_days: i32,
}

fn state_id(r: &Reader<'_>, image: u64, state: u64) -> Result<i32> {
    let l = crate::construction::layout(r);
    if !mgmt::is_pointer(state) || r.u64(state)? != image + l.state_vtable {
        return Err(Error::new("invalid_type", "Expected a state object"));
    }
    r.i32(state + l.state_id)
}

fn operative(r: &Reader<'_>, image: u64, leader: u64) -> Result<Operative> {
    let leader = object(r, leader, LEADER_TYPE)?;
    let name = text(r, leader + at(r, &LEADER_CLASS, LEADER_NAME)?)?;
    let codename = text(r, leader + at(r, &LEADER_CLASS, LEADER_CODENAME)?)?;
    let mut nationalities = Vec::new();
    for e in entries(r, leader + at(r, &LEADER_CLASS, LEADER_NATIONALITIES)?, 4, 16)? {
        nationalities.push(states::tag_name(r, image, r.i32(e)?)?);
    }
    let holder = leader + at(r, &LEADER_CLASS, LEADER_MISSION)?;
    mgmt::expect_type(r, holder, MISSION_TYPE)?;
    let mission_object = r.u64(holder + MISSION_OBJECT)?;
    let (mut mission, mut mission_target, mut mission_state) = (None, None, None);
    if mgmt::is_pointer(mission_object) {
        let kind = mgmt::type_name(r, mission_object)?;
        mission = mission_key(&kind);
        if mission.is_none() {
            return Err(Error::new("invalid_type", format!("Unknown mission class {kind}")));
        }
        if mission.as_deref() != Some("no_mission") {
            let tag = r.i32(mission_object + MISSION_TARGET)?;
            if tag > 0 {
                mission_target = Some(states::tag_name(r, image, tag)?);
            }
            let state = r.u64(mission_object + MISSION_STATE)?;
            if mgmt::is_pointer(state) {
                mission_state = state_id(r, image, state).ok();
            }
        }
    }
    Ok(Operative {
        name,
        codename,
        nationalities,
        mission,
        mission_target,
        mission_state,
        address: leader,
        // The character id is {i32 kind, i32 index} at +0x8; the index tells operatives apart.
        id: r.i32(leader + 0xc)?,
        operation: None,
        operation_id: Some(r.i32(leader + LEADER_OPERATION)?).filter(|i| *i > 0),
    })
}

pub(super) fn agency(r: &Reader<'_>, image: u64, country: u64) -> Result<Agency> {
    let a = mgmt::pointer(r, country + at(r, &COUNTRY, AGENCY)?)?;
    mgmt::expect_type(r, a, AGENCY_TYPE)?;
    let f = |mac| at(r, &AGENCY_CLASS, mac);
    let mut upgrades = Vec::new();
    for e in entries(r, a + f(UPGRADES_DONE)?, 16, LIMIT)? {
        let u = object(r, r.u64(e)?, UPGRADE_TYPE)?;
        upgrades.push((text(r, u + UPGRADE_KEY)?, r.i32(e + 8)?));
    }
    let running = r.u64(a + f(UPGRADING)?)?;
    let upgrading = if running == 0 {
        None
    } else {
        Some(text(r, object(r, running, UPGRADE_TYPE)? + UPGRADE_KEY)?)
    };
    let mut operatives = Vec::new();
    for e in entries(r, a + f(OPERATIVES)?, 8, LIMIT)? {
        operatives.push(operative(r, image, r.u64(e)?)?);
    }
    // Each operative's operation, joined by character index (an unreadable operation list
    // leaves `operation` unknown rather than failing the agency read, except a torn read).
    match operations(r, image, country) {
        Ok(ops) => {
            for o in operatives.iter_mut() {
                o.operation = ops
                    .iter()
                    .find(|op| o.operation_id == Some(op.id) || op.operatives.contains(&o.id))
                    .map(|op| op.key.clone());
            }
        }
        Err(e) if e.code == "torn_read" => return Err(e),
        Err(_) => {}
    }
    let mut candidates = Vec::new();
    for e in entries(r, a + f(CANDIDATES)?, 8, LIMIT)? {
        let c = object(r, r.u64(e)?, LEADER_TYPE)?;
        candidates.push(text(r, c + at(r, &LEADER_CLASS, LEADER_NAME)?)?);
    }
    Ok(Agency {
        created: r.byte(a + f(CREATED)?)? != 0,
        creating: r.byte(a + f(CREATING)?)? != 0,
        progress_days: fixed(r, a + f(PROGRESS)?)?,
        upgrading,
        upgrades,
        operatives,
        candidates,
        slots: r.i32(a + f(SLOTS)?)?,
        slot_wait_days: r.i32(a + f(SLOT_WAIT)?)?,
    })
}

/// The own networks, one per target country.
pub(super) fn networks(r: &Reader<'_>, image: u64, gs: u64, own_tag: i32) -> Result<Vec<Network>> {
    let manager = object(r, r.u64(gs + at(r, &GS_CLASS, GS_OPERATIVES)?)?, MANAGER_TYPE)?;
    let slot = r.array_entry(manager + MANAGER_BY_TAG, own_tag as usize, 8)?;
    let strategic = object(r, u64::from_le_bytes(slot.try_into().unwrap()), STRATEGIC_TYPE)?;
    let mut out = Vec::new();
    for e in entries(r, strategic + STRATEGIC_NETWORKS, 8, LIMIT)? {
        let n = object(r, r.u64(e)?, NETWORK_TYPE)?;
        let g = |mac| at(r, &NETWORK_CLASS, mac);
        let target = states::tag_name(r, image, r.i32(n + g(NETWORK_TARGET)?)?)?;
        let mut subs = Vec::new();
        for s in entries(r, n + g(NETWORK_SUBS)?, SUB_SIZE, LIMIT)? {
            mgmt::expect_type(r, s, SUB_TYPE)?;
            let h = |mac| at(r, &SUB_CLASS, mac);
            let mut covered = Vec::new();
            for p in entries(r, s + h(SUB_STATES)?, 16, STATE_LIMIT)? {
                covered.push((state_id(r, image, r.u64(p)?)?, fixed(r, p + 8)?));
            }
            subs.push(SubNetwork {
                strongest: fixed(r, s + h(SUB_MAX)?)?,
                total: fixed(r, s + h(SUB_TOTAL)?)?,
                target: fixed(r, s + h(SUB_TARGET)?)?,
                states: covered,
            });
        }
        let mut operatives = Vec::new();
        for p in entries(r, n + g(NETWORK_LOCATIONS)?, 16, LIMIT)? {
            // {i32 kind, i32 index} character id, then the state id (live 14 Nov 2026).
            operatives.push((r.i32(p + 4)?, r.i32(p + 8)?));
        }
        out.push(Network {
            target,
            subs,
            operatives,
        });
    }
    Ok(out)
}

const POLITICS: u64 = 0xd38;
const POLITICS_TYPE: &str = "16CPoliticalStatus";
const RULING_PARTY: u64 = 0xd0;
const PARTY_TYPE: &str = "15CPoliticalParty";
const PARTY_GROUP: u64 = 0x18;
const GROUP_TYPE: &str = "14CIdeologyGroup";
const GROUP_KEY: u64 = 0x18;

/// Ruling ideology group of `country` (`fascism`, ...): `CPoliticalStatus` (country `+0xd38`),
/// ruling `CPoliticalParty` `+0xd0`, its `CIdeologyGroup` `+0x18`, whose key is at `+0x18`
/// (read live 14 November 2026: Italy 1936 `fascism`).
pub(super) fn ruling_ideology(r: &Reader<'_>, country: u64) -> Result<String> {
    let politics = object(r, r.u64(country + at(r, &COUNTRY, POLITICS)?)?, POLITICS_TYPE)?;
    let party = object(r, r.u64(politics + RULING_PARTY)?, PARTY_TYPE)?;
    let group = object(r, r.u64(party + PARTY_GROUP)?, GROUP_TYPE)?;
    let key = text(r, group + GROUP_KEY)?;
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(Error::new("invalid_string", "Unexpected ideology key"));
    }
    Ok(key)
}

pub(super) fn operative_json(index: usize, o: &Operative) -> Value {
    json!({
        "index": index,
        "name": o.name,
        "codename": o.codename,
        "nationalities": o.nationalities,
        "mission": o.mission,
        "mission_target": o.mission_target,
        "mission_state": o.mission_state,
        "idle": o.mission.is_none() || o.mission.as_deref() == Some("no_mission"),
        "operation": o.operation,
        "available_for_operation": o.operation.is_none(),
    })
}

pub(super) fn agency_json(a: &Agency) -> Value {
    json!({
        "created": a.created,
        "creating": a.creating,
        "progress_days": a.progress_days,
        "upgrading": a.upgrading,
        "upgrades_done": a.upgrades.iter().map(|(k, l)| json!({"key": k, "level": l})).collect::<Vec<_>>(),
        "upgrade_count": a.upgrades.iter().map(|(_, l)| *l).sum::<i32>(),
        "candidates": a.candidates,
        "operative_slots": a.slots,
        "locked_slot_waited_days": a.slot_wait_days,
    })
}

pub(super) fn network_json(n: &Network) -> Value {
    let mut states: Vec<(i32, f64)> = n.subs.iter().flat_map(|s| s.states.iter().copied()).collect();
    states.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    json!({
        "target": n.target,
        "strength": n.strength(),
        "target_strength": n.subs.iter().map(|s| s.target).fold(0.0, f64::max),
        "sub_networks": n.subs.len(),
        "states": states.iter().map(|(s, v)| json!({"state_id": s, "strength": v})).collect::<Vec<_>>(),
        "operative_states": n.operatives.iter().map(|(_, s)| *s).collect::<Vec<_>>(),
    })
}

/// One operation of the player (preparing or running).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Operation {
    pub key: String,
    pub target: String,
    /// Index of the instance's own id `{kind, index}` at +0x8; operatives name it.
    pub id: i32,
    /// Character indexes of the operatives already at work in the target (an operative still
    /// travelling there is assigned, `Operative::operation_id`, but not listed yet).
    pub operatives: Vec<i32>,
    /// The game commences the operation itself once its resources are gathered.
    pub auto_commence: bool,
    pub address: u64,
}

/// `CCountryOperationManager` at country +0x1308 holds the country's operations as an array of
/// `COperationInstance*` at +0x10 (live 14 Nov 2026: one entry while a preparation ran, none
/// after its refund). An instance: `COperation*` +0x48 (key +0x450), owner country +0x50, target
/// tag id +0x58, operatives +0xd8 as 8-byte character ids `{kind, index}`.
const OPERATION_MANAGER: u64 = 0x1308;
const OPERATION_MANAGER_TYPE: &str = "24CCountryOperationManager";
const OPERATION_LIST: u64 = 0x10;
const INSTANCE_TYPE: &str = "18COperationInstance";
const INSTANCE_OPERATION: u64 = 0x48;
const INSTANCE_OWNER: u64 = 0x50;
const INSTANCE_TARGET: u64 = 0x58;
const INSTANCE_OPERATIVES: u64 = 0xd8;
const INSTANCE_ID: u64 = 0xc;
/// 1 with Commence when ready ticked, 0 without (live 14 Nov 2026, both seen).
const INSTANCE_AUTO_COMMENCE: u64 = 0x40;
const OPERATION_KEY: u64 = 0x450;

pub(super) fn operations(r: &Reader<'_>, image: u64, country: u64) -> Result<Vec<Operation>> {
    let manager = mgmt::pointer(r, country + OPERATION_MANAGER)?;
    mgmt::expect_type(r, manager, OPERATION_MANAGER_TYPE)?;
    let mut out = Vec::new();
    for p in entries(r, manager + OPERATION_LIST, 8, LIMIT)? {
        let inst = mgmt::pointer(r, p)?;
        mgmt::expect_type(r, inst, INSTANCE_TYPE)?;
        if r.u64(inst + INSTANCE_OWNER)? != country {
            return Err(Error::new("torn_read", "Operation owner changed during the read"));
        }
        let op = mgmt::pointer(r, inst + INSTANCE_OPERATION)?;
        let key = mgmt::text(r, op + OPERATION_KEY, 128)?.unwrap_or_default();
        let target = states::tag_name(r, image, r.i32(inst + INSTANCE_TARGET)?)?;
        let mut operatives = Vec::new();
        for e in entries(r, inst + INSTANCE_OPERATIVES, 8, 16)? {
            let index = r.i32(e + 4)?;
            if index > 0 {
                operatives.push(index);
            }
        }
        out.push(Operation {
            key,
            target,
            id: r.i32(inst + INSTANCE_ID)?,
            operatives,
            auto_commence: r.byte(inst + INSTANCE_AUTO_COMMENCE)? != 0,
            address: inst,
        });
    }
    Ok(out)
}

pub(super) fn operation_json(o: &Operation) -> Value {
    json!({"operation": o.key, "target": o.target, "id": o.id, "operatives_at_work": o.operatives,
           "auto_commence": o.auto_commence})
}

#[cfg(test)]
pub(super) mod tests {
    use super::super::mgmt::fake::Mem;
    use super::*;

    pub(crate) const IMAGE: u64 = 0x1_0000_0000;

    pub(crate) fn typed(m: &Mem, at: u64, class: &str, slot: u64) {
        let vt = 0x41000 + slot * 0x100;
        m.q(at, vt);
        m.q(vt - 8, vt + 0x40);
        m.q(vt + 0x40 + 8, vt + 0x80);
        m.patch(vt + 0x80, format!("{class}\0").as_bytes());
    }
    pub(crate) fn array(m: &Mem, at: u64, data: u64, size: i32) {
        m.q(at, data);
        m.i(at + 8, size);
        m.i(at + 12, size);
    }

    #[test]
    fn mission_keys_follow_the_class_names() {
        for (class, key) in [
            ("N18NOperativeMissions18CBuildIntelNetworkE", "build_intel_network"),
            ("N18NOperativeMissions20CCounterIntelligenceE", "counter_intelligence"),
            ("N18NOperativeMissions10CNoMissionE", "no_mission"),
            ("N18NOperativeMissions11CPropagandaE", "propaganda"),
        ] {
            assert_eq!(mission_key(class).as_deref(), Some(key));
        }
        assert_eq!(mission_key("17COperativeMission"), None);
    }

    #[test]
    fn network_strength_is_the_strongest_state() {
        let n = Network {
            target: "ETH".into(),
            subs: vec![
                SubNetwork { strongest: 35.36, total: 102.96, target: 105.0, states: vec![(842, 35.36), (841, 33.8)] },
                SubNetwork { strongest: 12.0, total: 12.0, target: 105.0, states: vec![(841, 12.0)] },
            ],
            operatives: vec![(4713, 842)],
        };
        assert_eq!(n.strength(), 35.36);
        assert_eq!(n.state(841), Some(33.8));
        assert_eq!(n.state(1), None);
        let v = network_json(&n);
        assert_eq!(v["states"][0]["state_id"], json!(842));
        assert_eq!(v["operative_states"], json!([842]));
    }

    #[test]
    fn agency_read_joins_upgrades_operatives_and_candidates() {
        let m = Mem::default();
        m.alloc(0x40000, 0x5000);
        let (country, ag, up, op, cand) = (0x100000u64, 0x110000u64, 0x120000u64, 0x130000u64, 0x140000u64);
        for (a, n) in [(country, 0x1000), (ag, 0x200), (up, 0x100), (op, 0x1000), (cand, 0x1000), (0x150000, 0x100), (0x160000, 0x100)] {
            m.alloc(a, n);
        }
        m.q(country + AGENCY, ag);
        typed(&m, ag, AGENCY_TYPE, 0);
        typed(&m, up, UPGRADE_TYPE, 1);
        m.sso(up + UPGRADE_KEY, "upgrade_army_department");
        m.q(0x150000, up);
        m.i(0x150000 + 8, 1);
        array(&m, ag + UPGRADES_DONE, 0x150000, 1);
        m.q(ag + UPGRADING, 0);
        m.patch(ag + CREATED, &[1, 0]);
        m.l(ag + PROGRESS, 3_000_000);
        typed(&m, op, LEADER_TYPE, 2);
        m.sso(op + LEADER_NAME, "Francesco");
        m.sso(op + LEADER_CODENAME, "Lupo");
        array(&m, op + LEADER_NATIONALITIES, 0x160000, 0);
        typed(&m, op + LEADER_MISSION, MISSION_TYPE, 3);
        m.q(op + LEADER_MISSION + MISSION_OBJECT, 0);
        m.q(0x160040, op);
        array(&m, ag + OPERATIVES, 0x160040, 1);
        typed(&m, cand, LEADER_TYPE, 2);
        m.sso(cand + LEADER_NAME, "Benito");
        m.q(0x160080, cand);
        array(&m, ag + CANDIDATES, 0x160080, 1);
        m.i(ag + SLOT_WAIT, 6);
        let r = Reader::new(&m);
        let a = agency_read(&r, country);
        assert!(a.created && !a.creating);
        assert_eq!(a.progress_days, 30.0);
        assert_eq!(a.upgrades, vec![("upgrade_army_department".to_string(), 1)]);
        assert_eq!(a.operatives[0].codename, "Lupo");
        assert_eq!(a.operatives[0].mission, None);
        assert_eq!(a.candidates, ["Benito"]);
        let v = agency_json(&a);
        assert_eq!(v["upgrade_count"], json!(1));
        assert_eq!(operative_json(0, &a.operatives[0])["idle"], json!(true));
        // A wrong object type fails closed.
        typed(&m, up, "11CTechnology", 4);
        assert_eq!(agency(&r, IMAGE, country).unwrap_err().code, "invalid_type");
    }

    fn agency_read(r: &Reader<'_>, country: u64) -> Agency {
        agency(r, IMAGE, country).unwrap()
    }
}
