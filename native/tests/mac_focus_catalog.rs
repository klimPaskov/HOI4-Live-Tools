use hoi4_live_native::{
    error::{Error, Result},
    memory::{ReadMemory, StringAbi},
    own_focus,
    platform::Process,
};
use std::{cell::RefCell, collections::BTreeMap};
const GS: u64 = 0x100000;
const ID: u64 = 0x110000;
const CO: u64 = 0x120000;
const TREE: u64 = 0x130000;
const DB: u64 = 0x140000;
const A: u64 = 0x150000;
const Z: u64 = 0x160000;
struct Fake {
    data: BTreeMap<u64, u8>,
    reads: RefCell<Vec<(u64, usize)>>,
    torn: Option<u64>,
}
impl Fake {
    fn put(&mut self, a: u64, b: &[u8]) {
        for (i, v) in b.iter().enumerate() {
            self.data.insert(a + i as u64, *v);
        }
    }
    fn word(&mut self, a: u64, v: u64) {
        self.put(a, &v.to_le_bytes());
    }
    fn int(&mut self, a: u64, v: i32) {
        self.put(a, &v.to_le_bytes());
    }
    fn array(&mut self, a: u64, p: u64, values: &[u64]) {
        self.word(a, p);
        self.int(a + 8, values.len() as i32);
        self.int(a + 12, values.len() as i32);
        for (i, v) in values.iter().enumerate() {
            self.word(p + i as u64 * 8, *v);
        }
    }
    fn string(&mut self, a: u64, key: &str) {
        assert!(key.len() <= 22);
        let mut b = [0; 24];
        b[0] = (key.len() * 2) as u8;
        b[1..1 + key.len()].copy_from_slice(key.as_bytes());
        self.put(a, &b);
    }
    fn new() -> Self {
        let mut f = Self {
            data: BTreeMap::new(),
            reads: RefCell::new(vec![]),
            torn: None,
        };
        for (a, v) in [
            (0x3519740, GS),
            (0x35196c0, ID),
            (GS, 0x3306260),
            (ID, 0x3308f90),
            (CO, 0x337bc38),
            (CO + 0x10f8, TREE),
            (TREE, 0x33863a8),
            (0x34d0a10, DB),
            (DB, 0x3386528),
            (A, 0x3386468),
            (Z, 0x3386468),
            (A + 0x130, TREE),
            (Z + 0x130, TREE),
        ] {
            f.word(a, v);
        }
        f.int(GS + 0x4d8, 1);
        f.int(CO + 8, 1);
        f.word(GS + 0x308, 0x200000);
        f.int(GS + 0x310, 2);
        f.int(GS + 0x314, 2);
        f.int(0x200000, -1);
        f.int(0x200004, 0);
        f.array(GS + 0x2d8, 0x210000, &[CO]);
        f.array(TREE + 0x28, 0x220000, &[A, Z]);
        f.array(DB + 0x58, 0x230000, &[A, 0, Z, A]);
        f.string(A + 0x18, "GER_rhineland");
        f.string(Z + 0x18, "GER_autarky");
        f
    }
}
impl ReadMemory for Fake {
    fn string_abi(&self) -> StringAbi {
        StringAbi::LibCxx
    }
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        let repeated = self.reads.borrow().contains(&(a, n));
        self.reads.borrow_mut().push((a, n));
        let mut out: Vec<u8> = (0..n)
            .map(|i| {
                self.data.get(&(a + i as u64)).copied().ok_or_else(|| {
                    Error::new("fixture_unmapped", "No Linux or UI payload in Mac fixture")
                })
            })
            .collect::<Result<_>>()?;
        if repeated {
            if let Some(t) = self.torn {
                if (a..a + n as u64).contains(&t) {
                    out[(t - a) as usize] ^= 1;
                }
            }
        }
        Ok(out)
    }
}
impl Process for Fake {
    fn check(&self) -> Result<()> {
        Ok(())
    }
    fn image_base(&self) -> u64 {
        0
    }
}
#[test]
fn mac_catalog_reads_own_canonical_keys_without_linux_or_ui_offsets() {
    let f = Fake::new();
    let v = own_focus::snapshot(&f, 0, 1, Some("GER_")).unwrap();
    assert_eq!(v["build_id"], "089EA519-6780-353B-8E39-4E9309B444D4");
    assert_eq!(v["catalog_count"], 2);
    assert_eq!(v["next_offset"], 1);
    assert_eq!(v["focuses"][0]["key"], "GER_rhineland");
    assert_eq!(v["details_available"], false);
    assert_eq!(v["eligibility_available"], false);
    assert_eq!(v["input_authorized"], false);
    let second = own_focus::snapshot(&f, 1, 1, None).unwrap();
    assert_eq!(second["focuses"][0]["key"], "GER_autarky");
    assert!(second["next_offset"].is_null());
    assert_eq!(
        own_focus::snapshot(&f, 0, 10, Some("Rhineland")).unwrap()["filtered_count"],
        0
    );
    assert_eq!(
        own_focus::snapshot(&f, 0, 10, Some("GER_rhineland")).unwrap()["filtered_count"],
        1
    );
}
#[test]
fn invalid_unreturned_member_is_not_hidden_by_filter_or_page() {
    for bad in [0, TREE + 8] {
        let mut f = Fake::new();
        f.word(Z + 0x130, bad);
        assert_eq!(
            own_focus::snapshot(&f, 0, 1, Some("GER_rhineland"))
                .unwrap_err()
                .code,
            "invalid_focus_owner"
        );
    }
    let mut f = Fake::new();
    f.string(Z + 0x18, "GER_rhineland");
    assert_eq!(
        own_focus::snapshot(&f, 0, 1, None).unwrap_err().code,
        "invalid_focus_catalog"
    );
    let mut f = Fake::new();
    f.array(TREE + 0x28, 0x220000, &[A, A]);
    assert!(own_focus::snapshot(&f, 0, 1, None).is_err());
    let mut f = Fake::new();
    f.array(DB + 0x58, 0x230000, &[A]);
    assert!(own_focus::snapshot(&f, 0, 1, None).is_err());
}
#[test]
fn ownership_and_final_witnesses_refuse() {
    let mut f = Fake::new();
    f.int(CO + 8, 2);
    assert_eq!(
        own_focus::snapshot(&f, 0, 10, None).unwrap_err().code,
        "invalid_focus_owner"
    );
    for target in [
        0x3519740,
        0x35196c0,
        GS,
        ID,
        GS + 0x4d8,
        0x200004,
        0x210000,
        CO,
        CO + 8,
        CO + 0x10f8,
        TREE,
        TREE + 0x28,
        DB,
        DB + 0x58,
        0x220000,
        0x230000,
        A,
        A + 0x130,
        A + 0x18,
        Z + 0x18,
    ] {
        let mut f = Fake::new();
        f.torn = Some(target);
        assert_eq!(
            own_focus::snapshot(&f, 0, 1, None).unwrap_err().code,
            "torn_read",
            "{target:x}"
        );
    }
}
#[test]
fn unported_mac_ui_diagnostics_refuse_before_reading() {
    let f = Fake {
        data: BTreeMap::new(),
        reads: RefCell::new(vec![]),
        torn: None,
    };
    for result in [
        own_focus::node_detail(&f, "GER_rhineland"),
        own_focus::selected_detail(&f, "GER_rhineland", true),
        own_focus::search_state(&f, "GER_rhineland"),
        own_focus::search_state_with_control(&f, "GER_rhineland"),
    ] {
        assert_eq!(result.unwrap_err().code, "capability_unavailable");
    }
    assert_eq!(
        own_focus::command_owned(&f, "GER_rhineland", true)
            .unwrap_err()
            .code,
        "capability_unavailable"
    );
    assert!(f.reads.borrow().is_empty());
}

