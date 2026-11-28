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
    /// what hoi4_states POL lists, when set (victory points per state and their controllers)
    pol_rows: RefCell<Option<Value>>,
    /// Poland's land touches ours (hoi4_wars borders list it)
    pol_border: RefCell<bool>,
    /// what hoi4_commanders lists, when set
    commanders: RefCell<Value>,
    /// what hoi4_supply answers, when set (provinces and divisions)
    supply: RefCell<Option<Value>>,
    /// what hoi4_province_graph answers, when set
    graph: RefCell<Option<Value>>,
    calls: RefCell<Vec<(String, Value)>>,
    /// the roster does not show a stop (a read taken before the stop landed)
    stale_stop: RefCell<bool>,
    /// every front drawn as a segment (`from_state`) refused
    refuse_segment: RefCell<bool>,
    /// the own states on the Polish border, as hoi4_wars lists them (none when empty)
    own_border: RefCell<Vec<i64>>,
    /// what hoi4_battles answers, when set
    battles: RefCell<Option<Value>>,
    /// what hoi4_template_get answers, when set
    templates: RefCell<Option<Value>>,
    /// every offensive draw read back Diverged: the game drew the arrow to this state instead
    diverge_to: RefCell<Option<i64>>,
    /// what hoi4_war_stats answers, when set
    war_stats: RefCell<Option<Value>>,
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
            pol_rows: RefCell::new(None),
            pol_border: RefCell::new(true),
            commanders: RefCell::new(json!({})),
            supply: RefCell::new(None),
            graph: RefCell::new(None),
            calls: RefCell::new(vec![]),
            stale_stop: RefCell::new(false),
            refuse_segment: RefCell::new(false),
            own_border: RefCell::new(vec![]),
            battles: RefCell::new(None),
            templates: RefCell::new(None),
            diverge_to: RefCell::new(None),
            war_stats: RefCell::new(None),
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
            "hoi4_commanders" => return Ok(self.commanders.borrow().clone()),
            "hoi4_wars" => {
                let pol = if *self.pol_border.borrow() {
                    json!([86])
                } else {
                    json!([])
                };
                let mut wars = json!({"wars": {"enemies": *self.enemies.borrow()},
                    "borders": [{"neighbour_tag": "POL", "neighbour_states": pol},
                                {"neighbour_tag": "FRA", "neighbour_states": [28]}]});
                if !self.own_border.borrow().is_empty() {
                    wars["borders"][0]["own_border_states"] = json!(*self.own_border.borrow());
                }
                return Ok(wars);
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
            "hoi4_states" if args["query"] == json!("POL") && self.pol_rows.borrow().is_some() => {
                return Ok(json!({"states": self.pol_rows.borrow().clone().unwrap()}));
            }
            "hoi4_encirclement_candidates" => return Ok(self.pockets.borrow().clone()),
            "hoi4_army_detach" if *self.refuse_detach.borrow() => {
                return Ok(json!({"receipt": {"status": "NotApplied"}}));
            }
            "hoi4_army_offensive" if self.diverge_to.borrow().is_some() => {
                let to = self.diverge_to.borrow().unwrap();
                self.unit(&army, |r| {
                    let plan = r["plan"].as_array_mut().unwrap();
                    let front = plan
                        .iter_mut()
                        .find(|o| o["kind"] == json!("front"))
                        .unwrap();
                    let offs = front["offensives"].as_array_mut().unwrap();
                    let id = 200 + offs.len();
                    offs.push(json!({"id": id, "executing": false, "divisions": 20,
                                     "target": {"states": [to]}}));
                });
                return Ok(json!({"receipt": {"status": "Diverged"}}));
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
            "hoi4_army_front" if args["from_state"].is_i64() && *self.refuse_segment.borrow() => {
                return Ok(json!({"receipt": {"status": "NotApplied"}}));
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
            "hoi4_army_plan" if args["execute"] == json!(false) && *self.stale_stop.borrow() => {}
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
            "hoi4_war_stats" if self.war_stats.borrow().is_some() => {
                return Ok(self.war_stats.borrow().clone().unwrap());
            }
            "hoi4_battles" if self.battles.borrow().is_some() => {
                return Ok(self.battles.borrow().clone().unwrap());
            }
            "hoi4_template_get" if self.templates.borrow().is_some() => {
                return Ok(self.templates.borrow().clone().unwrap());
            }
            "hoi4_province_graph" if self.graph.borrow().is_some() => {
                return Ok(self.graph.borrow().clone().unwrap());
            }
            "hoi4_supply" if self.supply.borrow().is_some() => {
                return Ok(self.supply.borrow().clone().unwrap());
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
            "hoi4_army_group_create" => {
                let mut roster = self.roster.borrow_mut();
                let n = roster["army_groups"].as_array().unwrap().len() + 1;
                let g = format!("Army Group {n}");
                roster["army_groups"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"army_group": g, "plan": []}));
                let names = strings(args, "armies");
                for row in roster["armies"].as_array_mut().unwrap() {
                    if names.iter().any(|a| row["army"] == json!(a)) {
                        row["army_group"] = json!(g);
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
fn every_attacker_a_wing_too_executes_before_the_declaration_and_the_declaration_waits_for_it() {
    // p101a (28 Nov 2026): the pre-declaration burst executed Army Group 1 and both Panzergruppen;
    // the East Prussian wing (23 divisions, one way in over a large river) held "never alone"
    // for the guard's day-long read and executed at the first war turn, W+3
    let game = Game::new();
    let plan = plan();
    let mut state = State::default();
    prepared(&mut state, &game, &plan);
    *game.own_border.borrow_mut() = vec![5];
    *game.graph.borrow_mut() = Some(json!({"provinces": [
        {"province": 1, "state_id": 98, "terrain": "urban", "attack_modifier": -0.5,
         "forts_in_history": [],
         "borders": [{"province": 2, "river": "large", "river_share": 1.0,
                      "river_attack_penalty": -0.6}]},
        {"province": 2, "state_id": 5, "terrain": "plains", "attack_modifier": 0.0,
         "borders": []}]}));
    // the declaration is due: the pass runs before the war
    let at = START + DAY;
    state.done.insert("preexec".to_string(), at);
    burst(&mut state, &game, &plan, at);
    let executed: Vec<String> = game
        .orders()
        .iter()
        .filter(|(_, a)| a["execute"] == json!(true))
        .map(|(_, a)| text(&a["army"]).to_string())
        .collect();
    assert!(executed.contains(&"Nord".to_string()), "{executed:?}");
    assert!(executed.contains(&"Panzer".to_string()), "{executed:?}");
    assert!(
        executed.contains(&"Army Group 1".to_string()),
        "{executed:?}"
    );
    assert!(
        state.op_notes["op_pol:guard:Nord"].contains("ATTACKS together with"),
        "{:?}",
        state.op_notes.get("op_pol:guard:Nord")
    );
    // every attacker reads executing: nothing holds the declaration
    assert!(pre_pending(&state, at).is_empty());
    // an attacker the roster does not read executing holds it, and is named
    game.unit("Nord", |r| {
        for o in r["plan"].as_array_mut().unwrap() {
            o["executing"] = json!(false);
            for off in o["offensives"].as_array_mut().unwrap() {
                off["executing"] = json!(false);
            }
        }
    });
    // the burst runs again while one is pending and executes it before the declaration
    // (once the step looks again, six hours after a pass with nothing to do)
    game.calls.borrow_mut().clear();
    let lines = burst(&mut state, &game, &plan, at + 7);
    assert!(
        game.orders()
            .iter()
            .any(|(_, a)| a["execute"] == json!(true) && a["army"] == json!("Nord")),
        "{lines:?} {:?}",
        state.op_notes
    );
    assert!(pre_pending(&state, at + 7).is_empty());
    // an execution not read back yet holds the declaration and is named
    state.done.insert(key("op_pol", "prepending"), at + 8);
    state.op_notes.insert(
        "op_pol:prepending".into(),
        "operation op_pol: before the declaration Nord not executing yet".into(),
    );
    let pending = pre_pending(&state, at + 8);
    assert_eq!(pending.len(), 1, "{pending:?}");
    assert!(pending[0].contains("Nord"), "{pending:?}");
    // past the pre-declaration window it holds nothing
    assert!(pre_pending(&state, at + 8 + PRE_EXECUTE + 1).is_empty());
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
fn a_stop_the_roster_does_not_show_yet_is_followed_by_the_execute_not_a_second_stop() {
    let game = Game::new();
    let plan = plan();
    let mut state = State::default();
    prepared(&mut state, &game, &plan);
    for a in ["Mitte", "Sued"] {
        game.unit(a, |r| r["plan_executing"] = json!(true));
    }
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    *game.held.borrow_mut() = vec![86];
    burst(&mut state, &game, &plan, START + 4 * DAY);
    game.calls.borrow_mut().clear();
    *game.stale_stop.borrow_mut() = true;
    burst(&mut state, &game, &plan, START + 13 * DAY);
    let group: Vec<Value> = game
        .orders()
        .iter()
        .filter(|(n, a)| n == "hoi4_army_plan" && a["army"] == json!("Army Group 1"))
        .map(|(_, a)| a["execute"].clone())
        .collect();
    assert_eq!(
        group,
        [json!(false), json!(true)],
        "t1939j: execute:false went out twice"
    );
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
    // an operation executing against another enemy of the war holds no declaration (w47r1)
    let mut low = State::default();
    let plan = json!({"operations": [{"id": "op_low", "enemies": ["HOL", "LUX"]}]});
    low.done.insert(key("op_low", "prep"), START);
    low.done.insert(key("op_low", "exec"), START);
    assert_eq!(unready(&low, &plan, "LUX"), None);
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
    state.war_active = true;
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
    plan["operations"][0]["armour"] = json!("strike");
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
    let plan = json!({"operations": [{"id": "op_x", "enemies": ["POL", "LIT"]}]});
    let mut state = State::default();
    state.done.insert(key("op_x", "prep"), 0);
    assert!(!awaiting_war(&state), "still preparing");
    state.done.insert(key("op_x", "ready"), 10);
    assert!(awaiting_war(&state));
    assert!(awaiting_war_on(&state, &plan, "POL"));
    assert!(!awaiting_war_on(&state, &plan, "FRA"));
    state.done.insert(key("op_x", "exec"), 20);
    state.done.insert(key("op_x", "fought:POL"), 20);
    assert!(!awaiting_war(&state), "executing");
    assert!(!awaiting_war_on(&state, &plan, "POL"));
    // the operation's later enemy, not fought yet, still waits for its declaration
    assert!(awaiting_war_on(&state, &plan, "LIT"));
    state.done.insert(key("op_x", "end"), 30);
    assert!(!awaiting_war_on(&state, &plan, "LIT"));
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
    // the direct-move engine (`armour: "strike"`): no front of its own; without the key the
    // spearhead is the default (s7n, 24 Nov 2026)
    let game = Game::new();
    plan["operations"][0]["armour"] = json!("strike");
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
fn a_plan_group_army_this_operation_holds_with_never_joins_its_group() {
    // bed c41d: the plan's group listed the five armies of a later operation; the pre-war regroup
    // took the Westwall hold's army into the Polish group and the hold front kept no division
    let game = Game::new();
    game.unit("Sued", |r| r["army_group"] = json!(null));
    game.unit("WestA", |r| r["army_group"] = json!(null));
    game.unit("Nord", |r| r["army_group"] = json!(null));
    let mut plan = plan();
    plan["army_group"] =
        json!([{"id": "ag", "armies": ["Mitte", "Sued", "Nord", "WestA", "WestB"]}]);
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
    plan["operations"][0]["armour"] = json!("strike");
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

fn split_plan() -> Value {
    json!({"war": {}, "operations": [
        {"id": "op_pol", "enemies": ["POL"], "prepare_from": "", "execute_on": "at_war_with:POL",
         "group": "ag", "group_cap": 3,
         "front": {"enemy": "POL", "own_states": [1, 2, 3, 4]},
         "offensive": {"axis": [2, 86, 10], "target_line": [85, 86, 10, 88]},
         "formations": [
            {"army": "Mitte", "role": "line"}, {"army": "Sued", "role": "line"},
            {"army": "Nord", "role": "line"}, {"army": "Panzer", "role": "line"}]}]})
}

fn split_game() -> Game {
    let game = Game::new();
    for (army, s) in [
        ("Mitte", "1"),
        ("Sued", "2"),
        ("Nord", "3"),
        ("Panzer", "4"),
    ] {
        game.unit(army, |r| r["states"] = json!({s: 24}));
    }
    *game.own.borrow_mut() = vec![
        (1, 0.0, 0.0),
        (2, 0.0, 10.0),
        (3, 0.0, 20.0),
        (4, 0.0, 30.0),
    ];
    *game.far.borrow_mut() = Some(vec![
        (85, 10.0, 10.0, 0.0),
        (86, 10.0, 10.0, 10.0),
        (10, 10.0, 10.0, 20.0),
        (88, 10.0, 10.0, 30.0),
    ]);
    game
}

#[test]
fn more_line_armies_than_the_group_cap_fight_in_two_groups_each_on_its_own_piece() {
    let game = split_game();
    let plan = split_plan();
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    let sent = game.orders();
    let groups: Vec<&Value> = sent
        .iter()
        .filter(|(n, _)| n == "hoi4_army_group_create")
        .map(|(_, a)| a)
        .collect();
    assert_eq!(groups, [&json!({"armies": ["Mitte", "Sued"]})], "{sent:?}");
    let fronts: Vec<&Value> = sent
        .iter()
        .filter(|(n, _)| n == "hoi4_army_front")
        .map(|(_, a)| a)
        .collect();
    assert_eq!(
        fronts,
        [
            &json!({"army": "Army Group 3", "state_id": 86, "from_states": [1, 2]}),
            &json!({"army": "Army Group 1", "state_id": 86, "from_states": [3, 4]}),
        ],
        "one front per group, each on its own states"
    );
    let arrows: Vec<(String, Value)> = sent
        .iter()
        .filter(|(n, _)| n == "hoi4_army_offensive")
        .map(|(_, a)| (text(&a["army"]).to_string(), a["states"].clone()))
        .collect();
    assert_eq!(
        arrows,
        [
            ("Army Group 3".to_string(), json!([85, 86])),
            ("Army Group 1".to_string(), json!([10, 88])),
        ]
    );
    // idempotent
    game.calls.borrow_mut().clear();
    burst(&mut state, &game, &plan, START + DAY);
    assert!(game.orders().is_empty(), "{:?}", game.orders());
    // both groups execute on the war
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + 2 * DAY);
    let executed: Vec<String> = game
        .orders()
        .iter()
        .filter(|(_, a)| a["execute"] == json!(true))
        .map(|(_, a)| text(&a["army"]).to_string())
        .collect();
    assert_eq!(executed, ["Army Group 3", "Army Group 1"]);
}

#[test]
fn without_the_group_cap_or_within_it_the_operation_keeps_one_group() {
    let game = split_game();
    let mut plan = split_plan();
    plan["operations"][0]["group_cap"] = json!(4);
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    let sent = game.orders();
    assert!(!sent.iter().any(|(n, _)| n == "hoi4_army_group_create"));
    let fronts: Vec<&Value> = sent
        .iter()
        .filter(|(n, _)| n == "hoi4_army_front")
        .map(|(_, a)| a)
        .collect();
    assert_eq!(fronts, [&json!({"army": "Army Group 1", "state_id": 86})]);
}

#[test]
fn a_group_without_a_field_marshal_gets_the_first_free_name_of_marshals() {
    let game = split_game();
    let mut plan = split_plan();
    plan["operations"][0]["marshals"] = json!(["Bock", "Rundstedt"]);
    game.unit("Army Group 1", |r| r["field_marshal"] = json!("Bock"));
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    let given: Vec<Value> = game
        .orders()
        .into_iter()
        .filter(|(n, _)| n == "hoi4_army_commander")
        .map(|(_, a)| a)
        .collect();
    assert_eq!(
        given,
        [json!({"army": "Army Group 3", "general": "Rundstedt"})]
    );
}

#[test]
fn a_new_group_takes_a_free_field_marshal_before_a_named_general_is_promoted() {
    // owner, 23 Nov 2026: Germany has enough field marshals; promotion only when none is free
    let game = split_game();
    let mut plan = split_plan();
    plan["operations"][0]["marshals"] = json!(["Bock", "Hoepner"]);
    game.unit("Army Group 1", |r| r["field_marshal"] = json!("Bock"));
    *game.commanders.borrow_mut() = json!({"commanders": [
        {"name": "Hoepner", "assigned_to": null, "available": true,
         "listed_in_panel_or_assigned": true, "promotion_needed_for_group": true, "skill": 4},
        {"name": "Leeb", "assigned_to": null, "available": true,
         "listed_in_panel_or_assigned": true, "promotion_needed_for_group": false, "skill": 3}]});
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    let given: Vec<Value> = game
        .orders()
        .into_iter()
        .filter(|(n, _)| n == "hoi4_army_commander")
        .map(|(_, a)| a)
        .collect();
    assert_eq!(given, [json!({"army": "Army Group 3", "general": "Leeb"})]);
}

#[test]
fn a_plan_click_off_the_border_gives_way_to_a_border_state() {
    let st = State::default();
    let front = json!({"enemy": "BEL", "own_states": [51, 42], "click_state": 980});
    let u = unit("Army Group 2".into(), Role::Group, &front, &json!({}));
    let wars = json!({"borders": [
        {"neighbour_tag": "BEL", "neighbour_states": [34, 6], "own_border_states": [51]},
        {"neighbour_tag": "HOL", "neighbour_states": [7], "own_border_states": [36]}
    ]});
    assert_eq!(click_state(&st, &wars, &u), Some(34));
    let on = json!({"enemy": "BEL", "own_states": [51], "click_state": 6});
    let u = unit("Army Group 2".into(), Role::Group, &on, &json!({}));
    assert_eq!(click_state(&st, &wars, &u), Some(6));
    // no border read: the plan's click stands
    let u = unit("Army Group 2".into(), Role::Group, &front, &json!({}));
    assert_eq!(click_state(&st, &json!({}), &u), Some(980));
}

#[test]
fn a_strike_formation_marches_while_only_a_holds_enemy_is_at_war() {
    // g17n and w42s (23 Nov 2026): the Westwall hold faces France, at war since September 1939;
    // counted as the operation's war, no march went out and the armour stood in the East
    let game = Game::new();
    *game.enemies.borrow_mut() = vec!["FRA"];
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
}

#[test]
fn a_side_on_its_own_countrys_border_is_clicked_plainly() {
    let mut u = unit(
        "Army Group 3".into(),
        Role::Group,
        &json!({"enemy": "HOL", "own_states": [56, 57]}),
        &json!({}),
    );
    u.side = Some(true);
    assert_eq!(
        front_args(&u, 35, true),
        json!({"army": "Army Group 3", "state_id": 35, "from_states": [56, 57]})
    );
    u.own_enemy = true;
    assert_eq!(
        front_args(&u, 35, true),
        json!({"army": "Army Group 3", "state_id": 35})
    );
}

#[test]
fn a_wing_on_one_own_state_draws_that_states_border_only() {
    let u = unit(
        "Armee Pommern".into(),
        Role::Wing,
        &json!({"enemy": "BEL", "own_states": [51]}),
        &json!({}),
    );
    assert_eq!(
        front_args(&u, 980, true),
        json!({"army": "Armee Pommern", "state_id": 980, "from_state": 51})
    );
    assert_eq!(
        front_args(&u, 980, false),
        json!({"army": "Armee Pommern", "state_id": 980})
    );
}

#[test]
fn the_sides_once_recorded_stay_while_the_armies_walk() {
    let mut state = State::default();
    for (s, p) in [(56, (2930.0, 1575.0)), (42, (2919.0, 1511.0))] {
        state.centers.insert(s, p);
    }
    let op =
        json!({"id": "op_w", "group_cap": 2, "front": {"enemy": "BEL", "own_states": [56, 42]}});
    let lines: Vec<String> = ["A", "B", "C", "D"].iter().map(|s| s.to_string()).collect();
    for (n, side) in [("A", 1), ("B", 2), ("C", 1), ("D", 2)] {
        state.done.insert(format!("op:op_w:side:{n}"), side);
    }
    let (_, low, high) = split_parts(&state, &op, &json!({"armies": []}), &lines).unwrap();
    assert_eq!(low, ["A", "C"]);
    assert_eq!(high, ["B", "D"]);
}

#[test]
fn a_sides_front_off_its_own_piece_is_cleared_and_drawn_again() {
    let game = Game::new();
    game.unit("Army Group 1", |r| {
        r["plan"] = json!([{"kind": "front", "id": 9, "divisions": 30,
            "line": {"states": [42, 51]}, "offensives": []}])
    });
    let mut u = unit(
        "Army Group 1".into(),
        Role::Group,
        &json!({"enemy": "HOL", "own_states": [56, 57], "click_state": 35}),
        &json!({"target_line": [7]}),
    );
    u.side = Some(true);
    u.own_enemy = true;
    let mut state = State::default();
    let roster = game.roster.borrow().clone();
    let line = reconcile(
        &mut state,
        &game,
        "op_w",
        &u,
        &[7],
        &roster,
        &json!({}),
        "l",
        START,
        true,
    )
    .unwrap()
    .unwrap();
    assert!(line.contains("off its own piece"), "{line}");
    assert_eq!(
        game.orders().last().unwrap().1,
        json!({"army": "Army Group 1", "clear_orders": true})
    );
}

#[test]
fn an_enemy_left_with_a_remnant_hands_its_units_on_and_the_operation_waits_for_the_next_war() {
    // w42g2 (23 Nov 2026): the Netherlands kept Suriname and Curacao (2 of 91 points), stayed at
    // war, and the group and the armour kept attacking Dutch Brabant, already ours, for 15 days
    // after Belgium was declared
    let game = Game::new();
    let mut plan = plan();
    plan["operations"][0]["enemies"] = json!(["POL", "LIT"]);
    let mut state = State::default();
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START);
    burst(&mut state, &game, &plan, START + DAY);
    assert!(state.done.contains_key(&key("op_pol", "fought:POL")));
    // Poland holds one point of a hundred, across the sea: out of this operation
    *game.pol_rows.borrow_mut() = Some(json!([
        {"state_id": 10, "controller": "GER", "victory_points": [
            {"province": 1, "victory_points": 99.0, "controller": "GER"}]},
        {"state_id": 300, "controller": "POL", "victory_points": [
            {"province": 2, "victory_points": 1.0, "controller": "POL"}]}]));
    // a remnant on our border fights on: not out, but beaten (the armour goes on, w47c1)
    burst(&mut state, &game, &plan, START + 2 * DAY);
    assert!(!state.done.contains_key(&key("op_pol", "out:POL")));
    assert!(state.done.contains_key(&key("op_pol", "beaten:POL")));
    // across the sea: out
    *game.pol_border.borrow_mut() = false;
    burst(&mut state, &game, &plan, START + 3 * DAY);
    assert!(state.done.contains_key(&key("op_pol", "out:POL")));
    assert!(
        !state.done.contains_key(&key("op_pol", "end")),
        "Lithuania is not fought yet: the operation goes on"
    );
    // Lithuania declared, fought, then out: the operation ends
    *game.enemies.borrow_mut() = vec!["POL", "LIT"];
    burst(&mut state, &game, &plan, START + 4 * DAY);
    assert!(state.done.contains_key(&key("op_pol", "fought:LIT")));
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + 6 * DAY);
    assert!(state.done.contains_key(&key("op_pol", "end")));
}

#[test]
fn at_war_a_mobile_army_fronts_from_the_border_state_it_stands_in() {
    // w43r1 (23 Nov 2026): each Panzergruppe's plain-click front took the whole Belgian border
    let roster = json!({"armies": [{"army": "Panzer", "states": {"35": 12, "7": 3, "51": 0}}]});
    let wars = json!({"borders": [
        {"neighbour_tag": "BEL", "neighbour_states": [6, 977], "own_border_states": [35, 51, 42]},
        {"neighbour_tag": "FRA", "neighbour_states": [28], "own_border_states": [7, 42]}]});
    assert_eq!(
        standing_border_state(&roster, &wars, "BEL", "Panzer"),
        Some(35)
    );
    assert_eq!(
        standing_border_state(&roster, &wars, "FRA", "Panzer"),
        Some(7)
    );
    assert_eq!(standing_border_state(&roster, &wars, "LUX", "Panzer"), None);
}

#[test]
fn a_reserve_standing_on_a_holds_border_joins_that_holds_front() {
    let game = Game::new();
    let op = json!({"id": "op_pol", "reserve_orders": true,
        "holds": [{"id": "west", "facing": ["FRA"], "armies": [{"army": "WestA"}], "states": [42],
                   "click_state": 28}],
        "reserve": [{"army": "Other", "station": 55}]});
    game.unit("Other", |r| r["states"] = json!({"42": 20}));
    let roster = game.roster.borrow().clone();
    let wars = json!({"wars": {"enemies": ["FRA"]},
        "borders": [{"neighbour_tag": "FRA", "neighbour_states": [28], "own_border_states": [42]}]});
    let mut state = State::default();
    let line = reserves(
        &mut state, &game, "op_pol", &op, None, &roster, &wars, "l", START,
    )
    .unwrap()
    .unwrap();
    assert!(line.contains("joins its front"), "{line}");
    let fronts: Vec<Value> = game
        .orders()
        .into_iter()
        .filter(|(n, _)| n == "hoi4_army_front")
        .map(|(_, a)| a)
        .collect();
    assert_eq!(fronts, vec![json!({"army": "Other", "state_id": 28})]);
}

#[test]
fn in_peace_a_stroke_that_will_not_draw_is_drawn_one_state_at_a_time_from_the_click() {
    // w43a1 (23 Nov 2026): a front with no arrow gains no planning; the armour's strokes were
    // refused four times in peace and the Panzergruppen attacked with planning 0
    let game = Game::new();
    *game.refuse_multi.borrow_mut() = true;
    let mut plan = plan();
    plan["operations"][0]["formations"][2] = json!({"army": "Panzer", "role": "mobile",
        "front": {"enemy": "POL", "own_states": [67], "click_state": 86},
        "offensive": {"axis": [67, 86, 10], "target_line": [10, 92]}});
    let mut state = State::default();
    for h in 0..6 {
        burst(&mut state, &game, &plan, START + h * 3);
    }
    assert_eq!(
        state.done.get(&key("op_pol", "one:Panzer:10-92")),
        Some(&86)
    );
    assert!(game.orders().iter().any(|(n, a)| n == "hoi4_army_offensive"
        && a["army"] == json!("Panzer")
        && a["state_id"] == json!(86)
        && a.get("states").is_none()));
}

#[test]
fn in_peace_a_mobile_army_whose_front_stands_off_its_start_state_is_drawn_again() {
    let game = Game::new();
    let mut plan = plan();
    plan["operations"][0]["formations"][2] = json!({"army": "Panzer", "role": "mobile",
        "front": {"enemy": "POL", "own_states": [67], "click_state": 86},
        "offensive": {"axis": [67, 86], "target_line": [86]}});
    game.unit("Panzer", |r| {
        r["army_group"] = Value::Null;
        r["plan"] = json!([{"kind": "front", "executing": false, "offensives": [],
                            "line": {"states": [51]}}])
    });
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    assert!(game.orders().iter().any(|(n, a)| n == "hoi4_army_plan"
        && a["army"] == json!("Panzer")
        && a["clear_orders"] == json!(true)));
}

#[test]
fn a_reserve_left_on_its_groups_front_is_cleared_and_marches_back_from_the_border() {
    let game = Game::new();
    let op = json!({"id": "op_pol", "reserve_orders": true,
        "reserve": [{"army": "Other", "station": 55}]});
    game.unit("Other", |r| {
        r["states"] = json!({"42": 20});
        r["army_group"] = json!("Group");
        r["plan"] = json!([{"kind": "front", "executing": false, "offensives": [],
                            "line": {"states": [42]}}]);
    });
    let roster = game.roster.borrow().clone();
    let wars = json!({"wars": {"enemies": ["FRA"]},
        "borders": [{"neighbour_tag": "FRA", "neighbour_states": [28], "own_border_states": [42]}]});
    let mut state = State::default();
    let line = reserves(
        &mut state, &game, "op_pol", &op, None, &roster, &wars, "l", START,
    )
    .unwrap()
    .unwrap();
    assert!(line.contains("clear its orders"), "{line}");
    let line = reserves(
        &mut state,
        &game,
        "op_pol",
        &op,
        None,
        &roster,
        &wars,
        "l",
        START + 1,
    )
    .unwrap()
    .unwrap();
    assert!(line.contains("marches to its station 55"), "{line}");
    let orders = game.orders();
    assert!(orders.iter().any(|(n, a)| n == "hoi4_army_plan"
        && a["army"] == json!("Other")
        && a["clear_orders"] == json!(true)));
    assert!(orders
        .iter()
        .any(|(n, a)| n == "hoi4_army_move" && a["state_id"] == json!(55)));
}

#[test]
fn a_reserve_inside_the_operations_group_is_left_out_when_the_group_is_formed_again() {
    // w43n1 (23 Nov 2026): the reserve's own orders cleared and its march read back, its
    // divisions stayed on the group's front: only a group of the line armies frees it
    let mut plan = plan();
    plan["operations"][0]["regroup_at_war"] = json!(true);
    plan["operations"][0]["reserve_orders"] = json!(true);
    plan["operations"][0]["formations"] =
        json!([{"army": "Mitte", "role": "line"}, {"army": "Nord", "role": "line"}]);
    plan["operations"][0]["reserve"] = json!([{"army": "Sued", "station": 55}]);
    let game = Game::new();
    *game.enemies.borrow_mut() = vec!["ENG"];
    let mut state = State {
        war_active: true,
        ..State::default()
    };
    burst(&mut state, &game, &plan, START);
    // the reserve stays out; Panzer, standing in the group and named by nothing, commits with
    // the line armies (THE GROUP COMMITS WHOLE)
    assert_eq!(
        group_creates(&game).first(),
        Some(&json!({"armies": ["Mitte", "Nord", "Panzer"]}))
    );
}

/// A supply read: (province, state, supply, demand, [(division id, army, organisation)]).
type SupplyRow<'a> = (i64, i64, f64, f64, Vec<(&'a str, &'a str, f64)>);

fn supply_read(rows: &[SupplyRow<'_>]) -> Value {
    let mut provinces = Vec::new();
    let mut divisions = Vec::new();
    for (p, s, sup, dem, divs) in rows {
        provinces.push(
            json!({"province_id": p, "state_id": s, "divisions": divs.len(),
            "division_ids": divs.iter().map(|d| d.0).collect::<Vec<_>>(),
            "supply": sup, "demand": dem, "headroom": sup - dem}),
        );
        for (id, army, org) in divs {
            divisions.push(
                json!({"id": id, "army": army, "province_id": p, "state_id": s,
                "organisation": org}),
            );
        }
    }
    json!({"provinces": provinces, "divisions": divisions})
}

fn ids(prefix: &str, n: usize) -> Vec<String> {
    (0..n).map(|i| format!("{prefix}{i:02}")).collect()
}

#[test]
fn a_province_carries_what_its_supply_feeds_and_never_more_than_the_battle_width() {
    // w43n1 (1 July 1940): 28 divisions draw 33.0 where province 9444 gives 3.4
    // (fed at half supply: 28 x 3.4 / 16.5)
    assert_eq!(stack_carry(28, 3.4, 33.0, STACK_MAX), 5);
    // fed in full but a battle takes no more than STACK_MAX
    assert_eq!(stack_carry(12, 30.0, 12.0, STACK_MAX), STACK_MAX);
    // starving: still one
    assert_eq!(stack_carry(3, 0.0, 9.0, STACK_MAX), 1);
    let a = ids("a", 12);
    let b = ids("b", 3);
    let mut divs: Vec<(&str, &str, f64)> = a.iter().map(|d| (d.as_str(), "Mitte", 50.0)).collect();
    // the most worn are taken first
    divs[0].2 = 10.0;
    divs[1].2 = 12.0;
    let read = supply_read(&[
        (9444, 980, 4.0, 12.0, divs),
        (
            6500,
            35,
            9.0,
            3.0,
            b.iter().map(|d| (d.as_str(), "Sued", 50.0)).collect(),
        ),
        // a stack of another army is not the operation's
        (
            11,
            64,
            1.0,
            9.0,
            vec![("x1", "Home", 50.0), ("x2", "Home", 50.0)],
        ),
    ]);
    let line: HashSet<String> = ["Mitte".to_string(), "Sued".to_string()].into();
    let seen = stack_read(&read, &line, &line, STACK_MAX);
    // 9444 starves (a third of its need) and carries 8 of 12 at half supply: 4 over; 6500
    // carries 11 (the width) and holds 3: room 8
    assert_eq!(seen.total, 4);
    assert_eq!(seen.room, 8);
    assert_eq!(seen.top, (12, 9444, 8));
    assert_eq!(seen.surplus.len(), 1);
    assert_eq!(seen.surplus[0].0, "Mitte");
    assert_eq!(seen.surplus[0].1.len(), 4);
    assert_eq!(
        &seen.surplus[0].1[..2],
        &["a00".to_string(), "a01".to_string()]
    );
}

#[test]
fn an_overstacked_line_forms_a_second_echelon_on_an_empty_border_state_and_feeds_it_forward() {
    let game = Game::new();
    let a = ids("a", 20);
    game.unit("Mitte", |r| {
        r["plan"] = json!([]);
        r["division_count"] = json!(20);
        r["states"] = json!({"980": 20});
    });
    let mut divs: Vec<(&str, &str, f64)> = a.iter().map(|d| (d.as_str(), "Mitte", 50.0)).collect();
    divs[5].2 = 5.0;
    *game.supply.borrow_mut() = Some(supply_read(&[(9444, 980, 4.0, 20.0, divs)]));
    // the mock's detach moves the ids of `divs` into the new army
    *game.divs.borrow_mut() = a
        .iter()
        .map(|d| (d.clone(), "Mitte".to_string(), "Inf".to_string()))
        .collect();
    let op = json!({"id": "op_low", "enemies": ["BEL"], "stacking": {"thin_at_war": true},
        "refine_days": 1});
    let units = vec![unit("Mitte".into(), Role::Line, &Value::Null, &Value::Null)];
    let wars = json!({"wars": {"enemies": ["BEL"]},
        "borders": [{"neighbour_tag": "BEL", "neighbour_states": [6],
                     "own_border_states": [980, 51, 36], "uncovered_own_border_states": [51]}]});
    let plan = json!({"war": {"me": "GER"}});
    let mut state = State::default();
    let roster = game.roster.borrow().clone();
    // before the attack executes nothing is read (the assembly stacks are not the front's)
    assert!(stacking(
        &mut state,
        &game,
        &plan,
        "op_low",
        &op,
        &units,
        &roster,
        &wars,
        "l",
        START - DAY
    )
    .unwrap()
    .is_none());
    assert!(game.calls.borrow().is_empty());
    state.done.insert(key("op_low", "exec"), START - DAY);
    // first read: seen once, nothing moves
    let first = stacking(
        &mut state, &game, &plan, "op_low", &op, &units, &roster, &wars, "l", START,
    )
    .unwrap();
    assert!(first.is_none(), "{first:?}");
    // second read half a day later: over the carry at half supply, half the stack at most, a
    // third of the army (6) at once, most worn first
    let line = stacking(
        &mut state,
        &game,
        &plan,
        "op_low",
        &op,
        &units,
        &roster,
        &wars,
        "l",
        START + DAY / 2,
    )
    .unwrap()
    .unwrap();
    assert!(line.contains("second echelon Army 9"), "{line}");
    let det = game
        .orders()
        .into_iter()
        .find(|(n, _)| n == "hoi4_army_detach")
        .unwrap();
    let moved = strings(&det.1, "divisions");
    assert_eq!(moved.len(), 6);
    assert_eq!(moved[0], "a05");
    assert_eq!(
        state.layout.get("echelon:op_low:Mitte").map(String::as_str),
        Some("Army 9")
    );
    // the echelon is the operation's (no other step orders it)
    let full_plan = json!({"operations": [op.clone()]});
    let roster = game.roster.borrow().clone();
    assert!(owned_names(&state, &full_plan, &op, &roster).contains(&"Army 9".to_string()));
    // it takes a real order: an area defence of the border state no division holds
    game.unit("Army 9", |r| {
        r["division_count"] = json!(6);
        r["states"] = json!({"980": 6});
    });
    game.unit("Mitte", |r| r["division_count"] = json!(14));
    let roster = game.roster.borrow().clone();
    let line = stacking(
        &mut state,
        &game,
        &plan,
        "op_low",
        &op,
        &units,
        &roster,
        &wars,
        "l",
        START + DAY,
    )
    .unwrap()
    .unwrap();
    assert!(line.contains("holds its station 51"), "{line}");
    assert!(game
        .orders()
        .iter()
        .any(|(n, a)| n == "hoi4_army_area_defense"
            && a["army"] == json!("Army 9")
            && a["states"] == json!([51])));
    // provinces taken: the line spread over six provinces with room for the echelon
    game.unit(
        "Army 9",
        |r| r["plan"] = json!([{"kind": "area_defense", "executing": false, "offensives": []}]),
    );
    let roster = game.roster.borrow().clone();
    let spread: Vec<SupplyRow<'_>> = (0..6)
        .map(|i| {
            (
                100 + i as i64,
                980,
                20.0,
                1.0,
                vec![(["b0", "b1", "b2", "b3", "b4", "b5"][i], "Mitte", 50.0)],
            )
        })
        .collect();
    *game.supply.borrow_mut() = Some(supply_read(&spread));
    let line = stacking(
        &mut state,
        &game,
        &plan,
        "op_low",
        &op,
        &units,
        &roster,
        &wars,
        "l",
        START + 2 * DAY,
    )
    .unwrap()
    .unwrap();
    assert!(line.contains("fed forward into Mitte"), "{line}");
    assert!(game.orders().iter().any(|(n, a)| n == "hoi4_army_merge"
        && a["source"] == json!("Army 9")
        && a["target"] == json!("Mitte")));
    assert!(!state.layout.contains_key("echelon:op_low:Mitte"));
}

#[test]
fn stacking_off_sends_nothing() {
    let game = Game::new();
    let op = json!({"id": "op_low", "enemies": ["BEL"], "stacking": false});
    let units = vec![unit("Mitte".into(), Role::Line, &Value::Null, &Value::Null)];
    let mut state = State::default();
    let roster = game.roster.borrow().clone();
    let r = stacking(
        &mut state,
        &game,
        &json!({}),
        "op_low",
        &op,
        &units,
        &roster,
        &json!({}),
        "l",
        START,
    )
    .unwrap();
    assert!(r.is_none());
    assert!(game.calls.borrow().is_empty());
}

#[test]
fn a_reserve_left_on_its_old_groups_front_has_that_front_cleared_once() {
    // w43o1 (23 Nov 2026): the group formed again from Mitte and Nord as Army Group 3; Armee Süd
    // stayed in Army Group 1, whose old front kept 11 of its divisions in Friesland 36
    let game = Game::new();
    let op = json!({"id": "op_low", "reserve_orders": true,
        "reserve": [{"army": "Sued", "station": 55}]});
    game.unit("Sued", |r| {
        r["states"] = json!({"36": 20});
        r["division_count"] = json!(20);
    });
    game.unit("Army Group 1", |r| {
        r["plan"] = json!([{"kind": "front", "executing": false, "offensives": []}])
    });
    let roster = game.roster.borrow().clone();
    let wars = json!({"wars": {"enemies": ["HOL"]}, "borders": []});
    let mut state = State::default();
    let line = reserves(
        &mut state,
        &game,
        "op_low",
        &op,
        Some("Army Group 3"),
        &roster,
        &wars,
        "l",
        START,
    )
    .unwrap()
    .unwrap();
    assert!(line.contains("clear Army Group 1"), "{line}");
    // once: the next call goes on with the reserve's own march
    game.unit("Army Group 1", |r| r["plan"] = json!([]));
    let roster = game.roster.borrow().clone();
    let line = reserves(
        &mut state,
        &game,
        "op_low",
        &op,
        Some("Army Group 3"),
        &roster,
        &wars,
        "l",
        START + 1,
    )
    .unwrap()
    .unwrap();
    assert!(!line.contains("clear Army Group 1"), "{line}");
}

#[test]
fn only_standing_infantry_of_the_line_goes_back_never_the_armour_or_its_sector() {
    let mut read = supply_read(&[
        (
            9522,
            51,
            4.0,
            24.0,
            vec![
                ("m1", "Mitte", 20.0),
                ("m2", "Mitte", 30.0),
                ("m3", "Mitte", 40.0),
                ("m4", "Mitte", 50.0),
                ("m5", "Mitte", 55.0),
                ("m6", "Mitte", 60.0),
            ],
        ),
        // the armour's state: the breakthrough sector keeps its weight
        (
            7000,
            56,
            2.0,
            24.0,
            vec![
                ("p1", "Panzer", 10.0),
                ("n1", "Mitte", 10.0),
                ("n2", "Mitte", 10.0),
                ("n3", "Mitte", 10.0),
                ("n4", "Mitte", 10.0),
                ("n5", "Mitte", 10.0),
            ],
        ),
    ]);
    // m1 attacks (it heads for its battle): it is never taken out of it
    read["divisions"][0]["heading_province"] = json!(6500);
    let line: HashSet<String> = ["Mitte".to_string(), "Panzer".to_string()].into();
    let pick: HashSet<String> = ["Mitte".to_string()].into();
    let seen = stack_read(&read, &line, &pick, STACK_MAX);
    // 9522 starves (a sixth of its need), carries 2 of 6: 4 over, half the stack (3) at most, the
    // standing m2..m4 most worn first; nothing from the armour's state 56
    assert_eq!(seen.total, 3);
    assert_eq!(
        seen.surplus,
        vec![(
            "Mitte".to_string(),
            vec!["m2".to_string(), "m3".to_string(), "m4".to_string()]
        )]
    );
}

/// Luxembourg as the province graph reads it (w46d1, live): two forest provinces; from Moselland
/// 42 one border over the Sauer (small river) and one dry, from the Belgian Ardennes 980 dry.
fn lux_graph() -> Value {
    json!({"provinces": [
        {"province": 6583, "state_id": 8, "terrain": "forest", "attack_modifier": -0.15,
         "forts_in_history": [],
         "borders": [{"province": 9418, "river": null, "river_share": 0.0},
                     {"province": 9575, "river": "small", "river_share": 1.0, "river_attack_penalty": -0.3},
                     {"province": 11435, "river": null, "river_share": 0.0}]},
        {"province": 11435, "state_id": 42, "terrain": "hills", "attack_modifier": -0.25, "borders": []},
        {"province": 9575, "state_id": 42, "terrain": "hills", "attack_modifier": -0.25, "borders": []},
        {"province": 9418, "state_id": 980, "terrain": "forest", "attack_modifier": -0.15, "borders": []}]})
}

#[test]
fn a_crossing_counts_the_river_the_ground_and_the_forts() {
    let ways = crossings(&lux_graph(), &[8], &[42]);
    assert_eq!(ways.len(), 2);
    let over = ways.iter().find(|w| w.1 == 9575).unwrap();
    assert!((over.2 - -0.45).abs() < 1e-9, "{over:?}");
    let dry = ways.iter().find(|w| w.1 == 11435).unwrap();
    assert!((dry.2 - -0.15).abs() < 1e-9, "{dry:?}");
    assert!(over.3.contains("small river"), "{over:?}");
    let fort =
        json!({"attack_modifier": 0.0, "forts_in_history": [{"building": "bunker", "level": 2}]});
    assert!((crossing_penalty(&fort, &json!({})) - -0.3).abs() < 1e-9);
}

#[test]
fn a_guarded_wing_never_attacks_alone_and_never_waits_forever() {
    let game = Game::new();
    *game.graph.borrow_mut() = Some(lux_graph());
    game.unit("Nord", |r| r["division_count"] = json!(7));
    game.unit("Mitte", |r| r["plan_executing"] = json!(false));
    let plan = json!({"war": {"me": "GER"}, "operations": [
        {"id": "op_low", "war": "w", "enemies": ["BEL"]},
        {"id": "op_lux", "war": "w", "enemies": ["LUX"], "assault_guard": "odds"}]});
    let op = plan["operations"][1].clone();
    let mut u = unit("Nord".into(), Role::Wing, &Value::Null, &Value::Null);
    u.enemy = "LUX".into();
    u.line = vec![8];
    let wars = |french: i64| {
        json!({"wars": {"enemies": ["LUX", "FRA"]}, "borders": [
        {"neighbour_tag": "LUX", "neighbour_states": [8], "own_border_states": [42],
         "neighbour_divisions_in_border_states": 1, "hostile_divisions_in_border_states": french}]})
    };
    let mut state = State::default();
    state.op_owned.insert("Mitte".into(), "op_low".into());
    let guarded = |state: &mut State, french: i64, at: GameHour| {
        let roster = game.roster.borrow().clone();
        assert_guard(state, &game, &plan, &op, &u, &roster, &wars(french), at)
    };
    // one French division, a dry way in, but no attack of the war executes: never alone
    assert!(guarded(&mut state, 1, START));
    let note = state.op_notes["op_lux:guard:Nord"].clone();
    assert!(note.contains("never alone"), "{note}");
    // the group's attack on Belgium executes: from its own border alone it still holds
    game.unit("Mitte", |r| r["plan_executing"] = json!(true));
    assert!(guarded(&mut state, 1, START + DAY));
    let note = state.op_notes["op_lux:guard:Nord"].clone();
    assert!(
        note.contains("joint attack of Mitte") && note.contains("no second direction"),
        "{note}"
    );
    // three weeks of the joint attack: it attacks when the odds allow an assault (7 against 1)...
    assert!(!guarded(&mut state, 1, START + 23 * DAY));
    // ...never 7 against fifteen French (w47c1: 17.4k lost to none)
    assert!(guarded(&mut state, 15, START + 25 * DAY));
    assert!(
        state.op_notes["op_lux:guard:Nord"].contains("waits for a second direction or the odds")
    );
    // a second direction (the Belgian Ardennes taken): it attacks with the joint attack at once
    let mut fresh = State::default();
    fresh.op_owned.insert("Mitte".into(), "op_low".into());
    assert!(guarded(&mut fresh, 4, START));
    let roster = game.roster.borrow().clone();
    let two = json!({"wars": {"enemies": ["LUX", "FRA"]}, "borders": [
        {"neighbour_tag": "LUX", "neighbour_states": [8], "own_border_states": [42, 980],
         "neighbour_divisions_in_border_states": 1, "hostile_divisions_in_border_states": 4}]});
    assert!(!assert_guard(
        &mut fresh,
        &game,
        &plan,
        &op,
        &u,
        &roster,
        &two,
        START + DAY
    ));
    assert!(fresh.op_notes["op_lux:guard:Nord"].contains("second direction from state(s) [980]"));
    // armour is never held
    let mut panzer = u.clone();
    panzer.spear = true;
    let roster = game.roster.borrow().clone();
    assert!(!assault_guarded(
        &mut fresh,
        &game,
        &plan,
        "op_lux",
        &op,
        &panzer,
        &roster,
        &wars(15),
        &[],
        START + 2 * DAY
    ));
}

#[test]
fn a_wing_attacks_at_once_in_one_blow_with_the_armour_into_the_same_state() {
    // w47r1 (23 Nov 2026): Armee West Süd held ten days behind Panzergruppe Ost's attack on
    // Luxembourg; the owner: Luxembourg is taken in the opening, never by a weak wing alone
    let game = Game::new();
    *game.graph.borrow_mut() = Some(lux_graph());
    game.unit("Nord", |r| r["division_count"] = json!(7));
    game.unit("Panzer", |r| {
        r["division_count"] = json!(9);
        r["plan_executing"] = json!(true);
        r["plan"] = json!([{"kind": "front", "executing": true,
            "offensives": [{"target_states": [8], "executing": true}]}]);
    });
    let plan = json!({"war": {"me": "GER"}, "operations": [
        {"id": "op_lux", "war": "w", "enemies": ["LUX"], "assault_guard": "odds"}]});
    let op = plan["operations"][0].clone();
    let mut u = unit("Nord".into(), Role::Wing, &Value::Null, &Value::Null);
    u.enemy = "LUX".into();
    u.line = vec![8];
    let wars = |french: i64| {
        json!({"wars": {"enemies": ["LUX", "FRA"]}, "borders": [
        {"neighbour_tag": "LUX", "neighbour_states": [8], "own_border_states": [42],
         "neighbour_divisions_in_border_states": 1, "hostile_divisions_in_border_states": french}]})
    };
    let mut state = State::default();
    state.op_owned.insert("Panzer".into(), "op_lux".into());
    let roster = game.roster.borrow().clone();
    // 7 + 9 against 4: one blow at once
    assert!(!assert_guard(
        &mut state,
        &game,
        &plan,
        &op,
        &u,
        &roster,
        &wars(4),
        START
    ));
    assert!(state.op_notes["op_lux:guard:Nord"].contains("one blow with Panzer"));
    // 16 against 10: still never a weak attack
    let mut later = State::default();
    later.op_owned.insert("Panzer".into(), "op_lux".into());
    assert!(assert_guard(
        &mut later,
        &game,
        &plan,
        &op,
        &u,
        &roster,
        &wars(10),
        START
    ));
}

#[allow(clippy::too_many_arguments)]
fn assert_guard(
    state: &mut State,
    game: &Game,
    plan: &Value,
    op: &Value,
    u: &Unit,
    roster: &Value,
    wars: &Value,
    at: GameHour,
) -> bool {
    assault_guarded(state, game, plan, "op_lux", op, u, roster, wars, &[], at)
}

#[test]
fn a_reserve_released_at_war_goes_forward_in_parts_into_the_line_army_with_room() {
    // w46c1: Armee Süd (20) was released while the war ran and stood at its station all war
    let game = Game::new();
    for (name, n) in [("Mitte", 14), ("Sued", 20)] {
        game.unit(name, |r| {
            r["division_count"] = json!(n);
            r["plan"] = json!([]);
        });
    }
    game.unit("Sued", |r| r["army_group"] = Value::Null);
    let s_ids = ids("s", 20);
    let mut rows: Vec<SupplyRow<'_>> = (0..7)
        .map(|i| {
            (
                100 + i as i64,
                35,
                20.0,
                1.0,
                vec![(["m0", "m1", "m2", "m3", "m4", "m5", "m6"][i], "Mitte", 50.0)],
            )
        })
        .collect();
    rows.push((
        900,
        55,
        50.0,
        20.0,
        s_ids.iter().map(|d| (d.as_str(), "Sued", 60.0)).collect(),
    ));
    *game.supply.borrow_mut() = Some(supply_read(&rows));
    let op = json!({"id": "op_low", "enemies": ["HOL"], "reserve_orders": true,
        "reserve": [{"army": "Sued", "station": 55, "to": "operation"}]});
    let mut grp = unit(
        "Army Group 1".into(),
        Role::Group,
        &Value::Null,
        &Value::Null,
    );
    grp.members = vec!["Mitte".into()];
    let units = vec![grp];
    let mut state = State::default();
    state.done.insert(key("op_low", "exec"), START - DAY);
    state.done.insert(key("op_low", "rel:Sued"), START - DAY);
    let roster = game.roster.borrow().clone();
    let wars = json!({"wars": {"enemies": ["HOL"]}, "borders": []});
    let line = stacking(
        &mut state,
        &game,
        &json!({"war": {"me": "GER"}}),
        "op_low",
        &op,
        &units,
        &roster,
        &wars,
        "l",
        START,
    )
    .unwrap()
    .unwrap();
    assert!(
        line.contains("echelon Sued fed forward into Mitte"),
        "{line}"
    );
    // Mitte takes 10 more (to 24): ten of Sued's divisions, never a merge of all 20
    let det = game
        .orders()
        .into_iter()
        .find(|(n, a)| n == "hoi4_army_detach" && a["into"] == json!("Mitte"))
        .unwrap();
    assert_eq!(strings(&det.1, "divisions").len(), 10);
    assert!(!game.orders().iter().any(|(n, _)| n == "hoi4_army_merge"));
}

#[test]
fn the_armour_of_an_enemy_out_joins_the_groups_enemy_the_wing_goes_to_the_next() {
    // w53a: Panzergruppe Ost took Luxembourg and went alone at Belgium while the group's
    // attack on Holland stalled without it
    let mk = |name: &str, role: Role, enemy: &str| {
        let mut u = unit(
            name.into(),
            role,
            &json!({"enemy": enemy, "own_states": [42], "click_state": 8}),
            &json!({}),
        );
        u.enemy = enemy.into();
        u
    };
    let mut units = vec![
        mk("Army Group 1", Role::Group, "HOL"),
        mk("Panzer Ost", Role::Mobile, "LUX"),
        mk("West Sued", Role::Wing, "LUX"),
    ];
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    hand_over_out(&mut units, &s(&["LUX"]), &[], &s(&["BEL", "HOL"]), "BEL");
    assert_eq!(units[1].enemy, "HOL", "the armour joins the group's enemy");
    assert_eq!(
        units[2].enemy, "BEL",
        "the wing goes to the next enemy at war"
    );
    // the group's enemy beaten: the armour goes on to the next
    let mut units = vec![
        mk("Army Group 1", Role::Group, "HOL"),
        mk("Panzer Ost", Role::Mobile, "LUX"),
    ];
    hand_over_out(
        &mut units,
        &s(&["LUX"]),
        &s(&["HOL"]),
        &s(&["BEL", "HOL"]),
        "BEL",
    );
    assert_eq!(units[1].enemy, "BEL");
}

#[test]
fn an_owned_army_over_its_limit_may_give_its_excess_away_and_nothing_else() {
    // s55a (23 Nov 2026): Armee West Nord held 35 divisions the whole war, every split refused
    let owned: HashMap<String, String> =
        [("Armee West Nord".to_string(), "op_lowlands".to_string())].into();
    let mut over = json!({"army": "Armee West Nord", "count": 11, "over_limit": true});
    assert_eq!(guard(&owned, "hoi4_army_split", &mut over), None);
    assert!(
        over.get("over_limit").is_none(),
        "the marker never reaches the tool"
    );
    let mut plain = json!({"army": "Armee West Nord", "count": 11});
    assert!(guard(&owned, "hoi4_army_split", &mut plain).is_some());
    // with nothing owned the marker is still taken off
    let mut free = json!({"army": "Army 3", "count": 2, "over_limit": true});
    assert_eq!(guard(&HashMap::new(), "hoi4_army_split", &mut free), None);
    assert!(free.get("over_limit").is_none());
}

struct Roster;
impl Tools for Roster {
    fn call(&self, name: &str, _args: &Value) -> Result<Value> {
        Ok(match name {
            "hoi4_armies" => json!({"armies": [
                {"army": "Army 2", "division_count": 9},
                {"army": "Army 9", "division_count": 3}]}),
            _ => json!({}),
        })
    }
}

#[test]
fn a_role_formed_in_the_same_paused_hour_becomes_the_operations_at_once() {
    // s7m: Panzergruppe Nord formed after the first ownership read of the paused setup hour, an
    // idle hold sent it back, its march never ended and the declaration waited
    let plan =
        json!({"operations": [{"id": "op_pol", "formations": [{"army": "Nord"}, {"army": "Pz"}]}]});
    let mut state = State::default();
    state.done.insert(key("op_pol", "prep"), START);
    state.layout.insert("Nord".into(), "Army 2".into());
    refresh(&mut state, &Roster, &plan, START);
    assert!(state.op_owned.contains_key("Army 2"));
    assert!(!state.op_owned.contains_key("Army 9"));
    state.layout.insert("Pz".into(), "Army 9".into());
    refresh(&mut state, &Roster, &plan, START);
    assert_eq!(
        state.op_owned.get("Army 9").map(String::as_str),
        Some("op_pol")
    );
}

/// The Poland plan with the armour on its spearhead (axis 67-87-10), at war, prepared.
fn spear_at_war() -> (Game, Value, State) {
    let game = Game::new();
    let mut plan = plan();
    plan["operations"][0]["formations"][2] = json!({"army": "Panzer", "role": "mobile",
        "drive": "strike", "front": {"enemy": "POL", "own_states": [67], "click_state": 87},
        "offensive": {"axis": [67, 87, 10], "target_line": [87, 10, 92]}});
    let mut state = State::default();
    game.unit("Panzer", |r| r["states"] = json!({"67": 12}));
    prepared(&mut state, &game, &plan);
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    game.calls.borrow_mut().clear();
    (game, plan, state)
}

#[test]
fn at_war_a_spearhead_whose_line_moved_loses_only_its_arrow_never_its_front() {
    // t1939j (24 Nov 2026): each new line cleared Panzergruppe Ost's orders; the front would not
    // draw again where the army stood and it fought without an order for weeks
    let (game, plan, mut state) = spear_at_war();
    *game.held.borrow_mut() = vec![87, 10];
    for hour in 0..6 {
        burst(&mut state, &game, &plan, START + 2 * DAY + hour * 3);
    }
    let panzer: Vec<(String, Value)> = game
        .orders()
        .into_iter()
        .filter(|(_, a)| a["army"] == json!("Panzer"))
        .collect();
    assert!(
        !panzer.iter().any(|(_, a)| a["clear_orders"] == json!(true)),
        "{panzer:?}"
    );
    let delete = panzer
        .iter()
        .position(|(_, a)| a["delete_order"].is_i64())
        .expect("the old arrow deleted alone");
    assert!(
        panzer[delete..]
            .iter()
            .any(|(n, a)| n == "hoi4_army_offensive" && a["spearhead"] == json!(true)),
        "a new spearhead on the standing front: {panzer:?}"
    );
    assert!(has_front(&game.roster.borrow(), "Panzer"));
}

#[test]
fn at_war_a_refused_segment_front_gives_way_to_the_plain_click_in_the_same_call() {
    let (game, plan, mut state) = spear_at_war();
    *game.own_border.borrow_mut() = vec![67];
    *game.refuse_segment.borrow_mut() = true;
    game.unit("Panzer", |r| r["plan"] = json!([]));
    let lines = burst(&mut state, &game, &plan, START + 3 * DAY);
    let fronts: Vec<Value> = game
        .orders()
        .into_iter()
        .filter(|(n, a)| n == "hoi4_army_front" && a["army"] == json!("Panzer"))
        .map(|(_, a)| a)
        .collect();
    assert_eq!(fronts.len(), 2, "{fronts:?}");
    assert!(fronts[0]["from_state"].is_i64());
    assert!(fronts[1].get("from_state").is_none());
    assert!(has_front(&game.roster.borrow(), "Panzer"), "{lines:?}");
    // the next loss: the plain click straight away, no segment again in this operation
    game.calls.borrow_mut().clear();
    game.unit("Panzer", |r| r["plan"] = json!([]));
    burst(&mut state, &game, &plan, START + 4 * DAY);
    let fronts: Vec<Value> = game
        .orders()
        .into_iter()
        .filter(|(n, a)| n == "hoi4_army_front" && a["army"] == json!("Panzer"))
        .map(|(_, a)| a)
        .collect();
    assert_eq!(fronts.len(), 1, "{fronts:?}");
    assert!(fronts[0].get("from_state").is_none());
}

#[test]
fn a_front_the_game_keeps_removing_is_drawn_again_every_time() {
    // the draws are counted per click only while no front reads back: eight losses, eight fronts
    let (game, plan, mut state) = spear_at_war();
    for loss in 0..8 {
        game.unit("Panzer", |r| r["plan"] = json!([]));
        burst(&mut state, &game, &plan, START + (3 + loss) * DAY);
        assert!(
            has_front(&game.roster.borrow(), "Panzer"),
            "loss {loss}: {:?}",
            state.op_notes
        );
    }
}

/// t1939k (24 Nov 2026): the handover draws the fronts of the next operation's orderless
/// holds, mobile and wing armies first (holds ahead), never one that has an order or is not on
/// the map, and never a group or line army (the group's front carries those).
#[test]
fn handover_draws_orderless_own_front_armies_first() {
    let f = json!({"enemy": "HOL", "own_states": [57], "click_state": 36});
    let o = json!({"target_line": [35, 7]});
    let units = vec![
        unit("Panzer".into(), Role::Mobile, &f, &o),
        unit("Nord".into(), Role::Wing, &f, &o),
        unit("WestA".into(), Role::Hold, &f, &o),
        unit("Mitte".into(), Role::Line, &f, &o),
        unit("Army Group 1".into(), Role::Group, &f, &o),
        unit("Gone".into(), Role::Wing, &f, &o),
    ];
    let roster = json!({"armies": [
        {"army": "Panzer", "plan": []},
        {"army": "Nord", "plan": [{"kind": "front"}]},
        {"army": "WestA", "plan": []},
        {"army": "Mitte", "plan": []}],
        "army_groups": [{"army_group": "Army Group 1", "plan": []}]});
    let names: Vec<&str> = orderless_first(&units, &roster)
        .iter()
        .map(|u| u.name.as_str())
        .collect();
    assert_eq!(names, vec!["WestA", "Panzer"]);
}

// THE WEST (engineer 88): width, axis, commands, planning, pockets, opening

/// One graph row as hoi4_province_graph gives it.
fn graph_row(p: i64, state: i64, terrain: &str, borders: &[(i64, f64)]) -> Value {
    let (w, sw, atk, mv) = match terrain {
        "forest" => (60.0, 30.0, -0.15, 1.5),
        _ => (70.0, 35.0, 0.0, 1.0),
    };
    json!({"province": p, "state_id": state, "terrain": terrain, "combat_width": w,
           "combat_support_width": sw, "attack_modifier": atk, "movement_cost": mv,
           "forts_in_history": [],
           "borders": borders.iter().map(|(q, r)| if *r < 0.0 {
               json!({"province": q, "river": "large", "river_share": 1.0, "river_attack_penalty": r})
           } else {
               json!({"province": q, "river": null, "river_share": 0.0})
           }).collect::<Vec<_>>()})
}

#[test]
fn by_width_a_front_province_keeps_what_its_battles_take_and_the_battle_reserve_goes_back() {
    let game = Game::new();
    let a = ids("a", 12);
    game.unit("Mitte", |r| {
        r["plan"] = json!([]);
        r["division_count"] = json!(14);
        r["states"] = json!({"51": 14});
    });
    // twelve standing in province 1 (fed in full) against one forest province of Belgium;
    // two more wait in the reserve of the battle for it
    let mut read = supply_read(&[(
        1,
        51,
        30.0,
        12.0,
        a.iter().map(|d| (d.as_str(), "Mitte", 50.0)).collect(),
    )]);
    for d in read["divisions"].as_array_mut().unwrap() {
        d["template"] = json!("Inf");
    }
    read["divisions"].as_array_mut().unwrap().extend([
        json!({"id": "r1", "army": "Mitte", "template": "Inf", "province_id": 2,
                        "state_id": 51, "organisation": 40.0, "heading_province": 10}),
        json!({"id": "r2", "army": "Mitte", "template": "Inf", "province_id": 2,
                        "state_id": 51, "organisation": 45.0, "heading_province": 10}),
    ]);
    *game.supply.borrow_mut() = Some(read);
    *game.templates.borrow_mut() =
        Some(json!({"templates": [{"template": "Inf", "totals": {"width": 24.0}}]}));
    *game.graph.borrow_mut() = Some(json!({"provinces": [
        graph_row(1, 51, "plains", &[(10, 0.0), (2, 0.0)]),
        graph_row(2, 51, "plains", &[(1, 0.0)]),
        graph_row(10, 980, "forest", &[(1, 0.0)])]}));
    *game.battles.borrow_mut() = Some(json!({"list": [{"province": 10, "our_side": "attacker",
        "attacker": {"engaged": 3, "reserve": 2, "reserve_divisions": [
            {"id": "r1", "org": 40.0}, {"id": "r2", "org": 45.0}]}}]}));
    *game.divs.borrow_mut() = a
        .iter()
        .chain(["r1".to_string(), "r2".to_string()].iter())
        .map(|d| (d.clone(), "Mitte".to_string(), "Inf".to_string()))
        .collect();
    let wars = json!({"wars": {"enemies": ["BEL"]},
        "borders": [{"neighbour_tag": "BEL", "neighbour_states": [980],
                     "own_border_states": [51, 36], "uncovered_own_border_states": [36]}]});
    let units = vec![unit("Mitte".into(), Role::Line, &Value::Null, &Value::Null)];
    let plan = json!({"war": {"me": "GER"}});
    let roster = game.roster.borrow().clone();
    // without the key: twelve in a province the supply feeds is one over STACK_MAX, no echelon
    let op = json!({"id": "op_low", "enemies": ["BEL"]});
    let mut state = State::default();
    state.done.insert(key("op_low", "exec"), START - DAY);
    for t in [START, START + DAY] {
        assert!(
            stacking(&mut state, &game, &plan, "op_low", &op, &units, &roster, &wars, "l", t)
                .unwrap()
                .is_none()
        );
    }
    // by width: the forest takes three 24-wide divisions from one side; nine stand over it
    // (half the stack moves at most: six) and the battle's two waiting divisions go back too
    let op = json!({"id": "op_low", "enemies": ["BEL"],
        "stacking": {"by_width": true, "thin_at_war": true}});
    let mut state = State::default();
    state.done.insert(key("op_low", "exec"), START - DAY);
    assert!(
        stacking(&mut state, &game, &plan, "op_low", &op, &units, &roster, &wars, "l", START)
            .unwrap()
            .is_none()
    );
    let note = state.op_notes["op_low:width"].clone();
    assert!(note.contains("take 3 line divisions"), "{note}");
    assert!(note.contains("1: 12 of 3"), "{note}");
    assert!(note.contains("2 waiting beyond the width"), "{note}");
    game.calls.borrow_mut().clear();
    let line = stacking(
        &mut state,
        &game,
        &plan,
        "op_low",
        &op,
        &units,
        &roster,
        &wars,
        "l",
        START + DAY,
    )
    .unwrap()
    .expect("an echelon");
    let detach = game
        .orders()
        .into_iter()
        .find(|(n, _)| n == "hoi4_army_detach")
        .expect("detach");
    // a third of the 14-division army at once
    assert_eq!(detach.1["divisions"].as_array().unwrap().len(), 4, "{line}");
}

#[test]
fn a_province_cap_by_width_thins_the_armours_sector_only_past_its_battles() {
    let divs: Vec<String> = ids("m", 8);
    let mut rows: Vec<(&str, &str, f64)> =
        divs.iter().map(|d| (d.as_str(), "Mitte", 50.0)).collect();
    rows.push(("p1", "Panzer", 60.0));
    let read = supply_read(&[(7000, 56, 30.0, 9.0, rows)]);
    let line: HashSet<String> = ["Mitte".to_string(), "Panzer".to_string()].into();
    let pick: HashSet<String> = ["Mitte".to_string()].into();
    // the armour's state keeps its weight without the width
    assert_eq!(stack_read(&read, &line, &pick, STACK_MAX).total, 0);
    // its battles take 5: four of the infantry go back (half the stack at most)
    let caps: HashMap<i64, i64> = [(7000, 5)].into();
    let seen = stack_read_caps(&read, &line, &pick, STACK_MAX, &caps, true);
    assert_eq!(seen.total, 4);
    // within its cap nothing moves
    let caps: HashMap<i64, i64> = [(7000, 9)].into();
    assert_eq!(
        stack_read_caps(&read, &line, &pick, STACK_MAX, &caps, true).total,
        0
    );
}

fn spear_at_war_by_command() -> (Game, Value, State) {
    let game = Game::new();
    let mut plan = plan();
    plan["operations"][0]["orders_by"] = json!("command");
    plan["operations"][0]["formations"][2] = json!({"army": "Panzer", "role": "mobile",
        "drive": "strike", "front": {"enemy": "POL", "own_states": [67], "click_state": 87},
        "offensive": {"axis": [67, 87, 10], "target_line": [87, 10, 92]}});
    let mut state = State::default();
    game.unit("Panzer", |r| r["states"] = json!({"67": 12}));
    prepared(&mut state, &game, &plan);
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    game.calls.borrow_mut().clear();
    (game, plan, state)
}

#[test]
fn by_command_a_spearhead_whose_line_moved_gets_a_new_path_on_the_same_order() {
    let (game, plan, mut state) = spear_at_war_by_command();
    let mut rows = Vec::new();
    for s in [85, 86, 87, 88, 10, 92] {
        rows.push(graph_row(s * 10, s, "plains", &[(s * 10 + 1, 0.0)]));
        rows.push(graph_row(s * 10 + 1, s, "plains", &[(s * 10, 0.0)]));
    }
    *game.graph.borrow_mut() = Some(json!({"provinces": rows}));
    *game.held.borrow_mut() = vec![87, 10];
    for hour in 0..6 {
        burst(&mut state, &game, &plan, START + 2 * DAY + hour * 3);
    }
    let panzer: Vec<(String, Value)> = game
        .orders()
        .into_iter()
        .filter(|(_, a)| a["army"] == json!("Panzer"))
        .collect();
    assert!(
        !panzer
            .iter()
            .any(|(_, a)| a["delete_order"].is_i64() || a["clear_orders"] == json!(true)),
        "{panzer:?}"
    );
    let repath = panzer
        .iter()
        .find(|(n, a)| n == "hoi4_army_offensive" && a["replace_order"].is_i64())
        .expect("a new path on the standing order");
    assert_eq!(repath.1["spearhead"], json!(true));
    assert!(!repath.1["target_provinces"].as_array().unwrap().is_empty());
    // the fronts of the prepare went out with the in-process arguments
    let game2 = Game::new();
    let mut plan2 = plan.clone();
    plan2["operations"][0]["orders_by"] = json!("command");
    let mut state2 = State::default();
    burst(&mut state2, &game2, &plan2, START);
    let group = game2
        .orders()
        .into_iter()
        .find(|(n, a)| n == "hoi4_army_front" && a["army"] == json!("Army Group 1"))
        .expect("the group's front");
    assert_eq!(group.1["against"], json!("POL"));
    assert_eq!(group.1["own_states"], json!([66, 67]));
}

#[test]
fn planning_gate_holds_the_ready_until_the_bonus_is_built_or_the_date_forces_it() {
    let units = vec![unit("Mitte".into(), Role::Line, &Value::Null, &Value::Null)];
    let roster = |p: f64| json!({"armies": [{"army": "Mitte", "division_count": 10, "planning": {"mean": p}}]});
    let op = json!({"planning_gate": true});
    let mut state = State::default();
    // no key: nothing held
    assert!(planning_held(&mut state, "o", &json!({}), &units, &roster(0.0), START).is_none());
    let held = planning_held(&mut state, "o", &op, &units, &roster(0.10), START);
    assert!(held.unwrap().contains("planning 0.10"));
    // rising: still held a day later
    assert!(planning_held(&mut state, "o", &op, &units, &roster(0.14), START + DAY).is_some());
    // built to 0.9 of the cap: ready
    assert!(planning_held(&mut state, "o", &op, &units, &roster(0.28), START + 2 * DAY).is_none());
    // a plateau two days long below the cap counts as the cap (doctrines, no HQ)
    let mut state = State::default();
    assert!(planning_held(&mut state, "o", &op, &units, &roster(0.2), START).is_some());
    assert!(planning_held(&mut state, "o", &op, &units, &roster(0.2), START + DAY).is_some());
    assert!(planning_held(&mut state, "o", &op, &units, &roster(0.2), START + 2 * DAY).is_none());
    // a dated declaration waits at most max_wait_days past its date
    let mut state = State::default();
    mark_due(&mut state, "o", START);
    let op = json!({"planning_gate": {"max_wait_days": 3}});
    let mut p = 0.01;
    for d in 0..3 {
        p += 0.02;
        let note = planning_held(&mut state, "o", &op, &units, &roster(p), START + d * DAY);
        assert!(note.is_some_and(|n| n.contains("days past its date")));
    }
    assert!(planning_held(&mut state, "o", &op, &units, &roster(0.09), START + 3 * DAY).is_none());
}

#[test]
fn two_spearheads_are_painted_to_a_pockets_neck_and_hold_the_closed_ring() {
    let game = Game::new();
    for (name, main) in [("Panzer", 67), ("Sued", 66)] {
        game.unit(name, |r| r["main_state"] = json!(main));
    }
    // a neck state without a victory point is a pocket for the spearheads
    *game.pockets.borrow_mut() = json!({"pockets": [
        {"already_cut_off": false, "pincer_ready": true, "capital_inside": false,
         "neck_states": [{"state_id": 87}], "pocket_states": [{"state_id": 86}]}]});
    let mut state = State::default();
    for (s, p) in [(87, (10.0, 0.0)), (67, (12.0, 0.0)), (66, (9.0, 0.0))] {
        state.centers.insert(s, p);
    }
    let f = json!({"enemy": "POL", "own_states": [67]});
    let o = json!({"target_line": [10]});
    let mut units = vec![
        unit("Panzer".into(), Role::Mobile, &f, &o),
        unit("Sued".into(), Role::Mobile, &f, &o),
    ];
    for u in units.iter_mut() {
        u.spear = true;
    }
    let plan = json!({"war": {"me": "GER"}});
    let roster = game.roster.borrow().clone();
    let enemies = vec!["POL".to_string()];
    let op = json!({"id": "op", "spear_pockets": true});
    // off without the key
    let mut off = units.clone();
    spear_pockets(
        &mut state,
        &game,
        &plan,
        &json!({}),
        &mut off,
        &roster,
        &enemies,
        &enemies,
        START,
    )
    .unwrap();
    assert_eq!(off[0].line, vec![10]);
    spear_pockets(
        &mut state, &game, &plan, &op, &mut units, &roster, &enemies, &enemies, START,
    )
    .unwrap();
    assert!(
        units.iter().all(|u| u.line == vec![87] && !u.ring),
        "{units:?}"
    );
    // the neck taken: the arms hold the ring
    game.held.borrow_mut().push(87);
    let mut units2: Vec<Unit> = units.clone();
    spear_pockets(
        &mut state,
        &game,
        &plan,
        &op,
        &mut units2,
        &roster,
        &enemies,
        &enemies,
        START + DAY,
    )
    .unwrap();
    assert!(units2.iter().all(|u| u.ring));
}

#[test]
fn a_pocket_arm_goes_to_the_neck_its_spearhead_had_given_up_and_keeps_it_past_the_strike_days() {
    // w92c (28 Nov 2026, Holland): Army 7 left state 35 to the infantry (10 days on its
    // spearhead) 51 hours before the pocket "Army 9 and Army 7 meet in state 35 to cut off [36]"
    // (Friesland); Army 7's line [35] was filtered out by that skip, it went on elsewhere, and the
    // neck was given up after 6 days with one arm
    let game = Game::new();
    for (name, main) in [("Army 9", 67), ("Army 7", 66)] {
        game.roster.borrow_mut()["armies"]
            .as_array_mut()
            .unwrap()
            .push(json!({"army": name, "division_count": 8, "main_state": main}));
    }
    *game.pockets.borrow_mut() = json!({"pockets": [
        {"already_cut_off": false, "pincer_ready": true, "capital_inside": false,
         "neck_states": [{"state_id": 35}], "pocket_states": [{"state_id": 36}]}]});
    let mut state = State::default();
    for (s, p) in [
        (35, (10.0, 0.0)),
        (36, (10.0, 5.0)),
        (67, (12.0, 0.0)),
        (66, (8.0, 0.0)),
    ] {
        state.centers.insert(s, p);
    }
    state
        .done
        .insert(key("op", "spearskip:Army 7:35"), START - 51);
    let f = json!({"enemy": "POL", "own_states": [67]});
    let o = json!({"target_line": [7]});
    let mut units = vec![
        unit("Army 9".into(), Role::Mobile, &f, &o),
        unit("Army 7".into(), Role::Mobile, &f, &o),
    ];
    for u in units.iter_mut() {
        u.spear = true;
    }
    let plan = json!({"war": {"me": "GER"}});
    let roster = game.roster.borrow().clone();
    let enemies = vec!["POL".to_string()];
    let op = json!({"id": "op", "spear_pockets": true});
    spear_pockets(
        &mut state, &game, &plan, &op, &mut units, &roster, &enemies, &enemies, START,
    )
    .unwrap();
    assert!(state.op_notes["pocket"].contains("meet in state 35"));
    // both arms are painted to the neck, the one that had given it up included, and they keep it
    // past STRIKE_MAX_DAYS (the pocket's own limit decides)
    for t in [START, START + 11 * DAY] {
        for u in &units {
            let line = spear_line(&mut state, "op", &roster, u, "GER", &game, &[7], t);
            assert_eq!(line, vec![35], "{} at {t}", u.name);
        }
    }
    assert!(!state.done.contains_key(&key("op", "spearskip:Army 9:35")));
    // the pocket over: the old skip stands again for Army 7
    state.done.retain(|k, _| !k.starts_with("pocket:"));
    let line = spear_line(
        &mut state,
        "op",
        &roster,
        &units[1],
        "GER",
        &game,
        &[7],
        START + 12 * DAY,
    );
    assert_ne!(line, vec![35]);
}

#[test]
fn the_axis_by_terrain_is_chosen_once_round_the_forest_behind_the_river() {
    let game = Game::new();
    *game.pol_rows.borrow_mut() = Some(json!([
        {"state_id": 200, "controller": "POL", "buildings": {"infrastructure": 3}},
        {"state_id": 201, "controller": "POL", "buildings": {"infrastructure": 3}},
        {"state_id": 300, "controller": "POL", "buildings": {"infrastructure": 3},
         "victory_points_total": 20}]));
    *game.graph.borrow_mut() = Some(json!({"provinces": [
        graph_row(1, 67, "plains", &[(11, -0.6), (12, 0.0)]),
        graph_row(11, 200, "forest", &[(1, -0.6), (21, 0.0)]),
        graph_row(21, 300, "forest", &[(11, 0.0)]),
        graph_row(12, 200, "plains", &[(1, 0.0), (22, 0.0)]),
        graph_row(22, 201, "plains", &[(12, 0.0), (23, 0.0)]),
        graph_row(23, 300, "plains", &[(22, 0.0)])]}));
    let op = json!({"id": "op", "enemies": ["POL"], "formations": [
        {"army": "Panzer", "role": "mobile", "drive": "strike",
         "front": {"enemy": "POL", "own_states": [67]},
         "offensive": {"axis_by": "terrain", "axis": [67, 200, 300], "target_line": [300]}}]});
    let plan = json!({"war": {"me": "GER"}, "operations": [op.clone()]});
    let wars = json!({"borders": []});
    let mut state = State::default();
    plan_axes(&mut state, &game, &plan, "op", &op, &wars, START);
    let f = &op["formations"][0];
    assert_eq!(stored_axis(&state, "op", f, &op), Some(vec![200, 201, 300]));
    assert_eq!(
        stored_axis_provinces(&state, "op", f),
        vec![(12, 200), (22, 201), (23, 300)]
    );
    assert!(state.op_notes["op:axis:Panzer"].contains("[200, 201, 300]"));
    // stated once: a later read does not choose again
    game.calls.borrow_mut().clear();
    plan_axes(&mut state, &game, &plan, "op", &op, &wars, START + 2 * DAY);
    assert!(game.calls.borrow().is_empty());
    // the spearhead takes it as its line, with the provinces for the commands
    let roster = game.roster.borrow().clone();
    let (units, _) = units(&state, "op", &op, &roster);
    let pz = units.iter().find(|u| u.name == "Panzer").unwrap();
    assert!(pz.spear);
    assert_eq!(pz.line, vec![200, 201, 300]);
    assert_eq!(command_provinces(&game, pz, &[201, 300]), vec![22, 23]);
}

#[test]
fn the_opening_sends_an_echelon_to_a_small_enemy_short_of_force_or_past_its_deadline() {
    let game = Game::new();
    game.unit("Nord", |r| {
        r["division_count"] = json!(3);
        r["division_limit"] = json!(24);
    });
    game.roster.borrow_mut()["armies"]
        .as_array_mut()
        .unwrap()
        .push(json!({"army": "Army 9", "army_group": null, "plan": [], "division_count": 4}));
    let f = json!({"enemy": "LUX", "own_states": [42]});
    let units = vec![unit(
        "Nord".into(),
        Role::Wing,
        &f,
        &json!({"target_line": [8]}),
    )];
    let wars = json!({"borders": [{"neighbour_tag": "LUX", "neighbour_states": [8],
        "hostile_divisions_in_border_states": 3}]});
    let op = json!({"id": "op", "opening": {"deadline_days": {"LUX": 4}, "ratio": 2}});
    let at_war = vec!["LUX".to_string()];
    let roster = game.roster.borrow().clone();
    let mut state = State::default();
    // no echelon yet: noted, nothing sent
    assert!(
        opening(&mut state, &game, "op", &op, &units, &roster, &wars, &at_war, "l", START)
            .unwrap()
            .is_none()
    );
    assert!(state.op_notes["op:opening:LUX"].contains("no second echelon"));
    state
        .layout
        .insert("echelon:op:Mitte".into(), "Army 9".into());
    let line = opening(
        &mut state, &game, "op", &op, &units, &roster, &wars, &at_war, "l", START,
    )
    .unwrap()
    .expect("sent");
    assert!(line.contains("Army 9 joins Nord"), "{line}");
    let merge = game
        .orders()
        .into_iter()
        .find(|(n, _)| n == "hoi4_army_merge")
        .unwrap();
    assert_eq!(merge.1, json!({"source": "Army 9", "target": "Nord"}));
    // strong enough and inside the deadline: nothing
    let wars =
        json!({"borders": [{"neighbour_tag": "LUX", "hostile_divisions_in_border_states": 1}]});
    let mut state = State::default();
    state
        .layout
        .insert("echelon:op:Mitte".into(), "Army 9".into());
    assert!(
        opening(&mut state, &game, "op", &op, &units, &roster, &wars, &at_war, "l", START)
            .unwrap()
            .is_none()
    );
    // past the deadline with the enemy still in the war: sent
    assert!(opening(
        &mut state,
        &game,
        "op",
        &op,
        &units,
        &roster,
        &wars,
        &at_war,
        "l",
        START + 5 * DAY
    )
    .unwrap()
    .is_some());
}

/// The supply-only stack read (no width caps).
fn stack_read(read: &Value, line: &HashSet<String>, pick: &HashSet<String>, max: i64) -> StackRead {
    stack_read_caps(read, line, pick, max, &HashMap::new(), false)
}

// ---- engineer 91 (t1939n, the West): commitment, arrows that execute, the echelon's general ----

#[test]
fn the_groups_other_armies_are_owned_and_fight_on_its_front() {
    // t1939n: Armee Süd stood in Army Group 2 on an area defence the whole West war
    let game = Game::new();
    game.roster.borrow_mut()["armies"]
        .as_array_mut()
        .unwrap()
        .push(
            json!({"army": "Extra", "army_group": "Army Group 1", "commander": "x",
                     "plan": [{"kind": "area_defense", "executing": false, "offensives": []}],
                     "defensive_orders": 1, "exercising": false, "division_count": 15}),
        );
    let plan = plan();
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    assert_eq!(
        state.op_owned.get("Extra").map(String::as_str),
        Some("op_pol")
    );
    assert!(
        game.orders()
            .iter()
            .any(|(n, a)| n == "hoi4_army_plan"
                && a == &json!({"army": "Extra", "clear_orders": true})),
        "{:?}",
        game.orders()
    );
    // at war it is given an order of its own again: cleared once, so the group front carries it
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    assert!(state.done.contains_key("op:op_pol:exec"));
    game.unit(
        "Extra",
        |r| r["plan"] = json!([{"kind": "area_defense", "executing": false, "offensives": []}]),
    );
    game.calls.borrow_mut().clear();
    burst(&mut state, &game, &plan, START + 2 * DAY);
    let clears = |game: &Game| {
        game.orders()
            .iter()
            .filter(|(n, a)| {
                n == "hoi4_army_plan" && a == &json!({"army": "Extra", "clear_orders": true})
            })
            .count()
    };
    assert_eq!(clears(&game), 1);
    game.unit(
        "Extra",
        |r| r["plan"] = json!([{"kind": "area_defense", "executing": false, "offensives": []}]),
    );
    game.calls.borrow_mut().clear();
    burst(&mut state, &game, &plan, START + 3 * DAY);
    assert_eq!(clears(&game), 0, "once at war, never a loop");
    // the units of the operation hold it as a line army; an army the operation names stays itself
    let roster = game.roster.borrow().clone();
    let (units, group) = units(&state, "op_pol", &plan["operations"][0], &roster);
    assert_eq!(group.as_deref(), Some("Army Group 1"));
    assert!(units
        .iter()
        .any(|u| u.name == "Extra" && u.role == Role::Line));
    assert_eq!(units.iter().filter(|u| u.name == "Panzer").count(), 1);
}

#[test]
fn at_war_an_arrow_that_holds_divisions_and_does_not_execute_is_executed() {
    // t1939n: Army Group 2's front read executing while its 37 divisions stood on arrows that did not
    let game = Game::new();
    let plan = plan();
    let mut state = State::default();
    prepared(&mut state, &game, &plan);
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    assert!(state.done.contains_key("op:op_pol:exec"));
    let stuck = || {
        json!([{"kind": "front", "executing": true, "offensives": [
            {"id": 5, "executing": true, "divisions": 0, "target": {"states": [85, 86, 10, 88]}},
            {"id": 7, "executing": false, "divisions": 30, "target": {"states": [977]}}]}])
    };
    game.unit("Army Group 1", |r| r["plan"] = stuck());
    game.calls.borrow_mut().clear();
    burst(&mut state, &game, &plan, START + 2 * DAY);
    let group: Vec<Value> = game
        .orders()
        .iter()
        .filter(|(n, a)| n == "hoi4_army_plan" && a["army"] == json!("Army Group 1"))
        .map(|(_, a)| a["execute"].clone())
        .collect();
    assert_eq!(group, [json!(false), json!(true)], "stop, then execute");
    // an attacker still standing a day on is named in the notes; executing again clears them
    let at_war = vec!["POL".to_string()];
    let u = unit(
        "Army Group 1".into(),
        Role::Group,
        &json!({"enemy": "POL"}),
        &Value::Null,
    );
    let mut st = State::default();
    note_not_executing(&mut st, "op", &u, true, &at_war, START);
    assert!(st.op_notes.is_empty(), "not before a day");
    note_not_executing(&mut st, "op", &u, true, &at_war, START + DAY + 1);
    assert!(st.op_notes["op:notexec:Army Group 1"].contains("NOT EXECUTING for 1.0 days"));
    note_not_executing(&mut st, "op", &u, false, &at_war, START + DAY + 2);
    assert!(st.op_notes.is_empty());
    assert!(!st.done.contains_key("op:op:notexec:Army Group 1"));
}

#[test]
fn at_war_a_stroke_the_game_draws_elsewhere_is_kept_and_executed_never_stacked() {
    // t1939n: four strokes at Gelderland came back over Antwerp, four arrows, none executing
    let game = Game::new();
    let plan = plan();
    let mut state = State::default();
    prepared(&mut state, &game, &plan);
    *game.enemies.borrow_mut() = vec!["POL"];
    burst(&mut state, &game, &plan, START + DAY);
    assert!(state.done.contains_key("op:op_pol:exec"));
    // the group's arrow came off (the front reached its line); every new stroke goes elsewhere
    game.unit("Army Group 1", |r| {
        r["plan"] = json!([{"kind": "front", "executing": true, "offensives": []}])
    });
    *game.diverge_to.borrow_mut() = Some(977);
    game.calls.borrow_mut().clear();
    for h in 0..12 {
        burst(&mut state, &game, &plan, START + 2 * DAY + h * 6);
    }
    let strokes = game
        .orders()
        .iter()
        .filter(|(n, a)| n == "hoi4_army_offensive" && a["army"] == json!("Army Group 1"))
        .count();
    assert_eq!(strokes, 1, "{:?}", game.orders());
    // the arrow it left executes
    let roster = game.roster.borrow().clone();
    let offs = offensives(&roster, "Army Group 1");
    assert_eq!(offs.len(), 1);
    assert_eq!(offs[0]["executing"], json!(true));
}

#[test]
fn orders_go_by_command_by_default_when_the_run_posts_both_battle_plan_families() {
    let none = json!({});
    let both = Some("set_game_speed,production, orders ,orders2,mio");
    assert!(orders_by_command(&none, both, Some("auto")));
    assert!(orders_by_command(&none, both, None));
    assert!(!orders_by_command(&none, both, Some("gui")));
    assert!(!orders_by_command(&none, Some("orders"), Some("auto")));
    assert!(!orders_by_command(&none, None, Some("auto")));
    assert!(!orders_by_command(
        &json!({"orders_by": "mouse"}),
        both,
        Some("auto")
    ));
    assert!(orders_by_command(
        &json!({"orders_by": "command"}),
        None,
        None
    ));
}

#[test]
fn the_opening_never_sends_an_echelon_back_into_the_army_the_width_thinned() {
    // t1939n: Army 3's six divisions went out for width and back in by the opening, six times
    let game = Game::new();
    game.unit("Nord", |r| {
        r["division_count"] = json!(14);
        r["division_limit"] = json!(24);
    });
    game.roster.borrow_mut()["armies"]
        .as_array_mut()
        .unwrap()
        .push(json!({"army": "Army 9", "army_group": null, "plan": [], "division_count": 6}));
    let f = json!({"enemy": "BEL", "own_states": [51]});
    let units = vec![unit(
        "Nord".into(),
        Role::Wing,
        &f,
        &json!({"target_line": [6]}),
    )];
    let wars = json!({"borders": [{"neighbour_tag": "BEL", "neighbour_states": [6],
        "hostile_divisions_in_border_states": 20}]});
    let op = json!({"id": "op", "opening": {"deadline_days": {"BEL": 16}, "ratio": 2}});
    let at_war = vec!["BEL".to_string()];
    let roster = game.roster.borrow().clone();
    let mut state = State::default();
    // the echelon came out of Nord itself
    state
        .layout
        .insert("echelon:op:Nord".into(), "Army 9".into());
    assert!(
        opening(&mut state, &game, "op", &op, &units, &roster, &wars, &at_war, "l", START)
            .unwrap()
            .is_none()
    );
    assert!(game.orders().is_empty(), "{:?}", game.orders());
    assert!(
        state.op_notes["op:opening:BEL"].contains("full for width"),
        "{:?}",
        state.op_notes
    );
    // an echelon of another army goes to it while it is not thinned
    let mut state = State::default();
    state
        .layout
        .insert("echelon:op:Mitte".into(), "Army 9".into());
    assert!(
        opening(&mut state, &game, "op", &op, &units, &roster, &wars, &at_war, "l", START)
            .unwrap()
            .is_some()
    );
    // but not once the width thinned it
    let mut state = State::default();
    state
        .layout
        .insert("echelon:op:Mitte".into(), "Army 9".into());
    state.done.insert("op:op:thinned:Nord".into(), START);
    game.calls.borrow_mut().clear();
    assert!(
        opening(&mut state, &game, "op", &op, &units, &roster, &wars, &at_war, "l", START)
            .unwrap()
            .is_none()
    );
    assert!(game.orders().is_empty());
}

#[test]
fn the_operations_execution_mode_is_sent_once_per_attacker_before_its_war() {
    let game = Game::new();
    let mut plan = plan();
    plan["operations"][0]["execution"] = json!("balanced");
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    let modes: Vec<String> = game
        .orders()
        .iter()
        .filter(|(n, a)| n == "hoi4_army_plan" && a["mode"] == json!("balanced"))
        .map(|(_, a)| text(&a["army"]).to_string())
        .collect();
    assert_eq!(modes.len(), 3, "{modes:?}");
    for a in ["Army Group 1", "Panzer", "Nord"] {
        assert!(modes.contains(&a.to_string()), "{modes:?}");
    }
    game.calls.borrow_mut().clear();
    burst(&mut state, &game, &plan, START + DAY);
    assert!(!game.orders().iter().any(|(_, a)| a["mode"].is_string()));
}

#[test]
fn a_second_echelon_without_a_general_gets_a_free_one_before_its_order() {
    // t1939n: "second echelon Army 1 holds its station 57: false no_commander"
    let game = Game::new();
    game.roster.borrow_mut()["armies"]
        .as_array_mut()
        .unwrap()
        .push(
            json!({"army": "Army 9", "army_group": null, "commander": null, "plan": [],
                     "exercising": false, "division_count": 6}),
        );
    *game.commanders.borrow_mut() = json!({"commanders": [
        {"name": "Busch", "id": "GER_busch", "assigned_to": "Mitte"},
        {"name": "Rundstedt", "id": "GER_r", "assigned_to": null, "rank": "field_marshal"},
        {"name": "Kluge", "id": "GER_k", "assigned_to": null}]});
    *game.supply.borrow_mut() = Some(supply_read(&[]));
    let plan = json!({"war": {"me": "GER"}});
    let op = json!({"id": "op", "stacking": {"by_width": false}});
    let units = vec![unit("Mitte".into(), Role::Line, &Value::Null, &Value::Null)];
    let wars = json!({"borders": []});
    let roster = game.roster.borrow().clone();
    let mut state = State::default();
    state.done.insert("op:op:exec".into(), START);
    state.done.insert("op:op:echmade:Army 9".into(), START);
    state
        .layout
        .insert("echelon:op:Mitte".into(), "Army 9".into());
    let line = stacking(
        &mut state, &game, &plan, "op", &op, &units, &roster, &wars, "l", START,
    )
    .unwrap()
    .expect("an order");
    assert!(
        line.contains("Kluge leads the second echelon Army 9"),
        "{line}"
    );
    assert_eq!(
        game.orders()[0],
        (
            "hoi4_army_commander".to_string(),
            json!({"army": "Army 9", "general": "Kluge"})
        )
    );
}

#[test]
fn an_advance_that_outruns_its_supply_waits_for_it_and_goes_on_when_it_is_back() {
    // t1939n (27 Nov 2026): 45 divisions in Dutch Brabant on local supply 1-2 of a demand of 8-9
    // pressed on and took nothing for ten days
    let game = Game::new();
    game.unit("Mitte", |r| {
        r["plan_executing"] = json!(true);
        r["motorization"] = json!(2);
    });
    let read = |pct: f64| {
        json!({"divisions": [
            {"id": "m1", "army": "Mitte", "province_id": 6496, "state_id": 35, "supply_percent": pct},
            {"id": "m2", "army": "Mitte", "province_id": 6496, "state_id": 35, "supply_percent": pct}]})
    };
    *game.supply.borrow_mut() = Some(read(20.0));
    let roster = game.roster.borrow().clone();
    let plan = json!({"war": {"relief_after_days": 2}});
    let op = json!({"id": "op_x", "supply": {"floor": 35}});
    let mut u = unit("Mitte".into(), Role::Line, &Value::Null, &Value::Null);
    u.enemy = "HOL".into();
    let at_war = vec!["HOL".to_string()];
    let mut state = State::default();
    // below the plan's floor: the arrow stops, the front stays
    let got = supply_pace(
        &mut state, &game, &plan, "op_x", &op, &u, &roster, &at_war, "l", START,
    )
    .unwrap();
    assert!(
        matches!(&got, Paced::Acted(l) if l.contains("waits for its supply")),
        "{got:?}"
    );
    assert!(game
        .orders()
        .iter()
        .any(|(n, a)| n == "hoi4_army_plan" && a["execute"] == json!(false)));
    // while it waits: held (the relief spell starts), and after the relief delay a railway goes
    // toward the starving province from the nearest own node
    assert_eq!(
        supply_pace(
            &mut state,
            &game,
            &plan,
            "op_x",
            &op,
            &u,
            &roster,
            &at_war,
            "l",
            START + DAY / 2
        )
        .unwrap(),
        Paced::Held
    );
    let later = START + 3 * DAY;
    let got = supply_pace(
        &mut state, &game, &plan, "op_x", &op, &u, &roster, &at_war, "l", later,
    )
    .unwrap();
    assert!(
        matches!(&got, Paced::Acted(l) if l.contains("railway")),
        "{got:?}"
    );
    assert!(game.orders().iter().any(|(n, a)| n == "hoi4_railway_build"
        && a["to_province"] == json!(6496)
        && a["from_nearest_node"] == json!(true)));
    assert_eq!(
        supply_pace(&mut state, &game, &plan, "op_x", &op, &u, &roster, &at_war, "l", later)
            .unwrap(),
        Paced::Held
    );
    // the supply back at the resume level: it goes on (the REPAIR executes it again)
    *game.supply.borrow_mut() = Some(read(80.0));
    let back = later + DAY;
    assert_eq!(
        supply_pace(&mut state, &game, &plan, "op_x", &op, &u, &roster, &at_war, "l", back)
            .unwrap(),
        Paced::Go
    );
    assert!(!state.done.contains_key("op:op_x:supplyheld:Mitte"));
    // `supply.pace: false` keeps the rule off
    *game.supply.borrow_mut() = Some(read(10.0));
    let off = json!({"id": "op_x", "supply": {"floor": 35, "pace": false}});
    assert_eq!(
        supply_pace(
            &mut State::default(),
            &game,
            &plan,
            "op_x",
            &off,
            &u,
            &roster,
            &at_war,
            "l",
            back
        )
        .unwrap(),
        Paced::Go
    );
}

/// A plain read-only theatre for the start and hand-over rules: hoi4_wars and hoi4_armies as given.
struct Reads {
    wars: Value,
    armies: Value,
}
impl Tools for Reads {
    fn call(&self, name: &str, _args: &Value) -> Result<Value> {
        Ok(match name {
            "hoi4_wars" => self.wars.clone(),
            "hoi4_armies" => self.armies.clone(),
            _ => json!({}),
        })
    }
}

fn west() -> (Value, Reads) {
    let plan = json!({"operations": [
        {"id": "op_low", "enemies": ["HOL", "BEL"], "prepare_from": "", "group": "ag",
         "on_capitulation": {"next": "op_fra"},
         "formations": [{"army": "Mitte", "role": "line"},
                        {"army": "PzN", "role": "mobile", "front": {"enemy": "HOL", "own_states": [56]}},
                        {"army": "Wing", "role": "wing", "front": {"enemy": "BEL", "own_states": [51]}}]},
        {"id": "op_fra", "enemies": ["FRA"], "prepare_from": "capitulated:BEL",
         "execute_on": {"all": ["at_war_with:FRA", "capitulated:BEL"]},
         "formations": [{"army": "Mitte", "role": "line"},
                        {"army": "PzN", "role": "mobile", "front": {"enemy": "FRA", "own_states": [42]}}]}]});
    let reads = Reads {
        wars: json!({"wars": {"enemies": ["HOL", "BEL", "FRA"]},
            "borders": [{"neighbour_tag": "FRA", "neighbour_states": [28], "own_border_states": [42, 978]},
                        {"neighbour_tag": "BEL", "neighbour_states": [980], "own_border_states": [51]}]}),
        armies: json!({"armies": [
            {"army": "Mitte", "army_group": "AG", "division_count": 30, "plan": []},
            {"army": "PzN", "army_group": null, "division_count": 8, "plan": []},
            {"army": "Wing", "army_group": null, "division_count": 12, "plan": []}],
            "army_groups": [{"army_group": "AG", "plan": []}]}),
    };
    (plan, reads)
}

#[test]
fn france_at_war_starts_with_the_force_holland_frees_not_on_belgiums_capitulation() {
    // t1939n (27 Nov 2026): op_france waited on capitulated:BEL while the Belgian war stalled;
    // France had been at war since 1939 and its border stood open at 42 and 978
    let (plan, reads) = west();
    let ops = list(&plan, "operations").to_vec();
    let mut state = State::default();
    state.done.insert("op:op_low:prep".into(), START);
    // every army of op_fra is held by op_low: nothing to start with
    assert_eq!(free_start(&state, &reads, &ops, &ops[1], "op_fra"), None);
    // Holland is out: the armour that fought it goes on to op_fra (the group's enemy is out, so
    // the main weight no longer holds it); the Belgian wing and the line army stay
    let roster = reads.armies.clone();
    let (mut units, _) = units(&state, "op_low", &ops[0], &roster);
    for u in units.iter_mut() {
        if u.role == Role::Group || u.name == "PzN" || u.name == "Mitte" {
            u.enemy = "HOL".into();
        }
    }
    let at_war = vec!["BEL".to_string(), "FRA".to_string()];
    release_to_next(
        &mut state,
        &plan,
        "op_low",
        &ops[0],
        &mut units,
        &roster,
        &reads.wars,
        &["HOL".to_string()],
        &at_war,
        START + DAY,
    );
    assert!(released(&state, "op_low", "PzN"));
    assert!(!released(&state, "op_low", "Wing"));
    assert!(!released(&state, "op_low", "Mitte"));
    assert!(units.iter().all(|u| u.name != "PzN"));
    // op_low no longer owns it; op_fra starts with it, on its open border, and says its plan
    // trigger had not held
    assert!(!owned_names(&state, &plan, &ops[0], &roster).contains(&"PzN".to_string()));
    let why = free_start(&state, &reads, &ops, &ops[1], "op_fra").unwrap();
    assert!(
        why.contains("PzN") && why.contains("[42, 978]") && why.contains("had not held"),
        "{why}"
    );
    assert!(why.contains("Mitte (op_low)"), "{why}");
    // a plan that keeps the operation to its trigger
    let mut keep = ops[1].clone();
    keep["start"] = json!("trigger");
    assert_eq!(free_start(&state, &reads, &ops, &keep, "op_fra"), None);
}

// THE WEST (engineer 95): deep lines, executing orders stay, refinements rest

#[test]
fn by_command_a_plain_offensive_aims_at_the_far_edge_of_its_line_not_a_province_into_it() {
    // w92c (28 Nov 2026): every plain offensive was a stroke between two state centres that read
    // back 1-4 provinces in the first enemy state; 21-31 divisions piled on one province
    let game = Game::new();
    let mut plan = plan();
    plan["operations"][0]["orders_by"] = json!("command");
    let mut rows = vec![
        graph_row(660, 66, "plains", &[(670, 0.0), (850, 0.0)]),
        graph_row(670, 67, "plains", &[(660, 0.0), (860, 0.0)]),
        graph_row(850, 85, "plains", &[(660, 0.0), (851, 0.0), (860, 0.0)]),
        graph_row(851, 85, "forest", &[(850, 0.0), (861, 0.0), (500, 0.0)]),
        graph_row(860, 86, "plains", &[(670, 0.0), (850, 0.0), (861, 0.0)]),
        graph_row(861, 86, "plains", &[(860, 0.0), (851, 0.0), (100, 0.0)]),
        graph_row(100, 10, "plains", &[(861, 0.0), (101, 0.0)]),
        graph_row(101, 10, "plains", &[(100, 0.0), (880, 0.0)]),
        graph_row(880, 88, "plains", &[(101, 0.0), (881, 0.0), (600, 0.0)]),
        graph_row(881, 88, "plains", &[(880, 0.0)]),
    ];
    rows[9]["coastal"] = json!(true);
    *game.graph.borrow_mut() = Some(json!({"provinces": rows}));
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    let off = game
        .orders()
        .into_iter()
        .find(|(n, a)| n == "hoi4_army_offensive" && a["army"] == json!("Army Group 1"))
        .expect("the group's offensive");
    let far = ints(&off.1["target_provinces"]);
    // the far border and the coast, every state of the line kept, nothing on our border
    for p in [880, 881] {
        assert!(far.contains(&p), "{far:?}");
    }
    for p in [850, 860] {
        assert!(!far.contains(&p), "{far:?}");
    }
    assert!(
        far.contains(&101) && far.contains(&851) && far.contains(&861),
        "{far:?}"
    );
    // the stroke's states go with it (the mouse fallback and the read-back cover)
    assert!(off.1["states"].is_array() || off.1["state_id"].is_i64());
    // without a graph read the stroke is drawn as before
    let game = Game::new();
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    let off = game
        .orders()
        .into_iter()
        .find(|(n, a)| n == "hoi4_army_offensive" && a["army"] == json!("Army Group 1"))
        .expect("the group's offensive");
    assert!(off.1.get("target_provinces").is_none(), "{off:?}");
    // `deep_line: false` keeps the stroke
    let game = Game::new();
    *game.graph.borrow_mut() = Some(json!({"provinces": []}));
    let mut off_plan = plan.clone();
    off_plan["operations"][0]["deep_line"] = json!(false);
    let mut state = State::default();
    burst(&mut state, &game, &off_plan, START);
    assert!(!game
        .orders()
        .iter()
        .any(|(_, a)| a.get("target_provinces").is_some()));
}

#[test]
fn a_planning_gate_that_passed_leaves_no_note_behind() {
    // w92c: "planning 0.15 against 0.27" stood from W-142 to the end of the war
    let game = Game::new();
    let plan = plan();
    let mut state = State::default();
    state.op_notes.insert(
        "op_pol:planning".into(),
        "planning 0.15 against 0.27".into(),
    );
    burst(&mut state, &game, &plan, START);
    assert!(!state.op_notes.contains_key("op_pol:planning"));
}

#[test]
fn a_refinement_rests_per_army_for_its_days() {
    let mut state = State::default();
    let op = json!({});
    assert!(refine_due(&state, "o", &op, "Mitte", START));
    refined(&mut state, "o", "Mitte", START);
    assert!(!refine_due(&state, "o", &op, "Mitte", START + 4 * DAY));
    assert!(refine_due(&state, "o", &op, "Sued", START + DAY));
    assert!(refine_due(
        &state,
        "o",
        &op,
        "Mitte",
        START + REFINE_DAYS * DAY
    ));
    let quick = json!({"refine_days": 0});
    assert!(refine_due(&state, "o", &quick, "Mitte", START));
}

#[test]
fn at_war_an_overstacked_line_is_not_thinned_by_default() {
    // w92c: 13 detach and merge orders in 28 days, each division arriving with planning 0
    let game = Game::new();
    let a = ids("a", 20);
    game.unit("Mitte", |r| {
        r["plan"] = json!([]);
        r["division_count"] = json!(20);
        r["states"] = json!({"980": 20});
    });
    let divs: Vec<(&str, &str, f64)> = a.iter().map(|d| (d.as_str(), "Mitte", 50.0)).collect();
    *game.supply.borrow_mut() = Some(supply_read(&[(9444, 980, 4.0, 20.0, divs)]));
    let op = json!({"id": "op_low", "enemies": ["BEL"]});
    let units = vec![unit("Mitte".into(), Role::Line, &Value::Null, &Value::Null)];
    let wars = json!({"wars": {"enemies": ["BEL"]},
        "borders": [{"neighbour_tag": "BEL", "neighbour_states": [6],
                     "own_border_states": [980, 51, 36], "uncovered_own_border_states": [51]}]});
    let plan = json!({"war": {"me": "GER"}});
    let mut state = State::default();
    state.done.insert(key("op_low", "exec"), START - DAY);
    let roster = game.roster.borrow().clone();
    for t in [START, START + DAY / 2, START + DAY] {
        assert!(
            stacking(&mut state, &game, &plan, "op_low", &op, &units, &roster, &wars, "l", t)
                .unwrap()
                .is_none()
        );
    }
    assert!(!game.orders().iter().any(|(n, _)| n == "hoi4_army_detach"));
    assert!(state.op_notes["op_low:stacking"].contains("no thinning at war"));
}

#[test]
fn by_default_a_wing_attacks_with_the_joint_attack_and_never_alone() {
    let game = Game::new();
    *game.graph.borrow_mut() = Some(lux_graph());
    game.unit("Nord", |r| r["division_count"] = json!(7));
    game.unit("Mitte", |r| r["plan_executing"] = json!(false));
    let plan = json!({"war": {"me": "GER"}, "operations": [
        {"id": "op_low", "war": "w", "enemies": ["BEL"]},
        {"id": "op_lux", "war": "w", "enemies": ["LUX"]}]});
    let op = plan["operations"][1].clone();
    let mut u = unit("Nord".into(), Role::Wing, &Value::Null, &Value::Null);
    u.enemy = "LUX".into();
    u.line = vec![8];
    // fifteen French divisions seen at the border: the count is no reason to wait
    let wars = json!({"wars": {"enemies": ["LUX", "FRA"]}, "borders": [
        {"neighbour_tag": "LUX", "neighbour_states": [8], "own_border_states": [42],
         "neighbour_divisions_in_border_states": 1, "hostile_divisions_in_border_states": 15}]});
    let mut state = State::default();
    state.op_owned.insert("Mitte".into(), "op_low".into());
    let roster = game.roster.borrow().clone();
    // no attack of the war executes: never alone
    assert!(assert_guard(
        &mut state, &game, &plan, &op, &u, &roster, &wars, START
    ));
    // the group's attack executes: the wing goes with it at once
    game.unit("Mitte", |r| r["plan_executing"] = json!(true));
    let roster = game.roster.borrow().clone();
    assert!(!assert_guard(
        &mut state,
        &game,
        &plan,
        &op,
        &u,
        &roster,
        &wars,
        START + DAY
    ));
}

// THE ORDER STRUCTURE (engineer 95, after ctl92a): one root per enemy border, divisions spread

#[test]
fn by_command_a_group_front_takes_every_border_it_names_in_one_call() {
    let game = Game::new();
    let mut plan = plan();
    plan["operations"][0]["orders_by"] = json!("command");
    plan["operations"][0]["front"]["enemies"] = json!(["POL", "LIT"]);
    let mut state = State::default();
    burst(&mut state, &game, &plan, START);
    let front = game
        .orders()
        .into_iter()
        .find(|(n, a)| n == "hoi4_army_front" && a["army"] == json!("Army Group 1"))
        .expect("the group's front");
    assert_eq!(front.1["against"], json!(["POL", "LIT"]));
    assert_eq!(front.1["own_states"], json!([66, 67]));
}

fn two_fronts(game: &Game, holland: i64, belgium: i64) {
    game.unit("Army Group 1", |r| {
        r["plan"] = json!([
            {"kind": "front", "id": 109, "executing": false, "divisions": holland,
             "line": {"states": [57, 56]}, "offensives": []},
            {"kind": "front", "id": 110, "executing": false, "divisions": belgium,
             "line": {"states": [42]}, "offensives": []}]);
    });
}

#[test]
fn a_group_with_two_fronts_spreads_its_divisions_and_each_border_gets_its_own_deep_attack() {
    // ctl92a (28 Nov 2026): Holland's front 0 divisions, Belgium's 68
    let game = Game::new();
    two_fronts(&game, 0, 68);
    let wars = json!({"wars": {"enemies": []}, "borders": [
        {"neighbour_tag": "HOL", "neighbour_states": [7], "own_border_states": [57, 56]},
        {"neighbour_tag": "BEL", "neighbour_states": [6], "own_border_states": [42]}]});
    let mut grp = unit(
        "Army Group 1".into(),
        Role::Group,
        &json!({"enemy": "HOL", "enemies": ["HOL", "BEL"], "own_states": [57, 56, 42]}),
        &json!({"target_line": [7]}),
    );
    grp.members = vec!["Mitte".into()];
    assert_eq!(grp.enemies, vec!["BEL".to_string()]);
    let units = vec![grp];
    let mut state = State::default();
    state.done.insert(key("op", "by_command"), 1);
    state.done.insert(key("op", "deep_line"), 1);
    let roster = game.roster.borrow().clone();
    let line = border_fronts(
        &mut state,
        &game,
        "op",
        &json!({}),
        &units,
        &roster,
        &wars,
        "l",
        START,
    )
    .unwrap()
    .expect("a spread");
    assert!(
        line.contains("spread over its 2 fronts") && line.contains("stands empty"),
        "{line}"
    );
    assert!(game.orders().iter().any(|(n, a)| n == "hoi4_army_plan"
        && a["army"] == json!("Army Group 1")
        && a["distribute"] == json!(true)));
    // the Belgian root is a border front, the Dutch one the group's own line
    assert!(border_front(
        &state,
        "op",
        "Army Group 1",
        &json!({"id": 110})
    ));
    assert!(!border_front(
        &state,
        "op",
        "Army Group 1",
        &json!({"id": 109})
    ));
    // spread: no second spread the same day, and none while both fronts hold divisions
    two_fronts(&game, 30, 38);
    game.calls.borrow_mut().clear();
    *game.graph.borrow_mut() = Some(json!({"provinces": [
        graph_row(420, 42, "plains", &[(60, 0.0)]),
        graph_row(60, 6, "forest", &[(420, 0.0), (61, 0.0)]),
        graph_row(61, 6, "plains", &[(60, 0.0), (900, 0.0)])]}));
    let roster = game.roster.borrow().clone();
    let line = border_fronts(
        &mut state,
        &game,
        "op",
        &json!({}),
        &units,
        &roster,
        &wars,
        "l",
        START + DAY,
    )
    .unwrap()
    .expect("the Belgian front's attack");
    assert!(line.contains("against BEL from its front 110"), "{line}");
    let off = game
        .orders()
        .into_iter()
        .find(|(n, _)| n == "hoi4_army_offensive")
        .unwrap();
    assert_eq!(off.1["parent_order"], json!(110));
    assert_eq!(ints(&off.1["target_provinces"]), vec![61]);
    assert!(!game
        .orders()
        .iter()
        .any(|(_, a)| a["distribute"].is_boolean()));
    // the group's own line offensive never counts the border front's arrow
    game.unit("Army Group 1", |r| {
        r["plan"][1]["offensives"] = json!([{"id": 111, "target": {"states": [6]}}]);
    });
    let roster = game.roster.borrow().clone();
    assert!(!line_offensives(&state, "op", &roster, "Army Group 1")
        .iter()
        .any(|o| o["id"] == json!(111)));
    assert!(offensives(&roster, "Army Group 1")
        .iter()
        .any(|o| o["id"] == json!(111)));
}

/// A battle as hoi4_battles {divisions} lists it: province, our side, our and the enemy side's
/// hours until broken, our engaged division ids, the enemy's tags.
fn battle_row(p: i64, us: f64, them: f64, ids: &[&str], tags: &[&str]) -> Value {
    json!({"province": p, "our_side": "attacker",
           "attacker": {"hours_until_broken": us, "reserve_divisions": [],
                        "engaged_divisions": ids.iter().map(|i| json!({"id": i})).collect::<Vec<_>>()},
           "defender": {"hours_until_broken": them, "tags": tags}})
}

/// hoi4_war_stats with the player's losses and the enemy's in one relation per enemy.
fn war_stats(rows: &[(&str, f64, f64)]) -> Value {
    let relations: Vec<Value> = rows
        .iter()
        .map(|(t, own, enemy)| {
            json!({"countries": [{"tag": "GER", "casualties": own}, {"tag": t, "casualties": enemy}]})
        })
        .collect();
    json!({"player": {"tag": "GER"}, "wars": [{"includes_player": true, "relations": relations}]})
}

#[test]
fn at_war_the_mode_follows_the_battles_and_the_losses_of_the_attack() {
    // o96a (28 Nov 2026, the Low Countries by hand): balanced from W to W+21 took 7 of 91 Dutch
    // points at 0.92 enemy losses to 1 own over W+10..W+18; rush took Holland in 6 days
    let game = Game::new();
    game.unit("Nord", |r| r["plan_executing"] = json!(true));
    *game.supply.borrow_mut() = Some(json!({"divisions": [
        {"id": "n1", "army": "Nord"}, {"id": "n2", "army": "Nord"}, {"id": "x1", "army": "Other"}]}));
    // W+1: the Ardennes 3488 and 11562 are won (the defender breaks first), Luxembourg 6583 lost
    *game.battles.borrow_mut() = Some(json!({"list": [
        battle_row(3488, 786.0, 139.0, &["n1"], &["BEL"]),
        battle_row(11562, 2120.0, 90.0, &["n2"], &["BEL", "HOL"]),
        battle_row(6583, 549.0, 1113.0, &["n2"], &["FRA", "LUX"]),
        battle_row(1, 10.0, 900.0, &["x1"], &["POL"])]}));
    let mut u = unit("Nord".into(), Role::Wing, &Value::Null, &Value::Null);
    u.enemy = "BEL".into();
    let units = vec![u];
    let at_war = vec!["BEL".to_string(), "HOL".to_string(), "LUX".to_string()];
    let op = json!({"id": "op_low", "enemies": ["BEL", "HOL", "LUX"]});
    let roster = game.roster.borrow().clone();
    let mut state = State::default();
    let run = |state: &mut State, op: &Value, t: GameHour| {
        war_modes(state, &game, "op_low", op, &units, &roster, &at_war, "l", t).unwrap()
    };
    // too few losses to judge, 2 of 3 battles won: the attack presses at once
    assert!(run(&mut state, &op, START).is_some());
    let modes = |game: &Game| -> Vec<String> {
        game.orders()
            .iter()
            .filter(|(n, a)| n == "hoi4_army_plan" && text(&a["army"]) == "Nord")
            .map(|(_, a)| text(&a["mode"]).to_string())
            .collect()
    };
    assert_eq!(modes(&game), vec!["rush"]);
    assert!(state.op_notes["op_low:mode:Nord"].contains("2 of 3 battles won"));
    // read again only after its two days; the same mode is never sent twice
    assert!(run(&mut state, &op, START + DAY).is_none());
    // W+10.5: the war's losses 22699 enemy to 6198 own, most battles lost: rush stays
    *game.war_stats.borrow_mut() = Some(war_stats(&[
        ("HOL", 1152.0, 10719.0),
        ("BEL", 3078.0, 11754.0),
        ("LUX", 1968.0, 226.0),
    ]));
    *game.battles.borrow_mut() = Some(json!({"list": [
        battle_row(13375, 508.0, 526.0, &["n1"], &["BEL", "FRA"]),
        battle_row(391, 234.0, 1248.0, &["n2"], &["BEL", "HOL"])]}));
    assert!(run(&mut state, &op, START + 2 * DAY).is_none());
    assert_eq!(modes(&game), vec!["rush"]);
    assert!(state.op_notes["op_low:mode:Nord"].contains("losses 3.66 enemy to 1 own"));
    assert!(winning(&state, "op_low", &op, "Nord", START + 2 * DAY));
    // W+18.5: 13428 more own losses against 12347: our losses outrun theirs, balanced
    *game.war_stats.borrow_mut() = Some(war_stats(&[
        ("HOL", 3793.0, 15974.0),
        ("BEL", 11692.0, 18846.0),
        ("LUX", 4141.0, 226.0),
    ]));
    assert!(run(&mut state, &op, START + 4 * DAY).is_some());
    assert_eq!(modes(&game), vec!["rush", "balanced"]);
    assert!(state.op_notes["op_low:mode:Nord"].contains("over the last days"));
    assert!(!winning(&state, "op_low", &op, "Nord", START + 4 * DAY));
    // a plan that names its mode keeps it; no battle read, nothing judged
    let fixed = json!({"id": "op_low", "execution": "rush"});
    assert!(run(&mut state, &fixed, START + 10 * DAY).is_none());
    *game.battles.borrow_mut() = None;
    assert!(run(&mut state, &op, START + 10 * DAY).is_none());
    assert_eq!(modes(&game).len(), 2);
}

#[test]
fn a_starving_attack_that_wins_on_its_losses_is_not_stopped_for_its_supply() {
    let game = Game::new();
    game.unit("Nord", |r| r["plan_executing"] = json!(true));
    *game.supply.borrow_mut() = Some(json!({"divisions": [
        {"id": "n1", "army": "Nord", "supply_percent": 20.0},
        {"id": "n2", "army": "Nord", "supply_percent": 24.0}]}));
    let mut u = unit("Nord".into(), Role::Wing, &Value::Null, &Value::Null);
    u.enemy = "BEL".into();
    let at_war = vec!["BEL".to_string()];
    let op = json!({"id": "op_low", "enemies": ["BEL"], "supply": {"floor": 35}});
    let plan = json!({"war": {"me": "GER"}});
    let roster = game.roster.borrow().clone();
    let stops = |game: &Game| {
        game.orders()
            .iter()
            .filter(|(n, a)| n == "hoi4_army_plan" && a["execute"] == json!(false))
            .count()
    };
    // the mode rule read a winning attack: it goes on, its supply note says why
    let mut state = State::default();
    state.done.insert(key("op_low", "modewin:Nord"), START);
    let paced = supply_pace(
        &mut state, &game, &plan, "op_low", &op, &u, &roster, &at_war, "l", START,
    )
    .unwrap();
    assert_ne!(paced, Paced::Held);
    assert_eq!(stops(&game), 0);
    assert!(state.op_notes["op_low:supply:Nord"].contains("the attack wins on its losses"));
    // no such reading (or a stale one): the advance waits for its supply as before
    let mut state = State::default();
    state
        .done
        .insert(key("op_low", "modewin:Nord"), START - 30 * DAY);
    let paced = supply_pace(
        &mut state, &game, &plan, "op_low", &op, &u, &roster, &at_war, "l", START,
    )
    .unwrap();
    assert!(matches!(paced, Paced::Acted(_)), "{paced:?}");
    assert_eq!(stops(&game), 1);
}

// CAPACITY (engineer 99, after o96a): fronts weighted by what their stretch can fight with and
// feed; the reserve of a full battle moves sideways to a front with room

fn capacity_theatre(game: &Game, holland: i64, belgium: i64) {
    game.unit("Army Group 1", |r| {
        r["plan"] = json!([
            {"kind": "front", "id": 109, "executing": true, "divisions": holland,
             "line": {"states": [57], "ids": [571, 572]}, "offensives": []},
            {"kind": "front", "id": 110, "executing": true, "divisions": belgium,
             "line": {"states": [42], "ids": [421]}, "offensives": []}]);
    });
    // the Dutch plains (71 from two own provinces, 72 from one), the Ardennes forest (61)
    *game.graph.borrow_mut() = Some(json!({"provinces": [
        graph_row(571, 57, "plains", &[(71, 0.0), (572, 0.0)]),
        graph_row(572, 57, "plains", &[(71, 0.0), (72, 0.0), (571, 0.0)]),
        graph_row(421, 42, "plains", &[(61, 0.0)]),
        graph_row(71, 7, "plains", &[(571, 0.0), (572, 0.0)]),
        graph_row(72, 7, "plains", &[(572, 0.0)]),
        graph_row(61, 6, "forest", &[(421, 0.0)])]}));
    // o96a W+10: 26 divisions in the forest province on 1.66 supply against 21.3
    let mut divisions: Vec<Value> = (0..26)
        .map(|i| json!({"id": format!("m{i}"), "army": "Mitte", "province_id": 421}))
        .collect();
    divisions
        .extend((0..4).map(|i| json!({"id": format!("s{i}"), "army": "Sued", "province_id": 571})));
    *game.supply.borrow_mut() = Some(json!({"divisions": divisions, "provinces": [
        {"province_id": 421, "divisions": 26, "supply": 1.66, "demand": 21.3},
        {"province_id": 571, "divisions": 4, "supply": 4.6, "demand": 3.3}]}));
}

#[test]
fn fronts_are_weighted_by_what_their_stretch_fights_with_and_feeds_and_a_full_battle_s_reserve_goes_sideways(
) {
    let game = Game::new();
    capacity_theatre(&game, 0, 68);
    let wars = json!({"wars": {"enemies": []}, "borders": [
        {"neighbour_tag": "HOL", "neighbour_states": [7], "own_border_states": [57]},
        {"neighbour_tag": "BEL", "neighbour_states": [6], "own_border_states": [42]}]});
    let mut grp = unit(
        "Army Group 1".into(),
        Role::Group,
        &json!({"enemy": "HOL", "enemies": ["HOL", "BEL"], "own_states": [57, 42]}),
        &json!({"target_line": [7]}),
    );
    grp.members = vec!["Mitte".into(), "Sued".into()];
    let units = vec![grp];
    let op = json!({"id": "op", "enemies": ["HOL", "BEL"], "refine_days": 0});
    let mut state = State::default();
    state.done.insert(key("op", "by_command"), 1);
    state.done.insert(key("op", "deep_line"), 1);
    let run = |state: &mut State, t: GameHour| {
        let roster = game.roster.borrow().clone();
        border_fronts(state, &game, "op", &op, &units, &roster, &wars, "l", t).unwrap()
    };
    // 1. the spread: the Dutch plains take 10 (6 from two sides into 71, 4 into 72), the forest
    // from one side 3 (and 4 fed at half supply): weights, not lengths
    let line = run(&mut state, START).expect("a spread");
    assert!(line.contains("by capacity"), "{line}");
    let spread = game
        .orders()
        .into_iter()
        .find(|(n, a)| n == "hoi4_army_plan" && a["distribute"].is_object())
        .expect("a spread by weights");
    assert_eq!(
        spread.1["distribute"]["weights"],
        json!({"109": 10, "110": 3})
    );
    // 2. in the prepare a spread far from the rooms is spread again by them
    capacity_theatre(&game, 1, 44);
    game.calls.borrow_mut().clear();
    let line = run(&mut state, START + DAY).expect("a spread again");
    assert!(line.contains("spread again by capacity"), "{line}");
    // 3. at war: the forest battle holds 9 in reserve behind 3 engaged; the Dutch front has room
    state.done.insert(key("op", "exec"), START);
    capacity_theatre(&game, 5, 40);
    let reserve: Vec<Value> = (0..9)
        .map(|i| json!({"id": format!("m{i}"), "org": 40.0 + i as f64, "province": 421}))
        .collect();
    *game.battles.borrow_mut() = Some(json!({"list": [
        {"province": 61, "our_side": "attacker", "width": {"directions": 1},
         "attacker": {"engaged": 3, "reserve": 9, "reserve_divisions": reserve}}]}));
    game.calls.borrow_mut().clear();
    let line = run(&mut state, START + 2 * DAY).expect("a move sideways");
    assert!(line.contains("sideways onto front 109"), "{line}");
    let assign = game
        .orders()
        .into_iter()
        .find(|(n, a)| n == "hoi4_army_plan" && a["assign"].is_object())
        .expect("an assign");
    assert_eq!(assign.1["assign"]["order"], json!(109));
    // the Dutch front has room for 5 of the 6 beyond the engaged: the freshest go
    assert_eq!(
        strings(&assign.1["assign"], "divisions"),
        vec!["m8", "m7", "m6", "m5", "m4"]
    );
    assert!(state.op_notes["op:capacity:Army Group 1"].contains("breadth: 1 of 2"));
}

#[test]
fn at_war_a_front_facing_no_enemy_province_gives_its_divisions_to_the_front_that_does_at_once() {
    // p101a (28 Nov 2026): Army 2's second front 169 held 9 divisions facing 0 enemy provinces for
    // the whole war; one sideways move in 33 days
    let game = Game::new();
    capacity_theatre(&game, 5, 9);
    // the Belgian piece is behind the line: no enemy province faces front 110 any more
    let wars = json!({"wars": {"enemies": ["HOL"]}, "borders": [
        {"neighbour_tag": "HOL", "neighbour_states": [7], "own_border_states": [57]}]});
    let mut grp = unit(
        "Army Group 1".into(),
        Role::Group,
        &json!({"enemy": "HOL", "own_states": [57, 42]}),
        &json!({"target_line": [7]}),
    );
    grp.members = vec!["Mitte".into(), "Sued".into()];
    let units = vec![grp];
    // a long refinement cool-down: the dead front does not wait for it
    let op = json!({"id": "op", "enemies": ["HOL"], "refine_days": 30});
    let mut state = State::default();
    state.done.insert(key("op", "by_command"), 1);
    state.done.insert(key("op", "exec"), START);
    state.done.insert(key("op", "spread:Army Group 1:2"), START);
    let roster = game.roster.borrow().clone();
    let line = border_fronts(
        &mut state,
        &game,
        "op",
        &op,
        &units,
        &roster,
        &wars,
        "l",
        START + DAY,
    )
    .unwrap()
    .expect("a spread");
    assert!(line.contains("face no enemy province"), "{line}");
    let spread = game
        .orders()
        .into_iter()
        .find(|(n, a)| n == "hoi4_army_plan" && a["distribute"].is_object())
        .expect("a spread by weights");
    assert_eq!(spread.1["distribute"]["weights"]["110"], json!(0));
    assert!(spread.1["distribute"]["weights"]["109"].as_i64().unwrap() >= 1);
}

// ARMOUR WHERE IT CAN BREAK THROUGH (engineer 99, after o96a)

#[test]
fn the_armour_leaves_a_rough_ground_battle_s_reserve_and_goes_where_its_ground_is_cheapest() {
    // o96a (28 Nov 2026): the Panzer divisions waited in the reserve of the Ardennes forest
    // battle 3488 (state 980) while the Brabant plains battle had none
    let game = Game::new();
    let mut u = unit(
        "Panzer".into(),
        Role::Mobile,
        &json!({"enemy": "BEL"}),
        &Value::Null,
    );
    u.spear = true;
    u.line = vec![8];
    // Luxembourg, the plan's own thrust, is taken
    *game.held.borrow_mut() = vec![8];
    *game.supply.borrow_mut() = Some(json!({"divisions": [
        {"id": "p0", "army": "Panzer", "province_id": 420, "state_id": 42},
        {"id": "p1", "army": "Panzer", "province_id": 420, "state_id": 42},
        {"id": "p2", "army": "Panzer", "province_id": 420, "state_id": 42}]}));
    // from Moselland 420: straight into the Ardennes forest 3488, or round through own Rheinland
    // 510 into the plains of Wallonia 340
    *game.graph.borrow_mut() = Some(json!({"provinces": [
        graph_row(420, 42, "plains", &[(3488, 0.0), (510, 0.0)]),
        graph_row(510, 51, "plains", &[(420, 0.0), (340, 0.0)]),
        graph_row(3488, 980, "forest", &[(420, 0.0), (340, 0.0)]),
        graph_row(340, 34, "plains", &[(510, 0.0), (3488, 0.0)])]}));
    let wars = json!({"wars": {"enemies": ["BEL"]}, "borders": [
        {"neighbour_tag": "BEL", "neighbour_states": [980, 34], "own_border_states": [42, 51]}]});
    let op = json!({"id": "op", "enemies": ["BEL"]});
    let plan = json!({"war": {"me": "GER"}});
    let at_war = vec!["BEL".to_string()];
    let op_line = [980, 34];
    let mut state = State::default();
    // 1. the next objective by the armour's cost: round the forest
    armour_ground(
        &mut state, &game, &plan, "op", &op, &u, &op_line, &wars, &at_war, START,
    );
    assert_eq!(state.done.get(&key("op", "armourpick:Panzer")), Some(&34));
    let roster = game.roster.borrow().clone();
    let next = spear_line(&mut state, "op", &roster, &u, "GER", &game, &op_line, START);
    assert_eq!(next, vec![34]);
    // 2. the spearhead was given the forest state and its divisions wait in a forest battle's
    // reserve: the state goes to the infantry
    state.done.insert(key("op", "spearfor:Panzer"), 980);
    *game.battles.borrow_mut() = Some(json!({"list": [
        {"province": 3488, "our_side": "attacker",
         "lines": [{"line": "terrain", "value": -0.15}],
         "attacker": {"engaged_divisions": [{"id": "x1"}],
                      "reserve_divisions": [{"id": "p0"}, {"id": "p1"}, {"id": "p2"}]}}]}));
    armour_ground(
        &mut state,
        &game,
        &plan,
        "op",
        &op,
        &u,
        &op_line,
        &wars,
        &at_war,
        START + DAY,
    );
    assert!(state.done.contains_key(&key("op", "spearskip:Panzer:980")));
    assert!(state.op_notes["op:armour:Panzer"].contains("rough-ground battles (3488)"));
    // the plan's own thrust is never given up for its ground
    let mut own = u.clone();
    own.line = vec![980];
    *game.held.borrow_mut() = vec![];
    let mut state = State::default();
    state.done.insert(key("op", "spearfor:Panzer"), 980);
    armour_ground(
        &mut state, &game, &plan, "op", &op, &own, &op_line, &wars, &at_war, START,
    );
    assert!(!state.done.contains_key(&key("op", "spearskip:Panzer:980")));
    assert!(state.op_notes["op:armour:Panzer"].contains("plan's own thrust: kept"));
}

// SUPPLY BEFORE THE ATTACK (engineer 99, after o96a)

#[test]
fn before_the_attack_the_unrailed_node_feeding_the_most_starved_start_province_gets_a_railway() {
    let game = Game::new();
    // o96a W+10: 26 divisions in the Ardennes forest province 3488 on 1.66 against 21.3, fed by a
    // hub in 3500 with no railway; 8 in 11562 on 2.0 against 5.0, fed by the railed node 11500; 6
    // in Rheinland 3444 with headroom
    let ids = |prefix: &str, n: usize| -> Vec<Value> {
        (0..n).map(|i| json!(format!("{prefix}{i}"))).collect()
    };
    let mut divisions: Vec<Value> = Vec::new();
    for (prefix, n) in [("a", 26), ("b", 8), ("c", 6)] {
        for i in 0..n {
            divisions.push(json!({"id": format!("{prefix}{i}"), "army": "Mitte"}));
        }
    }
    *game.supply.borrow_mut() = Some(json!({"divisions": divisions,
        "supply_nodes": [{"node": 1, "kind": "hub", "province_id": 3500, "railway_level": 0},
                         {"node": 2, "kind": "hub", "province_id": 11500, "railway_level": 2}],
        "provinces": [
        {"province_id": 11562, "state_id": 35, "divisions": 8, "division_ids": ids("b", 8),
         "supply": 2.03, "demand": 4.98, "railway_level": 2,
         "flows": [{"node": 2, "flow": 2.0, "node_province": 11500}]},
        {"province_id": 3488, "state_id": 980, "divisions": 26, "division_ids": ids("a", 26),
         "supply": 1.66, "demand": 21.3, "railway_level": 0,
         "flows": [{"node": 2, "flow": 0.2, "node_province": 11500},
                   {"node": 1, "flow": 1.4, "node_province": 3500}]},
        {"province_id": 3444, "state_id": 51, "divisions": 6, "division_ids": ids("c", 6),
         "supply": 4.57, "demand": 1.75, "railway_level": 0}]}));
    let u = unit("Mitte".into(), Role::Line, &Value::Null, &Value::Null);
    let units = vec![u];
    let op = json!({"id": "op"});
    let roster = game.roster.borrow().clone();
    let mut state = State::default();
    let line = prepare_supply(&mut state, &game, "op", &op, &units, &roster, "l", START)
        .unwrap()
        .expect("a railway");
    assert!(
        line.contains("railway to the supply node in province 3500, which feeds province 3488"),
        "{line}"
    );
    let rail = game
        .orders()
        .into_iter()
        .find(|(n, _)| n == "hoi4_railway_build")
        .unwrap();
    assert_eq!(rail.1["to_province"], json!(3500));
    assert_eq!(rail.1["from_nearest_node"], json!(true));
    assert!(rail.1.get("to_nearest_node").is_none());
    let note = &state.op_notes["op:supply_prepare"];
    assert!(note.contains("3488 (state 980): 26 divisions at 8% of their need, fed by node 3500 railway 0, node 11500 railway 2"), "{note}");
    assert!(note.contains("11562 (state 35): 8 divisions"));
    // once a day; the same node not again for 30 days; a province fed by railed nodes only never
    assert!(prepare_supply(
        &mut state,
        &game,
        "op",
        &op,
        &units,
        &roster,
        "l",
        START + 1
    )
    .unwrap()
    .is_none());
    assert!(prepare_supply(
        &mut state,
        &game,
        "op",
        &op,
        &units,
        &roster,
        "l",
        START + DAY
    )
    .unwrap()
    .is_none());
    // switched off by the plan
    let off = json!({"id": "op", "supply": {"prepare": false}});
    let mut state = State::default();
    assert!(
        prepare_supply(&mut state, &game, "op", &off, &units, &roster, "l", START)
            .unwrap()
            .is_none()
    );
}

#[test]
fn with_no_feeding_node_read_the_worst_fed_start_province_gets_its_nearest_railway_end() {
    // w96d (28 Nov 2026): every start province 'fed by no node' and no railway in the prepare
    let game = Game::new();
    let divisions: Vec<Value> = (0..20)
        .map(|i| json!({"id": format!("a{i}"), "army": "Mitte"}))
        .collect();
    let ids: Vec<Value> = (0..20).map(|i| json!(format!("a{i}"))).collect();
    *game.supply.borrow_mut() = Some(json!({"divisions": divisions, "provinces": [
        {"province_id": 17, "state_id": 86, "divisions": 20, "division_ids": ids,
         "supply": 2.0, "demand": 20.0, "railway_level": 0, "flows": []}]}));
    let units = vec![unit("Mitte".into(), Role::Line, &Value::Null, &Value::Null)];
    let roster = game.roster.borrow().clone();
    let mut state = State::default();
    let op = json!({"id": "op"});
    let line = prepare_supply(&mut state, &game, "op", &op, &units, &roster, "l", START)
        .unwrap()
        .expect("a railway");
    assert!(line.contains("railway toward province 17"), "{line}");
    let rail = game
        .orders()
        .into_iter()
        .find(|(n, _)| n == "hoi4_railway_build")
        .unwrap();
    assert_eq!(
        rail.1,
        json!({"to_province": 17, "to_state": 86, "to_nearest_node": true, "from_nearest_node": true})
    );
    assert!(state.op_notes["op:supply_prepare"].contains("no node read (no flow in the read)"));
    // not again for the same province within 30 days
    assert!(prepare_supply(
        &mut state,
        &game,
        "op",
        &op,
        &units,
        &roster,
        "l",
        START + DAY
    )
    .unwrap()
    .is_none());
}

// THE STUCK POINT (engineer 99, after o96a)

#[test]
fn a_battle_stuck_for_days_is_attacked_by_the_idle_divisions_on_its_unused_side() {
    let game = Game::new();
    *game.battles.borrow_mut() = Some(json!({"list": [
        {"province": 6583, "state": 8, "hours": 100, "our_side": "attacker", "winning": "defender",
         "width": {"attacker_max": 60.0, "attacker_engaged": 20.0, "full": false},
         "attacker": {"engaged_divisions": [{"id": "m1", "province": 6600, "width": 10.0},
                                            {"id": "m2", "province": 6600, "width": 10.0}]},
         "defender": {}}]}));
    let d = |id: &str, p: i64| json!({"id": id, "army": "Mitte", "template": "Inf", "province_id": p, "organisation": 60.0});
    *game.supply.borrow_mut() = Some(json!({"divisions": [
        d("m1", 6600), d("m2", 6600), d("m3", 6601), d("m4", 6601), d("m5", 6601)]}));
    *game.graph.borrow_mut() = Some(json!({"provinces": [
        {"province": 6583, "state_id": 8, "borders": [{"province": 6600}, {"province": 6601}]}]}));
    let units = vec![unit("Mitte".into(), Role::Line, &Value::Null, &Value::Null)];
    let roster = game.roster.borrow().clone();
    let op = json!({"id": "op"});
    let mut state = State::default();
    let line = stuck_push_step(&mut state, &game, "op", &op, &units, &roster, "l", START)
        .unwrap()
        .expect("a push");
    assert!(
        line.contains("stuck point: the battle at 6583 stood 4 days"),
        "{line}"
    );
    let mv = game
        .orders()
        .into_iter()
        .find(|(n, _)| n == "hoi4_army_move")
        .unwrap();
    assert_eq!(mv.1["province_id"], json!(6583));
    // the move tool takes the state with the province (w96d: invalid_arguments without it)
    assert_eq!(mv.1["state_id"], json!(8));
    assert_eq!(mv.1["divisions"], json!(["m3", "m4", "m5"]));
    // once a day; the same battle not again within 5 days; off by the plan
    assert!(stuck_push_step(
        &mut state,
        &game,
        "op",
        &op,
        &units,
        &roster,
        "l",
        START + 1
    )
    .unwrap()
    .is_none());
    assert!(stuck_push_step(
        &mut state,
        &game,
        "op",
        &op,
        &units,
        &roster,
        "l",
        START + DAY
    )
    .unwrap()
    .is_none());
    let mut state = State::default();
    let off = json!({"id": "op", "push": false});
    assert!(
        stuck_push_step(&mut state, &game, "op", &off, &units, &roster, "l", START)
            .unwrap()
            .is_none()
    );
}

#[test]
fn the_opening_s_need_against_a_small_enemy_is_room_for_a_loan_whatever_the_width_reads() {
    // w101a W+5 (28 Nov 2026): op_lowlands' battles took 51 divisions and 75 stood there (room
    // -24), Luxembourg had 8 facing it against 13 French; Army 1 (24) and Army 10 (4) stood at home
    let roster = json!({"armies": [
        {"army": "Army 7", "division_count": 8, "states": {"42": 8}},
        {"army": "Armee West Süd", "division_count": 14, "states": {"42": 14}},
        {"army": "Army 2", "division_count": 10, "states": {"51": 10}},
        {"army": "Army 1", "division_count": 24, "states": {"68": 24}},
        {"army": "Army 10", "division_count": 4, "states": {"64": 4}}]});
    let wars = json!({"wars": {"at_war": true, "enemies": ["LUX", "BEL", "HOL", "FRA"]},
        "borders": [{"neighbour_tag": "LUX", "own_border_states": [42], "neighbour_states": [8]}]});
    let ops = json!([
        {"id": "op_lowlands", "enemies": ["BEL", "HOL", "LUX"], "stacking": {"by_width": true},
         "opening": {"deadline_days": {"LUX": 4}, "ratio": 2},
         "formations": [{"army": "Army 2", "role": "line"},
                        {"army": "Army 7", "role": "mobile"},
                        {"army": "Armee West Süd", "role": "wing"}]}]);
    let plan = json!({"war": {"land_orders": false}, "operations": ops});
    let now = 1000 * DAY;
    let mut state = State::default();
    for k in ["prep", "exec"] {
        state.done.insert(key("op_lowlands", k), now - 5 * DAY);
    }
    state.done.insert(key("op_lowlands", "room"), -24);
    state.done.insert(key("op_lowlands", "room_at"), now - DAY);
    for a in ["Army 7", "Armee West Süd", "Army 2"] {
        state.op_owned.insert(a.into(), "op_lowlands".into());
    }
    let ops = list(&plan, "operations").to_vec();
    let active = vec![("op_lowlands".to_string(), ops[0].clone())];
    // without the opening's need: no room, nothing lent
    lend_idle(&mut state, &plan, &ops, &active, &roster, &wars, now);
    let note = state.op_notes["lend"].clone();
    assert!(
        note.contains(
            "Army 1 (24): idle, no executing operation has room on its front (op_lowlands -24)"
        ),
        "{note}"
    );
    assert!(echelons(&state, "op_lowlands").is_empty());
    // the opening read Luxembourg short: 2 x 13 - 8 = 18
    state.done.insert(key("op_lowlands", "need"), 18);
    state.done.insert(key("op_lowlands", "need_at"), now);
    lend_idle(&mut state, &plan, &ops, &active, &roster, &wars, now + 1);
    let note = state.op_notes["lend"].clone();
    assert!(
        note.contains("Army 1 (24): lent to op_lowlands (room 18) as the echelon of Army 2"),
        "{note}"
    );
    assert_eq!(state.layout["echelon:op_lowlands:Army 2"], "Army 1");
}

#[test]
fn at_war_an_army_no_running_operation_owns_is_lent_to_the_one_with_room_until_its_own_starts() {
    // w96d (28 Nov 2026): Army 1 (no formation) and Army 6 (op_weser's, waiting) stood on area
    // defences while op_france's battles took more than stood there
    let roster = json!({"armies": [
        {"army": "Armee A", "division_count": 20, "states": {"10": 20}},
        {"army": "Armee B", "division_count": 14, "states": {"11": 14}},
        {"army": "Army 1", "division_count": 24, "states": {"3": 24}},
        {"army": "Army 6", "division_count": 18, "states": {"4": 18}},
        {"army": "Army 10", "division_count": 5, "states": {"5": 5}},
        {"army": "Grenz", "division_count": 6, "states": {"50": 6}}]});
    let wars = json!({"wars": {"at_war": true, "enemies": ["FRA"]},
        "borders": [{"neighbour_tag": "FRA", "own_border_states": [50], "neighbour_states": [60]}]});
    let ops = json!([
        {"id": "op_france", "enemies": ["FRA"], "stacking": {"by_width": true},
         "formations": [{"army": "Armee A", "role": "line"}, {"army": "Armee B", "role": "line"}]},
        {"id": "op_weser", "enemies": ["DEN"], "formations": [{"army": "Army 6", "role": "line"}]}]);
    let plan = json!({"war": {"land_orders": false}, "operations": ops,
        "lend": {"keep": [{"army": "Army 10", "why": "the Danish coast garrison"}]}});
    let now = 1000 * DAY;
    let mut state = State::default();
    for k in ["prep", "exec"] {
        state.done.insert(key("op_france", k), now - 5 * DAY);
    }
    state.done.insert(key("op_france", "room"), 30);
    state.done.insert(key("op_france", "room_at"), now - DAY);
    for a in ["Armee A", "Armee B"] {
        state.op_owned.insert(a.into(), "op_france".into());
    }
    let ops = list(&plan, "operations").to_vec();
    let active = vec![("op_france".to_string(), ops[0].clone())];
    lend_idle(&mut state, &plan, &ops, &active, &roster, &wars, now);
    let note = state.op_notes["lend"].clone();
    assert!(
        note.contains("Army 1 (24): lent to op_france (room 30) as the echelon of Armee B"),
        "{note}"
    );
    assert!(
        note.contains("Army 6 (18): lent to op_france (room 6) as the echelon of Armee A, until op_weser starts its prepare"),
        "{note}"
    );
    assert!(
        note.contains("Army 10 (5): kept, the Danish coast garrison"),
        "{note}"
    );
    assert!(note.contains("Grenz (6): meets an enemy"), "{note}");
    assert_eq!(state.layout["echelon:op_france:Armee B"], "Army 1");
    assert_eq!(state.op_owned["Army 6"], "op_france");
    assert!(state.done.contains_key(&key("op_france", "lent:Army 1")));
    // the loans count against the room until they are forward: nothing more is lent
    lend_idle(
        &mut state,
        &plan,
        &ops,
        &active,
        &roster,
        &wars,
        now + DAY / 2,
    );
    assert_eq!(echelons(&state, "op_france").len(), 2);
    // op_weser starts its prepare: Army 6 goes back, Army 1 stays lent
    state.done.insert(key("op_weser", "prep"), now + DAY);
    lend_idle(&mut state, &plan, &ops, &active, &roster, &wars, now + DAY);
    let note = state.op_notes["lend"].clone();
    assert!(
        note.contains("Army 6: the loan to op_france ends, its own operation op_weser has started"),
        "{note}"
    );
    assert!(!state.done.contains_key(&key("op_france", "lent:Army 6")));
    assert!(!state.op_owned.contains_key("Army 6"));
    assert_eq!(
        echelons(&state, "op_france"),
        vec![("Armee B".to_string(), "Army 1".to_string())]
    );
    // in peace nothing is lent
    let mut calm = State::default();
    let peace = json!({"wars": {"at_war": false, "enemies": []}, "borders": []});
    lend_idle(&mut calm, &plan, &ops, &active, &roster, &peace, now);
    assert!(calm.layout.is_empty());
}

// LUXEMBOURG: A SMALL ENEMY INSIDE THE OPENING (engineer 103, after w101a)

/// The Rhineland (42) borders Luxembourg (8), the Belgian Ardennes (980) and Alsace (28):
/// Panzergruppe Ost's root 59 lies along the French border, root 60 faces Belgium and Luxembourg.
fn rhineland_graph() -> Value {
    json!({"provinces": [
        graph_row(9575, 42, "plains", &[(549, 0.0), (11640, 0.0)]),
        graph_row(11640, 978, "plains", &[(549, 0.0), (9575, 0.0)]),
        graph_row(11435, 42, "forest", &[(6583, 0.0), (3488, 0.0), (3547, 0.0)]),
        graph_row(3547, 51, "plains", &[(3488, 0.0), (11435, 0.0)]),
        graph_row(6583, 8, "forest", &[(11435, 0.0)]),
        graph_row(3488, 980, "forest", &[(11435, 0.0), (3547, 0.0)]),
        graph_row(549, 28, "plains", &[(9575, 0.0), (11640, 0.0)])]})
}

fn rhineland_wars(at_war: &[&str]) -> Value {
    json!({"wars": {"enemies": at_war}, "borders": [
        {"neighbour_tag": "LUX", "own_border_states": [42], "neighbour_states": [8]},
        {"neighbour_tag": "BEL", "own_border_states": [42, 51], "neighbour_states": [980]},
        {"neighbour_tag": "FRA", "own_border_states": [42, 978], "neighbour_states": [28]}]})
}

#[test]
fn draws_spent_in_peace_do_not_hold_the_war() {
    // four draws refused in peace: spent in peace, not at war
    assert!(draws_spent(4, 0, true));
    assert!(!draws_spent(4, 0, false));
    // at war the line gets REDRAWS draws of its own
    assert!(!draws_spent(5, 1, false));
    assert!(draws_spent(6, 2, false));
    // a line drawn only at war counts as before
    assert!(!draws_spent(3, 3, false));
    assert!(draws_spent(4, 4, false));
}

#[test]
fn a_spearhead_goes_under_the_root_that_faces_its_state() {
    let game = Game::new();
    *game.graph.borrow_mut() = Some(rhineland_graph());
    let f59 =
        json!({"kind": "front", "id": 59, "line": {"states": [978, 42], "ids": [11640, 9575]}});
    let f60 =
        json!({"kind": "front", "id": 60, "line": {"states": [42, 51], "ids": [11435, 3547]}});
    assert_eq!(facing_front(&game, &[&f59, &f60], &[8]), Some(60));
    assert_eq!(facing_front(&game, &[&f59, &f60], &[28]), Some(59));
    // one root: the caller's first front
    assert_eq!(facing_front(&game, &[&f59], &[8]), None);
}

#[test]
fn a_wing_on_a_state_bordering_another_enemy_of_the_opening_takes_its_piece_at_war() {
    // w101a: Armee West Süd stood in the Rhineland against Belgium; Luxembourg, an enemy of the
    // same operation, borders the Rhineland too and had no front but the armour's
    let game = Game::new();
    *game.graph.borrow_mut() = Some(rhineland_graph());
    game.unit("Nord", |r| {
        r["plan"] = json!([{"kind": "front", "id": 61, "executing": true, "divisions": 14,
            "line": {"states": [42, 51], "ids": [3547]}, "offensives": []}]);
    });
    let mut u = unit(
        "Nord".into(),
        Role::Wing,
        &json!({"enemy": "BEL", "own_states": [42]}),
        &Value::Null,
    );
    u.enemy = "BEL".into();
    u.own_states = vec![42];
    let mut units = vec![u];
    let enemies: Vec<String> = ["BEL", "HOL", "LUX"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    // in peace: the piece is noted, no root is drawn against a country at peace
    let peace = rhineland_wars(&["FRA"]);
    widen_wings(&mut units, &enemies, &peace);
    assert_eq!(units[0].enemies, vec!["LUX".to_string()]);
    let op = json!({"id": "op", "enemies": enemies});
    let mut state = State::default();
    state.done.insert(key("op", "by_command"), 1);
    let roster = game.roster.borrow().clone();
    let out = border_fronts(
        &mut state, &game, "op", &op, &units, &roster, &peace, "l", START,
    )
    .unwrap();
    assert!(
        !game.orders().iter().any(|(n, _)| n == "hoi4_army_front"),
        "{out:?}"
    );
    assert!(state.op_notes["op:piece:Nord:LUX"].contains("when the war on LUX is on"));
    // at war with Luxembourg: a root of its own against it, on its own states
    let war = rhineland_wars(&["FRA", "BEL", "LUX"]);
    let line = border_fronts(
        &mut state,
        &game,
        "op",
        &op,
        &units,
        &roster,
        &war,
        "l",
        START + DAY,
    )
    .unwrap()
    .expect("a front against Luxembourg");
    assert!(line.contains("no front of it faces LUX"), "{line}");
    let front = game
        .orders()
        .into_iter()
        .find(|(n, _)| n == "hoi4_army_front")
        .unwrap();
    assert_eq!(front.1["against"], json!("LUX"));
    assert_eq!(front.1["own_states"], json!([42]));
    assert_eq!(front.1["state_id"], json!(8));
    // a spearhead army keeps its one narrow front
    let mut spear = unit(
        "Panzer".into(),
        Role::Mobile,
        &json!({"enemy": "LUX"}),
        &Value::Null,
    );
    spear.own_states = vec![42];
    let mut only = vec![spear];
    widen_wings(&mut only, &enemies, &war);
    assert!(only[0].enemies.is_empty());
}
