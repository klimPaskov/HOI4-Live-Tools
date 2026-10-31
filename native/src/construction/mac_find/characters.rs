//! Ordinary printable-character forwarding through the owned outer receiver.
//! This is not a keyboard route, text-delivery or input-authority proof. The
//! exact empty Robin-Hood sentinel path skips capture/listener/stored replay
//! for every key; current Find focus and query are checked separately.
use crate::{
    construction::{mac_ui::exact, pointer},
    error::{Error, Result},
    game,
    memory::{Reader, StringAbi},
};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Mode {
    Direct,
    Disabled,
    Empty,
}

/// Immutable owner/receiver identity; no render-list or transient prefix cache.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Binding {
    base: u64,
    edges: Vec<(u64, u64)>,
    idler: u64,
    pump: u64,
    receiver: u64,
    mode: Mode,
}

pub(super) enum ReadGuard<'a> {
    Bind,
    Original(&'a Binding),
}

fn changed(message: &str) -> Error {
    Error::new("construction_state_changed", message)
}

fn image_base(base: u64) -> Result<()> {
    if base < 0x10000
        || base % 0x1000 != 0
        || base
            .checked_add(0x384b760)
            .is_none_or(|end| end > 0x7fff_ffff_0000)
    {
        return Err(changed("Bounded aligned character image base required"));
    }
    Ok(())
}

fn owned(r: &Reader<'_>, at: u64, expected: u64, edges: &mut Vec<(u64, u64)>) -> Result<()> {
    if r.u64(at)? != expected {
        return Err(changed("Owned character receiver edge differs"));
    }
    edges.push((at, expected));
    Ok(())
}

fn edge(r: &Reader<'_>, at: u64, edges: &mut Vec<(u64, u64)>) -> Result<u64> {
    let value = r.u64(at)?;
    edges.push((at, value));
    Ok(value)
}

fn ordinary(r: &Reader<'_>, base: u64, idler: u64, pump: u64) -> Result<()> {
    if r.byte(idler + 0x7e1)? != 1 || r.byte(base + 0x372b2b8)? != 0 || r.byte(pump + 0x44)? != 0 {
        return Err(changed("Ordinary owned character dispatch required"));
    }
    Ok(())
}

fn mode(r: &Reader<'_>, base: u64, receiver: u64, original: Option<&Mode>) -> Result<Mode> {
    if receiver == 0 {
        return Ok(Mode::Direct);
    }
    let flag = r.byte(base + 0x37ea780)?;
    let current = match flag {
        0 => Mode::Disabled,
        1 => Mode::Empty,
        _ => return Err(changed("Unsupported HotkeyManager character flag")),
    };
    if original.is_some_and(|before| *before != current) {
        return Err(changed("Original character forwarding mode changed"));
    }
    if current == Mode::Disabled {
        // 1026ebf10 flag0 tails to original receiver20 without +64/table reads.
        return Ok(current);
    }
    if r.i32(receiver + 0x64)? != 0 {
        return Err(changed("HotkeyManager character capture is not idle"));
    }
    // The defined weak symbol may be coalesced/rebound. Require both current
    // aliases to the exact own non-polymorphic storage BEFORE either state.
    let sentinel = base + 0x370ce20;
    if r.u64(base + 0x3477260)? != sentinel || r.u64(receiver + 8)? != sentinel {
        return Err(changed("Original owned character sentinel aliases differ"));
    }
    // Widths follow lookup1026eb390 and constructor1026ea8a0. No bucket key,
    // listener, timer or saved-event payload is followed by the empty path.
    if r.i32(receiver + 0x10)? != 0 || r.i32(receiver + 0x14)? != 0 || r.byte(receiver + 0x18)? != 0
    {
        return Err(changed("Exact empty character table headers required"));
    }
    if r.byte(sentinel + 4)? != 0 || r.byte(sentinel + 0x2c)? != 0xff {
        return Err(changed("Exact empty character sentinel states required"));
    }
    Ok(current)
}

impl Binding {
    /// Original aliases/classes first, before optional replacement payload.
    pub(super) fn require_original(&self, r: &Reader<'_>, base: u64) -> Result<()> {
        image_base(base)?;
        if base != self.base {
            return Err(changed("Original character image changed"));
        }
        for &(at, value) in &self.edges {
            if r.u64(at)? != value {
                return Err(changed("Original character owner changed before payload"));
            }
        }
        ordinary(r, base, self.idler, self.pump)?;
        if mode(r, base, self.receiver, Some(&self.mode))? != self.mode {
            return Err(changed("Original character forwarding mode changed"));
        }
        Ok(())
    }

    pub(super) fn forwarding_kind(&self) -> &'static str {
        match self.mode {
            Mode::Direct => "original_receiver_direct",
            Mode::Disabled => "owned_hotkeys_flag0",
            Mode::Empty => "owned_hotkeys_flag1_exact_empty_table",
        }
    }
}

impl ReadGuard<'_> {
    pub(super) fn before_context(&self, r: &Reader<'_>, base: u64) -> Result<()> {
        match self {
            Self::Bind => Ok(()),
            Self::Original(binding) => binding.require_original(r, base),
        }
    }

    pub(super) fn observe(
        &self,
        r: &Reader<'_>,
        base: u64,
        idler: u64,
        gui: u64,
    ) -> Result<Binding> {
        let binding = observe(r, base, idler, gui)?;
        if let Self::Original(original) = self {
            if **original != binding {
                return Err(changed("Original character binding changed"));
            }
        }
        Ok(binding)
    }
}

fn observe(r: &Reader<'_>, base: u64, idler: u64, gui: u64) -> Result<Binding> {
    if r.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "unsupported_platform",
            "Mac character layout required",
        ));
    }
    image_base(base)?;
    pointer(idler)?;
    pointer(gui)?;
    let mut edges = Vec::new();
    owned(r, base + game::MACOS.idler_global, idler, &mut edges)?;
    owned(r, idler, base + 0x35089d8, &mut edges)?;
    owned(r, base + 0x35089d8 + 0x20, base + 0x7bb2c0, &mut edges)?;
    owned(r, idler + 0x4c0, gui, &mut edges)?;
    owned(r, gui, base + 0x36a5810, &mut edges)?;
    owned(r, base + 0x37eda08, gui, &mut edges)?;
    let gfx = edge(r, gui + 0x98, &mut edges)?;
    pointer(gfx)?;
    owned(r, gfx, base + 0x35e5b58, &mut edges)?;
    let application = edge(r, idler + 0x4f8, &mut edges)?;
    pointer(application)?;
    owned(r, base + 0x384b758, application, &mut edges)?;
    owned(r, application + 0x38, idler, &mut edges)?;
    owned(r, application + 0x300, gfx, &mut edges)?;
    owned(r, application + 0x308, gui, &mut edges)?;
    let pump = edge(r, base + 0x37ea608, &mut edges)?;
    pointer(pump)?;
    owned(r, pump, base + 0x36b5160, &mut edges)?;
    ordinary(r, base, idler, pump)?;
    let receiver = edge(r, base + 0x37ea778, &mut edges)?;
    if receiver != 0 {
        pointer(receiver)?;
        // Unknown primary refuses before its stage/table or any listener data.
        exact(r, base, receiver, 0x36b6490)?;
        edges.push((receiver, base + 0x36b6490));
        owned(r, base + 0x36b6490 + 0x20, base + 0x2867050, &mut edges)?;
    }
    let mode = mode(r, base, receiver, None)?;
    Ok(Binding {
        base,
        edges,
        idler,
        pump,
        receiver,
        mode,
    })
}

#[cfg(test)]
mod tests;
