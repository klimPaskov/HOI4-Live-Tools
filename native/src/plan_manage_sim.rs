//! Production-balance simulation: the executor's own training gate, stockpile rule and balance
//! rule (`training_step`, `stockpile_rule`, `balance_rule`) run against a small day-by-day model
//! of a country's land production, 1 January 1936 to 1 September 1939.
//!
//! The model is seeded with what Germany's lines measured live (two 1936-1938 runs): output per
//! factory at 100% efficiency, the efficiency cap over time, the output bonus from industry
//! research, the start lines, stock and fielded divisions; military factories arrive on the
//! dates of a Germany plan's own economy forecast. Each game day the lines produce, divisions
//! in training draw equipment in queue order and deploy once trained and fully equipped, and
//! the executor ticks three times. The rules read the same tool shapes the live tools answer
//! (hoi4_stockpile, hoi4_supply, hoi4_production_lines, hoi4_economy, hoi4_division_templates,
//! hoi4_template_get) and their orders change the model.
//!
//! Pass: no equipment the template needs sits at a large surplus while another is the binding
//! shortage for more than a few weeks, training is blocked only briefly, and the equipped
//! divisions meet the game AI's own Germany on its dates.
use super::*;
use std::cell::RefCell;
use std::collections::VecDeque;

const MAIN: &str = "Infanterie-Division";
/// One division of the main template: (archetype, units).
const MAIN_NEED: [(&str, f64); 3] = [
    ("infantry_equipment", 910.0),
    ("support_equipment", 30.0),
    ("artillery_equipment", 12.0),
];
/// The start's other divisions (armour, cavalry, mountain), each.
const OTHER_NEED: [(&str, f64); 5] = [
    ("infantry_equipment", 510.0),
    ("support_equipment", 28.0),
    ("artillery_equipment", 10.0),
    ("motorized_equipment", 100.0),
    ("light_tank_chassis", 120.0),
];
const START_MAIN: usize = 24;
/// Trucks the start's motorised divisions lack (live: 598 of 605 fielded, both runs).
const TRUCKS_SHORT: f64 = 7.0;
const START_OTHER: usize = 6;
const TRAINING_DAYS: u32 = 120;
/// Units a factory makes a day at 100% efficiency before research bonuses (measured: rifles
/// 52.9/day on 10 factories at 50%, support 1.32 on 2, artillery 0.78 on 1, trucks 1.95 on 2,
/// light tank chassis 0.8 on 2; the same bonus factor fits every line through 1937).
const BASE: [(&str, f64); 8] = [
    ("infantry_equipment", 10.58),
    ("support_equipment", 1.32),
    ("artillery_equipment", 1.56),
    ("motorized_equipment", 1.95),
    ("light_tank_chassis", 0.8),
    ("small_plane_airframe", 0.5),
    ("small_plane_cas_airframe", 0.5),
    ("medium_plane_airframe", 0.3),
];
/// Start efficiency of a factory new to a line (BASE_FACTORY_START_EFFICIENCY_FACTOR).
const START_EFF: f64 = 10.0;
/// Daily gain factor (BASE_FACTORY_EFFICIENCY_GAIN x BASE_FACTORY_EFFICIENCY_BALANCE_FACTOR).
const GAIN: f64 = 0.1;

/// Military factories by date: the plan's economy forecast (civilian first, switch 1939.5.16).
const FACTORIES: [(&str, i64); 16] = [
    ("1936.1.1", 28),
    ("1936.10.1", 30),
    ("1936.12.31", 38),
    ("1938.3.31", 39),
    ("1938.6.30", 40),
    ("1938.9.29", 43),
    ("1938.12.29", 45),
    ("1939.6.29", 50),
    ("1939.7.15", 53),
    ("1939.8.1", 56),
    ("1939.9.1", 59),
    ("1939.9.28", 63),
    ("1939.12.28", 89),
    ("1940.3.29", 105),
    ("1940.6.28", 119),
    ("1941.1.1", 174),
];
/// Efficiency cap by date (measured: rifles held at 53, 68 and 78).
const CAP: [(&str, f64); 3] = [("1936.1.1", 53.0), ("1936.6.1", 68.0), ("1937.5.1", 78.0)];
/// Output bonus by date (measured on every line: +8% from autumn 1936, +21% from autumn 1937;
/// held there, later research not counted).
const BONUS: [(&str, f64); 3] = [("1936.1.1", 0.0), ("1936.9.1", 0.08), ("1937.9.1", 0.21)];
/// The game AI's Germany: divisions on these dates.
const AI_MARKS: [(&str, usize); 4] = [
    ("1937.3.1", 51),
    ("1938.1.1", 66),
    ("1938.10.1", 96),
    ("1939.9.1", 129),
];

