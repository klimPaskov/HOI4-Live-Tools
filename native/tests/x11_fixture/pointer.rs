//! Explicit generated-window input only, behind the separate Xvfb wrapper.
//! The forwarding display severs/withholds actual wire replies; no injected
//! production transport or game/build whitelist override is installed.
use super::*;
use hoi4_live_native::platform::x11::{PointerDelivery, PointerDisplay};
use std::{
    net::Shutdown,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    os::unix::net::UnixListener,
    sync::{atomic::AtomicUsize, Arc, Mutex},
    thread,
};
use x11rb::protocol::xproto::ConfigureWindowAux;

pub(super) fn pointer_event(event: &Event) -> Option<serde_json::Value> {
    match event {
        Event::MotionNotify(event) => {
            Some(serde_json::json!({"kind":"motion", "window":event.event,
            "client":[event.event_x,event.event_y], "root":[event.root_x,event.root_y]}))
        }
        Event::ButtonPress(event) => {
            Some(serde_json::json!({"kind":"press", "detail":event.detail,
            "window":event.event, "client":[event.event_x,event.event_y], "root":[event.root_x,event.root_y]}))
        }
        Event::ButtonRelease(event) => {
            Some(serde_json::json!({"kind":"release", "detail":event.detail,
            "window":event.event, "client":[event.event_x,event.event_y], "root":[event.root_x,event.root_y]}))
        }
        _ => None,
    }
}

struct Display {
    number: u16,
    server: Identity,
    authority: PathBuf,
}
impl Display {
    fn pointer_transport(&self) -> PointerDisplay<'_> {
        PointerDisplay {
            display: self.number,
            authority: &self.authority,
        }
    }
}
fn display() -> Display {
    // SAFETY: ordinary-user query only; never changes the suite/VM identity.
    assert_ne!(unsafe { libc::getuid() }, 0);
    let number: u16 = std::env::var("HOI4_NATIVE_X11_FIXTURE_DISPLAY")
        .unwrap()
        .parse()
        .unwrap();
    assert!(number >= 100);
    let server: Identity =
        serde_json::from_str(&std::env::var("HOI4_NATIVE_X11_FIXTURE_SERVER").unwrap()).unwrap();
    let authority = PathBuf::from(std::env::var("HOI4_NATIVE_X11_FIXTURE_AUTHORITY").unwrap());
    assert!(authority.is_absolute());
    assert_eq!(capture_identity(server.pid).unwrap(), server);
    Display {
        number,
        server,
        authority,
    }
}

fn focus(connection: &impl Connection, root: u32, window: u32) {
    connection
        .set_input_focus(InputFocus::PARENT, window, x11rb::CURRENT_TIME)
        .unwrap()
        .check()
        .unwrap();
    let active = connection
        .intern_atom(false, b"_NET_ACTIVE_WINDOW")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    connection
        .change_property32(PropMode::REPLACE, root, active, AtomEnum::WINDOW, &[window])
        .unwrap()
        .check()
        .unwrap();
}

