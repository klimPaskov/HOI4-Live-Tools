//! Optional Mac Find edit-box/focus facts. This is a passive same-witness
//! observation, never a prediction of key delivery or permission to type.
use super::{invalid, mac_ui};
use crate::{error::Result, memory::Reader};
use serde_json::{json, Value};

const EDIT_BOX: u64 = 0x34add98;
const TEXT_RECEIVER: u64 = 0x34ae0c8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Focus {
    find: u64,
    root: u64,
    registry: u64,
    edit_box: u64,
    children: Vec<(u64, Vec<u64>)>,
    focused: [u64; 2],
    flags: u32,
    edit_flags: u8,
}

impl Focus {
    pub(super) fn focused(&self) -> bool {
        self.focused == [self.edit_box, self.edit_box + 0x180]
    }

    pub(super) fn summary(&self) -> Value {
        json!({"exact_named_edit_box":true,"own_registry_verified":true,
            "keyboard_receiver_matches":self.focused[0]==self.edit_box,
            "text_receiver_matches":self.focused[1]==self.edit_box+0x180,
            "focused":self.focused(),"widget_flags_raw":self.flags,
            "edit_flags_raw":self.edit_flags,
            "input_authorized":false,"delivery_verified":false})
    }
}

/// The caller retains the Find object and registry in its complete witness.
/// Hidden/unmaterialized windows skip the retained edit pointer entirely.
pub(super) fn observe(
    r: &Reader<'_>,
    base: u64,
    find: u64,
    root: u64,
    registry: u64,
) -> Result<Option<Focus>> {
    if root == 0 {
        return Ok(None);
    }
    if mac_ui::window(r, base, root, registry)? != root {
        return Err(invalid("Canonical Mac Find focus root required"));
    }
    if r.byte(root + 0x95)? & 8 == 0 {
        return Ok(None);
    }
    if r.u64(base + 0x35dc058)? != registry {
        return Err(invalid("Mac Find focus must use the own GUI singleton"));
    }
    let edit_box = r.u64(find + 0xc40)?;
    if edit_box == 0 {
        return Ok(None);
    }
    // Independent ctor 102792be0 writes the primary and the +180 receiver.
    // Focus acquisition 102794af0 passes this+180 through 102863ee0 to GUI
    // setter 1027c8f30; do not borrow Linux's +1a0 receiver subobject.
    mac_ui::exact(r, base, edit_box, EDIT_BOX)?;
    if r.u64(edit_box + 0x58)? != registry {
        return Err(invalid("Mac Find edit box must retain the own registry"));
    }
    let mut children = Vec::new();
    if mac_ui::named(r, base, root, "to_find_text", &mut children)? != Some(edit_box) {
        return Err(invalid(
            "Mac Find edit box must be the unique named own child",
        ));
    }
    mac_ui::exact(r, base, edit_box + 0x180, TEXT_RECEIVER)?;
    Ok(Some(Focus {
        find,
        root,
        registry,
        edit_box,
        children,
        focused: [r.u64(registry + 0x138)?, r.u64(registry + 0x210)?],
        flags: u32::from_le_bytes(r.read(edit_box + 0x64, 4)?.try_into().unwrap()),
        edit_flags: r.byte(edit_box + 0x239)?,
    }))
}

#[cfg(test)]
#[path = "../../tests/support/mac_construction_find_focus.rs"]
mod tests;
