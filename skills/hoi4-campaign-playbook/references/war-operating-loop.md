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
  attacking armies on the same day.
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

## 2. Encirclement procedure

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
(front + offensive toward the axis target, execute), and keep line armies holding.

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
6. Stalled offensive (no progress for 10+ days) -> stop it, re-plan on another axis.
7. Production: replacement equipment for the losses (infantry equipment, artillery, support).

## 5. Tool notes

- `hoi4_army_front` fails in very flat desert where the game shows no highlight: use
  `hoi4_army_move` into the enemy state as a direct attack order instead.
- `hoi4_army_motorization {army, level}`: 0/1/2; level shown in `hoi4_armies` (motorization).
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
- `"stall_days": 8` (default): an ordered army without progress first gets a second direction
  (an offensive on another axis, `second_direction`, default on), then a direct `hoi4_army_move`
  into its target. The direct move burns planning (3% a day) and has no flank cover.
- Rest and resume (defaults): an army rests below 30% of the highest organisation it has shown
  (`rest_below_pct` 0.3) and resumes at 80% (`resume_above_pct` 0.8) with supply at 70%
  (`supply_resume`), after at most `rest_max_days` 20 unless supply is under `supply_hold`.
  `rest_below` / `resume_above` (absolute organisation) replace the shares when set.
- `"supply_floor": 50` (default): an army under 50% supply rests and gets no new attack order
  except a retake of an own state; under `supply_hold` 35 (default; attrition starts there)
  nothing attacks.
- Planning wait (default on): an offensive executes once planning reaches 90% of its 30% cap,
  at most `planning_max_days` 15; skipped `planning_skip_days` 20 before the goal deadline.
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

## 6. Principles (any war)

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
  30%, resume at about 80%, then attack again all at once on the same axis. Continuous weak attacks lose.
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
- **Never idle.** Every army has an order every tick; every stalled offensive is re-planned, every idle army gets a
  task (hold a border, reserve, or attack).
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
