//! Executor throughput (engineer 93): the tick's snapshot, many lanes per tick, the per-object
//! claim, the input meter, the clock-driven wait and the pace at the gate.
use super::*;
use std::cell::{Cell, RefCell};

const START: GameHour = 60_000_000;

/// Every call is recorded; an order is Verified, by command (`in_process` in the reply) or by
/// mouse; `hoi4_overview` reads the clock, which moves `step` hours a read.
struct Game {
    calls: RefCell<Vec<(String, Value)>>,
    by_command: bool,
    hour: Cell<GameHour>,
    step: GameHour,
}
impl Game {
    fn new(by_command: bool) -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            by_command,
            hour: Cell::new(START),
            step: 0,
        }
    }
    fn count(&self, name: &str) -> usize {
        self.calls
            .borrow()
            .iter()
            .filter(|(n, _)| n == name)
            .count()
    }
    fn orders(&self) -> usize {
        self.calls
            .borrow()
            .iter()
            .filter(|(_, a)| a.get("request_id").is_some())
            .count()
    }
}
impl Tools for Game {
    fn call(&self, name: &str, args: &Value) -> Result<Value> {
        self.calls
            .borrow_mut()
            .push((name.to_string(), args.clone()));
        if name == "hoi4_overview" {
            let now = self.hour.get();
            self.hour.set(now + self.step);
            return Ok(json!({"date": {"raw": now}}));
        }
        if args.get("request_id").is_some() {
            return Ok(if self.by_command {
                json!({"receipt": {"status": "Verified"}, "in_process": {"backend": "inprocess"}})
            } else {
                json!({"receipt": {"status": "Verified"}})
            });
        }
        Ok(json!({}))
    }
}

fn two_picks() -> Value {
    json!({"politics": [
        {"id": "a", "category": "government", "slot": 0, "idea": "idea_a"},
        {"id": "b", "category": "government", "slot": 1, "idea": "idea_b"}]})
}

#[test]
fn orders_by_command_share_one_tick_and_a_mouse_order_ends_it() {
    // w92b, t1939o (27 Nov 2026): one acting lane a tick, about two ticks a game day at war
    let game = Game::new(true);
    let mut state = State::default();
    let out = tick(&mut state, &game, &two_picks(), "l", START).unwrap();
    assert!(matches!(out, Tick::Acted(_)), "{out:?}");
    assert_eq!(
        game.count("hoi4_politics_choose"),
        2,
        "both picks by command in one tick"
    );
    assert!(
        state.sched.more.iter().any(|l| l.contains("idea_")),
        "the second pick's line is reported too: {:?}",
        state.sched.more
    );
    assert_eq!(state.sched.ticks, 1);
    // by mouse: one input sequence a tick
    let game = Game::new(false);
    let mut state = State::default();
    tick(&mut state, &game, &two_picks(), "l", START).unwrap();
    assert_eq!(game.count("hoi4_politics_choose"), 1);
    assert_eq!(game.orders(), 1, "one mouse order a tick");
    // the plan can keep one acting lane a tick
    let game = Game::new(true);
    let mut state = State::default();
    let mut plan = two_picks();
    plan["scheduler"] = json!({"many_lanes": false});
    tick(&mut state, &game, &plan, "l", START).unwrap();
    assert_eq!(game.count("hoi4_politics_choose"), 1);
}

#[test]
fn the_sweep_offers_the_lanes_whose_last_turn_needed_no_input() {
    let game = Game::new(true);
    let mut state = State::default();
    let plan = two_picks();
    state.sched.begin_tick(START);
    state.sched.lane_input.insert("fast:politics", false);
    let mut lines = Vec::new();
    sweep(
        &mut state,
        &game,
        &plan,
        "l",
        START,
        &mut lines,
        std::time::Instant::now(),
    )
    .unwrap();
    assert_eq!(
        game.count("hoi4_politics_choose"),
        1,
        "one sweep turn a lane"
    );
    assert_eq!(lines.len(), 1, "{lines:?}");
    // a lane whose last turn needed the input waits for the scheduler's order
    state.sched.begin_tick(START + 1);
    state.sched.lane_input.insert("fast:politics", true);
    let mut lines = Vec::new();
    sweep(
        &mut state,
        &game,
        &plan,
        "l",
        START + 1,
        &mut lines,
        std::time::Instant::now(),
    )
    .unwrap();
    assert_eq!(game.count("hoi4_politics_choose"), 1);
    assert!(lines.is_empty());
}

