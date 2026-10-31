//! Shared read-only decoding of macOS 089EA519 objects used by the naval, air
//! and design reads: equipment variants/types/modules, provinces and strategic
//! regions. Every offset was mapped live on 26 October 2026 (Germany, April
//! 1938) and checked against vanilla files; see docs/PLAYBOOK_TOOL_GAPS.md.
//! Nothing here writes memory or touches the UI.
use crate::{
    error::{Error, Result},
    game,
    memory::{Reader, StringAbi},
};
use serde_json::{json, Map, Value};

/// Equipment variant (`CEquipmentVariant`, vtable 0x357d1b8).
pub(super) const VARIANT_VTABLE: u64 = 0x357d1b8;
pub(super) const VARIANT_ID: u64 = 0xc;
pub(super) const VARIANT_NAME: u64 = 0x28;
pub(super) const VARIANT_CLASS_SUFFIX: u64 = 0x40;
pub(super) const VARIANT_MODULES: u64 = 0xa8;
pub(super) const VARIANT_TYPE: u64 = 0x3e0;
pub(super) const VARIANT_PARENT: u64 = 0x3e8;
pub(super) const VARIANT_MANPOWER: u64 = 0x410;
pub(super) const VARIANT_OBSOLETE: u64 = 0x414;
pub(super) const VARIANT_MAX_STRENGTH: u64 = 0x2d8;
/// `CEquipmentType` (vtable 0x357bad8): key string +0x10, module slots +0x60.
pub(super) const TYPE_KEY: u64 = 0x10;
pub(super) const TYPE_SLOTS: u64 = 0x60;
pub(super) const SLOT_STRIDE: u64 = 0x40;
pub(super) const SLOT_ID: u64 = 0x8;
pub(super) const SLOT_NAME: u64 = 0x28;
/// `CEquipmentModule`: key string +0x10.
pub(super) const MODULE_KEY: u64 = 0x10;
/// `CProvince`: id +0x9c, state +0xc0 (state id +0x40), strategic region +0xc8.
pub(super) const PROVINCE_ID: u64 = 0x9c;
pub(super) const PROVINCE_STATE: u64 = 0xc0;
pub(super) const PROVINCE_REGION: u64 = 0xc8;
pub(super) const STATE_ID: u64 = 0x40;
/// `CStrategicRegion` +0x20 template: display name +0x20, key +0x38.
pub(super) const REGION_TEMPLATE: u64 = 0x20;
pub(super) const TEMPLATE_NAME: u64 = 0x20;
pub(super) const TEMPLATE_KEY: u64 = 0x38;

/// Variant stat offsets (fixed point /100000). Derived as the equipment type's
/// stat offsets minus 0x30; 168 default-variant values matched the vanilla
/// equipment files, the remaining 27 differ by modules and country bonuses.
pub(super) const STATS: [(&str, u64); 33] = [
    ("defense", 0xf8),
    ("breakthrough", 0x100),
    ("hardness", 0x108),
    ("soft_attack", 0x110),
    ("hard_attack", 0x118),
    ("surface_detection", 0x188),
    ("sub_detection", 0x190),
    ("surface_visibility", 0x198),
    ("sub_visibility", 0x1a0),
    ("lg_attack", 0x1b8),
    ("lg_armor_piercing", 0x1c0),
    ("hg_attack", 0x1c8),
    ("hg_armor_piercing", 0x1d0),
    ("torpedo_attack", 0x1d8),
    ("sub_attack", 0x1e0),
    ("anti_air_attack", 0x1e8),
    ("naval_speed", 0x1f8),
    ("naval_range", 0x200),
    ("air_range", 0x260),
    ("air_defence", 0x268),
    ("air_attack", 0x270),
    ("air_agility", 0x278),
    ("air_bombing", 0x280),
    ("naval_strike_attack", 0x290),
    ("naval_strike_targetting", 0x298),
    ("air_ground_attack", 0x2a0),
    ("max_strength", 0x2d8),
    ("maximum_speed", 0x2e0),
    ("armor_value", 0x2e8),
    ("ap_attack", 0x2f0),
    ("reliability", 0x2f8),
    ("fuel_consumption", 0x318),
    ("build_cost_ic", 0x358),
];

