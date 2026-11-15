//! The encirclement operation against recorded geometry: the fixtures are
//! `hoi4_encirclement_candidates` answers computed over the game's own map (land adjacency,
//! terrain, state facts) for three fronts, recorded by `record_encirclement_fixtures`
//! (launch/macos/front_plan/tests.rs). State centres are the map's own.
//!   poland_start  GER-POL on the 1936 borders: the corridor pocket (Danzig, Poznań, Gdynia)
//!                 closes at Łódz and Płock from Silesia and East Prussia.
//!   poland_mid    GER holds Gdynia, Płock, Katowice and Kraków: Poznań, Łódz and Kielce are
//!                 cut off by taking Warsaw and Lublin; Danzig is already cut off.
//!   ethiopia_mid  ITA holds Tigray, Hararghe and Bale: Sidamo and Illubabor-Kaffa close at
//!                 Shewa and Welega, both arms into Shewa first.
use super::super::{war_step, GameHour, State, Tools, DAY};
use super::*;
use crate::error::Result;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

const START: GameHour = 1_000_000;

const POLAND_START: &str = include_str!("poland_start.json");
const POLAND_MID: &str = include_str!("poland_mid.json");
const ETHIOPIA_MID: &str = include_str!("ethiopia_mid.json");

/// State centres on the game map (map units), as the map's provinces give them.
const CENTERS: &[(i32, f64, f64)] = &[
    (5, 3130.0, 1598.0),
    (10, 3132.0, 1557.0),
    (63, 3056.0, 1595.0),
    (66, 3060.0, 1544.0),
    (67, 3088.0, 1531.0),
    (68, 3042.0, 1570.0),
    (85, 3100.0, 1606.0),
    (86, 3072.0, 1566.0),
    (87, 3100.0, 1553.0),
    (88, 3134.0, 1516.0),
    (89, 3184.0, 1493.0),
    (90, 3128.0, 1535.0),
    (91, 3194.0, 1510.0),
    (92, 3162.0, 1545.0),
    (93, 3204.0, 1534.0),
    (94, 3205.0, 1560.0),
    (95, 3199.0, 1588.0),
    (96, 3226.0, 1618.0),
    (97, 3163.0, 1585.0),
    (98, 3121.0, 1575.0),
    (762, 3100.0, 1528.0),
    (784, 3202.0, 1615.0),
    (807, 3093.0, 1591.0),
    (271, 3412.0, 787.0),
    (550, 3409.0, 898.0),
    (559, 3539.0, 743.0),
    (835, 3478.0, 782.0),
    (836, 3453.0, 746.0),
    (837, 3409.0, 732.0),
    (838, 3358.0, 765.0),
    (839, 3361.0, 798.0),
    (840, 3379.0, 824.0),
    (841, 3387.0, 851.0),
    (842, 3415.0, 873.0),
    (843, 3418.0, 839.0),
    (844, 3457.0, 664.0),
];

struct FakeArmy {
    name: &'static str,
    state: i32,
    divisions: u64,
    supply: f64,
    orders: u64,
    executing: bool,
}

/// A front: who owns and controls each state, the armies, and the recorded pocket read.
struct Front {
    me: &'static str,
    enemy: &'static str,
    owner: BTreeMap<i32, &'static str>,
    controller: RefCell<BTreeMap<i32, &'static str>>,
    armies: RefCell<Vec<FakeArmy>>,
    read: &'static str,
    /// the read shows no pocket (the enemy found another way out)
    no_pockets: Cell<bool>,
    calls: RefCell<Vec<(String, Value)>>,
}

