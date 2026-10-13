//! Fresh internal observation adapter. No MCP input registration or delivery.
//! OS ownership and the corroborated client-origin profile belong to the owner;
//! neither can be inferred from a diagnostic's false authority fields.
use crate::{
    error::{Error, Result},
    game, gui,
    platform::Process,
    research_attempt::{
        self, ClientCodec, Common, Hierarchy, PointCandidate, SpriteNode, Surface, TextResearch,
    },
    research_ui,
};
use serde_json::Value;

#[derive(Clone, Copy, Debug)]
pub enum Stage {
    View,
    Research,
}
/// Supplied by the immutable owned input binding, never player tool arguments.
pub struct OwnedClientProfile {
    pub window: u32,
    pub client: [u16; 2],
    pub logical: [u16; 2],
    pub os_binding_verified: bool,
    pub reviewed_builtin_client_origin: bool,
}
fn unavailable(message: &str) -> Error {
    Error::new("research_geometry_unavailable", message)
}
fn required(value: &Value, expected: bool, message: &str) -> Result<()> {
    if value.as_bool() == Some(expected) {
        Ok(())
    } else {
        Err(unavailable(message))
    }
}
fn int(value: &Value) -> Result<i32> {
    value
        .as_i64()
        .and_then(|v| i32::try_from(v).ok())
        .ok_or_else(|| unavailable("Signed diagnostic integer required"))
}
fn uint(value: &Value) -> Result<u32> {
    value
        .as_u64()
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| unavailable("Unsigned diagnostic integer required"))
}
fn float(value: &Value) -> Result<f32> {
    let value = value
        .as_f64()
        .ok_or_else(|| unavailable("Finite diagnostic float required"))?;
    let narrowed = value as f32;
    if !narrowed.is_finite() {
        return Err(unavailable("Finite diagnostic float required"));
    }
    Ok(narrowed)
}
fn pair<T>(value: &Value, parse: impl Fn(&Value) -> Result<T>) -> Result<[T; 2]> {
    let values = value
        .as_array()
        .filter(|v| v.len() == 2)
        .ok_or_else(|| unavailable("Exactly two diagnostic values required"))?;
    Ok([parse(&values[0])?, parse(&values[1])?])
}
fn size(value: &Value) -> Result<[u16; 2]> {
    pair(value, |v| {
        u16::try_from(uint(v)?).map_err(|_| unavailable("Bounded client size required"))
    })
}
fn common(
    raw: &Value,
    world: &str,
    anchor: &str,
    scalar: &str,
    hierarchy: &Value,
) -> Result<Common> {
    required(
        &hierarchy["graphics_root_closed"],
        true,
        "Owned graphics root closure required",
    )?;
    if hierarchy["status"] != "closed_to_owned_graphics_root" {
        return Err(unavailable("Typed owned root hierarchy required"));
    }
    required(
        &hierarchy["unit_cache_diagnostic"]["cache_consistent_under_observed_unit_branch"],
        true,
        "Conditional unit cache required",
    )?;
    Ok(Common {
        world: pair(&raw[world], float)?,
        anchor: pair(&raw[anchor], uint)?,
        scalar: float(&raw[scalar])?,
        hierarchy: Hierarchy {
            closed_to_owned_root_1f0: true,
            conditional_unit_cache_consistent: true,
        },
    })
}
pub(crate) fn codec(geometry: &Value, profile: &OwnedClientProfile) -> Result<ClientCodec> {
    if geometry["build_id"] != game::BUILD_ID {
        return Err(unavailable("Verified game build required"));
    }
    let source = &geometry["input_source"]["point_diagnostic"];
    required(
        &source["getter_source_layout_verified"],
        true,
        "Builtin point source required",
    )?;
    let association = &source["window_association"];
    required(
        &association["association_available"],
        true,
        "Reciprocal source window required",
    )?;
    required(
        &association["normal_absolute_mode"],
        true,
        "Absolute input mode required",
    )?;
    required(
        &association["graphics_dimensions_match"],
        true,
        "Source graphics size match required",
    )?;
    let physical = size(&geometry["physical_dimensions"])?;
    if size(&geometry["logical_dimensions"])? != profile.logical {
        return Err(unavailable("Owner-reviewed logical profile differs"));
    }
    if size(&association["window_dimensions"])? != physical {
        return Err(unavailable("Input window size differs"));
    }
    Ok(ClientCodec {
        owned_x11_window: profile.window,
        observed_sdl_window: u64::from(uint(&association["native_window_id"])?),
        owned_os_binding_verified: profile.os_binding_verified,
        builtin_point_source_verified: true,
        reciprocal_x11_association_verified: true,
        relative_raw: pair(&association["relative_mode_raw"], int)?,
        client: profile.client,
        graphics_physical: physical,
        graphics_logical: size(&geometry["logical_dimensions"])?,
        inverse: pair(&geometry["logical_per_physical"], float)?,
        forward: pair(&geometry["physical_per_logical"], float)?,
        reviewed_builtin_client_origin: profile.reviewed_builtin_client_origin,
    })
}
pub(crate) fn sprite_surface(render: &Value) -> Result<Surface> {
    required(
        &render["field_layout_verified"],
        true,
        "Exact Sprite fields required",
    )?;
    if render["render_type_encoding"] != "7CSprite"
        || render["resource_type_encoding"] != "11CSpriteType"
    {
        return Err(unavailable("Exact Sprite renderer/resource required"));
    }
    let raw = &render["raw"];
    if uint(&raw["visibility_flag"])? != 1 {
        return Err(unavailable("Locally visible node required"));
    }
    let cache = &render["resource_cache"];
    required(
        &cache["cache_equivalent"],
        true,
        "Sprite resource cache required",
    )?;
    required(
        &cache["no_mask_rejection_branch"],
        true,
        "Unmasked Sprite branch required",
    )?;
    let clip = raw["physical_scaled_clip"]
        .as_array()
        .filter(|v| v.len() == 4)
        .ok_or_else(|| unavailable("Physical clip required"))?;
    Ok(Surface::ExactSpriteNode(SpriteNode {
        common: common(
            raw,
            "cached_world_translation",
            "pivot_dimensions",
            "local_scalar",
            &render["parent_chain"],
        )?,
        texture: pair(&cache["texture_dimensions"], int)?,
        grid: pair(&cache["grid"], int)?,
        orientation: uint(&cache["orientation_raw"])?,
        orientation_full_width: true,
        cache_equivalent: true,
        no_mask_rejection_branch: true,
        physical_clip: Some([
            int(&clip[0])?,
            int(&clip[1])?,
            int(&clip[2])?,
            int(&clip[3])?,
        ]),
    }))
}
fn slot_surface(detail: &Value, slot: u32) -> Result<Surface> {
    if detail["owned"] != true
        || detail["build_id"] != game::BUILD_ID
        || detail["slot"].as_u64() != Some(u64::from(slot))
        || detail["descriptor"]["slot"].as_u64() != Some(u64::from(slot))
        || detail["active_mode_raw"] != 5
        || detail["panel"]["materialized"] != true
        || detail["panel"]["visible_predicate"] != true
        || !matches!(
            (
                detail["ordinary_tree"]["materialized"].as_bool(),
                detail["ordinary_tree"]["visible_predicate"].as_bool()
            ),
            (Some(true), Some(false)) | (Some(false), None)
        )
    {
        return Err(unavailable(
            "Fresh own ordinary research panel and requested slot required",
        ));
    }
    let binding = &detail["descriptor"]["ordinary_selection_binding"];
    required(
        &binding["binding_verified"],
        true,
        "Exact own slot callback binding required",
    )?;
    sprite_surface(&binding["render_diagnostic"])
}
fn surface(detail: &Value, key: &str, slot: u32, stage: Stage) -> Result<Surface> {
    required(&detail["owned"], true, "Own technology detail required")?;
    if detail["build_id"] != game::BUILD_ID
        || detail["id"] != key
        || uint(&detail["selected_slot"])? != slot
    {
        return Err(unavailable(
            "Fresh requested own key and selected slot required",
        ));
    }
    let info = &detail["own_info_postcondition"];
    match stage {
        Stage::View => {
            if uint(&info["matching_locally_visible_count"])? != 0
                || detail["detail"]["id"] != key
                || uint(&detail["detail"]["distinct_bound_items"])? != 1
            {
                return Err(unavailable(
                    "Unique bound own node and closed requested Info required",
                ));
            }
            let render = &detail["detail"]["render_diagnostic"];
            sprite_surface(render)
        }
        Stage::Research => {
            if uint(&info["matching_locally_visible_count"])? != 1 {
                return Err(unavailable("Unique visible own requested Info required"));
            }
            let button = &info["ordinary_research_button"];
            for field in [
                "control_layout_verified",
                "ordinary_alias_matches",
                "callback_binding_verified",
                "renderer_control_join_verified",
            ] {
                required(
                    &button[field],
                    true,
                    "Exact ordinary own Research binding required",
                )?;
            }
            if int(&button["disabled_raw"])? != 0
                || uint(&button["flags_raw"])? & (1 << 12) != 0
                || button["renderer_type_encoding"] != "11CTextSprite"
            {
                return Err(unavailable(
                    "Supported enabled Text Research branch required",
                ));
            }
            let raw = &button["text_raw_diagnostic"];
            required(
                &raw["resource_layout_verified"],
                true,
                "Exact Text resource required",
            )?;
            required(
                &raw["raw_fields_available"],
                true,
                "Bounded Text fields required",
            )?;
            Ok(Surface::ExactZeroAngleTextResearch(TextResearch {
                common: common(
                    raw,
                    "cache_translation_raw",
                    "anchor_raw",
                    "scalar_raw",
                    &raw["parent_chain"],
                )?,
                width: int(&raw["renderer_width_raw"])?,
                height: int(&raw["renderer_height_raw"])?,
                resource_count: uint(&raw["resource_count_raw"])?,
                hit_divisor: int(&raw["hit_width_divisor_raw"])?,
                angle: float(&raw["transform_110_raw"])?,
            }))
        }
    }
}
/// Each call obtains its own bounded, witnessed process observations. It does
/// not claim atomicity between snapshots, frame acknowledgement or routing.
/// The command owner must revalidate campaign/lease and OS geometry before input.
pub fn observe(
    process: &dyn Process,
    key: &str,
    slot: u32,
    stage: Stage,
    profile: &OwnedClientProfile,
) -> Result<PointCandidate> {
    research_ui::validate_detail_key(key)?;
    let detail = research_ui::node_detail(process, key)?;
    let surface = surface(&detail, key, slot, stage)?;
    let geometry = gui::input_geometry(process)?;
    let codec = codec(&geometry, profile)?;
    process.check()?;
    research_attempt::point_candidate(&surface, &codec)
}

