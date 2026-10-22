use super::{fixture::*, *};
use crate::memory::StringAbi;

fn abis() -> [StringAbi; 2] {
    [StringAbi::LibCxx, StringAbi::LibStdCxx]
}

#[test]
fn owned_observation_is_one_coherent_portable_context_and_full_binding() {
    for abi in abis() {
        let f = Fixture::new(abi);
        let value = f.observe(FIRST).unwrap();
        assert_eq!(value.context(), &f.context());
        assert_eq!(value.own(), f.own());
        assert_eq!(value.active_key(), None);
        assert_eq!(value.completed(), &BTreeSet::from([DONE.into()]));
        let binding = value.binding();
        assert_eq!(binding.game(), &f.game());
        assert_eq!((binding.gs(), binding.idler()), (GS, IDLER));
        assert_eq!((binding.country(), binding.status()), (COUNTRY, STATUS));
        assert_eq!(
            (binding.database(), binding.tree(), binding.progress()),
            (DATABASE, TREE, PROGRESS)
        );
        assert_eq!(binding.target(), f.focus(FIRST));
        assert_eq!(
            binding.members(),
            &[f.focus(FIRST), f.focus(SECOND), f.focus(DONE)]
        );
        assert_eq!(
            binding.keys(),
            &BTreeMap::from([
                (FIRST.into(), f.focus(FIRST)),
                (SECOND.into(), f.focus(SECOND)),
                (DONE.into(), f.focus(DONE)),
            ])
        );
        assert_eq!(f.snapshots.get(), 1);
        assert_eq!(f.checks.get(), 2);
    }
}

#[test]
fn active_busy_and_completed_target_are_observations_not_readiness_decisions() {
    for abi in abis() {
        let mut f = Fixture::new(abi);
        for active in [Some(FIRST), Some(SECOND), None] {
            f.set_active(active);
            let value = f.observe(FIRST).unwrap();
            assert_eq!(value.active_key(), active);
        }
        f.set_completed(&[FIRST, DONE, FIRST]);
        let value = f.observe(FIRST).unwrap();
        assert!(value.completed().contains(FIRST));
        assert_eq!(value.completed().len(), 2);
    }
}

#[test]
fn complete_membership_is_not_truncated_to_a_public_page() {
    for abi in abis() {
        let mut f = Fixture::new(abi);
        let keys: Vec<_> = (0..130).map(|i| format!("GER_done_{i:03}")).collect();
        for key in &keys {
            f.add_focus(key);
        }
        f.set_completed(&keys.iter().map(String::as_str).collect::<Vec<_>>());
        let value = f.observe(FIRST).unwrap();
        assert_eq!(value.completed().len(), 130);
        assert_eq!(value.binding().members().len(), 133);
        assert!(value.completed().contains("GER_done_129"));
    }
}

#[test]
fn expected_identity_roots_map_and_status_refuse_before_foreign_payload() {
    for abi in abis() {
        let f = Fixture::new(abi);
        let mut other = f.game();
        other.start_ticks.push('1');
        assert_eq!(
            snapshot(&f, FIRST, &other, &f.context(), f.own())
                .unwrap_err()
                .code,
            "ownership_changed"
        );
        assert_eq!(f.snapshots.get(), 0);
        for variant in 0..5 {
            let mut f = Fixture::new(abi);
            let g = f.game_layout();
            match variant {
                0 => f.word(BASE + g.gs_global, FOREIGN),
                1 => f.word(BASE + g.idler_global, FOREIGN),
                2 => f.word(BASE + g.technology_db_global, FOREIGN),
                3 => f.word(f.get_word(GS + g.countries), FOREIGN),
                _ => f.word(COUNTRY + g.country_technology_status, FOREIGN),
            }
            assert_eq!(f.observe(FIRST).unwrap_err().code, "ownership_changed");
            assert!(f.untouched(FOREIGN, 0x10000));
            assert!(f.untouched(f.focus(FIRST) + 0x18, 24));
        }
    }
}

#[test]
fn positive_own_tag_and_reciprocal_status_progress_and_focus_guards_precede_payload() {
    for abi in abis() {
        for variant in 0..5 {
            let mut f = Fixture::new(abi);
            let l = f.focus_layout();
            let (address, value) = match variant {
                0 => (COUNTRY + 8, 2),
                1 => (STATUS + 0x130, FOREIGN),
                2 => (STATUS + 0x138, 2),
                3 => (PROGRESS + 8, FOREIGN),
                _ => (f.focus(FIRST) + l.focus_tree, FOREIGN),
            };
            if matches!(variant, 0 | 2) {
                f.int(address, value as i32);
            } else {
                f.word(address, value);
            }
            assert_eq!(f.observe(FIRST).unwrap_err().code, "invalid_focus_owner");
            assert!(f.untouched(FOREIGN, 0x10000));
            assert!(f.untouched(f.focus(FIRST) + 0x18, 24));
            if variant <= 2 {
                assert!(f.untouched(STATUS + 0xa0, 16));
            }
        }
    }
}

