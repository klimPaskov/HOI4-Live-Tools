//! `hoi4_mios`: military-industrial organisations and the idea-based
//! designers they replace, for the own country. Pure read: no input, no lease,
//! no cursor movement, never pauses.
//!
//! What this build state allows, established on 26 October 2026 (Germany,
//! January 1938, game pid in the run's identity file):
//!
//! * CHECKED live: the organisation template database
//!   (`image+0x34d0cb0`, `COrganisationDatabase`, template array `+0x58`)
//!   holds 552 templates, and the own active ideas (politics status
//!   `country+0xd38`, idea array `+0x50`, idea key string `+0x18`) include
//!   `GER_daimler_benz`, the designer the player took; the other five are laws
//!   and spirits. So designers are the active mechanism here.
//! * NOT FOUND: any runtime `COrganisation` / `CPolicy` / `CTask` object. A
//!   vtable-pointer scan of the whole heap found none, and no pointer to the
//!   Daimler-Benz organisation template exists outside the template databases.
//!   Per-organisation state (size, funds, unlocked traits, assigned policy,
//!   design team, assigned production lines) therefore has no mapped source
//!   and is reported as unavailable instead of guessed. It is consistent with
//!   the game running without the Arms Against Tyranny organisation system
//!   (the designer ideas are the fallback), but that is an inference, not a
//!   read.
//!
//! The catalogue (what organisations and designers the install defines, for
//! which tags, with which equipment and trait counts) comes from the game's
//! own scripts and is complete regardless of the above.
use super::{
    mgmt,
    script::{self, Block, Entry, Val},
    ui::read,
};
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::Path,
    time::Instant,
};

const ORGANISATION_DB: u64 = 0x34d0cb0;
const DB_TEMPLATES: u64 = 0x58;
const POLITICS_TYPE: &str = "16CPoliticalStatus";
const ACTIVE_IDEAS: u64 = 0x50;
const IDEA_TYPE: &str = "5CIdea";
const IDEA_KEY: u64 = 0x18;
const IDEA_LIMIT: usize = 128;

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Org {
    pub key: String,
    pub tag: String,
    pub include: Option<String>,
    pub icon: String,
    pub equipment: Vec<String>,
    pub research_categories: Vec<String>,
    pub traits: usize,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Catalog {
    /// Organisation scripts with their `allowed` block, evaluated per tag in
    /// [`assemble`].
    pub orgs: Vec<(Org, Vec<Entry>)>,
    /// Designer idea key -> idea category (`tank_manufacturer`, ...).
    pub designers: HashMap<String, String>,
}

/// Three-valued trigger evaluation of the subset of triggers organisation
/// `allowed` blocks use; anything else is unknown.
pub(super) fn eval(block: &[Entry], tag: &str) -> Option<bool> {
    fn and(entries: &[Entry], tag: &str) -> Option<bool> {
        let mut unknown = false;
        for e in entries {
            match single(e, tag) {
                Some(false) => return Some(false),
                None => unknown = true,
                Some(true) => {}
            }
        }
        if unknown {
            None
        } else {
            Some(true)
        }
    }
    fn or(entries: &[Entry], tag: &str) -> Option<bool> {
        let mut unknown = false;
        for e in entries {
            match single(e, tag) {
                Some(true) => return Some(true),
                None => unknown = true,
                Some(false) => {}
            }
        }
        if unknown {
            None
        } else {
            Some(false)
        }
    }
    fn single(e: &Entry, tag: &str) -> Option<bool> {
        match (e.key.as_str(), &e.val) {
            ("always", Val::Scalar(v)) => Some(v == "yes"),
            ("tag" | "original_tag", Val::Scalar(v)) => Some(v == tag),
            ("AND", Val::Block(b)) => and(b, tag),
            ("OR", Val::Block(b)) => or(b, tag),
            // NOT is a NOR in game script: true only when nothing inside holds.
            ("NOT", Val::Block(b)) => or(b, tag).map(|v| !v),
            ("NAND", Val::Block(b)) => and(b, tag).map(|v| !v),
            _ => None,
        }
    }
    and(block, tag)
}

fn category_of(equipment: &[String]) -> &'static str {
    let any = |needles: &[&str]| {
        equipment
            .iter()
            .any(|e| needles.iter().any(|n| e.contains(n)))
    };
    if any(&[
        "ship",
        "submarine",
        "carrier",
        "cruiser",
        "battleship",
        "destroyer",
        "naval",
        "capital",
        "screen",
    ]) && !any(&["naval_bomber", "plane"])
    {
        "naval"
    } else if any(&[
        "plane",
        "airframe",
        "aircraft",
        "fighter",
        "bomber",
        "cas",
        "helicopter",
    ]) {
        "air"
    } else if equipment.is_empty() {
        "unknown"
    } else {
        "land"
    }
}

