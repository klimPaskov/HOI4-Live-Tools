//! `hoi4_research_extras`: the own technology bonuses that the research panel
//! only shows as tooltips: limited-use research bonuses ("free research
//! bonus" with uses left), limited-use cost reductions, idea research bonuses
//! and technology-sharing faction membership. Pure read: no input, no lease,
//! no cursor movement, never pauses.
//!
//! Layout (macOS Mach-O 089EA519, mapped live on 26 October 2026) below
//! `country+0xd08` (`CTechnologyStatus`, the object `research.rs` also uses):
//!
//! * `+0xe8` array of `CLimitedUseTechBonus` (`+0x8` i32 uses left, `+0x10`
//!   name string, `+0x28` instance id, `+0x48` array of category objects whose
//!   first 24 bytes are the category-key string, `+0x68` bonus raw/100000).
//!   CHECKED live against the focus scripts: Germany's six entries name the
//!   focus bonuses (four-year plan, fortify the Vaterland, Fallschirmjager,
//!   ...) with the scripted bonus, categories and uses (two of the checked
//!   ones: 1.0/industry spent, 0.5/cat_fortification with 2 uses).
//! * `+0x100` array of `CLimitedUseTechCostReduction` (`+0x8` reduction
//!   raw/100000, `+0x28` i32 uses left, `+0x30` name). CHECKED live
//!   (GER_adopt_new_panzer_doctrine 0.5, one use).
//! * `+0xb8` array of `CIdeaResearchBonus` (`+0x10` category string, `+0x30`
//!   bonus raw/100000, `+0x38` the idea). CHECKED live (armor 0.1 from the
//!   Daimler-Benz designer idea, as its script says).
//! * `+0x118` array of `NFactions::CTechnologySharingFaction` (`+0x48` the
//!   `CFaction`, whose name string is at `+0x18`). The faction name is
//!   CHECKED live ("Axis"); the sharing bonus fields are not read.
use super::{
    mgmt::{self, FIXED_SCALE},
    ui::read,
};
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
};
use serde_json::{json, Value};
use std::time::Instant;

