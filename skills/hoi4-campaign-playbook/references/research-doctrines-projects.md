# Research, doctrines, special projects, MIOs, variants

Scope: FAIR, Normal difficulty, historical AI, no exploits, 1936 start (the 1939 bookmark differs in doctrines,
slots and techs). Where this file and the in-game tooltip disagree, the tooltip wins.

## 1. Core mechanics (read first)

| Fact | Value |
|---|---|
| Base slots | 2 |
| 1936 start slots | GER 4, ENG 4, ITA 4, USA 4, POL 3, FRA 3, SOV 3, JAP 3, CHI 2 |
| GER 1939 bookmark | 5 slots (Heraeus focus done) |
| Tech cost | research cost x 110, then the ahead-of-time factor; `facts.py tech <id>` gives a tech's cost and year |
| Ahead-of-time | research speed = 1 / (2 x years ahead + 1): half a year ahead takes twice as long, one year three times. Ordinary bonuses do not reduce it; only ahead-of-time bonuses do |
| Focus tech bonus | `add_tech_bonus {bonus=X uses=N category=...}`: the next N techs of the category get +X research speed; only a bonus with an ahead-of-time reduction cuts the ahead penalty. `hoi4_research_extras` lists active bonuses and uses left |
| Idle slot carry | each slot keeps up to 30 days of progress while idle |
| Min research speed | 0.1 |
| Tech sharing | bonus up to 50%; a sharing-group member that already has a tech speeds it for the others |
| Licence production | +20% research on the matching tech; licensed output -25% (-35% outside the faction), -5% per year the design is ahead (up to 4); MIO traits do not apply; upgrading licensed gear costs 2x XP |

Doctrines are not technologies: they cost XP, not research slots. Read them with `hoi4_doctrines`, adopt with
`hoi4_doctrine_adopt`.

### 1.1 How to get more research slots (ordered by reliability)

1. National focus `add_research_slot = 1`:
   - GER: `GER_develop_heraeus_facilities` (needs `GER_kdf_wagen_factories` first); a second slot only in the Austrian-monarchy branch (`GER_reverse_the_brain_drain`), ignore.
   - POL: `POL_additional_research_slot1` (after `POL_the_four_year_plan`) and `POL_additional_research_slot2` (needs more than 50 owned factories).
   - CHI (nationalist tree): `CHI_rural_schooling`, `CHI_expand_the_academica_sinica`, `CHI_chemical_research_institute` each give a slot. Other variants of the tree use `CHI_unify_the_admissions_process`, `CHI_expand_academica_sinica`, `CHI_expand_national_sun_yat_sen_university`. Check which tree the save has with `hoi4_focus_list`.
   - Generic tree: `extra_tech_slot` after `infrastructure_effort_2`.
2. Many other trees have slot focuses: read the tree (`facts.py focus-tree`) for `add_research_slot`.
3. Scientists and special-project facilities do not add tech slots; they run a separate programme track (section 5).

Rule: for GER and POL the slot focus is high priority (earliest affordable). A 4th/5th/6th slot beats most
single-tech focuses. For CHI take the cheapest slot chain, but not before survival-critical focuses.

### 1.2 Research speed and bonus sources

- Scientists do not speed normal techs. Only facility basic research gives a tech bonus: +2% per scientist skill level, halved for a scientist shared across facilities.
- MIO design team on a matching research slot: +5% (some organisations more), 0.1 PP a day per assignment. A tech completed with a design team adds MIO funds (500 x research cost).
- Focus and idea bonuses: GER `GER_industrial_research_speed_factor` 5% in the industry branch.
- Subdoctrine rewards can grant tech bonuses (e.g. `mobile_infantry` immediate: +50%, 1 use, motorized_equipment; `irregulars` last reward: +50% x2 support_tech).

Rule: pull a tech ahead of time only when nothing in-year remains in any folder, or an ahead-of-time bonus covers it;
the plan says so.

## 2. Doctrines

Structure:

