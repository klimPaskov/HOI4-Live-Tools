# Nationalist China FAIR showcase (1936 start, HOI4 1.19.3 with No Compromise, No Surrender)

Objective: unify China including warlords and communists, expel and capitulate Japan, recover lost
territory and finish as a top-tier power. Normal difficulty, vanilla rules, historical AI, FAIR. Play the
Nationalist tag `CHI`, never Communist China or an unrelated tag.

Untagged text is [H]. Evidence tags: **[V path:key]** verified in installed files (build 1.19.3.0); **[L]** seen in Mac runs;
**[H]** heuristic, check in game. Paths: NF = `common/national_focus/china_nationalist_sea.txt` (NCNS tree
`china_nationalist_sea_focus`, active because NCNS is installed; the old `china_nationalist.txt` tree is NOT used),
DEC = `common/decisions/CHI_decisions.txt`, HIST = `history/countries/CHI - China.txt`, JAPDEC = `common/decisions/JAP.txt`.
One focus point = 7 days [V common/defines/00_defines.lua], so cost 5 = 35 days, 10 = 70 days.

## Contents
1. Checkpoint table
2. Start facts and national debuffs
3. Tree structure and the political choice
4. Dated focus plan (peace, war, unification)
5. Warlords: mechanics, faction goals and sequence
6. Communists
7. Japan: timing, defence, counter-offensive, capitulation, recovered territory
8. Laws, research, production, army sizing
9. Foreign assistance, air and navy
10. Contingencies
11. Do-not list
12. Measurable end state and evidence

## 1. Checkpoint table

