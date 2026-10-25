# Germany FAIR showcase (1936 start, HOI4 1.19.3, FAIR, Normal, historical AI)

Objective (Mac handoff): Anschluss and Czechoslovakia, defeat Poland, Low Countries
and France, successful Sea Lion and UK capitulation, form the Greater German Reich,
capitulate the USSR, defeat all Allies, record the final peace. Rules: FAIR (see
[rules-and-modes](rules-and-modes.md)): war no earlier than July 1939, Poland before
the Low Countries and France, no tank-infantry mixed divisions, no console/events.

Untagged text is [H]. Evidence tags: **[V path:key]** verified in the installed game files (Hearts of Iron IV
"Operation Postern" 1.19.3.0, `launcher-settings.json`); **[L mNNN]** observed in a failed
Mac run (`hoi4-macos-native/mNNN/OUTCOME.md`); **[H]** heuristic, check in-game. Paths:
NF = `common/national_focus/germany.txt`, EV = `events/Germany.txt`, DEC = `common/decisions/GER.txt`,
FORM = `common/decisions/formable_nation_decisions.txt`, DEF = `common/defines/00_defines.lua`,
HIST = `history/countries/GER - Germany.txt`. One focus point = 7 days [V DEF:FOCUS_POINT_DAYS],
so cost 5 = 35 days, 10 = 70 days, 4 = 28 days.

## Contents
1. Checkpoint table (use this to detect an off-track run)
2. Why earlier campaigns failed and the standing countermeasure
3. Hard gates in the files (army size, Anschluss ratio, blunders, Yalta)
4. Dated focus plan 1936-1939 (order, why, requirements)
5. Do-not list
6. Laws, decisions, advisors, research, production, army sizing
7. Poland: declaration mechanics and the defended-border rule
8. West: Denmark/Norway, Low Countries, France
9. Britain plan: air, naval, Sea Lion
10. USSR plan
11. Greater Germanic Reich formation and completion predicates
12. Evidence checklist

## 1. Checkpoint table

| Date | Must be true (else: stop, diagnose, fix before continuing) |
|---|---|
| 1936-01-01 | Start [V HIST/history states]: 35 civilian / 28 military / 10 naval factories, 4 research slots, ~30 divisions, laws limited_exports + limited_conscription + partial_economic_mobilisation, MEFO Bills mission running. Only 17 military factories have lines (10 rifles, 2 support, 1 artillery, 2 Panzer II, 2 trucks): assign the other 11 on day one. Focus started on day one. |
| 1936-03-11 | Four Year Plan and Remilitarize the Rhineland done; PP spent (trade-law change, advisors); no idle research slot; every unit assigned to an army (24 or fewer). |
| 1936-12-31 | 10 focuses done (section 4); trade law changed to export_focus or free_trade; at least 35 divisions with set deployment states; 25+ factories on military lines; civilian queue never empty. |
| 1937-12-31 | 5th research slot (Heraeus) or a recorded reason; 50+ civilian, 35+ military factories [H]; 60+ fully equipped divisions [H]; Anschluss tooltip "we estimate we have X" checked monthly. |
| 1938-03 (+/-2 mo) | Anschluss done: states 4, 152, 153, 848, 975, 976 owned [V NF:GER_anschluss]. Army manpower trending to 725k. |
| 1938-09/10 | Sudetenland done (states 69, 972, 74 resolved); army manpower above 725k at start of the focus, above 750k for Fate of Czechoslovakia. |
| 1939-03 (+/-1 mo) | Czechoslovakia resolved (Fate of Czechoslovakia or Integrate); Reassert Eastern Claims done by 1939-05. |
| 1939-06-15 | Army manpower above 950k [V NF:GER_danzig_or_war]; Westwall decision (GER_construct_the_westwall) finished; 30+ equipped divisions already on the western border; fronts drawn for every neighbor; Danzig or War started. |
| 1939-08/09 | Danzig or War complete, Poland refused, war goal vs POL exists [V EV:germany.88]; declare within 30 days [H]; speed 1 during the declaration. |
| 1939-10 | Poland capitulated (target 4-8 weeks); West group untouched or holding; nothing lost on German soil. |
| 1940-05/06 | Low Countries and France attacked after Around the Maginot line; France capitulated by 1940-07 [H]; Greater Reich NOT yet formable. |
| 1940-12 | No German-owned state occupied; air force building toward Channel superiority; Sea Lion prerequisites tracked (1M manpower, 90k infantry equipment in armies [V NF:GER_operation_sea_lion]). |
| 1941 (H1-H2) | Sea Lion bridgehead supplied, or recorded blocker; consumer-goods factor below 90%; USSR plan gate list (section 10) satisfied before Anti-Comintern/War with the USSR. |
| Final | Paris(16), Prague(9), Warsaw(10), Leningrad(195), Moscow(219), Stalingrad(217) controlled and no owned state occupied [V FORM:form_greater_german_reich]; UK, USSR and every remaining enemy capitulated or at peace. |