#[test]
#[ignore = "requires own authenticated Xvfb via wrapper --pointer-only"]
fn owned_xvfb_pointer_transport_checks_stale_target_and_records_one_pair() {
    let display = display();
    let (connection, screen) = x11rb::connect(Some(&format!(":{}", display.number))).unwrap();
    let root = connection.setup().roots[screen].root;
    let mut child = WmChild::start("pointer").unwrap();
    let window = u32::try_from(child.ready["window"].as_u64().unwrap()).unwrap();
    let process = FixtureProcess(child.identity.clone());
    // A real parent/border makes root translation differ from the child's
    // GetGeometry x/y. Moving only this frame must invalidate the target.
    let frame = connection.generate_id().unwrap();
    connection
        .create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            frame,
            root,
            100,
            60,
            200,
            180,
            3,
            WindowClass::INPUT_OUTPUT,
            0,
            &CreateWindowAux::new(),
        )
        .unwrap()
        .check()
        .unwrap();
    connection.map_window(frame).unwrap().check().unwrap();
    connection
        .reparent_window(window, frame, 20, 20)
        .unwrap()
        .check()
        .unwrap();
    focus(&connection, root, window); // Fixture setup only; candidate never focuses.
    let mut controls = X11Controls::open(
        &process,
        display.number,
        &display.server,
        &display.authority,
        window,
        WM_CLASS,
    )
    .unwrap();
    let pointer_before = connection.query_pointer(root).unwrap().reply().unwrap();
    for point in [[-1, 0], [0, -1], [160, 0], [0, 120]] {
        assert_eq!(
            controls
                .prepare_left_click_until(
                    &process,
                    display.pointer_transport(),
                    point,
                    [160, 120],
                    Instant::now() + Duration::from_secs(2)
                )
                .err()
                .unwrap()
                .code,
            "invalid_client_point"
        );
    }
    assert_eq!(
        controls
            .prepare_left_click_until(
                &process,
                display.pointer_transport(),
                [17, 19],
                [159, 120],
                Instant::now() + Duration::from_secs(2)
            )
            .err()
            .unwrap()
            .code,
        "input_geometry_changed"
    );
    assert!(controls
        .prepare_left_click_until(
            &process,
            display.pointer_transport(),
            [17, 19],
            [160, 120],
            Instant::now()
        )
        .is_err());
    // Move, resize, altered PID/class and lost focus AFTER the immutable
    // binding. Every refusal must precede the first motion, not merely clicks.
    let pid_atom = connection
        .intern_atom(true, b"_NET_WM_PID")
        .unwrap()
        .reply()
        .unwrap()
        .atom;
    for change in 0..5 {
        let mut attempt = controls
            .prepare_left_click_until(
                &process,
                display.pointer_transport(),
                [17, 19],
                [160, 120],
                Instant::now() + Duration::from_secs(2),
            )
            .unwrap();
        match change {
            0 => {
                connection
                    .configure_window(frame, &ConfigureWindowAux::new().x(101))
                    .unwrap()
                    .check()
                    .unwrap();
            }
            1 => {
                connection
                    .configure_window(window, &ConfigureWindowAux::new().width(161))
                    .unwrap()
                    .check()
                    .unwrap();
            }
            2 => {
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
            }
            3 => {
                connection
                    .change_property8(
                        PropMode::REPLACE,
                        window,
                        AtomEnum::WM_CLASS,
                        AtomEnum::STRING,
                        b"changed\0changed\0",
                    )
                    .unwrap()
                    .check()
                    .unwrap();
            }
            _ => {
                focus(&connection, root, root);
            }
        }
        let result = attempt.dispatch_once();
        assert_eq!(
            result.delivery,
            PointerDelivery::NotQueued,
            "Changed target {change}"
        );
        assert!(result.error.is_some());
        assert!(!result.game_action_verified);
        assert_eq!(
            attempt.dispatch_once().error.unwrap().code,
            "pointer_already_attempted"
        );
        connection
            .configure_window(frame, &ConfigureWindowAux::new().x(100))
            .unwrap()
            .check()
            .unwrap();
        connection
            .configure_window(window, &ConfigureWindowAux::new().width(160))
            .unwrap()
            .check()
            .unwrap();
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
        focus(&connection, root, window);
    }
    let before = child.events();
    assert_eq!(before["events"], serde_json::json!([]));
    assert_eq!(before["forbidden"], 0);
    let pointer = connection.query_pointer(root).unwrap().reply().unwrap();
    assert_eq!(
        (pointer.root_x, pointer.root_y),
        (pointer_before.root_x, pointer_before.root_y)
    );
    assert_ne!(
        (pointer.root_x, pointer.root_y),
        (140, 102),
        "Fresh Xvfb pointer must differ from explicit target"
    );
    let mut attempt = controls
        .prepare_left_click_until(
            &process,
            display.pointer_transport(),
            [17, 19],
            [160, 120],
            Instant::now() + Duration::from_secs(2),
        )
        .unwrap();
    let result = attempt.dispatch_once();
    assert_eq!(
        result.delivery,
        PointerDelivery::ServerConfirmed,
        "{:?}",
        result.error
    );
    assert!(result.error.is_none());
    assert!(!result.game_action_verified);
    assert_eq!(
        attempt.dispatch_once().error.unwrap().code,
        "pointer_already_attempted"
    );
    let events = child.events();
    assert_eq!(events["forbidden"], 0);
    assert_eq!(
        events["events"],
        serde_json::json!([
            {"kind":"motion","window":window,"client":[17,19],"root":[140,102]},
            {"kind":"press","detail":1,"window":window,"client":[17,19],"root":[140,102]},
            {"kind":"release","detail":1,"window":window,"client":[17,19],"root":[140,102]}
        ])
    );
    let after = connection.query_pointer(root).unwrap().reply().unwrap();
    assert_eq!((after.root_x, after.root_y), (140, 102));
    assert_eq!(u16::from(after.mask), 0);
    assert_eq!(
        connection.get_input_focus().unwrap().reply().unwrap().focus,
        window
    );
    assert!(
        child.finish().success(),
        "Private EOF only; no child signal"
    );
    connection.destroy_window(frame).unwrap().check().unwrap();
}

