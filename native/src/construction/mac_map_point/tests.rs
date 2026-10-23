use super::*;
use crate::{memory::ReadMemory, platform::Process, research_ui::Witnesses};
use std::{cell::RefCell, collections::BTreeMap};

const B: u64 = 0x100000000;
const GS: u64 = 0x1000000;
const IDLER: u64 = 0x1100000;
const COUNTRY: u64 = 0x1200000;
const STATE: u64 = 0x1300000;
const GUI: u64 = 0x1400000;
const GRAPHICS: u64 = 0x1500000;
const RESOURCE: u64 = 0x1600000;
const MAP: u64 = 0x1700000;
const CAMERA: u64 = 0x1800000;
const DATA: u64 = 0x1900000;
const MODE: u64 = 0x1a00000;
const HEIGHT: u64 = 0x1b00000;
const BITMAP: u64 = 0x1c00000;
const PROVINCE: u64 = 0x1d00000;
const DEFINITION: u64 = 0x1e00000;
const INDEXES: u64 = 0x20000000;
const COUNTRIES: u64 = 0x21000000;
const STATES: u64 = 0x22000000;
const PROVINCES: u64 = 0x23000000;
const DEFINITIONS: u64 = 0x24000000;
const HEIGHT_BYTES: u64 = 0x40000000;
const BITMAP_BYTES: u64 = 0x50000000;
const REPLACEMENT: u64 = 0x60000000;
const PROVINCE_ID: i32 = 6521;
const STATE_ID: i32 = 64;
const BITMAP_OFFSET: u64 = (1568 * 5632 + 3008) * 2;

// Actual finite scalar values captured in Q13/map-terrain014/point-live.json.
// The source result is independent of the implementation under test.
const PROJECTION: [f32; 16] = [
    0.7359651,
    0.,
    0.,
    0.,
    0.,
    0.41398036,
    0.,
    0.,
    0.,
    0.,
    0.,
    -0.19974999,
    0.,
    0.,
    1.,
    0.2,
];
const AFFINE: [f32; 16] = [
    1., 0., -0., 0., 0., 0.59405255, 0.8044262, 0., 0., -0.8044262, 0.59405255, 0., 3008.0068,
    50.123993, 1538.994, 1.,
];
const ORIGIN: [f32; 3] = [3008.0068, 50.123993, 1538.994];

