# Live validation queue

What must still be shown in the running game (HOI4 1.19.4.1, macOS), in an order where one loaded save serves
many probes. Each probe names its question, its calls and the read that answers it. Tick a probe with the run
that answered it and move the tool's evidence into `LIVE_VERIFICATION_1_19_4.md`. Navy, sea landings and trade
have their own queue.

## Proof state of the registered tools (23 November 2026)

Counted from the action receipts and read replies of every recorded run since the 1.19.4 migration (31
October): 181 registered tools.

| proof state | player tools | diagnostic tools |
|---|---:|---:|
| a Verified receipt or an answered read on 1.19.4.1 | 124 | 2 |
| documented live 31 Oct - 8 Nov only | 12 | 3 |
| live only on 1.19.3 | 6 | 0 |
| tried, never succeeded | 7 | 4 |
| never run | 5 | 18 |

Live but failing more often than working: hoi4_operative_assign, hoi4_operation_start, hoi4_agency_upgrade,
hoi4_general_trait (no pickable trait), hoi4_template_create (layout not saved), hoi4_doctrine_adopt (the
special-forces folder), hoi4_army_area_defense, hoi4_army_halt, hoi4_design_create, hoi4_save,
hoi4_research_start and hoi4_event_choose (mostly argument faults, eased in this change).

Tried, never succeeded: hoi4_mio_policy, hoi4_mio_design_team, hoi4_faction_create, hoi4_faction_leave,
hoi4_naval_invasion, hoi4_trade_set (refused while auto import is on, by design), hoi4_deploy_locations.
Never run: hoi4_faction_invite_accept, hoi4_fleet_balance, hoi4_fleet_delete, hoi4_peace_options (with a
conference open), hoi4_template_delete. Live only on 1.19.3: hoi4_air_hold, hoi4_air_wing_merge,
hoi4_design_obsolete, hoi4_special_project_recruit, hoi4_special_project_stop,
hoi4_special_project_unassign.

## Session 1: Germany at war, May 1940 (about 95 minutes, paused unless said, executor off)

1. New arguments (5 min): `hoi4_event_choose` with `option` set to one of the event's `option_keys`;
   `hoi4_research_start` without `slot` while a slot is empty; `hoi4_air_mission` with `region` as a number;
   `hoi4_politics_options {category:"advisor", slot:3}`.
2. Espionage (15 min): `hoi4_operative_assign` one operative to counter-intelligence and one to an intel network
   in an enemy state, the agency view watched; `hoi4_intelligence` two game days later; `hoi4_operations` and,
   when ready, `hoi4_operation_start` (collaboration government). Question: which click or read-back fails.
3. Design companies (10 min): `hoi4_mios`, `hoi4_mio_policy` on one organisation with the view watched,
   `hoi4_mio_design_team` once. Question: where the organisation list lives on 1.19.4.1.
4. Templates (8 min): `hoi4_template_create` one infantry template with the designer watched. Question: why the
   layout is not saved.
5. Special-forces doctrine (5 min): `hoi4_doctrines`, then `hoi4_doctrine_adopt special_forces_quantity` with
   the doctrine window watched. Question: where the folder's slot is.
6. Command power (5 min): the top bar's command power tooltip (value and cap) beside `hoi4_country_status`;
   `hoi4_general_trait {general, list:true}` for three generals. Question: the real cap; whether any trait can be
   picked.
7. Defensive orders (10 min): `hoi4_army_fallback`, `hoi4_army_area_defense`, `hoi4_army_halt` on three armies,
   `hoi4_armies` after each. Question: why area defence reads no battle-plan orders after the click.
8. Special projects (8 min): `hoi4_special_project_recruit`, `_unassign`, `_assign`, `_stop`.
9. Air missions never flown on 1.19.4 (15 min): one wing each for interception, logistics strike, port strike,
   naval strike and strategic bombing, `hoi4_air` after each.
10. Events left open (5 min, speed 2): when a news event opens, `hoi4_event_choose` with the window watched.
11. Saves (10 min, speed 2): ten `hoi4_save` calls a game day apart while events arrive; first-try successes and
    each failure's code.
    DONE v65h (23 Nov 2026): 10/10 paused and 10/10 running at speed 1, every other try from the naval map mode, all first try, 3.2-8.7 s. Causes fixed: every call (reads included) closed a shown game menu at the end of its pause bracket, so the resident watcher shut the menu the save had opened; a hover sent a zero wheel event.
12. Focus (10 min): executor on with only the focus lane for a game week; when a start is not applied, read
    `hoi4_focus_available` for it and look at the tree.

## Session 2: a short war that ends in a peace conference (about 40 minutes)

A save where a small country falls alone (a 1936 start with a war goal on an isolated minor).

