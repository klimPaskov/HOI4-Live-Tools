# Poland META showcase (1936 start, HOI4 1.19.3 with No Step Back's Polish tree)

Third campaign, explicitly META: Normal difficulty, vanilla, historical AI and every common campaign integrity rule stay. META lifts the
FAIR house bans on normal-UI exploits (see [rules-and-modes](rules-and-modes.md)); it does not allow console, debug decisions, event firing, save edits,
memory writes, reload or any privileged command. Untagged text is [H]. Evidence tags: **[V path:key]** verified in the installed files; **[L]** seen in Mac runs; **[H]** heuristic or candidate.
Paths: PF = `common/national_focus/poland.txt`, PEV = `events/NSB_Poland.txt`, PDEC = `common/decisions/POL.txt`, FORM = `common/decisions/formable_nation_decisions.txt`,
HIST = `history/countries/POL - Poland.txt`. One focus point = 7 days [V common/defines/00_defines.lua], so cost 5 = 35 days, 10 = 70 days.

Default endpoint (declare before day one unless the user sets another): survive independently; capitulate Germany and the USSR and obtain a settlement leaving no war with either;
restore the 18 starting Polish states (Warsaw 10, Danzig 85, Poznan 86, Lodz 87, Krakow 88, Stanislawow 89, Kielce 90, Lwow 91, Lublin 92, Wolyn 93, Polesie 94, Nowogrodek 95, Wilejka 96,
Bialystok 97, Plock 98, Katowice 762, Vilna 784, Gdynia 807 [V history/states]); a connected Polish-led settlement; at least one consequential META tactic with before/after evidence tied to a named operation.

## Contents
1. Checkpoint table
2. Start facts and the clocks that kill you
3. Routes compared
4. Deterministic monarch election
5. Dated plan 1936-1939 (two tiers)
6. Intermarium and the Commonwealth: exact conditions
7. Fronts, production, army
8. META toolbox: what is legal, what is cheating
9. Defeating Germany and the USSR, war timeline, exile fallback
10. Do-not list
11. Evidence checklist

## 1. Checkpoint table

| Date | Must be true |
|---|---|
| 1936-01-01 | Start [V HIST, history/states, history/units]: 18 civilian / 11 military / 3 naval factories, 3 research slots, 40 division entries (28 infantry, 2 mountain, 10 cavalry brigades), trade law uncontrolled_exports (other laws: read them in game), neutrality 38 / democratic 32 / communism 19 / fascism 11, ideas POL_april_constitution_1, POL_looming_peasants_strike, Sanation left/right opposition level 1. Four Year Plan started. |
| 1936-03-11 | Regency Council done: monarchy, Sanation oppositions removed [V PF:POL_assemble_the_regency_council]; Fifth of November (04-15) removes the April-constitution idea. |
| 1936-05 | Monarch election event arrived (35 days after Fifth of November); chosen candidate recorded. |
| before 1936-12-31 | `POL_agrarian_reform` complete: otherwise the 365-day Looming Peasants' Strike becomes the Great Peasants' Strike (building speed -50%, factory output -50%, consumer goods +50%, stability -20%) and a 150-day strike war [V PDEC:POL_looming_peasants_strike_mission]. |
| 1937-06-30 | Army manpower rising toward 700k [V PF:POL_the_between_the_seas_concept]; 25+ factories on military lines; no idle research slot (`POL_additional_research_slot1` gives the 4th slot; the 5th via slot2 needs more than 50 factories). |
| 1937-12 | Intermarium faction created and invitations out (Czechoslovakia, Yugoslavia, Austria, Hungary); Austria's status recorded before Germany's Anschluss (March 1938). |
| 1938-06 | Plan West started or done; first border bunkers and `POL_local_western_plans`; 50+ equipped divisions [H]. |
| 1939-01-01 | Benchmark [V history/states 1939 scenario]: Poland 18/11/3 (flat), Germany 69/62 with Austria and Sudetenland, USSR 65/47. Be well above 18/11 here; Danzig still Polish, controlled. |
| 1939-06-30 | 90+ divisions [H] with 70%+ equipment; bunkers on all German-facing states; air defences; reserve armies behind Poznan, Lodz, Krakow. |
| 1939-08/09 | Germany's Danzig or War resolved [V NF-GER, EV germany.86]; war with Germany with a prepared line; fronts for Germany (East Prussia, Pomerania, Silesia, Slovakia) and the Soviet border drawn. |
| 1940 | Poland still independent, capital intact; German offensive halted at a river line [H]. |
| 1941-1945 | German capitulation and Soviet capitulation recorded with fresh evidence; remaining wars listed; final peace leaves Polish-held states per the checklist. |