fn hour(date: &str) -> GameHour {
    crate::plan_predicate::date_hour(date).expect("date")
}

fn by_date<T: Copy>(table: &[(&str, T)], now: GameHour) -> T {
    let mut value = table[0].1;
    for (date, v) in table {
        if hour(date) <= now {
            value = *v;
        }
    }
    value
}

/// Military factories on day `now`: linear between the forecast's dates.
fn factories_at(now: GameHour) -> i64 {
    for pair in FACTORIES.windows(2) {
        let (a, b) = (hour(pair[0].0), hour(pair[1].0));
        if now < b {
            let t = ((now - a).max(0) as f64) / ((b - a) as f64);
            return pair[0].1 + ((pair[1].1 - pair[0].1) as f64 * t).floor() as i64;
        }
    }
    FACTORIES[FACTORIES.len() - 1].1
}

fn base(archetype: &str) -> f64 {
    BASE.iter()
        .find(|(a, _)| *a == archetype)
        .map_or(0.5, |(_, b)| *b)
}

struct Line {
    index: i64,
    equipment: String,
    archetype: String,
    requested: i64,
    active: i64,
    /// Efficiency of each active factory slot, percent.
    slots: Vec<f64>,
}

struct Training {
    days: u32,
    have: BTreeMap<String, f64>,
}

struct Game {
    now: GameHour,
    lines: Vec<Line>,
    next_index: i64,
    stock: BTreeMap<String, f64>,
    made: BTreeMap<String, f64>,
    /// Stock at the end of each of the last 31 days (the tool's 30-day trend).
    history: VecDeque<BTreeMap<String, f64>>,
    main: usize,
    other: usize,
    queue: Vec<Training>,
    trained_today: bool,
    orders: Vec<String>,
    /// Factories taken off a line by an order (each restarts at the start efficiency).
    moved: i64,
}

impl Game {
    fn new(now: GameHour) -> Game {
        let mut g = Game {
            now,
            lines: Vec::new(),
            next_index: 0,
            stock: BTreeMap::new(),
            made: BTreeMap::new(),
            history: VecDeque::new(),
            main: START_MAIN,
            other: START_OTHER,
            queue: Vec::new(),
            trained_today: false,
            orders: Vec::new(),
            moved: 0,
        };
        // the start lines after the day-0 assignment (live: rifles 10 -> 16, support 2 -> 3);
        // the start lines' history slots stand at 50%
        for (equipment, factories) in [
            ("infantry_equipment_1", 16),
            ("support_equipment_1", 3),
            ("artillery_equipment_1", 1),
            ("light_tank_chassis_2", 2),
            ("motorized_equipment_1", 2),
            ("small_plane_airframe_1", 1),
            ("small_plane_cas_airframe_1", 1),
            ("medium_plane_airframe_1", 1),
        ] {
            g.add_line(equipment, factories, 50.0);
        }
        for (a, s) in [
            ("infantry_equipment", 52.0),
            ("support_equipment", 1.0),
            ("artillery_equipment", 1.0),
            ("motorized_equipment", 2.0),
            ("light_tank_chassis", 1.0),
        ] {
            g.stock.insert(a.into(), s);
        }
        g.allocate();
        g
    }

    fn add_line(&mut self, equipment: &str, factories: i64, efficiency: f64) {
        self.lines.push(Line {
            index: self.next_index,
            equipment: equipment.into(),
            archetype: family(equipment).into(),
            requested: factories,
            active: 0,
            slots: vec![efficiency; factories.max(0) as usize],
        });
        self.next_index += 1;
    }

