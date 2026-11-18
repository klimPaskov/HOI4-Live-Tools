use super::*;
use std::cell::RefCell;

const START: GameHour = 1_000_000;

/// A Poland-like theatre: line armies Mitte and Sued in Army Group 1, the armour army Panzer and
/// the wing Nord, the hold group Army Group 2 (WestA, WestB). Orders the step sends change the
/// read-back the way the game's battle-plan read does.
struct Game {
    roster: RefCell<Value>,
    enemies: RefCell<Vec<&'static str>>,
    held: RefCell<Vec<i64>>,
    refuse_multi: RefCell<bool>,
    /// every offensive draw refused
    refuse_all: RefCell<bool>,
    /// (division id, army, template) as hoi4_supply lists them
    divs: RefCell<Vec<(String, String, String)>>,
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
            divs: RefCell::new(vec![]),
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
            "hoi4_army_offensive" if *self.refuse_all.borrow() => {
                return Ok(json!({"receipt": {"status": "NotApplied"}}));
            }
            "hoi4_army_offensive" if args["states"].is_array() && *self.refuse_multi.borrow() => {
                return Ok(json!({"receipt": {"status": "NotApplied"}}));
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
                    front["offensives"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"executing": false, "target": {"states": states}}));
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
                    .map(|(id, a, t)| json!({"id": id, "army": a, "template": t}))
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
            "hoi4_army_front Panzer",
            "hoi4_army_offensive Panzer",
            "hoi4_army_front Nord",
            "hoi4_army_offensive Nord",
            "hoi4_army_front Army Group 1",
            "hoi4_army_offensive Army Group 1",
            "hoi4_army_front Army Group 2",
        ]
    );
    assert_eq!(sent[1].1["clear_orders"], json!(true));
    assert_eq!(
        sent[3].1["state_id"],
        json!(86),
        "the front clicks the enemy border state"
    );
    assert_eq!(sent[4].1, json!({"army": "Panzer", "state_id": 10}));
    assert_eq!(
        sent[8].1,
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
    // and is renamed to the formation's name a quarter day later
    game.calls.borrow_mut().clear();
    burst(&mut state, &game, &plan, START + DAY / 4 + 1);
    let renamed = json!({"army": "Army 9", "name": "Schnell"});
    assert!(game
        .orders()
        .iter()
        .any(|(n, a)| n == "hoi4_army_rename" && *a == renamed));
    assert_eq!(resolve(&state, "Schnell"), "Schnell");
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
