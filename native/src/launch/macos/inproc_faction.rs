//! The faction leader's own choices through the in-process module (engineer 131, family `faction`), each the command
//! the faction view posts, checked by the game's own "can do" before it is posted:
//! * `faction.goal`: NFactions::CAddFactionGoalCommand(own tag, the goal) into a free slot of the goal's category; the
//!   module finds the goal by its key in the game's own goal database (the token map the command's reader resolves a
//!   goal through) and reads the faction's goal list back;
//! * `faction.spymaster`: CBecomeSpyMasterCommand(own tag); the game's check wants a faction whose spymaster slot is
//!   empty and the execution takes the initiative the defines name.
//!
//! The goals the tool offers come from the game's own `common/factions/goals` scripts (every top-level entry with a
//! `category`), so a mod's goals are offered without a code change. No mouse path exists; the family is off under
//! `HOI4_BACKEND=auto` until its live session (S32-FACTION) passes.
use super::inproc::{Channel, Routed};
use super::inproc_units::{request, routed};
use super::{loc, mgmt};
use crate::{
    error::{Error, Result},
    platform::Process,
    script::{self, Block},
};
use serde_json::{json, Value};
use std::{path::Path, sync::Arc};

pub(super) const GOAL_OP: &str = "faction.goal";
pub(super) const SPYMASTER_OP: &str = "faction.spymaster";
/// The module takes a key of at most 39 characters (letters, digits, _).
const KEY_MAX: usize = 39;

/// One faction goal of the scripts.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Goal {
    pub key: String,
    /// `short_term`, `medium_term`, `long_term` (as written; a mod may add others).
    pub category: String,
    /// The script's `group` (a focus filter key), when given.
    pub group: Option<String>,
    /// The localised name, else the key.
    pub name: String,
}

/// The goals of parsed `common/factions/goals` entries: every top-level block with a `category`.
pub(super) fn goals_from(
    entries: &[script::Entry],
    names: &dyn Fn(&str) -> Option<String>,
) -> Vec<Goal> {
    entries
        .iter()
        .filter_map(|e| {
            let script::Val::Block(body) = &e.val else {
                return None;
            };
            let category = body.text("category")?.to_string();
            let name_key = body.text("name").unwrap_or(&e.key);
            Some(Goal {
                key: e.key.clone(),
                category,
                group: body.text("group").map(str::to_string),
                name: names(name_key).unwrap_or_else(|| e.key.clone()),
            })
        })
        .collect()
}

fn load(dir: &Path) -> Vec<Goal> {
    let entries = script::parse_dir(&dir.join("common/factions/goals"));
    let keys: Vec<&str> = entries
        .iter()
        .filter_map(|e| match &e.val {
            script::Val::Block(b) => b.text("name"),
            script::Val::Scalar(_) => None,
        })
        .collect();
    let names = loc::lookup(dir, &keys);
    goals_from(&entries, &|k| names.get(k).cloned())
}

/// The game's faction goals (cached per install).
pub(super) fn goals(process: &dyn Process) -> Result<Arc<Vec<Goal>>> {
    let dir = mgmt::game_dir(process)?;
    let g = script::cached(&dir, "faction_goals", load);
    if g.is_empty() {
        return Err(Error::new(
            "catalog_unavailable",
            "The game's common/factions/goals scripts list no faction goal",
        ));
    }
    Ok(g)
}

/// The asked goal (script key or localised name); unknown names the closest keys.
pub(super) fn resolve<'a>(goals: &'a [Goal], asked: &str) -> Result<&'a Goal> {
    let asked = asked.trim();
    if let Some(g) = goals
        .iter()
        .find(|g| g.key == asked)
        .or_else(|| goals.iter().find(|g| loc::same(&g.name, asked)))
    {
        return Ok(g);
    }
    let low = asked.to_ascii_lowercase();
    let close: Vec<&str> = goals
        .iter()
        .filter(|g| {
            low.split(|c: char| !c.is_ascii_alphanumeric())
                .filter(|w| w.len() > 3)
                .any(|w| g.key.contains(w) || g.name.to_ascii_lowercase().contains(w))
        })
        .take(8)
        .map(|g| g.key.as_str())
        .collect();
    Err(Error::new(
        "faction_goal_unknown",
        format!(
            "{asked} is no faction goal of this game (common/factions/goals){}",
            if close.is_empty() {
                String::new()
            } else {
                format!("; closest: {}", close.join(", "))
            }
        ),
    ))
}