#[test]
fn two_lanes_never_order_the_same_object_in_one_tick() {
    let game = Game::new(true);
    let mut state = State::default();
    state.sched.begin_tick(START);
    let order = json!({"army": "Army 1", "state_id": 64});
    state.sched.cur_lane = "war";
    let (ok, _) = state
        .act(&game, "l", "hoi4_army_front", order.clone(), "w")
        .unwrap();
    assert!(ok);
    // the same lane may order its army again in its turn
    let (ok, _) = state
        .act(&game, "l", "hoi4_army_plan", order.clone(), "w")
        .unwrap();
    assert!(ok);
    state.sched.cur_lane = "hold";
    let (ok, why) = state
        .act(&game, "l", "hoi4_army_area_defense", order.clone(), "h")
        .unwrap();
    assert!(!ok && why.starts_with("deferred: army:Army 1"), "{why}");
    assert!(state.sched.deferred);
    assert_eq!(game.orders(), 2, "the deferred order was not sent");
    assert!(
        state.failures.is_empty() && state.failed_run.is_empty(),
        "a deferral is not a failure"
    );
    // the next tick it goes out
    state.sched.begin_tick(START + 1);
    state.sched.cur_lane = "hold";
    let (ok, _) = state
        .act(&game, "l", "hoi4_army_area_defense", order, "h")
        .unwrap();
    assert!(ok);
    // a slot is an object within its family: a research slot is not a law seat
    let keys = claim_keys("hoi4_research_start", &json!({"slot": 0, "tech": "t"}));
    let seat = claim_keys(
        "hoi4_politics_choose",
        &json!({"category": "government", "slot": 0, "idea": "i"}),
    );
    assert!(keys.iter().all(|k| !seat.contains(k)), "{keys:?} {seat:?}");
    assert_eq!(
        claim_keys("hoi4_air_mission", &json!({"wings": ["JG 1", "JG 2"]})),
        vec!["wing:JG 1".to_string(), "wing:JG 2".to_string()]
    );
}

#[test]
fn an_identical_order_goes_once_a_tick_whichever_turn_sends_it() {
    // a lane offered again while it sends orders by command never repeats an order in the tick
    let game = Game::new(true);
    let mut state = State::default();
    state.sched.begin_tick(START);
    let order = json!({"line": 2, "equipment": "infantry_equipment_1", "factories": 14});
    let opened = open_turn(&mut state, "balance", START);
    let (ok, _) = state
        .act(&game, "l", "hoi4_production_set", order.clone(), "b")
        .unwrap();
    assert!(ok);
    close_turn(&mut state, "balance", opened, Some("x".into()));
    let _ = open_turn(&mut state, "balance", START);
    let (ok, why) = state
        .act(&game, "l", "hoi4_production_set", order.clone(), "b")
        .unwrap();
    assert!(!ok && why.contains("sent in this tick already"), "{why}");
    assert_eq!(game.orders(), 1);
    state.sched.begin_tick(START + 1);
    let _ = open_turn(&mut state, "balance", START + 1);
    let (ok, _) = state
        .act(&game, "l", "hoi4_production_set", order, "b")
        .unwrap();
    assert!(ok, "the next tick reads the game again and may send it");
}

#[test]
fn a_deferred_turn_is_quiet_and_the_lane_waits_for_the_next_tick() {
    let mut state = State::default();
    state.sched.begin_tick(START);
    let opened = open_turn(&mut state, "hold", START);
    state.sched.deferred = true;
    state.last_ok = Some(false);
    let (result, input) = close_turn(&mut state, "hold", opened, Some("hold: false".into()));
    assert_eq!(result, None);
    assert!(!input);
    assert!(state.sched.tick_turned.contains("hold"));
    assert!(!state.sched.deferred);
}

