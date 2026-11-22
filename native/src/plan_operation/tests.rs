use super::*;
use std::cell::RefCell;

const START: GameHour = 1_000_000;

/// A Poland-like theatre: line armies Mitte and Sued in Army Group 1, the armour army Panzer and
/// the wing Nord, the hold group Army Group 2 (WestA, WestB). Orders the step sends change the
/// read-back the way the game's battle-plan read does.
/// An enemy state as hoi4_states lists it: id, victory points, x, z.
type EnemyState = (i64, f64, f64, f64);

struct Game {
    roster: RefCell<Value>,
    enemies: RefCell<Vec<&'static str>>,
    held: RefCell<Vec<i64>>,
    refuse_multi: RefCell<bool>,
    /// every offensive draw refused
    refuse_all: RefCell<bool>,
    /// every division move refused (a row the army panel does not draw)
    refuse_detach: RefCell<bool>,
    /// what hoi4_encirclement_candidates reads
    pockets: RefCell<Value>,
    /// (division id, army, template) as hoi4_supply lists them
    divs: RefCell<Vec<(String, String, String)>>,
    /// division id -> the state it stands in (hoi4_supply state_id)
    div_states: RefCell<HashMap<String, i64>>,
    /// the enemy's states as hoi4_states POL lists them (id, victory points, x, z), when set
    far: RefCell<Option<Vec<EnemyState>>>,
    /// the own front states' centres as hoi4_states GER lists them
    own: RefCell<Vec<(i64, f64, f64)>>,
    /// every delete of one order refused
    refuse_delete: RefCell<bool>,
    calls: RefCell<Vec<(String, Value)>>,
}

impl Game {
    fn new() -> Self {
        let army = |name: &str, group: Option<&str>| {
            json!({"army": name, "army_group": group, "commander": "x", "plan": [],
                   "exercising": false, "division_count": 24})
        };
        let mut roster = json!({
            "armies": [army("Mitte", Some("Army Group 1")), army("Sued", Some("Army Group 1")),
                       army("Panzer", Some("Army Group 1")), army("Nord", Some("Army Group 1")),
                       army("WestA", Some("Army Group 2")), army("WestB", Some("Army Group 2")),
                       army("Other", None)],
            "army_groups": [{"army_group": "Army Group 1", "plan": []},
                            {"army_group": "Army Group 2", "plan": []}]});
        // a stray own order on a line army, and an exercising one
        roster["armies"][0]["plan"] =
            json!([{"kind": "front", "executing": false, "offensives": []}]);
        roster["armies"][1]["exercising"] = json!(true);
        roster["armies"][4]["plan"] =
            json!([{"kind": "front", "executing": false, "offensives": []}]);
        Game {
            roster: RefCell::new(roster),
            enemies: RefCell::new(vec![]),
            held: RefCell::new(vec![]),
            refuse_multi: RefCell::new(false),
            refuse_all: RefCell::new(false),
            refuse_detach: RefCell::new(false),
            pockets: RefCell::new(json!({"pockets": []})),
            divs: RefCell::new(vec![]),
            div_states: RefCell::new(HashMap::new()),
            far: RefCell::new(None),
            own: RefCell::new(vec![]),
            refuse_delete: RefCell::new(false),
            calls: RefCell::new(vec![]),
        }
    }
    fn orders(&self) -> Vec<(String, Value)> {
        self.calls
            .borrow()
            .iter()
            .filter(|(_, a)| a.get("request_id").is_some())
            .map(|(n, a)| {
                let mut a = a.clone();
                a.as_object_mut().unwrap().remove("request_id");
                a.as_object_mut().unwrap().remove("lease_token");
                (n.clone(), a)
            })
            .collect()
    }
    fn unit<R>(&self, name: &str, f: impl FnOnce(&mut Value) -> R) -> R {
        let mut roster = self.roster.borrow_mut();
        for (key, field) in [("armies", "army"), ("army_groups", "army_group")] {
            for row in roster[key].as_array_mut().unwrap() {
                if row[field] == json!(name) {
                    return f(row);
                }
            }
        }
        panic!("no unit {name}");
    }
}

impl Tools for Game {
    fn call(&self, name: &str, args: &Value) -> Result<Value> {
        self.calls
            .borrow_mut()
            .push((name.to_string(), args.clone()));
        let army = text(&args["army"]).to_string();
        match name {
            "hoi4_armies" => return Ok(self.roster.borrow().clone()),
            "hoi4_wars" => {
                return Ok(json!({"wars": {"enemies": *self.enemies.borrow()},
                    "borders": [{"neighbour_tag": "POL", "neighbour_states": [86]},
                                {"neighbour_tag": "FRA", "neighbour_states": [28]}]}))
            }
            "hoi4_overview" => return Ok(json!({"player": "GER"})),
            "hoi4_states" if text(&args["query"]).parse::<i64>().is_ok() => {
                let s: i64 = text(&args["query"]).parse().unwrap();
                let c = if self.held.borrow().contains(&s) {
                    "GER"
                } else {
                    "POL"
                };
                return Ok(json!({"states": [{"state_id": s, "controller": c,
                    "provinces": [{"controller": c}]}]}));
            }
            "hoi4_encirclement_candidates" => return Ok(self.pockets.borrow().clone()),
            "hoi4_army_detach" if *self.refuse_detach.borrow() => {
                return Ok(json!({"receipt": {"status": "NotApplied"}}));
            }
            "hoi4_army_offensive" if *self.refuse_all.borrow() => {
                return Ok(json!({"receipt": {"status": "NotApplied"}}));
            }
            "hoi4_army_offensive" if args["states"].is_array() && *self.refuse_multi.borrow() => {
                return Ok(json!({"receipt": {"status": "NotApplied"}}));
            }
            "hoi4_states" if args["query"] == json!("POL") && self.far.borrow().is_some() => {
                let held = self.held.borrow();
                let rows: Vec<Value> = self
                    .far
                    .borrow()
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|(id, vp, x, z)| {
                        let c = if held.contains(id) { "GER" } else { "POL" };
                        json!({"state_id": id, "owner": "POL", "controller": c,
                               "victory_points_total": vp, "x": x, "z": z,
                               "provinces": [{"controller": c}]})
                    })
                    .collect();
                return Ok(json!({"states": rows}));
            }
            "hoi4_states" if args["query"] == json!("GER") => {
                let rows: Vec<Value> = self
                    .own
                    .borrow()
                    .iter()
                    .map(|(id, x, z)| json!({"state_id": id, "controller": "GER", "x": x, "z": z}))
                    .collect();
                return Ok(json!({"states": rows}));
            }
            "hoi4_army_plan" if args["delete_order"].is_i64() && *self.refuse_delete.borrow() => {
                return Ok(json!({"receipt": {"status": "NotApplied"}}));
            }
            "hoi4_army_plan" if args["delete_order"].is_i64() => {
                let id = args["delete_order"].clone();
                self.unit(&army, |r| {
                    for o in r["plan"].as_array_mut().unwrap() {
                        if let Some(offs) = o["offensives"].as_array_mut() {
                            offs.retain(|off| off["id"] != id);
                        }
                    }
                })
            }
            "hoi4_states" => {
                let held = self.held.borrow();
                let rows: Vec<Value> = ints(&args["ids"])
                    .into_iter()
                    .map(|s| {
                        let c = if held.contains(&s) { "GER" } else { "POL" };
                        json!({"state_id": s, "controller": c})
                    })
                    .collect();
                let rows = if args["query"] == json!("POL") {
                    vec![
                        json!({"state_id": 95, "controller": "POL", "victory_points_total": 5}),
                        json!({"state_id": 96, "controller": "POL", "victory_points_total": 1}),
                    ]
                } else {
                    rows
                };
                return Ok(json!({"states": rows}));
            }
            "hoi4_army_front" => self.unit(&army, |r| {
                r["plan"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"kind": "front", "executing": false, "offensives": []}))
            }),
            "hoi4_army_offensive" => {
                let states = if args["states"].is_array() {
                    args["states"].clone()
                } else {
                    json!([args["state_id"]])
                };
                self.unit(&army, |r| {
                    let plan = r["plan"].as_array_mut().unwrap();
                    let front = plan
                        .iter_mut()
                        .find(|o| o["kind"] == json!("front"))
                        .unwrap();
                    let offs = front["offensives"].as_array_mut().unwrap();
                    let id = 100 + offs.len();
                    offs.push(json!({"id": id, "executing": false, "target": {"states": states}}));
                })
            }
            "hoi4_army_plan" if args["clear_orders"] == json!(true) => {
                self.unit(&army, |r| r["plan"] = json!([]))
            }
            "hoi4_army_plan" if args["execute"].is_boolean() => {
                let on = args["execute"].clone();
                self.unit(&army, |r| {
                    for o in r["plan"].as_array_mut().unwrap() {
                        o["executing"] = on.clone();
                        for off in o["offensives"].as_array_mut().unwrap() {
                            off["executing"] = on.clone();
                        }
                    }
                })
            }
            "hoi4_supply" => {
                let rows: Vec<Value> = self
                    .divs
                    .borrow()
                    .iter()
                    .map(|(id, a, t)| {
                        json!({"id": id, "army": a, "template": t,
                               "state_id": self.div_states.borrow().get(id)})
                    })
                    .collect();
                return Ok(json!({"divisions": rows}));
            }
            "hoi4_army_detach" => {
                let into = match args["into"].as_str() {
                    Some(into) => into.to_string(),
                    None => {
                        let mut roster = self.roster.borrow_mut();
                        let rows = roster["armies"].as_array_mut().unwrap();
                        rows.push(
                            json!({"army": "Army 9", "army_group": null, "commander": "x",
                            "plan": [], "exercising": false}),
                        );
                        "Army 9".to_string()
                    }
                };
                let ids = strings(args, "divisions");
                for d in self.divs.borrow_mut().iter_mut() {
                    if ids.contains(&d.0) {
                        d.1 = into.clone();
                    }
                }
            }
            "hoi4_army_rename" => {
                let to = text(&args["name"]).to_string();
                self.unit(&army, |r| r["army"] = json!(to.clone()));
                for d in self.divs.borrow_mut().iter_mut() {
                    if d.1 == army {
                        d.1 = to.clone();
                    }
                }
            }
            "hoi4_army_exercise" => {
                for a in strings(args, "armies") {
                    self.unit(&a, |r| r["exercising"] = args["on"].clone());
                }
            }
            _ => {}
        }
        Ok(json!({"receipt": {"status": "Verified"}}))
    }
}