fn progress_fixture() -> Fake {
    let mut f = Fake::new();
    let progress = 0x170000;
    // Mac RTTI for the existing own CCountry primary object.
    f.word(0x337bc38 - 8, 0x180000);
    f.word(0x180000 + 8, 0x190000);
    f.put(0x190000, b"8CCountry\0");
    f.int(GS + 0x428, 60_759_371);
    f.word(CO + 0x1108, progress);
    f.word(progress, 0x33865d0);
    f.word(progress + 8, CO);
    f.word(progress + 0x10, Z);
    f.array(progress + 0x40, 0x240000, &[A, A]);
    f
}

#[test]
fn mac_progress_is_one_complete_typed_own_observation() {
    let f = progress_fixture();
    let value = own_focus::progress_snapshot(&f).unwrap();
    assert_eq!(value.active_key(), Some("GER_autarky"));
    assert_eq!(
        value.completed().iter().cloned().collect::<Vec<_>>(),
        ["GER_rhineland"]
    );
    let page = own_focus::completion_snapshot(&f, 0, 1).unwrap();
    assert_eq!(page["active_key"], "GER_autarky");
    assert_eq!(page["membership_entries"], 2);
    assert_eq!(page["completed_count"], 1);
    assert_eq!(page["date"]["text"], "1936.01.01.12");
    // The fixture maps neither Linux roots nor a Mac focus-view manager.
    assert!(!f.reads.borrow().iter().any(|(a, _)| *a == ID + 0x670));
}

#[test]
fn mac_progress_rejects_torn_or_foreign_membership_before_returning_a_model() {
    for address in [GS + 0x428, CO + 0x1108, 0x170010, 0x240000, Z + 0x130] {
        let mut f = progress_fixture();
        f.torn = Some(address);
        assert_eq!(
            own_focus::progress_snapshot(&f).unwrap_err().code,
            "torn_read"
        );
    }
    let mut f = progress_fixture();
    f.word(0x170008, CO + 8);
    assert_eq!(
        own_focus::progress_snapshot(&f).unwrap_err().code,
        "invalid_focus_owner"
    );
    assert!(!f.reads.borrow().iter().any(|(a, _)| *a == 0x240000));
}
