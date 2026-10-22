//! Synthetic endpoint/reader only: no game, display, input or external child.
use super::*;
use crate::protocol::{
    worker_events::{Disposition, Events, Terminal},
    Commands,
};

struct NoGame {
    entered: SyncSender<()>,
    release: Mutex<Receiver<()>>,
}
impl platform::Process for NoGame {
    fn check(&self) -> Result<()> {
        self.entered.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(3))
            .unwrap();
        Err(Error::new(
            "torn_read",
            "Synthetic in-flight snapshot rejected",
        ))
    }
    fn image_base(&self) -> u64 {
        panic!("fixture never accesses memory")
    }
}
impl crate::memory::ReadMemory for NoGame {
    fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
        panic!("fixture never accesses memory")
    }
}
struct Adapter(Arc<Events>);
impl Commands for Adapter {
    fn tools(&self) -> Vec<Value> {
        vec![]
    }
    fn supports(&self, _: &str) -> bool {
        false
    }
    fn capabilities(&self) -> Value {
        json!({})
    }
    fn call(&self, _: &dyn platform::Process, _: &str, _: &Value) -> Result<Value> {
        panic!("wait has no input")
    }
    fn wait_events(&self) -> Option<Arc<Events>> {
        Some(self.0.clone())
    }
}
fn synthetic_owner(events: Arc<Events>) -> (Arc<Shared>, Receiver<()>, SyncSender<()>) {
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let identity = capture_identity(std::process::id() as i32).unwrap();
    let (closing, _receiver) = mpsc::sync_channel(1);
    let shared = Arc::new(Shared {
        server: Server {
            process: Some(Box::new(NoGame {
                entered: entered_tx,
                release: Mutex::new(release_rx),
            })),
            unavailable: None,
        },
        endpoint: Endpoint {
            version: 1,
            nonce: "synthetic-wait".into(),
            server: identity.clone(),
            session: Session {
                game: identity.clone(),
                supervisor: identity,
                build_id: "fixture".into(),
                sha256: "fixture".into(),
                translated: None,
            },
        },
        reads: RwLock::new(()),
        accepting_reads: AtomicBool::new(true),
        input_gate: Arc::new(RwLock::new(true)),
        alive: AtomicBool::new(true),
        closing,
        commands: RwLock::new(Some(Arc::new(Adapter(events)))),
        display: None,
        directory: PathBuf::new(),
    });
    (shared, entered_rx, release_tx)
}
fn client(shared: Arc<Shared>) -> (UnixStream, thread::JoinHandle<()>) {
    let (stream, server) = UnixStream::pair().unwrap();
    let nonce = shared.endpoint.nonce.clone();
    let h = thread::spawn(move || service_client(server, shared));
    send_value(&stream,&json!({"version":1,"nonce":nonce,"request":{"op":"mcp","message":{"jsonrpc":"2.0","id":42,"method":"tools/call","params":{"name":"hoi4_wait_for","arguments":{"until_assignments":{"focus_id":"wanted"},"timeout_seconds":50}}}}})).unwrap();
    (stream, h)
}
#[test]
fn worker_wait_endpoint_terminal_before_first_read_has_only_historical_diagnostics() {
    if !ordinary() {
        return;
    }
    let events = Arc::new(Events::default());
    let instance = events.start(7).unwrap();
    events
        .finish(
            instance,
            7,
            Terminal::new(
                Disposition::Quarantined,
                "action_uncertain",
                "Original unresolved intent",
            ),
        )
        .unwrap();
    let (shared, entered, _release) = synthetic_owner(events);
    let (stream, h) = client(shared.clone());
    let response = read_value(&stream, Duration::from_secs(2)).unwrap();
    h.join().unwrap();
    let out: Value = serde_json::from_str(
        response["reply"]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(out["reasons"], json!(["daily_worker_quarantined"]));
    assert_eq!(out["state"], Value::Null);
    assert_eq!(out["current_state_verified"], false);
    assert_eq!(
        out["worker_event"]["original_error"]["code"],
        "action_uncertain"
    );
    assert!(entered.try_recv().is_err());
    assert!(shared.reads.try_write().is_ok());
}
#[test]
fn worker_wait_owner_retirement_notifies_before_read_gate_drain_and_prevents_next_read() {
    if !ordinary() {
        return;
    }
    let events = Arc::new(Events::default());
    events.start(1).unwrap();
    let (shared, entered, release) = synthetic_owner(events.clone());
    let (stream, h) = client(shared.clone());
    entered.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(shared.reads.try_write().is_err());
    let start = Instant::now();
    shared.retire_reads();
    assert!(events.snapshot().unwrap().retired);
    assert!(!shared.accepting_reads.load(Ordering::Acquire));
    // An already in-flight read cannot be preempted; it must not be followed by another.
    release.send(()).unwrap();
    let response = read_value(&stream, Duration::from_secs(2)).unwrap();
    h.join().unwrap();
    assert!(start.elapsed() < Duration::from_secs(1));
    let out: Value = serde_json::from_str(
        response["reply"]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(out["error"]["code"], "session_closed");
    assert!(entered.try_recv().is_err());
    assert!(shared.reads.try_write().is_ok());
}
