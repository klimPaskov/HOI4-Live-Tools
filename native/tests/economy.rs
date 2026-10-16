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
        f.word(0x28130, 0x20000);
        f.number(0x28138, 1);
        f.array(0x280a0, 0x3b000, 4);
        f.region(0x3b000, 32);
        for i in 0..4 {
            let a = 0x2c000 + i * 0x100;
            f.region(a, 64);
            f.object(a, "CResearchSlot", 0x50300);
            f.word(a + 0x10, 0x28000);
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

fn production_fixture() -> Fixture {
    let mut f = Fixture::new();
    f.economy();
    f.array(0x40058, 0x70000, 3);
    f.region(0x70000, 24);
    for (index, p, table, handle) in [
        (0, 0x80000, 0x42ede08, 0xffff000100000038u64),
        (1, 0x81000, 0x42edf88, 0xffff000100000039u64),
        (2, 0x82000, 0x42ede08, 0x0000000200000038u64),
    ] {
        f.region(p, 0xf8);
        f.word(0x70000 + index * 8, p);
        f.word(p, 0x10000 + table);
        f.word(p + 0x20, 0x40000);
        f.word(p + 8, handle);
        f.number(p + 0x14, 12);
        f.number(p + 0x18, 2);
        f.number(p + 0xe8, 15);
        f.word(p + 0x88, 0);
    }
    f
}
fn production_not_read(f: &Fixture, addresses: &[u64]) {
    for a in addresses {
        assert!(
            !f.tracked_reads.borrow().contains_key(a),
            "Forbidden payload address"
        );
    }
}
#[test]
fn production_lines_preserve_full_string_handles_distinct_counters_and_own_context() {
    let f = production_fixture();
    let (v, c) = hoi4_live_native::production_lines::owned_snapshot(&f).unwrap();
    assert_eq!(v["lines"][0]["line_handle"], "0xffff000100000038");
    assert_eq!(v["lines"][1]["line_handle"], "0xffff000100000039");
    assert_eq!(
        v["lines"][0]["factory_counters"]["active_factories_raw_signed"],
        12
    );
    assert_eq!(
        v["lines"][0]["factory_counters"]["damaged_factories_raw_signed"],
        2
    );
    assert_eq!(
        v["lines"][0]["factory_counters"]["requested_factories_raw_signed"],
        15
    );
    assert_eq!(c.campaign.player, "GER");
    assert_eq!(c.own, [0x20000, 0x40000]);
    assert_eq!(c.entries, [0x80000, 0x81000, 0x82000]);
    assert_eq!(f.checks.get(), 2);
    assert_eq!(v["complete_catalog"], true);
    assert_eq!(v["scope"], "own_equipment_line_collection_only");
    assert_eq!(v["general_production_collection_available"], false);
    assert_eq!(v["handle_scope"]["player"], "GER");
    assert_eq!(
        v["handle_scope"]["playthrough_id"],
        c.campaign.playthrough_id
    );
    assert!(v["handle_scope"].get("exe").is_none());
    assert_eq!(v["unsupported_count"], 0);
    for row in v["lines"].as_array().unwrap() {
        assert!(row["line_handle"].is_string());
        assert!(row["equipment_id"].is_null());
        assert_eq!(row["input_authorized"], false);
    }
    for flag in [
        "equipment_ids_available",
        "ui_allocation_available",
        "idle_capacity_available",
        "cross_load_identity_available",
        "eligibility_available",
        "input_authorized",
    ] {
        assert_eq!(v[flag], false);
    }
    production_not_read(&f, &[0x10000 + 0x439f440, 0xdead0000, 0xbeef0000]);
}
#[test]
fn production_lines_foreign_owner_refuses_before_handle_counter_and_equipment() {
    let mut f = production_fixture();
    f.word(0x80020, 0x40008);
    assert_eq!(
        hoi4_live_native::production_lines::snapshot(&f)
            .unwrap_err()
            .code,
        "invalid_production_owner"
    );
    production_not_read(&f, &[0x80008, 0x80014, 0x80018, 0x800e8, 0x80088]);
    for (address, value, code) in [
        (0x20008, 2, "invalid_economy_owner"),
        (0x30518, 0, "player_unavailable"),
    ] {
        let mut f = production_fixture();
        f.number(address, value);
        assert_eq!(
            hoi4_live_native::production_lines::snapshot(&f)
                .unwrap_err()
                .code,
            code
        );
        production_not_read(&f, &[0x40058, 0x80000]);
    }
    let mut f = production_fixture();
    f.word(0x40290, 0xdead0000);
    assert_eq!(
        hoi4_live_native::production_lines::snapshot(&f)
            .unwrap_err()
            .code,
        "invalid_economy_owner"
    );
    production_not_read(&f, &[0x40058, 0xdead0000]);
}
#[test]
fn production_lines_header_alignment_capacity_and_pointer_duplicates_precede_payload() {
    for (capacity, count, data, code) in [
        (2049, 3, 0xdead0000, "invalid_production_array"),
        (3, 4, 0x70000, "invalid_array"),
        (-1, 0, 0, "invalid_array"),
        (3, 3, 0x70001, "invalid_production_array"),
    ] {
        let mut f = production_fixture();
        f.word(0x40058, data);
        f.number(0x40060, capacity);
        f.number(0x40064, count);
        assert_eq!(
            hoi4_live_native::production_lines::snapshot(&f)
                .unwrap_err()
                .code,
            code
        );
        production_not_read(&f, &[0x70000, 0x70001, 0xdead0000, 0x80000]);
    }
    for pointer in [0, 0x80001, 0x80000] {
        let mut f = production_fixture();
        f.word(0x70008, pointer);
        assert_eq!(
            hoi4_live_native::production_lines::snapshot(&f)
                .unwrap_err()
                .code,
            "invalid_production_array"
        );
        production_not_read(&f, &[0x80000, 0x81000]);
    }
}
#[test]
fn production_lines_duplicate_full_handles_and_wrong_kind_refuse_before_counters() {
    for (address, value) in [
        (0x80008, 0u64),
        (0x80008, 0x0000000100000039),
        (0x82008, 0xffff000100000038),
    ] {
        let mut f = production_fixture();
        f.word(address, value);
        assert_eq!(
            hoi4_live_native::production_lines::snapshot(&f)
                .unwrap_err()
                .code,
            "invalid_production_handle"
        );
        let line = address - 8;
        production_not_read(&f, &[line + 0x14, line + 0x18, line + 0xe8, line + 0x88]);
    }
}
fn unknown_production(f: &mut Fixture, secondary: bool) {
    let base = 0x10000;
    let table = base + 0x4320000;
    let info = base + 0x435a000;
    let name = base + 0x350a000;
    f.array(0x40058, 0x70000, 4);
    f.word(0x70018, 0x90000);
    f.word(0x90000, table);
    f.word(table - 16, if secondary { (-80i64) as u64 } else { 0 });
    f.word(table - 8, info);
    f.region(info, 16);
    f.word(info + 8, name);
    f.region(name, 128);
    f.put(name, b"21CGeneratedUnknownLine");
}
#[test]
fn production_lines_unknown_and_secondary_types_never_assume_owner_or_payload() {
    for secondary in [false, true] {
        let mut f = production_fixture();
        unknown_production(&mut f, secondary);
        let v = hoi4_live_native::production_lines::snapshot(&f).unwrap();
        assert_eq!(v["collection_count"], 4);
        assert_eq!(v["known_line_count"], 3);
        assert_eq!(v["unsupported_count"], 1);
        assert_eq!(v["complete_catalog"], false);
        assert_eq!(v["counter_data_complete"], false);
        if secondary {
            assert!(v["unsupported_type_metadata"][0]["type_encoding"].is_null());
        } else {
            assert_eq!(
                v["unsupported_type_metadata"][0]["type_encoding"],
                "21CGeneratedUnknownLine"
            );
        }
        production_not_read(&f, &[0x90008, 0x90014, 0x90018, 0x90020, 0x90088, 0x900e8]);
    }
}
#[test]
fn production_lines_signed_anomalous_counters_are_raw_but_explicitly_unavailable() {
    for (offset, value) in [
        (0x14, -1),
        (0x18, i32::MIN),
        (0xe8, 1_000_001),
        (0xe8, i32::MAX),
    ] {
        let mut f = production_fixture();
        f.number(0x80000 + offset, value);
        let v = hoi4_live_native::production_lines::snapshot(&f).unwrap();
        assert_eq!(v["anomalous_counter_count"], 1);
        assert_eq!(v["counter_data_complete"], false);
        let c = &v["lines"][0]["factory_counters"];
        assert_eq!(c["available_under_reviewed_policy"], false);
        assert_eq!(c["unavailable_reason"], "anomalous_counter_policy");
        let field = match offset {
            0x14 => "active_factories_raw_signed",
            0x18 => "damaged_factories_raw_signed",
            _ => "requested_factories_raw_signed",
        };
        assert_eq!(c[field], value);
    }
    let mut f = production_fixture();
    for offset in [0x14, 0x18, 0xe8] {
        f.number(0x80000 + offset, 1_000_000);
    }
    assert_eq!(
        hoi4_live_native::production_lines::snapshot(&f).unwrap()["counter_data_complete"],
        true
    );
}
#[test]
fn production_lines_all_collection_owner_handle_counter_and_roots_are_witnessed() {
    for (address, width) in [
        (0x40058, 16),
        (0x70000, 24),
        (0x80000, 8),
        (0x80020, 8),
        (0x80008, 8),
        (0x80014, 4),
        (0x80018, 4),
        (0x800e8, 4),
        (0x20f40, 8),
        (0x40290, 8),
        (0x20008, 4),
        (0x10000 + 0x44086a8, 8),
        (0x10000 + 0x44085f8, 8),
        (0x60000, 36),
    ] {
        let mut f = production_fixture();
        let mut changed: Vec<_> = (0..width).map(|i| f.data[&(address + i)]).collect();
        changed[0] ^= 1;
        f.fault = Some((address, 1, changed));
        assert_eq!(
            hoi4_live_native::production_lines::snapshot(&f)
                .unwrap_err()
                .code,
            "torn_read",
            "witness {address:x}"
        );
    }
}
#[test]
fn production_lines_identical_members_relocation_and_process_context_change_refuse() {
    let mut f = production_fixture();
    let mut header = 0x71000u64.to_le_bytes().to_vec();
    header.extend(3i32.to_le_bytes());
    header.extend(3i32.to_le_bytes());
    let copied: Vec<_> = (0..24).map(|i| f.data[&(0x70000 + i)]).collect();
    f.put(0x71000, &copied);
    f.fault = Some((0x40058, 1, header));
    assert_eq!(
        hoi4_live_native::production_lines::snapshot(&f)
            .unwrap_err()
            .code,
        "torn_read"
    );
    let mut f = production_fixture();
    f.replaced = true;
    assert_eq!(
        hoi4_live_native::production_lines::snapshot(&f)
            .unwrap_err()
            .code,
        "ownership_changed"
    );
    let mut f = production_fixture();
    f.changed_id = true;
    assert_eq!(
        hoi4_live_native::production_lines::snapshot(&f)
            .unwrap_err()
            .code,
        "torn_read"
    );
    let mut f = production_fixture();
    f.word(0x20000, 0x10000 + 0x41fafb8);
    assert_eq!(
        hoi4_live_native::production_lines::snapshot(&f)
            .unwrap_err()
            .code,
        "invalid_economy_type"
    );
    production_not_read(&f, &[0x20f40, 0x40058]);
}
#[test]
fn production_lines_empty_catalog_is_known_without_any_line_or_general_collection() {
    let mut f = production_fixture();
    f.array(0x40058, 0, 0);
    let v = hoi4_live_native::production_lines::snapshot(&f).unwrap();
    assert_eq!(v["collection_count"], 0);
    assert_eq!(v["complete_catalog"], true);
    assert_eq!(v["known_line_count"], 0);
    assert_eq!(v["unsupported_count"], 0);
    f.number(0x40060, 2048);
    assert_eq!(
        hoi4_live_native::production_lines::snapshot(&f).unwrap()["collection_count"],
        0
    );
    production_not_read(&f, &[0x80000, 0x40070]);
}
#[test]
fn production_lines_protocol_is_read_only_no_foreign_or_action_arguments() {
    let s = hoi4_live_native::protocol::Server {
        process: None,
        unavailable: None,
    };
    for args in [
        serde_json::json!({"country":"OTHER"}),
        serde_json::json!({"line_id":"x"}),
        serde_json::json!({"allocate":1}),
        serde_json::json!({"equipment_id":"x"}),
        serde_json::json!({"offset":2049}),
        serde_json::json!({"offset":-1}),
        serde_json::json!({"offset":1.0}),
        serde_json::json!({"limit":0}),
        serde_json::json!({"limit":129}),
        serde_json::json!({"limit":"32"}),
        serde_json::json!({"limit":null}),
        serde_json::json!([]),
    ] {
        let v=s.handle(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_production_lines","arguments":args}})).unwrap();
        assert!(v.to_string().contains("invalid_arguments"));
    }
    let list = s
        .handle(serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .unwrap();
    let t = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "hoi4_production_lines")
        .unwrap();
    assert_eq!(t["annotations"]["readOnlyHint"], true);
    assert_eq!(t["inputSchema"]["properties"]["limit"]["default"], 32);
    assert_eq!(t["inputSchema"]["properties"]["limit"]["maximum"], 128);
    assert_eq!(t["inputSchema"]["properties"]["offset"]["maximum"], 2048);
}

#[test]
fn production_lines_maximum_page_with_long_metadata_fits_owned_mcp_wire_bound() {
    let mut f = production_fixture();
    unknown_production(&mut f, false);
    f.put(0x10000 + 0x350a000, &[b'A'; 127]);
    for offset in [0x14, 0x18, 0xe8] {
        f.number(0x80000 + offset, i32::MIN);
    }
    let mut value = hoi4_live_native::production_lines::snapshot(&f).unwrap();
    assert_eq!(
        value["unsupported_type_metadata"][0]["type_encoding"]
            .as_str()
            .unwrap()
            .len(),
        127
    );
    // Exercise real projections and maximum signed widths in the entire RPC
    // envelope: text content is itself escaped JSON, not just raw row bytes.
    let mut row = value["lines"][0].clone();
    row["entry_index"] = serde_json::json!(2047);
    let mut unknown = value["unsupported_type_metadata"][0].clone();
    unknown["entry_index"] = serde_json::json!(2047);
    value["collection_count"] = serde_json::json!(2048);
    value["total_count"] = serde_json::json!(2048);
    value["supported_count"] = serde_json::json!(1024);
    value["known_line_count"] = serde_json::json!(1024);
    value["unsupported_count"] = serde_json::json!(1024);
    value["limit"] = serde_json::json!(128);
    value["page_count"] = serde_json::json!(128);
    for known in [0, 64, 128] {
        value["lines"] = serde_json::json!(vec![row.clone(); known]);
        value["unsupported_type_metadata"] = serde_json::json!(vec![unknown.clone(); 128 - known]);
        let encoded=serde_json::to_string(&serde_json::json!({"jsonrpc":"2.0","id":1,"result":{"content":[{"type":"text","text":serde_json::to_string(&value).unwrap()}]}})).unwrap();
        assert!(encoded.len() <= hoi4_live_native::framing::LIMIT);
    }
}

#[test]
fn production_lines_pages_preserve_collection_indices_global_coverage_and_scope() {
    let mut f = production_fixture();
    unknown_production(&mut f, false);
    let first = hoi4_live_native::production_lines::page(&f, 1, 2).unwrap();
    assert_eq!(first["total_count"], 4);
    assert_eq!(first["supported_count"], 3);
    assert_eq!(first["unsupported_count"], 1);
    assert_eq!(first["complete_catalog"], false);
    assert_eq!(first["page_count"], 2);
    assert_eq!(first["next_offset"], 3);
    assert_eq!(first["lines"][0]["entry_index"], 1);
    assert_eq!(first["lines"][1]["entry_index"], 2);
    assert!(first["unsupported_type_metadata"]
        .as_array()
        .unwrap()
        .is_empty());
    let last = hoi4_live_native::production_lines::page(&f, 3, 128).unwrap();
    assert_eq!(last["page_count"], 1);
    assert_eq!(last["supported_count"], 3);
    assert_eq!(last["complete_catalog"], false);
    assert_eq!(last["unsupported_type_metadata"][0]["entry_index"], 3);
    assert_eq!(
        last["unsupported_type_metadata"][0]["ownership_verified"],
        false
    );
    assert!(last["lines"].as_array().unwrap().is_empty());
    assert!(last["next_offset"].is_null());
    let empty = hoi4_live_native::production_lines::page(&f, 2048, 1).unwrap();
    assert_eq!(empty["total_count"], 4);
    assert_eq!(empty["page_count"], 0);
    assert!(empty["next_offset"].is_null());
    production_not_read(&f, &[0x40070, 0x90008, 0x90020]);
}

#[test]
fn production_lines_paging_never_skips_outside_page_guards_or_witnesses() {
    for (address, value, expected) in [
        (0x82020, 0xdead0000, "invalid_production_owner"),
        (0x82008, 0xffff000100000038, "invalid_production_handle"),
        (0x70010, 0x80000, "invalid_production_array"),
    ] {
        let mut f = production_fixture();
        f.word(address, value);
        assert_eq!(
            hoi4_live_native::production_lines::page(&f, 0, 1)
                .unwrap_err()
                .code,
            expected
        );
    }
    let mut f = production_fixture();
    f.fault = Some((0x820e8, 1, 16i32.to_le_bytes().to_vec()));
    assert_eq!(
        hoi4_live_native::production_lines::page(&f, 0, 1)
            .unwrap_err()
            .code,
        "torn_read"
    );
    let mut f = production_fixture();
    unknown_production(&mut f, false);
    let mut changed = vec![0; 128];
    let name = b"21CGeneratedChangedLine";
    changed[..name.len()].copy_from_slice(name);
    f.fault = Some((0x10000 + 0x350a000, 1, changed));
    assert_eq!(
        hoi4_live_native::production_lines::page(&f, 0, 1)
            .unwrap_err()
            .code,
        "torn_read"
    );
}

#[test]
fn production_lines_library_page_bounds_fail_before_process_or_payload_access() {
    for (offset, limit) in [(2049, 32), (0, 0), (0, 129), (usize::MAX, 1)] {
        let f = production_fixture();
        assert_eq!(
            hoi4_live_native::production_lines::page(&f, offset, limit)
                .unwrap_err()
                .code,
            "invalid_arguments"
        );
        assert_eq!(f.checks.get(), 0);
        assert!(f.tracked_reads.borrow().is_empty());
    }
    let f = production_fixture();
    let value = hoi4_live_native::production_lines::snapshot(&f).unwrap();
    assert_eq!(value["offset"], 0);
    assert_eq!(value["limit"], 32);
    assert_eq!(value["page_count"], 3);
}

#[test]
fn production_lines_default_page_is_compact_with_fresh_nonpersistent_collection_indices() {
    let mut f = production_fixture();
    f.array(0x40058, 0x70000, 35);
    for index in 0..35u64 {
        let line = 0x80000 + index * 0x1000;
        f.region(line, 0xf8);
        f.word(0x70000 + index * 8, line);
        f.word(line, 0x10000 + 0x42ede08);
        f.word(line + 0x20, 0x40000);
        f.word(line + 8, ((index + 1) << 32) | 0x38);
    }
    let first = hoi4_live_native::production_lines::snapshot(&f).unwrap();
    assert_eq!(first["total_count"], 35);
    assert_eq!(first["supported_count"], 35);
    assert_eq!(first["page_count"], 32);
    assert_eq!(first["next_offset"], 32);
    assert_eq!(first["lines"][31]["entry_index"], 31);
    assert_eq!(first["complete_catalog"], true);
    let last = hoi4_live_native::production_lines::page(&f, 32, 128).unwrap();
    assert_eq!(last["page_count"], 3);
    assert_eq!(last["lines"][0]["entry_index"], 32);
    assert!(last["next_offset"].is_null());
    assert_eq!(last["page_index_scope"], "fresh_snapshot_collection_only");
}

#[path = "economy/equipment.rs"]
mod equipment;

#[path = "economy/construction.rs"]
mod construction;