## 2. Why earlier campaigns failed

| Run | Failure [L] | Standing countermeasure |
|---|---|---|
| m005 | Poland war goal took 11 months to justify; ~250 divisions with 14-20 rifle factories; armies of 43/169 divisions; no army groups. | Use the Danzig or War war goal (no justification); divisions only as fast as rifles (section 6); armies of 24 or fewer. |
| m006 | War declared with ~55 divisions; France took 24 of 41 German states; speed 3 unattended. | Hold 30+ equipped divisions west before declaring; speed 1 around declarations. |
| m007 | War goal usable only in 1941; completed goal expired in ~2 months; no coverage north/coast; supply 22%. | Start the war in 1939; poll the goal daily; fronts for every border and coast. |
| m008 | MEFO uncancelled, consumer goods 46 of 46 factories; Anschluss gated on 50k rifles; fronts only toward France; move orders left stacks in place. | Watch the Consumer Goods row; draw fronts for NL, BEL, DEN, SWI and coast; verify positions, not orders. |
| m009 | Started 1939; army all in France while heartland invaded. | Always start 1936; home-defence fronts. |
| m010 | Danzig or War completed Feb 1940 (950k manpower gate); only 1 of 3 eastern armies advanced; Germany "became" a German Republic. | Reach the manpower gates on schedule; answer pop-ups by hand; the flip is the Yalta partition after Allied occupation, see section 3. |
| qual-15 | PP unspent (414-670), 31 divisions unassigned, Anschluss refused, no naval production. | Spend PP weekly; assign every division; keep the checkpoint table. |

## 3. Hard gates and traps in the files

**Army-manpower gates** (`has_army_manpower`, counts manpower in army units): Befriend/Fate of
Czechoslovakia 750k (Sudetenland 725k), Anti-Soviet Pact 850k, MR Pact 850k for a human player,
Danzig or War 950k, Sea Lion 1,000,000 [V NF: respective focuses]. One infantry battalion is 1000
men and needs 100 infantry equipment [V common/units/infantry.txt], so 950k manpower is roughly
90+ divisions and 85-95k rifles. The trigger text distinguishes the two: Anschluss counts only "properly equipped units", the other gates read manpower [V
localisation/english/wuw_focus_l_english.yml:GER_anschluss_army_trigger_tt]. FAIR: meet every gate with real equipped divisions, not paper
divisions (m005); whether under-equipped units count toward a manpower gate is [H]. Conscription law: limited 2.5%, extensive 5% of population
[V common/ideas/_manpower.txt]; extensive needs fascism (fine).

**Anschluss** [V NF:GER_anschluss, common/on_actions/12_wuw_on_actions.txt]: with Gotterdammerung the
requirement is German strength at least `global.deployed_manpower_ratio_needed_for_anschluss` (starts
24, reduced by Austrian events, reduced by Austrian fascist/neutral popularity) times Austria's, counting only
properly equipped units; without it, 500k manpower and 50k infantry equipment. Blocked while
`rhineland_challenge_met` (183 days after a French challenge). The focus is bypassed if Austria joins a
faction containing another independent country [V]. Read the tooltip every month.

