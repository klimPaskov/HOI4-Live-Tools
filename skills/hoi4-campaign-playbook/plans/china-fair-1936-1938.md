# Nationalist China FAIR standing plan, 1936-01-01 to 1938-01-01

Companion to `china-fair-1936-1938.plan.json` (JSON has no comments, so every
section is explained here). Copy the JSON to the `plan.json` the runner reads
(see `native/src/plan_run.rs`; the file is re-read every tick). Keys that start
with `_` are ignored by the runner; `_validator` only feeds
`scripts/validate_plan.py`.

```
python3 scripts/validate_plan.py --strict skills/hoi4-campaign-playbook/plans/china-fair-1936-1938.plan.json
checked: research=29, focus=18, production=3, construction=3, politics=4, decisions=0; 0 error(s), 0 warning(s)
```

Goal of the campaign (FAIR, Normal, historical AI, no exploits): unify China (warlords and
the communists), defeat and capitulate Japan, recover lost territory, finish as a strong
major. This plan is only the opening: build the economy and the defence before Japan
attacks (Marco Polo Bridge, usually mid-1937) and keep the routine running through the
first months of the war. The long-range route is in
[china-fair](../references/china-fair.md).

## The DLC set decides the focus tree

The game log (`~/Documents/Paradox Interactive/Hearts of Iron IV/logs/system.log`, 2026-11-01)
lists **10** active DLCs: Anniversary Pack, Arms Against Tyranny, By Blood Alone,
Gotterdammerung, Historical German Portraits, La Resistance, **No Compromise, No Surrender**
(one DLC; its display name contains a comma), No Step Back, Poland: United and Ready, Rocket
Launcher Unit Pack. The validator adds the four DLCs integrated into the base game
(`integrated_dlc/`: Together for Victory, Death or Dishonor, Waking the Tiger, Man the Guns).
These exact names are in `_validator.dlcs`.

Tree weights for CHI (`common/national_focus/`):

| Tree | Weight with this set |
|---|---|
| `china_nationalist_sea_focus` (`china_nationalist_sea.txt`) | 1000 (`tag = CHI`, `has_dlc = "No Compromise, No Surrender"`) |
| `china_nationalist_focus` (`china_nationalist.txt`) | 10 (`tag = CHI`, Waking the Tiger) |
| `generic_focus` | default only |

So CHI plays the **NCNS tree** (274 focuses, all `CHI_*`). `china_nationalist_focus` loads
only when NCNS is inactive; if "No Compromise" and "No Surrender" were listed as two names,
neither would match `has_dlc = "No Compromise, No Surrender"` and the validator would pick
`china_nationalist_focus`, which is wrong for this install. The same NCNS switch decides the
history branches below (OOB `CHI_1936_sea`, NCNS modifiers, characters).

## Start position (verified for this DLC set)

* **17 states** (NCNS gives Sichuan, Shandong, Hebei-Chahar, Beiping etc. to warlords):
  Fujian 595, Zhejiang 596, Jiangsu 598, Jiangxi 600, Hunan 602, Guizhou 603, Anhui 606,
  Henan 607, Shanghai 613, Hubei 620, Qingdao 743, Zunyi 748, Huangshan 749, Changde 750,
  Suzhou 1034, Nanjing 1035, Wuhan 1036. **11 civilian, 8 military, 2 naval factories.**
  Existing warlords: SHX, GXC, YUN, XSM, SIK, SIC, SND, HBC, GDC, XIC; PRC holds 2 states.
* Army (`history/units/CHI_1936_sea.txt`): 85 divisions: 48 "Sanjiao Jun" (6 inf), 17
  "Juntuán Reformed" (6 inf + support artillery), 14 "Juntuán" (4 inf garrison), 6 "Qibing Jun"
  (cavalry). Production: one line `infantry_equipment_0` on 2 factories, so **6 military
  factories are idle on day one**. NSB adds 20 L3/35 and 16 Panzer I light tanks to the stockpile.
* Start techs: infantry_weapons, gw_artillery, interwar_antiair, basic_train, tech_support,
  tech_trucks (NCNS), gwtank_chassis (NSB), the BBA airframes and the MTG naval techs.
  2 research slots. Grand doctrine `mass_assault`.
* Laws: game defaults (civilian economy, volunteer only, export focus). Stability 0.20, war
  support 0.32, neutrality 91 / communism 7 / fascism 2.
