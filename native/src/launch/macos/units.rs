//! Read-only unit-level data of the own country on macOS: division
//! templates (columns, support companies), the sub-unit definitions they
//! use, the equipment each division holds, and the pure stat arithmetic over
//! them. Shared by `hoi4_template_get` and `hoi4_supply`.
//!
//! Every offset below was found on 26 October 2026 on the Mach-O 089EA519
//! build (Germany, 1938, paused, 38 deployed divisions) and checked three
//! ways, see the field notes on each constant:
//! * sub-unit definition stats against `common/units/*.txt`: 99 definitions
//!   in memory, each offset won the vote of every numeric attribute it is
//!   claimed for (for example `combat_width` 35 of 35 units);
//! * template composition against `history/units/GER_1936.txt` (column `x`
//!   is index `x*5+y` of the 5x5 grid, support slot `y` of column 0);
//! * live divisions against template arithmetic: hit points and manpower of
//!   all 38 divisions equal the template sums exactly; organisation equals the
//!   template mean times one common factor per country (the organisation
//!   modifier), which is reported rather than hidden.
//!
//! Nothing is copied from the Linux or Windows builds.
use super::ui::read;
use crate::{
    error::{Error, Result},
    game,
    memory::{Reader, StringAbi},
    platform::Process,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Fixed point scale of every stored quantity.
const FIXED: f64 = 100_000.0;

pub(super) struct Layout {
    country_divisions: u64,
    country_templates: u64,
    division_vtable: u64,
    division_handle: u64,
    division_group: u64,
    group_name: u64,
    division_location: u64,
    division_strength: u64,
    division_organisation: u64,
    division_manpower: u64,
    division_template: u64,
    /// Equipment held: array of `{type*, u32 slot, u32 flag, i64 amount}`.
    division_pool: u64,
    /// Variants behind it: array of `{variant*, i64 amount}`.
    division_variants: u64,
    template_vtable: u64,
    template_name: u64,
    template_grid: u64,
    template_support: u64,
    subunit_vtable: u64,
    subunit_key: u64,
    equipment_type_vtable: u64,
    equipment_key: u64,
    variant_vtable: u64,
    variant_type: u64,
    pub(super) country_characters: u64,
    characters_army_leaders: u64,
    leader_vtable: u64,
    leader_id: u64,
    leader_name: u64,
    leader_template: u64,
    leader_template_stats: u64,
    leader_template_traits: u64,
    status_offset: u64,
    status_buildings: u64,
    building_template: u64,
    building_level: u64,
}
/// macOS Mach-O 089EA519 (libc++), mapped live on 26 October 2026.
const MACOS: Layout = Layout {
    country_divisions: 0x250,
    country_templates: 0x180,
    division_vtable: 0x32af5a8,
    division_handle: 0x18,
    division_group: 0xc0,
    group_name: 0x160,
    division_location: 0x1f0,
    division_strength: 0x418,
    division_organisation: 0x420,
    division_manpower: 0x4e8,
    division_template: 0x3b0,
    division_pool: 0x348,
    division_variants: 0x360,
    template_vtable: 0x32fd840,
    template_name: 0x20,
    template_grid: 0x58,
    template_support: 0x78,
    subunit_vtable: 0x331e6e8,
    subunit_key: 0x10,
    equipment_type_vtable: 0x3379328,
    equipment_key: 0x10,
    variant_vtable: 0x337aac8,
    variant_type: 0x3e0,
    // CCountry +0xd98 -> CCountryCharacters (RTTI 18CCountryCharacters);
    // its +0x70 array holds every CArmyLeader of the country (36 for Germany
    // in 1938, the same names the army panel's commander list shows).
    country_characters: 0xd98,
    characters_army_leaders: 0x70,
    leader_vtable: 0x33ce6b8,
    leader_id: 0x20,
    leader_name: 0x38,
    leader_template: 0x18,
    // CUnitLeaderTemplate: i32 legacy_id, skill, attack, defense, planning,
    // logistics from +0x20 (Rundstedt 1/4/4/3/4/2 = GER.txt skill 4, attack 4,
    // defense 3, planning 4, logistics 2); trait keys: array at +0x8, stride
    // 0x20 libc++ strings.
    leader_template_stats: 0x20,
    leader_template_traits: 0x8,
    status_offset: 0x110,
    // CBuildingStatus +0x38 (state +0x148): array of the state's CBuilding
    // objects; building +0x1b0 template (key +0x228), level = low u16 of +0x40.
    status_buildings: 0x38,
    building_template: 0x1b0,
    building_level: 0x40,
};
/// Sub-unit definition fields (fixed point unless noted).
struct SubOffsets {
    width: u64,
    hp: u64,
    org: u64,
    morale: u64,
    supply: u64,
    suppression: u64,
    weight: u64,
    soft: u64,
    hard: u64,
    defense: u64,
    breakthrough: u64,
    armor: u64,
    piercing: u64,
    air: u64,
    recon: u64,
    manpower: u64,
    training: u64,
    need: u64,
}
const SUB: SubOffsets = SubOffsets {
    width: 0x48,
    hp: 0x270,
    org: 0x268,
    morale: 0x88,
    supply: 0xe0,
    suppression: 0xe8,
    weight: 0x2a0,
    soft: 0xa8,
    hard: 0xb0,
    defense: 0x90,
    breakthrough: 0x98,
    armor: 0x280,
    piercing: 0x288,
    air: 0x208,
    recon: 0xb8,
    manpower: 0x558,
    training: 0x568,
    need: 0x300,
};
/// Equipment variant stat fields (fixed point). Mapped against
/// infantry_equipment_1 (22/3/6/1/4/0.9), artillery_equipment_1,
/// light_tank_equipment_2 (def 5, bt 36, soft 16, hard 6, speed 12, armor 15,
/// ap 30) and anti_air_equipment_1 (air 19).
const VARIANT_STATS: &[(&str, u64)] = &[
    ("defense", 0xf8),
    ("breakthrough", 0x100),
    ("hardness", 0x108),
    ("soft_attack", 0x110),
    ("hard_attack", 0x118),
    ("air_attack", 0x270),
    ("maximum_speed", 0x2e0),
    ("armor_value", 0x2e8),
    ("piercing", 0x2f0),
    ("reliability", 0x2f8),
    ("fuel_consumption", 0x318),
    ("build_cost_ic", 0x3c0),
];

pub(super) fn layout(r: &Reader<'_>) -> Result<&'static Layout> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(&MACOS),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "Unit reads are not reviewed for this build",
        )),
    }
}
fn fixed(r: &Reader<'_>, a: u64) -> Result<f64> {
    Ok(i64::from_le_bytes(r.read(a, 8)?.try_into().unwrap()) as f64 / FIXED)
}
fn is_ptr(p: u64) -> bool {
    (0x10000..=0x7fff_ffff_0000).contains(&p) && p % 8 == 0
}
fn exact(r: &Reader<'_>, image: u64, p: u64, vtable: u64, what: &str) -> Result<()> {
    if !is_ptr(p) || r.u64(p)? != image + vtable {
        return Err(Error::new(
            "invalid_type",
            format!("Exact {what} object required"),
        ));
    }
    Ok(())
}
pub(super) fn own_country(r: &Reader<'_>, image: u64) -> Result<u64> {
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

/// Combat sub-unit or support company definition.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SubUnit {
    pub key: String,
    pub width: f64,
    pub hp: f64,
    pub org: f64,
    pub morale: f64,
    pub supply: f64,
    pub weight: f64,
    pub manpower: f64,
    pub training_days: f64,
    /// Sub-unit own stat modifiers (the `soft_attack = -0.5` style values of
    /// `common/units`); equipment stats come separately from the variants.
    pub unit_stats: BTreeMap<&'static str, f64>,
    /// Equipment archetype key -> pieces needed.
    pub need: Vec<(String, f64)>,
}
/// Equipment variant stats (per piece / per equipped battalion).
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Variant {
    pub name: String,
    pub stats: BTreeMap<&'static str, f64>,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Template {
    pub address: u64,
    pub name: String,
    /// `columns[x][y]` = sub-unit key of the regiment at column x row y.
    pub columns: Vec<Vec<Option<String>>>,
    pub support: Vec<String>,
    pub subunits: Vec<SubUnit>,
    pub support_units: Vec<SubUnit>,
}
pub(super) fn read_subunit(r: &Reader<'_>, image: u64, p: u64) -> Result<SubUnit> {
    let l = layout(r)?;
    exact(r, image, p, l.subunit_vtable, "sub-unit definition")?;
    let o = &SUB;
    let (header, data, count) = r.array_header(p + o.need)?;
    let _ = header;
    if count > 8 {
        return Err(Error::new("invalid_array", "Equipment need list too long"));
    }
    let mut need = Vec::new();
    for i in 0..count as u64 {
        let ty = r.u64(data + i * 16)?;
        exact(r, image, ty, l.equipment_type_vtable, "equipment type")?;
        need.push((
            r.string_bounded(ty + l.equipment_key, 128)?,
            fixed(r, data + i * 16 + 8)?,
        ));
    }
    let mut unit_stats = BTreeMap::new();
    for (name, off) in [
        ("soft_attack", o.soft),
        ("hard_attack", o.hard),
        ("defense", o.defense),
        ("breakthrough", o.breakthrough),
        ("armor_value", o.armor),
        ("piercing", o.piercing),
        ("air_attack", o.air),
        ("recon", o.recon),
        ("suppression", o.suppression),
    ] {
        let v = fixed(r, p + off)?;
        if v != 0.0 {
            unit_stats.insert(name, v);
        }
    }
    Ok(SubUnit {
        key: r.string_bounded(p + l.subunit_key, 64)?,
        width: fixed(r, p + o.width)?,
        hp: fixed(r, p + o.hp)?,
        org: fixed(r, p + o.org)?,
        morale: fixed(r, p + o.morale)?,
        supply: fixed(r, p + o.supply)?,
        weight: fixed(r, p + o.weight)?,
        manpower: r.i32(p + o.manpower)? as f64,
        training_days: r.i32(p + o.training)? as f64,
        unit_stats,
        need,
    })
}
/// Own templates in list order.
pub(super) fn templates(r: &Reader<'_>, image: u64) -> Result<Vec<Template>> {
    let l = layout(r)?;
    let country = own_country(r, image)?;
    let mut cache: BTreeMap<u64, SubUnit> = BTreeMap::new();
    let mut out = Vec::new();
    for t in r.pointer_array(country + l.country_templates)? {
        if out.len() >= 128 {
            return Err(Error::new("invalid_array", "Too many templates"));
        }
        exact(r, image, t, l.template_vtable, "division template")?;
        let mut load = |p: u64| -> Result<Option<SubUnit>> {
            if p == 0 || !is_ptr(p) || r.u64(p)? != image + l.subunit_vtable {
                return Ok(None); // empty cell (null object)
            }
            if let Some(s) = cache.get(&p) {
                return Ok(Some(s.clone()));
            }
            let s = read_subunit(r, image, p)?;
            cache.insert(p, s.clone());
            Ok(Some(s))
        };
        let grid = r.pointer_array(t + l.template_grid)?;
        let mut columns: Vec<Vec<Option<String>>> = Vec::new();
        let mut subunits = Vec::new();
        for (i, cell) in grid.iter().enumerate().take(25) {
            let (x, y) = (i / 5, i % 5);
            let unit = load(*cell)?;
            while columns.len() <= x {
                columns.push(vec![None; 5]);
            }
            if let Some(u) = unit {
                columns[x][y] = Some(u.key.clone());
                subunits.push(u);
            }
        }
        let mut support = Vec::new();
        let mut support_units = Vec::new();
        for cell in r.pointer_array(t + l.template_support)?.into_iter().take(5) {
            if let Some(u) = load(cell)? {
                support.push(u.key.clone());
                support_units.push(u);
            }
        }
        out.push(Template {
            address: t,
            name: r.string_bounded(t + l.template_name, 128)?,
            columns,
            support,
            subunits,
            support_units,
        });
    }
    Ok(out)
}

