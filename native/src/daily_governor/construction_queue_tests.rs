//! Core scheduling fixtures only. Fake is the existing trusted test backend,
//! never a runtime ConstructionQueue receipt converter or input implementation.
use super::*;

fn entry(id: &str, building: &str, state_id: u32, goal: u16) -> Entry {
    Entry {
        id: id.into(),
        intent: Intent::Economy {
            target: EconomyTarget::ConstructionQueue {
                building: building.into(),
                state_id,
                minimum_queued: goal,
            },
        },
        navigation: None,
        focus_search: None,
    }
}
fn queued(entries: Vec<Entry>) -> Plan {
    let mut p = plan();
    p.entries = entries;
    p
}
fn heads(g: &Governor, day: i64) -> Vec<String> {
    g.pending_construction(day)
        .into_iter()
        .map(|e| e.id)
        .collect()
}
fn day() -> i64 {
    1936 * 365
}
fn continuing(g: &mut Governor, b: &mut Fake, id: &str) -> Result<DailyResult> {
    g.continue_entry(b, id, 10000, Instant::now() + Duration::from_secs(1))
}

#[test]
fn construction_queue_heads_group_by_building_and_state_not_by_goal() {
    let t = Temp::new();
    let p = queued(vec![
        entry("a1", "arms_factory", 1, 1),
        entry("a2", "arms_factory", 1, 2),
        entry("b1", "arms_factory", 2, 1),
        entry("c1", "industrial_complex", 1, 1),
    ]);
    let g = Governor::open(&t.path(), p).unwrap();
    assert_eq!(heads(&g, day()), ["a1", "b1", "c1"]);
    assert!(g.pending_production(day()).is_empty());
    assert!(g.pending_research(day()).is_empty());
    assert!(g.pending_focus(day()).is_none());
}

#[test]
fn construction_queue_known_refusal_blocks_later_pair_and_survives_reopen() {
    let t = Temp::new();
    let p = queued(vec![
        entry("a1", "arms_factory", 1, 1),
        entry("a2", "arms_factory", 1, 2),
        entry("b1", "arms_factory", 2, 1),
    ]);
    let mut b = Fake::new(&p);
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    assert!(continuing(&mut g, &mut b, "a2")
        .unwrap()
        .verified
        .is_empty());
    assert_eq!(b.sends, 0);
    b.mode = Mode::NotQueued;
    continuing(&mut g, &mut b, "a1").unwrap();
    assert_eq!(heads(&g, day()), ["b1"]);
    assert_eq!(g.used_actions(day()), 1);
    drop(g);
    let g = Governor::open(&t.path(), p).unwrap();
    assert_eq!(heads(&g, day()), ["b1"]);
    assert_eq!(heads(&g, day() + 1), ["a1", "b1"]);
    assert_eq!(g.used_actions(day()), 1);
}

#[test]
fn construction_queue_completion_exposes_next_head_in_same_day_without_refilling_old() {
    let t = Temp::new();
    let p = queued(vec![
        entry("a1", "arms_factory", 1, 1),
        entry("a2", "arms_factory", 1, 2),
    ]);
    let mut b = Fake::new(&p);
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    assert_eq!(
        g.run_next(&mut b, 10000, Instant::now() + Duration::from_secs(1))
            .unwrap()
            .verified,
        ["a1"]
    );
    assert_eq!(heads(&g, day()), ["a2"]);
    assert_eq!(continuing(&mut g, &mut b, "a2").unwrap().verified, ["a2"]);
    assert_eq!(g.used_actions(day()), 2);
    assert!(heads(&g, day()).is_empty());
    let completed = g.entry_state("a1").cloned().unwrap();
    drop(g);
    let mut g = Governor::open(&t.path(), p).unwrap();
    b.day(2);
    assert!(run(&mut g, &mut b).unwrap().verified.is_empty());
    assert_eq!(b.sends, 2);
    assert!(heads(&g, day() + 1).is_empty());
    assert_eq!(g.entry_state("a1"), Some(&completed));
}