fn plan() -> Value {
    json!({"war": {}, "operations": [
        {"id": "op_pol", "enemies": ["POL"], "prepare_from": "", "execute_on": "at_war_with:POL", "group": "ag",
         "front": {"enemy": "POL", "own_states": [66, 67]},
         "offensive": {"axis": [67, 86, 10], "target_line": [85, 86, 10, 88]},
         "formations": [
            {"army": "Mitte", "role": "line"}, {"army": "Sued", "role": "line"},
            {"army": "Panzer", "role": "mobile", "front": {"enemy": "POL", "own_states": [67]},
             "offensive": {"axis": [67, 10], "target_line": [10]}},
            {"army": "Nord", "role": "wing", "front": {"enemy": "POL", "own_states": [5]},
             "offensive": {"axis": [5, 98], "target_line": [98, 97]}}],
         "holds": [{"id": "west", "facing": ["FRA"],
                    "armies": [{"army": "WestA"}, {"army": "WestB"}], "states": [42]}],
         "phase_lines": [{"id": "pl2", "target_line": [93, 94]}],
         "on_capitulation": {"next": "op_west"}},
        {"id": "op_west", "enemies": ["FRA"], "execute_on": "at_war_with:FRA",
         "formations": [{"army": "Other", "role": "wing"}]}]})
}

/// The step called again and again as the war burst does, until it has nothing to do.
fn burst(state: &mut State, game: &Game, plan: &Value, now: GameHour) -> Vec<String> {
    let mut lines = Vec::new();
    for _ in 0..40 {
        state.pcache = None;
        match step(state, game, plan, "l", now).unwrap() {
            Some(line) => lines.push(line),
            None => break,
        }
    }
    lines
}

fn prepared(state: &mut State, game: &Game, plan: &Value) {
    burst(state, game, plan, START);
    game.calls.borrow_mut().clear();
}

#[test]
fn prepare_reconciles_the_wanted_orders_with_the_read_back() {
    let game = Game::new();
    let plan = plan();
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    let sent = game.orders();
    let names: Vec<String> = sent
        .iter()
        .map(|(n, a)| format!("{n} {}", text(&a["army"])))
        .collect();
    assert_eq!(sent[0].0, "hoi4_army_exercise");
    assert_eq!(sent[0].1, json!({"armies": ["Sued"], "on": false}));
    assert_eq!(
        &names[1..],
        [
            "hoi4_army_plan Mitte",
            "hoi4_army_plan WestA",
            "hoi4_army_front Army Group 1",
            "hoi4_army_offensive Army Group 1",
            "hoi4_army_front Army Group 2",
            "hoi4_army_front Panzer",
            "hoi4_army_offensive Panzer",
            "hoi4_army_front Nord",
            "hoi4_army_offensive Nord",
        ]
    );
    assert_eq!(sent[1].1["clear_orders"], json!(true));
    assert_eq!(
        sent[6].1["state_id"],
        json!(86),
        "the front clicks the enemy border state"
    );
    assert_eq!(sent[7].1, json!({"army": "Panzer", "state_id": 10}));
    assert_eq!(
        sent[4].1,
        json!({"army": "Army Group 1", "state_id": 10, "states": [85, 86, 10, 88]})
    );
    // nothing executes in peace, the hold never
    assert!(!sent.iter().any(|(_, a)| a["execute"] == json!(true)));
    // idempotent: the same read-back a day later sends nothing
    game.calls.borrow_mut().clear();
    assert!(burst(&mut state, &game, &plan, START + DAY).is_empty());
    assert!(game.orders().is_empty());
    // a lost order is drawn again (and only that one)
    game.unit("Nord", |r| r["plan"] = json!([]));
    burst(&mut state, &game, &plan, START + 2 * DAY);
    let again: Vec<String> = game
        .orders()
        .iter()
        .map(|(n, a)| format!("{n} {}", text(&a["army"])))
        .collect();
    assert_eq!(again, ["hoi4_army_front Nord", "hoi4_army_offensive Nord"]);
}

#[test]
fn a_missed_target_line_is_drawn_again_twice_then_one_state_at_a_time_twice() {
    let game = Game::new();
    let plan = plan();
    let mut state = State::default();
    prepared(&mut state, &game, &plan);
    // the game drew the group's line somewhere else
    game.unit("Army Group 1", |r| {
        r["plan"][0]["offensives"] = json!([{"executing": false, "target": {"states": [90]}}])
    });
    let mut clears = 0;
    let mut drawn = Vec::new();
    for day in 1..10 {
        burst(&mut state, &game, &plan, START + day * DAY);
        clears += game
            .orders()
            .iter()
            .filter(|(n, a)| n == "hoi4_army_plan" && a["army"] == json!("Army Group 1"))
            .count();
        drawn.extend(
            game.orders()
                .into_iter()
                .filter(|(n, a)| n == "hoi4_army_offensive" && a["army"] == json!("Army Group 1"))
                .map(|(_, a)| a),
        );
        game.calls.borrow_mut().clear();
        game.unit("Army Group 1", |r| {
            r["plan"] = json!([{"kind": "front", "executing": false,
                "offensives": [{"executing": false, "target": {"states": [90]}}]}])
        });
    }
    assert_eq!(clears, 2 * REDRAWS as usize);
    assert_eq!(
        drawn.last(),
        Some(&json!({"army": "Army Group 1", "state_id": 85}))
    );
}

#[test]
fn the_declaration_executes_the_attackers_never_the_hold() {
    let game = Game::new();
    let plan = plan();
    let mut state = State::default();
    prepared(&mut state, &game, &plan);
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    let executed: Vec<String> = game
        .orders()
        .iter()
        .filter(|(_, a)| a["execute"] == json!(true))
        .map(|(_, a)| text(&a["army"]).to_string())
        .collect();
    assert_eq!(executed, ["Panzer", "Nord", "Army Group 1"]);
    assert!(state.done.contains_key("op:op_pol:exec"));
    let roster = game.roster.borrow().clone();
    assert!(executing(&roster, "Army Group 1"));
    assert!(!executing(&roster, "Army Group 2"));
}

#[test]
fn a_stalled_line_draws_the_next_phase_line_and_executes_it() {
    let game = Game::new();
    let plan = plan();
    let mut state = State::default();
    prepared(&mut state, &game, &plan);
    // the line armies carry the group's executing front (no solo front)
    for a in ["Mitte", "Sued"] {
        game.unit(a, |r| r["plan_executing"] = json!(true));
    }
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    game.calls.borrow_mut().clear();
    // three days on: Poznan taken, the line moved
    *game.held.borrow_mut() = vec![86];
    burst(&mut state, &game, &plan, START + 4 * DAY);
    assert!(game.orders().is_empty(), "a moving line is left alone");
    // nine days without a gain: the phase line
    let lines = burst(&mut state, &game, &plan, START + 13 * DAY);
    assert!(lines[0].contains("line stalled"), "{lines:?}");
    let sent = game.orders();
    assert!(sent.iter().any(|(n, a)| n == "hoi4_army_offensive"
        && *a == json!({"army": "Army Group 1", "state_id": 94, "states": [93, 94]})));
    // the new arrow starts unexecuted on an executing front: stop, then execute
    let group: Vec<&Value> = sent
        .iter()
        .filter(|(n, a)| n == "hoi4_army_plan" && a["army"] == json!("Army Group 1"))
        .map(|(_, a)| a)
        .collect();
    assert_eq!(group.len(), 2, "{group:?}");
    assert_eq!(group[0]["execute"], json!(false));
    assert_eq!(group[1]["execute"], json!(true));
    let roster = game.roster.borrow().clone();
    assert_eq!(
        arrow_executing(
            &roster,
            &unit(
                "Army Group 1".into(),
                Role::Group,
                &Value::Null,
                &Value::Null
            ),
            &[93, 94]
        ),
        Some(true)
    );
    assert_eq!(state.done.get("op:op_pol:line"), Some(&1));
    // the mobile army keeps its own line while Warsaw is not held
    assert!(!sent
        .iter()
        .any(|(n, a)| n == "hoi4_army_offensive" && a["army"] == json!("Panzer")));
}

#[test]
fn beyond_the_phase_lines_one_enemy_state_by_victory_points() {
    let game = Game::new();
    let plan = plan();
    let mut state = State::default();
    prepared(&mut state, &game, &plan);
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    game.calls.borrow_mut().clear();
    // the first line and the phase line are held: one state with the most victory points
    *game.held.borrow_mut() = vec![85, 86, 10, 88, 93, 94, 98, 97];
    let lines = burst(&mut state, &game, &plan, START + 4 * DAY);
    assert!(lines.iter().any(|l| l.contains("[95]")), "{lines:?}");
    assert!(game.orders().iter().any(|(n, a)| n == "hoi4_army_offensive"
        && *a == json!({"army": "Army Group 1", "state_id": 95})));
}

