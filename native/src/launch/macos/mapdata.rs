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
pub(super) fn parse_state_provinces(text: &str) -> Vec<i32> {
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

/// Mean map position of every state's provinces, read once per install.
pub(super) fn state_centers(dir: &Path) -> Result<std::sync::Arc<HashMap<i32, (f64, f64)>>> {
    type Centers = HashMap<i32, (f64, f64)>;
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, std::sync::Arc<Centers>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Ok(found.clone());
    }
    let pos = positions(dir)?;
    let entries = std::fs::read_dir(dir.join("history/states"))
        .map_err(|_| Error::new("map_data_unavailable", "State history unreadable"))?;
    let mut out = HashMap::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(id) = name.split('-').next().and_then(|v| v.parse::<i32>().ok()) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let provinces = parse_state_provinces(&text);
        if let Some(center) = mean(provinces.iter().filter_map(|p| pos.get(p).copied())) {
            out.insert(id, center);
        }
    }
    let out = std::sync::Arc::new(out);
    cache.lock().unwrap().insert(dir.to_path_buf(), out.clone());
    Ok(out)
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

/// `state;air_base;x;y;z;rotation;adjacent` rows of map/buildings.txt: state -> (x, z).
pub(super) fn parse_air_bases(text: &str) -> HashMap<i32, (f64, f64)> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let mut f = line.split(';');
        let (Some(s), Some(kind), Some(x), Some(_), Some(z)) =
            (f.next(), f.next(), f.next(), f.next(), f.next())
        else {
            continue;
        };
        if kind.trim() != "air_base" {
            continue;
        }
        if let (Ok(s), Ok(x), Ok(z)) = (s.trim().parse(), x.trim().parse(), z.trim().parse()) {
            out.entry(s).or_insert((x, z));
        }
    }
    out
}
/// Map position of every state's air base, read once per install.
pub(super) fn air_base_positions(dir: &Path) -> Result<std::sync::Arc<HashMap<i32, (f64, f64)>>> {
    type Bases = HashMap<i32, (f64, f64)>;
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, std::sync::Arc<Bases>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Ok(found.clone());
    }
    let text = std::fs::read_to_string(dir.join("map/buildings.txt"))
        .map_err(|_| Error::new("map_data_unavailable", "Building positions unreadable"))?;
    let parsed = std::sync::Arc::new(parse_air_bases(&text));
    cache.lock().unwrap().insert(dir.to_path_buf(), parsed.clone());
    Ok(parsed)
}

/// Provinces listed in a strategic region file's `provinces = { ... }` block.
pub(super) fn parse_region_provinces(text: &str) -> Vec<i32> {
    parse_state_provinces(text)
}
type RegionMap = HashMap<i32, i32>;
/// Region holding most of a state's provinces: (region id, provinces in it, provinces).
pub(super) type StateRegions = HashMap<i32, (i32, usize, usize)>;
/// Province -> strategic region id, read once per install (map/strategicregions/N-Name.txt).
pub(super) fn province_regions(dir: &Path) -> Result<std::sync::Arc<RegionMap>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, std::sync::Arc<RegionMap>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Ok(found.clone());
    }
    let entries = std::fs::read_dir(dir.join("map/strategicregions"))
        .map_err(|_| Error::new("map_data_unavailable", "Strategic regions unreadable"))?;
    let mut out = HashMap::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(id) = name.split('-').next().and_then(|v| v.parse::<i32>().ok()) else {
            continue;
        };
        if let Ok(text) = std::fs::read_to_string(entry.path()) {
            for p in parse_region_provinces(&text) {
                out.entry(p).or_insert(id);
            }
        }
    }
    let out = std::sync::Arc::new(out);
    cache.lock().unwrap().insert(dir.to_path_buf(), out.clone());
    Ok(out)
}
/// Region holding most provinces of each state: state -> (region id, provinces in it, provinces).
pub(super) fn state_regions(dir: &Path) -> Result<std::sync::Arc<StateRegions>> {
    type Out = StateRegions;
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, std::sync::Arc<Out>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Ok(found.clone());
    }
    let regions = province_regions(dir)?;
    let entries = std::fs::read_dir(dir.join("history/states"))
        .map_err(|_| Error::new("map_data_unavailable", "State history unreadable"))?;
    let mut out = HashMap::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(id) = name.split('-').next().and_then(|v| v.parse::<i32>().ok()) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let provinces = parse_state_provinces(&text);
        let mut votes: HashMap<i32, usize> = HashMap::new();
        for p in &provinces {
            if let Some(r) = regions.get(p) {
                *votes.entry(*r).or_default() += 1;
            }
        }
        if let Some((r, n)) = votes.into_iter().max_by_key(|(r, n)| (*n, -*r)) {
            out.insert(id, (r, n, provinces.len()));
        }
    }
    let out = std::sync::Arc::new(out);
    cache.lock().unwrap().insert(dir.to_path_buf(), out.clone());
    Ok(out)
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
    #[test]
    fn parses_air_base_rows() {
        let b = parse_air_bases("64;arms_factory;1;2;3;0;0\n64;air_base;3024.78;9.70;1565.10;5.08;0\nbad\n52;air_base;2975.56;12.30;1479.55;-0.12;0\n");
        assert_eq!(b.get(&64), Some(&(3024.78, 1565.10)));
        assert_eq!(b.get(&52), Some(&(2975.56, 1479.55)));
        assert_eq!(b.len(), 2);
        assert_eq!(parse_region_provinces("provinces={ 1 2 3 }"), vec![1, 2, 3]);
    }
}
