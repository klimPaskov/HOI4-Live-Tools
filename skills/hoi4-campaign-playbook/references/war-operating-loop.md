# War operating loop (tool by tool)

How to fight a land war with the macOS tools. The why behind each rule is in
`war-air-naval-peace.md` sections 1-4; this file is the procedure. Read it before any war
and follow it every war tick.

## 0. Principles

- **Concentrate.** Win by attacking with many divisions on a few axes, not by spreading every
  army along the whole border. One strong axis beats five weak ones.
- **Encircle, don't push.** Taking a pocket of enemy divisions destroys them; pushing them back
  only moves them. Every offensive should aim *behind* enemy troops (a neck state), not at them.
- **Never leave a border empty.** Every border with an enemy has a front line held by an army.
  Attacking armies come from a separate pool, not from the defensive line.
- **Plan, then execute.** Draw lines, let planning build (10-15 game days), execute all
  attacking armies on the same day. Against a weak defender with no continuous line, strike
  its victory points instead (section 5c): one march order per army, no planning wait.
- **Few orders, long-lived.** At speed 3 a decision and its call cost days of game time; give
  each army one order that holds for weeks and let rules carry the rest (section 5c).
- **Watch supply and organisation.** An attack that outruns supply stops and gets encircled.

## 1. Before the war (or on day one)

1. `hoi4_wars` : list every border with each enemy; `uncovered_own_border_states` must be empty.
2. `hoi4_armies` : armies, sizes, generals, where they stand. Every army 12-24 divisions with a
   general (`hoi4_army_commander`; take names from the error list, never guess).
3. Split roles:
   - **Line armies** (infantry): one per border sector, `hoi4_army_front` toward the enemy.
   - **Strike armies** (best divisions: armour, motorised, highest attack): 1-3 armies kept
     for offensives. They also get a front line on the sector they will attack from.
4. `hoi4_front_plan {"enemy":"TAG"}` : ranked attack axes (target, entry state, divisions
   needed, jump-off states). Pick the top 1-2 axes.
5. `hoi4_encirclement_candidates {"enemy":"TAG"}` : pockets, their neck states and pincer arms.
   A pocket with `pincer_ready:true` and high score is the first operation.
6. Air: fighters `air_superiority`, CAS `cas` over the region of the main axis
   (`hoi4_air_mission`, state_id of the target).
7. Overseas war: the home army is part of the force. Ship the divisions that fight well in the
   theatre in the first weeks (plan `sea_transfer.routes`; one `hoi4_army_move {army, state_id}`
   per army, three to four weeks through one canal) and let each landed army join a strike
   formation; disband the rest so their equipment refills the front. A war fought with the
   colonial garrison alone is slower than the game's own AI, which ships its home army at once.

## 2. Encirclement procedure

