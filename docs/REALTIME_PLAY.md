# Real-time play: architecture and performance record

Goal: an agent plays HOI4 at speed 3 or faster alongside human players, without falling behind.

This file records **why** each speed change exists, with the measurement that motivated it, so later work can tell a necessary step from an incidental one.

## The measured problem (Italy–Ethiopia runs w15–w17, November 2026)

At speed 3 one game day took about 2.5–3 s.

| What | Measured |
|---|---|
| Model reasoning per player turn | p50 3.5–3.9 s (from the player transcripts) |
| Tool wait per player turn, as the agent saw it | p50 5–21 s, p95 59–90 s |
| The single request lane | busy 75% of the wall time; `hoi4_plan_run` alone 63% |
| Player turns that started right after a plan run released the lane | 53–57% |
| Reads, server side | 0–30 ms |
| Map actions (front, move, commander, air) | 3–5 s each, about 70% `nanosleep` in a profile |
| Verified actions | about 2 per wall minute |
| Plan-runner repeats | the same "stalled, move" order sent 18 times in one run |

The model was not the bottleneck. Queueing behind one serial lane and slow, sleep-bound actions were.

## The design

```
game memory --> WATCHER (own situation sampled every 0.3-2 s -> typed changes, sequence numbers)
                  |                                   |
              READ LANE (threads)                 INPUT LANE (one worker, ordered)
              every lease-free tool call:         actions; when idle: one EXECUTOR step
              reads, hoi4_observe,                (the standing plan, at most one action),
              hoi4_wait_change                    so a request waits at most one step
```

## Changes and the reason for each

| Change | Where | Why (evidence) |
|---|---|---|
| Two lanes: lease-free tool calls answer on their own threads, actions keep one ordered worker; replies out of order by JSON-RPC id. `HOI4_SERIAL=1` restores one lane | `framing::serve_lanes`, `main.rs` | A 1 ms read waited up to 80 s behind a plan run (w17) |
| Concurrent broker: a thread per connection, replies routed by id; logs `q_ms`, `cli_ms`, `bytes` | harness `macplay.py` | The broker accepted one connection at a time and started its timer after `accept()`, so queueing was invisible |
| Change feed: `hoi4_observe {since}` and `hoi4_wait_change {since, kinds, timeout}`. `HOI4_WATCH=0` turns it off | `watch.rs`, watcher thread in `main.rs` | The agent found changes only on its next poll, 5–10 game days late, by re-reading 10–60 KB |
| Resident executor: `hoi4_autopilot {enabled}`. The input lane runs one standing-plan step when no request waits; its actions appear in the feed | `autopilot.rs`, `serve_lanes_idle` | The outside plan loop held the only lane 63% of the time |
| Army claims: an army the player orders directly is left to the player for `war.player_claim_seconds` (300 s) | `autopilot.rs`, `plan_run::war_step` | Two deciders must never fight over one army |
| Stall-move backoff: each move that leaves the army in place doubles the wait before the next | `plan_run::war_step` | 18 repeats of one move in w17 scattered the army |
| `hoi4_army_attack`: front, offensive and execute in one call (three journalled steps, one panel reset and zoom restore) | `launch/macos.rs` | A war order took three calls, each with its own setup |
| `hoi4_supply {summary}` / `{army}`; the broker defaults the player's calls to summary | `launch/macos.rs`, `macplay.py` | 44–66 KB per supply read, about 1.3 MB of tool output per run |
| `batch` output shrinks lists but stays valid JSON | `macplay.py` | The 700-character cut broke JSON mid-object |
| A repeated successful action returns its stored result | `macplay.py` | The `already_done` error cost the player a turn |
| The preflight window list is reused for 150 ms (identity check only; the occlusion check stays fresh) | `platform/macos_input.rs` | About 10% of a map action went to window-server round trips |
| Journal quarantine by domain: an unresolved action blocks new input only in its own domain (army, air, navy, research, focus, construction, production, politics, diplomacy, intelligence). The clock, events, the peace screens and unknown kinds still block everything | `journal::domain_of`, `input_gate::check_in` | One uncertain dispatch used to stop all input for a run (w11). Approved by the maintainer, 8 Nov 2026 |
| Camera-still waits, the Find view skipped when the point is visible, 35/35/25 ms clicks and 25/25 ms keys, a plan run that never sleeps holding the lane, `timing_ms`, `HOI4_TRACE` | commit `a35782c` | Fixed sleeps were most of each action |

## Not changed, on purpose

- **Sleeps that guard against lost clicks** (400 ms after opening the air overview, 250 ms after selecting a wing or an army for a panel, 600 ms between repeated construction clicks). Each was added after a click the game dropped.
- **Reads.** They are already 0–30 ms, so caching them or porting them to faster code gains nothing.

## How an agent should play in real time

1. Turn the executor on with your standing plan: `hoi4_plan` set, then `hoi4_autopilot {enabled:true}`. Routine work (events, research, focus, construction, production, war rules) continues between your turns.
2. Loop:
   - call `hoi4_wait_change {since: seq, urgent_only: true, timeout_seconds: 10}`;
   - decide;
   - send one `batch` of orders (`hoi4_army_attack`, `hoi4_army_move`, ...);
   - repeat with the new `seq`.

   No sitrep ritual and no large reads: use `hoi4_supply {summary:true}` or `{army}`.