struct Fixture {
    bytes: BTreeMap<u64, u8>,
    reads: RefCell<Vec<(u64, usize)>>,
    abi: StringAbi,
    torn_at_finish: Option<u64>,
    short_at: Option<u64>,
}
impl Fixture {
    fn put(&mut self, a: u64, bytes: &[u8]) {
        self.bytes
            .extend(bytes.iter().enumerate().map(|(i, v)| (a + i as u64, *v)));
    }
    fn word(&mut self, a: u64, v: u64) {
        self.put(a, &v.to_le_bytes());
    }
    fn number(&mut self, a: u64, v: i32) {
        self.put(a, &v.to_le_bytes());
    }
    fn byte(&mut self, a: u64, v: u8) {
        self.put(a, &[v]);
    }
    fn floats(&mut self, a: u64, values: &[f32]) {
        for (i, v) in values.iter().enumerate() {
            self.put(a + i as u64 * 4, &v.to_le_bytes());
        }
    }
    fn header(&mut self, a: u64, data: u64, count: i32) {
        self.word(a, data);
        self.number(a + 8, count);
        self.number(a + 12, count);
    }
    fn new() -> Self {
        Self::for_ids(STATE_ID, PROVINCE_ID)
    }
    fn for_ids(state: i32, province: i32) -> Self {
        let mut f = Self {
            bytes: BTreeMap::new(),
            reads: RefCell::new(vec![]),
            abi: StringAbi::LibCxx,
            torn_at_finish: None,
            short_at: None,
        };
        for (a, v) in [
            (B + 0x3519740, GS),
            (B + 0x35196c0, IDLER),
            (GS, B + 0x3306260),
            (IDLER, B + 0x3308f90),
            (COUNTRY, B + 0x337bc38),
            (STATE, B + 0x337caf0),
            (B + 0x36330a0, MODE),
            (MODE + 0x48, GS),
            (IDLER + 0x4c0, GUI),
            (GUI, B + 0x34974d0),
            (GUI + 0x98, GRAPHICS),
            (GRAPHICS, B + 0x33dafa8),
            (IDLER + 0x4f8, RESOURCE),
            (RESOURCE + 0x300, GRAPHICS),
            (RESOURCE + 0x308, GUI),
            (RESOURCE + 0x310, MAP),
            (MAP, B + 0x33dab60),
            (MAP + 8, B + 0x33dab98),
            (MAP + 0x18, GRAPHICS),
            (MAP + 0x20, CAMERA),
            (MAP + 0x28, DATA),
            (IDLER + 0x708, CAMERA),
            (B + 0x3519a00, DATA),
            (DATA, B + 0x330b5b8),
            (B + 0x35dc028, GRAPHICS),
            (B + 0x32a1d28, B + 0x2ac6970),
            (DATA + 0x130, HEIGHT),
            (DATA + 0x7d0, BITMAP),
            (HEIGHT + 0x28, HEIGHT_BYTES),
            (BITMAP + 0x18, BITMAP_BYTES),
            (PROVINCE, B + 0x337c7e8),
            (PROVINCE + 0xc0, STATE),
            (PROVINCE + 0xb8, DEFINITION),
        ] {
            f.word(a, v);
        }
        for (a, v) in [
            (GS + 0x4d8, 1),
            (COUNTRY + 8, 1),
            (STATE + 0xc0, 1),
            (STATE + 0xc4, 1),
            (STATE + 0x40, state),
            (MODE, 27),
            (HEIGHT + 0x10, 1),
            (HEIGHT + 0x1c, 5632),
            (PROVINCE + 0x188, 1),
            (PROVINCE + 0x9c, province),
            (DEFINITION + 0xbc, province),
        ] {
            f.number(a, v);
        }
        f.header(GS + 0x308, INDEXES, 2);
        f.number(INDEXES + 4, 0);
        f.header(GS + 0x2d8, COUNTRIES, 1);
        f.word(COUNTRIES, COUNTRY);
        f.header(GS + 0x290, STATES, state + 1);
        f.word(STATES + state as u64 * 8, STATE);
        f.header(GS + 0x278, PROVINCES, province + 1);
        f.word(PROVINCES + province as u64 * 8, PROVINCE);
        f.header(DATA + 0x208, DEFINITIONS, province + 1);
        f.word(DEFINITIONS + province as u64 * 8, DEFINITION);
        for (i, v) in [1920, 1080, 1920, 1080].iter().enumerate() {
            f.number(GRAPHICS + 0x170 + i as u64 * 4, *v);
        }
        f.floats(GRAPHICS + 0x2c5b0, &[1.]);
        f.put(CAMERA + 0x670, &[0, 0]);
        f.byte(CAMERA + 0x6f8, 1);
        f.floats(CAMERA + 0x110, &PROJECTION);
        f.floats(CAMERA + 0xd0, &AFFINE);
        f.floats(CAMERA + 0x190, &ORIGIN);
        for a in [DATA + 0x40, HEIGHT + 8, BITMAP + 0x30] {
            f.number(a, 5632);
            f.number(a + 4, 2048);
        }
        for z in 1557..=1576 {
            f.byte(
                HEIGHT_BYTES + z * 5632 + 3007,
                if (1559..=1562).contains(&z) { 98 } else { 97 },
            );
        }
        f.put(
            BITMAP_BYTES + BITMAP_OFFSET,
            &(province as u16).to_le_bytes(),
        );
        f.byte(DEFINITION + 0xca, 1);
        f
    }
    fn observe(&self, state: i32, original: Option<&Binding>) -> Result<Observation> {
        let w = Witnesses::new(self);
        let o = from_reader(&Reader::new(&w), B, state, original)?;
        w.finish()?;
        Ok(o)
    }
    fn untouched(&self, a: u64, size: u64) -> bool {
        self.reads
            .borrow()
            .iter()
            .all(|&(b, n)| b + n as u64 <= a || a + size <= b)
    }
    fn clear(&self) {
        self.reads.borrow_mut().clear();
    }
}
impl ReadMemory for Fixture {
    fn string_abi(&self) -> StringAbi {
        self.abi
    }
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        self.reads.borrow_mut().push((a, n));
        let count = if self.short_at == Some(a) { n - 1 } else { n };
        (0..count)
            .map(|i| {
                self.bytes.get(&(a + i as u64)).copied().ok_or_else(|| {
                    Error::new("short_read", "Unmapped sparse center-terrain fixture byte")
                })
            })
            .collect()
    }
    fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        requests
            .iter()
            .map(|&(a, n)| {
                let mut v = self.read(a, n)?;
                if let Some(at) = self.torn_at_finish {
                    if (a..a + n as u64).contains(&at) {
                        v[(at - a) as usize] ^= 1;
                    }
                }
                Ok(v)
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
fn captured_center_matches_independent_world_bits_and_join() {
    let f = Fixture::new();
    let o = f.observe(STATE_ID, None).unwrap();
    assert_eq!(
        o.world_point().map(f32::to_bits),
        [0x453c001c, 0x411b3333, 0x44c4172d]
    );
    assert_eq!(
        (o.center(), o.province_id(), o.state_id(), o.mode()),
        ([960, 540], 6521, 64, 27)
    );
    assert_eq!(o.terrain_counts(), [18, 39, 90]);
    assert_eq!(
        f.observe(STATE_ID, Some(o.binding())).unwrap().binding(),
        o.binding()
    );
}
#[test]
fn requested_state_and_derived_province_are_dynamic() {
    let f = Fixture::for_ids(97, 117);
    let o = f.observe(97, None).unwrap();
    assert_eq!((o.state_id(), o.province_id()), (97, 117));
}
#[test]
fn unqualified_abi_state_or_changed_request_refuses_before_reads() {
    let mut f = Fixture::new();
    let b = f.observe(STATE_ID, None).unwrap().binding;
    f.clear();
    f.abi = StringAbi::LibStdCxx;
    assert_eq!(
        f.observe(STATE_ID, None).unwrap_err().code,
        "unsupported_platform"
    );
    assert!(f.reads.borrow().is_empty());
    f.abi = StringAbi::LibCxx;
    for id in [0, -1, 4096, 65] {
        assert!(f.observe(id, Some(&b)).is_err());
        assert!(f.reads.borrow().is_empty());
    }
}
#[test]
fn zero_unaligned_and_overflow_image_bases_refuse_before_reads() {
    let f = Fixture::new();
    for base in [
        0,
        8,
        0xffff,
        0x10001,
        B + 1,
        MAX_ADDRESS,
        u64::MAX - 7,
        u64::MAX,
    ] {
        let r = Reader::new(&f);
        assert!(from_reader(&r, base, STATE_ID, None).is_err());
        assert_eq!(r.calls.get(), 0);
        assert!(f.reads.borrow().is_empty());
    }
}
#[test]
fn unqualified_wrap_zero_refuses_before_terrain_payload() {
    let mut f = Fixture::new();
    f.byte(CAMERA + 0x6f8, 0);
    assert_eq!(
        f.observe(STATE_ID, None).unwrap_err().code,
        "construction_map_point_unavailable"
    );
    assert!(f.untouched(DATA + 0x40, 8));
    assert!(f.untouched(HEIGHT_BYTES, 64 * 1024 * 1024));
}
#[test]
fn requested_owner_and_controller_refuse_before_state_identity_payload() {
    for off in [0xc0, 0xc4] {
        let mut f = Fixture::new();
        f.number(STATE + off, 2);
        assert!(f.observe(STATE_ID, None).is_err());
        assert!(f.untouched(STATE + 0x40, 4));
        assert!(f.untouched(CAMERA, 0x1000));
    }
}
#[test]
fn wrong_requested_state_identity_does_not_enter_terrain() {
    let mut f = Fixture::new();
    f.number(STATE + 0x40, 65);
    assert!(f.observe(STATE_ID, None).is_err());
    assert!(f.untouched(HEIGHT_BYTES, 64 * 1024 * 1024));
}
#[test]
fn foreign_country_tag_refuses_before_state_payload() {
    let mut f = Fixture::new();
    f.number(COUNTRY + 8, 2);
    assert!(f.observe(STATE_ID, None).is_err());
    assert!(f.untouched(STATE, 0x1000));
}
#[test]
fn exact_known_primary_types_are_required() {
    for address in [
        GS,
        IDLER,
        COUNTRY,
        STATE,
        GUI,
        GRAPHICS,
        MAP,
        MAP + 8,
        DATA,
        PROVINCE,
    ] {
        let mut f = Fixture::new();
        f.word(address, B + 0x123456);
        assert_eq!(
            f.observe(STATE_ID, None).unwrap_err().code,
            "construction_map_point_unavailable"
        );
    }
}
#[test]
fn mode_owner_is_checked_before_enum() {
    let mut f = Fixture::new();
    f.word(MODE + 0x48, REPLACEMENT);
    assert!(f.observe(STATE_ID, None).is_err());
    assert!(f.untouched(MODE, 4));
}
#[test]
fn decimal_27_supported_but_region_modes_and_unknown_modes_refuse() {
    for mode in [0, 27] {
        let mut f = Fixture::new();
        f.number(MODE, mode);
        assert_eq!(f.observe(STATE_ID, None).unwrap().mode(), mode);
    }
    for mode in [-1, 1, 2, 3, 39, 255, 256] {
        let mut f = Fixture::new();
        f.number(MODE, mode);
        assert!(f.observe(STATE_ID, None).is_err());
        assert!(f.untouched(HEIGHT_BYTES, 64 * 1024 * 1024));
    }
}
#[test]
fn foreign_province_control_refuses_before_other_province_payload() {
    let mut f = Fixture::new();
    f.number(PROVINCE + 0x188, 2);
    assert!(f.observe(STATE_ID, None).is_err());
    assert!(f.untouched(PROVINCE + 0x9c, 4));
    assert!(f.untouched(PROVINCE + 0xb8, 16));
}
#[test]
fn wrong_province_state_never_follows_foreign_state() {
    let mut f = Fixture::new();
    f.word(PROVINCE + 0xc0, REPLACEMENT);
    assert!(f.observe(STATE_ID, None).is_err());
    assert!(f.untouched(REPLACEMENT, 0x1000));
    assert!(f.untouched(PROVINCE + 0xb8, 8));
}
#[test]
fn definition_catalog_join_precedes_definition_payload_and_flags() {
    let mut f = Fixture::new();
    f.word(DEFINITIONS + PROVINCE_ID as u64 * 8, REPLACEMENT);
    assert!(f.observe(STATE_ID, None).is_err());
    assert!(f.untouched(DEFINITION + 0xbc, 0xf));
    assert!(f.untouched(REPLACEMENT, 0x1000));
}
#[test]
fn definition_ordinal_precedes_flags_and_flags_must_enable_fallback() {
    let mut f = Fixture::new();
    f.number(DEFINITION + 0xbc, 1);
    assert!(f.observe(STATE_ID, None).is_err());
    assert!(f.untouched(DEFINITION + 0xca, 1));
    for flags in [0, 4, 128] {
        let mut f = Fixture::new();
        f.byte(DEFINITION + 0xca, flags);
        assert!(f.observe(STATE_ID, None).is_err());
    }
    for flags in [1, 2, 3] {
        let mut f = Fixture::new();
        f.byte(DEFINITION + 0xca, flags);
        assert!(f.observe(STATE_ID, None).is_ok());
    }
}
#[test]
fn nonpolymorphic_headers_and_unrelated_input_scene_payload_are_not_read() {
    let f = Fixture::new();
    f.observe(STATE_ID, None).unwrap();
    for (a, n) in [
        (HEIGHT, 8),
        (BITMAP, 0x18),
        (DEFINITION, 8),
        (IDLER + 0x4c8, 8),
        (B + 0x34f9960, 8),
        (GS + 0x5e0, 16),
        (PROVINCE + 0xc8, 8),
    ] {
        assert!(f.untouched(a, n), "Unexpected payload at {a:x}");
    }
}
#[test]
fn original_owner_and_object_edges_refuse_before_replacement_payload() {
    for address in [
        B + 0x3519740,
        B + 0x35196c0,
        COUNTRIES,
        STATES + STATE_ID as u64 * 8,
        B + 0x36330a0,
        IDLER + 0x4c0,
        GUI + 0x98,
        IDLER + 0x4f8,
        RESOURCE + 0x310,
        IDLER + 0x708,
        B + 0x3519a00,
        DATA + 0x130,
        DATA + 0x7d0,
        PROVINCES + PROVINCE_ID as u64 * 8,
        PROVINCE + 0xb8,
    ] {
        let mut f = Fixture::new();
        let b = f.observe(STATE_ID, None).unwrap().binding;
        f.clear();
        f.word(address, REPLACEMENT);
        assert_eq!(
            f.observe(STATE_ID, Some(&b)).unwrap_err().code,
            "construction_map_point_changed",
            "edge {address:x}"
        );
        assert!(f.untouched(REPLACEMENT, 0x100000));
    }
}
#[test]
fn original_catalog_headers_refuse_before_replacement_array_payload() {
    for address in [GS + 0x308, GS + 0x2d8, GS + 0x290, GS + 0x278, DATA + 0x208] {
        let mut f = Fixture::new();
        let b = f.observe(STATE_ID, None).unwrap().binding;
        f.clear();
        f.word(address, REPLACEMENT);
        assert_eq!(
            f.observe(STATE_ID, Some(&b)).unwrap_err().code,
            "construction_map_point_changed"
        );
        assert!(f.untouched(REPLACEMENT, 0x100000));
    }
}
#[test]
fn retained_grid_data_replacement_refuses_before_samples() {
    for address in [HEIGHT + 0x28, BITMAP + 0x18] {
        let mut f = Fixture::new();
        let b = f.observe(STATE_ID, None).unwrap().binding;
        f.clear();
        f.word(address, REPLACEMENT);
        assert_eq!(
            f.observe(STATE_ID, Some(&b)).unwrap_err().code,
            "construction_map_point_changed"
        );
        assert!(f.untouched(REPLACEMENT, 64 * 1024 * 1024));
    }
}
#[test]
fn retained_changed_province_id_refuses_before_new_province_slot() {
    let mut f = Fixture::new();
    let b = f.observe(STATE_ID, None).unwrap().binding;
    f.clear();
    f.put(BITMAP_BYTES + BITMAP_OFFSET, &6520_u16.to_le_bytes());
    assert_eq!(
        f.observe(STATE_ID, Some(&b)).unwrap_err().code,
        "construction_map_point_changed"
    );
    assert!(f.untouched(PROVINCES + 6520 * 8, 8));
}
#[test]
fn metadata_dimensions_spans_and_strides_are_bounded_before_sampling() {
    for (a, value) in [
        (DATA + 0x40, 0),
        (HEIGHT + 8, -1),
        (HEIGHT + 8, 32769),
        (HEIGHT + 8, 32768),
        (HEIGHT + 12, 32768),
        (HEIGHT + 0x10, 2),
        (HEIGHT + 0x1c, 5631),
        (BITMAP + 0x30, 5631),
    ] {
        let mut f = Fixture::new();
        f.number(a, value);
        assert!(f.observe(STATE_ID, None).is_err(), "metadata {a:x}={value}");
        assert!(f.untouched(HEIGHT_BYTES, 64 * 1024 * 1024));
    }
    for (a, value) in [
        (HEIGHT + 0x28, 0xffff),
        (HEIGHT + 0x28, MAX_ADDRESS - 100),
        (BITMAP + 0x18, u64::MAX - 4),
    ] {
        let mut f = Fixture::new();
        f.word(a, value);
        assert!(f.observe(STATE_ID, None).is_err());
        assert!(f.untouched(HEIGHT_BYTES, 64 * 1024 * 1024));
    }
}
#[test]
fn catalog_counts_capacities_and_alignment_refuse_before_data() {
    for (a, n) in [
        (GS + 0x308 + 8, 4097),
        (GS + 0x2d8 + 12, 2),
        (GS + 0x290 + 12, STATE_ID),
        (GS + 0x278 + 8, 20001),
        (DATA + 0x208 + 12, PROVINCE_ID),
    ] {
        let mut f = Fixture::new();
        f.number(a, n);
        assert!(f.observe(STATE_ID, None).is_err());
    }
    let mut f = Fixture::new();
    f.word(GS + 0x290, STATES + 1);
    assert!(f.observe(STATE_ID, None).is_err());
    assert!(f.untouched(STATES, 0x10000));
}
#[test]
fn canvas_projection_alternates_and_nonfinite_camera_refuse() {
    for (a, v) in [(GRAPHICS + 0x170, 1919), (GRAPHICS + 0x178, 960)] {
        let mut f = Fixture::new();
        f.number(a, v);
        assert!(f.observe(STATE_ID, None).is_err());
    }
    for a in [CAMERA + 0x670, CAMERA + 0x671, CAMERA + 0x6f8] {
        let mut f = Fixture::new();
        f.byte(a, 2);
        assert!(f.observe(STATE_ID, None).is_err());
    }
    for a in [
        GRAPHICS + 0x2c5b0,
        CAMERA + 0x110,
        CAMERA + 0xd0,
        CAMERA + 0x190,
    ] {
        let mut f = Fixture::new();
        f.floats(a, &[f32::NAN]);
        assert!(f.observe(STATE_ID, None).is_err());
    }
    let mut f = Fixture::new();
    f.word(B + 0x32a1d28, B + 0x1234);
    assert!(f.observe(STATE_ID, None).is_err());
}
#[test]
fn changed_exact_owner_backlinks_refuse() {
    for a in [
        RESOURCE + 0x300,
        RESOURCE + 0x308,
        MAP + 0x18,
        MAP + 0x20,
        MAP + 0x28,
        B + 0x35dc028,
    ] {
        let mut f = Fixture::new();
        f.word(a, REPLACEMENT);
        assert!(f.observe(STATE_ID, None).is_err());
        assert!(f.untouched(REPLACEMENT, 0x1000));
    }
}
#[test]
fn short_metadata_samples_and_bitmap_reads_are_never_accepted() {
    for a in [
        HEIGHT + 8,
        CAMERA + 0x110,
        HEIGHT_BYTES + 1557 * 5632 + 3007,
        BITMAP_BYTES + BITMAP_OFFSET,
    ] {
        let mut f = Fixture::new();
        f.short_at = Some(a);
        assert_eq!(f.observe(STATE_ID, None).unwrap_err().code, "short_read");
    }
}
#[test]
fn final_witness_refuses_late_owner_metadata_and_sample_tears() {
    for a in [
        B + 0x3519740,
        GS + 0x290 + 12,
        STATE + 0xc4,
        CAMERA + 0x190,
        HEIGHT + 0x28,
        HEIGHT_BYTES + 1557 * 5632 + 3007,
        BITMAP_BYTES + BITMAP_OFFSET,
        DEFINITION + 0xca,
    ] {
        let mut f = Fixture::new();
        f.torn_at_finish = Some(a);
        assert_eq!(
            f.observe(STATE_ID, None).unwrap_err().code,
            "torn_read",
            "tear {a:x}"
        );
    }
}
