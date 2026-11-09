"""National focus trees that actually load, with per-focus detail and a sequential timeline.

    tree(world, tag)          the tree the game gives the tag with the active DLCs (game.loaded_tree),
                              its focuses (shared focuses included) with cost in days
                              (cost x NFocus.FOCUS_POINT_DAYS), prerequisites (OR groups, all
                              needed), mutually exclusive focuses, available / bypass text with the
                              static parts decided, allow_branch (a focus whose allow_branch is
                              statically false does not load), a reward summary and the source
    roots(tree), branch(tree, root, depth)   the outline
    earliest(tree)            earliest end day of each focus if it were taken as soon as possible
                              (a lower bound: one focus runs at a time)
    timeline(world, tag, ids, date)  start/end dates of the given order, one focus at a time, with
                              blocked (prerequisite not done earlier), exclusive, runtime-gated and
                              not-loaded flags

Rules from the national focus modding wiki: each `prerequisite = { focus = A focus = B }` block is an
OR group and every group must hold; `mutually_exclusive` lists focuses that may not be taken
together.
"""
from .script import bare, get, get1, line_of, parse_file, trigger
from .world import add_days, effect_summary, num


def _focus_blocks(world):
    """{tree id: [(focus block, path, line)]} and {shared focus id: (block, path, line)}."""

    def load():
        trees, shared, tree_shared = {}, {}, {}
        for p in world.game.files("common/national_focus"):
            items = parse_file(p)
            for i, (k, v) in enumerate(items):
                if k == "focus_tree" and isinstance(v, list):
                    tid = get1(v, "id")
                    rows = []
                    for j, (fk, fv) in enumerate(v):
                        if fk == "focus" and isinstance(fv, list):
                            rows.append((fv, p, line_of(v, j)))
                    trees[tid] = {"focuses": rows, "path": p, "line": line_of(items, i),
                                  "shared": [s for s in get(v, "shared_focus") if isinstance(s, str)],
                                  "continuous": get1(v, "continuous_focus_position") is not None}
                elif k in ("shared_focus", "joint_focus") and isinstance(v, list):
                    fid = get1(v, "id")
                    if fid:
                        shared[fid] = (v, p, line_of(items, i))
        return trees, shared

    return world.cached("focusblocks", load)


def tree_id(world, tag=None):
    return world.game.loaded_tree(world.ctx_for(tag or world.tag))


def tree(world, tag=None):
    """{"id", "src", "focuses": {id: row}, "hidden": [ids not loaded], "days_per_point"}."""
    tag = tag or world.tag
    return world.cached("focustree:" + tag, lambda: _tree(world, tag))


def _tree(world, tag):
    tid = tree_id(world, tag)
    trees, shared = _focus_blocks(world)
    t = trees.get(tid)
    days, days_fact = world.define("FOCUS_POINT_DAYS", 7)
    out = {"id": tid, "tag": tag, "src": world.src(t["path"], t["line"]) if t else None,
           "mod": world.content.source(t["path"]) if t else None, "focuses": {}, "hidden": [],
           "days_per_point": days_fact, "shared_roots": t["shared"] if t else []}
    if not t:
        return out
    blocks = list(t["focuses"])
    # shared focuses: the named roots plus every shared focus whose prerequisites lead back to one
    want = set(t["shared"])
    if want:
        changed = True
        while changed:
            changed = False
            for fid, (b, p, ln) in shared.items():
                if fid in want:
                    continue
                pre = {x for pb in get(b, "prerequisite") if isinstance(pb, list) for a, x in pb if a == "focus"}
                if pre & want:
                    want.add(fid)
                    changed = True
        for fid in sorted(want):
            if fid in shared:
                blocks.append(shared[fid])
    ctx = world.ctx_for(tag)
    for b, p, ln in blocks:
        row = focus_row(world, b, p, ln, days, ctx, tag)
        if row is None:
            continue
        if row.get("allow_branch_static") is False:
            out["hidden"].append(row["id"])
            continue
        out["focuses"][row["id"]] = row
    for row in out["focuses"].values():
        row["prereq"] = [[x for x in g if x in out["focuses"]] or g for g in row["prereq"]]
    return out


