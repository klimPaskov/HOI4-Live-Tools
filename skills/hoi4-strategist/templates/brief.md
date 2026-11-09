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

**Your items** (one batch per turn; exact calls; add `request_id` and `lease_token`). Keep one line per
player item of the plan and drop the kinds the plan does not use:
* Templates, <when>: `hoi4_template_create {"name":"<n>","regiments":[...],"support":[...],"priority":"<p>"}`.
* Gated law, when `hoi4_country_status {}` shows <gate>: `hoi4_politics_choose {"category":"<c>","slot":<s>,"idea":"<idea>"}`.
* Guarded decision, when <predicate>: `hoi4_decision_take {"decision":"<key>","target":<t>}`.
* Army group, <when>: `hoi4_army_group_create {"armies":[...]}`, then `hoi4_army_commander {"army":"<group>","general":"<marshal>"}`.
* Hold, <until>: `hoi4_army_front {"army":"<army>","state_id":<s>}`; retake, if <rule>: `hoi4_army_move {"army":"<a>","state_id":<s>,"province_id":<p>}`.
* Air, <phase or switch_when>: `hoi4_air_mission {"wing":"<w>","mission":"<m>","region":<r>}` / `hoi4_air_rebase {"wing":"<w>","state_id":<s>}`.
* Sea transfer, <when>: `hoi4_army_move {"army":"<a>","state_id":<s>}` (reads `sea_transfer_unverified` until landed).
* Navy, <when>: `hoi4_fleet_mission {"task_force":"<tf>","mission":"<m>"}` / `hoi4_naval_invasion {"state_id":<s>,"army":"<a>"}`.
* Designs, <when, XP>: `hoi4_design_create {"kind":"<k>","base":"<b>","modules":{...},"name":"<n>"}`, then `hoi4_design_set_production {"line":<i>,"design":"<n>"}`.
* MIO, <trait point>: `hoi4_mio_trait {"organization":"<o>","trait_index":<i>}`.
* Line replacement, <date>: `hoi4_production_set {"line":<i>,"equipment":"<e>","factories":<n>}`.
* Resources, on a deficit: `hoi4_auto_import {"enabled":true,"budget":<n>}` / `hoi4_trade_set {"resource":"<r>","country":"<TAG>","amount":<n>}`.
* Occupation, when <resistance / compliance rule>: `hoi4_occupation_law {"law":"<law>"}`.
* Diplomacy, <when>: `hoi4_diplomacy_act {"country":"<TAG>","action":"<a>","war_goal":"<g>"}`.
* Intelligence, <when>: `hoi4_agency_upgrade {"branch":"<b>","upgrade":"<u>"}`.
* Peace conference open: `hoi4_peace_take {"states":[<ranked>],"puppet":[...]}`, `hoi4_peace_submit {}`, `hoi4_peace_exit {}`.
* Thresholds the executor does not watch yet (`manage`): <read> <threshold> → <call>.

**Events that need judgement.**
* `<event id>`: pick `<option_name>` if `<predicate>`, else `<option_name>` — <why>.

**Do not.**
* <rule>. Never take: `<decision or option>` (<effect>).
* Never pause the game. Never reassign a general who already has an army.

**On `plan_revised`.** Re-read this brief once (`RUN/plan.brief.md`); drop items the new brief no longer
lists; keep playing at speed 3.

**If stuck.** `hoi4_plan {"op":"request_revision","reason":"<one line>"}` and continue the current plan.
