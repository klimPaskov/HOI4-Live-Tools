//! `hoi4_designs`: read-only list of the own equipment designs (variants) from
//! macOS memory (089EA519): name, hull/chassis type, parent design, chosen
//! modules per slot, computed stats and the production lines using each.
//!
//! The own list is `CProductionStatus` +0xa0 (48 variants for Germany in April
//! 1938: 29 default designs plus named ship/plane designs). Names and hulls
//! matched the vanilla history (Deutschland, Type 24, Zerstoerer 1934, Type II,
//! Type VII, Koenigsberg, Emden, Bremse, Leipzig, Admiral Hipper, Scharnhorst,
//! Ju 86, Do 17); stat offsets matched 168 vanilla default-variant values.
use super::fleetmem::{self, pointers, require_libcxx, Variant};
use crate::{
    economy,
    error::{Error, Result},
    memory::Reader,
    platform::Process,
};
use serde_json::{json, Map, Value};

const PRODUCTION_VARIANTS: u64 = 0xa0;
const PRODUCTION_LINES: u64 = 0x58;
const LINE_HANDLE: u64 = 0x8;
const LINE_VARIANT: u64 = 0x88;
const LINE_REQUESTED: u64 = 0xe8;
const VARIANT_LIMIT: usize = 512;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Design {
    pub variant: Variant,
    pub stats: Map<String, Value>,
    pub modules: Vec<(String, String)>,
    /// Upgrade levels (equipment without module slots), `(upgrade key, level)`.
    pub upgrades: Vec<(String, i32)>,
    pub lines: Vec<(u64, i32)>,
}

/// Coarse family of an equipment type key.
pub(super) fn kind_of(key: &str) -> &'static str {
    let k = key.to_lowercase();
    if k.starts_with("ship_hull") || k.starts_with("convoy") || k.contains("submarine") {
        "ship"
    } else if ["fighter", "bomber", "cas_", "plane", "airframe", "heli", "missile"]
        .iter()
        .any(|p| k.contains(p))
    {
        "plane"
    } else if k.contains("tank") || k.contains("armor") || k.contains("chassis") {
        "tank"
    } else {
        "other"
    }
}

pub(super) fn designs(r: &Reader<'_>, image: u64, production: u64) -> Result<Vec<Design>> {
    let mut lines: Vec<(u64, u64, i32)> = Vec::new();
    for line in pointers(r, production + PRODUCTION_LINES, 2048)? {
        let variant = r.u64(line + LINE_VARIANT)?;
        if variant != 0 {
            lines.push((variant, r.u64(line + LINE_HANDLE)?, r.i32(line + LINE_REQUESTED)?));
        }
    }
    let mut out = Vec::new();
    for address in pointers(r, production + PRODUCTION_VARIANTS, VARIANT_LIMIT)? {
        let variant = fleetmem::variant(r, image, address)?;
        out.push(Design {
            stats: fleetmem::stats(r, address)?,
            modules: fleetmem::modules(r, address)?,
            upgrades: fleetmem::upgrades(r, address).unwrap_or_default(),
            lines: lines
                .iter()
                .filter(|(v, _, _)| *v == address)
                .map(|(_, h, n)| (*h, *n))
                .collect(),
            variant,
        });
    }
    // Parent ids refer to designs of the same list.
    expect_unique_ids(&out)?;
    Ok(out)
}
fn expect_unique_ids(designs: &[Design]) -> Result<()> {
    let mut seen = std::collections::BTreeSet::new();
    for d in designs {
        if !seen.insert(d.variant.id) {
            return Err(Error::new("invalid_object", "Duplicate design id"));
        }
    }
    Ok(())
}

