//! Commander abilities (generals and field marshals), read from the game's files with a value judgement per
//! situation (engineer 110, 28 Nov 2026). The order itself (CTriggerAbilityCommand) is not issued yet: its layout
//! is known from the executable (below) and the module family is the next step; until then this read tells the
//! player and the executor which ability is worth its command power and why.
//!
//! The game's rules (1.20.1.0, common/abilities/*.txt `ability = { key = { type cost duration cooldown allowed
//! unit_modifiers } }`): `cost` is command power per day for each group of battalions the tooltip names
//! (ABILITY_TOOLTIP_DETAILED_COST "Base cost: VALUE (each for UNITS battalions)"), `duration` hours (-1 = until
//! cancelled), `cooldown` hours. Command power: BASE_MAX_COMMAND_POWER 80, BASE_COMMAND_POWER_GAIN 0.4 a day.
//! The ones that change a war (generic file): staff_office_plan planning_speed +4.0 (+400%, 0.12, 7 days);
//! rotating_reserves land_reinforce_rate +0.06 (0.12, 7 days); extra_suplies no_supply_grace 168 (0.2, 7 days,
//! cooldown 21 days); makeshift_bridges river attack +0.2 river crossing -0.4 penalty; siege_artillery fort attack
//! +0.2; deeper_dig_in dig_in_speed +6; force_attack offence +0.2 breakthrough +0.25 at org damage -100%,
//! strength damage +60% and no retreat; last_stand the same for defence.
//! DLC: no generic ability is gated on a DLC (the has_dlc lines in force_attack's `allowed` only choose which
//! national focuses exclude it); `dlc_in_allowed` lists any has_dlc an ability's `allowed` names.
//!
//! The order (static analysis of the 1.20 executable): CTriggerAbilityCommand, 0x68 bytes: tag +0x24, the ability
//! key (std::string) +0x28, the leaders (CPdxArray of handles) +0x40 count +0x4c, a flag +0x60; constructor
//! 0x100219b30 (cmd, CAbility*, &tag, const CPdxArray<handle>*, flag), can-do 0x10021a080 (looks the key up in the
//! ability database).
use crate::error::{Error, Result};
use crate::script::{self, Block, Entry, Val};
use serde_json::{json, Map, Value};
use std::path::Path;

pub(super) const TOOL: &str = "hoi4_army_abilities";

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Ability {
    pub key: String,
    pub kind: String,
    pub cost: f64,
    pub duration_h: i64,
    pub cooldown_h: i64,
    pub not_for_army_groups: bool,
    pub dlc_in_allowed: Vec<String>,
    pub modifiers: Vec<(String, f64)>,
    pub nested: Vec<(String, String, f64)>,
}

/// The situations the value rule judges in (any country, any mod: by what the modifiers do).
pub(super) const SITUATIONS: [&str; 7] = [
    "prepare_attack",
    "reinforce",
    "attack",
    "attack_river",
    "attack_fort",
    "defend",
    "short_supply",
];

pub(super) fn parse(entries: &[Entry]) -> Vec<Ability> {
    let mut out = Vec::new();
    for top in entries.all("ability") {
        for e in top.block() {
            let b = e.val.block();
            if b.is_empty() {
                continue;
            }
            let mut modifiers = Vec::new();
            let mut nested = Vec::new();
            for m in b.find_val("unit_modifiers").map(Val::block).unwrap_or(&[]) {
                match &m.val {
                    Val::Scalar(v) => {
                        if let Ok(n) = v.parse() {
                            modifiers.push((m.key.clone(), n));
                        }
                    }
                    Val::Block(inner) => {
                        for i in inner {
                            if let Some(n) = i.val.scalar().and_then(|v| v.parse().ok()) {
                                nested.push((m.key.clone(), i.key.clone(), n));
                            }
                        }
                    }
                }
            }
            let allowed = b.find_val("allowed").map(Val::block).unwrap_or(&[]);
            out.push(Ability {
                key: e.key.clone(),
                kind: b.text("type").unwrap_or("army_leader").to_string(),
                cost: b.number("cost").unwrap_or(0.0),
                duration_h: b.number("duration").unwrap_or(0.0) as i64,
                cooldown_h: b.number("cooldown").unwrap_or(0.0) as i64,
                not_for_army_groups: allowed.text("is_leading_army_group") == Some("no"),
                dlc_in_allowed: dlcs(allowed),
                modifiers,
                nested,
            });
        }
    }
    out
}

/// Every `has_dlc` named anywhere in a block (sorted, once each).
fn dlcs(b: &[Entry]) -> Vec<String> {
    let mut out = Vec::new();
    for e in b {
        match &e.val {
            Val::Scalar(v) if e.key == "has_dlc" => out.push(v.trim_matches('"').to_string()),
            Val::Block(inner) => out.extend(dlcs(inner)),
            Val::Scalar(_) => {}
        }
    }
    out.sort();
    out.dedup();
    out
}

