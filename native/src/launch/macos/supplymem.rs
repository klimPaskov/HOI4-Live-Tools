//! Memory layer of `hoi4_supply`: the engine's supply system on the macOS
//! Mach-O 089EA519 build (libc++), mapped read-only on 26 October 2026 on a
//! paused Germany 1938 game with 46 divisions. Nothing is copied from Linux
//! or Windows. Every offset names the evidence it was matched against; fields
//! that are only consistent (not yet seen on screen) say so.
//!
//! Objects (all reached from the game state, never by scanning):
//! * `CCurrentGameState` +0x398 -> `CSupplySystem` (RTTI 13CSupplySystem),
//!   +0x3a0 -> `CRailwayManager` (RTTI 15CRailwayManager). Both are single
//!   objects; the vtables are checked on every read.
//! * Division (`CArmy`): +0x30 supply need per day, +0x40 supply ratio
//!   (1.0 = fully supplied), +0x50 supply received. Consistency checked on
//!   all 46 divisions: received = need x ratio exactly, ratio 1.0 for 27 and
//!   0.94546 for the 19 divisions stacked in province 6521 whose demand
//!   (12.70) exceeds the province supply (10.16); every other province had
//!   demand well below supply. The same ratio is the `Supply status` the
//!   army tooltip shows (`ARMY_UNIT_SUPPLY` is `Supply status: VALUE|%0Y`)
//!   but no screen reading of an individual division was possible while the
//!   run belonged to another agent, so the field stays `validated:false`.
//! * `CSupplySystem` +0x130: array (size 13414 = provinces) of 0xd8-byte
//!   province records: +0x00 i64 demand placed on the province by the units
//!   standing in it, +0x08 i64 province supply, +0x30 an array of flows
//!   `{u16 node, u16 link, pad, i64 flow}` (16 bytes). Demand equals the sum
//!   of `need x f` of the divisions in the province for a constant `f` per
//!   province group (0.7367 / 0.2104 / 0.2651).
//! * `CSupplySystem` +0x28: array (1082 states, index = state id) of 0x28-byte
//!   records whose first i64 is the state's local supply. It equals
//!   `0.3 x infrastructure + max(0.1, 0.18e-6 x manpower) + 0.2 x
//!   local_supplies + sum(0.2 + 0.05 x vp)` (defines `INFRA_TO_SUPPLY`,
//!   `AVAILABLE_MANPOWER_STATE_SUPPLY`, `SUPPLY_BASE_MULT`, `VP_TO_SUPPLY_*`
//!   and the `local_supplies` key of `history/states`) for 700 of 1081 states
//!   to within 0.001; almost all others differ by exact multiples of 0.3,
//!   that is infrastructure built since the 1936 files.
//! * `CSupplySystem` +0x100: array of `CCountrySupplySystem` per country
//!   index; +0x78 is the owning country. Its +0xa0 is a sparse pool of 0x60
//!   byte supply-node records (used count at +0xa8, capacity at +0xac):
//!   +0x08 province, +0x0c kind (2 hub, 3 port), +0x14 node id. Germany had
//!   exactly the 31 records its used count says (ids 24..54); every hub
//!   province has a `supply_node` building and every port a `naval_base`.
//! * `CRailwayManager` +0x08: array indexed by province id of
//!   `CProvinceRailwayInfo` pointers (3215 non-null); +0x10 is the province
//!   `CBuildingStatus` (array at +0x38 of `CBuilding`, key at template +0x228,
//!   level = low u16 at +0x40): `rail_way` 1..5, `supply_node` 1,
//!   `naval_base` 1..10. 2786 of 3213 `rail_way` levels equal the highest
//!   link level of the province in `map/railways.txt`.
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
};
use std::collections::BTreeMap;

const FIXED: f64 = 100_000.0;

