//! Generated memory only. Both string ABIs run on every test host; no OS API.
use super::*;
use crate::memory::{ReadMemory, StringAbi};

pub(crate) const FIRST: &str = "GER_first";
pub(crate) const SECOND: &str = "GER_second";
pub(crate) const DONE: &str = "GER_done";
pub(crate) const BASE: u64 = 0x1_0000_0000;
pub(crate) const GS: u64 = 0x100000;
pub(crate) const IDLER: u64 = 0x110000;
pub(crate) const COUNTRY: u64 = 0x120000;
pub(crate) const STATUS: u64 = 0x130000;
pub(crate) const TREE: u64 = 0x140000;
pub(crate) const PROGRESS: u64 = 0x150000;
pub(crate) const DATABASE: u64 = 0x160000;
pub(crate) const TECHNOLOGY_DATABASE: u64 = 0x170000;
pub(crate) const FOCUS: u64 = 0x800000;
pub(crate) const SLOT: u64 = 0x190000;
pub(crate) const TAG_DATABASE: u64 = 0x1a0000;
pub(crate) const FOREIGN: u64 = 0xdead0000;
pub(crate) const DATE: i32 = 60759624;

pub(crate) struct Fixture {
    pub(crate) abi: StringAbi,
    pub(crate) id: Identity,
    expected: ClockContext,
    data: BTreeMap<u64, u8>,
    next: u64,
    next_focus: u64,
    focuses: Vec<(String, u64)>,
    reads: RefCell<Vec<(u64, usize)>>,
    counts: RefCell<BTreeMap<(u64, usize), usize>>,
    pub(crate) snapshots: Cell<usize>,
    pub(crate) checks: Cell<usize>,
    fault: Option<(u64, usize, usize)>,
    pub(crate) fail_final: bool,
    pub(crate) swap_final_identity: bool,
    final_identity: Identity,
}
impl Fixture {
    pub(crate) fn new(abi: StringAbi) -> Self {
        let id = Identity {
            pid: 42,
            start_ticks: match abi {
                StringAbi::LibCxx => "1792677600.000123",
                StringAbi::LibStdCxx => "123456789",
            }
            .into(),
            exe: "/generated/hoi4".into(),
            uid: 501,
        };
        let mut final_identity = id.clone();
        final_identity.start_ticks.push('9');
        let expected = ClockContext {
            playthrough_id: "aabbccdd-1122-4344-8566-778899aabbcc".into(),
            player: "GER".into(),
            anchors: [GS, IDLER, TECHNOLOGY_DATABASE],
            date_raw: DATE,
            difficulty: 2,
            historical_raw: 1,
            paused: true,
            speed: 0,
        };
        let mut f = Self {
            abi,
            id,
            expected,
            data: BTreeMap::new(),
            next: 0x400000,
            next_focus: FOCUS,
            focuses: Vec::new(),
            reads: RefCell::new(Vec::new()),
            counts: RefCell::new(BTreeMap::new()),
            snapshots: Cell::new(0),
            checks: Cell::new(0),
            fault: None,
            fail_final: false,
            swap_final_identity: false,
            final_identity,
        };
        let g = f.game_layout();
        let l = f.focus_layout();
        let technology = research::layout(&Reader::new(&f));
        for (address, value) in [
            (BASE + g.gs_global, GS),
            (BASE + g.idler_global, IDLER),
            (BASE + g.technology_db_global, TECHNOLOGY_DATABASE),
            (BASE + g.tag_db_global, TAG_DATABASE),
            (BASE + l.database_global, DATABASE),
            (GS, BASE + g.gs_vtable),
            (IDLER, BASE + g.idler_vtable),
            (COUNTRY, BASE + g.country_vtable),
            (COUNTRY + g.country_technology_status, STATUS),
            (COUNTRY + g.country_focus_tree, TREE),
            (COUNTRY + g.country_focus_progress, PROGRESS),
            (STATUS, BASE + technology.status_vtable),
            (STATUS + 0x130, COUNTRY),
            (TREE, BASE + l.tree_vtable),
            (PROGRESS, BASE + l.progress_vtable),
            (PROGRESS + 8, COUNTRY),
            (PROGRESS + 0x10, 0),
            (DATABASE, BASE + l.database_vtable),
            (TECHNOLOGY_DATABASE, BASE + technology.database_vtable),
        ] {
            f.word(address, value);
        }
        f.int(COUNTRY + 8, 1);
        f.int(STATUS + 0x138, 1);
        f.int(GS + g.player_tag, 1);
        f.int(GS + g.date, DATE);
        let (playthrough, speed, paused, difficulty, historical, slot_table) = match abi {
            StringAbi::LibCxx => (0x6c8, 0x47c, 0x679, 0x5d0, 0x5d8, 0x339e488),
            StringAbi::LibStdCxx => (0x718, 0x4b4, 0x6c1, 0x620, 0x628, 0x4210898),
        };
        f.int(GS + speed, 0);
        f.put(IDLER + paused, &[1]);
        f.int(GS + difficulty, 2);
        f.int(GS + historical, 1);
        f.string(GS + playthrough, "aabbccdd-1122-4344-8566-778899aabbcc");
        let index = f.alloc(8);
        f.word(GS + g.tag_country_index, index);
        f.int(GS + g.tag_country_index + 8, 2);
        f.int(GS + g.tag_country_index + 12, 2);
        f.int(index, -1);
        f.int(index + 4, 0);
        f.array(GS + g.countries, &[COUNTRY]);
        let tags = f.alloc(g.tag_stride as usize * 2);
        f.word(TAG_DATABASE, tags);
        f.int(TAG_DATABASE + 8, 2);
        f.int(TAG_DATABASE + 12, 2);
        f.string(tags, "---");
        f.string(tags + g.tag_stride, "GER");
        f.string(COUNTRY + 0x10, "German Reich");
        f.array(STATUS + 0xa0, &[SLOT]);
        f.word(SLOT, BASE + slot_table);
        f.word(SLOT + 0x10, STATUS);
        f.word(SLOT + 0x18, 0);
        f.array(PROGRESS + 0x40, &[]);
        for (i, (table, name)) in [
            (g.country_vtable, "CCountry"),
            (l.progress_vtable, "CNationalFocusProgress"),
            (l.focus_vtable, "CNationalFocus"),
            (technology.status_vtable, "CTechnologyStatus"),
            (slot_table, "CResearchSlot"),
        ]
        .into_iter()
        .enumerate()
        {
            f.rtti(table, name, i as u64);
        }
        for key in [FIRST, SECOND, DONE] {
            f.add_focus(key);
        }
        f.set_completed(&[DONE]);
        f
    }
    fn alloc(&mut self, size: usize) -> u64 {
        let address = self.next;
        self.next += (size.max(1) as u64).div_ceil(0x100) * 0x100;
        address
    }
    pub(crate) fn put(&mut self, address: u64, bytes: &[u8]) {
        self.data.extend(
            bytes
                .iter()
                .enumerate()
                .map(|(i, b)| (address + i as u64, *b)),
        );
    }
    pub(crate) fn word(&mut self, address: u64, value: u64) {
        self.put(address, &value.to_le_bytes());
    }
    pub(crate) fn int(&mut self, address: u64, value: i32) {
        self.put(address, &value.to_le_bytes());
    }
    pub(crate) fn get_word(&self, address: u64) -> u64 {
        u64::from_le_bytes(std::array::from_fn(|i| self.data[&(address + i as u64)]))
    }
    pub(crate) fn string(&mut self, address: u64, value: &str) {
        match self.abi {
            StringAbi::LibCxx if value.len() <= 22 => {
                let mut header = [0; 24];
                header[0] = (value.len() * 2) as u8;
                header[1..1 + value.len()].copy_from_slice(value.as_bytes());
                self.put(address, &header);
            }
            StringAbi::LibCxx => {
                let data = self.alloc(value.len());
                self.word(address, ((value.len() as u64 + 1) & !1) | 1);
                self.word(address + 8, value.len() as u64);
                self.word(address + 16, data);
                self.put(data, value.as_bytes());
            }
            StringAbi::LibStdCxx => {
                self.put(address, &[0; 32]);
                self.word(address + 8, value.len() as u64);
                if value.len() <= 15 {
                    self.word(address, address + 16);
                    self.put(address + 16, value.as_bytes());
                } else {
                    let data = self.alloc(value.len());
                    self.word(address, data);
                    self.put(data, value.as_bytes());
                }
            }
        }
    }
    pub(crate) fn array(&mut self, address: u64, values: &[u64]) {
        let data = self.alloc(values.len() * 8);
        self.word(address, data);
        self.int(address + 8, values.len() as i32);
        self.int(address + 12, values.len() as i32);
        for (i, value) in values.iter().enumerate() {
            self.word(data + i as u64 * 8, *value);
        }
    }
    fn rtti(&mut self, table: u64, name: &str, index: u64) {
        let info = 0x300000 + index * 0x100;
        self.word(BASE + table - 8, info);
        self.word(info + 8, info + 0x40);
        self.put(info + 0x40, format!("{}{name}\0", name.len()).as_bytes());
    }
    pub(crate) fn game_layout(&self) -> &'static game::Layout {
        game::layout(&Reader::new(self))
    }
    pub(crate) fn focus_layout(&self) -> &'static super::super::super::Layout {
        layout(&Reader::new(self))
    }
    pub(crate) fn game(&self) -> Identity {
        self.id.clone()
    }
    pub(crate) fn context(&self) -> ClockContext {
        self.expected.clone()
    }
    pub(crate) fn own(&self) -> [u64; 2] {
        [COUNTRY, STATUS]
    }
    pub(crate) fn observe(&self, key: &str) -> Result<AssignmentSnapshot> {
        snapshot(self, key, &self.id, &self.expected, self.own())
    }
    pub(crate) fn focus(&self, key: &str) -> u64 {
        self.focuses.iter().find(|(name, _)| name == key).unwrap().1
    }
    pub(crate) fn add_focus(&mut self, key: &str) -> u64 {
        let pointer = self.next_focus;
        self.next_focus += 0x1000;
        let l = self.focus_layout();
        self.word(pointer, BASE + l.focus_vtable);
        self.word(pointer + l.focus_tree, TREE);
        self.string(pointer + 0x18, key);
        let name = match self.abi {
            StringAbi::LibCxx => 0x50,
            StringAbi::LibStdCxx => 0x60,
        };
        self.string(pointer + name, "Generated Focus");
        self.focuses.push((key.into(), pointer));
        self.catalog();
        pointer
    }
    pub(crate) fn catalog(&mut self) {
        let values: Vec<_> = self.focuses.iter().map(|(_, p)| *p).collect();
        self.array(TREE + self.focus_layout().tree_focuses, &values);
        let mut registry = values.clone();
        registry.extend([0, values[0]]);
        self.array(DATABASE + 0x58, &registry);
    }
    pub(crate) fn set_active(&mut self, key: Option<&str>) {
        self.word(PROGRESS + 0x10, key.map(|key| self.focus(key)).unwrap_or(0));
    }
    pub(crate) fn set_completed(&mut self, keys: &[&str]) {
        let values: Vec<_> = keys.iter().map(|key| self.focus(key)).collect();
        self.array(PROGRESS + 0x40, &values);
    }
    pub(crate) fn set_date(&mut self, date: i32) {
        self.int(GS + self.game_layout().date, date);
        self.expected.date_raw = date;
    }
    pub(crate) fn set_playthrough(&mut self, value: &str) {
        let offset = match self.abi {
            StringAbi::LibCxx => 0x6c8,
            StringAbi::LibStdCxx => 0x718,
        };
        self.string(GS + offset, value);
        self.expected.playthrough_id = value.into();
    }
    pub(crate) fn replace_focus(&mut self, key: &str) -> u64 {
        let old = self.focus(key);
        let index = self.focuses.iter().position(|(_, p)| *p == old).unwrap();
        let pointer = self.add_focus(key);
        self.focuses.pop();
        self.focuses[index].1 = pointer;
        self.catalog();
        if self.get_word(PROGRESS + 0x10) == old {
            self.word(PROGRESS + 0x10, pointer);
        }
        let header = self.get_word(PROGRESS + 0x40);
        let count = i32::from_le_bytes(std::array::from_fn(|i| {
            self.data[&(PROGRESS + 0x4c + i as u64)]
        }));
        for i in 0..count as u64 {
            if self.get_word(header + i * 8) == old {
                self.word(header + i * 8, pointer);
            }
        }
        pointer
    }
    pub(crate) fn tear(&mut self, address: u64, size: usize, nth: usize) {
        self.fault = Some((address, size, nth));
    }
    pub(crate) fn read_counts(&self) -> BTreeMap<(u64, usize), usize> {
        self.counts.borrow().clone()
    }
    pub(crate) fn untouched(&self, address: u64, size: u64) -> bool {
        self.reads
            .borrow()
            .iter()
            .all(|(a, n)| *a >= address + size || *a + *n as u64 <= address)
    }
}
impl ReadMemory for Fixture {
    fn string_abi(&self) -> StringAbi {
        self.abi
    }
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        self.reads.borrow_mut().push((address, size));
        let count = {
            let mut counts = self.counts.borrow_mut();
            let count = counts.entry((address, size)).or_default();
            *count += 1;
            *count
        };
        let mut bytes = (0..size)
            .map(|i| {
                self.data
                    .get(&(address + i as u64))
                    .copied()
                    .ok_or_else(|| {
                        Error::new(
                            "fixture_unmapped",
                            format!(
                                "Synthetic {:?} fixture forbids {address:x}/{size}",
                                self.abi
                            ),
                        )
                    })
            })
            .collect::<Result<Vec<_>>>()?;
        if self.fault == Some((address, size, count)) {
            bytes[0] ^= 1;
        }
        Ok(bytes)
    }
}
impl Process for Fixture {
    fn begin_snapshot(&self) -> Result<()> {
        self.snapshots.set(self.snapshots.get() + 1);
        self.reads.borrow_mut().clear();
        self.counts.borrow_mut().clear();
        self.checks.set(0);
        self.check()
    }
    fn check(&self) -> Result<()> {
        self.checks.set(self.checks.get() + 1);
        if self.fail_final && self.checks.get() > 1 {
            return Err(Error::new(
                "ownership_changed",
                "Synthetic final ownership failure",
            ));
        }
        Ok(())
    }
    fn identity(&self) -> Option<&Identity> {
        Some(if self.swap_final_identity && self.checks.get() > 1 {
            &self.final_identity
        } else {
            &self.id
        })
    }
    fn image_base(&self) -> u64 {
        BASE
    }
}
