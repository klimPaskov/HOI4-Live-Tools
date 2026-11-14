//! The facts of the running game for the operation rules, read through the server's ordinary
//! read tools: the agency, operatives, networks, collaboration and ruling ideology
//! (`hoi4_intelligence`), wars (`hoi4_wars`), the equipment stockpile (`hoi4_stockpile`) and the
//! install's DLCs. Nothing here sends input or opens a view.
use super::{describe, verdict, Catalog, Facts};
use crate::focus_rules::live::Dlcs;
use crate::plan_predicate::Truth;
use crate::plan_run::Tools;
use serde_json::{json, Value};
use std::collections::HashMap;

pub struct Live<'d> {
    pub player: String,
    intel: Value,
    enemies: Option<Vec<String>>,
    stock: Option<HashMap<String, f64>>,
    dlcs: Option<&'d Dlcs>,
}

impl<'d> Live<'d> {
    pub fn read(tools: &dyn Tools, dlcs: Option<&'d Dlcs>) -> crate::error::Result<Self> {
        let intel = tools.call("hoi4_intelligence", &json!({}))?;
        let player = intel["game_time"]["player"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let enemies = tools.call("hoi4_wars", &json!({})).ok().and_then(|w| {
            w["wars"]["enemies"].as_array().map(|a| {
                a.iter()
                    .filter_map(|t| t.as_str().map(str::to_string))
                    .collect()
            })
        });
        let stock = tools.call("hoi4_stockpile", &json!({})).ok().and_then(|s| {
            s["equipment"].as_array().map(|rows| {
                rows.iter()
                    .filter_map(|r| {
                        Some((
                            r["archetype"].as_str()?.to_string(),
                            r["stockpile"].as_f64()?,
                        ))
                    })
                    .collect()
            })
        });
        Ok(Self::from_reads(player, intel, enemies, stock, dlcs))
    }

    pub fn from_reads(
        player: String,
        intel: Value,
        enemies: Option<Vec<String>>,
        stock: Option<HashMap<String, f64>>,
        dlcs: Option<&'d Dlcs>,
    ) -> Self {
        Live {
            player,
            intel,
            enemies,
            stock,
            dlcs,
        }
    }

    fn network_row(&self, target: &str) -> Option<&Value> {
        self.intel["intel_network"]["networks"]
            .as_array()?
            .iter()
            .find(|n| n["target"] == json!(target))
    }

    /// Targets with an own network, strongest first.
    pub fn network_targets(&self) -> Vec<String> {
        let mut rows: Vec<(String, f64)> = self.intel["intel_network"]["networks"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|n| {
                        Some((
                            n["target"].as_str()?.to_string(),
                            n["strength"].as_f64().unwrap_or(0.0),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        rows.into_iter().map(|(t, _)| t).collect()
    }
}

impl Facts for Live<'_> {
    fn player(&self) -> String {
        self.player.clone()
    }
    fn ideology(&self) -> Option<String> {
        self.intel["ruling_ideology"].as_str().map(str::to_string)
    }
    fn subject(&self) -> Truth {
        // Subjects have their own rules (autonomy levels); only a free country is judged.
        match self.intel["overlord"].as_str() {
            Some(_) => Truth::True,
            None if self.intel.get("overlord").is_some() => Truth::False,
            None => Truth::False,
        }
    }
    fn at_war_with(&self, tag: &str) -> Truth {
        match &self.enemies {
            Some(e) => Truth::from_bool(e.iter().any(|t| t == tag)),
            None => Truth::Unknown,
        }
    }
    fn at_war(&self) -> Truth {
        match &self.enemies {
            Some(e) => Truth::from_bool(!e.is_empty()),
            None => Truth::Unknown,
        }
    }
    fn network(&self, target: &str) -> Option<f64> {
        if self.intel["intel_network"]["state_read"] != json!(true) {
            return None;
        }
        Some(
            self.network_row(target)
                .and_then(|n| n["strength"].as_f64())
                .unwrap_or(0.0),
        )
    }
    fn collaboration(&self, target: &str) -> Option<f64> {
        let c = &self.intel["collaboration"];
        if c["state_read"] != json!(true) {
            return None;
        }
        Some(
            c["targets"]
                .as_array()
                .and_then(|a| a.iter().find(|r| r["target"] == json!(target)))
                .and_then(|r| r["value"].as_f64())
                .unwrap_or(0.0),
        )
    }
    fn token(&self, target: &str, token: &str) -> Truth {
        let t = &self.intel["operation_tokens"];
        if t["state_read"] != json!(true) {
            return Truth::Unknown;
        }
        Truth::from_bool(t["held"].as_array().is_some_and(|a| {
            a.iter()
                .any(|r| r["target"] == json!(target) && r["token"] == json!(token))
        }))
    }
    fn dlc(&self, name: &str) -> Truth {
        self.dlcs.map_or(Truth::Unknown, |d| d.has(name))
    }
    fn operatives(&self) -> Option<usize> {
        if self.intel["operatives"]["state_read"] != json!(true) {
            return None;
        }
        self.intel["operatives"]["recruited"]
            .as_u64()
            .map(|n| n as usize)
    }
    fn free_operatives(&self) -> Option<usize> {
        if self.intel["operatives"]["state_read"] != json!(true) {
            return None;
        }
        Some(
            self.intel["operatives"]["list"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter(|o| o["available_for_operation"] == json!(true))
                        .count()
                })
                .unwrap_or(0),
        )
    }
    fn stockpile(&self, archetype: &str) -> Option<f64> {
        self.stock
            .as_ref()
            .map(|s| s.get(archetype).copied().unwrap_or(0.0))
    }
}

/// `hoi4_operations`: every operation of the content against each target, with its costs and
/// whether it can be prepared now. `only` narrows to some operation keys; hidden operations
/// (their visible conditions certainly fail) are counted, and listed with `include_hidden`.
pub fn report(
    catalog: &Catalog,
    live: &Live<'_>,
    targets: &[String],
    only: &[String],
    include_hidden: bool,
) -> Value {
    let mut out = Vec::new();
    for target in targets {
        let mut rows = Vec::new();
        let mut hidden = 0;
        for op in &catalog.operations {
            if !only.is_empty() && !only.contains(&op.key) {
                continue;
            }
            let v = verdict(catalog, op, target, live);
            if v.visible == Truth::False && !include_hidden && only.is_empty() {
                hidden += 1;
                continue;
            }
            let mut row = describe(catalog, op);
            row["verdict"] = v.json();
            rows.push(row);
        }
        out.push(json!({
            "target": target,
            "network_strength": live.network(target),
            "collaboration": live.collaboration(target),
            "at_war": live.at_war_with(target).label(),
            "operations": rows,
            "hidden_operations": hidden,
        }));
    }
    json!({
        "player": live.player,
        "ruling_ideology": live.ideology(),
        "recruited_operatives": live.operatives(),
        "free_operatives": live.free_operatives(),
        "targets": out,
        "rule": "ready true = every condition the game checks before Prepare holds now; false lists each failing piece; unknown names the triggers the server cannot read (look once in the agency's operations list)",
    })
}
