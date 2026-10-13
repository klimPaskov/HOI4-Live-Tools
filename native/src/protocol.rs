use crate::{
    calendar,
    error::{Error, Result},
    game, gui,
    platform::Process,
    research, research_ui, technology,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
pub struct Server {
    pub process: Option<Box<dyn Process + Send + Sync>>,
    pub unavailable: Option<Error>,
}
/// Only an owned supervisor supplies a verified ordinary-input implementation.
/// Standalone readers and unavailable platform adapters expose observations.
pub trait Commands: Send + Sync {
    fn tools(&self) -> Vec<Value>;
    fn supports(&self, name: &str) -> bool;
    fn call(&self, process: &dyn Process, name: &str, args: &Value) -> Result<Value>;
    fn capabilities(&self) -> Value;
}
fn tools(commands: Option<&dyn Commands>) -> Value {
    let mut value = json!({"tools":[
        {"name":"hoi4_capabilities","description":"Verified runtime capabilities and explicit platform/build limitations.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"hoi4_overview","description":"Fresh owned-country process snapshot. Pure bounded reads; never pauses.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"hoi4_research","description":"Fresh own completed technology IDs with bounded pagination, and active raw research counters. Progress units, days remaining and eligibility remain unverified. Pure reads; never pauses.","inputSchema":{"type":"object","properties":{"offset":{"type":"integer","minimum":0,"maximum":8192},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}},
        {"name":"hoi4_input_geometry","description":"Read-only own GUI pointer, dimensions and scale diagnostics. Does not prove client origin, control geometry or click authorization.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"hoi4_research_ui","description":"Bounded own research slot widgets, selected slot and primary button/render type diagnostics. No hit routing or input authorization.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"hoi4_technology_list","description":"Canonical loaded public technology IDs with bounded filtering/pagination. Catalog membership does not imply player research eligibility. Pure reads; never pauses.","inputSchema":{"type":"object","properties":{"filter":{"type":"string","maxLength":128},"offset":{"type":"integer","minimum":0,"maximum":8192},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}},
        {"name":"hoi4_wait_for","description":"Bounded server wait for own focus/research change, game date, or research UI view predicates and optional selected own slot. UI matches do not authorize input. Never pauses.","inputSchema":{"type":"object","properties":{"timeout_seconds":{"type":"integer","minimum":1,"maximum":50},"until_date":{"type":"array","items":{"type":"integer"},"minItems":3,"maxItems":3},"until_research_ui":{"type":"object","properties":{"view":{"type":"string","enum":["panel","tree"]},"selected_slot":{"type":"integer","minimum":0,"maximum":31}},"required":["view"],"additionalProperties":false}},"additionalProperties":false}}
    ]});
    if let Some(commands) = commands {
        value["tools"]
            .as_array_mut()
            .unwrap()
            .extend(commands.tools());
    }
    value
}
fn content(value: Value) -> Value {
    json!({"content":[{"type":"text","text":serde_json::to_string(&value).unwrap()}]})
}
fn page_bounds(object: &serde_json::Map<String, Value>) -> Result<(usize, usize)> {
    let integer = |key: &str, default: u64, low: u64, high: u64| -> Result<usize> {
        let value = object
            .get(key)
            .map(|v| {
                v.as_u64()
                    .ok_or_else(|| Error::new("invalid_arguments", "Pagination must be integers"))
            })
            .transpose()?
            .unwrap_or(default);
        if !(low..=high).contains(&value) {
            return Err(Error::new("invalid_arguments", "Pagination exceeds bounds"));
        }
        Ok(value as usize)
    };
    Ok((
        integer("offset", 0, 0, 8192)?,
        integer("limit", 100, 1, 100)?,
    ))
}
fn fresh_wait_read<T, F>(
    start: Instant,
    timeout: Duration,
    rejected: &mut u64,
    mut read: F,
) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    loop {
        if start.elapsed() >= timeout {
            return Err(Error::new(
                "unstable_observation",
                "Fresh observation deadline reached; no further read or cached result",
            ));
        }
        match read() {
            Err(e) if e.code == "torn_read" => {
                *rejected += 1;
                let remaining = timeout.saturating_sub(start.elapsed());
                if remaining.is_zero() {
                    return Err(Error::new(
                        "unstable_observation",
                        "No stable snapshot within wait deadline; no stale result returned",
                    ));
                }
                std::thread::sleep(remaining.min(Duration::from_millis(250)));
            }
            result => return result,
        }
    }
}
fn same_wait_context(
    before: &game::ClockContext,
    current: &game::ClockContext,
    last_date: i32,
) -> Result<()> {
    if before.player != current.player
        || before.playthrough_id != current.playthrough_id
        || before.anchors != current.anchors
        || before.difficulty != current.difficulty
        || before.historical_raw != current.historical_raw
        || current.date_raw < last_date
    {
        return Err(Error::new(
            "wait_context_changed",
            "Own campaign/player/anchors/settings/timeline changed during wait",
        ));
    }
    Ok(())
}
struct UiTarget {
    tree: bool,
    selected_slot: Option<u64>,
}
impl UiTarget {
    fn parse(value: &Value) -> Result<Self> {
        let object = value.as_object().ok_or_else(|| {
            Error::new(
                "invalid_arguments",
                "Research UI wait target must be an object",
            )
        })?;
        if object
            .keys()
            .any(|k| !["view", "selected_slot"].contains(&k.as_str()))
        {
            return Err(Error::new(
                "invalid_arguments",
                "Unknown research UI wait field",
            ));
        }
        let tree = match object.get("view").and_then(Value::as_str) {
            Some("panel") => false,
            Some("tree") => true,
            _ => {
                return Err(Error::new(
                    "invalid_arguments",
                    "Research UI view must be panel or tree",
                ))
            }
        };
        let selected_slot = object
            .get("selected_slot")
            .map(|v| {
                v.as_u64().filter(|n| *n < 32).ok_or_else(|| {
                    Error::new(
                        "invalid_arguments",
                        "Selected own slot must be an integer 0..31",
                    )
                })
            })
            .transpose()?;
        Ok(Self {
            tree,
            selected_slot,
        })
    }
    fn reached(&self, value: &Value) -> bool {
        let (visible, hidden, mode) = if self.tree {
            ("ordinary_tree", "panel", 6)
        } else {
            ("panel", "ordinary_tree", 5)
        };
        value["active_mode_raw"] == mode
            && value[visible]["materialized"] == true
            && value[visible]["visible_predicate"] == true
            && (value[hidden]["materialized"] == false
                || value[hidden]["visible_predicate"] == false)
            && self
                .selected_slot
                .is_none_or(|v| value["selected_slot"].as_u64() == Some(v))
    }
}
fn same_ui_context(
    before: &research_ui::UiContext,
    current: &research_ui::UiContext,
    last_date: i32,
) -> Result<()> {
    same_wait_context(&before.campaign, &current.campaign, last_date)?;
    if before.anchors != current.anchors || before.slots != current.slots {
        return Err(Error::new(
            "wait_context_changed",
            "Own research UI or slot identities changed during wait",
        ));
    }
    Ok(())
}
impl Server {
    fn process(&self) -> Result<&dyn Process> {
        self.process
            .as_ref()
            .map(|p| p.as_ref() as &dyn Process)
            .ok_or_else(|| {
                Error::new(
                    "capability_unavailable",
                    self.unavailable
                        .as_ref()
                        .map(ToString::to_string)
                        .unwrap_or_else(|| "No owned session manifest; use --session".into()),
                )
            })
    }
    fn overview(&self) -> Result<Value> {
        game::snapshot(self.process()?)
    }
    fn wait_ui(&self, target: UiTarget, timeout: Duration) -> Result<Value> {
        let start = Instant::now();
        let mut rejected = 0;
        let (mut current, baseline) = fresh_wait_read(start, timeout, &mut rejected, || {
            research_ui::wait_snapshot(self.process()?)
        })?;
        if target
            .selected_slot
            .is_some_and(|slot| slot as usize >= baseline.slots.len())
        {
            return Err(Error::new(
                "invalid_arguments",
                "Selected slot exceeds fresh own slot count",
            ));
        }
        let mut last_date = baseline.campaign.date_raw;
        let mut current_received = Instant::now();
        loop {
            let reached = target.reached(&current);
            if reached || start.elapsed() >= timeout {
                return Ok(
                    json!({"reasons":[if reached {"research_ui_reached"} else {"wall_timeout"}],"state":current,"condition_meaning":"active view and local visibility predicates; no hit routing or input authorization","elapsed_seconds":start.elapsed().as_secs_f64(),"rejected_torn_reads":rejected,"state_age_ms":current_received.elapsed().as_millis()}),
                );
            }
            std::thread::sleep(
                Duration::from_millis(250).min(timeout.saturating_sub(start.elapsed())),
            );
            if start.elapsed() >= timeout {
                continue;
            }
            let (value, context) = fresh_wait_read(start, timeout, &mut rejected, || {
                research_ui::wait_snapshot(self.process()?)
            })?;
            same_ui_context(&baseline, &context, last_date)?;
            last_date = context.campaign.date_raw;
            current = value;
            current_received = Instant::now();
        }
    }
    fn call(&self, name: &str, args: &Value, commands: Option<&dyn Commands>) -> Result<Value> {
        let object = args
            .as_object()
            .ok_or_else(|| Error::new("invalid_arguments", "Tool arguments must be an object"))?;
        match name {
            "hoi4_capabilities" if object.is_empty() => Ok(
                json!({"runtime":"rust","platform":std::env::consts::OS,"owned_session_bound":self.process.is_some(),"unavailable":self.unavailable,"observations":["own_focus","own_research","clock","bounded_wait","technology_definitions","gui_input_diagnostics","own_research_ui_diagnostics"],"commands":commands.map(|c| c.capabilities()).unwrap_or_else(|| json!("unavailable: no verified input binding")),"native_transactions":"unavailable: failed posting path quarantined","default_mode":"FAIR","mod_qa":"separate; unavailable in this runtime","windows_macos":"unverified native adapters"}),
            ),
            "hoi4_overview" if object.is_empty() => self.overview(),
            "hoi4_research" => {
                if object
                    .keys()
                    .any(|k| !["offset", "limit"].contains(&k.as_str()))
                {
                    return Err(Error::new("invalid_arguments", "Unknown research argument"));
                }
                let (offset, limit) = page_bounds(object)?;
                research::page(self.process()?, offset, limit)
            }
            "hoi4_input_geometry" if object.is_empty() => gui::input_geometry(self.process()?),
            "hoi4_research_ui" => {
                if !object.is_empty() {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Research UI accepts no arguments",
                    ));
                }
                research_ui::snapshot(self.process()?)
            }
            "hoi4_technology_list" => {
                if object
                    .keys()
                    .any(|k| !["filter", "offset", "limit"].contains(&k.as_str()))
                {
                    return Err(Error::new("invalid_arguments", "Unknown catalog argument"));
                }
                let filter = object
                    .get("filter")
                    .map(|v| {
                        v.as_str().ok_or_else(|| {
                            Error::new("invalid_arguments", "Filter must be a string")
                        })
                    })
                    .transpose()?
                    .unwrap_or("");
                let (offset, limit) = page_bounds(object)?;
                if filter.chars().count() > 128 {
                    return Err(Error::new("invalid_arguments", "Filter exceeds bound"));
                }
                technology::catalog(self.process()?, filter, offset, limit)
            }
            "hoi4_wait_for" => {
                if object.keys().any(|k| {
                    !["timeout_seconds", "until_date", "until_research_ui"].contains(&k.as_str())
                }) {
                    return Err(Error::new("invalid_arguments", "Unknown wait argument"));
                }
                let timeout = if let Some(v) = object.get("timeout_seconds") {
                    v.as_u64()
                        .ok_or_else(|| Error::new("invalid_arguments", "Timeout must be integer"))?
                } else {
                    45
                };
                if !(1..=50).contains(&timeout) {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Timeout must be 1..50 seconds",
                    ));
                }
                let ui_target = object
                    .get("until_research_ui")
                    .map(UiTarget::parse)
                    .transpose()?;
                if ui_target.is_some() && object.contains_key("until_date") {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Choose one date or research UI wait target",
                    ));
                }
                if let Some(target) = ui_target {
                    return self.wait_ui(target, Duration::from_secs(timeout));
                }
                let target = if let Some(v) = object.get("until_date") {
                    let v = v.as_array().filter(|a| a.len() == 3).ok_or_else(|| {
                        Error::new(
                            "invalid_arguments",
                            "until_date must contain year,month,day",
                        )
                    })?;
                    let numbers: Vec<_> = v
                        .iter()
                        .map(|n| {
                            n.as_i64().ok_or_else(|| {
                                Error::new("invalid_arguments", "Date components must be integers")
                            })
                        })
                        .collect::<Result<_>>()?;
                    Some(calendar::encode(
                        numbers[0],
                        usize::try_from(numbers[1])
                            .map_err(|_| Error::new("invalid_date", "Invalid month"))?,
                        numbers[2],
                    )?)
                } else {
                    None
                };
                let start = Instant::now();
                let timeout = Duration::from_secs(timeout);
                let mut rejected = 0;
                let (before, baseline) = fresh_wait_read(start, timeout, &mut rejected, || {
                    game::wait_snapshot(self.process()?)
                })?;
                let mut current = before.clone();
                let mut last_date = baseline.date_raw;
                let mut current_received = Instant::now();
                loop {
                    let mut reasons = Vec::new();
                    for field in ["focus", "research"] {
                        if current[field] != before[field] {
                            reasons.push(format!("{field}_changed"));
                        }
                    }
                    if target.is_some_and(|v| {
                        current["date"]["raw"]
                            .as_i64()
                            .is_some_and(|n| n >= i64::from(v))
                    }) {
                        reasons.push("date_reached".into());
                    }
                    if !reasons.is_empty() || start.elapsed() >= timeout {
                        if reasons.is_empty() {
                            reasons.push("wall_timeout".into());
                        }
                        return Ok(
                            json!({"reasons":reasons,"state":current,"elapsed_seconds":start.elapsed().as_secs_f64(),"rejected_torn_reads":rejected,"state_age_ms":current_received.elapsed().as_millis()}),
                        );
                    }
                    std::thread::sleep(
                        Duration::from_millis(250).min(timeout.saturating_sub(start.elapsed())),
                    );
                    if start.elapsed() >= timeout {
                        continue;
                    }
                    let (observed, context) =
                        fresh_wait_read(start, timeout, &mut rejected, || {
                            game::wait_snapshot(self.process()?)
                        })?;
                    same_wait_context(&baseline, &context, last_date)?;
                    last_date = context.date_raw;
                    current = observed;
                    current_received = Instant::now();
                }
            }
            _ if commands.is_some_and(|c| c.supports(name)) => {
                let process = self.process.as_ref().ok_or_else(|| {
                    Error::new("capability_unavailable", "Owned process required for input")
                })?;
                commands.unwrap().call(process.as_ref(), name, args)
            }
            _ => Err(Error::new(
                "capability_unavailable",
                "Unknown tool or unsupported arguments; no raw/debug/action fallback",
            )),
        }
    }
    pub fn handle(&self, msg: Value) -> Option<Value> {
        self.handle_with_commands(msg, None)
    }
    pub fn handle_with_commands(
        &self,
        msg: Value,
        commands: Option<&dyn Commands>,
    ) -> Option<Value> {
        let commands = commands.filter(|_| self.process.is_some());
        let id = msg.get("id").cloned();
        let valid = msg.is_object()
            && msg.get("jsonrpc") == Some(&json!("2.0"))
            && msg.get("method").and_then(Value::as_str).is_some();
        if !valid {
            return Some(
                json!({"jsonrpc":"2.0","id":null,"error":{"code":-32600,"message":"Invalid Request"}}),
            );
        }
        let method = msg["method"].as_str().unwrap();
        id.as_ref()?;
        let result = match method {
            "initialize" => Ok(
                json!({"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"hoi4-live-native","version":env!("CARGO_PKG_VERSION")}}),
            ),
            "ping" => Ok(json!({})),
            "tools/list" => Ok(tools(commands)),
            "tools/call" => {
                let name = msg["params"]["name"].as_str().unwrap_or("");
                let empty = json!({});
                let args = msg["params"].get("arguments").unwrap_or(&empty);
                Ok(match self.call(name, args, commands) {
                    Ok(v) => content(v),
                    Err(e) => {
                        let mut v = content(json!({"error":e}));
                        v["isError"] = json!(true);
                        v
                    }
                })
            }
            _ => Err((-32601, "Method not found")),
        };
        Some(match result {
            Ok(v) => json!({"jsonrpc":"2.0","id":id,"result":v}),
            Err((code, message)) => {
                json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wait_retries_torn_reads_and_only_returns_a_fresh_snapshot() {
        let mut calls = 0;
        let mut rejected = 0;
        let result = fresh_wait_read(
            Instant::now(),
            Duration::from_secs(2),
            &mut rejected,
            || {
                calls += 1;
                if calls == 1 {
                    Err(Error::new("torn_read", "Fixture clock advanced"))
                } else {
                    Ok(json!({"date":"fresh"}))
                }
            },
        )
        .unwrap();
        assert_eq!(result["date"], "fresh");
        assert_eq!(calls, 2);
        assert_eq!(rejected, 1);
    }
    #[test]
    fn wait_deadline_never_returns_a_prior_snapshot_and_identity_errors_are_not_retried() {
        let mut rejected = 0;
        let mut calls = 0;
        let error = fresh_wait_read::<Value, _>(
            Instant::now(),
            Duration::from_millis(1),
            &mut rejected,
            || {
                calls += 1;
                if calls == 1 {
                    Err(Error::new("torn_read", "Fixture first sample torn"))
                } else {
                    Ok(json!({"date":"too late"}))
                }
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "unstable_observation");
        // Scheduling can consume the entire 1 ms before the first callback.
        // Both paths must refuse the late fresh sample, with no second read.
        assert!(calls <= 1);
        assert_eq!(rejected, calls);
        let rejected_before = rejected;
        let error = fresh_wait_read::<Value, _>(
            Instant::now().checked_sub(Duration::from_secs(2)).unwrap(),
            Duration::from_secs(1),
            &mut rejected,
            || panic!("Already expired deadline must never invoke a read"),
        )
        .unwrap_err();
        assert_eq!(error.code, "unstable_observation");
        assert_eq!(rejected, rejected_before);
        let mut calls = 0;
        let error = fresh_wait_read::<Value, _>(
            Instant::now(),
            Duration::from_secs(60),
            &mut rejected,
            || {
                calls += 1;
                Err(Error::new("ownership_changed", "Fixture replacement"))
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "ownership_changed");
        assert_eq!(calls, 1);
        assert_eq!(rejected, rejected_before);
    }
    #[test]
    fn wait_binds_context_and_monotonic_date_but_allows_ordinary_clock_changes() {
        let before = game::ClockContext {
            playthrough_id: "campaign".into(),
            player: "GER".into(),
            anchors: [1, 2, 3],
            date_raw: 100,
            difficulty: 2,
            historical_raw: 1,
            paused: true,
            speed: 0,
        };
        let mut current = before.clone();
        current.date_raw = 101;
        current.paused = false;
        current.speed = 4;
        assert!(same_wait_context(&before, &current, 100).is_ok());
        for field in 0..6 {
            let mut changed = current.clone();
            match field {
                0 => changed.player = "FRA".into(),
                1 => changed.playthrough_id = "other".into(),
                2 => changed.anchors[1] = 9,
                3 => changed.difficulty = 3,
                4 => changed.historical_raw = 0,
                _ => changed.date_raw = 99,
            }
            assert_eq!(
                same_wait_context(&before, &changed, 100).unwrap_err().code,
                "wait_context_changed"
            );
        }
        assert_eq!(
            same_wait_context(&before, &current, 102).unwrap_err().code,
            "wait_context_changed"
        );
    }
}
