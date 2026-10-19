//! Read-only vanilla map data from the owned game's install: province unit
//! positions (map/unitstacks.txt) and state province lists (history/states).
//! Used only to orient battle-plan drags on screen.
use crate::error::{Error, Result};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

type Positions = HashMap<i32, (f64, f64)>;

/// Game install directory of an executable at `<dir>/hoi4.app/Contents/MacOS/hoi4`.
pub(super) fn game_dir(exe: &str) -> Result<PathBuf> {
    Path::new(exe)
        .ancestors()
        .nth(4)
        .filter(|d| d.join("map").is_dir() && d.join("history").is_dir())
        .map(Path::to_path_buf)
        .ok_or_else(|| Error::new("map_data_unavailable", "Game install not found"))
}

/// Parse unitstacks lines `province;type;x;y;z;rotation;offset`; the type-0
/// (standing unit) entry gives the province position on the map plane.
fn parse_positions(text: &str) -> Positions {
    let mut out = HashMap::new();
    for line in text.lines() {
        let mut f = line.split(';');
        let (Some(p), Some(t), Some(x), Some(_), Some(z)) =
            (f.next(), f.next(), f.next(), f.next(), f.next())
        else {
            continue;
        };
        if t.trim() != "0" {
            continue;
        }
        if let (Ok(p), Ok(x), Ok(z)) = (p.trim().parse(), x.trim().parse(), z.trim().parse()) {
            out.entry(p).or_insert((x, z));
        }
    }
    out
}

fn positions(dir: &Path) -> Result<std::sync::Arc<Positions>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, std::sync::Arc<Positions>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Ok(found.clone());
    }
    let text = std::fs::read_to_string(dir.join("map/unitstacks.txt"))
        .map_err(|_| Error::new("map_data_unavailable", "Province positions unreadable"))?;
    let parsed = std::sync::Arc::new(parse_positions(&text));
    cache
        .lock()
        .unwrap()
        .insert(dir.to_path_buf(), parsed.clone());
    Ok(parsed)
}

/// Province IDs listed in a state history file's `provinces = { ... }` block.
fn parse_state_provinces(text: &str) -> Vec<i32> {
    let Some(start) = text.find("provinces") else {
        return Vec::new();
    };
    let rest = &text[start..];
    let (Some(open), Some(close)) = (rest.find('{'), rest.find('}')) else {
        return Vec::new();
    };
    if close < open {
        return Vec::new();
    }
    rest[open + 1..close]
        .split_whitespace()
        .filter_map(|v| v.parse().ok())
        .collect()
}

pub(super) fn state_provinces(dir: &Path, state: i32) -> Result<Vec<i32>> {
    let prefix = format!("{state}-");
    let entries = std::fs::read_dir(dir.join("history/states"))
        .map_err(|_| Error::new("map_data_unavailable", "State history unreadable"))?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(&prefix) {
            let text = std::fs::read_to_string(entry.path())
                .map_err(|_| Error::new("map_data_unavailable", "State history unreadable"))?;
            let found = parse_state_provinces(&text);
            if !found.is_empty() {
                return Ok(found);
            }
        }
    }
    Err(Error::new("invalid_target", "Unknown state"))
}

fn mean(points: impl Iterator<Item = (f64, f64)>) -> Option<(f64, f64)> {
    let (mut sx, mut sz, mut n) = (0.0, 0.0, 0.0);
    for (x, z) in points {
        sx += x;
        sz += z;
        n += 1.0;
    }
    (n > 0.0).then(|| (sx / n, sz / n))
}

/// Unit screen direction (x right, y down) from state `state` toward the
/// mean position of `provinces` (the army's divisions).
pub(super) fn direction_to(dir: &Path, state: i32, provinces: &[i32]) -> Result<(f64, f64)> {
    let pos = positions(dir)?;
    let target = mean(
        state_provinces(dir, state)?
            .iter()
            .filter_map(|p| pos.get(p).copied()),
    )
    .ok_or_else(|| Error::new("map_data_unavailable", "State position unknown"))?;
    let origin = mean(provinces.iter().filter_map(|p| pos.get(p).copied()))
        .ok_or_else(|| Error::new("map_data_unavailable", "Army position unknown"))?;
    // Map z grows northwards; screen y grows downwards.
    let (dx, dy) = (origin.0 - target.0, target.1 - origin.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 {
        return Err(Error::new(
            "invalid_target",
            "The army already stands in this state",
        ));
    }
    Ok((dx / len, dy / len))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_unit_positions_and_state_provinces() {
        let p = parse_positions("2;0;3429.00;9.85;1699.00;0.00;0.30\n2;1;1;1;1;0;0\nbad\n");
        assert_eq!(p.get(&2), Some(&(3429.0, 1699.0)));
        assert_eq!(p.len(), 1);
        let s =
            "state={\n id=86\n history={ victory_points = { 1 2 } }\n provinces={\n 3 4 5\n }\n}";
        assert_eq!(parse_state_provinces(s), vec![3, 4, 5]);
    }
}
