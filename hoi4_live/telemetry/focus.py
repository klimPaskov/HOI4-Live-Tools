"""Bounded, own-tree focus observations. Progress is NOT proof of active focus.

The installed game's documentation/triggers_documentation.md supplies
has_focus_tree, has_completed_focus and focus_progress. It supplies no
active-focus ID getter. The same progress trigger is used by vanilla
common/scripted_triggers/GER_scripted_triggers.txt (going_war_with_SOV).
Only temporary logging variables are changed by this generated observation code.
"""
import copy
import re

from ..errors import ToolError

KINDS = ("fb", "fc", "fp", "fe")
MAX_TREES = 256
MAX_FOCUSES = 2048
MAX_TOTAL = 32768
NAME = re.compile(r"^[A-Za-z_][A-Za-z0-9_-]{0,199}$")


def catalog(value):
    if value is None:
        return {}
    if (not isinstance(value, dict) or len(value) > MAX_TREES
            or any(not isinstance(tree, str) for tree in value)):
        raise ToolError("invalid or oversized focus tree catalog")
    out = {}
    total = 0
    for tree, ids in sorted(value.items()):
        if (not isinstance(tree, str) or not NAME.fullmatch(tree)
                or not isinstance(ids, list) or len(ids) > MAX_FOCUSES
                or any(not isinstance(fid, str) or not NAME.fullmatch(fid) for fid in ids)
                or len(set(ids)) != len(ids)):
            raise ToolError("invalid or oversized focus ID catalog")
        out[tree] = list(ids)
        total += len(ids)
    if total > MAX_TOTAL:
        raise ToolError("focus catalog exceeds total ID limit")
    return out


def installed_catalog(game_dir, mods):
    from ..gamedata import focus as data
    idx = data.index(game_dir, mods)
    return catalog({tree: sorted(data.tree_focuses(idx, tree)) for tree in idx["trees"]})


def _progress(low=0, high=9):
    """Binary comparisons yield decile bounds, never an invented exact percent."""
    if low == high:
        return "set_temp_variable = { hoi4live_fp = %d }" % (low * 10)
    mid = (low + high + 1) // 2
    return ("if = { limit = { focus_progress = { focus = $FID$ progress > %.1f } } %s } "
            "else = { %s }") % (mid / 10, _progress(mid, high), _progress(low, mid - 1))


def effects(value):
    trees = catalog(value)
    out = ["hoi4live_telemetry_focus = {", "\tif = { limit = { is_ai = no }"]
    for tid, (tree, ids) in enumerate(trees.items()):
        out += ["\t\tif = { limit = { has_focus_tree = %s }" % tree,
                "\t\t\tset_temp_variable = { hoi4live_ft = %d }" % tid,
                "\t\t\tset_temp_variable = { hoi4live_fn = %d }" % len(ids),
                "\t\t\tset_temp_variable = { hoi4live_fc = 0 }",
                "\t\t\tset_temp_variable = { hoi4live_fpc = 0 }",
                "\t\t\tlog = hoi4live_tlm_fb"]
        for index, fid in enumerate(ids):
            out += ["\t\t\tif = { limit = { has_completed_focus = %s }" % fid,
                    "\t\t\t\tset_temp_variable = { hoi4live_fid = %d }" % index,
                    "\t\t\t\tadd_to_temp_variable = { hoi4live_fc = 1 } log = hoi4live_tlm_fc",
                    "\t\t\t}",
                    "\t\t\telse_if = { limit = { focus_progress = { focus = %s progress > 0 } }" % fid,
                    "\t\t\t\tset_temp_variable = { hoi4live_fid = %d }" % index,
                    "\t\t\t\t" + _progress().replace("$FID$", fid),
                    "\t\t\t\tadd_to_temp_variable = { hoi4live_fpc = 1 } log = hoi4live_tlm_fp",
                    "\t\t\t}"]
        out += ["\t\t\tlog = hoi4live_tlm_fe", "\t\t}"]
    return "\n".join(out + ["\t}", "}", ""])


