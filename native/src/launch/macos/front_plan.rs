//! War planning intelligence for an automated player: `hoi4_front_plan`,
//! `hoi4_encirclement_candidates` and `hoi4_war_goal_targets`. All three are
//! pure reads (no input, no lease, no cursor, no pause) over a [`StateGraph`].
//!
//! Visible information only: the owner and controller of every state exactly as
//! the political map shows them (same validated reads as `hoi4_states` and
//! `hoi4_wars`) and the player's own division positions (`hoi4_armies`). No
//! hidden enemy unit data is read or implied; "enemy-held" means the state's
//! controller is one of the tags the caller names.
//!
//! Map facts come from the install files, once per install:
//! - land adjacency: `map/provinces.bmp` pixel contact between land provinces
//!   (`wars::land_edges`), minus the `impassable` rows of `map/adjacencies.csv`
//!   (Himalayas), folded to states through `history/states`. Sea contact is NOT
//!   adjacency (it only marks a state `coastal`); the strait/sea rows of
//!   `adjacencies.csv` are naval rules and are ignored. A state flagged
//!   `impassable = yes` is a wall: nobody walks through it.
//! - terrain: `map/definition.csv` terrain per province, `common/terrain`
//!   movement cost and attack malus, averaged over the state's provinces.
//! - state facts: category, manpower, infrastructure, factories, docks,
//!   resources, victory points, cores, province bunkers (Maginot line) from
//!   `history/states`; capitals from `history/countries`. Vanilla files only;
//!   mods are ignored (stated in the output).
//!
//! The numbers (enter cost, division need, value) are documented planning
//! heuristics, not game formulas. They rank options; they do not predict
//! battles. Rivers, fortified supply, air cover and weather are not modelled.
use super::{mapdata, states, wars};
use crate::{
    error::{Error, Result},
    platform::Process,
};
use serde_json::{json, Map, Value};
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::Instant,
};

type Adjacency = BTreeMap<i32, BTreeSet<i32>>;

// ---------------------------------------------------------------------------
// Clausewitz script parsing (just enough for state, terrain and country files).

#[derive(Debug, Clone)]
enum Node {
    Val(String),
    Block(Vec<(String, Node)>),
}

fn tokenize(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let (mut quote, mut comment) = (false, false);
    for c in text.chars() {
        if comment {
            if c == '\n' {
                comment = false;
            }
            continue;
        }
        if quote {
            if c == '"' {
                quote = false;
                out.push(std::mem::take(&mut cur));
            } else {
                cur.push(c);
            }
            continue;
        }
        match c {
            '#' | '"' | '{' | '}' | '=' => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
                match c {
                    '#' => comment = true,
                    '"' => quote = true,
                    _ => out.push(c.to_string()),
                }
            }
            c if c.is_whitespace() || c == '\u{feff}' => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn parse_block(toks: &[String], pos: &mut usize) -> Vec<(String, Node)> {
    let mut out = Vec::new();
    while *pos < toks.len() {
        let t = toks[*pos].clone();
        *pos += 1;
        match t.as_str() {
            "}" => return out,
            "=" => continue,
            "{" => {
                let b = parse_block(toks, pos);
                out.push((String::new(), Node::Block(b)));
                continue;
            }
            _ => {}
        }
        if toks.get(*pos).map(String::as_str) == Some("=") {
            *pos += 1;
            match toks.get(*pos).map(String::as_str) {
                Some("{") => {
                    *pos += 1;
                    let b = parse_block(toks, pos);
                    out.push((t, Node::Block(b)));
                }
                Some(v) => {
                    out.push((t, Node::Val(v.to_string())));
                    *pos += 1;
                }
                None => {}
            }
        } else {
            out.push((String::new(), Node::Val(t)));
        }
    }
    out
}

fn parse_script(text: &str) -> Vec<(String, Node)> {
    let toks = tokenize(text);
    let mut pos = 0;
    parse_block(&toks, &mut pos)
}

fn find<'a>(block: &'a [(String, Node)], key: &str) -> Option<&'a Node> {
    block.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}

fn find_block<'a>(block: &'a [(String, Node)], key: &str) -> Option<&'a [(String, Node)]> {
    match find(block, key) {
        Some(Node::Block(b)) => Some(b),
        _ => None,
    }
}

fn find_val<'a>(block: &'a [(String, Node)], key: &str) -> Option<&'a str> {
    match find(block, key) {
        Some(Node::Val(v)) => Some(v),
        _ => None,
    }
}

fn find_num(block: &[(String, Node)], key: &str) -> Option<f64> {
    find_val(block, key).and_then(|v| v.parse().ok())
}

