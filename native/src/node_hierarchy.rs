//! Typed reciprocal GUI hierarchy only, never own-country ancestor semantics.
use crate::{
    error::{Error, Result},
    memory::Reader,
    node_reuse::Reuse,
    research_ui::exact,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
const POSITION: u64 = 0x43484f0;
const TYPES: [u64; 4] = [POSITION, 0x4356e08, 0x434b370, 0x434c5b0];
fn membership(r: &Reader<'_>, parent: u64, child: u64, total: &mut usize) -> Result<usize> {
    let (header, data, count) = r.array_header(parent + 0xc0)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if capacity > 2048 || (count > 0 && data % 8 != 0) || count > 4096 - *total {
        return Err(Error::new(
            "invalid_gui_hierarchy_bound",
            "Hierarchy child-array bound exceeded",
        ));
    }
    *total += count;
    let payload = if count == 0 {
        Vec::new()
    } else {
        r.read(data, count * 8)?
    };
    if payload
        .chunks_exact(8)
        .filter(|p| u64::from_le_bytes((*p).try_into().unwrap()) == child)
        .count()
        != 1
    {
        return Err(Error::new(
            "invalid_gui_hierarchy",
            "Unique reciprocal child membership required",
        ));
    }
    // Sibling pointers are compared only; never dereferenced.
    Ok(count)
}
pub(crate) fn observe(
    r: &Reader<'_>,
    reuse: &mut Reuse,
    container: u64,
    render: u64,
    registry: u64,
) -> Result<Value> {
    let base = reuse.base();
    exact(r, registry, base + 0x42736f8)?;
    exact(r, container, base + 0x4347d58)?;
    if r.u64(container + 0x98)? != registry {
        return Err(Error::new(
            "invalid_gui_owner",
            "Own container registry join required",
        ));
    }
    let graphics = r.u64(base + 0x4577890)?;
    exact(r, graphics, base + 0x42417f8)?;
    if r.u64(registry + 0xb8)? != graphics {
        return Err(Error::new(
            "invalid_gui_owner",
            "Own graphics registry join required",
        ));
    }
    let roots = [graphics + 0x1f0, graphics + 0x350];
    for root in roots {
        exact(r, root, base + POSITION)?;
    }
    let positions = [r.u64(container + 0xe70)?, r.u64(container + 0xe78)?];
    if positions[0] == positions[1] || positions.iter().any(|p| *p == 0 || roots.contains(p)) {
        return Err(Error::new(
            "invalid_gui_position",
            "Distinct constructor-created container positions required",
        ));
    }
    for p in positions {
        exact(r, p, base + POSITION)?;
    }
    let mut total = 0;
    if r.u64(positions[1] + 0x108)? != positions[0] {
        return Err(Error::new(
            "invalid_gui_position",
            "Container E78 parent must match E70",
        ));
    }
    membership(r, positions[0], positions[1], &mut total)?;
    // Constructor-positive positions need no control138: null is legitimate.
    let mut child = render;
    let mut seen = BTreeSet::from([render]);
    let mut chain = Vec::new();
    let mut status = "depth_limit";
    for _ in 0..16 {
        let parent = r.u64(child + 0x108)?;
        if parent == 0 {
            status = "detached_ancestor";
            break;
        }
        if parent % 8 != 0 || !seen.insert(parent) {
            return Err(Error::new(
                "invalid_gui_hierarchy",
                "Unaligned or cyclic renderer hierarchy",
            ));
        }
        let table = r.u64(parent)?;
        let (encoding, metadata_ok) = match reuse.nullable_type(r, parent) {
            Ok(v) => (v, true),
            Err(e) if e.code == "invalid_gui_type" => (Value::Null, false),
            Err(e) => return Err(e),
        };
        if !TYPES.iter().any(|v| table == base + v) {
            chain.push(
                json!({"type_encoding":encoding,"payload_read":false,"role":"unknown_ancestor"}),
            );
            status = if metadata_ok {
                "unknown_ancestor"
            } else {
                "unknown_primary_metadata"
            };
            break;
        }
        let count = membership(r, parent, child, &mut total)?;
        let role = if parent == roots[0] {
            "owned_graphics_root_1f0"
        } else if parent == roots[1] {
            "owned_graphics_root_350"
        } else if parent == positions[0] {
            "owned_container_e70"
        } else if parent == positions[1] {
            "owned_container_e78"
        } else {
            "exact_reciprocal_gui_ancestor"
        };
        chain.push(json!({"type_encoding":encoding,"role":role,"payload_read":true,"reciprocal_child_membership":true,"child_count_raw":count}));
        if roots.contains(&parent) {
            status = "closed_to_owned_graphics_root";
            break;
        }
        child = parent;
    }
    let payload_hops = chain
        .iter()
        .filter(|node| node["payload_read"] == true)
        .count();
    Ok(
        json!({"limit":16,"observed_ancestors":chain.len(),"payload_hops":payload_hops,"chain":chain,"status":status,"graphics_root_closed":status=="closed_to_owned_graphics_root","container_position_joins_verified":true,"gui_hierarchy_provenance":"typed reciprocal links plus constructor positions/owned graphics anchor","ancestor_own_country_semantics":"unproved","client_rectangle_available":false,"effective_routing_verified":false,"input_authorized":false}),
    )
}
