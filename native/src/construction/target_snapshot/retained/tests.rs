use super::*;
use crate::{
    actions::Epoch,
    own_focus::assignment_fixture::{Fixture, BASE, COUNTRY, GS, STATUS},
};
use std::{cell::Cell, time::Duration};
const PRODUCTION: u64 = 0x900000;
const DB: u64 = 0x910000;
const TEMPLATE: u64 = 0x920000;
const NULL: u64 = 0x930000;
const STATE: u64 = 0x940000;
const BUILDING: u64 = 0x950000;
const LINE: u64 = 0xa00000;
const LINE2: u64 = LINE + 0x100;
const FOREIGN: u64 = 0xdead0000;
fn abis() -> [StringAbi; 2] {
    [StringAbi::LibStdCxx, StringAbi::LibCxx]
}
fn until() -> Instant {
    Instant::now() + Duration::from_secs(2)
}
fn scope(f: &Fixture) -> Scope {
    let ctx = f.context();
    Scope {
        epoch: Epoch {
            game: f.game(),
            campaign: format!("queue:{}", ctx.playthrough_id),
            player: ctx.player,
        },
        anchors: ctx.anchors,
        own_country: COUNTRY,
        own_status: STATUS,
    }
}
fn fixture(abi: StringAbi) -> Fixture {
    let mut f = Fixture::new(abi);
    let g = f.game_layout();
    let l = c::layout(&Reader::new(&f));
    for (a, v) in [
        (COUNTRY + g.country_production, PRODUCTION),
        (PRODUCTION, BASE + g.production_vtable),
        (PRODUCTION + g.production(0x290), COUNTRY),
        (BASE + l.database_global, DB),
        (DB, BASE + l.database_vtable),
        (TEMPLATE, BASE + l.template_vtable),
        (NULL, BASE + l.null_template_vtable),
        (STATE, BASE + l.state_vtable),
        (BUILDING, BASE + l.building_vtable),
        (BUILDING + l.building_template, TEMPLATE),
        (BUILDING + l.building_status, STATE + l.state_status),
        (STATE + l.state_status, BASE + l.status_vtable),
    ] {
        f.word(a, v);
    }
    f.put(NULL + 0x18, &[0]);
    f.put(TEMPLATE + 0x18, &[1]);
    f.string(TEMPLATE + l.template_key, "arms_factory");
    f.int(TEMPLATE + l.template_ordinal, 1);
    f.array(DB + 0x40, &[NULL, TEMPLATE]);
    f.array(GS + l.gs_states, &[0, STATE]);
    for (a, v) in [
        (STATE + l.state_id, 1),
        (STATE + l.state_owner, 1),
        (STATE + l.state_controller, 1),
        (STATE + l.state_status + 0x68, 2),
        (STATE + l.state_status + 0x6c, 1),
        (STATE + l.state_status + 0x70, 0),
    ] {
        f.int(a, v);
    }
    f.array(STATE + l.state_status + 0x20, &[]);
    // Territorial template->building index is i32, not the pointer-array helper.
    let slots = 0x960000;
    f.word(STATE + l.state_status + 0x20, slots);
    f.int(STATE + l.state_status + 0x28, 2);
    f.int(STATE + l.state_status + 0x2c, 2);
    f.int(slots, -1);
    f.int(slots + 4, 0);
    f.array(STATE + l.state_status + 0x38, &[BUILDING]);
    for (line, handle) in [(LINE, 0x10000003b), (LINE2, 0x20000003b)] {
        for (a, v) in [
            (line, BASE + l.building_line_vtable),
            (line + 8, handle),
            (line + 0x20, PRODUCTION),
            (line + 0x28, 100000),
            (line + 0x30, 100),
            (line + 0x38, 0),
            (line + 0x70 + l.line_tail_shift, BUILDING),
        ] {
            f.word(a, v);
        }
        for (a, v) in [
            (line + 0x14, 1),
            (line + 0x18, 0),
            (line + 0x40, 1),
            (line + 0x44, 0),
            (line + 0x48, 2),
            (line + 0x78 + l.line_tail_shift, 0),
        ] {
            f.int(a, v);
        }
        f.put(line + 0x8c + l.line_tail_shift, &[0]);
        f.put(line + 0x99 + l.line_tail_shift, &[0]);
    }
    f.array(PRODUCTION + 0x70, &[LINE, LINE2]);
    f
}
fn bind(f: &Fixture) -> Anchor {
    Anchor::bind_scoped(f, &scope(f), "arms_factory", 1, until()).unwrap()
}
fn poison(f: &Fixture, error: Error) {
    assert_ne!(
        error.code, "fixture_unmapped",
        "foreign payload was followed: {error:?}"
    );
    assert!(f.untouched(FOREIGN, 0x10000));
}
#[test]
fn both_abis_bind_complete_original_and_keep_public_output_unchanged() {
    for abi in abis() {
        let f = fixture(abi);
        let before = super::super::snapshot(&f, "arms_factory", 1).unwrap();
        let a = bind(&f);
        let o = a.observe_unchanged(&f, until()).unwrap();
        assert_eq!(a.scope(), &scope(&f));
        assert_eq!(a.game(), &f.game());
        assert_eq!(a.building_id(), "arms_factory");
        assert_eq!(a.state_id(), 1);
        assert_eq!(a.line_count(), 2);
        assert_eq!(o.line_count(), 2);
        assert_eq!(o.context(), a.context());
        let after = super::super::snapshot(&f, "arms_factory", 1).unwrap();
        let mut before = before;
        let mut after = after;
        before.as_object_mut().unwrap().remove("freshness");
        after.as_object_mut().unwrap().remove("freshness");
        assert_eq!(before, after);
    }
}
#[test]
fn advancing_date_is_allowed_but_regression_setup_or_playthrough_is_not() {
    for abi in abis() {
        let mut f = fixture(abi);
        let a = bind(&f);
        let date = a.context().date_raw;
        f.set_date(date + 25);
        assert_eq!(
            a.observe_unchanged(&f, until()).unwrap().context().date_raw,
            date + 25
        );
        f.set_date(date - 1);
        assert_eq!(
            a.observe_unchanged(&f, until()).unwrap_err().code,
            "construction_queue_changed"
        );
        f.set_date(date);
        // Same independent setup fields seeded by the reused ABI fixture;
        // production code continues to use game::context_from_reader.
        let (difficulty, historical) = match abi {
            StringAbi::LibCxx => (0x5d0, 0x5d8),
            StringAbi::LibStdCxx => (0x620, 0x628),
        };
        f.int(GS + difficulty, 1);
        assert!(a.observe_unchanged(&f, until()).is_err());
        f.int(GS + difficulty, 2);
        f.int(GS + historical, 0);
        assert!(a.observe_unchanged(&f, until()).is_err());
        f.int(GS + historical, 1);
        f.set_playthrough("new-campaign");
        assert!(a.observe_unchanged(&f, until()).is_err());
    }
}
#[test]
fn original_scope_roots_country_and_status_refuse_before_replacement_payload() {
    for abi in abis() {
        for which in 0..5 {
            let mut f = fixture(abi);
            let a = bind(&f);
            let g = f.game_layout();
            let address = match which {
                0 => BASE + g.gs_global,
                1 => BASE + g.idler_global,
                2 => BASE + g.technology_db_global,
                3 => COUNTRY + g.country_technology_status,
                _ => f.get_word(GS + g.countries),
            };
            f.word(address, FOREIGN);
            poison(&f, a.observe_unchanged(&f, until()).unwrap_err());
            assert!(f.untouched(PRODUCTION + 0x70, 16));
        }
    }
}
#[test]
fn scoped_initial_bind_never_follows_replacement_country() {
    for abi in abis() {
        let mut f = fixture(abi);
        let s = scope(&f);
        let g = f.game_layout();
        f.word(f.get_word(GS + g.countries), FOREIGN);
        poison(
            &f,
            Anchor::bind_scoped(&f, &s, "arms_factory", 1, until()).unwrap_err(),
        );
    }
}
#[test]
fn requested_key_state_and_own_initial_state_are_required_even_with_empty_queue() {
    for abi in abis() {
        let mut f = fixture(abi);
        f.array(PRODUCTION + 0x70, &[]);
        for (key, state) in [("arms factory", 1), ("rail_way", 1), ("arms_factory", 0)] {
            let snapshots = f.snapshots.get();
            assert!(Anchor::bind_scoped(&f, &scope(&f), key, state, until()).is_err());
            assert_eq!(f.snapshots.get(), snapshots);
        }
        assert_eq!(
            Anchor::bind_scoped(&f, &scope(&f), "dockyard", 1, until())
                .unwrap_err()
                .code,
            "construction_target_unavailable"
        );
        let l = c::layout(&Reader::new(&f));
        f.int(STATE + l.state_owner, 2);
        assert!(Anchor::bind_scoped(&f, &scope(&f), "arms_factory", 1, until()).is_err());
        assert!(f.untouched(TEMPLATE + l.template_key, 24));
        assert!(f.untouched(PRODUCTION + 0x70, 16));
    }
}
#[test]
fn original_production_owner_and_pointer_refuse_before_queue_payload() {
    for abi in abis() {
        for field in 0..2 {
            let mut f = fixture(abi);
            let a = bind(&f);
            let address = if field == 0 {
                COUNTRY + f.game_layout().country_production
            } else {
                PRODUCTION + f.game_layout().production(0x290)
            };
            f.word(address, FOREIGN);
            poison(&f, a.observe_unchanged(&f, until()).unwrap_err());
            assert!(f.untouched(PRODUCTION + 0x70, 16));
        }
    }
}
#[test]
fn original_database_catalog_header_member_and_target_replacements_refuse_before_payload() {
    for abi in abis() {
        for field in 0..3 {
            let mut f = fixture(abi);
            let a = bind(&f);
            let l = c::layout(&Reader::new(&f));
            let address = match field {
                0 => BASE + l.database_global,
                1 => DB + 0x40,
                _ => f.get_word(DB + 0x40) + 8,
            };
            f.word(address, FOREIGN);
            poison(&f, a.observe_unchanged(&f, until()).unwrap_err());
            assert!(f.untouched(PRODUCTION + 0x70, 16));
        }
    }
}
#[test]
fn requested_state_pointer_or_ownership_change_refuses_before_queue_and_template() {
    for abi in abis() {
        for field in 0..3 {
            let mut f = fixture(abi);
            let a = bind(&f);
            let l = c::layout(&Reader::new(&f));
            if field == 0 {
                f.word(f.get_word(GS + l.gs_states) + 8, FOREIGN);
            } else {
                f.int(
                    STATE
                        + if field == 1 {
                            l.state_owner
                        } else {
                            l.state_controller
                        },
                    2,
                );
            }
            poison(&f, a.observe_unchanged(&f, until()).unwrap_err());
            assert!(f.untouched(TEMPLATE + l.template_key, 24));
            assert!(f.untouched(PRODUCTION + 0x70, 16));
        }
    }
}
#[test]
fn catalog_key_ordinal_or_type_changes_refuse_without_queue() {
    for abi in abis() {
        for field in 0..3 {
            let mut f = fixture(abi);
            let a = bind(&f);
            let l = c::layout(&Reader::new(&f));
            match field {
                0 => f.string(TEMPLATE + l.template_key, "dockyard"),
                1 => f.int(TEMPLATE + l.template_ordinal, 2),
                _ => f.word(TEMPLATE, BASE + l.null_template_vtable),
            }
            assert!(a.observe_unchanged(&f, until()).is_err());
            assert!(f.untouched(PRODUCTION + 0x70, 16));
        }
    }
}
#[test]
fn queue_reallocation_member_replacement_or_reorder_never_follows_foreign_row() {
    for abi in abis() {
        for field in 0..3 {
            let mut f = fixture(abi);
            let a = bind(&f);
            match field {
                0 => f.word(PRODUCTION + 0x70, FOREIGN),
                1 => f.word(f.get_word(PRODUCTION + 0x70), FOREIGN),
                _ => {
                    let data = f.get_word(PRODUCTION + 0x70);
                    f.word(data, LINE2);
                    f.word(data + 8, LINE);
                }
            }
            poison(&f, a.observe_unchanged(&f, until()).unwrap_err());
            assert!(f.untouched(LINE, 8));
        }
    }
}
#[test]
fn queue_amount_handle_building_template_and_location_changes_refuse_before_foreign_payload() {
    for abi in abis() {
        for field in 0..6 {
            let mut f = fixture(abi);
            let a = bind(&f);
            let l = c::layout(&Reader::new(&f));
            match field {
                0 => f.int(LINE + 0x48, 3),
                1 => f.word(LINE + 8, 0x30000003b),
                2 => f.word(LINE + 0x70 + l.line_tail_shift, FOREIGN),
                3 => f.word(BUILDING + l.building_template, FOREIGN),
                4 => f.word(BUILDING + l.building_status, FOREIGN),
                _ => f.word(LINE, BASE + l.railway_line_vtable),
            }
            poison(&f, a.observe_unchanged(&f, until()).unwrap_err());
        }
    }
}