- Folders: land, naval, air, special_forces (special_forces needs No Compromise, No Surrender or Arms Against Tyranny).
- Grand doctrine: one per folder, paid with XP (100, army/navy/air). Gives immediate bonuses and unlocks 4 tracks (land: infantry, combat_support, armor, operations; air: fighter, strike, medium, heavy aircraft; naval: capital_ships, carriers, screens, submarines).
- Track: one subdoctrine per track (100 XP each; the cheap special ones cost 50: irregulars, peoples_war, great_war_infantry, patrol_boats).
- Mastery: the track fills automatically. Each reward needs 100 mastery by default (some subdoctrines less, e.g. peoples_war 50). Finishing all rewards in a track triggers the grand doctrine's milestone for that track.
- Mastery source: manpower of units in the track's categories (combat, missions; training gives one tenth). Diminishing returns beyond 100,000 contributing manpower; cap 40 mastery a month per track; surplus on a full track is banked at 25%, up to 200. Track multipliers: infantry x1, combat_support x8, armor x10, so armour and support tracks need far fewer men.
- Cost and mastery modifiers exist per folder (`<folder>_doctrine_cost_factor`, `<folder>_track_mastery_gain_factor`).
- Faction doctrine sharing: 1 faction initiative per folder, +10 mastery a month plus 2 per theatre commander.
- Subdoctrines marked `xor` cannot be held in different tracks at once; replacing within the same track is allowed (costs XP again, mastery restarts).

### 2.1 What each country starts with (1936 start)

| Tag | Grand doctrines set at 1936 |
|---|---|
| GER | new_mobile_warfare (land), new_convoy_raiding (naval), new_battlefield_support (air) |
| CHI | mass_assault (land), new_fleet_in_being (naval); air grand doctrine only at 1939, which also sets large_unit_tactics + 200 mastery |
| POL | grand_battleplan (land), new_fleet_in_being (naval); air only at 1939 (new_strategic_destruction) |
| FRA | grand_battleplan, new_fleet_in_being (+air at 1939) |
| SOV | mass_assault, new_fleet_in_being |
| ITA, JAP | grand_battleplan; ITA fleet_in_being, JAP base_strike |

Consequence: at 1936 nobody spends XP on land grand doctrines. The first army XP goes into track subdoctrines. CHI
and POL may also want an air grand doctrine (100 air XP) once any air XP exists.

Land grand doctrines: mobile_warfare = +20% planning speed, +10% army speed, Unexpected Thrust tactic;
superior_firepower = +10% line artillery soft attack, +5% support artillery soft attack; grand_battleplan = +10% max
planning, +25% command power gain, Planned Attack tactic; mass_assault = 48 hours of no-supply grace, -10%
out-of-supply penalty, Relentless Assault tactic. Re-picking a grand doctrine costs 100 XP again and resets mastery:
do not.

### 2.2 Subdoctrine choice (army XP 100 each; DLC gates apply)

| Track | Germany (mobile warfare) | China (mass assault) | Poland (grand battleplan) | Generic major / minor |
|---|---|---|---|---|
| Infantry | mobile_infantry (+10% breakthrough, free +50% motorized tech bonus) | large_unit_tactics; peoples_war only if communist | defensive_postures (dig-in, width) | defensive_postures (defender), assault_infantry (attacker); minors: irregulars (50 XP, NCNS) |
| Combat support | mobile_recon_and_assault (La Resistance) else fire_concentration | fire_concentration | fire_concentration or anti_tank_frontline vs Germany | fire_concentration default |
| Armor | armored_spearhead (La Resistance; `_no_lar` otherwise) | streamlined_deployment (only if armor exists, else delay) | armored_infantry_support / mobile_defense, low priority | by tag |
| Operations | mission_type_tactics early; deep_battle later | infiltration_tactics or mission_type_tactics (not last_stand) | mission_type_tactics | mission_type_tactics |

Notes: `deep_battle` and `armored_spearhead` have `_no_lar` twins for owners without La Resistance; only one variant
is visible. Poland and China rarely field enough armour for the armor track, so leave it last.
Air (air XP): fighter track = air_subdoctrine_fighter_homeland_defense (defender) / escort_fighter (offence);
strike_aircraft = flying_artillery for CAS-heavy GER; medium = tactical_battlefield_support; heavy = skip unless
bombers exist.
Naval: Germany keeps Trade Interdiction (`convoy_raiding`) with `wolfpacks` for submarines; screens get
`patrol_boats` (50 XP) or `torpedo_primacy`; `convoy_escort` only for route protection (Italy, UK). Navy choices
matter only if a navy is built.

Spend order for army XP (XP also pays variants, section 6): (1) infantry subdoctrine, (2) operations, (3) combat
support, (4) armor. Keep 35 XP in reserve for variant creation when a design is planned.

When NOT to apply: do not swap subdoctrines mid-war unless the current track's rewards are done (mastery restarts);
do not buy a subdoctrine the game greys out.

### 2.3 Experience budget (every spend of army, air and navy XP)

