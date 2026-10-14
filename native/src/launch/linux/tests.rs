use super::*;
use std::io::BufRead;
use std::sync::atomic::AtomicI32;
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
    fn with_server(child: OwnedChild, server: Server) -> Self {
        Self::with_display(child, server, None)
    }
    fn with_display(mut child: OwnedChild, server: Server, display: Option<Display>) -> Self {
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
            input_gate: Arc::new(RwLock::new(true)),
            alive: AtomicBool::new(true),
            closing,
            commands: RwLock::new(None),
            display,
            directory: directory.clone(),
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

mod window_close;

struct LaunchPathFixture {
    directory: PathBuf,
    config: PathBuf,
}
impl LaunchPathFixture {
    fn new(socket_bytes: usize, unicode: bool) -> Self {
        assert!(ordinary());
        let directory = PathBuf::from(format!("/tmp/h4-run-path-{}", nonce().unwrap()));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&directory)
            .unwrap();
        let prefix = std::os::unix::ffi::OsStrExt::as_bytes(directory.as_os_str()).len() + 1;
        // `/control.sock` is 13 pathname bytes, independently of character count.
        let count = socket_bytes - prefix - 13;
        let name = if unicode {
            format!("{}{}", "é".repeat(count / 2), "a".repeat(count % 2))
        } else {
            "a".repeat(count)
        };
        let run = directory.join(name);
        let socket = run.join("control.sock");
        assert_eq!(
            std::os::unix::ffi::OsStrExt::as_bytes(socket.as_os_str()).len(),
            socket_bytes
        );
        let config = directory.join("config.json");
        // This independent ELF is not the game. Valid arguments must still
        // reach/refuse the existing production build guard, never spawn it.
        json_new(&config,json!({"executable":std::env::current_exe().unwrap(),"run_directory":run,
            "display":{"number":99,"server":{"pid":std::process::id(),"uid":uid(),"exe":"/not-a-display","start_ticks":"0"},
            "xauthority":directory.join("never-opened-authority")},"settings_seed":directory.join("never-opened-seed")})).unwrap();
        Self { directory, config }
    }
    fn entries(&self) -> Vec<std::ffi::OsString> {
        let mut names: Vec<_> = fs::read_dir(&self.directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        names.sort();
        names
    }
    fn run(&self) -> PathBuf {
        json_read::<Value>(&self.config).unwrap()["run_directory"]
            .as_str()
            .unwrap()
            .into()
    }
}
impl Drop for LaunchPathFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn refuse_long_launch(unicode: bool) {
    if !ordinary() {
        return;
    }
    let fixture = LaunchPathFixture::new(108, unicode);
    let before = fixture.entries();
    let bytes = fs::read(&fixture.config).unwrap();
    let error = super::serve(&fixture.config).unwrap_err();
    assert_eq!(
        error.code, "invalid_endpoint",
        "Path error precedes executable/build/display/lock effects"
    );
    assert!(error.message.contains("107 bytes plus the terminating NUL"));
    assert_eq!(fixture.entries(), before);
    assert_eq!(fs::read(&fixture.config).unwrap(), bytes);
    assert!(!fixture.run().exists());
    assert!(!fixture.run().join("userdata").exists());
    assert!(!fixture.run().join("game.log").exists());
    assert!(!fixture.run().join("control.sock").exists());
    assert!(!fixture.run().join("session.json").exists());
}
#[test]
fn overlength_launch_endpoint_refuses_before_run_userdata_or_game_effects() {
    refuse_long_launch(false);
}
#[test]
fn unicode_launch_endpoint_uses_bytes_before_filesystem_effects() {
    refuse_long_launch(true);
}
#[test]
fn maximum_launch_endpoint_passes_path_check_but_keeps_build_guard() {
    if !ordinary() {
        return;
    }
    let fixture = LaunchPathFixture::new(107, false);
    let before = fixture.entries();
    let error = super::serve(&fixture.config).unwrap_err();
    assert_eq!(
        error.code, "unsupported_build",
        "107-byte pathname is valid; independent fixture ELF remains refused"
    );
    assert_eq!(fixture.entries(), before);
    assert!(!fixture.run().exists());
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

const WAIT_EVIDENCE_CHILD: &str = "launch::linux::tests::generated_wait_evidence_child";
const WAIT_EVIDENCE_ENV: &str = "HOI4_GENERATED_WAIT_EVIDENCE_CHILD";
static WAIT_EVIDENCE_EXIT_CODE: AtomicI32 = AtomicI32::new(0);

extern "C" fn generated_wait_exit(_: libc::c_int) {
    // SAFETY: this handler is installed only in the exact private reexec child.
    // The x86_64 atomic load is lock-free and _exit is async-signal-safe.
    unsafe { libc::_exit(WAIT_EVIDENCE_EXIT_CODE.load(Ordering::Relaxed)) }
}

#[test]
fn generated_wait_evidence_child() {
    let Ok(payload) = std::env::var(WAIT_EVIDENCE_ENV) else {
        return;
    };
    let (fixture_nonce, mode) = payload.split_once(':').unwrap();
    assert!(fixture_nonce.len() == 32 && fixture_nonce.bytes().all(|v| v.is_ascii_hexdigit()));
    let handler = match mode {
        "exit0" => {
            WAIT_EVIDENCE_EXIT_CODE.store(0, Ordering::Relaxed);
            generated_wait_exit as *const () as libc::sighandler_t
        }
        "exit6" => {
            WAIT_EVIDENCE_EXIT_CODE.store(6, Ordering::Relaxed);
            generated_wait_exit as *const () as libc::sighandler_t
        }
        "signal" => libc::SIG_DFL,
        _ => panic!("Unexpected private wait fixture mode"),
    };
    // SAFETY: only this reexec fixture's disposition changes; suite parent,
    // game and display processes are untouched. Production still sends SIGTERM.
    assert_ne!(
        unsafe { libc::signal(libc::SIGTERM, handler) },
        libc::SIG_ERR
    );
    println!("wait-evidence-ready:{fixture_nonce}");
    std::io::stdout().flush().unwrap();
    let mut byte = [0];
    // EOF is a safe escape if the private parent disappears. Successful cases
    // exit through caught/default SIGTERM after the nonce handshake above.
    let _ = std::io::stdin().read(&mut byte);
    std::process::exit(97);
}

/// Fixture-only startup ownership. Release the private EOF escape before the
/// production OwnedChild Drop runs, including on an unexpected unwind.
struct WaitEvidenceStartup(Option<OwnedChild>);
impl WaitEvidenceStartup {
    fn close_stdin(&mut self) {
        if let Some(child) = self.0.as_mut() {
            drop(child.child.stdin.take());
        }
    }
    fn into_child(mut self) -> Result<OwnedChild> {
        self.0
            .take()
            .ok_or_else(|| Error::new("fixture_failed", "Startup child missing"))
    }
}
impl Drop for WaitEvidenceStartup {
    fn drop(&mut self) {
        self.close_stdin();
    }
}

fn wait_evidence_ready(ready: &mut UnixStream, marker: &[u8]) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut seen = Vec::new();
    // Libtest may prefix the marker. Bound total bytes and every blocking read
    // by one absolute deadline; no unbounded read_line startup wait.
    for _ in 0..4096 {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(Error::new(
                "fixture_startup_timeout",
                "Private wait fixture did not become ready",
            ));
        }
        ready.set_read_timeout(Some(remaining))?;
        let mut byte = [0];
        ready.read_exact(&mut byte)?;
        seen.push(byte[0]);
        if seen.ends_with(marker) {
            return Ok(());
        }
    }
    Err(Error::new(
        "fixture_startup_bound",
        "Private wait fixture handshake exceeded byte bound",
    ))
}

