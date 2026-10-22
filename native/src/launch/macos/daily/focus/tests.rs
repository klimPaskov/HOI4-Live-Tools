use super::*;
use crate::{
    actions::Epoch,
    daily_governor as core,
    memory::StringAbi,
    own_focus::assignment_fixture::{Fixture, DATE, DONE, FIRST, SECOND},
};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

fn scope_for(f: &Fixture) -> Scope {
    let context = f.context();
    Scope {
        epoch: Epoch {
            game: f.game(),
            campaign: format!("nonce:{}", context.playthrough_id),
            player: context.player,
        },
        anchors: context.anchors,
        own_country: f.own()[0],
        own_status: f.own()[1],
    }
}
fn entry(key: &str) -> Entry {
    Entry {
        id: key.into(),
        intent: Intent::Focus {
            focus_id: key.into(),
        },
        navigation: None,
        focus_search: None,
    }
}
fn observation(f: &Fixture, sequence: u64) -> Observation {
    Observation {
        scope: scope_for(f),
        date_raw: f.context().date_raw,
        sequence,
        observed_unix_ms: 1,
        source: "generated coherent Focus fixture".into(),
        counts: Default::default(),
    }
}
fn attempt(f: &Fixture) -> Attempt {
    Attempt {
        request_id: "current-focus".into(),
        entry: entry(FIRST),
        scope: scope_for(f),
        day: 0,
        before_sequence: 2,
    }
}
#[test]
fn only_current_idle_to_exact_active_transition_produces_receipt() {
    let mut f = Fixture::new(StringAbi::LibCxx);
    let a = attempt(&f);
    let observed = observation(&f, 2);
    let before = f.observe(FIRST).unwrap();
    f.set_active(Some(FIRST));
    f.set_date(DATE + 1);
    let after = f.observe(FIRST).unwrap();
    let receipt = complete(&a, &observed, before, after, observation(&f, 3)).unwrap();
    assert!(receipt.current);
    assert_eq!(receipt.canonical_intent, a.entry.intent);
}
#[test]
fn stale_wrong_replaced_completed_and_rewound_evidence_never_complete() {
    for variant in 0..13 {
        let mut f = Fixture::new(StringAbi::LibCxx);
        let a = attempt(&f);
        let mut observed = observation(&f, 2);
        if variant == 0 {
            f.set_active(Some(SECOND));
        }
        if variant == 1 {
            f.set_completed(&[DONE, FIRST]);
        }
        let before = f.observe(FIRST).unwrap();
        f.set_active(Some(FIRST));
        match variant {
            2 => f.set_active(None),
            3 => f.set_active(Some(SECOND)),
            4 => f.set_completed(&[DONE, FIRST]),
            5 => f.set_completed(&[]),
            6 => {
                f.replace_focus(FIRST);
            }
            7 => {
                f.replace_focus(SECOND);
            }
            8 => f.set_date(DATE - 1),
            9 => observed.date_raw += 1,
            10 => observed.sequence += 1,
            _ => (),
        }
        let after = f
            .observe(if variant == 11 { SECOND } else { FIRST })
            .unwrap();
        let mut published = observation(&f, 3);
        if variant == 12 {
            published.sequence = 2;
        }
        assert_eq!(
            complete(&a, &observed, before, after, published)
                .unwrap_err()
                .code,
            "receipt_mismatch",
            "variant {variant}"
        );
    }
}
#[test]
fn fresh_completion_is_required_before_next_idle_assignment() {
    let mut f = Fixture::new(StringAbi::LibCxx);
    let observed = observation(&f, 2);
    f.set_active(Some(FIRST));
    let busy = f.observe(SECOND).unwrap();
    assert!(matches!(
        readiness(&busy, &entry(SECOND), &observed, Some(FIRST)).unwrap(),
        Readiness::WaitingForFocus
    ));
    assert!(matches!(
        readiness(&f.observe(FIRST).unwrap(), &entry(FIRST), &observed, None).unwrap(),
        Readiness::BlockingDecision(_)
    ));
    f.set_active(None);
    assert!(matches!(
        readiness(
            &f.observe(SECOND).unwrap(),
            &entry(SECOND),
            &observed,
            Some(FIRST)
        )
        .unwrap(),
        Readiness::BlockingDecision(_)
    ));
    f.set_completed(&[DONE, FIRST]);
    assert!(matches!(
        readiness(
            &f.observe(SECOND).unwrap(),
            &entry(SECOND),
            &observed,
            Some(FIRST)
        )
        .unwrap(),
        Readiness::Ready
    ));
}

