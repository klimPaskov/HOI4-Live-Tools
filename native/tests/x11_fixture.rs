//! Opt-in transport test on a separately owned Xvfb server. This is not HOI4
//! live acceptance, and never opens the user's desktop or game display.
#![cfg(target_os = "linux")]
use hoi4_live_native::{
    error::{Error, Result},
    memory::ReadMemory,
    platform::{
        capture_identity,
        x11::{ControlKey, X11Controls},
        x11_close::{CloseDelivery, X11WindowClose},
        Identity, Process,
    },
};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::{
        fd::OwnedFd,
        unix::{
            fs::{symlink, OpenOptionsExt},
            net::UnixStream,
        },
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::{
        xproto::{
            AtomEnum, ConnectionExt as _, CreateWindowAux, EventMask, InputFocus, PropMode,
            WindowClass,
        },
        xtest::ConnectionExt as _,
        Event,
    },
    wrapper::ConnectionExt as _,
};

#[path = "x11_fixture/pointer.rs"]
mod pointer;

struct FixtureProcess(Identity);
impl Process for FixtureProcess {
    fn check(&self) -> Result<()> {
        if capture_identity(self.0.pid)? != self.0 {
            return Err(Error::new("ownership_changed", "Fixture process replaced"));
        }
        Ok(())
    }
    fn image_base(&self) -> u64 {
        0
    }
    fn identity(&self) -> Option<&Identity> {
        Some(&self.0)
    }
}

const WM_HELPER: &str = "generated_wm_client";
const WM_HELPER_ENV: &str = "HOI4_NATIVE_GENERATED_WM_CLIENT";
const WM_CLASS: &[u8] = b"generated-wm\0generated-wm\0";

