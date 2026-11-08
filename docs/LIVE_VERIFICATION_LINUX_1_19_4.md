# Live verification: HOI4 1.19.4.1 on Linux

Date: 5 November 2026. Purpose: record which tools of the native server were run live against the Linux
HOI4 1.19.4.1 executable, what they did, and every difference from macOS found and fixed on the way. The
macOS record for the same game version is [LIVE_VERIFICATION_1_19_4.md](LIVE_VERIFICATION_1_19_4.md); the
tool list is [TOOLS_MACOS.md](TOOLS_MACOS.md).

## Setup

| Item | Value |
|---|---|
| Game | HOI4 1.19.4.1 Linux x86_64, GNU build ID `2265e6e7c1f527bf4b1f2c4b4ca3efd4371dc082`, SHA-256 `36a69ea446df3903baa0d1b2dec1331cfd2fe16eacee482d84cc36e5b6f9b5da` |
| Content | 10 DLCs as on macOS (NSB, BBA, AAT, NCNS, LaR, Gotterdammerung and the packs); AAT, LaR and BBA were active in play |
| Host | Ubuntu 24.04 under WSL2 on Windows 11, an isolated copy of the game used only for this work |
| Launch | `hoi4-live-native --serve` with the owned Linux launcher: private `HOME` and user folder per run, no Steam variables |
| Display | Xvfb 1920x1080x24 with its own Xauthority, `openbox` as the window manager (the game needs `_NET_ACTIVE_WINDOW`), the game window undecorated at the top left |
| Renderer | `llvm_pipe` (Mesa software). The d3d12 (WSLg) path crashed when the map loaded (`D3D12: Removing Device`). The settings seed turns off terrain, water, trees, rivers, sky and building models, which brought a key's round trip from about 1.5 s to 0.07 s |
| Binding | `hoi4-live-native --bind-management RUN_DIRECTORY BINDING` (the full management toolset) |
| Layout | the generated table `native/layouts/linux-2265e6e7c1f527bf4b1f2c4b4ca3efd4371dc082.json` with the live entries of `scripts/data/xlayout-live-1.19.4.1.json` |

Rules kept throughout: no console, no save editing and no
memory writes (the memory checks were read-only, through the owning process). Process ownership was
checked by start time, executable and user. One game instance ran at a time.

Each new game was a 1936 Germany start (Normal, historical AI). Runs r9 to r19 cover peace from 1 January
1936, a declared war on Lithuania from October 1936 to September 1938 and a fresh peaceful start for the
last checks.

## Results by tool