#[derive(Clone, Copy)]
enum WireFault {
    Sever,
    Unresponsive,
}

struct ForwardDisplay {
    display: Display,
    socket: PathBuf,
    meta: fs::Metadata,
    directory: PathBuf,
    stop: Arc<AtomicBool>,
    connections: Arc<AtomicUsize>,
    packets: Arc<Mutex<Vec<Vec<u8>>>>,
    withheld: Arc<AtomicUsize>,
    listener: Option<thread::JoinHandle<Result<()>>>,
}

fn nonce() -> Result<String> {
    let mut bytes = [0; 16];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
fn exact_until(
    stream: &mut UnixStream,
    mut bytes: &mut [u8],
    stop: &AtomicBool,
    until: Instant,
) -> Result<()> {
    while !bytes.is_empty() {
        if stop.load(Ordering::Acquire) || Instant::now() >= until {
            return Err(Error::new("fixture_timeout", "Forwarder fixed deadline"));
        }
        stream.set_read_timeout(Some(Duration::from_millis(50)))?;
        match stream.read(bytes) {
            Ok(0) => return Err(Error::new("fixture_closed", "Forwarder EOF")),
            Ok(n) => bytes = &mut bytes[n..],
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
fn u16_wire(bytes: [u8; 2], little: bool) -> usize {
    usize::from(if little {
        u16::from_le_bytes(bytes)
    } else {
        u16::from_be_bytes(bytes)
    })
}

fn upstream_until(source: &Display, until: Instant) -> Result<UnixStream> {
    use std::os::fd::{AsRawFd, FromRawFd};
    // SAFETY: zero is a valid initialized sockaddr_un representation.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    let path = format!("/tmp/.X11-unix/X{}", source.number);
    assert!(path.len() < address.sun_path.len());
    for (to, from) in address.sun_path.iter_mut().zip(path.bytes()) {
        *to = from as libc::c_char;
    }
    // SAFETY: a new private relay descriptor, nonblocking and close-on-exec.
    let raw = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        )
    };
    if raw < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    // SAFETY: socket returned one new owned descriptor.
    let socket = unsafe { UnixStream::from_raw_fd(raw) };
    // SAFETY: a valid initialized address of its exact supplied native size.
    let connected = unsafe {
        libc::connect(
            socket.as_raw_fd(),
            (&address as *const libc::sockaddr_un).cast(),
            std::mem::size_of_val(&address) as libc::socklen_t,
        )
    };
    if connected < 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EINPROGRESS) {
            return Err(error.into());
        }
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::new(
                    "fixture_timeout",
                    "Upstream connection deadline",
                ));
            }
            let mut descriptor = libc::pollfd {
                fd: socket.as_raw_fd(),
                events: libc::POLLOUT,
                revents: 0,
            };
            // SAFETY: one live owned descriptor and one initialized pollfd.
            let result =
                unsafe { libc::poll(&mut descriptor, 1, left.as_millis().clamp(1, 2000) as i32) };
            if result > 0 {
                break;
            }
            if result < 0
                && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
            {
                return Err(std::io::Error::last_os_error().into());
            }
        }
    }
    if let Some(error) = socket.take_error()? {
        return Err(error.into());
    }
    socket.peer_addr()?;
    // SAFETY: initialized native credential output of the exact option size.
    let mut peer: libc::ucred = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of_val(&peer) as libc::socklen_t;
    let result = unsafe {
        libc::getsockopt(
            socket.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut peer as *mut libc::ucred).cast(),
            &mut length,
        )
    };
    if result != 0
        || length as usize != std::mem::size_of_val(&peer)
        || peer.pid != source.server.pid
        || peer.uid != source.server.uid
        || capture_identity(peer.pid)? != source.server
        || Instant::now() >= until
    {
        return Err(Error::new(
            "ownership_changed",
            "Relay upstream is not the owned Xvfb",
        ));
    }
    socket.set_nonblocking(false)?;
    Ok(socket)
}

