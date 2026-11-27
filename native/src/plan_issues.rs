//! The run monitor's open issues and its lessons, as the executor reads them (docs/MONITORING.md).
//!
//! The monitor (`observe.py watch RUN`, rules only, no model) writes `RUN/issues_open.json`: each
//! open issue with the executor lane that owns it. Lessons live in the learned store beside the
//! skills (`learned/lessons/<id>.json`). Both are plain files the executor re-reads only when
//! their modification time changes (a stat every few wall seconds, never a parse per tick).
//!
//! - Due work: a lane that owns an open issue is offered ahead of the routine pass, at most once
//!   per game day per lane (`scheduler.issues_due: false` turns it off). The lane decides what
//!   to do as always; an issue only moves its turn forward.
//! - Refusals: before the failure ledger rests a refused order, the lessons are asked about its
//!   (tool, cause). A transient cause rests briefly and is never given up; a permanent one is not
//!   sent unchanged again and the player is told at once with the lesson's advice; a wait rests
//!   the days the lesson names. A lesson is used only at confidence 0.5 or more, active, learned
//!   on a supported game version (or marked version-independent), and never overrides a safety
//!   check: it changes only how long the ledger rests an order.
//! - Issues no executor lane can act on (a plan decision, a tool fault, an escalated issue with
//!   its packet) join the player's wake list as omission rows.
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use super::GameHour;

/// Lessons below this confidence are evidence only: they appear in packets, not in the ledger.
pub const MIN_CONFIDENCE: f64 = 0.5;
/// How often the files are stat-ed (wall time).
const RECHECK: Duration = Duration::from_secs(5);
/// A transient refusal rests at most this long (game hours).
pub const TRANSIENT_REST: GameHour = 2;
/// Most rows the wake list takes from the monitor.
const MAX_ROWS: usize = 8;

/// A monitor lane name that is not an executor lane name. The monitor's "navy" is the navy
/// rule's lane (admirals, task forces, missions: plan_military's `navy`), not the sea-transfer
/// step `ship`: mapped there, the admiral and idle task force issues of t1939n (27 Nov 2026)
/// were offered to the sea transfers and stayed open for 158 days.
const LANE_ALIAS: [(&str, &str); 5] = [
    ("mio_lines", "mio"),
    ("cp_spend", "seams"),
    ("doctrines", "extras"),
    ("special_projects", "extras"),
    ("air", "air_plan"),
];

#[derive(Debug, Clone, PartialEq)]
pub enum Treat {
    Transient,
    Permanent,
    /// rest at least these game hours
    Wait(GameHour),
}

#[derive(Debug, Clone)]
pub struct Advice {
    pub id: String,
    pub treat: Treat,
    pub advice: String,
}

#[derive(Default)]
pub struct Issues {
    run_dir: Option<PathBuf>,
    store: Option<PathBuf>,
    checked: Option<Instant>,
    open_mtime: Option<SystemTime>,
    lessons_mtime: Option<SystemTime>,
    /// the open issues as the monitor last wrote them
    pub open: Vec<Value>,
    lessons: Vec<Value>,
    /// lane -> game day it was last offered for an issue
    offered: HashMap<String, GameHour>,
    /// the run folder's name: the monitor's lessons of this run carry `scope: run:<name>`
    run_name: Option<String>,
    /// (issue id) -> the last outcome line written, so a repeated answer is not written again
    answered: HashMap<String, String>,
}

