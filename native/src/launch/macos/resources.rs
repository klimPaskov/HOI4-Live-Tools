//! `hoi4_resources`, `hoi4_production_shortages` and `hoi4_industry`: pure
//! memory reads of the own country's strategic resources, fuel, per-line
//! resource allocation and factory counts. No input, no lease, no pause.
//!
//! Provenance (macOS 1.19.3 build 089EA519, found 26 Oct 2026 by RTTI/vtable
//! scans and value correlation; nothing is copied from Linux/Windows):
//! - `country + 0xf80` points to a `CCountryResources` (vtable `0x339faf8`).
//!   Inside it sit ten embedded `CStrategicResourcePool` objects (vtable
//!   `0x331e228`; pool +8 = data pointer into the same object, pool +0x10 =
//!   i32 count 8). An entry is 16 bytes `{i64 amount (x100000), i32 id}`. The
//!   eight ids are the seven strategic resources plus an unused id 357.
//! - resource ids come from `CStrategicResourceDatabase` (global
//!   `base + 0x34d0bd0`, vtable `0x331e370`, pointer array at `+0x28`, i32
//!   count at `+0x30`): entry 0 is a null object, entries 1..7 are
//!   `CStrategicResource` (vtable `0x331e1c8`, id at `+8`) in the order of
//!   `common/resources/00_resources.txt` (oil, aluminium, rubber, tungsten,
//!   steel, chromium, coal).
//! - `country + 0x12e0` points to `CCountryFuelStatus` (vtable `0x32b4e10`):
//!   `+0x8` fuel stock, `+0x10` fuel capacity (i64 x100000).
//! - each production line has a `CProductionResourceCost` at `line + 0x58`:
//!   three `{vtable 0x338be68, CStrategicResource*, i64 amount}` records of
//!   0x20 bytes (null object = unused record).
//!
//! What is validated and what is not is stated in every payload
//! (`validated`/`evidence`); slot semantics other than slot 0 and slot 5 are
//! deliberately exposed raw.
use super::ui::read;
use crate::{
    economy,
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
};
use serde_json::{json, Map, Value};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const FIXED: f64 = 100_000.0;
const COUNTRY_RESOURCES: u64 = 0xf80;
const COUNTRY_FUEL: u64 = 0x12e0;
const RESOURCES_VTABLE: u64 = 0x339faf8;
const FUEL_VTABLE: u64 = 0x32b4e10;
const POOL_VTABLE: u64 = 0x331e228;
const RESOURCE_VTABLE: u64 = 0x331e1c8;
const RESOURCE_DB_GLOBAL: u64 = 0x34d0bd0;
const RESOURCE_DB_VTABLE: u64 = 0x331e370;
const COST_VTABLE: u64 = 0x338be68;
const MILITARY_LINE: u64 = 0x338a030;
const NAVAL_LINE: u64 = 0x338a1b0;
const EQUIPMENT_VARIANT_TYPE: u64 = 0x3e0;
/// Bytes of `CCountryResources` scanned for pools (ten pools end at 0x710).
const OBJECT_SCAN: usize = 0x800;
const POOL_ENTRIES: usize = 8;
/// Game-file order of `common/resources/00_resources.txt`.
const NAMES: [&str; 7] = [
    "oil",
    "aluminium",
    "rubber",
    "tungsten",
    "steel",
    "chromium",
    "coal",
];
const LINE_LIMIT: usize = 256;

/// Pool slots by their order of appearance in the object.
const SLOT_PRODUCED: usize = 0;
const SLOT_AVAILABLE: usize = 3;
const SLOT_NEEDED: usize = 5;
const SLOT_NOTES: &str = "slot0 gross per-day production (matches state resource sums x ~1.9); slot3 (== slot4) is the amount available to the country's production (about 20% of slot0); slot5 negative = daily demand of production lines; slot3+slot5 equals the number the Production view's resource strip shows (validated live, 2 screenshots, 6 resources); slots 6,7 ~80% of slot0 (meaning unidentified); slots 1,2 stayed 0 with no trade deal; slot8 only coal; slot9 zero";

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Pool {
    /// Byte offset of the pool inside `CCountryResources`.
    pub offset: usize,
    /// `(resource id, amount x100000)` per entry.
    pub entries: Vec<(i32, i64)>,
}

