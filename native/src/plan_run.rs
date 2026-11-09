//! Standing-plan runner: the routine a player declares once in `plan.json`
//! (research priorities, a focus path, construction, training, production,
//! politics, decisions) is carried out one verified action at a time through
//! the same tools a model would call, so the model only wakes for war,
//! diplomacy, armies and real decisions. Nothing here pauses the game or
//! bypasses a tool: every action is an ordinary tool call under the caller's
//! input lease, journaled and verified like any other.
//!
//! Plan keys (all optional; the file is re-read every tick):
//!   active                 false pauses the runner
//!   stop_on_date           "YYYY.MM.DD" or {"year","month","day"}: when the game date (hoi4_overview) reaches or
//!                          passes it the runner pauses the game once through hoi4_time (paused:true, its own
//!                          request id; skipped when the overview already says paused), reports
//!                          `stopped_on_date:true` with a `reason`, and takes no other action again. A refused or
//!                          uncertain pause is reported in `stop_on_date_pause` and never retried. Alarm `target_date_reached`
//!   stop_on_date_pause     false: only stop acting at stop_on_date, never pause (default true)
//!   stop_on_failure        true returns the first refused/diverged action instead of trying other work
//!   (all timers are GAME time, measured in in-game hours from `hoi4_overview`'s date,
//!   so a failed action is skipped for N game days whatever the speed, and nothing
//!   advances while the game is paused)
//!   alarms                 thresholds for the compact digest's `alarms` list; see `crate::alarms`
//!   event_prefix_options   {"DOD_":"last"}: option for every event whose id starts with the prefix
//!   auto_events            answer news and single-option events with option 0 (default true);
//!                          other events after `wait_events_days` game days (default 2) with
//!                          `event_options[event]` (default 0)
//!   research_priority      technology keys; the first not-yet-researched key the game accepts
//!                          (then the catalog in order, unless research_fallback is false)
//!   research_fallback      default true: an empty slot with nothing startable in research_priority takes the
//!                          first startable catalogue tech (this can be naval hull junk; every pick raises the
//!                          alarm `research_fallback_used`). false or "priority_only": never use the catalogue
//!   research_fallback_prefixes   ["infantry","support_"]: the catalogue fallback only takes keys starting with one
//!   research_fallback_categories ["armor","air"]: ... or containing one (catalogue keys carry no category field,
//!                          so a category is a word inside the key). With either list the fallback is restricted
//!   research_fallback_exclude    words the fallback never takes (default ship, hull, naval, carrier, submarine,
//!                          destroyer, cruiser, battleship, torpedo, convoy; [] allows everything)
//!   focus_path             focus keys; the first not completed key the game accepts. A key the game blocks
//!                          (focus_not_available, prerequisite, exclusive) is skipped for `skip_hours.focus_blocked`
//!                          and the next key is tried at once; `focus_blocked:[key:reason]` is in the digest
//!   focus_fallback         focus keys tried only after every focus_path key is completed or skipped
//!   skip_hours             {"tech":72,"tech_fallback":168,"focus":24,"focus_blocked":6}: game hours a refused key
//!                          is left alone (defaults shown); all optional
//!                          The digest carries one `research` line and one `focus` line per run (like `construction`)
//!                          and the alarms `research_slot_idle_plan_exhausted` / `no_active_focus_plan_exhausted`
//!                          when nothing from the plan could be started. `active` missing means active; only
//!                          `"active": false` pauses the runner (result reason `plan inactive (active:false)`)
//!   _meta                  {"campaign":"<id>",..}: optional; `campaign` names the campaign this plan
//!                          belongs to, and plan_state.json of another campaign is ignored (below)
//!   politics               [{"id"?,"category","slot","idea","cost"?}] one-shot picks in list order;
//!                          "cost": political power needed first. Done-memory is keyed by `id`
//!                          (stable across a reorder or a revision), else by list position and idea
//!   decisions              decision keys taken whenever enabled
//!   training               {"template","queue","batch","deploy_state","when"?} keep a training queue, or a
//!                          list of them (plan_manage.rs, with `training_rules`)
//!   training_rules         {"only_if_stockpile","cap_template","cap"} guard and cap for training (plan_manage.rs)
//!   templates              [{"id","name","base"?,"regiments","support",..}] templates made once by plan (plan_manage.rs)
//!   occupation             {"default_law","harsher_at_resistance","milder_at_compliance",..} law steps (plan_manage.rs)
//!   peace                  {"take","puppet"?,"liberate"?} demands when a conference opens, else the stop (plan_manage.rs)
//!   auto_import            {"budget"} automatic importing on a resource deficit (plan_manage.rs)
//!   air_plan               [{"wing"|"airframe","region","mission","base_state"|"base_region",..}] (plan_manage.rs);
//!                          any mission hoi4_air_mission accepts (strategic_bombing, naval_strike, port_strike,
//!                          logistics_strike, interception, ..): there is no separate `air_ops` key (plan_military.rs)
//!   manage                 thresholds of the standing rules: alerts, manpower, stability, equipment (plan_manage.rs);
//!                          casualty_ratio_max, casualty_window_days, casualty_min_losses, casualty_rest_days,
//!                          enemy_air_superiority, air_front_regions, air_hold_days (plan_military.rs)
//!   army_group             [{"id","name"?,"armies":[..],"marshal"?,"when"?}] grouped once, field marshal once (plan_military.rs)
//!   navy                   {"posture","fleets":[{"fleet","missions":[{"mission","regions",..}],..}],"convoy_escort"?,
//!                          "admirals"?,"repair_below"?,"repair_until"?} regions, missions, admirals, repair (plan_military.rs)
//!   invasions              [{"id","from_state","target_state","target_province"?,"armies":[..],"fleet"?,"when"?,..}]
//!                          one naval invasion at a time through hoi4_naval_invasion (plan_military.rs)
//!   production_priority    equipment prefixes that receive military factories on no line
//!   production_min_idle    idle military factories needed before rebalancing (default 2)
//!   production             [{"equipment","factories"}] lines added once
//!   construction           [{"building","states":[..],"count":1}] spent while civilian factories are idle;
//!                          states are ids (or numeric strings, or exact state names, resolved with
//!                          hoi4_states); states with no free slot are remembered full for
//!                          `construction_full_days` (default 14) game days; free slots are read with
//!                          hoi4_state_slots unless `construction_probe_slots` is false
//!   construction_min_idle  idle civilian factories needed first (default 4), unless the queue
//!                          is shorter than `construction_queue_depth` (default 3 projects)
//!   construction_fallback_any_own_state  true: when every listed state is full, build in other own states
//!                          (hoi4_states with the player tag); every tick's result names why
//!                          nothing was queued in `construction`
//!   construction_plan      [{"id"?,"order","building","state"?,"province"?,"path"?:[province ids],
//!                          "from_state"?,"to_state"?,"when"?,"priority"?}] a triggered build order,
//!                          each item queued once (hoi4_construction_add, or hoi4_railway_build for
//!                          "rail_way"/"railway" from `path`'s first to last province) when `when` holds:
//!                          "YYYY.MM.DD", "province_controlled:<id>", "state_fully_controlled:<id>",
//!                          "owner:<state>=<TAG>" or empty (at once). Runs in peace and at war (when the
//!                          war runs first, right after the war step; else just before `construction`).
//!                          Done-memory is keyed by `id` (stable), else by `order`
//!   doctrines              [{"id","folder","grand_doctrine"?,"track"?,"subdoctrine"?,"xp"}] adopted in
//!                          list order through hoi4_doctrine_adopt once the folder's experience (folder
//!                          "land"/"special_forces" army XP, "naval"/"navy" navy XP, "air" air XP;
//!                          default "land") reaches "xp" (default 100): "grand_doctrine" is sent as
//!                          {doctrine}, "subdoctrine" as {doctrine, track} (an entry with both adopts
//!                          the grand doctrine first). The old entry shape {"doctrine","track"} is still
//!                          read for one release. An entry that waits blocks the ones after it.
//!                          Done-memory is keyed by `id` (stable), else by list position
//!   war                    {"enemies":["FRA"],"min_army":6,"per_target":2,"reserve":["Army 9"],"cooldown_hours":12}
//!                          send armies at the nearest enemy-held states (own states held by an enemy first):
//!                          step into the state, draw front and offensive lines, execute the plan.
//!                          Optional: "axes":[[842,843,271],[835,271]] ordered targets (an army follows the
//!                          axis whose next enemy-held state is nearest); "theatre":[550,559] own states an
//!                          army must stand in to be driven (armies in enemy land always are);
//!                          "stall_days":8 without progress -> direct hoi4_army_move into the target;
//!                          "rest_below":12 / "resume_above":30 army organisation (absolute) to stop and
//!                          resume the plan ("rest":false turns it off); "supply_floor":25 an army below this
//!                          supply percent rests too; "rest_max_days":20 resumes anyway; "motorization":2 sets every driven
//!                          army's supply trucks once
//!   disband                {"templates":["Camicie Nere"],"keep_states":[ids],"max":N} disband divisions of these
//!                          templates outside keep_states and war.theatre, one army per tick
//!   armies                 {"max":24,"min_for_general":8,"generals":["Name",..]} keep every division in an
//!                          army of at most `max`: create armies where divisions stand unassigned (folding in
//!                          standing armies when the game refuses and "absorb" is true), split oversized ones, and give the next
//!                          listed general to an army of at least `min_for_general` divisions;
//!                          armies below that size are merged into others (hoi4_army_merge) unless
//!                          "consolidate": false
//!
//! The strategy layer (step 0, before events and war; reads only, never input). Predicates are
//! `crate::plan_predicate` strings or {all|any|not}; an atom that cannot be read is unknown, never
//! true, and whatever waits on it waits; every unknown atom is listed once per run in the digest's
//! `predicates_unknown` with the reason. These keys are judged at most every 6 game hours (and at
//! once when they change); the digest's `strategy` block shows the phase, the goal status, the
//! checkpoints and the active contingencies, `strategy_changes` what changed in this run:
//!   goal                   {"predicates":[..],"deadline"?:"YYYY.MM.DD","fail_predicates"?:[..]}: every
//!                          predicate holds -> alarm `goal_reached`; a fail predicate holds, the deadline has
//!                          passed or is less than `alarms.goal_risk_days` (default 30) game days away ->
//!                          `goal_at_risk`. Both are also noted in the change feed when the status changes
//!   phases                 [{"id","enter"?,"exit"?,"overrides"?,"revise_on_enter"?}] in order: with no
//!                          phase active the first whose `enter` holds (no enter: holds) starts; when the
//!                          active phase's `exit` holds, the next later phase whose `enter` holds takes over.
//!                          The active phase's `overrides` are merged over the plan for the whole tick
//!                          (top-level keys replaced, null removes a key, `war` and `alarms` merged key by
//!                          key; the strategy keys, `active` and `_meta` are never overridden). A change is
//!                          kept in plan_state.json, noted in the change feed and raised as `phase_changed`
//!   checkpoints            [{"id","by":"YYYY.MM.DD","must":[..],"else"?}] judged once the date is reached:
//!                          every `must` holds -> met; one is false -> alarm and feed `checkpoint_missed`,
//!                          then `else` "phase:<id>" (or a bare phase id) switches to that phase,
//!                          "contingency:<id>" triggers that contingency, "revise" only raises the alarm;
//!                          an unknown `must` (and none false) waits. Each checkpoint is settled once
//!   contingencies          [{"id","when" (or "trigger"),"then":{"phase"?,"overrides"?} (or "phase" and
//!                          "overrides" beside "when"),"revise"?}]: when the trigger holds the contingency
//!                          becomes active (feed and alarm `contingency_triggered`; its phase is switched to
//!                          once) and its overrides apply over the phase's while it is active; it clears when
//!                          the trigger is false again (unknown leaves it as it is; one triggered by a missed
//!                          checkpoint stays)
//!   politics[].when        a gated pick (a law behind war support, ...) waits until its predicate holds
//!   decision_rules         [{"id","decision","when"?,"target"?,"max"?,"never"?}] tried before `decisions`:
//!                          taken when the game enables it and `when` holds; `target` picks the row of a
//!                          targeted decision (hoi4_decision_take target); `max` successful takes at most
//!                          (done-memory by id); "never": true blocks the decision in every path, the
//!                          `decisions` list included (`decision_forbidden`)
//!   event_rules            [{"event","option_name","when" (or "condition")?,"else_option_name"?,
//!                          "fallback_option_name"?,"fallback_index"?}]: the first rule of the open event
//!                          whose condition holds picks the option whose text (hoi4_events options) is
//!                          option_name, ignoring case, markup and punctuation (else the one option that
//!                          contains it); a false condition takes else_option_name or leaves the event to
//!                          the next rule; a name not offered falls back to fallback_option_name, then
//!                          fallback_index, then event_options; an unknown condition waits
//!
//! Memory across a server restart: `plan_state.json` next to plan.json (written by the runner
//! through a temp file and a rename whenever it changes; never edited by hand) keeps what is done
//! (politics, doctrines, production lines; construction_plan items queued, each with its game
//! hour), the running skip timers (a decision taken waits 30 days), the failure ledger, the
//! army_layout roles and the strategy layer (active phase, settled checkpoints, active
//! contingencies, last goal status). It is read once per server life when the game date is known, and ignored
//! when it names another campaign (player tag, the game's playthrough id, or `_meta.campaign`).
//! Work dated after the current game date is dropped (an earlier save was loaded). A note on
//! an ignored or unwritable file is in the run result as `plan_state`.
use crate::error::{Error, Result};
use crate::plan_predicate::{Cache, Ctx, Truth};
use serde_json::{json, Map, Value};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
    time::Duration,
};

/// Continuous management (alerts, templates, occupation, peace, resources, air plan, `manage`).
#[path = "plan_manage.rs"]
mod plan_manage;
/// Military domains (army groups, navy, naval invasions, casualty ratio, air superiority).
#[path = "plan_military.rs"]
mod plan_military;

/// Game time is counted in in-game hours (the `raw` value of the `date` object
/// that `hoi4_overview` returns). Every skip, cooldown and patience below is a
/// span of game hours, so the plan behaves the same at speed 1, 5 or paused.
pub type GameHour = i64;
/// One in-game day in [`GameHour`] units.
pub const DAY: GameHour = 24;

/// One tool call by name (a read, or an action with its own request id).
pub trait Tools {
    fn call(&self, name: &str, args: &Value) -> Result<Value>;
}

/// Times every call of the run (tool -> (calls, milliseconds)), reported as `timing`.
struct Timed<'a> {
    inner: &'a dyn Tools,
    spent: std::cell::RefCell<std::collections::BTreeMap<String, (u32, u128)>>,
}
impl Tools for Timed<'_> {
    fn call(&self, name: &str, args: &Value) -> Result<Value> {
        let t = std::time::Instant::now();
        let out = self.inner.call(name, args);
        let mut spent = self.spent.borrow_mut();
        let e = spent.entry(name.to_string()).or_insert((0, 0));
        e.0 += 1;
        e.1 += t.elapsed().as_millis();
        out
    }
}

/// What one tick did.
#[derive(Debug, PartialEq)]
pub enum Tick {
    Idle,
    Acted(String),
    /// An event the plan will not answer yet; the model may choose
    /// (`new` on the first tick that sees it).
    Waiting {
        event: String,
        new: bool,
    },
    Stop(String),
    /// `stop_on_date` reached: the runner is done (the text says how the pause went).
    DateStop(String),
    Nothing,
}

pub struct State {
    stop_on_failure: bool,
    /// The game hour of the current step (for the failure ledger).
    clock: GameHour,
    /// Failed orders by signature (tool + arguments): when they failed. The same order is not
    /// sent again after 3 failures in 10 game days (w24: the same selection and research orders
    /// failed again and again).
    failures: HashMap<String, Vec<GameHour>>,
    boot: u64,
    bad: HashMap<String, GameHour>,
    last_hour: GameHour,
    /// One-shot work done (`pol:..`, `prod:..`, `doc:..`): key -> game hour it was done.
    done: HashMap<String, GameHour>,
    seen: HashMap<String, GameHour>,
    seq: u64,
    sent: HashMap<String, (String, Value)>,
    war_assign: HashMap<String, i32>,
    war_stage: HashMap<String, u8>,
    war_cool: HashMap<String, GameHour>,
    /// per army: (target, main state, game hour) of the last progress seen
    war_since: HashMap<String, (i32, Option<i32>, GameHour)>,
    /// armies on direct move orders (their battle plan did not advance)
    war_move: HashSet<String>,
    /// armies resting (plan stopped for organisation or supply) since a game hour
    war_rest: HashMap<String, GameHour>,
    /// Armies bound to a `war.guard_states` post (army -> own state).
    war_guard: HashMap<String, i32>,
    /// Direct moves issued without the army changing state since: (target, state, count).
    war_repeat: HashMap<String, (i32, Option<i32>, u32)>,
    /// The enemy-held province of an own state last ordered for an army, and when.
    war_province: HashMap<String, (i32, GameHour)>,
    /// `construction_plan` items already queued (by `id`, else `order`) -> game hour queued.
    plan_built: HashMap<String, GameHour>,
    /// `army_layout`: name_hint -> the army that plays that role.
    layout: HashMap<String, String>,
    /// Continuous management (plan_manage.rs): wake alarms, digest lines, the peace conference.
    manage: plan_manage::Memory,
    /// Military domains (plan_military.rs): wake alarms, digest lines, loss samples.
    military: plan_military::Memory,
    /// divisions disbanded by the `disband` stage
    disbanded: u64,
    /// Refusals per politics pick (its done key): each refusal doubles the wait.
    pol_fails: HashMap<String, u32>,
    /// Political power when a pick was last refused: it is not tried again until the
    /// country has gained some (a refusal is usually a missing few points).
    pol_pp_at_fail: HashMap<String, f64>,
    centers: HashMap<i32, (f64, f64)>,
    con: ConState,
    /// stop_on_date: (target hour, outcome of the single pause attempt)
    date_stop: Option<(i64, String)>,
    stages: Stages,
    /// `plan_state.json`: the memory that survives a server restart.
    persist: Persist,
    /// goal, phases, checkpoints, contingencies (step 0)
    strategy: Strategy,
    /// this tick's predicate reads (game hour, cache): each read at most once per tick
    pcache: Option<(GameHour, Cache)>,
}

/// What `run` knows about the persisted memory next to plan.json.
#[derive(Default)]
struct Persist {
    /// The file was looked at (restored or ignored) once the game date was known.
    restored: bool,
    /// The campaign this memory belongs to: {"player","playthrough","campaign"}.
    identity: Option<Value>,
    /// The memory last written (nothing is written while it is unchanged).
    saved: Option<Value>,
    /// Why the file was ignored or could not be written (reported as `plan_state`).
    note: Option<String>,
}

/// What the research and focus stages saw this tick (the digest lines).
#[derive(Default)]
struct Stages {
    /// (digest line, a slot is idle and the plan has nothing left to start)
    research: Option<(String, bool)>,
    /// (digest line, no focus is active and the plan has nothing left to start)
    focus: Option<(String, bool)>,
    /// catalogue techs the fallback started during this run
    fallback_used: Vec<String>,
    /// short refusal reason per focus key, for `focus_blocked`
    focus_why: HashMap<String, String>,
}

/// Skip window in game hours: `skip_hours.<key>` of the plan, else `default`.
fn skip_hours(plan: &Value, key: &str, default: GameHour) -> GameHour {
    plan["skip_hours"][key]
        .as_i64()
        .filter(|h| *h > 0)
        .unwrap_or(default)
}

/// Memory of the construction stage (all times in game hours).
#[derive(Default)]
struct ConState {
    /// free slots read by hoi4_state_slots: (free, hour read), decremented per queued building
    free: HashMap<i64, (u32, GameHour)>,
    /// largest `count` worth trying in a state (a partial add proved it)
    cap: HashMap<i64, i64>,
    /// consecutive transient failures per state
    fails: HashMap<i64, u32>,
    /// state name -> id, from hoi4_states
    names: HashMap<String, i64>,
    /// own states (hour read, ids), for the optional fallback
    own: Option<(GameHour, Vec<i64>)>,
    /// rotates the first candidate so the listed states take turns
    rr: usize,
    pause_until: GameHour,
    probe_off_until: GameHour,
    queued_total: u64,
    last_queue: Option<GameHour>,
    baseline: Option<GameHour>,
    /// the last tick queued a building: leave the next tick to armies and war (the queue
    /// depth keeps the factories busy meanwhile)
    yield_next: bool,
    /// this tick's digest line (None when the stage was not reached)
    digest: Option<String>,
}
impl Default for State {
    /// Request ids carry the start time so a restarted server never reuses
    /// an id that an earlier run already bound to a different action.
    fn default() -> Self {
        Self {
            stop_on_failure: false,
            clock: 0,
            failures: HashMap::new(),
            boot: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            bad: HashMap::new(),
            last_hour: 0,
            done: HashMap::new(),
            seen: HashMap::new(),
            seq: 0,
            sent: HashMap::new(),
            war_assign: HashMap::new(),
            war_stage: HashMap::new(),
            war_cool: HashMap::new(),
            war_since: HashMap::new(),
            war_move: HashSet::new(),
            war_rest: HashMap::new(),
            war_guard: HashMap::new(),
            war_repeat: HashMap::new(),
            war_province: HashMap::new(),
            plan_built: HashMap::new(),
            layout: HashMap::new(),
            manage: Default::default(),
            military: Default::default(),
            disbanded: 0,
            pol_fails: HashMap::new(),
            pol_pp_at_fail: HashMap::new(),
            centers: HashMap::new(),
            con: ConState::default(),
            date_stop: None,
            stages: Stages::default(),
            persist: Persist::default(),
            strategy: Strategy::default(),
            pcache: None,
        }
    }
}

fn skip_key(kind: &str, key: &str) -> String {
    format!("{kind}:{key}")
}
impl State {
    fn skip(&mut self, now: GameHour, kind: &str, key: &str, hours: GameHour) {
        self.bad.insert(skip_key(kind, key), now + hours);
    }
    fn skipped(&self, now: GameHour, kind: &str, key: &str) -> bool {
        self.bad
            .get(&skip_key(kind, key))
            .is_some_and(|until| *until > now)
    }
    /// One action with a fresh id; an uncertain dispatch is settled by
    /// retrying its own id once. If readback remains uncertain, stop the
    /// runner before it can try a fallback under a new id.
    fn act(
        &mut self,
        tools: &dyn Tools,
        lease: &str,
        name: &str,
        args: Value,
        tag: &str,
    ) -> Result<(bool, String)> {
        let signature = format!("{name} {args}");
        let window = 10 * DAY;
        let clock = self.clock;
        if let Some(times) = self.failures.get_mut(&signature) {
            times.retain(|t| clock - *t < window);
            if times.len() >= 3 {
                return Ok((
                    false,
                    "skipped: the same order failed 3 times in 10 game days".into(),
                ));
            }
        }
        self.seq += 1;
        let mut args = args;
        if let Some(object) = args.as_object_mut() {
            object.insert(
                "request_id".into(),
                json!(format!("plan-{tag}-{}-{}", self.boot, self.seq)),
            );
            object.insert("lease_token".into(), json!(lease));
        }
        let id = args["request_id"].as_str().unwrap_or("").to_string();
        self.sent.insert(id, (name.to_string(), args.clone()));
        let result = self.dispatch(tools, name, &args);
        match &result {
            Ok((false, _)) => self.failures.entry(signature).or_default().push(clock),
            Ok((true, _)) => {
                self.failures.remove(&signature);
            }
            Err(_) => {}
        }
        result
    }
    fn dispatch(&mut self, tools: &dyn Tools, name: &str, args: &Value) -> Result<(bool, String)> {
        let args = args.clone();
        let mut result = tools.call(name, &args);
        if matches!(&result, Err(e) if e.code == "action_uncertain") {
            // Another dispatch of this run is unresolved and blocks input: settle
            // it by retrying its own request id, then send this one again.
            let blocker = match &result {
                Err(e) => e
                    .message
                    .split("Unresolved dispatch ")
                    .nth(1)
                    .and_then(|rest| rest.split_whitespace().next())
                    .map(String::from),
                _ => None,
            };
            if let Some((blocked_name, blocked_args)) =
                blocker.and_then(|b| self.sent.get(&b).cloned())
            {
                let _ = tools.call(&blocked_name, &blocked_args);
            }
            result = tools.call(name, &args);
        }
        match result {
            Err(e) if e.code == "action_uncertain" => Err(e),
            Err(e) if self.stop_on_failure => Err(e),
            Err(e) => {
                let mut text = format!("{}: {}", e.code, e.message);
                let mut end = text.len().min(160);
                while !text.is_char_boundary(end) {
                    end -= 1;
                }
                text.truncate(end);
                Ok((false, text))
            }
            Ok(body) => {
                let status = body["receipt"]["status"].as_str().unwrap_or("").to_string();
                match status.as_str() {
                    "Verified" => Ok((true, status)),
                    "NotApplied" | "Diverged" if self.stop_on_failure => Err(Error::new(
                        "plan_action_failed",
                        format!("{name} returned {status}; standing plan stopped"),
                    )),
                    "NotApplied" | "Diverged" => Ok((false, status)),
                    _ => Err(Error::new(
                        "action_uncertain",
                        format!("{name} returned no terminal receipt; settle request {} before new input", args["request_id"]),
                    )),
                }
            }
        }
    }
}

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or("")
}
/// The stable `id` of a plan list item (a non-empty string or a number), used for its
/// done-memory instead of its position so a revised or reordered list keeps what was done.
fn item_id(item: &Value) -> Option<String> {
    match &item["id"] {
        Value::String(id) if !id.trim().is_empty() => Some(id.trim().to_string()),
        Value::Number(id) => Some(id.to_string()),
        _ => None,
    }
}
/// Loose own divisions standing in `state_id` (hoi4_armies unassigned_by_state).
fn unassigned_in(roster: &Value, state_id: i64) -> u64 {
    list(roster, "unassigned_by_state")
        .iter()
        .filter(|r| r["state_id"].as_i64() == Some(state_id))
        .filter_map(|r| r["divisions"].as_u64())
        .sum()
}
fn list<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    v[key].as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn strings(v: &Value, key: &str) -> Vec<String> {
    list(v, key)
        .iter()
        .filter_map(|s| s.as_str().map(String::from))
        .collect()
}