/// Template totals from the definitions (no modifiers).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Totals {
    pub width: f64,
    pub hp: f64,
    pub org: f64,
    pub manpower: f64,
    pub supply_use: f64,
    pub weight: f64,
    pub training_days: f64,
    pub battalions: usize,
    pub support_companies: usize,
    pub need: BTreeMap<String, f64>,
}
/// Division arithmetic of the game as verified against live divisions:
/// hit points and manpower are sums over every battalion and company,
/// organisation is the plain mean of their maximum organisation, width counts
/// the combat battalions (companies have width 0), supply use sums, training
/// time is the longest.
pub(super) fn totals(subunits: &[SubUnit], support: &[SubUnit]) -> Totals {
    let all: Vec<&SubUnit> = subunits.iter().chain(support.iter()).collect();
    let n = all.len();
    let mut need: BTreeMap<String, f64> = BTreeMap::new();
    for u in &all {
        for (k, a) in &u.need {
            *need.entry(k.clone()).or_default() += a;
        }
    }
    Totals {
        width: all.iter().map(|u| u.width).sum(),
        hp: all.iter().map(|u| u.hp).sum(),
        org: if n == 0 {
            0.0
        } else {
            all.iter().map(|u| u.org).sum::<f64>() / n as f64
        },
        manpower: all.iter().map(|u| u.manpower).sum(),
        supply_use: all.iter().map(|u| u.supply).sum(),
        weight: all.iter().map(|u| u.weight).sum(),
        training_days: all.iter().map(|u| u.training_days).fold(0.0, f64::max),
        battalions: subunits.len(),
        support_companies: support.len(),
        need,
    }
}
pub(super) fn round(v: f64, digits: i32) -> f64 {
    let m = 10f64.powi(digits);
    (v * m).round() / m
}
impl Totals {
    pub(super) fn to_json(&self) -> Value {
        json!({"width":round(self.width,2),"hp":round(self.hp,2),
            "organisation_base":round(self.org,2),"manpower":self.manpower,
            "supply_use_base":round(self.supply_use,3),"weight":round(self.weight,2),
            "training_days":self.training_days,"battalions":self.battalions,
            "support_companies":self.support_companies,
            "equipment_need":self.need.iter().map(|(k,v)|(k.clone(),json!(round(*v,2)))).collect::<serde_json::Map<_,_>>()})
    }
}

