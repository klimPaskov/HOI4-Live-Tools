//! Independently recovered current Linux own Army list/handle/location getters.
//! Consulted smerss/Clausewitz-MCP calling/{game,combat,unitorders}.py, MIT;
//! ../LICENSE.clausewitz. No native calls or action eligibility.
use crate::{
    error::{Error, Result},
    game,
    memory::Reader,
    platform::Process,
    research_ui::Witnesses,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    time::{Instant, SystemTime, UNIX_EPOCH},
};
/// Division-list layout of one inspected build.
struct Layout {
    country_divisions: u64,
    division_vtable: u64,
    strength: u64,
    organisation: u64,
    province_id: u64,
}
/// Linux ELF a4971c9e (libstdc++).
const LINUX: Layout = Layout {
    country_divisions: 0x290,
    division_vtable: 0x4277fe8,
    strength: 0x420,
    organisation: 0x428,
    province_id: 0xa4,
};
/// macOS Mach-O 089EA519 (libc++), mapped live on 18 October 2026: the list of
/// 30 CArmy divisions and the field offsets agree with aligned methods.
const MACOS: Layout = Layout {
    country_divisions: 0x250,
    division_vtable: 0x32af5a8,
    strength: 0x418,
    organisation: 0x420,
    province_id: 0x9c,
};
fn layout(r: &Reader<'_>) -> &'static Layout {
    match r.string_abi() {
        crate::memory::StringAbi::LibStdCxx => &LINUX,
        crate::memory::StringAbi::LibCxx => &MACOS,
    }
}
fn exact(r: &Reader<'_>, p: u64, v: u64) -> Result<()> {
    if p % 8 != 0 || r.u64(p)? != v {
        return Err(Error::new(
            "invalid_army_type",
            "Exact own army primary required",
        ));
    }
    Ok(())
}
fn entry(r: &Reader<'_>, a: u64, index: usize, stride: usize, bound: i32) -> Result<Vec<u8>> {
    let (h, d, _) = r.array_header(a)?;
    let cap = i32::from_le_bytes(h[8..12].try_into().unwrap());
    if cap > bound || d % stride as u64 != 0 {
        return Err(Error::new(
            "invalid_array",
            "Own array policy/alignment required",
        ));
    }
    r.array_entry(a, index, stride)
}
pub fn snapshot(process: &dyn Process) -> Result<Value> {
    let start = Instant::now();
    process.begin_snapshot()?;
    let identity = process.identity().ok_or_else(|| {
        Error::new(
            "identity_unavailable",
            "Bound owned process identity required",
        )
    })?;
    // Linux start ticks are integer clock ticks; macOS start times are
    // fractional seconds. Either must be a positive number.
    if identity.pid <= 0
        || identity
            .start_ticks
            .parse::<f64>()
            .ok()
            .filter(|n| *n > 0.0)
            .is_none()
    {
        return Err(Error::new(
            "identity_unavailable",
            "Valid process creation identity required",
        ));
    }
    let w = Witnesses::new(process);
    let r = Reader::new(&w);
    let base = process.image_base();
    let g = game::layout(&r);
    let l = layout(&r);
    let provinces = crate::construction::layout(&r).gs_provinces;
    let gs = r.u64(base + g.gs_global)?;
    exact(&r, gs, base + g.gs_vtable)?;
    let idler = r.u64(base + g.idler_global)?;
    exact(&r, idler, base + g.idler_vtable)?;
    let tag = r.i32(gs + g.player_tag)?;
    if !(1..4096).contains(&tag) {
        return Err(Error::new(
            "player_unavailable",
            "Positive own country tag required",
        ));
    }
    let index = i32::from_le_bytes(
        entry(&r, gs + g.tag_country_index, tag as usize, 4, 4096)?
            .try_into()
            .unwrap(),
    );
    if index < 0 {
        return Err(Error::new("player_unavailable", "Own country absent"));
    }
    let country = u64::from_le_bytes(
        entry(&r, gs + g.countries, index as usize, 8, 4096)?
            .try_into()
            .unwrap(),
    );
    exact(&r, country, base + g.country_vtable)?;
    if r.i32(country + 8)? != tag {
        return Err(Error::new("invalid_army_owner", "Own country tag required"));
    }
    let context = game::context_from_reader(&r, base)?;
    let (h, data, count) = r.array_header(country + l.country_divisions)?;
    let cap = i32::from_le_bytes(h[8..12].try_into().unwrap());
    if cap > 4096 || data % 8 != 0 {
        return Err(Error::new(
            "invalid_array",
            "Own army list exceeds policy/alignment",
        ));
    }
    let payload = if count == 0 {
        Vec::new()
    } else {
        r.read(data, count * 8)?
    };
    let mut pointers = BTreeSet::new();
    let mut handles = BTreeSet::new();
    let mut output = Vec::new();
    for bytes in payload.chunks_exact(8) {
        let army = u64::from_le_bytes(bytes.try_into().unwrap());
        if !pointers.insert(army) {
            return Err(Error::new(
                "ambiguous_army_identity",
                "Duplicate own list pointer",
            ));
        }
        exact(&r, army, base + l.division_vtable)?;
        if r.i32(army + 0x1d8)? != tag {
            return Err(Error::new(
                "invalid_army_owner",
                "Own division owner required; alternate controller never expands scope",
            ));
        }
        let handle = r.read(army + 0x18, 8)?;
        let kind = u32::from_le_bytes(handle[..4].try_into().unwrap());
        let token = u32::from_le_bytes(handle[4..].try_into().unwrap());
        if (kind == 0 && token == 0) || !handles.insert((kind, token)) {
            return Err(Error::new(
                "ambiguous_army_identity",
                "Nonnull unique complete own division handle required",
            ));
        }
        let location_pointer = r.u64(army + 0x1f0)?;
        let location = if location_pointer == 0 {
            json!({"available":false,"reason":"location_absent"})
        } else {
            if location_pointer % 8 != 0 {
                return Err(Error::new(
                    "invalid_army_location",
                    "Aligned own location required",
                ));
            }
            if r.u64(location_pointer)? != base + crate::construction::layout(&r).province_vtable {
                json!({"available":false,"reason":"unknown_province_layout"})
            } else {
                let id = r.i32(location_pointer + l.province_id)?;
                if id <= 0 {
                    json!({"available":false,"reason":"location_id_absent"})
                } else {
                    let joined = u64::from_le_bytes(
                        entry(&r, gs + provinces, id as usize, 8, 20000)?
                            .try_into()
                            .unwrap(),
                    );
                    if joined != location_pointer {
                        return Err(Error::new(
                            "invalid_army_location",
                            "Own province catalog join required",
                        ));
                    }
                    json!({"available":true,"province_id":id,"join":"own division location equals indexed current GS province"})
                }
            }
        };
        let strength = i64::from_le_bytes(r.read(army + l.strength, 8)?.try_into().unwrap());
        let organisation =
            i64::from_le_bytes(r.read(army + l.organisation, 8)?.try_into().unwrap());
        output.push(json!({"id":format!("{kind:08x}:{token:08x}"),"handle_words":{"type_word":kind,"opaque_token_word":token},"location":location,"strength_raw_signed_decimal":strength.to_string(),"organisation_raw_signed_decimal":organisation.to_string(),"absolute_units_available":false,"input_authorized":false}));
    }
    let (end_calls, end_bytes) = w.finish()?;
    process.check()?;
    Ok(
        json!({"source":"native own division snapshot","build_id":g.build_id,"owned":true,"player":context.player,"identity_scope":{"pid":identity.pid,"process_start_ticks":identity.start_ticks,"playthrough_id":context.playthrough_id,"meaning":"complete handle in this owned process/campaign observation; not persistent across reloads or pointer/ID reuse"},"count":count,"divisions":output,"unavailable":{"registry_publication":true,"handle_generation_semantics":true,"persistent_unit_ids":true,"normalised_strength_organisation":true,"viewport_visibility":true,"commands":true},"input_authorized":false,"observation_paused_game":false,"freshness":{"observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"read_calls_kind":"logical_read_requests","read_calls":r.calls.get()+end_calls as u64,"read_bytes":r.bytes.get()+end_bytes}}),
    )
}