#[test]
fn the_meter_tells_an_order_by_command_from_one_by_mouse() {
    let mouse = Game::new(false);
    let command = Game::new(true);
    let before = input_calls();
    let m = Metered { inner: &mouse };
    m.call("hoi4_armies", &json!({})).unwrap();
    assert_eq!(input_calls(), before, "a memory read needs no input");
    m.call(
        "hoi4_production_set",
        &json!({"request_id": "r", "lease_token": "l"}),
    )
    .unwrap();
    assert_eq!(input_calls(), before + 1, "a mouse order");
    m.call("hoi4_politics_options", &json!({"lease_token": "l"}))
        .unwrap();
    assert_eq!(input_calls(), before + 2, "a read through a panel");
    let orders = in_process_orders();
    let c = Metered { inner: &command };
    c.call(
        "hoi4_production_set",
        &json!({"request_id": "r", "lease_token": "l"}),
    )
    .unwrap();
    assert_eq!(input_calls(), before + 2, "an order by command");
    assert_eq!(in_process_orders(), orders + 1);
    assert!(reply_in_process(
        &json!({"receipt": {}, "result": {"backend": "inprocess"}})
    ));
    assert!(!reply_in_process(
        &json!({"receipt": {}, "in_process": null})
    ));
}

#[test]
fn the_snapshot_is_shared_and_an_order_forgets_only_what_its_family_moves() {
    let game = Game::new(true);
    let reads = TickReads {
        inner: &game,
        seen: Default::default(),
    };
    for _ in 0..3 {
        reads.call("hoi4_armies", &json!({})).unwrap();
        reads.call("hoi4_events", &json!({})).unwrap();
        reads.call("hoi4_production_lines", &json!({})).unwrap();
    }
    assert_eq!(game.count("hoi4_armies"), 1);
    assert_eq!(game.count("hoi4_events"), 1, "every memory read is shared");
    // a production order: the production lines again, the armies and events from the snapshot
    reads
        .call("hoi4_production_set", &json!({"request_id": "r"}))
        .unwrap();
    reads.call("hoi4_armies", &json!({})).unwrap();
    reads.call("hoi4_events", &json!({})).unwrap();
    reads.call("hoi4_production_lines", &json!({})).unwrap();
    assert_eq!(game.count("hoi4_armies"), 1);
    assert_eq!(game.count("hoi4_events"), 1);
    assert_eq!(game.count("hoi4_production_lines"), 2);
    // an order outside the known families (a decision may open an event) forgets everything
    reads
        .call("hoi4_decision_take", &json!({"request_id": "d"}))
        .unwrap();
    reads.call("hoi4_armies", &json!({})).unwrap();
    reads.call("hoi4_events", &json!({})).unwrap();
    assert_eq!(game.count("hoi4_armies"), 2);
    assert_eq!(game.count("hoi4_events"), 2);
    // a read through a panel is never shared
    for _ in 0..2 {
        reads
            .call("hoi4_politics_options", &json!({"lease_token": "l"}))
            .unwrap();
    }
    assert_eq!(game.count("hoi4_politics_options"), 2);
    assert!(read_survives("hoi4_air_mission", "hoi4_production_lines"));
    assert!(!read_survives("hoi4_air_mission", "hoi4_air"));
    assert!(!read_survives("hoi4_air_attach", "hoi4_armies"));
    assert!(read_survives("hoi4_research_start", "hoi4_armies"));
    assert!(!read_survives("hoi4_research_start", "hoi4_research"));
    assert!(read_survives("hoi4_army_move", "hoi4_province_graph"));
}

