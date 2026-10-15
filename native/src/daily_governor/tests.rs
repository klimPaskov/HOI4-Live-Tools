use super::*;
use crate::{actions::Epoch, platform::Identity};
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};
#[cfg(unix)]
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};
#[cfg(unix)]
struct Temp {
    root: std::path::PathBuf,
}
#[cfg(unix)]
impl Temp {
    fn new() -> Self {
        static N: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "private-daily-governor-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        #[cfg(unix)]
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
        Self { root }
    }
    fn path(&self) -> std::path::PathBuf {
        self.root.join("plan.json")
    }
}
#[cfg(unix)]
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
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
                    exe: "/owned/hoi4".into(),
                    uid: 1000,
                },
                campaign: "owned-campaign".into(),
                player: "GER".into(),
            },
            anchors: [0x10000, 0x20000, 0x30000],
            own_country: 0x40000,
            own_status: 0x50000,
        },
        entries: vec![
            Entry {
                id: "research-1".into(),
                intent: Intent::Research {
                    technology_id: "support_weapons".into(),
                    slot: 0,
                },
                navigation: None,
            },
            Entry {
                id: "focus-1".into(),
                intent: Intent::Focus {
                    focus_id: "GER_rhineland".into(),
                },
                navigation: None,
            },
        ],
        max_actions_per_day: 4,
        max_observation_age_ms: 1000,
        review_day: Some(1936 * 365 + 30),
    }
}
#[cfg(unix)]
#[derive(Clone, Copy)]
enum Mode {
    Verified,
    NotQueued,
    Uncertain,
    Error,
    Historical,
    WrongTarget,
    WrongOwner,
    Late,
    StaleReceipt,
}
#[cfg(unix)]
struct Fake {
    obs: Observation,
    mode: Mode,
    ready: Readiness,
    unresolved: bool,
    observes: usize,
    sends: usize,
    change_on: Option<usize>,
    torn_on: Option<usize>,
    panic: bool,
    unresolved_after_send: bool,
}
#[cfg(unix)]
impl Fake {
    fn new(p: &Plan) -> Self {
        Self {
            obs: Observation {
                scope: p.scope.clone(),
                date_raw: crate::calendar::encode(1936, 1, 1).unwrap(),
                sequence: 1,
                observed_unix_ms: 9999,
                source: "fresh generated own state".into(),
                counts: OwnCounts::default(),
            },
            mode: Mode::Verified,
            ready: Readiness::Ready,
            unresolved: false,
            observes: 0,
            sends: 0,
            change_on: None,
            torn_on: None,
            panic: false,
            unresolved_after_send: false,
        }
    }
    fn day(&mut self, n: i64) {
        self.obs.date_raw = crate::calendar::encode(1936, 1, n).unwrap();
    }
}
#[cfg(unix)]
impl Backend for Fake {
    fn unresolved_input(&mut self) -> Result<bool> {
        Ok(self.unresolved)
    }
    fn observe(&mut self) -> Result<Observation> {
        self.observes += 1;
        if self.torn_on == Some(self.observes) {
            return Err(Error::new("torn_read", "Late own slot/root witness"));
        }
        if self.change_on == Some(self.observes) {
            self.obs.scope.own_country += 8;
        }
        self.obs.sequence += 1;
        Ok(self.obs.clone())
    }
    fn readiness(&mut self, _: &Entry, _: &Observation) -> Result<Readiness> {
        Ok(self.ready.clone())
    }
    fn attempt(&mut self, a: &Attempt, _: &Observation, deadline: Instant) -> Result<Outcome> {
        self.sends += 1;
        if self.unresolved_after_send {
            self.unresolved = true;
        }
        if self.panic {
            panic!("generated crash after durable intent");
        }
        match self.mode {
            Mode::NotQueued => Ok(Outcome::NotQueued("local refusal; no input queued".into())),
            Mode::Uncertain => Ok(Outcome::Uncertain("transport receipt unknown".into())),
            Mode::Error => Err(Error::new("transport_error", "possibly sent")),
            _ => {
                self.obs.sequence += 1;
                let mut r = VerifiedReceipt {
                    attempt: a.clone(),
                    current: true,
                    canonical_intent: a.entry.intent.clone(),
                    observed: self.obs.clone(),
                };
                match self.mode {
                    Mode::Historical => r.current = false,
                    Mode::WrongTarget => {
                        r.canonical_intent = Intent::Focus {
                            focus_id: "wrong_focus".into(),
                        }
                    }
                    Mode::WrongOwner => r.observed.scope.epoch.game.start_ticks = "changed".into(),
                    Mode::StaleReceipt => r.observed.sequence = a.before_sequence,
                    Mode::Late => {
                        std::thread::sleep(
                            deadline.saturating_duration_since(Instant::now())
                                + Duration::from_millis(1),
                        );
                    }
                    _ => {}
                }
                Ok(Outcome::Verified(Box::new(r)))
            }
        }
    }
}
#[cfg(unix)]
fn run(g: &mut Governor, b: &mut Fake) -> Result<DailyResult> {
    g.run_day(b, 10000, Instant::now() + Duration::from_secs(1))
}
#[cfg(unix)]
fn extended(mut p: Plan) -> Plan {
    p.revision += 1;
    p.entries.push(Entry {
        id: "research-2".into(),
        intent: Intent::Research {
            technology_id: "construction1".into(),
            slot: 1,
        },
        navigation: None,
    });
    p
}

#[test]
#[cfg(unix)]
fn plan_update_preserves_completed_evidence_and_same_day_consumption_across_restart() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    run(&mut g, &mut b).unwrap();
    let old = serde_json::to_value(g.entry_state("research-1")).unwrap();
    let next = extended(p);
    assert!(g.update_plan(next.clone()).unwrap());
    assert_eq!(
        serde_json::to_value(g.entry_state("research-1")).unwrap(),
        old
    );
    assert!(!g.update_plan(next.clone()).unwrap());
    assert!(!run(&mut g, &mut b).unwrap().advanced);
    assert_eq!(b.sends, 2);
    drop(g);
    let mut g = Governor::open(&t.path(), next).unwrap();
    assert_eq!(
        serde_json::to_value(g.entry_state("research-1")).unwrap(),
        old
    );
    b.day(2);
    assert_eq!(run(&mut g, &mut b).unwrap().verified, vec!["research-2"]);
    assert_eq!(b.sends, 3);
}

#[test]
#[cfg(unix)]
fn plan_update_retains_notqueued_attempts_and_pins_legacy_request_revisions() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    b.mode = Mode::NotQueued;
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    run(&mut g, &mut b).unwrap();
    // Old immutable-plan files have no explicit per-record revision.
    for record in &mut g.state.journal {
        record.plan_revision = None;
    }
    g.persist().unwrap();
    drop(g);
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    let next = extended(p);
    g.update_plan(next.clone()).unwrap();
    assert!(g.state.journal.iter().all(|r| r.plan_revision == Some(1)));
    assert!(matches!(
        g.entry_state("research-1"),
        Some(EntryState::Pending { attempts: 1, .. })
    ));
    drop(g);
    let mut g = Governor::open(&t.path(), next.clone()).unwrap();
    b.day(2);
    b.mode = Mode::Verified;
    assert_eq!(run(&mut g, &mut b).unwrap().verified.len(), 3);
    assert_eq!(b.sends, 5);
    drop(g);
    let g = Governor::open(&t.path(), next).unwrap();
    assert!(matches!(
        g.entry_state("research-1"),
        Some(EntryState::Complete { .. })
    ));
}

#[test]
#[cfg(unix)]
fn invalid_plan_updates_change_neither_saved_bytes_nor_pending_entries() {
    let t = Temp::new();
    let p = plan();
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    let saved = fs::read(t.path()).unwrap();
    for case in 0..5 {
        let mut next = extended(p.clone());
        match case {
            0 => {
                next.entries.remove(0);
            }
            1 => {
                next.entries[0].intent = Intent::Focus {
                    focus_id: "different_focus".into(),
                }
            }
            2 => next.scope.epoch.game.start_ticks = "789".into(),
            3 => next.revision = p.revision,
            _ => next.max_actions_per_day = 0,
        }
        assert!(g.update_plan(next).is_err());
        assert_eq!(fs::read(t.path()).unwrap(), saved);
        assert_eq!(g.state.plan, p);
        assert_eq!(g.state.entries.len(), 2);
    }
}

