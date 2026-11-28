//! Development builds only (`dev-table`): live checks of the Windows `SendInput` backend against
//! the owned game, before any tool is bound to it.
//!
//! The owned server watches its private run directory for `probe-req.json`, a JSON array of
//! operations, and appends one result line per operation to `probe-resp.jsonl`. The operator
//! (who owns the run directory and the game) writes the requests. Operations:
//! - `{"op":"bind"}`: bind the owned game's largest visible top-level window at its current
//!   client size, and report the origin, size and DPI awareness.
//! - `{"op":"capture","name":"x"}`: capture the client area into `x.bgra` (top-down BGRA) and
//!   report the size and how much of it is not black.
//! - `{"op":"click","x":..,"y":..,"button":"left|right","modifier":"shift|ctrl"}`
//! - `{"op":"key","key":"space|escape|a|1|f1|up|...","modifier":"shift|ctrl|alt"}`
//! - `{"op":"drag","points":[[x,y],...],"button":"left|right"}`
//! - `{"op":"scroll","x":..,"y":..,"pixels":..}`
//!
//! Coordinates are client pixels. Every input goes through `WinInput` and its refusals. The
//! probe sends nothing by itself and holds no binding before `bind`.
use crate::error::{Error, Result};
use crate::platform::game_input::{vk, GameInput};
use crate::platform::windows::input::WinInput;
use crate::platform::Process;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[repr(C)]
#[derive(Default)]
struct Pt {
    x: i32,
    y: i32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn EnumWindows(callback: extern "system" fn(isize, isize) -> i32, data: isize) -> i32;
    fn GetWindowThreadProcessId(window: isize, pid: *mut u32) -> u32;
    fn IsWindowVisible(window: isize) -> i32;
    fn GetClientRect(window: isize, rect: *mut Rect) -> i32;
    fn ClientToScreen(window: isize, point: *mut Pt) -> i32;
    fn GetWindowDpiAwarenessContext(window: isize) -> isize;
    fn GetAwarenessFromDpiAwarenessContext(context: isize) -> i32;
    fn GetDpiForWindow(window: isize) -> u32;
}

struct Search {
    pid: u32,
    best: Option<(isize, (u32, u32))>,
}
extern "system" fn each(window: isize, data: isize) -> i32 {
    // SAFETY: `data` is the `Search` passed by `game_window` for the duration of EnumWindows.
    let search = unsafe { &mut *(data as *mut Search) };
    let mut pid = 0u32;
    // SAFETY: plain window queries on an enumerated handle.
    unsafe {
        GetWindowThreadProcessId(window, &mut pid);
        if pid != search.pid || IsWindowVisible(window) == 0 {
            return 1;
        }
        let mut r = Rect::default();
        if GetClientRect(window, &mut r) == 0 {
            return 1;
        }
        let size = ((r.right - r.left).max(0) as u32, (r.bottom - r.top).max(0) as u32);
        let area = u64::from(size.0) * u64::from(size.1);
        if area > 0
            && search
                .best
                .is_none_or(|(_, b)| area > u64::from(b.0) * u64::from(b.1))
        {
            search.best = Some((window, size));
        }
    }
    1
}
fn game_window(pid: u32) -> Result<(isize, (u32, u32))> {
    let mut search = Search { pid, best: None };
    // SAFETY: the callback only touches `search`, which outlives the call.
    unsafe { EnumWindows(each, &mut search as *mut Search as isize) };
    search
        .best
        .ok_or_else(|| Error::new("input_unavailable", "The owned game has no visible window yet"))
}

fn key_code(name: &str) -> Result<u16> {
    let name = name.to_ascii_lowercase();
    let code = match name.as_str() {
        "space" => vk::SPACE,
        "escape" => vk::ESCAPE,
        "return" | "enter" => vk::RETURN,
        "tab" => vk::TAB,
        "f1" => vk::F1,
        "f2" => vk::F2,
        "f3" => vk::F3,
        "f4" => vk::F4,
        "f5" => vk::F5,
        "up" => vk::UP,
        "down" => vk::DOWN,
        "left" => vk::LEFT,
        "right" => vk::RIGHT,
        "plus" => vk::KEYPAD_PLUS,
        "minus" => vk::KEYPAD_MINUS,
        s if s.len() == 1 => {
            let c = s.as_bytes()[0];
            match c {
                b'a'..=b'z' => vk::LETTERS[(c - b'a') as usize],
                b'0'..=b'9' => vk::DIGITS[(c - b'0') as usize],
                _ => return Err(Error::new("invalid_arguments", "Unknown probe key")),
            }
        }
        _ => return Err(Error::new("invalid_arguments", "Unknown probe key")),
    };
    Ok(code)
}

fn f(v: &Value, key: &str) -> Result<f64> {
    v[key]
        .as_f64()
        .ok_or_else(|| Error::new("invalid_arguments", format!("{key} required")))
}

struct Bound {
    input: WinInput<'static>,
    client: (u32, u32),
}

