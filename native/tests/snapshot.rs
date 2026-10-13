use hoi4_live_native::{
    error::{Error, Result},
    game, gui,
    memory::ReadMemory,
    platform::Process,
    research, research_ui, technology,
};
use std::{cell::Cell, collections::BTreeMap};
struct Fixture {
    tracked_reads: std::cell::RefCell<BTreeMap<u64, usize>>,
    data: BTreeMap<u64, u8>,
    clock_reads: Cell<u32>,
    torn: bool,
    checks: Cell<u32>,
    replaced: bool,
    id_reads: Cell<u32>,
    changed_id: bool,
    pause_reads: Cell<u32>,
    invalid_final_pause: bool,
    catalog_reads: Cell<u32>,
    changed_catalog: bool,
    geometry_reads: Cell<u32>,
    changed_geometry: bool,
    catalog_name_reads: Cell<u32>,
    changed_catalog_name: bool,
    catalog_template_reads: Cell<u32>,
    relocated_catalog: bool,
    fault: Option<(u64, usize, Vec<u8>)>,
    fault_reads: Cell<usize>,
    historical_reads: Cell<usize>,
    inconsistent_historical_body: bool,
    root_reads: Cell<usize>,
    split_context_roots: bool,
}
impl Fixture {
    fn put(&mut self, a: u64, data: &[u8]) {
        for (i, v) in data.iter().enumerate() {
            self.data.insert(a + i as u64, *v);
        }
    }
    fn region(&mut self, a: u64, n: usize) {
        self.put(a, &vec![0; n]);
    }
    fn word(&mut self, a: u64, n: u64) {
        self.put(a, &n.to_le_bytes());
    }
    fn number(&mut self, a: u64, n: i32) {
        self.put(a, &n.to_le_bytes());
    }
    fn string(&mut self, a: u64, text: &str) {
        assert!(text.len() <= 15);
        self.region(a, 32);
        self.word(a, a + 16);
        self.word(a + 8, text.len() as u64);
        self.put(a + 16, text.as_bytes());
    }
    fn object(&mut self, a: u64, name: &str, vt: u64) {
        self.region(vt - 8, 32);
        self.region(vt + 32, 128);
        self.word(a, vt);
        self.word(vt - 8, vt + 16);
        self.word(vt + 24, vt + 32);
        self.put(vt + 32, format!("{}{name}\0", name.len()).as_bytes());
    }
    fn array(&mut self, a: u64, data: u64, count: i32) {
        self.region(a, 16);
        self.word(a, data);
        self.number(a + 8, count);
        self.number(a + 12, count);
    }
    fn new() -> Self {
        let mut f = Self {
            tracked_reads: std::cell::RefCell::new(BTreeMap::new()),
            data: BTreeMap::new(),
            clock_reads: Cell::new(0),
            torn: false,
            checks: Cell::new(0),
            replaced: false,
            id_reads: Cell::new(0),
            changed_id: false,
            pause_reads: Cell::new(0),
            invalid_final_pause: false,
            catalog_reads: Cell::new(0),
            changed_catalog: false,
            geometry_reads: Cell::new(0),
            changed_geometry: false,
            catalog_name_reads: Cell::new(0),
            changed_catalog_name: false,
            catalog_template_reads: Cell::new(0),
            relocated_catalog: false,
            fault: None,
            fault_reads: Cell::new(0),
            historical_reads: Cell::new(0),
            inconsistent_historical_body: false,
            root_reads: Cell::new(0),
            split_context_roots: false,
        };
        let base = 0x10000;
        f.word(base + 0x44086a8, 0x30000);
        f.word(base + 0x44085f8, 0x32000);
        f.word(base + 0x439f450, 0x36000);
        f.word(base + 0x43f3eb8, 0x37000);
        for (a, n) in [
            (0x30000, 0x700),
            (0x32000, 0x700),
            (0x20000, 0x1400),
            (0x28000, 0x200),
            (0x2a000, 0x50),
        ] {
            f.region(a, n);
        }
        f.word(0x30000, base + 0x42c7598);
        f.word(0x32000, base + 0x42c9a88);
        f.number(0x30460, 60759371);
        f.number(0x304b4, 4);
        f.number(0x30518, 1);
        f.number(0x30620, 2);
        f.number(0x30628, 1);
        let id = b"aabbccdd-1122-4344-8566-778899aabbcc";
        f.region(0x30718, 32);
        f.word(0x30718, 0x60000);
        f.word(0x30720, id.len() as u64);
        f.put(0x60000, id);
        f.put(0x326c1, &[1]);
        f.array(0x30340, 0x39000, 3);
        f.region(0x39000, 12);
        f.number(0x39004, 2);
        f.array(0x30310, 0x3a000, 3);
        f.region(0x3a000, 24);
        f.word(0x3a000, 0xdead0000);
        f.word(0x3a008, 0xbeef0000);
        f.word(0x3a010, 0x20000);
        f.array(0x37000, 0x38000, 2);
        f.string(0x38000, "---");
        f.string(0x38020, "GER");
        f.string(0x20010, "German Reich");
        f.object(0x20000, "CCountry", 0x50000);
        f.word(0x21348, 0x2a000);
        f.object(0x2a000, "CNationalFocusProgress", 0x50100);
        f.word(0x20f38, 0x28000);
        f.object(0x28000, "CTechnologyStatus", 0x50200);
        f.array(0x280a0, 0x3b000, 4);
        f.region(0x3b000, 32);
        for i in 0..4 {
            let a = 0x2c000 + i * 0x100;
            f.region(a, 64);
            f.object(a, "CResearchSlot", 0x50300);
            f.word(0x3b000 + i * 8, a);
        }
        f
    }
    fn catalog(&mut self) {
        let base = 0x10000;
        self.region(0x36000, 0x100);
        self.word(0x36000, base + 0x42110c8);
        self.array(0x36048, 0x62000, 4);
        self.region(0x62000, 32);
        // Canonical indices are independent of copied template+0x34 values.
        for (i, name) in ["tools1", "tools2", "construction1"].iter().enumerate() {
            let ptr = 0x63000 + i as u64 * 0x100;
            self.region(ptr, 0x80);
            self.word(0x62008 + i as u64 * 8, ptr);
            self.word(ptr, base + 0x4211038);
            self.string(ptr + 8, name);
            self.number(ptr + 0x30, i as i32 + 1);
            self.number(ptr + 0x34, 999);
        }
    }
    fn geometry(&mut self) {
        let base = 0x10000;
        self.word(0x324f8, 0x70000);
        self.region(0x70000, 0xc0);
        self.word(0x70000, base + 0x42736f8);
        self.word(0x700b8, 0x71000);
        self.word(base + 0x4577890, 0x71000);
        self.region(0x71000, 0x200);
        self.word(0x71000, base + 0x42417f8);
        for (i, v) in [512, 288, 1280, 720].iter().enumerate() {
            self.number(0x71178 + i as u64 * 4, *v);
        }
        for (i, v) in [2.5f32, 2.5, 0.4, 0.4].iter().enumerate() {
            self.put(0x711d0 + i as u64 * 4, &v.to_le_bytes());
        }
        self.put(0x71040, &640f32.to_le_bytes());
        self.put(0x71044, &20f32.to_le_bytes());
    }
    fn own_research(&mut self) {
        self.catalog();
        let base = 0x10000;
        self.word(0x28000, base + 0x42108f8);
        self.word(0x28130, 0x20000);
        self.number(0x28138, 1);
        self.array(0x28088, 0x82000, 3);
        self.region(0x82000, 24);
        self.array(0x28040, 0x83000, 1);
        self.word(0x83000, 0x81000);
        for (i, (level, threshold)) in [(1, 2), (3, 3)].iter().enumerate() {
            let pointer = 0x80000 + i as u64 * 0x1000;
            let template = 0x63000 + i as u64 * 0x100;
            self.region(pointer, 0x1a0);
            self.word(pointer, base + 0x42107f8);
            self.word(pointer + 0x158, 0x28000);
            self.word(pointer + 0x160, template);
            self.number(pointer + 0x174, *level);
            self.number(template + 0x3cc, *threshold);
            self.word(0x82000 + i as u64 * 8, pointer);
        }
        self.put(0x80198, &(-0x1234_5678_9abci64).to_le_bytes());
        for i in 0..4 {
            let slot = 0x2c000 + i * 0x100;
            self.word(slot, base + 0x4210898);
            self.word(slot + 0x10, 0x28000);
            self.word(slot + 0x20, 999999);
        }
        self.word(0x2c018, 0x80000);
    }
    fn research_ui(&mut self) {
        self.own_research();
        self.geometry();
        let base = 0x10000;
        self.word(0x326b8, 0x90000);
        self.region(0x90000, 0x400);
        self.word(0x90180, 0x32000);
        self.word(0x900f0, 0x91000);
        self.word(0x900f8, 0x92000);
        self.number(0x903d8, 5);
        self.region(0x91000, 0x400);
        self.region(0x92000, 0xe30);
        self.word(0x91000, base + 0x424dc38);
        self.word(0x92000, base + 0x424de40);
        self.word(0x912f0, 0x32000);
        self.word(0x922f0, 0x32000);
        self.number(0x92e24, 6);
        self.word(0x92e08, 0x2c100);
        self.array(0x91308, 0x93000, 4);
        self.region(0x93000, 32);
        for i in 0..4 {
            let widget = 0x94000 + i * 0x1000;
            let container = 0xa0000 + i * 0x2000;
            self.region(widget, 0x300);
            self.word(widget, base + 0x424e7d8);
            self.word(widget + 0x2a8, 0x70000);
            self.word(widget + 0x2b0, 0x91000);
            self.word(widget + 0x2b8, 0x2c000 + i * 0x100);
            self.word(widget + 0x10, container);
            self.word(0x93000 + i * 8, widget);
            self.region(container, 0xf00);
            self.word(container, base + 0x4347d58);
            self.word(container + 0x98, 0x70000);
            self.put(container + 0xa5, &[8]);
        }
        self.word(0x912f8, 0xa0000);
        self.array(0xa0958, 0xb0000, 1);
        self.word(0xb0000, 0xb1000);
        self.region(0xb1000, 0x100);
        self.word(0xb10e8, 0xb2000);
        self.region(0xb2000, 0x1c0);
        self.number(0xb2034, 3);
        self.word(0xb2168, 0xb3000);
        self.word(0xb21b0, 0xb3000);
        self.region(0xb3000, 8);
        for (object, name, vptr, info, text) in [
            (
                0xb1000,
                "CButtonStandard",
                base + 0x43470c8,
                base + 0x4350000,
                base + 0x38afd74,
            ),
            (
                0xb2000,
                "CFrameAnimatedSprite",
                base + 0x434b370,
                base + 0x4351000,
                base + 0x38b4750,
            ),
            (
                0xb3000,
                "CFrameAnimatedSpriteType",
                base + 0x434b668,
                base + 0x4352000,
                base + 0x3507000,
            ),
        ] {
            self.word(object, vptr);
            self.word(vptr - 16, 0);
            self.word(vptr - 8, info);
            self.region(info, 16);
            self.word(info + 8, text);
            self.region(text, 128);
            self.put(text, format!("{}{name}", name.len()).as_bytes());
        }
    }
    fn research_drag(&mut self) {
        self.research_ui();
        let base = 0x10000;
        self.region(0xb1000, 0x120);
        self.word(0xb1000, base + 0x4346b10);
        self.word(base + 0x4346b10 - 16, 0);
        self.word(base + 0x4346b10 - 8, base + 0x4350000);
        self.region(base + 0x38afd74, 128);
        self.put(base + 0x38afd74, b"11CButtonDrag");
        self.word(0xb1110, 0xb2000);
        self.number(0xb10ac, 4);
        // Position data is deliberately unreadable as a pointer. The exact
        // Drag layout must not read the Standard renderer field at +e8.
        for address in 0xb10e8..0xb10f0 {
            self.data.remove(&address);
        }
    }
    fn research_plain_sprite(&mut self) {
        self.research_drag();
        let base = 0x10000;
        for (object, table, info, text, name) in [
            (0xb2000, 0x4356e08, 0x4351000, 0x38b4750, "7CSprite"),
            (0xb3000, 0x4357160, 0x4352000, 0x3507000, "11CSpriteType"),
        ] {
            self.word(object, base + table);
            self.word(base + table - 16, 0);
            self.word(base + table - 8, base + info);
            self.region(base + text, 128);
            self.put(base + text, name.as_bytes());
        }
        // The plain primary uses its direct resource at +168. Neither the
        // Instant +160 nor Frame +1b0 field exists in this fixture.
        for address in (0xb2160..0xb2168).chain(0xb21b0..0xb21b8) {
            self.data.remove(&address);
        }
    }
    fn research_info(&mut self) {
        self.research_ui();
        let base = 0x10000;
        self.array(0x92dc0, 0xc0000, 1);
        self.word(0xc0000, 0xc1000);
        self.region(0xc1000, 0xb00);
        self.word(0xc1000, base + 0x430bbf0);
        self.word(0xc1a78, 0x32000);
        self.word(0xc1a80, 0x92000);
        self.word(0xc1a98, 0x80000);
        self.word(0xc1a88, 0xc3000);
        self.word(0xc1ac0, 0xc5000);
        self.word(0xc1ab0, 0xc5000);
        self.region(0xc3000, 0xf00);
        self.word(0xc3000, base + 0x4347d58);
        self.word(0xc3098, 0x70000);
        self.put(0xc30a5, &[8]);
        self.array(0xc39a0, 0xc7000, 2);
        self.array(0xc39b8, 0xc6000, 2);
        self.word(0xc7000, 0xc8000);
        self.word(0xc7008, 0xc5000);
        self.string(0xc6000, "close");
        self.string(0xc6020, "research");
        self.region(0xc5000, 0x120);
        self.word(0xc5000, base + 0x43470c8);
        self.number(0xc50ac, 3);
        self.put(0xc5074, &0x1234_0800u32.to_le_bytes());
        // Another named registry has no readable payload here. It must not
        // be used in place of the actual virtual +88 button lookup.
        for address in (0xc3a60..0xc3a70).chain(0xc3a78..0xc3a88) {
            self.data.remove(&address);
        }
    }
    fn current_research_nodes(&mut self) {
        self.research_ui();
        let base = 0x10000;
        self.number(0x903d8, 6);
        self.put(0xa00a5, &[0]);
        self.word(0x922f8, 0xd0000);
        self.region(0xd0000, 0xf00);
        self.word(0xd0000, base + 0x4347d58);
        self.word(0xd0098, 0x70000);
        self.put(0xd00a5, &[8]);
        self.array(0x92dd8, 0xd2000, 2);
        self.region(0xd2000, 80);
        self.word(0xd2010, 0xd3000);
        self.word(0xd2020, 0x92000); // Context, not the item.
        self.word(0xd2038, 0xd3000);
        self.word(0xd2048, 0xdead0000); // Unreadable context, never dereferenced.
        self.array(0x92df0, 0, 0);
        self.region(0xd3000, 0xd0);
        self.word(0xd3000, base + 0x424e2e0);
        self.word(0xd3030, 0x92000);
        self.word(0xd30c8, 0x80000);
    }
    fn research_wait(&mut self) {
        self.research_drag();
        let base = 0x10000;
        // Real tables are near each other. Keep synthetic RTTI/names separate
        // rather than placing fake name buffers inside another real table.
        for (i, (a, name, vt)) in [
            (0x28000, "CTechnologyStatus", 0x42108f8),
            (0x2c000, "CResearchSlot", 0x4210898),
            (0x2c100, "CResearchSlot", 0x4210898),
            (0x2c200, "CResearchSlot", 0x4210898),
            (0x2c300, "CResearchSlot", 0x4210898),
            (0x80000, "CTechnology", 0x42107f8),
            (0x63000, "CTechnologyTemplate", 0x4211038),
        ]
        .iter()
        .enumerate()
        {
            let info = base + 0x4353000 + i as u64 * 0x100;
            let text = base + 0x3508000 + i as u64 * 0x100;
            self.word(*a, base + vt);
            self.word(base + vt - 16, 0);
            self.word(base + vt - 8, info);
            self.region(info, 16);
            self.word(info + 8, text);
            self.region(text, 128);
            self.put(text, format!("{}{name}", name.len()).as_bytes());
        }
    }
}
impl ReadMemory for Fixture {
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        *self.tracked_reads.borrow_mut().entry(a).or_default() += 1;
        if a == 0x10000 + 0x44086a8 {
            self.root_reads.set(self.root_reads.get() + 1);
            if self.split_context_roots && (2..=3).contains(&self.root_reads.get()) {
                return Ok(0x90000u64.to_le_bytes().to_vec());
            }
        }
        if a == 0x30628 {
            self.historical_reads.set(self.historical_reads.get() + 1);
            if self.inconsistent_historical_body && (2..=3).contains(&self.historical_reads.get()) {
                return Ok(0i32.to_le_bytes().to_vec());
            }
        }
        if let Some((address, after, replacement)) = &self.fault {
            if a == *address {
                self.fault_reads.set(self.fault_reads.get() + 1);
                if self.fault_reads.get() > *after {
                    assert_eq!(replacement.len(), n);
                    return Ok(replacement.clone());
                }
            }
        }
        if a == 0x63000 {
            self.catalog_template_reads
                .set(self.catalog_template_reads.get() + 1);
        }
        if a == 0x36048 && self.relocated_catalog && self.catalog_template_reads.get() > 0 {
            let mut bytes = 0x62100u64.to_le_bytes().to_vec();
            bytes.extend(4i32.to_le_bytes());
            bytes.extend(4i32.to_le_bytes());
            assert_eq!(bytes.len(), n);
            return Ok(bytes);
        }
        if a == 0x64000 {
            self.catalog_name_reads
                .set(self.catalog_name_reads.get() + 1);
            if self.changed_catalog_name && self.catalog_name_reads.get() > 1 {
                return Ok(vec![b'z'; n]);
            }
        }
        if a == 0x71040 {
            self.geometry_reads.set(self.geometry_reads.get() + 1);
            if self.changed_geometry && self.geometry_reads.get() > 1 {
                return Ok(vec![0; n]);
            }
        }
        if a == 0x62000 {
            self.catalog_reads.set(self.catalog_reads.get() + 1);
            if self.changed_catalog && self.catalog_reads.get() > 1 {
                return Ok(vec![0; n]);
            }
        }
        if a == 0x326c1 {
            self.pause_reads.set(self.pause_reads.get() + 1);
            if self.invalid_final_pause && self.pause_reads.get() > 2 {
                return Ok(vec![2]);
            }
        }
        if a == 0x60000 {
            self.id_reads.set(self.id_reads.get() + 1);
            if self.changed_id && self.id_reads.get() > 1 {
                return Ok(b"aabbccdd-1122-4344-8566-778899aabbcd".to_vec());
            }
        }
        if a == 0x30460 {
            self.clock_reads.set(self.clock_reads.get() + 1);
            if self.torn && self.clock_reads.get() > 1 {
                return Ok(60759372i32.to_le_bytes().to_vec());
            }
        }
        (0..n)
            .map(|i| {
                self.data.get(&(a + i as u64)).copied().ok_or_else(|| {
                    Error::new("unexpected_read", format!("Fixture forbids reading {a:x}"))
                })
            })
            .collect()
    }
}
impl Process for Fixture {
    fn image_base(&self) -> u64 {
        0x10000
    }
    fn check(&self) -> Result<()> {
        self.checks.set(self.checks.get() + 1);
        if self.replaced && self.checks.get() > 1 {
            Err(Error::new("ownership_changed", "Fixture replacement"))
        } else {
            Ok(())
        }
    }
}
#[test]
fn own_research_uses_completed_membership_and_active_i64_counter() {
    let mut f = Fixture::new();
    f.own_research();
    let value = research::snapshot(&f).unwrap();
    assert_eq!(value["researched"][0]["id"], "tools2");
    assert_eq!(value["researched"][0]["level_raw"], 3);
    assert_eq!(value["researched"][0]["completion_threshold_raw"], 3);
    assert_eq!(value["researched"].as_array().unwrap().len(), 1);
    assert_eq!(value["slots"][0]["researching"], "tools1");
    assert_eq!(value["slots"][0]["progress_raw"], -0x1234_5678_9abci64);
    assert!(value["slots"][0]["progress"].is_null());
    assert!(value["slots"][1]["progress_raw"].is_null());
    assert_eq!(f.checks.get(), 2);
}
#[test]
fn research_pages_sorted_membership_and_distinguishes_known_empty() {
    let mut f = Fixture::new();
    f.own_research();
    f.number(0x80174, 2);
    f.array(0x28040, 0x83000, 2);
    f.word(0x83008, 0x80000);
    let first = research::page(&f, 0, 1).unwrap();
    assert_eq!(first["researched_count"], 2);
    assert_eq!(first["researched"][0]["id"], "tools1");
    assert_eq!(first["next_offset"], 1);
    let next = research::page(&f, 1, 1).unwrap();
    assert_eq!(next["researched"][0]["id"], "tools2");
    assert!(next["next_offset"].is_null());
    assert_eq!(
        research::page(&f, 2, 1).unwrap()["researched"],
        serde_json::json!([])
    );
    f.array(0x28040, 0, 0);
    assert_eq!(research::snapshot(&f).unwrap()["researched_count"], 0);
}
#[test]
fn research_refuses_foreign_owners_and_inconsistent_canonical_joins() {
    for address in [0x28130, 0x2c010, 0x80158, 0x80160, 0x82000] {
        let mut f = Fixture::new();
        f.own_research();
        f.word(address, 0xdead0000);
        assert!(
            research::snapshot(&f).is_err(),
            "accepted changed field {address:x}"
        );
    }
    let mut f = Fixture::new();
    f.own_research();
    f.number(0x63030, 2);
    assert_eq!(
        research::snapshot(&f).unwrap_err().code,
        "invalid_research_join"
    );
}
#[test]
fn research_refuses_duplicate_members_partial_completion_and_uninitialized_arrays() {
    let mut f = Fixture::new();
    f.own_research();
    f.array(0x28040, 0x83000, 2);
    f.word(0x83008, 0x81000);
    assert_eq!(
        research::snapshot(&f).unwrap_err().code,
        "invalid_completed_research"
    );
    let mut f = Fixture::new();
    f.own_research();
    f.number(0x81174, 2);
    assert_eq!(
        research::snapshot(&f).unwrap_err().code,
        "invalid_completed_research"
    );
    let mut f = Fixture::new();
    f.own_research();
    f.array(0x28088, 0x82000, 2);
    assert_eq!(
        research::snapshot(&f).unwrap_err().code,
        "research_unavailable"
    );
    let mut f = Fixture::new();
    f.own_research();
    f.number(0x28090, 8193);
    assert_eq!(research::snapshot(&f).unwrap_err().code, "invalid_array");
}
#[test]
fn research_refuses_changed_counters_payloads_slots_and_relocated_lists() {
    for (address, replacement) in [
        (0x80198, 17i64.to_le_bytes().to_vec()),
        (0x83000, 0x80000u64.to_le_bytes().to_vec()),
        (
            0x82000,
            [0u64, 0x81000, 0]
                .into_iter()
                .flat_map(u64::to_le_bytes)
                .collect(),
        ),
        (0x2c018, 0u64.to_le_bytes().to_vec()),
        (0x28130, 0xdead0000u64.to_le_bytes().to_vec()),
    ] {
        let mut f = Fixture::new();
        f.own_research();
        f.fault = Some((address, 1, replacement));
        assert!(
            research::snapshot(&f).is_err(),
            "accepted torn field {address:x}"
        );
    }
    let mut f = Fixture::new();
    f.own_research();
    f.region(0x84000, 24);
    f.word(0x84000, 0x80000);
    f.word(0x84008, 0x81000);
    let mut relocated = 0x84000u64.to_le_bytes().to_vec();
    relocated.extend(3i32.to_le_bytes());
    relocated.extend(3i32.to_le_bytes());
    f.fault = Some((0x28088, 2, relocated));
    assert_eq!(research::snapshot(&f).unwrap_err().code, "torn_read");
}
#[test]
fn research_retains_final_process_check_and_refuses_wrong_primary_type() {
    let mut f = Fixture::new();
    f.own_research();
    f.replaced = true;
    assert_eq!(
        research::snapshot(&f).unwrap_err().code,
        "ownership_changed"
    );
    let mut f = Fixture::new();
    f.own_research();
    f.word(0x80000, 0x10000 + 0x4210868);
    assert_eq!(research::snapshot(&f).unwrap_err().code, "invalid_type");
}
#[test]
fn player_tag_index_uses_mapping_not_country_index() {
    let f = Fixture::new();
    let value = game::snapshot(&f).unwrap();
    assert_eq!(value["player"], "GER");
    assert_eq!(value["country"], "German Reich");
    assert_eq!(value["research"].as_array().unwrap().len(), 4);
    assert!(value["focus"].is_null());
    assert_eq!(f.checks.get(), 2);
}
#[test]
fn changed_clock_refuses_snapshot() {
    let mut f = Fixture::new();
    f.torn = true;
    assert_eq!(game::snapshot(&f).unwrap_err().code, "torn_read");
}
#[test]
fn replaced_process_refuses_snapshot() {
    let mut f = Fixture::new();
    f.replaced = true;
    assert_eq!(game::snapshot(&f).unwrap_err().code, "ownership_changed");
}