fn mtime(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn sev_rank(v: &Value) -> u8 {
    match v["severity"].as_str() {
        Some("high") => 0,
        Some("medium") => 1,
        _ => 2,
    }
}

/// The executor lane a monitor lane names.
pub fn executor_lane(lane: &str) -> &str {
    LANE_ALIAS
        .iter()
        .find(|(from, _)| *from == lane)
        .map_or(lane, |(_, to)| to)
}

impl Issues {
    /// The run folder (the plan's folder); the store follows the learned-skills rule.
    pub fn set_run_dir(&mut self, dir: &Path) {
        if self.run_dir.as_deref() != Some(dir) {
            self.run_dir = Some(dir.to_path_buf());
            self.run_name = dir.file_name().and_then(|n| n.to_str()).map(str::to_string);
            self.store = Some(crate::learn::skills::store_dir(dir).join("lessons"));
            self.checked = None;
            self.open_mtime = None;
            self.lessons_mtime = None;
        }
    }

    /// Re-read what changed (at most every RECHECK wall seconds).
    pub fn refresh(&mut self) {
        if self.checked.is_some_and(|t| t.elapsed() < RECHECK) {
            return;
        }
        self.checked = Some(Instant::now());
        if let Some(dir) = &self.run_dir {
            let path = dir.join("issues_open.json");
            let m = mtime(&path);
            if m != self.open_mtime {
                self.open_mtime = m;
                self.open = read_json(&path)
                    .and_then(|v| v["issues"].as_array().cloned())
                    .unwrap_or_default();
            }
        }
        if let Some(store) = &self.store {
            let m = mtime(store);
            if m != self.lessons_mtime {
                self.lessons_mtime = m;
                self.lessons = load_lessons(store);
            }
        }
    }

    /// Executor lanes that own open issues, the most severe first (each once).
    pub fn due_lanes(&self) -> Vec<(String, String)> {
        let mut rows: Vec<&Value> = self
            .open
            .iter()
            .filter(|i| i["lane"].is_string() && i["state"] != json!("resolved"))
            .filter(|i| i["severity"] != json!("low"))
            .collect();
        rows.sort_by_key(|i| (i["overdue"] != json!(true), sev_rank(i)));
        let mut out: Vec<(String, String)> = Vec::new();
        for i in rows {
            let lane = executor_lane(i["lane"].as_str().unwrap_or_default()).to_string();
            if !out.iter().any(|(l, _)| *l == lane) {
                out.push((lane, i["id"].as_str().unwrap_or_default().to_string()));
            }
        }
        out
    }

    /// Offer `lane` for an issue now? Once per game day per lane.
    pub fn take_turn(&mut self, lane: &str, now: GameHour) -> bool {
        let day = now.div_euclid(24);
        if self.offered.get(lane) == Some(&day) {
            return false;
        }
        self.offered.insert(lane.to_string(), day);
        true
    }

    /// What the lessons say about a refusal of `tool` with `cause` (the most specific usable
    /// lesson, then the most confident).
    ///
    /// A lesson that names the order's arguments (`situation.args`, written by the monitor after
    /// the second identical refusal) holds only for an order with those arguments, and is the
    /// most specific kind.
    pub fn advice(&self, tool: &str, cause: &str, args: Option<&Value>) -> Option<Advice> {
        if cause.is_empty() {
            return None;
        }
        let mut best: Option<(u8, f64, Advice)> = None;
        for le in &self.lessons {
            let Some(treat) = usable_treat(le, self.run_name.as_deref()) else {
                continue;
            };
            let s = &le["situation"];
            let Some(c) = s["cause"].as_str() else {
                continue;
            };
            let by_args = s["args"].is_object();
            if by_args && !args_match(&s["args"], args) {
                continue;
            }
            // an empty cause matches every cause: only for one named order
            if !cause.starts_with(c) || (c.is_empty() && !by_args) {
                continue;
            }
            let lt = s["tool"].as_str().unwrap_or("*");
            if lt != "*" && lt != tool {
                continue;
            }
            let specific = u8::from(lt == tool) + 2 * u8::from(by_args);
            let conf = le["confidence"].as_f64().unwrap_or(0.0);
            if best
                .as_ref()
                .is_none_or(|(sp, cf, _)| (specific, conf) > (*sp, *cf))
            {
                let advice = Advice {
                    id: le["id"].as_str().unwrap_or_default().to_string(),
                    treat,
                    advice: le["executor"]["advice"]
                        .as_str()
                        .or(le["statement"].as_str())
                        .unwrap_or_default()
                        .to_string(),
                };
                best = Some((specific, conf, advice));
            }
        }
        best.map(|b| b.2)
    }

    /// Open issues no executor lane can act on (or escalated with a packet), as wake rows.
    pub fn wake_rows(&self, executor_lanes: &[&str], today: Option<GameHour>) -> Vec<Value> {
        let mut out = Vec::new();
        for i in &self.open {
            if i["severity"] == json!("low") || i["state"] == json!("resolved") {
                continue;
            }
            let lane = i["lane"].as_str().map(executor_lane);
            let owned = lane.is_some_and(|l| executor_lanes.contains(&l));
            let executor_own = i["actor"] == json!("executor");
            if (owned || executor_own) && i["packet"].is_null() {
                continue;
            }
            let first = i["first_seen"].as_str().and_then(super::date_hour);
            let days = match (first, today) {
                (Some(f), Some(t)) if t >= f => (t - f) / 24,
                _ => 0,
            };
            let mut todo = i["text"].as_str().unwrap_or_default().to_string();
            if let Some(adv) = i["lesson"]["advice"].as_str() {
                todo.push_str(&format!(" | lesson: {adv}"));
            }
            if let Some(p) = i["packet"].as_str() {
                todo.push_str(&format!(
                    " | packet: {}",
                    p.chars().take(600).collect::<String>()
                ));
            }
            out.push(json!({
                "item": format!("issue:{}", i["key"].as_str().or(i["code"].as_str()).unwrap_or("?")),
                "days_open": days,
                "todo": todo,
                "owner": "player",
                "owner_note": format!("run monitor {}: no executor lane acts on it ({})",
                    i["id"].as_str().unwrap_or("?"), i["lane"].as_str().unwrap_or("none")),
                "note_contradicts_read": false,
                "source": "monitor",
            }));
            if out.len() >= MAX_ROWS {
                break;
            }
        }
        out
    }
}

fn load_lessons(dir: &Path) -> Vec<Value> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<Value> = rd
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .filter_map(|p| read_json(&p))
        .filter(|v| v["executor"].is_object())
        .collect();
    out.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    out
}