#[test]
fn construction_queue_complete_remains_complete_across_days_and_append_update() {
    let t = Temp::new();
    let p = queued(vec![entry("first", "arms_factory", 1, 1)]);
    let mut b = Fake::new(&p);
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    assert_eq!(run(&mut g, &mut b).unwrap().verified, ["first"]);
    let completed = g.entry_state("first").cloned().unwrap();
    let initial_sends = b.sends;
    // Core has no queued-amount observation/callback, so changed counts cannot
    // reopen a completed assignment or imply construction completion/refill.
    b.obs.counts.idle_civilian_factories = Some(100);
    b.day(2);
    run(&mut g, &mut b).unwrap();
    assert_eq!(b.sends, initial_sends);
    assert_eq!(g.entry_state("first"), Some(&completed));
    let mut updated = p;
    updated.revision += 1;
    updated.entries.push(entry("next", "arms_factory", 1, 2));
    g.update_plan(updated.clone()).unwrap();
    drop(g);
    let g = Governor::open(&t.path(), updated).unwrap();
    assert_eq!(g.entry_state("first"), Some(&completed));
    assert_eq!(heads(&g, day() + 1), ["next"]);
}

#[test]
fn construction_queue_update_requires_unchanged_pair_prefix_and_allows_cross_pair_moves() {
    let t = Temp::new();
    let a = entry("a1", "arms_factory", 1, 1);
    let a2 = entry("a2", "arms_factory", 1, 2);
    let b = entry("b1", "arms_factory", 2, 1);
    let p = queued(vec![a.clone(), a2.clone(), b.clone()]);
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    for entries in [
        vec![a2.clone(), a.clone(), b.clone()],
        vec![
            entry("inserted", "arms_factory", 1, 3),
            a.clone(),
            a2.clone(),
            b.clone(),
        ],
    ] {
        let mut next = p.clone();
        next.revision += 1;
        next.entries = entries;
        assert_eq!(g.update_plan(next).unwrap_err().code, "plan_conflict");
    }
    let mut next = p;
    next.revision += 1;
    next.entries = vec![b, a, a2, entry("tail", "arms_factory", 1, 3)];
    assert!(g.update_plan(next.clone()).unwrap());
    assert!(!g.update_plan(next).unwrap());
}

#[test]
fn construction_queue_update_cannot_reinterpret_legacy_or_retarget_existing_entry() {
    let t = Temp::new();
    let mut old = entry("old", "arms_factory", 1, 1);
    old.intent = Intent::Economy {
        target: EconomyTarget::Construction {
            building: "arms_factory".into(),
            state_id: 1,
            total_levels: 1,
        },
    };
    let p = queued(vec![old]);
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    let mut changed = p;
    changed.revision += 1;
    changed.entries[0] = entry("old", "arms_factory", 1, 1);
    assert_eq!(g.update_plan(changed).unwrap_err().code, "plan_conflict");
    let t2 = Temp::new();
    let p = queued(vec![entry("old", "arms_factory", 1, 1)]);
    let mut g = Governor::open(&t2.path(), p.clone()).unwrap();
    for replacement in [
        entry("old", "arms_factory", 1, 2),
        entry("old", "arms_factory", 2, 1),
        entry("old", "industrial_complex", 1, 1),
    ] {
        let mut next = p.clone();
        next.revision += 1;
        next.entries[0] = replacement;
        assert_eq!(g.update_plan(next).unwrap_err().code, "plan_conflict");
    }
}

#[test]
fn construction_queue_legacy_target_is_not_in_the_new_queue_group() {
    let t = Temp::new();
    let mut legacy = entry("built", "arms_factory", 1, 1);
    legacy.intent = Intent::Economy {
        target: EconomyTarget::Construction {
            building: "arms_factory".into(),
            state_id: 1,
            total_levels: 1,
        },
    };
    let g = Governor::open(
        &t.path(),
        queued(vec![legacy, entry("queued", "arms_factory", 1, 1)]),
    )
    .unwrap();
    assert_eq!(heads(&g, day()), ["queued"]);
}

