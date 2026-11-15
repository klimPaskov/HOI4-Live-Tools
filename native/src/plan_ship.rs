//! Sea transfers: home divisions shipped to an overseas theatre (plan key `sea_transfer.routes`).
//! A child module of `plan_run` (declared there with `#[path]`), like plan_war.rs: every order
//! goes through `State::act` and its failure ledger; this module's memory (`State::ship`) is
//! this server life's.
//!
//! Why: a country that fights overseas starts with most of its army at home. Disbanding it
//! refills the front's equipment, but the front stays as small as the colonial garrison; the
//! game's own AI ships its home army to the theatre in the first weeks and attacks in many
//! places at once. A sea move is one order (hoi4_army_move with the target state: the game
//! plans the convoy route itself) and lands in three to four weeks.
//!
//! `sea_transfer.routes`: [{"id", "to_state", "templates":[..], "count":N, "from_state"?,
//! "join"?}], each route in plan order:
//!   templates   the division templates shipped (exact names; empty: none)
//!   count       how many divisions the route ships in all (default: every one it finds)
//!   from_state  ship only divisions standing in this state (absent: any state the capital
//!               reaches over own land outside the theatre)
//!   to_state    the state the army is ordered to (a port state of the theatre)
//!   join        an army (or a layout `name_hint`) the shipped army is merged into once every
//!               division has landed in `to_state` or the theatre; absent: the shipped army
//!               stays an army of its own, which the war step drives like the others
//! One action a tick: loose divisions of a wanted template are first formed into an army
//! (hoi4_army_assign), the wanted divisions of a mixed home army are detached into one of
//! their own (hoi4_army_detach), an army made only of wanted divisions first marches to the
//! naval base of its state (hoi4_army_move province_id) and, once every division stands in an
//! own port, is ordered to `to_state` (hoi4_army_move state_id), and a landed army is merged
//! into `join` (hoi4_army_merge). Divisions a route still wants are not disbanded (plan key
//! `disband`).
//!
//! Why the port first: from an inland province the game offers no sea transfer at all (the
//! hover reads "There is no available land route to this province." and a right click gives no
//! order); from a port province the same order sails (live 15 Nov 2026, x3: a Piedmont
//! division in province 611 never sailed, marched to its naval base 6788 it sailed at once;
//! two Sicily divisions sailed from their island and landed in Massawa in 11 days). A state
//! without a naval base, or a naval base list that cannot be read, sends the sea order as is.
//! A layout formation and an army with any division outside the home land are never shipped.
use super::{list, plan_war, strings, text, GameHour, State, Tools, DAY};
use crate::error::Result;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// The divisions an army may hold (the game's limit, as hoi4_army_merge checks it).
const ARMY_MAX: usize = 24;
/// The most divisions one hoi4_army_detach call moves.
const DETACH_MAX: usize = 18;
/// Game days a route may fail every sea move before it gives up and frees its templates for
/// the disband step (live 15 Nov 2026: moves that never took held the home divisions back
/// while the front's equipment fell to a third).
const GIVE_UP_DAYS: GameHour = 20;
/// Game days after the order when a voyage that has not landed is reported.
const LATE_DAYS: GameHour = 45;

/// One shipped army on its way.
#[derive(Debug, Clone)]
struct Voyage {
    route: usize,
    ids: Vec<String>,
    ordered: GameHour,
}

#[derive(Debug, Default)]
pub(super) struct Memory {
    /// divisions ordered to sea, per route (index in plan order)
    shipped: HashMap<usize, usize>,
    voyages: Vec<Voyage>,
    /// divisions detached to sail: never detached a second time, and their army is not folded
    /// into another by the consolidation (plan_run.rs armies_step)
    detached: HashSet<String>,
    /// the game hour of the first call: the step acts only once game time runs (a sea move
    /// given in the paused setup shows no heading and reads NotApplied, live 15 Nov 2026)
    first: Option<GameHour>,
    /// per route: the game hour of its first failed sea move since its last success
    failing: HashMap<usize, GameHour>,
}

