---
name: hoi4-live-campaign
description: Play a Hearts of Iron IV campaign as any country through the HOI4 Live Tools MCP server (hoi4_* tools): the advance-brief-act loop, which tool answers which situation, popup rules, military basics, journal discipline and the pitfalls found live. Use when asked to play HOI4, run a country for months or years, or test how a mod behaves over a campaign.
---

# Playing HOI4 with HOI4 Live Tools

The server drives the real game through its interface. Campaign mode refuses console cheats, so everything you do is
something a player could do. The game is shared with other sessions: take the desktop lock (`hoi4_lock
action=acquire`) for the run and release it at the end.

## Start

1. `hoi4_status` (is the game running, which playset, which mode). `hoi4_launch` if it is not; `hoi4_load` a save or
   `hoi4_new_game` (bookmark and country by name) to start.
2. `hoi4_lock action=set_mode mode=campaign` once the position is set up (test mode keeps the console for set-up).
3. `hoi4_journal action=add kind=plan text=...`: what this campaign is for and the first priorities.
4. `hoi4_save name=start` then `hoi4_briefing save=hoi4live_start`: the deep state (focus, research, production,
   construction, laws) comes from a save, so save before every briefing that must be current.

## The loop

1. **Brief**: `hoi4_save`, `hoi4_briefing`. Read "Needs attention"; each item has a `code` in the data.
2. **Act** on each item with the campaign tools (table below). Write each choice and its reason with
   `hoi4_journal action=add kind=decision`. The server writes every verified action to the journal itself.
3. **Advance**: `hoi4_advance days=30`. Time runs in two-day chunks and stops on the first decision point
   (`stopped_by`): a technology finished, a focus completed, free factories, a war started or ended, states lost,
   a losing war, low stability, an event that needs an answer. The result carries `suggestions`: the tool call that
   answers each attention item.
4. Repeat. When something surprising happened, `hoi4_journal kind=lesson`. At the end `hoi4_journal action=report`.

Choose `stop_on` for the situation: in a quiet peace, `["war_started", "losing", "focus_done"]` and `days=90`; in a war
add `territory_lost` and `territory_gained`. `stop_on=[]` runs to the date. The result's `changes` shows what moved.

## Which tool answers what

| attention code | tool | notes |
|---|---|---|
| `no_focus` | `hoi4_focus` list, then `action=select focus=<id>` | costs 1 political power a day while running; check `requires` |
| `idle_research` | `hoi4_research` `slots`, `list ahead=false`, `start tech=<id>` | one technology per folder keeps bonuses spread |
| `idle_construction` | `hoi4_construction` `queue`, `build building=industrial_complex state=<name>` | a full state answers `no_slot`: try the next |
| `idle_production`, `equipment_short` | `hoi4_production` `lines`, `add equipment=...`, `assign line=.. factories=..` | equipment the divisions lack first |
| `spend_pp`, `low_stability` | `hoi4_politics` `options slot=...`, `hire`, `change_law`; `hoi4_decisions` | advisors cost about 150 political power |
| `resource_deficit` | `hoi4_diplomacy action=trade` | trade needs civilian factories |
| `losing_war`, `lost_states`, `front` | `hoi4_army`, `hoi4_orders` | see Military |
| `popups` | `hoi4_popups action=read`, then `answer` | |

## Events and popups

`hoi4_advance popups=choices` (the default) answers news and one-option events by itself and stops at an event with
a real choice, returning its text and options in `events_waiting`. Read it, decide, answer with `hoi4_popups
action=answer option=<n>` or `option_text=<regex>`, journal the reason, advance again. To decide families of events
in advance, pass `popup_rules`: `[{"match": "Earhart", "option": "^No"}, {"match": "concession", "stop": true}]`
(regex on title and text; the first match decides; `option` is an index or a regex on the option text).
`popups=first` answers everything with the first option (for tests), `stop` leaves everything to you.

## Military

- Templates: `hoi4_army action=templates`, `design template=... duplicate=true new_name=... add=[{"type":
  "artillery_brigade","column":4}]` (costs army experience).
- Forces: `recruit template=... count=N state=<where to deploy>`, later `deploy template=...`. Equipment limits
  what trains: check `hoi4_production` first.
- Armies: `create_army state=<where the divisions stand> name=... general=...`, `assign`, `general`, `army_group`.
- Orders (an army's battle plan): `hoi4_orders action=front army=... target=<state held by the enemy>` draws the
  front line against that country; `offensive` or `spearhead` through a named state; `execute`; `hold`; `status`
  (battle progress, war score, captured provinces).
- Navy and air: `hoi4_navy_air naval_mission fleet=... mission=patrol region=... near=<coastal state>`,
  `air_mission base=<state> mission=air_superiority`.
- Declare war through `hoi4_diplomacy action=act do=declare_war confirm_war=true` only when the plan says so.

## Running a long campaign fast (version 1.1)

- Switch the cheap modes on once: `hoi4_lock action=set_pace pace=fast`, `action=set_images images=errors`, and
  `action=record path=<calls.jsonl>` when the run is filmed.
- Declare routine as standing orders (`hoi4_standing`): a research queue on `research_idle`, a focus queue on
  `focus_done` and `focus_idle` (use `if_idle=true` and `on_fail=skip`), construction plans on `construction_done`, a
  production `fill` on `production_idle`. Check `hoi4_standing list` after a stop: an empty queue or a failed entry
  stops the run with the reason.
- Run time with `hoi4_advance background=true`, act while it runs, `hoi4_clock action=join` for the result. The clock
  stops by itself for a real event, a war, lost states, a peace conference (`hoi4_peace`) and `pp_high`.
- Read first, then act: `hoi4_briefing`, `hoi4_research list` (the `offscreen` flag marks tiles the tool cannot click),
  `hoi4_focus list status=available`. A focus whose Start button is disabled says so (`hoi4_focus details`).
- Wars: air missions and equipment first (the Polish war stalled four months for lack of both), then
  `hoi4_orders batch` with a state at the border and a target beyond it for every army. Name states by their in-game
  names; where the Find view fails, give another state of the same country (`via`).
- Keep a western army group on the Rhine before declaring on Poland.

## Habits that worked

- Reuse the running game and the loaded position. `hoi4_status` first; load a save only to start or to change the
  position, never to repeat a check. A load costs one to three minutes and a save about one, and each pushes older
  saves further down the load list, so one save per briefing is the budget. When one step of a longer sequence
  fails, repeat that step alone on the running game (fix, call it again) instead of restarting the sequence; run the
  whole sequence again only when the step is about persistence (what a save shows).
- Test runs: `scripts/exit_check_phase4.py --reuse-game` plays on the running game without load, setup or restore.
- Names are the game's localised names (`Coahuila`, `Battle Force`); the server finds places through the map's Find
  view and fleets and armies by their cards, so spelling matters.
- After a long advance, brief again before acting: political power, factories and slots change.
- A busy machine makes the interface slow; the tools wait and re-read, so a slow call is not a failed one. Repeat a
  failed call once before changing the plan.
- Never touch the game window yourself while a tool runs; release the lock when you stop.
- Keep the journal honest: failed attempts and why (`kind=lesson`) are what the next session needs.
