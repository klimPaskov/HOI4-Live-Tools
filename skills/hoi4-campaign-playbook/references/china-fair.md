# Nationalist China FAIR showcase (1936 start, No Compromise, No Surrender)

Objective: unify China including warlords and communists, expel and capitulate Japan, recover lost
territory and finish as a top-tier power. Normal difficulty, vanilla rules, historical AI, FAIR. Play the
Nationalist tag `CHI`, never Communist China or an unrelated tag. The standing plan for the opening is
[plans/china-fair-1936-1938.plan.json](../plans/china-fair-1936-1938.plan.json) with its
[explanation](../plans/china-fair-1936-1938.md).

One focus point = 7 days, so cost 5 = 35 days, 10 = 70 days.

## 0. Which content loads (read first)

Active DLCs: Anniversary Pack, Arms Against Tyranny, By Blood Alone, Gotterdammerung, Historical German Portraits,
La Resistance, **No Compromise, No Surrender** (one DLC, the comma is part of the name), No Step Back, Poland: United
and Ready, Rocket Launcher Unit Pack. Integrated into the base game (always active): Together for Victory, Death or
Dishonor, Waking the Tiger, Man the Guns.

* **CHI plays the NCNS tree** `china_nationalist_sea_focus`. The Waking the Tiger tree `china_nationalist_focus` and
  `generic_focus` are not used. If NCNS is ever disabled, the WtT tree loads instead and every focus key below is wrong.
* With NCNS the start uses OOB `CHI_1936_sea`, the NCNS modifiers and characters, and the warlord state transfers.
* Japan loads `japan_wtt_focus`.

## Contents
1. Checkpoint table
2. Start facts and national debuffs
3. Tree structure and the political choice
4. Focus plan (opening, war, unification)
5. Warlords: mechanics, faction goals and sequence
6. Communists
7. Japan: timing, defence, counter-offensive, capitulation, recovered territory
8. Laws, advisors, research, production, army sizing
9. Foreign assistance, air and navy
10. Contingencies
11. Do-not list
12. Measurable end state and run record

## 1. Checkpoint table

