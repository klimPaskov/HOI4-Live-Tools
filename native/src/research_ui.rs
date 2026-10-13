//! Bounded own research panel diagnostics, independently recovered for the
//! verified Linux build. Descriptors never authorize input or claim hit routing.
use crate::{
    error::{Error, Result},
    game,
    memory::{ReadMemory, Reader},
    platform::Process,
};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

/// Every request reads fresh bytes. Retained bytes are witnesses, never a cache.
struct Witnesses<'a> {
    process: &'a dyn Process,
    reads: RefCell<BTreeMap<(u64, usize), Vec<u8>>>,
}
impl ReadMemory for Witnesses<'_> {
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        let bytes = self.process.read(address, size)?;
        let mut reads = self.reads.borrow_mut();
        if let Some(prior) = reads.get(&(address, size)) {
            if *prior != bytes {
                return Err(Error::new("torn_read", "Research UI field changed"));
            }
        } else {
            reads.insert((address, size), bytes.clone());
        }
        Ok(bytes)
    }
}
impl Witnesses<'_> {
    fn finish(&self) -> Result<(usize, usize)> {
        let reads = self.reads.borrow();
        let mut bytes = 0;
        for ((address, size), prior) in reads.iter() {
            if self.process.read(*address, *size)? != *prior {
                return Err(Error::new("torn_read", "Research UI witness changed"));
            }
            bytes += size;
        }
        Ok((reads.len(), bytes))
    }
}
fn exact(r: &Reader<'_>, pointer: u64, vptr: u64) -> Result<()> {
    if pointer % 8 != 0 || r.u64(pointer)? != vptr {
        return Err(Error::new(
            "invalid_gui_type",
            "Exact primary research UI type required",
        ));
    }
    Ok(())
}
fn array(r: &Reader<'_>, address: u64, bound: usize) -> Result<Vec<u64>> {
    let (header, data, count) = r.array_header(address)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if capacity > bound || (count > 0 && data % 8 != 0) {
        return Err(Error::new(
            "invalid_array",
            "Research UI array exceeds bound/alignment",
        ));
    }
    let payload = if count == 0 {
        Vec::new()
    } else {
        r.read(data, count * 8)?
    };
    Ok(payload
        .chunks_exact(8)
        .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
        .collect())
}
fn window(r: &Reader<'_>, base: u64, pointer: u64) -> Result<Value> {
    if pointer == 0 {
        return Ok(json!({"materialized":false,"visible_predicate":null}));
    }
    exact(r, pointer, base + 0x4347d58)?;
    let flags = r.byte(pointer + 0xa5)?;
    let states = [r.i32(pointer + 0xef8)?, r.i32(pointer + 0xeb0)?];
    Ok(
        json!({"materialized":true,"visible_predicate":flags&8!=0&&states.iter().all(|v|*v!=2),"effective_hit_routing_verified":false}),
    )
}
/// Type metadata is diagnostic only. No field layout follows from an unknown
/// name. Tables/typeinfo must lie in this ELF's data.rel.ro; names in rodata.
fn type_encoding(r: &Reader<'_>, base: u64, object: u64) -> Result<String> {
    let table = r.u64(object)?;
    let in_section = |p: u64, size: u64, low: u64, high: u64| {
        p >= base + low && p.checked_add(size).is_some_and(|end| end <= base + high)
    };
    let metadata = |p, size| in_section(p, size, 0x41adde0, 0x436d9f8);
    if !metadata(table.saturating_sub(16), 24)
        || i64::from_le_bytes(r.read(table - 16, 8)?.try_into().unwrap()) != 0
    {
        return Err(Error::new(
            "invalid_gui_type",
            "Main-image primary GUI type metadata required",
        ));
    }
    let info = r.u64(table - 8)?;
    if !metadata(info, 16) {
        return Err(Error::new(
            "invalid_gui_type",
            "GUI type information outside verified image",
        ));
    }
    let name = r.u64(info + 8)?;
    if !in_section(name, 128, 0x3506000, 0x3975f39) {
        return Err(Error::new(
            "invalid_gui_type",
            "GUI type name outside verified image",
        ));
    }
    let bytes = r.read(name, 128)?;
    let end = bytes
        .iter()
        .position(|v| *v == 0)
        .ok_or_else(|| Error::new("invalid_gui_type", "GUI type name exceeds bound"))?;
    if end == 0
        || !bytes[..end]
            .iter()
            .all(|v| v.is_ascii_alphanumeric() || *v == b'_')
    {
        return Err(Error::new(
            "invalid_gui_type",
            "Invalid bounded GUI type encoding",
        ));
    }
    Ok(String::from_utf8(bytes[..end].to_vec()).unwrap())
}

