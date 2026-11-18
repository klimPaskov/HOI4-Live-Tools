# Germany FAIR showcase (1936 start, Normal, historical AI)

Objective: Anschluss and Czechoslovakia, defeat Poland, Low Countries
and France, successful Sea Lion and UK capitulation, form the Greater German Reich,
capitulate the USSR, defeat all Allies, record the final peace. Rules: FAIR (see
[rules-and-modes](rules-and-modes.md)): war no earlier than July 1939, Poland before
the Low Countries and France, no tank-infantry mixed divisions, no console/events.

One focus point = 7 days, so cost 5 = 35 days, 10 = 70 days, 4 = 28 days.

**DLC set**: active Anniversary Pack, Arms Against Tyranny, By Blood Alone, Gotterdammerung, Historical German
Portraits, La Resistance, No Compromise No Surrender, No Step Back, Poland: United and Ready, Rocket Launcher Unit
Pack; Together for Victory, Death or Dishonor, Waking the Tiger and Man the Guns are integrated. Consequences: start
OOB `GER_1936_nsb` + `GER_1936_air_bba` + `GER_1936_naval_mtg`; tanks and planes are designer equipment (chassis
`light_tank_chassis_2` Panzer II, airframes `small_plane_airframe_1` Bf 109 D, `small_plane_cas_airframe_1` Ju 87,
`medium_plane_airframe_1` He 111/Do 17); legacy keys such as `fighter_equipment_1`, `CAS_equipment_1`,
`basic_medium_tank`, `fighter2` do not exist. A Panzer III (`medium_tank_chassis_1`) or any second-generation plane
needs a design created in game first (they exist ready-made only in the 1939 bookmark). Section 6 and
`plans/germany-fair-1936-1938.md` use these keys.

## Contents
1. Checkpoint table (use this to detect an off-track run)
2. Failure modes and standing countermeasures
3. Hard gates (army size, Anschluss ratio, blunders, Yalta)
4. Dated focus plan 1936-1939 (order, why, requirements)
5. Do-not list
6. Laws, decisions, politics, advisors, research, production, army sizing
7. Poland: declaration mechanics and the defended-border rule
8. West: Denmark/Norway, Low Countries, France
9. Britain plan: air, naval, Sea Lion
10. USSR plan
11. Greater Germanic Reich formation and completion predicates
12. Run record checklist

## 1. Checkpoint table

| Date | Must be true (else: stop, diagnose, fix before continuing) |
|---|---|
| 1936-01-01 | Start: 35 civilian / 28 military / 10 naval factories, 4 research slots, ~30 divisions, laws limited_exports + limited_conscription + partial_economic_mobilisation, MEFO Bills mission running. 20 military factories have lines (10 rifles, 2 support, 1 artillery, 2 Panzer II, 2 trucks, 1 each Bf 109 D, Ju 87, He 111). Before the first unpause (day-0 grace: factories added to an existing line come in at its efficiency): rifles 18, artillery 4, support 3, trucks 3; the plane and Panzer II lines closed; naval queue: finish the four Type II subs, the two Z1934 destroyers and Deutschland, remove Hipper and both Scharnhorst unless the plan wants capitals. Rhineland started; Basic Medium Tank researching in its own slot; agency founded; air and navy exercising; Political Loyalty in the academy. |
| 1936-03-11 | Rhineland and Führerprinzip done; Free Trade taken; no idle research slot; every unit assigned to an army (24 or fewer). |
| 1936-12-31 | Reorganize the Wehrmacht, both Organization Todt focuses and Hess done; the Four Year Plan timed to finish with machine tools II; at least 35 divisions with set deployment states; civilian queue never empty; a French intelligence network growing. |
| 1937-12-31 | 5th research slot (Heraeus) or a recorded reason; Basic Medium researched and the medium design on a line; military construction since about June 1937; 50+ civilian, 40+ military factories; 75+ fully equipped divisions; the first French collaboration done; Anschluss tooltip "we estimate we have X" checked monthly. |
| 1938-03 (+/-2 mo) | Anschluss done: states 4, 152, 153, 848, 975, 976 owned. Army manpower trending to 725k. |
| 1938-09/10 | Sudetenland done (states 69, 972, 74 resolved); army manpower above 725k at start of the focus, above 750k for Fate of Czechoslovakia. |
| 1939-03 (+/-1 mo) | Czechoslovakia resolved (Fate of Czechoslovakia or Integrate); Reassert Eastern Claims done by 1939-05. |
| 1939-06-15 | Army manpower above 950k; Westwall decision (GER_construct_the_westwall) finished; 30+ equipped divisions already on the western border; fronts drawn for every neighbor; Danzig or War started. |
| 1939-08/09 | Danzig or War complete, Poland refused, war goal vs POL exists (event germany.88); declare within 30 days; the fronts, the Westwall garrison and the reserve stand before the declaration, so the game keeps speed 3 through it. |
| 1939-10 | Poland capitulated (target 4-8 weeks); West group untouched or holding; nothing lost on German soil. |
| 1940-05/06 | Low Countries and France attacked after Around the Maginot line; France capitulated by 1940-07; Greater Reich NOT yet formable. |
| 1940-12 | No German-owned state occupied; air force building toward Channel superiority; Sea Lion prerequisites tracked (1M manpower, 90k infantry equipment in armies). |
| 1941 (H1-H2) | Sea Lion bridgehead supplied, or recorded blocker; consumer-goods factor below 90%; USSR plan gate list (section 10) satisfied before Anti-Comintern/War with the USSR. |
| Final | Paris(16), Prague(9), Warsaw(10), Leningrad(195), Moscow(219), Stalingrad(217) controlled and no owned state occupied (`form_greater_german_reich`); UK, USSR and every remaining enemy capitulated or at peace. |

