//! Current ordinary T binding and the keyboard receivers preceding it.
//! This is a witnessed cache observation, not an atomic render-frame proof.
use super::{exact, invalid, pointer, Reader, Result, Value};
use serde::Serialize;
use serde_json::json;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Button {
    object: u64,
    flags: u32,
    state: i32,
    control: u8,
    handler: u64,
    parent: u64,
    wrapper: u8,
    shortcuts: Vec<Vec<u8>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Node {
    renderer: u64,
    target: u64,
    button: Option<Button>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct Observation {
    gui: u64,
    graphics: u64,
    renderer: u64,
    button: u64,
    sources: Vec<u64>,
    prefix: Vec<Node>,
    focused: [u64; 2],
    gui_flags: [u8; 4],
    reason: &'static str,
}

fn array(r: &Reader<'_>, at: u64, stride: usize, bound: usize) -> Result<Vec<u8>> {
    let (header, data, count) = r.array_header(at)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if capacity > bound {
        return Err(invalid(
            "Keyboard collection exceeds complete observation bound",
        ));
    }
    if data != 0 {
        pointer(data)?;
    }
    if count == 0 {
        return Ok(Vec::new());
    }
    r.read(data, count * stride)
}
fn shortcuts(r: &Reader<'_>, button: u64) -> Result<Vec<Vec<u8>>> {
    let bytes = array(r, button + 0xa0, 40, 128)?;
    let mut result = Vec::new();
    for entry in bytes.chunks_exact(40) {
        let data = u64::from_le_bytes(entry[16..24].try_into().unwrap());
        let cap = i32::from_le_bytes(entry[24..28].try_into().unwrap());
        let count = i32::from_le_bytes(entry[28..32].try_into().unwrap());
        if !(0..=128).contains(&cap) || count < 0 || count > cap || (count > 0 && data == 0) {
            return Err(invalid("Bounded extra-key header required"));
        }
        if data != 0 {
            pointer(data)?;
        }
        // Ordinary unmodified T has no extra keys. A nonzero count therefore
        // cannot match, regardless of contents; never follow those payloads.
        result.push(entry.to_vec());
    }
    Ok(result)
}
fn plain_t(entries: &[Vec<u8>]) -> usize {
    entries
        .iter()
        .filter(|e| {
            i32::from_le_bytes(e[..4].try_into().unwrap()) == 0x74
                && i32::from_le_bytes(e[8..12].try_into().unwrap()) == 0
                && i32::from_le_bytes(e[28..32].try_into().unwrap()) == 0
        })
        .count()
}
fn sprite(r: &Reader<'_>, base: u64, node: u64) -> Result<()> {
    pointer(node)?;
    if ![base + 0x34bdb10, base + 0x34afed0].contains(&r.u64(node)?) {
        return Err(invalid("Unsupported keyboard-source renderer type"));
    }
    Ok(())
}

pub(super) fn observe(r: &Reader<'_>, base: u64, gui: u64, button: u64) -> Result<Observation> {
    if r.u64(base + 0x35dc058)? != gui {
        return Err(invalid(
            "Keyboard handler must use the same own GUI singleton",
        ));
    }
    let graphics = r.u64(gui + 0x98)?;
    exact(r, base, graphics, 0x33dafa8)?;
    if r.u64(button + 0xd0)? != graphics {
        return Err(invalid("Keyboard button graphics owner differs"));
    }
    let renderer = r.u64(button + 0xd8)?;
    sprite(r, base, renderer)?;
    if r.u64(renderer + 0x138)? != button {
        return Err(invalid("Keyboard renderer must retain the named button"));
    }
    let mut out = Observation {
        gui,
        graphics,
        renderer,
        button,
        sources: Vec::new(),
        prefix: Vec::new(),
        focused: [r.u64(gui + 0x138)?, r.u64(gui + 0x210)?],
        gui_flags: [
            r.byte(gui + 0x10)?,
            r.byte(gui + 0x11)?,
            r.byte(gui + 0x12)?,
            r.byte(gui + 0x130)?,
        ],
        reason: "ordinary_t_route_observed",
    };
    if out.focused != [0, 0] {
        out.reason = "focused_keyboard_or_text_receiver";
        return Ok(out);
    }
    if out.gui_flags[0] == 0 || out.gui_flags[1..] != [0, 0, 0] {
        out.reason = "unsupported_keyboard_dispatch_mode";
        return Ok(out);
    }
    out.sources = array(r, graphics + 0x68, 8, 4096)?
        .chunks_exact(8)
        .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
        .collect();
    if out.sources.iter().filter(|v| **v == renderer).count() != 1 {
        out.reason = "named_keyboard_renderer_missing_or_duplicated";
        return Ok(out);
    }
    for source in out.sources.iter().rev().copied() {
        sprite(r, base, source)?;
        let target = r.u64(source + 0x138)?;
        if target == 0 {
            out.prefix.push(Node {
                renderer: source,
                target,
                button: None,
            });
            continue;
        }
        exact(r, base, target, 0x34aafa0)?;
        if r.u64(target + 0x58)? != gui
            || r.u64(target + 0xd0)? != graphics
            || r.u64(target + 0xd8)? != source
        {
            return Err(invalid(
                "Keyboard predecessor must retain own GUI, graphics and renderer",
            ));
        }
        let flags = u32::from_le_bytes(r.read(target + 0x64, 4)?.try_into().unwrap());
        let mut b = Button {
            object: target,
            flags,
            state: -1,
            control: 0,
            handler: 0,
            parent: 0,
            wrapper: 0,
            shortcuts: Vec::new(),
        };
        if flags & 0xc04 != 0x804 {
            out.prefix.push(Node {
                renderer: source,
                target,
                button: Some(b),
            });
            if source == renderer {
                out.reason = "named_keyboard_button_ineligible";
                break;
            } else {
                continue;
            }
        }
        b.state = r.i32(target + 0x9c)?;
        b.handler = r.u64(target + 0xc0)?;
        b.parent = r.u64(target + 0x178)?;
        b.wrapper = r.byte(target + 0x1d1)?;
        b.control = r.byte(target + 0xcc)?;
        if b.handler != 0 || b.parent != 0 || b.wrapper != 0 || ![0, 2].contains(&b.state) {
            out.reason = "unsupported_keyboard_button_gate";
        } else if b.control & 8 == 0 {
            if source == renderer {
                out.reason = "named_keyboard_shortcut_disabled";
            }
        } else {
            b.shortcuts = shortcuts(r, target)?;
            let count = plain_t(&b.shortcuts);
            if source == renderer {
                if count != 1 {
                    out.reason = "named_button_requires_one_plain_t_binding";
                }
            } else if count != 0 || flags & 2 != 0 {
                out.reason = "preceding_keyboard_target_can_handle_or_consume_t";
            }
        }
        out.prefix.push(Node {
            renderer: source,
            target,
            button: Some(b),
        });
        if source == renderer || out.reason != "ordinary_t_route_observed" {
            break;
        }
    }
    Ok(out)
}
impl Observation {
    #[cfg(target_os = "macos")]
    pub(super) fn require_ready(&self) -> Result<()> {
        if self.reason != "ordinary_t_route_observed" {
            return Err(invalid(self.reason));
        }
        Ok(())
    }
    pub(super) fn diagnostic(&self) -> Value {
        json!({"key":"t","source_count":self.sources.len(),"inspected_receiver_count":self.prefix.len(),
            "focused_receiver_present":self.focused.iter().any(|v|*v!=0),"reason":self.reason,
            "ordinary_route_observed":self.reason=="ordinary_t_route_observed",
            "complete_source_membership_observed":!self.sources.is_empty(),
            "keyboard_handler_enabled_raw":self.gui_flags[0],"event_flag_raw":self.gui_flags[1],
            "conditional_dispatch_raw":self.gui_flags[2],"skip_dispatch_raw":self.gui_flags[3],
            "input_authorized":false,"cache_completion_verified":false,"delivery_verified":false,
            "cursor_movement_required":false})
    }
}
