//! Read-only geometry for Mach-O 089EA519. Independently recovered from this
//! build's CGui/CGraphics constructors and SDL Cocoa creation/move/resize code.
//! SDL caches contentRectForFrameRect, not the enclosing CG window rectangle.
//! The OS input backend must separately corroborate this content rectangle.
use crate::{
    error::{Error, Result},
    memory::{Reader, StringAbi},
    platform::Process,
};
use serde_json::{json, Value};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Geometry {
    // Internal ownership witnesses; never exposed in tool responses.
    pub objects: [u64; 10],
    pub physical: [i32; 2],
    pub logical: [i32; 2],
    pub scale: f32,
    pub client: [i32; 4],
    pub flags: u32,
}
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
impl Geometry {
    pub fn diagnostic(&self) -> Value {
        json!({
            "source":"native own GUI and SDL Cocoa diagnostics",
            "build_id":"089EA519-6780-353B-8E39-4E9309B444D4",
            "physical_dimensions":self.physical,"logical_dimensions":self.logical,
            "gui_scale":self.scale,"native_content_rectangle":self.client,
            "window_flags":self.flags,"cocoa_backlinks_verified":true,
            "owned_os_window_verified":false,"input_mapping_verified":false,
            "unverified":["current OS window association","effective hit routing"],
            "observation_paused_game":false
        })
    }
    /// The ordinary Mac adapters currently have one reviewed logical profile.
    /// Observation is still allowed when this stricter input profile refuses.
    pub fn require_profile(&self) -> Result<()> {
        if self.physical != [1920, 1080]
            || self.logical != self.physical
            || self.client[2..] != self.physical
            || self.scale != 1.0
        {
            return Err(Error::new(
                "input_geometry_unverified",
                "Mac input requires matching native and logical 1920x1080 at GUI scale 1",
            ));
        }
        // Accept only the observed OpenGL, shown, input-focused window mode.
        // Cocoa handlers prove SHOWN=4, HIDDEN=8, MINIMIZED=0x40 and
        // INPUT_FOCUS=0x200. Mouse focus (0x400) may change with cursor motion.
        // Other window modes require separate qualification.
        if self.flags & 0x206 != 0x206 || self.flags & !0x606 != 0 {
            return Err(Error::new(
                "input_target_not_frontmost",
                "Mac input requires the shown focused windowed SDL game",
            ));
        }
        Ok(())
    }
    pub fn same_binding(&self, other: &Self) -> bool {
        self.objects == other.objects
            && self.physical == other.physical
            && self.logical == other.logical
            && self.scale == other.scale
            && self.client == other.client
    }
}
fn pointer(p: u64) -> Result<u64> {
    super::source_pointer(p)
}
fn require(ok: bool, message: &'static str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(Error::new("invalid_gui_geometry", message))
    }
}
fn ivar(r: &Reader<'_>, base: u64, offset: u64) -> Result<u64> {
    // ObjC may relocate subclass ivars after AppKit superclass reconciliation.
    // Values observed in this build are not the on-disk initial value of 8.
    let value = r.u64(base + offset)?;
    require(
        (8..=4096).contains(&value) && value % 8 == 0,
        "Bounded runtime Cocoa ivar offset required",
    )?;
    Ok(value)
}
/// The canvas is an own-GUI graphics association, not an OS window or input
/// grant. Callers retain it inside their encompassing final-witness bracket.
pub(crate) struct Canvas {
    pub object: u64,
    pub physical: [i32; 2],
    pub logical: [i32; 2],
    pub scale: f32,
}
pub(crate) fn canvas_from_reader(r: &Reader<'_>, base: u64, gui: u64) -> Result<Canvas> {
    require(
        r.string_abi() == StringAbi::LibCxx,
        "Mac canvas requires the reviewed Mach-O layout",
    )?;
    pointer(gui)?;
    require(
        r.u64(gui)? == base + 0x34974d0,
        "Exact Mac game GUI required",
    )?;
    // CGui ctor 0x27c4739 stores its graphics argument here.
    let graphics = pointer(r.u64(gui + 0x98)?)?;
    require(
        r.u64(graphics)? == base + 0x33dafa8,
        "Exact Mac game graphics required",
    )?;
    let dimensions = r.read(graphics + 0x170, 16)?;
    let dims: Vec<i32> = dimensions
        .chunks_exact(4)
        .map(|v| i32::from_le_bytes(v.try_into().unwrap()))
        .collect();
    require(
        dims.iter().all(|v| (1..=16384).contains(v)),
        "Bounded graphics dimensions required",
    )?;
    // Constructor 0x27b1161, consumed by the scale transform at 0x27b1223.
    let scale = f32::from_le_bytes(r.read(graphics + 0x2c5b0, 4)?.try_into().unwrap());
    require(
        scale.is_finite() && (0.25..=8.0).contains(&scale),
        "Bounded finite GUI scale required",
    )?;
    Ok(Canvas {
        object: graphics,
        physical: [dims[0], dims[1]],
        logical: [dims[2], dims[3]],
        scale,
    })
}
/// Caller must finish a Witnesses bracket; no build offsets from Linux apply.
pub(crate) fn from_reader(r: &Reader<'_>, base: u64) -> Result<Geometry> {
    require(
        r.string_abi() == StringAbi::LibCxx,
        "Mac geometry requires the reviewed Mach-O layout",
    )?;
    let idler = pointer(r.u64(base + 0x35196c0)?)?;
    require(
        r.u64(idler)? == base + 0x3308f90,
        "In-game Mac idler required",
    )?;
    let gui = pointer(r.u64(idler + 0x4c0)?)?;
    let canvas = canvas_from_reader(r, base, gui)?;
    let graphics = canvas.object;
    let video = pointer(r.u64(base + 0x3639800)?)?;
    // Cocoa_CreateDevice constructor callback stores, not borrowed Linux offsets.
    for (offset, function) in [(0x8, 0x263fea0), (0x48, 0x2643a50), (0x128, 0x2644350)] {
        require(
            r.u64(video + offset)? == base + function,
            "Reviewed Cocoa video backend required",
        )?;
    }
    let backend = pointer(r.u64(video + 0x5a8)?)?;
    let mut window = r.u64(video + 0x2c8)?;
    let mut seen = Vec::new();
    let mut selected = None;
    while window != 0 {
        pointer(window)?;
        require(
            seen.len() < 8 && !seen.contains(&window),
            "Bounded acyclic SDL window list required",
        )?;
        seen.push(window);
        require(
            r.u64(window)? == video + 0x2d8,
            "SDL window belongs to another device",
        )?;
        let bytes = r.read(window + 0x20, 16)?;
        let rect: Vec<i32> = bytes
            .chunks_exact(4)
            .map(|v| i32::from_le_bytes(v.try_into().unwrap()))
            .collect();
        if rect[2..] == canvas.physical {
            require(selected.is_none(), "Ambiguous SDL game window dimensions")?;
            require(
                rect[..2].iter().all(|v| v.unsigned_abs() <= 1_000_000),
                "Bounded native content origin required",
            )?;
            let data = pointer(r.u64(window + 0xd8)?)?;
            let nswindow = pointer(r.u64(data + 8)?)?;
            let view = pointer(r.u64(data + 0x10)?)?;
            let listener = pointer(r.u64(data + 0x30)?)?;
            require(
                r.u64(data)? == window && r.u64(data + 0x38)? == backend,
                "Reciprocal Cocoa driver association required",
            )?;
            let view_offset = ivar(r, base, 0x34ce008)?;
            let listener_offset = ivar(r, base, 0x34cdfa8)?;
            require(
                r.u64(view + view_offset)? == window && r.u64(listener + listener_offset)? == data,
                "Reciprocal Cocoa view/listener association required",
            )?;
            selected = Some(Geometry {
                objects: [
                    idler, gui, graphics, video, backend, window, data, nswindow, view, listener,
                ],
                physical: canvas.physical,
                logical: canvas.logical,
                scale: canvas.scale,
                client: rect.try_into().unwrap(),
                flags: r.i32(window + 0x40)? as u32,
            });
        }
        window = r.u64(window + 0xe8)?;
    }
    selected.ok_or_else(|| {
        Error::new(
            "input_geometry_unavailable",
            "No SDL window matches the live graphics dimensions",
        )
    })
}

