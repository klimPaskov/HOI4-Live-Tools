//! Holds through the in-process module (engineer 79, family `holds`): the AREA DEFENCE order, as the battle-plan map
//! tool posts it. The tool's area button only arms the tool; every left click on a state posts one
//! EditAreaDefenseState(army, order id, the state, the tool's settings) through the tool's own constructor: order id 0
//! creates a NEW area-defence order of the army with that state, an order's id toggles the state in it. So an order over
//! N states is N clicks: the first creates the order (the module reads back its new id), the others add to that id. The
//! module refuses a state already in the order (the click would remove it) and checks the army and the order with the
//! game's own lookups; the states are the MCP's own-controlled reads. hoi4_army_area_defense keeps its name, schema and
//! Verified read-back (new battle-plan orders on the army); the mouse path stays the fallback in the same call. The
//! family is off under `HOI4_BACKEND=auto` until its live session (S11-HOLDS) passes.
use super::inproc::{Channel, Routed};
use super::inproc_units::{request, routed};
use serde_json::{json, Value};

/// The module's request for one batch of clicks: the order's id (0 = a new order, then exactly one state) and states.
pub(super) fn area_request(
    group: u64,
    order_id: i32,
    states: &[i32],
) -> crate::error::Result<Value> {
    let mut vals = vec![i64::from(order_id)];
    vals.extend(states.iter().map(|s| i64::from(*s)));
    request("holds.area_defense", &[group], &[], &vals, None)
}

/// The id of the area order the module read back (a new order's id, or the order the states went into).
pub(super) fn area_order_id(reply: &Value) -> Option<i32> {
    reply["area_order_id"]
        .as_i64()
        .filter(|v| *v > 0)
        .and_then(|v| i32::try_from(v).ok())
}

/// An area-defence order of `group` over `states` (1..8, own-controlled, distinct): the first click makes the order,
/// the rest go into its id. Done carries both module replies; Refused = the game's own check said no; Gui = the module
/// could not post the first click (the mouse path then draws the whole order). After the first click the order exists,
/// so a later failure is reported in the reply, never re-drawn by the mouse (no second order).
pub(super) fn area(ch: &mut Channel, group: u64, states: &[i32]) -> Routed {
    let Some((first, rest)) = states.split_first() else {
        return Routed::Gui;
    };
    let reply = match area_request(group, 0, &[*first]).and_then(|r| ch.post_raw(r)) {
        Ok(r) => r,
        Err(_) => return Routed::Gui,
    };
    let first_reply = match routed(reply, "an area-defence order over this state") {
        Routed::Done(v) => v,
        other => return other,
    };
    if rest.is_empty() {
        return Routed::Done(first_reply);
    }
    let Some(id) = area_order_id(&first_reply["module"]) else {
        return Routed::Done(json!({"backend":"inprocess","module":first_reply["module"],
            "states_not_added":rest,"reason":"the new area order's id was not read back; the order holds the first state"}));
    };
    let more = area_request(group, id, rest)
        .and_then(|r| ch.post_raw(r))
        .unwrap_or_else(|e| json!({"ok":false,"error":e.message}));
    Routed::Done(json!({"backend":"inprocess","module":first_reply["module"],"added":more}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_order_names_order_zero_and_one_state() {
        let r = area_request(0x6000_1000, 0, &[64]).unwrap();
        assert_eq!(r["cmd"], "holds.area_defense");
        assert_eq!(r["objs"], "0x60001000");
        assert_eq!(r["vals"], "0,64");
        assert!(r.get("divs").is_none() && r.get("obj2").is_none());
    }

    #[test]
    fn the_other_states_go_into_the_new_orders_id() {
        let r = area_request(0x6000_1000, 4711, &[65, 66, 67]).unwrap();
        assert_eq!(r["vals"], "4711,65,66,67");
        assert_eq!(
            area_order_id(&json!({"area_order_id":4711,"states_added":1})),
            Some(4711)
        );
        assert_eq!(area_order_id(&json!({"posted":1})), None);
        assert_eq!(area_order_id(&json!({"area_order_id":0})), None);
    }
}
