use super::*;
use std::io::BufRead;
use std::sync::Mutex;

struct Fixture {
    directory: PathBuf,
    endpoint: Endpoint,
    shared: Arc<Shared>,
    owner: Option<thread::JoinHandle<Result<()>>>,
}
impl Fixture {
    fn new(child: OwnedChild) -> Self {
        Self::with_server(
            child,
            Server {
                process: None,
                unavailable: None,
            },
        )
    }
    fn with_server(mut child: OwnedChild, server: Server) -> Self {
        assert_ne!(uid(), 0, "Launcher fixtures require an ordinary user");
        child.identity = Some(capture_identity(child.child.id() as i32).unwrap());
        let directory =
            std::env::temp_dir().join(format!("hoi4-owner-fixture-{}", nonce().unwrap()));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let listener = UnixListener::bind(directory.join("control.sock")).unwrap();
        fs::set_permissions(
            directory.join("control.sock"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        listener.set_nonblocking(true).unwrap();
        let socket_meta = fs::symlink_metadata(directory.join("control.sock")).unwrap();
        let server_identity = capture_identity(std::process::id() as i32).unwrap();
        let endpoint = Endpoint {
            version: 1,
            nonce: nonce().unwrap(),
            server: server_identity.clone(),
            session: Session {
                game: child.identity.clone().unwrap(),
                supervisor: server_identity,
                build_id: "generated-fixture".into(),
                sha256: "generated-fixture".into(),
            },
        };
        json_new(&directory.join("endpoint.json"), &endpoint).unwrap();
        let (closing, receiver) = mpsc::sync_channel(8);
        let shared = Arc::new(Shared {
            server,
            endpoint: json_read(&directory.join("endpoint.json")).unwrap(),
            reads: RwLock::new(()),
            accepting_reads: AtomicBool::new(true),
            alive: AtomicBool::new(true),
            closing,
        });
        let path = directory.clone();
        let owner_shared = Arc::clone(&shared);
        let owner = thread::spawn(move || {
            run_owner(listener, socket_meta, &path, child, owner_shared, receiver)
        });
        Self {
            directory,
            endpoint,
            shared,
            owner: Some(owner),
        }
    }
    fn sleeper() -> Self {
        let mut command = Command::new("/bin/sleep");
        command
            .arg("120")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        Self::new(OwnedChild::spawn(&mut command).unwrap())
    }
    fn status(&self) -> Value {
        request(&self.directory, json!({"op":"status"})).unwrap()
    }
    fn ping(&self) -> Value {
        request(
            &self.directory,
            json!({"op":"mcp","message":{"jsonrpc":"2.0","id":8,"method":"ping"}}),
        )
        .unwrap()
    }
    fn stream(&self) -> UnixStream {
        UnixStream::connect(self.directory.join("control.sock")).unwrap()
    }
    fn close(&mut self) {
        if let Some(owner) = self.owner.take() {
            super::close(&self.directory).unwrap();
            owner.join().unwrap().unwrap();
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.close();
        let _ = fs::remove_dir_all(&self.directory);
    }
}
fn ordinary() -> bool {
    if uid() == 0 {
        eprintln!("SKIP: generated launcher fixtures require ordinary UID");
        false
    } else {
        true
    }
}

#[test]
fn reconnects_and_failed_clients_preserve_one_owned_child() {
    if !ordinary() {
        return;
    }
    let mut fixture = Fixture::sleeper();
    let before = fixture.status();
    assert_eq!(fixture.ping()["reply"]["id"], 8);
    drop(fixture.stream()); // EOF before a request.
    let mut malformed = fixture.stream();
    malformed.write_all(b"{bad\n").unwrap();
    assert!(read_value(&malformed, Duration::from_secs(3))
        .unwrap()
        .get("error")
        .is_some());
    let mut partial = fixture.stream();
    partial.write_all(b"{\"version\":").unwrap();
    let start = Instant::now();
    assert_eq!(fixture.status()["owned_live"], true); // Idle peer cannot block status.
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(read_value(&partial, Duration::from_secs(3))
        .unwrap()
        .get("error")
        .is_some());
    let mut oversized = fixture.stream();
    let _ = oversized.write_all(&vec![b'x'; framing::LIMIT + 1]);
    assert!(read_value(&oversized, Duration::from_secs(3))
        .unwrap()
        .get("error")
        .is_some());
    let disconnected = fixture.stream();
    send_value(&disconnected, &json!({"version":1,"nonce":fixture.endpoint.nonce,"request":{"op":"mcp","message":{"jsonrpc":"2.0","id":1,"method":"ping"}}})).unwrap();
    drop(disconnected); // An accepted request followed by a broken reply is isolated.
    assert_eq!(fixture.ping()["reply"]["id"], 8);
    assert_eq!(fixture.status()["game"], before["game"]);
    assert_eq!(
        capture_identity(fixture.endpoint.session.game.pid).unwrap(),
        fixture.endpoint.session.game
    );

    let binary = std::env::var_os("HOI4_NATIVE_FIXTURE_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("hoi4-live-native")
        });
    assert!(
        binary.is_file(),
        "Build the native CLI, or provide HOI4_NATIVE_FIXTURE_BINARY"
    );
    for _ in 0..2 {
        let mut proxy = Command::new(&binary)
            .arg("--connect")
            .arg(&fixture.directory)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        proxy
            .stdin
            .take()
            .unwrap()
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":17,\"method\":\"ping\"}\n")
            .unwrap();
        let output = proxy.wait_with_output().unwrap();
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let response: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(response["id"], 17);
        assert_eq!(fixture.status()["game"], before["game"]);
    }
    fixture.close();
    assert!(capture_identity(fixture.endpoint.session.game.pid).is_err());
    assert!(fixture.directory.join("exited.json").is_file());
}

#[test]
fn stale_identity_nonce_and_public_paths_are_refused() {
    if !ordinary() {
        return;
    }
    let fixture = Fixture::sleeper();
    let stream = fixture.stream();
    send_value(
        &stream,
        &json!({"version":1,"nonce":"wrong","request":{"op":"close"}}),
    )
    .unwrap();
    assert_eq!(
        read_value(&stream, Duration::from_secs(3)).unwrap()["error"]["code"],
        "invalid_endpoint"
    );
    assert_eq!(fixture.status()["owned_live"], true);
    let original = fs::read(fixture.directory.join("endpoint.json")).unwrap();
    let mut replacement: Value = serde_json::from_slice(&original).unwrap();
    replacement["server"]["start_ticks"] = json!("0");
    fs::write(
        fixture.directory.join("endpoint.json"),
        serde_json::to_vec(&replacement).unwrap(),
    )
    .unwrap();
    assert_eq!(
        request(&fixture.directory, json!({"op":"close"}))
            .unwrap_err()
            .code,
        "ownership_changed"
    );
    fs::write(fixture.directory.join("endpoint.json"), original).unwrap();
    fs::set_permissions(
        fixture.directory.join("control.sock"),
        fs::Permissions::from_mode(0o666),
    )
    .unwrap();
    assert_eq!(
        request(&fixture.directory, json!({"op":"close"}))
            .unwrap_err()
            .code,
        "invalid_endpoint"
    );
    fs::set_permissions(
        fixture.directory.join("control.sock"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();
    assert_eq!(fixture.status()["owned_live"], true);
}

#[test]
fn cleanup_never_unlinks_a_replacement_socket() {
    if !ordinary() {
        return;
    }
    let mut fixture = Fixture::sleeper();
    let stream = fixture.stream();
    fs::remove_file(fixture.directory.join("control.sock")).unwrap();
    let replacement = UnixListener::bind(fixture.directory.join("control.sock")).unwrap();
    let replacement_inode = fs::metadata(fixture.directory.join("control.sock"))
        .unwrap()
        .ino();
    send_value(
        &stream,
        &json!({"version":1,"nonce":fixture.endpoint.nonce,"request":{"op":"close"}}),
    )
    .unwrap();
    assert_eq!(
        read_value(&stream, Duration::from_secs(25)).unwrap()["closed"],
        true
    );
    fixture.owner.take().unwrap().join().unwrap().unwrap();
    assert_eq!(
        fs::metadata(fixture.directory.join("control.sock"))
            .unwrap()
            .ino(),
        replacement_inode
    );
    drop(replacement);
}

#[test]
fn unsupported_build_does_not_create_a_run_or_launch() {
    if !ordinary() {
        return;
    }
    let directory = std::env::temp_dir().join(format!("hoi4-rejected-launch-{}", nonce().unwrap()));
    fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let executable = fs::canonicalize("/bin/true").unwrap();
    let run = directory.join("run");
    let config = directory.join("launch.json");
    json_new(&config, json!({"executable":executable,"run_directory":run,"display":{"number":123,"server":capture_identity(std::process::id() as i32).unwrap(),"xauthority":directory.join("no-authority")}})).unwrap();
    assert_eq!(serve(&config).unwrap_err().code, "unsupported_build");
    assert!(!run.exists());
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn generated_hung_child() {
    if std::env::var_os("HOI4_GENERATED_HUNG_CHILD").is_none() {
        return;
    }
    // SAFETY: this exact reexec fixture child alone ignores SIGTERM; parent
    // handler state is untouched, and private pipe EOF releases it normally.
    unsafe {
        libc::signal(libc::SIGTERM, libc::SIG_IGN);
    }
    println!("child-ready");
    std::io::stdout().flush().unwrap();
    let mut bytes = Vec::new();
    std::io::stdin().read_to_end(&mut bytes).unwrap();
}

#[test]
fn hung_shutdown_preserves_owner_then_reaps_normal_exit() {
    if !ordinary() {
        return;
    }
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "launch::linux::tests::generated_hung_child",
            "--nocapture",
        ])
        .env("HOI4_GENERATED_HUNG_CHILD", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    let input = child.child.stdin.take().unwrap();
    let mut output = std::io::BufReader::new(child.child.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        assert_ne!(output.read_line(&mut line).unwrap(), 0);
        if line.contains("child-ready") {
            break;
        }
    }
    let mut fixture = Fixture::new(child);
    assert_eq!(
        super::close(&fixture.directory).unwrap_err().code,
        "shutdown_pending"
    );
    assert_eq!(fixture.status()["owned_live"], true);
    assert_eq!(
        capture_identity(fixture.endpoint.session.game.pid).unwrap(),
        fixture.endpoint.session.game
    );
    drop(input); // Normal child exit after diagnostic proof; never SIGKILL.
    fixture.close();
    assert!(fixture.directory.join("exited.json").is_file());
}

const LIMIT_OWNER_TEST: &str = "launch::linux::tests::generated_worker_limit_owner";
const MEMORY_CHILD_TEST: &str = "launch::linux::tests::generated_worker_memory_child";
const MEMORY_READY: &str = "worker-memory-ready:";

fn selected_helper(variable: &str, name: &str) -> Option<String> {
    let token = std::env::var(variable).ok()?;
    let arguments: Vec<_> = std::env::args().collect();
    if token.len() != 32
        || !token.bytes().all(|byte| byte.is_ascii_hexdigit())
        || !arguments
            .windows(2)
            .any(|pair| pair[0] == "--exact" && pair[1] == name)
    {
        return None;
    }
    Some(token)
}

#[test]
fn generated_worker_memory_child() {
    let Some(token) = selected_helper("HOI4_WORKER_MEMORY_CHILD", MEMORY_CHILD_TEST) else {
        return;
    };
    assert_ne!(uid(), 0);
    let bytes = token.into_bytes().into_boxed_slice();
    println!(
        "{MEMORY_READY}{}",
        json!({"pid":std::process::id(),"address":bytes.as_ptr() as usize,"length":bytes.len()})
    );
    std::io::stdout().flush().unwrap();
    // This generated child alone owns the nonce allocation. Its parent's
    // private input pipe releases it on EOF even if that parent is killed by
    // the outer fixture watchdog; no process discovery or fallback signal.
    let mut input = [0; 1];
    assert_eq!(std::io::stdin().read(&mut input).unwrap(), 0);
    assert_eq!(std::hint::black_box(bytes.as_ref()).len(), 32);
}

fn memory_child_ready(output: &mut std::process::ChildStdout) -> Value {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut line = Vec::new();
    let mut total = 0;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "Generated child readiness timed out");
        let mut poll = libc::pollfd {
            fd: output.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one live private pipe and initialized poll buffer. A single
        // reader consumes one byte only after readiness, with an absolute cap.
        let ready =
            unsafe { libc::poll(&mut poll, 1, remaining.as_millis().clamp(1, 5000) as i32) };
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            panic!("Generated child readiness poll: {error}");
        }
        if ready == 0 {
            continue;
        }
        let mut byte = [0; 1];
        match output.read(&mut byte) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => assert_eq!(
                result.unwrap(),
                1,
                "Generated child exited before readiness"
            ),
        }
        total += 1;
        assert!(total <= 8192, "Generated child readiness exceeded bound");
        if byte[0] == b'\n' {
            let text = std::str::from_utf8(&line).unwrap();
            if let Some((_, record)) = text.split_once(MEMORY_READY) {
                return serde_json::from_str(record).unwrap();
            }
            line.clear();
        } else {
            line.push(byte[0]);
        }
    }
}

