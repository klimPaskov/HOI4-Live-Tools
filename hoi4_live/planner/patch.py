"""The update format: a patch of operations on the plan, split into detail keys and strategy keys.

    {"summary": "one line", "reason": "why", "escalated": false,
     "ops": [{"op": "extend", "key": "research_path", "items": [{"year": 1936, "techs": [...], "why": "..."}]},
             {"op": "replace_tail", "key": "focus_path", "items": ["ITA_...", ...]},
             {"op": "set", "key": "politics_auto.order", "value": [...]},
             {"op": "add", "key": "production", "item": {"id": "u3_art", "equipment": "...", "factories": 3}},
             {"op": "remove", "key": "construction_plan", "id": "cp04"}]}

extend        append list items not already listed (strings by value, objects by id)
replace_tail  keep the list's items the state shows done (or running), replace the rest
set           set a key or a dotted sub-key to a value
add           append one object; it gets an id when it has none
remove        drop one not-yet-done object by id

Detail keys change freely. Strategy keys (the goal, phases, checkpoints, horizons, contingencies, the war's
enemies, axes and theatre, the army layout, peace, diplomacy, factions) change only in an escalated update
that states its reason. Any other key is refused, so an update can never touch the run's own settings.
"""

import copy

DETAIL_KEYS = {
    "research_path", "research_priority", "research_fallback", "research_fallback_prefixes",
    "research_fallback_categories", "research_fallback_exclude", "research_bonuses",
    "focus_path", "focus_fallback",
    "politics", "politics_auto", "decisions", "decision_rules", "event_rules", "event_options",
    "production", "production_priority", "production_min_idle", "stockpile_targets", "auto_import",
    "training", "training_rules", "disband", "templates", "designs", "mio", "trade", "doctrines",
    "construction", "construction_plan", "construction_min_idle", "construction_queue_depth",
    "construction_fallback_any_own_state",
    "air_plan", "navy", "manage", "alarms", "occupation", "vp_targets", "hold_targets", "intelligence",
    "war",  # its sub-keys other than the strategy ones below
}
STRATEGY_KEYS = {"goal", "phases", "checkpoints", "horizons", "contingencies", "force_posture", "army_layout",
                 "army_group", "armies", "war_by_phase", "peace", "diplomacy", "faction", "invasions",
                 "war.enemies", "war.axes", "war.theatre", "war.guard_states"}
OPS = ("extend", "replace_tail", "set", "add", "remove")


class PatchError(ValueError):
    pass


def _root(key):
    return key.split(".")[0]


def is_strategy(key):
    return key in STRATEGY_KEYS or _root(key) in STRATEGY_KEYS


def shape_errors(patch):
    """Why the patch's shape is refused ([] when it is fine)."""
    if not isinstance(patch, dict) or not isinstance(patch.get("ops"), list):
        return ["a patch is an object with an ops list"]
    out = []
    escalated = bool(patch.get("escalated"))
    for i, op in enumerate(patch["ops"]):
        if not isinstance(op, dict) or op.get("op") not in OPS or not isinstance(op.get("key"), str):
            out.append("ops[%d]: needs op (%s) and key" % (i, "|".join(OPS)))
            continue
        key = op["key"]
        if key == "war" and op["op"] == "set":
            out.append("ops[%d]: war is changed by its sub-keys (war.supply_floor, ...), never as a whole" % i)
        elif is_strategy(key):
            if not escalated or not str(patch.get("reason") or "").strip():
                out.append("ops[%d]: %s is a strategy key: only an escalated update with a reason may change it"
                           % (i, key))
        elif _root(key) not in DETAIL_KEYS:
            out.append("ops[%d]: %s is not a key an update may change" % (i, key))
        if op["op"] in ("extend", "replace_tail") and not isinstance(op.get("items"), list):
            out.append("ops[%d]: %s needs items (a list)" % (i, op["op"]))
        if op["op"] == "add" and not isinstance(op.get("item"), dict):
            out.append("ops[%d]: add needs item (an object)" % i)
        if op["op"] == "remove" and not op.get("id"):
            out.append("ops[%d]: remove needs id" % i)
        if op["op"] == "set" and "value" not in op:
            out.append("ops[%d]: set needs value" % i)
    return out


