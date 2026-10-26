//! `hoi4_air`: read-only own air wings and air bases from macOS memory
//! (089EA519). Unlike `hoi4_air_wings` it opens no panel and sends no input.
//!
//! Mapped live on 26 October 2026 (Germany, April 1938). Validated against
//! `history/units/GER_1936_air_bba.txt`: the wing names at each air base match
//! (state 64: Jagdgeschwader 132 and Kampfgeschwader 153; state 763:
//! Jagdgeschwader 232; ...) and base capacity is 200 per air base level for
//! the states whose level could be read from history (50: 2, 51: 3). Mission
//! mask and region are read from other countries' wings (fighters 1 = air
//! superiority, training 2048 = frame 12 - 1 of the mission toolbar); no
//! German wing had a mission to compare; validated against own orders on 26 October 2026.
use super::air::MISSIONS;
use super::fleetmem::{
    self, expect, pointers, require_libcxx, AIR_BY_COUNTRY, GS_AIR_MANAGER,
};
use crate::{
    error::{Error, Result},
    memory::Reader,
    platform::Process,
};
use serde_json::{json, Map, Value};
use std::{collections::BTreeMap, path::Path};

const STRATEGIC_AIR_VTABLE: u64 = 0x331a850;
const SA_BASES: u64 = 0x10;
const BASE_VTABLE: u64 = 0x331a7e8;
const BASE_ID: u64 = 0xc;
const BASE_STATE: u64 = 0x68;
const BASE_PROVINCE: u64 = 0x70;
const BASE_CAPACITY: u64 = 0x78;
const BASE_CONTAINERS: u64 = 0x48;
const CONTAINER_VTABLE: u64 = 0x331a768;
const CONTAINER_POOLS: u64 = 0xb8;
const POOL_VTABLE: u64 = 0x32af020;
const POOL_WINGS: u64 = 0x28;
pub(super) const WING_VTABLE: u64 = 0x32aef58;
pub(super) const WING_NAME: u64 = 0x938;
const WING_UNIT: u64 = 0x320;
const WING_PLANES: u64 = 0x74;
const WING_MANPOWER: u64 = 0x78;
const WING_MISSIONS: u64 = 0x98;
const WING_VARIANTS: u64 = 0x1e8;
/// Destination air base id while the wing flies a transfer, 0 otherwise (JG 132 rebased from state
/// 64 to 66 read base id 10, JG 134 to state 56 read 4, JG 136 to 64 read 9; 0 for resting wings).
const WING_TRANSFER_BASE: u64 = 0x44;
/// `CStrategicRegion` file id (the N of map/strategicregions/N-Name.txt) +0x40; used for the region
/// of an air base (province +0xc8 -> region).
const REGION_FILE: u64 = 0x40;
/// A wing's assigned strategic region: pointer to its `CStrategicRegion` at +0xb8, null while the
/// wing stands by. Set by a mission order and cleared by Hold (read live 26 October 2026: JG 1 held
/// -> null, air superiority over North-West Germany -> pointer, Hold -> null again); the mission mask
/// at +0x98 and the province list at +0xc0 stay stale after a Hold, so only the pointer counts.
const WING_REGION_PTR: u64 = 0xb8;
/// Aircraft capacity of one air base level.
const CAPACITY_PER_LEVEL: i32 = 200;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Wing {
    pub name: String,
    pub unit: String,
    pub equipment: String,
    pub variant: String,
    pub planes: i32,
    pub manpower: i32,
    pub mission_mask: i32,
    /// File id of the assigned strategic region (0: standing by).
    pub region_id: i32,
    pub region_name: Option<String>,
    pub base_id: i32,
    pub base_state: Option<i32>,
    pub transfer_base_id: i32,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Base {
    pub id: i32,
    pub state: Option<i32>,
    pub province: i32,
    pub capacity: i32,
    /// File id of the strategic region the base lies in.
    pub region_id: Option<i32>,
}

fn wing(r: &Reader<'_>, image: u64, address: u64, base: &Base) -> Result<Wing> {
    expect(r, address, WING_VTABLE, image, "Air wing")?;
    let variants = pointers(r, address + WING_VARIANTS, 8)?;
    let (variant, equipment) = match variants.first() {
        Some(v) if *v != 0 => {
            let v = fleetmem::variant(r, image, *v)?;
            (v.name, v.type_key)
        }
        _ => (String::new(), String::new()),
    };
    let (region_id, region_name) = match r.u64(address + WING_REGION_PTR)? {
        0 => (0, None),
        region => {
            let template = r.u64(region + fleetmem::REGION_TEMPLATE)?;
            let name = if template == 0 {
                None
            } else {
                Some(r.string_bounded(template + fleetmem::TEMPLATE_NAME, 128)?)
            };
            (r.i32(region + REGION_FILE)?, name)
        }
    };
    Ok(Wing {
        region_id,
        region_name,
        name: r.string_bounded(address + WING_NAME, 128)?,
        unit: r.string_bounded(address + WING_UNIT, 64)?,
        equipment,
        variant,
        planes: r.i32(address + WING_PLANES)?,
        manpower: r.i32(address + WING_MANPOWER)?,
        mission_mask: r.i32(address + WING_MISSIONS)?,
        base_id: base.id,
        base_state: base.state,
        transfer_base_id: r.i32(address + WING_TRANSFER_BASE)?,
    })
}
/// Own air bases and the wings stationed at each, from `strategic`.
pub(super) fn bases_and_wings(
    r: &Reader<'_>,
    image: u64,
    strategic: u64,
) -> Result<(Vec<Base>, Vec<Wing>)> {
    expect(r, strategic, STRATEGIC_AIR_VTABLE, image, "Strategic air")?;
    let mut bases = Vec::new();
    let mut wings = Vec::new();
    for b in pointers(r, strategic + SA_BASES, 128)? {
        expect(r, b, BASE_VTABLE, image, "Air base")?;
        let state = r.u64(b + BASE_STATE)?;
        let province = r.u64(b + BASE_PROVINCE)?;
        let region_id = if province == 0 {
            None
        } else {
            match r.u64(province + fleetmem::PROVINCE_REGION)? {
                0 => None,
                region => Some(r.i32(region + REGION_FILE)?),
            }
        };
        let base = Base {
            region_id,
            id: r.i32(b + BASE_ID)?,
            state: if state == 0 {
                None
            } else {
                Some(r.i32(state + fleetmem::STATE_ID)?)
            },
            province: if province == 0 {
                0
            } else {
                r.i32(province + fleetmem::PROVINCE_ID)?
            },
            capacity: r.i32(b + BASE_CAPACITY)?,
        };
        for container in pointers(r, b + BASE_CONTAINERS, 4)? {
            expect(r, container, CONTAINER_VTABLE, image, "Air container")?;
            for pool in pointers(r, container + CONTAINER_POOLS, 32)? {
                expect(r, pool, POOL_VTABLE, image, "Air wing pool")?;
                for w in pointers(r, pool + POOL_WINGS, 64)? {
                    wings.push(wing(r, image, w, &base)?);
                }
            }
        }
        bases.push(base);
    }
    Ok((bases, wings))
}

/// Mission names set in `mask` (bit n = toolbar frame n + 1).
pub(super) fn missions_in(mask: i32) -> Vec<&'static str> {
    MISSIONS
        .iter()
        .filter(|(_, frame)| (1..=31).contains(frame) && mask & (1 << (frame - 1)) != 0)
        .map(|(n, _)| *n)
        .collect()
}
/// `N-Name.txt` of map/strategicregions -> (N, Name).
pub(super) fn parse_region_file(name: &str) -> Option<(i32, String)> {
    let stem = name.strip_suffix(".txt")?;
    let (id, label) = stem.split_once('-')?;
    Some((id.parse().ok()?, label.trim().to_string()))
}
pub(super) fn region_names(dir: &Path) -> BTreeMap<i32, String> {
    let mut out = BTreeMap::new();
    if let Ok(entries) = std::fs::read_dir(dir.join("map/strategicregions")) {
        for e in entries.flatten() {
            if let Some((id, name)) = e.file_name().to_str().and_then(parse_region_file) {
                out.insert(id, name);
            }
        }
    }
    out
}