    /// The game gives the factories to the lines top first; a slot new to a line starts at the
    /// start efficiency, a slot that leaves is gone (a moved factory restarts).
    fn allocate(&mut self) {
        let mut free = factories_at(self.now);
        for line in &mut self.lines {
            line.active = line.requested.min(free).max(0);
            free -= line.active;
            let n = line.active as usize;
            if line.slots.len() > n {
                line.slots.truncate(n);
            }
            while line.slots.len() < n {
                line.slots.push(START_EFF);
            }
        }
    }

    fn need(&self, archetype: &str) -> (f64, f64) {
        let main = MAIN_NEED
            .iter()
            .find(|(a, _)| *a == archetype)
            .map_or(0.0, |(_, n)| *n);
        let other = OTHER_NEED
            .iter()
            .find(|(a, _)| *a == archetype)
            .map_or(0.0, |(_, n)| *n);
        (main, other)
    }

    fn fielded_need(&self, archetype: &str) -> f64 {
        let (m, o) = self.need(archetype);
        m * self.main as f64 + o * self.other as f64
    }

    /// One game day: production, training draws in queue order, deployment, efficiency growth.
    fn day(&mut self) {
        self.allocate();
        let cap = by_date(&CAP, self.now);
        let bonus = by_date(&BONUS, self.now);
        self.made.clear();
        for line in &mut self.lines {
            let per = base(&line.archetype) * (1.0 + bonus) / 100.0;
            let out: f64 = line.slots.iter().map(|e| per * e).sum();
            *self.made.entry(line.archetype.clone()).or_insert(0.0) += out;
            *self.stock.entry(line.archetype.clone()).or_insert(0.0) += out;
            for e in &mut line.slots {
                if *e < cap {
                    *e = (*e + GAIN * cap * cap / e.max(1.0) / 100.0).min(cap);
                }
            }
        }
        for t in &mut self.queue {
            for (a, n) in MAIN_NEED {
                let have = t.have.entry(a.into()).or_insert(0.0);
                let stock = self.stock.entry(a.into()).or_insert(0.0);
                let take = (n - *have).min(*stock).max(0.0);
                *have += take;
                *stock -= take;
            }
            t.days += 1;
        }
        let before = self.queue.len();
        self.queue.retain(|t| {
            !(t.days >= TRAINING_DAYS
                && MAIN_NEED
                    .iter()
                    .all(|(a, n)| t.have.get(*a).copied().unwrap_or(0.0) >= n - 1e-6))
        });
        self.main += before - self.queue.len();
        self.history.push_back(self.stock.clone());
        while self.history.len() > 31 {
            self.history.pop_front();
        }
        self.now += DAY;
        self.trained_today = false;
    }

    fn stockpile_read(&self) -> Value {
        let oldest = self.history.front();
        let span = self.history.len().saturating_sub(1) as f64;
        let mut rows = Vec::new();
        for (a, s) in &self.stock {
            let need = self.fielded_need(a);
            // the start's motorised divisions lacked 7 trucks all through both live runs
            // (598/605): a small shortfall the stockpile rule counts as felt at the front
            let short = if a == "motorized_equipment" {
                TRUCKS_SHORT
            } else {
                0.0
            };
            let net = oldest
                .filter(|_| span >= 1.0)
                .map(|o| (s - o.get(a).copied().unwrap_or(0.0)) / span);
            let (days, depleting) = match net {
                Some(n) if n < -1e-9 => (json!((s / -n * 100.0).round() / 100.0), json!(true)),
                Some(_) => (Value::Null, json!(false)),
                None => (Value::Null, Value::Null),
            };
            let factories: i64 = self
                .lines
                .iter()
                .filter(|l| &l.archetype == a)
                .map(|l| l.active)
                .sum();
            rows.push(json!({
                "archetype": a,
                "stockpile": s,
                "variants": [{"type": format!("{a}_1")}, {"type": format!("{a}_2")}],
                "produced_per_day": self.made.get(a).copied().unwrap_or(0.0),
                "factories": factories,
                "fielded_need": need,
                "fielded_have": need - short,
                "fielded_short": short,
                "net_per_day": net,
                "days_of_cover": days,
                "depleting": depleting,
            }));
        }
        let trucks = rows
            .iter()
            .find(|r| r["archetype"] == "motorized_equipment")
            .cloned();
        json!({"equipment": rows, "trucks": trucks})
    }