struct Probe<'a> {
    inner: &'a Fixture,
    base: u64,
    short: Option<(u64, usize)>,
    batches: Cell<usize>,
    batch_delay: Duration,
}
impl<'a> Probe<'a> {
    fn new(inner: &'a Fixture) -> Self {
        Self {
            inner,
            base: BASE,
            short: None,
            batches: Cell::new(0),
            batch_delay: Duration::ZERO,
        }
    }
}
impl ReadMemory for Probe<'_> {
    fn string_abi(&self) -> StringAbi {
        self.inner.string_abi()
    }
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        let mut bytes = self.inner.read(address, size)?;
        if self.short == Some((address, size)) {
            bytes.pop();
        }
        Ok(bytes)
    }
    fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        self.batches.set(self.batches.get() + 1);
        std::thread::sleep(self.batch_delay);
        self.inner.read_many(requests)
    }
}
impl Process for Probe<'_> {
    fn identity(&self) -> Option<&Identity> {
        self.inner.identity()
    }
    fn image_base(&self) -> u64 {
        self.base
    }
    fn begin_snapshot(&self) -> Result<()> {
        self.inner.begin_snapshot()
    }
    fn check(&self) -> Result<()> {
        self.inner.check()
    }
}
#[test]
fn invalid_or_replaced_image_base_refuses_without_new_snapshot() {
    for abi in abis() {
        let f = fixture(abi);
        let a = bind(&f);
        for base in [0, 1, u64::MAX, BASE + 0x10000] {
            let mut p = Probe::new(&f);
            p.base = base;
            let snapshots = f.snapshots.get();
            assert!(a.observe_unchanged(&p, until()).is_err());
            assert_eq!(f.snapshots.get(), snapshots);
            if base != BASE + 0x10000 {
                assert!(Anchor::bind_scoped(&p, &scope(&f), "arms_factory", 1, until()).is_err());
                assert_eq!(f.snapshots.get(), snapshots);
            }
        }
    }
}
#[test]
fn short_header_read_cannot_publish_queue_evidence() {
    for abi in abis() {
        let f = fixture(abi);
        let a = bind(&f);
        let mut p = Probe::new(&f);
        p.short = Some((PRODUCTION + 0x70, 16));
        assert_eq!(
            a.observe_unchanged(&p, until()).unwrap_err().code,
            "short_read"
        );
        assert!(f.untouched(LINE, 8));
    }
}
#[test]
fn final_witness_delegates_bounded_batches_instead_of_scalar_transfers() {
    for abi in abis() {
        let f = fixture(abi);
        let a = bind(&f);
        let p = Probe::new(&f);
        assert_eq!(a.observe_unchanged(&p, until()).unwrap().line_count(), 2);
        let fields = f.read_counts().len();
        assert!(fields > 32);
        assert_eq!(p.batches.get(), fields.div_ceil(128));
        assert!(p.batches.get() < fields);
    }
}
#[test]
fn late_final_witness_batch_is_discarded_without_next_transfer() {
    for abi in abis() {
        let f = fixture(abi);
        let a = bind(&f);
        let mut p = Probe::new(&f);
        p.batch_delay = Duration::from_millis(125);
        assert_eq!(
            a.observe_unchanged(&p, Instant::now() + Duration::from_millis(100))
                .unwrap_err()
                .code,
            "construction_queue_deadline"
        );
        assert_eq!(p.batches.get(), 1);
        assert_eq!(
            f.checks.get(),
            1,
            "Late witness must not publish or run the final process check"
        );
    }
}
#[test]
fn ordinary_progress_changes_do_not_change_queue_identity() {
    for abi in abis() {
        let mut f = fixture(abi);
        let a = bind(&f);
        f.word(LINE + 0x28, 200000);
        f.word(LINE + 0x30, 200);
        f.word(LINE + 0x38, 100);
        f.int(LINE + 0x14, 2);
        f.int(LINE + 0x44, 3);
        assert_eq!(a.observe_unchanged(&f, until()).unwrap().line_count(), 2);
    }
}
fn with_provincial_row(mut f: Fixture) -> Fixture {
    let l = c::layout(&Reader::new(&f));
    let province = 0x980000;
    let building = 0x990000;
    let status = province + l.province_status;
    for (a, v) in [
        (province, BASE + l.province_vtable),
        (province + l.province_state, STATE),
        (status, BASE + l.status_vtable),
        (building, BASE + l.building_vtable),
        (building + l.building_status, status),
        (building + l.building_template, TEMPLATE),
        (LINE2 + 0x70 + l.line_tail_shift, building),
    ] {
        f.word(a, v);
    }
    for (a, v) in [
        (province + l.province_owner, 1),
        (province + l.province_id, 1),
        (status + 0x68, 1),
        (status + 0x6c, 1),
        (status + 0x70, 0),
    ] {
        f.int(a, v);
    }
    f.array(GS + l.gs_provinces, &[0, province]);
    let indices = 0x9a0000;
    f.word(status + 0x20, indices);
    f.int(status + 0x28, 2);
    f.int(status + 0x2c, 2);
    f.int(indices, -1);
    f.int(indices + 4, 0);
    f.array(status + 0x38, &[building]);
    f
}
#[test]
fn full_provincial_row_identity_is_retained_before_any_changed_province_payload() {
    for abi in abis() {
        for which in 0..5 {
            let mut f = with_provincial_row(fixture(abi));
            let a = bind(&f);
            assert_eq!(a.observe_unchanged(&f, until()).unwrap().line_count(), 2);
            let l = c::layout(&Reader::new(&f));
            match which {
                0 => f.word(GS + l.gs_provinces, FOREIGN),
                1 => f.word(f.get_word(GS + l.gs_provinces) + 8, FOREIGN),
                2 => f.int(0x980000 + l.province_owner, 2),
                3 => f.word(0x980000 + l.province_state, FOREIGN),
                _ => f.int(0x980000 + l.province_id, 2),
            }
            poison(&f, a.observe_unchanged(&f, until()).unwrap_err());
        }
    }
}
#[test]
fn incomplete_queue_is_unavailable_not_zero_and_empty_queue_still_retains_catalog() {
    for abi in abis() {
        let mut f = fixture(abi);
        f.word(LINE + 0x70 + c::layout(&Reader::new(&f)).line_tail_shift, 0);
        assert_eq!(
            Anchor::bind_scoped(&f, &scope(&f), "arms_factory", 1, until())
                .unwrap_err()
                .code,
            "construction_queue_unavailable"
        );
        let mut f = fixture(abi);
        f.array(PRODUCTION + 0x70, &[]);
        let a = bind(&f);
        assert_eq!(a.line_count(), 0);
        let l = c::layout(&Reader::new(&f));
        f.word(BASE + l.database_global, FOREIGN);
        poison(&f, a.observe_unchanged(&f, until()).unwrap_err());
    }
}
#[test]
fn torn_observation_and_final_identity_loss_never_publish() {
    for abi in abis() {
        let mut f = fixture(abi);
        let a = bind(&f);
        f.tear(LINE + 0x38, 8, 2);
        assert_eq!(
            a.observe_unchanged(&f, until()).unwrap_err().code,
            "torn_read"
        );
        let mut f = fixture(abi);
        let a = bind(&f);
        f.swap_final_identity = true;
        assert!(a.observe_unchanged(&f, until()).is_err());
    }
}
#[test]
fn expired_original_bound_refuses_without_snapshot_or_reads() {
    for abi in abis() {
        let f = fixture(abi);
        let a = bind(&f);
        let snapshots = f.snapshots.get();
        assert_eq!(
            a.observe_unchanged(&f, Instant::now()).unwrap_err().code,
            "construction_queue_deadline"
        );
        assert_eq!(f.snapshots.get(), snapshots);
    }
}
struct Late {
    inner: Fixture,
    checks: Cell<usize>,
}
impl ReadMemory for Late {
    fn string_abi(&self) -> StringAbi {
        self.inner.string_abi()
    }
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        self.inner.read(a, n)
    }
}
impl Process for Late {
    fn image_base(&self) -> u64 {
        BASE
    }
    fn identity(&self) -> Option<&Identity> {
        self.inner.identity()
    }
    fn begin_snapshot(&self) -> Result<()> {
        self.checks.set(0);
        self.inner.begin_snapshot()
    }
    fn check(&self) -> Result<()> {
        self.checks.set(self.checks.get() + 1);
        std::thread::sleep(Duration::from_millis(20));
        self.inner.check()
    }
}
#[test]
fn late_final_success_is_discarded_under_same_original_deadline() {
    for abi in abis() {
        let f = fixture(abi);
        let a = bind(&f);
        let late = Late {
            inner: f,
            checks: Cell::new(0),
        };
        assert_eq!(
            a.observe_unchanged(&late, Instant::now() + Duration::from_millis(15))
                .unwrap_err()
                .code,
            "construction_queue_deadline"
        );
    }
}