/// The goals as the tool lists them (key, name, category, group).
pub(super) fn list_json(goals: &[Goal]) -> Value {
    json!(goals
        .iter()
        .map(|g| json!({"key":g.key,"name":g.name,"category":g.category,"group":g.group}))
        .collect::<Vec<_>>())
}

/// The module request that sets a goal: no object (objs = 0), the goal's key as character codes.
pub(super) fn goal_request(key: &str) -> Result<Value> {
    if key.is_empty()
        || key.len() > KEY_MAX
        || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(Error::new(
            "invalid_arguments",
            "faction goal key: letters, digits and _ (1..39)",
        ));
    }
    let vals: Vec<i64> = key.bytes().map(i64::from).collect();
    request(GOAL_OP, &[0], &[], &vals, None)
}

/// The module request that makes the player's country the faction's spymaster (the module takes the own country).
pub(super) fn spymaster_request() -> Result<Value> {
    request(SPYMASTER_OP, &[0], &[], &[], None)
}

/// Post one request: Done (the module's reply), Refused (the game's own check: the answer), Gui (the module refused on
/// its own checks or did not answer; there is no mouse path, so the caller reports it).
pub(super) fn post(ch: &mut Channel, req: Value, what: &str) -> Routed {
    match ch.post_raw(req) {
        Ok(reply) => routed(reply, what),
        Err(_) => Routed::Gui,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<Goal> {
        let text = r#"
        faction_goal_one_germany = { name = one_germany_goal_name category = short_term group = FOCUS_FILTER_ANNEXATION }
        industrial_expansion = { name = industrial_expansion_goal_name category = medium_term }
        manifest_x = { name = m visible = { always = yes } }
        "#;
        let names = |k: &str| match k {
            "one_germany_goal_name" => Some("Unite the German States".to_string()),
            "industrial_expansion_goal_name" => Some("Industrial Expansion".to_string()),
            _ => None,
        };
        goals_from(&script::parse(text), &names)
    }

    #[test]
    fn goals_are_the_entries_with_a_category_named_from_localisation() {
        let g = sample();
        assert_eq!(
            g.len(),
            2,
            "a block without a category (a manifest) is no goal"
        );
        assert_eq!(g[0].key, "faction_goal_one_germany");
        assert_eq!(g[0].category, "short_term");
        assert_eq!(g[0].group.as_deref(), Some("FOCUS_FILTER_ANNEXATION"));
        assert_eq!(g[1].name, "Industrial Expansion");
        assert!(g[1].group.is_none());
    }

    #[test]
    fn a_goal_resolves_by_key_or_name_and_an_unknown_names_the_closest() {
        let g = sample();
        assert_eq!(
            resolve(&g, "industrial_expansion").unwrap().key,
            "industrial_expansion"
        );
        assert_eq!(
            resolve(&g, "industrial expansion").unwrap().key,
            "industrial_expansion"
        );
        let e = resolve(&g, "industrial growth").unwrap_err();
        assert_eq!(e.code, "faction_goal_unknown");
        assert!(e.message.contains("industrial_expansion"), "{}", e.message);
    }

    #[test]
    fn requests_carry_no_object_and_the_goal_key_as_codes() {
        let r = goal_request("ab_1").unwrap();
        assert_eq!(r["cmd"], GOAL_OP);
        assert_eq!(r["objs"], "0x0");
        assert_eq!(r["vals"], "97,98,95,49");
        assert!(goal_request("bad key").is_err());
        assert!(goal_request(&"a".repeat(40)).is_err());
        let s = spymaster_request().unwrap();
        assert_eq!(s["cmd"], SPYMASTER_OP);
        assert_eq!(s["objs"], "0x0");
        assert!(s.get("vals").is_none());
    }
}
