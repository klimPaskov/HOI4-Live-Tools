//! Exact-build mouse-cache correlation, never an input admission predicate.
//! The processed cursor stamp precedes the rebuild; a coherent witness bracket
//! cannot turn it into a generation/commit marker. No engine functions run.
use super::{exact, invalid, pointer, Reader, Rect, Result, Value};
use serde_json::json;

// Observer policy bounds, not claimed engine limits. Never truncate a route.
const MAX_ENTRIES: usize = 4096;
const MAX_COORDINATE: f32 = 1_000_000.0;

#[derive(Clone, Debug, PartialEq)]
pub(in crate::construction) struct Observation {
    sources: Vec<u64>,
    targets: Vec<u64>,
    renderer: u64,
    button: u64,
    current: [f32; 2],
    processed: [f32; 2],
    to_graphics: [f32; 2],
    to_event: [f32; 2],
    target_point: [i32; 2],
    expected_stamp: [f32; 2],
    processed_event_point: [i32; 2],
    aliases_match: [bool; 2],
    dispatch_skip: u8,
    conditional_enable: u8,
    flags: u32,
    wrapper_gate: u8,
    optional_gate_present: bool,
    post_click_hook_present: bool,
}

// Reader records both headers and complete payloads in the parent's final
// witness. The allocator is not an input-routing semantic field.
fn entries(r: &Reader<'_>, address: u64, stride: usize) -> Result<Vec<u8>> {
    // Reader validates the entire header, including reserved capacity <=20000.
    // Only active members are routing facts; never read spare allocation bytes.
    let (_, data, count) = r.array_header(address)?;
    if count > MAX_ENTRIES {
        return Err(invalid(
            "Mac mouse route exceeds the complete observation bound",
        ));
    }
    if data != 0 {
        pointer(data)?;
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    let size = count
        .checked_mul(stride)
        .ok_or_else(|| invalid("Mac route size overflow"))?;
    let end = data
        .checked_add(size as u64)
        .ok_or_else(|| invalid("Mac route address overflow"))?;
    if end > 0x7fff_ffff_0000 {
        return Err(invalid("Mac route exceeds bounded process addresses"));
    }
    r.read(data, size)
}

fn pair(r: &Reader<'_>, address: u64, factors: bool) -> Result<[f32; 2]> {
    let bytes = r.read(address, 8)?;
    let values = [
        f32::from_le_bytes(bytes[..4].try_into().unwrap()),
        f32::from_le_bytes(bytes[4..].try_into().unwrap()),
    ];
    if values.iter().any(|v| {
        !v.is_finite()
            || if factors {
                !(0.01..=100.0).contains(v)
            } else {
                v.abs() > MAX_COORDINATE
            }
    }) {
        return Err(invalid(
            "Finite bounded Mac mouse coordinates and conversion factors required",
        ));
    }
    Ok(values)
}

fn convert(point: [f32; 2], factors: [f32; 2]) -> Result<[i32; 2]> {
    let values = [point[0] * factors[0], point[1] * factors[1]];
    if values
        .iter()
        .any(|v| !v.is_finite() || v.abs() > MAX_COORDINATE)
    {
        return Err(invalid(
            "Mac mouse conversion exceeds the observation bound",
        ));
    }
    // The engine uses cvttss2si (truncate toward zero), not nearest rounding.
    Ok([values[0].trunc() as i32, values[1].trunc() as i32])
}

pub(in crate::construction) fn observe(
    r: &Reader<'_>,
    base: u64,
    gui: u64,
    graphics: u64,
    button: u64,
    rect: Rect,
) -> Result<Observation> {
    // CButtonStandard ctor retains the own GUI graphics at +d0, then links
    // its +d8 renderer directly back to this primary via renderer+138.
    if r.u64(button + 0xd0)? != graphics {
        return Err(invalid(
            "Mac opener and GUI must retain the same graphics owner",
        ));
    }
    let renderer = r.u64(button + 0xd8)?;
    pointer(renderer)?;
    let primary = r.u64(renderer)?;
    if ![base + 0x34bdb10, base + 0x34afed0].contains(&primary) {
        return Err(invalid(
            "Exact Mac opener sprite required for route correlation",
        ));
    }
    if r.u64(renderer + 0x138)? != button {
        return Err(invalid(
            "Mac opener renderer must target the same named button",
        ));
    }

    let sources = entries(r, graphics + 0x50, 8)?
        .chunks_exact(8)
        .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
        .collect();
    let route = entries(r, graphics + 0x28, 16)?;
    let mut targets = Vec::with_capacity(route.len() / 16);
    for entry in route.chunks_exact(16) {
        // Persisted CCollisionObject entries differ from the temporary
        // C2dCollisionObject used while assembling the scratch array.
        if u64::from_le_bytes(entry[..8].try_into().unwrap()) != base + 0x34b0ab8 {
            return Err(invalid("Exact Mac collision-route entry type required"));
        }
        targets.push(u64::from_le_bytes(entry[8..].try_into().unwrap()));
    }
    // Never follow other source nodes or route targets, including a competing
    // first target. Their membership still participates in the full witness.
    let current = pair(r, graphics + 0x40, false)?;
    let processed = pair(r, graphics + 0x48, false)?;
    let to_graphics = pair(r, graphics + 0x1c0, true)?;
    let to_event = pair(r, graphics + 0x1c8, true)?;
    let target_point = [rect.x + rect.w / 2, rect.y + rect.h / 2];
    let stamp = convert(target_point.map(|v| v as f32), to_graphics)?;
    let processed_event_point = convert(processed, to_event)?;
    let flags = u32::from_le_bytes(r.read(button + 0x64, 4)?.try_into().unwrap());
    // The parent already binds button+58 to this exact own registry. Do not
    // dereference unknown optional Standard or global post-click handlers.
    exact(r, base, gui, 0x34974d0)?;
    Ok(Observation {
        sources,
        targets,
        renderer,
        button,
        current,
        processed,
        to_graphics,
        to_event,
        target_point,
        expected_stamp: stamp.map(|v| v as f32),
        processed_event_point,
        aliases_match: [r.u64(gui + 0x120)? == button, r.u64(gui + 0x128)? == button],
        dispatch_skip: r.byte(gui + 0x130)?,
        conditional_enable: r.byte(gui + 0x12)?,
        flags,
        wrapper_gate: r.byte(button + 0x1d1)?,
        optional_gate_present: r.u64(button + 0x178)? != 0,
        post_click_hook_present: r.u64(base + 0x35dbf60)? != 0,
    })
}

impl Observation {
    /// Narrow ordinary-input correlation. This does not assert an atomic cache
    /// generation or physical delivery; same-attempt game readback is required.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(in crate::construction) fn require_ordinary_target(
        &self,
        parent_verified: bool,
    ) -> Result<()> {
        if self.sources.iter().filter(|p| **p == self.renderer).count() != 1
            || self.targets.iter().filter(|p| **p == self.button).count() != 1
            || self.targets.first() != Some(&self.button)
            || self.aliases_match != [true, true]
            || self.current != self.expected_stamp
            || self.processed != self.expected_stamp
            || self.processed_event_point != self.target_point
            || self.dispatch_skip != 0
            || (self.conditional_enable != 0 && self.flags & 2 == 0)
            || self.flags & 0xc00 != 0x800
            || self.flags & 0x2000 == 0
            || self.wrapper_gate != 0
            || (self.optional_gate_present && !parent_verified)
            || self.post_click_hook_present
        {
            return Err(crate::error::Error::new(
                "construction_route_unavailable",
                "Current cursor and complete hit route must correlate with the original requested button",
            ));
        }
        Ok(())
    }
    pub(super) fn diagnostic(&self) -> Value {
        self.diagnostic_with_parent(false)
    }
    /// The construction-item observer independently verifies a nonnull
    /// graphical parent. Opener callers retain their original unknown gate.
    pub(in crate::construction) fn diagnostic_with_parent(&self, verified: bool) -> Value {
        let source_count = self.sources.iter().filter(|p| **p == self.renderer).count();
        let target_count = self.targets.iter().filter(|p| **p == self.button).count();
        json!({
            "source_count":self.sources.len(),"route_count":self.targets.len(),
            "complete_membership_observed":true,"renderer_target_join_verified":true,
            "route_entry_types_verified":true,"selected_source_occurrences":source_count,
            "selected_target_occurrences":target_count,
            "source_tail_matches_renderer":self.sources.last()==Some(&self.renderer),
            "first_route_target_matches_button":self.targets.first()==Some(&self.button),
            "gui_route_aliases_match_button":self.aliases_match,
            "target_client_point":self.target_point,"cached_current_graphics_point":self.current,
            "cached_processed_graphics_point":self.processed,"input_to_graphics_factors":self.to_graphics,
            "graphics_to_event_factors":self.to_event,"expected_graphics_stamp":self.expected_stamp,
            "processed_event_point":self.processed_event_point,
            "cached_cursor_matches_target":self.current==self.expected_stamp
                && self.processed==self.expected_stamp && self.processed_event_point==self.target_point,
            "dispatch_skip_raw":self.dispatch_skip,"conditional_enable_raw":self.conditional_enable,
            "source_target_flags_eligible":self.flags&0xc00==0x800,
            "conditional_target_gate_clear":self.conditional_enable==0 || self.flags&2!=0,
            "standard_wrapper_gate_raw":self.wrapper_gate,
            "unsupported_optional_gate_present":self.optional_gate_present && !verified,
            "unsupported_post_click_hook_present":self.post_click_hook_present,
            "cache_completion_verified":false,"os_cursor_verified":false,
            "input_authorized":false,"delivery_verified":false
        })
    }
}

