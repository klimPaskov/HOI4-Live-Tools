# Design: `hoi4_country_status` and `hoi4_wars` (macOS HOI4 1.19.3, read-only)

Status: research and design only. Nothing in this document was run against a live game
during its writing, and no source file was changed. Every "validated on macOS" mark below
is taken from repo comments, tests or private notes that record a live macOS check;
everything else is explicitly unvalidated. No offset in this document is a recommendation
to read a field: each unvalidated one is a hypothesis with a stated check.

Target build: macOS Mach-O x86_64, 1.19.3.0, LC_UUID 089EA519-6780-353B-8E39-4E9309B444D4
(`native/src/game.rs` `MACOS`, libc++ strings). Repo rule (AGENTS.md): never copy offsets
across platforms or builds; unknown builds and stale or torn reads fail explicitly; no
console, debug, eval or injected game-function calls in campaigns; no hidden enemy
intelligence.

## 0. Conventions

- Evidence tags used in tables:
  - `V-mac`: recorded as mapped or confirmed live on macOS 089EA519 (repo comment, test, or
    private notes). Strength varies and is stated.
  - `V-mac-plausible`: ran live on macOS and returned plausible values, but no recorded
    comparison with the screen.
  - `X`: not validated on macOS (other platform, other build, or pure hypothesis).
  - `?`: unknown.
- Fixed point: Clausewitz stores most economy and political numbers as int64 with scale
  100000 (`FIXED = 100000` in `native/src/economy.rs`; `O.FIXED` in the pinned upstream
  `third_party/clausewitz_mcp/calling/offsets.py`). Political power is
  `raw_signed / 100000`. Factory counts are `raw / 100000` (`economy.rs`, `mil`, `naval`,
  `civ`). Fractions (stability, war support, world tension) are 0..1 internally and shown as
  percent. `defines/00_defines.lua` (NCountry): `MIN_STABILITY 0.0`, `MAX_STABILITY 1.0`,
  `MIN_WAR_SUPPORT 0.0`, `MAX_WAR_SUPPORT 1.0`, `DEFAULT_STABILITY 0.5`, `DEFAULT_WAR_SUPPORT
  0.5`, `POLITICAL_POWER_LOWER_CAP -500`, `POLITICAL_POWER_UPPER_CAP 2000`,
  `BASE_FUEL_CAPACITY 50000`; the world tension define comment states "range [0,1]".
- Whole men for manpower (no scale evidence either way on macOS, see 3.4).
- Game install paths below are written `<GAME>`; do not commit personal paths.

## 1. What the repo already reads (inventory)

### 1.1 Native (Rust), macOS

| Capability | Function | File | Evidence |
| --- | --- | --- | --- |
| Image base, GS/idler/tag db globals, player tag, date, speed, pause | `game::MACOS` layout, `game::context_from_reader` | `native/src/game.rs` | V-mac (private ANCHORS.md table: values changed with date, speed, player switch) |
| Own country object (tag -> index -> country, vtable, `country+8 == tag`) | `economy::own_production` | `native/src/economy.rs` | V-mac for the join (used live every day by the governor) |
| Own factories (mil/naval/civ trigger counts), project civilians, political power (signed fixed) | `economy::snapshot` (`hoi4_economy`) | `native/src/economy.rs` | V-mac-plausible: a recorded macOS Germany run returned civilian 35, military 28, naval 10, PP raw 232400 then 18405800; no recorded screen comparison found. Layout note in ANCHORS.md: "Economy fields identical" to Linux (production `+0x2b8/+0x318/+0x378`, politics `+0xe0`) |
| Country sub-objects: tech status `+0xd08`, production `+0xd10`, politics `+0xd38`, focus tree `+0x10f8`, focus progress `+0x1108`, divisions array `+0x250`, name `+0x10` | `game::MACOS`, `armies::MACOS` | `native/src/game.rs`, `native/src/launch/macos/armies.rs` | V-mac (vtable joins and live use) |
| Every state: owner, controller, id, vtable; all states array `GS+0x290`; province owner/state/id; provinces array `GS+0x278` | `construction::MACOS` layout (`state_owner 0xc0`, `state_controller 0xc4`, `state_id 0x40`, `province_owner 0x188`, `province_state 0xc0`, `province_id 0x9c`, `gs_states 0x290`, `gs_provinces 0x278`) | `native/src/construction.rs` | V-mac ("state/province IDs by catalog index and owners by own tag", live 18 Oct) |
| Owner and controller of states by tag or name | `states::lookup` (`hoi4_states`) via `holders` | `native/src/launch/macos/states.rs` | V-mac (used by the governor and by private `macplay.py sitrep` to list occupied own states) |
| Own state ids by owner tag | `armies::own_states` | `native/src/launch/macos/armies.rs` | V-mac |
| Own divisions, each division's province and state, armies, orders count | `armies::armies`, `division_states`, `plan_nodes` (`hoi4_armies`) | `native/src/launch/macos/armies.rs` | V-mac (`division_location 0x1f0`, province id `0x9c`, orders `0x98`) |
| State name, province list, unit positions | `mapdata::state_provinces`, `state_centers`, `names()` | `native/src/launch/macos/mapdata.rs`, `states.rs` | V-mac (install files, no memory) |
| GUI tree walk by widget name, text boxes, rectangles | `gui_tree::{children, name, child, container, rect, visible}`; text at widget base `+0x100` (libc++ string) in dialogs and grids | `native/src/gui_tree.rs`, `diplomacy.rs` `grid_texts`/`dialog_text`, `events.rs`, `politics.rs` | V-mac for the walk and for the `+0x100` text read inside event, diplomacy and politics windows; not recorded for top bar text boxes |
| Top bar root: `idler+0x670` (interface manager) `+0x188` (top bar) `+0x68` (root container), buttons found by name | `decisions::topbar_button`, `ui::topbar_rect` | `native/src/launch/macos/decisions.rs`, `ui.rs` | V-mac (used for live clicks on `decisionview_button` and others) |
| Current idea in a politics slot (for example mobilization law = category `government`, slot 0) | `politics::current`, `hoi4_politics_options` | `native/src/launch/macos/politics.rs`, `macos.rs` | V-mac for reads with the politics panel open; unknown with the panel never opened (see 3.9) |
| Diplomacy view rows for a foreign country (action name, enabled, cost) | `diplomacy::current`/`rows` (`hoi4_diplomacy`) | `native/src/launch/macos/diplomacy.rs` | V-mac, needs a lease and opens a view |
| Production lines, construction queue | `hoi4_production_lines`, `hoi4_construction_queue` | `native/src/production_lines.rs`, `construction.rs` | V-mac |

