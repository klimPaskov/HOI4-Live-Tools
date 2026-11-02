//! Admission uses the real persisted governor with generated owner evidence.
//! No native transport, process or game is opened by these fixtures.
use super::*;
use crate::{actions::Epoch, platform::Identity};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Temp(PathBuf);
impl Temp {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let directory = std::env::temp_dir().join(format!(
            "hoi4-daily-one-admission-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        Self(directory)
    }
    fn path(&self) -> PathBuf {
        self.0.join("daily-plan.json")
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn plan() -> Plan {
    Plan {
        version: 1,
        revision: 1,
        scope: Scope {
            epoch: Epoch {
                game: Identity {
                    pid: 123,
                    start_ticks: "456".into(),
                    exe: "/generated/hoi4".into(),
                    uid: 1000,
                },
                campaign: "generated-controller:generated-playthrough".into(),
                player: "GER".into(),
            },
            anchors: [0x10000, 0x20000, 0x30000],
            own_country: 0x40000,
            own_status: 0x50000,
        },
        entries: [
            (
                "focus",
                Intent::Focus {
                    focus_id: "GER_rhineland".into(),
                },
            ),
            (
                "research",
                Intent::Research {
                    technology_id: "support_weapons".into(),
                    slot: 0,
                },
            ),
            (
                "production",
                Intent::Economy {
                    target: EconomyTarget::Production {
                        line_id: "0x0000000100000038".into(),
                        equipment_id: "infantry_equipment_1".into(),
                        total_factories: 11,
                    },
                },
            ),
        ]
        .into_iter()
        .map(|(id, intent)| Entry {
            id: id.into(),
            intent,
            navigation: None,
            focus_search: None,
        })
        .collect(),
        max_actions_per_day: 3,
        max_observation_age_ms: 5000,
        review_day: None,
    }
}

#[derive(Clone, Copy)]
enum Reply {
    Verified,
    NotQueued,
    Uncertain,
    Error,
    Late,
    ForeignReceipt,
}
struct Generated {
    observation: Observation,
    readiness: BTreeMap<String, Readiness>,
    readiness_error: Option<&'static str>,
    checked: Vec<String>,
    attempted: Vec<String>,
    deadlines: Vec<Instant>,
    reply: Reply,
    path: PathBuf,
}
impl Generated {
    fn new(plan: &Plan, temp: &Temp) -> Self {
        Self {
            observation: Observation {
                scope: plan.scope.clone(),
                date_raw: crate::calendar::encode(1936, 1, 1).unwrap(),
                sequence: 1,
                observed_unix_ms: 10_000,
                source: "generated coherent own observation".into(),
                counts: OwnCounts::default(),
            },
            readiness: BTreeMap::new(),
            readiness_error: None,
            checked: Vec::new(),
            attempted: Vec::new(),
            deadlines: Vec::new(),
            reply: Reply::Verified,
            path: temp.path(),
        }
    }
    fn day(&self) -> i64 {
        (i64::from(self.observation.date_raw) - 43_800_000).div_euclid(24)
    }
}
impl Backend for Generated {
    fn unresolved_input(&mut self) -> Result<bool> {
        Ok(false)
    }
    fn observe(&mut self) -> Result<Observation> {
        self.observation.sequence += 1;
        Ok(self.observation.clone())
    }
    fn readiness(&mut self, entry: &Entry, _: &Observation) -> Result<Readiness> {
        self.checked.push(entry.id.clone());
        if let Some(code) = self.readiness_error {
            return Err(Error::new(code, "Generated invalid own target"));
        }
        Ok(self
            .readiness
            .get(&entry.id)
            .cloned()
            .unwrap_or(Readiness::Ready))
    }
    fn attempt(&mut self, attempt: &Attempt, _: &Observation, until: Instant) -> Result<Outcome> {
        // Exercise admission at the actual durability boundary, not a counter
        // that substitutes for the governor's persisted Intent.
        let persisted: Value = serde_json::from_slice(&fs::read(&self.path).unwrap()).unwrap();
        assert_eq!(persisted["entries"][&attempt.entry.id]["state"], "Intent");
        assert_eq!(
            persisted["journal"].as_array().unwrap().last().unwrap()["request_id"],
            attempt.request_id
        );
        self.attempted.push(attempt.entry.id.clone());
        self.deadlines.push(until);
        match self.reply {
            Reply::NotQueued => Ok(Outcome::NotQueued("Generated zero-input refusal".into())),
            Reply::Uncertain => Ok(Outcome::Uncertain("Generated possible input".into())),
            Reply::Error => Err(Error::new("transport_error", "Generated possible input")),
            _ => {
                let mut observed = self.observe()?;
                if matches!(self.reply, Reply::ForeignReceipt) {
                    observed.scope.own_country += 8;
                }
                if matches!(self.reply, Reply::Late) {
                    std::thread::sleep(
                        until.saturating_duration_since(Instant::now()) + Duration::from_millis(2),
                    );
                }
                Ok(Outcome::Verified(Box::new(VerifiedReceipt {
                    attempt: attempt.clone(),
                    current: true,
                    canonical_intent: attempt.entry.intent.clone(),
                    observed,
                })))
            }
        }
    }
}
fn next(governor: &mut Governor, generated: &mut Generated) -> Result<DailyResult> {
    governor.run_next(generated, 10_000, Instant::now() + Duration::from_secs(2))
}
fn entry(governor: &Governor, id: &str) -> Value {
    serde_json::to_value(governor.entry_state(id)).unwrap()
}

#[test]
fn two_ready_heads_admit_only_one_intent_and_leave_later_entries_untouched() {
    let temp = Temp::new();
    let plan = plan();
    let mut generated = Generated::new(&plan, &temp);
    let mut governor = Governor::open(&temp.path(), plan).unwrap();
    let later = [entry(&governor, "research"), entry(&governor, "production")];
    let until = Instant::now() + Duration::from_secs(2);
    let result = governor.run_next(&mut generated, 10_000, until).unwrap();
    assert_eq!(result.verified, ["focus"]);
    assert_eq!(result.retained, ["research", "production"]);
    assert_eq!(generated.checked, ["focus"]);
    assert_eq!(generated.attempted, ["focus"]);
    assert_eq!(generated.deadlines, [until]);
    assert_eq!(governor.state.journal.len(), 2);
    assert_eq!(governor.used_actions(generated.day()), 1);
    assert_eq!(entry(&governor, "research"), later[0]);
    assert_eq!(entry(&governor, "production"), later[1]);
}

#[test]
fn waiting_focus_cannot_starve_research_or_production_when_every_cycle_advances_day() {
    let temp = Temp::new();
    let plan = plan();
    let mut generated = Generated::new(&plan, &temp);
    generated
        .readiness
        .insert("focus".into(), Readiness::WaitingForFocus);
    let mut governor = Governor::open(&temp.path(), plan).unwrap();
    let first_day = generated.day();
    assert_eq!(
        next(&mut governor, &mut generated).unwrap().verified,
        ["research"]
    );
    generated.observation.date_raw += 24;
    assert_eq!(
        next(&mut governor, &mut generated).unwrap().verified,
        ["production"]
    );
    assert_eq!(
        generated.checked,
        ["focus", "research", "focus", "production"]
    );
    assert_eq!(generated.attempted, ["research", "production"]);
    assert_eq!(governor.used_actions(first_day), 1);
    assert_eq!(governor.used_actions(first_day + 1), 1);
    assert!(matches!(
        governor.entry_state("focus"),
        Some(EntryState::Pending {
            attempts: 0,
            reason: Some(_),
            reason_day: None
        })
    ));
}

#[test]
fn deferred_heads_do_not_consume_admission_but_blocking_decisions_remain_visible() {
    for ready in [
        Readiness::WaitingForFocus,
        Readiness::Deferred("Unknown eligibility".into()),
        Readiness::Unavailable("Unsupported adapter".into()),
        Readiness::BlockingDecision("Target assigned outside this attempt".into()),
    ] {
        let temp = Temp::new();
        let plan = plan();
        let mut generated = Generated::new(&plan, &temp);
        let blocking = matches!(ready, Readiness::BlockingDecision(_));
        generated.readiness.insert("focus".into(), ready);
        let mut governor = Governor::open(&temp.path(), plan).unwrap();
        let result = next(&mut governor, &mut generated).unwrap();
        assert_eq!(result.blocking_decision.is_some(), blocking);
        if blocking {
            assert!(generated.attempted.is_empty());
            assert_eq!(generated.checked, ["focus"]);
        } else {
            assert_eq!(generated.attempted, ["research"]);
        }
    }
}

#[test]
fn missing_or_foreign_focus_evidence_is_never_skipped_for_a_ready_later_entry() {
    for code in [
        "focus_unavailable",
        "foreign_focus_scope",
        "ownership_changed",
    ] {
        let temp = Temp::new();
        let plan = plan();
        let mut generated = Generated::new(&plan, &temp);
        generated.readiness_error = Some(code);
        let mut governor = Governor::open(&temp.path(), plan).unwrap();
        assert_eq!(next(&mut governor, &mut generated).unwrap_err().code, code);
        assert_eq!(generated.checked, ["focus"]);
        assert!(generated.attempted.is_empty());
        assert!(governor.state.journal.is_empty());
    }
}

#[test]
fn existing_same_day_continuation_can_use_remaining_budget_after_one_admission() {
    let temp = Temp::new();
    let plan = plan();
    let mut generated = Generated::new(&plan, &temp);
    let mut governor = Governor::open(&temp.path(), plan).unwrap();
    next(&mut governor, &mut generated).unwrap();
    let completed = entry(&governor, "focus");
    assert!(next(&mut governor, &mut generated)
        .unwrap()
        .verified
        .is_empty());
    assert_eq!(generated.attempted, ["focus"]);
    let continued = governor
        .continue_entry(
            &mut generated,
            "research",
            10_000,
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(continued.verified, ["research"]);
    assert_eq!(generated.attempted, ["focus", "research"]);
    assert_eq!(governor.used_actions(generated.day()), 2);
    assert!(governor.budget_available(generated.day()));
    assert_eq!(entry(&governor, "focus"), completed);
}

#[test]
fn not_queued_consumes_daily_budget_and_ends_this_batch_before_other_ready_entries() {
    let temp = Temp::new();
    let plan = plan();
    let mut generated = Generated::new(&plan, &temp);
    generated.reply = Reply::NotQueued;
    let mut governor = Governor::open(&temp.path(), plan).unwrap();
    let untouched = entry(&governor, "research");
    next(&mut governor, &mut generated).unwrap();
    assert_eq!(generated.attempted, ["focus"]);
    assert_eq!(generated.checked, ["focus"]);
    assert_eq!(entry(&governor, "research"), untouched);
    assert_eq!(governor.used_actions(generated.day()), 1);
    // The one-admission limit must stop the batch even though two daily
    // reservations remain and the following research entry is ready.
    assert!(governor.budget_available(generated.day()));
    assert!(matches!(
        governor.entry_state("focus"),
        Some(EntryState::Pending {
            attempts: 1,
            reason: Some(_),
            reason_day: Some(_)
        })
    ));
}

#[test]
fn a_consumed_daily_budget_survives_restart_and_only_a_new_day_reopens_it() {
    let temp = Temp::new();
    let mut plan = plan();
    plan.max_actions_per_day = 1;
    let mut generated = Generated::new(&plan, &temp);
    generated.reply = Reply::NotQueued;
    let mut governor = Governor::open(&temp.path(), plan.clone()).unwrap();
    let untouched = entry(&governor, "research");
    next(&mut governor, &mut generated).unwrap();
    assert_eq!(generated.attempted, ["focus"]);
    assert_eq!(generated.checked, ["focus"]);
    assert_eq!(entry(&governor, "research"), untouched);
    assert_eq!(governor.used_actions(generated.day()), 1);
    assert!(!governor.budget_available(generated.day()));
    drop(governor);
    let mut reopened = Governor::open(&temp.path(), plan).unwrap();
    assert!(!reopened.budget_available(generated.day()));
    assert!(reopened
        .continue_entry(
            &mut generated,
            "research",
            10_000,
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap()
        .verified
        .is_empty());
    assert_eq!(generated.attempted, ["focus"]);
    generated.observation.date_raw += 24;
    generated.reply = Reply::Verified;
    assert_eq!(
        next(&mut reopened, &mut generated).unwrap().verified,
        ["focus"]
    );
    assert_eq!(generated.attempted, ["focus", "focus"]);
    assert_eq!(reopened.used_actions(generated.day()), 1);
    assert_eq!(entry(&reopened, "research"), untouched);
}

#[test]
fn expired_batch_never_creates_intent_or_renews_its_deadline() {
    let temp = Temp::new();
    let plan = plan();
    let mut generated = Generated::new(&plan, &temp);
    let mut governor = Governor::open(&temp.path(), plan).unwrap();
    let original = entry(&governor, "focus");
    governor
        .run_next(&mut generated, 10_000, Instant::now())
        .unwrap();
    assert!(generated.attempted.is_empty());
    assert!(generated.checked.is_empty());
    assert!(governor.state.journal.is_empty());
    assert_eq!(governor.used_actions(generated.day()), 0);
    assert_eq!(entry(&governor, "focus"), original);
}

#[test]
fn late_foreign_and_uncertain_results_keep_quarantine_and_never_expose_another_entry() {
    for reply in [
        Reply::Late,
        Reply::ForeignReceipt,
        Reply::Uncertain,
        Reply::Error,
    ] {
        let temp = Temp::new();
        let plan = plan();
        let mut generated = Generated::new(&plan, &temp);
        generated.reply = reply;
        let mut governor = Governor::open(&temp.path(), plan.clone()).unwrap();
        assert!(governor
            .run_next(
                &mut generated,
                10_000,
                Instant::now() + Duration::from_secs(1),
            )
            .is_err());
        assert_eq!(generated.attempted, ["focus"]);
        assert!(governor.blocks_work());
        drop(governor);
        let mut reopened = Governor::open(&temp.path(), plan).unwrap();
        generated.reply = Reply::Verified;
        assert_eq!(
            next(&mut reopened, &mut generated).unwrap_err().code,
            "governor_quarantined"
        );
        assert_eq!(generated.attempted, ["focus"]);
    }
}

#[test]
fn legacy_run_day_still_admits_multiple_ready_domains_with_the_shared_original_bound() {
    let temp = Temp::new();
    let plan = plan();
    let mut generated = Generated::new(&plan, &temp);
    let mut governor = Governor::open(&temp.path(), plan).unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    let result = governor.run_day(&mut generated, 10_000, until).unwrap();
    assert_eq!(result.verified, ["focus", "research", "production"]);
    assert_eq!(generated.attempted, ["focus", "research", "production"]);
    assert_eq!(generated.deadlines, [until; 3]);
    assert_eq!(governor.used_actions(generated.day()), 3);
}