struct InFlightProbe {
    identity: Identity,
    address: usize,
    expected: Vec<u8>,
    entered: SyncSender<()>,
    release: Mutex<Receiver<()>>,
    read_ok: Arc<AtomicBool>,
}
impl platform::Process for InFlightProbe {
    fn check(&self) -> Result<()> {
        verify_identity(&self.identity)?;
        let stat = fs::read_to_string(format!("/proc/{}/stat", self.identity.pid))?;
        if platform::parse_stat(&stat)?.1 != std::process::id() as i32 {
            return Err(Error::new(
                "ownership_changed",
                "Generated child parent changed",
            ));
        }
        Ok(())
    }
    fn image_base(&self) -> u64 {
        0
    }
    fn identity(&self) -> Option<&Identity> {
        Some(&self.identity)
    }
}
impl crate::memory::ReadMemory for InFlightProbe {
    fn read(&self, _address: u64, _size: usize) -> Result<Vec<u8>> {
        self.entered
            .try_send(())
            .map_err(|_| Error::new("fixture_read_repeated", "Nonce probe is one-shot"))?;
        self.release
            .lock()
            .map_err(|_| Error::new("fixture_failed", "Nonce probe gate poisoned"))?
            .recv_timeout(Duration::from_secs(8))
            .map_err(|_| Error::new("fixture_failed", "Nonce probe release timed out"))?;
        platform::Process::check(self)?;
        let mut bytes = [0; 32];
        let local = libc::iovec {
            iov_base: bytes.as_mut_ptr().cast(),
            iov_len: bytes.len(),
        };
        let remote = libc::iovec {
            iov_base: self.address as *mut libc::c_void,
            iov_len: bytes.len(),
        };
        // SAFETY: local buffer is valid for exactly 32 bytes; remote address
        // came through the generated owned child's private readiness pipe.
        // This is one read-only kernel call, not a game-layout reader or writer.
        let count = unsafe { libc::process_vm_readv(self.identity.pid, &local, 1, &remote, 1, 0) };
        if count < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        if count != bytes.len() as isize || bytes.as_slice() != self.expected {
            return Err(Error::new(
                "fixture_failed",
                "Generated nonce read was incomplete",
            ));
        }
        platform::Process::check(self)?;
        self.read_ok.store(true, Ordering::Release);
        // Stop the real MCP overview path after the isolated nonce read. No
        // generated allocation is presented as a validated game layout.
        Err(Error::new(
            "fixture_read_complete",
            "Generated nonce read completed",
        ))
    }
}

