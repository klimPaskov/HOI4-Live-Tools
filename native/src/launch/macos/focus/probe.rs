//! Observe the same own-target binding used by input, without opening a panel,
//! acquiring a controller, moving the pointer, or authorizing any input.
use super::*;
use serde_json::{json, Value};

pub(crate) fn snapshot(process: &dyn Process, key: &str, include_pan: bool) -> Result<Value> {
    own_focus::validate_key(key)?;
    let start = Instant::now();
    let (_, mut value) = binding::observe(process, key, None, |r, image, own| {
        let (target, pan) = if include_pan {
            let tree = binding::symbols(r, image, own)?;
            let rectangles: Vec<_> = tree.obstacles.iter().map(|(_, rect)| *rect).collect();
            let pan = json!({
                "obstacle_count": rectangles.len(),
                "origin_available": background(&rectangles).is_some()
            });
            (tree.target, pan)
        } else {
            (binding::node(r, image, own)?, Value::Null)
        };
        let target = target.map(|control| {
            let rect = control.rect;
            json!({
                "rect": {"x": rect.x, "y": rect.y, "w": rect.w, "h": rect.h},
                "inside_safe_area": inside(&rect),
                "on_tree": on_tree(&rect)
            })
        });
        Ok(json!({
            "focus_id": key,
            "owned": true,
            "source": "macos own Focus input-binding probe",
            "target": target,
            "pan": pan,
            "client_geometry": [1920, 1080],
            "payload_read_calls": r.calls.get(),
            "payload_read_bytes": r.bytes.get(),
            "read_count_scope": "payload logical reads; excludes final witness rereads and process identity checks",
            "eligibility_available": false,
            "input_authorized": false,
            "observation_paused_game": false
        }))
    })?;
    // Includes final memory witnesses and process ownership verification.
    value["elapsed_ms"] = json!(start.elapsed().as_secs_f64() * 1000.0);
    Ok(value)
}
