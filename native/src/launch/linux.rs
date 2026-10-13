use crate::{
    error::{Error, Result},
    framing,
    game::{BUILD_ID, SHA256},
    platform::{
        self, capture_identity,
        x11_close::{CloseDelivery, CloseDispatch, X11WindowClose},
        Identity, Session,
    },
    protocol::Server,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
            fs::{DirBuilderExt, FileTypeExt, MetadataExt, OpenOptionsExt, PermissionsExt},
            net::{UnixListener, UnixStream},
            process::{CommandExt, ExitStatusExt},
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
        Arc, RwLock,
    },
    thread,
    time::{Duration, Instant},
};
mod clock;

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Display {
    number: u16,
    server: Identity,
    xauthority: PathBuf,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Renderer {
    #[default]
    Default,
    LlvmPipe,
    Virpipe,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    executable: PathBuf,
    run_directory: PathBuf,
    display: Display,
    settings_seed: Option<PathBuf>,
    #[serde(default)]
    renderer: Renderer,
    gpu_transport: Option<PathBuf>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Endpoint {
    version: u8,
    nonce: String,
    server: Identity,
    session: Session,
}
fn uid() -> u32 {
    // SAFETY: getuid has no arguments or memory effects.
    unsafe { libc::getuid() }
}
fn private(path: &Path, directory: bool) -> Result<()> {
    let m = fs::symlink_metadata(path)?;
    if m.uid() != uid()
        || m.mode() & 0o077 != 0
        || (if directory { !m.is_dir() } else { !m.is_file() })
    {
        return Err(Error::new(
            "invalid_endpoint",
            "Path must be private and owned by the ordinary runtime user",
        ));
    }
    Ok(())
}
fn json_read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T> {
    let mut bytes = Vec::new();
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.uid() != uid() || meta.mode() & 0o077 != 0 {
        return Err(Error::new(
            "invalid_endpoint",
            "Private regular configuration file required",
        ));
    }
    file.take(65537).read_to_end(&mut bytes)?;
    if bytes.len() > 65536 {
        return Err(Error::new("invalid_session", "Configuration exceeds bound"));
    }
    serde_json::from_slice(&bytes).map_err(|e| Error::new("invalid_session", e.to_string()))
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}
fn json_new(path: &Path, value: impl Serialize) -> Result<()> {
    write_new(
        path,
        &serde_json::to_vec(&value).map_err(|e| Error::new("invalid_session", e.to_string()))?,
    )
}
fn verify_identity(expected: &Identity) -> Result<()> {
    if expected.uid != uid() || expected.uid == 0 || capture_identity(expected.pid)? != *expected {
        return Err(Error::new(
            "ownership_changed",
            "Owned endpoint/display process identity changed",
        ));
    }
    Ok(())
}
fn peer(stream: &UnixStream) -> Result<Identity> {
    let mut cred: libc::ucred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut length = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    // SAFETY: valid live socket, correctly sized output credential buffer.
    let n = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    if n != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if length as usize != std::mem::size_of::<libc::ucred>() || cred.uid != uid() || cred.uid == 0 {
        return Err(Error::new(
            "ownership_changed",
            "Endpoint peer must be the same ordinary user",
        ));
    }
    let identity = capture_identity(cred.pid)?;
    if identity.uid != cred.uid {
        return Err(Error::new(
            "ownership_changed",
            "Endpoint peer identity changed",
        ));
    }
    Ok(identity)
}
fn read_value(mut stream: &UnixStream, timeout: Duration) -> Result<Value> {
    let deadline = Instant::now() + timeout;
    let mut bytes = Vec::new();
    let mut buffer = [0; 4096];
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(|| Error::new("transport_timeout", "Endpoint read deadline exceeded"))?;
        stream.set_read_timeout(Some(remaining))?;
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            return Err(Error::new(
                "transport_closed",
                "Endpoint closed before complete record",
            ));
        }
        if bytes.len() + count > framing::LIMIT {
            return Err(Error::new("request_bound", "Endpoint record exceeds bound"));
        }
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.iter().position(|b| *b == b'\n') {
            if end + 1 != bytes.len() {
                return Err(Error::new(
                    "invalid_request",
                    "One endpoint record per connection required",
                ));
            }
            return serde_json::from_slice(&bytes[..end])
                .map_err(|e| Error::new("invalid_request", e.to_string()));
        }
    }
}
fn send_value(mut stream: &UnixStream, value: &Value) -> Result<()> {
    let mut bytes =
        serde_json::to_vec(value).map_err(|e| Error::new("transport_failed", e.to_string()))?;
    bytes.push(b'\n');
    if bytes.len() > framing::LIMIT {
        return Err(Error::new(
            "response_bound",
            "Endpoint response exceeds bound",
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut remaining = bytes.as_slice();
    while !remaining.is_empty() {
        stream.set_write_timeout(Some(
            deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| {
                    Error::new("transport_timeout", "Endpoint write deadline exceeded")
                })?,
        ))?;
        let count = stream.write(remaining)?;
        if count == 0 {
            return Err(Error::new(
                "transport_closed",
                "Endpoint closed during reply",
            ));
        }
        remaining = &remaining[count..];
    }
    Ok(())
}
fn request(directory: &Path, value: Value) -> Result<Value> {
    if uid() == 0 {
        return Err(Error::new(
            "permission_denied",
            "Do not run the native runtime as root",
        ));
    }
    private(directory, true)?;
    private(&directory.join("endpoint.json"), false)?;
    let endpoint: Endpoint = json_read(&directory.join("endpoint.json"))?;
    if endpoint.version != 1 || endpoint.nonce.len() != 32 {
        return Err(Error::new(
            "invalid_endpoint",
            "Endpoint version/nonce invalid",
        ));
    }
    verify_identity(&endpoint.server)?;
    let socket = directory.join("control.sock");
    let m = fs::symlink_metadata(&socket)?;
    if !m.file_type().is_socket() || m.uid() != uid() || m.mode() & 0o077 != 0 {
        return Err(Error::new(
            "invalid_endpoint",
            "Private endpoint socket required",
        ));
    }
    let stream = platform::connect_socket(&socket)?;
    if peer(&stream)? != endpoint.server {
        return Err(Error::new(
            "ownership_changed",
            "Connected peer is not the recorded launch supervisor",
        ));
    }
    send_value(
        &stream,
        &json!({"version":1,"nonce":endpoint.nonce,"request":value}),
    )?;
    // No reconnect/retry: a lost reply may follow an accepted future action.
    let reply = read_value(&stream, Duration::from_secs(55))?;
    if !matches!(
        value.get("op").and_then(Value::as_str),
        Some("close" | "close_window")
    ) {
        verify_identity(&endpoint.server)?;
    }
    Ok(reply)
}
pub fn connect(directory: &Path) -> Result<()> {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    framing::serve(stdin.lock(), stdout.lock(), |message| {
        let id = message.get("id").cloned();
        match request(directory, json!({"op":"mcp","message":message})) {
            Ok(reply) if reply.get("error").is_some() => id.map(|id| json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":reply["error"].to_string()}})),
            Ok(reply) => reply.get("reply").filter(|r| !r.is_null()).cloned(),
            Err(e) => {
                eprintln!("{e}");
                id.map(|id| json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,"message":e.to_string()}}))
            }
        }
    })?;
    Ok(())
}
pub fn bind_input(directory: &Path, config: &Path) -> Result<()> {
    let binding: Value = json_read(config)?;
    let reply = request(directory, json!({"op":"bind_input","binding":binding}))?;
    if reply.get("bound") != Some(&json!(true)) {
        return Err(Error::new("input_binding_failed", reply.to_string()));
    }
    eprintln!("Owned ordinary clock input bound; campaign acceptance remains unverified");
    Ok(())
}
pub fn close(directory: &Path) -> Result<()> {
    let reply = request(directory, json!({"op":"close"}))?;
    if reply.get("closed") != Some(&json!(true)) {
        return Err(Error::new(
            "shutdown_pending",
            "Owned game did not exit; supervisor remains alive",
        ));
    }
    Ok(())
}
pub fn close_window(directory: &Path, config: &Path) -> Result<()> {
    let target: WindowCloseTarget = json_read(config)?;
    let reply = request(directory, json!({"op":"close_window","target":target}))?;
    if reply.get("closed") != Some(&json!(true)) {
        return Err(Error::new("shutdown_pending", reply.to_string()));
    }
    Ok(())
}
fn duplicate_game(executable: &Path) -> Result<()> {
    let expected = fs::metadata(executable)?;
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        if entry.file_name().to_string_lossy().parse::<i32>().is_err() {
            continue;
        }
        if let Ok(meta) = fs::metadata(entry.path().join("exe")) {
            if meta.dev() == expected.dev() && meta.ino() == expected.ino() {
                return Err(Error::new(
                    "game_already_running",
                    "An installed game instance already exists; never take it over",
                ));
            }
        }
    }
    Ok(())
}
/// A pinned child and its only reaper. Unexpected owner errors preserve the
/// parent until the launched child exits, even when graceful shutdown hangs.
struct OwnedChild {
    child: Child,
    identity: Option<Identity>,
    pidfd: File,
    exited: bool,
}
impl OwnedChild {
    fn spawn(command: &mut Command) -> Result<Self> {
        // A spawned PID may still expose its pre-exec image briefly. Capture
        // the intended file before launch; never publish that transitional
        // parent image as the game's executable identity.
        let executable = fs::canonicalize(command.get_program())?;
        let executable_meta = fs::metadata(&executable)?;
        if !executable_meta.is_file() {
            return Err(Error::new(
                "invalid_executable",
                "Regular executable required",
            ));
        }
        // Refuse an unavailable pidfd API before creating a game process.
        // SAFETY: opening our own process descriptor has no game side effects.
        let probe = unsafe { libc::syscall(libc::SYS_pidfd_open, std::process::id() as i32, 0) };
        if probe < 0 {
            return Err(Error::new(
                "platform_unavailable",
                format!(
                    "Pinned process descriptors unavailable: {}",
                    std::io::Error::last_os_error()
                ),
            ));
        }
        // SAFETY: successful syscall returned a new descriptor, closed here.
        drop(unsafe { <File as std::os::fd::FromRawFd>::from_raw_fd(probe as i32) });
        let mut child = command.spawn()?;
        // SAFETY: pidfd_open identifies our still-unreaped direct child.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, child.id() as i32, 0) };
        if fd < 0 {
            let error = std::io::Error::last_os_error();
            eprintln!("Cannot pin launched child; preserving parent until it exits");
            let _ = child.wait();
            return Err(error.into());
        }
        // SAFETY: successful pidfd_open returns a new descriptor owned here.
        let pidfd = unsafe { <File as std::os::fd::FromRawFd>::from_raw_fd(fd as i32) };
        let mut owned = Self {
            child,
            identity: None,
            pidfd,
            exited: false,
        };
        owned.await_executable(&executable, &executable_meta)?;
        Ok(owned)
    }
    fn await_executable(&mut self, executable: &Path, expected: &fs::Metadata) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut creation = None;
        loop {
            if self.exited_without_reaping()? {
                return Err(Error::new(
                    "launch_exited",
                    "Child exited before executable readiness",
                ));
            }
            let identity = capture_identity(self.child.id() as i32)?;
            let (_, parent, ticks) =
                platform::parse_stat(&fs::read_to_string(format!("/proc/{}/stat", identity.pid))?)?;
            let first = creation.get_or_insert(identity.start_ticks.clone());
            if identity.uid != uid()
                || parent != std::process::id() as i32
                || first != &identity.start_ticks
                || ticks != identity.start_ticks
            {
                return Err(Error::new(
                    "ownership_changed",
                    "Launched child ownership changed",
                ));
            }
            let actual = fs::metadata(format!("/proc/{}/exe", identity.pid))?;
            if identity.exe == executable.to_string_lossy()
                && actual.dev() == expected.dev()
                && actual.ino() == expected.ino()
                && capture_identity(identity.pid)? == identity
            {
                self.identity = Some(identity);
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Error::new(
                    "launch_not_ready",
                    "Configured executable was not observed",
                ));
            }
            thread::sleep(Duration::from_millis(5));
        }
    }
    fn exited_without_reaping(&self) -> Result<bool> {
        let mut poll = libc::pollfd {
            fd: self.pidfd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: poll references a live pinned child descriptor.
        let n = unsafe { libc::poll(&mut poll, 1, 0) };
        if n < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(n != 0 && poll.revents & libc::POLLIN != 0)
    }
    fn terminate(&self) -> Result<()> {
        if self.exited_without_reaping()? {
            return Ok(());
        }
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| Error::new("ownership_changed", "Child identity was not established"))?;
        verify_identity(identity)?;
        let (_, parent, _) =
            platform::parse_stat(&fs::read_to_string(format!("/proc/{}/stat", identity.pid))?)?;
        if parent != std::process::id() as i32 {
            return Err(Error::new(
                "ownership_changed",
                "Launched child parent changed",
            ));
        }
        // SAFETY: pinned direct child plus exact current identity; no numeric-PID signal.
        if unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.pidfd.as_raw_fd(),
                libc::SIGTERM,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(())
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.exited {
            return;
        }
        if let Err(error) = self.terminate() {
            eprintln!("{error}");
        }
        eprintln!("Preserving launch ownership until the exact child exits; no forced kill");
        loop {
            match self.child.wait() {
                Ok(_) => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => {
                    eprintln!("Owned child reap failed: {error}");
                    thread::sleep(Duration::from_secs(1));
                }
            }
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct WindowCloseTarget {
    #[serde(with = "CloseGameIdentity")]
    game: Identity,
    window: u32,
    window_class: Vec<u8>,
}
#[derive(Deserialize, Serialize)]
#[serde(remote = "Identity", deny_unknown_fields)]
struct CloseGameIdentity {
    pid: i32,
    start_ticks: String,
    exe: String,
    uid: u32,
}
impl WindowCloseTarget {
    fn validate(&self, expected: &Identity) -> Result<()> {
        if self.game != *expected
            || self.window == 0
            || !platform::x11_close::valid_class(&self.window_class)
        {
            return Err(Error::new(
                "invalid_window_binding",
                "Exact owned game/window/class required",
            ));
        }
        Ok(())
    }
}
enum CloseKind {
    Terminate,
    Window(WindowCloseTarget),
}
struct CloseRequest {
    kind: CloseKind,
    until: Instant,
    reply: SyncSender<Value>,
}
struct WindowCloseAttempt {
    target: WindowCloseTarget,
    until: Instant,
    quiesced: bool,
    started: bool,
    delivery: CloseDelivery,
    error: Option<Value>,
    pending: Vec<SyncSender<Value>>,
}
impl WindowCloseAttempt {
    fn response(&self, exit: &Option<Value>) -> Value {
        json!({"closed":exit.is_some(),"exit":exit,"window_close":{
            "target_window":self.target.window,"attempted":self.started,
            "delivery":self.delivery,"error":self.error,
            "deadline_expired":Instant::now()>=self.until,
            "timestamp_profile":"sdl_current_time","no_signal_fallback":true
        }})
    }
    fn complete(&mut self, dispatch: CloseDispatch) {
        self.started = true;
        self.delivery = dispatch.delivery;
        self.error = dispatch.error.map(|error| json!(error));
    }
    fn reply_ready(&self, exit: &Option<Value>) -> bool {
        exit.is_some() || self.error.is_some() || Instant::now() >= self.until
    }
    fn replies(&mut self, exit: &Option<Value>) {
        let reply = self.response(exit);
        for sender in self.pending.drain(..) {
            let _ = sender.send(reply.clone());
        }
    }
}

/// Quiescence, authentication and delivery share this attempt's one budget.
/// Never unwind the owned child or renew/replay an expired attempt on a client
/// reconnect. The gate is released before X11; its stored false value rejects
/// all later key pairs, without acquiring clock state or controller leases.
fn advance_window_close(
    attempt: &mut WindowCloseAttempt,
    input_gate: &RwLock<bool>,
    dispatch: impl FnOnce(&WindowCloseTarget, Instant) -> CloseDispatch,
) {
    if !attempt.quiesced {
        match input_gate.try_write() {
            Ok(mut gate) => {
                *gate = false;
                attempt.quiesced = true;
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                if !attempt.started && Instant::now() >= attempt.until {
                    attempt.complete(CloseDispatch::refused(Error::new(
                        "window_close_timeout",
                        "Close expired during quiescence; owned child preserved",
                    )));
                }
                return;
            }
            Err(std::sync::TryLockError::Poisoned(_)) => {
                if !attempt.started {
                    attempt.complete(CloseDispatch::refused(Error::new(
                        "input_unavailable",
                        "Input shutdown gate poisoned; owned child preserved",
                    )));
                }
                return;
            }
        }
    }
    // Even an expired/refused attempt keeps trying to disable later input after
    // an already held key-pair permit is released, without retrying delivery.
    if attempt.started {
        return;
    }
    if Instant::now() >= attempt.until {
        attempt.complete(CloseDispatch::refused(Error::new(
            "window_close_timeout",
            "Close expired before delivery; owned child preserved",
        )));
        return;
    }
    // A panic from the new display path must not leave this loop and invoke
    // OwnedChild's legacy SIGTERM Drop policy. Any uncertain dispatch stays
    // quarantined in the retained attempt instead of becoming a second send.
    attempt.started = true;
    let until = attempt.until.min(Instant::now() + Duration::from_secs(2));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        dispatch(&attempt.target, until)
    }))
    .unwrap_or_else(|_| CloseDispatch {
        delivery: CloseDelivery::Unknown,
        error: Some(Error::new(
            "window_close_unavailable",
            "Display close panicked; delivery unknown and owned child preserved",
        )),
    });
    attempt.complete(outcome);
}