pub(super) fn require_libcxx(r: &Reader<'_>) -> Result<()> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(()),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "Naval, air and design reads are reviewed for the macOS build only",
        )),
    }
}
fn fixed(raw: i32) -> f64 {
    f64::from(raw) / 100_000.0
}
/// Round a fixed-point value for output (3 decimals).
pub(super) fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}
pub(super) fn fixed_at(r: &Reader<'_>, address: u64) -> Result<f64> {
    Ok(round3(fixed(r.i32(address)?)))
}
pub(super) fn is_object(r: &Reader<'_>, address: u64, vtable: u64, image: u64) -> Result<bool> {
    Ok(address != 0 && r.u64(address)? == image + vtable)
}
pub(super) fn expect(
    r: &Reader<'_>,
    address: u64,
    vtable: u64,
    image: u64,
    what: &str,
) -> Result<()> {
    if is_object(r, address, vtable, image)? {
        Ok(())
    } else {
        Err(Error::new(
            "invalid_object",
            format!("{what} has an unexpected type"),
        ))
    }
}
/// Pointer array {data, capacity, size} (bounded).
pub(super) fn pointers(r: &Reader<'_>, header: u64, limit: usize) -> Result<Vec<u64>> {
    let (_, _, count) = r.array_header(header)?;
    if count > limit {
        return Err(Error::new("invalid_array", "Collection exceeds bound"));
    }
    r.pointer_array(header)
}

/// What the design reads need from one variant.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Variant {
    pub address: u64,
    pub id: i32,
    pub name: String,
    pub class_suffix: String,
    pub type_key: String,
    pub parent_id: Option<i32>,
    pub manpower: i32,
    pub max_strength: f64,
    /// Decommissioned (marked outdated equipment): variant +0x414, 0/1. Flipped by the
    /// Production list's Decommission button on 26 October 2026 (0 -> 1, back to 0 by Recommission).
    pub obsolete: bool,
}
pub(super) fn variant(r: &Reader<'_>, image: u64, address: u64) -> Result<Variant> {
    expect(r, address, VARIANT_VTABLE, image, "Equipment variant")?;
    let kind = r.u64(address + VARIANT_TYPE)?;
    let type_key = if kind == 0 {
        String::new()
    } else {
        r.string_bounded(kind + TYPE_KEY, 128)?
    };
    let parent = r.u64(address + VARIANT_PARENT)?;
    let parent_id = if parent == 0 {
        None
    } else {
        Some(r.i32(parent + VARIANT_ID)?)
    };
    Ok(Variant {
        address,
        id: r.i32(address + VARIANT_ID)?,
        name: r.string_bounded(address + VARIANT_NAME, 128)?,
        class_suffix: r.string_bounded(address + VARIANT_CLASS_SUFFIX, 128)?,
        type_key,
        parent_id,
        manpower: r.i32(address + VARIANT_MANPOWER)?,
        max_strength: fixed_at(r, address + VARIANT_MAX_STRENGTH)?,
        obsolete: r.i32(address + VARIANT_OBSOLETE)? != 0,
    })
}
/// Non-zero stats of a variant keyed by stat name.
pub(super) fn stats(r: &Reader<'_>, variant: u64) -> Result<Map<String, Value>> {
    let mut out = Map::new();
    for (name, offset) in STATS {
        let value = fixed_at(r, variant + offset)?;
        if value != 0.0 {
            out.insert(name.to_string(), json!(value));
        }
    }
    Ok(out)
}
/// (slot name, module key) pairs of a variant's chosen modules.
pub(super) fn modules(r: &Reader<'_>, variant: u64) -> Result<Vec<(String, String)>> {
    let (_, data, count) = r.array_header(variant + VARIANT_MODULES)?;
    if count > 64 {
        return Err(Error::new("invalid_array", "Module list exceeds bound"));
    }
    let kind = r.u64(variant + VARIANT_TYPE)?;
    let (_, slots, slot_count) = if kind == 0 {
        (Vec::new(), 0, 0)
    } else {
        r.array_header(kind + TYPE_SLOTS)?
    };
    if slot_count > 64 {
        return Err(Error::new("invalid_array", "Slot list exceeds bound"));
    }
    let mut table = Vec::new();
    for index in 0..slot_count as u64 {
        let slot = slots + index * SLOT_STRIDE;
        table.push((
            r.i32(slot + SLOT_ID)?,
            r.string_bounded(slot + SLOT_NAME, 128)?,
        ));
    }
    let mut out = Vec::new();
    for index in 0..count as u64 {
        let entry = r.read(data + index * 16, 16)?;
        let slot_id = i32::from_le_bytes(entry[..4].try_into().unwrap());
        let module = u64::from_le_bytes(entry[8..16].try_into().unwrap());
        let slot = table
            .iter()
            .find(|(id, _)| *id == slot_id)
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| format!("slot_{slot_id}"));
        let key = if module == 0 {
            String::new()
        } else {
            r.string_bounded(module + MODULE_KEY, 128)?
        };
        out.push((slot, key));
    }
    Ok(out)
}