fn view(
    bases: &[Base],
    wings: &[Wing],
    names: &BTreeMap<i32, String>,
    filter: Option<&str>,
) -> Value {
    let wanted = filter.map(str::to_lowercase);
    let wing_rows: Vec<Value> = wings
        .iter()
        .filter(|w| {
            wanted
                .as_ref()
                .is_none_or(|f| w.name.to_lowercase().contains(f))
        })
        .map(|w| {
            json!({"wing":w.name,"unit_type":w.unit,"aircraft_type":w.equipment,"variant":(!w.variant.is_empty()).then_some(&w.variant),
                "aircraft":w.planes,"manpower":w.manpower,
                "air_base":{"base_id":w.base_id,"state_id":w.base_state},
                "transfer_to_base_id":(w.transfer_base_id > 0).then_some(w.transfer_base_id),
                "missions":if w.region_id > 0 { missions_in(w.mission_mask) } else { Vec::new() },
                "standing_by":w.region_id <= 0,"last_mission_selection":missions_in(w.mission_mask),
                "region":(w.region_id > 0).then(|| json!({"id":w.region_id,"name":w.region_name.as_ref().or_else(|| names.get(&w.region_id))}))})
        })
        .collect();
    let base_rows: Vec<Value> = bases
        .iter()
        .map(|b| {
            let here: Vec<&Wing> = wings.iter().filter(|w| w.base_id == b.id).collect();
            let used: i32 = here.iter().map(|w| w.planes).sum();
            json!({"base_id":b.id,"state_id":b.state,"province":b.province,
                "region":b.region_id.map(|id| json!({"id":id,"name":names.get(&id)})),
                "level":b.capacity / CAPACITY_PER_LEVEL,"capacity":b.capacity,
                "aircraft_stationed":used,"free":b.capacity - used,
                "wings":here.iter().map(|w| w.name.as_str()).collect::<Vec<_>>()})
        })
        .collect();
    json!({"wings":wing_rows,"air_bases":base_rows,
        "validated":{"wing_names_and_bases":true,"base_capacity":true,"aircraft_count":"matches the 40/80 aircraft of the 1936 history once reinforced; current values unverified",
            "missions":"mask bits 1 (air superiority) and 2 (CAS) read after own orders, 26 October 2026","region":"pointer at wing +0xb8 (file id +0x40, template name); null while standing by; matched the overview's Area text for Western and Eastern Germany, North-West Germany and Western Poland","level":"capacity / 200, matches states 50 and 51"},
        "unavailable":{"organisation":"wings have none","fuel":"not mapped","range":"see hoi4_designs air_range of the variant",
            "aces":"not mapped","air_superiority_by_region":"not mapped"}})
}