/// The ledger effect of a lesson, if the executor may use it.
/// Every argument the lesson names that the order also carries is equal (nulls in the lesson are
/// left out), and at least one is compared.
fn args_match(want: &Value, args: Option<&Value>) -> bool {
    let (Some(want), Some(args)) = (want.as_object(), args.and_then(Value::as_object)) else {
        return false;
    };
    let mut compared = 0;
    for (k, v) in want {
        if v.is_null() {
            continue;
        }
        match args.get(k) {
            Some(a) if a == v => compared += 1,
            Some(_) => return false,
            None => {}
        }
    }
    compared > 0
}

impl Issues {
    /// Record what an offered lane did with the open issues it owns (docs/MONITORING.md): the
    /// monitor reads `RUN/issue_outcomes.jsonl` and keeps the answer on the issue, so "acted"
    /// means an order passed and a lane that waits says what for. A repeated answer is not
    /// written again.
    pub fn record_outcome(&mut self, lane: &str, outcome: &str, reason: &str, now: GameHour) {
        let Some(dir) = self.run_dir.clone() else {
            return;
        };
        let ids: Vec<String> = self
            .open
            .iter()
            .filter(|i| i["state"] != json!("resolved"))
            .filter(|i| i["lane"].as_str().map(executor_lane) == Some(lane))
            .filter_map(|i| i["id"].as_str().map(str::to_string))
            .collect();
        let reason: String = reason.chars().take(400).collect();
        let mut lines = String::new();
        for id in ids {
            let said = format!("{outcome}|{reason}");
            if self.answered.get(&id) == Some(&said) {
                continue;
            }
            self.answered.insert(id.clone(), said);
            lines.push_str(
                &json!({"id": id, "lane": lane, "outcome": outcome,
                                   "reason": reason, "hour": now})
                .to_string(),
            );
            lines.push('\n');
        }
        if lines.is_empty() {
            return;
        }
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("issue_outcomes.jsonl"))
        {
            let _ = f.write_all(lines.as_bytes());
        }
    }
}

fn usable_treat(le: &Value, run: Option<&str>) -> Option<Treat> {
    if le["status"].as_str().unwrap_or("active") != "active" {
        return None;
    }
    if le["confidence"].as_f64().unwrap_or(0.0) < MIN_CONFIDENCE {
        return None;
    }
    let scope = le["scope"].as_str().unwrap_or("any");
    let own_run = scope
        .strip_prefix("run:")
        .is_some_and(|r| run.is_some_and(|name| name == r));
    if scope != "any" && !scope.starts_with("country:") && !scope.starts_with("plan:") && !own_run {
        return None; // another run's lesson is evidence for review, not a rule for this run
    }
    let version = le["learned_on"]["game_version"].as_str().unwrap_or("");
    if !version.is_empty()
        && le["version_independent"] != json!(true)
        && !crate::builds::current().map_or_else(
            || {
                crate::builds::SUPPORTED
                    .iter()
                    .any(|b| b.game_version == version)
            },
            |b| b.game_version == version,
        )
    {
        return None;
    }
    match le["executor"]["treat_as"].as_str()? {
        "transient" => Some(Treat::Transient),
        "permanent" => Some(Treat::Permanent),
        "wait" => Some(Treat::Wait(
            (le["executor"]["days"].as_i64().unwrap_or(10)).clamp(1, 120) * 24,
        )),
        _ => None,
    }
}

