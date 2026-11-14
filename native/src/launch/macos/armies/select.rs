//! One selection for one army action. Every own division's province is read from memory and
//! projected through the live camera (map/unitstacks.txt unit positions), the camera is framed
//! once (Find view only when the divisions are not already on screen, then at most one zoom-out
//! swoop), and the divisions are picked with ONE box around all wanted stacks when no other own
//! division stands inside it, else with one click per clean stack (Shift+click after the first).
//! The selection is read back from memory: a division of another army refuses the action before
//! the army card is clicked, and wanted divisions the pick missed get one Shift+click round.
//! Replaces the repeated box sweeps that showed the same selection several times and took
//! 52.8 s for Eritrea (Italy test run s1, 9 Nov 2026).
use super::{membership, selected_divisions, selection_size, GAME};
use crate::{
    error::{Error, Result},
    memory::Reader,
    platform::{game_input::GameInput, Process},
};
use std::collections::{BTreeMap, BTreeSet};

/// A screen point in game pixels.
pub(super) type Point = (f64, f64);

/// Screen rectangle (game pixels) the selection may use: the map between the top bar, the
/// left-hand panels, the theatre box and the army card bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Area {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}
pub(super) const AREA: Area = Area {
    x0: 470.0,
    y0: 125.0,
    x1: 1560.0,
    y1: 850.0,
};
/// What a stack draws around its projected position: the unit model stands above it and the
/// counters hang below it, one row per counter (first row 45-57 px below at the Find view's
/// closest zoom over Sicily, live 9 Nov 2026; rows about 26 px). The box reaches MARGIN_X to the
/// sides of the outermost positions and covers every wanted stack's model and counter rows.
const MARGIN_X: f64 = 50.0;
const MODEL_RISE: f64 = 50.0;
const COUNTER_DROP: f64 = 64.0;
const COUNTER_ROW: f64 = 26.0;
/// Most counter rows counted for one stack (one per division at worst).
const ROWS_MAX: usize = 6;
/// Another stack whose model or counters come this close to the box makes the box unsafe.
const GUARD_X: f64 = 30.0;
const GUARD_Y: f64 = 20.0;
/// A clean stack is clicked only when no other stack's position lies this close.
const CLICK_CLEAR: f64 = 34.0;
/// Highest camera the framing zooms out to. The game groups unit counters by camera distance
/// (common/defines/00_graphics.lua, 1.19.4): from MAP_ICONS_GROUP_CAM_DISTANCE 90 nearby stacks
/// begin to group, from MAP_ICONS_STATE_GROUP_CAM_DISTANCE 180 a state's units show as one
/// counter at the state, from 350 several states' (live 9 Nov 2026: at height 369 Piedmont's ten
/// divisions showed as one counter beside Milan and the box around their provinces caught a
/// fleet instead). Below 180 every counter stays at its own province.
const MAX_HEIGHT: f64 = 160.0;
/// A camera above this has passed into the state-level grouping and comes back in.
const GROUP_STATE: f64 = 175.0;
/// Wanted stacks are framed with this much room to spare.
const FRAME_SLACK: f64 = 1.08;

/// One province holding own divisions, as the camera shows it.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Stack {
    pub province: i32,
    /// Projected unit position; None behind the camera.
    pub at: Option<Point>,
    /// Wanted divisions standing there.
    pub wanted: Vec<String>,
    /// Own divisions standing there that the action must not take.
    pub others: usize,
}

/// How the wanted divisions are picked.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Pick {
    /// One box drag from the first corner to the second.
    Box(Point, Point),
    /// Click the first point, Shift+click the others.
    Clicks(Vec<Point>),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Plan {
    pub pick: Option<Pick>,
    /// Wanted divisions the pick covers.
    pub covered: usize,
    /// Wanted divisions it cannot take: off screen, or in a stack shared with other divisions.
    pub left_out: usize,
}

fn inner(area: Area, p: Point) -> bool {
    p.0 >= area.x0 + MARGIN_X
        && p.0 <= area.x1 - MARGIN_X
        && p.1 >= area.y0 + MODEL_RISE
        && p.1 <= area.y1 - COUNTER_DROP
}
/// Top and bottom of what a stack at `y` draws: its model above, `rows` counter rows below.
fn extent(y: f64, rows: usize) -> (f64, f64) {
    let rows = rows.clamp(1, ROWS_MAX) as f64;
    (
        y - MODEL_RISE,
        y + COUNTER_DROP + COUNTER_ROW * (rows - 1.0),
    )
}