- Pools: army, navy and air XP each cap at **500**; XP above the cap is lost, so plan spends before a pool fills.
- Sources: combat (the main one); training divisions give at most 0.08 army XP a day and training ships at most 1
  navy XP a day; army exercises in peace (no tool yet); focus and decision rewards (`army_experience = N`, read with
  `facts.py focus <id>`); chiefs and high command with experience-gain traits (`politics-diplomacy-espionage.md` §4).
  Read the start pools with `hoi4_doctrines` at the first reconcile.
- Costs: grand doctrine 100; subdoctrines 100 (most), 60 or 50 (§2); template edits 5 per battalion, 10 per support
  company and 25 for an extra regiment column (the template tools return the cost); designer modules add 5,
  replace 6, convert 3, remove 1; legacy upgrades 10 plus a ramp (§6.2); officer-corps spirits 20, 35 and 50 XP for
  the first, second and third.
- Order: write every spend per branch with its month (plan `xp_budget`); no month spends more than the forecast;
  keep the 35 XP variant reserve (§2.2) only when a design is planned.

## 3. Research priority tables

Costs are research costs (x 110, §1). Years are start years; researching earlier is ahead of time.

### 3.1 Industry and infrastructure (always first)

| Tech | Year | Cost | Why |
|---|---|---|---|
| basic_machine_tools | 1936 | 1.5 | factory output; never skip |
| construction1 | 1936 | 2 | construction speed (also gates the facility/infrastructure line) |
| improved_machine_tools / construction2 | 1937 | 1.5 / 2 | |
| excavation1-2 | None/1937 | 2 | resource extraction (only if mine-rich) |
| concentrated_industry (or dispersed) | None/1937 | 2 | pick one branch by country; each level also +20% building slots |
| improved_equipment_conversion | 1937 | 2 | cheaper conversion of old stock |
| advanced_machine_tools / construction3 | 1939 | 2 | |
| fuel_silos, fuel_refining | 1936 | 1 / 3 | fuel for GER |
| rubber/oil processing (synth) | 1937+ | 1.5 | only for resource-starved GER/JAP/ITA |

### 3.2 Electronics, computing, radar, encryption

electronic_mechanical_engineering (1936, cost 1) is the root; it also gates the `sp_air_radar` special project. Then:
radio (cost 2, root of signals), improved_radio 1938 (gates sp_naval_proximity_fuze), radio_detection (cost 1.75,
radar line), computing_machine 1938 (cost 2.5), basic_encryption/decryption 1938, cavity_magnatron 1938,
centimetric_radar 1940. Research the radar line with the Germany knickebein bonus only if airfields matter; for
CHI/POL computing is low priority.
DLC gates: with La Resistance `basic_encryption`/`basic_decryption` are hidden (cryptology is in the intelligence
agency). `radio_detection` and `cavity_magnatron` need the completed `sp_air_radar` project: put `sp_air_radar` in
the special projects, not those techs in a research list.

### 3.3 Land equipment lines

| Group | Sequence | Notes |
|---|---|---|
| Infantry weapons | infantry_weapons1 (1936, 1.5), infantry_weapons2 (1938, 1.5), improved_infantry_weapons (1939, 2), improved_infantry_weapons_2 (1940, 2) | top priority; every division uses them. GER 1936 only has base infantry_weapons. |
| Support weapons | support_weapons (1918 base, 1.5), support_weapons2 (1938), support_weapons3 (1940) | |
| Support companies | tech_engineers 1936 (1.25), tech_recon, tech_military_police, tech_maintenance_company, tech_field_hospital, tech_logistics_company, tech_signal_company (all 1936, ~1.25), second tiers 1939 (1.5) | engineers and logistics first (section 7) |
| Artillery | interwar_artillery 1936 (1.5), artillery1 1939, artillery2 1940, artillery3 1941 | GER 1936 has gw_artillery only |
| Anti-air / anti-tank | interwar_antiair 1936, antiair1 1939; interwar_antitank 1936, antitank1 1939, antitank2 1940 | AT before 1939 for POL/CHI vs armour |
| Motorised | tech_trucks, motorised_infantry (1936), mechanised_infantry 1940 | trucks gate logistics (section 7) |
| Armor | The folder depends on No Step Back (`armour_folder` without it, `nsb_armour_folder` with it). With NSB: gwtank_chassis, basic_light_tank_chassis, improved_light_tank_chassis 1936, basic_medium_tank_chassis (1938, cost 2, needs only gwtank_chassis), improved_medium_tank_chassis 1940; armor_tech_1/2/3 (1936/38/41, cost 1) and engine_tech_1/2/3 (1934/38/41) modules; equipment are chassis designs (`light_tank_chassis_2`, `medium_tank_chassis_1`). Without NSB: gwtank/basic_light_tank/improved_light_tank, basic_medium_tank 1939, improved_medium_tank 1941 (`medium_tank_equipment_1`). | modules are cheap, high value; a research list must use the keys of the folder that is shown |
| Railway | basic_train 1910, wartime_train 1939 | rail throughput |

