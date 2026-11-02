//! Generated Mac-only layout facts, exercised on every build host.
use super::*;
use crate::{
    memory::{ReadMemory, StringAbi},
    own_focus::assignment_fixture::{Fixture, BASE, COUNTRY, DATE, FOREIGN, GS, IDLER},
};

const PRODUCTION: u64 = 0x900000;
const REGISTRY: u64 = 0x910000;
const MANAGER: u64 = 0x920000;
const FIND_OBJECT: u64 = 0x930000;
const WINDOW: u64 = 0x940000;
const FIRST_ROW: u64 = 0x950000;
const ENTRIES: u64 = 0x960000;
const CAMERA: u64 = 0x970000;
const STATE: u64 = 0x980000;
const TEMPLATE: u64 = 0x990000;

fn floats(f: &mut Fixture, address: u64, values: &[f32]) {
    for (i, value) in values.iter().enumerate() {
        f.put(address + i as u64 * 4, &value.to_le_bytes());
    }
}

fn fixture() -> Fixture {
    let mut f = Fixture::new(StringAbi::LibCxx);
    for (field, value) in [
        (COUNTRY + 0xd10, PRODUCTION),
        (PRODUCTION, BASE + 0x358e0c8),
        (PRODUCTION + 0x2a8, COUNTRY),
        (IDLER + 0x4c0, REGISTRY),
        (REGISTRY, BASE + 0x36a5810),
        (IDLER + 0x670, MANAGER),
        (MANAGER + 0x180, IDLER),
        (MANAGER + 0x190, FIND_OBJECT),
        (FIND_OBJECT, BASE + FIND),
        (FIND_OBJECT + 0xd50, IDLER),
        (FIND_OBJECT + 0xd58, WINDOW),
        (WINDOW, BASE + 0x36ba978),
        (WINDOW + 0x88, REGISTRY),
        (FIND_OBJECT + 0xda8, FIRST_ROW),
        (FIRST_ROW, BASE + ROW),
        (FIRST_ROW + 0x458, STATE),
        (IDLER + 0x718, CAMERA),
        (STATE, BASE + 0x357f410),
        (STATE + 0x20, TEMPLATE),
        (TEMPLATE, BASE + STATE_TEMPLATE),
    ] {
        f.word(field, value);
    }
    f.put(WINDOW + 0x95, &[8]);
    f.int(WINDOW + 0x13d8, 0);
    f.int(WINDOW + 0x1390, 0);
    f.string(FIND_OBJECT + 0xd68, "1");
    f.word(FIND_OBJECT + 0xdb0, ENTRIES);
    f.int(FIND_OBJECT + 0xdb8, 1);
    f.int(FIND_OBJECT + 0xdbc, 1);
    f.put(ENTRIES, &[0; ENTRY_BYTES]);
    f.word(ENTRIES + 0x30, STATE);
    f.int(ENTRIES + 0x38, 2);
    f.int(FIRST_ROW + 0x468, 2);
    f.array(GS + 0x290, &[0, STATE]);
    f.int(STATE + 0x40, 1);
    f.int(STATE + 0xc0, 1);
    f.int(STATE + 0xc4, 1);
    f.int(TEMPLATE + 0x120, 3120);
    f.int(TEMPLATE + 0x124, 1480);
    floats(
        &mut f,
        CAMERA + 0x698,
        &[3120.0, 300.0, 1390.0, 3120.0, 9.5, 1480.0],
    );
    f
}

#[test]
fn mac_row_and_own_anchor_join_without_linux_offsets() {
    let f = fixture();
    let value = snapshot(&f, Some(1)).unwrap();
    assert_eq!(value["find_view"]["query"], "1");
    assert_eq!(value["find_view"]["results"]["first_row"]["state_id"], 1);
    assert_eq!(value["camera"]["look_at"], json!([3120.0, 9.5, 1480.0]));
    assert_eq!(value["target"]["camera_centered"], true);
    for key in [
        "input_authorized",
        "placement_eligibility_verified",
        "effective_routing_verified",
        "current_frame_verified",
        "observation_paused_game",
    ] {
        assert_eq!(value[key], false);
    }
    for address in [
        IDLER + 0x750,
        FIND_OBJECT + 0x878,
        FIND_OBJECT + 0x8c8,
        TEMPLATE + 0x140,
    ] {
        assert!(f.untouched(address, 8));
    }
}