**Hitler blunders -> Oster Conspiracy** [V EV:germany.70-73]: each of Memel refused (germany.23), Austria
refuses Anschluss (germany.29), Munich with Allied backing for Czechoslovakia (germany.47/48/49), a
challenged Rhineland remilitarization (`rhineland_challenge_met`) and the Yugoslavia claims of germany.93 each add 1 to `GER_hitlers_blunders_variable`. At 2 or more the Oster
Conspiracy can fire (mean 150 days): 80% fails, 15% Hitler dies, 5% civil war (33/33/33 if war score is
poor). Keep stability above 0.6 and war score healthy; accept the dice but do not stack blunders.

**The "German Republic under Adenauer"** [V EV:germany.119, WUW_Germany wuw_GER_german_politics.01]:
Yalta Conference fires when a democratic faction leader and the USSR are at war with Germany and
German core states are controlled by non-German enemies; it partitions Germany into a West German
democracy. It is a symptom of foreign occupation (m010), not a random flip. The other way to flip is
the player clicking `GER_oppose_hitler(_ww)` or `GER_re_establish_free_elections_ww`: these are root
focuses, mutually exclusive with Remilitarize the Rhineland, never take them.

**Consumer goods (MEFO Bills)** [V common/scripted_effects/GER_scripted_effects.txt, EV WUW_Germany
germany_mefo_bills.1-4]: the factor rises monthly (3% start, more with non-core states), warnings at 25%,
50%, 100%; at 100% factory and dockyard output -20% and expected goods +10 points. Focuses
Establish the Reichswerke, Seize Foreign Industries, Coal Liquefaction, Hoesch Benzin and Buna Werke
need the factor below 0.9. First war: event germany_mefo_bills.1 offers "continue to rearm" (Economy
of Conquest) or "stop" (Recovering Economy, arms output -10%): choose continue for a war economy.
Decision `GER_cancel_mefos` costs 0 PP but pays back PP and starts an economic-crash window of 60-220
days: use only if the factor nears 100% with war more than 6 months away [H].

**War goals from focuses never expire** (`expire = 0`, 75 uses in NF): Around the Maginot, War with
France, Weserubung, War with the USSR, Danzig/Memel claims. The Danzig war goal comes from event
germany.88 (no `expire`), so declare promptly [H]. A goal created by manual justification expired
after about two months [L m007]; justification time is long at low tension (~160 days at 50%, ~10 days at
91% in m007) [L], so avoid it.

## 4. Dated focus plan (earliest-start dates if nothing gates)

Order and why (all [V] unless marked):

1. **GER_the_four_year_plan** (5, root; 1936-01-01 to 02-05): +2 civilian factories, 100% industry tech
   bonus once. Mutually exclusive with Prioritize Economic Growth. Use the bonus on construction2 or machine tools.
2. **GER_remilitarize_the_rhineland** (5; to 03-11): needs fascism and control of states 42, 51, 978. +75 PP,
   5 army XP, unlocks the rest, +5 threat. French/British reaction (germany.60-62) is suppressed by the
   historical-focus rule unless Germany is 10 times stronger.
3. **GER_accelerate_the_rearmament_program** (5; to 04-15): +2 arms factories, +5% arms speed.
4. **GER_establish_production_targets** (5; to 05-20): +1 arms factory, +5% max/start efficiency.
5. **GER_construct_the_reichsautobahn** (5; to 06-24): infrastructure 5 in states 64, 59, 60, 54.
6. **GER_kdf_wagen_factories** (5; to 07-29): +arms factories in state 59.
7. **GER_develop_modern_maneuver_warfare** (5; to 09-02): command power, Chief of Army, tank company bonus.
8. **GER_fortify_the_vaterland** (5; to 10-07): fortification tech bonus, bunker speed +20% for 730 days, unlocks the
   Westwall/Ostwall/Alpenwall decisions.
9. **GER_reorganize_the_wehrmacht** (5; to 11-11): +50 PP, +5% war support, Ribbentrop. Chosen over **GER_heed_von_neuraths_concerns**
   [H] because only it leads to **GER_molotov_ribbentrop_pact**; Heed leads to War Preparations
   (supply +10% node range, 730 days) and Befriend Czechoslovakia. The two are mutually exclusive; pick Heed
   only if the first war is against the USSR [H].
