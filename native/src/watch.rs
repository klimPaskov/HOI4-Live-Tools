//! Change feed for real-time play: a watcher samples a compact view of the player's own
//! situation (the same reads the read tools serve) a few times a second, and every difference
//! between two samples becomes a typed change with a sequence number. Agents ask for the changes
//! since the last sequence they saw (`hoi4_observe`) or block until one arrives
//! (`hoi4_wait_change`), instead of re-reading whole structures to find out what happened.
//!
//! Only what the player's own screens show is used (own armies, own and adjacent state control,
//! open events, the alert row, idle slots). Nothing here sends input.
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Changes kept for late readers; older ones are dropped (the reply says how many were missed).
const KEEP: usize = 1024;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Army {
    pub divisions: i64,
    pub state: Option<i64>,
    pub executing: bool,
    pub organisation: Option<f64>,
    pub supply: Option<f64>,
}

/// The compact view. A field read failed this sample keeps its previous value (`merge`), so a
/// torn read never shows up as a change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Compact {
    pub hour: Option<i64>,
    pub date: Option<String>,
    pub paused: Option<bool>,
    pub events: Option<BTreeSet<String>>,
    pub peace_open: Option<bool>,
    pub armies: Option<BTreeMap<String, Army>>,
    pub hostile: Option<BTreeSet<String>>,
    pub occupied_own: Option<BTreeSet<i64>>,
    pub enemy_adjacent: Option<BTreeSet<i64>>,
    pub alerts: Option<BTreeSet<String>>,
    pub idle_research: Option<usize>,
    pub focus: Option<Option<String>>,
    pub civ_idle: Option<i64>,
}

fn state_ids(v: &Value) -> BTreeSet<i64> {
    v.as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_i64().or_else(|| x["state_id"].as_i64()))
                .collect()
        })
        .unwrap_or_default()
}

/// Reads by tool name; a missing or failed read is `None`.
#[derive(Default)]
pub struct Reads {
    pub overview: Option<Value>,
    pub events: Option<Value>,
    pub armies: Option<Value>,
    pub supply: Option<Value>,
    pub wars: Option<Value>,
    pub alerts: Option<Value>,
    pub research: Option<Value>,
    pub focus: Option<Value>,
    pub economy: Option<Value>,
}

