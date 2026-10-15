//! Exact Linux Focus search edit box. Opt-in observation only; ordinary input
//! still requires its own stage, persisted intent and independently checked UI.
use super::*;
use crate::node_reuse::Reuse;

fn wide_query(r: &Reader<'_>, edit: u64) -> Result<String> {
    // CEditBox embeds the text editor at +80. Its getter reads the std::wstring
    // at editor+30; this exact Linux conversion consumes four-byte code points.
    let address = edit + 0xb0;
    let pointer = r.u64(address)?;
    let length = r.u64(address + 8)?;
    if length > 128 || pointer % 4 != 0 {
        return Err(invalid("Bounded aligned search query required"));
    }
    if pointer == address + 16 {
        if length > 3 {
            return Err(invalid("Inline search query exceeds wide string storage"));
        }
    } else {
        let capacity = r.u64(address + 16)?;
        if capacity < length || capacity > 4096 {
            return Err(invalid("Search query capacity outside policy"));
        }
    }
    let bytes = r.read(pointer, (length as usize + 1) * 4)?;
    if bytes[bytes.len() - 4..] != [0; 4] {
        return Err(invalid("Search query terminator changed"));
    }
    let mut query = String::new();
    for unit in bytes[..bytes.len() - 4].chunks_exact(4) {
        let value = u32::from_le_bytes(unit.try_into().unwrap());
        let character = char::from_u32(value)
            .filter(|c| *c != '\0')
            .ok_or_else(|| invalid("Invalid search query code point"))?;
        query.push(character);
    }
    Ok(query)
}

fn listener(r: &Reader<'_>, base: u64, edit: u64, view: u64) -> Result<()> {
    exact_search_type(r, base, edit + 0x80, 0x4349b78, "embedded_editor")?;
    // This edit-box observer is a list rooted in the embedded editor. Require
    // the one constructor-bound Focus listener; never chase an unknown target.
    if r.i32(edit + 0x98)? != 1 || r.u64(edit + 0xa0)? != 0 {
        return Err(invalid("Exact single Focus search listener required"));
    }
    let node = r.u64(edit + 0x88)?;
    if node == 0 || node % 8 != 0 || r.u64(edit + 0x90)? != node {
        return Err(invalid("Search listener endpoints differ"));
    }
    let target = view + 0x678;
    if r.u64(node)? != target
        || r.u64(node + 8)? != 0
        || r.u64(node + 0x10)? != 0
        || r.byte(node + 0x18)? != 0
    {
        return Err(invalid("Search listener must capture exact own Focus view"));
    }
    exact_search_type(r, base, target, 0x424aa30, "own_view_listener")?;
    if r.u64(target + 8)? != view {
        return Err(invalid("Search listener own-view receiver differs"));
    }
    for (offset, member) in [
        (0x10, 0x1510480),
        (0x20, 0x1510480),
        (0x30, 0x1510830),
        (0x40, 0x1510870),
    ] {
        if r.u64(target + offset)? != base + member || r.u64(target + offset + 8)? != 0 {
            return Err(invalid("Search listener ordinary member binding differs"));
        }
    }
    Ok(())
}

pub(crate) fn observe(r: &Reader<'_>, base: u64, view: u64, registry: u64) -> Result<Value> {
    let edit = r.u64(view + 0x1200)?;
    if edit == 0 {
        return Ok(
            json!({"available":false,"reason":"query_control_unmaterialized",
            "input_authorized":false}),
        );
    }
    exact_search_type(r, base, edit, 0x43498b8, "query_control")?;
    if r.u64(edit + 0x68)? != registry {
        return Err(Error::new(
            "foreign_focus_scope",
            "Search control registry differs",
        ));
    }
    let container = r.u64(edit + 0x60)?;
    exact_search_type(r, base, container, 0x4347d58, "query_container")?;
    if r.u64(container + 0x98)? != registry {
        return Err(Error::new(
            "foreign_focus_scope",
            "Search container registry differs",
        ));
    }
    let children = pointers(r, container + 0x9a0, 256)?;
    unique(&children)?;
    let (header, names, count) = r.array_header(container + 0x9b8)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if capacity > 256 || count != children.len() || (count > 0 && names % 8 != 0) {
        return Err(invalid("Bounded paired search container names required"));
    }
    let mut seen = BTreeSet::new();
    let mut found = None;
    for (index, child) in children.iter().enumerate() {
        let name = r.string_bounded(names + index as u64 * 32, 256)?;
        if !seen.insert(name.clone()) {
            return Err(invalid("Duplicate search container control name"));
        }
        if name == "to_find_text" {
            found = Some(*child);
        }
    }
    if found != Some(edit) {
        return Err(invalid("Named search control and Focus view alias differ"));
    }
    listener(r, base, edit, view)?;
    let query = wide_query(r, edit)?;
    let render = r.u64(edit + 0x1b0)?;
    let mut geometry = json!({"available":false,"reason":"search_renderer_layout_unavailable",
        "input_authorized":false});
    let renderer_join = render != 0 && r.u64(render)? == base + 0x43593d8;
    if renderer_join {
        if r.u64(render + 0x138)? != edit {
            return Err(invalid("Search text renderer and control differ"));
        }
        let mut reuse = Reuse::for_owned_focus(base);
        geometry =
            crate::info_research_button::text_raw(r, &mut reuse, render, container, registry)?;
        geometry["render_type_encoding"] = json!("11CTextSprite");
    }
    Ok(
        json!({"available":true,"name":"to_find_text","query_text":query,
        "query_encoding":"bounded_utf32le","ordinary_listener_binding_verified":true,
        "flags74_raw":u32::from_le_bytes(r.read(edit+0x74,4)?.try_into().unwrap()),
        "edit_flags271_raw":r.byte(edit+0x271)?,
        "renderer_control_join_verified":renderer_join,"render_diagnostic":geometry,
        "keyboard_focus_verified":false,"effective_routing_verified":false,
        "input_authorized":false}),
    )
}