def _get(plan, dotted, create=False):
    """(container, last key) for a dotted key."""
    parts = dotted.split(".")
    node = plan
    for p in parts[:-1]:
        if not isinstance(node.get(p), dict):
            if not create:
                return None, parts[-1]
            node[p] = {}
        node = node[p]
    return node, parts[-1]


def _ident(item):
    if isinstance(item, dict):
        return item.get("id") or (tuple(item.get("techs") or ()) if "techs" in item else None)
    return item


def _done(item, done):
    if isinstance(item, str):
        return item in done
    if isinstance(item, dict):
        if item.get("id") in done:
            return True
        techs = item.get("techs")
        return isinstance(techs, list) and bool(techs) and all(t in done for t in techs)
    return False


def apply(plan, patch, done=(), new_id=None):
    """A new plan with the patch's operations applied; raises PatchError with every reason it cannot."""
    errors = shape_errors(patch)
    if errors:
        raise PatchError("; ".join(errors))
    out = copy.deepcopy(plan)
    done = set(done)
    counter = [0]

    def next_id(key):
        counter[0] += 1
        return new_id(key, counter[0]) if new_id else "%s_u%d" % (key.split(".")[-1], counter[0])

    for i, op in enumerate(patch["ops"]):
        node, last = _get(out, op["key"], create=op["op"] in ("set", "add", "extend"))
        if node is None:
            raise PatchError("ops[%d]: %s has no parent object in the plan" % (i, op["key"]))
        kind = op["op"]
        if kind == "set":
            node[last] = copy.deepcopy(op["value"])
            continue
        current = node.get(last)
        if current is None:
            current = node[last] = []
        if not isinstance(current, list):
            raise PatchError("ops[%d]: %s is not a list" % (i, op["key"]))
        if kind == "extend":
            have = {_ident(x) for x in current}
            for item in op["items"]:
                if _ident(item) not in have or _ident(item) is None:
                    current.append(copy.deepcopy(item))
                    have.add(_ident(item))
        elif kind == "replace_tail":
            kept = [x for x in current if _done(x, done)]
            have = {_ident(x) for x in kept}
            node[last] = kept + [copy.deepcopy(x) for x in op["items"] if _ident(x) not in have]
        elif kind == "add":
            item = copy.deepcopy(op["item"])
            item.setdefault("id", next_id(op["key"]))
            if any(isinstance(x, dict) and x.get("id") == item["id"] for x in current):
                raise PatchError("ops[%d]: id %s is already in %s" % (i, item["id"], op["key"]))
            current.append(item)
        elif kind == "remove":
            hit = [x for x in current if isinstance(x, dict) and x.get("id") == op["id"]]
            if not hit:
                raise PatchError("ops[%d]: no %s item with id %s" % (i, op["key"], op["id"]))
            if _done(hit[0], done):
                raise PatchError("ops[%d]: %s is done; a done item stays in the plan" % (i, op["id"]))
            current.remove(hit[0])
    return out


def touched(patch):
    """The top-level keys a patch changes."""
    return sorted({_root(op["key"]) for op in patch.get("ops") or [] if isinstance(op, dict) and op.get("key")})


def merge(patches, summary=None):
    """One patch from several (the triggers of one update), in order."""
    ops, reasons, escalated = [], [], False
    for p in patches:
        if p:
            ops += p.get("ops") or []
            if p.get("reason"):
                reasons.append(p["reason"])
            escalated = escalated or bool(p.get("escalated"))
    return {"ops": ops, "reason": "; ".join(reasons), "escalated": escalated,
            "summary": summary or "; ".join(p.get("summary", "") for p in patches if p and p.get("summary"))}
