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
            },
            Entry {
                id: "focus-1".into(),
                intent: Intent::Focus {
                    focus_id: "GER_rhineland".into(),
                },
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
        },
        Entry {
            id: "army".into(),
            intent: Intent::Army {
                division_ids: vec![1, 2],
                army_id: 3,
                order_id: "defend_home".into(),
            },
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