#[test]
#[cfg(unix)]
fn plan_updates_reorder_and_change_limits_without_resetting_existing_work() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    b.ready = Readiness::Deferred("own view not ready".into());
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    run(&mut g, &mut b).unwrap();
    let before = serde_json::to_value(&g.state.entries).unwrap();
    let mut next = p;
    next.revision = 4;
    next.entries.reverse();
    next.max_actions_per_day = 1;
    next.max_observation_age_ms = 500;
    next.review_day = None;
    g.update_plan(next.clone()).unwrap();
    assert_eq!(serde_json::to_value(&g.state.entries).unwrap(), before);
    drop(g);
    let mut g = Governor::open(&t.path(), next).unwrap();
    b.day(2);
    b.ready = Readiness::Ready;
    assert_eq!(run(&mut g, &mut b).unwrap().verified, vec!["focus-1"]);
    assert_eq!(b.sends, 1);
    assert!(matches!(
        g.entry_state("research-1"),
        Some(EntryState::Pending { attempts: 0, .. })
    ));
}

#[test]
#[cfg(unix)]
fn uncertain_input_and_persistence_identity_loss_block_plan_updates() {
    for uncertain in [true, false] {
        let t = Temp::new();
        let p = plan();
        let mut b = Fake::new(&p);
        let mut g = Governor::open(&t.path(), p.clone()).unwrap();
        if uncertain {
            b.mode = Mode::Uncertain;
            assert!(run(&mut g, &mut b).is_err());
        } else {
            // The lock inode is pinned for the lifetime of the writer. Data
            // files legitimately change inode on the Store's atomic replace.
            let lock = t.root.join("plan.json.lock");
            fs::rename(&lock, t.root.join("displaced.lock")).unwrap();
            fs::write(&lock, []).unwrap();
            fs::set_permissions(&lock, fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert!(g.update_plan(extended(p.clone())).is_err());
        assert_eq!(g.state.plan, p);
    }
}

#[test]
#[cfg(unix)]
fn forged_or_mismatched_historical_plan_revisions_refuse_restart() {
    for forged in [0, 3, 2, 4] {
        let t = Temp::new();
        let p = plan();
        let mut b = Fake::new(&p);
        let mut g = Governor::open(&t.path(), p.clone()).unwrap();
        run(&mut g, &mut b).unwrap();
        let next = extended(p);
        g.update_plan(next.clone()).unwrap();
        drop(g);
        let mut state: Value = serde_json::from_slice(&fs::read(t.path()).unwrap()).unwrap();
        if forged == 4 {
            state["journal"][0]["plan_revision"] = json!(2);
            state["journal"][1]["plan_revision"] = json!(2);
        } else {
            state["journal"][1]["plan_revision"] = json!(forged);
        }
        fs::write(t.path(), serde_json::to_vec(&state).unwrap()).unwrap();
        assert!(Governor::open(&t.path(), next).is_err());
    }
}

#[test]
#[cfg(unix)]
fn completed_current_receipts_are_idempotent_across_restart_and_days() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    let out = run(&mut g, &mut b).unwrap();
    assert_eq!(out.verified, vec!["research-1", "focus-1"]);
    assert_eq!(b.sends, 2);
    assert!(!run(&mut g, &mut b).unwrap().advanced);
    drop(g);
    let mut g = Governor::open(&t.path(), p).unwrap();
    b.day(2);
    assert!(run(&mut g, &mut b).unwrap().advanced);
    assert_eq!(b.sends, 2);
    assert!(matches!(
        g.entry_state("research-1"),
        Some(EntryState::Complete { .. })
    ));
}
#[test]
#[cfg(unix)]
fn local_refusal_retains_work_once_per_advancing_day() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    b.mode = Mode::NotQueued;
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    assert_eq!(run(&mut g, &mut b).unwrap().retained.len(), 2);
    assert_eq!(b.sends, 2);
    run(&mut g, &mut b).unwrap();
    assert_eq!(b.sends, 2);
    drop(g);
    let mut g = Governor::open(&t.path(), p).unwrap();
    b.day(2);
    b.mode = Mode::Verified;
    assert_eq!(run(&mut g, &mut b).unwrap().verified.len(), 2);
    assert_eq!(b.sends, 4);
}
#[test]
#[cfg(unix)]
fn unavailable_and_unknown_preserve_none_and_action_intent() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    b.ready = Readiness::Unavailable("ordinary focus capability unavailable".into());
    let mut g = Governor::open(&t.path(), p).unwrap();
    let out = run(&mut g, &mut b).unwrap();
    assert_eq!(b.sends, 0);
    assert!(out.digest["counts"]["idle_research_slots"].is_null());
    assert_eq!(out.digest["retained_count"], 2);
    assert_eq!(out.digest["retained"][0]["fix"]["entry_id"], "research-1");
    assert!(serde_json::to_vec(&out.digest).unwrap().len() <= 2048);
}
#[test]
#[cfg(unix)]
fn uncertain_error_historical_foreign_target_stale_and_late_receipts_quarantine_restart() {
    for mode in [
        Mode::Uncertain,
        Mode::Error,
        Mode::Historical,
        Mode::WrongTarget,
        Mode::WrongOwner,
        Mode::StaleReceipt,
        Mode::Late,
    ] {
        let t = Temp::new();
        let p = plan();
        let mut b = Fake::new(&p);
        b.mode = mode;
        let mut g = Governor::open(&t.path(), p.clone()).unwrap();
        assert!(g
            .run_day(&mut b, 10000, Instant::now() + Duration::from_secs(1))
            .is_err());
        assert_eq!(b.sends, 1);
        drop(g);
        let mut g = Governor::open(&t.path(), p).unwrap();
        b.mode = Mode::Verified;
        assert_eq!(
            run(&mut g, &mut b).unwrap_err().code,
            "governor_quarantined"
        );
        assert_eq!(b.sends, 1);
    }
}
#[test]
#[cfg(unix)]
fn crash_after_intent_never_replays_on_restart() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    b.panic = true;
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&mut g, &mut b))).is_err()
    );
    b.panic = false;
    assert_eq!(
        run(&mut g, &mut b).unwrap_err().code,
        "governor_quarantined"
    );
    drop(g);
    let mut g = Governor::open(&t.path(), p).unwrap();
    assert_eq!(
        run(&mut g, &mut b).unwrap_err().code,
        "governor_quarantined"
    );
    assert_eq!(b.sends, 1);
}
#[test]
#[cfg(unix)]
fn shared_uncertainty_prevents_any_observation_or_new_token_rescue() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    b.unresolved = true;
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    assert!(run(&mut g, &mut b).is_err());
    assert_eq!((b.observes, b.sends), (0, 0));
    drop(g);
    let mut g = Governor::open(&t.path(), p).unwrap();
    b.unresolved = false;
    assert_eq!(
        run(&mut g, &mut b).unwrap_err().code,
        "governor_quarantined"
    );
}
#[test]
#[cfg(unix)]
fn late_context_and_torn_mutations_prevent_backend_attempt() {
    for n in [1, 2, 3] {
        let t = Temp::new();
        let p = plan();
        let mut b = Fake::new(&p);
        b.change_on = Some(n);
        let mut g = Governor::open(&t.path(), p).unwrap();
        assert_eq!(run(&mut g, &mut b).unwrap_err().code, "ownership_changed");
        assert_eq!(b.sends, 0);
    }
    for n in [1, 2, 3] {
        let t = Temp::new();
        let p = plan();
        let mut b = Fake::new(&p);
        b.torn_on = Some(n);
        let mut g = Governor::open(&t.path(), p).unwrap();
        assert_eq!(run(&mut g, &mut b).unwrap_err().code, "torn_read");
        assert_eq!(b.sends, 0);
    }
}
#[test]
#[cfg(unix)]
fn backwards_dates_and_stale_unknown_source_refuse() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    b.day(2);
    b.ready = Readiness::Deferred("unknown eligibility".into());
    let mut g = Governor::open(&t.path(), p).unwrap();
    run(&mut g, &mut b).unwrap();
    b.day(1);
    assert_eq!(run(&mut g, &mut b).unwrap_err().code, "date_regressed");
    b.day(3);
    b.obs.source.clear();
    assert_eq!(run(&mut g, &mut b).unwrap_err().code, "stale_observation");
    b.obs.source = "fresh".into();
    b.obs.observed_unix_ms = 1;
    assert_eq!(run(&mut g, &mut b).unwrap_err().code, "stale_observation");
    assert_eq!(b.sends, 0);
}
#[test]
#[cfg(unix)]
fn one_writer_private_modes_atomic_replacement_and_external_mutation() {
    let t = Temp::new();
    let p = plan();
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    assert_eq!(
        Governor::open(&t.path(), p.clone()).err().unwrap().code,
        "plan_busy"
    );
    // Pin the old inode: a daily batch makes several atomic replacements, and
    // ext4 may otherwise reuse an earlier unlinked inode for a later temp file.
    // A retained reader must still see its original complete snapshot.
    let mut original = fs::File::open(t.path()).unwrap();
    let before = original.metadata().unwrap().ino();
    let original_bytes = fs::read(t.path()).unwrap();
    let mut b = Fake::new(&p);
    b.ready = Readiness::Deferred("unknown".into());
    run(&mut g, &mut b).unwrap();
    assert_ne!(fs::metadata(t.path()).unwrap().ino(), before);
    let mut retained = Vec::new();
    std::io::Read::read_to_end(&mut original, &mut retained).unwrap();
    assert_eq!(retained, original_bytes);
    assert_ne!(fs::read(t.path()).unwrap(), original_bytes);
    assert_eq!(fs::metadata(t.path()).unwrap().mode() & 0o777, 0o600);
    fs::write(t.path(), b"poison").unwrap();
    assert_eq!(run(&mut g, &mut b).unwrap_err().code, "plan_changed");
    assert_eq!(b.sends, 0);
    drop(g);
    assert!(Governor::open(&t.path(), p).is_err());
}
#[test]
#[cfg(unix)]
fn symlinks_hardlinks_permissions_and_oversize_are_refused() {
    use std::os::unix::fs::symlink;
    for kind in 0..4 {
        let t = Temp::new();
        let p = plan();
        let other = t.root.join("other");
        fs::write(&other, b"{}").unwrap();
        fs::set_permissions(&other, fs::Permissions::from_mode(0o600)).unwrap();
        match kind {
            0 => symlink(&other, t.path()).unwrap(),
            1 => fs::hard_link(&other, t.path()).unwrap(),
            2 => {
                fs::write(t.path(), b"{}").unwrap();
                fs::set_permissions(t.path(), fs::Permissions::from_mode(0o644)).unwrap();
            }
            _ => {
                let f = fs::File::create(t.path()).unwrap();
                f.set_len((1 << 20) + 1).unwrap();
                fs::set_permissions(t.path(), fs::Permissions::from_mode(0o600)).unwrap();
            }
        }
        assert!(Governor::open(&t.path(), p).is_err());
    }
}
#[test]
#[cfg(unix)]
fn blocking_choice_reports_without_consumption_or_pause() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    b.ready =
        Readiness::BlockingDecision("exclusive focus alternative needs declared policy".into());
    let mut g = Governor::open(&t.path(), p).unwrap();
    assert!(run(&mut g, &mut b).unwrap().blocking_decision.is_some());
    assert_eq!(b.sends, 0);
    assert!(matches!(
        g.entry_state("research-1"),
        Some(EntryState::Pending { .. })
    ));
}
#[test]
fn event_notifications_are_bounded_and_not_model_polling() {
    let wake = std::sync::Arc::new(Wakeup::default());
    assert_eq!(wake.wait_after(0, Duration::ZERO).unwrap().generation, 0);
    let w = wake.clone();
    let thread = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(5));
        w.notify(WakeReason::GameDay).unwrap();
    });
    assert_eq!(
        wake.wait_after(0, Duration::from_secs(1))
            .unwrap()
            .generation,
        1
    );
    thread.join().unwrap();
    assert!(wake.wait_after(1, Duration::from_secs(61)).is_err());
}
#[test]
fn explicit_future_domain_ids_validate_without_creating_capabilities() {
    let mut p = plan();
    p.entries.extend([
        Entry {
            id: "economy".into(),
            intent: Intent::Economy {
                target: EconomyTarget::Construction {
                    building: "industrial_complex".into(),
                    state_id: 64,
                    total_levels: 4,
                },
            },
            navigation: None,
        },
        Entry {
            id: "army".into(),
            intent: Intent::Army {
                division_ids: vec![1, 2],
                army_id: 3,
                order_id: "defend_home".into(),
            },
            navigation: None,
        },
        Entry {
            id: "invasion".into(),
            intent: Intent::NavalInvasion {
                division_ids: vec![4, 5],
                army_id: 6,
                source_province: 7,
                destination_province: 8,
                sea_region_ids: vec![9, 10],
            },
            navigation: None,
        },
    ]);
    p.validate().unwrap();
    p.entries[4].intent = Intent::Army {
        division_ids: vec![1, 1],
        army_id: 3,
        order_id: "order".into(),
    };
    assert!(p.validate().is_err());
}