3. Armies you order yourself are yours for 5 minutes. Hand them back with `hoi4_autopilot {release:[...]}`.

## Benchmarks and acceptance criteria

The harness's `bench.py RUN LABEL` measures:
- reads at rest;
- reads while actions run;
- repeated actions;

and reports p50/p95 for the server and the round trip.

Acceptance at speed 3:

| Criterion | Target |
|---|---|
| Read p95 while an action runs | ≤ 100 ms |
| Common action p50 / p95 | ≤ 1.5 s / ≤ 3 s |
| Urgent change to agent awareness | ≤ 1 game day |
| Agent decision cycle | ≤ 6–8 s |
| Sustained verified actions | ≥ 10 per minute |
| Failures | ≤ 2% |
| Campaign | Italy–Ethiopia won before mid-June 1936 in 2 of 3 runs at speed 3, never paused |

The full research report (both the main investigation and an independent review) is kept with the run notes.

## Live measurements (8 November 2026, Germany 1936, speed 2-3, same game)

| Case | Baseline (dc0a55d + overlay fix) | New |
|---|---|---|
| Read (`hoi4_overview`) while actions run, round trip p50 / p95 | 187 / 484 ms | **1 / 1 ms** |
| `hoi4_production_set` p50 / p95 | 444 / 739 ms | 419 / 455 ms |
| Player action while the executor runs (round trip) | (no executor) | 594–738 ms, at most one executor step of waiting |
| Change feed: event opened to `hoi4_wait_change` reply | (no feed) | woke on the same 300 ms sample |
| Event report (`hoi4_events`: title, description, options) | title and options | adds full description and leader, 0.7 ms |
| Executor with an action it cannot complete | (no executor) | 346 attempts in 50 steps, now 4 in 40 s (backoff) |

### Map and panel actions (8 November 2026, Italy 1936-37, same game; server time per call)

| Action | w17 / baseline build | Now | What changed |
|---|---|---|---|
| `hoi4_event_choose` (plain, news, leader) | 345-370 ms, news and leader by screen scan | 250-590 ms, no capture for any layout | Option centres from the options grid's screen transform (validated against the layout's anchor sprite) |
| `hoi4_army_move` | 3.0-3.4 s | 0.6 s when the target shows, 1.4-2.0 s otherwise | Right-click a projected province of the target; Find view only when none shows |
| `hoi4_army_front` | 3.75-4.9 s | 2.9-3.2 s | Find only when the camera is too high or the border point is off screen; no 300 ms panel wait; highlight captured when drawn |
| `hoi4_army_offensive` | 4.0-4.9 s | 1.9 s | Drawn at the state's projected centre, Find as fallback |
| `hoi4_army_attack` (offensive + execute) | never ran (sub-request ids refused) | 2.3 s | `<id>:front` ids |
| `hoi4_railway_build` | 11.6-13.6 s | 1.9 s (both ends on screen), 4.7 s (from far) | One Find and one zoom burst instead of the arrow-key crawl |
| `hoi4_construction_add` | 3.6 s | 1.2 s on screen, 3.2 s otherwise | Placement where the camera is; no 350 ms after Find |
| `hoi4_production_set` | 0.7-1.75 s | 0.08-0.4 s | Empty notice check is one read (was a 60 ms settle before nearly every action) |
| `hoi4_research_start` | 1.5-1.8 s | about 1.5 s | (the research panel's slide-in dominates) |
| `hoi4_air_mission` | 5-8 s, some false successes | 3.9-4.3 s; 25-80 ms when already assigned | Region-checked readback, no Find on entering air mode, clicks off the base icons |
| `hoi4_army_create` | 39.9 s (10 + 18 divisions) | 4.7 s (2 divisions), 10.5 s (5 + 6) | Stops at the dialog or the new army instead of a fixed 500 ms |
| `hoi4_focus_start` | 1.7 s | 0.84 s | (notice check, panel waits) |

The Find view costs about 1.5 s by nature: the game glides the camera to the state in an eased
movement of about a second (sampled live: still settling 1.8 s after the key), which is why every
order first tries to aim where the camera already is.

Army orders now leave the camera at unit level (the maintainer watches the units); the strategic
view returns before the next non-army action. Front and offensive aim without the Find view only at
camera height 760 or below: the game draws no highlighted fronts at 932.

### The executor at speed 3 (Italy, August-December 1937)

| Run | Actions | Failures | Notes |
|---|---|---|---|
| 3 min, before the liberation fix | 31 | 2 refusals (an army assignment whose box selection was not exact; nothing done) | the same liberation move sent 13 times |
| 2.5 min, after it | 13 | 0 | no repeats; research, focus, production, events, war rest/resume, offensive and execute |

The change feed's `hoi4_wait_change` answered with the new changes in a p50 of 1.8 s round trip
(10 s timeout, woken by the first change).

Open: moving a fighter wing already assigned elsewhere into the region of its own base did not take
(three tries); the commander list sometimes closes while scrolling (the retry recovers, about 3 s).

## Status (8 November 2026)

Everything above passes the unit tests (2,713). Live: two lanes, change feed, executor, event reports, the overlay rule and the map and panel actions in the table are validated.
