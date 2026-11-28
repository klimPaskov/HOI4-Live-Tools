//! Diplomacy through the in-process module (engineer 78, family `diplomacy`): the diplomacy view's actions as the
//! view sends them: the action object (sender = the player, receiver = the target, the game date; declare war: one of
//! the player's own war goals against the target, as the declare-war dialog lists them) is built with the action
//! class's own constructor, wrapped by the game's command maker, checked by the command's own "can do" (the action's
//! legality, the same call that enables the view's Send button) and posted. No view is opened. hoi4_diplomacy_act keeps
//! its name, schema and Verified read-back (the relation snapshot); the mouse path stays the fallback in the same call.
//! The family is off under `HOI4_BACKEND=auto` until its live session (S10-DIPLOMACY) passes.
use super::inproc::{Channel, Routed};
use super::inproc_units::{request, routed};
use crate::error::Result;
use serde_json::{json, Value};

/// The module op for a diplomacy key (diplomacy_keys::Spec::key); None = the mouse path (the action needs choices the
/// module does not make: a war goal type and states, a faction, equipment, volunteers).
pub(super) fn op_of(key: &str) -> Option<&'static str> {
    Some(match key {
        "guarantee" => "diplomacy.guarantee",
        "ask_military_access" => "diplomacy.military_access",
        "offer_military_access" => "diplomacy.offer_military_access",
        "non_aggression_pact" => "diplomacy.non_aggression",
        "improve_relations" => "diplomacy.improve_relations",
        "declare_war" => "diplomacy.declare_war",
        // engineer 125 (live 126, S25 V7): the attaché went by mouse under the in-process backend (no op here) and
        // clicked a province; the module's hand row diplomacy.send_attache sends it as the view does
        "send_attache" => "diplomacy.send_attache",
        // engineer 122: view rows whose action class the module's catalog builds (family diplomacy2, off under `auto`
        // until S27-DIPLOMACY-ANY passes); the toggled counterparts (lift_embargo, revoke_docking_rights) keep the mouse
        "embargo" => "diplomacy2.embargo",
        "ask_docking_rights" => "diplomacy2.docking_rights",
        "offer_docking_rights" => "diplomacy2.offer_docking_rights",
        "return_expeditions" => "diplomacy2.return_all_expeditionary_forces",
        _ => return None,
    })
}

/// The module family of an op ("diplomacy2.embargo" -> "diplomacy2").
pub(super) fn family_of(op: &str) -> &str {
    op.split('.').next().unwrap_or(op)
}

/// The canonical hoi4_diplomacy_act key of a catalog key (the game's action class in snake case, without C and Action),
/// where the tool already names that action otherwise; any other catalog key is its own canonical key, so a class the
/// game adds is addressed by its class name without a code change.
pub(super) fn canonical(game_key: &str) -> &str {
    match game_key {
        "military_access" => "ask_military_access",
        "improve_relation" => "improve_relations",
        "send_volunteer" => "send_volunteers",
        "recall_volunteer" => "recall_volunteers",
        "generate_war_goal" => "justify_war_goal",
        "request_licensed_production" => "request_licence_production",
        "lend_lease" => "start_lend_lease",
        "incoming_lend_lease" => "request_lend_lease",
        "send_expeditionary_force" => "expeditionary_force",
        "return_all_expeditionary_forces" => "return_expeditions",
        "call_ally" => "call_to_arms",
        "join_ally" => "join_allies",
        "docking_rights" => "ask_docking_rights",
        "ask_for_state_control" => "ask_state_control",
        other => other,
    }
}

/// The module's catalog: every diplomatic action class the game has, with its key, shape and op (scan facts only).
pub(super) fn catalog(ch: &mut Channel) -> Result<Value> {
    ch.call_raw(json!({"op":"diplomacy_actions"}))
}

/// The game's own "can do" for one action toward the country with this tag index, without posting (flag 2: the module
/// builds the action, asks the command's check, and frees both).
pub(super) fn check(ch: &mut Channel, op: &str, tag_index: i32) -> Result<Value> {
    let mut r = request(op, &[0], &[], &[i64::from(tag_index)], None)?;
    r["flag"] = json!(2);
    ch.post_raw(r)
}