#[test]
fn construction_queue_uncertain_head_quarantines_and_never_promotes_tail() {
    let t = Temp::new();
    let p = queued(vec![
        entry("a1", "arms_factory", 1, 1),
        entry("a2", "arms_factory", 1, 2),
    ]);
    let mut b = Fake::new(&p);
    b.mode = Mode::Uncertain;
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    assert_eq!(run(&mut g, &mut b).unwrap_err().code, "action_uncertain");
    assert!(g.blocks_work());
    assert!(heads(&g, day()).is_empty());
    drop(g);
    let mut g = Governor::open(&t.path(), p).unwrap();
    assert!(g.blocks_work());
    b.mode = Mode::Verified;
    b.day(2);
    assert!(continuing(&mut g, &mut b, "a2").is_err());
    assert_eq!(b.sends, 1);
}

#[test]
fn construction_queue_intent_duplicates_are_rejected_and_goal_changes_are_distinct() {
    let a = entry("a", "arms_factory", 1, 1);
    let mut same = a.clone();
    same.id = "b".into();
    assert_eq!(
        queued(vec![a.clone(), same]).validate().unwrap_err().code,
        "invalid_plan"
    );
    queued(vec![a, entry("b", "arms_factory", 1, 2)])
        .validate()
        .unwrap();
}

#[test]
fn construction_queue_pending_head_queries_do_not_persist_or_observe() {
    let t = Temp::new();
    let p = queued(vec![entry("a", "arms_factory", 1, 1)]);
    let g = Governor::open(&t.path(), p).unwrap();
    let before = fs::read(t.path()).unwrap();
    let stamp = fs::metadata(t.path()).unwrap().modified().unwrap();
    for _ in 0..20 {
        assert_eq!(heads(&g, day()), ["a"]);
        assert_eq!(heads(&g, day() + 1), ["a"]);
    }
    assert_eq!(fs::read(t.path()).unwrap(), before);
    assert_eq!(fs::metadata(t.path()).unwrap().modified().unwrap(), stamp);
    assert_eq!(g.last_sequence(), 0);
}

#[test]
fn construction_queue_completed_prefix_stays_immutable_on_update() {
    let t = Temp::new();
    let p = queued(vec![
        entry("a", "arms_factory", 1, 1),
        entry("b", "arms_factory", 1, 2),
    ]);
    let mut b = Fake::new(&p);
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    g.run_next(&mut b, 10000, Instant::now() + Duration::from_secs(1))
        .unwrap();
    let mut swapped = p;
    swapped.revision += 1;
    swapped.entries.swap(0, 1);
    assert_eq!(g.update_plan(swapped).unwrap_err().code, "plan_conflict");
}

#[test]
fn construction_queue_addition_preserves_exact_legacy_entry_bytes_and_request_identity() {
    let wire = r#"{"id":"legacy","intent":{"kind":"Economy","target":{"kind":"Construction","building":"arms_factory","state_id":1,"total_levels":4}}}"#;
    let legacy: Entry = serde_json::from_str(wire).unwrap();
    legacy.validate().unwrap();
    assert_eq!(serde_json::to_string(&legacy).unwrap(), wire);
    let p = queued(vec![legacy.clone()]);
    assert_eq!(
        request_id_day(&p, &legacy, 1, 1, day()).unwrap(),
        "599f08403ea8c49da49785c07e4138e440aff373eea428b2b6c089c849ae0b80"
    );
    let temp = Temp::new();
    let governor = Governor::open(&temp.path(), p.clone()).unwrap();
    assert!(governor.pending_construction(day()).is_empty());
    drop(governor);
    let reopened = Governor::open(&temp.path(), p).unwrap();
    assert!(matches!(
        reopened.plan().entries[0].intent,
        Intent::Economy {
            target: EconomyTarget::Construction {
                total_levels: 4,
                ..
            }
        }
    ));
}
