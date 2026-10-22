//! Pure bounded candidate planning. Supplied rectangles are not rendering proof;
//! callers retain the complete tree witnesses and recheck before mouse-down.
use super::{inside, Rect, SAFE};
use crate::error::{Error, Result};

const INSET: f64 = 16.0;
const MARGIN: f64 = 24.0;
const MAX_OBSTACLES: usize = 8192;
const MAX_CANDIDATES: usize = 1232;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Pan {
    pub from: (f64, f64),
    pub to: (f64, f64),
}

fn unavailable(message: &str) -> Error {
    Error::new("gui_target_unavailable", message)
}
fn positive(rect: &Rect) -> Result<()> {
    if rect.w <= 0 || rect.h <= 0 {
        return Err(unavailable(
            "Positive Focus rectangles required for pan planning",
        ));
    }
    Ok(())
}
fn bounds() -> (f64, f64, f64, f64) {
    (
        SAFE.0 + INSET,
        SAFE.1 + INSET,
        SAFE.2 - INSET,
        SAFE.3 - INSET,
    )
}
fn axis(low: f64, high: f64, stride: f64) -> Vec<f64> {
    let intervals = ((high - low) / stride).floor() as usize;
    let mut values: Vec<_> = (0..=intervals)
        .map(|index| low + index as f64 * stride)
        .collect();
    if values.last().copied() != Some(high) {
        values.push(high);
    }
    values
}
fn grid() -> (Vec<f64>, Vec<f64>) {
    let (left, top, right, bottom) = bounds();
    (axis(left, right, 37.0), axis(top, bottom, 29.0))
}
fn clear(point: (f64, f64), obstacles: &[Rect]) -> bool {
    obstacles.iter().all(|rect| {
        // Convert before adding: valid positive dimensions can extend beyond
        // i32::MAX even though each stored component fits in i32.
        let left = f64::from(rect.x);
        let top = f64::from(rect.y);
        let right = left + f64::from(rect.w);
        let bottom = top + f64::from(rect.h);
        point.0 < left - MARGIN
            || point.0 > right + MARGIN
            || point.1 < top - MARGIN
            || point.1 > bottom + MARGIN
    })
}
fn displacement(value: f64, low: f64, high: f64) -> f64 {
    if value < low {
        low + INSET - value
    } else if value >= high {
        high - INSET - value
    } else {
        0.0
    }
}

