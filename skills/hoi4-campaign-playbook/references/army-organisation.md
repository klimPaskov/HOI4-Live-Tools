# Army organisation: quality, roles, generals, field marshals

Works for any country and any war. Decide from the game's own numbers (tool reads), not
from a country script.

## 1. Judge every template, then every division

- `hoi4_division_templates` lists the templates and `hoi4_template_get` gives each one's
  composition (battalions per column), estimated stats and how many divisions use it.
- A front-line template has at least 7 line battalions (for example 7 infantry and 2
  artillery) plus support companies (engineers, recon, artillery).
- 2 to 4 battalion divisions, cavalry and irregular bands, and colonial divisions without
  support are not front-line divisions. Starting armies are full of them (Italy 1936: dozens
  of 6-battalion infantry, black shirts, bands and irregulars).
- `hoi4_supply` shows every division's template, equipment fill, strength and supply.

## 2. Keep, move or disband

- Good divisions go to the front, on the main effort and the victory points.
- Weak divisions hold quiet borders, coasts, ports and rear states, or garrison occupied land.
- Disband (`hoi4_division_disband`) what only costs equipment, manpower and supply: tiny
  divisions, bands and irregulars nobody needs as garrisons, divisions far below strength
  while equipment is short. At home all their manpower and equipment return to the pool. A
  whole army goes in one call when every division of it is listed.
- Never disband divisions overseas (75% of their equipment is lost) or in a pocket.
- Never leave weak divisions in a strike army: `hoi4_army_detach` moves them to a rear army.

## 3. Train what wins

- Improve one or two good templates (`hoi4_template_edit`): add artillery and support companies
  when the equipment exists, and widen weak infantry to 7-2 or larger.
- Train them (`hoi4_train`) while manpower allows; the game alert `no_divisions_in_training`
  means the queue is empty. Size training to equipment production, not to division count.

## 4. Few formations, tasks inside them, generals that stay

- Commanders learn from fighting: a general gains experience and traits from the battles of
  his divisions, a field marshal from the armies of his group. An army without a general gets
  no leader bonus, and an army of one or two divisions teaches its general almost nothing.
  So concentrate command: a few standing formations per theatre (by default at most 3 under one
  field marshal, each of 3 or more divisions, at most 24), each with a sector and a purpose
  (main effort, second axis, the other front), and each led by one general all war.
- Guarding a border, holding a corridor or watching a port is a task of the formation whose
  sector it is: its front line covers the state (`covers`, or an `army_layout` entry with
  `task_of`), not a separate army. Separate tiny armies leave generals idle, get moved more
  often (each move costs organisation) and read as pockets at every enemy step.
- Give orders once and keep them: a front line, then an offensive line, planning, execute.
  Direct moves are for mopping up and stalls; an army keeps a move for a few days and has a
  small move budget.
- Read generals with `hoi4_commanders` (skill, attack, defence, planning, logistics and
  traits) and match them to the formation, then assign with `hoi4_army_commander`:
  - tanks and motorized: panzer_leader or armor_officer, else the best attack and skill;
  - infantry assaults: infantry_officer or offensive traits, high attack;
  - mountains (Ethiopia, Alps, Balkans): trait_mountaineer;
  - deserts (Libya, Egypt): desert_fox;
  - rivers and forts: trait_engineer or fortress_buster;
  - cavalry and irregulars: cavalry_leader or irregulars_officer;
  - holding lines: defensive traits (defensive_doctrine, trait_cautious, organizer);
  - the best skill always leads the main effort.
