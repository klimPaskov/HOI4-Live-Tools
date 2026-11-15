//! Encirclement: the war step's operation that cuts enemy divisions off and destroys them. A
//! child module of `plan_run` (declared there with `#[path]`), like plan_war.rs and
//! plan_route.rs: every order still goes through `State::act` and the war step's own stage
//! machine; this module's memory (`State::encircle`) is this server life's.
//!
//! Why: a division cut off from supply loses organisation, cannot reinforce and is destroyed or
//! surrenders when its last province falls, while a frontal push only pushes it back. Pockets
//! are how a land war is won quickly and cheaply, for any country on any front.
//!
//! The operation, one at a time:
//!   find      every `scan_days` (2) without an operation the war step reads
//!             `hoi4_encirclement_candidates` for the enemies it is at war with: groups of
//!             enemy-held states that taking at most `max_gap` neck states would cut from the
//!             rest of the enemy, the two own states the pincers start from, the divisions each
//!             arm wants and a score (the pocket's value per effort). Visible ownership only.
//!   choose    the best-scoring pocket at or above `min_score` that the armies can close: each
//!             arm takes a driven army (the war step's own filter: not a guard, not reserve, in
//!             the theatre, at least `min_army` divisions) that stands within `reach` map units
//!             of the arm's own state, has `need_share` of the arm's divisions, its supply at
//!             `min_supply` or more, its equipment fill at `war.fill_floor` and is not resting;
//!             the read's nearest army first. A third fit army within `reach` of the pocket is
//!             the anvil, if one is free. `keep_free` driven armies stay out. A pocket with
//!             one usable arm only, or one with a sea side when sea escapes are not accepted, is
//!             passed over. A pocket already cut off needs one army, to reduce it.
//!   close     each arm's target is its neck state (a neck state already taken hands the arm the
//!             nearest one still open): the war step draws the front, the offensive line and
//!             waits for planning as for any attack, and an arm whose plan is ready waits up
//!             to `sync_days` (5) for the other so both pincers go in together. The anvil keeps
//!             a front against the pocket and does not advance: it pins the enemy inside.
//!   hold      when every neck state is held the ring is closed: the arms hold it with a front
//!             facing the nearest enemy-held state outside it (an executing plan stopped first,
//!             so they do not advance out of the ring). A neck state lost again sends both arms
//!             back in.
//!   reduce    the anvil attacks the pocket's states nearest first; without an anvil the arm
//!             with the most divisions does, and an arm with no enemy outside to face helps
//!             (with `reduce` false everyone holds until the pocket collapses on its own).
//!   resume    when no pocket state is enemy-held any more the operation ends and the armies go
//!             back to their axes and targets.
//!   give up   an operation not closed in `close_max_days`, an arm whose supply falls below the
//!             boldness' abort line, an arm army gone or shrunk below `min_army`, or a pocket
//!             the read no longer shows while its neck is open ends; that pocket is not tried
//!             again for `retry_days` (30).
//! Never: in a phase with `war.axes` [] (hold only), with `war.direct_moves`, or with
//! `war.encircle` false.
//!
//! Plan keys (all under `war.encircle`, all optional; `war.encircle: false` switches it off):
//!   boldness   cautious | measured (default) | bold: the defaults below in that order
//!     max_gap        1 | 2 | 3        neck states a pocket may need
//!     min_score      4 | 2 | 1        the read's score a pocket needs
//!     reach          100 | 160 | 260  map units from an army to its arm's own state
//!     need_share     1.0 | 0.75 | 0.5 of an arm's divisions_needed the army must have
//!     min_supply     max(80, war.supply_resume) | war.supply_resume | war.supply_floor
//!     abort_supply   war.supply_floor | war.supply_hold | none: an arm below it ends the
//!                    operation (the war step's rest rules apply to every arm in any case)
//!     close_max_days 20 | 30 | 45
//!     keep_free      1 | 0 | 0        driven armies never committed
//!     sea_escape     false | true | true: accept a pocket with a coast
//!   sync_days 5, reduce true, retry_days 30, scan_days 2, armies [names or army_layout
//!   name_hints that may be committed; empty: any driven army]
use super::{list, plan_war, text, GameHour, State, Tools, DAY};
use crate::error::Result;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// What the operation keeps (this server life).
#[derive(Default)]
pub(super) struct Memory {
    op: Option<Op>,
    /// game hour of the last candidate read without an operation
    scanned: Option<GameHour>,
    /// pocket key -> game hour until which it is not tried again
    shelved: HashMap<String, GameHour>,
    /// army -> game hour its execute order was first ready in this operation
    ready: HashMap<String, GameHour>,
    /// army -> the state its holding front was drawn against
    fronts: HashMap<String, i32>,
}