struct Actor {
    fixture: Fixture,
    sequence: u64,
    previous: Option<String>,
    path: PathBuf,
    inputs: usize,
}
impl core::Backend for Actor {
    fn unresolved_input(&mut self) -> Result<bool> {
        Ok(false)
    }
    fn observe(&mut self) -> Result<Observation> {
        self.sequence += 1;
        Ok(observation(&self.fixture, self.sequence))
    }
    fn readiness(&mut self, e: &Entry, o: &Observation) -> Result<Readiness> {
        readiness(
            &self.fixture.observe(target(e)?)?,
            e,
            o,
            self.previous.as_deref(),
        )
    }
    fn attempt(&mut self, a: &Attempt, o: &Observation, _: Instant) -> Result<core::Outcome> {
        let persisted: serde_json::Value = serde_json::from_slice(&fs::read(&self.path)?).unwrap();
        assert_eq!(persisted["entries"][&a.entry.id]["state"], "Intent");
        let key = target(&a.entry)?;
        let before = self.fixture.observe(key)?;
        // Synthetic ordinary-input outcome. This test sends no real input.
        self.inputs += 1;
        self.fixture.set_active(Some(key));
        let after = self.fixture.observe(key)?;
        self.sequence += 1;
        let receipt = complete(
            a,
            o,
            before,
            after,
            observation(&self.fixture, self.sequence),
        )?;
        self.previous = Some(key.into());
        Ok(core::Outcome::Verified(Box::new(receipt)))
    }
}
#[test]
fn successive_queue_keeps_receipts_across_reopen_and_waits_without_poll_writes() {
    let directory = super::super::tests::dir();
    let path = directory.join("daily-plan.json");
    let f = Fixture::new(StringAbi::LibCxx);
    let plan = core::Plan {
        version: 1,
        revision: 1,
        scope: scope_for(&f),
        entries: vec![entry(FIRST), entry(SECOND)],
        max_actions_per_day: 2,
        max_observation_age_ms: 5000,
        review_day: None,
    };
    let mut actor = Actor {
        fixture: f,
        sequence: 0,
        previous: None,
        path: path.clone(),
        inputs: 0,
    };
    let mut governor = core::Governor::open(&path, plan.clone()).unwrap();
    governor
        .run_day(&mut actor, 1, Instant::now() + Duration::from_secs(10))
        .unwrap();
    assert_eq!(actor.inputs, 1);
    assert!(matches!(
        governor.entry_state(FIRST),
        Some(core::EntryState::Complete { .. })
    ));
    assert!(matches!(
        governor.entry_state(SECOND),
        Some(core::EntryState::Pending { attempts: 0, .. })
    ));
    drop(governor);
    let mut governor = core::Governor::open(&path, plan).unwrap();
    actor.previous = governor
        .completed_focus_hint()
        .unwrap()
        .map(|e| target(&e).unwrap().to_owned());
    assert_eq!(actor.previous.as_deref(), Some(FIRST));
    let bytes = fs::read(&path).unwrap();
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    for _ in 0..8 {
        let busy = actor.fixture.observe(SECOND).unwrap();
        assert!(matches!(
            readiness_at(
                &busy,
                &entry(SECOND),
                &scope_for(&actor.fixture),
                DATE,
                actor.previous.as_deref()
            )
            .unwrap(),
            Readiness::WaitingForFocus
        ));
        // Match worker scheduling: a busy probe never calls continue_entry.
    }
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    actor.fixture.set_active(None);
    actor.fixture.set_completed(&[DONE, FIRST]);
    actor.fixture.set_date(DATE + 35 * 24);
    governor
        .run_day(&mut actor, 1, Instant::now() + Duration::from_secs(10))
        .unwrap();
    assert_eq!(actor.inputs, 2);
    assert!(matches!(
        governor.entry_state(SECOND),
        Some(core::EntryState::Complete { .. })
    ));
    drop(governor);
    fs::remove_dir_all(directory).unwrap();
}