fn m(a: &Ability, key: &str) -> f64 {
    a.modifiers
        .iter()
        .filter(|(k, _)| k == key)
        .map(|(_, v)| v)
        .sum()
}
fn n(a: &Ability, scope: &str, key: &str) -> f64 {
    a.nested
        .iter()
        .filter(|(s, k, _)| s == scope && k == key)
        .map(|(_, _, v)| v)
        .sum()
}

/// What the ability buys in a situation, as a relative gain (0 = nothing), and its price beyond command power:
/// (gain, drawback). The gain is the modifier that situation needs, in the game's own units: planning speed for a
/// prepared attack, attack and breakthrough when attacking (a river's or a fort's own attack bonus on top), defence
/// and dig-in when defending, the days without supply when supply is short. A drawback (more strength lost, no
/// retreat) is named so the rule keeps such an ability for a battle that must be won.
pub(super) fn worth(a: &Ability, situation: &str) -> (f64, Option<String>) {
    let attack = m(a, "offence") + m(a, "army_attack_factor") + 0.5 * m(a, "breakthrough_factor");
    let gain = match situation {
        "prepare_attack" => m(a, "planning_speed"),
        "reinforce" => m(a, "land_reinforce_rate"),
        "attack" => attack,
        "attack_river" => attack + n(a, "river", "attack") - m(a, "river_crossing_factor") * 0.5,
        "attack_fort" => attack + n(a, "fort", "attack"),
        "defend" => {
            m(a, "defence")
                + m(a, "army_defence_factor")
                + 0.25 * m(a, "combat_entrenchment")
                + 0.02 * m(a, "dig_in_speed")
        }
        "short_supply" => m(a, "no_supply_grace") / 24.0,
        _ => 0.0,
    };
    let drawback = if m(a, "str_damage_multiplier") > 0.0 {
        Some(format!(
            "strength damage +{:.0}%{}",
            m(a, "str_damage_multiplier") * 100.0,
            if a.modifiers
                .iter()
                .any(|(k, _)| k.starts_with("cannot_retreat"))
            {
                ", no retreat"
            } else {
                ""
            }
        ))
    } else if m(a, "offence") < 0.0 {
        Some(format!("attack {:+.0}%", m(a, "offence") * 100.0))
    } else if m(a, "max_planning") < 0.0 {
        Some(format!(
            "max planning {:+.0}%",
            m(a, "max_planning") * 100.0
        ))
    } else {
        None
    };
    (gain.max(0.0), drawback)
}

/// The best ability for a situation: the highest gain per command power among those without a drawback, else
/// (only with `must_win`) the best with one.
pub(super) fn best<'a>(
    all: &'a [Ability],
    situation: &str,
    must_win: bool,
) -> Option<(&'a Ability, f64)> {
    let mut rows: Vec<(&Ability, f64, bool)> = all
        .iter()
        .filter(|a| a.kind == "army_leader" && a.cost > 0.0)
        .map(|a| {
            let (g, d) = worth(a, situation);
            (a, g / a.cost, d.is_some())
        })
        .filter(|(_, v, d)| *v > 0.0 && (must_win || !d))
        .collect();
    rows.sort_by(|x, y| y.1.total_cmp(&x.1).then(x.0.key.cmp(&y.0.key)));
    rows.first().map(|(a, v, _)| (*a, *v))
}

pub(super) fn answer(all: &[Ability], command_power: Option<f64>) -> Value {
    let rows: Vec<Value> = all
        .iter()
        .map(|a| {
            let worth_in: Map<String, Value> = SITUATIONS
                .iter()
                .filter_map(|s| {
                    let (g, d) = worth(a, s);
                    (g > 0.0).then(|| ((*s).to_string(), json!({"gain": (g * 1000.0).round() / 1000.0, "drawback": d})))
                })
                .collect();
            json!({
                "ability": a.key, "type": a.kind,
                "cost_per_day_per_battalion_group": a.cost,
                "duration_days": if a.duration_h < 0 { Value::Null } else { json!(a.duration_h as f64 / 24.0) },
                "until_cancelled": a.duration_h < 0,
                "cooldown_days": (a.cooldown_h > 0).then(|| a.cooldown_h as f64 / 24.0),
                "field_marshal_may_use": !a.not_for_army_groups,
                "dlc_in_allowed": a.dlc_in_allowed,
                "unit_modifiers": a.modifiers.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<Map<_, _>>(),
                "terrain_modifiers": a.nested.iter().map(|(s, k, v)| json!({"where": s, k: v})).collect::<Vec<_>>(),
                "worth": worth_in,
            })
        })
        .collect();
    let best_by: Map<String, Value> = SITUATIONS
        .iter()
        .map(|s| {
            (
                (*s).to_string(),
                best(all, s, false).map_or(Value::Null, |(a, v)| json!({"ability": a.key, "gain_per_cost": (v * 100.0).round() / 100.0})),
            )
        })
        .collect();
    json!({
        "command_power": command_power,
        "abilities": rows,
        "best_by_situation": best_by,
        "issue": "not built: the order (CTriggerAbilityCommand) has no tool yet; use the army card's ability buttons",
        "rule": "use the best ability of the army's situation while command power would otherwise sit at its cap; an ability with a drawback (more strength lost, no retreat) only for a battle that must be won",
    })
}

