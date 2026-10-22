//! One original action deadline and lease across every nested read and input.
//! Existing UI stage timers may narrow this bound, but cannot renew it.
use crate::{
    actions::Epoch,
    error::{Error, Result},
    game::{self, ClockContext},
    input_lease::InputAuthority,
    memory::{ReadMemory, StringAbi},
    platform::{Identity, Process},
};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

pub(super) struct Fence<'a> {
    pub process: &'a (dyn Process + Send + Sync),
    pub epoch: &'a Epoch,
    pub expected: &'a ClockContext,
    pub authority: &'a InputAuthority,
    pub token: &'a str,
    pub until: Instant,
    pub stop: Option<&'a AtomicBool>,
}
fn same_scope(before: &ClockContext, now: &ClockContext, epoch: &Epoch) -> Result<()> {
    if now.player != epoch.player
        || before.player != epoch.player
        || now.playthrough_id != before.playthrough_id
        || now.anchors != before.anchors
        || now.difficulty != before.difficulty
        || now.historical_raw != before.historical_raw
        || now.date_raw < before.date_raw
    {
        return Err(Error::new(
            "ownership_changed",
            "Action campaign, roots, setup or chronology changed",
        ));
    }
    Ok(())
}
impl Fence<'_> {
    fn live(&self) -> Result<()> {
        if self.stop.is_some_and(|s| s.load(Ordering::Acquire)) {
            return Err(Error::new("session_closed", "Owned action stop requested"));
        }
        if Instant::now() >= self.until {
            return Err(Error::new(
                "action_deadline",
                "Original action deadline expired",
            ));
        }
        self.authority.authorize(self.token, self.epoch)?;
        if self.process.identity() != Some(&self.epoch.game) {
            return Err(Error::new(
                "ownership_changed",
                "Action process identity differs",
            ));
        }
        Ok(())
    }
    /// Called before each new native input, including after cursor settling.
    /// Key/button release cleanup remains unconditional in the transport.
    pub(super) fn input_check(&self) -> Result<()> {
        self.live()?;
        let context = game::clock_context(self)?;
        same_scope(self.expected, &context, self.epoch)?;
        self.live()
    }
}
impl ReadMemory for Fence<'_> {
    fn read(&self, address: u64, size: usize) -> Result<Vec<u8>> {
        self.live()?;
        let bytes = self.process.read(address, size)?;
        self.live()?;
        Ok(bytes)
    }
    fn read_many(&self, requests: &[(u64, usize)]) -> Result<Vec<Vec<u8>>> {
        // Keep stop/deadline checks between logical payloads, not just once per
        // potentially large native batch. The wrapper is action-local only.
        crate::memory::validate_read_batch(requests)?;
        let mut results = Vec::with_capacity(requests.len());
        for &(address, size) in requests {
            let bytes = self.read(address, size)?;
            if bytes.len() != size {
                return Err(Error::new("short_read", "Incomplete batch read"));
            }
            results.push(bytes);
        }
        Ok(results)
    }
    fn string_abi(&self) -> StringAbi {
        self.process.string_abi()
    }
}
impl Process for Fence<'_> {
    fn check(&self) -> Result<()> {
        self.live()?;
        self.process.check()?;
        self.live()
    }
    fn begin_snapshot(&self) -> Result<()> {
        self.live()?;
        self.process.begin_snapshot()?;
        self.live()
    }
    fn image_base(&self) -> u64 {
        self.process.image_base()
    }
    fn identity(&self) -> Option<&Identity> {
        self.process.identity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::atomic::AtomicUsize, time::Duration};
    struct Fake {
        id: Identity,
        reads: AtomicUsize,
        cancel: AtomicBool,
    }
    impl ReadMemory for Fake {
        fn read(&self, _: u64, n: usize) -> Result<Vec<u8>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            self.cancel.store(true, Ordering::SeqCst);
            Ok(vec![0; n])
        }
        fn string_abi(&self) -> StringAbi {
            StringAbi::LibCxx
        }
    }
    impl Process for Fake {
        fn check(&self) -> Result<()> {
            Ok(())
        }
        fn image_base(&self) -> u64 {
            0x100000000
        }
        fn identity(&self) -> Option<&Identity> {
            Some(&self.id)
        }
    }
    fn context() -> ClockContext {
        ClockContext {
            playthrough_id: "fixture".into(),
            player: "GER".into(),
            anchors: [0x10000, 0x20000, 0x30000],
            date_raw: 60759624,
            difficulty: 2,
            historical_raw: 1,
            paused: true,
            speed: 1,
        }
    }
    #[test]
    fn action_scope_allows_time_progress_but_never_context_substitution() {
        let before = context();
        let epoch = Epoch {
            game: Identity {
                pid: 2,
                start_ticks: "1.000001".into(),
                exe: "/fixture".into(),
                uid: 501,
            },
            campaign: "token:fixture".into(),
            player: "GER".into(),
        };
        let mut now = before.clone();
        now.date_raw += 1;
        now.paused = false;
        now.speed = 4;
        same_scope(&before, &now, &epoch).unwrap();
        for mutate in [
            |c: &mut ClockContext| c.player = "ENG".into(),
            |c: &mut ClockContext| c.playthrough_id = "other".into(),
            |c: &mut ClockContext| c.anchors[1] += 8,
            |c: &mut ClockContext| c.date_raw -= 1,
            |c: &mut ClockContext| c.difficulty = 1,
            |c: &mut ClockContext| c.historical_raw = 0,
        ] {
            let mut c = before.clone();
            mutate(&mut c);
            assert_eq!(
                same_scope(&before, &c, &epoch).unwrap_err().code,
                "ownership_changed"
            );
        }
    }
    #[test]
    fn expired_and_cancelled_actions_never_read_and_midread_stop_discards_payload() {
        let epoch = Epoch {
            game: Identity {
                pid: 2,
                start_ticks: "1.000001".into(),
                exe: "/fixture".into(),
                uid: 501,
            },
            campaign: "token:fixture".into(),
            player: "GER".into(),
        };
        let fake = Fake {
            id: epoch.game.clone(),
            reads: AtomicUsize::new(0),
            cancel: AtomicBool::new(false),
        };
        let mut authority = InputAuthority::new("fixture".into(), epoch.clone()).unwrap();
        let token = "a".repeat(32);
        authority
            .acquire("fixture", token.clone(), 30, &epoch)
            .unwrap();
        let expected = context();
        let mut fence = Fence {
            process: &fake,
            epoch: &epoch,
            expected: &expected,
            authority: &authority,
            token: &token,
            until: Instant::now(),
            stop: Some(&fake.cancel),
        };
        assert_eq!(fence.read(0x10000, 1).unwrap_err().code, "action_deadline");
        assert_eq!(fake.reads.load(Ordering::SeqCst), 0);
        fence.until = Instant::now() + Duration::from_secs(1);
        assert_eq!(fence.string_abi(), StringAbi::LibCxx);
        assert_eq!(fence.read(0x10000, 1).unwrap_err().code, "session_closed");
        assert_eq!(fake.reads.load(Ordering::SeqCst), 1);
        assert_eq!(fence.read(0x10000, 1).unwrap_err().code, "session_closed");
        assert_eq!(fake.reads.load(Ordering::SeqCst), 1);
    }

    fn epoch_fixture() -> Epoch {
        Epoch {
            game: Identity {
                pid: 2,
                start_ticks: "1.000001".into(),
                exe: "/fixture".into(),
                uid: 501,
            },
            campaign: "token:fixture".into(),
            player: "GER".into(),
        }
    }

    fn leased_authority(epoch: &Epoch) -> (InputAuthority, String) {
        let mut authority = InputAuthority::new("fixture".into(), epoch.clone()).unwrap();
        let token = "a".repeat(32);
        authority
            .acquire("fixture", token.clone(), 30, epoch)
            .unwrap();
        (authority, token)
    }

    enum ReadEffect {
        Zeros,
        Short,
        Until(Instant),
    }

    struct Probe {
        id: Identity,
        reads: AtomicUsize,
        checks: AtomicUsize,
        snapshots: AtomicUsize,
        stop: AtomicBool,
        effect: ReadEffect,
        stop_after_check: bool,
        stop_after_snapshot: bool,
    }

    impl Probe {
        fn new(epoch: &Epoch) -> Self {
            Self {
                id: epoch.game.clone(),
                reads: AtomicUsize::new(0),
                checks: AtomicUsize::new(0),
                snapshots: AtomicUsize::new(0),
                stop: AtomicBool::new(false),
                effect: ReadEffect::Zeros,
                stop_after_check: false,
                stop_after_snapshot: false,
            }
        }
    }

    impl ReadMemory for Probe {
        fn read(&self, _: u64, size: usize) -> Result<Vec<u8>> {
            self.reads.fetch_add(1, Ordering::SeqCst);
            match self.effect {
                ReadEffect::Zeros => Ok(vec![0; size]),
                ReadEffect::Short => Ok(vec![0; size.saturating_sub(1)]),
                ReadEffect::Until(until) => {
                    std::thread::sleep(until.saturating_duration_since(Instant::now()));
                    Ok(vec![0; size])
                }
            }
        }

        fn string_abi(&self) -> StringAbi {
            StringAbi::LibCxx
        }
    }

    impl Process for Probe {
        fn check(&self) -> Result<()> {
            self.checks.fetch_add(1, Ordering::SeqCst);
            if self.stop_after_check {
                self.stop.store(true, Ordering::Release);
            }
            Ok(())
        }

        fn begin_snapshot(&self) -> Result<()> {
            self.snapshots.fetch_add(1, Ordering::SeqCst);
            if self.stop_after_snapshot {
                self.stop.store(true, Ordering::Release);
            }
            Ok(())
        }

        fn image_base(&self) -> u64 {
            0x100000000
        }

        fn identity(&self) -> Option<&Identity> {
            Some(&self.id)
        }
    }

    #[test]
    fn worker_fences_and_owned_input_bindings_are_send_and_sync() {
        fn require_send_sync<T: Send + Sync>() {}
        require_send_sync::<Fence<'static>>();
        require_send_sync::<crate::platform::macos_input::MacKeys<'static>>();
        require_send_sync::<crate::platform::macos::MacProcess>();
    }

    #[test]
    fn missing_or_wrong_authority_and_process_identity_refuse_before_reads() {
        let epoch = epoch_fixture();
        let expected = context();
        let process = Probe::new(&epoch);
        let missing = InputAuthority::new("fixture".into(), epoch.clone()).unwrap();
        let (authority, token) = leased_authority(&epoch);
        let wrong_token = "b".repeat(32);
        let mut stale_epoch = epoch.clone();
        stale_epoch.campaign = "other:fixture".into();
        for (candidate, candidate_token, candidate_epoch) in [
            (&missing, token.as_str(), &epoch),
            (&authority, wrong_token.as_str(), &epoch),
            (&authority, token.as_str(), &stale_epoch),
        ] {
            let fence = Fence {
                process: &process,
                epoch: candidate_epoch,
                expected: &expected,
                authority: candidate,
                token: candidate_token,
                until: Instant::now() + Duration::from_secs(5),
                stop: None,
            };
            assert_eq!(
                fence.read(0x10000, 8).unwrap_err().code,
                "input_lease_required"
            );
        }
        assert_eq!(process.reads.load(Ordering::SeqCst), 0);

        let mut foreign = Probe::new(&epoch);
        foreign.id.start_ticks = "2.000001".into();
        let fence = Fence {
            process: &foreign,
            epoch: &epoch,
            expected: &expected,
            authority: &authority,
            token: &token,
            until: Instant::now() + Duration::from_secs(5),
            stop: None,
        };
        assert_eq!(
            fence.read(0x10000, 8).unwrap_err().code,
            "ownership_changed"
        );
        assert_eq!(foreign.reads.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn batch_validates_every_request_before_reads_and_stops_between_payloads() {
        let epoch = epoch_fixture();
        let expected = context();
        let (authority, token) = leased_authority(&epoch);
        let process = Fake {
            id: epoch.game.clone(),
            reads: AtomicUsize::new(0),
            cancel: AtomicBool::new(false),
        };
        let fence = Fence {
            process: &process,
            epoch: &epoch,
            expected: &expected,
            authority: &authority,
            token: &token,
            until: Instant::now() + Duration::from_secs(5),
            stop: Some(&process.cancel),
        };
        for batch in [
            [(0x10000, 8), (0, 8)],
            [(0x10000, 8), (0x20000, (1 << 20) + 1)],
        ] {
            assert_eq!(fence.read_many(&batch).unwrap_err().code, "read_bound");
            assert_eq!(process.reads.load(Ordering::SeqCst), 0);
        }
        assert_eq!(
            fence
                .read_many(&[(0x10000, 8), (0x20000, 8)])
                .unwrap_err()
                .code,
            "session_closed"
        );
        assert_eq!(process.reads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn short_batch_payload_is_discarded_before_another_backend_read() {
        let epoch = epoch_fixture();
        let expected = context();
        let (authority, token) = leased_authority(&epoch);
        let mut process = Probe::new(&epoch);
        process.effect = ReadEffect::Short;
        let fence = Fence {
            process: &process,
            epoch: &epoch,
            expected: &expected,
            authority: &authority,
            token: &token,
            until: Instant::now() + Duration::from_secs(5),
            stop: None,
        };
        assert_eq!(
            fence
                .read_many(&[(0x10000, 8), (0x20000, 8)])
                .unwrap_err()
                .code,
            "short_read"
        );
        assert_eq!(process.reads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn native_read_finishing_after_original_deadline_cannot_return_payload() {
        let epoch = epoch_fixture();
        let expected = context();
        let (authority, token) = leased_authority(&epoch);
        let mut process = Probe::new(&epoch);
        let until = Instant::now() + Duration::from_secs(1);
        process.effect = ReadEffect::Until(until + Duration::from_millis(5));
        let fence = Fence {
            process: &process,
            epoch: &epoch,
            expected: &expected,
            authority: &authority,
            token: &token,
            until,
            stop: None,
        };
        assert_eq!(fence.read(0x10000, 8).unwrap_err().code, "action_deadline");
        assert_eq!(process.reads.load(Ordering::SeqCst), 1);
        assert_eq!(fence.read(0x20000, 8).unwrap_err().code, "action_deadline");
        assert_eq!(process.reads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn process_and_snapshot_boundaries_discard_success_after_stop() {
        let epoch = epoch_fixture();
        let expected = context();
        let (authority, token) = leased_authority(&epoch);
        for snapshot in [false, true] {
            let mut process = Probe::new(&epoch);
            process.stop_after_check = !snapshot;
            process.stop_after_snapshot = snapshot;
            let fence = Fence {
                process: &process,
                epoch: &epoch,
                expected: &expected,
                authority: &authority,
                token: &token,
                until: Instant::now() + Duration::from_secs(5),
                stop: Some(&process.stop),
            };
            let result = if snapshot {
                fence.begin_snapshot()
            } else {
                fence.check()
            };
            assert_eq!(result.unwrap_err().code, "session_closed");
            assert_eq!(
                process.checks.load(Ordering::SeqCst),
                usize::from(!snapshot)
            );
            assert_eq!(
                process.snapshots.load(Ordering::SeqCst),
                usize::from(snapshot)
            );
            assert_eq!(fence.read(0x10000, 8).unwrap_err().code, "session_closed");
            assert_eq!(process.reads.load(Ordering::SeqCst), 0);
        }
    }

    #[test]
    fn input_context_uses_fenced_snapshot_reads_without_recursive_input_checks() {
        let epoch = epoch_fixture();
        let expected = context();
        let (authority, token) = leased_authority(&epoch);
        let process = Probe::new(&epoch);
        let fence = Fence {
            process: &process,
            epoch: &epoch,
            expected: &expected,
            authority: &authority,
            token: &token,
            until: Instant::now() + Duration::from_secs(5),
            stop: Some(&process.stop),
        };
        assert_eq!(fence.input_check().unwrap_err().code, "invalid_pointer");
        assert_eq!(process.snapshots.load(Ordering::SeqCst), 1);
        assert_eq!(process.reads.load(Ordering::SeqCst), 1);
        assert_eq!(process.checks.load(Ordering::SeqCst), 0);
    }
}
