//! Persistent identity for one owned Mac controller. No process reads, input,
//! lease creation or migration of an older controller's journals lives here.
use crate::{
    actions::Epoch,
    calendar,
    daily_governor::store::Store,
    error::{Error, Result},
    game::ClockContext,
    platform::Identity,
};
use serde::{Deserialize, Deserializer, Serialize};
use std::{
    fs::{File, OpenOptions},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

const HISTORY: &[&str] = &[
    "clock-actions.jsonl",
    "game-actions.jsonl",
    "research-actions.jsonl",
    "focus-actions.jsonl",
    "construction-actions.jsonl",
    "production-actions.jsonl",
    "daily-sequence.json",
    "daily-plan.json",
    "daily-navigation.json",
    "daily-focus-navigation.json",
    "daily-production.json",
    "daily-panel-navigation.json",
];

fn invalid(message: &str) -> Error {
    Error::new("invalid_controller_epoch", message)
}
fn changed(message: &str) -> Error {
    Error::new("ownership_changed", message)
}

// Identity's general platform decoder accepts unknown fields. A persisted
// controller record instead has a closed, versioned shape at every level.
fn strict_epoch<'de, D: Deserializer<'de>>(decoder: D) -> std::result::Result<Epoch, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Game {
        pid: i32,
        start_ticks: String,
        exe: String,
        uid: u32,
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Saved {
        game: Game,
        campaign: String,
        player: String,
    }
    let saved = Saved::deserialize(decoder)?;
    Ok(Epoch {
        game: Identity {
            pid: saved.game.pid,
            start_ticks: saved.game.start_ticks,
            exe: saved.game.exe,
            uid: saved.game.uid,
        },
        campaign: saved.campaign,
        player: saved.player,
    })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    #[serde(deserialize_with = "strict_epoch")]
    epoch: Epoch,
    playthrough: String,
    anchors: [u64; 3],
    difficulty: i32,
    historical_raw: i32,
    high_water_date: i32,
}
fn valid_playthrough(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(i, byte)| {
            if [8, 13, 18, 23].contains(&i) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}
fn validate(epoch: &Epoch, context: &ClockContext) -> Result<()> {
    let game = &epoch.game;
    let timestamp = game.start_ticks.split_once('.');
    let canonical_timestamp = timestamp.is_some_and(|(seconds, micros)| {
        !seconds.is_empty()
            && seconds.bytes().all(|b| b.is_ascii_digit())
            && (seconds.len() == 1 || !seconds.starts_with('0'))
            && seconds.parse::<u64>().is_ok()
            && micros.len() == 6
            && micros.bytes().all(|b| b.is_ascii_digit())
    });
    let campaign = epoch.campaign.split_once(':');
    if game.pid <= 0
        || game.start_ticks.len() > 32
        || !canonical_timestamp
        || game.exe.len() > 4096
        || !Path::new(&game.exe).is_absolute()
        || game.exe.chars().any(char::is_control)
        || epoch.player.len() != 3
        || !epoch.player.bytes().all(|b| b.is_ascii_uppercase())
        || epoch.player != context.player
        || !valid_playthrough(&context.playthrough_id)
        || !campaign.is_some_and(|(nonce, playthrough)| {
            nonce.len() == 32
                && nonce
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                && playthrough == context.playthrough_id
        })
        || context
            .anchors
            .iter()
            .any(|p| !(0x10000..=0x7fff_ffff_0000).contains(p) || p % 8 != 0)
        || context.anchors[0] == context.anchors[1]
        || context.anchors[0] == context.anchors[2]
        || context.anchors[1] == context.anchors[2]
        || !(0..=4).contains(&context.difficulty)
        || !(0..=1).contains(&context.historical_raw)
        || context.speed > 4
    {
        return Err(invalid("Bounded canonical Mac controller context required"));
    }
    calendar::decode(context.date_raw)?;
    Ok(())
}
impl Record {
    fn context(&self) -> ClockContext {
        ClockContext {
            playthrough_id: self.playthrough.clone(),
            player: self.epoch.player.clone(),
            anchors: self.anchors,
            date_raw: self.high_water_date,
            difficulty: self.difficulty,
            historical_raw: self.historical_raw,
            paused: false,
            speed: 0,
        }
    }
    fn validate(&self) -> Result<()> {
        if self.version != 1 {
            return Err(invalid("Unsupported controller epoch version"));
        }
        validate(&self.epoch, &self.context())
    }
    fn accepts(&self, proposed: &Epoch, context: &ClockContext) -> Result<()> {
        // Only a newly proposed nonce may differ on reopen. This does not
        // normalize identities or accept a new process/player/campaign scope.
        if proposed.game != self.epoch.game {
            return Err(changed("Controller epoch game identity differs"));
        }
        if proposed.player != self.epoch.player || context.player != self.epoch.player {
            return Err(changed("Controller epoch player differs"));
        }
        if context.playthrough_id != self.playthrough || context.anchors != self.anchors {
            return Err(changed("Controller epoch playthrough or roots differ"));
        }
        if context.difficulty != self.difficulty || context.historical_raw != self.historical_raw {
            return Err(changed("Controller epoch campaign setup differs"));
        }
        if context.date_raw < self.high_water_date {
            return Err(changed(
                "Controller epoch timeline regressed below its persisted high water",
            ));
        }
        Ok(())
    }
}

fn private_file(metadata: &std::fs::Metadata) -> Result<()> {
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o777 != 0o600
        || metadata.nlink() != 1
    {
        return Err(Error::new(
            "unsafe_plan_path",
            "Private owned regular controller state required",
        ));
    }
    Ok(())
}
fn check_bootstrap(path: &Path) -> Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| invalid("Private controller directory required"))?;
    for name in HISTORY {
        let metadata = match directory.join(name).symlink_metadata() {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        private_file(&metadata)?;
        if metadata.len() != 0 {
            return Err(Error::new(
                "controller_epoch_migration_required",
                format!("Controller epoch is missing but {name} contains history; automatic migration or uncertainty reset is forbidden"),
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
impl Stamp {
    fn read(metadata: &std::fs::Metadata) -> Result<Self> {
        private_file(metadata)?;
        Ok(Self {
            device: metadata.dev(),
            inode: metadata.ino(),
            size: metadata.len(),
            modified: (metadata.mtime(), metadata.mtime_nsec()),
            changed: (metadata.ctime(), metadata.ctime_nsec()),
        })
    }
}
struct PinnedFile {
    file: File,
    stamp: Stamp,
}
impl PinnedFile {
    fn open(path: &Path) -> Result<Self> {
        let before = Stamp::read(&path.symlink_metadata()?)?;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        let pinned = Self {
            stamp: Stamp::read(&file.metadata()?)?,
            file,
        };
        if before != pinned.stamp {
            return Err(Error::new(
                "plan_changed",
                "Controller state changed while opening",
            ));
        }
        pinned.check(path)?;
        Ok(pinned)
    }
    fn check(&self, path: &Path) -> Result<()> {
        if Stamp::read(&path.symlink_metadata()?)? != self.stamp
            || Stamp::read(&self.file.metadata()?)? != self.stamp
        {
            return Err(Error::new(
                "plan_changed",
                "Controller epoch file replaced or modified",
            ));
        }
        Ok(())
    }
}

pub(super) struct ControllerEpoch {
    store: Store,
    path: PathBuf,
    pinned: PinnedFile,
    record: Record,
    poisoned: bool,
}
impl ControllerEpoch {
    /// The caller proves session ownership and supplies a random proposed epoch.
    /// Missing state is permitted only for an explicitly new empty controller;
    /// an existing record is never reset, and its original nonce is retained.
    pub(super) fn open(
        path: &Path,
        proposed: Epoch,
        context: &ClockContext,
        bootstrap_empty: bool,
    ) -> Result<Self> {
        validate(&proposed, context)?;
        // Pin an existing inode before Store reads it, so a same-byte rename
        // during open cannot silently adopt a different state file either.
        let initial = match path.symlink_metadata() {
            Ok(_) => Some(PinnedFile::open(path)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        let (mut store, bytes) = Store::open(path)?;
        if let Some(pinned) = initial.as_ref() {
            pinned.check(path)?;
        } else if bytes.is_some() {
            return Err(Error::new(
                "plan_changed",
                "Controller state appeared while opening",
            ));
        }
        let record = if let Some(bytes) = bytes {
            let record: Record = serde_json::from_slice(&bytes)
                .map_err(|error| Error::new("invalid_controller_epoch", error.to_string()))?;
            record.validate()?;
            record.accepts(&proposed, context)?;
            record
        } else {
            if !bootstrap_empty {
                return Err(Error::new(
                    "controller_epoch_migration_required",
                    "Controller epoch is missing; only a new empty controller may bootstrap",
                ));
            }
            check_bootstrap(path)?;
            let record = Record {
                version: 1,
                epoch: proposed,
                playthrough: context.playthrough_id.clone(),
                anchors: context.anchors,
                difficulty: context.difficulty,
                historical_raw: context.historical_raw,
                high_water_date: context.date_raw,
            };
            store.replace(&serde_json::to_vec(&record).map_err(|e| invalid(&e.to_string()))?)?;
            record
        };
        let mut guard = Self {
            pinned: match initial {
                Some(pinned) => pinned,
                None => PinnedFile::open(path)?,
            },
            store,
            path: path.to_owned(),
            record,
            poisoned: false,
        };
        let epoch = guard.record.epoch.clone();
        guard.observe(&epoch, context)?;
        Ok(guard)
    }
    fn check(&mut self) -> Result<()> {
        if self.poisoned {
            return Err(Error::new(
                "action_uncertain",
                "Controller epoch uncertainty is retained; input remains blocked",
            ));
        }
        let result = (|| {
            self.pinned.check(&self.path)?;
            self.store.check()?;
            self.pinned.check(&self.path)
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    /// A fresh storage check precedes returning the retained epoch. The caller
    /// still needs a current owned context and independently authorized lease.
    pub(super) fn epoch(&mut self) -> Result<Epoch> {
        self.check()?;
        Ok(self.record.epoch.clone())
    }
    /// Exact context checks precede input. Only advancing dates replace/fsync
    /// state; same-date observations and epoch queries perform checked reads.
    pub(super) fn observe(&mut self, epoch: &Epoch, context: &ClockContext) -> Result<()> {
        self.check()?;
        let result = (|| {
            validate(epoch, context)?;
            if epoch != &self.record.epoch {
                return Err(changed(
                    "Observation does not carry the retained controller epoch",
                ));
            }
            self.record.accepts(epoch, context)?;
            if context.date_raw == self.record.high_water_date {
                return Ok(());
            }
            let mut next = self.record.clone();
            next.high_water_date = context.date_raw;
            self.store
                .replace(&serde_json::to_vec(&next).map_err(|e| invalid(&e.to_string()))?)?;
            let pinned = PinnedFile::open(&self.path)?;
            self.store.check()?;
            pinned.check(&self.path)?;
            self.pinned = pinned;
            self.record = next;
            Ok(())
        })();
        if result.is_err() {
            self.poisoned = true;
        }
        result
    }
    pub(super) fn blocked(&self) -> bool {
        self.poisoned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::{
        fs::{self, DirBuilder},
        io::Write,
        os::unix::fs::{symlink, DirBuilderExt, PermissionsExt},
        sync::atomic::{AtomicU64, Ordering},
    };

    const DATE: i32 = 60_759_371;
    const PLAYTHROUGH: &str = "aabbccdd-1122-4344-8566-778899aabbcc";
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "mac-controller-epoch-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            DirBuilder::new().mode(0o700).create(&path).unwrap();
            Self(path)
        }
        fn path(&self) -> PathBuf {
            self.0.join("controller-epoch.json")
        }
        fn open(&self) -> ControllerEpoch {
            ControllerEpoch::open(&self.path(), epoch('a'), &context(), true).unwrap()
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700));
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn epoch(nonce: char) -> Epoch {
        Epoch {
            game: Identity {
                pid: 120,
                start_ticks: "1792677600.000123".into(),
                exe: "/generated/hoi4.app/Contents/MacOS/hoi4".into(),
                uid: 501,
            },
            campaign: format!("{}:{PLAYTHROUGH}", nonce.to_string().repeat(32)),
            player: "GER".into(),
        }
    }
    fn context() -> ClockContext {
        ClockContext {
            playthrough_id: PLAYTHROUGH.into(),
            player: "GER".into(),
            anchors: [0x100000, 0x110000, 0x120000],
            date_raw: DATE,
            difficulty: 2,
            historical_raw: 1,
            paused: true,
            speed: 0,
        }
    }
    fn write_private(path: &Path, bytes: &[u8]) {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
    }
    fn read(path: &Path) -> Value {
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    }
    fn error(result: Result<ControllerEpoch>) -> Error {
        match result {
            Ok(_) => panic!("Unexpected successful controller binding"),
            Err(error) => error,
        }
    }

    #[test]
    fn reopen_keeps_original_random_epoch_and_exclusive_lock_for_its_lifetime() {
        let t = Temp::new();
        let mut guard = t.open();
        assert_eq!(guard.epoch().unwrap(), epoch('a'));
        assert!(!guard.blocked());
        let before = fs::read(t.path()).unwrap();
        let stamp = guard.pinned.stamp;
        assert_eq!(
            error(ControllerEpoch::open(
                &t.path(),
                epoch('b'),
                &context(),
                false
            ))
            .code,
            "plan_busy"
        );
        drop(guard);
        let mut guard = ControllerEpoch::open(&t.path(), epoch('b'), &context(), false).unwrap();
        assert_eq!(guard.epoch().unwrap(), epoch('a'));
        assert_eq!(guard.pinned.stamp, stamp);
        assert_eq!(fs::read(t.path()).unwrap(), before);
        assert_eq!(read(&t.path())["version"], 1);
    }

    #[test]
    fn only_advancing_dates_replace_state_and_high_water_survives_reopen() {
        let t = Temp::new();
        let mut guard = t.open();
        let initial = guard.pinned.stamp;
        let mut observed = context();
        for _ in 0..4 {
            observed.paused = !observed.paused;
            observed.speed = 4 - observed.speed;
            guard.observe(&epoch('a'), &observed).unwrap();
            assert_eq!(guard.epoch().unwrap(), epoch('a'));
            assert_eq!(guard.pinned.stamp, initial);
            assert_eq!(Stamp::read(&t.path().metadata().unwrap()).unwrap(), initial);
        }
        observed.date_raw += 1;
        guard.observe(&epoch('a'), &observed).unwrap();
        assert_ne!(guard.pinned.stamp, initial);
        assert_eq!(read(&t.path())["high_water_date"], DATE + 1);
        let advanced = guard.pinned.stamp;
        guard.observe(&epoch('a'), &observed).unwrap();
        assert_eq!(guard.pinned.stamp, advanced);
        drop(guard);
        assert_eq!(
            error(ControllerEpoch::open(
                &t.path(),
                epoch('b'),
                &context(),
                false
            ))
            .code,
            "ownership_changed"
        );
        let before = fs::read(t.path()).unwrap();
        observed.date_raw += 1;
        let mut guard = ControllerEpoch::open(&t.path(), epoch('b'), &observed, false).unwrap();
        assert_eq!(guard.epoch().unwrap(), epoch('a'));
        assert_ne!(fs::read(t.path()).unwrap(), before);
        assert_eq!(read(&t.path())["high_water_date"], DATE + 2);
    }

    #[test]
    fn missing_epoch_never_migrates_nonempty_action_or_daily_history() {
        let t = Temp::new();
        assert_eq!(
            error(ControllerEpoch::open(
                &t.path(),
                epoch('a'),
                &context(),
                false
            ))
            .code,
            "controller_epoch_migration_required"
        );
        assert!(!t.path().exists());
        for name in HISTORY {
            let t = Temp::new();
            let history = t.0.join(name);
            write_private(&history, b"preserved controller history");
            let failure = error(ControllerEpoch::open(
                &t.path(),
                epoch('a'),
                &context(),
                true,
            ));
            assert_eq!(
                failure.code, "controller_epoch_migration_required",
                "{name}"
            );
            assert!(failure.message.contains(name));
            assert!(!t.path().exists());
            assert_eq!(fs::read(history).unwrap(), b"preserved controller history");
        }
    }

    #[test]
    fn new_empty_controller_may_keep_declarative_configuration_and_empty_journals() {
        let t = Temp::new();
        write_private(
            &t.0.join("input.json"),
            br#"{"controller":"fixture","player":"GER"}"#,
        );
        write_private(&t.0.join("plan.json"), br#"{"active":false}"#);
        for name in HISTORY {
            write_private(&t.0.join(name), b"");
        }
        assert_eq!(t.open().epoch().unwrap(), epoch('a'));
    }

    #[test]
    fn different_owned_context_never_rewrites_an_existing_scope() {
        let t = Temp::new();
        drop(t.open());
        let original = fs::read(t.path()).unwrap();
        for change in [
            |e: &mut Epoch, _: &mut ClockContext| e.game.pid += 1,
            |e: &mut Epoch, _: &mut ClockContext| e.game.uid += 1,
            |e: &mut Epoch, _: &mut ClockContext| e.game.start_ticks = "1792677600.000124".into(),
            |e: &mut Epoch, _: &mut ClockContext| e.game.exe = "/generated/other-game".into(),
            |e: &mut Epoch, c: &mut ClockContext| {
                e.player = "FRA".into();
                c.player = "FRA".into();
            },
            |e: &mut Epoch, c: &mut ClockContext| {
                c.playthrough_id.replace_range(35..36, "d");
                e.campaign = format!("{}:{}", "b".repeat(32), c.playthrough_id);
            },
            |_: &mut Epoch, c: &mut ClockContext| c.anchors[0] += 8,
            |_: &mut Epoch, c: &mut ClockContext| c.difficulty = 3,
            |_: &mut Epoch, c: &mut ClockContext| c.historical_raw = 0,
            |_: &mut Epoch, c: &mut ClockContext| c.date_raw -= 1,
        ] {
            let mut proposed = epoch('b');
            let mut observed = context();
            change(&mut proposed, &mut observed);
            assert_eq!(
                error(ControllerEpoch::open(&t.path(), proposed, &observed, true)).code,
                "ownership_changed"
            );
            assert_eq!(fs::read(t.path()).unwrap(), original);
        }
    }

    #[test]
    fn malformed_versioned_records_and_nested_unknown_fields_never_bootstrap() {
        let t = Temp::new();
        drop(t.open());
        let valid = read(&t.path());
        let mut candidates = vec![Vec::new(), b"{".to_vec(), b"null".to_vec()];
        for change in [
            |v: &mut Value| v["version"] = json!(2),
            |v: &mut Value| v["unknown"] = json!(true),
            |v: &mut Value| v["epoch"]["unknown"] = json!(true),
            |v: &mut Value| v["epoch"]["game"]["unknown"] = json!(true),
            |v: &mut Value| v["epoch"]["game"]["pid"] = json!(0),
            |v: &mut Value| v["epoch"]["game"]["start_ticks"] = json!("01792677600.000123"),
            |v: &mut Value| v["epoch"]["game"]["exe"] = json!("relative"),
            |v: &mut Value| v["epoch"]["campaign"] = json!("not-random"),
            |v: &mut Value| v["playthrough"] = json!("not-a-uuid"),
            |v: &mut Value| v["anchors"] = json!([0, 0x110000, 0x120000]),
            |v: &mut Value| v["anchors"] = json!([0x100001, 0x110000, 0x120000]),
            |v: &mut Value| v["anchors"] = json!([0x100000, 0x100000, 0x120000]),
            |v: &mut Value| v["high_water_date"] = json!(0),
            |v: &mut Value| v["difficulty"] = json!(-1),
            |v: &mut Value| v["historical_raw"] = json!(2),
        ] {
            let mut broken = valid.clone();
            change(&mut broken);
            candidates.push(serde_json::to_vec(&broken).unwrap());
        }
        let text = serde_json::to_string(&valid).unwrap();
        candidates.push(
            text.replace("\"version\":1", "\"version\":1,\"version\":1")
                .into_bytes(),
        );
        candidates.push(
            text.replace("\"pid\":120", "\"pid\":120,\"pid\":120")
                .into_bytes(),
        );
        for bytes in candidates {
            fs::write(t.path(), &bytes).unwrap();
            assert!(ControllerEpoch::open(&t.path(), epoch('b'), &context(), true).is_err());
            assert_eq!(fs::read(t.path()).unwrap(), bytes);
        }
    }

    #[test]
    fn malformed_initial_context_cannot_create_controller_state() {
        for change in [
            |e: &mut Epoch, _: &mut ClockContext| e.game.start_ticks = "1792677600.123".into(),
            |e: &mut Epoch, _: &mut ClockContext| e.game.start_ticks = "1792677600.0001230".into(),
            |e: &mut Epoch, _: &mut ClockContext| {
                e.game.start_ticks = "18446744073709551616.000001".into()
            },
            |e: &mut Epoch, _: &mut ClockContext| e.player = "FRA".into(),
            |e: &mut Epoch, _: &mut ClockContext| {
                e.campaign = format!("{}:{PLAYTHROUGH}", "x".repeat(32))
            },
            |_: &mut Epoch, c: &mut ClockContext| c.anchors[0] = 0,
            |_: &mut Epoch, c: &mut ClockContext| c.date_raw = 0,
            |_: &mut Epoch, c: &mut ClockContext| c.speed = 5,
        ] {
            let t = Temp::new();
            let mut proposed = epoch('a');
            let mut observed = context();
            change(&mut proposed, &mut observed);
            assert!(ControllerEpoch::open(&t.path(), proposed, &observed, true).is_err());
            assert!(!t.path().exists());
        }
    }

    #[test]
    fn symlink_nonprivate_and_hardlinked_state_or_writer_guard_refuse() {
        for variant in 0..6 {
            let t = Temp::new();
            drop(t.open());
            let original = fs::read(t.path()).unwrap();
            let target = t.0.join("target.json");
            match variant {
                0 => {
                    fs::rename(t.path(), &target).unwrap();
                    symlink(&target, t.path()).unwrap();
                }
                1 => {
                    fs::set_permissions(t.path(), fs::Permissions::from_mode(0o640)).unwrap();
                }
                2 => fs::hard_link(t.path(), &target).unwrap(),
                3 => {
                    let lock = t.0.join("controller-epoch.json.lock");
                    fs::rename(&lock, &target).unwrap();
                    symlink(&target, lock).unwrap();
                }
                4 => {
                    fs::set_permissions(&t.0, fs::Permissions::from_mode(0o750)).unwrap();
                }
                _ => {
                    fs::hard_link(t.0.join("controller-epoch.json.lock"), &target).unwrap();
                }
            }
            assert!(ControllerEpoch::open(&t.path(), epoch('b'), &context(), true).is_err());
            assert_eq!(fs::read(t.path()).unwrap(), original);
        }
        let t = Temp::new();
        let real = t.0.join("real");
        DirBuilder::new().mode(0o700).create(&real).unwrap();
        symlink(&real, t.0.join("alias")).unwrap();
        assert!(
            ControllerEpoch::open(&t.0.join("alias/epoch.json"), epoch('a'), &context(), true)
                .is_err()
        );
        assert!(!real.join("epoch.json").exists());
    }

    #[test]
    fn untrusted_empty_history_file_is_not_safe_bootstrap_evidence() {
        for variant in 0..3 {
            let t = Temp::new();
            let file = t.0.join("game-actions.jsonl");
            let target = t.0.join("other");
            write_private(&target, b"");
            match variant {
                0 => symlink(&target, &file).unwrap(),
                1 => fs::hard_link(&target, &file).unwrap(),
                _ => {
                    write_private(&file, b"");
                    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
                }
            }
            assert_eq!(
                error(ControllerEpoch::open(
                    &t.path(),
                    epoch('a'),
                    &context(),
                    true
                ))
                .code,
                "unsafe_plan_path"
            );
            assert!(!t.path().exists());
        }
    }

    #[test]
    fn regression_or_wrong_epoch_poison_is_sticky_without_rewriting_history() {
        for variant in 0..2 {
            let t = Temp::new();
            let mut guard = t.open();
            let before = fs::read(t.path()).unwrap();
            let mut observed = context();
            let supplied = if variant == 0 {
                observed.date_raw -= 1;
                epoch('a')
            } else {
                epoch('b')
            };
            assert_eq!(
                guard.observe(&supplied, &observed).unwrap_err().code,
                "ownership_changed"
            );
            assert!(guard.blocked());
            assert_eq!(
                guard.observe(&epoch('a'), &context()).unwrap_err().code,
                "action_uncertain"
            );
            assert_eq!(guard.epoch().unwrap_err().code, "action_uncertain");
            assert_eq!(fs::read(t.path()).unwrap(), before);
        }
    }

    #[test]
    fn same_byte_file_replacement_and_restoration_cannot_clear_uncertainty() {
        let t = Temp::new();
        let mut guard = t.open();
        let original = fs::read(t.path()).unwrap();
        let replacement = t.0.join("replacement.json");
        write_private(&replacement, &original);
        fs::rename(replacement, t.path()).unwrap();
        assert_eq!(guard.epoch().unwrap_err().code, "plan_changed");
        assert!(guard.blocked());
        fs::write(t.path(), &original).unwrap();
        assert_eq!(
            guard.observe(&epoch('a'), &context()).unwrap_err().code,
            "action_uncertain"
        );
        assert_eq!(fs::read(t.path()).unwrap(), original);
    }

    #[test]
    fn writer_or_parent_replacement_and_permission_loss_are_sticky() {
        for variant in 0..3 {
            let t = Temp::new();
            let directory = t.0.join("controller");
            DirBuilder::new().mode(0o700).create(&directory).unwrap();
            let path = directory.join("epoch.json");
            let mut guard = ControllerEpoch::open(&path, epoch('a'), &context(), true).unwrap();
            match variant {
                0 => {
                    fs::remove_file(directory.join("epoch.json.lock")).unwrap();
                    write_private(&directory.join("epoch.json.lock"), b"");
                }
                1 => {
                    fs::rename(&directory, t.0.join("old-controller")).unwrap();
                    DirBuilder::new().mode(0o700).create(&directory).unwrap();
                }
                _ => {
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
                }
            }
            assert!(guard.epoch().is_err());
            assert!(guard.blocked());
            if variant == 2 {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
            }
            assert_eq!(guard.epoch().unwrap_err().code, "action_uncertain");
        }
    }
}