/// None means the target center is already inside the existing SAFE area.
/// Otherwise choose the most requested travel available among bounded clear
/// grid origins. This predicts a translation, not an actual pan or visibility.
pub(super) fn plan(target: &Rect, obstacles: &[Rect]) -> Result<Option<Pan>> {
    positive(target)?;
    if obstacles.len() > MAX_OBSTACLES {
        return Err(unavailable("Focus pan obstacle count exceeds its bound"));
    }
    for rect in obstacles {
        positive(rect)?;
    }
    if inside(target) {
        return Ok(None);
    }
    let center = target.center();
    let need = (
        displacement(center.0, SAFE.0, SAFE.2),
        displacement(center.1, SAFE.1, SAFE.3),
    );
    let original = need.0 * need.0 + need.1 * need.1;
    let (left, top, right, bottom) = bounds();
    let middle = ((left + right) / 2.0, (top + bottom) / 2.0);
    let (xs, ys) = grid();
    if xs.len().saturating_mul(ys.len()) > MAX_CANDIDATES {
        return Err(unavailable("Focus pan candidate grid exceeds its bound"));
    }
    let mut best = None;
    for y in ys {
        for &x in &xs {
            let from = (x, y);
            if !clear(from, obstacles) {
                continue;
            }
            let to = (
                (x + need.0).clamp(left, right),
                (y + need.1).clamp(top, bottom),
            );
            let remaining = (need.0 - (to.0 - x), need.1 - (to.1 - y));
            let residual = remaining.0 * remaining.0 + remaining.1 * remaining.1;
            if residual >= original {
                continue;
            }
            // Prefer an exact requested displacement, then the most progress.
            // Centered paths and ascending y/x make every tie deterministic.
            let midpoint = ((x + to.0) / 2.0, (y + to.1) / 2.0);
            let distance = (midpoint.0 - middle.0).powi(2) + (midpoint.1 - middle.1).powi(2);
            let score = (residual, distance, y, x);
            if best.as_ref().is_none_or(|(old, _)| score < *old) {
                best = Some((score, Pan { from, to }));
            }
        }
    }
    best.map(|(_, pan)| Some(pan))
        .ok_or_else(|| unavailable("No clear progressing Focus pan origin on the bounded grid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(x: i32, y: i32) -> Rect {
        Rect {
            x: x - 50,
            y: y - 44,
            w: 100,
            h: 88,
        }
    }
    fn shifted_center(rect: &Rect, pan: Pan) -> (f64, f64) {
        let center = rect.center();
        (
            center.0 + pan.to.0 - pan.from.0,
            center.1 + pan.to.1 - pan.from.1,
        )
    }
    fn endpoints(pan: Pan) {
        let (left, top, right, bottom) = bounds();
        for (x, y) in [pan.from, pan.to] {
            assert!((left..=right).contains(&x));
            assert!((top..=bottom).contains(&y));
        }
    }

    #[test]
    fn q6_rect_and_already_visible_centers_need_no_pan_or_extra_inset() {
        let q6 = Rect {
            x: 447,
            y: 164,
            w: 100,
            h: 88,
        };
        assert_eq!(plan(&q6, &[]).unwrap(), None);
        // Current semantics concern the center, not a whole-widget fit.
        let clipped = Rect {
            x: -500,
            y: -300,
            w: 2000,
            h: 1000,
        };
        assert_eq!(plan(&clipped, &[]).unwrap(), None);
        assert_eq!(plan(&target(260, 200), &[]).unwrap(), None);
        let half_pixel = Rect {
            x: 1859,
            y: 999,
            w: 1,
            h: 1,
        };
        assert_eq!(plan(&half_pixel, &[]).unwrap(), None);
    }

    #[test]
    fn far_left_right_up_and_down_request_only_the_needed_axis() {
        for (x, y, expected) in [
            (-1000, 600, (276.0, 600.0)),
            (3000, 600, (1844.0, 600.0)),
            (960, -300, (960.0, 216.0)),
            (960, 1700, (960.0, 984.0)),
        ] {
            let rect = target(x, y);
            let pan = plan(&rect, &[]).unwrap().unwrap();
            endpoints(pan);
            assert_eq!(shifted_center(&rect, pan), expected);
            if y == 600 {
                assert_eq!(pan.from.1, pan.to.1);
                assert!((pan.to.0 - pan.from.0).abs() > 700.0);
            } else {
                assert_eq!(pan.from.0, pan.to.0);
                assert!((pan.to.1 - pan.from.1).abs() > 350.0);
            }
        }
    }

    #[test]
    fn diagonal_requests_reach_the_near_inset_without_centering_the_tree() {
        for (x, y, expected) in [
            (-1000, -300, (276.0, 216.0)),
            (3000, 1700, (1844.0, 984.0)),
            (-1000, 1700, (276.0, 984.0)),
            (3000, -300, (1844.0, 216.0)),
        ] {
            let rect = target(x, y);
            let pan = plan(&rect, &[]).unwrap().unwrap();
            endpoints(pan);
            assert_eq!(shifted_center(&rect, pan), expected);
        }
    }

    #[test]
    fn nearest_displacement_preserves_visible_axes_and_half_open_boundaries() {
        for (x, y, expected) in [
            (259, 208, (276.0, 208.0)),
            (1860, 600, (1844.0, 600.0)),
            (497, 199, (497.0, 216.0)),
            (497, 1000, (497.0, 984.0)),
        ] {
            let rect = target(x, y);
            let pan = plan(&rect, &[]).unwrap().unwrap();
            assert_eq!(shifted_center(&rect, pan), expected);
        }
        let half_pixel = Rect {
            x: 1860,
            y: 599,
            w: 1,
            h: 1,
        };
        let pan = plan(&half_pixel, &[]).unwrap().unwrap();
        assert_eq!(shifted_center(&half_pixel, pan), (1844.0, 599.5));
    }

    #[test]
    fn far_diagonal_uses_endpoint_travel_without_reversal_or_overshoot() {
        let rect = target(-3000, -1500);
        let pan = plan(&rect, &[]).unwrap().unwrap();
        assert_eq!(pan.from, (276.0, 216.0));
        assert_eq!(pan.to, (1844.0, 984.0));
        assert_eq!(shifted_center(&rect, pan), (-1432.0, -732.0));
        endpoints(pan);
        let opposite = plan(&target(5000, 2500), &[]).unwrap().unwrap();
        assert_eq!(opposite.from, (1844.0, 984.0));
        assert_eq!(opposite.to, (276.0, 216.0));
    }

    #[test]
    fn a_clear_boundary_origin_is_used_when_the_central_grid_is_blocked() {
        // Expanded obstacle begins at x=277: only the x=276 grid column clears.
        let obstacle = Rect {
            x: 301,
            y: 0,
            w: 1619,
            h: 1080,
        };
        let pan = plan(&target(-3000, 600), &[obstacle]).unwrap().unwrap();
        assert_eq!(pan.from.0, 276.0);
        assert_eq!(pan.to.0, 1844.0);
        assert_eq!(pan.from.1, pan.to.1);
        assert!(clear(pan.from, &[obstacle]));
    }

    #[test]
    fn all_blocked_or_only_zero_progress_origins_refuse_without_fallback() {
        let dense = Rect {
            x: 260,
            y: 200,
            w: 1600,
            h: 800,
        };
        let no_progress = Rect {
            x: 0,
            y: 0,
            w: 1819,
            h: 1080,
        };
        // The only clear column is x=1844, which cannot move farther right.
        assert!(clear((1844.0, 600.0), &[no_progress]));
        for obstacles in [[dense], [no_progress]] {
            let error = plan(&target(-1000, 600), &obstacles).unwrap_err();
            assert_eq!(error.code, "gui_target_unavailable");
            assert!(error.message.contains("No clear progressing"));
        }
    }

    #[test]
    fn exact_twenty_four_pixel_margin_remains_excluded_on_every_edge() {
        let rect = Rect {
            x: 300,
            y: 300,
            w: 40,
            h: 40,
        };
        for point in [
            (276.0, 320.0),
            (364.0, 320.0),
            (320.0, 276.0),
            (320.0, 364.0),
        ] {
            assert!(!clear(point, &[rect]));
        }
        for point in [
            (275.5, 320.0),
            (364.5, 320.0),
            (320.0, 275.5),
            (320.0, 364.5),
        ] {
            assert!(clear(point, &[rect]));
        }
    }

    #[test]
    fn grid_has_both_endpoints_no_duplicates_and_at_most_1232_candidates() {
        let (xs, ys) = grid();
        assert_eq!((xs.first(), xs.last()), (Some(&276.0), Some(&1844.0)));
        assert_eq!((ys.first(), ys.last()), (Some(&216.0), Some(&984.0)));
        assert!(xs.windows(2).all(|x| x[1] > x[0] && x[1] - x[0] <= 37.0));
        assert!(ys.windows(2).all(|y| y[1] > y[0] && y[1] - y[0] <= 29.0));
        let points: std::collections::BTreeSet<_> = ys
            .iter()
            .flat_map(|y| xs.iter().map(move |x| (*x as i32, *y as i32)))
            .collect();
        assert_eq!((xs.len(), ys.len()), (44, 28));
        assert_eq!(points.len(), xs.len() * ys.len());
        assert_eq!(points.len(), MAX_CANDIDATES);
    }

    #[test]
    fn obstacle_order_and_repeated_calls_do_not_change_the_selected_candidate() {
        let obstacles = [
            Rect {
                x: 900,
                y: 450,
                w: 150,
                h: 300,
            },
            Rect {
                x: 400,
                y: 200,
                w: 50,
                h: 700,
            },
            Rect {
                x: 1500,
                y: 250,
                w: 100,
                h: 400,
            },
        ];
        let rect = target(-1000, -300);
        let first = plan(&rect, &obstacles).unwrap();
        assert_eq!(first, plan(&rect, &obstacles).unwrap());
        let reversed: Vec<_> = obstacles.into_iter().rev().collect();
        assert_eq!(first, plan(&rect, &reversed).unwrap());
        let pan = first.unwrap();
        assert!(clear(pan.from, &reversed));
        assert_eq!(shifted_center(&rect, pan), (276.0, 216.0));
    }

    #[test]
    fn all_quadrants_make_strict_progress_without_moving_a_visible_axis() {
        for x in [-3000, -1000, 259, 260, 500, 1859, 1860, 3000] {
            for y in [-1500, -300, 199, 200, 600, 999, 1000, 1800] {
                let rect = target(x, y);
                let Some(pan) = plan(&rect, &[]).unwrap() else {
                    assert!(inside(&rect));
                    continue;
                };
                endpoints(pan);
                let end = shifted_center(&rect, pan);
                for (old, new, low, high) in [
                    (f64::from(x), end.0, SAFE.0, SAFE.2),
                    (f64::from(y), end.1, SAFE.1, SAFE.3),
                ] {
                    if old < low {
                        assert!(new >= old && new <= low + INSET);
                    } else if old >= high {
                        assert!(new <= old && new >= high - INSET);
                    } else {
                        assert_eq!(new, old);
                    }
                }
                assert_ne!(end, (f64::from(x), f64::from(y)));
            }
        }
    }

    #[test]
    fn malformed_dimensions_and_excessive_obstacles_are_refused() {
        for (w, h) in [(0, 1), (1, 0), (-1, 1), (1, -1)] {
            let invalid = Rect { x: 0, y: 0, w, h };
            assert_eq!(
                plan(&invalid, &[]).unwrap_err().code,
                "gui_target_unavailable"
            );
            assert_eq!(
                plan(&target(-1000, 600), &[invalid]).unwrap_err().code,
                "gui_target_unavailable"
            );
        }
        let rect = Rect {
            x: -10000,
            y: -10000,
            w: 1,
            h: 1,
        };
        let mut obstacles = vec![rect; MAX_OBSTACLES];
        assert_eq!(plan(&target(960, 600), &obstacles).unwrap(), None);
        obstacles.push(rect);
        assert_eq!(
            plan(&target(-1000, 600), &obstacles).unwrap_err().code,
            "gui_target_unavailable"
        );
    }

    #[test]
    fn large_valid_coordinates_do_not_overflow_obstacle_edges_or_target_center() {
        let obstacle = Rect {
            x: i32::MAX - 1,
            y: i32::MAX - 1,
            w: 100,
            h: 100,
        };
        let pan = plan(&target(-1000, 600), &[obstacle]).unwrap().unwrap();
        assert_eq!(shifted_center(&target(-1000, 600), pan), (276.0, 600.0));
        let huge_target = Rect {
            x: i32::MAX - 1,
            y: 500,
            w: 100,
            h: 88,
        };
        let pan = plan(&huge_target, &[]).unwrap().unwrap();
        endpoints(pan);
        assert_eq!(pan.from.0 - pan.to.0, 1568.0);
        assert_eq!(pan.from.1, pan.to.1);
    }
}
