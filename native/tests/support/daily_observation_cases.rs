#[cfg(unix)]
mod daily_producer_cases {
    use super::*;
    use serde_json::Value;
    use hoi4_live_native::{actions::Epoch, daily_observation::Producer, platform::Identity};
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
        time::{Duration, Instant},
    };
    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let p = std::env::temp_dir().join(format!(
                "private-daily-producer-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&p).unwrap();
            #[cfg(unix)]
            fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
            Self(p)
        }
        fn path(&self) -> PathBuf {
            self.0.join("sequence.json")
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    struct Bound {
        f: Fixture,
        id: Identity,
        delay: Cell<bool>,
        changed: Cell<bool>,
        check_fail: Cell<bool>,
        reads: Cell<usize>,
    }
    impl Bound {
        fn new() -> Self {
            let f = owned_research_fixture();
            Self {
                f,
                id: Identity {
                    pid: 120,
                    start_ticks: "456".into(),
                    exe: "/generated/owned-game".into(),
                    uid: 1000,
                },
                delay: Cell::new(false),
                changed: Cell::new(false),
                check_fail: Cell::new(false),
                reads: Cell::new(0),
            }
        }
        fn epoch(&self) -> Epoch {
            Epoch {
                game: self.id.clone(),
                campaign: "fixture-epoch".into(),
                player: "GER".into(),
            }
        }
    }
    impl ReadMemory for Bound {
        fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
            self.reads.set(self.reads.get() + 1);
            if self.delay.get() {
                std::thread::sleep(Duration::from_millis(20));
            }
            if self.changed.get() && a == 0x30460 {
                return Ok(60759372i32.to_le_bytes().to_vec());
            }
            self.f.read(a, n)
        }
    }
    impl Process for Bound {
        fn image_base(&self) -> u64 {
            self.f.image_base()
        }
        fn identity(&self) -> Option<&Identity> {
            Some(&self.id)
        }
        fn check(&self) -> Result<()> {
            if self.check_fail.get() {
                Err(Error::new("ownership_changed", "Generated replacement"))
            } else {
                self.f.check()
            }
        }
    }
    fn until() -> Instant {
        Instant::now() + Duration::from_secs(2)
    }
    #[test]
    #[cfg(unix)]
    fn actual_canonical_producer_persists_sequence_scope_and_unknown_counts() {
        let b = Bound::new();
        let s = Scratch::new();
        let expected = game::clock_context(&b).unwrap();
        let producer = Producer::open(&b, b.epoch(), expected.clone(), &s.path(), until()).unwrap();
        let p = producer.observe(&b, until()).unwrap();
        assert_eq!(p.observation.sequence, 1);
        assert_eq!(p.observation.date_raw, 60759371);
        assert_eq!(p.observation.counts.idle_research_slots, Some(3));
        assert_eq!(p.observation.counts.idle_civilian_factories, None);
        assert_eq!(p.canonical["slots"][0]["researching"], "tools1");
        let stored: Value = serde_json::from_slice(&fs::read(s.path()).unwrap()).unwrap();
        assert_eq!(stored["sequence"], 1);
        drop(producer);
        let producer = Producer::open(&b, b.epoch(), expected, &s.path(), until()).unwrap();
        assert_eq!(
            producer.observe(&b, until()).unwrap().observation.sequence,
            2
        );
    }
    #[test]
    #[cfg(unix)]
    fn late_blocking_read_never_advances_sequence_or_publishes() {
        let b = Bound::new();
        let s = Scratch::new();
        let producer = Producer::open(
            &b,
            b.epoch(),
            game::clock_context(&b).unwrap(),
            &s.path(),
            until(),
        )
        .unwrap();
        let old = fs::read(s.path()).unwrap();
        b.delay.set(true);
        assert_eq!(
            producer
                .observe(&b, Instant::now() + Duration::from_millis(5))
                .err()
                .unwrap()
                .code,
            "daily_deadline"
        );
        assert_eq!(fs::read(s.path()).unwrap(), old);
    }
    #[test]
    #[cfg(unix)]
    fn replaced_process_refuses_before_payload_and_sequence_advance() {
        let mut b = Bound::new();
        let s = Scratch::new();
        let producer = Producer::open(
            &b,
            b.epoch(),
            game::clock_context(&b).unwrap(),
            &s.path(),
            until(),
        )
        .unwrap();
        let before = b.reads.get();
        b.id.start_ticks = "457".into();
        assert_eq!(
            producer.observe(&b, until()).err().unwrap().code,
            "ownership_changed"
        );
        assert_eq!(b.reads.get(), before);
        let stored: Value = serde_json::from_slice(&fs::read(s.path()).unwrap()).unwrap();
        assert_eq!(stored["sequence"], 0);
    }
    #[test]
    #[cfg(unix)]
    fn context_clock_slot_and_owner_changes_refuse_without_publication() {
        for (address, bytes) in [
            (0x30460, 60759370i32.to_le_bytes().to_vec()),
            (0x28130, 0x40008u64.to_le_bytes().to_vec()),
            (0x2c018, 0x81000u64.to_le_bytes().to_vec()),
        ] {
            let mut b = Bound::new();
            let s = Scratch::new();
            let producer = Producer::open(
                &b,
                b.epoch(),
                game::clock_context(&b).unwrap(),
                &s.path(),
                until(),
            )
            .unwrap();
            let old = fs::read(s.path()).unwrap();
            b.f.fault = Some((address, 1, bytes));
            b.f.fault_reads.set(0);
            assert!(producer.observe(&b, until()).is_err());
            assert_eq!(fs::read(s.path()).unwrap(), old);
        }
    }
    #[test]
    #[cfg(unix)]
    fn counter_mutation_private_scope_and_single_writer_refuse() {
        let b = Bound::new();
        let s = Scratch::new();
        let expected = game::clock_context(&b).unwrap();
        let producer = Producer::open(&b, b.epoch(), expected.clone(), &s.path(), until()).unwrap();
        assert_eq!(
            Producer::open(&b, b.epoch(), expected.clone(), &s.path(), until())
                .err()
                .unwrap()
                .code,
            "plan_busy"
        );
        fs::write(s.path(), b"poison").unwrap();
        assert_eq!(
            producer.observe(&b, until()).err().unwrap().code,
            "plan_changed"
        );
        drop(producer);
        assert!(Producer::open(&b, b.epoch(), expected, &s.path(), until()).is_err());
    }
    #[test]
    #[cfg(unix)]
    fn final_process_check_and_nonpositive_own_scope_are_not_metadata_success() {
        let b = Bound::new();
        let s = Scratch::new();
        let producer = Producer::open(
            &b,
            b.epoch(),
            game::clock_context(&b).unwrap(),
            &s.path(),
            until(),
        )
        .unwrap();
        b.check_fail.set(true);
        assert_eq!(
            producer.observe(&b, until()).err().unwrap().code,
            "ownership_changed"
        );
        let mut b = Bound::new();
        let expected = game::clock_context(&b).unwrap();
        b.f.number(0x30518, 0);
        assert_eq!(
            Producer::open(&b, b.epoch(), expected, &s.0.join("other.json"), until())
                .err()
                .unwrap()
                .code,
            "player_unavailable"
        );
    }

    #[test]
    fn unchanged_clock_probes_never_publish_sequences_or_rewrite_state() {
        let b=Bound::new();let s=Scratch::new();let producer=Producer::open(&b,b.epoch(),game::clock_context(&b).unwrap(),&s.path(),until()).unwrap();let old=fs::read(s.path()).unwrap();
        for _ in 0..3 {assert_eq!(producer.probe_clock(&b,until()).unwrap(),60759371);}
        assert_eq!(fs::read(s.path()).unwrap(),old);b.changed.set(true);assert_eq!(producer.probe_clock(&b,until()).unwrap(),60759372);b.changed.set(false);
        assert_eq!(producer.observe(&b,until()).err().unwrap().code,"ownership_changed");assert_eq!(fs::read(s.path()).unwrap(),old);
    }
    #[test]
    fn foreign_own_map_is_refused_before_country_payload() {
        let mut b=Bound::new();let s=Scratch::new();let producer=Producer::open(&b,b.epoch(),game::clock_context(&b).unwrap(),&s.path(),until()).unwrap();b.f.word(0x3a010,0xdead0000);b.f.tracked_reads.borrow_mut().clear();
        assert_eq!(producer.probe_clock(&b,until()).err().unwrap().code,"ownership_changed");assert!(!b.f.tracked_reads.borrow().keys().any(|a|*a>=0xdead0000&&*a<0xdead1000));
    }
}