#[test]
fn own_clock_context_has_a_bounded_campaign_identity() {
    let fixture = Fixture::new();
    let context = game::clock_context(&fixture).unwrap();
    assert_eq!(context.player, "GER");
    assert_eq!(
        context.playthrough_id,
        "aabbccdd-1122-4344-8566-778899aabbcc"
    );
    assert_eq!(context.historical_raw, 1);
    assert!(context.paused);
    assert_eq!(fixture.checks.get(), 2);
}
#[test]
fn wait_payload_and_campaign_witness_share_one_owned_bracket() {
    let fixture = Fixture::new();
    let (value, context) = game::wait_snapshot(&fixture).unwrap();
    assert_eq!(value["player"], context.player);
    assert_eq!(value["date"]["raw"], context.date_raw);
    assert_eq!(value["paused"], context.paused);
    assert_eq!(fixture.checks.get(), 2);
    let mut changed = Fixture::new();
    changed.inconsistent_historical_body = true;
    assert_eq!(game::wait_snapshot(&changed).unwrap_err().code, "torn_read");
    let mut changed = Fixture::new();
    let copy: Vec<_> = (0..0x800)
        .map(|i| changed.data.get(&(0x30000 + i)).copied().unwrap_or(0))
        .collect();
    changed.put(0x90000, &copy);
    changed.split_context_roots = true;
    assert_eq!(game::wait_snapshot(&changed).unwrap_err().code, "torn_read");
    let mut changed = Fixture::new();
    changed.changed_id = true;
    assert_eq!(game::wait_snapshot(&changed).unwrap_err().code, "torn_read");
    let mut changed = Fixture::new();
    changed.replaced = true;
    assert_eq!(
        game::wait_snapshot(&changed).unwrap_err().code,
        "ownership_changed"
    );
}