#[derive(Clone, Debug)]
struct Op {
    /// the pocket's states, sorted and joined: the pocket's identity across reads
    key: String,
    pocket: Vec<i32>,
    neck: Vec<i32>,
    arms: Vec<Arm>,
    /// the army that pins the pocket while the arms close it, then reduces it
    anvil: Option<String>,
    started: GameHour,
    /// when the ring closed (every neck state held)
    closed: Option<GameHour>,
    /// game hour of the last check that the read still shows the pocket
    checked: GameHour,
    /// state names for the notes
    names: HashMap<i32, String>,
}

#[derive(Clone, Debug)]
struct Arm {
    army: String,
    from: i32,
    into: i32,
}

/// What an army does for the operation this tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Role {
    /// attack this neck state
    Pincer(i32),
    /// hold the ring: a front against this enemy-held state, no advance
    Hold(i32),
    /// attack this pocket state
    Reduce(i32),
}

impl Role {
    /// The state the war step's battle plan goes for (none for a holding army).
    pub(super) fn target(self) -> Option<i32> {
        match self {
            Role::Pincer(t) | Role::Reduce(t) => Some(t),
            Role::Hold(_) => None,
        }
    }
}

/// A driven army the operation may use (the war step's own filter has passed).
pub(super) struct Cand {
    pub name: String,
    pub state: Option<i32>,
    pub divisions: u64,
    /// mean supply percent (hoi4_supply), when read
    pub supply: Option<f64>,
    /// mean equipment fill (0..1), when read
    pub fill: Option<f64>,
    /// below the rest line
    pub tired: bool,
}

/// What the war step knows this tick.
pub(super) struct Ctx<'a> {
    /// enemy tags at war with the player
    pub enemies: &'a [String],
    /// states an enemy controls
    pub held: &'a HashSet<i32>,
    pub armies: &'a [Cand],
    pub limits: &'a plan_war::Limits,
    /// `war.axes` [] in this phase: no attack on enemy land
    pub hold_only: bool,
    pub now: GameHour,
}

struct Cfg {
    on: bool,
    max_gap: u64,
    min_score: f64,
    reach: f64,
    need_share: f64,
    min_supply: f64,
    abort_supply: Option<f64>,
    close_days: i64,
    keep_free: usize,
    sea_escape: bool,
    sync_days: i64,
    reduce: bool,
    retry_days: i64,
    scan_days: i64,
    armies: Vec<String>,
}