pub(super) fn design_json(d: &Design, detail: bool, names: &[(i32, String)]) -> Value {
    let v = &d.variant;
    let parent = v.parent_id.map(|p| {
        json!({"id":p,"name":names.iter().find(|(i, _)| *i == p).map(|(_, n)| n)})
    });
    let mut row = json!({"design_id":v.id,"name":(!v.name.is_empty()).then_some(&v.name),
        "class_suffix":(!v.class_suffix.is_empty()).then_some(&v.class_suffix),
        "hull_or_chassis":v.type_key,"kind":kind_of(&v.type_key),
        "default_design":v.parent_id.is_none(),"based_on":parent,
        "manpower":v.manpower,"obsolete":v.obsolete,"in_country_design_list":true,
        "production_lines":d.lines.iter().map(|(h, n)| json!({"line_handle":h,"requested_factories":n})).collect::<Vec<_>>()});
    if detail {
        row["stats"] = Value::Object(d.stats.clone());
        row["modules"] = Value::Array(
            d.modules
                .iter()
                .map(|(slot, m)| json!({"slot":slot,"module":m}))
                .collect(),
        );
        if !d.upgrades.is_empty() {
            row["upgrades"] = Value::Array(
                d.upgrades
                    .iter()
                    .map(|(k, l)| json!({"upgrade":k,"level":l}))
                    .collect(),
            );
        }
    }
    row
}
pub(super) fn view(all: &[Design], kind: Option<&str>, custom_only: bool, detail: bool, name: Option<&str>) -> Value {
    let names: Vec<(i32, String)> = all
        .iter()
        .map(|d| (d.variant.id, display(&d.variant)))
        .collect();
    let wanted = name.map(str::to_lowercase);
    let rows: Vec<Value> = all
        .iter()
        .filter(|d| kind.is_none_or(|k| kind_of(&d.variant.type_key) == k))
        .filter(|d| !custom_only || !d.variant.name.is_empty())
        .filter(|d| {
            wanted.as_ref().is_none_or(|w| {
                d.variant.name.to_lowercase().contains(w)
                    || d.variant.type_key.to_lowercase().contains(w)
            })
        })
        .map(|d| design_json(d, detail, &names))
        .collect();
    json!({"designs":rows,"count":rows.len(),"total_in_country":all.len(),
        "validated":{"names_and_hulls":true,"stat_offsets":"168 default-variant values matched vanilla files; values include modules and country bonuses",
            "modules":"slot ids matched the hull's slot table; module keys read, not compared to a screen",
            "production_line_join":"line variant pointer equals design address",
            "obsolete":"variant +0x414 flipped 0->1->0 by the Production list's Decommission/Recommission buttons (26 October 2026)"},
        "unavailable":{"producible":"not identified; every listed design is in the country list",
            "design_creation":"hoi4_design_create (tank/plane upgrades, ship modules); tank and plane module designers not driven"}})
}
pub(super) fn display(v: &Variant) -> String {
    if v.name.is_empty() {
        v.type_key.clone()
    } else {
        v.name.clone()
    }
}
/// Every own design (fresh read bracket); the shared source of `hoi4_designs` and the designer tools.
pub(super) fn read_all(process: &dyn Process) -> Result<Vec<Design>> {
    super::ui::read(process, |r, image| {
        require_libcxx(r)?;
        let (_, production) = economy::own_production(r, image)?;
        designs(r, image, production)
    })
}
/// Detail rows (as `hoi4_designs` with detail) for the given designs of one list.
pub(super) fn rows_json(all: &[Design], pick: impl Fn(&Design) -> bool) -> Vec<Value> {
    let names: Vec<(i32, String)> = all
        .iter()
        .map(|d| (d.variant.id, display(&d.variant)))
        .collect();
    all.iter()
        .filter(|d| pick(d))
        .map(|d| design_json(d, true, &names))
        .collect()
}
pub(super) fn tool(process: &dyn Process, args: &Map<String, Value>) -> Result<Value> {
    let kind = args.get("kind").and_then(Value::as_str);
    if kind.is_some_and(|k| !["ship", "plane", "tank", "other"].contains(&k)) {
        return Err(Error::new(
            "invalid_arguments",
            "kind must be ship, plane, tank or other",
        ));
    }
    let name = args.get("name").and_then(Value::as_str);
    if name.is_some_and(|n| n.len() > 64 || n.chars().any(char::is_control)) {
        return Err(Error::new("invalid_arguments", "Invalid name filter"));
    }
    let custom_only = args
        .get("custom_only")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let detail = args.get("detail").and_then(Value::as_bool).unwrap_or(false);
    let all = super::ui::read(process, |r, image| {
        require_libcxx(r)?;
        let (_, production) = economy::own_production(r, image)?;
        designs(r, image, production)
    })?;
    Ok(view(&all, kind, custom_only, detail, name))
}
pub(super) fn schema() -> Value {
    json!({"name":"hoi4_designs","description":"Own equipment designs (ship, plane, tank and other variants) read from game memory; no panel, no input, never pauses. Per design: id, name and class suffix, hull/chassis type, whether it is a default design or based on another, manpower, the production lines currently building it (line handle, requested factories); with detail:true also computed stats (non-zero only) and the chosen module per slot. Filters: kind, custom_only (named designs only), name. Obsolete and producible flags are not identified; design creation is not available.","inputSchema":{"type":"object","properties":{"kind":{"type":"string","enum":["ship","plane","tank","other"]},"custom_only":{"type":"boolean","default":false},"detail":{"type":"boolean","default":false},"name":{"type":"string","maxLength":64}},"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::super::testmem::{Mem, IMAGE};
    use super::*;

    fn variant(mem: &Mem, id: i32, name: &str, key: &str, parent: u64) -> u64 {
        let v = mem.alloc(0x500);
        mem.u64(v, IMAGE + fleetmem::VARIANT_VTABLE);
        mem.i32(v + fleetmem::VARIANT_ID, id);
        mem.string(v + fleetmem::VARIANT_NAME, name);
        let kind = mem.alloc(0x400);
        mem.string(kind + fleetmem::TYPE_KEY, key);
        mem.u64(v + fleetmem::VARIANT_TYPE, kind);
        mem.u64(v + fleetmem::VARIANT_PARENT, parent);
        mem.i32(v + fleetmem::VARIANT_MANPOWER, 1200);
        mem.i32(v + 0x1f8, 2_622_000);
        v
    }
    #[test]
    fn lists_designs_with_lines_modules_and_stats() {
        let mem = Mem::new();
        let production = mem.alloc(0x200);
        let base = variant(&mem, 1343, "", "ship_hull_cruiser_panzerschiff", 0);
        let custom = variant(&mem, 1347, "Deutschland", "ship_hull_cruiser_panzerschiff", base);
        let plane = variant(&mem, 1360, "Ju 86", "tac_bomber_equipment_0", 0);
        // module slot table on the custom design's hull and one chosen module
        let kind = mem.u64_at(custom + fleetmem::VARIANT_TYPE);
        let slots = mem.alloc(0x80);
        mem.i32(slots + fleetmem::SLOT_ID, 0x703c);
        mem.string(slots + fleetmem::SLOT_NAME, "fixed_ship_battery_slot");
        mem.u64(kind + fleetmem::TYPE_SLOTS, slots);
        mem.i32(kind + fleetmem::TYPE_SLOTS + 8, 2);
        mem.i32(kind + fleetmem::TYPE_SLOTS + 12, 1);
        let module = mem.alloc(0x100);
        mem.string(module + fleetmem::MODULE_KEY, "ship_heavy_battery_2");
        let entries = mem.alloc(16);
        mem.i32(entries, 0x703c);
        mem.u64(entries + 8, module);
        mem.u64(custom + fleetmem::VARIANT_MODULES, entries);
        mem.i32(custom + fleetmem::VARIANT_MODULES + 8, 2);
        mem.i32(custom + fleetmem::VARIANT_MODULES + 12, 1);
        let line = mem.alloc(0x100);
        mem.u64(line + LINE_HANDLE, 777);
        mem.u64(line + LINE_VARIANT, plane);
        mem.i32(line + LINE_REQUESTED, 3);
        mem.pointer_array(production + PRODUCTION_LINES, &[line]);
        mem.pointer_array(production + PRODUCTION_VARIANTS, &[base, custom, plane]);
        let r = Reader::new(&mem);
        let all = designs(&r, IMAGE, production).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[1].modules, vec![("fixed_ship_battery_slot".to_string(), "ship_heavy_battery_2".to_string())]);
        assert_eq!(all[2].lines, vec![(777, 3)]);
        let v = view(&all, Some("ship"), true, true, None);
        assert_eq!(v["count"], 1);
        assert_eq!(v["designs"][0]["name"], "Deutschland");
        assert_eq!(v["designs"][0]["based_on"]["id"], 1343);
        assert_eq!(v["designs"][0]["stats"]["naval_speed"], 26.22);
        assert_eq!(v["designs"][0]["modules"][0]["module"], "ship_heavy_battery_2");
        let planes = view(&all, Some("plane"), false, false, Some("ju"));
        assert_eq!(planes["designs"][0]["production_lines"][0]["requested_factories"], 3);
        assert!(planes["designs"][0].get("stats").is_none());
    }
    #[test]
    fn classifies_kinds_and_rejects_duplicates() {
        assert_eq!(kind_of("ship_hull_light_1"), "ship");
        assert_eq!(kind_of("fighter_equipment_0"), "plane");
        assert_eq!(kind_of("CAS_equipment_1"), "plane");
        assert_eq!(kind_of("light_tank_equipment_1"), "tank");
        assert_eq!(kind_of("infantry_equipment_1"), "other");
        let mem = Mem::new();
        let production = mem.alloc(0x200);
        let a = variant(&mem, 1, "", "infantry_equipment_1", 0);
        let b = variant(&mem, 1, "", "infantry_equipment_1", 0);
        mem.pointer_array(production + PRODUCTION_LINES, &[]);
        mem.pointer_array(production + PRODUCTION_VARIANTS, &[a, b]);
        let r = Reader::new(&mem);
        assert_eq!(designs(&r, IMAGE, production).unwrap_err().code, "invalid_object");
    }
}