* NCNS dynamic modifiers driven by variables that focuses improve: Corruption in the
  Administration (political power +35%, consumer goods +25%, construction -10%, **economy-law
  cost +65%**), Illicit Trade (consumer goods -15%, factory output -15%), Inefficient Economy
  (efficiency gain -35%), Neglected Countryside (research speed -20%, communism drift),
  Corruption in the Armed Forces (conscription -78%, defence -15%, morale -25%), Nine Power
  Treaty (consumer goods, minimum export). Few civilian factories are free for construction
  until the law and the focuses below cut consumer goods.

## Runner behaviour that shapes the plan

* One action per tick in this order: events, research, focus, politics, decisions, training,
  military-factory rebalance, `production` lines, construction, special projects/doctrines/air,
  armies. A refused action waits 1-3 game days (politics back off 3 to 30 days).
* `focus_path`: the first 8 not-completed keys are tried, then 4 of `focus_fallback`. A key
  the game refuses as unavailable is skipped for 6 game hours and the next one is tried, so
  focuses that need a war can sit in the path: they start at the first free slot after the
  war begins.
* A `production` entry counts as done when a line of that equipment already exists (restart
  safety), so the plan cannot add a second `infantry_equipment_0` line; it adds the better
  rifle `infantry_equipment_1` once the tech exists.
* `politics` picks may carry `cost`: the runner waits until political power reaches it.

## Top-level keys

| Key | Value | Why / when to deviate |
|---|---|---|
| `active`, `stop_on_failure` | true / false | A failed action must not freeze the routine. |
| `stop_on_date` | `"1938.01.01"` | Window end; extend together with the focus tail. |
| `auto_events`, `wait_events_days` | true / 3 | Single-option events get option 0; multi-option ones wait 3 days for the model, then use `event_options` or 0. |
| `event_options` | `china.1: 0`, `wtt_xian_incident.1: 0`, `wtt_xian_incident.4: 0` | `china.1` (Marco Polo Bridge, `events/China.txt`): option 0 "Breaking point!" means war; option 1 gives Japan Beiping 608, Shandong 597, Jinan 1038, Hebei 614 and Qingdao 743 for a 180-day truce. FAIR plan fights. Xi'an (`events/WTT_China.txt`): option 0 of `.1` is "agree to go" and of `.4` "agree" to the alliance with the communists, the historical Second United Front against Japan. Chinese warlords and the PRC must be beaten later anyway (Crush the Communists); fighting them now while Japan attacks is the bigger risk. |
| `alarms` | PP > 250, consumer goods > 0.55, idle civ >= 2, idle mil >= 2, unassigned >= 2 | PP 250 = the law at NCNS prices plus margin. Consumer goods start high (corruption, Nine Power Treaty), so 0.55 only flags a real problem. China has only 1-4 free civilian factories, so 2 idle is already worth a look. |

## research_priority (29 technologies)