const MANAGER: u64 = 0xb00000;
fn linux_fixture(mode: i32) -> Fixture {
    let mut f = fixture(StringAbi::LibStdCxx);
    let idler = scope(&f).anchors[1];
    f.word(idler + 0x6b8, MANAGER);
    f.word(MANAGER + 0x180, idler);
    f.int(MANAGER + 0x3f0, mode);
    f.word(MANAGER + 0x3e8, TEMPLATE);
    f.word(DB + 0x3a8, NULL);
    f
}
fn linux_bind(f: &Fixture) -> Anchor {
    Anchor::bind_linux_selection_scoped(f, &scope(f), "arms_factory", 1, until()).unwrap()
}
#[test]
fn default_queue_remains_gui_free_on_both_abis() {
    for abi in abis() {
        let f = fixture(abi);
        let idler = scope(&f).anchors[1];
        let a = bind(&f);
        assert_eq!(a.linux_selection(), None);
        assert_eq!(
            a.observe_unchanged(&f, until()).unwrap().linux_selection(),
            None
        );
        assert!(f.untouched(idler + 0x6b8, 8));
        assert!(f.untouched(MANAGER, 0x400));
    }
}
#[test]
fn linux_selection_refuses_mac_abi_before_snapshot_or_payload() {
    let f = fixture(StringAbi::LibCxx);
    let s = scope(&f);
    let before = f.read_counts();
    let snapshots = f.snapshots.get();
    assert_eq!(
        Anchor::bind_linux_selection_scoped(&f, &s, "arms_factory", 1, until())
            .unwrap_err()
            .code,
        "unsupported_platform"
    );
    assert_eq!(f.snapshots.get(), snapshots);
    assert_eq!(f.read_counts(), before);
}
#[test]
fn inactive_linux_selection_never_reads_default_pointer_or_railway_cache() {
    let mut f = linux_fixture(0);
    f.word(MANAGER + 0x3e8, FOREIGN);
    f.word(DB + 0x3a8, FOREIGN);
    let a = linux_bind(&f);
    assert_eq!(a.linux_selection(), Some(Selection::Inactive));
    let o = a.observe_unchanged(&f, until()).unwrap();
    assert_eq!(o.linux_selection(), Some(Selection::Inactive));
    assert!(f.untouched(MANAGER + 0x3e8, 8));
    assert!(f.untouched(DB + 0x3a8, 8));
    assert!(f.untouched(FOREIGN, 0x10000));
}
#[test]
fn same_original_queue_can_observe_selection_transitions_without_claiming_effect() {
    let mut f = linux_fixture(0);
    let a = linux_bind(&f);
    f.int(MANAGER + 0x3f0, 1);
    f.set_date(a.context().date_raw + 25);
    f.word(LINE + 0x38, 100);
    let o = a.observe_unchanged(&f, until()).unwrap();
    assert_eq!(o.linux_selection(), Some(Selection::Requested));
    assert_eq!(o.line_count(), a.line_count());
    assert_eq!(o.context().date_raw, a.context().date_raw + 25);
    assert_eq!(a.linux_selection(), Some(Selection::Inactive));
    f.int(MANAGER + 0x3f0, 0);
    f.word(MANAGER + 0x3e8, FOREIGN);
    assert_eq!(
        a.observe_unchanged(&f, until()).unwrap().linux_selection(),
        Some(Selection::Inactive)
    );
}
#[test]
fn manager_replacement_and_backlink_fail_before_replacement_or_queue_payload() {
    for replacement in [true, false] {
        let mut f = linux_fixture(1);
        let a = linux_bind(&f);
        if replacement {
            f.word(scope(&f).anchors[1] + 0x6b8, FOREIGN);
        } else {
            f.word(MANAGER + 0x180, FOREIGN);
        }
        poison(&f, a.observe_unchanged(&f, until()).unwrap_err());
        assert!(f.untouched(MANAGER + 0x3f0, 4));
        assert!(f.untouched(PRODUCTION + 0x70, 16));
    }
}
#[test]
fn unsupported_modes_and_nonrequested_templates_never_follow_selected_payload() {
    for mode in [-1, 2, 3, 4, i32::MAX] {
        let mut f = linux_fixture(mode);
        f.word(MANAGER + 0x3e8, FOREIGN);
        let e = Anchor::bind_linux_selection_scoped(&f, &scope(&f), "arms_factory", 1, until())
            .unwrap_err();
        assert_eq!(e.code, "construction_selection_unavailable");
        assert!(f.untouched(MANAGER + 0x3e8, 8));
        assert!(f.untouched(FOREIGN, 0x10000));
    }
    for selected in [FOREIGN, NULL, 0] {
        let mut f = linux_fixture(1);
        let a = linux_bind(&f);
        f.word(MANAGER + 0x3e8, selected);
        let e = a.observe_unchanged(&f, until()).unwrap_err();
        assert_eq!(e.code, "construction_selection_unavailable");
        assert!(f.untouched(FOREIGN, 0x10000));
        assert!(f.untouched(DB + 0x3a8, 8));
    }
}
#[test]
fn typed_linux_selection_cannot_adopt_replacement_catalog_or_same_name_template() {
    for replacement in [true, false] {
        let mut f = linux_fixture(1);
        let a = linux_bind(&f);
        if replacement {
            let l = c::layout(&Reader::new(&f));
            f.word(BASE + l.database_global, FOREIGN);
        } else {
            f.word(f.get_word(DB + 0x40) + 8, FOREIGN);
            f.word(MANAGER + 0x3e8, FOREIGN);
        }
        poison(&f, a.observe_unchanged(&f, until()).unwrap_err());
        assert!(f.untouched(MANAGER + 0x3e8, 8));
    }
}
#[test]
fn railway_cache_cannot_relabel_requested_template_as_ordinary() {
    let mut f = linux_fixture(1);
    let a = linux_bind(&f);
    f.word(DB + 0x3a8, TEMPLATE);
    assert_eq!(
        a.observe_unchanged(&f, until()).unwrap_err().code,
        "construction_selection_unavailable"
    );
}
#[test]
fn linux_selection_and_queue_share_final_witness_and_original_deadline() {
    for address in [MANAGER + 0x3f0, MANAGER + 0x3e8] {
        let mut f = linux_fixture(1);
        let a = linux_bind(&f);
        let size = if address == MANAGER + 0x3f0 { 4 } else { 8 };
        f.tear(address, size, 2);
        assert_eq!(
            a.observe_unchanged(&f, until()).unwrap_err().code,
            "torn_read"
        );
    }
    let mut f = linux_fixture(1);
    let a = linux_bind(&f);
    f.int(LINE + 0x48, 3);
    assert!(a.observe_unchanged(&f, until()).is_err());
    let snapshots = f.snapshots.get();
    assert_eq!(
        a.observe_unchanged(&f, Instant::now()).unwrap_err().code,
        "construction_queue_deadline"
    );
    assert_eq!(f.snapshots.get(), snapshots);
}
#[test]
fn another_valid_original_catalog_member_is_not_requested_selection() {
    let mut f = linux_fixture(1);
    let other = 0xc00000;
    let l = c::layout(&Reader::new(&f));
    f.word(other, BASE + l.template_vtable);
    f.put(other + 0x18, &[1]);
    f.int(other + l.template_ordinal, 2);
    f.string(other + l.template_key, "dockyard");
    f.array(DB + 0x40, &[NULL, TEMPLATE, other]);
    let a = linux_bind(&f);
    f.word(MANAGER + 0x3e8, other);
    assert_eq!(
        a.observe_unchanged(&f, until()).unwrap_err().code,
        "construction_selection_unavailable"
    );
    assert!(f.untouched(DB + 0x3a8, 8));
}
#[test]
fn original_scope_replacement_refuses_before_linux_manager_access() {
    let mut f = linux_fixture(1);
    let a = linux_bind(&f);
    let idler = scope(&f).anchors[1];
    f.word(BASE + f.game_layout().idler_global, FOREIGN);
    poison(&f, a.observe_unchanged(&f, until()).unwrap_err());
    assert!(f.untouched(idler + 0x6b8, 8));
    assert!(f.untouched(MANAGER, 0x400));
}
