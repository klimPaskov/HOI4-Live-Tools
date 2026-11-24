//! The in-process backend (stage 2 of the in-process module, `inproc/` in this repository).
//!
//! The module, loaded by the project's own launcher, posts the commands a player could issue through the game's own
//! command path (the UI's validity check, then `Post`), and serves a local channel (a Unix socket in the run folder,
//! owner-only, with a per-process token). This file is the MCP side of that channel: it decides per tool call
//! whether a family goes in process or through the GUI, and posts. The tool keeps its name, schema and its own
//! Verified read-back from memory; only the dispatch changes, and the GUI path stays the fallback inside the same call.
//!
//! The switch, `HOI4_BACKEND` (set by the launcher from the run's settings): `gui` never uses the module;
//! `inprocess` uses it for every family the module reports on (opted in by the launcher and resolved on this build);
//! `auto` (the default) uses it only for families that ALSO passed their live session (the module's
//! `live_passed`), so a family not yet proven live never runs unless a live engineer names it. The channel path
//! comes from `HOI4_INPROC_SOCK`; without it the backend is off.
use crate::error::{Error, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Auto,
    InProcess,
    Gui,
}

pub(super) fn mode_from(value: Option<&str>) -> Mode {
    match value.map(str::trim) {
        Some("gui") => Mode::Gui,
        Some("inprocess") => Mode::InProcess,
        _ => Mode::Auto,
    }
}

pub(super) fn mode() -> Mode {
    mode_from(std::env::var("HOI4_BACKEND").ok().as_deref())
}

/// Whether the hello's family state allows posting this family in this mode.
pub(super) fn family_usable(hello: &Value, family: &str, mode: Mode) -> bool {
    let f = &hello["families"][family];
    let on = f["on"].as_bool() == Some(true) && hello["post"]["enabled"].as_bool() == Some(true);
    match mode {
        Mode::Gui => false,
        Mode::InProcess => on,
        Mode::Auto => on && f["live_passed"].as_bool() == Some(true),
    }
}

pub(super) struct Channel {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
    next_id: i64,
    pub hello: Value,
}

impl Channel {
    /// Connect and authenticate; None when no channel is configured or the module is not serving (the GUI path runs).
    pub(super) fn connect() -> Option<Channel> {
        let path = std::env::var("HOI4_INPROC_SOCK").ok()?;
        let token = std::fs::read_to_string(format!("{path}.token")).ok()?;
        let stream = UnixStream::connect(&path).ok()?;
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
        let writer = stream.try_clone().ok()?;
        let mut ch = Channel {
            reader: BufReader::new(stream),
            writer,
            next_id: 1,
            hello: Value::Null,
        };
        let hello = ch.call(json!({"op":"hello","token":token.trim()})).ok()?;
        if hello["ok"].as_bool() != Some(true) {
            return None;
        }
        ch.hello = hello;
        Some(ch)
    }

    fn call(&mut self, mut req: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        req["v"] = json!(1);
        req["id"] = json!(id);
        let line = request_line(&req)?;
        self.writer
            .write_all(line.as_bytes())
            .map_err(|e| Error::new("inproc_channel", format!("write: {e}")))?;
        let mut reply = String::new();
        self.reader
            .read_line(&mut reply)
            .map_err(|e| Error::new("inproc_channel", format!("read: {e}")))?;
        let v: Value = serde_json::from_str(reply.trim())
            .map_err(|e| Error::new("inproc_channel", format!("reply: {e}")))?;
        if v["id"].as_i64() != Some(id) {
            return Err(Error::new("inproc_channel", "reply id mismatch"));
        }
        Ok(v)
    }

    /// One family op over up to 32 own objects (one command each, one main-thread job). Requests are spaced
    /// `POST_GAP` apart in this process (the module refuses a stage-2 request within 50 ms of the last one, for
    /// every connection); a rate-limit reply is tried once more after the gap.
    pub(super) fn post(
        &mut self,
        op: &str,
        objs: &[u64],
        vals: &[i64],
        obj2: Option<u64>,
        flag: Option<i64>,
    ) -> Result<Value> {
        let req = post_request(op, objs, vals, obj2, flag)?;
        for attempt in 0..2 {
            space_posts();
            let reply = self.call(req.clone())?;
            if attempt == 0 && rate_limited(&reply) {
                continue;
            }
            return Ok(reply);
        }
        unreachable!()
    }
}