/// engineer 134: the game's own check and the acceptance the diplomacy view shows (flag 3: the module builds the action,
/// asks the command's check, then the game's acceptance wrapper on the action: the total the receiver's AI answers by
/// and its parts with their texts), without posting.
pub(super) fn acceptance(ch: &mut Channel, op: &str, tag_index: i32) -> Result<Value> {
    let mut r = request(op, &[0], &[], &[i64::from(tag_index)], None)?;
    r["flag"] = json!(3);
    ch.post_raw(r)
}

/// The acceptance total and parts of a flag-3 reply, when the game's check passed and the module asked it.
pub(super) fn acceptance_of(reply: &Value) -> Option<(i64, Value)> {
    let total = reply["acceptance"].as_i64()?;
    Some((total, reply["acceptance_parts"].clone()))
}

/// The refusal an AI-decided request meets when the acceptance is not above 0 (the game's own test after its
/// acceptance wrapper): the parts name why (opinion, the base welcome, puppet relations ...).
pub(super) fn would_refuse(total: i64, parts: &Value) -> Option<String> {
    (total <= 0).then(|| {
        let why: Vec<String> = parts
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| {
                let v = p["value"].as_i64()?;
                let t = p["text"].as_str().unwrap_or("").trim().trim_end_matches(':');
                Some(format!("{t} {v:+}"))
            })
            .collect();
        format!(
            "the receiver's AI would refuse: acceptance {total} (it takes the request only above 0): {}; nothing was sent (pass try_anyway:true to send anyway)",
            if why.is_empty() { "no parts read".to_string() } else { why.join(", ") }
        )
    })
}

/// The catalog entry for a key (catalog or canonical), if any.
pub(super) fn entry<'a>(catalog: &'a Value, key: &str) -> Option<&'a Value> {
    catalog["actions"].as_array()?.iter().find(|a| {
        a["key"]
            .as_str()
            .is_some_and(|k| k == key || canonical(k) == key)
    })
}

/// The scripted diplomatic actions of the running content (common/scripted_diplomatic_actions, a mod's added actions):
/// their keys, read from the files.
pub(super) fn scripted_keys(content: &std::path::Path) -> Vec<String> {
    let mut keys = Vec::new();
    for e in crate::script::parse_dir(&content.join("common/scripted_diplomatic_actions")) {
        if e.key == "scripted_diplomatic_actions" {
            for a in e.val.block() {
                if !keys.contains(&a.key) {
                    keys.push(a.key.clone());
                }
            }
        }
    }
    keys
}

/// The listing hoi4_diplomacy returns with `catalog`: one row per action class (and per scripted action), with the
/// game's own answer where the module asked it. `checks` maps an op to the module's check reply; `usable` says whether
/// the op's family serves in this mode. A shape the module cannot build is listed as unsupported, never hidden.
pub(super) fn listing(
    catalog: &Value,
    checks: &[(String, Value)],
    scripted: &[String],
    view_key: impl Fn(&str) -> bool,
) -> Value {
    let mut rows = Vec::new();
    for a in catalog["actions"].as_array().into_iter().flatten() {
        let key = a["key"].as_str().unwrap_or("");
        let op = a["op"].as_str().unwrap_or("");
        let shape = a["shape"].as_str().unwrap_or("");
        let canon = canonical(key);
        let reply = checks.iter().find(|(o, _)| o == op).map(|(_, r)| r);
        let answer = reply.and_then(|r| r["per_object"][0]["result"].as_str());
        let can_do = answer.map(|x| x == "can_do");
        let route = if !op.is_empty() && a["on"].as_bool() == Some(true) {
            "in_process"
        } else if view_key(canon) {
            "mouse (the diplomacy view's row)"
        } else {
            "not postable yet"
        };
        rows.push(json!({"key":canon,"game_key":key,"class":a["class"],"shape":shape,"op":op,
            "can_do":can_do,"game_answer":answer.or_else(|| reply.and_then(|r| r["reason"].as_str())),"route":route}));
    }
    for k in scripted {
        rows.push(json!({"key":k,"game_key":k,"class":"25CScriptedDiplomaticAction","shape":"unsupported:scripted_type",
            "op":"","can_do":Value::Null,"game_answer":Value::Null,"route":"mouse (the diplomacy view's row)"}));
    }
    json!({"classes":catalog["classes"],"buildable":catalog["buildable"],"actions":rows})
}

