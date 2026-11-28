"""Idle days per sink: a run's idleness as one table (owner, 27 Nov 2026: political power, each
experience pool, research slots, factories, dockyards, wings, task forces, manpower, leaders, MIO
points, alerts).

Each open issue belongs to at most one sink by its code (and, for experience, its subject); a sink
is idle on a game day when at least one of its issues is open that day. The detector calls
`IdleDays.step(issues, days)` each time the game date moves and `answer(issue, outcome)` for every
lane answer; the gate packet prints `table(lane_turns)`: per row the idle days, the turns its lanes
had and what they answered, so "offered 145 times, did nothing" shows in the gate.

What a row counts (engineer 97, 28 Nov 2026: three rows measured the wrong thing): the game's
decisions_available alert lights while ANY decision is enabled, wanted or not, so it is no
political-power idleness (the executor's political_power_unspent row is: power above the plan's
reserve with a pick of value left); missing_equipment_production is equipment without a line, not
an unassigned factory (every factory of g92b worked all window): its own row; the naval
headquarters admiral (admiral_assignable) is a leader, not an idle task force.

An issue whose owning rule answers with the plan's own choice and its why (the executor's omission
row `left_by_plan`: an equipment the plan removes or opens on a later date, engineer 102) is not
idle: its days go to the row's "left by the plan" count, printed beside the idle days, so the
owner sees the choice and can overrule the plan.
"""
from __future__ import annotations

from . import issue_rules as R