#[test]
#[cfg(unix)]
fn malformed_journal_and_false_completion_are_not_accepted_on_restart() {
    for kind in 0..4 {
        let t = Temp::new();
        let p = plan();
        let mut b = Fake::new(&p);
        let mut g = Governor::open(&t.path(), p.clone()).unwrap();
        run(&mut g, &mut b).unwrap();
        drop(g);
        let mut value: Value = serde_json::from_slice(&fs::read(t.path()).unwrap()).unwrap();
        match kind {
            0 => {
                value["journal"].as_array_mut().unwrap().remove(0);
            }
            1 => value["journal"][1]["after_sequence"] = json!(1),
            2 => value["entries"]["research-1"]["attempt"]["scope"]["own_country"] = json!(0x60000),
            _ => value["last_sequence"] = json!(1),
        }
        fs::write(t.path(), serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(Governor::open(&t.path(), p).is_err());
    }
}
#[test]
#[cfg(unix)]
fn deadline_prevents_input_and_does_not_consume_pending_entries() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    let mut g = Governor::open(&t.path(), p).unwrap();
    let out = g.run_day(&mut b, 10000, Instant::now()).unwrap();
    assert!(out.verified.is_empty());
    assert_eq!(b.sends, 0);
    assert!(matches!(
        g.entry_state("research-1"),
        Some(EntryState::Pending { .. })
    ));
}
#[test]
#[cfg(unix)]
fn revision_process_campaign_or_own_anchor_change_cannot_reset_state() {
    let t = Temp::new();
    let p = plan();
    drop(Governor::open(&t.path(), p.clone()).unwrap());
    for kind in 0..4 {
        let mut changed = p.clone();
        match kind {
            0 => changed.revision += 1,
            1 => changed.scope.epoch.game.start_ticks = "457".into(),
            2 => changed.scope.epoch.campaign = "replacement".into(),
            _ => changed.scope.anchors[0] += 8,
        };
        assert_eq!(
            Governor::open(&t.path(), changed).err().unwrap().code,
            "plan_conflict"
        );
    }
}
#[test]
#[cfg(unix)]
fn partial_state_truncation_and_writer_guard_replacement_refuse() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    let guard = t.root.join("plan.json.lock");
    fs::rename(&guard, t.root.join("preserved.lock")).unwrap();
    fs::write(&guard, b"").unwrap();
    fs::set_permissions(&guard, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(run(&mut g, &mut b).unwrap_err().code, "plan_changed");
    assert_eq!(b.sends, 0);
    drop(g);
    fs::write(t.path(), b"{\"version\":1").unwrap();
    assert!(Governor::open(&t.path(), p).is_err());
}
#[test]
fn unknown_fields_duplicate_intents_and_nonpositive_scope_are_rejected() {
    let mut p = plan();
    p.entries.push(p.entries[0].clone());
    assert!(p.validate().is_err());
    p.entries.pop();
    p.scope.epoch.game.pid = 0;
    assert!(p.validate().is_err());
    let mut data = serde_json::to_value(plan()).unwrap();
    data["console"] = json!(true);
    assert!(serde_json::from_value::<Plan>(data).is_err());
}

#[test]
#[cfg(not(unix))]
fn unsupported_private_storage_never_enables_a_worker() {
    let result = Governor::open(std::path::Path::new("unverified-plan.json"), plan());
    assert_eq!(result.err().unwrap().code, "capability_unavailable");
}

#[test]
#[cfg(unix)]
fn shared_uncertainty_after_verified_transport_prevents_consumption() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    b.unresolved_after_send = true;
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    assert_eq!(run(&mut g, &mut b).unwrap_err().code, "action_uncertain");
    assert!(matches!(
        g.entry_state("research-1"),
        Some(EntryState::Intent { .. })
    ));
    assert_eq!(b.sends, 1);
    drop(g);
    let mut g = Governor::open(&t.path(), p).unwrap();
    b.unresolved = false;
    assert_eq!(
        run(&mut g, &mut b).unwrap_err().code,
        "governor_quarantined"
    );
}

