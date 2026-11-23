//! Terrain, rivers, forts and special crossings per land province and per
//! province border, from the map files of the running game's content (pure file
//! reads, no game memory):
//!
//! - terrain: `map/definition.csv` (`id;r;g;b;type;coastal;terrain;continent`);
//!   the terrain's combat numbers (combat width, support width, attack
//!   modifier, movement cost, attrition) from `common/terrain`.
//! - rivers: `map/rivers.bmp` (8 bpp, palette indices) against `map/provinces.bmp`:
//!   every 4-neighbour pixel pair of two different land provinces is a border
//!   pixel; a pair where either pixel is a river pixel (index <= the large-river
//!   stop index) is a river pixel of that border. A border mostly drawn by a
//!   river (river share >= RIVER_SHARE) is a river crossing, large when its
//!   widest index is past the small-river stop index (the river defines of
//!   `common/defines`: RIVER_SMALL_STOP_INDEX, RIVER_LARGE_STOP_INDEX and the
//!   crossing penalties). Which rule the engine applies to a border touched by a
//!   river only at a point is not known: those report `touches` (estimate until
//!   the battle read shows the river modifier of a real attack).
//! - special crossings: `map/adjacencies.csv` (straits, impassable borders,
//!   canals with their rule).
//! - forts: the starting `bunker` / `coastal_bunker` levels per province from
//!   `history/states` (undated, and per dated history block); the live levels
//!   are game memory (not read here).
use super::script::{self, Block};
use crate::error::{Error, Result};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

/// A border counts as drawn by a river when at least this share of its pixels is river.
pub(super) const RIVER_SHARE: f64 = 0.5;

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Border {
    pub pixels: u32,
    pub river_pixels: u32,
    pub river_index_max: Option<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RiverRules {
    pub small_stop: u8,
    pub large_stop: u8,
    pub penalty_small: f64,
    pub penalty_large: f64,
}
impl Default for RiverRules {
    fn default() -> Self {
        Self {
            small_stop: 6,
            large_stop: 11,
            penalty_small: -0.3,
            penalty_large: -0.6,
        }
    }
}

impl Border {
    pub(super) fn share(&self) -> f64 {
        if self.pixels == 0 {
            0.0
        } else {
            f64::from(self.river_pixels) / f64::from(self.pixels)
        }
    }
    /// "large" | "small" (a river crossing), "touches" (a river meets the border), None.
    pub(super) fn river(&self, rules: &RiverRules) -> Option<&'static str> {
        let index = self.river_index_max?;
        if self.share() >= RIVER_SHARE {
            Some(if index > rules.small_stop {
                "large"
            } else {
                "small"
            })
        } else {
            Some("touches")
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Category {
    pub combat_width: Option<f64>,
    pub combat_support_width: Option<f64>,
    pub attack: Option<f64>,
    pub movement_cost: Option<f64>,
    pub attrition: Option<f64>,
    pub supply_flow_penalty: Option<f64>,
    pub is_water: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Special {
    pub kind: String,
    pub through: Option<i32>,
    pub rule: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Fort {
    pub building: String,
    pub level: f64,
    /// None: the undated starting history; Some(date): a dated history block.
    pub from: Option<String>,
}

#[derive(Default)]
pub(super) struct MapTerrain {
    pub terrain: HashMap<i32, String>,
    pub coastal: HashSet<i32>,
    pub borders: HashMap<(i32, i32), Border>,
    pub special: HashMap<(i32, i32), Vec<Special>>,
    pub forts: HashMap<i32, Vec<Fort>>,
    pub categories: HashMap<String, Category>,
    pub rivers: RiverRules,
}

fn pair(a: i32, b: i32) -> (i32, i32) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// definition.csv: packed rgb -> (id, land, coastal, terrain).
pub(super) fn parse_definition(text: &str) -> HashMap<u32, (i32, bool, bool, String)> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split(';').collect();
        if f.len() < 7 {
            continue;
        }
        let (Ok(id), Ok(r), Ok(g), Ok(b)) = (
            f[0].trim().parse::<i32>(),
            f[1].trim().parse::<u32>(),
            f[2].trim().parse::<u32>(),
            f[3].trim().parse::<u32>(),
        ) else {
            continue;
        };
        if id <= 0 || r > 255 || g > 255 || b > 255 {
            continue;
        }
        out.insert(
            (r << 16) | (g << 8) | b,
            (
                id,
                f[4].trim() == "land",
                f[5].trim() == "true",
                f[6].trim().to_string(),
            ),
        );
    }
    out
}

struct Bmp<'a> {
    data: &'a [u8],
    offset: usize,
    width: usize,
    height: usize,
    stride: usize,
    bpp: u16,
}

fn bmp<'a>(data: &'a [u8], want_bpp: u16, name: &'static str) -> Result<Bmp<'a>> {
    let bad = |m: String| Error::new("map_data_unavailable", m);
    if data.len() < 54 || &data[..2] != b"BM" {
        return Err(bad(format!("{name} is not a BMP")));
    }
    let le32 = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap());
    let offset = le32(10) as usize;
    let width = le32(18) as i32;
    let height = le32(22) as i32;
    let bpp = u16::from_le_bytes([data[28], data[29]]);
    if width <= 0 || height == 0 || bpp != want_bpp || le32(30) != 0 {
        return Err(bad(format!(
            "{name} must be an uncompressed {want_bpp} bpp BMP"
        )));
    }
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    let stride = (w * usize::from(bpp) / 8).div_ceil(4) * 4;
    if offset.saturating_add(stride * h) > data.len() {
        return Err(bad(format!("{name} is truncated")));
    }
    Ok(Bmp {
        data,
        offset,
        width: w,
        height: h,
        stride,
        bpp,
    })
}

impl Bmp<'_> {
    fn row(&self, y: usize) -> &[u8] {
        let start = self.offset + y * self.stride;
        &self.data[start..start + self.width * usize::from(self.bpp) / 8]
    }
}

