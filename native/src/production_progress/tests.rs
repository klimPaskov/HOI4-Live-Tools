use super::*;
use crate::{
    actions::Epoch,
    construction_stage::DispatchOutcome,
    daily_governor::{Observation, OwnCounts, VerifiedReceipt},
    input_lease::InputAuthority,
    platform::Identity,
    production_step::{self as ordinary, Backend, PanelState},
};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicUsize, Ordering},
        RwLock,
    },
    time::{Duration, Instant},
};
const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TIME: Duration = Duration::from_secs(2);
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        static N: AtomicUsize = AtomicUsize::new(0);
        let p = std::env::temp_dir().join(format!(
            "hoi4-production-progress-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&p).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
        Self(p)
    }
    fn progress(&self) -> std::path::PathBuf {
        self.0.join("progress.json")
    }
    fn journal(&self) -> std::path::PathBuf {
        self.0.join("ordinary.jsonl")
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
fn context(d: i64) -> ClockContext {
    ClockContext {
        playthrough_id: "fixture-playthrough".into(),
        player: "GER".into(),
        anchors: scope().anchors,
        date_raw: crate::calendar::encode(1936, 1, d).unwrap(),
        difficulty: 2,
        historical_raw: 1,
        paused: false,
        speed: 3,
    }
}
fn entry(count: u16) -> Entry {
    Entry {
        id: "infantry-allocation".into(),
        intent: Intent::Economy {
            target: EconomyTarget::Production {
                line_id: "0x0000000100000038".into(),
                equipment_id: "infantry_equipment_1".into(),
                total_factories: count,
            },
        },
        navigation: None,
        focus_search: None,
    }
}
fn factory(count: i32) -> FactoryState {
    FactoryState {
        epoch: scope().epoch,
        line: LineIdentity {
            country: scope().own_country,
            production: 0xb0000,
            line: 0x60000,
            variant: 0x70000,
            base_type: 0x80000,
            handle: 0x0000000100000038,
            equipment_id: "infantry_equipment_1".into(),
        },
        requested: count,
    }
}
struct Clear;
impl ordinary::UnresolvedInputs for Clear {
    fn clear(&self, _: &Identity) -> Result<()> {
        Ok(())
    }
}
struct Fake {
    current: FactoryState,
    panel: PanelState,
    clicks: usize,
    reject: bool,
    delivery: u8,
}
impl Fake {
    fn new(count: i32, shown: bool) -> Self {
        Self {
            current: factory(count),
            panel: PanelState {
                epoch: scope().epoch,
                panel_identity: vec![
                    0x40000, 0xb0000, 0x20000, 0xc0000, 0xd0000, 0xe0000, 0xf0000,
                ],
                managed_mode: if shown { 2 } else { 23 },
                panel_shown: shown,
            },
            clicks: 0,
            reject: false,
            delivery: 0,
        }
    }
}
impl Backend for Fake {
    type Candidate = ();
    fn observe(&mut self, t: &Target, _: Instant) -> Result<State> {
        Ok(match t {
            Target::Panel(_) => State::Panel(self.panel.clone()),
            _ => State::Factories(self.current.clone()),
        })
    }
    fn candidate(&mut self, t: &Target, _: &State, _: Instant) -> Result<()> {
        if self.reject {
            return Err(Error::new("fixture_precondition", "No candidate"));
        }
        if matches!(t, Target::Factories(_)) && !self.panel.panel_shown {
            return Err(Error::new("fixture_precondition", "Panel closed"));
        }
        Ok(())
    }
    fn dispatch_once(&mut self, t: &Target, _: &State, _: &(), _: Instant) -> DispatchOutcome {
        self.clicks += 1;
        if self.delivery == 1 {
            return DispatchOutcome::NotQueued(Error::new("fixture_not_queued", "No input queued"));
        }
        if self.delivery == 2 {
            return DispatchOutcome::Unknown(Some(Error::new("fixture_unknown", TOKEN)));
        }
        match t {
            Target::Panel(_) => {
                self.panel.managed_mode = 2;
                self.panel.panel_shown = true;
            }
            Target::Factories(t) => self.current.requested = t.factories,
        }
        DispatchOutcome::ServerConfirmed
    }
    fn confirm_input_idle(&mut self, _: Instant) -> Result<()> {
        Ok(())
    }
}
fn execute(j: &mut ordinary::Journal, f: &mut Fake, r: &Request) -> Result<Execution> {
    let mut auth = InputAuthority::new("player".into(), scope().epoch.clone()).unwrap();
    auth.acquire("player", TOKEN.into(), 900, &scope().epoch)
        .unwrap();
    j.execute(
        &r.id,
        r.target.clone(),
        TOKEN,
        &auth,
        &RwLock::new(true),
        &Clear,
        f,
        TIME,
    )
}
fn finalize(
    p: &mut Progress,
    j: &mut ordinary::Journal,
    f: &mut Fake,
    e: &Entry,
    c: &ClockContext,
    limit: u8,
) {
    let a = Attempt {
        request_id: "b".repeat(64),
        entry: e.clone(),
        scope: scope(),
        day: day(c.date_raw).unwrap(),
        before_sequence: 5,
    };
    let r = p.begin_completion(&a, &f.current, c, limit).unwrap();
    let execution = execute(j, f, &r).unwrap();
    let (original, before_context) = p.original(e).unwrap();
    VerifiedReceipt::from_production(
        a,
        &execution,
        original,
        &f.current,
        before_context,
        c,
        Observation {
            scope: scope(),
            date_raw: c.date_raw,
            sequence: 6,
            observed_unix_ms: 1000,
            source: "generated canonical production".into(),
            counts: OwnCounts::default(),
        },
    )
    .unwrap();
    p.verified(&r.id, &execution, &f.current, c).unwrap();
}
fn prepare(
    p: &mut Progress,
    j: &mut ordinary::Journal,
    f: &mut Fake,
    e: &Entry,
    c: &ClockContext,
    limit: u8,
) {
    let Next::Prepare(t) = p
        .next(e, &f.current, c, f.panel.panel_shown, limit)
        .unwrap()
    else {
        panic!("preparation expected")
    };
    let r = p.begin_preparation(e, t, &f.current, c, limit).unwrap();
    assert!(p.blocked());
    let execution = execute(j, f, &r).unwrap();
    p.verified(&r.id, &execution, &f.current, c).unwrap();
    assert!(!p.blocked());
}

#[test]
fn opening_and_multiple_days_preserve_progress_budgets_and_original_identity() {
    let temp = Temp::new();
    let e = entry(14);
    let mut p = Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).unwrap();
    let mut j = ordinary::Journal::open(&temp.journal()).unwrap();
    let mut f = Fake::new(10, false);
    prepare(&mut p, &mut j, &mut f, &e, &context(1), 2); // Opening is not goal completion.
    prepare(&mut p, &mut j, &mut f, &e, &context(1), 2); // 10 -> 11.
    assert_eq!(
        p.next(&e, &f.current, &context(1), true, 2).unwrap(),
        Next::BudgetExhausted
    );
    assert_eq!(p.used_steps(1936 * 365), 2);
    assert_eq!(f.current.requested, 11);
    drop(p);
    drop(j);
    let mut p = Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).unwrap();
    let mut j = ordinary::Journal::open(&temp.journal()).unwrap();
    assert_eq!(
        p.next(&e, &f.current, &context(1), true, 1).unwrap(),
        Next::BudgetExhausted
    );
    prepare(&mut p, &mut j, &mut f, &e, &context(2), 2); // 11 -> 12.
    prepare(&mut p, &mut j, &mut f, &e, &context(2), 2); // 12 -> 13.
    assert_eq!(
        p.next(&e, &f.current, &context(2), true, 2).unwrap(),
        Next::BudgetExhausted
    );
    assert!(matches!(
        p.next(&e, &f.current, &context(3), true, 2).unwrap(),
        Next::Finalize(_)
    ));
    finalize(&mut p, &mut j, &mut f, &e, &context(3), 2);
    assert_eq!(
        p.next(&e, &f.current, &context(3), true, 2).unwrap(),
        Next::Finished
    );
    assert_eq!(f.current.requested, 14);
    assert_eq!(f.clicks, 5);
    assert_eq!(p.original(&e).unwrap().0, &factory(10).line);
    assert_eq!(p.original(&e).unwrap().1, &context(1));
    drop(p);
    let mut p = Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).unwrap();
    assert_eq!(
        p.next(&e, &f.current, &context(4), true, 16).unwrap(),
        Next::Finished
    );
    assert_eq!(p.used_steps(1936 * 365 + 2), 1);
    let summary = p.summary().to_string();
    for absent in [
        "playthrough",
        "variant",
        "anchors",
        "/fixture",
        "production\"",
    ] {
        assert!(!summary.contains(absent));
    }
}

