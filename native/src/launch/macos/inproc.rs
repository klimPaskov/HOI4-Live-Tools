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

    /// One family op over up to 32 own objects (one command each, one main-thread job).
    pub(super) fn post(
        &mut self,
        op: &str,
        objs: &[u64],
        vals: &[i64],
        obj2: Option<u64>,
        flag: Option<i64>,
    ) -> Result<Value> {
        self.call(post_request(op, objs, vals, obj2, flag)?)
    }
}

/// The module's parser takes one flat object of at most 8 members, strings without escapes or integers, one line
/// under 1 KiB; lists travel as comma-separated strings.
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
    if line.len() > 1000 || line.contains('\\') {
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

/// Missions in process: one request for the wings lacking the bit, one for the wings not in the region. Some(info)
/// when every needed command was posted; None hands the whole call to the GUI path (the region object is only known
/// from an own base's province, so a region with no own base goes through the GUI).
pub(super) fn air_mission(
    ch: &mut Channel,
    bases: &[super::airread::Base],
    wings: &[super::airread::Wing],
    names: &[String],
    region: Option<i32>,
    mask: i32,
) -> Option<Value> {
    let mut need_bit = Vec::new();
    let mut need_region = Vec::new();
    for n in names {
        let w = wings.iter().find(|w| &w.name == n && w.address != 0)?;
        let (to_region, set_bit) = mission_steps(w.region_id, w.mission_mask, region, mask);
        if set_bit {
            need_bit.push(w.address);
        }
        if to_region {
            need_region.push(w.address);
        }
    }
    let region_obj = match region {
        Some(r) if !need_region.is_empty() => Some(
            bases
                .iter()
                .find(|b| b.region_id == Some(r) && b.region_address != 0)?
                .region_address,
        ),
        _ => None,
    };
    let mut replies = Vec::new();
    if !need_bit.is_empty() {
        let r = ch
            .post(
                "air.set_mission",
                &need_bit,
                &[i64::from(mask)],
                None,
                Some(1),
            )
            .ok()?;
        if !all_posted(&r) {
            return None;
        }
        replies.push(r);
    }
    if let Some(obj) = region_obj {
        let r = ch
            .post("air.enable_mission", &need_region, &[], Some(obj), None)
            .ok()?;
        if !all_posted(&r) {
            return None;
        }
        replies.push(r);
    }
    Some(json!({"backend":"inprocess","wings":names,"module":replies}))
}

/// Rebases in process: one air.transfer request per destination base, only for wings not already there or bound
/// there. Some(info) when every command was posted, None for the GUI path.
pub(super) fn air_rebase(
    ch: &mut Channel,
    bases: &[super::airread::Base],
    wings: &[super::airread::Wing],
    plans: &[super::airact::RebasePlan],
) -> Option<Value> {
    let mut by_base: Vec<(u64, Vec<u64>)> = Vec::new();
    for p in plans {
        let w = wings.iter().find(|w| w.name == p.wing && w.address != 0)?;
        if w.base_id == p.to_base_id || w.transfer_base_id == p.to_base_id {
            continue;
        }
        let b = bases
            .iter()
            .find(|b| b.id == p.to_base_id && b.address != 0)?
            .address;
        match by_base.iter_mut().find(|(a, _)| *a == b) {
            Some((_, list)) => list.push(w.address),
            None => by_base.push((b, vec![w.address])),
        }
    }
    let mut replies = Vec::new();
    for (base, list) in &by_base {
        let r = ch.post("air.transfer", list, &[], Some(*base), None).ok()?;
        if !all_posted(&r) {
            return None;
        }
        replies.push(r);
    }
    Some(json!({"backend":"inprocess","module":replies}))
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
    fn all_posted_needs_every_object() {
        assert!(all_posted(&json!({"ok":true,"posted":3,"objects":3})));
        assert!(!all_posted(&json!({"ok":true,"posted":2,"objects":3})));
        assert!(!all_posted(&json!({"ok":false,"posted":3,"objects":3})));
    }
}