/// Border pixels and river pixels of every pair of touching land provinces.
/// `rivers` must have the same size and row order as `provinces`.
pub(super) fn borders(
    provinces: &[u8],
    rivers: &[u8],
    defs: &HashMap<u32, (i32, bool, bool, String)>,
    rules: &RiverRules,
) -> Result<HashMap<(i32, i32), Border>> {
    let p = bmp(provinces, 24, "provinces.bmp")?;
    let r = bmp(rivers, 8, "rivers.bmp")?;
    if (p.width, p.height) != (r.width, r.height) {
        return Err(Error::new(
            "map_data_unavailable",
            "rivers.bmp and provinces.bmp differ in size",
        ));
    }
    // colour -> land province id (0 = not land), one lookup per colour change
    let mut land: HashMap<u32, i32> = HashMap::new();
    for (colour, (id, is_land, _, _)) in defs {
        land.insert(*colour, if *is_land { *id } else { 0 });
    }
    let ids: Vec<Vec<i32>> = (0..p.height)
        .map(|y| {
            let row = p.row(y);
            let mut out = Vec::with_capacity(p.width);
            let (mut last, mut last_id) = (u32::MAX, 0);
            for x in 0..p.width {
                let o = x * 3;
                let c = (u32::from(row[o + 2]) << 16)
                    | (u32::from(row[o + 1]) << 8)
                    | u32::from(row[o]);
                if c != last {
                    last = c;
                    last_id = land.get(&c).copied().unwrap_or(0);
                }
                out.push(last_id);
            }
            out
        })
        .collect();
    let river = |x: usize, y: usize| -> Option<u8> {
        let v = r.row(y)[x];
        (v <= rules.large_stop).then_some(v)
    };
    let mut out: HashMap<(i32, i32), Border> = HashMap::new();
    let mut note = |a: i32, b: i32, ra: Option<u8>, rb: Option<u8>| {
        if a == b || a == 0 || b == 0 {
            return;
        }
        let e = out.entry(pair(a, b)).or_default();
        e.pixels += 1;
        let idx = ra.max(rb);
        if idx.is_some() {
            e.river_pixels += 1;
            e.river_index_max = e.river_index_max.max(idx);
        }
    };
    for y in 0..p.height {
        for x in 0..p.width {
            let here = ids[y][x];
            if x > 0 && ids[y][x - 1] != here {
                note(ids[y][x - 1], here, river(x - 1, y), river(x, y));
            }
            if y > 0 && ids[y - 1][x] != here {
                note(ids[y - 1][x], here, river(x, y - 1), river(x, y));
            }
        }
    }
    Ok(out)
}