#[test]
fn hidden_and_unmaterialized_find_skip_retained_query_and_rows() {
    for materialized in [false, true] {
        let mut f = fixture();
        if materialized {
            f.put(WINDOW + 0x95, &[0]);
        } else {
            f.word(FIND_OBJECT + 0xd58, 0);
        }
        f.word(FIND_OBJECT + 0xda8, 0xdead0000);
        let value = snapshot(&f, None).unwrap();
        assert_eq!(value["find_view"]["materialized"], materialized);
        assert!(value["find_view"]["query"].is_null());
        assert!(value["find_view"]["results"].is_null());
        assert!(f.untouched(FIND_OBJECT + 0xd68, 24));
        assert!(f.untouched(FIND_OBJECT + 0xda8, 8));
        assert!(f.untouched(0xdead0000, 8));
    }
}

#[test]
fn foreign_target_exposes_no_template_anchor_payload() {
    let mut f = fixture();
    f.int(STATE + 0xc0, 7);
    let value = snapshot(&f, Some(1)).unwrap();
    assert_eq!(value["target"]["own_owned_and_controlled"], false);
    assert!(value["target"]["anchor"].is_null());
    assert!(f.untouched(STATE + 0x20, 8));
    assert!(f.untouched(TEMPLATE, 0x128));
}

#[test]
fn first_result_kind_and_object_disagreement_refuse_as_refresh() {
    for (address, value) in [(ENTRIES + 0x38, 1), (ENTRIES + 0x30, STATE + 8)] {
        let mut f = fixture();
        if address == ENTRIES + 0x30 {
            f.word(address, value);
        } else {
            f.int(address, value as i32);
        }
        assert_eq!(snapshot(&f, None).unwrap_err().code, "torn_read");
    }
}

#[test]
fn visible_empty_results_skip_the_retained_old_row() {
    let mut f = fixture();
    f.int(FIND_OBJECT + 0xdbc, 0);
    f.word(FIND_OBJECT + 0xda8, 0xdead0000);
    let value = snapshot(&f, None).unwrap();
    assert_eq!(value["find_view"]["results"]["result_count"], 0);
    assert!(value["find_view"]["results"]["first_row"].is_null());
    assert!(f.untouched(FIND_OBJECT + 0xda8, 8));
    assert!(f.untouched(0xdead0000, 8));
}

#[test]
fn other_result_kinds_do_not_follow_their_object_payload() {
    for (kind, field) in [(1, 0x450), (3, 0x460)] {
        let mut f = fixture();
        f.int(FIRST_ROW + 0x468, kind);
        f.int(ENTRIES + 0x38, kind);
        f.word(FIRST_ROW + field, 0xdead0000);
        f.word(ENTRIES + 0x30, 0xdead0000);
        let value = snapshot(&f, None).unwrap();
        assert_eq!(
            value["find_view"]["results"]["first_row"]["supported"],
            false
        );
        assert!(f.untouched(0xdead0000, 8));
    }
}

#[test]
fn camera_and_anchor_comparison_are_facts_without_delivery_authority() {
    let mut f = fixture();
    floats(&mut f, CAMERA + 0x6a4, &[3121.0, 9.5, 1480.0]);
    assert_eq!(
        snapshot(&f, Some(1)).unwrap()["target"]["camera_centered"],
        false
    );
    for v in [f32::NAN, f32::INFINITY, 1_000_001.0] {
        let mut f = fixture();
        floats(&mut f, CAMERA + 0x698, &[v]);
        assert!(snapshot(&f, Some(1)).is_err());
    }
    let mut f = fixture();
    f.int(TEMPLATE + 0x120, i32::MIN);
    assert!(snapshot(&f, Some(1)).is_err());
}