    fn supply_read(&self) -> Value {
        let mut divisions = Vec::new();
        let row = |need: &[(&str, f64)]| -> Vec<Value> {
            need.iter()
                .map(|(a, n)| json!({"archetype": a, "need": n, "have": n}))
                .collect()
        };
        for _ in 0..self.main {
            divisions.push(
                json!({"template": MAIN, "army": "A", "equipment_fill_min": 1.0,
                "equipment": row(&MAIN_NEED)}),
            );
        }
        for _ in 0..self.other {
            divisions.push(json!({"template": "Panzer-Division", "army": "B",
                "equipment_fill_min": 1.0, "equipment": row(&OTHER_NEED)}));
        }
        json!({ "divisions": divisions })
    }

    fn lines_read(&self) -> Value {
        let lines: Vec<Value> = self
            .lines
            .iter()
            .map(|l| {
                let eff = if l.slots.is_empty() {
                    START_EFF
                } else {
                    l.slots.iter().sum::<f64>() / l.slots.len() as f64
                };
                json!({"line_kind": "military", "equipment_id": l.equipment,
                    "entry_index": l.index,
                    "factory_counters": {"requested_factories_raw_signed": l.requested,
                        "active_factories_raw_signed": l.active},
                    "output": {"efficiency": eff, "efficiency_start": START_EFF}})
            })
            .collect();
        json!({ "lines": lines })
    }
}

struct Sim(RefCell<Game>);

impl Tools for Sim {
    fn call(&self, name: &str, args: &Value) -> Result<Value> {
        let mut g = self.0.borrow_mut();
        let verified = json!({"receipt": {"status": "Verified"}});
        match name {
            "hoi4_economy" => {
                Ok(json!({"factory_trigger_count": {"military": factories_at(g.now)}}))
            }
            "hoi4_production_lines" => Ok(if args["offset"].as_u64().unwrap_or(0) > 0 {
                json!({"lines": []})
            } else {
                g.lines_read()
            }),
            "hoi4_stockpile" => Ok(g.stockpile_read()),
            "hoi4_supply" => Ok(g.supply_read()),
            "hoi4_division_templates" => Ok(json!({"templates": [
                {"template": MAIN, "queued_lines": g.queue.len()}]})),
            "hoi4_template_get" => {
                let need: serde_json::Map<String, Value> = MAIN_NEED
                    .iter()
                    .map(|(a, n)| (a.to_string(), json!(n)))
                    .collect();
                Ok(json!({"templates": [{"totals": {"equipment_need": need},
                    "equipment_variants": MAIN_NEED.iter()
                        .map(|(a, _)| json!({"archetype": a, "variant": format!("{a}_1")}))
                        .collect::<Vec<_>>()}]}))
            }
            "hoi4_production_set" => {
                let (index, factories) = (args["line"].as_i64(), args["factories"].as_i64());
                let at = g.now;
                g.orders.push(format!(
                    "{at} set {} {}",
                    text(&args["equipment"]),
                    factories.unwrap_or(-1)
                ));
                match factories {
                    Some(0) => g.lines.retain(|l| Some(l.index) != index),
                    Some(f) => {
                        let mut taken = 0;
                        if let Some(l) = g.lines.iter_mut().find(|l| Some(l.index) == index) {
                            taken = (l.active - f).max(0);
                            l.requested = f;
                        }
                        g.moved += taken;
                    }
                    None => {}
                }
                g.allocate();
                Ok(verified)
            }
            "hoi4_production_add" => {
                let equipment = text(&args["equipment"]).to_string();
                let equipment = if family(&equipment) == equipment {
                    format!("{equipment}_1")
                } else {
                    equipment
                };
                let factories = args["factories"].as_i64().unwrap_or(1);
                let at = g.now;
                g.orders.push(format!("{at} add {equipment} {factories}"));
                g.add_line(&equipment, 0, START_EFF);
                if let Some(l) = g.lines.last_mut() {
                    l.requested = factories;
                }
                g.allocate();
                Ok(verified)
            }
            "hoi4_train" => {
                let count = args["count"].as_u64().unwrap_or(1);
                for _ in 0..count {
                    g.queue.push(Training {
                        days: 0,
                        have: BTreeMap::new(),
                    });
                }
                g.trained_today = true;
                Ok(verified)
            }
            other => Err(crate::error::Error::new(
                "unknown_tool",
                format!("the simulation does not model {other}"),
            )),
        }
    }
}

