# Live verification inventory: HOI4 1.19.4.1 with 10 DLCs (macOS)

Date: 1 November 2026, updated 9 November 2026 with the live rounds of 5-8 November (section "Live results, 5-8
November"; rows of table 1a name the newest evidence). Purpose: list every macOS tool, what live evidence it has on
HOI4 1.19.4.1 (f584), and what must be run before the campaigns. Written from documents, descriptors and git history
only; the game was not touched for this inventory. `[?]` marks a result recorded only in the session notes of a test
round, with no commit message or document that states it.

Active content now: No Step Back (NSB), By Blood Alone (BBA), Arms Against Tyranny (AAT), No Compromise No Surrender
(NCNS), La Resistance (LaR), Gotterdammerung, Anniversary Pack, Poland United and Ready, Rocket Launcher, Historical
German Portraits, plus Waking the Tiger, Man the Guns, Together for Victory and Death or Dishonor (integrated). Before 1
November only Anniversary Pack, Poland United and Ready and Rocket Launcher were active (commit 8a6feee), so tank and
plane module designers, MIOs, the intelligence agency and BBA air were mostly never driven.

## Sources and evidence labels

| Label | Meaning | Source |
|---|---|---|
| **1 Nov, 10 DLC** | Run live on 1.19.4.1 with the 10 DLCs | Caller report for this inventory (1 Nov 2026) |
| **1 Nov, gate** | Used live in the first 1.19.4 two-year Germany gate (3 DLCs: "The first 1.19.4 gate ran without By Blood Alone", `plans/germany-fair-1936-1938.md` "1.19.4 gate notes") | Commits 1aad8fb, 9bf0049, 862a375, 90b3ce3, c2a8475 |
| **31 Oct, 3 DLC** | Named in the live table of `docs/HOI4_1_19_4_UPDATE.md` ("Verified with real actions (journal receipt + memory readback)") | `HOI4_1_19_4_UPDATE.md` |
| **31 Oct, 3 DLC, blanket** | A read covered only by "All read tools pass" in `HOI4_1_19_4_UPDATE.md`; no per-tool record, and at peace only | `HOI4_1_19_4_UPDATE.md` |
| **1.19.3 only (date)** | Live evidence exists only on the old build (Mach-O 089EA519); not re-run on 1.19.4 | `PLAYBOOK_TOOL_GAPS.md` sections 6c, 11-14, commits of 18-26 Oct |
| **never** | No live run recorded; descriptor or doc says so (`UNVALIDATED (no live test yet)`, `validated:false`) | descriptors, `LIVE_VALIDATION_QUEUE.md` |
| **unknown** | Nothing documented either way for 1.19.4 | |
| **5-8 Nov** (commit) | Live result stated in a commit message of 5-8 November (test beds Italy 1936/1937 with 10 DLCs, rounds w8 and w12-w17, the Ethiopia training wars and the speed-3 work) | the commit named in the row |
| **round notes [?]** | Stated only in the round's session notes (rounds w12-w14, 6 Nov), not in a commit or a document | session notes |

Not mapped to a single tool: the caller also reports the "status digest" as verified on 1 Nov with 10 DLCs.

Priority: **P1** needed for the Germany, China or Poland campaign; **P2** useful; **P3** rare, debug or replaced.
`A` = action (lease and request_id), `R` = read.

## 1. Every macOS tool

The macOS server lists 151 tools: 123 from `native/src/launch/macos*.rs` (the 122 in `docs/TOOLS_MACOS.md` plus
`hoi4_save`) and 28 common ones from `native/src/protocol.rs` and the construction and production preview modules
(`hoi4_capabilities`, `hoi4_overview`, `hoi4_economy`, research, focus and construction diagnostics, ...). Table 1a
holds the 126 player and plumbing tools, table 1b the 25 diagnostic, staged-primitive and debug tools.

### 1a. Player tools

#### Time, session, plan, events

| Tool | A/R | Last live verification | Game state needed to verify | P |
|---|---|---|---|---|
| `hoi4_capabilities` | R | 1 Nov, 10 DLC | any | P1 |
| `hoi4_overview` | R | 31 Oct, 3 DLC, blanket | any | P1 |
| `hoi4_input_lease` | A | not listed by name; every action of 31 Oct and 1 Nov takes a lease | any | P1 |
| `hoi4_time` | A | 1-2 Nov, 10 DLC ("verified", live results table) | any | P1 |
| `hoi4_wait_for` | R | 31 Oct, 3 DLC, blanket | running clock | P2 |
| `hoi4_events` | R | 31 Oct, 3 DLC, blanket | an open event | P1 |
| `hoi4_event_choose` | A | 1-2 Nov, 10 DLC; 8 Nov: 15 events in 250-590 ms, including three stacked windows (0aea416) | an open event with 2+ options (NSB Poland election `NSB_poland_royal_election.1`) | P1 |
| `hoi4_plan` | R | unknown (plans rewritten 1 Nov, 56e478e) | none (file only) | P1 |
| `hoi4_plan_run` | A | 1 Nov, gate. The new steps of 024e140 (special projects, doctrine adoption, war air missions, dated lines) and db9073b (BBA fighters in war) have no live record | running game with a plan; war for the war steps | P1 |
| `hoi4_save` | A | 1 Nov, 1.19.4 ("Verified live: GER_1937_03_19_16.hoi4 written", c2a8475); 6 Nov round w12: Escape until the game menu opens (3681b1a) | any | P1 |

#### Economy, construction, production

| Tool | A/R | Last live verification | Game state needed to verify | P |
|---|---|---|---|---|
| `hoi4_economy` | R | 31 Oct, 3 DLC, blanket | any | P1 |
| `hoi4_industry` | R | 31 Oct, 3 DLC, blanket | any | P1 |
| `hoi4_resources` | R | 31 Oct, 3 DLC, blanket (`validated:false`) | any | P1 |
| `hoi4_production_shortages` | R | 31 Oct, 3 DLC, blanket (`validated:false`) | a resource deficit | P2 |
| `hoi4_economy_detail` | R | 31 Oct, 3 DLC, blanket | any | P2 |
| `hoi4_construction_queue` | R | 31 Oct, 3 DLC, blanket ("Selection, province and railway live validation pending") | queued construction | P1 |
| `hoi4_construction_add` | A | 1-2 Nov, 10 DLC (state buildings); 8 Nov: infrastructure in Eritrea in 1.2 s (eeda2ae). Province buildings (`province_id`: supply_node, naval_base, bunker, coastal_bunker): unit-tested, live test pending (a7039cb) [?] | own state with a free slot, idle civilian factories | P1 |
| `hoi4_state_slots` | A | 31 Oct, 3 DLC | any | P1 |
| `hoi4_production_lines` | R | 1 Nov, 10 DLC | any | P1 |
| `hoi4_production_add` | A | 1 Nov, 10 DLC for plane and tank lines with `design` (Shift+click skips the AAT MIO chooser, a0e8a2d). Base-type add without `design`, land equipment and naval lines: never with AAT | idle military factories; dockyards for naval | P1 |
| `hoi4_production_set` | A | unknown on 1.19.4 (not in the 31 Oct table; 4 calls in the 25 Oct gate on 1.19.3) | an existing line | P1 |
| `hoi4_design_set_production` | A | 31 Oct, 3 DLC. With AAT a click on a design opens the MIO chooser (a0e8a2d); 6 Nov: accepts the line index (c4fe48c) | line plus a second design of its family | P1 |
| `hoi4_naval_production_options` | R | 31 Oct, 3 DLC, blanket | any | P2 |
| `hoi4_trade` | R-style A | 31 Oct, 3 DLC (with trade_set and auto_import) | any | P2 |
| `hoi4_trade_set` | A | 31 Oct, 3 DLC; 6 Nov round w12: a country tag resolves to the game's display name (3681b1a) | Automatic Importing off, a short resource | P2 |
| `hoi4_auto_import` | A | 31 Oct, 3 DLC (on/off, budget 5 to 8 to 3) | any | P1 |

#### Research, doctrines, special projects, MIOs, intelligence

| Tool | A/R | Last live verification | Game state needed to verify | P |
|---|---|---|---|---|
| `hoi4_research` | R | 31 Oct, 3 DLC, blanket | any | P1 |
| `hoi4_technology_list` | R | 31 Oct, 3 DLC, blanket | any | P2 |
| `hoi4_research_extras` | R | 31 Oct, 3 DLC, blanket | any | P2 |
| `hoi4_research_start` | A | 1-2 Nov, 10 DLC: armor_tech_1 (NSB) and armor_piercing_bombs (BBA) with Shift+click (suggested MIO); 5 Nov: `replace:true` live-tested (7c0b6fc) | empty research slot; one NSB armour and one BBA air tech | P1 |
| `hoi4_focus_list` | R | 31 Oct, 3 DLC, blanket (Mac: catalog only) | any | P2 |
| `hoi4_focus_completed` | R | 31 Oct, 3 DLC, blanket (descriptor: "other Focus trees and full campaigns unverified") | a completed focus | P1 |
| `hoi4_focus_start` | A | 1-2 Nov, 10 DLC ("verified"). Never on the NSB Poland and NCNS China trees | empty focus slot, PP; Poland and China starts | P1 |
| `hoi4_doctrines` | R | 1 Nov, 10 DLC ("track mastery fields are validated:false until a track holds a subdoctrine") | any | P1 |
| `hoi4_doctrine_adopt` | A | 1-2 Nov, 10 DLC: armored_spearhead adopted (land). Air and naval adoption never [?] | army/navy/air experience at or above the cost | P1 |
| `hoi4_special_projects` | R | 31 Oct, 3 DLC, blanket | a facility | P2 |
| `hoi4_special_projects_panel` | A | 1 Nov, 10 DLC | a facility | P2 |
| `hoi4_special_project_assign` | A | 31 Oct, 3 DLC | facility, idle scientist, breakthrough point | P2 |
| `hoi4_special_project_recruit` | A | 1.19.3 only (26 Oct) | PP 25-100 | P2 |
| `hoi4_special_project_stop` | A | 1.19.3 only (26 Oct) | a running project | P3 |
| `hoi4_special_project_unassign` | A | 1.19.3 only (26 Oct) | crewed facility without a project | P3 |
| `hoi4_mios` | R | 1 Nov, 10 DLC (catalogue); 6 Nov: organisation list and details work, rows found by opening Details (b722b01); per-organisation funds and size still `state_read:false` | any | P1 |
| `hoi4_mio_design_team` | A | never (`UNVALIDATED`) | AAT, an organisation slot in the politics view, PP | P2 |
| `hoi4_mio_policy` | A | never (`UNVALIDATED`); 6 Nov round notes: the policy rows are a smooth listbox the tool cannot reach, and blind clicks are refused because of the 180-day lock [?] | AAT, organisation, 25 PP (playbook: "Policies cost 25 PP") | P2 |
| `hoi4_mio_trait` | A | 6 Nov, 10 DLC: traits queue, verified by reopening the detail window (b722b01); descriptor still `UNVALIDATED` | AAT, an organisation with a trait point | P1 |
| `hoi4_intelligence` | R | 1 Nov, 10 DLC (agency existence, levels and operatives `state_read:false`) | any | P2 |

#### Politics, decisions, diplomacy, war planning

| Tool | A/R | Last live verification | Game state needed to verify | P |
|---|---|---|---|---|
| `hoi4_country_status` | R | 31 Oct, 3 DLC, blanket (descriptor: "Unvalidated until the live checks of docs/COUNTRY_STATUS_WARS_DESIGN.md are recorded") | any, compare with top bar | P1 |
| `hoi4_politics_options` | A | 31 Oct, 3 DLC. Category `research_production` under AAT (MIO slots): never | any | P1 |
| `hoi4_politics_choose` | A | 1-2 Nov, 10 DLC (success path, "verified") | PP 150 for a law or advisor | P1 |
| `hoi4_decisions_list` | A | 31 Oct, 3 DLC | any | P1 |
| `hoi4_decision_take` | A | 1-2 Nov, 10 DLC (untargeted). Targeted path open | enabled untargeted decision, PP | P1 |
| `hoi4_diplomacy` | A | 31 Oct, 3 DLC | any foreign country | P1 |
| `hoi4_diplomacy_act` | A | 1-2 Nov, 10 DLC: justify war goal and declare war (Germany 1939). Other keys 1.19.3 only (26 Oct: cancel, improve relations, NAP, access, guarantee); 6 Nov: one retry of the war-goal type click (c00ee7e) | PP; completed war goal for declare_war | P1 |
| `hoi4_diplomacy_countries` | R | 31 Oct, 3 DLC, blanket | any | P1 |
| `hoi4_diplomacy_state` | R | 31 Oct, 3 DLC, blanket; 6 Nov: reads with the player at war and names factions (8377624, the player's first live war failed the old read) | a war for the war fields | P1 |
| `hoi4_diplomacy_incoming` | R/A | never (`UNVALIDATED`) | an AI proposal pop-up | P1 |
| `hoi4_faction_invite_accept` | A | never (`UNVALIDATED`) | a faction invitation pop-up | P2 |
| `hoi4_faction_create` | A | 6 Nov, round w12: opened from the political view's `new_faction_button` (404412a); the game's national-spirit refusal reads `faction_creation_not_allowed`. That a faction was founded is in the round notes only [?]; descriptor still `UNVALIDATED` | player without a faction who may create one | P3 |
| `hoi4_faction_leave` | A | never (`UNVALIDATED`) | player in a faction (test on a throwaway save) | P3 |
| `hoi4_wars` | R | 31 Oct, 3 DLC, blanket at peace; war output 1.19.3 only (26 Oct) | a war | P1 |
| `hoi4_war_risk` | R | never (added 1 Nov, 90b3ce3) | target with guarantors or a faction | P1 |
| `hoi4_war_goal_targets` | R | 31 Oct, 3 DLC, blanket | named enemy | P1 |
| `hoi4_front_plan` | R | 31 Oct, 3 DLC, blanket ("No live validation against a running game yet" for the plan output, `PLAYBOOK_TOOL_GAPS.md` 10) | border with an enemy, own divisions | P1 |
| `hoi4_encirclement_candidates` | R | as front_plan | war with a land enemy | P1 |
| `hoi4_states` | R | 31 Oct, 3 DLC, blanket; 8 Nov: VP provinces with controllers and the surrender estimate (2d0d660), province controllers read live (4f8d4b6) | any | P1 |

#### Army, templates, designs, supply

| Tool | A/R | Last live verification | Game state needed to verify | P |
|---|---|---|---|---|
| `hoi4_armies` | R | 31 Oct, 3 DLC, blanket | any | P1 |
| `hoi4_army` | R | 31 Oct, 3 DLC, blanket | any | P2 |
| `hoi4_commanders` | R | 31 Oct, 3 DLC, blanket (NSB leader traits not checked) | any | P1 |
| `hoi4_supply` | R | 31 Oct, 3 DLC, blanket ("Supply fields are validated:false") | divisions in an overdrawn province | P1 |
| `hoi4_train` | A | 1-2 Nov, 10 DLC | template, manpower, equipment | P1 |
| `hoi4_deploy_locations` | A | unknown on 1.19.4 (label offset moved to +0x160 in a56dadc; train works without it) | a training line without location | P2 |
| `hoi4_army_assign_plan` | R | 31 Oct, 3 DLC, blanket | unassigned divisions | P1 |
| `hoi4_army_assign` | A | 31 Oct, 3 DLC: new-army path. Fold path: 1-2 Nov ("partial folds in large states"); 6 Nov round w12: unit-counter selection, provinces that also hold the receiving army (3681b1a, fc9c58a) | unassigned divisions next to an army | P1 |
| `hoi4_army_create` | A | 1-2 Nov, 10 DLC; 5 Nov: folds the state's remaining unassigned divisions (5bc2580); slow in large states, 49-82 s (round notes) [?] | unassigned divisions | P1 |
| `hoi4_army_merge` | A | never (added 1 Nov, 1aad8fb) | two armies, total 24 or fewer divisions | P1 |
| `hoi4_army_split` | A | 1.19.3 only (18 Oct, 2546aae) | army of 2+ divisions | P1 |
| `hoi4_army_group_create` | A | 1-2 Nov, 10 DLC ("all verified in war") | 2+ armies, a field marshal or promotable general | P1 |
| `hoi4_army_commander` | A | 1-2 Nov, 10 DLC: general to army; field marshal promoted and assigned to a group (Rundstedt) | army plus free general | P1 |
| `hoi4_army_front` | A | 1-2 Nov, 10 DLC (war); 5 Nov: highlighted front found around Bale (5bc2580); 8 Nov: aimed without the Find view when the camera is low (5f4e248) | army on a border with a foothold | P1 |
| `hoi4_army_offensive` | A | 1-2 Nov, 10 DLC (war); 8 Nov: drawn from the army to the target, Jubaland to Sidamo and to Addis Abeba on the first stroke (6981beb), 1.9 s (5f4e248) | war, army holding a front line | P1 |
| `hoi4_army_plan` | A | 1-2 Nov, 10 DLC (war); 8 Nov: 0.5 s (5f4e248) | army with battle plan orders | P1 |
| `hoi4_army_move` | A | 1-2 Nov, 10 DLC (war); 5 Nov: a move by sea counts as started (d006b39, seen live); 6 Nov round w12: overseas moves, readback `sea_transfer_unverified` until the troops land (e1eaed7); 8 Nov: `province_id` (4f8d4b6), 0.6 s (0aea416) | any army | P1 |
| `hoi4_division_templates` | R | 31 Oct, 3 DLC, blanket | any | P1 |
| `hoi4_template_get` | R | 31 Oct, 3 DLC, blanket | any | P1 |
| `hoi4_template_options` | A | 1 Nov, 10 DLC | any | P1 |
| `hoi4_template_create` | A | 31 Oct, 3 DLC | army XP (5 per battalion, 10 per company, +25 for a new group) | P1 |
| `hoi4_template_edit` | A | 31 Oct, 3 DLC | army XP | P1 |
| `hoi4_template_rename` | A | 31 Oct, 3 DLC; 6 Nov round w13: locked templates refused as `template_locked` (565fcc9) | any | P2 |
| `hoi4_template_delete` | A | unknown (no live record on any build) | an unused template | P2 |
| `hoi4_designs` | R | 1 Nov, 10 DLC | any | P1 |
| `hoi4_design_options` | A | 1 Nov, 10 DLC (tank, plane, ship; tank/plane module slots) | any | P1 |
| `hoi4_design_create` | A | Ship: 31 Oct, 3 DLC. Tank and plane: 1-2 Nov, 10 DLC, designs saved (Panzer III S 5194, Bf 109 E N 5195); 6 Nov: accepts a design name (c4fe48c) | army XP (tank), air XP (plane), navy XP (ship) | P1 |
| `hoi4_design_obsolete` | A | 1.19.3 only (26 Oct); flag width fixed 1 Nov (a0e8a2d), no live record | a non-obsolete own design | P2 |

#### Added after 2 November

| Tool | A/R | Last live verification | Game state needed to verify | P |
|---|---|---|---|---|
| `hoi4_alerts` | R | 5 Nov, Italy 1937: 17 alert frames named, each checked against the game's tooltip (1ad2100); 6 Nov: frames 29 `insufficient_resources` and 68 `insufficient_energy` (c4fe48c) | any alert in the top bar | P1 |
| `hoi4_division_disband` | A | 5 Nov, Italy 1937: one division, then a whole army; manpower returned to the pool (6cb027e) | an army with a division to drop | P1 |
| `hoi4_map_mode` | A | 5 Nov: F-key table mapped live (political, naval, air, supply, terrain, resistance, compliance, resources, air raids, factions); no readback (7ac629c) | any | P2 |
| `hoi4_occupation` | R | 5 Nov: default occupation law validated by `hoi4_occupation_law` (f10cd06); resistance and compliance per state from the game's own triggers; the extras (target before additions, neighbour share, resisting country) unconfirmed (41c10b8) [?] | an occupied or resisting state | P1 |
| `hoi4_occupation_law` | A | 5 Nov, Italy 1937: Military Governor -> Martial Law -> Military Governor, verified from memory (f10cd06) | an occupied or resisting state | P1 |
| `hoi4_war_stats` | R | added 5 Nov (41c10b8); no live comparison with the war screen recorded [?] | a war with casualties | P1 |
| `hoi4_army_motorization` | A | 5 Nov, Ethiopia training war: a starving army's supply rose from 29% to 41% in nine game days (5bc2580) | an army at war | P1 |
| `hoi4_army_detach` | A | 5 Nov: live-tested (7c0b6fc) | an army of 2+ divisions | P1 |
| `hoi4_railway_build` | A | 6 Nov: added (e1eaed7); Eritrea to Wello queued in 26 s after the zoom fix (dc0a55d); 8 Nov: Eritrea to Tigray in 4.7 s, 1.9 s with both ends on screen (3611139) | a hub or port and a target province | P1 |
| `hoi4_construction_order` | A | 6 Nov, round w17: top, down, up and remove verified by the line's new queue position (ab02318) | two or more construction lines | P1 |
| `hoi4_army_attack` | A | 8 Nov: offensive + execute in 2.3 s (5f4e248; the first version never ran because of its request ids) | army on a front with a target | P1 |
| `hoi4_autopilot` | A | 8 Nov: added (e25a492, "live validation pending"); used in the speed-3 soaks merged as 463c475 with no per-tool record [?] | a running game with a plan | P1 |

#### Air

| Tool | A/R | Last live verification | Game state needed to verify | P |
|---|---|---|---|---|
| `hoi4_air` | R | 31 Oct, 3 DLC, blanket (legacy planes; BBA airframe wings never) | BBA wings | P1 |
| `hoi4_air_regions` | R | 31 Oct, 3 DLC, blanket | any | P1 |
| `hoi4_air_wings` | A | unknown on 1.19.4 | any | P2 |
| `hoi4_air_mission` | A | 1-2 Nov, 10 DLC: CAS and air superiority over Poland; 8 Nov: region readback, CAS over Tigray 4.1 s, air superiority 3.9 s (9db0800); a wing already on another region takes the second point tried (3259d3b) | wing in range of a region | P1 |
| `hoi4_air_rebase` | A | 31 Oct, 3 DLC | air base with free capacity | P1 |
| `hoi4_air_wing_create` | A | 31 Oct, 3 DLC (legacy reserve). BBA reserve planes: never | reserve aircraft at a base | P1 |
| `hoi4_air_wing_split` | A | 31 Oct, 3 DLC | wing of 2+ planes | P2 |
| `hoi4_air_wing_disband` | A | 31 Oct, 3 DLC | any wing | P2 |
| `hoi4_air_wing_merge` | A | 1.19.3 only (26 Oct) | two wings of one type at one base | P2 |
| `hoi4_air_hold` | A | 1.19.3 only (26 Oct) | wing on a mission | P2 |
| `hoi4_air_region_probe` | A | 1.19.3 only (26 Oct) | any region | P2 |

#### Navy

| Tool | A/R | Last live verification | Game state needed to verify | P |
|---|---|---|---|---|
| `hoi4_fleets` | R | 31 Oct, 3 DLC, blanket | any fleet | P1 |
| `hoi4_convoys` | R | 31 Oct, 3 DLC, blanket | any | P1 |
| `hoi4_fleet_mission` | A | 31 Oct, 3 DLC; 6 Nov round w12: fleet panel titles cut at 31 characters, sea regions from the map files (3681b1a); fleet matched by its task forces (4f4d18c) | task force | P1 |
| `hoi4_fleet_region` | A | 31 Oct, 3 DLC | fleet and a coastal state | P1 |
| `hoi4_fleet_admiral` | A | 31 Oct, 3 DLC | fleet and a free admiral | P2 |
| `hoi4_fleet_create` | A | 31 Oct, 3 DLC | task force | P2 |
| `hoi4_fleet_split` | A | 31 Oct, 3 DLC (descriptor still says `UNVALIDATED`); 6 Nov round w12: delete-task-force and progress-loss dialogs accepted (c4fe48c) | task force of 4+ ships | P2 |
| `hoi4_fleet_merge` | A | 31 Oct, 3 DLC (descriptor still `UNVALIDATED`; task forces in different ports are split off by the game) | fleet with 2+ task forces in one port | P2 |
| `hoi4_fleet_assign` | A | 31 Oct, 3 DLC (descriptor still `UNVALIDATED`) | two fleets | P2 |
| `hoi4_fleet_reserve` | A | 31 Oct, 3 DLC (descriptor still `UNVALIDATED`) | task force | P3 |
| `hoi4_fleet_balance` | A | never (button meaning "inferred from its name") | fleet with 2+ task forces | P3 |
| `hoi4_fleet_delete` | A | never (destroys ships) | throwaway task force | P3 |
| `hoi4_naval_invasion` | A | 6 Nov, rounds w13/w14: "executed and landed" in the round notes only, no commit records it [?]; `plan_not_executable` explains invasion preparation and naval dominance (c00ee7e); descriptor still `UNVALIDATED` | war with a coastal enemy, army in a port state, convoys above 0 | P1 |

#### Espionage (La Resistance)

| Tool | A/R | Last live verification | Game state needed to verify | P |
|---|---|---|---|---|
| `hoi4_agency_create` | A | 6 Nov, round w12: creation verified live (946dca4); descriptor still `UNVALIDATED` | LaR, no agency, 5 civilian factories for 30 days | P2 |
| `hoi4_agency_upgrade` | A | 1-2 Nov, 10 DLC: upgrade_army_department started | agency created, no upgrade running | P2 |
| `hoi4_operative_recruit` | A | 6 Nov, round w13: fixed live (565fcc9): candidates from the leaders grid, the recruiting bar read; descriptor still `UNVALIDATED` | agency, free operative slot | P3 |
| `hoi4_operative_assign` | A | 6 Nov, round w13: fixed live (565fcc9): switches an operative already on a mission; `operation` and `boost_ideology` refuse by design; descriptor still `UNVALIDATED` | recruited operative, target country | P3 |

#### Peace conference

| Tool | A/R | Last live verification | Game state needed to verify | P |
|---|---|---|---|---|
| `hoi4_peace_status` | R | 1-2 Nov, 10 DLC with a conference open (Luxembourg; conference state list) | open conference | P1 |
| `hoi4_peace_options` | R | never with a conference open (`validated:false`) | open conference | P1 |
| `hoi4_peace_take` | A | 1-2 Nov, 10 DLC: Luxembourg taken; 6 Nov rounds w13/w14: Albania end to end (round notes only) [?] | open conference, own turn | P1 |
| `hoi4_peace_submit` | A | 1-2 Nov, 10 DLC: Luxembourg; 6 Nov rounds w13/w14: Albania (round notes only) [?] | open conference with a pending demand | P1 |
| `hoi4_peace_exit` | A | 1-2 Nov, 10 DLC: Luxembourg (Return accepts the exit popup); 6 Nov rounds w13/w14: Albania (round notes only) [?] | conference after the last round | P1 |

### 1b. Diagnostic, staged-primitive and debug tools (all P3)

None of these is listed for 1.19.4 by name; the blanket read statement may cover the read-only ones.

| Tool | A/R | Status on 1.19.4 | Note |
|---|---|---|---|
| `hoi4_daily_governor` | A | unknown | "Long campaigns remain unverified"; `hoi4_plan_run` does the campaign work |
| `hoi4_navy_ui` | A | unknown | "DEBUG ONLY (never listed to a player)" |
| `hoi4_input_geometry` | R | unknown | engineering |
| `hoi4_research_slot_control`, `hoi4_research_category`, `hoi4_research_ui`, `hoi4_research_node_detail` | R | unknown | support `research_start` |
| `hoi4_research_nodes` | R | refuses on macOS (`capability_unavailable`, `HOI4_1_19_4_UPDATE.md`) | Linux only |
| `hoi4_research_folders` | R | refuses on macOS (`unsupported_platform`) | Linux only |
| `hoi4_focus_search_state`, `hoi4_focus_node_detail`, `hoi4_focus_selected_detail`, `hoi4_focus_input_probe` | R | unknown | support `focus_start` |
| `hoi4_construction_controls`, `hoi4_construction_find_view`, `hoi4_construction_target`, `hoi4_construction_ui_probe`, `hoi4_construction_panel_probe` | R | unknown | probes |
| `hoi4_production_controls` | R | unknown | "Linux read-only preview" |
| `hoi4_construction_open`, `_select`, `_find_state`, `_place`, `_clear_selection`, `_find_open` | A | unknown | "Default-off Mac qualification" stages of `construction_add` |

### 1c. Counts

| | Tools | P1 | P2 | P3 |
|---|---|---|---|---|
| Verified on 1 Nov with 10 DLCs (full or partial) | 11 | 9 | 2 | 0 |
| Verified on 1.19.4 with 3 DLCs only (31 Oct table or 1 Nov gate) | 40 | 28 | 11 | 1 |
| Reads covered only by the 31 Oct blanket statement | 35 | 26 | 9 | 0 |
| Live evidence on 1.19.3 only | 14 | 7 | 5 | 2 |
| Never verified | 21 | 10 | 5 | 6 |
| Unknown (player tools) | 5 | 2 | 3 | 0 |
| **Player tools, total** | **126** | **82** | **35** | **9** |
| Diagnostic, staged and debug tools (table 1b) | 25 | 0 | 0 | 25 |

`hoi4_production_add` and `hoi4_design_create` are counted under 10 DLC although only part of their paths ran.
`hoi4_input_lease` and `hoi4_politics_choose` (refusal path only) are counted with the 3-DLC group.

These counts are those of 1 November and are not recomputed. Since then (rows above): the 1-2 November table moved
`hoi4_time`, `hoi4_doctrine_adopt`, `hoi4_agency_upgrade`, the peace take/submit/exit tools, `hoi4_army_group_create`,
`hoi4_army_offensive`, `hoi4_army_plan`, `hoi4_army_move` and others to "1-2 Nov, 10 DLC"; the 5-8 November rounds
added 12 tools (table "Added after 2 November") and moved `hoi4_mio_trait`, `hoi4_agency_create` and the operative
tools out of "never"; `hoi4_naval_invasion` and `hoi4_faction_create` rest on round notes only [?].

## Live results, 1-2 November 2026 (10 DLCs, test games 1939 Germany, 1936 Italy, 1936 Germany)

Verified with real actions (receipt plus memory or window readback); each item names the fix it needed.

| Tool | Result | Fix needed |
|---|---|---|
| hoi4_design_create (tank, plane) | saved designs 5194 (Panzer III S) and 5195 (Bf 109 E N) | tank/plane module path enabled; name box located by marker; plane name box forward-delete |
| hoi4_designs | correct decommission flags | flag is one byte at +0x414 |
| hoi4_production_add (designs, base keys) | lines for named plane/tank designs; artillery by base key | Shift+click skips the AAT MIO chooser; base key resolves to the newest own variant |
| hoi4_research_start (NSB, BBA techs) | armor_tech_1, armor_piercing_bombs started | Shift+click researches with the suggested MIO |
| hoi4_doctrine_adopt | armored_spearhead adopted | row button located from its icon |
| hoi4_agency_upgrade | upgrade_army_department started | idle progress texts; upgrades window under the agency view |
| hoi4_army_commander (army group) | Gerd von Rundstedt promoted and assigned | Promote placed from the row bottom; command power read |
| hoi4_army_create / hoi4_army_assign | armies created, partial folds in large states | default map mode first; exact or army-free partial boxes only |
| hoi4_army_front / offensive / plan / move / group_create | all verified in war | none |
| hoi4_air_mission | CAS and air superiority over Poland | taller overview list |
| hoi4_diplomacy / diplomacy_act | justify war goal, declare war | E opens the International Market: its diplo_view_button opens diplomacy; real refusal texts |
| hoi4_peace_take / submit / exit | Luxembourg taken, submitted, exited | conference state list; text-sprite buttons; Return accepts the exit popup |
| hoi4_construction_add, hoi4_politics_choose, hoi4_decision_take, hoi4_train, hoi4_focus_start, hoi4_event_choose, hoi4_time | verified | none |
| hoi4_plan_run (Germany plan) | 1 Jan - 1 Jul 1936 unattended: research, focus, politics, special project, folds, events, production, construction, stop date | politics entries carry cost |
| All 37 read tools | run without error on the 1939 game | dynamic tags (civil war) read as ?index |

Still open: MIO list rows (CSmoothListbox item storage), folds outside the boxes around a state's centre,
naval invasion, operative recruit/assign, targeted decisions, province buildings, automatic diplomatic responses.

## Live results, 5-8 November 2026 (10 DLCs; Italy 1936/1937 test beds, rounds w8 and w12-w17, Ethiopia training wars)

Each row names the commit whose message states the result. Rows marked [?] rest on the round's session notes only.

| Tool | Result | Evidence |
|---|---|---|
| hoi4_alerts | 17 frames named from the game's tooltips (no focus, free dockyards, missing equipment production, modify government, wings without mission, nothing training, resistance, unassigned divisions, decisions, garrison, doctrines, officer corps, MIO, admiral ...); frames 29 and 68 added | 1ad2100, c4fe48c |
| hoi4_division_disband | single division, then a whole army, each verified gone; manpower returned | 6cb027e |
| hoi4_map_mode | ten modes by F key; no readback | 7ac629c |
| hoi4_occupation_law, hoi4_occupation | default law changed and changed back, read back from memory | f10cd06, 41c10b8 |
| hoi4_railway_build | Eritrea to Wello, Eritrea to Tigray queued | e1eaed7, dc0a55d, 3611139 |
| hoi4_construction_order | top, down, up, remove | ab02318 |
| hoi4_army_move overseas (sea transfer) | a move by sea counts as started; the readback says `sea_transfer_unverified` until the troops land; "sea transfer works" in round w12 [?] | d006b39, e1eaed7 |
| hoi4_mios, hoi4_mio_trait | organisation list, details and trait queue | b722b01 |
| hoi4_operative_recruit, hoi4_operative_assign | recruitment and assignment fixed live | 565fcc9 |
| hoi4_agency_create | agency created | 946dca4 |
| hoi4_faction_create | opened from the political view; refusal text mapped; a faction founded [?] | 404412a, 8377624 |
| hoi4_naval_invasion | executed and landed [?] | round w13/w14 notes; c00ee7e (refusal text) |
| hoi4_peace_take, _submit, _exit | Albania end to end [?] (Luxembourg already on 1-2 Nov) | round w13/w14 notes |
| hoi4_army_motorization | supply 29% to 41% in nine game days | 5bc2580 |
| hoi4_army_detach, hoi4_research_start `replace` | live-tested | 7c0b6fc |
| hoi4_air_mission | honest region readback; CAS 4.1 s, air superiority 3.9 s; second point for a wing already on another region | 9db0800, 3259d3b |
| hoi4_army_attack, hoi4_army_offensive, hoi4_army_move, hoi4_event_choose | speed work: 2.3 s, 1.9 s, 0.6 s, 250-590 ms | 5f4e248, 6981beb, 0aea416 |

Still open after 8 November: `hoi4_mio_policy` (no live test), faction invite and leave and incoming-proposal answers
(need a game state with an invitation or a proposal), province buildings through `hoi4_construction_add province_id`
(live test pending, a7039cb), targeted decisions, a stockpile read, a production-line remove, the regimental-support
row in the template tools, a garrison-template action.

## 2. Missing capabilities a strong player needs

No tool covers these. Sources are the playbook references (`skills/hoi4-campaign-playbook/references/`), descriptor
limits and `HOI4_1_19_4_UPDATE.md`.

| Gap | Evidence | Needed by | P |
|---|---|---|---|
| Targeted decisions (target state or country) | `poland-meta.md`: "the monarchy decisions are targeted, target LIT; the runner's `hoi4_decision_take` has no target argument"; `china-fair.md`: warlord integration "decisions are targeted", relocating industry is "state-targeted". The descriptor has no target argument | China, Poland | **P1** |
| Province buildings: land and coastal bunkers, naval bases, supply hubs, railways | `hoi4_construction_add`: "(state buildings only)"; Poland plan: "bunkers on all German-facing states"; Germany needs hubs and rail for Poland and the USSR, naval bases for Britain | all three | **P1** |
| Choosing or changing the MIO of a production line | a0e8a2d: with AAT a click "opens the Military Industrial Organization chooser; Shift+click adds the line directly" (so lines get no MIO); `politics-diplomacy-espionage.md`: "assign the MIO with the best production-efficiency traits to the biggest line" and MIO funds come from assigned lines | all three | **P1** |
| MIO state read (size, funds, trait points, policy, design team) | `hoi4_mios`: per-organisation state "NOT mapped ... reported unavailable"; needed before `hoi4_mio_trait` can be used sensibly | all three | **P1** |
| Equipment stockpile read (stock, daily output and use per type) | `templates-equipment-production.md` 9(c) item 6 `hoi4_stockpile`; rule "rebalance when stock ... exceeds 60 days ... or falls below 15 days (needs a stockpile read)"; no tool reads it | all three | **P1** |
| War goal justification progress and expiry | `hoi4_diplomacy_state`: "justification timers ... null"; `generic-playbook.md`: "A war goal completes and then expires (about two months)". Only the Cancel Justification dialog shows progress | Germany | **P1** |
| Defensive battle-plan orders: fallback line, area defence, deleting orders | `war-air-naval-peace.md` 2: "Area defense ... No tool yet"; Poland "pivot defence behind the Vistula, San and Narew"; China "river lines" | Poland, China | **P1** |
| Automatic diplomatic responses (1.19.4) | `HOI4_1_19_4_UPDATE.md`: "would remove request pop-ups during long campaigns"; `generic-playbook.md` 8: pop-ups covering panels make "army actions silently fail" | all three | **P1** |
| Peace conference demand kinds other than take, puppet, liberate; blind-bid rounds | `hoi4_peace_take` offers only those three; `PLAYBOOK_TOOL_GAPS.md` 14: "The blind-bid rules ... are not modelled"; window has force government, take navy, resource rights, reparations, dismantle industry | Germany (Poland, France), China | P2 |
| Spearhead order and execute mode (careful, balanced, rush) | `war-air-naval-peace.md` 2: "Spearhead ... no tool yet", "the UI mode switch has no tool" | Germany | P2 |
| Construction queue priority and 1.19.4 repair priority | `PLAYBOOK_TOOL_GAPS.md` 3.11.1: "Priority of that line is not controllable by any tool yet"; 1.19.4 repair priority "not yet driven" | Poland (bunkers first), Germany under bombing | P2 |
| Import priority window (1.19.4: order, accepted shortage, oil for fuel) | `hoi4_auto_import`: "Import priority order and accepted shortages are left at the game's settings" | Germany (oil, rubber) | P2 |
| Lend-lease, volunteers, licence production, expeditionary forces | `diplomacy_act` cancels dialogs that need choices (`dialog_unsupported`); `china-fair.md` expects "Soviet or Western lend-lease flowing"; volunteers give experience | China, Germany | P2 |
| Garrison template (occupation laws and the resistance read exist since 5 Nov: `hoi4_occupation_law`, `hoi4_occupation`) | `countryoccupationview.gui` `garrison_template_button`; no action | Italy 1936 (colonies resist from day 1), Germany, China | P1 |
| Faction goals, rules, initiative and influence | `politics-diplomacy-espionage.md` 7; `china-fair.md`: warlord integration needs "own influence ratio more than twice the target's", faction goals define "subjugated" | China | P2 |
| Recruiting new generals and assigning earned commander traits | `hoi4_commanders` lists "not-yet-recruited" leaders but no tool recruits; `army_commander` only assigns and promotes | China (85 divisions), all | P2 |
| Division template change and upgrade of existing divisions, reinforcement priority, training cancel | `templates-equipment-production.md` 9(c) items 3 and 7 | all three | P2 |
| Special project reward choices | `PLAYBOOK_TOOL_GAPS.md` 3.11: "Reward choices ... are not driven"; the game picks the default after a timeout | all three | P2 |
| Theatre creation and theatre commanders | `PLAYBOOK_TOOL_GAPS.md` 3.7: "Theatres: none" | Germany vs USSR | P2 |
| Intelligence network values, operations (steal tech, infiltrate), boost ideology mission | `hoi4_intelligence` `state_read:false`; `hoi4_operative_assign` refuses `operation_not_built`, `mission_not_built` | Germany | P3 |
| Per-wing air aggressiveness, day/night, reinforcement preference | `LIVE_VALIDATION_QUEUE.md` "Not built": no memory field found | all | P3 |
| Paradrop and air landings | `poland-meta.md`: "Airborne VP/port seizure ... candidate"; `PLAYBOOK_TOOL_GAPS.md` 6c: paradrop "not driven" | Poland (optional) | P3 |
| Subject management (autonomy, release, annex subject) | `politics-diplomacy-espionage.md` (c): "only through `hoi4_diplomacy` actions ... not specifically supported" | Poland (Lithuania union) | P3 |
| 1.19.4 QoL: agency upgrade queue, recall volunteers, research keyboard shortcuts, saved fleet compositions, G / Shift+G merges | `HOI4_1_19_4_UPDATE.md` "Not yet driven" | speed only | P3 |

## 3. Suggested verification order

Each scenario uses an owned, paused game and one tool at a time with a screenshot next to every result
(`LIVE_VALIDATION_QUEUE.md` common checks: refusals before input, one dispatch per request_id, windows closed after,
a retry returns the stored receipt). Save before any destructive step and reload after it.

### A. 1936 Germany start, 10 DLCs, at peace (re-run the 3-DLC evidence)

1. All reads once, compared with the screen where a number is shown: `capabilities`, `overview`, `economy`,
   `industry`, `resources`, `country_status` (top bar), `research`, `research_extras`, `focus_completed`, `armies`,
   `commanders` (NSB traits), `template_get`, `division_templates`, `supply`, `air` and `air_regions` (BBA wings),
   `fleets`, `convoys`, `naval_production_options`, `diplomacy_state`, `diplomacy_countries`, `states`, `wars`,
   `special_projects`, `economy_detail`.
2. Economy: `construction_add` (and a second batch), `state_slots`, `production_add` without `design` for
   `infantry_equipment` and one naval hull (check whether the MIO chooser opens), `production_set`,
   `design_set_production` (check the MIO chooser), `auto_import`, `trade`, `trade_set` (auto import off).
3. Research and politics: `research_start` on an NSB armour tech and a BBA air tech, `focus_start`,
   `politics_options` for `government` and `research_production` (AAT layout), `politics_choose` success (a law or
   advisor), `decisions_list`, `decision_take`, `event_choose` on a multi-option event.
4. MIOs (AAT): `mio_design_team` on a tech being researched, `mio_policy` (25 PP), `mio_trait {list:true}`, then a
   trait when a point exists.
5. Army: `train`, `deploy_locations`, `army_create`, `army_assign` fold path, `army_merge`, `army_split`,
   `army_group_create` with a field marshal, `army_commander`, `army_move`, `army_front` toward a neighbour,
   `template_rename`, `template_delete` (a spare template).
6. Air with BBA airframes: `air_wings`, `air_wing_create` from reserve (disband a wing first if the reserve is
   empty), `air_mission`, `air_rebase`, `air_wing_split`, `air_wing_merge`, `air_hold`, `air_region_probe`,
   `air_wing_disband`.
7. Navy: `fleet_mission`, `fleet_region`, `fleet_admiral`, `fleet_create`, `fleet_split`, `fleet_merge`,
   `fleet_assign`, `fleet_reserve`, `fleet_balance` (hover first), `fleet_delete` on a throwaway task force.
8. Special projects: `special_project_recruit`, `special_project_unassign`, `special_project_stop`.
9. `hoi4_save`, `hoi4_time`, `hoi4_plan_run` for 60 s with the 10-DLC Germany plan (special projects and doctrine
   steps of 024e140).

### B. La Resistance agency test (same game, PP and 5 civilian factories)

`agency_create`; run about 30 game days; `intelligence` (compare the screen); `agency_upgrade`
(`upgrade_training_centers`), a second call must answer `upgrade_in_progress`; `operative_recruit`; once recruited,
`operative_assign` with `build_intel_network` on Poland.

### C. Experience-dependent (load a save with banked XP; check `hoi4_doctrines` experience first)

`design_create` Save for a tank (army XP) and a plane (air XP) through the module designers, `production_add` with
the new design, `design_obsolete` on an old one; `template_create` and `template_edit` (army XP); `doctrine_adopt`
for a grand doctrine and a subdoctrine (this also validates `doctrines` track mastery). The 1 Nov gate save
`GER_1937_03_19_16` or a 1939 start may hold enough experience.

### D. 1939 Germany start: war with Poland and the peace conference

`war_risk` on Poland, `war_goal_targets`, `diplomacy_act declare_war` (with and without `accept_widening`),
`front_plan`, `encirclement_candidates`, `army_front`, `army_offensive`, `army_plan` execute and stop,
`air_mission` with BBA fighters and CAS over the front, `supply` during the advance, `wars` and `diplomacy_state` war
fields, `plan_run` war steps. At capitulation: `peace_status`, `peace_options`, `peace_take` (states and one
puppet), `peace_submit`, `peace_exit`. If the USSR is also a victor, this tests the multi-round case. A cheaper
single-round rehearsal is the documented Luxembourg route (justify 250 days, 3 divisions, `PLAYBOOK_TOOL_GAPS.md` 13a).

### E. Naval invasion test (Germany at war with a coastal enemy)

Army in a port state (Konigsberg or Stettin), convoys above 0, Poland's coast or Denmark as the target:
`naval_invasion` with an army, then with `divisions`; the refusals `own_state`, `enemy_required`, `not_coastal`,
`no_transports`; `fleet_mission` invasion support and convoy escort.

### F. Incoming proposals and factions

Run a minor nation (Poland 1936 or China) until an AI pop-up appears: `diplomacy_incoming` read, then refuse and
accept; `faction_invite_accept` on an invitation. On a throwaway Germany save: `faction_leave`, then
`faction_create`.

### G. Country-specific starts

Poland 1936 (NSB tree): `focus_start` on the Polish tree, `event_choose` on the royal election, `decision_take`
(untargeted only until targeted decisions exist). China 1936 (NCNS tree): `focus_start`, faction from
`CHI_one_nation_undivided`, `army_merge` and `army_group_create` with 85 divisions, `decision_take`.

### H. Build, then verify, the P1 gaps

Targeted decisions, province buildings (bunkers, hubs, rail, naval base), MIO per production line and the MIO state
read, stockpile read, war goal timer read, fallback line and area defence orders, automatic diplomatic responses.
Each needs its own live pass in A, D or G before the campaigns start.

## Documentation mismatches found

- `fleetops.rs` still prefixes `hoi4_fleet_split`, `_merge`, `_assign` and `_reserve` with `UNVALIDATED (no live
  test yet)`, though `HOI4_1_19_4_UPDATE.md` lists them as verified on 31 Oct. The same holds after 5-8 November for
  `hoi4_mio_trait`, `hoi4_agency_create`, `hoi4_operative_recruit`, `hoi4_operative_assign` and `hoi4_faction_create`
  (live per the rows above) and `hoi4_naval_invasion` (round notes only). These are descriptor texts in code; this
  inventory records the mismatch and does not change code.
- `docs/TOOLS_MACOS.md` was regenerated on 9 November (it gained `hoi4_construction_order`, `hoi4_army_attack` and
  `hoi4_railway_build`). The generator still marks lease-taking tools whose `lease_token` comes late in the schema as
  `R` (it looks only at the first 1500 characters after the description), misses `hoi4_occupation_law` (its
  description is built with `format!` without the `{}` prefix the generator expects) and lists none of the common
  tools of `protocol.rs` (`hoi4_autopilot`, `hoi4_capabilities`, ...). Fixing that is a change to
  `scripts/gen_tool_index.py`, outside this inventory.
- `LIVE_VALIDATION_QUEUE.md` expects `mio_system_absent` "in a game without Arms Against Tyranny" and the agency
  refusal for an inactive LaR; both DLCs are now active, so the real flows can be tested.
- `skills/hoi4-campaign-playbook/references/*.md` still list as missing several tools that now exist (doctrine
  purchase, MIO actions, designers, template editor, naval tools, peace tools, air rebase, faction creation,
  occupation law, railway build, construction order, disband). Their "(c) Tools" sections are out of date, and several
  titles still say "HOI4 1.19.3".
- `docs/MANAGEMENT_MATRIX.md` dated from 21 October; refreshed on 9 November from the coverage audit.