/// Fresh complete geometry witness, reused by the OS input guard.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn observe(process: &dyn Process) -> Result<Geometry> {
    process.begin_snapshot()?;
    let witnesses = crate::research_ui::Witnesses::new(process);
    let result = from_reader(&Reader::new(&witnesses), process.image_base())?;
    witnesses.finish()?;
    process.check()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::ReadMemory;
    use std::collections::BTreeMap;
    const B: u64 = 0x1_0000_0000;
    const I: u64 = 0x2_0000_0000;
    const G: u64 = I + 0x1000;
    const R: u64 = I + 0x10000;
    const V: u64 = I + 0x50000;
    const W: u64 = I + 0x60000;
    const D: u64 = I + 0x70000;
    const N: u64 = I + 0x80000;
    const VIEW: u64 = I + 0x90000;
    const LISTENER: u64 = I + 0xa0000;
    const BACKEND: u64 = I + 0xb0000;
    struct Fixture {
        bytes: BTreeMap<u64, u8>,
        abi: StringAbi,
        changed: Option<u64>,
    }
    impl Fixture {
        fn put(&mut self, address: u64, bytes: &[u8]) {
            for (i, b) in bytes.iter().enumerate() {
                self.bytes.insert(address + i as u64, *b);
            }
        }
        fn word(&mut self, address: u64, value: u64) {
            self.put(address, &value.to_le_bytes());
        }
        fn new() -> Self {
            let mut f = Self {
                bytes: BTreeMap::new(),
                abi: StringAbi::LibCxx,
                changed: None,
            };
            for (address, value) in [
                (B + 0x35196c0, I),
                (I, B + 0x3308f90),
                (I + 0x4c0, G),
                (G, B + 0x34974d0),
                (G + 0x98, R),
                (R, B + 0x33dafa8),
                (B + 0x3639800, V),
                (V + 0x5a8, BACKEND),
                (V + 8, B + 0x263fea0),
                (V + 0x48, B + 0x2643a50),
                (V + 0x128, B + 0x2644350),
                (V + 0x2c8, W),
                (W, V + 0x2d8),
                (W + 0xd8, D),
                (W + 0xe8, 0),
                (D, W),
                (D + 8, N),
                (D + 0x10, VIEW),
                (D + 0x30, LISTENER),
                (D + 0x38, BACKEND),
                (B + 0x34ce008, 536),
                (B + 0x34cdfa8, 32),
                (VIEW + 536, W),
                (LISTENER + 32, D),
            ] {
                f.word(address, value);
            }
            f.put(
                R + 0x170,
                &[1920i32, 1080, 1920, 1080]
                    .into_iter()
                    .flat_map(i32::to_le_bytes)
                    .collect::<Vec<_>>(),
            );
            f.put(R + 0x2c5b0, &1.0f32.to_le_bytes());
            f.put(
                W + 0x20,
                &[0i32, 81, 1920, 1080]
                    .into_iter()
                    .flat_map(i32::to_le_bytes)
                    .collect::<Vec<_>>(),
            );
            f.put(W + 0x40, &0x606u32.to_le_bytes());
            f
        }
    }
    impl ReadMemory for Fixture {
        fn string_abi(&self) -> StringAbi {
            self.abi
        }
        fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
            (0..n)
                .map(|i| {
                    self.bytes
                        .get(&(a + i as u64))
                        .copied()
                        .ok_or_else(|| Error::new("short_read", "Missing fixture byte"))
                })
                .collect()
        }
        fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
            requests
                .iter()
                .map(|&(a, n)| {
                    let mut bytes = self.read(a, n)?;
                    if let Some(target) = self.changed {
                        if (a..a + n as u64).contains(&target) {
                            bytes[(target - a) as usize] ^= 1;
                        }
                    }
                    Ok(bytes)
                })
                .collect()
        }
    }
    impl Process for Fixture {
        fn check(&self) -> Result<()> {
            Ok(())
        }
        fn image_base(&self) -> u64 {
            B
        }
    }
    #[test]
    fn runtime_ivars_and_native_content_are_read_without_input() {
        let f = Fixture::new();
        let g = observe(&f).unwrap();
        assert_eq!(g.client, [0, 81, 1920, 1080]);
        g.require_profile().unwrap();
        let report = crate::gui::input_geometry(&f).unwrap();
        assert_eq!(report["cocoa_backlinks_verified"], true);
        assert_eq!(report["input_mapping_verified"], false);
        assert_eq!(report["observation_paused_game"], false);
        assert!(report["freshness"]["read_bytes"].as_u64().unwrap() < 1024);
        assert!(!report.to_string().contains(&W.to_string()));
    }
    #[test]
    fn other_abi_refuses_before_layout_reads() {
        let mut f = Fixture::new();
        f.abi = StringAbi::LibStdCxx;
        let r = Reader::new(&f);
        assert!(from_reader(&r, B).is_err());
        assert_eq!(r.calls.get(), 0);
    }
    #[test]
    fn every_dependency_must_survive_final_witness() {
        for address in [
            B + 0x35196c0,
            I,
            G,
            R,
            G + 0x98,
            R + 0x170,
            R + 0x178,
            R + 0x2c5b0,
            B + 0x3639800,
            V + 8,
            V + 0x48,
            V + 0x128,
            V + 0x5a8,
            V + 0x2c8,
            W,
            W + 0x20,
            W + 0x40,
            W + 0xd8,
            W + 0xe8,
            D,
            D + 8,
            D + 0x10,
            D + 0x30,
            D + 0x38,
            B + 0x34ce008,
            B + 0x34cdfa8,
            VIEW + 536,
            LISTENER + 32,
        ] {
            let mut f = Fixture::new();
            f.changed = Some(address);
            assert_eq!(observe(&f).unwrap_err().code, "torn_read", "{address:x}");
        }
    }
    #[test]
    fn wrong_objects_backlinks_or_backend_refuse() {
        for address in [
            I,
            G,
            R,
            V + 8,
            V + 0x48,
            V + 0x128,
            W,
            D,
            D + 0x38,
            VIEW + 536,
            LISTENER + 32,
        ] {
            let mut f = Fixture::new();
            f.word(address, 0);
            assert!(observe(&f).is_err(), "{address:x}");
        }
        for address in [B + 0x34ce008, B + 0x34cdfa8] {
            for value in [0, 7, 4097, u64::MAX] {
                let mut f = Fixture::new();
                f.word(address, value);
                assert!(observe(&f).is_err());
            }
        }
    }
    #[test]
    fn cycle_ambiguity_and_missing_matching_window_refuse() {
        let mut f = Fixture::new();
        f.word(W + 0xe8, W);
        assert!(observe(&f).is_err());
        let mut f = Fixture::new();
        f.word(V + 0x2c8, 0);
        assert_eq!(observe(&f).unwrap_err().code, "input_geometry_unavailable");
        let mut f = Fixture::new();
        let second = W + 0x1000;
        f.word(W + 0xe8, second);
        f.word(second, V + 0x2d8);
        f.put(second + 0x20, &f.read(W + 0x20, 16).unwrap());
        assert!(observe(&f).unwrap_err().message.contains("Ambiguous"));
    }
    #[test]
    fn profile_refuses_scaling_and_inactive_window_but_allows_mouse_focus_changes() {
        let g = observe(&Fixture::new()).unwrap();
        for flags in [0, 0x206 | 0x8, 0x206 | 0x40, 0x206 | 1, 0x406] {
            let mut other = g.clone();
            other.flags = flags;
            assert!(other.require_profile().is_err());
        }
        let mut other = g.clone();
        other.flags = 0x206;
        other.require_profile().unwrap();
        assert!(g.same_binding(&other));
        other.scale = 1.25;
        assert!(other.require_profile().is_err());
        assert!(!g.same_binding(&other));
        let mut other = g.clone();
        other.logical = [1536, 864];
        assert!(other.require_profile().is_err());
        let mut other = g.clone();
        other.client[1] += 1;
        assert!(!g.same_binding(&other));
        let mut other = g.clone();
        other.objects[7] += 8;
        assert!(!g.same_binding(&other));
        for scale in [f32::NAN, f32::INFINITY, 0.0, 9.0] {
            let mut f = Fixture::new();
            f.put(R + 0x2c5b0, &scale.to_le_bytes());
            assert!(observe(&f).is_err());
        }
    }
}
