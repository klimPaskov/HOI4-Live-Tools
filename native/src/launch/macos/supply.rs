//! `hoi4_supply`: per own division and per state where the army stands:
//! hit points against the template, manpower, equipment stock against need
//! (shortages) and the state's infrastructure level. Pure memory read: no
//! lease, no input, no cursor, no pause.
//!
//! NOT available (reported as null with a reason, never guessed): the
//! per-division supply percentage the game shows, supply flow, railway level,
//! supply hub and state supply capacity. The supply pass keeps its result in
//! `CSupplySystem` per province/country structures whose layout could not be
//! mapped with the game paused and every division fully supplied; the
//! `CArmy` object has no supply-ratio field (a diff of all 42 divisions over
//! 0x0..0x800 and a search for `CSupplyConsumer` found none). The lists
//! `below_70`/`below_50` are therefore empty and `supply_available` is false.
use super::units::{self, round};
use crate::{error::Result, platform::Process};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_supply","description":"Own divisions and the states they stand in, read from game memory (no input, no lease, no pause). Per division: id (as hoi4_armies), army, template, state_id/province_id, hit points and percent of the template maximum, manpower against need, equipment held against need per archetype with fill ratio and the archetypes that are short, plus the infrastructure level of its state. `shortages` lists divisions with missing equipment or below `strength_below` (default 0.7) percent hit points. SUPPLY PERCENTAGE, supply flow, railway level, hub and state supply capacity are NOT readable yet: `supply_available` is false, `below_70`/`below_50` are empty and each division has supply_percent null. Do not read the empty lists as 'everything is supplied'; check the supply map mode or the army panel for those.","inputSchema":{"type":"object","properties":{"strength_below":{"type":"number","minimum":0,"maximum":1}},"additionalProperties":false}})
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
        let mut rows = Vec::new();
        let mut shortages = Vec::new();
        let mut per_state: BTreeMap<i32, (usize, usize)> = BTreeMap::new();
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
            let fill_min = equipment.as_ref().and_then(|e| {
                e.iter()
                    .map(|x| x.3)
                    .fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.min(v))))
            });
            let weak = hp_pct.is_some_and(|p| p < strength_below);
            if let Some(s) = d.state {
                let e = per_state.entry(s).or_default();
                e.0 += 1;
                if !short_types.is_empty() || weak {
                    e.1 += 1;
                }
            }
            if !short_types.is_empty() || weak {
                shortages.push(json!({"id":d.id,"army":d.army,"state_id":d.state,
                    "short_equipment":short_types,"hp_percent":hp_pct}));
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
                "supply_percent":Value::Null,
                "infrastructure":d.state.and_then(|s| buildings.get(&s)).and_then(|b| b.get("infrastructure")),
            }));
        }
        // Per army: averages over its divisions (hit points as percent of the
        // template, organisation, lowest equipment fill) and where it stands.
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
                json!({"army":name,"divisions":rs.len(),
                    "hp_percent_mean":mean(rs,"hp_percent"),
                    "organisation_mean":mean(rs,"organisation"),
                    "equipment_fill_min":rs.iter().filter_map(|r| r["equipment_fill_min"].as_f64()).fold(None, |m: Option<f64>, v| Some(m.map_or(v, |m| m.min(v)))),
                    "states":states.iter().map(|(s,n)|(s.to_string(),json!(n))).collect::<serde_json::Map<_,_>>(),
                    "supply_percent_mean":Value::Null})
            })
            .collect();
        let state_rows: Vec<Value> = per_state
            .iter()
            .map(|(s, (n, bad))| {
                json!({"state_id":s,"divisions":n,"divisions_with_problems":bad,
                    "buildings":buildings.get(s),
                    "infrastructure":buildings.get(s).and_then(|b| b.get("infrastructure")),
                    "railway_level":Value::Null,"supply_capacity":Value::Null})
            })
            .collect();
        Ok(json!({
            "divisions":rows,
            "armies":army_rows,
            "states":state_rows,
            "shortages":shortages,
            "below_70":[],"below_50":[],
            "supply_available":false,
            "unavailable":{"fields":["supply_percent","supply_flow","railway_level","supply_hub","state_supply_capacity"],
                "reason":"no field for the per-division supply ratio was found in the division object and the supply system structures are unmapped; null, not guessed"},
            "strength_below":strength_below,
            "validated":false,
            "validated_fields":["id","state_id","hp","manpower","equipment have/need (matched the templates on every division)","infrastructure (matched history/states for 4 of 5 buildings of state 64; infrastructure was one level higher, built in the run)"],
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
}
