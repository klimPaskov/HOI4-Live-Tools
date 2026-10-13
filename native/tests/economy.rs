use hoi4_live_native::{
    error::{Error, Result},
    memory::ReadMemory,
    platform::Process,
};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
};
struct Fixture {
    tracked_reads: RefCell<BTreeMap<u64, usize>>,
    data: BTreeMap<u64, u8>,
    checks: Cell<u32>,
    replaced: bool,
    id_reads: Cell<u32>,
    changed_id: bool,
    fault: Option<(u64, usize, Vec<u8>)>,
    fault_reads: Cell<usize>,
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
            tracked_reads: RefCell::new(BTreeMap::new()),
            data: BTreeMap::new(),
            checks: Cell::new(0),
            replaced: false,
            id_reads: Cell::new(0),
            changed_id: false,
            fault: None,
            fault_reads: Cell::new(0),
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
        *self.tracked_reads.borrow_mut().entry(a).or_default() += 1;
        if let Some((address, after, replacement)) = &self.fault {
            if a == *address {
                self.fault_reads.set(self.fault_reads.get() + 1);
                if self.fault_reads.get() > *after {
                    assert_eq!(replacement.len(), n);
                    return Ok(replacement.clone());
                }
            }
        }
        if a == 0x60000 {
            self.id_reads.set(self.id_reads.get() + 1);
            if self.changed_id && self.id_reads.get() > 1 {
                return Ok(b"aabbccdd-1122-4344-8566-778899aabbcd".to_vec());
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
impl Fixture {
    fn economy(&mut self) {
        self.object(0x20000, "CCountry", 0x10000 + 0x41fafb0);
        self.number(0x20008, 1);
        self.region(0x40000, 0x400);
        self.region(0x41000, 0x100);
        self.word(0x20f40, 0x40000);
        self.word(0x20f68, 0x41000);
        self.word(0x40000, 0x10000 + 0x42066f8);
        self.word(0x40290, 0x20000);
        self.word(0x41000, 0x10000 + 0x4202e30);
        self.word(0x410c8, 0x20000);
        for (off, raw) in [
            (0x2b8, 1_099_999i64),
            (0x318, 599_999),
            (0x378, 2_099_999),
            (0x390, 350_000),
        ] {
            self.put(0x40000 + off, &raw.to_le_bytes());
        }
        for (off, n) in [(0x2f8, 1), (0x300, 2), (0x3b8, 3), (0x3c0, 4), (0x3b0, 5)] {
            self.number(0x40000 + off, n);
        }
        self.put(0x410e0, &(-150_001i64).to_le_bytes());
    }
}
#[test]
fn named_own_economy_formulas_and_negative_pp_are_exact() {
    let mut f = Fixture::new();
    f.economy();
    let v = hoi4_live_native::economy::snapshot(&f).unwrap();
    assert_eq!(v["factory_trigger_count"]["military"], 7);
    assert_eq!(v["factory_trigger_count"]["naval"], 5);
    assert_eq!(v["factory_trigger_count"]["civilian"], 13);
    assert_eq!(v["civilian_factories_available_for_projects"], 12);
    assert_eq!(v["political_power"]["raw_signed"], -150001);
    assert_eq!(v["political_power"]["integer_trigger_count"], -1);
    assert_eq!(
        v["political_power"]["exact_rational"]["denominator"],
        100000
    );
    assert_eq!(v["input_authorized"], false);
    assert_eq!(v["source"], "native own economy trigger counts");
    assert_eq!(v["build_id"], hoi4_live_native::game::BUILD_ID);
    assert_eq!(v["freshness"]["read_calls_kind"], "logical_read_requests");
    for k in ["observed_unix_ms", "elapsed_ms", "read_calls", "read_bytes"] {
        assert!(v["freshness"][k].is_number());
    }
    assert_eq!(v["unavailable"]["ui_factory_totals"], true);
}
#[test]
fn own_tag_primary_and_owner_guards_precede_payload() {
    for (address, n, code, blocked) in [
        (0x30518, 0, "player_unavailable", 0x39004),
        (
            0x20000,
            0x10000 + 0x41fafb8,
            "invalid_economy_type",
            0x20f40,
        ),
        (0x20008, 2, "invalid_economy_owner", 0x20f40),
        (
            0x40000,
            0x10000 + 0x4206780,
            "invalid_economy_type",
            0x40290,
        ),
        (0x40290, 0x20008, "invalid_economy_owner", 0x402b8),
        (
            0x41000,
            0x10000 + 0x4202e38,
            "invalid_economy_type",
            0x410c8,
        ),
        (0x410c8, 0x20008, "invalid_economy_owner", 0x410e0),
    ] {
        let mut f = Fixture::new();
        f.economy();
        if [0x30518, 0x20008].contains(&address) {
            f.number(address, n as i32);
        } else {
            f.word(address, n);
        }
        assert_eq!(
            hoi4_live_native::economy::snapshot(&f).unwrap_err().code,
            code
        );
        assert!(!f.tracked_reads.borrow().contains_key(&blocked));
    }
}
#[test]
fn economy_all_joins_and_values_are_final_witnesses() {
    for (a, n) in [
        (0x20f40, 8),
        (0x20f68, 8),
        (0x20008, 4),
        (0x40290, 8),
        (0x410c8, 8),
        (0x402b8, 8),
        (0x40318, 8),
        (0x40378, 8),
        (0x402f8, 4),
        (0x40300, 4),
        (0x403b8, 4),
        (0x403c0, 4),
        (0x40390, 8),
        (0x403b0, 4),
        (0x410e0, 8),
    ] {
        let mut f = Fixture::new();
        f.economy();
        let mut b = (0..n).map(|i| f.data[&(a + i)]).collect::<Vec<_>>();
        b[0] ^= 1;
        f.fault = Some((a, 1, b));
        assert_eq!(
            hoi4_live_native::economy::snapshot(&f).unwrap_err().code,
            "torn_read"
        );
    }
}
#[test]
fn economy_policy_and_negative_factory_results_refuse() {
    for (a, n, width) in [
        (0x402b8, -1i64, 8),
        (0x40390, 100_000_000_001, 8),
        (0x410e0, i64::MIN, 8),
        (0x402f8, -1, 4),
        (0x403c0, 1_000_001, 4),
        (0x402f8, 11, 4),
    ] {
        let mut f = Fixture::new();
        f.economy();
        if width == 8 {
            f.put(a, &n.to_le_bytes());
        } else {
            f.number(a, n as i32);
        }
        assert_eq!(
            hoi4_live_native::economy::snapshot(&f).unwrap_err().code,
            "invalid_economy_value"
        );
    }
}
#[test]
fn project_only_clamps_and_fixed_boundaries_remain_checked() {
    let mut f = Fixture::new();
    f.economy();
    f.put(0x40390, &3_000_000i64.to_le_bytes());
    assert_eq!(
        hoi4_live_native::economy::snapshot(&f).unwrap()
            ["civilian_factories_available_for_projects"],
        0
    );
    for n in [
        -100_000_000_000i64,
        -100001,
        -99999,
        0,
        99999,
        100001,
        100_000_000_000,
    ] {
        let mut f = Fixture::new();
        f.economy();
        f.put(0x410e0, &n.to_le_bytes());
        assert_eq!(
            hoi4_live_native::economy::snapshot(&f).unwrap()["political_power"]
                ["integer_trigger_count"],
            n / 100000
        );
    }
}
#[test]
fn economy_process_and_context_failures_still_refuse() {
    let mut f = Fixture::new();
    f.economy();
    f.replaced = true;
    assert_eq!(
        hoi4_live_native::economy::snapshot(&f).unwrap_err().code,
        "ownership_changed"
    );
    let mut f = Fixture::new();
    f.economy();
    f.changed_id = true;
    assert_eq!(
        hoi4_live_native::economy::snapshot(&f).unwrap_err().code,
        "torn_read"
    );
}
#[test]
fn economy_protocol_rejects_unknown_arguments_before_binding() {
    let server = hoi4_live_native::protocol::Server {
        process: None,
        unavailable: None,
    };
    let v=server.handle(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_economy","arguments":{"country":"OTHER"}}})).unwrap();
    assert!(v.to_string().contains("invalid_arguments"));
    let caps=server.handle(serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"hoi4_capabilities","arguments":{}}})).unwrap();
    assert!(caps.to_string().contains("own_economy_trigger_counts"));
}

#[test]
fn economy_country_map_policy_refuses_before_entry_payload() {
    let mut f = Fixture::new();
    f.economy();
    f.number(0x30348, 4097);
    assert_eq!(
        hoi4_live_native::economy::snapshot(&f).unwrap_err().code,
        "invalid_array"
    );
    assert!(!f.tracked_reads.borrow().contains_key(&0x39004));
    let mut f = Fixture::new();
    f.economy();
    f.number(0x39004, -1);
    assert_eq!(
        hoi4_live_native::economy::snapshot(&f).unwrap_err().code,
        "player_unavailable"
    );
    assert!(!f.tracked_reads.borrow().contains_key(&0x3a010));
}
