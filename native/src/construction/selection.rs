//! Owned manager selection, independently recovered from the exact Linux ELF.
//! Selection is not placement eligibility, effective map routing or input proof.
use super::{boolean, invalid, members, pointer};
use crate::{error::Result, memory::Reader};
use serde_json::{json, Value};

pub(super) fn observe(r: &Reader<'_>, base: u64, manager: u64) -> Result<Value> {
    // The caller proves the current positive own player and manager ownership.
    // The setter writes mode before pointer; both participate in final witnesses.
    let mode = r.i32(manager + 0x3f0)?;
    let mut out = json!({"scope":"owned_interface_selection","mode_raw":mode,
        "supported":mode==0 || mode==1 || mode==3,"active":null,"building_id":null,
        "reason":"unsupported_mode","placement_eligibility_verified":false,
        "map_routing_verified":false,"input_authorized":false,"live_validation":"pending"});
    if mode == 0 {
        out["active"] = json!(false);
        out["reason"] = json!("inactive");
        return Ok(out);
    }
    if mode != 1 && mode != 3 {
        return Ok(out);
    }
    // Never follow the default/sentinel pointer for inactive or unknown modes.
    let selected = r.u64(manager + 0x3e8)?;
    pointer(selected)?;
    let db = r.u64(base + 0x439f400)?;
    pointer(db)?;
    if r.u64(db)? != base + 0x41b3ed8 {
        return Err(invalid(
            "Exact selection building database primary required",
        ));
    }
    // Independent of queue membership: a choice may exist with no queued lines.
    let catalog = members(r, db + 0x40, 4096)?;
    let Some(ordinal) = catalog.iter().position(|p| *p == selected) else {
        return Err(invalid("Selected building must belong to fresh catalog"));
    };
    if r.u64(selected)? != base + 0x41b49c8 || !boolean(r, selected + 0x18)? {
        return Err(invalid("Exact valid selected building template required"));
    }
    let stored_ordinal = r.i32(selected + 0x2b8)?;
    if stored_ordinal < 0 || stored_ordinal as usize != ordinal {
        return Err(invalid("Selected building ordinal must match catalog slot"));
    }
    let key = r.string_bounded(selected + 0x248, 128)?;
    if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(invalid("Bounded canonical selected building key required"));
    }
    // The database's rail_way cache corroborates mode, without reading its payload.
    let railway = r.u64(db + 0x3a8)?;
    pointer(railway)?;
    if (mode == 3 && (selected != railway || key != "rail_way"))
        || (mode == 1 && selected == railway)
    {
        return Err(invalid("Construction mode and special template disagree"));
    }
    out["active"] = json!(true);
    out["building_id"] = json!(key);
    out["reason"] = json!(if mode == 3 {
        "rail_way_choice"
    } else {
        "ordinary_construction_choice"
    });
    Ok(out)
}