### 3.4 Air and naval

Air: the two sets are exclusive (`air_techs_folder` without By Blood Alone, `bba_air_techs_folder` with it). With
BBA: airframes iw_/basic_ (1936)/improved_ (1940)/advanced_ (1944) small, medium and large; engines_1 1922,
engines_2 1936, engines_3 1940; modules aa_lmg 1922, aa_hmg 1939, range_improvements 1936, armor_piercing_bombs 1936,
heavy_bombs 1940; equipment are airframe designs (`small_plane_airframe_1` fighter, `small_plane_cas_airframe_1` CAS,
`medium_plane_airframe_1` bomber). Without BBA: fighter1 1936, fighter2 1940; CAS1 1936, CAS2 1940; tactical_bomber1
1936, 2 1940; heavy_fighter1/2, naval_bomber1, strategic_bomber1 (`fighter_equipment_1`, `CAS_equipment_1`).
Naval (only if the navy matters): basic_destroyer / light_cruiser / submarine 1936, improved 1940; transport 1922 for
any invasion; landing_craft 1940 (cost 2).

### 3.5 Role-based ordering

| Role | Order of focus |
|---|---|
| Industrial major (GER) | industry > construction > infantry weapons > artillery > electronics > support > armor > air > naval |
| Poor/large (CHI) | construction > infantry weapons > support (engineers, logistics) > artillery > cheap AT/AA > industry; skip naval entirely |
| Defensive small (POL) | industry+construction > infantry weapons > AT > artillery > fighter1/CAS1 > motorised; no naval beyond basics |
| Generic minor | infantry weapons, support, artillery, construction; licence to avoid wasted slots |

## 4. Dated research checklist 1936-1941

Slot counts: GER 4 (5 with Heraeus focus), POL 3 (+2 via focus), CHI 2 (+3 via focus chain), generic 3. Keep every
slot busy; the standing plan fills an idle slot from its lists (`hoi4_plan_run`, section 8).

### 4.1 Germany (FAIR)

| When | Queue (one per slot) | Note |
|---|---|---|
| 1936 Jan | basic_machine_tools, infantry_weapons1, construction1, electronic_mechanical_engineering | GER lacks all four at start; no tech bonus needed |
| 1936 mid | interwar_artillery, interwar_antitank, tech_logistics_company (fighter1 only without BBA; with BBA basic_small_airframe and engines_2 are start techs) | |
| 1937 | improved_machine_tools, construction2, concentrated_industry (then 2), tech_maintenance_company, armor_tech_1 (NSB; engine_tech_1 is a start tech with NSB), range_improvements / armor_piercing_bombs (BBA) or CAS1 (without BBA) | focus bonuses: industry 1x1 (`GER_industrial_expansion`), etc. |
| 1938 | infantry_weapons2, support_weapons2, improved_radio, engine_tech_2, armor_tech_2, basic_medium_tank_chassis, tech_engineers2 if engineers used | `GER_advanced_panzer_research` gives +100% once for cat_medium_armor |
| 1939 | improved_infantry_weapons, artillery1, antiair1, advanced_machine_tools, construction3, wartime_train, aa_hmg (basic_medium_tank only without NSB) | `GER_artillery_bombardment` 0.5 x2 artillery |
| 1940 | improved_infantry_weapons_2, artillery2, antitank2, mechanised_infantry, engines_3, improved_small_airframe + improved_medium_airframe (BBA) or fighter2 + tactical_bomber2 (without BBA) | |
| 1941 | improved_medium_tank_chassis (NSB, 1940) or improved_medium_tank, artillery3, support_weapons3, assembly_line_production, centimetric radar after sp_air_radar if air war, super_heavy only if resourced | |

The standing plan `plans/germany-fair-1936-1938.plan.json` implements this table for NSB + BBA + La Resistance + AAT +
Götterdämmerung.

### 4.2 Nationalist China (FAIR)