impl Front {
    fn new(
        me: &'static str,
        enemy: &'static str,
        own: &[i32],
        theirs: &[i32],
        read: &'static str,
        armies: Vec<FakeArmy>,
    ) -> Self {
        let mut owner = BTreeMap::new();
        for s in own {
            owner.insert(*s, me);
        }
        for s in theirs {
            owner.insert(*s, enemy);
        }
        Self {
            me,
            enemy,
            controller: RefCell::new(owner.clone()),
            owner,
            armies: RefCell::new(armies),
            read,
            no_pockets: Cell::new(false),
            calls: RefCell::new(vec![]),
        }
    }
    /// Germany and Poland, 1936 borders (Danzig counted Polish as the install's owners have it).
    fn poland(read: &'static str, taken: &[i32], armies: Vec<FakeArmy>) -> Self {
        let f = Self::new(
            "GER",
            "POL",
            &[5, 63, 66, 67, 68],
            &[
                10, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97, 98, 762, 784, 807,
            ],
            read,
            armies,
        );
        f.take(taken);
        f
    }
    fn ethiopia(taken: &[i32], armies: Vec<FakeArmy>) -> Self {
        let f = Self::new(
            "ITA",
            "ETH",
            &[550, 559, 844],
            &[271, 835, 836, 837, 838, 839, 840, 841, 842, 843],
            ETHIOPIA_MID,
            armies,
        );
        f.take(taken);
        f
    }
    fn take(&self, states: &[i32]) {
        for s in states {
            self.controller.borrow_mut().insert(*s, self.me);
        }
    }
    fn lose(&self, states: &[i32]) {
        for s in states {
            self.controller.borrow_mut().insert(*s, self.enemy);
        }
    }
    fn army(&self, name: &str, f: impl FnOnce(&mut FakeArmy)) {
        let mut armies = self.armies.borrow_mut();
        f(armies.iter_mut().find(|a| a.name == name).expect("army"));
    }
    fn actions(&self) -> Vec<(String, Value)> {
        self.calls
            .borrow()
            .iter()
            .filter(|(_, a)| a.get("request_id").is_some())
            .cloned()
            .collect()
    }
    fn reads(&self, tool: &str) -> Vec<Value> {
        self.calls
            .borrow()
            .iter()
            .filter(|(n, _)| n == tool)
            .map(|(_, a)| a.clone())
            .collect()
    }
    fn rows(&self, tag: &str) -> Value {
        let controller = self.controller.borrow();
        let rows: Vec<Value> = self
            .owner
            .iter()
            .filter(|(_, o)| **o == tag)
            .filter_map(|(s, o)| {
                let (_, x, z) = CENTERS.iter().find(|c| c.0 == *s)?;
                Some(
                    json!({"state_id": s, "owner": o, "controller": controller[s], "x": x, "z": z}),
                )
            })
            .collect();
        json!({ "states": rows })
    }
}

impl Tools for Front {
    fn call(&self, name: &str, args: &Value) -> Result<Value> {
        self.calls
            .borrow_mut()
            .push((name.to_string(), args.clone()));
        if args.get("request_id").is_some() {
            let army = text(&args["army"]).to_string();
            match name {
                "hoi4_army_plan" => {
                    self.army(&army, |a| a.executing = args["execute"] == json!(true))
                }
                "hoi4_army_front" => self.army(&army, |a| a.orders = a.orders.max(1)),
                "hoi4_army_offensive" => self.army(&army, |a| a.orders = 2),
                _ => {}
            }
            return Ok(json!({"receipt": {"status": "Verified"}}));
        }
        Ok(match name {
            "hoi4_overview" => json!({"player": self.me}),
            "hoi4_wars" => json!({"wars": {"validated": true, "enemies": [self.enemy]}}),
            "hoi4_construction_queue" => json!({"lines": []}),
            "hoi4_states" => {
                let q = text(&args["query"]);
                if q == self.me || q == self.enemy {
                    self.rows(q)
                } else {
                    json!({"states": []})
                }
            }
            "hoi4_armies" => {
                let armies: Vec<Value> = self
                    .armies
                    .borrow()
                    .iter()
                    .map(|a| {
                        json!({"army": a.name, "division_count": a.divisions, "main_state": a.state,
                            "battle_plan_orders": a.orders, "plan_executing": a.executing,
                            "planning": {"mean": 0.3}, "commander": "X",
                            "threat": {"share": 0.1, "encirclement_risk": false}})
                    })
                    .collect();
                json!({"armies": armies, "unassigned_by_state": []})
            }
            "hoi4_supply" => {
                let armies: Vec<Value> = self
                    .armies
                    .borrow()
                    .iter()
                    .map(|a| json!({"army": a.name, "organisation_mean": 50.0, "supply_percent_mean": a.supply}))
                    .collect();
                json!({"armies": armies, "divisions": [], "provinces": [], "supply_nodes": []})
            }
            "hoi4_encirclement_candidates" => {
                if self.no_pockets.get() {
                    json!({"pockets": [], "pockets_found": 0})
                } else {
                    serde_json::from_str(self.read).unwrap()
                }
            }
            _ => json!({}),
        })
    }
}