pub fn snapshot(process: &dyn Process) -> Result<Value> {
    let start = Instant::now();
    process.begin_snapshot()?;
    let observed = Witnesses {
        process,
        reads: RefCell::new(BTreeMap::new()),
    };
    let r = Reader::new(&observed);
    let base = process.image_base();
    let gs = r.u64(base + 0x44086a8)?;
    let idler = r.u64(base + 0x44085f8)?;
    exact(&r, gs, base + 0x42c7598)?;
    exact(&r, idler, base + 0x42c9a88)?;
    let registry = r.u64(idler + 0x4f8)?;
    exact(&r, registry, base + 0x42736f8)?;
    let tag = r.i32(gs + 0x518)?;
    if !(1..4096).contains(&tag) {
        return Err(Error::new(
            "player_unavailable",
            "Positive own country tag required",
        ));
    }
    let index = i32::from_le_bytes(
        r.array_entry(gs + 0x340, tag as usize, 4)?
            .try_into()
            .unwrap(),
    );
    if index < 0 {
        return Err(Error::new("player_unavailable", "Own country index absent"));
    }
    let country = u64::from_le_bytes(
        r.array_entry(gs + 0x310, index as usize, 8)?
            .try_into()
            .unwrap(),
    );
    r.object_type(country, "CCountry")?;
    let status = r.u64(country + 0xf38)?;
    exact(&r, status, base + 0x42108f8)?;
    if r.u64(status + 0x130)? != country || r.i32(status + 0x138)? != tag {
        return Err(Error::new(
            "invalid_research_owner",
            "Own research status required",
        ));
    }
    let slots = array(&r, status + 0xa0, 32)?;
    let mut unique = std::collections::BTreeSet::new();
    for slot in &slots {
        exact(&r, *slot, base + 0x4210898)?;
        if !unique.insert(*slot) || r.u64(slot + 0x10)? != status {
            return Err(Error::new(
                "invalid_research_owner",
                "Unique own research slot required",
            ));
        }
    }
    let manager = r.u64(idler + 0x6b8)?;
    if r.u64(manager + 0x180)? != idler {
        return Err(Error::new(
            "invalid_gui_owner",
            "Research interface owner changed",
        ));
    }
    let view = r.u64(manager + 0xf0)?;
    let tree = r.u64(manager + 0xf8)?;
    exact(&r, view, base + 0x424dc38)?;
    exact(&r, tree, base + 0x424de40)?;
    if r.u64(view + 0x2f0)? != idler || r.u64(tree + 0x2f0)? != idler || r.i32(tree + 0xe24)? != 6 {
        return Err(Error::new(
            "invalid_gui_owner",
            "Ordinary own research views required",
        ));
    }
    let display_tag = r.i32(tree + 0xe20)?;
    if display_tag > 0 && display_tag != tag {
        return Err(Error::new(
            "foreign_ui_context",
            "Own displayed research country required",
        ));
    }
    let selected = r.u64(tree + 0xe08)?;
    let selected_index = if selected == 0 {
        None
    } else {
        Some(slots.iter().position(|v| *v == selected).ok_or_else(|| {
            Error::new(
                "invalid_research_owner",
                "Selected slot is outside own research",
            )
        })?)
    };
    let active = r.i32(manager + 0x3d8)?;
    let panel_state = window(&r, base, r.u64(view + 0x2f8)?)?;
    let tree_state = window(&r, base, r.u64(tree + 0x2f8)?)?;
    let widgets = array(&r, view + 0x308, 32)?;
    if widgets.len() > slots.len() {
        return Err(Error::new(
            "invalid_research_join",
            "More widgets than own research slots",
        ));
    }
    let mut output = Vec::new();
    let mut widget_ids = std::collections::BTreeSet::new();
    for (index, widget) in widgets.iter().enumerate() {
        exact(&r, *widget, base + 0x424e7d8)?;
        if !widget_ids.insert(*widget)
            || r.u64(widget + 0x2a8)? != registry
            || r.u64(widget + 0x2b0)? != view
            || r.u64(widget + 0x2b8)? != slots[index]
        {
            return Err(Error::new(
                "invalid_research_join",
                "Own widget/slot array join required",
            ));
        }
        let container = r.u64(widget + 0x10)?;
        let container_state = window(&r, base, container)?;
        if container == 0 {
            return Err(Error::new(
                "gui_unavailable",
                "Research slot container unavailable",
            ));
        }
        let buttons = array(&r, container + 0x958, 64)?;
        let mut button_output = Vec::new();
        for button in buttons.iter().take(8) {
            let encoding = type_encoding(&r, base, *button)?;
            let mut value = json!({"type_encoding":encoding,"geometry_verified":false});
            if r.u64(*button)? == base + 0x43470c8 {
                value["disabled_raw"] = json!(r.i32(button + 0xac)?);
                let render = r.u64(button + 0xe8)?;
                value["render_type_encoding"] = if render == 0 {
                    Value::Null
                } else {
                    json!(type_encoding(&r, base, render)?)
                };
            }
            button_output.push(value);
        }
        output.push(json!({"slot":index,"container":container_state,"button_count":buttons.len(),"buttons_observed":button_output,"button_sample_limit":8,"input_authorized":false}));
    }
    let (end_calls, end_bytes) = observed.finish()?;
    process.check()?;
    Ok(
        json!({"source":"native own research UI diagnostics","build_id":game::BUILD_ID,"owned":true,"own_slot_count":slots.len(),"widget_count":widgets.len(),"widgets_complete":widgets.len()==slots.len(),"slot_identity":"own array index in current process/status snapshot; not a persistent ID","selected_slot":selected_index,"active_mode_raw":active,"panel":panel_state,"ordinary_tree":tree_state,"slots":output,"input_authorized":false,"unverified":["effective ancestor visibility and clipping","sprite hit predicates","topmost hit routing","physical client conversion"],"observation_paused_game":false,"freshness":{"observed_unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),"elapsed_ms":start.elapsed().as_secs_f64()*1000.0,"read_calls":r.calls.get()+end_calls as u64,"read_bytes":r.bytes.get()+end_bytes}}),
    )
}