const STATUS_TYPE: &str = "17CTechnologyStatus";
const BONUS_TYPE: &str = "20CLimitedUseTechBonus";
const REDUCTION_TYPE: &str = "28CLimitedUseTechCostReduction";
const IDEA_BONUS_TYPE: &str = "18CIdeaResearchBonus";
const SHARING_TYPE: &str = "N9NFactions25CTechnologySharingFactionE";
const ARRAY_LIMIT: usize = 64;
const CATEGORY_LIMIT: usize = 32;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct TechBonus {
    pub name: String,
    pub uses_left: i32,
    pub bonus: f64,
    pub categories: Vec<String>,
    pub instance: i32,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct CostReduction {
    pub name: String,
    pub uses_left: i32,
    pub reduction: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct IdeaBonus {
    pub idea: String,
    pub category: String,
    pub bonus: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Extras {
    pub bonuses: Vec<TechBonus>,
    pub reductions: Vec<CostReduction>,
    pub idea_bonuses: Vec<IdeaBonus>,
    pub sharing: Vec<String>,
}

fn pointers(r: &Reader<'_>, array: u64, mangled: &str) -> Result<Vec<u64>> {
    let items = r.pointer_array(array)?;
    if items.len() > ARRAY_LIMIT {
        return Err(Error::new("invalid_array", "Implausible technology array"));
    }
    for item in &items {
        if !mgmt::is_pointer(*item) {
            return Err(Error::new("invalid_pointer", "Stale technology entry"));
        }
        mgmt::expect_type(r, *item, mangled)?;
    }
    Ok(items)
}

pub(super) fn read_extras(r: &Reader<'_>, status: u64) -> Result<Extras> {
    mgmt::expect_type(r, status, STATUS_TYPE)?;
    let mut bonuses = Vec::new();
    for b in pointers(r, status + 0xe8, BONUS_TYPE)? {
        let head = r.read(b, 0x70)?;
        let categories = r.pointer_array(b + 0x48)?;
        if categories.len() > CATEGORY_LIMIT {
            return Err(Error::new("invalid_array", "Implausible bonus categories"));
        }
        let mut names = Vec::new();
        for c in categories {
            if mgmt::is_pointer(c) {
                if let Some(name) = mgmt::text(r, c, 64)? {
                    names.push(name);
                }
            }
        }
        bonuses.push(TechBonus {
            name: mgmt::text(r, b + 0x10, 128)?.unwrap_or_default(),
            uses_left: mgmt::i32_at(&head, 8),
            bonus: mgmt::i64_at(&head, 0x68) as f64 / FIXED_SCALE,
            categories: names,
            instance: mgmt::i32_at(&head, 0x28),
        });
    }
    let mut reductions = Vec::new();
    for c in pointers(r, status + 0x100, REDUCTION_TYPE)? {
        let head = r.read(c, 0x30)?;
        reductions.push(CostReduction {
            name: mgmt::text(r, c + 0x30, 128)?.unwrap_or_default(),
            uses_left: mgmt::i32_at(&head, 0x28),
            reduction: mgmt::i64_at(&head, 8) as f64 / FIXED_SCALE,
        });
    }
    let mut idea_bonuses = Vec::new();
    for i in pointers(r, status + 0xb8, IDEA_BONUS_TYPE)? {
        let head = r.read(i, 0x40)?;
        let idea_ptr = mgmt::u64_at(&head, 0x38);
        let idea = if mgmt::is_pointer(idea_ptr) {
            mgmt::text(r, idea_ptr + 0x18, 128)?.unwrap_or_default()
        } else {
            String::new()
        };
        idea_bonuses.push(IdeaBonus {
            idea,
            category: mgmt::text(r, i + 0x10, 64)?.unwrap_or_default(),
            bonus: mgmt::i64_at(&head, 0x30) as f64 / FIXED_SCALE,
        });
    }
    let mut sharing = Vec::new();
    for s in pointers(r, status + 0x118, SHARING_TYPE)? {
        let faction = mgmt::pointer(r, s + 0x48)?;
        sharing.push(mgmt::text(r, faction + 0x18, 64)?.unwrap_or_default());
    }
    Ok(Extras {
        bonuses,
        reductions,
        idea_bonuses,
        sharing,
    })
}

fn snapshot(r: &Reader<'_>, image: u64) -> Result<Extras> {
    let own = mgmt::own(r, image)?;
    let status = mgmt::pointer(r, own.country + game::layout(r).country_technology_status)?;
    if r.u64(status + 0x130)? != own.country {
        return Err(Error::new(
            "invalid_research_owner",
            "Own research status country join required",
        ));
    }
    read_extras(r, status)
}

pub(super) fn assemble(e: &Extras) -> Value {
    let bonuses: Vec<Value> = e
        .bonuses
        .iter()
        .map(|b| {
            json!({
                "name": b.name,
                "uses_left": b.uses_left,
                "exhausted": b.uses_left <= 0,
                "bonus": mgmt::round(b.bonus, 4),
                "categories": b.categories,
                "instance": b.instance,
            })
        })
        .collect();
    let reductions: Vec<Value> = e
        .reductions
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "uses_left": c.uses_left,
                "exhausted": c.uses_left <= 0,
                "reduction": mgmt::round(c.reduction, 4),
            })
        })
        .collect();
    let ideas: Vec<Value> = e
        .idea_bonuses
        .iter()
        .map(|i| json!({"idea": i.idea, "category": i.category, "bonus": mgmt::round(i.bonus, 4)}))
        .collect();
    let live = e.bonuses.iter().filter(|b| b.uses_left > 0).count();
    json!({
        "tech_bonuses": {"entries": bonuses, "usable": live, "validated": true},
        "tech_cost_reductions": {"entries": reductions, "validated": true},
        "idea_research_bonuses": {"entries": ideas, "validated": true},
        "tech_sharing": {"factions": e.sharing, "validated": true, "bonus_values_read": false},
        "not_read": {
            "ahead_of_time_penalty": "computed by the game per technology; not stored as a readable field",
            "licenses": "licensed-production relations are not mapped on macOS yet",
            "scientists": "see hoi4_special_projects (scientists per specialization)",
        },
    })
}