#[cfg(test)]
mod admission_tests {
    use super::*;
    fn ready() -> Observation {
        Observation {
            sources: vec![11, 13],
            targets: vec![12, 14],
            renderer: 11,
            button: 12,
            current: [100.0, 200.0],
            processed: [100.0, 200.0],
            to_graphics: [1.0, 1.0],
            to_event: [1.0, 1.0],
            target_point: [100, 200],
            expected_stamp: [100.0, 200.0],
            processed_event_point: [100, 200],
            aliases_match: [true, true],
            dispatch_skip: 0,
            conditional_enable: 0,
            flags: 0x2800,
            wrapper_gate: 0,
            optional_gate_present: true,
            post_click_hook_present: false,
        }
    }
    #[test]
    fn target_correlation_does_not_require_source_tail_or_claim_delivery() {
        let observation = ready();
        observation.require_ordinary_target(true).unwrap();
        let d = observation.diagnostic_with_parent(true);
        assert_eq!(d["source_tail_matches_renderer"], false);
        for key in [
            "cache_completion_verified",
            "os_cursor_verified",
            "input_authorized",
            "delivery_verified",
        ] {
            assert_eq!(d[key], false);
        }
        assert!(observation.require_ordinary_target(false).is_err());
    }
    #[test]
    fn stale_ambiguous_or_intercepted_routes_refuse_without_replacing_the_target() {
        let changes: [fn(&mut Observation); 16] = [
            |o| o.sources.clear(),
            |o| o.sources.push(o.renderer),
            |o| o.targets.clear(),
            |o| o.targets.push(o.button),
            |o| o.targets.swap(0, 1),
            |o| o.aliases_match[0] = false,
            |o| o.aliases_match[1] = false,
            |o| o.current[0] += 1.0,
            |o| o.processed[0] += 1.0,
            |o| o.processed_event_point[1] += 1,
            |o| o.dispatch_skip = 1,
            |o| o.conditional_enable = 1,
            |o| o.flags &= !0x2000,
            |o| o.flags |= 0x400,
            |o| o.wrapper_gate = 1,
            |o| o.post_click_hook_present = true,
        ];
        for change in changes {
            let mut o = ready();
            change(&mut o);
            assert_eq!(
                o.require_ordinary_target(true).unwrap_err().code,
                "construction_route_unavailable"
            );
        }
    }
    #[test]
    fn supported_conditional_flag_and_absent_graphical_parent_remain_valid() {
        let mut o = ready();
        o.conditional_enable = 1;
        o.flags |= 2;
        o.optional_gate_present = false;
        o.require_ordinary_target(false).unwrap();
    }
}
