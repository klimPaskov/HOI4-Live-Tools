//! `hoi4_supply`: per own division and per state/province where the army
//! stands: the supply status the game shows (received over needed), the
//! division's supply need, the supply the province receives against the
//! demand of the units in it (with the flows from the supply nodes), the
//! state's local supply, hub and railway level of the province, plus hit
//! points, manpower and equipment against the template and the state's
//! infrastructure. Pure memory read: no lease, no input, no cursor, no pause.
//!
//! The supply machinery lives in `CSupplySystem` / `CRailwayManager`
//! (reached from the game state) and in three fields of every division;
//! offsets, evidence and the formula behind the state value are in
//! `supplymem`. Not mapped (typed null, never guessed): hub and railway
//! throughput, trucks and trains in stock, the stored supply buffer, attrition
//! and the supply a province would get after new construction. The division
//! supply fields are consistent (received = need x ratio on every division,
//! the only divisions below 100 percent stand in the one province whose
//! demand exceeds its supply) but could not be compared with an on-screen
//! reading, so they stay `validated:false`.
use super::supplymem::{self, Node, NodeKind, ProvinceRail, ProvinceSupply};
use super::units::{self, round};
use crate::{error::Result, platform::Process};
use serde_json::{json, Value};
use std::collections::{btree_map::Entry, BTreeMap, BTreeSet};

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_supply","description":"Own divisions, the provinces and states they stand in and the supply system, read from game memory (no input, no lease, no pause). Per division: id (as hoi4_armies), army, template, state_id/province_id, hit points and percent of the template maximum, manpower against need, equipment held against need per archetype, plus supply: supply_percent (the 'Supply status' of the army tooltip, 100 = fully supplied), supply_need and supply_received per day, supply_band (full|ok|low|very_low) and the supply and demand of its province. `provinces` lists each province where own divisions stand: supply the province receives, demand placed by the units there, headroom (negative = overdrawn, the divisions there lose supply), flows from the supply nodes (node id, node province and kind when the node is own), supply hub present, railway level and naval base level. `states` gives the state's local supply and infrastructure. `supply_nodes` lists own hubs and ports with province, railway and naval base level. `below_70`/`below_50` list divisions under those supply percentages, `shortages` those with missing equipment or below `strength_below` (default 0.7) hit points, `overdrawn_provinces` the stacks that draw more than the province receives (spread them out, or build a hub or rail). Supply fields are validated:false (internally consistent, not yet compared with an on-screen reading); hub and rail throughput, trucks and trains, stored buffer and attrition are listed under `unavailable` as null.","inputSchema":{"type":"object","properties":{"strength_below":{"type":"number","minimum":0,"maximum":1}},"additionalProperties":false}})
}

/// Fill rows `(archetype, have, need, ratio)` for one division. Archetypes the
/// division holds beyond its need are ignored; a missing archetype is held 0.
pub(super) fn fill(
    need: &BTreeMap<String, f64>,
    held: &BTreeMap<String, f64>,
) -> Vec<(String, f64, f64, f64)> {
    need.iter()
        .filter(|(_, n)| **n > 0.0)
        .map(|(k, n)| {
            let have = held.get(k).copied().unwrap_or(0.0);
            (k.clone(), have, *n, (have / n).clamp(0.0, 1.0))
        })
        .collect()
}
/// Archetypes whose fill is below 100 percent.
pub(super) fn short(rows: &[(String, f64, f64, f64)]) -> Vec<&str> {
    rows.iter()
        .filter(|(_, have, need, _)| have + 1e-6 < *need)
        .map(|(k, ..)| k.as_str())
        .collect()
}