fn army(name: &'static str, state: i32, divisions: u64) -> FakeArmy {
    FakeArmy {
        name,
        state,
        divisions,
        supply: 90.0,
        orders: 0,
        executing: false,
    }
}

fn plan(enemy: &str, encircle: Value) -> Value {
    json!({"war": {"enemies": [enemy], "planning_wait": false, "second_direction": false,
        "bound": false, "supply_relief": false, "encircle": encircle}})
}

fn step(state: &mut State, front: &Front, plan: &Value, now: GameHour) -> Option<String> {
    war_step(state, front, plan, "l", now).unwrap()
}

/// Ticks every 3 game hours for `hours`; the lines of the orders sent.
fn run(state: &mut State, front: &Front, plan: &Value, from: GameHour, hours: i64) -> Vec<String> {
    (0..hours / 3)
        .filter_map(|i| step(state, front, plan, from + i * 3))
        .collect()
}

fn note(state: &State, key: &str) -> String {
    plan_war::digest(state)
        .and_then(|d| d[key].as_str().map(str::to_string))
        .unwrap_or_default()
}

fn sent(front: &Front, army: &str, tool: &str) -> Vec<Value> {
    front
        .actions()
        .into_iter()
        .filter(|(n, a)| n == tool && a["army"] == json!(army))
        .map(|(_, a)| a)
        .collect()
}

fn mid_armies() -> Vec<FakeArmy> {
    vec![
        army("Army 1", 98, 12),
        army("Army 2", 762, 14),
        army("Army 3", 807, 10),
    ]
}

#[test]
fn the_best_pocket_gets_two_pincers_into_its_neck() {
    let front = Front::poland(POLAND_MID, &[807, 98, 762, 88], mid_armies());
    let mut state = State::default();
    let plan = plan("POL", json!({}));
    let line = step(&mut state, &front, &plan, START).unwrap();
    // the largest army goes first, for its arm: Kraków into Lublin
    assert!(
        line.starts_with("war front and offensive Army 2 -> state 92"),
        "{line}"
    );
    let n = note(&state, "encircle");
    assert!(
        n.starts_with("closing pocket Poznań (86), Łódz (87), Kielce (90) (score 44.6) at the neck Warsaw (10), Lublin (92)"),
        "{n}"
    );
    assert!(n.contains("Army 1 from 98 into Warsaw (10)"), "{n}");
    assert!(n.contains("Army 2 from 88 into Lublin (92)"), "{n}");
    let read = &front.reads("hoi4_encirclement_candidates")[0];
    assert_eq!(
        (read["enemy"].clone(), read["max_gap"].clone()),
        (json!(["POL"]), json!(2))
    );
    run(&mut state, &front, &plan, START + 3, 24);
    assert_eq!(
        sent(&front, "Army 1", "hoi4_army_front")[0]["state_id"],
        json!(10)
    );
    assert_eq!(
        sent(&front, "Army 1", "hoi4_army_offensive")[0]["state_id"],
        json!(10)
    );
    assert_eq!(
        sent(&front, "Army 2", "hoi4_army_offensive")[0]["state_id"],
        json!(92)
    );
    // the third army is the anvil: a front against the pocket, no offensive
    assert!(n.ends_with("; anvil Army 3"), "{n}");
    assert_eq!(
        sent(&front, "Army 3", "hoi4_army_front")[0]["state_id"],
        json!(86)
    );
    assert!(sent(&front, "Army 3", "hoi4_army_offensive").is_empty());
}

