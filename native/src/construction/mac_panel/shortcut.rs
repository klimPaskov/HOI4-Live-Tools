//! Current ordinary T binding and the keyboard receivers preceding it.
//! This is a witnessed cache observation, not an atomic render-frame proof.
use super::{exact, invalid, pointer, Reader, Result, Value};
use serde::Serialize;
use serde_json::json;

mod escape;
pub(super) use escape::{observe_escape_current_prefix, EscapeTarget};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(super) enum PlainKey {
    T,
    Escape,
}
impl PlainKey {
    fn code(self) -> i32 {
        match self {
            Self::T => 0x74,
            Self::Escape => 0x1b,
        }
    }
    fn ready(self) -> &'static str {
        match self {
            Self::T => "ordinary_t_route_observed",
            Self::Escape => "ordinary_escape_route_observed",
        }
    }
    fn conflict(self) -> &'static str {
        match self {
            Self::T => "preceding_keyboard_target_can_handle_or_consume_t",
            Self::Escape => "preceding_keyboard_target_can_handle_or_consume_escape",
        }
    }
    fn absent(self) -> &'static str {
        match self {
            Self::T => "named_button_requires_one_plain_t_binding",
            Self::Escape => "named_button_requires_one_plain_escape_binding",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Self::T => "t",
            Self::Escape => "escape",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
enum Kind {
    Standard,
    Icon,
    Drag,
    Track,
    InstantTextBox,
}
impl Kind {
    fn observe(r: &Reader<'_>, base: u64, object: u64) -> Result<Self> {
        pointer(object)?;
        match r.u64(object)?.checked_sub(base) {
            Some(0x34aafa0) => Ok(Self::Standard),
            Some(0x34a81a8) => Ok(Self::Icon),
            Some(0x34aa400) => Ok(Self::Drag),
            Some(0x34c0918) => Ok(Self::Track),
            Some(0x34b1768) => Ok(Self::InstantTextBox),
            _ => Err(invalid("Unsupported keyboard-target primary type")),
        }
    }
    fn links(&self) -> (u64, u64) {
        // Independent Mac constructor/registration proofs. In particular,
        // Icon's +d0 is its renderer; Drag/Track's +d8 is not a pointer.
        match self {
            Self::Standard => (0xd0, 0xd8),
            Self::Icon => (0xe0, 0xd0),
            Self::Drag => (0xd0, 0x100),
            Self::Track => (0xd0, 0xe8),
            Self::InstantTextBox => (0x88, 0xb0),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct BaseGate {
    state: i32,
    handler: u64,
    disabled: Option<u32>,
    control: Option<u8>,
    shortcuts: Vec<Vec<u8>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct ParentGate {
    parent: u64,
    enabled: Option<u8>,
    wrapper: u8,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
enum Gate {
    Ineligible,
    Standard(ParentGate, Option<BaseGate>),
    Direct(BaseGate),
    Drag { state: i32 },
    InstantTextBox,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Receiver {
    kind: Kind,
    flags: u32,
    gate: Gate,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
struct Node {
    renderer: u64,
    target: u64,
    receiver: Option<Receiver>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(super) struct Observation {
    key: PlainKey,
    gui: u64,
    graphics: u64,
    renderer: u64,
    button: u64,
    named_parent: u64,
    sources: Vec<u64>,
    prefix: Vec<Node>,
    focused: [u64; 2],
    gui_flags: [u8; 4],
    reason: &'static str,
}

fn array(
    r: &Reader<'_>,
    at: u64,
    stride: usize,
    count_bound: usize,
    capacity_bound: usize,
) -> Result<Vec<u8>> {
    let (header, data, count) = r.array_header(at)?;
    let capacity = i32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
    if count > count_bound || capacity > capacity_bound {
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
    let bytes = array(r, button + 0xa0, 40, 128, 128)?;
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
        // Ordinary unmodified keys have no extra keys. A nonzero count therefore
        // cannot match, regardless of contents; never follow those payloads.
        result.push(entry.to_vec());
    }
    Ok(result)
}
fn matches_key(entries: &[Vec<u8>], key: PlainKey) -> usize {
    entries
        .iter()
        .filter(|e| {
            i32::from_le_bytes(e[..4].try_into().unwrap()) == key.code()
                && i32::from_le_bytes(e[8..12].try_into().unwrap()) == 0
                && i32::from_le_bytes(e[28..32].try_into().unwrap()) == 0
        })
        .count()
}
fn sprite(r: &Reader<'_>, base: u64, node: u64) -> Result<()> {
    pointer(node)?;
    // Exact primary C2dVisibleObject chains with the common +138 backlink.
    // CTextSprite's secondary base is deliberately not normalized.
    let primary = r.u64(node)?.checked_sub(base);
    if ![
        0x34bdb10, 0x34afed0, 0x34acdc0, 0x34ba6f0, 0x34b1b30, 0x33e2b28, 0x34c0380,
    ]
    .into_iter()
    .any(|rva| Some(rva) == primary)
    {
        return Err(invalid("Unsupported keyboard-source renderer type"));
    }
    Ok(())
}

fn base_gate(
    r: &Reader<'_>,
    target: u64,
    flags: u32,
    named: bool,
    disabled_standard: bool,
    key: PlainKey,
) -> Result<(BaseGate, Option<&'static str>)> {
    let mut gate = BaseGate {
        state: r.i32(target + 0x9c)?,
        handler: r.u64(target + 0xc0)?,
        disabled: None,
        control: None,
        shortcuts: Vec::new(),
    };
    let mut reason = None;
    if gate.handler != 0 {
        reason = Some("unsupported_keyboard_button_gate");
    } else if gate.state == 3 && disabled_standard && !named {
        // The disabled base path bypasses shortcuts but can still consume.
        // C8 is a u32, unlike CC and the Standard parent's enabled byte.
        let disabled = u32::from_le_bytes(r.read(target + 0xc8, 4)?.try_into().unwrap());
        gate.disabled = Some(disabled);
        if disabled != 0 || flags & 2 != 0 {
            reason = Some(key.conflict());
        }
    } else if ![0, 2].contains(&gate.state) {
        reason = Some("unsupported_keyboard_button_gate");
    } else {
        let control = r.byte(target + 0xcc)?;
        gate.control = Some(control);
        if control & 8 == 0 {
            if named {
                reason = Some("named_keyboard_shortcut_disabled");
            }
        } else {
            gate.shortcuts = shortcuts(r, target)?;
            let count = matches_key(&gate.shortcuts, key);
            if named {
                if count != 1 {
                    reason = Some(key.absent());
                }
            } else if count != 0 || flags & 2 != 0 {
                reason = Some(key.conflict());
            }
        }
    }
    Ok((gate, reason))
}

pub(super) fn observe(
    r: &Reader<'_>,
    base: u64,
    gui: u64,
    button: u64,
    container: u64,
) -> Result<Observation> {
    observe_key(r, base, gui, button, container, PlainKey::T, None)
}

pub(super) fn observe_key(
    r: &Reader<'_>,
    base: u64,
    gui: u64,
    button: u64,
    container: u64,
    key: PlainKey,
    original: Option<&Observation>,
) -> Result<Observation> {
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
    // Only the already-owned direct CContainerWindow supplies this field.
    exact(r, base, container, 0x34ac0b0)?;
    let named_parent = r.u64(container + 0x1238)?;
    exact(r, base, named_parent, 0x34b0428)?;
    let mut out = Observation {
        key,
        gui,
        graphics,
        renderer,
        button,
        named_parent,
        sources: Vec::new(),
        prefix: Vec::new(),
        focused: [r.u64(gui + 0x138)?, r.u64(gui + 0x210)?],
        gui_flags: [
            r.byte(gui + 0x10)?,
            r.byte(gui + 0x11)?,
            r.byte(gui + 0x12)?,
            r.byte(gui + 0x130)?,
        ],
        reason: key.ready(),
    };
    if out.focused != [0, 0] {
        out.reason = "focused_keyboard_or_text_receiver";
        return Ok(out);
    }
    if out.gui_flags[0] == 0 || out.gui_flags[1..] != [0, 0, 0] {
        out.reason = "unsupported_keyboard_dispatch_mode";
        return Ok(out);
    }
    // The keyboard cache retains allocation after UI teardown. Bound active
    // membership separately; spare slots are never read or interpreted. The
    // complete header remains witnessed under Reader's 20,000-capacity limit.
    out.sources = array(r, graphics + 0x68, 8, 4096, 20_000)?
        .chunks_exact(8)
        .map(|v| u64::from_le_bytes(v.try_into().unwrap()))
        .collect();
    if original.is_some_and(|before| before.key != key || before.sources != out.sources) {
        return Err(invalid(
            "Original keyboard source membership changed before payload",
        ));
    }
    if out.sources.iter().filter(|v| **v == renderer).count() != 1 {
        out.reason = "named_keyboard_renderer_missing_or_duplicated";
        return Ok(out);
    }
    for source in out.sources.iter().rev().copied() {
        sprite(r, base, source)?;
        let target = r.u64(source + 0x138)?;
        if original.is_some_and(|before| {
            before
                .prefix
                .get(out.prefix.len())
                .is_none_or(|old| old.renderer != source || old.target != target)
        }) {
            return Err(invalid("Original keyboard receiver changed before payload"));
        }
        if target == 0 {
            out.prefix.push(Node {
                renderer: source,
                target,
                receiver: None,
            });
            continue;
        }
        let kind = Kind::observe(r, base, target)?;
        let (graphics_offset, renderer_offset) = kind.links();
        if r.u64(target + 0x58)? != gui
            || r.u64(target + graphics_offset)? != graphics
            || r.u64(target + renderer_offset)? != source
        {
            return Err(invalid(
                "Keyboard predecessor must retain own GUI, graphics and renderer",
            ));
        }
        let flags = u32::from_le_bytes(r.read(target + 0x64, 4)?.try_into().unwrap());
        let named = source == renderer;
        if named && (target != button || kind != Kind::Standard) {
            return Err(invalid("Named keyboard receiver changed"));
        }
        let mut receiver = Receiver {
            kind,
            flags,
            gate: Gate::Ineligible,
        };
        if flags & 0xc04 != 0x804 {
            out.prefix.push(Node {
                renderer: source,
                target,
                receiver: Some(receiver),
            });
            if source == renderer {
                out.reason = "named_keyboard_button_ineligible";
                break;
            } else {
                continue;
            }
        }
        let reason = match receiver.kind {
            Kind::Standard => {
                let parent = r.u64(target + 0x178)?;
                if named && parent != named_parent {
                    return Err(invalid(
                        "Named keyboard parent differs from own direct container",
                    ));
                }
                let enabled = if parent != 0 {
                    exact(r, base, parent, 0x34b0428)?;
                    if r.u64(source + 0x108)? != parent {
                        return Err(invalid("Keyboard renderer and button parent differ"));
                    }
                    Some(r.byte(parent + 0x128)?)
                } else {
                    None
                };
                let wrapper = r.byte(target + 0x1d1)?;
                let parent = ParentGate {
                    parent,
                    enabled,
                    wrapper,
                };
                // Keep inactive wrappers unsupported, even for predecessors.
                if enabled == Some(0) || wrapper != 0 {
                    receiver.gate = Gate::Standard(parent, None);
                    Some("unsupported_keyboard_button_gate")
                } else {
                    let (gate, reason) = base_gate(r, target, flags, named, true, key)?;
                    receiver.gate = Gate::Standard(parent, Some(gate));
                    reason
                }
            }
            Kind::Icon | Kind::Track => {
                let (gate, reason) = base_gate(r, target, flags, false, false, key)?;
                receiver.gate = Gate::Direct(gate);
                reason
            }
            Kind::Drag => {
                let state = r.i32(target + 0x9c)?;
                receiver.gate = Gate::Drag { state };
                if state == 0 && flags & 0x6000 == 0 {
                    None
                } else {
                    Some("unsupported_keyboard_drag_gate")
                }
            }
            Kind::InstantTextBox => {
                // Its exact handler is passive for ordinary key event kind 1
                // on all optional-pointer branches. No CButton fields apply.
                receiver.gate = Gate::InstantTextBox;
                None
            }
        };
        if let Some(reason) = reason {
            out.reason = reason;
        }
        out.prefix.push(Node {
            renderer: source,
            target,
            receiver: Some(receiver),
        });
        if source == renderer || out.reason != key.ready() {
            break;
        }
    }
    Ok(out)
}
impl Observation {
    pub(super) fn require_ready(&self) -> Result<()> {
        if self.reason != self.key.ready() {
            return Err(invalid(self.reason));
        }
        Ok(())
    }
    pub(super) fn diagnostic(&self) -> Value {
        json!({"key":self.key.label(),"source_count":self.sources.len(),"inspected_receiver_count":self.prefix.len(),
            "focused_receiver_present":self.focused.iter().any(|v|*v!=0),"reason":self.reason,
            "ordinary_route_observed":self.reason==self.key.ready(),
            "complete_source_membership_observed":!self.sources.is_empty(),
            "keyboard_handler_enabled_raw":self.gui_flags[0],"event_flag_raw":self.gui_flags[1],
            "conditional_dispatch_raw":self.gui_flags[2],"skip_dispatch_raw":self.gui_flags[3],
            "input_authorized":false,"cache_completion_verified":false,"delivery_verified":false,
            "cursor_movement_required":false})
    }
}
