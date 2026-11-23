//! Read-only map data from the owned game's content (install, or install plus enabled mods): province unit
//! positions (map/unitstacks.txt) and state province lists (history/states).
//! Used only to orient battle-plan drags on screen. The state province lists hold land provinces
//! only: a lake a state lists (map/definition.csv) is left out, so no map point falls on water.
use crate::error::{Error, Result};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

pub(super) type Positions = HashMap<i32, (f64, f64)>;

/// Game install directory of an executable at `<dir>/hoi4.app/Contents/MacOS/hoi4`
/// (macOS) or `<dir>/hoi4` (Linux).
pub(super) fn install_dir(exe: &str) -> Result<PathBuf> {
    let levels: &[usize] = if cfg!(target_os = "linux") {
        &[1]
    } else {
        &[4]
    };
    let ancestors: Vec<&Path> = Path::new(exe).ancestors().collect();
    levels
        .iter()
        .filter_map(|&n| ancestors.get(n).copied())
        .find(|d| d.join("map").is_dir() && d.join("history").is_dir())
        .map(Path::to_path_buf)
        .ok_or_else(|| Error::new("map_data_unavailable", "Game install not found"))
}
/// The game content every reader uses: the install, or with enabled mods a merged view of the
/// install and the mods in load order (see `super::modlayer`), so map, history, common and
/// localisation files follow the mods the game itself loaded.
pub(super) fn game_dir(exe: &str) -> Result<PathBuf> {
    let install = install_dir(exe)?;
    Ok(super::modlayer::content_dir(&install, exe).unwrap_or(install))
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

/// Parse the type-0 (standing unit) height of every province from unitstacks lines: the terrain
/// under the unit model, which the camera sees raised above the flat map (Ethiopia's highlands stand
/// 7-9 units above the plains, 80-140 px on screen at the close zoom).
fn parse_heights(text: &str) -> HashMap<i32, f64> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let mut f = line.split(';');
        let (Some(p), Some(t), Some(_), Some(y)) = (f.next(), f.next(), f.next(), f.next()) else {
            continue;
        };
        if t.trim() != "0" {
            continue;
        }
        if let (Ok(p), Ok(y)) = (p.trim().parse(), y.trim().parse()) {
            out.entry(p).or_insert(y);
        }
    }
    out
}

/// Unit model heights by province (map/unitstacks.txt, cached per content folder).
pub(super) fn unit_heights(dir: &Path) -> Result<std::sync::Arc<HashMap<i32, f64>>> {
    type Heights = std::sync::Arc<HashMap<i32, f64>>;
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Heights>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Ok(found.clone());
    }
    let text = std::fs::read_to_string(dir.join("map/unitstacks.txt"))
        .map_err(|_| Error::new("map_data_unavailable", "Province positions unreadable"))?;
    let parsed = std::sync::Arc::new(parse_heights(&text));
    cache
        .lock()
        .unwrap()
        .insert(dir.to_path_buf(), parsed.clone());
    Ok(parsed)
}

