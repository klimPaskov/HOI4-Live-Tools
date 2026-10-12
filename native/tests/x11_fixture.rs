//! Opt-in transport test on a separately owned Xvfb server. This is not HOI4
//! live acceptance, and never opens the user's desktop or game display.
#![cfg(target_os = "linux")]
use hoi4_live_native::{
    error::{Error, Result},
    memory::ReadMemory,
    platform::{
        capture_identity,
        x11::{ControlKey, X11Controls},
        Identity, Process,
    },
};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{symlink, OpenOptionsExt},
    path::{Path, PathBuf},
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