/// One pass over the plan: the first thing that can be done is done.
pub fn tick(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Tick> {
    state.clock = now;
    if plan["active"] == json!(false) {
        return Ok(Tick::Idle);
    }
    state.stop_on_failure = match plan.get("stop_on_failure") {
        None => false,
        Some(Value::Bool(value)) => *value,
        _ => {
            return Err(Error::new(
                "invalid_plan",
                "stop_on_failure must be boolean",
            ))
        }
    };
    // stop_on_date comes before everything, events included: once the date is
    // reached the runner pauses once (journal-safe) and then never acts again.
    if let Some((target, label)) = crate::alarms::stop_on_date(plan)? {
        let pause = match plan.get("stop_on_date_pause") {
            None | Some(Value::Bool(true)) => true,
            Some(Value::Bool(false)) => false,
            _ => {
                return Err(Error::new(
                    "invalid_plan",
                    "stop_on_date_pause must be boolean",
                ))
            }
        };
        let known = matches!(&state.date_stop, Some((t, _)) if *t == target);
        if known || now >= target {
            if !known {
                let outcome = date_stop_pause(state, tools, lease, pause);
                state.date_stop = Some((target, outcome));
            }
            let outcome = state.date_stop.as_ref().map_or("", |(_, o)| o.as_str());
            let at = i32::try_from(now)
                .ok()
                .and_then(|raw| crate::calendar::decode(raw).ok())
                .map_or_else(|| format!("hour {now}"), |d| d.text);
            return Ok(Tick::DateStop(format!(
                "stop_on_date {label} reached (game date {at}); pause: {outcome}"
            )));
        }
    }
    state.con.digest = None;
    state.stages.research = None;
    state.stages.focus = None;
    state.con.baseline.get_or_insert(now);
    // 0. strategy (reads only): goal, phases, checkpoints, contingencies; the active phase's and
    // contingencies' overrides are merged over the plan for the rest of this tick. One predicate
    // context per tick: every read behind a predicate happens at most once.
    state.pcache = None;
    let merged = strategy_step(state, tools, plan, now);
    let plan = merged.as_ref().unwrap_or(plan);
    // 1. events (they block every other input)
    let events = tools.call("hoi4_events", &json!({}))?;
    if events["peace_conference_open"] == json!(true) {
        // the plan's `peace` demands, else the stop (plan_manage.rs)
        return plan_manage::peace(state, tools, plan, lease, now);
    }
    if let Some(top) = list(&events, "events").last() {
        let key = text(&top["event"]).to_string();
        let new = !state.seen.contains_key(&key);
        let first = *state.seen.entry(key.clone()).or_insert(now);
        let auto = plan["auto_events"] != json!(false);
        let news = key.contains("news") || list(top, "options").len() == 1;
        let waited = now.saturating_sub(first);
        let patience = plan["wait_events_days"]
            .as_i64()
            .map_or(2 * DAY, |days| days.saturating_mul(DAY));
        // A choice event is the model's decision unless the plan names an option for it
        // or explicitly allows answering every event ("auto_events": "all").
        // `event_rules` come first: an option by its name; an unknown condition waits.
        let ruled = event_rule_choice(state, tools, plan, &key, top, now);
        if matches!(ruled, Some(None)) {
            return Ok(Tick::Waiting { event: key, new });
        }
        let (ruled, how) = ruled
            .flatten()
            .map_or((None, String::new()), |(i, how)| (Some(i), how));
        let chosen = plan["event_options"][&key].as_u64().or_else(|| {
            // {"event_prefix_options": {"DOD_": "last"}}: a rule for a whole family of events
            plan["event_prefix_options"]
                .as_object()?
                .iter()
                .find_map(|(prefix, v)| {
                    key.starts_with(prefix.as_str()).then(|| {
                        if v == "last" {
                            list(top, "options").len().saturating_sub(1) as u64
                        } else {
                            v.as_u64().unwrap_or(0)
                        }
                    })
                })
        });
        let chosen = ruled.or(chosen);
        let all = plan["auto_events"] == json!("all");
        if auto && (news || chosen.is_some() || (waited > patience && all)) {
            let option = chosen.unwrap_or(0);
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_event_choose",
                json!({"event": key, "option": option}),
                "ev",
            )?;
            return Ok(Tick::Acted(format!(
                "event {key} option {option}{how}: {ok} {why}"
            )));
        }
        return Ok(Tick::Waiting { event: key, new });
    }
    // 1a. war first: at war a game day passes in about 2.5 s at speed 3, and one construction
    // step can take 100 s (w15, 6 Nov 2026: 111-day stalls while the tick built factories), so
    // the war executive acts before everything else unless the plan says "war": {"first": false}.
    let war_on = plan["war"].is_object() && plan["war"]["enabled"] != json!(false);
    if war_on && plan["war"]["first"] != json!(false) {
        if let Some(step) = layout_step(state, tools, plan, lease, now)? {
            return Ok(Tick::Acted(step));
        }
        if let Some(step) = war_step(state, tools, plan, lease, now)? {
            return Ok(Tick::Acted(step));
        }
        if let Some(step) = construction_plan_step(state, tools, plan, lease, now)? {
            return Ok(Tick::Acted(step));
        }
        if let Some(step) = armies_step(state, tools, plan, lease, now)? {
            return Ok(Tick::Acted(step));
        }
        // Home divisions disbanded early refill the fighting armies' equipment (w16: the
        // African infantry fought at 13-33% for most of the year).
        if plan["disband"].is_object() {
            if let Some(step) = disband_step(state, tools, plan, lease, now)? {
                return Ok(Tick::Acted(step));
            }
        }
    }
    // 2. research
    let research = tools.call("hoi4_research", &json!({"limit": 1}))?;
    let slots: Vec<Option<String>> = list(&research, "slots")
        .iter()
        .map(|s| s["researching"].as_str().map(String::from))
        .collect();
    if let Some(empty) = slots.iter().position(Option::is_none) {
        let idle_slots = slots.iter().filter(|s| s.is_none()).count();
        let mut done: HashSet<String> = HashSet::new();
        let mut offset = 0;
        loop {
            let page = tools.call("hoi4_research", &json!({"offset": offset, "limit": 100}))?;
            done.extend(
                list(&page, "researched")
                    .iter()
                    .filter_map(|t| t["id"].as_str().map(String::from)),
            );
            match page["next_offset"].as_u64() {
                Some(next) if next > offset => offset = next,
                _ => break,
            }
        }
        let busy: HashSet<String> = slots.iter().flatten().cloned().collect();
        let year = game_year(now);
        // Techs startable now (prerequisites done, from the game files); None when the server
        // cannot say (then every key is tried as before).
        let available = tools
            .call("hoi4_technology_list", &json!({"available": true}))
            .ok()
            .filter(|v| v["available_only"] == json!(true));
        let startable: Option<HashSet<String>> = available
            .as_ref()
            .map(|v| strings(v, "technologies").into_iter().collect());
        let start_year = |t: &str| -> Option<i64> {
            available
                .as_ref()
                .and_then(|v| v["start_years"][t].as_i64())
        };
        // `research_path`: [{"year":1936,"techs":[...]}, ...] in order; an entry counts from its
        // year on (the plan's forward view: industry first, nothing ahead of its time).
        let mut priority: Vec<String> = Vec::new();
        if let Some(path) = plan["research_path"].as_array() {
            for step in path {
                let due = match step["not_before"].as_str().and_then(date_hour) {
                    Some(at) if year.is_some() => now >= at,
                    _ => year.is_none_or(|y| step["year"].as_i64().unwrap_or(0) <= y),
                };
                if due {
                    priority.extend(strings(step, "techs"));
                }
            }
        }
        priority.extend(strings(plan, "research_priority"));
        let open: Vec<&String> = priority
            .iter()
            .filter(|t| !done.contains(*t) && !busy.contains(*t))
            .filter(|t| startable.as_ref().is_none_or(|s| s.contains(*t)))
            .collect();
        let waiting = open
            .iter()
            .filter(|t| state.skipped(now, "tech", t))
            .count();
        let candidates: Vec<String> = open
            .into_iter()
            .filter(|t| !state.skipped(now, "tech", t))
            .take(3)
            .cloned()
            .collect();
        let mut refused: Vec<String> = Vec::new();
        for tech in candidates {
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_research_start",
                json!({"technology": tech, "slot": empty}),
                "rs",
            )?;
            if ok {
                state.stages.research = Some((
                    format!(
                        "research: started {tech} slot {empty} ({idle_slots} slot(s) were idle)"
                    ),
                    false,
                ));
                return Ok(Tick::Acted(format!("research {tech} slot {empty}")));
            }
            state.skip(
                now,
                "tech",
                &tech,
                tech_skip(&why, skip_hours(plan, "tech", 3 * DAY)),
            );
            refused.push(format!("{tech}:{}", why.split(':').next().unwrap_or("")));
        }
        // Nothing from the priority list is startable: never leave a slot idle.
        // Walk the technology catalog; the game refuses ineligible ones before
        // any input, so a few attempts per tick are cheap. Plans can switch this
        // off ("priority_only") or restrict it to wanted key prefixes/words.
        let fallback_on = plan["research_fallback"] != json!(false)
            && plan["research_fallback"] != json!("priority_only");
        let prefixes = strings(plan, "research_fallback_prefixes");
        let categories = strings(plan, "research_fallback_categories");
        // Words the fallback never takes (default: ships; a plan sets [] to allow them).
        let exclude: Vec<String> = match plan.get("research_fallback_exclude") {
            Some(v) if v.is_array() => strings(plan, "research_fallback_exclude"),
            _ => [
                "ship",
                "hull",
                "naval",
                "carrier",
                "submarine",
                "destroyer",
                "cruiser",
                "battleship",
                "torpedo",
                "convoy",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        };
        let wanted = |tech: &str| {
            !exclude.iter().any(|x| tech.contains(x.as_str()))
                && ((prefixes.is_empty() && categories.is_empty())
                    || prefixes.iter().any(|p| tech.starts_with(p.as_str()))
                    || categories.iter().any(|c| tech.contains(c.as_str())))
        };
        if fallback_on && startable.is_some() {
            // From the techs startable now, never ahead of their year, in the plan's order of
            // interest (research_fallback_prefixes: industry first in the plans), oldest first.
            let rank = |t: &str| {
                prefixes
                    .iter()
                    .position(|p| t.starts_with(p.as_str()))
                    .unwrap_or(prefixes.len())
            };
            let mut pool: Vec<String> = startable
                .iter()
                .flatten()
                .filter(|t| !busy.contains(*t) && wanted(t) && !state.skipped(now, "tech", t))
                .filter(|t| match (year, start_year(t)) {
                    (Some(y), Some(sy)) => sy <= y,
                    _ => true,
                })
                .cloned()
                .collect();
            pool.sort_by_key(|t| (rank(t), start_year(t).unwrap_or(0), t.clone()));
            for tech in pool.into_iter().take(3) {
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_research_start",
                    json!({"technology": tech, "slot": empty}),
                    "rf",
                )?;
                if ok {
                    state.stages.fallback_used.push(tech.clone());
                    state.stages.research = Some((
                        format!("research: started {tech} slot {empty} (available, in time, plan order)"),
                        false,
                    ));
                    return Ok(Tick::Acted(format!(
                        "research {tech} slot {empty} (fallback)"
                    )));
                }
                state.skip(
                    now,
                    "tech",
                    &tech,
                    tech_skip(&why, skip_hours(plan, "tech_fallback", 7 * DAY)),
                );
            }
        } else if fallback_on && plan["research_blind_catalogue"] == json!(true) {
            // The old catalogue walk (every key tried by input) only on request: without the
            // available list it opened the research tree for techs the game could never take
            // (w24: 26 refusals). A server without the list leaves the slot to the player.
            let mut tried = 0;
            let mut offset = 0;
            'catalog: loop {
                let page = tools.call(
                    "hoi4_technology_list",
                    &json!({"offset": offset, "limit": 100}),
                )?;
                for tech in strings(&page, "technologies") {
                    if done.contains(&tech)
                        || busy.contains(&tech)
                        || !wanted(&tech)
                        || state.skipped(now, "tech", &tech)
                    {
                        continue;
                    }
                    let (ok, _) = state.act(
                        tools,
                        lease,
                        "hoi4_research_start",
                        json!({"technology": tech, "slot": empty}),
                        "rf",
                    )?;
                    if ok {
                        state.stages.fallback_used.push(tech.clone());
                        state.stages.research = Some((
                            format!("research: started {tech} slot {empty} from the catalogue fallback (priority list had nothing startable)"),
                            false,
                        ));
                        return Ok(Tick::Acted(format!(
                            "research {tech} slot {empty} (fallback)"
                        )));
                    }
                    state.skip(
                        now,
                        "tech",
                        &tech,
                        skip_hours(plan, "tech_fallback", 7 * DAY),
                    );
                    tried += 1;
                    if tried >= 6 {
                        break 'catalog;
                    }
                }
                match page["next_offset"].as_u64() {
                    Some(next) if next > offset => offset = next,
                    _ => break,
                }
            }
        }
        let fallback = if !fallback_on {
            "catalogue fallback off"
        } else if prefixes.is_empty() && categories.is_empty() {
            "catalogue fallback found nothing startable"
        } else {
            "restricted catalogue fallback found nothing startable"
        };
        state.stages.research = Some((
            format!(
                "research: {idle_slots} slot(s) idle, plan exhausted ({} priority done, {} on skip timer, refused now [{}]; {fallback})",
                priority.iter().filter(|t| done.contains(*t)).count(),
                waiting,
                refused.join(", ")
            ),
            true,
        ));
    } else if !slots.is_empty() {
        state.stages.research =
            Some((format!("research: all {} slot(s) busy", slots.len()), false));
    }
    // 3. focus
    let focus = tools.call("hoi4_focus_completed", &json!({"limit": 100}))?;
    if let Some(active) = focus["active_key"].as_str() {
        state.stages.focus = Some((format!("focus: active {active}"), false));
    } else {
        let completed = strings(&focus, "completed");
        let path = strings(plan, "focus_path");
        let fallback = strings(plan, "focus_fallback");
        let startable = |state: &State, keys: &[String]| -> Vec<String> {
            keys.iter()
                .filter(|f| !completed.contains(*f) && !state.skipped(now, "focus", f))
                .cloned()
                .collect()
        };
        let mut candidates: Vec<String> = startable(state, &path).into_iter().take(8).collect();
        candidates.extend(startable(state, &fallback).into_iter().take(4));
        for key in candidates {
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_focus_start",
                json!({"focus": key}),
                "fo",
            )?;
            if ok {
                state.stages.focus_why.remove(&key);
                state.stages.focus = Some((format!("focus: started {key}"), false));
                return Ok(Tick::Acted(format!("focus {key}")));
            }
            // A focus the game disables (requirements, exclusive branch) waits 10 game days
            // (6 game hours was under a second at speed 3) and does not hold up the next key.
            // Anything else (a torn read, a readback) ends the focus step for this tick: going on
            // to the next key started focuses out of plan order (w24).
            let blocked = why.contains("focus_not_available")
                || why.contains("prerequisite")
                || why.contains("exclusive")
                || why.contains("invalid_target");
            let hours = if blocked {
                skip_hours(plan, "focus_blocked", 10 * DAY)
            } else {
                skip_hours(plan, "focus", DAY)
            };
            state.skip(now, "focus", &key, hours);
            let code = why.split(':').next().unwrap_or("refused").to_string();
            state.stages.focus_why.insert(key, code);
            if !blocked {
                break;
            }
        }
        if !path.is_empty() || !fallback.is_empty() {
            let left = startable(state, &path).len() + startable(state, &fallback).len();
            let all_done = path.iter().chain(&fallback).all(|f| completed.contains(f));
            let blocked = focus_blocked(state, plan, now);
            let line = if all_done {
                "focus: none active, focus_path and focus_fallback are all completed".to_string()
            } else {
                format!(
                    "focus: none active, nothing started; blocked [{}]",
                    blocked.join(", ")
                )
            };
            state.stages.focus = Some((line, left == 0));
        }
    }
    // 4. politics (one-shot picks)
    let mut political_power: Option<f64> = None;
    for (index, pick) in list(plan, "politics").iter().enumerate() {
        let key = item_id(pick).map_or_else(
            || format!("pol:{index}:{}", text(&pick["idea"])),
            |id| format!("pol:id:{id}"),
        );
        if state.done.contains_key(&key) || state.skipped(now, "pol", &key) {
            continue;
        }
        // A gated pick (a law behind war support or a war) waits until its gate holds.
        if !pick["when"].is_null() && !judge(state, tools, &pick["when"], now).is_true() {
            continue;
        }
        if political_power.is_none() {
            political_power = tools
                .call("hoi4_economy", &json!({}))
                .ok()
                .and_then(|e| e["political_power"]["raw_signed"].as_f64())
                .map(|raw| raw / 100_000.0);
        }
        // Not before the country has gained 15 points since the last refusal, or the
        // pick's own "cost" when the plan names it.
        if let Some(pp) = political_power {
            let need = pick["cost"].as_f64().unwrap_or(0.0);
            let since = state
                .pol_pp_at_fail
                .get(&key)
                .map_or(f64::MIN, |p| p + 15.0);
            if pp < need || pp < since {
                continue;
            }
        }
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_politics_choose",
            json!({"category": pick["category"], "slot": pick["slot"], "idea": pick["idea"]}),
            "po",
        )?;
        if ok || why.contains("already holds") {
            state.pol_fails.remove(&key);
            state.pol_pp_at_fail.remove(&key);
            state.done.insert(key, now);
        } else {
            if let Some(pp) = political_power {
                state.pol_pp_at_fail.insert(key.clone(), pp);
            }
            // Usually short of political power. Each attempt opens the politics screen, so
            // back off (3, 6, 12 ... 30 days) instead of retrying every game day.
            let fails = state.pol_fails.entry(key.clone()).or_insert(0);
            *fails += 1;
            let days = (3u64 << (*fails - 1).min(4)).min(30);
            state.skip(now, "pol", &key, days as GameHour * DAY);
        }
        return Ok(Tick::Acted(format!(
            "politics {} : {ok} {why}",
            text(&pick["idea"])
        )));
    }
    // 5. decisions: `decision_rules` (guarded, targeted, counted, never-take) before the plain
    // `decisions` list
    if let Some(step) = decisions_step(state, tools, plan, lease, now)? {
        return Ok(Tick::Acted(step));
    }
    // 6. training: one template or a list, with training_rules (plan_manage.rs)
    if !plan["training"].is_null() {
        if let Some(step) = plan_manage::training_step(state, tools, plan, lease, now)? {
            return Ok(Tick::Acted(step));
        }
    }
    // 6b. military factories on no line go to the first priority line that exists
    let economy = tools.call("hoi4_economy", &json!({}))?;
    let total_military = economy["factory_trigger_count"]["military"]
        .as_i64()
        .unwrap_or(0);
    let lines = tools.call("hoi4_production_lines", &json!({"limit": 128}))?;
    let military: Vec<(i64, String, i64)> = list(&lines, "lines")
        .iter()
        .filter(|l| l["line_kind"] == json!("military"))
        .map(|l| {
            (
                l["entry_index"].as_i64().unwrap_or(-1),
                text(&l["equipment_id"]).to_string(),
                l["factory_counters"]["requested_factories_raw_signed"]
                    .as_i64()
                    .unwrap_or(0),
            )
        })
        .collect();
    let idle = total_military - military.iter().map(|m| m.2).sum::<i64>();
    if idle >= plan["production_min_idle"].as_i64().unwrap_or(2)
        && !state.skipped(now, "prodset", "x")
    {
        let mut priority = strings(plan, "production_priority");
        if priority.is_empty() {
            priority = [
                "infantry_equipment",
                "support_equipment",
                "artillery_equipment",
            ]
            .map(String::from)
            .to_vec();
        }
        for want in priority {
            if let Some((index, equipment, factories)) = military
                .iter()
                .find(|m| !m.1.is_empty() && m.1.starts_with(&want))
            {
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_production_set",
                    json!({"line": index, "equipment": equipment, "factories": factories + idle}),
                    "ps",
                )?;
                if !ok {
                    state.skip(now, "prodset", "x", 2 * DAY);
                }
                return Ok(Tick::Acted(format!(
                    "production_set {equipment} {factories}+{idle}: {ok} {why}"
                )));
            }
        }
    }
    // 7. production lines added once. The done-memory is kept in plan_state.json, and a line
    // that already exists in the game counts as done too (a restart on 1 Nov 2026, before the
    // memory was persisted, re-added five lines).
    let mut existing: Option<HashSet<String>> = None;
    for line in list(plan, "production") {
        let equipment = text(&line["equipment"]).to_string();
        if state.done.contains_key(&format!("prod:{equipment}"))
            || state.skipped(now, "prod", &equipment)
            || !reached(line, now)
        {
            continue;
        }
        if existing.is_none() {
            let read = tools
                .call("hoi4_production_lines", &json!({"limit": 200}))
                .unwrap_or(Value::Null);
            existing = Some(
                list(&read, "lines")
                    .iter()
                    .filter_map(|l| l["equipment_id"].as_str().map(str::to_string))
                    .collect(),
            );
        }
        if existing.as_ref().is_some_and(|e| e.contains(&equipment)) {
            state.done.insert(format!("prod:{equipment}"), now);
            continue;
        }
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_production_add",
            json!({"equipment": equipment, "factories": line["factories"].as_u64().unwrap_or(5)}),
            "pr",
        )?;
        if ok {
            state.done.insert(format!("prod:{equipment}"), now);
        } else {
            state.skip(now, "prod", &equipment, 14 * DAY);
        }
        return Ok(Tick::Acted(format!("production {equipment}: {ok} {why}")));
    }
    // 7b. the triggered build order, in peace (and at war when the war does not run first;
    // a war-first tick ran it right after the war step). Its items go before the open
    // `construction` jobs so those cannot take the factories an ordered item waits for.
    if !(war_on && plan["war"]["first"] != json!(false)) {
        if let Some(step) = construction_plan_step(state, tools, plan, lease, now)? {
            return Ok(Tick::Acted(step));
        }
    }
    // 8. construction with idle civilian factories. It comes before the armies
    // (which always find something to retry) so they cannot starve it.
    if let Some(step) = construction_step(state, tools, plan, lease, now, &economy)? {
        return Ok(Tick::Acted(step));
    }
    // 8a. special projects, doctrines, air missions in war (one action at most)
    if let Some(step) = extras_step(state, tools, plan, lease, now)? {
        return Ok(Tick::Acted(step));
    }
    // 8a'. continuous management (plan_manage.rs): alerts, templates, occupation, resources,
    // air_plan, manage
    if let Some(step) = plan_manage::step(state, tools, plan, lease, now)? {
        return Ok(Tick::Acted(step));
    }
    // 8a''. military domains (plan_military.rs): naval invasions, casualty ratio, air
    // superiority, army groups, navy
    if let Some(step) = plan_military::step(state, tools, plan, lease, now)? {
        return Ok(Tick::Acted(step));
    }
    // 9. armies (here unless the war runs first, which runs them right after itself)
    if !(war_on && plan["war"]["first"] != json!(false)) {
        if let Some(step) = armies_step(state, tools, plan, lease, now)? {
            return Ok(Tick::Acted(step));
        }
    }
    // 8b. war executive (when it does not run first)
    if war_on && plan["war"]["first"] == json!(false) {
        if let Some(step) = war_step(state, tools, plan, lease, now)? {
            return Ok(Tick::Acted(step));
        }
    }
    if plan["disband"].is_object() && !(war_on && plan["war"]["first"] != json!(false)) {
        if let Some(step) = disband_step(state, tools, plan, lease, now)? {
            return Ok(Tick::Acted(step));
        }
    }
    Ok(Tick::Nothing)
}

/// 9. every division in an army of sensible size, generals only for real armies. At war it runs
///    right after the war step: the war cannot start before its armies exist (w15: the opening's
///    armies came a month late behind research, production and construction ticks).
fn armies_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    if plan["armies"].is_object() {
        let cfg = &plan["armies"];
        let max = cfg["max"].as_u64().unwrap_or(24);
        let min_general = cfg["min_for_general"].as_u64().unwrap_or(8);
        let roster = tools.call("hoi4_armies", &json!({}))?;
        // "states": [ids] limits army creation to those states, in that order (the theatre
        // first), so home garrisons do not cost the war its ticks.
        let only: Vec<i64> = list(cfg, "states")
            .iter()
            .filter_map(Value::as_i64)
            .collect();
        let mut pending: Vec<&Value> = list(&roster, "unassigned_by_state").iter().collect();
        if !only.is_empty() {
            pending.retain(|e| e["state_id"].as_i64().is_some_and(|id| only.contains(&id)));
            pending.sort_by_key(|e| {
                let id = e["state_id"].as_i64().unwrap_or(0);
                only.iter().position(|o| *o == id).unwrap_or(usize::MAX)
            });
        }
        for entry in pending {
            let Some(id) = entry["state_id"].as_i64() else {
                continue;
            };
            let key = id.to_string();
            if state.skipped(now, "armycreate", &key) {
                continue;
            }
            // "absorb": true|"assign" folds unassigned divisions into the army that
            // stands there (or makes a new one when none does) without deleting it;
            // "assign" never falls back to the merging absorb_existing path.
            let assign = cfg["absorb"] == json!(true) || cfg["absorb"] == json!("assign");
            let assigned = if assign {
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_army_assign",
                    json!({"state_id": id}),
                    "as",
                )?;
                // an unmapped control or an older backend: try the older paths
                let unavailable = !ok
                    && cfg["absorb"] == json!(true)
                    && (why.contains("capability_unavailable") || why.contains("unknown"));
                (!unavailable).then_some((ok, why))
            } else {
                None
            };
            if let Some((ok, why)) = assigned {
                state.skip(
                    now,
                    "armycreate",
                    &key,
                    if ok {
                        DAY
                    } else if why.contains("selection_mismatch") {
                        20 * DAY
                    } else {
                        2 * DAY
                    },
                );
                return Ok(Some(format!("army_assign state {id}: {ok} {why}")));
            }
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_army_create",
                json!({"state_id": id}),
                "ac",
            )?;
            let (ok, why) = if !ok
                && why.contains("Divisions of an existing")
                && cfg["absorb"] == json!(true)
            {
                state.act(
                    tools,
                    lease,
                    "hoi4_army_create",
                    json!({"state_id": id, "absorb_existing": true}),
                    "ab",
                )?
            } else {
                (ok, why)
            };
            // Whether or not it worked, give the game time before trying the
            // same state again (a box selection can miss stacked divisions).
            state.skip(
                now,
                "armycreate",
                &key,
                if ok {
                    DAY
                } else if why.contains("selection_mismatch") {
                    20 * DAY
                } else {
                    // a box that selects nothing does not fix itself in two days (live
                    // 6 Nov 2026: Somaliland retried every tick and starved the war stage)
                    10 * DAY
                },
            );
            return Ok(Some(format!("army_create state {id}: {ok} {why}")));
        }
        for army in list(&roster, "armies") {
            let name = text(&army["army"]);
            let count = army["division_count"].as_u64().unwrap_or(0);
            if count > max && !state.skipped(now, "armysplit", name) {
                let chunk = (count - max).clamp(1, 19);
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_army_split",
                    json!({"army": name, "count": chunk}),
                    "sp",
                )?;
                if !ok {
                    state.skip(now, "armysplit", name, 2 * DAY);
                }
                return Ok(Some(format!("army_split {name} {chunk}: {ok} {why}")));
            }
        }
        // Consolidate: fold the smallest army below `min_for_general` into the army that ends
        // fullest without passing `max` (hoi4_army_merge), so generals lead real armies.
        // An army with battle-plan orders is never emptied (the game deletes its orders).
        if cfg["consolidate"] != json!(false) {
            let mut sizes: Vec<(String, u64, bool)> = list(&roster, "armies")
                .iter()
                .map(|a| {
                    (
                        text(&a["army"]).to_string(),
                        a["division_count"].as_u64().unwrap_or(0),
                        a["battle_plan_orders"].as_u64().unwrap_or(0) > 0,
                    )
                })
                .filter(|(_, n, _)| *n > 0)
                .collect();
            sizes.sort_by_key(|(_, n, _)| *n);
            let source = sizes
                .iter()
                .find(|(name, n, orders)| {
                    *n < min_general && !orders && !state.skipped(now, "armymerge", name)
                })
                .cloned();
            if let Some((source, n, _)) = source {
                let target = sizes
                    .iter()
                    .filter(|(name, m, _)| *name != source && n + m <= max)
                    .max_by_key(|(_, m, _)| *m)
                    .cloned();
                if let Some((target, _, _)) = target {
                    let (ok, why) = state.act(
                        tools,
                        lease,
                        "hoi4_army_merge",
                        json!({"source": source, "target": target}),
                        "mg",
                    )?;
                    if !ok {
                        state.skip(now, "armymerge", &source, 2 * DAY);
                    }
                    return Ok(Some(format!("army_merge {source} -> {target}: {ok} {why}")));
                }
            }
        }
        let generals = strings(cfg, "generals");
        let used: HashSet<String> = list(&roster, "armies")
            .iter()
            .filter_map(|a| a["commander"].as_str().map(String::from))
            .collect();
        let mut spare = generals.iter().filter(|g| !used.contains(*g));
        for army in list(&roster, "armies") {
            let name = text(&army["army"]);
            if army["commander"].is_null()
                && army["division_count"].as_u64().unwrap_or(0) >= min_general
                && !state.skipped(now, "armygeneral", name)
            {
                let Some(general) = spare.next() else { break };
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_army_commander",
                    json!({"army": name, "general": general}),
                    "gn",
                )?;
                state.skip(
                    now,
                    "armygeneral",
                    name,
                    if ok { 14 * DAY } else { 2 * DAY },
                );
                return Ok(Some(format!("general {general} -> {name}: {ok} {why}")));
            }
        }
    }
    Ok(None)
}

/// Calendar year of a raw game hour (1936.01.01 is about 6936 * 8760: the engine counts from
/// year -5000); None for test clocks and other implausible values.
fn game_year(now: GameHour) -> Option<i64> {
    let y = now / 8760 - 5000;
    (1900..2100).contains(&y).then_some(y)
}
/// The game hour at the start of date `text` ("1936.03.05" or "1936-03-05"); the game's calendar
/// has 365 days and the raw hour counts from year -5000.
fn date_hour(text: &str) -> Option<GameHour> {
    let parts: Vec<i64> = text
        .split(['.', '-'])
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    let (y, m, d) = (*parts.first()?, *parts.get(1)?, *parts.get(2)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    const DAYS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let before: i64 = DAYS[..(m - 1) as usize].iter().sum();
    Some(((y + 5000) * 365 + before + d - 1) * 24)
}

/// How long to leave a refused tech alone: for the rest of the run when the game will never
/// take it (not in this country's tree, an inactive variant, disabled), else `default`.
fn tech_skip(why: &str, default: GameHour) -> GameHour {
    let never = [
        "no tile",
        "not active",
        "disables Research",
        "already researched",
    ];
    if never.iter().any(|w| why.contains(w)) {
        365 * DAY * 20
    } else {
        default
    }
}

/// `army_layout`: split the armies by role on day one, as the plan says (strike armies attack,
/// guard armies hold): `[{name_hint, state, templates, count, role, general?,
/// guard_states_in_order?, when?}]`. The first strike entry of a state takes the army standing
/// there; every other entry gets up to `count` divisions of its templates detached from it. One
/// action per step; guards are bound to their first post (they hold, never attack).
fn layout_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let Some(entries) = plan["army_layout"].as_array() else {
        return Ok(None);
    };
    if state.skipped(now, "layout", "all") {
        return Ok(None);
    }
    let roster = tools.call("hoi4_armies", &json!({"threat": false}))?;
    let armies = list(&roster, "armies");
    let present: HashSet<String> = armies
        .iter()
        .map(|a| text(&a["army"]).to_string())
        .collect();
    state
        .layout
        .retain(|_, a| a.is_empty() || present.contains(a));
    let taken: HashSet<String> = state.layout.values().cloned().collect();
    for (index, e) in entries.iter().enumerate() {
        let hint = text(&e["name_hint"]).to_string();
        let home = e["state"].as_i64().unwrap_or(0);
        if hint.is_empty() || state.layout.contains_key(&hint) {
            continue;
        }
        if let Some(rest) = text(&e["when"]).strip_prefix("owner:") {
            let (id, tag) = rest.split_once('=').unwrap_or(("0", ""));
            let row = tools
                .call("hoi4_states", &json!({"query": id.trim()}))
                .map(|v| list(&v, "states").first().cloned().unwrap_or_default())
                .unwrap_or_default();
            if text(&row["owner"]) != tag.trim() {
                continue;
            }
        }
        // The army standing in the entry's state that no other role holds yet.
        let base = armies
            .iter()
            .filter(|a| a["main_state"].as_i64() == Some(home))
            .map(|a| text(&a["army"]).to_string())
            .find(|n| !taken.contains(n));
        // No free army there yet but loose divisions stand in the state (a fresh 1936 start
        // has none in armies): make the army first; the next step binds or detaches from it.
        if base.is_none() && unassigned_in(&roster, home) > 0 {
            if state.skipped(now, "layoutcreate", &home.to_string()) {
                continue;
            }
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_army_create",
                json!({"state_id": home}),
                "lc",
            )?;
            if !ok {
                state.skip(now, "layoutcreate", &home.to_string(), 2 * DAY);
            }
            return Ok(Some(format!(
                "layout: army created in {home} for {hint}: {ok} {why}"
            )));
        }
        let first_strike = text(&e["role"]) == "strike"
            && !entries[..index]
                .iter()
                .any(|x| x["state"].as_i64() == Some(home) && text(&x["role"]) == "strike");
        if first_strike {
            let Some(base) = base else { continue };
            state.layout.insert(hint.clone(), base.clone());
            return Ok(Some(format!("layout: {base} is {hint}")));
        }
        // Detach from the state's first strike army (or any army standing there).
        let parent = entries[..index]
            .iter()
            .filter(|x| x["state"].as_i64() == Some(home) && text(&x["role"]) == "strike")
            .filter_map(|x| state.layout.get(text(&x["name_hint"])))
            .find(|a| present.contains(*a))
            .cloned()
            .or(base);
        let Some(parent) = parent else { continue };
        let detail = tools.call("hoi4_supply", &json!({"army": parent}))?;
        let templates: Vec<String> = strings(e, "templates");
        let count = e["count"].as_u64().unwrap_or(1) as usize;
        let pick: Vec<String> = list(&detail, "divisions")
            .iter()
            .filter(|d| text(&d["army"]) == parent)
            .filter(|d| templates.iter().any(|t| t == text(&d["template"])))
            .filter_map(|d| d["id"].as_str().map(str::to_string))
            .take(count)
            .collect();
        if pick.is_empty() {
            // Nothing of those templates here: the role stays empty (never retried).
            state.layout.insert(hint.clone(), String::new());
            continue;
        }
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_army_detach",
            json!({"army": parent, "divisions": pick}),
            "la",
        )?;
        if !ok {
            state.skip(now, "layout", "all", 2 * DAY);
            return Ok(Some(format!(
                "layout: detach for {hint} from {parent}: {ok} {why}"
            )));
        }
        let after = tools.call("hoi4_armies", &json!({"threat": false}))?;
        let new = list(&after, "armies")
            .iter()
            .map(|a| text(&a["army"]).to_string())
            .find(|n| !present.contains(n));
        let Some(new) = new else {
            state.skip(now, "layout", "all", DAY);
            return Ok(Some(format!(
                "layout: detach for {hint} from {parent}: no new army seen"
            )));
        };
        state.layout.insert(hint.clone(), new.clone());
        if text(&e["role"]) == "guard" {
            if let Some(post) = list(e, "guard_states_in_order")
                .first()
                .and_then(Value::as_i64)
            {
                state.war_guard.insert(new.clone(), post as i32);
            }
        }
        return Ok(Some(format!(
            "layout: {} divisions of {parent} -> {new} ({hint}): {ok} {why}",
            pick.len()
        )));
    }
    // Generals named by the layout, only where the army has none (no reassignment churn).
    for e in entries {
        let hint = text(&e["name_hint"]);
        let general = text(&e["general"]);
        let Some(army) = state.layout.get(hint).filter(|a| !a.is_empty()).cloned() else {
            continue;
        };
        if general.is_empty() || state.skipped(now, "layoutgen", &army) {
            continue;
        }
        let row = armies.iter().find(|a| text(&a["army"]) == army);
        if row.is_some_and(|a| a["commander"].is_null()) {
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_army_commander",
                json!({"army": army, "general": general}),
                "lg",
            )?;
            state.skip(
                now,
                "layoutgen",
                &army,
                if ok { 365 * DAY } else { 5 * DAY },
            );
            return Ok(Some(format!(
                "layout general {general} -> {army}: {ok} {why}"
            )));
        }
    }
    Ok(None)
}

/// `construction_plan`: ordered items `{id?, order, building, state, province?, path?, from_state?,
/// to_state?, when, priority?}` queued once each when `when` holds: a date ("1936.01.01"),
/// `province_controlled:<id>`, `state_fully_controlled:<id>` or `owner:<state>=<TAG>`. Supply
/// lines are then raised by the supply-first rule. One item per step, in peace and at war. An
/// item is remembered as queued by its `id` (else its `order`), also across a restart.
fn construction_plan_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let Some(items) = plan["construction_plan"].as_array() else {
        return Ok(None);
    };
    let key_of = |item: &Value| {
        item_id(item).map_or_else(
            || format!("order:{}", item["order"].as_i64().unwrap_or(-1)),
            |id| format!("id:{id}"),
        )
    };
    let pending = |state: &State, item: &Value| {
        let key = key_of(item);
        !state.plan_built.contains_key(&key) && !state.skipped(now, "cplan", &key)
    };
    // Nothing left to queue: no reads at all (in peace this step runs every tick).
    if !items.iter().any(|item| pending(state, item)) {
        return Ok(None);
    }
    let me = text(&tools.call("hoi4_overview", &json!({}))?["player"]).to_string();
    let mut provinces_of: HashMap<i64, Value> = HashMap::new();
    let mut detail = |state_id: i64| -> Value {
        provinces_of
            .entry(state_id)
            .or_insert_with(|| {
                tools
                    .call("hoi4_states", &json!({"query": state_id.to_string()}))
                    .map(|v| list(&v, "states").first().cloned().unwrap_or_default())
                    .unwrap_or_default()
            })
            .clone()
    };
    for item in items {
        if !pending(state, item) {
            continue;
        }
        let order = item["order"].as_i64().unwrap_or(-1);
        let key = key_of(item);
        let label = item_id(item).unwrap_or_else(|| format!("#{order}"));
        let home = item["state"]
            .as_i64()
            .or_else(|| item["to_state"].as_i64())
            .unwrap_or(0);
        let when = text(&item["when"]);
        let due = if let Some(p) = when.strip_prefix("province_controlled:") {
            let p: i64 = p.trim().parse().unwrap_or(0);
            list(&detail(home), "provinces")
                .iter()
                .any(|r| r["province"].as_i64() == Some(p) && text(&r["controller"]) == me)
        } else if let Some(id) = when.strip_prefix("state_fully_controlled:") {
            let row = detail(id.trim().parse().unwrap_or(0));
            let all = list(&row, "provinces");
            !all.is_empty() && all.iter().all(|r| text(&r["controller"]) == me)
        } else if let Some(rest) = when.strip_prefix("owner:") {
            let (id, tag) = rest.split_once('=').unwrap_or(("0", ""));
            text(&detail(id.trim().parse().unwrap_or(0))["owner"]) == tag.trim()
        } else if let Some(at) = date_hour(when) {
            now >= at
        } else {
            when.is_empty()
        };
        if !due {
            continue;
        }
        let building = text(&item["building"]);
        let (tool, args) = if building == "rail_way" || building == "railway" {
            let path: Vec<i64> = list(item, "path")
                .iter()
                .filter_map(Value::as_i64)
                .collect();
            let mut a = json!({
                "from_state": item["from_state"].as_i64().unwrap_or(home),
                "to_state": item["to_state"].as_i64().unwrap_or(home)});
            if let (Some(f), Some(t)) = (path.first(), path.last()) {
                a["from_province"] = json!(f);
                a["to_province"] = json!(t);
            }
            ("hoi4_railway_build", a)
        } else {
            let mut a = json!({"building": building, "state_id": home});
            if let Some(p) = item["province"].as_i64() {
                a["province_id"] = json!(p);
            }
            ("hoi4_construction_add", a)
        };
        let (ok, why) = state.act(tools, lease, tool, args, "cp")?;
        if ok {
            state.plan_built.insert(key, now);
        } else {
            state.skip(now, "cplan", &key, 2 * DAY);
        }
        return Ok(Some(format!(
            "construction plan {label} {building} in {home}: {ok} {why}"
        )));
    }
    Ok(None)
}

/// The first supply line (supply hub or railway) that stands behind a line of another kind, by
/// its 0-based queue index.
fn supply_line_to_raise(lines: &[Value]) -> Option<usize> {
    let supply = |l: &Value| {
        l["line_kind"] == json!("railway") || l["building"]["building_id"] == json!("supply_node")
    };
    let first_other = lines.iter().position(|l| !supply(l))?;
    lines
        .iter()
        .enumerate()
        .skip(first_other + 1)
        .find(|(_, l)| supply(l))
        .map(|(i, _)| i)
}

/// `disband`: {"templates":["Camicie Nere",..],"keep_states":[ids],"max":N}: divisions of these
/// templates in an army, standing outside `keep_states` (default: war.theatre), are disbanded
/// one army at a time so their men and equipment refill the fighting armies (Ethiopia run 10:
/// disbanding home divisions took the African armies from 30% to 100% equipment).
fn disband_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let cfg = &plan["disband"];
    let templates = strings(cfg, "templates");
    if templates.is_empty() {
        return Ok(None);
    }
    let max = cfg["max"].as_u64().unwrap_or(u64::MAX);
    if state.disbanded >= max || state.skipped(now, "disband", "all") {
        return Ok(None);
    }
    let keep: HashSet<i64> = list(cfg, "keep_states")
        .iter()
        .chain(list(&plan["war"], "theatre").iter())
        .filter_map(Value::as_i64)
        .collect();
    // Only divisions standing in own states are candidates: an army fighting in enemy land
    // is never thinned (live 6 Nov 2026: Army 1 in Hararghe was picked).
    let me = text(&tools.call("hoi4_overview", &json!({}))?["player"]).to_string();
    let own: HashSet<i64> = list(&tools.call("hoi4_states", &json!({"query": me}))?, "states")
        .iter()
        .filter(|row| text(&row["owner"]) == me && text(&row["controller"]) == me)
        .filter_map(|row| row["state_id"].as_i64())
        .collect();
    let supply = tools.call("hoi4_supply", &json!({}))?;
    // Armies with a division in enemy-held land are fighting and left whole; a home army with
    // a few divisions already in the theatre is not.
    let mut fighting: HashSet<String> = HashSet::new();
    for d in list(&supply, "divisions") {
        if let (Some(army), Some(at)) = (d["army"].as_str(), d["state_id"].as_i64()) {
            if !own.contains(&at) {
                fighting.insert(army.to_string());
            }
        }
    }
    let mut by_army: Vec<(String, Vec<String>)> = Vec::new();
    for d in list(&supply, "divisions") {
        let (Some(army), Some(id)) = (d["army"].as_str(), d["id"].as_str()) else {
            continue;
        };
        // Each division is judged where it stands: one division of a home army that already
        // reached the theatre no longer shields the rest (w16: Army 5 kept nine divisions in
        // Italy while the African infantry fought at 13-33% equipment).
        let standing = d["state_id"].as_i64();
        let candidate = standing.is_some_and(|at| own.contains(&at) && !keep.contains(&at));
        if !templates.iter().any(|t| t == text(&d["template"]))
            || !candidate
            || fighting.contains(army)
            || state.skipped(now, "disband", army)
        {
            continue;
        }
        match by_army.iter_mut().find(|(a, _)| a == army) {
            Some((_, ids)) => ids.push(id.to_string()),
            None => by_army.push((army.to_string(), vec![id.to_string()])),
        }
    }
    let Some((army, mut ids)) = by_army.into_iter().next() else {
        state.skip(now, "disband", "all", 7 * DAY);
        return Ok(None);
    };
    ids.truncate((max - state.disbanded).min(24) as usize);
    let count = ids.len();
    let (ok, why) = state.act(
        tools,
        lease,
        "hoi4_division_disband",
        json!({"army": army, "divisions": ids}),
        "db",
    )?;
    if ok {
        state.disbanded += count as u64;
    } else {
        state.skip(now, "disband", &army, 3 * DAY);
    }
    Ok(Some(format!("disband {count} from {army}: {ok} {why}")))
}

/// Buildings that share a state's building slots: one state is "full" for all.
const SHARED_SLOT_BUILDINGS: [&str; 7] = [
    "arms_factory",
    "industrial_complex",
    "dockyard",
    "synthetic_refinery",
    "fuel_silo",
    "rocket_site",
    "nuclear_reactor",
];
/// Slot readings older than this are read again before they are trusted.
const SLOT_READING_AGE: GameHour = 3 * DAY;
/// States read with one hoi4_state_slots call per tick.
const PROBE_BATCH: usize = 4;

fn full_key(building: &str, id: i64) -> String {
    if SHARED_SLOT_BUILDINGS.contains(&building) {
        format!("slots:{id}")
    } else {
        format!("{building}:{id}")
    }
}

/// How a refused construction_add is understood.
#[derive(Debug, PartialEq)]
enum Refusal {
    /// no free slot / not allowed / nothing changed: the state is full for a while
    Full,
    /// some of `count` were queued before the rest failed: try one at a time
    Partial,
    /// a pop-up or event blocks input: nothing is wrong with the state
    Blocked,
    /// anything else (missed click, torn read): try this state again soon
    Transient,
}
fn refusal(why: &str) -> Refusal {
    if why.contains("no_free_slot_or_not_allowed") || why.starts_with("NotApplied") {
        Refusal::Full
    } else if why.starts_with("Diverged") {
        Refusal::Partial
    } else if why.contains("ui_blocked") || why.contains("events_open") {
        Refusal::Blocked
    } else {
        Refusal::Transient
    }
}