/// Find every well-formed pool in an object image. Pure; entries must lie in
/// the same image (`image_base` is the object's address).
pub(super) fn parse_pools(buf: &[u8], image_base: u64, pool_vtable: u64) -> Vec<Pool> {
    let mut pools = Vec::new();
    let mut o = 0usize;
    while o + 0x18 <= buf.len() {
        let vt = u64::from_le_bytes(buf[o..o + 8].try_into().unwrap());
        if vt == pool_vtable {
            let data = u64::from_le_bytes(buf[o + 8..o + 16].try_into().unwrap());
            let count = i32::from_le_bytes(buf[o + 16..o + 20].try_into().unwrap());
            if count as usize == POOL_ENTRIES && data >= image_base {
                let start = (data - image_base) as usize;
                if start + POOL_ENTRIES * 16 <= buf.len() {
                    let entries = (0..POOL_ENTRIES)
                        .map(|k| {
                            let p = start + k * 16;
                            (
                                i32::from_le_bytes(buf[p + 8..p + 12].try_into().unwrap()),
                                i64::from_le_bytes(buf[p..p + 8].try_into().unwrap()),
                            )
                        })
                        .collect();
                    pools.push(Pool { offset: o, entries });
                    o += 0x18;
                    continue;
                }
            }
        }
        o += 8;
    }
    pools
}

fn amount(raw: i64) -> f64 {
    raw as f64 / FIXED
}

/// Daily need against what the production can use. `needed` is positive.
pub(super) fn shortage(needed: f64, available: f64) -> Value {
    let eps = 1e-6;
    json!({
        "short": needed > available + eps,
        "deficit": (needed - available).max(0.0),
    })
}

struct Resources {
    ids: Vec<i32>,
    pools: Vec<Pool>,
    fuel: (i64, i64),
    date_raw: i32,
    player: String,
}

fn rtti_vtable(r: &Reader<'_>, p: u64, image: u64, vtable: u64, what: &str) -> Result<()> {
    if !(0x10000..=0x7fff_ffff_0000).contains(&p) || p % 8 != 0 || r.u64(p)? != image + vtable {
        return Err(Error::new(
            "invalid_type",
            format!("{what}: unexpected object type"),
        ));
    }
    Ok(())
}

fn resource_ids(r: &Reader<'_>, image: u64) -> Result<Vec<i32>> {
    let db = r.u64(image + RESOURCE_DB_GLOBAL)?;
    rtti_vtable(r, db, image, RESOURCE_DB_VTABLE, "resource database")?;
    let header = r.read(db + 0x28, 16)?;
    let ptr = u64::from_le_bytes(header[..8].try_into().unwrap());
    let count = i32::from_le_bytes(header[8..12].try_into().unwrap());
    if count != 9 || !(0x10000..=0x7fff_ffff_0000).contains(&ptr) {
        return Err(Error::new(
            "invalid_array",
            "Resource database shape not reviewed",
        ));
    }
    let raw = r.read(ptr, 8 * count as usize)?;
    let mut ids = Vec::new();
    for chunk in raw.chunks_exact(8).skip(1).take(NAMES.len()) {
        let p = u64::from_le_bytes(chunk.try_into().unwrap());
        rtti_vtable(r, p, image, RESOURCE_VTABLE, "strategic resource")?;
        ids.push(r.i32(p + 8)?);
    }
    Ok(ids)
}

fn snapshot(r: &Reader<'_>, image: u64) -> Result<Resources> {
    let (country, _) = economy::own_production(r, image)?;
    let context = game::context_from_reader(r, image)?;
    let ids = resource_ids(r, image)?;
    let object = r.u64(country + COUNTRY_RESOURCES)?;
    rtti_vtable(r, object, image, RESOURCES_VTABLE, "country resources")?;
    let buf = r.read(object, OBJECT_SCAN)?;
    let pools = parse_pools(&buf, object, image + POOL_VTABLE);
    if pools.len() < 6 {
        return Err(Error::new(
            "invalid_type",
            "Country resource pools not found",
        ));
    }
    let fuel_object = r.u64(country + COUNTRY_FUEL)?;
    rtti_vtable(r, fuel_object, image, FUEL_VTABLE, "country fuel")?;
    let fuel = r.read(fuel_object + 8, 16)?;
    // Torn-read guard: the pool image must not have moved while reading.
    if r.read(object, OBJECT_SCAN)? != buf {
        return Err(Error::new("torn_read", "Country resources changed"));
    }
    Ok(Resources {
        ids,
        pools,
        fuel: (
            i64::from_le_bytes(fuel[..8].try_into().unwrap()),
            i64::from_le_bytes(fuel[8..16].try_into().unwrap()),
        ),
        date_raw: context.date_raw as i32,
        player: context.player,
    })
}

fn slot(pool: &Pool, id: i32) -> Option<f64> {
    pool.entries
        .iter()
        .find(|(i, _)| *i == id)
        .map(|(_, a)| amount(*a))
}

