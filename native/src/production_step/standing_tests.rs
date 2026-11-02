//! Use the real durable production journal with a generated ordinary backend.
//! These tests are not process/input/live-campaign evidence.
use super::*;
use crate::daily_governor::{
    Attempt, EconomyTarget, Entry, Intent, Observation, OwnCounts, Scope, VerifiedReceipt,
};

fn fixture(n: i32) -> (Attempt, Observation, crate::game::ClockContext) {
    let scope = Scope {
        epoch: epoch(),
        anchors: [0x70000, 0x80000, 0x90000],
        own_country: factory_state(n).line.country,
        own_status: 0xa0000,
    };
    let context = crate::game::ClockContext {
        playthrough_id: "fixture-playthrough".into(),
        player: "GER".into(),
        anchors: scope.anchors,
        date_raw: crate::calendar::encode(1936, 1, 1).unwrap(),
        difficulty: 2,
        historical_raw: 1,
        paused: false,
        speed: 3,
    };
    let attempt = Attempt {
        request_id: "production-standing-final".into(),
        entry: Entry {
            id: "factory-target".into(),
            intent: Intent::Economy {
                target: EconomyTarget::Production {
                    line_id: target(n).factory().unwrap().line_handle.clone(),
                    equipment_id: "infantry_equipment_1".into(),
                    total_factories: n as u16,
                },
            },
            navigation: None,
            focus_search: None,
        },
        scope: scope.clone(),
        day: 1936 * 365,
        before_sequence: 5,
    };
    let observed = Observation {
        scope,
        date_raw: context.date_raw,
        sequence: 6,
        observed_unix_ms: 1000,
        source: "generated witnessed own state".into(),
        counts: OwnCounts::default(),
    };
    (attempt, observed, context)
}

#[test]
fn fresh_allocation_and_noop_complete_without_exposing_private_proof() {
    for initial in [10, 11, 12] {
        let s = Scratch::new();
        let mut j = Journal::open(&s.path()).unwrap();
        let mut f = Fake::new(s.path(), initial);
        let (a, o, c) = fixture(11);
        let e = execute(&mut j, &mut f, &a.request_id, 11).unwrap();
        VerifiedReceipt::from_production(
            a,
            &e,
            &factory_state(initial).line,
            &f.current,
            &c,
            &c,
            o,
        )
        .unwrap();
        assert_eq!(f.dispatches, usize::from(initial != 11));
        let public = serde_json::to_value(&e).unwrap();
        assert_eq!(public.as_object().unwrap().len(), 10);
        for k in [
            "proof",
            "proof_historical",
            "epoch",
            "line",
            "before",
            "last",
        ] {
            assert!(public.get(k).is_none());
        }
    }
}

#[test]
fn historical_receipt_cannot_be_promoted_by_mutating_public_summary() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let mut f = Fake::new(s.path(), 10);
    let (a, o, c) = fixture(11);
    execute(&mut j, &mut f, &a.request_id, 11).unwrap();
    drop(j);
    let mut j = Journal::open(&s.path()).unwrap();
    let mut e = execute(&mut j, &mut f, &a.request_id, 11).unwrap();
    assert!(e.historical);
    e.historical = false;
    e.current_state_verified = true;
    assert!(
        VerifiedReceipt::from_production(a, &e, &f.current.line, &f.current, &c, &c, o,).is_err()
    );
    assert_eq!(f.dispatches, 1);
}