impl Compact {
    pub fn from_reads(r: &Reads) -> Self {
        let mut c = Compact::default();
        if let Some(o) = &r.overview {
            c.hour = o["date"]["raw"].as_i64();
            c.date = o["date"]["text"].as_str().map(str::to_string);
            c.paused = o["paused"].as_bool();
        }
        if let Some(e) = &r.events {
            c.events = e["events"].as_array().map(|a| {
                a.iter()
                    .filter_map(|x| x["event"].as_str().map(str::to_string))
                    .collect()
            });
            c.peace_open = e["peace_conference_open"].as_bool();
        }
        if let Some(a) = &r.armies {
            if let Some(list) = a["armies"].as_array() {
                let condition: BTreeMap<String, (Option<f64>, Option<f64>)> = r
                    .supply
                    .as_ref()
                    .and_then(|s| s["armies"].as_array())
                    .map(|rows| {
                        rows.iter()
                            .filter_map(|x| {
                                Some((
                                    x["army"].as_str()?.to_string(),
                                    (
                                        x["organisation_mean"].as_f64(),
                                        x["supply_percent_mean"].as_f64(),
                                    ),
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                c.armies = Some(
                    list.iter()
                        .filter_map(|x| {
                            let name = x["army"].as_str()?.to_string();
                            let (organisation, supply) =
                                condition.get(&name).copied().unwrap_or((None, None));
                            Some((
                                name,
                                Army {
                                    divisions: x["division_count"].as_i64().unwrap_or(0),
                                    state: x["main_state"].as_i64(),
                                    executing: x["plan_executing"] == json!(true),
                                    organisation,
                                    supply,
                                },
                            ))
                        })
                        .collect(),
                );
            }
        }
        if let Some(w) = &r.wars {
            c.hostile = w["hostile_tags"].as_array().map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().or_else(|| x["tag"].as_str()).map(str::to_string))
                    .collect()
            });
            c.occupied_own = Some(state_ids(&w["occupied_own_states"]));
            c.enemy_adjacent = Some(state_ids(&w["enemy_held_adjacent_states"]));
        }
        if let Some(a) = &r.alerts {
            c.alerts = a["alerts"].as_array().map(|list| {
                list.iter()
                    .filter_map(|x| {
                        x["alert"]
                            .as_str()
                            .map(str::to_string)
                            .or_else(|| x["frame"].as_i64().map(|f| format!("frame_{f}")))
                    })
                    .collect()
            });
        }
        if let Some(rs) = &r.research {
            c.idle_research = rs["slots"]
                .as_array()
                .map(|s| s.iter().filter(|x| x["researching"].is_null()).count());
        }
        if let Some(f) = &r.focus {
            c.focus = Some(f["active_key"].as_str().map(str::to_string));
        }
        if let Some(e) = &r.economy {
            c.civ_idle = e["civilian_factories_idle"].as_i64();
        }
        c
    }
    /// Fields this sample could not read keep the previous sample's value.
    pub fn merge(self, prev: &Compact) -> Compact {
        macro_rules! keep {
            ($($f:ident),*) => { Compact { $($f: self.$f.or_else(|| prev.$f.clone()),)* } };
        }
        keep!(
            hour,
            date,
            paused,
            events,
            peace_open,
            armies,
            hostile,
            occupied_own,
            enemy_adjacent,
            alerts,
            idle_research,
            focus,
            civ_idle
        )
    }
    pub fn brief(&self) -> Value {
        json!({"date": self.date, "paused": self.paused,
            "open_events": self.events.as_ref().map(|e| e.iter().collect::<Vec<_>>()),
            "at_war_with": self.hostile.as_ref().map(|h| h.iter().collect::<Vec<_>>()),
            "own_states_occupied": self.occupied_own.as_ref().map(|s| s.iter().collect::<Vec<_>>()),
            "armies": self.armies.as_ref().map(|a| a.len()),
            "idle_research_slots": self.idle_research,
            "focus": self.focus,
            "civilian_factories_idle": self.civ_idle})
    }
}

/// Thresholds below which an army's condition is reported (absolute organisation, supply %).
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub organisation: f64,
    pub supply: f64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            organisation: 20.0,
            supply: 30.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub kind: &'static str,
    pub urgent: bool,
    pub detail: Value,
}
fn change(kind: &'static str, urgent: bool, detail: Value) -> Change {
    Change {
        kind,
        urgent,
        detail,
    }
}

/// What differs between two samples. The first sample (no previous one) reports nothing.
pub fn diff(prev: &Compact, cur: &Compact, limits: Limits) -> Vec<Change> {
    let mut out = Vec::new();
    if let (Some(a), Some(b)) = (prev.paused, cur.paused) {
        if a != b {
            out.push(change("paused_changed", true, json!({"paused": b})));
        }
    }
    if let (Some(a), Some(b)) = (&prev.events, &cur.events) {
        for e in b.difference(a) {
            out.push(change("event_opened", true, json!({"event": e})));
        }
        for e in a.difference(b) {
            out.push(change("event_closed", false, json!({"event": e})));
        }
    }
    if let (Some(false), Some(true)) = (prev.peace_open, cur.peace_open) {
        out.push(change("peace_conference_opened", true, json!({})));
    }
    if let (Some(a), Some(b)) = (&prev.hostile, &cur.hostile) {
        for t in b.difference(a) {
            out.push(change("war_started", true, json!({"with": t})));
        }
        for t in a.difference(b) {
            out.push(change("war_ended", true, json!({"with": t})));
        }
    }
    if let (Some(a), Some(b)) = (&prev.occupied_own, &cur.occupied_own) {
        for s in b.difference(a) {
            out.push(change("own_state_lost", true, json!({"state_id": s})));
        }
        for s in a.difference(b) {
            out.push(change("own_state_regained", false, json!({"state_id": s})));
        }
    }
    if let (Some(a), Some(b)) = (&prev.enemy_adjacent, &cur.enemy_adjacent) {
        for s in a.difference(b) {
            out.push(change(
                "enemy_state_taken_or_left",
                false,
                json!({"state_id": s}),
            ));
        }
        for s in b.difference(a) {
            out.push(change(
                "enemy_state_now_adjacent",
                false,
                json!({"state_id": s}),
            ));
        }
    }
    if let (Some(a), Some(b)) = (&prev.armies, &cur.armies) {
        for (name, now) in b {
            let Some(was) = a.get(name) else {
                out.push(change(
                    "army_created",
                    false,
                    json!({"army": name, "divisions": now.divisions}),
                ));
                continue;
            };
            if was.state != now.state {
                out.push(change(
                    "army_moved",
                    false,
                    json!({"army": name, "from_state": was.state, "to_state": now.state}),
                ));
            }
            if was.divisions != now.divisions {
                out.push(change(
                    "army_size_changed",
                    now.divisions < was.divisions,
                    json!({"army": name, "from": was.divisions, "to": now.divisions}),
                ));
            }
            if was.executing && !now.executing {
                out.push(change("army_plan_stopped", false, json!({"army": name})));
            }
            let crossed = |w: Option<f64>, n: Option<f64>, limit: f64| matches!((w, n), (Some(w), Some(n)) if w >= limit && n < limit);
            if crossed(was.organisation, now.organisation, limits.organisation) {
                out.push(change(
                    "army_organisation_low",
                    true,
                    json!({"army": name, "organisation": now.organisation}),
                ));
            }
            if crossed(was.supply, now.supply, limits.supply) {
                out.push(change(
                    "army_supply_low",
                    true,
                    json!({"army": name, "supply_percent": now.supply}),
                ));
            }
        }
        for name in a.keys().filter(|n| !b.contains_key(*n)) {
            out.push(change("army_gone", true, json!({"army": name})));
        }
    }
    if let (Some(a), Some(b)) = (&prev.alerts, &cur.alerts) {
        for x in b.difference(a) {
            out.push(change("alert_raised", false, json!({"alert": x})));
        }
        for x in a.difference(b) {
            out.push(change("alert_cleared", false, json!({"alert": x})));
        }
    }
    if let (Some(a), Some(b)) = (prev.idle_research, cur.idle_research) {
        if b > a {
            out.push(change(
                "research_slot_idle",
                false,
                json!({"idle_slots": b}),
            ));
        }
    }
    if let (Some(Some(f)), Some(None)) = (&prev.focus, &cur.focus) {
        out.push(change("focus_finished", false, json!({"focus": f})));
    }
    if let (Some(a), Some(b)) = (prev.civ_idle, cur.civ_idle) {
        if a == 0 && b > 0 {
            out.push(change("civilian_factories_idle", false, json!({"idle": b})));
        }
    }
    out
}

struct Entry {
    seq: u64,
    hour: Option<i64>,
    date: Option<String>,
    change: Change,
}

/// The change log shared by the watcher and the observe/wait tools.
pub struct Feed {
    inner: Mutex<FeedState>,
    changed: Condvar,
}
struct FeedState {
    entries: VecDeque<Entry>,
    next: u64,
    last: Option<Compact>,
    samples: u64,
    sampled_at: Option<Instant>,
}
impl Default for Feed {
    fn default() -> Self {
        Self {
            inner: Mutex::new(FeedState {
                entries: VecDeque::new(),
                next: 1,
                last: None,
                samples: 0,
                sampled_at: None,
            }),
            changed: Condvar::new(),
        }
    }
}

/// The process-wide feed (one owned game per server).
pub fn feed() -> &'static Feed {
    static FEED: OnceLock<Feed> = OnceLock::new();
    FEED.get_or_init(Feed::default)
}

impl Feed {
    /// Add one sample; returns how many changes it produced.
    pub fn push(&self, sample: Compact, limits: Limits) -> usize {
        let Ok(mut s) = self.inner.lock() else {
            return 0;
        };
        let merged = match &s.last {
            Some(prev) => sample.merge(prev),
            None => sample,
        };
        let changes = match &s.last {
            Some(prev) => diff(prev, &merged, limits),
            None => Vec::new(),
        };
        let n = changes.len();
        for c in changes {
            let seq = s.next;
            s.next += 1;
            let (hour, date) = (merged.hour, merged.date.clone());
            s.entries.push_back(Entry {
                seq,
                hour,
                date,
                change: c,
            });
            if s.entries.len() > KEEP {
                s.entries.pop_front();
            }
        }
        s.last = Some(merged);
        s.samples += 1;
        s.sampled_at = Some(Instant::now());
        drop(s);
        if n > 0 {
            self.changed.notify_all();
        }
        n
    }