#[test]
fn decreasing_and_already_matching_goals_use_only_required_steps() {
    for initial in [2, 5] {
        let temp = Temp::new();
        let e = entry(2);
        let mut p = Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).unwrap();
        let mut j = ordinary::Journal::open(&temp.journal()).unwrap();
        let mut f = Fake::new(initial, initial != 2);
        loop {
            match p
                .next(&e, &f.current, &context(1), f.panel.panel_shown, 16)
                .unwrap()
            {
                Next::Prepare(_) => prepare(&mut p, &mut j, &mut f, &e, &context(1), 16),
                Next::Finalize(_) => {
                    finalize(&mut p, &mut j, &mut f, &e, &context(1), 16);
                    break;
                }
                other => panic!("unexpected {other:?}"),
            }
        }
        assert_eq!(f.current.requested, 2);
        assert_eq!(f.clicks, (initial - 2) as usize);
    }
}

#[test]
fn changed_line_variant_roots_campaign_or_unexplained_count_never_rebase() {
    let temp = Temp::new();
    let e = entry(14);
    let mut p = Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).unwrap();
    p.next(&e, &factory(10), &context(1), true, 2).unwrap();
    for case in 0..9 {
        let mut fresh = factory(10);
        let mut c = context(1);
        match case {
            0 => fresh.line.variant += 8,
            1 => fresh.line.line += 8,
            2 => fresh.line.production += 8,
            3 => fresh.requested = 11,
            4 => fresh.epoch.game.start_ticks.push('7'),
            5 => c.playthrough_id.push('x'),
            6 => c.anchors[0] += 8,
            7 => c.historical_raw = 0,
            8 => c.date_raw -= 1,
            _ => unreachable!(),
        }
        assert!(p.next(&e, &fresh, &c, true, 2).is_err(), "case {case}");
        assert_eq!(p.used_steps(1936 * 365), 0);
        assert!(!p.blocked());
    }
}

