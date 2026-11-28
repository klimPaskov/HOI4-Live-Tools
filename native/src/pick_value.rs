//! What a political-power pick (a decision, a law, an adviser, an idea) does, read from the
//! running game's own scripts, and what it is worth to the country.
//!
//! Effects come from the content folder the game reads (mods layered over the install):
//! `common/decisions` (cost, `days_remove`, the decision's `modifier` block, the scripted
//! `complete_effect` / `remove_effect`), `common/ideas` (an idea's `modifier`, `cost`),
//! `common/characters` (an adviser's `cost`, `idea_token` and `traits`) and
//! `common/country_leader` (what each adviser trait grants). A scripted effect this reader does
//! not value (flags, variables, events, influence) is named in `unread`, never guessed.
//!
//! Value is in factory-years: what the effects add to the country's output over their
//! duration, scaled by the country's own factories (`Ctx`, read from the game). The per-modifier
//! shares in [`WEIGHTS`] are the value model (a judgement, not game data); a plan overrides any
//! of them with `value.weights` (modifier -> factory-years per +1.0 held a year, already scaled)
//! and the horizon a held effect counts for with `value.horizon_days`. Political power, combat
//! modifiers and experience are not converted into factories: they are named as unvalued, so a
//! pick whose worth lies there is the plan's or the player's judgement.
use crate::script::{Block, Entry, Val};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

/// One effect span: modifiers held for `days` (None: while the pick is held).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Part {
    pub modifiers: BTreeMap<String, f64>,
    pub days: Option<f64>,
}

/// What a pick does, from the scripts.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Item {
    /// Political power, where the script names a plain number.
    pub cost: Option<f64>,
    pub parts: Vec<Part>,
    /// One-off effects: stability, war_support, political_power, manpower, the experience pools,
    /// `building:<type>` levels, `resource:<type>` amounts, research bonuses.
    pub once: BTreeMap<String, f64>,
    /// Scripted effects this reader does not value.
    pub unread: Vec<String>,
}

/// The country the value is judged for (read from the game: hoi4_economy, hoi4_research).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Ctx {
    pub military: f64,
    pub civilian: f64,
    pub dockyards: f64,
    pub research_slots: f64,
    /// Game days a held effect counts for.
    pub horizon_days: f64,
}

impl Ctx {
    /// From hoi4_economy (`factory_trigger_count`) and the plan's `value.horizon_days`.
    pub(crate) fn from_reads(economy: &Value, research_slots: Option<f64>, plan: &Value) -> Ctx {
        let n = |k: &str| economy["factory_trigger_count"][k].as_f64().unwrap_or(0.0);
        Ctx {
            military: n("military"),
            civilian: n("civilian"),
            dockyards: n("naval"),
            research_slots: research_slots.unwrap_or(0.0),
            horizon_days: plan["value"]["horizon_days"]
                .as_f64()
                .filter(|d| *d > 0.0)
                .unwrap_or(HORIZON_DAYS),
        }
    }
}

/// The default horizon a held effect is counted for (a year of output): the value model's.
const HORIZON_DAYS: f64 = 365.0;

/// What one unit of a factory base is: the country's military, civilian or all factories,
/// dockyards or research slots.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Base {
    Military,
    Civilian,
    All,
    Dockyards,
    Slots,
}