## 2. Failure modes and standing countermeasures

| Failure | Standing countermeasure |
|---|---|
| Poland war goal took 11 months to justify; ~250 divisions with 14-20 rifle factories; armies of 43/169 divisions; no army groups. | Use the Danzig or War war goal (no justification); divisions only as fast as rifles (section 6); armies of 24 or fewer. |
| War declared with ~55 divisions; France took 24 of 41 German states; speed 3 unattended. | Hold 30+ equipped divisions west and a front on every border before declaring; readiness comes from those gates, not from slowing the game. |
| War goal usable only in 1941; completed goal expired after 60 days; no coverage north/coast; supply 22%. | Start the war in 1939; poll the goal daily; fronts for every border and coast. |
| MEFO uncancelled, consumer goods 46 of 46 factories; Anschluss gated on 50k rifles; fronts only toward France; move orders left stacks in place. | Watch the Consumer Goods row; draw fronts for NL, BEL, DEN, SWI and coast; check positions, not orders. |
| Started 1939; army all in France while the heartland was invaded. | Always start 1936; home-defence fronts. |
| Danzig or War completed Feb 1940 (950k manpower gate); only 1 of 3 eastern armies advanced; Germany "became" a German Republic. | Reach the manpower gates on schedule; answer pop-ups by hand; the flip is the Yalta partition after Allied occupation, see section 3. |
| PP unspent (414-670), 31 divisions unassigned, Anschluss refused, no naval production. | Spend PP weekly; assign every division; keep the checkpoint table. |

## 3. Hard gates and traps

**Anschluss army gate** (Gotterdammerung): `GER_anschluss` is available only while the fighting army is more than
`global.deployed_manpower_ratio_needed_for_anschluss` times Austria's (24, set on game start in
`common/on_actions/12_wuw_on_actions.txt`; Austrian focuses can raise or lower it). 52 divisions were refused in
1938; the AI had it done by October 1938 with about 96. Size the training queue and caps for that count before the planned date.

**Army-manpower gates** (`has_army_manpower`, counts manpower in army units): Befriend/Fate of
Czechoslovakia 750k (Sudetenland 725k), Anti-Soviet Pact 850k, MR Pact 850k for a human player,
Danzig or War 950k, Sea Lion 1,000,000. One infantry battalion is 1000 men and needs 100 infantry
equipment, so 950k manpower is roughly 90+ divisions and 85-95k rifles. Anschluss counts only properly
equipped units; the other gates read manpower. FAIR: meet every gate with real equipped divisions, not
paper divisions; read the focus tooltip for the current count. Conscription law: limited 2.5%, extensive
5% of population; extensive needs war support above 20% and fascism (Germany qualifies).

**Anschluss**: with Gotterdammerung German strength must be at least 24 times Austria's (the ratio falls with
Austrian events and with Austrian fascist/neutral popularity), counting only properly equipped units; without it,
500k manpower and 50k infantry equipment. Blocked while `rhineland_challenge_met` (183 days after a French
challenge). The focus is bypassed if Austria joins a faction containing another independent country. Read the
tooltip every month.