class FocusLedger:
    """Accept a closed batch only with a matching manifest and daily anchor."""
    def __init__(self, days=400):
        self.days = days
        self.trees = {}
        self.generation = None
        self.clear()

    def clear(self):
        self.pending = {}
        self.records = {}

    def configure(self, value, generation):
        trees = catalog(value)
        if trees != self.trees or generation != self.generation:
            self.clear()
        self.trees, self.generation = trees, generation

    def add(self, rec, anchor):
        from .modgen import SCHEMA
        tag, day, tree = rec.get("t"), rec.get("n"), rec.get("tree")
        if (not isinstance(tag, str) or not re.fullmatch(r"[A-Z][A-Z0-9]{2}", tag)
                or type(day) is not int or day < 0):
            return
        key = (tag, day)
        if tag not in {k[0] for k in self.records} | {k[0] for k in self.pending} and len(
                {k[0] for k in self.records} | {k[0] for k in self.pending}) >= 16:
            return
        valid = (anchor and rec.get("g") == self.generation and bool(self.generation)
                 and rec.get("g") == anchor.get("g") and type(rec.get("v")) is int
                 and rec.get("v") == SCHEMA == anchor.get("v") and day == anchor.get("n")
                 and tag == anchor.get("t") and rec.get("_date") == anchor.get("_date")
                 and type(tree) is int and 0 <= tree < len(self.trees))
        if not valid:
            self.pending.pop(key, None)
            self.records.pop(key, None)
            return
        names = list(self.trees)
        ids = self.trees[names[tree]]
        if rec["k"] == "fb":
            # A second batch for one country/day is ambiguous (including a replay).
            if key in self.pending or key in self.records:
                self.pending[key] = None
                self.records.pop(key, None)
                return
            self.pending = {k: v for k, v in self.pending.items() if k[0] != tag}
            if type(rec.get("total")) is not int or rec["total"] != len(ids):
                self.pending[key] = None
                return
            self.pending[key] = {"tree": tree, "date": rec.get("_date"), "completed": {}, "progress": {}}
            for old in [k for k in self.records if k[0] == tag and k[1] <= day - self.days]:
                del self.records[old]
            return
        batch = self.pending.get(key)
        if not batch:
            # Orphan/late records must not leave a previously closed batch trusted.
            self.records.pop(key, None)
            if key in self.pending:
                self.pending[key] = None
            return
        if batch["tree"] != tree or batch["date"] != rec.get("_date"):
            self.pending[key] = None
            return
        if rec["k"] == "fe":
            if (type(rec.get("completed")) is int and type(rec.get("progressed")) is int
                    and rec["completed"] == len(batch["completed"])
                    and rec["progressed"] == len(batch["progress"])):
                self.records[key] = {"tag": tag, "day": day, "tree": names[tree],
                                     "completed": sorted(batch["completed"]),
                                     "progress": copy.deepcopy(batch["progress"]),
                                     "current": None, "current_known": False,
                                     "source": "telemetry", "generation": self.generation,
                                     "scope": "current_tree", "as_of_date": batch["date"]}
            self.pending.pop(key, None)
            return
        fid = rec.get("fid")
        if type(fid) is not int or not 0 <= fid < len(ids):
            self.pending[key] = None
            return
        name = ids[fid]
        if name in batch["completed"] or name in batch["progress"]:
            self.pending[key] = None
        elif rec["k"] == "fc":
            batch["completed"][name] = True
        elif type(rec.get("low")) is int and rec["low"] in range(0, 100, 10):
            batch["progress"][name] = {"lower_exclusive": rec["low"] / 100,
                                       "upper_inclusive": (rec["low"] + 10) / 100}
        else:
            self.pending[key] = None

    def get(self, tag, day):
        if type(day) is not int:
            return None
        return copy.deepcopy(self.records.get((tag, day)))
