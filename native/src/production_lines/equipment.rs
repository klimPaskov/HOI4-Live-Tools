//! Base equipment types referenced by authenticated own production lines only.
//! Semantics consulted: Clausewitz-MCP calling/game.py production_lines/_load_equipment.
//! Current Linux layout independently recovered in production-variants-v2,
//! ELF a4971c9e8ac9a6f2487dea109e01f9839ee59db7. No variant design/owner payload.
use crate::{
    error::{Error, Result},
    memory::Reader,
};
use std::collections::{BTreeMap, BTreeSet};

const CATALOG_LIMIT: usize = 8192;
pub(super) struct Catalog {
    members: BTreeSet<u64>,
    selected: BTreeMap<u64, (u32, String)>,
    keys: BTreeMap<String, u64>,
}
pub(super) struct Equipment {
    pub key: Option<String>,
    pub unavailable: Option<&'static str>,
}
impl Equipment {
    fn unavailable(reason: &'static str) -> Self {
        Self {
            key: None,
            unavailable: Some(reason),
        }
    }
}
fn invalid(message: &str) -> Error {
    Error::new("invalid_production_equipment", message)
}
fn pointer(p: u64) -> Result<()> {
    if !(0x10000..=0x7fff_ffff_0000).contains(&p) || p % 8 != 0 {
        return Err(invalid("Bounded aligned equipment pointer required"));
    }
    Ok(())
}
fn word(r: &Reader<'_>, address: u64) -> Result<u32> {
    Ok(u32::from_le_bytes(r.read(address, 4)?.try_into().unwrap()))
}
impl Catalog {
    fn load(r: &Reader<'_>, base: u64) -> Result<Self> {
        let l = super::layout(r);
        let db = r.u64(base + l.equipment_db_global)?;
        pointer(db)?;
        if r.u64(db)? != base + l.equipment_db_vtable {
            return Err(invalid("Exact equipment database primary required"));
        }
        let (header, data, count) = r.array_header(db + l.equipment_catalog)?;
        let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
        if capacity > CATALOG_LIMIT || (count > 0 && data % 8 != 0) {
            return Err(invalid("Bounded aligned base-equipment catalog required"));
        }
        let bytes = if count == 0 {
            Vec::new()
        } else {
            r.read(data, count * 8)?
        };
        let mut members = BTreeSet::new();
        for bytes in bytes.chunks_exact(8) {
            let p = u64::from_le_bytes(bytes.try_into().unwrap());
            pointer(p)?;
            if !members.insert(p) {
                return Err(invalid("Unique base-equipment catalog pointers required"));
            }
        }
        // This list includes a derived null sentinel. Unselected pointers are
        // compared only; their primary types and payloads are never traversed.
        Ok(Self {
            members,
            selected: BTreeMap::new(),
            keys: BTreeMap::new(),
        })
    }
}
/// The caller must already prove exact line type, own reciprocal status and
/// typed line handle. All bytes participate in its final whole-snapshot witness.
pub(super) fn observe(
    r: &Reader<'_>,
    base: u64,
    line: u64,
    catalog: &mut Option<Catalog>,
) -> Result<Equipment> {
    let l = super::layout(r);
    let variant = r.u64(line + 0x88)?;
    if variant == 0 {
        return Ok(Equipment::unavailable("no_variant"));
    }
    pointer(variant)?;
    if r.u64(variant)? != base + l.variant_vtable {
        return Ok(Equipment::unavailable("unsupported_variant_type"));
    }
    let equipment_type = r.u64(variant + l.variant_type)?;
    if equipment_type == 0 {
        return Ok(Equipment::unavailable("no_type"));
    }
    pointer(equipment_type)?;
    if catalog.is_none() {
        *catalog = Some(Catalog::load(r, base)?);
    }
    let catalog = catalog.as_mut().expect("fresh catalog initialized");
    if !catalog.members.contains(&equipment_type) {
        return Ok(Equipment::unavailable("type_outside_reviewed_base_catalog"));
    }
    if r.u64(equipment_type)? != base + l.equipment_type_vtable {
        return Ok(Equipment::unavailable("unsupported_equipment_type"));
    }
    let intern = word(r, variant + 0x14)?;
    let (type_intern, key) = if let Some(value) = catalog.selected.get(&equipment_type) {
        value.clone()
    } else {
        let type_intern = word(r, equipment_type + 8)?;
        // Intern IDs are not array indices. Compare IDs before the type name.
        if intern != type_intern {
            return Err(invalid("Variant and base-type intern keys differ"));
        }
        let key = r.string_bounded(equipment_type + 0x10, 128)?;
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(invalid("Bounded canonical base-equipment key required"));
        }
        if catalog.keys.insert(key.clone(), equipment_type).is_some() {
            return Err(invalid(
                "Distinct selected equipment types require distinct canonical keys",
            ));
        }
        catalog
            .selected
            .insert(equipment_type, (type_intern, key.clone()));
        (type_intern, key)
    };
    if intern != type_intern {
        return Err(invalid("Variant and base-type intern keys differ"));
    }
    Ok(Equipment {
        key: Some(key),
        unavailable: None,
    })
}