/// The value model: factory-years per +1.0 of a modifier held a year, as a share of a base of
/// the country's own (a +10% factory output is a tenth of its military factories for a year;
/// consumer goods take that share of all factories; a research slot is counted as three
/// factories). Construction speed counts for the share of civilian factories a peacetime build
/// queue gives each building kind.
const WEIGHTS: &[(&str, Base, f64)] = &[
    ("industrial_capacity_factory", Base::Military, 1.0),
    ("industrial_capacity_dockyard", Base::Dockyards, 1.0),
    ("industry_free_repair_factor", Base::Military, 0.05),
    (
        "production_factory_max_efficiency_factor",
        Base::Military,
        0.5,
    ),
    (
        "production_factory_efficiency_gain_factor",
        Base::Military,
        0.2,
    ),
    (
        "production_factory_start_efficiency_factor",
        Base::Military,
        0.1,
    ),
    (
        "line_change_production_efficiency_factor",
        Base::Military,
        0.05,
    ),
    ("consumer_goods_factor", Base::All, -1.0),
    ("production_speed_buildings_factor", Base::Civilian, 0.9),
    (
        "production_speed_industrial_complex_factor",
        Base::Civilian,
        0.4,
    ),
    ("production_speed_arms_factory_factor", Base::Civilian, 0.3),
    ("production_speed_dockyard_factor", Base::Civilian, 0.05),
    (
        "production_speed_infrastructure_factor",
        Base::Civilian,
        0.1,
    ),
    ("production_speed_rail_way_factor", Base::Civilian, 0.05),
    (
        "production_speed_synthetic_refinery_factor",
        Base::Civilian,
        0.03,
    ),
    ("production_speed_air_base_factor", Base::Civilian, 0.02),
    ("production_speed_naval_base_factor", Base::Civilian, 0.02),
    ("production_speed_bunker_factor", Base::Civilian, 0.02),
    (
        "production_speed_coastal_bunker_factor",
        Base::Civilian,
        0.01,
    ),
    (
        "production_speed_radar_station_factor",
        Base::Civilian,
        0.01,
    ),
    (
        "production_speed_anti_air_building_factor",
        Base::Civilian,
        0.01,
    ),
    ("local_resources_factor", Base::Military, 0.3),
    ("research_speed_factor", Base::Slots, 3.0),
    // stability and war support move factory output and the laws they gate
    ("stability_factor", Base::All, 0.2),
    ("war_support_factor", Base::All, 0.05),
];

/// Effects with a weekly rate (their value is the rate times the weeks held).
const WEEKLY: &[(&str, &str)] = &[
    ("stability_weekly", "stability_factor"),
    ("war_support_weekly", "war_support_factor"),
];

/// Keys of a leader trait or idea that are not modifiers.
const NOT_MODIFIERS: &[&str] = &[
    "sprite",
    "random",
    "cost",
    "removal_cost",
    "level",
    "picture",
    "name",
    "allowed_civil_war",
];

/// Block keys inside an effect that are triggers or weights, never effects.
const NOT_EFFECTS: &[&str] = &[
    "limit",
    "ai_will_do",
    "allowed",
    "available",
    "visible",
    "target_trigger",
    "trigger",
    "modifier",
    "tooltip",
];

/// Whether an effect block's key opens a scope or a condition whose effects run (`if`,
/// `every_owned_state`, `random_owned_controlled_state`, a country tag, a state id, ROOT), not an
/// effect of its own (`add_to_variable = { .. }` is an effect this reader does not value).
fn is_scope(key: &str) -> bool {
    matches!(
        key,
        "if" | "else"
            | "else_if"
            | "hidden_effect"
            | "ROOT"
            | "FROM"
            | "PREV"
            | "THIS"
            | "capital_scope"
            | "owner"
            | "controller"
            | "overlord"
    ) || ["every_", "random_", "all_", "any_"]
        .iter()
        .any(|p| key.starts_with(p) && key != "random_list")
        || key.bytes().all(|b| b.is_ascii_digit())
        || (key.len() == 3
            && key
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()))
}

pub(crate) struct Catalog {
    pub items: HashMap<String, Item>,
}

fn numbers(block: &[Entry]) -> BTreeMap<String, f64> {
    block
        .iter()
        .filter(|e| !NOT_MODIFIERS.contains(&e.key.as_str()))
        .filter_map(|e| Some((e.key.clone(), e.val.scalar()?.trim().parse::<f64>().ok()?)))
        .collect()
}

fn add(map: &mut BTreeMap<String, f64>, key: &str, v: f64) {
    *map.entry(key.to_string()).or_insert(0.0) += v;
}