fn bare_numbers(block: &[(String, Node)]) -> Vec<i64> {
    block
        .iter()
        .filter_map(|(k, v)| match v {
            Node::Val(v) if k.is_empty() => v.parse().ok(),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Static map facts.

/// Per-state facts from `history/states` plus aggregated province terrain.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct StateInfo {
    pub id: i32,
    pub category: String,
    pub manpower: i64,
    pub impassable: bool,
    pub provinces: Vec<i32>,
    pub infra: i32,
    pub civ: i32,
    pub mil: i32,
    pub dock: i32,
    /// mean province bunker level over all provinces of the state
    pub bunker: f64,
    /// (province, points)
    pub vp: Vec<(i32, i32)>,
    pub resources: BTreeMap<String, i32>,
    pub cores: Vec<String>,
    pub initial_owner: Option<String>,
    /// most common province terrain
    pub terrain: String,
    /// mean terrain movement cost (plains 1.0, mountain 2.0)
    pub move_cost: f64,
    /// mean attacker attack malus of the terrain (mountain 0.25..0.3)
    pub malus: f64,
    /// share of urban provinces
    pub urban: f64,
    pub coastal: bool,
}

impl StateInfo {
    pub(super) fn blank(id: i32) -> Self {
        Self {
            id,
            category: "rural".into(),
            manpower: 0,
            impassable: false,
            provinces: Vec::new(),
            infra: 3,
            civ: 0,
            mil: 0,
            dock: 0,
            bunker: 0.0,
            vp: Vec::new(),
            resources: BTreeMap::new(),
            cores: Vec::new(),
            initial_owner: None,
            terrain: "plains".into(),
            move_cost: 1.0,
            malus: 0.0,
            urban: 0.0,
            coastal: false,
        }
    }
    pub(super) fn vp_sum(&self) -> i32 {
        self.vp.iter().map(|(_, v)| v).sum()
    }
}

fn parse_state(text: &str) -> Option<StateInfo> {
    let root = parse_script(text);
    let state = find_block(&root, "state")?;
    let id = find_num(state, "id")? as i32;
    let mut s = StateInfo::blank(id);
    s.category = find_val(state, "state_category")
        .unwrap_or("rural")
        .to_string();
    s.manpower = find_num(state, "manpower").unwrap_or(0.0) as i64;
    s.impassable = find_val(state, "impassable") == Some("yes");
    if let Some(p) = find_block(state, "provinces") {
        s.provinces = bare_numbers(p).into_iter().map(|v| v as i32).collect();
    }
    if let Some(r) = find_block(state, "resources") {
        for (k, v) in r {
            if let Node::Val(v) = v {
                if let Ok(n) = v.parse::<f64>() {
                    s.resources.insert(k.clone(), n as i32);
                }
            }
        }
    }
    if let Some(h) = find_block(state, "history") {
        s.initial_owner = find_val(h, "owner").map(str::to_string);
        for (k, v) in h {
            match (k.as_str(), v) {
                ("add_core_of", Node::Val(t)) => s.cores.push(t.clone()),
                ("victory_points", Node::Block(b)) => {
                    let n = bare_numbers(b);
                    if n.len() >= 2 {
                        s.vp.push((n[0] as i32, n[1] as i32));
                    }
                }
                _ => {}
            }
        }
        if let Some(b) = find_block(h, "buildings") {
            s.infra = find_num(b, "infrastructure").unwrap_or(0.0) as i32;
            s.civ = find_num(b, "industrial_complex").unwrap_or(0.0) as i32;
            s.mil = find_num(b, "arms_factory").unwrap_or(0.0) as i32;
            s.dock = find_num(b, "dockyard").unwrap_or(0.0) as i32;
            let levels: f64 = b
                .iter()
                .filter(|(k, _)| k.parse::<i32>().is_ok())
                .filter_map(|(_, v)| match v {
                    Node::Block(pb) => find_num(pb, "bunker"),
                    _ => None,
                })
                .sum();
            s.bunker = levels / s.provinces.len().max(1) as f64;
        }
    }
    Some(s)
}

/// Terrain table: category -> (movement cost, attacker attack malus).
fn parse_terrain_table(text: &str) -> HashMap<String, (f64, f64)> {
    let root = parse_script(text);
    let mut out = HashMap::new();
    if let Some(cats) = find_block(&root, "categories") {
        for (name, node) in cats {
            if let Node::Block(b) = node {
                let cost = find_num(b, "movement_cost").unwrap_or(1.0);
                let attack = find_block(b, "units")
                    .and_then(|u| find_num(u, "attack"))
                    .unwrap_or(0.0);
                out.insert(name.clone(), (cost, (-attack).max(0.0)));
            }
        }
    }
    out
}

/// One `definition.csv` row.
#[derive(Clone, Debug)]
struct ProvinceRow {
    id: i32,
    rgb: u32,
    kind: String,
    terrain: String,
}

fn parse_definition_rows(text: &str) -> Vec<ProvinceRow> {
    let mut out = Vec::new();
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
        out.push(ProvinceRow {
            id,
            rgb: (r << 16) | (g << 8) | b,
            kind: f[4].trim().to_string(),
            terrain: f[6].trim().to_string(),
        });
    }
    out
}

/// `adjacencies.csv` rows of type `impassable`, as unordered province pairs.
fn parse_impassable(text: &str) -> HashSet<(i32, i32)> {
    let mut out = HashSet::new();
    for line in text.lines() {
        let f: Vec<&str> = line.split(';').collect();
        if f.len() < 3 || f[2].trim() != "impassable" {
            continue;
        }
        if let (Ok(a), Ok(b)) = (f[0].trim().parse::<i32>(), f[1].trim().parse::<i32>()) {
            out.insert((a, b));
            out.insert((b, a));
        }
    }
    out
}

/// Everything read from the install, independent of the running campaign.
#[derive(Default)]
pub(super) struct StaticMap {
    pub adj: Adjacency,
    pub info: BTreeMap<i32, StateInfo>,
    pub capitals: BTreeMap<String, i32>,
    pub centers: HashMap<i32, (f64, f64)>,
    pub names: HashMap<i32, String>,
}

/// Fold province contact into state adjacency and mark coastal states.
/// `edges` are all contact pairs (any province kinds).
fn fold_map(
    edges: &BTreeSet<(i32, i32)>,
    rows: &[ProvinceRow],
    impassable: &HashSet<(i32, i32)>,
    state_of: &HashMap<i32, i32>,
) -> (Adjacency, BTreeSet<i32>) {
    let kind: HashMap<i32, &str> = rows.iter().map(|r| (r.id, r.kind.as_str())).collect();
    let mut adj: Adjacency = BTreeMap::new();
    let mut coastal = BTreeSet::new();
    for &(a, b) in edges {
        let (ka, kb) = (kind.get(&a).copied(), kind.get(&b).copied());
        match (ka, kb) {
            (Some("land"), Some("land")) => {
                if impassable.contains(&(a, b)) {
                    continue;
                }
                if let (Some(&sa), Some(&sb)) = (state_of.get(&a), state_of.get(&b)) {
                    if sa != sb {
                        adj.entry(sa).or_default().insert(sb);
                        adj.entry(sb).or_default().insert(sa);
                    }
                }
            }
            (Some("land"), Some("sea")) => {
                if let Some(&s) = state_of.get(&a) {
                    coastal.insert(s);
                }
            }
            (Some("sea"), Some("land")) => {
                if let Some(&s) = state_of.get(&b) {
                    coastal.insert(s);
                }
            }
            _ => {}
        }
    }
    (adj, coastal)
}

fn apply_terrain(
    info: &mut StateInfo,
    terrain_of: &HashMap<i32, String>,
    table: &HashMap<String, (f64, f64)>,
) {
    let (mut cost, mut malus, mut urban, mut n) = (0.0, 0.0, 0.0, 0.0);
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for p in &info.provinces {
        let Some(t) = terrain_of.get(p) else { continue };
        let (c, m) = table.get(t).copied().unwrap_or((1.0, 0.0));
        cost += c;
        malus += m;
        if t == "urban" {
            urban += 1.0;
        }
        n += 1.0;
        *counts.entry(t.as_str()).or_default() += 1;
    }
    if n > 0.0 {
        info.move_cost = cost / n;
        info.malus = malus / n;
        info.urban = urban / n;
        // The most common terrain; ties go to the alphabetically first name.
        if let Some((t, _)) = counts.iter().max_by_key(|(t, c)| (**c, Reverse(**t))) {
            info.terrain = (*t).to_string();
        }
    }
}

fn read_text(dir: &Path, rel: &str) -> Result<String> {
    std::fs::read_to_string(dir.join(rel))
        .map_err(|_| Error::new("map_data_unavailable", format!("{rel} unreadable")))
}

fn read_capitals(dir: &Path) -> BTreeMap<String, i32> {
    let mut out = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir.join("history/countries")) else {
        return out;
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let tag: String = name.chars().take(3).collect();
        if tag.len() != 3 || !tag.bytes().all(|b| b.is_ascii_alphanumeric()) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        if let Some(c) = find_num(&parse_script(&text), "capital") {
            out.insert(tag, c as i32);
        }
    }
    out
}

fn build_static(dir: &Path) -> Result<StaticMap> {
    let rows = parse_definition_rows(&read_text(dir, "map/definition.csv")?);
    // Every province is marked "land" so `land_edges` reports all contact.
    let all: HashMap<u32, (i32, bool)> = rows.iter().map(|r| (r.rgb, (r.id, true))).collect();
    let bmp = std::fs::read(dir.join("map/provinces.bmp"))
        .map_err(|_| Error::new("map_data_unavailable", "provinces.bmp unreadable"))?;
    let edges = wars::land_edges(&bmp, &all)?;
    let impassable = std::fs::read_to_string(dir.join("map/adjacencies.csv"))
        .map(|t| parse_impassable(&t))
        .unwrap_or_default();
    let terrain_of: HashMap<i32, String> = rows.iter().map(|r| (r.id, r.terrain.clone())).collect();
    let table = parse_terrain_table(&read_text(dir, "common/terrain/00_terrain.txt")?);
    let mut info = BTreeMap::new();
    let mut state_of = HashMap::new();
    let entries = std::fs::read_dir(dir.join("history/states"))
        .map_err(|_| Error::new("map_data_unavailable", "State history unreadable"))?;
    for e in entries.flatten() {
        let Ok(text) = std::fs::read_to_string(e.path()) else {
            continue;
        };
        if let Some(mut s) = parse_state(&text) {
            for p in &s.provinces {
                state_of.entry(*p).or_insert(s.id);
            }
            apply_terrain(&mut s, &terrain_of, &table);
            info.insert(s.id, s);
        }
    }
    let (adj, coastal) = fold_map(&edges, &rows, &impassable, &state_of);
    for id in coastal {
        if let Some(s) = info.get_mut(&id) {
            s.coastal = true;
        }
    }
    let centers = mapdata::state_centers(dir)
        .map(|c| c.as_ref().clone())
        .unwrap_or_default();
    let names = states::names(dir)
        .map(|n| n.into_iter().collect())
        .unwrap_or_default();
    Ok(StaticMap {
        adj,
        info,
        capitals: read_capitals(dir),
        centers,
        names,
    })
}

