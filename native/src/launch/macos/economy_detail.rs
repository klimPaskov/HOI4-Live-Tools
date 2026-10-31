//! `hoi4_economy_detail`: pure memory read of the own country's laws, the
//! aggregated country modifier table, the civilian-factory consumers
//! (consumer goods, trade, projects), fuel and raw manpower objects. No input,
//! no lease, no pause.
//!
//! Provenance (macOS 1.19.3 build 089EA519, found 26 Oct 2026 by RTTI scans,
//! embedded-vtable scans and value correlation against the screen and the
//! game files; nothing is copied from Linux/Windows):
//! - `country + 0x550` is an embedded `CModifier` (vtable `0x350cdc0`): the
//!   country's aggregated modifier table. Entry array pointer at `+0x10`, i32
//!   capacity at `+0x18`, i32 count at `+0x1c`; 16-byte entries
//!   `{i32 modifier id, i32 flag/junk, i64 value x100000}` sorted by id, zero
//!   modifiers absent. The same entry layout is used by every idea
//!   (`idea + 0x60` pointer, i32 count at `idea + 0x6c`).
//! - `politics + 0x50` is the array of active `CIdea` (vtable `0x35882b8`);
//!   the id string is a libc++ string at `idea + 0x18`. Laws are told apart
//!   by the id lists of `common/ideas/_economic.txt` and `_manpower.txt`.
//! - `country + 0x2e8` embedded `CCountryManpower` (vtable `0x350ad90`).
//! - `production + 0x448..0x4b8` (1.19.4) five consumer objects of the civilian
//!   factories (consumer goods `0x358d580`, special projects, licensed
//!   production, trade `0x358d640`, contract payment).
//! - `country + 0x12e0` `CCountryFuelStatus` (`+8` stock, `+0x10` capacity).
//!
//! Modifier id names were identified by correlation: each name below is an
//! idea modifier whose id and value were read from the active idea object
//! and equal the game file (see `MODIFIER_NAMES`). Unnamed ids are returned
//! raw.
use super::ui::read;
use crate::{
    economy,
    error::{Error, Result},
    game,
    memory::Reader,
};
use crate::platform::Process;
use serde_json::{json, Map, Value};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const FIXED: f64 = 100_000.0;
const COUNTRY_MODIFIER: u64 = 0x550;
const MODIFIER_VTABLE: u64 = 0x350cdc0;
const COUNTRY_MANPOWER: u64 = 0x2e8;
const MANPOWER_VTABLE: u64 = 0x350ad90;
const COUNTRY_FUEL: u64 = 0x12e0;
const FUEL_VTABLE: u64 = 0x34b2928;
const POLITICS_IDEAS: u64 = 0x50;
const IDEA_VTABLE: u64 = 0x35882b8;
const IDEA_NAME: u64 = 0x18;
const IDEA_MODIFIERS: u64 = 0x60;
const IDEA_MODIFIER_COUNT: u64 = 0x6c;
const CONSUMERS: u64 = 0x448; // 1.19.4: +0x18 (was 0x430 in 1.19.3)
/// `(label, offset from the consumers base, vtable)`.
const CONSUMER_KINDS: [(&str, u64, u64); 5] = [
    ("consumer_goods", 0, 0x358d580),
    ("special_projects", 0x18, 0x358d5c0),
    ("licensed_production", 0x30, 0x358d600),
    ("trade", 0x48, 0x358d640),
    ("contract_payment", 0x60, 0x358d680),
];
const ENTRY_LIMIT: usize = 400;
const IDEA_LIMIT: usize = 200;
const MODIFIER_ENTRY: usize = 16;