#[test]
fn stale_own_links_types_state_catalog_and_local_states_refuse() {
    for (field, value) in [
        (FIND_OBJECT, BASE + FIND + 8),
        (FIND_OBJECT + 0xd50, IDLER + 8),
        (MANAGER + 0x180, IDLER + 8),
        (WINDOW + 0x88, REGISTRY + 8),
        (STATE, BASE + 0x357f418),
        (TEMPLATE, BASE + STATE_TEMPLATE + 8),
    ] {
        let mut f = fixture();
        f.word(field, value);
        assert!(snapshot(&f, Some(1)).is_err(), "field {field:x}");
    }
    let mut f = fixture();
    f.int(WINDOW + 0x1390, 3);
    assert!(snapshot(&f, Some(1)).is_err());
    let mut f = fixture();
    f.int(STATE + 0x40, 2);
    assert!(snapshot(&f, Some(1)).is_err());
}

#[test]
fn final_witness_and_exact_process_check_cannot_be_skipped() {
    let mut f = fixture();
    f.tear(FIRST_ROW + 0x468, 4, 2);
    assert_eq!(snapshot(&f, Some(1)).unwrap_err().code, "torn_read");
    let mut f = fixture();
    f.swap_final_identity = true;
    assert!(snapshot(&f, Some(1)).is_err());
    let mut f = fixture();
    f.fail_final = true;
    assert!(snapshot(&f, Some(1)).is_err());
}

fn root(f: &mut Fixture, object: u64, registry: u64, flags: u8, transitions: [i32; 2]) {
    f.word(object, BASE + 0x36ba978);
    f.word(object + 0x88, registry);
    f.put(object + 0x95, &[flags]);
    f.int(object + 0x13d8, transitions[0]);
    f.int(object + 0x1390, transitions[1]);
}

#[test]
fn typed_observation_retains_exact_scope_and_own_state_without_focus_payload() {
    let f = fixture();
    let observation = observe(&f, Some(1)).unwrap();
    let binding = observation.binding();
    assert_eq!(binding.process(), &f.id);
    assert_eq!(binding.base(), BASE);
    assert_eq!(binding.own_tag(), 1);
    assert_eq!(binding.country(), COUNTRY);
    assert_eq!(binding.production(), PRODUCTION);
    assert_eq!(binding.registry(), REGISTRY);
    assert_eq!(binding.manager(), MANAGER);
    assert_eq!(binding.find(), FIND_OBJECT);
    assert_eq!(binding.camera(), CAMERA);
    let own = binding.own_state().unwrap();
    assert_eq!(own.state_id(), 1);
    assert_eq!(own.state(), STATE);
    assert_eq!(own.template(), TEMPLATE);
    assert_eq!(own.map_position(), [3120, 1480]);
    assert_eq!(observation.context().anchors[..2], [GS, IDLER]);
    assert_eq!(observation.context().player, "GER");
    assert_eq!(observation.query(), Some("1"));
    assert_eq!(observation.result_count(), Some(1));
    assert_eq!(observation.first_result().unwrap().kind(), 2);
    assert_eq!(observation.first_result().unwrap().state_id(), Some(1));
    assert!(observation.first_row_is_requested_state());
    assert_eq!(observation.camera().eye(), [3120.0, 300.0, 1390.0]);
    assert_eq!(observation.camera().look_at(), [3120.0, 9.5, 1480.0]);
    assert_eq!(
        observation.root(),
        &RootLifecycle::Materialized {
            pointer: WINDOW,
            object: WINDOW,
            flags: 8,
            transitions: [0, 0],
        }
    );
    let anchor = observation.anchor().unwrap();
    assert_eq!(anchor.binding(), binding);
    assert_eq!(anchor.context(), observation.context());
    anchor.require_same_scope(&observation).unwrap();
    for address in [FIND_OBJECT + 0xd60, REGISTRY + 0x138, REGISTRY + 0x210] {
        assert!(f.untouched(address, 8));
    }
}

