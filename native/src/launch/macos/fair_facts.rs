//! The game content the FAIR guards judge by (`crate::fair_guard::Facts`), read once per game
//! content folder (install plus enabled mods) from the script files, as the plan lint reads them
//! (scripts/hoi4facts/lint_fair.py `sub_facts`, `unit_class`): land sub-units with their width
//! and class, submarines above Submarine II (a third tier of a hull line, or a hull without a
//! submarine parent newer than the second tier), AIP and coating modules, mine-laying modules,
//! and the technologies and special projects that unlock any of them.
use super::script::{self, Block, Entry, Val};
use crate::fair_guard::{Facts, Unit};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::Arc,
};

/// The facts of one content folder (cached for the server's life).
pub(super) fn facts(dir: &Path) -> Arc<Facts> {
    script::cached(dir, "fair_facts", build)
}

fn top_level(dir: &Path) -> Vec<Entry> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "txt"))
        .collect();
    files.sort();
    files
        .iter()
        .filter_map(|f| std::fs::read(f).ok())
        .flat_map(|b| script::parse(&String::from_utf8_lossy(&b)))
        .collect()
}

fn words(val: Option<&Val>) -> Vec<String> {
    match val {
        Some(Val::Scalar(s)) => vec![s.clone()],
        Some(Val::Block(b)) => b
            .iter()
            .filter(|e| e.op.is_empty() && !e.key.is_empty())
            .map(|e| e.key.clone())
            .collect(),
        None => Vec::new(),
    }
}

/// `key = { name = { ... } ... }` blocks of the entries, flattened to (name, body).
fn named<'a>(entries: &'a [Entry], key: &'a str) -> impl Iterator<Item = (&'a str, &'a [Entry])> {
    entries
        .iter()
        .filter(move |e| e.key == key)
        .flat_map(|e| e.val.block().iter())
        .filter(|e| !e.key.is_empty() && matches!(e.val, Val::Block(_)))
        .map(|e| (e.key.as_str(), e.val.block()))
}

fn build(dir: &Path) -> Facts {
    let units_dir = dir.join("common/units");
    let units = units_of(&top_level(&units_dir));
    let equipment = script::parse_dir(&units_dir.join("equipment"));
    let technologies = script::parse_dir(&dir.join("common/technologies"));
    let projects = script::parse_dir(&dir.join("common/special_projects/projects"));
    from_entries(units, &equipment, &technologies, &projects)
}

fn units_of(entries: &[Entry]) -> HashMap<String, Unit> {
    let mut out = HashMap::new();
    for (name, body) in named(entries, "sub_units") {
        if body.text("group") == Some("support") {
            continue;
        }
        let types: HashSet<String> = words(body.find_val("type")).into_iter().collect();
        let categories: HashSet<String> = words(body.find_val("categories")).into_iter().collect();
        out.insert(
            name.to_string(),
            Unit {
                width: body.number("combat_width").unwrap_or(0.0),
                leg: (types.len() == 1 && types.contains("infantry"))
                    || categories.contains("category_cavalry"),
                armour: types.contains("armor"),
                para: categories.contains("category_paratroopers"),
            },
        );
    }
    out
}

fn special_module(text: &str) -> bool {
    let t = text.to_ascii_lowercase();
    t.contains("aip") || t.contains("anechoic") || t.contains("coating")
}