def focus_row(world, b, path, line, days, ctx, tag):
    fid = get1(b, "id")
    if not fid:
        return None
    prereq = []
    for pb in get(b, "prerequisite"):
        if isinstance(pb, list):
            group = [x for a, x in pb if a == "focus"]
            if group:
                prereq.append(group)
    excl = []
    for mb in get(b, "mutually_exclusive"):
        if isinstance(mb, list):
            excl += [x for a, x in mb if a == "focus"]
    cost = num(get1(b, "cost"), 10)
    if not isinstance(cost, (int, float)):
        cost = 10
    row = {"id": fid, "name": world.name(fid), "cost": cost, "days": cost * days,
           "prereq": prereq, "exclusive": sorted(set(excl)),
           "x": num(get1(b, "x"), 0), "y": num(get1(b, "y"), 0),
           "relative_position_id": get1(b, "relative_position_id"),
           "src": world.src(path, line), "mod": world.content.source(path)}
    for key in ("available", "bypass", "allow_branch", "cancel"):
        v = get1(b, key)
        if isinstance(v, list) and v:
            text = world.text(v, 260, tag)
            if text:
                row[key] = text
    ab = get1(b, "allow_branch")
    if isinstance(ab, list):
        row["allow_branch_static"] = trigger(ab, ctx)
    reward = get1(b, "completion_reward")
    if isinstance(reward, list):
        row["reward"] = effect_summary(reward, 10)
    war = get1(b, "will_lead_to_war_with")
    if war:
        row["war_with"] = war
    filters = get1(b, "search_filters")
    if isinstance(filters, list):
        row["filters"] = [f.replace("FOCUS_FILTER_", "").lower() for f in bare(filters)]
    for flag in ("cancel_if_invalid", "continue_if_invalid", "available_if_capitulated"):
        if get1(b, flag) is not None:
            row[flag] = get1(b, flag) == "yes"
    return row


def roots(t):
    return [fid for fid, r in t["focuses"].items() if not r["prereq"]]


def children(t):
    out = {fid: [] for fid in t["focuses"]}
    for fid, r in t["focuses"].items():
        for g in r["prereq"]:
            for p in g:
                if p in out and fid not in out[p]:
                    out[p].append(fid)
    return out


def branch(t, root, depth=None):
    """[(level, focus id)] depth-first from root (each focus once)."""
    kids = children(t)
    out, seen = [], set()

    def walk(fid, level):
        if fid in seen or (depth is not None and level > depth):
            return
        seen.add(fid)
        out.append((level, fid))
        for c in sorted(kids.get(fid, []), key=lambda c: (t["focuses"][c]["x"], c)):
            walk(c, level + 1)

    walk(root, 0)
    return out


def branch_of(t):
    """{focus: [root ids it descends from]}."""
    out = {}
    for r in roots(t):
        for _, fid in branch(t, r):
            out.setdefault(fid, []).append(r)
    return out


def earliest(t):
    """{focus: earliest end in days from the start} if every focus could run at once (lower bound)."""
    memo = {}

    def end(fid, stack=()):
        if fid in memo:
            return memo[fid]
        r = t["focuses"].get(fid)
        if r is None or fid in stack:
            return float("inf")
        start = 0
        for g in r["prereq"]:
            start = max(start, min((end(p, stack + (fid,)) for p in g), default=0))
        memo[fid] = start + r["days"]
        return memo[fid]

    for fid in t["focuses"]:
        end(fid)
    return memo


def timeline(world, tag, ids, date=None, done=()):
    """One focus at a time in the given order from `date`: [{id, start, end, days, flags}]."""
    t = tree(world, tag)
    date = date or world.date
    done = set(done) | {f["v"] for f in world.country(tag)["completed_focuses"]}
    taken = set()
    rows = []
    for fid in ids:
        r = t["focuses"].get(fid)
        row = {"id": fid}
        if r is None:
            row["flags"] = ["not loaded" if fid in t["hidden"] else "unknown focus"]
            rows.append(row)
            continue
        flags = []
        have = done | taken
        missing = [g for g in r["prereq"] if not (set(g) & have)]
        if missing:
            flags.append("blocked: needs " + "; ".join(" or ".join(g) for g in missing))
        clash = sorted(set(r["exclusive"]) & have)
        if clash:
            flags.append("exclusive with " + ", ".join(clash))
        if r.get("available"):
            flags.append("runtime: " + r["available"][:120])
        end = add_days(date, r["days"])
        row.update({"name": r["name"], "start": date, "end": end, "days": r["days"], "flags": flags,
                    "d": "sequential; cost x FOCUS_POINT_DAYS, no focus speed modifiers"})
        rows.append(row)
        taken.add(fid)
        date = end
    return rows
