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

Total: about 155 minutes in three sessions.