fn wait_evidence_child(mode: &str) -> Result<OwnedChild> {
    if !matches!(mode, "exit0" | "exit6" | "signal") {
        return Err(Error::new("fixture_failed", "Unknown private wait mode"));
    }
    let fixture_nonce = nonce()?;
    let marker = format!("wait-evidence-ready:{fixture_nonce}\n");
    let (mut ready, output) = UnixStream::pair()?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .args([
            "--exact",
            WAIT_EVIDENCE_CHILD,
            "--nocapture",
            "--test-threads=1",
        ])
        .env(WAIT_EVIDENCE_ENV, format!("{fixture_nonce}:{mode}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::from(std::os::fd::OwnedFd::from(output)))
        .stderr(Stdio::null());
    let mut startup = WaitEvidenceStartup(Some(OwnedChild::spawn(&mut command)?));
    if let Err(error) = wait_evidence_ready(&mut ready, marker.as_bytes()) {
        // Explicitly release stdin before preserved ownership is dropped and
        // before the caller can raise a test failure. Drop also covers unwind.
        startup.close_stdin();
        return Err(error);
    }
    startup.into_child()
}

fn assert_owned_wait_evidence(
    mode: &str,
    expected_code: Option<i32>,
    expected_signal: Option<i32>,
) {
    if !ordinary() {
        return;
    }
    let child = wait_evidence_child(mode).unwrap();
    let mut fixture = Fixture::new(child);
    let response = request(&fixture.directory, json!({"op":"close"})).unwrap();
    fixture.owner.take().unwrap().join().unwrap().unwrap();
    let record: Value = json_read(&fixture.directory.join("exited.json")).unwrap();
    assert_eq!(response["closed"], true);
    assert_eq!(record["game"], json!(fixture.endpoint.session.game));
    assert_eq!(record["reason"], "owned child exited");
    let exit = &response["exit"];
    for key in ["exit_code", "signal", "core_dumped", "wait_status"] {
        assert_eq!(
            exit[key], record[key],
            "Response differs from sole-reaper record: {key}"
        );
    }
    assert_eq!(exit["exit_code"], json!(expected_code));
    assert_eq!(exit["signal"], json!(expected_signal));
    // SIGTERM never carries the default core-dump action. No core limit or
    // VM/global signal setting is changed by these fixtures.
    assert_eq!(exit["core_dumped"], false);
    let actual_wait = exit["wait_status"].as_str().unwrap();
    if let Some(code) = expected_code {
        assert_eq!(actual_wait, format!("exit status: {code}"));
    } else {
        assert!(actual_wait.starts_with("signal: 15"), "{actual_wait}");
    }
    assert!(capture_identity(fixture.endpoint.session.game.pid).is_err());
}

#[test]
fn owned_close_preserves_normal_exit_zero_evidence() {
    assert_owned_wait_evidence("exit0", Some(0), None);
}

#[test]
fn owned_close_preserves_normal_exit_six_without_signal_conflation() {
    assert_owned_wait_evidence("exit6", Some(6), None);
}

#[test]
fn owned_close_preserves_signal_termination_without_exit_code() {
    assert_owned_wait_evidence("signal", None, Some(libc::SIGTERM));
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

fn window_target(game: Identity) -> WindowCloseTarget {
    WindowCloseTarget {
        game,
        window: 123,
        window_class: b"generated-close\0generated-close\0".to_vec(),
    }
}
fn window_attempt(target: WindowCloseTarget, until: Instant) -> WindowCloseAttempt {
    WindowCloseAttempt {
        target,
        until,
        quiesced: false,
        started: false,
        delivery: CloseDelivery::NotQueued,
        error: None,
        pending: Vec::new(),
    }
}

#[test]
fn window_close_target_is_exact_and_bounded_before_queueing() {
    let game = Identity {
        pid: 13,
        uid: 1000,
        start_ticks: "19".into(),
        exe: "/fixture".into(),
    };
    let target = window_target(game.clone());
    target.validate(&game).unwrap();
    for class in [
        vec![],
        b"partial".to_vec(),
        b"one\0\0".to_vec(),
        vec![b'a'; 257],
        b"one\0two\0three\0".to_vec(),
    ] {
        let mut invalid = target.clone();
        invalid.window_class = class;
        assert_eq!(
            invalid.validate(&game).unwrap_err().code,
            "invalid_window_binding"
        );
    }
    let mut changed = target.clone();
    changed.game.start_ticks = "20".into();
    assert!(changed.validate(&game).is_err());
    changed = target.clone();
    changed.window = 0;
    assert!(changed.validate(&game).is_err());
    let mut value = json!(target);
    value["display"] = json!(120);
    assert!(serde_json::from_value::<WindowCloseTarget>(value).is_err());
    let mut value = json!(target);
    value["game"]["extra"] = json!(true);
    assert!(serde_json::from_value::<WindowCloseTarget>(value).is_err());
}

#[test]
fn window_close_waits_for_whole_input_pair_and_never_replays_unknown_delivery() {
    let game = Identity {
        pid: 13,
        uid: 1000,
        start_ticks: "19".into(),
        exe: "/fixture".into(),
    };
    let mut attempt = window_attempt(
        window_target(game),
        Instant::now() + Duration::from_secs(20),
    );
    let gate = RwLock::new(true);
    let pair = gate.read().unwrap();
    advance_window_close(&mut attempt, &gate, |_, _| {
        panic!("Must not dispatch during key pair")
    });
    assert!(!attempt.started);
    assert!(*pair);
    drop(pair);
    let mut calls = 0;
    advance_window_close(&mut attempt, &gate, |_, until| {
        calls += 1;
        assert!(until <= attempt_deadline_for_test());
        assert!(
            !*gate.try_read().unwrap(),
            "Writer is released, stored false remains"
        );
        CloseDispatch {
            delivery: CloseDelivery::Unknown,
            error: Some(Error::new("fixture_wire_error", "Uncertain send")),
        }
    });
    assert_eq!(calls, 1);
    assert!(attempt.quiesced);
    assert_eq!(attempt.delivery, CloseDelivery::Unknown);
    attempt.until = Instant::now() - Duration::from_millis(1);
    for _ in 0..3 {
        advance_window_close(&mut attempt, &gate, |_, _| {
            panic!("Unknown send must not replay")
        });
    }
    assert_eq!(
        attempt.error.as_ref().unwrap()["code"],
        "fixture_wire_error"
    );
    assert!(attempt.reply_ready(&None));
}
fn attempt_deadline_for_test() -> Instant {
    // Dispatch gets at most two seconds, not a per-property renewed budget.
    Instant::now() + Duration::from_secs(2)
}

#[test]
fn window_close_expiry_still_disables_input_after_pair_release_without_dispatch() {
    let game = Identity {
        pid: 13,
        uid: 1000,
        start_ticks: "19".into(),
        exe: "/fixture".into(),
    };
    let mut attempt = window_attempt(
        window_target(game),
        Instant::now() - Duration::from_millis(1),
    );
    let gate = RwLock::new(true);
    let pair = gate.read().unwrap();
    advance_window_close(&mut attempt, &gate, |_, _| {
        panic!("Expired close must not dispatch")
    });
    assert!(attempt.started);
    assert!(!attempt.quiesced);
    assert_eq!(attempt.delivery, CloseDelivery::NotQueued);
    drop(pair);
    advance_window_close(&mut attempt, &gate, |_, _| {
        panic!("Expired close must not dispatch")
    });
    assert!(attempt.quiesced);
    assert!(!*gate.read().unwrap());
}

#[test]
fn window_close_adapter_panic_is_unknown_and_never_unwinds_owner() {
    let game = Identity {
        pid: 13,
        uid: 1000,
        start_ticks: "19".into(),
        exe: "/fixture".into(),
    };
    let mut attempt = window_attempt(
        window_target(game),
        Instant::now() + Duration::from_secs(20),
    );
    let gate = RwLock::new(true);
    advance_window_close(&mut attempt, &gate, |_, _| {
        panic!("Generated adapter panic")
    });
    assert_eq!(attempt.delivery, CloseDelivery::Unknown);
    assert!(!*gate.read().unwrap());
    advance_window_close(&mut attempt, &gate, |_, _| {
        panic!("Never replay after caught panic")
    });
}

struct WindowFixtureProcess {
    identity: Identity,
    checks: Arc<std::sync::atomic::AtomicUsize>,
}
impl crate::memory::ReadMemory for WindowFixtureProcess {
    fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
        Err(Error::new(
            "fixture_only",
            "No game-memory payload in close fixture",
        ))
    }
}
impl platform::Process for WindowFixtureProcess {
    fn check(&self) -> Result<()> {
        self.checks.fetch_add(1, Ordering::Relaxed);
        verify_identity(&self.identity)
    }
    fn image_base(&self) -> u64 {
        0
    }
    fn identity(&self) -> Option<&Identity> {
        Some(&self.identity)
    }
}

#[test]
fn refused_window_close_preserves_child_read_and_endpoint_without_replay() {
    if !ordinary() {
        return;
    }
    let mut command = Command::new("/bin/sleep");
    command
        .arg("120")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = OwnedChild::spawn(&mut command).unwrap();
    let identity = child.identity.clone().unwrap();
    let checks = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut fixture = Fixture::with_server(
        child,
        Server {
            process: Some(Box::new(WindowFixtureProcess {
                identity: identity.clone(),
                checks: checks.clone(),
            })),
            unavailable: None,
        },
    );
    let target = window_target(identity.clone());
    // A real outstanding reader remains pinned. Window close must not wait for
    // its observation or accidentally reap/signal the still-running child.
    let reading = fixture.shared.reads.read().unwrap();
    let pair = fixture.shared.input_gate.read().unwrap();
    let path = fixture.directory.clone();
    let requested_target = target.clone();
    let request_thread = thread::spawn(move || {
        request(
            &path,
            json!({"op":"close_window","target":requested_target}),
        )
    });
    let until = Instant::now() + Duration::from_secs(3);
    while fixture.shared.accepting_reads.load(Ordering::Acquire) {
        assert!(
            Instant::now() < until,
            "WM request did not enter owner queue"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(checks.load(Ordering::Relaxed), 0);
    assert_eq!(capture_identity(identity.pid).unwrap(), identity);
    drop(pair);
    let response = request_thread.join().unwrap().unwrap();
    assert_eq!(response["closed"], false);
    assert_eq!(response["window_close"]["delivery"], "not_queued");
    assert_eq!(
        response["window_close"]["error"]["code"],
        "window_close_unavailable"
    );
    assert!(!fixture.owner.as_ref().unwrap().is_finished());
    assert!(fixture.shared.alive.load(Ordering::Acquire));
    assert!(!*fixture.shared.input_gate.read().unwrap());
    assert_eq!(capture_identity(identity.pid).unwrap(), identity);
    let original_checks = checks.load(Ordering::Relaxed);
    assert_eq!(
        original_checks, 1,
        "Fresh reader check preceded display capability refusal"
    );
    for _ in 0..2 {
        let repeated = request(
            &fixture.directory,
            json!({"op":"close_window","target":target}),
        )
        .unwrap();
        assert_eq!(repeated, response);
    }
    assert_eq!(checks.load(Ordering::Relaxed), original_checks);
    let mut changed = target.clone();
    changed.window += 1;
    assert_eq!(
        request(
            &fixture.directory,
            json!({"op":"close_window","target":changed})
        )
        .unwrap()["error"]["code"],
        "window_close_target_changed"
    );
    assert_eq!(fixture.status()["owned_live"], true);
    assert!(!fixture.directory.join("exited.json").exists());
    drop(reading);
    // Only this separate explicit legacy fixture close is allowed to signal.
    fixture.close();
    let exited: Value = json_read(&fixture.directory.join("exited.json")).unwrap();
    assert_eq!(exited["exit_code"], Value::Null);
    assert_eq!(exited["signal"], libc::SIGTERM);
}

// Private scoped generated-process diagnostics. No game/display/backend access.
#[test]
fn generated_short_delayed_exec_child() {
    if selected_helper(
        "HOI4_SHORT_EXEC_CHILD",
        "launch::linux::tests::generated_short_delayed_exec_child",
    )
    .is_none()
    {
        return;
    }
    let mut release = [0; 1];
    assert_eq!(std::io::stdin().read(&mut release).unwrap(), 1);
    panic!(
        "Generated exec failed: {}",
        Command::new("/bin/sleep").arg("2").exec()
    );
}
fn private_ready_child() -> OwnedChild {
    let mut c = Command::new("/bin/sleep");
    c.arg("5");
    OwnedChild::spawn(&mut c).unwrap()
}
fn private_finish(mut c: OwnedChild) {
    c.terminate().unwrap();
    c.child.wait().unwrap();
    c.exited = true;
}
#[test]
fn readiness_stage_notfound_retries_against_real_pinned_child() {
    if !ordinary() {
        return;
    }
    for stage in ["exe_readlink", "exe_metadata"] {
        let mut c = private_ready_child();
        let mut calls = 0;
        let exe = fs::canonicalize("/bin/sleep").unwrap();
        let meta = fs::metadata(&exe).unwrap();
        c.await_executable_observed(&exe, &meta, &mut |pid| {
            calls += 1;
            if calls == 1 {
                Err((stage, std::io::Error::from_raw_os_error(libc::ENOENT)))
            } else {
                readiness_image(pid)
            }
        })
        .unwrap();
        assert_eq!(calls, 3);
        assert_eq!(c.identity.as_ref().unwrap().exe, exe.to_string_lossy());
        private_finish(c);
    }
}
#[test]
fn readiness_nontransient_image_errors_do_not_retry() {
    if !ordinary() {
        return;
    }
    for (stage, errno, code) in [
        ("exe_readlink", libc::EACCES, "permission_denied"),
        ("exe_metadata", libc::EIO, "io_error"),
        ("exe_path_encoding", libc::ENOENT, "io_error"),
    ] {
        let mut c = private_ready_child();
        let mut calls = 0;
        let exe = fs::canonicalize("/bin/sleep").unwrap();
        let e = c
            .await_executable_observed(&exe, &fs::metadata(&exe).unwrap(), &mut |_| {
                calls += 1;
                Err((stage, std::io::Error::from_raw_os_error(errno)))
            })
            .unwrap_err();
        assert_eq!(e.code, code);
        assert!(e.message.contains(stage));
        assert_eq!(calls, 1);
        private_finish(c);
    }
}
#[test]
fn readiness_prior_creation_uid_and_pid_are_not_reset() {
    if !ordinary() {
        return;
    }
    for field in 0..3 {
        let mut c = private_ready_child();
        let original = c.identity.clone().unwrap();
        let id = c.identity.as_mut().unwrap();
        match field {
            0 => id.start_ticks.push('0'),
            1 => id.uid += 1,
            _ => id.pid += 1,
        };
        let mut calls = 0;
        let exe = fs::canonicalize("/bin/sleep").unwrap();
        let e = c
            .await_executable_observed(&exe, &fs::metadata(&exe).unwrap(), &mut |pid| {
                calls += 1;
                readiness_image(pid)
            })
            .unwrap_err();
        assert_eq!(e.code, "ownership_changed");
        assert_eq!(calls, 0);
        c.identity = Some(original);
        private_finish(c);
    }
}
#[test]
fn readiness_exact_device_inode_and_second_image_are_required() {
    if !ordinary() {
        return;
    }
    for field in 0..3 {
        let mut c = private_ready_child();
        let mut calls = 0;
        let exe = fs::canonicalize("/bin/sleep").unwrap();
        c.await_executable_observed(&exe, &fs::metadata(&exe).unwrap(), &mut |pid| {
            calls += 1;
            let mut i = readiness_image(pid)?;
            if (field < 2 && calls == 1) || (field == 2 && calls == 2) {
                match field {
                    0 => i.device += 1,
                    1 => i.inode += 1,
                    _ => i.path = "/unconfigured/image".into(),
                }
            }
            Ok(i)
        })
        .unwrap();
        assert!(calls >= 3);
        private_finish(c);
    }
}
#[test]
fn readiness_late_image_cannot_publish_success() {
    if !ordinary() {
        return;
    }
    let mut c = private_ready_child();
    let before = c.identity.clone();
    let exe = fs::canonicalize("/bin/sleep").unwrap();
    let e = c
        .await_executable_observed(&exe, &fs::metadata(&exe).unwrap(), &mut |pid| {
            thread::sleep(Duration::from_millis(2100));
            readiness_image(pid)
        })
        .unwrap_err();
    assert_eq!(e.code, "launch_not_ready");
    assert_eq!(c.identity, before);
    private_finish(c);
}
// This accepts an observed conservative refusal, never a ready identity. It
// cannot classify other ownership changes as expected or retry the input/exec.
fn parent_zero_exec_refusal(
    error: &Error,
    expected: &Option<ReadinessAnchor>,
    observed: &ReadinessAnchor,
    before: &Identity,
) -> bool {
    let Some(expected) = expected else {
        return false;
    };
    error.code == "ownership_changed"
        && expected.parent == std::process::id() as i32
        && expected.pid == before.pid
        && observed.pid == before.pid
        && expected.uid == before.uid
        && observed.uid == before.uid
        && before.uid == uid()
        && expected.start_ticks == before.start_ticks
        && observed.start_ticks == before.start_ticks
        && observed.parent == 0
}
#[test]
fn exec_stress_refusal_policy_is_exact_not_blanket_ownership_ignore() {
    let before = Identity {
        pid: 7,
        uid: uid(),
        start_ticks: "123".into(),
        exe: "prior_image".into(),
    };
    let expected = ReadinessAnchor {
        pid: before.pid,
        uid: before.uid,
        parent: std::process::id() as i32,
        start_ticks: before.start_ticks.clone(),
    };
    let mut observed = expected.clone();
    observed.parent = 0;
    let error = Error::new("ownership_changed", "Generated typed guard refusal");
    assert!(parent_zero_exec_refusal(
        &error,
        &Some(expected.clone()),
        &observed,
        &before
    ));
    for field in 0..4 {
        let mut wrong = observed.clone();
        match field {
            0 => wrong.pid += 1,
            1 => wrong.uid += 1,
            2 => wrong.start_ticks.push('0'),
            _ => wrong.parent = 1,
        };
        assert!(!parent_zero_exec_refusal(
            &error,
            &Some(expected.clone()),
            &wrong,
            &before
        ));
    }
    for code in ["io_error", "launch_not_ready", "launch_exited"] {
        assert!(!parent_zero_exec_refusal(
            &Error::new(code, "Other refusal"),
            &Some(expected.clone()),
            &observed,
            &before
        ));
    }
    assert!(!parent_zero_exec_refusal(&error, &None, &observed, &before));
}
#[test]
fn readiness_threaded_exec_collects_actual_image_operation_failures() {
    if !ordinary() {
        return;
    }
    let mut genuine = 0;
    let mut ready = 0;
    let mut parent_zero_refusals = 0;
    for iteration in 0..12 {
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "--exact",
            "launch::linux::tests::generated_short_delayed_exec_child",
            "--nocapture",
        ])
        .env("HOI4_SHORT_EXEC_CHILD", nonce().unwrap())
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
        let mut child = OwnedChild::spawn(&mut cmd).unwrap();
        let before = child.identity.clone().unwrap();
        let mut input = child.child.stdin.take().unwrap();
        let release = thread::spawn(move || {
            thread::sleep(Duration::from_millis(50));
            input.write_all(b"x").unwrap();
        });
        let exe = fs::canonicalize("/bin/sleep").unwrap();
        LAST_READINESS_ANCHOR_REFUSAL.with(|last| {
            last.borrow_mut().take();
        });
        let result=child.await_executable_observed(&exe,&fs::metadata(&exe).unwrap(),&mut |pid| {
            // Diagnostic-only tighter read-only sampling of generated exec.
            // Production observer retains 5ms polling; its exact timing is not reproduced.
            let sample_until=Instant::now()+Duration::from_millis(100);
            let result=loop {
                let result=readiness_image(pid);
                match &result {
                    Ok(image) if image.path!=exe.to_string_lossy()&&Instant::now()<sample_until=>continue,
                    _=>break result,
                }
            };
            if let Err((stage,e))=&result {eprintln!("ACTUAL_EXEC_OBSERVATION iteration={iteration} stage={stage} errno={:?} kind={:?}",e.raw_os_error(),e.kind());genuine+=1;}
            result
        });
        release.join().unwrap();
        match result {
            Ok(()) => {
                ready += 1;
                let after = child.identity.clone().unwrap();
                assert_eq!(before.pid, after.pid);
                assert_eq!(before.uid, after.uid);
                assert_eq!(before.start_ticks, after.start_ticks);
                assert_eq!(after.exe, exe.to_string_lossy());
                let image = readiness_image(after.pid).unwrap();
                let expected = fs::metadata(&exe).unwrap();
                assert_eq!(
                    (image.device, image.inode),
                    (expected.dev(), expected.ino())
                );
                private_finish(child);
            }
            Err(error) => {
                let refusal = LAST_READINESS_ANCHOR_REFUSAL.with(|last| last.borrow_mut().take());
                let (expected, observed) = refusal
                    .as_ref()
                    .expect("Unclassified stress refusal must fail");
                assert!(
                    parent_zero_exec_refusal(&error, expected, observed, &before),
                    "Unexpected readiness failure: {error}; typed={refusal:?}"
                );
                assert_eq!(
                    child.identity,
                    Some(before.clone()),
                    "Refusal must not publish a new executable identity"
                );
                eprintln!("ACTUAL_EXEC_GUARD_REFUSAL iteration={iteration} code={} expected={expected:?} observed={observed:?}; readiness_not_verified=true; no_retry_or_signal",error.code);
                parent_zero_refusals += 1;
                // Only the pinned generated child; preserve until its ordinary
                // sleep2 exit. Never retry readiness or signal an unverified image.
                let until = Instant::now() + Duration::from_secs(5);
                while !child.exited_without_reaping().unwrap() {
                    assert!(
                        Instant::now() < until,
                        "Generated refusal child exceeded bounded natural-exit fixture"
                    );
                    thread::sleep(Duration::from_millis(5));
                }
                let status = child.child.wait().unwrap();
                child.exited = true;
                assert!(status.success(), "Generated exec child failed: {status}");
            }
        }
    }
    assert_eq!(ready + parent_zero_refusals, 12);
    eprintln!("ACTUAL_EXEC_OBSERVATION summary generated_execs=12 ready={ready} conservative_parent0_refusals={parent_zero_refusals} real_image_errors={genuine}; refusal_is_not_readiness_success");
}