#[test]
fn a_run_with_nothing_to_do_looks_again_on_the_next_game_hour() {
    let mut game = Game::new(true);
    game.step = 1;
    let mut state = State::default();
    let at = std::time::Instant::now();
    let now = wait_next_hour(&mut state, &game, START, Duration::from_secs(5));
    assert!(now > START);
    assert!(at.elapsed() < Duration::from_secs(1), "no fixed sleep");
    // a clock that stands still (a paused game) waits the longest wait only
    let still = Game::new(true);
    let mut state = State::default();
    let at = std::time::Instant::now();
    let now = wait_next_hour(&mut state, &still, START, Duration::from_millis(250));
    assert_eq!(now, START);
    assert!(at.elapsed() >= Duration::from_millis(250));
}

#[test]
fn the_gate_shows_ticks_and_lane_turns_per_game_day() {
    let game = Game::new(true);
    let mut state = State::default();
    let plan = two_picks();
    for day in 0..4 {
        tick(&mut state, &game, &plan, "l", START + day * DAY).unwrap();
    }
    let rows = lane_turns(&state, START + 4 * DAY);
    let pace = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "ticks")
        .cloned()
        .unwrap();
    assert_eq!(pace["ticks"], 4);
    assert_eq!(pace["ticks_per_day"], 1.0);
    assert!(pace["lane_turns_per_day"].as_f64().unwrap() > 1.0, "{pace}");
    let politics = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["lane"] == "politics")
        .cloned()
        .unwrap();
    assert_eq!(politics["input"], false, "{politics}");
    assert!(politics["per_day"].is_number(), "{politics}");
    assert_eq!(politics["starved"], false);
}

#[test]
fn while_an_operation_runs_the_war_is_offered_before_a_fast_lanes_mouse_order() {
    // w92c (28 Nov 2026): 11 operation turns in 33 war days; a fast lane's mouse order ends the
    // tick's rounds, so it must not come before the operation's turn
    let game = Game::new(false);
    let mut state = State::default();
    state.done.insert("op:op_x:prep".into(), START);
    state.done.insert("op:op_x:ready".into(), START);
    state.done.insert("op:op_x:exec".into(), START);
    let mut plan = two_picks();
    plan["war"] = json!({"enabled": true});
    tick(&mut state, &game, &plan, "l", START).unwrap();
    assert_eq!(
        game.count("hoi4_politics_choose"),
        1,
        "the fast lane still acts"
    );
    assert_eq!(
        state.sched.war_tick,
        Some(state.sched.tick_no),
        "the war was offered in the round the mouse order ended"
    );
    // without an operation the fast lanes keep their place ahead of the war
    let game = Game::new(false);
    let mut state = State::default();
    tick(&mut state, &game, &plan, "l", START).unwrap();
    assert_eq!(game.count("hoi4_politics_choose"), 1);
    assert_ne!(state.sched.war_tick, Some(state.sched.tick_no));
}

#[test]
fn war_turns_by_command_take_none_of_the_lanes_input_share() {
    let mut round = super::Round::default();
    for h in 0..10 {
        round.war_acted(h, false);
    }
    assert!(
        !round.lanes_owed(10),
        "ten war turns by command, no input tick taken"
    );
    for h in 10..10 + WAR_SHARE as GameHour {
        round.war_acted(h, true);
    }
    assert!(
        round.lanes_owed(20),
        "the input turns still have their share"
    );
    // the day's bound holds whatever the war's turns were
    let mut round = super::Round::default();
    round.lanes_turn(0);
    round.war_acted(1, false);
    assert!(round.lanes_owed(DAY));
}

/// An executing operation (the done-memory keys `plan_operation` reads).
fn executing(state: &mut State) {
    state.done.insert("op:op_x:prep".into(), START);
    state.done.insert("op:op_x:ready".into(), START);
    state.done.insert("op:op_x:exec".into(), START);
}