#[test]
fn duplicate_unselected_keys_and_invalid_unreturned_members_cannot_hide_behind_target() {
    for abi in abis() {
        let mut f = Fixture::new(abi);
        f.string(f.focus(SECOND) + 0x18, FIRST);
        assert_eq!(f.observe(FIRST).unwrap_err().code, "invalid_focus_catalog");
        let mut f = Fixture::new(abi);
        f.word(f.focus(SECOND) + f.focus_layout().focus_tree, FOREIGN);
        assert_eq!(f.observe(FIRST).unwrap_err().code, "invalid_focus_owner");
        assert!(f.untouched(f.focus(SECOND) + 0x18, 24));
    }
}

#[test]
fn missing_target_foreign_membership_and_duplicate_member_pointers_refuse() {
    for abi in abis() {
        let f = Fixture::new(abi);
        assert_eq!(f.observe("GER_missing").unwrap_err().code, "invalid_target");
        for variant in 0..4 {
            let mut f = Fixture::new(abi);
            match variant {
                0 => f.array(PROGRESS + 0x40, &[FOREIGN]),
                1 => f.word(PROGRESS + 0x10, FOREIGN),
                2 => f.array(
                    TREE + f.focus_layout().tree_focuses,
                    &[f.focus(FIRST), f.focus(FIRST)],
                ),
                _ => f.array(DATABASE + 0x58, &[f.focus(SECOND), f.focus(DONE)]),
            }
            assert!(f.observe(FIRST).is_err());
            assert!(f.untouched(FOREIGN, 0x10000));
        }
    }
}

#[test]
fn a_foreign_registry_target_is_never_resolved_or_read_as_an_own_target() {
    for abi in abis() {
        let mut f = Fixture::new(abi);
        f.array(
            DATABASE + 0x58,
            &[f.focus(FIRST), f.focus(SECOND), f.focus(DONE), FOREIGN],
        );
        f.observe(FIRST).unwrap();
        assert!(f.untouched(FOREIGN, 0x10000));
        assert_eq!(f.observe("GER_foreign").unwrap_err().code, "invalid_target");
        assert!(f.untouched(FOREIGN, 0x10000));
    }
}

#[test]
fn oversized_own_registry_and_completed_collections_refuse_before_data_reads() {
    for abi in abis() {
        for (field, cap) in [
            (TREE + Fixture::new(abi).focus_layout().tree_focuses, 4097),
            (DATABASE + 0x58, 20001),
            (PROGRESS + 0x40, 4097),
        ] {
            let mut f = Fixture::new(abi);
            f.word(field, FOREIGN);
            f.int(field + 8, cap);
            f.int(field + 12, cap);
            assert_eq!(f.observe(FIRST).unwrap_err().code, "invalid_array");
            assert!(f.untouched(FOREIGN, 0x10000));
        }
    }
}

#[test]
fn same_key_replacement_changes_immutable_binding_even_when_public_progress_matches() {
    for abi in abis() {
        let mut f = Fixture::new(abi);
        let before = f.observe(FIRST).unwrap();
        let replacement = f.replace_focus(FIRST);
        let after = f.observe(FIRST).unwrap();
        assert_eq!(before.active_key(), after.active_key());
        assert_eq!(before.completed(), after.completed());
        assert_ne!(before.binding(), after.binding());
        assert_ne!(before.binding().target(), replacement);
        assert_eq!(after.binding().target(), replacement);
        assert_eq!(after.binding().keys()[FIRST], replacement);
    }
}

#[test]
fn ordered_member_and_non_target_key_changes_are_retained_in_binding() {
    for abi in abis() {
        let mut f = Fixture::new(abi);
        let before = f.observe(FIRST).unwrap();
        f.array(
            TREE + f.focus_layout().tree_focuses,
            &[f.focus(SECOND), f.focus(FIRST), f.focus(DONE)],
        );
        let reordered = f.observe(FIRST).unwrap();
        assert_eq!(before.binding().keys(), reordered.binding().keys());
        assert_ne!(before.binding(), reordered.binding());
        f.string(f.focus(SECOND) + 0x18, "GER_other");
        let renamed = f.observe(FIRST).unwrap();
        assert_eq!(reordered.binding().members(), renamed.binding().members());
        assert_ne!(reordered.binding(), renamed.binding());
    }
}

