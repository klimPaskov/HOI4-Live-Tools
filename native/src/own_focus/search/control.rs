//! Exact Linux Focus search edit box. Opt-in observation only; ordinary input
//! still requires its own stage, persisted intent and independently checked UI.
use super::*;
use crate::node_reuse::Reuse;
mod render;

fn routing(
    r: &Reader<'_>,
    base: u64,
    edit: u64,
    registry: u64,
    flags: u32,
    edit_flags: u8,
) -> Result<Value> {
    // GUI+158 is keyboard focus, GUI+230 is the separate text receiver.
    // Never dereference another control or infer either identity from flags.
    let gui = r.u64(base + 0x45778e0)?;
    if gui != registry {
        return Ok(
            json!({"available":false,"reason":"owned_gui_global_join_unavailable",
            "keyboard_focus_verified":false,"text_receiver_verified":false,
            "input_authorized":false}),
        );
    }
    exact_search_type(r, base, gui, 0x42736f8, "input_gui")?;
    exact_search_type(r, base, edit + 0x1a0, 0x4349be8, "text_receiver")?;
    let keyboard = r.u64(gui + 0x158)? == edit;
    let text = r.u64(gui + 0x230)? == edit + 0x1a0;
    let enabled = flags & 0x800 != 0 && edit_flags & 2 == 0;
    let focused = edit_flags & 1 != 0;
    Ok(
        json!({"available":true,"reason":"exact_owned_gui_and_edit_receiver",
        "gui_global_binding_verified":true,"receiver_layout_verified":true,
        "keyboard_target_matches":keyboard,"text_receiver_matches":text,
        "control_enabled_flag":enabled,"edit_focused_flag":focused,
        "keyboard_focus_verified":keyboard&&enabled&&focused,
        "text_receiver_verified":text&&enabled&&focused,
        "effective_routing_verified":false,"input_authorized":false}),
    )
}

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

fn named_child(
    r: &Reader<'_>,
    container: u64,
    pointer_offset: u64,
    name_offset: u64,
    requested: &str,
) -> Result<u64> {
    let children = pointers(r, container + pointer_offset, 256)?;
    unique(&children)?;
    if children.iter().any(|child| child % 8 != 0) {
        return Err(invalid("Aligned search collection entries required"));
    }
    let (header, names, count) = r.array_header(container + name_offset)?;
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
        if name == requested {
            found = Some(*child);
        }
    }
    // Validate the complete paired collection before reading selected payload.
    found.ok_or_else(|| invalid("Named search container entry missing"))
}

pub(crate) fn observe(
    r: &Reader<'_>,
    base: u64,
    view: u64,
    registry: u64,
) -> Result<(Value, Option<[u64; 3]>)> {
    let edit = r.u64(view + 0x1200)?;
    if edit == 0 {
        return Ok((
            json!({"available":false,"reason":"query_control_unmaterialized",
            "input_authorized":false}),
            None,
        ));
    }
    exact_search_type(r, base, edit, 0x43498b8, "query_control")?;
    if r.u64(edit + 0x68)? != registry {
        return Err(Error::new(
            "foreign_focus_scope",
            "Search control registry differs",
        ));
    }
    // edit+60 is CEditBoxType, not the parent window (live Run059 confirmed
    // primary 4349f00). Follow the owned view root and its named window list.
    let root = r.u64(view + 0x2f8)?;
    exact_search_type(r, base, root, 0x4347d58, "query_root")?;
    if r.u64(root + 0x98)? != registry {
        return Err(Error::new(
            "foreign_focus_scope",
            "Search root registry differs",
        ));
    }
    let container = named_child(r, root, 0xbb0, 0xbc8, "find_view")?;
    if container == root {
        return Err(invalid("Search container cannot alias its root"));
    }
    exact_search_type(r, base, container, 0x4347d58, "query_container")?;
    if r.u64(container + 0x98)? != registry {
        return Err(Error::new(
            "foreign_focus_scope",
            "Search container registry differs",
        ));
    }
    // Exact edit-box lookup 2e88080 uses ac0/ad8; 9a0/9b8 is buttons.
    if named_child(r, container, 0xac0, 0xad8, "to_find_text")? != edit {
        return Err(invalid("Named search control and Focus view alias differ"));
    }
    listener(r, base, edit, view)?;
    let query = wide_query(r, edit)?;
    let geometry = render::observe(r, base, edit, container, registry)?;
    let flags = u32::from_le_bytes(r.read(edit + 0x74, 4)?.try_into().unwrap());
    let edit_flags = r.byte(edit + 0x271)?;
    let routing = routing(r, base, edit, registry, flags, edit_flags)?;
    let container_flags = r.byte(container + 0xa5)?;
    let transitions = [r.i32(container + 0xeb0)?, r.i32(container + 0xef8)?];
    // Use the already typed local edit identity; never re-follow a view pointer
    // after its type check. The renderer pointer itself remains a witness.
    let anchors = [root, edit, r.u64(edit + 0x1b0)?];
    Ok((
        json!({"available":true,"name":"to_find_text","query_text":query,
        "query_encoding":"bounded_utf32le","named_container_binding_verified":true,
        "container_flags_a5_raw":container_flags,"container_states_raw":transitions,
        "container_locally_ready":container_flags&8!=0 && transitions==[0,0],
        "ordinary_listener_binding_verified":true,
        "flags74_raw":flags,"edit_flags271_raw":edit_flags,
        "renderer_control_join_verified":geometry["renderer_control_join_verified"]==true,
        "render_diagnostic":geometry,"keyboard_focus_verified":routing["keyboard_focus_verified"],
        "routing_diagnostic":routing,"effective_routing_verified":false,
        "input_authorized":false}),
        Some(anchors),
    ))
}
