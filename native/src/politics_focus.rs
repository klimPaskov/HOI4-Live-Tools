//! Current-build own-politics focus-opener diagnostics. No input registration.
//! Country accessor, child-container registry and VOID glue are independently
//! decoded from the licensed Linux ELF; all geometry remains diagnostic only.
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    node_reuse::Reuse,
    platform::Process,
    research_ui::{array, exact, Witnesses},
};
use serde_json::{json, Value};
use std::collections::BTreeSet;

const NAME_CAP: usize = 256;
const NAME_BYTES: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct PoliticsContext {
    campaign: game::ClockContext,
    // Country, manager, registry, politics view, focus view, politics window.
    anchors: [u64; 6],
    mode_raw: i32,
}
impl PoliticsContext {
    pub fn campaign(&self) -> &game::ClockContext {
        &self.campaign
    }
    pub fn anchors(&self) -> [u64; 6] {
        self.anchors
    }
    pub fn mode_raw(&self) -> i32 {
        self.mode_raw
    }
}
fn fail(code: &'static str, message: &'static str) -> Error {
    Error::new(code, message)
}
fn pointer(p: u64) -> Result<u64> {
    if !(0x10000..=0x7fff_ffff_0000).contains(&p) || p % 8 != 0 {
        return Err(fail(
            "invalid_gui_owner",
            "Aligned canonical own UI pointer required",
        ));
    }
    Ok(p)
}
fn at(p: u64, offset: u64) -> Result<u64> {
    p.checked_add(offset)
        .ok_or_else(|| fail("read_bound", "UI address overflow"))
}
fn local_window(r: &Reader<'_>, base: u64, window: u64, registry: u64) -> Result<Value> {
    if window == 0 {
        return Ok(json!({"materialized":false,"visible_predicate":null}));
    }
    pointer(window)?;
    exact(r, window, at(base, 0x4347d58)?)?;
    if r.u64(at(window, 0x98)?)? != registry {
        return Err(fail(
            "invalid_gui_owner",
            "Own politics container registry required",
        ));
    }
    let flags = r.byte(at(window, 0xa5)?)?;
    let states = [r.i32(at(window, 0xeb0)?)?, r.i32(at(window, 0xef8)?)?];
    Ok(
        json!({"materialized":true,"flags_a5_raw":flags,"states_raw":states,
        "visible_predicate":flags&8!=0&&states.iter().all(|state|*state!=2),
        "global_modal_clear_verified":false,"effective_routing_verified":false}),
    )
}
/// Container child lookup uses BB0/BC8 (virtual1F8), NOT button9A0/9B8.
/// Missing direct membership never follows the engine's global fallback.
fn named(
    r: &Reader<'_>,
    container: u64,
    pointers: u64,
    names: u64,
    key: &str,
) -> Result<Option<u64>> {
    let values = array(r, at(container, pointers)?, NAME_CAP)?;
    let (header, strings, count) = r.array_header(at(container, names)?)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap());
    if capacity > NAME_CAP as i32 || count != values.len() || (count != 0 && strings % 8 != 0) {
        return Err(fail(
            "invalid_gui_registry",
            "Bounded paired names and pointers required",
        ));
    }
    let mut matched = None;
    for (index, value) in values.into_iter().enumerate() {
        let name = r.string_bounded(at(strings, (index as u64) * 32)?, NAME_BYTES)?;
        if name == key && matched.replace(value).is_some() {
            return Err(fail("invalid_gui_registry", "Ambiguous own named control"));
        }
    }
    Ok(matched)
}
fn callback(r: &Reader<'_>, base: u64, view: u64, button: u64) -> Result<bool> {
    if r.u64(at(button, 0x80)?)? != at(base, 0x4347488)? {
        return Ok(false);
    }
    let count = r.i32(at(button, 0x98)?)?;
    if !(0..=16).contains(&count) {
        return Err(fail(
            "invalid_gui_binding",
            "Bounded Standard listeners required",
        ));
    }
    if r.u64(at(button, 0xa0)?)? != 0 {
        return Ok(false);
    }
    let mut link = r.u64(at(button, 0x88)?)?;
    let tail = r.u64(at(button, 0x90)?)?;
    let mut prior = 0;
    let mut seen = BTreeSet::new();
    let mut target = 0;
    for _ in 0..count {
        pointer(link)?;
        if !seen.insert(link) || r.u64(at(link, 8)?)? != prior || r.byte(at(link, 0x18)?)? != 0 {
            return Err(fail(
                "invalid_gui_binding",
                "Malformed or removed Standard listener",
            ));
        }
        target = r.u64(link)?;
        prior = link;
        link = r.u64(at(link, 0x10)?)?;
    }
    if link != 0 || prior != tail || (count == 0 && tail != 0) {
        return Err(fail(
            "invalid_gui_binding",
            "Listener endpoints/count mismatch",
        ));
    }
    let glue = at(view, 0x300)?;
    if count != 1 || target != glue {
        return Ok(false);
    }
    if r.u64(glue)? != at(base, 0x4254880)?
        || r.u64(at(glue, 0x18)?)? != at(base, 0x1693160)?
        || r.u64(at(glue, 0x20)?)? != at(base, 0x1693130)?
    {
        return Ok(false);
    }
    //1691df0 initializes the whole0x280-byte std::function bundle to zero and
    //1685a30 supplies only FIRST VOID. Non-default inactive bytes are unknown,
    //never additional callbacks whose captures we interpret speculatively.
    if r.read(at(glue, 0x28)?, 0x260)?
        .iter()
        .any(|byte| *byte != 0)
    {
        return Ok(false);
    }
    let capture = pointer(r.u64(at(glue, 8)?)?)?;
    if r.u64(at(capture, 0x10)?)? != view {
        return Err(fail(
            "invalid_gui_owner",
            "Focus opener must capture exact own politics view",
        ));
    }
    Ok(r.u64(capture)? == at(base, 0x1686aa0)? && r.u64(at(capture, 8)?)? == 0)
}
fn diagnostics(
    r: &Reader<'_>,
    base: u64,
    view: u64,
    registry: u64,
    window: u64,
    mode: i32,
    geometry: bool,
) -> Result<Value> {
    let local = local_window(r, base, window, registry)?;
    let mut out = json!({"available":false,"reason":"politics_window_unavailable",
        "active_mode_raw":mode,"window":local,"target_managed_mode":13,
        "callback_route_verified":false,"input_authorized":false,
        "client_rectangle_available":false,"global_modal_clear_verified":false,
        "effective_routing_verified":false,"current_frame_verified":false});
    if mode != 12 || local["visible_predicate"] != true {
        return Ok(out);
    }
    let Some(container) = named(r, window, 0xbb0, 0xbc8, "active_goal")? else {
        out["reason"] = json!("named_container_unavailable");
        return Ok(out);
    };
    if container == 0 {
        out["reason"] = json!("container_unmaterialized");
        return Ok(out);
    }
    let container_state = local_window(r, base, container, registry)?;
    out["active_goal"] = container_state.clone();
    if container_state["visible_predicate"] != true {
        out["reason"] = json!("active_goal_not_locally_visible");
        return Ok(out);
    }
    let Some(button) = named(r, container, 0x9a0, 0x9b8, "add_national_goal_button")? else {
        out["reason"] = json!("named_button_unavailable");
        return Ok(out);
    };
    if button == 0 {
        out["reason"] = json!("button_unmaterialized");
        return Ok(out);
    }
    pointer(button)?;
    let mut reuse = Reuse::new(base, 0);
    out["control_type_encoding"] = match reuse.nullable_type(r, button) {
        Ok(value) => value,
        Err(error) if error.code == "invalid_gui_type" => Value::Null,
        Err(error) => return Err(error),
    };
    if r.u64(button)? != at(base, 0x43470c8)? {
        out["reason"] = json!("standard_layout_unavailable");
        return Ok(out);
    }
    if r.u64(at(button, 0x68)?)? != registry {
        return Err(fail(
            "invalid_gui_owner",
            "Standard opener must belong to own registry",
        ));
    }
    if !callback(r, base, view, button)? {
        out["reason"] = json!("callback_binding_unavailable");
        return Ok(out);
    }
    out["available"] = json!(true);
    out["reason"] = json!("own_named_callback_metadata");
    out["callback_route_verified"] = json!(true);
    out["disabled_raw"] = json!(r.i32(at(button, 0xac)?)?);
    out["flags_raw"] = json!(u32::from_le_bytes(
        r.read(at(button, 0x74)?, 4)?.try_into().unwrap()
    ));
    if geometry {
        out["render_diagnostic"] = crate::node_detail::standard_sprite_diagnostic(
            r, &mut reuse, container, button, registry,
        )?;
    }
    Ok(out)
}
/// Separate explicit internal observer. No default public page/tool changes.
#[doc(hidden)]
pub fn owned_snapshot(
    process: &dyn Process,
    include_geometry: bool,
) -> Result<(Value, PoliticsContext)> {
    process.begin_snapshot()?;
    let w = Witnesses::new(process);
    let r = Reader::new(&w);
    let base = process.image_base();
    let gs = pointer(r.u64(at(base, 0x44086a8)?)?)?;
    let idler = pointer(r.u64(at(base, 0x44085f8)?)?)?;
    exact(&r, gs, at(base, 0x42c7598)?)?;
    exact(&r, idler, at(base, 0x42c9a88)?)?;
    let tag = r.i32(at(gs, 0x518)?)?;
    if !(1..4096).contains(&tag) {
        return Err(fail(
            "player_unavailable",
            "Positive own tag required before UI/country payload",
        ));
    }
    //20811a0 (idler virtualC0) returns GS518 exactly when positive. The active
    //goal updater16886b0 calls it then d77320 before any container payload.
    if r.u64(at(base, 0x42c9a88 + 0xc0)?)? != at(base, 0x20811a0)? {
        return Err(fail(
            "invalid_gui_owner",
            "Exact own-country accessor required",
        ));
    }
    let index = i32::from_le_bytes(
        r.array_entry(at(gs, 0x340)?, tag as usize, 4)?
            .try_into()
            .unwrap(),
    );
    if index < 0 {
        return Err(fail("player_unavailable", "Own country index unavailable"));
    }
    let country = pointer(u64::from_le_bytes(
        r.array_entry(at(gs, 0x310)?, index as usize, 8)?
            .try_into()
            .unwrap(),
    ))?;
    r.object_type(country, "CCountry")?;
    if r.i32(at(country, 8)?)? != tag {
        return Err(fail(
            "invalid_gui_owner",
            "Canonical own country/tag required",
        ));
    }
    let registry = pointer(r.u64(at(idler, 0x4f8)?)?)?;
    exact(&r, registry, at(base, 0x42736f8)?)?;
    let manager = pointer(r.u64(at(idler, 0x6b8)?)?)?;
    if r.u64(at(manager, 0x180)?)? != idler {
        return Err(fail(
            "invalid_gui_owner",
            "Own manager/idler reciprocal join required",
        ));
    }
    let view = pointer(r.u64(at(manager, 0x128)?)?)?;
    exact(&r, view, at(base, 0x42546e8)?)?;
    if r.u64(at(view, 0x2f0)?)? != idler || r.u64(at(view, 0x38)?)? != manager {
        return Err(fail(
            "invalid_gui_owner",
            "Exact politics idler and manager required",
        ));
    }
    let focus = pointer(r.u64(at(manager, 0x130)?)?)?;
    exact(&r, focus, at(base, 0x4249c70)?)?;
    let displayed = r.u64(at(focus, 0x11d8)?)?;
    if r.u64(at(focus, 0x2f0)?)? != idler || (displayed != 0 && displayed != country) {
        return Err(fail(
            "foreign_focus_scope",
            "Opener target must already resolve own country",
        ));
    }
    //Same Witnesses/Reader; no nested snapshot, no context read from cached state.
    let campaign = game::context_from_reader(&r, base)?;
    if campaign.anchors[..2] != [gs, idler] {
        return Err(fail("torn_read", "Politics and campaign roots differ"));
    }
    let mode = r.i32(at(manager, 0x3d8)?)?;
    let window = r.u64(at(view, 0x2f8)?)?;
    let mut value = diagnostics(&r, base, view, registry, window, mode, include_geometry)?;
    value["country_binding_verified"] = json!(true);
    value["country_binding_meaning"] =
        json!("current_build_active_goal_updater_uses_positive_own_idler_accessor");
    let (reads, bytes) = w.finish()?;
    process.check()?;
    value["metrics"] = json!({"reads":reads,"bytes":bytes});
    Ok((
        value,
        PoliticsContext {
            campaign,
            anchors: [country, manager, registry, view, focus, window],
            mode_raw: mode,
        },
    ))
}