/// Decide the pick from the stacks alone (reads nothing, presses nothing).
pub(super) fn plan(stacks: &[Stack], area: Area) -> Plan {
    let wanted_total: usize = stacks.iter().map(|s| s.wanted.len()).sum();
    let shown: Vec<&Stack> = stacks
        .iter()
        .filter(|s| !s.wanted.is_empty() && s.at.is_some_and(|p| inner(area, p)))
        .collect();
    let others: Vec<(Point, usize)> = stacks
        .iter()
        .filter(|s| s.others > 0)
        .filter_map(|s| Some((s.at?, s.others + s.wanted.len())))
        .collect();
    if shown.is_empty() {
        return Plan {
            pick: None,
            covered: 0,
            left_out: wanted_total,
        };
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for s in &shown {
        let (x, y) = s.at.unwrap_or_default();
        let (top, bottom) = extent(y, s.wanted.len());
        x0 = x0.min(x);
        y0 = y0.min(top);
        x1 = x1.max(x);
        y1 = y1.max(bottom);
    }
    let (bx0, by0, bx1, by1) = (x0 - MARGIN_X, y0, x1 + MARGIN_X, y1.min(area.y1));
    let threatens = |(p, rows): &(Point, usize)| {
        let (top, bottom) = extent(p.1, *rows);
        p.0 >= bx0 - GUARD_X
            && p.0 <= bx1 + GUARD_X
            && bottom >= by0 - GUARD_Y
            && top <= by1 + GUARD_Y
    };
    if !others.iter().any(threatens) {
        let covered: usize = shown.iter().map(|s| s.wanted.len()).sum();
        return Plan {
            pick: Some(Pick::Box((bx0, by0), (bx1, by1))),
            covered,
            left_out: wanted_total - covered,
        };
    }
    // Another army stands among them: click each stack that holds only wanted divisions and
    // has no other stack beside it, top to bottom.
    let mut clean: Vec<&Stack> = shown
        .into_iter()
        .filter(|s| s.others == 0)
        .filter(|s| {
            let p = s.at.unwrap_or_default();
            !others
                .iter()
                .any(|(o, _)| (o.0 - p.0).abs() < CLICK_CLEAR && (o.1 - p.1).abs() < CLICK_CLEAR)
        })
        .collect();
    clean.sort_by(|a, b| {
        let (pa, pb) = (a.at.unwrap_or_default(), b.at.unwrap_or_default());
        pa.1.total_cmp(&pb.1).then(pa.0.total_cmp(&pb.0))
    });
    let covered: usize = clean.iter().map(|s| s.wanted.len()).sum();
    Plan {
        pick: (!clean.is_empty())
            .then(|| Pick::Clicks(clean.iter().filter_map(|s| s.at).collect())),
        covered,
        left_out: wanted_total - covered,
    }
}

/// Arrow-key taps the selection may spend bringing wanted stacks into the area.
const PAN_TAPS: usize = 6;

/// The arrow key that pans the camera towards the wanted stacks outside the area (None when
/// every wanted stack is inside it, or none projects). A stack behind the camera lies below
/// the screen. The key follows the larger offset of the wanted stacks' centre from the area's
/// centre, measured in area half-sizes.
pub(super) fn pan_key(stacks: &[Stack], area: Area) -> Option<u16> {
    use crate::platform::game_input::vk;
    let wanted: Vec<&Stack> = stacks.iter().filter(|s| !s.wanted.is_empty()).collect();
    if wanted.iter().all(|s| s.at.is_some_and(|p| inner(area, p))) {
        return None;
    }
    if wanted.iter().any(|s| s.at.is_none()) {
        return Some(vk::DOWN);
    }
    let n = wanted.len() as f64;
    let cx = wanted.iter().filter_map(|s| s.at).map(|p| p.0).sum::<f64>() / n;
    let cy = wanted.iter().filter_map(|s| s.at).map(|p| p.1).sum::<f64>() / n;
    let (mx, my) = ((area.x0 + area.x1) / 2.0, (area.y0 + area.y1) / 2.0);
    let (hx, hy) = ((area.x1 - area.x0) / 2.0, (area.y1 - area.y0) / 2.0);
    let (dx, dy) = ((cx - mx) / hx, (cy - my) / hy);
    if dx.abs() < 1e-6 && dy.abs() < 1e-6 {
        return None;
    }
    Some(if dx.abs() >= dy.abs() {
        if dx < 0.0 {
            vk::LEFT
        } else {
            vk::RIGHT
        }
    } else if dy < 0.0 {
        vk::UP
    } else {
        vk::DOWN
    })
}

/// How far the camera must zoom out (around the screen centre) for every wanted position to lie
/// inside the area with its box margins: the factor by which their offsets from the centre must
/// shrink; 1.0 when all fit already.
pub(super) fn fit_factor(points: &[Point], area: Area) -> f64 {
    let (cx, cy) = (960.0, 540.0);
    let lo = (area.x0 + MARGIN_X, area.y0 + MODEL_RISE);
    let hi = (area.x1 - MARGIN_X, area.y1 - COUNTER_DROP);
    let side = |v: f64, c: f64, low: f64, high: f64| -> f64 {
        if v < low {
            (c - v) / (c - low).max(1.0)
        } else if v > high {
            (v - c) / (high - c).max(1.0)
        } else {
            1.0
        }
    };
    points
        .iter()
        .map(|p| side(p.0, cx, lo.0, hi.0).max(side(p.1, cy, lo.1, hi.1)))
        .fold(1.0, f64::max)
}

/// The camera height to frame at: screen offsets shrink about as the height grows, so the
/// height times the fit factor (with some room), never above MAX_HEIGHT. None when the camera
/// should stay (all fit, or it is at the limit already).
pub(super) fn frame_height(factor: f64, height: f64) -> Option<f64> {
    if factor <= 1.0 || height <= 0.0 || height >= MAX_HEIGHT {
        return None;
    }
    Some((height * factor * FRAME_SLACK).min(MAX_HEIGHT))
}

/// Wheel events from `height` to `target` when one event multiplies the height by `step`
/// (measured: the zoom per event varies with the height). Positive zooms out.
pub(super) fn wheel_events(height: f64, target: f64, step: f64) -> i32 {
    if step <= 1.001 || height <= 0.0 || target <= 0.0 {
        return 0;
    }
    let k = (target / height).ln() / step.ln();
    // Rounded down: going out it stops short of the limit, coming in it gets below it.
    k.floor().clamp(-25.0, 25.0) as i32
}

/// What a selection read back holds against what was wanted.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Verdict {
    /// Wanted divisions selected.
    pub taken: Vec<String>,
    /// Wanted divisions not selected.
    pub missing: Vec<String>,
    /// Selected divisions of another army (the action must not go on).
    pub foreign: Vec<String>,
    /// Selected divisions in no army outside the wanted set (harmless for a new army).
    pub loose: Vec<String>,
}
/// `groups`: division id -> orders group (0 = no army); `allowed`: selected divisions that may
/// come along (the receiving army's own divisions).
pub(super) fn judge(
    selected: &[String],
    wanted: &[String],
    allowed: &BTreeSet<String>,
    groups: &BTreeMap<String, u64>,
) -> Verdict {
    let chosen: BTreeSet<&str> = selected.iter().map(String::as_str).collect();
    let want: BTreeSet<&str> = wanted.iter().map(String::as_str).collect();
    let mut v = Verdict::default();
    for id in wanted {
        if chosen.contains(id.as_str()) {
            v.taken.push(id.clone());
        } else {
            v.missing.push(id.clone());
        }
    }
    for id in selected {
        if want.contains(id.as_str()) || allowed.contains(id) {
            continue;
        }
        if groups.get(id).copied().unwrap_or(1) == 0 {
            v.loose.push(id.clone());
        } else {
            v.foreign.push(id.clone());
        }
    }
    v
}