#[test]
fn both_pincers_execute_together() {
    let front = Front::poland(POLAND_MID, &[807, 98, 762, 88], mid_armies());
    let mut state = State::default();
    let plan = plan("POL", json!({}));
    step(&mut state, &front, &plan, START);
    run(&mut state, &front, &plan, START + 3, 48);
    let order: Vec<(String, String)> = front
        .actions()
        .into_iter()
        .filter(|(_, a)| a["army"] == json!("Army 1") || a["army"] == json!("Army 2"))
        .map(|(n, a)| (n, text(&a["army"]).to_string()))
        .collect();
    let first_execute = order
        .iter()
        .position(|(n, _)| n == "hoi4_army_plan")
        .expect("an execute");
    let offensives = order[..first_execute]
        .iter()
        .filter(|(n, _)| n == "hoi4_army_offensive")
        .count();
    assert_eq!(
        offensives, 2,
        "both lines drawn before any arm goes: {order:?}"
    );
    assert_eq!(sent(&front, "Army 1", "hoi4_army_plan").len(), 1);
    assert_eq!(sent(&front, "Army 2", "hoi4_army_plan").len(), 1);
}

#[test]
fn a_ready_arm_waits_for_the_other_at_most_sync_days() {
    let front = Front::poland(POLAND_MID, &[807, 98, 762, 88], mid_armies());
    let mut state = State::default();
    let plan = plan("POL", json!({"sync_days": 2}));
    step(&mut state, &front, &plan, START);
    let limits = plan_war::Limits::of(&plan);
    let roster = front.call("hoi4_armies", &json!({})).unwrap();
    let why = launch_wait(&mut state, &plan, "Army 2", &roster, &limits, START).unwrap();
    assert!(why.contains("waits for Army 1"), "{why}");
    assert!(launch_wait(&mut state, &plan, "Army 2", &roster, &limits, START + DAY).is_some());
    // after sync_days the ready arm goes alone
    assert!(launch_wait(
        &mut state,
        &plan,
        "Army 2",
        &roster,
        &limits,
        START + 2 * DAY
    )
    .is_none());
    // an arm whose partner already executes goes at once
    let mut state2 = State::default();
    step(&mut state2, &front, &plan, START);
    front.army("Army 1", |a| a.executing = true);
    let roster = front.call("hoi4_armies", &json!({})).unwrap();
    assert!(launch_wait(&mut state2, &plan, "Army 2", &roster, &limits, START).is_none());
    // an army outside the operation never waits
    assert!(launch_wait(&mut state2, &plan, "Army 3", &roster, &limits, START).is_none());
}

/// Runs the operation until both pincers execute, then hands them Warsaw and Lublin.
fn close_ring(front: &Front, state: &mut State, plan: &Value) -> GameHour {
    run(state, front, plan, START, 60);
    front.take(&[10, 92]);
    front.army("Army 1", |a| {
        a.state = 10;
        a.executing = true;
    });
    front.army("Army 2", |a| {
        a.state = 92;
        a.executing = true;
    });
    START + 3 * DAY
}

#[test]
fn a_closed_ring_is_held_by_the_arms_and_reduced_by_the_anvil_then_the_advance_resumes() {
    let front = Front::poland(POLAND_MID, &[807, 98, 762, 88], mid_armies());
    let mut state = State::default();
    let plan = plan("POL", json!({}));
    let at = close_ring(&front, &mut state, &plan);
    let lines = run(&mut state, &front, &plan, at, 18);
    let n = note(&state, "encircle");
    assert!(
        n.starts_with("ring closed around Poznań (86), Łódz (87), Kielce (90)"),
        "{n}"
    );
    // both arms stop their plans and face Białystok, the nearest enemy state outside the ring
    for arm in ["Army 1", "Army 2"] {
        let stops = sent(&front, arm, "hoi4_army_plan");
        assert_eq!(
            stops.last().unwrap()["execute"],
            json!(false),
            "{arm} {lines:?}"
        );
        assert_eq!(
            sent(&front, arm, "hoi4_army_front").last().unwrap()["state_id"],
            json!(97)
        );
    }
    // the anvil reduces the pocket from Gdynia: Poznań first
    assert_eq!(state.war_assign.get("Army 3"), Some(&86));
    assert_eq!(
        sent(&front, "Army 3", "hoi4_army_offensive")
            .last()
            .unwrap()["state_id"],
        json!(86)
    );
    // the holders get no further order while they hold
    let before = sent(&front, "Army 1", "hoi4_army_front").len();
    run(&mut state, &front, &plan, at + DAY, 12);
    assert_eq!(sent(&front, "Army 1", "hoi4_army_front").len(), before);
    // the pocket falls: the operation ends and every army is driven again
    front.take(&[86, 87, 90]);
    step(&mut state, &front, &plan, at + 2 * DAY);
    // (the next scan may start on Danzig in the same tick: the end is kept as encircle_last)
    let n = note(&state, "encircle_last");
    assert!(
        n.starts_with("pocket Poznań (86), Łódz (87), Kielce (90) reduced in"),
        "{n}"
    );
    assert!(state
        .encircle
        .op
        .as_ref()
        .is_none_or(|op| op.key != "86,87,90"));
}