    fn reply(
        s: &FeedState,
        since: u64,
        kinds: &[String],
        urgent_only: bool,
        limit: usize,
    ) -> Value {
        let oldest = s.entries.front().map_or(s.next, |e| e.seq);
        let wanted = |e: &&Entry| {
            e.seq > since
                && (kinds.is_empty() || kinds.iter().any(|k| k == e.change.kind))
                && (!urgent_only || e.change.urgent)
        };
        let rows: Vec<Value> = s
            .entries
            .iter()
            .filter(wanted)
            .take(limit)
            .map(|e| {
                json!({"seq": e.seq, "kind": e.change.kind, "urgent": e.change.urgent,
                    "date": e.date, "game_hour": e.hour, "detail": e.change.detail})
            })
            .collect();
        let more = s
            .entries
            .iter()
            .filter(wanted)
            .count()
            .saturating_sub(rows.len());
        json!({
            "seq": s.next - 1,
            "changes": rows,
            "more": more,
            "missed": since + 1 < oldest && since > 0,
            "now": s.last.as_ref().map(Compact::brief),
            "watcher": {"samples": s.samples,
                "last_sample_ms_ago": s.sampled_at.map(|t| t.elapsed().as_millis() as u64)},
        })
    }

    /// Changes after `since` (0: none, just the current brief and sequence).
    pub fn observe(&self, since: u64, kinds: &[String], urgent_only: bool, limit: usize) -> Value {
        match self.inner.lock() {
            Ok(s) => Self::reply(&s, since, kinds, urgent_only, limit),
            Err(_) => json!({"error": "feed unavailable"}),
        }
    }