#[test]
fn interrupted_intent_and_unknown_delivery_stay_blocked_after_restart() {
    for delivered in [false, true] {
        let temp = Temp::new();
        let e = entry(14);
        let mut p = Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).unwrap();
        let mut j = ordinary::Journal::open(&temp.journal()).unwrap();
        let mut f = Fake::new(10, true);
        let Next::Prepare(t) = p.next(&e, &f.current, &context(1), true, 2).unwrap() else {
            panic!()
        };
        let r = p
            .begin_preparation(&e, t, &f.current, &context(1), 2)
            .unwrap();
        if delivered {
            f.delivery = 2;
            let error = execute(&mut j, &mut f, &r).unwrap_err();
            let evidence = j.failure_evidence(&r.id, &r.target, &error);
            assert_eq!(evidence, FailureEvidence::Unknown);
            p.failed(&r.id, &error, evidence, TOKEN).unwrap();
            assert!(!p.summary().to_string().contains(TOKEN));
        }
        drop(p);
        let mut p = Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).unwrap();
        assert!(p.blocked());
        assert!(p.next(&e, &f.current, &context(2), true, 16).is_err());
        assert_eq!(p.used_steps(1936 * 365), 1);
        assert_eq!(f.clicks, usize::from(delivered));
    }
}

#[test]
fn known_zero_input_retains_goal_and_daily_budget_without_same_day_retry() {
    for before_intent in [true, false] {
        let temp = Temp::new();
        let e = entry(14);
        let mut p = Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).unwrap();
        let mut j = ordinary::Journal::open(&temp.journal()).unwrap();
        let mut f = Fake::new(10, true);
        let Next::Prepare(t) = p.next(&e, &f.current, &context(1), true, 2).unwrap() else {
            panic!()
        };
        let r = p
            .begin_preparation(&e, t, &f.current, &context(1), 2)
            .unwrap();
        f.reject = before_intent;
        f.delivery = 1;
        let error = execute(&mut j, &mut f, &r).unwrap_err();
        let evidence = j.failure_evidence(&r.id, &r.target, &error);
        assert_eq!(
            evidence,
            if before_intent {
                FailureEvidence::BeforeIntent
            } else {
                FailureEvidence::ConfirmedNotQueued
            }
        );
        p.failed(&r.id, &error, evidence, TOKEN).unwrap();
        assert!(!p.blocked());
        assert_eq!(
            p.next(&e, &f.current, &context(1), true, 16).unwrap(),
            Next::Deferred
        );
        drop(p);
        let mut p = Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).unwrap();
        assert_eq!(
            p.next(&e, &f.current, &context(1), true, 16).unwrap(),
            Next::Deferred
        );
        f.reject = false;
        f.delivery = 0;
        prepare(&mut p, &mut j, &mut f, &e, &context(2), 2);
        assert_eq!(f.current.requested, 11);
        assert_eq!(p.used_steps(1936 * 365), 1);
        assert_eq!(p.used_steps(1936 * 365 + 1), 1);
    }
}