impl Memory {
    /// The route has failed every sea move for GIVE_UP_DAYS.
    fn given_up(&self, route: usize, now: GameHour) -> bool {
        self.failing
            .get(&route)
            .is_some_and(|since| now - since >= GIVE_UP_DAYS * DAY)
    }
    /// The route ships no more: full, or given up.
    fn done(&self, route: usize, count: usize, now: GameHour) -> bool {
        self.shipped.get(&route).copied().unwrap_or(0) >= count || self.given_up(route, now)
    }
}

/// One route of `sea_transfer.routes`.
struct Route {
    templates: Vec<String>,
    count: usize,
    from_state: Option<i64>,
    to_state: i64,
    join: String,
}

fn routes(plan: &Value) -> Vec<Route> {
    let cfg = &plan["sea_transfer"];
    list(cfg, "routes")
        .iter()
        .filter_map(|r| {
            Some(Route {
                templates: strings(r, "templates"),
                count: r["count"].as_u64().map_or(usize::MAX, |n| n as usize),
                from_state: r["from_state"].as_i64(),
                to_state: r["to_state"].as_i64()?,
                join: text(&r["join"]).to_string(),
            })
        })
        .collect()
}

/// Templates a route still wants to ship and the divisions at sea: the disband step leaves both.
pub(super) fn reserved(
    state: &State,
    plan: &Value,
    now: GameHour,
) -> (HashSet<String>, HashSet<String>) {
    let mut templates = HashSet::new();
    for (i, route) in routes(plan).iter().enumerate() {
        if !state.ship.done(i, route.count, now) {
            templates.extend(route.templates.iter().cloned());
        }
    }
    let ids = state
        .ship
        .voyages
        .iter()
        .flat_map(|v| v.ids.iter().cloned())
        .collect();
    (templates, ids)
}

/// Divisions the step works on (detached to sail, or at sea): the consolidation leaves their
/// armies alone (live 15 Nov 2026: a detached division was merged back twenty times).
pub(super) fn claimed(state: &State) -> HashSet<String> {
    state
        .ship
        .voyages
        .iter()
        .flat_map(|v| v.ids.iter().cloned())
        .chain(state.ship.detached.iter().cloned())
        .collect()
}

struct Row {
    id: String,
    army: Option<String>,
    template: String,
    state: Option<i64>,
    province: Option<i64>,
}

/// Own naval-base provinces (hoi4_convoys); empty when unreadable.
fn own_ports(tools: &dyn Tools) -> HashSet<i64> {
    tools
        .call("hoi4_convoys", &json!({}))
        .map(|v| {
            list(&v, "naval_bases")
                .iter()
                .filter_map(|b| b["province"].as_i64())
                .collect()
        })
        .unwrap_or_default()
}

/// The naval-base province of `state` (its provinces from hoi4_states), when it has one.
fn state_port(tools: &dyn Tools, state: i64, ports: &HashSet<i64>) -> Option<i64> {
    let read = tools
        .call("hoi4_states", &json!({"query": state.to_string()}))
        .ok()?;
    let row = list(&read, "states")
        .iter()
        .find(|r| r["state_id"].as_i64() == Some(state))?;
    let mut found: Vec<i64> = list(row, "provinces")
        .iter()
        .filter_map(|p| p["province"].as_i64())
        .filter(|p| ports.contains(p))
        .collect();
    found.sort_unstable();
    found.first().copied()
}