#[test]
fn production_completion_rejects_other_goal_scope_count_and_stale_evidence() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let mut f = Fake::new(s.path(), 10);
    let (a, o, c) = fixture(11);
    let e = execute(&mut j, &mut f, &a.request_id, 11).unwrap();
    for case in 0..15 {
        let (mut a, mut o, mut c) = (a.clone(), o.clone(), c.clone());
        let mut original = f.current.line.clone();
        let mut fresh = f.current.clone();
        match case {
            0 => a.request_id.push('x'),
            1 => {
                a.entry.intent = Intent::Research {
                    technology_id: "support_weapons".into(),
                    slot: 0,
                }
            }
            2 => {
                if let Intent::Economy {
                    target:
                        EconomyTarget::Production {
                            total_factories, ..
                        },
                } = &mut a.entry.intent
                {
                    *total_factories = 12;
                }
            }
            3 => {
                if let Intent::Economy {
                    target: EconomyTarget::Production { line_id, .. },
                } = &mut a.entry.intent
                {
                    *line_id = "0xffff000200000038".into();
                }
            }
            4 => {
                if let Intent::Economy {
                    target: EconomyTarget::Production { equipment_id, .. },
                } = &mut a.entry.intent
                {
                    *equipment_id = "support_equipment_1".into();
                }
            }
            5 => original.variant += 8,
            6 => fresh.line.line += 8,
            7 => fresh.requested = 10,
            8 => o.sequence = a.before_sequence,
            9 => o.scope.own_status += 8,
            10 => c.anchors[0] += 8,
            11 => c.playthrough_id.push('x'),
            12 => c.date_raw -= 1,
            13 => o.date_raw -= 1,
            14 => {
                a.scope.epoch.game.start_ticks = "457".into();
                o.scope = a.scope.clone();
            }
            _ => unreachable!(),
        }
        let before = fixture(11).2;
        assert!(
            VerifiedReceipt::from_production(a, &e, &original, &fresh, &before, &c, o).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn public_summary_cannot_retarget_private_production_proof() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let mut f = Fake::new(s.path(), 10);
    let (mut a, o, c) = fixture(11);
    let mut e = execute(&mut j, &mut f, &a.request_id, 11).unwrap();
    a.request_id = "different-request".into();
    e.request_id = a.request_id.clone();
    assert!(
        VerifiedReceipt::from_production(a, &e, &f.current.line, &f.current, &c, &c, o).is_err()
    );
}

#[test]
fn opening_receipt_never_completes_factory_allocation() {
    let s = Scratch::new();
    let mut j = Journal::open(&s.path()).unwrap();
    let mut f = PanelFake {
        state: panel_state(23, false),
        dispatches: 0,
        reads: 0,
        fail: false,
        uncertain: false,
        transitions: 0,
    };
    let (a, o, c) = fixture(11);
    let e = open(&mut j, &mut f, &a.request_id).unwrap();
    let fresh = factory_state(11);
    assert!(VerifiedReceipt::from_production(a, &e, &fresh.line, &fresh, &c, &c, o,).is_err());
}

#[test]
#[cfg(unix)]
fn real_journal_completion_survives_governor_restart_without_replay() {
    use crate::daily_governor::{self as core, Backend as _};
    use std::os::unix::fs::PermissionsExt;
    struct Owner {
        journal: Journal,
        factory: Fake,
        observation: Observation,
        context: crate::game::ClockContext,
    }
    impl core::Backend for Owner {
        fn unresolved_input(&mut self) -> Result<bool> {
            Ok(self.journal.blocks_game(&epoch().game))
        }
        fn observe(&mut self) -> Result<Observation> {
            self.observation.sequence += 1;
            Ok(self.observation.clone())
        }
        fn readiness(&mut self, _: &Entry, _: &Observation) -> Result<core::Readiness> {
            Ok(core::Readiness::Ready)
        }
        fn attempt(
            &mut self,
            a: &Attempt,
            _: &Observation,
            until: Instant,
        ) -> Result<core::Outcome> {
            let original = self.factory.current.line.clone();
            let e = self.journal.execute(
                &a.request_id,
                target(11),
                TOKEN,
                &authority(),
                &RwLock::new(true),
                &Domains(false),
                &mut self.factory,
                until.saturating_duration_since(Instant::now()),
            )?;
            let observed = self.observe()?;
            Ok(core::Outcome::Verified(Box::new(
                VerifiedReceipt::from_production(
                    a.clone(),
                    &e,
                    &original,
                    &self.factory.current,
                    &self.context,
                    &self.context,
                    observed,
                )?,
            )))
        }
    }
    let s = Scratch::new();
    fs::set_permissions(&s.0, fs::Permissions::from_mode(0o700)).unwrap();
    let (a, o, c) = fixture(11);
    let p = core::Plan {
        version: 1,
        revision: 1,
        scope: a.scope,
        entries: vec![a.entry],
        max_actions_per_day: 1,
        max_observation_age_ms: 5000,
        review_day: None,
    };
    let mut owner = Owner {
        journal: Journal::open(&s.path()).unwrap(),
        factory: Fake::new(s.path(), 10),
        observation: o,
        context: c,
    };
    let path = s.0.join("standing-plan.json");
    let mut g = core::Governor::open(&path, p.clone()).unwrap();
    assert_eq!(
        g.run_day(&mut owner, 1000, Instant::now() + TIME)
            .unwrap()
            .verified,
        vec!["factory-target"]
    );
    assert_eq!(g.used_actions(1936 * 365), 1);
    assert_eq!(owner.factory.dispatches, 1);
    drop(g);
    let mut g = core::Governor::open(&path, p).unwrap();
    assert!(g.pending_production(1936 * 365 + 1).is_empty());
    owner.observation.date_raw += 24;
    assert!(g
        .run_day(&mut owner, 1000, Instant::now() + TIME)
        .unwrap()
        .verified
        .is_empty());
    assert_eq!(owner.factory.dispatches, 1);
    assert!(!owner.unresolved_input().unwrap());
}