fn trait_count(body: &[Entry]) -> usize {
    body.iter()
        .filter(|e| {
            matches!(e.key.as_str(), "trait" | "add_trait") && matches!(e.val, Val::Block(_))
        })
        .count()
}

/// Organisation catalogue from the parsed scripts of one directory.
pub(super) fn orgs_from(entries: &[Entry]) -> Vec<(Org, Vec<Entry>)> {
    let bodies: HashMap<&str, &[Entry]> = entries
        .iter()
        .filter(|e| !e.key.starts_with("my_"))
        .filter_map(|e| match &e.val {
            Val::Block(b)
                if b.find_val("icon").is_some()
                    || b.find_val("include").is_some()
                    || b.find_val("equipment_type").is_some() =>
            {
                Some((e.key.as_str(), b.as_slice()))
            }
            _ => None,
        })
        .collect();
    let mut out = Vec::new();
    for (key, body) in &bodies {
        let include = body.text("include").map(str::to_string);
        let parent = include.as_deref().and_then(|i| bodies.get(i));
        let mut equipment = body.words("equipment_type");
        let mut research = body.words("research_categories");
        let mut traits = trait_count(body);
        if let Some(p) = parent {
            if equipment.is_empty() {
                equipment = p.words("equipment_type");
            }
            if research.is_empty() {
                research = p.words("research_categories");
            }
            traits += trait_count(p);
        }
        let tag = key
            .split('_')
            .next()
            .filter(|t| t.len() == 3 && t.chars().all(|c| c.is_ascii_uppercase()))
            .unwrap_or("generic")
            .to_string();
        let org = Org {
            key: (*key).to_string(),
            tag,
            include,
            icon: body
                .text("icon")
                .or_else(|| parent.and_then(|p| p.text("icon")))
                .unwrap_or("")
                .to_string(),
            equipment,
            research_categories: research,
            traits,
        };
        // `allowed` belongs to the organisation or, absent that, its template.
        let allowed: Vec<Entry> = body
            .find_val("allowed")
            .or_else(|| parent.and_then(|p| p.find_val("allowed")))
            .map(|v| v.block().to_vec())
            .unwrap_or_default();
        out.push((org, allowed));
    }
    out.sort_by(|a, b| a.0.key.cmp(&b.0.key));
    out
}

/// Designer ideas: idea categories flagged `designer = yes`.
pub(super) fn designers_from(entries: &[Entry]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for top in entries.iter().filter(|e| e.key == "ideas") {
        for group in top.val.block() {
            // `ideas = { country = { ... } tank_manufacturer = { ... } }`
            let Val::Block(categories) = &group.val else {
                continue;
            };
            for category in categories {
                let Val::Block(ideas) = &category.val else {
                    continue;
                };
                if ideas.text("designer") != Some("yes") {
                    continue;
                }
                for idea in ideas.iter().filter(|i| matches!(i.val, Val::Block(_))) {
                    out.insert(idea.key.clone(), category.key.clone());
                }
            }
            // Flat form: `ideas = { tank_manufacturer = { designer = yes ... } }`.
            if let Val::Block(ideas) = &group.val {
                if ideas.text("designer") == Some("yes") {
                    for idea in ideas.iter().filter(|i| matches!(i.val, Val::Block(_))) {
                        out.insert(idea.key.clone(), group.key.clone());
                    }
                }
            }
        }
    }
    out
}

pub(super) fn load(dir: &Path) -> Catalog {
    let base = dir.join("common");
    let org_entries =
        script::parse_dir(&base.join("military_industrial_organization/organizations"));
    let idea_entries = script::parse_dir(&base.join("ideas"));
    Catalog {
        orgs: orgs_from(&org_entries),
        designers: designers_from(&idea_entries),
    }
}

pub(super) fn catalog(dir: &Path) -> std::sync::Arc<Catalog> {
    script::cached(dir, "mios", load)
}

/// Designer idea matching an organisation key (`GER_daimler_benz_organization`
/// -> `GER_daimler_benz`, `..._improved` variants share it).
pub(super) fn designer_for<'a>(
    org_key: &str,
    designers: &'a HashMap<String, String>,
) -> Option<&'a str> {
    let base = org_key
        .strip_suffix("_organization")
        .or_else(|| org_key.strip_suffix("_org"))
        .unwrap_or(org_key);
    designers
        .keys()
        .find(|k| k.as_str() == base)
        .map(String::as_str)
}