Read cost reference: `hoi4_economy` took about 1 ms and 223 to 303 logical reads in the
recorded macOS run (`freshness.elapsed_ms` 0.9 to 1.0). That is the budget class for a
memory-only status read.

### 1.2 Legacy Python (Windows-validated or inert on macOS)

| Capability | Function | Notes |
| --- | --- | --- |
| Stability and war support **base** fractions, factories, PP | `hoi4_live/memory/reader.py` using `hoi4_live/memory/layout.py` (`country_stability_base 0x10D0`, `country_war_support_base 0x10D8`, Windows build `6a427fb9_3519e78` only) | Windows offsets. `LIMITATIONS` says these are base fractions, not the modified UI values. `UNAVAILABLE` lists wars, supply, equipment as unverified. `hoi4_live/memory/macho.py` states "no game field offsets" and "No Mac HOI4 build is validated here". Not usable on macOS. |
| Top bar OCR of all cells | `hoi4_live/perception/topbar.py` `read_bar`, `LEFT_CELLS`, `HOVER`, `glyphs`, plus `hoi4_live/platform/ocr_macos.py` | Cells: political_power, stability, war_support, manpower, factories, fuel, supply, convoys, command_power, 3 experiences, world_tension (right edge). Calibrated on Windows (`tests/test_topbar_tooltip.py` is Windows-only). Needs a screenshot. |
| Tooltip read by hovering a top bar item | `hoi4_hover` in `hoi4_live/tools/lowlevel.py` (default wait 0.9 s) | Moves the real cursor: this is input, needs the input owner. |
| Daily telemetry mod (observational) | `hoi4_live/telemetry/modgen.py`, `parser.py` | Writes `HOI4LIVE` JSON lines to `logs/game.log` once per game day. Fields below. Not listed as used by the macOS native stack in the files read; whether the mod is loaded in a macOS campaign was not verified. A copy of `game.log` on this machine contains no `HOI4LIVE` lines. |

Telemetry mod fields relevant here (script triggers, so values are the game's own final
numbers, not base values): `day`: `pp`, `pp_gain` (`political_power_growth`), `stab`
(`stability`, 3 decimals), `ws` (`has_war_support`), `mp_k` (`manpower_k`), `mp_max_avail_k`
(`max_available_manpower_k`), `mp_deployed_k`, `mp_queue`, `garrison_need`, `civ`, `mil`,
`nav`, `civ_free`, `civ_proj`, `fuel_k`, `fuel_max_k`, `fuel_ratio`, `convoys`, `threat`,
`states`, `states_ctrl`, `occupied`, `war`, `enemies`, `war_months`, `surrender`;
`res` per resource: net, `_prod`, `_used`, `_exp`, `_imp`; `eq` per equipment archetype:
stock, `_army`, `_need`; `war` per enemy; `we` enemy pairs of majors and countries at war
with the player; `front` and `front_own` (enemy-held states touching ours, and our states
touching them); `lost`; `wld` (threat, year, countries at war). In campaign mode
`modgen.templates` drops the `HIDDEN` keys (`enemy_str`, `ally_str`) and filters foreign rows
to `PUBLIC_WAR`/`PUBLIC_COUNTRY` in `hoi4_live/state/visibility.py`. Consumer goods share,
factory output modifier, conscription law and war-goal timers are **not** in the mod.

### 1.3 Pinned upstream reference (Linux 1.19.2, not this build)

`third_party/clausewitz_mcp/calling/{offsets,game,combat,armyops}.py` (smerss, MIT):
`CO_STABILITY_BASE 0x10A8`, `CO_WARSUPPORT_BASE 0x10B0`, `CO_MANPOWER_SUB 0x328`
(`GetManpower(country+0x328)`), `CO_FUEL 0x1520` (object, int64 at `+0x08`),
`CO_RESOURCES 0x11C0` (7 records of 16 bytes from `+0x60`), `CO_DIPLOMACY 0xF60`
(`CDiplomacyStatus`), `DS_WAR_RELATIONS 0x20`, `CO_FACTION 0x2B00`, `CO_MODIFIERS 0x5B0`,
`CO_ARMIES_IN_COMBAT 0x12F0`, `CFront` `FRONT_SECTIONS 0x58`, `CFrontSection` province list
`0x30`. All are for a different OS and build: **X**. They are used below only to name which
object to look for, never as macOS offsets. Its stability and war support "final" values come
from **calling game functions inside the process** (`FN_GET_STABILITY`), which this project
forbids for campaigns; only the base fields are plain memory reads.

## 2. Class vtables known for macOS (identity anchors)

From the private `vtmap.json` (RTTI class -> primary vtable, live macOS, image-relative):
`CDiplomacyStatus 0x3342770`, `CWarRelation 0x3347ca8`, `CWarGoal 0x3325720`,
`CWarGoalType 0x3385390`, `CTimedWargoalActivity 0x3387f38`, `CWorldThreat 0x3349148`,
`CThreatSource 0x33490e8`, `CWarScoreBreakdown 0x33491d8`, `CFront 0x3320960`,
`CFrontSection 0x33209c8`, `CCountryManpower 0x330b4b8`, `CCountryResources 0x339faf8`,
`CPoliticalStatus 0x3387148` (already used by `game::MACOS`), `CStrategicResource 0x331e1c8`,
`CWarOverView 0x34929c8`, `CWarRelationStripView 0x347e0f8`, `CActiveWargoalStripView
0x347e1f8`. These prove the **classes exist** in this build and give an exact value to
search for; they say nothing about field offsets. There is no `CWar` class: a war exists as
per-pair `CWarRelation` objects (upstream notes that two countries share the same
`CWarRelation` pointer in their `CDiplomacyStatus` war lists).