/// The effects of a scripted effect block into `item`; `ideas` gives an added idea's modifiers.
fn effects(block: &[Entry], ideas: &HashMap<String, BTreeMap<String, f64>>, item: &mut Item) {
    for e in block {
        let key = e.key.as_str();
        let num = e.val.scalar().and_then(|s| s.trim().parse::<f64>().ok());
        match (key, &e.val) {
            ("add_political_power", _) if num.is_some() => {
                add(&mut item.once, "political_power", num.unwrap_or(0.0))
            }
            ("add_stability", _) if num.is_some() => {
                add(&mut item.once, "stability", num.unwrap_or(0.0))
            }
            ("add_war_support", _) if num.is_some() => {
                add(&mut item.once, "war_support", num.unwrap_or(0.0))
            }
            ("add_manpower", _) if num.is_some() => {
                add(&mut item.once, "manpower", num.unwrap_or(0.0))
            }
            ("army_experience" | "navy_experience" | "air_experience", _) if num.is_some() => {
                add(&mut item.once, key, num.unwrap_or(0.0))
            }
            ("add_research_slot", _) if num.is_some() => {
                add(&mut item.once, "research_slot", num.unwrap_or(0.0))
            }
            ("add_ideas", Val::Scalar(idea)) => add_idea(idea, None, ideas, item),
            ("add_ideas", Val::Block(list)) => {
                for i in list.iter().filter(|i| i.op.is_empty()) {
                    add_idea(&i.key, None, ideas, item);
                }
            }
            ("add_timed_idea", Val::Block(b)) => {
                if let Some(idea) = b.text("idea") {
                    add_idea(idea, b.number("days"), ideas, item);
                }
            }
            ("add_building_construction" | "add_offsite_building", Val::Block(b)) => {
                if let Some(kind) = b.text("type") {
                    add(
                        &mut item.once,
                        &format!("building:{kind}"),
                        b.number("level").unwrap_or(1.0),
                    );
                }
            }
            ("add_resource", Val::Block(b)) => {
                if let Some(kind) = b.text("type") {
                    add(
                        &mut item.once,
                        &format!("resource:{kind}"),
                        b.number("amount").unwrap_or(0.0),
                    );
                }
            }
            ("add_tech_bonus", Val::Block(b)) => add(
                &mut item.once,
                "research_bonus",
                b.number("bonus").unwrap_or(0.0) * b.number("uses").unwrap_or(1.0),
            ),
            (k, Val::Block(b)) if is_scope(k) => {
                // a scope (a state, every owned state, a country) or a condition: its effects
                // count
                effects(b, ideas, item)
            }
            (k, _) if NOT_EFFECTS.contains(&k) => {}
            (k, _) => {
                if !item.unread.iter().any(|u| u == k) {
                    item.unread.push(k.to_string());
                }
            }
        }
    }
}

fn add_idea(
    idea: &str,
    days: Option<f64>,
    ideas: &HashMap<String, BTreeMap<String, f64>>,
    item: &mut Item,
) {
    match ideas.get(idea) {
        Some(m) => item.parts.push(Part {
            modifiers: m.clone(),
            days,
        }),
        None => item.unread.push(format!("add_ideas {idea}")),
    }
}

/// Every idea's modifiers and cost (common/ideas: `ideas = { <category> = { <idea> = {..} } }`).
fn ideas_of(entries: &[Entry]) -> (HashMap<String, BTreeMap<String, f64>>, HashMap<String, f64>) {
    let mut mods = HashMap::new();
    let mut costs = HashMap::new();
    for top in entries.iter().filter(|e| e.key == "ideas") {
        for category in top.val.block() {
            for idea in category.val.block() {
                let b = idea.val.block();
                if b.is_empty() {
                    continue;
                }
                let m = b
                    .find_val("modifier")
                    .map(|v| numbers(v.block()))
                    .unwrap_or_default();
                mods.insert(idea.key.clone(), m);
                if let Some(c) = b.number("cost") {
                    costs.insert(idea.key.clone(), c);
                }
            }
        }
    }
    (mods, costs)
}

/// Every leader trait's modifiers (common/country_leader: `leader_traits = { <trait> = {..} }`).
fn traits_of(entries: &[Entry]) -> HashMap<String, BTreeMap<String, f64>> {
    let mut out = HashMap::new();
    for top in entries.iter().filter(|e| e.key == "leader_traits") {
        for t in top.val.block() {
            let b = t.val.block();
            let mut m = numbers(b);
            if let Some(inner) = b.find_val("modifier") {
                for (k, v) in numbers(inner.block()) {
                    add(&mut m, &k, v);
                }
            }
            out.insert(t.key.clone(), m);
        }
    }
    out
}