/// One action of the sea-transfer routes, or None when nothing is to be done.
pub(super) fn step(
    state: &mut State,
    tools: &dyn Tools,
    plan: &Value,
    lease: &str,
    now: GameHour,
) -> Result<Option<String>> {
    let routes = routes(plan);
    if routes.is_empty() || state.skipped(now, "ship", "all") {
        return Ok(None);
    }
    if *state.ship.first.get_or_insert(now) >= now {
        return Ok(None);
    }
    let pending = state.ship.voyages.is_empty()
        && routes
            .iter()
            .enumerate()
            .all(|(i, r)| state.ship.done(i, r.count, now));
    if pending {
        return Ok(None);
    }
    let supply = tools.call("hoi4_supply", &json!({}))?;
    let rows: Vec<Row> = list(&supply, "divisions")
        .iter()
        .filter_map(|d| {
            Some(Row {
                id: d["id"].as_str()?.to_string(),
                army: d["army"].as_str().map(str::to_string),
                template: text(&d["template"]).to_string(),
                state: d["state_id"].as_i64(),
                province: d["province_id"].as_i64(),
            })
        })
        .collect();
    let theatre: HashSet<i64> = list(&plan["war"], "theatre")
        .iter()
        .filter_map(Value::as_i64)
        .collect();
    if let Some(line) = landings(state, tools, lease, now, &routes, &rows, &theatre)? {
        return Ok(Some(line));
    }
    departures(state, tools, lease, now, &routes, &rows, &theatre)
}

/// A voyage whose divisions all stand in its target state or the theatre is merged into its
/// `join` army (or simply ends); a late one is reported.
fn landings(
    state: &mut State,
    tools: &dyn Tools,
    lease: &str,
    now: GameHour,
    routes: &[Route],
    rows: &[Row],
    theatre: &HashSet<i64>,
) -> Result<Option<String>> {
    let voyages = state.ship.voyages.clone();
    for voyage in &voyages {
        let route = &routes[voyage.route.min(routes.len() - 1)];
        let mine: Vec<&Row> = rows.iter().filter(|r| voyage.ids.contains(&r.id)).collect();
        let landed = !mine.is_empty()
            && mine.iter().all(|r| {
                r.state
                    .is_some_and(|s| s == route.to_state || theatre.contains(&s))
            });
        if !landed {
            if now - voyage.ordered > LATE_DAYS * DAY {
                plan_war::set_note(
                    state,
                    "ship",
                    Some(format!(
                        "{} division(s) ordered to state {} {} game days ago have not all landed",
                        mine.len(),
                        route.to_state,
                        (now - voyage.ordered) / DAY
                    )),
                );
            }
            continue;
        }
        let source = mine.iter().find_map(|r| r.army.clone()).unwrap_or_default();
        let target = state
            .layout
            .get(&route.join)
            .cloned()
            .unwrap_or_else(|| route.join.clone());
        let first = voyage.ids.first().cloned();
        let done = |state: &mut State, line: String| {
            state
                .ship
                .voyages
                .retain(|v| v.ids.first() != first.as_ref());
            for r in &mine {
                state.ship.detached.remove(&r.id);
            }
            plan_war::set_note(state, "ship", Some(line.clone()));
            line
        };
        if route.join.is_empty() || source.is_empty() || source == target {
            let line = format!(
                "sea transfer: {} division(s) of {source} landed in state {}",
                mine.len(),
                route.to_state
            );
            done(state, line);
            continue;
        }
        let held = rows
            .iter()
            .filter(|r| r.army.as_deref() == Some(target.as_str()))
            .count();
        let alone = rows
            .iter()
            .filter(|r| r.army.as_deref() == Some(source.as_str()))
            .count();
        if held == 0 || held + alone > ARMY_MAX {
            let line = format!(
                "sea transfer: {source} landed in state {}; not merged into {target} ({held} + {alone} divisions)",
                route.to_state
            );
            done(state, line);
            continue;
        }
        if state.skipped(now, "ship", &format!("merge:{source}")) {
            continue;
        }
        let (ok, why) = state.act(
            tools,
            lease,
            "hoi4_army_merge",
            json!({"source": source, "target": target}),
            "sm",
        )?;
        if ok {
            done(
                state,
                format!("sea transfer: {source} landed and joined {target}"),
            );
        } else {
            state.skip(now, "ship", &format!("merge:{source}"), 2 * DAY);
        }
        return Ok(Some(format!(
            "sea transfer: merge {source} into {target}: {ok} {why}"
        )));
    }
    Ok(None)
}