/// The river defines (number defines of common/defines/*.lua).
pub(super) fn river_rules(defines: &str) -> RiverRules {
    let d = RiverRules::default();
    let get = |name: &str| super::leader_act::define(defines, name);
    RiverRules {
        small_stop: get("RIVER_SMALL_STOP_INDEX").map_or(d.small_stop, |v| v as u8),
        large_stop: get("RIVER_LARGE_STOP_INDEX").map_or(d.large_stop, |v| v as u8),
        penalty_small: get("RIVER_CROSSING_PENALTY").unwrap_or(d.penalty_small),
        penalty_large: get("RIVER_CROSSING_PENALTY_LARGE").unwrap_or(d.penalty_large),
    }
}

/// common/terrain: `categories = { forest = { combat_width = 60 ... units = { attack = -0.15 } } }`.
pub(super) fn categories(entries: &[script::Entry]) -> HashMap<String, Category> {
    let mut out = HashMap::new();
    for cats in entries.all("categories") {
        for e in cats.block() {
            let b = e.val.block();
            if b.is_empty() {
                continue;
            }
            let units = b.find_val("units").map(|v| v.block()).unwrap_or(&[]);
            out.insert(
                e.key.clone(),
                Category {
                    combat_width: b.number("combat_width"),
                    combat_support_width: b.number("combat_support_width"),
                    attack: units.number("attack"),
                    movement_cost: b.number("movement_cost"),
                    attrition: b.number("attrition"),
                    supply_flow_penalty: b.number("supply_flow_penalty_factor"),
                    is_water: b.text("is_water") == Some("yes"),
                },
            );
        }
    }
    out
}

/// adjacencies.csv `From;To;Type;Through;x;y;x;y;rule;Comment` -> special crossings.
pub(super) fn parse_adjacencies(text: &str) -> HashMap<(i32, i32), Vec<Special>> {
    let mut out: HashMap<(i32, i32), Vec<Special>> = HashMap::new();
    for line in text.lines().skip(1) {
        let f: Vec<&str> = line.split(';').collect();
        if f.len() < 4 {
            continue;
        }
        let (Ok(a), Ok(b)) = (f[0].trim().parse::<i32>(), f[1].trim().parse::<i32>()) else {
            continue;
        };
        if a <= 0 || b <= 0 {
            continue;
        }
        let kind = match f[2].trim() {
            "" => "land".to_string(),
            k => k.to_string(),
        };
        let through = f[3].trim().parse::<i32>().ok().filter(|t| *t > 0);
        let rule = f
            .get(8)
            .map(|r| r.trim())
            .filter(|r| !r.is_empty())
            .map(str::to_string);
        out.entry(pair(a, b)).or_default().push(Special {
            kind,
            through,
            rule,
        });
    }
    out
}

/// Province buildings `PROVINCE = { bunker = 2 }` of one state file's history, undated and per
/// dated block.
pub(super) fn state_forts(entries: &[script::Entry]) -> Vec<(i32, Fort)> {
    let mut out = Vec::new();
    let mut take = |buildings: &[script::Entry], from: Option<String>| {
        for e in buildings {
            let Ok(province) = e.key.parse::<i32>() else {
                continue;
            };
            for b in e.val.block() {
                if b.key == "bunker" || b.key == "coastal_bunker" {
                    if let Some(level) = b.val.scalar().and_then(|s| s.parse::<f64>().ok()) {
                        out.push((
                            province,
                            Fort {
                                building: b.key.clone(),
                                level,
                                from: from.clone(),
                            },
                        ));
                    }
                }
            }
        }
    };
    for state in entries.all("state") {
        for history in state.block().all("history") {
            let h = history.block();
            for b in h.all("buildings") {
                take(b.block(), None);
            }
            for dated in h {
                let k = &dated.key;
                if k.split('.').count() == 3 && k.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    for b in dated.val.block().all("buildings") {
                        take(b.block(), Some(k.clone()));
                    }
                }
            }
        }
    }
    out
}

fn read(dir: &Path, rel: &str) -> Result<Vec<u8>> {
    std::fs::read(dir.join(rel))
        .map_err(|_| Error::new("map_data_unavailable", format!("{rel} unreadable")))
}