/// Estimate of the equipment-derived stats: every battalion and company takes
/// its stats from the variant of the first (main) equipment it needs, fully
/// equipped, and the stats add up. Not modifier-exact and not screen-checked:
/// callers report it with `validated:false`.
pub(super) fn equipment_stat_estimate(
    units: &[&SubUnit],
    variants: &BTreeMap<String, Variant>,
) -> BTreeMap<&'static str, f64> {
    let mut sum: BTreeMap<&'static str, f64> = BTreeMap::new();
    for u in units {
        let Some((kind, _)) = u.need.first() else {
            continue;
        };
        let Some(v) = variants.get(kind) else {
            continue;
        };
        for (name, value) in &v.stats {
            if matches!(
                *name,
                "defense"
                    | "breakthrough"
                    | "soft_attack"
                    | "hard_attack"
                    | "air_attack"
                    | "armor_value"
                    | "piercing"
            ) {
                *sum.entry(*name).or_default() += value;
            }
        }
    }
    sum
}

/// Division handle text id as `hoi4_armies` prints it.
pub(super) fn division_id(r: &Reader<'_>, division: u64, offset: u64) -> Result<String> {
    let handle = r.read(division + offset, 8)?;
    Ok(format!(
        "{:08x}:{:08x}",
        u32::from_le_bytes(handle[..4].try_into().unwrap()),
        u32::from_le_bytes(handle[4..].try_into().unwrap())
    ))
}
pub(super) struct DivisionRead {
    pub id: String,
    pub address: u64,
    pub template: u64,
    pub army: Option<String>,
    pub province: Option<i32>,
    pub state: Option<i32>,
    pub hp: f64,
    pub org: f64,
    pub manpower: f64,
    /// Held equipment by archetype key; `None` when the pool was unreadable
    /// (divisions that are still being created).
    pub held: Option<BTreeMap<String, f64>>,
    pub variants: Vec<(String, f64)>,
}
fn read_variant(r: &Reader<'_>, image: u64, v: u64) -> Result<Variant> {
    let l = layout(r)?;
    exact(r, image, v, l.variant_vtable, "equipment variant")?;
    let ty = r.u64(v + l.variant_type)?;
    exact(r, image, ty, l.equipment_type_vtable, "equipment type")?;
    let mut stats = BTreeMap::new();
    for (name, off) in VARIANT_STATS {
        stats.insert(*name, round(fixed(r, v + off)?, 3));
    }
    Ok(Variant {
        name: r.string_bounded(ty + l.equipment_key, 128)?,
        stats,
    })
}
/// Own divisions with their live numbers.
pub(super) fn divisions(r: &Reader<'_>, image: u64) -> Result<Vec<DivisionRead>> {
    let l = layout(r)?;
    let c = crate::construction::layout(r);
    let country = own_country(r, image)?;
    let (_, data, count) = r.array_header(country + l.country_divisions)?;
    let mut out = Vec::new();
    for index in 0..count.min(4096) as u64 {
        let division = r.u64(data + index * 8)?;
        exact(r, image, division, l.division_vtable, "division")?;
        let id = division_id(r, division, l.division_handle)?;
        let group = r.u64(division + l.division_group)?;
        let army = if group == 0 {
            None
        } else {
            r.string_bounded(group + l.group_name, 128).ok()
        };
        let location = r.u64(division + l.division_location)?;
        let (province, state) = if location != 0 && r.u64(location)? == image + c.province_vtable {
            let state = r.u64(location + c.province_state)?;
            let sid = if state != 0 && r.u64(state)? == image + c.state_vtable {
                Some(r.i32(state + c.state_id)?)
            } else {
                None
            };
            (Some(r.i32(location + c.province_id)?), sid)
        } else {
            (None, None)
        };
        let template = r.u64(division + l.division_template)?;
        let (held, variants) = match read_pool(r, image, division) {
            Ok(v) => (Some(v.0), v.1),
            Err(e) if e.code == "invalid_array" || e.code == "invalid_type" => (None, Vec::new()),
            Err(e) => return Err(e),
        };
        out.push(DivisionRead {
            id,
            address: division,
            template,
            army,
            province,
            state,
            hp: fixed(r, division + l.division_strength)?,
            org: fixed(r, division + l.division_organisation)?,
            manpower: fixed(r, division + l.division_manpower)?,
            held,
            variants,
        });
    }
    Ok(out)
}
type Pool = (BTreeMap<String, f64>, Vec<(String, f64)>);
fn read_pool(r: &Reader<'_>, image: u64, division: u64) -> Result<Pool> {
    let l = layout(r)?;
    let (_, data, count) = r.array_header(division + l.division_pool)?;
    if count > 32 {
        return Err(Error::new("invalid_array", "Equipment pool too long"));
    }
    let mut held = BTreeMap::new();
    for i in 0..count as u64 {
        let ty = r.u64(data + i * 24)?;
        exact(r, image, ty, l.equipment_type_vtable, "equipment type")?;
        held.insert(
            r.string_bounded(ty + l.equipment_key, 128)?,
            fixed(r, data + i * 24 + 16)?,
        );
    }
    let (_, vdata, vcount) = r.array_header(division + l.division_variants)?;
    if vcount > 32 {
        return Err(Error::new("invalid_array", "Variant pool too long"));
    }
    let mut variants = Vec::new();
    for i in 0..vcount as u64 {
        let v = r.u64(vdata + i * 16)?;
        let ty = r.u64(v + l.variant_type)?;
        exact(r, image, ty, l.equipment_type_vtable, "equipment type")?;
        variants.push((
            r.string_bounded(ty + l.equipment_key, 128)?,
            fixed(r, vdata + i * 16 + 8)?,
        ));
    }
    Ok((held, variants))
}
/// Variant stats by archetype key for a sample division of the template.
pub(super) fn sample_variants(
    r: &Reader<'_>,
    image: u64,
    division: u64,
) -> Result<BTreeMap<String, Variant>> {
    let l = layout(r)?;
    let (_, data, count) = r.array_header(division + l.division_pool)?;
    let (_, vdata, vcount) = r.array_header(division + l.division_variants)?;
    let mut archetypes = Vec::new();
    for i in 0..count.min(32) as u64 {
        let ty = r.u64(data + i * 24)?;
        archetypes.push(r.string_bounded(ty + l.equipment_key, 128)?);
    }
    let mut out = BTreeMap::new();
    for i in 0..vcount.min(32) as u64 {
        let v = r.u64(vdata + i * 16)?;
        let variant = read_variant(r, image, v)?;
        // The pool lists archetypes and variants in different orders; a
        // variant belongs to the archetype its name starts with.
        if let Some(a) = archetypes
            .iter()
            .filter(|a| variant.name.starts_with(a.as_str()))
            .max_by_key(|a| a.len())
        {
            out.insert(a.clone(), variant);
        } else if let Some(a) = archetypes.iter().find(|a| {
            // tank archetypes are chassis (`light_tank_chassis`), variants are
            // `light_tank_equipment_2`
            a.strip_suffix("_chassis")
                .is_some_and(|stem| variant.name.starts_with(&format!("{stem}_equipment")))
        }) {
            out.insert(a.clone(), variant);
        }
    }
    Ok(out)
}