10. **GER_expanding_the_luftwaffe** (5, root; to 12-16): air bases, 25 air XP.
11. 1937: **GER_concentrated_armament_program** (5), **GER_develop_heraeus_facilities** (10; needs 50+ civilian factories;
    5th research slot, electronics bonus), **GER_aeronautical_research_institute** (5; +arms factory), **GER_form_the_jagdwaffe** (5),
    **GER_adopt_new_panzer_doctrine** (5; exclusive with Prussian Legacy), **GER_panzer_troops_school** (5),
    **GER_autonomy_in_the_kriegsschulen** (5; exclusive with Army Indoctrination), **GER_expand_the_truppenschulen** (5; +15k manpower, support tech bonus).
12. **GER_anschluss** (10): start the day the tooltip ratio is met (target 1938-01-01, done ~03-12). Fill gaps with the cheap
    5-cost focuses above; never idle the slot.
13. **GER_demand_sudetenland** (10; needs 725k): target start ~1938-06-25, done ~09-03, to coincide with the historical Munich window.
    Exclusive with Befriend Czechoslovakia. Event outcomes germany.47/48/49 depend on Allied stance.
14. **GER_first_vienna_award** (5; bypassed if Hungary is not friendly) then **GER_fate_of_czechoslovakia** (10; 750k): start ~1938-12-31, done ~1939-03-12.
15. **GER_reassert_eastern_claims** (5): done by 1939-05; adds claims on Memel, Danzig, Gdynia, Poznan, Katowice. Completing it (or Integrate/Fate) makes the
    UK/France guarantee of Poland possible (events/GOT_Generic_Events.txt: needs German added tension above 14).
16. **GER_molotov_ribbentrop_pact** (5; 850k; needs fascism, Reorganize path): ~1939-05/06. Non-aggression pact, Soviet claims on eastern Poland,
    Baltics, Bessarabia, Karelia, 730-day Soviet idea [V EV:germany.11]. Breakable only after 12 months [V DEF:NAP_UNBREAKABLE_MONTHS].
17. **GER_danzig_or_war** (10; 950k; Poland must own and control state 85; start by 1939-06-15): on completion Poland chooses (AI refuses ~80%,
    always under the historical rule); refusal fires `create_wargoal take_state vs POL (states 85, 807)` [V EV:germany.86-88]. Peaceful alternative
    **GER_danzig_for_slovakia** is exclusive: do not take it.

Then: **GER_around_maginot** (4; creates war goals vs Belgium state 6 and Netherlands state 7; exclusive with Influence the Benelux),
**GER_war_with_france** (10; goal on state 28), optional **GER_operation_weserubung** (4; goals vs Denmark state 37 and Norway state 110;
needs Warsaw controlled and war with the UK), **GER_operation_sea_lion** (5), **GER_anti_comintern_pact** (5), **GER_war_with_the_ussr** (10; goal
`annex_everything`, exclusive with Alliance with the USSR).

Later economy once a war economy: Zentrale Planung -> Rustungsstab / Armament Rationalization -> Totaler Krieg (need war with a major,
Totaler Krieg war support 0.89 or surrender progress). Mass Production / Build Defense Industry need threat above 0.15 / 0.3 or war.

## 5. Do-not list

- No `GER_oppose_hitler*`, `GER_re_establish_free_elections*`, `GER_monarchy*`, `GER_start_the_proletarian_revolution` (government change).
- No `GER_danzig_for_slovakia`, `GER_influence_the_benelux`, `GER_befriend_*` that remove war goals the plan needs.
- Do not declare before Danzig or War is complete, before July 1939 (FAIR), or with fewer than 30 equipped divisions west.
- Do not leave the MEFO factor unwatched; do not take Reichswerke-chain focuses at 90%+ factor.
- No mixed leg-infantry/tank divisions, no console, no event firing, no reload [FAIR].
- No unattended game speed above 1 near declarations, capitulations, or Yalta-class events.
- Do not train divisions without a deployment state or beyond what rifle stock supports.

## 6. Laws, decisions, advisors, research, production, army

