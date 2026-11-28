//! The navy through the in-process module (engineer 85, family `navy`): the commands the navy view posts, each after
//! the module's own checks (a live object of the command's class, owned by the player's country) and the game's own
//! "can do":
//! * `navy.mission`: the mission bar's button for the selected task forces (ONE command, the button's mission code);
//! * `navy.add_region` / `navy.remove_region`: a fleet's area of operation by strategic region id (the right click on a
//!   sea region, the remove tool);
//! * `navy.leader`: the fleet panel's commander list (an admiral, or none = unassign);
//! * `navy.assign`: task forces into another fleet (a fleet card as the drop target);
//! * `navy.merge`: the toolbar's merge of the selected task forces.
//!
//! The tools (hoi4_fleet_mission, hoi4_fleet_region, hoi4_fleet_admiral, hoi4_fleet_assign, hoi4_fleet_merge) keep
//! their names, schemas and Verified read-backs; the mouse path stays the fallback inside the same call. The family is
//! off under `HOI4_BACKEND=auto` until its live session (S17-NAVY) passes.
use super::inproc::{Channel, Routed};
use super::inproc_units::{request, routed};
use super::naval::{Fleet, NavyObjects};
use crate::{economy, error::Result, platform::Process};

/// The own fleets' and admirals' object addresses (one read).
pub(super) fn objects(process: &dyn Process) -> Result<NavyObjects> {
    super::ui::read(process, |r, image| {
        let (country, _) = economy::own_production(r, image)?;
        super::naval::fleets_objects(r, image, country)
    })
}

/// True when the address read has the shape of the fleet read (same fleets, same task force counts): the two reads
/// are separate, and a fleet founded or merged between them would pair a name with the wrong object.
pub(super) fn same_shape(fleets: &[Fleet], objs: &NavyObjects) -> bool {
    fleets.len() == objs.fleets.len()
        && fleets
            .iter()
            .zip(&objs.fleets)
            .all(|(f, (_, t))| f.task_forces.len() == t.len())
}

/// The fleet object at the fleet read's position of `fleet` (the planner's exact fleet).
pub(super) fn fleet_object(fleets: &[Fleet], objs: &NavyObjects, fleet: &Fleet) -> Option<u64> {
    if !same_shape(fleets, objs) {
        return None;
    }
    let i = fleets.iter().position(|f| std::ptr::eq(f, fleet))?;
    Some(objs.fleets[i].0)
}

/// The task force object of `tf` (a task force of the fleet read).
pub(super) fn task_force_object(
    fleets: &[Fleet],
    objs: &NavyObjects,
    tf: &super::naval::TaskForce,
) -> Option<u64> {
    if !same_shape(fleets, objs) {
        return None;
    }
    for (i, f) in fleets.iter().enumerate() {
        if let Some(k) = f.task_forces.iter().position(|t| std::ptr::eq(t, tf)) {
            return Some(objs.fleets[i].1[k]);
        }
    }
    None
}

/// The admiral object named `name` (the name the fleet read shows; exactly one).
pub(super) fn admiral_object(objs: &NavyObjects, name: &str) -> Option<u64> {
    let want = name.trim().to_lowercase();
    let hits: Vec<u64> = objs
        .admirals
        .iter()
        .filter(|(_, n)| n.trim().to_lowercase() == want)
        .map(|(a, _)| *a)
        .collect();
    (hits.len() == 1).then(|| hits[0])
}

fn post(ch: &mut Channel, req: Result<serde_json::Value>, what: &str) -> Routed {
    match req.and_then(|r| ch.post_raw(r)) {
        Ok(reply) => routed(reply, what),
        Err(_) => Routed::Gui,
    }
}

/// The mission bar's button for these task forces (one command). Codes as the task force's mission code reads them.
pub(super) fn mission(ch: &mut Channel, task_forces: &[u64], code: i32) -> Routed {
    // 8 is the reserve (no button); 5 and 6 (mines) are enabled by technology, a rule of the button the command's own
    // check does not repeat; 7 (training) carries a byte the button sets from a game setting the module does not read:
    // the mouse path serves those (its button reads the game's own state)
    if task_forces.is_empty() || task_forces.len() > 32 || !matches!(code, 0..=4 | 9) {
        return Routed::Gui;
    }
    post(
        ch,
        request("navy.mission", task_forces, &[], &[i64::from(code)], None),
        "this mission for this task force now",
    )
}

