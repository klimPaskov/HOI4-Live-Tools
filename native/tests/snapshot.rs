use hoi4_live_native::{
    error::{Error, Result},
    game,
    memory::ReadMemory,
    platform::Process,
};
use std::{cell::Cell, collections::BTreeMap};
struct Fixture {
    data: BTreeMap<u64, u8>,
    clock_reads: Cell<u32>,
    torn: bool,
    checks: Cell<u32>,
    replaced: bool,
    id_reads: Cell<u32>,
    changed_id: bool,
    pause_reads: Cell<u32>,
    invalid_final_pause: bool,
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
            data: BTreeMap::new(),
            clock_reads: Cell::new(0),
            torn: false,
            checks: Cell::new(0),
            replaced: false,
            id_reads: Cell::new(0),
            changed_id: false,
            pause_reads: Cell::new(0),
            invalid_final_pause: false,
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
}
impl ReadMemory for Fixture {
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
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