Search technique that needs no copied offset: an embedded or referenced sub-object is found
by scanning the 8-byte-aligned words of the `CCountry` object (bounded, for example
`country+0x100 .. country+0x3000`) for a pointer whose pointee's first word equals
`image + vtable`, or for a first word equal to `image + vtable` (embedded object). The hit
offset is then the macOS offset and is recorded together with the evidence. The existing
joins (`production +0xd10`, `politics +0xd38`) are all of the pointer kind.

Relative-position hints only (to bound the search window, **not** values to use): between
Linux a4971c9e and macOS, `production` moved `0xf40 -> 0xd10` and `politics` `0xf68 -> 0xd38`
(delta -0x230), `focus tree` `0x1338 -> 0x10f8` (delta -0x240). A Linux/upstream field
between those (stability base `0x10A8`) therefore lies plausibly at `0xe68` or `0xe78` on
macOS (`+0x28` Windows-to-Linux spacing of base stability from production is `0x168` on both
platforms, which would give `0xe78`). Divisions array moved by a different delta (`0x290 ->
0x250`), so deltas are not constant. Treat as a search window of `0xe00..0xf00`.

## 3. `hoi4_country_status`: field by field

Common rules: own country only (the player tag from `GS+0x4d8`, joined through
`economy::own_production`-style checks). No foreign country values. Primary sources are
ordered: (1) memory, (2) top bar text boxes read from GUI memory (no input), (3) panel text
read from GUI memory (opens a panel: needs the input lease), (4) screenshot OCR, (5)
derived. Every returned field carries its `source` and `validated` mark.

### 3.1 Top bar text boxes from GUI memory (new, shared by most fields)

Vanilla `interface/topbar.gui` (the container `topbar`) defines `instantTextBoxType` widgets
named: `pol_power`, `manpower`, `stability`, `war_support`, `industrial_capacity`,
`convoys_count`, `fuel_value`, `supply_value`, `command_power_value`, `experience_value`,
`navy_experience_value`, `air_experience_value`, `nukes_value`, `threat_value` (world
tension), `DateText`. The macOS root path `idler+0x670 -> +0x188 -> +0x68`, descent by
child name and `container()`, is V-mac (clicks on `decisionview_button` and other buttons).
Reading text from a found text box would be `r.string_bounded(widget + 0x100, ..)`, the
same read V-mac in events, diplomacy and politics windows. **Unvalidated**: that offset on
the top bar text boxes, how often the engine rewrites the text (per frame or per day), and
the exact string formats (percent rounding, `K`/`M` suffixes). Per `test_topbar_tooltip.py`
(Windows OCR) the shown forms are like `94%`, `261.62K`, `29 d`. All values read this way
are **UI-rounded**; that is stated in `precision`.
Cost: one snapshot, a descent of a few dozen nodes: estimated 1 to 3 ms (no measurement).

### 3.2 Stability

- Quantity: fraction 0..1, shown as percent (`STABILITY_DESC`; defines above). Effects are
  listed in the stability tooltip (`STABILITY_EFFECT_TITLE`).
- Existing reader: none in native. Legacy Windows `reader.py` returns **base** only.
- Final (modified) value from memory: needs the game's modifier evaluation, which upstream
  obtains by calling `GetStability`. Not allowed. A pure read would have to replicate the
  base plus modifier sum from the modifier container (Linux `CO_MODIFIERS 0x5B0` plus a
  modifier-definition table): unknown on macOS and a large reverse-engineering job.
- Base value candidate: a `int64 / 100000` field in the window `country+0xe00..0xf00`
  (hint only). Validated on macOS: **no**. It would be reported as `stability_base`, never
  as `stability`.
- Recommended source: top bar `stability` text (3.1). Validated on macOS: **no** (path yes,
  text offset and format no).
- GUI/OCR fallbacks and speed: (a) screenshot OCR of the cell
  (`perception.topbar.read_bar`, glyph path): about 0.3 to 1 s including capture (estimate,
  no macOS timing found); (b) hover tooltip (`hoi4_hover target=stability`): about 1.2 to 2 s,
  moves the cursor, gives the factor breakdown (war factor, ruling-party popularity,
  coastal protection, propaganda); (c) telemetry `stab` line: current to the last game day,
  3 decimals.

### 3.3 War support

Same as stability. Widget `war_support`. Unit fraction 0..1. Breakdown strings
`WAR_SUPPORT_AT_OFFENSIVE_WAR`, `..._DEFENSIVE_WAR`, `..._TENSION`, bombing, interception,
casualties, raided convoys. Defines: offensive war -0.2, defensive war +0.2, tension impact
up to 0.4. Telemetry `ws`. Base candidate: next to stability base (`+0x8` upstream and
Windows layouts, which the hint window covers); validated: no.

### 3.4 Manpower available and total

- Units: whole men (`manpower_k` in the telemetry mod has 10-man resolution). Top bar shows
  free manpower abbreviated (`261.62K`): resolution 10 men at that size, 1,000 or more at
  `M`.
- Definitions from localisation (`country_l_english.yml`): `MANPOWER_USAGE` splits **Free
  Manpower** and **Used Manpower** (army: in the field, in training; air; navy; garrison);
  `MANPOWER_CURRENT` gives **Total Manpower** and "percent of Eligible Core Population
  available"; `MANPOWER_NON_CORE` the non-core part; `MANPOWER_GROWTH` monthly growth;
  `MANPOWER_TOTAL` "Total Manpower: MAX". Whether "total" means free plus used, or the
  eligible-population maximum, must be settled from the tooltip text on the first live look
  (see validation V3); the schema therefore returns the tooltip's own labelled numbers
  rather than a guess.
- Existing reader: none (`hoi4_economy` does not carry manpower; PLAYBOOK_TOOL_GAPS notes it
  is absent).
- Memory candidate: class `CCountryManpower` (vtable `0x330b4b8`) exists. Upstream passes
  `country+0x328` to `GetManpower` (a function call), implying an embedded or sub-object
  with an int64; location and field offsets on macOS unknown. Validated: **no**.
- Recommended source: top bar `manpower` text for `available`; tooltip text for the
  breakdown only when a lease is held (opening a tooltip needs a hover) or from telemetry
  `mp_k`, `mp_max_avail_k`, `mp_deployed_k`, `mp_queue`, `garrison_need`.
- Divisions' own strength/organisation (`hoi4_army`) is not manpower.

### 3.5 Consumer goods share