struct ProcessLimit {
    original: libc::rlimit,
    active: bool,
}
fn current_process_limit() -> libc::rlimit {
    // SAFETY: initialized output buffer receives this process's limits only.
    let mut limit: libc::rlimit = unsafe { std::mem::zeroed() };
    assert_eq!(
        unsafe { libc::getrlimit(libc::RLIMIT_NPROC, &mut limit) },
        0
    );
    limit
}
impl ProcessLimit {
    fn refuse_new_workers() -> Self {
        // SAFETY: only the exact reexec owner helper reaches this code. The
        // process-wide soft limit changes here, never in the outer suite.
        let original = current_process_limit();
        let lowered = libc::rlimit {
            rlim_cur: 0,
            rlim_max: original.rlim_max,
        };
        assert_eq!(unsafe { libc::setrlimit(libc::RLIMIT_NPROC, &lowered) }, 0);
        Self {
            original,
            active: true,
        }
    }
    fn restore(&mut self) {
        // SAFETY: restoring the saved soft/hard limits of this helper only;
        // the hard limit was never lowered and no elevated privilege is needed.
        assert_eq!(
            unsafe { libc::setrlimit(libc::RLIMIT_NPROC, &self.original) },
            0
        );
        self.active = false;
        let actual = current_process_limit();
        assert_eq!(actual.rlim_cur, self.original.rlim_cur);
        assert_eq!(actual.rlim_max, self.original.rlim_max);
    }
}