/// The correction round: stacks on screen whose wanted divisions were all missed and that a
/// click can take alone.
pub(super) fn corrections(stacks: &[Stack], missing: &[String], area: Area) -> Vec<Point> {
    let missing: BTreeSet<&str> = missing.iter().map(String::as_str).collect();
    let others: Vec<Point> = stacks
        .iter()
        .filter(|s| s.others > 0)
        .filter_map(|s| s.at)
        .collect();
    stacks
        .iter()
        .filter(|s| s.others == 0 && !s.wanted.is_empty())
        .filter(|s| s.wanted.iter().all(|d| missing.contains(d.as_str())))
        .filter_map(|s| s.at)
        .filter(|p| inner(area, *p))
        .filter(|p| {
            !others
                .iter()
                .any(|o| (o.0 - p.0).abs() < CLICK_CLEAR && (o.1 - p.1).abs() < CLICK_CLEAR)
        })
        .collect()
}

/// Own divisions by province: (id, orders group, province) for every division standing still.
fn standing(r: &Reader<'_>, image: u64) -> Result<Vec<(String, u64, i32)>> {
    let groups = membership(r, image)?;
    let at = super::division_provinces(r, image)?;
    Ok(groups
        .into_iter()
        .filter_map(|(id, g)| {
            let p = at.get(&id).copied().flatten()?;
            Some((id, g, p))
        })
        .collect())
}