/// One action against the country with this tag index (the game's own number for the tag, as the diplomacy view shows
/// it). The module names no object (objs = 0); the target travels as the one value.
pub(super) fn act(ch: &mut Channel, op: &str, tag_index: i32) -> Routed {
    if tag_index <= 0 {
        return Routed::Gui;
    }
    match request(op, &[0], &[], &[i64::from(tag_index)], None).and_then(|r| ch.post_raw(r)) {
        Ok(reply) => routed(reply, "this diplomatic action now"),
        Err(_) => Routed::Gui,
    }
}

/// engineer 125: the module request that justifies a war goal: vals = the receiver's tag index, then the war goal
/// type's key (the game's own key from common/wargoals, e.g. take_state) as character codes; provs = the states the goal
/// names (take-state goals; none for a goal on the whole country). The module finds the type in the game's own war goal
/// database by that key, builds the action with its own constructor and asks the game's check before posting.
pub(super) fn justify_request(tag_index: i32, war_goal_key: &str, states: &[i32]) -> Result<Value> {
    use crate::error::Error;
    if tag_index <= 0 {
        return Err(Error::new(
            "invalid_target",
            "the receiver has no tag index",
        ));
    }
    if war_goal_key.is_empty()
        || war_goal_key.len() > 39
        || !war_goal_key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
    {
        return Err(Error::new(
            "invalid_arguments",
            "war_goal_key: the game's war goal key (letters, digits, _; 1..39), e.g. take_state",
        ));
    }
    if states.len() > 64 || states.iter().any(|s| !(1..=100_000).contains(s)) {
        return Err(Error::new(
            "invalid_arguments",
            "state_ids: 0..64 state ids",
        ));
    }
    let mut vals = vec![i64::from(tag_index)];
    vals.extend(war_goal_key.bytes().map(i64::from));
    let mut r = request(JUSTIFY_OP, &[0], &[], &vals, None)?;
    if !states.is_empty() {
        r["provs"] = json!(states
            .iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(","));
    }
    Ok(r)
}

/// The module op that justifies a war goal (the catalog's generate_war_goal row, shape war_goal_type).
pub(super) const JUSTIFY_OP: &str = "diplomacy2.generate_war_goal";

/// engineer 125: post one justification; the module's reply on success. The game's own check refusing it (no such goal
/// toward that country now, not enough political power, a goal already justified ...) is the answer.
pub(super) fn justify(
    ch: &mut Channel,
    tag_index: i32,
    war_goal_key: &str,
    states: &[i32],
) -> Routed {
    match justify_request(tag_index, war_goal_key, states).and_then(|r| ch.post_raw(r)) {
        Ok(reply) => routed(reply, "this war goal justification now"),
        Err(_) => Routed::Gui,
    }
}

/// engineer 132: the module op of a licence request (the catalog's request_licensed_production row, shape variant_list).
pub(super) const LICENCE_OP: &str = "diplomacy2.request_licensed_production";
/// engineer 132: the most designs one licence request names (the module's own bound).
pub(super) const LICENCE_MAX: usize = 16;