fn static_map(dir: &Path) -> Result<Arc<StaticMap>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Arc<StaticMap>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(found) = cache.lock().unwrap().get(dir) {
        return Ok(found.clone());
    }
    let built = Arc::new(build_static(dir)?);
    cache
        .lock()
        .unwrap()
        .insert(dir.to_path_buf(), built.clone());
    Ok(built)
}

// ---------------------------------------------------------------------------
// The state graph the pure logic works on.

/// Who holds a state, as the player's own map shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    Own,
    Enemy,
    /// controlled by neither the player nor a named enemy (neutral, ally)
    Third,
}

pub(super) struct StateGraph {
    pub map: Arc<StaticMap>,
    pub player: String,
    pub enemies: BTreeSet<String>,
    pub owner: BTreeMap<i32, String>,
    pub controller: BTreeMap<i32, String>,
    /// own divisions standing in each state
    pub divisions: BTreeMap<i32, usize>,
}

/// Divisions a strike needs against a plain state; the rest scale from it.
const BASE_ATTACK: f64 = 5.0;

fn milli(c: f64) -> i64 {
    (c * 1000.0).round() as i64
}

fn r1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}

impl StateGraph {
    fn info(&self, id: i32) -> Option<&StateInfo> {
        self.map.info.get(&id)
    }
    fn name(&self, id: i32) -> String {
        self.map
            .names
            .get(&id)
            .cloned()
            .unwrap_or_else(|| format!("State {id}"))
    }
    fn passable(&self, id: i32) -> bool {
        self.info(id).is_none_or(|i| !i.impassable)
    }
    fn side(&self, id: i32) -> Option<Side> {
        let c = self.controller.get(&id)?;
        Some(if *c == self.player {
            Side::Own
        } else if self.enemies.contains(c) {
            Side::Enemy
        } else {
            Side::Third
        })
    }
    fn is_side(&self, id: i32, side: Side) -> bool {
        self.passable(id) && self.side(id) == Some(side)
    }
    /// Passable land neighbours.
    fn neighbours(&self, id: i32) -> Vec<i32> {
        self.map
            .adj
            .get(&id)
            .map(|s| s.iter().copied().filter(|n| self.passable(*n)).collect())
            .unwrap_or_default()
    }
    fn states_of(&self, side: Side) -> Vec<i32> {
        self.controller
            .keys()
            .copied()
            .filter(|id| self.is_side(*id, side))
            .collect()
    }
    fn vp(&self, id: i32) -> i32 {
        self.info(id).map_or(0, StateInfo::vp_sum)
    }
    /// Planning cost for an attacker to take the state (1.0 = flat open plain).
    fn enter_cost(&self, id: i32) -> f64 {
        let Some(i) = self.info(id) else { return 1.5 };
        let terrain = 1.0 + (i.move_cost - 1.0) + 3.0 * i.malus;
        let infra = 1.0 + f64::from(5 - i.infra.clamp(0, 5)) * 0.06;
        let size = ((i.provinces.len().max(1) as f64).sqrt() / 2.5).clamp(0.6, 1.8);
        (terrain + 0.8 * i.urban + 0.25 * i.bunker) * infra * size
    }
    /// Divisions a breakthrough into the state wants.
    fn attack_need(&self, id: i32) -> usize {
        let Some(i) = self.info(id) else { return 6 };
        let n = BASE_ATTACK * (1.0 + 2.0 * i.malus + 0.5 * i.urban + 0.1 * i.bunker);
        (n.ceil() as usize).clamp(4, 16)
    }
    /// Divisions a holding line wants in one border state.
    fn defend_need(&self, id: i32) -> usize {
        let Some(i) = self.info(id) else { return 2 };
        let base = if i.malus >= 0.15 || i.urban >= 0.3 {
            2
        } else {
            3
        };
        base + usize::from(i.vp_sum() >= 10)
    }
    /// Relative worth of holding the state (points, industry, manpower, resources).
    fn value(&self, id: i32) -> f64 {
        let Some(i) = self.info(id) else { return 0.0 };
        let res: f64 = i
            .resources
            .iter()
            .map(|(k, v)| {
                let w = match k.as_str() {
                    "oil" | "rubber" | "tungsten" | "aluminium" => 0.15,
                    "chromium" => 0.2,
                    "steel" => 0.08,
                    _ => 0.0,
                };
                w * f64::from(*v)
            })
            .sum();
        f64::from(i.vp_sum()) * 2.0
            + f64::from(i.civ)
            + f64::from(i.mil) * 1.5
            + f64::from(i.dock) * 0.5
            + i.manpower as f64 / 250_000.0
            + res
    }
    fn capital_of(&self, tag: &str) -> Option<i32> {
        self.map.capitals.get(tag).copied()
    }
    fn capitals(&self) -> BTreeSet<i32> {
        self.enemies
            .iter()
            .filter_map(|t| self.capital_of(t))
            .collect()
    }
    /// Separation of two states: centre distance, else graph hops.
    fn separation(&self, a: i32, b: i32) -> f64 {
        if let (Some(p), Some(q)) = (self.map.centers.get(&a), self.map.centers.get(&b)) {
            return ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
        }
        let hops = self.hops(&[a], &|_| true, 40);
        hops.get(&b).map_or(99.0, |h| f64::from(*h)) * 100.0
    }
    /// Breadth-first hop counts from `sources` through states passing `allow`.
    fn hops(&self, sources: &[i32], allow: &dyn Fn(i32) -> bool, max: u32) -> HashMap<i32, u32> {
        let mut dist = HashMap::new();
        let mut queue = VecDeque::new();
        for s in sources {
            if allow(*s) && dist.insert(*s, 0).is_none() {
                queue.push_back(*s);
            }
        }
        while let Some(u) = queue.pop_front() {
            let d = dist[&u];
            if d >= max {
                continue;
            }
            for v in self.neighbours(u) {
                if allow(v) && !dist.contains_key(&v) {
                    dist.insert(v, d + 1);
                    queue.push_back(v);
                }
            }
        }
        dist
    }
    /// Cheapest attack routes from `sources` through states passing `allow`.
    fn dijkstra(
        &self,
        sources: &[i32],
        allow: &dyn Fn(i32) -> bool,
    ) -> (HashMap<i32, i64>, HashMap<i32, i32>) {
        let mut dist: HashMap<i32, i64> = HashMap::new();
        let mut parent = HashMap::new();
        let mut heap = BinaryHeap::new();
        for s in sources {
            if allow(*s) {
                let c = milli(self.enter_cost(*s));
                if dist.get(s).is_none_or(|d| c < *d) {
                    dist.insert(*s, c);
                    heap.push(Reverse((c, *s)));
                }
            }
        }
        while let Some(Reverse((d, u))) = heap.pop() {
            if dist.get(&u).is_some_and(|x| *x < d) {
                continue;
            }
            for v in self.neighbours(u) {
                if !allow(v) {
                    continue;
                }
                let nd = d + milli(self.enter_cost(v));
                if dist.get(&v).is_none_or(|x| nd < *x) {
                    dist.insert(v, nd);
                    parent.insert(v, u);
                    heap.push(Reverse((nd, v)));
                }
            }
        }
        (dist, parent)
    }
    fn brief(&self, id: i32) -> Value {
        let i = self.info(id);
        json!({
            "state_id": id,
            "name": self.name(id),
            "terrain": i.map(|i| i.terrain.clone()),
            "category": i.map(|i| i.category.clone()),
            "owner": self.owner.get(&id),
            "controller": self.controller.get(&id),
            "victory_points": i.map(StateInfo::vp_sum),
            "civ": i.map(|i| i.civ),
            "mil": i.map(|i| i.mil),
            "enter_cost": r1(self.enter_cost(id)),
        })
    }
    fn own_divisions_within(&self, border: &[i32], depth: u32) -> usize {
        let own = |s: i32| self.is_side(s, Side::Own);
        self.hops(border, &own, depth)
            .keys()
            .map(|s| self.divisions.get(s).copied().unwrap_or(0))
            .sum()
    }
}