/// A province: id, owning state id (land) and strategic region.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Place {
    pub province: i32,
    pub state: Option<i32>,
    pub region: Option<(String, String)>,
}
pub(super) fn place(r: &Reader<'_>, province: u64) -> Result<Option<Place>> {
    if province == 0 {
        return Ok(None);
    }
    let state = r.u64(province + PROVINCE_STATE)?;
    let region = r.u64(province + PROVINCE_REGION)?;
    Ok(Some(Place {
        province: r.i32(province + PROVINCE_ID)?,
        state: if state == 0 {
            None
        } else {
            Some(r.i32(state + STATE_ID)?)
        },
        region: if region == 0 {
            None
        } else {
            let template = r.u64(region + REGION_TEMPLATE)?;
            if template == 0 {
                None
            } else {
                Some((
                    r.string_bounded(template + TEMPLATE_KEY, 64)?,
                    r.string_bounded(template + TEMPLATE_NAME, 128)?,
                ))
            }
        },
    }))
}
pub(super) fn place_json(place: &Option<Place>) -> Value {
    match place {
        None => Value::Null,
        Some(p) => json!({"province":p.province,"state_id":p.state,
            "kind":if p.state.is_some(){"land_or_port"}else{"sea"},
            "region":p.region.as_ref().map(|(_, n)| n),"region_key":p.region.as_ref().map(|(k, _)| k)}),
    }
}

/// Image-relative GS offsets of the per-country strategic managers.
pub(super) const GS_AIR_MANAGER: u64 = 0x630;
pub(super) const GS_NAVY_MANAGER: u64 = 0x638;
/// Per-country-index pointer table inside both managers.
pub(super) const AIR_BY_COUNTRY: u64 = 0x30;
pub(super) const NAVY_BY_COUNTRY: u64 = 0x8;
/// The own entry of a strategic manager's per-country table: GS tag -> country
/// index -> pointer (index 1 for the German campaign).
pub(super) fn own_strategic(
    r: &Reader<'_>,
    image: u64,
    manager_field: u64,
    table_field: u64,
) -> Result<u64> {
    let g = game::layout(r);
    let gs = r.u64(image + g.gs_global)?;
    let tag = r.i32(gs + g.player_tag)?;
    let (_, index_data, index_count) = r.array_header(gs + g.tag_country_index)?;
    if tag < 1 || tag as usize >= index_count {
        return Err(Error::new("player_unavailable", "Own tag outside the table"));
    }
    let index = r.i32(index_data + 4 * tag as u64)?;
    let manager = r.u64(gs + manager_field)?;
    let (_, table, count) = r.array_header(manager + table_field)?;
    if index < 0 || index as usize >= count {
        return Err(Error::new("player_unavailable", "Own strategic slot absent"));
    }
    let own = r.u64(table + 8 * index as u64)?;
    if own == 0 {
        return Err(Error::new("player_unavailable", "Own strategic slot empty"));
    }
    Ok(own)
}

/// (slot id, slot name) table of a variant's equipment type (module slots; empty for
/// equipment without modules).
pub(super) fn type_slots(r: &Reader<'_>, variant: u64) -> Result<Vec<(i32, String)>> {
    let kind = r.u64(variant + VARIANT_TYPE)?;
    if kind == 0 {
        return Ok(Vec::new());
    }
    let (_, slots, count) = r.array_header(kind + TYPE_SLOTS)?;
    if count > 64 {
        return Err(Error::new("invalid_array", "Slot list exceeds bound"));
    }
    let mut out = Vec::new();
    for index in 0..count as u64 {
        let slot = slots + index * SLOT_STRIDE;
        out.push((r.i32(slot + SLOT_ID)?, r.string_bounded(slot + SLOT_NAME, 128)?));
    }
    Ok(out)
}

/// Upgrade levels of a variant (equipment without modules): array +0x60 of 16-byte entries
/// `{CEquipmentUpgrade*, i32 level}`, upgrade key (e.g. `tank_gun_upgrade`) at +0x10. Mapped
/// live on 26 October 2026: a variant created with Main Gun +1 held `tank_gun_upgrade` 1 and
/// three zero entries.
pub(super) const VARIANT_UPGRADES: u64 = 0x60;
pub(super) fn upgrades(r: &Reader<'_>, variant: u64) -> Result<Vec<(String, i32)>> {
    let (_, data, count) = r.array_header(variant + VARIANT_UPGRADES)?;
    if count > 32 {
        return Err(Error::new("invalid_array", "Upgrade list exceeds bound"));
    }
    let mut out = Vec::new();
    for index in 0..count as u64 {
        let entry = r.read(data + index * 16, 16)?;
        let upgrade = u64::from_le_bytes(entry[..8].try_into().unwrap());
        let level = i32::from_le_bytes(entry[8..12].try_into().unwrap());
        if upgrade == 0 {
            continue;
        }
        out.push((r.string_bounded(upgrade + 0x10, 96)?, level));
    }
    Ok(out)
}
