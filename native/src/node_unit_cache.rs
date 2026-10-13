//! Conditional float32 cache consistency, never current-frame currency.
use crate::{
    error::{Error, Result},
    memory::Reader,
};
use serde_json::{json, Value};
fn float(r: &Reader<'_>, a: u64) -> Result<f32> {
    Ok(f32::from_le_bytes(r.read(a, 4)?.try_into().unwrap()))
}
pub(crate) fn observe(r: &Reader<'_>, leaf_to_root: &[u64]) -> Result<Value> {
    let mut rows = Vec::new();
    let mut x = 0.0f32;
    let mut y = 0.0f32;
    let mut unit = true;
    let mut all_match = true;
    let mut path_visited = true;
    for (depth, &node) in leaf_to_root.iter().rev().enumerate() {
        let local_x = float(r, node + 0x70)?;
        let local_y = float(r, node + 0x74)?;
        let scalar = float(r, node + 0x114)?;
        let visible = r.read(node + 0x144, 1)?[0] != 0;
        let descend = r.read(node + 0x128, 1)?[0] != 0;
        let count = i32::from_le_bytes(r.read(node + 0xcc, 4)?.try_into().unwrap());
        if count < 0 {
            return Err(Error::new(
                "invalid_gui_hierarchy",
                "Negative typed child count",
            ));
        }
        let pivot = r.read(node + 0x134, 1)?[0];
        let finite = local_x.is_finite() && local_y.is_finite() && scalar.is_finite();
        unit &= finite && scalar.to_bits() == 1.0f32.to_bits();
        let processed = visible || (descend && count > 0);
        path_visited &= processed;
        if depth + 1 < leaf_to_root.len() {
            path_visited &= descend && count > 0;
        }
        // Unit translation induction is conditional on this actual traversal branch.
        if unit && path_visited {
            x += local_x;
            y += local_y;
            unit &= x.is_finite() && y.is_finite();
        }
        let comparable = unit && path_visited;
        let cache = if comparable {
            let cx = float(r, node + 0xb0)?;
            let cy = float(r, node + 0xb4)?;
            let s80 = float(r, node + 0x80)?;
            let s94 = float(r, node + 0x94)?;
            let matched = cx.to_bits() == x.to_bits()
                && cy.to_bits() == y.to_bits()
                && s80.to_bits() == 1.0f32.to_bits()
                && s94.to_bits() == 1.0f32.to_bits();
            all_match &= matched;
            json!({"expected_world_x":x,"expected_world_y":y,"world_cache_finite":cx.is_finite()&&cy.is_finite(),"cache_matches_float32_induction":matched})
        } else {
            all_match = false;
            json!({"cache_comparison_available":false})
        };
        rows.push(json!({"root_to_leaf_depth":depth,"local_translation_finite":local_x.is_finite()&&local_y.is_finite(),"scalar_exact_unit":scalar.to_bits()==1.0f32.to_bits(),"scalar_finite":scalar.is_finite(),"visible_raw":visible,"descend_raw":descend,"child_count_raw":count,"pivot_branch_raw":pivot,"would_process_under_observed_flags":processed,"cache":cache}));
    }
    let target_visible = rows
        .last()
        .map(|v| v["visible_raw"] == true)
        .unwrap_or(false);
    Ok(
        json!({"status":if !unit{"unavailable_nonunit_or_nonfinite"}else if !path_visited||!target_visible{"unavailable_observed_traversal_flags"}else if !all_match{"cache_mismatch"}else{"conditional_cache_consistent"},"chain":rows,"root_incoming_scalar":1.0,"root_incoming_translation":[0.0,0.0],"cache_consistent_under_observed_unit_branch":unit&&path_visited&&target_visible&&all_match,"current_frame_verified":false,"event_ack_verified":false,"effective_routing_verified":false,"client_rectangle_available":false,"input_authorized":false}),
    )
}