impl Channel {
    /// One request as built by the caller (the orders read and posts of `inproc_orders`).
    pub(super) fn call_raw(&mut self, req: Value) -> Result<Value> {
        self.call(req)
    }

    /// A stage-2 post built by the caller, spaced and retried once on a rate limit like `post`.
    pub(super) fn post_raw(&mut self, req: Value) -> Result<Value> {
        for attempt in 0..2 {
            space_posts();
            let reply = self.call(req.clone())?;
            if attempt == 0 && rate_limited(&reply) {
                continue;
            }
            return Ok(reply);
        }
        unreachable!()
    }
}

/// The module's minimum gap between stage-2 requests is 50 ms; 60 leaves room for clock skew.
const POST_GAP: Duration = Duration::from_millis(60);
static LAST_POST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Wait until `POST_GAP` has passed since the last request this process sent, and mark this one.
fn space_posts() {
    let Ok(mut last) = LAST_POST.lock() else {
        return;
    };
    if let Some(t) = *last {
        let since = t.elapsed();
        if since < POST_GAP {
            std::thread::sleep(POST_GAP - since);
        }
    }
    *last = Some(std::time::Instant::now());
}

/// The module's rate-limit refusal (nothing was posted): `"error":"rate limit: ..."`.
pub(super) fn rate_limited(reply: &Value) -> bool {
    reply["ok"].as_bool() != Some(true)
        && ["error", "reason"].iter().any(|k| {
            reply[*k]
                .as_str()
                .is_some_and(|e| e.starts_with("rate limit"))
        })
}

/// The module's parser takes one flat object of at most 8 members, strings without escapes or integers, one line
/// under about 9 KB; lists travel as comma-separated strings.
pub(super) fn post_request(
    op: &str,
    objs: &[u64],
    vals: &[i64],
    obj2: Option<u64>,
    flag: Option<i64>,
) -> Result<Value> {
    if objs.is_empty() || objs.len() > 32 {
        return Err(Error::new("invalid_arguments", "1..32 objects per request"));
    }
    let mut req = json!({
        "op":"post",
        "cmd":op,
        "objs":objs.iter().map(|o| format!("0x{o:x}")).collect::<Vec<_>>().join(","),
    });
    if !vals.is_empty() {
        req["vals"] = json!(vals
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(","));
    }
    if let Some(o) = obj2 {
        req["obj2"] = json!(format!("0x{o:x}"));
    }
    if let Some(f) = flag {
        req["flag"] = json!(f);
    }
    Ok(req)
}

fn request_line(req: &Value) -> Result<String> {
    let line =
        serde_json::to_string(req).map_err(|e| Error::new("inproc_channel", e.to_string()))? + "\n";
    // the module reads lines up to about 9 KB (engineer 73: a front's province list); older modules 1 KiB, but they
    // have no orders family, so no long request reaches them
    if line.len() > 8800 || line.contains('\\') {
        return Err(Error::new(
            "invalid_arguments",
            "request too long for the channel",
        ));
    }
    Ok(line)
}

/// A route for one tool call: Some(channel) when this family goes in process.
pub(super) fn route(family: &str) -> Option<Channel> {
    let m = mode();
    if m == Mode::Gui {
        return None;
    }
    let ch = Channel::connect()?;
    family_usable(&ch.hello, family, m).then_some(ch)
}

/// What one in-process route decided: every command posted, the game's own check refused the change (the answer:
/// the GUI would send the same refused order, so it is not tried), or hand the call to the GUI path.
#[derive(Debug)]
pub(super) enum Routed {
    Done(Value),
    Refused(String),
    Gui,
}

/// The game's own "can do" check refused every object and nothing was posted: the change is not legal now (for
/// example a production line whose factories would pass its cap, or no free dockyard). Any other refusal (not own,
/// not a live object, the policy, the main thread did not run the job) is the module's and the GUI path decides.
pub(super) fn game_refused(reply: &Value) -> bool {
    let objs = reply["per_object"].as_array();
    reply["posted"].as_i64() == Some(0)
        && objs.is_some_and(|o| {
            !o.is_empty() && o.iter().all(|x| x["result"] == "refused_by_game_check")
        })
}