fn path_to(parent: &HashMap<i32, i32>, target: i32) -> Vec<i32> {
    let mut path = vec![target];
    let mut cur = target;
    while let Some(&p) = parent.get(&cur) {
        path.push(p);
        cur = p;
    }
    path.reverse();
    path
}

// ---------------------------------------------------------------------------
// Own armies, for concrete tool-call suggestions.

#[derive(Clone, Debug)]
pub(super) struct ArmyRef {
    pub name: String,
    pub main_state: Option<i32>,
    pub divisions: usize,
}

/// Armies from the `hoi4_armies` view.
pub(super) fn armies_from_view(view: &Value) -> Vec<ArmyRef> {
    view["armies"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|a| {
                    Some(ArmyRef {
                        name: a["army"].as_str()?.to_string(),
                        main_state: a["main_state"].as_i64().map(|v| v as i32),
                        divisions: a["division_count"].as_u64().unwrap_or(0) as usize,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn army_order(a: &(ArmyRef, u32), b: &(ArmyRef, u32)) -> std::cmp::Ordering {
    (a.1, Reverse(a.0.divisions), &a.0.name).cmp(&(b.1, Reverse(b.0.divisions), &b.0.name))
}

/// Nearest armies (by hops over own states) to a set of own states.
fn nearest_armies(
    g: &StateGraph,
    armies: &[ArmyRef],
    near: &[i32],
    taken: &BTreeSet<String>,
) -> Vec<(ArmyRef, u32)> {
    let own = |s: i32| g.is_side(s, Side::Own);
    let dist = g.hops(near, &own, 20);
    let mut out: Vec<(ArmyRef, u32)> = armies
        .iter()
        .filter(|a| !taken.contains(&a.name))
        .filter_map(|a| Some((a.clone(), *dist.get(&a.main_state?)?)))
        .collect();
    out.sort_by(army_order);
    out
}

fn call(tool: &str, args: Value) -> Value {
    json!({"tool": tool, "args": args})
}

/// The attack call sequence for one army: front line on the entry border,
/// offensive line into the objective state, then execute.
fn attack_calls(army: &str, front_state: i32, objective: i32) -> Vec<Value> {
    vec![
        call(
            "hoi4_army_front",
            json!({"army": army, "state_id": front_state}),
        ),
        call(
            "hoi4_army_offensive",
            json!({"army": army, "state_id": objective}),
        ),
        call("hoi4_army_plan", json!({"army": army, "execute": true})),
    ]
}

fn hold_calls(army: &str, front_state: i32) -> Vec<Value> {
    vec![
        call(
            "hoi4_army_front",
            json!({"army": army, "state_id": front_state}),
        ),
        call("hoi4_army_plan", json!({"army": army, "execute": true})),
    ]
}

const NEEDS: [&str; 2] = ["request_id", "lease_token"];

// ---------------------------------------------------------------------------
// hoi4_front_plan

struct UnionFind(HashMap<i32, i32>);
impl UnionFind {
    fn find(&mut self, x: i32) -> i32 {
        let p = *self.0.entry(x).or_insert(x);
        if p == x {
            return x;
        }
        let root = self.find(p);
        self.0.insert(x, root);
        root
    }
    fn union(&mut self, a: i32, b: i32) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.0.insert(ra.max(rb), ra.min(rb));
        }
    }
}

/// Enemy states worth marching on: hostile capitals, victory-point cities and
/// the highest-value states.
fn attack_targets(g: &StateGraph) -> BTreeSet<i32> {
    let mut targets = BTreeSet::new();
    let hostile = g.states_of(Side::Enemy);
    for c in g.capitals() {
        if g.is_side(c, Side::Enemy) {
            targets.insert(c);
        }
    }
    for s in &hostile {
        if g.vp(*s) >= 10 {
            targets.insert(*s);
        }
    }
    let mut by_value: Vec<(i64, i32)> = hostile.iter().map(|s| (-milli(g.value(*s)), *s)).collect();
    by_value.sort();
    targets.extend(by_value.iter().take(8).map(|(_, s)| *s));
    targets
}

struct Axis {
    entry: i32,
    target: i32,
    path: Vec<i32>,
    cost: f64,
    captured: f64,
    score: f64,
    passes: Vec<i32>,
}

fn axes_for_front(
    g: &StateGraph,
    enemy_border: &[i32],
    targets: &BTreeSet<i32>,
    limit: usize,
) -> Vec<Axis> {
    let hostile = |s: i32| g.is_side(s, Side::Enemy);
    let (dist, parent) = g.dijkstra(enemy_border, &hostile);
    let capitals = g.capitals();
    let mut axes: Vec<Axis> = targets
        .iter()
        .filter_map(|t| {
            let d = *dist.get(t)?;
            let path = path_to(&parent, *t);
            let cost = d as f64 / 1000.0;
            let captured: f64 = path.iter().map(|s| g.value(*s)).sum();
            let bonus = if capitals.contains(t) { 40.0 } else { 0.0 };
            let passes = path[..path.len() - 1]
                .iter()
                .copied()
                .filter(|s| targets.contains(s))
                .collect();
            Some(Axis {
                entry: path[0],
                target: *t,
                score: (captured + bonus) / cost.max(0.5),
                cost,
                captured,
                path,
                passes,
            })
        })
        .collect();
    axes.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.target.cmp(&b.target))
    });
    let mut kept: Vec<Axis> = Vec::new();
    for axis in axes {
        // A target already on a better axis's route is reported there.
        if kept.iter().any(|k| k.path.contains(&axis.target)) {
            continue;
        }
        if kept.iter().filter(|k| k.entry == axis.entry).count() >= 2 {
            continue;
        }
        kept.push(axis);
        if kept.len() >= limit {
            break;
        }
    }
    kept
}

fn strike_need(g: &StateGraph, path: &[i32]) -> usize {
    path.iter()
        .take(2)
        .map(|s| g.attack_need(*s))
        .max()
        .unwrap_or(BASE_ATTACK as usize)
}

fn axis_json(g: &StateGraph, rank: usize, a: &Axis, own_jump: &[i32]) -> Value {
    let capital = g.capitals().contains(&a.target);
    json!({
        "rank": rank,
        "entry_state": a.entry,
        "entry_name": g.name(a.entry),
        "jump_off_own_states": own_jump,
        "target_state": a.target,
        "target_name": g.name(a.target),
        "target_is_capital": capital,
        "route": a.path.iter().map(|s| g.brief(*s)).collect::<Vec<_>>(),
        "route_states": a.path.len(),
        "route_cost": r1(a.cost),
        "captured_value": r1(a.captured),
        "also_captures_targets": a.passes,
        "score": r1(a.score),
        "divisions_needed": strike_need(g, &a.path),
    })
}

