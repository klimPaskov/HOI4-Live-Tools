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

- **The remaining post-success sleeps** (army selection, scroll bursts, the Find view stages). They need live tuning against misclicks.
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

## Status (8 November 2026)

Everything above passes the unit tests (2,705). Live validation and the before/after benchmark are pending: the session that built them had no input or screen-capture permission on the test Mac.
