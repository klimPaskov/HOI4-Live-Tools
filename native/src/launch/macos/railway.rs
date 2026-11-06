//! `hoi4_railway_build`: queue one railway with the construction panel's railway mode.
//!
//! The game's own route (railway mode tooltip, 1.19.4.1, read live 6 Nov 2026): Shift+Click on a
//! node (a supply hub, a naval base or the capital) and click the target province: the game builds
//! the shortest path, re-using existing railways. Start and target come from the game's (or a
//! mod's) map files: `map/supply_nodes.txt`, the state history's naval bases and
//! `map/unitstacks.txt` positions, projected through the live camera. Verified by the own
//! construction queue holding more lines than before (railway lines carry a path, not a state
//! building, so the line count is the evidence).
use super::mapdata;
use super::ui::{clear_notices, find_state, read};
use crate::{
    error::{Error, Result},
    platform::{game_input::GameInput, Process},
};
use serde_json::{json, Value};
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

const GAME: (u32, u32) = (1920, 1080);
/// Left edge of the open map while the construction panel is shown (game points).
const MAP_LEFT: f64 = 740.0;

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_railway_build","description":"Queue ONE railway with the game's railway construction mode: Shift+Click on a start node (a supply hub, naval base or the capital; the game builds the shortest path re-using existing railways) and a click on the target province. Give the start as `from_province` or `from_state` (its supply hub or naval base from the map files) and the target as `to_province` or `to_state` (its supply hub, else its largest victory point). Both ends must be own or controlled land the game lets railways cross. Verified by the own construction queue holding one more line or one more railway level (a path already queued gets another level on its line). Typed refusals before input: no_node_in_state (lists what to build), province_not_in_state, unknown_state. Railways cost 170 per level (+130 per existing level) and are the way to carry supply from the capital to far fronts.","inputSchema":{"type":"object","properties":{"from_province":{"type":"integer","minimum":1,"maximum":99999},"from_state":{"type":"integer","minimum":1,"maximum":4095},"to_province":{"type":"integer","minimum":1,"maximum":99999},"to_state":{"type":"integer","minimum":1,"maximum":4095},"request_id":{"type":"string"},"lease_token":{"type":"string"}},"required":["request_id","lease_token"],"additionalProperties":false}})
}

/// Supply hub provinces listed in `map/supply_nodes.txt` (`level province` per line).
pub(super) fn parse_supply_nodes(text: &str) -> Vec<i32> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split_whitespace();
            let _level = it.next()?;
            it.next()?.parse::<i32>().ok()
        })
        .collect()
}
/// Provinces holding a naval base in a state history file (`PROVINCE = { naval_base = N }`).
pub(super) fn parse_naval_base_provinces(text: &str) -> Vec<i32> {
    let mut out = Vec::new();
    let mut last_number: Option<i32> = None;
    for token in text
        .split(|c: char| c.is_whitespace() || c == '=' || c == '{' || c == '}')
        .filter(|t| !t.is_empty())
    {
        if token == "naval_base" {
            if let Some(p) = last_number.take() {
                if p > 0 && !out.contains(&p) {
                    out.push(p);
                }
            }
        } else if let Ok(n) = token.parse::<i32>() {
            last_number = Some(n);
        } else {
            last_number = None;
        }
    }
    out
}
/// Victory point provinces of a state history file, largest value first.
pub(super) fn parse_victory_points(text: &str) -> Vec<(i32, f64)> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(at) = rest.find("victory_points") {
        rest = &rest[at + "victory_points".len()..];
        let Some(open) = rest.find('{') else { break };
        let Some(close) = rest[open..].find('}') else {
            break;
        };
        let inner = &rest[open + 1..open + close];
        // Comments (`#Massaua`) run to the end of their line.
        let nums: Vec<&str> = inner
            .lines()
            .map(|l| l.split('#').next().unwrap_or(""))
            .flat_map(str::split_whitespace)
            .collect();
        if nums.len() >= 2 {
            if let (Ok(p), Ok(v)) = (nums[0].parse::<i32>(), nums[1].parse::<f64>()) {
                out.push((p, v));
            }
        }
        rest = &rest[open + close..];
    }
    out.sort_by(|a, b| b.1.total_cmp(&a.1));
    out
}

fn state_text(dir: &Path, state: i32) -> Result<String> {
    let prefix = format!("{state}-");
    let entries = std::fs::read_dir(dir.join("history/states"))
        .map_err(|_| Error::new("map_data_unavailable", "State history unreadable"))?;
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            return std::fs::read_to_string(entry.path())
                .map_err(|_| Error::new("map_data_unavailable", "State history unreadable"));
        }
    }
    Err(Error::new(
        "unknown_state",
        format!("No state {state} in the map files"),
    ))
}