**Hitler blunders -> Oster Conspiracy** (events germany.70-73): each of Memel refused (germany.23), Austria
refuses Anschluss (germany.29), Munich with Allied backing for Czechoslovakia (germany.47/48/49), a
challenged Rhineland remilitarization (`rhineland_challenge_met`) and the Yugoslavia claims of germany.93 each
add 1 to `GER_hitlers_blunders_variable`. At 2 or more the Oster Conspiracy can fire (mean 150 days): 80% fails,
15% Hitler dies, 5% civil war (33/33/33 if war score is poor). Keep stability above 0.6 and war score healthy;
accept the dice but do not stack blunders.

**The "German Republic under Adenauer"** (event germany.119): the Yalta Conference fires when a democratic faction
leader and the USSR are at war with Germany and German core states are controlled by non-German enemies; it
partitions Germany into a West German democracy. It is a symptom of foreign occupation, not a random flip. The other
way to flip is the player clicking `GER_oppose_hitler(_ww)` or `GER_re_establish_free_elections_ww`: these are root
focuses, mutually exclusive with Remilitarize the Rhineland, never take them.

**Consumer goods (MEFO Bills)**: the factor rises monthly (3% start, more with non-core states), warnings at 25%,
50%, 100%; at 100% factory and dockyard output -20% and expected goods +10 points. Focuses
Establish the Reichswerke, Seize Foreign Industries, Coal Liquefaction, Hoesch Benzin and Buna Werke
need the factor below 0.9. First war: event germany_mefo_bills.1 offers "continue to rearm" (Economy
of Conquest) or "stop" (Recovering Economy, arms output -10%): choose continue for a war economy.
Decision `GER_cancel_mefos` costs 0 PP but pays back PP and starts an economic-crash window of 60-220
days: use only if the factor nears 100% with war more than 6 months away.

**War goals from focuses never expire**: Around the Maginot, War with France, Weserubung, War with the USSR,
Danzig/Memel claims. The Danzig war goal comes from event germany.88 and does not expire; declare promptly anyway.
A justified (take-state) war goal expires 60 days after completion, and justification is slow at low world tension
(~160 days at 50%, ~10 days at 91%), so avoid it.

**High Seas Fleet**: completing `GER_high_seas_fleet` makes `GER_construct_aircraft_carriers` and
`GER_grosskampfschiff_construction` unavailable; take either first if the plan needs it.

## 4. Dated focus plan (earliest-start dates if nothing gates)

The multiplayer opening order (Rhineland, Führerprinzip, Todt, Reorganize the Wehrmacht, Anschluss about 30 days
later) is the base; FAIR keeps its order and moves only the diplomatic focuses to their historical season (Austria
in 1938, the Sudetenland in autumn 1938, Czechia in March 1939, Danzig from July 1939). The economy and army focuses
the multiplayer order takes after Czechia fill 1936-37 instead. One focus point = 7 days.

1. **GER_remilitarize_the_rhineland** (5; 1936-01-01 to 02-05): needs fascism and control of states 42, 51, 978. +75 PP,
   5 army XP, unlocks the rest, +5 threat. Buy Free Trade and the Political Loyalty academy spirit while it runs.
   The French/British reaction (germany.60-62) is suppressed by the historical-focus rule unless Germany is 10 times
   stronger.
2. **GER_fuhrerprinzip** (5; to 03-11): +50 PP, opens the inner-circle focuses.
3. **GER_ascension_of_todt** (2; to 03-25): Reich Engineer, +10% civilian, infrastructure and rail construction speed,
   -0.1 PP a day; opens the Organization Todt focuses.
4. **GER_reorganize_the_wehrmacht** (5; to 04-29): +50 PP, +5% war support, Ribbentrop. Chosen over
   **GER_heed_von_neuraths_concerns** because only it leads to **GER_molotov_ribbentrop_pact**. Schacht while it runs.
5. **GER_form_organization_todt** (inner-circle duration, about 70 days): +1 building slot, an arms factory, the
   Organization Todt concern. Then **GER_autonomous_organization_todt** (about 70 days): Todt becomes the armaments
   minister (+10% civilian, military, infrastructure, rail and dockyard construction, -0.3 PP a day) and the concern
   improves. Both are done before the military-factory wave; with Todt as armaments minister there is no reason to
   replace him with Goebbels at the end of the civilian phase (the multiplayer order does that because it skips
   Autonomous Organization Todt).
6. **GER_party_chancellor_hess** (2): opens **GER_optimize_reich_labour_service** and with it the Reich Labour decisions
   (Develop Western and Central Germany, 50 PP each, +1 slot, June 1937).
