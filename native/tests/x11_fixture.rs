//! Opt-in transport test on a separately owned Xvfb server. This is not HOI4
//! live acceptance, and never opens the user's desktop or game display.
#![cfg(target_os = "linux")]
use hoi4_live_native::{
    error::{Error, Result},
    memory::ReadMemory,
    platform::{
        parse_stat,
        x11::{ControlKey, X11Controls},
        Identity, Process,
    },
};
use std::{
    fs,
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
        let p = format!("/proc/{}", self.0.pid);
        let (_, _, ticks) = parse_stat(&fs::read_to_string(format!("{p}/stat"))?)?;
        if ticks != self.0.start_ticks
            || fs::read_link(format!("{p}/exe"))?.to_string_lossy() != self.0.exe
        {
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
    let (connection, screen) = x11rb::connect(Some(&format!(":{number}"))).unwrap();
    let root = connection.setup().roots[screen].root;
    let pid = std::process::id() as i32;
    let (_, _, ticks) =
        parse_stat(&fs::read_to_string(format!("/proc/{pid}/stat")).unwrap()).unwrap();
    // SAFETY: getuid has no arguments or memory effects.
    let uid = unsafe { libc::getuid() };
    assert_ne!(uid, 0);
    let process = FixtureProcess(Identity {
        pid,
        start_ticks: ticks,
        exe: std::env::current_exe().unwrap().to_string_lossy().into(),
        uid,
    });
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
    let controls = X11Controls::open(&process, number, window, class).unwrap();
    let pointer = connection.query_pointer(root).unwrap().reply().unwrap();
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