struct HelperWatchdog(Child);
impl Drop for HelperWatchdog {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            // The exact unreaped helper is our only kill target. Its input
            // writer closes on exit, releasing the generated memory child.
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
impl Drop for ProcessLimit {
    fn drop(&mut self) {
        if self.active {
            // SAFETY: panic recovery restores only this generated child's
            // original limits before fixture cleanup attempts another worker.
            if unsafe { libc::setrlimit(libc::RLIMIT_NPROC, &self.original) } != 0 {
                eprintln!(
                    "Fixture limit restoration failed: {}",
                    std::io::Error::last_os_error()
                );
            }
        }
    }
}

#[test]
fn generated_worker_limit_owner() {
    let Some(token) = selected_helper("HOI4_WORKER_LIMIT_OWNER", LIMIT_OWNER_TEST) else {
        return;
    };
    assert_ne!(uid(), 0);
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", MEMORY_CHILD_TEST, "--nocapture"])
        .env("HOI4_WORKER_MEMORY_CHILD", &token)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    let input = child.child.stdin.take().unwrap();
    let mut output = child.child.stdout.take().unwrap();
    let ready = memory_child_ready(&mut output);
    assert_eq!(ready["pid"], child.child.id());
    assert_eq!(ready["length"], 32);
    let identity = capture_identity(child.child.id() as i32).unwrap();
    let (entered, arrival) = mpsc::sync_channel(1);
    let (release, gate) = mpsc::sync_channel(1);
    let read_ok = Arc::new(AtomicBool::new(false));
    let probe = InFlightProbe {
        identity: identity.clone(),
        address: usize::try_from(ready["address"].as_u64().unwrap()).unwrap(),
        expected: token.into_bytes(),
        entered,
        release: Mutex::new(gate),
        read_ok: Arc::clone(&read_ok),
    };
    let mut fixture = Fixture::with_server(
        child,
        Server {
            process: Some(Box::new(probe)),
            unavailable: None,
        },
    );
    let directory = fixture.directory.clone();
    let pending_read = thread::spawn(move || {
        request(
            &directory,
            json!({"op":"mcp","message":{"jsonrpc":"2.0","id":41,"method":"tools/call","params":{"name":"hoi4_overview","arguments":{}}}}),
        )
    });
    arrival.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(fixture.shared.reads.try_write().is_err());

    let mut limit = ProcessLimit::refuse_new_workers();
    let mut refused = fixture.stream();
    refused
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let envelope = json!({"version":1,"nonce":fixture.endpoint.nonce,"request":{"op":"status"}});
    if let Err(error) =
        refused.write_all(&[serde_json::to_vec(&envelope).unwrap(), vec![b'\n']].concat())
    {
        assert!(matches!(
            error.kind(),
            std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
        ));
    }
    refused
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let mut byte = [0; 1];
    match refused.read(&mut byte) {
        Ok(0) => {}
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
            ) => {}
        result => panic!("Worker refusal must close only the new connection: {result:?}"),
    }
    limit.restore();
    assert!(!fixture.owner.as_ref().unwrap().is_finished());
    assert!(fixture.shared.alive.load(Ordering::Acquire));
    assert!(fixture.shared.accepting_reads.load(Ordering::Acquire));
    assert!(fixture.shared.reads.try_write().is_err());
    assert_eq!(capture_identity(identity.pid).unwrap(), identity);