impl Cfg {
    fn of(plan: &Value, limits: &plan_war::Limits) -> Self {
        let war = &plan["war"];
        let v = &war["encircle"];
        let on = war["direct_moves"] != json!(true)
            && match v {
                Value::Bool(b) => *b,
                Value::Object(_) => v["enabled"] != json!(false),
                _ => true,
            };
        let bold = v["boldness"].as_str().unwrap_or("measured");
        // (max_gap, min_score, reach, need_share, close days, keep_free, sea escape)
        let d = match bold {
            "cautious" => (1, 4.0, 100.0, 1.0, 20, 1, false),
            "bold" => (3, 1.0, 260.0, 0.5, 45, 0, true),
            _ => (2, 2.0, 160.0, 0.75, 30, 0, true),
        };
        let min_supply = match bold {
            "cautious" => limits.resume().max(80.0),
            "bold" => limits.floor(),
            _ => limits.resume(),
        };
        let abort_supply = match bold {
            "cautious" => Some(limits.floor()),
            "bold" => None,
            _ => Some(limits.hold()),
        };
        let num = |k: &str, d: f64| v[k].as_f64().filter(|x| *x >= 0.0).unwrap_or(d);
        Self {
            on,
            max_gap: v["max_gap"].as_u64().unwrap_or(d.0).min(3),
            min_score: num("min_score", d.1),
            reach: num("reach", d.2),
            need_share: num("need_share", d.3),
            min_supply: v["min_supply"].as_f64().map_or(min_supply, |p| {
                if p <= 1.0 {
                    p * 100.0
                } else {
                    p
                }
            }),
            abort_supply: match &v["abort_supply"] {
                Value::Null => abort_supply,
                Value::Bool(false) => None,
                x => x
                    .as_f64()
                    .map(|p| if p <= 1.0 { p * 100.0 } else { p })
                    .or(abort_supply),
            },
            close_days: num("close_max_days", f64::from(d.4)) as i64,
            keep_free: v["keep_free"].as_u64().map_or(d.5, |n| n as usize),
            sea_escape: v["sea_escape"].as_bool().unwrap_or(d.6),
            sync_days: num("sync_days", 5.0) as i64,
            reduce: v["reduce"] != json!(false),
            retry_days: num("retry_days", 30.0) as i64,
            scan_days: num("scan_days", 2.0) as i64,
            armies: list(v, "armies")
                .iter()
                .filter_map(|a| a.as_str().map(str::to_string))
                .collect(),
        }
    }
}

/// An army was renamed: what is kept for it follows the name.
pub(super) fn army_renamed(memory: &mut Memory, old: &str, new: &str) {
    if let Some(op) = memory.op.as_mut() {
        for arm in op.arms.iter_mut().filter(|a| a.army == old) {
            arm.army = new.to_string();
        }
    }
    if let Some(v) = memory.ready.remove(old) {
        memory.ready.insert(new.to_string(), v);
    }
    if let Some(v) = memory.fronts.remove(old) {
        memory.fronts.insert(new.to_string(), v);
    }
}

fn note(state: &mut State, line: String) {
    plan_war::set_note(state, "encircle", Some(line));
}

/// How the last operation ended (kept while the next one runs).
fn ended(state: &mut State, line: String) {
    plan_war::set_note(state, "encircle_last", Some(line.clone()));
    note(state, line);
}

fn ids(v: &Value, key: &str) -> Vec<i32> {
    let mut out: Vec<i32> = list(v, key)
        .iter()
        .filter_map(|s| s["state_id"].as_i64().map(|i| i as i32))
        .collect();
    out.sort_unstable();
    out
}

