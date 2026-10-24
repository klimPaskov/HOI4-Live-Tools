//! Linux selection facts inside the retained queue's witness bracket.
//! These facts do not prove routing, eligibility, delivery or an action effect.
use crate::{
    construction::{self as c, catalog},
    error::{Error, Result},
    memory::Reader,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Selection {
    Inactive,
    Requested,
}

#[derive(Clone, Debug)]
pub(super) struct Observation {
    pub(super) manager: u64,
    pub(super) selection: Selection,
}

fn unavailable(message: &str) -> Error {
    Error::new("construction_selection_unavailable", message)
}

/// The original own idler has already passed the caller's scoped guard.
/// Reject a changed manager pointer before reading its replacement payload.
pub(super) fn manager(r: &Reader<'_>, idler: u64, original: Option<u64>) -> Result<u64> {
    let current = r.u64(idler + 0x6b8)?;
    if original.is_some_and(|old| old != current) {
        return Err(unavailable("Original Linux construction manager changed"));
    }
    c::pointer(current)?;
    if r.u64(current + 0x180)? != idler {
        return Err(unavailable("Linux construction manager owner changed"));
    }
    Ok(current)
}

/// Read only the requested ordinary template or inactive mode. The complete
/// catalog and target were validated in this same witness bracket. A selected
/// foreign/replacement pointer is compared before any dependent payload read.
pub(super) fn observe(
    r: &Reader<'_>,
    manager: u64,
    catalog: &catalog::Catalog,
    key: &str,
) -> Result<Observation> {
    let selection = match r.i32(manager + 0x3f0)? {
        0 => Selection::Inactive, // Never read the inactive/default pointer.
        1 => {
            let target = catalog.required(key)?;
            if r.u64(manager + 0x3e8)? != target.address {
                return Err(unavailable(
                    "Requested original ordinary template not selected",
                ));
            }
            let railway = r.u64(catalog.database + 0x3a8)?;
            c::pointer(railway)?;
            if target.address == railway || target.key == "rail_way" {
                return Err(unavailable(
                    "Railway selection is not ordinary construction",
                ));
            }
            Selection::Requested
        }
        _ => return Err(unavailable("Unsupported Linux construction selection mode")),
    };
    Ok(Observation { manager, selection })
}