/// Start node of a state: a supply hub listed for one of its provinces, else a naval base.
fn node_of_state(dir: &Path, state: i32) -> Result<i32> {
    let provinces = mapdata::state_provinces(dir, state)?;
    let hubs = std::fs::read_to_string(dir.join("map/supply_nodes.txt"))
        .map(|t| parse_supply_nodes(&t))
        .unwrap_or_default();
    if let Some(p) = provinces.iter().find(|p| hubs.contains(p)) {
        return Ok(*p);
    }
    if let Some(p) = parse_naval_base_provinces(&state_text(dir, state)?).first() {
        return Ok(*p);
    }
    Err(Error::new(
        "no_node_in_state",
        format!(
            "State {state} has no supply hub or naval base in the map files to start a railway from: \
             pass from_province (a hub built during the game, or the capital) or build a supply hub \
             there first (hoi4_construction_add building supply_node)"
        ),
    ))
}
/// Target province of a state: its supply hub, else its largest victory point, else the first.
fn target_of_state(dir: &Path, state: i32) -> Result<i32> {
    let provinces = mapdata::state_provinces(dir, state)?;
    let hubs = std::fs::read_to_string(dir.join("map/supply_nodes.txt"))
        .map(|t| parse_supply_nodes(&t))
        .unwrap_or_default();
    if let Some(p) = provinces.iter().find(|p| hubs.contains(p)) {
        return Ok(*p);
    }
    let text = state_text(dir, state)?;
    if let Some((p, _)) = parse_victory_points(&text)
        .into_iter()
        .find(|(p, _)| provinces.contains(p))
    {
        return Ok(p);
    }
    provinces
        .first()
        .copied()
        .ok_or_else(|| Error::new("unknown_state", format!("State {state} lists no provinces")))
}
fn state_of(dir: &Path, province: i32) -> Result<i32> {
    super::wars::province_states(dir)?
        .get(&province)
        .copied()
        .ok_or_else(|| {
            Error::new(
                "province_not_in_state",
                format!("Province {province} belongs to no state in the map files"),
            )
        })
}
/// (province, its state) for one end from the arguments.
fn end(dir: &Path, province: Option<i32>, state: Option<i32>, start: bool) -> Result<(i32, i32)> {
    match (province, state) {
        (Some(p), Some(s)) => {
            if !mapdata::state_provinces(dir, s)?.contains(&p) {
                return Err(Error::new(
                    "province_not_in_state",
                    format!("Province {p} is not in state {s}"),
                ));
            }
            Ok((p, s))
        }
        (Some(p), None) => Ok((p, state_of(dir, p)?)),
        (None, Some(s)) => Ok((
            if start {
                node_of_state(dir, s)?
            } else {
                target_of_state(dir, s)?
            },
            s,
        )),
        (None, None) => Err(Error::new(
            "invalid_arguments",
            if start {
                "Give the start as from_province or from_state"
            } else {
                "Give the target as to_province or to_state"
            },
        )),
    }
}

/// Pan the camera with the arrow keys until `province` projects on screen (at most 24 taps).
fn pan_to(
    process: &dyn Process,
    keys: &dyn GameInput,
    dir: &Path,
    province: i32,
) -> Result<(f64, f64)> {
    use crate::platform::game_input::vk;
    let positions = mapdata::positions(dir)?;
    let &(x, z) = positions.get(&province).ok_or_else(|| {
        Error::new(
            "map_data_unavailable",
            format!("No map position for province {province}"),
        )
    })?;
    for _ in 0..24 {
        let p = read(process, |r, i| super::screenmap::to_screen(r, i, x, z))?;
        let Some((sx, sy)) = p else {
            break;
        };
        if (MAP_LEFT..1700.0).contains(&sx) && (120.0..900.0).contains(&sy) {
            return Ok((sx, sy));
        }
        if sx < MAP_LEFT {
            keys.press_code(vk::LEFT)?;
        } else if sx >= 1700.0 {
            keys.press_code(vk::RIGHT)?;
        }
        if sy < 120.0 {
            keys.press_code(vk::UP)?;
        } else if sy >= 900.0 {
            keys.press_code(vk::DOWN)?;
        }
        thread::sleep(Duration::from_millis(220));
    }
    Err(Error::new(
        "gui_target_unavailable",
        format!("Province {province} could not be brought on screen with the arrow keys"),
    ))
}
/// Queue lines plus the levels queued on railway lines: a railway along a path already in the
/// queue raises that line's level count instead of adding a line (live 6 Nov 2026: four
/// Asmara-Tigray orders made one line of 4 levels, and the line count never moved).
fn queue_lines(process: &dyn Process) -> Result<i64> {
    let lines = crate::economy::snapshot(process)?["construction_projects_queued"]
        .as_i64()
        .ok_or_else(|| Error::new("construction_target_unavailable", "Queue lines unreadable"))?;
    let page = crate::construction::page(process, 0, 128)?;
    Ok(lines + railway_levels(&page))
}
pub(super) fn railway_levels(page: &Value) -> i64 {
    page["lines"]
        .as_array()
        .map(|rows| {
            rows.iter()
                .filter(|r| r["line_kind"] == "railway")
                .filter_map(|r| r["amount_raw_signed"].as_i64())
                .sum()
        })
        .unwrap_or(0)
}

