//! Generated memory on the portable fixture process; no OS or game is opened.
pub(crate) use crate::own_focus::assignment_fixture::{Fixture, BASE, COUNTRY, FOREIGN, GS, IDLER};
use crate::{
    construction::mac_ui::{self, PlacementAnchor},
    game,
    memory::StringAbi,
};
pub(crate) const PRODUCTION: u64 = 0x900000;
pub(crate) const DATABASE: u64 = 0x910000;
pub(crate) const TEMPLATE: u64 = 0x920000;
pub(crate) const STATE: u64 = 0x930000;
pub(crate) const LINE: u64 = 0x960000;
pub(crate) const SECOND: u64 = 0x961000;
pub(crate) const NEW: u64 = 0x962000;
pub(crate) const GUI: u64 = 0x1000000;
pub(crate) const MANAGER: u64 = 0x1010000;
pub(crate) const PANEL: u64 = 0x1020000;
pub(crate) const ROOT: u64 = 0x1030000;
pub(crate) const FIND: u64 = 0x1040000;
pub(crate) const FIND_ROOT: u64 = 0x1050000;
pub(crate) const GRAPHICS: u64 = 0x2000000;
pub(crate) const VIDEO: u64 = 0x3000000;
pub(crate) const WINDOW: u64 = 0x3020000;
pub(crate) const DRIVER: u64 = 0x3030000;
pub(crate) const VIEW: u64 = 0x3050000;
pub(crate) const CAMERA: u64 = 0x2400000;
pub(crate) const DATA: u64 = 0x2500000;
pub(crate) const PROVINCE: u64 = 0x2900000;
pub(crate) const HEIGHT_BYTES: u64 = 0x40000000;
const OTHER: u64 = 0x921000;
const NULL: u64 = 0x922000;
const BUILDING: u64 = 0x950000;
const BUILDING2: u64 = 0x951000;
fn template(f: &mut Fixture, p: u64, i: i32, key: &str) {
    f.word(p, BASE + 0x352fa88);
    f.put(p + 0x18, &[1]);
    f.int(p + 0x290, i);
    f.string(p + 0x228, key);
}
fn line(f: &mut Fixture, p: u64, h: u64, building: u64, n: i32) {
    for (a, v) in [
        (p, BASE + 0x358c690),
        (p + 8, h),
        (p + 0x20, PRODUCTION),
        (p + 0x28, 100000),
        (p + 0x30, 100),
        (p + 0x38, 0),
        (p + 0x78, building),
    ] {
        f.word(a, v);
    }
    for (o, v) in [
        (0x14, 1),
        (0x18, 0),
        (0x40, 1),
        (0x44, 0),
        (0x48, n),
        (0x80, 0),
    ] {
        f.int(p + o, v);
    }
    f.put(p + 0x94, &[0]);
    f.put(p + 0xa1, &[0]);
}
pub(crate) fn memory() -> Fixture {
    let mut f = Fixture::new(StringAbi::LibCxx);
    for (a, v) in [
        (COUNTRY + 0xd10, PRODUCTION),
        (PRODUCTION, BASE + 0x358e0c8),
        (PRODUCTION + 0x2a8, COUNTRY),
        (BASE + 0x3703ad0, DATABASE),
        (DATABASE, BASE + 0x352eea8),
        (NULL, BASE + 0x352f078),
        (STATE, BASE + 0x357f410),
        (IDLER + 0x4c0, GUI),
        (GUI, BASE + 0x36a5810),
        (IDLER + 0x670, MANAGER),
        (MANAGER + 0x180, IDLER),
        (MANAGER + 0x3b8, TEMPLATE),
        (DATABASE + 0x3a8, NULL),
        (STATE + 0x110, BASE + 0x352f828),
        (BUILDING, BASE + 0x352ee30),
        (BUILDING + 0x1a8, STATE + 0x110),
        (BUILDING + 0x1b0, TEMPLATE),
        (BUILDING2, BASE + 0x352ee30),
        (BUILDING2 + 0x1a8, STATE + 0x110),
        (BUILDING2 + 0x1b0, OTHER),
    ] {
        f.word(a, v);
    }
    f.put(NULL + 0x18, &[0]);
    template(&mut f, TEMPLATE, 1, "arms_factory");
    template(&mut f, OTHER, 2, "industrial_complex");
    f.array(DATABASE + 0x40, &[NULL, TEMPLATE, OTHER]);
    f.array(GS + 0x290, &[0, STATE]);
    for (a, v) in [
        (STATE + 0x40, 1),
        (STATE + 0xc0, 1),
        (STATE + 0xc4, 1),
        (MANAGER + 0x3c0, 1),
        (STATE + 0x178, 2),
        (STATE + 0x17c, 1),
        (STATE + 0x180, 0),
    ] {
        f.int(a, v);
    }
    f.array(STATE + 0x130, &[0, 0]);
    let indexes = f.get_word(STATE + 0x130);
    f.int(STATE + 0x138, 3);
    f.int(STATE + 0x13c, 3);
    f.int(indexes, -1);
    f.int(indexes + 4, 0);
    f.int(indexes + 8, 1);
    f.array(STATE + 0x148, &[BUILDING, BUILDING2]);
    line(&mut f, LINE, 0x10000003b, BUILDING, 2);
    line(&mut f, SECOND, 0x20000003b, BUILDING2, 4);
    line(&mut f, NEW, 0x30000003b, BUILDING, 1);
    f.array(PRODUCTION + 0x70, &[LINE, SECOND]);
    f
}
pub(crate) fn anchor(f: &Fixture) -> PlacementAnchor {
    mac_ui::bind_selection(f, "arms_factory", 1)
        .unwrap()
        .placement_anchor()
        .unwrap()
}
pub(crate) fn context(f: &Fixture) -> game::ClockContext {
    game::clock_context(f).unwrap()
}
fn floats(f: &mut Fixture, a: u64, v: &[f32]) {
    for (i, n) in v.iter().enumerate() {
        f.put(a + i as u64 * 4, &n.to_le_bytes());
    }
}
pub(crate) fn point_memory() -> Fixture {
    let mut f = memory();
    const RESOURCE: u64 = 0x2200000;
    const MAP: u64 = 0x2300000;
    const MODE: u64 = 0x2600000;
    const HEIGHT: u64 = 0x2700000;
    const BITMAP: u64 = 0x2800000;
    const DEF: u64 = 0x2a00000;
    const BACKEND: u64 = 0x3010000;
    const NSWINDOW: u64 = 0x3040000;
    const LISTENER: u64 = 0x3060000;
    const PROVIDER: u64 = 0x3070000;
    for (a, v) in [
        (GUI + 0x98, GRAPHICS),
        (GRAPHICS, BASE + 0x35e5b58),
        (IDLER + 0x4f8, RESOURCE),
        (RESOURCE + 0x300, GRAPHICS),
        (RESOURCE + 0x308, GUI),
        (RESOURCE + 0x310, MAP),
        (MAP, BASE + 0x35e5478),
        (MAP + 8, BASE + 0x35e54b0),
        (MAP + 0x18, GRAPHICS),
        (MAP + 0x20, CAMERA),
        (MAP + 0x28, DATA),
        (IDLER + 0x718, CAMERA),
        (BASE + 0x372b328, DATA),
        (DATA, BASE + 0x350ae90),
        (BASE + 0x3844d08, MODE),
        (MODE + 0x48, GS),
        (BASE + 0x37ed9d8, GRAPHICS),
        (BASE + 0x349f108, BASE + 0x2c5ddc0),
        (DATA + 0x130, HEIGHT),
        (DATA + 0x7d0, BITMAP),
        (HEIGHT + 0x28, HEIGHT_BYTES),
        (BITMAP + 0x18, 0x50000000),
        (PROVINCE, BASE + 0x357f020),
        (PROVINCE + 0xc0, STATE),
        (PROVINCE + 0xb8, DEF),
        (BASE + 0x384b480, VIDEO),
        (VIDEO + 8, BASE + 0x27b8910),
        (VIDEO + 0x48, BASE + 0x27bc4c0),
        (VIDEO + 0x128, BASE + 0x27bcdc0),
        (VIDEO + 0x5a8, BACKEND),
        (VIDEO + 0x2c8, WINDOW),
        (WINDOW, VIDEO + 0x2d8),
        (WINDOW + 0xe8, 0),
        (WINDOW + 0xd8, DRIVER),
        (DRIVER, WINDOW),
        (DRIVER + 8, NSWINDOW),
        (DRIVER + 0x10, VIEW),
        (DRIVER + 0x30, LISTENER),
        (DRIVER + 0x38, BACKEND),
        (BASE + 0x36dcdf8, 16),
        (BASE + 0x36dcd98, 24),
        (VIEW + 16, WINDOW),
        (LISTENER + 24, DRIVER),
        (IDLER + 0x4c8, PROVIDER),
        (PROVIDER, BASE + 0x36b5388),
        (BASE + 0x36b5388 + 0xc8, BASE + 0x2834a90),
        (BASE + 0x3708e00, BASE + 0x26e9f50),
        (BASE + 0x384b168, WINDOW),
        (MANAGER + 0x150, PANEL),
        (PANEL, BASE + 0x3647478),
        (PANEL + 0x488, IDLER),
        (PANEL + 0x30, MANAGER),
        (PANEL + 0x490, ROOT),
        (ROOT, BASE + 0x36ba978),
        (ROOT + 0x88, GUI),
        (MANAGER + 0x190, FIND),
        (FIND, BASE + 0x3665308),
        (FIND + 0xd50, IDLER),
        (FIND + 0xd58, FIND_ROOT),
        (FIND_ROOT, BASE + 0x36ba978),
        (FIND_ROOT + 0x88, GUI),
    ] {
        f.word(a, v);
    }
    for (a, v) in [
        (MODE, 27),
        (HEIGHT + 0x10, 1),
        (HEIGHT + 0x1c, 5632),
        (PROVINCE + 0x188, 1),
        (PROVINCE + 0x9c, 117),
        (DEF + 0xbc, 117),
        (WINDOW + 0x40, 0x606),
        (BASE + 0x384b190, 1),
        (BASE + 0x384b194, 0),
        (BASE + 0x384b198, 0),
        (BASE + 0x384b170, 62),
        (BASE + 0x384b174, 10),
        (MANAGER + 0x3a8, 17),
        (ROOT + 0x13d8, 0),
        (ROOT + 0x1390, 0),
        (FIND_ROOT + 0x13d8, 0),
        (FIND_ROOT + 0x1390, 0),
    ] {
        f.int(a, v);
    }
    for (i, v) in [0, 81, 1920, 1080].iter().enumerate() {
        f.int(WINDOW + 0x20 + i as u64 * 4, *v);
    }
    for (i, v) in [1920, 1080, 1920, 1080].iter().enumerate() {
        f.int(GRAPHICS + 0x170 + i as u64 * 4, *v);
    }
    floats(&mut f, GRAPHICS + 0x2c5b0, &[1.]);
    f.put(CAMERA + 0x670, &[0, 0]);
    f.put(CAMERA + 0x6f8, &[1]);
    floats(
        &mut f,
        CAMERA + 0x110,
        &[
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
        ],
    );
    floats(
        &mut f,
        CAMERA + 0xd0,
        &[
            1., 0., -0., 0., 0., 0.59405255, 0.8044262, 0., 0., -0.8044262, 0.59405255, 0.,
            3008.0068, 50.123993, 1538.994, 1.,
        ],
    );
    floats(&mut f, CAMERA + 0x190, &[3008.0068, 50.123993, 1538.994]);
    for a in [DATA + 0x40, HEIGHT + 8, BITMAP + 0x30] {
        f.int(a, 5632);
        f.int(a + 4, 2048);
    }
    for z in 1557..=1576 {
        f.put(
            HEIGHT_BYTES + z * 5632 + 3007,
            &[if (1559..=1562).contains(&z) { 98 } else { 97 }],
        );
    }
    f.put(0x50000000 + (1568 * 5632 + 3008) * 2, &117u16.to_le_bytes());
    f.put(DEF + 0xca, &[1]);
    let mut p = vec![0; 118];
    p[117] = PROVINCE;
    f.array(GS + 0x278, &p);
    p[117] = DEF;
    f.array(DATA + 0x208, &p);
    f.put(ROOT + 0x95, &[8]);
    f.put(FIND_ROOT + 0x95, &[0]);
    f.array(COUNTRY + 0x250, &[]);
    f.array(COUNTRY + 0x238, &[]);
    f.array(GS + 0x5e0, &[]);
    f
}
