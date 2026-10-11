"""National focus trees from common/national_focus (game + mods): the trees, their focuses with absolute grid
positions, costs, prerequisites, mutual exclusions, conditions and rewards, plus a planner that says which focuses
can be taken given the completed ones.

Rules (National focus modding wiki, "Position", "Interaction with other focuses", "Cost", "Shared focuses"):
    position     x, y in tree units, relative to relative_position_id's absolute position when given;
                 offset = { x y trigger } blocks apply only when their trigger holds (reported, not applied)
    prerequisite each prerequisite = { focus = A focus = B } block is an OR group; all groups must hold (AND)
    mutually_exclusive  none of the listed focuses may be completed (or in progress)
    cost         weeks: 1 cost = 7 days at the default focus speed
    shared_focus a tree's `shared_focus = X` adds the shared focus X and every shared focus whose prerequisites
                 lead back to it
On screen a tree unit is 96 x 130 pixels at zoom 1 (nationalfocusview.gui), which hoi4_focus uses as a fallback
when the search box cannot find a focus.
"""

import re

from . import scriptdb
from .scriptdb import as_list, num, render

DAYS_PER_COST = 7


def _focus_entry(f, shared=False):
    fid = f.get("id")
    if not isinstance(fid, str):
        return None
    prereq = []
    for p in as_list(f.get("prerequisite")):
        if isinstance(p, dict):
            group = [x for x in as_list(p.get("focus")) if isinstance(x, str)]
            if group:
                prereq.append(group)
    mutex = []
    for m in as_list(f.get("mutually_exclusive")):
        if isinstance(m, dict):
            mutex += [x for x in as_list(m.get("focus")) if isinstance(x, str)]
    offsets = []
    for o in as_list(f.get("offset")):
        if isinstance(o, dict):
            offsets.append({"x": num(o.get("x"), 0), "y": num(o.get("y"), 0),
                            "trigger": render(o.get("trigger") or {}, 160)})
    icon = f.get("icon")
    if isinstance(icon, dict):                  # dynamic icons: icon = { trigger value }
        icon = icon.get("value")
    elif isinstance(icon, list):
        icon = next((i if isinstance(i, str) else i.get("value") for i in icon
                     if isinstance(i, str) or isinstance(i, dict)), None)
    filters = f.get("search_filters")
    out = {"id": fid, "x": num(f.get("x"), 0), "y": num(f.get("y"), 0),
           "rel": f.get("relative_position_id") if isinstance(f.get("relative_position_id"), str) else None,
           "cost": num(f.get("cost"), 10), "prereq": prereq, "mutex": sorted(set(mutex)),
           "icon": icon if isinstance(icon, str) else None,
           "available": render(f["available"], 500) if isinstance(f.get("available"), dict) else None,
           "bypass": render(f["bypass"], 300) if isinstance(f.get("bypass"), dict) else None,
           "allow_branch": render(f["allow_branch"], 200) if isinstance(f.get("allow_branch"), dict) else None,
           "reward": render(f["completion_reward"], 700) if isinstance(f.get("completion_reward"), dict) else None,
           "filters": [x for x in as_list(filters) if isinstance(x, str)] if not isinstance(filters, dict) else
           [x for x in as_list(filters.get("_items")) if isinstance(x, str)],
           "war_with": f.get("will_lead_to_war_with") if isinstance(f.get("will_lead_to_war_with"), str) else None,
           "continue_if_invalid": f.get("continue_if_invalid") == "yes",
           "cancel_if_invalid": f.get("cancel_if_invalid") != "no",
           "available_if_capitulated": f.get("available_if_capitulated") == "yes"}
    if offsets:
        out["offsets"] = offsets
    if shared:
        out["shared"] = True
    return out


def _country_rules(c):
    """The tree's `country = { factor modifier = { add/factor tag/original_tag } }` as [(kind, value, tags)]."""
    rules = []
    if not isinstance(c, dict):
        return {"base": 0, "rules": rules}
    for m in as_list(c.get("modifier")):
        if not isinstance(m, dict):
            continue
        tags = [t for t in as_list(m.get("tag")) + as_list(m.get("original_tag")) if isinstance(t, str)]
        other = [k for k in m if k not in ("add", "factor", "tag", "original_tag", "has_dlc", "_repeated")]
        kind = "add" if "add" in m else "factor"
        rules.append({"kind": kind, "value": num(m.get(kind), 0), "tags": tags, "other": other})
    return {"base": num(c.get("factor"), 0), "rules": rules}


def build_index(paths):
    trees, shared = {}, {}
    for p in paths:
        try:
            block = scriptdb.parse_file(p)
        except Exception:                               # a broken mod file must not break the index
            continue
        if not isinstance(block, dict):
            continue
        for t in as_list(block.get("focus_tree")):
            if not isinstance(t, dict) or not isinstance(t.get("id"), str):
                continue
            focuses = {}
            for f in as_list(t.get("focus")):
                if isinstance(f, dict):
                    e = _focus_entry(f)
                    if e:
                        focuses[e["id"]] = e
            isp = t.get("initial_show_position") or {}
            trees[t["id"]] = {"id": t["id"], "file": p, "default": t.get("default") == "yes",
                              "country": _country_rules(t.get("country")),
                              "shared": [x for x in as_list(t.get("shared_focus")) if isinstance(x, str)],
                              "initial": {"focus": isp.get("focus"), "x": num(isp.get("x")), "y": num(isp.get("y"))}
                              if isinstance(isp, dict) else {},
                              "focuses": focuses}
        for key in ("shared_focus", "joint_focus"):
            for f in as_list(block.get(key)):
                if isinstance(f, dict):
                    e = _focus_entry(f, shared=True)
                    if e:
                        e["file"] = p
                        if key == "joint_focus":
                            e["joint"] = True
                        shared[e["id"]] = e
    return {"trees": trees, "shared": shared}


