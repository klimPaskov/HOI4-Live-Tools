//! Command limits from the game's own files, so a roster read can say how many divisions an army
//! commander leads without penalty and how many armies a field marshal does: the base defines
//! (`CORPS_COMMANDER_DIVISIONS_CAP`, `FIELD_MARSHAL_ARMIES_CAP` in common/defines) plus the
//! `max_commander_army_size` / `max_army_group_size` modifiers of the leader's traits
//! (common/unit_leader, any modifier block of the trait). Read from the install and its mods, so
//! a mod that changes either is followed.
use super::mgmt;
use crate::platform::Process;
use crate::script::{self, Entry, Val};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// The vanilla bases (1.19.4 defines), used when the defines file gives none.
const DIVISIONS_BASE: i64 = 24;
const ARMIES_BASE: i64 = 5;

#[derive(Debug, Default)]
pub(super) struct Limits {
    divisions: i64,
    armies: i64,
    army_size: HashMap<String, i64>,
    group_size: HashMap<String, i64>,
}

impl Limits {
    /// Divisions an army commander with these traits leads without penalty.
    pub(super) fn divisions(&self, traits: &[String]) -> i64 {
        self.divisions
            + traits
                .iter()
                .filter_map(|t| self.army_size.get(t))
                .sum::<i64>()
    }
    /// Armies a field marshal with these traits leads without penalty.
    pub(super) fn armies(&self, traits: &[String]) -> i64 {
        self.armies
            + traits
                .iter()
                .filter_map(|t| self.group_size.get(t))
                .sum::<i64>()
    }
}

/// Sum of `key` anywhere inside one trait's body (its modifier blocks).
fn sum_key(entries: &[Entry], key: &str) -> i64 {
    entries
        .iter()
        .map(|e| match &e.val {
            Val::Scalar(v) if e.key == key => v.parse::<f64>().map_or(0, |n| n.round() as i64),
            Val::Block(inner) => sum_key(inner, key),
            Val::Scalar(_) => 0,
        })
        .sum()
}

pub(super) fn from_scripts(entries: &[Entry], defines: &str) -> Limits {
    let mut out = Limits {
        divisions: super::leader_act::define(defines, "CORPS_COMMANDER_DIVISIONS_CAP")
            .map_or(DIVISIONS_BASE, |n| n as i64),
        armies: super::leader_act::define(defines, "FIELD_MARSHAL_ARMIES_CAP")
            .map_or(ARMIES_BASE, |n| n as i64),
        ..Limits::default()
    };
    for top in entries.iter().filter(|e| e.key == "leader_traits") {
        for t in top.val.block() {
            let body = t.val.block();
            let army = sum_key(body, "max_commander_army_size");
            if army != 0 {
                out.army_size.insert(t.key.clone(), army);
            }
            let group = sum_key(body, "max_army_group_size");
            if group != 0 {
                out.group_size.insert(t.key.clone(), group);
            }
        }
    }
    out
}

fn load(dir: &Path) -> Limits {
    let defines =
        std::fs::read_to_string(dir.join("common/defines/00_defines.lua")).unwrap_or_default();
    from_scripts(
        &script::parse_dir(&dir.join("common/unit_leader")),
        &defines,
    )
}

/// The limits of the running game (cached per install).
pub(super) fn limits(process: &dyn Process) -> Option<Arc<Limits>> {
    let dir = mgmt::game_dir(process).ok()?;
    Some(script::cached(&dir, "command_limits", load))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_come_from_the_defines_and_the_traits_modifier_blocks() {
        let traits = script::parse(
            "leader_traits = {
                organizer = { corps_commander_modifier = { max_commander_army_size = 6 } }
                expert_delegator = { type = field_marshal field_marshal_modifier = { max_army_group_size = 2 } }
                politically_connected = { modifier = { max_commander_army_size = -18 max_army_group_size = -4 } }
                brawler = { modifier = { attack = 1 } }
            }",
        );
        let l = from_scripts(
            &traits,
            "\tCORPS_COMMANDER_DIVISIONS_CAP = 24,\t-- x\n\tFIELD_MARSHAL_ARMIES_CAP = 5,\n",
        );
        assert_eq!(l.divisions(&[]), 24);
        assert_eq!(l.divisions(&["organizer".into(), "brawler".into()]), 30);
        assert_eq!(l.divisions(&["politically_connected".into()]), 6);
        assert_eq!(l.armies(&["expert_delegator".into()]), 7);
        assert_eq!(l.armies(&["politically_connected".into()]), 1);
        // no defines file: the vanilla bases
        assert_eq!(from_scripts(&[], "").divisions(&[]), 24);
        assert_eq!(from_scripts(&[], "").armies(&[]), 5);
    }
}