    release.send(()).unwrap();
    let response = pending_read.join().unwrap().unwrap();
    assert_eq!(response["reply"]["result"]["isError"], true);
    let content: Value = serde_json::from_str(
        response["reply"]["result"]["content"][0]["text"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(content["error"]["code"], "fixture_read_complete");
    assert!(read_ok.load(Ordering::Acquire));
    assert_eq!(fixture.ping()["reply"]["id"], 8);
    assert_eq!(fixture.status()["owned_live"], true);
    assert_eq!(capture_identity(identity.pid).unwrap(), identity);
    fixture.close();
    assert!(capture_identity(identity.pid).is_err());
    assert!(fixture.directory.join("exited.json").is_file());
    drop(input);
}

#[test]
fn worker_creation_refusal_preserves_owner_and_inflight_reads_then_recovers() {
    if !ordinary() {
        return;
    }
    // The suite process never changes its limits. An unreaped exact helper is
    // the only watchdog target; its private pipe EOF also releases its child.
    let original = current_process_limit();
    let mut child = HelperWatchdog(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", LIMIT_OWNER_TEST, "--nocapture"])
            .env("HOI4_WORKER_LIMIT_OWNER", nonce().unwrap())
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(40);
    let status = loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            break status;
        }
        if Instant::now() >= deadline {
            panic!("Isolated worker-refusal fixture exceeded its 40-second deadline");
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert!(
        status.success(),
        "Isolated worker-refusal fixture failed: {status}"
    );
    let actual = current_process_limit();
    assert_eq!(actual.rlim_cur, original.rlim_cur);
    assert_eq!(actual.rlim_max, original.rlim_max);
}

#[test]
fn generated_delayed_exec_child() {
    if selected_helper(
        "HOI4_DELAYED_EXEC_CHILD",
        "launch::linux::tests::generated_delayed_exec_child",
    )
    .is_none()
    {
        return;
    }
    let mut release = [0; 1];
    // Only the generated child's private pipe can release this ordinary exec.
    assert_eq!(std::io::stdin().read(&mut release).unwrap(), 1);
    panic!(
        "Generated exec failed: {}",
        Command::new("/bin/sleep").arg("120").exec()
    );
}

#[test]
fn executable_readiness_waits_for_the_configured_image() {
    if !ordinary() {
        return;
    }
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "launch::linux::tests::generated_delayed_exec_child",
            "--nocapture",
        ])
        .env("HOI4_DELAYED_EXEC_CHILD", nonce().unwrap())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = OwnedChild::spawn(&mut command).unwrap();
    let before = child.identity.clone().unwrap();
    let mut input = child.child.stdin.take().unwrap();
    let release = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        input.write_all(b"x").unwrap();
    });
    let executable = fs::canonicalize("/bin/sleep").unwrap();
    child
        .await_executable(&executable, &fs::metadata(&executable).unwrap())
        .unwrap();
    release.join().unwrap();
    let after = child.identity.clone().unwrap();
    assert_eq!(before.pid, after.pid);
    assert_eq!(before.start_ticks, after.start_ticks);
    assert_ne!(before.exe, after.exe);
    assert_eq!(after.exe, executable.to_string_lossy());
    let mut fixture = Fixture::new(child);
    assert_eq!(fixture.status()["owned_live"], true);
    fixture.close();
    assert!(capture_identity(after.pid).is_err());
}