# sink -> issue codes (the sitrep's, the sinks', the executor's rows and the game's alerts)
SINKS = (
    ("political power", ("political_power_unspent", "political_power_at_cap", "conscription_above_need")),
    ("army experience", ("experience_unspent:army", "doctrine_affordable", "doctrines_available",
                         "doctrine_track_complete")),
    ("navy experience", ("experience_unspent:navy",)),
    ("air experience", ("experience_unspent:air",)),
    ("officer corps", ("modify_officer_corps", "officer_corps_slot_empty")),
    ("research slots", ("research_slot_idle", "research_slot_available")),
    ("military factories", ("military_factories_unassigned", "free_military_factories")),
    ("equipment without a line", ("missing_equipment_production",)),
    ("lines short of resources", ("insufficient_resources",)),
    ("civilian factories", ("civilian_factories_idle", "free_civilian_factories")),
    ("dockyards", ("dockyards_unassigned", "free_dockyards")),
    ("air wings", ("air_wings_without_mission",)),
    ("task forces", ("naval_task_force_idle", "reserve_fleet_ships", "fleet_training_at_war")),
    ("manpower", ("manpower_idle", "manpower_surplus", "no_divisions_in_training")),
    ("leaders", ("army_without_general", "army_group_without_field_marshal", "command_power_at_cap",
                 "can_assign_field_marshal", "admiral_assignable")),
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

# A failing order keeps its sink idle: a tool fault of the executor counts for the sink the tool
# fills (g115a, 28 Nov 2026: three focus starts failed while no focus ran and the idle table showed
# the focus row at 0). Exact names first, then prefixes.
TOOL_FAULT_CODES = ("tool_failing", "refusal_retried_unchanged", "invalid_arguments", "precondition_unmet",
                    "readback_no_change")
TOOL_SINKS = (
    ("hoi4_focus_start", "focus"),
    ("hoi4_research_start", "research slots"),
    ("hoi4_general_trait", "leaders"),
    ("hoi4_army_commander", "leaders"),
    ("hoi4_fleet_admiral", "leaders"),
    ("hoi4_doctrine_adopt", "army experience"),
    ("hoi4_operative_", "intelligence"),
    ("hoi4_agency_", "intelligence"),
    ("hoi4_fleet_", "task forces"),
    ("hoi4_air_mission", "air wings"),
    ("hoi4_air_rebase", "air wings"),
    ("hoi4_train", "manpower"),
    ("hoi4_production_", "military factories"),
    ("hoi4_construction_", "civilian factories"),
    ("hoi4_mio_", "MIO points"),
    ("hoi4_special_project_", "special projects"),
)


def tool_sink(tool):
    """The sink a tool fills, or None."""
    tool = str(tool or "")
    for name, sink in TOOL_SINKS:
        if tool == name or (name.endswith("_") and tool.startswith(name)):
            return sink
    return None


def sink_of(issue):
    """The sink an issue counts for, or None."""
    code = str(issue.get("code") or "")
    name = R.alert_name(code) or code.split(":", 1)[0]
    subject = issue.get("subject")
    if name in TOOL_FAULT_CODES:
        return tool_sink(issue.get("tool") or subject)
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
        self.answers = {}     # sink -> {"lanes": set, outcome: count, "last": reason}
        self.left = {}        # sink -> game days its issues stood open as the plan's choice
        self.left_why = {}    # sink -> the last such reason

    def step(self, issues, step_days):
        """Count `step_days` game days for every sink with an open issue (`issues`: the detector's
        issue dicts)."""
        idle = {}
        left = {}
        for it in issues:
            if it.get("state") == "resolved":
                continue
            sinks = []
            s = sink_of(it)
            numbers = it.get("numbers") or {}
            if s and numbers.get("left_by_plan"):
                left[s] = numbers.get("owner_note") or "the plan's choice"
                s = None
            if s:
                sinks.append(s)
            if R.alert_name(str(it.get("code") or "")) or str(it.get("code") or "").startswith("frame_"):
                sinks.append(ALERTS)
            for s in sinks:
                idle.setdefault(s, []).append(it.get("id"))
        for s, why in left.items():
            if s not in idle:
                self.left[s] = self.left.get(s, 0) + step_days
                self.left_why[s] = str(why)
        for s in set(self.spell) | set(idle):
            if s in idle:
                self.days[s] = self.days.get(s, 0) + step_days
                self.spell[s] = self.spell.get(s, 0) + step_days
                self.longest[s] = max(self.longest.get(s, 0), self.spell[s])
            else:
                self.spell[s] = 0
        self.open_now = idle

    def answer(self, issue, outcome, reason=""):
        """One lane answer (acted / refused / waiting / cannot) to an issue of a sink."""
        sink = sink_of(issue)
        if not sink:
            return
        a = self.answers.setdefault(sink, {"lanes": set()})
        if issue.get("lane"):
            a["lanes"].add(str(issue["lane"]))
        a[outcome or "?"] = a.get(outcome or "?", 0) + 1
        if reason:
            a["last"] = str(reason)

    def answered(self, sink, lane_turns=None):
        """The row's lanes, the turns they had (the executor's lane_turns) and their answers."""
        a = self.answers.get(sink)
        if not a:
            return self._left_text(sink).lstrip("; ") or "-"
        turns = {str(r.get("lane")): r.get("turns") or 0 for r in lane_turns or []
                 if isinstance(r, dict) and r.get("lane") and r.get("kind") != "ticks"}
        lanes = sorted(a["lanes"])
        had = [f"{l} {turns[l]} turns" for l in lanes if l in turns]
        counts = ", ".join(f"{k} {a[k]}" for k in ("acted", "refused", "waiting", "cannot") if a.get(k))
        last = (a.get("last") or "")[:140]
        head = "; ".join(had) or ", ".join(lanes) or "?"
        return f"{head}: {counts}" + (f"; last: {last}" if last else "") + self._left_text(sink)

    def _left_text(self, sink):
        if not self.left.get(sink):
            return ""
        return f"; left by the plan {round(self.left[sink], 1):g} days: {self.left_why.get(sink, '')[:140]}"

    def rows(self):
        names = [s for s, _ in SINKS] + [ALERTS]
        return [(s, round(self.days.get(s, 0), 1), round(self.longest.get(s, 0), 1), self.open_now.get(s, []))
                for s in names]

    def table(self, lane_turns=None):
        """The gate packet's table: every sink, idle days so far, the longest spell, open now, and
        the turns its lanes had with what they answered."""
        lines = ["Idle days per sink (game days with an open issue of the sink; longest unbroken spell; open now; "
                 "its lanes' turns and answers):",
                 "| sink | idle days | longest | open now | lane turns and answers |", "|---|---|---|---|---|"]
        for s, days, longest, now in self.rows():
            said = self.answered(s, lane_turns).replace("|", "/")
            lines.append(f"| {s} | {days:g} | {longest:g} | {', '.join(now[:4]) or '-'} | {said} |")
        return "\n".join(lines)