/// Building levels of the wanted states (one pass over the state list).
pub(super) fn state_buildings(
    r: &Reader<'_>,
    image: u64,
    wanted: &std::collections::BTreeSet<i32>,
) -> Result<BTreeMap<i32, BTreeMap<String, u32>>> {
    let l = layout(r)?;
    let c = crate::construction::layout(r);
    let g = game::layout(r);
    let gs = r.u64(image + g.gs_global)?;
    let (_, sdata, scount) = r.array_header(gs + c.gs_states)?;
    let mut out = BTreeMap::new();
    for i in 0..scount.min(4096) as u64 {
        if out.len() == wanted.len() {
            break;
        }
        let state = r.u64(sdata + i * 8)?;
        if state == 0 || r.u64(state)? != image + c.state_vtable {
            continue;
        }
        let id = r.i32(state + c.state_id)?;
        if !wanted.contains(&id) {
            continue;
        }
        let (_, bdata, bcount) = r.array_header(state + l.status_offset + l.status_buildings)?;
        let mut levels = BTreeMap::new();
        for b in 0..bcount.min(64) as u64 {
            let building = r.u64(bdata + b * 8)?;
            if building == 0 || r.u64(building)? != image + c.building_vtable {
                continue;
            }
            let template = r.u64(building + l.building_template)?;
            if template == 0 {
                continue;
            }
            let key = r.string_bounded(template + c.template_key, 64)?;
            let level =
                u16::from_le_bytes(r.read(building + l.building_level, 2)?.try_into().unwrap());
            levels.insert(key, u32::from(level));
        }
        out.insert(id, levels);
    }
    Ok(out)
}