#[test]
fn other_steps_cannot_order_an_owned_army() {
    let game = Game::new();
    let plan = plan();
    let mut state = State::default();
    refresh(&mut state, &game, &plan, START);
    for owned in [
        "Mitte",
        "Panzer",
        "Nord",
        "WestA",
        "Army Group 1",
        "Army Group 2",
    ] {
        assert!(state.op_owned.contains_key(owned), "{owned}");
    }
    assert!(
        !state.op_owned.contains_key("Other"),
        "op_west has not started"
    );
    game.calls.borrow_mut().clear();
    let (ok, why) = state
        .act(
            &game,
            "l",
            "hoi4_army_move",
            json!({"army": "Mitte", "state_id": 66}),
            "wr",
        )
        .unwrap();
    assert!(!ok && why.starts_with("owned_by_operation"), "{why}");
    let (ok, _) = state
        .act(
            &game,
            "l",
            "hoi4_army_plan",
            json!({"army": "Army Group 1", "execute": true}),
            "wr",
        )
        .unwrap();
    assert!(!ok);
    let (ok, _) = state
        .act(
            &game,
            "l",
            "hoi4_army_group_create",
            json!({"armies": ["Other", "Nord"]}),
            "ag",
        )
        .unwrap();
    assert!(!ok);
    let (ok, _) = state
        .act(
            &game,
            "l",
            "hoi4_army_merge",
            json!({"source": "Nord", "target": "Other"}),
            "cm",
        )
        .unwrap();
    assert!(!ok);
    assert!(
        game.orders().is_empty(),
        "no refused order reached the game"
    );
    // an exercise start keeps only the free armies; new divisions may join a formation
    let (ok, _) = state
        .act(
            &game,
            "l",
            "hoi4_army_exercise",
            json!({"armies": ["Mitte", "Other"], "on": true}),
            "ex",
        )
        .unwrap();
    assert!(ok);
    let (ok, _) = state
        .act(
            &game,
            "l",
            "hoi4_army_merge",
            json!({"source": "Other", "target": "Mitte"}),
            "cm",
        )
        .unwrap();
    assert!(ok);
    let (ok, _) = state
        .act(
            &game,
            "l",
            "hoi4_army_commander",
            json!({"army": "Mitte", "general": "G"}),
            "cg",
        )
        .unwrap();
    assert!(ok, "a general where none leads");
    let sent = game.orders();
    assert_eq!(sent[0].1["armies"], json!(["Other"]));
    // the war step's and the layout's reads leave the owned armies out
    let mut roster = game.roster.borrow().clone();
    hide_owned(&state, &mut roster);
    let left: Vec<&str> = list(&roster, "armies")
        .iter()
        .map(|a| text(&a["army"]))
        .collect();
    assert_eq!(left, ["Other"]);
}

#[test]
fn the_capitulation_clears_the_orders_and_starts_the_next_operation() {
    let game = Game::new();
    let plan = plan();
    let mut state = State::default();
    prepared(&mut state, &game, &plan);
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    game.calls.borrow_mut().clear();
    *game.enemies.borrow_mut() = vec![];
    let lines = burst(&mut state, &game, &plan, START + 30 * DAY);
    let cleared: Vec<String> = game
        .orders()
        .iter()
        .filter(|(_, a)| a["clear_orders"] == json!(true))
        .map(|(_, a)| text(&a["army"]).to_string())
        .collect();
    assert_eq!(cleared, ["Panzer", "Nord", "Army Group 1"]);
    assert!(
        lines.iter().any(|l| l.contains("op_west prepares")),
        "{lines:?}"
    );
    assert!(state.done.contains_key("op:op_pol:end"));
    assert!(state.done.contains_key("op:op_west:prep"));
    refresh(&mut state, &game, &plan, START + 30 * DAY + 1);
    assert!(state.op_owned.contains_key("Other"));
    assert!(
        !state.op_owned.contains_key("Mitte"),
        "Poland's armies are free again"
    );
}

#[test]
fn a_far_line_that_does_not_draw_at_war_is_taken_one_state_at_a_time() {
    let game = Game::new();
    let plan = plan();
    let mut state = State::default();
    prepared(&mut state, &game, &plan);
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    *game.refuse_multi.borrow_mut() = true;
    game.calls.borrow_mut().clear();
    burst(&mut state, &game, &plan, START + 13 * DAY);
    // the refused stroke rests (the step looks again after its quiet six hours)
    burst(&mut state, &game, &plan, START + 13 * DAY + 7);
    let sent = game.orders();
    // never cleared at war
    assert!(
        !sent.iter().any(|(_, a)| a["clear_orders"] == json!(true)),
        "{sent:?}"
    );
    let drawn: Vec<&Value> = sent
        .iter()
        .filter(|(n, a)| n == "hoi4_army_offensive" && a["army"] == json!("Army Group 1"))
        .map(|(_, a)| a)
        .collect();
    assert_eq!(drawn.len(), 2, "{drawn:?}");
    assert!(drawn[0]["states"].is_array());
    assert_eq!(*drawn[1], json!({"army": "Army Group 1", "state_id": 93}));
    let roster = game.roster.borrow().clone();
    let group = unit(
        "Army Group 1".into(),
        Role::Group,
        &Value::Null,
        &Value::Null,
    );
    assert_eq!(arrow_executing(&roster, &group, &[93]), Some(true));
}

#[test]
fn a_mobile_formation_gathers_its_templates_from_the_other_armies_but_not_from_a_wing() {
    let game = Game::new();
    let d = |id: &str, army: &str, t: &str| (id.to_string(), army.to_string(), t.to_string());
    *game.divs.borrow_mut() = vec![
        d("d1", "Mitte", "Pz"),
        d("d2", "Mitte", "Inf"),
        d("d3", "WestA", "Pz"),
        d("d4", "WestA", "Pz"),
        d("d5", "WestA", "Inf"),
        d("d6", "Nord", "Pz"),
    ];
    let mut plan = plan();
    plan["operations"][0]["formations"][2] = json!({"army": "Schnell", "role": "mobile",
        "divisions": {"Pz": 4}, "front": {"enemy": "POL", "own_states": [67]},
        "offensive": {"axis": [67, 10], "target_line": [10]}});
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    let moves: Vec<(String, Value)> = game
        .orders()
        .into_iter()
        .filter(|(n, _)| n == "hoi4_army_detach" || n == "hoi4_army_rename")
        .collect();
    assert_eq!(
        moves,
        vec![
            (
                "hoi4_army_detach".into(),
                json!({"army": "WestA", "divisions": ["d3", "d4"]})
            ),
            (
                "hoi4_army_detach".into(),
                json!({"army": "Mitte", "divisions": ["d1"], "into": "Army 9"})
            ),
        ]
    );
    // the wing keeps its armour; the formed army gets its own front under the game's name
    assert_eq!(game.divs.borrow()[5].1, "Nord");
    assert!(game
        .orders()
        .iter()
        .any(|(n, a)| n == "hoi4_army_front" && a["army"] == json!("Army 9")));
    // and is not renamed by the prepare (a name is cosmetic; the layout's name step renames it
    // in idle time): the operation keeps ordering it under the game's name
    game.calls.borrow_mut().clear();
    burst(&mut state, &game, &plan, START + DAY / 4 + 1);
    assert!(!game.orders().iter().any(|(n, _)| n == "hoi4_army_rename"));
    assert_eq!(resolve(&state, "Schnell"), "Army 9");
}

#[test]
fn a_declaration_waits_for_an_operation_still_preparing() {
    let plan = json!({"operations": [{"id": "op_poland", "enemies": ["POL"]}]});
    let mut state = State::default();
    assert_eq!(unready(&state, &plan, "POL"), None, "not started");
    state.done.insert(key("op_poland", "prep"), START);
    assert_eq!(unready(&state, &plan, "POL").as_deref(), Some("op_poland"));
    assert_eq!(unready(&state, &plan, "FRA"), None, "another enemy");
    state.done.insert(key("op_poland", "ready"), START);
    assert_eq!(unready(&state, &plan, "POL"), None);
}

#[test]
fn a_failed_assembly_holds_the_war_only_until_its_declaration_is_due() {
    let game = Game::new();
    let d = |id: &str, army: &str, t: &str| (id.to_string(), army.to_string(), t.to_string());
    *game.divs.borrow_mut() = vec![d("d1", "Mitte", "Pz"), d("d2", "Mitte", "Inf")];
    *game.refuse_detach.borrow_mut() = true;
    let mut plan = plan();
    plan["operations"][0]["formations"][2] = json!({"army": "Schnell", "role": "mobile",
        "divisions": {"Pz": 1}, "front": {"enemy": "POL", "own_states": [67]},
        "offensive": {"axis": [67, 10], "target_line": [10]}});
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    assert!(
        !state.done.contains_key("op:op_pol:ready"),
        "a refused assembly rests and holds the prepare"
    );
    // the declaration is due: the formation that had its try no longer holds the war
    mark_due(&mut state, "op_pol", START + 1);
    // (before the rest of the refused move has run out)
    wake(&mut state);
    burst(&mut state, &game, &plan, START + 2);
    assert!(state.done.contains_key("op:op_pol:ready"));
    assert!(state.done.contains_key("op:op_pol:asm:Schnell"));
    assert!(unready(&state, &plan, "POL").is_none());
    // the line formations' fronts were all drawn (the assembly did not stop the prepare)
    assert!(game
        .orders()
        .iter()
        .any(|(n, a)| n == "hoi4_army_front" && a["army"] == json!("Army Group 2")));
}

#[test]
fn an_operation_already_at_war_prepares_by_the_war_rules_and_executes() {
    // it15 (18 Nov 2026): Italy starts at war with Ethiopia; the prepare cleared every offensive
    // that read back over part of the line until its draws ran out, and nothing executed
    let game = Game::new();
    let plan = plan();
    let mut state = State::default();
    *game.enemies.borrow_mut() = vec!["POL"];
    for day in 0..6 {
        burst(&mut state, &game, &plan, START + day * DAY);
        game.unit("Army Group 1", |r| {
            if let Some(front) = r["plan"].as_array_mut().and_then(|p| p.first_mut()) {
                for off in front["offensives"].as_array_mut().unwrap() {
                    off["target"]["states"] = json!([85, 90]);
                }
            }
        });
    }
    let sent = game.orders();
    assert!(
        !sent
            .iter()
            .any(|(_, a)| a["army"] == json!("Army Group 1") && a["clear_orders"] == json!(true)),
        "{sent:?}"
    );
    assert!(sent
        .iter()
        .any(|(_, a)| a["army"] == json!("Army Group 1") && a["execute"] == json!(true)));
}

