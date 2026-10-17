use super::*;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};

struct AssignmentFixture {
    fixture: Mutex<Fixture>,
    snapshots: Arc<AtomicUsize>,
    changes: Vec<(u64, Vec<u8>)>,
}
impl ReadMemory for AssignmentFixture {
    fn read(&self, a: u64, n: usize) -> Result<Vec<u8>> {
        self.fixture.lock().unwrap().read(a, n)
    }
}
impl Process for AssignmentFixture {
    fn image_base(&self) -> u64 {
        0x10000
    }
    fn check(&self) -> Result<()> {
        self.fixture.lock().unwrap().check()
    }
    fn begin_snapshot(&self) -> Result<()> {
        let mut f = self.fixture.lock().unwrap();
        if self.snapshots.fetch_add(1, Ordering::SeqCst) == 1 {
            for (address, bytes) in &self.changes {
                f.put(*address, bytes);
            }
        }
        f.check()
    }
}
fn fixture(assigned: bool) -> Fixture {
    let mut f = Fixture::new();
    f.region(0x80000, 0x200);
    f.object(0x80000, "CTechnology", 0x50400);
    f.word(0x80158, 0x28000);
    f.word(0x80160, 0x81000);
    f.region(0x81000, 0x100);
    f.object(0x81000, "CTechnologyTemplate", 0x50500);
    f.string(0x81008, "tools1");
    f.word(0x2c018, if assigned { 0x80000 } else { 0 });
    f.region(0x82000, 0x100);
    f.object(0x82000, "CNationalFocus", 0x50600);
    f.string(0x82018, "GER_rhineland");
    f.string(0x82060, "Rhineland");
    f.word(0x2a010, 0x82000);
    f
}
fn run(f: Fixture, changes: Vec<(u64, Vec<u8>)>, target: Value) -> (Value, usize) {
    let snapshots = Arc::new(AtomicUsize::new(0));
    let server = hoi4_live_native::protocol::Server {
        process: Some(Box::new(AssignmentFixture {
            fixture: Mutex::new(f),
            snapshots: snapshots.clone(),
            changes,
        })),
        unavailable: None,
    };
    let reply=server.handle(json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"hoi4_wait_for","arguments":{"timeout_seconds":1,"until_assignments":target}}})).unwrap();
    (
        serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap(),
        snapshots.load(Ordering::SeqCst),
    )
}
fn target() -> Value {
    json!({"focus_id":"GER_rhineland","research":[{"slot":0,"technology_id":"tools1"}]})
}
#[test]
fn assignment_wait_already_reached_returns_the_first_owned_sample() {
    let (v, n) = run(fixture(true), vec![], target());
    assert_eq!(v["reasons"], json!(["assignments_reached"]), "{v}");
    assert_eq!(
        n, 1,
        "Already present assignments must not poll for another change"
    );
    assert_eq!(v["state"]["research"][0]["researching"], "tools1");
    assert_eq!(v["state"]["focus"]["key"], "GER_rhineland");
}
#[test]
fn assignment_wait_waits_for_the_missing_half_of_the_conjunction() {
    let (v, n) = run(
        fixture(false),
        vec![(0x2c018, 0x80000u64.to_le_bytes().to_vec())],
        target(),
    );
    assert_eq!(v["reasons"], json!(["assignments_reached"]), "{v}");
    assert_eq!(n, 2);
}
#[test]
fn assignment_wait_unrelated_changes_cannot_finish_an_unmet_target() {
    let (v, n) = run(
        fixture(false),
        vec![(0x2a010, 0u64.to_le_bytes().to_vec())],
        target(),
    );
    assert_eq!(v["reasons"], json!(["wall_timeout"]), "{v}");
    assert!(n >= 2);
    assert_eq!(v["state_historical"], true);
    assert_eq!(v["current_state_verified"], false);
    assert_eq!(v["input_authorized"], false);
}
#[test]
fn assignment_wait_matching_state_from_a_new_campaign_is_rejected() {
    let (v, _) = run(
        fixture(false),
        vec![
            (0x60000, vec![b'b']),
            (0x2c018, 0x80000u64.to_le_bytes().to_vec()),
        ],
        target(),
    );
    assert_eq!(v["error"]["code"], "wait_context_changed", "{v}");
    assert!(v.get("state").is_none());
}
