//! Typed reciprocal GUI hierarchy only, never own-country ancestor semantics.
use crate::{
    error::{Error, Result},
    memory::Reader,
    node_reuse::{HierarchyPolicy, Reuse},
    research_ui::exact,
};
use serde_json::{json, Value};
use std::collections::BTreeSet;
const POSITION: u64 = 0x43484f0;
const TYPES: [u64; 4] = [POSITION, 0x4356e08, 0x434b370, 0x434c5b0];
enum Membership {
    Verified(usize),
    PolicyUnavailable(Value),
}
fn membership(
    r: &Reader<'_>,
    parent: u64,
    child: u64,
    total: &mut usize,
    owned_root: bool,
    policy: HierarchyPolicy,
    focus_position: bool,
) -> Result<Membership> {
    let (header, data, count) = r.array_header(parent + 0xc0)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    // Malformed structure is never an optional policy refusal, even when its
    // otherwise valid size would also exceed our conservative traversal cap.
    if count > 0 && data % 8 != 0 {
        return Err(Error::new(
            "invalid_gui_hierarchy",
            "Aligned nonempty hierarchy child array required",
        ));
    }
    // Recorded run 031 had own-focus position count 4687/capacity 5395. The opt-in
    // permits only existing typed positions beyond the owned container. All
    // sibling entries remain pointer comparisons, bounded to 128 KiB per path.
    let focus = policy == HierarchyPolicy::OwnFocus;
    let (capacity_limit, aggregate_limit, scope) = if owned_root {
        (
            8192,
            if focus { 16384 } else { 8192 },
            "exact_owned_graphics_root",
        )
    } else if focus && focus_position {
        (8192, 16384, "own_focus_position_ancestor")
    } else {
        (2048, if focus { 16384 } else { 4096 }, "ordinary_ancestor")
    };
    if capacity > capacity_limit || *total > aggregate_limit || count > aggregate_limit - *total {
        return Ok(Membership::PolicyUnavailable(json!({
            "capacity":capacity,"count":count,
            "inspected_entries_before_attempt":*total,
            "capacity_limit":capacity_limit,"aggregate_entry_limit":aggregate_limit,"policy_scope":scope,
            "child_payload_read":false
        })));
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
    Ok(Membership::Verified(count))
}
fn summary(
    chain: Vec<Value>,
    status: &str,
    container_verified: bool,
    policy: Option<Value>,
) -> Value {
    let payload_hops = chain
        .iter()
        .filter(|node| node["payload_read"] == true)
        .count();
    json!({"limit":16,"observed_ancestors":chain.len(),"payload_hops":payload_hops,"chain":chain,"status":status,"policy_boundary":policy,"unit_cache_diagnostic":{"status":"unavailable_incomplete_hierarchy","current_frame_verified":false,"input_authorized":false},"graphics_root_closed":status=="closed_to_owned_graphics_root","container_position_joins_verified":container_verified,"gui_hierarchy_provenance":"typed reciprocal links plus constructor positions/owned graphics anchor; incomplete when a boundary is reported","ancestor_own_country_semantics":"unproved","client_rectangle_available":false,"effective_routing_verified":false,"input_authorized":false})
}
pub(crate) fn observe(
    r: &Reader<'_>,
    reuse: &mut Reuse,
    container: u64,
    render: u64,
    registry: u64,
) -> Result<Value> {
    let base = reuse.base();
    let policy = reuse.hierarchy_policy();
    // Text is supported ONLY as the constructor-proved leaf, not as an ancestor.
    let leaf = r.u64(render)?;
    if !TYPES.iter().any(|table| leaf == base + table) && leaf != base + 0x43593d8 {
        return Err(Error::new(
            "invalid_gui_hierarchy",
            "Exact supported hierarchy leaf required",
        ));
    }
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
    if let Membership::PolicyUnavailable(mut policy) = membership(
        r,
        positions[0],
        positions[1],
        &mut total,
        false,
        policy,
        false,
    )? {
        policy["stage"] = json!("container_position_membership");
        return Ok(summary(
            Vec::new(),
            "unavailable_policy_bound",
            false,
            Some(policy),
        ));
    }
    // Constructor-positive positions need no control138: null is legitimate.
    let mut typed_path = vec![render];
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
        let count = match membership(
            r,
            parent,
            child,
            &mut total,
            roots.contains(&parent),
            policy,
            role == "exact_reciprocal_gui_ancestor" && table == base + POSITION,
        )? {
            Membership::Verified(count) => count,
            Membership::PolicyUnavailable(mut policy) => {
                policy["stage"] = json!("ancestor_membership");
                policy["type_encoding"] = encoding;
                policy["role"] = json!(role);
                return Ok(summary(
                    chain,
                    "unavailable_policy_bound",
                    true,
                    Some(policy),
                ));
            }
        };
        let root = roots.contains(&parent);
        let focus_position = policy == HierarchyPolicy::OwnFocus
            && role == "exact_reciprocal_gui_ancestor"
            && table == base + POSITION;
        chain.push(json!({"type_encoding":encoding,"role":role,"payload_read":true,"reciprocal_child_membership":true,"child_count_raw":count,"membership_policy_scope":if root{"exact_owned_graphics_root"}else if focus_position{"own_focus_position_ancestor"}else{"ordinary_ancestor"},"capacity_limit":if root || focus_position{8192}else{2048},"aggregate_entry_limit":if policy==HierarchyPolicy::OwnFocus{16384}else if root{8192}else{4096},"inspected_entries_after":total}));
        typed_path.push(parent);
        if roots.contains(&parent) {
            status = "closed_to_owned_graphics_root";
            break;
        }
        child = parent;
    }
    let mut result = summary(chain, status, true, None);
    result["unit_cache_diagnostic"] = if status == "closed_to_owned_graphics_root" {
        crate::node_unit_cache::observe(r, &typed_path)?
    } else {
        json!({"status":"unavailable_incomplete_hierarchy","current_frame_verified":false,"input_authorized":false})
    };
    Ok(result)
}