fn key_of(pocket: &[i32]) -> String {
    pocket
        .iter()
        .map(i32::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn distance(state: &State, a: i32, b: i32) -> Option<f64> {
    let (ax, az) = state.centers.get(&a).copied()?;
    let (bx, bz) = state.centers.get(&b).copied()?;
    Some(((ax - bx).powi(2) + (az - bz).powi(2)).sqrt())
}

/// The state of `from` nearest to `to`'s centre (unknown centres last).
fn nearest(state: &State, among: impl Iterator<Item = i32>, to: Option<i32>) -> Option<i32> {
    among.min_by(|a, b| {
        let d = |s: i32| to.and_then(|t| distance(state, s, t)).unwrap_or(f64::MAX);
        d(*a).total_cmp(&d(*b)).then(a.cmp(b))
    })
}

impl Op {
    fn label(&self, states: &[i32]) -> String {
        states
            .iter()
            .map(|s| match self.names.get(s) {
                Some(n) => format!("{n} ({s})"),
                None => s.to_string(),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
    fn arm_of(&self, army: &str) -> Option<&Arm> {
        self.arms.iter().find(|a| a.army == army)
    }
}

/// The operation's step for this tick: follow the running operation or look for a pocket, and
/// say what each committed army does. Armies without a role are driven as before.
pub(super) fn step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    ctx: &Ctx,
) -> Result<HashMap<String, Role>> {
    let cfg = Cfg::of(plan, ctx.limits);
    let now = ctx.now;
    if !cfg.on || ctx.hold_only {
        if let Some(op) = state.encircle.op.take() {
            let why = if ctx.hold_only {
                "the phase holds (war.axes [])"
            } else {
                "war.encircle is off"
            };
            end(state, &op);
            note(
                state,
                format!("operation on pocket {} ended: {why}", op.label(&op.pocket)),
            );
        }
        return Ok(HashMap::new());
    }
    state.encircle.shelved.retain(|_, until| *until > now);
    if state.encircle.op.is_some() {
        follow(state, tools, &cfg, ctx)?;
    }
    if state.encircle.op.is_none()
        && state
            .encircle
            .scanned
            .is_none_or(|at| now - at >= cfg.scan_days * DAY)
    {
        state.encircle.scanned = Some(now);
        scan(state, tools, &cfg, ctx)?;
    }
    Ok(roles(state, &cfg, ctx))
}

/// The candidate read for the enemies at war (tags only), or why it failed.
fn read(tools: &dyn Tools, cfg: &Cfg, enemies: &[String]) -> std::result::Result<Value, String> {
    let tags: Vec<&String> = enemies
        .iter()
        .filter(|t| {
            t.len() == 3
                && t.bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
        })
        .take(32)
        .collect();
    if tags.is_empty() {
        return Err("no enemy at war".into());
    }
    tools
        .call(
            "hoi4_encirclement_candidates",
            &json!({"enemy": tags, "max_gap": cfg.max_gap, "limit": 8}),
        )
        .map_err(|e| e.to_string())
}

/// Look for the best pocket the armies can close now, and start the operation on it.
fn scan(state: &mut State, tools: &dyn Tools, cfg: &Cfg, ctx: &Ctx) -> Result<()> {
    let read = match read(tools, cfg, ctx.enemies) {
        Ok(v) => v,
        Err(why) => {
            note(state, format!("no pocket read: {why}"));
            return Ok(());
        }
    };
    let allowed: HashSet<String> = cfg
        .armies
        .iter()
        .map(|a| state.layout.get(a).cloned().unwrap_or_else(|| a.clone()))
        .collect();
    let fit = |c: &Cand| {
        (allowed.is_empty() || allowed.contains(&c.name))
            && !c.tired
            && c.supply.is_none_or(|s| s >= cfg.min_supply)
            && !ctx.limits.underfilled(c.fill)
    };
    let usable = ctx.armies.iter().filter(|c| fit(c)).count();
    let mut first_why: Option<String> = None;
    for p in list(&read, "pockets") {
        let pocket = ids(p, "pocket_states");
        let neck = ids(p, "neck_states");
        let key = key_of(&pocket);
        let score = p["score"].as_f64().unwrap_or(0.0);
        let cut_off = p["already_cut_off"] == json!(true) || neck.is_empty();
        let names: HashMap<i32, String> = list(p, "pocket_states")
            .iter()
            .chain(list(p, "neck_states"))
            .filter_map(|s| Some((s["state_id"].as_i64()? as i32, text(&s["name"]).to_string())))
            .collect();
        let mut why = |w: String| {
            if first_why.is_none() {
                first_why = Some(format!("pocket {key}: {w}"));
            }
        };
        if pocket.is_empty() || !pocket.iter().any(|s| ctx.held.contains(s)) {
            continue;
        }
        if state.encircle.shelved.contains_key(&key) {
            why("given up lately".into());
            continue;
        }
        if p["gap"].as_u64().is_some_and(|g| g > cfg.max_gap) {
            why(format!(
                "{} neck states, more than max_gap {}",
                neck.len(),
                cfg.max_gap
            ));
            continue;
        }
        if !cut_off && score < cfg.min_score {
            why(format!("score {score} below {}", cfg.min_score));
            continue;
        }
        if !cut_off && p["pincer_ready"] != json!(true) {
            why("one own side only touches the neck".into());
            continue;
        }
        if !cut_off && !cfg.sea_escape && p["sea_escape_possible"] == json!(true) {
            why("it has a coast (sea_escape false)".into());
            continue;
        }
        // The arms: (own state, neck state, divisions needed, the read's army). A pocket already
        // cut off needs one army near it.
        let wanted: Vec<(i32, i32, f64, Option<String>)> = if cut_off {
            vec![(pocket[0], pocket[0], 0.0, None)]
        } else {
            list(p, "pincer_arms")
                .iter()
                .filter_map(|a| {
                    Some((
                        a["from_own_state"]["state_id"].as_i64()? as i32,
                        a["into_neck_state"]["state_id"].as_i64()? as i32,
                        a["divisions_needed"].as_f64().unwrap_or(0.0),
                        a["army_candidate"]["army"].as_str().map(str::to_string),
                    ))
                })
                .collect()
        };
        let mut arms: Vec<Arm> = Vec::new();
        let mut missing: Option<String> = None;
        for (from, into, need, suggested) in &wanted {
            let ok = |c: &&Cand| {
                fit(c)
                    && !arms.iter().any(|a| a.army == c.name)
                    && c.divisions as f64 >= (need * cfg.need_share).ceil()
                    && c.state.is_some_and(|s| {
                        cut_off
                            && pocket
                                .iter()
                                .any(|t| distance(state, s, *t).is_some_and(|d| d <= cfg.reach))
                            || distance(state, s, *from).is_some_and(|d| d <= cfg.reach)
                    })
            };
            let pick = ctx
                .armies
                .iter()
                .filter(ok)
                .find(|c| suggested.as_deref() == Some(c.name.as_str()))
                .or_else(|| {
                    ctx.armies.iter().filter(ok).min_by(|a, b| {
                        let d = |c: &Cand| {
                            c.state
                                .and_then(|s| distance(state, s, *from))
                                .unwrap_or(f64::MAX)
                        };
                        d(a).total_cmp(&d(b)).then(a.name.cmp(&b.name))
                    })
                });
            match pick {
                Some(c) => arms.push(Arm {
                    army: c.name.clone(),
                    from: *from,
                    into: *into,
                }),
                None => {
                    missing = Some(format!(
                        "no fit army within {} of state {from} for the arm into {into} ({} divisions, supply {}%+)",
                        cfg.reach,
                        (need * cfg.need_share).ceil(),
                        cfg.min_supply
                    ));
                    break;
                }
            }
        }
        if let Some(m) = missing {
            why(m);
            continue;
        }
        if usable.saturating_sub(arms.len()) < cfg.keep_free {
            why(format!(
                "{} armies would leave fewer than keep_free {} driven",
                arms.len(),
                cfg.keep_free
            ));
            continue;
        }
        // The anvil: the fit army nearest the pocket that is not an arm, if one may be spared.
        let anvil = (!cut_off && usable.saturating_sub(arms.len() + 1) >= cfg.keep_free)
            .then(|| {
                ctx.armies
                    .iter()
                    .filter(|c| fit(c) && !arms.iter().any(|a| a.army == c.name))
                    .filter_map(|c| {
                        let d = pocket
                            .iter()
                            .filter_map(|t| distance(state, c.state?, *t))
                            .min_by(f64::total_cmp)?;
                        (d <= cfg.reach).then_some((c.name.clone(), d))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
                    .map(|(n, _)| n)
            })
            .flatten();
        let op = Op {
            key,
            pocket,
            neck,
            arms,
            anvil,
            started: ctx.now,
            closed: cut_off.then_some(ctx.now),
            checked: ctx.now,
            names,
        };
        let arms_text = op
            .arms
            .iter()
            .map(|a| {
                if cut_off {
                    a.army.clone()
                } else {
                    format!("{} from {} into {}", a.army, a.from, op.label(&[a.into]))
                }
            })
            .collect::<Vec<_>>()
            .join("; ")
            + &op
                .anvil
                .as_ref()
                .map(|a| format!("; anvil {a}"))
                .unwrap_or_default();
        note(
            state,
            if cut_off {
                format!(
                    "reducing pocket {} (already cut off, score {score}): {arms_text}",
                    op.label(&op.pocket)
                )
            } else {
                format!(
                    "closing pocket {} (score {score}) at the neck {}: {arms_text}",
                    op.label(&op.pocket),
                    op.label(&op.neck)
                )
            },
        );
        state.encircle.ready.clear();
        state.encircle.op = Some(op);
        return Ok(());
    }
    note(
        state,
        match first_why {
            Some(w) => format!("no pocket taken: {w}"),
            None => "no pocket on the front".into(),
        },
    );
    Ok(())
}

/// Release the operation's armies: the war step picks their targets again.
fn end(state: &mut State, op: &Op) {
    for army in op.arms.iter().map(|a| &a.army).chain(op.anvil.iter()) {
        state.war_assign.remove(army);
        state.encircle.ready.remove(army);
        state.encircle.fronts.remove(army);
    }
}

fn give_up(state: &mut State, op: Op, cfg: &Cfg, now: GameHour, why: String) {
    end(state, &op);
    state
        .encircle
        .shelved
        .insert(op.key.clone(), now + cfg.retry_days * DAY);
    ended(
        state,
        format!(
            "gave up pocket {}: {why}; not tried again for {} days",
            op.label(&op.pocket),
            cfg.retry_days
        ),
    );
}

/// Follow the running operation: closed, broken, reduced, or given up.
fn follow(state: &mut State, tools: &dyn Tools, cfg: &Cfg, ctx: &Ctx) -> Result<()> {
    let Some(mut op) = state.encircle.op.take() else {
        return Ok(());
    };
    let now = ctx.now;
    let left: Vec<i32> = op
        .pocket
        .iter()
        .copied()
        .filter(|s| ctx.held.contains(s))
        .collect();
    if left.is_empty() {
        end(state, &op);
        ended(
            state,
            format!(
                "pocket {} reduced in {} days{}; the armies resume the advance",
                op.label(&op.pocket),
                (now - op.started) / DAY,
                op.closed
                    .map(|c| format!(" (ring closed after {} days)", (c - op.started) / DAY))
                    .unwrap_or_default()
            ),
        );
        return Ok(());
    }
    let by_name: HashMap<&str, &Cand> = ctx.armies.iter().map(|c| (c.name.as_str(), c)).collect();
    if let Some(gone) = op
        .arms
        .iter()
        .find(|a| !by_name.contains_key(a.army.as_str()))
    {
        let why = format!("{} is no longer a driven army of the theatre", gone.army);
        give_up(state, op, cfg, now, why);
        return Ok(());
    }
    if op
        .anvil
        .as_ref()
        .is_some_and(|a| !by_name.contains_key(a.as_str()))
    {
        op.anvil = None;
    }
    let open_neck: Vec<i32> = op
        .neck
        .iter()
        .copied()
        .filter(|s| ctx.held.contains(s))
        .collect();
    match op.closed {
        None if open_neck.is_empty() => {
            op.closed = Some(now);
            note(
                state,
                format!(
                    "ring closed around {} after {} days: {}",
                    op.label(&left),
                    (now - op.started) / DAY,
                    if cfg.reduce {
                        "reducing it"
                    } else {
                        "holding it"
                    }
                ),
            );
        }
        None => {
            if now - op.started > cfg.close_days * DAY {
                let why = format!("the neck was not closed in {} days", cfg.close_days);
                give_up(state, op, cfg, now, why);
                return Ok(());
            }
            if let Some(line) = cfg.abort_supply {
                if let Some(c) = op.arms.iter().find_map(|a| {
                    by_name
                        .get(a.army.as_str())
                        .filter(|c| c.supply.is_some_and(|s| s < line))
                }) {
                    let why = format!(
                        "{} is at {:.0}% supply, below {line:.0}%",
                        c.name,
                        c.supply.unwrap_or_default()
                    );
                    give_up(state, op, cfg, now, why);
                    return Ok(());
                }
            }
            if now - op.checked >= DAY {
                op.checked = now;
                if let Ok(read) = read(tools, cfg, ctx.enemies) {
                    let still = list(&read, "pockets").iter().any(|p| {
                        let pocket = ids(p, "pocket_states");
                        !pocket.is_empty() && pocket.iter().all(|s| op.pocket.contains(s))
                            || ids(p, "neck_states") == op.neck
                    });
                    if !still {
                        give_up(
                            state,
                            op,
                            cfg,
                            now,
                            "the read no longer shows the pocket (the enemy has another way out)"
                                .into(),
                        );
                        return Ok(());
                    }
                }
            }
        }
        Some(_) if !open_neck.is_empty() => {
            op.closed = None;
            op.started = now;
            state.encircle.ready.clear();
            note(
                state,
                format!(
                    "ring around {} broken at {}: the arms retake the neck",
                    op.label(&left),
                    op.label(&open_neck)
                ),
            );
        }
        Some(_) => {}
    }
    state.encircle.op = Some(op);
    Ok(())
}

/// Each committed army's role this tick.
fn roles(state: &State, cfg: &Cfg, ctx: &Ctx) -> HashMap<String, Role> {
    let mut out = HashMap::new();
    let Some(op) = state.encircle.op.as_ref() else {
        return out;
    };
    let here = |army: &str| {
        ctx.armies
            .iter()
            .find(|c| c.name == army)
            .and_then(|c| c.state)
    };
    let left: Vec<i32> = op
        .pocket
        .iter()
        .copied()
        .filter(|s| ctx.held.contains(s))
        .collect();
    if let Some(anvil) = &op.anvil {
        let pos = here(anvil);
        if let Some(t) = nearest(state, left.iter().copied(), pos) {
            out.insert(
                anvil.clone(),
                if op.closed.is_some() && cfg.reduce {
                    Role::Reduce(t)
                } else {
                    Role::Hold(t)
                },
            );
        }
    }
    if op.closed.is_none() {
        for arm in &op.arms {
            let t = if ctx.held.contains(&arm.into) {
                Some(arm.into)
            } else {
                nearest(
                    state,
                    op.neck.iter().copied().filter(|s| ctx.held.contains(s)),
                    here(&arm.army).or(Some(arm.into)),
                )
            };
            if let Some(t) = t {
                out.insert(arm.army.clone(), Role::Pincer(t));
            }
        }
        return out;
    }
    // Without an anvil the arm with the most divisions reduces; the others hold the ring.
    let reducer = (cfg.reduce && op.anvil.is_none())
        .then(|| {
            op.arms
                .iter()
                .max_by_key(|a| {
                    ctx.armies
                        .iter()
                        .find(|c| c.name == a.army)
                        .map_or(0, |c| c.divisions)
                })
                .map(|a| a.army.clone())
        })
        .flatten();
    for arm in &op.arms {
        let pos = here(&arm.army);
        let reduce_target = nearest(state, left.iter().copied(), pos.or(Some(arm.into)));
        if reducer.as_deref() == Some(arm.army.as_str()) {
            if let Some(t) = reduce_target {
                out.insert(arm.army.clone(), Role::Reduce(t));
            }
            continue;
        }
        // Face the nearest enemy-held state outside the pocket from the neck the arm closed.
        let outside = nearest(
            state,
            ctx.held
                .iter()
                .copied()
                .filter(|s| !op.pocket.contains(s) && !op.neck.contains(s)),
            Some(arm.into),
        )
        .filter(|s| distance(state, *s, arm.into).is_some_and(|d| d <= cfg.reach));
        match (outside, reduce_target) {
            (Some(f), _) => {
                out.insert(arm.army.clone(), Role::Hold(f));
            }
            // Nothing outside to hold against: help reduce.
            (None, Some(t)) if cfg.reduce => {
                out.insert(arm.army.clone(), Role::Reduce(t));
            }
            _ => {}
        }
    }
    out
}

/// Whether the running operation uses this army (an arm or the anvil).
pub(super) fn committed(state: &State, army: &str) -> bool {
    state
        .encircle
        .op
        .as_ref()
        .is_some_and(|op| op.arm_of(army).is_some() || op.anvil.as_deref() == Some(army))
}

/// A pincer arm's execute order waits for the other arm (both plans go in together), at most
/// `sync_days` from the game hour it was first ready. Some(why) holds the order this tick.
pub(super) fn launch_wait(
    state: &mut State,
    plan: &Value,
    army: &str,
    roster: &Value,
    limits: &plan_war::Limits,
    now: GameHour,
) -> Option<String> {
    let op = state.encircle.op.as_ref()?;
    if op.closed.is_some() || op.arm_of(army).is_none() {
        return None;
    }
    let sync = Cfg::of(plan, limits).sync_days;
    let others: Vec<String> = op
        .arms
        .iter()
        .filter(|a| a.army != army)
        .map(|a| a.army.clone())
        .collect();
    let first = *state.encircle.ready.entry(army.to_string()).or_insert(now);
    let executing = |n: &str| {
        list(roster, "armies")
            .iter()
            .any(|a| text(&a["army"]) == n && a["plan_executing"] == json!(true))
    };
    let waiting: Vec<String> = others
        .into_iter()
        .filter(|o| !state.encircle.ready.contains_key(o) && !executing(o))
        .collect();
    if waiting.is_empty() || now - first >= sync * DAY {
        return None;
    }
    let why = format!(
        "{army} is ready and waits for {} so both pincers go in together ({} of {sync} days)",
        waiting.join(", "),
        (now - first) / DAY
    );
    plan_war::set_note(state, &format!("encircle:{army}"), Some(why.clone()));
    Some(why)
}

/// A holding army: a front against `facing` once, its executing plan stopped first.
/// Some((ok, why, line)) when an order was tried (`why` is the action's own answer).
pub(super) fn hold(
    state: &mut State,
    tools: &dyn Tools,
    lease: &str,
    army: &Value,
    facing: i32,
) -> Result<Option<(bool, String, String)>> {
    let name = text(&army["army"]).to_string();
    if army["plan_executing"] == json!(true) {
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_army_plan",
            json!({"army": name, "execute": false}),
            "we",
        )?;
        let line = format!("encircle hold {name}: plan stopped to hold the ring: {ok} {why}");
        return Ok(Some((ok, why, line)));
    }
    if state.encircle.fronts.get(&name) == Some(&facing)
        && army["battle_plan_orders"].as_u64().unwrap_or(0) > 0
    {
        return Ok(None);
    }
    let (ok, why) = state.act(
        tools,
        lease,
        "hoi4_army_front",
        json!({"army": name, "state_id": facing}),
        "we",
    )?;
    if ok {
        state.encircle.fronts.insert(name.clone(), facing);
    }
    let line =
        format!("encircle hold {name}: front against state {facing} around the pocket: {ok} {why}");
    Ok(Some((ok, why, line)))
}

#[cfg(test)]
#[path = "plan_encircle/tests.rs"]
mod tests;