struct Front {
    id: String,
    enemy: String,
    own_border: Vec<i32>,
    enemy_border: Vec<i32>,
    axes: Vec<Axis>,
    best: f64,
}

/// Contiguous borders with each named enemy, with their attack axes.
fn find_fronts(g: &StateGraph, max_axes: usize) -> (Vec<Front>, Vec<String>) {
    let targets = attack_targets(g);
    let mut fronts: Vec<Front> = Vec::new();
    let mut no_border: Vec<String> = Vec::new();
    for tag in &g.enemies {
        let mut pairs: Vec<(i32, i32)> = Vec::new();
        for a in g.states_of(Side::Own) {
            for b in g.neighbours(a) {
                if g.is_side(b, Side::Enemy) && g.controller.get(&b) == Some(tag) {
                    pairs.push((a, b));
                }
            }
        }
        if pairs.is_empty() {
            no_border.push(tag.clone());
            continue;
        }
        // Contiguous border: pairs sharing an own or an enemy state form one front.
        let mut uf = UnionFind(HashMap::new());
        for (a, b) in &pairs {
            uf.union(*a, -*b);
        }
        let mut groups: BTreeMap<i32, (BTreeSet<i32>, BTreeSet<i32>)> = BTreeMap::new();
        for (a, b) in &pairs {
            let root = uf.find(*a);
            let e = groups.entry(root).or_default();
            e.0.insert(*a);
            e.1.insert(*b);
        }
        for (n, (own, enemy)) in groups.into_values().enumerate() {
            let enemy_border: Vec<i32> = enemy.into_iter().collect();
            let axes = axes_for_front(g, &enemy_border, &targets, max_axes);
            let best = axes.first().map_or(0.0, |a| a.score);
            fronts.push(Front {
                id: format!("{tag}-{}", n + 1),
                enemy: tag.clone(),
                own_border: own.into_iter().collect(),
                enemy_border,
                axes,
                best,
            });
        }
    }
    (fronts, no_border)
}

/// Build the per-front analysis. `armies` may be empty (no call suggestions).
pub(super) fn front_plan(g: &StateGraph, armies: &[ArmyRef], max_axes: usize) -> Value {
    let started = Instant::now();
    let (fronts, no_border) = find_fronts(g, max_axes);
    let own_capital = g.capital_of(&g.player);
    let own_all = |s: i32| g.is_side(s, Side::Own);
    let cap_hops = own_capital
        .map(|c| g.hops(&[c], &own_all, 60))
        .unwrap_or_default();

    // The two best-scoring fronts attack when their best axis is at least half
    // the global best; every other front holds.
    let global_best = fronts.iter().map(|f| f.best).fold(0.0, f64::max);
    let mut order: Vec<usize> = (0..fronts.len()).collect();
    order.sort_by(|a, b| {
        fronts[*b]
            .best
            .partial_cmp(&fronts[*a].best)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(b))
    });
    let attackers: BTreeSet<usize> = order
        .iter()
        .copied()
        .take(2)
        .filter(|i| fronts[*i].best > 0.0 && fronts[*i].best >= 0.5 * global_best)
        .collect();

    // Each army serves the front whose border it stands closest to.
    let mut army_front: HashMap<String, (usize, u32)> = HashMap::new();
    for a in armies {
        let Some(s) = a.main_state else { continue };
        let dist = g.hops(&[s], &own_all, 25);
        let best = fronts
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                let d = f.own_border.iter().filter_map(|b| dist.get(b)).min()?;
                Some((*d, usize::from(!attackers.contains(&i)), i))
            })
            .min();
        if let Some((d, _, i)) = best {
            army_front.insert(a.name.clone(), (i, d));
        }
    }

    let mut out_fronts = Vec::new();
    let mut hold_lines = Vec::new();
    let mut total_missing = 0usize;
    for (i, f) in fronts.iter().enumerate() {
        let attack = attackers.contains(&i);
        let have_border: usize = f
            .own_border
            .iter()
            .map(|s| g.divisions.get(s).copied().unwrap_or(0))
            .sum();
        let have_near = g.own_divisions_within(&f.own_border, 2);
        // Holding line: border states nearest the own capital first.
        let mut line: Vec<i32> = f.own_border.clone();
        line.sort_by_key(|s| {
            (
                cap_hops.get(s).copied().unwrap_or(99),
                Reverse(g.vp(*s)),
                *s,
            )
        });
        line.truncate(8);
        let hold_need: usize = line.iter().map(|s| g.defend_need(*s)).sum();
        // Second line: own states on the cheapest enemy route toward the capital.
        let mut second_line = Vec::new();
        if let Some(cap) = own_capital {
            if g.is_side(cap, Side::Own) {
                let (dist, parent) = g.dijkstra(&f.own_border, &own_all);
                if dist.contains_key(&cap) {
                    second_line = path_to(&parent, cap)
                        .into_iter()
                        .filter(|s| !f.own_border.contains(s))
                        .take(3)
                        .collect();
                }
            }
        }
        let main_axis = f.axes.first();
        let strike = if attack {
            main_axis.map_or(0, |a| strike_need(g, &a.path))
        } else {
            0
        };
        let required = if attack {
            strike + hold_need.div_ceil(2)
        } else {
            hold_need
        };
        let missing = required.saturating_sub(have_near);
        total_missing += missing;
        let mut assigned: Vec<(ArmyRef, u32)> = armies
            .iter()
            .filter(|a| army_front.get(&a.name).is_some_and(|(fi, _)| *fi == i))
            .map(|a| (a.clone(), army_front[&a.name].1))
            .collect();
        assigned.sort_by(army_order);

        let mut calls: Vec<Value> = Vec::new();
        let mut taken = BTreeSet::new();
        if attack {
            if let Some(axis) = main_axis {
                let mut got = 0;
                for (a, _) in &assigned {
                    if got >= strike && !calls.is_empty() {
                        break;
                    }
                    calls.extend(attack_calls(&a.name, axis.entry, axis.path[0]));
                    got += a.divisions;
                    taken.insert(a.name.clone());
                }
            }
        }
        // Remaining armies assigned to the front hold the line.
        let hold_entry = f
            .enemy_border
            .iter()
            .copied()
            .find(|b| line.first().is_some_and(|l| g.neighbours(*l).contains(b)))
            .or_else(|| f.enemy_border.first().copied());
        if let Some(entry) = hold_entry {
            for (a, _) in &assigned {
                if !taken.contains(&a.name) {
                    calls.extend(hold_calls(&a.name, entry));
                }
            }
        }
        let axes: Vec<Value> = f
            .axes
            .iter()
            .enumerate()
            .map(|(r, a)| {
                let jump: Vec<i32> = g
                    .neighbours(a.entry)
                    .into_iter()
                    .filter(|s| f.own_border.contains(s))
                    .collect();
                axis_json(g, r + 1, a, &jump)
            })
            .collect();
        let line_json: Vec<Value> = line
            .iter()
            .map(|s| {
                let mut b = g.brief(*s);
                b["divisions_here"] = json!(g.divisions.get(s).copied().unwrap_or(0));
                b["divisions_wanted"] = json!(g.defend_need(*s));
                b["hops_to_own_capital"] = json!(cap_hops.get(s));
                b
            })
            .collect();
        let second_json: Vec<Value> = second_line.iter().map(|s| g.brief(*s)).collect();
        if !attack {
            hold_lines.push(json!({"front": f.id, "hold_line": line_json.clone(),
                "second_line": second_json.clone()}));
        }
        let mut shortfall = Vec::new();
        if missing > 0 {
            shortfall.push(call(
                "hoi4_train",
                json!({"note": format!("train or redeploy {missing} more divisions toward {}", g.name(f.own_border[0]))}),
            ));
        }
        out_fronts.push(json!({
            "front": f.id,
            "enemy": f.enemy,
            "role": if attack { "attack" } else { "hold" },
            "own_border_states": f.own_border.iter().map(|s| g.brief(*s)).collect::<Vec<_>>(),
            "enemy_border_states": f.enemy_border.iter().map(|s| g.brief(*s)).collect::<Vec<_>>(),
            "axes": axes,
            "defensive_line": line_json,
            "second_line": second_json,
            "divisions": {
                "in_border_states": have_border,
                "within_2_states": have_near,
                "required_for_strike": strike,
                "required_to_hold": hold_need,
                "required_total": required,
                "missing": missing,
            },
            "armies_here": assigned.iter().map(|(a, d)| json!({"army": a.name,
                "divisions": a.divisions, "hops_to_border": d})).collect::<Vec<_>>(),
            "suggested_calls": calls,
            "if_short": shortfall,
        }));
    }
    let main = order
        .first()
        .filter(|i| attackers.contains(i))
        .map(|i| fronts[*i].id.clone());
    json!({
        "player": g.player,
        "enemies": g.enemies,
        "fronts": out_fronts,
        "main_effort_front": main,
        "other_fronts_defensive_lines": hold_lines,
        "enemies_without_land_border": no_border,
        "divisions_missing_total": total_missing,
        "armies_known": !armies.is_empty(),
        "calls_also_need": NEEDS,
        "repeat_hint": "Each offensive targets the first enemy state of the route. After it falls, run hoi4_front_plan again and issue the new axis.",
        "method": "Cheapest route (terrain, infrastructure, state size, urban, bunkers) through enemy-controlled states from each contiguous border to the enemy capital and valuable states; score is captured value (points, industry, manpower, resources) per route cost. Division numbers are planning heuristics. Visible state ownership only; vanilla map files; rivers, supply and air cover are not modelled.",
        "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
    })
}

