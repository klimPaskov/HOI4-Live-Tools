//! Cached SDL source coordinates for Mach-O 089EA519. This reader supplies no
//! current-event, frame, routing, input or delivery authority. The caller owns
//! the encompassing Geometry/context/final-witness and process-identity checks.
use super::{pointer, Geometry};
use crate::{
    error::{Error, Result},
    memory::{Reader, StringAbi},
};

#[derive(Clone, Debug)]
pub(crate) struct Binding {
    base: u64,
    provider: u64,
    geometry: Geometry,
}

impl Binding {
    pub(crate) fn image_base(&self) -> u64 {
        self.base
    }

    pub(crate) fn idler(&self) -> u64 {
        self.geometry.objects[0]
    }

    pub(crate) fn provider(&self) -> u64 {
        self.provider
    }

    pub(crate) fn window(&self) -> u64 {
        self.geometry.objects[5]
    }

    pub(crate) fn dimensions(&self) -> [i32; 2] {
        self.geometry.physical
    }

    /// Coordinates and raw SDL window flags may change. The original source,
    /// complete native-window association and coordinate mapping may not.
    pub(crate) fn same_binding(&self, now: &Self) -> bool {
        self.base == now.base
            && self.provider == now.provider
            && self.geometry.same_binding(&now.geometry)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct SourcePoint {
    binding: Binding,
    point: [i32; 2],
}

impl SourcePoint {
    pub(crate) fn binding(&self) -> &Binding {
        &self.binding
    }

    pub(crate) fn point(&self) -> [i32; 2] {
        self.point
    }
}

fn invalid(message: &'static str) -> Error {
    Error::new("invalid_input_source", message)
}

fn changed(message: &'static str) -> Error {
    Error::new("input_source_changed", message)
}

fn image(base: u64, offset: u64) -> Result<u64> {
    pointer(
        base.checked_add(offset)
            .ok_or_else(|| invalid("Mac source image address overflow"))?,
    )
}

/// `geometry` must come from this same encompassing witnessed observation.
/// An original binding rejects replacement links before following their new
/// payloads. This function neither finishes nor creates a witness bracket.
pub(crate) fn from_reader(
    r: &Reader<'_>,
    base: u64,
    geometry: &Geometry,
    original: Option<&Binding>,
) -> Result<SourcePoint> {
    if r.string_abi() != StringAbi::LibCxx {
        return Err(Error::new(
            "unsupported_platform",
            "Mac source point requires the reviewed Mach-O layout",
        ));
    }
    if original.is_some_and(|b| b.base != base || !b.geometry.same_binding(geometry)) {
        return Err(changed("Original Mac source geometry changed"));
    }
    if geometry.physical.iter().any(|v| !(1..=16384).contains(v)) {
        return Err(invalid("Bounded Mac source dimensions required"));
    }
    // This is a passive shown/windowed-mode check. Keyboard or mouse focus
    // flags alone do not grant foreground, event or input authority.
    if geometry.flags & 0x6 != 0x6 || geometry.flags & !0x606 != 0 {
        return Err(Error::new(
            "input_source_unavailable",
            "Ordinary shown Mac SDL window flags required",
        ));
    }

    let idler = pointer(geometry.objects[0])?;
    if r.u64(image(base, 0x372afc0)?)? != idler {
        return Err(changed("Original Mac source idler changed before payload"));
    }
    if r.u64(idler)? != image(base, 0x35089d8)? {
        return Err(invalid("Exact Mac source idler required"));
    }
    let provider = r.u64(idler + 0x4c8)?;
    if original.is_some_and(|b| b.provider != provider) {
        return Err(changed(
            "Original Mac input provider changed before payload",
        ));
    }
    pointer(provider)?;
    let primary = image(base, 0x36b5388)?;
    if r.u64(provider)? != primary
        || r.u64(primary + 0xc8)? != image(base, 0x2834a90)?
        || r.u64(image(base, 0x3708e00)?)? != image(base, 0x26e9f50)?
    {
        return Err(invalid(
            "Exact Mac mouse getter and SDL implementation required",
        ));
    }

    let window = pointer(geometry.objects[5])?;
    // Never follow a foreign focus pointer. Coordinates survive focus changes.
    if r.u64(image(base, 0x384b168)?)? != window {
        return Err(changed(
            "Mac mouse focus must remain the original owned SDL window",
        ));
    }
    let flags = r.read(image(base, 0x384b190)?, 12)?;
    let initialized = i32::from_le_bytes(flags[..4].try_into().unwrap());
    let relative = i32::from_le_bytes(flags[4..8].try_into().unwrap());
    let warp = i32::from_le_bytes(flags[8..].try_into().unwrap());
    if initialized != 1 || relative != 0 || warp != 0 {
        return Err(Error::new(
            "input_source_unavailable",
            "Initialized ordinary absolute Mac mouse coordinates required",
        ));
    }

    let video = pointer(geometry.objects[3])?;
    if r.u64(image(base, 0x384b480)?)? != video {
        return Err(changed("Original Mac SDL device changed before payload"));
    }
    if r.u64(window)? != video + 0x2d8 {
        return Err(invalid("Original Mac SDL window magic required"));
    }
    let size = [r.i32(window + 0x28)?, r.i32(window + 0x2c)?];
    if size != geometry.physical {
        return Err(changed(
            "Mac mouse window dimensions differ from original geometry",
        ));
    }

    // The getter returns these signed window-relative integers without another
    // scale/origin transform. No coordinate payload is read before all gates.
    let bytes = r.read(image(base, 0x384b170)?, 8)?;
    let point = [
        i32::from_le_bytes(bytes[..4].try_into().unwrap()),
        i32::from_le_bytes(bytes[4..].try_into().unwrap()),
    ];
    if point
        .iter()
        .zip(size)
        .any(|(coordinate, bound)| !(0..bound).contains(coordinate))
    {
        return Err(invalid(
            "Mac cached source point must lie inside the owned SDL window",
        ));
    }
    Ok(SourcePoint {
        binding: Binding {
            base,
            provider,
            geometry: geometry.clone(),
        },
        point,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{memory::ReadMemory, platform::Process, research_ui::Witnesses};
    use std::{cell::RefCell, collections::BTreeMap};

    const B: u64 = 0x1_0000_0000;
    const I: u64 = 0x2_0000_0000;
    const PROVIDER: u64 = I + 0x1000;
    const VIDEO: u64 = I + 0x2000;
    const WINDOW: u64 = I + 0x3000;
    const REPLACEMENT: u64 = I + 0x4000;
    const POINT: u64 = B + 0x384b170;

    struct Fixture {
        bytes: BTreeMap<u64, u8>,
        abi: StringAbi,
        reads: RefCell<Vec<(u64, usize)>>,
        changed_at_finish: Option<u64>,
    }

    impl Fixture {
        fn put(&mut self, address: u64, bytes: &[u8]) {
            self.bytes.extend(
                bytes
                    .iter()
                    .enumerate()
                    .map(|(i, b)| (address + i as u64, *b)),
            );
        }
        fn word(&mut self, address: u64, value: u64) {
            self.put(address, &value.to_le_bytes());
        }
        fn integer(&mut self, address: u64, value: i32) {
            self.put(address, &value.to_le_bytes());
        }
        fn new() -> Self {
            let mut f = Self {
                bytes: BTreeMap::new(),
                abi: StringAbi::LibCxx,
                reads: RefCell::new(Vec::new()),
                changed_at_finish: None,
            };
            for (address, value) in [
                (B + 0x372afc0, I),
                (I, B + 0x35089d8),
                (I + 0x4c8, PROVIDER),
                (PROVIDER, B + 0x36b5388),
                (B + 0x36b5388 + 0xc8, B + 0x2834a90),
                (B + 0x3708e00, B + 0x26e9f50),
                (B + 0x384b168, WINDOW),
                (B + 0x384b480, VIDEO),
                (WINDOW, VIDEO + 0x2d8),
            ] {
                f.word(address, value);
            }
            for (address, value) in [
                (B + 0x384b190, 1),
                (B + 0x384b194, 0),
                (B + 0x384b198, 0),
                (WINDOW + 0x28, 1920),
                (WINDOW + 0x2c, 1080),
                (POINT, 960),
                (POINT + 4, 540),
            ] {
                f.integer(address, value);
            }
            f
        }
        fn untouched(&self, address: u64, size: usize) -> bool {
            self.reads
                .borrow()
                .iter()
                .all(|(a, n)| address + size as u64 <= *a || *a + *n as u64 <= address)
        }
        fn observe(&self, geometry: &Geometry, original: Option<&Binding>) -> Result<SourcePoint> {
            self.begin_snapshot()?;
            let w = Witnesses::new(self);
            let result = from_reader(&Reader::new(&w), B, geometry, original)?;
            w.finish()?;
            self.check()?;
            Ok(result)
        }
    }

    impl ReadMemory for Fixture {
        fn string_abi(&self) -> StringAbi {
            self.abi
        }
        fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
            self.reads.borrow_mut().push((address, size));
            (0..size)
                .map(|i| {
                    self.bytes
                        .get(&(address + i as u64))
                        .copied()
                        .ok_or_else(|| Error::new("short_read", "Missing sparse source byte"))
                })
                .collect()
        }
        fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
            requests
                .iter()
                .map(|(address, size)| {
                    let mut bytes = self.read(*address, *size)?;
                    if let Some(changed) = self.changed_at_finish {
                        if (*address..*address + *size as u64).contains(&changed) {
                            bytes[(changed - *address) as usize] ^= 1;
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

    fn geometry() -> Geometry {
        Geometry {
            objects: [
                I,
                I + 0x5000,
                I + 0x6000,
                VIDEO,
                I + 0x7000,
                WINDOW,
                I + 0x8000,
                I + 0x9000,
                I + 0xa000,
                I + 0xb000,
            ],
            physical: [1920, 1080],
            logical: [1920, 1080],
            scale: 1.0,
            client: [0, 81, 1920, 1080],
            flags: 0x606,
        }
    }

    #[test]
    fn typed_point_retains_the_original_source_and_native_mapping() {
        let point = Fixture::new().observe(&geometry(), None).unwrap();
        assert_eq!(point.point(), [960, 540]);
        let b = point.binding();
        assert_eq!(b.image_base(), B);
        assert_eq!(b.idler(), I);
        assert_eq!(b.provider(), PROVIDER);
        assert_eq!(b.window(), WINDOW);
        assert_eq!(b.dimensions(), [1920, 1080]);
        assert!(b.same_binding(b));
    }

    #[test]
    fn unsupported_abi_refuses_before_any_layout_read() {
        let mut f = Fixture::new();
        f.abi = StringAbi::LibStdCxx;
        assert_eq!(
            f.observe(&geometry(), None).unwrap_err().code,
            "unsupported_platform"
        );
        assert!(f.reads.borrow().is_empty());
    }

    #[test]
    fn focus_reset_cannot_attach_old_coordinates_to_a_new_focus() {
        let mut f = Fixture::new();
        f.integer(B + 0x384b190, 0);
        assert_eq!(
            f.observe(&geometry(), None).unwrap_err().code,
            "input_source_unavailable"
        );
        assert!(f.untouched(POINT, 8));
        assert!(f.untouched(WINDOW, 0x30));
    }

    #[test]
    fn foreign_or_missing_focus_is_not_followed_and_point_is_skipped() {
        for focus in [0, REPLACEMENT] {
            let mut f = Fixture::new();
            f.word(B + 0x384b168, focus);
            assert_eq!(
                f.observe(&geometry(), None).unwrap_err().code,
                "input_source_changed"
            );
            assert!(f.untouched(REPLACEMENT, 0x30));
            assert!(f.untouched(POINT, 8));
        }
    }

    #[test]
    fn relative_warp_and_unknown_initialization_flags_refuse_before_point() {
        for (address, values) in [
            (B + 0x384b190, vec![-1, 2, i32::MAX]),
            (B + 0x384b194, vec![-1, 1, i32::MAX]),
            (B + 0x384b198, vec![-1, 1, i32::MAX]),
        ] {
            for value in values {
                let mut f = Fixture::new();
                f.integer(address, value);
                assert!(f.observe(&geometry(), None).is_err());
                assert!(f.untouched(POINT, 8));
            }
        }
    }

    #[test]
    fn original_provider_replacement_refuses_before_replacement_payload() {
        let initial = Fixture::new().observe(&geometry(), None).unwrap();
        let mut f = Fixture::new();
        f.word(I + 0x4c8, REPLACEMENT);
        assert_eq!(
            f.observe(&geometry(), Some(initial.binding()))
                .unwrap_err()
                .code,
            "input_source_changed"
        );
        assert!(f.untouched(REPLACEMENT, 8));
        assert!(f.untouched(POINT, 8));
    }

    #[test]
    fn changed_idler_and_video_links_refuse_without_new_payload() {
        for global in [B + 0x372afc0, B + 0x384b480] {
            let mut f = Fixture::new();
            f.word(global, REPLACEMENT);
            assert_eq!(
                f.observe(&geometry(), None).unwrap_err().code,
                "input_source_changed"
            );
            assert!(f.untouched(REPLACEMENT, 8));
            assert!(f.untouched(POINT, 8));
        }
    }

    #[test]
    fn wrong_primary_virtual_or_dynapi_refuses_before_coordinates() {
        for address in [I, PROVIDER, B + 0x36b5388 + 0xc8, B + 0x3708e00] {
            let mut f = Fixture::new();
            f.word(address, B + 0x26e9f58);
            assert_eq!(
                f.observe(&geometry(), None).unwrap_err().code,
                "invalid_input_source"
            );
            assert!(f.untouched(POINT, 8));
        }
    }

    #[test]
    fn same_sized_new_window_is_not_the_original_window() {
        let original = Fixture::new().observe(&geometry(), None).unwrap();
        let mut now = geometry();
        now.objects[5] = REPLACEMENT;
        let f = Fixture::new();
        assert_eq!(
            f.observe(&now, Some(original.binding())).unwrap_err().code,
            "input_source_changed"
        );
        assert!(f.reads.borrow().is_empty());
    }

    #[test]
    fn every_original_geometry_identity_and_mapping_is_retained() {
        let original = Fixture::new().observe(&geometry(), None).unwrap();
        for index in 0..10 {
            let mut now = geometry();
            now.objects[index] += 0x100;
            let f = Fixture::new();
            assert!(f.observe(&now, Some(original.binding())).is_err());
            assert!(f.reads.borrow().is_empty());
        }
        for change in [
            |g: &mut Geometry| g.physical[0] += 1,
            |g: &mut Geometry| g.logical[0] += 1,
            |g: &mut Geometry| g.scale = 2.0,
            |g: &mut Geometry| g.client[0] += 1,
        ] {
            let mut now = geometry();
            change(&mut now);
            let f = Fixture::new();
            assert!(f.observe(&now, Some(original.binding())).is_err());
            assert!(f.reads.borrow().is_empty());
        }
    }

    #[test]
    fn coordinates_and_mouse_window_flags_can_change_without_rebinding() {
        let original = Fixture::new().observe(&geometry(), None).unwrap();
        let mut f = Fixture::new();
        f.integer(POINT, 1919);
        f.integer(POINT + 4, 0);
        let mut g = geometry();
        g.flags = 0x406;
        let now = f.observe(&g, Some(original.binding())).unwrap();
        assert_eq!(now.point(), [1919, 0]);
        assert!(original.binding().same_binding(now.binding()));
    }

    #[test]
    fn hidden_minimized_unknown_or_unshown_window_refuses_before_coordinates() {
        for flags in [0, 2, 4, 0x60e, 0x646, 0x1606] {
            let mut g = geometry();
            g.flags = flags;
            let f = Fixture::new();
            assert_eq!(
                f.observe(&g, None).unwrap_err().code,
                "input_source_unavailable"
            );
            assert!(f.reads.borrow().is_empty());
        }
        // Passive observation permits the ordinary shown window even when
        // neither SDL focus flag is set. Exact mouse.focus is checked anyway.
        let mut g = geometry();
        g.flags = 6;
        assert!(Fixture::new().observe(&g, None).is_ok());
    }

    #[test]
    fn bad_window_magic_or_dimensions_refuse_before_point() {
        let mut f = Fixture::new();
        f.word(WINDOW, VIDEO + 0x2e0);
        assert!(f.observe(&geometry(), None).is_err());
        assert!(f.untouched(POINT, 8));
        for (address, value) in [
            (WINDOW + 0x28, 1919),
            (WINDOW + 0x2c, 1081),
            (WINDOW + 0x28, 0),
        ] {
            let mut f = Fixture::new();
            f.integer(address, value);
            assert!(f.observe(&geometry(), None).is_err());
            assert!(f.untouched(POINT, 8));
        }
        let mut g = geometry();
        g.physical = [0, 1080];
        let f = Fixture::new();
        assert!(f.observe(&g, None).is_err());
        assert!(f.reads.borrow().is_empty());
    }

    #[test]
    fn invalid_coordinates_are_never_clamped_or_converted_to_center() {
        for point in [
            [-1, 540],
            [960, -1],
            [1920, 540],
            [960, 1080],
            [i32::MAX, 0],
            [0, i32::MIN],
        ] {
            let mut f = Fixture::new();
            f.integer(POINT, point[0]);
            f.integer(POINT + 4, point[1]);
            assert_eq!(
                f.observe(&geometry(), None).unwrap_err().code,
                "invalid_input_source"
            );
        }
    }

    #[test]
    fn every_consumed_dependency_must_survive_the_encompassing_witness() {
        for address in [
            B + 0x372afc0,
            I,
            I + 0x4c8,
            PROVIDER,
            B + 0x36b5388 + 0xc8,
            B + 0x3708e00,
            B + 0x384b168,
            B + 0x384b190,
            B + 0x384b194,
            B + 0x384b198,
            B + 0x384b480,
            WINDOW,
            WINDOW + 0x28,
            WINDOW + 0x2c,
            POINT,
            POINT + 4,
        ] {
            let mut f = Fixture::new();
            f.changed_at_finish = Some(address);
            assert_eq!(
                f.observe(&geometry(), None).unwrap_err().code,
                "torn_read",
                "{address:x}"
            );
        }
    }

    #[test]
    fn unrelated_late_bytes_cannot_supply_a_fresh_event_or_generation_proof() {
        let mut f = Fixture::new();
        f.changed_at_finish = Some(REPLACEMENT);
        let observed = f.observe(&geometry(), None).unwrap();
        assert_eq!(observed.point(), [960, 540]);
        assert!(f.untouched(REPLACEMENT, 8));
        // The result carries only a binding and cached point. Finishing this
        // bracket makes no assertion about unobserved fields or cache age.
    }

    #[test]
    fn incomplete_coordinate_read_and_image_overflow_refuse() {
        let mut f = Fixture::new();
        f.bytes.remove(&(POINT + 7));
        assert_eq!(f.observe(&geometry(), None).unwrap_err().code, "short_read");
        let f = Fixture::new();
        assert_eq!(
            from_reader(&Reader::new(&f), u64::MAX, &geometry(), None)
                .unwrap_err()
                .code,
            "invalid_input_source"
        );
        assert!(f.reads.borrow().is_empty());
    }
}
