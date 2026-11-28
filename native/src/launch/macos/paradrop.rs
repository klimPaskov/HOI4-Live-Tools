//! `hoi4_paradrop` (engineer 106): an own army's paratroopers dropped from an own air base onto a province, by the
//! game's own battle-plan commands through the in-process module (family orders2):
//!
//! 1. `orders2.paradrop_source` with order 0 and the divisions: the paradrop tool's first click. The game removes the
//!    group's area-defence orders, creates a new paradrop order (kind 4), sets its departure province (the air base's)
//!    and assigns the divisions (1.20.0.0 exec 0x10094bba0). The module reads the new order's id back.
//! 2. `orders2.paradrop_target` with that id and the drop province: the second click.
//! 3. `orders.execute` with that id (unless `execute` is false): the plan bar's execute.
//!
//! The game's own checks decide what may drop (paratrooper divisions, transport planes in range, the air superiority
//! ratio NDefines.NCountry.PARADROP_AIR_SUPERIORITY_RATIO): a drop it refuses is reported, never forced. OFF in auto
//! (HOI4_BACKEND unset) until its live session S21-PARADROP passes (`LIVE_PASSED`); HOI4_BACKEND=inprocess runs it.
//! There is no mouse path. FAIR: at most 18 divisions named in one drop (fair_guard fair.paratroopers; a nation's 18);
//! a capital drop to force a capitulation without a supportable operation is META only (MX12), which the plan step
//! decides (the tool does not know the operation).

use super::airread::Base;
use super::inproc::{self, Channel};
use super::inproc_orders;
use crate::error::{Error, Result};
use serde_json::{json, Value};

/// Whether S21-PARADROP passed live: until then the tool runs only with HOI4_BACKEND=inprocess.
pub(super) const LIVE_PASSED: bool = false;