// ---------------------------------------------------------------------------
// hoi4_encirclement_candidates

/// Connected components of `set` with `removed` taken out.
fn components(g: &StateGraph, set: &BTreeSet<i32>, removed: &[i32]) -> Vec<BTreeSet<i32>> {
    let mut seen: HashSet<i32> = HashSet::new();
    let mut out = Vec::new();
    for &s in set {
        if removed.contains(&s) || seen.contains(&s) {
            continue;
        }
        let mut comp = BTreeSet::new();
        let mut stack = vec![s];
        seen.insert(s);
        while let Some(u) = stack.pop() {
            comp.insert(u);
            for v in g.neighbours(u) {
                if set.contains(&v) && !removed.contains(&v) && seen.insert(v) {
                    stack.push(v);
                }
            }
        }
        out.push(comp);
    }
    out
}

fn pocket_value(g: &StateGraph, states: impl Iterator<Item = i32>) -> f64 {
    let capitals = g.capitals();
    states
        .map(|s| g.value(s) + if capitals.contains(&s) { 40.0 } else { 0.0 })
        .sum()
}

fn combinations(items: &[i32], k: usize, f: &mut dyn FnMut(&[i32])) {
    fn rec(items: &[i32], k: usize, start: usize, cur: &mut Vec<i32>, f: &mut dyn FnMut(&[i32])) {
        if cur.len() == k {
            f(cur);
            return;
        }
        for i in start..items.len() {
            cur.push(items[i]);
            rec(items, k, i + 1, cur, f);
            cur.pop();
        }
    }
    rec(items, k, 0, &mut Vec::new(), f);
}

struct Pocket {
    inside: BTreeSet<i32>,
    neck: Vec<i32>,
    value: f64,
    effort: f64,
    score: f64,
    arms: Vec<(i32, i32)>,
    ready: bool,
}

/// The two own states farthest apart that touch the neck, preferring
/// different neck states: `(own state, neck state)` pairs.
fn pick_arms(g: &StateGraph, neck: &[i32]) -> Vec<(i32, i32)> {
    let mut cands: Vec<(i32, i32)> = Vec::new();
    for &n in neck {
        for p in g.neighbours(n) {
            if g.is_side(p, Side::Own) {
                cands.push((p, n));
            }
        }
    }
    type Pair = (i32, i32);
    let mut best: Option<(f64, Pair, Pair)> = None;
    for (i, a) in cands.iter().enumerate() {
        for b in cands.iter().skip(i + 1) {
            if a.0 == b.0 {
                continue;
            }
            let sep = g.separation(a.0, b.0) + if a.1 != b.1 { 1.0 } else { 0.0 };
            if best.as_ref().is_none_or(|x| sep > x.0) {
                best = Some((sep, *a, *b));
            }
        }
    }
    match best {
        Some((_, a, b)) => vec![a, b],
        None => cands.into_iter().take(1).collect(),
    }
}

fn find_pockets(g: &StateGraph, max_gap: usize) -> Vec<Pocket> {
    let hostile: BTreeSet<i32> = g.states_of(Side::Enemy).into_iter().collect();
    if hostile.is_empty() {
        return Vec::new();
    }
    // Neck candidates: enemy states within three hops of own territory, nearest first.
    let own: Vec<i32> = g.states_of(Side::Own);
    let reach = g.hops(
        &own,
        &|s| g.passable(s) && (hostile.contains(&s) || g.is_side(s, Side::Own)),
        3,
    );
    let mut cands: Vec<i32> = hostile
        .iter()
        .copied()
        .filter(|s| reach.contains_key(s))
        .collect();
    cands.sort_by_key(|s| (reach[s], Reverse(milli(g.value(*s))), *s));
    cands.truncate(60);

    // Adjacency restricted to the enemy block, for the cheap connectivity test.
    let local: HashMap<i32, Vec<i32>> = hostile
        .iter()
        .map(|s| {
            (
                *s,
                g.neighbours(*s)
                    .into_iter()
                    .filter(|n| hostile.contains(n))
                    .collect(),
            )
        })
        .collect();
    // Block id of every enemy state; only the blocks holding a neck state can split.
    let mut block_of: HashMap<i32, usize> = HashMap::new();
    let mut block_size: Vec<usize> = Vec::new();
    for c in components(g, &hostile, &[]) {
        for s in &c {
            block_of.insert(*s, block_size.len());
        }
        block_size.push(c.len());
    }
    let still_one_piece = |neck: &[i32]| -> bool {
        let blocks: BTreeSet<usize> = neck.iter().map(|n| block_of[n]).collect();
        blocks.into_iter().all(|b| {
            let inside = neck.iter().filter(|n| block_of[*n] == b).count();
            let Some(start) = hostile
                .iter()
                .find(|s| block_of[*s] == b && !neck.contains(s))
            else {
                return true;
            };
            let mut seen: HashSet<i32> = HashSet::from([*start]);
            let mut stack = vec![*start];
            while let Some(u) = stack.pop() {
                for v in &local[&u] {
                    if !neck.contains(v) && seen.insert(*v) {
                        stack.push(*v);
                    }
                }
            }
            seen.len() + inside == block_size[b]
        })
    };
    let mut seen: BTreeSet<BTreeSet<i32>> = BTreeSet::new();
    let mut found: Vec<Pocket> = Vec::new();
    let mut consider = |neck: &[i32]| {
        if !neck.is_empty() && still_one_piece(neck) {
            return;
        }
        // The bulk of the enemy (most states, then value) is not the pocket.
        let comps = components(g, &hostile, neck);
        let main = comps
            .iter()
            .enumerate()
            .max_by_key(|(i, c)| {
                (
                    c.len(),
                    milli(pocket_value(g, c.iter().copied())),
                    Reverse(*i),
                )
            })
            .map(|(i, _)| i);
        for (i, c) in comps.iter().enumerate() {
            if Some(i) == main {
                continue;
            }
            // Enemy-held contact of the pocket must be exactly the neck.
            let boundary: BTreeSet<i32> = c
                .iter()
                .flat_map(|s| g.neighbours(*s))
                .filter(|n| hostile.contains(n) && !c.contains(n))
                .collect();
            let want: BTreeSet<i32> = neck.iter().copied().collect();
            if boundary != want {
                continue;
            }
            // Own forces must be able to reach the pocket or its neck.
            let touches_own = c
                .iter()
                .chain(neck.iter())
                .any(|s| g.neighbours(*s).iter().any(|n| g.is_side(*n, Side::Own)));
            if !touches_own || !seen.insert(c.clone()) {
                continue;
            }
            let arms = pick_arms(g, neck);
            let distinct: BTreeSet<i32> = arms.iter().map(|a| a.0).collect();
            let ready = neck.is_empty() || distinct.len() >= 2;
            let value = pocket_value(g, c.iter().chain(neck.iter()).copied());
            let effort: f64 =
                neck.iter().map(|s| g.enter_cost(*s)).sum::<f64>() + 0.3 * c.len() as f64;
            let mut score = value / (1.0 + effort);
            if !ready {
                score *= 0.4;
            }
            found.push(Pocket {
                inside: c.clone(),
                neck: neck.to_vec(),
                value,
                effort,
                score,
                arms,
                ready,
            });
        }
    };
    consider(&[]);
    for k in 1..=max_gap.min(3) {
        combinations(&cands, k, &mut |neck| consider(neck));
    }
    found.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.neck.cmp(&b.neck))
    });
    // Nested pockets: keep the better-scoring one.
    let mut kept: Vec<Pocket> = Vec::new();
    for p in found {
        if kept
            .iter()
            .any(|k| k.inside.is_superset(&p.inside) || p.inside.is_superset(&k.inside))
        {
            continue;
        }
        kept.push(p);
    }
    kept
}