#[cfg(test)]
impl Issues {
    /// A store with these lessons for run `run`, not re-read from disk for a while.
    pub fn with_lessons(lessons: Vec<Value>, run: &str) -> Issues {
        Issues {
            lessons,
            run_name: Some(run.to_string()),
            checked: Some(Instant::now()),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lesson(id: &str, tool: &str, cause: &str, treat: &str, conf: f64) -> Value {
        // Learned on the build this binary reads here (macOS and Linux may be on different versions).
        let version = crate::builds::current().map_or("1.19.4.1", |b| b.game_version);
        json!({"id": id, "statement": "s", "situation": {"tool": tool, "cause": cause},
               "scope": "any", "confidence": conf, "status": "active",
               "learned_on": {"game_version": version, "mod_set": "plain"},
               "executor": {"treat_as": treat, "advice": format!("advice {id}"), "days": 30}})
    }

    fn with(lessons: Vec<Value>) -> Issues {
        Issues {
            lessons,
            ..Default::default()
        }
    }

    #[test]
    fn the_most_specific_confident_lesson_answers_a_refusal() {
        let is = with(vec![
            lesson("any_events", "*", "events_open", "transient", 0.8),
            lesson(
                "rebase_full",
                "hoi4_air_rebase",
                "air_base_full",
                "permanent",
                0.6,
            ),
            lesson("weak", "hoi4_air_rebase", "events_open", "permanent", 0.3),
        ]);
        let a = is
            .advice("hoi4_air_mission", "events_open: an event is open", None)
            .unwrap();
        assert_eq!((a.id.as_str(), a.treat), ("any_events", Treat::Transient));
        // the weak specific lesson is below the bar: the general one still answers
        assert_eq!(
            is.advice("hoi4_air_rebase", "events_open", None)
                .unwrap()
                .id,
            "any_events"
        );
        assert_eq!(
            is.advice("hoi4_air_rebase", "air_base_full", None)
                .unwrap()
                .treat,
            Treat::Permanent
        );
        assert!(is.advice("hoi4_air_rebase", "", None).is_none());
        assert!(is.advice("hoi4_save", "invalid_gui", None).is_none());
    }

    #[test]
    fn lessons_of_another_version_a_run_scope_or_retired_are_not_used() {
        let mut old = lesson("old", "*", "events_open", "transient", 0.9);
        old["learned_on"]["game_version"] = json!("1.12.0");
        let mut run = lesson("run", "*", "events_open", "transient", 0.9);
        run["scope"] = json!("run:g17n");
        let mut retired = lesson("ret", "*", "events_open", "transient", 0.9);
        retired["status"] = json!("retired");
        assert!(with(vec![old.clone(), run, retired])
            .advice("hoi4_x", "events_open", None)
            .is_none());
        old["version_independent"] = json!(true);
        assert!(with(vec![old])
            .advice("hoi4_x", "events_open", None)
            .is_some());
    }

    #[test]
    fn a_runs_own_repeat_lesson_holds_for_that_order_in_that_run_only() {
        let mut le = lesson(
            "mio",
            "hoi4_mio_policy",
            "input_readback_failed",
            "wait",
            0.5,
        );
        le["scope"] = json!("run:p65a");
        le["situation"]["args"] = json!({"organization": "O", "policy": "P", "line": null});
        let mut is = with(vec![le]);
        let same = json!({"organization": "O", "policy": "P"});
        let other = json!({"organization": "O", "policy": "Q"});
        // another run: evidence only
        assert!(is
            .advice("hoi4_mio_policy", "input_readback_failed", Some(&same))
            .is_none());
        is.run_name = Some("p65a".into());
        assert!(is
            .advice("hoi4_mio_policy", "input_readback_failed: x", Some(&same))
            .is_some());
        assert!(is
            .advice("hoi4_mio_policy", "input_readback_failed", Some(&other))
            .is_none());
        assert!(is
            .advice("hoi4_mio_policy", "input_readback_failed", None)
            .is_none());
    }

    #[test]
    fn an_offered_lanes_answer_is_written_once_per_issue() {
        let dir = std::env::temp_dir().join(format!("issues-outcome-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut is = Issues::default();
        is.set_run_dir(&dir);
        is.open = vec![
            json!({"id": "I1", "lane": "navy", "state": "overdue"}),
            json!({"id": "I2", "lane": "politics", "state": "open"}),
        ];
        is.record_outcome("navy", "waiting", "navy: region North Sea not found", 24);
        is.record_outcome("navy", "waiting", "navy: region North Sea not found", 48);
        let text = std::fs::read_to_string(dir.join("issue_outcomes.jsonl")).unwrap();
        assert_eq!(text.lines().count(), 1);
        assert!(text.contains("\"I1\"") && text.contains("waiting"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_wait_lesson_rests_its_days() {
        let is = with(vec![lesson(
            "w",
            "hoi4_production_add",
            "invalid_target",
            "wait",
            0.6,
        )]);
        assert_eq!(
            is.advice("hoi4_production_add", "invalid_target", None)
                .unwrap()
                .treat,
            Treat::Wait(30 * 24)
        );
    }

    #[test]
    fn due_lanes_put_overdue_and_severe_first_and_each_lane_is_offered_once_a_day() {
        let mut is = Issues {
            open: vec![
                json!({"id": "I1", "lane": "politics", "severity": "medium", "state": "open"}),
                json!({"id": "I2", "lane": "dockyards", "severity": "medium", "state": "open", "overdue": true}),
                json!({"id": "I3", "lane": "navy", "severity": "high", "state": "open"}),
                json!({"id": "I4", "lane": "navy", "severity": "high", "state": "open"}),
                json!({"id": "I5", "lane": "seams", "severity": "low", "state": "open"}),
            ],
            ..Default::default()
        };
        let lanes: Vec<String> = is.due_lanes().into_iter().map(|(l, _)| l).collect();
        assert_eq!(lanes, ["dockyards", "navy", "politics"]);
        assert!(is.take_turn("dockyards", 100));
        assert!(!is.take_turn("dockyards", 110));
        assert!(is.take_turn("dockyards", 130));
    }

    #[test]
    fn issues_no_lane_acts_on_join_the_wake_list() {
        let is = Issues {
            open: vec![
                json!({"id": "I1", "key": "checkpoint_missed:cp", "lane": "gate", "severity": "high",
                       "state": "open", "text": "checkpoint missed", "first_seen": "1940.05.01.00"}),
                json!({"id": "I2", "key": "dockyards_unassigned", "lane": "dockyards", "severity": "medium",
                       "state": "open", "text": "idle"}),
                json!({"id": "I3", "key": "province_overstacked:1", "lane": "war", "severity": "high",
                       "state": "open", "text": "63 divisions", "packet": "ESCALATION ...",
                       "lesson": {"advice": "widen the front"}}),
                json!({"id": "I4", "key": "refusal", "lane": "plan", "severity": "high", "actor": "executor",
                       "state": "open", "text": "x"}),
            ],
            ..Default::default()
        };
        let today = super::super::date_hour("1940.05.11.00");
        let rows = is.wake_rows(&["dockyards", "war"], today);
        let items: Vec<&str> = rows.iter().map(|r| r["item"].as_str().unwrap()).collect();
        assert_eq!(
            items,
            ["issue:checkpoint_missed:cp", "issue:province_overstacked:1"]
        );
        assert_eq!(rows[0]["days_open"], json!(10));
        assert!(rows[1]["todo"]
            .as_str()
            .unwrap()
            .contains("lesson: widen the front"));
        assert!(!rows[0]["owner_note"].is_null());
    }

    #[test]
    fn files_are_read_from_the_run_folder_and_the_store() {
        let root = std::env::temp_dir().join(format!("plan_issues_{}", std::process::id()));
        let run = root.join("run1");
        std::fs::create_dir_all(root.join("learned/lessons")).unwrap();
        std::fs::create_dir_all(&run).unwrap();
        std::fs::write(
            run.join("issues_open.json"),
            r#"{"issues":[{"id":"I1","lane":"dockyards","severity":"high","state":"open"}]}"#,
        )
        .unwrap();
        std::fs::write(
            root.join("learned/lessons/a.json"),
            lesson("a", "*", "events_open", "transient", 0.7).to_string(),
        )
        .unwrap();
        let mut is = Issues::default();
        is.set_run_dir(&run);
        // the store as `store_dir` finds it beside the run (whatever HOI4_LIVE_LEARNED says here)
        is.store = Some(root.join("learned/lessons"));
        is.refresh();
        assert_eq!(is.open.len(), 1);
        assert!(is.advice("hoi4_save", "events_open", None).is_some());
        std::fs::remove_dir_all(&root).ok();
    }
}