13. Peace (30 min): executor on with take, puppet and liberate in the plan's peace keys; `hoi4_peace_status`,
    `hoi4_peace_options` with the conference window open, then `hoi4_peace_take`, `hoi4_peace_submit`,
    `hoi4_peace_exit`. Note the force-government control (not built yet).
14. Occupation (10 min): `hoi4_occupation`; name the Occupied Territories view's garrison template control and
    its per-state law control (both not built yet).

## Session 3: factions and proposals (about 20 minutes)

15. `hoi4_faction_create`, `hoi4_faction_leave` (watch why its read-back fails), `hoi4_faction_invite_accept`
    on an invitation, `hoi4_diplomacy_incoming` while a proposal is open.

## Session 4: BATCHING PROBES (about 85 minutes)

Each bulk path below is off until its probe passes (`manage.bulk` in the plan, or `bulk: true` on the call);
the single path stays in use until then. A passed probe adds its name to the plan's `manage.bulk`. Seconds
are wall seconds from the receipt (dispatch to receipt) and from the `[order]` timing lines; the situation
report's cleanliness block shows calls, elements changed and seconds per element per action kind.

The May 1940 Germany bed source (1 May 1940, paused, executor off unless said):

16. Factory chords (5 min): `hoi4_production_lines`; a line with a few factories:
    `hoi4_production_set {line, equipment, factories: current + 23, bulk: true}`, then back to the old count
    with `bulk: true`. Expected: Verified both ways, 5 clicks each way (two Shift, three plain; the phase
    timing shows the click count), no trace "chords off". Before: 1.3 s + about 0.3 s a factory (23 factories
    about 8 s); after about 2.5 s. When a chord moves the line by one only, the game on macOS reads another
    modifier: note which (Ctrl or Command) and leave `production_chords` off.
    DONE v60a (23 Nov 2026): anti_air line 1 -> 24 -> 1 Verified both ways, 5 clicks each, 2.56 s and 2.13 s. production_chords on by default.
17. Air missions by base (10 min): `hoi4_air`; a base whose wings are all fighters (three or more) and one
    fighter at a base it shares with another type. `hoi4_air_mission {wings: [the base's wings and the shared
    one], mission: "air_superiority", region, bulk: true}` with the map watched. Expected: Verified; the result's
    `by_base` row for the clean base ok, the shared base's wing ordered by the single path; one base click, one
    mission click and one region click for the clean base. Before: 2.5-5 s a wing; after about 4 s a base. The
    same call again clicks nothing (every wing flies it already).
    DONE v60a: 9 wings (8 of one base, 1 of a mixed base) air_superiority Verified in 7.9 s (0.88 s a wing), 9 clicks; the same call again 1.2 s, no click. air_base_select on by default.
18. Air rebase by base (5 min): `hoi4_air_rebase {wings: [every wing of one base], state_id: another own base
    with room, bulk: true}`. Expected: each wing's transfer_to_base_id is the target; one base click and one
    right click. Before: 1.8-3.4 s a wing; after about 3 s a base.
    DONE v60a: 4 wings of one base rebased Verified in 1.35 s, 3 clicks.
19. Selection questions for the next step (5 min, by hand, nothing kept): in air mode, Shift+Click a second
    base circle while one base is selected (are its wings added?), and Shift+Click (then Ctrl+Click) a wing row
    of the selection view (`airwing_selection_item`): is that wing taken out of the selection? Either answer lets
    mixed bases join the bulk path.
20. Design-company traits in one visit (10 min): `hoi4_mio_trait {organization, list: true}`, then
    `hoi4_mio_trait {organization, trait_indices: [three enabled, visible traits], bulk: true}` with the tree
    window watched; then `hoi4_mio_trait {organization: another, auto: 2, bulk: true}`. Expected: Verified, the
    three traits read queue numbers in the result's `entries`, the second organisation has two enabled traits
    queued. Before: 8.3 s a trait plus 8 s for a listing visit; after about 10 s a visit. Note how often the
    detail window had to be opened again after a click.
    RUN v60a, NOT FASTER: three traits Verified in 26 s (8.7 s a trait, 40 clicks): after each queued trait the detail window closes and the organisation is found again row by row. mio_queue_many stays off until the found row is reopened directly.
21. Executor bed with every bulk path (25 min): a STUDY bed of 7 war days from the same source with the plan's
    `manage.bulk: ["all"]`, hands off; compare its cleanliness table with a bed of the same plan without it
    (air_mission, air_rebase, production_set, mio_trait: calls, elements changed, s per element). Expected: air
    orders at least three times fewer calls, no order repeated after a Verified read-back.

A 1936 start (Germany, paused unless said):