pub(super) fn build(
    process: &dyn Process,
    keys: &dyn GameInput,
    from: (Option<i32>, Option<i32>),
    to: (Option<i32>, Option<i32>),
) -> Result<Value> {
    let dir = process
        .identity()
        .and_then(|i| mapdata::game_dir(&i.exe).ok())
        .ok_or_else(|| Error::new("map_data_unavailable", "Game install not found"))?;
    let (from_p, from_s) = end(&dir, from.0, from.1, true)?;
    let (to_p, to_s) = end(&dir, to.0, to.1, false)?;
    clear_notices(process, keys)?;
    let before = queue_lines(process)?;
    super::construction::select_building(process, keys, "rail_way")?;
    let result = (|| -> Result<Value> {
        find_state(process, keys, from_s)?;
        thread::sleep(Duration::from_millis(350));
        // The construction panel covers the left of the screen: both ends are clicked in the
        // open map to its right (a Shift+Click on Asmara landed on the panel, live 6 Nov 2026).
        let (x, y) = pan_to(process, keys, &dir, from_p)?;
        keys.shift_click(GAME, x, y)?;
        thread::sleep(Duration::from_millis(400));
        // The Find view would end the railway in progress (live 6 Nov 2026: 'Railway must start
        // on a supply node'), so reach the target with the arrow keys, projecting after each tap.
        let (x, y) = pan_to(process, keys, &dir, to_p)?;
        keys.click(GAME, x, y)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let now = queue_lines(process)?;
            if now > before {
                return Ok(
                    json!({"from_province":from_p,"from_state":from_s,"to_province":to_p,
                    "to_state":to_s,"queue_work_before":before,"queue_work_after":now,
                    "note":"queue work = construction lines + railway levels queued"}),
                );
            }
            if Instant::now() >= deadline {
                return Err(Error::new(
                    "railway_not_queued",
                    format!(
                        "Shift+Click on province {from_p} and a click on province {to_p} queued no \
                         railway within 5 s (no new line, no new railway level). Most likely {from_p} is not a railway node now (a \
                         supply hub, naval base or the capital) or no railway path joins them \
                         through own territory; nothing was retried"
                    ),
                ));
            }
            thread::sleep(Duration::from_millis(60));
        }
    })();
    super::construction::leave(process, keys);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn railway_levels_sum_only_railway_lines() {
        let page = json!({"lines":[{"line_kind":"building","amount_raw_signed":1},
            {"line_kind":"railway","amount_raw_signed":4},{"line_kind":"railway","amount_raw_signed":2}]});
        assert_eq!(railway_levels(&page), 6);
        assert_eq!(railway_levels(&json!({})), 0);
    }
    #[test]
    fn supply_nodes_file_lines_give_provinces() {
        assert_eq!(
            parse_supply_nodes("1 67 \n1 101 \n\n2 13250\n"),
            vec![67, 101, 13250]
        );
    }
    #[test]
    fn naval_bases_come_from_province_blocks_only() {
        let text = "buildings = {\n infrastructure = 2\n air_base = 3\n 12766 = {\n naval_base = 3\n }\n}\n";
        assert_eq!(parse_naval_base_provinces(text), vec![12766]);
    }
    #[test]
    fn victory_points_are_sorted_largest_first() {
        let text = "victory_points = { #Massaua\n 12766 1\n }\n victory_points = { 13250 3 }\n";
        assert_eq!(parse_victory_points(text), vec![(13250, 3.0), (12766, 1.0)]);
    }
}