**Laws** (150 PP each) [V common/ideas/_economic.txt, _manpower.txt]: trade law `limited_exports` ->
`export_focus` (+10% factory output, +10% construction speed, +5% research) or `free_trade` (+15/+15/+10%) while
`GER_autarky_efforts` is NOT completed [V: unavailable after it]; economy `partial_economic_mobilisation` ->
`war_economy` (+20% arms speed, needs war support above 0.5 and fascism) when the war is within ~6 months; conscription
`limited` -> `extensive` (needs fascism) when the pool, not rifles, is the limit. Late war: `totaler_krieg_economy`, `tot_economic_mobilisation`.

**Decisions**: `GER_cancel_mefos` (see section 3); `GER_construct_the_westwall` (25 CP + 15 PP, 90 days, 2 civilian factories; needs states 51, 42, 978
non-demilitarized and controlled) then `GER_construct_the_ostwall`, `GER_construct_the_alpenwall`, Atlantikwall variants [V DEC]; foreign trade for rubber/oil/
chromium/tungsten; `form_greater_german_reich` at the end. Spend PP weekly: laws, advisors, decisions.

**Research** (4 slots, 5th via Heraeus) [H tech keys from common/technologies]: 1936 construction1, basic_machine_tools, infantry_weapons2, tech_engineers,
tech_recon, radio, artillery1; 1937 construction2, improved_machine_tools, excavation1, support_weapons, antiair1; 1938 advanced_machine_tools (also a
focus requirement), excavation3 (Vereinigte Stahlwerke), basic_medium_tank_chassis, fighter and CAS upgrades, oil_processing, rubber_processing, improved infantry weapons;
1939+ doctrines. Never leave a slot idle.

**Production** [H, formula V]: rifle (infantry_equipment) costs 0.5 IC [V common/units/equipment/infantry.txt]; factory speed 3.5 IC/day [V DEF:BASE_FACTORY_SPEED_MIL];
so a factory at ~85% makes about 6 rifles/day. Targets: 1936 12-15 factories on rifles, 3 support, 4-6 artillery, 3 trucks, 2 tanks; 1937-38 add fighters and CAS
(6-10), medium tanks (6-8) by 1939; keep civilian construction going until the 1938 build wave is queued. Factory benchmark: Germany in the 1939.1.1 scenario has 69 civilian,
62 military, 13 naval (with Austria and the Sudetenland) [V history/states]; a good run is at or above that in January 1939. Rival benchmarks at that date: FRA 45/9, UK 40/18, USSR 65/47, Italy 29/29.

**Army sizing** [H, gates V]: ~30 divisions in 1936 -> 45 (end 1936) -> 60 (end 1937) -> 80 (mid 1938) -> 95+ (June 1939). Template: stock "Infanterie-Division" is 9 infantry
battalions plus engineer and artillery support [V history/units]; prefer 7 infantry + 2 artillery with engineers/recon/AA support (generic-playbook section 2). Separate tank and
motorized/mechanized assault divisions (FAIR). Army groups under field marshals, armies of 24 or fewer, a deployment state on every training line.

## 7. Poland and the defended-border rule

Prerequisites at declaration: Danzig or War done and the war goal visible; July 1939 or later; 950k manpower; Westwall finished; **fronts for every border**: France
(Saar, Alsace-Lorraine, Upper Rhine), Belgium, Netherlands, Luxembourg, Denmark, Switzerland, Austria/Italy, Lithuania/East Prussia, and every coastal region (North Sea, Baltic ports Kiel,
Hamburg, Bremerhaven, Stettin, Konigsberg) [L m008, m009]. Minimum [H]: West 30+ divisions in two armies behind bunkers plus a mobile reserve of 4 divisions; 2 Denmark, 3 Switzerland/Alps,
1 per major port, fighters and AA over the Ruhr. East: 3 armies (East Prussia/Pomerania, Silesia, Slovakia axis) with 6-8 tank/motorized divisions as the exploitation arm; air superiority over
Warsaw/Lodz first. Planning needs ~15 days at 2% per day to the 30% cap [generic-playbook]. After capitulation reassign east armies at once, check for UK/France attacks, and verify unit positions
rather than orders (stacks did not move in m008).

The Allies join through the guarantee chain; expect France to attack within days. Do not run speed above 1 until the first week is stable.

