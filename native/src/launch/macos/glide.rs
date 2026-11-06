//! One smooth wheel movement ("glide") instead of a read-scroll-read crawl.
//!
//! Every wheel event moves a list, tree or the map camera by a roughly fixed amount whatever its
//! pixel value. A glide sends one event, measures how far the target moved, sends the remaining
//! events as one quick burst (the game eases them into one continuous movement) and then at most
//! one short correction burst. Positions are fresh memory reads taken once the movement rests.
use super::ui::read;
use crate::{
    error::Result,
    memory::Reader,
    platform::{game_input::GameInput, Process},
};
use std::{
    thread,
    time::{Duration, Instant},
};

const GAME: (u32, u32) = (1920, 1080);
/// Most wheel events in one burst (a burst of 60 lasts about half a second).
const MAX_BURST: u32 = 60;
/// Single events sent while the target's position is unknown (the old loops' bound).
const SEARCH_STEPS: u32 = 40;
/// A list eases its movement: a position counts as resting once two reads this far apart agree.
const REST_POLL: Duration = Duration::from_millis(40);
const REST_LIMIT: Duration = Duration::from_millis(1200);
/// Below this many pixels the target did not move.
const STILL: f64 = 0.5;

/// Where a glide left the target (the start of its span: a row's top edge).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Glided {
    /// The span lies inside the wanted band.
    InBand(f64),
    /// The list stopped moving (its end) before the band was reached.
    Stuck(f64),
    /// The movement ended outside the band (one event moves further than the band leaves room
    /// for, or the correction did not land).
    Short(f64),
    /// The target's position could not be read.
    Unknown,
}

/// The target's extent along the scroll axis: (start, end), e.g. a row's top and bottom edge.
pub(super) type Span = (f64, f64);

/// One glide: wheel point, pixels per event, wanted band of the measured span.
#[derive(Clone, Copy, Debug)]
pub(super) struct Glide {
    pub at: (f64, f64),
    /// Magnitude of one wheel event; `+pixels` is assumed to raise the position (list rows move
    /// down), which the first measured event confirms or reverses.
    pub pixels: i32,
    /// Wanted range: the whole span must lie inside it (inclusive).
    pub band: (f64, f64),
    /// Wheel direction (sign of pixels) to step one event at a time while the position is
    /// unknown; 0 stops at once with `Glided::Unknown`.
    pub search: i32,
}

fn inside(p: Span, band: (f64, f64)) -> bool {
    p.0 >= band.0 && p.1 <= band.1
}
fn centre(p: Span) -> f64 {
    (p.0 + p.1) / 2.0
}
/// The band the span's centre must reach.
fn centre_band(p: Span, band: (f64, f64)) -> (f64, f64) {
    let half = (p.1 - p.0).abs() / 2.0;
    (band.0 + half, band.1 - half)
}

/// Signed wheel events (positive = `+pixels`) that bring `pos` nearest the middle of `band`, when
/// one `+pixels` event changes the position by `step`. `None` when the step is too small to
/// move the target; `Some(0)` when no whole number of events comes closer.
pub(super) fn events_needed(pos: f64, step: f64, band: (f64, f64), cap: u32) -> Option<i32> {
    if pos >= band.0 && pos <= band.1 {
        return Some(0);
    }
    if step.abs() < STILL {
        return None;
    }
    let middle = (band.0 + band.1) / 2.0;
    let k = ((middle - pos) / step).round();
    let cap = f64::from(cap);
    Some(k.clamp(-cap, cap) as i32)
}