| Date | Must be true |
|---|---|
| 1936-01-01 | Start [V HIST, history/states]: 28 states, 13 civilian / 11 military / 2 naval factories, stability 0.20, war support 0.32, ~85 division entries in `CHI_1936_sea` (many 3-battalion "Juntuan"), neutrality 91% / communism 7% / fascism 2%. Warlords SIK, XSM, SHX, YUN, GXC exist (MEN and MAN are Japanese clients); PRC holds 3 states (Shaanbei 622, Yan'an 1032, Yulin 1046). Focus started on day one. |
| 1936-03-11 | Dang Guo and Denounce Feudalism done; Chiang Kai-shek still the ruler (`CHI_chiang_kaishek`). |
| 1936-05-20 | One Nation Undivided done: a faction exists; at least 2 warlords answered the invitation (event SEA_china.7) by 1936-09 [H]; otherwise improve relations and retry the invitation-granting focuses. |
| 1936-12-31 | Currency + Three-Year Plan + Military Affairs Commission + NCO/field-promotion focus done [H]; all factories busy; first rifle lines running on all 11 military factories; no PP above 150 unspent. |
| 1937-06-30 | Corruption fixes done (Crackdown on Corruption in Recruitment); 40+ divisions at 70%+ equipment [H]; second-line forts and supply hubs under construction; nothing left empty in Beiping 608, Hebei 614, Shandong 597/Qingdao 743, Shanghai 613, Nanjing 1035. |
| latest 1937-12-27 | Japan's Kwantung Army timer (725 days) can fire the Marco Polo event china.1 [V JAPDEC:JAP_kwantung_army_impatience, HIST JAP kwantung_army_timeout_time_var = 725]; earlier if Japan finishes `JAP_reinforce_the_beijing_garrison`. Fronts, reserves and the Yangtze/Yellow River lines ready; PP banked for decisions. |
| 1938-06-30 | No collapse of the northern line; Wuhan 1036 and Chongqing 1037 safe; Gung Ho (needs war) done; Soviet or Western lend-lease flowing (Secure the Soviet Border / Send the First Lady Abroad). |
| 1939-01-01 | Benchmark [V history/states]: CHI 31 civilian / 16 military, 34 states, JAP 30 / 19 / 17 naval. Be at or above it. |
| 1940-12 | Warlords in the faction (at least the tour regions you intend); Japanese front pinned on rivers; 80+ divisions [H]. |
| 1942-1945 | Faction goal `faction_goal_no_japanese_in_china` complete [V]; Victory over the Invaders done; Crush the Communists war running or won; PRC gone. |
| Final | Japan capitulated (Tokyo area occupied, settlement proof), all declared unification states owned/integrated, no enemy on declared states, 100+ factories (60+ military), 100+ divisions at 90%, 500+ aircraft, supplied home-island operation (house targets). |

## 2. Start facts and national debuffs

NCNS start modifiers [V HIST lines 351-470]: Corruption in the Administration (economy cost factor +65%, construction speed -10%, free repair -25%),
Illicit Trade (consumer goods -15%, factory output -15%), Inefficient Economy (factory efficiency gain -35%, base fuel gain -50%), Neglected Countryside (communist drift +0.05,
research speed -20%), Corruption in the Armed Forces (conscription factor -78%, defence -15%, morale -25%, attack -5%, organisation -5%, max dig-in -1), Nine Power Treaty (consumer goods, min export,
enemy wargoal justification time modifiers). Starting techs: infantry_weapons, gw_artillery, interwar_antiair, basic_train, tech_support, tech_trucks. These explain [L] "avoid unlimited recruitment":
the pool is small until the armed-forces focuses repair it. The 1939 scenario has a very different state (war_economy, service_by_requirement, closed_economy); do not assume it.

Rival benchmarks 1936 base states [V]: JAP 25/15/15, USSR 46/33, UK 32/14, ITA 21/20. Warlord economies are tiny (SHX 3/2, XSM 5/2, SIK 4/1, YUN 3/1, GXC 3/1).

## 3. Tree structure and the political choice

Root focuses (no prerequisites) [V NF]: three mutually exclusive political roots - `CHI_end_military_primacy` (10; civilian-republican path, retires Dai Li, Soong Mei-ling, Chiang Ching-kuo,
Chen Cheng, Hu Zongnan, Wang Yaowu), `CHI_dang_guo` (5; party-state, stability +10%, communist and fascist popularity -5%) and `CHI_strengthening_our_resolve` (5; Blue Shirt/fascism path, leads to
`CHI_a_party_unified`, `CHI_expel_the_occupiers`, `CHI_put_an_end_to_PRC`). Also roots: `CHI_promote_the_national_currency` (economy), `CHI_military_affairs_commission_sea` (army), `CHI_rocaf` (air),
`CHI_republic_of_china_navy`.

Choose **Dang Guo -> Denounce Feudalism** [H, reasons V]: it keeps Chiang as ruler, which `CHI_a_generalissimo_for_all_of_china` requires (available: Chiang ruling, `PRC` must not exist) and avoids
`CHI_prepare_for_a_change_in_leadership` (exclusive; gives the leadership to Wang Jingwei and the Wang Front branch). Denounce Feudalism: +5% stability, every warlord gains opinion modifier
`CHI_denounced_feudalism`, unlocks Chen Guofu, registers warlord subjects. The Blue Shirt and civilian roots are legal FAIR alternatives but break the recorded plan (Chiang ruling, PRC removal by focus).

## 4. Dated focus plan (earliest-start dates if nothing gates; adapt when the war starts)

Peace phase (focus slot never idle) [V for effects, H for the order]:

1. **CHI_dang_guo** (5; 1936-01-01 to 02-05).
2. **CHI_denounce_feudalism** (5; to 03-11).
3. **CHI_one_nation_undivided** (10; to 05-20): needs a non-subject warlord who is not at war with us; creates `faction_template_chinese_united_front` (or the Wang Front variant if Communist
   China already holds the united front) and sends event SEA_china.7 to each eligible warlord (not Sinkiang under `SIK_battle_for_sinkiang`, not HBC). Eligible warlords receive the invitation and +5% war support; acceptance is the AI's choice.
4. **CHI_promote_the_national_currency** (5; to 06-24): +25 PP, economy cost factor -10%, corruption effects improved; cheaper laws and advisors.
5. **CHI_the_three_year_plan** (10; needs `has_war = no`; to 09-02): a 1095-day national spirit [V]. Must be taken before Japan attacks.
6. **CHI_military_affairs_commission_sea** (5; to 10-07): army XP +5%, morale +10%, planning +2% (corruption variables).
7. **CHI_formalize_field_promotions** (10; training time -25%, org +1%) or **CHI_set_up_an_nco_school** (10; org +2%, training -15%): exclusive; to 1936-12-16.
8. **CHI_crackdown_on_corruption_in_recruitment** (10; morale +10%, org +2%, event SEA_china_improvement.1): to 1937-02-24.
9. **CHI_protracted_warfare** (10; defensive_postures subdoctrine), then **CHI_defence_in_depth** (10; defence +5%, max dig-in +1; unlocks decision `CHI_breach_the_yellow_river`), **CHI_the_4_defence_areas** (10; coastal state modifier), **CHI_relocate_factories_from_the_coast** (5; evacuation factor 0.75). Decisive Battles is exclusive-by-prerequisite with Protracted Warfare until each side's follow-up is done: finish the defensive chain first.
10. **CHI_introduce_the_baojia_system** (10; adds the "Baojia Juntuan" garrison template and 3 half-equipped divisions) or **CHI_expand_the_tongzi_jun** (10; war support +10%): exclusive; then **CHI_inspect_the_troops** (5; needs to be in a faction).

War phase (Japan attacks): **CHI_demonstrate_our_resolve** (5; needs war with Japan) -> **CHI_secure_the_soviet_border** (10; needs war and USSR; Soviet non-aggression and lend-lease) or
**CHI_send_the_first_lady_abroad** (10; needs a democratic power not at war with us) -> burma_campaign / the_burma_road; **CHI_initiate_the_gung_ho_movement** (10; war required; +10% arms-factory speed, +10% war support,
+2 arms factories in inland states); then **CHI_from_many_to_one** (10) -> **CHI_towards_a_professional_army** (10) or **CHI_towards_a_popular_army** (10; needs 1,000,000 army manpower).

Unification phase: **CHI_the_sky_cannot_have_two_suns** (5) -> **CHI_tour_the_north / tour_the_west / tour_the_south** (5 each; each needs an allied warlord of that region: north = NXM, SHX, GSM, HBC, XIC or SND; west = XSM, KHM, SIK or SIC;
south = YUN, GXC or GDC) -> **CHI_victory_over_the_invaders** (5; needs faction goal `faction_goal_no_japanese_in_china` complete and no war with Japan) -> **CHI_crush_the_communists** (10; `annex_everything` war goal on PRC, `expire = 0`) ->
**CHI_a_generalissimo_for_all_of_china** (10) -> **CHI_subjugate_korea** (10, exclusive with Joining the International Community) -> **CHI_reclaim_the_north** (10). Also **CHI_request_handover_of_coastal_cities** (5; states 326 Hong Kong,
729 Macau, 728 Guangzhouwan) and **CHI_petition_territorial_restoration** (5). Alternative conventional route: **CHI_preemptive_strike** (10; creates annex war goals on every non-allied warlord; do not take while Japan
is the main front).

## 5. Warlords: mechanics, faction goals and sequence

Mechanism [V DEC:CHI_integrate_the_warlords_sea_cat, NF]: warlords join the player's faction; each member has `faction_influence_ratio`. Decision `CHI_press_warlord_to_become_puppet`
needs own influence ratio more than twice the target's, costs 125 PP and 15 points of faction influence (refunded by events if refused), then event SEA_china_warlords.1 asks the warlord; success sets
`autonomy_sea_warlord_subject` (freedom 0.8). After a tour focus the cheaper `..._upgrade` decision costs 75 PP and 10 points. Subjects then drift via `CHI_warlord_autonomy_drift_decision`
(autonomy), lend-lease and economic/industrial support decisions (cost `CHI_warlord_integration_pp_cost`, 180 days each); `CHI_integrating_states` (100 PP, 50 days, after `CHI_empower_the_mtac`) adds cores.
Tour focuses reduce the integration cost for that region's warlords.

Sequence [H]: faction first (spring 1936), keep relations high (improve-relations, Denounce Feudalism modifier), take the tour focuses as soon as an ally of that region exists even while fighting Japan, press the
strongest (Guangxi, Shanxi) first because their land borders Japan or the front. A friendly independent warlord is not unification; integrate or annex. Warlord states: SHX Shanxi 615, Suiyuan 621, Ordos 746;
XSM Gansu 283, Qinghai 604, Ningxia 616 and others; SIK Urumqi 617 and 6 more; YUN Yunnan 325, Dali 747; GXC Guangxi 599, Guangdong 593, Guangzhou 592, Nanning 594, Hainan 591, Nanlu 1033; MEN Chahar 611,
Xilingol 612, Pailingmiao 1043 [V history/states].

**Faction goals define "subjugated"** [V common/factions/goals/faction_goals_short_term.txt]: `faction_goal_north_subjugated` is complete when NXM, SHX, GSM, HBC, XIC ... have no core state left
outside China (all their core states owned by CHI); `faction_goal_west_subjugated` does the same for XSM, KHM, SIK, SIC; `faction_goal_south_subjugated` for the southern group. Visible only to CHI with the
`chinese_united_front` template and the matching tour focus. Medium-term goals `faction_goal_control_the_chinese_capitals`, `faction_goal_control_the_chinese_coast` and `faction_goal_subjugate_the_faction`
exist; the long-term `faction_goal_no_japanese_in_china` (no Japanese-controlled state in `china_potential_core_mainland_states`) completes with +100 power projection, +20 faction influence to big members and
rewards 100 PP / 100 army XP / 100k manpower to qualifying members. Use these as the measurable unification checklist, together with ownership checks per state.

## 6. Communists

`PRC` starts with 3 weak states and a hostile opinion modifier. Options in order of preference [H]: (a) keep them out of your faction, watch `CHI_communist_infiltration_in_*` decisions (Gansu, Shandong, Jiangsu, Henan, Beijing,
Hebei, Shanxi, Suiyuan, Xi'an, Ordos [V DEC]); (b) after Victory over the Invaders take **Crush the Communists**, whose war goal never expires; (c) the Blue Shirt path offers `CHI_put_an_end_to_PRC`
(needs fascism and a border). The Xi'an incident decision `CHI_instigate_xian_incident` (25 PP) belongs to the warlords after 1936-12-01 and can create an early united front with PRC; if it fires, check
whether PRC joined your faction before pressing warlords. Do not fight PRC and Japan at the same time unless the northern line is stable.

## 7. Japan

**Timing** [V]: expect the war by 1937-12-27 at the latest; the Marco Polo event china.1 offers "breaking point" (war, Japan declares annex_everything on CHI, SHX, SND, HBC and subjects) or concessions; the historical-focus rule
removes the concession option for the AI but a player may choose. Prepare so that the war option is survivable.

**Defence** [H]: three layers. North: Beiping 608, Hebei 614, Shandong/Qingdao/Jinan (597, 743, 1038): do not defend every plain; hold the Yellow River and the Beiping-Hankow railway hubs with 2 armies of 12-16 divisions each,
fortify river towns, keep one reserve army (8-10 divisions) behind Henan 607. Coast: dedicated garrisons (2-3 divisions) in Shanghai 613, Nanjing 1035, Hangzhou/Zhejiang 596, Fujian 595, Guangzhou 592, Hainan 591; Japanese
landings must be counter-attacked before a supplied bridgehead. Centre: Wuhan 1036 is the hub; Chongqing 1037 and Chengdu 1041 the fallback. Never empty a port repeatedly [FAIR]. Use `CHI_declare_war_zone`, `CHI_bog_them_down`,
`CHI_holding_state_mission`, industrial evacuation decisions and `CHI_breach_the_yellow_river` only after reading their tooltips (the last has heavy side effects).

**Counter-offensive** [H]: after equipment and supply allow (1940-1942), concentrate one army on a short axis, close pockets, retake northern hubs, then move on Jehol 610, Manchukuo (328, 714-717, 761), East Hebei 609, Dalian 745, Chahar 611 and
Taiwan 524 (all China cores or Japanese/puppet holdings [V history/states]). Expelling Japan from the mainland completes the faction goal; it is an intermediate milestone, not Japanese capitulation.

**Capitulation**: refuse a peace that ends the route unless a lawful later route is recorded. Home-island assault needs transports, escorts, air range, a secured port and the invasion state machine in
[strategy](strategy.md); Kyushu or Shikoku first, then Kansai/Kanto. Defaults [V DEF]: naval invasion preparation 60 days, one plan at a time, base 4 divisions per invasion; techs and ships raise it, read the tooltip.
Japan capitulates only after the capital region and victory points are held; verify with fresh evidence.

## 8. Laws, research, production, army

- Laws (150 PP each; cheaper after the currency focus) [V common/ideas/_economic.txt]: leave default until war nears; then `partial_economic_mobilisation` (+10% arms speed) -> `war_economy` (+20%, needs war support above 0.5 and fascism/communism or war with a country at IC ratio above 0.4); `closed_economy` only at war. Conscription: raise only when rifles exist
  (the armed-forces corruption cuts the pool 78%).
- Research [H keys]: infantry_weapons1/2 (rifle model), tech_engineers, tech_recon, construction1/2, basic_machine_tools, interwar_antitank, support_weapons, radio, then motorised_infantry/trucks for the reserve, air for fighters/CAS.
  Neglected countryside lowers research speed 20%; `CHI_extend_compulsory_education` gives a 25% electronics bonus and lowers the penalty.
- Production [H]: 11 military factories on rifles (8), support (1), artillery (1), trucks (1) in 1936; add anti-tank and AA after the first Japanese armour is seen; navy only after the land defence holds. Civilian factories: construction first (corruption -10% speed),
  then evacuate coastal industry per the focus when war starts.
- Army [H]: 60 equipped divisions by 1937-12, 90 by 1940, 100+ by 1944; generals lead 24 or fewer divisions; army groups under field marshals; separate tank and motorised formations (FAIR). Keep garrison templates for resistance areas.

## 9. Foreign assistance, air and navy

Lend-lease and support are normal FAIR play. Focuses [V NF]: `CHI_secure_the_soviet_border` (non-aggression, `CHI_soviet_support`, Soviet lend-lease), `CHI_send_the_first_lady_abroad` (`CHI_american_support`),
`CHI_the_burma_road`, `CHI_the_hanoi_route`, `CHI_the_hump_sea`, `CHI_collaborate_with_allied_forces` -> `CHI_burma_campaign` / `CHI_indo_chinese_campaign`, `CHI_scout_for_air_experts` ->
`CHI_international_squadron` or `CHI_international_instructors`, `CHI_sell_lend_lease` vs `CHI_allocate_lend_lease` (exclusive). Delivery needs working ports and routes: record actual deliveries, not accepted requests.
German advisors are already a start spirit in the old tree; `CHI_utilize_german_contacts` and `CHI_the_german_mission` are conditional on German opinion/government [V NF].

Air [H]: ROCAF has a -20% air XP start modifier [V HIST]; take `CHI_rocaf` -> `CHI_controlling_our_skies` (defence, interception) before `CHI_projecting_power`; 200 fighters before any CAS. Navy [H]: transports and escorts only,
no battleship programme before 1943; the `CHI_republic_of_china_navy` branch is for late home-island support (long range, strike from below).

## 10. Contingencies

- No warlord accepts the faction invitation by 1936-09 [H]: raise opinion with the improve-relations diplomatic action, keep Denounce Feudalism's opinion bonus alive, and re-check every month; acceptance is the warlord AI's choice (event SEA_china.7). Do not switch to preemptive strikes while Japan is the main threat.
- Japan attacks before 1937-06: skip remaining peace focuses; bank PP; take `CHI_demonstrate_our_resolve` at once; protect Shanghai/Nanjing with garrisons and trade space for time on the Yellow River line.
- Northern line collapses: execute a controlled retreat to the Yangtze and Wuhan hubs, evacuate factories (focus and decisions), preserve the field army [FAIR: no deliberate delete of trapped units].
- PRC joins a united front through the Xi'an incident [H]: read the faction template and `CHI_one_nation_undivided` branch text (it creates the Wang Front variant if PRC already holds the united front); finish Japan first, then use Crush the Communists after Victory over the Invaders.
- Peace offer from Japan before the mainland is clear: decline unless the lawful later route to capitulation is recorded.

## 11. Do-not list

- No Wang Jingwei branch (`CHI_prepare_for_a_change_in_leadership`, `CHI_look_for_allies` chain) if the plan needs Chiang for Generalissimo; no democratic-league branch.
- No `CHI_preemptive_strike` before the Japanese front is stable; no simultaneous war with PRC and several warlords while Japan attacks.
- No `CHI_end_military_primacy` (purges the military leadership); no Blue Shirt branch unless deliberately recording a fascist route.
- No console, events, save edits, reload, empty-port farming, or exploiting foreign-assistance loops.
- Do not spend focus time on the navy tree before the land line holds; do not leave the focus slot idle; do not recruit past rifles.
- Do not leave the three-year plan until after the war starts (requires peace).

## 12. End state and evidence

Define before day one (house defaults unless the user overrides): independent; in-game major status observable; 100+ civilian + military factories with 60+ military; 100+ deployed divisions averaging 90% equipment,
no theater critical-supply alert; 500+ mission-ready aircraft; supplied home-island operation; non-negative essential equipment trends for 30 days after the last offensive. Territory checklist: all unification
states in section 5 owned or integrated; Manchukuo/Jehol/East Hebei/Dalian/Taiwan recovered; Hong Kong, Macau, Guangzhouwan through the focus or war; PRC gone; Japan capitulated with settlement proof; no declared state occupied by an enemy; no active war with a remaining enemy major.
Record: baseline and final factory/division/aircraft counts, warlord status table (member, subject, integrated, annexed), Marco Polo date and choice, each recovered state, Japanese capitulation, and unknown metrics as unknown. Never lower targets late.