## 2. Start facts and the clocks

- **Looming Peasants' Strike** (365-day mission from 1936-01-01) [V PDEC:POL_peasant_strike_category]: cancelled by `POL_agrarian_reform` (10, needs `POL_fill_the_railways_gaps`, which needs `POL_the_four_year_plan`).
  Minimum chain 35 + 70 + 70 = 175 days. Do it before everything else that is not a root.
- **Sanation oppositions** escalate every 175 days to level 4 [V PDEC:POL_sanation_decisions]: left 4 = stability -25%, consumer goods +20%; right 4 = war support -25%, conscription -30%.
  Removed by the Regency Council, `POL_camp_of_national_unity`, `POL_ozon` and similar political foci.
- **Danzig** (state 85, international city): Germany's `GER_danzig_or_war` needs Poland to own and control state 85 and 950k German army manpower; AI Germany takes it after 1939-01-01 [V NF-GER ai_will_do].
  Polish decisions `POL_unification_propaganda` (75 PP), `POL_military_appeasement`, `POL_raid_nazi_resistance`, `POL_tighten_control` manage the city [V PDEC]. If Poland refuses (event germany.86), Germany
  gets a `take_state` war goal on states 85 and 807; if Poland accepts, Danzig and Gdynia pass to Germany [V EV:germany.86/87]. The historical AI always refuses.
- **UK/France guarantee**: the guarantee event fires after German added tension above 14 and Integrate/Fate/Reassert [V events/GOT_Generic_Events.txt]; it does not replace your own preparation.
- **MR Pact**: if AI Germany completes `GER_molotov_ribbentrop_pact`, the USSR receives claims on eastern Poland (states 89, 91, 93-97) [V EV germany.11]; plan for a two-front end of August 1939.

## 3. Routes compared (political roots are mutually exclusive)

| Root focus | Branch | Strength | Cost / risk | Verdict |
|---|---|---|---|---|
| `POL_assemble_the_regency_council` (5) | monarchy: Friedrich Christian -> Lithuania/Commonwealth; Habsburg -> Czech lands/Hungary; Hohenzollern -> Romania; Bermondt (fascist) -> Cossack/Baltic; Romanov -> Russia | Removes oppositions at once; peaceful expansion; Commonwealth formable | Neutral ruling party; election cycle costs 35 days per skipped monarch | **Primary** [H] |
| `POL_complete_april_constitution` (10) | Sanation: Plan West/East; `POL_align_with_the_west` -> `POL_join_allies` | Western alliance and guarantee | April constitution idea (political power -15%, advisor/economy/mobilisation-law costs +15%) until replaced; oppositions escalate until removed | Fallback for a purely defensive game |
| `POL_restore_the_sejm` (10) | democratic: `POL_draw_closer_to_britain`, `POL_intervention_focus` | Liberal drift, Britain | Gives up the monarchy/Intermarium synergies [H] | Not chosen |
| `POL_organize_the_peasants_strike` (10) | Morges/communist | Soviet-friendly | Communist drift, Soviet-leaning government conflicts with the endpoint | Not chosen |
| `POL_nationalist_constitution` (10) | Falange/Endecja -> `POL_beck_ribbentrop` -> German alignment | Might avoid German war | Makes Germany's defeat a betrayal; conflicts with the endpoint | Not chosen |
| `POL_the_between_the_seas_concept` (10, independent root) | Intermarium faction | Austria, Czechoslovakia, Yugoslavia, Hungary, Nordics, Italy, Turkey/Greece | Needs 700k army manpower; faction leader duties | **Primary**, combined with the Regency |

Habsburg route [V PF]: requires Karl Albrecht ruling; `POL_restore_the_diet_of_galicia`, `POL_support_monarchy_in_CZE`, `POL_king_of_bohemia` (Czechoslovakia subject or 80% monarchist support and no war), `POL_unite_west_slavia`; choose it only if Czechoslovakia
is the priority. Romanov route needs flag `romanov_enabled` (reached late in the election cycle) and ends in `POL_claim_russia` / `POL_proclaim_slavic_unity` (control Moscow or Leningrad, plus Kiev/Stalingrad region and USSR surrender > 0.5 or dead).

## 4. Deterministic monarch election [V PEV:NSB_poland_royal_election.1-5]