7. 1936 fillers while the Four Year Plan waits for its bonus window (item 8): **GER_develop_modern_maneuver_warfare**,
   **GER_adopt_new_panzer_doctrine** (exclusive with Prussian Legacy; +25 command power), **GER_expanding_the_luftwaffe**
   (root), **GER_fortify_the_vaterland** (Westwall decisions), **GER_legion_condor** when the Spanish Civil War starts.
8. **GER_the_four_year_plan** (5): timed to complete the week machine tools II finishes, so its 100% industry bonus
   goes to Advanced Machine Tools (a 1939 tech started early 1937: about 410 days with the bonus against a finish in
   early 1939 without it). Every industry tech (construction, concentrated, excavation, tools) started after the
   focus spends the bonus, and the executor cannot decline one: in the window the other slots research electronics,
   armour, anti-tank, radio and logistics, and the freeing slot may wait up to 10 days for the focus.
9. **GER_construct_the_reichsautobahn**, **GER_kdf_wagen_factories**, **GER_develop_heraeus_facilities** (10; needs 50+
   civilian factories; 5th research slot, electronics bonus; Seldte), **GER_accelerate_the_rearmament_program**,
   **GER_concentrated_armament_program**, **GER_establish_production_targets**.
10. **GER_autarky_efforts** (5): timed like item 8 for Assembly Line Production (1941). It sets the Autarky trade law
    (Free Trade and Export Focus are then blocked); switch to Limited Exports at once (+5% factory output beats
    Autarky's research). Then **GER_institute_price_controls** and every price-control decision except Gömbös.
11. **GER_advanced_panzer_research** (10; needs Radio): its 100% medium-armour bonus on the improved medium chassis in
    mid-1938.
12. **GER_anschluss** (10): start the day the tooltip ratio is met, not before spring 1938 (target done ~03-12). Fill gaps
    with the cheap 5-cost focuses above; never idle the slot.
13. **GER_demand_sudetenland** (10; needs 725k): target start ~1938-06-25, done ~09-03, to coincide with the historical
    Munich window. Exclusive with Befriend Czechoslovakia. Event outcomes germany.47/48/49 depend on Allied stance.
14. **GER_first_vienna_award** (5; bypassed if Hungary is not friendly) then **GER_fate_of_czechoslovakia** (10; 750k):
    start ~1938-12-31, done ~1939-03-12.
15. **GER_reassert_eastern_claims** (5): done by 1939-05; adds claims on Memel, Danzig, Gdynia, Poznan, Katowice.
    Completing it (or Integrate/Fate) makes the UK/France guarantee of Poland possible (needs German added tension
    above 14).
16. **GER_molotov_ribbentrop_pact** (5; 850k; needs fascism, Reorganize path): ~1939-05/06. Non-aggression pact, Soviet
    claims on eastern Poland, Baltics, Bessarabia, Karelia, 730-day Soviet idea (event germany.11). Breakable only
    after 12 months. **GER_coal_liquefaction** (Develop Ersatz Material decision) around it.
17. **GER_danzig_or_war** (10; 950k; Poland must own and control state 85; start by 1939-06-15): on completion Poland
    chooses (AI refuses ~80%, always under the historical rule); refusal creates a `take_state` war goal vs POL on
    states 85 and 807 (events germany.86-88). Peaceful alternative **GER_danzig_for_slovakia** is exclusive: do not
    take it.

Then: **GER_around_maginot** (4; creates war goals vs Belgium state 6 and Netherlands state 7; exclusive with Influence
the Benelux), **GER_war_with_france** (10; goal on state 28), optional **GER_operation_weserubung** (4; goals vs Denmark
state 37 and Norway state 110; needs Warsaw and Danzig 85 controlled and war with the UK; its reward includes a 100%
transport research bonus, best spent on landing craft), **GER_operation_sea_lion** (5), **GER_anti_comintern_pact**
(5), **GER_war_with_the_ussr** (10; goal `annex_everything`, exclusive with Alliance with the USSR). A multiplayer
shortcut justifies on France so Belgium joins and skips Around the Maginot; against the AI France is already at war
through the Polish guarantee, so a French war goal brings nobody in: take Around the Maginot. Denmark and Norway can be
justified instead of Weserübung while at war with the UK (fascists justify 80% faster in a major war).

Later economy once a war economy: Zentrale Planung -> Rustungsstab / Armament Rationalization -> Totaler Krieg (need war
with a major, Totaler Krieg war support 0.89 or surrender progress). Mass Production / Build Defense Industry need
threat above 0.15 / 0.3 or war. Loyalty to the Führer for War Bonds (renew on expiry); the gold decisions before
Rüstungsstab bring consumer goods near 10%.

**GER_prioritize_economic_growth** is the long-war alternative to the Four-Year Plan: +2 civilian factories, Civilian
Economy, MEFO ended with Schacht removed, Erhard unlocked (-33% economy and trade law cost); it gives up the 100%
industry bonus and the rearmament focuses.

## 5. Do-not list

- No `GER_oppose_hitler*`, `GER_re_establish_free_elections*`, `GER_monarchy*`, `GER_start_the_proletarian_revolution` (government change).
- No `GER_danzig_for_slovakia`, `GER_influence_the_benelux`, `GER_befriend_*` that remove war goals the plan needs.
- Do not declare before Danzig or War is complete, before July 1939 (FAIR), or with fewer than 30 equipped divisions west.
- Do not leave the MEFO factor unwatched; do not take Reichswerke-chain focuses at 90%+ factor.
- No mixed leg-infantry/tank divisions, no console, no event firing, no reload (FAIR).
- Speed 3 all campaign, no pause after the start (the owner's run rule): before declarations, capitulations and Yalta-class events the gates (fronts on every border, reserve, events answered by rule) are met in advance; the game is never slowed to make up for a missing gate.
- Do not train divisions without a deployment state or beyond what rifle stock supports.

## 6. Laws, decisions, politics, advisors, research, production, army

The decisions below follow the multiplayer FAIR build that this card starts from; where playing the AI or the 1.19.4
files change the best choice, the line says so and why.

**Laws** (150 PP each). Trade: **Free Trade during the Rhineland focus** (+15% factory and dockyard output, +15%
construction, +10% research; it exports 80% of surplus resources), held until Autarky Efforts replaces it with the
Autarky law, then **Limited Exports** (+5% factory output and construction). The trade-off against taking Autarky and
price controls in 1936: Free Trade's extra 10 points of construction over Autarky on 40-60 civilian factories are worth
about 4-6 factories for roughly 22 months, against a faster MEFO consumer-goods rise until Institute Price Controls;
read the consumer-goods factories each quarter and, if they grow by more than Free Trade adds, bring the Four Year
Plan, Autarky and Price Controls forward. Economy: `partial_economic_mobilisation` -> `war_economy` (+20% military
factory construction, consumer goods 20%) the day war support passes 50% (fascism is also required; Germany starts at
30% and passes 50% after Munich, about November 1938). Conscription `limited` -> `extensive` when the pool, not rifles,
is the limit. At war with a major and war support above 80%: Total Mobilisation, later Totaler Krieg.

**Political power order** (compounding first, each when affordable): Free Trade -> Schacht (75) -> an army chief who
trains (Keitel, drill) -> Krupp -> Speer as Captain of Industry (the Four Year Plan event: pick Speer) -> Seldte (with
Heraeus) -> Limited Exports (after Autarky) -> War Economy (war-support gate) -> Funk as soon as he is available ->
War Bonds (Loyalty to the Führer, renewed on expiry) -> Total Mobilisation. Then the air chief, a land theorist and
army high command (Rommel, Schörner, Fritsch; never a naval officer in an army seat), the design-company seats. The
DNSAP event: take the 50 PP option. Reich Labour decisions Develop Western and Central Germany in June 1937; Develop
Ersatz Material after Coal Liquefaction; seize gold after each capitulation (Poland about -30% consumer goods; France
50 PP), after building military factories in Poland and western France (the decisions cut consumer goods more the
more is built there). Never call allies into the Poland war.

**Decisions**: `GER_cancel_mefos` (see section 3); `GER_construct_the_westwall` (25 CP + 15 PP, 90 days, 2 civilian
factories; needs states 51, 42, 978 non-demilitarized and controlled) then `GER_construct_the_ostwall`,
`GER_construct_the_alpenwall`, Atlantikwall variants; foreign trade for rubber/oil/chromium/tungsten;
`form_greater_german_reich` at the end. Spend PP weekly: laws, advisors, decisions.

**Research** (4 slots at start, the 5th from Heraeus). Slot 1: machine tools I, mechanical computing, computing
machine, armour 2, radio 2, construction 3, excavation 3, concentrated 4, engine 3, logistics 3. Slot 2: construction
1, concentrated 1-2, artillery upgrade, armour 1, engineers 2, logistics 1-2, excavation 2, radio 3, rubber 3. Slot 3:
electro-mechanical engineering, construction 2, anti-tank upgrade and AT 2, excavation 1, armoured engineers, armour
3, gun upgrade. **Slot 4: Basic Medium Tank from day 1** (a 1938 tech: about 21 months ahead of time from January
1936, done about autumn 1937, a year before an on-time start), then Radio (the gate of Advanced Panzer Research),
rubber 1-2. Slot 5: machine tools II -> III (Four Year Plan bonus) -> IV (Autarky bonus) -> mechanised (from mid-1939,
half a year ahead). Bonuses: the two 100% industry bonuses on machine tools III and IV (section 4 items 8 and 10),
the 100% medium bonus on the improved medium chassis (mid-1938), the faction's 50% bonuses on concentrated 3-4 and
excavation 2-3. Against the AI the line infantry attacks with line artillery, so interwar artillery (1936) and
Artillery I (on time, 1939) stay in the plan (the multiplayer build skips artillery techs because its infantry only
holds); anti-tank ahead of time from mid-1937; radar from the special project `sp_air_radar`; encryption techs are
hidden with La Resistance; Mobile Warfare stays the doctrine through Poland and France (the start doctrine; a Grand
Battleplan switch is a late, infantry-war choice). Keep the slots busy; hold one empty only for a named tech (the bonus
waits above, RE04), up to 10 days.

**Production** (a rifle costs 0.5 IC; a military factory makes 4.5 IC a day, 3.5 unpowered). Day 0, before the first
unpause, every factory onto an EXISTING start line (it keeps the line's efficiency): rifles 10 -> 18, artillery 1 -> 4
(the three start plane lines close, their factories move here), support 2 -> 3 and trucks 2 -> 3 (the two Panzer II
factories); no plane or light-tank line on day 0. Then: rifles to the Anschluss count, then the rifle line down once
stock covers training; the medium-tank line on the day its design exists (about October 1937) at 8 factories, 16 by
April 1938, 24 by October 1938, 30 by March 1939; trucks to 5 for the motorised battalions; fighters from January 1937
and CAS from January 1938 (Junkers), sized to beat the AI's 1939 air force (about 1,400 fighters and 900 CAS) by a
tenth; AT from 1939; mechanised on its tech; idle factories go to items that never go obsolete (support, trucks,
trains, rifles). Never move factories off a mature line. Factory benchmark: Germany in the 1939.1.1 scenario has 69
civilian, 62 military, 13 naval (with Austria and the Sudetenland); a good run is above that in January 1939. Rival
benchmarks at that date: FRA 45/9, UK 40/18, USSR 65/47, Italy 29/29.

**Construction**: civilian factories from day 1 in the best-infrastructure western states (Hessen, Westfalen,
Moselland first), military factories from about June 1937 (2-2.5 years before the war: civilian factories queued in
the last 18 months never repay), both Organization Todt focuses done before that wave; the refinery wave from July
1938, sized to the wartime rubber deficit (14,500 each, 3 a state, 1 rubber plus 1 per rubber-processing tech, 48 fuel
a day; seven to twelve cover Germany's rubber once sea trade closes), in infrastructure-5 or dam states, then
military factories again; an air facility at war. Infrastructure only in slot-rich states the Autobahn does not max.

**Design companies** (an MIO on every line; production traits first: capacity, then cost, then output; then soft
attack): Mauser (rifles) and Rheinmetall (artillery) from 1936; **Henschel** on the armour from the medium design
(policy Cutting Corners, then Assembly Line Optimisers from mid-1938, Armor Welding Specialists from late 1939 when
the tanks move to Skoda); **Skoda** after Czechoslovakia, used once Henschel reaches level 10; **Junkers** on CAS;
Opel when the truck line grows. A new variant of a mature line switches WITHOUT the MIO first (production-cost clicks),
then the MIO is applied: switching with the MIO loses the line's efficiency.

**Templates** (columns of 3 so each gets a regimental slot; the designer opens 4 rows until a doctrine milestone):
- attack line **Infanterie-Division (A)**: 7 infantry + 2 line artillery, engineer, recon, artillery support, width 20
  (fills plains 0.98 and forest 1.00). A NEW template: the start Infanterie-Division (9 infantry, engineer, artillery)
  is never edited and holds the West, the coast and East Prussia, so no fielded division drops strength or waits for
  guns. Size the artillery line before the template trains: about 60 guns a division, 1.4 a day per mature factory.
- **Panzer-Division** from the medium design: 9 medium + 8 motorised, width 34 (plains 0.97); engineer, recon,
  artillery, logistics. Mechanised replaces motorised from 1940; 12 tank + 8 mechanised (38-40 wide) only for the
  open plains of the USSR (40 wide fills forest at 0.67). The three start divisions are edited when medium stock
  covers them (3 x 450). Medium design: medium chassis, 2-man turret, close-support howitzer, riveted armour, gasoline
  engine, radio, no armour clicks (against the AI about 87% of the soft attack for about 40% of the cost).
- motorised exploitation 9 motorised; garrison Sicherungs-Division; mountaineers only for the Alps and Norway later.
Separate tank plus motorised/mechanised assault divisions from foot infantry (FAIR).

**Army sizing**: ~30 divisions in 1936 -> 45 (end 1936) -> 75+ (end 1937; the Anschluss ratio wants about 90-96
equipped divisions in spring 1938) -> 95+ (mid 1938) -> about 145 on 1 September 1939 (the AI fields 129): 8
Panzer, 10 motorised, about 72 attack-line, about 40 holding, garrisons. Against the AI 4-8 real tank divisions win;
about 90% of fights are against infantry, so soft attack decides. Army groups under field marshals, armies of 24 or
fewer, a deployment state on every training line; exercises from spring 1936 (land) and day 1 (air and navy), stopped
before the war.

**Command power and officers**: command power starts at 0 (+0.4 a day, cap 80; a promotion costs 40): promote one
general to field marshal each time 40 is banked, the army-group commanders the operations name first (the
multiplayer build promotes everyone on day 1, which needs banked power); the Westwall's 25; assignable traits (15)
for the marshals: Manstein and Weichs Combined Arms Expert, Fortress Buster, Offensive Doctrine; Guderian Organizer,
Logistics Wizard, Thorough Planner. Army experience: Political Loyalty in the academy on day 1 (stability lifts
output; generals cost half), the medium design, Motorization Drive (armour design cost -100%) on the Basic Medium
day, then the templates, then doctrine.

**Intelligence**: found the agency on day 1 (enough for full collaboration on France and part of Poland by 1939);
upgrades Invisible Ink, Form Department, Army Department, Passive Defence x4 (five cheap upgrades give the extra
operative; spies are rarely lost since 1.18, so no Suicide Pills); decrypt France, then the UK. Collaboration
governments in order: France 3, Poland 3, Belgium 2, Netherlands 2, Denmark 2, Norway 2 (each success +0.3; a
collaboration lowers the surrender limit and lifts compliance); none in the USSR (FAIR Axis rule); the UK after the
landing. Faction goals when the Axis exists: Unite the German Cores, spymaster, Expand the Faction, Joint Command,
Joint Research; medium goals 2 x 50% industry, then 2 x 50% excavation.

**Air and navy**: train the air wings and most of the fleet on day 1 (the four capital ships hold coastal dominance);
no plane line until 1937; the Poland air plan puts at least 400 fighters and 400 CAS over the Polish front from day
one; CAS to about 60 factories in early 1940 on the improved CAS airframe.

## 7. Poland and the defended-border rule

Prerequisites at declaration: Danzig or War done and the war goal visible; July 1939 or later; 950k manpower; Westwall finished; **fronts for every border**: France
(Saar, Alsace-Lorraine, Upper Rhine), Belgium, Netherlands, Luxembourg, Denmark, Switzerland, Austria/Italy, Lithuania/East Prussia, and every coastal region (North Sea, Baltic ports Kiel,
Hamburg, Bremerhaven, Stettin, Konigsberg). Minimum: West 30+ divisions in two armies behind bunkers plus a mobile reserve of 4 divisions; 2 Denmark, 3 Switzerland/Alps,
1 per major port, fighters and AA over the Ruhr. East: 3 armies (East Prussia/Pomerania, Silesia, Slovakia axis) with 6-8 tank/motorized divisions as the exploitation arm; air superiority over
Warsaw/Lodz first; the armour is the Panzer divisions of 9 medium tanks and the line attack is the 7+2 artillery template; three Polish collaboration governments before the declaration lower the surrender limit. Planning needs about 15 days (2% a day to the 30% cap). After capitulation reassign east armies at once, check for UK/France attacks, and check unit positions
rather than orders (move orders can leave stacks in place).

The Allies join through the guarantee chain; expect France to attack within days. Every border has its front and reserve before the declaration, so the first week runs at speed 3 like the rest of the war.

## 8. West

Order: Around the Maginot (war goals vs Belgium/Netherlands; France is already at war through the Polish guarantee, three French and two Belgian and Dutch collaboration governments come before the offensive), optional Weserubung (needs the war with ENG and Warsaw and Danzig controlled; Norwegian ore; spend its transport research bonus on landing craft), War with France. Plan: pin the Maginot sector; main effort through the
Netherlands/Belgium and the Ardennes toward Sedan and the Channel; air bases forward before the offensive; infantry follows tanks, pockets closed on the Somme and Channel. Lorraine frontal assaults
grind. After each capitulation redraw fronts and check supply (target above 70% before the next bound). France capitulation and Paris control unlock `form_greater_german_reich`
visibility but not availability.

## 9. Britain plan

Prerequisites: war with the UK; 1,000,000 army manpower; more than 90k infantry equipment in armies; the Operation Sea Lion focus (5, after War with France or Invade Italy) adds a 180-day idea:
+25% naval invasion prep speed, +25% shore bombardment, -15% invasion penalty, +5% attack vs ENG, marine/para contribution -25%. Invasion capacity by tech (divisions / plans / prep days):
base 4/1/60, transport 6/2/50, landing craft 10/5/40, tank landing craft 15/9/30; Sea Lion needs landing craft (10 divisions, 5 plans, 40 days). Launch gates: 60% control in every route region
(not supremacy), naval intel 40%, air superiority over the route and the landing, radar on the Channel coast.

Plan: (1) 1940-H2: take Channel ports, build 20+ airfields in Pas-de-Calais/Belgium; win air superiority over the south-east (fighters, then tactical bombers; radar and AA for own bases);
(2) navy: 100+ torpedo destroyers plus light cruisers 1 per 4 destroyers, U-boats in task forces of 8-12, one per raid region in deep ocean, 300-600 naval bombers by mid-1940, 400+ convoys; no capital
ships; Kriegsmarine focus chain `GER_strengthen_the_kriegsmarine` -> `GER_trade_interdiction` -> `GER_wolfpack_tactics` (needs war) -> `GER_u_boat_efforts`; (3) rehearse with one small landing to prove ports and supply;
(4) launch Kent/Sussex only when the launch gates hold; follow with a second wave, capture a port, repair it, bring fighters over; (5) London and Midlands
for surrender progress. If the landing fails, accept the loss and rebuild; no reload. Confirm the UK capitulation from a fresh read; it does not end the war with the Allies.

## 10. USSR plan

Gate list before Anti-Comintern/War with the USSR: UK capitulated or contained; Poland/France garrisons stable; 25+ motorized/mechanized or tank divisions; fuel and truck stock for a
2-month offensive; 3000+ aircraft-equivalent air force; supply hubs and rail in Poland/Romania ready; non-aggression pact either expired or deliberately broken after 12 months. Focus
chain: Anti-Comintern Pact (5) -> War with the USSR (10; `annex_everything` goal, no expiry; mutually exclusive with Alliance). Execution: three army groups on Leningrad (195), Moscow (219),
Stalingrad/south (217); infantry holds the widening front, tanks pocket; avoid winter/mud pushes. Soviet capitulation list: record each.

When the UK and the USSR fight Germany together, the AI USSR grants the Allies air base access and their fighters and CAS fly from Soviet bases: keep a fighter reserve for the Eastern Front.

## 11. Greater Germanic Reich and completion

The decision is `form_greater_german_reich` ("Proclaim the New Order", cost 25 PP; category title "The Greater Germanic Reich of the German Nation").
Allowed: original GER with Waking the Tiger (integrated). Visible: fascist GER, flag not set, and (at war with a capitulated France, or Paris state 16 owned/controlled,
or Moscow 219 controlled). Available: not a subject; **Paris 16, Prague 9, Warsaw 10, Leningrad 195, Moscow 219, Stalingrad 217 each fully controlled by Germany or a subject; and every owned state fully
controlled by Germany** (an occupied German state blocks it). Effect: +5% war support, +5% stability, cosmetic tag "Greater Germanic Reich", Germania (province 6521) +25 VP. Do not infer completion from
the map color; read the decision. A communist `form_greater_proletarian_state` exists and is not ours.

Success predicates: all of section 1 final row plus: UK, USSR, France, Poland, Czech lands settled; remaining Allied majors listed and at peace or capitulated; peace ownership checked separately
from battle control.

## 12. Run record checklist

Record each diplomatic acquisition (dates), Danzig or War outcome and war goal, Polish/French/UK/Soviet capitulation, British landing and supplied bridgehead, the formable decision result, the
remaining-war list, final peace ownership, MEFO/consumer-goods readings at each year end, and every failure. Success = all predicates true, not a large army or green front.
