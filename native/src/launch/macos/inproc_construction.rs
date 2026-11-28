//! Construction through the in-process module (engineer 82, family `construction`): the construction view's own three
//! posts, each checked by the game's own "can do" before it is posted.
//! - add: the map click in construction mode, AddConstruction(mode 2, own tag, the building reference, count 1), one
//!   command per click: a state building names the state, a province building (supply hub, naval base, bunker, coastal
//!   bunker) names the province, exactly as the game's own reference constructors decide from the building template;
//! - remove: RemoveConstruction(line, its level count): the whole line leaves the queue;
//! - move: SetProductionLinePlacement(line, index, placed-by-hand byte) as the view's move posts it, the byte computed by
//!   the view's own rule from the neighbours at the new place.
//!
//! Railways are another command (CBuildRailwayCommand carries a province path the UI's own route finder computes): they
//! stay on the mouse. hoi4_construction_add and hoi4_construction_order keep their names, schemas and Verified
//! read-backs; the mouse path stays the fallback in the same call. The family is off under `HOI4_BACKEND=auto` until
//! its live session (S14-CONSTRUCTION) passes.
use super::inproc::{Channel, Routed};
use super::inproc_units::{request, routed};
use crate::construction::{placement_byte, InprocLine};

/// The location kind the module checks against the template: 0 a state id, 1 a province id.
pub(super) fn location_kind(province: bool) -> i64 {
    i64::from(province)
}

/// `clicks` more of the building in the state (or on the province), one command per click.
pub(super) fn add(
    ch: &mut Channel,
    template: u64,
    location: i32,
    province: bool,
    clicks: i64,
) -> Routed {
    if !(1..=10).contains(&clicks) || location <= 0 {
        return Routed::Gui;
    }
    let vals = [i64::from(location), location_kind(province), clicks];
    match request("construction.add", &[template], &[], &vals, None).and_then(|r| ch.post_raw(r)) {
        Ok(reply) => routed(reply, "this building here now"),
        Err(_) => Routed::Gui,
    }
}

/// The whole line out of the queue (every level it holds).
pub(super) fn remove(ch: &mut Channel, line: &InprocLine) -> Routed {
    let levels = i64::from(line.amount.max(1));
    match request("construction.remove", &[line.address], &[], &[levels], None)
        .and_then(|r| ch.post_raw(r))
    {
        Ok(reply) => routed(reply, "removing this construction line"),
        Err(_) => Routed::Gui,
    }
}

/// The view's move of line `from` to index `to`, with the byte the view computes.
pub(super) fn move_line(ch: &mut Channel, lines: &[InprocLine], from: usize, to: usize) -> Routed {
    let Some(line) = lines.get(from) else {
        return Routed::Gui;
    };
    let (Ok(index), byte) = (i64::try_from(to), placement_byte(lines, from, to)) else {
        return Routed::Gui;
    };
    let vals = [index, i64::from(byte)];
    match request("construction.move", &[line.address], &[], &vals, None)
        .and_then(|r| ch.post_raw(r))
    {
        Ok(reply) => routed(reply, "moving this construction line"),
        Err(_) => Routed::Gui,
    }
}

/// The target index of a row button: top, up, down, bottom (None for remove or an unknown action).
pub(super) fn target_index(action: &str, line: usize, n: usize) -> Option<usize> {
    if n == 0 || line >= n {
        return None;
    }
    match action {
        "top" => Some(0),
        "up" => Some(line.saturating_sub(1)),
        "down" => Some((line + 1).min(n - 1)),
        "bottom" => Some(n - 1),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(address: u64, template: u64, placed: bool) -> InprocLine {
        InprocLine {
            address,
            amount: 1,
            kind: 1,
            placed,
            template: Some(template),
            real: true,
            active: true,
        }
    }

    #[test]
    fn requests_carry_the_location_its_kind_and_the_clicks() {
        let r = request(
            "construction.add",
            &[0x7000_1000],
            &[],
            &[64, location_kind(false), 3],
            None,
        )
        .unwrap();
        assert_eq!(r["cmd"], "construction.add");
        assert_eq!(r["vals"], "64,0,3");
        let r = request(
            "construction.add",
            &[0x7000_1000],
            &[],
            &[9400, location_kind(true), 1],
            None,
        )
        .unwrap();
        assert_eq!(r["vals"], "9400,1,1");
        let r = request("construction.move", &[0x7000_2000], &[], &[0, 1], None).unwrap();
        assert_eq!(
            (r["objs"].as_str(), r["vals"].as_str()),
            (Some("0x70002000"), Some("0,1"))
        );
    }

    #[test]
    fn the_row_buttons_name_their_target_index() {
        assert_eq!(target_index("top", 3, 5), Some(0));
        assert_eq!(target_index("up", 0, 5), Some(0));
        assert_eq!(target_index("down", 4, 5), Some(4));
        assert_eq!(target_index("bottom", 1, 5), Some(4));
        assert_eq!(target_index("remove", 1, 5), None);
        assert_eq!(target_index("up", 5, 5), None);
    }

    #[test]
    fn the_view_groups_a_line_with_its_own_building_and_marks_other_moves_by_hand() {
        // factories A A, then a refinery R: moving R up between the two A lines lands it next to an A: by hand
        let q = vec![
            line(1, 0xa, false),
            line(2, 0xa, false),
            line(3, 0xb, false),
        ];
        assert!(placement_byte(&q, 2, 1));
        // the last A moved to the top lands next to the first A (the line below): grouped
        let q = vec![
            line(1, 0xa, false),
            line(2, 0xb, false),
            line(3, 0xa, false),
        ];
        assert!(!placement_byte(&q, 2, 0));
        // moved down onto the place after an A of its own building (above): grouped
        let q = vec![
            line(1, 0xa, false),
            line(2, 0xb, false),
            line(3, 0xa, false),
        ];
        assert!(!placement_byte(&q, 0, 2));
        // a neighbour placed by hand does not group
        let q = vec![line(1, 0xa, true), line(2, 0xb, false), line(3, 0xa, false)];
        assert!(placement_byte(&q, 2, 0));
        // a railway never groups (its own virtual says no): 0
        let mut q = vec![line(1, 0xa, false), line(2, 0xb, false)];
        q[1].kind = 2;
        q[1].template = None;
        assert!(!placement_byte(&q, 1, 0));
        // onto its own place: the line keeps its byte
        let q = vec![line(1, 0xa, true), line(2, 0xb, false)];
        assert!(placement_byte(&q, 0, 0));
    }
}