After `POL_fulfil_fifth_of_november` completes (35 days, needs neutrality above 20% and democratic below 50%) the first offer arrives 35 days later. The cycle is fixed, not random:
1 Friedrich Christian (Wettin; unlocks `POL_claiming_lithuania`, Commonwealth) -> "keep looking" 35 days -> 5 Karl Albrecht (Habsburg; unlocks `POL_habsburg`) -> 2 Michael of Romania (Hohenzollern; `POL_hohenzollern`) -> 3 Pavel Bermondt-Avalov
(fascism, `POL_cossack_king`) -> back to 1 once, then 4 the Romanov pretender (sets `romanov_enabled`, unlocks `POL_romanov`). Accept the first offer for the Commonwealth route. Never fire these events by script.

## 5. Dated plan 1936-1939

Tier 1 (defence first), earliest-start dates [H order, V effects]:

1. **POL_the_four_year_plan** (5; to 1936-02-05): construction tech +100% once, +10% building speed for 1460 days.
2. **POL_assemble_the_regency_council** (5; to 03-11) then **POL_fulfil_fifth_of_november** (5; to 04-15); accept Friedrich Christian when the event arrives (~05-20).
3. **POL_fill_the_railways_gaps** (10; to 06-24) then **POL_agrarian_reform** (10; to 09-02): beats the strike clock with margin.
4. **POL_central_region_strategy** (5; to 10-07), **POL_expansion_of_new_towns** (5; arms factories in 86/807; to 11-11), **POL_start_central_industrial_region** (10; +1 arms +1 civilian factory, state 88; to 1937-01-20).
   Later, as time allows: `POL_invest_in_the_old_polish_region` (5; +arms 88), `POL_modernize_congressional_factories` (10; Warsaw, Kielce), `POL_expand_central_industrial_region` (10), `POL_develop_upper_silesia` (5),
   `POL_expand_katowice_mines` (10; +24 steel, +12 aluminium), `POL_additional_research_slot1/2` (10 each; the 5th slot needs more than 50 factories).
5. **POL_prepare_for_the_next_war** (5; to 02-24), **POL_new_military_academy** (10; leader cost -30%, start level 1; to 05-05), **POL_standardisation_of_equipment** (5; infantry-weapons tech bonus 100%; to 06-09).
6. **POL_the_between_the_seas_concept** (10; to 08-18): creates the faction (the player may create factions via `can_create_factions`); needs 700k manpower and not being a subject.
7. **POL_the_mediterranean** (5), **POL_coerce_czechoslovakia** (5), **POL_austro_hungarian_alliance** (5): to 12-01, together with decisions `POL_diplomatic_mission_to_CZE/YUG/AUS/HUN` (10 PP each, 35 days).
   Austria in this faction before March 1938 makes `GER_anschluss` bypassed [V: bypass when AUS is in a faction with another independent country that is not Germany]. Record the result.
8. **POL_plan_west** (10; border-state bunker speed +30% for 547 days) -> **POL_local_western_plans** (unlocks decision `POL_defend_in_west_state`: +10% attack, +20% defence, +15% breakthrough for 60 days, 50 PP) ->
   **POL_silesia_fortified_area**, **POL_the_prusya_line** (bunkers in 97, 98, 807), **POL_expand_poznan_forts**, **POL_hel_fortified_area**, **POL_invest_anti_air**; plus **POL_plan_east** and `POL_local_eastern_plans` if the USSR threatens.
9. **POL_army_modernisation** (10; doctrine cost -50%, weapons bonus), **POL_artillery_modernisation**, **POL_anti_tank_guns**, **POL_adaptive_designs** for armour or anti-tank, and `POL_air_base_expansion` -> `POL_fighter_modernisation`.

Tier 2 (Commonwealth, if time or after 1939): **POL_claiming_lithuania** (10), **POL_restoration_of_the_royal_sejm** (10), **POL_support_monarchism_in_LIT** (10) with decisions `POL_propagandise_monarchist_sentiment`,
`POL_double_propagandise...`, `POL_sway_lithuanian_generals`, `POL_invest_in_lithuania`, **POL_institute_royal_guards** (10; creates two XP-1.0 Royal Guard divisions), **POL_king_of_lithuania** (5), **POL_LIT_union** (10),
then **POL_claim_livonia** (10; annex goals on Estonia/Latvia), **POL_claim_prussia**, **POL_claim_greater_lithuania**, **POL_greater_commonwealth** (10). Swap rule: if Germany has already annexed Austria or the manpower gate is missed, move Plan West forward
and postpone Tier 2 [H].