// ---------------------------------------------------------------- memory

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Live {
    pub template_count: usize,
    pub active_ideas: Vec<String>,
}

pub(super) fn read_live(r: &Reader<'_>, image: u64, politics: u64) -> Result<Live> {
    let db = mgmt::pointer(r, image + ORGANISATION_DB)?;
    let (_, _, template_count) = r.array_header(db + DB_TEMPLATES)?;
    mgmt::expect_type(r, politics, POLITICS_TYPE)?;
    let ideas = r.pointer_array(politics + ACTIVE_IDEAS)?;
    if ideas.len() > IDEA_LIMIT {
        return Err(Error::new("invalid_array", "Implausible active idea count"));
    }
    let mut keys = Vec::new();
    for idea in ideas {
        if !mgmt::is_pointer(idea) {
            return Err(Error::new("invalid_pointer", "Stale active idea"));
        }
        mgmt::expect_type(r, idea, IDEA_TYPE)?;
        keys.push(mgmt::text(r, idea + IDEA_KEY, 128)?.unwrap_or_default());
    }
    Ok(Live {
        template_count,
        active_ideas: keys,
    })
}

fn snapshot(r: &Reader<'_>, image: u64) -> Result<Live> {
    let own = mgmt::own(r, image)?;
    let politics = mgmt::pointer(r, own.country + game::layout(r).country_politics)?;
    read_live(r, image, politics)
}

// -------------------------------------------------------------- assembly

pub(super) fn assemble(live: &Live, catalog: &Catalog, tag: &str, include_all: bool) -> Value {
    let active_designers: Vec<&String> = live
        .active_ideas
        .iter()
        .filter(|k| catalog.designers.contains_key(*k))
        .collect();
    let designer_names: HashSet<&str> = catalog.designers.keys().map(String::as_str).collect();
    let mut entries = Vec::new();
    let mut shown = 0usize;
    for (org, allowed) in &catalog.orgs {
        if !include_all && org.key.starts_with("DEBUG_") {
            continue;
        }
        let verdict = eval(allowed, tag);
        let own_prefix = org.tag == tag;
        let relevant = own_prefix || verdict == Some(true);
        if !include_all && !(relevant && verdict != Some(false)) {
            continue;
        }
        shown += 1;
        let designer = designer_for(&org.key, &catalog.designers)
            .filter(|d| designer_names.contains(d))
            .map(str::to_string);
        entries.push(json!({
            "key": org.key,
            "tag": org.tag,
            "category": category_of(&org.equipment),
            "equipment": org.equipment,
            "research_categories": org.research_categories,
            "traits": org.traits,
            "icon": org.icon,
            "include": org.include,
            "allowed_for_tag": verdict,
            "designer_idea": designer,
        }));
    }
    json!({
        "mio_system": {
            "state_read": false,
            "reason": "No runtime organisation object is reachable on this build state; size, funds, unlocked traits, policy, design team and production-line assignment have no mapped source",
            "inference": "designer ideas are the active mechanism (organisation DLC system not in use)",
        },
        "designers": {
            "active": active_designers,
            "active_ideas": live.active_ideas,
            "known_designer_ideas": catalog.designers.len(),
            "validated": true,
        },
        "organisations": {
            "tag": tag,
            "templates_in_memory": live.template_count,
            "scripts_in_install": catalog.orgs.len(),
            "listed": shown,
            "include_all": include_all,
            "entries": entries,
            "allowed_for_tag_note": "true/false evaluates tag, original_tag, AND, OR, NOT, always; null means the block depends on DLC, ideology or flags that are not read",
            "validated": "catalogue from scripts; counts compared with memory",
        },
    })
}