#[test]
fn retained_camera_coordinates_query_and_clock_controls_may_change() {
    let mut f = fixture();
    let anchor = observe(&f, Some(1)).unwrap().anchor().unwrap();
    f.set_date(DATE + 1);
    f.int(GS + 0x47c, 3);
    f.put(IDLER + 0x679, &[0]);
    f.string(FIND_OBJECT + 0xd68, "changed query");
    floats(
        &mut f,
        CAMERA + 0x698,
        &[1500.0, 400.0, 2500.0, 1501.0, 9.5, 2501.0],
    );
    let now = anchor.observe(&f).unwrap();
    anchor.require_same_scope(&now).unwrap();
    assert_eq!(now.context().date_raw, DATE + 1);
    assert_eq!(now.context().speed, 3);
    assert!(!now.context().paused);
    assert_eq!(anchor.context().date_raw, DATE);
    assert_eq!(now.query(), Some("changed query"));
    assert_eq!(now.camera().look_at(), [1501.0, 9.5, 2501.0]);
    assert_eq!(now.value().unwrap()["target"]["camera_centered"], false);
}

#[test]
fn retained_find_models_materialization_rebuild_hide_and_teardown() {
    let mut f = fixture();
    f.word(FIND_OBJECT + 0xd58, 0);
    let initial = observe(&f, Some(1)).unwrap();
    assert_eq!(initial.root(), &RootLifecycle::Unmaterialized);
    assert_eq!(initial.query(), None);
    assert_eq!(initial.result_count(), None);
    let anchor = initial.anchor().unwrap();
    f.word(FIND_OBJECT + 0xd58, WINDOW);
    let now = anchor.observe(&f).unwrap();
    assert!(now.root().visible());
    assert!(now.root().local_transitions_idle());
    assert_eq!(now.query(), Some("1"));

    let replacement = WINDOW + 0x20000;
    root(&mut f, replacement, REGISTRY, 8, [1, 0]);
    f.word(FIND_OBJECT + 0xd58, replacement);
    let now = anchor.observe(&f).unwrap();
    assert_eq!(
        now.root(),
        &RootLifecycle::Materialized {
            pointer: replacement,
            object: replacement,
            flags: 8,
            transitions: [1, 0],
        }
    );
    assert!(!now.root().local_transitions_idle());
    assert!(f.untouched(WINDOW, 0x1420));

    f.put(replacement + 0x95, &[0]);
    f.word(FIND_OBJECT + 0xda8, FOREIGN);
    let hidden = anchor.observe(&f).unwrap();
    assert!(!hidden.root().visible());
    assert_eq!(hidden.query(), None);
    assert_eq!(hidden.first_result(), None);
    assert!(f.untouched(FIND_OBJECT + 0xd68, 24));
    assert!(f.untouched(FIND_OBJECT + 0xda8, 8));
    f.word(FIND_OBJECT + 0xd58, 0);
    assert_eq!(
        anchor.observe(&f).unwrap().root(),
        &RootLifecycle::Unmaterialized
    );
    assert!(f.untouched(replacement, 0x1420));
}

#[test]
fn every_new_root_requires_exact_own_container_before_query_payload() {
    for bad_type in [false, true] {
        let mut f = fixture();
        let anchor = observe(&f, Some(1)).unwrap().anchor().unwrap();
        let replacement = WINDOW + 0x20000;
        root(
            &mut f,
            replacement,
            if bad_type { REGISTRY } else { FOREIGN },
            8,
            [0, 0],
        );
        if bad_type {
            f.word(replacement, BASE + 0x36ba980);
        }
        f.word(FIND_OBJECT + 0xd58, replacement);
        assert!(anchor.observe(&f).is_err());
        assert!(f.untouched(FIND_OBJECT + 0xd68, 24));
        assert!(f.untouched(FIND_OBJECT + 0xda8, 8));
        assert!(f.untouched(FOREIGN, 0x1000));
    }
}