/// The helper receives no game payload or eligibility test hook. Its ordinary
/// process owns the generated X window and reports the actual wire event.
#[test]
#[ignore = "private exact reexec helper; inert without nonce"]
fn generated_wm_client() {
    let Ok(payload) = std::env::var(WM_HELPER_ENV) else {
        return;
    };
    assert!(std::env::args().any(|arg| arg == WM_HELPER));
    let (nonce, mode) = payload.split_once(':').unwrap();
    assert_eq!(nonce.len(), 32);
    assert!(nonce.bytes().all(|byte| byte.is_ascii_hexdigit()));
    assert!(matches!(mode, "ignore" | "exit" | "pointer"));
    assert_ne!(unsafe { libc::getuid() }, 0);
    // SAFETY: a watchdog confined to this exact generated process; no parent,
    // VM setting or game signal is changed. EOF normally exits it well earlier.
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
            160,
            120,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new().event_mask(
                EventMask::KEY_PRESS
                    | EventMask::KEY_RELEASE
                    | EventMask::BUTTON_PRESS
                    | EventMask::BUTTON_RELEASE
                    | if mode == "pointer" {
                        EventMask::POINTER_MOTION
                    } else {
                        EventMask::NO_EVENT
                    },
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
    let pid_atom = atom(b"_NET_WM_PID");
    let protocols = atom(b"WM_PROTOCOLS");
    let delete = atom(b"WM_DELETE_WINDOW");
    connection
        .change_property32(
            PropMode::REPLACE,
            window,
            pid_atom,
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
            WM_CLASS,
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
    let print = |value: serde_json::Value| {
        println!("wm-client:{nonce}:{value}");
        std::io::stdout().flush().unwrap();
    };
    print(
        serde_json::json!({"kind":"ready","window":window,"pid":std::process::id(),"protocols":protocols,"delete":delete}),
    );
    // SAFETY: only this helper's private inherited stdin is made nonblocking.
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
            if mode == "pointer" {
                if let Some(event) = pointer::pointer_event(&event) {
                    events.push(event);
                    continue;
                }
            }
            match event {
                Event::ClientMessage(event) => {
                    events.push(serde_json::json!({"format":event.format,"window":event.window,
                        "type":event.type_,"data":event.data.as_data32(),"synthetic":event.response_type&0x80!=0}));
                    assert!(events.len() <= 4, "Close requests must not replay");
                    if mode == "exit" {
                        print(
                            serde_json::json!({"kind":"events","events":events,"forbidden":forbidden}),
                        );
                        std::process::exit(0);
                    }
                }
                Event::KeyPress(_)
                | Event::KeyRelease(_)
                | Event::ButtonPress(_)
                | Event::ButtonRelease(_) => forbidden += 1,
                _ => {}
            }
        }
        let mut command = [0];
        match std::io::stdin().read(&mut command) {
            Ok(0) => std::process::exit(0),
            Ok(_) => {
                assert_eq!(command[0], b'r');
                // A server roundtrip places already delivered client messages
                // into this connection before the following drain/report.
                connection.get_input_focus().unwrap().reply().unwrap();
                while let Some(event) = connection.poll_for_event().unwrap() {
                    if mode == "pointer" {
                        if let Some(event) = pointer::pointer_event(&event) {
                            events.push(event);
                            continue;
                        }
                    }
                    match event {
                        Event::ClientMessage(event) => {
                            events.push(serde_json::json!({"format":event.format,
                            "window":event.window,"type":event.type_,"data":event.data.as_data32(),
                            "synthetic":event.response_type&0x80!=0}))
                        }
                        Event::KeyPress(_)
                        | Event::KeyRelease(_)
                        | Event::ButtonPress(_)
                        | Event::ButtonRelease(_) => forbidden += 1,
                        _ => {}
                    }
                }
                assert!(events.len() <= 4);
                print(serde_json::json!({"kind":"events","events":events,"forbidden":forbidden}));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(error) => panic!("Private helper pipe failed: {error}"),
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

struct WmChild {
    child: Child,
    output: UnixStream,
    nonce: String,
    ready: serde_json::Value,
    identity: Identity,
}
impl WmChild {
    fn start(mode: &str) -> Result<Self> {
        let mut random = [0; 16];
        fs::File::open("/dev/urandom")?.read_exact(&mut random)?;
        let nonce: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let (output, writer) = UnixStream::pair()?;
        let child = Command::new(std::env::current_exe()?)
            .args([
                "--ignored",
                "--exact",
                WM_HELPER,
                "--nocapture",
                "--test-threads=1",
            ])
            .env(WM_HELPER_ENV, format!("{nonce}:{mode}"))
            .stdin(Stdio::piped())
            .stdout(Stdio::from(OwnedFd::from(writer)))
            .stderr(Stdio::inherit())
            .spawn()?;
        // This guard exists before any fallible startup read; its Drop releases
        // private EOF before waiting, including read errors and test unwind.
        let mut guard = Self {
            child,
            output,
            nonce,
            ready: serde_json::Value::Null,
            identity: Identity {
                pid: 0,
                uid: 0,
                start_ticks: String::new(),
                exe: String::new(),
            },
        };
        guard.ready = guard.record("ready")?;
        guard.identity = capture_identity(guard.child.id() as i32)?;
        if guard.ready["pid"] != guard.child.id() {
            return Err(Error::new(
                "fixture_failed",
                "Wrong generated child readiness PID",
            ));
        }
        Ok(guard)
    }
    fn record(&mut self, kind: &str) -> Result<serde_json::Value> {
        let until = Instant::now() + Duration::from_secs(5);
        let marker = format!("wm-client:{}:", self.nonce);
        let mut line = Vec::new();
        for _ in 0..8192 {
            let remaining = until.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Error::new(
                    "fixture_timeout",
                    "Bounded child report expired",
                ));
            }
            self.output.set_read_timeout(Some(remaining))?;
            let mut byte = [0];
            self.output.read_exact(&mut byte)?;
            line.push(byte[0]);
            if byte[0] == b'\n' {
                if let Some(at) = line
                    .windows(marker.len())
                    .position(|bytes| bytes == marker.as_bytes())
                {
                    let value: serde_json::Value =
                        serde_json::from_slice(&line[at + marker.len()..])
                            .map_err(|error| Error::new("fixture_failed", error.to_string()))?;
                    if value["kind"] != kind {
                        return Err(Error::new("fixture_failed", "Unexpected child report kind"));
                    }
                    return Ok(value);
                }
                line.clear();
            }
        }
        Err(Error::new(
            "fixture_failed",
            "Bounded child output exceeded",
        ))
    }
    fn events(&mut self) -> serde_json::Value {
        self.child.stdin.as_mut().unwrap().write_all(b"r").unwrap();
        self.record("events").unwrap()
    }
    fn finish(&mut self) -> std::process::ExitStatus {
        drop(self.child.stdin.take());
        self.child.wait().unwrap()
    }
}
impl Drop for WmChild {
    fn drop(&mut self) {
        drop(self.child.stdin.take());
        // The helper's self-watchdog bounds even unexpected X11 startup hangs.
        let _ = self.child.wait();
    }
}
struct RefusableProcess {
    identity: Identity,
    refused: AtomicBool,
}
impl ReadMemory for RefusableProcess {
    fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
        Err(Error::new(
            "fixture_only",
            "No game payload in generated close fixture",
        ))
    }
}
impl Process for RefusableProcess {
    fn image_base(&self) -> u64 {
        0
    }
    fn identity(&self) -> Option<&Identity> {
        Some(&self.identity)
    }
    fn check(&self) -> Result<()> {
        if self.refused.load(Ordering::Relaxed) {
            return Err(Error::new(
                "ownership_changed",
                "Generated process witness changed",
            ));
        }
        FixtureProcess(self.identity.clone()).check()
    }
}

#[test]
#[ignore = "requires separate authenticated Xvfb; wrapper --close-only disables XTEST"]
fn owned_xvfb_window_close() {
    let number: u16 = std::env::var("HOI4_NATIVE_X11_FIXTURE_DISPLAY")
        .unwrap()
        .parse()
        .unwrap();
    assert!(number >= 100);
    let server: Identity =
        serde_json::from_str(&std::env::var("HOI4_NATIVE_X11_FIXTURE_SERVER").unwrap()).unwrap();
    let authority = PathBuf::from(std::env::var("HOI4_NATIVE_X11_FIXTURE_AUTHORITY").unwrap());
    assert_eq!(capture_identity(server.pid).unwrap(), server);
    let (connection, screen) = x11rb::connect(Some(&format!(":{number}"))).unwrap();
    if std::env::var("HOI4_NATIVE_X11_CLOSE_NO_XTEST").is_ok() {
        assert!(
            !connection
                .query_extension(b"XTEST")
                .unwrap()
                .reply()
                .unwrap()
                .present
        );
    }
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
    let mut child = WmChild::start("ignore").unwrap();
    let window = u32::try_from(child.ready["window"].as_u64().unwrap()).unwrap();
    let protocols = u32::try_from(child.ready["protocols"].as_u64().unwrap()).unwrap();
    let delete = u32::try_from(child.ready["delete"].as_u64().unwrap()).unwrap();
    let process = RefusableProcess {
        identity: child.identity.clone(),
        refused: AtomicBool::new(false),
    };
    let open = || {
        X11WindowClose::open_until(
            &process,
            number,
            &server,
            &authority,
            window,
            WM_CLASS,
            Instant::now() + Duration::from_secs(2),
        )
    };
    assert_eq!(
        X11WindowClose::open_until(
            &process,
            number,
            &capture_identity(std::process::id() as i32).unwrap(),
            &authority,
            window,
            WM_CLASS,
            Instant::now() + Duration::from_secs(2)
        )
        .err()
        .unwrap()
        .code,
        "ownership_changed"
    );
    assert_eq!(
        X11WindowClose::open_until(
            &process,
            number,
            &server,
            Path::new("relative-authority"),
            window,
            WM_CLASS,
            Instant::now() + Duration::from_secs(2)
        )
        .err()
        .unwrap()
        .code,
        "invalid_xauthority"
    );
    assert!(X11WindowClose::open_until(
        &process,
        number,
        &server,
        &authority,
        window,
        b"wrong\0wrong\0",
        Instant::now() + Duration::from_secs(2)
    )
    .is_err());
    let pid_atom = connection
        .intern_atom(true, b"_NET_WM_PID")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    connection
        .change_property32(
            PropMode::REPLACE,
            window,
            pid_atom,
            AtomEnum::CARDINAL,
            &[std::process::id()],
        )
        .unwrap()
        .check()
        .unwrap();
    assert!(open().is_err());
    connection
        .change_property32(
            PropMode::REPLACE,
            window,
            pid_atom,
            AtomEnum::CARDINAL,
            &[child.child.id()],
        )
        .unwrap()
        .check()
        .unwrap();
    // The global delete atom exists, but the owned window must advertise it in
    // a complete typed bounded list. None of these refusals may deliver input.
    for values in [
        vec![],
        vec![protocols],
        vec![delete, delete],
        vec![delete; 65],
    ] {
        connection
            .change_property32(
                PropMode::REPLACE,
                window,
                protocols,
                AtomEnum::ATOM,
                &values,
            )
            .unwrap()
            .check()
            .unwrap();
        assert!(open().is_err());
    }
    connection
        .change_property8(
            PropMode::REPLACE,
            window,
            protocols,
            AtomEnum::STRING,
            b"bad",
        )
        .unwrap()
        .check()
        .unwrap();
    assert!(open().is_err());
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
    let mut refused = open().unwrap();
    process.refused.store(true, Ordering::Relaxed);
    assert_eq!(
        refused.request_once(&process).delivery,
        CloseDelivery::NotQueued
    );
    process.refused.store(false, Ordering::Relaxed);
    assert_eq!(
        refused.request_once(&process).error.unwrap().code,
        "window_close_already_attempted"
    );
    assert!(X11WindowClose::open_until(
        &process,
        number,
        &server,
        &authority,
        window,
        WM_CLASS,
        Instant::now()
    )
    .is_err());
    assert_eq!(child.events()["events"], serde_json::json!([]));
    let mut closer = open().unwrap();
    assert_eq!(
        closer.request_once(&process).delivery,
        CloseDelivery::ServerConfirmed
    );
    assert_eq!(
        closer.request_once(&process).error.unwrap().code,
        "window_close_already_attempted"
    );
    let observed = child.events();
    assert_eq!(observed["forbidden"], 0);
    assert_eq!(
        observed["events"],
        serde_json::json!([{"format":32,"window":window,"type":protocols,
        "data":[delete,0,0,0,0],"synthetic":true}])
    );
    assert_eq!(
        capture_identity(child.identity.pid).unwrap(),
        child.identity,
        "Ignoring close preserves child"
    );
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
    assert!(
        child.finish().success(),
        "Private EOF cleanup exits normally, no parent signal"
    );
    let mut exiting = WmChild::start("exit").unwrap();
    let exiting_window = u32::try_from(exiting.ready["window"].as_u64().unwrap()).unwrap();
    let exiting_process = FixtureProcess(exiting.identity.clone());
    let mut closer = X11WindowClose::open_until(
        &exiting_process,
        number,
        &server,
        &authority,
        exiting_window,
        WM_CLASS,
        Instant::now() + Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(
        closer.request_once(&exiting_process).delivery,
        CloseDelivery::ServerConfirmed
    );
    assert_eq!(
        exiting.record("events").unwrap()["events"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    use std::os::unix::process::ExitStatusExt;
    let status = exiting.finish();
    assert_eq!(status.code(), Some(0));
    assert_eq!(status.signal(), None);
    assert!(!status.core_dumped());
    connection.destroy_window(other).unwrap().check().unwrap();
}
impl ReadMemory for FixtureProcess {
    fn read(&self, _: u64, _: usize) -> Result<Vec<u8>> {
        Err(Error::new(
            "fixture_only",
            "No game memory in X11 transport fixture",
        ))
    }
}
#[test]
#[ignore = "requires a separately owned authenticated Xvfb display >=100"]
fn owned_xvfb_keyboard_controls() {
    let display =
        std::env::var("HOI4_NATIVE_X11_FIXTURE_DISPLAY").expect("owned fixture display required");
    let number: u16 = display
        .parse()
        .expect("explicit owned fixture display number");
    assert!(
        number >= 100,
        "fixture must use separate high-numbered display"
    );
    let server: Identity = serde_json::from_str(
        &std::env::var("HOI4_NATIVE_X11_FIXTURE_SERVER").expect("owned display identity required"),
    )
    .unwrap();
    let authority = PathBuf::from(
        std::env::var_os("HOI4_NATIVE_X11_FIXTURE_AUTHORITY")
            .expect("explicit owned display authority required"),
    );
    assert!(authority.is_absolute());
    assert_eq!(capture_identity(server.pid).unwrap(), server);
    let (connection, screen) = x11rb::connect(Some(&format!(":{number}"))).unwrap();
    let root = connection.setup().roots[screen].root;
    let pid = std::process::id() as i32;
    // SAFETY: getuid has no arguments or memory effects.
    let uid = unsafe { libc::getuid() };
    assert_ne!(uid, 0);
    let process = FixtureProcess(capture_identity(pid).unwrap());
    let window = connection.generate_id().unwrap();
    connection
        .create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            window,
            root,
            10,
            10,
            300,
            200,
            0,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new().event_mask(EventMask::KEY_PRESS | EventMask::KEY_RELEASE),
        )
        .unwrap()
        .check()
        .unwrap();
    let pid_atom = connection
        .intern_atom(false, b"_NET_WM_PID")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    let active = connection
        .intern_atom(false, b"_NET_ACTIVE_WINDOW")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    let class = b"hoi4-fixture\0hoi4-fixture\0";
    connection
        .change_property32(
            PropMode::REPLACE,
            window,
            pid_atom,
            AtomEnum::CARDINAL,
            &[pid as u32],
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
            class,
        )
        .unwrap()
        .check()
        .unwrap();
    connection.map_window(window).unwrap().check().unwrap();
    connection
        .set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    connection
        .change_property32(PropMode::REPLACE, root, active, AtomEnum::WINDOW, &[window])
        .unwrap()
        .check()
        .unwrap();
    let pointer = connection.query_pointer(root).unwrap().reply().unwrap();
    // A freshly valid process identity for somebody other than this display
    // must fail SO_PEERCRED, even while ambient XAUTHORITY points at valid auth.
    assert_eq!(
        X11Controls::open(&process, number, &process.0, &authority, window, class)
            .err()
            .unwrap()
            .code,
        "ownership_changed"
    );
    let mut stale_server = server.clone();
    stale_server.start_ticks = "0".into();
    assert_eq!(
        X11Controls::open(&process, number, &stale_server, &authority, window, class)
            .err()
            .unwrap()
            .code,
        "ownership_changed"
    );
    let directory = authority.parent().unwrap();
    let alias = directory.join("authority-alias");
    symlink(&authority, &alias).unwrap();
    for path in [
        Path::new("authority"),
        directory,
        alias.as_path(),
        &directory.join("missing-authority"),
    ] {
        assert_eq!(
            X11Controls::open(&process, number, &server, path, window, class)
                .err()
                .unwrap()
                .code,
            "invalid_xauthority"
        );
    }
    fs::remove_file(alias).unwrap();
    let wrong_cookie = directory.join("wrong-cookie");
    let mut bytes = fs::read(&authority).unwrap();
    assert!(bytes.len() <= 64 << 10);
    *bytes.last_mut().unwrap() ^= 0xff;
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&wrong_cookie)
        .unwrap()
        .write_all(&bytes)
        .unwrap();
    assert_eq!(
        X11Controls::open(&process, number, &server, &wrong_cookie, window, class)
            .err()
            .unwrap()
            .code,
        "input_unavailable"
    );
    fs::remove_file(wrong_cookie).unwrap();
    assert_eq!(
        connection.get_input_focus().unwrap().reply().unwrap().focus,
        window
    );
    while let Some(event) = connection.poll_for_event().unwrap() {
        assert!(!matches!(event, Event::KeyPress(_) | Event::KeyRelease(_)));
    }
    let controls = X11Controls::open(&process, number, &server, &authority, window, class).unwrap();
    // A refreshed completed decision, refused decision or expired authority
    // must not send even the first key event to the owned fixture window.
    assert!(!controls
        .send_until_checked(
            &process,
            ControlKey::Pause,
            Instant::now() + Duration::from_secs(5),
            || Ok(false)
        )
        .unwrap());
    assert_eq!(
        controls
            .send_until_checked(
                &process,
                ControlKey::Pause,
                Instant::now() + Duration::from_secs(5),
                || Err(Error::new("state_changed", "Fixture decision changed"))
            )
            .unwrap_err()
            .code,
        "state_changed"
    );
    assert_eq!(
        controls
            .send_until(&process, ControlKey::Pause, Instant::now())
            .unwrap_err()
            .code,
        "input_lease_required"
    );
    connection.get_input_focus().unwrap().reply().unwrap();
    while let Some(event) = connection.poll_for_event().unwrap() {
        assert!(!matches!(event, Event::KeyPress(_) | Event::KeyRelease(_)));
    }
    for key in [ControlKey::Pause, ControlKey::Slower, ControlKey::Faster] {
        controls.send(&process, key).unwrap();
        let start = Instant::now();
        let mut pressed = None;
        let mut released = None;
        while start.elapsed() < Duration::from_secs(2) && released.is_none() {
            match connection.poll_for_event().unwrap() {
                Some(Event::KeyPress(v)) => pressed = Some(v.detail),
                Some(Event::KeyRelease(v)) => released = Some(v.detail),
                Some(_) => {}
                None => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        assert!(pressed.is_some());
        assert_eq!(pressed, released);
    }
    let after = connection.query_pointer(root).unwrap().reply().unwrap();
    assert_eq!(
        (pointer.root_x, pointer.root_y),
        (after.root_x, after.root_y)
    );
    assert!(connection
        .query_keymap()
        .unwrap()
        .reply()
        .unwrap()
        .keys
        .iter()
        .all(|v| *v == 0));
    // Focus loss must refuse rather than activate a window or deliver input.
    connection
        .set_input_focus(InputFocus::PARENT, root, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        controls.send(&process, ControlKey::Pause).unwrap_err().code,
        "input_unavailable"
    );
    connection
        .set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    // A recycled/forged window PID is refused even though class and focus agree.
    connection
        .change_property32(
            PropMode::REPLACE,
            window,
            pid_atom,
            AtomEnum::CARDINAL,
            &[(pid + 1) as u32],
        )
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        controls.send(&process, ControlKey::Pause).unwrap_err().code,
        "ownership_changed"
    );
    connection
        .change_property32(
            PropMode::REPLACE,
            window,
            pid_atom,
            AtomEnum::CARDINAL,
            &[pid as u32],
        )
        .unwrap()
        .check()
        .unwrap();
    let mapping = connection
        .get_keyboard_mapping(
            connection.setup().min_keycode,
            connection.setup().max_keycode - connection.setup().min_keycode + 1,
        )
        .unwrap()
        .reply()
        .unwrap();
    let shift = mapping
        .keysyms
        .chunks_exact(mapping.keysyms_per_keycode as usize)
        .position(|s| s.contains(&0xffe1))
        .unwrap() as u8
        + connection.setup().min_keycode;
    connection
        .xtest_fake_input(
            x11rb::protocol::xproto::KEY_PRESS_EVENT,
            shift,
            0,
            root,
            0,
            0,
            0,
        )
        .unwrap()
        .check()
        .unwrap();
    assert_eq!(
        controls.send(&process, ControlKey::Pause).unwrap_err().code,
        "input_unavailable"
    );
    connection
        .xtest_fake_input(
            x11rb::protocol::xproto::KEY_RELEASE_EVENT,
            shift,
            0,
            root,
            0,
            0,
            0,
        )
        .unwrap()
        .check()
        .unwrap();
    connection.destroy_window(window).unwrap().check().unwrap();
}