/// Everything read from the supply system for the divisions' provinces.
#[derive(Default)]
struct SupplyRead {
    divisions: BTreeMap<String, supplymem::DivisionSupply>,
    provinces: BTreeMap<i32, ProvinceSupply>,
    rails: BTreeMap<i32, Option<ProvinceRail>>,
    states: BTreeMap<i32, f64>,
    nodes: Vec<Node>,
}
type Soft<T> = std::result::Result<T, (String, String)>;
/// Errors that a fresh bracket may cure are passed on; everything else turns
/// the supply part into typed nulls with a reason.
fn soft<T>(result: Result<T>) -> Result<Soft<T>> {
    match result {
        Ok(v) => Ok(Ok(v)),
        Err(e)
            if matches!(
                e.code,
                "torn_read" | "short_read" | "invalid_gui" | "invalid_array"
            ) =>
        {
            Err(e)
        }
        Err(e) => Ok(Err((e.code.to_string(), e.message.to_string()))),
    }
}
fn collect(
    r: &crate::memory::Reader<'_>,
    image: u64,
    divisions: &[units::DivisionRead],
) -> Result<SupplyRead> {
    let gs = supplymem::game_state(r, image)?;
    let sys = supplymem::systems(r, image, gs)?;
    let mut out = SupplyRead::default();
    for d in divisions {
        out.divisions
            .insert(d.id.clone(), supplymem::division_supply(r, d.address)?);
        if let Some(p) = d.province {
            if let Entry::Vacant(v) = out.provinces.entry(p) {
                v.insert(supplymem::province_supply(r, &sys, p)?);
                out.rails
                    .insert(p, supplymem::province_rail(r, image, &sys, p)?);
            }
        }
        if let Some(s) = d.state {
            if let Entry::Vacant(v) = out.states.entry(s) {
                v.insert(supplymem::state_supply(r, &sys, s)?);
            }
        }
    }
    let country = units::own_country(r, image)?;
    let index = supplymem::own_country_index(r, image)?;
    out.nodes = supplymem::country_nodes(r, image, &sys, country, index)?;
    for n in &out.nodes {
        if let Entry::Vacant(v) = out.rails.entry(n.province) {
            v.insert(supplymem::province_rail(r, image, &sys, n.province)?);
        }
    }
    Ok(out)
}

fn rail_json(rail: &Option<ProvinceRail>) -> serde_json::Map<String, Value> {
    let (hub, level, naval) = rail
        .as_ref()
        .map_or((false, 0, 0), |x| (x.supply_hub, x.rail_level, x.naval_base_level));
    let mut m = serde_json::Map::new();
    m.insert("supply_hub".into(), json!(hub));
    m.insert("railway_level".into(), json!(level));
    m.insert("naval_base_level".into(), json!(naval));
    m
}
fn kind_name(k: NodeKind) -> &'static str {
    match k {
        NodeKind::Hub => "hub",
        NodeKind::Port => "port",
    }
}
fn lowest(values: impl Iterator<Item = f64>) -> Option<f64> {
    values.fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.min(v))))
}

