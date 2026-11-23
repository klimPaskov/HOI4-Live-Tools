"""What the recorder reads, and what it cannot read yet.

Only tools that take no input lease are called: the server answers them on its
read lane (memory reads, map files), so a recording never sends input, never
opens a panel and never waits behind the executor or the player. Tools that need
a lease (they open a view to read it) are listed in GAPS with the domain they
would have covered; the recorder does not call them.
"""
from __future__ import annotations

# (key, tool, arguments, every): `every` = take this read on every Nth snapshot
# (1 = always). Heavy and slow-changing reads are spaced out; the snapshot keeps
# the last value with the date it was read.
READS = (
    ("overview", "hoi4_overview", {}, 1),
    ("country_status", "hoi4_country_status", {}, 1),
    ("economy", "hoi4_economy", {}, 1),
    ("economy_detail", "hoi4_economy_detail", {}, 1),
    ("resources", "hoi4_resources", {}, 1),
    ("production_lines", "hoi4_production_lines", {"limit": 128}, 1),
    ("production_shortages", "hoi4_production_shortages", {}, 1),
    ("stockpile", "hoi4_stockpile", {}, 1),
    ("construction_queue", "hoi4_construction_queue", {"limit": 128, "include_location": True}, 1),
    ("research_extras", "hoi4_research_extras", {}, 4),
    ("doctrines", "hoi4_doctrines", {}, 1),
    ("commanders", "hoi4_commanders", {}, 1),
    ("armies", "hoi4_armies", {"threat": False}, 1),
    ("supply", "hoi4_supply", {"summary": False}, 1),
    ("wars", "hoi4_wars", {}, 1),
    ("war_stats", "hoi4_war_stats", {}, 1),
    ("diplomacy", "hoi4_diplomacy_state", {}, 1),
    ("air", "hoi4_air", {}, 1),
    ("air_regions", "hoi4_air_regions", {}, 1),
    ("fleets", "hoi4_fleets", {}, 1),
    ("convoys", "hoi4_convoys", {}, 1),
    ("occupation", "hoi4_occupation", {}, 2),
    ("intelligence", "hoi4_intelligence", {}, 4),
    ("mios", "hoi4_mios", {}, 4),
    ("special_projects", "hoi4_special_projects", {}, 4),
    ("templates", "hoi4_division_templates", {}, 2),
    ("alerts", "hoi4_alerts", {}, 1),
    ("events", "hoi4_events", {}, 1),
    ("focus_completed", "hoi4_focus_completed", {"limit": 1}, 4),
    # the watcher's brief, its open omissions and (with the recorder's cursor) the
    # typed changes since the last snapshot
    ("observe", "hoi4_observe", {"since": 0, "limit": 200}, 1),
)

# Per enemy at war: its states (owner, controller, victory points) as the map
# shows them to the player. Taken for the own tag too (own states lost).
STATES_TOOL = "hoi4_states"

# The one-call snapshot tool (newer servers): returns every READS body at once.
SNAPSHOT_TOOL = "hoi4_state_snapshot"

# What no pure read gives today. Each: the domain, why the recorder cannot read
# it, the tool that would show it (needs a lease: opens a view), and the step
# that makes it readable.
GAPS = (
    {"what": "advisors, high command, chiefs of staff and their empty seats; laws affordable now",
     "why": "only hoi4_politics_options lists them, and it opens the politics panel (lease)",
     "proxy": "the game's own alert row (modify_government) and the executor's omissions",
     "live_step": "map the own country's idea slots (CCountry ideas array by category) the way "
                  "economy_detail maps the three laws; then a pure read lists each seat and its holder"},
    {"what": "decisions available and taken",
     "why": "hoi4_decisions_list opens the decisions panel (lease)",
     "proxy": "alert decisions_available",
     "live_step": "map the decision category cache (the panel's model, CDecisionsManager) once; "
                  "taken decisions are in the country's flags/cooldown map"},
    {"what": "trade deals per resource",
     "why": "hoi4_trade opens the trade view (lease); economy_detail reports trade deals null",
     "proxy": "resources imported/exported fields when mapped",
     "live_step": "map CCountry trade-deal array (economy_detail lists it as offset_not_validated)"},
    {"what": "rank of leaders who lead nothing (general or field marshal), skill levels gained",
     "why": "hoi4_commanders reads base values from the character template; rank is known only for "
            "a leader who leads an army group, or while the army panel's list is open",
     "proxy": "leaders listed with rank null",
     "live_step": "find the unit-leader's rank field (CUnitLeader role flags) by diffing one "
                  "promotion (hoi4_army_commander on a group promotes a general)"},
    {"what": "battles in progress (attacker and defender divisions, width, every modifier line)",
     "why": "the combat objects are not mapped; no tool reads them",
     "proxy": "divisions losing organisation in a province next to the enemy",
     "live_step": "probe plan in the notes (battle read): CCombat list from a province, then its "
                  "two sides, width and the modifier table"},
    {"what": "supply hub capacity, railway throughput per node, trucks and trains in stock, attrition",
     "why": "hoi4_supply lists them as unavailable (structures not mapped)",
     "proxy": "province supply, demand, headroom and flows (read)",
     "live_step": "probe plan in the notes (supply nodes): the CSupplyNode array behind the flows"},
    {"what": "surrender progress (own and enemy), war score, war participation",
     "why": "hoi4_wars reports them not_mapped",
     "proxy": "victory points held per enemy from hoi4_states",
     "live_step": "map CCountry surrender progress (the war overview's bar) by reading it before and "
                  "after one victory point changes hands"},
    {"what": "air superiority per region (own and enemy), air losses",
     "why": "hoi4_air_regions reports enemy_air_superiority not_mapped",
     "proxy": "own planes per region",
     "live_step": "map the strategic region's air superiority values (the air map mode tooltip)"},
    {"what": "naval dominance per sea region, ship fuel and organisation",
     "why": "hoi4_fleets reports them not_mapped",
     "proxy": "fleet missions and regions",
     "live_step": "map the sea region's naval dominance array (Strategic Navy map mode)"},
    {"what": "garrison strength and suppression",
     "why": "hoi4_occupation reads resistance and compliance, not garrisons",
     "proxy": "resistance trend per occupied state",
     "live_step": "map the garrison manager (occupied state garrison need vs held)"},
    {"what": "a division's own planning bonus, entrenchment and experience",
     "why": "hoi4_armies gives planning per army (min, mean, max), not per division",
     "proxy": "army planning",
     "live_step": "read CDivision planning and entrenchment offsets next to the organisation the "
                  "supply read already uses"},
)