fn from_entries(
    units: HashMap<String, Unit>,
    equipment: &[Entry],
    technologies: &[Entry],
    projects: &[Entry],
) -> Facts {
    // equipment -> (year, parent, archetype or parent link, own types, is archetype)
    struct Raw {
        year: Option<i64>,
        parent: Option<String>,
        archetype: Option<String>,
        types: Vec<String>,
        is_archetype: bool,
    }
    let mut raw: HashMap<String, Raw> = HashMap::new();
    for (name, body) in named(equipment, "equipments") {
        raw.insert(
            name.to_string(),
            Raw {
                year: body.number("year").map(|y| y as i64),
                parent: body.text("parent").map(str::to_string),
                archetype: body.text("archetype").map(str::to_string),
                types: words(body.find_val("type")),
                is_archetype: body.text("is_archetype") == Some("yes"),
            },
        );
    }
    let is_sub = |key: &str| -> bool {
        let mut seen = HashSet::new();
        let mut cur = Some(key.to_string());
        while let Some(k) = cur {
            let Some(r) = raw.get(&k) else { break };
            if !seen.insert(k.clone()) {
                break;
            }
            if r.types.iter().any(|t| t == "submarine") {
                return true;
            }
            cur = r.archetype.clone().or_else(|| r.parent.clone());
        }
        false
    };
    let subs: HashSet<&str> = raw
        .iter()
        .filter(|(k, r)| !r.is_archetype && is_sub(k))
        .map(|(k, _)| k.as_str())
        .collect();
    let tier = |key: &str| -> u32 {
        let mut t = 1;
        let mut seen = HashSet::from([key.to_string()]);
        let mut cur = raw.get(key).and_then(|r| r.parent.clone());
        while let Some(p) = cur {
            if !subs.contains(p.as_str()) || !seen.insert(p.clone()) {
                break;
            }
            t += 1;
            cur = raw.get(&p).and_then(|r| r.parent.clone());
        }
        t
    };
    let tiers: HashMap<&str, u32> = subs.iter().map(|k| (*k, tier(k))).collect();
    let cut = tiers
        .iter()
        .filter(|(_, t)| **t == 2)
        .filter_map(|(k, _)| raw[*k].year)
        .min();
    let mut sub_equipment = HashMap::new();
    for (k, t) in &tiers {
        let year = raw[*k].year;
        if *t >= 3 {
            sub_equipment.insert(k.to_string(), format!("{k} is a tier-{t} submarine"));
        } else if let (1, Some(cut), Some(year)) = (*t, cut, year) {
            if year > cut {
                sub_equipment.insert(
                    k.to_string(),
                    format!("{k} ({year}) is newer than Submarine II ({cut})"),
                );
            }
        }
    }
    let mut sub_modules = HashMap::new();
    let mut mine_modules = HashSet::new();
    for (name, body) in named(equipment, "equipment_modules") {
        let category = body.text("category").unwrap_or("");
        if special_module(name) || special_module(category) {
            sub_modules.insert(
                name.to_string(),
                format!("{name} is a special submarine module"),
            );
        }
        let plants = body
            .find_val("add_stats")
            .and_then(|s| s.block().number("mines_planting"))
            .is_some_and(|n| n != 0.0);
        if name.contains("mine_layer") || category.contains("mine_layer") || plants {
            mine_modules.insert(name.to_string());
        }
    }
    let unlocks: HashMap<&str, &String> = sub_equipment
        .iter()
        .chain(sub_modules.iter())
        .map(|(k, v)| (k.as_str(), v))
        .collect();
    fn found(body: &[Entry], unlocks: &HashMap<&str, &String>, depth: u32, out: &mut Vec<String>) {
        if depth > 8 {
            return;
        }
        for e in body {
            if let Val::Block(inner) = &e.val {
                if e.key == "enable_equipments" || e.key == "enable_equipment_modules" {
                    for w in words(Some(&e.val)) {
                        if let Some(why) = unlocks.get(w.as_str()) {
                            out.push((*why).clone());
                        }
                    }
                } else {
                    found(inner, unlocks, depth + 1, out);
                }
            }
        }
    }
    let unlocking = |pairs: Vec<(&str, &[Entry])>| -> HashMap<String, String> {
        let mut out = HashMap::new();
        for (name, body) in pairs {
            let mut hits = Vec::new();
            found(body, &unlocks, 0, &mut hits);
            if !hits.is_empty() {
                hits.sort();
                hits.dedup();
                out.insert(name.to_string(), hits.join("; "));
            }
        }
        out
    };
    let techs = unlocking(named(technologies, "technologies").collect());
    let project_list: Vec<(&str, &[Entry])> = projects
        .iter()
        .filter(|e| !e.key.is_empty() && matches!(e.val, Val::Block(_)))
        .map(|e| (e.key.as_str(), e.val.block()))
        .collect();
    let projects = unlocking(project_list);
    Facts {
        units,
        sub_equipment,
        sub_modules,
        mine_modules,
        techs,
        projects,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNITS: &str = r#"
    sub_units = {
        infantry = { type = { infantry } group = infantry categories = { category_front_line category_all_infantry } combat_width = 2 }
        cavalry = { type = { infantry } categories = { category_cavalry } combat_width = 2 }
        motorized = { type = { motorized } combat_width = 2 }
        light_armor = { type = { armor } combat_width = 2 }
        light_sp_artillery_brigade = { type = { armor } combat_width = 3 }
        paratrooper = { type = { infantry } categories = { category_paratroopers } combat_width = 2 }
        engineer = { group = support type = { infantry } combat_width = 0 }
    }"#;

    const EQUIPMENT: &str = r#"
    equipments = {
        ship_hull_submarine = { year = 1922 is_archetype = yes type = submarine }
        ship_hull_submarine_1 = { year = 1922 archetype = ship_hull_submarine }
        ship_hull_submarine_2 = { year = 1936 archetype = ship_hull_submarine parent = ship_hull_submarine_1 }
        ship_hull_submarine_3 = { year = 1940 archetype = ship_hull_submarine parent = ship_hull_submarine_2 }
        ship_hull_cruiser_submarine = { year = 1940 archetype = ship_hull_submarine }
        ship_hull_light = { year = 1922 is_archetype = yes type = { destroyer } }
        ship_hull_light_3 = { year = 1940 archetype = ship_hull_light }
    }
    equipment_modules = {
        ship_anechoic_tiles = { category = ship_coating }
        ship_aip_engine = { category = ship_engine }
        ship_mine_layer_sub = { category = ship_mine_layer_sub }
        ship_depth_charge = { category = ship_anti_submarine add_stats = { sub_attack = 2 } }
        ship_mines_stats = { category = misc add_stats = { mines_planting = 0.5 } }
    }"#;

    #[test]
    fn units_submarines_modules_and_their_unlocks() {
        let units = units_of(&script::parse(UNITS));
        assert!(units["infantry"].leg && !units["infantry"].armour);
        assert!(units["cavalry"].leg);
        assert!(!units["motorized"].leg && !units["motorized"].armour);
        assert!(units["light_sp_artillery_brigade"].armour);
        assert!(units["paratrooper"].para && units["paratrooper"].leg);
        assert!(
            !units.contains_key("engineer"),
            "support companies are left out"
        );
        assert_eq!(units["light_sp_artillery_brigade"].width, 3.0);
        let techs = script::parse(
            "technologies = { improved_submarine = { enable_equipments = { ship_hull_submarine_3 } }
               basic_submarine = { enable_equipments = { ship_hull_submarine_2 } }
               sub_tiles = { path = { leads_to_tech = x } enable_equipment_modules = { ship_anechoic_tiles } } }",
        );
        let projects = script::parse(
            "sp_aip = { project_output = { enable_equipment_modules = { ship_aip_engine } } } sp_radar = { x = 1 }",
        );
        let f = from_entries(units, &script::parse(EQUIPMENT), &techs, &projects);
        assert!(f.sub_equipment.contains_key("ship_hull_submarine_3"));
        assert!(f.sub_equipment["ship_hull_cruiser_submarine"].contains("newer than Submarine II"));
        assert!(!f.sub_equipment.contains_key("ship_hull_submarine_2"));
        assert!(!f.sub_equipment.contains_key("ship_hull_submarine_1"));
        assert!(!f.sub_equipment.contains_key("ship_hull_light_3"));
        assert!(f.sub_modules.contains_key("ship_anechoic_tiles"));
        assert!(f.sub_modules.contains_key("ship_aip_engine"));
        assert!(!f.sub_modules.contains_key("ship_depth_charge"));
        assert!(f.mine_modules.contains("ship_mine_layer_sub"));
        assert!(f.mine_modules.contains("ship_mines_stats"));
        assert!(f.techs.contains_key("improved_submarine"));
        assert!(f.techs.contains_key("sub_tiles"));
        assert!(!f.techs.contains_key("basic_submarine"));
        assert!(f.projects.contains_key("sp_aip"));
        assert!(!f.projects.contains_key("sp_radar"));
    }

    /// The installed game's content, when this machine has it (read-only files; skipped otherwise).
    #[test]
    fn installed_content_when_present() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let dir = Path::new(&home)
            .join("Library/Application Support/Steam/steamapps/common/Hearts of Iron IV");
        if !dir.join("common/units").is_dir() {
            return;
        }
        let f = build(&dir);
        assert!(f.units.get("infantry").is_some_and(|u| u.leg));
        assert!(f.units.get("light_armor").is_some_and(|u| u.armour));
        assert!(f.sub_equipment.contains_key("ship_hull_submarine_3"));
        assert!(!f.sub_equipment.contains_key("ship_hull_submarine_2"));
        assert!(f.techs.contains_key("improved_submarine"));
        assert!(f.sub_modules.keys().any(|m| m.contains("aip")));
    }
}
