# HOI4 1.19.4 update

Status of HOI4 Live Tools against **Hearts of Iron IV 1.19.4.1 "Operation Postern"**, measured
on 31 October 2026. Only what was run against the game is called verified.

## Versions

| | Old | New (installed, Steam stable) |
|---|---|---|
| Version | 1.19.3 (checksum c01a) | 1.19.4.1.99a5 (checksum **f584**) |
| Steam build | 25205862 | 26282552 |
| macOS Mach-O SHA-256 | `09419b08…ac15b1fb` | `d440ad8823e1ca5106b2ff87bc5135acf1b40d56f6bf0a76b52b5b247a668e07` |
| macOS LC_UUID | `089EA519-6780-353B-8E39-4E9309B444D4` | `DFA875A5-25BA-35E0-9736-627F8A5C4396` |

1.19.4 ("QoL patch", 27 October 2026, checksum a783) was followed by a hotfix (f584). The macOS
runtime accepts only the f584 binary above; any other build is refused with `unsupported_build`.
Linux (VM still on 1.19.3) and the legacy Python/Windows runtime were **not** run against 1.19.4.

## What changed in the game (released)

Relevant to the tools, from the 1.19.4 patch notes and the installed game files:

- **Automatic resource trading** (Trade view): a checkbox, a civilian-factory budget and an import
  priority window (order, accepted shortage per resource, oil when fuel is low). While it is on,
  manual trades are blocked. Shift-click on an insufficient resource enables it.
- **Automatic diplomatic responses**: on the own diplomacy screen, per request type accept /
  decline / off, plus accept-all and decline-all. Right-click on the top-bar flag opens it.
- **Repair priority queue** in the Construction window (with manual override), intelligence agency
  **upgrade queue**, **recall volunteers**, **research keyboard shortcuts** (skip the details
  window, select the next slot), research without spending limited-use bonuses, **saved fleet
  compositions** (per session), **G** merges selected fleets and **Shift+G** merges all in a theatre,
  right-click on the reserve-fleet button sends all ships of the theatre to reserve, notification
  grouping for calls to arms and military access, the focus tree mini-map, and political advisor
  filters. The Division Designer button now opens an empty template.
- **Hotkeys**: the Find view moved from **F to Ctrl+F**; day/night moved from N to Ctrl+N; several
  air/naval/operative hotkeys moved to the new `keybinds/00_keybinds.txt` (rebindable). Research is
  still W (`research_open`), Ctrl+W opens the facilities tab.
- Gameplay: naval production lines allocate manpower at build time; new home bases default to auto.

Announced or experimental, **not** in the stable game: the Energy rework (open beta), the Romania
DLC, strikes and the Great Depression mechanics (dev corners only).

## Compatibility fixes

The binary was recompiled with new layouts, so most offsets moved. The migration was done in
three layers, each checked live:

1. **Addresses** (`scripts/macos_build_migrate.py`): vtables by RTTI name (212 classes), globals by
   RIP-relative signatures, functions by vtable slot and unique byte signatures.
2. **Struct fields** (`scripts/macos_field_migrate.py`, `scripts/macos_disp_match.py`): virtual
   functions of each class and all uniquely shaped functions are paired across builds and their
   memory displacements aligned; fuzzy matching (difflib on instruction streams between exact
   anchors) handles functions whose shape changed. Every result was then checked against the live
   game (RTTI class of the pointee, known back-pointers, visible names, values).
3. **Behaviour**: real tool runs on an owned 1936 Germany test game.

Main layout changes (macOS):