| Date | Must be true |
|---|---|
| 1936-01-01 | Start: 17 states, 11 civilian / 8 military / 2 naval factories, stability 0.20, war support 0.32, 85 divisions in `CHI_1936_sea`, neutrality 91% / communism 7% / fascism 2%. Warlords SHX, GXC, YUN, XSM, SIK, SIC, SND, HBC, GDC, XIC exist (MEN and MAN are Japanese clients); PRC holds 2 states. Focus `CHI_dang_guo` started on day one; 6 idle military factories put to work. |
| 1936-05-20 | Dang Guo, national currency and the Three-Year Plan done (the plan needs peace). Zhang Qun hired; partial mobilisation bought or PP saved for it. |
| 1936-09-02 | Denounce Feudalism and One Nation Undivided done: the Chinese United Front exists; warlords answered the invitation (event `SEA_china.7`; acceptance is their AI's choice). |
| 1936-12-31 | Fund the Plan Domestically (+2 civs) done; all factories busy; `infantry_equipment_1` line running; no PP above 250 unspent. |
| 1937-07-14 | Military Affairs Commission, Protracted Warfare, Resource Commission and Central Steel (+3 civs) done; river lines and garrisons ready (section 7). |
| War start (historically 1937-07-07) | `china.1` answered with option 0 (war). Gung Ho (+2 arms factories, needs war) and Demonstrate Our Resolve next in the slot; construction switched to arms factories. |
| 1938-01-01 | End of the standing plan: Defence in Depth near done; Soviet/Western support focuses queued; no collapse of the Yellow River line. |
| 1938-06-30 | Wuhan 1036 safe; Soviet or Western lend-lease flowing (Secure the Soviet Border / Send the First Lady Abroad). |
| 1939-01-01 | Benchmark (1939 bookmark, includes integrated warlords): CHI 31 civilian / 16 military, 34 states. Be at or above it. |
| 1940-12 | Warlords in the faction (at least the tour regions you intend); Japanese front pinned on rivers; 80+ divisions. |
| 1942-1945 | Faction goal `faction_goal_no_japanese_in_china` complete; Victory over the Invaders done; Crush the Communists war running or won; PRC gone. |
| Final | Japan capitulated (Tokyo area occupied, settlement proof), all declared unification states owned/integrated, no enemy on declared states, 100+ factories (60+ military), 100+ divisions at 90%, 500+ aircraft, supplied home-island operation (house targets). |

## 2. Start facts and national debuffs

States: Fujian 595, Zhejiang 596, Jiangsu 598, Jiangxi 600, Hunan 602, Guizhou 603, Anhui 606, Henan 607, Shanghai 613,
Hubei 620, Qingdao 743, Zunyi 748, Huangshan 749, Changde 750, Suzhou 1034, Nanjing 1035 (capital), Wuhan 1036. Beiping,
Hebei, Shandong, Sichuan (Chongqing, Chengdu), Shaanxi/Xi'an belong to warlords (HBC, SND, SIC, ...). Interior with free
slots: Hubei 620, Wuhan 1036, Jiangxi 600, Hunan 602, Changde 750, Guizhou 603, Zunyi 748.

Army: 48 "Sanjiao Jun" (6 infantry), 17 "Juntuán Reformed" (6 infantry + support artillery), 14 "Juntuán" (4 infantry,
garrison), 6 "Qibing Jun" (cavalry). One production line: `infantry_equipment_0` on 2 factories. NSB adds 20 L3/35 and
16 Panzer I to the stockpile.

NCNS start modifiers: Corruption in the Administration (political power +35%, consumer goods +25%, construction speed
-10%, free repair -25%, economy-law cost +65%), Illicit Trade (consumer goods -15%, factory output -15%), Inefficient
Economy (factory efficiency gain -35%, base fuel gain -50%), Neglected Countryside (communist drift +0.05, research
speed -20%), Corruption in the Armed Forces (conscription factor -78%, defence -15%, morale -25%, attack -5%,
organisation -5%, max dig-in -1), ROCAF (air XP -20%, accidents +25%), Nine Power Treaty (consumer goods, minimum
export, enemy war-goal justification; locks China on Free Trade: no Export Focus, Limited Exports or Closed Economy
while it holds), NSB spirit Whampoa Military Academy. Focuses change the underlying variables (e.g. currency, decree
the end to corruption). Starting techs: infantry_weapons, gw_artillery, interwar_antiair, basic_train, tech_support,
tech_trucks, gwtank_chassis, the BBA airframes and the MTG naval techs. 2 research slots. Laws: civilian economy,
volunteer only, Free Trade (locked by the Nine Power Treaty). The 1939 scenario has a very different state
(war_economy, service_by_requirement, closed_economy, focuses pre-completed); do not assume it.

Rival benchmark: JAP 31 states, 26 civilian / 16 military / 15 naval. Warlord economies are tiny (GXC 5/3, XSM 5/2,
SHX 3/2, SIK 4/1, YUN 3/1, SIC 2/3).

## 3. Tree structure and the political choice

Root focuses (no prerequisites): three mutually exclusive political roots - `CHI_end_military_primacy` (10; civilian-republican path, retires Dai Li, Soong Mei-ling, Chiang Ching-kuo,
Chen Cheng, Hu Zongnan, Wang Yaowu), `CHI_dang_guo` (5; party-state, stability +10%, communist and fascist popularity -5%) and `CHI_strengthening_our_resolve` (5; Blue Shirt/fascism path, leads to
`CHI_a_party_unified`, `CHI_expel_the_occupiers`, `CHI_put_an_end_to_PRC`). Also roots: `CHI_promote_the_national_currency` (economy), `CHI_military_affairs_commission_sea` (army), `CHI_rocaf` (air),
`CHI_republic_of_china_navy`.

Choose **Dang Guo -> Denounce Feudalism**: it keeps Chiang as ruler, which `CHI_a_generalissimo_for_all_of_china` requires (available: Chiang ruling, `PRC` must not exist) and avoids
`CHI_prepare_for_a_change_in_leadership` (exclusive; gives the leadership to Wang Jingwei and the Wang Front branch). Denounce Feudalism: +5% stability, every warlord gains opinion modifier
`CHI_denounced_feudalism`, unlocks Chen Guofu, registers warlord subjects. The Blue Shirt and civilian roots are legal FAIR alternatives but break the recorded plan (Chiang ruling, PRC removal by focus).

## 4. Focus plan

Opening (the standing plan's `focus_path`, dates if nothing is refused):

1. **CHI_dang_guo** (5; to 1936-02-05).
2. **CHI_promote_the_national_currency** (5; to 03-11): +25 PP, economy-law cost -10%, consumer goods -5%.
3. **CHI_the_three_year_plan** (10; needs `has_war = no`, bypassed at war; to 05-20): 1095-day spirit
   (consumer goods -5%, construction +10%, efficiency gain +10%) that ends at war.
4. **CHI_denounce_feudalism** (5; to 06-24).
5. **CHI_one_nation_undivided** (10; to 09-02): needs a non-subject warlord who is not at war with us; creates `faction_template_chinese_united_front` (or the Wang Front variant if Communist
   China already holds the united front) and sends event SEA_china.7 to each eligible warlord (not Sinkiang under `SIK_battle_for_sinkiang`, not HBC). Eligible warlords receive the invitation and +5% war support; acceptance is the AI's choice.
6. **CHI_fund_the_plan_domestically** (10; to 11-11): +2 civilian factories and slots instantly. The exclusive
   `CHI_find_foreign_financiers` needs a major with opinion > 19 and pays CIC only from majors above 69.
7. **CHI_decree_the_end_to_corruption** (5; to 12-16): consumer goods -5%, construction +10%, economy cost -10%.
8. **CHI_military_affairs_commission_sea** (5; to 1937-01-20): army XP +5%, morale +10%, planning +2%.
9. **CHI_protracted_warfare** (10; to 03-31): `defensive_postures` sub-doctrine if the infantry track has none
   (else mastery). Not exclusive with `CHI_decisive_battles`; the defensive branch simply comes first.
10. **CHI_monopolise_resource_extraction** (5; to 05-05) -> **CHI_central_steel_and_machine_works** (10; to
    07-14): +3 civilian factories and +30 steel.
11. War-gated, in the path so they start at the first free slot after the war begins:
    **CHI_initiate_the_gung_ho_movement** (10; +2 arms factories inland, arms speed +10%, war support +10%),
    **CHI_demonstrate_our_resolve** (5; needs war with Japan; unlocks the state-targeted
    `CHI_demonstrate_our_resolve_decision`, 25 PP), then **CHI_defence_in_depth** (10; defence +5%, dig-in +1,
    unlocks `CHI_breach_the_yellow_river`), **CHI_send_the_first_lady_abroad** and **CHI_secure_the_soviet_border**
    (10 each, war only), **CHI_arrest_the_seven_gentlemen** (10) -> **CHI_draw_support_from_the_big_families**
    (5; unlocks H. H. Kung and T. V. Soong).

Fillers when nothing above can start: relocate_factories_from_the_coast (5), the_4_defence_areas (10),
guerrilla_warfare (5), introduce_the_baojia_system (10; exclusive with expand_the_tongzi_jun) ->
inspect_the_troops (5; needs a faction) -> from_many_to_one (10), formalize_field_promotions (10; exclusive with
set_up_an_nco_school) -> crackdown_on_corruption_in_recruitment (10), reinforce_connections_with_shanghai_elites
(5), institute_a_national_registry_of_property (10).

After 1938: **CHI_towards_a_professional_army** (10) or **CHI_towards_a_popular_army** (10; needs 1,000,000
army manpower), lend-lease branches (section 9), the remaining defence and army reform focuses.

Unification phase: **CHI_the_sky_cannot_have_two_suns** (5) -> **CHI_tour_the_north / tour_the_west / tour_the_south** (5 each; each needs an allied warlord of that region: north = NXM, SHX, GSM, HBC, XIC or SND; west = XSM, KHM, SIK or SIC;
south = YUN, GXC or GDC) -> **CHI_victory_over_the_invaders** (5; needs faction goal `faction_goal_no_japanese_in_china` complete and no war with Japan) -> **CHI_crush_the_communists** (10; `annex_everything` war goal on PRC, never expires) ->
**CHI_a_generalissimo_for_all_of_china** (10) -> **CHI_subjugate_korea** (10, exclusive with Joining the International Community) -> **CHI_reclaim_the_north** (10). Also **CHI_request_handover_of_coastal_cities** (5; states 326 Hong Kong,
729 Macau, 728 Guangzhouwan) and **CHI_petition_territorial_restoration** (5). Alternative conventional route: **CHI_preemptive_strike** (10; creates annex war goals on every non-allied warlord; do not take while Japan
is the main front).

**CHI_extend_compulsory_education** (5) is the second focus, not a filler: the third research slot needs the countryside
variable at -0.05 (start -0.20) and the focus's mission adds +0.01 every 180 days. More slots later:
**CHI_expand_national_sun_yat_sen_university** (more than 74 factories plus the steel, tungsten and fertiliser focuses) and
**CHI_unify_the_admissions_process**.

## 5. Warlords: mechanics, faction goals and sequence

Mechanism: warlords join the player's faction; each member has a faction influence ratio. Decision `CHI_press_warlord_to_become_puppet`
needs own influence ratio more than twice the target's, costs 125 PP and 15 points of faction influence (refunded by events if refused), then event SEA_china_warlords.1 asks the warlord; success sets
`autonomy_sea_warlord_subject` (freedom 0.8). After a tour focus the cheaper `..._upgrade` decision costs 75 PP and 10 points. Subjects then drift via `CHI_warlord_autonomy_drift_decision`
(autonomy), lend-lease and economic/industrial support decisions (180 days each); `CHI_integrating_states` (100 PP, 50 days, after `CHI_empower_the_mtac`) adds cores.
Tour focuses reduce the integration cost for that region's warlords. These decisions are targeted: take them with `hoi4_decision_take` and its
`target` (the warlord's name), or a plan `decision_rules` entry with `target`. United Front influence is a hazard: spending it can hand the
faction lead to a warlord, so keep own influence well above every member's before pressing.

Sequence: faction first (1936), keep relations high (improve-relations, Denounce Feudalism modifier), take the tour focuses as soon as an ally of that region exists even while fighting Japan, press the
strongest (Guangxi, Shanxi) first because their land borders Japan or the front. A friendly independent warlord is not unification; integrate or annex. Warlord states: SHX Shanxi 615, Suiyuan 621, Ordos 746;
XSM Gansu 283, Qinghai 604, Ningxia 616 and others; SIK Urumqi 617 and 6 more; YUN Yunnan 325, Dali 747; GXC Guangxi 599, Guangdong 593, Guangzhou 592, Nanning 594, Hainan 591, Nanlu 1033; SIC Chongqing 1037,
Chengdu 1041 and others; MEN Chahar 611, Xilingol 612, Pailingmiao 1043.

**Faction goals define "subjugated"**: `faction_goal_north_subjugated` is complete when NXM, SHX, GSM, HBC, XIC ... have no core state left
outside China (all their core states owned by CHI); `faction_goal_west_subjugated` does the same for XSM, KHM, SIK, SIC; `faction_goal_south_subjugated` for the southern group. Visible only to CHI with the
`chinese_united_front` template and the matching tour focus. Medium-term goals `faction_goal_control_the_chinese_capitals`, `faction_goal_control_the_chinese_coast` and `faction_goal_subjugate_the_faction`
(unlocked by `CHI_from_many_to_one`) exist; the long-term `faction_goal_no_japanese_in_china` (no Japanese-controlled state in mainland China) completes with +100 power projection, +20 faction influence to big members and
rewards 100 PP / 100 army XP / 100k manpower to qualifying members. Use these as the measurable unification checklist, together with ownership checks per state.

On the `CHI_dang_guo` route pick the first option of the Liangguang incident: it forms the Chinese United Front and invites
the warlords early; the second option kills Chiang after `CHI_strengthening_our_resolve`.

## 6. Communists

`PRC` starts with 2 weak states and a hostile opinion modifier. The Xi'an incident is started by a warlord's
decision `CHI_instigate_xian_incident` and reaches CHI as `wtt_xian_incident.1` (go to Xi'an) and
`wtt_xian_incident.4` (ally with the communists). The standing plan answers option 0 to both: Chiang goes and
agrees, PRC is invited into the united front (`wtt_xian_incident.5`). Refusing to go expels the instigating
warlord from the faction; refusing the alliance leads to `wtt_xian_incident.6` (Chiang arrested). Both are worse
while Japan is the threat.

Later: watch `CHI_communist_infiltration_in_*` decisions (Gansu, Shandong, Jiangsu, Henan, Beijing, Hebei,
Shanxi, Suiyuan, Xi'an, Ordos); after Victory over the Invaders take **Crush the Communists** (available
while PRC exists and is not at war with us; its war goal never expires); the Blue Shirt path's
`CHI_put_an_end_to_PRC` is not on this route. Do not fight PRC and Japan at the same time.

Counter-infiltration costs 50 PP each: `CHI_develop_counterinsurgency_tactics` halves it (needs
`CHI_empower_the_bureau_of_investigation_and_statistics`), `CHI_arrest_the_seven_gentlemen` divides it by 1.5 and
`CHI_co_opt_the_koh_hsin_movement` cuts it by a quarter; finish the Bureau-Counterinsurgency pair before the 1939 waves.

## 7. Japan

**Timing**: `china.1` (Marco Polo Bridge) is sent by Japan's `JAP_reinforce_the_beijing_garrison`
path or by the timeout of the mission `JAP_kwantung_army_impatience` (725 days), which only Japanese focuses
activate (`JAP_revere_the_emperor_destroy_the_traitors`, `JAP_sea_purge_the_kodoha_faction`); there is no fixed
latest date. Option 0 "Breaking point!" is war (Japan declares `annex_everything` on CHI, SHX, SND, HBC); option 1
hands over Beiping 608, Shandong 597, Jinan 1038, Hebei 614 and Qingdao 743 (whoever controls them) for a 180-day
truce. The historical-focus rule removes the concession option for the AI; the plan chooses war.

**Defence**: in this DLC set the north (Beiping 608, Hebei 614, Shandong) is warlord land at the start, so CHI's own
front is Henan 607, Anhui 606, Jiangsu 598 and the coast. Hold the Yellow River and the Beiping-Hankow railway hubs with 2
armies of 12-16 divisions each, fortify river towns, keep one reserve army (8-10 divisions) at Wuhan 1036. Coast: dedicated
garrisons (2-3 divisions) in Shanghai 613, Nanjing 1035, Zhejiang 596, Fujian 595, Qingdao 743, Jiangsu 598; Japanese landings
must be counter-attacked before a supplied bridgehead. Centre: Wuhan 1036 is the hub; Chongqing 1037 and Chengdu 1041 (SIC,
a warlord) are the fallback only once Sichuan is in the faction or integrated. Never empty a port repeatedly (FAIR). Use
`CHI_declare_war_zone`, `CHI_bog_them_down`, `CHI_holding_state_mission`, industrial evacuation decisions and
`CHI_breach_the_yellow_river` only after reading their tooltips (the last has heavy side effects).

**Counter-offensive**: after equipment and supply allow (1940-1942), concentrate one army on a short axis, close pockets, retake northern hubs, then move on Jehol 610, Manchukuo (328, 714-717, 761), East Hebei 609, Dalian 745, Chahar 611 and
Taiwan 524 (all China cores or Japanese/puppet holdings). Expelling Japan from the mainland completes the faction goal; it is an intermediate milestone, not Japanese capitulation.

**Capitulation**: refuse a peace that ends the route unless a lawful later route is recorded. Home-island assault needs transports, escorts, air range, a secured port and the invasion state machine in
[strategy](strategy.md); Kyushu or Shikoku first, then Kansai/Kanto. Invasion capacity by tech (divisions / plans / prep days): base 4/1/60, transport 6/2/50, landing craft 10/5/40, tank landing
craft 15/9/30; launch needs 50%+ supremacy in every crossed region and 40% naval intel. Japan capitulates only after the capital region and victory points are held; confirm with a fresh read.

**Jinan**: if Japan takes it, the Han Fuju event lets China annex Shandong with its divisions; do not spend the main army
holding Jinan: hold the Yellow River line and retake the city once the Japanese attackers are worn down.

**Fast Japan**: declares in late February 1937, lands at Shanghai-Suzhou and Hangzhou-Ningbo 5-10 days after the northern
attack draws China north and aims to capitulate China by April: keep a coastal reserve for Shanghai, Hangzhou and Nanjing
whatever happens in Hebei.

## 8. Laws, advisors, research, production, army

- Laws: base cost 150 PP, times (1 + economy cost factor): ~248 PP at start (+65%), ~233 after the currency focus.
  `partial_economic_mobilisation` (war support > 0.25, CHI has 0.32; consumer goods expected value 0.25 instead of
  0.35, arms speed +10%) is the first purchase. `war_economy` needs war support above 0.5 and war with an enemy whose
  industry is over 40% of China's (Japan qualifies); not Total Mobilisation. Trade law stays Free Trade while the Nine
  Power Treaty holds. Conscription: raise only when rifles exist (the armed-forces corruption cuts the pool 78%).
- Advisors: `CHI_zhang_qun` (political power +15%, no condition) first; `CHI_t_v_soong` (financial expert, consumer
  goods -15%) and `CHI_h_h_kung` (captain of industry) need `CHI_draw_support_from_the_big_families`; `CHI_chen_guofu`
  (consumer goods -10%, fascism drift +0.06) after Denounce Feudalism. Army chiefs Huang Qixiang and Teng Chie are
  hidden after Dang Guo; Falkenhausen (offensive 2, 50 PP) stays. Gu Zhutong, Fu Zuoyi and Zhang Xueliang are not
  CHI characters in this setup.
- Research: 2 slots, -20% speed. infantry_weapons1, basic_machine_tools, construction1, support_weapons,
  tech_engineers, concentrated_industry, construction2, interwar_antitank, interwar_artillery, then forts, radio and the
  second tiers. The third slot (`CHI_expand_academica_sinica`) needs the Neglected Countryside research variable at -0.05
  or better; the dang guo path reaches only -0.12 with its focuses, so it is a post-1938 target via the countryside missions.
- Production: 8 military factories (2 on rifles at start): artillery 3, support 1, rifles 2 + 2 (the
  `infantry_equipment_1` line from mid-1936), add anti-tank and AA after the first Japanese armour; navy only after the land
  defence holds. Civilian factories: interior civilian factories first, arms factories from the war start, evacuate coastal
  industry by hand (state-targeted decision after `CHI_relocate_factories_from_the_coast`).
- Army: equip the 85 starting divisions before training more; 60 equipped divisions by 1937-12, 90 by 1940, 100+ by 1944;
  generals lead 24 or fewer divisions; army groups under field marshals; separate tank and motorised formations (FAIR).
  Keep garrison templates for resistance areas. Template names carry an accent ("Juntuán"); check that
  `hoi4_division_templates` reads them before automating training.
- Disband the weakest 20-35 starting divisions for their rifles before training new ones.

## 9. Foreign assistance, air and navy

Lend-lease and support are normal FAIR play. Focuses: `CHI_secure_the_soviet_border` (non-aggression, `CHI_soviet_support`, Soviet lend-lease), `CHI_send_the_first_lady_abroad` (`CHI_american_support`),
`CHI_the_burma_road`, `CHI_the_hanoi_route`, `CHI_the_hump_sea`, `CHI_collaborate_with_allied_forces` -> `CHI_burma_campaign` / `CHI_indo_chinese_campaign`, `CHI_scout_for_air_experts` ->
`CHI_international_squadron` or `CHI_international_instructors`, `CHI_sell_lend_lease` vs `CHI_allocate_lend_lease` (exclusive). Delivery needs working ports and routes: record actual deliveries, not accepted requests.
`CHI_utilize_german_contacts` and `CHI_the_german_mission` are on other political branches and conditional on German opinion/government.

Air: ROCAF has a -20% air XP start modifier; take `CHI_rocaf` -> `CHI_controlling_our_skies` (defence, interception) before `CHI_projecting_power`; 200 fighters before any CAS. With By Blood Alone
active, aircraft are airframe designs (CHI has the interwar small and medium airframes). Navy: transports and escorts only, no battleship programme before 1943; the `CHI_republic_of_china_navy` branch is for late home-island support (long range, strike from below).

## 10. Contingencies

- No warlord accepts the faction invitation by 1936-12: raise opinion with the improve-relations diplomatic action, keep Denounce Feudalism's opinion bonus alive, and re-check every month; acceptance is the warlord AI's choice (event SEA_china.7). Do not switch to preemptive strikes while Japan is the main threat.
- Japan attacks before 1937-06: the plan's war-gated focuses start automatically; bank PP; protect Shanghai/Nanjing with garrisons and trade space for time on the Yellow River line; move construction to arms factories.
- Northern line collapses: execute a controlled retreat to the Yangtze and Wuhan hubs, evacuate factories (focus and decisions), preserve the field army (FAIR: no deliberate delete of trapped units).
- PRC not in the united front (Xi'an did not fire or was refused): finish Japan first, then use Crush the Communists after Victory over the Invaders.
- Peace offer from Japan before the mainland is clear: decline unless the lawful later route to capitulation is recorded.

## 11. Do-not list

- No Wang Jingwei branch (`CHI_prepare_for_a_change_in_leadership`, `CHI_look_for_allies` chain) if the plan needs Chiang for Generalissimo; no democratic-league branch.
- No `CHI_preemptive_strike` before the Japanese front is stable; no simultaneous war with PRC and several warlords while Japan attacks.
- No `CHI_end_military_primacy` (purges the military leadership); no Blue Shirt branch unless deliberately recording a fascist route.
- No console, events, save edits, reload, empty-port farming, or exploiting foreign-assistance loops.
- Do not spend focus time on the navy tree before the land line holds; do not leave the focus slot idle; do not recruit past rifles.
- Do not leave the Three-Year Plan until after the war starts (it requires peace and is bypassed at war).
- Do not adopt an infantry sub-doctrine by hand before `CHI_protracted_warfare` (it would turn its sub-doctrine reward into mastery).

## 12. End state and run record

Define before day one (house defaults unless the user overrides): independent; in-game major status observable; 100+ civilian + military factories with 60+ military; 100+ deployed divisions averaging 90% equipment,
no theater critical-supply alert; 500+ mission-ready aircraft; supplied home-island operation; non-negative essential equipment trends for 30 days after the last offensive. Territory checklist: all unification
states in section 5 owned or integrated; Manchukuo/Jehol/East Hebei/Dalian/Taiwan recovered; Hong Kong, Macau, Guangzhouwan through the focus or war; PRC gone; Japan capitulated with settlement proof; no declared state occupied by an enemy; no active war with a remaining enemy major.
Record: baseline and final factory/division/aircraft counts, warlord status table (member, subject, integrated, annexed), Marco Polo date and choice, each recovered state, Japanese capitulation, and unknown metrics as unknown. Never lower targets late.
