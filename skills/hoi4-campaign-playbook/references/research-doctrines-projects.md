# Research, doctrines, special projects, MIOs, variants (HOI4 1.19.3, FAIR)

Scope: Normal difficulty, historical AI, no exploits. Every rule names the proving file.
`G` = `.../Steam/steamapps/common/Hearts of Iron IV`. Tags: [V] verified in game files,
[W] wiki/common knowledge, not re-verified here, [?] uncertain or version dependent.
Where this file and the on-screen tooltip disagree, the tooltip wins; log the difference.

## 1. Core mechanics (read first)

| Fact | Value | Proof |
|---|---|---|
| Base slots (no history override) | 2 | G/common/defines/00_defines.lua `BASE_RESEARCH_SLOTS = 2` [V] |
| 1936 start slots | GER 4, ENG 4, ITA 4, POL 3, FRA 3, SOV 3, JAP 3, CHI none set so 2 [?] | G/history/countries/*.txt `set_research_slots` [V]; CHI has no line, so base 2, confirm with `hoi4_research slots` |
| GER 1939 bookmark | 5 slots pre-granted (comment: Heraeus focus done) | G/history/countries/GER - Germany.txt ~l.454 [V] |
| Tech cost | `research_cost` in tech file x `BASE_TECH_COST` 110 x ahead-of-time factor | defines `NTechnology` [V]; exact days-per-point [?]. Trust the cost in `hoi4_research list`. |
| Ahead-of-time | `BASE_YEAR_AHEAD_PENALTY_FACTOR = 2`; penalty grows with each year the tech start_year is ahead of the date | defines [V]; exact curve [W] |
| Focus tech bonus | `add_tech_bonus {bonus=X uses=N category=...}`: next N researches in the category get +X research bonus AND a reduction of the ahead-of-time penalty | G/localisation/english (EFFECT_ADD_TECH_BONUS) [V] |
| Idle slot carry | `BASE_RESEARCH_POINTS_SAVED = 30` per slot (progress that survives short idle) | defines [V]; mechanics [?] |
| Min research speed | 0.1 | defines [V] |
| Tech sharing cap | `MAX_TECH_SHARING_BONUS = 0.5` (instant). A sharing group member that already has a tech speeds it for others | defines + technology_sharing_l_english.yml [V] |
| Licence production | `LICENSE_PRODUCTION_TECH_BONUS = 0.2` (research bonus when you license-produce), licence MIC speed -25% base, -5%/yr behind, -10% outside faction, upgrading licensed gear costs 2x XP | defines l.658-666, l.712 [V] |

Doctrines are NOT technologies in 1.19. They cost XP, not research slots
(G/common/doctrines/grand_doctrines `xp_cost`/`xp_type`) [V]. The tool `hoi4_research list`
deliberately excludes doctrine folders (hoi4_live/tools/economy_tools.py l.77) [V].

### 1.1 How to get more research slots (ordered by reliability)

1. National focus `add_research_slot = 1`. Per tag [V, G/common/national_focus]:
   - GER: `GER_develop_heraeus_facilities` (needs `GER_kdf_wagen_factories` first) in germany.txt; a second slot only in the Austrian-monarchy branch (`GER_reverse_the_brain_drain`), ignore.
   - POL: `POL_additional_research_slot1` (after `POL_the_four_year_plan`) and `POL_additional_research_slot2` (needs more than 50 owned factories) in poland.txt.
   - CHI (nationalist tree, china_nationalist.txt): `CHI_rural_schooling`, `CHI_expand_the_academica_sinica`, `CHI_chemical_research_institute` each give a slot. Other DLC variants of the tree (china_nationalist_sea.txt, Waking the Tiger) use different ids: `CHI_unify_the_admissions_process`, `CHI_expand_academica_sinica`, `CHI_expand_national_sun_yat_sen_university` [V]. Check which tree the save has via the focus tool.
   - Generic tree: `extra_tech_slot` after `infrastructure_effort_2` (generic.txt) [V].
2. Technology-sharing focuses and a few events; 231 focus files use `add_research_slot` [V count], so always read the focus list for "research" icons.
3. Scientists/special-project facilities do not add tech slots; they add a separate program track (section 5).
4. No cheap non-focus slot source was found in common/ideas for the three target tags [?].

Rule: for GER and POL the slot focus is high priority (earliest affordable). A 4th/5th/6th slot beats most
single-tech focuses. For CHI prioritise the cheapest slot chain, but not before survival-critical focuses.

### 1.2 Research speed and bonus sources

- Scientists do not speed normal techs. Only facility "basic research" gives a tech bonus: `BASIC_RESEARCH_TECHNOLOGY_BONUS_FACTOR = 0.02` per scientist skill level, with diminishing returns (0.5) for a scientist shared across facilities [V, defines NProject].
- MIO design team assigned to a matching research slot: `DESIGN_TEAM_RESEARCH_BONUS = 0.05` (some orgs set `research_bonus`), costs 0.1 PP/day per assignment [V, defines NIndustrialOrganisation]. Completing a tech with a design team also adds MIO funds (500 x research_cost).
- Focus/idea bonuses: GER `GER_industrial_research_speed_factor` 5% in the industry branch (germany.txt l.548) [V].
- Subdoctrine rewards can grant tech bonuses (e.g. `mobile_infantry` immediate: +50% bonus, 1 use, motorized_equipment; `irregulars` last reward: +50% x2 support_tech) [V, subdoctrines/land/*.txt].

Rule: never start an ahead-of-time tech while a normal on-date tech that gates equipment is available. Exception: a focus bonus
currently covers it (bonus listed in section 3 tables) or the tech is cheap (cost <= 1.5) and gates a capability.

## 2. Doctrines in 1.19 (exact)

Structure [V, G/common/doctrines/_documentation.md and grand_doctrines/_documentation.md]:

- Folder: land, naval, air, special_forces (special_forces needs NCNS or Arms Against Tyranny DLC) [V, folders/doctrine_folders.txt].
- Grand doctrine: one per folder, paid with XP (all `xp_cost = 100`, type army/navy/air) [V]. Gives immediate bonuses and unlocks 4 tracks (land: infantry, combat_support, armor, operations; air: fighter, strike, medium, heavy aircraft; naval: capital_ships, carriers, screens, submarines).
- Track: a slot. Put one subdoctrine in each (xp 100 each; the cheap special ones cost 50: irregulars, peoples_war, great_war_infantry, patrol_boats) [V].
- Mastery: the track fills automatically. Each reward needs 100 mastery by default (`DEFAULT_REWARD_MASTERY`; subdoctrines can override, e.g. peoples_war 50) [V]. Finishing all rewards in a track triggers the grand doctrine's milestone for that track [V].
- Mastery source: manpower of units in the track's categories (combat, missions, one tenth from training: `TRAINING_MASTERY_GAIN_FACTOR = 0.1`). Diminishing returns beyond 100,000 contributing manpower; hard cap `MAX_MONTHLY_MASTERY_GAIN = 40`; surplus when a track is full is banked at 25%, bank max 200 [V, defines NDoctrines]. Track multipliers: infantry x1, combat_support x8, armor x10 [V, tracks/land_doctrine_tracks.txt], so armor and support tracks need far fewer men.
- Doctrine cost modifiers `<folder>_doctrine_cost_factor`, mastery gain `<folder>_track_mastery_gain_factor` exist [V].
- Faction doctrine sharing: unlock costs 1 faction initiative per folder, base +10 mastery/month plus 2 per theater commander [V, defines l.4570-4572].
- Cannot hold subdoctrines marked `xor` in different tracks simultaneously; replacing within the same track is allowed (costs XP again) [V, subdoctrines/_documentation.md].

### 2.1 What each country starts with (1936 start)

| Tag | Grand doctrines set at 1936 | Proof |
|---|---|---|
| GER | new_mobile_warfare (land), new_convoy_raiding (naval), new_battlefield_support (air) | history/countries/GER - Germany.txt l.34-36 [V] |
| CHI | mass_assault (land), new_fleet_in_being (naval); air grand doctrine only at 1939 | CHI - China.txt l.217-218, 859-862 [V]. Also sets large_unit_tactics + 200 mastery at 1939. |
| POL | grand_battleplan (land), new_fleet_in_being (naval); air only at 1939 (new_strategic_destruction) | POL - Poland.txt l.50, 245-247 [V] |
| FRA | grand_battleplan, new_fleet_in_being (+air at 1939) | FRA l.130-131 [V] |
| SOV | mass_assault, new_fleet_in_being | SOV l.798 [V] |
| ITA, JAP | grand_battleplan; ITA fleet_in_being, JAP base_strike | ITA l.369, JAP l.463 [V] |

Consequence: at 1936 nobody spends XP on land grand doctrines. The first army XP goes into track subdoctrines. CHI and POL
may also want an air grand doctrine (100 air XP) once any air XP exists.

Land grand doctrine one-liners [V, grand_doctrines/land_grand_doctrines.txt]:
mobile_warfare = +20% planning, +10% army speed, Unexpected Thrust tactic; superior_firepower = +10% line artillery SA, +5% support
artillery SA; grand_battleplan = +10% max planning, +25% command power gain, Planned Attack tactic; mass_assault =
48h no-supply grace, -10% out-of-supply penalty, Relentless Assault tactic. Re-picking a grand doctrine costs 100 XP again;
do not.

### 2.2 Subdoctrine choice (army XP 100 each; read the tooltip first, DLC gates [V])

AI weights show which fit which nation (ai_will_do in subdoctrine files) [V]. For the agent:

| Track | Germany (mobile warfare) | China (mass assault) | Poland (grand battleplan) | Generic major / minor |
|---|---|---|---|---|
| Infantry | mobile_infantry (+10% breakthrough, free +50% motorized tech bonus) | large_unit_tactics; peoples_war only if communist | defensive_postures (dig-in, width) | defensive_postures (defender), assault_infantry (attacker); minors: irregulars (50 XP, NCNS) |
| Combat support | mobile_recon_and_assault (La Resistance) else fire_concentration | fire_concentration | fire_concentration or anti_tank_frontline vs Germany | fire_concentration default |
| Armor | armored_spearhead (La Resistance; `_no_lar` otherwise) | streamlined_deployment (only if armor exists, else delay) | armored_infantry_support / mobile_defense, low priority | by tag weights |
| Operations | mission_type_tactics early; deep_battle later | last_stand is AI weight 0; prefer infiltration_tactics/mission_type_tactics | mission_type_tactics | mission_type_tactics |

Notes: `deep_battle` and `armored_spearhead` have `_no_lar` twins for owners without La Resistance; only one variant is visible
[V, operations/armor files]. Poland and China rarely field enough armor for the armor track, so leave it last.
Air (needs air XP): fighter track = air_subdoctrine_fighter_homeland_defense (defender) / escort_fighter (offence);
strike_aircraft = flying_artillery for CAS-heavy GER; medium = tactical_battlefield_support; heavy = skip unless bombers exist [V ids, air/*.txt; value judgement W].
Naval: GER stays convoy_raiding; subdoctrines wolfpacks (submarines), convoy_escort (screens) [V ids]. Navy choices matter only if a navy is built.

Spend order for army XP (XP also pays variants, section 6): (1) infantry subdoctrine, (2) operations, (3) combat support,
(4) armor. Keep >= 35 XP reserve for variant creation (mirrors AI cutoff `DEFAULT_*_VARIANT_CREATION_XP_CUTOFF_LAND = 35`, defines l.2465) [V].

When NOT to apply: do not swap subdoctrines mid-war unless the current track's rewards are idle (mastery restarts for the new subdoctrine); do not buy a
subdoctrine whose `available` is false (greyed in UI).

## 3. Research priority tables

Costs below are `research_cost` from G/common/technologies (multiply by 110 for points [?]). Years are `start_year`; researching earlier than the year is ahead of time.

### 3.1 Industry and infrastructure (always first) [V industry.txt]

| Tech | Year | Cost | Why |
|---|---|---|---|
| basic_machine_tools | 1936 | 1.5 | factory output; never skip |
| construction1 | 1936 | 2 | construction speed (also gates facility/infrastructure line) |
| improved_machine_tools / construction2 | 1937 | 1.5 / 2 | |
| excavation1-2 | None/1937 | 2 | resource extraction (only if mine-rich) |
| concentrated_industry (or dispersed) | None/1937 | 2 | pick one branch by country; XOR with the other |
| improved_equipment_conversion | 1937 | 2 | cheaper convert of old stock |
| advanced_machine_tools / construction3 | 1939 | 2 | |
| fuel_silos, fuel_refining | 1936 | 1 / 3 | fuel for GER |
| rubber/oil processing (synth) | 1937+ | 1.5 | only for GER/JAP/ITA style resource-starved nations |

### 3.2 Electronics, computing, radar, encryption [V electronic_mechanical_engineering.txt]

electronic_mechanical_engineering (1936, cost 1) is the root; it also gates `sp_air_radar` special project (`FROM = { has_tech = electronic_mechanical_engineering }` in radar_projects.txt) [V]. Then: radio (cost 2, root of signals), improved_radio 1938 (gates sp_naval_proximity_fuze), radio_detection (cost 1.75, radar line), computing_machine 1938 (cost 2.5), basic_encryption/decryption 1938, cavity_magnatron 1938, centimetric_radar 1940. Research radar line with the Germany knickebein bonus only if airfields matter; for CHI/POL computing is low priority.

### 3.3 Land equipment lines [V infantry/support/artillery/armor.txt]

| Group | Sequence | Notes |
|---|---|---|
| Infantry weapons | infantry_weapons1 (1936, 1.5), infantry_weapons2 (1938, 1.5), improved_infantry_weapons (1939, 2), improved_infantry_weapons_2 (1940, 2) | top priority; every division uses them. GER 1936 only has base infantry_weapons. |
| Support weapons | support_weapons (1918 base, 1.5), support_weapons2 (1938), support_weapons3 (1940) | |
| Support companies | tech_engineers 1936 (1.25), tech_recon, tech_military_police, tech_maintenance_company, tech_field_hospital, tech_logistics_company, tech_signal_company (all 1936, ~1.25), second tiers 1939 (1.5) | engineers and logistics first (section 7) |
| Artillery | interwar_artillery 1936 (1.5), artillery1 1939, artillery2 1940, artillery3 1941 | GER 1936 has gw_artillery only |
| Anti-air / anti-tank | interwar_antiair 1936, antiair1 1939; interwar_antitank 1936, antitank1 1939, antitank2 1940 | AT before 1939 for POL/CHI vs armor |
| Motorised | tech_trucks, motorised_infantry (1936), mechanised_infantry 1940 | trucks gate logistics (section 7) |
| Armor | gwtank/basic_light_tank/improved_light_tank 1936; basic_medium_tank (1939, cost 2) + basic_medium_tank_chassis (1938); improved_medium_tank 1941; armor_tech_1/2/3 (1936/38/41, cost 1) and engine_tech_1/2/3 (1934/38/41) modules | modules are cheap, high value; chassis and gun/vehicle tech separate |
| Railway | basic_train 1910, wartime_train 1939 | rail throughput |

### 3.4 Air and naval [V air_techs.txt, bba_air_techs.txt, naval.txt]

Air: fighter1 1936, fighter2 1940; CAS1 1936, CAS2 1940; tactical_bomber1 1936, 2 1940; heavy_fighter1/2, naval_bomber1 1936, strategic_bomber1 1936; airframes basic_small_airframe 1936, improved_small_airframe 1940; engines_1 1922, engines_2 1936, engines_3 1940; modules aa_lmg 1922, aa_hmg 1939, range_improvements 1936.
Naval (only if navy matters): basic_destroyer/ light_cruiser/ submarine 1936, improved 1940; transport 1922 for any invasion; landing_craft 1940 (cost 2).

### 3.5 Role-based ordering

| Role | Order of focus |
|---|---|
| Industrial major (GER) | industry > construction > infantry weapons > artillery > electronics > support > armor > air > naval |
| Poor/large (CHI) | construction > infantry weapons > support (engineers, logistics) > artillery > cheap AT/AA > industry; skip naval entirely |
| Defensive small (POL) | industry+construction > infantry weapons > AT > artillery > fighter1/CAS1 > motorised; no naval beyond basics |
| Generic minor | infantry weapons, support, artillery, construction; license to avoid wasted slots |

## 4. Dated research checklist 1936-1941

Slot counts: GER 4 (5 with Heraeus focus), POL 3 (+2 via focus), CHI 2 (+3 via focus chain), generic 3. Always keep every slot busy; start the next queued entry the day a slot idles (`hoi4_research` waits per section 8).

### 4.1 Germany (FAIR)

| When | Queue (one per slot) | Source of order |
|---|---|---|
| 1936 Jan | basic_machine_tools, infantry_weapons1, construction1, electronic_mechanical_engineering | GER lacks all four at start (GER - Germany.txt l.86-101) [V]; no tech bonus needed |
| 1936 mid | interwar_artillery, interwar_antitank, tech_logistics_company, fighter1 (+ small airframe) | |
| 1937 | improved_machine_tools, construction2, concentrated_industry (then 2), tech_maintenance_company, CAS1, armor_tech_1, engine_tech_1 | focus bonuses: industry 1x1 (`GER_industrial_expansion`), etc. |
| 1938 | infantry_weapons2, support_weapons2, improved_radio, basic_medium_tank_chassis, tech_engineers2 if engineers used | `GER_advanced_panzer_research` gives +100% bonus once for cat_medium_armor [V] |
| 1939 | basic_medium_tank, improved_infantry_weapons, artillery1, antiair1, advanced_machine_tools, construction3, wartime_train | `GER_artillery_bombardment` 0.5 x2 artillery |
| 1940 | improved_infantry_weapons_2, artillery2, antitank2, fighter2, tactical_bomber2, mechanised_infantry, engines_3 | |
| 1941 | improved_medium_tank(+chassis 1940), artillery3, support_weapons3, assembly_line_production, radio_detection/centimetric if air war, super_heavy only if resourced | |

### 4.2 Nationalist China (FAIR)

| When | Queue |
|---|---|
| 1936 | infantry_weapons1, construction1, basic_machine_tools (2 slots until the focus chain) |
| 1937 (war with Japan) | tech_engineers, interwar_antitank, tech_support line, support_weapons |
| 1938 | infantry_weapons2, interwar_artillery, tech_logistics_company, construction2 |
| 1939 | improved_infantry_weapons, tech_trucks only if truck production exists, antiair1 |
| 1940-41 | artillery1-2, antitank1-2, infantry_weapons upgrades; ignore tanks and air unless license-produced |

Reason: CHI starts with weak tech (history CHI file only gives infantry_weapons, gw_artillery, interwar_antiair, basic_train; l.205-216 [V]). It is slot-starved, so avoid ahead-of-time techs entirely.

### 4.3 Poland (FAIR)

| When | Queue |
|---|---|
| 1936 | basic_machine_tools, infantry_weapons1, construction1 (focus `POL_the_four_year_plan` gives 1.0 x1 construction_tech) |
| 1937 | interwar_antitank, interwar_artillery, improved_machine_tools, fighter1 |
| 1938 | infantry_weapons2, support_weapons2, antitank1 (focus `POL_anti_blitz_tactics` 1x2 cat_anti_tank), antiair1, tech_engineers |
| 1939 (before September) | improved_infantry_weapons, artillery1, fortification line (basic_fortification_tech, land_fort_tech_1 1938) |

Poland's own focuses give tech bonuses: infantry_weapons 1.0 x1 twice, armor 1.0 x1, motorized 2.0 x1, artillery 1.0 x1, cat_anti_tank 1.0 x1 (poland.txt) [V]. Time those focuses before queuing the matching techs.

### 4.4 Generic nation (any other tag)

1936: basic_machine_tools, infantry_weapons1, construction1. 1937: interwar_artillery/antitank, engineers, concentrated_industry. 1938: infantry_weapons2, support_weapons2, radio. 1939: improved_infantry_weapons, artillery1. 1940: next tiers. Always check the actual `hoi4_research list` first because the 1936 start grants different techs per tag.

## 5. Special projects, scientists, facilities

Mechanism [V: special_projects_documentation.md, common/defines NProject, buildings 00_buildings.txt]:

1. Build a specialization facility in a state with a construction slot: `land_facility`, `air_facility`, `naval_facility`, `nuclear_facility` (base cost 10000 + 6000 per owned facility, one per state, needs supply, can be captured) [V].
2. Recruit scientists: PP cost 25/50/75/100 depending on how many are idle `RECRUIT_SCIENTIST_COST` [V]. Scientist level thresholds 100/100/300/700/1500 XP; speed modifier -100%, -5%, +5%, +10%, +15%, +25% by skill level [V]. Traits add effects only on the project they work on (00_traits.txt: genius `special_project_speed_factor = 0.1`, aircraft/armored/artillery/submarine/nuclear specialists, etc.) [V].
3. Basic research (idle scientist in a matching facility): gains breakthrough progress (12% x ... per defines: `BREAKTHROUGH_DAILY_TECHNOLOGY_GAIN`, 5/10000 per skill level) and a research bonus for that specialization; 0.28 XP per day for the scientist [V].
4. Breakthrough points per specialization (air/land/naval/nuclear) are spent when starting a project (`breakthrough_cost`, e.g. radar 1 air, jet engine 3 air) [V]. Cancelled early projects refund the points [V].
5. Project runs prototype phase (progress to 100, iterations). Time presets: short 45, medium 60, long 100, very_long 200 days; complexity small 15-25, medium 15-20, large 10-15, insane 5-10 iterations ranges [V, script_constants/special_project_constants.txt] and more than one resource must be supplied (steel/aluminium/chromium/tungsten per `resource_cost`); resource shortage slows to min 20%, supply shortage min 20% [V].
6. Rewards: each iteration rolls generic or unique prototype rewards (threshold/weight/options); finishing grants `project_output` (equipment, modules, subunits, design bonuses, state effects) [V].
7. Supportive scientist: a faction partner's scientist works at 25% efficiency, gives the owner 10% of an iteration's progress and +5% research sharing in the faction [V].

Projects relevant to our roles (early availability from files) [V, projects/*.txt]:

| Project | Spec | Tech gate | Breakthrough | Time/complexity | Use |
|---|---|---|---|---|---|
| sp_air_radar | air | electronic_mechanical_engineering | air 1 | medium/medium | best early project; AI weight x3 for majors, x2 after 1940 |
| sp_land_flamethrower_tank | land | tech_engineers | land 1 | short/small | cheap, GER/ITA fortress breaking |
| sp_land_military_engineering_vehicles | land | tech_engineers | land 1 | short/small | engineers breakthrough support |
| sp_land_large_caliber_kinetic_energy_sabot | land | interwar_antitank | land 1 | short/small | AT boost for POL/CHI |
| sp_land_super_heavy_howitzer | land | gw_artillery | land 2 | short/medium | siege; low value |
| sp_land_stronghold_network | land | basic_fortification_tech | land 2 | medium/large | defence, POL/CHI |
| sp_naval_cruiser_submarine / midget | naval | early submarine | naval 1 | medium or short | GER only if U-boats |
| sp_air_jet_engine | air | engines_2 + fighter2 etc. | air 3 | long/large | 1942+ only |
| sp_rockets_*, sp_nuclear_* | air/nuclear | 1942+ techs | | | out of 1936-41 scope |

Facilities exist as a country gate: no matching facility means the project cannot start (UI text FACILITY_WITH_SPECIALIZATION_MISSING) [V]. Scientists and facilities for the three roles: GER builds an air facility first (radar), then land; POL builds one land or air facility only if factory surplus; CHI builds none before 1940 (spend construction on factories and infrastructure).

Rule: if a facility sits idle, put its scientist on basic research (free bonus). If all projects require resources the nation does not have (e.g. tungsten, chromium for China), do not start (speed floor 20%).

## 6. MIOs and equipment variants

### 6.1 MIO selection [V: G/common/military_industrial_organization/organizations]

- Country-specific organisations replace generic ones: GER has Mauser/Walther (infantry weapons), Rheinmetall (artillery), Opel/Daimler (vehicles), Porsche/Henschel/MAN/Alkett (tanks), Messerschmitt/Junkers/Focke-Wulf/Heinkel/Dornier (air) in GER_organization.txt; CHI has Hanyan arsenal (infantry equipment), Taiyuan arsenal (artillery), Liaoning (motorized), Shanghai arsenal and Sino-German technical mission (armored cars/light tank) in CHI_organization.txt; POL has PZInz (infantry tank), PZL (air), PWS, Gdynia shipyard in POL_organization.txt [V]. The generic infantry-equipment MIO is blocked for GER and for POL/CHI when their DLC trees exist [V, 00_generic_organization.txt].
- Pick order of MIO to unlock: infantry equipment, artillery, support equipment, then motorised, then tank, then air. Matches the 1936-41 priority list.
- Size up costs funds: `FUNDS_FOR_SIZE_UP = 700` multiplied by level^1.8 style growth; each size-up gives 1 trait point [V]. Funds come from production lines with the MIO as manufacturer (0.1 per IC/day, capped 100/day) and from research completions with the design team [V].
- Trait choice, generic infantry MIO [V]: take production efficiency cap/gain traits first (`mass_rifle_production`, `bolt_action` for cheap production vs `semi_auto` for quality, exclusive), then reliability, then AP rounds (hard attack) if opponents field armor. In general: efficiency before attack, reliability before build cost.
- Assignments cost PP (0.1/day per design team). Only assign MIOs that match active techs/variants.

### 6.2 Variants and upgrades

- Land equipment uses the module designer (tank/vehicle modules) and the legacy upgrade system for infantry gear and guns. Costs: legacy upgrade base `LAND_EQUIPMENT_BASE_COST = 10` army XP rising per total level; module add 5, replace 6, convert 3, remove 1 XP [V, defines l.668-671, 1121].
- Upgrade (legacy) vs new variant: upgrade infantry equipment/artillery lines in place (existing production keeps going; stock upgrades gradually). Create a new variant only when a new chassis/tech arrives (e.g. basic_medium_tank) or the module set changes; switching a production line to a new variant resets efficiency unless it is a version of the same variant (rule [W]).
- AI keeps 35 XP before trying and reserves 50 XP when short [V, defines l.2465-2471]; we mirror the 35 XP minimum.
- Licensed equipment: -25% MIC speed, upgrades cost 2x XP, only worth it for CHI/POL when a faction partner has much better gear [V defines].
- Do not research a new chassis and keep producing the old one on a long-running line without a plan: finish production to current stock before switching [W].

## 7. Tech that gates capabilities [V from tech files, unless noted]

| Capability | Tech / project | File |
|---|---|---|
| Combat engineers (fortification, river crossing), land projects | tech_engineers (1936, 1.25), tech_engineers2 (1939) | support.txt |
| Trucks and motorised divisions | tech_trucks, motorised_infantry (1936), mechanised_infantry (1940) | infantry.txt |
| Supply throughput: logistics company, trains | tech_logistics_company (1936), basic_train, wartime_train (1939) | support.txt |
| Maintenance and recovery | tech_maintenance_company(+2) | support.txt |
| Signals/command | tech_signal_company | support.txt |
| Radar (air detection, sp_air_radar) | electronic_mechanical_engineering, radio_detection | electronic_mechanical_engineering.txt |
| Decrypt/encrypt | computing_machine 1938, basic_encryption/decryption 1938 | same |
| Fortifications | basic_fortification_tech 1936, land_fort_tech_1 1938, _2 1940 | same |
| Naval invasions | transport (1922), landing_craft (1940) | naval.txt |
| Paratroopers/marines | paratroopers, marines (1936) | infantry.txt |

## 8. Automation split

### (a) Automatable standing rules

1. Keep all research slots busy: when `hoi4_research slots` shows idle, start the next queue entry (plan lists in section 4). Idempotent: skip if already researched (verified completed history) or already in a slot.
2. Prefer on-date techs; skip ahead-of-time unless the entry is flagged in the queue (cost <= 1.5 and capability gating).
3. When a focus gives `add_tech_bonus`, hold the matching tech until the bonus is active (if not already started). Keep the tag-specific table in section 4.
4. Spend army XP: (a) subdoctrine per section 2.2 when XP >= 100 and a track is empty, (b) otherwise keep >= 35 XP for variants.
5. Per-day check of new focus options containing `add_research_slot`: pick it when no survival-critical focus is pending.
6. Daily: if facility exists and idle scientist, assign basic research.
7. After each tech finishes, re-run `hoi4_research list` (fresh) and pick the next from the queue.
8. Fail loudly (journal) if the research list cannot be verified (unknown completed techs, per tool design).

### (b) Needs live decisions

- Choosing between Concentrated vs Dispersed industry (XOR) and between mutually exclusive MIO traits (bolt_action vs semi_auto).
- Subdoctrine choice when the country is mid-war with a changed front (tank-heavy vs infantry-heavy).
- Whether to build a facility (competes with factories/infrastructure) and which special project to start.
- Variant design choices (modules per chassis, role of the tank).
- Whether to license equipment from a partner.
- Switching nation to a different research focus after losing major territory (CHI) or after Poland falls.

### (c) Tools

Existing tools (hoi4_live/tools/registry.py and economy_tools.py) [V]:

| Need | Tool | Status |
|---|---|---|
| Read research slots | `hoi4_research action=slots` | exists |
| List techs, year, cost, ahead flag, folder filter | `hoi4_research action=list` (folder, filter, ahead, limit) | exists |
| Start tech in slot (exact ID, same-slot verification, optional replace) | `hoi4_research action=start` | exists |
| Daily standing plan that starts techs | `hoi4_standing` (default plan uses `hoi4_research start`) | exists, only pre-written entries |
| Event wait / observation | `hoi4_wait_for`, `hoi4_telemetry`, `hoi4_briefing` | exists |
| Focus choice (slot focuses) | `hoi4_focus` | exists |
| Production line to apply new equipment | `hoi4_production` | exists |

Note: the Rust native server (`--mac-input`, 68 tools) DOES expose `hoi4_technology_list`, `hoi4_research_nodes`, `hoi4_research_node_detail`, `hoi4_research`, `hoi4_research_start` (verified via tools/list on 25 Oct 2026); the Python registry uses a single `hoi4_research` with list/start actions. Use whichever the connected server lists.

NO tool exists for (tool-gap list):

1. Doctrine purchase: grand doctrine, subdoctrine selection, mastery inspection, XP balance read (army/navy/air XP).
2. Special projects: facility construction by type (construction tool may cover buildings; verify facility types), scientist recruit/assign, project start, breakthrough points, basic research.
3. MIO: org list, design-team assignment to tech/variant, manufacturer assignment, trait picking, funds read.
4. Variant design: tank/ship/plane module designer, equipment upgrade (legacy upgrades), XP cost check.
5. Faction doctrine/tech sharing initiatives.
6. Reading the tech bonus availability (`add_tech_bonus` remaining uses) and scientist roster.
7. Scripted read of the actual research speed/bonus per slot (only `slots` shows days left).

## 9. Uncertain or version-dependent claims (verify live)

- Exact cost formula and ahead-of-time curve (factor 2 is defined; the shape is from wiki memory).
- Whether the nationalist China tree in this save is china_nationalist.txt or the Waking the Tiger variant (different focus ids).
- DLC gates change which subdoctrines and organisations are visible (La Resistance, NCNS, Gotterdammerung, Thunder at Our Gates tags appear in the files).
- 1936 start vs 1939 bookmark differ materially (doctrines, slots, techs); this file assumes the 1936 start.
- Reward weights of special projects were not read; treat prototype rewards as random.
- Subdoctrine reward effects were sampled, not exhaustively read; read the in-game tooltip before purchase.
- Wiki cross-check for tech mechanics was truncated (hoi4.paradoxwikis.com/Technology returned partial text); no conflicting data was seen.