/// The stacks of the own divisions as the camera shows them now. `wanted` are the divisions to
/// take; `allowed` may come along; every other own division is to be avoided.
fn project(
    process: &dyn Process,
    positions: &super::super::mapdata::Positions,
    wanted: &BTreeSet<String>,
    allowed: &BTreeSet<String>,
) -> Result<(Vec<Stack>, f64)> {
    super::read(process, |r, i| {
        let rows = standing(r, i)?;
        let mut by: BTreeMap<i32, Stack> = BTreeMap::new();
        for (id, _, p) in rows {
            let s = by.entry(p).or_insert_with(|| Stack {
                province: p,
                at: None,
                wanted: Vec::new(),
                others: 0,
            });
            if wanted.contains(&id) {
                s.wanted.push(id);
            } else if !allowed.contains(&id) {
                s.others += 1;
            }
        }
        let mut out = Vec::with_capacity(by.len());
        for (p, mut s) in by {
            if let Some(&(x, z)) = positions.get(&p) {
                s.at = super::super::screenmap::to_screen(r, i, x, z)?;
            }
            out.push(s);
        }
        Ok((out, super::super::screenmap::camera_height(r, i)?))
    })
}

/// What `select` achieved.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Selected {
    pub verdict: Verdict,
    /// "box" or "clicks", plus "+fix" after the correction round.
    pub how: String,
    /// The state was found with the Find view (else the camera already showed it).
    pub found: bool,
    pub zoom_events: u32,
}

/// The selection as read from memory once it rests (two equal reads 60 ms apart, at most
/// 900 ms); None while it holds anything but own divisions.
fn rested_selection(process: &dyn Process) -> Result<Option<Vec<String>>> {
    let started = std::time::Instant::now();
    let mut last = super::read(process, selected_divisions)?;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(60));
        let now = super::read(process, selected_divisions)?;
        let held = now.as_ref().is_some_and(|ids| !ids.is_empty());
        let resting = now == last && held;
        // A selection shows within about 100 ms; one still empty after 350 ms is not coming.
        let empty = !held && started.elapsed() > std::time::Duration::from_millis(350);
        if resting || empty || started.elapsed() > std::time::Duration::from_millis(900) {
            super::super::ui::trace("selection rests", started);
            return Ok(now);
        }
        last = now;
    }
}

