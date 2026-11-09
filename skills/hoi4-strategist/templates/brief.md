# Brief: <Country> — <goal> (plan v<N>)

<On a revision, first line: "Revised v<N>: <one-line summary of what changed>".>

**Win.** <predicates in one line, with ids>, before <deadline>.
Check: `<exact read call, e.g. hoi4_states {"query":"<ENEMY>"}>` — <what the answer must show>.

**Next checkpoints.**
1. <date>: <must be true> — if not: <action>.
2. <date>: <must be true> — if not: <action>.
3. <date>: <must be true> — if not: <action>.

**The executor already does** (do not repeat): research and research bonuses, focus, construction,
production and design switches, laws (gated ones too) and decisions in the plan, templates, training,
armies and layout, army groups and marshals, hold posts, war moves on `vp_targets`, guards, doctrines, air
missions, fleets, admirals and naval invasions, designs, MIOs, trade and auto-import, diplomacy (justify,
declare, pacts, faction), the agency, occupation laws, peace demands, events in `event_rules` /
`event_options`, the casualty and air-superiority rests. <add or remove per plan> It wakes you with a
`wake_player` alarm (`rule`, `reason`, `evidence`) when one of these needs judgement: act on that one item.

**Your items** (one batch per turn; add `request_id` and `lease_token`). One line per player item of the
plan (keys tagged `x-owner: player`, or no key yet); drop the kinds it does not use. Argument names and types
come from `python3 scripts/hoi4facts/tool_schemas.py show <tool>`; numbers below are examples, the plan gives the values:
* Retake, if <rule>: `hoi4_army_move {"army":"<army>","state_id":1,"province_id":1}`.
* Sea transfer, <when>: `hoi4_army_move {"army":"<army>","state_id":1}` (reads `sea_transfer_unverified` until landed).
* Line change, <date>: `hoi4_production_set {"line":0,"equipment":"<equipment>","factories":0}` (0 factories empties or removes the line).
* Encirclement, at war: `hoi4_encirclement_candidates {"enemy":"<TAG>"}`, then each pincer arm with `hoi4_army_move {"army":"<army>","state_id":1}`.
* Incoming proposal of <kind>: `hoi4_diplomacy_incoming {}`, then `hoi4_diplomacy_incoming {"answer":"accept","index":0}` (or `"refuse"`).
* Thresholds the executor does not watch yet (schema `x-executor-planned`): <read> <threshold> → <call>.

**Events that need judgement.**
* `<event id>`: pick `<option_name>` if `<predicate>`, else `<option_name>` — <why>.

**Do not.**
* <rule>. Never take: `<decision or option>` (<effect>).
* Never pause the game. Never reassign a general who already has an army.

**On `plan_revised`.** Re-read this brief once (`RUN/plan.brief.md`); drop items the new brief no longer
lists; keep playing at speed 3.

**If stuck.** `hoi4_plan {"op":"request_revision","reason":"<one line>"}` and continue the current plan.