| Object | Change |
|---|---|
| GUI window/view base | +0x60 from +0x428 (root 0x430→0x490, idler 0x428→0x488) |
| CContainerWindow | children 0xd30→0xe50; container-derived windows +0x120 |
| CButtonStandard / CIcon | renderer, graphics, handler, flags, text, parent +8 |
| CProductionStatus | 0x18-byte array inserted at 0x288 (later fields +0x18) |
| CBuildingProductionLine | fields from +0x70 +8 (building, repairs, conversion, allied) |
| CNationalFocusView | items/panels/override/zoom +0x1480; detail panel is now `CNationalFocusInfoWindow` |
| CNaviesView | leader window +0xa90, panel windows +0xe90 |
| Others | CFindView +0x120, CInGameMenu +0x3c0, CTopBar, CMapModesInterface +0x360, CDropDownBox +0xc0, diplomacy view, trade offer window, special projects, decision items, production rows, find rows |

Other fixes: Find opens with Ctrl+F (`MacKeys::press_control`); the research tree node reader is
Linux-only and now refuses on macOS (`capability_unavailable`) instead of misreading; a window
root that the game has not created yet is treated as closed; `hoi4_trade` reads the renamed
`factories_trade` and the new `factories_consumer`.

## New capabilities

- `hoi4_auto_import`: read or set Automatic Importing (on/off, civilian factory budget), journaled
  and verified from the Trade view. `hoi4_trade_set` refuses with `auto_import_enabled` while the
  game manages imports, instead of failing on a blocked window.

## Live test results (macOS, 1.19.4.1, owned test games, 31 October 2026)

Two owned 1936 Germany games (the first closed at the end of its planned six-hour launch window).
All read tools pass; `hoi4_research_nodes` is Linux-only and refuses on macOS.

| Area | Verified with real actions (journal receipt + memory readback) |
|---|---|
| Economy | research_start, construction_add, production_add, design_set_production, trade_set, **auto_import** (read, on/off, budget 5→8→3, trade_set refused while on) |
| Politics | focus_start (new CNationalFocusInfoWindow), decision_take, event_choose; politics_choose gave the correct refusal (idea disabled) |
| Army | train, army_create, army_assign (new army), army_commander, army_front, template_create/edit/rename |
| Air | air_mission, air_rebase, air_wing_create, air_wing_split, air disband |
| Navy | fleet_region, fleet_mission, fleet_split/create/assign/merge, fleet_reserve, fleet_admiral |
| Other | special_project_assign, design_create (ship), diplomacy, politics_options, state_slots, decisions_list, template_options, design_options, special_projects_panel |

Not verified: doctrine_adopt (typed refusal works; adopting needs 100 naval experience),
design_create for planes and tanks (not enough experience), the fold path of army_assign,
agency_create (the La Résistance DLC is not active in this install, so the tool refuses before
input). About six actions lost their first click on the shared macOS input path; each failed
safe (`input_readback_failed`, memory unchanged) and succeeded on a retry with a new request ID.
Merging task forces that sit in different ports is split by the game into a detached task force.

Automatic Importing against the previous approach: keeping resources covered by hand needs one
`hoi4_trade_set` per resource and partner (Trade view, filter, partner row, slider, Send: about
five inputs and 3–6 s each, repeated whenever a shortage or partner changes, plus a read of the
shortages). With Automatic Importing the agent sets it once (one to three inputs) and the game
starts, resizes and ends contracts itself; the agent only reads the budget and shortages.

Unit tests: 2,643 Rust tests pass; they model the 1.19.4 layout and are not live evidence.

## Limitations and risks

- Only one macOS build is supported; the next patch needs the same migration.
- Linux and Windows are untested on 1.19.4. The Python Find helper now tries Ctrl+F, then F
  (untested on Windows).
- A construction intent recorded before the layout fix had to be closed as Diverged: retries
  cannot settle an action whose pre-dispatch baseline was not stored.
- Not yet driven: the import priority window (order, accepted shortages, oil for fuel),
  automatic diplomatic responses, repair priority, the agency upgrade queue, research keyboard
  shortcuts, saved fleet compositions and G/Shift+G merges. These are the recommended next tools:
  automatic diplomatic responses would remove request pop-ups during long campaigns, and the
  research shortcuts could cut research_start to fewer clicks.
- `deployment.rs` `location_label` (+0x158) may need +0x160 like other button texts; train works
  without it.