pub(super) fn encirclement_candidates(
    g: &StateGraph,
    armies: &[ArmyRef],
    max_gap: usize,
    limit: usize,
) -> Value {
    let started = Instant::now();
    let pockets = find_pockets(g, max_gap);
    let total = pockets.len();
    let rows: Vec<Value> = pockets
        .iter()
        .take(limit)
        .enumerate()
        .map(|(rank, p)| {
            let sum = |f: &dyn Fn(&StateInfo) -> i64| -> i64 {
                p.inside
                    .iter()
                    .chain(p.neck.iter())
                    .filter_map(|s| g.info(*s))
                    .map(f)
                    .sum()
            };
            let vp: i64 = sum(&|i| i64::from(i.vp_sum()));
            let coastal = p
                .inside
                .iter()
                .any(|s| g.info(*s).is_some_and(|i| i.coastal));
            let capital_inside = g.capitals().iter().any(|c| p.inside.contains(c));
            let third: BTreeSet<i32> = p
                .inside
                .iter()
                .chain(p.neck.iter())
                .flat_map(|s| g.neighbours(*s))
                .filter(|n| g.is_side(*n, Side::Third))
                .collect();
            let mut arms_json = Vec::new();
            let mut calls = Vec::new();
            let mut taken = BTreeSet::new();
            for (from, into) in &p.arms {
                let need = g.attack_need(*into);
                let have = g.own_divisions_within(&[*from], 1);
                let mut b = json!({
                    "from_own_state": g.brief(*from),
                    "into_neck_state": g.brief(*into),
                    "divisions_needed": need,
                    "divisions_available": have,
                    "divisions_missing": need.saturating_sub(have),
                });
                let near = nearest_armies(g, armies, &[*from], &taken);
                if let Some((a, hops)) = near.first() {
                    b["army_candidate"] = json!({"army": a.name, "hops": hops});
                    taken.insert(a.name.clone());
                    calls.extend(attack_calls(&a.name, *into, *into));
                }
                arms_json.push(b);
            }
            let already = p.neck.is_empty();
            let note = if already {
                "No enemy corridor remains: the pocket only needs to be attacked."
            } else if p.ready {
                "Attack both arms into the neck states; the pocket closes when they are own-controlled."
            } else {
                "Only one own state touches the neck: break through on a second side first."
            };
            json!({
                "rank": rank + 1,
                "pocket_states": p.inside.iter().map(|s| g.brief(*s)).collect::<Vec<_>>(),
                "neck_states": p.neck.iter().map(|s| g.brief(*s)).collect::<Vec<_>>(),
                "gap": p.neck.len(),
                "already_cut_off": already,
                "pincer_ready": p.ready,
                "pincer_arms": arms_json,
                "divisions_needed_per_arm": p.arms.iter().map(|a| g.attack_need(a.1)).max(),
                "value": r1(p.value),
                "victory_points": vp,
                "industry": {"civ": sum(&|i| i64::from(i.civ)), "mil": sum(&|i| i64::from(i.mil)),
                    "dock": sum(&|i| i64::from(i.dock))},
                "manpower": sum(&|i| i.manpower),
                "effort": r1(p.effort),
                "score": r1(p.score),
                "capital_inside": capital_inside,
                "sea_escape_possible": coastal,
                "third_party_neighbours": third,
                "suggested_calls": calls,
                "note": note,
            })
        })
        .collect();
    json!({
        "player": g.player,
        "enemies": g.enemies,
        "pockets": rows,
        "pockets_found": total,
        "max_gap": max_gap,
        "calls_also_need": NEEDS,
        "method": "A pocket is a group of enemy-controlled states that would be cut from the rest of the enemy by taking the neck states (at most max_gap of them); sea, impassable states and non-enemy neighbours (neutral, allied) count as walls. Arms are the two own states farthest apart that touch the neck. Score is pocket value (points, industry, manpower, resources, capital) per effort (terrain cost of the neck plus pocket size). Visible ownership only; vanilla map; sea evacuation and enemy reserves are not modelled.",
        "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
    })
}

// ---------------------------------------------------------------------------
// hoi4_war_goal_targets