#[test]
fn clock_context_retains_final_ownership_and_clock_readback() {
    let mut fixture = Fixture::new();
    fixture.replaced = true;
    assert_eq!(
        game::clock_context(&fixture).unwrap_err().code,
        "ownership_changed"
    );
    assert_eq!(fixture.checks.get(), 2);
    let mut fixture = Fixture::new();
    fixture.torn = true;
    assert_eq!(game::clock_context(&fixture).unwrap_err().code, "torn_read");
}

#[test]
fn invalid_final_pause_byte_cannot_masquerade_as_false() {
    let mut fixture = Fixture::new();
    fixture.put(0x326c1, &[0]);
    fixture.invalid_final_pause = true;
    assert_eq!(game::clock_context(&fixture).unwrap_err().code, "torn_read");
}

#[test]
fn unchanged_string_header_does_not_hide_changed_campaign_contents() {
    let mut fixture = Fixture::new();
    fixture.changed_id = true;
    assert_eq!(game::clock_context(&fixture).unwrap_err().code, "torn_read");
    let mut fixture = Fixture::new();
    fixture.number(0x30720, 4097);
    assert_eq!(
        game::clock_context(&fixture).unwrap_err().code,
        "invalid_string"
    );
}

#[test]
fn technology_catalog_filters_pages_and_uses_primary_ids() {
    let mut fixture = Fixture::new();
    fixture.catalog();
    let first = technology::catalog(&fixture, "TOOLS", 0, 1).unwrap();
    assert_eq!(first["definition_count"], 3);
    assert_eq!(first["count"], 2);
    assert_eq!(first["technologies"], serde_json::json!(["tools1"]));
    assert_eq!(first["next_offset"], 1);
    assert_eq!(fixture.checks.get(), 2);
    let next = technology::catalog(&fixture, "tools", 1, 1).unwrap();
    assert_eq!(next["technologies"], serde_json::json!(["tools2"]));
    assert!(next["next_offset"].is_null());
    assert!(
        technology::catalog(&fixture, "no_match", 0, 100).unwrap()["technologies"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn malformed_and_changed_catalogs_return_no_page() {
    let mut fixture = Fixture::new();
    fixture.catalog();
    fixture.number(0x63030, 2);
    assert_eq!(
        technology::catalog(&fixture, "", 0, 100).unwrap_err().code,
        "invalid_catalog"
    );
    let mut fixture = Fixture::new();
    fixture.catalog();
    fixture.string(0x63108, "tools1");
    assert_eq!(
        technology::catalog(&fixture, "", 0, 100).unwrap_err().code,
        "invalid_catalog"
    );
    let mut fixture = Fixture::new();
    fixture.catalog();
    fixture.changed_catalog = true;
    assert_eq!(
        technology::catalog(&fixture, "", 0, 100).unwrap_err().code,
        "torn_read"
    );
    let mut fixture = Fixture::new();
    fixture.catalog();
    let name = b"long_technology_identifier";
    fixture.region(0x63008, 32);
    fixture.word(0x63008, 0x64000);
    fixture.word(0x63010, name.len() as u64);
    fixture.put(0x64000, name);
    fixture.changed_catalog_name = true;
    assert_eq!(
        technology::catalog(&fixture, "", 0, 100).unwrap_err().code,
        "torn_read"
    );
    let mut fixture = Fixture::new();
    fixture.catalog();
    fixture.replaced = true;
    assert_eq!(
        technology::catalog(&fixture, "", 0, 100).unwrap_err().code,
        "ownership_changed"
    );
}

#[test]
fn oversized_catalog_is_refused_before_pointer_payload() {
    let mut fixture = Fixture::new();
    fixture.catalog();
    fixture.array(0x36048, 0x62000, 8193);
    assert_eq!(
        technology::catalog(&fixture, "", 0, 100).unwrap_err().code,
        "invalid_array"
    );
    assert_eq!(fixture.catalog_reads.get(), 0);
    let mut fixture = Fixture::new();
    fixture.catalog();
    fixture.region(0x63008, 32);
    fixture.word(0x63008, 0x64000);
    fixture.word(0x63010, 257);
    assert_eq!(
        technology::catalog(&fixture, "", 0, 100).unwrap_err().code,
        "invalid_string"
    );
    assert_eq!(fixture.catalog_name_reads.get(), 0);
}

#[test]
fn relocated_catalog_with_identical_entries_is_a_changed_snapshot() {
    let mut fixture = Fixture::new();
    fixture.catalog();
    let bytes: Vec<_> = (0..32).map(|i| fixture.data[&(0x62000 + i)]).collect();
    fixture.put(0x62100, &bytes);
    fixture.relocated_catalog = true;
    assert_eq!(
        technology::catalog(&fixture, "", 0, 100).unwrap_err().code,
        "torn_read"
    );
}

#[test]
fn input_geometry_is_diagnostic_and_uses_finalized_ratios() {
    let mut fixture = Fixture::new();
    fixture.geometry();
    let value = gui::input_geometry(&fixture).unwrap();
    assert_eq!(value["physical_dimensions"], serde_json::json!([512, 288]));
    assert_eq!(value["logical_dimensions"], serde_json::json!([1280, 720]));
    assert_eq!(value["logical_pointer"], serde_json::json!([640.0, 20.0]));
    assert_eq!(value["input_mapping_verified"], false);
    assert_eq!(fixture.checks.get(), 2);
}

#[test]
fn wrong_gui_type_inconsistent_scales_and_torn_geometry_fail() {
    let mut fixture = Fixture::new();
    fixture.geometry();
    fixture.word(0x70000, 0x10000 + 0x434c068);
    let refusal = gui::input_geometry(&fixture).unwrap_err();
    assert_eq!(refusal.code, "gui_unavailable");
    assert!(refusal.message.contains("CGameGui"));
    let mut fixture = Fixture::new();
    fixture.geometry();
    fixture.word(0x71000, 0x10000 + 0x43608e0);
    assert_eq!(
        gui::input_geometry(&fixture).unwrap_err().code,
        "gui_unavailable"
    );
    let mut fixture = Fixture::new();
    fixture.geometry();
    fixture.put(0x711d0, &3.0f32.to_le_bytes());
    assert_eq!(
        gui::input_geometry(&fixture).unwrap_err().code,
        "invalid_gui_geometry"
    );
    let mut fixture = Fixture::new();
    fixture.geometry();
    fixture.changed_geometry = true;
    assert_eq!(gui::input_geometry(&fixture).unwrap_err().code, "torn_read");
    let mut fixture = Fixture::new();
    fixture.geometry();
    fixture.replaced = true;
    assert_eq!(
        gui::input_geometry(&fixture).unwrap_err().code,
        "ownership_changed"
    );
}

#[test]
fn research_ui_reports_owned_widgets_and_diagnostic_types_without_input() {
    let mut f = Fixture::new();
    f.research_ui();
    let v = research_ui::snapshot(&f).unwrap();
    assert_eq!(v["own_slot_count"], 4);
    assert_eq!(v["widget_count"], 4);
    assert_eq!(v["selected_slot"], 1);
    assert_eq!(v["active_mode_raw"], 5);
    assert_eq!(v["panel"]["visible_predicate"], true);
    assert_eq!(v["ordinary_tree"]["materialized"], false);
    assert_eq!(
        v["slots"][0]["buttons_observed"][0]["type_encoding"],
        "15CButtonStandard"
    );
    assert_eq!(
        v["slots"][0]["buttons_observed"][0]["render_type_encoding"],
        "20CFrameAnimatedSprite"
    );
    assert_eq!(
        v["slots"][0]["buttons_observed"][0]["geometry_verified"],
        false
    );
    assert_eq!(v["input_authorized"], false);
    let renderer = &v["slots"][0]["buttons_observed"][0]["render_details"];
    assert_eq!(renderer["current_frame_raw"], 3);
    assert_eq!(renderer["resources_same"], true);
    assert_eq!(
        renderer["hit_resource_type_encoding"],
        "24CFrameAnimatedSpriteType"
    );
    assert_eq!(f.checks.get(), 2);
}
#[test]
fn research_ui_distinguishes_hidden_unmaterialized_and_incomplete_widgets() {
    let mut f = Fixture::new();
    f.research_ui();
    f.number(0xa0ef8, 2);
    assert_eq!(
        research_ui::snapshot(&f).unwrap()["panel"]["visible_predicate"],
        false
    );
    f.array(0x91308, 0, 0);
    f.word(0x92e08, 0);
    let v = research_ui::snapshot(&f).unwrap();
    assert_eq!(v["widget_count"], 0);
    assert_eq!(v["widgets_complete"], false);
    assert!(v["selected_slot"].is_null());
    assert_eq!(v["own_slot_count"], 4);
}
#[test]
fn research_ui_refuses_foreign_and_wrong_slot_or_view_joins() {
    for (address, value, code) in [
        (0x92e20, 2, "foreign_ui_context"),
        (0x90180, 0xdead0000, "invalid_gui_owner"),
        (0x922f0, 0xdead0000, "invalid_gui_owner"),
        (0x92e24, 7, "invalid_gui_owner"),
        (0x92e08, 0xdead0000, "invalid_research_owner"),
        (0x942b8, 0x2c100, "invalid_research_join"),
        (0x942b0, 0x92000, "invalid_research_join"),
        (0x942a8, 0xdead0000, "invalid_research_join"),
        (0xa0098, 0xdead0000, "invalid_gui_owner"),
        (0x93008, 0x94000, "invalid_research_join"),
        (0x2c010, 0xdead0000, "invalid_research_owner"),
    ] {
        let mut f = Fixture::new();
        f.research_ui();
        if [0x92e20, 0x92e24].contains(&address) {
            f.number(address, value as i32);
        } else {
            f.word(address, value);
        }
        assert_eq!(research_ui::snapshot(&f).unwrap_err().code, code);
    }
}
#[test]
fn research_ui_checks_counts_before_payload_and_rejects_secondary_metadata() {
    for (address, value, code) in [
        (0x91310, 33, "invalid_array"),
        (0xa0960, 65, "invalid_array"),
        (0x10000 + 0x43470c8 - 16, 8, "invalid_gui_type"),
        (0x10000 + 0x4350000 + 8, 0x90000, "invalid_gui_type"),
        (
            0x10000 + 0x43470c8 - 8,
            0x10000 + 0x439e7a0,
            "invalid_gui_type",
        ),
        (
            0x10000 + 0x4350000 + 8,
            0x10000 + 0x436f150,
            "invalid_gui_type",
        ),
    ] {
        let mut f = Fixture::new();
        f.research_ui();
        if [0x91310, 0xa0960].contains(&address) {
            f.number(address, value as i32);
        } else {
            f.word(address, value);
        }
        assert_eq!(research_ui::snapshot(&f).unwrap_err().code, code);
    }
}
#[test]
fn research_ui_checks_final_headers_payload_owners_visibility_and_types() {
    for (address, size, value) in [
        (0x326b8, 8, 0xdead0000u64.to_le_bytes().to_vec()),
        (0x90180, 8, 0xdead0000u64.to_le_bytes().to_vec()),
        (0x903d8, 4, 6i32.to_le_bytes().to_vec()),
        (0x93000, 32, vec![0; 32]),
        (0x942b8, 8, 0x2c100u64.to_le_bytes().to_vec()),
        (0x942a8, 8, 0xdead0000u64.to_le_bytes().to_vec()),
        (0xa0ef8, 4, 2i32.to_le_bytes().to_vec()),
        (0xb10ac, 4, 3i32.to_le_bytes().to_vec()),
        (0xb10e8, 8, 0u64.to_le_bytes().to_vec()),
        (0xb2034, 4, 1u32.to_le_bytes().to_vec()),
        (0xb2168, 8, 0u64.to_le_bytes().to_vec()),
        (0xb21b0, 8, 0u64.to_le_bytes().to_vec()),
        (0x10000 + 0x38b4750, 128, vec![0; 128]),
    ] {
        let mut f = Fixture::new();
        f.research_ui();
        f.fault = Some((address, if address == 0xa0ef8 { 2 } else { 1 }, value));
        assert!(size > 0);
        assert_eq!(
            research_ui::snapshot(&f).unwrap_err().code,
            "torn_read",
            "changed field {address:x}"
        );
    }
    let mut f = Fixture::new();
    f.research_ui();
    f.replaced = true;
    assert_eq!(
        research_ui::snapshot(&f).unwrap_err().code,
        "ownership_changed"
    );
}

#[test]
fn research_ui_unknown_primary_table_never_implies_button_layout() {
    let mut f = Fixture::new();
    f.research_ui();
    let table = 0x10000 + 0x4347400;
    f.word(0xb1000, table);
    f.word(table - 16, 0);
    f.word(table - 8, 0x10000 + 0x4350000);
    for (address, size) in [(0xb10ac, 4), (0xb10e8, 8)] {
        for n in 0..size {
            f.data.remove(&(address + n));
        }
    }
    let v = research_ui::snapshot(&f).unwrap();
    let button = &v["slots"][0]["buttons_observed"][0];
    assert_eq!(button["type_encoding"], "15CButtonStandard");
    assert!(button.get("disabled_raw").is_none());
    assert!(button.get("render_type_encoding").is_none());
}

#[test]
fn research_ui_exact_drag_uses_its_renderer_and_reports_state_without_enable_claim() {
    let mut f = Fixture::new();
    f.research_drag();
    let v = research_ui::snapshot(&f).unwrap();
    let button = &v["slots"][0]["buttons_observed"][0];
    assert_eq!(button["type_encoding"], "11CButtonDrag");
    assert_eq!(button["state_raw"], 4);
    assert!(button.get("disabled_raw").is_none());
    assert_eq!(button["render_type_encoding"], "20CFrameAnimatedSprite");
    assert_eq!(button["geometry_verified"], false);
    assert_eq!(v["input_authorized"], false);
    f.word(0xb1110, 0);
    assert!(
        research_ui::snapshot(&f).unwrap()["slots"][0]["buttons_observed"][0]
            ["render_type_encoding"]
            .is_null()
    );
}

#[test]
fn research_ui_drag_end_witnesses_refuse_state_or_renderer_changes() {
    for (address, value) in [
        (0xb10ac, 2i32.to_le_bytes().to_vec()),
        (0xb1110, 0u64.to_le_bytes().to_vec()),
    ] {
        let mut f = Fixture::new();
        f.research_drag();
        f.fault = Some((address, 1, value));
        assert_eq!(research_ui::snapshot(&f).unwrap_err().code, "torn_read");
    }
    let mut f = Fixture::new();
    f.research_drag();
    let table = 0x10000 + 0x4347400;
    f.word(0xb1000, table);
    f.word(table - 16, 0);
    f.word(table - 8, 0x10000 + 0x4350000);
    for (address, size) in [(0xb10ac, 4), (0xb1110, 8)] {
        for n in 0..size {
            f.data.remove(&(address + n));
        }
    }
    let v = research_ui::snapshot(&f).unwrap();
    let button = &v["slots"][0]["buttons_observed"][0];
    assert_eq!(button["type_encoding"], "11CButtonDrag");
    assert!(button.get("state_raw").is_none());
    assert!(button.get("render_type_encoding").is_none());
}

#[test]
fn research_ui_instant_and_unknown_renderers_do_not_borrow_frame_layout() {
    let mut f = Fixture::new();
    f.research_drag();
    let base = 0x10000;
    let table = base + 0x434c5b0;
    f.word(0xb2000, table);
    f.word(table - 16, 0);
    f.word(table - 8, base + 0x4351000);
    f.word(0xb2160, 0xb3000);
    for address in (0xb2168..0xb2170).chain(0xb21b0..0xb21b8) {
        f.data.remove(&address);
    }
    let v = research_ui::snapshot(&f).unwrap();
    let details = &v["slots"][0]["buttons_observed"][0]["render_details"];
    assert_eq!(details["current_frame_raw"], 3);
    assert_eq!(
        details["hit_resource_type_encoding"],
        "24CFrameAnimatedSpriteType"
    );
    assert!(details.get("resources_same").is_none());
    let table = base + 0x434b400;
    f.word(0xb2000, table);
    f.word(table - 16, 0);
    f.word(table - 8, base + 0x4351000);
    for (address, size) in [(0xb2034, 4), (0xb2160, 8)] {
        for n in 0..size {
            f.data.remove(&(address + n));
        }
    }
    let v = research_ui::snapshot(&f).unwrap();
    assert_eq!(
        v["slots"][0]["buttons_observed"][0]["render_details"]["field_layout_verified"],
        false
    );
}

#[test]
fn research_ui_plain_sprite_uses_direct_resource_without_other_renderer_fields() {
    let mut f = Fixture::new();
    f.research_plain_sprite();
    let v = research_ui::snapshot(&f).unwrap();
    let button = &v["slots"][0]["buttons_observed"][0];
    assert_eq!(button["render_type_encoding"], "7CSprite");
    let details = &button["render_details"];
    assert_eq!(details["current_frame_raw"], 3);
    assert_eq!(details["hit_resource_type_encoding"], "11CSpriteType");
    assert!(details.get("resources_same").is_none());
    assert_eq!(details["geometry_verified"], false);
    assert_eq!(v["input_authorized"], false);
    f.word(0xb2168, 0);
    assert!(
        research_ui::snapshot(&f).unwrap()["slots"][0]["buttons_observed"][0]["render_details"]
            ["hit_resource_type_encoding"]
            .is_null()
    );
}

#[test]
fn research_ui_plain_sprite_witnesses_changes_and_rejects_name_only_layouts() {
    for (address, value) in [
        (0xb2034, 4u32.to_le_bytes().to_vec()),
        (0xb2168, 0u64.to_le_bytes().to_vec()),
        (0xb3000, 0u64.to_le_bytes().to_vec()),
    ] {
        let mut f = Fixture::new();
        f.research_plain_sprite();
        f.fault = Some((address, 1, value));
        assert_eq!(research_ui::snapshot(&f).unwrap_err().code, "torn_read");
    }
    let mut f = Fixture::new();
    f.research_plain_sprite();
    let table = 0x10000 + 0x4356e80;
    f.word(0xb2000, table);
    f.word(table - 16, 0);
    f.word(table - 8, 0x10000 + 0x4351000);
    for (address, size) in [(0xb2034, 4), (0xb2168, 8)] {
        for n in 0..size {
            f.data.remove(&(address + n));
        }
    }
    let v = research_ui::snapshot(&f).unwrap();
    let button = &v["slots"][0]["buttons_observed"][0];
    assert_eq!(button["render_type_encoding"], "7CSprite");
    assert_eq!(button["render_details"]["field_layout_verified"], false);
}

#[test]
fn research_info_reports_canonical_selection_and_ordinary_button_without_eligibility() {
    let mut f = Fixture::new();
    f.research_info();
    let v = research_ui::snapshot(&f).unwrap();
    let infos = &v["technology_info"];
    assert_eq!(infos["count"], 1);
    assert_eq!(infos["visible_count"], 1);
    let info = &infos["windows"][0];
    assert_eq!(info["technology"], "tools1"); // Template+34 deliberately is 999.
    assert_eq!(info["eligibility"], "unavailable");
    let button = &info["ordinary_research_button"];
    assert_eq!(button["disabled_predicate"], true);
    assert_eq!(button["flags_raw"], 0x1234_0800u32);
    assert_eq!(button["selected_control_alias_matches"], true);
    assert_eq!(button["input_authorized"], false);
    f.word(0xc1ab0, 0xc9000); // Special branch alias is not ordinary control.
    f.number(0xc50ac, 0);
    f.number(0xc3eb0, 2);
    let v = research_ui::snapshot(&f).unwrap();
    assert_eq!(v["technology_info"]["visible_count"], 0);
    let button = &v["technology_info"]["windows"][0]["ordinary_research_button"];
    assert_eq!(button["disabled_predicate"], false);
    assert_eq!(button["selected_control_alias_matches"], false);
    assert_eq!(button["input_authorized"], false);
}

#[test]
fn research_info_retained_empty_selection_and_unmaterialized_window_are_explicit() {
    let mut f = Fixture::new();
    f.research_info();
    f.word(0xc1a88, 0);
    let v = research_ui::snapshot(&f).unwrap();
    assert_eq!(v["technology_info"]["windows"][0]["technology"], "tools1");
    assert_eq!(
        v["technology_info"]["windows"][0]["window"]["materialized"],
        false
    );
    assert_eq!(
        v["technology_info"]["windows"][0]["ordinary_research_button"]["materialized"],
        false
    );
    f.word(0xc1a98, 0);
    // Empty retained objects require neither a database nor a guessed control.
    for address in (0xc1ab0..0xc1ab8)
        .chain(0xc1ac0..0xc1ac8)
        .chain((0x10000 + 0x439f450)..(0x10000 + 0x439f458))
    {
        f.data.remove(&address);
    }
    let v = research_ui::snapshot(&f).unwrap();
    let info = &v["technology_info"]["windows"][0];
    assert!(info["technology"].is_null());
    assert!(info["ordinary_research_button"].is_null());
    assert_eq!(info["eligibility"], "unavailable");
}

#[test]
fn research_info_refuses_foreign_objects_wrong_catalog_and_misbound_controls() {
    for (address, value, code) in [
        (0xc1a78, 0xdead0000, "invalid_gui_owner"),
        (0xc1a80, 0xdead0000, "invalid_gui_owner"),
        (0xc3098, 0xdead0000, "invalid_gui_owner"),
        (0x80158, 0xdead0000, "invalid_research_owner"),
        (0x62008, 0x63100, "invalid_research_join"),
        (0x82000, 0x81000, "invalid_research_join"),
        (0xc1a98, 0x63000, "invalid_gui_type"),
        (0xc7008, 0xc8000, "invalid_gui_registry"),
        (0xc1ac0, 0xc8000, "invalid_gui_registry"),
        (0xc5000, 0x10000 + 0x4346b10, "invalid_gui_type"),
    ] {
        let mut f = Fixture::new();
        f.research_info();
        f.word(address, value);
        if address == 0x80158 {
            for a in 0x80160..0x80168 {
                f.data.remove(&a);
            }
        }
        if address == 0xc3098 {
            for a in 0xc39a0..0xc39c8 {
                f.data.remove(&a);
            }
        }
        assert_eq!(
            research_ui::snapshot(&f).unwrap_err().code,
            code,
            "{address:x}"
        );
    }
}

#[test]
fn research_info_bounds_and_unique_named_lookup_precede_untrusted_payloads() {
    let mut f = Fixture::new();
    f.research_info();
    f.array(0x92dc0, 0xdead0000, 65);
    assert_eq!(research_ui::snapshot(&f).unwrap_err().code, "invalid_array");
    let mut f = Fixture::new();
    f.research_info();
    f.array(0x92dc0, 0xc0000, 2);
    f.word(0xc0008, 0xc1000);
    assert_eq!(
        research_ui::snapshot(&f).unwrap_err().code,
        "invalid_gui_owner"
    );
    let mut f = Fixture::new();
    f.research_info();
    f.array(0xc39b8, 0xdead0000, 257);
    assert_eq!(
        research_ui::snapshot(&f).unwrap_err().code,
        "invalid_gui_registry"
    );
    let mut f = Fixture::new();
    f.research_info();
    f.string(0xc6000, "research");
    assert_eq!(
        research_ui::snapshot(&f).unwrap_err().code,
        "invalid_gui_registry"
    );
    let mut f = Fixture::new();
    f.research_info();
    f.string(0xc6020, "other");
    assert_eq!(
        research_ui::snapshot(&f).unwrap_err().code,
        "gui_unavailable"
    );
}

#[test]
fn research_info_end_witnesses_cover_selection_registry_controls_and_catalog() {
    for (address, value) in [
        (
            0x92dc0,
            [
                0u64.to_le_bytes().as_slice(),
                1i32.to_le_bytes().as_slice(),
                1i32.to_le_bytes().as_slice(),
            ]
            .concat(),
        ),
        (0xc0000, 0u64.to_le_bytes().to_vec()),
        (0xc1a78, 0u64.to_le_bytes().to_vec()),
        (0xc1a98, 0u64.to_le_bytes().to_vec()),
        (0xc1a88, 0u64.to_le_bytes().to_vec()),
        (0xc3098, 0u64.to_le_bytes().to_vec()),
        (
            0xc39b8,
            [
                0u64.to_le_bytes().as_slice(),
                2i32.to_le_bytes().as_slice(),
                2i32.to_le_bytes().as_slice(),
            ]
            .concat(),
        ),
        (0xc1ab0, 0u64.to_le_bytes().to_vec()),
        (0xc50ac, 0i32.to_le_bytes().to_vec()),
        (0xc5074, 0u32.to_le_bytes().to_vec()),
        (0x80158, 0u64.to_le_bytes().to_vec()),
        (0x80160, 0u64.to_le_bytes().to_vec()),
        (0x63030, 2i32.to_le_bytes().to_vec()),
    ] {
        let mut f = Fixture::new();
        f.research_info();
        f.fault = Some((address, 1, value));
        assert_eq!(
            research_ui::snapshot(&f).unwrap_err().code,
            "torn_read",
            "{address:x}"
        );
    }
}

struct UiWaitFixture {
    fixture: std::sync::Mutex<Fixture>,
    snapshots: std::sync::atomic::AtomicUsize,
    second_snapshot_changes: Vec<(u64, Vec<u8>)>,
}
impl ReadMemory for UiWaitFixture {
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        self.fixture.lock().unwrap().read(a, n)
    }
}
impl Process for UiWaitFixture {
    fn image_base(&self) -> u64 {
        0x10000
    }
    fn check(&self) -> Result<()> {
        self.fixture.lock().unwrap().check()
    }
    fn begin_snapshot(&self) -> Result<()> {
        let n = self
            .snapshots
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut f = self.fixture.lock().unwrap();
        if n == 1 {
            for (address, bytes) in &self.second_snapshot_changes {
                f.put(*address, bytes);
            }
        }
        f.check()
    }
}
fn ui_wait_reply(
    f: Fixture,
    changes: Vec<(u64, Vec<u8>)>,
    target: serde_json::Value,
    timeout: u64,
) -> serde_json::Value {
    let server = hoi4_live_native::protocol::Server {
        process: Some(Box::new(UiWaitFixture {
            fixture: std::sync::Mutex::new(f),
            snapshots: std::sync::atomic::AtomicUsize::new(0),
            second_snapshot_changes: changes,
        })),
        unavailable: None,
    };
    let reply = server.handle(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_wait_for","arguments":{"timeout_seconds":timeout,"until_research_ui":target}}})).unwrap();
    serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}

#[test]
fn ui_wait_observes_a_delayed_view_transition_without_input_or_process_pause() {
    let mut f = Fixture::new();
    f.research_wait();
    let changes = vec![
        (0x903d8, 6i32.to_le_bytes().to_vec()),
        (0xa00a5, vec![0]),
        (0x922f8, 0xa2000u64.to_le_bytes().to_vec()),
    ];
    let v = ui_wait_reply(
        f,
        changes,
        serde_json::json!({"view":"tree","selected_slot":1}),
        10,
    );
    assert_eq!(v["reasons"][0], "research_ui_reached");
    assert_eq!(v["state"]["selected_slot"], 1);
    assert_eq!(v["state"]["ordinary_tree"]["visible_predicate"], true);
    assert_eq!(v["state"]["input_authorized"], false);
    assert_eq!(v["state"]["observation_paused_game"], false);
}

#[test]
fn ui_wait_refuses_changed_slot_identity_even_when_every_own_join_is_valid() {
    let mut f = Fixture::new();
    f.research_wait();
    let mut slots = Vec::new();
    for slot in [0x2c100u64, 0x2c000, 0x2c200, 0x2c300] {
        slots.extend(slot.to_le_bytes());
    }
    let changes = vec![
        (0x3b000, slots),
        (0x942b8, 0x2c100u64.to_le_bytes().to_vec()),
        (0x952b8, 0x2c000u64.to_le_bytes().to_vec()),
    ];
    let v = ui_wait_reply(f, changes, serde_json::json!({"view":"tree"}), 10);
    assert_eq!(v["error"]["code"], "wait_context_changed");
    assert!(v.get("state").is_none());
}

#[test]
fn ui_wait_distinguishes_reached_predicates_unavailable_slots_and_wall_timeout() {
    let mut f = Fixture::new();
    f.research_wait();
    let v = ui_wait_reply(
        f,
        Vec::new(),
        serde_json::json!({"view":"panel","selected_slot":1}),
        10,
    );
    assert_eq!(v["reasons"][0], "research_ui_reached");
    let mut f = Fixture::new();
    f.research_wait();
    let v = ui_wait_reply(
        f,
        Vec::new(),
        serde_json::json!({"view":"panel","selected_slot":4}),
        10,
    );
    assert_eq!(v["error"]["code"], "invalid_arguments");
    let mut f = Fixture::new();
    f.research_wait();
    let v = ui_wait_reply(f, Vec::new(), serde_json::json!({"view":"tree"}), 1);
    assert_eq!(v["reasons"][0], "wall_timeout");
    assert_eq!(v["state"]["panel"]["visible_predicate"], true);
    assert!(v["state_age_ms"].as_u64().is_some());
}

#[test]
fn ui_wait_requires_positive_own_tag_before_any_country_payload_read() {
    let mut f = Fixture::new();
    f.research_wait();
    f.number(0x30518, 0);
    for address in 0x39000..0x3900c {
        f.data.remove(&address);
    }
    let v = ui_wait_reply(f, Vec::new(), serde_json::json!({"view":"tree"}), 10);
    assert_eq!(v["error"]["code"], "player_unavailable");
    assert!(v.get("state").is_none());
}

#[test]
fn current_node_tool_uses_canonical_public_keys_and_actual_snapshot_boundary() {
    let mut f = Fixture::new();
    f.current_research_nodes();
    let v = research_ui::nodes(&f).unwrap();
    assert_eq!(v["current_nodes"]["record_count"], 2);
    assert_eq!(v["current_nodes"]["node_count"], 1);
    assert_eq!(v["current_nodes"]["nodes"][0]["id"], "tools1");
    assert_eq!(v["current_nodes"]["nodes"][0]["definition_index"], 1);
    assert_eq!(v["selected_slot"], 1);
    assert_eq!(v["input_authorized"], false);
    assert_eq!(f.checks.get(), 2);
    for (address, size) in [
        (0xd2000, 80),
        (0xd3030, 8),
        (0xd30c8, 8),
        (0x63030, 4),
        (0x63008, 32),
    ] {
        let mut f = Fixture::new();
        f.current_research_nodes();
        let mut replacement: Vec<_> = (0..size).map(|i| f.data[&(address + i as u64)]).collect();
        replacement[0] ^= 1;
        f.fault = Some((address, 1, replacement));
        assert_eq!(research_ui::nodes(&f).unwrap_err().code, "torn_read");
    }
}

#[test]
fn current_node_tool_requires_open_own_tree_before_node_payload() {
    for mode in [5, 23] {
        let mut f = Fixture::new();
        f.current_research_nodes();
        f.number(0x903d8, mode);
        for address in 0x92dd8..0x92de8 {
            f.data.remove(&address);
        }
        assert_eq!(research_ui::nodes(&f).unwrap_err().code, "gui_unavailable");
    }
    let mut f = Fixture::new();
    f.current_research_nodes();
    f.word(0x80158, 0xdead0000);
    for address in 0x80160..0x80168 {
        f.data.remove(&address);
    }
    assert_eq!(
        research_ui::nodes(&f).unwrap_err().code,
        "invalid_research_owner"
    );
}

#[test]
fn current_node_pages_bound_output_without_skipping_full_validation() {
    let mut f = Fixture::new();
    f.current_research_nodes();
    let page = research_ui::nodes_page(&f, 0, 1, Some("tool")).unwrap();
    assert_eq!(page["current_nodes"]["node_count"], 1);
    assert_eq!(page["current_nodes"]["filtered_count"], 1);
    assert_eq!(page["current_nodes"]["nodes"][0]["id"], "tools1");
    let empty = research_ui::nodes_page(&f, 1, 1, None).unwrap();
    assert_eq!(empty["current_nodes"]["nodes"].as_array().unwrap().len(), 0);
    assert!(empty["current_nodes"]["next_offset"].is_null());
    assert_eq!(empty["current_nodes"]["node_count"], 1);
    f.word(0xd3030, 0x92008);
    assert_eq!(
        research_ui::nodes_page(&f, 1, 1, Some("absent"))
            .unwrap_err()
            .code,
        "invalid_gui_owner"
    );
}
#[test]
fn invalid_current_node_page_never_enters_process() {
    let f = Fixture::new();
    for (offset, limit, prefix) in [
        (513, 1, None),
        (0, 0, None),
        (0, 101, None),
        (0, 1, Some("bad;key")),
    ] {
        assert_eq!(
            research_ui::nodes_page(&f, offset, limit, prefix)
                .unwrap_err()
                .code,
            "invalid_arguments"
        );
    }
    assert_eq!(f.checks.get(), 0);
}

impl Fixture {
    fn node_binding(&mut self) {
        self.current_research_nodes();
        let base = 0x10000;
        self.word(0xd3010, 0xd4000);
        self.region(0xd4000, 0x980);
        self.word(0xd4000, base + 0x4347d58);
        self.word(0xd4098, 0x70000);
        self.word(base + 0x4347d58 - 16, 0);
        self.word(base + 0x4347d58 - 8, base + 0x4355100);
        self.region(base + 0x4355100, 16);
        self.word(base + 0x4355108, base + 0x350d100);
        self.region(base + 0x350d100, 128);
        self.put(base + 0x350d100, b"16CContainerWindow");
        self.array(0xd4958, 0xd4800, 1);
        self.word(0xd4800, 0xd5000);
        self.region(0xd5000, 0x120);
        self.word(0xd5000, base + 0x4346b10);
        self.word(base + 0x4346b10 - 16, 0);
        self.word(base + 0x4346b10 - 8, base + 0x4355000);
        self.region(base + 0x4355000, 16);
        self.word(base + 0x4355008, base + 0x350d000);
        self.region(base + 0x350d000, 128);
        self.put(base + 0x350d000, b"11CButtonDrag");
        self.word(0xd5008, 0x80000);
        self.word(0xd5068, 0x70000);
        self.word(0xd5080, base + 0x4346ea8);
        self.word(0xd5088, 0xd6000);
        self.word(0xd5090, 0xd6000);
        self.number(0xd5098, 1);
        self.region(0xd6000, 32);
        self.word(0xd6000, 0x925e0);
        self.word(0x925e0, base + 0x424e0a8);
        self.word(0x92608, 0xd7000);
        self.word(0x92618, base + 0x15a5710);
        self.word(0x92620, base + 0x15a56e0);
        self.region(0xd7000, 24);
        self.word(0xd7000, base + 0x1599290);
        self.word(0xd7010, 0x92000);
    }
}
#[test]
fn node_control_binding_is_conditional_and_pointer_free() {
    let mut f = Fixture::new();
    f.node_binding();
    let v = research_ui::nodes(&f).unwrap();
    let b = &v["current_nodes"]["nodes"][0]["ordinary_control_bindings"][0];
    assert_eq!(b["binding_verified"], true);
    assert_eq!(b["input_authorized"], false);
    assert_eq!(b["coordinates"], "unavailable");
    assert!(!v.to_string().contains(&0xd5000u64.to_string()));
}
#[test]
fn node_wrong_control_data_refuses_before_callback() {
    let mut f = Fixture::new();
    f.node_binding();
    f.word(0xd5008, 0x80008);
    f.data.remove(&0xd5080);
    assert_eq!(
        research_ui::nodes(&f).unwrap_err().code,
        "invalid_research_join"
    );
}
#[test]
fn node_override_and_wrong_callback_never_claim_binding() {
    for (a, v) in [
        (0xd50a0, 0xdead0000),
        (0x92620, 0x10000 + 0x1599290),
        (0xd7010, 0x92008),
    ] {
        let mut f = Fixture::new();
        f.node_binding();
        f.word(a, v);
        let v = research_ui::nodes(&f).unwrap();
        assert_eq!(
            v["current_nodes"]["nodes"][0]["ordinary_control_bindings"][0]["binding_verified"],
            false
        );
    }
}
#[test]
fn node_null_and_unknown_layout_are_diagnostic_only() {
    let mut f = Fixture::new();
    f.node_binding();
    f.word(0xd3010, 0);
    let v = research_ui::nodes(&f).unwrap();
    assert_eq!(
        v["current_nodes"]["nodes"][0]["ordinary_control_bindings"][0]["reason"],
        "gui_unmaterialized"
    );
    let mut f = Fixture::new();
    f.node_binding();
    f.word(0xd4000, 0x10000 + 0x4346b10);
    for a in 0xd4958..0xd4968 {
        f.data.remove(&a);
    }
    let v = research_ui::nodes(&f).unwrap();
    assert_eq!(
        v["current_nodes"]["nodes"][0]["ordinary_control_bindings"][0]["binding_verified"],
        false
    );
}
#[test]
fn node_binding_fields_use_production_final_witnesses() {
    for (a, n) in [
        (0xd4098, 8),
        (0xd5068, 8),
        (0xd5008, 8),
        (0xd5088, 8),
        (0xd5098, 4),
        (0xd6010, 8),
        (0x92620, 8),
        (0xd7010, 8),
    ] {
        let mut f = Fixture::new();
        f.node_binding();
        let mut replacement: Vec<_> = (0..n).map(|i| f.data[&(a + i)]).collect();
        replacement[0] ^= 1;
        f.fault = Some((a, 1, replacement));
        assert_eq!(research_ui::nodes(&f).unwrap_err().code, "torn_read");
    }
}
#[test]
fn node_malformed_listener_is_refused() {
    let mut f = Fixture::new();
    f.node_binding();
    f.word(0xd6010, 0xd6000);
    assert_eq!(
        research_ui::nodes(&f).unwrap_err().code,
        "invalid_gui_binding"
    );
}
#[test]
fn repeated_key_controls_retain_separate_binding_observations() {
    let mut f = Fixture::new();
    f.node_binding();
    f.region(0xd8000, 0xd0);
    f.word(0xd8000, 0x10000 + 0x424e2e0);
    f.word(0xd8030, 0x92000);
    f.word(0xd80c8, 0x80000);
    f.word(0xd8010, 0xd4000);
    f.word(0xd2038, 0xd8000);
    let v = research_ui::nodes(&f).unwrap();
    assert_eq!(v["current_nodes"]["node_count"], 1);
    assert_eq!(
        v["current_nodes"]["nodes"][0]["ordinary_control_bindings"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn binding_never_relaxes_key_or_node_owner() {
    let mut f = Fixture::new();
    f.node_binding();
    f.string(0x63008, "bad\nkey");
    assert_eq!(
        research_ui::nodes(&f).unwrap_err().code,
        "invalid_research_id"
    );
    let mut f = Fixture::new();
    f.node_binding();
    f.word(0xd3030, 0x92008);
    assert_eq!(
        research_ui::nodes(&f).unwrap_err().code,
        "invalid_gui_owner"
    );
}

#[test]
fn corrected_record_offset_and_stage_diagnostics_do_not_relax_types() {
    let mut f = Fixture::new();
    f.node_binding();
    // +32 decoys are deliberately unreadable objects; +16 is actual item.
    assert_eq!(
        research_ui::nodes(&f).unwrap()["current_nodes"]["node_count"],
        1
    );
    f.word(0xd3000, 0x10000 + 0x4347d58);
    let error = research_ui::nodes(&f).unwrap_err();
    assert_eq!(error.code, "node_item_type_unavailable");
    assert!(error.message.contains("CContainerWindow"));
    assert!(!error.message.contains("0xd3000"));
    let mut f = Fixture::new();
    f.node_binding();
    f.word(0x80000, 0x10000 + 0x4347d58);
    // Existing Info observation may refuse first; isolate current node helper
    // by an empty Info list, preserving production owner/snapshot guards.
    f.array(0x92dc0, 0, 0);
    assert_eq!(
        research_ui::nodes(&f).unwrap_err().code,
        "node_technology_type_unavailable"
    );
}

#[test]
fn node_foreign_registry_refused_before_unmapped_control_array() {
    let mut f = Fixture::new();
    f.node_binding();
    f.word(0xd4098, 0xdead0000);
    for address in 0xd4958..0xd4968 {
        f.data.remove(&address);
    }
    assert_eq!(
        research_ui::nodes(&f).unwrap_err().code,
        "invalid_gui_owner"
    );
}

#[test]
fn node_foreign_control_registry_refuses_before_data_render_or_callback() {
    let mut f = Fixture::new();
    f.node_binding();
    f.word(0xd5068, 0xdead0000);
    for (start, length) in [(0xd5008, 8), (0xd5080, 8), (0xd5110, 8)] {
        for address in start..start + length {
            f.data.remove(&address);
        }
    }
    assert_eq!(
        research_ui::nodes(&f).unwrap_err().code,
        "invalid_gui_owner"
    );
}

#[test]
fn request_reuse_backend_read_measurement() {
    let mut f = Fixture::new();
    f.node_binding();
    f.region(0xd8000, 0xd0);
    f.word(0xd8000, 0x10000 + 0x424e2e0);
    f.word(0xd8030, 0x92000);
    f.word(0xd80c8, 0x80000);
    f.word(0xd8010, 0xd4000);
    f.word(0xd2038, 0xd8000);
    let value = research_ui::nodes(&f).unwrap();
    assert_eq!(value["current_nodes"]["node_count"], 1);
    let reads = f.tracked_reads.borrow();
    println!("metadata_table_reads={} callback_manager_reads={} object_control_reads={} rpc_read_calls={}",reads[&(0x10000+0x4346b10-16)],reads[&0x92618],reads[&0xd5000],value["freshness"]["read_calls"]);
    // Measured unmodified baseline on this fixture: 3/3/5/404.
    assert_eq!(reads[&(0x10000 + 0x4346b10 - 16)], 2);
    assert_eq!(reads[&0x92618], 2);
    assert_eq!(reads[&0xd5000], 5);
    assert!(value["freshness"]["read_calls"].as_u64().unwrap() < 404);
}

impl Fixture {
    fn reused_two_controls(&mut self) {
        self.node_binding();
        for (from, to, size) in [
            (0xd4000, 0xd9000, 0x980),
            (0xd5000, 0xda000, 0x120),
            (0xd6000, 0xdb000, 32),
        ] {
            let bytes: Vec<_> = (0..size).map(|i| self.data[&(from + i)]).collect();
            self.put(to, &bytes);
        }
        self.array(0xd9958, 0xd9800, 1);
        self.word(0xd9800, 0xda000);
        self.word(0xda088, 0xdb000);
        self.word(0xda090, 0xdb000);
        self.region(0xd8000, 0xd0);
        self.word(0xd8000, 0x10000 + 0x424e2e0);
        self.word(0xd8030, 0x92000);
        self.word(0xd80c8, 0x80000);
        self.word(0xd8010, 0xd9000);
        self.word(0xd2038, 0xd8000);
    }
}
#[test]
fn reused_metadata_and_capture_keep_final_production_witnesses() {
    for (a, n) in [
        (0x10000 + 0x4346b10 - 16, 8),
        (0x10000 + 0x4355008, 8),
        (0x10000 + 0x350d000, 128),
        (0x92618, 8),
        (0x92620, 8),
        (0x92608, 8),
        (0xd7000, 8),
        (0xd7008, 8),
        (0xd7010, 8),
    ] {
        let mut f = Fixture::new();
        f.reused_two_controls();
        let mut replacement: Vec<_> = (0..n).map(|i| f.data[&(a + i)]).collect();
        replacement[0] ^= 1;
        f.fault = Some((a, 1, replacement));
        assert_eq!(
            research_ui::nodes(&f).unwrap_err().code,
            "torn_read",
            "field {a:x}"
        );
    }
}
#[test]
fn reused_proof_never_reuses_later_object_owner_or_control_data() {
    for (a, v, code) in [
        (0xd8030, 0x92008, "invalid_gui_owner"),
        (0xd9098, 0xdead0000, "invalid_gui_owner"),
        (0xda068, 0xdead0000, "invalid_gui_owner"),
        (0xda008, 0x80008, "invalid_research_join"),
        (0xd8000, 0x10000 + 0x4347d58, "node_item_type_unavailable"),
    ] {
        let mut f = Fixture::new();
        f.reused_two_controls();
        f.word(a, v);
        assert_eq!(research_ui::nodes(&f).unwrap_err().code, code);
    }
    let mut f = Fixture::new();
    f.reused_two_controls();
    f.word(0xda000, 0x10000 + 0x4347d58);
    let v = research_ui::nodes(&f).unwrap();
    assert_eq!(
        v["current_nodes"]["nodes"][0]["ordinary_control_bindings"][1]["binding_verified"],
        false
    );
}
#[test]
fn request_local_reuse_does_not_survive_requests() {
    let mut f = Fixture::new();
    f.reused_two_controls();
    research_ui::nodes(&f).unwrap();
    f.word(0xd7010, 0x92008);
    let v = research_ui::nodes(&f).unwrap();
    for b in v["current_nodes"]["nodes"][0]["ordinary_control_bindings"]
        .as_array()
        .unwrap()
    {
        assert_eq!(b["binding_verified"], false);
    }
}