/// The glide's decisions over abstract input and measurement, so the arithmetic is testable.
/// `send(sign, count)` sends `count` wheel events of `sign * pixels` (one burst when count > 1);
/// `measure()` returns the resting span.
pub(super) fn plan_glide(
    band: (f64, f64),
    search: i32,
    mut send: impl FnMut(i32, u32) -> Result<()>,
    mut measure: impl FnMut() -> Result<Option<Span>>,
) -> Result<Glided> {
    let mut pos = measure()?;
    let mut searched = 0;
    while pos.is_none() {
        if search == 0 || searched >= SEARCH_STEPS {
            return Ok(Glided::Unknown);
        }
        send(search.signum(), 1)?;
        searched += 1;
        pos = measure()?;
    }
    let Some(p0) = pos else {
        return Ok(Glided::Unknown);
    };
    if inside(p0, band) {
        return Ok(Glided::InBand(p0.0));
    }
    // One event to learn how far (and which way) the target moves per event.
    let first = if centre(p0) < centre(centre_band(p0, band)) {
        1
    } else {
        -1
    };
    send(first, 1)?;
    let Some(p1) = measure()? else {
        return Ok(Glided::Unknown);
    };
    if inside(p1, band) {
        return Ok(Glided::InBand(p1.0));
    }
    let mut step = (centre(p1) - centre(p0)) * f64::from(first);
    let mut pos = p1;
    // The main burst, then at most one correction.
    for _ in 0..2 {
        let Some(k) = events_needed(centre(pos), step, centre_band(pos, band), MAX_BURST) else {
            return Ok(Glided::Stuck(pos.0));
        };
        if k == 0 {
            return Ok(Glided::Short(pos.0));
        }
        send(k.signum(), k.unsigned_abs())?;
        let Some(now) = measure()? else {
            return Ok(Glided::Unknown);
        };
        if inside(now, band) {
            return Ok(Glided::InBand(now.0));
        }
        // A burst that moved less than planned may have met the list's end or had events
        // merged by the game: the correction uses the step the burst itself achieved, and a
        // burst that did not move at all means the end.
        let moved = centre(now) - centre(pos);
        if moved.abs() < STILL {
            return Ok(Glided::Stuck(now.0));
        }
        step = moved / f64::from(k);
        pos = now;
    }
    Ok(Glided::Short(pos.0))
}