#[test]
fn at_war_an_attacker_whose_offensive_never_draws_executes_its_front() {
    let game = Game::new();
    let plan = plan();
    let mut state = State::default();
    *game.enemies.borrow_mut() = vec!["POL"];
    *game.refuse_all.borrow_mut() = true;
    for hour in 0..80 {
        burst(&mut state, &game, &plan, START + hour * 7);
    }
    let executed: Vec<String> = game
        .orders()
        .iter()
        .filter(|(_, a)| a["execute"] == json!(true))
        .map(|(_, a)| text(&a["army"]).to_string())
        .collect();
    assert!(
        executed.contains(&"Army Group 1".to_string()),
        "{executed:?}"
    );
    assert!(state.done.contains_key("op:op_pol:exec"));
}

#[test]
fn a_stopped_attacker_with_no_arrow_on_its_line_executes_its_front() {
    // it26b (18 Nov 2026): the group was stopped to execute a new arrow that never read back
    let game = Game::new();
    let plan = plan();
    let mut state = State::default();
    prepared(&mut state, &game, &plan);
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    assert!(state.done.contains_key("op:op_pol:exec"));
    *game.refuse_all.borrow_mut() = true;
    game.unit("Army Group 1", |r| {
        r["plan"] = json!([{"kind": "front", "executing": false, "offensives": []}])
    });
    game.calls.borrow_mut().clear();
    for hour in 0..40 {
        burst(&mut state, &game, &plan, START + 2 * DAY + hour * 7);
    }
    assert!(game
        .orders()
        .iter()
        .any(|(_, a)| a["army"] == json!("Army Group 1") && a["execute"] == json!(true)));
    let roster = game.roster.borrow().clone();
    assert!(executing(&roster, "Army Group 1"));
}

#[test]
fn a_hold_army_is_assembled_from_its_own_states_first_never_from_a_lines_own_divisions() {
    let game = Game::new();
    let d = |id: &str, army: &str, t: &str| (id.to_string(), army.to_string(), t.to_string());
    *game.divs.borrow_mut() = vec![
        d("d1", "Mitte", "Inf"),
        d("d2", "Mitte", "Inf"),
        d("d3", "Mitte", "Inf"),
        d("d5", "Other", "Inf"),
        d("d4", "Other", "Inf"),
    ];
    game.div_states.borrow_mut().insert("d4".into(), 42);
    let mut plan = plan();
    plan["operations"][0]["formations"][0]["divisions"] = json!({"Inf": 2});
    plan["operations"][0]["holds"] = json!([{"id": "guard", "facing": ["FRA"],
        "armies": [{"army": "Guard", "divisions": {"Inf": 2}}], "states": [42]}]);
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    let moves: Vec<(String, Value)> = game
        .orders()
        .into_iter()
        .filter(|(n, _)| n == "hoi4_army_detach")
        .collect();
    assert_eq!(
        moves,
        vec![
            (
                "hoi4_army_detach".into(),
                json!({"army": "Other", "divisions": ["d4"]})
            ),
            (
                "hoi4_army_detach".into(),
                json!({"army": "Mitte", "divisions": ["d1"], "into": "Army 9"})
            ),
        ],
        "the division in the hold's state first; Mitte gives only what it does not keep"
    );
    assert_eq!(resolve(&state, "Guard"), "Army 9");
}

#[test]
fn a_player_order_on_an_owned_army_is_refused_outside_the_executor() {
    let mut owned = HashMap::new();
    owned.insert("Mitte".to_string(), "op_pol".to_string());
    owned.insert("__test_publish__".to_string(), "x".to_string());
    publish(&owned);
    let mut args = json!({"army": "Mitte", "state_id": 66});
    let why = guard_outside("hoi4_army_move", &mut args).unwrap_or_default();
    assert!(why.starts_with("owned_by_operation"), "{why}");
    let mut free = json!({"army": "Other", "state_id": 66});
    assert_eq!(guard_outside("hoi4_army_move", &mut free), None);
    let mut read = json!({});
    assert_eq!(guard_outside("hoi4_armies", &mut read), None);
    publish(&HashMap::from([(
        "__test_publish__".to_string(),
        "x".to_string(),
    )]));
}

#[test]
fn the_operations_own_nothing_until_the_army_layout_has_formed() {
    let game = Game::new();
    let mut plan = plan();
    plan["army_layout"] = json!([
        {"name_hint": "Mitte", "state": 64, "role": "strike"},
        {"name_hint": "Later", "state": 64, "role": "guard", "when": "owner:64=GER"}]);
    let mut state = State::default();
    refresh(&mut state, &game, &plan, START);
    assert!(state.op_owned.is_empty());
    assert_eq!(step(&mut state, &game, &plan, "l", START).unwrap(), None);
    assert!(state.op_notes["layout"].contains("Mitte"));
    // the role bound (a dated role is not waited for): the operations own their armies
    state.layout.insert("Mitte".into(), "Mitte".into());
    refresh(&mut state, &game, &plan, START + 1);
    assert!(state.op_owned.contains_key("Mitte"));
    // a layout that never forms holds the operations LAYOUT_WAIT_DAYS at most
    let mut state = State::default();
    refresh(&mut state, &game, &plan, START);
    assert!(state.op_owned.is_empty());
    refresh(&mut state, &game, &plan, START + LAYOUT_WAIT_DAYS * DAY);
    assert!(state.op_owned.contains_key("Mitte"));
}

#[test]
fn an_enemy_in_exile_is_out_of_the_war() {
    // g17j3: Poland in exile stayed in hoi4_wars `enemies`; `wars_won` names it, so the
    // operation against it ends and its armies are free
    let wars = json!({"wars": {"enemies": ["POL", "FRA"], "wars_won": ["POL"]}});
    assert_eq!(war_tags(&wars), vec!["FRA".to_string()]);
    let wars = json!({"wars": {"enemies": ["POL"]}});
    assert_eq!(war_tags(&wars), vec!["POL".to_string()]);
}

#[test]
fn a_line_army_without_an_executing_order_fights_on_its_own_front_after_two_days() {
    let game = Game::new();
    // the old behaviour, kept behind the plan switch (off by default since 19 Nov 2026)
    let mut plan = plan();
    plan["operations"][0]["solo_armies"] = json!(true);
    let plan = plan;
    let mut state = State::default();
    prepared(&mut state, &game, &plan);
    // Mitte stands outside the group (no front carries it); Sued executes its own orders
    game.unit("Mitte", |r| {
        r["main_state"] = json!(66);
        r["army_group"] = Value::Null;
    });
    game.unit("Sued", |r| r["plan_executing"] = json!(true));
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    burst(&mut state, &game, &plan, START + DAY + 12);
    assert!(
        !state.done.contains_key("op:op_pol:solo:Mitte"),
        "not before two days"
    );
    game.calls.borrow_mut().clear();
    burst(&mut state, &game, &plan, START + 4 * DAY + 1);
    burst(&mut state, &game, &plan, START + 4 * DAY + 8);
    assert!(state.done.contains_key("op:op_pol:solo:Mitte"));
    assert!(
        !state.done.contains_key("op:op_pol:solo:Sued"),
        "an executing army stays"
    );
    let sent: Vec<String> = game
        .orders()
        .iter()
        .filter(|(_, a)| a["army"] == json!("Mitte"))
        .map(|(n, a)| format!("{n} {}", a["execute"]))
        .collect();
    assert_eq!(
        sent,
        [
            "hoi4_army_front null",
            "hoi4_army_offensive null",
            "hoi4_army_plan true"
        ],
        "{:?}",
        game.orders()
    );
    let off = game
        .orders()
        .into_iter()
        .find(|(n, a)| n == "hoi4_army_offensive" && a["army"] == json!("Mitte"))
        .unwrap()
        .1;
    let line = [85, 86, 10, 88];
    let states = if off["states"].is_array() {
        ints(&off["states"])
    } else {
        vec![off["state_id"].as_i64().unwrap()]
    };
    assert!(states.iter().all(|s| line.contains(s)), "{off}");
    assert!(states.len() <= SOLO_STATES);
}

#[test]
fn an_operation_never_takes_the_armies_of_a_running_one() {
    let game = Game::new();
    let mut plan = plan();
    plan["operations"].as_array_mut().unwrap().push(json!(
        {"id": "op_sov", "enemies": ["SOV"], "execute_on": "at_war_with:SOV",
         "formations": [{"army": "Mitte", "role": "line"}, {"army": "Other", "role": "wing"}]}));
    let mut state = State::default();
    prepared(&mut state, &game, &plan);
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    // the Soviet Union declares in the middle of the Polish war
    *game.enemies.borrow_mut() = vec!["POL", "SOV"];
    burst(&mut state, &game, &plan, START + 2 * DAY);
    assert!(!state.done.contains_key("op:op_sov:prep"));
    assert!(state.op_notes["op_sov"].contains("waits for operation op_pol"));
    assert_eq!(
        state.op_owned.get("Mitte").map(String::as_str),
        Some("op_pol")
    );
}

#[test]
fn a_cut_army_name_is_found_on_the_map() {
    let roster = json!({"armies": [{"army": "Grenzschutz Ostpreus"}, {"army": "Nord"}]});
    assert_eq!(
        on_map(&roster, "Grenzschutz Ostpreussen"),
        "Grenzschutz Ostpreus"
    );
    let roster = json!({"armies": [{"army": "Panzergrup"}, {"army": "Army 1"}]});
    assert_eq!(
        on_map(&roster, "Panzergruppe Nord"),
        "Panzergrup",
        "cut at ten (qc4)"
    );
    assert_eq!(
        on_map(&roster, "Army 12"),
        "Army 12",
        "a short name is never a cut one"
    );
    assert_eq!(on_map(&roster, "Nord"), "Nord");
    assert_eq!(
        on_map(&roster, "Nordwest"),
        "Nordwest",
        "short names are never cut"
    );
}