#[test]
fn retained_edges_reject_replacement_before_foreign_object_payload() {
    let countries = fixture().get_word(GS + 0x2d8);
    let states = fixture().get_word(GS + 0x290);
    for field in [
        BASE + 0x372b040,
        BASE + 0x372afc0,
        BASE + 0x3706540,
        countries,
        COUNTRY + 0xd10,
        PRODUCTION + 0x2a8,
        IDLER + 0x4c0,
        IDLER + 0x670,
        MANAGER + 0x180,
        MANAGER + 0x190,
        FIND_OBJECT + 0xd50,
        IDLER + 0x718,
        states + 8,
        STATE + 0x20,
    ] {
        let mut f = fixture();
        let anchor = observe(&f, Some(1)).unwrap().anchor().unwrap();
        f.word(field, FOREIGN);
        assert_eq!(
            anchor.observe(&f).unwrap_err().code,
            "construction_state_changed",
            "{field:x}"
        );
        assert!(f.untouched(FOREIGN, 0x1420), "{field:x}");
        assert!(f.untouched(FIND_OBJECT + 0xd68, 24), "{field:x}");
    }
}

#[test]
fn retained_ownership_loss_refuses_before_state_template_and_view_payload() {
    for field in [STATE + 0xc0, STATE + 0xc4] {
        let mut f = fixture();
        let anchor = observe(&f, Some(1)).unwrap().anchor().unwrap();
        f.int(field, 7);
        assert_eq!(
            anchor.observe(&f).unwrap_err().code,
            "construction_target_unavailable"
        );
        assert!(f.untouched(STATE + 0x20, 8));
        assert!(f.untouched(TEMPLATE, 0x128));
        assert!(f.untouched(FIND_OBJECT + 0xd68, 24));
    }
}

#[test]
fn same_id_state_or_same_position_template_replacement_is_not_adopted() {
    for replace_state in [false, true] {
        let mut f = fixture();
        let anchor = observe(&f, Some(1)).unwrap().anchor().unwrap();
        if replace_state {
            let replacement = STATE + 0x20000;
            f.word(replacement, BASE + 0x357f410);
            f.int(replacement + 0x40, 1);
            f.int(replacement + 0xc0, 1);
            f.int(replacement + 0xc4, 1);
            f.word(replacement + 0x20, TEMPLATE);
            f.array(GS + 0x290, &[0, replacement]);
            assert_eq!(
                anchor.observe(&f).unwrap_err().code,
                "construction_state_changed"
            );
            assert!(f.untouched(replacement, 0x128));
        } else {
            let replacement = TEMPLATE + 0x20000;
            f.word(replacement, BASE + STATE_TEMPLATE);
            f.int(replacement + 0x120, 3120);
            f.int(replacement + 0x124, 1480);
            f.word(STATE + 0x20, replacement);
            assert_eq!(
                anchor.observe(&f).unwrap_err().code,
                "construction_state_changed"
            );
            assert!(f.untouched(replacement, 0x128));
        }
    }
}

#[test]
fn retained_map_anchor_and_state_id_are_exact_even_with_same_pointers() {
    for (field, value) in [
        (TEMPLATE + 0x120, 3121),
        (TEMPLATE + 0x124, 1481),
        (STATE + 0x40, 2),
    ] {
        let mut f = fixture();
        let anchor = observe(&f, Some(1)).unwrap().anchor().unwrap();
        f.int(field, value);
        assert_eq!(
            anchor.observe(&f).unwrap_err().code,
            "construction_state_changed"
        );
        assert!(f.untouched(FIND_OBJECT + 0xd68, 24));
    }
}

#[test]
fn retained_campaign_setup_player_playthrough_and_chronology_cannot_change() {
    for (field, value) in [
        (GS + 0x4d8, 2),
        (COUNTRY + 8, 2),
        (GS + 0x5d0, 1),
        (GS + 0x5d8, 0),
        (GS + 0x428, DATE - 1),
    ] {
        let mut f = fixture();
        let anchor = observe(&f, Some(1)).unwrap().anchor().unwrap();
        f.int(field, value);
        assert_eq!(
            anchor.observe(&f).unwrap_err().code,
            "construction_state_changed",
            "{field:x}"
        );
        assert!(f.untouched(FIND_OBJECT + 0xd68, 24));
    }
    let mut f = fixture();
    let anchor = observe(&f, Some(1)).unwrap().anchor().unwrap();
    f.set_playthrough("ccddccdd-1122-4344-8566-778899aabbcc");
    assert_eq!(
        anchor.observe(&f).unwrap_err().code,
        "construction_state_changed"
    );
    assert!(f.untouched(FIND_OBJECT + 0xd68, 24));
}

