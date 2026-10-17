//! Opt-in ordinary generated clients and an owned forwarding display. Never
//! connects an inherited user display or installs a production transport hook.
use super::*;
use std::{net::Shutdown, process::ChildStdin, sync::atomic::AtomicUsize};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{
            AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, InputFocus, PropMode,
            WindowClass,
        },
        Event,
    },
    wrapper::ConnectionExt as _,
};

const CLIENT: &str = "launch::linux::tests::window_close::generated_close_client";
const CLIENT_NONCE: &str = "HOI4_PRIVATE_CLOSE_CLIENT_NONCE";
const CLASS: &[u8] = b"owned-wm-reaper\0owned-wm-reaper\0";

fn display() -> Display {
    assert!(ordinary());
    let number: u16 = std::env::var("HOI4_NATIVE_X11_FIXTURE_DISPLAY")
        .unwrap()
        .parse()
        .unwrap();
    assert!(number >= 100, "Only explicit isolated fixture display");
    let server =
        serde_json::from_str(&std::env::var("HOI4_NATIVE_X11_FIXTURE_SERVER").unwrap()).unwrap();
    let value = Display {
        number,
        server,
        xauthority: std::env::var("HOI4_NATIVE_X11_FIXTURE_AUTHORITY")
            .unwrap()
            .into(),
    };
    verify_identity(&value.server).unwrap();
    assert!(value.xauthority.is_absolute());
    value
}

#[test]
#[ignore = "exact private nonce-selected reexec only"]
fn generated_close_client() {
    let Some(token) = selected_helper(CLIENT_NONCE, CLIENT) else {
        return;
    };
    assert_ne!(uid(), 0);
    let mode = std::env::var("HOI4_PRIVATE_CLOSE_CLIENT_MODE").unwrap();
    assert!(matches!(mode.as_str(), "exit" | "ignore"));
    // SAFETY: only this exact generated helper's self-watchdog changes. EOF
    // normally exits first; no parent/game/VM limit or signal is changed.
    unsafe {
        libc::alarm(30);
    }
    let (connection, screen) = x11rb::connect(None).unwrap();
    let root = connection.setup().roots[screen].root;
    let window = connection.generate_id().unwrap();
    connection
        .create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            root,
            20,
            20,
            120,
            90,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new().event_mask(
                EventMask::KEY_PRESS
                    | EventMask::KEY_RELEASE
                    | EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE,
            ),
        )
        .unwrap()
        .check()
        .unwrap();
    let atom = |name: &[u8]| {
        connection
            .intern_atom(false, name)
            .unwrap()
            .reply()
            .unwrap()
            .atom
    };
    let pid = atom(b"_NET_WM_PID");
    let protocols = atom(b"WM_PROTOCOLS");
    let delete = atom(b"WM_DELETE_WINDOW");
    connection
        .change_property32(
            PropMode::REPLACE,
            window,
            pid,
            AtomEnum::CARDINAL,
            &[std::process::id()],
        )
        .unwrap()
        .check()
        .unwrap();
    connection
        .change_property8(
            PropMode::REPLACE,
            window,
            AtomEnum::WM_CLASS,
            AtomEnum::STRING,
            CLASS,
        )
        .unwrap()
        .check()
        .unwrap();
    connection
        .change_property32(
            PropMode::REPLACE,
            window,
            protocols,
            AtomEnum::ATOM,
            &[delete],
        )
        .unwrap()
        .check()
        .unwrap();
    connection.map_window(window).unwrap().check().unwrap();
    let report = |kind: &str, events: &[Value], forbidden: u32| {
        println!(
            "close-client:{token}:{}",
            json!({"kind":kind,"pid":std::process::id(),
            "window":window,"protocols":protocols,"delete":delete,"events":events,"forbidden":forbidden})
        );
        std::io::stdout().flush().unwrap();
    };
    report("ready", &[], 0);
    // SAFETY: this helper's private inherited pipe only, never suite stdin.
    let flags = unsafe { libc::fcntl(0, libc::F_GETFL) };
    assert!(flags >= 0);
    assert_ne!(
        unsafe { libc::fcntl(0, libc::F_SETFL, flags | libc::O_NONBLOCK) },
        -1
    );
    let mut events = Vec::new();
    let mut forbidden = 0;
    loop {
        while let Some(event) = connection.poll_for_event().unwrap() {
            match event {
                Event::ClientMessage(event)=>events.push(json!({"format":event.format,"window":event.window,
                    "type":event.type_,"data":event.data.as_data32(),"synthetic":event.response_type&0x80!=0})),
                Event::KeyPress(_)|Event::KeyRelease(_)|Event::ButtonPress(_)|Event::ButtonRelease(_)=>forbidden+=1,
                _=>{},
            }
        }
        assert!(events.len() <= 4);
        if mode == "exit" && !events.is_empty() {
            report("events", &events, forbidden);
            std::process::exit(0);
        }
        let mut byte = [0];
        match std::io::stdin().read(&mut byte) {
            Ok(0) => std::process::exit(0),
            Ok(_) => {
                assert_eq!(byte[0], b'r');
                // Roundtrip before reporting preserves the real server/event
                // ordering even when command pipe scheduling wins the race.
                connection.get_input_focus().unwrap().reply().unwrap();
                while let Some(event) = connection.poll_for_event().unwrap() {
                    match event {
                        Event::ClientMessage(event) => events.push(json!({"format":event.format,
                            "window":event.window,"type":event.type_,"data":event.data.as_data32(),
                            "synthetic":event.response_type&0x80!=0})),
                        Event::KeyPress(_)
                        | Event::KeyRelease(_)
                        | Event::ButtonPress(_)
                        | Event::ButtonRelease(_) => forbidden += 1,
                        _ => {}
                    }
                }
                assert!(events.len() <= 4);
                report("events", &events, forbidden);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("Private child input failed: {error}"),
        }
        thread::sleep(Duration::from_millis(5));
    }
}

