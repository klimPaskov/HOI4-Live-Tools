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
//!   resistance, compliance) are fractions 0..1; a value above 1 or written with % is a
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
//!                               the enemies' hoi4_states reads (other provinces: unknown)
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
//!   capitulated:<TAG>           hoi4_wars + hoi4_events + hoi4_states: no longer at war with TAG and
//!                               (a peace conference is open or TAG owns no state)
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

/// Who holds province `p`: looked up in the victory points and the provinces held by others of
/// the player's and the enemies' hoi4_states reads.
fn province_controller(ctx: &mut Ctx, p: i64) -> Read<String> {
    let mut tags = vec![ctx.me()?];
    if let Ok(enemies) = enemies(ctx) {
        tags.extend(enemies);
    }
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
    Err(format!(
        "province {p} not found in the own or enemy state reads (only victory points and provinces held by others are listed; state_fully_controlled:<state> reads every province)"
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
        "capitulated" => {
            // What a player sees: the war with TAG is over and either the peace conference that
            // follows a capitulation is open or TAG no longer exists (annexed). A white peace
            // leaves TAG existing with no conference, so it does not count.
            let tag = tag_arg(atom)?;
            if enemies(ctx)?.contains(&tag) {
                return Ok(Reading::Flag(false));
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