struct OtherBase<'a>(&'a Fixture);
impl ReadMemory for OtherBase<'_> {
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        self.0.read(address, size)
    }
    fn string_abi(&self) -> StringAbi {
        self.0.string_abi()
    }
}
impl Process for OtherBase<'_> {
    fn begin_snapshot(&self) -> Result<()> {
        self.0.begin_snapshot()
    }
    fn check(&self) -> Result<()> {
        self.0.check()
    }
    fn image_base(&self) -> u64 {
        BASE + 0x10000
    }
    fn identity(&self) -> Option<&crate::platform::Identity> {
        self.0.identity()
    }
}

#[test]
fn retained_process_and_base_refuse_before_any_memory_payload() {
    let mut f = fixture();
    let anchor = observe(&f, Some(1)).unwrap().anchor().unwrap();
    assert_eq!(
        anchor.observe(&OtherBase(&f)).unwrap_err().code,
        "ownership_changed"
    );
    assert!(f.read_counts().is_empty());
    f.id.start_ticks.push('9');
    assert_eq!(anchor.observe(&f).unwrap_err().code, "ownership_changed");
    assert!(f.read_counts().is_empty());
}

#[test]
fn foreign_requested_state_stays_public_metadata_but_cannot_be_retained_as_own() {
    let mut f = fixture();
    f.int(STATE + 0xc0, 7);
    let observation = observe(&f, Some(1)).unwrap();
    assert_eq!(
        observation.value().unwrap()["target"]["own_owned_and_controlled"],
        false
    );
    assert_eq!(
        observation.anchor().unwrap_err().code,
        "construction_target_unavailable"
    );
    assert!(observation.binding().own_state().is_none());
    assert!(!observation.first_row_is_requested_state());
    assert!(f.untouched(STATE + 0x20, 8));
    let anchor = observe(&f, None).unwrap().anchor().unwrap();
    let now = anchor.observe(&f).unwrap();
    anchor.require_same_scope(&now).unwrap();
    assert!(now.binding().own_state().is_none());
    assert!(now.value().unwrap()["target"].is_null());
}

#[test]
fn typed_empty_results_skip_retained_row_and_do_not_claim_a_target_result() {
    let mut f = fixture();
    let anchor = observe(&f, Some(1)).unwrap().anchor().unwrap();
    f.int(FIND_OBJECT + 0xdbc, 0);
    f.word(FIND_OBJECT + 0xda8, FOREIGN);
    let now = anchor.observe(&f).unwrap();
    assert_eq!(now.result_count(), Some(0));
    assert!(now.first_result().is_none());
    assert!(!now.first_row_is_requested_state());
    assert!(f.untouched(FIND_OBJECT + 0xda8, 8));
    assert!(f.untouched(FOREIGN, 0x1000));
}

#[test]
fn retained_observation_requires_final_window_camera_and_edge_witnesses() {
    for (field, size, nth) in [
        (FIND_OBJECT + 0xd58, 8, 2),
        (CAMERA + 0x698, 24, 2),
        (MANAGER + 0x190, 8, 2),
        (TEMPLATE + 0x120, 4, 3),
    ] {
        let mut f = fixture();
        let anchor = observe(&f, Some(1)).unwrap().anchor().unwrap();
        f.tear(field, size, nth);
        assert_eq!(
            anchor.observe(&f).unwrap_err().code,
            "torn_read",
            "{field:x}"
        );
    }
    for swap in [false, true] {
        let mut f = fixture();
        let anchor = observe(&f, Some(1)).unwrap().anchor().unwrap();
        f.swap_final_identity = swap;
        f.fail_final = !swap;
        assert!(anchor.observe(&f).is_err());
    }
}