fn load(dir: &Path) -> Vec<Ability> {
    parse(&script::parse_dir(&dir.join("common/abilities")))
}

pub(super) fn call(
    process: &dyn crate::platform::Process,
    args: &Map<String, Value>,
) -> Result<Value> {
    super::allowed(args, &[])?;
    let dir = super::mgmt::game_dir(process)?;
    let all = script::cached(&dir, "abilities", load);
    if all.is_empty() {
        return Err(Error::new(
            "game_data_unavailable",
            "common/abilities holds no ability",
        ));
    }
    let cp = super::ui::read(process, super::country_status::command_power)
        .ok()
        .flatten();
    Ok(answer(&all, cp))
}

pub(super) fn tool() -> Value {
    json!({"name": "hoi4_army_abilities", "description": "Read-only, no input: the commander abilities of the running game (common/abilities, mods included): per ability its command power cost per day (for each battalion group the game's tooltip names), duration, cooldown, whether a field marshal may use it, its unit modifiers, and its worth in each situation (prepare_attack: planning speed; reinforce: reinforce rate; attack, attack_river, attack_fort: attack and breakthrough plus the river's or fort's own bonus; defend: defence and dig-in; short_supply: days without supply), with the drawback named (more strength lost, no retreat, less attack); best_by_situation picks the highest gain per command power without a drawback; the current command power from the top bar. The order itself is not built yet (issue).", "inputSchema": {"type": "object", "properties": {}, "additionalProperties": false}})
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "ability = {
      force_attack = { type = army_leader allowed = { is_leading_army_group = no OWNER = { if = { limit = { has_dlc = \"Waking the Tiger\" } } } } cost = 0.22 duration = 168
        unit_modifiers = { offence = 0.2 breakthrough_factor = 0.25 org_damage_multiplier = -1.0 str_damage_multiplier = 0.6 cannot_retreat_while_attacking = 1 } }
      staff_office_plan = { type = army_leader cost = 0.12 duration = 168 unit_modifiers = { planning_speed = 4.0 } }
      makeshift_bridges = { type = army_leader cost = 0.1 duration = 168 unit_modifiers = { river_crossing_factor = -0.4 river = { attack = 0.2 } } }
      probing_attack = { type = army_leader cost = 0.1 duration = -1 unit_modifiers = { dont_lose_dig_in_on_attack = 1.0 offence = -0.2 } }
      extra_suplies = { type = army_leader cost = 0.2 duration = 168 cooldown = 504 unit_modifiers = { no_supply_grace = 168 } }
      rotating_reserves = { type = army_leader cost = 0.12 duration = 168 unit_modifiers = { land_reinforce_rate = 0.06 } }
    }";

    #[test]
    fn abilities_are_read_with_costs_durations_and_modifiers() {
        let all = parse(&script::parse(FILE));
        assert_eq!(all.len(), 6);
        let fa = &all[0];
        assert!(fa.not_for_army_groups && fa.cost == 0.22 && fa.duration_h == 168);
        assert_eq!(fa.dlc_in_allowed, vec!["Waking the Tiger".to_string()]);
        assert_eq!(all[2].nested, vec![("river".into(), "attack".into(), 0.2)]);
        assert_eq!(all[3].duration_h, -1);
        assert_eq!(all[4].cooldown_h, 504);
    }

    #[test]
    fn the_value_rule_picks_by_situation_and_keeps_drawbacks_for_battles_that_must_be_won() {
        let all = parse(&script::parse(FILE));
        assert_eq!(
            best(&all, "prepare_attack", false).unwrap().0.key,
            "staff_office_plan"
        );
        assert_eq!(
            best(&all, "attack_river", false).unwrap().0.key,
            "makeshift_bridges"
        );
        assert_eq!(
            best(&all, "short_supply", false).unwrap().0.key,
            "extra_suplies"
        );
        assert_eq!(
            best(&all, "reinforce", false).unwrap().0.key,
            "rotating_reserves"
        );
        // plain attack: only force_attack raises it, and it costs strength and retreat
        assert!(best(&all, "attack", false).is_none());
        assert_eq!(best(&all, "attack", true).unwrap().0.key, "force_attack");
        let (_, d) = worth(&all[3], "defend");
        assert_eq!(d.as_deref(), Some("attack -20%"));
        let a = answer(&all, Some(80.0));
        assert_eq!(
            a["best_by_situation"]["prepare_attack"]["ability"],
            "staff_office_plan"
        );
        assert_eq!(a["abilities"][4]["cooldown_days"], 21.0);
    }
}
