use crate::{
    army, calendar, construction, economy,
    error::{Error, Result},
    game, gui, own_focus,
    platform::Process,
    production_lines, research, research_folders, research_ui, technology,
};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
mod assignment_wait;
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
    /// Where the standing plan lives (`hoi4_plan`, `hoi4_plan_run`), if this
    /// adapter has a run directory.
    fn plan_path(&self) -> Option<std::path::PathBuf> {
        None
    }
}

/// The server's own tools as the plan runner's tool set.
struct Internal<'a> {
    server: &'a Server,
    commands: &'a dyn Commands,
}
impl crate::plan_run::Tools for Internal<'_> {
    fn call(&self, name: &str, args: &Value) -> Result<Value> {
        self.server.call(name, args, Some(self.commands))
    }
}
static PLAN_STATE: std::sync::Mutex<Option<crate::plan_run::State>> = std::sync::Mutex::new(None);
fn tools(commands: Option<&dyn Commands>) -> Value {
    let mut value = json!({"tools":[
        {"name":"hoi4_capabilities","description":"Verified runtime capabilities and explicit platform/build limitations.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"hoi4_overview","description":"Fresh owned-country process snapshot. Pure bounded reads; never pauses.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"hoi4_research","description":"Fresh own completed technology IDs with bounded pagination, and active raw research counters. Progress units, days remaining and eligibility remain unverified. Pure reads; never pauses.","inputSchema":{"type":"object","properties":{"offset":{"type":"integer","minimum":0,"maximum":8192},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}},
        {"name":"hoi4_input_geometry","description":"Read-only own GUI pointer, dimensions and scale diagnostics. Does not prove client origin, control geometry or click authorization.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"hoi4_research_slot_control","description":"One own research slot by current array index: conditional ordinary callback binding and raw typed renderer diagnostics. Optional include_geometry adds bounded Sprite cache and typed hierarchy observations. No guessed Background name, coordinates or input authorization.","inputSchema":{"type":"object","properties":{"slot":{"type":"integer","minimum":0,"maximum":31},"include_geometry":{"type":"boolean"}},"required":["slot"],"additionalProperties":false}},
        {"name":"hoi4_research_category","description":"Read-only exact public folder ID association with the current own research tree, named tab, dense selection index and ordinary callback. Optional raw Sprite diagnostics; no tab switch or input authorization.","inputSchema":{"type":"object","properties":{"folder_id":{"type":"string","minLength":1,"maxLength":256},"include_geometry":{"type":"boolean"}},"required":["folder_id"],"additionalProperties":false}},
        {"name":"hoi4_research_folders","description":"Fresh sorted exact installed public research-folder IDs, independent of open GUI. Use these canonical keys rather than display labels; no aliases, eligibility, geometry or input authorization. Pure reads; never pauses.","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":true}},
        {"name":"hoi4_research_ui","description":"Bounded own research slot widgets, selected slot, canonical technology Info windows, named ordinary research button state and exact render diagnostics. No eligibility, hit routing or input authorization.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"hoi4_army","description":"Bounded own-country divisions with observation-scoped full handles, selected own location joins and raw signed strength/organisation. No persistent IDs, normalized units, enemies or actions.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"hoi4_economy","description":"Own-country named factory trigger counts, civilian project capacity and signed fixed-point political power. Pure reads; UI totals, stability, war support and shortages unavailable.","inputSchema":{"type":"object","properties":{},"additionalProperties":false}},
        {"name":"hoi4_production_lines","description":"Fresh own equipment-line collection only, validated in full before paging; default page 32. Exact Military/Naval snapshot-scoped opaque handles and signed active/damaged/requested factory counters. Unsupported types are metadata only, ownership unproved; completeness describes this collection's type coverage. Known owned lines can expose a base equipment type key after fresh canonical membership and variant/type checks; explicit unavailability covers unreviewed types. Custom variant identity, general production, allocation/idle capacity, eligibility and input authority unavailable. Pure reads; never pauses.","inputSchema":{"type":"object","properties":{"offset":{"type":"integer","minimum":0,"maximum":2048,"default":0},"limit":{"type":"integer","minimum":1,"maximum":128,"default":32}},"additionalProperties":false},"annotations":{"readOnlyHint":true}},
        {"name":"hoi4_construction_queue","description":"Read-only preview of own construction collection, validated fully before paging. Exact Building/Railway types, reciprocal owner and snapshot-scoped full handles; named cost/produced/speed and factory counters. Building keys use fresh template catalog joins. Unknown types are metadata only. Progress ratio is unclamped produced/cost, not ETA or completion; optional include_location adds selected-building state/province IDs only for own-owned-and-controlled territory with reciprocal catalog/status joins. Default false avoids location reads. Optional include_ui implies location reads and joins visible-panel row text to exact own located building handles; displayed text may be stale and never authorizes input. It also reports owned construction selection modes 0/1/3 and canonical building key, independent of queued lines; selection does not prove placement eligibility or map routing. Railway paths, allocation and input remain unavailable. Run071 observed one advancing dockyard; Run072 observed its state/label with a late-lease protocol deviation. Selection, province and railway live validation pending. Never pauses.","inputSchema":{"type":"object","properties":{"include_location":{"type":"boolean","default":false},"include_ui":{"type":"boolean","default":false},"offset":{"type":"integer","minimum":0,"maximum":2048,"default":0},"limit":{"type":"integer","minimum":1,"maximum":128,"default":32}},"additionalProperties":false},"annotations":{"readOnlyHint":true}},
        {"name":"hoi4_research_node_detail","description":"One exact own current technology key: complete current-record ownership checks, target-only raw renderer/hierarchy and own detail-state diagnostics. Unrelated GUI is outside scope; no client rectangle or input authorization.","inputSchema":{"type":"object","properties":{"technology_id":{"type":"string","minLength":1,"maxLength":256}},"required":["technology_id"],"additionalProperties":false}},
        {"name":"hoi4_research_nodes","description":"Current own-tree nodes joined to public technology keys and conditional ordinary-control diagnostics. Full model validated before prefix filtering/pagination; default page 25. Visibility, eligibility and click routing unavailable. Pure reads; never pauses.","inputSchema":{"type":"object","properties":{"filter":{"type":"string","maxLength":128},"offset":{"type":"integer","minimum":0,"maximum":512},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}},
        {"name":"hoi4_focus_list","description":"Bounded own canonical focus keys and own selected detail windows. All own memberships and keys validated before pagination; branch visibility, eligibility and input authorization unavailable. Pure reads; never pauses.","inputSchema":{"type":"object","properties":{"filter":{"type":"string","maxLength":128},"offset":{"type":"integer","minimum":0,"maximum":4096},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}},
        {"name":"hoi4_focus_completed","description":"Preview: fresh own completed-focus membership from the current Linux engine predicate, with active key and bounded sorted pages. Independent of open GUI; all members validated before paging. Completion cause, repeat counts, changed/shared trees and eligibility unavailable. Bounded Linux Rhineland completion verified; other Focus trees and full campaigns unverified. Pure reads; never pauses.","inputSchema":{"type":"object","properties":{"offset":{"type":"integer","minimum":0,"maximum":4096},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false},"annotations":{"readOnlyHint":true}},
        {"name":"hoi4_focus_search_state","description":"Read-only current own Focus search highlight and match counts for an exact canonical focus_id. Validates all search-cache/current-item joins before returning the selected key. A highlight is not an open detail, assignment or eligibility proof. Optional include_control adds exact own query text, listener and renderer diagnostics. Independent keyboard/text receiver identities are diagnostic; effective delivery and input authority remain unavailable.","inputSchema":{"type":"object","properties":{"focus_id":{"type":"string","minLength":1,"maxLength":128},"include_control":{"type":"boolean","default":false}},"required":["focus_id"],"additionalProperties":false},"annotations":{"readOnlyHint":true}},
        {"name":"hoi4_focus_node_detail","description":"Exact own focus key joined to complete current-item ownership and only the requested control/callback metadata. Optional raw Sprite/Text/Frame diagnostics; no input or eligibility authority.","inputSchema":{"type":"object","properties":{"focus_id":{"type":"string","minLength":1,"maxLength":128},"include_geometry":{"type":"boolean","default":false}},"required":["focus_id"],"additionalProperties":false}},
        {"name":"hoi4_focus_selected_detail","description":"Compact fresh exact own selected focus detail window. Start binding is window.start.ordinary_binding; the node symbol is a separate control. Refuses missing, different or multiple selected details. Optional existing raw renderer diagnostics; no client point, input or eligibility authority.","inputSchema":{"type":"object","properties":{"focus_id":{"type":"string","minLength":1,"maxLength":128},"include_geometry":{"type":"boolean","default":false}},"required":["focus_id"],"additionalProperties":false},"annotations":{"readOnlyHint":true}},
        {"name":"hoi4_technology_list","description":"Canonical loaded public technology IDs with bounded filtering/pagination. Catalog membership does not imply player research eligibility. Pure reads; never pauses.","inputSchema":{"type":"object","properties":{"filter":{"type":"string","maxLength":128},"offset":{"type":"integer","minimum":0,"maximum":8192},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}},
        {"name":"hoi4_wait_for","description":"Bounded server wait for own focus/research change, game date, research UI, or all explicit until_assignments targets. Assignment targets check the first sample too and mean active focus/current slot assignment, not completion over game time or a durable plan receipt. Choose one target family. UI/assignment matches do not authorize input. Wall timeout returns only an explicitly historical earlier accepted sample; no late target is accepted. Never pauses.","inputSchema":{"type":"object","properties":{"timeout_seconds":{"type":"integer","minimum":1,"maximum":50},"until_date":{"type":"array","items":{"type":"integer"},"minItems":3,"maxItems":3},"until_assignments":{"type":"object","properties":{"focus_id":{"type":"string","minLength":1,"maxLength":128,"pattern":"^[A-Za-z0-9_]+$"},"research":{"type":"array","maxItems":32,"items":{"type":"object","properties":{"slot":{"type":"integer","minimum":0,"maximum":31},"technology_id":{"type":"string","minLength":1,"maxLength":256,"pattern":"^[A-Za-z0-9_]+$"}},"required":["slot","technology_id"],"additionalProperties":false}}},"anyOf":[{"required":["focus_id"]},{"required":["research"],"properties":{"research":{"minItems":1}}}],"additionalProperties":false},"until_research_ui":{"type":"object","properties":{"view":{"type":"string","enum":["panel","tree"]},"selected_slot":{"type":"integer","minimum":0,"maximum":31}},"required":["view"],"additionalProperties":false}},"additionalProperties":false}}
    ]});
    value["tools"]
        .as_array_mut()
        .unwrap()
        .push(construction::controls::tool());
    value["tools"]
        .as_array_mut()
        .unwrap()
        .push(construction::find_view::tool());
    value["tools"]
        .as_array_mut()
        .unwrap()
        .push(production_lines::control_snapshot::tool());
    if let Some(commands) = commands {
        value["tools"]
            .as_array_mut()
            .unwrap()
            .extend(commands.tools());
    }
    value
}
/// Bounded whole-snapshot replacement for read-only construction tools, the same
/// policy as action pre-input reads: only torn_read, at most four reads.
fn read_only_recovered(producer: impl FnMut() -> Result<Value>) -> Result<Value> {
    use crate::construction_stage::read_recovery::{observe, Phase, Report};
    let mut report = Report::default();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    match observe(Phase::ReadOnly, deadline, producer, &mut report) {
        Ok(mut value) => {
            value["discarded_torn_snapshots"] = report.value("");
            Ok(value)
        }
        Err(error) => Err(report.with_error(error, "")),
    }
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
    read: F,
) -> Result<T>
where
    F: FnMut() -> Result<T>,
{
    fresh_wait_read_with(
        start,
        timeout,
        rejected,
        read,
        Instant::now,
        std::thread::sleep,
    )
}
fn fresh_wait_read_with<T>(
    start: Instant,
    timeout: Duration,
    rejected: &mut u64,
    read: impl FnMut() -> Result<T>,
    now: impl FnMut() -> Instant,
    sleep: impl FnMut(Duration),
) -> Result<T> {
    match bounded_wait_read_with(start, timeout, rejected, read, now, sleep)? {
        WaitRead::Fresh(value) => Ok(value),
        WaitRead::Deadline(refusal) => Err(refusal.error()),
    }
}
#[derive(Debug)]
enum WaitRead<T> {
    Fresh(T),
    // No late payload or context survives this outcome.
    Deadline(WaitDeadline),
}
#[derive(Clone, Copy, Debug)]
enum WaitDeadline {
    BeforeRead,
    TornExhausted,
    LateCompletion,
}
impl WaitDeadline {
    fn reason(self) -> &'static str {
        match self {
            Self::BeforeRead => "deadline_before_read",
            Self::TornExhausted => "no_stable_snapshot_before_deadline",
            Self::LateCompletion => "snapshot_completed_after_deadline",
        }
    }
    fn error(self) -> Error {
        Error::new(
            "unstable_observation",
            match self {
                Self::BeforeRead => {
                    "Fresh observation deadline reached; no further read or cached result"
                }
                Self::TornExhausted => {
                    "No stable snapshot within wait deadline; no stale result returned"
                }
                Self::LateCompletion => {
                    "Snapshot completed after wait deadline; no late target accepted"
                }
            },
        )
    }
}
fn bounded_wait_read<T>(
    start: Instant,
    timeout: Duration,
    rejected: &mut u64,
    read: impl FnMut() -> Result<T>,
) -> Result<WaitRead<T>> {
    bounded_wait_read_with(
        start,
        timeout,
        rejected,
        read,
        Instant::now,
        std::thread::sleep,
    )
}
fn bounded_wait_read_with<T>(
    start: Instant,
    timeout: Duration,
    rejected: &mut u64,
    mut read: impl FnMut() -> Result<T>,
    mut now: impl FnMut() -> Instant,
    mut sleep: impl FnMut(Duration),
) -> Result<WaitRead<T>> {
    loop {
        if now().saturating_duration_since(start) >= timeout {
            return Ok(WaitRead::Deadline(WaitDeadline::BeforeRead));
        }
        match read() {
            Err(e) if e.code == "torn_read" => {
                *rejected += 1;
                let remaining = timeout.saturating_sub(now().saturating_duration_since(start));
                if remaining.is_zero() {
                    return Ok(WaitRead::Deadline(WaitDeadline::TornExhausted));
                }
                sleep(remaining.min(Duration::from_millis(250)));
            }
            Ok(_) if now().saturating_duration_since(start) >= timeout => {
                return Ok(WaitRead::Deadline(WaitDeadline::LateCompletion));
            }
            result => return result.map(WaitRead::Fresh),
        }
    }
}
/// Only callers with an earlier timely, context-accepted snapshot may call this.
/// It reports historical timeout evidence, never a fresh or reached state.
fn historical_wait_timeout(
    state: Value,
    start: Instant,
    received: Instant,
    rejected: u64,
    refusal: Option<WaitDeadline>,
) -> Value {
    json!({"reasons":["wall_timeout"],"state":state,
        "state_historical":true,"current_state_verified":false,"input_authorized":false,
        "state_role":"last_snapshot_accepted_before_wait_deadline",
        "state_age_ms":received.elapsed().as_millis(),"state_age_basis":"time_since_accepted_snapshot",
        "elapsed_seconds":start.elapsed().as_secs_f64(),"rejected_torn_reads":rejected,
        "deadline_refusal":refusal.map(|d|json!({"reason":d.reason(),"fresh_read_error":d.error(),
            "late_snapshot_accepted":false,"late_target_accepted":false,"late_scope_accepted":false}))})
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
            if start.elapsed() >= timeout {
                let mut value =
                    historical_wait_timeout(current, start, current_received, rejected, None);
                value["condition_meaning"] = json!("active view and local visibility predicates; no hit routing or input authorization");
                return Ok(value);
            }
            let reached = target.reached(&current);
            if reached && start.elapsed() < timeout {
                return Ok(
                    json!({"reasons":["research_ui_reached"],"state":current,"condition_meaning":"active view and local visibility predicates; no hit routing or input authorization","elapsed_seconds":start.elapsed().as_secs_f64(),"rejected_torn_reads":rejected,"state_age_ms":current_received.elapsed().as_millis()}),
                );
            }
            std::thread::sleep(
                Duration::from_millis(250).min(timeout.saturating_sub(start.elapsed())),
            );
            if start.elapsed() >= timeout {
                continue;
            }
            let outcome = bounded_wait_read(start, timeout, &mut rejected, || {
                let sample = research_ui::wait_snapshot(self.process()?)?;
                // Refuse a real changed scope even if the read completed late.
                same_ui_context(&baseline, &sample.1, last_date)?;
                Ok(sample)
            })?;
            let (value, context) = match outcome {
                WaitRead::Fresh(sample) => sample,
                WaitRead::Deadline(refusal) => {
                    let mut value = historical_wait_timeout(
                        current,
                        start,
                        current_received,
                        rejected,
                        Some(refusal),
                    );
                    value["condition_meaning"] = json!("active view and local visibility predicates; no hit routing or input authorization");
                    return Ok(value);
                }
            };
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
                json!({"runtime":"rust","platform":std::env::consts::OS,"owned_session_bound":self.process.is_some(),"unavailable":self.unavailable,"observations":["own_production_controls","own_construction_controls","own_construction_queue","own_equipment_production_lines","public_research_folder_catalog","own_research_category_associations","own_research_slot_callback_diagnostics","own_division_snapshot_identifiers","own_economy_trigger_counts","own_focus","own_research","clock","bounded_wait","technology_definitions","gui_input_diagnostics","own_research_ui_diagnostics","own_current_research_nodes","own_research_node_detail_diagnostics","own_focus_catalog_ui","own_focus_selected_detail","own_focus_completion_membership","own_focus_search_selection"],"commands":commands.map(|c| c.capabilities()).unwrap_or_else(|| json!("unavailable: no verified input binding")),"unverified_observations":["own_production_controls","own_construction_controls","own_construction_queue","own_equipment_production_lines","own_research_category_associations"],"bounded_observation_validation":{"own_construction_queue":"run071: one dockyard identity and produced-work advancement; run072: state56 and stored UI label, late-lease deviation. Selection, province, railway and precise numeric UI comparison unverified","own_focus_completion_membership":"run052: actual Rhineland completion on the supported Linux build","own_focus_search_selection":"run068: exact query, six canonical matches, three ordinary Tab steps and View"},"native_transactions":"unavailable: failed posting path quarantined","default_mode":"FAIR","mod_qa":"separate; unavailable in this runtime","windows_macos":"unverified native adapters"}),
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
            "hoi4_research_folders" => {
                if !object.is_empty() {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Research folder catalog accepts no arguments",
                    ));
                }
                research_folders::snapshot(self.process()?)
            }
            "hoi4_focus_completed" => {
                if object
                    .keys()
                    .any(|k| !matches!(k.as_str(), "offset" | "limit"))
                {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Unknown completed focus argument",
                    ));
                }
                let (offset, limit) = page_bounds(object)?;
                if offset > 4096 {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Completed focus offset exceeds bound",
                    ));
                }
                own_focus::completion_snapshot(self.process()?, offset, limit)
            }
            "hoi4_input_geometry" if object.is_empty() => gui::input_geometry(self.process()?),
            "hoi4_research_slot_control" => {
                if !object.contains_key("slot")
                    || object
                        .keys()
                        .any(|k| !matches!(k.as_str(), "slot" | "include_geometry"))
                {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Own slot and optional include_geometry arguments required",
                    ));
                }
                let slot = object["slot"].as_u64().filter(|v| *v < 32).ok_or_else(|| {
                    Error::new("invalid_arguments", "Bounded own slot integer required")
                })?;
                let geometry = object
                    .get("include_geometry")
                    .map(|v| {
                        v.as_bool().ok_or_else(|| {
                            Error::new("invalid_arguments", "include_geometry must be boolean")
                        })
                    })
                    .transpose()?
                    .unwrap_or(false);
                if geometry {
                    research_ui::slot_control_geometry(self.process()?, slot as usize)
                } else {
                    research_ui::slot_control_snapshot(self.process()?, slot as usize)
                }
            }
            "hoi4_research_category" => {
                if object
                    .keys()
                    .any(|k| k != "folder_id" && k != "include_geometry")
                {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Folder ID and optional geometry flag only",
                    ));
                }
                let key = object
                    .get("folder_id")
                    .and_then(Value::as_str)
                    .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
                    .ok_or_else(|| {
                        Error::new("invalid_arguments", "Bounded public folder ID required")
                    })?;
                let geometry = match object.get("include_geometry") {
                    None => false,
                    Some(Value::Bool(v)) => *v,
                    _ => {
                        return Err(Error::new(
                            "invalid_arguments",
                            "Boolean geometry flag required",
                        ))
                    }
                };
                Ok(research_ui::category_with_geometry_owned(self.process()?, key, geometry)?.0)
            }
            "hoi4_research_ui" => {
                if !object.is_empty() {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Research UI accepts no arguments",
                    ));
                }
                research_ui::snapshot(self.process()?)
            }
            "hoi4_army" => {
                if !object.is_empty() {
                    return Err(Error::new("invalid_arguments", "Army accepts no arguments"));
                }
                army::snapshot(self.process()?)
            }
            "hoi4_economy" => {
                if !object.is_empty() {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Economy accepts no arguments",
                    ));
                }
                economy::snapshot(self.process()?)
            }
            "hoi4_production_controls" => {
                if object.keys().any(|k| {
                    !matches!(
                        k.as_str(),
                        "line_handle" | "equipment_id" | "include_geometry"
                    )
                }) {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Only line_handle, equipment_id and include_geometry accepted",
                    ));
                }
                let handle = object
                    .get("line_handle")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::new("invalid_arguments", "line_handle required"))?;
                let key = object
                    .get("equipment_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::new("invalid_arguments", "equipment_id required"))?;
                production_lines::control_snapshot::validate_target(handle, key)?;
                let geometry = object
                    .get("include_geometry")
                    .map(Value::as_bool)
                    .unwrap_or(Some(false))
                    .ok_or_else(|| {
                        Error::new("invalid_arguments", "include_geometry must be boolean")
                    })?;
                let process = self.process()?;
                read_only_recovered(|| {
                    production_lines::control_snapshot::snapshot(process, handle, key, geometry)
                })
            }
            "hoi4_construction_controls" => {
                if object
                    .keys()
                    .any(|k| k != "building_id" && k != "include_geometry")
                {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Only building_id and include_geometry accepted",
                    ));
                }
                let key = object
                    .get("building_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::new("invalid_arguments", "building_id required"))?;
                construction::controls::validate_key(key)?;
                let geometry = object
                    .get("include_geometry")
                    .map(Value::as_bool)
                    .unwrap_or(Some(false))
                    .ok_or_else(|| {
                        Error::new("invalid_arguments", "include_geometry must be boolean")
                    })?;
                let process = self.process()?;
                read_only_recovered(|| construction::controls::snapshot(process, key, geometry))
            }
            "hoi4_construction_find_view" => {
                if object.keys().any(|k| k != "state_id") {
                    return Err(Error::new("invalid_arguments", "Only state_id accepted"));
                }
                let state = match object.get("state_id") {
                    None => None,
                    Some(v) => Some(construction::find_view::validate_state_id(
                        v.as_i64().ok_or_else(|| {
                            Error::new("invalid_arguments", "state_id must be an integer")
                        })?,
                    )?),
                };
                let process = self.process()?;
                read_only_recovered(|| construction::find_view::snapshot(process, state))
            }
            "hoi4_production_lines" | "hoi4_construction_queue" => {
                if object.keys().any(|k| {
                    k != "offset"
                        && k != "limit"
                        && !(name == "hoi4_construction_queue"
                            && (k == "include_location" || k == "include_ui"))
                }) {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Unknown production page argument",
                    ));
                }
                let integer = |key: &str, default, low, high| {
                    object
                        .get(key)
                        .map_or(Some(default), Value::as_u64)
                        .filter(|v| (low..=high).contains(v))
                        .map(|v| v as usize)
                        .ok_or_else(|| {
                            Error::new("invalid_arguments", "Production page exceeds bounds")
                        })
                };
                let offset = integer("offset", 0, 0, 2048)?;
                let limit = integer("limit", 32, 1, 128)?;
                if name == "hoi4_construction_queue" {
                    let include_location = object
                        .get("include_location")
                        .map_or(Some(false), Value::as_bool)
                        .ok_or_else(|| {
                            Error::new("invalid_arguments", "include_location must be boolean")
                        })?;
                    let include_ui = object
                        .get("include_ui")
                        .map_or(Some(false), Value::as_bool)
                        .ok_or_else(|| {
                            Error::new("invalid_arguments", "include_ui must be boolean")
                        })?;
                    construction::page_with_options(
                        self.process()?,
                        offset,
                        limit,
                        include_location,
                        include_ui,
                    )
                } else {
                    production_lines::page(self.process()?, offset, limit)
                }
            }
            "hoi4_research_node_detail" => {
                if object.keys().any(|k| k != "technology_id") {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Unknown node detail argument",
                    ));
                }
                let key = object
                    .get("technology_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        Error::new("invalid_arguments", "technology_id string required")
                    })?;
                research_ui::validate_detail_key(key)?;
                research_ui::node_detail(self.process()?, key)
            }
            "hoi4_research_nodes" => {
                if object
                    .keys()
                    .any(|k| !["filter", "offset", "limit"].contains(&k.as_str()))
                {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Unknown current node argument",
                    ));
                }
                let (offset, parsed_limit) = page_bounds(object)?;
                let limit = if object.contains_key("limit") {
                    parsed_limit
                } else {
                    25
                };
                let filter = object
                    .get("filter")
                    .map(|v| {
                        v.as_str().ok_or_else(|| {
                            Error::new("invalid_arguments", "Node filter must be a string")
                        })
                    })
                    .transpose()?;
                if offset > 512
                    || filter.is_some_and(|v| {
                        v.len() > 128 || !v.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    })
                {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Current node query exceeds bounds",
                    ));
                }
                research_ui::nodes_page(self.process()?, offset, limit, filter)
            }
            "hoi4_focus_search_state" => {
                if object
                    .keys()
                    .any(|key| !["focus_id", "include_control"].contains(&key.as_str()))
                {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Unknown Focus search argument",
                    ));
                }
                let key = object
                    .get("focus_id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::new("invalid_arguments", "focus_id string required"))?;
                own_focus::validate_key(key)
                    .map_err(|error| Error::new("invalid_arguments", error.message))?;
                let include_control = match object.get("include_control") {
                    None => false,
                    Some(Value::Bool(value)) => *value,
                    _ => {
                        return Err(Error::new(
                            "invalid_arguments",
                            "include_control must be boolean",
                        ))
                    }
                };
                if include_control {
                    own_focus::search_state_with_control(self.process()?, key)
                } else {
                    own_focus::search_state(self.process()?, key)
                }
            }
            "hoi4_focus_node_detail" | "hoi4_focus_selected_detail" => {
                if !object.contains_key("focus_id")
                    || object
                        .keys()
                        .any(|key| key != "focus_id" && key != "include_geometry")
                {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Exactly one canonical focus_id required",
                    ));
                }
                let key = object["focus_id"]
                    .as_str()
                    .ok_or_else(|| Error::new("invalid_arguments", "focus_id must be a string"))?;
                own_focus::validate_key(key)?;
                let include_geometry = object
                    .get("include_geometry")
                    .map(|v| {
                        v.as_bool().ok_or_else(|| {
                            Error::new("invalid_arguments", "include_geometry must be boolean")
                        })
                    })
                    .transpose()?
                    .unwrap_or(false);
                if name == "hoi4_focus_selected_detail" {
                    own_focus::selected_detail(self.process()?, key, include_geometry)
                } else {
                    own_focus::node_detail_with_geometry(self.process()?, key, include_geometry)
                }
            }
            "hoi4_focus_list" => {
                if object
                    .keys()
                    .any(|k| !["filter", "offset", "limit"].contains(&k.as_str()))
                {
                    return Err(Error::new("invalid_arguments", "Unknown focus argument"));
                }
                let (offset, limit) = page_bounds(object)?;
                let filter = object
                    .get("filter")
                    .map(|value| {
                        value.as_str().ok_or_else(|| {
                            Error::new("invalid_arguments", "Focus filter must be a string")
                        })
                    })
                    .transpose()?;
                if offset > 4096
                    || filter.is_some_and(|value| {
                        value.len() > 128
                            || !value
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                    })
                {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Focus query exceeds bounds",
                    ));
                }
                own_focus::snapshot(self.process()?, offset, limit, filter)
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
                    ![
                        "timeout_seconds",
                        "until_date",
                        "until_research_ui",
                        "until_assignments",
                    ]
                    .contains(&k.as_str())
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
                if ["until_date", "until_research_ui", "until_assignments"]
                    .iter()
                    .filter(|key| object.contains_key(**key))
                    .count()
                    > 1
                {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Choose one date, research UI or assignment wait target",
                    ));
                }
                let assignments = object
                    .get("until_assignments")
                    .map(assignment_wait::Target::parse)
                    .transpose()?;
                let ui_target = object
                    .get("until_research_ui")
                    .map(UiTarget::parse)
                    .transpose()?;
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
                    if start.elapsed() >= timeout {
                        return Ok(historical_wait_timeout(
                            current,
                            start,
                            current_received,
                            rejected,
                            None,
                        ));
                    }
                    let mut reasons = Vec::new();
                    if let Some(assignments) = &assignments {
                        // Includes the first accepted sample: completion before
                        // this RPC is not a missed edge requiring another event.
                        if assignments.reached(&current) {
                            reasons.push("assignments_reached".into());
                        }
                    } else {
                        for field in ["focus", "research"] {
                            if current[field] != before[field] {
                                reasons.push(format!("{field}_changed"));
                            }
                        }
                    }
                    if target.is_some_and(|v| {
                        current["date"]["raw"]
                            .as_i64()
                            .is_some_and(|n| n >= i64::from(v))
                    }) {
                        reasons.push("date_reached".into());
                    }
                    if !reasons.is_empty() && start.elapsed() < timeout {
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
                    let outcome = bounded_wait_read(start, timeout, &mut rejected, || {
                        let sample = game::wait_snapshot(self.process()?)?;
                        same_wait_context(&baseline, &sample.1, last_date)?;
                        Ok(sample)
                    })?;
                    let (observed, context) = match outcome {
                        WaitRead::Fresh(sample) => sample,
                        WaitRead::Deadline(refusal) => {
                            return Ok(historical_wait_timeout(
                                current,
                                start,
                                current_received,
                                rejected,
                                Some(refusal),
                            ))
                        }
                    };
                    last_date = context.date_raw;
                    current = observed;
                    current_received = Instant::now();
                }
            }
            "hoi4_plan" | "hoi4_plan_run" if commands.and_then(|c| c.plan_path()).is_some() => {
                let commands = commands.unwrap();
                let path = commands.plan_path().unwrap();
                if name == "hoi4_plan" {
                    return match object.get("op").and_then(Value::as_str) {
                        Some("get") if object.len() == 1 => crate::plan_run::read_plan(&path),
                        Some("set") if object.len() == 2 => {
                            let changes = object
                                .get("plan")
                                .and_then(Value::as_object)
                                .ok_or_else(|| {
                                    Error::new("invalid_arguments", "plan must be an object")
                                })?;
                            crate::plan_run::merge_plan(&path, changes)
                        }
                        _ => Err(Error::new(
                            "invalid_arguments",
                            "Use {op:get} or {op:set, plan:{...}}",
                        )),
                    };
                }
                if object
                    .keys()
                    .any(|k| !["seconds", "lease_token"].contains(&k.as_str()))
                {
                    return Err(Error::new(
                        "invalid_arguments",
                        "Only seconds and lease_token accepted",
                    ));
                }
                let lease = object
                    .get("lease_token")
                    .and_then(Value::as_str)
                    .ok_or_else(|| Error::new("invalid_arguments", "lease_token required"))?;
                let seconds = object.get("seconds").and_then(Value::as_u64).unwrap_or(60);
                if !(1..=300).contains(&seconds) {
                    return Err(Error::new("invalid_arguments", "seconds must be 1..300"));
                }
                let tools = Internal {
                    server: self,
                    commands,
                };
                let mut guard = PLAN_STATE.lock().unwrap_or_else(|p| p.into_inner());
                let state = guard.get_or_insert_with(Default::default);
                crate::plan_run::run(state, &tools, &path, lease, seconds)
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
    use std::cell::Cell;

    #[test]
    fn late_followup_discards_target_and_reports_only_earlier_historical_state() {
        for completed in [Duration::from_secs(1), Duration::from_secs(2)] {
            let start = Instant::now();
            let now = Cell::new(start);
            let mut rejected = 0;
            let prior = fresh_wait_read_with(
                start,
                Duration::from_secs(1),
                &mut rejected,
                || Ok(json!({"date":"earlier_accepted","focus":"unchanged"})),
                || now.get(),
                |_| panic!("Timely initial sample must not sleep"),
            )
            .unwrap();
            let outcome = bounded_wait_read_with(
                start,
                Duration::from_secs(1),
                &mut rejected,
                || {
                    now.set(start + completed);
                    Ok(json!({"date":"late_target","focus":"late_change"}))
                },
                || now.get(),
                |_| panic!("Late success must not sleep or retry"),
            )
            .unwrap();
            let WaitRead::Deadline(refusal) = outcome else {
                panic!("Late value must be discarded");
            };
            let value = historical_wait_timeout(
                prior,
                start,
                start.checked_sub(Duration::from_millis(250)).unwrap(),
                rejected,
                Some(refusal),
            );
            assert_eq!(value["reasons"], json!(["wall_timeout"]));
            assert_eq!(value["state"]["date"], "earlier_accepted");
            assert_eq!(value["state"]["focus"], "unchanged");
            assert_eq!(value["state_historical"], true);
            assert_eq!(value["current_state_verified"], false);
            assert_eq!(value["input_authorized"], false);
            assert!(value["state_age_ms"].as_u64().unwrap() >= 250);
            assert_eq!(
                value["deadline_refusal"]["reason"],
                "snapshot_completed_after_deadline"
            );
            assert_eq!(value["deadline_refusal"]["late_target_accepted"], false);
            assert_eq!(value["deadline_refusal"]["late_scope_accepted"], false);
            assert!(!value.to_string().contains("late_target\""));
        }
    }

    #[test]
    fn late_followup_real_errors_are_never_normalized_into_historical_timeout() {
        for code in [
            "ownership_changed",
            "permission_denied",
            "wait_context_changed",
            "unstable_observation",
        ] {
            let start = Instant::now();
            let now = Cell::new(start);
            let mut rejected = 0;
            let error = bounded_wait_read_with::<Value>(
                start,
                Duration::from_secs(1),
                &mut rejected,
                || {
                    now.set(start + Duration::from_secs(2));
                    Err(Error::new(code, "Real backend refusal"))
                },
                || now.get(),
                |_| panic!("Real error must not sleep or retry"),
            )
            .unwrap_err();
            assert_eq!(error.code, code);
            assert!(error.to_string().contains("Real backend refusal"));
            assert_eq!(rejected, 0);
        }
    }

    #[test]
    fn followup_cross_baseline_scope_is_checked_even_when_payload_completes_late() {
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
        for field in 0..7 {
            let mut current = before.clone();
            match field {
                0 => current.player = "FRA".into(),
                1 => current.playthrough_id = "different".into(),
                2 => current.anchors[1] = 9,
                3 => current.difficulty = 3,
                4 => current.historical_raw = 0,
                5 => current.date_raw = 99,
                _ => current.date_raw = 101,
            }
            let start = Instant::now();
            let now = Cell::new(start);
            let mut rejected = 0;
            let error = bounded_wait_read_with(
                start,
                Duration::from_secs(1),
                &mut rejected,
                || {
                    now.set(start + Duration::from_secs(2));
                    same_wait_context(&before, &current, if field == 6 { 102 } else { 100 })?;
                    Ok(json!({"target":"not_accepted"}))
                },
                || now.get(),
                |_| panic!("Changed scope must not retry"),
            )
            .unwrap_err();
            assert_eq!(error.code, "wait_context_changed");
        }
    }

    #[test]
    fn late_ui_followup_preserves_changed_anchor_or_slot_error() {
        let before = research_ui::UiContext {
            campaign: game::ClockContext {
                playthrough_id: "campaign".into(),
                player: "GER".into(),
                anchors: [1, 2, 3],
                date_raw: 100,
                difficulty: 2,
                historical_raw: 1,
                paused: true,
                speed: 0,
            },
            anchors: [4, 5, 6, 7, 8, 9],
            slots: vec![10, 11],
        };
        for slot_changed in [false, true] {
            let mut current = before.clone();
            if slot_changed {
                current.slots.swap(0, 1);
            } else {
                current.anchors[5] = 99;
            }
            let start = Instant::now();
            let now = Cell::new(start);
            let mut rejected = 0;
            let error = bounded_wait_read_with(
                start,
                Duration::from_secs(1),
                &mut rejected,
                || {
                    now.set(start + Duration::from_secs(2));
                    same_ui_context(&before, &current, 100)?;
                    Ok(json!({"view":"late_target"}))
                },
                || now.get(),
                |_| panic!("Changed UI scope must not retry"),
            )
            .unwrap_err();
            assert_eq!(error.code, "wait_context_changed");
            assert_eq!(rejected, 0);
        }
    }

    #[test]
    fn followup_torn_deadline_with_prior_state_is_historical_but_initial_remains_strict() {
        let start = Instant::now();
        let now = Cell::new(start);
        let mut rejected = 0;
        let outcome = bounded_wait_read_with::<Value>(
            start,
            Duration::from_secs(1),
            &mut rejected,
            || {
                now.set(start + Duration::from_secs(1));
                Err(Error::new("torn_read", "Changed fields"))
            },
            || now.get(),
            |_| panic!("Expired torn read must not sleep"),
        )
        .unwrap();
        let WaitRead::Deadline(refusal) = outcome else {
            panic!("No timely sample exists");
        };
        let value = historical_wait_timeout(
            json!({"accepted":"prior"}),
            start,
            start,
            rejected,
            Some(refusal),
        );
        assert_eq!(value["reasons"], json!(["wall_timeout"]));
        assert_eq!(value["state_historical"], true);
        assert_eq!(value["rejected_torn_reads"], 1);
        assert_eq!(
            value["deadline_refusal"]["reason"],
            "no_stable_snapshot_before_deadline"
        );
        now.set(start);
        let error = fresh_wait_read_with::<Value>(
            start,
            Duration::from_secs(1),
            &mut rejected,
            || {
                now.set(start + Duration::from_secs(1));
                Err(Error::new("torn_read", "No initial sample"))
            },
            || now.get(),
            |_| panic!("No initial stable sample must not retry late"),
        )
        .unwrap_err();
        assert_eq!(error.code, "unstable_observation");
    }

    #[test]
    fn followup_timely_sample_and_already_expired_boundary_stay_distinct() {
        let start = Instant::now();
        let now = Cell::new(start);
        let mut rejected = 0;
        let outcome = bounded_wait_read_with(
            start,
            Duration::from_secs(1),
            &mut rejected,
            || {
                now.set(start + Duration::from_millis(999));
                Ok(json!({"sample":"timely"}))
            },
            || now.get(),
            |_| panic!("Timely read must not sleep"),
        )
        .unwrap();
        let WaitRead::Fresh(value) = outcome else {
            panic!("Timely sample must remain fresh");
        };
        assert_eq!(value["sample"], "timely");
        now.set(start + Duration::from_secs(1));
        let outcome = bounded_wait_read_with::<Value>(
            start,
            Duration::from_secs(1),
            &mut rejected,
            || panic!("Already expired followup must not start any read"),
            || now.get(),
            |_| panic!("Already expired followup must not sleep"),
        )
        .unwrap();
        let WaitRead::Deadline(refusal) = outcome else {
            panic!("Expired followup has no payload");
        };
        assert_eq!(refusal.reason(), "deadline_before_read");
    }

    #[test]
    fn wait_refuses_a_successful_read_completed_at_or_after_its_deadline() {
        for late in [Duration::from_secs(1), Duration::from_secs(2)] {
            let start = Instant::now();
            let now = Cell::new(start);
            let reads = Cell::new(0);
            let mut rejected = 0;
            let error = fresh_wait_read_with(
                start,
                Duration::from_secs(1),
                &mut rejected,
                || {
                    reads.set(reads.get() + 1);
                    now.set(start + late);
                    Ok(json!({"date":"target_reached_too_late"}))
                },
                || now.get(),
                |_| panic!("A late successful read must not retry or sleep"),
            )
            .unwrap_err();
            assert_eq!(error.code, "unstable_observation");
            assert_eq!(reads.get(), 1);
            assert_eq!(rejected, 0);
        }
    }

    #[test]
    fn wait_torn_retry_retains_original_budget_and_caps_its_sleep() {
        let start = Instant::now();
        let now = Cell::new(start);
        let reads = Cell::new(0);
        let slept = Cell::new(Duration::ZERO);
        let mut rejected = 0;
        let error = fresh_wait_read_with::<Value>(
            start,
            Duration::from_millis(300),
            &mut rejected,
            || {
                reads.set(reads.get() + 1);
                now.set(start + Duration::from_millis(280));
                Err(Error::new("torn_read", "Own clock changed"))
            },
            || now.get(),
            |delay| {
                slept.set(slept.get() + delay);
                now.set(now.get() + delay);
            },
        )
        .unwrap_err();
        assert_eq!(error.code, "unstable_observation");
        assert_eq!(reads.get(), 1);
        assert_eq!(rejected, 1);
        assert_eq!(slept.get(), Duration::from_millis(20));
    }

    #[test]
    fn wait_accepts_only_a_timely_fresh_sample_after_a_torn_retry() {
        let start = Instant::now();
        let now = Cell::new(start);
        let reads = Cell::new(0);
        let mut rejected = 0;
        let value = fresh_wait_read_with(
            start,
            Duration::from_secs(1),
            &mut rejected,
            || {
                reads.set(reads.get() + 1);
                now.set(now.get() + Duration::from_millis(100));
                if reads.get() == 1 {
                    Err(Error::new("torn_read", "Own clock changed"))
                } else {
                    Ok(json!({"date":"fresh_timely_target"}))
                }
            },
            || now.get(),
            |delay| now.set(now.get() + delay),
        )
        .unwrap();
        assert_eq!(value["date"], "fresh_timely_target");
        assert_eq!(reads.get(), 2);
        assert_eq!(rejected, 1);
        assert_eq!(now.get() - start, Duration::from_millis(450));
    }

    #[test]
    fn wait_preserves_a_late_identity_error_without_retry_or_cached_success() {
        let start = Instant::now();
        let now = Cell::new(start);
        let reads = Cell::new(0);
        let mut rejected = 0;
        let error = fresh_wait_read_with::<Value>(
            start,
            Duration::from_secs(1),
            &mut rejected,
            || {
                reads.set(reads.get() + 1);
                now.set(start + Duration::from_secs(2));
                Err(Error::new("ownership_changed", "Own process was replaced"))
            },
            || now.get(),
            |_| panic!("Identity errors must not sleep or retry"),
        )
        .unwrap_err();
        assert_eq!(error.code, "ownership_changed");
        assert_eq!(reads.get(), 1);
        assert_eq!(rejected, 0);
    }

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

    #[test]
    fn read_only_tools_replace_only_whole_torn_snapshots_and_report_them() {
        use std::cell::Cell;
        let calls = Cell::new(0);
        let value = read_only_recovered(|| {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                Err(Error::new(
                    "torn_read",
                    "Snapshot witness changed; section=opener",
                ))
            } else {
                Ok(json!({"ok":true}))
            }
        })
        .unwrap();
        assert_eq!(calls.get(), 2);
        assert_eq!(value["ok"], true);
        let discarded = value["discarded_torn_snapshots"].as_array().unwrap();
        assert_eq!(discarded.len(), 1);
        assert_eq!(discarded[0]["phase"], "read_only");
        assert_eq!(discarded[0]["input_authorized"], false);
        let calls = Cell::new(0);
        let error = read_only_recovered(|| {
            calls.set(calls.get() + 1);
            Err(Error::new("invalid_construction", "type differs"))
        })
        .unwrap_err();
        assert_eq!((calls.get(), error.code), (1, "invalid_construction"));
        let calls = Cell::new(0);
        let error = read_only_recovered(|| {
            calls.set(calls.get() + 1);
            Err(Error::new("torn_read", "always"))
        })
        .unwrap_err();
        assert_eq!((calls.get(), error.code), (4, "torn_read"));
        assert!(error.message.contains("discarded_preinput_reads"));
    }
}