/// Select `wanted` (own divisions standing in `state`) in one go; `allowed` divisions may be
/// selected with them. Errors before any selection input with `selection_unavailable` when no
/// wanted stack can be picked safely, and with `selection_mismatch` (no further input) when the
/// selection read back holds a division of another army.
pub(super) fn select(
    process: &dyn Process,
    keys: &dyn GameInput,
    state: i32,
    wanted: &[String],
    allowed: &[String],
) -> Result<Selected> {
    let exe = &process
        .identity()
        .ok_or_else(|| Error::new("map_data_unavailable", "Game identity unavailable"))?
        .exe;
    let dir = super::super::mapdata::game_dir(exe)?;
    let positions = super::super::mapdata::positions(&dir)?;
    let want: BTreeSet<String> = wanted.iter().cloned().collect();
    let ok: BTreeSet<String> = allowed.iter().cloned().collect();
    let points = |stacks: &[Stack]| -> Vec<Point> {
        stacks
            .iter()
            .filter(|s| !s.wanted.is_empty())
            .filter_map(|s| s.at)
            .collect()
    };
    let t0 = std::time::Instant::now();
    super::clear_selection(process, keys)?;
    let (mut stacks, mut height) = project(process, &positions, &want, &ok)?;
    // The camera already shows every wanted stack at counter height: no camera movement at all.
    let on_screen = |stacks: &[Stack]| {
        stacks
            .iter()
            .filter(|s| !s.wanted.is_empty())
            .all(|s| s.at.is_some_and(|p| inner(AREA, p)))
    };
    let found = !(height <= MAX_HEIGHT && on_screen(&stacks));
    if found {
        super::super::ui::find_state(process, keys, state)?;
        (stacks, height) = project(process, &positions, &want, &ok)?;
    }
    // Framing: one wheel event measures how far an event zooms here, one burst goes most of the
    // way to the height that fits, and one correction at most (never above MAX_HEIGHT).
    super::super::ui::trace_text(&format!(
        "army select state {state}: framing at height {height:.0}: wanted at {:?}",
        points(&stacks)
            .iter()
            .map(|(x, y)| (x.round(), y.round()))
            .collect::<Vec<_>>()
    ));
    let mut zoom_events = 0u32;
    let mut step: Option<f64> = None;
    for _ in 0..4 {
        let Some(target) = frame_height(fit_factor(&points(&stacks), AREA), height) else {
            break;
        };
        // At least one event while the stacks do not fit: close in, one event zooms out far
        // (50 -> 104 over Piedmont), so the measured step overstates the next one.
        let k = match step {
            None => 1,
            Some(q) => wheel_events(height, target, q).max(1),
        };
        let before = height;
        keys.scroll_burst(GAME, (960.0, 540.0), -120, k as u32)?;
        std::thread::sleep(std::time::Duration::from_millis(60));
        super::super::screenmap::wait_camera_still(process, 900);
        (stacks, height) = project(process, &positions, &want, &ok)?;
        zoom_events += k as u32;
        if height <= before * 1.001 {
            break;
        }
        step = Some((height / before).powf(1.0 / f64::from(k)));
    }
    // A burst that went past the grouping limit comes back in (one burst).
    if height > GROUP_STATE {
        if let Some(q) = step {
            let k = (-wheel_events(height, MAX_HEIGHT, q)).max(1);
            keys.scroll_burst(GAME, (960.0, 540.0), 120, k as u32)?;
            std::thread::sleep(std::time::Duration::from_millis(60));
            super::super::screenmap::wait_camera_still(process, 900);
            (stacks, height) = project(process, &positions, &want, &ok)?;
        }
    }
    // A state larger than the screen at the highest framing leaves wanted stacks outside the
    // area (shakedown 10 Nov 2026: Somaliland's southern stack sat 40 px below it and the
    // selection was refused 40 times): pan towards them with the arrow keys, a few taps at most.
    for _ in 0..PAN_TAPS {
        let Some(key) = pan_key(&stacks, AREA) else {
            break;
        };
        keys.press_code(key)?;
        std::thread::sleep(std::time::Duration::from_millis(220));
        super::super::screenmap::wait_camera_still(process, 900);
        (stacks, height) = project(process, &positions, &want, &ok)?;
    }
    let plan = plan(&stacks, AREA);
    super::super::ui::trace_text(&format!(
        "army select state {state}: height {height:.0}, found {found}, zoom {zoom_events}, plan {:?}, stacks {:?}",
        plan.pick,
        stacks
            .iter()
            .filter(|s| s.at.is_some_and(|p| (0.0..1920.0).contains(&p.0) && (0.0..1080.0).contains(&p.1)))
            .map(|s| (s.province, s.at.map(|(x, y)| (x.round(), y.round())), s.wanted.len(), s.others))
            .collect::<Vec<_>>()
    ));
    super::super::ui::trace("army select framed", t0);
    let Some(pick) = plan.pick.clone() else {
        return Err(Error::new(
            "selection_unavailable",
            format!(
                "None of the {} wanted divisions of state {state} can be picked alone on screen \
                 ({} stand off screen or share a province with another army); nothing was selected.",
                wanted.len(),
                plan.left_out
            ),
        ));
    };
    let before = super::read(process, selected_divisions).ok().flatten();
    let mut rested = None;
    let mut how = match &pick {
        Pick::Box(from, to) => {
            keys.drag(GAME, *from, *to)?;
            let now = rested_selection(process)?;
            // A press on another country's unit model (a border state's corner) starts no box
            // and leaves the selection as it was (live 9 Nov 2026, Piedmont's corner on a French
            // division): the same box once more from the opposite corner.
            if now == before || now.as_ref().is_none_or(|ids| ids.is_empty()) {
                keys.drag(GAME, *to, *from)?;
                rested = Some(rested_selection(process)?);
                "box (from the opposite corner)".to_string()
            } else {
                rested = Some(now);
                "box".to_string()
            }
        }
        Pick::Clicks(points) => {
            for (k, (x, y)) in points.iter().enumerate() {
                if k == 0 {
                    keys.click(GAME, *x, *y)?;
                } else {
                    keys.shift_click(GAME, *x, *y)?;
                }
            }
            format!("clicks:{}", points.len())
        }
    };
    let judge_now = |selection: Option<Vec<String>>, how: &str| -> Result<Verdict> {
        let groups = super::read(process, membership)?;
        let Some(ids) = selection else {
            return Err(Error::new(
                "selection_mismatch",
                format!(
                    "The selection in state {state} holds something other than own divisions \
                     (size {}, object types {:?}, after {how}); the army card was not clicked.",
                    super::read(process, selection_size).unwrap_or(0),
                    super::read(process, super::selection_types).unwrap_or_default()
                ),
            ));
        };
        let v = judge(&ids, wanted, &ok, &groups);
        if !v.foreign.is_empty() {
            return Err(Error::new(
                "selection_mismatch",
                format!(
                    "The {how} in state {state} also took {} division(s) of another army; the army \
                     card was not clicked, nothing was assigned.",
                    v.foreign.len()
                ),
            ));
        }
        Ok(v)
    };
    let mut first = match rested {
        Some(selection) => selection,
        None => rested_selection(process)?,
    };
    // A box that holds no division the engine can pick takes what else stands in it (a fleet in
    // the port, an air wing): the selection is then not a division list. A plain click on a
    // wanted stack replaces that selection, Shift+click adds the other stacks.
    if first.is_none() && matches!(pick, Pick::Box(..)) {
        let points = corrections(&stacks, wanted, AREA);
        if !points.is_empty() {
            for (k, (x, y)) in points.iter().enumerate() {
                if k == 0 {
                    keys.click(GAME, *x, *y)?;
                } else {
                    keys.shift_click(GAME, *x, *y)?;
                }
            }
            how = format!("clicks:{} (the box took no division)", points.len());
            first = rested_selection(process)?;
        }
    }
    let mut verdict = judge_now(first, &how)?;
    if !verdict.missing.is_empty() {
        let fix = corrections(&stacks, &verdict.missing, AREA);
        if !fix.is_empty() {
            for (x, y) in &fix {
                keys.shift_click(GAME, *x, *y)?;
            }
            how.push_str("+fix");
            verdict = judge_now(rested_selection(process)?, &how)?;
        }
    }
    if verdict.taken.is_empty() {
        return Err(Error::new(
            "selection_mismatch",
            format!(
                "The {how} in state {state} took none of its {} wanted divisions; the army card was \
                 not clicked.",
                wanted.len()
            ),
        ));
    }
    super::super::ui::trace_text(&format!(
        "army select state {state}: {how} took {}/{} (loose {}, missing {})",
        verdict.taken.len(),
        wanted.len(),
        verdict.loose.len(),
        verdict.missing.len()
    ));
    Ok(Selected {
        verdict,
        how,
        found,
        zoom_events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_wanted_stack_below_the_area_pans_the_camera_down_and_none_when_all_are_inside() {
        use crate::platform::game_input::vk;
        // Somaliland, shakedown 10 Nov 2026: the wanted stack at (544, 827) below the area
        let stacks = vec![
            stack(1964, (986.0, 442.0), &[], 0),
            stack(2063, (544.0, 827.0), &["a", "b"], 0),
        ];
        assert_eq!(pan_key(&stacks, AREA), Some(vk::DOWN));
        let left = vec![stack(1, (300.0, 500.0), &["a"], 0)];
        assert_eq!(pan_key(&left, AREA), Some(vk::LEFT));
        let behind = vec![Stack {
            province: 1,
            at: None,
            wanted: vec!["a".into()],
            others: 0,
        }];
        assert_eq!(pan_key(&behind, AREA), Some(vk::DOWN));
        let inside = vec![
            stack(1, (900.0, 500.0), &["a"], 0),
            stack(2, (300.0, 500.0), &[], 3),
        ];
        assert_eq!(pan_key(&inside, AREA), None);
    }
    fn stack(province: i32, at: Point, wanted: &[&str], others: usize) -> Stack {
        Stack {
            province,
            at: Some(at),
            wanted: wanted.iter().map(|s| s.to_string()).collect(),
            others,
        }
    }
    #[test]
    fn one_box_takes_every_wanted_stack_when_no_other_army_stands_near() {
        let stacks = vec![
            stack(1, (800.0, 400.0), &["a", "b"], 0),
            stack(2, (1000.0, 600.0), &["c"], 0),
            stack(3, (900.0, 500.0), &["d", "e", "f"], 0),
            // an army far away
            stack(4, (1400.0, 300.0), &[], 4),
        ];
        let p = plan(&stacks, AREA);
        // from the top stack's model to the bottom stack's single counter row
        assert_eq!(
            p.pick,
            Some(Pick::Box(
                (800.0 - MARGIN_X, 400.0 - MODEL_RISE),
                (1000.0 + MARGIN_X, 600.0 + COUNTER_DROP)
            ))
        );
        assert_eq!((p.covered, p.left_out), (6, 0));
        // a stack of three draws three rows: the box reaches the last one
        let stacks = vec![stack(1, (800.0, 400.0), &["a", "b", "c"], 0)];
        let Some(Pick::Box(_, (_, bottom))) = plan(&stacks, AREA).pick else {
            panic!("box expected")
        };
        assert_eq!(bottom, 400.0 + COUNTER_DROP + 2.0 * COUNTER_ROW);
    }
    #[test]
    fn an_army_inside_the_box_turns_it_into_clicks_on_the_clean_stacks() {
        let stacks = vec![
            stack(1, (800.0, 400.0), &["a"], 0),
            stack(2, (1000.0, 600.0), &["b", "c"], 0),
            // another army between them
            stack(3, (900.0, 500.0), &[], 3),
            // a province shared with that army: never clicked
            stack(5, (1100.0, 420.0), &["d"], 2),
        ];
        let p = plan(&stacks, AREA);
        assert_eq!(
            p.pick,
            Some(Pick::Clicks(vec![(800.0, 400.0), (1000.0, 600.0)]))
        );
        assert_eq!((p.covered, p.left_out), (3, 1));
    }
    #[test]
    fn a_stack_just_above_the_box_counts_since_its_counters_hang_into_it() {
        // two rows of another army 60 px above: its counters reach down into the box
        let stacks = vec![
            stack(1, (800.0, 400.0), &["a"], 0),
            stack(2, (820.0, 400.0 - MODEL_RISE - 60.0), &[], 2),
        ];
        assert!(matches!(plan(&stacks, AREA).pick, Some(Pick::Clicks(_))));
        // well above, and well below, it is harmless
        for y in [
            400.0 - MODEL_RISE - COUNTER_DROP - COUNTER_ROW - GUARD_Y - 5.0,
            400.0 + COUNTER_DROP + MODEL_RISE + GUARD_Y + 5.0,
        ] {
            let stacks = vec![
                stack(1, (800.0, 400.0), &["a"], 0),
                stack(2, (820.0, y), &[], 2),
            ];
            assert!(
                matches!(plan(&stacks, AREA).pick, Some(Pick::Box(..))),
                "{y}"
            );
        }
    }
    #[test]
    fn a_clean_stack_right_beside_another_army_is_not_clicked() {
        let stacks = vec![
            stack(1, (800.0, 400.0), &["a"], 0),
            stack(2, (812.0, 410.0), &[], 2),
        ];
        let p = plan(&stacks, AREA);
        assert_eq!(p.pick, None);
        assert_eq!((p.covered, p.left_out), (0, 1));
    }
    #[test]
    fn off_screen_stacks_are_left_out_and_nothing_on_screen_plans_nothing() {
        let stacks = vec![
            stack(1, (800.0, 400.0), &["a"], 0),
            stack(2, (1800.0, 400.0), &["b"], 0),
            Stack {
                province: 3,
                at: None,
                wanted: vec!["c".into()],
                others: 0,
            },
        ];
        let p = plan(&stacks, AREA);
        assert!(matches!(p.pick, Some(Pick::Box(..))));
        assert_eq!((p.covered, p.left_out), (1, 2));
        let p = plan(&stacks[1..], AREA);
        assert_eq!(p.pick, None);
        assert_eq!(p.left_out, 2);
    }
    #[test]
    fn framing_asks_for_just_enough_height_and_never_past_state_grouping() {
        // everything inside: no zoom
        assert_eq!(fit_factor(&[(900.0, 500.0)], AREA), 1.0);
        assert_eq!(frame_height(1.0, 50.0), None);
        // x 1700 (centre 960, right limit 1510): offsets must shrink 740/550
        let f = fit_factor(&[(1700.0, 500.0)], AREA);
        assert!((f - 740.0 / 550.0).abs() < 1e-9);
        let h = frame_height(f, 50.0).unwrap();
        assert!((h - 50.0 * f * FRAME_SLACK).abs() < 1e-9);
        // far apart: capped below the state-level grouping (180)
        assert_eq!(frame_height(5.0, 50.0), Some(MAX_HEIGHT));
        assert_eq!(frame_height(1.5, MAX_HEIGHT), None);
        // above the centre: top limit 175, a stack at y 0 -> 540/365
        let f = fit_factor(&[(960.0, 0.0)], AREA);
        assert!((f - 540.0 / 365.0).abs() < 1e-9);
    }
    #[test]
    fn zoom_events_come_from_the_measured_step_and_stop_short_going_out() {
        // 50 -> 160 at 1.22 per event: ln 3.2 / ln 1.22 = 5.86 -> 5 (never past the target)
        assert_eq!(wheel_events(50.0, 160.0, 1.22), 5);
        // coming back in from 370 to 160: -4.2 -> -5 (enough to get below)
        assert_eq!(wheel_events(370.0, 160.0, 1.22), -5);
        // an event that did not move the camera gives no burst
        assert_eq!(wheel_events(50.0, 160.0, 1.0), 0);
        assert_eq!(wheel_events(50.0, 1.0e9, 1.1), 25);
    }
    #[test]
    fn the_verdict_separates_missing_foreign_and_loose_divisions() {
        let groups: BTreeMap<String, u64> =
            [("a", 0), ("b", 0), ("c", 0), ("x", 7), ("own", 9), ("n", 0)]
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let allowed: BTreeSet<String> = ["own".to_string()].into();
        let v = judge(
            &s(&["a", "c", "own", "n"]),
            &s(&["a", "b", "c"]),
            &allowed,
            &groups,
        );
        assert_eq!(v.taken, s(&["a", "c"]));
        assert_eq!(v.missing, s(&["b"]));
        assert!(v.foreign.is_empty());
        assert_eq!(v.loose, s(&["n"]));
        let v = judge(&s(&["a", "x"]), &s(&["a"]), &BTreeSet::new(), &groups);
        assert_eq!(v.foreign, s(&["x"]));
    }
    #[test]
    fn the_correction_round_clicks_only_wholly_missed_clean_stacks() {
        let stacks = vec![
            stack(1, (800.0, 400.0), &["a", "b"], 0),
            stack(2, (1000.0, 600.0), &["c"], 0),
            stack(3, (1100.0, 300.0), &["d"], 1),
            stack(4, (1700.0, 300.0), &["e"], 0),
        ];
        let missing: Vec<String> = ["b", "c", "d", "e"].iter().map(|s| s.to_string()).collect();
        // stack 1 is half taken (a click would toggle it), 3 is shared, 4 is off screen
        assert_eq!(corrections(&stacks, &missing, AREA), vec![(1000.0, 600.0)]);
    }
}