/// Modifier ids named by correlation with the active ideas (value equal to
/// the game file entry). `(id, name, how)`.
const MODIFIER_NAMES: [(i32, &str, &str); 12] = [
    (73, "conscription", "extensive_conscription idea 0.05 == total 0.05"),
    (103, "consumer_goods_expected_value", "partial_economic_mobilisation idea 0.25 (total 0.35: other sources add 0.10)"),
    (159, "conversion_cost_civ_to_mil_factor", "partial_economic_mobilisation -0.1 == total"),
    (160, "conversion_cost_mil_to_civ_factor", "partial_economic_mobilisation -0.1 == total"),
    (205, "research_speed_factor", "free_trade 0.10 (total 0.13)"),
    (223, "training_time_factor", "extensive_conscription 0.1 == total"),
    (224, "min_export", "free_trade 0.8 == total"),
    (252, "drift_defence_factor", "sour_loser idea 0.5 (total 0.75)"),
    (401, "fuel_gain_factor", "partial_economic_mobilisation -0.1 (total 0.3)"),
    (625, "cic_construction_boost_factor", "free_trade 0.05 == total"),
    (2066, "production_speed_arms_factory_factor", "partial_economic_mobilisation 0.1 (total 0.35)"),
    (2311, "production_speed_coastal_bunker_factor", "GER_fortify_the_vaterland_idea 0.2 (total 0.2)"),
];

const ECONOMY_LAWS: [&str; 10] = [
    "undisturbed_isolation",
    "isolation",
    "civilian_economy",
    "low_economic_mobilisation",
    "partial_economic_mobilisation",
    "war_economy",
    "tot_economic_mobilisation",
    "totaler_krieg_economy",
    "new_economic_policy",
    "capital_investment_model",
];
const TRADE_LAWS: [&str; 8] = [
    "uncontrolled_exports",
    "free_trade",
    "export_focus",
    "limited_exports",
    "autarkic_economy",
    "autarkic_economy_improved",
    "closed_economy",
    "special_economic_zones",
];
const CONSCRIPTION_LAWS: [&str; 7] = [
    "disarmed_nation",
    "volunteer_only",
    "limited_conscription",
    "extensive_conscription",
    "service_by_requirement",
    "all_adults_serve",
    "scraping_the_barrel",
];

/// Which law slot an idea id occupies, if it is a law.
pub(super) fn law_kind(id: &str) -> Option<&'static str> {
    if ECONOMY_LAWS.contains(&id) {
        Some("economy")
    } else if TRADE_LAWS.contains(&id) {
        Some("trade")
    } else if CONSCRIPTION_LAWS.contains(&id) {
        Some("conscription")
    } else {
        None
    }
}

pub(super) fn modifier_name(id: i32) -> Option<&'static str> {
    MODIFIER_NAMES.iter().find(|m| m.0 == id).map(|m| m.1)
}

/// Parse `{i32 id, i32 flag, i64 value}` entries.
pub(super) fn parse_entries(buf: &[u8]) -> Vec<(i32, i64)> {
    buf.chunks_exact(MODIFIER_ENTRY)
        .map(|c| {
            (
                i32::from_le_bytes(c[..4].try_into().unwrap()),
                i64::from_le_bytes(c[8..16].try_into().unwrap()),
            )
        })
        .collect()
}

fn value_of(entries: &[(i32, i64)], id: i32) -> Option<f64> {
    entries.iter().find(|e| e.0 == id).map(|e| e.1 as f64 / FIXED)
}

/// Consumer goods share arithmetic: factories taken out of `total` and what
/// is left for projects.
pub(super) fn civilian_split(total: i64, taken: i64) -> Value {
    let free = (total - taken).max(0);
    json!({
        "civilian_total": total,
        "taken_by_consumers": taken,
        "free": free,
        "share_taken": if total > 0 { json!(taken as f64 / total as f64) } else { Value::Null },
    })
}

fn object_vtable(r: &Reader<'_>, a: u64, image: u64, vtable: u64, what: &str) -> Result<()> {
    if r.u64(a)? != image + vtable {
        return Err(Error::new(
            "invalid_type",
            format!("{what}: unexpected object type"),
        ));
    }
    Ok(())
}

fn entries_at(r: &Reader<'_>, array: u64, count_at: u64, limit: usize) -> Result<Vec<(i32, i64)>> {
    let ptr = r.u64(array)?;
    let count = r.i32(count_at)?;
    if count < 0 || count as usize > limit {
        return Err(Error::new("invalid_array", "Modifier entry count"));
    }
    if count == 0 {
        return Ok(vec![]);
    }
    if !(0x10000..=0x7fff_ffff_0000).contains(&ptr) {
        return Err(Error::new("invalid_array", "Modifier entry pointer"));
    }
    Ok(parse_entries(&r.read(ptr, count as usize * MODIFIER_ENTRY)?))
}