pub(super) fn positions(dir: &Path) -> Result<std::sync::Arc<Positions>> {
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

/// State id -> its starting infrastructure level (history/states `infrastructure = N`), read
/// once per install. Construction places factories where infrastructure is high.
pub(super) type LevelIndex = std::sync::Arc<HashMap<i32, i32>>;
pub(super) fn state_infrastructure(dir: &Path) -> Result<LevelIndex> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, LevelIndex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Ok(found.clone());
    }
    let entries = std::fs::read_dir(dir.join("history/states"))
        .map_err(|_| Error::new("map_data_unavailable", "State history unreadable"))?;
    let mut index = HashMap::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(id) = name
            .split('-')
            .next()
            .and_then(|n| n.trim().parse::<i32>().ok())
        else {
            continue;
        };
        if let Ok(text) = std::fs::read_to_string(entry.path()) {
            let level = text.lines().find_map(|l| {
                let l = l.split('#').next().unwrap_or("").trim();
                let rest = l
                    .strip_prefix("infrastructure")?
                    .trim_start()
                    .strip_prefix('=')?;
                rest.trim().parse::<i32>().ok()
            });
            if let Some(level) = level {
                index.insert(id, level);
            }
        }
    }
    let index = std::sync::Arc::new(index);
    cache
        .lock()
        .unwrap()
        .insert(dir.to_path_buf(), index.clone());
    Ok(index)
}
/// State id -> its victory point provinces (province, value), read once per install.
pub(super) type VictoryIndex = std::sync::Arc<HashMap<i32, Vec<(i32, f64)>>>;
pub(super) fn state_victory_points(dir: &Path) -> Result<VictoryIndex> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, VictoryIndex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Ok(found.clone());
    }
    let entries = std::fs::read_dir(dir.join("history/states"))
        .map_err(|_| Error::new("map_data_unavailable", "State history unreadable"))?;
    let mut index = HashMap::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(id) = name
            .split('-')
            .next()
            .and_then(|n| n.trim().parse::<i32>().ok())
        else {
            continue;
        };
        if let Ok(text) = std::fs::read_to_string(entry.path()) {
            let found = super::railway::parse_victory_points(&text);
            if !found.is_empty() {
                index.insert(id, found);
            }
        }
    }
    let index = std::sync::Arc::new(index);
    cache
        .lock()
        .unwrap()
        .insert(dir.to_path_buf(), index.clone());
    Ok(index)
}
/// Factories a state starts with (history/states `buildings`, dated history blocks left out):
/// industrial complexes, arms factories and dockyards.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Factories {
    pub civilian: i64,
    pub military: i64,
    pub naval: i64,
}
impl Factories {
    pub(super) fn total(&self) -> i64 {
        self.civilian + self.military + self.naval
    }
}
/// (state id, starting factories) of one history/states file.
pub(super) fn parse_state_factories(text: &str) -> Option<(i32, Factories)> {
    use crate::script::Block as _;
    let root = crate::script::parse(text);
    let state = root.find_val("state")?.block();
    let id = state.number("id")? as i32;
    let buildings = state
        .find_val("history")
        .map(|h| h.block())
        .and_then(|h| h.find_val("buildings"))
        .map(|b| b.block())
        .unwrap_or(&[]);
    let n = |key: &str| buildings.number(key).unwrap_or(0.0).max(0.0) as i64;
    Some((
        id,
        Factories {
            civilian: n("industrial_complex"),
            military: n("arms_factory"),
            naval: n("dockyard"),
        },
    ))
}
/// State id -> its starting factories, read once per content folder (mods included).
pub(super) fn state_factories(dir: &Path) -> std::sync::Arc<HashMap<i32, Factories>> {
    crate::script::cached(dir, "state_factories", |dir| {
        let mut index = HashMap::new();
        for entry in std::fs::read_dir(dir.join("history/states"))
            .into_iter()
            .flatten()
            .flatten()
        {
            if let Ok(bytes) = std::fs::read(entry.path()) {
                if let Some((id, f)) = parse_state_factories(&String::from_utf8_lossy(&bytes)) {
                    index.insert(id, f);
                }
            }
        }
        index
    })
}
/// State id -> its land provinces (history/states, lakes left out), read once per install.
pub(super) type StateIndex = std::sync::Arc<HashMap<i32, Vec<i32>>>;
pub(super) fn state_province_index(dir: &Path) -> Result<StateIndex> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, StateIndex>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Ok(found.clone());
    }
    let entries = std::fs::read_dir(dir.join("history/states"))
        .map_err(|_| Error::new("map_data_unavailable", "State history unreadable"))?;
    let water = water_provinces(dir);
    let mut index = HashMap::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(id) = name
            .split('-')
            .next()
            .and_then(|n| n.trim().parse::<i32>().ok())
        else {
            continue;
        };
        if let Ok(text) = std::fs::read_to_string(entry.path()) {
            let found = land_only(&water, parse_state_provinces(&text));
            if !found.is_empty() {
                index.insert(id, found);
            }
        }
    }
    let index = std::sync::Arc::new(index);
    cache
        .lock()
        .unwrap()
        .insert(dir.to_path_buf(), index.clone());
    Ok(index)
}
/// The land provinces of one state (history/states, lakes left out).
pub(super) fn state_provinces(dir: &Path, state: i32) -> Result<Vec<i32>> {
    let entries = std::fs::read_dir(dir.join("history/states"))
        .map_err(|_| Error::new("map_data_unavailable", "State history unreadable"))?;
    for entry in entries.flatten() {
        if state_file_id(&entry.file_name().to_string_lossy()) == Some(state) {
            let text = std::fs::read_to_string(entry.path())
                .map_err(|_| Error::new("map_data_unavailable", "State history unreadable"))?;
            let found = land_only(&water_provinces(dir), parse_state_provinces(&text));
            if !found.is_empty() {
                return Ok(found);
            }
        }
    }
    Err(Error::new("invalid_target", "Unknown state"))
}

/// The state id a history/states file name starts with: `7-Holland.txt` and `977 - Antwerp.txt`
/// alike (1.19 added states whose file names put spaces around the dash; read as `977-` they had
/// no provinces and no map position, and every order naming them was refused).
pub(super) fn state_file_id(name: &str) -> Option<i32> {
    name.split('-')
        .next()
        .and_then(|v| v.trim().parse::<i32>().ok())
}