- Every army has a general. The executor gives any army without one the best free general
  for its role (the plan's names first) and never moves a general who leads another army.

## 5. Army groups and field marshals

- One army group per theatre over all its formations (`hoi4_army_group_create`), led by a
  field marshal (`hoi4_army_commander` with the army group's name). His traits reach every
  army of the group and he gains experience from all of them. Only field marshals can lead a
  group; the tool promotes a general for command power. When no listed marshal is free, the
  executor takes the best free one (field-marshal traits, then skill).
- Pick the marshal by traits (Logistics Wizard, Offensive or Defensive Doctrine, Thorough
  Planner), then planning and skill.
- The sitrep flags armies without an army group and groups without a marshal.

## 6. Occupied land

- The game's `resistance` and `not_enough_garrison` alerts mean the occupation needs work.
- `hoi4_occupation` reads resistance, resistance target, compliance and the default law;
  `hoi4_occupation_law` changes the default law:
  - step one law harsher when a state's resistance target reaches 25 (Martial Law, Harsh
    Quotas, Brutal Oppression cut resistance but need more garrison and slow or stop compliance);
  - go milder (Civilian Oversight, Local Police Force) only at peace;
  - set laws per state when a tool allows it: Harsh Quotas on factory states, Forced Labour on
    resource states, Military Governor on populous states, Martial Law on supply routes.
- Garrison template by hardness: 1 cavalry battalion where manpower is plentiful, else 2-4
  obsolete light tanks or armoured cars; no military police (it raises losses).
- Map modes `resistance` and `compliance` (`hoi4_map_mode`) show where it burns.

## 7. Every month, one pass

1. Read templates, divisions (`hoi4_supply`) and generals.
2. Disband, detach or move the weak; strengthen the strike army.
3. Edit and train templates.
4. Check generals, army groups and marshals.
5. Set the occupation law and garrisons.
6. Look at the `supply` map mode before an offensive.

## 8. Tactics: generals, HQs and command power

| ID | Tactic | How it works | When | Mode |
|---|---|---|---|---|
| GN01 | What a skill point does | Attack +2.5% attack; defence +2.5% defence and breakthrough; logistics -2.5% supply use; planning +5% planning speed and +2% cap | Choosing generals | FAIR |
| GN02 | Marshals by traits | A marshal's traits reach every army in full, his skills at 50%; his own traits are off while he leads one army; pick by Logistics Wizard, Offensive or Defensive Doctrine, Thorough Planner | Army groups | FAIR |
| GN03 | One HQ per strike army | 1.19: no deployed HQ = planning cap and speed x0.8; HQ 0-4 provinces behind the front: planning x1.0 to x0.8, leader modifiers and abilities x1.06 to x1.0 (proximity setting close 1, medium 2, far 3 provinces); deploy 1-3 days, redeploy 15; an army with a deployed HQ cannot disband; a pocketed HQ loses its general to capture (marshals cannot be captured), so pull the HQ back first under encirclement risk (a rescue raid costs 20 command power and 60 days) | Every army of 10+ divisions at war | FAIR |
| GN04 | No reassignment churn | Leader modifiers wait 15 days after a group change; Recently Promoted is -1 skill for 100 days; swap divisions between generals instead of unassigning one | All | FAIR |
| GN05 | Traits come faster to a traitless general | Trait XP is divided by the number of traits held; Infantry Leader needs over 80% infantry, Panzer Leader over 40% armour; a terrain trait takes 700 XP fighting there (+10% attack and defence); keep one general on one terrain all war | General per army | FAIR |
| GN06 | Command power budget | +0.4 a day, cap 80; new general 5 x generals held; promotion 40; preferred tactic 20; trait 15; HQ 7-9; Force Attack and Last Stand cost 0.22 per battalion; spend before the cap | All | FAIR |
| GN07 | Best assignable traits | Logistics Wizard (-15% supply use), Offensive Doctrine, Thorough Planner, Adaptable (-30% terrain penalty); one per marshal per year | Mid-game | FAIR |
| GN08 | Command limits | 24 divisions per general (penalty grows to 100% at double), 72 on an area-defence order, Skilled Staffer +6; 5 armies per marshal, Expert Delegator +2 | Army layout | FAIR |
| GN09 | Sick and wounded generals | 2% a day sickness when 75%+ of his divisions stand in desert, jungle or marsh (30 days, half skills, no abilities); wounds halve skills for 90 days; never use abilities on them | Colonial and long wars | FAIR |
| GN10 | Army HQ template by role | Up to 4 battalions and 4 support companies; the HQ company gives +20% leader skill; engineer unlocks Deeper Dig-In, recon Defense in Depth, signal Rotating Reserves (+3% max planning), logistics Front Overrun (+2 command army size), field hospital Extra Medics (+10% unit XP). Deploying fills the template from the stockpile, else from the army's own divisions (never below 10% of their target); the HQ division's experience level is half the general's rank, rounded up | Offensive army: HQ + signal + logistics + engineer; defensive: HQ + engineer + recon + field hospital; deploy from a full stockpile before the attack, an infantry HQ while tanks are short | FAIR |
| GN11 | HQ abilities | Need a deployed HQ: Deeper Dig-In +600% dig-in speed for 7 days (0.12 per battalion); Defense in Depth toggle +3 max dig-in, -50% max planning (0.1); Rotating Reserves +6% reinforce rate for 7 days (0.12); Front Overrun toggle -10% org loss when moving (0.1); Extra Medics 4% casualty trickleback for 14 days (0.22) | Deeper Dig-In on every new defensive line below 40% dig-in, then Defense in Depth where no attack is planned | FAIR |
| GN12 | Trait abilities and Staff Office Plan | Expert Improviser: Makeshift Bridges (-40% river-crossing penalty, +20% attack on rivers, 7 days); Skirmisher: Probing Attack (attack without losing dig-in, -20% offence); Logistics Wizard: Extra Supplies (7 days of no-supply grace); Fortress Buster: Siege Artillery (+20% attack); Invader II: +50% invasion preparation speed; any general: Staff Office Plan (+400% planning speed for 7 days, 0.12) | Makeshift Bridges in the same hour a river assault starts; Staff Office Plan before a planned offensive | FAIR |