| When | Queue |
|---|---|
| 1936 | infantry_weapons1, construction1, basic_machine_tools (2 slots until the focus chain) |
| 1937 (war with Japan) | tech_engineers, interwar_antitank, tech_support line, support_weapons |
| 1938 | infantry_weapons2, interwar_artillery, tech_logistics_company, construction2 |
| 1939 | improved_infantry_weapons, tech_trucks only if truck production exists, antiair1 |
| 1940-41 | artillery1-2, antitank1-2, infantry_weapons upgrades; ignore tanks and air unless licence-produced |

Reason: CHI starts with only infantry_weapons, gw_artillery, interwar_antiair and basic_train. It is slot-starved, so
it avoids ahead-of-time techs entirely.

### 4.3 Poland (FAIR)

| When | Queue |
|---|---|
| 1936 | basic_machine_tools, infantry_weapons1, construction1 (focus `POL_the_four_year_plan` gives 1.0 x1 construction_tech) |
| 1937 | interwar_antitank, interwar_artillery, improved_machine_tools, fighter1 |
| 1938 | infantry_weapons2, support_weapons2, antitank1 (focus `POL_anti_blitz_tactics` 1x2 cat_anti_tank), antiair1, tech_engineers |
| 1939 (before September) | improved_infantry_weapons, artillery1, fortification line (basic_fortification_tech, land_fort_tech_1 1938) |

Poland's own focuses give tech bonuses: infantry_weapons 1.0 x1 twice, armor 1.0 x1, motorized 2.0 x1, artillery
1.0 x1, cat_anti_tank 1.0 x1. Time those focuses before queuing the matching techs.

### 4.4 Generic nation (any other tag)

1936: basic_machine_tools, infantry_weapons1, construction1. 1937: interwar_artillery/antitank, engineers,
concentrated_industry. 1938: infantry_weapons2, support_weapons2, radio. 1939: improved_infantry_weapons, artillery1.
1940: next tiers. The 1936 start grants different techs per tag: read the completed techs first (`hoi4_research`,
`hoi4_technology_list` with `available:true`, `facts.py research-calendar <TAG>`).

## 5. Special projects, scientists, facilities

Mechanism:

1. Build a specialization facility in a state with a free slot: `land_facility`, `air_facility`, `naval_facility`, `nuclear_facility` (cost 10000 + 6000 per owned facility, one per state, needs supply, can be captured).
2. Recruit scientists: 25/50/75/100 PP by how many are idle. Level thresholds 100/100/300/700/1500 XP; speed -100%, -5%, +5%, +10%, +15%, +25% by skill level. Traits act only on the project they work on (genius +10% project speed; aircraft, armour, artillery, submarine and nuclear specialists).
3. Basic research (scientist in a matching facility without a project): earns breakthrough progress (growing with skill) and a research bonus for that specialization; the scientist gains 0.28 XP a day.
4. Breakthrough points per specialization (air/land/naval/nuclear) are spent when starting a project (e.g. radar 1 air, jet engine 3 air). A project cancelled before an iteration completes refunds the points.
5. A project runs a prototype phase (progress to 100, iterations). Time presets: short 45, medium 60, long 100, very_long 200 days; iterations: small 15-25, medium 15-20, large 10-15, insane 5-10. It needs resources (steel/aluminium/chromium/tungsten); a resource or supply shortage slows it to at least 20%.
6. Rewards: each iteration rolls generic or unique prototype rewards (treat them as random); finishing grants the project output (equipment, modules, subunits, design bonuses, state effects).
7. Supportive scientist: a faction partner's scientist works at 25% efficiency, gives the owner 10% of an iteration's progress and +5% research sharing in the faction.

Projects relevant to our roles:

| Project | Spec | Tech gate | Breakthrough | Time/complexity | Use |
|---|---|---|---|---|---|
| sp_air_radar | air | electronic_mechanical_engineering | air 1 | medium/medium | best early project for majors |
| sp_land_flamethrower_tank | land | tech_engineers | land 1 | short/small | cheap, GER/ITA fortress breaking |
| sp_land_military_engineering_vehicles | land | tech_engineers | land 1 | short/small | engineers breakthrough support |
| sp_land_large_caliber_kinetic_energy_sabot | land | interwar_antitank | land 1 | short/small | AT boost for POL/CHI |
| sp_land_super_heavy_howitzer | land | gw_artillery | land 2 | short/medium | siege; low value |
| sp_land_stronghold_network | land | basic_fortification_tech | land 2 | medium/large | defence, POL/CHI |
| sp_naval_cruiser_submarine / midget | naval | early submarine | naval 1 | medium or short | GER only if U-boats |
| sp_air_jet_engine | air | engines_2 + fighter2 etc. | air 3 | long/large | 1942+ only |
| sp_rockets_*, sp_nuclear_* | air/nuclear | 1942+ techs | | | out of 1936-41 scope |

