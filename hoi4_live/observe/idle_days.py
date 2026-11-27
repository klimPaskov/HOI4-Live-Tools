"""Idle days per sink: a run's idleness as one table (owner, 27 Nov 2026: political power, each
experience pool, research slots, factories, dockyards, wings, task forces, manpower, leaders, MIO
points, alerts).

Each open issue belongs to at most one sink by its code (and, for experience, its subject); a sink
is idle on a game day when at least one of its issues is open that day. The detector calls
`IdleDays.step(issues, days)` each time the game date moves; the gate packet prints `table()`.
"""
from __future__ import annotations

from . import issue_rules as R

# sink -> issue codes (the sitrep's, the sinks', the executor's rows and the game's alerts)
SINKS = (
    ("political power", ("political_power_unspent", "political_power_at_cap", "decisions_available",
                         "conscription_above_need")),
    ("army experience", ("experience_unspent:army", "doctrine_affordable", "doctrines_available",
                         "doctrine_track_complete")),
    ("navy experience", ("experience_unspent:navy",)),
    ("air experience", ("experience_unspent:air",)),
    ("officer corps", ("modify_officer_corps", "officer_corps_slot_empty")),
    ("research slots", ("research_slot_idle", "research_slot_available")),
    ("military factories", ("military_factories_unassigned", "free_military_factories",
                            "missing_equipment_production")),
    ("civilian factories", ("civilian_factories_idle", "free_civilian_factories")),
    ("dockyards", ("dockyards_unassigned", "free_dockyards")),
    ("air wings", ("air_wings_without_mission",)),
    ("task forces", ("naval_task_force_idle", "reserve_fleet_ships", "fleet_training_at_war",
                     "admiral_assignable")),
    ("manpower", ("manpower_idle", "manpower_surplus", "no_divisions_in_training")),
    ("leaders", ("army_without_general", "army_group_without_field_marshal", "command_power_at_cap",
                 "can_assign_field_marshal")),
    ("MIO points", ("mio_traits", "mio_state_unread")),
    ("special projects", ("special_project_idle", "special_project_available", "scientist_unassigned",
                          "special_project_facility_idle")),
    ("intelligence", ("operative_recruitable", "operatives_home_only", "decryption_idle", "decryption_unread",
                      "intel_no_network_abroad")),
    ("focus", ("no_national_focus",)),
    ("armies without orders", ("army_without_order", "war_orders_silent", "divisions_unassigned",
                               "unassigned_divisions")),
)
ALERTS = "game alerts (any)"


def sink_of(issue):
    """The sink an issue counts for, or None."""
    code = str(issue.get("code") or "")
    name = R.alert_name(code) or code.split(":", 1)[0]
    subject = issue.get("subject")
    for sink, codes in SINKS:
        for c in codes:
            base, _, sub = c.partition(":")
            if name == base and (not sub or sub == subject):
                return sink
    return None


class IdleDays:
    def __init__(self):
        self.days = {}        # sink -> idle game days
        self.longest = {}     # sink -> longest unbroken idle spell (game days)
        self.spell = {}       # sink -> current unbroken spell
        self.open_now = {}    # sink -> issue ids open at the last step

    def step(self, issues, step_days):
        """Count `step_days` game days for every sink with an open issue (`issues`: the detector's
        issue dicts)."""
        idle = {}
        for it in issues:
            if it.get("state") == "resolved":
                continue
            sinks = []
            s = sink_of(it)
            if s:
                sinks.append(s)
            if R.alert_name(str(it.get("code") or "")) or str(it.get("code") or "").startswith("frame_"):
                sinks.append(ALERTS)
            for s in sinks:
                idle.setdefault(s, []).append(it.get("id"))
        for s in set(self.spell) | set(idle):
            if s in idle:
                self.days[s] = self.days.get(s, 0) + step_days
                self.spell[s] = self.spell.get(s, 0) + step_days
                self.longest[s] = max(self.longest.get(s, 0), self.spell[s])
            else:
                self.spell[s] = 0
        self.open_now = idle

    def rows(self):
        names = [s for s, _ in SINKS] + [ALERTS]
        return [(s, round(self.days.get(s, 0), 1), round(self.longest.get(s, 0), 1), self.open_now.get(s, []))
                for s in names]

    def table(self):
        """The gate packet's table: every sink, idle days so far, the longest spell, open now."""
        lines = ["Idle days per sink (game days with an open issue of the sink; longest unbroken spell; open now):",
                 "| sink | idle days | longest | open now |", "|---|---|---|---|"]
        for s, days, longest, now in self.rows():
            lines.append(f"| {s} | {days:g} | {longest:g} | {', '.join(now[:4]) or '-'} |")
        return "\n".join(lines)