## 6. Intermarium and the Commonwealth: exact conditions

**Intermarium** [V PF, PDEC:POL_miedzymorze_decisions]: `POL_the_between_the_seas_concept` available if not a subject, major or faction leader or unaligned, army manpower above 699,999, and if Baltic/Falange/Morges
alliance foci were taken their counterparts too. Reward: leave any faction, `set_rule can_create_factions`, `create_faction_from_template faction_template_miedzymorze_faction` (NCNS manifest `faction_manifest_between_the_seas`), Romania receives event poland.16.
Sub-foci need `is_faction_leader`: Baltic alliance (Latvia/Estonia), Treaty with Lithuania (Lithuania alone), North Sea (Finland, then Denmark/Sweden/Norway), Mediterranean (Czechoslovakia, Yugoslavia; then Austria/Hungary, Italy only if ideology matches, Greece or Turkey exclusive).
Each unlocks one 10 PP / 35-day mission decision; acceptance is the target AI's choice, so keep opinion high.

**Commonwealth** [V FORM:form_commonwealth]: decision for POL or LIT; available if not a subject, controlling Lithuania's states 11 (Kaunas) and 189 (Zemaitija), controlling the Polish states 86, 87, 88, 90, 98, 10, 97, 92, 91, 89, 93, 94, 95, 96, 784, and core compliance in the other
country above 80. Effect: cosmetic tag `PLC_UNIFIED` (or `PLC_UNIFIED_RUS` under a Russian-style monarchy), `POL_basic_form_commonwealth`. `POL_LIT_union` activates it. **POL_greater_commonwealth** requires control of 36 states (Belarus, Ukraine, Smolensk-Tula line,
Crimea, East Prussia 5/763, Silesia 66/67, Pomerania 63, Ostmark 68) [V PF]: a victory-stage goal, not an opening.

## 7. Fronts, production, army

- Start allocation [H]: 11 military factories: rifles 6, support 1, artillery 1, AT/AA 1, tanks/armoured cars 1, fighters 1 as soon as possible; civilian factories on construction in Warsaw, Lodz, Krakow, Kielce; avoid the vulnerable western border states for factories.
- Army sizing [H]: 40 entries -> 60 equipped divisions by end 1938 -> 90 in mid-1939. Corps-level template: 7/2 infantry with AA/AT/engineer support; cavalry brigades consolidated; armoured reserve of 4-6 divisions; commanders of 24 or fewer divisions.
- Fronts [H]: Germany from three directions (East Prussia/Pomerania/Danzig corridor; Silesia/Poznan; Slovakia) requires a pivot defence behind the Vistula, San and Narew; USSR border long and unfortified; assign a minimum screen of 8-10 divisions
  east with reserve moves by rail; fortify river towns, not the whole frontier.
- Research [H]: construction1/2, basic_machine_tools, infantry_weapons1/2, support tech (engineers, recon, AA/AT), artillery1, radio; doctrine for defence; basic_medium_tank_chassis only if the armour tactics below are used.
- Laws [V ideas]: economy `partial_economic_mobilisation` -> `war_economy` needs war support above 0.5 and fascism/communism or war with a country above IC ratio 0.4, so unavailable to a neutral Poland until war; manpower `limited_conscription` -> `extensive_conscription`
  requires fascism/communism or war with strength ratio above 0.5 [V common/ideas/_manpower.txt]; plan around it.

## 8. META toolbox

| Candidate | Status | Bounded test and evidence |
|---|---|---|
| Diplomatic gate-block: Austria in the Intermarium bypasses `GER_anschluss` | verified in files | Record Austria's faction state each month before 1938-03; success = Anschluss focus skipped without effect. |
| Paper-division manpower inflation to hit 700k (`has_army_manpower`) | candidate [H] | One bounded test: train a handful of low-equipment divisions and watch the Between-the-Seas availability tooltip. Fill with equipment afterwards; never call a paper army combat-ready. |
| Hybrid armoured infantry (tank battalion inside infantry divisions) | candidate | Duplicate a template, add the line armour type, check stats and equipment demand, convert 2-3 divisions first; observe battle losses vs ordinary divisions on a named sector. |
| Airborne VP/port seizure on a named objective | candidate | Check air superiority and range in the tooltip; plan fallback; accept losses (no reload). |
| Personal-union puppet manpower (Lithuania via `POL_king_of_lithuania`, `autonomy_personal_union`) | verified in files | Peaceful gain of divisions/cores; record manpower before and after `POL_LIT_union`. |
| AI baiting: let German AI overextend in Poland, then pocket | normal play | Hold the river line; counter-attack only into pockets with escape routes cut. |
| Encircled-template swap | unavailable (patched 1.10.6) | Do not attempt. |