fn run_op(
    op: &Value,
    process: &Arc<dyn Process + Send + Sync>,
    pid: u32,
    bound: &mut Option<Bound>,
    directory: &Path,
) -> Result<Value> {
    let kind = op["op"].as_str().unwrap_or("");
    if kind == "bind" {
        if bound.is_some() {
            return Err(Error::new("input_already_bound", "Probe input is already bound"));
        }
        let (window, client) = game_window(pid)?;
        let input = WinInput::open_owned(process.clone(), window, pid, client)?;
        let mut origin = Pt::default();
        // SAFETY: plain window queries on the bound window; one writable POINT.
        let (awareness, dpi) = unsafe {
            ClientToScreen(window, &mut origin);
            (
                GetAwarenessFromDpiAwarenessContext(GetWindowDpiAwarenessContext(window)),
                GetDpiForWindow(window),
            )
        };
        *bound = Some(Bound { input, client });
        return Ok(json!({
            "client": [client.0, client.1],
            "origin": [origin.x, origin.y],
            "dpi_awareness": awareness,
            "window_dpi": dpi,
        }));
    }
    let b = bound
        .as_ref()
        .ok_or_else(|| Error::new("input_unavailable", "Send bind first"))?;
    let game = b.client;
    let modifier = op["modifier"].as_str();
    let right = op["button"].as_str() == Some("right");
    match kind {
        "capture" => {
            let name = op["name"].as_str().unwrap_or("capture");
            if !name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_') {
                return Err(Error::new("invalid_arguments", "Plain capture name required"));
            }
            let c = b.input.capture(game)?;
            let lit = c
                .data
                .chunks_exact(4)
                .filter(|p| p[..3] != [0, 0, 0])
                .count();
            std::fs::write(directory.join(format!("{name}.bgra")), &c.data)?;
            Ok(json!({
                "width": c.width, "height": c.height, "stride": c.stride,
                "not_black": lit as f64 / (c.width * c.height).max(1) as f64,
            }))
        }
        "click" => {
            let (x, y) = (f(op, "x")?, f(op, "y")?);
            match (modifier, right) {
                (None, false) => b.input.click(game, x, y)?,
                (None, true) => b.input.right_click(game, x, y)?,
                (Some("shift"), false) => b.input.shift_click(game, x, y)?,
                (Some("ctrl"), false) => b.input.ctrl_click(game, x, y)?,
                _ => return Err(Error::new("invalid_arguments", "Unsupported click form")),
            };
            Ok(json!({}))
        }
        "key" => {
            let code = key_code(op["key"].as_str().unwrap_or(""))?;
            match modifier {
                None => b.input.press_code(code)?,
                Some("shift") => b.input.press_shifted(code)?,
                Some("ctrl") => b.input.press_control(code)?,
                Some("alt") => b.input.press_option(code)?,
                _ => return Err(Error::new("invalid_arguments", "Unknown modifier")),
            }
            Ok(json!({}))
        }
        "drag" => {
            let points: Vec<(f64, f64)> = op["points"]
                .as_array()
                .ok_or_else(|| Error::new("invalid_arguments", "points required"))?
                .iter()
                .map(|p| (p[0].as_f64().unwrap_or(-1.0), p[1].as_f64().unwrap_or(-1.0)))
                .collect();
            if right {
                b.input.right_drag_path(game, &points)?;
            } else {
                b.input.drag_path(game, &points)?;
            }
            Ok(json!({"points": points.len()}))
        }
        "scroll" => {
            let pixels = op["pixels"].as_i64().unwrap_or(0) as i32;
            b.input.scroll(game, (f(op, "x")?, f(op, "y")?), pixels)?;
            Ok(json!({}))
        }
        _ => Err(Error::new("invalid_arguments", "Unknown probe operation")),
    }
}

/// Watch `directory` for probe requests until `stop` is set. Runs on its own thread.
pub(crate) fn watch(
    process: Arc<dyn Process + Send + Sync>,
    pid: u32,
    directory: PathBuf,
    stop: Arc<AtomicBool>,
) {
    let mut bound = None;
    let request = directory.join("probe-req.json");
    let response = directory.join("probe-resp.jsonl");
    while !stop.load(Ordering::Acquire) {
        std::thread::sleep(Duration::from_millis(300));
        let Ok(bytes) = std::fs::read(&request) else {
            continue;
        };
        let _ = std::fs::remove_file(&request);
        let ops: Vec<Value> = match serde_json::from_slice::<Value>(&bytes) {
            Ok(Value::Array(a)) => a,
            Ok(v) => vec![v],
            Err(e) => vec![json!({"op":"invalid","error":e.to_string()})],
        };
        for op in ops {
            if stop.load(Ordering::Acquire) {
                break;
            }
            let started = Instant::now();
            let result = run_op(&op, &process, pid, &mut bound, &directory);
            let ms = started.elapsed().as_secs_f64() * 1000.0;
            let line = match result {
                Ok(v) => json!({"op": op, "ok": true, "ms": ms, "result": v}),
                Err(e) => json!({"op": op, "ok": false, "ms": ms, "error": e}),
            };
            use std::io::Write;
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&response)
            {
                let _ = writeln!(file, "{line}");
            }
            if let Some(wait) = op["then_ms"].as_u64() {
                std::thread::sleep(Duration::from_millis(wait.min(10_000)));
            }
        }
    }
}
