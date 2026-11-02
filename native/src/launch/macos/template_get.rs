//! `hoi4_template_get`: composition and numbers of the own division
//! templates. Pure memory read: no lease, no input, no cursor, no pause.
//!
//! Exact, from the sub-unit definitions in memory (validated against
//! `common/units/*.txt` and live divisions, see `units.rs`): composition,
//! width, hit points, mean base organisation, manpower, base supply use,
//! equipment need per archetype. Estimated and NOT screen-validated
//! (`validated:false`): the equipment-derived attack/defence/breakthrough/
//! armour/piercing sums, the Industry-cost line and the live organisation
//! factor.
use super::units::{self, round, SubUnit};
use crate::{
    error::{Error, Result},
    platform::Process,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_template_get","description":"Own division templates read from game memory (no input, no lease, no pause): per template the regiment columns (column x rows, e.g. [[infantry,infantry,infantry],...]), support companies, width, hit points, base organisation (mean of the battalions and companies; `organisation_live` is the same mean times the country's organisation modifier, taken from a live division of the template), manpower, supply use, equipment need per archetype (pieces), and per equipment archetype the variant a live division holds with its per-piece stats. `equipment_stats_estimate` (soft/hard attack, defence, breakthrough, armour, piercing, air attack) is an ESTIMATE summed from each battalion's main equipment variant, validated:false. Exact fields were recomputed against common/units and live divisions (hit points and manpower matched all own divisions). Optional `template` (exact name) narrows to one.","inputSchema":{"type":"object","properties":{"template":{"type":"string","maxLength":128}},"additionalProperties":false}})
}

fn unit_json(u: &SubUnit) -> Value {
    json!({"unit":u.key,"width":u.width,"hp":u.hp,"organisation":u.org,"morale":u.morale,
        "manpower":u.manpower,"supply_use":u.supply,"weight":u.weight,"training_days":u.training_days,
        "unit_stat_modifiers":u.unit_stats,
        "equipment_need":u.need.iter().map(|(k,v)|(k.clone(),json!(v))).collect::<serde_json::Map<_,_>>()})
}

pub(super) fn get(process: &dyn Process, only: Option<&str>) -> Result<Value> {
    units::snapshot(process, |r, image| {
        let templates = units::templates(r, image)?;
        if let Some(name) = only {
            if !templates.iter().any(|t| t.name == name) {
                let names: Vec<&str> = templates.iter().map(|t| t.name.as_str()).collect();
                return Err(Error::new(
                    "template_unknown",
                    format!(
                        "No own division template named \"{name}\". Templates: {}.",
                        names.join(", ")
                    ),
                ));
            }
        }
        let divisions = units::divisions(r, image)?;
        let mut rows = Vec::new();
        for t in templates
            .iter()
            .filter(|t| only.is_none_or(|n| n == t.name))
        {
            let totals = units::totals(&t.subunits, &t.support_units);
            let sample: Vec<&units::DivisionRead> = divisions
                .iter()
                .filter(|d| d.template == t.address && d.held.is_some())
                .collect();
            let live = sample.first();
            let variants: Option<BTreeMap<String, units::Variant>> = match live {
                Some(d) => units::sample_variants(r, image, d.address).ok(),
                None => None,
            };
            let all: Vec<&SubUnit> = t.subunits.iter().chain(t.support_units.iter()).collect();
            let estimate = variants
                .as_ref()
                .map(|v| units::equipment_stat_estimate(&all, v));
            let organisation_live = live.map(|d| round(d.org, 2));
            let org_factor = live
                .filter(|_| totals.org > 0.0)
                .map(|d| round(d.org / totals.org, 4));
            // Columns: trim empty trailing rows and columns.
            let columns: Vec<Vec<Option<&String>>> = t
                .columns
                .iter()
                .map(|c| {
                    let used = c.iter().rposition(Option::is_some).map_or(0, |i| i + 1);
                    c.iter().take(used).map(Option::as_ref).collect()
                })
                .filter(|c: &Vec<Option<&String>>| !c.is_empty())
                .collect();
            let mut combat: BTreeMap<&str, usize> = BTreeMap::new();
            for u in &t.subunits {
                *combat.entry(u.key.as_str()).or_default() += 1;
            }
            let variant_rows: Vec<Value> = variants
                .iter()
                .flatten()
                .map(|(k, v)| json!({"archetype":k,"variant":v.name,"per_piece_stats":v.stats}))
                .collect();
            rows.push(json!({
                "template":t.name,
                "priority":units::priority_name(t.priority),
                "columns":columns,
                "support":t.support,
                "composition":combat,
                "totals":totals.to_json(),
                "organisation_live":organisation_live,
                "organisation_modifier_factor":org_factor,
                "divisions_using_template":divisions.iter().filter(|d| d.template == t.address).count(),
                "equipment_variants":variant_rows,
                "equipment_stats_estimate":estimate,
                "units":t.subunits.iter().chain(t.support_units.iter()).map(unit_json).collect::<Vec<_>>(),
                "validated_fields":["columns","support","width","hp","manpower","equipment_need","organisation_base"],
                "validated":false,
                "validation_note":"composition, width, hit points, manpower and equipment need agree with common/units, history/units/GER_1936 and all live divisions; organisation_live/modifier_factor, equipment variant stats and equipment_stats_estimate are not screen-checked"
            }));
        }
        Ok(json!({"templates":rows,
            "estimate_method":"each battalion/company takes its stats from the variant of the first equipment it needs, fully equipped; sums, no modifiers"}))
    })
}
