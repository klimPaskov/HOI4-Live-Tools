use super::*;
use serde_json::json;

const PROGRESS: u64 = 0x300000;
const DATA: u64 = 0x310000;
const SECOND: u64 = 0x320000;

fn fixture() -> Fake {
    let mut f = Fake::new();
    f.int(GS + 0x460, 60_759_371);
    f.word(CO + 0x1348, PROGRESS);
    f.region(PROGRESS, 0x50);
    f.word(PROGRESS, 0x4202638);
    f.word(PROGRESS + 8, CO);
    f.array(PROGRESS + 0x40, DATA, &[F]);
    // Poison the upstream field: this build's completion predicate uses +0x40.
    f.int(PROGRESS + 0x2c, -1);
    f
}
fn read(f: &Fake, offset: usize, limit: usize) -> Result<serde_json::Value> {
    own_focus::completion_snapshot(f, offset, limit)
}
fn add_second(f: &mut Fake) {
    f.region(SECOND, 0x168);
    f.word(SECOND, 0x42024d0);
    f.word(SECOND + 0x160, TREE);
    f.string(SECOND + 0x18, "GER_alpha");
    f.array(TREE + 0x30, 0x212000, &[F, SECOND]);
    f.array(DB + 0x58, 0x213000, &[SECOND, F, F, 0]);
    f.array(PROGRESS + 0x40, DATA, &[F, SECOND, F]);
}

#[test]
fn completion_uses_engine_membership_without_gui_or_upstream_field() {
    let f = fixture();
    let value = read(&f, 0, 100).unwrap();
    assert_eq!(value["completed"], json!(["GER_rhineland"]));
    assert_eq!(value["date"]["text"], "1936.01.01.12");
    assert!(value["active_key"].is_null());
    assert_eq!(value["input_authorized"], false);
    assert_eq!(value["observation_paused_game"], false);
    assert!(f.untouched(PROGRESS + 0x20, 16));
    assert!(f.untouched(ID + 0x4f8, 8));
    assert!(f.untouched(ID + 0x6b8, 8));
    assert!(f.untouched(VIEW, 0x1800));
    assert_eq!(f.checks.get(), 2);
}

#[test]
fn completion_sorted_paging_preserves_membership_not_repeat_counts() {
    let mut f = fixture();
    add_second(&mut f);
    f.word(PROGRESS + 0x10, SECOND);
    let value = read(&f, 0, 1).unwrap();
    assert_eq!(value["completed"], json!(["GER_alpha"]));
    assert_eq!(value["active_key"], "GER_alpha");
    assert_eq!(value["membership_entries"], 3);
    assert_eq!(value["completed_count"], 2);
    assert_eq!(value["next_offset"], 1);
    let value = read(&f, 1, 1).unwrap();
    assert_eq!(value["completed"], json!(["GER_rhineland"]));
    assert!(value["next_offset"].is_null());
    assert_eq!(read(&f, 20, 1).unwrap()["completed"], json!([]));
}

#[test]
fn empty_completed_membership_is_distinct_from_an_active_focus() {
    let mut f = fixture();
    f.array(PROGRESS + 0x40, 0, &[]);
    f.word(PROGRESS + 0x10, F);
    let value = read(&f, 0, 10).unwrap();
    assert_eq!(value["completed"], json!([]));
    assert_eq!(value["completed_count"], 0);
    assert_eq!(value["active_key"], "GER_rhineland");
}

#[test]
fn foreign_progress_refuses_before_membership_payload() {
    let mut f = fixture();
    f.word(PROGRESS + 8, CO + 8);
    assert_eq!(read(&f, 0, 1).unwrap_err().code, "invalid_focus_owner");
    assert!(f.untouched(PROGRESS + 0x10, 0x40));
    assert!(f.untouched(DATA, 8));
}

#[test]
fn foreign_or_null_membership_refuses_before_any_foreign_payload() {
    for pointer in [SECOND, 0] {
        let mut f = fixture();
        f.array(PROGRESS + 0x40, DATA, &[F, pointer]);
        assert_eq!(read(&f, 0, 1).unwrap_err().code, "invalid_focus_owner");
        assert!(f.untouched(SECOND, 0x200));
        assert!(f.untouched(F + 0x18, 32));
    }
    let mut f = fixture();
    f.word(PROGRESS + 0x10, SECOND);
    assert_eq!(read(&f, 0, 1).unwrap_err().code, "invalid_focus_owner");
    assert!(f.untouched(SECOND, 0x200));
}

#[test]
fn validates_unreturned_members_and_duplicate_keys_before_pagination() {
    let mut f = fixture();
    add_second(&mut f);
    f.word(SECOND + 0x160, TREE + 8);
    assert_eq!(read(&f, 4096, 1).unwrap_err().code, "invalid_focus_owner");
    let mut f = fixture();
    add_second(&mut f);
    f.string(SECOND + 0x18, "GER_rhineland");
    assert_eq!(read(&f, 0, 1).unwrap_err().code, "invalid_focus_catalog");
}

#[test]
fn completion_membership_and_owner_witnesses_refuse_torn_reads() {
    for address in [
        0x44086a8,
        GS + 0x460,
        GS + 0x518,
        CO + 0x1348,
        PROGRESS + 8,
        PROGRESS + 0x10,
        PROGRESS + 0x40,
        DATA,
        TREE + 0x30,
        F + 0x160,
        F + 0x18,
    ] {
        let mut f = fixture();
        f.torn = Some(address);
        assert_eq!(read(&f, 0, 1).unwrap_err().code, "torn_read", "{address:x}");
    }
}

#[test]
fn completion_requires_canonical_primary_types_and_tag() {
    for (address, bad, code) in [
        (PROGRESS, 0x4202639, "invalid_focus_type"),
        (F, 0x42024d1, "invalid_focus_type"),
        (TREE, 0x4202411, "invalid_focus_type"),
        (CO + 8, 2, "invalid_focus_owner"),
    ] {
        let mut f = fixture();
        f.word(address, bad);
        assert_eq!(read(&f, 0, 1).unwrap_err().code, code);
    }
    let mut f = fixture();
    f.array(DB + 0x58, 0x213000, &[]);
    assert_eq!(read(&f, 0, 1).unwrap_err().code, "invalid_focus_catalog");
    assert!(f.untouched(PROGRESS + 0x40, 16));
}

#[test]
fn completion_refuses_bad_bounds_before_payload() {
    for (offset, limit) in [(4097, 1), (0, 0), (0, 101)] {
        let f = fixture();
        assert_eq!(
            read(&f, offset, limit).unwrap_err().code,
            "invalid_arguments"
        );
        assert_eq!(f.checks.get(), 0);
    }
    for (capacity, count) in [(4097, 1), (1, 2), (1, -1)] {
        let mut f = fixture();
        f.int(PROGRESS + 0x48, capacity);
        f.int(PROGRESS + 0x4c, count);
        assert_eq!(read(&f, 0, 1).unwrap_err().code, "invalid_array");
        assert!(f.untouched(DATA, 8));
    }
}

#[test]
fn completion_checks_final_process_ownership() {
    let mut f = fixture();
    f.fail_final = true;
    assert_eq!(read(&f, 0, 1).unwrap_err().code, "ownership_changed");
}