/// A country character that is an army leader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Leader {
    pub id: String,
    pub name: String,
    pub skill: i32,
    pub attack: i32,
    pub defense: i32,
    pub planning: i32,
    pub logistics: i32,
    pub traits: Vec<String>,
}
/// Every army leader of the own country (assigned or not).
pub(super) fn leaders(r: &Reader<'_>, image: u64) -> Result<Vec<Leader>> {
    let l = layout(r)?;
    let country = own_country(r, image)?;
    let characters = r.u64(country + l.country_characters)?;
    if !is_ptr(characters) {
        return Err(Error::new("invalid_type", "Country characters unavailable"));
    }
    r.object_type(characters, "CCountryCharacters")?;
    let mut out = Vec::new();
    for leader in r.pointer_array(characters + l.characters_army_leaders)? {
        if out.len() >= 256 {
            return Err(Error::new("invalid_array", "Too many leaders"));
        }
        exact(r, image, leader, l.leader_vtable, "army leader")?;
        let template = r.u64(leader + l.leader_template)?;
        if !is_ptr(template) {
            return Err(Error::new("invalid_type", "Leader template unavailable"));
        }
        r.object_type(template, "CUnitLeaderTemplate")?;
        let stats = r.read(template + l.leader_template_stats, 24)?;
        let n = |i: usize| i32::from_le_bytes(stats[i * 4..i * 4 + 4].try_into().unwrap());
        let (_, tdata, tcount) = r.array_header(template + l.leader_template_traits)?;
        let mut traits = Vec::new();
        for t in 0..tcount.min(16) as u64 {
            traits.push(r.string_bounded(tdata + t * 32, 64)?);
        }
        out.push(Leader {
            id: r.string_bounded(leader + l.leader_id, 128)?,
            name: r.string_bounded(leader + l.leader_name, 128)?,
            skill: n(1),
            attack: n(2),
            defense: n(3),
            planning: n(4),
            logistics: n(5),
            traits,
        });
    }
    Ok(out)
}