#[cfg(test)]
mod tests;

#[doc(hidden)]
pub fn observe_owned(
    process: &dyn Process,
    key: &str,
    slot: u32,
    stage: Stage,
    profile: &OwnedClientProfile,
) -> Result<(PointCandidate, research_ui::UiContext)> {
    let (detail, context) = research_ui::node_detail_owned(process, key)?;
    let surface = surface(&detail, key, slot, stage)?;
    let geometry = gui::input_geometry(process)?;
    let codec = codec(&geometry, profile)?;
    process.check()?;
    Ok((
        research_attempt::point_candidate(&surface, &codec)?,
        context,
    ))
}

/// A prepared slot point retains all diagnostic authority limits. The owner
/// additionally pins canonical emptiness and revalidates before ordinary input.
#[doc(hidden)]
pub fn observe_slot_owned(
    process: &dyn Process,
    slot: u32,
    profile: &OwnedClientProfile,
) -> Result<(PointCandidate, research_ui::UiContext)> {
    let (detail, context) = research_ui::slot_control_owned(process, slot as usize)?;
    let surface = slot_surface(&detail, slot)?;
    let geometry = gui::input_geometry(process)?;
    let codec = codec(&geometry, profile)?;
    process.check()?;
    Ok((
        research_attempt::point_candidate(&surface, &codec)?,
        context,
    ))
}