/// The next order that brings a route's divisions to sea: form, detach or ship.
fn departures(
    state: &mut State,
    tools: &dyn Tools,
    lease: &str,
    now: GameHour,
    routes: &[Route],
    rows: &[Row],
    theatre: &HashSet<i64>,
) -> Result<Option<String>> {
    let Some(home) = plan_war::home_land(state, tools) else {
        state.skip(now, "ship", "all", 7 * DAY);
        return Ok(None);
    };
    let at_sea: HashSet<String> = state
        .ship
        .voyages
        .iter()
        .flat_map(|v| v.ids.iter().cloned())
        .collect();
    let formations: HashSet<String> = state.layout.values().cloned().collect();
    // own naval bases, read once and only when an army is ready to sail
    let mut ports: Option<HashSet<i64>> = None;
    for (i, route) in routes.iter().enumerate() {
        let shipped = state.ship.shipped.get(&i).copied().unwrap_or(0);
        if state.ship.done(i, route.count, now) || route.templates.is_empty() {
            continue;
        }
        let left = route.count - shipped;
        let at_home = |r: &Row| {
            r.state.is_some_and(|s| {
                home.contains(&s)
                    && !theatre.contains(&s)
                    && s != route.to_state
                    && route.from_state.is_none_or(|f| f == s)
            })
        };
        let wanted = |r: &Row| route.templates.contains(&r.template) && !at_sea.contains(&r.id);
        // Armies holding wanted divisions at home, the most wanted first.
        let mut armies: Vec<(String, Vec<&Row>)> = Vec::new();
        for r in rows.iter().filter(|r| wanted(r) && at_home(r)) {
            let Some(army) = &r.army else { continue };
            if formations.contains(army) {
                continue;
            }
            match armies.iter_mut().find(|(a, _)| a == army) {
                Some((_, v)) => v.push(r),
                None => armies.push((army.clone(), vec![r])),
            }
        }
        armies.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
        for (army, want) in &armies {
            if state.skipped(now, "ship", army) {
                continue;
            }
            let all: Vec<&Row> = rows
                .iter()
                .filter(|r| r.army.as_deref() == Some(army.as_str()))
                .collect();
            // an army with a division abroad or one already ordered to sea is not a home army
            if all.iter().any(|r| !at_home(r) || at_sea.contains(&r.id)) {
                continue;
            }
            if all.len() == want.len() && want.len() <= left {
                // Every division in an own port first: from inland the game offers no sea move.
                let ports = ports.get_or_insert_with(|| own_ports(tools));
                let ashore = all
                    .iter()
                    .find(|r| r.province.is_none_or(|p| !ports.contains(&p)));
                if let Some(at) = ashore.and_then(|r| r.state) {
                    if let Some(port) = state_port(tools, at, ports) {
                        let (ok, why) = state.act(
                            tools,
                            lease,
                            "hoi4_army_move",
                            json!({"army": army, "state_id": at, "province_id": port}),
                            "sp",
                        )?;
                        // the march takes days: the army is looked at again after one
                        state.skip(now, "ship", army, if ok { DAY } else { 3 * DAY });
                        return Ok(Some(format!(
                            "sea transfer: {army} marches to its port {port} in state {at}: {ok} {why}"
                        )));
                    }
                }
                let (ok, why) = state.act(
                    tools,
                    lease,
                    "hoi4_army_move",
                    json!({"army": army, "state_id": route.to_state}),
                    "sh",
                )?;
                if ok {
                    state.ship.failing.remove(&i);
                    *state.ship.shipped.entry(i).or_insert(0) += want.len();
                    state.ship.voyages.push(Voyage {
                        route: i,
                        ids: want.iter().map(|r| r.id.clone()).collect(),
                        ordered: now,
                    });
                } else {
                    state.ship.failing.entry(i).or_insert(now);
                    state.skip(now, "ship", army, 3 * DAY);
                    if state.ship.given_up(i, now) {
                        plan_war::set_note(
                            state,
                            "ship",
                            Some(format!(
                                "sea transfer route {} gave up: no sea move to state {} took for {GIVE_UP_DAYS} game days; its divisions may be disbanded",
                                i + 1,
                                route.to_state
                            )),
                        );
                    }
                }
                return Ok(Some(format!(
                    "sea transfer: {army} ({} division(s)) -> state {}: {ok} {why}",
                    want.len(),
                    route.to_state
                )));
            }
            // A mixed army (or more than the route still wants): its wanted divisions become an
            // army of their own, never the whole army (hoi4_army_detach cannot empty one).
            let take = left.min(DETACH_MAX).min(if all.len() == want.len() {
                want.len() - 1
            } else {
                want.len()
            });
            if take == 0 {
                continue;
            }
            // a division detached once that stands in a mixed army again was folded back by
            // another step: it stays (no detach loop)
            let ids: Vec<&str> = want
                .iter()
                .filter(|r| !state.ship.detached.contains(&r.id))
                .take(take)
                .map(|r| r.id.as_str())
                .collect();
            if ids.is_empty() {
                continue;
            }
            let take = ids.len();
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_army_detach",
                json!({"army": army, "divisions": ids}),
                "sd",
            )?;
            if ok {
                state
                    .ship
                    .detached
                    .extend(ids.iter().map(|id| id.to_string()));
            } else {
                state.skip(now, "ship", army, 3 * DAY);
            }
            return Ok(Some(format!(
                "sea transfer: {take} division(s) out of {army} to sail: {ok} {why}"
            )));
        }
        // Loose wanted divisions at home are formed into an army first (all loose divisions of
        // that state; the next tick detaches the wanted ones when others came along).
        let mut loose: HashMap<i64, usize> = HashMap::new();
        for r in rows
            .iter()
            .filter(|r| r.army.is_none() && wanted(r) && at_home(r))
        {
            *loose.entry(r.state.unwrap_or(0)).or_insert(0) += 1;
        }
        let mut loose: Vec<(i64, usize)> = loose.into_iter().collect();
        loose.sort_by_key(|(s, n)| (std::cmp::Reverse(*n), *s));
        for (at, n) in loose {
            let key = format!("loose:{at}");
            if state.skipped(now, "ship", &key) {
                continue;
            }
            let (ok, why) = state.act(
                tools,
                lease,
                "hoi4_army_assign",
                json!({"state_id": at}),
                "sa",
            )?;
            state.skip(now, "ship", &key, if ok { DAY } else { 5 * DAY });
            return Ok(Some(format!(
                "sea transfer: {n} loose division(s) in state {at} formed into an army to sail: {ok} {why}"
            )));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const START: GameHour = 1_000_000;

    /// A country with its capital land (states 1, 2), a theatre port state 9 overseas and
    /// division rows the test changes between ticks.
    struct Game {
        rows: RefCell<Value>,
        calls: RefCell<Vec<(String, Value)>>,
    }
    impl Game {
        fn new(rows: Value) -> Self {
            Game {
                rows: RefCell::new(rows),
                calls: RefCell::new(vec![]),
            }
        }
        fn sent(&self, tool: &str) -> Vec<Value> {
            self.calls
                .borrow()
                .iter()
                .filter(|(n, _)| n == tool)
                .map(|(_, a)| a.clone())
                .collect()
        }
    }
    impl Tools for Game {
        fn call(&self, name: &str, args: &Value) -> Result<Value> {
            self.calls
                .borrow_mut()
                .push((name.to_string(), args.clone()));
            match name {
                "hoi4_wars" => Ok(json!({"capital_land": {"states": [1, 2]}})),
                "hoi4_supply" => Ok(json!({"divisions": self.rows.borrow().clone()})),
                _ if args.get("request_id").is_some() => {
                    Ok(json!({"receipt": {"status": "Verified"}}))
                }
                _ => Ok(json!({})),
            }
        }
    }

    fn div(id: &str, army: Option<&str>, template: &str, state: Option<i64>) -> Value {
        json!({"id": id, "army": army, "template": template, "state_id": state})
    }

    fn plan(count: u64) -> Value {
        json!({"war": {"theatre": [9]},
            "sea_transfer": {"routes": [{"id": "r1", "to_state": 9, "count": count,
                "templates": ["Infantry"], "join": "Strike"}]}})
    }

    #[test]
    fn wanted_divisions_of_a_mixed_home_army_sail_as_their_own_army_and_join_on_landing() {
        let game = Game::new(json!([
            div("a", Some("Army 5"), "Infantry", Some(1)),
            div("b", Some("Army 5"), "Infantry", Some(1)),
            div("c", Some("Army 5"), "Cavalry", Some(1)),
            div("s1", Some("North"), "Infantry", Some(9)),
        ]));
        let plan = plan(2);
        let mut state = State::default();
        // the paused setup: no order before game time runs
        assert_eq!(
            step(&mut state, &game, &plan, "l", START - 1).unwrap(),
            None
        );
        assert_eq!(
            step(&mut state, &game, &plan, "l", START - 1).unwrap(),
            None
        );
        state.layout.insert("Strike".into(), "North".into());
        // the cavalry stays: the infantry is detached into an army of its own
        let line = step(&mut state, &game, &plan, "l", START).unwrap().unwrap();
        assert!(line.contains("2 division(s) out of Army 5"), "{line}");
        assert_eq!(
            game.sent("hoi4_army_detach")[0]["divisions"],
            json!(["a", "b"])
        );
        // the disband step leaves the route's template alone while the route still wants it
        let (templates, _) = reserved(&state, &plan, START + 1);
        assert!(templates.contains("Infantry"));
        *game.rows.borrow_mut() = json!([
            div("a", Some("Army 6"), "Infantry", Some(1)),
            div("b", Some("Army 6"), "Infantry", Some(2)),
            div("c", Some("Army 5"), "Cavalry", Some(1)),
            div("s1", Some("North"), "Infantry", Some(9)),
        ]);
        let line = step(&mut state, &game, &plan, "l", START + 1)
            .unwrap()
            .unwrap();
        assert!(
            line.starts_with("sea transfer: Army 6 (2 division(s)) -> state 9: true"),
            "{line}"
        );
        assert_eq!(
            game.sent("hoi4_army_move")[0],
            json!({"army": "Army 6", "state_id": 9, "request_id": game.sent("hoi4_army_move")[0]["request_id"], "lease_token": "l"})
        );
        // the route is full: its template is free for the disband step; the sailors are not
        let (templates, sailing) = reserved(&state, &plan, START + 1);
        assert!(templates.is_empty());
        assert!(sailing.contains("a") && sailing.contains("b"));
        // at sea (no state): nothing to do
        *game.rows.borrow_mut() = json!([
            div("a", Some("Army 6"), "Infantry", None),
            div("b", Some("Army 6"), "Infantry", None),
            div("s1", Some("North"), "Infantry", Some(9)),
        ]);
        assert_eq!(
            step(&mut state, &game, &plan, "l", START + 5 * DAY).unwrap(),
            None
        );
        // landed: merged into the layout formation the route names
        *game.rows.borrow_mut() = json!([
            div("a", Some("Army 6"), "Infantry", Some(9)),
            div("b", Some("Army 6"), "Infantry", Some(9)),
            div("s1", Some("North"), "Infantry", Some(9)),
        ]);
        let line = step(&mut state, &game, &plan, "l", START + 20 * DAY)
            .unwrap()
            .unwrap();
        assert!(line.contains("merge Army 6 into North: true"), "{line}");
        assert_eq!(state.ship.voyages.len(), 0);
        // done: no more orders
        assert_eq!(
            step(&mut state, &game, &plan, "l", START + 21 * DAY).unwrap(),
            None
        );
        assert_eq!(game.sent("hoi4_army_move").len(), 1);
    }

    #[test]
    fn loose_home_divisions_are_formed_first_and_formations_or_armies_abroad_never_sail() {
        let game = Game::new(json!([
            div("f1", Some("North"), "Infantry", Some(1)),
            div("x1", Some("Army 3"), "Infantry", Some(1)),
            div("x2", Some("Army 3"), "Infantry", Some(9)),
            div("l1", None, "Infantry", Some(2)),
            div("l2", None, "Infantry", Some(2)),
        ]));
        let plan = plan(10);
        let mut state = State::default();
        // the paused setup: no order before game time runs
        assert_eq!(
            step(&mut state, &game, &plan, "l", START - 1).unwrap(),
            None
        );
        assert_eq!(
            step(&mut state, &game, &plan, "l", START - 1).unwrap(),
            None
        );
        state.layout.insert("Strike".into(), "North".into());
        let line = step(&mut state, &game, &plan, "l", START).unwrap().unwrap();
        assert!(
            line.contains("2 loose division(s) in state 2 formed"),
            "{line}"
        );
        assert_eq!(game.sent("hoi4_army_assign")[0]["state_id"], json!(2));
        assert!(game.sent("hoi4_army_move").is_empty());
        assert!(game.sent("hoi4_army_detach").is_empty());
    }

    #[test]
    fn a_route_without_join_ends_its_voyage_on_landing_and_a_late_one_is_reported() {
        let game = Game::new(json!([div("a", Some("Army 7"), "Infantry", Some(1))]));
        let plan = json!({"war": {"theatre": [9]},
            "sea_transfer": {"routes": [{"id": "r", "to_state": 9, "templates": ["Infantry"]}]}});
        let mut state = State::default();
        // the paused setup: no order before game time runs
        assert_eq!(
            step(&mut state, &game, &plan, "l", START - 1).unwrap(),
            None
        );
        assert_eq!(
            step(&mut state, &game, &plan, "l", START - 1).unwrap(),
            None
        );
        let line = step(&mut state, &game, &plan, "l", START).unwrap().unwrap();
        assert!(line.contains("Army 7 (1 division(s)) -> state 9"), "{line}");
        *game.rows.borrow_mut() = json!([div("a", Some("Army 7"), "Infantry", None)]);
        assert_eq!(
            step(&mut state, &game, &plan, "l", START + 50 * DAY).unwrap(),
            None
        );
        assert!(plan_war::digest(&state).unwrap()["ship"]
            .as_str()
            .unwrap()
            .contains("have not all landed"));
        *game.rows.borrow_mut() = json!([div("a", Some("Army 7"), "Infantry", Some(9))]);
        assert_eq!(
            step(&mut state, &game, &plan, "l", START + 51 * DAY).unwrap(),
            None
        );
        assert!(state.ship.voyages.is_empty());
    }

    #[test]
    fn a_detached_division_folded_back_into_a_mixed_army_is_not_detached_again() {
        let game = Game::new(json!([
            div("a", Some("Army 2"), "Infantry", Some(1)),
            div("c", Some("Army 2"), "Cavalry", Some(1)),
        ]));
        let plan = plan(1);
        let mut state = State::default();
        assert_eq!(
            step(&mut state, &game, &plan, "l", START - 1).unwrap(),
            None
        );
        let line = step(&mut state, &game, &plan, "l", START).unwrap().unwrap();
        assert!(line.contains("1 division(s) out of Army 2"), "{line}");
        assert!(claimed(&state).contains("a"));
        // another step merged it back: the step leaves it
        assert_eq!(
            step(&mut state, &game, &plan, "l", START + DAY).unwrap(),
            None
        );
        assert_eq!(game.sent("hoi4_army_detach").len(), 1);
    }

    #[test]
    fn a_route_whose_sea_moves_never_take_gives_up_and_frees_its_template() {
        struct Refuse(Game);
        impl Tools for Refuse {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                if name == "hoi4_army_move" {
                    return Ok(json!({"receipt": {"status": "NotApplied"}}));
                }
                self.0.call(name, args)
            }
        }
        let game = Refuse(Game::new(json!([div(
            "a",
            Some("Army 7"),
            "Infantry",
            Some(1)
        )])));
        let plan = plan(1);
        let mut state = State::default();
        assert_eq!(
            step(&mut state, &game, &plan, "l", START - 1).unwrap(),
            None
        );
        let line = step(&mut state, &game, &plan, "l", START).unwrap().unwrap();
        assert!(line.contains("-> state 9: false"), "{line}");
        assert!(reserved(&state, &plan, START + DAY).0.contains("Infantry"));
        // a retry after the skip fails again; after GIVE_UP_DAYS the route frees its template
        let line = step(&mut state, &game, &plan, "l", START + 4 * DAY)
            .unwrap()
            .unwrap();
        assert!(line.contains("false"), "{line}");
        assert!(reserved(&state, &plan, START + 21 * DAY).0.is_empty());
        assert_eq!(
            step(&mut state, &game, &plan, "l", START + 25 * DAY).unwrap(),
            None
        );
    }

    #[test]
    fn an_army_inland_marches_to_its_state_port_before_the_sea_order() {
        // State 1 has its naval base in province 50; state 2 has no port.
        struct Ported(Game);
        impl Tools for Ported {
            fn call(&self, name: &str, args: &Value) -> Result<Value> {
                match name {
                    "hoi4_convoys" => {
                        self.0.call(name, args)?;
                        Ok(json!({"naval_bases": [{"province": 50, "level": 3}, {"province": 70}]}))
                    }
                    "hoi4_states" => {
                        self.0.call(name, args)?;
                        Ok(json!({"states": [{"state_id": 1, "provinces": [
                            {"province": 40}, {"province": 50}]}]}))
                    }
                    _ => self.0.call(name, args),
                }
            }
        }
        let at = |id: &str, army: &str, state: i64, province: i64| {
            json!({"id": id, "army": army, "template": "Infantry", "state_id": state,
                "province_id": province})
        };
        let game = Ported(Game::new(json!([
            at("a", "Army 6", 1, 40),
            at("b", "Army 6", 1, 50)
        ])));
        let plan = plan(2);
        let mut state = State::default();
        assert_eq!(
            step(&mut state, &game, &plan, "l", START - 1).unwrap(),
            None
        );
        // one division stands inland: the army marches to the port of its state
        let line = step(&mut state, &game, &plan, "l", START).unwrap().unwrap();
        assert!(
            line.contains("Army 6 marches to its port 50 in state 1: true"),
            "{line}"
        );
        let moves = game.0.sent("hoi4_army_move");
        assert_eq!(moves.len(), 1);
        assert_eq!(moves[0]["province_id"], 50);
        assert_eq!(moves[0]["state_id"], 1);
        // the march takes days: nothing while the army is skipped
        assert_eq!(
            step(&mut state, &game, &plan, "l", START + 1).unwrap(),
            None
        );
        // both divisions in a port: the sea order to the theatre
        *game.0.rows.borrow_mut() = json!([at("a", "Army 6", 1, 50), at("b", "Army 6", 1, 50)]);
        let line = step(&mut state, &game, &plan, "l", START + DAY + 1)
            .unwrap()
            .unwrap();
        assert!(
            line.starts_with("sea transfer: Army 6 (2 division(s)) -> state 9: true"),
            "{line}"
        );
        assert_eq!(game.0.sent("hoi4_army_move")[1]["state_id"], 9);
        assert!(game.0.sent("hoi4_army_move")[1]
            .get("province_id")
            .is_none());

        // a state without a naval base: the sea order goes out as is
        let game = Ported(Game::new(json!([at("c", "Army 8", 2, 41)])));
        let one = super::tests::plan(1);
        let mut state = State::default();
        step(&mut state, &game, &one, "l", START - 1).unwrap();
        let line = step(&mut state, &game, &one, "l", START).unwrap().unwrap();
        assert!(line.contains("Army 8 (1 division(s)) -> state 9"), "{line}");
    }
}