## 8. West

Order [V]: Around the Maginot (war goals vs Belgium/Netherlands), optional Weserubung (needs ENG war; Norwegian ore), War with France. Plan [H]: pin the Maginot sector; main effort through the
Netherlands/Belgium and the Ardennes toward Sedan and the Channel; air bases forward before the offensive; infantry follows tanks, pockets closed on the Somme and Channel. Lorraine frontal assaults
grind [L generic-playbook]. After each capitulation redraw fronts and verify supply (target above 70% before the next bound). France capitulation and Paris control unlock `form_greater_german_reich`
visibility but not availability.

## 9. Britain plan

Prerequisites [V]: war with the UK; 1,000,000 army manpower; more than 90k infantry equipment in armies; the Operation Sea Lion focus (5, after War with France or Invade Italy) adds a 180-day idea:
+25% naval invasion prep speed, +25% shore bombardment, -15% invasion penalty, +5% attack vs ENG, marine/para contribution -25% [V common/ideas/GER.txt]. Defaults [V DEF]: naval invasion preparation
60 days, one naval invasion plan at a time, 4 divisions per invasion before techs/ships raise it; the true cap and air/naval gates are in the launch tooltip (treat as unknown until read).

Plan [H]: (1) 1940-H2: take Channel ports, build 20+ airfields in Pas-de-Calais/Belgium; win air superiority over the south-east (fighters, then tactical bombers; radar and AA for own bases);
(2) build 100+ escorts (destroyers), subs and naval bombers rather than capital ships; transports (convoys) 400+; (3) rehearse with one small landing to prove ports and supply;
(4) launch Kent/Sussex only when air supremacy and sea-region control hold on every route region; follow with a second wave, capture a port, repair it, bring fighters over; (5) London and Midlands
for surrender progress. If the landing fails, accept the loss and rebuild; no reload. UK capitulation must appear in fresh evidence; it does not end the war with the Allies.

## 10. USSR plan

Gate list before Anti-Comintern/War with the USSR [H]: UK capitulated or contained; Poland/France garrisons stable; 25+ motorized/mechanized or tank divisions; fuel and truck stock for a
2-month offensive; 3000+ aircraft-equivalent air force [H]; supply hubs and rail in Poland/Romania ready; non-aggression pact either expired or deliberately broken after 12 months [V DEF]. Focus
chain [V]: Anti-Comintern Pact (5) -> War with the USSR (10; `annex_everything` goal, no expiry; mutually exclusive with Alliance). Execution: three army groups on Leningrad (195), Moscow (219),
Stalingrad/south (217); infantry holds the widening front, tanks pocket; avoid winter/mud pushes. Soviet capitulation list: record each.

## 11. Greater Germanic Reich and completion

This install has it as decision `form_greater_german_reich` ("Proclaim the New Order", cost 25 PP; category title "The Greater Germanic Reich of the German Nation") [V FORM:9800-9877,
`wtt_decisions_l_english.yml`]. Allowed: original GER with Waking the Tiger (integrated). Visible: fascist GER, flag not set, and (at war with a capitulated France, or Paris state 16 owned/controlled,
or Moscow 219 controlled). Available: not a subject; **Paris 16, Prague 9, Warsaw 10, Leningrad 195, Moscow 219, Stalingrad 217 each fully controlled by Germany or a subject; and every owned state fully
controlled by Germany** (an occupied German state blocks it). Effect: +5% war support, +5% stability, cosmetic tag "Greater Germanic Reich", Germania (province 6521) +25 VP. Do not infer completion from
the map color; read the decision. A communist `form_greater_proletarian_state` exists and is not ours.

Success predicates: all of section 1 final row plus: UK, USSR, France, Poland, Czech lands settled; remaining Allied majors listed and at peace or capitulated; peace ownership verified separately
from battle control.

## 12. Evidence checklist

Record each diplomatic acquisition (dates), Danzig or War outcome and war goal, Polish/French/UK/Soviet capitulation, British landing and supplied bridgehead, the formable decision result, the
remaining-war list, final peace ownership, MEFO/consumer-goods readings at each year end, and every failure. Success = all predicates true, not a large army or green front.
