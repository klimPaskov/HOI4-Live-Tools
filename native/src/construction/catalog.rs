//! Complete canonical public building catalog under the caller's witnesses.
use super::{boolean, controls::validate_key, invalid, location, members};
use crate::{
    error::{Error, Result},
    memory::Reader,
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Template {
    pub address: u64,
    pub ordinal: usize,
    pub key: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Catalog {
    pub database: u64,
    pub members: Vec<u64>,
    pub templates: BTreeMap<u64, Template>,
}
impl Catalog {
    pub fn required(&self, key: &str) -> Result<&Template> {
        self.templates
            .values()
            .find(|t| t.key == key)
            .ok_or_else(|| {
                Error::new(
                    "construction_target_unavailable",
                    "Requested building absent from the current valid public catalog",
                )
            })
    }
}
fn phase(stage: &str, error: Error) -> Error {
    Error::new(error.code, format!("{stage}: {}", error.message))
}

/// An empty queue still needs an existing canonical template. Validate the
/// complete bounded public catalog; invalid sentinel templates have no payload.
pub(super) fn observe(r: &Reader<'_>, base: u64) -> Result<Catalog> {
    let l = super::layout(r);
    let db = r.u64(base + l.database_global)?;
    location::exact(r, db, base + l.database_vtable)
        .map_err(|e| phase("public_catalog_database", e))?;
    let catalog = members(r, db + 0x40, 4096)?;
    let mut keys = BTreeSet::new();
    let mut templates = BTreeMap::new();
    for (ordinal, address) in catalog.iter().copied().enumerate() {
        // The inspected builds have a distinct TNullObject<CBuildingTemplate>
        // primary at catalog slot zero. Only that exact inactive sentinel may
        // omit canonical payload. Other derived/unknown types still refuse.
        if r.u64(address)? == base + l.null_template_vtable {
            if ordinal != 0 || boolean(r, address + 0x18)? {
                return Err(invalid(
                    "Inactive null building template required only at catalog zero",
                ));
            }
            continue;
        }
        location::exact(r, address, base + l.template_vtable)
            .map_err(|e| phase(&format!("public_catalog_template_{ordinal}"), e))?;
        if !boolean(r, address + 0x18)? {
            continue;
        }
        if r.i32(address + l.template_ordinal)? != ordinal as i32 {
            return Err(invalid("Construction template ordinal/catalog mismatch"));
        }
        let canonical = r.string_bounded(address + l.template_key, 128)?;
        validate_key(&canonical).map_err(|_| invalid("Bounded canonical building key required"))?;
        if !keys.insert(canonical.clone()) {
            return Err(invalid("Unique canonical building catalog keys required"));
        }
        templates.insert(
            address,
            Template {
                address,
                ordinal,
                key: canonical,
            },
        );
    }
    Ok(Catalog {
        database: db,
        members: catalog,
        templates,
    })
}