22. Construction fill (5 min): `hoi4_state_slots` for a state with three or more free slots for arms_factory;
    `hoi4_construction_add {building: "arms_factory", state_id, count: <free slots>, fill: true, bulk: true}`.
    Expected: Verified, the state's queue grew by the free slots after one Shift+Click. Before: about 1 s a
    building after the first (a 600 ms settle and a read); after one click.
23. Peace years with every bulk path (20 min): executor on with `manage.bulk: ["all"]` for 60 game days at speed
    3; the cleanliness table against the 1936 campaign baseline (production_set about 1.3 s a line, mio_trait
    8.3 s a trait, construction_add 2.9 s a state).

## Session 5: support, supply and logistics (about 85 minutes, Germany 1939-40 save, executor off unless said)

Ordered by value. Each item names the read-back; memory structures are mapped the way `supplymem.rs` (supply nodes
and province flows) and `stockpile.rs` (the country's equipment stockpile) were: find the number on screen,
search the country object for it, confirm on a second value, then read it without opening the view.

24. Hub motorisation, first (10 min, executor off): `hoi4_army_motorization` level 2 on an army at the front (never
   set live above 1), read it back in `hoi4_armies`, then open the supply map mode and read the supply range of
   the hubs it draws from before and after (screenshot and `hoi4_supply` province supply/headroom of the
   provinces 2-4 beyond the hub); note the trucks taken from the stockpile (`hoi4_stockpile` motorized_equipment
   before and after). Questions: does the army level set every hub it draws from, or only some; how many trucks a
   hub took at level 1 and 2; whether a per-hub level is readable from memory (the hub's node object in
   `supplymem.rs`). Then the executor on with `logistics.hubs` and an operation prepared: the formation's hubs
   go to its `supply.motorization` before the execute.
25. Railway legs (10 min, executor on with one `construction_plan` item `legs: true` from Silesia to a Polish state
   at war): the first leg queued (`hoi4_railway_build` leg:true), the queue read back, the item re-sent after
   `leg_days`. Question: does the leg stop at the own-controlled edge and re-use existing track.
26. Train need (10 min): the top bar's `logistics_button` opens `countrylogisticsview`; the train row of
   `logistics_overview_land_other_entry` shows stock against `needs`/`in_need`. Map the need beside the stockpile
   row `stockpile.rs` already reads; add `need` and `short` to `hoi4_stockpile` train_equipment and trucks
   (hub motorisation use). Read-back: the view's numbers on two dates.
27. Rail and hub throughput (10 min): the supply map mode (`hoi4_map_mode supply`) tooltip of a hub and a railway
   segment: flow, level, damage. Map the railway segment flow next to the node flows `supplymem.rs` reads; add
   `rail_flow` and `damaged` to `hoi4_supply` supply_nodes. Read-back: the tooltip text.
28. Damage and repair (10 min, after an RAF raid or our own logistics strike on the enemy): damaged railway levels
   and infrastructure of a state from the state object the slot read uses; the construction view's `repair`
   entries (`countryconstructionsview`, `production_railway_gun_repair_entry`) and their priority. A tool
   `hoi4_repair_priority {state|building}` if the game exposes one; else the read alone feeds an omission row.
29. Logistics strike (10 min): `hoi4_air_mission logistics_strike` one CAS wing over the Polish or French front
   region; two game days later `hoi4_air_wings` mission mask and the wing's kills in the air view; the enemy's
   supply in `hoi4_wars`. Question: does the mission stay set and does the tool read it back.
30. Railway guns (5 min, read only): `railway_gun.gui` army list (`armies_count_box`, `btn_select_half`); the
   railway gun as a unit in memory; production via `hoi4_production_add railway_gun_equipment` (refused or not).
31. Radar coverage (5 min): `hoi4_construction_add radar_station` in a state once radio_detection is held (the
   executor's `requires` path), the queue read-back; radar level per state from the state's buildings; the radar
   map icon. Coverage is then computed offline from RADAR_RANGE_* and the state centres.
32. Special projects (10 min): `hoi4_special_projects_panel`, a prototype reward choice in the project's reward
   window (`multiple_selection_reward_window`, `reward_option`, `confirm_option_button`): a typed
   `hoi4_special_project_reward {facility_id, option}`; `dismantle_facility_button` read only (100 days).
   Also GAPS Q12 (recruit, unassign, stop).
33. Change a division's template (5 min): the division panel's `template_change_icon` / `change_template_icon`
   (unitview.gui) on one Sicherungs-Division: a typed `hoi4_division_change_template {army, divisions, template}`
   would move weak divisions to the line template without retraining; the `formations` rule then prefers it to
   an in-place layout edit when the weak template has other uses.

Total: about 325 minutes in five sessions.
