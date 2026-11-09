# Brief: <Country> — <goal> (plan v<N>)

<On a revision, first line: "Revised v<N>: <one-line summary of what changed>".>

**Win.** <predicates in one line, with ids>, before <deadline>.
Check: `<exact read call, e.g. hoi4_states {"query":"<ENEMY>"}>` — <what the answer must show>.

**Next checkpoints.**
1. <date>: <must be true> — if not: <action>.
2. <date>: <must be true> — if not: <action>.
3. <date>: <must be true> — if not: <action>.

**The executor already does** (do not repeat): research, focus, construction, production, laws in the
plan, decisions in the plan, armies and layout, war moves on `vp_targets`, guards, doctrines, events in
`event_options`. <add or remove per plan>

**Your items** (one batch per turn; add `request_id` and `lease_token`). One line per player item of the
plan; drop the kinds it does not use. Argument names and types come from
`python3 scripts/hoi4facts/tool_schemas.py show <tool>`; numbers below are examples, the plan gives the values:
* Templates, <when>: `hoi4_template_create {"name":"<name>","regiments":[["<battalion>","<battalion>"],["<battalion>"]],"support":["<company>"],"priority":"regular"}`.
* Gated law, when `hoi4_country_status {}` shows <gate>: `hoi4_politics_choose {"category":"government","slot":1,"idea":"<idea>"}`.
* Guarded decision, when <predicate>: `hoi4_decision_take {"decision":"<decision>","target":"<target>"}`.
* Army group, <when>: `hoi4_army_group_create {"armies":["<army>","<army>"]}`, then `hoi4_army_commander {"army":"<army group>","general":"<marshal>"}`.
* Hold, <until>: `hoi4_army_front {"army":"<army>","state_id":1}`; retake, if <rule>: `hoi4_army_move {"army":"<army>","state_id":1,"province_id":1}`.
* Air, <phase or switch_when>: `hoi4_air_mission {"wing":"<wing>","mission":"<mission>","region":"<region>"}` / `hoi4_air_rebase {"wing":"<wing>","state_id":1}`.
* Sea transfer, <when>: `hoi4_army_move {"army":"<army>","state_id":1}` (reads `sea_transfer_unverified` until landed).
* Navy, <when>: `hoi4_fleet_mission {"task_force":"<task force>","mission":"<mission>"}` / `hoi4_naval_invasion {"state_id":1,"army":"<army>"}`.
* Designs, <when, XP>: `hoi4_design_create {"kind":"tank","base":"<base>","modules":{"<slot>":"<module>"},"name":"<name>"}`, then `hoi4_design_set_production {"line":0,"design":"<name>"}`.
* MIO, <trait point>: `hoi4_mio_trait {"organization":"<organisation>","trait_index":0}`.
* Line change, <date>: `hoi4_production_set {"line":0,"equipment":"<equipment>","factories":0}` (0 factories empties or removes the line).
* Resources, on a deficit: `hoi4_auto_import {"enabled":true,"budget":5}` / `hoi4_trade_set {"resource":"<resource>","country":"<country>","amount":"needed"}`.
* Occupation, when <resistance / compliance rule>: `hoi4_occupation_law {"law":"<law>"}`.
* Diplomacy, <when>: `hoi4_diplomacy_act {"country":"<TAG>","action":"<action>","war_goal":"<war goal>"}`.
* Intelligence, <when>: `hoi4_agency_upgrade {"branch":"<branch>","upgrade":"<upgrade>"}`.
* Peace conference open: `hoi4_peace_take {"states":[1,2],"puppet":false}`, `hoi4_peace_submit {}`, `hoi4_peace_exit {}`.
* Thresholds the executor does not watch yet (`manage`): <read> <threshold> → <call>.

**Events that need judgement.**
* `<event id>`: pick `<option_name>` if `<predicate>`, else `<option_name>` — <why>.

**Do not.**
* <rule>. Never take: `<decision or option>` (<effect>).
* Never pause the game. Never reassign a general who already has an army.

**On `plan_revised`.** Re-read this brief once (`RUN/plan.brief.md`); drop items the new brief no longer
lists; keep playing at speed 3.

**If stuck.** `hoi4_plan {"op":"request_revision","reason":"<one line>"}` and continue the current plan.