/// The catalogue of a content folder; pure over the parsed scripts (see [`load`]).
pub(crate) fn catalog_of(
    decisions: &[Entry],
    ideas: &[Entry],
    characters: &[Entry],
    traits: &[Entry],
) -> Catalog {
    let (idea_mods, idea_costs) = ideas_of(ideas);
    let trait_mods = traits_of(traits);
    let mut items: HashMap<String, Item> = HashMap::new();
    for (key, m) in &idea_mods {
        items.insert(
            key.clone(),
            Item {
                cost: idea_costs.get(key).copied(),
                parts: vec![Part {
                    modifiers: m.clone(),
                    days: None,
                }],
                ..Item::default()
            },
        );
    }
    // advisers: the character id and the idea token both name the pick
    for top in characters.iter().filter(|e| e.key == "characters") {
        for c in top.val.block() {
            for adv in c.val.block().iter().filter(|e| e.key == "advisor") {
                let b = adv.val.block();
                let mut m = BTreeMap::new();
                let mut unread = Vec::new();
                for t in b.words("traits") {
                    match trait_mods.get(&t) {
                        Some(tm) => {
                            for (k, v) in tm {
                                add(&mut m, k, *v);
                            }
                        }
                        None => unread.push(format!("trait {t}")),
                    }
                }
                if let Some(inner) = b.find_val("modifier") {
                    for (k, v) in numbers(inner.block()) {
                        add(&mut m, &k, v);
                    }
                }
                let item = Item {
                    cost: b.number("cost"),
                    parts: vec![Part {
                        modifiers: m,
                        days: None,
                    }],
                    once: BTreeMap::new(),
                    unread,
                };
                if let Some(token) = b.text("idea_token") {
                    items.insert(token.to_string(), item.clone());
                }
                items.insert(c.key.clone(), item);
            }
        }
    }
    for category in decisions {
        for d in category.val.block() {
            let b = d.val.block();
            if b.is_empty()
                || (b.find_val("cost").is_none() && b.find_val("complete_effect").is_none())
            {
                continue;
            }
            let mut item = Item {
                cost: b.number("cost"),
                ..Item::default()
            };
            if let Some(m) = b.find_val("modifier") {
                item.parts.push(Part {
                    modifiers: numbers(m.block()),
                    days: b.number("days_remove"),
                });
            }
            for k in ["complete_effect", "remove_effect"] {
                if let Some(e) = b.find_val(k) {
                    effects(e.block(), &idea_mods, &mut item);
                }
            }
            items.insert(d.key.clone(), item);
        }
    }
    Catalog { items }
}

/// The catalogue of the running game's content folder (mods included).
pub(crate) fn load(dir: &Path) -> Catalog {
    let p = |sub: &str| crate::script::parse_dir(&dir.join(sub));
    catalog_of(
        &p("common/decisions"),
        &p("common/ideas"),
        &p("common/characters"),
        &p("common/country_leader"),
    )
}

/// An item as the read tools list it (`effects` of a decision or an option).
pub(crate) fn item_json(item: &Item) -> Value {
    json!({
        "parts": item.parts.iter().map(|p| json!({"modifiers": p.modifiers, "days": p.days})).collect::<Vec<_>>(),
        "once": item.once,
        "unread": item.unread,
    })
}

fn base(ctx: &Ctx, b: Base) -> f64 {
    match b {
        Base::Military => ctx.military,
        Base::Civilian => ctx.civilian,
        Base::All => ctx.military + ctx.civilian,
        Base::Dockyards => ctx.dockyards,
        Base::Slots => ctx.research_slots,
    }
}

/// Factory-years per +1.0 of `key` held a year: the plan's `value.weights` first, else
/// [`WEIGHTS`] scaled by the country; None for a modifier the model does not value.
fn weight(key: &str, ctx: &Ctx, plan: &Value) -> Option<f64> {
    if let Some(w) = plan["value"]["weights"][key].as_f64() {
        return Some(w);
    }
    WEIGHTS
        .iter()
        .find(|(k, _, _)| *k == key)
        .map(|(_, b, share)| share * base(ctx, *b))
}

