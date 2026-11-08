# Army organisation: quality, roles, generals, field marshals

Works for any country and any war. Decide from the game's own numbers (tool reads), not
from a country script. Tags: `[L]` seen in a live run, `[H]` heuristic.

## 1. Judge every template, then every division

- `hoi4_division_templates` lists the templates and `hoi4_template_get` gives each one's
  composition (battalions per column), estimated stats and how many divisions use it.
- A front-line template has at least 7 line battalions (for example 7 infantry and 2
  artillery) plus support companies (engineers, recon, artillery). `[H]`
- 2 to 4 battalion divisions, cavalry and irregular bands, and colonial divisions without
  support are not front-line divisions. Starting armies are full of them (Italy 1936: dozens
  of 6-battalion infantry, black shirts, bands and irregulars). `[L]`
- `hoi4_supply` shows every division's template, equipment fill, strength and supply.

## 2. Keep, move or disband

- Good divisions go to the front, on the main effort and the victory points.
- Weak divisions hold quiet borders, coasts, ports and rear states, or garrison occupied land.
- Disband (`hoi4_division_disband`) what only costs equipment, manpower and supply: tiny
  divisions, bands and irregulars nobody needs as garrisons, divisions far below strength
  while equipment is short. Disbanding returns their men and equipment to the pool (Army 4
  of two divisions gave back about 13,000 men, 5 Nov 2026). `[L]` A whole army goes in one
  call when every division of it is listed.
- Never leave weak divisions in a strike army: `hoi4_army_detach` moves them to a rear army.

## 3. Train what wins

- Improve one or two good templates (`hoi4_template_edit`): add artillery and support companies
  when the equipment exists, and widen weak infantry to 7-2 or larger. `[H]`
- Train them (`hoi4_train`) while manpower allows; the game alert `no_divisions_in_training`
  means the queue is empty. Size training to equipment production, not to division count.

## 4. Armies by role, with generals that fit

- One army per task, at most 24 divisions: a strike army (best divisions, all tanks and
  motorized), holding armies for each front, and a rear or garrison army.
- Read generals with `hoi4_commanders` (skill, attack, defence, planning, logistics and
  traits) and match them to the task, then assign with `hoi4_army_commander`:
  - tanks and motorized: panzer_leader or armor_officer, else the best attack and skill;
  - infantry assaults: infantry_officer or offensive traits, high attack;
  - mountains (Ethiopia, Alps, Balkans): trait_mountaineer;
  - deserts (Libya, Egypt): desert_fox;
  - rivers and forts: trait_engineer or fortress_buster;
  - cavalry and irregulars: cavalry_leader or irregulars_officer;
  - holding lines: defensive traits (defensive_doctrine, trait_cautious, organizer);
  - the best skill always leads the main effort.
- Every army needs a general; the sitrep flags armies without one.

## 5. Army groups and field marshals

- Each front with two or more armies is one army group (`hoi4_army_group_create`) with a
  field marshal (`hoi4_army_commander` with the army group's name). The marshal's bonuses
  apply to every army in the group. Only field marshals can lead a group: when the tool
  refuses a leader, pick another from the group's commander list.
- Pick the marshal with the best planning and skill.
- The sitrep flags armies without an army group and groups without a marshal.

## 6. Occupied land

- The game's `resistance` and `not_enough_garrison` alerts mean the occupation needs work.
- `hoi4_occupation` reads resistance, compliance and the default law. `hoi4_occupation_law`
  changes the default law:
  - harsher laws (Martial Law, Harsh Quotas, Brutal Oppression) cut resistance fast but need
    more garrison and slow compliance;
  - milder laws (Civilian Oversight, Local Police Force) raise compliance and need fewer
    troops once resistance is low.
- Map modes `resistance` and `compliance` (`hoi4_map_mode`) show where it burns.

## 7. Every month, one pass

1. Read templates, divisions (`hoi4_supply`) and generals.
2. Disband, detach or move the weak; strengthen the strike army.
3. Edit and train templates.
4. Check generals, army groups and marshals.
5. Set the occupation law and garrisons.
6. Look at the `supply` map mode before an offensive.