No matching facility means the project cannot start. GER builds an air facility first (radar), then land; POL builds
one land or air facility only with a factory surplus; CHI builds none before 1940 (spend construction on factories
and infrastructure).

Rule: an idle facility's scientist does basic research (free bonus). Do not start a project whose resources the
nation lacks (e.g. tungsten, chromium for China): it crawls at the 20% floor.

## 6. MIOs and equipment variants

### 6.1 MIO selection

- Country organisations replace generic ones: GER has Mauser/Walther (infantry weapons), Rheinmetall (artillery), Opel/Daimler (vehicles), Porsche/Henschel/MAN/Alkett (tanks), Messerschmitt/Junkers/Focke-Wulf/Heinkel/Dornier (air); CHI has Hanyan arsenal (infantry equipment), Taiyuan arsenal (artillery), Liaoning (motorized), Shanghai arsenal and Sino-German technical mission (armoured cars/light tank); POL has PZInz (infantry tank), PZL (air), PWS, Gdynia shipyard. The generic infantry-equipment MIO is blocked for GER, and for POL/CHI when their DLC trees exist. `facts.py mio <TAG>` and `hoi4_mios` list the keys.
- Unlock order: infantry equipment, artillery, support equipment, motorised, tank, air.
- A size-up costs 700 + 100 x (size + 1)^1.8 funds and gives 1 trait point. Funds: lines with the MIO as manufacturer (0.1 per IC a day, cap 100 a day) and tech completions with its design team.
- Traits: production first (efficiency cap, then cost or output, growth last), then the role stat (soft attack in single-player; hard attack and piercing against tanks). Generic infantry MIO: `mass_rifle_production`/`bolt_action` for cheap production or `semi_auto` for quality (exclusive).
- Assign only MIOs that match active techs and variants.

### 6.2 Variants and upgrades

- Land equipment uses the module designer (tank/vehicle modules) and the legacy upgrade system for infantry gear and guns. Costs: legacy upgrade 10 army XP rising per total level; module add 5, replace 6, convert 3, remove 1 XP.
- Upgrade (legacy) infantry equipment and artillery in place (production continues; stock upgrades gradually). Create a new variant only when a new chassis or tech arrives (e.g. basic_medium_tank) or the module set changes. A line switched to a variant of the same model keeps 90% efficiency, same family 70%, a parent/child tier 30% (`templates-equipment-production.md` §5).
- Licensed equipment (§1): only worth it for CHI/POL when a faction partner has much better gear.
- A new chassis or tier: start it on new or idle factories and switch the old line whole when old stock covers about 30 days of field demand (`templates-equipment-production.md` §5).

## 7. Tech that gates capabilities

| Capability | Tech / project |
|---|---|
| Combat engineers (fortification, river crossing), land projects | tech_engineers (1936, 1.25), tech_engineers2 (1939) |
| Trucks and motorised divisions | tech_trucks, motorised_infantry (1936), mechanised_infantry (1940) |
| Supply throughput: logistics company, trains | tech_logistics_company (1936), basic_train, wartime_train (1939) |
| Maintenance and recovery | tech_maintenance_company (+2) |
| Signals/command | tech_signal_company |
| Radar (air detection) | electronic_mechanical_engineering, sp_air_radar, radio_detection |
| Decrypt/encrypt | computing_machine 1938, basic_encryption/decryption 1938 (without La Resistance) |
| Fortifications | basic_fortification_tech 1936, land_fort_tech_1 1938, _2 1940 |
| Naval invasions | transport (1922), landing_craft (1940) |
| Paratroopers/marines | paratroopers, marines (1936) |

## 8. Automation split

### (a) Automatable standing rules

1. Keep every research slot busy: the plan's `research_path` / `research_priority` fill idle slots (`hoi4_plan_run`); by hand `hoi4_research_start`. Skip techs already researched or already in a slot.
2. Prefer in-year techs; ahead of time only when nothing in-year remains or an ahead-of-time bonus covers it (§1).
3. A focus with `add_tech_bonus`: hold the matching tech until the bonus is active (plan `research_bonuses`; `hoi4_research_extras` shows uses left).
4. Army XP: a subdoctrine per §2.2 when XP >= 100 and a track is empty (plan `doctrines`, `hoi4_doctrine_adopt`); otherwise keep 35 XP for variants.
5. A new focus option with `add_research_slot`: pick it when no survival-critical focus is pending.
6. A facility with an idle scientist: basic research (`hoi4_special_project_assign`).
7. After each tech finishes, re-read completed techs (`hoi4_research`) and pick the next.
8. Report in the journal when completed techs cannot be read.