/// A sea region into (or out of) the fleet's area of operation, by strategic region id.
pub(super) fn region(ch: &mut Channel, fleet: u64, region_id: i32, remove: bool) -> Routed {
    if region_id < 1 {
        return Routed::Gui;
    }
    let op = if remove {
        "navy.remove_region"
    } else {
        "navy.add_region"
    };
    post(
        ch,
        request(op, &[fleet], &[], &[i64::from(region_id)], None),
        "this sea region for this fleet now",
    )
}

/// An admiral in command of the fleet (None: unassign).
pub(super) fn leader(ch: &mut Channel, fleet: u64, admiral: Option<u64>) -> Routed {
    post(
        ch,
        request("navy.leader", &[fleet], &[], &[], admiral),
        "this admiral for this fleet now",
    )
}

/// Task forces into the receiving fleet.
pub(super) fn assign(ch: &mut Channel, fleet: u64, task_forces: &[u64]) -> Routed {
    if task_forces.is_empty() || task_forces.len() > 32 {
        return Routed::Gui;
    }
    post(
        ch,
        request("navy.assign", &[fleet], task_forces, &[], None),
        "these task forces into this fleet now",
    )
}

/// The toolbar's merge of these task forces (at least two).
pub(super) fn merge(ch: &mut Channel, task_forces: &[u64]) -> Routed {
    if task_forces.len() < 2 || task_forces.len() > 32 {
        return Routed::Gui;
    }
    post(
        ch,
        request("navy.merge", task_forces, &[], &[], None),
        "merging these task forces now",
    )
}

/// The strategic region id of a sea region named as the game shows it (or by its map file name or id).
pub(super) fn region_id(process: &dyn Process, region: &str) -> Option<i32> {
    let dir = process
        .identity()
        .and_then(|i| super::mapdata::game_dir(&i.exe).ok())?;
    super::airact::pick_region(&super::airread::region_display_names(&dir), region)
        .or_else(|_| super::airact::pick_region(&super::airread::region_names(&dir), region))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::super::naval::TaskForce;
    use super::*;

    fn tf(name: &str) -> TaskForce {
        TaskForce {
            name: name.into(),
            place: None,
            mission_code: 0,
            ships: Vec::new(),
        }
    }
    fn fleet(name: &str, tfs: &[&str]) -> Fleet {
        Fleet {
            name: name.into(),
            home_port: None,
            regions: Vec::new(),
            admiral: None,
            task_forces: tfs.iter().map(|t| tf(t)).collect(),
        }
    }

    #[test]
    fn objects_pair_with_the_fleet_read_by_position_only_when_the_shapes_agree() {
        let fleets = vec![fleet("A", &["a1", "a2"]), fleet("B", &["b1"])];
        let objs = NavyObjects {
            fleets: vec![(0x100, vec![0x110, 0x120]), (0x200, vec![0x210])],
            admirals: vec![(0x900, "Raeder".into()), (0x910, "Lütjens".into())],
        };
        assert_eq!(fleet_object(&fleets, &objs, &fleets[1]), Some(0x200));
        assert_eq!(
            task_force_object(&fleets, &objs, &fleets[0].task_forces[1]),
            Some(0x120)
        );
        assert_eq!(admiral_object(&objs, " raeder "), Some(0x900));
        assert_eq!(admiral_object(&objs, "Dönitz"), None);
        // a fleet founded between the two reads: no pairing (the mouse path serves the call)
        let moved = NavyObjects {
            fleets: vec![(0x100, vec![0x110]), (0x200, vec![0x210, 0x220])],
            admirals: Vec::new(),
        };
        assert_eq!(fleet_object(&fleets, &moved, &fleets[0]), None);
        assert_eq!(
            task_force_object(&fleets, &moved, &fleets[0].task_forces[0]),
            None
        );
        // an object not of this read is never paired
        let other = fleet("A", &["a1", "a2"]);
        assert_eq!(fleet_object(&fleets, &objs, &other), None);
    }

    #[test]
    fn requests_name_the_module_ops() {
        let r = request("navy.mission", &[0x10, 0x20], &[], &[3], None).unwrap();
        assert_eq!(r["cmd"], "navy.mission");
        assert_eq!(r["objs"], "0x10,0x20");
        assert_eq!(r["vals"], "3");
        let a = request("navy.assign", &[0x30], &[0x10, 0x20], &[], None).unwrap();
        assert_eq!(a["divs"], "0x10,0x20");
        let l = request("navy.leader", &[0x30], &[], &[], Some(0x40)).unwrap();
        assert_eq!(l["obj2"], "0x40");
    }
}