struct Detail {
    player: String,
    date_raw: i64,
    modifiers: Vec<(i32, i64)>,
    ideas: Vec<(String, Vec<(i32, i64)>)>,
    consumers: Vec<(&'static str, i32, i32)>,
    consumer_goods_taken: i32,
    blocks: [Vec<u8>; 3],
    manpower: Vec<u8>,
    fuel: (i64, i64),
}

fn snapshot(r: &Reader<'_>, image: u64) -> Result<Detail> {
    let (country, production) = economy::own_production(r, image)?;
    let context = game::context_from_reader(r, image)?;
    let g = game::layout(r);
    // aggregated country modifier
    let m = country + COUNTRY_MODIFIER;
    object_vtable(r, m, image, MODIFIER_VTABLE, "country modifier")?;
    let modifiers = entries_at(r, m + 0x10, m + 0x1c, ENTRY_LIMIT)?;
    // active ideas
    let politics = r.u64(country + g.country_politics)?;
    let list = r.pointer_array(politics + POLITICS_IDEAS)?;
    if list.len() > IDEA_LIMIT {
        return Err(Error::new("invalid_array", "Idea count"));
    }
    let mut ideas = Vec::new();
    for idea in list {
        if !(0x10000..=0x7fff_ffff_0000).contains(&idea) {
            continue;
        }
        object_vtable(r, idea, image, IDEA_VTABLE, "idea")?;
        let name = r.string_bounded(idea + IDEA_NAME, 128)?;
        let mods = if r.u64(idea + IDEA_MODIFIERS)? < 0x10000 {
            vec![]
        } else {
            entries_at(r, idea + IDEA_MODIFIERS, idea + IDEA_MODIFIER_COUNT, 64)?
        };
        ideas.push((name, mods));
    }
    // civilian factory consumers
    let mut consumers = Vec::new();
    for (label, off, vt) in CONSUMER_KINDS {
        let a = production + CONSUMERS + off;
        object_vtable(r, a, image, vt, label)?;
        consumers.push((label, r.i32(a + 8)?, r.i32(a + 12)?));
    }
    let consumer_goods_taken = r.i32(production + CONSUMERS + 0x10)?;
    let blocks = [
        r.read(production + 0x2c8, 0x58)?,
        r.read(production + 0x328, 0x58)?,
        r.read(production + 0x388, 0x58)?,
    ];
    let manpower_object = country + COUNTRY_MANPOWER;
    object_vtable(r, manpower_object, image, MANPOWER_VTABLE, "country manpower")?;
    let manpower = r.read(manpower_object, 0x28)?;
    let fuel_object = r.u64(country + COUNTRY_FUEL)?;
    object_vtable(r, fuel_object, image, FUEL_VTABLE, "country fuel")?;
    let fuel = r.read(fuel_object + 8, 16)?;
    // Torn-read guard: the aggregated table must not have moved.
    if entries_at(r, m + 0x10, m + 0x1c, ENTRY_LIMIT)? != modifiers {
        return Err(Error::new("torn_read", "Country modifiers changed"));
    }
    Ok(Detail {
        player: context.player,
        date_raw: context.date_raw as i64,
        modifiers,
        ideas,
        consumers,
        consumer_goods_taken,
        blocks,
        manpower,
        fuel: (
            i64::from_le_bytes(fuel[..8].try_into().unwrap()),
            i64::from_le_bytes(fuel[8..16].try_into().unwrap()),
        ),
    })
}

fn i32_at(b: &[u8], o: usize) -> i64 {
    i32::from_le_bytes(b[o..o + 4].try_into().unwrap()) as i64
}
fn fx_at(b: &[u8], o: usize) -> f64 {
    i64::from_le_bytes(b[o..o + 8].try_into().unwrap()) as f64 / FIXED
}

/// Factory block: `{i32 count, i32 count, fixed total, fixed, fixed, 0, i64 x, fixed, fixed}`.
fn block(b: &[u8]) -> Value {
    json!({
        "count": i32_at(b, 0), "total": fx_at(b, 8), "in_service": fx_at(b, 0x10),
        "second_in_service": fx_at(b, 0x18), "x28": i32_at(b, 0x28), "x30": fx_at(b, 0x30),
    })
}

fn laws_json(ideas: &[(String, Vec<(i32, i64)>)]) -> Value {
    let mut laws = Map::new();
    let mut other = Vec::new();
    for (name, mods) in ideas {
        let entries: Vec<Value> = mods
            .iter()
            .map(|(id, v)| json!({"id": id, "name": modifier_name(*id), "value": *v as f64 / FIXED}))
            .collect();
        match law_kind(name) {
            Some(kind) => {
                laws.insert(kind.to_string(), json!({"id": name, "modifiers": entries}));
            }
            None => other.push(json!({"id": name, "modifiers": entries})),
        }
    }
    json!({"laws": laws, "other_ideas": other})
}

pub(super) fn detail(process: &dyn Process) -> Result<Value> {
    let started = Instant::now();
    let d = read(process, snapshot)?;
    let economy_json = economy::snapshot(process)?;
    let f = &economy_json["factory_trigger_count"];
    let civ = f["civilian"].as_i64().unwrap_or(0);
    let free = economy_json["civilian_factories_available_for_projects"]
        .as_i64()
        .unwrap_or(0);
    let named: Map<String, Value> = MODIFIER_NAMES
        .iter()
        .filter_map(|(id, name, _)| {
            value_of(&d.modifiers, *id).map(|v| ((*name).to_string(), json!(v)))
        })
        .collect();
    let raw: Vec<Value> = d
        .modifiers
        .iter()
        .map(|(id, v)| json!({"id": id, "name": modifier_name(*id), "value": *v as f64 / FIXED}))
        .collect();
    let cg_expected = value_of(&d.modifiers, 103);
    let mil = d.blocks[0].as_slice();
    let nav = d.blocks[1].as_slice();
    let civb = d.blocks[2].as_slice();
    let mil_n = i32_at(mil, 0);
    let nav_n = i32_at(nav, 0);
    let civ_n = i32_at(civb, 0);
    let (stock, capacity) = (d.fuel.0 as f64 / FIXED, d.fuel.1 as f64 / FIXED);
    let conscription = value_of(&d.modifiers, 73);
    let consumers: Vec<Value> = d
        .consumers
        .iter()
        .map(|(l, a, b)| json!({"consumer": l, "i32_at_8": a, "i32_at_c": b}))
        .collect();
    let mut out = laws_json(&d.ideas);
    let o = out.as_object_mut().unwrap();
    o.insert("source".into(), json!("native own economy detail (memory)"));
    o.insert("owned".into(), json!(true));
    o.insert("player".into(), json!(d.player));
    o.insert("date_raw".into(), json!(d.date_raw));
    o.insert("law_validation".into(), json!({
        "validated": true,
        "evidence": "active idea ids (partial_economic_mobilisation, free_trade, extensive_conscription) read from CPoliticalStatus+0x50; ids and modifier values equal common/ideas/_economic.txt and _manpower.txt"}));
    o.insert("modifiers".into(), json!({
        "named": named, "raw": raw,
        "validated": false,
        "evidence": "table country+0x550 holds 87 entries; every named id was correlated with a modifier of an active idea (value equal to the game file, totals >= idea value); raw ids not named are exposed unnamed",
        "name_evidence": MODIFIER_NAMES.iter().map(|m| json!({"id": m.0, "name": m.1, "evidence": m.2})).collect::<Vec<_>>(),
    }));
    o.insert("consumer_goods".into(), json!({
        "expected_value": cg_expected,
        "taken_by_consumer_goods_raw": d.consumer_goods_taken,
        "civilian_total": civ,
        "free_for_projects": free,
        "split": civilian_split(civ, d.consumer_goods_taken as i64),
        "share_of_civilian": if civ > 0 { json!(d.consumer_goods_taken as f64 / civ as f64) } else { Value::Null },
        "note": "taken_by_consumer_goods_raw is CConsumerGoodsConsumer+0x10; civilian_total minus it equals the 'available for projects' count (7 in the validated state) which every later consumer reports at +8. Not compared with the Production view (never opened); the relation of the raw count to expected_value (0.35) is unidentified, so do not read it as expected_value x factories",
        "validated": false,
    }));
    o.insert("civilian_consumers".into(), json!(consumers));
    o.insert("trade".into(), json!({
        "deals": null,
        "deals_reason": "no trade-deal object identified (CTradeConsumer holds no deal list at +8..+0x18; resource pool slots 1 and 2 are zero without deals; CCountryTradeView data not mapped)",
        "min_export": named.get("min_export"),
        "min_export_note": "free_trade gives min_export 0.8: 80% of production is committed to the market and the country keeps about 20% (matches resource pool slots 3 and 6/7 of hoi4_resources)",
        "trade_civilian_factories": null,
        "trade_civilian_reason": "CTradeConsumer count not identified while no deal exists; expect it to show a non-zero value at +8/+0xc once a deal runs",
        "validated": false,
    }));
    o.insert("industrial_capacity".into(), json!({
        "military": {"trigger_count": f["military"], "block": block(mil)},
        "dockyards": {"trigger_count": f["naval"], "block": block(nav)},
        "civilian": {"trigger_count": f["civilian"], "block": block(civb)},
        "top_bar_industry_candidate": mil_n + nav_n + civ_n,
        "top_bar_note": "military + dockyard + civilian count at the 0x370 block equals the top bar number (101) in the checked screenshot of 26 Oct 1938; the civilian trigger count (59) is 4 higher",
        "top_bar_validated": true,
        "factory_output_modifier": null,
        "factory_output_reason": "production_factory_* modifier ids not identified; see modifiers.raw",
        "validated": false,
    }));
    o.insert("manpower".into(), json!({
        "available": null,
        "total": null,
        "daily_growth": null,
        "recruited": null,
        "reason": "no field equal to the top bar manpower (2.90M) found in the country, politics, production, logistics or CCountryManpower objects",
        "country_manpower_object": {
            "offset": COUNTRY_MANPOWER,
            "x8_i64": i64::from_le_bytes(d.manpower[8..16].try_into().unwrap()),
            "x10_fixed": fx_at(&d.manpower, 0x10),
            "x18_i64": i64::from_le_bytes(d.manpower[0x18..0x20].try_into().unwrap()),
            "x20_fixed": fx_at(&d.manpower, 0x20),
            "meaning": "unidentified (0.5M? and 0.494M)",
        },
        "validated": false,
    }));
    o.insert("conscription".into(), json!({
        "law": o_law(&out_law(&d.ideas, "conscription")),
        "share_of_population": conscription,
        "share_validated": "equals the extensive_conscription idea value (0.05)",
    }));
    o.insert("fuel".into(), json!({
        "stock": stock, "capacity": capacity,
        "ratio": if capacity > 0.0 { json!(stock / capacity) } else { Value::Null },
        "gain_factor": named.get("fuel_gain_factor"),
        "days_left": null,
        "days_left_reason": "army/air/naval fuel burn is not stored; top bar shows 'Full' while stock == capacity",
        "validated": false,
    }));
    o.insert("speed_modifiers".into(), json!({
        "research_speed_factor": named.get("research_speed_factor"),
        "production_speed_arms_factory_factor": named.get("production_speed_arms_factory_factor"),
        "note": "research_speed_factor is the modifier total (0.13) not the effective speed; production_speed_arms_factory_factor is factory construction speed",
        "validated": false,
    }));
    o.insert("stability".into(), json!({"value": null, "reason": "not stored: computed on demand from DEFAULT_STABILITY 0.5, stability_factor and party popularity; use hoi4_country_status (top bar text)"}));
    o.insert("war_support".into(), json!({"value": null, "reason": "same as stability"}));
    o.insert("political_power_daily_gain".into(), json!({"value": null, "reason": "political_power_gain modifier id not identified; sample hoi4_economy twice"}));
    o.insert("freshness".into(), json!({
        "observed_unix_ms": SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
        "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0}));
    o.insert("observation_paused_game".into(), json!(false));
    o.insert("input_authorized".into(), json!(false));
    Ok(out)
}

fn out_law(ideas: &[(String, Vec<(i32, i64)>)], kind: &str) -> Option<String> {
    ideas
        .iter()
        .find(|(n, _)| law_kind(n) == Some(kind))
        .map(|(n, _)| n.clone())
}
fn o_law(v: &Option<String>) -> Value {
    v.as_ref().map_or(Value::Null, |s| json!(s))
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_economy_detail","description":"Own economy detail from game memory (no input, no lease, never pauses): current economy, trade and conscription law ids with their modifiers, the country's aggregated modifier table (named where identified: consumer_goods_expected_value, min_export, conscription, fuel_gain_factor, research_speed_factor, arms factory speed), civilian factory consumers (consumer goods raw count, free civilians), industry total as on the top bar, fuel stock/capacity/gain factor. Trade deals, trade-civilian usage, exact stability/war support, manpower pool, PP daily gain, factory output modifier and fuel days left are null with reasons. Every block states validated true/false.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: i32, v: i64) -> Vec<u8> {
        let mut b = vec![0u8; 16];
        b[..4].copy_from_slice(&id.to_le_bytes());
        b[4..8].copy_from_slice(&1i32.to_le_bytes());
        b[8..].copy_from_slice(&v.to_le_bytes());
        b
    }

    #[test]
    fn entries_parse_id_and_value() {
        let mut buf = entry(103, 25_000);
        buf.extend(entry(159, -10_000));
        let e = parse_entries(&buf);
        assert_eq!(e, vec![(103, 25_000), (159, -10_000)]);
        assert_eq!(value_of(&e, 159), Some(-0.1));
        assert_eq!(value_of(&e, 1), None);
    }

    #[test]
    fn law_kinds_classify_ideas() {
        assert_eq!(law_kind("partial_economic_mobilisation"), Some("economy"));
        assert_eq!(law_kind("free_trade"), Some("trade"));
        assert_eq!(law_kind("extensive_conscription"), Some("conscription"));
        assert_eq!(law_kind("GER_daimler_benz"), None);
    }

    #[test]
    fn named_ids_are_unique_and_found() {
        assert_eq!(modifier_name(103), Some("consumer_goods_expected_value"));
        assert_eq!(modifier_name(224), Some("min_export"));
        assert_eq!(modifier_name(1), None);
        let mut ids: Vec<i32> = MODIFIER_NAMES.iter().map(|m| m.0).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), MODIFIER_NAMES.len());
    }

    #[test]
    fn civilian_split_arithmetic() {
        let v = civilian_split(59, 52);
        assert_eq!(v["free"], json!(7));
        assert_eq!(civilian_split(0, 0)["share_taken"], Value::Null);
        assert_eq!(civilian_split(5, 9)["free"], json!(0));
    }

    #[test]
    fn block_reads_counts_and_fixed() {
        let mut b = vec![0u8; 0x58];
        b[0..4].copy_from_slice(&55i32.to_le_bytes());
        b[8..16].copy_from_slice(&5_900_000i64.to_le_bytes());
        let v = block(&b);
        assert_eq!(v["count"], json!(55));
        assert_eq!(v["total"], json!(59.0));
    }

    #[test]
    fn laws_json_splits_laws_and_other_ideas() {
        let ideas = vec![
            ("free_trade".to_string(), vec![(224, 80_000)]),
            ("GER_daimler_benz".to_string(), vec![]),
        ];
        let v = laws_json(&ideas);
        assert_eq!(v["laws"]["trade"]["id"], json!("free_trade"));
        assert_eq!(v["laws"]["trade"]["modifiers"][0]["name"], json!("min_export"));
        assert_eq!(v["other_ideas"][0]["id"], json!("GER_daimler_benz"));
    }

    #[test]
    fn tool_is_parameterless() {
        assert!(tool()["inputSchema"]["properties"].as_object().unwrap().is_empty());
    }
}