/// The typed error for a game-check refusal; the journal reports it unless the read-back shows the change anyway.
pub(super) fn refused_error(what: &str) -> Error {
    Error::new(
        "refused_by_game_check",
        format!("The game's own check refused this order ({what}); nothing was posted"),
    )
}

/// True when every object of the reply was posted (the module refuses whole objects, never parts of one).
pub(super) fn all_posted(reply: &Value) -> bool {
    reply["ok"].as_bool() == Some(true)
        && reply["posted"].as_i64().is_some()
        && reply["posted"].as_i64() == reply["objects"].as_i64()
}

/// The production delta for a line: factories asked minus factories requested now; 0 removes the line.
pub(super) fn production_op(requested_now: i32, wanted: i32) -> Option<(&'static str, i64)> {
    match (wanted, wanted - requested_now) {
        (0, _) => Some(("production.remove_line", 0)),
        (_, 0) => None,
        (_, d) => Some(("production.add_factories", i64::from(d))),
    }
}

/// A new production line in process (family production2 for the line, production for its factories): one
/// AddProductionLine(own tag, variant, no organisation, not at the top), read until the new line shows, then one
/// AddProductionLineFactories for the difference to `factories`, read until it shows. `new_line` answers the new line's
/// (address, requested factories). Gui when the line did not appear (nothing else posted).
pub(super) fn add_line(
    ch: &mut Channel,
    variant: u64,
    factories: i32,
    new_line: &mut dyn FnMut() -> Option<(u64, i32)>,
) -> Routed {
    let Ok(r) = ch.post("production2.add_line", &[variant], &[], None, Some(0)) else {
        return Routed::Gui;
    };
    if game_refused(&r) {
        return Routed::Refused("a line of this design".into());
    }
    if !all_posted(&r) {
        return Routed::Gui;
    }
    let start = std::time::Instant::now();
    let mut line = None;
    while start.elapsed() < MISSION_BIT_WAIT {
        line = new_line();
        if line.is_some() {
            break;
        }
        std::thread::sleep(MISSION_BIT_POLL);
    }
    let Some((address, requested)) = line else {
        // posted but not seen: the journal's settle decides (never post a second line)
        return Routed::Done(json!({"backend":"inprocess","module":[r,{"line_seen":false}]}));
    };
    let mut replies = vec![
        r,
        json!({"line_seen":true,"waited_ms":start.elapsed().as_millis() as u64}),
    ];
    if let Some((op, delta)) = production_op(requested, factories) {
        if op == "production.add_factories" {
            if let Ok(f) = ch.post(op, &[address], &[delta], None, None) {
                let t = std::time::Instant::now();
                while t.elapsed() < MISSION_BIT_WAIT
                    && new_line().map(|(_, n)| n) != Some(factories)
                {
                    std::thread::sleep(MISSION_BIT_POLL);
                }
                replies.push(f);
            }
        }
    }
    Routed::Done(json!({"backend":"inprocess","module":replies}))
}

/// A line's priority in process (family production2): ChangePriority(line, place), then read until `at_place` holds.
pub(super) fn priority_post(
    ch: &mut Channel,
    line: u64,
    place: usize,
    at_place: &mut dyn FnMut() -> Option<bool>,
) -> Routed {
    let Ok(r) = ch.post("production2.priority", &[line], &[place as i64], None, None) else {
        return Routed::Gui;
    };
    if game_refused(&r) {
        return Routed::Refused("this line's priority".into());
    }
    if !all_posted(&r) {
        return Routed::Gui;
    }
    let start = std::time::Instant::now();
    let mut seen = false;
    while start.elapsed() < MISSION_BIT_WAIT {
        if at_place() == Some(true) {
            seen = true;
            break;
        }
        std::thread::sleep(MISSION_BIT_POLL);
    }
    Routed::Done(
        json!({"backend":"inprocess","module":[r,{"executed":seen,"waited_ms":start.elapsed().as_millis() as u64}]}),
    )
}

/// The air mission plan per wing: (assign the region, set the mission bit). Posted in the UI's order: the mission
/// button first (CStratAirSetMissionCommand), then the region (CStratAirEnableMissionCommand).
pub(super) fn mission_steps(
    wing_region: i32,
    wing_mask: i32,
    region: Option<i32>,
    mask: i32,
) -> (bool, bool) {
    let to_region = matches!(region, Some(r) if r != wing_region);
    let set_bit = wing_mask & mask == 0;
    (to_region, set_bit)
}