pub(super) fn schema() -> Value {
    json!({"name":"hoi4_paradrop","description":"Drop an own army's paratroopers from an own air base onto a province with the game's own battle-plan commands (in-process module, family orders2): the paradrop tool's source click (a new paradrop order with the divisions; the game removes the army's area-defence orders), its target click, then the plan bar's execute (execute:false leaves the order drawn). `from_state` names the state of the departure air base (an own air base). `division_ids` limits the drop to those divisions of the army (ids as hoi4_armies lists them; default: all its divisions; the game drops only what can drop). The game's own checks decide: paratrooper divisions, transport planes within range of the target, the air superiority the paradrop needs (PARADROP_AIR_SUPERIORITY_RATIO); a refused drop is reported (paradrop_refused). Refused before any post: army_unknown, air_base_unknown, paradrop_unvalidated (off unless HOI4_BACKEND=inprocess until its live session S21-PARADROP passes), in_process_unavailable. Verified when the army has a paradrop order with the read-back order id (executing when asked).","inputSchema":{"type":"object","properties":{"army":{"type":"string","maxLength":128},"from_state":{"type":"integer","minimum":1,"maximum":4095},"target_province":{"type":"integer","minimum":1,"maximum":100000},"division_ids":{"type":"array","items":{"type":"string","maxLength":64},"minItems":1,"maxItems":64},"execute":{"type":"boolean"},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["army","from_state","target_province","request_id","lease_token"],"additionalProperties":false}})
}

/// Whether the tool may run in this backend mode.
pub(super) fn allowed_mode(mode: inproc::Mode) -> Result<()> {
    match mode {
        inproc::Mode::InProcess => Ok(()),
        inproc::Mode::Auto if LIVE_PASSED => Ok(()),
        _ => Err(Error::new(
            "paradrop_unvalidated",
            "hoi4_paradrop runs through the in-process module only, and only with HOI4_BACKEND=inprocess until its \
             live session S21-PARADROP passes (docs/LIVE_VALIDATION_QUEUE.md); nothing was posted",
        )),
    }
}

/// The departure province: the own air base's province in `state`.
pub(super) fn source_province(bases: &[Base], state: i32) -> Result<i32> {
    bases
        .iter()
        .find(|b| b.state == Some(state) && b.province > 0)
        .map(|b| b.province)
        .ok_or_else(|| {
            let mut own: Vec<i32> = bases.iter().filter_map(|b| b.state).collect();
            own.sort_unstable();
            own.dedup();
            Error::new(
                "air_base_unknown",
                format!("state {state} has no own air base (own air base states: {own:?})"),
            )
        })
}

fn check(reply: &Value, what: &str) -> Result<()> {
    if inproc::game_refused(reply) {
        return Err(Error::new(
            "paradrop_refused",
            format!(
                "the game's own check refused the {what}: {}",
                reply["reason"].as_str().unwrap_or("?")
            ),
        ));
    }
    if !inproc::all_posted(reply) {
        return Err(Error::new(
            "inproc_refused",
            format!(
                "the module did not post the {what}: {}",
                reply["reason"]
                    .as_str()
                    .or(reply["error"].as_str())
                    .unwrap_or("?")
            ),
        ));
    }
    Ok(())
}

/// The three posts; returns the module's replies and the new order's id.
pub(super) fn drop(
    ch: &mut Channel,
    group: u64,
    source: i32,
    target: i32,
    divisions: &[u64],
    execute: bool,
) -> Result<(i32, Value)> {
    if divisions.is_empty() {
        return Err(Error::new("invalid_arguments", "no division to drop"));
    }
    let req = inproc_orders::request2(
        "orders2.paradrop_source",
        group,
        &[0, i64::from(source)],
        &[],
        divisions,
        None,
    )?;
    let first = ch.orders_post(req)?;
    check(&first, "paradrop order (source)")?;
    let order = first["order_id"]
        .as_i64()
        .filter(|id| *id > 0)
        .ok_or_else(|| {
            Error::new(
                "inproc_refused",
                "the module read back no new paradrop order id",
            )
        })? as i32;
    let req = inproc_orders::request2(
        "orders2.paradrop_target",
        group,
        &[i64::from(order), i64::from(target)],
        &[],
        &[],
        None,
    )?;
    let second = ch.orders_post(req)?;
    check(&second, "drop province (target)")?;
    let third = if execute {
        let req = inproc_orders::request("orders.execute", group, order, &[], None, Some(1))?;
        let r = ch.orders_post(req)?;
        check(&r, "execute")?;
        r
    } else {
        Value::Null
    };
    Ok((
        order,
        json!({"order_id":order,"source":first,"target":second,"execute":third}),
    ))
}

/// The read-back: the group's paradrop order with that id and whether it executes. `orders`: (kind, id, executing)
/// of the group's top orders.
pub(super) fn settled(orders: &[(i32, i32, bool)], order: i32, execute: bool) -> Option<bool> {
    let o = orders.iter().find(|(k, id, _)| *k == 4 && *id == order)?;
    Some(!execute || o.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base(state: i32, province: i32) -> Base {
        Base {
            id: state,
            state: Some(state),
            province,
            capacity: 200,
            region_id: None,
            address: 0,
            region_address: 0,
        }
    }

    #[test]
    fn source_is_the_air_base_province() {
        let bases = [base(64, 6521), base(66, 3000)];
        assert_eq!(source_province(&bases, 66).unwrap(), 3000);
        let e = source_province(&bases, 9).unwrap_err();
        assert_eq!(e.code, "air_base_unknown");
        assert!(e.message.contains("[64, 66]"));
    }

    #[test]
    fn off_in_auto_until_live() {
        assert!(allowed_mode(inproc::Mode::InProcess).is_ok());
        if !LIVE_PASSED {
            let e = allowed_mode(inproc::Mode::Auto).unwrap_err();
            assert_eq!(e.code, "paradrop_unvalidated");
        }
        assert!(allowed_mode(inproc::Mode::Gui).is_err());
    }

    #[test]
    fn settled_needs_the_paradrop_order_and_execution() {
        let orders = [(2, 5, false), (4, 9, true), (4, 11, false)];
        assert_eq!(settled(&orders, 9, true), Some(true));
        assert_eq!(settled(&orders, 11, true), Some(false));
        assert_eq!(settled(&orders, 11, false), Some(true));
        assert_eq!(settled(&orders, 5, false), None);
    }
}