fn dispatch_window_close(
    child: &OwnedChild,
    shared: &Shared,
    target: &WindowCloseTarget,
    until: Instant,
) -> CloseDispatch {
    let opened = (|| -> Result<X11WindowClose> {
        if Instant::now() >= until {
            return Err(Error::new(
                "window_close_timeout",
                "Close wire budget expired",
            ));
        }
        target.validate(&shared.endpoint.session.game)?;
        if child.identity.as_ref() != Some(&target.game)
            || child.child.id() as i32 != target.game.pid
            || child.exited_without_reaping()?
        {
            return Err(Error::new(
                "ownership_changed",
                "Pinned owned child is unavailable",
            ));
        }
        verify_identity(&target.game)?;
        let (_, parent, ticks) = platform::parse_stat(&fs::read_to_string(format!(
            "/proc/{}/stat",
            target.game.pid
        ))?)?;
        if parent != std::process::id() as i32 || ticks != target.game.start_ticks {
            return Err(Error::new(
                "ownership_changed",
                "Owned direct-child epoch changed",
            ));
        }
        let process = shared.server.process.as_ref().ok_or_else(|| {
            Error::new("window_close_unavailable", "Owned process reader required")
        })?;
        if process.identity() != Some(&target.game) {
            return Err(Error::new(
                "ownership_changed",
                "Close reader differs from owned child",
            ));
        }
        process.check()?;
        let display = shared.display.as_ref().ok_or_else(|| {
            Error::new(
                "window_close_unavailable",
                "Configured owned display required",
            )
        })?;
        X11WindowClose::open_until(
            process.as_ref(),
            display.number,
            &display.server,
            &display.xauthority,
            target.window,
            &target.window_class,
            until,
        )
    })();
    match opened {
        Ok(mut closer) => closer.request_once(shared.server.process.as_ref().unwrap().as_ref()),
        Err(error) => CloseDispatch::refused(error),
    }
}
struct Shared {
    server: Server,
    endpoint: Endpoint,
    reads: RwLock<()>,
    accepting_reads: AtomicBool,
    input_gate: Arc<RwLock<bool>>,
    alive: AtomicBool,
    closing: SyncSender<CloseRequest>,
    commands: RwLock<Option<Arc<dyn crate::protocol::Commands>>>,
    display: Option<Display>,
    directory: PathBuf,
}
fn service_client(stream: UnixStream, shared: Arc<Shared>) {
    let response = (|| -> Result<Value> {
        let client = peer(&stream)?;
        verify_identity(&client)?;
        let envelope = read_value(&stream, Duration::from_secs(2))?;
        if envelope.get("version") != Some(&json!(1))
            || envelope.get("nonce") != Some(&json!(shared.endpoint.nonce))
        {
            return Err(Error::new("invalid_endpoint", "Endpoint handshake differs"));
        }
        let request = &envelope["request"];
        match request.get("op").and_then(Value::as_str) {
            Some("mcp") => {
                let _read = shared
                    .reads
                    .read()
                    .map_err(|_| Error::new("ownership_changed", "Read gate poisoned"))?;
                if !shared.accepting_reads.load(Ordering::Acquire) {
                    return Err(Error::new(
                        "session_closed",
                        "Owned game is exiting or has exited",
                    ));
                }
                let commands = shared
                    .commands
                    .read()
                    .map_err(|_| Error::new("input_unavailable", "Input binding lock poisoned"))?
                    .clone();
                Ok(
                    json!({"reply":shared.server.handle_with_commands(request.get("message").cloned().unwrap_or(Value::Null), commands.as_deref())}),
                )
            }
            Some("bind_input") => {
                let _read = shared
                    .reads
                    .read()
                    .map_err(|_| Error::new("ownership_changed", "Read gate poisoned"))?;
                if !shared.accepting_reads.load(Ordering::Acquire) {
                    return Err(Error::new("session_closed", "Owned game is exiting"));
                }
                let mut commands = shared
                    .commands
                    .write()
                    .map_err(|_| Error::new("input_unavailable", "Input binding lock poisoned"))?;
                if commands.is_some() {
                    return Err(Error::new(
                        "input_already_bound",
                        "Never replace an active input binding",
                    ));
                }
                let process = shared.server.process.as_ref().ok_or_else(|| {
                    Error::new("input_unavailable", "Owned process reader required")
                })?;
                let display = shared.display.as_ref().ok_or_else(|| {
                    Error::new("input_unavailable", "Owned display binding required")
                })?;
                let binding = serde_json::from_value(request["binding"].clone())
                    .map_err(|e| Error::new("invalid_input_binding", e.to_string()))?;
                let service = clock::ClockService::bind(
                    process.as_ref(),
                    display,
                    binding,
                    &shared.directory,
                    shared.input_gate.clone(),
                )?;
                let capabilities = crate::protocol::Commands::capabilities(&service);
                *commands = Some(Arc::new(service));
                Ok(json!({"bound":true,"capabilities":capabilities}))
            }
            Some("close") | Some("close_window") => {
                let kind = if request.get("op") == Some(&json!("close_window")) {
                    let target: WindowCloseTarget =
                        serde_json::from_value(request["target"].clone()).map_err(|error| {
                            Error::new("invalid_window_binding", error.to_string())
                        })?;
                    target.validate(&shared.endpoint.session.game)?;
                    CloseKind::Window(target)
                } else {
                    CloseKind::Terminate
                };
                let (sender, receiver) = mpsc::sync_channel(1);
                shared
                    .closing
                    .try_send(CloseRequest {
                        kind,
                        until: Instant::now() + Duration::from_secs(20),
                        reply: sender,
                    })
                    .map_err(|_| Error::new("endpoint_busy", "Shutdown request queue is full"))?;
                receiver.recv_timeout(Duration::from_secs(22)).map_err(|_| {
                    Error::new("shutdown_pending", "Owner is still preserving the game")
                })
            }
            Some("status") => Ok(
                json!({"game":shared.endpoint.session.game,"owned_live":shared.alive.load(Ordering::Acquire),"campaign_acceptance":false}),
            ),
            _ => Err(Error::new(
                "invalid_request",
                "Unsupported endpoint operation",
            )),
        }
    })();
    let response = match response {
        Ok(value) => value,
        Err(error) => json!({"error":error}),
    };
    if let Err(error) = send_value(&stream, &response) {
        eprintln!("{error}");
    }
}
static SIGNAL_CLOSE: AtomicBool = AtomicBool::new(false);
extern "C" fn signal_close(_: libc::c_int) {
    SIGNAL_CLOSE.store(true, Ordering::Relaxed);
}
fn install_signals() -> Result<()> {
    // SAFETY: zeroed sigaction is initialized with a signal-safe atomic handler;
    // no Rust allocation, I/O or game operations occur inside the signal handler.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = signal_close as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        for signal in [libc::SIGTERM, libc::SIGINT] {
            if libc::sigaction(signal, &action, std::ptr::null_mut()) != 0 {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        action.sa_sigaction = libc::SIG_IGN;
        if libc::sigaction(libc::SIGHUP, &action, std::ptr::null_mut()) != 0 {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    Ok(())
}
fn launch_lock() -> Result<File> {
    let directory = PathBuf::from(format!("/tmp/hoi4-live-native-{}", uid()));
    match fs::DirBuilder::new().mode(0o700).create(&directory) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    private(&directory, true)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(directory.join("launch.lock"))?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.uid() != uid() || meta.mode() & 0o077 != 0 {
        return Err(Error::new(
            "invalid_endpoint",
            "Private launch lock required",
        ));
    }
    file.try_lock_exclusive().map_err(|_| {
        Error::new(
            "game_already_running",
            "Another native launch supervisor owns the launch lock",
        )
    })?;
    Ok(file)
}
fn nonce() -> Result<String> {
    let mut bytes = [0; 16];
    File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
/// Terminal evidence comes from the existing sole reaper, never from a signal
/// we requested, a shell status, or a previously saved exit record.
fn unix_wait_evidence(status: &std::process::ExitStatus) -> Value {
    json!({
        "exit_code":status.code(),
        "signal":status.signal(),
        "core_dumped":status.core_dumped(),
        "wait_status":status.to_string()
    })
}
fn run_owner(
    listener: UnixListener,
    socket_meta: fs::Metadata,
    directory: &Path,
    mut child: OwnedChild,
    shared: Arc<Shared>,
    closing: Receiver<CloseRequest>,
) -> Result<()> {
    let mut workers: Vec<thread::JoinHandle<()>> = Vec::new();
    let mut pending: Vec<SyncSender<Value>> = Vec::new();
    let mut deadline = None;
    let mut exit_evidence = None;
    let mut window_attempt: Option<WindowCloseAttempt> = None;

    loop {
        let mut index = 0;
        while index < workers.len() {
            if workers[index].is_finished() {
                if workers.swap_remove(index).join().is_err() {
                    eprintln!("Endpoint client handler panicked; owner remains alive");
                }
            } else {
                index += 1;
            }
        }
        while let Ok(request) = closing.try_recv() {
            match request.kind {
                CloseKind::Terminate => pending.push(request.reply),
                CloseKind::Window(target) => match window_attempt.as_mut() {
                    Some(attempt) if attempt.target != target => {
                        let _ = request.reply.send(json!({"error":Error::new(
                            "window_close_target_changed", "Never replace the retained close target"
                        )}));
                    }
                    Some(attempt) => attempt.pending.push(request.reply),
                    None => {
                        shared.accepting_reads.store(false, Ordering::Release);
                        window_attempt = Some(WindowCloseAttempt {
                            target,
                            until: request.until,
                            quiesced: false,
                            started: false,
                            delivery: CloseDelivery::NotQueued,
                            error: None,
                            pending: vec![request.reply],
                        });
                    }
                },
            }
        }
        if let Some(attempt) = window_attempt.as_mut() {
            advance_window_close(attempt, &shared.input_gate, |target, until| {
                dispatch_window_close(&child, &shared, target, until)
            });
        }
        if (!pending.is_empty() || SIGNAL_CLOSE.swap(false, Ordering::Relaxed))
            && deadline.is_none()
        {
            shared.accepting_reads.store(false, Ordering::Release);
            // Drain an already queued bounded key pair before signalling the
            // child; every later pair sees the disabled input gate.
            *shared
                .input_gate
                .write()
                .map_err(|_| Error::new("input_unavailable", "Input shutdown gate poisoned"))? =
                false;
            if let Err(error) = child.terminate() {
                // The exact child can exit between the pidfd poll and identity
                // verification. Keep pending replies through exit monitoring;
                // a failed signal never authorizes a numeric-PID fallback.
                eprintln!("Owned termination not sent: {error}");
            }
            deadline = Some(Instant::now() + Duration::from_secs(20));
        }
        if !child.exited {
            match child.exited_without_reaping() {
                Ok(true) => {
                    shared.accepting_reads.store(false, Ordering::Release);
                    *shared.input_gate.write().map_err(|_| {
                        Error::new("input_unavailable", "Input exit gate poisoned")
                    })? = false;
                    shared.alive.store(false, Ordering::Release);
                    // Drain every in-flight snapshot before reaping; PID reuse cannot
                    // authorize a later syscall by an observer worker.
                    if let Ok(_drained) = shared.reads.try_write() {
                        match child.child.try_wait() {
                            Ok(Some(status)) => {
                                let evidence = unix_wait_evidence(&status);
                                let mut record = evidence.clone();
                                record["game"] = json!(shared.endpoint.session.game);
                                record["reason"] = json!("owned child exited");
                                exit_evidence = Some(evidence);
                                child.exited = true;
                                if let Err(error) = json_new(&directory.join("exited.json"), record)
                                {
                                    eprintln!("{error}");
                                }
                            }
                            Ok(None) => {}
                            Err(error) => eprintln!("Owned child reap failed: {error}"),
                        }
                    }
                }
                Ok(false) => {}
                Err(error) => eprintln!("Owned child status failed: {error}"),
            }
        }
        if let Some(attempt) = window_attempt.as_mut() {
            if attempt.reply_ready(&exit_evidence) {
                attempt.replies(&exit_evidence);
            }
        }
        if child.exited && (deadline.is_some() || window_attempt.is_some()) {
            for reply in pending.drain(..) {
                let _ = reply.send(json!({"closed":true,"exit":exit_evidence}));
            }
            break;
        }
        if deadline.is_some_and(|value| Instant::now() >= value) {
            for reply in pending.drain(..) {
                let _ = reply.send(json!({"closed":false}));
            }
            // A failed termination remains diagnosable through this same owner.
            deadline = None;
            eprintln!("Owned shutdown timed out; preserving game, endpoint and userdata");
        }
        let mut poll = libc::pollfd {
            fd: listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: bounded listener poll also drives exit and shutdown monitoring.
        let n = unsafe { libc::poll(&mut poll, 1, 25) };
        if n < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::Interrupted {
                eprintln!("Endpoint poll failed: {error}");
                thread::sleep(Duration::from_millis(25));
            }
            continue;
        }
        if n == 0 {
            continue;
        }
        match listener.accept() {
            Ok((stream, _)) if workers.len() < 8 => {
                let shared = Arc::clone(&shared);
                match thread::Builder::new()
                    .name("owned-mcp-client".into())
                    .spawn(move || service_client(stream, shared))
                {
                    Ok(worker) => workers.push(worker),
                    Err(error) => {
                        // OS resource refusal affects this connection only.
                        // Unwinding would abandon active reads before the sole
                        // child reaper's gate, so worker creation must be fallible.
                        eprintln!("Endpoint worker unavailable; owner remains alive: {error}");
                    }
                }
            }
            Ok((stream, _)) => {
                drop(stream);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => {
                eprintln!("Endpoint accept failed: {error}");
                thread::sleep(Duration::from_millis(25));
            }
        }
    }
    for worker in workers {
        let _ = worker.join();
    }
    let actual = fs::symlink_metadata(directory.join("control.sock"))?;
    if actual.dev() == socket_meta.dev() && actual.ino() == socket_meta.ino() {
        fs::remove_file(directory.join("control.sock"))?;
    }
    Ok(())
}
pub fn serve(path: &Path) -> Result<()> {
    if uid() == 0 {
        return Err(Error::new(
            "permission_denied",
            "Do not run the native runtime as root",
        ));
    }
    private(path, false)?;
    let config: Config = json_read(path)?;
    if !config.run_directory.is_absolute() || !config.executable.is_absolute() {
        return Err(Error::new(
            "invalid_session",
            "Launch paths must be absolute",
        ));
    }
    // Refuse the complete endpoint path before build/display/lock checks or
    // creating the run directory, userdata, logs or the game process. Keep the
    // typed contract shared with reconnects instead of leaking SUN_LEN I/O.
    let socket = config.run_directory.join("control.sock");
    platform::validate_socket_path(&socket)?;
    let executable = config.executable.canonicalize()?;
    let executable_meta = fs::metadata(&executable)?;
    platform::verify_executable(&executable)?;
    let _launch_lock = launch_lock()?;
    duplicate_game(&executable)?;
    verify_identity(&config.display.server)?;
    if !config.display.xauthority.is_absolute() {
        return Err(Error::new(
            "invalid_session",
            "Display authentication path must be absolute",
        ));
    }
    private(&config.display.xauthority, false)?;
    let display = platform::connect_socket(Path::new(&format!(
        "/tmp/.X11-unix/X{}",
        config.display.number
    )))?;
    if peer(&display)? != config.display.server {
        return Err(Error::new(
            "ownership_changed",
            "Display socket peer differs from the recorded owned server",
        ));
    }
    verify_identity(&config.display.server)?;
    drop(display);
    if let Some(socket) = &config.gpu_transport {
        let m = fs::symlink_metadata(socket)?;
        if !m.file_type().is_socket() || m.uid() != uid() || m.mode() & 0o077 != 0 {
            return Err(Error::new(
                "invalid_session",
                "Private GPU transport required",
            ));
        }
    }
    if matches!(config.renderer, Renderer::Virpipe) && config.gpu_transport.is_none() {
        return Err(Error::new(
            "invalid_session",
            "Virpipe renderer requires an owned GPU transport",
        ));
    }
    let seed = if let Some(path) = &config.settings_seed {
        if !path.is_absolute() {
            return Err(Error::new(
                "invalid_session",
                "Settings seed must be absolute",
            ));
        }
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)?;
        if !file.metadata()?.is_file() {
            return Err(Error::new(
                "invalid_session",
                "Settings seed must be a regular file",
            ));
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take((1 << 20) + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 1 << 20 {
            return Err(Error::new("invalid_session", "Settings seed exceeds bound"));
        }
        Some(bytes)
    } else {
        None
    };
    let directory = &config.run_directory;
    let parent = directory
        .parent()
        .ok_or_else(|| Error::new("invalid_session", "Run directory has no parent"))?;
    private(parent, true)?;
    fs::DirBuilder::new().mode(0o700).create(directory)?;
    let userdata = directory.join("userdata");
    fs::DirBuilder::new().mode(0o700).create(&userdata)?;
    if let Some(seed) = &seed {
        write_new(&userdata.join("settings.txt"), seed)?;
    }
    write_new(
        &userdata.join("dlc_load.json"),
        b"{\"disabled_dlcs\":[],\"enabled_mods\":[]}",
    )?;
    // Copies and changes are confined to fresh isolated userdata. The seed and
    // user's default settings/playsets are never modified, including on exit.
    let log = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(directory.join("game.log"))?;
    let game_dir = executable.parent().unwrap();
    let mut command = Command::new(&executable);
    command
        .arg(format!("-userdir={}", userdata.display()))
        .current_dir(game_dir)
        .stdin(Stdio::null())
        .process_group(0)
        .stdout(log.try_clone()?)
        .stderr(log)
        .env_clear();
    for name in [
        "HOME",
        "USER",
        "LOGNAME",
        "PATH",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
        .env("DISPLAY", format!(":{}", config.display.number))
        .env("XAUTHORITY", &config.display.xauthority)
        .env("LD_LIBRARY_PATH", game_dir)
        .env("HOI4_PATH", game_dir)
        .env("SDL_AUDIODRIVER", "dummy");
    match config.renderer {
        Renderer::Default => {}
        Renderer::LlvmPipe => {
            command.env("GALLIUM_DRIVER", "llvmpipe");
        }
        Renderer::Virpipe => {
            command
                .env("GALLIUM_DRIVER", "virpipe")
                .env("VTEST_SOCKET_NAME", config.gpu_transport.as_ref().unwrap());
        }
    }
    let listener = UnixListener::bind(&socket)?;
    fs::set_permissions(
        directory.join("control.sock"),
        fs::Permissions::from_mode(0o600),
    )?;
    listener.set_nonblocking(true)?;
    // Capture before publishing the endpoint: a later replacement socket must
    // never become the saved cleanup identity.
    let socket_meta = fs::symlink_metadata(directory.join("control.sock"))?;
    install_signals()?;
    let mut child = OwnedChild::spawn(&mut command)?;
    let game = capture_identity(child.child.id() as i32)?;
    child.identity = Some(game.clone());
    let (_, actual_parent, _) =
        platform::parse_stat(&fs::read_to_string(format!("/proc/{}/stat", game.pid))?)?;
    let actual_exe = fs::metadata(format!("/proc/{}/exe", game.pid))?;
    if game.exe != executable.to_string_lossy()
        || game.uid != uid()
        || actual_parent != std::process::id() as i32
        || actual_exe.dev() != executable_meta.dev()
        || actual_exe.ino() != executable_meta.ino()
    {
        return Err(Error::new(
            "ownership_changed",
            "Launched executable, parent or owner differs",
        ));
    }
    let supervisor = capture_identity(std::process::id() as i32)?;
    let session = Session {
        game,
        supervisor: supervisor.clone(),
        build_id: BUILD_ID.into(),
        sha256: SHA256.into(),
    };
    json_new(&directory.join("session.json"), &session)?;
    let (process, unavailable) = match platform::open(&directory.join("session.json")) {
        Ok(process) => (Some(process), None),
        Err(error) => (None, Some(error)),
    };
    let endpoint = Endpoint {
        version: 1,
        nonce: nonce()?,
        server: supervisor,
        session,
    };
    json_new(&directory.join("endpoint.json"), &endpoint)?;
    let (closing, receiver) = mpsc::sync_channel(8);
    let shared = Arc::new(Shared {
        server: Server {
            process,
            unavailable,
        },
        endpoint,
        reads: RwLock::new(()),
        accepting_reads: AtomicBool::new(true),
        input_gate: Arc::new(RwLock::new(true)),
        alive: AtomicBool::new(true),
        closing,
        commands: RwLock::new(None),
        display: Some(config.display.clone()),
        directory: directory.to_path_buf(),
    });
    eprintln!("Owned Linux supervisor ready; optional input requires an explicit binding");
    run_owner(listener, socket_meta, directory, child, shared, receiver)
}

#[cfg(test)]
mod tests;