`Verified`: the tool ran with real input and its receipt was verified from memory (or, for reads, the values
matched the screen or the game's own data). `Refusal`: the tool stopped before input with the typed
refusal the game state called for; the success path needs a state the run did not reach. Labels in
brackets are the run where the tool was last checked.

### Verified (104)

Reads: `hoi4_capabilities`, `hoi4_overview`, `hoi4_research`, `hoi4_research_folders`, `hoi4_research_extras`,
`hoi4_technology_list`, `hoi4_economy`, `hoi4_economy_detail`, `hoi4_industry`, `hoi4_resources`,
`hoi4_production_lines`, `hoi4_production_shortages`, `hoi4_construction_queue`, `hoi4_focus_completed`,
`hoi4_country_status`, `hoi4_input_geometry`, `hoi4_states`, `hoi4_supply`, `hoi4_army`, `hoi4_armies`,
`hoi4_army_assign_plan`, `hoi4_commanders`, `hoi4_division_templates`, `hoi4_template_get`,
`hoi4_template_options`, `hoi4_deploy_locations`, `hoi4_designs`, `hoi4_design_options`,
`hoi4_naval_production_options`, `hoi4_fleets`, `hoi4_convoys`, `hoi4_air`, `hoi4_air_wings`,
`hoi4_air_regions`, `hoi4_doctrines`, `hoi4_mios`, `hoi4_intelligence`, `hoi4_special_projects`,
`hoi4_decisions_list`, `hoi4_politics_options`, `hoi4_diplomacy`, `hoi4_diplomacy_state`,
`hoi4_diplomacy_countries`, `hoi4_diplomacy_incoming`, `hoi4_war_risk`, `hoi4_wars`,
`hoi4_war_goal_targets`, `hoi4_front_plan`, `hoi4_encirclement_candidates`, `hoi4_trade`, `hoi4_events`,
`hoi4_peace_status`, `hoi4_peace_options` (closed conference), `hoi4_daily_governor` (digest).

Actions: `hoi4_input_lease`, `hoi4_time` (pause, unpause, speed; also read-only settlement of an
interrupted dispatch), `hoi4_save`, `hoi4_research_start`, `hoi4_focus_start`, `hoi4_decision_take`,
`hoi4_politics_choose` (trade law), `hoi4_event_choose` (ordinary and news layouts),
`hoi4_construction_add`, `hoi4_production_add`, `hoi4_production_set` (including the last of 21 lines,
reached by wheel scrolling), `hoi4_state_slots`, `hoi4_trade_set` (bought 8 rubber, then cancelled),
`hoi4_auto_import`, `hoi4_train`, `hoi4_army_create`, `hoi4_army_assign` (new army), `hoi4_army_split`,
`hoi4_army_merge`, `hoi4_army_commander`, `hoi4_army_group_create`, `hoi4_army_front`,
`hoi4_army_offensive`, `hoi4_army_plan` (executing), `hoi4_template_rename`, `hoi4_template_edit`
(priority), `hoi4_design_obsolete`, `hoi4_diplomacy_act` (justify war goal, declare war),
`hoi4_air_mission`, `hoi4_air_rebase`, `hoi4_air_wing_create` (from reserve), `hoi4_air_wing_split`,
`hoi4_air_wing_merge`, `hoi4_air_wing_disband`, `hoi4_air_region_probe`, `hoi4_fleet_region`,
`hoi4_fleet_mission`, `hoi4_fleet_admiral` (list and assign), `hoi4_fleet_create`, `hoi4_fleet_split`,
`hoi4_fleet_assign`, `hoi4_fleet_reserve`, `hoi4_fleet_balance`, `hoi4_fleet_delete`,
`hoi4_special_projects_panel`,
`hoi4_special_project_assign`, `hoi4_special_project_stop`, `hoi4_special_project_unassign`,
`hoi4_special_project_recruit`, `hoi4_agency_create`, `hoi4_plan_run`.

Several of these were never run on macOS 1.19.4.1 (`hoi4_fleet_balance`, `hoi4_fleet_delete`, `hoi4_agency_create`,
`hoi4_special_project_stop`, `_unassign` and `_recruit` on 1.19.4, the success path of
`hoi4_politics_choose`).

### Refusal only, success path not reached

| Tool | What happened | Same as macOS |
|---|---|---|
| `hoi4_army_move` | `no land route` from East Prussia (a correct game refusal) | success verified on macOS in war |
| `hoi4_army_assign` fold path | `selection_mismatch`: the box selection caught other divisions, nothing assigned | fold path never verified on macOS |
| `hoi4_template_create`, `hoi4_template_delete` | `insufficient_experience` (0 to 5.8 army experience) | macOS needed experience too |
| `hoi4_design_create` | not run: no army, air or navy experience in the runs | macOS tank and plane path stops at the experience check |
| `hoi4_design_set_production` | `already_building` (the only spare light tank design was already on its line) | 31 Oct on macOS |
| `hoi4_doctrine_adopt` | `already_adopted`; a new doctrine needs 100 experience | macOS: refusal only on 31 Oct |
| `hoi4_air_hold` | `nothing_to_change` (the wing stood by) | |
| `hoi4_fleet_merge` | the game kept task forces in different ports apart; `input_readback_failed`, no repeated input | the macOS record notes the same game behaviour |
| `hoi4_naval_invasion` | the drawing from the state centres found no invasion order; cancelled with Escape, nothing repeated | never on macOS (`UNVALIDATED`) |
| `hoi4_mio_trait`, `hoi4_mio_design_team` | the organisation list opens, but its rows are a `CSmoothListbox` the shared reader does not read, so `organization_not_offered` | never on macOS (`UNVALIDATED`); same code |
| `hoi4_mio_policy` | `policy_unknown` for a wrong key (catalogue check works) | never on macOS |

### Not reached

| Tool | Reason |
|---|---|
| `hoi4_peace_take`, `hoi4_peace_submit`, `hoi4_peace_exit` | No war ended in the runs: Lithuania held East Prussia, which had no land route for reinforcements. Verified on macOS. |
| `hoi4_agency_upgrade`, `hoi4_operative_recruit`, `hoi4_operative_assign` | Need a finished agency (30 game days after creation). The runs that reached it were cut off by host restarts; not repeated. |
| `hoi4_faction_create`, `hoi4_faction_leave`, `hoi4_faction_invite_accept` | Germany leads the Axis; never verified on macOS either. |
| `hoi4_wait_for`, `hoi4_plan` | not called with valid arguments (file and wait helpers; no game input) |

### Diagnostics that fail closed on Linux 1.19.4.1

The older Linux-only diagnostic readers (`hoi4_focus_list` with selected details, `hoi4_focus_search_state`,
`hoi4_focus_node_detail`, `hoi4_focus_selected_detail`, `hoi4_focus_input_probe`, `hoi4_research_ui`,
`hoi4_research_nodes`, `hoi4_research_node_detail`, `hoi4_research_slot_control`, `hoi4_research_category`,
`hoi4_construction_controls`, `hoi4_construction_find_view`, `hoi4_construction_target`,
`hoi4_construction_ui_probe`, `hoi4_production_controls`) and the staged construction primitives
(`hoi4_construction_open` ... `_find_open`, `hoi4_construction_panel_probe`) were written for Linux 1.19.3
and refuse on 1.19.4.1 with typed errors (`foreign_focus_scope`, `invalid_gui_owner`, `invalid_construction`,
`unsupported_platform`). 1.19.4 removed classes they depend on (for example `CNationalFocusDetailView`). The
player tools that replaced them on macOS (`hoi4_research_start`, `hoi4_focus_start`,
`hoi4_construction_add`, the production tools) are verified above. On macOS these diagnostics are listed as
unknown or macOS-qualification stages.

## Differences found and fixed

Layout (all recorded with their evidence in `scripts/data/xlayout-live-1.19.4.1.json`, 305 entries):

- View and popup roots: windows derived from `CCountryView` keep idler and root at +0x330/+0x338, those
  derived from `CPopUpWindow` at +0x328/+0x330 (macOS +0x488/+0x490).
- Fleet panel (`CNaviesView`): panel windows -0x2840, leader window +0x3368, task force array +0x5478.
- Navy leaders: name +0x40. Fleet sea region names now go through the layout table (they used raw macOS
  offsets in `naval.rs`).
- Trade offer window root +0x928, MIO organisation list +0x2698 with root +0xb0, scientist roster root
  +0x28, recruit window root +0x28, agency logo window +0x340 with root +0x50.
- Special projects: state machine gain, progress and iterations at +0x74 to +0x80; scientist skill levels;
  the breakthrough `std::map` is a libstdc++ red-black tree (root +0x68, children +0x10/+0x18, key +0x20,
  value +0x24). The value is now read as an i32 on both platforms (the next dword is not zero on Linux).
- Diplomacy action dialogs: the action info window is the action controller glue +0x610 behind view
  +0x2ce0. War goal type key +0x10. Dialog grid scroll position +0x344.
- Event windows: root +0x28 and event +0x40 (the static guess was +0x30/+0x48).
- `CEditBox`: the Linux build keeps no UTF-8 copy of the text. Edit box text is read from the editor's
  UTF-32 `std::wstring` at +0xb0 (new `Reader::wstring_bounded`).

Behaviour:

- Strategic air mode: the Linux game shows the small air base icon (`bg_btn`) in the default map mode too, so
  air mode is detected from the large `bg_strat_btn`.
- `hoi4_air_region_probe` clicks below the screen centre: the capital's victory point star at the centre took
  the click.
- The agency's creation progress bar is shown before creation (it displays the cost), so "creation running"
  is read from the cancel button. Create Agency opens `CAgencyLogoSelectionWindow`, now found through the
  agency view.
- New confirmations and notices: `CConfirmDeleteUnits` ("Delete task force") is accepted, and
  `CIntelAgencyFinishedPopUpWindow` ("Agency Created!") is dismissed like the other OK-only notices.
- Confirmation buttons are clicked only once their rectangle reads the same twice 300 ms apart: a dialog that
  had just appeared reported its button at the top left before the game laid it out.
- Captures: X refuses `GetImage` on a window that is partly off screen, and the window manager placed the
  game at (1, 1). The capture then reads the visible part from the root window.
- Launcher and platform: X11 connections fall back to the abstract socket when `/tmp/.X11-unix` is not
  usable (WSLg mounts it read-only), the launcher can seed the user folder's `dlc_signature`, the
  controller epoch accepts Linux start times in clock ticks, and the map data path is found from the Linux
  install layout.

## X11 input calibration

- Keys: the game samples modifiers per frame, so a modifier is held 150 ms before and after the key or click
  and a press lasts 60 ms (Shift-clicks were lost before).
- Wheel: one notch per 120 macOS scroll pixels. Verified by `hoi4_production_set` reaching the last of 21
  production lines and by the design list.
- Clicks: pointer motion, 60 ms, press, release. No calibration beyond the timing was needed at 1920x1080.

## Known limits of this setup

- Software rendering slows the game in war (about one game day every ten seconds at speed 4 with fights on
  screen). Some readbacks then timed out (`input_readback_failed`, or `action_uncertain` for the clock) and
  were settled read-only. A GPU renderer would not show this.
- A tool call interrupted by the client leaves its journal entry unresolved, as on macOS. The same tool with
  the same `request_id` settles it read-only.