#[test]
fn existing_research_receipt_converter_never_consumes_view_or_historical_receipts() {
    use crate::research_stage::{Execution, OwnState, Receipt, Stage, Status, Target};
    let p = plan();
    let attempt = Attempt {
        request_id: "a".repeat(64),
        entry: p.entries[0].clone(),
        scope: p.scope.clone(),
        day: 1936 * 365,
        before_sequence: 5,
    };
    let obs = Observation {
        scope: p.scope.clone(),
        date_raw: crate::calendar::encode(1936, 1, 1).unwrap(),
        sequence: 6,
        observed_unix_ms: 9999,
        source: "fresh canonical own".into(),
        counts: OwnCounts::default(),
    };
    let mut execution = Execution {
        receipt: Receipt {
            version: 1,
            request_id: attempt.request_id.clone(),
            target: Target {
                epoch: p.scope.epoch.clone(),
                stage: Stage::Research,
                technology: "support_weapons".into(),
                folder_id: None,
                slot: 0,
            },
            status: Status::Verified,
            delivery: None,
            observed: OwnState {
                epoch: p.scope.epoch,
                selected_slot: Some(0),
                selected_folder: None,
                tree_visible: true,
                panel_visible: Some(false),
                active_mode: Some(6),
                researching: vec![Some("support_weapons".into())],
                visible_info: vec![],
            },
        },
        historical: false,
        current_state_verified: true,
        event_causality_verified: false,
    };
    VerifiedReceipt::from_research(attempt.clone(), &execution, obs.clone()).unwrap();
    execution.historical = true;
    assert!(VerifiedReceipt::from_research(attempt.clone(), &execution, obs.clone()).is_err());
    execution.historical = false;
    execution.receipt.target.stage = Stage::View;
    assert!(VerifiedReceipt::from_research(attempt.clone(), &execution, obs.clone()).is_err());
    execution.receipt.target.stage = Stage::Research;
    execution.receipt.observed.researching[0] = Some("construction1".into());
    assert!(VerifiedReceipt::from_research(attempt, &execution, obs).is_err());
}

#[test]
#[cfg(unix)]
fn navigation_preparation_refuses_external_plan_mutation_before_any_stage() {
    let t = Temp::new();
    let p = plan();
    let mut g = Governor::open(&t.path(), p).unwrap();
    assert!(g.preparation_check().is_ok());
    fs::write(t.path(), b"changed externally").unwrap();
    assert!(g.preparation_check().is_err());
    assert!(g.preparation_check().is_err());
}

#[test]
#[cfg(unix)]
fn completed_focus_hint_is_checked_history_without_budget_or_assignment_mutation() {
    let t = Temp::new();
    let mut p = plan();
    let mut next_focus = p.entries[1].clone();
    next_focus.id = "focus-2".into();
    next_focus.intent = Intent::Focus {
        focus_id: "GER_four_year_plan".into(),
    };
    p.entries.push(next_focus.clone());
    let mut b = Fake::new(&p);
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    assert_eq!(g.completed_focus_hint().unwrap(), None);
    continue_goal(&mut g, &mut b, "research-1").unwrap();
    assert_eq!(g.completed_focus_hint().unwrap(), None);
    continue_goal(&mut g, &mut b, "focus-1").unwrap();
    assert_eq!(
        g.completed_focus_hint().unwrap(),
        Some(p.entries[1].clone())
    );
    continue_goal(&mut g, &mut b, "focus-2").unwrap();
    let bytes = fs::read(t.path()).unwrap();
    let budget = g.used_actions(1936 * 365);
    for _ in 0..2 {
        assert_eq!(g.completed_focus_hint().unwrap(), Some(next_focus.clone()));
        assert_eq!(g.used_actions(1936 * 365), budget);
        assert_eq!(fs::read(t.path()).unwrap(), bytes);
    }
    drop(g);
    let mut g = Governor::open(&t.path(), p).unwrap();
    assert_eq!(g.completed_focus_hint().unwrap(), Some(next_focus));
    assert_eq!(fs::read(t.path()).unwrap(), bytes);
    fs::write(t.path(), b"external mutation").unwrap();
    assert!(g.completed_focus_hint().is_err());
    assert!(g.completed_focus_hint().is_err());
}

#[test]
#[cfg(unix)]
fn completed_focus_hint_refuses_unresolved_intent_even_with_an_older_completion() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    let mut g = Governor::open(&t.path(), p).unwrap();
    continue_goal(&mut g, &mut b, "focus-1").unwrap();
    b.mode = Mode::Uncertain;
    assert!(continue_goal(&mut g, &mut b, "research-1").is_err());
    assert_eq!(
        g.completed_focus_hint().unwrap_err().code,
        "governor_quarantined"
    );
}

#[cfg(unix)]
fn refill_plan() -> Plan {
    let mut p = plan();
    p.entries = (0..4)
        .map(|slot| Entry {
            id: format!("refill_{slot}"),
            intent: Intent::Research {
                technology_id: format!("tech_{slot}"),
                slot,
            },
            navigation: None,
        })
        .collect();
    p.max_actions_per_day = 2;
    p
}
#[cfg(unix)]
fn continue_goal(g: &mut Governor, b: &mut Fake, id: &str) -> Result<DailyResult> {
    g.continue_entry(b, id, 10000, Instant::now() + Duration::from_secs(1))
}
#[test]
#[cfg(unix)]
fn same_day_multiple_slots_restart_repeat_and_exhaustion_preserve_budget() {
    let t = Temp::new();
    let p = refill_plan();
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    let mut b = Fake::new(&p);
    let day = 1936 * 365;
    assert_eq!(
        continue_goal(&mut g, &mut b, "refill_0").unwrap().verified,
        vec!["refill_0"]
    );
    assert_eq!(g.used_actions(day), 1);
    assert!(continue_goal(&mut g, &mut b, "refill_0")
        .unwrap()
        .verified
        .is_empty());
    assert_eq!(b.sends, 1);
    drop(g);
    let mut g = Governor::open(&t.path(), p).unwrap();
    assert_eq!(
        continue_goal(&mut g, &mut b, "refill_1").unwrap().verified,
        vec!["refill_1"]
    );
    assert_eq!(g.used_actions(day), 2);
    assert!(continue_goal(&mut g, &mut b, "refill_2")
        .unwrap()
        .verified
        .is_empty());
    assert_eq!(b.sends, 2);
    assert!(!g.budget_available(day));
    assert_eq!(g.state.last_day, Some(day));
    assert!(g
        .entry_state("refill_2")
        .is_some_and(|s| matches!(s, EntryState::Pending { .. })));
    b.day(2);
    assert_eq!(
        continue_goal(&mut g, &mut b, "refill_2").unwrap().verified,
        vec!["refill_2"]
    );
    assert_eq!(g.used_actions(day), 2);
    assert_eq!(g.used_actions(day + 1), 1);
}
#[test]
#[cfg(unix)]
fn continuation_updates_limits_without_resetting_used_or_revision_evidence() {
    let t = Temp::new();
    let mut p = refill_plan();
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    let mut b = Fake::new(&p);
    continue_goal(&mut g, &mut b, "refill_0").unwrap();
    p.revision = 2;
    p.max_actions_per_day = 1;
    g.update_plan(p.clone()).unwrap();
    assert!(continue_goal(&mut g, &mut b, "refill_1")
        .unwrap()
        .verified
        .is_empty());
    assert_eq!(b.sends, 1);
    p.revision = 3;
    p.max_actions_per_day = 3;
    g.update_plan(p.clone()).unwrap();
    assert!(!g.update_plan(p.clone()).unwrap());
    continue_goal(&mut g, &mut b, "refill_1").unwrap();
    assert_eq!(g.used_actions(1936 * 365), 2);
    assert_eq!(g.state.journal[0].plan_revision, Some(1));
    assert_eq!(g.state.journal[2].plan_revision, Some(3));
    drop(g);
    let mut g = Governor::open(&t.path(), p).unwrap();
    continue_goal(&mut g, &mut b, "refill_2").unwrap();
    assert_eq!(g.used_actions(1936 * 365), 3);
}
#[test]
#[cfg(unix)]
fn notqueued_goal_consumes_budget_but_does_not_idle_other_same_day_slots() {
    let t = Temp::new();
    let p = refill_plan();
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    let mut b = Fake::new(&p);
    b.mode = Mode::NotQueued;
    continue_goal(&mut g, &mut b, "refill_0").unwrap();
    assert_eq!(g.used_actions(1936 * 365), 1);
    continue_goal(&mut g, &mut b, "refill_0").unwrap();
    assert_eq!(b.sends, 1);
    b.mode = Mode::Verified;
    continue_goal(&mut g, &mut b, "refill_1").unwrap();
    assert_eq!(b.sends, 2);
    assert!(g
        .entry_state("refill_0")
        .is_some_and(|s| matches!(s, EntryState::Pending { .. })));
}
#[test]
#[cfg(unix)]
fn stale_date_sequence_and_late_receipts_cannot_replenish_or_continue() {
    for mode in [Mode::StaleReceipt, Mode::Late] {
        let t = Temp::new();
        let p = refill_plan();
        let mut g = Governor::open(&t.path(), p.clone()).unwrap();
        let mut b = Fake::new(&p);
        continue_goal(&mut g, &mut b, "refill_0").unwrap();
        b.mode = mode;
        assert!(continue_goal(&mut g, &mut b, "refill_1").is_err());
        assert_eq!(g.used_actions(1936 * 365), 2);
        assert!(continue_goal(&mut g, &mut b, "refill_2").is_err());
        assert_eq!(b.sends, 2);
    }
    let t = Temp::new();
    let p = refill_plan();
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    let mut b = Fake::new(&p);
    continue_goal(&mut g, &mut b, "refill_0").unwrap();
    b.obs.sequence = 0;
    assert!(continue_goal(&mut g, &mut b, "refill_1").is_err());
    assert_eq!(b.sends, 1);
    b.obs.sequence = 100;
    b.obs.date_raw -= 24;
    assert!(continue_goal(&mut g, &mut b, "refill_1").is_err());
    assert_eq!(b.sends, 1);
}
#[test]
#[cfg(unix)]
fn forged_duplicate_or_regressing_budget_history_refuses_reopen() {
    for duplicate in [true, false] {
        let t = Temp::new();
        let p = refill_plan();
        let mut g = Governor::open(&t.path(), p.clone()).unwrap();
        let mut b = Fake::new(&p);
        continue_goal(&mut g, &mut b, "refill_0").unwrap();
        continue_goal(&mut g, &mut b, "refill_1").unwrap();
        drop(g);
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(t.path()).unwrap()).unwrap();
        if duplicate {
            let pair = value["journal"].as_array().unwrap()[0..2].to_vec();
            value["journal"].as_array_mut().unwrap().extend(pair);
        } else {
            value["journal"][2]["before_sequence"] = value["journal"][0]["before_sequence"].clone();
            value["journal"][3]["before_sequence"] = value["journal"][0]["before_sequence"].clone();
        }
        fs::write(t.path(), serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(Governor::open(&t.path(), p).is_err());
    }
}