#[test]
fn a_strike_formation_fights_as_one_stack_and_waits_for_supply_and_organisation() {
    let mut k = json!({"stack": 3, "widen": true, "rest": 0.35, "rest_days": 10, "max_days": 12});
    concentrate(&mut k);
    assert_eq!(k["stack"], json!(0));
    assert_eq!(k["widen"], json!(false));
    assert_eq!(k["feed"], json!(false));
    assert_eq!(k["garrison"], json!(false));
    assert_eq!(k["rest_days"], json!(STRIKE_REST_DAYS));
    assert_eq!(k["worn_gate"], json!(STRIKE_WORN));
    assert_eq!(k["supply_gate"], json!(STRIKE_SUPPLY));
    assert_eq!(k["max_days"], json!(STRIKE_MAX_DAYS));
    assert!(k["rest"].as_f64().unwrap() <= 0.25);
}

struct Contact;
impl Tools for Contact {
    fn call(&self, name: &str, _args: &Value) -> Result<Value> {
        Ok(match name {
            "hoi4_states" => json!({"states": [
                {"state_id": 86, "controller": "POL", "victory_points_total": 10},
                {"state_id": 87, "controller": "POL", "victory_points_total": 12},
                {"state_id": 762, "controller": "POL", "victory_points_total": 5},
                {"state_id": 98, "controller": "GER", "victory_points_total": 5},
                {"state_id": 88, "controller": "POL", "victory_points_total": 24},
                {"state_id": 10, "controller": "POL", "victory_points_total": 26}]}),
            _ => json!({}),
        })
    }
}

#[test]
fn a_stalled_line_rolls_on_to_the_enemy_states_the_front_touches() {
    let wars = json!({"borders": [{"neighbour_tag": "POL", "neighbour_states": [86, 87, 762, 98, 88]},
                                  {"neighbour_tag": "FRA", "neighbour_states": [28]}]});
    // touching and not held, the most victory points first; Warsaw (not touching) is not in it
    assert_eq!(
        contact_line(&Contact, &wars, &["POL".to_string()], "GER"),
        vec![88, 87, 86]
    );
}

#[test]
fn a_strike_formation_marches_to_its_start_state_before_the_war() {
    let game = Game::new();
    let mut plan = plan();
    plan["operations"][0]["formations"][2]["drive"] = json!("strike");
    let mut state = State::default();
    game.unit("Panzer", |r| r["states"] = json!({"71": 24}));
    burst(&mut state, &game, &plan, START);
    let marches: Vec<Value> = game
        .orders()
        .into_iter()
        .filter(|(n, a)| n == "hoi4_army_move" && a["army"] == json!("Panzer"))
        .map(|(_, a)| a)
        .collect();
    assert_eq!(marches, [json!({"army": "Panzer", "state_id": 67})]);
    assert!(
        !state.done.contains_key("op:op_pol:ready"),
        "not ready on the march"
    );
    // arrived: ready
    game.unit("Panzer", |r| r["states"] = json!({"67": 20, "71": 4}));
    burst(&mut state, &game, &plan, START + DAY);
    assert!(state.done.contains_key("op:op_pol:ready"));
}

#[test]
fn a_line_army_standing_a_week_on_the_groups_front_is_driven_one_at_a_time() {
    let game = Game::new();
    // the old behaviour, kept behind the plan switch (off by default since 19 Nov 2026)
    let mut plan = plan();
    plan["operations"][0]["solo_armies"] = json!(true);
    let plan = plan;
    let mut state = State::default();
    prepared(&mut state, &game, &plan);
    // both line armies stand still on the group's executing front
    for n in ["Mitte", "Sued"] {
        game.unit(n, |r| {
            r["main_state"] = json!(66);
            r["army_group"] = json!("Army Group 1");
        });
    }
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    burst(&mut state, &game, &plan, START + 4 * DAY);
    let driven = |s: &State| {
        ["Mitte", "Sued"]
            .iter()
            .filter(|n| s.done.contains_key(&format!("op:op_pol:drive:{n}")))
            .count()
    };
    assert_eq!(driven(&state), 0, "not before a week");
    burst(&mut state, &game, &plan, START + (STUCK_DAYS + 5) * DAY);
    burst(&mut state, &game, &plan, START + (STUCK_DAYS + 6) * DAY);
    assert_eq!(
        driven(&state),
        1,
        "one army of the group leaves its front: {:?}",
        state
            .done
            .iter()
            .filter(|(k, _)| k.contains(":at"))
            .map(|(k, v)| format!("{k}={}", v - START))
            .collect::<Vec<_>>()
    );
}

#[test]
fn an_operation_is_preparing_from_its_start_until_it_reads_back_ready() {
    let mut state = State::default();
    assert!(!preparing(&state));
    state.done.insert(key("op_x", "prep"), 0);
    assert!(preparing(&state), "started, nothing read back yet");
    state.done.insert(key("op_x", "ready"), 10);
    assert!(!preparing(&state), "ready: the lanes alternate again");
    state.done.remove(&key("op_x", "ready"));
    state.done.insert(key("op_x", "end"), 20);
    assert!(!preparing(&state), "an ended operation prepares nothing");
}

#[test]
fn by_default_an_idle_line_army_gets_no_front_of_its_own_and_is_not_driven() {
    let mut state = State::default();
    let plan = plan();
    let op = &plan["operations"][0];
    let roster = json!({"armies": [{"army": "Mitte", "army_group": "ag", "main_state": 66,
        "plan": [], "plan_executing": false}], "army_groups": []});
    let wars = vec!["POL".to_string()];
    for day in 0..20 {
        solo_watch(&mut state, "op_pol", op, &roster, &wars, START + day * DAY);
    }
    assert!(!state
        .done
        .keys()
        .any(|k| k.contains("solo:") || k.contains("drive:")));
}

#[test]
fn the_groups_front_and_offensive_are_drawn_before_any_assembly_or_march() {
    // the planning bonus grows only while the line stands: the group's line first (rs5/rs6,
    // 19 Nov 2026: drawn after the assembly and the marches, six game days later)
    let game = Game::new();
    let d = |id: &str, army: &str, t: &str| (id.to_string(), army.to_string(), t.to_string());
    *game.divs.borrow_mut() = vec![d("d1", "Mitte", "Pz"), d("d2", "Mitte", "Inf")];
    let mut plan = plan();
    plan["operations"][0]["formations"][2] = json!({"army": "Schnell", "role": "mobile",
        "drive": "strike", "divisions": {"Pz": 1}, "front": {"enemy": "POL", "own_states": [67]}});
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    let names: Vec<String> = game
        .orders()
        .iter()
        .map(|(n, a)| format!("{n} {}", text(&a["army"])))
        .collect();
    let at = |what: &str| names.iter().position(|n| n == what).unwrap();
    assert!(at("hoi4_army_offensive Army Group 1") < at("hoi4_army_detach Mitte"));
    assert!(at("hoi4_army_front Army Group 1") < at("hoi4_army_front Army Group 2"));
}

#[test]
fn a_ready_operation_awaits_its_war_until_it_executes() {
    let plan = json!({"operations": [{"id": "op_x", "enemies": ["POL"]}]});
    let mut state = State::default();
    state.done.insert(key("op_x", "prep"), 0);
    assert!(!awaiting_war(&state), "still preparing");
    state.done.insert(key("op_x", "ready"), 10);
    assert!(awaiting_war(&state));
    assert!(awaiting_war_on(&state, &plan, "POL"));
    assert!(!awaiting_war_on(&state, &plan, "FRA"));
    state.done.insert(key("op_x", "exec"), 20);
    assert!(!awaiting_war(&state), "executing");
    assert!(!awaiting_war_on(&state, &plan, "POL"));
}

#[test]
fn an_armour_formation_on_the_spearhead_switch_fights_on_its_own_spearhead() {
    // the owner's war model (19 Nov 2026): with `armour: "spearhead"` the armour advances on the
    // game's spearhead order painted to its axis's deepest state; the direct moves are the default
    let game = Game::new();
    let mut plan = plan();
    plan["operations"][0]["armour"] = json!("spearhead");
    plan["operations"][0]["formations"][2] = json!({"army": "Panzer", "role": "mobile",
        "drive": "strike", "front": {"enemy": "POL", "own_states": [67], "click_state": 87},
        "offensive": {"axis": [67, 87, 10], "target_line": [87, 10, 92]}});
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    let panzer: Vec<(String, Value)> = game
        .orders()
        .into_iter()
        .filter(|(n, a)| a["army"] == json!("Panzer") && n != "hoi4_army_move")
        .collect();
    assert_eq!(panzer[0].0, "hoi4_army_front");
    assert_eq!(
        panzer[0].1["from_state"],
        json!(67),
        "a narrow front in peace"
    );
    assert_eq!(panzer[1].0, "hoi4_army_offensive");
    assert_eq!(panzer[1].1["spearhead"], json!(true));
    // one stroke from the front to the thrust's deepest state (its axis: 87, then 10)
    assert_eq!(panzer[1].1["state_id"], json!(10));
    assert!(panzer[1].1.get("states").is_none());
    // the default engine: no front of its own
    let game = Game::new();
    plan["operations"][0]["armour"] = Value::Null;
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    assert!(!game
        .orders()
        .iter()
        .any(|(n, a)| n == "hoi4_army_front" && a["army"] == json!("Panzer")));
}

#[test]
fn a_spearhead_covers_its_line_by_the_path_it_was_painted_through() {
    let mut u = unit(
        "Panzer".into(),
        Role::Mobile,
        &json!({"enemy": "POL"}),
        &Value::Null,
    );
    u.spear = true;
    let roster = json!({"armies": [{"army": "Panzer", "plan": [{"kind": "front", "offensives": [
        {"executing": true, "path_states": [67, 87, 10, 92], "target": {"states": [92]}}]}]}]});
    assert_eq!(arrow_executing(&roster, &u, &[10, 92]), Some(true));
    assert_eq!(arrow_executing(&roster, &u, &[90]), None);
}