/// How long the region step waits for the game to execute the mission bits (it executes posted commands at its
/// next frame, also while paused), and how often it reads.
const MISSION_BIT_WAIT: Duration = Duration::from_millis(1500);
const MISSION_BIT_POLL: Duration = Duration::from_millis(15);

/// True when every wing at these addresses carries the mission bit in `wings`.
pub(super) fn bits_executed(wings: &[super::airread::Wing], addresses: &[u64], mask: i32) -> bool {
    addresses.iter().all(|a| {
        wings
            .iter()
            .any(|w| w.address == *a && w.mission_mask & mask != 0)
    })
}

/// True when every wing at these addresses is assigned to `region` in `wings`.
pub(super) fn regions_executed(
    wings: &[super::airread::Wing],
    addresses: &[u64],
    region: i32,
) -> bool {
    addresses.iter().all(|a| {
        wings
            .iter()
            .any(|w| w.address == *a && w.region_id == region)
    })
}

/// Missions in process: one request for the wings lacking the bit, one for the wings not in the region. The game's
/// check for the region command reads the wing's mission, so the region request waits until the game has executed
/// the mission commands (one frame; read back from memory by `reread`), as a player's region click always comes a
/// frame after the mission button. Gui when a needed object is not known (the region object is only known from an
/// own base's province, so a region with no own base goes through the GUI) or the bits were not seen in time.
pub(super) fn air_mission(
    ch: &mut Channel,
    bases: &[super::airread::Base],
    wings: &[super::airread::Wing],
    names: &[String],
    region: Option<i32>,
    mask: i32,
    reread: &mut dyn FnMut() -> Option<Vec<super::airread::Wing>>,
) -> Routed {
    let mut need_bit = Vec::new();
    let mut need_region = Vec::new();
    for n in names {
        let Some(w) = wings.iter().find(|w| &w.name == n && w.address != 0) else {
            return Routed::Gui;
        };
        let (to_region, set_bit) = mission_steps(w.region_id, w.mission_mask, region, mask);
        if set_bit {
            need_bit.push(w.address);
        }
        if to_region {
            need_region.push(w.address);
        }
    }
    // the region: the object found through an own base's province (family air), else, when the airwing family is
    // usable, the region id itself (the map's region click needs no own base; the module looks the region up the way
    // the command's own check does); else the GUI
    let by_id = family_usable(&ch.hello, "airwing", mode());
    let region_obj = match region {
        Some(r) if !need_region.is_empty() => {
            match bases
                .iter()
                .find(|b| b.region_id == Some(r) && b.region_address != 0)
            {
                Some(b) => Some(RegionRef::Object(b.region_address)),
                None if by_id && r > 0 => Some(RegionRef::Id(r)),
                None => return Routed::Gui,
            }
        }
        _ => None,
    };
    let mut replies = Vec::new();
    if !need_bit.is_empty() {
        let Ok(r) = ch.post(
            "air.set_mission",
            &need_bit,
            &[i64::from(mask)],
            None,
            Some(1),
        ) else {
            return Routed::Gui;
        };
        if game_refused(&r) {
            return Routed::Refused("the mission for these wings".into());
        }
        if !all_posted(&r) {
            return Routed::Gui;
        }
        replies.push(r);
        if region_obj.is_some() {
            let start = std::time::Instant::now();
            let mut seen = false;
            while start.elapsed() < MISSION_BIT_WAIT {
                if reread().is_some_and(|now| bits_executed(&now, &need_bit, mask)) {
                    seen = true;
                    break;
                }
                std::thread::sleep(MISSION_BIT_POLL);
            }
            let waited = start.elapsed().as_millis() as u64;
            replies.push(json!({"mission_bits_executed":seen,"waited_ms":waited}));
            if !seen {
                return Routed::Gui;
            }
        }
    }
    if let Some(rr) = region_obj {
        let posted = match rr {
            RegionRef::Object(obj) => {
                ch.post("air.enable_mission", &need_region, &[], Some(obj), None)
            }
            RegionRef::Id(id) => {
                ch.post("airwing.region", &need_region, &[i64::from(id)], None, None)
            }
        };
        let Ok(r) = posted else {
            return Routed::Gui;
        };
        if game_refused(&r) {
            return Routed::Refused("the region for these wings".into());
        }
        if !all_posted(&r) {
            return Routed::Gui;
        }
        replies.push(r);
        // the call's read-back follows at once: return only once the game has executed the region (a read right
        // after the post saw the old region and the call closed NotApplied, s2d 24 Nov 2026)
        if let Some(want) = region {
            let start = std::time::Instant::now();
            let mut seen = false;
            while start.elapsed() < MISSION_BIT_WAIT {
                if reread().is_some_and(|now| regions_executed(&now, &need_region, want)) {
                    seen = true;
                    break;
                }
                std::thread::sleep(MISSION_BIT_POLL);
            }
            let waited = start.elapsed().as_millis() as u64;
            replies.push(json!({"region_executed":seen,"waited_ms":waited}));
        }
    }
    Routed::Done(json!({"backend":"inprocess","wings":names,"module":replies}))
}