pub(super) fn war_goal_targets(g: &StateGraph, limit: usize) -> Value {
    let started = Instant::now();
    let mine = |tag: &str| tag == g.player;
    // Enemy-owned states, wherever they are controlled.
    let owned: Vec<i32> = g
        .owner
        .iter()
        .filter(|(id, o)| g.enemies.contains(*o) && g.passable(**id) && g.info(**id).is_some())
        .map(|(id, _)| *id)
        .collect();
    let capitals = g.capitals();
    let own_all = |s: i32| g.is_side(s, Side::Own);
    let own_set = g.states_of(Side::Own);
    let reach = g.hops(&own_set, &|s| g.passable(s), 12);
    let own_core = |s: i32| g.info(s).is_some_and(|i| i.cores.iter().any(|c| mine(c)));
    let mut rows: Vec<(f64, i32)> = owned
        .iter()
        .map(|s| {
            let mut score = g.value(*s);
            if capitals.contains(s) {
                score += 40.0;
            }
            if own_core(*s) {
                score += 10.0;
            }
            (score, *s)
        })
        .collect();
    rows.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });
    let total: f64 = rows.iter().map(|r| r.0).sum();
    let mut cumulative = 0.0;
    let mut tier_of = HashMap::new();
    for (score, s) in &rows {
        let tier = if cumulative < 0.6 * total {
            "must_take"
        } else if cumulative < 0.85 * total {
            "worth_taking"
        } else {
            "optional"
        };
        tier_of.insert(*s, tier);
        cumulative += score;
    }
    let row_json = |(score, s): &(f64, i32)| {
        let i = g.info(*s);
        json!({
            "state_id": s,
            "name": g.name(*s),
            "tier": tier_of[s],
            "score": r1(*score),
            "victory_points": i.map(StateInfo::vp_sum),
            "industry": {"civ": i.map(|i| i.civ), "mil": i.map(|i| i.mil), "dock": i.map(|i| i.dock)},
            "manpower": i.map(|i| i.manpower),
            "resources": i.map(|i| i.resources.clone()),
            "category": i.map(|i| i.category.clone()),
            "capital": capitals.contains(s),
            "own_core": own_core(*s),
            "owner": g.owner.get(s),
            "controller": g.controller.get(s),
            "already_occupied_by_us": own_all(*s),
            "adjacent_to_own": g.neighbours(*s).iter().any(|n| own_all(*n)),
            "hops_from_own_territory": reach.get(s),
        })
    };
    let ranked: Vec<Value> = rows.iter().take(limit).map(row_json).collect();
    let resource_prizes: Vec<Value> = {
        let mut v: Vec<(String, i32, i32)> = owned
            .iter()
            .filter_map(|s| {
                let i = g.info(*s)?;
                i.resources
                    .iter()
                    .filter(|(k, v)| {
                        **v > 0
                            && matches!(
                                k.as_str(),
                                "oil" | "rubber" | "tungsten" | "aluminium" | "chromium" | "steel"
                            )
                    })
                    .map(|(k, v)| (k.clone(), *v, *s))
                    .max_by_key(|x| x.1)
            })
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.2.cmp(&b.2)));
        v.into_iter()
            .take(12)
            .map(|(k, n, s)| json!({"state_id": s, "name": g.name(s), "resource": k, "amount": n}))
            .collect()
    };
    // Contiguous blocks of enemy-owned states to ask for together.
    let owned_set: BTreeSet<i32> = owned.iter().copied().collect();
    let mut blocks: Vec<(f64, Value)> = components(g, &owned_set, &[])
        .into_iter()
        .map(|c| {
            let score: f64 = c.iter().map(|s| g.value(*s)).sum();
            let touching = c
                .iter()
                .any(|s| own_all(*s) || g.neighbours(*s).iter().any(|n| own_all(*n)));
            (
                score,
                json!({"states": c.iter().copied().collect::<Vec<_>>(),
                    "names": c.iter().take(6).map(|s| g.name(*s)).collect::<Vec<_>>(),
                    "value": r1(score), "touches_own_territory": touching}),
            )
        })
        .collect();
    blocks.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    json!({
        "player": g.player,
        "enemies": g.enemies,
        "enemy_states_total": owned.len(),
        "targets": ranked,
        "must_take_state_ids": rows.iter().filter(|r| tier_of[&r.1] == "must_take").map(|r| r.1).collect::<Vec<_>>(),
        "resource_prizes": resource_prizes,
        "contiguous_blocks": blocks.into_iter().take(8).map(|b| b.1).collect::<Vec<_>>(),
        "method": "Enemy-owned states ranked by points x2 + civilian + military x1.5 + dock x0.5 + manpower per 250k + strategic resources, +40 for a capital and +10 for a state our country has a core on. Tiers cover the best 60 percent, the next 25 percent and the rest of the total value. A state we already control is cheapest to demand. A read, not an action; the peace conference cost itself is not modelled.",
        "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
    })
}

// ---------------------------------------------------------------------------
// Tool entry points.

fn tag_ok(t: &str) -> bool {
    t.len() == 3
        && t.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// `enemy` (or `enemies`): one tag, a comma separated list or an array.
pub(super) fn parse_enemies(args: &Map<String, Value>) -> Result<BTreeSet<String>> {
    let v = args
        .get("enemy")
        .or_else(|| args.get("enemies"))
        .ok_or_else(|| {
            Error::new(
                "invalid_arguments",
                "enemy is required (a tag or a list of tags)",
            )
        })?;
    let raw: Vec<String> = match v {
        Value::String(s) => s
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect(),
        Value::Array(list) => list
            .iter()
            .map(|t| {
                t.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| Error::new("invalid_tag", "Tags are three upper-case letters"))
            })
            .collect::<Result<_>>()?,
        _ => {
            return Err(Error::new(
                "invalid_arguments",
                "enemy must be a tag or a list of tags",
            ))
        }
    };
    if raw.is_empty() || raw.len() > 8 {
        return Err(Error::new(
            "invalid_arguments",
            "Name one to eight enemy tags",
        ));
    }
    let mut out = BTreeSet::new();
    for t in raw {
        if !tag_ok(&t) {
            return Err(Error::new(
                "invalid_tag",
                "Tags are three upper-case letters",
            ));
        }
        out.insert(t);
    }
    Ok(out)
}

fn bounded(
    args: &Map<String, Value>,
    key: &str,
    default: usize,
    lo: u64,
    hi: u64,
) -> Result<usize> {
    match args.get(key) {
        None => Ok(default),
        Some(v) => v
            .as_u64()
            .filter(|n| (lo..=hi).contains(n))
            .map(|n| n as usize)
            .ok_or_else(|| Error::new("invalid_arguments", format!("{key} must be {lo}..{hi}"))),
    }
}

/// Live graph: install map facts plus the player's current state ownership.
fn live_graph(process: &dyn Process, dir: &Path, enemies: BTreeSet<String>) -> Result<StateGraph> {
    let map = static_map(dir)?;
    let snap = super::ui::read(process, wars::snapshot)?;
    if enemies.contains(&snap.player) {
        return Err(Error::new(
            "invalid_arguments",
            "The player's own tag cannot be the enemy",
        ));
    }
    Ok(StateGraph {
        map,
        player: snap.player,
        enemies,
        owner: snap
            .states
            .iter()
            .map(|s| (s.id, s.owner.clone()))
            .collect(),
        controller: snap
            .states
            .iter()
            .map(|s| (s.id, s.controller.clone()))
            .collect(),
        divisions: snap.divisions,
    })
}

fn finish(mut v: Value) -> Value {
    v["observation_paused_game"] = json!(false);
    v["input_authorized"] = json!(false);
    v["data"] =
        json!("vanilla install map files + visible state ownership; no hidden enemy information");
    v
}

fn armies_of(process: &dyn Process) -> Vec<ArmyRef> {
    super::armies::armies(process)
        .map(|v| armies_from_view(&v))
        .unwrap_or_default()
}

pub(super) fn front_plan_tool(
    process: &dyn Process,
    dir: &Path,
    args: &Map<String, Value>,
) -> Result<Value> {
    let enemies = parse_enemies(args)?;
    let max_axes = bounded(args, "max_axes", 3, 1, 6)?;
    let g = live_graph(process, dir, enemies)?;
    Ok(finish(front_plan(&g, &armies_of(process), max_axes)))
}

pub(super) fn encirclement_tool(
    process: &dyn Process,
    dir: &Path,
    args: &Map<String, Value>,
) -> Result<Value> {
    let enemies = parse_enemies(args)?;
    let max_gap = bounded(args, "max_gap", 2, 0, 3)?;
    let limit = bounded(args, "limit", 6, 1, 20)?;
    let g = live_graph(process, dir, enemies)?;
    Ok(finish(encirclement_candidates(
        &g,
        &armies_of(process),
        max_gap,
        limit,
    )))
}

pub(super) fn war_goal_tool(
    process: &dyn Process,
    dir: &Path,
    args: &Map<String, Value>,
) -> Result<Value> {
    let enemies = parse_enemies(args)?;
    let limit = bounded(args, "limit", 25, 1, 100)?;
    let g = live_graph(process, dir, enemies)?;
    Ok(finish(war_goal_targets(&g, limit)))
}

#[cfg(test)]
mod tests;