#[test]
#[cfg(unix)]
fn same_valid_range_forged_goal_day_cannot_release_budget() {
    let t = Temp::new();
    let p = refill_plan();
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    let mut b = Fake::new(&p);
    b.day(2);
    continue_goal(&mut g, &mut b, "refill_0").unwrap();
    drop(g);
    let mut value: serde_json::Value =
        serde_json::from_slice(&fs::read(t.path()).unwrap()).unwrap();
    for record in value["journal"].as_array_mut().unwrap() {
        record["day"] = serde_json::json!(1936 * 365);
    }
    value["entries"]["refill_0"]["attempt"]["day"] = serde_json::json!(1936 * 365);
    fs::write(t.path(), serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(Governor::open(&t.path(), p).is_err());
}
#[test]
#[cfg(unix)]
fn legacy_unbound_day_is_conservatively_exhausted_until_fresh_advance() {
    let t = Temp::new();
    let p = refill_plan();
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    let mut b = Fake::new(&p);
    continue_goal(&mut g, &mut b, "refill_0").unwrap();
    let old = request_id_revision(&p, &p.entries[0], 1, p.revision).unwrap();
    for r in &mut g.state.journal {
        r.day_bound = false;
        r.request_id = old.clone();
    }
    if let EntryState::Complete { attempt, .. } = g.state.entries.get_mut("refill_0").unwrap() {
        attempt.request_id = old;
    }
    g.persist().unwrap();
    drop(g);
    let mut g = Governor::open(&t.path(), p).unwrap();
    assert!(!g.budget_available(1936 * 365));
    let (_, budget) = g.stopped_history().unwrap();
    assert_eq!(budget["used_assignment_attempts"], 1);
    assert_eq!(budget["remaining"], 0); // Legacy day is blocked, not limit - used.
    assert!(continue_goal(&mut g, &mut b, "refill_1")
        .unwrap()
        .verified
        .is_empty());
    assert_eq!(b.sends, 1);
    b.day(2);
    continue_goal(&mut g, &mut b, "refill_1").unwrap();
    assert_eq!(b.sends, 2);
}

#[cfg(unix)]
fn ordered_refill_plan() -> Plan {
    let mut p = plan();
    p.entries
        .retain(|e| matches!(e.intent, Intent::Research { .. }));
    p.entries.push(Entry {
        id: "slot0-next".into(),
        intent: Intent::Research {
            technology_id: "construction1".into(),
            slot: 0,
        },
        navigation: None,
    });
    p.entries.push(Entry {
        id: "slot1-head".into(),
        intent: Intent::Research {
            technology_id: "basic_machine_tools".into(),
            slot: 1,
        },
        navigation: None,
    });
    p
}
#[cfg(unix)]
struct QueueFake {
    inner: Fake,
    occupied: std::collections::BTreeSet<u32>,
    order: Vec<String>,
}
#[cfg(unix)]
impl Backend for QueueFake {
    fn unresolved_input(&mut self) -> Result<bool> {
        self.inner.unresolved_input()
    }
    fn observe(&mut self) -> Result<Observation> {
        self.inner.observe()
    }
    fn readiness(&mut self, entry: &Entry, _: &Observation) -> Result<Readiness> {
        let Intent::Research { slot, .. } = entry.intent else {
            panic!("research fixture only")
        };
        Ok(if self.occupied.contains(&slot) {
            Readiness::WaitingForResearchSlot
        } else {
            Readiness::Ready
        })
    }
    fn attempt(
        &mut self,
        a: &Attempt,
        observed: &Observation,
        deadline: Instant,
    ) -> Result<Outcome> {
        let Intent::Research { slot, .. } = a.entry.intent else {
            panic!("research fixture only")
        };
        self.occupied.insert(slot);
        self.order.push(a.entry.id.clone());
        self.inner.attempt(a, observed, deadline)
    }
}
#[test]
#[cfg(unix)]
fn refill_waiting_head_blocks_later_but_other_slot_progresses_then_empty_same_day() {
    let t = Temp::new();
    let p = ordered_refill_plan();
    let mut b = QueueFake {
        inner: Fake::new(&p),
        occupied: [0].into_iter().collect(),
        order: vec![],
    };
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    let out = g
        .run_day(&mut b, 10000, Instant::now() + Duration::from_secs(1))
        .unwrap();
    assert_eq!(out.verified, vec!["slot1-head"]);
    assert_eq!(b.order, vec!["slot1-head"]);
    assert_eq!(
        g.pending_research(1936 * 365)
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        vec!["research-1"]
    );
    assert!(g
        .continue_entry(
            &mut b,
            "slot0-next",
            10000,
            Instant::now() + Duration::from_secs(1)
        )
        .unwrap()
        .verified
        .is_empty());
    assert_eq!(b.inner.sends, 1);
    b.occupied.remove(&0);
    assert_eq!(
        g.continue_entry(
            &mut b,
            "research-1",
            10000,
            Instant::now() + Duration::from_secs(1)
        )
        .unwrap()
        .verified,
        vec!["research-1"]
    );
    assert_eq!(g.used_actions(1936 * 365), 2);
    let completed = serde_json::to_value(g.entry_state("research-1")).unwrap();
    assert!(g
        .continue_entry(
            &mut b,
            "slot0-next",
            10000,
            Instant::now() + Duration::from_secs(1)
        )
        .unwrap()
        .verified
        .is_empty());
    b.occupied.remove(&0);
    assert_eq!(
        g.continue_entry(
            &mut b,
            "slot0-next",
            10000,
            Instant::now() + Duration::from_secs(1)
        )
        .unwrap()
        .verified,
        vec!["slot0-next"]
    );
    assert_eq!(b.order, vec!["slot1-head", "research-1", "slot0-next"]);
    assert_eq!(
        serde_json::to_value(g.entry_state("research-1")).unwrap(),
        completed
    );
    assert_eq!(g.used_actions(1936 * 365), 3);
    drop(g);
    let mut g = Governor::open(&t.path(), p).unwrap();
    b.inner.day(2);
    assert!(g
        .run_day(&mut b, 10000, Instant::now() + Duration::from_secs(1))
        .unwrap()
        .verified
        .is_empty());
    assert_eq!(b.inner.sends, 3);
}
#[test]
#[cfg(unix)]
fn refill_update_restart_preserves_budget_and_future_day_activation() {
    let t = Temp::new();
    let mut p = ordered_refill_plan();
    p.max_actions_per_day = 1;
    let mut b = QueueFake {
        inner: Fake::new(&p),
        occupied: std::collections::BTreeSet::new(),
        order: vec![],
    };
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    g.run_day(&mut b, 10000, Instant::now() + Duration::from_secs(1))
        .unwrap();
    let completed = serde_json::to_value(g.entry_state("research-1")).unwrap();
    p.revision += 1;
    p.entries.push(Entry {
        id: "slot0-third".into(),
        intent: Intent::Research {
            technology_id: "electronic_mechanical_engineering".into(),
            slot: 0,
        },
        navigation: None,
    });
    assert!(g.update_plan(p.clone()).unwrap());
    assert!(!g.update_plan(p.clone()).unwrap());
    drop(g);
    let mut g = Governor::open(&t.path(), p).unwrap();
    b.occupied.clear();
    assert!(!g.budget_available(1936 * 365));
    assert!(g
        .continue_entry(
            &mut b,
            "slot0-next",
            10000,
            Instant::now() + Duration::from_secs(1)
        )
        .unwrap()
        .verified
        .is_empty());
    b.inner.day(2);
    assert_eq!(
        g.run_day(&mut b, 10000, Instant::now() + Duration::from_secs(1))
            .unwrap()
            .verified,
        vec!["slot0-next"]
    );
    assert_eq!(g.used_actions(1936 * 365), 1);
    assert_eq!(g.used_actions(1936 * 365 + 1), 1);
    assert_eq!(
        serde_json::to_value(g.entry_state("research-1")).unwrap(),
        completed
    );
    assert_eq!(b.order, vec!["research-1", "slot0-next"]);
}
#[test]
#[cfg(unix)]
fn refill_duplicate_technology_and_slot_reordering_or_prepend_refuse() {
    let t = Temp::new();
    let p = ordered_refill_plan();
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    let mut duplicate = p.clone();
    duplicate.revision += 1;
    duplicate.entries.push(Entry {
        id: "conflicting-other-slot".into(),
        intent: Intent::Research {
            technology_id: "support_weapons".into(),
            slot: 3,
        },
        navigation: None,
    });
    assert_eq!(g.update_plan(duplicate).unwrap_err().code, "invalid_plan");
    let mut reorder = p.clone();
    reorder.revision += 1;
    reorder.entries.swap(0, 1);
    assert_eq!(g.update_plan(reorder).unwrap_err().code, "plan_conflict");
    let mut prepend = p.clone();
    prepend.revision += 1;
    prepend.entries.insert(
        0,
        Entry {
            id: "new-before-head".into(),
            intent: Intent::Research {
                technology_id: "electronic_mechanical_engineering".into(),
                slot: 0,
            },
            navigation: None,
        },
    );
    assert_eq!(g.update_plan(prepend).unwrap_err().code, "plan_conflict");
    let mut cross_slot_order = p;
    cross_slot_order.revision += 1;
    cross_slot_order.entries.swap(1, 2);
    assert!(g.update_plan(cross_slot_order).unwrap());
}
#[test]
#[cfg(unix)]
fn refused_head_does_not_expose_later_same_slot_on_same_or_next_day() {
    let t = Temp::new();
    let p = ordered_refill_plan();
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    if let Some(EntryState::Pending { reason_day, .. }) = g.state.entries.get_mut("research-1") {
        *reason_day = Some(1936 * 365);
    }
    assert_eq!(
        g.pending_research(1936 * 365)
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        vec!["slot1-head"]
    );
    assert_eq!(
        g.pending_research(1936 * 365 + 1)
            .iter()
            .map(|e| e.id.as_str())
            .collect::<Vec<_>>(),
        vec!["research-1", "slot1-head"]
    );
}

#[test]
#[cfg(unix)]
fn focus_ordered_head_update_and_completed_assignment_preserved() {
    let t = Temp::new();
    let mut p = plan();
    p.entries
        .retain(|e| matches!(e.intent, Intent::Focus { .. }));
    p.entries.push(Entry {
        id: "focus-next".into(),
        intent: Intent::Focus {
            focus_id: "GER_autarky".into(),
        },
        navigation: None,
    });
    let mut b = Fake::new(&p);
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    assert_eq!(g.pending_focus(1936 * 365).unwrap().id, "focus-1");
    b.ready = Readiness::Deferred("Unknown active state".into());
    run(&mut g, &mut b).unwrap();
    assert_eq!(b.sends, 0);
    assert!(g.pending_focus(1936 * 365).is_none());
    assert_eq!(g.pending_focus(1936 * 365 + 1).unwrap().id, "focus-1");
    let mut reordered = p.clone();
    reordered.revision += 1;
    reordered.entries.swap(0, 1);
    assert_eq!(g.update_plan(reordered).unwrap_err().code, "plan_conflict");
    b.day(2);
    b.ready = Readiness::Ready;
    run(&mut g, &mut b).unwrap();
    let complete = serde_json::to_value(g.entry_state("focus-1")).unwrap();
    assert_eq!(b.sends, 2);
    p.revision += 1;
    p.entries.push(Entry {
        id: "focus-third".into(),
        intent: Intent::Focus {
            focus_id: "GER_four_year_plan".into(),
        },
        navigation: None,
    });
    assert!(g.update_plan(p.clone()).unwrap());
    drop(g);
    let g = Governor::open(&t.path(), p).unwrap();
    assert_eq!(
        serde_json::to_value(g.entry_state("focus-1")).unwrap(),
        complete
    );
    assert_eq!(g.used_actions(1936 * 365 + 1), 2);
    assert_eq!(g.pending_focus(1936 * 365 + 1).unwrap().id, "focus-third");
}

#[test]
fn focus_converter_requires_current_start_exact_active_and_full_domain_scope() {
    use crate::focus_stage::{self as f, ActiveFocus, LocalRoot};
    let p = plan();
    let entry = p.entries[1].clone();
    let attempt = Attempt {
        request_id: "focus-start-current".into(),
        entry,
        scope: p.scope.clone(),
        day: 1936 * 365,
        before_sequence: 5,
    };
    let date = crate::calendar::encode(1936, 1, 1).unwrap();
    let before = crate::own_focus::FocusContext {
        campaign: crate::game::ClockContext {
            playthrough_id: "game-campaign".into(),
            player: "GER".into(),
            anchors: p.scope.anchors,
            date_raw: date,
            difficulty: 2,
            historical_raw: 1,
            paused: true,
            speed: 1,
        },
        anchors: [
            p.scope.own_country,
            0x60000,
            0x70000,
            0x80000,
            0x90000,
            0xa0000,
            p.scope.own_status,
        ],
        slots: vec![0xb0000],
        active_focus: 0,
        active_key: None,
        selected: vec![],
        matching_details: vec![],
        ui_facts: None,
        search_anchors: None,
    };
    let mut after = before.clone();
    after.active_focus = 0xc0000;
    after.active_key = Some("GER_rhineland".into());
    let obs = Observation {
        scope: p.scope.clone(),
        date_raw: date,
        sequence: 6,
        observed_unix_ms: 9999,
        source: "generated fresh owned".into(),
        counts: OwnCounts::default(),
    };
    let mut execution = f::Execution {
        receipt: f::Receipt {
            version: 1,
            request_id: attempt.request_id.clone(),
            target: f::Target {
                epoch: p.scope.epoch.clone(),
                stage: f::Stage::Start,
                focus_id: "GER_rhineland".into(),
            },
            status: f::Status::Verified,
            delivery: Some(f::Delivery::ServerConfirmed),
            observed: f::OwnState {
                epoch: p.scope.epoch,
                active_focus: ActiveFocus::Active("GER_rhineland".into()),
                canonical_focus_id: Some("GER_rhineland".into()),
                managed_mode: Some(13),
                politics_root: LocalRoot::Hidden,
                focus_root: LocalRoot::Ready,
                own_detail_counts: Some([0, 0]),
                own_research_info_counts: Some([0, 0]),
                research_panel_root: LocalRoot::Unknown,
                research_tree_root: LocalRoot::Unknown,
                selected_focus: None,
                requested_detail_ready: Some(false),
                search_control: None,
            },
        },
        historical: false,
        current_state_verified: true,
        event_causality_verified: false,
    };
    VerifiedReceipt::from_focus(attempt.clone(), &execution, &before, &after, obs.clone()).unwrap();
    execution.historical = true;
    assert!(
        VerifiedReceipt::from_focus(attempt.clone(), &execution, &before, &after, obs.clone())
            .is_err()
    );
    execution.historical = false;
    for stage in [f::Stage::OpenPolitics, f::Stage::OpenFocus, f::Stage::View] {
        execution.receipt.target.stage = stage;
        assert!(VerifiedReceipt::from_focus(
            attempt.clone(),
            &execution,
            &before,
            &after,
            obs.clone()
        )
        .is_err());
    }
    execution.receipt.target.stage = f::Stage::Start;
    execution.receipt.observed.active_focus = ActiveFocus::Unknown;
    assert!(
        VerifiedReceipt::from_focus(attempt.clone(), &execution, &before, &after, obs.clone())
            .is_err()
    );
    execution.receipt.observed.active_focus = ActiveFocus::Active("GER_other".into());
    assert!(
        VerifiedReceipt::from_focus(attempt.clone(), &execution, &before, &after, obs.clone())
            .is_err()
    );
    execution.receipt.observed.active_focus = ActiveFocus::Active("GER_rhineland".into());
    for i in 0..7 {
        let mut changed = after.clone();
        changed.anchors[i] += 8;
        assert!(VerifiedReceipt::from_focus(
            attempt.clone(),
            &execution,
            &before,
            &changed,
            obs.clone()
        )
        .is_err());
    }
    let mut stale = obs;
    stale.sequence = 5;
    assert!(VerifiedReceipt::from_focus(attempt, &execution, &before, &after, stale).is_err());
}

#[test]
#[cfg(unix)]
fn focus_wait_never_consumes_budget_or_day_and_update_restart_keeps_original_limit() {
    let t = Temp::new();
    let mut p = plan();
    p.entries
        .retain(|e| matches!(e.intent, Intent::Focus { .. }));
    p.max_actions_per_day = 1;
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    let mut b = Fake::new(&p);
    b.ready = Readiness::WaitingForFocus;
    run(&mut g, &mut b).unwrap();
    assert_eq!(b.sends, 0);
    assert_eq!(g.used_actions(1936 * 365), 0);
    assert!(g.pending_focus(1936 * 365).is_some());
    g.continue_entry(
        &mut b,
        "focus-1",
        10000,
        Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(b.sends, 0);
    b.ready = Readiness::Ready;
    g.continue_entry(
        &mut b,
        "focus-1",
        10000,
        Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(b.sends, 1);
    let complete = serde_json::to_value(g.entry_state("focus-1")).unwrap();
    p.revision = 2;
    p.entries.push(Entry {
        id: "focus-next".into(),
        intent: Intent::Focus {
            focus_id: "GER_autarky".into(),
        },
        navigation: None,
    });
    assert!(g.update_plan(p.clone()).unwrap());
    assert!(!g.update_plan(p.clone()).unwrap());
    drop(g);
    let mut g = Governor::open(&t.path(), p).unwrap();
    assert_eq!(g.used_actions(1936 * 365), 1);
    assert!(!g.budget_available(1936 * 365));
    assert_eq!(g.pending_focus(1936 * 365).unwrap().id, "focus-next");
    b.ready = Readiness::WaitingForFocus;
    g.continue_entry(
        &mut b,
        "focus-next",
        10000,
        Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(b.sends, 1);
    assert_eq!(
        serde_json::to_value(g.entry_state("focus-1")).unwrap(),
        complete
    );
}

#[test]
#[cfg(unix)]
fn assignment_summary_survives_empty_batch_update_restart_without_fresh_authority() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    assert_eq!(g.assignment_summary().unwrap()["completed"], 0);
    assert_eq!(run(&mut g, &mut b).unwrap().verified.len(), 2);
    let summary = g.assignment_summary().unwrap();
    assert_eq!(summary["completed"], 2);
    assert_eq!(summary["pending"], 0);
    assert_eq!(summary["latest_completion"]["entry_id"], "focus-1");
    assert_eq!(summary["latest_completion"]["state"], "Complete");
    assert_eq!(summary["current_state_verified"], false);
    assert_eq!(summary["input_authorized"], false);
    let latest = summary["latest_completion"].clone();
    assert!(run(&mut g, &mut b).unwrap().verified.is_empty());
    assert_eq!(g.assignment_summary().unwrap()["latest_completion"], latest);
    let next = extended(p);
    assert!(g.update_plan(next.clone()).unwrap());
    let updated = g.assignment_summary().unwrap();
    assert_eq!(updated["plan_revision"], 2);
    assert_eq!(updated["completed"], 2);
    assert_eq!(updated["pending"], 1);
    assert_eq!(updated["latest_completion"], latest);
    drop(g);
    let mut g = Governor::open(&t.path(), next).unwrap();
    assert_eq!(g.assignment_summary().unwrap(), updated);
    assert_eq!(
        g.used_actions((i64::from(b.obs.date_raw) - 43_800_000).div_euclid(24)),
        2
    );
    assert_eq!(b.sends, 2);
}

#[test]
#[cfg(unix)]
fn stopped_history_survives_immediate_restart_without_worker_report_or_game_read() {
    let t = Temp::new();
    let mut p = plan();
    p.max_actions_per_day = 1;
    let mut b = Fake::new(&p);
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    run(&mut g, &mut b).unwrap();
    let next = extended(p);
    g.update_plan(next.clone()).unwrap();
    assert!(!g.update_plan(next.clone()).unwrap());
    let expected = g.stopped_history().unwrap();
    let (assignments, budget) = &expected;
    assert_eq!(assignments["completed"], 1);
    assert_eq!(assignments["pending"], 2);
    assert_eq!(budget["plan_revision"], 2);
    assert_eq!(budget["day"], 1936 * 365);
    assert_eq!(budget["used_assignment_attempts"], 1);
    assert_eq!(budget["remaining"], 0);
    assert_eq!(budget["historical"], true);
    assert_eq!(budget["current_state_verified"], false);
    assert_eq!(budget["input_authorized"], false);
    let reads = b.observes;
    let bytes = fs::read(t.path()).unwrap();
    for _ in 0..2 {
        drop(g);
        g = Governor::open(&t.path(), next.clone()).unwrap();
        assert_eq!(g.stopped_history().unwrap(), expected);
    }
    assert_eq!(fs::read(t.path()).unwrap(), bytes);
    assert_eq!(b.observes, reads);
    assert_eq!(b.sends, 1);
}

#[test]
#[cfg(unix)]
fn stopped_history_before_first_daily_observation_does_not_invent_a_day_or_budget() {
    let t = Temp::new();
    let mut g = Governor::open(&t.path(), plan()).unwrap();
    let (assignments, budget) = g.stopped_history().unwrap();
    assert_eq!(assignments["pending"], 2);
    assert_eq!(budget["available"], true);
    assert_eq!(budget["limit"], 4);
    for key in [
        "day",
        "date_raw",
        "observed_sequence",
        "used_assignment_attempts",
        "remaining",
    ] {
        assert!(budget[key].is_null(), "{key} must remain unknown");
    }
}

#[test]
#[cfg(unix)]
fn stopped_history_counts_nonqueued_and_unresolved_reservations_without_refund() {
    for mode in [Mode::NotQueued, Mode::Uncertain] {
        let t = Temp::new();
        let mut p = plan();
        p.max_actions_per_day = 1;
        let mut b = Fake::new(&p);
        b.mode = mode;
        let mut g = Governor::open(&t.path(), p.clone()).unwrap();
        let _ = run(&mut g, &mut b);
        drop(g);
        let mut g = Governor::open(&t.path(), p).unwrap();
        let (assignments, budget) = g.stopped_history().unwrap();
        assert_eq!(assignments["completed"], 0);
        assert_eq!(budget["used_assignment_attempts"], 1);
        assert_eq!(budget["remaining"], 0);
        assert_eq!(budget["input_authorized"], false);
        assert_eq!(b.sends, 1);
    }
}

#[test]
#[cfg(unix)]
fn assignment_summary_never_promotes_nonqueued_or_unresolved_intent() {
    for mode in [Mode::NotQueued, Mode::Uncertain] {
        let t = Temp::new();
        let p = plan();
        let mut b = Fake::new(&p);
        b.mode = mode;
        let mut g = Governor::open(&t.path(), p).unwrap();
        let _ = run(&mut g, &mut b);
        let summary = g.assignment_summary().unwrap();
        assert_eq!(summary["completed"], 0);
        assert!(summary["latest_completion"].is_null());
        assert_eq!(summary["input_authorized"], false);
        assert_eq!(summary["current_state_verified"], false);
        match mode {
            Mode::NotQueued => assert_eq!(summary["pending"], 2),
            Mode::Uncertain => assert_eq!(summary["in_flight"], 1),
            _ => unreachable!(),
        }
    }
}

#[test]
#[cfg(unix)]
fn assignment_summary_refuses_replaced_store_and_stays_poisoned() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    let mut g = Governor::open(&t.path(), p).unwrap();
    run(&mut g, &mut b).unwrap();
    let old = t.root.join("old.json");
    fs::rename(t.path(), &old).unwrap();
    fs::write(t.path(), "{}").unwrap();
    fs::set_permissions(t.path(), fs::Permissions::from_mode(0o600)).unwrap();
    assert!(g.assignment_summary().is_err());
    assert_eq!(
        g.stopped_history().unwrap_err().code,
        "governor_quarantined"
    );
    fs::remove_file(t.path()).unwrap();
    fs::rename(old, t.path()).unwrap();
    assert_eq!(
        g.assignment_summary().unwrap_err().code,
        "governor_quarantined"
    );
}

#[test]
#[cfg(unix)]
fn stopped_history_refuses_changed_store_before_returning_budget_and_stays_poisoned() {
    let t = Temp::new();
    let p = plan();
    let mut b = Fake::new(&p);
    let mut g = Governor::open(&t.path(), p).unwrap();
    run(&mut g, &mut b).unwrap();
    let original = fs::read(t.path()).unwrap();
    fs::write(t.path(), "{}").unwrap();
    assert_eq!(g.stopped_history().unwrap_err().code, "plan_changed");
    fs::write(t.path(), original).unwrap();
    assert_eq!(
        g.stopped_history().unwrap_err().code,
        "governor_quarantined"
    );
    assert!(g.assignment_summary().is_err());
}

#[test]
#[cfg(unix)]
fn assignment_summary_bounds_ids_but_keeps_latest_of_256_actual_completions() {
    let t = Temp::new();
    let mut p = plan();
    p.review_day = None;
    p.max_actions_per_day = 16;
    p.max_observation_age_ms = 5000;
    p.entries = (0..256)
        .map(|i| Entry {
            id: format!("focus-{i}-{}", "x".repeat(90)),
            intent: Intent::Focus {
                focus_id: format!("GER_generated_{i}"),
            },
            navigation: None,
        })
        .collect();
    let mut b = Fake::new(&p);
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    for (i, entry) in p.entries.iter().enumerate() {
        b.day((i / 16 + 1) as i64);
        g.continue_entry(
            &mut b,
            &entry.id,
            10000,
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap();
    }
    let summary = g.assignment_summary().unwrap();
    assert_eq!(b.sends, 256);
    assert_eq!(summary["total"], 256);
    assert_eq!(summary["completed"], 256);
    assert_eq!(summary["completed_ids"].as_array().unwrap().len(), 4);
    assert_eq!(summary["completed_ids_truncated"], true);
    assert_eq!(summary["latest_completion"]["entry_id"], p.entries[255].id);
    assert!(serde_json::to_vec(&summary).unwrap().len() < 1400);
    assert!(!serde_json::to_string(&summary)
        .unwrap()
        .contains("/owned/hoi4"));
}

#[test]
#[cfg(unix)]
fn digest_reserves_unfinished_count_before_long_verified_ids() {
    let t = Temp::new();
    let p = plan();
    let b = Fake::new(&p);
    let g = Governor::open(&t.path(), p).unwrap();
    let ids = (0..16)
        .map(|i| format!("generated-{i}-{}", "x".repeat(110)))
        .collect::<Vec<_>>();
    let digest = g.digest(&b.obs, 10000, &ids, None).unwrap();
    assert!(serde_json::to_vec(&digest).unwrap().len() <= 2048);
    assert_eq!(digest["retained_count"], 2);
    assert!(digest["verified"].as_array().unwrap().len() < ids.len());
    assert_eq!(digest["input_authorized"], false);
}

#[test]
#[cfg(unix)]
fn refused_reason_survives_later_completion_update_and_restart() {
    let t = Temp::new();
    let mut p = plan();
    p.entries.truncate(1);
    p.max_actions_per_day = 1;
    let mut b = Fake::new(&p);
    b.mode = Mode::NotQueued;
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    run(&mut g, &mut b).unwrap();
    let refused_day = g.state.last_day.unwrap();
    let first = g.assignment_summary().unwrap();
    assert_eq!(first["not_queued_attempts"], 1);
    let refusal = first["latest_not_queued"].clone();
    assert_eq!(refusal["reason"], "local refusal; no input queued");
    assert_eq!(g.used_actions(refused_day), 1);
    drop(g);
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    b.mode = Mode::Verified;
    b.day(2);
    run(&mut g, &mut b).unwrap();
    assert_eq!(b.sends, 2);
    assert_eq!(g.assignment_summary().unwrap()["completed"], 1);
    assert_eq!(
        g.assignment_summary().unwrap()["latest_not_queued"],
        refusal
    );
    assert_eq!(g.used_actions(refused_day), 1);
    assert_eq!(g.used_actions(refused_day + 1), 1);
    p.revision += 1;
    g.update_plan(p.clone()).unwrap();
    let expected = g.stopped_history().unwrap();
    drop(g);
    let mut g = Governor::open(&t.path(), p).unwrap();
    assert_eq!(g.stopped_history().unwrap(), expected);
    assert_eq!(expected.0["latest_not_queued"], refusal);
    run(&mut g, &mut b).unwrap();
    assert_eq!(b.sends, 2);
    assert_eq!(g.state.journal.len(), 4);
}

#[test]
#[cfg(unix)]
fn legacy_refusal_reason_remains_unknown_without_reconstruction() {
    let t = Temp::new();
    let mut p = plan();
    p.entries.truncate(1);
    let mut b = Fake::new(&p);
    b.mode = Mode::NotQueued;
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    run(&mut g, &mut b).unwrap();
    drop(g);
    let mut saved: Value = serde_json::from_slice(&fs::read(t.path()).unwrap()).unwrap();
    saved["journal"][1]
        .as_object_mut()
        .unwrap()
        .remove("not_queued_reason");
    fs::write(t.path(), serde_json::to_vec(&saved).unwrap()).unwrap();
    let mut g = Governor::open(&t.path(), p.clone()).unwrap();
    let summary = g.assignment_summary().unwrap();
    assert_eq!(summary["not_queued_attempts"], 1);
    assert!(summary["latest_not_queued"]["reason"].is_null());
    // Even the still-present Pending reason cannot prove the original cause.
    assert!(matches!(
        g.entry_state("research-1"),
        Some(EntryState::Pending {
            reason: Some(_),
            ..
        })
    ));
    b.mode = Mode::Verified;
    b.day(2);
    run(&mut g, &mut b).unwrap();
    drop(g);
    let mut g = Governor::open(&t.path(), p).unwrap();
    assert_eq!(g.assignment_summary().unwrap()["completed"], 1);
    assert!(g.assignment_summary().unwrap()["latest_not_queued"]["reason"].is_null());
}

#[test]
#[cfg(unix)]
fn refuse_malformed_or_wrong_outcome_historical_reasons() {
    for (mode, row, bad) in [
        (Mode::NotQueued, 0, "wrong intent".to_string()),
        (Mode::Verified, 1, "wrong success".to_string()),
        (Mode::NotQueued, 1, "x".repeat(161)),
        (Mode::NotQueued, 1, "界".repeat(161)),
        (Mode::NotQueued, 1, "bad\ncontrol".to_string()),
    ] {
        let t = Temp::new();
        let mut p = plan();
        p.entries.truncate(1);
        let mut b = Fake::new(&p);
        b.mode = mode;
        let mut g = Governor::open(&t.path(), p.clone()).unwrap();
        run(&mut g, &mut b).unwrap();
        drop(g);
        let mut saved: Value = serde_json::from_slice(&fs::read(t.path()).unwrap()).unwrap();
        saved["journal"][row]["not_queued_reason"] = json!(bad);
        fs::write(t.path(), serde_json::to_vec(&saved).unwrap()).unwrap();
        assert!(Governor::open(&t.path(), p).is_err());
    }
}