#[test]
fn invalid_state_and_non_mac_abi_refuse_before_process_reads() {
    let f = fixture();
    for id in [-1, 0, 4096] {
        assert_eq!(observe(&f, Some(id)).unwrap_err().code, "invalid_arguments");
        assert_eq!(f.snapshots.get(), 0);
    }
    let mut f = fixture();
    f.abi = StringAbi::LibStdCxx;
    assert_eq!(observe(&f, None).unwrap_err().code, "unsupported_platform");
    assert_eq!(f.snapshots.get(), 0);
}

fn focus_fixture() -> Fixture {
    let mut f = fixture();
    let edit = 0x9a0000;
    let kind = 0x9b0000;
    for (at, value) in [
        (BASE + 0x37eda08, REGISTRY),
        (FIND_OBJECT + 0xd60, edit),
        (edit, BASE + 0x36bc6a0),
        (edit + 0x180, BASE + 0x36bc9e0),
        (edit + 0x58, REGISTRY),
        (edit + 0x50, kind),
        (kind, BASE + 0x36bca58),
        (REGISTRY + 0x138, edit),
        (REGISTRY + 0x210, edit + 0x180),
    ] {
        f.word(at, value);
    }
    f.put(edit + 0x64, &0x804u32.to_le_bytes());
    f.put(edit + 0x239, &[1]);
    f.string(kind + 0x28, "to_find_text");
    f.array(WINDOW + 0xe50, &[edit]);
    f
}

#[test]
fn default_find_retains_null_focus_and_skips_all_new_focus_payloads() {
    let f = focus_fixture();
    let value = snapshot(&f, Some(1)).unwrap();
    assert!(value["find_view"]["text_box_focused"].is_null());
    assert!(value["find_view"].get("focus").is_none());
    for field in [FIND_OBJECT + 0xd60, REGISTRY + 0x138, REGISTRY + 0x210] {
        assert!(f.untouched(field, 8));
    }
}

#[test]
fn optional_focus_renders_the_same_witnessed_exact_box_and_aliases() {
    let f = focus_fixture();
    let value = snapshot_with_focus(&f, Some(1), true).unwrap();
    assert_eq!(value["find_view"]["text_box_focused"], true);
    assert_eq!(value["find_view"]["focus"]["exact_named_edit_box"], true);
    assert_eq!(value["find_view"]["focus"]["input_authorized"], false);
    assert_eq!(value["current_frame_verified"], false);
    assert_eq!(f.snapshots.get(), 1);
}

#[test]
fn optional_focus_hidden_view_skips_stale_edit_and_aliases() {
    let mut f = focus_fixture();
    f.put(WINDOW + 0x95, &[0]);
    f.word(FIND_OBJECT + 0xd60, 0xdead0000);
    let value = snapshot_with_focus(&f, Some(1), true).unwrap();
    assert!(value["find_view"]["focus"].is_null());
    assert!(value["find_view"]["text_box_focused"].is_null());
    assert!(f.untouched(FIND_OBJECT + 0xd60, 8));
    assert!(f.untouched(REGISTRY + 0x210, 8));
}

#[test]
fn optional_focus_alias_changes_are_torn_inside_the_same_witness() {
    for field in [REGISTRY + 0x138, REGISTRY + 0x210] {
        let mut f = focus_fixture();
        f.tear(field, 8, 2);
        assert_eq!(
            snapshot_with_focus(&f, Some(1), true).unwrap_err().code,
            "torn_read"
        );
    }
}

#[test]
fn retained_focus_read_refuses_replaced_find_before_its_edit_payload() {
    let mut f = focus_fixture();
    let anchor = observe(&f, Some(1)).unwrap().anchor().unwrap();
    f.word(MANAGER + 0x190, 0xdead0000);
    assert_eq!(
        anchor.observe_with_focus(&f).unwrap_err().code,
        "construction_state_changed"
    );
    assert!(f.untouched(0xdead0000, 8));
    assert!(f.untouched(FIND_OBJECT + 0xd60, 8));
}