#[test]
fn a_spearhead_that_misses_gives_way_to_the_armours_offensive_line() {
    let game = Game::new();
    let mut plan = plan();
    plan["operations"][0]["formations"][2] = json!({"army": "Panzer", "role": "mobile",
        "drive": "strike", "front": {"enemy": "POL", "own_states": [67], "click_state": 87},
        "offensive": {"axis": [67, 87, 10], "target_line": [87, 10, 92]}});
    plan["operations"][0]["armour"] = json!("spearhead");
    let mut state = State::default();
    game.unit("Panzer", |r| r["states"] = json!({"67": 24}));
    // the spear reads back elsewhere: one redraw, then the offensive line
    game.unit("Panzer", |r| {
        r["plan"] = json!([{"kind": "front", "offensives": [
            {"executing": false, "path_states": [67, 88], "target": {"states": [88]}}]}])
    });
    state.done.insert(key("op_pol", "redraw:Panzer:87-10"), 1);
    burst(&mut state, &game, &plan, START);
    assert!(state.done.contains_key(&key("op_pol", "nospear:Panzer")));
    let (units, _) = units(
        &state,
        "op_pol",
        &plan["operations"][0],
        &game.roster.borrow(),
    );
    assert!(!units.iter().any(|u| u.spear));
}

#[test]
fn a_spearhead_stuck_on_a_state_leaves_it_to_the_infantry_and_goes_on() {
    // qt1 (19 Nov 2026): Panzergruppe Ost stood 35 days in Lodz on its Warsaw spearhead
    let game = Game::new();
    *game.held.borrow_mut() = vec![87];
    let mut u = unit(
        "Panzer".into(),
        Role::Mobile,
        &json!({"enemy": "POL"}),
        &Value::Null,
    );
    u.spear = true;
    u.line = vec![87, 10];
    let mut state = State::default();
    let roster = game.roster.borrow().clone();
    let op_line = [10, 90, 92];
    let first = spear_line(&mut state, "op", &roster, &u, "GER", &game, &op_line, START);
    assert_eq!(first, vec![10], "Lodz taken: Warsaw next");
    let later = spear_line(
        &mut state,
        "op",
        &roster,
        &u,
        "GER",
        &game,
        &op_line,
        START + 5 * DAY,
    );
    assert_eq!(later, vec![10], "kept while it may still fall");
    let stuck = spear_line(
        &mut state,
        "op",
        &roster,
        &u,
        "GER",
        &game,
        &op_line,
        START + 11 * DAY,
    );
    assert_ne!(stuck, vec![10], "given up after STRIKE_MAX_DAYS");
    assert_eq!(stuck.len(), 1);
    assert!(state.done.contains_key(&key("op", "spearskip:Panzer:10")));
}

#[test]
fn a_wing_standing_before_its_first_state_attacks_the_next_one() {
    let game = Game::new();
    game.unit("Nord", |r| r["states"] = json!({"5": 17}));
    // East Prussia is own land
    game.held.borrow_mut().push(5);
    let u = unit(
        "Nord".into(),
        Role::Wing,
        &json!({"enemy": "POL", "own_states": [5]}),
        &json!({"axis": [5, 807], "target_line": [807, 98, 97]}),
    );
    let roster = game.roster.borrow().clone();
    let mut state = State::default();
    let mut wanted = vec![807, 98, 97];
    let r = wing_stall(
        &mut state,
        &game,
        "op",
        &u,
        "GER",
        &roster,
        &mut wanted,
        "l",
        START,
    )
    .unwrap();
    assert!(r.is_none());
    let mut wanted = vec![807, 98, 97];
    let r = wing_stall(
        &mut state,
        &game,
        "op",
        &u,
        "GER",
        &roster,
        &mut wanted,
        "l",
        START + 4 * DAY,
    )
    .unwrap();
    assert!(r.is_none(), "not yet {WING_STALL_DAYS} days");
    let mut wanted = vec![807, 98, 97];
    let r = wing_stall(
        &mut state,
        &game,
        "op",
        &u,
        "GER",
        &roster,
        &mut wanted,
        "l",
        START + WING_STALL_DAYS * DAY,
    )
    .unwrap();
    assert!(r.is_some());
    assert_eq!(wanted, vec![98, 97]);
    assert!(game.orders().iter().any(|(n, a)| n == "hoi4_army_plan"
        && a["army"] == json!("Nord")
        && a["clear_orders"] == json!(true)));
    // later calls want the next line at once, and divisions across the border keep it
    game.unit("Nord", |r| r["states"] = json!({"5": 10, "98": 7}));
    let roster = game.roster.borrow().clone();
    let mut wanted = vec![807, 98, 97];
    let r = wing_stall(
        &mut state,
        &game,
        "op",
        &u,
        "GER",
        &roster,
        &mut wanted,
        "l",
        START + 20 * DAY,
    )
    .unwrap();
    assert!(r.is_none());
    assert_eq!(wanted, vec![98, 97]);
}

#[test]
fn two_armour_formations_meet_in_a_pockets_neck_hold_the_ring_and_go_on() {
    let game = Game::new();
    for (name, main) in [("Panzer", 67), ("Sued", 66), ("Mitte", 63)] {
        game.unit(name, |r| r["main_state"] = json!(main));
    }
    *game.pockets.borrow_mut() = json!({"pockets": [
        {"already_cut_off": false, "pincer_ready": true, "capital_inside": false,
         "neck_states": [{"state_id": 87}, {"state_id": 88}], "pocket_states": [{"state_id": 90}]},
        {"already_cut_off": false, "pincer_ready": true, "capital_inside": false,
         "neck_states": [{"state_id": 87}], "pocket_states": [{"state_id": 86}]}]});
    let mut state = State::default();
    for (s, p) in [
        (87, (10.0, 0.0)),
        (67, (12.0, 0.0)),
        (66, (9.0, 0.0)),
        (63, (40.0, 0.0)),
    ] {
        state.centers.insert(s, p);
    }
    let strikers: Vec<String> = ["Panzer", "Sued", "Mitte"].map(String::from).to_vec();
    let fighting: HashSet<String> = ["POL".to_string()].into_iter().collect();
    let best: HashMap<i64, (i64, f64)> = [(87, (584, 10.0)), (86, (6, 10.0))].into_iter().collect();
    let roster = game.roster.borrow().clone();
    let step = |state: &mut State, now| {
        pocket_step(
            state, &game, &roster, &strikers, &fighting, &best, "GER", now,
        )
        .unwrap()
    };
    // the one-neck pocket (the two-neck one is passed over); the two nearest formations
    let pk = step(&mut state, START).expect("a pocket");
    assert_eq!(pk.province, 584);
    assert!(!pk.closed);
    assert_eq!(pk.arms, vec!["Sued".to_string(), "Panzer".to_string()]);
    // the neck taken: the ring is closed and counted, the arms hold
    game.held.borrow_mut().push(87);
    let pk = step(&mut state, START + DAY).expect("held");
    assert!(pk.closed);
    assert_eq!(state.done.get("op:pockets_closed"), Some(&1));
    // the pocket's states taken: the armour goes on
    game.held.borrow_mut().push(86);
    assert!(step(&mut state, START + 2 * DAY).is_none());
    assert!(!state.done.contains_key("pocket:neck"));
}

#[test]
fn a_pockets_neck_not_taken_in_time_is_given_up_and_not_tried_again_soon() {
    let game = Game::new();
    for (name, main) in [("Panzer", 67), ("Sued", 66)] {
        game.unit(name, |r| r["main_state"] = json!(main));
    }
    *game.pockets.borrow_mut() = json!({"pockets": [
        {"already_cut_off": false, "pincer_ready": true, "capital_inside": false,
         "neck_states": [{"state_id": 87}], "pocket_states": [{"state_id": 86}]}]});
    let mut state = State::default();
    for (s, p) in [(87, (10.0, 0.0)), (67, (12.0, 0.0)), (66, (9.0, 0.0))] {
        state.centers.insert(s, p);
    }
    let strikers: Vec<String> = ["Panzer", "Sued"].map(String::from).to_vec();
    let fighting: HashSet<String> = ["POL".to_string()].into_iter().collect();
    let best: HashMap<i64, (i64, f64)> = [(87, (584, 10.0))].into_iter().collect();
    let roster = game.roster.borrow().clone();
    let step = |state: &mut State, now| {
        pocket_step(
            state, &game, &roster, &strikers, &fighting, &best, "GER", now,
        )
        .unwrap()
    };
    assert!(step(&mut state, START).is_some());
    assert!(step(&mut state, START + POCKET_CLOSE_DAYS * DAY).is_none());
    assert!(step(&mut state, START + (POCKET_CLOSE_DAYS + 3) * DAY).is_none());
    assert_eq!(state.done.get("op:pockets_closed"), None);
}

#[test]
fn a_mobile_formation_sends_its_other_templates_to_the_line_most_short_of_its_divisions() {
    // g17k, g17m: Panzergruppe Ost was the game's start army with 17 infantry divisions; the
    // armour column marched to its narrow front with them
    let game = Game::new();
    let d = |id: &str, army: &str, t: &str| (id.to_string(), army.to_string(), t.to_string());
    *game.divs.borrow_mut() = vec![
        d("p1", "Panzer", "Pz"),
        d("i1", "Panzer", "Inf"),
        d("i2", "Panzer", "Inf"),
        d("m1", "Mitte", "Inf"),
        d("m2", "Mitte", "Inf"),
        d("s1", "Sued", "Inf"),
        d("p2", "Other", "Pz"),
        d("p3", "Other", "Pz"),
    ];
    let mut plan = plan();
    plan["operations"][0]["formations"][0]["divisions"] = json!({"Inf": 2});
    plan["operations"][0]["formations"][1]["divisions"] = json!({"Inf": 4});
    plan["operations"][0]["formations"][2]["divisions"] = json!({"Pz": 2});
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    let moves: Vec<Value> = game
        .orders()
        .into_iter()
        .filter(|(n, _)| n == "hoi4_army_detach")
        .map(|(_, a)| a)
        .collect();
    assert_eq!(
        moves.first(),
        Some(&json!({"army": "Panzer", "divisions": ["i1", "i2"], "into": "Sued"})),
        "the infantry leaves first, to Sued (1 of 4) rather than Mitte (2 of 2): {moves:?}"
    );
    assert!(
        moves
            .iter()
            .any(|m| m == &json!({"army": "Other", "divisions": ["p2"], "into": "Panzer"})),
        "then the armour is gathered in: {moves:?}"
    );
}