fn plain(s: &str) -> String {
    s.chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'ä' => 'a',
            'ö' => 'o',
            'ü' => 'u',
            'ß' => 's',
            'é' | 'è' | 'ê' => 'e',
            'á' | 'à' | 'â' => 'a',
            c => c,
        })
        .collect()
}

/// A listed state as an id: a number, a numeric string, or an exact state name
/// resolved (and remembered) with `hoi4_states`.
fn resolve_state(
    state: &mut State,
    tools: &dyn Tools,
    item: &Value,
) -> std::result::Result<i64, String> {
    if let Some(id) = item.as_i64() {
        return Ok(id);
    }
    let Some(name) = item.as_str() else {
        return Err(format!("{item}:bad_entry"));
    };
    if let Ok(id) = name.trim().parse::<i64>() {
        return Ok(id);
    }
    if let Some(id) = state.con.names.get(&plain(name)) {
        return Ok(*id);
    }
    let found = tools
        .call("hoi4_states", &json!({"query": name}))
        .map_err(|e| format!("{name}:name_lookup_{}", e.code))?;
    let rows = list(&found, "states");
    let wanted = plain(name);
    let hit = rows
        .iter()
        .find(|r| plain(text(&r["name"])) == wanted)
        .or_else(|| (rows.len() == 1).then(|| &rows[0]))
        .and_then(|r| r["state_id"].as_i64());
    match hit {
        Some(id) => {
            state.con.names.insert(wanted, id);
            Ok(id)
        }
        None => Err(format!("{name}:name_not_found")),
    }
}

/// Own states of the player (for the fallback), read at most every 5 game days.
fn own_states(state: &mut State, tools: &dyn Tools, tag: &str, now: GameHour) -> Vec<i64> {
    if let Some((at, ids)) = &state.con.own {
        if now - *at < 5 * DAY {
            return ids.clone();
        }
    }
    // Best places first: highest infrastructure, fully held, away from any enemy (w24 built in
    // low-infrastructure states and on the front).
    let front: HashSet<i64> = tools
        .call("hoi4_wars", &json!({}))
        .map(|w| {
            list(&w, "borders")
                .iter()
                .filter(|b| b["hostile"] == json!(true))
                .flat_map(|b| list(b, "own_border_states"))
                .filter_map(|v| v.as_i64())
                .collect()
        })
        .unwrap_or_default();
    let mut rows: Vec<(i64, i64)> = tools
        .call("hoi4_states", &json!({"query": tag}))
        .map(|found| {
            list(&found, "states")
                .iter()
                .filter(|r| {
                    let ok = |k: &str| r[k].as_str().is_none_or(|o| o == tag);
                    ok("owner")
                        && ok("controller")
                        && list(r, "provinces_held_by_others").is_empty()
                })
                .filter_map(|r| {
                    let id = r["state_id"].as_i64()?;
                    Some((id, r["infrastructure_start"].as_i64().unwrap_or(0)))
                })
                .filter(|(id, _)| !front.contains(id))
                .collect()
        })
        .unwrap_or_default();
    rows.sort_by_key(|(id, infra)| (std::cmp::Reverse(*infra), *id));
    let ids: Vec<i64> = rows.into_iter().map(|(id, _)| id).collect();
    if !ids.is_empty() {
        state.con.own = Some((now, ids.clone()));
    }
    ids
}

/// One construction decision: at most one building order. Sets the tick's
/// digest line (`state.con.digest`) so the player can see why nothing was queued:
/// `construction: idle=N min=M queued=[..] skipped=[state:reason,..] total=K`.
fn construction_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
    economy: &Value,
) -> Result<Option<String>> {
    let jobs = list(plan, "construction");
    if jobs.is_empty() {
        return Ok(None);
    }
    // Spare capacity: the pool minus what queued projects already use (older servers
    // report only the pool).
    let idle = economy["civilian_factories_idle"]
        .as_i64()
        .or(economy["civilian_factories_available_for_projects"].as_i64())
        .unwrap_or(0);
    let min_idle = plan["construction_min_idle"].as_i64().unwrap_or(4);
    let mut skipped: Vec<String> = Vec::new();
    let mut queued: Vec<String> = Vec::new();
    let line = |state: &State, why: &str, queued: &[String], skipped: &[String]| {
        format!(
            "construction: idle={idle} min={min_idle} queued=[{}] skipped=[{}] total={} {why}",
            queued.join(","),
            skipped.join(","),
            state.con.queued_total
        )
    };
    // Keep the queue `construction_queue_depth` projects deep (default 3) so the factories
    // never wait for the next tick when a project finishes; older servers do not report the
    // queue and fall back to idle factories only.
    let depth = plan["construction_queue_depth"]
        .as_i64()
        .unwrap_or(3)
        .clamp(0, 20);
    let in_queue = economy["construction_projects_queued"].as_i64();
    let short = in_queue.map_or(0, |n| (depth - n).max(0));
    if idle < min_idle && short == 0 {
        state.con.digest = Some(line(
            state,
            "(factories busy and the queue is full)",
            &queued,
            &skipped,
        ));
        return Ok(None);
    }
    if state.con.yield_next {
        state.con.yield_next = false;
        state.con.digest = Some(line(
            state,
            "(yielded this tick to armies and war)",
            &queued,
            &skipped,
        ));
        return Ok(None);
    }
    if now < state.con.pause_until {
        state.con.digest = Some(line(
            state,
            "(paused: input was blocked by a pop-up or event)",
            &queued,
            &skipped,
        ));
        return Ok(None);
    }
    let full_days = plan["construction_full_days"]
        .as_i64()
        .unwrap_or(14)
        .clamp(1, 365);
    let probe =
        plan["construction_probe_slots"] != json!(false) && now >= state.con.probe_off_until;
    let fallback_all = plan["construction_fallback_any_own_state"] == json!(true);
    let tag = text(&economy["player"]).to_string();

    for (job_index, job) in jobs.iter().enumerate() {
        let building = text(&job["building"]).to_string();
        if building.is_empty() {
            skipped.push(format!("job{job_index}:no_building"));
            continue;
        }
        let want = job["count"].as_i64().unwrap_or(1).clamp(1, 10);
        // listed states first, then (opt-in) every other own state
        let mut ids: Vec<i64> = Vec::new();
        for item in list(job, "states") {
            if state.skipped(now, "conname", &item.to_string()) {
                skipped.push(format!("{}:unresolved", text_or(item)));
                continue;
            }
            match resolve_state(state, tools, item) {
                Ok(id) => {
                    if !ids.contains(&id) {
                        ids.push(id)
                    }
                }
                Err(why) => {
                    state.skip(now, "conname", &item.to_string(), DAY);
                    skipped.push(why);
                }
            }
        }
        let usable = |state: &State, id: i64| {
            !state.skipped(now, "confull", &full_key(&building, id))
                && !state.skipped(now, "con", &format!("{building}:{id}"))
        };
        let mut eligible: Vec<i64> = ids
            .iter()
            .copied()
            .filter(|id| usable(state, *id))
            .collect();
        let listed = !eligible.is_empty();
        if eligible.is_empty()
            && (fallback_all || job["fallback_any_own_state"] == json!(true))
            && !tag.is_empty()
        {
            for id in own_states(state, tools, &tag, now) {
                if !ids.contains(&id) && usable(state, id) {
                    eligible.push(id);
                }
            }
        }
        for id in &ids {
            if !eligible.contains(id) {
                let why = if state.skipped(now, "confull", &full_key(&building, *id)) {
                    "full"
                } else {
                    "retry_later"
                };
                skipped.push(format!("{id}:{why}"));
            }
        }
        if eligible.is_empty() {
            continue;
        }
        // take turns between the listed states; the fallback keeps its best-first order
        if listed {
            let turn = state.con.rr % eligible.len();
            eligible.rotate_left(turn);
        }
        // read free slots where they are unknown or stale
        if probe && SHARED_SLOT_BUILDINGS.contains(&building.as_str()) {
            let unknown: Vec<i64> = eligible
                .iter()
                .copied()
                .filter(|id| {
                    state
                        .con
                        .free
                        .get(id)
                        .is_none_or(|(_, at)| now - *at >= SLOT_READING_AGE)
                })
                .take(PROBE_BATCH)
                .collect();
            if !unknown.is_empty() {
                match tools.call(
                    "hoi4_state_slots",
                    &json!({"state_ids": unknown, "lease_token": lease}),
                ) {
                    Ok(read) => {
                        for row in list(&read, "states") {
                            let Some(id) = row["state_id"].as_i64() else {
                                continue;
                            };
                            match row["free_slots"].as_u64() {
                                Some(free) => {
                                    state.con.free.insert(id, (free as u32, now));
                                    if free == 0 {
                                        state.skip(
                                            now,
                                            "confull",
                                            &full_key(&building, id),
                                            full_days * DAY,
                                        );
                                    }
                                }
                                None => {
                                    // not an own state, or the view never opened
                                    state.skip(now, "con", &format!("{building}:{id}"), 2 * DAY);
                                    skipped.push(format!("{id}:slots_unreadable"));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if e.code == "action_uncertain" {
                            return Err(e);
                        }
                        // reading slots is optional: add blindly, read again later
                        state.con.probe_off_until = now + DAY;
                        skipped.push(format!("probe:{}", e.code));
                    }
                }
                eligible.retain(|id| usable(state, *id));
            }
            // most free slots first; unknown ones keep their turn order
            eligible.sort_by_key(|id| {
                std::cmp::Reverse(state.con.free.get(id).map_or(1, |(f, _)| (*f).min(99)))
            });
        }
        let Some(&id) = eligible.first() else {
            continue;
        };
        let known_free = state.con.free.get(&id).map(|(f, _)| i64::from(*f));
        // Fill idle capacity in one action: each project takes at most 15 civilian
        // factories, so queue about idle/15 + 1 buildings (job count at least, 5 at most).
        // A short queue alone adds one building per line: a line of n buildings still works
        // through them one at a time, so lines (not buildings) keep the factories busy.
        let want = want.max((idle / 15 + 1).min(5));
        let count = want
            .min(state.con.cap.get(&id).copied().unwrap_or(want))
            .min(known_free.unwrap_or(want))
            .max(1);
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_construction_add",
            json!({"building": building, "state_id": id, "count": count}),
            "co",
        )?;
        state.con.rr += 1;
        if ok {
            state.con.fails.remove(&id);
            state.con.queued_total += count as u64;
            state.con.last_queue = Some(now);
            state.con.yield_next = true;
            queued.push(format!("{building}@{id}x{count}"));
            if let Some((free, _)) = state.con.free.get_mut(&id) {
                *free = free.saturating_sub(count as u32);
                if *free == 0 {
                    state.skip(now, "confull", &full_key(&building, id), full_days * DAY);
                }
            }
        } else {
            match refusal(&why) {
                Refusal::Full if count > 1 => {
                    // maybe only some slots were free: take them one at a time
                    state.con.cap.insert(id, 1);
                    skipped.push(format!("{id}:one_at_a_time"));
                }
                Refusal::Full => {
                    state.skip(now, "confull", &full_key(&building, id), full_days * DAY);
                    state.con.free.insert(id, (0, now));
                    skipped.push(format!("{id}:full"));
                }
                Refusal::Partial => {
                    state.con.cap.insert(id, 1);
                    state.con.last_queue = Some(now);
                    state.con.free.remove(&id);
                    skipped.push(format!("{id}:partial"));
                }
                Refusal::Blocked => {
                    state.con.pause_until = now + 6;
                    skipped.push(format!("{id}:blocked"));
                }
                Refusal::Transient => {
                    let fails = state.con.fails.entry(id).or_insert(0);
                    *fails += 1;
                    let hours = if *fails >= 3 { 2 * DAY } else { 6 };
                    if *fails >= 3 {
                        *fails = 0;
                    }
                    state.skip(now, "con", &format!("{building}:{id}"), hours);
                    skipped.push(format!("{id}:retry_later"));
                }
            }
        }
        let note = format!("{ok} {why}");
        state.con.digest = Some(line(state, &note, &queued, &skipped));
        return Ok(Some(format!(
            "construct {building} {id}: {ok} {why} x{count}"
        )));
    }
    let why = if jobs.iter().all(|j| list(j, "states").is_empty()) {
        "(no states listed)"
    } else {
        "(every listed state is full, unresolved or waiting to retry)"
    };
    state.con.digest = Some(line(state, why, &queued, &skipped));
    Ok(None)
}
fn text_or(v: &Value) -> String {
    v.as_str().map_or_else(|| v.to_string(), String::from)
}

/// Whether an entry with an optional "from" date ("YYYY.MM.DD") is due at `now`.
fn reached(entry: &Value, now: GameHour) -> bool {
    match entry.get("from") {
        None | Some(Value::Null) => true,
        Some(from) => match crate::alarms::stop_on_date(&json!({"stop_on_date": from})) {
            Ok(Some((hour, _))) => now >= hour,
            _ => false,
        },
    }
}

/// Routine domains the player tends to forget, one action per tick:
///   special_projects {"priority":["sp_key",..]}  an empty facility gets an idle scientist of its
///                    specialisation and the first priority (else visible) project of it;
///   doctrines [{"id","folder":"land","grand_doctrine"?,"track"?,"subdoctrine"?,"xp":100},..]  the
///                    next entry is adopted once that folder's experience reaches "xp" (old shape
///                    {"doctrine","track"} still read); see the module docs;
///   air {"war_missions":true}  in war, standing-by fighters fly air superiority and CAS wings
///                    close air support over our first border state with an enemy.
fn extras_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    if plan["special_projects"].is_object() && !state.skipped(now, "spcheck", "x") {
        state.skip(now, "spcheck", "x", 7 * DAY);
        let sp = tools.call("hoi4_special_projects", &json!({}))?;
        let priority = strings(&plan["special_projects"], "priority");
        let running: HashSet<String> = list(&sp, "projects")
            .iter()
            .filter(|p| !p["running_in_facility_id"].is_null())
            .map(|p| text(&p["key"]).to_string())
            .collect();
        for facility in list(&sp, "facilities") {
            let Some(id) = facility["facility_id"].as_i64() else {
                continue;
            };
            if !facility["project"].is_null() || state.skipped(now, "spfac", &id.to_string()) {
                continue;
            }
            let spec = text(&facility["specialization"]).to_string();
            let scientist = list(&sp, "scientists")
                .iter()
                .filter(|c| c["assigned_facility_id"].is_null())
                .find(|c| {
                    text(&c["specialization"]) == spec
                        || c["skills"][&spec].as_i64().unwrap_or(0) > 0
                })
                .map(|c| text(&c["name"]).to_string());
            let Some(scientist) = scientist else { continue };
            let pool: Vec<String> = list(&sp, "projects")
                .iter()
                .filter(|p| {
                    text(&p["specialization"]) == spec && p["running_in_facility_id"].is_null()
                })
                .map(|p| text(&p["key"]).to_string())
                .filter(|k| !running.contains(k))
                .collect();
            let project = priority
                .iter()
                .find(|k| pool.contains(k))
                .cloned()
                .or_else(|| pool.first().cloned());
            let Some(project) = project else { continue };
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_special_project_assign",
                json!({"facility_id": id, "scientist": scientist, "project": project}),
                "sj",
            )?;
            if !ok {
                state.skip(now, "spfac", &id.to_string(), 30 * DAY);
            }
            return Ok(Some(format!(
                "special project {project} in facility {id} by {scientist}: {ok} {why}"
            )));
        }
    }
    let doctrines = list(plan, "doctrines");
    if !doctrines.is_empty() && !state.skipped(now, "doccheck", "x") {
        state.skip(now, "doccheck", "x", 7 * DAY);
        let read = tools.call("hoi4_doctrines", &json!({}))?;
        for (index, entry) in doctrines.iter().enumerate() {
            let key =
                item_id(entry).map_or_else(|| format!("doc:{index}"), |id| format!("doc:id:{id}"));
            if state.done.contains_key(&key) {
                continue;
            }
            // What to adopt: the grand doctrine, the subdoctrine on its track, or (old shape,
            // kept for one release) `doctrine` with an optional `track`. An entry naming both
            // a grand doctrine and a subdoctrine adopts the grand doctrine first.
            let grand = entry["grand_doctrine"].as_str().filter(|g| !g.is_empty());
            let sub = entry["subdoctrine"]
                .as_str()
                .or_else(|| entry["doctrine"].as_str())
                .filter(|d| !d.is_empty());
            let grand_key = format!("{key}:grand");
            let (part_key, doctrine, track) = match (grand, sub) {
                (Some(g), Some(_)) if !state.done.contains_key(&grand_key) => (grand_key, g, None),
                (_, Some(d)) => (key.clone(), d, entry["track"].as_str()),
                (Some(g), None) => (key.clone(), g, None),
                // nothing to adopt: the entry is not this step's to fix
                (None, None) => continue,
            };
            if state.skipped(now, "doc", &part_key) {
                break;
            }
            let folder = entry["folder"].as_str().unwrap_or("land");
            let kind = match folder {
                "air" => "air",
                "naval" | "navy" => "navy",
                _ => "army",
            };
            let xp = read["experience"][kind]["value"].as_f64().unwrap_or(0.0);
            if xp < entry["xp"].as_f64().unwrap_or(100.0) {
                break;
            }
            let mut args = json!({"doctrine": doctrine});
            if let Some(track) = track.filter(|t| !t.is_empty()) {
                args["track"] = json!(track);
            }
            let (ok, why) = state.act(tools, lease, "hoi4_doctrine_adopt", args, "dc")?;
            if ok || why.contains("already") {
                state.done.insert(part_key, now);
            } else {
                state.skip(now, "doc", &part_key, 15 * DAY);
            }
            return Ok(Some(format!("doctrine {doctrine}: {ok} {why}")));
        }
    }
    if plan["air"]["war_missions"] == json!(true) && !state.skipped(now, "aircheck", "x") {
        state.skip(now, "aircheck", "x", 3 * DAY);
        let wars = tools.call("hoi4_wars", &json!({}))?;
        let front = list(&wars, "borders")
            .iter()
            .find(|b| b["hostile"] == json!(true))
            .and_then(|b| {
                b["own_border_states"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(Value::as_i64)
            });
        if let Some(state_id) = front {
            let air = tools.call("hoi4_air", &json!({}))?;
            for wing in list(&air, "wings") {
                let name = text(&wing["wing"]).to_string();
                if wing["standing_by"] != json!(true)
                    || state.skipped(now, "airwing", &name)
                    || plan_manage::air_plan_claims(plan, wing)
                {
                    continue;
                }
                let Some(mission) = war_mission(text(&wing["aircraft_type"])) else {
                    continue;
                };
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_air_mission",
                    json!({"wing": name, "mission": mission, "state_id": state_id}),
                    "am",
                )?;
                state.skip(now, "airwing", &name, if ok { 30 * DAY } else { 7 * DAY });
                return Ok(Some(format!(
                    "air {name} {mission} over state {state_id}: {ok} {why}"
                )));
            }
        }
    }
    Ok(None)
}

/// Game hours between two stages of one army's war order.
const WAR_STAGE_PAUSE: GameHour = 2;

/// One war decision: at most one action, or None when nothing needs doing.
fn war_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let cfg = &plan["war"];
    let me = match cfg["me"].as_str() {
        Some(t) => t.to_string(),
        None => text(&tools.call("hoi4_overview", &json!({}))?["player"]).to_string(),
    };
    if me.is_empty() {
        return Ok(None);
    }
    let min_army = cfg["min_army"].as_u64().unwrap_or(6);
    let per_target = cfg["per_target"].as_u64().unwrap_or(2) as usize;
    let cooldown: GameHour = cfg["cooldown_hours"].as_i64().unwrap_or(12);
    // Armies the player ordered directly in the last `player_claim_seconds` (default 300) are
    // left to the player, like `reserve` (the executor never fights the player for an army).
    let mut reserve = strings(cfg, "reserve");
    let claim = cfg["player_claim_seconds"].as_u64().unwrap_or(300);
    reserve.extend(crate::autopilot::autopilot().claimed(Duration::from_secs(claim)));
    // Targets: own states an enemy holds (priority), then enemy-owned states an enemy holds.
    let mine = tools.call("hoi4_states", &json!({"query": me}))?;
    let mut enemies: HashSet<String> = strings(cfg, "enemies").into_iter().collect();
    // The plan's enemies count only while at war with them (live 8 Nov 2026: after the peace
    // the executor still sent "stalled" moves into Ethiopia). Best effort: without a validated
    // war list the plan's names stand.
    if let Ok(wars) = tools.call("hoi4_wars", &json!({})) {
        let w = &wars["wars"];
        if w["validated"] == json!(true) {
            let at_war: HashSet<String> = list(w, "enemies")
                .iter()
                .filter_map(|e| {
                    e.as_str()
                        .map(str::to_string)
                        .or_else(|| e["tag"].as_str().map(str::to_string))
                })
                .collect();
            enemies.retain(|t| at_war.contains(t));
        }
    }
    let mut targets: Vec<(i32, bool)> = Vec::new();
    for row in list(&mine, "states") {
        if let (Some(id), Some(x), Some(z)) = (
            row["state_id"].as_i64(),
            row["x"].as_f64(),
            row["z"].as_f64(),
        ) {
            state.centers.insert(id as i32, (x, z));
            if text(&row["owner"]) == me && text(&row["controller"]) != me {
                enemies.insert(text(&row["controller"]).to_string());
                targets.push((id as i32, true));
            } else if text(&row["owner"]) == me {
                // Part of an own state an enemy holds (w22: Ethiopia took Eritrea province by
                // province around an army standing in it): retake it like an occupied state.
                let held: Vec<String> = list(row, "provinces_held_by_others")
                    .iter()
                    .map(|p| text(&p["controller"]).to_string())
                    .filter(|c| !c.is_empty() && *c != me)
                    .collect();
                if let Some(c) = held.first() {
                    enemies.insert(c.clone());
                    targets.push((id as i32, true));
                }
            }
        }
    }
    for tag in &enemies {
        let theirs = tools.call("hoi4_states", &json!({"query": tag}))?;
        for row in list(&theirs, "states") {
            let (Some(id), Some(x), Some(z)) = (
                row["state_id"].as_i64(),
                row["x"].as_f64(),
                row["z"].as_f64(),
            ) else {
                continue;
            };
            state.centers.insert(id as i32, (x, z));
            let owner = text(&row["owner"]);
            let controller = text(&row["controller"]);
            if owner == tag && controller == tag && !targets.iter().any(|t| t.0 == id as i32) {
                targets.push((id as i32, false));
            }
        }
    }
    if targets.is_empty() {
        return Ok(None);
    }
    // Fully held own states with their positions: where an army in a pocket falls back to.
    let safe: Vec<(i32, (f64, f64))> = list(&mine, "states")
        .iter()
        .filter(|r| {
            text(&r["owner"]) == me
                && text(&r["controller"]) == me
                && list(r, "provinces_held_by_others").is_empty()
        })
        .filter_map(|r| {
            Some((
                r["state_id"].as_i64()? as i32,
                (r["x"].as_f64()?, r["z"].as_f64()?),
            ))
        })
        .collect();
    // At war, supply comes first in the construction queue: a hub or railway behind another
    // line gets no factories (w23: the Tigray and Wello hubs sat behind industrial complexes
    // with none and the army starved in Wello).
    if cfg["supply_first"] != json!(false) && !state.skipped(now, "supplyfirst", "queue") {
        let queue = tools.call("hoi4_construction_queue", &json!({"limit": 32}))?;
        if let Some(line) = supply_line_to_raise(list(&queue, "lines")) {
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_construction_order",
                json!({"line": line, "action": "top"}),
                "sq",
            )?;
            if !ok {
                state.skip(now, "supplyfirst", "queue", DAY);
            }
            return Ok(Some(format!(
                "supply first: construction line {line} to the top: {ok} {why}"
            )));
        }
    }
    let roster = tools.call("hoi4_armies", &json!({}))?;
    let mut vp_rows: HashMap<i64, Value> = HashMap::new();
    let mut load: HashMap<i32, usize> = HashMap::new();
    for target in state.war_assign.values() {
        *load.entry(*target).or_default() += 1;
    }
    // Axes: ordered state lists ([[842,843,271],[835,271]]); an army follows the axis whose
    // next enemy-held state is nearest to it. Without axes: the nearest enemy-held state.
    let axes: Vec<Vec<i32>> = list(cfg, "axes")
        .iter()
        .map(|axis| {
            axis.as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_i64().map(|v| v as i32))
                .collect()
        })
        .filter(|axis: &Vec<i32>| !axis.is_empty())
        .collect();
    // Theatre: own states armies must stand in to be driven (armies in enemy land always are);
    // keeps home armies that can never reach the war out of it.
    let theatre: HashSet<i32> = list(cfg, "theatre")
        .iter()
        .filter_map(|v| v.as_i64().map(|v| v as i32))
        .collect();
    let mine_ids: HashSet<i32> = list(&mine, "states")
        .iter()
        .filter(|row| text(&row["owner"]) == me)
        .filter_map(|row| row["state_id"].as_i64().map(|v| v as i32))
        .collect();
    let stall: GameHour = cfg["stall_days"].as_i64().unwrap_or(8) * DAY;
    let rest_below = cfg["rest_below"].as_f64().unwrap_or(12.0);
    let resume_above = cfg["resume_above"].as_f64().unwrap_or(30.0);
    // Organisation per army (absolute: a rested infantry division reads about 45).
    let supply_floor = cfg["supply_floor"].as_f64().unwrap_or(25.0);
    let rest_max: GameHour = cfg["rest_max_days"].as_i64().unwrap_or(20) * DAY;
    let condition: HashMap<String, (f64, f64)> = if cfg["rest"] == json!(false) {
        HashMap::new()
    } else {
        tools
            .call("hoi4_supply", &json!({}))
            .ok()
            .map(|supply| {
                list(&supply, "armies")
                    .iter()
                    .filter_map(|a| {
                        Some((
                            text(&a["army"]).to_string(),
                            (
                                a["organisation_mean"].as_f64()?,
                                a["supply_percent_mean"].as_f64().unwrap_or(100.0),
                            ),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    // Guards: {"Army 4": 844} holds that own state: the army goes there and draws a front on
    // the nearest border, and never attacks (w15: Ethiopia walked into Eritrea and Somalia
    // after every army had left for the offensive).
    let mut guards: HashMap<String, i32> = cfg["guards"]
        .as_object()
        .map(|g| {
            g.iter()
                .filter_map(|(k, v)| Some((k.clone(), v.as_i64()? as i32)))
                .collect()
        })
        .unwrap_or_default();
    // "guard_states": [ids]: the first army seen standing in such a state guards it (army names
    // depend on the order the armies were made in).
    for post in list(cfg, "guard_states")
        .iter()
        .filter_map(|v| v.as_i64().map(|v| v as i32))
    {
        if state.war_guard.values().any(|p| *p == post) {
            continue;
        }
        // An army the plan's layout gave a role (a strike army) is never taken as a guard here.
        let laid_out: HashSet<&String> = state.layout.values().collect();
        let standing = list(&roster, "armies").iter().find(|a| {
            let n = text(&a["army"]);
            a["main_state"].as_i64() == Some(post as i64)
                && !guards.contains_key(n)
                && !state.war_guard.contains_key(n)
                && !laid_out.contains(&n.to_string())
        });
        if let Some(a) = standing {
            let n = text(&a["army"]).to_string();
            if let Some(old) = state.war_assign.remove(&n) {
                if let Some(c) = load.get_mut(&old) {
                    *c = c.saturating_sub(1);
                }
            }
            state.war_guard.insert(n, post);
        }
    }
    let present: HashSet<String> = list(&roster, "armies")
        .iter()
        .map(|a| text(&a["army"]).to_string())
        .collect();
    state.war_guard.retain(|n, _| present.contains(n));
    for (n, p) in &state.war_guard {
        guards.entry(n.clone()).or_insert(*p);
    }
    for army in list(&roster, "armies") {
        let name = text(&army["army"]).to_string();
        let Some(&post) = guards.get(&name) else {
            continue;
        };
        if state.war_cool.get(&name).is_some_and(|until| *until > now) {
            continue;
        }
        let here = army["main_state"].as_i64().map(|s| s as i32);
        let key = format!("guard:{name}");
        let stage = state.war_stage.get(&key).copied().unwrap_or(0);
        let (tool, args, label, next) = if here != Some(post) && stage < 2 {
            (
                "hoi4_army_move",
                json!({"army": name, "state_id": post}),
                "guard move",
                1,
            )
        } else if stage < 2 {
            // The front tool takes an enemy-held state: the one nearest the post (the game
            // refuses an own, own-controlled state as a front target, live w16).
            let facing = state.centers.get(&post).copied().and_then(|(px, pz)| {
                targets
                    .iter()
                    .filter_map(|(id, _)| {
                        let (x, z) = state.centers.get(id).copied()?;
                        Some((*id, (x - px).powi(2) + (z - pz).powi(2)))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(id, _)| id)
            });
            (
                "hoi4_army_front",
                json!({"army": name, "state_id": facing.unwrap_or(post)}),
                "guard front",
                2,
            )
        } else {
            continue;
        };
        let (ok, why) = state.act(tools, lease, tool, args, "wg")?;
        if ok && next == 2 {
            state.war_stage.insert(key, 2);
        }
        state
            .war_cool
            .insert(name.clone(), now + if ok { cooldown * 2 } else { cooldown });
        return Ok(Some(format!(
            "war {label} {name} -> state {post}: {ok} {why}"
        )));
    }
    // An own state an enemy holds that no army is retaking draws the nearest fighting army off
    // its axis at once: the axes alone left Eritrea and Somalia occupied for months (w15).
    for (lib, _) in targets.iter().filter(|t| t.1) {
        if load.get(lib).copied().unwrap_or(0) > 0 {
            continue;
        }
        let Some(&(lx, lz)) = state.centers.get(lib) else {
            continue;
        };
        let nearest = list(&roster, "armies")
            .iter()
            .filter(|a| {
                let n = text(&a["army"]);
                a["division_count"].as_u64().unwrap_or(0) >= min_army
                    && !reserve.iter().any(|r| r == n)
                    && !guards.contains_key(n)
                    && !state
                        .war_assign
                        .get(n)
                        .is_some_and(|t| targets.iter().any(|x| x.0 == *t && x.1))
            })
            .filter_map(|a| {
                let s = a["main_state"].as_i64()? as i32;
                let (x, z) = state.centers.get(&s).copied()?;
                Some((
                    text(&a["army"]).to_string(),
                    (x - lx).powi(2) + (z - lz).powi(2),
                ))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((name, _)) = nearest {
            if let Some(old) = state.war_assign.insert(name.clone(), *lib) {
                if let Some(n) = load.get_mut(&old) {
                    *n = n.saturating_sub(1);
                }
            }
            *load.entry(*lib).or_default() += 1;
            state.war_stage.remove(&name);
            state.war_move.remove(&name);
            state.war_since.remove(&name);
            state.war_rest.remove(&name);
            state.war_cool.remove(&name);
        }
    }
    for army in list(&roster, "armies") {
        let name = text(&army["army"]).to_string();
        if guards.contains_key(&name) {
            continue;
        }
        let here = army["main_state"].as_i64().map(|s| s as i32);
        let in_theatre = theatre.is_empty()
            || here.is_some_and(|s| theatre.contains(&s) || !mine_ids.contains(&s));
        if army["division_count"].as_u64().unwrap_or(0) < min_army
            || reserve.contains(&name)
            || !in_theatre
            || state.war_cool.get(&name).is_some_and(|until| *until > now)
        {
            continue;
        }
        // Trucks for the army's supply hubs (far from railways supply needs them).
        if let Some(level) = cfg["motorization"].as_u64() {
            if army["motorization"].as_u64().unwrap_or(0) < level
                && !state.skipped(now, "motor", &name)
            {
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_army_motorization",
                    json!({"army": name, "level": level}),
                    "wz",
                )?;
                state.skip(now, "motor", &name, if ok { 30 * DAY } else { 3 * DAY });
                return Ok(Some(format!(
                    "war motorization {name} level {level}: {ok} {why}"
                )));
            }
        }
        let position = here.and_then(|s| state.centers.get(&s).copied());
        let open = |id: i32| targets.iter().any(|x| x.0 == id);
        // Keep the current target while it is still held by an enemy, unless a state before it
        // on its axis was lost again: that one comes first (w16: Wello fell back to Ethiopia
        // while both armies kept attacking Shewa behind it).
        let behind = |t: i32| {
            axes.iter().any(|axis| {
                axis.iter()
                    .position(|id| *id == t)
                    .is_some_and(|at| axis[..at].iter().any(|id| open(*id)))
            })
        };
        let mut target = state
            .war_assign
            .get(&name)
            .copied()
            .filter(|t| open(*t) && !behind(*t));
        if target.is_none() {
            if let Some(old) = state.war_assign.remove(&name) {
                if let Some(n) = load.get_mut(&old) {
                    *n = n.saturating_sub(1);
                }
            }
            state.war_stage.remove(&name);
            state.war_move.remove(&name);
            state.war_since.remove(&name);
            let distance = |id: i32| -> Option<f64> {
                let (x, z) = state.centers.get(&id).copied()?;
                Some(position.map_or(0.0, |(px, pz)| ((x - px).powi(2) + (z - pz).powi(2)).sqrt()))
            };
            let from_axes: Vec<(i32, f64)> = axes
                .iter()
                .filter_map(|axis| axis.iter().copied().find(|id| open(*id)))
                .filter_map(|id| Some((id, distance(id)?)))
                .collect();
            let best = if !from_axes.is_empty() {
                from_axes.into_iter().min_by(|a, b| a.1.total_cmp(&b.1))
            } else {
                targets
                    .iter()
                    .filter(|(id, _)| load.get(id).copied().unwrap_or(0) < per_target)
                    .filter_map(|(id, liberate)| {
                        let d = distance(*id)?;
                        Some((*id, if *liberate { d * 0.5 } else { d }))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
            };
            if let Some((id, _)) = best {
                state.war_assign.insert(name.clone(), id);
                *load.entry(id).or_default() += 1;
                target = Some(id);
            }
        }
        let Some(target) = target else { continue };
        // `direct_moves`: attack with direct moves, state then province by province, instead of
        // battle plans (w11 and w23: the plans stalled for weeks in the Ethiopian mountains and a
        // direct move broke each stall).
        let direct = cfg["direct_moves"] == json!(true);
        let liberate = direct || targets.iter().any(|t| t.0 == target && t.1);
        let stage = state.war_stage.get(&name).copied().unwrap_or(0);
        let executing = army["plan_executing"] == json!(true);
        let standing_in_target = here == Some(target);
        let org = condition.get(&name).copied();
        // Progress: the army's main state changes, or the target changes. Reset on either.
        let since = match state.war_since.get(&name) {
            Some((t, h, at)) if *t == target && *h == here => *at,
            _ => {
                state.war_since.insert(name.clone(), (target, here, now));
                now
            }
        };
        // A pocket forming: most land around the army is held by others. Fall back to the
        // nearest fully held own state once, then re-check (w24: armies were encircled).
        if army["threat"]["encirclement_risk"] == json!(true)
            && cfg["avoid_pockets"] != json!(false)
            && !state.skipped(now, "pocket", &name)
        {
            let (ax, az) = position.unwrap_or((0.0, 0.0));
            if let Some((safe_id, _)) = safe
                .iter()
                .filter(|(id, _)| Some(*id) != here)
                .min_by(|a, b| {
                    let d = |p: &(f64, f64)| (p.0 - ax).powi(2) + (p.1 - az).powi(2);
                    d(&a.1).total_cmp(&d(&b.1))
                })
                .copied()
            {
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_army_move",
                    json!({"army": name, "state_id": safe_id}),
                    "wp",
                )?;
                state.skip(now, "pocket", &name, 3 * DAY);
                state.war_move.remove(&name);
                state.war_province.remove(&name);
                state.war_cool.insert(name.clone(), now + cooldown * 2);
                return Ok(Some(format!(
                    "war pocket {name}: {:.0}% of the land around it is held by others -> back to state {safe_id}: {ok} {why}",
                    army["threat"]["share"].as_f64().unwrap_or(0.0) * 100.0
                )));
            }
        }
        // Direct moves have no plan to stop: an exhausted or starving army gets no new attack
        // order until it recovers (w24 kept pushing at 22-28% supply and organisation 17-27).
        // (Retaking an own state is how a starving army gets its supply back: never held.)
        let own_target = targets.iter().any(|t| t.0 == target && t.1);
        if direct && !own_target {
            if let Some((org, supply)) = org {
                if org < rest_below || supply < supply_floor {
                    let recovered = org >= resume_above && supply >= supply_floor;
                    if !recovered {
                        state.war_cool.insert(name.clone(), now + cooldown);
                        continue;
                    }
                }
            }
        }
        // `vp_targets`: the plan's victory-point provinces in order, each for a named army or any
        // army. The army goes for the first one not yet held (own land to retake comes first).
        if !own_target {
            if let Some(vp) = plan["vp_targets"].as_array() {
                let names: Vec<String> = list(&roster, "armies")
                    .iter()
                    .map(|a| text(&a["army"]).to_string())
                    .collect();
                let by_layout = |t: &Value| state.layout.get(text(&t["army"])).cloned();
                let mine_now = |t: &Value| {
                    by_layout(t).is_some_and(|a| a == name)
                        || (by_layout(t).is_none() && text(&t["army"]).contains(name.as_str()))
                };
                let unowned = |t: &Value| {
                    by_layout(t).is_none_or(|a| a.is_empty())
                        && !names.iter().any(|n| text(&t["army"]).contains(n.as_str()))
                };
                let mut chosen: Option<(i64, i64)> = None;
                for t in vp {
                    if !(mine_now(t) || unowned(t)) {
                        continue;
                    }
                    let (Some(p), Some(st)) = (t["province"].as_i64(), t["state"].as_i64()) else {
                        continue;
                    };
                    let row = vp_rows
                        .entry(st)
                        .or_insert_with(|| {
                            tools
                                .call("hoi4_states", &json!({"query": st.to_string()}))
                                .map(|v| list(&v, "states").first().cloned().unwrap_or_default())
                                .unwrap_or_default()
                        })
                        .clone();
                    let held = list(&row, "provinces")
                        .iter()
                        .any(|r| r["province"].as_i64() == Some(p) && text(&r["controller"]) == me);
                    if !held {
                        chosen = Some((st, p));
                        break;
                    }
                }
                if let Some((st, p)) = chosen {
                    let on_way = state
                        .war_province
                        .get(&name)
                        .is_some_and(|(q, at)| i64::from(*q) == p && now - at < 3 * DAY);
                    if on_way {
                        state.war_cool.insert(name.clone(), now + cooldown);
                        continue;
                    }
                    state.war_province.insert(name.clone(), (p as i32, now));
                    let (ok, why) = state.act(
                        tools,
                        lease,
                        "hoi4_army_move",
                        json!({"army": name, "state_id": st, "province_id": p}),
                        "wv",
                    )?;
                    state.war_cool.insert(name.clone(), now + cooldown);
                    return Ok(Some(format!(
                        "war vp target {name} -> province {p} (state {st}): {ok} {why}"
                    )));
                }
            }
        }
        // Exhausted (organisation) or starving (supply): stop the plan, attack again when it
        // recovers or after `rest_max_days` (supply where the army stands may never recover).
        if let Some((org, supply)) = org {
            let starving = supply < supply_floor;
            if (org < rest_below || starving) && executing && !state.war_rest.contains_key(&name) {
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_army_plan",
                    json!({"army": name, "execute": false}),
                    "wr",
                )?;
                if ok {
                    state.war_rest.insert(name.clone(), now);
                }
                state.war_cool.insert(name.clone(), now + cooldown);
                return Ok(Some(format!(
                    "war rest {name} (organisation {org:.0}, supply {supply:.0}%): {ok} {why}"
                )));
            }
            if let Some(&since_rest) = state.war_rest.get(&name) {
                let recovered = org >= resume_above && !starving;
                if !recovered && now - since_rest < rest_max {
                    continue;
                }
                state.war_rest.remove(&name);
                state.war_since.insert(name.clone(), (target, here, now));
                let (tool, args, label) = if state.war_move.contains(&name) {
                    (
                        "hoi4_army_move",
                        json!({"army": name, "state_id": target}),
                        "resume move",
                    )
                } else {
                    (
                        "hoi4_army_plan",
                        json!({"army": name, "execute": true}),
                        "resume",
                    )
                };
                let (ok, why) = state.act(tools, lease, tool, args, "wr")?;
                state.war_cool.insert(name.clone(), now + cooldown);
                return Ok(Some(format!(
                    "war {label} {name} -> state {target} (organisation {org:.0}, supply {supply:.0}%): {ok} {why}"
                )));
            }
        }
        // Stalled: no progress for `stall_days` while ordered. Direct moves into the target
        // advance where battle plans do not (seen in Ethiopia: a plan that never left the
        // start state; moves took Bale and Sidamo).
        let ordered = executing || stage >= 3 || state.war_move.contains(&name);
        // An army inside its target that has not taken it for twice the stall time also gets a
        // direct move: it gathers the divisions on the state's centre instead of a thin plan line
        // (Wello stayed Ethiopian for two months with 12 divisions inside it, live 6 Nov 2026).
        // Backoff: each direct move that did not get the army out of its state doubles the wait
        // before the next one (w17 re-issued one stalled move 18 times, each a full army order,
        // while the army stayed put and scattered).
        let repeats = match state.war_repeat.get(&name) {
            Some((t, h, n)) if *t == target && *h == here => *n,
            _ => 0,
        };
        let stall_now = stall * (1 << repeats.min(3));
        let stuck_inside = standing_in_target && now - since >= 2 * stall_now;
        if ordered && (!standing_in_target || stuck_inside) && now - since >= stall_now {
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_army_move",
                json!({"army": name, "state_id": target}),
                "wm",
            )?;
            state.war_move.insert(name.clone());
            state.war_since.insert(name.clone(), (target, here, now));
            state
                .war_repeat
                .insert(name.clone(), (target, here, repeats + 1));
            state.war_cool.insert(name.clone(), now + cooldown);
            return Ok(Some(format!(
                "war stalled {name} {} days -> move to state {target}: {ok} {why}",
                (now - since) / DAY
            )));
        }
        if state.war_move.contains(&name) && !(liberate && standing_in_target) {
            // Moving: wait for progress (the stall check above re-issues the move). Inside a
            // state to retake or take, go on province by province below.
            continue;
        }
        let (tool, args, label, next_stage) = if liberate {
            if standing_in_target {
                // Inside the state: go for the nearest province the enemy holds in it (w22: the
                // army waited inside Eritrea while it was taken around it and starved).
                let detail = tools.call("hoi4_states", &json!({"query": target.to_string()}))?;
                let (ax, az) = position.unwrap_or((0.0, 0.0));
                let nearest = list(&detail, "states")
                    .first()
                    .map(|row| list(row, "provinces"))
                    .unwrap_or_default()
                    .iter()
                    .filter(|p| {
                        let c = text(&p["controller"]);
                        !c.is_empty() && c != me
                    })
                    .filter_map(|p| {
                        let id = p["province"].as_i64()? as i32;
                        let (x, z) = (p["x"].as_f64()?, p["z"].as_f64()?);
                        // Victory points first, largest first, then nearest (w24 held four
                        // provinces of Shewa but never Addis Abeba's own, so Ethiopia never
                        // came near capitulation).
                        let vp = p["victory_points"].as_f64().unwrap_or(0.0);
                        Some((id, (x - ax).powi(2) + (z - az).powi(2) - vp * 1.0e9))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1));
                let Some((province, _)) = nearest else {
                    state.war_cool.insert(name.clone(), now + cooldown * 3);
                    continue;
                };
                // Still on the way to the same province: let it arrive (no repeated orders).
                if state
                    .war_province
                    .get(&name)
                    .is_some_and(|(p, at)| *p == province && now - at < 3 * DAY)
                {
                    state.war_cool.insert(name.clone(), now + cooldown);
                    continue;
                }
                state.war_province.insert(name.clone(), (province, now));
                (
                    "hoi4_army_move",
                    json!({"army": name, "state_id": target, "province_id": province}),
                    "move",
                    0,
                )
            } else {
                (
                    "hoi4_army_move",
                    json!({"army": name, "state_id": target}),
                    "move",
                    0,
                )
            }
        } else if stage == 0 && !executing {
            (
                "hoi4_army_front",
                json!({"army": name, "state_id": target}),
                "front",
                1,
            )
        } else if stage == 1 {
            (
                "hoi4_army_offensive",
                json!({"army": name, "state_id": target}),
                "offensive",
                2,
            )
        } else if stage == 2 {
            (
                "hoi4_army_plan",
                json!({"army": name, "execute": true}),
                "execute",
                3,
            )
        } else {
            state.war_cool.insert(name.clone(), now + cooldown);
            continue;
        };
        let one_province = args.get("province_id").is_some();
        let (ok, why) = state.act(tools, lease, tool, args, "wr")?;
        if ok {
            state.war_stage.insert(name.clone(), next_stage);
            state.war_cool.insert(name.clone(), now + WAR_STAGE_PAUSE);
            if next_stage == 3 {
                state.war_since.insert(name.clone(), (target, here, now));
            }
            // A liberation move is a direct move: wait for progress, and let the stall check
            // (with its backoff) re-issue it (live 8 Nov 2026: the same move to an occupied own
            // state went out 13 times in three minutes).
            if label == "move" && !one_province {
                state.war_move.insert(name.clone());
                state.war_since.insert(name.clone(), (target, here, now));
            }
            return Ok(Some(format!("war {label} {name} -> state {target}: ok")));
        }
        // No foothold for a front line yet: step into the state first.
        if label == "front" {
            let (ok, why2) = state.act(
                tools,
                lease,
                "hoi4_army_move",
                json!({"army": name, "state_id": target}),
                "wm",
            )?;
            if ok {
                state.war_move.insert(name.clone());
                state.war_since.insert(name.clone(), (target, here, now));
            }
            state.war_cool.insert(name.clone(), now + cooldown);
            return Ok(Some(format!(
                "war move {name} -> state {target}: {ok} {why2} (front: {why})"
            )));
        }
        state.war_cool.insert(name.clone(), now + cooldown);
        return Ok(Some(format!(
            "war {label} {name} -> state {target}: false {why}"
        )));
    }
    Ok(None)
}

/// Game hours between two judgements of the strategy keys (a change of the keys is judged at once).
const STRATEGY_EVERY: GameHour = 6;

/// The strategy layer: what is kept in plan_state.json (phase, settled checkpoints, active
/// contingencies, goal status) and what the current run reports.
#[derive(Default)]
struct Strategy {
    /// the active phase and the game hour it began
    phase: Option<(String, GameHour)>,
    /// settled checkpoints: id -> (game hour, met)
    checkpoints: HashMap<String, (GameHour, bool)>,
    /// active contingencies: id -> (game hour, triggered by a missed checkpoint)
    contingencies: HashMap<String, (GameHour, bool)>,
    /// the goal status last judged ("open", "reached", "at_risk")
    goal: Option<String>,
    /// last judgement: (game hour, the strategy keys as text)
    evaluated: Option<(GameHour, String)>,
    /// the digest's `strategy` block
    summary: Value,
    /// this run: predicate atoms that read unknown -> why
    unknown: BTreeMap<String, String>,
    /// this run: phase changes, missed checkpoints, triggered contingencies
    news: Vec<Value>,
}

impl Strategy {
    fn memory(&self) -> Value {
        json!({"phase": self.phase.as_ref().map(|(p, _)| p),
            "phase_since": self.phase.as_ref().map(|(_, h)| h),
            "checkpoints": self.checkpoints, "contingencies": self.contingencies,
            "goal": self.goal})
    }
    /// Take back a saved strategy memory; anything dated after `now` is dropped.
    fn restore(&mut self, saved: &Value, now: GameHour, rewound: bool) {
        if let (Some(phase), Some(since)) = (saved["phase"].as_str(), saved["phase_since"].as_i64())
        {
            if since <= now && self.phase.is_none() {
                self.phase = Some((phase.to_string(), since));
            }
        }
        let pairs = |key: &str| -> Vec<(String, GameHour, bool)> {
            saved[key]
                .as_object()
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| Some((k.clone(), v[0].as_i64()?, v[1].as_bool()?)))
                        .collect()
                })
                .unwrap_or_default()
        };
        for (id, at, flag) in pairs("checkpoints") {
            if at <= now {
                self.checkpoints.entry(id).or_insert((at, flag));
            }
        }
        for (id, at, flag) in pairs("contingencies") {
            if at <= now {
                self.contingencies.entry(id).or_insert((at, flag));
            }
        }
        if !rewound {
            if let Some(goal) = saved["goal"].as_str() {
                self.goal.get_or_insert_with(|| goal.to_string());
            }
        }
    }
    fn phase_id(&self) -> Option<String> {
        self.phase.as_ref().map(|(p, _)| p.clone())
    }
}

/// A plan predicate judged in this tick's context; unknown atoms go to the run digest.
fn judge(state: &mut State, tools: &dyn Tools, predicate: &Value, now: GameHour) -> Truth {
    let cache = match state.pcache.take() {
        Some((hour, cache)) if hour == now => cache,
        _ => Cache::default(),
    };
    let mut ctx = Ctx::with_cache(tools, now, cache);
    ctx.phase = state.strategy.phase_id();
    let truth = crate::plan_predicate::check(predicate, &mut ctx);
    for (atom, why) in ctx.take_unknown() {
        state.strategy.unknown.entry(atom).or_insert(why);
    }
    state.pcache = Some((now, ctx.into_cache()));
    truth
}

/// 0. The strategy layer (reads only, never input): `goal`, `phases`, `checkpoints` and
///    `contingencies` are judged at most every `STRATEGY_EVERY` game hours (at once when they
///    change); the plan for the rest of the tick is the base plan with the active phase's and
///    the active contingencies' overrides merged over it (None: no overrides).
fn strategy_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    now: GameHour,
) -> Option<Value> {
    let keys = [
        &plan["goal"],
        &plan["phases"],
        &plan["checkpoints"],
        &plan["contingencies"],
    ];
    if keys.iter().all(|v| v.is_null()) {
        state.strategy.summary = Value::Null;
        return None;
    }
    let signature = Value::Array(keys.iter().map(|v| (*v).clone()).collect()).to_string();
    let due = match &state.strategy.evaluated {
        None => true,
        Some((at, seen)) => *seen != signature || now < *at || now >= at + STRATEGY_EVERY,
    };
    if due {
        state.strategy.evaluated = Some((now, signature));
        // the atoms this judgement could not read stay in the `strategy` block until the next
        let earlier = std::mem::take(&mut state.strategy.unknown);
        judge_strategy(state, tools, plan, now);
        let unread: Vec<&String> = state.strategy.unknown.keys().collect();
        state.strategy.summary["unknown_atoms"] = json!(unread);
        for (atom, why) in earlier {
            state.strategy.unknown.entry(atom).or_insert(why);
        }
    }
    strategy_overrides(state, plan)
}

/// Phases that have an id, in plan order.
fn phase_list(plan: &Value) -> Vec<(String, &Value)> {
    list(plan, "phases")
        .iter()
        .filter_map(|p| Some((item_id(p)?, p)))
        .collect()
}

/// Make `to` the active phase (persisted, noted in the change feed, raised as an alarm).
fn switch_phase(
    state: &mut State,
    phases: &[(String, &Value)],
    to: &str,
    why: String,
    now: GameHour,
) {
    let from = state.strategy.phase_id();
    if from.as_deref() == Some(to) {
        return;
    }
    state.strategy.phase = Some((to.to_string(), now));
    let revise = phases
        .iter()
        .find(|(id, _)| id == to)
        .is_some_and(|(_, p)| p["revise_on_enter"] == json!(true));
    let detail = json!({"from": from, "to": to, "reason": why, "game_hour": now,
        "revise_on_enter": revise});
    crate::watch::feed().note(crate::watch::PHASE_CHANGED, revise, detail.clone());
    state
        .strategy
        .news
        .push(json!({"kind": crate::watch::PHASE_CHANGED, "detail": detail}));
}

/// The contingency with this id.
fn contingency<'p>(plan: &'p Value, id: &str) -> Option<&'p Value> {
    list(plan, "contingencies")
        .iter()
        .find(|c| item_id(c).as_deref() == Some(id))
}

/// A field of a contingency's `then` object, else of the contingency itself.
fn then_field<'c>(c: &'c Value, key: &str) -> &'c Value {
    if c["then"].get(key).is_some() {
        &c["then"][key]
    } else {
        &c[key]
    }
}