// This reads ONLY the generated wrapper's private authority. Production still
// uses the strict Xauthority reader, unchanged. No ambient lookup/env mutation.
fn fixture_authority(source: &Display, number: u16) -> Result<Vec<u8>> {
    let bytes = fs::read(&source.authority)?;
    if bytes.len() > 64 << 10 {
        return Err(Error::new("fixture_failed", "Generated authority bound"));
    }
    let mut index = 0;
    let mut matches = Vec::new();
    while index < bytes.len() {
        let word = |index: &mut usize| -> Result<usize> {
            let pair = bytes
                .get(*index..*index + 2)
                .ok_or_else(|| Error::new("fixture_failed", "Authority word"))?;
            *index += 2;
            Ok(usize::from(u16::from_be_bytes(pair.try_into().unwrap())))
        };
        let family = word(&mut index)?;
        let mut parts = Vec::new();
        for _ in 0..4 {
            let n = word(&mut index)?;
            let part = bytes
                .get(index..index + n)
                .ok_or_else(|| Error::new("fixture_failed", "Authority field"))?;
            parts.push(part);
            index += n;
        }
        if family == 256
            && parts[1] == source.number.to_string().as_bytes()
            && parts[2] == b"MIT-MAGIC-COOKIE-1"
            && parts[3].len() == 16
        {
            let mut result = 256u16.to_be_bytes().to_vec();
            for field in [parts[0], number.to_string().as_bytes(), parts[2], parts[3]] {
                result.extend((field.len() as u16).to_be_bytes());
                result.extend(field);
            }
            matches.push(result);
        }
    }
    if matches.len() != 1 {
        return Err(Error::new(
            "fixture_failed",
            "Unique generated cookie required",
        ));
    }
    Ok(matches.remove(0))
}