fn load(dir: &Path) -> Result<MapTerrain> {
    let definition = String::from_utf8_lossy(&read(dir, "map/definition.csv")?).to_string();
    let defs = parse_definition(&definition);
    let mut defines = String::new();
    if let Ok(entries) = std::fs::read_dir(dir.join("common/defines")) {
        let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for p in paths {
            if p.extension().is_some_and(|e| e == "lua") {
                defines.push_str(&std::fs::read_to_string(&p).unwrap_or_default());
                defines.push('\n');
            }
        }
    }
    let rivers = river_rules(&defines);
    let borders = borders(
        &read(dir, "map/provinces.bmp")?,
        &read(dir, "map/rivers.bmp")?,
        &defs,
        &rivers,
    )?;
    let mut terrain = HashMap::new();
    let mut coastal = HashSet::new();
    for (id, land, coast, kind) in defs.into_values() {
        if land {
            terrain.insert(id, kind);
            if coast {
                coastal.insert(id);
            }
        }
    }
    let special = std::fs::read_to_string(dir.join("map/adjacencies.csv"))
        .map(|t| parse_adjacencies(&t))
        .unwrap_or_default();
    let mut forts: HashMap<i32, Vec<Fort>> = HashMap::new();
    if let Ok(entries) = std::fs::read_dir(dir.join("history/states")) {
        for e in entries.flatten() {
            let Ok(text) = std::fs::read_to_string(e.path()) else {
                continue;
            };
            for (province, fort) in state_forts(&script::parse(&text)) {
                forts.entry(province).or_default().push(fort);
            }
        }
    }
    let categories = categories(&script::parse_dir(&dir.join("common/terrain")));
    Ok(MapTerrain {
        terrain,
        coastal,
        borders,
        special,
        forts,
        categories,
        rivers,
    })
}

/// The map terrain of a content directory, built once per directory.
pub(super) fn map_terrain(dir: &Path) -> Result<Arc<MapTerrain>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<MapTerrain>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Ok(found.clone());
    }
    let built = Arc::new(load(dir)?);
    cache
        .lock()
        .unwrap()
        .insert(dir.to_path_buf(), built.clone());
    Ok(built)
}