#[test]
fn while_an_operation_executes_the_war_is_offered_in_every_tick_even_when_the_lanes_are_owed() {
    // w101b, w101c, p101b (28 Nov 2026): a tick at war lasts 0.7-0.8 game days, so the lanes were
    // owed after every input tick and the war had at best every other tick (0.27-0.43 acting war
    // ticks a war day)
    let game = Game::new(false);
    let mut state = State::default();
    executing(&mut state);
    state.round.lanes_turn(START - 2 * DAY);
    assert!(state.round.lanes_owed(START));
    let mut plan = two_picks();
    plan["war"] = json!({"enabled": true});
    tick(&mut state, &game, &plan, "l", START).unwrap();
    assert_eq!(
        state.sched.war_tick,
        Some(state.sched.tick_no),
        "the war was offered although the lanes were owed"
    );
    assert_eq!(
        game.count("hoi4_politics_choose"),
        1,
        "the lanes still have the tick's input"
    );
}

#[test]
fn a_fast_lane_s_input_turn_is_the_lanes_tick_so_the_war_has_the_input_next() {
    // p101b (28 Nov 2026): the fast lanes and the fairness returned before the pass's own
    // `lanes_turn`, the lanes stayed owed and five ticks went by without a war turn (353 h)
    let game = Game::new(false);
    let mut state = State::default();
    executing(&mut state);
    state.round.lanes_turn(START - 2 * DAY);
    let mut plan = two_picks();
    plan["war"] = json!({"enabled": true});
    tick(&mut state, &game, &plan, "l", START).unwrap();
    assert_eq!(game.count("hoi4_politics_choose"), 1);
    assert!(
        !state.round.lanes_owed(START),
        "the fast lane's mouse order was the lanes' tick"
    );
}

#[test]
fn by_command_alone_the_war_s_mouse_orders_wait_and_its_orders_by_command_go() {
    let game = Game::new(false);
    let metered = Metered { inner: &game };
    let mut state = State::default();
    let args = json!({"category": "government", "slot": 0, "idea": "idea_a"});
    note_tool_input("hoi4_politics_choose", true);
    state.sched.input_yield = true;
    let (ok, why) = state
        .act(&metered, "l", "hoi4_politics_choose", args.clone(), "t")
        .unwrap();
    assert!(!ok);
    assert!(
        why.starts_with("deferred: the lanes have this tick's input"),
        "{why}"
    );
    assert_eq!(game.orders(), 0, "nothing was sent");
    assert!(state.failures.is_empty(), "a deferral is no failure");
    assert_eq!(state.sched.yielded, 1);
    // a tool whose last order went by command is sent at once
    note_tool_input("hoi4_politics_choose", false);
    let (ok, _) = state
        .act(&metered, "l", "hoi4_politics_choose", args.clone(), "t")
        .unwrap();
    assert!(ok);
    assert_eq!(game.orders(), 1);
    // the meter records what the order cost: by mouse here
    assert_eq!(tool_last_input("hoi4_politics_choose"), Some(true));
    // outside the yield the mouse order goes
    state.sched.input_yield = false;
    let (ok, _) = state
        .act(&metered, "l", "hoi4_politics_choose", args, "t2")
        .unwrap();
    assert!(ok);
    assert_eq!(game.orders(), 2);
}