#[test]
fn a_line_army_outside_the_group_before_the_war_forms_the_group_again_with_it() {
    // g17m: the group was formed in 1936 of the two line armies that existed; Armee Süd formed
    // later and stood outside it on the war day with no front and no planning bonus
    let game = Game::new();
    game.unit("Sued", |r| r["army_group"] = json!(null));
    let plan = plan();
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    let groups: Vec<Value> = game
        .orders()
        .into_iter()
        .filter(|(n, _)| n == "hoi4_army_group_create")
        .map(|(_, a)| a)
        .collect();
    assert_eq!(
        groups.first(),
        Some(&json!({"armies": ["Mitte", "Sued"]})),
        "{groups:?}"
    );
    // at war it is never regrouped
    let game = Game::new();
    game.unit("Sued", |r| r["army_group"] = json!(null));
    let mut state = State {
        war_active: true,
        ..State::default()
    };
    burst(&mut state, &game, &plan, START);
    assert!(game
        .orders()
        .iter()
        .all(|(n, _)| n != "hoi4_army_group_create"));
}

#[test]
fn a_hold_whose_second_army_was_never_formed_fronts_with_the_army_it_has() {
    // g17m: Armee West Süd never existed; the hold waited for a group of two and the West
    // stood without a front
    let game = Game::new();
    game.roster.borrow_mut()["armies"]
        .as_array_mut()
        .unwrap()
        .retain(|a| a["army"] != json!("WestB"));
    game.unit("WestA", |r| r["army_group"] = json!(null));
    let plan = plan();
    let state = State::default();
    let roster = game.roster.borrow().clone();
    let (units, _) = units(&state, "op_pol", &plan["operations"][0], &roster);
    let hold = units
        .iter()
        .find(|u| u.role == Role::Hold)
        .expect("the hold stands");
    assert_eq!(hold.name, "WestA");
    assert!(hold.members.is_empty());
}

#[test]
fn a_hold_of_armies_in_no_group_fronts_with_its_largest_army() {
    let game = Game::new();
    game.unit("WestA", |r| r["army_group"] = json!(null));
    game.unit("WestB", |r| {
        r["army_group"] = json!(null);
        r["division_count"] = json!(30);
    });
    let plan = plan();
    let roster = game.roster.borrow().clone();
    let (units, _) = units(&State::default(), "op_pol", &plan["operations"][0], &roster);
    let hold = units
        .iter()
        .find(|u| u.role == Role::Hold)
        .expect("the hold stands");
    assert_eq!(hold.name, "WestB");
    assert!(hold.members.is_empty());
}

/// Poland's states on the g17m2 save (hoi4_states POL: id, victory points, x, z) and the centres
/// of the group's own front states (Pomerania 63, Silesia 66 and 67, Brandenburg 68).
fn poland() -> Vec<EnemyState> {
    vec![
        (10, 26.0, 3132.0, 1557.0),
        (72, 0.0, 3096.0, 1510.0),
        (85, 10.0, 3100.0, 1606.0),
        (86, 10.0, 3072.0, 1566.0),
        (87, 10.0, 3100.0, 1553.0),
        (88, 24.0, 3134.0, 1516.0),
        (89, 1.0, 3184.0, 1493.0),
        (90, 10.0, 3128.0, 1535.0),
        (91, 3.0, 3194.0, 1510.0),
        (92, 1.0, 3162.0, 1545.0),
        (93, 4.0, 3204.0, 1534.0),
        (94, 2.0, 3205.0, 1560.0),
        (95, 4.0, 3199.0, 1588.0),
        (96, 1.0, 3226.0, 1618.0),
        (97, 4.0, 3163.0, 1585.0),
        (98, 5.0, 3121.0, 1575.0),
        (762, 5.0, 3100.0, 1528.0),
        (784, 3.0, 3202.0, 1615.0),
        (807, 11.0, 3093.0, 1591.0),
    ]
}

fn germany() -> Vec<(i64, f64, f64)> {
    vec![
        (63, 3056.0, 1595.0),
        (66, 3060.0, 1544.0),
        (67, 3088.0, 1531.0),
        (68, 3042.0, 1570.0),
    ]
}

#[test]
fn the_far_line_lies_behind_the_points_that_make_the_enemy_capitulate() {
    let states: Vec<Far> = poland()
        .into_iter()
        .map(|(id, vp, x, z)| Far { id, vp, x, z })
        .collect();
    let own = germany();
    let n = own.len() as f64;
    let origin = own
        .iter()
        .fold((0.0, 0.0), |a, p| (a.0 + p.1 / n, a.1 + p.2 / n));
    // Lwow, Polesie, Nowogrodek, Vilna: the eastern border from south to north; every city
    // with points (Warsaw, Krakow, Danzig, Gdynia, Poznan, Lodz, Kielce) lies in front of it
    assert_eq!(far_line(origin, &states), [91, 94, 95, 784]);
    assert!(far_line(origin, &states[..1]).is_empty());
}

fn deep_plan(replace: bool) -> Value {
    let mut plan = plan();
    plan["operations"][0]["offensive"]["deep"] = json!(true);
    plan["operations"][0]["replace_arrows"] = json!(replace);
    plan["operations"][0]["front"]["own_states"] = json!([63, 66, 67, 68]);
    plan
}

fn group_offensives(game: &Game) -> Vec<Value> {
    game.unit("Army Group 1", |r| {
        r["plan"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|o| list(o, "offensives").to_vec())
            .collect()
    })
}

#[test]
fn the_group_draws_one_offensive_at_the_far_line_and_never_a_phase_line() {
    let game = Game::new();
    *game.far.borrow_mut() = Some(poland());
    *game.own.borrow_mut() = germany();
    let plan = deep_plan(true);
    let mut state = State::default();
    let lines = burst(&mut state, &game, &plan, START);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("far line [91, 94, 95, 784]")),
        "{lines:?}"
    );
    assert!(game.orders().iter().any(|(n, a)| n == "hoi4_army_offensive"
        && a["army"] == json!("Army Group 1")
        && a["states"] == json!([91, 94, 95, 784])));
    assert_eq!(group_offensives(&game).len(), 1);
    game.calls.borrow_mut().clear();
    // at war, Warsaw taken (the plan's phase line would be due): no new arrow while the war moves
    for a in ["Mitte", "Sued"] {
        game.unit(a, |r| r["plan_executing"] = json!(true));
    }
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    for (day, held) in [(4, vec![86]), (8, vec![86, 807]), (12, vec![86, 807, 10])] {
        *game.held.borrow_mut() = held;
        burst(&mut state, &game, &plan, START + day * DAY);
    }
    assert!(
        !game
            .orders()
            .iter()
            .any(|(n, a)| n == "hoi4_army_offensive" && a["army"] == json!("Army Group 1")),
        "{:?}",
        game.orders()
    );
    assert_eq!(group_offensives(&game).len(), 1);
    assert_eq!(state.done.get("op:op_pol:line"), Some(&0));
}

#[test]
fn a_stalled_far_line_replaces_its_arrow_with_the_points_still_to_take() {
    let game = Game::new();
    *game.far.borrow_mut() = Some(poland());
    *game.own.borrow_mut() = germany();
    let plan = deep_plan(true);
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    for a in ["Mitte", "Sued"] {
        game.unit(a, |r| r["plan_executing"] = json!(true));
    }
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    *game.held.borrow_mut() = vec![807, 10];
    burst(&mut state, &game, &plan, START + 3 * DAY);
    game.calls.borrow_mut().clear();
    // nothing more taken for nine days: the front touches Poznan (86) only
    let lines = burst(&mut state, &game, &plan, START + 12 * DAY);
    assert!(lines.iter().any(|l| l.contains("[86]")), "{lines:?}");
    let sent = game.orders();
    let names: Vec<String> = sent
        .iter()
        .filter(|(_, a)| a["army"] == json!("Army Group 1"))
        .map(|(n, a)| {
            format!(
                "{n} {}",
                if a["delete_order"].is_i64() {
                    "delete"
                } else if a["clear_orders"] == json!(true) {
                    "clear"
                } else {
                    ""
                }
            )
        })
        .collect();
    assert_eq!(names[0], "hoi4_army_plan delete", "{names:?}");
    assert!(!names.iter().any(|n| n.ends_with("clear")), "{names:?}");
    assert!(!sent
        .iter()
        .any(|(n, a)| n == "hoi4_army_front" && a["army"] == json!("Army Group 1")));
    let offs = group_offensives(&game);
    assert_eq!(offs.len(), 1, "{offs:?}");
    assert_eq!(offs[0]["target"]["states"], json!([86]));
}

#[test]
fn an_arrow_that_will_not_delete_alone_is_cleared_with_its_front_at_war_and_drawn_again() {
    let game = Game::new();
    let mut plan = plan();
    plan["operations"][0]["replace_arrows"] = json!(true);
    let mut state = State::default();
    prepared(&mut state, &game, &plan);
    for a in ["Mitte", "Sued"] {
        game.unit(a, |r| r["plan_executing"] = json!(true));
    }
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    *game.refuse_delete.borrow_mut() = true;
    game.calls.borrow_mut().clear();
    // the group's arrow points elsewhere: delete refused, then one clear, then front and arrow
    game.unit("Army Group 1", |r| {
        r["plan"][0]["offensives"] =
            json!([{"id": 7, "executing": true, "target": {"states": [90]}}])
    });
    burst(&mut state, &game, &plan, START + 2 * DAY);
    burst(&mut state, &game, &plan, START + 3 * DAY);
    let sent: Vec<String> = game
        .orders()
        .iter()
        .filter(|(_, a)| a["army"] == json!("Army Group 1"))
        .map(|(n, a)| {
            if a["delete_order"].is_i64() {
                "delete".to_string()
            } else if a["clear_orders"] == json!(true) {
                "clear".to_string()
            } else {
                n.trim_start_matches("hoi4_army_").to_string()
            }
        })
        .collect();
    assert_eq!(
        sent[..4],
        ["delete", "clear", "front", "offensive"],
        "{sent:?}"
    );
    assert_eq!(group_offensives(&game).len(), 1);
}