/// Activate a contingency: remembered, noted (feed and alarm), and its phase switched to.
fn activate_contingency(
    state: &mut State,
    phases: &[(String, &Value)],
    c: &Value,
    forced: bool,
    why: String,
    now: GameHour,
) {
    let Some(id) = item_id(c) else { return };
    state
        .strategy
        .contingencies
        .insert(id.clone(), (now, forced));
    let overrides: Vec<String> = then_field(c, "overrides")
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    let phase = then_field(c, "phase").as_str().map(str::to_string);
    let detail = json!({"contingency": id, "reason": why, "phase": phase, "overrides": overrides,
        "revise": then_field(c, "revise"), "game_hour": now});
    crate::watch::feed().note(crate::watch::CONTINGENCY_TRIGGERED, true, detail.clone());
    state
        .strategy
        .news
        .push(json!({"kind": crate::watch::CONTINGENCY_TRIGGERED, "detail": detail}));
    if let Some(phase) = phase {
        if phases.iter().any(|(p, _)| *p == phase) {
            switch_phase(state, phases, &phase, format!("contingency {id}"), now);
        } else {
            state
                .strategy
                .unknown
                .entry(format!("contingency {id}"))
                .or_insert_with(|| format!("names phase {phase}, which the plan does not have"));
        }
    }
}

/// One judgement of the strategy keys: phases in order, then checkpoints that are due, then
/// contingencies (both may force a phase), then the goal; the digest block is rebuilt.
fn judge_strategy(state: &mut State, tools: &dyn Tools, plan: &Value, now: GameHour) {
    let phases = phase_list(plan);
    if state
        .strategy
        .phase
        .as_ref()
        .is_some_and(|(p, _)| !phases.iter().any(|(id, _)| id == p))
    {
        state.strategy.phase = None;
    }
    let known: HashSet<String> = list(plan, "contingencies")
        .iter()
        .filter_map(item_id)
        .collect();
    state
        .strategy
        .contingencies
        .retain(|id, _| known.contains(id));
    // a. phases: with none active the first whose `enter` holds; on through `exit`
    for _ in 0..=phases.len() {
        let current = state
            .strategy
            .phase
            .as_ref()
            .and_then(|(p, _)| phases.iter().position(|(id, _)| id == p));
        let (from, why) = match current {
            None => (0, "its enter holds".to_string()),
            Some(i) => {
                let exit = &phases[i].1["exit"];
                if exit.is_null() || judge(state, tools, exit, now) != Truth::True {
                    break;
                }
                (i + 1, format!("the exit of {} holds", phases[i].0))
            }
        };
        let mut next = None;
        for (id, p) in &phases[from.min(phases.len())..] {
            if p["enter"].is_null() || judge(state, tools, &p["enter"], now) == Truth::True {
                next = Some(id.clone());
                break;
            }
        }
        match next {
            Some(id) => switch_phase(state, &phases, &id, why, now),
            None => break,
        }
    }
    // b. checkpoints whose date has come, each settled once
    for c in list(plan, "checkpoints") {
        let Some(id) = item_id(c) else { continue };
        if state.strategy.checkpoints.contains_key(&id) {
            continue;
        }
        let Some(by) = c["by"].as_str().and_then(crate::plan_predicate::date_hour) else {
            state
                .strategy
                .unknown
                .entry(format!("checkpoint {id}"))
                .or_insert_with(|| "`by` is not a YYYY.MM.DD date".into());
            continue;
        };
        if now < by {
            continue;
        }
        let mut failed = Vec::new();
        let mut unknown = false;
        for must in list(c, "must") {
            match judge(state, tools, must, now) {
                Truth::True => {}
                Truth::False => failed.push(text_or(must)),
                Truth::Unknown => unknown = true,
            }
        }
        if failed.is_empty() {
            if !unknown {
                state.strategy.checkpoints.insert(id, (now, true));
            }
            continue;
        }
        state.strategy.checkpoints.insert(id.clone(), (now, false));
        let otherwise = text(&c["else"]).trim().to_string();
        let detail = json!({"checkpoint": id, "by": c["by"], "failed": failed,
            "else": otherwise, "game_hour": now});
        crate::watch::feed().note(crate::watch::CHECKPOINT_MISSED, true, detail.clone());
        state
            .strategy
            .news
            .push(json!({"kind": crate::watch::CHECKPOINT_MISSED, "detail": detail}));
        let why = format!("checkpoint {id} missed");
        if let Some(cid) = otherwise.strip_prefix("contingency:") {
            match contingency(plan, cid) {
                Some(c) => activate_contingency(state, &phases, c, true, why, now),
                None => {
                    state
                        .strategy
                        .unknown
                        .entry(format!("checkpoint {id}"))
                        .or_insert_with(|| {
                            format!("else names contingency {cid}, which the plan does not have")
                        });
                }
            }
        } else if otherwise != "revise" && !otherwise.is_empty() {
            let phase = otherwise.strip_prefix("phase:").unwrap_or(&otherwise);
            if phases.iter().any(|(p, _)| p == phase) {
                switch_phase(state, &phases, phase, why, now);
            } else {
                state
                    .strategy
                    .unknown
                    .entry(format!("checkpoint {id}"))
                    .or_insert_with(|| {
                        format!("else names phase {phase}, which the plan does not have")
                    });
            }
        }
    }
    // c. contingencies: active while the trigger holds (unknown leaves them as they are)
    for c in list(plan, "contingencies") {
        let Some(id) = item_id(c) else { continue };
        let when = if c["when"].is_null() {
            &c["trigger"]
        } else {
            &c["when"]
        };
        if when.is_null() {
            continue;
        }
        let truth = judge(state, tools, when, now);
        match (truth, state.strategy.contingencies.get(&id).copied()) {
            (Truth::True, None) => {
                activate_contingency(state, &phases, c, false, "its trigger holds".into(), now)
            }
            (Truth::False, Some((_, false))) => {
                state.strategy.contingencies.remove(&id);
            }
            _ => {}
        }
    }
    // d. the goal
    let goal = &plan["goal"];
    let mut goal_block = Value::Null;
    if goal.is_object() {
        let reached = if goal["predicates"].is_null() {
            Truth::Unknown
        } else {
            judge(state, tools, &goal["predicates"], now)
        };
        let mut failing = Vec::new();
        for f in list(goal, "fail_predicates") {
            if judge(state, tools, f, now) == Truth::True {
                failing.push(text_or(f));
            }
        }
        let deadline = goal["deadline"]
            .as_str()
            .and_then(crate::plan_predicate::date_hour);
        let margin = crate::alarms::goal_risk_days(plan).map(|d| (d * DAY as f64) as GameHour);
        let label = text(&goal["deadline"]);
        let (status, why) = match deadline {
            _ if reached == Truth::True => ("reached", "every goal predicate holds".to_string()),
            _ if !failing.is_empty() => (
                "at_risk",
                format!("fail predicate(s) hold: {}", failing.join(", ")),
            ),
            Some(at) if now >= at => (
                "at_risk",
                format!("the deadline {label} has passed and the goal is not reached"),
            ),
            Some(at) if margin.is_some_and(|m| now + m >= at) => (
                "at_risk",
                format!(
                    "{} game days to the deadline {label} and the goal is not reached",
                    (at - now) / DAY
                ),
            ),
            _ if reached == Truth::Unknown => (
                "open",
                "some goal predicates read unknown (see predicates_unknown)".to_string(),
            ),
            _ => ("open", "not reached yet".to_string()),
        };
        if state.strategy.goal.as_deref() != Some(status) {
            if status != "open" {
                let kind = if status == "reached" {
                    crate::watch::GOAL_REACHED
                } else {
                    crate::watch::GOAL_AT_RISK
                };
                crate::watch::feed().note(
                    kind,
                    true,
                    json!({"status": status, "why": why, "game_hour": now}),
                );
            }
            state.strategy.goal = Some(status.to_string());
        }
        goal_block = json!({"status": status, "why": why, "deadline": goal["deadline"],
            "predicates": reached.label()});
    }
    let settled = |met: bool| -> Vec<String> {
        let mut ids: Vec<String> = state
            .strategy
            .checkpoints
            .iter()
            .filter(|(_, (_, m))| *m == met)
            .map(|(id, _)| id.clone())
            .collect();
        ids.sort();
        ids
    };
    let pending: Vec<String> = list(plan, "checkpoints")
        .iter()
        .filter_map(item_id)
        .filter(|id| !state.strategy.checkpoints.contains_key(id))
        .collect();
    let mut active: Vec<String> = state.strategy.contingencies.keys().cloned().collect();
    active.sort();
    state.strategy.summary = json!({
        "phase": state.strategy.phase_id(),
        "phase_since_hour": state.strategy.phase.as_ref().map(|(_, h)| h),
        "goal": goal_block,
        "checkpoints": {"met": settled(true), "missed": settled(false), "pending": pending},
        "contingencies_active": active,
        "judged_at_hour": now,
    });
}

/// The plan with the active phase's and the active contingencies' overrides merged over it
/// (None when nothing is overridden).
fn strategy_overrides(state: &State, plan: &Value) -> Option<Value> {
    let mut layers: Vec<&Value> = Vec::new();
    if let Some((phase, _)) = &state.strategy.phase {
        if let Some((_, p)) = phase_list(plan).into_iter().find(|(id, _)| id == phase) {
            layers.push(&p["overrides"]);
        }
    }
    for c in list(plan, "contingencies") {
        if item_id(c).is_some_and(|id| state.strategy.contingencies.contains_key(&id)) {
            layers.push(then_field(c, "overrides"));
        }
    }
    let layers: Vec<&Map<String, Value>> = layers
        .into_iter()
        .filter_map(Value::as_object)
        .filter(|o| !o.is_empty())
        .collect();
    if layers.is_empty() {
        return None;
    }
    let mut merged = plan.clone();
    for layer in layers {
        apply_overrides(&mut merged, layer);
    }
    Some(merged)
}

/// Overrides over a plan: top-level keys are replaced (null removes one), `war` and `alarms` are
/// merged key by key; the strategy keys, `active` and `_meta` are never overridden.
pub fn apply_overrides(plan: &mut Value, overrides: &Map<String, Value>) {
    let Some(object) = plan.as_object_mut() else {
        return;
    };
    for (key, value) in overrides {
        if matches!(
            key.as_str(),
            "goal" | "phases" | "checkpoints" | "contingencies" | "active" | "_meta"
        ) {
            continue;
        }
        if value.is_null() {
            object.remove(key);
            continue;
        }
        if let (Some(parts), true) = (value.as_object(), matches!(key.as_str(), "war" | "alarms")) {
            if let Some(Value::Object(base)) = object.get_mut(key) {
                for (k, v) in parts {
                    if v.is_null() {
                        base.remove(k);
                    } else {
                        base.insert(k.clone(), v.clone());
                    }
                }
                continue;
            }
        }
        object.insert(key.clone(), value.clone());
    }
}

/// Whether the plan forbids a decision (`decision_rules` with "never": true): no path takes it.
pub fn decision_forbidden(plan: &Value, decision: &str) -> bool {
    list(plan, "decision_rules")
        .iter()
        .any(|r| r["never"] == json!(true) && text(&r["decision"]) == decision)
}

/// 5. Decisions: `decision_rules` in plan order (when enabled by the game and `when` holds; the
///    row of `target`; at most `max` successful takes), then the plain `decisions` list. Nothing
///    a never-take rule names is ever taken. The decisions panel is opened only when something
///    could be taken.
fn decisions_step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let wanted: Vec<String> = strings(plan, "decisions")
        .into_iter()
        .filter(|d| !decision_forbidden(plan, d))
        .collect();
    // (rule id, decision, target, successful takes so far, skip key)
    let mut rules: Vec<(String, String, Option<String>, usize, String)> = Vec::new();
    for (index, rule) in list(plan, "decision_rules").iter().enumerate() {
        let key = text(&rule["decision"]).to_string();
        if key.is_empty() || rule["never"] == json!(true) || decision_forbidden(plan, &key) {
            continue;
        }
        let id = item_id(rule).unwrap_or_else(|| format!("{index}:{key}"));
        let target = match &rule["target"] {
            Value::String(t) if !t.trim().is_empty() => Some(t.trim().to_string()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        };
        let prefix = format!("decrule:{id}#");
        let taken = state.done.keys().filter(|k| k.starts_with(&prefix)).count();
        if rule["max"].as_u64().is_some_and(|max| taken as u64 >= max) {
            continue;
        }
        let skip = target
            .as_ref()
            .map_or_else(|| key.clone(), |t| format!("{key}@{t}"));
        if state.skipped(now, "dec", &skip) {
            continue;
        }
        if !rule["when"].is_null() && !judge(state, tools, &rule["when"], now).is_true() {
            continue;
        }
        rules.push((id, key, target, taken, skip));
    }
    if wanted.is_empty() && rules.is_empty() {
        return Ok(None);
    }
    let offered = tools.call("hoi4_decisions_list", &json!({"lease_token": lease}))?;
    let ready =
        |d: &Value| d["enabled"] == json!(true) && !d["active_timer"].as_bool().unwrap_or(false);
    for (id, key, target, taken, skip) in rules {
        let wanted_row = |d: &&Value| {
            text(&d["decision"]) == key
                && ready(d)
                && target
                    .as_ref()
                    .is_none_or(|t| text(&d["title"]).to_lowercase().contains(&t.to_lowercase()))
        };
        if !list(&offered, "decisions").iter().any(|d| wanted_row(&d)) {
            continue;
        }
        let mut args = json!({"decision": key});
        if let Some(t) = &target {
            args["target"] = json!(t);
        }
        let (ok, why) = state.act(tools, lease, "hoi4_decision_take", args, "dr")?;
        state.skip(now, "dec", &skip, if ok { 30 * DAY } else { 3 * DAY });
        if ok {
            state
                .done
                .insert(format!("decrule:{id}#{}", taken + 1), now);
        }
        let on = target.map_or_else(String::new, |t| format!(" on {t}"));
        return Ok(Some(format!("decision {key}{on} (rule {id}): {ok} {why}")));
    }
    for d in list(&offered, "decisions") {
        let key = text(&d["decision"]);
        if wanted.iter().any(|w| w == key) && ready(d) && !state.skipped(now, "dec", key) {
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_decision_take",
                json!({"decision": key}),
                "de",
            )?;
            state.skip(now, "dec", key, if ok { 30 * DAY } else { 3 * DAY });
            return Ok(Some(format!("decision {key}: {ok} {why}")));
        }
    }
    Ok(None)
}

/// Option text reduced for matching: colour codes, icons and $vars$ dropped, lower case, accents
/// folded, punctuation as spaces.
fn option_words(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '§' => {
                chars.next();
            }
            '£' | '$' => {
                for n in chars.by_ref() {
                    if n == c {
                        break;
                    }
                }
            }
            c if c.is_alphanumeric() => out.push(c),
            _ => out.push(' '),
        }
    }
    plain(&out).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The index of the option named `wanted`: its text (or `name`/`key`/`text` when hoi4_events
/// lists objects) equal after `option_words`, else the one option that contains it.
fn option_index(options: &[Value], wanted: &str) -> Option<usize> {
    let want = option_words(wanted);
    if want.is_empty() {
        return None;
    }
    let names: Vec<Vec<String>> = options
        .iter()
        .map(|o| match o {
            Value::String(s) => vec![option_words(s)],
            other => ["name", "key", "text"]
                .iter()
                .filter_map(|k| other[*k].as_str())
                .map(option_words)
                .collect(),
        })
        .collect();
    if let Some(i) = names.iter().position(|n| n.contains(&want)) {
        return Some(i);
    }
    let partial: Vec<usize> = names
        .iter()
        .enumerate()
        .filter(|(_, n)| n.iter().any(|t| t.contains(&want)))
        .map(|(i, _)| i)
        .collect();
    match partial.as_slice() {
        [only] => Some(*only),
        _ => None,
    }
}

/// 1. `event_rules` for the open event: None when no rule decides (event_options and the
///    defaults do), Some(None) when a rule's condition reads unknown (the event waits), else the
///    option index and how it was chosen.
fn event_rule_choice(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    key: &str,
    top: &Value,
    now: GameHour,
) -> Option<Option<(u64, String)>> {
    let options = list(top, "options");
    for rule in list(plan, "event_rules") {
        if text(&rule["event"]) != key {
            continue;
        }
        let condition = if rule["when"].is_null() {
            &rule["condition"]
        } else {
            &rule["when"]
        };
        let truth = if condition.is_null() {
            Truth::True
        } else {
            judge(state, tools, condition, now)
        };
        let name = match truth {
            Truth::True => text(&rule["option_name"]),
            Truth::False => match rule["else_option_name"].as_str() {
                Some(other) => other,
                None => continue,
            },
            Truth::Unknown => return Some(None),
        };
        if let Some(i) = option_index(options, name) {
            return Some(Some((i as u64, format!(" (event_rules '{name}')"))));
        }
        if let Some((fallback, i)) = rule["fallback_option_name"]
            .as_str()
            .and_then(|f| Some((f, option_index(options, f)?)))
        {
            return Some(Some((
                i as u64,
                format!(" (event_rules fallback '{fallback}'; '{name}' not offered)"),
            )));
        }
        if let Some(i) = rule["fallback_index"]
            .as_u64()
            .filter(|i| (*i as usize) < options.len())
        {
            return Some(Some((
                i,
                format!(" (event_rules fallback_index; '{name}' not offered)"),
            )));
        }
        let offered: Vec<String> = options.iter().map(text_or).collect();
        state
            .strategy
            .unknown
            .entry(format!("event_rules {key}"))
            .or_insert_with(|| {
                format!("option '{name}' is not among {offered:?}; event_options decides")
            });
        return None;
    }
    None
}

/// The single `stop_on_date` pause: `hoi4_time paused:true` under its own request id,
/// skipped when the overview already says paused. The outcome text is final: a refused
/// or uncertain pause is reported and never retried under a new id.
fn date_stop_pause(state: &mut State, tools: &dyn Tools, lease: &str, pause: bool) -> String {
    if !pause {
        return "not requested (stop_on_date_pause false)".into();
    }
    if tools
        .call("hoi4_overview", &json!({}))
        .is_ok_and(|overview| overview["paused"] == json!(true))
    {
        return "already paused".into();
    }
    // The server's own date watcher shares the journal: if it fired or is
    // sending the pause right now, report that instead of a second pause.
    if let Ok(watcher) = tools.call("hoi4_date_stop", &json!({})) {
        let outcome = watcher["outcome"].as_str().unwrap_or("");
        if watcher["fired"] == json!(true) && !outcome.is_empty() {
            return format!("{outcome} (server date watcher)");
        }
        if watcher["in_flight"] == json!(true) {
            return "pause in flight (server date watcher); not re-issued".into();
        }
    }
    match state.act(tools, lease, "hoi4_time", json!({"paused": true}), "sd") {
        Ok((true, _)) => "paused".into(),
        Ok((false, why)) => format!("pause refused ({why}); not retried"),
        Err(e) if e.code == "action_uncertain" => {
            format!(
                "pause uncertain ({}); not retried, settle by hand",
                e.message
            )
        }
        Err(e) => format!("pause failed ({}: {}); not retried", e.code, e.message),
    }
}

/// `focus_blocked` digest entries: `key:reason` for every listed focus that is
/// still on its skip timer after a refusal.
fn focus_blocked(state: &State, plan: &Value, now: GameHour) -> Vec<String> {
    let mut keys = strings(plan, "focus_path");
    keys.extend(strings(plan, "focus_fallback"));
    keys.iter()
        .filter(|k| state.skipped(now, "focus", k))
        .filter_map(|k| {
            state
                .stages
                .focus_why
                .get(k)
                .map(|why| format!("{k}:{why}"))
        })
        .collect()
}

/// The current in-game hour from `hoi4_overview`'s `date.raw`. The clock never
/// runs backwards: when the read fails or lacks a date the last known hour is
/// kept, so timers neither expire early nor stall the runner.
pub fn game_hour(state: &mut State, tools: &dyn Tools) -> GameHour {
    if let Some(raw) = tools
        .call("hoi4_overview", &json!({}))
        .ok()
        .and_then(|overview| overview["date"]["raw"].as_i64())
    {
        state.last_hour = state.last_hour.max(raw);
    }
    state.last_hour
}

/// Read `plan.json` (missing means an empty plan).
pub fn read_plan(path: &Path) -> Result<Value> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| Error::new("invalid_plan", format!("plan.json is not valid JSON: {e}"))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.into()),
    }
}