#[test]
fn a_slow_panel_read_stands_for_its_span_at_war_and_an_order_of_its_family_forgets_it() {
    let game = Game::new(false);
    let cache = std::cell::RefCell::new(SlowCache::new());
    let read = |now: GameHour, args: Value| {
        SlowReads {
            inner: &game,
            cache: &cache,
            now,
            on: true,
        }
        .call("hoi4_politics_options", &args)
        .unwrap();
    };
    let seat = json!({"category": "government", "slot": 0, "lease_token": "l"});
    let day0 = (START / DAY) * DAY;
    read(day0 + 1, seat.clone());
    read(
        day0 + 20,
        json!({"category": "government", "slot": 0, "lease_token": "other"}),
    );
    assert_eq!(
        game.count("hoi4_politics_options"),
        1,
        "one read a game day"
    );
    read(
        day0 + 20,
        json!({"category": "government", "slot": 1, "lease_token": "l"}),
    );
    assert_eq!(
        game.count("hoi4_politics_options"),
        2,
        "another seat is another read"
    );
    read(day0 + DAY, seat.clone());
    assert_eq!(
        game.count("hoi4_politics_options"),
        3,
        "a new game day reads again"
    );
    // a politics order forgets the seats' answers
    SlowReads {
        inner: &game,
        cache: &cache,
        now: day0 + DAY + 1,
        on: true,
    }
    .call("hoi4_politics_choose", &json!({"request_id": "r"}))
    .unwrap();
    read(day0 + DAY + 2, seat.clone());
    assert_eq!(game.count("hoi4_politics_options"), 4);
    // the diplomacy view stands a quarter day: a declaration reads it by the hour it is due
    assert!(slow_fresh(day0, day0 + 5, DAY / 4));
    assert!(!slow_fresh(day0, day0 + DAY / 4, DAY / 4));
    // off (peace, or scheduler.slow_reads false): every read is fresh
    SlowReads {
        inner: &game,
        cache: &cache,
        now: day0 + DAY + 3,
        on: false,
    }
    .call("hoi4_politics_options", &seat)
    .unwrap();
    assert_eq!(game.count("hoi4_politics_options"), 5);
    // a memory read is not one of them
    assert!(slow_key("hoi4_states", &json!({})).is_none());
}

#[test]
fn the_watchdog_names_the_war_step_s_longest_wait_and_what_ran_instead() {
    let mut watch = WarWatch::default();
    watch.offered(START, true, DAY);
    watch.acted(START);
    watch.spent("fast factories", 3.2);
    watch.spent("fair mio_lines", 3.9);
    watch.offered(START + 10, true, DAY);
    assert_eq!(watch.over, 0);
    watch.spent("issue production", 13.1);
    watch.offered(START + 10 + 353, true, DAY);
    watch.acted(START + 10 + 353);
    let row = watch.row().unwrap();
    assert_eq!(row["kind"], "war_idle");
    assert_eq!(row["over"], 1);
    assert_eq!(row["longest_hours"], 353);
    assert_eq!(row["limit_hours"], DAY);
    assert!(
        row["instead"]
            .as_str()
            .unwrap()
            .starts_with("issue production 13.1 s"),
        "{row}"
    );
    assert_eq!(row["longest_acted_hours"], 363);
    // outside an executing operation nothing is counted
    watch.offered(START + 2000, false, DAY);
    watch.offered(START + 4000, true, DAY);
    assert_eq!(watch.row().unwrap()["over"], 1);
    // the gate reads it in lane_turns
    let mut state = State::default();
    state.sched.war_watch = watch;
    let rows = lane_turns(&state, START + 4000);
    assert!(rows
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["kind"] == "war_idle" && r["over"] == 1));
}

/// A war step whose every call is one order that takes 0.6 s.
fn slow_order_step(
    state: &mut State,
    tools: &dyn Tools,
    _plan: &Value,
    lease: &str,
    _now: GameHour,
) -> Result<Option<String>> {
    std::thread::sleep(Duration::from_millis(600));
    let n = state.seq;
    let (ok, why) = state.act(
        tools,
        lease,
        "hoi4_politics_choose",
        json!({"category": "government", "slot": n, "idea": format!("idea_{n}")}),
        "w",
    )?;
    Ok(Some(format!("order {n}: {ok} {why}")))
}

#[test]
fn a_war_burst_of_mouse_orders_gives_the_tick_back_once_half_a_game_day_has_passed() {
    // w101b (28 Nov 2026): one 84-second war burst ran 164 game hours
    let mut game = Game::new(false);
    game.step = 7;
    let mut state = State::default();
    state.last_hour = START;
    let plan = json!({"war": {"enabled": true}});
    let out = war_burst_for(&mut state, &game, &plan, "l", START, slow_order_step, 30)
        .unwrap()
        .unwrap();
    assert!(
        (2..16).contains(&game.orders()),
        "{} orders: {out}",
        game.orders()
    );
}