- Definition (`production_l_english.yml` `PRODUCTION_CONSUMER_GOODS_DESC`): the economic
  policy expects a percentage of the country's **civilian and military** factories to produce
  consumer goods; the need is filled from **civilian** factories. Modifier
  `consumer_goods_factor` ("Consumer Goods Factories factor"). The displayed number is a
  factory **count**.
- Existing reader: none. `economy::snapshot` subtracts two i32 counts at `production+0x3b8`
  and `+0x3c0` from the civilian total and two at `+0x2f8`, `+0x300` from military; what
  those counts are (consumer goods, trade, occupation, other) is not documented in the
  repo, and they are not exposed. Hypothesis to test: one of them is the consumer goods
  factory count. Validated: **no**.
- GUI: construction view window `static_consumer_goods_entry`, text child `factories`
  (`interface/countryconstructionsview.gui`); production view row
  `production_consumergoods_line_entry`, text child `goods` and `factories`
  (`countryproductionlineview.gui`). Both views must be open or materialised. Native
  already has construction and production view openers (`construction_open.rs`,
  `production.rs`), so a panel-text fallback is the same pattern: open (lease), read text,
  close. Cost: about the cost of those existing opens, in the order of 1 to 2 s with input
  (no measurement).
- Share to return: `consumer_factories / (civilian_total + military_total)` using the
  `hoi4_economy` totals **only after** validation V4 proves which totals the count refers
  to; also return the raw count and both totals.

### 3.6 Fuel and resource balance

- Fuel: top bar `fuel_value` (shown as days per the Windows OCR tests; exact semantics
  unverified on macOS). `FUEL_DESC`: fuel comes from excess oil, capacity from base
  (`BASE_FUEL_CAPACITY 50000`) plus fuel silos. Telemetry: `fuel_k`, `fuel_max_k`,
  `fuel_ratio`. Logistics view: `fuel_info` window. Memory candidate: object holding an
  int64 fixed value (upstream `CO_FUEL`, Linux only). Validated: **no**.
- Resources: the seven resources in `common/resources/00_resources.txt` order are `oil,
  aluminium, rubber, tungsten, steel, chromium, coal`. Class `CCountryResources` (vtable
  `0x339faf8`) exists; upstream reads 7 records `{int64, int32 id, ..}` of 16 bytes from
  `+0x60` of that object (Linux, unvalidated here; what the int64 is (stock, net, produced)
  is not documented). GUI: production view `resource_strip_container`/`resources_grid`
  (7 slots), logistics view `resource_strip_container`/`resources_grid` and per-equipment
  `resources_grid`. Telemetry: `res` (net), `_prod`, `_used`, `_exp`, `_imp`. Validated
  macOS: **no**.
- Convoys and supply (cheap by-products of the top bar): `convoys_count`, `supply_value`.

### 3.7 Equipment stockpile and deficit ("if cheap")

Not cheap. No native reader. Memory location of the country stockpile is unknown on macOS
(an `EquipmentDatabase` global exists: `0x34d0be0`, catalog `+0x50`, variant to type
`+0x3e0` per ANCHORS.md, used by production lines; it is the catalog, not the stockpile).
GUI: the logistics view (top bar `logistics_button`) `materiel_grid`: per equipment entry
text children `needs`, `produced`, `balance`, `in_stock`
(`countrylogisticsview.gui`). Telemetry `eq`: stock, in armies, needed for a configured
equipment list. Recommendation: ship the schema field as optional, source `logistics_view`
or `telemetry`, behind a lease; do not block the first release on it.

### 3.8 Factory output modifier

Definition: the net multiplier on factory output (base `BASE_FACTORY_SPEED 4`, `..._MIL 3.5`,
`..._NAV 2` in defines; plus laws, ideas, stability effects). No reader and no widget that
shows one "output modifier" number was found in `topbar.gui`. Sources, in order of
feasibility: the stability tooltip lists the stability-derived effects
(`STABILITY_EFFECT_TITLE`), the factories tooltip lists factories in use and owned/occupied/
damaged (`TOPBAR_FACTORY_STATUS_*`, `MILITARY_FACTORIES_STATUS`), and the real number lives in
the modifier container (Linux `CO_MODIFIERS 0x5B0` of `{int32 id, int64 value}` plus a
definition table `SModifierDef` stride `0x78`): unknown on macOS, **validated: no**. Return
`null` with `reason:"unavailable"` in v1 unless the tooltip route is built.

### 3.9 Political power rate and conscription law

- PP value: `hoi4_economy.political_power` (V-mac-plausible). PP rate: telemetry
  `pp_gain`; tooltip `MODIFIER_POLITICAL_POWER_GAIN`; memory unknown. Cheap derived rate:
  two `hoi4_economy` reads at game dates `d1 < d2` give `(pp2 - pp1) / (d2 - d1)` per day.
  This is **net of spending**, so mark `source:"derived"` and invalidate the window when PP
  decreased (an idea, advisor or decision was bought). No validation needed beyond a
  PP-tooltip comparison.
- Conscription (mobilization) law: category `government`, slot 0 (see `hoi4_politics_options`
  description in `launch/macos.rs`). `politics::current(process, "government", 0)` reads the
  slot's current idea key from the politics view. Keys in
  `common/ideas/_manpower.txt`: `disarmed_nation, volunteer_only, limited_conscription,
  extensive_conscription, service_by_requirement, all_adults_serve, scraping_the_barrel`.
  Unknown: whether the slot grid exists while the politics panel was never opened this
  session (the function is used with the panel open). If absent, return the code
  `view_not_materialised` and offer a lease-gated open. Memory (`CPoliticalStatus` idea list)
  offsets unknown on macOS.

### 3.10 Proposed JSON schema

Request (all optional):

```json
{"fields": ["stability","war_support","manpower","consumer_goods","fuel","resources",
            "equipment","factory_output","political_power","conscription_law","topbar"],
 "sources": ["memory","topbar_text"],
 "lease_token": "…",
 "equipment_limit": 24}
```

`sources` defaults to `["memory","topbar_text"]` (pure read, no lease, never pauses).
`"panels"` and `"tooltip"` open views or hover and require `lease_token`
(error `input_lease_required` otherwise, as in `launch/macos.rs`).

Response:

```json
{
 "source": "native own country status",
 "build_id": "089EA519-6780-353B-8E39-4E9309B444D4",
 "owned": true,
 "player": "GER",
 "game_time": {"date_raw": 60759371, "paused": false, "speed": 3},
 "fields": {
  "stability":   {"value": 0.94, "unit": "fraction", "display": "94%", "precision": 0.01,
                  "source": "topbar_text", "validated": false},
  "war_support": {"value": 0.05, "unit": "fraction", "display": "5%", "precision": 0.01,
                  "source": "topbar_text", "validated": false},
  "manpower":    {"available": 261620, "available_text": "261.62K", "resolution": 10,
                  "total": null, "used": null, "source": "topbar_text", "validated": false},
  "consumer_goods": {"factories": null, "share_of_civ_mil": null, "expected_share": null,
                  "reason": "unvalidated_source"},
  "fuel":        {"display": "29 d", "days": 29, "stock": null, "capacity": null,
                  "source": "topbar_text", "validated": false},
  "resources":   {"oil": {"balance": null, "produced": null, "consumed": null,
                  "imported": null, "exported": null}, "…": {}},
  "equipment":   [{"equipment_id": "infantry_equipment", "in_stock": null, "needed": null,
                  "balance": null, "produced": null}],
  "factory_output": {"value": null, "reason": "unavailable"},
  "political_power": {"value": 13.38, "raw_signed": 1338000, "raw_units_per_point": 100000,
                  "daily_rate": {"value": null, "source": "derived", "window_days": null}},
  "conscription_law": {"key": "limited_conscription", "source": "politics_view"},
  "topbar": {"convoys": 641, "supply": 1.0, "world_tension": 0.0}
 },
 "unavailable": [{"field": "factory_output", "code": "field_unavailable"}],
 "freshness": {"observed_unix_ms": 0, "elapsed_ms": 0.0, "read_calls": 0, "read_bytes": 0},
 "observation_paused_game": false,
 "input_authorized": false
}
```

Rules: a field that cannot be produced is `null` with a `reason` and listed in
`unavailable`; the call itself fails only for whole-call errors. `validated:false` is kept in
the payload until the validation in section 7 has been recorded, so a caller cannot mistake
a hypothesis for an observation. `precision` always states the UI rounding.

### 3.11 Error codes

Reused (already in the repo): `capability_unavailable` (non-macOS or unreviewed build),
`player_unavailable`, `invalid_economy_owner`, `invalid_economy_value`, `invalid_gui`,
`gui_target_unavailable`, `torn_read`, `short_read`, `invalid_array`, `invalid_type`,
`input_lease_required`, `ui_blocked`, `invalid_arguments`.
New: `field_unavailable` (per field, not an error of the call), `topbar_unmaterialized`
(manager `+0x188` null or root missing, for example in a menu or before the game loads),
`topbar_text_unparsed` (string format not recognised; includes the raw string),
`view_not_materialised` (panel-backed field, no lease or never opened), `stale_text`
(optional: text unchanged across a game-day change while the date moved).
Torn-read behaviour follows `ui::read`: retry up to 8 times with a 5 ms ramp, then fail.

## 4. `hoi4_wars`: data sources

### 4.1 What is known today

- Owner and controller of every state: V-mac. From this alone the tool can compute, in one
  snapshot of the state array (about 1,100 states, order of 10 ms estimated, no
  measurement): own states controlled by someone else (occupied), foreign states controlled
  by the player, and for each controller tag the number of own states it holds.
- Own divisions and the state each stands in: V-mac (`armies::division_states`).
- State to provinces and province positions: install files (`mapdata.rs`). **State
  adjacency is not available anywhere in the repo.**
- Who is at war: nothing read. The only proxy is "controller of an own state is a foreign
  country", which is **not** an authority: an ally can control own states, and wars without
  occupation or contact are invisible (`plan_run.rs` already uses this proxy for its `war`
  step, with the caveat that it adds controllers of own states to its enemy set).

### 4.2 War participants (memory candidate, object-identity method)

Upstream `calling/combat.py::war_relations` reads, for the own country, the array of
`CWarRelation*` in `CDiplomacyStatus` (`CCountry+0xF60`, array at `+0x20`, Linux 1.19.2),
then for **every** country reads the same array and treats countries that hold the same
`CWarRelation` pointer as the two sides. It deliberately avoids side fields because their id
spaces are inconsistent after civil-war countries appear. This is offset-light and exact if
three macOS offsets are found:

1. `country -> CDiplomacyStatus` pointer (hint window `0xd00..0xd60`; Linux `0xf60` minus the
   `0x230` delta of neighbours gives `0xd30`; **unvalidated**; self-checking because the
   pointee's first word must equal `image + 0x3342770`).
2. array header of `CWarRelation*` inside it (upstream `+0x20`; checked by every entry's
   first word equalling `image + 0x3347ca8`).
3. optionally per relation: threat (`+0x190` upstream), first-was-instigator (`+0x198`),
   side ids (`+0x0C/+0x10`, unreliable): **do not use**; derive the enemy pair from pointer
   identity only.

Cost: header of 440 countries plus a few entries each: order of 1 to 3k logical reads, a few
ms (estimate). Output must be restricted to wars in which the **player** participates
(public in the war overview; enemy economies and force sizes stay hidden per
`state/visibility.py` `PUBLIC_WAR`).

Because a relation is per pair, "a war" is derived: the enemy set E is the set of countries
sharing a relation with the player. Allies of the player in that war are the player's
faction/ally members that share relations with members of E (needs the faction object, see
GUI source below). v1 may report only `enemies` and `at_war`, and put allies into
`unavailable`.

### 4.3 War overview GUI as a fallback (participants, sides, war score)

`interface/waroverview.gui` window `waroverview_window`: `attacker_participants`,
`defender_participants`, `attacker_grid`, `defender_grid`, `warscore_progressbar`; entries
`war_faction_entry` (faction_name, call_all, divisions, ic, casualties) and `war_ally_entry`
(country_name, surrender_progressbar, warscore, divisions, ic, casualties). Classes in the
build: `CWarOverView`, `CWarAllyItem`, `CWarFactionItem`. Entry points: the menubar war
overview shield button (`WarOverviewButtonWindow` in `menubar.gui`, button
`button_war_overview_item`) and the world tension popup (`worldtensionwindowpopup.gui`,
`show_wars_button`, entries `world_tension_war_entry` with `war_name`, flags). Opening needs
ordinary input and a lease; whether any of these windows exist while closed, and the
hotkey, are **unknown** (no mapping in the repo). Per the visibility policy, do **not**
return the `divisions`, `ic`, `casualties` of other countries; surrender progress and
war score are in `PUBLIC_WAR`/`PUBLIC_COUNTRY`.

### 4.4 Own states threatened, occupied, and enemy-held states next to own armies

Definitions (to be fixed in the schema docs):

- `occupied_own_states`: `owner == player` and `controller != player`.
- `lost_to`: group of the above by controller tag.
- `threatened_own_states`: own states controlled by the player that are land-adjacent to at
  least one state controlled by an enemy in E.
- `enemy_held_adjacent_to_armies`: states with `controller in E` that are adjacent to a state
  in which at least one own division stands, with the armies and divisions involved.
- `armies_in_enemy_states`: own divisions standing in a state controlled by E.

Data needed: state adjacency (new), state owner/controller (V-mac), division state
(V-mac), E (4.2 or the proxy, flagged `enemies_source`).

State adjacency from install data (`<GAME>/map`): `provinces.bmp` (34,603,062 bytes in this
install, 24 bpp, matches 5632 x 2048 x 3), `definition.csv` (`id;r;g;b;type;coastal;terrain;
continent`), `adjacencies.csv` (sea and special crossings with `Through`), and
`history/states/*.txt` (province lists, already parsed by `mapdata::state_provinces`).
Method (as upstream `mapdata.build_adjacency`): scan the bitmap, mark two provinces adjacent
when horizontally or vertically neighbouring pixels differ, map colour to province id, keep
only land provinces for land fronts, fold provinces to states via the state files, and cache
per install directory like `mapdata::positions`. Cost: one-time scan of 11.5 M pixels; order
of 0.1 to 0.5 s in Rust (estimate), then lookups are free. The runtime province to state
mapping is also in memory (V-mac: `province_state +0xc0`, `province_id +0x9c`), which can be
used instead of the state files to survive any history-file mismatch.
Unknown: whether the engine already stores province neighbour lists in `CProvince` (would
avoid the bitmap); no field was identified.

### 4.5 Borders without a front line

"Front line" here is ambiguous, and the schema must say which meaning it returns:

1. **war border**: a land border between a player state and a state controlled by an enemy
   in E. Pure data (4.4).
2. **covered border**: some own army has a front-line order along it. Orders exist as
   `COrderInstance` under an army's order list (`group_orders +0x98`, vtable `0x3313328`,
   children lists `+0x1c8`/`+0x2a0`; V-mac, but only **counting** is implemented:
   `armies::plan_nodes`). What an order node holds (provinces or front section reference,
   its kind: front, offensive, move) is **not mapped** on macOS. `executing` (plan running)
   is read (`armies::executing`).
3. **game front object**: `CFront` (vtable `0x3320960`) with `CFrontSection` (vtable
   `0x33209c8`); upstream finds them by an object-index heap scan and reads provinces per
   section. No heap scan exists in native; scanning is a large read budget. Unvalidated.

v1 answer: report for each war border (kind 1): `own_divisions_in_state`,
`own_divisions_in_neighbour_zone`, `army_names`, and `front_line_ordered` as a tri-state
`true | false | null`. `null` is returned until the order node layout is mapped; an
`inferred` heuristic may be offered separately and labelled: an army with `plan_nodes > 0`
whose `main_state` is the border state or adjacent to it counts as likely covered. Borders
with countries not in E are reported as `peace_border` (no front possible).

### 4.6 War-goal timers and world tension

- World tension: top bar `threat_value` (right edge, text `"$VAL|%0$"` form
  `TOB_BAR_THREAT`; tooltip `THREAT_TOOLTIP`, "Current value: ..%"); memory class
  `CWorldThreat` (vtable `0x3349148`) exists, no offset or global known. Telemetry
  `wld.threat`. Unit: fraction 0..1. Validated macOS: **no**.
- War goals: types and expiry are in `<GAME>/common/wargoals/00_invasion.txt`:
  `take_state expire = 60`, `take_state_focus 730`, `puppet_wargoal_focus 730`,
  `liberate_wargoal 730`; others have no `expire`. Justification time and cost are driven by
  defines (`BASE_GENERATE_WARGOAL_DAILY_PP 0.2`, `MIN_WARGOAL_JUSTIFY_COST`,
  `WARGOAL_WORLD_TENSION_REDUCTION -0.5` at 100 percent tension; the cost to justify falls
  as tension rises, which matches m007/m008 notes of about 160 days at 50 percent tension
  and about 10 days at 91 percent; `docs/PLAYBOOK_TOOL_GAPS.md`). The **current**
  justification progress and the remaining days of a completed goal are **not read
  anywhere**: `hoi4_diplomacy` lists action rows with name/enabled/cost only and only
  verifies row changes (for example "Justify War goal" becoming "Cancel Justification").
  Classes: `CWarGoal`, `CTimedWargoalActivity` (vtable `0x3387f38`), `CActiveWargoalStripView`,
  `CDiplomacyWarGoalItem`; the diplomacy view has `diplomacy_wargoal_entry` (`name`,
  `recipient_flag`). Timer text, if any, probably sits in a tooltip: **unknown**.
  Discovery procedure in V10. Until then: `wargoals: null` with reason, and keep the
  existing pollers.

### 4.7 Proposed JSON schema

Request:

```json
{"include": ["wars","fronts","tension"], "sources": ["memory"], "max_states": 200,
 "lease_token": "…"}
```

Response:

```json
{
 "source": "native own wars",
 "build_id": "089EA519-6780-353B-8E39-4E9309B444D4",
 "player": "GER",
 "at_war": true,
 "enemies_source": "war_relations_memory",
 "wars": [
  {"enemies": [{"tag": "POL", "name": "Poland", "surrender_progress": null}],
   "allies": null,
   "we_started": null,
   "war_score": null,
   "wargoals": null}
 ],
 "fronts": {
  "occupied_own_states": [{"state_id": 87, "name": "…", "controller": "POL"}],
  "lost_to": {"POL": 1},
  "threatened_own_states": [{"state_id": 64, "name": "…", "adjacent_enemy_states": [89],
                             "own_divisions": 3}],
  "enemy_held_adjacent_to_armies": [{"state_id": 89, "name": "…", "controller": "POL",
                             "adjacent_own_army_states": [64], "armies": ["Army 1"],
                             "own_divisions": 12}],
  "armies_in_enemy_states": [{"army": "Army 2", "state_id": 89, "divisions": 4}],
  "borders": [
   {"enemy": "POL", "kind": "war_border",
    "segments": [{"own_state": 64, "enemy_state": 89, "own_divisions_near": 3,
                  "front_line_ordered": null}],
    "uncovered_by_divisions": 0, "front_line_ordered_known": false},
   {"neighbour": "DEN", "kind": "peace_border", "segments": 2}
  ]
 },
 "world_tension": {"value": 0.12, "source": "topbar_text", "validated": false},
 "unavailable": [
  {"field": "wars[].allies", "code": "field_unavailable"},
  {"field": "wars[].wargoals", "code": "field_unavailable"},
  {"field": "borders[].segments[].front_line_ordered", "code": "field_unavailable"}
 ],
 "freshness": {"observed_unix_ms": 0, "elapsed_ms": 0.0, "read_calls": 0, "read_bytes": 0},
 "observation_paused_game": false,
 "input_authorized": false
}
```

Fairness constraints: only the player's wars; enemy fields restricted to the
`PUBLIC_WAR`/`PUBLIC_COUNTRY` allowlists in `hoi4_live/state/visibility.py` (tag, name,
surrender progress, opinion, attackers/defenders, participants, casualty and province
capture counts); no enemy division counts, organisation, strength, factories or manpower.
Occupation and adjacency use public map data (owner and controller are on the political
map). The output is observational; no action is authorised.

### 4.8 Error codes

Reused: `capability_unavailable`, `player_unavailable`, `invalid_tag`, `invalid_type`,
`invalid_array`, `map_data_unavailable` (install files missing, as in `mapdata.rs`),
`torn_read`, `short_read`, `invalid_gui`, `gui_target_unavailable`, `input_lease_required`.
New: `field_unavailable`, `war_relations_unavailable` (offsets not validated: the proxy
source is used and `enemies_source:"owner_controller_proxy"` is set, never silently),
`war_relation_inconsistent` (relation pointer held by only one country, or more than two),
`adjacency_unavailable` (bitmap or definition unreadable; occupied lists still returned),
`war_view_not_open` (GUI fallback requested without lease).

## 5. Where to put the code (for the engineer)

- New module `native/src/country_status.rs` (portable pure logic: field structs, schema,
  string parsers for `94%`, `261.62K`, `29 d`; unit tests with injected `ReadMemory` fakes as
  in `tests/economy.rs`) and macOS-only reads in `native/src/launch/macos/status.rs` and
  `wars.rs`, with a `MACOS` layout struct like the other modules (libc++ only; Linux returns
  `capability_unavailable`). Register in `launch/macos.rs` next to `hoi4_armies`: tool list
  (line ~411), the "pure read" `matches!` list (line ~455) and the dispatch (line ~497). Add
  the tool names to `protocol.rs` only if they should also be listed outside the controller.
- Add `provinces.bmp` adjacency to `mapdata.rs` beside `state_centers`, cached per install
  dir.
- Keep logs on stderr; stdout stays protocol JSON.
- Do not add any code path that calls game functions, writes memory, or reads foreign
  economy or forces.

## 6. Speed and pause policy

Memory and top-bar-text sources are single snapshots, never pause the game, need no lease,
and use the existing `ui::read` retry. Hover and panel sources use input and therefore the
lease; they must be requested explicitly (`sources`), restore the UI state, and report their
elapsed time. The tool never pauses and never opens a panel by default. All elapsed-time
figures in this document are estimates except the 1 ms `hoi4_economy` reference.

## 7. Live validation procedure (minutes per field)

Prerequisites: the owned macOS game running a **test** game (not a recorded campaign), 1920x1080,
the player's own country, any date. Use the existing diagnostics: `hoi4_overview`,
`hoi4_economy`, `hoi4_states`, `hoi4_armies`. Write a small throwaway read-only probe (a
`#[test] #[ignore]` or a scratch Rust bin using `Reader`) that logs raw bytes; do not use
game memory writes. Record each result (offset, evidence, date) in a private notes file and
only then flip `validated` in code.

V0. Topbar text path (5 min). Walk `idler+0x670 -> +0x188 -> +0x68`, enumerate descendants
of the root, print `(name, rtti/class, first 256 bytes at widget base +0x100 as libc++
string)` for each of the names in 3.1. Compare with a screenshot taken at the same moment.
Pass: strings equal what the screen shows for `pol_power`, `stability`, `war_support`,
`manpower`, `fuel_value`, `supply_value`, `convoys_count`, `threat_value`. If not found at
`+0x100`, scan the widget for a libc++ string equal to the visible text and record that
offset instead. Then unpause for a game day and confirm the strings update (rate:
per frame or per day). Also confirm behaviour in a menu or paused state.

V1. Political power (2 min). `hoi4_economy.political_power.raw_signed / 100000` against the
screen cell `pol_power` and the PP tooltip. Pass: equal to the displayed integer (the display
truncates or rounds; note which).

V2. Stability and war support (10 min). Read the V0 strings; also hover `stability` and
`war_support` and read the breakdown. Pass: V0 string equals the cell and the tooltip total.
For the base candidate: scan `country+0xe00 .. +0xf00` for 8-byte words with
`0 <= value <= 100000` and print them; pick the pair adjacent by `+0x8` that matches
`(displayed - modifiers)`; confirm by changing a known contribution (change a law or take
an event option with an `add_stability` effect, or toggle `at war`): the candidate moves by
exactly the same amount (for example `+5000` for +5 percent) while the modifier-only part
stays constant. Only then publish it as `stability_base`.

V3. Manpower (10 min). Read the V0 string and the tooltip (`hoi4_hover target=manpower`
or manual hover); write down `Free`, the used categories and `Total`. Pass: top bar equals
`Free`; decide whether `Total` equals free plus used. Check conservation: queue a division
(training) and observe free decrease and training increase by the division manpower. For the
object: scan `country+0x100 .. +0x3000` for a word equal to `image + 0x330b4b8` or a pointer
to such an object; dump 0x80 bytes; look for an int64 equal to the displayed free manpower
(the value may be whole men, or a fixed-point scaled number).

