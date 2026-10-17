//! Conditional read-only control binding, never a hit/input authorization.
use crate::{
    error::{Error, Result},
    memory::Reader,
    research_ui::array,
};
use serde_json::{json, Value};
pub(crate) fn observe(
    r: &Reader<'_>,
    gui: u64,
    technology: u64,
    registry: u64,
    reuse: &mut crate::node_reuse::Reuse,
) -> Result<Value> {
    let base = reuse.base();
    let tree = reuse.tree();
    let mut out = json!({"binding_verified":false,"reason":"layout_unavailable","control_type_encoding":null,"coordinates":"unavailable","input_authorized":false});
    if gui == 0 {
        out["reason"] = json!("gui_unmaterialized");
        return Ok(out);
    }
    if r.u64(gui)? != base + 0x4347d58 {
        return Ok(out);
    }
    if r.u64(gui + 0x98)? != registry {
        return Err(Error::new(
            "invalid_gui_owner",
            "Node container must belong to own GUI registry",
        ));
    }
    let buttons = array(r, gui + 0x958, 256)?;
    let Some(&button) = buttons.first() else {
        out["reason"] = json!("control_unmaterialized");
        return Ok(out);
    };
    out["control_type_encoding"] = reuse.nullable_type(r, button)?;
    if button == 0 || r.u64(button)? != base + 0x4346b10 {
        return Ok(out);
    }
    // Exact Drag -> CGuiElement constructor preserves the registry at +0x68.
    if r.u64(button + 0x68)? != registry {
        return Err(Error::new(
            "invalid_gui_owner",
            "Node control must belong to own GUI registry",
        ));
    }
    // Exact Drag constructor/getter only; render metadata never implies layout.
    out["render_type_encoding"] = reuse.nullable_type(r, r.u64(button + 0x110)?)?;
    if r.u64(button + 8)? != technology {
        return Err(Error::new(
            "invalid_research_join",
            "Ordinary control data must equal own node technology",
        ));
    }
    // Exact Drag embedded observer table. Unknown secondary layouts stay unavailable.
    if r.u64(button + 0x80)? != base + 0x4346ea8 {
        return Ok(out);
    }
    if r.u64(button + 0xa0)? != 0 {
        out["reason"] = json!("single_observer_override");
        return Ok(out);
    }
    let count = r.i32(button + 0x98)?;
    if !(0..=16).contains(&count) {
        return Err(Error::new(
            "invalid_gui_binding",
            "Listener count exceeds diagnostic bound",
        ));
    }
    let mut link = r.u64(button + 0x88)?;
    let tail = r.u64(button + 0x90)?;
    let mut prior = 0;
    let mut seen = std::collections::BTreeSet::new();
    let mut matched = false;
    let mut unknown = count != 1;
    for _ in 0..count {
        if link == 0 || link % 8 != 0 || !seen.insert(link) {
            return Err(Error::new("invalid_gui_binding", "Malformed listener list"));
        }
        let target = r.u64(link)?;
        if r.u64(link + 8)? != prior {
            return Err(Error::new(
                "invalid_gui_binding",
                "Listener backlink mismatch",
            ));
        }
        let removed = r.byte(link + 0x18)?;
        if removed != 0 {
            unknown = true;
        }
        if [tree + 0x5e0, tree + 0x868].contains(&target) && r.u64(target)? == base + 0x424e0a8 {
            if reuse.callback(r, target)? {
                if removed == 0 {
                    matched = true;
                }
            } else {
                unknown = true;
            }
        } else {
            unknown = true;
        }
        prior = link;
        link = r.u64(link + 0x10)?;
    }
    if link != 0 || prior != tail || (count == 0 && tail != 0) {
        return Err(Error::new(
            "invalid_gui_binding",
            "Listener terminal/count mismatch",
        ));
    }
    out["binding_verified"] = json!(matched && !unknown);
    out["reason"] = json!(if matched && !unknown {
        "ordinary_callback_data_join"
    } else {
        "callback_unavailable"
    });
    Ok(out)
}