/// How a mission's region reaches the module: the region object (read through an own base), or its id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RegionRef {
    Object(u64),
    Id(i32),
}

/// Post one wing-structure request (family airwing) and return only once the game has executed it: `done` reads the
/// wings again until it holds (the call's Verified read-back follows at once; a read right after the post can still
/// see the old state, s2d/s2g 24 Nov 2026). Refused when the game's own check refused every object; Gui on any other
/// refusal or silence (the GUI path decides in the same call).
#[allow(clippy::too_many_arguments)]
pub(super) fn airwing_post(
    ch: &mut Channel,
    op: &str,
    objs: &[u64],
    vals: &[i64],
    obj2: Option<u64>,
    what: &str,
    reread: &mut dyn FnMut() -> Option<Vec<super::airread::Wing>>,
    done: &dyn Fn(&[super::airread::Wing]) -> bool,
) -> Routed {
    let Ok(r) = ch.post(op, objs, vals, obj2, None) else {
        return Routed::Gui;
    };
    if game_refused(&r) {
        return Routed::Refused(what.into());
    }
    if !all_posted(&r) {
        return Routed::Gui;
    }
    let start = std::time::Instant::now();
    let mut seen = false;
    while start.elapsed() < MISSION_BIT_WAIT {
        if reread().is_some_and(|now| done(&now)) {
            seen = true;
            break;
        }
        std::thread::sleep(MISSION_BIT_POLL);
    }
    let waited = start.elapsed().as_millis() as u64;
    Routed::Done(
        json!({"backend":"inprocess","op":op,"module":[r,{"executed":seen,"waited_ms":waited}]}),
    )
}

/// The address of an own wing by name (0 or absent: not known, the GUI path runs).
pub(super) fn wing_address(wings: &[super::airread::Wing], name: &str) -> Option<u64> {
    wings
        .iter()
        .find(|w| w.name == name && w.address != 0)
        .map(|w| w.address)
}

/// Hold in process: the Hold button's command, EnableMission(wing, region 0). A wing flying a transfer stays on the
/// GUI path (the game's hold helper cancels the transfer first: a second command this family does not post).
pub(super) fn air_hold(
    ch: &mut Channel,
    wings: &[super::airread::Wing],
    wing: &str,
    reread: &mut dyn FnMut() -> Option<Vec<super::airread::Wing>>,
) -> Routed {
    let Some(w) = wings.iter().find(|w| w.name == wing && w.address != 0) else {
        return Routed::Gui;
    };
    if w.transfer_base_id != 0 {
        return Routed::Gui;
    }
    let a = w.address;
    airwing_post(
        ch,
        "airwing.region",
        &[a],
        &[0],
        None,
        "the hold of this wing",
        reread,
        &|now| now.iter().any(|x| x.address == a && x.region_id <= 0),
    )
}

/// The aircraft the Split button moves to the new wing: half, rounded down (the UI's helper: count / 2).
pub(super) fn split_count(planes: i32) -> i64 {
    i64::from(planes / 2)
}

