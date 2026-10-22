use super::*;
use crate::memory::StringAbi;
use crate::own_focus::assignment_fixture::{Fixture, DATE, FIRST, SECOND};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

static NEXT: AtomicUsize = AtomicUsize::new(0);
fn setup(f: &Fixture) -> (Producer, std::path::PathBuf) {
    let directory = std::env::temp_dir().join(format!(
        "hoi4-focus-probe-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir(&directory).unwrap();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
    let path = directory.join("sequence.json");
    let context = f.context();
    let record = Record {
        version: 1,
        scope: Scope {
            epoch: Epoch {
                game: f.game(),
                campaign: format!("nonce:{}", context.playthrough_id),
                player: context.player.clone(),
            },
            anchors: context.anchors,
            own_country: f.own()[0],
            own_status: f.own()[1],
        },
        sequence: 42,
        last_date: context.date_raw,
    };
    let (mut store, old) = crate::daily_governor::store::Store::open(&path).unwrap();
    assert!(old.is_none());
    store
        .replace(&serde_json::to_vec(&record).unwrap())
        .unwrap();
    (
        Producer {
            expected: context,
            counter: Mutex::new(Counter {
                store,
                probe_latest_date: record.last_date,
                record,
                poisoned: false,
            }),
        },
        directory,
    )
}
#[test]
fn focus_busy_probes_do_not_allocate_sequences_or_rewrite_storage() {
    for abi in [StringAbi::LibCxx, StringAbi::LibStdCxx] {
        let mut f = Fixture::new(abi);
        f.set_active(Some(FIRST));
        let (producer, directory) = setup(&f);
        let path = directory.join("sequence.json");
        let bytes = fs::read(&path).unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        for _ in 0..12 {
            let probe = producer
                .probe_focus(&f, SECOND, Instant::now() + Duration::from_secs(3))
                .unwrap();
            assert_eq!(probe.active_key(), Some(FIRST));
        }
        assert_eq!(producer.counter.lock().unwrap().record.sequence, 42);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
        drop(producer);
        fs::remove_dir_all(directory).unwrap();
    }
}
#[test]
fn focus_probe_high_water_and_original_deadline_refuse_without_writes() {
    let mut f = Fixture::new(StringAbi::LibCxx);
    let (producer, directory) = setup(&f);
    let path = directory.join("sequence.json");
    let bytes = fs::read(&path).unwrap();
    f.set_date(DATE + 1);
    producer
        .probe_focus(&f, FIRST, Instant::now() + Duration::from_secs(3))
        .unwrap();
    f.set_date(DATE);
    assert_eq!(
        producer
            .probe_focus(&f, FIRST, Instant::now() + Duration::from_secs(3))
            .unwrap_err()
            .code,
        "ownership_changed"
    );
    let reads = f.read_counts();
    assert_eq!(
        producer
            .probe_focus(&f, FIRST, Instant::now())
            .unwrap_err()
            .code,
        "daily_deadline"
    );
    assert_eq!(f.read_counts(), reads);
    assert_eq!(producer.counter.lock().unwrap().record.sequence, 42);
    assert_eq!(fs::read(&path).unwrap(), bytes);
    drop(producer);
    fs::remove_dir_all(directory).unwrap();
}