#[test]
fn without_an_anvil_the_larger_arm_reduces_and_the_other_holds() {
    let front = Front::poland(POLAND_MID, &[807, 98, 762, 88], mid_armies());
    let mut state = State::default();
    let plan = plan("POL", json!({"armies": ["Army 1", "Army 2"]}));
    let at = close_ring(&front, &mut state, &plan);
    run(&mut state, &front, &plan, at, 18);
    // Army 1 (12 divisions) holds against Białystok; Army 2 (14) reduces from Lublin: Kielce
    assert_eq!(
        sent(&front, "Army 1", "hoi4_army_front").last().unwrap()["state_id"],
        json!(97)
    );
    assert_eq!(state.war_assign.get("Army 2"), Some(&90));
    assert_eq!(
        sent(&front, "Army 2", "hoi4_army_offensive")
            .last()
            .unwrap()["state_id"],
        json!(90)
    );
}

#[test]
fn a_lost_neck_state_sends_the_arms_back_in() {
    let front = Front::poland(POLAND_MID, &[807, 98, 762, 88, 10, 92], mid_armies());
    let mut state = State::default();
    let plan = plan("POL", json!({}));
    // Start the operation on the open neck, then close it.
    front.lose(&[10, 92]);
    step(&mut state, &front, &plan, START);
    front.take(&[10, 92]);
    run(&mut state, &front, &plan, START + 3, 6);
    assert!(note(&state, "encircle").starts_with("ring closed"));
    front.lose(&[92]);
    run(&mut state, &front, &plan, START + DAY, 6);
    let n = note(&state, "encircle");
    assert!(n.contains("broken at Lublin (92)"), "{n}");
    assert_eq!(state.war_assign.get("Army 1"), Some(&92));
    assert_eq!(state.war_assign.get("Army 2"), Some(&92));
}

#[test]
fn an_arm_short_of_supply_is_taken_by_another_army_or_the_pocket_waits() {
    let front = Front::poland(POLAND_MID, &[807, 98, 762, 88], mid_armies());
    front.army("Army 1", |a| a.supply = 60.0);
    let mut state = State::default();
    let plan = plan("POL", json!({}));
    step(&mut state, &front, &plan, START);
    let n = note(&state, "encircle");
    // Army 3 at Gdynia is near enough to Płock and supplied: it takes the Warsaw arm
    assert!(n.contains("Army 3 from 98 into Warsaw (10)"), "{n}");

    let front = Front::poland(POLAND_MID, &[807, 98, 762, 88], mid_armies());
    front.army("Army 1", |a| a.supply = 60.0);
    front.army("Army 3", |a| a.supply = 60.0);
    let mut state = State::default();
    step(&mut state, &front, &plan, START);
    let n = note(&state, "encircle");
    // no arm for Warsaw: the big pocket waits; Danzig, already cut off, is reduced instead
    assert!(
        n.starts_with("reducing pocket Danzig (85) (already cut off"),
        "{n}"
    );
    assert!(n.ends_with("Army 2"), "{n}");
}

#[test]
fn an_arm_that_starves_or_a_neck_not_closed_in_time_gives_the_pocket_up() {
    let front = Front::poland(POLAND_MID, &[807, 98, 762, 88], mid_armies());
    let mut state = State::default();
    let plan = plan("POL", json!({}));
    step(&mut state, &front, &plan, START);
    front.army("Army 2", |a| a.supply = 30.0);
    step(&mut state, &front, &plan, START + 3);
    let n = note(&state, "encircle");
    assert!(
        n.starts_with("gave up pocket Poznań (86), Łódz (87), Kielce (90): Army 2 is at 30% supply, below 35%"),
        "{n}"
    );
    assert!(state.encircle.shelved.contains_key("86,87,90"));

    let front = Front::poland(POLAND_MID, &[807, 98, 762, 88], mid_armies());
    let mut state = State::default();
    step(&mut state, &front, &plan, START);
    step(&mut state, &front, &plan, START + 31 * DAY);
    let n = note(&state, "encircle_last");
    assert!(n.contains("the neck was not closed in 30 days"), "{n}");
}

