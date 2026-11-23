//! Plan predicates: the checks a plan writes for its goal, phases, checkpoints, contingencies,
//! gated laws (`politics[].when`), guarded decisions (`decision_rules[].when`) and event rules
//! (`event_rules[].when`). A predicate is parsed from the plan JSON and evaluated over the
//! ordinary read tools only (never input, never a pause) with three-valued logic: an atom the
//! server cannot read right now is `Unknown`, never true, and whoever asked waits.
//!
//! Grammar (one predicate):
//!   "atom op value"        op is one of < <= > >= == != (spaces optional): "stability < 0.5"
//!   "atom"                 a boolean atom: "at_war_with:ETH", "focus_done:ITA_paris"
//!   "YYYY.MM.DD"           a date (also "date:YYYY.MM.DD" or "date >= YYYY.MM.DD"): reached
//!   "" or true / false     always / always / never
//!   {"all":[..]} {"any":[..]} {"not":p}   combinations; a plain list [..] means all
//!   Kept for older plans: "owner:<state>=<TAG>", "state_controller:<s>=<TAG>",
//!   "province_controller:<p>=<TAG>" (a single = is ==), "vp_share:<TAG><=0.5", "at_war:<TAG>",
//!   "not_at_war:<TAG>", "not_exists:<TAG>", "focus_completed:<key>", "tech_researched:<key>".
//!   Ratio atoms (stability, war_support, world_tension, fuel_ratio, vp_share, equipment_fill,
//!   resistance, compliance, surrender_progress, naval_dominance, naval_intel) are fractions 0..1; a value above 1 or written with % is a
//!   percentage ("war_support >= 25" is 0.25). Truth: all is false when one part is false,
//!   unknown when one part is unknown and none false; any is the mirror; not keeps unknown.
//!
//! Atoms, what reads them, and whether a server can read them today:
//!   date                        game hour (the caller's clock)                      readable
//!   stability, war_support      hoi4_country_status fields.<name>.value (top-bar text,
//!   manpower                    validated:false)                                     readable
//!   pp                          hoi4_economy political_power.raw_signed / 100000,
//!                               else hoi4_country_status political_power              readable
//!   world_tension               hoi4_diplomacy_state world_tension.value, else
//!                               hoi4_country_status world_tension                     readable
//!   xp:<army|air|navy>          hoi4_doctrines experience.<kind>.value (land=army, naval=navy)
//!   fuel_ratio                  hoi4_resources fuel.ratio (stock / capacity)          readable
//!   resistance:<state>          hoi4_occupation states[].resistance / 100 (a state that
//!   compliance:<state>          has no resistance block is not listed: unknown)      readable
//!   equipment_fill:<army>       hoi4_supply: mean over the army's divisions of their lowest
//!                               equipment fill (else the army row's equipment_fill_min)
//!   at_peace_conference         hoi4_events peace_conference_open                    readable
//!   at_war_with:<TAG>           hoi4_wars wars.enemies, else hoi4_diplomacy_state
//!   vp_share:<TAG>              1 - hoi4_states{query:TAG} victory_points.share_held_by_others
//!                               (share of TAG's own victory points TAG still holds)  readable
//!   controls_state:<id>         hoi4_states{query:id} controller == player           readable
//!   state_fully_controlled:<id> every province of hoi4_states{query:id} held by the player
//!   owner:<id>, controller:<id> that state's owner / controller tag (== TAG)          readable
//!   state_controller:<id>       the one tag holding every province (== TAG)          readable
//!   province_controlled:<p>     the province's controller is the player; found in the
//!   province_controller:<p>     victory points and provinces_held_by_others of the own and
//!                               the enemies' hoi4_states reads, else in its own state's read
//!                               (hoi4_province_graph names the state; hoi4_states{query:id})
//!   own_surrender_progress      the share of the player's own victory points others hold (the
//!                               game's surrender progress: the country capitulates when it
//!                               reaches its surrender limit), hoi4_states{query:<own tag>}
//!                               victory_points.share_held_by_others                  readable
//!   factories:<TAG>             civilian + military + naval factories: the player's from
//!                               hoi4_economy factory_trigger_count (live); another country's an
//!                               estimate, hoi4_states{query:TAG} factories_start.controlled (the
//!                               game files' factories of the states it controls now)  readable
//!   industry_ratio:<TAG>        factories:<TAG> / the player's factories (the game's ic_ratio
//!                               gate of war laws, e.g. "an enemy above 40% of yours"; a plain
//!                               number, 1 is equal industry)                         readable
//!   casualty_ratio:<TAG>        men TAG lost in its war relation with the player over the
//!                               men the player lost in it (hoi4_war_stats); unknown until
//!                               both sides have losses                               readable
//!   exists:<TAG>                TAG owns a state (hoi4_states{query:TAG})            readable
//!   focus_done:<key>            hoi4_focus_completed completed (all pages)           readable
//!   tech_done:<key>             hoi4_research researched[].id (all pages)            readable
//!   phase:<id>                  the plan's active phase (`Ctx::phase`)               readable
//!   convoys_free                hoi4_convoys convoys.free (stock minus in use, the top-bar
//!                               figure), else hoi4_country_status convoys.free     readable
//!   stockpile_days:<equipment>  hoi4_stockpile{equipment} days_of_cover of the archetype row
//!                               (an archetype or a variant key); a stock that is not falling
//!                               (depleting false) counts as 9999 days; unknown until the server
//!                               has measured a trend (net_per_day null)            readable
//!   var:<name>                  unknown: no tool reads scenario variables (audit W1-18)
//!   bop:<id>                    hoi4_country_status balances_of_power[].value of the balance
//!                               whose id is <id> (-1..1); an inactive balance is unknown
//!   mission_days:<id>           unknown: hoi4_decisions_list has no mission timers (W1-18)
//!   naval_dominance:<region>    hoi4_fleets sea_regions[] row of strategic sea region <region id>,
//!                               field own_dominance (0..1: the player's and friends' share of the
//!                               region's naval dominance; the region is controlled at
//!                               NDefines.NNavy.DOMINANCE_CONTROLLED_THRESHOLD_RATIO, 0.6)
//!   naval_intel:<TAG>           hoi4_wars wars.wars[] row of the war with TAG, field own_naval_intel
//!                               (0..1, the navy figure of TAG's intel ledger; under
//!                               NIntel.NAVAL_DOMINANCE_INTEL_LOW, 0.4, dominance against TAG is
//!                               scaled down). Both unknown today: the tools report the fields null
//!                               with a reason until they are located in memory (gap register
//!                               navy.dominance_intel_read)
//!   capitulated:<TAG>           hoi4_wars + hoi4_events + hoi4_states: TAG owns and controls no
//!                               state (annexed or in exile, even while the game keeps the war), or
//!                               the war is over and a peace conference is open or TAG owns no state
//!   surrender_progress:<TAG>    hoi4_wars wars.wars[] row of the war with TAG, field
//!                               enemy_surrender_progress (0..1: the share of TAG's surrender limit
//!                               reached, the bar under TAG's flag in the war overview; 1 is the
//!                               capitulation). Unknown today: the value is not located in memory
//!                               yet and the tool reports null with enemy_surrender_progress_reason
//!                               (gap register peace.surrender_progress_read); unknown as well when
//!                               the player is not at war with TAG (no war overview row for it)
//!   anything else               unknown atom (reported, never true)
//!
//! API (plan_run and plan_manage):
//!   `parse(&Value) -> Result<Predicate>`   a malformed predicate is `invalid_predicate`
//!   `holds(&Predicate, &mut Ctx) -> Truth` evaluates; an unreadable atom is recorded in the
//!                                          context with its reason and is `Truth::Unknown`
//!   `check(&Value, &mut Ctx) -> Truth`     parse + holds; a parse error is Unknown and recorded
//!   `Ctx::new(&dyn Tools, now)`            one context per tick: every read (tool + arguments)
//!                                          happens at most once in its life, a failed read
//!                                          included; set `ctx.phase` to the active phase
//!   `Ctx::with_cache` / `into_cache`        carry those reads (`Cache`) from one context to
//!                                          the next within a tick (plan_run keeps one per tick
//!                                          in its State; `plan_run::judge` is the usual entry)
//!   `Ctx::read(name, &args)`               the cached read itself (for other rules)
//!   `Ctx::unknown()` / `take_unknown()`    atom text -> why it was unknown, for the digest
use crate::error::{Error, Result};
use crate::plan_run::{GameHour, Tools};
use serde_json::{json, Value};
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Three-valued truth of a predicate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Truth {
    True,
    False,
    /// Something it needs cannot be read now: never act on it.
    Unknown,
}
impl Truth {
    pub fn is_true(self) -> bool {
        self == Truth::True
    }
    pub fn from_bool(value: bool) -> Self {
        if value {
            Truth::True
        } else {
            Truth::False
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Truth::True => "true",
            Truth::False => "false",
            Truth::Unknown => "unknown",
        }
    }
}
impl std::ops::Not for Truth {
    type Output = Truth;
    fn not(self) -> Truth {
        match self {
            Truth::True => Truth::False,
            Truth::False => Truth::True,
            Truth::Unknown => Truth::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}
impl Op {
    fn apply(self, order: Ordering) -> bool {
        match self {
            Op::Lt => order == Ordering::Less,
            Op::Le => order != Ordering::Greater,
            Op::Gt => order == Ordering::Greater,
            Op::Ge => order != Ordering::Less,
            Op::Eq => order == Ordering::Equal,
            Op::Ne => order != Ordering::Equal,
        }
    }
}

/// The right-hand side of a comparison, typed by the atom.
#[derive(Clone, Debug, PartialEq)]
pub enum Operand {
    Number(f64),
    Bool(bool),
    Text(String),
    Hour(GameHour),
}

/// One check on the game state.
#[derive(Clone, Debug, PartialEq)]
pub struct Atom {
    /// The text as the plan wrote it (the key of an unknown report).
    pub text: String,
    /// Canonical atom name ("focus_done" for "focus_completed").
    pub name: String,
    /// What follows the first ':' ("army" in "xp:army").
    pub arg: Option<String>,
    /// None for a bare boolean atom (it must be true).
    pub cmp: Option<(Op, Operand)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Predicate {
    Const(bool),
    Atom(Atom),
    All(Vec<Predicate>),
    Any(Vec<Predicate>),
    Not(Box<Predicate>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// a fraction 0..1 (a value above 1 or with % is a percentage)
    Ratio,
    Number,
    Flag,
    /// a country tag compared with == or !=
    Tag,
    Date,
    Unknown,
}

/// (name, kind, needs an argument after ':')
const ATOMS: &[(&str, Kind, bool)] = &[
    ("date", Kind::Date, false),
    ("stability", Kind::Ratio, false),
    ("war_support", Kind::Ratio, false),
    ("world_tension", Kind::Ratio, false),
    ("fuel_ratio", Kind::Ratio, false),
    ("vp_share", Kind::Ratio, true),
    ("own_surrender_progress", Kind::Ratio, false),
    ("surrender_progress", Kind::Ratio, true),
    ("factories", Kind::Number, true),
    ("industry_ratio", Kind::Number, true),
    ("casualty_ratio", Kind::Number, true),
    ("equipment_fill", Kind::Ratio, true),
    ("resistance", Kind::Ratio, true),
    ("compliance", Kind::Ratio, true),
    ("pp", Kind::Number, false),
    ("manpower", Kind::Number, false),
    ("convoys_free", Kind::Number, false),
    ("xp", Kind::Number, true),
    ("stockpile_days", Kind::Number, true),
    ("var", Kind::Number, true),
    ("bop", Kind::Number, true),
    ("mission_days", Kind::Number, true),
    ("naval_dominance", Kind::Ratio, true),
    ("naval_intel", Kind::Ratio, true),
    ("at_peace_conference", Kind::Flag, false),
    ("at_war_with", Kind::Flag, true),
    ("controls_state", Kind::Flag, true),
    ("state_fully_controlled", Kind::Flag, true),
    ("province_controlled", Kind::Flag, true),
    ("focus_done", Kind::Flag, true),
    ("tech_done", Kind::Flag, true),
    ("phase", Kind::Flag, true),
    ("exists", Kind::Flag, true),
    ("capitulated", Kind::Flag, true),
    ("owner", Kind::Tag, true),
    ("controller", Kind::Tag, true),
    ("state_controller", Kind::Tag, true),
    ("province_controller", Kind::Tag, true),
];

/// Whether `tag` still holds our border in a hoi4_wars read: its divisions stand in its states
/// on our border (`neighbour_divisions_in_border_states` > 0), or, when that count is not read,
/// it holds any state touching ours. Pure.
pub(crate) fn border_held(wars: &Value, tag: &str) -> bool {
    wars["borders"].as_array().is_some_and(|bs| {
        bs.iter().any(|b| {
            b["neighbour_tag"].as_str() == Some(tag)
                && b["neighbour_states"]
                    .as_array()
                    .is_some_and(|n| !n.is_empty())
                && b["neighbour_divisions_in_border_states"]
                    .as_i64()
                    .is_none_or(|n| n > 0)
        })
    })
}

/// An enemy that still controls only a remnant: at most REMNANT_SHARE of its victory points (a
/// colony or two across the sea). Its war is over in practice and the next declaration may go
/// (w42s3, 23 Nov 2026: the Netherlands held Suriname and Curacao, 2 of 91 victory points, for 24
/// days after Holland fell; the Belgian declaration waited on capitulated:HOL that never read true).
const REMNANT_SHARE: f64 = 0.05;

pub(crate) fn remnant(rows: &[Value], tag: &str) -> bool {
    let (mut held, mut total) = (0.0, 0.0);
    for v in rows
        .iter()
        .filter_map(|r| r["victory_points"].as_array())
        .flatten()
    {
        let points = v["victory_points"].as_f64().unwrap_or(0.0);
        total += points;
        if v["controller"].as_str() == Some(tag) {
            held += points;
        }
    }
    total > 0.0 && held <= total * REMNANT_SHARE
}

fn bad(message: impl Into<String>) -> Error {
    Error::new("invalid_predicate", message)
}

/// The game hour at 00:00 of "YYYY.MM.DD" (dots or dashes; a trailing hour field is ignored),
/// on the game's 365-day calendar counted from year -5000 (as `hoi4_overview` date.raw).
pub fn date_hour(text: &str) -> Option<GameHour> {
    let parts: Vec<&str> = text.trim().split(['.', '-']).collect();
    if !(3..=4).contains(&parts.len())
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let (y, m, d): (i64, i64, i64) = (
        parts[0].parse().ok()?,
        parts[1].parse().ok()?,
        parts[2].parse().ok()?,
    );
    const DAYS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    if parts[0].len() != 4 || !(1..=12).contains(&m) || !(1..=DAYS[(m - 1) as usize]).contains(&d) {
        return None;
    }
    let before: i64 = DAYS[..(m - 1) as usize].iter().sum();
    Some(((y + 5000) * 365 + before + d - 1) * 24)
}

/// Parse a predicate from the plan (see the module docs for the grammar).
pub fn parse(value: &Value) -> Result<Predicate> {
    match value {
        Value::Bool(b) => Ok(Predicate::Const(*b)),
        Value::String(text) => parse_text(text),
        Value::Array(items) => Ok(Predicate::All(
            items.iter().map(parse).collect::<Result<_>>()?,
        )),
        Value::Object(map) if map.len() == 1 => {
            let Some((key, inner)) = map.iter().next() else {
                return Err(bad("empty predicate object"));
            };
            match key.as_str() {
                "all" | "any" => {
                    let items = inner
                        .as_array()
                        .ok_or_else(|| bad(format!("{key} needs a list of predicates")))?;
                    let parts = items.iter().map(parse).collect::<Result<Vec<_>>>()?;
                    Ok(if key == "all" {
                        Predicate::All(parts)
                    } else {
                        Predicate::Any(parts)
                    })
                }
                "not" => Ok(Predicate::Not(Box::new(parse(inner)?))),
                _ => Err(bad(format!(
                    "unknown combination '{key}' (all, any or not)"
                ))),
            }
        }
        _ => Err(bad(format!(
            "a predicate is a string, a list or {{all|any|not}}, not {value}"
        ))),
    }
}

/// Split "left op value" at the first comparison character.
fn split_comparison(s: &str) -> Result<(&str, Option<(Op, &str)>)> {
    let Some(i) = s.find(['<', '>', '=', '!']) else {
        return Ok((s, None));
    };
    let rest = &s[i..];
    let (op, len) = if rest.starts_with("<=") {
        (Op::Le, 2)
    } else if rest.starts_with(">=") {
        (Op::Ge, 2)
    } else if rest.starts_with("==") {
        (Op::Eq, 2)
    } else if rest.starts_with("!=") {
        (Op::Ne, 2)
    } else if rest.starts_with('<') {
        (Op::Lt, 1)
    } else if rest.starts_with('>') {
        (Op::Gt, 1)
    } else if rest.starts_with('=') {
        (Op::Eq, 1)
    } else {
        return Err(bad(format!("'{s}': '!' must be '!='")));
    };
    let value = rest[len..].trim();
    if value.is_empty() {
        return Err(bad(format!("'{s}': a comparison needs a value")));
    }
    Ok((&s[..i], Some((op, value))))
}

fn number(text: &str, ratio: bool, whole: &str) -> Result<f64> {
    let (digits, percent) = match text.strip_suffix('%') {
        Some(d) => (d.trim(), true),
        None => (text, false),
    };
    let value: f64 = digits
        .parse()
        .ok()
        .filter(|v: &f64| v.is_finite())
        .ok_or_else(|| bad(format!("'{whole}': '{text}' is not a number")))?;
    if percent && !ratio {
        return Err(bad(format!("'{whole}': this atom is not a percentage")));
    }
    Ok(if ratio && (percent || value > 1.0) {
        value / 100.0
    } else {
        value
    })
}

fn boolean(text: &str, whole: &str) -> Result<bool> {
    match text.to_ascii_lowercase().as_str() {
        "true" | "yes" | "1" => Ok(true),
        "false" | "no" | "0" => Ok(false),
        _ => Err(bad(format!("'{whole}': '{text}' is not true or false"))),
    }
}

fn parse_text(raw: &str) -> Result<Predicate> {
    let s = raw.trim();
    match s {
        "" | "true" => return Ok(Predicate::Const(true)),
        "false" => return Ok(Predicate::Const(false)),
        _ => {}
    }
    let (left, cmp) = split_comparison(s)?;
    let left = left.trim();
    let atom = |name: &str, arg: Option<String>, cmp: Option<(Op, Operand)>| {
        Predicate::Atom(Atom {
            text: s.to_string(),
            name: name.to_string(),
            arg,
            cmp,
        })
    };
    if cmp.is_none() {
        if let Some(hour) = date_hour(left) {
            return Ok(atom("date", None, Some((Op::Ge, Operand::Hour(hour)))));
        }
    }
    let (name, arg) = match left.split_once(':') {
        Some((n, a)) => (
            n.trim(),
            Some(a.trim().to_string()).filter(|a| !a.is_empty()),
        ),
        None => (left, None),
    };
    if name.is_empty() {
        return Err(bad(format!("'{s}': no atom before the comparison")));
    }
    let (name, negate) = match name {
        "focus_completed" => ("focus_done", false),
        "tech_researched" => ("tech_done", false),
        "at_war" => ("at_war_with", false),
        "not_at_war" => ("at_war_with", true),
        "not_exists" => ("exists", true),
        other => (other, false),
    };
    let (kind, needs_arg) = ATOMS
        .iter()
        .find(|(n, _, _)| *n == name)
        .map_or((Kind::Unknown, false), |(_, k, a)| (*k, *a));
    if needs_arg && arg.is_none() {
        return Err(bad(format!("'{s}': {name} needs ':<argument>'")));
    }
    let typed = match (kind, cmp) {
        (Kind::Date, None) => {
            let hour = arg
                .as_deref()
                .and_then(date_hour)
                .ok_or_else(|| bad(format!("'{s}': date needs YYYY.MM.DD")))?;
            return Ok(atom("date", None, Some((Op::Ge, Operand::Hour(hour)))));
        }
        (Kind::Date, Some((op, v))) => Some((
            op,
            Operand::Hour(
                date_hour(v).ok_or_else(|| bad(format!("'{s}': '{v}' is not YYYY.MM.DD")))?,
            ),
        )),
        (Kind::Ratio | Kind::Number, None) => {
            return Err(bad(format!(
                "'{s}': {name} needs a comparison such as '{name} < 0.5'"
            )))
        }
        (Kind::Ratio, Some((op, v))) => Some((op, Operand::Number(number(v, true, s)?))),
        (Kind::Number, Some((op, v))) => Some((op, Operand::Number(number(v, false, s)?))),
        (Kind::Flag, None) => None,
        (Kind::Flag, Some((op @ (Op::Eq | Op::Ne), v))) => {
            Some((op, Operand::Bool(boolean(v, s)?)))
        }
        (Kind::Tag, Some((op @ (Op::Eq | Op::Ne), v))) => Some((op, Operand::Text(v.to_string()))),
        (Kind::Flag | Kind::Tag, Some(_)) => {
            return Err(bad(format!("'{s}': {name} compares only with == or !=")))
        }
        (Kind::Tag, None) => return Err(bad(format!("'{s}': {name} needs '== <TAG>'"))),
        (Kind::Unknown, cmp) => cmp.map(|(op, v)| {
            let operand = v
                .parse::<f64>()
                .ok()
                .filter(|n| n.is_finite())
                .map(Operand::Number)
                .or_else(|| boolean(v, s).ok().map(Operand::Bool))
                .unwrap_or_else(|| Operand::Text(v.to_string()));
            (op, operand)
        }),
    };
    let parsed = atom(name, arg, typed);
    Ok(if negate {
        Predicate::Not(Box::new(parsed))
    } else {
        parsed
    })
}

/// What an atom reads.
#[derive(Clone, Debug, PartialEq)]
enum Reading {
    Number(f64),
    Flag(bool),
    Text(String),
    Hour(GameHour),
}

fn compare(reading: &Reading, cmp: Option<&(Op, Operand)>) -> Option<bool> {
    let equality = |op: Op, same: bool| match op {
        Op::Eq => Some(same),
        Op::Ne => Some(!same),
        _ => None,
    };
    match (reading, cmp) {
        (Reading::Flag(b), None) => Some(*b),
        (Reading::Flag(b), Some((op, Operand::Bool(v)))) => equality(*op, b == v),
        (Reading::Number(x), Some((op, Operand::Number(v)))) => {
            let order = if (x - v).abs() < 1e-9 {
                Ordering::Equal
            } else {
                x.partial_cmp(v)?
            };
            Some(op.apply(order))
        }
        (Reading::Hour(x), Some((op, Operand::Hour(v)))) => Some(op.apply(x.cmp(v))),
        (Reading::Text(x), Some((op, Operand::Text(v)))) => {
            equality(*op, x.trim().eq_ignore_ascii_case(v.trim()))
        }
        _ => None,
    }
}

/// What one evaluation pass may read: the tools, the clock, the active phase, and every read
/// made so far (one per tool and arguments).
pub struct Ctx<'a> {
    tools: &'a dyn Tools,
    /// The current game hour (`date` atoms).
    pub now: GameHour,
    /// The plan's active phase (`phase:<id>` atoms).
    pub phase: Option<String>,
    cache: Cache,
    unknown: BTreeMap<String, String>,
}

/// The reads one context made (tool and arguments -> reply, None when the read failed), kept
/// apart from the context so a caller can carry one snapshot through a whole tick
/// (`Ctx::with_cache`, `Ctx::into_cache`).
#[derive(Default)]
pub struct Cache {
    reads: HashMap<String, Option<Value>>,
    sets: HashMap<&'static str, Option<HashSet<String>>>,
}

impl<'a> Ctx<'a> {
    pub fn new(tools: &'a dyn Tools, now: GameHour) -> Self {
        Self::with_cache(tools, now, Cache::default())
    }
    /// A context that goes on with the reads of an earlier one (the same tick).
    pub fn with_cache(tools: &'a dyn Tools, now: GameHour, cache: Cache) -> Self {
        Self {
            tools,
            now,
            phase: None,
            cache,
            unknown: BTreeMap::new(),
        }
    }
    /// The reads made, for the next context of the same tick.
    pub fn into_cache(self) -> Cache {
        self.cache
    }
    /// A read tool's reply, made at most once per context (a failed read stays failed).
    pub fn read(&mut self, name: &str, args: &Value) -> Option<&Value> {
        let tools = self.tools;
        self.cache
            .reads
            .entry(format!("{name} {args}"))
            .or_insert_with(|| tools.call(name, args).ok())
            .as_ref()
    }
    /// Atoms (as written) that evaluated unknown, with the reason.
    pub fn unknown(&self) -> &BTreeMap<String, String> {
        &self.unknown
    }
    pub fn take_unknown(&mut self) -> BTreeMap<String, String> {
        std::mem::take(&mut self.unknown)
    }
    /// Record an unknown (the first reason for an atom is kept).
    pub fn note_unknown(&mut self, what: &str, why: impl Into<String>) {
        self.unknown
            .entry(what.to_string())
            .or_insert_with(|| why.into());
    }
    fn me(&mut self) -> std::result::Result<String, String> {
        self.read("hoi4_overview", &json!({}))
            .and_then(|o| o["player"].as_str())
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .ok_or_else(|| "player tag unknown (hoi4_overview unreadable)".to_string())
    }
    /// Every id of a paged list read (hoi4_research researched, hoi4_focus_completed completed).
    fn paged(
        &mut self,
        tool: &'static str,
        key: &str,
    ) -> std::result::Result<&HashSet<String>, String> {
        let tools = self.tools;
        let set =
            self.cache.sets.entry(tool).or_insert_with(|| {
                let mut all = HashSet::new();
                let mut offset = 0u64;
                for _ in 0..64 {
                    let page = tools
                        .call(tool, &json!({"offset": offset, "limit": 100}))
                        .ok()?;
                    all.extend(page[key].as_array().into_iter().flatten().filter_map(|x| {
                        x.as_str().or_else(|| x["id"].as_str()).map(str::to_string)
                    }));
                    match page["next_offset"].as_u64() {
                        Some(next) if next > offset => offset = next,
                        _ => break,
                    }
                }
                Some(all)
            });
        set.as_ref().ok_or_else(|| format!("{tool} unreadable"))
    }
}

/// Typed reads for rules that are not written as predicates (focus availability): the same
/// cached tool reads the atoms use.
impl Ctx<'_> {
    /// The player's tag.
    pub(crate) fn player(&mut self) -> Option<String> {
        self.me().ok()
    }
    /// The tags at war with the player.
    pub(crate) fn war_enemies(&mut self) -> Option<Vec<String>> {
        enemies(self).ok()
    }
    /// Every completed focus of the player.
    pub(crate) fn completed_focuses(&mut self) -> Option<&HashSet<String>> {
        self.paged("hoi4_focus_completed", "completed").ok()
    }
    /// Every technology the player researched.
    pub(crate) fn researched(&mut self) -> Option<&HashSet<String>> {
        self.paged("hoi4_research", "researched").ok()
    }
    /// Whether `tag` owns a state (None: not readable, or not a tag `hoi4_states` can list).
    pub(crate) fn owns_a_state(&mut self, tag: &str) -> Option<bool> {
        if tag.len() != 3 || !tag.bytes().all(|b| b.is_ascii_uppercase()) {
            return None;
        }
        let read = self.read("hoi4_states", &json!({"query": tag}))?;
        let rows = read["states"].as_array()?;
        Some(rows.iter().any(|r| r["owner"].as_str() == Some(tag)))
    }
    /// A state's owner, its controller, and the one tag that holds every province of it
    /// (`Some(None)`: several tags do; None: the provinces were not listed).
    #[allow(clippy::type_complexity)]
    pub(crate) fn state_holders(
        &mut self,
        id: i64,
    ) -> Option<(Option<String>, Option<String>, Option<Option<String>>)> {
        let row = state_row(self, id).ok()?;
        let text = |key: &str| {
            row[key]
                .as_str()
                .filter(|t| !t.is_empty())
                .map(str::to_string)
        };
        let holders: HashSet<&str> = row["provinces"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|p| p["controller"].as_str())
            .collect();
        let full = match holders.len() {
            0 => None,
            1 => Some(holders.iter().next().map(|h| h.to_string())),
            _ => Some(None),
        };
        Some((text("owner"), text("controller"), full))
    }
    /// The reading of a numeric atom without an argument (`stability`, `war_support`, `pp`,
    /// `manpower`, `world_tension`).
    pub(crate) fn number(&mut self, name: &str) -> Option<f64> {
        let atom = Atom {
            text: name.to_string(),
            name: name.to_string(),
            arg: None,
            cmp: None,
        };
        match reading(&atom, self) {
            Ok(Reading::Number(value)) => Some(value),
            _ => None,
        }
    }
}

/// Parse and evaluate; a predicate that does not parse is Unknown (recorded with its error).
pub fn check(value: &Value, ctx: &mut Ctx) -> Truth {
    match parse(value) {
        Ok(predicate) => holds(&predicate, ctx),
        Err(e) => {
            let text = value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_string);
            ctx.note_unknown(&text, format!("invalid predicate: {}", e.message));
            Truth::Unknown
        }
    }
}

/// The atoms of `value` that read false now, as written: why a `when` is not met.
pub fn false_atoms(value: &Value, ctx: &mut Ctx) -> Vec<String> {
    fn walk(p: &Predicate, ctx: &mut Ctx, out: &mut Vec<String>) {
        match p {
            Predicate::Const(_) => {}
            Predicate::Not(inner) => {
                if holds(inner, ctx) == Truth::True {
                    out.push(format!("not {}", describe(inner)));
                }
            }
            Predicate::All(parts) | Predicate::Any(parts) => {
                for part in parts {
                    walk(part, ctx, out);
                }
            }
            Predicate::Atom(atom) => {
                if holds(p, ctx) == Truth::False {
                    out.push(atom.text.clone());
                }
            }
        }
    }
    fn describe(p: &Predicate) -> String {
        match p {
            Predicate::Atom(a) => a.text.clone(),
            _ => "(group)".to_string(),
        }
    }
    let mut out = Vec::new();
    if let Ok(p) = parse(value) {
        walk(&p, ctx, &mut out);
    }
    out
}

/// Evaluate a parsed predicate (see the module docs).
pub fn holds(predicate: &Predicate, ctx: &mut Ctx) -> Truth {
    match predicate {
        Predicate::Const(b) => Truth::from_bool(*b),
        Predicate::Not(inner) => !holds(inner, ctx),
        Predicate::All(parts) => {
            let mut unknown = false;
            for part in parts {
                match holds(part, ctx) {
                    Truth::False => return Truth::False,
                    Truth::Unknown => unknown = true,
                    Truth::True => {}
                }
            }
            if unknown {
                Truth::Unknown
            } else {
                Truth::True
            }
        }
        Predicate::Any(parts) => {
            let mut unknown = false;
            for part in parts {
                match holds(part, ctx) {
                    Truth::True => return Truth::True,
                    Truth::Unknown => unknown = true,
                    Truth::False => {}
                }
            }
            if unknown {
                Truth::Unknown
            } else {
                Truth::False
            }
        }
        Predicate::Atom(atom) => match reading(atom, ctx) {
            Ok(value) => match compare(&value, atom.cmp.as_ref()) {
                Some(b) => Truth::from_bool(b),
                None => {
                    ctx.note_unknown(
                        &atom.text,
                        format!("the reading {value:?} cannot be compared with the value"),
                    );
                    Truth::Unknown
                }
            },
            Err(why) => {
                ctx.note_unknown(&atom.text, why);
                Truth::Unknown
            }
        },
    }
}

type Read<T> = std::result::Result<T, String>;

fn id_arg(atom: &Atom) -> Read<i64> {
    atom.arg
        .as_deref()
        .and_then(|a| a.parse().ok())
        .ok_or_else(|| {
            format!(
                "'{}' is not a numeric id",
                atom.arg.as_deref().unwrap_or("")
            )
        })
}

fn tag_arg(atom: &Atom) -> Read<String> {
    let tag = atom.arg.as_deref().unwrap_or("").trim();
    if tag.len() == 3 && tag.bytes().all(|b| b.is_ascii_uppercase()) {
        Ok(tag.to_string())
    } else {
        Err(format!(
            "'{tag}' is not a country tag hoi4_states can list (three capital letters)"
        ))
    }
}

/// A country's factories (civilian, military and naval): the player's own from hoi4_economy
/// (live), another country's estimated from hoi4_states{query:TAG} factories_start.controlled
/// (the factories the game files give the states it controls now; nothing reads another
/// country's buildings live).
fn factories(ctx: &mut Ctx, tag: &str) -> Read<f64> {
    if ctx.me().ok().as_deref() == Some(tag) {
        let counts = ctx
            .read("hoi4_economy", &json!({}))
            .map(|e| e["factory_trigger_count"].clone())
            .ok_or("hoi4_economy unreadable")?;
        let kinds = ["civilian", "military", "naval"];
        let values: Option<Vec<f64>> = kinds.iter().map(|k| counts[*k].as_f64()).collect();
        return values
            .map(|v| v.iter().sum())
            .ok_or_else(|| "hoi4_economy factory_trigger_count unreadable".into());
    }
    let read = ctx
        .read("hoi4_states", &json!({"query": tag}))
        .ok_or("hoi4_states unreadable")?;
    read["factories_start"]["controlled"]["total"]
        .as_f64()
        .ok_or_else(|| format!("hoi4_states gives no factories_start for {tag}"))
}

/// A top-bar number of hoi4_country_status.
fn status_field(ctx: &mut Ctx, field: &str) -> Read<f64> {
    let status = ctx
        .read("hoi4_country_status", &json!({}))
        .ok_or("hoi4_country_status unreadable")?;
    let f = &status["fields"][field];
    f["value"].as_f64().ok_or_else(|| {
        format!(
            "hoi4_country_status {field}: {}",
            f["reason"].as_str().unwrap_or("no value")
        )
    })
}

/// The hoi4_states row of one state id.
fn state_row(ctx: &mut Ctx, id: i64) -> Read<Value> {
    let read = ctx
        .read("hoi4_states", &json!({"query": id.to_string()}))
        .ok_or("hoi4_states unreadable")?;
    read["states"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|row| row["state_id"].as_i64() == Some(id))
        .cloned()
        .ok_or_else(|| format!("state {id} not found by hoi4_states"))
}

/// The tags at war with the player (hoi4_wars, else hoi4_diplomacy_state).
fn enemies(ctx: &mut Ctx) -> Read<Vec<String>> {
    for tool in ["hoi4_wars", "hoi4_diplomacy_state"] {
        if let Some(list) = ctx
            .read(tool, &json!({}))
            .and_then(|v| v["wars"]["enemies"].as_array())
        {
            return Ok(list
                .iter()
                .filter_map(|t| t.as_str().or_else(|| t["tag"].as_str()))
                .map(str::to_string)
                .collect());
        }
    }
    Err("own wars unreadable (hoi4_wars wars, hoi4_diplomacy_state wars)".into())
}

/// The surrender progress of enemy `tag` (0..1) from its row in hoi4_wars `wars.wars[]`
/// (`enemy_surrender_progress`; a value above 1 is read as a percent). A null value is unknown with
/// the tool's own reason: the read is a seam the tool fills once the value is located in memory.
/// `casualty_ratio:<TAG>`: the men TAG lost in its war relation with the player over the men
/// the player lost in it (hoi4_war_stats; 2 is "they lost twice as many"). A measure of an
/// attacker worn down by a defence; unknown until both sides have losses.
fn casualty_ratio(ctx: &mut Ctx, tag: &str) -> Read<f64> {
    let me = ctx.me()?;
    let read = ctx
        .read("hoi4_war_stats", &json!({}))
        .ok_or("hoi4_war_stats unreadable")?;
    let losses = |who: &str| -> f64 {
        read["wars"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|w| w["relations"].as_array().into_iter().flatten())
            .filter(|r| {
                let tags: Vec<&str> = r["countries"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|c| c["tag"].as_str())
                    .collect();
                tags.contains(&me.as_str()) && tags.contains(&tag)
            })
            .flat_map(|r| r["countries"].as_array().into_iter().flatten())
            .filter(|c| c["tag"].as_str() == Some(who))
            .filter_map(|c| c["casualties"].as_f64())
            .sum()
    };
    let (theirs, ours) = (losses(tag), losses(&me));
    if ours <= 0.0 || theirs <= 0.0 {
        return Err(format!(
            "hoi4_war_stats has no losses on both sides between {me} and {tag} yet"
        ));
    }
    Ok(theirs / ours)
}

pub(crate) fn surrender_progress(ctx: &mut Ctx, tag: &str) -> Read<f64> {
    war_ratio(
        ctx,
        tag,
        "enemy_surrender_progress",
        "surrender progress",
        "peace.surrender_progress_read",
    )
}

/// A 0..1 `field` of the row of the war with `tag` in hoi4_wars `wars.wars[]` (a value above 1 is
/// a percent). Null is unknown with the tool's `<field>_reason` and the gap register `row`.
fn war_ratio(ctx: &mut Ctx, tag: &str, field: &str, what: &str, row_id: &str) -> Read<f64> {
    let read = ctx
        .read("hoi4_wars", &json!({}))
        .ok_or("hoi4_wars unreadable")?;
    let row = read["wars"]["wars"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|w| w["enemy"]["tag"].as_str().or_else(|| w["enemy"].as_str()) == Some(tag))
        .ok_or_else(|| format!("no own war with {tag} in hoi4_wars (no war overview row)"))?;
    match row[field].as_f64() {
        Some(v) if v > 1.0 => Ok(v / 100.0),
        Some(v) => Ok(v.max(0.0)),
        None => Err(format!(
            "hoi4_wars reads no {what} for {tag} ({}; gap register {row_id})",
            row[format!("{field}_reason").as_str()]
                .as_str()
                .unwrap_or("field missing")
        )),
    }
}

/// The own naval dominance share of strategic sea region `region` from hoi4_fleets `sea_regions[]`
/// (0..1, a percent above 1). A null list is unknown with the tool's reason: the read is a seam the
/// tool fills once the sea region values are located in memory.
fn sea_dominance(ctx: &mut Ctx, region: i64) -> Read<f64> {
    let read = ctx
        .read("hoi4_fleets", &json!({}))
        .ok_or("hoi4_fleets unreadable")?;
    let Some(rows) = read["sea_regions"].as_array() else {
        return Err(format!(
            "hoi4_fleets reads no sea region dominance ({}; gap register navy.dominance_intel_read)",
            read["sea_regions_reason"]
                .as_str()
                .unwrap_or("field missing")
        ));
    };
    let row = rows
        .iter()
        .find(|r| r["region_id"].as_i64() == Some(region))
        .ok_or_else(|| format!("sea region {region} not listed by hoi4_fleets"))?;
    match row["own_dominance"].as_f64() {
        Some(v) if v > 1.0 => Ok(v / 100.0),
        Some(v) => Ok(v.max(0.0)),
        None => Err(format!(
            "hoi4_fleets sea region {region} has no own_dominance"
        )),
    }
}

/// Who holds province `p`: looked up in the victory points and the provinces held by others of
/// the player's and the enemies' hoi4_states reads.
fn province_controller(ctx: &mut Ctx, p: i64) -> Read<String> {
    let mut tags = vec![ctx.me()?];
    if let Ok(enemies) = enemies(ctx) {
        tags.extend(enemies);
    }
    let tags_read = tags.clone();
    for tag in tags {
        let Some(read) = ctx.read("hoi4_states", &json!({"query": tag})) else {
            continue;
        };
        for row in read["states"].as_array().into_iter().flatten() {
            let found = row["provinces_held_by_others"]
                .as_array()
                .into_iter()
                .flatten()
                .chain(row["victory_points"].as_array().into_iter().flatten())
                .chain(row["provinces"].as_array().into_iter().flatten())
                .find(|x| x["province"].as_i64() == Some(p))
                .and_then(|x| x["controller"].as_str())
                .filter(|c| !c.is_empty());
            if let Some(c) = found {
                return Ok(c.to_string());
            }
        }
    }
    // Not listed (a plain province neither a victory point nor held by others): find its state
    // in the map files and read that state by id, which lists every province with its
    // controller. Without this an own province retaken read as unknown forever, so a
    // contingency on it never ended (i18a, 1936: Italy held in a "retake Eritrea" phase that
    // attacks nothing from January to November).
    let mut ids: Vec<i64> = Vec::new();
    for tag in tags_read {
        if let Some(read) = ctx.read("hoi4_states", &json!({"query": tag})) {
            ids.extend(
                read["states"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|r| r["state_id"].as_i64()),
            );
        }
    }
    ids.sort_unstable();
    ids.dedup();
    for chunk in ids.chunks(128) {
        let Some(graph) = ctx.read("hoi4_province_graph", &json!({"states": chunk})) else {
            continue;
        };
        let Some(state) = graph["provinces"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|x| x["province"].as_i64() == Some(p))
            .and_then(|x| x["state_id"].as_i64())
        else {
            continue;
        };
        if let Some(c) = ctx
            .read("hoi4_states", &json!({"query": state.to_string()}))
            .and_then(|read| {
                read["states"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|r| r["provinces"].as_array().into_iter().flatten())
                    .find(|x| x["province"].as_i64() == Some(p))
                    .and_then(|x| x["controller"].as_str())
                    .filter(|c| !c.is_empty())
                    .map(str::to_string)
            })
        {
            return Ok(c);
        }
        break;
    }
    Err(format!(
        "province {p} not found in the own or enemy state reads nor in its own state's read"
    ))
}

/// Days of cover of `equipment` (an archetype or a variant key) in a hoi4_stockpile read: the
/// row's days_of_cover; a stock that is not falling counts as [`NOT_FALLING_DAYS`].
pub(crate) fn stockpile_days(read: &Value, equipment: &str) -> Read<f64> {
    let rows = read["equipment"]
        .as_array()
        .ok_or("hoi4_stockpile lists no equipment")?;
    let row = rows
        .iter()
        .find(|r| r["archetype"].as_str() == Some(equipment))
        .or_else(|| {
            rows.iter().find(|r| {
                r["variants"]
                    .as_array()
                    .is_some_and(|v| v.iter().any(|x| x["type"].as_str() == Some(equipment)))
            })
        })
        .ok_or_else(|| format!("{equipment} not in hoi4_stockpile"))?;
    if let Some(days) = row["days_of_cover"].as_f64() {
        return Ok(days);
    }
    if row["depleting"] == json!(false) {
        return Ok(NOT_FALLING_DAYS);
    }
    Err(format!(
        "{equipment}: no stockpile trend yet (net_per_day needs two game days seen by this server)"
    ))
}

/// Days of cover of a stockpile that is not falling.
pub(crate) const NOT_FALLING_DAYS: f64 = 9999.0;

fn reading(atom: &Atom, ctx: &mut Ctx) -> Read<Reading> {
    let arg = atom.arg.as_deref().unwrap_or("");
    let num = |v: Option<f64>, why: &str| v.map(Reading::Number).ok_or_else(|| why.to_string());
    match atom.name.as_str() {
        "date" => Ok(Reading::Hour(ctx.now)),
        "stability" | "war_support" | "manpower" => {
            status_field(ctx, &atom.name).map(Reading::Number)
        }
        "pp" => {
            let economy = ctx
                .read("hoi4_economy", &json!({}))
                .and_then(|e| e["political_power"]["raw_signed"].as_f64())
                .map(|raw| raw / 100_000.0);
            match economy {
                Some(pp) => Ok(Reading::Number(pp)),
                None => status_field(ctx, "political_power").map(Reading::Number),
            }
        }
        "world_tension" => {
            let tension = ctx
                .read("hoi4_diplomacy_state", &json!({}))
                .and_then(|d| d["world_tension"]["value"].as_f64());
            match tension {
                Some(t) => Ok(Reading::Number(t)),
                None => status_field(ctx, "world_tension").map(Reading::Number),
            }
        }
        "xp" => {
            let kind = match arg {
                "army" | "land" => "army",
                "air" => "air",
                "navy" | "naval" => "navy",
                other => return Err(format!("xp:{other}: use army, air or navy")),
            };
            let v = ctx
                .read("hoi4_doctrines", &json!({}))
                .and_then(|d| d["experience"][kind]["value"].as_f64());
            num(v, "hoi4_doctrines experience unreadable")
        }
        "fuel_ratio" => {
            let v = ctx
                .read("hoi4_resources", &json!({}))
                .and_then(|r| r["fuel"]["ratio"].as_f64());
            num(v, "hoi4_resources fuel.ratio unreadable")
        }
        "convoys_free" => {
            let free = |c: &Value| c["convoys"]["free"].as_f64();
            let v = ctx
                .read("hoi4_convoys", &json!({}))
                .and_then(free)
                .or_else(|| ctx.read("hoi4_country_status", &json!({})).and_then(free));
            num(
                v,
                "free convoys unreadable (hoi4_convoys and hoi4_country_status convoys.free)",
            )
        }
        "resistance" | "compliance" => {
            let id = id_arg(atom)?;
            let read = ctx
                .read("hoi4_occupation", &json!({}))
                .ok_or("hoi4_occupation unreadable")?;
            let row = read["states"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|s| s["state_id"].as_i64() == Some(id))
                .ok_or_else(|| {
                    format!("state {id} not listed by hoi4_occupation (no resistance block read)")
                })?;
            num(
                row[atom.name.as_str()].as_f64().map(|pct| pct / 100.0),
                "hoi4_occupation value missing",
            )
        }
        "equipment_fill" => {
            let read = ctx
                .read("hoi4_supply", &json!({}))
                .ok_or("hoi4_supply unreadable")?;
            let fills: Vec<f64> = read["divisions"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|d| d["army"].as_str() == Some(arg))
                .filter_map(|d| d["equipment_fill_min"].as_f64())
                .collect();
            if !fills.is_empty() {
                return Ok(Reading::Number(
                    fills.iter().sum::<f64>() / fills.len() as f64,
                ));
            }
            let row = read["armies"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|a| a["army"].as_str() == Some(arg))
                .and_then(|a| a["equipment_fill_min"].as_f64());
            num(
                row,
                &format!("army '{arg}' has no equipment reading in hoi4_supply"),
            )
        }
        "stockpile_days" => {
            if arg.is_empty() {
                return Err("stockpile_days needs an equipment key".into());
            }
            let read = ctx
                .read("hoi4_stockpile", &json!({"equipment": arg}))
                .ok_or("hoi4_stockpile unreadable")?;
            stockpile_days(read, arg).map(Reading::Number)
        }
        "var" => Err("no tool reads scenario variables (audit W1-18)".into()),
        "bop" => {
            let read = ctx
                .read("hoi4_country_status", &json!({}))
                .ok_or("hoi4_country_status unreadable")?;
            let list = read["balances_of_power"].as_array().ok_or_else(|| {
                format!(
                    "balances of power unreadable ({})",
                    read["balances_of_power_error"]["code"]
                        .as_str()
                        .unwrap_or("no list")
                )
            })?;
            let row = list
                .iter()
                .find(|b| b["id"].as_str() == Some(arg))
                .ok_or_else(|| format!("no active balance of power '{arg}'"))?;
            num(row["value"].as_f64(), "balance of power value missing")
        }
        "mission_days" => Err("hoi4_decisions_list reports no mission timers (audit W1-18)".into()),
        "naval_dominance" => sea_dominance(ctx, id_arg(atom)?).map(Reading::Number),
        "naval_intel" => war_ratio(
            ctx,
            &tag_arg(atom)?,
            "own_naval_intel",
            "naval intel",
            "navy.dominance_intel_read",
        )
        .map(Reading::Number),
        "capitulated" => {
            // What a player sees: the war with TAG is over and either the peace conference that
            // follows a capitulation is open or TAG no longer exists (annexed). A white peace
            // leaves TAG existing with no conference, so it does not count.
            // An enemy that owns no state any more was annexed (or fights on only as a
            // government in exile): the game can keep its war relation for months after the
            // capitulation, and reading that relation alone called a won war lost (live 15 Nov
            // 2026).
            let tag = tag_arg(atom)?;
            if enemies(ctx)?.contains(&tag) {
                // hoi4_wars names the enemies that control none of their states (capitulated,
                // exiled or annexed): the game's own war relation stays with an exile for good
                // (g17j3 19 Nov 2026: Poland in exile read "not capitulated" for 282 days)
                if let Some(won) = ctx
                    .read("hoi4_wars", &json!({}))
                    .and_then(|w| w["wars"]["wars_won"].as_array())
                {
                    if won.iter().any(|t| t.as_str() == Some(tag.as_str())) {
                        return Ok(Reading::Flag(true));
                    }
                }
                // a remnant still holding land on our border fights on: out only when none of
                // its divisions stands in its states on our border (w43c1, 23 Nov 2026: 2 of 91
                // points held in Zeeland, Holland retook 20); a remnant on our border with no
                // division there is beaten (w43n1: Holland held 2 of 91 points and Belgium 1 of
                // 86 on our border for 15-25 days, the game never capitulated them and the
                // operation into France never started). A read without the enemy's divisions
                // counts any state on our border, as before.
                let touching = ctx
                    .read("hoi4_wars", &json!({}))
                    .is_some_and(|w| border_held(w, &tag));
                let read = ctx
                    .read("hoi4_states", &json!({"query": tag}))
                    .ok_or("hoi4_states unreadable")?;
                let rows = read["states"].as_array().ok_or("hoi4_states unreadable")?;
                // control decides: an exile still owns its occupied states until the peace;
                // a country that holds a state or a victory point still fights
                let landless = !rows.iter().any(|r| {
                    r["controller"].as_str() == Some(tag.as_str())
                        || r["victory_points"].as_array().is_some_and(|vps| {
                            vps.iter()
                                .any(|v| v["controller"].as_str() == Some(tag.as_str()))
                        })
                });
                return Ok(Reading::Flag(
                    landless || (!touching && remnant(rows, &tag)),
                ));
            }
            let conference = ctx
                .read("hoi4_events", &json!({}))
                .and_then(|e| e["peace_conference_open"].as_bool())
                .ok_or("hoi4_events unreadable")?;
            let read = ctx
                .read("hoi4_states", &json!({"query": tag}))
                .ok_or("hoi4_states unreadable")?;
            let rows = read["states"].as_array().ok_or("hoi4_states unreadable")?;
            let exists = rows
                .iter()
                .any(|r| r["owner"].as_str() == Some(tag.as_str()));
            Ok(Reading::Flag(conference || !exists))
        }
        "at_peace_conference" => ctx
            .read("hoi4_events", &json!({}))
            .and_then(|e| e["peace_conference_open"].as_bool())
            .map(Reading::Flag)
            .ok_or_else(|| "hoi4_events unreadable".into()),
        "at_war_with" => {
            let tag = arg.to_string();
            Ok(Reading::Flag(enemies(ctx)?.contains(&tag)))
        }
        "vp_share" => {
            let tag = tag_arg(atom)?;
            let read = ctx
                .read("hoi4_states", &json!({"query": tag}))
                .ok_or("hoi4_states unreadable")?;
            match read["victory_points"]["share_held_by_others"].as_f64() {
                Some(lost) => Ok(Reading::Number(1.0 - lost)),
                None => Err(format!(
                    "hoi4_states lists no victory points owned by {tag}"
                )),
            }
        }
        // surrender_progress:<TAG>: that enemy's progress from hoi4_wars
        "surrender_progress" => surrender_progress(ctx, &tag_arg(atom)?).map(Reading::Number),
        "own_surrender_progress" => {
            // Bare: the player's own progress, the share of its own victory points others hold
            // (it capitulates when that reaches its surrender limit).
            let me = ctx.me()?;
            let read = ctx
                .read("hoi4_states", &json!({"query": me}))
                .ok_or("hoi4_states unreadable")?;
            match read["victory_points"]["share_held_by_others"].as_f64() {
                Some(lost) => Ok(Reading::Number(lost)),
                None => Err(format!("hoi4_states lists no victory points owned by {me}")),
            }
        }
        "factories" => factories(ctx, &tag_arg(atom)?).map(Reading::Number),
        "casualty_ratio" => casualty_ratio(ctx, &tag_arg(atom)?).map(Reading::Number),
        "industry_ratio" => {
            let tag = tag_arg(atom)?;
            let me = ctx.me()?;
            let own = factories(ctx, &me)?;
            if own <= 0.0 {
                return Err(format!("{me} has no factories to compare with"));
            }
            Ok(Reading::Number(factories(ctx, &tag)? / own))
        }
        "exists" => {
            let tag = tag_arg(atom)?;
            let read = ctx
                .read("hoi4_states", &json!({"query": tag}))
                .ok_or("hoi4_states unreadable")?;
            let rows = read["states"].as_array().ok_or("hoi4_states unreadable")?;
            Ok(Reading::Flag(
                rows.iter()
                    .any(|r| r["owner"].as_str() == Some(tag.as_str())),
            ))
        }
        "controls_state" => {
            let me = ctx.me()?;
            let row = state_row(ctx, id_arg(atom)?)?;
            Ok(Reading::Flag(
                row["controller"].as_str() == Some(me.as_str()),
            ))
        }
        "owner" | "controller" => {
            let row = state_row(ctx, id_arg(atom)?)?;
            row[atom.name.as_str()]
                .as_str()
                .map(|t| Reading::Text(t.to_string()))
                .ok_or_else(|| format!("hoi4_states has no {} for that state", atom.name))
        }
        "state_fully_controlled" | "state_controller" => {
            let row = state_row(ctx, id_arg(atom)?)?;
            let holders: HashSet<&str> = row["provinces"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|p| p["controller"].as_str())
                .collect();
            if holders.is_empty() {
                return Err("hoi4_states listed no province controllers for that state".into());
            }
            let single = if holders.len() == 1 {
                holders.iter().next().map(|h| h.to_string())
            } else {
                None
            };
            if atom.name == "state_controller" {
                return Ok(Reading::Text(single.unwrap_or_else(|| "(mixed)".into())));
            }
            let me = ctx.me()?;
            Ok(Reading::Flag(single.as_deref() == Some(me.as_str())))
        }
        "province_controlled" => {
            let me = ctx.me()?;
            let holder = province_controller(ctx, id_arg(atom)?)?;
            Ok(Reading::Flag(holder == me))
        }
        "province_controller" => province_controller(ctx, id_arg(atom)?).map(Reading::Text),
        "focus_done" => Ok(Reading::Flag(
            ctx.paged("hoi4_focus_completed", "completed")?
                .contains(arg),
        )),
        "tech_done" => Ok(Reading::Flag(
            ctx.paged("hoi4_research", "researched")?.contains(arg),
        )),
        "phase" => Ok(Reading::Flag(ctx.phase.as_deref() == Some(arg))),
        other => Err(format!("unknown atom '{other}' (see plan_predicate.rs)")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        replies: HashMap<&'static str, Value>,
        calls: RefCell<Vec<(String, Value)>>,
    }
    impl Tools for Fake {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.calls
                .borrow_mut()
                .push((name.to_string(), args.clone()));
            if name == "hoi4_states" {
                let q = args["query"].as_str().unwrap_or("");
                return self
                    .replies
                    .get(name)
                    .and_then(|v| v.get(q).cloned())
                    .ok_or_else(|| Error::new("not_found", "no fixture"));
            }
            self.replies
                .get(name)
                .cloned()
                .ok_or_else(|| Error::new("capability_unavailable", "no fixture"))
        }
    }
    fn game() -> Fake {
        let replies = [
            ("hoi4_overview", json!({"player": "ITA"})),
            (
                "hoi4_country_status",
                json!({"fields": {"stability": {"value": 0.62}, "war_support": {"value": 0.3},
                    "manpower": {"value": 120000.0}, "convoys": {"value": 40.0},
                    "world_tension": {"value": null, "reason": "text_empty"}}}),
            ),
            (
                "hoi4_economy",
                json!({"political_power": {"raw_signed": 12_345_000}}),
            ),
            (
                "hoi4_doctrines",
                json!({"experience": {"army": {"value": 55.0}, "air": {"value": 3.0}}}),
            ),
            (
                "hoi4_wars",
                json!({"wars": {"at_war": true, "enemies": ["ETH"]}}),
            ),
            ("hoi4_events", json!({"peace_conference_open": false})),
            ("hoi4_resources", json!({"fuel": {"ratio": 0.4}})),
            ("hoi4_convoys", json!({"convoys": {"total": 80}})),
            (
                "hoi4_occupation",
                json!({"states": [{"state_id": 550, "resistance": 31.5, "compliance": 12.0}]}),
            ),
            (
                "hoi4_supply",
                json!({"divisions": [
                    {"army": "Army 1", "equipment_fill_min": 0.6},
                    {"army": "Army 1", "equipment_fill_min": 0.8},
                    {"army": "Army 2", "equipment_fill_min": 1.0}],
                    "armies": [{"army": "Army 3", "equipment_fill_min": 0.5}]}),
            ),
            (
                "hoi4_focus_completed",
                json!({"completed": ["ITA_a"], "active_key": null}),
            ),
            (
                "hoi4_research",
                json!({"researched": [{"id": "infantry_weapons"}]}),
            ),
            (
                "hoi4_states",
                json!({
                    "550": {"states": [{"state_id": 550, "owner": "ETH", "controller": "ITA",
                        "provinces": [{"province": 1, "controller": "ITA"}, {"province": 2, "controller": "ETH"}]}]},
                    "551": {"states": [{"state_id": 551, "owner": "ETH", "controller": "ITA",
                        "provinces": [{"province": 3, "controller": "ITA"}]}]},
                    "ETH": {"states": [{"state_id": 550, "owner": "ETH", "controller": "ITA",
                        "victory_points": [{"province": 2, "victory_points": 5, "controller": "ETH"}],
                        "provinces_held_by_others": [{"province": 1, "controller": "ITA"}]}],
                        "victory_points": {"share_held_by_others": 0.75}},
                    "ITA": {"states": [{"state_id": 1, "owner": "ITA", "controller": "ITA"}]},
                    "AUS": {"states": []}
                }),
            ),
        ];
        Fake {
            replies: replies.into_iter().collect(),
            calls: RefCell::new(Vec::new()),
        }
    }
    #[test]
    fn a_plain_province_is_read_from_its_own_state() {
        // province 3: no victory point, held by nobody else: neither tag read lists it (i18a: a
        // retaken Eritrean province stayed unknown and its contingency never ended)
        assert_eq!(
            eval(&game(), json!("province_controlled:3")),
            Truth::Unknown
        );
        let mut f = game();
        f.replies.insert(
            "hoi4_province_graph",
            json!({"provinces": [{"province": 3, "state_id": 551, "neighbours": []}]}),
        );
        assert_eq!(eval(&f, json!("province_controlled:3")), Truth::True);
        assert_eq!(
            eval(&f, json!("province_controller:3 == ETH")),
            Truth::False
        );
    }

    #[test]
    fn a_false_when_names_its_false_atoms() {
        let f = game();
        let mut ctx = Ctx::new(&f, date_hour("1940.6.1").unwrap());
        let when = json!({"all": ["date >= 1940.5.10", "at_war_with:FRA", "xp:army >= 60"]});
        assert_eq!(
            false_atoms(&when, &mut ctx),
            vec!["at_war_with:FRA".to_string(), "xp:army >= 60".to_string()]
        );
    }

    #[test]
    fn capitulation_is_the_war_ending_in_a_conference_or_annexation() {
        // at war: not capitulated
        assert_eq!(eval(&game(), json!("capitulated:ETH")), Truth::False);
        // war over, conference open: capitulated
        let mut f = game();
        f.replies.insert(
            "hoi4_wars",
            json!({"wars": {"at_war": false, "enemies": []}}),
        );
        f.replies
            .insert("hoi4_events", json!({"peace_conference_open": true}));
        assert_eq!(eval(&f, json!("capitulated:ETH")), Truth::True);
        // war over, no conference, ETH still owns states: a white peace, not a capitulation
        f.replies
            .insert("hoi4_events", json!({"peace_conference_open": false}));
        assert_eq!(eval(&f, json!("capitulated:ETH")), Truth::False);
        // war over, ETH owns nothing: annexed
        assert_eq!(eval(&f, json!("capitulated:AUS")), Truth::True);
        // still a war relation, but the enemy holds no land: annexed or in exile, won
        let mut f = game();
        f.replies.insert(
            "hoi4_wars",
            json!({"wars": {"at_war": true, "validated": true, "enemies": ["AUS"]}}),
        );
        assert_eq!(eval(&f, json!("capitulated:AUS")), Truth::True);
        // a government in exile: still an enemy, still the owner of its occupied states, named
        // in wars_won: capitulated
        let mut f = game();
        f.replies.insert(
            "hoi4_wars",
            json!({"wars": {"at_war": true, "enemies": ["POL", "FRA"], "fighting": ["FRA"],
                "wars_won": ["POL"]}}),
        );
        assert_eq!(eval(&f, json!("capitulated:POL")), Truth::True);
    }
    #[test]
    fn an_enemy_left_with_a_remnant_of_its_victory_points_is_out() {
        let states = |colonies: &str| {
            json!({"HOL": {"states": [
                {"state_id": 7, "owner": "HOL", "controller": "GER",
                 "victory_points": [{"province": 391, "victory_points": 40.0, "controller": "GER"},
                                    {"province": 9498, "victory_points": 49.0, "controller": "GER"}]},
                {"state_id": 309, "owner": "HOL", "controller": colonies,
                 "victory_points": [{"province": 10906, "victory_points": 2.0, "controller": colonies}]}]}})
        };
        let mut f = game();
        f.replies.insert(
            "hoi4_wars",
            json!({"wars": {"at_war": true, "enemies": ["HOL"], "wars_won": []}}),
        );
        f.replies.insert("hoi4_states", states("HOL"));
        // 2 of 91 victory points across the sea: out
        assert_eq!(eval(&f, json!("capitulated:HOL")), Truth::True);
        // Holland still held: not out
        f.replies.insert(
            "hoi4_states",
            json!({"HOL": {"states": [{"state_id": 7, "owner": "HOL", "controller": "HOL",
                "victory_points": [{"province": 391, "victory_points": 40.0, "controller": "HOL"},
                                   {"province": 9498, "victory_points": 49.0, "controller": "GER"}]}]}}),
        );
        assert_eq!(eval(&f, json!("capitulated:HOL")), Truth::False);
        // the remnant on our border (Zeeland, not a colony): it fights on, not out
        f.replies.insert("hoi4_states", states("HOL"));
        f.replies.insert(
            "hoi4_wars",
            json!({"wars": {"at_war": true, "enemies": ["HOL"], "wars_won": []},
                   "borders": [{"neighbour_tag": "HOL", "neighbour_states": [309]}]}),
        );
        assert_eq!(eval(&f, json!("capitulated:HOL")), Truth::False);
        // the read names its divisions there: two of them hold the border, it fights on
        f.replies.insert(
            "hoi4_wars",
            json!({"wars": {"at_war": true, "enemies": ["HOL"], "wars_won": []},
                   "borders": [{"neighbour_tag": "HOL", "neighbour_states": [309],
                                "neighbour_divisions_in_border_states": 2}]}),
        );
        assert_eq!(eval(&f, json!("capitulated:HOL")), Truth::False);
        // the remnant on our border with no division in it is beaten (w43n1)
        f.replies.insert(
            "hoi4_wars",
            json!({"wars": {"at_war": true, "enemies": ["HOL"], "wars_won": []},
                   "borders": [{"neighbour_tag": "HOL", "neighbour_states": [309],
                                "neighbour_divisions_in_border_states": 0}]}),
        );
        assert_eq!(eval(&f, json!("capitulated:HOL")), Truth::True);
    }
    #[test]
    fn surrender_progress_is_unknown_until_hoi4_wars_maps_it() {
        let mut f = game();
        // today's tool: the war row has the field null with a reason
        f.replies.insert(
            "hoi4_wars",
            json!({"wars": {"at_war": true, "enemies": ["ETH"], "wars": [
                {"enemy": {"tag": "ETH", "name": "Ethiopia"}, "enemy_surrender_progress": null,
                 "enemy_surrender_progress_reason": "not_mapped"}]}}),
        );
        let mut ctx = Ctx::new(&f, date_hour("1936.05.01").unwrap());
        assert_eq!(
            check(&json!("surrender_progress:ETH >= 0.5"), &mut ctx),
            Truth::Unknown
        );
        let why = ctx.unknown()["surrender_progress:ETH >= 0.5"].clone();
        assert!(why.contains("not_mapped"), "{why}");
        // a war that is not in the list (not at war): unknown, never false
        assert_eq!(
            eval(&f, json!("surrender_progress:AUS < 0.5")),
            Truth::Unknown
        );
        // once the tool fills it: a fraction, or a percent above 1
        f.replies.insert(
            "hoi4_wars",
            json!({"wars": {"at_war": true, "enemies": ["ETH"], "wars": [
                {"enemy": {"tag": "ETH"}, "enemy_surrender_progress": 0.62}]}}),
        );
        assert_eq!(
            eval(&f, json!("surrender_progress:ETH >= 0.6")),
            Truth::True
        );
        assert_eq!(
            eval(&f, json!("surrender_progress:ETH >= 70%")),
            Truth::False
        );
        f.replies.insert(
            "hoi4_wars",
            json!({"wars": {"wars": [{"enemy": {"tag": "ETH"}, "enemy_surrender_progress": 75.0}]}}),
        );
        assert_eq!(eval(&f, json!("surrender_progress:ETH > 0.7")), Truth::True);
        // the argument is a tag
        assert_eq!(
            eval(&f, json!("surrender_progress:Ethiopia > 0.7")),
            Truth::Unknown
        );
    }
    #[test]
    fn naval_dominance_and_intel_wait_for_the_sea_region_read() {
        let mut f = game();
        f.replies.insert(
            "hoi4_fleets",
            json!({"fleets": [], "sea_regions": null, "sea_regions_reason": "not_mapped: test"}),
        );
        let mut ctx = Ctx::new(&f, date_hour("1936.05.01").unwrap());
        assert_eq!(
            check(&json!("naval_dominance:42 >= 0.6"), &mut ctx),
            Truth::Unknown
        );
        assert!(ctx.unknown()["naval_dominance:42 >= 0.6"].contains("not_mapped: test"));
        f.replies.insert(
            "hoi4_fleets",
            json!({"sea_regions": [{"region_id": 42, "own_dominance": 70}]}),
        );
        assert_eq!(eval(&f, json!("naval_dominance:42 >= 0.6")), Truth::True);
        assert_eq!(eval(&f, json!("naval_dominance:7 >= 0.6")), Truth::Unknown);
        // naval intel is per country: the war row, null with a reason today
        f.replies.insert(
            "hoi4_wars",
            json!({"wars": {"at_war": true, "enemies": ["ENG"], "wars": [
                {"enemy": {"tag": "ENG"}, "own_naval_intel": null,
                 "own_naval_intel_reason": "not_mapped: intel"}]}}),
        );
        let mut ctx = Ctx::new(&f, date_hour("1936.05.01").unwrap());
        assert_eq!(
            check(&json!("naval_intel:ENG >= 0.4"), &mut ctx),
            Truth::Unknown
        );
        assert!(ctx.unknown()["naval_intel:ENG >= 0.4"].contains("not_mapped: intel"));
        f.replies.insert(
            "hoi4_wars",
            json!({"wars": {"at_war": true, "enemies": ["ENG"], "wars": [
                {"enemy": {"tag": "ENG"}, "own_naval_intel": 35}]}}),
        );
        assert_eq!(eval(&f, json!("naval_intel:ENG >= 0.4")), Truth::False);
    }
    #[test]
    fn casualty_ratio_reads_the_losses_of_the_war_relation() {
        let mut f = game();
        f.replies.insert(
            "hoi4_war_stats",
            json!({"wars": [{"relations": [
                {"countries": [{"tag": "ETH", "casualties": 3000}, {"tag": "ITA", "casualties": 2000}]},
                {"countries": [{"tag": "ITA", "casualties": 500}, {"tag": "FRA", "casualties": 9000}]}]}]}),
        );
        assert_eq!(eval(&f, json!("casualty_ratio:ETH >= 1.5")), Truth::True);
        assert_eq!(eval(&f, json!("casualty_ratio:ETH > 1.6")), Truth::False);
        // no losses yet with that enemy: unknown, never true
        assert_eq!(eval(&f, json!("casualty_ratio:GER > 0")), Truth::Unknown);
    }
    #[test]
    fn law_gates_read_the_enemy_industry_and_the_own_surrender_progress() {
        let mut f = game();
        f.replies.insert(
            "hoi4_economy",
            json!({"political_power": {"raw_signed": 0},
                "factory_trigger_count": {"civilian": 20, "military": 15, "naval": 5}}),
        );
        let states = f.replies.get_mut("hoi4_states").unwrap();
        states["ETH"]["factories_start"] =
            json!({"owned": {"total": 12}, "controlled": {"total": 8}});
        states["ITA"]["victory_points"] = json!({"share_held_by_others": 0.3});
        for (p, want) in [
            // the enemy's factories: the states it controls now (8), not the ones it owns
            (json!("factories:ETH == 8"), Truth::True),
            // the player's own count is live (20 + 15 + 5)
            (json!("factories:ITA >= 40"), Truth::True),
            // 8 / 40 = 0.2: the war economy gate "an enemy above 40% of yours" does not hold
            (json!("industry_ratio:ETH > 0.4"), Truth::False),
            (json!("industry_ratio:ETH >= 0.2"), Truth::True),
            // 30% of the own victory points lost: Scraping the Barrel's "over 25%" holds
            (json!("own_surrender_progress > 25%"), Truth::True),
            (json!("own_surrender_progress > 0.5"), Truth::False),
        ] {
            assert_eq!(eval(&f, p.clone()), want, "{p}");
        }
        // a country hoi4_states lists no factories for, and no own count, stay unknown
        assert_eq!(eval(&f, json!("factories:AUS > 1")), Truth::Unknown);
        let mut g = game();
        g.replies.insert("hoi4_economy", json!({}));
        assert_eq!(eval(&g, json!("industry_ratio:ETH > 0.4")), Truth::Unknown);
        // the own victory points not listed: unknown, never true
        assert_eq!(
            eval(&game(), json!("own_surrender_progress > 0")),
            Truth::Unknown
        );
        let Predicate::Atom(a) = parse(&json!("industry_ratio:GER > 1.5")).unwrap() else {
            panic!()
        };
        // a plain number, not a percentage: 1.5 is one and a half times the own industry
        assert_eq!(a.cmp, Some((Op::Gt, Operand::Number(1.5))));
        assert!(parse(&json!("factories")).is_err());
    }
    fn eval(f: &Fake, p: Value) -> Truth {
        let mut ctx = Ctx::new(f, date_hour("1936.05.01").unwrap());
        check(&p, &mut ctx)
    }

    #[test]
    fn the_grammar_parses_comparisons_booleans_dates_and_combinations() {
        let p = parse(&json!("stability < 50%")).unwrap();
        let Predicate::Atom(a) = p else { panic!() };
        assert_eq!(a.name, "stability");
        assert_eq!(a.cmp, Some((Op::Lt, Operand::Number(0.5))));
        let Predicate::Atom(a) = parse(&json!("war_support>=25")).unwrap() else {
            panic!()
        };
        assert_eq!(a.cmp, Some((Op::Ge, Operand::Number(0.25))));
        let Predicate::Atom(a) = parse(&json!("vp_share:ETH<=0.2")).unwrap() else {
            panic!()
        };
        assert_eq!(
            (a.arg.as_deref(), a.cmp),
            (Some("ETH"), Some((Op::Le, Operand::Number(0.2))))
        );
        let Predicate::Atom(a) = parse(&json!("owner:550=ITA")).unwrap() else {
            panic!()
        };
        assert_eq!(a.cmp, Some((Op::Eq, Operand::Text("ITA".into()))));
        assert!(
            matches!(parse(&json!("1936.5.1")).unwrap(), Predicate::Atom(a) if a.name == "date")
        );
        assert!(matches!(
            parse(&json!("not_at_war:ETH")).unwrap(),
            Predicate::Not(_)
        ));
        assert!(matches!(
            parse(&json!({"any": ["pp > 100", {"not": "phase:war"}]})).unwrap(),
            Predicate::Any(v) if v.len() == 2
        ));
        assert_eq!(parse(&json!("")).unwrap(), Predicate::Const(true));
        for wrong in [
            json!("stability"),
            json!("stability <"),
            json!("pp > lots"),
            json!("pp > 50%"),
            json!("at_war_with"),
            json!("focus_done:x > 3"),
            json!("owner:5"),
            json!({"maybe": []}),
            json!({"all": "x"}),
            json!(3),
            json!("date:soon"),
        ] {
            assert_eq!(
                parse(&wrong).unwrap_err().code,
                "invalid_predicate",
                "{wrong}"
            );
        }
        // an unknown atom parses; it can only ever be unknown
        assert!(parse(&json!("supremacy:med > 0.5")).is_ok());
    }

    #[test]
    fn readable_atoms_evaluate_from_the_read_tools() {
        let f = game();
        for (p, want) in [
            (json!("stability >= 0.6"), Truth::True),
            (json!("war_support < 25%"), Truth::False),
            (json!("manpower > 100000"), Truth::True),
            (json!("pp >= 123.45"), Truth::True),
            (json!("xp:army >= 50"), Truth::True),
            (json!("xp:land < 50"), Truth::False),
            (json!("fuel_ratio < 0.5"), Truth::True),
            (json!("resistance:550 > 30"), Truth::True),
            (json!("compliance:550 >= 0.2"), Truth::False),
            (json!("equipment_fill:Army 1 < 0.8"), Truth::True),
            (json!("equipment_fill:Army 3 == 50%"), Truth::True),
            (json!("at_peace_conference"), Truth::False),
            (json!("at_war_with:ETH"), Truth::True),
            (json!("at_war:FRA"), Truth::False),
            (json!("vp_share:ETH <= 0.25"), Truth::True),
            (json!("controls_state:550"), Truth::True),
            (json!("state_fully_controlled:550"), Truth::False),
            (json!("state_fully_controlled:551"), Truth::True),
            (json!("state_controller:551 == ITA"), Truth::True),
            (json!("owner:550 == ETH"), Truth::True),
            (json!("controller:550 != ITA"), Truth::False),
            (json!("province_controlled:1"), Truth::True),
            (json!("province_controller:2 == ETH"), Truth::True),
            (json!("exists:ETH"), Truth::True),
            (json!("not_exists:AUS"), Truth::True),
            (json!("focus_done:ITA_a"), Truth::True),
            (json!("focus_completed:ITA_b"), Truth::False),
            (json!("tech_done:infantry_weapons"), Truth::True),
            (json!("1936.04.30"), Truth::True),
            (json!("date < 1936.05.01"), Truth::False),
            (
                json!({"all": ["at_war_with:ETH", "stability > 0.5"]}),
                Truth::True,
            ),
            (json!({"not": "at_war_with:ETH"}), Truth::False),
            (json!(["at_war_with:ETH", "focus_done:ITA_b"]), Truth::False),
        ] {
            assert_eq!(eval(&f, p.clone()), want, "{p}");
        }
    }

    #[test]
    fn unknown_and_unreadable_atoms_are_never_true_and_are_reported() {
        let f = game();
        let mut ctx = Ctx::new(&f, 0);
        for p in [
            "bop:ETH_centralization > 0.5",
            "var:ITA_eth_war_days < 170",
            "mission_days:ITA_mission > 10",
            "stockpile_days:infantry_equipment < 30",
            "convoys_free > 10",
            "world_tension > 0.5",
            "province_controlled:999",
            "resistance:12 > 10",
            "supremacy:med > 0.5",
        ] {
            assert_eq!(check(&json!(p), &mut ctx), Truth::Unknown, "{p}");
            // negation keeps unknown: never acts on it either way
            assert_eq!(check(&json!({"not": p}), &mut ctx), Truth::Unknown, "{p}");
        }
        let unknown = ctx.take_unknown();
        assert_eq!(unknown.len(), 9, "{unknown:?}");
        assert!(unknown["bop:ETH_centralization > 0.5"].contains("balances of power"));
        assert!(unknown["convoys_free > 10"].contains("free convoys unreadable"));
        assert!(unknown["world_tension > 0.5"].contains("text_empty"));
        // three-valued combinations: a false part decides all, a true part decides any
        assert_eq!(
            check(&json!({"all": ["bop:x > 1", "at_war_with:FRA"]}), &mut ctx),
            Truth::False
        );
        assert_eq!(
            check(&json!({"any": ["bop:x > 1", "at_war_with:ETH"]}), &mut ctx),
            Truth::True
        );
        assert_eq!(
            check(&json!({"all": ["bop:x > 1", "at_war_with:ETH"]}), &mut ctx),
            Truth::Unknown
        );
        // an invalid predicate is unknown too, with its parse error
        assert_eq!(check(&json!("stability <"), &mut ctx), Truth::Unknown);
        assert!(ctx.unknown()["stability <"].starts_with("invalid predicate"));
    }

    #[test]
    fn stockpile_convoys_and_balance_of_power_are_read() {
        let mut f = game();
        f.replies.insert(
            "hoi4_convoys",
            json!({"convoys": {"total": 200, "in_use": 32, "free": 168, "needed": 32}}),
        );
        f.replies.insert(
            "hoi4_country_status",
            json!({"fields": {}, "balances_of_power": [
                {"id": "ETH_centralization", "value": -0.35, "own": false}]}),
        );
        f.replies.insert(
            "hoi4_stockpile",
            json!({"equipment": [
                {"archetype": "infantry_equipment", "days_of_cover": 12.5, "depleting": true,
                 "variants": [{"type": "infantry_equipment_1", "amount": 900}]},
                {"archetype": "motorized_equipment", "days_of_cover": null, "depleting": false, "variants": []},
                {"archetype": "artillery_equipment", "days_of_cover": null, "depleting": null, "variants": []}]}),
        );
        for (p, want) in [
            ("convoys_free > 100", Truth::True),
            ("convoys_free < 100", Truth::False),
            ("bop:ETH_centralization < 0", Truth::True),
            ("bop:ETH_centralization > -0.2", Truth::False),
            ("stockpile_days:infantry_equipment < 30", Truth::True),
            ("stockpile_days:infantry_equipment_1 < 10", Truth::False),
            ("stockpile_days:motorized_equipment > 365", Truth::True),
            ("stockpile_days:artillery_equipment < 30", Truth::Unknown),
            ("stockpile_days:support_equipment < 30", Truth::Unknown),
            ("bop:ITA_other > 0", Truth::Unknown),
        ] {
            assert_eq!(eval(&f, json!(p)), want, "{p}");
        }
    }

    #[test]
    fn every_read_happens_once_per_context() {
        let f = game();
        let mut ctx = Ctx::new(&f, 0);
        for _ in 0..3 {
            check(&json!("stability > 0.1"), &mut ctx);
            check(&json!("war_support > 0.1"), &mut ctx);
            check(&json!("controls_state:550"), &mut ctx);
            check(&json!("focus_done:ITA_a"), &mut ctx);
            check(&json!("var:x > 1"), &mut ctx);
        }
        let calls = f.calls.borrow();
        let count = |n: &str| calls.iter().filter(|(c, _)| c == n).count();
        assert_eq!(count("hoi4_country_status"), 1);
        assert_eq!(count("hoi4_states"), 1);
        assert_eq!(count("hoi4_overview"), 1);
        assert_eq!(count("hoi4_focus_completed"), 1);
        assert_eq!(calls.len(), 4, "{calls:?}");
    }

    #[test]
    fn phase_atoms_follow_the_context() {
        let f = game();
        let mut ctx = Ctx::new(&f, 0);
        assert_eq!(check(&json!("phase:war"), &mut ctx), Truth::False);
        ctx.phase = Some("war".into());
        assert_eq!(check(&json!("phase:war"), &mut ctx), Truth::True);
        assert!(f.calls.borrow().is_empty());
    }

    #[test]
    fn dates_match_the_game_calendar() {
        assert_eq!(
            date_hour("1936.01.01"),
            crate::calendar::encode(1936, 1, 1).ok().map(i64::from)
        );
        assert_eq!(
            date_hour("1939-9-1.12"),
            crate::calendar::encode(1939, 9, 1).ok().map(i64::from)
        );
        assert_eq!(date_hour("1936.02.30"), None);
        assert_eq!(date_hour("36.1.1"), None);
        assert_eq!(date_hour("soon"), None);
    }
}
