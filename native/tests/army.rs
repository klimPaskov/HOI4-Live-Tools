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
    identity: hoi4_live_native::platform::Identity,
    unknown_build: bool,
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
            identity: hoi4_live_native::platform::Identity {
                pid: 42,
                start_ticks: "123".into(),
                exe: "private-owned-fixture".into(),
                uid: 1000,
            },
            unknown_build: false,
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
    fn identity(&self) -> Option<&hoi4_live_native::platform::Identity> {
        Some(&self.identity)
    }
    fn image_base(&self) -> u64 {
        0x10000
    }
    fn check(&self) -> Result<()> {
        if self.unknown_build {
            return Err(Error::new("unknown_build", "Fixture unsupported build"));
        }
        self.checks.set(self.checks.get() + 1);
        if self.replaced && self.checks.get() > 1 {
            Err(Error::new("ownership_changed", "Fixture replacement"))
        } else {
            Ok(())
        }
    }
}

impl Fixture {
    fn army(&mut self) {
        self.object(0x20000, "CCountry", 0x10000 + 0x41fafb0);
        self.number(0x20008, 1);
        self.array(0x20290, 0x70000, 2);
        self.region(0x70000, 16);
        for (i, a) in [0x71000u64, 0x72000].into_iter().enumerate() {
            self.region(a, 0x430);
            self.word(a, 0x10000 + 0x4277fe8);
            self.number(a + 0x1d8, 1);
            self.number(a + 0x1dc, 999);
            self.put(a + 0x18, &u32::MAX.to_le_bytes());
            self.put(a + 0x1c, &(i as u32 + 1).to_le_bytes());
            self.word(0x70000 + i as u64 * 8, a);
            self.put(a + 0x420, &(-123i64).to_le_bytes());
            self.put(a + 0x428, &i64::MAX.to_le_bytes());
        }
        self.word(0x711f0, 0x73000);
        self.region(0x73000, 0xa8);
        self.word(0x73000, 0x10000 + 0x41fb728);
        self.number(0x730a4, 2);
        self.array(0x302b0, 0x74000, 3);
        self.region(0x74000, 24);
        self.word(0x74000, 0xdead0000);
        self.word(0x74008, 0xbeef0000);
        self.word(0x74010, 0x73000);
    }
}
#[test]
fn own_complete_handles_and_selected_location_are_pointer_free() {
    let mut f = Fixture::new();
    f.army();
    let v = hoi4_live_native::army::snapshot(&f).unwrap();
    assert_eq!(v["count"], 2);
    assert_eq!(v["divisions"][0]["id"], "ffffffff:00000001");
    assert_eq!(v["divisions"][1]["id"], "ffffffff:00000002");
    assert_eq!(v["divisions"][0]["location"]["province_id"], 2);
    assert_eq!(v["divisions"][1]["location"]["available"], false);
    assert_eq!(v["divisions"][0]["strength_raw_signed_decimal"], "-123");
    assert_eq!(
        v["divisions"][0]["organisation_raw_signed_decimal"],
        i64::MAX.to_string()
    );
    assert_eq!(v["input_authorized"], false);
    f.put(0x7101c, &u32::MAX.to_le_bytes());
    assert_eq!(
        hoi4_live_native::army::snapshot(&f).unwrap()["divisions"][0]["id"],
        "ffffffff:ffffffff"
    );
    assert_eq!(v["identity_scope"]["pid"], 42);
    for a in [0x74000, 0x74008, 0xdead0000, 0xbeef0000, 0x711dc, 0x721dc] {
        assert!(!f.tracked_reads.borrow().contains_key(&a));
    }
}
#[test]
fn foreign_owner_alternate_controller_and_wrong_primary_never_expand_scope() {
    for (a, n, code, blocked) in [
        (0x20008, 2, "invalid_army_owner", 0x20290),
        (0x71000, 0x10000 + 0x4278270, "invalid_army_type", 0x711d8),
        (0x711d8, 2, "invalid_army_owner", 0x71018),
    ] {
        let mut f = Fixture::new();
        f.army();
        if a == 0x71000 {
            f.word(a, n);
        } else {
            f.number(a, n as i32);
        }
        f.number(0x711dc, 1);
        assert_eq!(hoi4_live_native::army::snapshot(&f).unwrap_err().code, code);
        assert!(!f.tracked_reads.borrow().contains_key(&blocked));
        assert!(!f.tracked_reads.borrow().contains_key(&0x71420));
    }
}
#[test]
fn duplicate_pointers_full_handle_aliases_and_null_handles_refuse() {
    for mode in 0..3 {
        let mut f = Fixture::new();
        f.army();
        match mode {
            0 => f.word(0x70008, 0x71000),
            1 => f.put(0x7201c, &1u32.to_le_bytes()),
            _ => f.word(0x71018, 0),
        };
        assert_eq!(
            hoi4_live_native::army::snapshot(&f).unwrap_err().code,
            "ambiguous_army_identity"
        );
    }
}
#[test]
fn own_location_join_and_unknown_layout_do_not_borrow_fields() {
    let mut f = Fixture::new();
    f.army();
    f.word(0x74010, 0x73008);
    assert_eq!(
        hoi4_live_native::army::snapshot(&f).unwrap_err().code,
        "invalid_army_location"
    );
    let mut f = Fixture::new();
    f.army();
    f.word(0x73000, 0x12345678);
    for a in 0x730a4..0x730a8 {
        f.data.remove(&a);
    }
    let v = hoi4_live_native::army::snapshot(&f).unwrap();
    assert_eq!(
        v["divisions"][0]["location"]["reason"],
        "unknown_province_layout"
    );
    assert!(!f.tracked_reads.borrow().contains_key(&0x730a4));
    assert!(!f.tracked_reads.borrow().contains_key(&0x74010));
}
#[test]
fn own_list_bound_and_negative_tag_refuse_before_payload() {
    let mut f = Fixture::new();
    f.army();
    f.number(0x20298, 4097);
    assert_eq!(
        hoi4_live_native::army::snapshot(&f).unwrap_err().code,
        "invalid_array"
    );
    assert!(!f.tracked_reads.borrow().contains_key(&0x70000));
    let mut f = Fixture::new();
    f.army();
    f.number(0x30518, 0);
    assert_eq!(
        hoi4_live_native::army::snapshot(&f).unwrap_err().code,
        "player_unavailable"
    );
    assert!(!f.tracked_reads.borrow().contains_key(&0x39004));
}
#[test]
fn own_army_identity_location_status_and_full_list_are_final_witnesses() {
    for (a, n) in [
        (0x20290, 16),
        (0x70000, 16),
        (0x71000, 8),
        (0x711d8, 4),
        (0x71018, 8),
        (0x711f0, 8),
        (0x73000, 8),
        (0x730a4, 4),
        (0x302b0, 16),
        (0x74010, 8),
        (0x71420, 8),
        (0x71428, 8),
    ] {
        let mut f = Fixture::new();
        f.army();
        let mut b = (0..n).map(|i| f.data[&(a + i)]).collect::<Vec<_>>();
        b[0] ^= 1;
        f.fault = Some((a, 1, b));
        assert_eq!(
            hoi4_live_native::army::snapshot(&f).unwrap_err().code,
            "torn_read"
        );
    }
}
#[test]
fn unsupported_build_and_replaced_process_refuse() {
    let mut f = Fixture::new();
    f.army();
    f.unknown_build = true;
    assert_eq!(
        hoi4_live_native::army::snapshot(&f).unwrap_err().code,
        "unknown_build"
    );
    assert!(f.tracked_reads.borrow().is_empty());
    let mut invalid = Fixture::new();
    invalid.army();
    invalid.identity.start_ticks.clear();
    assert_eq!(
        hoi4_live_native::army::snapshot(&invalid).unwrap_err().code,
        "identity_unavailable"
    );
    assert!(invalid.tracked_reads.borrow().is_empty());
    let mut f = Fixture::new();
    f.army();
    f.replaced = true;
    assert_eq!(
        hoi4_live_native::army::snapshot(&f).unwrap_err().code,
        "ownership_changed"
    );
}
#[test]
fn army_tool_is_noargs_and_capability_scope_is_explicit() {
    let s = hoi4_live_native::protocol::Server {
        process: None,
        unavailable: None,
    };
    let v=s.handle(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_army","arguments":{"country":"OTHER"}}})).unwrap();
    assert!(v.to_string().contains("invalid_arguments"));
    let v=s.handle(serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"hoi4_capabilities","arguments":{}}})).unwrap();
    assert!(v.to_string().contains("own_division_snapshot_identifiers"));
}