impl MapTerrain {
    /// One province: terrain with its combat numbers, coast, starting forts.
    pub(super) fn province(&self, id: i32) -> Value {
        let kind = self.terrain.get(&id);
        let cat = kind.and_then(|k| self.categories.get(k));
        let forts: Vec<Value> = self
            .forts
            .get(&id)
            .map(|f| {
                f.iter()
                    .map(|f| json!({"building": f.building, "level": f.level, "from": f.from.clone().unwrap_or_else(|| "start".into())}))
                    .collect()
            })
            .unwrap_or_default();
        json!({"terrain": kind,
            "combat_width": cat.and_then(|c| c.combat_width),
            "combat_support_width": cat.and_then(|c| c.combat_support_width),
            "attack_modifier": cat.and_then(|c| c.attack),
            "movement_cost": cat.and_then(|c| c.movement_cost),
            "attrition": cat.and_then(|c| c.attrition),
            "coastal": self.coastal.contains(&id),
            "forts_in_history": forts})
    }
    /// The border between two provinces: river (with the attack penalty it gives), special crossings.
    pub(super) fn border(&self, a: i32, b: i32) -> Value {
        let e = self.borders.get(&pair(a, b));
        let river = e.and_then(|e| e.river(&self.rivers));
        let penalty = match river {
            Some("large") => Some(self.rivers.penalty_large),
            Some("small") => Some(self.rivers.penalty_small),
            _ => None,
        };
        let special: Vec<Value> = self
            .special
            .get(&pair(a, b))
            .map(|s| {
                s.iter()
                    .map(|s| json!({"kind": s.kind, "through": s.through, "rule": s.rule}))
                    .collect()
            })
            .unwrap_or_default();
        json!({"province": b, "river": river,
            "river_attack_penalty": penalty,
            "river_share": e.map(|e| (e.share() * 100.0).round() / 100.0),
            "border_pixels": e.map(|e| e.pixels),
            "special": special})
    }
    pub(super) fn rules(&self) -> Value {
        json!({"river": {"small_stop_index": self.rivers.small_stop,
                "large_stop_index": self.rivers.large_stop,
                "attack_penalty_small": self.rivers.penalty_small,
                "attack_penalty_large": self.rivers.penalty_large,
                "crossing_when_share_at_least": RIVER_SHARE,
                "touches": "a river meets the border without drawing it: whether the engine applies the penalty is not known yet (the battle read will show it)"},
            "forts": "forts_in_history are the bunker levels of the state history (start, or the dated block that sets them); the live levels are not read here"})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny BMP: 24 bpp from rgb rows (top row first, stored bottom-up) or 8 bpp indices.
    fn bmp24(rows: &[&[u32]]) -> Vec<u8> {
        let (w, h) = (rows[0].len(), rows.len());
        let stride = (w * 3).div_ceil(4) * 4;
        let mut v = header(w, h, 24, stride, 0);
        for row in rows.iter().rev() {
            let mut line = vec![0u8; stride];
            for (x, c) in row.iter().enumerate() {
                line[x * 3] = (*c & 0xff) as u8;
                line[x * 3 + 1] = ((*c >> 8) & 0xff) as u8;
                line[x * 3 + 2] = ((*c >> 16) & 0xff) as u8;
            }
            v.extend(line);
        }
        v
    }
    fn bmp8(rows: &[&[u8]]) -> Vec<u8> {
        let (w, h) = (rows[0].len(), rows.len());
        let stride = w.div_ceil(4) * 4;
        let mut v = header(w, h, 8, stride, 1024);
        v.extend(vec![0u8; 1024]);
        for row in rows.iter().rev() {
            let mut line = vec![0u8; stride];
            line[..w].copy_from_slice(row);
            v.extend(line);
        }
        v
    }
    fn header(w: usize, h: usize, bpp: u16, stride: usize, palette: usize) -> Vec<u8> {
        let offset = 54 + palette;
        let mut v = vec![0u8; 54];
        v[0] = b'B';
        v[1] = b'M';
        v[2..6].copy_from_slice(&((offset + stride * h) as u32).to_le_bytes());
        v[10..14].copy_from_slice(&(offset as u32).to_le_bytes());
        v[14..18].copy_from_slice(&40u32.to_le_bytes());
        v[18..22].copy_from_slice(&(w as u32).to_le_bytes());
        v[22..26].copy_from_slice(&(h as u32).to_le_bytes());
        v[26..28].copy_from_slice(&1u16.to_le_bytes());
        v[28..30].copy_from_slice(&bpp.to_le_bytes());
        v
    }

    const A: u32 = 0x0000_0001;
    const B: u32 = 0x0000_0002;
    const C: u32 = 0x0000_0003;
    const SEA: u32 = 0x0000_0004;
    const NO: u8 = 255;

    fn defs() -> HashMap<u32, (i32, bool, bool, String)> {
        parse_definition(
            "0;0;0;0;land;false;unknown;0\n1;0;0;1;land;false;plains;1\n2;0;0;2;land;true;forest;1\n3;0;0;3;land;false;hills;1\n4;0;0;4;sea;false;ocean;0\n",
        )
    }

    #[test]
    fn a_border_drawn_by_a_river_is_a_crossing_and_its_width_comes_from_the_index() {
        // A | B along a large river (index 9, drawn on B's edge pixels); B | C touched at one point
        let provinces = bmp24(&[
            &[A, A, B, B, C, C],
            &[A, A, B, B, C, C],
            &[A, A, B, B, C, C],
            &[A, A, B, B, SEA, SEA],
        ]);
        let rivers = bmp8(&[
            &[NO, NO, 9, NO, NO, NO],
            &[NO, NO, 9, NO, NO, NO],
            &[NO, NO, 9, NO, NO, NO],
            &[NO, NO, 9, 3, NO, NO],
        ]);
        let rules = RiverRules::default();
        let b = borders(&provinces, &rivers, &defs(), &rules).unwrap();
        let ab = &b[&(1, 2)];
        assert_eq!(
            (ab.pixels, ab.river_pixels, ab.river_index_max),
            (4, 4, Some(9))
        );
        assert_eq!(ab.river(&rules), Some("large"));
        let bc = &b[&(2, 3)];
        assert_eq!(bc.pixels, 3);
        assert_eq!(bc.river(&rules), None); // the index-3 pixel is not on the B|C border
        assert!(!b.contains_key(&(2, 4)), "sea is not a land border");
        // a small river: same border drawn with index 4
        let rivers = bmp8(&[
            &[NO, NO, 4, NO, NO, NO],
            &[NO, NO, 4, NO, NO, NO],
            &[NO, NO, NO, NO, NO, NO],
            &[NO, NO, NO, NO, NO, NO],
        ]);
        let b = borders(&provinces, &rivers, &defs(), &rules).unwrap();
        assert_eq!(b[&(1, 2)].river(&rules), Some("small"));
        let rivers = bmp8(&[
            &[NO, NO, 4, NO, NO, NO],
            &[NO, NO, NO, NO, NO, NO],
            &[NO, NO, NO, NO, NO, NO],
            &[NO, NO, NO, NO, NO, NO],
        ]);
        let b = borders(&provinces, &rivers, &defs(), &rules).unwrap();
        assert_eq!(b[&(1, 2)].river(&rules), Some("touches"));
    }

    #[test]
    fn river_defines_terrain_categories_adjacencies_and_forts_parse() {
        let rules = river_rules(
            "NDefines = { NMilitary = {\n\tRIVER_CROSSING_PENALTY = -0.3,  -- small\n\tRIVER_CROSSING_PENALTY_LARGE = -0.6,\n\tRIVER_SMALL_STOP_INDEX = 6,\n\tRIVER_LARGE_STOP_INDEX = 11,\n} }",
        );
        assert_eq!(rules, RiverRules::default());
        let cats = categories(&script::parse(
            "categories = { forest = { movement_cost = 1.5 combat_width = 60 combat_support_width = 30 units = { attack = -0.15 } supply_flow_penalty_factor = 0.08 } ocean = { is_water = yes } }",
        ));
        assert_eq!(cats["forest"].combat_width, Some(60.0));
        assert_eq!(cats["forest"].attack, Some(-0.15));
        assert!(cats["ocean"].is_water);
        let adj = parse_adjacencies(
            "From;To;Type;Through;start_x;start_y;stop_x;stop_y;adjacency_rule_name;Comment\n480;9431;sea;8621;-1;-1;-1;-1;;Netherlands outlier\n10;20;impassable;-1;-1;-1;-1;-1;;\n-1;-1;;-1;-1;-1;-1;-1;;\n",
        );
        assert_eq!(adj[&(480, 9431)][0].kind, "sea");
        assert_eq!(adj[&(480, 9431)][0].through, Some(8621));
        assert_eq!(adj[&(10, 20)][0].kind, "impassable");
        let forts = state_forts(&script::parse(
            "state = { id = 42 history = { buildings = { infrastructure = 4 6115 = { naval_base = 6 coastal_bunker = 3 } } 1939.1.1 = { buildings = { 11470 = { bunker = 2 } } } } }",
        ));
        assert!(forts.contains(&(
            6115,
            Fort {
                building: "coastal_bunker".into(),
                level: 3.0,
                from: None
            }
        )));
        assert!(forts.contains(&(
            11470,
            Fort {
                building: "bunker".into(),
                level: 2.0,
                from: Some("1939.1.1".into())
            }
        )));
    }

    /// The real map (set HOI4_GAME_DIR to a game or content folder): the Low Countries.
    #[test]
    fn low_countries_borders_on_the_real_map() {
        let Some(dir) = std::env::var_os("HOI4_GAME_DIR").map(PathBuf::from) else {
            return;
        };
        if !dir.join("map/rivers.bmp").exists() {
            return;
        }
        let t = map_terrain(&dir).unwrap();
        let river = |a, b| t.borders.get(&pair(a, b)).and_then(|e| e.river(&t.rivers));
        // Friesland/Gelderland 6241 | Dutch Brabant 11456: the Rhine/Waal (large)
        assert_eq!(river(6241, 11456), Some("large"));
        // inside Dutch Brabant 6500 | 11456: the Maas (small)
        assert_eq!(river(6500, 11456), Some("small"));
        // Dutch Brabant 480 | Belgium 6560: the land border, no river
        assert_eq!(river(480, 6560), None);
        assert!(t.borders.contains_key(&pair(480, 6560)));
        assert_eq!(t.terrain.get(&9498).map(String::as_str), Some("urban"));
        // Westwall bunkers (state 42, 1939 history)
        assert!(t.forts[&11470].iter().any(|f| f.building == "bunker"));
        assert_eq!(t.categories["forest"].combat_width, Some(60.0));
    }
}