/// Split in process: Split(wing, aircraft / 2), as the toolbar's Split button. Done once a new wing stands at the base
/// and the original holds fewer aircraft.
pub(super) fn air_split(
    ch: &mut Channel,
    wings: &[super::airread::Wing],
    plan: &super::airact::SplitPlan,
    reread: &mut dyn FnMut() -> Option<Vec<super::airread::Wing>>,
) -> Routed {
    let Some(a) = wing_address(wings, &plan.wing) else {
        return Routed::Gui;
    };
    let n = split_count(plan.planes);
    if n < 1 {
        return Routed::Gui;
    }
    airwing_post(
        ch,
        "airwing.split",
        &[a],
        &[n],
        None,
        "the split of this wing",
        reread,
        &|now| super::airact::settle_split(plan, now).0 == super::settle::Verdict::Reached,
    )
}

/// Merge in process: ONE Consolidate command for the whole group (the first named wing first), as the Merge button.
pub(super) fn air_merge(
    ch: &mut Channel,
    wings: &[super::airread::Wing],
    plan: &super::airact::MergePlan,
    reread: &mut dyn FnMut() -> Option<Vec<super::airread::Wing>>,
) -> Routed {
    let mut order = vec![plan.wing.clone()];
    order.extend(plan.group.iter().filter(|n| **n != plan.wing).cloned());
    let mut objs = Vec::new();
    for n in &order {
        match wing_address(wings, n) {
            Some(a) => objs.push(a),
            None => return Routed::Gui,
        }
    }
    if objs.len() < 2 {
        return Routed::Gui;
    }
    airwing_post(
        ch,
        "airwing.merge",
        &objs,
        &[],
        None,
        "the merge of these wings",
        reread,
        &|now| super::airact::settle_merge(plan, now) == super::settle::Verdict::Reached,
    )
}

/// Disband in process: Delete(owner tag, wing), as the overview's Disband button; the aircraft go to the reserve.
pub(super) fn air_disband(
    ch: &mut Channel,
    wings: &[super::airread::Wing],
    wing: &str,
    reread: &mut dyn FnMut() -> Option<Vec<super::airread::Wing>>,
) -> Routed {
    let Some(a) = wing_address(wings, wing) else {
        return Routed::Gui;
    };
    airwing_post(
        ch,
        "airwing.disband",
        &[a],
        &[],
        None,
        "the disbanding of this wing",
        reread,
        &|now| !now.iter().any(|x| x.address == a),
    )
}

/// Rebases in process: one air.transfer request per destination base, only for wings not already there or bound
/// there. Done when every command was posted, Refused when the game's own check refused a base's wings, else Gui.
pub(super) fn air_rebase(
    ch: &mut Channel,
    bases: &[super::airread::Base],
    wings: &[super::airread::Wing],
    plans: &[super::airact::RebasePlan],
    reread: &mut dyn FnMut() -> Option<Vec<super::airread::Wing>>,
) -> Routed {
    let mut by_base: Vec<(u64, Vec<u64>)> = Vec::new();
    let mut want: Vec<(u64, i32)> = Vec::new();
    for p in plans {
        let Some(w) = wings.iter().find(|w| w.name == p.wing && w.address != 0) else {
            return Routed::Gui;
        };
        if w.base_id == p.to_base_id || w.transfer_base_id == p.to_base_id {
            continue;
        }
        let Some(b) = bases
            .iter()
            .find(|b| b.id == p.to_base_id && b.address != 0)
        else {
            return Routed::Gui;
        };
        want.push((w.address, p.to_base_id));
        match by_base.iter_mut().find(|(a, _)| *a == b.address) {
            Some((_, list)) => list.push(w.address),
            None => by_base.push((b.address, vec![w.address])),
        }
    }
    let mut replies = Vec::new();
    for (base, list) in &by_base {
        let Ok(r) = ch.post("air.transfer", list, &[], Some(*base), None) else {
            return Routed::Gui;
        };
        if game_refused(&r) && replies.is_empty() {
            return Routed::Refused("the rebase of these wings".into());
        }
        if !all_posted(&r) {
            return Routed::Gui;
        }
        replies.push(r);
    }
    // the call's read-back follows at once: return only once the game has executed every transfer (read right after
    // the post, one of two wings still showed no transfer and the call closed Diverged, s2g 24 Nov 2026)
    if !want.is_empty() {
        let start = std::time::Instant::now();
        let mut seen = false;
        while start.elapsed() < MISSION_BIT_WAIT {
            if reread().is_some_and(|now| transfers_executed(&now, &want)) {
                seen = true;
                break;
            }
            std::thread::sleep(MISSION_BIT_POLL);
        }
        replies.push(
            json!({"transfers_executed":seen,"waited_ms":start.elapsed().as_millis() as u64}),
        );
    }
    Routed::Done(json!({"backend":"inprocess","module":replies}))
}