/// One consistent bracket of reads for the tools.
pub(super) fn snapshot<T>(
    process: &dyn Process,
    f: impl Fn(&Reader<'_>, u64) -> Result<T>,
) -> Result<T> {
    read(process, f)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(
        key: &str,
        width: f64,
        hp: f64,
        org: f64,
        manpower: f64,
        need: &[(&str, f64)],
    ) -> SubUnit {
        SubUnit {
            key: key.into(),
            width,
            hp,
            org,
            manpower,
            supply: 0.06,
            need: need.iter().map(|(k, v)| (k.to_string(), *v)).collect(),
            ..SubUnit::default()
        }
    }
    fn infantry_division() -> (Vec<SubUnit>, Vec<SubUnit>) {
        let inf = unit(
            "infantry",
            2.0,
            25.0,
            60.0,
            1000.0,
            &[("infantry_equipment", 100.0)],
        );
        let eng = unit(
            "engineer",
            0.0,
            2.0,
            20.0,
            300.0,
            &[("infantry_equipment", 10.0), ("support_equipment", 30.0)],
        );
        let art = unit(
            "artillery",
            0.0,
            0.2,
            0.0,
            300.0,
            &[("artillery_equipment", 12.0)],
        );
        (vec![inf; 9], vec![eng, art])
    }
    #[test]
    fn german_1936_infantry_division_matches_the_live_numbers() {
        let (b, s) = infantry_division();
        let t = totals(&b, &s);
        // Live divisions of Germany showed 227.2 hit points and 9600 manpower.
        assert!((t.hp - 227.2).abs() < 1e-9);
        assert_eq!(t.manpower, 9600.0);
        assert_eq!(t.width, 18.0);
        assert!((t.org - 560.0 / 11.0).abs() < 1e-9);
        assert_eq!(t.need["infantry_equipment"], 910.0);
        assert_eq!(t.need["support_equipment"], 30.0);
        assert_eq!(t.need["artillery_equipment"], 12.0);
        assert_eq!(t.battalions, 9);
        assert_eq!(t.support_companies, 2);
    }
    #[test]
    fn empty_template_has_zero_organisation_not_nan() {
        let t = totals(&[], &[]);
        assert_eq!(t.org, 0.0);
        assert_eq!(t.need.len(), 0);
    }
    #[test]
    fn equipment_estimate_uses_the_first_need_only() {
        let (b, s) = infantry_division();
        let mut variants = BTreeMap::new();
        let mut inf = Variant::default();
        inf.stats.insert("soft_attack", 6.0);
        inf.stats.insert("defense", 22.0);
        inf.stats.insert("reliability", 0.9);
        variants.insert("infantry_equipment".to_string(), inf);
        let mut art = Variant::default();
        art.stats.insert("soft_attack", 26.0);
        variants.insert("artillery_equipment".to_string(), art);
        let all: Vec<&SubUnit> = b.iter().chain(s.iter()).collect();
        let e = equipment_stat_estimate(&all, &variants);
        // 9 infantry + the engineer's first need (infantry) + the artillery
        assert_eq!(e["soft_attack"], 10.0 * 6.0 + 26.0);
        assert_eq!(e["defense"], 10.0 * 22.0);
        assert!(!e.contains_key("reliability"));
    }
    #[test]
    fn round_is_stable() {
        assert_eq!(round(1.23456, 2), 1.23);
        assert_eq!(round(-0.0001, 2), 0.0);
    }
}