/// Merge `changes` into the plan file (a null value removes a key) and return it.
pub fn merge_plan(path: &Path, changes: &Map<String, Value>) -> Result<Value> {
    let mut plan = read_plan(path)?;
    let object = plan
        .as_object_mut()
        .ok_or_else(|| Error::new("invalid_plan", "plan.json must hold an object"))?;
    for (key, value) in changes {
        if value.is_null() {
            object.remove(key);
        } else {
            object.insert(key.clone(), value.clone());
        }
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(
        &temporary,
        serde_json::to_vec_pretty(&plan).map_err(|e| Error::new("invalid_plan", e.to_string()))?,
    )?;
    std::fs::rename(&temporary, path)?;
    Ok(plan)
}

/// `plan_state.json` next to `plan.json` (`<stem>_state.json` for another plan file name).
pub fn state_path(plan_path: &Path) -> std::path::PathBuf {
    let stem = plan_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("plan");
    plan_path.with_file_name(format!("{stem}_state.json"))
}

/// The campaign the memory belongs to: the player tag (hoi4_overview), the game's playthrough
/// id (the `handle_scope` of hoi4_construction_queue, which distinguishes campaigns) and the
/// plan's `_meta.campaign` when the plan names one. None while the overview cannot be read.
fn campaign_identity(tools: &dyn Tools, plan: &Value) -> Option<Value> {
    let overview = tools.call("hoi4_overview", &json!({})).ok()?;
    let playthrough = tools
        .call("hoi4_construction_queue", &json!({"limit": 1}))
        .ok()
        .and_then(|q| {
            q["handle_scope"]["playthrough_id"]
                .as_str()
                .map(String::from)
        });
    Some(json!({
        "player": overview["player"].as_str(),
        "playthrough": playthrough,
        "campaign": plan["_meta"]["campaign"].as_str(),
    }))
}

/// Same campaign unless a field both sides know differs (an unknown field never rejects).
fn same_campaign(saved: &Value, now: &Value) -> bool {
    ["player", "playthrough", "campaign"].iter().all(|field| {
        match (saved[field].as_str(), now[field].as_str()) {
            (Some(a), Some(b)) if !a.is_empty() && !b.is_empty() => a == b,
            _ => true,
        }
    })
}

impl State {
    /// The part of the memory that must survive a server restart: one-shot work done
    /// (politics, doctrines, production lines), construction_plan items queued, running skip
    /// timers (decisions taken wait 30 days), the failure ledger and the army_layout roles.
    fn memory(&self, now: GameHour) -> Value {
        let skips: HashMap<&String, &GameHour> =
            self.bad.iter().filter(|(_, until)| **until > now).collect();
        let failures: HashMap<&String, &Vec<GameHour>> = self
            .failures
            .iter()
            .filter(|(_, times)| times.iter().any(|t| now - *t < 10 * DAY))
            .collect();
        let layout: HashMap<&String, &String> =
            self.layout.iter().filter(|(_, a)| !a.is_empty()).collect();
        json!({"done": self.done, "plan_built": self.plan_built, "skips": skips,
            "failures": failures, "layout": layout, "strategy": self.strategy.memory()})
    }
    /// Take back a saved memory. Work dated after `now` is dropped (an earlier save was
    /// loaded: in that game it has not happened); after such a reload the timers and the
    /// failure ledger are dropped too.
    fn restore(&mut self, saved: &Value, now: GameHour) {
        let hours = |key: &str| -> Vec<(String, GameHour)> {
            saved[key]
                .as_object()
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| Some((k.clone(), v.as_i64()?)))
                        .collect()
                })
                .unwrap_or_default()
        };
        for (key, at) in hours("done") {
            if at <= now {
                self.done.entry(key).or_insert(at);
            }
        }
        for (key, at) in hours("plan_built") {
            if at <= now {
                self.plan_built.entry(key).or_insert(at);
            }
        }
        let rewound = saved["game_hour"].as_i64().is_some_and(|h| now < h);
        if !rewound {
            for (key, until) in hours("skips") {
                if until > now {
                    self.bad.entry(key).or_insert(until);
                }
            }
            if let Some(failures) = saved["failures"].as_object() {
                for (signature, times) in failures {
                    let times: Vec<GameHour> = times
                        .as_array()
                        .map(|t| t.iter().filter_map(Value::as_i64).collect())
                        .unwrap_or_default();
                    self.failures.entry(signature.clone()).or_insert(times);
                }
            }
        }
        if let Some(layout) = saved["layout"].as_object() {
            for (hint, army) in layout {
                if let Some(army) = army.as_str().filter(|a| !a.is_empty()) {
                    self.layout
                        .entry(hint.clone())
                        .or_insert_with(|| army.to_string());
                }
            }
        }
        self.strategy.restore(&saved["strategy"], now, rewound);
    }
}

/// Once per server life, when the game date is known: read `plan_state.json` back unless it
/// belongs to another campaign (then it is ignored and later overwritten).
fn restore_memory(state: &mut State, tools: &dyn Tools, plan: &Value, path: &Path, now: GameHour) {
    let Some(identity) = campaign_identity(tools, plan) else {
        return;
    };
    state.persist.restored = true;
    state.persist.saved = None;
    let file = state_path(path);
    match std::fs::read(&file) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => state.persist.note = Some(format!("plan_state.json unreadable: {e}")),
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Err(e) => state.persist.note = Some(format!("plan_state.json is not JSON: {e}")),
            Ok(saved) if !same_campaign(&saved["identity"], &identity) => {
                state.persist.note = Some(format!(
                    "plan_state.json belongs to another campaign ({}), ignored",
                    saved["identity"]
                ));
            }
            Ok(saved) => state.restore(&saved, now),
        },
    }
    state.persist.identity = Some(identity);
}

/// Write the memory when it changed (temp file and rename, like merge_plan).
fn save_memory(state: &mut State, path: &Path, now: GameHour) {
    let memory = state.memory(now);
    if state.persist.saved.as_ref() == Some(&memory) {
        return;
    }
    let mut body = memory.clone();
    body["version"] = json!(1);
    body["identity"] = state.persist.identity.clone().unwrap_or(Value::Null);
    body["game_hour"] = json!(now);
    let file = state_path(path);
    let temporary = file.with_extension("json.tmp");
    let written = serde_json::to_vec_pretty(&body)
        .map_err(|e| Error::new("invalid_plan", e.to_string()))
        .and_then(|bytes| Ok(std::fs::write(&temporary, bytes)?))
        .and_then(|()| Ok(std::fs::rename(&temporary, &file)?));
    match written {
        Ok(()) => state.persist.saved = Some(memory),
        Err(e) => state.persist.note = Some(format!("plan_state.json not written: {}", e.message)),
    }
}

/// Run the plan for up to `seconds`, returning early when an event needs the
/// model's choice or the runner must stop (peace conference). The plan file is
/// read before every tick, so changes apply at once.
pub fn run(
    state: &mut State,
    tools: &dyn Tools,
    plan_path: &Path,
    lease: &str,
    seconds: u64,
) -> Result<Value> {
    let timed = Timed {
        inner: tools,
        spent: Default::default(),
    };
    let tools: &dyn Tools = &timed;
    let start = std::time::Instant::now();
    let deadline = start + Duration::from_secs(seconds);
    let mut actions: Vec<String> = Vec::new();
    let mut construction: Option<String> = None;
    let mut research_line: Option<(String, bool)> = None;
    let mut focus_line: Option<(String, bool)> = None;
    let mut stopped_on_date = false;
    state.stages.fallback_used.clear();
    state.strategy.unknown.clear();
    state.strategy.news.clear();
    let reason = loop {
        let plan = read_plan(plan_path)?;
        let now = game_hour(state, tools);
        if !state.persist.restored && now > 0 {
            restore_memory(state, tools, &plan, plan_path, now);
        }
        let step = tick(state, tools, &plan, lease, now);
        if state.persist.restored {
            save_memory(state, plan_path, now);
        }
        // the digest survives a failing tick: it says what the stage saw
        match state.con.digest.take() {
            Some(digest) => construction = Some(digest),
            None if construction.is_none() && !list(&plan, "construction").is_empty() => {
                construction =
                    Some("construction: not reached this tick (an earlier step acted)".into())
            }
            None => {}
        }
        if let Some(line) = state.stages.research.take() {
            research_line = Some(line);
        }
        if let Some(line) = state.stages.focus.take() {
            focus_line = Some(line);
        }
        let step = step?;
        let pause = match step {
            Tick::Acted(what) => {
                // A failed action ends the run: retrying it at once only repeats the failure
                // (live 8 Nov 2026: an event behind a system dialog was clicked 346 times).
                let failed = what.contains(": false");
                actions.push(what);
                if failed {
                    break json!("action failed");
                }
                // Never sleep while holding the game: the player's calls wait behind a run
                // (w17: plan runs held the broker 63% of the wall time, 4 s sleeps included).
                // After 3 s of work the run returns so the next caller gets the input.
                if start.elapsed() >= Duration::from_secs(3) {
                    break json!("yield");
                }
                Duration::ZERO
            }
            Tick::Stop(why) => break json!({"stopped": why}),
            Tick::Waiting { event, new } => {
                // The model sees the event once; later runs wait for the
                // plan's patience and then answer it.
                if new {
                    break json!({"event_open": event});
                }
                Duration::from_secs(12)
            }
            Tick::DateStop(why) => {
                stopped_on_date = true;
                break json!(why);
            }
            Tick::Idle => break json!("plan inactive (active:false)"),
            Tick::Nothing => Duration::from_secs(12),
        };
        if std::time::Instant::now() + pause >= deadline {
            break json!("deadline");
        }
        std::thread::sleep(pause);
    };
    let plan = read_plan(plan_path).unwrap_or_else(|_| json!({}));
    // the digest uses the plan as the ticks did: with the active phase's overrides (alarms too)
    let plan = strategy_overrides(state, &plan).unwrap_or(plan);
    let mut alarms = observe_alarms(tools, &plan);
    alarms.extend(plan_manage::take_alarms(state));
    alarms.extend(plan_military::take_alarms(state));
    if let Some(alarm) = construction_alarm(state, tools, &plan, construction.as_deref()) {
        alarms.push(alarm);
    }
    if research_line.is_none() && !strings(&plan, "research_priority").is_empty() {
        research_line = Some((
            "research: not reached this tick (an earlier step acted)".into(),
            false,
        ));
    }
    if focus_line.is_none() && !strings(&plan, "focus_path").is_empty() {
        focus_line = Some((
            "focus: not reached this tick (an earlier step acted)".into(),
            false,
        ));
    }
    let quiet = |code: &str| plan["alarms"][code] == json!(false);
    if research_line.as_ref().is_some_and(|l| l.1) && !quiet("research_slot_idle_plan_exhausted") {
        alarms.push(json!({"code": "research_slot_idle_plan_exhausted",
            "message": research_line.as_ref().map_or("", |l| l.0.as_str()),
            "fix": {"tool": "hoi4_plan", "args_hint": {"op": "set", "plan": {"research_priority": ["<more technology keys>"]}}}}));
    }
    if focus_line.as_ref().is_some_and(|l| l.1) && !quiet("no_active_focus_plan_exhausted") {
        alarms.push(json!({"code": "no_active_focus_plan_exhausted",
            "message": focus_line.as_ref().map_or("", |l| l.0.as_str()),
            "fix": {"tool": "hoi4_plan", "args_hint": {"op": "set", "plan": {"focus_path": ["<more focus keys>"], "focus_fallback": ["<keys>"]}}}}));
    }
    if !state.stages.fallback_used.is_empty() && !quiet("research_fallback_used") {
        alarms.push(json!({"code": "research_fallback_used",
            "message": format!("research_priority had nothing startable; the catalogue fallback picked: {}",
                state.stages.fallback_used.join(", ")),
            "fix": {"tool": "hoi4_plan", "args_hint": {"op": "set", "plan": {"research_priority": ["<wanted keys>"], "research_fallback": "priority_only"}}}}));
    }
    alarms.extend(crate::alarms::strategy(
        &plan,
        &state.strategy.summary,
        &state.strategy.news,
    ));
    let mut result = json!({"reason": reason, "actions": actions,
        "alarms": alarms,
        "game_hour": state.last_hour,
        "elapsed_seconds": start.elapsed().as_secs_f64()});
    let mut timing: Vec<(String, u32, u128)> = timed
        .spent
        .borrow()
        .iter()
        .map(|(k, v)| (k.clone(), v.0, v.1))
        .collect();
    timing.sort_by_key(|t| std::cmp::Reverse(t.2));
    result["timing_ms"] = json!(timing
        .iter()
        .take(8)
        .map(|(k, n, ms)| format!("{k} x{n} {ms}"))
        .collect::<Vec<_>>());
    if let Some(digest) = construction {
        result["construction"] = json!(digest);
    }
    if let Some((line, _)) = research_line {
        result["research"] = json!(line);
    }
    if let Some((line, _)) = focus_line {
        result["focus"] = json!(line);
    }
    let blocked = focus_blocked(state, &plan, state.last_hour);
    if !blocked.is_empty() {
        result["focus_blocked"] = json!(blocked);
    }
    if let Some(note) = state.persist.note.take() {
        result["plan_state"] = json!(note);
    }
    if let Some(manage) = plan_manage::digest(state) {
        result["manage"] = manage;
    }
    if let Some(military) = plan_military::digest(state) {
        result["military"] = military;
    }
    if !state.strategy.summary.is_null() {
        result["strategy"] = state.strategy.summary.clone();
    }
    if !state.strategy.news.is_empty() {
        result["strategy_changes"] = json!(state.strategy.news);
    }
    if !state.strategy.unknown.is_empty() {
        result["predicates_unknown"] = json!(state
            .strategy
            .unknown
            .iter()
            .map(|(atom, why)| json!({"atom": atom, "why": why}))
            .collect::<Vec<_>>());
    }
    if stopped_on_date {
        result["stopped_on_date"] = json!(true);
        result["stop_on_date_pause"] =
            json!(state.date_stop.as_ref().map_or("", |(_, o)| o.as_str()));
    }
    Ok(result)
}

/// `construction_stalled`: civilian factories sat idle for `alarms.construction_stall_days`
/// (default 5) game days although the plan lists construction and nothing was queued.
fn construction_alarm(
    state: &State,
    tools: &dyn Tools,
    plan: &Value,
    digest: Option<&str>,
) -> Option<Value> {
    if list(plan, "construction").is_empty()
        || plan["alarms"]["construction_stalled"] == json!(false)
    {
        return None;
    }
    let days = plan["alarms"]["construction_stall_days"]
        .as_i64()
        .unwrap_or(5);
    let economy = tools.call("hoi4_economy", &json!({})).ok()?;
    let idle = economy["civilian_factories_idle"]
        .as_i64()
        .or(economy["civilian_factories_available_for_projects"].as_i64())?;
    if idle < plan["construction_min_idle"].as_i64().unwrap_or(4) {
        return None;
    }
    let since = state.con.last_queue.or(state.con.baseline)?;
    let waited = state.last_hour - since;
    (waited >= days * DAY).then(|| {
        json!({"code": "construction_stalled",
            "message": format!("{idle} civilian factories idle and nothing queued for {} game days; {}",
                waited / DAY, digest.unwrap_or("construction stage did not run")),
            "fix": {"tool": "hoi4_state_slots", "args_hint": {"state_ids": "<own states>"}}})
    })
}

/// Read what the alarms need (pure reads, never input) and compute them. A
/// failed read just leaves that observation out.
pub fn observe_alarms(tools: &dyn Tools, plan: &Value) -> Vec<Value> {
    let read = |name: &str, args: Value| tools.call(name, &args).ok();
    let economy = read("hoi4_economy", json!({}));
    let research = read("hoi4_research", json!({"limit": 1}));
    let focus = read("hoi4_focus_completed", json!({"limit": 1}));
    let armies = read("hoi4_armies", json!({}));
    let events = read("hoi4_events", json!({}));
    let lines = read("hoi4_production_lines", json!({"limit": 128}));
    let wars = read("hoi4_wars", json!({}));
    let game_hour = read("hoi4_overview", json!({})).and_then(|o| o["date"]["raw"].as_i64());
    crate::alarms::compute(
        plan,
        &crate::alarms::Observations {
            economy: economy.as_ref(),
            research: research.as_ref(),
            focus: focus.as_ref(),
            armies: armies.as_ref(),
            events: events.as_ref(),
            production_lines: lines.as_ref(),
            wars: wars.as_ref(),
            game_hour,
        },
    )
}