Cheating (forbidden in META too): console or debug decisions (`debug_decisions` exists in the files), forcing events (monarch election, Danzig), editing saves or game files, memory writes, privileged command posting, reload-to-undo, tag switching,
unreproducible "exploit" claims. A tactic only counts as the required META demonstration with a named objective, measured contribution and cost.

## 9. Defeating Germany and the USSR, war timeline, exile fallback

Germany [H]: absorb the opening on prepared lines; exhaust attacks with defensive fighting; counter-attack exposed salients; take East Prussia and Silesia hubs first, then Berlin via Poznan-Frankfurt; use air superiority and encirclements; list capitulation and settlement.
USSR [H]: supply in bounds; Smolensk, Minsk, Kiev axes; do not drive east unsupported. Whichever major is attacked second needs a reserve held back for it. Capitulation requires occupying the capital region and enough victory points; verify `surrender_progress`.
Peace: rank the Polish core checklist first, then Lithuania/Prussia/Ukraine objectives, then the Commonwealth decision conditions. Verify final ownership, not battle control.

**War timeline [H, triggers V]**
- T-120 days: the German `GER_danzig_or_war` gate is 950k army manpower and Polish ownership/control of Danzig [V]; a German army above 90 divisions and a completed Reassert Eastern Claims
  (claims on 85, 86, 762, 807, Memel) are the visible tripwires. Stop optional foci and put everything into bunkers, rifles, AA, fighters and the reserve.
- T-30: all fronts drawn (Germany three axes, Soviet border screen); armies of 24 or fewer under field marshals; trains and trucks stocked; air bases rebased east of the Vistula; use `POL_defend_in_west_state` decisions on the states
  where the German axis is confirmed (50 PP, 60 days each).
- Day 1-14: trade space only where terrain helps (Narew, Vistula, Warta, San); do not hold the open Silesian plain with infantry alone; keep 4-6 divisions as a counter-attack force behind Lodz/Kielce; fighters on air superiority over the Warsaw-Lodz-Krakow triangle.
- Day 15-60: the German operational tempo should slow at the Vistula line; counter-attack pockets with escape routes cut; recruit and equip the next wave from reserves; watch Danzig/Gdynia (Hel fortified area) and Slovak approach to Krakow.
- Western relief: expect the Allies to declare after the guarantee; their offensive is not guaranteed to land, do not plan around it.
- Soviet entry: with an MR pact the USSR holds claims over states 89, 91, 93-97 [V]; move the screen east, accept losing the far east to save the core if the Vistula line holds.

**Exile fallback** [V PF:POL_prepare_for_the_inevitable, cost 10, available if capitulated or at war with GER/SOV in a faction with surrender progress above 0.6]: foreign air support, foreign army support, aces in exile, foreign naval support, resistance industries,
exile industries, intelligence branch (`POL_expand_polish_intelligence`, `POL_niech_zyje_opor`, `POL_the_long_push_home`, `POL_the_bombe`, `POL_the_cyclometer`). Use it to keep the war and the settlement alive; record the loss honestly. No reload.

## 10. Do-not list

- No Sanation-and-monarchy mix (mutually exclusive roots); no skipping `POL_agrarian_reform`; no waiting past 1938-03 to start the Intermarium if the Anschluss block is wanted.
- No accepting `germany.86` Danzig terms (loses Gdynia too) unless the endpoint was rewritten before day one.
- No fighting Germany and the USSR at full commitment on both fronts at once; no unfortified long front; no unattended speed above 1 around declarations.
- No console, debug decisions, event firing, save editing, reload; no passing off an ordinary tactic as the META demonstration.

## 11. Evidence checklist

Record: chosen route and each election offer; strike-clock status; manpower at each gate; Intermarium members by date; Austria/Czechoslovakia fate; Danzig decision; first German/Soviet contact; each META technique with source, test, cost, outcome;
German and Soviet capitulation evidence; unresolved wars; final settlement ownership against the 18-state checklist; economy and force condition. Success is the declared predicates, not a clever tactic or a formable alone.