/// The value of a pick's effects (`effects` as [`item_json`] lists it) in factory-years, with
/// the terms that make it and the effects it could not value. None when nothing in it is
/// valued (every effect scripted or outside the model): a judgement for the plan or the player.
pub(crate) fn value(
    effects: &Value,
    ctx: &Ctx,
    plan: &Value,
) -> Option<(f64, Vec<String>, Vec<String>)> {
    let mut total = 0.0;
    let mut terms: Vec<(f64, String)> = Vec::new();
    let mut unvalued: Vec<String> = effects["unread"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|u| u.as_str().map(str::to_string))
        .collect();
    let mut any = false;
    for part in effects["parts"].as_array().into_iter().flatten() {
        let days = part["days"]
            .as_f64()
            .unwrap_or(ctx.horizon_days)
            .min(ctx.horizon_days);
        for (k, v) in part["modifiers"].as_object().into_iter().flatten() {
            let Some(x) = v.as_f64() else { continue };
            let weekly = WEEKLY.iter().find(|(w, _)| *w == k.as_str());
            let (key, amount) = match weekly {
                // a weekly rate held for the span is that many weeks of it
                Some((_, as_factor)) => (*as_factor, x * days / 7.0),
                None => (k.as_str(), x),
            };
            let years = if weekly.is_some() {
                // the stability gained stays for the rest of the horizon
                ((ctx.horizon_days - days / 2.0) / 365.0).max(0.0)
            } else {
                days / 365.0
            };
            match weight(key, ctx, plan) {
                Some(w) => {
                    let t = w * amount * years;
                    total += t;
                    any = true;
                    terms.push((t, format!("{k} {x:+} for {days:.0} days")));
                }
                None => unvalued.push(format!("{k} {x:+}")),
            }
        }
    }
    for (k, v) in effects["once"].as_object().into_iter().flatten() {
        let Some(x) = v.as_f64() else { continue };
        let years = ctx.horizon_days / 365.0;
        let t = match k.as_str() {
            "stability" => weight("stability_factor", ctx, plan).map(|w| w * x * years),
            "war_support" => weight("war_support_factor", ctx, plan).map(|w| w * x * years),
            "research_slot" => Some(3.0 * x * years),
            // a building level is a factory (or the infrastructure / slot it opens) for the
            // horizon, less the half year it takes to build
            b if b == "building:industrial_complex" || b == "building:arms_factory" => {
                Some(x * ((ctx.horizon_days - 180.0) / 365.0).max(0.0))
            }
            "building:dockyard" => Some(x * ((ctx.horizon_days - 180.0) / 365.0).max(0.0)),
            _ => None,
        };
        match t {
            Some(t) => {
                total += t;
                any = true;
                terms.push((t, format!("{k} {x:+}")));
            }
            None => unvalued.push(format!("{k} {x:+}")),
        }
    }
    if !any {
        return None;
    }
    terms.sort_by(|a, b| b.0.abs().total_cmp(&a.0.abs()));
    let shown = terms
        .into_iter()
        .take(4)
        .map(|(t, s)| format!("{s} ({t:+.1})"))
        .collect();
    Some((total, shown, unvalued))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Ctx {
        Ctx {
            military: 28.0,
            civilian: 35.0,
            dockyards: 10.0,
            research_slots: 4.0,
            horizon_days: 365.0,
        }
    }

    const DECISIONS: &str = r#"
    economy_decisions = {
        improved_worker_conditions = {
            available = { has_stability < 1.0 }
            modifier = { stability_weekly = 0.005 consumer_goods_factor = 0.10 industrial_capacity_factory = -0.1 }
            days_remove = 180
            cost = 100
            complete_effect = { }
        }
        HOL_demand_increased_dutch_trade_with_germany = {
            cost = 10
            days_remove = 20
            remove_effect = { HOL = { add_to_variable = { ger_temp_trade_influence = 2 } } country_event = x.1 }
        }
        GER_build_factories = {
            cost = 50
            complete_effect = {
                random_owned_controlled_state = { limit = { is_core_of = ROOT } add_building_construction = { type = industrial_complex level = 2 instant_build = yes } }
                add_ideas = GER_economy_idea
            }
        }
    }"#;
    const IDEAS: &str = r#"ideas = {
        economy = { free_trade = { cost = 150 modifier = { production_speed_buildings_factor = 0.15 research_speed_factor = 0.05 } } }
        country = { GER_economy_idea = { modifier = { consumer_goods_factor = -0.05 } } }
    }"#;
    const CHARS: &str = r#"characters = { GER_hjalmar_schacht = { advisor = { slot = political_advisor idea_token = hjalmar_schacht cost = 75 traits = { captain_of_industry } } } }"#;
    const TRAITS: &str = r#"leader_traits = { captain_of_industry = { sprite = 10 random = yes production_speed_industrial_complex_factor = 0.1 production_speed_infrastructure_factor = 0.1 ai_will_do = { factor = 1 } } }"#;

    fn cat() -> Catalog {
        let p = crate::script::parse;
        catalog_of(&p(DECISIONS), &p(IDEAS), &p(CHARS), &p(TRAITS))
    }

    #[test]
    fn effects_are_read_from_the_decision_idea_and_adviser_scripts() {
        let c = cat();
        let w = &c.items["improved_worker_conditions"];
        assert_eq!(w.cost, Some(100.0));
        assert_eq!(w.parts[0].days, Some(180.0));
        assert_eq!(w.parts[0].modifiers["industrial_capacity_factory"], -0.1);
        let b = &c.items["GER_build_factories"];
        assert_eq!(b.once["building:industrial_complex"], 2.0);
        assert_eq!(b.parts[0].modifiers["consumer_goods_factor"], -0.05);
        let h = &c.items["HOL_demand_increased_dutch_trade_with_germany"];
        assert!(
            h.unread.iter().any(|u| u == "add_to_variable"),
            "{:?}",
            h.unread
        );
        assert!(h.unread.iter().any(|u| u == "country_event"));
        let s = &c.items["hjalmar_schacht"];
        assert_eq!(s.cost, Some(75.0));
        assert_eq!(
            s.parts[0].modifiers["production_speed_industrial_complex_factor"],
            0.1
        );
        assert!(!s.parts[0].modifiers.contains_key("sprite"));
        assert_eq!(c.items["GER_hjalmar_schacht"], *s);
        assert_eq!(c.items["free_trade"].cost, Some(150.0));
    }

    #[test]
    fn value_is_output_over_the_effects_span_and_unvalued_effects_are_named() {
        let c = cat();
        let plan = json!({});
        // the worker conditions: -10% output and +10% consumer goods for half a year outweigh
        // the stability gained
        let (v, terms, _) = value(
            &item_json(&c.items["improved_worker_conditions"]),
            &ctx(),
            &plan,
        )
        .unwrap();
        assert!(v < 0.0, "{v} {terms:?}");
        assert!(terms.iter().any(|t| t.contains("consumer_goods_factor")));
        // two factories and lower consumer goods are worth something
        let (v, _, _) = value(&item_json(&c.items["GER_build_factories"]), &ctx(), &plan).unwrap();
        assert!(v > 0.0, "{v}");
        // Schacht's construction speed is worth something
        let (v, _, _) = value(&item_json(&c.items["hjalmar_schacht"]), &ctx(), &plan).unwrap();
        assert!(v > 0.0);
        // influence variables and an event: nothing valued, a judgement
        assert!(value(
            &item_json(&c.items["HOL_demand_increased_dutch_trade_with_germany"]),
            &ctx(),
            &plan
        )
        .is_none());
        // the plan's own weight replaces the model's
        let plan = json!({"value": {"weights": {"industrial_capacity_factory": 0.0,
            "consumer_goods_factor": 0.0}}});
        let (v, _, _) = value(
            &item_json(&c.items["improved_worker_conditions"]),
            &ctx(),
            &plan,
        )
        .unwrap();
        assert!(v > 0.0, "{v}");
    }
}