#[test]
fn historical_and_other_receipts_do_not_clear_original_intent() {
    let temp = Temp::new();
    let e = entry(14);
    let mut p = Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).unwrap();
    let mut j = ordinary::Journal::open(&temp.journal()).unwrap();
    let mut f = Fake::new(10, true);
    let Next::Prepare(t) = p.next(&e, &f.current, &context(1), true, 2).unwrap() else {
        panic!()
    };
    let r = p
        .begin_preparation(&e, t, &f.current, &context(1), 2)
        .unwrap();
    let execution = execute(&mut j, &mut f, &r).unwrap();
    let replay = execute(&mut j, &mut f, &r).unwrap();
    assert!(p.verified(&r.id, &replay, &f.current, &context(1)).is_err());
    assert!(p.blocked());
    let mut replaced = f.current.clone();
    replaced.line.variant += 8;
    assert!(p
        .verified(&r.id, &execution, &replaced, &context(1))
        .is_err());
    assert!(p.blocked());
    p.verified(&r.id, &execution, &f.current, &context(1))
        .unwrap();
    assert!(!p.blocked());
    assert_eq!(f.clicks, 1);
}

#[test]
fn stored_history_rejects_jump_tampering_missing_binding_and_plan_replacement() {
    for case in 0..7 {
        let temp = Temp::new();
        let e = entry(14);
        let mut p = Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).unwrap();
        let mut j = ordinary::Journal::open(&temp.journal()).unwrap();
        let mut f = Fake::new(10, true);
        prepare(&mut p, &mut j, &mut f, &e, &context(1), 2);
        drop(p);
        let bytes = fs::read(temp.progress()).unwrap();
        let mut data: Value = serde_json::from_slice(&bytes).unwrap();
        match case {
            0 => data["steps"][0]["before"] = json!(9),
            1 => data["steps"][0]["after"] = json!(14),
            2 => data["steps"][0]["id"] = json!("d".repeat(64)),
            3 => data["bindings"] = json!([]),
            4 => data["steps"][0]["status"] = json!("Intent"),
            5 => data["steps"][0]["after_date"] = Value::Null,
            6 => data["steps"][0]["after_date"] = json!(context(1).date_raw - 1),
            _ => unreachable!(),
        }
        fs::write(temp.progress(), serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).is_err());
        fs::write(temp.progress(), bytes).unwrap();
        assert!(Progress::open(&temp.progress(), scope(), &[entry(15)]).is_err());
        assert!(Progress::open(&temp.progress(), scope(), &[]).is_err());
        assert!(Progress::open(&temp.progress(), scope(), &[e]).is_ok());
    }
}

#[test]
fn original_uncertainty_survives_read_only_settlement_and_cannot_be_relabelled() {
    let temp = Temp::new();
    let e = entry(14);
    let mut p = Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).unwrap();
    let mut j = ordinary::Journal::open(&temp.journal()).unwrap();
    let mut f = Fake::new(10, true);
    let Next::Prepare(t) = p.next(&e, &f.current, &context(1), true, 2).unwrap() else {
        panic!()
    };
    let r = p
        .begin_preparation(&e, t, &f.current, &context(1), 2)
        .unwrap();
    f.delivery = 2;
    let cause = execute(&mut j, &mut f, &r).unwrap_err();
    p.failed(
        &r.id,
        &cause,
        j.failure_evidence(&r.id, &r.target, &cause),
        TOKEN,
    )
    .unwrap();
    let failure = p.summary()["latest"]["failure"].clone();
    assert!(p
        .failed(
            &r.id,
            &Error::new("later", "replacement"),
            FailureEvidence::BeforeIntent,
            TOKEN
        )
        .is_err());
    assert!(p.blocked());
    p.failed(
        &r.id,
        &Error::new("later", "replacement"),
        FailureEvidence::Unknown,
        TOKEN,
    )
    .unwrap();
    assert_eq!(p.summary()["latest"]["failure"], failure);
    drop(p);
    drop(j);
    let mut p = Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).unwrap();
    let mut j = ordinary::Journal::open(&temp.journal()).unwrap();
    // Simulate late observed delivery; recovery reads the original target only.
    f.current.requested = 11;
    let execution = execute(&mut j, &mut f, &r).unwrap();
    assert!(!execution.historical);
    assert_eq!(f.clicks, 1);
    p.verified(&r.id, &execution, &f.current, &context(2))
        .unwrap();
    assert!(!p.blocked());
    assert_eq!(p.summary()["latest"]["failure"], failure);
    drop(p);
    let mut p = Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).unwrap();
    assert_eq!(p.summary()["latest"]["failure"], failure);
    assert!(p.next(&e, &f.current, &context(1), true, 16).is_err());
}

