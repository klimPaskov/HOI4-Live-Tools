//! Only static definitions referenced by authenticated own building lines.
//! Never traverses building location/foreign state, localized names or rail paths.
use super::{boolean, invalid, members, pointer};
use crate::{error::Result, memory::Reader};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct Catalog {
    members: BTreeSet<u64>,
    selected: BTreeMap<u64, String>,
    keys: BTreeMap<String, u64>,
}
pub(super) struct Building {
    pub key: Option<String>,
    pub unavailable: Option<&'static str>,
}
impl Building {
    fn unavailable(reason: &'static str) -> Self {
        Self {
            key: None,
            unavailable: Some(reason),
        }
    }
}
pub(super) fn observe(
    r: &Reader<'_>,
    base: u64,
    line: u64,
    catalog: &mut Option<Catalog>,
) -> Result<Building> {
    let building = r.u64(line + 0x70)?;
    if building == 0 {
        return Ok(Building::unavailable("no_building"));
    }
    pointer(building)?;
    if r.u64(building)? != base + 0x41b3e60 {
        return Ok(Building::unavailable("unsupported_building_type"));
    }
    let template = r.u64(building + 0x1c0)?;
    if template == 0 {
        return Ok(Building::unavailable("no_template"));
    }
    pointer(template)?;
    if catalog.is_none() {
        let db = r.u64(base + 0x439f400)?;
        pointer(db)?;
        if r.u64(db)? != base + 0x41b3ed8 {
            return Err(invalid("Exact building database primary required"));
        }
        *catalog = Some(Catalog {
            members: members(r, db + 0x40, 4096)?.into_iter().collect(),
            selected: BTreeMap::new(),
            keys: BTreeMap::new(),
        });
    }
    let catalog = catalog.as_mut().expect("fresh catalog initialized");
    if !catalog.members.contains(&template) {
        return Ok(Building::unavailable("template_outside_reviewed_catalog"));
    }
    if r.u64(template)? != base + 0x41b49c8 {
        return Ok(Building::unavailable("unsupported_template_type"));
    }
    // BuildingReference constructors require this flag before using a template;
    // it is not an assertion about player eligibility or current activity.
    if !boolean(r, template + 0x18)? {
        return Ok(Building::unavailable("template_validity_flag_unset"));
    }
    let key = if let Some(key) = catalog.selected.get(&template) {
        key.clone()
    } else {
        // The parser key is copied here by the template constructor. Neither
        // building+8 nor template+8 is a catalog index. Localized names are separate.
        let key = r.string_bounded(template + 0x248, 128)?;
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(invalid("Bounded canonical building key required"));
        }
        if catalog.keys.insert(key.clone(), template).is_some() {
            return Err(invalid("Distinct selected templates require distinct keys"));
        }
        catalog.selected.insert(template, key.clone());
        key
    };
    Ok(Building {
        key: Some(key),
        unavailable: None,
    })
}