fn record(stream: &mut UnixStream, token: &str, kind: &str) -> Result<Value> {
    let until = Instant::now() + Duration::from_secs(5);
    let marker = format!("close-client:{token}:");
    let mut line = Vec::new();
    for _ in 0..8192 {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(Error::new("fixture_timeout", "Child report deadline"));
        }
        stream.set_read_timeout(Some(left))?;
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        line.push(byte[0]);
        if byte[0] == b'\n' {
            if let Some(at) = line
                .windows(marker.len())
                .position(|value| value == marker.as_bytes())
            {
                let value: Value = serde_json::from_slice(&line[at + marker.len()..])
                    .map_err(|error| Error::new("fixture_failed", error.to_string()))?;
                if value["kind"] != kind {
                    return Err(Error::new("fixture_failed", "Unexpected report"));
                }
                return Ok(value);
            }
            line.clear();
        }
    }
    Err(Error::new("fixture_failed", "Child report byte bound"))
}

struct WindowOwner {
    fixture: Fixture,
    input: Option<ChildStdin>,
    output: UnixStream,
    token: String,
    ready: Value,
    target: WindowCloseTarget,
    checks: Arc<AtomicUsize>,
}
impl WindowOwner {
    fn start(actual: &Display, owned: &Display, mode: &str) -> Result<Self> {
        let token = nonce()?;
        let (mut output, writer) = UnixStream::pair()?;
        let mut command = Command::new(std::env::current_exe()?);
        command
            .args([
                "--ignored",
                "--exact",
                CLIENT,
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CLIENT_NONCE, &token)
            .env("HOI4_PRIVATE_CLOSE_CLIENT_MODE", mode)
            .env("DISPLAY", format!(":{}", actual.number))
            .env("XAUTHORITY", &actual.xauthority)
            .stdin(Stdio::piped())
            .stdout(Stdio::from(std::os::fd::OwnedFd::from(writer)))
            .stderr(Stdio::inherit());
        let mut startup = WaitEvidenceStartup(Some(OwnedChild::spawn(&mut command)?));
        // This fallible bounded handshake retains the EOF-before-Drop startup
        // guard. No assert/unbounded read can strand a child with stdin open.
        let ready = record(&mut output, &token, "ready")?;
        let child = startup
            .0
            .as_mut()
            .ok_or_else(|| Error::new("fixture_failed", "Startup child missing"))?;
        if ready["pid"] != child.child.id() {
            return Err(Error::new("fixture_failed", "Readiness PID mismatch"));
        }
        let identity = child
            .identity
            .clone()
            .ok_or_else(|| Error::new("fixture_failed", "Identity missing"))?;
        let window = ready["window"]
            .as_u64()
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| Error::new("fixture_failed", "Bounded window missing"))?;
        let input = child.child.stdin.take();
        let checks = Arc::new(AtomicUsize::new(0));
        let target = WindowCloseTarget {
            game: identity.clone(),
            window,
            window_class: CLASS.to_vec(),
        };
        let fixture = Fixture::with_display(
            startup.into_child()?,
            Server {
                process: Some(Box::new(WindowFixtureProcess {
                    identity,
                    checks: checks.clone(),
                })),
                unavailable: None,
            },
            Some(owned.clone()),
        );
        Ok(Self {
            fixture,
            input,
            output,
            token,
            ready,
            target,
            checks,
        })
    }
    fn events(&mut self) -> Value {
        self.input.as_mut().unwrap().write_all(b"r").unwrap();
        record(&mut self.output, &self.token, "events").unwrap()
    }
    fn assert_event(&self, observed: &Value) {
        assert_eq!(observed["forbidden"], 0);
        assert_eq!(
            observed["events"],
            json!([{"format":32,"window":self.target.window,
            "type":self.ready["protocols"],"data":[self.ready["delete"],0,0,0,0],"synthetic":true}])
        );
    }
    fn finish(&mut self) -> Result<Value> {
        drop(self.input.take());
        let until = Instant::now() + Duration::from_secs(35);
        while self.fixture.shared.alive.load(Ordering::Acquire)
            && !self.fixture.owner.as_ref().unwrap().is_finished()
        {
            if Instant::now() >= until {
                return Err(Error::new(
                    "fixture_timeout",
                    "EOF/watchdog child did not reap",
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
        // If no WM intent existed (startup/test unwind), the legacy close only
        // ends an owner whose pinned pidfd has ALREADY observed child exit.
        // No live child is signalled as cleanup, including an assertion unwind.
        if !self.fixture.owner.as_ref().unwrap().is_finished() {
            // Do not open a client socket while an exiting owner is joining
            // workers. The private queue is safe only AFTER terminal pidfd evidence;
            // a dropped receiver simply means that owner is already ending.
            let (reply, _response) = mpsc::sync_channel(1);
            let _ = self.fixture.shared.closing.try_send(CloseRequest {
                kind: CloseKind::Terminate,
                until,
                reply,
            });
        }
        while !self.fixture.owner.as_ref().unwrap().is_finished() {
            if Instant::now() >= until {
                return Err(Error::new(
                    "fixture_timeout",
                    "Exited-child owner did not finish",
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
        let owner = self
            .fixture
            .owner
            .take()
            .ok_or_else(|| Error::new("fixture_failed", "Owner missing"))?;
        owner
            .join()
            .map_err(|_| Error::new("fixture_failed", "Owner panicked"))??;
        json_read(&self.fixture.directory.join("exited.json"))
    }
}
impl Drop for WindowOwner {
    fn drop(&mut self) {
        drop(self.input.take());
        if self.fixture.owner.is_some() {
            if let Err(error) = self.finish() {
                eprintln!("Private window fixture cleanup: {error}");
            }
        }
    }
}

#[derive(Clone, Copy)]
enum Fault {
    Sever,
    Unresponsive,
}
struct ForwardDisplay {
    display: Display,
    socket: PathBuf,
    socket_meta: fs::Metadata,
    directory: PathBuf,
    stop: Arc<AtomicBool>,
    connections: Arc<AtomicUsize>,
    packets: Arc<Mutex<Vec<Vec<u8>>>>,
    withheld_bytes: Arc<AtomicUsize>,
    listener: Option<thread::JoinHandle<Result<()>>>,
}
fn io_until(
    stream: &mut UnixStream,
    mut bytes: &mut [u8],
    stop: &AtomicBool,
    until: Instant,
) -> Result<()> {
    while !bytes.is_empty() {
        if stop.load(Ordering::Acquire) || Instant::now() >= until {
            return Err(Error::new(
                "fixture_timeout",
                "Forwarder absolute read deadline",
            ));
        }
        stream.set_read_timeout(Some(Duration::from_millis(50)))?;
        match stream.read(bytes) {
            Ok(0) => return Err(Error::new("fixture_closed", "Forwarded connection EOF")),
            Ok(count) => bytes = &mut bytes[count..],
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
fn word(bytes: [u8; 2], little: bool) -> usize {
    usize::from(if little {
        u16::from_le_bytes(bytes)
    } else {
        u16::from_be_bytes(bytes)
    })
}
fn forward_connection(
    mut client: UnixStream,
    source: &Display,
    fault: Fault,
    stop: Arc<AtomicBool>,
    packets: Arc<Mutex<Vec<Vec<u8>>>>,
    withheld_bytes: Arc<AtomicUsize>,
) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(10);
    let mut upstream =
        platform::connect_socket(Path::new(&format!("/tmp/.X11-unix/X{}", source.number)))?;
    if peer(&upstream)? != source.server {
        return Err(Error::new("ownership_changed", "Wrong owned Xvfb upstream"));
    }
    verify_identity(&source.server)?;
    upstream.set_write_timeout(Some(Duration::from_secs(2)))?;
    client.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut hello = [0; 12];
    io_until(&mut client, &mut hello, &stop, until)?;
    let little = match hello[0] {
        b'l' => true,
        b'B' => false,
        _ => return Err(Error::new("fixture_failed", "X byte order")),
    };
    let name = word([hello[6], hello[7]], little);
    let data = word([hello[8], hello[9]], little);
    if name > 256 || data > 256 {
        return Err(Error::new("fixture_failed", "Bounded X authorization"));
    }
    let mut auth = vec![0; ((name + 3) & !3) + ((data + 3) & !3)];
    io_until(&mut client, &mut auth, &stop, until)?;
    upstream.write_all(&hello)?;
    upstream.write_all(&auth)?;
    let mut setup = [0; 8];
    io_until(&mut upstream, &mut setup, &stop, until)?;
    let length = word([setup[6], setup[7]], little) * 4;
    if setup[0] != 1 || length > 128 << 10 {
        return Err(Error::new(
            "fixture_failed",
            "Owned Xvfb setup refused/bounded",
        ));
    }
    let mut payload = vec![0; length];
    io_until(&mut upstream, &mut payload, &stop, until)?;
    client.write_all(&setup)?;
    client.write_all(&payload)?;
    let muted = Arc::new(AtomicBool::new(false));
    let mut reverse_upstream = upstream.try_clone()?;
    let mut reverse_client = client.try_clone()?;
    let reverse_stop = stop.clone();
    let reverse_muted = muted.clone();
    let reverse = thread::Builder::new()
        .name("owned-x11-fixture-replies".into())
        .spawn(move || -> Result<()> {
            let mut bytes = [0; 4096];
            reverse_upstream.set_read_timeout(Some(Duration::from_millis(50)))?;
            while !reverse_stop.load(Ordering::Acquire) && Instant::now() < until {
                match reverse_upstream.read(&mut bytes) {
                    Ok(0) => return Ok(()),
                    Ok(count) if !reverse_muted.load(Ordering::Acquire) => {
                        reverse_client.write_all(&bytes[..count])?
                    }
                    Ok(count) => {
                        withheld_bytes.fetch_add(count, Ordering::AcqRel);
                    } // Actual replies withheld.
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::TimedOut
                                | std::io::ErrorKind::WouldBlock
                                | std::io::ErrorKind::Interrupted
                        ) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Ok(())
        })?;
    let result = (|| -> Result<()> {
        loop {
            let mut header = [0; 4];
            io_until(&mut client, &mut header, &stop, until)?;
            let words = word([header[2], header[3]], little);
            if words == 0 || words > 16384 {
                return Err(Error::new(
                    "fixture_failed",
                    "Bounded normal X request required",
                ));
            }
            let mut request = vec![0; words * 4];
            request[..4].copy_from_slice(&header);
            io_until(&mut client, &mut request[4..], &stop, until)?;
            let close = header[0] == 25; // Core SendEvent; capture actual request.
            if close {
                if request.len() != 44 {
                    return Err(Error::new("fixture_failed", "SendEvent width"));
                }
                muted.store(true, Ordering::Release);
            }
            upstream.write_all(&request)?;
            if close {
                let mut captured = packets
                    .lock()
                    .map_err(|_| Error::new("fixture_failed", "Capture poisoned"))?;
                captured.push(request);
                if captured.len() > 2 {
                    return Err(Error::new("fixture_failed", "Unexpected close replay"));
                }
                drop(captured);
                if matches!(fault, Fault::Sever) {
                    client.shutdown(Shutdown::Both)?; // Socket only, no process signal.
                    return Ok(());
                }
            }
        }
    })();
    let _ = upstream.shutdown(Shutdown::Both);
    let _ = client.shutdown(Shutdown::Both);
    let reverse_result = reverse
        .join()
        .map_err(|_| Error::new("fixture_failed", "Reply relay panicked"))?;
    if let Err(error) = reverse_result {
        if !stop.load(Ordering::Acquire) && !matches!(fault, Fault::Sever) {
            return Err(error);
        }
    }
    result
}
impl ForwardDisplay {
    fn start(source: &Display, fault: Fault) -> Result<Self> {
        let directory = std::env::temp_dir().join(format!("hoi4-wm-wire-{}", nonce()?));
        fs::DirBuilder::new().mode(0o700).create(&directory)?;
        let mut bound = None;
        for number in 150..200_u16 {
            let socket = PathBuf::from(format!("/tmp/.X11-unix/X{number}"));
            if fs::symlink_metadata(&socket).is_ok()
                || Path::new(&format!("/tmp/.X{number}-lock")).exists()
            {
                continue;
            }
            match UnixListener::bind(&socket) {
                Ok(listener) => {
                    bound = Some((number, socket, listener));
                    break;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {}
                Err(error) => return Err(error.into()),
            }
        }
        let (number, socket, listener) =
            bound.ok_or_else(|| Error::new("fixture_failed", "No unused forwarding display"))?;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
        let socket_meta = fs::symlink_metadata(&socket)?;
        listener.set_nonblocking(true)?;
        let authority = directory.join("authority");
        // Establish cleanup ownership before cookie copying or thread creation
        // can fail. Only the socket inode this fixture bound can be removed.
        let display = Display {
            number,
            server: capture_identity(std::process::id() as i32)?,
            xauthority: authority.clone(),
        };
        let stop = Arc::new(AtomicBool::new(false));
        let connections = Arc::new(AtomicUsize::new(0));
        let packets = Arc::new(Mutex::new(Vec::new()));
        let withheld_bytes = Arc::new(AtomicUsize::new(0));
        let mut owned = Self {
            display,
            socket,
            socket_meta,
            directory,
            stop,
            connections,
            packets,
            withheld_bytes,
            listener: None,
        };
        let (hostname, cookie) =
            platform::x11::fixture_local_cookie(&source.xauthority, source.number)?;
        let mut bytes = 256_u16.to_be_bytes().to_vec();
        for part in [
            &hostname[..],
            number.to_string().as_bytes(),
            b"MIT-MAGIC-COOKIE-1",
            &cookie,
        ] {
            bytes.extend_from_slice(&(part.len() as u16).to_be_bytes());
            bytes.extend_from_slice(part);
        }
        write_new(&authority, &bytes)?;
        let relay_stop = owned.stop.clone();
        let relay_connections = owned.connections.clone();
        let relay_packets = owned.packets.clone();
        let relay_withheld = owned.withheld_bytes.clone();
        let source = source.clone();
        let relay = thread::Builder::new()
            .name("owned-x11-fixture-forwarder".into())
            .spawn(move || -> Result<()> {
                let until = Instant::now() + Duration::from_secs(12);
                let mut connection = None;
                while !relay_stop.load(Ordering::Acquire) && Instant::now() < until {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let count = relay_connections.fetch_add(1, Ordering::AcqRel);
                            if count == 0 {
                                let source = source.clone();
                                let stopped = relay_stop.clone();
                                let capture = relay_packets.clone();
                                let withheld = relay_withheld.clone();
                                connection = Some(
                                    thread::Builder::new()
                                        .name("owned-x11-fixture-wire".into())
                                        .spawn(move || {
                                            forward_connection(
                                                stream, &source, fault, stopped, capture, withheld,
                                            )
                                        })?,
                                );
                            } else {
                                drop(stream);
                            } // Count/reject any accidental reconnect.
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(error) => return Err(error.into()),
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                relay_stop.store(true, Ordering::Release);
                if let Some(worker) = connection {
                    if let Err(error) = worker
                        .join()
                        .map_err(|_| Error::new("fixture_failed", "Wire relay panicked"))?
                    {
                        if !matches!(error.code, "fixture_closed" | "fixture_timeout") {
                            return Err(error);
                        }
                    }
                }
                Ok(())
            })?;
        owned.listener = Some(relay);
        Ok(owned)
    }
}
impl Drop for ForwardDisplay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.listener.take() {
            match worker.join() {
                Ok(Ok(())) => {}
                other => eprintln!("Private forwarding display cleanup: {other:?}"),
            }
        }
        if let Ok(current) = fs::symlink_metadata(&self.socket) {
            if current.dev() == self.socket_meta.dev() && current.ino() == self.socket_meta.ino() {
                let _ = fs::remove_file(&self.socket);
            }
        }
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[test]
#[ignore = "wrapper --owner-close-only; generated process plus real isolated Xvfb"]
fn owned_xvfb_window_close_reaches_sole_reaper_normal_exit() {
    let actual = display();
    let (connection, screen) = x11rb::connect(Some(&format!(":{}", actual.number))).unwrap();
    assert!(
        !connection
            .query_extension(b"XTEST")
            .unwrap()
            .reply()
            .unwrap()
            .present
    );
    let root = connection.setup().roots[screen].root;
    let other = connection.generate_id().unwrap();
    connection
        .create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            other,
            root,
            1,
            1,
            40,
            40,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new().event_mask(
                EventMask::KEY_PRESS
                    | EventMask::KEY_RELEASE
                    | EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE,
            ),
        )
        .unwrap()
        .check()
        .unwrap();
    connection.map_window(other).unwrap().check().unwrap();
    connection
        .set_input_focus(InputFocus::PARENT, other, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    let pointer = connection.query_pointer(root).unwrap().reply().unwrap();
    let mut child = WindowOwner::start(&actual, &actual, "exit").unwrap();
    let shared = child.fixture.shared.clone();
    let reading = shared.reads.read().unwrap();
    let path = child.fixture.directory.clone();
    let target = child.target.clone();
    let result =
        thread::spawn(move || request(&path, json!({"op":"close_window","target":target})));
    let observed = record(&mut child.output, &child.token, "events").unwrap();
    child.assert_event(&observed);
    let until = Instant::now() + Duration::from_secs(5);
    while shared.alive.load(Ordering::Acquire) {
        assert!(
            Instant::now() < until,
            "Pinned child exit was not monitored"
        );
        thread::sleep(Duration::from_millis(5));
    }
    assert!(
        !result.is_finished(),
        "Sole reaper must wait for the existing read permit"
    );
    assert!(!child.fixture.directory.join("exited.json").exists());
    let (_, parent, ticks) = platform::parse_stat(
        &fs::read_to_string(format!("/proc/{}/stat", child.target.game.pid)).unwrap(),
    )
    .unwrap();
    assert_eq!(parent, std::process::id() as i32);
    assert_eq!(
        ticks, child.target.game.start_ticks,
        "Exited child remained unreaped/pinned"
    );
    drop(reading);
    let response = result.join().unwrap().unwrap();
    assert_eq!(response["closed"], true);
    assert_eq!(response["window_close"]["delivery"], "server_confirmed");
    assert_eq!(response["exit"]["exit_code"], 0);
    assert_eq!(response["exit"]["signal"], Value::Null);
    assert_eq!(response["exit"]["core_dumped"], false);
    assert!(response["exit"]["wait_status"]
        .as_str()
        .unwrap()
        .contains("0"));
    let evidence = child.finish().unwrap();
    for field in ["exit_code", "signal", "core_dumped", "wait_status"] {
        assert_eq!(evidence[field], response["exit"][field]);
    }
    assert!(!child.fixture.directory.join("control.sock").exists());
    assert!(capture_identity(child.target.game.pid).is_err());
    assert_eq!(
        connection.get_input_focus().unwrap().reply().unwrap().focus,
        other
    );
    let after = connection.query_pointer(root).unwrap().reply().unwrap();
    assert_eq!(
        (pointer.root_x, pointer.root_y),
        (after.root_x, after.root_y)
    );
    while let Some(event) = connection.poll_for_event().unwrap() {
        assert!(!matches!(
            event,
            Event::ClientMessage(_)
                | Event::KeyPress(_)
                | Event::KeyRelease(_)
                | Event::ButtonPress(_)
                | Event::ButtonRelease(_)
        ));
    }
    connection.destroy_window(other).unwrap().check().unwrap();
}

fn ambiguous_wire(fault: Fault) {
    let actual = display();
    let proxy = ForwardDisplay::start(&actual, fault).unwrap();
    let mut child = WindowOwner::start(&actual, &proxy.display, "ignore").unwrap();
    let start = Instant::now();
    let first = request(
        &child.fixture.directory,
        json!({"op":"close_window","target":child.target}),
    )
    .unwrap();
    let elapsed = start.elapsed();
    assert_eq!(first["closed"], false);
    assert_eq!(first["window_close"]["delivery"], "unknown");
    assert_eq!(
        first["window_close"]["error"]["code"],
        "window_close_unavailable"
    );
    assert!(
        elapsed < Duration::from_secs(6),
        "Wire budget is two seconds inside the 20-second close budget: {elapsed:?}"
    );
    if matches!(fault, Fault::Unresponsive) {
        assert!(
            first["window_close"]["error"]["message"]
                .as_str()
                .unwrap()
                .contains("deadline"),
            "Unresponsive real wire must expire rather than get an injected failure: {first}"
        );
        assert!(
            proxy.withheld_bytes.load(Ordering::Acquire) > 0,
            "The real Xvfb actually replied; the proxy suppressed acknowledgement bytes"
        );
    }
    let observed = child.events();
    child.assert_event(&observed);
    assert_eq!(
        capture_identity(child.target.game.pid).unwrap(),
        child.target.game
    );
    assert_eq!(child.fixture.status()["owned_live"], true);
    assert!(!child.fixture.owner.as_ref().unwrap().is_finished());
    assert!(
        !child.fixture.directory.join("exited.json").exists(),
        "No signal fallback or reap after ambiguous wire"
    );
    assert!(!*child.fixture.shared.input_gate.read().unwrap());
    let checks = child.checks.load(Ordering::Acquire);
    for _ in 0..2 {
        let repeated = request(
            &child.fixture.directory,
            json!({"op":"close_window","target":child.target}),
        )
        .unwrap();
        assert_eq!(repeated, first);
    }
    assert_eq!(child.checks.load(Ordering::Acquire), checks);
    assert_eq!(
        proxy.connections.load(Ordering::Acquire),
        1,
        "No second X11 connect after lost ACK"
    );
    let packets = proxy.packets.lock().unwrap().clone();
    assert_eq!(
        packets.len(),
        1,
        "Actual SendEvent request was not replayed"
    );
    assert_eq!(packets[0].len(), 44);
    assert_eq!(packets[0][0], 25);
    assert_eq!(packets[0][1], 0);
    assert_eq!(&packets[0][4..8], &child.target.window.to_ne_bytes());
    assert_eq!(&packets[0][8..12], &0_u32.to_ne_bytes());
    assert_eq!(child.events()["events"].as_array().unwrap().len(), 1);
    let evidence = child.finish().unwrap(); // EOF only; no live legacy signal.
    assert_eq!(evidence["exit_code"], 0);
    assert_eq!(evidence["signal"], Value::Null);
    assert_eq!(evidence["core_dumped"], false);
}

#[test]
#[ignore = "wrapper --owner-close-only; real SendEvent then socket severance"]
fn owned_xvfb_window_close_severed_wire_is_unknown_without_replay() {
    ambiguous_wire(Fault::Sever);
}
#[test]
#[ignore = "wrapper --owner-close-only; real server replies withheld until absolute deadline"]
fn owned_xvfb_window_close_unresponsive_wire_expires_without_replay() {
    ambiguous_wire(Fault::Unresponsive);
}