#[test]
fn a_stalled_far_line_aims_at_one_place_not_across_the_country() {
    let at = |s: i64| -> Option<(f64, f64)> {
        poland().into_iter().find(|r| r.0 == s).map(|r| (r.2, r.3))
    };
    // b38g's last line: Danzig (10) and Volhynia and Bialystok (4 each) far apart
    assert_eq!(cluster(&[(4.0, 93), (10.0, 85), (4.0, 97)], &at), [85]);
    // Bialystok, Nowogrodek and Vilna lie together
    assert_eq!(
        cluster(&[(4.0, 97), (5.0, 95), (3.0, 784), (4.0, 93)], &at),
        [95, 97, 784]
    );
    assert!(cluster(&[], &at).is_empty());
}

#[test]
fn an_attack_that_reads_back_while_the_armour_still_marches_awaits_its_war() {
    // b38g: Panzergruppe Nord's march was never finished, the operation never read as ready
    // before the declaration's day, and the declaration's scheduling never began
    let game = Game::new();
    game.roster.borrow_mut()["armies"]
        .as_array_mut()
        .unwrap()
        .push(
            json!({"army": "Schnell", "army_group": null, "commander": "x", "plan": [],
                     "exercising": false, "division_count": 6, "states": {"66": 6}}),
        );
    let mut plan = plan();
    plan["operations"][0]["formations"][2] = json!({"army": "Schnell", "role": "mobile",
        "drive": "strike", "front": {"enemy": "POL", "own_states": [67]}});
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    assert!(game
        .orders()
        .iter()
        .any(|(n, a)| n == "hoi4_army_move" && a["army"] == json!("Schnell")));
    assert!(
        !state.done.contains_key("op:op_pol:ready"),
        "the march rests"
    );
    assert!(
        !prepared_against(&state, &plan, "POL"),
        "the declaration still waits"
    );
    assert!(awaiting_war(&state), "but its scheduling begins");
    assert!(awaiting_war_on(&state, &plan, "POL"));
    // the declaration's day: the march no longer holds the war
    mark_due(&mut state, "op_pol", START + DAY);
    burst(&mut state, &game, &plan, START + DAY);
    assert!(prepared_against(&state, &plan, "POL"));
}

fn group_creates(game: &Game) -> Vec<Value> {
    game.orders()
        .into_iter()
        .filter(|(n, _)| n == "hoi4_army_group_create")
        .map(|(_, a)| a)
        .collect()
}

#[test]
fn with_regroup_at_war_an_army_outside_the_group_joins_it_while_the_country_fights_elsewhere() {
    // Germany stays at war with Britain after Poland: without the key the armies outside the
    // group never join it for the next operation
    let mut plan = plan();
    plan["operations"][1]["execute_on"] = json!("at_war_with:SOV");
    let game = Game::new();
    game.unit("Sued", |r| r["army_group"] = json!("Army Group 2"));
    *game.enemies.borrow_mut() = vec!["ENG"];
    let mut state = State {
        war_active: true,
        ..State::default()
    };
    burst(&mut state, &game, &plan, START);
    assert!(group_creates(&game).is_empty(), "off by default");
    plan["operations"][0]["regroup_at_war"] = json!(true);
    let game = Game::new();
    game.unit("Sued", |r| r["army_group"] = json!("Army Group 2"));
    *game.enemies.borrow_mut() = vec!["ENG"];
    let mut state = State {
        war_active: true,
        ..State::default()
    };
    burst(&mut state, &game, &plan, START);
    assert_eq!(
        group_creates(&game).first(),
        Some(&json!({"armies": ["Mitte", "Sued"]})),
        "an army in another group joins too"
    );
    // never while the operation's own enemy is at war with us
    let game = Game::new();
    game.unit("Sued", |r| r["army_group"] = json!(null));
    *game.enemies.borrow_mut() = vec!["POL"];
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    assert!(group_creates(&game).is_empty());
}

#[test]
fn a_regroup_at_war_never_takes_an_army_in_contact_with_an_enemy() {
    let mut plan = plan();
    plan["operations"][0]["regroup_at_war"] = json!(true);
    plan["operations"][1]["execute_on"] = json!("at_war_with:SOV");
    let game = Game::new();
    // Sued stands on the French border while France is at war with us
    game.unit("Sued", |r| {
        r["army_group"] = json!(null);
        r["states"] = json!({"28": 20, "42": 4});
    });
    *game.enemies.borrow_mut() = vec!["FRA"];
    let mut state = State {
        war_active: true,
        ..State::default()
    };
    burst(&mut state, &game, &plan, START);
    assert!(group_creates(&game).is_empty(), "{:?}", game.orders());
    // an army another running operation owns is never moved either
    let mut state = State::default();
    state.op_owned.insert("Sued".into(), "op_other".into());
    assert!(owned_elsewhere(&state, "op_pol", "Sued"));
    assert!(!owned_elsewhere(&state, "op_other", "Sued"));
}

fn reserve_plan() -> Value {
    let mut plan = plan();
    plan["operations"][0]["reserve_orders"] = json!(true);
    plan["operations"][0]["reserve"] = json!([{"army": "Other", "station": 55,
        "release_when": "at_war_with:POL", "to": "operation", "divisions": {"inf": 24}}]);
    plan
}

#[test]
fn a_reserve_marches_to_its_station_then_holds_it_and_is_released_into_the_group() {
    let plan = reserve_plan();
    let game = Game::new();
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    let moves: Vec<Value> = game
        .orders()
        .into_iter()
        .filter(|(n, _)| n == "hoi4_army_move")
        .map(|(_, a)| a)
        .collect();
    assert_eq!(moves, [json!({"army": "Other", "state_id": 55})]);
    assert_eq!(
        state.op_owned.get("Other").map(String::as_str),
        Some("op_pol")
    );
    // at its station: an area defence of the station, once
    game.unit("Other", |r| r["states"] = json!({"55": 24}));
    game.calls.borrow_mut().clear();
    burst(&mut state, &game, &plan, START + 3 * DAY);
    let held: Vec<Value> = game
        .orders()
        .into_iter()
        .filter(|(n, _)| n == "hoi4_army_area_defense")
        .map(|(_, a)| a)
        .collect();
    assert_eq!(held, [json!({"army": "Other", "states": [55]})]);
    game.unit("Other", |r| r["defensive_orders"] = json!(1));
    game.calls.borrow_mut().clear();
    burst(&mut state, &game, &plan, START + 5 * DAY);
    assert!(game.orders().is_empty(), "{:?}", game.orders());
    // released: its standing order is cleared
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + 6 * DAY);
    assert!(game.orders().iter().any(
        |(n, a)| n == "hoi4_army_plan" && a == &json!({"army": "Other", "clear_orders": true})
    ));
    assert!(state.done.contains_key("op:op_pol:rel:Other"));
}

#[test]
fn a_reserve_with_a_fallback_line_holds_it_and_one_released_in_peace_joins_the_group() {
    let mut plan = reserve_plan();
    plan["operations"][0]["reserve"][0]["fallback"] = json!([50, 55]);
    let game = Game::new();
    game.unit("Other", |r| r["states"] = json!({"55": 24}));
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    assert!(game
        .orders()
        .iter()
        .any(|(n, a)| n == "hoi4_army_fallback"
            && a == &json!({"army": "Other", "states": [50, 55]})));
    // released before the war: the group is formed again with it
    plan["operations"][0]["reserve"][0]["release_when"] = json!("");
    let game = Game::new();
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    assert_eq!(
        group_creates(&game).first(),
        Some(&json!({"armies": ["Mitte", "Sued", "Other"]})),
        "{:?}",
        game.orders()
    );
    // without the key a reserve row is text
    let mut plan = reserve_plan();
    plan["operations"][0]["reserve_orders"] = json!(false);
    let game = Game::new();
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    assert!(!game
        .orders()
        .iter()
        .any(|(_, a)| a["army"] == json!("Other")));
}

#[test]
fn the_end_of_an_operation_redeploys_its_rows_whole_armies_unless_in_contact() {
    let mut plan = plan();
    plan["operations"][0]["redeploy"] = json!([{"army": "Mitte", "state": 55},
        {"army": "WestA", "state": 42}, {"army": "Sued", "state": 61}]);
    let game = Game::new();
    let mut state = State::default();
    prepared(&mut state, &game, &plan);
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    // WestA already stands in 42; Sued stands on the French border while France fights on
    game.unit("WestA", |r| r["states"] = json!({"42": 24}));
    game.unit("Sued", |r| r["states"] = json!({"28": 24}));
    game.unit("Mitte", |r| {
        r["plan"] = json!([{"kind": "front", "offensives": []}])
    });
    game.calls.borrow_mut().clear();
    *game.enemies.borrow_mut() = vec!["FRA"];
    burst(&mut state, &game, &plan, START + 30 * DAY);
    let sent: Vec<(String, Value)> = game
        .orders()
        .into_iter()
        .filter(|(_, a)| ["Mitte", "WestA", "Sued"].contains(&text(&a["army"])))
        .collect();
    assert_eq!(
        sent,
        [
            (
                "hoi4_army_plan".to_string(),
                json!({"army": "Mitte", "clear_orders": true})
            ),
            (
                "hoi4_army_move".to_string(),
                json!({"army": "Mitte", "state_id": 55})
            ),
        ]
    );
    assert!(state.done.contains_key("op:op_pol:end"));
}