### (b) Needs live decisions

- Concentrated vs Dispersed industry (exclusive) and mutually exclusive MIO traits (bolt_action vs semi_auto).
- Subdoctrine choice when the country is mid-war with a changed front (tank-heavy vs infantry-heavy).
- Whether to build a facility (competes with factories/infrastructure) and which special project to start.
- Variant design choices (modules per chassis, role of the tank).
- Whether to licence equipment from a partner.
- Switching the research focus after losing major territory (CHI) or after Poland falls.

### (c) Tools

| Need | Tool |
|---|---|
| Completed techs, raw research counters | `hoi4_research` |
| Tech catalogue, what is researchable now | `hoi4_technology_list` (`available:true`), `hoi4_research_nodes`, `hoi4_research_node_detail` |
| Research slots on screen | `hoi4_research_ui` |
| Start a tech (or replace one in a busy slot) | `hoi4_research_start` |
| Research bonuses, cost reductions, tech sharing | `hoi4_research_extras` |
| Doctrines, mastery, XP pools, adoptable doctrines; adopt | `hoi4_doctrines` (`include_catalog`), `hoi4_doctrine_adopt` |
| Special projects: read, recruit, assign, stop | `hoi4_special_projects` (`rules`), `hoi4_special_projects_panel`, `hoi4_special_project_recruit`, `hoi4_special_project_assign`, `hoi4_special_project_unassign`, `hoi4_special_project_stop`; facilities with `hoi4_construction_add` |
| MIOs | `hoi4_mios`, `hoi4_mio_trait`, `hoi4_mio_policy`, `hoi4_mio_design_team` |
| Designs and variants | `hoi4_designs`, `hoi4_design_options`, `hoi4_design_create`, `hoi4_design_set_production`, `hoi4_design_obsolete` |
| Slot focuses | `hoi4_focus_list`, `hoi4_focus_start` |
| Standing plan | `hoi4_plan`, `hoi4_plan_run` |
| Offline facts | `facts.py tech`, `facts.py research-calendar`, `facts.py focus`, `facts.py projects`, `facts.py mio` |

No tool yet:
- research start without spending a bonus
- research progress, days left and ahead-of-time penalty reads
- army exercises and officer-corps spirits
- MIO line assignment and per-MIO funds, size and trait reads
- faction initiatives (doctrine and tech sharing)
- licence dialogs; equipment conversion

## 9. Version and DLC notes

- DLC changes which subdoctrines, organisations, tank and air folders and focus ids exist: read the live catalogue
  (`hoi4_doctrines include_catalog`, `hoi4_mios`, `hoi4_technology_list`) before writing ids into a plan.
- Subdoctrine rewards vary: read the tooltip before buying.

## 10. Tactics: research, doctrines and MIOs