#[test]
fn a_pocket_the_read_no_longer_shows_is_given_up() {
    let front = Front::poland(POLAND_MID, &[807, 98, 762, 88], mid_armies());
    let mut state = State::default();
    let plan = plan("POL", json!({}));
    step(&mut state, &front, &plan, START);
    front.no_pockets.set(true);
    step(&mut state, &front, &plan, START + DAY + 1);
    assert!(
        note(&state, "encircle").contains("the read no longer shows the pocket"),
        "{}",
        note(&state, "encircle")
    );
}

#[test]
fn the_corridor_closes_from_silesia_and_east_prussia() {
    let front = Front::poland(
        POLAND_START,
        &[],
        vec![
            army("Army 1", 5, 12),
            army("Army 2", 63, 10),
            army("Army 3", 67, 14),
        ],
    );
    let mut state = State::default();
    let measured = plan("POL", json!({}));
    step(&mut state, &front, &measured, START);
    let n = note(&state, "encircle");
    assert!(n.contains("at the neck Łódz (87), Płock (98)"), "{n}");
    assert!(n.contains("Army 3 from 67 into Łódz (87)"), "{n}");
    assert!(n.contains("Army 1 from 5 into Płock (98)"), "{n}");
    // cautious: the read asks for one neck state at most and a coast is not accepted
    let front = Front::poland(
        POLAND_START,
        &[],
        vec![
            army("Army 1", 5, 12),
            army("Army 2", 63, 10),
            army("Army 3", 67, 14),
        ],
    );
    let mut state = State::default();
    step(
        &mut state,
        &front,
        &plan("POL", json!({"boldness": "cautious"})),
        START,
    );
    assert_eq!(
        front.reads("hoi4_encirclement_candidates")[0]["max_gap"],
        json!(1)
    );
    let n = note(&state, "encircle");
    // the corridor needs two neck states (more than cautious allows); with gap 2 it has a coast
    assert_eq!(
        n,
        "no pocket taken: pocket 85,86,807: 2 neck states, more than max_gap 1"
    );
    let front = Front::poland(
        POLAND_START,
        &[],
        vec![
            army("Army 1", 5, 12),
            army("Army 2", 63, 10),
            army("Army 3", 67, 14),
        ],
    );
    let mut state = State::default();
    let p = plan("POL", json!({"boldness": "cautious", "max_gap": 2}));
    step(&mut state, &front, &p, START);
    let n = note(&state, "encircle");
    assert_eq!(
        n,
        "no pocket taken: pocket 85,86,807: it has a coast (sea_escape false)"
    );
}

#[test]
fn ethiopia_both_arms_go_into_shewa_then_one_takes_welega() {
    let front = Front::ethiopia(
        &[842, 835, 836],
        vec![
            army("Army 1", 835, 8),
            army("Army 2", 836, 6),
            army("Army 3", 842, 12),
        ],
    );
    let mut state = State::default();
    let plan = plan("ETH", json!({}));
    step(&mut state, &front, &plan, START);
    let n = note(&state, "encircle");
    // Army 2 has 6 of the 8 divisions an arm into the mountains needs: Army 3 takes its arm
    assert!(n.contains("Army 1 from 835 into Shewa (271)"), "{n}");
    assert!(n.contains("Army 3 from 836 into Shewa (271)"), "{n}");
    run(&mut state, &front, &plan, START + 3, 12);
    assert_eq!(state.war_assign.get("Army 1"), Some(&271));
    assert_eq!(state.war_assign.get("Army 3"), Some(&271));
    // Shewa falls; Welega is the neck still open: both arms go for it
    front.take(&[271]);
    front.army("Army 1", |a| a.state = 271);
    run(&mut state, &front, &plan, START + 2 * DAY, 12);
    assert_eq!(state.war_assign.get("Army 1"), Some(&839));
    assert_eq!(state.war_assign.get("Army 3"), Some(&839));
}