pub(super) const GS_SUPPLY_SYSTEM: u64 = 0x398;
pub(super) const GS_RAILWAY_MANAGER: u64 = 0x3a0;
pub(super) const SUPPLY_SYSTEM_VTABLE: u64 = 0x339ccd0;
pub(super) const RAILWAY_MANAGER_VTABLE: u64 = 0x339cc40;
const COUNTRY_SYSTEM_VTABLE: u64 = 0x339c498;
const SYS_STATES: u64 = 0x28;
const STATE_STRIDE: u64 = 0x28;
const SYS_COUNTRIES: u64 = 0x100;
const SYS_PROVINCES: u64 = 0x130;
const PROVINCE_STRIDE: u64 = 0xd8;
const PROVINCE_DEMAND: u64 = 0x00;
const PROVINCE_SUPPLY: u64 = 0x08;
const PROVINCE_FLOWS: u64 = 0x30;
const FLOW_STRIDE: u64 = 0x10;
const COUNTRY_SYSTEM_OWNER: u64 = 0x78;
const COUNTRY_NODES: u64 = 0xa0;
const NODE_STRIDE: u64 = 0x60;
const NODE_PROVINCE: u64 = 0x08;
const NODE_KIND: u64 = 0x0c;
const NODE_ONE: u64 = 0x10;
const NODE_ID: u64 = 0x14;
const NODE_OBJECT: u64 = 0x18;
const NODE_LINK: u64 = 0x20;
const NODE_SLOT_LIMIT: usize = 128;
const RAIL_PROVINCES: u64 = 0x08;
const RAIL_INFO_VTABLE: u64 = 0x339cbe0;
const RAIL_STATUS: u64 = 0x10;
const STATUS_BUILDINGS: u64 = 0x38;
const BUILDING_TEMPLATE: u64 = 0x1b0;
const BUILDING_LEVEL: u64 = 0x40;
const TEMPLATE_KEY: u64 = 0x228;
pub(super) const DIVISION_NEED: u64 = 0x30;
pub(super) const DIVISION_RATIO: u64 = 0x40;
pub(super) const DIVISION_RECEIVED: u64 = 0x50;
pub(super) const DIVISION_COVERED: u64 = 0x78;

fn fixed(r: &Reader<'_>, a: u64) -> Result<f64> {
    Ok(i64::from_le_bytes(r.read(a, 8)?.try_into().unwrap()) as f64 / FIXED)
}
fn le_i64(b: &[u8], o: usize) -> f64 {
    i64::from_le_bytes(b[o..o + 8].try_into().unwrap()) as f64 / FIXED
}
fn le_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn le_u64(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}
fn is_ptr(p: u64) -> bool {
    (0x10000..=0x7fff_ffff_0000).contains(&p) && p % 8 == 0
}
fn typed(r: &Reader<'_>, image: u64, p: u64, vtable: u64, what: &str) -> Result<()> {
    if !is_ptr(p) || r.u64(p)? != image + vtable {
        return Err(Error::new(
            "invalid_type",
            format!("Exact {what} object required"),
        ));
    }
    Ok(())
}

/// The two single objects of the supply machinery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Systems {
    pub supply: u64,
    pub railway: u64,
}
pub(super) fn systems(r: &Reader<'_>, image: u64, gs: u64) -> Result<Systems> {
    let supply = r.u64(gs + GS_SUPPLY_SYSTEM)?;
    typed(r, image, supply, SUPPLY_SYSTEM_VTABLE, "supply system")?;
    let railway = r.u64(gs + GS_RAILWAY_MANAGER)?;
    typed(r, image, railway, RAILWAY_MANAGER_VTABLE, "railway manager")?;
    Ok(Systems { supply, railway })
}
/// Game state of the running image (global at the build's `gs_global`).
pub(super) fn game_state(r: &Reader<'_>, image: u64) -> Result<u64> {
    r.u64(image + game::layout(r).gs_global)
}