fn forward(
    mut client: UnixStream,
    source: &Display,
    opcode: u8,
    fault: WireFault,
    stop: Arc<AtomicBool>,
    packets: Arc<Mutex<Vec<Vec<u8>>>>,
    withheld: Arc<AtomicUsize>,
) -> Result<()> {
    let until = Instant::now() + Duration::from_secs(10);
    let mut upstream = upstream_until(source, until)?;
    // Existing owned server is independently pinned before relaying any bytes.
    assert_eq!(capture_identity(source.server.pid)?, source.server);
    upstream.set_write_timeout(Some(Duration::from_secs(2)))?;
    client.set_write_timeout(Some(Duration::from_secs(2)))?;
    let mut hello = [0; 12];
    exact_until(&mut client, &mut hello, &stop, until)?;
    let little = match hello[0] {
        b'l' => true,
        b'B' => false,
        _ => return Err(Error::new("fixture_failed", "X byte order")),
    };
    let name = u16_wire([hello[6], hello[7]], little);
    let data = u16_wire([hello[8], hello[9]], little);
    if name > 256 || data > 256 {
        return Err(Error::new("fixture_failed", "X auth bound"));
    }
    let mut auth = vec![0; ((name + 3) & !3) + ((data + 3) & !3)];
    exact_until(&mut client, &mut auth, &stop, until)?;
    upstream.write_all(&hello)?;
    upstream.write_all(&auth)?;
    let mut setup = [0; 8];
    exact_until(&mut upstream, &mut setup, &stop, until)?;
    let length = u16_wire([setup[6], setup[7]], little) * 4;
    if setup[0] != 1 || length > 128 << 10 {
        return Err(Error::new("fixture_failed", "Owned Xvfb setup"));
    }
    let mut payload = vec![0; length];
    exact_until(&mut upstream, &mut payload, &stop, until)?;
    client.write_all(&setup)?;
    client.write_all(&payload)?;
    let muted = Arc::new(AtomicBool::new(false));
    let mut reply_source = upstream.try_clone()?;
    let mut reply_target = client.try_clone()?;
    let reply_stop = stop.clone();
    let reply_muted = muted.clone();
    let replies = thread::Builder::new()
        .name("pointer-fixture-replies".into())
        .spawn(move || -> Result<()> {
            reply_source.set_read_timeout(Some(Duration::from_millis(50)))?;
            let mut bytes = [0; 4096];
            while !reply_stop.load(Ordering::Acquire) && Instant::now() < until {
                match reply_source.read(&mut bytes) {
                    Ok(0) => return Ok(()),
                    Ok(n) if !reply_muted.load(Ordering::Acquire) => {
                        reply_target.write_all(&bytes[..n])?
                    }
                    Ok(n) => {
                        withheld.fetch_add(n, Ordering::AcqRel);
                    }
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
            exact_until(&mut client, &mut header, &stop, until)?;
            let words = u16_wire([header[2], header[3]], little);
            if words == 0 || words > 16384 {
                return Err(Error::new("fixture_failed", "Normal X request bound"));
            }
            let mut request = vec![0; words * 4];
            request[..4].copy_from_slice(&header);
            exact_until(&mut client, &mut request[4..], &stop, until)?;
            let pointer = header[0] == opcode && header[1] == 2;
            if pointer {
                muted.store(true, Ordering::Release);
            }
            upstream.write_all(&request)?;
            if pointer {
                if request.len() != 36 {
                    return Err(Error::new("fixture_failed", "FakeInput width"));
                }
                packets
                    .lock()
                    .map_err(|_| Error::new("fixture_failed", "Capture poisoned"))?
                    .push(request);
                if matches!(fault, WireFault::Sever) {
                    client.shutdown(Shutdown::Both)?; // Actual wire EOF after first motion.
                    return Ok(());
                }
            }
        }
    })();
    let _ = client.shutdown(Shutdown::Both);
    let _ = upstream.shutdown(Shutdown::Both);
    let reverse = replies
        .join()
        .map_err(|_| Error::new("fixture_failed", "Reply relay panic"))?;
    if !stop.load(Ordering::Acquire) && !matches!(fault, WireFault::Sever) {
        reverse?;
    }
    result
}

impl ForwardDisplay {
    fn start(source: &Display, fault: WireFault, opcode: u8) -> Result<Self> {
        let directory = std::env::temp_dir().join(format!("hoi4-pointer-wire-{}", nonce()?));
        fs::DirBuilder::new().mode(0o700).create(&directory)?;
        let mut bound = None;
        for number in 150..200u16 {
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
            bound.ok_or_else(|| Error::new("fixture_failed", "No private relay display"))?;
        fs::set_permissions(&socket, fs::Permissions::from_mode(0o600))?;
        let meta = fs::symlink_metadata(&socket)?;
        listener.set_nonblocking(true)?;
        let authority = directory.join("authority");
        let mut owned = Self {
            display: Display {
                number,
                server: capture_identity(std::process::id() as i32)?,
                authority,
            },
            socket,
            meta,
            directory,
            stop: Arc::new(AtomicBool::new(false)),
            connections: Arc::new(AtomicUsize::new(0)),
            packets: Arc::new(Mutex::new(Vec::new())),
            withheld: Arc::new(AtomicUsize::new(0)),
            listener: None,
        };
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&owned.display.authority)?
            .write_all(&fixture_authority(source, number)?)?;
        let source = Display {
            number: source.number,
            server: source.server.clone(),
            authority: source.authority.clone(),
        };
        let stop = owned.stop.clone();
        let connections = owned.connections.clone();
        let packets = owned.packets.clone();
        let withheld = owned.withheld.clone();
        owned.listener = Some(
            thread::Builder::new()
                .name("pointer-fixture-listener".into())
                .spawn(move || -> Result<()> {
                    let until = Instant::now() + Duration::from_secs(12);
                    let mut workers = Vec::new();
                    while !stop.load(Ordering::Acquire) && Instant::now() < until {
                        match listener.accept() {
                            Ok((stream, _)) => {
                                let count = connections.fetch_add(1, Ordering::AcqRel);
                                if count >= 2 {
                                    drop(stream);
                                    continue;
                                }
                                let source = Display {
                                    number: source.number,
                                    server: source.server.clone(),
                                    authority: source.authority.clone(),
                                };
                                let stopped = stop.clone();
                                let capture = packets.clone();
                                let withheld = withheld.clone();
                                workers.push(
                                    thread::Builder::new()
                                        .name("pointer-fixture-wire".into())
                                        .spawn(move || {
                                            forward(
                                                stream, &source, opcode, fault, stopped, capture,
                                                withheld,
                                            )
                                        })?,
                                );
                            }
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                            Err(error) => return Err(error.into()),
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    stop.store(true, Ordering::Release);
                    for worker in workers {
                        let result = worker
                            .join()
                            .map_err(|_| Error::new("fixture_failed", "Wire worker panic"))?;
                        if let Err(error) = result {
                            if !matches!(
                                error.code,
                                "fixture_closed" | "fixture_timeout" | "io_error"
                            ) {
                                return Err(error);
                            }
                        }
                    }
                    Ok(())
                })?,
        );
        Ok(owned)
    }
}
impl Drop for ForwardDisplay {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.listener.take() {
            match worker.join() {
                Ok(Ok(())) => {}
                result => eprintln!("Pointer wire fixture cleanup: {result:?}"),
            }
        }
        if let Ok(current) = fs::symlink_metadata(&self.socket) {
            if (current.dev(), current.ino()) == (self.meta.dev(), self.meta.ino()) {
                let _ = fs::remove_file(&self.socket);
            }
        }
        let _ = fs::remove_file(&self.display.authority);
        let _ = fs::remove_dir(&self.directory);
    }
}

#[test]
#[ignore = "requires own authenticated Xvfb; actual severed/unresponsive wire"]
fn owned_xvfb_pointer_ambiguous_wire_never_replays_or_claims_game_action() {
    let actual = display();
    let (connection, screen) = x11rb::connect(Some(&format!(":{}", actual.number))).unwrap();
    let root = connection.setup().roots[screen].root;
    let opcode = connection
        .query_extension(b"XTEST")
        .unwrap()
        .reply()
        .unwrap()
        .major_opcode;
    assert_ne!(opcode, 0);
    for (index, fault) in [WireFault::Sever, WireFault::Unresponsive]
        .into_iter()
        .enumerate()
    {
        let proxy = ForwardDisplay::start(&actual, fault, opcode).unwrap();
        let mut child = WmChild::start("pointer").unwrap();
        let window = u32::try_from(child.ready["window"].as_u64().unwrap()).unwrap();
        let process = FixtureProcess(child.identity.clone());
        focus(&connection, root, window);
        let mut controls = X11Controls::open(
            &process,
            proxy.display.number,
            &proxy.display.server,
            &proxy.display.authority,
            window,
            WM_CLASS,
        )
        .unwrap();
        let start = Instant::now();
        let mut click = controls
            .prepare_left_click_until(
                &process,
                proxy.display.pointer_transport(),
                [18 + index as i32, 20 + index as i32],
                [160, 120],
                start + Duration::from_secs(2),
            )
            .unwrap();
        let delivered = click.dispatch_once();
        assert_eq!(
            delivered.delivery,
            PointerDelivery::Unknown,
            "{:?}",
            delivered.error
        );
        assert!(delivered.error.is_some());
        assert!(!delivered.game_action_verified);
        assert!(
            start.elapsed() < Duration::from_secs(3),
            "One absolute deadline, not one per connection"
        );
        assert_eq!(
            click.dispatch_once().error.unwrap().code,
            "pointer_already_attempted"
        );
        assert_eq!(
            proxy.connections.load(Ordering::Acquire),
            2,
            "No reconnect/replay"
        );
        let captured = proxy.packets.lock().unwrap();
        let types: Vec<_> = captured.iter().map(|packet| packet[4]).collect();
        assert_eq!(
            types,
            if matches!(fault, WireFault::Sever) {
                vec![6]
            } else {
                vec![6, 4, 5]
            }
        );
        drop(captured);
        let events = child.events();
        assert_eq!(events["forbidden"], 0);
        assert_eq!(
            events["events"].as_array().unwrap().len(),
            if matches!(fault, WireFault::Sever) {
                1
            } else {
                3
            }
        );
        if matches!(fault, WireFault::Unresponsive) {
            assert!(
                proxy.withheld.load(Ordering::Acquire) > 0,
                "Real acknowledgement bytes were withheld"
            );
        }
        assert_eq!(
            capture_identity(child.identity.pid).unwrap(),
            child.identity,
            "Transport failure preserves generated owner"
        );
        assert!(child.finish().success(), "EOF-only fixture cleanup");
    }
}