/// engineer 132: the module request for a licence: vals = the licensor's tag index, divs = the licensor's designs
/// (CEquipmentVariant addresses, read from the licensor's own design list, as the licence dialog lists them). The module
/// checks each is a live design and the game's own check decides whether the licensor's designs may be licensed now.
pub(super) fn licence_request(tag_index: i32, designs: &[u64]) -> Result<Value> {
    use crate::error::Error;
    if tag_index <= 0 {
        return Err(Error::new(
            "invalid_target",
            "the licensor has no tag index",
        ));
    }
    if designs.is_empty() || designs.len() > LICENCE_MAX {
        return Err(Error::new(
            "invalid_arguments",
            format!("a licence request names 1..{LICENCE_MAX} of the licensor's designs"),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    if designs.iter().any(|d| *d == 0 || !seen.insert(*d)) {
        return Err(Error::new(
            "invalid_arguments",
            "a design is missing or listed twice",
        ));
    }
    request(LICENCE_OP, &[0], designs, &[i64::from(tag_index)], None)
}

/// engineer 132: the module op of a one-tag action (call to arms, join the ally's war), by the tool's key.
pub(super) fn tag_list_op(key: &str) -> Option<&'static str> {
    match key {
        "call_to_arms" | "call_ally" => Some("diplomacy2.call_ally"),
        "join_allies" | "join_ally" => Some("diplomacy2.join_ally"),
        _ => None,
    }
}

/// engineer 132: the module request of a one-tag action: vals = the receiver's tag index (the ally), then the enemy's
/// (the war the ally is called into, or joins). The game's own check decides (an ally, a war against that enemy, the
/// faction rules).
pub(super) fn tag_list_request(op: &str, tag_index: i32, enemy_index: i32) -> Result<Value> {
    use crate::error::Error;
    if tag_index <= 0 || enemy_index <= 0 {
        return Err(Error::new(
            "invalid_target",
            "the ally or the enemy has no tag index",
        ));
    }
    if tag_index == enemy_index {
        return Err(Error::new(
            "invalid_arguments",
            "the enemy is the ally itself",
        ));
    }
    request(
        op,
        &[0],
        &[],
        &[i64::from(tag_index), i64::from(enemy_index)],
        None,
    )
}

/// engineer 132: post one prepared request; the module's reply on success.
pub(super) fn post_prepared(ch: &mut Channel, req: Value, what: &str) -> Routed {
    match ch.post_raw(req) {
        Ok(reply) => routed(reply, what),
        Err(_) => Routed::Gui,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_licence_names_the_licensors_designs_and_a_call_to_arms_names_the_enemy() {
        let r = licence_request(42, &[0x6000_0000_1000, 0x6000_0000_2000]).unwrap();
        assert_eq!(r["cmd"], LICENCE_OP);
        assert_eq!(r["objs"], "0x0");
        assert_eq!(r["divs"], "0x600000001000,0x600000002000");
        assert!(r["vals"].to_string().contains("42"));
        assert!(licence_request(42, &[]).is_err());
        assert!(licence_request(42, &[1, 1]).is_err());
        assert!(licence_request(0, &[1]).is_err());
        assert!(licence_request(42, &[7u64; LICENCE_MAX + 1]).is_err());
        assert_eq!(tag_list_op("call_to_arms"), Some("diplomacy2.call_ally"));
        assert_eq!(tag_list_op("join_allies"), Some("diplomacy2.join_ally"));
        assert_eq!(tag_list_op("guarantee"), None);
        let c = tag_list_request("diplomacy2.call_ally", 12, 30).unwrap();
        assert_eq!(c["cmd"], "diplomacy2.call_ally");
        let v = c["vals"].to_string();
        assert!(v.contains("12") && v.contains("30"), "{v}");
        assert!(tag_list_request("diplomacy2.call_ally", 12, 12).is_err());
        assert!(tag_list_request("diplomacy2.call_ally", 12, 0).is_err());
    }

    #[test]
    fn the_simple_actions_and_declare_war_have_an_op_and_dialog_actions_do_not() {
        assert_eq!(op_of("guarantee"), Some("diplomacy.guarantee"));
        assert_eq!(op_of("declare_war"), Some("diplomacy.declare_war"));
        assert_eq!(op_of("send_attache"), Some("diplomacy.send_attache"));
        assert_eq!(
            op_of("non_aggression_pact"),
            Some("diplomacy.non_aggression")
        );
        for k in [
            "justify_war_goal",
            "join_faction",
            "start_lend_lease",
            "send_volunteers",
            "call_allies",
        ] {
            assert_eq!(
                op_of(k),
                None,
                "{k} needs the dialog's choices: the mouse path"
            );
        }
    }

    #[test]
    fn a_justification_carries_the_war_goal_key_as_codes_and_the_states_as_provinces() {
        let r = justify_request(7, "take_state", &[64, 65]).unwrap();
        assert_eq!(r["cmd"], JUSTIFY_OP);
        assert_eq!(r["vals"], "7,116,97,107,101,95,115,116,97,116,101");
        assert_eq!(r["provs"], "64,65");
        assert!(justify_request(7, "annex_everything", &[])
            .unwrap()
            .get("provs")
            .is_none());
        assert_eq!(
            justify_request(7, "Conquer!", &[]).unwrap_err().code,
            "invalid_arguments"
        );
        assert_eq!(
            justify_request(0, "take_state", &[]).unwrap_err().code,
            "invalid_target"
        );
    }

    fn cat() -> Value {
        json!({"classes":3,"buildable":1,"actions":[
            {"key":"embargo","class":"14CEmbargoAction","op":"diplomacy2.embargo","shape":"simple","type":12916,"on":true},
            {"key":"generate_war_goal","class":"22CGenerateWarGoalAction","op":"","shape":"unsupported:constructor_args_1","type":0,"on":false},
            {"key":"stage_coup","class":"16CStageCoupAction","op":"","shape":"unsupported:dialog_fields","type":0,"on":false}]})
    }

    #[test]
    fn catalog_keys_map_to_the_tools_names_and_new_classes_keep_their_own() {
        assert_eq!(canonical("generate_war_goal"), "justify_war_goal");
        assert_eq!(canonical("improve_relation"), "improve_relations");
        assert_eq!(
            canonical("a_new_action_of_a_later_game"),
            "a_new_action_of_a_later_game"
        );
        assert_eq!(family_of("diplomacy2.embargo"), "diplomacy2");
        assert_eq!(op_of("embargo"), Some("diplomacy2.embargo"));
        assert_eq!(
            op_of("lift_embargo"),
            None,
            "the toggled counterpart keeps the mouse"
        );
        let c = cat();
        assert_eq!(
            entry(&c, "justify_war_goal").unwrap()["class"],
            "22CGenerateWarGoalAction"
        );
        assert_eq!(entry(&c, "embargo").unwrap()["op"], "diplomacy2.embargo");
        assert!(entry(&c, "nothing").is_none());
    }

    #[test]
    fn the_listing_shows_every_class_with_the_games_answer_and_never_hides_an_unsupported_shape() {
        let checks = vec![(
            "diplomacy2.embargo".to_string(),
            json!({"result":"can_do","per_object":[{"obj":"0x0","result":"can_do"}]}),
        )];
        let l = listing(&cat(), &checks, &["mod_alliance_offer".to_string()], |k| {
            k == "justify_war_goal" || k == "stage_coup"
        });
        let rows = l["actions"].as_array().unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(rows[0]["can_do"], true);
        assert_eq!(rows[0]["route"], "in_process");
        assert_eq!(rows[1]["key"], "justify_war_goal");
        assert_eq!(rows[1]["shape"], "unsupported:constructor_args_1");
        assert_eq!(rows[1]["route"], "mouse (the diplomacy view's row)");
        assert!(rows[1]["can_do"].is_null());
        assert_eq!(rows[3]["shape"], "unsupported:scripted_type");
        let refused = vec![(
            "diplomacy2.embargo".to_string(),
            json!({"result":"refused","per_object":[{"obj":"0x0","result":"refused_by_game_check"}]}),
        )];
        let l = listing(&cat(), &refused, &[], |_| false);
        assert_eq!(l["actions"][0]["can_do"], false);
        assert_eq!(l["actions"][0]["game_answer"], "refused_by_game_check");
        assert_eq!(l["actions"][2]["route"], "not postable yet");
    }

    #[test]
    fn an_acceptance_at_or_below_zero_is_named_as_the_ais_refusal() {
        // s129e V7: the attache to Nationalist Spain: the base welcome and the low-opinion penalty
        let reply = json!({"posted":0,"acceptance":-38,"acceptance_parts":[
            {"value":50,"text":"The help is welcome: "},{"value":-100,"text":"Too low opinion about us: "},{"value":12,"text":"Opinion"}]});
        let (total, parts) = acceptance_of(&reply).unwrap();
        let why = would_refuse(total, &parts).unwrap();
        assert!(why.contains("acceptance -38") && why.contains("Too low opinion about us -100"));
        assert!(would_refuse(25, &parts).is_none());
        assert!(acceptance_of(&json!({"posted":0})).is_none());
    }

    #[test]
    fn a_check_request_carries_flag_two() {
        let mut r = request("diplomacy2.embargo", &[0], &[], &[10], None).unwrap();
        r["flag"] = json!(2);
        assert_eq!(r["flag"], 2);
        assert_eq!(r["cmd"], "diplomacy2.embargo");
    }

    #[test]
    fn a_diplomacy_request_names_no_object_and_carries_the_target_tag() {
        let r = request("diplomacy.guarantee", &[0], &[], &[10], None).unwrap();
        assert_eq!(r["cmd"], "diplomacy.guarantee");
        assert_eq!(r["objs"], "0x0");
        assert_eq!(r["vals"], "10");
        assert!(r.get("obj2").is_none() && r.get("divs").is_none());
    }
}