def index(game_dir, mods):
    paths = scriptdb.effective_files("common/national_focus", game_dir, mods)
    return scriptdb.cached_index("focus", paths, build_index)


def for_session(session):
    return scriptdb.memo(session, "focus", index)


def tree_score(tree, tag, original_tag=None):
    """The tree's weight for a country from its simple tag rules (other conditions are ignored)."""
    c = tree.get("country") or {}
    score = c.get("base", 0)
    hit = False
    for r in c.get("rules", []):
        if not r["tags"] or r["other"]:
            continue
        if tag in r["tags"] or (original_tag and original_tag in r["tags"]):
            hit = True
            score = score + r["value"] if r["kind"] == "add" else score * r["value"]
    return score, hit


def tree_for_tag(idx, tag, original_tag=None):
    """Best tree id for a country without a save: the highest tag-rule score, else the default tree."""
    best = None
    for tid, t in idx["trees"].items():
        score, hit = tree_score(t, tag, original_tag)
        if hit and (best is None or score > best[0]):
            best = (score, tid)
    if best:
        return best[1]
    for tid, t in idx["trees"].items():
        if t.get("default"):
            return tid
    return None


def tree_focuses(idx, tree_id):
    """{id: focus} of a tree including its shared focuses (and the shared focuses that follow from them)."""
    t = idx["trees"].get(tree_id)
    if not t:
        return {}
    out = dict(t["focuses"])
    shared = idx.get("shared", {})
    todo = [s for s in t.get("shared", []) if s in shared]
    added = set()
    while todo:
        s = todo.pop()
        if s in added:
            continue
        added.add(s)
        out[s] = shared[s]
        for sid, sf in shared.items():
            if sid not in added and any(s in g for g in sf["prereq"]):
                todo.append(sid)
    return out


def positions(focuses):
    """{id: (x, y)} absolute grid positions through relative_position_id chains (conditional offsets ignored)."""
    pos = {}

    def resolve(fid, seen):
        if fid in pos:
            return pos[fid]
        f = focuses.get(fid)
        if f is None:
            return (0, 0)
        x, y = f["x"], f["y"]
        rel = f.get("rel")
        if rel and rel in focuses and rel not in seen:
            rx, ry = resolve(rel, seen | {fid})
            x, y = x + rx, y + ry
        pos[fid] = (x, y)
        return pos[fid]

    for fid in focuses:
        resolve(fid, frozenset())
    return pos


def status(f, completed, current=None):
    """'completed', 'current', 'available' (prerequisites met, no exclusive taken), 'locked' (a prerequisite group
    is unmet) or 'excluded' (a mutually exclusive focus is completed or in progress). Script conditions
    (available, allow_branch) are not evaluated here: the tree view and the tooltip decide those."""
    done = set(completed or ())
    if f["id"] in done:
        return "completed"
    if current and f["id"] == current:
        return "current"
    if any(m in done or m == current for m in f.get("mutex", ())):
        return "excluded"
    for group in f.get("prereq", ()):
        if not any(g in done for g in group):
            return "locked"
    return "available"


def plan(focuses, completed, current=None, names=None):
    """Rows for every focus with its status, sorted: available first (by tree position), then current, locked,
    excluded, completed."""
    names = names or {}
    pos = positions(focuses)
    order = {"available": 0, "current": 1, "locked": 2, "excluded": 3, "completed": 4}
    rows = []
    for fid, f in focuses.items():
        st = status(f, completed, current)
        x, y = pos.get(fid, (f["x"], f["y"]))
        rows.append({"id": fid, "name": names.get(fid, fid), "status": st, "x": x, "y": y,
                     "cost": f["cost"], "days": int(f["cost"] * DAYS_PER_COST),
                     "prerequisites": f.get("prereq", []), "mutually_exclusive": f.get("mutex", []),
                     "requires": f.get("available"), "bypass": f.get("bypass"), "branch": f.get("allow_branch"),
                     "effects": f.get("reward"), "filters": f.get("filters", []),
                     "war_with": f.get("war_with")})
    rows.sort(key=lambda r: (order[r["status"]], r["y"], r["x"]))
    return rows


def unmet(f, completed):
    """Human-readable reasons a focus cannot be taken from the files' point of view."""
    done = set(completed or ())
    out = []
    for group in f.get("prereq", ()):
        if not any(g in done for g in group):
            out.append("needs one of: " + ", ".join(group))
    taken = [m for m in f.get("mutex", ()) if m in done]
    if taken:
        out.append("mutually exclusive with completed: " + ", ".join(taken))
    return out


def match_name(rows, query):
    """Find the focus a query means: an id, an exact localised name, or a unique substring of a name."""
    q = query.strip().lower()
    for r in rows:
        if r["id"].lower() == q:
            return r, []
    exact = [r for r in rows if (r.get("name") or "").lower() == q]
    if exact:
        return exact[0], exact[1:]
    norm = lambda s: re.sub(r"[^a-z0-9]+", "", (s or "").lower())
    nq = norm(q)
    part = [r for r in rows if nq and (nq in norm(r.get("name")) or nq in norm(r["id"]))]
    if len(part) == 1:
        return part[0], []
    return None, part