Two slots and -20% research speed (Neglected Countryside): about 12-16 techs finish in two
years, so positions past ~16 are tail. The third slot from `CHI_expand_academica_sinica` is not
reachable in this window (it needs the Neglected Countryside research variable at -0.05; the
dang guo path's focuses bring it only from -0.20 to -0.12).

| Positions | Block | Reason |
|---|---|---|
| 1-3 | infantry_weapons1, basic_machine_tools, construction1 | Rifle model `infantry_equipment_1` (added as a production line from 1936-07-15), efficiency cap, construction speed. |
| 4-5 | support_weapons, tech_engineers | tech_support is a start tech here; engineers for river and hill defence. |
| 6-10 | concentrated_industry, construction2, interwar_antitank, interwar_artillery, improved_machine_tools | Factory output; anti-tank before Japanese armour; artillery for the Reformed divisions. |
| 11-16 | basic_fortification_tech, tech_recon, electronic_mechanical_engineering, concentrated_industry2, excavation1, infantry_weapons2 | Forts on the river lines, 1937 economy, rifles II. |
| 17-29 | support_weapons2, land_fort_tech_1, radio, artillery1, antitank1, tech_engineers2, improved_equipment_conversion, construction3, excavation2, advanced_machine_tools, concentrated_industry3, mechanical_computing, improved_infantry_weapons | Tail so the list never runs dry. |

Doctrines are not research: CHI holds the grand doctrine `mass_assault`.
`CHI_protracted_warfare` sets the `defensive_postures` sub-doctrine when the infantry track
has none, so do **not** adopt an infantry sub-doctrine by hand before that focus (it would
turn the reward into mere mastery). No `doctrines` key is used for that reason.

## focus_path (18 focuses) and focus_fallback

All keys are in `china_nationalist_sea_focus`; each prerequisite has a member earlier in the
path; no exclusive pair is listed. One focus point = 7 days. Dates assume no delay.

| # | Focus | Cost | Done about | Why |
|---|---|---|---|---|
| 1 | CHI_dang_guo | 5 | 1936-02-05 | Political root: stability +10%, communist and fascist popularity -5%. Exclusive with End Military Primacy (purges the generals, democratic league) and Strengthening Our Resolve (Blue Shirts). Keeps Chiang ruling, which the Generalissimo route needs. |
| 2 | CHI_promote_the_national_currency | 5 | 03-11 | +25 PP; economy-law cost -10%, consumer goods -5%, PP gain -5% (corruption variables). |
| 3 | CHI_the_three_year_plan | 10 | 05-20 | **Needs peace** (bypassed at war). Spirit for 3 years while at peace: consumer goods -5%, construction +10%, efficiency gain +10%. Unlocks the industrial branch. |
| 4 | CHI_denounce_feudalism | 5 | 06-24 | Stability +5%, warlord opinion modifier, registers warlord subjects. Exclusive with Prepare for a Change in Leadership (Wang Jingwei route). |
| 5 | CHI_one_nation_undivided | 10 | 09-02 | **Unification core**: creates the Chinese United Front faction (or the Wang Front variant if PRC already leads one) and invites every independent warlord not at war (event `SEA_china.7`, except SIK in its civil war and HBC). Available while such a warlord exists. Prerequisite of Demonstrate Our Resolve. |
| 6 | CHI_fund_the_plan_domestically | 10 | 11-11 | +2 civilian factories (and slots) instantly in random fully controlled core states. Chosen over Find Foreign Financiers (exclusive), which needs a major with opinion above 19 and pays only from majors above 69. |
| 7 | CHI_decree_the_end_to_corruption | 5 | 12-16 | Consumer goods -5%, construction +10%, construction-boost +10%, economy cost -10%. Leads to Arrest the Seven Gentlemen. |
| 8 | CHI_military_affairs_commission_sea | 5 | 1937-01-20 | Army root: infantry mastery, army XP +5%, morale +10%, planning +2%. |
| 9 | CHI_initiate_the_gung_ho_movement | 10 | war + 70 days | **Needs war.** +2 arms factories in inland states, arms production speed +10%, war support +10%. Starts at the first free slot after Japan attacks. |
| 10 | CHI_demonstrate_our_resolve | 5 | war + 105 days | **Needs war with Japan** (or a defensive war against a major). War support +5%; unlocks `CHI_demonstrate_our_resolve_decision` (state-targeted, take by hand) and the Soviet/Western support focuses. |
| 11 | CHI_protracted_warfare | 10 | 1937-03-31 | `defensive_postures` sub-doctrine (or 50-75 days of infantry mastery), CAS damage reduction. |
| 12 | CHI_monopolise_resource_extraction | 5 | 05-05 | Unlocks the National Resource Commission industrial concern. Prerequisite of the steel works. |
| 13 | CHI_central_steel_and_machine_works | 10 | 07-14 | +3 civilian factories (and slots), +30 steel (states 605, 602, 620 preferred). |
| 14 | CHI_defence_in_depth | 10 | after the war focuses | Defence +5%, max dig-in +1, unlocks the Breach the Yellow River decision (never automated). |
| 15 | CHI_send_the_first_lady_abroad | 10 | war only | US (else UK/France) support: CIC and lend-lease if they accept (event `SEA_china.4`). |
| 16 | CHI_secure_the_soviet_border | 10 | war only | Soviet non-aggression pact, Soviet support modifier and lend-lease if accepted (event `SEA_china.1`). |
| 17-18 | CHI_arrest_the_seven_gentlemen, CHI_draw_support_from_the_big_families | 10, 5 | 1938 | Stability +15%, war support +10%; then H. H. Kung and T. V. Soong become hireable (see politics). Exclusive with Prosecute Cronyism (not listed). |

If Marco Polo comes in July 1937 as historically, the slot runs roughly: currency and
three-year plan in the first half of 1936, faction by September, +2 civs in November, army
root in January 1937, Protracted Warfare, the two industry focuses (done ~1937-07-14), then
Gung Ho (~09-22), Demonstrate Our Resolve (~10-27) and Defence in Depth (~1938-01). If Japan
attacks earlier, Gung Ho and Demonstrate take the next free slot automatically; the three-year
plan is bypassed once at war (it must be finished before the war to count).

The 1939 bookmark (`history/countries/CHI - China.txt`, 1939.1.1 block) completes nearly the
same set for the historical AI: dang guo, currency, denounce feudalism, decree the end to
corruption, arrest the seven gentlemen, Military Affairs Commission, protracted warfare,
defence in depth, Gung Ho, One Nation Undivided, the four defence areas, guerrilla warfare,
relocate factories, field promotions, Demonstrate Our Resolve, first lady, Soviet border,
crackdown on corruption in recruitment.

`focus_fallback` (tried when the path offers nothing startable): relocate_factories_from_the_coast
(evacuation decisions keep 75% of moved factories), the_4_defence_areas, guerrilla_warfare,
introduce_the_baojia_system (3 garrison divisions; exclusive with Expand the Tongzi Jun),
inspect_the_troops (needs a faction), from_many_to_one, formalize_field_promotions (training
time -25%; exclusive with Set up an NCO School), crackdown_on_corruption_in_recruitment,
reinforce_connections_with_shanghai_elites (consumer goods -5%, Du Yuesheng),
extend_compulsory_education (electronics bonus), institute_a_national_registry_of_property.

Never in this window: `CHI_end_military_primacy`, `CHI_strengthening_our_resolve`,
`CHI_prepare_for_a_change_in_leadership` (exclusive with the chosen roots), `CHI_preemptive_strike`
(annex wargoals on every non-allied warlord while Japan is the threat), `CHI_decisive_battles`
branch before the defensive one, navy and ROCAF branches.

## politics

`hoi4_politics_choose` takes `category`, `slot` and `idea`; for `government` slot 0 is
mobilisation, 1 trade, 2 economy, 3-5 political advisors.

| Pick | Cost | Why |
|---|---|---|
| government 3 `CHI_zhang_qun` | default advisor cost | Vice premier: political power +15%. NCNS character, no focus condition; hired first because every later pick is PP. |
| government 2 `partial_economic_mobilisation` | 150 x (1 + economy cost factor): ~248 PP at start, ~233 after the currency focus; `cost: 235` | War support 0.32 > 0.25 qualifies. Consumer goods expected value 0.35 -> 0.25, arms speed +10%. The runner waits for 235 PP; one early refusal is possible if the factor is still higher. |
| government 4 `CHI_t_v_soong` | default | Financial expert, consumer goods -15%. Available only after Draw Support from the Big Families (path 18); until then the runner backs off up to 30 days per try. |
| government 5 `CHI_h_h_kung` | default | Captain of industry: civilian factory, infrastructure and railway construction +10%. Same focus condition. |

Not picked: Chen Guofu (available after Denounce Feudalism: consumer goods -10% but fascism drift
+0.06), conscription laws (the armed-forces corruption cuts conscription 78%, and rifles, not
manpower, are the limit in 1936-37), war economy (needs war support above 0.5). By hand in
`military_staff`: the army chiefs `CHI_huang_qixiang` (defensive 2) and `CHI_teng_chie`
(organisational 2) are both hidden once Dang Guo is done (their `visible` blocks exclude it), so
hire one of them in the first five weeks if wanted; Alexander von Falkenhausen (army chief,
offensive 2, 50 PP) stays available and is the cheap pick.

## decisions

Empty on purpose. The tool `hoi4_decision_take` takes only a decision key, and the useful
NCNS decisions are state- or country-targeted: `CHI_demonstrate_our_resolve_decision`,
`CHI_industrial_evacuations_from_state`, `CHI_press_warlord_to_become_puppet` (125 PP plus faction
influence), `CHI_integrating_states`, war zones. Missions (`CHI_improving_neglected_countryside_mission`,
`CHI_improving_inefficient_economy_mission`) activate by themselves. `CHI_breach_the_yellow_river`
and the concession decisions are never automated. Take the targeted ones by hand.

## training and armies

* No `training` key. CHI starts with 85 divisions that the 8 military factories cannot keep
  equipped; new divisions would dilute rifles. In addition the template names carry an accent
  ("Juntuán"); `hoi4_division_templates` decodes names strictly as UTF-8, which is not yet
  verified for these names. Add training by hand once a `hoi4_division_templates` read shows the
  names and the rifle stockpile is above ~5k: `{"template": "Juntuán Reformed", "queue": 2,
  "batch": 1, "deploy_state": 1036}` (Wuhan, the central hub).
* `armies`: at most 24 divisions per army, absorb on, generals for armies of 8+, by English
  display name from the NCNS roster: Hsueh Yueh (field marshal), Chen Cheng, Wei Lihuang, Hu
  Zongnan, Zhang Fakui, He Yingqin, Sun Li Jen, Tu Yu-ming, Wang Yao-wu, Tai An-lan, Feng Yuxiang,
  Zhu Shaoliang. The pre-NCNS names (Gu Zhutong, Fu Zuoyi, Zhang Xueliang) are not recruited
  with NCNS.

## production

Day one: 6 idle military factories. The rebalance (idle factories into the first
`production_priority` line) runs before `production` lines are added, so
`production_min_idle: 7` keeps it off while the lines are created:

| Line | Factories | Notes |
|---|---|---|
| `artillery_equipment_1` | 3 | Support artillery of the 17 Reformed divisions, later field artillery. |
| `support_equipment_1` | 1 | tech_support is a start tech; engineers/recon need it. |
| `infantry_equipment_1` | 2, `from` 1936-07-15 | After infantry_weapons1 (~150 base days at -20% speed). Refused and retried every 14 days if the tech is late. |

Rifles: the stock line `infantry_equipment_0` (2 factories) keeps running. Two factories are
idle from day one to mid-July unless you act: **manual edit**, add them by hand to the rifle
line on day one (`hoi4_production_set`), or accept the gap. After the `infantry_equipment_1`
line exists set `production_min_idle` to 2 so factories from Gung Ho and construction go
straight to rifles (`production_priority`). Infantry, artillery and support equipment are not
designer types, so no NSB/BBA design is needed. Light tanks (L3/35, Panzer I variants exist) and
aircraft are not produced in this window.

## construction

`construction_min_idle: 1`, because China rarely has more than 1-4 free civilian factories.
Free slots below are from the history files (category slots minus buildings); focus rewards add
some.

| Job | States (id, name, free slots) | Reason |
|---|---|---|
| 1. `industrial_complex` | 603 Guizhou (1), 748 Zunyi (1), 602 Hunan (2), 750 Changde (2), 620 Hubei (3), 1036 Wuhan (4), 600 Jiangxi (3) | Interior states furthest from the Japanese landings and the northern plain. Nothing on the coast or Yangtze delta (613, 1034, 1035, 596, 598, 595, 743) that Japan reaches first. |
| 2. `arms_factory` | 603, 602, 750, 620, 1036, 600 | No fallback to other states: a coastal arms factory is lost within months. |
| 3. `infrastructure` | 603, 748, 602, 750, 620, 1036, 600 | Sink when civ slots are full; supply for the interior. |

Job 1 has 16 slots, more than China fills in two years (a civilian factory is 10,800 IC and
China has 1-4 free factories), so job 2 is reached only by hand: **move job 2 first** once Japan
attacks. Bunkers are province buildings and are not automated; build them by hand on the Yellow
River crossings in Henan 607, around Wuhan and on the Yangtze above Nanjing after
`basic_fortification_tech`.

## special_projects, air

`special_projects` with an empty priority list (any idle facility gets the first visible project;
China likely has none in this window) and `air.war_missions: true` (standing-by fighters fly air
superiority and CAS over the first hostile border state once at war), both as in the Germany plan.

## 1937 war-start checklist (by hand)

Timing: Japan loads `japan_wtt_focus`. `china.1` is sent by Japan's
`JAP_reinforce_the_beijing_garrison` path or by the `JAP_kwantung_army_impatience` mission
timeout (725 days, activated by Japanese focuses). Watch Japan's focus and the news.

1. Move construction job 2 first; free military factories on rifles; bank PP for the
   demonstrate-resolve decision (25 PP) and laws.
2. Front: the north is held by the warlords (HBC, SND, SHX) at the start; hold the main
   line on the Yellow River (Henan 607) with two armies, garrisons of 2-3 divisions in Shanghai
   613, Nanjing 1035, Qingdao 743, Zhejiang 596, Fujian 595, Jiangsu 598, reserve at Wuhan 1036.
   Fight from rivers, hills and forts (army morale -25%, defence -15% until the army focuses).
3. Counter-attack Japanese landings before the beachhead is supplied.
4. After Demonstrate Our Resolve take `CHI_demonstrate_our_resolve_decision` on a threatened
   core state by hand.

## What could not be verified from the files

* Starting political power and the exact advisor cost (Zhang Qun, T. V. Soong and H. H. Kung
  have no `cost`, so the game default applies; check `hoi4_politics_options` on day one).
* Whether `hoi4_politics_choose` expects the character id (used here) or the `idea_token`
  (identical for these three).
* The live consumer-goods share and free civilian factories (depends on the engine's formula
  for the NCNS variables); the alarms will show it.
* Whether the warlord AIs accept the One Nation Undivided invitation (their choice).
* Which states the random focus rewards (+2, +3 civs, +2 arms) pick.
* The Marco Polo date in this DLC set; the plan does not depend on it.
* Whether template names with "á" decode as UTF-8 in `hoi4_division_templates` (reason
  training is not automated).