fn per_resource(s: &Resources) -> Map<String, Value> {
    let mut out = Map::new();
    for (name, id) in NAMES.iter().zip(&s.ids) {
        let slots: Vec<Option<f64>> = s.pools.iter().map(|p| slot(p, *id)).collect();
        let at = |i: usize| slots.get(i).copied().flatten().unwrap_or(0.0);
        let produced = at(SLOT_PRODUCED);
        let needed = (-at(SLOT_NEEDED)).max(0.0);
        let available = at(SLOT_AVAILABLE);
        out.insert(
            (*name).to_string(),
            json!({
                "id": id,
                "produced": produced,
                "needed_by_production": needed,
                "available_for_production": available,
                "balance": available - needed,
                "balance_validated_vs_production_strip": true,
                "shortage": shortage(needed, available),
                "imported": null,
                "exported": null,
                "trade_reason": "trade slots not identified; slots 1 and 2 were zero with no trade deal",
                "slots": slots,
                "validated": false,
            }),
        );
    }
    out
}

pub(super) fn resources(process: &dyn Process) -> Result<Value> {
    let started = Instant::now();
    let s = read(process, snapshot)?;
    let (stock, capacity) = (amount(s.fuel.0), amount(s.fuel.1));
    Ok(json!({
        "source": "native own country resources (memory)",
        "owned": true,
        "player": s.player,
        "date_raw": s.date_raw,
        "unit": "units per day (strategic resources), absolute (fuel)",
        "resources": per_resource(&s),
        "fuel": {
            "stock": stock,
            "capacity": capacity,
            "ratio": if capacity > 0.0 { json!(stock / capacity) } else { Value::Null },
            "daily_gain": null,
            "days_left": null,
            "reason": "stock/capacity read from CCountryFuelStatus; gain and burn are not stored (sample twice, about 33 per game day at peace in Germany 1936)",
            "validated": false,
        },
        "slot_notes": SLOT_NOTES,
        "pool_count": s.pools.len(),
        "shortage_penalty_percent": null,
        "shortage_penalty_reason": "no memory field found; the Production view shows 'Lack of resources' per line",
        "validated": false,
        "evidence": "produced vs state resource sums (x1.76-2.0 by infrastructure/supply); needed aluminium == sum of line allocations; resource ids vs database order; fuel rises ~33 per game day",
        "freshness": {"observed_unix_ms": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0},
        "observation_paused_game": false,
        "input_authorized": false
    }))
}

struct Line {
    kind: &'static str,
    equipment: Option<String>,
    factories: i32,
    allocation: Vec<(i32, i64)>,
}

fn lines(r: &Reader<'_>, image: u64) -> Result<Vec<Line>> {
    let (_, production) = economy::own_production(r, image)?;
    let (_, data, count) = r.array_header(production + 0x58)?;
    if count > LINE_LIMIT {
        return Err(Error::new("invalid_array", "Production line count"));
    }
    let table = if count == 0 { vec![] } else { r.read(data, count * 8)? };
    let mut out = Vec::new();
    for chunk in table.chunks_exact(8) {
        let line = u64::from_le_bytes(chunk.try_into().unwrap());
        if line == 0 || !(0x10000..=0x7fff_ffff_0000).contains(&line) {
            continue;
        }
        let vt = r.u64(line)?;
        let kind = if vt == image + MILITARY_LINE {
            "military"
        } else if vt == image + NAVAL_LINE {
            "naval"
        } else {
            continue;
        };
        if r.u64(line + 0x20)? != production {
            continue;
        }
        let factories = r.i32(line + 0x14)?;
        let equipment = (|| -> Result<Option<String>> {
            let variant = r.u64(line + 0x88)?;
            if variant < 0x10000 {
                return Ok(None);
            }
            let ty = r.u64(variant + EQUIPMENT_VARIANT_TYPE)?;
            if ty < 0x10000 {
                return Ok(None);
            }
            Ok(Some(r.string_bounded(ty + 0x10, 128)?))
        })()?;
        let cost = r.u64(line + 0x58)?;
        let mut allocation = Vec::new();
        if (0x10000..=0x7fff_ffff_0000).contains(&cost) {
            let b = r.read(cost, 0x60)?;
            for k in 0..3 {
                let o = k * 0x20;
                let vt = u64::from_le_bytes(b[o..o + 8].try_into().unwrap());
                let res = u64::from_le_bytes(b[o + 8..o + 16].try_into().unwrap());
                if vt != image + COST_VTABLE || res < 0x10000 {
                    continue;
                }
                if r.u64(res)? != image + RESOURCE_VTABLE {
                    continue; // null object: unused record
                }
                allocation.push((
                    r.i32(res + 8)?,
                    i64::from_le_bytes(b[o + 16..o + 24].try_into().unwrap()),
                ));
            }
        }
        out.push(Line {
            kind,
            equipment,
            factories,
            allocation,
        });
    }
    Ok(out)
}

