//! Live GUI widget tree and screen rectangles for ordinary input geometry.
//! Read-only: names come from each widget's GUI type, rectangles from the
//! laid-out sprite renderer (bottom-left translation in centre-origin y-up
//! pixels plus the frame size). Rectangles locate a click target; they
//! never authorize input, and every action must still be verified by fresh
//! game-state readback. Only builds with a reviewed layout are supported.
use crate::{
    error::{Error, Result},
    memory::{Reader, StringAbi},
};

/// Widget-tree layout of one inspected build.
#[derive(Debug)]
pub(crate) struct Layout {
    container_vtable: u64,
    /// Offset of the GUI-object base inside a container window object.
    container_base: u64,
    /// GUI type pointer and type-name string, relative to a GUI-object base.
    widget_type: u64,
    type_name: u64,
    /// Children array and visibility byte, relative to the container object.
    children: u64,
    visibility: u64,
    visible_bit: u8,
    /// Renderer pointer per exact widget primary vtable.
    renderers: &'static [(u64, u64)],
    /// Typed sprites: size and frame count on the sprite type.
    sprite_vtables: &'static [u64],
    /// Text sprites: size on the sprite itself.
    text_sprite_vtable: u64,
    text_sprite_size: u64,
    sprite_translation: u64,
    sprite_type: u64,
    sprite_size: u64,
    sprite_frames: u64,
    /// Button state; the engine stores 3 for a disabled button.
    button_state: u64,
}
/// macOS Mach-O 089EA519 (libc++), mapped live on 18 October 2026 from the
/// topbar: children/type/name joins, a visible/hidden split of named views,
/// and sprite rectangles matching the topbar .gui positions; then confirmed by
/// clicks on a research slot, a technology tile and the Research button.
const MACOS: Layout = Layout {
    container_vtable: 0x34ac0b0,
    container_base: 0x30,
    widget_type: 0x50,
    type_name: 0x28,
    children: 0xd30,
    visibility: 0x95,
    visible_bit: 8,
    // CIcon, CButtonStandard.
    renderers: &[(0x34a81a8, 0xd0), (0x34aafa0, 0xd8)],
    // CSprite, CFrameAnimatedSprite.
    sprite_vtables: &[0x34bdb10, 0x34afed0],
    text_sprite_vtable: 0x34c0380,
    text_sprite_size: 0x1b0,
    sprite_translation: 0xb0,
    sprite_type: 0x8,
    sprite_size: 0x8c,
    sprite_frames: 0x78,
    button_state: 0x9c,
};
pub(crate) fn layout(r: &Reader<'_>) -> Result<&'static Layout> {
    match r.string_abi() {
        StringAbi::LibCxx => Ok(&MACOS),
        StringAbi::LibStdCxx => Err(Error::new(
            "capability_unavailable",
            "GUI rectangles are not reviewed for this build",
        )),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}
impl Rect {
    pub fn center(&self) -> (f64, f64) {
        (
            f64::from(self.x) + f64::from(self.w) / 2.0,
            f64::from(self.y) + f64::from(self.h) / 2.0,
        )
    }
}

fn pointer(p: u64) -> Result<u64> {
    if !(0x10000..=0x7fff_ffff_0000).contains(&p) || p % 8 != 0 {
        return Err(Error::new(
            "invalid_gui",
            "Bounded aligned GUI pointer required",
        ));
    }
    Ok(p)
}

/// Widget name from its GUI type, or None for an untyped widget.
pub(crate) fn name(r: &Reader<'_>, base: u64, image: u64) -> Result<Option<String>> {
    let l = layout(r)?;
    let kind = r.u64(base + l.widget_type)?;
    if kind == 0 {
        return Ok(None);
    }
    pointer(kind)?;
    // The type's own primary must be an image vtable; anything else is not a GUI type.
    if !(image..image + 0x400_0000).contains(&r.u64(kind)?) {
        return Ok(None);
    }
    let name = r.string_bounded(kind + l.type_name, 128)?;
    Ok((!name.is_empty()).then_some(name))
}

/// Container object for a container pointer stored either as the object or as
/// its GUI-object base; None for any other widget.
pub(crate) fn container(r: &Reader<'_>, pointer: u64, image: u64) -> Result<Option<u64>> {
    let l = layout(r)?;
    if r.u64(pointer)? == image + l.container_vtable {
        return Ok(Some(pointer));
    }
    let object = pointer.wrapping_sub(l.container_base);
    Ok((r.u64(object)? == image + l.container_vtable).then_some(object))
}
/// Whether a button is disabled by the game (state 3), e.g. Research for a
/// technology whose prerequisites are missing.
pub(crate) fn disabled(r: &Reader<'_>, button: u64) -> Result<bool> {
    Ok(r.i32(button + layout(r)?.button_state)? == 3)
}
/// Direct child of a container object with this name.
pub(crate) fn child(r: &Reader<'_>, image: u64, object: u64, wanted: &str) -> Result<Option<u64>> {
    for child in children(r, object)? {
        if name(r, child, image)?.as_deref() == Some(wanted) {
            return Ok(Some(child));
        }
    }
    Ok(None)
}

pub(crate) fn visible(r: &Reader<'_>, object: u64) -> Result<bool> {
    let l = layout(r)?;
    Ok(r.byte(object + l.visibility)? & l.visible_bit != 0)
}

/// Child GUI-object bases of a container object, in engine order.
pub(crate) fn children(r: &Reader<'_>, object: u64) -> Result<Vec<u64>> {
    let l = layout(r)?;
    let (_, data, count) = r.array_header(object + l.children)?;
    if count > 4096 {
        return Err(Error::new("invalid_gui", "GUI child list exceeds bound"));
    }
    let bytes = if count == 0 {
        Vec::new()
    } else {
        r.read(data, count * 8)?
    };
    bytes
        .chunks_exact(8)
        .map(|b| pointer(u64::from_le_bytes(b.try_into().unwrap())))
        .collect()
}

/// Screen rectangle in game pixels of a widget with a laid-out sprite.
pub(crate) fn rect(r: &Reader<'_>, image: u64, base: u64, screen: (u32, u32)) -> Result<Rect> {
    let Rect { x, y, w, h } = rect_unclipped(r, image, base, screen)?;
    let inside = x + w > 0 && y + h > 0 && x < screen.0 as i32 && y < screen.1 as i32;
    if !inside {
        return Err(Error::new(
            "gui_geometry_unavailable",
            "Widget is off screen",
        ));
    }
    Ok(Rect { x, y, w, h })
}
/// Rectangle without the on-screen requirement (for panning targets into view).
pub(crate) fn rect_unclipped(
    r: &Reader<'_>,
    image: u64,
    base: u64,
    screen: (u32, u32),
) -> Result<Rect> {
    let l = layout(r)?;
    let primary = r.u64(base)?;
    let Some(&(_, offset)) = l.renderers.iter().find(|(vt, _)| image + vt == primary) else {
        return Err(Error::new(
            "gui_geometry_unavailable",
            "Widget class has no reviewed renderer",
        ));
    };
    let sprite = pointer(r.u64(base + offset)?)?;
    let renderer = r.u64(sprite)?;
    let (w, h) = if renderer == image + l.text_sprite_vtable {
        (
            r.i32(sprite + l.text_sprite_size)?,
            r.i32(sprite + l.text_sprite_size + 4)?,
        )
    } else if l.sprite_vtables.iter().any(|vt| image + vt == renderer) {
        let kind = pointer(r.u64(sprite + l.sprite_type)?)?;
        let frames = r.i32(kind + l.sprite_frames)?;
        if !(1..=64).contains(&frames) {
            return Err(Error::new(
                "invalid_gui_geometry",
                "Bounded frame count required",
            ));
        }
        (
            r.i32(kind + l.sprite_size)? / frames,
            r.i32(kind + l.sprite_size + 4)?,
        )
    } else {
        return Err(Error::new(
            "gui_geometry_unavailable",
            "Widget renderer is not a reviewed sprite",
        ));
    };
    let raw = r.read(sprite + l.sprite_translation, 8)?;
    let tx = f32::from_le_bytes(raw[..4].try_into().unwrap());
    let ty = f32::from_le_bytes(raw[4..].try_into().unwrap());
    if !tx.is_finite() || !ty.is_finite() || !(1..=4096).contains(&w) || !(1..=4096).contains(&h) {
        return Err(Error::new(
            "invalid_gui_geometry",
            "Bounded sprite geometry required",
        ));
    }
    let x = (f64::from(screen.0) / 2.0 + f64::from(tx)).round() as i32;
    let y = (f64::from(screen.1) / 2.0 - f64::from(ty)).round() as i32 - h;
    Ok(Rect { x, y, w, h })
}

/// Rectangle of a widget inside a zoomed view: the renderer translation is
/// already scaled, the texture size is not.
pub(crate) fn rect_zoomed(
    r: &Reader<'_>,
    image: u64,
    base: u64,
    screen: (u32, u32),
    zoom: f32,
) -> Result<Rect> {
    if !(0.1..=4.0).contains(&zoom) {
        return Err(Error::new(
            "invalid_gui_geometry",
            "Bounded view zoom required",
        ));
    }
    let plain = rect_unclipped(r, image, base, screen)?;
    let w = (f64::from(plain.w) * f64::from(zoom)).round() as i32;
    let h = (f64::from(plain.h) * f64::from(zoom)).round() as i32;
    Ok(Rect {
        x: plain.x,
        y: plain.y + plain.h - h,
        w,
        h,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::ReadMemory;
    use std::collections::BTreeMap;

    /// Sparse little-endian memory with the macOS string ABI.
    #[derive(Default)]
    struct Fake(BTreeMap<u64, u8>);
    impl Fake {
        fn put(&mut self, a: u64, bytes: &[u8]) {
            for (i, b) in bytes.iter().enumerate() {
                self.0.insert(a + i as u64, *b);
            }
        }
        fn q(&mut self, a: u64, v: u64) {
            self.put(a, &v.to_le_bytes());
        }
        fn short(&mut self, a: u64, s: &str) {
            let mut b = vec![(s.len() as u8) << 1];
            b.extend_from_slice(s.as_bytes());
            b.resize(24, 0);
            self.put(a, &b);
        }
    }
    impl ReadMemory for Fake {
        fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
            Ok((0..n as u64)
                .map(|i| *self.0.get(&(a + i)).unwrap_or(&0))
                .collect())
        }
        fn string_abi(&self) -> StringAbi {
            StringAbi::LibCxx
        }
    }
    const IMAGE: u64 = 0x1_0000_0000;

    fn topbar() -> Fake {
        let mut m = Fake::default();
        let (bar, icon, sprite, kind, ty_bar, ty_icon, list) = (
            0x20_0000u64,
            0x21_0000,
            0x22_0000,
            0x23_0000,
            0x24_0000,
            0x25_0000,
            0x26_0000,
        );
        m.q(bar, IMAGE + MACOS.container_vtable);
        m.put(bar + MACOS.visibility, &[8]);
        m.q(bar + 0x30 + MACOS.widget_type, ty_bar);
        m.q(ty_bar, IMAGE + 0x100);
        m.short(ty_bar + MACOS.type_name, "topbar");
        m.q(bar + MACOS.children, list);
        m.put(bar + MACOS.children + 8, &[1, 0, 0, 0, 1, 0, 0, 0]);
        m.q(list, icon);
        m.q(icon, IMAGE + 0x34a81a8);
        m.q(icon + MACOS.widget_type, ty_icon);
        m.q(ty_icon, IMAGE + 0x200);
        m.short(ty_icon + MACOS.type_name, "pol_power_value_bg");
        m.q(icon + 0xd0, sprite);
        m.q(sprite, IMAGE + 0x34bdb10);
        m.put(sprite + 0xb0, &(-864f32).to_le_bytes());
        m.put(sprite + 0xb4, &510f32.to_le_bytes());
        m.q(sprite + 8, kind);
        m.put(kind + 0x78, &[1, 0, 0, 0]);
        m.put(kind + 0x8c, &[70, 0, 0, 0, 26, 0, 0, 0]);
        m
    }

    #[test]
    fn finds_named_child_and_its_screen_rectangle() {
        let m = topbar();
        let r = Reader::new(&m);
        let icon = child(&r, IMAGE, 0x20_0000, "pol_power_value_bg")
            .unwrap()
            .unwrap();
        assert_eq!(icon, 0x21_0000);
        let rect = rect(&r, IMAGE, icon, (1920, 1080)).unwrap();
        assert_eq!(
            rect,
            Rect {
                x: 96,
                y: 4,
                w: 70,
                h: 26
            }
        );
        assert_eq!(rect.center(), (131.0, 17.0));
    }
    #[test]
    fn frames_split_the_texture_and_text_sprites_carry_their_own_size() {
        let mut m = topbar();
        m.put(0x23_0000 + 0x78, &[2, 0, 0, 0]);
        let r = Reader::new(&m);
        assert_eq!(rect(&r, IMAGE, 0x21_0000, (1920, 1080)).unwrap().w, 35);
        let mut m = topbar();
        m.q(0x22_0000, IMAGE + MACOS.text_sprite_vtable);
        m.put(0x22_0000 + 0x1b0, &[123, 0, 0, 0, 36, 0, 0, 0]);
        let r = Reader::new(&m);
        assert_eq!(
            rect(&r, IMAGE, 0x21_0000, (1920, 1080)).unwrap(),
            Rect {
                x: 96,
                y: -6,
                w: 123,
                h: 36
            }
        );
    }
    #[test]
    fn containers_resolve_from_either_pointer_form() {
        let m = topbar();
        let r = Reader::new(&m);
        assert_eq!(container(&r, 0x20_0000, IMAGE).unwrap(), Some(0x20_0000));
        assert_eq!(container(&r, 0x20_0030, IMAGE).unwrap(), Some(0x20_0000));
        assert_eq!(container(&r, 0x21_0000, IMAGE).unwrap(), None);
        assert_eq!(
            child(&r, IMAGE, 0x20_0000, "pol_power_value_bg").unwrap(),
            Some(0x21_0000)
        );
    }
    #[test]
    fn hidden_containers_and_unknown_names_are_reported() {
        let mut m = topbar();
        let r = Reader::new(&m);
        assert!(child(&r, IMAGE, 0x20_0000, "missing").unwrap().is_none());
        assert!(visible(&r, 0x20_0000).unwrap());
        m.put(0x20_0000 + MACOS.visibility, &[0]);
        let r = Reader::new(&m);
        assert!(!visible(&r, 0x20_0000).unwrap());
    }
    #[test]
    fn off_screen_and_unreviewed_renderers_refuse() {
        let mut m = topbar();
        m.put(0x22_0000 + 0xb0, &(-2000f32).to_le_bytes());
        let r = Reader::new(&m);
        assert_eq!(
            rect(&r, IMAGE, 0x21_0000, (1920, 1080)).unwrap_err().code,
            "gui_geometry_unavailable"
        );
        let mut m = topbar();
        m.q(0x21_0000, IMAGE + 0x999);
        let r = Reader::new(&m);
        assert!(rect(&r, IMAGE, 0x21_0000, (1920, 1080)).is_err());
    }
}