/// True when every (wing address, base id) pair shows the wing at that base or on its way to it.
pub(super) fn transfers_executed(wings: &[super::airread::Wing], want: &[(u64, i32)]) -> bool {
    want.iter().all(|(a, base)| {
        wings
            .iter()
            .any(|w| w.address == *a && (w.base_id == *base || w.transfer_base_id == *base))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes() {
        assert_eq!(mode_from(None), Mode::Auto);
        assert_eq!(mode_from(Some("gui")), Mode::Gui);
        assert_eq!(mode_from(Some("inprocess")), Mode::InProcess);
        assert_eq!(mode_from(Some("anything")), Mode::Auto);
    }

    #[test]
    fn units_and_picks_stay_off_in_auto_until_their_live_pass() {
        let hello = json!({"post":{"enabled":true},"families":{
            "units":{"on":true,"live_passed":false},"picks":{"on":true,"live_passed":false}}});
        for fam in ["units", "picks"] {
            assert!(!family_usable(&hello, fam, Mode::Auto));
            assert!(family_usable(&hello, fam, Mode::InProcess));
            assert!(!family_usable(&hello, fam, Mode::Gui));
        }
        let off = json!({"post":{"enabled":true},"families":{"units":{"on":false,"live_passed":true}}});
        assert!(!family_usable(&off, "units", Mode::InProcess));
    }

    #[test]
    fn split_moves_half_rounded_down() {
        assert_eq!(split_count(100), 50);
        assert_eq!(split_count(41), 20);
        assert_eq!(split_count(2), 1);
        assert_eq!(split_count(1), 0);
    }

    #[test]
    fn airwing_is_off_in_auto_until_its_live_pass() {
        // the module's hello as built by engineer 71: airwing on (opted in, resolved) but not live-passed
        let hello = json!({"post":{"enabled":true},"families":{
            "air":{"on":true,"live_passed":true},"airwing":{"on":true,"live_passed":false}}});
        assert!(!family_usable(&hello, "airwing", Mode::Auto));
        assert!(family_usable(&hello, "airwing", Mode::InProcess));
        assert!(family_usable(&hello, "air", Mode::Auto));
    }

    #[test]
    fn production2_is_off_in_auto_and_a_new_line_needs_both_families() {
        let hello = json!({"post":{"enabled":true},"families":{
            "production":{"on":true,"live_passed":true},"production2":{"on":true,"live_passed":false}}});
        assert!(!family_usable(&hello, "production2", Mode::Auto));
        assert!(family_usable(&hello, "production2", Mode::InProcess));
        // the add path posts the line (production2) and then its factories (production)
        assert!(family_usable(&hello, "production", Mode::InProcess));
        // the factories of the new line: from 0 to 5 is one add_factories of 5
        assert_eq!(production_op(0, 5), Some(("production.add_factories", 5)));
    }

    #[test]
    fn auto_uses_only_live_passed_families() {
        let hello = json!({"post":{"enabled":true},"families":{
            "production":{"on":true,"live_passed":false},"air":{"on":true,"live_passed":true}}});
        assert!(!family_usable(&hello, "production", Mode::Auto));
        assert!(family_usable(&hello, "production", Mode::InProcess));
        assert!(family_usable(&hello, "air", Mode::Auto));
        assert!(!family_usable(&hello, "air", Mode::Gui));
        assert!(!family_usable(&hello, "navy", Mode::InProcess));
        let off =
            json!({"post":{"enabled":false},"families":{"air":{"on":true,"live_passed":true}}});
        assert!(!family_usable(&off, "air", Mode::InProcess));
    }

    #[test]
    fn requests_fit_the_module_parser() {
        let objs: Vec<u64> = (0..32).map(|i| 0x7f00_0000_0000 + i * 0x1000).collect();
        let req = post_request("air.transfer", &objs, &[], Some(0x7f00_1234_5000), None).unwrap();
        let line = request_line(&{
            let mut r = req.clone();
            r["v"] = json!(1);
            r["id"] = json!(99);
            r
        })
        .unwrap();
        assert!(line.len() < 1000, "{}", line.len());
        assert_eq!(req.as_object().unwrap().len() + 2, 6);
        assert!(post_request("air.transfer", &[], &[], None, None).is_err());
        assert!(post_request("air.transfer", &[1; 33], &[], None, None).is_err());
        let p = post_request(
            "production.add_factories",
            &[0x10, 0x20],
            &[5, -3],
            None,
            None,
        )
        .unwrap();
        assert_eq!(p["objs"], "0x10,0x20");
        assert_eq!(p["vals"], "5,-3");
    }

    #[test]
    fn production_and_mission_plans() {
        assert_eq!(production_op(5, 5), None);
        assert_eq!(production_op(5, 0), Some(("production.remove_line", 0)));
        assert_eq!(production_op(5, 15), Some(("production.add_factories", 10)));
        assert_eq!(
            production_op(15, 3),
            Some(("production.add_factories", -12))
        );
        assert_eq!(mission_steps(0, 0, Some(12), 1), (true, true));
        assert_eq!(mission_steps(12, 1, Some(12), 1), (false, false));
        assert_eq!(mission_steps(12, 2, Some(12), 1), (false, true));
        assert_eq!(mission_steps(0, 0, None, 2048), (false, true));
    }

    #[test]
    fn a_game_check_refusal_is_its_own_answer() {
        let refused = json!({"ok":false,"posted":0,"objects":1,"refused":1,
            "per_object":[{"obj":"0x1","result":"refused_by_game_check"}]});
        assert!(game_refused(&refused));
        let not_own = json!({"ok":false,"posted":0,"objects":2,"refused":2,"per_object":[
            {"obj":"0x1","result":"refused_by_game_check"},{"obj":"0x2","result":"not_own"}]});
        assert!(!game_refused(&not_own));
        let partly = json!({"ok":false,"posted":1,"objects":2,"refused":1,"per_object":[
            {"obj":"0x1","result":"refused_by_game_check"},{"obj":"0x2","result":"posted"}]});
        assert!(!game_refused(&partly));
        assert!(!game_refused(&json!({"op":"x","result":"not_run"})));
        assert!(!game_refused(&json!({"posted":0,"per_object":[]})));
        assert_eq!(refused_error("x").code, "refused_by_game_check");
    }

    #[test]
    fn the_region_waits_for_every_mission_bit() {
        let wing = |address: u64, mask: i32| super::super::airread::Wing {
            address,
            mission_mask: mask,
            ..Default::default()
        };
        let now = vec![wing(0x10, 1), wing(0x20, 0), wing(0x30, 3)];
        assert!(bits_executed(&now, &[0x10, 0x30], 1));
        assert!(!bits_executed(&now, &[0x10, 0x20], 1));
        assert!(!bits_executed(&now, &[0x40], 1));
        assert!(bits_executed(&now, &[0x30], 2));
        let mut there = wing(0x10, 1);
        there.region_id = 7;
        assert!(regions_executed(
            &[there.clone(), wing(0x20, 1)],
            &[0x10],
            7
        ));
        assert!(!regions_executed(&[there, wing(0x20, 1)], &[0x10, 0x20], 7));
        let mut moving = wing(0x10, 1);
        moving.transfer_base_id = 5;
        assert!(transfers_executed(&[moving.clone()], &[(0x10, 5)]));
        assert!(!transfers_executed(
            &[moving, wing(0x20, 1)],
            &[(0x10, 5), (0x20, 5)]
        ));
    }

    #[test]
    fn a_rate_limit_reply_is_retried_and_requests_are_spaced() {
        assert!(rate_limited(&json!({"ok":false,
            "error":"rate limit: 12 ms since the last request (minimum 50)"})));
        assert!(!rate_limited(
            &json!({"ok":false,"error":"objs: 1 to 32 comma-separated object addresses"})
        ));
        assert!(!rate_limited(&json!({"ok":true,"posted":1,"objects":1})));
        let t = std::time::Instant::now();
        space_posts();
        space_posts();
        assert!(t.elapsed() >= POST_GAP);
    }

    #[test]
    fn all_posted_needs_every_object() {
        assert!(all_posted(&json!({"ok":true,"posted":3,"objects":3})));
        assert!(!all_posted(&json!({"ok":true,"posted":2,"objects":3})));
        assert!(!all_posted(&json!({"ok":false,"posted":3,"objects":3})));
    }
}