The executor runs this procedure itself when the plan lets it (`war.encircle`, on by default; the
run result's `war.encircle` line says which pocket it is closing and why another was passed over).
The player does it by hand only with the executor off, or for a pocket the executor gave up.

For the chosen pocket (from step 5):

1. **Anvil:** the line army facing the pocket keeps its front line (it pins the enemy).
2. **Pincers:** two strike armies, one at each pincer arm (`pincer_arms`: own state -> neck
   state). Move them there first if needed (`hoi4_army_move` to the own arm state).
3. Each pincer: `hoi4_army_front` toward the neck's owner, then
   `hoi4_army_offensive {"army":..., "state_id": <neck state>}`. Both pincers aim at the
   **neck**, never at the pocket itself.
4. Wait for planning (planning gains 2% a day to its 30% cap: about 14 game days, at most 15;
   `hoi4_wait_for`), then `hoi4_army_plan` execute on both pincers on the same day.
5. Every 3-5 game days: `hoi4_wars` (occupied/threatened states), `hoi4_supply` (under 50% =
   stop that pincer, `hoi4_army_plan execute:false`; under 35% pull part of it out),
   `hoi4_encirclement_candidates` (did the pocket close? `already_cut_off:true`).
6. Closed: pincers get a front facing outward (the rest of the enemy); the anvil gets an
   offensive into the pocket states to destroy it.
7. Next pocket.

No pocket available: attack the top `hoi4_front_plan` axis with the strike armies
(front + offensive toward the axis target, execute), and keep line armies holding. Against a
weak defender whose victory points lie far apart, use objective strikes instead (section 5c).

## 3. Defence procedure

1. Every border covered (`hoi4_wars`). Line armies on fronts; never move them all to attack.
2. Prefer rivers, mountains, forests and forts: a front drawn on good terrain holds with fewer
   divisions.
3. Keep one reserve army one state behind the most threatened sector; when an enemy breaks
   through (`occupied_own_states` grows), counter-attack the broken state with the reserve
   (`hoi4_army_move` into it, or an offensive line).
4. Losing a sector: pull back to the next defensible line instead of leaving divisions to be
   encircled; protect the capital and high-victory-point states first (capitulation comes at
   about 80% of victory points lost). Fall back when half the land around an army is hostile
   (`hoi4_armies` threat) or the neck behind it is 2 own provinces or fewer; the
   `encirclement_risk` flag (70%) comes too late. The runner falls back at the same half
   (`war.pocket_share`, default 50%). The HQ goes first.
5. Use what the enemy gives you: an enemy salient (an enemy-held state sticking into your
   line) is a pocket for you; check `hoi4_encirclement_candidates` every week even in defence.

## 4. Every war tick (every 3-5 game days, speed 3, never paused)

0. `hoi4_alerts` : the game's own alert row; each alert names the tool that fixes it.
1. Answer events at once (`hoi4_events`, `hoi4_event_choose`): an open event blocks every action.
2. `hoi4_wars` : new occupied or threatened own states, uncovered borders -> fix first.
3. `hoi4_armies` : armies without plans, unassigned divisions (fold them, `hoi4_army_assign`).
4. `hoi4_supply` : under 50% -> no new attacks there; under 35% stop and pull 25-50% of the
   divisions out; resume at 70%.
5. `hoi4_encirclement_candidates` : new pockets -> start section 2.
6. Stalled offensive (no progress for 10+ days) -> stop it, re-plan on another axis. Under
   objective strikes the army turns to its next objective by itself; do not re-order it.
7. Production: replacement equipment for the losses (infantry equipment, artillery, support).

## 5. Tool notes

- `hoi4_army_front` fails in very flat desert where the game shows no highlight: use
  `hoi4_army_move` into the enemy state as a direct attack order instead.
- `hoi4_army_motorization {army, level}`: 0/1/2; level shown in `hoi4_armies` (motorization).
- `hoi4_army_plan {army, mode}`: how the army's battle plans are carried out, `careful`, `balanced`
  (the game's default) or `rush`; with `execute` the mode is set first. Rush suits a weak or
  broken enemy line (few divisions, no forts, low organisation). It attacks every province of the
  plan, lengthens the front and can gather an army in one unsupplied province, so it is a plan
  decision, set with `war.execution` or `war.stall_rush`, and no cure for a plan that stands
  still: an army executing on rush stood three weeks with a flat planning bonus. Go back to
  balanced against a line that holds. Careful suits an army short of supply. (Gap register
  `orders.spearhead_exec_mode`, unvalidated until its live run is recorded.)
- `hoi4_army_create` folds every unassigned division standing in the state into the new army.
- `hoi4_army_detach {army, divisions:[ids], into?}`: move chosen divisions (ids and their template, equipment
  fill and supply from `hoi4_supply`) into a new army or an existing one: weak units to the rear, fresh ones forward.
- Equipment: `macplay sitrep` lists divisions short of an equipment type; losses of artillery and support equipment
  need their own production lines, not only infantry equipment.
- `hoi4_research_start {..., replace:true}` switches a busy research slot.
- `hoi4_division_disband {army, divisions:[ids]}`: disband useless divisions (all of an army's ids
  disband the army); men and equipment return to the pool. See army-organisation.md.
- `hoi4_map_mode {mode}`: supply before an offensive, resistance/compliance for occupied land, air
  for superiority; back to `political` before army orders.
- `hoi4_occupation_law {law}`: default occupation law; `hoi4_occupation` reads it.
- At the end of a war, `hoi4_war_stats`: own losses must be far below the enemy's.
- `hoi4_army_move` replaces the army's battle plan with a manual order (planning is lost).
  With `province_id` the whole army marches on that one province and attacks what stands there: the
  way to mass an army on one objective. It is sent again while any division neither marches on the
  province nor stands in it. `{"army":..., "halt":true}` cancels every march and attack order of
  the army (the unit panel's halt button): worn divisions stop attacking and recover where they
  stand. `hoi4_supply` shows each division's `heading_province`.
- `hoi4_province_graph {"states":[ids]}`: the land provinces of those states and the provinces each
  touches: which provinces an objective can be attacked from, and the way to it over land.
- `hoi4_naval_invasion` needs convoys and naval superiority; its receipt does not confirm the
  landing, so check the map/army after it starts.
- Peace: `hoi4_peace_options`, `hoi4_peace_take {"states":[ids]}`, `hoi4_peace_submit`,
  `hoi4_peace_exit`.

## 5b. Let the plan runner carry routine fighting

At speed 3 a game month passes in under two wall minutes; a player making a few tool calls a
minute cannot re-issue every stalled attack. The plan's `war` object makes the runner do it, and
the player decides strategy:

- `"war": {"enemies": ["ETH"], "axes": [[835, 271], [843, 271, 840]], "theatre": [550, 559]}`:
  every army of `min_army`+ divisions standing in a `theatre` state (or in enemy land) follows the
  axis whose next enemy-held state is nearest: front line, offensive line, execute.
- Lines first. An army without its lines gets its front and its offensive line in one step,
  before any rename, commander or disband: planning only grows once the lines stand. In a game
  still paused before the first unpause the executor draws them too, and the feed says
  `setup_complete` when nothing is left to order; unpause then. Trucks (`motorization`) follow
  the lines. Drawing lines is not an attack: an army short of organisation, supply or equipment
  still gets them and plans while it recovers; the limits hold back the execute order and moves.
- The offensive line is drawn across the target state, parallel to the army's front and as wide
  as the state, through the depth of the army's victory-point province (`hoi4_army_offensive`
  `province_id`; without one, through the middle of the state). The game extends every
  offensive line to the edges of the enemy land, so a battle plan cannot gather an army on one
  province: only the depth of the line is a choice. An executing army advances to its line and
  stands there: when the row the line was drawn for is held and the plan's next row lies in the
  same state, the executor draws the line again at the new row's depth (the plan keeps
  executing, no new planning wait). By hand: `hoi4_army_offensive` with the next `province_id`.
- Encirclement takes an army only when it has no target of its own or its target lies in the
  pocket or its neck.
- A hold phase (`"axes": []`) prepares the attack that follows it (`hold_prepares`, default on):
  the armies draw their lines toward their next target and plan on them, the execute order waits
  for a phase with axes, and a plan still executing on enemy land is stopped. The planning wait
  therefore runs during the hold, not after it.
- `"stall_days": 8` (default): an ordered army that took no province of its target, or of any
  state its divisions stand in, for that long
  first has its plan stopped and planned again (`replans`, default 2 per target: it stands on its
  line, regains organisation, entrenches and rebuilds planning bonus, then executes through the
  planning wait), then gets a second direction (an offensive on another axis, `second_direction`,
  default on), then a direct `hoi4_army_move` into its target. The direct move burns planning
  (3% a day) and has no flank cover.
- `"stall_rush": true` (default off): before all of that, an executing plan that stalled is set
  to rush once per army; it keeps executing and keeps its planning bonus, and the stall clock
  starts again. A plan decision for a weak or broken enemy line, never automatic: rush lengthens
  the front and can gather an army in one unsupplied province.
- `"strike": {"after_vp": 5}` (default off): objective strikes instead of battle plans once the
  player holds `after_vp` of the enemies' victory points. How it chooses, ends and retakes, the
  knobs and how to write the rows: section 5c.
- `"execution": "rush"` (or `"careful"`, `"balanced"`): the execution mode of every driven
  army's battle plans, set once per army when its lines stand and before its first execute
  order. Without the key the game's default (balanced) stands and nothing is sent.
- Rest and resume (defaults): an army rests below 30% of the highest organisation it has shown
  (`rest_below_pct` 0.3) and resumes at 80% (`resume_above_pct` 0.8) with supply at 70%
  (`supply_resume`), after at most `rest_max_days` 20 unless supply is under `supply_hold`.
  `rest_below` / `resume_above` (absolute organisation) replace the shares when set.
- `"supply_floor": 50` (default): an army under 50% supply rests and gets no new attack order
  except a retake of an own state; under `supply_hold` 35 (default; attrition starts there)
  nothing attacks.
- Planning wait (default on): an offensive executes once planning reaches 90% of its 30% cap,
  at most `planning_max_days` 15; skipped `planning_skip_days` 20 before the goal deadline.
  An offensive planned again (a stopped plan) also executes once its bonus has stood still for
  `planning_flat_days` 3: divisions in contact or on the move build none, and the wait would
  only give the enemy the time.
- The stall's direct move goes to the army's victory-point province when it lies in the target
  state (else to the plan path's waypoint, else to the state): it gathers the army on the
  province the plan is after.
- Advance in bounds (`bound`, default on): after a captured hub, port or railway the next
  advance waits until it feeds the front (rail about 10 days, 5 on cores), at most 12 days.
- `"motorization": 2`: every driven army's supply hubs use trucks.
- `"reserve": ["Army 2"]`: armies only the player commands. Executing plans are never touched.
- `"guard_states": [559, 844]` (or `"guards": {"Army 3": 844}`): the army standing in that own
  state holds a front there and never attacks. Keep one on every own border state the offensive
  leaves behind: an empty border state is walked into.
- An own state an enemy takes draws the nearest fighting army off its axis at once.
- The war step runs right after events (`"first": false` puts it back after construction): a
  game day passes in about 2.5 s at speed 3.
- `"armies": {"states": [550, 559], "generals": [...], "consolidate": false}`: armies are made
  from unassigned divisions only in those states; generals come from the `hoi4_army_commander`
  list (names outside it are refused).
- Construction works in occupied states you control: railway levels on the axis from the
  nearest port or hub first (top of the queue); a supply hub only when it finishes in time.
- `"disband": {"templates": [...], "max": N}`: divisions of weak templates standing in own states
  outside the theatre are disbanded so their equipment refills the front.

## 5c. Objective strikes: the method that won against a weak defender

Battle plans spread an army along the whole border, and the game extends every offensive line
to the edges of the enemy land, so a planned offensive takes the border states and then stands.
What took victory points at speed 3 was simpler: one direct march order of a whole army on one
victory-point province it stands beside, sent once, and the next objective ordered the day the
last one falls. The strike step of the executor (`war.strike`) does exactly this; a player can do
it by hand with `hoi4_army_move {"army":..., "province_id":...}`.

**When to prefer it.** A defender with few divisions and no continuous line, victory points far
apart, roads more than fronts (colonial and minor wars, a collapsing enemy, the mop-up after a
capitulation of the main army). Keep battle plans and encirclement (sections 2-3) against a
continuous line, a strong defender, or a pocket that can be closed: a march order has no planning
bonus, no flank cover and pushes rather than encircles. Let the opening plans take the border
first (`after_vp`), then switch.

**How a strike runs.**
- The army stands beside the objective, ideally in two or three provinces touching it (more
  directions, OP04). From afar it first marches to an assembly province on its road.
- One order, never repeated while the army moves or fights: a new order starts the march again.
  The tool resends only to divisions that neither march on the province nor stand in it.
- The province falls: the next objective is ordered the same day. Strikes that took their
  objective did so in days to a few weeks.

**Turn away from an objective that holds.** A strike ends when the divisions beside the
objective fall under about 35% of their best organisation, or after about 18 days. Do not feed
it: the objective is closed for that army (about 10 days after a worn strike, 25 after one that
stood its whole time) and the army's next march order goes to another objective. Whether the
same town falls in four days or holds for months varies from game to game; there is no read yet
of the enemy divisions beside an objective, so the step finds out by striking. Do not halt the
army: a halted division walks back to the front line its army still holds, which after an
advance is the border it started from (`halt` stays false unless a plan has a reason).

**Retake behind the advance first.** An advance leaves its victory points empty. A held
objective that is lost again comes before every new one, and so does own land an enemy holds
beside the army. Keep the guard armies as guards on the border the advance leaves behind
(`guards` false): when every army struck, the enemy retook towns and the home border behind
them and the advance starved. Taken victory points get no garrison by themselves; a guard army
whose front follows the advance, or a division left in each, is still an open question.

**Never stack more than the head province feeds; spread instead.** A whole army in one
province starves: eleven divisions in one unfed province stood for three months at 6-10% supply
and organisation about 10, while three divisions took a supply hub four days after their order.
The game's own AI fights the same colonial war with about two dozen divisions spread over eleven
to fifteen provinces, never more than four in one, and keeps 75-100% supply; its organisation and
equipment are no better than ours, its supply is. So a strike goes with at most `stack` (4)
divisions from one province (the freshest, those beside the objective first), and the divisions
it leaves over take the enemy provinces beside the army two at a time, those touching the
objective first (`widen`). Each taken province is more land to stand on, another road for
supply and another direction for the next strike; the left-over divisions rejoin their army when
their province is held or after `max_days`. Plan armies one per road and name a fed province as
each row's first path province. Outside a strike, a stack its province cannot feed still sends
its most worn divisions to held land beside it.

**More force does not cure starvation.** Shipping a dozen home divisions into a theatre that
starves at the head made the war slower, not faster: they joined the same corridor, and the
divisions kept for shipping were no longer disbanded, so the front's equipment fill fell from
about 0.8 to 0.3. Ship only what opens its own road (another port, another axis) and keep the
equipment of what is useless flowing to the front. In a war where the enemy's escalation counts
the attacker's divisions in the theatre (Ethiopia 1936: more than 24 regular Italian divisions is
one trigger towards a closed Suez canal), the AI keeps under it.

**Why orders must be few.** At speed 3 one decision and its call take 30-90 seconds of wall time,
5-15 game days. A war of about six months is about twenty decisions. Orders must be few, given
early and meant to last weeks; everything else is a rule the executor runs. A player who
re-orders an army every tick gives the enemy the days.

**Keep the regrouping rules off armies under orders.** The overdrawn rule detaches divisions
from a crowded province and the command step merges small armies: on an army someone is
ordering they undo the grouping. The strike step switches both off for the armies it drives.
An army the player orders by hand goes in `war.reserve` (by exact name, and so does any army
made by a detach); `manage.off: ["overdrawn"]` and `armies.consolidate: false` switch the rules
off for every army.

**In the plan.**

```json
"war": {"enemies": ["ENE"], "strike": {"after_vp": 5, "why": "weak defender, towns far apart"}},
"vp_targets": [
  {"order": 1, "province": 1001, "state": 10, "army": "North strike", "path": [2001, 2002], "vp": 5},
  {"order": 2, "province": 1002, "state": 11, "army": "North strike", "vp": 3},
  {"order": 3, "province": 1003, "state": 12, "army": "South strike", "path": [2010], "vp": 10}
]
```

- `vp_targets` rows per army, in the order its road opens: the step takes an army's rows in list
  order. `army` must be the army's exact name or its `army_layout` name hint; a row without an
  army is not used by the strike step.
- `path`: waypoint provinces from own land to the objective, each touching the next. The first
  is the assembly point (name a fed province); the first one not yet held is the next leg. A row
  is open only when its leg touches land already held; a path through a neutral's land waits
  until that land is the player's.
- With no open row an army takes an enemy victory point with `beside` (2) fresh divisions next to
  it, else the nearest enemy victory point no other army strikes.
- Knobs and defaults: `after_vp` 0, `rest` 0.35, `resume` 0.72, `rest_days` 10, `max_days` 18,
  `close_days` 25, `beside` 2, `stack` 4, `widen` true, `guards` false, `halt` false. `war.reserve` and
  `war.player_claim_seconds` (300) keep the step off armies the player orders.

**What is still unproven.** The game's own AI does not win this kind of war on a fixed date:
measured twice from another seat, it took Ethiopia's capitulation on 17 June once and in early
September once (2 of 32 victory points in mid-February, 7 in mid-April), so judge a run against a
spread of dates, not one mark, and do not end a run as lost only because it is behind the faster
one. The strike method won by date ahead of the slow AI game several times and stalled at 5-17
victory points in others, each time with stacks starving at the head; spreading (`stack`,
`widen`) is the answer to that and still has to show it holds over a series of runs. Garrisons
for taken victory points are open. Treat the numbers above as defaults to check, not tuned
values.

## 6. Principles (any war)

- **Take states, not only points.** A state is controlled by whoever holds its largest victory point, and
  some countries' surrender rules count states: Ethiopia in 1936 gets a 15% lower surrender limit once it has
  controlled half of its states or fewer for 90 days, which is why it capitulates at about half of its victory
  points. Read the enemy's decisions and national spirits in the game files for such rules before the war;
  order objectives so the states fall early (the cheapest largest point of each state), because the clock
  starts with the state count, not with the points. The strike step weighs such points (`state_weight`).
- **Supply caps a front.** Every front has a supply ceiling. More divisions than it carries starve all of them
  (18 divisions on one front sat at 14-27% supply and organisation about 12). Read `hoi4_supply` every war tick;
  when an army is below 50% supply, thin it (split and move part elsewhere) rather than adding more.
- **Quality at the front, quantity in the rear.** When supply is the limit, every division at the front eats the
  same supply and manpower whether it fights well or not. Attack with the best divisions only (highest attack,
  full equipment, good organisation); pull weak units (irregulars, cavalry, under-strength or low-equipment
  divisions) out of the strike armies (`hoi4_army_split`, `hoi4_army_merge`) into a rear army that holds quiet
  borders and captured states. Fewer, better divisions at full supply beat many starving ones.
- **Concentrate on victory points.** Strike armies go for the enemy's victory-point cities in order of value, not
  along the whole border; the line armies only hold.
- **Organisation decides attacks.** Attack with organisation above about 50% of the template's max; rest at about
  30%, resume at about 80%, then attack again all at once on the same axis. Continuous weak attacks lose. Under
  objective strikes a worn army turns to another objective instead of resting in front of the same one (5c).
- **Exploit what the enemy lacks.** No air force: every fighter and CAS wing over the attack. No anti-tank: armour
  and fast divisions lead breakthroughs and pincers. Weak defence in open terrain: encircle there.
- **Aim at victory points.** Capitulation comes from lost victory points; the capital and big cities end wars, so
  operations point at them, and defence protects one's own.
- **Reinforcements take time.** Moving divisions by sea or across a continent takes weeks to months; decide in the
  first days of a war, not when the front stalls.
- **Motorize supply far from railways.** An army fighting beyond the railheads (colonies, deserts,
  mountains, deep advances) gets far more supply when its hubs use trucks:
  `hoi4_army_motorization {army, level:2}` (one army rose from 29% to 41% supply in nine game days).
  A fully motorised hub uses 60 trucks, so keep a truck production line running.
  Set it on day one of such a war, not after the attack stalls.
- **Read the scenario's own rules.** Missions, timers and war-ending conditions of the country's focus
  tree and decisions decide what "winning" means (a mission timer can end a war as a loss even while
  advancing); check them before planning and plan the operation around them.
- **Construction is slow.** Buildings finish long after a short war; in a war of months spend factories on the
  economy and production, not on front-line infrastructure or supply hubs.
- **Never idle, never busy.** Every army has a standing order (hold a border, reserve, an offensive or a strike);
  every stalled offensive is re-planned or turned to another objective, every idle army gets a task. An order that
  is working is left alone: re-issuing it costs days (5c).
- **Adapt.** When the situation changes (new enemy, counter-offensive, an ally collapsing, a pocket against you),
  stop, re-read `hoi4_wars`, `hoi4_front_plan` and `hoi4_encirclement_candidates`, and re-plan from section 1. No
  scenario plan survives contact; these principles do.

## 7. Tactics: continuous play at speed 3

| ID | Tactic | How it works | When | Mode |
|---|---|---|---|---|
| CP01 | War-readiness gate | Before a declaration: fronts on every border, plans drawn 15 days earlier, fighters up 7-14 days earlier, laws at their gates, stockpile cover 30+ days, generals and HQs placed, motorisation set; the declaration waits for them | Every planned war | FAIR |
| CP02 | Switch production when the tech lands | The tech completion is the trigger for the line rule (tier guard or parallel ramp), not a later review | Tier techs | FAIR |
| CP03 | Pockets from candidates | Close pockets with `hoi4_encirclement_candidates` and an offensive into the neck, planned ahead, not hour-by-hour micro | Every war | FAIR |
| CP04 | Cancel losing attacks | The casualty-ratio rest rule and careful execution stop attacks that lose badly | Every war | FAIR |
| CP05 | Events by option name | Answer events with `event_rules` by option name, never by index | Always | FAIR |
| CP06 | Peace conference | The game pauses itself; bid by the peace order | Each capitulation | FAIR |
| CP07 | Rebalance after a capitulation | A phase that enters on the capitulation frees the guards, re-axes the armies, switches occupation laws and moves factories land to air or navy | Each capitulation | FAIR |
| CP08 | Air missions at war start | Air plan entries with a `when` on the war predicate start every wing at once | War start | FAIR |
| CP09 | Posture per phase with a cull | Each phase sets the production posture and removes the lines it no longer needs | Phase changes | FAIR |
| CP10 | Objective strikes | One march order of a whole army on a victory-point province it stands beside, sent once; the next objective the day it falls (`war.strike`, 5c) | Weak defender, victory points far apart | FAIR |
| CP11 | Strike or battle plan | Strikes against a weak defender without a continuous line; battle plans and pockets against a line, a strong defender or a pocket that can close; the opening plans take the border first (`after_vp`) | Choosing the war method | FAIR |
| CP12 | Turn away from a town that holds | A strike ends at about 35% of best organisation or 18 days; the objective closes for the army and it marches on another; no halt order (halted divisions walk back to the old front) | Every strike | FAIR |
| CP13 | Retake behind the advance first | A lost objective and own land an enemy holds beside the army come before new objectives; guard armies stay guards (`guards` false) | Every advance | FAIR |
| CP14 | Stack only what the head feeds | The strike sends the whole army into one province: plan one small army per road, sized to the supply where the road ends, with a fed assembly province first in the path | Planning strike armies | FAIR |
| CP15 | Few, long-lived orders | A decision costs 5-15 game days at speed 3, so a six-month war has room for about twenty; rules carry the rest; never re-issue an order that is working | All war play at speed 3 | FAIR |
| CP16 | Regrouping rules off ordered armies | The overdrawn rule and command merges undo a grouping someone is ordering: the strike step switches them off for its armies; hand-ordered armies go in `war.reserve` by exact name | Hand orders beside the executor | FAIR |