#[test]
fn coherent_context_mismatch_and_regression_refuse_but_time_controls_may_advance() {
    for abi in abis() {
        for variant in 0..6 {
            let mut f = Fixture::new(abi);
            let old = f.context();
            match variant {
                0 => f.set_playthrough("bbccddee-2233-4455-8677-8899aabbccdd"),
                1 => f.int(GS + f.game_layout().date, DATE - 1),
                2 => {
                    let a = if abi == StringAbi::LibCxx {
                        0x5d0
                    } else {
                        0x620
                    };
                    f.int(GS + a, 1);
                }
                3 => {
                    let a = if abi == StringAbi::LibCxx {
                        0x5d8
                    } else {
                        0x628
                    };
                    f.int(GS + a, 0);
                }
                4 => {
                    let tags = f.get_word(TAG_DATABASE);
                    f.string(tags + f.game_layout().tag_stride, "ENG");
                }
                _ => f.word(BASE + f.game_layout().technology_db_global, FOREIGN),
            }
            assert_eq!(
                snapshot(&f, FIRST, &f.game(), &old, f.own())
                    .unwrap_err()
                    .code,
                "ownership_changed"
            );
        }
        let mut f = Fixture::new(abi);
        let old = f.context();
        f.set_date(DATE + 1);
        let (speed, paused) = if abi == StringAbi::LibCxx {
            (0x47c, 0x679)
        } else {
            (0x4b4, 0x6c1)
        };
        f.int(GS + speed, 3);
        f.put(IDLER + paused, &[0]);
        let value = snapshot(&f, FIRST, &f.game(), &old, f.own()).unwrap();
        assert_eq!(value.context().date_raw, DATE + 1);
        assert_eq!((value.context().paused, value.context().speed), (false, 3));
    }
}

#[test]
fn final_witnesses_cover_full_scope_catalog_membership_and_clock() {
    for abi in abis() {
        let f = Fixture::new(abi);
        f.observe(FIRST).unwrap();
        let g = f.game_layout();
        let l = f.focus_layout();
        let counts = f.read_counts();
        let (playthrough, speed, paused) = if abi == StringAbi::LibCxx {
            (GS + 0x6c8, GS + 0x47c, IDLER + 0x679)
        } else {
            (GS + 0x718, GS + 0x4b4, IDLER + 0x6c1)
        };
        let string_size = if abi == StringAbi::LibCxx { 24 } else { 32 };
        let playthrough_data =
            f.get_word(playthrough + if abi == StringAbi::LibCxx { 16 } else { 0 });
        for (address, size) in [
            (BASE + g.gs_global, 8),
            (BASE + g.idler_global, 8),
            (BASE + g.technology_db_global, 8),
            (COUNTRY + g.country_technology_status, 8),
            (STATUS + 0x130, 8),
            (STATUS + 0x138, 4),
            (STATUS, 8),
            (COUNTRY + g.country_focus_tree, 8),
            (COUNTRY + g.country_focus_progress, 8),
            (BASE + l.database_global, 8),
            (TREE + l.tree_focuses, 16),
            (DATABASE + 0x58, 16),
            (PROGRESS + 0x40, 16),
            (PROGRESS + 0x10, 8),
            (f.focus(SECOND) + l.focus_tree, 8),
            (
                f.focus(SECOND) + 0x18,
                if abi == StringAbi::LibCxx { 24 } else { 32 },
            ),
            (GS + g.date, 4),
            (playthrough, string_size),
            (playthrough_data, 36),
            (speed, 4),
            (paused, 1),
        ] {
            let mut torn = Fixture::new(abi);
            torn.tear(address, size, counts[&(address, size)]);
            assert_eq!(
                torn.observe(FIRST).unwrap_err().code,
                "torn_read",
                "{abi:?} field {address:x}/{size}"
            );
        }
    }
}

#[test]
fn final_process_ownership_and_exact_identity_are_checked_after_memory_witnesses() {
    for abi in abis() {
        let mut f = Fixture::new(abi);
        f.fail_final = true;
        assert_eq!(f.observe(FIRST).unwrap_err().code, "ownership_changed");
        let mut f = Fixture::new(abi);
        f.swap_final_identity = true;
        assert_eq!(f.observe(FIRST).unwrap_err().code, "ownership_changed");
    }
}