/// The plan subset the three rules read (Germany FAIR plan 1117b3: eight training entries of
/// the main template with their stock guards, cap 125; the manage keys; production targets).
fn plan() -> Value {
    let entry = |queue: u64, state: i64| {
        json!({"template": MAIN, "queue": queue, "batch": 2, "cap": 125, "when": null,
            "deploy_state": state,
            "only_if_stockpile": {"infantry_equipment": 950, "support_equipment": 60,
                "artillery_equipment": 24}})
    };
    json!({
        "manage": {"stockpile_days_critical": 7, "stockpile_days_warning": 30,
            "trucks_ratio": 1.5, "rebalance_step": 3, "shift_factories": true},
        "stockpile_targets": {"infantry_equipment": 30, "artillery_equipment": 20,
            "support_equipment": 20, "motorized_equipment": 30, "medium_tank_chassis": 20},
        "training_rules": {"only_if_stockpile": {"infantry_equipment": 700,
            "support_equipment": 40}},
        "training": [entry(5, 66), entry(4, 67), entry(4, 68), entry(3, 5), entry(3, 63),
            entry(4, 51), entry(3, 978), entry(3, 42)],
        "production": [
            {"equipment": "infantry_equipment_1", "factories": 16},
            {"equipment": "artillery_equipment_1", "factories": 2},
            {"equipment": "support_equipment_1", "factories": 4},
            {"equipment": "motorized_equipment_1", "factories": 2},
            {"equipment": "light_tank_chassis_2", "factories": 1},
            {"equipment": "medium_tank_chassis_1", "factories": 6},
            {"equipment": "small_plane_airframe_1", "factories": 1},
            {"equipment": "small_plane_cas_airframe_1", "factories": 1},
            {"equipment": "medium_plane_airframe_1", "factories": 1}],
        "production_priority": ["infantry_equipment_1", "artillery_equipment_1",
            "support_equipment_1", "motorized_equipment_1", "light_tank_chassis_2"],
    })
}

struct Outcome {
    /// Equipped divisions on each AI mark date.
    marks: Vec<(String, usize, usize)>,
    /// Longest run of days a needed equipment held 10+ divisions' worth while another bound.
    worst_imbalance: (u32, String),
    /// Longest run of days training was held while the queue had room and the cap was not met.
    worst_block: u32,
    blocked_days: u32,
    open_days: u32,
    moved: i64,
    samples: Vec<String>,
}