#[test]
fn budget_is_shared_across_entries_and_cannot_be_reset_by_reopening() {
    let temp = Temp::new();
    let first = entry(14);
    let mut second = entry(12);
    second.id = "second-allocation".into();
    let entries = [first.clone(), second.clone()];
    let mut p = Progress::open(&temp.progress(), scope(), &entries).unwrap();
    let mut j = ordinary::Journal::open(&temp.journal()).unwrap();
    let mut f = Fake::new(10, true);
    for limit in [0, 17, 255] {
        assert!(p
            .next(&first, &f.current, &context(1), true, limit)
            .is_err());
        assert!(!temp.progress().exists());
    }
    prepare(&mut p, &mut j, &mut f, &first, &context(1), 1);
    // The governor owns ordering. Even an independent entry cannot reset the
    // production-wide daily step count by introducing a new binding.
    assert_eq!(
        p.next(&second, &f.current, &context(1), true, 1).unwrap(),
        Next::BudgetExhausted
    );
    drop(p);
    let mut p = Progress::open(&temp.progress(), scope(), &entries).unwrap();
    assert_eq!(
        p.next(&second, &f.current, &context(1), true, 1).unwrap(),
        Next::BudgetExhausted
    );
    assert!(matches!(
        p.next(&second, &f.current, &context(2), true, 1).unwrap(),
        Next::Finalize(_)
    ));
    assert_eq!(f.clicks, 1);
}

#[test]
fn invalid_steps_and_foreign_completion_attempts_never_persist_input_authority() {
    for case in 0..8 {
        let temp = Temp::new();
        let e = entry(if case == 1 || case >= 3 { 11 } else { 14 });
        let mut p = Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).unwrap();
        p.next(&e, &factory(10), &context(1), true, 2).unwrap();
        let original = fs::read(temp.progress()).unwrap();
        let mut attempt = Attempt {
            request_id: "b".repeat(64),
            entry: e.clone(),
            scope: scope(),
            day: day(context(1).date_raw).unwrap(),
            before_sequence: 5,
        };
        let result = match case {
            7 => p.begin_preparation(
                &e,
                Target::Panel(PanelTarget::closed(scope().epoch)),
                &factory(10),
                &context(1),
                2,
            ), // Closing cannot masquerade as allocation preparation.
            0 | 1 => {
                let mut desired = target(&scope(), &e).unwrap();
                desired.factories = if case == 0 { 12 } else { 11 };
                p.begin_preparation(&e, Target::Factories(desired), &factory(10), &context(1), 2)
            }
            _ => {
                match case {
                    2 => {} // A final step cannot jump from 10 to 14.
                    3 => attempt.scope.epoch.game.start_ticks.push('7'),
                    4 => attempt.day += 1,
                    5 => attempt.before_sequence = 0,
                    6 => attempt.request_id = "invalid-id".into(),
                    _ => unreachable!(),
                }
                p.begin_completion(&attempt, &factory(10), &context(1), 2)
            }
        };
        assert!(result.is_err(), "case {case}");
        assert_eq!(fs::read(temp.progress()).unwrap(), original);
        drop(p);
        let p = Progress::open(&temp.progress(), scope(), &[e]).unwrap();
        assert!(!p.blocked());
        assert_eq!(p.used_steps(1936 * 365), 0);
    }
}

#[test]
fn failed_durable_write_quarantines_the_writer_before_any_action() {
    let temp = Temp::new();
    let e = entry(14);
    let mut p = Progress::open(&temp.progress(), scope(), std::slice::from_ref(&e)).unwrap();
    let Next::Prepare(desired) = p.next(&e, &factory(10), &context(1), true, 2).unwrap() else {
        panic!()
    };
    // An external rewrite, even harmless whitespace, invalidates the writer's
    // fingerprint. The caller receives no Request to pass to an input adapter.
    let mut bytes = fs::read(temp.progress()).unwrap();
    bytes.push(b'\n');
    fs::write(temp.progress(), &bytes).unwrap();
    assert!(p
        .begin_preparation(&e, desired, &factory(10), &context(1), 2)
        .is_err());
    assert!(p.blocked());
    assert!(p.next(&e, &factory(10), &context(2), true, 16).is_err());
    assert_eq!(fs::read(temp.progress()).unwrap(), bytes);
}