/// Air mission for a wing's aircraft type in war: fighters (legacy `fighter_equipment_*`,
/// By Blood Alone small airframes and heavy fighters) fly air superiority, CAS flies close air
/// support; other types (bombers, naval bombers, transports) are left to the player.
fn war_mission(kind: &str) -> Option<&'static str> {
    let k = kind.to_lowercase();
    if k.contains("cas") {
        Some("cas")
    } else if k.contains("fighter")
        || k.starts_with("small_plane_airframe")
        || k.starts_with("cv_small_plane_airframe")
    {
        Some("air_superiority")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn war_mission_covers_designer_airframes() {
        assert_eq!(war_mission("fighter_equipment_1"), Some("air_superiority"));
        assert_eq!(
            war_mission("small_plane_airframe_1"),
            Some("air_superiority")
        );
        assert_eq!(
            war_mission("cv_small_plane_airframe_1"),
            Some("air_superiority")
        );
        assert_eq!(war_mission("small_plane_cas_airframe_1"), Some("cas"));
        assert_eq!(war_mission("CAS_equipment_1"), Some("cas"));
        assert_eq!(war_mission("medium_plane_airframe_1"), None);
        assert_eq!(war_mission("small_plane_naval_bomber_airframe_1"), None);
    }
    use super::*;
    use std::cell::{Cell, RefCell};

    const START: GameHour = 1_000_000;

    struct Fake {
        calls: RefCell<Vec<(String, Value)>>,
        replies: HashMap<&'static str, Value>,
    }
    impl Tools for Fake {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.calls
                .borrow_mut()
                .push((name.to_string(), args.clone()));
            if name.starts_with("hoi4_") && args.get("request_id").is_some() {
                return Ok(json!({"receipt": {"status": "Verified"}}));
            }
            Ok(self.replies.get(name).cloned().unwrap_or_else(|| json!({})))
        }
    }
    fn fake(replies: Vec<(&'static str, Value)>) -> Fake {
        Fake {
            calls: RefCell::new(Vec::new()),
            replies: replies.into_iter().collect(),
        }
    }
    /// Tool names called; the cheap available-techs query is labelled apart from the catalogue
    /// walk (only the walk is what the priority-only tests forbid).
    fn names(f: &Fake) -> Vec<String> {
        f.calls
            .borrow()
            .iter()
            .map(|(n, a)| {
                if a["available"] == json!(true) {
                    format!("{n}(available)")
                } else {
                    n.clone()
                }
            })
            .collect()
    }

    struct ActionReplies<'a> {
        reads: &'a dyn Tools,
        actions: RefCell<Vec<(String, Value)>>,
        replies: RefCell<std::collections::VecDeque<Result<Value>>>,
    }
    impl Tools for ActionReplies<'_> {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            if args.get("request_id").is_some() {
                self.actions.borrow_mut().push((name.into(), args.clone()));
                self.replies
                    .borrow_mut()
                    .pop_front()
                    .expect("unexpected extra action")
            } else {
                self.reads.call(name, args)
            }
        }
    }
    fn uncertain() -> Result<Value> {
        Err(Error::new(
            "action_uncertain",
            "Own readback unavailable after input",
        ))
    }

    #[test]
    fn uncertain_research_stops_before_another_priority_or_catalog_fallback() {
        let reads = fake(vec![(
            "hoi4_research",
            json!({"slots": [{"researching": null}]}),
        )]);
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([uncertain(), uncertain()].into()),
        };
        let mut state = State::default();
        let error = tick(
            &mut state,
            &tools,
            &json!({"research_priority": ["radio", "construction1"]}),
            "l",
            START,
        )
        .unwrap_err();
        assert_eq!(error.code, "action_uncertain");
        let calls = tools.actions.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], calls[1], "Only the same request may be settled");
        assert_eq!(calls[0].1["technology"], "radio");
        assert!(!names(&reads).contains(&"hoi4_technology_list".into()));
        assert!(
            state.bad.is_empty(),
            "Unfinished research must not be skipped"
        );
    }

    #[test]
    fn same_request_settled_as_verified_completes_normally() {
        let reads = fake(vec![]);
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new(
                [uncertain(), Ok(json!({"receipt":{"status":"Verified"}}))].into(),
            ),
        };
        let result = State::default()
            .act(
                &tools,
                "l",
                "hoi4_research_start",
                json!({"technology":"radio", "slot":0}),
                "rs",
            )
            .unwrap();
        assert_eq!(result, (true, "Verified".into()));
        let calls = tools.actions.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], calls[1]);
    }

    #[test]
    fn nonterminal_or_missing_receipts_stop_instead_of_triggering_fallbacks() {
        let reads = fake(vec![]);
        for body in [
            json!({}),
            json!({"receipt":{"status":"Dispatching"}}),
            json!({"receipt":{"status":"Unknown"}}),
        ] {
            let tools = ActionReplies {
                reads: &reads,
                actions: RefCell::new(vec![]),
                replies: RefCell::new([Ok(body)].into()),
            };
            let error = State::default()
                .act(
                    &tools,
                    "l",
                    "hoi4_research_start",
                    json!({"technology":"radio", "slot":0}),
                    "rs",
                )
                .unwrap_err();
            assert_eq!(error.code, "action_uncertain");
            assert_eq!(tools.actions.borrow().len(), 1);
        }
    }

    #[test]
    fn refused_action_diagnostics_truncate_unicode_without_panicking() {
        let reads = fake(vec![]);
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([Err(Error::new("invalid_target", "界".repeat(100)))].into()),
        };
        let (ok, reason) = State::default()
            .act(&tools, "l", "hoi4_army_front", json!({}), "wr")
            .unwrap();
        assert!(!ok);
        assert!(reason.starts_with("invalid_target: 界"));
        assert!(reason.len() <= 160);
    }

    #[test]
    fn starts_the_first_unresearched_priority_in_an_empty_slot() {
        let tools = fake(vec![(
            "hoi4_research",
            json!({"slots": [{"researching": "radio"}, {"researching": null}],
                   "researched": [{"id": "construction1"}]}),
        )]);
        let plan = json!({"research_priority": ["construction1", "radio", "support_weapons"]});
        let mut state = State::default();
        let step = tick(&mut state, &tools, &plan, "lease", START).unwrap();
        assert_eq!(step, Tick::Acted("research support_weapons slot 1".into()));
        let calls = tools.calls.borrow();
        let (name, args) = calls.last().unwrap();
        assert_eq!(name, "hoi4_research_start");
        assert_eq!(args["slot"], json!(1));
        assert_eq!(args["lease_token"], json!("lease"));
        assert!(args["request_id"].as_str().unwrap().starts_with("plan-rs-"));
    }

    #[test]
    fn strict_plan_stops_before_other_candidates_or_domains_after_failure() {
        for reply in [
            Err(Error::new(
                "invalid_target",
                "Technology no longer eligible",
            )),
            Ok(json!({"receipt":{"status":"NotApplied"}})),
            Ok(json!({"receipt":{"status":"Diverged"}})),
        ] {
            let reads = fake(vec![(
                "hoi4_research",
                json!({"slots":[{"researching":null}]}),
            )]);
            let tools = ActionReplies {
                reads: &reads,
                actions: RefCell::new(vec![]),
                replies: RefCell::new([reply].into()),
            };
            let result = tick(
                &mut State::default(),
                &tools,
                &json!({"stop_on_failure":true,"research_priority":["construction1","radio"],"focus_path":["f"]}),
                "lease",
                START,
            );
            assert!(result.is_err());
            assert_eq!(tools.actions.borrow().len(), 1);
            assert!(!names(&reads)
                .iter()
                .any(|n| n == "hoi4_focus_completed" || n == "hoi4_economy"));
        }
    }

    #[test]
    fn invalid_stop_policy_refuses_before_any_observation_or_action() {
        let tools = fake(vec![]);
        assert_eq!(
            tick(
                &mut State::default(),
                &tools,
                &json!({"stop_on_failure":"true"}),
                "lease",
                START
            )
            .unwrap_err()
            .code,
            "invalid_plan"
        );
        assert!(names(&tools).is_empty());
    }

    #[test]
    fn a_choice_event_waits_for_the_model_but_a_planned_option_is_taken_at_once() {
        let events = json!({"events": [{"event": "x.1", "options": ["a", "b"]}]});
        let tools = fake(vec![("hoi4_events", events)]);
        let mut state = State::default();
        let t0 = START;
        assert_eq!(
            tick(&mut state, &tools, &json!({}), "l", t0).unwrap(),
            Tick::Waiting {
                event: "x.1".into(),
                new: true
            }
        );
        let planned = json!({"event_options": {"x.1": 1}});
        let step = tick(&mut state, &tools, &planned, "l", t0).unwrap();
        assert!(matches!(&step, Tick::Acted(s) if s.starts_with("event x.1 option 1")));
        let family = json!({"event_prefix_options": {"x.": "last"}});
        let step = tick(&mut State::default(), &tools, &family, "l", t0).unwrap();
        assert!(matches!(&step, Tick::Acted(s) if s.starts_with("event x.1 option 1")));
    }

    struct Clock(RefCell<Value>);
    impl Tools for Clock {
        fn call(&self, name: &str, _: &Value) -> Result<Value> {
            match name {
                "hoi4_overview" => match &*self.0.borrow() {
                    Value::Null => Err(Error::new("unavailable", "no snapshot")),
                    value => Ok(value.clone()),
                },
                _ => Ok(json!({})),
            }
        }
    }

    #[test]
    fn game_hour_follows_the_overview_date_and_never_runs_backwards() {
        let clock = Clock(RefCell::new(json!({"date": {"raw": 1234}})));
        let mut state = State::default();
        assert_eq!(game_hour(&mut state, &clock), 1234);
        *clock.0.borrow_mut() = json!({"date": {"raw": 1200}});
        assert_eq!(game_hour(&mut state, &clock), 1234);
        *clock.0.borrow_mut() = Value::Null;
        assert_eq!(game_hour(&mut state, &clock), 1234);
        *clock.0.borrow_mut() = json!({"player": "GER"});
        assert_eq!(game_hour(&mut state, &clock), 1234);
        *clock.0.borrow_mut() = json!({"date": {"raw": 1300}});
        assert_eq!(game_hour(&mut state, &clock), 1300);
    }

    #[test]
    fn a_skip_lasts_game_days_and_does_not_depend_on_real_time() {
        let mut state = State::default();
        state.skip(START, "focus", "f", 3 * DAY);
        // many real seconds may pass, but only game time releases the skip
        assert!(state.skipped(START, "focus", "f"));
        assert!(state.skipped(START + 3 * DAY - 1, "focus", "f"));
        assert!(!state.skipped(START + 3 * DAY, "focus", "f"));
        assert!(!state.skipped(START, "focus", "g"));
    }

    #[test]
    fn a_focus_the_game_refuses_is_retried_after_ten_game_days() {
        struct Refusing;
        impl Tools for Refusing {
            fn call(&self, name: &str, _: &Value) -> Result<Value> {
                match name {
                    "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                    "hoi4_focus_completed" => Ok(json!({"active_key": null, "completed": []})),
                    "hoi4_focus_start" => Err(Error::new("invalid_target", "not eligible")),
                    _ => Ok(json!({})),
                }
            }
        }
        let plan = json!({"focus_path": ["F1"]});
        let mut state = State::default();
        let first = tick(&mut state, &Refusing, &plan, "l", START).unwrap();
        assert_eq!(first, Tick::Nothing);
        assert!(state.skipped(START, "focus", "F1"));
        // any time within ten game days: untouched
        assert_eq!(
            tick(&mut state, &Refusing, &plan, "l", START + 10 * DAY - 1).unwrap(),
            Tick::Nothing
        );
        // then it is tried again
        assert!(!state.skipped(START + 10 * DAY, "focus", "F1"));
    }

    #[test]
    fn event_patience_counts_game_days() {
        let events = json!({"events": [{"event": "x.1", "options": ["a", "b"]}]});
        let tools = fake(vec![("hoi4_events", events)]);
        let plan = json!({"auto_events": "all", "wait_events_days": 3});
        let mut state = State::default();
        assert_eq!(
            tick(&mut state, &tools, &plan, "l", START).unwrap(),
            Tick::Waiting {
                event: "x.1".into(),
                new: true
            }
        );
        // paused or slow game: no game time passed, the model keeps its decision
        assert_eq!(
            tick(&mut state, &tools, &plan, "l", START + 3 * DAY).unwrap(),
            Tick::Waiting {
                event: "x.1".into(),
                new: false
            }
        );
        let step = tick(&mut state, &tools, &plan, "l", START + 3 * DAY + 1).unwrap();
        assert!(matches!(&step, Tick::Acted(s) if s.starts_with("event x.1 option 0")));
    }

    #[test]
    fn observe_alarms_reads_only_and_ignores_failed_reads() {
        let tools = fake(vec![
            ("hoi4_focus_completed", json!({"active_key": null})),
            ("hoi4_research", json!({"slots": [{"researching": null}]})),
        ]);
        let alarms = observe_alarms(&tools, &json!({}));
        let codes: Vec<&str> = alarms.iter().map(|a| a["code"].as_str().unwrap()).collect();
        assert_eq!(codes, ["research_slot_empty", "no_active_focus"]);
        assert!(names(&tools)
            .iter()
            .all(|n| !n.contains("_start") && !n.contains("choose")));
    }

    #[test]
    fn news_and_single_option_events_are_answered_at_once() {
        let events = json!({"events": [{"event": "news.5", "options": ["a", "b"]}]});
        let tools = fake(vec![("hoi4_events", events)]);
        let mut state = State::default();
        let step = tick(&mut state, &tools, &json!({}), "l", START).unwrap();
        assert!(matches!(step, Tick::Acted(_)));
    }

    #[test]
    fn an_open_peace_conference_stops_the_runner() {
        let tools = fake(vec![(
            "hoi4_events",
            json!({"peace_conference_open": true}),
        )]);
        let mut state = State::default();
        let step = tick(&mut state, &tools, &json!({}), "l", START).unwrap();
        assert!(matches!(step, Tick::Stop(_)));
    }

    #[test]
    fn an_inactive_plan_touches_nothing() {
        let tools = fake(vec![]);
        let mut state = State::default();
        let step = tick(&mut state, &tools, &json!({"active": false}), "l", START).unwrap();
        assert_eq!(step, Tick::Idle);
        assert!(names(&tools).is_empty());
    }

    #[test]
    fn idle_military_factories_join_the_first_priority_line() {
        let tools = fake(vec![
            ("hoi4_research", json!({"slots": [{"researching": "a"}]})),
            ("hoi4_focus_completed", json!({"active_key": "f"})),
            (
                "hoi4_economy",
                json!({"factory_trigger_count": {"military": 20}, "civilian_factories_available_for_projects": 0}),
            ),
            (
                "hoi4_production_lines",
                json!({"lines": [
                    {"line_kind": "military", "entry_index": 3, "equipment_id": "infantry_equipment_2",
                     "factory_counters": {"requested_factories_raw_signed": 12}},
                    {"line_kind": "military", "entry_index": 4, "equipment_id": "support_equipment_1",
                     "factory_counters": {"requested_factories_raw_signed": 4}}]}),
            ),
        ]);
        let mut state = State::default();
        let step = tick(&mut state, &tools, &json!({}), "l", START).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("production_set infantry_equipment_2 12+4"))
        );
        let calls = tools.calls.borrow();
        assert_eq!(calls.last().unwrap().1["factories"], json!(16));
    }

    #[test]
    fn a_failed_construction_state_is_skipped_for_a_long_time() {
        struct Failing;
        impl Tools for Failing {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                match name {
                    "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                    "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                    "hoi4_economy" => Ok(json!({"factory_trigger_count": {"military": 0},
                        "civilian_factories_available_for_projects": 9})),
                    "hoi4_construction_add" => {
                        assert!(args["request_id"].is_string());
                        Err(Error::new("input_readback_failed", "no free slot"))
                    }
                    _ => Ok(json!({})),
                }
            }
        }
        let plan = json!({"construction": [{"building": "arms_factory", "states": [4, 5]}]});
        let mut state = State::default();
        let now = START;
        let first = tick(&mut state, &Failing, &plan, "l", now).unwrap();
        assert!(
            matches!(&first, Tick::Acted(s) if s.starts_with("construct arms_factory 4: false"))
        );
        let second = tick(&mut state, &Failing, &plan, "l", now).unwrap();
        assert!(matches!(&second, Tick::Acted(s) if s.starts_with("construct arms_factory 5")));
        assert_eq!(
            tick(&mut state, &Failing, &plan, "l", now).unwrap(),
            Tick::Nothing
        );
    }

    /// Idle civilian factories, no other work; construction answers by state.
    struct Builder {
        slots: HashMap<i64, u32>,
        full: Vec<i64>,
        added: RefCell<Vec<(i64, i64)>>,
        probe_fails: bool,
        own: Vec<i64>,
    }
    impl Builder {
        fn new() -> Self {
            Self {
                slots: HashMap::new(),
                full: vec![],
                added: RefCell::new(vec![]),
                probe_fails: false,
                own: vec![],
            }
        }
    }
    impl Tools for Builder {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            match name {
                "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                "hoi4_economy" => Ok(json!({"factory_trigger_count": {"military": 0},
                    "civilian_factories_available_for_projects": 20, "player": "GER"})),
                "hoi4_states" => {
                    let q = args["query"].as_str().unwrap();
                    if q == "GER" {
                        let rows: Vec<Value> = self
                            .own
                            .iter()
                            .map(|id| json!({"state_id": id, "owner": "GER", "controller": "GER"}))
                            .collect();
                        Ok(json!({"states": rows}))
                    } else if q == "Hessen" {
                        Ok(json!({"states": [{"state_id": 60, "name": "Hessen"}]}))
                    } else {
                        Ok(json!({"states": []}))
                    }
                }
                "hoi4_state_slots" => {
                    if self.probe_fails {
                        return Err(Error::new("ui_blocked", "pop-up"));
                    }
                    let ids: Vec<i64> = args["state_ids"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter_map(Value::as_i64)
                        .collect();
                    let rows: Vec<Value> = ids
                        .iter()
                        .map(|id| {
                            json!({"state_id": id, "free_slots": self.slots.get(id).copied().unwrap_or(0)})
                        })
                        .collect();
                    Ok(json!({"states": rows}))
                }
                "hoi4_construction_add" => {
                    let id = args["state_id"].as_i64().unwrap();
                    if self.full.contains(&id) {
                        return Err(Error::new("no_free_slot_or_not_allowed", "no slot"));
                    }
                    self.added
                        .borrow_mut()
                        .push((id, args["count"].as_i64().unwrap()));
                    Ok(json!({"receipt": {"status": "Verified"}}))
                }
                _ => Ok(json!({})),
            }
        }
    }
    fn acted(step: Tick) -> String {
        match step {
            Tick::Acted(s) => s,
            other => panic!("expected an action, got {other:?}"),
        }
    }

    #[test]
    fn construction_chooses_states_by_free_slots_and_respects_the_count() {
        let mut b = Builder::new();
        b.slots = [(51, 1), (55, 0), (60, 3)].into();
        let plan = json!({"construction": [{"building": "industrial_complex", "states": [51, 55, 60], "count": 3}]});
        let mut state = State::default();
        let first = acted(tick(&mut state, &b, &plan, "l", START).unwrap());
        assert!(
            first.starts_with("construct industrial_complex 60: true"),
            "{first}"
        );
        // the full state was never tried; three slots were free, three asked
        assert_eq!(*b.added.borrow(), vec![(60, 3)]);
        let digest = state.con.digest.clone().unwrap();
        assert!(
            digest.contains("idle=20") && digest.contains("queued=[industrial_complex@60x3]"),
            "{digest}"
        );
        assert!(
            !digest.contains("55:"),
            "probing marks 55 full silently: {digest}"
        );
        // the next tick yields to armies and war, the one after builds in state 51 (1 free)
        assert_eq!(
            tick(&mut state, &b, &plan, "l", START + 1).unwrap(),
            Tick::Nothing
        );
        assert!(state.con.digest.clone().unwrap().contains("yielded"));
        let third = acted(tick(&mut state, &b, &plan, "l", START + 2).unwrap());
        assert!(
            third.starts_with("construct industrial_complex 51"),
            "{third}"
        );
        assert_eq!(b.added.borrow()[1], (51, 1));
        assert!(state.con.digest.clone().unwrap().contains("55:full"));
    }

    #[test]
    fn construction_names_and_numeric_strings_are_resolved_not_dropped() {
        let mut b = Builder::new();
        b.slots = [(60, 2), (54, 1)].into();
        let plan = json!({"construction": [{"building": "arms_factory", "states": ["Hessen", "54", "Nowhere"]}]});
        let mut state = State::default();
        let step = acted(tick(&mut state, &b, &plan, "l", START).unwrap());
        assert!(step.starts_with("construct arms_factory 60"), "{step}");
        assert!(state
            .con
            .digest
            .clone()
            .unwrap()
            .contains("Nowhere:name_not_found"));
        assert_eq!(state.con.names.get("hessen"), Some(&60));
    }

    #[test]
    fn full_states_are_remembered_for_a_game_time_window_then_retried() {
        let mut b = Builder::new();
        b.full = vec![4];
        let plan = json!({"construction": [{"building": "arms_factory", "states": [4]}],
            "construction_probe_slots": false, "construction_full_days": 10});
        let mut state = State::default();
        let first = acted(tick(&mut state, &b, &plan, "l", START).unwrap());
        assert!(first.contains("construct arms_factory 4: false"), "{first}");
        // A refused batch is retried one building at a time before the state counts as full.
        let single = acted(tick(&mut state, &b, &plan, "l", START + 1).unwrap());
        assert!(
            single.contains("construct arms_factory 4: false") && single.ends_with("x1"),
            "{single}"
        );
        assert_eq!(
            tick(&mut state, &b, &plan, "l", START + 9 * DAY).unwrap(),
            Tick::Nothing
        );
        let digest = state.con.digest.clone().unwrap();
        assert!(
            digest.contains("skipped=[4:full]") && digest.contains("every listed state is full"),
            "{digest}"
        );
        b.full.clear();
        let again = acted(tick(&mut state, &b, &plan, "l", START + 10 * DAY + 1).unwrap());
        assert!(
            again.starts_with("construct arms_factory 4: true"),
            "{again}"
        );
    }

    #[test]
    fn fallback_builds_in_other_own_states_when_the_listed_ones_are_full() {
        let mut b = Builder::new();
        b.slots = [(51, 0), (70, 2)].into();
        b.own = vec![51, 70, 71];
        let plan = json!({"construction": [{"building": "industrial_complex", "states": [51]}]});
        let mut state = State::default();
        // without the opt-in nothing else is touched
        assert_eq!(
            tick(&mut state, &b, &plan, "l", START).unwrap(),
            Tick::Nothing
        );
        assert!(b.added.borrow().is_empty());
        let plan = json!({"construction": [{"building": "industrial_complex", "states": [51]}],
            "construction_fallback_any_own_state": true});
        let step = acted(tick(&mut state, &b, &plan, "l", START).unwrap());
        assert!(
            step.starts_with("construct industrial_complex 70: true"),
            "{step}"
        );
    }

    #[test]
    fn construction_waits_below_min_idle_and_names_the_reason() {
        let b = Builder::new();
        let plan = json!({"construction": [{"building": "arms_factory", "states": [4]}], "construction_min_idle": 30});
        let mut state = State::default();
        assert_eq!(
            tick(&mut state, &b, &plan, "l", START).unwrap(),
            Tick::Nothing
        );
        let digest = state.con.digest.clone().unwrap();
        assert!(
            digest.starts_with("construction: idle=20 min=30"),
            "{digest}"
        );
        assert!(b.added.borrow().is_empty());
    }

    #[test]
    fn a_short_queue_is_filled_even_with_every_factory_busy() {
        // Ethiopia run 10: one project took all 12 factories; when it finished the runner
        // queued the next one only on a later tick. A queue deeper than the pool avoids that.
        struct Busy(Builder, i64);
        impl Tools for Busy {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                if name == "hoi4_economy" {
                    return Ok(json!({"factory_trigger_count": {"military": 0},
                        "civilian_factories_available_for_projects": 12, "civilian_factories_idle": 0,
                        "construction_projects_queued": self.1, "player": "ITA"}));
                }
                self.0.call(name, args)
            }
        }
        let mut b = Builder::new();
        b.slots = [(4, 5)].into();
        let plan = json!({"construction": [{"building": "industrial_complex", "states": [4]}]});
        let short = Busy(b, 1);
        let mut state = State::default();
        let step = acted(tick(&mut state, &short, &plan, "l", START).unwrap());
        assert!(
            step.starts_with("construct industrial_complex 4: true"),
            "{step}"
        );
        assert_eq!(*short.0.added.borrow(), vec![(4, 1)]);
        let mut b = Builder::new();
        b.slots = [(4, 5)].into();
        let full = Busy(b, 3);
        let mut state = State::default();
        assert_eq!(
            tick(&mut state, &full, &plan, "l", START).unwrap(),
            Tick::Nothing
        );
        assert!(state.con.digest.clone().unwrap().contains("queue is full"));
    }

    #[test]
    fn a_failing_slot_read_is_not_fatal_and_a_blocked_add_pauses_every_state() {
        let mut b = Builder::new();
        b.probe_fails = true;
        let plan = json!({"construction": [{"building": "arms_factory", "states": [4, 5]}]});
        let mut state = State::default();
        let step = acted(tick(&mut state, &b, &plan, "l", START).unwrap());
        assert!(step.starts_with("construct arms_factory 4: true"), "{step}");
        assert!(state
            .con
            .digest
            .clone()
            .unwrap()
            .contains("probe:ui_blocked"));
        // an event or pop-up refusing input is not the state's fault
        struct Blocked;
        impl Tools for Blocked {
            fn call(&self, name: &str, _: &Value) -> Result<Value> {
                match name {
                    "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                    "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                    "hoi4_economy" => Ok(json!({"civilian_factories_available_for_projects": 9})),
                    "hoi4_construction_add" => Err(Error::new("ui_blocked", "event open")),
                    _ => Ok(json!({})),
                }
            }
        }
        let plan = json!({"construction": [{"building": "arms_factory", "states": [4, 5]}], "construction_probe_slots": false});
        let mut state = State::default();
        acted(tick(&mut state, &Blocked, &plan, "l", START).unwrap());
        assert_eq!(
            tick(&mut state, &Blocked, &plan, "l", START + 2).unwrap(),
            Tick::Nothing
        );
        assert!(state.con.digest.clone().unwrap().contains("paused"));
        // no state was marked failed or full: after the pause both are tried again
        assert!(!state.skipped(START + 7, "confull", "slots:4"));
        assert!(matches!(
            tick(&mut state, &Blocked, &plan, "l", START + 7).unwrap(),
            Tick::Acted(_)
        ));
    }

    #[test]
    fn construction_runs_before_armies_that_always_have_something_to_retry() {
        struct Both(Builder);
        impl Tools for Both {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                if name == "hoi4_armies" {
                    return Ok(
                        json!({"unassigned_by_state": [{"state_id": 56, "divisions": 4}], "armies": []}),
                    );
                }
                if name == "hoi4_army_create" {
                    return Ok(json!({"receipt": {"status": "Verified"}}));
                }
                self.0.call(name, args)
            }
        }
        let mut inner = Builder::new();
        inner.slots = [(4, 5)].into();
        let tools = Both(inner);
        let plan = json!({"construction": [{"building": "arms_factory", "states": [4]}], "armies": {"max": 24}});
        let mut state = State::default();
        let first = acted(tick(&mut state, &tools, &plan, "l", START).unwrap());
        assert!(first.starts_with("construct arms_factory 4"), "{first}");
        let second = acted(tick(&mut state, &tools, &plan, "l", START + 1).unwrap());
        assert!(second.starts_with("army_create state 56"), "{second}");
    }

    #[test]
    fn a_stalled_construction_raises_an_alarm_after_the_configured_days() {
        let b = Builder::new();
        let mut state = State {
            last_hour: START + 6 * DAY,
            ..State::default()
        };
        state.con.baseline = Some(START);
        let plan = json!({"construction": [{"building": "arms_factory", "states": [4]}]});
        let alarm =
            construction_alarm(&state, &b, &plan, Some("construction: idle=20 [4:full]")).unwrap();
        assert_eq!(alarm["code"], json!("construction_stalled"));
        assert!(alarm["message"].as_str().unwrap().contains("4:full"));
        state.con.last_queue = Some(START + 5 * DAY);
        assert!(construction_alarm(&state, &b, &plan, None).is_none());
        let off = json!({"construction": [{"building": "x", "states": [1]}], "alarms": {"construction_stalled": false}});
        state.con.last_queue = None;
        assert!(construction_alarm(&state, &b, &off, None).is_none());
    }

    #[test]
    fn unassigned_divisions_get_an_army_and_big_armies_are_split() {
        struct Roster;
        impl Tools for Roster {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                match name {
                    "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                    "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                    "hoi4_economy" => Ok(json!({"factory_trigger_count": {"military": 0},
                        "civilian_factories_available_for_projects": 0})),
                    "hoi4_armies" => Ok(json!({
                        "unassigned_by_state": [{"state_id": 66, "divisions": 3}],
                        "armies": [{"army": "Army 1", "division_count": 40, "commander": null}]})),
                    "hoi4_army_create" => {
                        if args.get("absorb_existing").is_some() {
                            Ok(json!({"receipt": {"status": "Verified"}}))
                        } else {
                            Err(Error::new(
                                "invalid_target",
                                "Divisions of an existing army stand here",
                            ))
                        }
                    }
                    "hoi4_army_assign" => {
                        Err(Error::new("capability_unavailable", "control not mapped"))
                    }
                    "hoi4_army_split" => {
                        assert_eq!(args["count"], json!(16));
                        Ok(json!({"receipt": {"status": "Verified"}}))
                    }
                    _ => Ok(json!({})),
                }
            }
        }
        let plan = json!({"armies": {"max": 24, "absorb": true}});
        let mut state = State::default();
        let now = START;
        // the assign control is unmapped: the older create/merge path still works
        let first = tick(&mut state, &Roster, &plan, "l", now).unwrap();
        assert!(matches!(&first, Tick::Acted(s) if s.starts_with("army_create state 66: true")));
        // the state is retried later; the oversized army is split next
        state.skip(now, "armycreate", "66", 2 * DAY);
        let second = tick(&mut state, &Roster, &plan, "l", now).unwrap();
        assert!(matches!(&second, Tick::Acted(s) if s.starts_with("army_split Army 1 16: true")));
    }

    #[test]
    fn unassigned_divisions_are_assigned_to_the_standing_army_before_any_merge() {
        struct Assigning(RefCell<Vec<String>>, bool);
        impl Tools for Assigning {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                self.0.borrow_mut().push(name.to_string());
                match name {
                    "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                    "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                    "hoi4_economy" => Ok(json!({"civilian_factories_available_for_projects": 0})),
                    "hoi4_armies" => Ok(json!({
                        "unassigned_by_state": [{"state_id": 56, "divisions": 4}], "armies": []})),
                    "hoi4_army_assign" if self.1 => {
                        assert_eq!(args["state_id"], json!(56));
                        Ok(json!({"receipt": {"status": "Verified"}}))
                    }
                    "hoi4_army_assign" => {
                        Err(Error::new("capability_unavailable", "control not mapped"))
                    }
                    _ => Ok(json!({})),
                }
            }
        }
        let plan = json!({"armies": {"absorb": true}});
        let tools = Assigning(RefCell::new(vec![]), true);
        let step = acted(tick(&mut State::default(), &tools, &plan, "l", START).unwrap());
        assert!(step.starts_with("army_assign state 56: true"), "{step}");
        assert!(!tools.0.borrow().contains(&"hoi4_army_create".to_string()));
        // "assign" never merges: an unmapped control just waits and reports
        let plan = json!({"armies": {"absorb": "assign"}});
        let tools = Assigning(RefCell::new(vec![]), false);
        let mut state = State::default();
        let step = acted(tick(&mut state, &tools, &plan, "l", START).unwrap());
        assert!(
            step.starts_with("army_assign state 56: false capability_unavailable"),
            "{step}"
        );
        assert!(!tools.0.borrow().contains(&"hoi4_army_create".to_string()));
        assert!(state.skipped(START + DAY, "armycreate", "56"));
    }

    #[test]
    fn only_real_armies_get_a_general() {
        struct Roster;
        impl Tools for Roster {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                match name {
                    "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                    "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                    "hoi4_economy" => Ok(json!({"factory_trigger_count": {"military": 0},
                        "civilian_factories_available_for_projects": 0})),
                    "hoi4_armies" => Ok(json!({"unassigned_by_state": [], "armies": [
                        {"army": "Army 2", "division_count": 3, "commander": null},
                        {"army": "Army 3", "division_count": 12, "commander": null}]})),
                    "hoi4_army_commander" => {
                        assert_eq!(args["army"], json!("Army 3"));
                        assert_eq!(args["general"], json!("B"));
                        Ok(json!({"receipt": {"status": "Verified"}}))
                    }
                    _ => Ok(json!({})),
                }
            }
        }
        let plan = json!({"armies": {"generals": ["B", "C"], "consolidate": false}});
        let mut state = State::default();
        let step = tick(&mut state, &Roster, &plan, "l", START).unwrap();
        assert!(matches!(&step, Tick::Acted(s) if s.starts_with("general B -> Army 3: true")));
    }

    struct Extras(std::cell::RefCell<Vec<(String, Value)>>);
    impl Tools for Extras {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.0.borrow_mut().push((name.to_string(), args.clone()));
            match name {
                "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                "hoi4_economy" => Ok(json!({"factory_trigger_count": {"military": 0},
                    "civilian_factories_available_for_projects": 0})),
                "hoi4_special_projects" => Ok(json!({
                    "facilities": [{"facility_id": 7, "project": null, "scientist": null, "specialization": "land"}],
                    "projects": [{"key": "sp_land_a", "specialization": "land", "running_in_facility_id": null},
                                 {"key": "sp_land_b", "specialization": "land", "running_in_facility_id": null},
                                 {"key": "sp_air_x", "specialization": "air", "running_in_facility_id": null}],
                    "scientists": [{"name": "Air Person", "specialization": "air", "assigned_facility_id": null, "skills": {"land": 0}},
                                   {"name": "Land Person", "specialization": "land", "assigned_facility_id": null, "skills": {"land": 2}}]})),
                "hoi4_wars" => Ok(json!({"borders": [
                    {"neighbour_tag": "DEN", "hostile": false, "own_border_states": [5]},
                    {"neighbour_tag": "POL", "hostile": true, "own_border_states": [64, 65]}]})),
                "hoi4_air" => Ok(json!({"wings": [
                    {"wing": "KG 1", "aircraft_type": "tac_bomber_equipment_1", "standing_by": true},
                    {"wing": "JG 1", "aircraft_type": "fighter_equipment_1", "standing_by": true}]})),
                _ => Ok(json!({"receipt": {"status": "Verified"}})),
            }
        }
    }

    #[test]
    fn an_empty_facility_gets_a_matching_scientist_and_the_priority_project() {
        let tools = Extras(Default::default());
        let plan = json!({"special_projects": {"priority": ["sp_land_b"]}});
        let mut state = State::default();
        let step = acted(tick(&mut state, &tools, &plan, "l", START).unwrap());
        assert!(
            step.starts_with("special project sp_land_b in facility 7 by Land Person: true"),
            "{step}"
        );
    }

    #[test]
    fn in_war_standing_by_fighters_fly_air_superiority_over_the_enemy_border() {
        let tools = Extras(Default::default());
        let plan = json!({"air": {"war_missions": true}});
        let mut state = State::default();
        let step = acted(tick(&mut state, &tools, &plan, "l", START).unwrap());
        assert!(
            step.starts_with("air JG 1 air_superiority over state 64: true"),
            "{step}"
        );
    }

    #[test]
    fn a_dated_production_line_waits_for_its_date() {
        let line =
            json!({"equipment": "medium_tank_equipment_1", "factories": 4, "from": "1938.06.01"});
        let early = crate::alarms::stop_on_date(&json!({"stop_on_date": "1938.05.31"}))
            .unwrap()
            .unwrap()
            .0;
        let late = crate::alarms::stop_on_date(&json!({"stop_on_date": "1938.06.02"}))
            .unwrap()
            .unwrap()
            .0;
        assert!(!reached(&line, early));
        assert!(reached(&line, late));
        assert!(reached(&json!({"equipment": "x"}), early));
    }

    #[test]
    fn small_armies_merge_into_the_fullest_army_that_stays_under_the_cap() {
        struct Roster;
        impl Tools for Roster {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                match name {
                    "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                    "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                    "hoi4_economy" => Ok(json!({"factory_trigger_count": {"military": 0},
                        "civilian_factories_available_for_projects": 0})),
                    "hoi4_armies" => Ok(json!({"unassigned_by_state": [], "armies": [
                        {"army": "Army 1", "division_count": 2, "commander": null},
                        {"army": "Army 2", "division_count": 23, "commander": "A"},
                        {"army": "Army 3", "division_count": 12, "commander": "B"},
                        {"army": "Army 4", "division_count": 1, "commander": null, "battle_plan_orders": 2}]})),
                    "hoi4_army_merge" => {
                        assert_eq!(args["source"], json!("Army 1"));
                        assert_eq!(args["target"], json!("Army 3"));
                        Ok(json!({"receipt": {"status": "Verified"}}))
                    }
                    _ => Ok(json!({})),
                }
            }
        }
        let plan = json!({"armies": {"generals": ["C"]}});
        let mut state = State::default();
        let step = tick(&mut state, &Roster, &plan, "l", START).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("army_merge Army 1 -> Army 3: true")),
            "{step:?}"
        );
    }

    struct War {
        liberate: bool,
        front_ok: bool,
        calls: RefCell<Vec<(String, Value)>>,
    }
    impl Tools for War {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.calls
                .borrow_mut()
                .push((name.to_string(), args.clone()));
            match name {
                "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                "hoi4_economy" => Ok(json!({"factory_trigger_count": {"military": 0},
                    "civilian_factories_available_for_projects": 0})),
                "hoi4_overview" => Ok(json!({"player": "GER"})),
                "hoi4_states" => match args["query"].as_str().unwrap() {
                    "GER" => Ok(json!({"states": [
                        {"state_id": 42, "owner": "GER", "controller": "GER", "x": 0.0, "z": 0.0},
                        {"state_id": 5, "owner": "GER", "controller": if self.liberate { "POL" } else { "GER" },
                         "x": 100.0, "z": 0.0}]})),
                    "FRA" => Ok(json!({"states": [
                        {"state_id": 28, "owner": "FRA", "controller": "FRA", "x": if self.liberate { 300.0 } else { 10.0 }, "z": 10.0},
                        {"state_id": 18, "owner": "FRA", "controller": "FRA", "x": 50.0, "z": 50.0}]})),
                    "POL" => Ok(json!({"states": []})),
                    other => panic!("unexpected query {other}"),
                },
                "hoi4_armies" => Ok(json!({"unassigned_by_state": [], "armies": [
                    {"army": "Army 1", "division_count": 10, "main_state": 42,
                     "battle_plan_orders": 0, "plan_executing": false, "commander": "X"}]})),
                "hoi4_army_front" if !self.front_ok => Err(Error::new(
                    "input_readback_failed",
                    "No front line was placed",
                )),
                _ if args.get("request_id").is_some() => {
                    Ok(json!({"receipt": {"status": "Verified"}}))
                }
                _ => Ok(json!({})),
            }
        }
    }
    fn war_plan() -> Value {
        json!({"war": {"enemies": ["FRA"], "cooldown": 40}})
    }
    fn last_action(w: &War) -> (String, Value) {
        w.calls
            .borrow()
            .iter()
            .rev()
            .find(|(n, _)| n.starts_with("hoi4_army_"))
            .cloned()
            .unwrap()
    }

    #[test]
    fn a_supply_line_behind_factories_is_raised_first() {
        let ic =
            json!({"line_kind": "building", "building": {"building_id": "industrial_complex"}});
        let hub = json!({"line_kind": "building", "building": {"building_id": "supply_node"}});
        let rail = json!({"line_kind": "railway", "building": null});
        assert_eq!(
            super::supply_line_to_raise(&[ic.clone(), ic.clone(), hub.clone(), rail.clone()]),
            Some(2)
        );
        assert_eq!(
            super::supply_line_to_raise(&[hub.clone(), rail.clone(), ic.clone()]),
            None
        );
        assert_eq!(
            super::supply_line_to_raise(&[hub.clone(), ic.clone(), rail.clone()]),
            Some(2)
        );
        assert_eq!(super::supply_line_to_raise(std::slice::from_ref(&ic)), None);
    }

    #[test]
    fn direct_moves_attack_with_a_move_instead_of_a_front() {
        let w = War {
            liberate: false,
            front_ok: true,
            calls: RefCell::new(vec![]),
        };
        let mut state = State::default();
        let plan = json!({"war": {"enemies": ["FRA"], "direct_moves": true}});
        tick(&mut state, &w, &plan, "l", START).unwrap();
        let (name, args) = last_action(&w);
        assert_eq!(name, "hoi4_army_move", "{:?}", w.calls.borrow());
        assert!(args["state_id"] == 28 || args["state_id"] == 18, "{args}");
        assert!(!w.calls.borrow().iter().any(|(n, _)| n == "hoi4_army_front"));
    }

    /// An army standing in an own state an enemy holds part of.
    struct Inside(RefCell<Vec<(String, Value)>>);
    impl Tools for Inside {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.0.borrow_mut().push((name.to_string(), args.clone()));
            match name {
                "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                "hoi4_economy" => Ok(json!({"factory_trigger_count": {"military": 0},
                    "civilian_factories_available_for_projects": 0})),
                "hoi4_overview" => Ok(json!({"player": "GER"})),
                "hoi4_wars" => Ok(json!({"wars": {"validated": true, "enemies": ["POL"]}})),
                "hoi4_states" => match args["query"].as_str().unwrap() {
                    "GER" => Ok(
                        json!({"states": [{"state_id": 42, "owner": "GER", "controller": "GER",
                        "x": 0.0, "z": 0.0,
                        "provinces_held_by_others": [{"province": 7, "controller": "POL"}]}]}),
                    ),
                    "POL" => Ok(json!({"states": []})),
                    "42" => Ok(json!({"states": [{"state_id": 42, "provinces": [
                        {"province": 6, "controller": "GER", "x": 0.0, "z": 0.0},
                        {"province": 8, "controller": "POL", "x": 50.0, "z": 0.0},
                        {"province": 7, "controller": "POL", "x": 5.0, "z": 0.0}]}]})),
                    other => panic!("unexpected query {other}"),
                },
                "hoi4_armies" => Ok(json!({"unassigned_by_state": [], "armies": [
                    {"army": "Army 1", "division_count": 10, "main_state": 42,
                     "battle_plan_orders": 0, "plan_executing": false, "commander": "X"}]})),
                _ if args.get("request_id").is_some() => {
                    Ok(json!({"receipt": {"status": "Verified"}}))
                }
                _ => Ok(json!({})),
            }
        }
    }
    #[test]
    fn an_army_inside_a_partly_taken_own_state_retakes_the_nearest_province_once() {
        let w = Inside(RefCell::new(vec![]));
        let mut state = State::default();
        let plan = json!({"war": {"enemies": ["POL"], "cooldown_hours": 1}});
        let moves = |w: &Inside| -> Vec<Value> {
            w.0.borrow()
                .iter()
                .filter(|(n, _)| n == "hoi4_army_move")
                .map(|(_, a)| a.clone())
                .collect()
        };
        tick(&mut state, &w, &plan, "l", START).unwrap();
        let m = moves(&w);
        assert_eq!(m.len(), 1, "{:?}", w.0.borrow());
        assert_eq!(m[0]["province_id"], 7);
        assert_eq!(m[0]["state_id"], 42);
        // a few hours later, still on the way: no second order
        tick(&mut state, &w, &plan, "l", START + 6).unwrap();
        assert_eq!(moves(&w).len(), 1);
    }

    #[test]
    fn a_victory_point_province_comes_before_a_nearer_one() {
        struct Vp(RefCell<Vec<(String, Value)>>);
        impl Tools for Vp {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                self.0.borrow_mut().push((name.to_string(), args.clone()));
                if name == "hoi4_states" && args["query"] == "42" {
                    return Ok(json!({"states": [{"state_id": 42, "provinces": [
                        {"province": 7, "controller": "POL", "x": 5.0, "z": 0.0},
                        {"province": 8, "controller": "POL", "x": 50.0, "z": 0.0, "victory_points": 5.0}]}]}));
                }
                Inside(RefCell::new(vec![])).call(name, args)
            }
        }
        let w = Vp(RefCell::new(vec![]));
        let mut state = State::default();
        let plan = json!({"war": {"enemies": ["POL"], "cooldown_hours": 1}});
        tick(&mut state, &w, &plan, "l", START).unwrap();
        let moves: Vec<Value> =
            w.0.borrow()
                .iter()
                .filter(|(n, _)| n == "hoi4_army_move")
                .map(|(_, a)| a.clone())
                .collect();
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0]["province_id"], 8);
    }

    #[test]
    fn an_army_in_a_forming_pocket_falls_back_once() {
        struct Pocket(RefCell<Vec<(String, Value)>>);
        impl Tools for Pocket {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                self.0.borrow_mut().push((name.to_string(), args.clone()));
                if name == "hoi4_armies" {
                    return Ok(json!({"unassigned_by_state": [], "armies": [
                        {"army": "Army 1", "division_count": 10, "main_state": 28,
                         "battle_plan_orders": 0, "plan_executing": false, "commander": "X",
                         "threat": {"share": 0.8, "encirclement_risk": true}}]}));
                }
                War {
                    liberate: false,
                    front_ok: true,
                    calls: RefCell::new(vec![]),
                }
                .call(name, args)
            }
        }
        let w = Pocket(RefCell::new(vec![]));
        let mut state = State::default();
        let step = tick(&mut state, &w, &war_plan(), "l", START).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("war pocket Army 1")),
            "{step:?}"
        );
        let moves = |w: &Pocket| {
            w.0.borrow()
                .iter()
                .filter(|(n, _)| n == "hoi4_army_move")
                .count()
        };
        assert_eq!(moves(&w), 1);
        tick(&mut state, &w, &war_plan(), "l", START + 6).unwrap();
        assert_eq!(moves(&w), 1, "no second fall-back order within 3 days");
    }

    #[test]
    fn the_same_failing_order_is_not_sent_a_fourth_time_within_ten_days() {
        struct Refuse(RefCell<usize>);
        impl Tools for Refuse {
            fn call(&self, _: &str, _: &Value) -> Result<Value> {
                *self.0.borrow_mut() += 1;
                Ok(json!({"receipt": {"status": "NotApplied"}}))
            }
        }
        let tools = Refuse(RefCell::new(0));
        let mut state = State {
            clock: START,
            ..State::default()
        };
        for _ in 0..5 {
            let (ok, _) = state
                .act(
                    &tools,
                    "l",
                    "hoi4_army_move",
                    json!({"army": "Army 1", "state_id": 5}),
                    "t",
                )
                .unwrap();
            assert!(!ok);
        }
        assert_eq!(*tools.0.borrow(), 3);
        // another order still goes out, and the first one again after the window
        state
            .act(
                &tools,
                "l",
                "hoi4_army_move",
                json!({"army": "Army 1", "state_id": 6}),
                "t",
            )
            .unwrap();
        assert_eq!(*tools.0.borrow(), 4);
        state.clock = START + 11 * DAY;
        state
            .act(
                &tools,
                "l",
                "hoi4_army_move",
                json!({"army": "Army 1", "state_id": 5}),
                "t",
            )
            .unwrap();
        assert_eq!(*tools.0.borrow(), 5);
    }

    #[test]
    fn dates_map_to_raw_game_hours() {
        // 1936.01.01.12 read live as 60759371 (hour 11 of the day, counted from 0)
        assert_eq!(super::date_hour("1936.01.01"), Some(60759360));
        assert_eq!(
            super::date_hour("1936-03-05"),
            Some(60759360 + (31 + 28 + 4) * 24)
        );
        assert_eq!(super::date_hour("1936.13.01"), None);
        assert_eq!(super::game_year(60759371), Some(1936));
    }

    #[test]
    fn construction_plan_items_wait_for_their_trigger_and_go_once() {
        struct Plan(RefCell<Vec<(String, Value)>>, RefCell<&'static str>);
        impl Tools for Plan {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                self.0.borrow_mut().push((name.to_string(), args.clone()));
                Ok(match name {
                    "hoi4_overview" => json!({"player": "ITA"}),
                    "hoi4_states" => json!({"states": [{"state_id": 842, "owner": "ETH",
                        "provinces": [{"province": 13238, "controller": *self.1.borrow()},
                                      {"province": 2072, "controller": "ETH"}]}]}),
                    _ if args.get("request_id").is_some() => {
                        json!({"receipt": {"status": "Verified"}})
                    }
                    _ => json!({}),
                })
            }
        }
        let plan = json!({"construction_plan": [
            {"order": 3, "building": "rail_way", "from_state": 550, "to_state": 842,
             "path": [13250, 13238], "when": "province_controlled:13238"},
            {"order": 5, "building": "supply_node", "state": 842, "province": 2072,
             "when": "state_fully_controlled:842"}]});
        let tools = Plan(RefCell::new(vec![]), RefCell::new("ETH"));
        let mut state = State::default();
        let built = |t: &Plan| {
            t.0.borrow()
                .iter()
                .filter(|(n, _)| n == "hoi4_railway_build" || n == "hoi4_construction_add")
                .count()
        };
        assert_eq!(
            construction_plan_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        *tools.1.borrow_mut() = "ITA";
        let step = construction_plan_step(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(step.unwrap().starts_with("construction plan #3"));
        let rail = tools
            .0
            .borrow()
            .iter()
            .find(|(n, _)| n == "hoi4_railway_build")
            .cloned()
            .unwrap();
        assert_eq!(rail.1["from_province"], 13250);
        assert_eq!(rail.1["to_province"], 13238);
        // #3 done, #5 waits for all of 842
        assert_eq!(
            construction_plan_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
        assert_eq!(built(&tools), 1);
    }

    #[test]
    fn vp_targets_send_the_army_to_the_first_province_not_yet_held() {
        struct Vp(RefCell<Vec<(String, Value)>>);
        impl Tools for Vp {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                self.0.borrow_mut().push((name.to_string(), args.clone()));
                if name == "hoi4_states" && args["query"] == "28" {
                    return Ok(json!({"states": [{"state_id": 28, "provinces": [
                        {"province": 70, "controller": "GER"}, {"province": 71, "controller": "FRA"}]}]}));
                }
                War {
                    liberate: false,
                    front_ok: true,
                    calls: RefCell::new(vec![]),
                }
                .call(name, args)
            }
        }
        let w = Vp(RefCell::new(vec![]));
        let mut state = State::default();
        let plan = json!({"war": {"enemies": ["FRA"], "direct_moves": true},
            "vp_targets": [{"province": 70, "state": 28, "army": "strike (Army 1)"},
                           {"province": 71, "state": 28, "army": "strike (Army 1)"}]});
        let step = tick(&mut state, &w, &plan, "l", START).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("war vp target Army 1 -> province 71")),
            "{step:?}"
        );
    }

    #[test]
    fn army_layout_takes_the_strike_army_and_detaches_a_guard_by_template() {
        struct L(RefCell<Vec<(String, Value)>>, RefCell<bool>);
        impl Tools for L {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                self.0.borrow_mut().push((name.to_string(), args.clone()));
                let split = *self.1.borrow();
                Ok(match name {
                    "hoi4_armies" => {
                        let mut a =
                            vec![json!({"army": "Army 1", "main_state": 550, "commander": null})];
                        if split {
                            a.push(json!({"army": "Army 2", "main_state": 550, "commander": null}));
                        }
                        json!({"armies": a})
                    }
                    "hoi4_supply" => json!({"divisions": [
                        {"army": "Army 1", "id": "d1", "template": "Divisione di Fanteria"},
                        {"army": "Army 1", "id": "d2", "template": "Camicie Nere"},
                        {"army": "Army 1", "id": "d3", "template": "Camicie Nere"},
                        {"army": "Army 1", "id": "d4", "template": "Camicie Nere"}]}),
                    "hoi4_army_detach" => {
                        *self.1.borrow_mut() = true;
                        json!({"receipt": {"status": "Verified"}})
                    }
                    _ if args.get("request_id").is_some() => {
                        json!({"receipt": {"status": "Verified"}})
                    }
                    _ => json!({}),
                })
            }
        }
        let plan = json!({"army_layout": [
            {"name_hint": "North strike (Army 1)", "state": 550, "role": "strike", "general": "Giovanni Messe"},
            {"name_hint": "North guard", "state": 550, "role": "guard", "templates": ["Camicie Nere"],
             "count": 2, "guard_states_in_order": [550]}]});
        let tools = L(RefCell::new(vec![]), RefCell::new(false));
        let mut state = State::default();
        let s1 = layout_step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert_eq!(s1, "layout: Army 1 is North strike (Army 1)");
        let s2 = layout_step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(s2.contains("-> Army 2 (North guard)"), "{s2}");
        let detach = tools
            .0
            .borrow()
            .iter()
            .find(|(n, _)| n == "hoi4_army_detach")
            .cloned()
            .unwrap();
        assert_eq!(detach.1["divisions"], json!(["d2", "d3"]));
        assert_eq!(state.war_guard.get("Army 2"), Some(&550));
        let s3 = layout_step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(
            s3.starts_with("layout general Giovanni Messe -> Army 1"),
            "{s3}"
        );
        assert_eq!(
            layout_step(&mut state, &tools, &plan, "l", START).unwrap(),
            None
        );
    }

    #[test]
    fn army_layout_creates_the_army_from_loose_divisions_first() {
        struct C(RefCell<bool>);
        impl Tools for C {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                let made = *self.0.borrow();
                Ok(match name {
                    "hoi4_armies" if made => json!({"armies": [
                        {"army": "Army 1", "main_state": 550, "commander": null}],
                        "unassigned_by_state": []}),
                    "hoi4_armies" => json!({"armies": [], "unassigned_by_state": [
                        {"state_id": 550, "divisions": 12}, {"state_id": 158, "divisions": 10}]}),
                    "hoi4_army_create" => {
                        assert_eq!(args["state_id"], 550);
                        *self.0.borrow_mut() = true;
                        json!({"receipt": {"status": "Verified"}})
                    }
                    _ => json!({}),
                })
            }
        }
        let plan = json!({"army_layout": [
            {"name_hint": "North strike (Army 1)", "state": 550, "role": "strike"}]});
        let tools = C(RefCell::new(false));
        let mut state = State::default();
        let s1 = layout_step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert!(s1.starts_with("layout: army created in 550"), "{s1}");
        let s2 = layout_step(&mut state, &tools, &plan, "l", START)
            .unwrap()
            .unwrap();
        assert_eq!(s2, "layout: Army 1 is North strike (Army 1)");
    }

    /// The war fake with a validated war list at peace.
    struct AtPeace(War);
    impl Tools for AtPeace {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            if name == "hoi4_wars" {
                return Ok(json!({"wars": {"validated": true, "at_war": false, "enemies": []}}));
            }
            self.0.call(name, args)
        }
    }
    #[test]
    fn the_plans_enemies_are_left_alone_at_peace() {
        let w = AtPeace(War {
            liberate: false,
            front_ok: true,
            calls: RefCell::new(vec![]),
        });
        let mut state = State::default();
        tick(&mut state, &w, &war_plan(), "l", START).unwrap();
        assert!(
            !w.0.calls
                .borrow()
                .iter()
                .any(|(n, _)| n.starts_with("hoi4_army_")),
            "{:?}",
            w.0.calls.borrow()
        );
    }

    #[test]
    fn a_guard_army_moves_to_its_post_and_never_attacks() {
        let w = War {
            liberate: false,
            front_ok: true,
            calls: RefCell::new(vec![]),
        };
        let mut state = State::default();
        let plan = json!({"war": {"enemies": ["FRA"], "guards": {"Army 1": 5}}});
        let step = tick(&mut state, &w, &plan, "l", START).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("war guard move Army 1 -> state 5")),
            "{step:?}"
        );
        assert!(!w
            .calls
            .borrow()
            .iter()
            .any(|(n, _)| n == "hoi4_army_offensive"));
    }

    #[test]
    fn the_army_standing_in_a_guard_state_holds_a_front_there() {
        let w = War {
            liberate: false,
            front_ok: true,
            calls: RefCell::new(vec![]),
        };
        let mut state = State::default();
        let plan = json!({"war": {"enemies": ["FRA"], "guard_states": [42]}});
        let step = tick(&mut state, &w, &plan, "l", START).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("war guard front Army 1 -> state 42")),
            "{step:?}"
        );
        let (_, args) = last_action(&w);
        assert_eq!(
            args["state_id"],
            json!(28),
            "the front faces the nearest enemy state"
        );
        assert_eq!(state.war_guard.get("Army 1"), Some(&42));
    }

    #[test]
    fn a_liberation_move_is_not_repeated_while_the_army_travels() {
        let w = War {
            liberate: true,
            front_ok: true,
            calls: RefCell::new(vec![]),
        };
        let mut state = State::default();
        state.war_assign.insert("Army 1".into(), 28);
        state.war_stage.insert("Army 1".into(), 3);
        let plan = json!({"war": {"enemies": ["FRA"], "axes": [[28]], "stall_days": 4, "cooldown_hours": 1}});
        let moves = |w: &War| {
            w.calls
                .borrow()
                .iter()
                .filter(|(n, _)| n == "hoi4_army_move")
                .count()
        };
        tick(&mut state, &w, &plan, "l", START).unwrap();
        assert_eq!(moves(&w), 1);
        // a day later, still on the way: no second order
        for h in 1..24 {
            tick(&mut state, &w, &plan, "l", START + h * DAY / 24 * 2).unwrap();
        }
        assert_eq!(moves(&w), 1);
    }

    #[test]
    fn an_occupied_own_state_takes_the_nearest_army_off_its_axis() {
        let w = War {
            liberate: true,
            front_ok: true,
            calls: RefCell::new(vec![]),
        };
        let mut state = State::default();
        state.war_assign.insert("Army 1".into(), 28);
        state.war_stage.insert("Army 1".into(), 3);
        let plan = json!({"war": {"enemies": ["FRA"], "axes": [[28]]}});
        let step = tick(&mut state, &w, &plan, "l", START).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("war move Army 1 -> state 5")),
            "{step:?}"
        );
    }

    #[test]
    fn a_lost_state_earlier_on_the_axis_comes_before_the_assigned_target() {
        let w = War {
            liberate: false,
            front_ok: true,
            calls: RefCell::new(vec![]),
        };
        let mut state = State::default();
        state.war_assign.insert("Army 1".into(), 18);
        state.war_stage.insert("Army 1".into(), 3);
        let plan = json!({"war": {"enemies": ["FRA"], "axes": [[28, 18]]}});
        let step = tick(&mut state, &w, &plan, "l", START).unwrap();
        assert_eq!(state.war_assign.get("Army 1"), Some(&28), "{step:?}");
    }

    #[test]
    fn repeated_stall_moves_back_off_while_the_army_stays_put() {
        let w = War {
            liberate: false,
            front_ok: true,
            calls: RefCell::new(vec![]),
        };
        let mut state = State::default();
        state.war_assign.insert("Army 1".into(), 28);
        state.war_stage.insert("Army 1".into(), 3);
        let plan = json!({"war": {"enemies": ["FRA"], "stall_days": 4, "cooldown_hours": 1}});
        let moves = |w: &War| {
            w.calls
                .borrow()
                .iter()
                .filter(|(n, _)| n == "hoi4_army_move")
                .count()
        };
        // first move after 4 days of no progress
        tick(&mut state, &w, &plan, "l", START).unwrap();
        tick(&mut state, &w, &plan, "l", START + 4 * DAY).unwrap();
        assert_eq!(moves(&w), 1);
        // the army did not move: the next one waits 8 days, not 4
        tick(&mut state, &w, &plan, "l", START + 8 * DAY + 1).unwrap();
        assert_eq!(moves(&w), 1);
        tick(&mut state, &w, &plan, "l", START + 12 * DAY + 1).unwrap();
        assert_eq!(moves(&w), 2);
    }

    #[test]
    fn the_war_acts_before_research_when_it_runs_first() {
        let w = War {
            liberate: false,
            front_ok: true,
            calls: RefCell::new(vec![]),
        };
        let mut state = State::default();
        tick(&mut state, &w, &war_plan(), "l", START).unwrap();
        assert!(!w.calls.borrow().iter().any(|(n, _)| n == "hoi4_research"));
    }

    #[test]
    fn an_army_fronts_the_nearest_enemy_state_then_draws_the_offensive_and_executes() {
        let w = War {
            liberate: false,
            front_ok: true,
            calls: RefCell::new(vec![]),
        };
        let mut state = State::default();
        let now = START;
        let first = tick(&mut state, &w, &war_plan(), "l", now).unwrap();
        assert!(matches!(&first, Tick::Acted(s) if s.starts_with("war front Army 1 -> state 28")));
        let (name, args) = last_action(&w);
        assert_eq!(
            (name.as_str(), &args["state_id"]),
            ("hoi4_army_front", &json!(28))
        );
        let later = now + 3;
        let second = tick(&mut state, &w, &war_plan(), "l", later).unwrap();
        assert!(matches!(&second, Tick::Acted(s) if s.starts_with("war offensive")));
        let third = tick(&mut state, &w, &war_plan(), "l", later + 3).unwrap();
        assert!(matches!(&third, Tick::Acted(s) if s.starts_with("war execute")));
    }

    /// A war with a configurable army (executing flag, main state) and organisation.
    struct War2 {
        executing: Cell<bool>,
        org: Cell<f64>,
        supply: Cell<f64>,
        calls: RefCell<Vec<(String, Value)>>,
    }
    impl War2 {
        fn new() -> Self {
            Self {
                executing: Cell::new(false),
                org: Cell::new(40.0),
                supply: Cell::new(80.0),
                calls: RefCell::new(vec![]),
            }
        }
        fn last(&self) -> (String, Value) {
            self.calls
                .borrow()
                .iter()
                .rev()
                .find(|(n, _)| n.starts_with("hoi4_army_"))
                .cloned()
                .unwrap()
        }
    }
    impl Tools for War2 {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.calls
                .borrow_mut()
                .push((name.to_string(), args.clone()));
            match name {
                "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                "hoi4_economy" => Ok(json!({"factory_trigger_count": {"military": 0},
                    "civilian_factories_available_for_projects": 0})),
                "hoi4_overview" => Ok(json!({"player": "GER"})),
                "hoi4_states" => match args["query"].as_str().unwrap() {
                    "GER" => Ok(json!({"states": [
                        {"state_id": 42, "owner": "GER", "controller": "GER", "x": 0.0, "z": 0.0},
                        {"state_id": 7, "owner": "GER", "controller": "GER", "x": 500.0, "z": 0.0}]})),
                    "FRA" => Ok(json!({"states": [
                        {"state_id": 28, "owner": "FRA", "controller": "FRA", "x": 10.0, "z": 10.0},
                        {"state_id": 18, "owner": "FRA", "controller": "FRA", "x": 50.0, "z": 50.0}]})),
                    other => panic!("unexpected query {other}"),
                },
                "hoi4_armies" => Ok(json!({"unassigned_by_state": [], "armies": [
                    {"army": "Army 1", "division_count": 10, "main_state": 42,
                     "battle_plan_orders": 0, "plan_executing": self.executing.get(), "commander": "X"}]})),
                "hoi4_supply" => Ok(
                    json!({"armies": [{"army": "Army 1", "organisation_mean": self.org.get(),
                        "supply_percent_mean": self.supply.get()}]}),
                ),
                _ if args.get("request_id").is_some() => {
                    Ok(json!({"receipt": {"status": "Verified"}}))
                }
                _ => Ok(json!({})),
            }
        }
    }

    #[test]
    fn weak_templates_outside_the_theatre_are_disbanded_one_army_at_a_time() {
        // Army 9 fights in enemy land (835) and Army 1 stands in the theatre: both are kept.
        // A home army with one division already in the theatre loses only its home divisions.
        struct D(RefCell<Vec<(String, Value)>>);
        impl Tools for D {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                self.0.borrow_mut().push((name.to_string(), args.clone()));
                match name {
                    "hoi4_research" => Ok(json!({"slots": [{"researching": "a"}]})),
                    "hoi4_focus_completed" => Ok(json!({"active_key": "f"})),
                    "hoi4_economy" => Ok(json!({"factory_trigger_count": {"military": 0},
                        "civilian_factories_available_for_projects": 0})),
                    "hoi4_overview" => Ok(json!({"player": "ITA"})),
                    "hoi4_states" => Ok(json!({"states": [
                        {"state_id": 158, "owner": "ITA", "controller": "ITA"},
                        {"state_id": 160, "owner": "ITA", "controller": "ITA"},
                        {"state_id": 550, "owner": "ITA", "controller": "ITA"}]})),
                    "hoi4_supply" => Ok(json!({"divisions": [
                        {"army": "Army 9", "id": "x", "template": "Camicie Nere", "state_id": 835},
                        {"army": "Army 9", "id": "y", "template": "Camicie Nere", "state_id": 160},
                        {"army": "Army 4", "id": "a", "template": "Camicie Nere", "state_id": 158},
                        {"army": "Army 4", "id": "b", "template": "Divisione di Fanteria", "state_id": 158},
                        {"army": "Army 1", "id": "c", "template": "Camicie Nere", "state_id": 550},
                        {"army": "Army 5", "id": "d", "template": "Camicie Nere", "state_id": 160}]})),
                    _ if args.get("request_id").is_some() => {
                        Ok(json!({"receipt": {"status": "Verified"}}))
                    }
                    _ => Ok(json!({})),
                }
            }
        }
        let d = D(RefCell::new(vec![]));
        let plan = json!({"disband": {"templates": ["Camicie Nere"], "max": 1},
            "war": {"enabled": false, "theatre": [550]}});
        let mut state = State::default();
        let step = tick(&mut state, &d, &plan, "l", START).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("disband 1 from Army 4")),
            "{step:?}"
        );
        let (_, args) =
            d.0.borrow()
                .iter()
                .rev()
                .find(|(n, _)| n == "hoi4_division_disband")
                .cloned()
                .unwrap();
        assert_eq!(args["divisions"], json!(["a"]));
        // max reached: nothing more
        assert_eq!(
            tick(&mut state, &d, &plan, "l", START + DAY).unwrap(),
            Tick::Nothing
        );
    }

    #[test]
    fn a_starving_army_rests_and_attacks_again_after_the_longest_rest() {
        let w = War2::new();
        w.executing.set(true);
        w.supply.set(15.0);
        let plan = json!({"war": {"enemies": ["FRA"], "rest_max_days": 10}});
        let mut state = State::default();
        let step = tick(&mut state, &w, &plan, "l", START).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("war rest Army 1") && s.contains("supply 15%")),
            "{step:?}"
        );
        w.executing.set(false);
        assert_eq!(
            tick(&mut state, &w, &plan, "l", START + 5 * DAY).unwrap(),
            Tick::Nothing
        );
        let step = tick(&mut state, &w, &plan, "l", START + 11 * DAY).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("war resume Army 1")),
            "{step:?}"
        );
    }

    #[test]
    fn an_axis_beats_the_nearest_state() {
        let w = War2::new();
        let plan = json!({"war": {"enemies": ["FRA"], "axes": [[18, 28]]}});
        let mut state = State::default();
        let step = tick(&mut state, &w, &plan, "l", START).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("war front Army 1 -> state 18")),
            "{step:?}"
        );
    }

    #[test]
    fn armies_outside_the_theatre_are_left_alone() {
        let w = War2::new();
        let plan = json!({"war": {"enemies": ["FRA"], "theatre": [7]}});
        let mut state = State::default();
        assert_eq!(
            tick(&mut state, &w, &plan, "l", START).unwrap(),
            Tick::Nothing
        );
        assert!(!w
            .calls
            .borrow()
            .iter()
            .any(|(n, _)| n.starts_with("hoi4_army_")));
    }

    #[test]
    fn a_stalled_plan_becomes_a_direct_move_into_the_target() {
        let w = War2::new();
        let plan = json!({"war": {"enemies": ["FRA"], "stall_days": 8}});
        let mut state = State::default();
        let mut now = START;
        for _ in 0..3 {
            tick(&mut state, &w, &plan, "l", now).unwrap();
            now += 3;
        }
        assert_eq!(w.last().0, "hoi4_army_plan");
        w.executing.set(true);
        // planning and fighting, no progress: nothing for a week
        now += 13;
        tick(&mut state, &w, &plan, "l", now).unwrap();
        assert_eq!(w.last().0, "hoi4_army_plan");
        now += 7 * DAY;
        assert_eq!(
            tick(&mut state, &w, &plan, "l", now).unwrap(),
            Tick::Nothing
        );
        now += 2 * DAY;
        let step = tick(&mut state, &w, &plan, "l", now).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("war stalled Army 1")),
            "{step:?}"
        );
        let (name, args) = w.last();
        assert_eq!(
            (name.as_str(), &args["state_id"]),
            ("hoi4_army_move", &json!(28))
        );
    }

    #[test]
    fn an_exhausted_army_rests_and_attacks_again() {
        let w = War2::new();
        w.executing.set(true);
        w.org.set(8.0);
        let plan = json!({"war": {"enemies": ["FRA"]}});
        let mut state = State::default();
        let step = tick(&mut state, &w, &plan, "l", START).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("war rest Army 1")),
            "{step:?}"
        );
        assert_eq!(w.last().1["execute"], json!(false));
        w.executing.set(false);
        w.org.set(20.0);
        assert_eq!(
            tick(&mut state, &w, &plan, "l", START + DAY).unwrap(),
            Tick::Nothing
        );
        w.org.set(35.0);
        let step = tick(&mut state, &w, &plan, "l", START + 2 * DAY).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("war resume Army 1")),
            "{step:?}"
        );
        assert_eq!(w.last().1["execute"], json!(true));
    }

    #[test]
    fn without_a_foothold_the_army_steps_into_the_state() {
        let w = War {
            liberate: false,
            front_ok: false,
            calls: RefCell::new(vec![]),
        };
        let mut state = State::default();
        let step = tick(&mut state, &w, &war_plan(), "l", START).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("war move Army 1 -> state 28: true"))
        );
    }

    #[test]
    fn uncertain_front_stops_before_move_fallback_and_preserves_the_war_stage() {
        let reads = War {
            liberate: false,
            front_ok: true,
            calls: RefCell::new(vec![]),
        };
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([uncertain(), uncertain()].into()),
        };
        let mut state = State::default();
        let error = tick(&mut state, &tools, &war_plan(), "l", START).unwrap_err();
        assert_eq!(error.code, "action_uncertain");
        let calls = tools.actions.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0], calls[1]);
        assert_eq!(calls[0].0, "hoi4_army_front");
        assert!(state.war_stage.is_empty());
        assert!(state.war_cool.is_empty());
    }

    #[test]
    fn own_states_held_by_an_enemy_come_first_and_are_attacked_by_moving_in() {
        let w = War {
            liberate: true,
            front_ok: true,
            calls: RefCell::new(vec![]),
        };
        let mut state = State::default();
        let step = tick(&mut state, &w, &war_plan(), "l", START).unwrap();
        assert!(matches!(&step, Tick::Acted(s) if s.starts_with("war move Army 1 -> state 5")));
    }

    #[test]
    fn merging_the_plan_replaces_keys_and_null_removes_them() {
        let dir = std::env::temp_dir().join(format!("plan-run-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plan.json");
        let _ = std::fs::remove_file(&path);
        let mut first = Map::new();
        first.insert("a".into(), json!(1));
        first.insert("b".into(), json!(2));
        merge_plan(&path, &first).unwrap();
        let mut second = Map::new();
        second.insert("b".into(), Value::Null);
        second.insert("c".into(), json!([3]));
        let merged = merge_plan(&path, &second).unwrap();
        assert_eq!(merged, json!({"a": 1, "c": [3]}));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn plan_file(plan: &Value) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "plan-run-test-{}-{}.json",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::write(&path, serde_json::to_vec(plan).unwrap()).unwrap();
        path
    }
    fn target() -> GameHour {
        crate::calendar::encode(1939, 9, 1).unwrap() as GameHour
    }
    fn time_calls(f: &Fake) -> Vec<Value> {
        f.calls
            .borrow()
            .iter()
            .filter(|(n, _)| n == "hoi4_time")
            .map(|(_, a)| a.clone())
            .collect()
    }
    fn date_plan() -> Value {
        json!({"stop_on_date": "1939.09.01", "research_priority": ["radio"], "focus_path": ["F"]})
    }

    #[test]
    fn before_the_stop_date_the_runner_works_normally() {
        let tools = fake(vec![(
            "hoi4_research",
            json!({"slots": [{"researching": null}]}),
        )]);
        let step = tick(
            &mut State::default(),
            &tools,
            &date_plan(),
            "l",
            target() - 1,
        )
        .unwrap();
        assert_eq!(step, Tick::Acted("research radio slot 0".into()));
        assert!(time_calls(&tools).is_empty());
    }

    #[test]
    fn on_the_stop_date_the_game_is_paused_once_and_nothing_else_is_done() {
        let tools = fake(vec![
            ("hoi4_research", json!({"slots": [{"researching": null}]})),
            (
                "hoi4_events",
                json!({"events": [{"event": "news.1", "options": ["a"]}]}),
            ),
        ]);
        let mut state = State::default();
        let step = tick(&mut state, &tools, &date_plan(), "lease", target()).unwrap();
        assert!(
            matches!(&step, Tick::DateStop(s) if s.contains("1939.09.01") && s.ends_with("pause: paused")),
            "{step:?}"
        );
        let pauses = time_calls(&tools);
        assert_eq!(pauses.len(), 1);
        assert_eq!(pauses[0]["paused"], json!(true));
        assert_eq!(pauses[0]["lease_token"], json!("lease"));
        assert!(pauses[0]["request_id"]
            .as_str()
            .unwrap()
            .starts_with("plan-sd-"));
        assert!(names(&tools)
            .iter()
            .all(|n| matches!(n.as_str(), "hoi4_overview" | "hoi4_time" | "hoi4_date_stop")));
        // later ticks (and a later date) never pause again nor act
        for hour in [target(), target() + 5 * DAY] {
            let step = tick(&mut state, &tools, &date_plan(), "lease", hour).unwrap();
            assert!(matches!(step, Tick::DateStop(_)));
        }
        assert_eq!(time_calls(&tools).len(), 1);
        assert!(!names(&tools).contains(&"hoi4_events".to_string()));
    }

    #[test]
    fn a_runner_started_after_the_date_stops_at_once() {
        let tools = fake(vec![]);
        let step = tick(
            &mut State::default(),
            &tools,
            &date_plan(),
            "l",
            target() + 40 * DAY,
        )
        .unwrap();
        assert!(matches!(step, Tick::DateStop(_)));
        assert_eq!(time_calls(&tools).len(), 1);
    }

    #[test]
    fn an_already_paused_game_gets_no_pause_action() {
        let tools = fake(vec![("hoi4_overview", json!({"paused": true}))]);
        let step = tick(&mut State::default(), &tools, &date_plan(), "l", target()).unwrap();
        assert!(matches!(&step, Tick::DateStop(s) if s.ends_with("already paused")));
        assert!(time_calls(&tools).is_empty());
    }

    struct WithWatcher<'a>(&'a Fake, Value);
    impl Tools for WithWatcher<'_> {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            if name == "hoi4_date_stop" {
                return Ok(self.1.clone());
            }
            self.0.call(name, args)
        }
    }

    #[test]
    fn a_pause_the_server_watcher_fired_or_is_sending_is_not_repeated() {
        for (watcher, expect) in [
            (
                json!({"fired": true, "outcome": "paused"}),
                "paused (server date watcher)",
            ),
            (
                json!({"in_flight": true}),
                "pause in flight (server date watcher); not re-issued",
            ),
        ] {
            let reads = fake(vec![]);
            let tools = WithWatcher(&reads, watcher);
            let step = tick(
                &mut State::default(),
                &tools,
                &date_plan(),
                "l",
                target() + 9 * DAY,
            )
            .unwrap();
            assert!(
                matches!(&step, Tick::DateStop(s) if s.ends_with(expect)),
                "{step:?}"
            );
            assert!(time_calls(&reads).is_empty());
        }
    }

    #[test]
    fn stop_on_date_pause_false_only_stops_acting() {
        let tools = fake(vec![]);
        let mut plan = date_plan();
        plan["stop_on_date_pause"] = json!(false);
        let step = tick(&mut State::default(), &tools, &plan, "l", target()).unwrap();
        assert!(matches!(&step, Tick::DateStop(s) if s.contains("not requested")));
        assert!(time_calls(&tools).is_empty());
        assert_eq!(names(&tools), Vec::<String>::new());
    }

    #[test]
    fn a_refused_pause_is_reported_and_never_retried() {
        let reads = fake(vec![]);
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([Err(Error::new("invalid_target", "no window"))].into()),
        };
        let mut state = State::default();
        let step = tick(&mut state, &tools, &date_plan(), "l", target()).unwrap();
        assert!(
            matches!(&step, Tick::DateStop(s) if s.contains("pause refused") && s.contains("not retried")),
            "{step:?}"
        );
        for _ in 0..3 {
            assert!(matches!(
                tick(&mut state, &tools, &date_plan(), "l", target() + DAY).unwrap(),
                Tick::DateStop(_)
            ));
        }
        assert_eq!(tools.actions.borrow().len(), 1);
    }

    #[test]
    fn an_uncertain_pause_settles_its_own_id_once_then_reports_without_retrying() {
        let reads = fake(vec![]);
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([uncertain(), uncertain()].into()),
        };
        let mut state = State::default();
        let step = tick(&mut state, &tools, &date_plan(), "l", target()).unwrap();
        assert!(
            matches!(&step, Tick::DateStop(s) if s.contains("pause uncertain")),
            "{step:?}"
        );
        assert!(matches!(
            tick(&mut state, &tools, &date_plan(), "l", target() + 1).unwrap(),
            Tick::DateStop(_)
        ));
        let calls = tools.actions.borrow();
        assert_eq!(calls.len(), 2);
        assert_eq!(
            calls[0], calls[1],
            "only the same request id may be settled"
        );
    }

    #[test]
    fn invalid_stop_dates_are_refused_before_any_call() {
        for plan in [
            json!({"stop_on_date": "tomorrow"}),
            json!({"stop_on_date": "1939.02.30"}),
            json!({"stop_on_date": "1939.09.01", "stop_on_date_pause": "yes"}),
        ] {
            let tools = fake(vec![]);
            let at = target();
            assert_eq!(
                tick(&mut State::default(), &tools, &plan, "l", at)
                    .unwrap_err()
                    .code,
                "invalid_plan"
            );
            assert!(names(&tools).is_empty());
        }
    }

    #[test]
    fn a_later_stop_date_rearms_the_runner() {
        let tools = fake(vec![]);
        let mut state = State::default();
        tick(&mut state, &tools, &date_plan(), "l", target()).unwrap();
        let later = json!({"stop_on_date": {"year": 1940, "month": 1, "day": 1}});
        let step = tick(&mut state, &tools, &later, "l", target() + DAY).unwrap();
        assert!(!matches!(step, Tick::DateStop(_)));
    }

    #[test]
    fn run_reports_stopped_on_date_and_the_alarm_names_it() {
        let tools = fake(vec![(
            "hoi4_overview",
            json!({"date": {"raw": target() + 30}}),
        )]);
        let path = plan_file(&date_plan());
        let result = run(&mut State::default(), &tools, &path, "l", 1).unwrap();
        assert_eq!(result["stopped_on_date"], json!(true));
        assert!(result["reason"]
            .as_str()
            .unwrap()
            .contains("stop_on_date 1939.09.01"));
        assert_eq!(result["stop_on_date_pause"], json!("paused"));
        assert!(result["actions"].as_array().unwrap().is_empty());
        let codes: Vec<&str> = result["alarms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["code"].as_str().unwrap())
            .collect();
        assert!(codes.contains(&"target_date_reached"), "{codes:?}");
        assert_eq!(time_calls(&tools).len(), 1);
        let _ = std::fs::remove_file(path);
    }

    // ----- research and focus never stall

    fn catalogue() -> Value {
        json!({"technologies": ["naval_hull1", "infantry_weapons1", "armor_basic", "radio"]})
    }
    fn idle_research() -> Value {
        json!({"slots": [{"researching": null}], "researched": [{"id": "radio"}]})
    }
    fn started(tools: &Fake) -> Vec<String> {
        tools
            .calls
            .borrow()
            .iter()
            .filter(|(n, _)| n == "hoi4_research_start")
            .map(|(_, a)| a["technology"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn the_default_fallback_skips_ships_and_takes_the_first_land_tech() {
        let tools = fake(vec![
            ("hoi4_research", idle_research()),
            ("hoi4_technology_list", catalogue()),
        ]);
        let mut state = State::default();
        let step = tick(
            &mut state,
            &tools,
            &json!({"research_blind_catalogue": true, "research_priority": ["radio"]}),
            "l",
            START,
        )
        .unwrap();
        assert_eq!(
            step,
            Tick::Acted("research infantry_weapons1 slot 0 (fallback)".into())
        );
        assert_eq!(state.stages.fallback_used, ["infantry_weapons1"]);
        assert!(state
            .stages
            .research
            .as_ref()
            .unwrap()
            .0
            .contains("catalogue fallback"));
    }

    #[test]
    fn research_takes_available_in_time_techs_in_the_plans_order() {
        struct Avail(RefCell<Vec<(String, Value)>>);
        impl Tools for Avail {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                self.0.borrow_mut().push((name.to_string(), args.clone()));
                Ok(match name {
                    "hoi4_research" => idle_research(),
                    "hoi4_technology_list" if args["available"] == json!(true) => json!({
                        "available_only": true,
                        "technologies": ["artillery1", "industrial_complex_expansion1", "jet_engines"],
                        "start_years": {"artillery1": 1936, "industrial_complex_expansion1": 1936, "jet_engines": 1939}}),
                    _ if args.get("request_id").is_some() => {
                        json!({"receipt": {"status": "Verified"}})
                    }
                    _ => json!({}),
                })
            }
        }
        let tools = Avail(RefCell::new(vec![]));
        let mut state = State::default();
        // 1936.03.01: raw hours count from year -5000
        let now = (6936 * 8760 + 60 * 24) as GameHour;
        let plan = json!({"research_priority": ["artillery2"],
            "research_fallback_prefixes": ["industr", "artillery", "jet"]});
        tick(&mut state, &tools, &plan, "l", now).unwrap();
        let started: Vec<Value> = tools
            .0
            .borrow()
            .iter()
            .filter(|(n, _)| n == "hoi4_research_start")
            .map(|(_, a)| a["technology"].clone())
            .collect();
        // artillery2 is not available (prerequisite missing); industry first; jet_engines is
        // ahead of its time in 1936.
        assert_eq!(started, vec![json!("industrial_complex_expansion1")]);
    }

    #[test]
    fn priority_only_or_false_never_touches_the_catalogue_and_flags_the_idle_slot() {
        for mode in [json!("priority_only"), json!(false)] {
            let tools = fake(vec![
                ("hoi4_research", idle_research()),
                ("hoi4_technology_list", catalogue()),
            ]);
            let mut state = State::default();
            let plan = json!({"research_priority": ["radio"], "research_fallback": mode});
            let step = tick(&mut state, &tools, &plan, "l", START).unwrap();
            assert_eq!(step, Tick::Nothing);
            assert!(!names(&tools).contains(&"hoi4_technology_list".to_string()));
            let (line, idle) = state.stages.research.clone().unwrap();
            assert!(
                idle && line.contains("plan exhausted") && line.contains("fallback off"),
                "{line}"
            );
        }
    }

    #[test]
    fn fallback_prefixes_and_categories_restrict_the_catalogue_pick() {
        let tools = fake(vec![
            ("hoi4_research", idle_research()),
            ("hoi4_technology_list", catalogue()),
        ]);
        let plan =
            json!({"research_blind_catalogue": true, "research_fallback_prefixes": ["infantry"]});
        tick(&mut State::default(), &tools, &plan, "l", START).unwrap();
        assert_eq!(started(&tools), ["infantry_weapons1"]);
        let tools = fake(vec![
            ("hoi4_research", idle_research()),
            ("hoi4_technology_list", catalogue()),
        ]);
        let plan =
            json!({"research_blind_catalogue": true, "research_fallback_categories": ["armor"]});
        tick(&mut State::default(), &tools, &plan, "l", START).unwrap();
        assert_eq!(started(&tools), ["armor_basic"]);
        // nothing in the catalogue matches: nothing is started and the plan is exhausted
        let tools = fake(vec![
            ("hoi4_research", idle_research()),
            ("hoi4_technology_list", catalogue()),
        ]);
        let mut state = State::default();
        let plan =
            json!({"research_blind_catalogue": true, "research_fallback_prefixes": ["air_"]});
        assert_eq!(
            tick(&mut state, &tools, &plan, "l", START).unwrap(),
            Tick::Nothing
        );
        assert!(started(&tools).is_empty());
        assert!(state.stages.research.unwrap().1);
    }

    #[test]
    fn refused_techs_use_the_configured_skip_windows() {
        let reads = fake(vec![("hoi4_research", idle_research())]);
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([Err(Error::new("invalid_target", "no"))].into()),
        };
        let mut state = State::default();
        let plan = json!({"research_priority": ["a"], "research_fallback": false, "skip_hours": {"tech": 10}});
        tick(&mut state, &tools, &plan, "l", START).unwrap();
        assert!(state.skipped(START + 9, "tech", "a"));
        assert!(!state.skipped(START + 10, "tech", "a"));
    }

    fn focus_reads() -> Fake {
        fake(vec![
            ("hoi4_research", json!({"slots": [{"researching": "x"}]})),
            (
                "hoi4_focus_completed",
                json!({"active_key": null, "completed": ["Done"]}),
            ),
        ])
    }
    fn blocked() -> Result<Value> {
        Err(Error::new("focus_not_available", "requirements unmet"))
    }

    #[test]
    fn a_blocked_focus_waits_ten_days_and_the_next_key_starts_in_the_same_tick() {
        let reads = focus_reads();
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new(
                [blocked(), Ok(json!({"receipt": {"status": "Verified"}}))].into(),
            ),
        };
        let mut state = State::default();
        let plan = json!({"focus_path": ["Done", "A", "B"]});
        let step = tick(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(step, Tick::Acted("focus B".into()));
        let tried: Vec<String> = tools
            .actions
            .borrow()
            .iter()
            .map(|a| a.1["focus"].as_str().unwrap().into())
            .collect();
        assert_eq!(tried, ["A", "B"]);
        assert!(state.skipped(START + 5, "focus", "A"));
        assert!(
            !state.skipped(START + 10 * DAY, "focus", "A"),
            "default blocked window is 10 game days (6 hours was under a second at speed 3)"
        );
        assert_eq!(
            focus_blocked(&state, &plan, START + 1),
            ["A:focus_not_available"]
        );
        assert!(focus_blocked(&state, &plan, START + 10 * DAY).is_empty());
    }

    #[test]
    fn the_blocked_window_and_the_focus_fallback_list_are_configurable() {
        let reads = focus_reads();
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new(
                [blocked(), Ok(json!({"receipt": {"status": "Verified"}}))].into(),
            ),
        };
        let mut state = State::default();
        let plan = json!({"focus_path": ["A"], "focus_fallback": ["Z"], "skip_hours": {"focus_blocked": 30}});
        let step = tick(&mut state, &tools, &plan, "l", START).unwrap();
        assert_eq!(step, Tick::Acted("focus Z".into()));
        assert!(state.skipped(START + 29, "focus", "A"));
        assert!(!state.skipped(START + 30, "focus", "A"));
    }

    #[test]
    fn other_focus_refusals_keep_the_day_long_skip() {
        let reads = focus_reads();
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([Err(Error::new("ui_blocked", "popup"))].into()),
        };
        let mut state = State::default();
        tick(
            &mut state,
            &tools,
            &json!({"focus_path": ["A"]}),
            "l",
            START,
        )
        .unwrap();
        assert!(state.skipped(START + DAY - 1, "focus", "A"));
        assert!(!state.skipped(START + DAY, "focus", "A"));
    }

    #[test]
    fn an_exhausted_or_fully_blocked_focus_plan_is_flagged_idle() {
        let reads = focus_reads();
        let mut state = State::default();
        let plan = json!({"focus_path": ["Done"], "focus_fallback": ["Done"]});
        assert_eq!(
            tick(&mut state, &reads, &plan, "l", START).unwrap(),
            Tick::Nothing
        );
        let (line, idle) = state.stages.focus.clone().unwrap();
        assert!(idle && line.contains("all completed"), "{line}");
        // everything left is on a skip timer
        let tools = ActionReplies {
            reads: &reads,
            actions: RefCell::new(vec![]),
            replies: RefCell::new([blocked()].into()),
        };
        let mut state = State::default();
        let plan = json!({"focus_path": ["Done", "A"]});
        assert_eq!(
            tick(&mut state, &tools, &plan, "l", START).unwrap(),
            Tick::Nothing
        );
        let (line, idle) = state.stages.focus.clone().unwrap();
        assert!(idle && line.contains("A:focus_not_available"), "{line}");
        // a plan with no focus keys at all says nothing
        let mut state = State::default();
        tick(&mut state, &reads, &json!({}), "l", START).unwrap();
        assert!(state.stages.focus.is_none());
    }

    #[test]
    fn run_digest_has_research_and_focus_lines_and_the_exhaustion_alarms() {
        let tools = fake(vec![
            ("hoi4_research", idle_research()),
            (
                "hoi4_focus_completed",
                json!({"active_key": null, "completed": ["Done"]}),
            ),
            ("hoi4_overview", json!({"date": {"raw": START}})),
        ]);
        let path = plan_file(
            &json!({"research_priority": ["radio"], "research_fallback": "priority_only",
            "focus_path": ["Done"]}),
        );
        let result = run(&mut State::default(), &tools, &path, "l", 1).unwrap();
        assert!(result["research"]
            .as_str()
            .unwrap()
            .contains("plan exhausted"));
        assert!(result["focus"]
            .as_str()
            .unwrap()
            .starts_with("focus: none active"));
        let codes: Vec<&str> = result["alarms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["code"].as_str().unwrap())
            .collect();
        assert!(
            codes.contains(&"research_slot_idle_plan_exhausted"),
            "{codes:?}"
        );
        assert!(
            codes.contains(&"no_active_focus_plan_exhausted"),
            "{codes:?}"
        );
        assert!(!codes.contains(&"research_fallback_used"));
        // silenced by the plan's alarms section
        let path2 = plan_file(
            &json!({"research_priority": ["radio"], "research_fallback": false,
            "focus_path": ["Done"], "alarms": {"research_slot_idle_plan_exhausted": false, "no_active_focus_plan_exhausted": false}}),
        );
        let result = run(&mut State::default(), &tools, &path2, "l", 1).unwrap();
        let codes: Vec<&str> = result["alarms"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["code"].as_str().unwrap())
            .collect();
        assert!(
            !codes.iter().any(|c| c.ends_with("plan_exhausted")),
            "{codes:?}"
        );
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path2);
    }

    #[test]
    fn run_reports_the_fallback_pick_and_focus_blocked_entries() {
        let tools = fake(vec![
            ("hoi4_research", idle_research()),
            ("hoi4_technology_list", catalogue()),
            ("hoi4_focus_completed", json!({"active_key": "Busy"})),
        ]);
        let path =
            plan_file(&json!({"research_blind_catalogue": true, "research_priority": ["radio"]}));
        let mut state = State::default();
        let result = run(&mut state, &tools, &path, "l", 1).unwrap();
        let alarm = result["alarms"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["code"] == "research_fallback_used")
            .cloned();
        assert!(alarm.unwrap()["message"]
            .as_str()
            .unwrap()
            .contains("infantry_weapons1"));
        assert!(result["research"]
            .as_str()
            .unwrap()
            .contains("catalogue fallback"));
        // focus_blocked comes from skip timers still running
        state.skip(START, "focus", "A", 6);
        state
            .stages
            .focus_why
            .insert("A".into(), "focus_not_available".into());
        state.last_hour = START;
        let path2 = plan_file(&json!({"focus_path": ["A"]}));
        let result = run(
            &mut state,
            &fake(vec![(
                "hoi4_focus_completed",
                json!({"active_key": "Busy"}),
            )]),
            &path2,
            "l",
            1,
        )
        .unwrap();
        assert_eq!(result["focus_blocked"], json!(["A:focus_not_available"]));
        assert_eq!(result["focus"], json!("focus: active Busy"));
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(path2);
    }

    #[test]
    fn an_inactive_plan_reports_why_in_the_run_result() {
        let path = plan_file(&json!({"active": false}));
        let result = run(&mut State::default(), &fake(vec![]), &path, "l", 1).unwrap();
        assert_eq!(result["reason"], json!("plan inactive (active:false)"));
        let _ = std::fs::remove_file(path);
    }

    /// The actions sent (name, args) by a fake.
    fn actions(f: &Fake) -> Vec<(String, Value)> {
        f.calls
            .borrow()
            .iter()
            .filter(|(_, a)| a.get("request_id").is_some())
            .cloned()
            .collect()
    }

    #[test]
    fn construction_plan_runs_in_peace_before_open_construction() {
        let tools = fake(vec![("hoi4_overview", json!({"player": "ITA"}))]);
        let plan = json!({
            "construction": [{"building": "arms_factory", "states": [7]}],
            "construction_plan": [
                {"id": "hub-5", "order": 1, "building": "supply_node", "state": 5,
                 "province": 900, "when": "1936.01.01"}]});
        let mut state = State::default();
        let now = date_hour("1936.03.01").unwrap();
        let step = tick(&mut state, &tools, &plan, "l", now).unwrap();
        assert!(
            matches!(&step, Tick::Acted(s) if s.starts_with("construction plan hub-5 supply_node in 5: true")),
            "{step:?}"
        );
        let sent = actions(&tools);
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert_eq!(sent[0].0, "hoi4_construction_add");
        assert_eq!(sent[0].1["state_id"], 5);
        assert_eq!(sent[0].1["province_id"], 900);
        assert!(state.plan_built.contains_key("id:hub-5"));
        // queued once: the next peace tick does not send it again
        tick(&mut state, &tools, &plan, "l", now + DAY).unwrap();
        assert_eq!(
            actions(&tools)
                .iter()
                .filter(|(_, a)| a["building"] == "supply_node")
                .count(),
            1
        );
        // a plan whose items are all done reads nothing for them
        let before = tools.calls.borrow().len();
        assert_eq!(
            construction_plan_step(&mut state, &tools, &plan, "l", now).unwrap(),
            None
        );
        assert_eq!(tools.calls.borrow().len(), before);
    }

    #[test]
    fn construction_plan_waits_for_its_date_in_peace() {
        let tools = fake(vec![("hoi4_overview", json!({"player": "ITA"}))]);
        let plan = json!({"construction_plan": [
            {"order": 2, "building": "infrastructure", "state": 5, "when": "1937.01.01"}]});
        let mut state = State::default();
        tick(
            &mut state,
            &tools,
            &plan,
            "l",
            date_hour("1936.06.01").unwrap(),
        )
        .unwrap();
        assert!(actions(&tools).is_empty());
        let step = tick(
            &mut state,
            &tools,
            &plan,
            "l",
            date_hour("1937.01.02").unwrap(),
        )
        .unwrap();
        assert!(matches!(&step, Tick::Acted(s) if s.starts_with("construction plan #2")));
        assert!(state.plan_built.contains_key("order:2"));
    }

    fn doctrine_tools() -> Fake {
        fake(vec![(
            "hoi4_doctrines",
            json!({"experience": {"army": {"value": 150.0}, "navy": {"value": 10.0},
                "air": {"value": 0.0}}}),
        )])
    }
    /// One doctrine pass a week apart (the check runs every 7 game days).
    fn doctrine_pass(state: &mut State, tools: &Fake, plan: &Value, week: i64) -> Option<String> {
        extras_step(state, tools, plan, "l", START + week * 8 * DAY).unwrap()
    }
    fn adopted(tools: &Fake) -> Vec<Value> {
        actions(tools)
            .into_iter()
            .filter(|(n, _)| n == "hoi4_doctrine_adopt")
            .map(|(_, mut a)| {
                let o = a.as_object_mut().unwrap();
                o.remove("request_id");
                o.remove("lease_token");
                a
            })
            .collect()
    }

    #[test]
    fn doctrines_new_shape_send_the_grand_doctrine_then_the_subdoctrine_on_its_track() {
        let plan = json!({"doctrines": [
            {"id": "land-grand", "folder": "land", "grand_doctrine": "new_mobile_warfare", "xp": 50},
            {"id": "land-inf", "folder": "land", "track": "infantry", "subdoctrine": "mobile_infantry", "xp": 100},
            {"id": "navy-grand", "folder": "naval", "grand_doctrine": "fleet_in_being", "xp": 50},
            {"id": "never", "folder": "land", "grand_doctrine": "x", "xp": 1}]});
        let tools = doctrine_tools();
        let mut state = State::default();
        let first = doctrine_pass(&mut state, &tools, &plan, 0).unwrap();
        assert!(
            first.starts_with("doctrine new_mobile_warfare: true"),
            "{first}"
        );
        doctrine_pass(&mut state, &tools, &plan, 1).unwrap();
        // the naval entry waits for navy XP (10 < 50) and blocks the entry after it
        assert_eq!(doctrine_pass(&mut state, &tools, &plan, 2), None);
        assert_eq!(
            adopted(&tools),
            vec![
                json!({"doctrine": "new_mobile_warfare"}),
                json!({"doctrine": "mobile_infantry", "track": "infantry"}),
            ]
        );
        assert!(state.done.contains_key("doc:id:land-grand"));
        assert!(state.done.contains_key("doc:id:land-inf"));
    }

    #[test]
    fn doctrines_old_shape_and_combined_entries_still_work() {
        let plan = json!({"doctrines": [
            {"doctrine": "superior_firepower", "track": "operations", "xp": 100, "folder": "land"},
            {"folder": "air", "xp": 0, "grand_doctrine": "strategic_destruction",
             "subdoctrine": "air_superiority_sub", "track": "fighters"}]});
        let tools = doctrine_tools();
        let mut state = State::default();
        for week in 0..4 {
            doctrine_pass(&mut state, &tools, &plan, week);
        }
        assert_eq!(
            adopted(&tools),
            vec![
                json!({"doctrine": "superior_firepower", "track": "operations"}),
                json!({"doctrine": "strategic_destruction"}),
                json!({"doctrine": "air_superiority_sub", "track": "fighters"}),
            ]
        );
        assert!(state.done.contains_key("doc:0"));
        assert!(state.done.contains_key("doc:1:grand"));
        assert!(state.done.contains_key("doc:1"));
    }

    #[test]
    fn id_keyed_done_memory_survives_a_list_reorder() {
        // doctrines
        let tools = doctrine_tools();
        let mut state = State::default();
        let a = json!({"id": "a", "folder": "land", "grand_doctrine": "ga", "xp": 10});
        let b = json!({"id": "b", "folder": "land", "subdoctrine": "sb", "track": "t", "xp": 10});
        doctrine_pass(
            &mut state,
            &tools,
            &json!({"doctrines": [a.clone(), b.clone()]}),
            0,
        );
        let reordered = json!({"doctrines": [b, a]});
        doctrine_pass(&mut state, &tools, &reordered, 1);
        assert_eq!(doctrine_pass(&mut state, &tools, &reordered, 2), None);
        assert_eq!(
            adopted(&tools),
            vec![
                json!({"doctrine": "ga"}),
                json!({"doctrine": "sb", "track": "t"})
            ]
        );
        // construction_plan: the revision renumbers and reorders the items
        let tools = fake(vec![("hoi4_overview", json!({"player": "ITA"}))]);
        let x = json!({"id": "x", "order": 1, "building": "infrastructure", "state": 5});
        let y = json!({"id": "y", "order": 2, "building": "supply_node", "state": 6});
        let mut state = State::default();
        let first = json!({"construction_plan": [x.clone(), y.clone()]});
        construction_plan_step(&mut state, &tools, &first, "l", START).unwrap();
        let mut y2 = y;
        y2["order"] = json!(1);
        let mut x2 = x;
        x2["order"] = json!(2);
        let revised = json!({"construction_plan": [y2, x2]});
        construction_plan_step(&mut state, &tools, &revised, "l", START).unwrap();
        assert_eq!(
            construction_plan_step(&mut state, &tools, &revised, "l", START).unwrap(),
            None
        );
        let states: Vec<Value> = actions(&tools)
            .iter()
            .map(|(_, a)| a["state_id"].clone())
            .collect();
        assert_eq!(states, vec![json!(5), json!(6)]);
        // politics
        let tools = fake(vec![]);
        let mut state = State::default();
        let p = json!({"id": "law-1", "category": "law", "slot": 0, "idea": "partial_economic_mobilisation"});
        let q = json!({"id": "adv-1", "category": "advisor", "slot": 1, "idea": "some_advisor"});
        tick(
            &mut state,
            &tools,
            &json!({"politics": [p.clone()]}),
            "l",
            START,
        )
        .unwrap();
        tick(
            &mut state,
            &tools,
            &json!({"politics": [q.clone(), p.clone()]}),
            "l",
            START,
        )
        .unwrap();
        tick(
            &mut state,
            &tools,
            &json!({"politics": [q, p]}),
            "l",
            START + DAY,
        )
        .unwrap();
        let ideas: Vec<Value> = actions(&tools)
            .iter()
            .filter(|(n, _)| n == "hoi4_politics_choose")
            .map(|(_, a)| a["idea"].clone())
            .collect();
        assert_eq!(
            ideas,
            vec![
                json!("partial_economic_mobilisation"),
                json!("some_advisor")
            ]
        );
    }

    /// A plan file in its own fresh directory (so its plan_state.json is its own).
    fn plan_dir(plan: &Value) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "plan-state-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("plan.json");
        std::fs::write(&path, serde_json::to_vec(plan).unwrap()).unwrap();
        path
    }
    fn campaign(playthrough: &str, hour: GameHour) -> Fake {
        fake(vec![
            (
                "hoi4_overview",
                json!({"player": "ITA", "date": {"raw": hour}}),
            ),
            (
                "hoi4_construction_queue",
                json!({"handle_scope": {"playthrough_id": playthrough}}),
            ),
        ])
    }

    #[test]
    fn state_round_trips_through_plan_state_json() {
        let path = plan_dir(&json!({}));
        assert_eq!(state_path(&path), path.with_file_name("plan_state.json"));
        let tools = campaign("p-1", START);
        let mut state = State::default();
        restore_memory(&mut state, &tools, &json!({}), &path, START);
        assert!(state.persist.restored);
        state.done.insert("pol:id:law-1".into(), START - DAY);
        state
            .done
            .insert("prod:infantry_equipment".into(), START - 2);
        state.plan_built.insert("id:hub-5".into(), START - 3);
        state.skip(START, "dec", "some_decision", 30 * DAY);
        state.skip(START - 40 * DAY, "dec", "old", DAY);
        state.layout.insert("strike".into(), "Army 2".into());
        state.layout.insert("empty".into(), String::new());
        save_memory(&mut state, &path, START);
        let saved: Value =
            serde_json::from_slice(&std::fs::read(state_path(&path)).unwrap()).unwrap();
        assert_eq!(saved["identity"]["playthrough"], "p-1");
        assert_eq!(saved["identity"]["player"], "ITA");
        assert!(
            saved["skips"].get("dec:old").is_none(),
            "expired timers are not kept"
        );
        assert!(!path.with_file_name("plan_state.json.tmp").exists());

        // a restarted server (a fresh State) takes it back
        let mut again = State::default();
        restore_memory(&mut again, &tools, &json!({}), &path, START + DAY);
        assert_eq!(again.done, state.done);
        assert_eq!(again.plan_built, state.plan_built);
        assert!(again.skipped(START + DAY, "dec", "some_decision"));
        assert_eq!(
            again.layout.get("strike").map(String::as_str),
            Some("Army 2")
        );
        assert!(!again.layout.contains_key("empty"));
        assert_eq!(again.persist.note, None);

        // an earlier save loaded: work done after the game date is dropped, timers too
        let mut rewound = State::default();
        restore_memory(&mut rewound, &tools, &json!({}), &path, START - DAY);
        assert!(rewound.done.contains_key("pol:id:law-1"));
        assert!(!rewound.done.contains_key("prod:infantry_equipment"));
        assert!(rewound.plan_built.is_empty());
        assert!(rewound.bad.is_empty());
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn plan_state_of_another_campaign_is_ignored_and_replaced() {
        let path = plan_dir(&json!({}));
        let mut state = State::default();
        restore_memory(
            &mut state,
            &campaign("p-1", START),
            &json!({}),
            &path,
            START,
        );
        state.done.insert("doc:id:a".into(), START);
        save_memory(&mut state, &path, START);
        // another playthrough in the same folder
        let mut other = State::default();
        restore_memory(
            &mut other,
            &campaign("p-2", START),
            &json!({}),
            &path,
            START,
        );
        assert!(other.done.is_empty());
        assert!(other
            .persist
            .note
            .as_deref()
            .unwrap()
            .contains("another campaign"));
        save_memory(&mut other, &path, START);
        let saved: Value =
            serde_json::from_slice(&std::fs::read(state_path(&path)).unwrap()).unwrap();
        assert_eq!(saved["identity"]["playthrough"], "p-2");
        assert_eq!(saved["done"], json!({}));
        // the plan's _meta.campaign tells campaigns apart too
        let plan_a = json!({"_meta": {"campaign": "ita-1936-a"}});
        let mut a = State::default();
        restore_memory(&mut a, &campaign("p-3", START), &plan_a, &path, START);
        a.done.insert("doc:id:a".into(), START);
        save_memory(&mut a, &path, START);
        let mut b = State::default();
        let plan_b = json!({"_meta": {"campaign": "ita-1936-b"}});
        restore_memory(&mut b, &campaign("p-3", START), &plan_b, &path, START);
        assert!(b.done.is_empty());
        let mut same = State::default();
        restore_memory(&mut same, &campaign("p-3", START), &plan_a, &path, START);
        assert!(same.done.contains_key("doc:id:a"));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_restarted_server_does_not_queue_a_construction_plan_item_again() {
        let plan = json!({"construction_plan": [
            {"id": "rail-1", "order": 1, "building": "infrastructure", "state": 5}]});
        let path = plan_dir(&plan);
        let tools = campaign("p-1", START);
        let result = run(&mut State::default(), &tools, &path, "l", 1).unwrap();
        assert!(
            result["actions"][0]
                .as_str()
                .unwrap()
                .starts_with("construction plan rail-1"),
            "{result}"
        );
        // the server restarts: a fresh State reads plan_state.json back
        let tools = campaign("p-1", START + DAY);
        let result = run(&mut State::default(), &tools, &path, "l", 1).unwrap();
        assert!(
            !actions(&tools)
                .iter()
                .any(|(n, _)| n == "hoi4_construction_add"),
            "{result}"
        );
        assert!(result.get("plan_state").is_none(), "{result}");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    // ---- strategy layer (goal, phases, checkpoints, contingencies), gated politics, decision
    // rules and event rules ----

    fn hour(date: &str) -> GameHour {
        crate::plan_predicate::date_hour(date).unwrap()
    }
    /// A game with an idle research slot: the research step shows which priority list (base or
    /// overridden) the tick used.
    fn slot_idle(extra: Vec<(&'static str, Value)>) -> Fake {
        let mut replies = vec![("hoi4_research", json!({"slots": [{"researching": null}]}))];
        replies.extend(extra);
        fake(replies)
    }
    fn news_of<'s>(state: &'s State, kind: &str) -> Vec<&'s Value> {
        state
            .strategy
            .news
            .iter()
            .filter(|n| n["kind"] == kind)
            .collect()
    }
    fn alarm_codes(alarms: &[Value]) -> Vec<String> {
        alarms
            .iter()
            .filter_map(|a| a["code"].as_str().map(str::to_string))
            .collect()
    }

    #[test]
    fn phases_switch_on_exit_and_their_overrides_apply_to_the_whole_tick() {
        let plan = json!({
            "research_priority": ["base_tech"],
            "phases": [
                {"id": "build", "exit": "at_war_with:ETH",
                 "overrides": {"research_priority": ["build_tech"]}},
                {"id": "war", "enter": "at_war_with:ETH", "revise_on_enter": true,
                 "overrides": {"research_priority": ["war_tech"], "war": {"enabled": false}}}]});
        let mut state = State::default();
        let now = hour("1936.01.01");
        let peace = slot_idle(vec![(
            "hoi4_wars",
            json!({"wars": {"at_war": false, "enemies": []}}),
        )]);
        assert_eq!(
            acted(tick(&mut state, &peace, &plan, "l", now).unwrap()),
            "research build_tech slot 0"
        );
        assert_eq!(state.strategy.phase_id().as_deref(), Some("build"));
        let war = slot_idle(vec![(
            "hoi4_wars",
            json!({"wars": {"at_war": true, "enemies": ["ETH"]}}),
        )]);
        assert_eq!(
            acted(tick(&mut state, &war, &plan, "l", now + DAY).unwrap()),
            "research war_tech slot 0"
        );
        assert_eq!(state.strategy.phase_id().as_deref(), Some("war"));
        let changes = news_of(&state, "phase_changed");
        assert_eq!(changes.len(), 2, "none -> build, build -> war");
        assert_eq!(changes[1]["detail"]["from"], "build");
        assert_eq!(changes[1]["detail"]["revise_on_enter"], true);
        assert!(changes[1]["detail"]["reason"]
            .as_str()
            .unwrap()
            .contains("exit of build"));
        // the phase is kept for plan_state.json; the base plan itself is untouched
        assert_eq!(state.memory(now + DAY)["strategy"]["phase"], "war");
        assert_eq!(plan["research_priority"], json!(["base_tech"]));
        let alarms = crate::alarms::strategy(&plan, &state.strategy.summary, &state.strategy.news);
        assert!(alarm_codes(&alarms).contains(&"phase_changed".to_string()));
    }

    #[test]
    fn overrides_replace_keys_merge_war_and_never_touch_the_strategy_keys() {
        let mut plan = json!({"war": {"enemies": ["ETH"], "min_army": 6}, "decisions": ["a"],
            "phases": [{"id": "x"}], "active": true});
        let overrides = json!({"war": {"min_army": 2, "enemies": null}, "decisions": null,
            "focus_path": ["f"], "phases": [], "active": false});
        apply_overrides(&mut plan, overrides.as_object().unwrap());
        assert_eq!(plan["war"], json!({"min_army": 2}));
        assert!(plan.get("decisions").is_none());
        assert_eq!(plan["focus_path"], json!(["f"]));
        assert_eq!(plan["phases"], json!([{"id": "x"}]));
        assert_eq!(plan["active"], true);
    }

    #[test]
    fn a_missed_checkpoint_raises_its_alarm_once_and_switches_to_its_else_phase() {
        let plan = json!({
            "phases": [{"id": "advance"},
                {"id": "defend", "enter": false, "overrides": {"research_priority": ["fort_tech"]}}],
            "checkpoints": [{"id": "addis", "by": "1936.06.01", "must": ["controls_state:550"],
                "else": "phase:defend"}]});
        let tools = slot_idle(vec![
            ("hoi4_overview", json!({"player": "ITA"})),
            (
                "hoi4_states",
                json!({"states": [{"state_id": 550, "owner": "ETH", "controller": "ETH"}]}),
            ),
        ]);
        let mut state = State::default();
        // before its date a checkpoint reads nothing
        tick(&mut state, &tools, &plan, "l", hour("1936.05.01")).unwrap();
        assert!(!names(&tools).contains(&"hoi4_states".to_string()));
        assert_eq!(state.strategy.phase_id().as_deref(), Some("advance"));
        let step = acted(tick(&mut state, &tools, &plan, "l", hour("1936.06.01")).unwrap());
        assert_eq!(step, "research fort_tech slot 0");
        assert_eq!(state.strategy.phase_id().as_deref(), Some("defend"));
        let missed = news_of(&state, "checkpoint_missed");
        assert_eq!(missed.len(), 1);
        assert_eq!(missed[0]["detail"]["failed"], json!(["controls_state:550"]));
        let alarms = crate::alarms::strategy(&plan, &state.strategy.summary, &state.strategy.news);
        let codes = alarm_codes(&alarms);
        assert!(
            codes.contains(&"checkpoint_missed".to_string()),
            "{codes:?}"
        );
        // settled once: later judgements do not raise it again
        state.strategy.news.clear();
        tick(&mut state, &tools, &plan, "l", hour("1936.06.03")).unwrap();
        assert!(state.strategy.news.is_empty(), "{:?}", state.strategy.news);
        assert_eq!(
            state.strategy.summary["checkpoints"]["missed"],
            json!(["addis"])
        );
    }

    #[test]
    fn a_checkpoint_that_reads_unknown_waits_and_one_that_holds_is_met() {
        let plan = json!({"checkpoints": [
            {"id": "a", "by": "1936.02.01", "must": ["bop:ETH_centralization > 0.2"], "else": "revise"},
            {"id": "b", "by": "1936.02.01", "must": ["1936.01.15"]}]});
        let mut state = State::default();
        tick(&mut state, &fake(vec![]), &plan, "l", hour("1936.02.02")).unwrap();
        assert!(state.strategy.news.is_empty());
        assert_eq!(state.strategy.summary["checkpoints"]["met"], json!(["b"]));
        assert_eq!(
            state.strategy.summary["checkpoints"]["pending"],
            json!(["a"])
        );
        assert!(state
            .strategy
            .unknown
            .contains_key("bop:ETH_centralization > 0.2"));
    }

    #[test]
    fn a_contingency_applies_its_overrides_while_its_trigger_holds() {
        let plan = json!({"research_priority": ["base_tech"],
            "contingencies": [{"id": "unrest", "when": "stability < 40%",
                "then": {"overrides": {"research_priority": ["calm_tech"]}}}]});
        let status = |s: f64| {
            slot_idle(vec![(
                "hoi4_country_status",
                json!({"fields": {"stability": {"value": s}}}),
            )])
        };
        let mut state = State::default();
        let now = hour("1936.03.01");
        assert_eq!(
            acted(tick(&mut state, &status(0.3), &plan, "l", now).unwrap()),
            "research calm_tech slot 0"
        );
        assert!(state.strategy.contingencies.contains_key("unrest"));
        assert_eq!(news_of(&state, "contingency_triggered").len(), 1);
        // within the judgement interval nothing is read again
        let calm = status(0.7);
        assert_eq!(
            acted(tick(&mut state, &calm, &plan, "l", now + 1).unwrap()),
            "research calm_tech slot 0"
        );
        assert!(!names(&calm).contains(&"hoi4_country_status".to_string()));
        // unreadable: the contingency stays as it is
        assert_eq!(
            acted(tick(&mut state, &slot_idle(vec![]), &plan, "l", now + DAY).unwrap()),
            "research calm_tech slot 0"
        );
        // calm again: cleared, the base plan applies
        assert_eq!(
            acted(tick(&mut state, &calm, &plan, "l", now + 2 * DAY).unwrap()),
            "research base_tech slot 0"
        );
        assert!(state.strategy.contingencies.is_empty());
    }

    #[test]
    fn a_contingency_with_a_phase_switches_to_it() {
        let plan = json!({"phases": [{"id": "attack"}, {"id": "hold", "enter": false}],
            "contingencies": [{"id": "core_lost", "trigger": "not_exists:ETH", "phase": "hold"}]});
        let tools = fake(vec![("hoi4_states", json!({"states": []}))]);
        let mut state = State::default();
        tick(&mut state, &tools, &plan, "l", hour("1936.03.01")).unwrap();
        assert_eq!(state.strategy.phase_id().as_deref(), Some("hold"));
        let changes = news_of(&state, "phase_changed");
        assert_eq!(
            changes.last().unwrap()["detail"]["reason"],
            "contingency core_lost"
        );
    }

    #[test]
    fn the_goal_is_reached_or_at_risk_by_its_predicates_and_deadline() {
        let plan = json!({"goal": {"predicates": ["vp_share:ETH <= 0.2"], "deadline": "1937.01.01",
            "fail_predicates": ["at_war_with:FRA"]}});
        let game = |share: f64, enemies: Value| {
            fake(vec![
                (
                    "hoi4_states",
                    json!({"states": [], "victory_points": {"share_held_by_others": share}}),
                ),
                ("hoi4_wars", json!({"wars": {"enemies": enemies}})),
            ])
        };
        let status = |state: &State| state.strategy.summary["goal"]["status"].clone();
        let mut state = State::default();
        tick(
            &mut state,
            &game(0.3, json!(["ETH"])),
            &plan,
            "l",
            hour("1936.03.01"),
        )
        .unwrap();
        assert_eq!(status(&state), "open");
        tick(
            &mut state,
            &game(0.3, json!(["ETH"])),
            &plan,
            "l",
            hour("1936.12.15"),
        )
        .unwrap();
        assert_eq!(status(&state), "at_risk", "{}", state.strategy.summary);
        let alarms = crate::alarms::strategy(&plan, &state.strategy.summary, &[]);
        assert_eq!(alarm_codes(&alarms), ["goal_at_risk"]);
        let mut state = State::default();
        tick(
            &mut state,
            &game(0.3, json!(["ETH", "FRA"])),
            &plan,
            "l",
            hour("1936.03.01"),
        )
        .unwrap();
        assert!(state.strategy.summary["goal"]["why"]
            .as_str()
            .unwrap()
            .contains("at_war_with:FRA"));
        tick(
            &mut state,
            &game(0.85, json!(["ETH"])),
            &plan,
            "l",
            hour("1936.04.01"),
        )
        .unwrap();
        assert_eq!(status(&state), "reached");
        let alarms = crate::alarms::strategy(&plan, &state.strategy.summary, &[]);
        assert_eq!(alarm_codes(&alarms), ["goal_reached"]);
    }

    #[test]
    fn a_gated_law_waits_for_its_gate_then_is_taken() {
        let plan = json!({"politics": [{"id": "econ", "category": "government", "slot": 1,
            "idea": "partial_economic_mobilisation", "when": "war_support >= 25"}]});
        let support = |v: f64| {
            fake(vec![(
                "hoi4_country_status",
                json!({"fields": {"war_support": {"value": v}}}),
            )])
        };
        let mut state = State::default();
        let low = support(0.1);
        tick(&mut state, &low, &plan, "l", START).unwrap();
        assert!(!names(&low).contains(&"hoi4_politics_choose".to_string()));
        // unreadable: waits too, and says why
        let unread = fake(vec![]);
        tick(&mut state, &unread, &plan, "l", START + 1).unwrap();
        assert!(!names(&unread).contains(&"hoi4_politics_choose".to_string()));
        assert!(state.strategy.unknown.contains_key("war_support >= 25"));
        let high = support(0.3);
        let step = acted(tick(&mut state, &high, &plan, "l", START + DAY).unwrap());
        assert!(
            step.starts_with("politics partial_economic_mobilisation : true"),
            "{step}"
        );
        assert!(state.done.contains_key("pol:id:econ"));
    }

    #[test]
    fn a_never_take_decision_is_refused_in_every_path() {
        let plan = json!({"decisions": ["ITA_withdraw_from_ethiopia", "ITA_good"],
            "decision_rules": [
                {"id": "no-withdraw", "decision": "ITA_withdraw_from_ethiopia", "never": true},
                {"id": "withdraw-anyway", "decision": "ITA_withdraw_from_ethiopia"}]});
        let tools = fake(vec![(
            "hoi4_decisions_list",
            json!({"decisions": [
                {"decision": "ITA_withdraw_from_ethiopia", "enabled": true, "active_timer": false},
                {"decision": "ITA_good", "enabled": true, "active_timer": false}]}),
        )]);
        let mut state = State::default();
        assert_eq!(
            acted(tick(&mut state, &tools, &plan, "l", START).unwrap()),
            "decision ITA_good: true Verified"
        );
        tick(&mut state, &tools, &plan, "l", START + DAY).unwrap();
        let taken: Vec<Value> = actions(&tools)
            .iter()
            .filter(|(n, _)| n == "hoi4_decision_take")
            .map(|(_, a)| a["decision"].clone())
            .collect();
        assert_eq!(taken, vec![json!("ITA_good")]);
        assert!(decision_forbidden(&plan, "ITA_withdraw_from_ethiopia"));
        assert!(!decision_forbidden(&plan, "ITA_good"));
    }

    #[test]
    fn decision_rules_wait_for_when_pick_the_target_row_and_stop_at_max() {
        let plan = json!({"decision_rules": [{"id": "rail", "decision": "ETH_rail",
            "target": "Addis", "when": "at_war_with:ITA", "max": 1}]});
        let rows = json!({"decisions": [
            {"decision": "ETH_rail", "title": "Rail to Dire Dawa", "enabled": true},
            {"decision": "ETH_rail", "title": "Rail to Addis Abeba", "enabled": true}]});
        let game = |enemies: Value| {
            fake(vec![
                ("hoi4_decisions_list", rows.clone()),
                ("hoi4_wars", json!({"wars": {"enemies": enemies}})),
            ])
        };
        let mut state = State::default();
        let peace = game(json!([]));
        tick(&mut state, &peace, &plan, "l", START).unwrap();
        assert!(
            !names(&peace).contains(&"hoi4_decisions_list".to_string()),
            "the decisions panel is not opened while nothing may be taken"
        );
        let war = game(json!(["ITA"]));
        assert_eq!(
            acted(tick(&mut state, &war, &plan, "l", START + DAY).unwrap()),
            "decision ETH_rail on Addis (rule rail): true Verified"
        );
        assert_eq!(actions(&war)[0].1["target"], "Addis");
        let later = game(json!(["ITA"]));
        tick(&mut state, &later, &plan, "l", START + 40 * DAY).unwrap();
        assert!(actions(&later).is_empty(), "max 1 reached");
    }

    #[test]
    fn event_rules_choose_an_option_by_its_name_and_fall_back() {
        let events = json!({"events": [{"event": "ita_eth.5",
            "options": ["Negotiate a settlement", "§YPush on§! to Addis Ababa!"]}]});
        let plan = json!({"event_rules": [{"event": "ita_eth.5",
                "option_name": "push on to addis ababa", "when": "at_war_with:ETH"}],
            "event_options": {"ita_eth.5": 0}});
        let game = |enemies: Option<Value>| {
            let mut replies = vec![("hoi4_events", events.clone())];
            if let Some(e) = enemies {
                replies.push(("hoi4_wars", json!({"wars": {"enemies": e}})));
            }
            fake(replies)
        };
        let run_once = |plan: &Value, tools: &Fake| {
            acted(tick(&mut State::default(), tools, plan, "l", START).unwrap())
        };
        let step = run_once(&plan, &game(Some(json!(["ETH"]))));
        assert!(
            step.starts_with(
                "event ita_eth.5 option 1 (event_rules 'push on to addis ababa'): true"
            ),
            "{step}"
        );
        // the condition is false and the rule names no else option: event_options decides
        let step = run_once(&plan, &game(Some(json!([]))));
        assert!(step.starts_with("event ita_eth.5 option 0: true"), "{step}");
        // the condition cannot be read: the event waits for the model
        let mut state = State::default();
        assert_eq!(
            tick(&mut state, &game(None), &plan, "l", START).unwrap(),
            Tick::Waiting {
                event: "ita_eth.5".into(),
                new: true
            }
        );
        assert!(state.strategy.unknown.contains_key("at_war_with:ETH"));
        // a name the event does not offer: the rule's fallbacks, then event_options
        let fallback = json!({"event_rules": [{"event": "ita_eth.5", "option_name": "Retreat",
            "fallback_option_name": "negotiate"}]});
        let step = run_once(&fallback, &game(None));
        assert!(
            step.starts_with("event ita_eth.5 option 0 (event_rules fallback 'negotiate'"),
            "{step}"
        );
        let by_index = json!({"event_rules": [{"event": "ita_eth.5", "option_name": "Retreat",
            "fallback_index": 1}]});
        let step = run_once(&by_index, &game(None));
        assert!(
            step.starts_with("event ita_eth.5 option 1 (event_rules fallback_index"),
            "{step}"
        );
        let mut state = State::default();
        let unmatched = json!({"event_rules": [{"event": "ita_eth.5", "option_name": "Retreat"}],
            "event_options": {"ita_eth.5": 1}});
        let step = acted(tick(&mut state, &game(None), &unmatched, "l", START).unwrap());
        assert!(step.starts_with("event ita_eth.5 option 1: true"), "{step}");
        assert!(state.strategy.unknown["event_rules ita_eth.5"].contains("Retreat"));
    }

    #[test]
    fn option_names_ignore_case_markup_and_punctuation() {
        let options = [
            json!("Accept the £pol_power£ deal."),
            json!("Refuse!"),
            json!({"name": "ita.1.c", "text": "Wait"}),
        ];
        assert_eq!(option_index(&options, "accept the deal"), Some(0));
        assert_eq!(option_index(&options, "REFUSE"), Some(1));
        assert_eq!(option_index(&options, "ita.1.c"), Some(2));
        assert_eq!(option_index(&options, "deal"), Some(0));
        assert_eq!(option_index(&options, "e"), None, "ambiguous");
        assert_eq!(option_index(&options, ""), None);
    }

    #[test]
    fn the_run_digest_lists_unknown_atoms_and_the_phase_survives_a_restart() {
        let plan = json!({"phases": [{"id": "build", "exit": "bop:ETH_centralization > 0.5"},
                {"id": "war"}],
            "goal": {"predicates": ["vp_share:ETH < 0.1"], "deadline": "1937.01.01"}});
        let path = plan_dir(&plan);
        let now = hour("1936.03.01");
        let result = run(&mut State::default(), &campaign("p-1", now), &path, "l", 1).unwrap();
        let atoms: Vec<&str> = result["predicates_unknown"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|u| u["atom"].as_str())
            .collect();
        assert_eq!(
            atoms,
            ["bop:ETH_centralization > 0.5", "vp_share:ETH < 0.1"],
            "{result}"
        );
        assert_eq!(result["strategy"]["phase"], "build");
        assert_eq!(result["strategy"]["goal"]["status"], "open");
        assert!(alarm_codes(result["alarms"].as_array().unwrap())
            .contains(&"phase_changed".to_string()));
        // a restarted server takes the phase back from plan_state.json and does not announce it
        let result = run(
            &mut State::default(),
            &campaign("p-1", now + DAY),
            &path,
            "l",
            1,
        )
        .unwrap();
        assert_eq!(result["strategy"]["phase"], "build", "{result}");
        assert!(result.get("strategy_changes").is_none(), "{result}");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