fn simulate() -> Outcome {
    let start = hour("1936.1.1");
    let end = hour("1939.9.1");
    let tools = Sim(RefCell::new(Game::new(start)));
    let plan = plan();
    let mut state = State::default();
    let target_queue: usize = list(&plan, "training")
        .iter()
        .map(|t| t["queue"].as_u64().unwrap_or(0) as usize)
        .sum();
    let mut out = Outcome {
        marks: Vec::new(),
        worst_imbalance: (0, String::new()),
        worst_block: 0,
        blocked_days: 0,
        open_days: 0,
        moved: 0,
        samples: Vec::new(),
    };
    let (mut imbalance, mut block) = (0u32, 0u32);
    let mut day = 0u32;
    loop {
        let now = tools.0.borrow().now;
        if now >= end {
            break;
        }
        for h in [1, 9, 17] {
            let at = now + h;
            if training_step(&mut state, &tools, &plan, "lease", at)
                .expect("training")
                .is_some()
            {
                continue;
            }
            if stockpile_rule(&mut state, &tools, &plan, "lease", at)
                .expect("stockpile")
                .is_some()
            {
                continue;
            }
            let _ = balance_rule(&mut state, &tools, &plan, "lease", at).expect("balance");
        }
        {
            let g = tools.0.borrow();
            // equipment a division needs: divisions' worth in stock, and what the queue lacks
            let worth: Vec<(&str, f64)> = MAIN_NEED
                .iter()
                .map(|(a, n)| (*a, g.stock.get(*a).copied().unwrap_or(0.0) / n))
                .collect();
            let lacking: Vec<&str> = MAIN_NEED
                .iter()
                .filter(|(a, n)| {
                    g.queue
                        .iter()
                        .any(|t| t.have.get(*a).copied().unwrap_or(0.0) < n - 1e-6)
                })
                .map(|(a, _)| *a)
                .collect();
            let room = g.main + g.queue.len() < 125 && g.queue.len() < target_queue;
            // held: the queue has room, nothing was queued, and the divisions in training lack
            // nothing (the queue is not what takes the output)
            let held = room && !g.trained_today && lacking.is_empty();
            let binding: Option<&str> = if held {
                worth
                    .iter()
                    .min_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal))
                    .filter(|(_, w)| *w < 2.0)
                    .map(|(a, _)| *a)
            } else {
                lacking
                    .iter()
                    .min_by(|x, y| {
                        let wx = worth.iter().find(|w| w.0 == **x).map_or(0.0, |w| w.1);
                        let wy = worth.iter().find(|w| w.0 == **y).map_or(0.0, |w| w.1);
                        wx.partial_cmp(&wy).unwrap_or(std::cmp::Ordering::Equal)
                    })
                    .copied()
            };
            let surplus = worth
                .iter()
                .filter(|(a, w)| Some(*a) != binding && *w >= 10.0)
                .map(|(a, w)| format!("{a} {w:.0} divisions"))
                .next();
            match (binding, surplus) {
                (Some(b), Some(s)) => {
                    imbalance += 1;
                    if imbalance > out.worst_imbalance.0 {
                        out.worst_imbalance = (
                            imbalance,
                            format!("{b} binds while {s} in stock, day {day}"),
                        );
                    }
                }
                _ => imbalance = 0,
            }
            if g.main + g.queue.len() < 125 {
                out.open_days += 1;
            }
            if held && binding.is_some() {
                block += 1;
                out.blocked_days += 1;
                out.worst_block = out.worst_block.max(block);
            } else {
                block = 0;
            }
            if day % 61 == 0 {
                let f = |a: &str| -> i64 {
                    g.lines
                        .iter()
                        .filter(|l| l.archetype == a)
                        .map(|l| l.active)
                        .sum()
                };
                out.samples.push(format!(
                    "day {day}: divisions {} (+{} training), factories {} rifles {} support {} artillery {}, stock {:.0}/{:.0}/{:.0}",
                    g.main + g.other,
                    g.queue.len(),
                    factories_at(g.now),
                    f("infantry_equipment"),
                    f("support_equipment"),
                    f("artillery_equipment"),
                    worth[0].1, worth[1].1, worth[2].1
                ));
            }
        }
        tools.0.borrow_mut().day();
        day += 1;
        let g = tools.0.borrow();
        for (date, want) in AI_MARKS {
            if g.now == hour(date) {
                out.marks.push((date.into(), g.main + g.other, want));
            }
        }
    }
    out.moved = tools.0.borrow().moved;
    if std::env::var("SIM_ORDERS").is_ok() {
        for o in &tools.0.borrow().orders {
            let (h, rest) = o.split_once(' ').unwrap_or((o, ""));
            let d = (h.parse::<i64>().unwrap_or(0) - start) / DAY;
            eprintln!("order day {d}: {rest}");
        }
    }
    out
}

#[test]
fn production_balance_meets_the_ai_marks_without_lopsided_lines() {
    let o = simulate();
    let report = format!(
        "marks {:?}; worst imbalance {} days ({}); longest block {} days, blocked {} of {} open days; factories moved off a line {}\n{}",
        o.marks,
        o.worst_imbalance.0,
        o.worst_imbalance.1,
        o.worst_block,
        o.blocked_days,
        o.open_days,
        o.moved,
        o.samples.join("\n")
    );
    eprintln!("{report}");
    assert_eq!(o.marks.len(), AI_MARKS.len(), "{report}");
    for (date, have, want) in &o.marks {
        assert!(
            have >= want,
            "{date}: {have} equipped divisions, the AI has {want}\n{report}"
        );
    }
    assert!(o.worst_imbalance.0 <= 28, "lopsided production\n{report}");
    assert!(o.worst_block <= 45, "training held too long\n{report}");
    assert!(o.moved <= 20, "factories moved back and forth\n{report}");
}