V4. Consumer goods (10 min). Open construction view; read `static_consumer_goods_entry` ->
`factories` text; open production view; read `production_consumergoods_line_entry` ->
`factories` text; hover the consumer goods entry for the percent. Pass: both counts agree
and equal `round(percent * (civ + mil))` using totals from the factories tooltip. Memory
probe: read `production+0x2f8`, `+0x300`, `+0x3b8`, `+0x3c0` as i32 and compare with the GUI
count; change the economy law (a 150 PP action in a test game) and confirm the same field
moves with the GUI count. Pass only if one field tracks it exactly.

V5. Fuel and resources (15 min). Compare V0 `fuel_value` with the logistics view
`fuel_info` text and production-view resource strip values (7 entries, order oil, aluminium,
rubber, tungsten, steel, chromium, coal). Memory: find the `CCountryResources` object by
scanning the country for a pointer whose pointee's first word is `image + 0x339faf8`, then
print 7 x 16-byte records from the object start `+0x60` as `i64/100000`; pass when seven
values equal the production strip numbers (stock vs net identified by sign and by a
building that changes steel). Report which of stock, net, produced the int64 is.

V6. Equipment (10 min). Read logistics `materiel_grid` rows' `in_stock`, `needs`, `balance`
text; compare with telemetry `eq` if the mod is loaded, and with the production line output
(`hoi4_production_lines`) after a week. Memory: unknown; do not attempt in v1.

V7. Factory output modifier (15 min, optional). Hover the factories item and the stability
item; check whether a total output modifier appears. If not, record `unavailable` and move
on.

V8. Conscription law (3 min). `hoi4_politics_options category=government slot=0` with the
panel closed after a fresh game load, then after opening Q once. Pass: returns the idea
shown in the Politics screen (for Germany 1936 `limited_conscription` per
`references/politics-diplomacy-espionage.md`, to be confirmed live). Record whether it works
with the panel never opened.

V9. Wars, memory (15 min).
 (a) Locate `CDiplomacyStatus`: scan the player's country for a pointer whose pointee's
 first word is `image + 0x3342770`; then within that object find arrays whose entries all
 have first word `image + 0x3347ca8` (empty at peace). Do the same for another country.
 (b) In a test game, create a war (declare a war on a weak neighbour, or wait for an AI war
 such as Japan versus China in 1937 and read that country's list). Pass: the array on the
 player contains N entries, the opponent's array contains the same pointer(s); N equals the
 number of enemy countries shown in the War Overview window; allies of the enemy that join
 produce further shared relation pointers.
 (c) Cross-check with the proxy: `hoi4_states <enemy tag>` shows states controlled by the
 player or the enemy only after contact; confirm the memory list is a superset.
 (d) Cross-check with the telemetry `we`/`war` lines if the mod is loaded.

V10. War-goal timers (discovery, 20 min). With a justification running (test game), open the
diplomacy view for the target and record the row text and the tooltip; scan heap for
`CTimedWargoalActivity` (vtable `image + 0x3387f38`) and `CWarGoal` (`0x3325720`) instances
twice, one game day apart, and diff fields; look for an int that decreases by exactly one
per day (or a date) and equals the days left on the tooltip. Then repeat for a completed
goal (expiry 60 days for `take_state`; check `common/wargoals/00_invasion.txt`).

V11. World tension (3 min). V0 `threat_value` against the world tension tooltip
(`THREAT_TOOLTIP`) and the popup. Note the rounding (`$VAL|%0$`).

V12. States and fronts (10 min). Cross-check the new single-snapshot owner/controller
listing against `hoi4_states <TAG>` (identical set expected). After a war has produced
occupations, compare `occupied_own_states` to the political map. Adjacency: pick five known
neighbouring state pairs and five non-neighbours from the map; confirm; confirm that a sea
strait pair from `adjacencies.csv` is not reported as land-adjacent.

V13. Front line orders (30 min, optional, mapping). Give one army a front-line order
(`hoi4_army_front`), then dump the new `COrderInstance` (vtable `0x3313328`) object fields
(0x200 bytes) before and after, to find the kind and province list; repeat with an offensive
line and a move order. Without this the `front_line_ordered` field stays `null`.

Pass criteria are per field. A field that fails stays `validated:false` or is removed from
the default `fields` list. Nothing from the Linux or Windows layouts may be promoted to a
`MACOS` layout constant without its own recorded macOS evidence.

## 8. Unknowns and risks (explicit)

- Top bar text offset, update cadence and exact string formats: unvalidated.
- Final stability and war support from memory without calling game functions: no known
  route; base fields are not the displayed values.
- Manpower, fuel, resources, consumer goods, equipment, factory output modifier, PP rate,
  stockpile, convoys: no macOS memory offsets known; some are searchable by vtable identity
  (manpower, resources); the rest have GUI or telemetry routes only.
- Whether the telemetry mod is loaded in macOS campaigns: not verified here; its data is a
  day behind and needs a restart to change.
- War participants/sides: classes exist, offsets unvalidated; allies and war score need a
  second source (war overview GUI, unmapped).
- War-goal timers: no data source identified.
- Front-line coverage: order contents unmapped; heap scan for `CFront` not available.
- Province adjacency from the bitmap is install-data derived and ignores mods; vanilla
  campaigns only. Strait and rail crossings need `adjacencies.csv` handling.
- Timing figures other than `hoi4_economy` are estimates.
- Lease and input: panel and hover sources cannot run while another controller owns input.

## 9. Suggested order of work

1. V0 and V1, then `hoi4_country_status` v1 with `topbar_text` for stability, war support,
   manpower (available), fuel, supply, convoys, world tension, plus PP and the conscription
   law (about one day including tests).
2. V9 and V12, then `hoi4_wars` v1: single-snapshot occupation, threatened and adjacent
   states, enemies from memory when validated, otherwise the proxy with an explicit
   `enemies_source` (about two days, adjacency scan included).
3. V4 and V5 (consumer goods, resources) via memory identity search; panel fallbacks only if
   memory fails.
4. V10, V13 only when the playbook needs war-goal timers and front coverage; these are
   mapping jobs.