/// `hoi4_air`.
pub(super) fn tool(process: &dyn Process, args: &Map<String, Value>) -> Result<Value> {
    let filter = args.get("wing").and_then(Value::as_str);
    if filter.is_some_and(|f| f.len() > 64 || f.chars().any(char::is_control)) {
        return Err(Error::new("invalid_arguments", "Invalid wing filter"));
    }
    let (bases, wings) = super::ui::read(process, |r, image| {
        require_libcxx(r)?;
        let own = fleetmem::own_strategic(r, image, GS_AIR_MANAGER, AIR_BY_COUNTRY)?;
        bases_and_wings(r, image, own)
    })?;
    let names = process
        .identity()
        .and_then(|i| super::mapdata::game_dir(&i.exe).ok())
        .map(|d| region_names(&d))
        .unwrap_or_default();
    Ok(view(&bases, &wings, &names, filter))
}
pub(super) fn schema() -> Value {
    json!({"name":"hoi4_air","description":"Own air wings and air bases read from game memory (no panel, no input, never pauses). Per wing: name, unit type (light/medium/heavy plane), aircraft type and variant, aircraft count, manpower, air base (id and state), missions set (bitmask decoded with the toolbar order) and assigned strategic region. Per air base: state, level (capacity/200), capacity, aircraft stationed, free capacity and the wings there. Mission mask bits (air superiority 1, CAS 2, interception 4, strategic bombing 8) and the region (pointer at wing +0xb8, null while the wing stands by after Hold) were validated against the Air Forces Overview after own orders on 26 October 2026; the mask keeps the last mission after Hold, so judge by `region`; `transfer_to_base_id` is the destination base id while a wing flies a transfer (arrival moves the base). Orders: hoi4_air_mission, hoi4_air_rebase, hoi4_air_wing_create/disband/split/merge, hoi4_air_hold; hoi4_air_regions groups the same data by region.","inputSchema":{"type":"object","properties":{"wing":{"type":"string","maxLength":64,"description":"Only wings whose name contains this text"}},"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::super::testmem::{Mem, IMAGE};
    use super::*;

    #[test]
    fn decodes_mission_bits_and_region_files() {
        assert_eq!(missions_in(1), vec!["air_superiority"]);
        assert_eq!(missions_in(2048), vec!["training"]);
        assert_eq!(missions_in(0b1010), vec!["cas", "strategic_bombing"]);
        assert!(missions_in(0).is_empty());
        assert_eq!(
            parse_region_file("127-Sahara Desert.txt"),
            Some((127, "Sahara Desert".to_string()))
        );
        assert_eq!(parse_region_file("readme.md"), None);
    }
    #[test]
    fn reads_bases_pools_and_wings() {
        let mem = Mem::new();
        let strategic = mem.alloc(0x200);
        mem.u64(strategic, IMAGE + STRATEGIC_AIR_VTABLE);
        let variant = mem.alloc(0x500);
        mem.u64(variant, IMAGE + fleetmem::VARIANT_VTABLE);
        mem.string(variant + fleetmem::VARIANT_NAME, "Ju 86");
        let kind = mem.alloc(0x300);
        mem.string(kind + fleetmem::TYPE_KEY, "tac_bomber_equipment_0");
        mem.u64(variant + fleetmem::VARIANT_TYPE, kind);
        let w = mem.alloc(0xa00);
        mem.u64(w, IMAGE + WING_VTABLE);
        mem.string(w + WING_NAME, "Kampfgeschwader 153");
        mem.string(w + WING_UNIT, "medium_plane");
        mem.i32(w + WING_PLANES, 70);
        mem.i32(w + WING_MANPOWER, 3080);
        mem.i32(w + WING_MISSIONS, 2);
        let region = mem.alloc(0x100);
        mem.i32(region + REGION_FILE, 127);
        let template = mem.alloc(0x100);
        mem.string(template + fleetmem::TEMPLATE_NAME, "Sahara Desert");
        mem.u64(region + fleetmem::REGION_TEMPLATE, template);
        mem.u64(w + WING_REGION_PTR, region);
        mem.pointer_array(w + WING_VARIANTS, &[variant]);
        let pool = mem.alloc(0x100);
        mem.u64(pool, IMAGE + POOL_VTABLE);
        mem.pointer_array(pool + POOL_WINGS, &[w]);
        let container = mem.alloc(0x200);
        mem.u64(container, IMAGE + CONTAINER_VTABLE);
        mem.pointer_array(container + CONTAINER_POOLS, &[pool]);
        let state = mem.alloc(0x100);
        mem.i32(state + fleetmem::STATE_ID, 64);
        let b = mem.alloc(0x200);
        mem.u64(b, IMAGE + BASE_VTABLE);
        mem.i32(b + BASE_ID, 9);
        mem.u64(b + BASE_STATE, state);
        mem.i32(b + BASE_CAPACITY, 1200);
        mem.pointer_array(b + BASE_CONTAINERS, &[container]);
        mem.pointer_array(strategic + SA_BASES, &[b]);
        let r = Reader::new(&mem);
        let (bases, wings) = bases_and_wings(&r, IMAGE, strategic).unwrap();
        assert_eq!(bases[0].state, Some(64));
        assert_eq!(wings[0].name, "Kampfgeschwader 153");
        assert_eq!(wings[0].equipment, "tac_bomber_equipment_0");
        let mut names = BTreeMap::new();
        names.insert(127, "Sahara Desert".to_string());
        let v = view(&bases, &wings, &names, None);
        assert_eq!(v["wings"][0]["missions"][0], "cas");
        assert_eq!(v["wings"][0]["region"]["name"], "Sahara Desert");
        assert_eq!(v["wings"][0]["region"]["id"], 127);
        assert_eq!(v["air_bases"][0]["level"], 6);
        assert_eq!(v["air_bases"][0]["free"], 1130);
        assert!(view(&bases, &wings, &names, Some("zz"))["wings"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    #[test]
    fn rejects_foreign_objects() {
        let mem = Mem::new();
        let strategic = mem.alloc(0x200);
        let r = Reader::new(&mem);
        assert_eq!(
            bases_and_wings(&r, IMAGE, strategic).unwrap_err().code,
            "invalid_object"
        );
    }
}