/// Supply fields of one division (`CArmy`).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct DivisionSupply {
    /// Supply the division draws per day at full strength.
    pub need: f64,
    /// Received over needed, 1.0 = fully supplied.
    pub ratio: f64,
    pub received: f64,
    /// Raw field at +0x78, `need x (1 - f)` with a province constant `f`:
    /// meaning unproven (a local buffer is the candidate).
    pub covered: f64,
}
pub(super) fn division_supply(r: &Reader<'_>, division: u64) -> Result<DivisionSupply> {
    let b = r.read(division + DIVISION_NEED, 0x58)?;
    let need = le_i64(&b, 0);
    let ratio = le_i64(&b, (DIVISION_RATIO - DIVISION_NEED) as usize);
    let received = le_i64(&b, (DIVISION_RECEIVED - DIVISION_NEED) as usize);
    let covered = fixed(r, division + DIVISION_COVERED)?;
    if !(0.0..=100.0).contains(&need)
        || !(0.0..=1.0 + 1e-9).contains(&ratio)
        || (received - need * ratio).abs() > 0.002
    {
        return Err(Error::new(
            "invalid_supply",
            "Division supply fields failed the received = need x ratio check",
        ));
    }
    Ok(DivisionSupply {
        need,
        ratio,
        received,
        covered,
    })
}

/// One supply flow into a province from a node.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Flow {
    pub node: u16,
    pub flow: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct ProvinceSupply {
    /// Demand the units in the province place on it.
    pub demand: f64,
    /// Supply the province receives in total.
    pub supply: f64,
    pub flows: Vec<Flow>,
}
pub(super) fn province_supply(
    r: &Reader<'_>,
    sys: &Systems,
    province: i32,
) -> Result<ProvinceSupply> {
    let (_, data, count) = r.array_header(sys.supply + SYS_PROVINCES)?;
    if province < 0 || province as usize >= count {
        return Err(Error::new("invalid_index", "Province outside the supply map"));
    }
    let element = data + PROVINCE_STRIDE * province as u64;
    let b = r.read(element, PROVINCE_STRIDE as usize)?;
    let demand = le_i64(&b, PROVINCE_DEMAND as usize);
    let supply = le_i64(&b, PROVINCE_SUPPLY as usize);
    let ptr = le_u64(&b, PROVINCE_FLOWS as usize);
    let cap = le_u32(&b, PROVINCE_FLOWS as usize + 8);
    let size = le_u32(&b, PROVINCE_FLOWS as usize + 12);
    if size > cap || size > 64 || (size > 0 && !is_ptr(ptr)) || demand < 0.0 || supply < 0.0 {
        return Err(Error::new("invalid_array", "Province supply record invalid"));
    }
    let mut flows = Vec::new();
    if size > 0 {
        let f = r.read(ptr, size as usize * FLOW_STRIDE as usize)?;
        for i in 0..size as usize {
            flows.push(Flow {
                node: u16::from_le_bytes(f[i * 16..i * 16 + 2].try_into().unwrap()),
                flow: le_i64(&f, i * 16 + 8),
            });
        }
    }
    if r.read(element, PROVINCE_STRIDE as usize)? != b {
        return Err(Error::new("torn_read", "Province supply changed"));
    }
    Ok(ProvinceSupply {
        demand,
        supply,
        flows,
    })
}

/// Local (state) supply, the value the formula in the module notes gives.
pub(super) fn state_supply(r: &Reader<'_>, sys: &Systems, state: i32) -> Result<f64> {
    let (_, data, count) = r.array_header(sys.supply + SYS_STATES)?;
    if state < 1 || state as usize >= count {
        return Err(Error::new("invalid_index", "State outside the supply map"));
    }
    let v = fixed(r, data + STATE_STRIDE * state as u64)?;
    if !(0.0..=500.0).contains(&v) {
        return Err(Error::new("invalid_supply", "State supply out of range"));
    }
    Ok(v)
}