    /// Block until a matching change after `since` exists or `timeout` passes.
    pub fn wait(
        &self,
        since: u64,
        kinds: &[String],
        urgent_only: bool,
        timeout: Duration,
        limit: usize,
    ) -> Value {
        let deadline = Instant::now() + timeout;
        let Ok(mut s) = self.inner.lock() else {
            return json!({"error": "feed unavailable"});
        };
        loop {
            let found = s.entries.iter().any(|e| {
                e.seq > since
                    && (kinds.is_empty() || kinds.iter().any(|k| k == e.change.kind))
                    && (!urgent_only || e.change.urgent)
            });
            let now = Instant::now();
            if found || now >= deadline {
                let mut v = Self::reply(&s, since, kinds, urgent_only, limit);
                v["woke_because"] = json!(if found { "change" } else { "timeout" });
                return v;
            }
            match self.changed.wait_timeout(s, deadline - now) {
                Ok((guard, _)) => s = guard,
                Err(_) => return json!({"error": "feed unavailable"}),
            }
        }
    }

    pub fn running(&self) -> bool {
        self.inner.lock().map(|s| s.samples > 0).unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample(events: &[&str], occupied: &[i64], org: f64, state: i64) -> Compact {
        let mut armies = BTreeMap::new();
        armies.insert(
            "Army 1".to_string(),
            Army {
                divisions: 10,
                state: Some(state),
                executing: true,
                organisation: Some(org),
                supply: Some(80.0),
            },
        );
        Compact {
            hour: Some(100),
            date: Some("1936.3.1".into()),
            paused: Some(false),
            events: Some(events.iter().map(|s| s.to_string()).collect()),
            peace_open: Some(false),
            armies: Some(armies),
            hostile: Some(["ETH".to_string()].into()),
            occupied_own: Some(occupied.iter().copied().collect()),
            enemy_adjacent: Some(BTreeSet::new()),
            alerts: Some(BTreeSet::new()),
            idle_research: Some(0),
            focus: Some(Some("f".into())),
            civ_idle: Some(0),
        }
    }
    #[test]
    fn differences_become_typed_changes_and_urgent_ones_are_marked() {
        let a = sample(&[], &[], 40.0, 842);
        let b = sample(&["news.1"], &[559], 15.0, 843);
        let kinds: Vec<(&str, bool)> = diff(&a, &b, Limits::default())
            .iter()
            .map(|c| (c.kind, c.urgent))
            .collect();
        assert!(kinds.contains(&("event_opened", true)));
        assert!(kinds.contains(&("own_state_lost", true)));
        assert!(kinds.contains(&("army_moved", false)));
        assert!(kinds.contains(&("army_organisation_low", true)));
        assert!(diff(&a, &a, Limits::default()).is_empty());
    }
    #[test]
    fn an_unreadable_field_keeps_its_value_and_makes_no_change() {
        let a = sample(&["news.1"], &[], 40.0, 842);
        let mut b = sample(&[], &[], 40.0, 842);
        b.events = None;
        let merged = b.merge(&a);
        assert!(diff(&a, &merged, Limits::default()).is_empty());
    }
    #[test]
    fn observe_returns_changes_after_a_sequence_and_wait_wakes_on_a_change() {
        let feed = Feed::default();
        feed.push(sample(&[], &[], 40.0, 842), Limits::default());
        assert_eq!(
            feed.observe(0, &[], false, 50)["changes"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        feed.push(sample(&["news.1"], &[], 40.0, 842), Limits::default());
        let v = feed.observe(0, &[], false, 50);
        assert_eq!(v["changes"][0]["kind"], "event_opened");
        let seq = v["seq"].as_u64().unwrap();
        assert_eq!(
            feed.observe(seq, &[], false, 50)["changes"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        std::thread::scope(|s| {
            let waiter = s.spawn(|| feed.wait(seq, &[], true, Duration::from_secs(5), 50));
            std::thread::sleep(Duration::from_millis(50));
            feed.push(sample(&["news.1"], &[559], 40.0, 842), Limits::default());
            let v = waiter.join().unwrap();
            assert_eq!(v["woke_because"], "change");
            assert_eq!(v["changes"][0]["kind"], "own_state_lost");
        });
        let t = Instant::now();
        let v = feed.wait(
            feed.observe(0, &[], false, 1)["seq"].as_u64().unwrap(),
            &[],
            false,
            Duration::from_millis(100),
            10,
        );
        assert_eq!(v["woke_because"], "timeout");
        assert!(t.elapsed() >= Duration::from_millis(100));
    }
}