| ID | Tactic | How it works | When | Mode |
|---|---|---|---|---|
| RE01 | Ahead-of-time costs a lot | Research speed = 1 / (2 x years ahead + 1): half a year ahead takes twice as long, one year three times; normal bonuses do not reduce it, only ahead-of-time bonuses do. Fill slots with in-year techs of other folders; pull a tier forward only when nothing in-year remains, and say so in the plan | Every research plan | FAIR |
| RE02 | Big bonuses on big techs | Spend a limited 100% industry bonus on Tools 4, Construction 4 or Concentrated 2-3, not on a cheap tech; a +10% bonus on a 100-day tech saves about 9 days, 50% to 100% only about 17 | Focus bonuses | FAIR |
| RE03 | Research without spending a bonus | 1.19.4: start a tech without consuming the matching bonus (shift-right-click) to keep it for the right tech | 1.19.4 | FAIR |
| RE04 | Never idle | An idle slot keeps up to 30 days of points; leave one idle at most 30 days and only before a big tech | Always | FAIR |
| RE05 | Industry order | Construction 1-3 on time, construction 4 is weak by 1941; tools and industry at or slightly ahead of their year; electronics adds +3-8% speed per computing tech | All | FAIR |
| RE06 | Keep the grand doctrine | A new grand doctrine resets mastery in every track; mastery is capped at 40 a month per track; a full track banks 25% up to 200 | Doctrine planning | FAIR |
| RE07 | Mastery multipliers | Track multipliers: infantry x1, combat support x8, armour x10; training gives 10% mastery | Doctrine planning | FAIR |
| RE08 | 1.19.4 land doctrine choice | Mass Assault is the defensive pick (-0.2 infantry width milestone, -15% supply use); Grand Battle Plan the offensive one (+10% max planning, +25% command power gain); Mobile Warfare +20% planning speed; Superior Firepower +10% line artillery soft attack; Large Unit Tactics -0.2 infantry width | Land doctrine | FAIR |
| RE09 | Naval and air doctrine follow the build | Trade Interdiction + wolfpacks for a submarine war; Base Strike for carriers; Fleet in Being for capitals; patrol_boats (50 XP) or torpedo_primacy for destroyer navies; convoy_escort only to protect routes; naval_gunfire_support (+25% bombardment, +20% invasion planning) before landings; Battlefield Support for CAS-led majors, Operational Integrity for minors; Dispersed Operations for countries without an air force | Doctrine planning | FAIR |
| RE10 | Attachés | 100 PP + 50 command power; 15% of the attaché's XP and 10% of the host's mastery; +10% war support; the AI accepts at opinion 20+ | Neutrals and democracies while others fight | FAIR |
| RE11 | Theorist and chiefs | Theorist before an expensive doctrine (-10% cost); chief of army gives 0.2/0.3/0.4 army XP a day by tier | Early game | FAIR |
| RE12 | Tech sharing and licences | Tech-sharing bonus up to 50%; producing under licence gives +20% research on the matching tech | Faction members | FAIR |
| RE13 | MIO on every line | A line's MIO costs nothing and earns 0.1 funds per IC a day (cap 100); a design team on research costs 0.1 PP a day, gives +5% speed and earns about five times the funds of production | AAT | FAIR |
| RE14 | MIO trait order | Production first (cap, then cost or output, growth last), then the role stat (soft attack in single-player, hard attack and piercing against tanks); about 6 traits by the war; skip cost-penalty traits | AAT | FAIR |
| RE15 | Design MIO and production MIO | Combat-stat MIO on the design, production MIO on the line; two MIOs level more slowly | SOV, GER, USA tank and plane lines | FAIR |
| RE16 | MIO policies | Queue traits so the size-6 policy prompt appears; policies cost 25 PP and lock 180 days; Assembly Line Optimisers by default, Cutting Corners for tanks | AAT | FAIR |
| RE17 | MIO traits are not retroactive | Re-apply the MIO to the variant, switch the line, convert the stock | AAT | FAIR |
| RE18 | Naval techs when building ships | The research fallback skips naval techs: a plan that builds ships lists the sub hull, snorkel, torpedo, transport and landing-craft and radar techs itself | Navies | FAIR |
| RE19 | Map the bonuses first | List every focus and faction research bonus and pin each to its target before the year starts: 100% industry bonuses on Tools 3-4 or Concentrated 4, the faction's two 50% industry bonuses on Concentrated 3-4 or Tools 4, its 50% excavation bonuses on Excavation 2-3, two-year ahead-of-time bonuses on Tools 5 and the 1941 AT tech, tank focus bonuses on the improved chassis | Every major | FAIR |
| RE20 | Fixed-date ahead-of-time | Start the 1941 AT tech about October 1939 and the 1941 radio about December 1939 (Germany: AT 3 about January 1941 via early AT work from mid-1937, radio 3 March 1940); otherwise avoid ahead-of-time research | Majors preparing 1941 wars | FAIR |
| RE21 | Skip techs the build does not use | An armour nation skips line-artillery equipment techs (radar instead); a carrier navy skips battleship techs; research only the module techs the planned designs need | Research plan | FAIR |
| RE22 | MIO policy timing | Assembly Line Optimisers while the MIO's lines produce; Armor Welding Specialists once its tanks stop; Cutting Corners on the main tank designer at level 6, then upgrade its equipment once and never again; switch "update variants with the latest design team traits" off; save trait points for traits a focus unlocks | AAT | FAIR |
| RE23 | Bank XP and manpower on starter subdoctrines | Take the subdoctrines whose rewards come early (Guerilla War: war support and stability; Irregulars and Desperate Defense: manpower), max them, then switch to the war doctrine (Grand Assault, Individual Excellence) before the war; a war-support decision that grants army XP on the Spanish Civil War pays for it | 1936-39, all majors | FAIR |
