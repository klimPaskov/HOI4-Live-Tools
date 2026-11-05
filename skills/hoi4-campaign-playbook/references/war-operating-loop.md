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
4. Wait for planning (about 10 game days; `hoi4_wait_for`), then `hoi4_army_plan` execute on
   both pincers on the same day.
5. Every 3-5 game days: `hoi4_wars` (occupied/threatened states), `hoi4_supply` (below 70% =
   stop that pincer, `hoi4_army_plan execute:false`), `hoi4_encirclement_candidates` (did the
   pocket close? `already_cut_off:true`).
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
   about 80% of victory points lost).
5. Use what the enemy gives you: an enemy salient (an enemy-held state sticking into your
   line) is a pocket for you; check `hoi4_encirclement_candidates` every week even in defence.

## 4. Every war tick (every 3-5 game days, speed 2 in war)

1. Answer events at once (`hoi4_events`, `hoi4_event_choose`): an open event blocks every action.
2. `hoi4_wars` : new occupied or threatened own states, uncovered borders -> fix first.
3. `hoi4_armies` : armies without plans, unassigned divisions (fold them, `hoi4_army_assign`).
4. `hoi4_supply` : sectors below 70% -> stop attacks there.
5. `hoi4_encirclement_candidates` : new pockets -> start section 2.
6. Stalled offensive (no progress for 10+ days) -> stop it, re-plan on another axis.
7. Production: replacement equipment for the losses (infantry equipment, artillery, support).

## 5. Tool notes

- `hoi4_army_front` fails in very flat desert where the game shows no highlight: use
  `hoi4_army_move` into the enemy state as a direct attack order instead.
- `hoi4_army_motorization {army, level}`: 0/1/2; level shown in `hoi4_armies` (motorization).
- `hoi4_army_create` folds every unassigned division standing in the state into the new army.
- `hoi4_army_move` replaces the army's battle plan with a manual order (planning is lost).
- `hoi4_naval_invasion` needs convoys and naval superiority; its receipt says
  `landing_verified:false`, so check the map/army after it starts.
- Peace: `hoi4_peace_options`, `hoi4_peace_take {"states":[ids]}`, `hoi4_peace_submit`,
  `hoi4_peace_exit`.

## 6. Principles learned in live play (apply to any war)

These came from failed training wars; they are general, not tied to one country.

- **Supply caps a front.** Every front has a supply ceiling. More divisions than it carries starve all of them
  (seen: 18 divisions at 14-27% supply, organisation ~12, no progress). Read `hoi4_supply` every war tick; when an
  army is below 50% supply, thin it (split and move part elsewhere) rather than adding more.
- **Organisation decides attacks.** Attack with organisation above ~50%; below that, stop the plan, recover, then
  attack again all at once on the same axis. Continuous weak attacks lose.
- **Exploit what the enemy lacks.** No air force: every fighter and CAS wing over the attack. No anti-tank: armour
  and fast divisions lead breakthroughs and pincers. Weak defence in open terrain: encircle there.
- **Aim at victory points.** Capitulation comes from lost victory points; the capital and big cities end wars, so
  operations point at them, and defence protects one's own.
- **Reinforcements take time.** Moving divisions by sea or across a continent takes weeks to months; decide in the
  first days of a war, not when the front stalls.
- **Motorize supply far from railways.** An army fighting beyond the railheads (colonies, deserts,
  mountains, deep advances) gets far more supply when its hubs use trucks:
  `hoi4_army_motorization {army, level:2}` (seen live: 29% -> 41% supply and rising organisation in nine
  game days). It spends trucks (about 40 per hub per level), so keep a truck production line running.
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