fn shortage_snapshot(r: &Reader<'_>, image: u64) -> Result<(Vec<Line>, Resources)> {
    let lines = lines(r, image)?;
    let s = snapshot(r, image)?;
    Ok((lines, s))
}

pub(super) fn production_shortages(process: &dyn Process) -> Result<Value> {
    let started = Instant::now();
    let (lines, s) = read(process, shortage_snapshot)?;
    let table = per_resource(&s);
    let name_of = |id: i32| {
        NAMES
            .iter()
            .zip(&s.ids)
            .find(|(_, i)| **i == id)
            .map(|(n, _)| *n)
    };
    let short_resources: Vec<&str> = table
        .iter()
        .filter(|(_, v)| v["shortage"]["short"] == json!(true))
        .map(|(k, _)| k.as_str())
        .collect();
    let rows: Vec<Value> = lines
        .iter()
        .map(|l| {
            let alloc: Vec<Value> = l
                .allocation
                .iter()
                .map(|(id, a)| {
                    let name = name_of(*id);
                    let affected = name.is_some_and(|n| short_resources.contains(&n));
                    json!({"resource": name, "allocated_per_day": amount(*a),
                        "resource_in_deficit": affected,
                        "zero_allocation": *a == 0})
                })
                .collect();
            let short: Vec<Value> = alloc
                .iter()
                .filter(|a| a["resource_in_deficit"] == json!(true))
                .map(|a| a["resource"].clone())
                .collect();
            json!({"kind": l.kind, "equipment": l.equipment, "factories": l.factories,
                "resources": alloc, "short_of": short,
                "efficiency_penalty_percent": null,
                "penalty_reason": "not stored in memory; Production view shows 'Lack of resources'"})
        })
        .collect();
    Ok(json!({
        "source": "native production line resource allocation (memory)",
        "lines": rows,
        "resources_in_deficit": short_resources,
        "note": "allocated_per_day is what the engine granted the line at the last daily allocation (new lines show 0 until the next day); a resource is in deficit when daily line demand exceeds what production can use (the Production strip shows it negative).",
        "validated": false,
        "freshness": {"observed_unix_ms": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0},
        "observation_paused_game": false,
        "input_authorized": false
    }))
}

/// Factory counts (validated against the top bar: civilian + military + naval
/// trigger counts equalled the top bar's industrial number, 85, on 26 Oct).
pub(super) fn industry(process: &dyn Process) -> Result<Value> {
    let economy = economy::snapshot(process)?;
    let f = &economy["factory_trigger_count"];
    let total: i64 = ["civilian", "military", "naval"]
        .iter()
        .map(|k| f[*k].as_i64().unwrap_or(0))
        .sum();
    Ok(json!({
        "source": "native own economy trigger counts",
        "factories": {
            "civilian": f["civilian"], "military": f["military"], "dockyards": f["naval"],
            "total": total,
            "civilian_available_for_projects": economy["civilian_factories_available_for_projects"],
            "validated": true,
            "evidence": "total equals the top bar industrial capacity number (85) in Germany 1936 a private run on 26 October 2026"
        },
        "political_power": economy["political_power"],
        "consumer_goods": {"factories": null, "share": null,
            "reason": "no memory field identified; shown in the Production and Construction views"},
        "factory_output_modifier": {"value": null, "reason": "modifier-derived; no memory field identified"},
        "trade_civilian_usage": {"value": null, "reason": "Trade view data not mapped"},
        "stability": {"value": null, "reason": "computed on demand; no stored value found. hoi4_country_status gives the UI-rounded top bar text"},
        "war_support": {"value": null, "reason": "same as stability"},
        "political_power_daily_gain": {"value": null, "reason": "not stored; sample hoi4_economy twice"},
        "freshness": economy["freshness"],
        "observation_paused_game": false,
        "input_authorized": false
    }))
}