#[test]
fn no_operation_when_off_in_a_hold_phase_or_with_direct_moves() {
    for p in [
        plan("POL", json!(false)),
        plan("POL", json!({"enabled": false})),
        {
            let mut p = plan("POL", json!({}));
            p["war"]["axes"] = json!([]);
            p
        },
        {
            let mut p = plan("POL", json!({}));
            p["war"]["direct_moves"] = json!(true);
            p
        },
    ] {
        let front = Front::poland(POLAND_MID, &[807, 98, 762, 88], mid_armies());
        let mut state = State::default();
        run(&mut state, &front, &p, START, 12);
        assert!(
            front.reads("hoi4_encirclement_candidates").is_empty(),
            "{p}"
        );
        assert!(state.encircle.op.is_none());
    }
}

#[test]
fn keep_free_and_the_armies_list_limit_what_is_committed() {
    let front = Front::poland(POLAND_MID, &[807, 98, 762, 88], mid_armies());
    let mut state = State::default();
    step(
        &mut state,
        &front,
        &plan("POL", json!({"keep_free": 2})),
        START,
    );
    // two arms would leave one driven army: the big pocket waits, Danzig needs only one army
    let n = note(&state, "encircle");
    assert!(n.starts_with("reducing pocket Danzig (85)"), "{n}");
    let front = Front::poland(POLAND_MID, &[807, 98, 762, 88], mid_armies());
    let mut state = State::default();
    step(
        &mut state,
        &front,
        &plan("POL", json!({"keep_free": 3})),
        START,
    );
    let n = note(&state, "encircle");
    assert!(n.contains("fewer than keep_free 3"), "{n}");
    let front = Front::poland(POLAND_MID, &[807, 98, 762, 88], mid_armies());
    let mut state = State::default();
    step(
        &mut state,
        &front,
        &plan("POL", json!({"armies": ["Army 1", "Army 3"]})),
        START,
    );
    let n = note(&state, "encircle");
    // Army 2 may not be committed: Army 3 at Gdynia takes the Kraków arm, and no anvil is left
    assert!(n.contains("Army 3 from 88 into Lublin (92)"), "{n}");
    assert!(!n.contains("anvil"), "{n}");
    // a read row with more neck states than max_gap is passed over
    let front = Front::poland(POLAND_MID, &[807, 98, 762, 88], mid_armies());
    let mut state = State::default();
    step(
        &mut state,
        &front,
        &plan("POL", json!({"max_gap": 1})),
        START,
    );
    let n = note(&state, "encircle");
    assert!(n.starts_with("reducing pocket Danzig (85)"), "{n}");
}

/// One turn of the war block as the executor runs it: the opening pass (lines only), then the
/// war step itself when the pass ordered nothing.
fn block(state: &mut State, front: &Front, plan: &Value, now: GameHour) -> Option<String> {
    state.war_lines_only = true;
    let lines = war_step(state, front, plan, "l", now).unwrap();
    state.war_lines_only = false;
    lines.or_else(|| step(state, front, plan, now))
}

#[test]
fn the_opening_pass_leaves_the_armies_of_an_operation_to_their_roles() {
    let front = Front::poland(POLAND_MID, &[807, 98, 762, 88], mid_armies());
    let mut state = State::default();
    let plan = plan("POL", json!({}));
    // the operation starts in the war step; from then on the pass draws no line for its armies
    step(&mut state, &front, &plan, START);
    for i in 1..40 {
        block(&mut state, &front, &plan, START + i * 3);
    }
    // each arm's lines go into its own neck state, as without the pass
    for (arm, neck) in [("Army 1", 10), ("Army 2", 92)] {
        for line in ["hoi4_army_front", "hoi4_army_offensive"] {
            let drawn = sent(&front, arm, line);
            assert_eq!(drawn.len(), 1, "{arm} {line} {drawn:?}");
            assert_eq!(drawn[0]["state_id"], json!(neck), "{arm} {line}");
        }
    }
    // the anvil holds its front against the pocket: one front, never an offensive line
    let fronts = sent(&front, "Army 3", "hoi4_army_front");
    assert_eq!(fronts.len(), 1, "{fronts:?}");
    assert_eq!(fronts[0]["state_id"], json!(86));
    assert!(sent(&front, "Army 3", "hoi4_army_offensive").is_empty());
}