/// Buildings of a province that matter for supply.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ProvinceRail {
    pub rail_level: u32,
    pub supply_hub: bool,
    pub naval_base_level: u32,
}
/// `None` when the province has no railway record (no rail, no hub, no port).
pub(super) fn province_rail(
    r: &Reader<'_>,
    image: u64,
    sys: &Systems,
    province: i32,
) -> Result<Option<ProvinceRail>> {
    let (_, data, count) = r.array_header(sys.railway + RAIL_PROVINCES)?;
    if province < 0 || province as usize >= count {
        return Err(Error::new("invalid_index", "Province outside the railway map"));
    }
    let info = r.u64(data + 8 * province as u64)?;
    if info == 0 {
        return Ok(None);
    }
    typed(r, image, info, RAIL_INFO_VTABLE, "province railway info")?;
    let status = r.u64(info + RAIL_STATUS)?;
    if !is_ptr(status) {
        return Ok(None);
    }
    let (_, buildings, n) = r.array_header(status + STATUS_BUILDINGS)?;
    let mut out = ProvinceRail::default();
    for i in 0..n.min(32) as u64 {
        let building = r.u64(buildings + 8 * i)?;
        if !is_ptr(building) {
            continue;
        }
        let template = r.u64(building + BUILDING_TEMPLATE)?;
        if !is_ptr(template) {
            continue;
        }
        let key = r.string_bounded(template + TEMPLATE_KEY, 64)?;
        let level = u32::from(u16::from_le_bytes(
            r.read(building + BUILDING_LEVEL, 2)?.try_into().unwrap(),
        ));
        match key.as_str() {
            "rail_way" => out.rail_level = level,
            "supply_node" => out.supply_hub = level > 0,
            "naval_base" => out.naval_base_level = level,
            _ => {}
        }
    }
    Ok(Some(out))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NodeKind {
    Hub,
    Port,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Node {
    pub id: u32,
    pub province: i32,
    pub kind: NodeKind,
}
/// Supply nodes (hubs and ports) of one country. The pool is sparse: a slot
/// counts only when its shape matches and the number found equals the used
/// count the pool records (otherwise `invalid_supply`).
pub(super) fn country_nodes(
    r: &Reader<'_>,
    image: u64,
    sys: &Systems,
    country: u64,
    index: usize,
) -> Result<Vec<Node>> {
    let (_, data, count) = r.array_header(sys.supply + SYS_COUNTRIES)?;
    if index >= count {
        return Err(Error::new("invalid_index", "Country outside the supply map"));
    }
    let cs = r.u64(data + 8 * index as u64)?;
    typed(r, image, cs, COUNTRY_SYSTEM_VTABLE, "country supply system")?;
    if r.u64(cs + COUNTRY_SYSTEM_OWNER)? != country {
        return Err(Error::new(
            "invalid_supply",
            "Country supply system belongs to another country",
        ));
    }
    let pool = r.u64(cs + COUNTRY_NODES)?;
    let used = r.i32(cs + COUNTRY_NODES + 8)?;
    let cap = r.i32(cs + COUNTRY_NODES + 12)?;
    if !(0..=cap).contains(&used) || cap > 256 || (cap > 0 && !is_ptr(pool)) {
        return Err(Error::new("invalid_array", "Supply node pool invalid"));
    }
    // The header capacity (63 on the mapped build) is not the slot count: a
    // record of Germany sat in slot 64. Scan on past it, one slot at a time,
    // until the used count is reached or NODE_SLOT_LIMIT slots were looked at.
    let mut nodes: Vec<Node> = Vec::new();
    let mut slot = 0usize;
    while nodes.len() < used as usize && slot < NODE_SLOT_LIMIT {
        let b = match r.read(pool + slot as u64 * NODE_STRIDE, NODE_STRIDE as usize) {
            Ok(b) => b,
            Err(_) if slot >= cap as usize => break,
            Err(e) => return Err(e),
        };
        slot += 1;
        let province = le_u32(&b, NODE_PROVINCE as usize);
        let kind = le_u32(&b, NODE_KIND as usize);
        let id = le_u32(&b, NODE_ID as usize);
        let record = le_u32(&b, NODE_ONE as usize) == 1
            && le_u32(&b, NODE_LINK as usize) == 1
            && is_ptr(le_u64(&b, NODE_OBJECT as usize));
        if record
            && (kind == 2 || kind == 3)
            && (1..20_000).contains(&province)
            && (1..4096).contains(&id)
            && !nodes.iter().any(|n| n.id == id)
        {
            nodes.push(Node {
                id,
                province: province as i32,
                kind: if kind == 2 { NodeKind::Hub } else { NodeKind::Port },
            });
        }
    }
    if nodes.len() != used as usize {
        return Err(Error::new(
            "invalid_supply",
            "Supply node pool shape did not match its used count",
        ));
    }
    nodes.sort_by_key(|n| n.id);
    Ok(nodes)
}

/// Index of the own country in the per-country supply arrays: the position in
/// the tag -> country index table, the same one `own_country` uses.
pub(super) fn own_country_index(r: &Reader<'_>, image: u64) -> Result<usize> {
    let g = game::layout(r);
    let gs = r.u64(image + g.gs_global)?;
    let tag = r.i32(gs + g.player_tag)?;
    let index = i32::from_le_bytes(
        r.array_entry(gs + g.tag_country_index, tag as usize, 4)?
            .try_into()
            .unwrap(),
    );
    usize::try_from(index).map_err(|_| Error::new("invalid_index", "Own country index"))
}

// Pure rules.

/// Supply band shown to the player: below 50 percent is the game's "very low"
/// alert, below 70 percent the playbook's watch line.
pub(super) fn band(ratio: f64) -> &'static str {
    if ratio >= 0.999 {
        "full"
    } else if ratio >= 0.7 {
        "ok"
    } else if ratio >= 0.5 {
        "low"
    } else {
        "very_low"
    }
}
/// Need-weighted mean ratio of `(need, ratio)` rows; `None` without need.
pub(super) fn weighted_mean(rows: &[(f64, f64)]) -> Option<f64> {
    let need: f64 = rows.iter().map(|r| r.0).sum();
    if need <= 0.0 {
        return None;
    }
    Some(rows.iter().map(|r| r.0 * r.1).sum::<f64>() / need)
}
/// Provinces where the demand placed by the units standing in them exceeds
/// the supply the province receives, worst first: `(province, shortfall)`.
pub(super) fn overdrawn(provinces: &BTreeMap<i32, ProvinceSupply>) -> Vec<(i32, f64)> {
    let mut v: Vec<(i32, f64)> = provinces
        .iter()
        .filter(|(_, p)| p.demand > p.supply + 1e-6)
        .map(|(id, p)| (*id, p.demand - p.supply))
        .collect();
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
    v
}

#[cfg(test)]
mod tests {
    use super::super::testmem::{Mem, IMAGE};
    use super::*;

    fn fx(v: f64) -> u64 {
        (v * FIXED).round() as i64 as u64
    }
    /// Supply system with provinces 0..8, states 0..4, one country and the
    /// railway manager; province 5 holds demand 12.7 against supply 10.2 and
    /// two flows, province 6 is a hub with rail 2 and a naval base 4.
    fn world(mem: &Mem) -> (u64, Systems) {
        let gs = mem.alloc(0x500);
        let supply = mem.alloc(0x200);
        mem.u64(supply, IMAGE + SUPPLY_SYSTEM_VTABLE);
        mem.u64(gs + GS_SUPPLY_SYSTEM, supply);
        let railway = mem.alloc(0x100);
        mem.u64(railway, IMAGE + RAILWAY_MANAGER_VTABLE);
        mem.u64(gs + GS_RAILWAY_MANAGER, railway);
        // provinces
        let provinces = mem.alloc(PROVINCE_STRIDE * 8);
        mem.u64(supply + SYS_PROVINCES, provinces);
        mem.i32(supply + SYS_PROVINCES + 8, 8);
        mem.i32(supply + SYS_PROVINCES + 12, 8);
        let p5 = provinces + 5 * PROVINCE_STRIDE;
        mem.u64(p5 + PROVINCE_DEMAND, fx(12.7));
        mem.u64(p5 + PROVINCE_SUPPLY, fx(10.2));
        let flows = mem.alloc(32);
        mem.u64(flows, 43 | (1 << 16));
        mem.u64(flows + 8, fx(8.455));
        mem.u64(flows + 16, 45 | (1 << 16));
        mem.u64(flows + 24, fx(5.25));
        mem.u64(p5 + PROVINCE_FLOWS, flows);
        mem.i32(p5 + PROVINCE_FLOWS + 8, 8);
        mem.i32(p5 + PROVINCE_FLOWS + 12, 2);
        // states
        let states = mem.alloc(STATE_STRIDE * 4);
        mem.u64(supply + SYS_STATES, states);
        mem.i32(supply + SYS_STATES + 8, 4);
        mem.i32(supply + SYS_STATES + 12, 4);
        mem.u64(states + 2 * STATE_STRIDE, fx(4.75263));
        // railway
        let infos = mem.alloc(8 * 8);
        mem.u64(railway + RAIL_PROVINCES, infos);
        mem.i32(railway + RAIL_PROVINCES + 8, 8);
        mem.i32(railway + RAIL_PROVINCES + 12, 8);
        let info = mem.alloc(0x100);
        mem.u64(info, IMAGE + RAIL_INFO_VTABLE);
        let status = mem.alloc(0x100);
        mem.u64(info + RAIL_STATUS, status);
        let mut buildings = Vec::new();
        for (key, level) in [("rail_way", 2u16), ("supply_node", 1), ("naval_base", 4)] {
            let template = mem.alloc(0x300);
            mem.string(template + TEMPLATE_KEY, key);
            let b = mem.alloc(0x200);
            mem.u64(b + BUILDING_TEMPLATE, template);
            mem.i32(b + BUILDING_LEVEL, i32::from(level));
            buildings.push(b);
        }
        mem.pointer_array(status + STATUS_BUILDINGS, &buildings);
        mem.u64(infos + 8 * 6, info);
        (gs, Systems { supply, railway })
    }

    #[test]
    fn systems_need_the_exact_objects() {
        let mem = Mem::new();
        let (gs, expected) = world(&mem);
        let r = Reader::new(&mem);
        assert_eq!(systems(&r, IMAGE, gs).unwrap(), expected);
        mem.u64(expected.railway, 0x1234);
        assert_eq!(systems(&r, IMAGE, gs).unwrap_err().code, "invalid_type");
    }
    #[test]
    fn reads_province_supply_with_flows() {
        let mem = Mem::new();
        let (_, sys) = world(&mem);
        let r = Reader::new(&mem);
        let p = province_supply(&r, &sys, 5).unwrap();
        assert_eq!(p.demand, 12.7);
        assert_eq!(p.supply, 10.2);
        assert_eq!(
            p.flows,
            vec![
                Flow { node: 43, flow: 8.455 },
                Flow { node: 45, flow: 5.25 }
            ]
        );
        let empty = province_supply(&r, &sys, 1).unwrap();
        assert!(empty.flows.is_empty() && empty.demand == 0.0);
        assert_eq!(province_supply(&r, &sys, 8).unwrap_err().code, "invalid_index");
        assert_eq!(province_supply(&r, &sys, -1).unwrap_err().code, "invalid_index");
    }
    #[test]
    fn province_flow_array_must_be_sane() {
        let mem = Mem::new();
        let (_, sys) = world(&mem);
        let r = Reader::new(&mem);
        let data = r.u64(sys.supply + SYS_PROVINCES).unwrap();
        mem.i32(data + 5 * PROVINCE_STRIDE + PROVINCE_FLOWS + 12, 9);
        assert_eq!(province_supply(&r, &sys, 5).unwrap_err().code, "invalid_array");
    }
    #[test]
    fn reads_state_supply() {
        let mem = Mem::new();
        let (_, sys) = world(&mem);
        let r = Reader::new(&mem);
        assert_eq!(state_supply(&r, &sys, 2).unwrap(), 4.75263);
        assert_eq!(state_supply(&r, &sys, 0).unwrap_err().code, "invalid_index");
        assert_eq!(state_supply(&r, &sys, 4).unwrap_err().code, "invalid_index");
    }
    #[test]
    fn reads_province_buildings() {
        let mem = Mem::new();
        let (_, sys) = world(&mem);
        let r = Reader::new(&mem);
        let rail = province_rail(&r, IMAGE, &sys, 6).unwrap().unwrap();
        assert_eq!(
            rail,
            ProvinceRail {
                rail_level: 2,
                supply_hub: true,
                naval_base_level: 4
            }
        );
        assert_eq!(province_rail(&r, IMAGE, &sys, 3).unwrap(), None);
        assert_eq!(
            province_rail(&r, IMAGE, &sys, 9).unwrap_err().code,
            "invalid_index"
        );
    }
    fn division(mem: &Mem, need: f64, ratio: f64) -> u64 {
        let d = mem.alloc(0x200);
        mem.u64(d + DIVISION_NEED, fx(need));
        mem.u64(d + DIVISION_RATIO, fx(ratio));
        mem.u64(d + DIVISION_RECEIVED, fx(need * ratio));
        mem.u64(d + DIVISION_COVERED, fx(need * 0.2633));
        d
    }
    #[test]
    fn division_supply_is_consistency_checked() {
        let mem = Mem::new();
        let r = Reader::new(&mem);
        let d = division(&mem, 1.331, 0.94546);
        let s = division_supply(&r, d).unwrap();
        assert_eq!(s.need, 1.331);
        assert_eq!(s.ratio, 0.94546);
        assert!((s.received - 1.25842).abs() < 1e-4);
        // received no longer equals need x ratio: a wrong build or a torn write
        mem.u64(d + DIVISION_RECEIVED, fx(0.5));
        assert_eq!(division_supply(&r, d).unwrap_err().code, "invalid_supply");
        let over = division(&mem, 0.8, 1.0);
        mem.u64(over + DIVISION_RATIO, fx(1.5));
        assert_eq!(division_supply(&r, over).unwrap_err().code, "invalid_supply");
    }
    /// Country supply system with a sparse pool: used 4 of cap 6, one record
    /// sits in slot 7, past the header capacity.
    fn country(mem: &Mem, sys: &Systems, owner: u64, junk_slot: bool) {
        let cs = mem.alloc(0x200);
        mem.u64(cs, IMAGE + COUNTRY_SYSTEM_VTABLE);
        mem.u64(cs + COUNTRY_SYSTEM_OWNER, owner);
        let pool = mem.alloc(NODE_STRIDE * 8);
        mem.u64(cs + COUNTRY_NODES, pool);
        mem.i32(cs + COUNTRY_NODES + 8, 4);
        mem.i32(cs + COUNTRY_NODES + 12, 6);
        let object = mem.alloc(0x40);
        for (slot, id, province, kind) in [
            (0u64, 24u32, 241u32, 3u32),
            (1, 43, 6521, 2),
            (4, 27, 6282, 3),
            (7, 31, 11372, 3),
        ] {
            let n = pool + slot * NODE_STRIDE;
            mem.i32(n + NODE_PROVINCE, province as i32);
            mem.i32(n + NODE_KIND, kind as i32);
            mem.i32(n + NODE_ONE, 1);
            mem.i32(n + NODE_ID, id as i32);
            mem.u64(n + NODE_OBJECT, object);
            mem.i32(n + NODE_LINK, 1);
        }
        if junk_slot {
            // stale slots: kind 7, and the right kind without the live object
            let n = pool + 2 * NODE_STRIDE;
            mem.i32(n + NODE_PROVINCE, 99);
            mem.i32(n + NODE_KIND, 7);
            mem.i32(n + NODE_ONE, 1);
            mem.i32(n + NODE_ID, 5);
            mem.u64(n + NODE_OBJECT, object);
            mem.i32(n + NODE_LINK, 1);
            let n = pool + 3 * NODE_STRIDE;
            mem.i32(n + NODE_PROVINCE, 98);
            mem.i32(n + NODE_KIND, 2);
            mem.i32(n + NODE_ONE, 1);
            mem.i32(n + NODE_ID, 6);
        }
        let list = mem.alloc(8 * 4);
        mem.u64(sys.supply + SYS_COUNTRIES, list);
        mem.i32(sys.supply + SYS_COUNTRIES + 8, 4);
        mem.i32(sys.supply + SYS_COUNTRIES + 12, 4);
        mem.u64(list + 8, cs);
    }
    #[test]
    fn reads_the_sparse_node_pool() {
        let mem = Mem::new();
        let (_, sys) = world(&mem);
        let owner = mem.alloc(0x100);
        country(&mem, &sys, owner, true);
        let r = Reader::new(&mem);
        let nodes = country_nodes(&r, IMAGE, &sys, owner, 1).unwrap();
        assert_eq!(
            nodes,
            vec![
                Node { id: 24, province: 241, kind: NodeKind::Port },
                Node { id: 27, province: 6282, kind: NodeKind::Port },
                Node { id: 31, province: 11372, kind: NodeKind::Port },
                Node { id: 43, province: 6521, kind: NodeKind::Hub },
            ]
        );
        assert_eq!(
            country_nodes(&r, IMAGE, &sys, owner + 8, 1).unwrap_err().code,
            "invalid_supply"
        );
        assert_eq!(
            country_nodes(&r, IMAGE, &sys, owner, 9).unwrap_err().code,
            "invalid_index"
        );
    }
    #[test]
    fn node_pool_must_match_its_used_count() {
        let mem = Mem::new();
        let (_, sys) = world(&mem);
        let owner = mem.alloc(0x100);
        country(&mem, &sys, owner, false);
        let r = Reader::new(&mem);
        let list = r.u64(sys.supply + SYS_COUNTRIES).unwrap();
        let cs = r.u64(list + 8).unwrap();
        mem.i32(cs + COUNTRY_NODES + 8, 5);
        assert_eq!(
            country_nodes(&r, IMAGE, &sys, owner, 1).unwrap_err().code,
            "invalid_supply"
        );
    }
    #[test]
    fn bands_follow_the_game_alert_levels() {
        assert_eq!(band(1.0), "full");
        assert_eq!(band(0.945), "ok");
        assert_eq!(band(0.7), "ok");
        assert_eq!(band(0.69), "low");
        assert_eq!(band(0.5), "low");
        assert_eq!(band(0.49), "very_low");
    }
    #[test]
    fn weighted_mean_uses_need() {
        assert_eq!(weighted_mean(&[]), None);
        assert_eq!(weighted_mean(&[(0.0, 0.1)]), None);
        let m = weighted_mean(&[(1.0, 1.0), (3.0, 0.5)]).unwrap();
        assert!((m - 0.625).abs() < 1e-12);
    }
    #[test]
    fn overdrawn_provinces_worst_first() {
        let mut m = BTreeMap::new();
        let p = |demand, supply| ProvinceSupply {
            demand,
            supply,
            flows: Vec::new(),
        };
        m.insert(1, p(1.0, 5.0));
        m.insert(2, p(12.7, 10.2));
        m.insert(3, p(9.0, 3.0));
        m.insert(4, p(5.0, 5.0));
        assert_eq!(overdrawn(&m), vec![(3, 6.0), (2, 2.5)]);
    }
}