/// Read `position` until two reads in a row agree (the eased movement has stopped).
fn rest(
    process: &dyn Process,
    position: &impl Fn(&Reader<'_>, u64) -> Result<Option<Span>>,
) -> Result<Option<Span>> {
    let until = Instant::now() + REST_LIMIT;
    let mut last = read(process, position)?;
    loop {
        if Instant::now() >= until {
            return Ok(last);
        }
        thread::sleep(REST_POLL);
        let now = read(process, position)?;
        let same = match (last, now) {
            (Some(a), Some(b)) => (a.0 - b.0).abs() < STILL && (a.1 - b.1).abs() < STILL,
            (None, None) => true,
            _ => false,
        };
        if same {
            return Ok(now);
        }
        last = now;
    }
}

/// Glide the target into `g.band`. `guard` runs before every input (e.g. "the list is still
/// open"; its error aborts the glide); `position` reads the target's span (None: not shown).
pub(super) fn glide(
    process: &dyn Process,
    keys: &dyn GameInput,
    g: &Glide,
    mut guard: impl FnMut() -> Result<()>,
    position: impl Fn(&Reader<'_>, u64) -> Result<Option<Span>>,
) -> Result<Glided> {
    let pixels = g.pixels.abs();
    plan_glide(
        g.band,
        g.search,
        // Probe and bursts both go through `scroll_burst`, so one measured event is the same
        // unit as one burst event on every backend (X11 `scroll` sends one notch per 120 px).
        |sign, count| {
            guard()?;
            keys.scroll_burst(GAME, g.at, sign * pixels, count)
        },
        || rest(process, &position),
    )
}

/// A rectangle's vertical span.
pub(super) fn span_y(rect: &crate::gui_tree::Rect) -> Span {
    (f64::from(rect.y), f64::from(rect.y + rect.h))
}

/// The decisions of `glide_to_end` (testable): bursts until one event no longer moves the list.
pub(super) fn plan_to_end(
    mut send: impl FnMut(u32) -> Result<()>,
    mut measure: impl FnMut() -> Result<f64>,
) -> Result<f64> {
    let mut pos = measure()?;
    for _ in 0..4 {
        send(1)?;
        let now = measure()?;
        if (now - pos).abs() < STILL {
            return Ok(now);
        }
        send(MAX_BURST)?;
        pos = measure()?;
    }
    Ok(pos)
}

/// Scroll a list to its end in `pixels`' direction with bursts (one probe event shows whether it
/// still moves); returns the resting position.
pub(super) fn glide_to_end(
    process: &dyn Process,
    keys: &dyn GameInput,
    at: (f64, f64),
    pixels: i32,
    mut guard: impl FnMut() -> Result<()>,
    position: impl Fn(&Reader<'_>, u64) -> Result<f64>,
) -> Result<f64> {
    let position = |r: &Reader<'_>, i: u64| position(r, i).map(|p| Some((p, p)));
    plan_to_end(
        |count| {
            guard()?;
            keys.scroll_burst(GAME, at, pixels, count)
        },
        || Ok(rest(process, &position)?.map_or(0.0, |p| p.0)),
    )
}

/// One wheel event zooms the camera about 10% (measured live 5 November 2026).
const ZOOM_PER_EVENT: f64 = 1.1;
/// Zoom-out events (around the screen centre) that bring screen point `p` inside `area`
/// (x0, y0, x1, y1): screen offsets from the centre shrink by about 10% per event.
pub(super) fn zoom_out_events(p: (f64, f64), area: (f64, f64, f64, f64), cap: u32) -> u32 {
    let (cx, cy) = (960.0, 540.0);
    let side = |v: f64, centre: f64, low: f64, high: f64| -> f64 {
        if v < low {
            (centre - v) / (centre - low).max(1.0)
        } else if v > high {
            (v - centre) / (high - centre).max(1.0)
        } else {
            1.0
        }
    };
    let factor = side(p.0, cx, area.0, area.2).max(side(p.1, cy, area.1, area.3));
    if factor <= 1.0 || cap == 0 {
        return 0;
    }
    ((factor.ln() / ZOOM_PER_EVENT.ln()).ceil() as u32).clamp(1, cap)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A list whose target sits at `pos`; each `+` event moves it by `step`, clamped to `range`.
    struct List {
        pos: f64,
        step: f64,
        range: (f64, f64),
        sent: Vec<(i32, u32)>,
    }
    fn run(list: List, band: (f64, f64)) -> (Glided, Vec<(i32, u32)>, f64) {
        let list = RefCell::new(list);
        let out = plan_glide(
            band,
            0,
            |sign, count| {
                let mut l = list.borrow_mut();
                l.sent.push((sign, count));
                let to = l.pos + l.step * f64::from(sign) * f64::from(count);
                l.pos = to.clamp(l.range.0, l.range.1);
                Ok(())
            },
            || {
                let p = list.borrow().pos;
                Ok(Some((p, p)))
            },
        )
        .unwrap();
        let l = list.into_inner();
        (out, l.sent, l.pos)
    }

    #[test]
    fn events_from_measured_step() {
        // Row at 1500, band 300..900 (middle 600), each + event moves it +60: 15 events up.
        assert_eq!(events_needed(1500.0, 60.0, (300.0, 900.0), 60), Some(-15));
        assert_eq!(events_needed(100.0, 60.0, (300.0, 900.0), 60), Some(8));
        // Reversed backend: + moves it down the screen.
        assert_eq!(events_needed(1500.0, -60.0, (300.0, 900.0), 60), Some(15));
        assert_eq!(events_needed(600.0, 60.0, (300.0, 900.0), 60), Some(0));
        assert_eq!(events_needed(1500.0, 0.0, (300.0, 900.0), 60), None);
        assert_eq!(events_needed(90000.0, 60.0, (300.0, 900.0), 60), Some(-60));
    }

    #[test]
    fn glides_in_one_burst_after_one_probe() {
        let (out, sent, pos) = run(
            List {
                pos: 2000.0,
                step: 45.0,
                range: (-5000.0, 5000.0),
                sent: vec![],
            },
            (200.0, 1000.0),
        );
        assert!(matches!(out, Glided::InBand(_)), "{out:?}");
        assert!(inside((pos, pos), (200.0, 1000.0)));
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0], (-1, 1));
        assert_eq!(sent[1].0, -1);
        assert!(sent[1].1 > 10);
    }

    #[test]
    fn end_of_list_stops() {
        // The row cannot rise above 1100 (last row of the list): the probe does not move it.
        let (out, sent, _) = run(
            List {
                pos: 1100.0,
                step: 45.0,
                range: (1100.0, 5000.0),
                sent: vec![],
            },
            (200.0, 1000.0),
        );
        assert_eq!(out, Glided::Stuck(1100.0));
        assert_eq!(sent, vec![(-1, 1)]);
        // The list ends part way through the burst: no further bursts.
        let (out, sent, _) = run(
            List {
                pos: 3000.0,
                step: 45.0,
                range: (2000.0, 5000.0),
                sent: vec![],
            },
            (200.0, 1000.0),
        );
        // The list ends part way through the burst; the correction finds it still.
        assert_eq!(out, Glided::Stuck(2000.0));
        assert_eq!(sent.len(), 3);
    }

    #[test]
    fn wrong_direction_reverses_once() {
        // This backend's + events move the row up the screen.
        let (out, sent, _) = run(
            List {
                pos: 100.0,
                step: -30.0,
                range: (-5000.0, 5000.0),
                sent: vec![],
            },
            (300.0, 900.0),
        );
        assert!(matches!(out, Glided::InBand(_)), "{out:?}");
        assert_eq!(sent[0], (1, 1));
        assert_eq!(sent[1].0, -1);
        assert_eq!(sent.len(), 2);
    }

    #[test]
    fn eased_short_burst_gets_one_correction() {
        // The probe moves 40 px, the burst only 30 px per event: one correction burst.
        let list = RefCell::new((2000.0f64, 0u32, vec![]));
        let out = plan_glide(
            (500.0, 560.0),
            0,
            |sign, count| {
                let mut l = list.borrow_mut();
                let per = if count == 1 && l.1 == 0 { 40.0 } else { 30.0 };
                l.0 += per * f64::from(sign) * f64::from(count);
                l.1 += 1;
                l.2.push((sign, count));
                Ok(())
            },
            || {
                let p = list.borrow().0;
                Ok(Some((p, p)))
            },
        )
        .unwrap();
        assert!(matches!(out, Glided::InBand(_)), "{out:?}");
        assert_eq!(list.borrow().2.len(), 3);
    }

    #[test]
    fn merged_burst_events_are_corrected_not_taken_for_the_end() {
        // The game handles only two thirds of a burst's events: the correction lands it.
        let list = RefCell::new((2000.0f64, vec![]));
        let out = plan_glide(
            (200.0, 1000.0),
            0,
            |sign, count| {
                let mut l = list.borrow_mut();
                let per = if count == 1 { 45.0 } else { 30.0 };
                l.0 += per * f64::from(sign) * f64::from(count);
                l.1.push((sign, count));
                Ok(())
            },
            || {
                let p = list.borrow().0;
                Ok(Some((p, p)))
            },
        )
        .unwrap();
        assert!(matches!(out, Glided::InBand(_)), "{out:?}");
        assert_eq!(list.borrow().1, vec![(-1, 1), (-1, 30), (-1, 15)]);
    }

    #[test]
    fn row_span_must_fit_whole() {
        // A 60 px row: its centre aims at the band's middle, so the whole row lands inside.
        let row = RefCell::new(1500.0f64);
        let out = plan_glide(
            (300.0, 400.0),
            0,
            |sign, count| {
                *row.borrow_mut() += 20.0 * f64::from(sign) * f64::from(count);
                Ok(())
            },
            || {
                let top = *row.borrow();
                Ok(Some((top, top + 60.0)))
            },
        )
        .unwrap();
        let top = *row.borrow();
        assert!(top >= 300.0 && top + 60.0 <= 400.0, "{out:?} {top}");
    }

    #[test]
    fn unknown_position_searches_or_stops() {
        let calls = RefCell::new(0);
        let out = plan_glide(
            (0.0, 10.0),
            0,
            |_, _| Ok(()),
            || {
                *calls.borrow_mut() += 1;
                Ok(None)
            },
        )
        .unwrap();
        assert_eq!(out, Glided::Unknown);
        assert_eq!(*calls.borrow(), 1);
        // Searching downwards one event at a time until the row appears, then gliding.
        let pos = RefCell::new((None::<f64>, 0));
        let out = plan_glide(
            (300.0, 900.0),
            -1,
            |sign, count| {
                let mut p = pos.borrow_mut();
                p.1 += 1;
                p.0 = match p.0 {
                    None if p.1 >= 3 => Some(1400.0),
                    None => None,
                    Some(v) => Some(v + 50.0 * f64::from(sign) * f64::from(count)),
                };
                Ok(())
            },
            || Ok(pos.borrow().0.map(|p| (p, p))),
        )
        .unwrap();
        assert!(matches!(out, Glided::InBand(_)), "{out:?}");
    }

    #[test]
    fn to_end_bursts_until_still() {
        let pos = RefCell::new((0.0f64, vec![]));
        let end = plan_to_end(
            |count| {
                let mut p = pos.borrow_mut();
                p.0 = (p.0 + 15.0 * f64::from(count)).min(400.0);
                p.1.push(count);
                Ok(())
            },
            || Ok(pos.borrow().0),
        )
        .unwrap();
        assert_eq!(end, 400.0);
        assert_eq!(pos.borrow().1, vec![1, MAX_BURST, 1]);
        // Already at the end: one probe only.
        let sent = RefCell::new(0);
        let end = plan_to_end(
            |c| {
                *sent.borrow_mut() += c;
                Ok(())
            },
            || Ok(7.0),
        )
        .unwrap();
        assert_eq!((end, *sent.borrow()), (7.0, 1));
    }

    #[test]
    fn zoom_events_from_screen_offset() {
        let area = (520.0, 160.0, 1500.0, 800.0);
        assert_eq!(zoom_out_events((960.0, 540.0), area, 4), 0);
        // 1.21 times too far right: two events (1.1^2).
        assert_eq!(zoom_out_events((960.0 + 540.0 * 1.2, 540.0), area, 8), 2);
        assert_eq!(zoom_out_events((960.0, 540.0 + 260.0 * 1.05), area, 8), 1);
        assert_eq!(zoom_out_events((-9000.0, 540.0), area, 4), 4);
    }
}