pub(super) fn mios(process: &dyn Process, tag: Option<&str>, include_all: bool) -> Result<Value> {
    let started = Instant::now();
    let dir = mgmt::game_dir(process).ok();
    let cat = dir
        .as_deref()
        .map(catalog)
        .unwrap_or_else(|| std::sync::Arc::new(Catalog::default()));
    let (live, context, stamp, build_id) = read(process, |r, image| {
        let live = snapshot(r, image)?;
        Ok((
            live,
            mgmt::clock(r, image)?,
            mgmt::stamp(started, r),
            game::layout(r).build_id,
        ))
    })?;
    let own_tag = context["player"].as_str().unwrap_or("").to_string();
    let tag = tag.unwrap_or(&own_tag).to_string();
    let mut body = assemble(&live, &cat, &tag, include_all);
    let object = body.as_object_mut().expect("object");
    object.insert("source".into(), json!("native own MIO and designer read"));
    object.insert("build_id".into(), json!(build_id));
    object.insert("owned".into(), json!(true));
    object.insert("game_time".into(), context);
    object.insert("freshness".into(), stamp);
    object.insert("catalog_loaded".into(), json!(!cat.orgs.is_empty()));
    object.insert("observation_paused_game".into(), json!(false));
    object.insert("input_authorized".into(), json!(false));
    Ok(body)
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_mios","description":"Military-industrial organisations and designers for the own country, read-only (no input, no lease, never pauses). Reports the own active designer ideas (validated live: e.g. GER_daimler_benz), the organisation catalogue of the install for the own tag (key, category land/naval/air, equipment, research categories, trait count, icon, matching designer idea; allowed_for_tag is true/false/null when it depends on DLC or flags) and the memory template count. Per-organisation state (size, funds, unlocked traits, policy, design team, production lines) is NOT mapped on this build state and is reported unavailable (mio_system.state_read:false). tag overrides the catalogue's tag; include_all lists every organisation script.","inputSchema":{"type":"object","properties":{"tag":{"type":"string","pattern":"^[A-Z]{3}$"},"include_all":{"type":"boolean"}},"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::super::mgmt::fake::Mem;
    use super::*;

    const ORGS: &str = r#"
    @x = 9
    generic_tank_organization = {
        icon = GFX_idea_generic_tank
        allowed = { NOT = { OR = { original_tag = GER original_tag = ENG } } }
        equipment_type = { mio_cat_eq_all_light_tank mio_cat_eq_all_medium_tank }
        research_categories = { mio_cat_tech_light_armor }
        initial_trait = { name = a }
        trait = { token = t1 }
        trait = { token = t2 }
    }
    GER_daimler_benz_organization = {
        include = generic_tank_organization
        icon = GFX_idea_GER_daimler_benz
        allowed = { OR = { original_tag = GER AND = { original_tag = BEL has_dlc = "Gotterdammerung" } } }
        add_trait = { token = g1 }
    }
    GER_messerschmitt_organization = {
        icon = GFX_idea_GER_messerschmitt
        allowed = { original_tag = GER }
        equipment_type = { small_plane_airframe medium_plane_fighter_airframe }
    }
    GER_blohm_organization = {
        allowed = { original_tag = GER }
        equipment_type = { mio_cat_eq_all_battleship submarine }
    }
    BEL_only_organization = {
        allowed = { AND = { original_tag = BEL has_dlc = "X" } }
        equipment_type = { artillery_equipment }
    }
    "#;
    const IDEAS: &str = r#"
    ideas = {
        country = { sour_loser = { cost = 0 } }
        tank_manufacturer = {
            designer = yes
            GER_daimler_benz = { cost = 150 }
            GER_daimler_benz_improved = { cost = 150 }
        }
        political_advisor = { GER_fascist_advisor = { cost = 150 } }
    }
    "#;

    fn cat() -> Catalog {
        Catalog {
            orgs: orgs_from(&script::parse(ORGS)),
            designers: designers_from(&script::parse(IDEAS)),
        }
    }

    #[test]
    fn trigger_evaluation_is_three_valued() {
        let top = script::parse("a = { NOT = { OR = { original_tag = GER original_tag = ENG } } }");
        let body = top[0].val.block();
        assert_eq!(eval(body, "GER"), Some(false));
        assert_eq!(eval(body, "FRA"), Some(true));
        let dlc = script::parse(
            "a = { OR = { original_tag = GER AND = { original_tag = BEL has_dlc = X } } }",
        );
        let body = dlc[0].val.block();
        assert_eq!(eval(body, "GER"), Some(true));
        assert_eq!(eval(body, "BEL"), None);
        assert_eq!(eval(body, "FRA"), Some(false));
        assert_eq!(eval(&[], "GER"), Some(true));
    }

    #[test]
    fn catalogue_resolves_includes_categories_and_designers() {
        let c = cat();
        let find = |k: &str| c.orgs.iter().find(|(o, _)| o.key == k).unwrap().0.clone();
        let d = find("GER_daimler_benz_organization");
        assert_eq!(d.tag, "GER");
        assert_eq!(
            d.equipment,
            ["mio_cat_eq_all_light_tank", "mio_cat_eq_all_medium_tank"]
        );
        assert_eq!(d.traits, 3);
        assert_eq!(category_of(&d.equipment), "land");
        assert_eq!(
            category_of(&find("GER_messerschmitt_organization").equipment),
            "air"
        );
        assert_eq!(
            category_of(&find("GER_blohm_organization").equipment),
            "naval"
        );
        assert_eq!(find("generic_tank_organization").tag, "generic");
        assert_eq!(c.designers.len(), 2);
        assert_eq!(
            designer_for("GER_daimler_benz_organization", &c.designers),
            Some("GER_daimler_benz")
        );
        assert_eq!(
            designer_for("GER_messerschmitt_organization", &c.designers),
            None
        );
    }

    fn live() -> Live {
        Live {
            template_count: 552,
            active_ideas: vec![
                "sour_loser".into(),
                "GER_daimler_benz".into(),
                "free_trade".into(),
            ],
        }
    }

    #[test]
    fn assembly_filters_to_the_own_tag_and_reports_designers() {
        let v = assemble(&live(), &cat(), "GER", false);
        assert_eq!(v["mio_system"]["state_read"], json!(false));
        assert_eq!(v["designers"]["active"], json!(["GER_daimler_benz"]));
        let keys: Vec<&str> = v["organisations"]["entries"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["key"].as_str().unwrap())
            .collect();
        assert_eq!(
            keys,
            [
                "GER_blohm_organization",
                "GER_daimler_benz_organization",
                "GER_messerschmitt_organization"
            ]
        );
        assert_eq!(v["organisations"]["templates_in_memory"], json!(552));
        assert_eq!(v["organisations"]["scripts_in_install"], json!(5));
        // The generic tank organisation excludes GER, so it is not listed.
        let all = assemble(&live(), &cat(), "FRA", true);
        assert_eq!(all["organisations"]["entries"].as_array().unwrap().len(), 5);
    }

    #[test]
    fn unknown_dlc_dependence_stays_null_not_true() {
        let v = assemble(&live(), &cat(), "BEL", false);
        let entries = v["organisations"]["entries"].as_array().unwrap();
        let bel = entries
            .iter()
            .find(|e| e["key"] == "BEL_only_organization")
            .unwrap();
        assert_eq!(bel["allowed_for_tag"], Value::Null);
    }

    fn object(m: &Mem, at: u64, class: &str, slot: u64) {
        let vt = 0x41000 + slot * 0x100;
        m.q(at, vt);
        m.q(vt - 8, vt + 0x40);
        m.q(vt + 0x40 + 8, vt + 0x80);
        m.patch(vt + 0x80, format!("{class}\0").as_bytes());
    }

    #[test]
    fn live_read_returns_template_count_and_active_idea_keys() {
        let m = Mem::default();
        m.alloc(0x40000, 0x5000);
        let image = 0x1_0000_0000u64;
        // Image global holding the database pointer.
        m.alloc(image + ORGANISATION_DB, 0x10);
        m.q(image + ORGANISATION_DB, 0x200000);
        m.alloc(0x200000, 0x100);
        m.q(0x200000 + DB_TEMPLATES, 0x210000);
        m.i(0x200000 + DB_TEMPLATES + 8, 711);
        m.i(0x200000 + DB_TEMPLATES + 12, 552);
        m.alloc(0x210000, 0x40);
        m.alloc(0x300000, 0x100); // politics
        object(&m, 0x300000, POLITICS_TYPE, 0);
        m.alloc(0x310000, 0x40);
        m.alloc(0x320000, 0x80);
        m.alloc(0x330000, 0x80);
        object(&m, 0x320000, IDEA_TYPE, 1);
        m.sso(0x320000 + IDEA_KEY, "GER_daimler_benz");
        object(&m, 0x330000, IDEA_TYPE, 1);
        m.sso(0x330000 + IDEA_KEY, "free_trade");
        m.q(0x300000 + ACTIVE_IDEAS, 0x310000);
        m.i(0x300000 + ACTIVE_IDEAS + 8, 4);
        m.i(0x300000 + ACTIVE_IDEAS + 12, 2);
        m.q(0x310000, 0x320000);
        m.q(0x310008, 0x330000);
        let r = Reader::new(&m);
        let live = read_live(&r, image, 0x300000).unwrap();
        assert_eq!(live.template_count, 552);
        assert_eq!(live.active_ideas, ["GER_daimler_benz", "free_trade"]);
        // A stale idea pointer is an error, never a skipped entry.
        m.q(0x310008, 0);
        let r = Reader::new(&m);
        assert_eq!(
            read_live(&r, image, 0x300000).unwrap_err().code,
            "invalid_pointer"
        );
    }

    #[test]
    fn tool_schema_validates_tag_shape() {
        let t = tool();
        assert_eq!(t["name"], json!("hoi4_mios"));
        assert_eq!(
            t["inputSchema"]["properties"]["tag"]["pattern"],
            json!("^[A-Z]{3}$")
        );
    }
}