pub(super) fn report(process: &dyn Process, strength_below: f64) -> Result<Value> {
    units::snapshot(process, |r, image| {
        let templates = units::templates(r, image)?;
        let totals: BTreeMap<u64, (String, units::Totals)> = templates
            .iter()
            .map(|t| {
                (
                    t.address,
                    (t.name.clone(), units::totals(&t.subunits, &t.support_units)),
                )
            })
            .collect();
        let divisions = units::divisions(r, image)?;
        let states: BTreeSet<i32> = divisions.iter().filter_map(|d| d.state).collect();
        let buildings = units::state_buildings(r, image, &states)?;
        let (sup, supply_error) = match soft(collect(r, image, &divisions))? {
            Ok(s) => (Some(s), None),
            Err(e) => (None, Some(e)),
        };
        let mut rows = Vec::new();
        let mut shortages = Vec::new();
        let mut below_70 = Vec::new();
        let mut below_50 = Vec::new();
        let mut bands: BTreeMap<&str, usize> = BTreeMap::new();
        let mut weighted: Vec<(f64, f64)> = Vec::new();
        let mut per_state: BTreeMap<i32, (usize, usize)> = BTreeMap::new();
        let mut per_province: BTreeMap<i32, Vec<String>> = BTreeMap::new();
        for d in &divisions {
            let t = totals.get(&d.template);
            let (hp_max, manpower_max) =
                t.map_or((None, None), |(_, t)| (Some(t.hp), Some(t.manpower)));
            let hp_pct = hp_max.filter(|m| *m > 0.0).map(|m| round(d.hp / m, 3));
            let equipment = match (&d.held, t) {
                (Some(held), Some((_, t))) => Some(fill(&t.need, held)),
                _ => None,
            };
            let short_types: Vec<&str> = equipment.as_ref().map(|e| short(e)).unwrap_or_default();
            let fill_min = equipment.as_ref().and_then(|e| lowest(e.iter().map(|x| x.3)));
            let weak = hp_pct.is_some_and(|p| p < strength_below);
            let ds = sup.as_ref().and_then(|s| s.divisions.get(&d.id));
            let ps = sup
                .as_ref()
                .and_then(|s| d.province.and_then(|p| s.provinces.get(&p)));
            let low = ds.is_some_and(|s| s.ratio < 0.7);
            if let Some(s) = d.state {
                let e = per_state.entry(s).or_default();
                e.0 += 1;
                if !short_types.is_empty() || weak || low {
                    e.1 += 1;
                }
            }
            if let Some(p) = d.province {
                per_province.entry(p).or_default().push(d.id.clone());
            }
            if !short_types.is_empty() || weak {
                shortages.push(json!({"id":d.id,"army":d.army,"state_id":d.state,
                    "short_equipment":short_types,"hp_percent":hp_pct}));
            }
            if let Some(s) = ds {
                weighted.push((s.need, s.ratio));
                *bands.entry(supplymem::band(s.ratio)).or_default() += 1;
                let line = json!({"id":d.id,"army":d.army,"state_id":d.state,"province_id":d.province,
                    "supply_percent":round(s.ratio * 100.0, 1)});
                if s.ratio < 0.5 {
                    below_50.push(line.clone());
                }
                if s.ratio < 0.7 {
                    below_70.push(line);
                }
            }
            rows.push(json!({
                "id":d.id,"army":d.army,"template":t.map(|(n, _)| n.as_str()),
                "state_id":d.state,"province_id":d.province,
                "hp":round(d.hp,2),"hp_max":hp_max,"hp_percent":hp_pct,
                "organisation":round(d.org,2),
                "manpower":d.manpower,"manpower_max":manpower_max,
                "equipment":equipment.as_ref().map(|e| e.iter().map(|(k,have,need,ratio)|
                    json!({"archetype":k,"have":have,"need":need,"fill":round(*ratio,3)})).collect::<Vec<_>>()),
                "variants_held":d.variants.iter().map(|(n,c)| json!({"variant":n,"count":c})).collect::<Vec<_>>(),
                "equipment_fill_min":fill_min.map(|v| round(v,3)),
                "short_equipment":short_types,
                "equipment_readable":equipment.is_some(),
                "supply_percent":ds.map(|s| round(s.ratio * 100.0, 1)),
                "supply_ratio":ds.map(|s| round(s.ratio, 5)),
                "supply_need":ds.map(|s| round(s.need, 4)),
                "supply_received":ds.map(|s| round(s.received, 4)),
                "supply_band":ds.map(|s| supplymem::band(s.ratio)),
                "province_supply":ps.map(|p| round(p.supply, 3)),
                "province_demand":ps.map(|p| round(p.demand, 3)),
                "province_headroom":ps.map(|p| round(p.supply - p.demand, 3)),
                "infrastructure":d.state.and_then(|s| buildings.get(&s)).and_then(|b| b.get("infrastructure")),
            }));
        }
        // Per army: averages over its divisions (hit points as percent of the
        // template, organisation, lowest equipment fill, need-weighted supply)
        // and where it stands.
        let mut by_army: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
        for row in &rows {
            if let Some(a) = row["army"].as_str() {
                by_army.entry(a.to_string()).or_default().push(row);
            }
        }
        let mean = |rows: &[&Value], key: &str| -> Value {
            let v: Vec<f64> = rows.iter().filter_map(|r| r[key].as_f64()).collect();
            if v.is_empty() {
                Value::Null
            } else {
                json!(round(v.iter().sum::<f64>() / v.len() as f64, 3))
            }
        };
        let army_rows: Vec<Value> = by_army
            .iter()
            .map(|(name, rs)| {
                let mut states: BTreeMap<i64, usize> = BTreeMap::new();
                for r in rs {
                    if let Some(s) = r["state_id"].as_i64() {
                        *states.entry(s).or_default() += 1;
                    }
                }
                let sw: Vec<(f64, f64)> = rs
                    .iter()
                    .filter_map(|r| Some((r["supply_need"].as_f64()?, r["supply_ratio"].as_f64()?)))
                    .collect();
                json!({"army":name,"divisions":rs.len(),
                    "hp_percent_mean":mean(rs,"hp_percent"),
                    "organisation_mean":mean(rs,"organisation"),
                    "equipment_fill_min":lowest(rs.iter().filter_map(|r| r["equipment_fill_min"].as_f64())),
                    "states":states.iter().map(|(s,n)|(s.to_string(),json!(n))).collect::<serde_json::Map<_,_>>(),
                    "supply_percent_mean":supplymem::weighted_mean(&sw).map(|m| round(m * 100.0, 1)),
                    "supply_percent_lowest":lowest(rs.iter().filter_map(|r| r["supply_percent"].as_f64()))})
            })
            .collect();
        let state_rows: Vec<Value> = per_state
            .iter()
            .map(|(s, (n, bad))| {
                let provinces: BTreeSet<i32> = divisions
                    .iter()
                    .filter(|d| d.state == Some(*s))
                    .filter_map(|d| d.province)
                    .collect();
                let rail_max = sup.as_ref().and_then(|x| {
                    provinces
                        .iter()
                        .filter_map(|p| x.rails.get(p))
                        .map(|r| r.as_ref().map_or(0, |r| r.rail_level))
                        .max()
                });
                json!({"state_id":s,"divisions":n,"divisions_with_problems":bad,
                    "buildings":buildings.get(s),
                    "infrastructure":buildings.get(s).and_then(|b| b.get("infrastructure")),
                    "local_supply":sup.as_ref().and_then(|x| x.states.get(s)).map(|v| round(*v, 3)),
                    "province_ids":provinces,
                    "railway_level_max":rail_max})
            })
            .collect();
        let province_rows: Vec<Value> = sup.as_ref().map_or_else(Vec::new, |s| {
            per_province
                .iter()
                .filter_map(|(p, ids)| {
                    let ps = s.provinces.get(p)?;
                    let state = divisions
                        .iter()
                        .find(|d| d.province == Some(*p))
                        .and_then(|d| d.state);
                    let flows: Vec<Value> = ps
                        .flows
                        .iter()
                        .map(|f| {
                            let node = s.nodes.iter().find(|n| n.id == u32::from(f.node));
                            json!({"node":f.node,"flow":round(f.flow, 3),
                                "node_province":node.map(|n| n.province),
                                "node_kind":node.map(|n| kind_name(n.kind))})
                        })
                        .collect();
                    let mut row = json!({"province_id":p,"state_id":state,"divisions":ids.len(),
                        "division_ids":ids,
                        "supply":round(ps.supply, 3),"demand":round(ps.demand, 3),
                        "headroom":round(ps.supply - ps.demand, 3),"flows":flows});
                    if let Some(o) = row.as_object_mut() {
                        o.extend(rail_json(s.rails.get(p).unwrap_or(&None)));
                    }
                    Some(row)
                })
                .collect()
        });
        let overdrawn: Vec<Value> = sup.as_ref().map_or_else(Vec::new, |s| {
            supplymem::overdrawn(&s.provinces)
                .into_iter()
                .map(|(p, shortfall)| {
                    json!({"province_id":p,"shortfall":round(shortfall, 3),
                        "division_ids":per_province.get(&p).cloned().unwrap_or_default(),
                        "supply":round(s.provinces[&p].supply, 3),
                        "demand":round(s.provinces[&p].demand, 3)})
                })
                .collect()
        });
        let node_rows: Vec<Value> = sup.as_ref().map_or_else(Vec::new, |s| {
            s.nodes
                .iter()
                .map(|n| {
                    let mut row = json!({"node":n.id,"kind":kind_name(n.kind),"province_id":n.province});
                    if let Some(o) = row.as_object_mut() {
                        o.extend(rail_json(s.rails.get(&n.province).unwrap_or(&None)));
                    }
                    row
                })
                .collect()
        });
        let summary = json!({
            "divisions":rows.len(),
            "supply_percent_mean":supplymem::weighted_mean(&weighted).map(|m| round(m * 100.0, 1)),
            "supply_percent_lowest":lowest(weighted.iter().map(|w| w.1)).map(|v| round(v * 100.0, 1)),
            "bands":bands,
            "overdrawn_provinces":overdrawn.len(),
        });
        Ok(json!({
            "divisions":rows,
            "armies":army_rows,
            "states":state_rows,
            "provinces":province_rows,
            "supply_nodes":node_rows,
            "overdrawn_provinces":overdrawn,
            "summary":summary,
            "shortages":shortages,
            "below_70":below_70,"below_50":below_50,
            "supply_available":sup.is_some(),
            "supply_error":supply_error.map(|(code, message)| json!({"code":code,"message":message})),
            "unavailable":{
                "fields":["hub_capacity","railway_throughput","trucks_and_trains_in_stock","stored_supply_buffer","attrition","supply_after_planned_construction"],
                "reason":"the throughput, stock and attrition structures of the supply system were not mapped; null, not guessed. Local state supply, province supply, demand and flows, hub and railway levels and the division supply status are read."
            },
            "strength_below":strength_below,
            "validated":false,
            "validated_fields":[
                "id","state_id","hp","manpower",
                "equipment have/need (matched the templates on every division)",
                "infrastructure (matched history/states for 4 of 5 buildings of state 64; infrastructure was one level higher, built in the run)",
                "local_supply (state): equals 0.3 x infrastructure + max(0.1, 0.18e-6 x manpower) + 0.2 x local_supplies + sum(0.2 + 0.05 x vp) for 700 of 1081 states within 0.001; the rest differ by multiples of 0.3 (infrastructure built since 1936)",
                "supply_nodes (31 pool records equal the used count; every hub province has a supply_node building, every port a naval_base)",
                "railway_level (province rail_way building: 2786 of 3213 equal the highest link level in map/railways.txt)"
            ],
            "unvalidated_fields":[
                "supply_percent/supply_ratio/supply_need/supply_received (consistent, no on-screen comparison)",
                "province_supply/province_demand/flows (the only division group below 100 percent stands in the one overdrawn province)"
            ],
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(v: &[(&str, f64)]) -> BTreeMap<String, f64> {
        v.iter().map(|(k, a)| (k.to_string(), *a)).collect()
    }
    #[test]
    fn full_equipment_has_no_shortage() {
        let need = map(&[("infantry_equipment", 910.0), ("support_equipment", 30.0)]);
        let rows = fill(&need, &need.clone());
        assert!(short(&rows).is_empty());
        assert!(rows.iter().all(|r| r.3 == 1.0));
    }
    #[test]
    fn missing_and_partial_archetypes_are_short() {
        let need = map(&[("infantry_equipment", 100.0), ("artillery_equipment", 12.0)]);
        let held = map(&[("infantry_equipment", 60.0)]);
        let rows = fill(&need, &held);
        assert_eq!(short(&rows), ["artillery_equipment", "infantry_equipment"]);
        let inf = rows.iter().find(|r| r.0 == "infantry_equipment").unwrap();
        assert_eq!(inf.3, 0.6);
        let art = rows.iter().find(|r| r.0 == "artillery_equipment").unwrap();
        assert_eq!(art.3, 0.0);
    }
    #[test]
    fn surplus_is_capped_and_zero_need_ignored() {
        let need = map(&[("a", 10.0), ("b", 0.0)]);
        let held = map(&[("a", 25.0)]);
        let rows = fill(&need, &held);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].3, 1.0);
        assert!(short(&rows).is_empty());
    }
    #[test]
    fn retryable_errors_pass_through_and_others_become_nulls() {
        use crate::error::Error;
        let torn: Result<u8> = Err(Error::new("torn_read", "x"));
        assert!(soft(torn).is_err());
        let wrong: Result<u8> = Err(Error::new("invalid_supply", "bad build"));
        let s = soft(wrong).unwrap().unwrap_err();
        assert_eq!(s.0, "invalid_supply");
        assert_eq!(soft(Ok(3u8)).unwrap().unwrap(), 3);
    }
    #[test]
    fn rail_json_has_typed_zeroes_without_a_record() {
        let none = rail_json(&None);
        assert_eq!(none["supply_hub"], json!(false));
        assert_eq!(none["railway_level"], json!(0));
        let some = rail_json(&Some(ProvinceRail {
            rail_level: 3,
            supply_hub: true,
            naval_base_level: 5,
        }));
        assert_eq!(some["railway_level"], json!(3));
        assert_eq!(some["naval_base_level"], json!(5));
    }
    #[test]
    fn lowest_of_nothing_is_none() {
        assert_eq!(lowest(std::iter::empty()), None);
        assert_eq!(lowest([0.9, 0.4, 1.0].into_iter()), Some(0.4));
    }
}
