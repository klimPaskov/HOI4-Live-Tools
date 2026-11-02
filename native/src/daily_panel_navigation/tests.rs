use super::*;
use crate::{
    actions::Epoch,
    daily_governor::{EconomyTarget, Intent},
    platform::Identity,
    research_stage::PanelBinding,
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    sync::atomic::{AtomicUsize, Ordering},
};
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "panel-selection-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
        Self(p)
    }
    fn file(&self) -> std::path::PathBuf {
        self.0.join("selection.json")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn scope() -> Scope {
    Scope {
        epoch: Epoch {
            game: Identity {
                pid: 123,
                start_ticks: "456".into(),
                exe: "/fixture/game".into(),
                uid: 1000,
            },
            campaign: "fixture-campaign".into(),
            player: "GER".into(),
        },
        anchors: [0x10000, 0x20000, 0x30000],
        own_country: 0x40000,
        own_status: 0x50000,
    }
}
fn research() -> Entry {
    Entry {
        id: "research".into(),
        intent: Intent::Research {
            technology_id: "basic_machine_tools".into(),
            slot: 0,
        },
        navigation: None,
        focus_search: None,
    }
}
fn production() -> Entry {
    Entry {
        id: "production".into(),
        intent: Intent::Economy {
            target: EconomyTarget::Production {
                line_id: "0x0000000100000038".into(),
                equipment_id: "infantry_equipment_1".into(),
                total_factories: 14,
            },
        },
        navigation: None,
        focus_search: None,
    }
}
fn plan() -> Plan {
    Plan {
        version: 1,
        revision: 1,
        scope: scope(),
        entries: vec![production(), research()],
        max_actions_per_day: 4,
        max_observation_age_ms: 1000,
        review_day: None,
    }
}
fn original() -> Destination {
    Destination::Research {
        state: OwnState {
            epoch: scope().epoch,
            selected_slot: None,
            selected_folder: None,
            tree_visible: false,
            panel_visible: Some(false),
            active_mode: Some(2),
            researching: vec![None; 4],
            visible_info: vec![],
            panel_binding: Some(PanelBinding {
                anchors: [0x40000, 0x50000, 0x60000, 0x70000, 0x80000, 0x90000],
                slots: vec![0xa0000, 0xb0000, 0xc0000, 0xd0000],
                roots: [0xe0000, 0xf0000, 0x100000, 0x110000],
                facilities_parent: 0x120000,
                selected_slot_pointer: 0,
                folder_index: 0,
                folder_windows: vec![],
                tab: 0,
            }),
        },
    }
}
fn date() -> i32 {
    crate::calendar::encode(1936, 1, 1).unwrap()
}
fn completed_research() -> Destination {
    let Destination::Research { mut state } = original() else {
        unreachable!()
    };
    state.researching[0] = Some("basic_machine_tools".into());
    state.active_mode = Some(5);
    state.panel_visible = Some(true);
    Destination::Research { state }
}
#[test]
fn omitted_policy_does_not_persist_or_enable_selection() {
    let t = Temp::new();
    let mut n = Navigator::open(&t.file()).unwrap();
    let mut selection_governor =
        crate::daily_governor::Governor::open(&t.0.join("selection-governor.json"), plan())
            .unwrap();
    n.configure(&plan(), None).unwrap();
    assert!(!t.file().exists());
    assert!(n
        .select(&mut selection_governor, &research(), original(), date())
        .is_err());
    assert!(n.active().is_none());
}
#[test]
fn original_destination_and_policy_survive_restart_and_cannot_be_stolen() {
    let t = Temp::new();
    let mut n = Navigator::open(&t.file()).unwrap();
    let mut selection_governor =
        crate::daily_governor::Governor::open(&t.0.join("selection-governor.json"), plan())
            .unwrap();
    n.configure(&plan(), Some(2)).unwrap();
    let id = n
        .select(&mut selection_governor, &research(), original(), date())
        .unwrap();
    assert_eq!(id.len(), 64);
    assert!(n
        .select(&mut selection_governor, &production(), original(), date())
        .is_err());
    drop(n);
    let mut n = Navigator::open(&t.file()).unwrap();
    assert_eq!(n.active(), Some(&research()));
    n.configure(&plan(), Some(2)).unwrap();
    for limit in [None, Some(1), Some(3)] {
        assert!(n.configure(&plan(), limit).is_err());
    }
    let mut next = plan();
    next.revision += 1;
    assert!(n.configure(&next, Some(2)).is_err());
    assert_eq!(n.active(), Some(&research()));
    let public = n.summary().to_string();
    assert!(!public.contains("anchors") && !public.contains("panel_binding"));
    assert_eq!(n.summary()["input_authorized"], false);
}
#[test]
fn equal_revision_reorder_or_entry_changes_are_rejected_before_persistence() {
    let t = Temp::new();
    let mut n = Navigator::open(&t.file()).unwrap();
    n.configure(&plan(), Some(2)).unwrap();
    let before = fs::read(t.file()).unwrap();
    let mut changed = plan();
    changed.entries.swap(0, 1);
    assert!(n.configure(&changed, Some(2)).is_err());
    changed = plan();
    changed.entries[1].id = "replacement".into();
    assert!(n.configure(&changed, Some(2)).is_err());
    assert_eq!(fs::read(t.file()).unwrap(), before);
}
#[test]
fn later_revision_only_when_idle_preserves_history() {
    let t = Temp::new();
    let mut n = Navigator::open(&t.file()).unwrap();
    n.configure(&plan(), Some(2)).unwrap();
    let mut next = plan();
    next.revision += 1;
    n.configure(&next, Some(3)).unwrap();
    drop(n);
    let n = Navigator::open(&t.file()).unwrap();
    assert_eq!(n.data.policies.len(), 2);
    assert_eq!(n.data.policies[0].limit, 2);
    assert_eq!(n.data.policies[1].limit, 3);
}
#[test]
fn missing_foreign_or_occupied_original_slot_never_selects_destination() {
    for mutation in 0..4 {
        let t = Temp::new();
        let mut n = Navigator::open(&t.file()).unwrap();
        let mut selection_governor =
            crate::daily_governor::Governor::open(&t.0.join("selection-governor.json"), plan())
                .unwrap();
        n.configure(&plan(), Some(2)).unwrap();
        let mut o = original();
        let Destination::Research { state } = &mut o else {
            unreachable!()
        };
        match mutation {
            0 => state.panel_binding = None,
            1 => state.panel_binding.as_mut().unwrap().anchors[0] += 8,
            2 => state.researching[0] = Some("support_weapons".into()),
            _ => state.panel_binding.as_mut().unwrap().slots[0] = 0,
        }
        assert!(n
            .select(&mut selection_governor, &research(), o, date())
            .is_err());
        assert!(n.active().is_none());
    }
}
#[test]
fn panel_preparation_or_pending_governor_cannot_complete_selected_goal() {
    let t = Temp::new();
    let mut n = Navigator::open(&t.file()).unwrap();
    let mut selection_governor =
        crate::daily_governor::Governor::open(&t.0.join("selection-governor.json"), plan())
            .unwrap();
    n.configure(&plan(), Some(2)).unwrap();
    n.select(&mut selection_governor, &research(), original(), date())
        .unwrap();
    let mut gov =
        crate::daily_governor::Governor::open(&t.0.join("governor.json"), plan()).unwrap();
    assert!(n.finish(&mut gov, &completed_research(), date()).is_err());
    assert_eq!(n.active(), Some(&research()));
}
#[test]
fn changed_private_file_quarantines_current_writer_and_invalid_history_refuses_reopen() {
    let t = Temp::new();
    let mut n = Navigator::open(&t.file()).unwrap();
    let mut selection_governor =
        crate::daily_governor::Governor::open(&t.0.join("selection-governor.json"), plan())
            .unwrap();
    n.configure(&plan(), Some(2)).unwrap();
    n.select(&mut selection_governor, &research(), original(), date())
        .unwrap();
    let mut data: Value = serde_json::from_slice(&fs::read(t.file()).unwrap()).unwrap();
    data["selections"][0]["entry"]["id"] = json!("changed");
    fs::write(t.file(), serde_json::to_vec(&data).unwrap()).unwrap();
    assert!(n.configure(&plan(), Some(2)).is_err());
    assert_eq!(n.summary()["storage_quarantined"], true);
    drop(n);
    assert!(Navigator::open(&t.file()).is_err());
}
#[test]
fn malformed_policy_never_changes_existing_valid_journal() {
    let t = Temp::new();
    let mut n = Navigator::open(&t.file()).unwrap();
    n.configure(&plan(), Some(2)).unwrap();
    let before = fs::read(t.file()).unwrap();
    let mut next = plan();
    next.revision += 1;
    for limit in [0, 17, 255] {
        assert!(n.configure(&next, Some(limit)).is_err());
        assert_eq!(fs::read(t.file()).unwrap(), before);
    }
}
struct Fixture(u64);
impl crate::daily_governor::Backend for Fixture {
    fn unresolved_input(&mut self) -> Result<bool> {
        Ok(false)
    }
    fn observe(&mut self) -> Result<crate::daily_governor::Observation> {
        self.0 += 1;
        Ok(crate::daily_governor::Observation {
            scope: scope(),
            date_raw: date(),
            sequence: self.0,
            observed_unix_ms: 1000,
            source: "generated-fixture".into(),
            counts: Default::default(),
        })
    }
    fn readiness(
        &mut self,
        _: &Entry,
        _: &crate::daily_governor::Observation,
    ) -> Result<crate::daily_governor::Readiness> {
        Ok(crate::daily_governor::Readiness::Ready)
    }
    fn attempt(
        &mut self,
        a: &crate::daily_governor::Attempt,
        _: &crate::daily_governor::Observation,
        _: std::time::Instant,
    ) -> Result<crate::daily_governor::Outcome> {
        let after = self.observe()?;
        Ok(crate::daily_governor::Outcome::Verified(Box::new(
            crate::daily_governor::VerifiedReceipt {
                attempt: a.clone(),
                current: true,
                canonical_intent: a.entry.intent.clone(),
                observed: after,
            },
        )))
    }
}
#[test]
fn selected_research_survives_competing_production_completion_until_its_own_goal_finishes() {
    let t = Temp::new();
    let mut n = Navigator::open(&t.file()).unwrap();
    let mut selection_governor =
        crate::daily_governor::Governor::open(&t.0.join("selection-governor.json"), plan())
            .unwrap();
    n.configure(&plan(), Some(2)).unwrap();
    n.select(&mut selection_governor, &research(), original(), date())
        .unwrap();
    let gp = t.0.join("governor.json");
    let mut gov = crate::daily_governor::Governor::open(&gp, plan()).unwrap();
    let mut fixture = Fixture(0);
    gov.continue_entry(
        &mut fixture,
        &production().id,
        1000,
        std::time::Instant::now() + std::time::Duration::from_secs(2),
    )
    .unwrap();
    assert!(n.finish(&mut gov, &completed_research(), date()).is_err());
    assert_eq!(n.active(), Some(&research()));
    gov.continue_entry(
        &mut fixture,
        &research().id,
        1000,
        std::time::Instant::now() + std::time::Duration::from_secs(2),
    )
    .unwrap();
    n.finish(&mut gov, &completed_research(), date()).unwrap();
    assert!(n.active().is_none());
    let before = fs::read(t.file()).unwrap();
    drop(n);
    let mut n = Navigator::open(&t.file()).unwrap();
    n.configure(&plan(), Some(2)).unwrap();
    assert!(n.active().is_none());
    assert_eq!(fs::read(t.file()).unwrap(), before);
    assert_eq!(n.summary()["selection_count"], 1);
}
#[test]
fn production_original_line_and_panel_are_bound_without_selected_count_noops() {
    let t = Temp::new();
    let mut n = Navigator::open(&t.file()).unwrap();
    let mut selection_governor =
        crate::daily_governor::Governor::open(&t.0.join("selection-governor.json"), plan())
            .unwrap();
    n.configure(&plan(), Some(2)).unwrap();
    let state = FactoryState {
        epoch: scope().epoch,
        line: crate::production_step::LineIdentity {
            country: scope().own_country,
            production: 0xb0000,
            line: 0x60000,
            variant: 0x70000,
            base_type: 0x80000,
            handle: 0x0000000100000038,
            equipment_id: "infantry_equipment_1".into(),
        },
        requested: 14,
    };
    let panel = PanelState {
        epoch: scope().epoch,
        panel_identity: vec![
            0x40000, 0xb0000, 0x20000, 0xc0000, 0xd0000, 0xe0000, 0xf0000,
        ],
        managed_mode: 5,
        panel_shown: false,
    };
    assert!(n
        .select(
            &mut selection_governor,
            &production(),
            Destination::Production {
                state: state.clone(),
                panel: panel.clone()
            },
            date()
        )
        .is_err());
    let mut wanted = state;
    wanted.requested = 10;
    n.select(
        &mut selection_governor,
        &production(),
        Destination::Production {
            state: wanted,
            panel,
        },
        date(),
    )
    .unwrap();
    drop(n);
    let n = Navigator::open(&t.file()).unwrap();
    assert_eq!(n.active(), Some(&production()));
}

mod departure;

#[test]
fn checked_original_and_static_continuation_never_rebind_after_restart() {
    let t = Temp::new();
    let mut governor =
        crate::daily_governor::Governor::open(&t.0.join("governor.json"), plan()).unwrap();
    let mut n = Navigator::open(&t.file()).unwrap();
    n.configure(&plan(), Some(2)).unwrap();
    assert!(n.original().unwrap().is_none());
    n.select(&mut governor, &research(), original(), date())
        .unwrap();
    drop(n);
    let mut n = Navigator::open(&t.file()).unwrap();
    assert_eq!(n.original().unwrap(), Some(original()));
    let Destination::Research { state } = original() else {
        unreachable!()
    };
    let binding = state.panel_binding.unwrap();
    n.verify_research_identity(
        &state.epoch,
        &binding.anchors,
        &binding.slots,
        &binding.roots,
        binding.facilities_parent,
        date(),
    )
    .unwrap();
    for mutation in 0..5 {
        let mut b = binding.clone();
        let mut at = date();
        match mutation {
            0 => b.anchors[0] += 8,
            1 => b.slots[0] += 8,
            2 => b.roots[0] += 8,
            3 => b.facilities_parent += 8,
            _ => at -= 1,
        }
        assert!(n
            .verify_research_identity(
                &state.epoch,
                &b.anchors,
                &b.slots,
                &b.roots,
                b.facilities_parent,
                at
            )
            .is_err());
    }
    fs::write(t.file(), b"{}").unwrap();
    assert!(n.original().is_err());
    assert!(n.blocked());
}
