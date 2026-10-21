//! Bounded Linux GUI registry name lookup shared by the reviewed readers.
//!
//! Callers select the independently recovered registry and validate its pointer
//! collection and owning container. This helper only joins its parallel GNU
//! string array; it does not treat a text registry as a generic widget tree.
//! No global fallback, persistent cache, geometry or input authority is supplied.
// GNU string semantics follow memory.rs / Clausewitz-MCP calling/mem.py.
// Copyright (c) 2026 smerss, MIT; see ../LICENSE.clausewitz.
use crate::{
    error::{Error, Result},
    memory::{Reader, StringAbi},
};

const MAX_NAMES: usize = 256;

fn invalid(message: &str) -> Error {
    Error::new("invalid_gui_registry", message)
}

/// Join one exact key to a caller-validated pointer collection. Unrelated names
/// may repeat, but every name is inspected before returning a unique match.
/// The caller's snapshot witnesses must cover the pointer collection as well
/// as this lookup. Local rechecks catch changed name metadata, not atomicity.
pub(crate) fn lookup(
    r: &Reader<'_>,
    names_header: u64,
    values: &[u64],
    key: &str,
    name_limit: usize,
) -> Result<Option<u64>> {
    if r.string_abi() != StringAbi::LibStdCxx {
        return Err(Error::new(
            "capability_unavailable",
            "GNU GUI name registry requires the reviewed Linux string ABI",
        ));
    }
    if values.len() > MAX_NAMES || !(1..=256).contains(&name_limit) {
        return Err(invalid("Bounded GUI name collection required"));
    }
    let (header, data, count) = r.array_header(names_header)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if capacity > MAX_NAMES || count != values.len() || (count != 0 && data % 8 != 0) {
        return Err(invalid("Bounded paired GUI names and pointers required"));
    }
    // Most GUI names use GNU's inline string. Fetch all headers together,
    // including their inline bytes, rather than twice per individual string.
    let names = if count == 0 {
        Vec::new()
    } else {
        r.read(data, count * 32)?
    };
    let mut selected = None;
    for (index, bytes) in names.chunks_exact(32).enumerate() {
        let address = data + index as u64 * 32;
        let pointer = u64::from_le_bytes(bytes[..8].try_into().unwrap());
        let size = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
        if size > name_limit as u64 || (pointer == address + 16 && size > 15) {
            return Err(Error::new("invalid_string", "Invalid bounded GUI name"));
        }
        let heap;
        let payload: &[u8] = if pointer == address + 16 {
            &bytes[16..16 + size as usize]
        } else if size == 0 {
            &[]
        } else {
            heap = r.read(pointer, size as usize)?;
            &heap
        };
        let name = std::str::from_utf8(payload)
            .map_err(|_| Error::new("invalid_string", "Invalid UTF-8 GUI name"))?;
        if name == key && selected.replace(values[index]).is_some() {
            return Err(invalid("Ambiguous requested GUI name"));
        }
    }
    if (count != 0 && r.read(data, count * 32)? != names) || r.read(names_header, 16)? != header {
        return Err(Error::new("torn_read", "GUI name registry changed"));
    }
    Ok(selected)
}

#[cfg(test)]
mod tests;