/// Category navigation candidate only. Owner must revalidate lease/campaign and
/// OS/profile before any separate normal action; no dispatch exists here.
fn category_surface(detail: &Value, key: &str, slot: u32) -> Result<Surface> {
    if detail["owned"] != true
        || detail["build_id"] != game::BUILD_ID
        || detail["selected_slot"].as_u64() != Some(u64::from(slot))
    {
        return Err(unavailable("Fresh selected own category slot required"));
    }
    let category = &detail["category"];
    if category["folder_key"] != key
        || category["binding_available"] != true
        || category["selected_slot_empty"] != true
    {
        return Err(unavailable(
            "Exact requested category binding and own empty slot required",
        ));
    }
    match category["control_type_encoding"].as_str() {
        Some("11CButtonDrag") => {}
        Some("15CButtonStandard") => {
            let state = int(&category["standard_state_raw"])?;
            if !(0..=2).contains(&state) || category["standard_disabled_predicate"] != false {
                return Err(unavailable(
                    "Standard category disabled/state branch unavailable",
                ));
            }
        }
        _ => return Err(unavailable("Exact category control layout required")),
    }
    let flags = uint(&category["raw_flags"])?;
    // Local active/hide-suppression branches only, never topmost/eligibility.
    if flags & (1 << 11) == 0 || flags & (1 << 12) != 0 {
        return Err(unavailable("Category local active/hide branch unavailable"));
    }
    sprite_surface(&category["sprite_diagnostic"])
}
#[doc(hidden)]
pub fn observe_category_owned(
    process: &dyn Process,
    key: &str,
    slot: u32,
    profile: &OwnedClientProfile,
) -> Result<(PointCandidate, research_ui::UiContext)> {
    research_ui::validate_detail_key(key)?;
    let (detail, context) = research_ui::category_with_geometry_owned(process, key, true)?;
    let surface = category_surface(&detail, key, slot)?;
    if slot as usize >= context.slots.len() {
        return Err(unavailable("Selected category slot context bound"));
    }
    let geometry = gui::input_geometry(process)?;
    let codec = codec(&geometry, profile)?;
    process.check()?;
    Ok((
        research_attempt::point_candidate(&surface, &codec)?,
        context,
    ))
}