/// Mean map position of every state's land provinces, read once per install.
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
    let water = water_provinces(dir);
    let mut out = HashMap::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Some(id) = state_file_id(&name) else {
            continue;
        };
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let provinces = land_only(&water, parse_state_provinces(&text));
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
    cache
        .lock()
        .unwrap()
        .insert(dir.to_path_buf(), parsed.clone());
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
        let Some(id) = state_file_id(&name) else {
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
        let Some(id) = state_file_id(&name) else {
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

/// Provinces that are not land (map/definition.csv `id;r;g;b;type;coastal;terrain;continent`
/// with the type `sea` or `lake`), read once per install. A state's history file lists the lakes
/// inside it beside its land (Begemder lists Lake Tana): a point on one is no place for a
/// battle-plan line or a move order (live 15 Nov 2026: every offensive line toward Begemder
/// failed, its ends and centre were taken from a list with the lake in it). Empty when the file
/// is missing or unreadable: every listed province then counts as land, as before.
pub(super) fn water_provinces(dir: &Path) -> std::sync::Arc<std::collections::HashSet<i32>> {
    type Water = std::sync::Arc<std::collections::HashSet<i32>>;
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Water>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return found.clone();
    }
    let out: Water = std::sync::Arc::new(
        std::fs::read(dir.join("map/definition.csv"))
            .map(|bytes| parse_water(&String::from_utf8_lossy(&bytes)))
            .unwrap_or_default(),
    );
    cache.lock().unwrap().insert(dir.to_path_buf(), out.clone());
    out
}
/// Province ids of the definition rows whose type is given and is not `land`.
pub(super) fn parse_water(text: &str) -> std::collections::HashSet<i32> {
    text.lines()
        .filter_map(|line| {
            let mut f = line.split(';');
            let id = f.next()?.trim().parse::<i32>().ok()?;
            let kind = f.nth(3)?.trim();
            (!kind.is_empty() && !kind.eq_ignore_ascii_case("land")).then_some(id)
        })
        .collect()
}
/// `provinces` without the water among them. A list that would be left empty is kept whole
/// (a state always has land; an empty answer would only hide the state).
fn land_only(water: &std::collections::HashSet<i32>, provinces: Vec<i32>) -> Vec<i32> {
    if water.is_empty() || !provinces.iter().any(|p| water.contains(p)) {
        return provinces;
    }
    let land: Vec<i32> = provinces
        .iter()
        .copied()
        .filter(|p| !water.contains(p))
        .collect();
    if land.is_empty() {
        provinces
    } else {
        land
    }
}

/// Coastal land provinces (map/definition.csv: id;r;g;b;land;true;terrain;continent).
pub(super) fn coastal_provinces(
    dir: &Path,
) -> Result<std::sync::Arc<std::collections::HashSet<i32>>> {
    static CACHE: OnceLock<
        Mutex<HashMap<PathBuf, std::sync::Arc<std::collections::HashSet<i32>>>>,
    > = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Ok(found.clone());
    }
    let text = std::fs::read_to_string(dir.join("map/definition.csv"))
        .map_err(|_| Error::new("map_data_unavailable", "Province definitions unreadable"))?;
    let out: std::collections::HashSet<i32> = parse_coastal(&text);
    let out = std::sync::Arc::new(out);
    cache.lock().unwrap().insert(dir.to_path_buf(), out.clone());
    Ok(out)
}
pub(super) fn parse_coastal(text: &str) -> std::collections::HashSet<i32> {
    text.lines()
        .filter_map(|line| {
            let f: Vec<&str> = line.split(';').collect();
            (f.len() > 5 && f[4].trim() == "land" && f[5].trim() == "true")
                .then(|| f[0].trim().parse::<i32>().ok())
                .flatten()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_state_file_name_with_spaces_around_the_dash_names_its_state() {
        assert_eq!(state_file_id("7-Holland.txt"), Some(7));
        assert_eq!(state_file_id("977 - Antwerp.txt"), Some(977));
        assert_eq!(state_file_id("1080 - Marlborough.txt"), Some(1080));
        assert_eq!(state_file_id("readme.txt"), None);
    }
    #[test]
    fn parses_unit_positions_and_state_provinces() {
        let p = parse_positions("2;0;3429.00;9.85;1699.00;0.00;0.30\n2;1;1;1;1;0;0\nbad\n");
        assert_eq!(p.get(&2), Some(&(3429.0, 1699.0)));
        assert_eq!(p.len(), 1);
        let h = parse_heights("2;0;3429.00;9.85;1699.00;0.00;0.30\n7944;0;3417.00;16.87;849.00;0.00;0.40\n2;1;1;5;1;0;0\n");
        assert_eq!(h.get(&2), Some(&9.85));
        assert_eq!(h.get(&7944), Some(&16.87));
        let s =
            "state={\n id=86\n history={ victory_points = { 1 2 } }\n provinces={\n 3 4 5\n }\n}";
        assert_eq!(parse_state_provinces(s), vec![3, 4, 5]);
    }
    #[test]
    fn reads_the_starting_factories_of_a_state() {
        let s = "state={ id=3 history={ owner = AAA buildings = { infrastructure = 4 \
                 industrial_complex = 2 arms_factory = 3 dockyard = 1 77 = { naval_base = 3 } } \
                 1939.1.1 = { buildings = { arms_factory = 9 } } } provinces={ 1 2 } }";
        let (id, f) = parse_state_factories(s).unwrap();
        assert_eq!(id, 3);
        assert_eq!((f.civilian, f.military, f.naval, f.total()), (2, 3, 1, 6));
        let (_, none) = parse_state_factories("state={ id=4 history={ owner = AAA } }").unwrap();
        assert_eq!(none.total(), 0);
        assert!(parse_state_factories("nothing = yes").is_none());
    }
    #[test]
    fn parses_coastal_land_provinces() {
        let c = parse_coastal("0;0;0;0;land;false;unknown;0\n12;1;2;3;land;true;plains;1\n13;1;2;4;sea;true;ocean;0\n");
        assert!(c.contains(&12) && !c.contains(&13) && c.len() == 1);
    }
    #[test]
    fn parses_the_provinces_that_are_not_land() {
        let w = parse_water(
            "0;0;0;0;land;false;unknown;0\n1;230;81;119;lake;false;lakes;7\n12;1;2;3;land;true;plains;1\n\
             13;1;2;4;sea;true;ocean;0\n14;1;2;5\nbad;1;2;3;lake\n15;1;2;6;LAND;false;hills;1\n",
        );
        assert_eq!(w, [1, 13].into_iter().collect());
        // nothing known about water, or no water in the list: the list is returned as it is
        assert_eq!(land_only(&Default::default(), vec![3, 4]), vec![3, 4]);
        assert_eq!(land_only(&w, vec![12, 14]), vec![12, 14]);
        assert_eq!(land_only(&w, vec![12, 1, 14, 13]), vec![12, 14]);
        // a list of water alone is kept whole
        assert_eq!(land_only(&w, vec![1, 13]), vec![1, 13]);
    }
    /// A content folder with two states: state 841 lists the lake 13137 beside its land.
    fn content(name: &str, definition: Option<&str>) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hoi4-mapdata-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("history/states")).unwrap();
        std::fs::create_dir_all(dir.join("map")).unwrap();
        std::fs::write(
            dir.join("history/states/841-Begemder.txt"),
            "state={ id=841 provinces={ 100 13137 101 } }",
        )
        .unwrap();
        std::fs::write(
            dir.join("history/states/842-Tigray.txt"),
            "state={ id=842 provinces={ 200 201 } }",
        )
        .unwrap();
        // the lake lies far to the east of the state's land
        std::fs::write(
            dir.join("map/unitstacks.txt"),
            "100;0;10.0;0;10.0;0;0\n101;0;20.0;0;10.0;0;0\n13137;0;300.0;0;10.0;0;0\n\
             200;0;10.0;0;40.0;0;0\n201;0;20.0;0;40.0;0;0\n",
        )
        .unwrap();
        if let Some(text) = definition {
            std::fs::write(dir.join("map/definition.csv"), text).unwrap();
        }
        dir
    }
    #[test]
    fn a_lake_is_no_province_of_its_state() {
        let dir = content(
            "lake",
            Some("100;1;1;1;land;false;hills;5\n101;1;1;2;land;false;hills;5\n13137;151;30;108;lake;false;lakes;5\n"),
        );
        assert_eq!(state_provinces(&dir, 841).unwrap(), vec![100, 101]);
        assert_eq!(state_provinces(&dir, 842).unwrap(), vec![200, 201]);
        let index = state_province_index(&dir).unwrap();
        assert_eq!(index.get(&841), Some(&vec![100, 101]));
        // the centre is the mean of the land, not pulled toward the lake
        assert_eq!(state_centers(&dir).unwrap().get(&841), Some(&(15.0, 10.0)));
        // an army in 842 stands north of the state: seen from the state it lies straight up the
        // screen (x right, y down); with the lake in the list the direction pointed sideways
        let (dx, dy) = direction_to(&dir, 841, &[200, 201]).unwrap();
        assert!(dx.abs() < 1e-9 && (dy + 1.0).abs() < 1e-9, "{dx} {dy}");
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn without_province_definitions_every_listed_province_is_kept() {
        let dir = content("nodef", None);
        assert!(water_provinces(&dir).is_empty());
        assert_eq!(state_provinces(&dir, 841).unwrap(), vec![100, 13137, 101]);
        assert_eq!(
            state_province_index(&dir).unwrap().get(&841),
            Some(&vec![100, 13137, 101])
        );
        assert_eq!(state_centers(&dir).unwrap().get(&841), Some(&(110.0, 10.0)));
        let _ = std::fs::remove_dir_all(&dir);
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