pub(super) fn research_extras(process: &dyn Process) -> Result<Value> {
    let started = Instant::now();
    let (extras, context, stamp, build_id) = read(process, |r, image| {
        let extras = snapshot(r, image)?;
        Ok((
            extras,
            mgmt::clock(r, image)?,
            mgmt::stamp(started, r),
            game::layout(r).build_id,
        ))
    })?;
    let mut body = assemble(&extras);
    let object = body.as_object_mut().expect("object");
    object.insert("source".into(), json!("native own research extras read"));
    object.insert("build_id".into(), json!(build_id));
    object.insert("owned".into(), json!(true));
    object.insert("game_time".into(), context);
    object.insert("freshness".into(), stamp);
    object.insert("observation_paused_game".into(), json!(false));
    object.insert("input_authorized".into(), json!(false));
    Ok(body)
}

pub(super) fn tool() -> Value {
    json!({"name":"hoi4_research_extras","description":"Own research bonuses that the research panel only shows as tooltips, read from game memory (no input, no lease, never pauses): limited-use technology bonuses ('free research bonus': name, uses left, bonus as a fraction, categories), limited-use cost reductions, idea research bonuses, and technology-sharing faction membership. The ahead-of-time penalty, licenses and scientist rosters are not mapped (see the not_read block; scientists belong to hoi4_special_projects). Validated live against the focus and idea scripts of the owned game.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}})
}

#[cfg(test)]
mod tests {
    use super::super::mgmt::fake::Mem;
    use super::*;

    fn object(m: &Mem, at: u64, class: &str, slot: u64) {
        let vt = 0x41000 + slot * 0x100;
        m.q(at, vt);
        m.q(vt - 8, vt + 0x40);
        m.q(vt + 0x40 + 8, vt + 0x80);
        m.patch(vt + 0x80, format!("{class}\0").as_bytes());
    }
    fn array(m: &Mem, at: u64, data: u64, items: &[u64]) {
        m.q(at, data);
        m.i(at + 8, items.len() as i32 + 1);
        m.i(at + 12, items.len() as i32);
        for (i, v) in items.iter().enumerate() {
            m.q(data + 8 * i as u64, *v);
        }
    }

    fn build() -> Mem {
        let m = Mem::default();
        m.alloc(0x40000, 0x5000);
        m.alloc(0x100000, 0x400); // status
        m.alloc(0x110000, 0x100); // arrays storage
        m.alloc(0x120000, 0x100); // bonus 0
        m.alloc(0x121000, 0x100); // bonus 1
        m.alloc(0x130000, 0x100); // category objects
        m.alloc(0x140000, 0x100); // cost reduction
        m.alloc(0x150000, 0x100); // idea bonus
        m.alloc(0x160000, 0x100); // idea
        m.alloc(0x170000, 0x100); // sharing
        m.alloc(0x180000, 0x100); // faction
        object(&m, 0x100000, "17CTechnologyStatus", 0);
        // Bonuses
        array(&m, 0x1000e8, 0x110000, &[0x120000, 0x121000]);
        object(&m, 0x120000, BONUS_TYPE, 1);
        m.i(0x120008, 0);
        m.sso(0x120010, "GER_the_four_year_plan");
        m.i(0x120028, 1);
        array(&m, 0x120048, 0x110040, &[0x130000]);
        m.l(0x120068, 100_000);
        m.sso(0x130000, "industry");
        object(&m, 0x121000, BONUS_TYPE, 1);
        m.i(0x121008, 2);
        m.sso(0x121010, "GER_fortify_the_vaterland");
        m.i(0x121028, 2);
        array(&m, 0x121048, 0x110080, &[0x130040]);
        m.l(0x121068, 50_000);
        m.sso(0x130040, "cat_fortification");
        // Cost reduction
        array(&m, 0x100100, 0x1100c0, &[0x140000]);
        object(&m, 0x140000, REDUCTION_TYPE, 2);
        m.l(0x140008, 50_000);
        m.i(0x140028, 1);
        m.sso(0x140030, "GER_adopt_new_panzer_doctrine");
        // Idea bonus
        array(&m, 0x1000b8, 0x110000 + 0xe0, &[0x150000]);
        object(&m, 0x150000, IDEA_BONUS_TYPE, 3);
        m.sso(0x150010, "armor");
        m.l(0x150030, 10_000);
        m.q(0x150038, 0x160000);
        m.sso(0x160018, "GER_daimler_benz");
        // Sharing
        array(&m, 0x100118, 0x110000 + 0xf0, &[0x170000]);
        object(&m, 0x170000, SHARING_TYPE, 4);
        m.q(0x170048, 0x180000);
        m.sso(0x180018, "Axis");
        m
    }

    #[test]
    fn extras_decode_bonuses_reductions_idea_bonuses_and_sharing() {
        let m = build();
        let r = Reader::new(&m);
        let e = read_extras(&r, 0x100000).unwrap();
        assert_eq!(e.bonuses.len(), 2);
        assert_eq!(e.bonuses[0].name, "GER_the_four_year_plan");
        assert_eq!(e.bonuses[0].uses_left, 0);
        assert_eq!(e.bonuses[0].categories, ["industry"]);
        assert_eq!(e.bonuses[1].bonus, 0.5);
        assert_eq!(e.bonuses[1].uses_left, 2);
        assert_eq!(e.reductions[0].reduction, 0.5);
        assert_eq!(e.reductions[0].uses_left, 1);
        assert_eq!(e.idea_bonuses[0].idea, "GER_daimler_benz");
        assert_eq!(e.idea_bonuses[0].bonus, 0.1);
        assert_eq!(e.sharing, ["Axis"]);
    }

    #[test]
    fn assembly_flags_exhausted_bonuses_and_lists_what_is_not_read() {
        let m = build();
        let r = Reader::new(&m);
        let v = assemble(&read_extras(&r, 0x100000).unwrap());
        assert_eq!(v["tech_bonuses"]["usable"], json!(1));
        assert_eq!(v["tech_bonuses"]["entries"][0]["exhausted"], json!(true));
        assert_eq!(v["tech_bonuses"]["entries"][1]["exhausted"], json!(false));
        assert!(v["not_read"]["licenses"].is_string());
        assert_eq!(v["tech_sharing"]["bonus_values_read"], json!(false));
    }

    #[test]
    fn wrong_object_types_and_stale_entries_are_errors() {
        let m = build();
        // Entry of the wrong class.
        object(&m, 0x121000, REDUCTION_TYPE, 2);
        let r = Reader::new(&m);
        assert_eq!(read_extras(&r, 0x100000).unwrap_err().code, "invalid_type");
        // Status that is not a technology status.
        let m = build();
        object(&m, 0x100000, "8CCountry", 5);
        let r = Reader::new(&m);
        assert_eq!(read_extras(&r, 0x100000).unwrap_err().code, "invalid_type");
        // A null pointer inside an array is stale.
        let m = build();
        m.q(0x110008, 0);
        let r = Reader::new(&m);
        assert_eq!(
            read_extras(&r, 0x100000).unwrap_err().code,
            "invalid_pointer"
        );
    }

    #[test]
    fn tool_is_read_only_without_lease() {
        let t = tool();
        assert_eq!(t["name"], json!("hoi4_research_extras"));
        assert!(t["inputSchema"]["properties"]
            .as_object()
            .unwrap()
            .is_empty());
    }
}