pub(super) fn tools() -> Vec<Value> {
    let empty = json!({"type":"object","properties":{},"additionalProperties":false});
    vec![
        json!({"name":"hoi4_resources","description":"Own strategic resources per day from memory: for oil, aluminium, rubber, tungsten, steel, chromium and coal the gross produced, available_for_production, daily need of the production lines, balance, balance (= the Production view strip number) and shortage flag, the raw ten pool slots, plus fuel stock and capacity. Imports/exports and the shortage penalty percent are null with a reason (not identified). Pure read, no lease, never pauses; validated:false (see evidence).","inputSchema":empty}),
        json!({"name":"hoi4_production_shortages","description":"Per production line (military and naval) the resources it was allocated per day and which of them are in deficit for the whole country. The efficiency penalty is null (not in memory). Pure read; validated:false.","inputSchema":empty}),
        json!({"name":"hoi4_industry","description":"Own civilian, military and dockyard factory counts (validated against the top bar total) and political power; consumer goods share, output modifier, trade-civilian usage, exact stability, war support and PP daily gain are null with reasons. Pure read.","inputSchema":empty}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object_with_pool(base: u64, vt: u64, amounts: [i64; 8]) -> Vec<u8> {
        let mut buf = vec![0u8; 0x100];
        buf[0x10..0x18].copy_from_slice(&vt.to_le_bytes());
        buf[0x18..0x20].copy_from_slice(&(base + 0x40).to_le_bytes());
        buf[0x20..0x24].copy_from_slice(&8i32.to_le_bytes());
        for (k, a) in amounts.iter().enumerate() {
            let p = 0x40 + k * 16;
            buf[p..p + 8].copy_from_slice(&a.to_le_bytes());
            buf[p + 8..p + 12].copy_from_slice(&(15000 + k as i32).to_le_bytes());
        }
        buf
    }

    #[test]
    fn pool_parse_finds_embedded_pool_and_entries() {
        let buf = object_with_pool(0x5000, 0x1000, [0, 400_000, 15_300_000, 0, 0, 0, 0, -100_000]);
        let pools = parse_pools(&buf, 0x5000, 0x1000);
        assert_eq!(pools.len(), 1);
        assert_eq!(pools[0].offset, 0x10);
        assert_eq!(pools[0].entries[1], (15001, 400_000));
        assert_eq!(amount(pools[0].entries[7].1), -1.0);
    }

    #[test]
    fn pool_parse_rejects_wrong_vtable_count_and_out_of_object_data() {
        let mut buf = object_with_pool(0x5000, 0x2000, [0; 8]);
        assert!(parse_pools(&buf, 0x5000, 0x1000).is_empty());
        buf = object_with_pool(0x5000, 0x1000, [0; 8]);
        buf[0x20] = 7;
        assert!(parse_pools(&buf, 0x5000, 0x1000).is_empty());
        buf = object_with_pool(0x5000, 0x1000, [0; 8]);
        buf[0x18..0x20].copy_from_slice(&0x9000u64.to_le_bytes());
        assert!(parse_pools(&buf, 0x5000, 0x1000).is_empty());
    }

    #[test]
    fn shortage_flags() {
        let s = shortage(9.0, 0.0);
        assert_eq!(s["short"], json!(true));
        assert_eq!(s["deficit"], json!(9.0));
        let s = shortage(74.0, 61.0);
        assert_eq!(s["short"], json!(true));
        assert_eq!(s["deficit"], json!(13.0));
        let s = shortage(18.0, 31.0);
        assert_eq!(s["short"], json!(false));
    }

    #[test]
    fn resources_table_names_and_balance() {
        let ids = vec![10, 11, 12, 13, 14, 15, 16];
        let mk = |produced: i64, needed: i64| Pool {
            offset: 0,
            entries: ids
                .iter()
                .map(|i| (*i, if *i == 12 { produced } else { 0 }))
                .chain([(357, 0)])
                .collect::<Vec<_>>()
                .into_iter()
                .map(|(i, a)| (i, if i == 12 { needed } else { a }))
                .collect(),
        };
        let mut pools: Vec<Pool> = (0..10).map(|_| mk(0, 0)).collect();
        pools[SLOT_PRODUCED] = mk(0, 0);
        pools[SLOT_NEEDED] = mk(0, -900_000);
        let s = Resources {
            ids,
            pools,
            fuel: (11_400_000_000, 12_468_224_000),
            date_raw: 0,
            player: "GER".into(),
        };
        let t = per_resource(&s);
        assert_eq!(t["rubber"]["needed_by_production"], json!(9.0));
        assert_eq!(t["rubber"]["shortage"]["short"], json!(true));
        assert_eq!(t["rubber"]["balance"], json!(-9.0));
        assert_eq!(t["oil"]["shortage"]["short"], json!(false));
        assert_eq!(t.len(), 7);
    }

    #[test]
    fn tools_are_parameterless_reads() {
        for t in tools() {
            assert!(t["inputSchema"]["properties"].as_object().unwrap().is_empty());
        }
    }
}
