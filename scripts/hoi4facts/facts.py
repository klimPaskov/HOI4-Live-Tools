#!/usr/bin/env python3
"""Facts CLI for the strategist: deterministic answers from the effective game files, with sources.

    facts.py dossier build --tag T --date D [--dlc auto|"A,B"] [--mods auto|none] --out DIR
    facts.py dossier show <section 0-9 or name> [--out DIR]
    facts.py focus <id>
    facts.py focus-tree <TAG> [--branch id] [--depth N]
    facts.py focus-timeline <TAG> --date D <ids...>
    facts.py tech <id>
    facts.py research-calendar <TAG> --date D --years 1936-1938
    facts.py research-timeline PLAN.json [--rule all|any]
    facts.py state <id>          facts.py states <TAG>
    facts.py province <id>       facts.py route <from> <to> [--avoid-rivers]
    facts.py vp <TAG>            facts.py oob <TAG>
    facts.py characters <TAG> [--role general|marshal|admiral|advisor|leader]
    facts.py laws <TAG>          facts.py idea <id>
    facts.py decisions <TAG>     facts.py missions <TAG>
    facts.py hooks <TAG>         facts.py event <id>
    facts.py grep <token>        facts.py defines <KEY>

Global options (before or after the command): --game DIR, --userdir DIR, --mod PATH (repeat),
--date D, --mode fair|meta, --json, --tag T (our country, for DLC-aware and FAIR answers), --dlc.

Content = the install plus the user folder's enabled mods (dlc_load.json) plus every --mod. Without
--dlc the active DLCs come from the run's dossier manifest, else the user folder's logs/system.log,
else the installed DLC folders. The run's dossier is found from --out, else $HOI4_LIVE_RUN/dossier,
else ./dossier, else the current directory; its manifest also supplies the tag, date and mode
defaults. Plain compact text by default; --json for machines. Stdlib only, Python 3.9+.
"""
import argparse
import json
import os
import sys

if __package__ in (None, ""):
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    __package__ = "hoi4facts"  # noqa: A001

from hoi4facts import dossier as dossiermod  # noqa: E402
from hoi4facts import focus as focusmod  # noqa: E402
from hoi4facts import mapgraph  # noqa: E402
from hoi4facts import research as researchmod  # noqa: E402
from hoi4facts import scenario as scenariomod  # noqa: E402
from hoi4facts.content import default_userdir, mod_from_arg, mods_from_userdir  # noqa: E402
from hoi4facts.game import Game  # noqa: E402
from hoi4facts.world import World  # noqa: E402

GLOBALS = ("game", "userdir", "mod", "date", "mode", "json", "tag", "dlc", "out")


# ---------------------------------------------------------------- setup

def dossier_dir(out=None):
    """The run's dossier folder: --out, $HOI4_LIVE_RUN/dossier, ./dossier, or the current directory."""
    cands = []
    if out:
        cands.append(out)
    run = os.environ.get("HOI4_LIVE_RUN")
    if run:
        cands += [os.path.join(run, "dossier"), run]
    cands += [os.path.join(os.getcwd(), "dossier"), os.getcwd()]
    for c in cands:
        if os.path.isfile(os.path.join(c, "dossier.md")) or os.path.isfile(os.path.join(c, "manifest.json")):
            return c
    return out


def load_manifest(out=None):
    d = dossier_dir(out)
    if not d:
        return {}
    try:
        with open(os.path.join(d, "manifest.json"), encoding="utf-8") as fh:
            return json.load(fh)
    except (OSError, ValueError):
        return {}


def resolve(args):
    """(game_dir, mods, dlcs, tag, date, mode) from the options, the manifest and the defaults."""
    man = load_manifest(getattr(args, "out", None))
    game_dir = args.game or os.environ.get("HOI4_DIR") or mapgraph.DEFAULT_GAME_DIR
    userdir = args.userdir or default_userdir()
    mods_mode = getattr(args, "mods", None) or "auto"
    mods = mods_from_userdir(userdir) if (userdir and mods_mode != "none") else []
    for m in args.mod or []:
        mod = mod_from_arg(m, userdir)
        if mod:
            mods.append(mod)
    if args.dlc and args.dlc != "auto":
        dlcs = split_dlcs(args.dlc, game_dir, userdir)
    elif man.get("dlcs") is not None and not args.dlc:
        dlcs = man["dlcs"]
    else:
        dlcs, _ = dossiermod.dlcs_auto(game_dir, userdir)
    tag = args.tag or man.get("tag") or ""
    date = args.date or man.get("date") or "1936.1.1"
    mode = args.mode or man.get("mode") or "fair"
    return game_dir, mods, dlcs, tag, date, mode, userdir


def split_dlcs(text, game_dir=None, userdir=None):
    """A --dlc list: ";" or "|" separated, or "," separated with names that contain a comma
    ("No Compromise, No Surrender") rejoined against the installed DLC names."""
    for sep in (";", "|"):
        if sep in text:
            return [x.strip() for x in text.split(sep) if x.strip()]
    known = set(dossiermod.dlcs_auto(game_dir, None)[0]) if game_dir else set()
    parts = [x.strip() for x in text.split(",") if x.strip()]
    out, i = [], 0
    while i < len(parts):
        if i + 1 < len(parts) and f"{parts[i]}, {parts[i + 1]}" in known:
            out.append(f"{parts[i]}, {parts[i + 1]}")
            i += 2
        else:
            out.append(parts[i])
            i += 1
    return out


def world(args, tag=None):
    game_dir, mods, dlcs, our, date, mode, _ = resolve(args)
    return World(Game(game_dir, mods), tag or our or "", date, dlcs, mode)


# ---------------------------------------------------------------- output

def emit(args, data, text_fn=None):
    if args.json or text_fn is None:
        print(json.dumps(data, ensure_ascii=False, indent=1, default=lambda o: sorted(o) if isinstance(o, set) else str(o)))
    else:
        out = text_fn(data)
        print(out if isinstance(out, str) else "\n".join(out))
    return 0


def fail(msg):
    print(json.dumps({"error": msg}))
    return 1


def kv(row, keys):
    return " ".join(f"{k}={row[k]}" for k in keys if row.get(k) not in (None, "", [], {}))


# ---------------------------------------------------------------- commands

def cmd_dossier(args):
    if args.action == "build":
        if not args.tag or not args.date:
            return fail("dossier build needs --tag and --date")
        game_dir, mods, dlcs, _, _, mode, userdir = resolve(args)
        if args.dlc in (None, "auto"):
            dlcs, src = dossiermod.dlcs_auto(game_dir, userdir)
        else:
            src = "--dlc"
        game = Game(game_dir, mods)
        w = World(game, args.tag, args.date, dlcs, mode)
        d = dossiermod.build_world(w, enemies=args.enemy or None, dlc_src=src)
        out = args.out or os.path.join(os.environ.get("HOI4_LIVE_RUN") or ".", "dossier")
        r = dossiermod.write(d, out)
        return emit(args, {"dir": r["dir"], "md_bytes": r["md_bytes"], "md_tokens_estimate": r["md_bytes"] // 4,
                           "content_hash": d["_meta"]["content_hash"], "dlcs": dlcs, "mods": [m.name for m in mods]},
                    lambda x: f"dossier written to {x['dir']}: dossier.md {x['md_bytes']} bytes "
                              f"(~{x['md_tokens_estimate']} tokens), hash {x['content_hash']}")
    d = dossier_dir(args.out)
    if not d or not os.path.isfile(os.path.join(d, "dossier.md")):
        return fail("no dossier found (use --out DIR, set HOI4_LIVE_RUN, or run dossier build)")
    if args.section is None:
        with open(os.path.join(d, "dossier.md"), encoding="utf-8") as fh:
            print(fh.read())
        return 0
    if args.json:
        key = None
        for num, k, title, _ in dossiermod.SECTIONS:
            if args.section in (str(num), k):
                key = k
        with open(os.path.join(d, "dossier.json"), encoding="utf-8") as fh:
            data = json.load(fh)
        part = {k: data.get(k) for k in (["neighbours", "enemies"] if key == "neighbours" else [key])}
        return emit(args, part)
    with open(os.path.join(d, "dossier.md"), encoding="utf-8") as fh:
        text = dossiermod.section_text(fh.read(), args.section)
    if text is None:
        return fail(f"unknown section {args.section!r} (0-9 or " +
                    ", ".join(k for _, k, _, _ in dossiermod.SECTIONS) + ")")
    print(text)
    return 0


def _find_focus(w, fid):
    trees, shared = focusmod._focus_blocks(w)
    for tid, t in trees.items():
        for b, p, ln in t["focuses"]:
            if focusmod.get1(b, "id") == fid:
                return tid, b, p, ln
    if fid in shared:
        b, p, ln = shared[fid]
        return "shared", b, p, ln
    return None


def cmd_focus(args):
    w = world(args)
    hit = _find_focus(w, args.id)
    if not hit:
        return fail(f"unknown focus {args.id}")
    tid, b, p, ln = hit
    days, _ = w.define("FOCUS_POINT_DAYS", 7)
    row = focusmod.focus_row(w, b, p, ln, days, w.ctx, w.tag)
    row["tree"] = tid
    if w.tag and tid != "shared":
        t = focusmod.tree(w, w.tag)
        row["loaded_for_tag"] = args.id in t["focuses"]
        row["unlocks"] = focusmod.children(t).get(args.id, []) if args.id in t["focuses"] else []
    # events fired anywhere in the reward
    row["events"] = [x.split(":", 1)[1] for x in row.get("reward", []) if x.startswith("event:")]

    def text(r):
        lines = [f"{r['id']} \"{r['name']}\" tree {r['tree']} cost {r['cost']:g} = {r['days']:g} days [{r['src']}]"]
        lines.append("needs: " + (" AND ".join("(" + " OR ".join(g) + ")" for g in r["prereq"]) or "-"))
        if r["exclusive"]:
            lines.append("exclusive: " + ", ".join(r["exclusive"]))
        for k in ("available", "bypass", "allow_branch", "cancel", "war_with"):
            if r.get(k):
                lines.append(f"{k}: {r[k]}")
        if r.get("reward"):
            lines.append("reward: " + ", ".join(r["reward"]))
        if r.get("unlocks"):
            lines.append("unlocks: " + ", ".join(r["unlocks"]))
        if "loaded_for_tag" in r:
            lines.append(f"loaded for {w.tag}: {r['loaded_for_tag']}")
        return lines

    return emit(args, row, text)


def cmd_focus_tree(args):
    w = world(args, args.TAG)
    t = focusmod.tree(w, args.TAG)
    if not t["focuses"]:
        return fail(f"no focus tree loads for {args.TAG}")
    roots = [args.branch] if args.branch else focusmod.roots(t)
    rows = []
    for r in roots:
        if r not in t["focuses"]:
            return fail(f"{r} is not a focus of {t['id']}")
        for level, fid in focusmod.branch(t, r, args.depth):
            f = t["focuses"][fid]
            rows.append({"level": level, "id": fid, "name": f["name"], "cost": f["cost"], "days": f["days"],
                         "prereq": f["prereq"], "exclusive": f["exclusive"], "available": f.get("available"),
                         "reward": f.get("reward", [])[:4], "src": f["src"]})
    data = {"tree": t["id"], "src": t["src"], "count": len(t["focuses"]), "hidden": t["hidden"], "rows": rows}

    def text(x):
        out = [f"tree {x['tree']} [{x['src']}] {x['count']} focuses" + (f", hidden {x['hidden']}" if x["hidden"] else "")]
        for r in x["rows"]:
            out.append("  " * r["level"] + f"{r['id']} {r['cost']:g}/{r['days']:g}d"
                       + (f" excl {','.join(r['exclusive'])}" if r["exclusive"] else "")
                       + (f" | {', '.join(r['reward'])}" if r["reward"] else ""))
        return out

    return emit(args, data, text)


def cmd_focus_timeline(args):
    w = world(args, args.TAG)
    rows = focusmod.timeline(w, args.TAG, args.ids, w.date)

    def text(rs):
        return [f"{r.get('start', '?'):>10} -> {r.get('end', '?'):<10} {r['id']} {r.get('days', '')}d"
                + (" | " + "; ".join(r["flags"]) if r.get("flags") else "") for r in rs]

    return emit(args, rows, text)


def cmd_tech(args):
    w = world(args)
    ts = researchmod.techs(w, all_folders=True)
    t = ts.get(args.id)
    if not t:
        return fail(f"unknown technology {args.id}")
    avail = researchmod.techs(w)
    done = researchmod.start_techs(w) if w.tag else set()
    row = dict(t, name=w.name(args.id), available_with_dlcs=args.id in avail,
               days=researchmod.days(w, args.id, f"{t['year']}.1.1" if t["year"] else None, t),
               days_at_start=researchmod.days(w, args.id, w.date, t), d=researchmod.DAYS_METHOD,
               children=sorted(k for k, x in ts.items() if args.id in x["parents"]))
    if w.tag:
        row["researched_at_start"] = args.id in done
        row["chain_all"] = researchmod.chain(w, args.id, done, "all")
        row["chain_any"] = researchmod.chain(w, args.id, done, "any")

    def text(r):
        out = [f"{r['id']} \"{r['name']}\" year {r['year']} cost {r['cost']:g} = {r['days']}d "
               f"({r['days_at_start']}d at {w.date}) folders {','.join(r['folders'])} [{r['src']}]",
               "parents: " + (", ".join(r["parents"]) or "-") + "; deps: " + (", ".join(r["deps"]) or "-"),
               "leads to: " + (", ".join(r["children"]) or "-")]
        if r["xor"]:
            out.append("xor: " + ", ".join(r["xor"]))
        if r.get("allow"):
            out.append("allow: " + r["allow"])
        out.append(f"available with these DLCs: {r['available_with_dlcs']}")
        if "chain_all" in r:
            out.append(f"{w.tag}: researched at start {r['researched_at_start']}; chain (all parents) "
                       f"{r['chain_all'] or 'none'}; chain (any parent) {r['chain_any'] or 'none'}")
        return out

    return emit(args, row, text)


def _years(text, default):
    if not text:
        return default
    a, _, b = text.partition("-")
    return int(a), int(b or a)


def cmd_research_calendar(args):
    w = world(args, args.TAG)
    y0 = int(w.date.split(".")[0])
    cal = researchmod.calendar(w, args.TAG, w.date, _years(args.years, (y0, y0 + 2)))

    def text(c):
        out = []
        for y in c:
            out.append(f"{y['year']} ({len(y['techs'])})")
            for t in y["techs"]:
                out.append(f"  {t['id']} {t['folder']} {t['cost']:g}={t['days']:g}d "
                           + ("now" if t["startable"] else "after " + ",".join(t["chain_all"]))
                           + (f" | any: after {','.join(t['chain_any']) or 'nothing'}" if "chain_any" in t else ""))
        return out

    return emit(args, cal, text)


def cmd_research_timeline(args):
    plan = researchmod.load_plan(args.plan)
    meta = plan.get("_validator") or {}
    if meta.get("dlcs") and not args.dlc:
        args.dlc = ",".join(meta["dlcs"])
    if meta.get("start_date") and not args.date:
        args.date = meta["start_date"]
    w = world(args, meta.get("tag"))
    r = researchmod.timeline(w, plan, rule=args.rule)

    def text(x):
        out = [f"{x['slots']} slots, rule {x['rule']}; {x['d']}"]
        for row in x["rows"]:
            out.append(f"slot {row['slot']} {row['start']:>10} -> {row['end']:<10} {row['id']} {row['days']:g}d"
                       + (f" ahead {row['ahead_years']}y" if row["ahead_years"] else ""))
        for b in x["blocked"]:
            out.append(f"BLOCKED {b['id']}: missing {', '.join(b['missing'])}")
        for u in x["unknown"]:
            out.append(f"UNKNOWN {u}")
        return out

    return emit(args, r, text)


def cmd_state(args):
    w = world(args)
    st = w.states().get(args.id)
    if not st:
        return fail(f"unknown state {args.id}")
    nb = w.state_borders().get(args.id, {})
    row = dict(st, neighbours=[{"state": s, "name": w.states()[s]["name"], "owner": w.states()[s]["owner"],
                                "pairs": len(p)} for s, p in sorted(nb.items()) if s in w.states()],
               vp_src=None)
    row["vp_sources"] = {str(k): v for k, v in st["vp_src"].items()}
    row.pop("vp_src", None)

    def text(r):
        return [f"{r['id']} {r['name']} owner {r['owner']} controller {r['controller']} cores {','.join(r['cores'])}"
                f" [{r['file']}]",
                f"category {r['category']} slots {r['slots']} used {r['used_slots']} free {r['free_slots']} "
                f"infrastructure {r['infrastructure']} manpower {r['manpower']}",
                "buildings: " + ", ".join(f"{k} {v}" for k, v in r["buildings"].items()),
                "province buildings: " + "; ".join(f"{p}: " + ",".join(f"{k} {v}" for k, v in b.items())
                                                   for p, b in r["province_buildings"].items()),
                "resources: " + ", ".join(f"{k} {v}" for k, v in r["resources"].items()),
                "VP: " + ", ".join(f"{p} {w.province_name(p) or ''} {v:g}" for p, v in r["vps"].items()),
                "provinces: " + " ".join(map(str, r["provinces"])),
                "neighbours: " + "; ".join(f"{n['state']} {n['name']} ({n['owner']})" for n in r["neighbours"])]

    return emit(args, row, text)


def cmd_states(args):
    w = world(args, args.TAG)
    rows = dossiermod.state_rows(w, args.TAG)

    def text(rs):
        out = ["rank id name cat/slots free infra civ/mil/dock VP enemy/border-steps borders [src]"]
        for r in rs:
            out.append(f"{r['rank']} {r['id']} {r['name']}{'' if r['core'] else '*'} {r['category']}/{r['slots']} "
                       f"{r['free_slots']} {r['infrastructure']} {r['civ']}/{r['mil']}/{r['dock']} {r['vp']:g} "
                       f"{r['enemy_steps']}/{r['border_steps']} {' '.join(r['borders'])} [{r['src']}]")
        out.append("(* non-core) " + dossiermod.STATES_METHOD)
        return out

    return emit(args, rows, text)


def cmd_province(args):
    w = world(args)
    g = w.graph()
    if g is None:
        return fail("no map graph")
    p = mapgraph.province(g, args.id)
    if p is None:
        return fail(f"unknown province {args.id}")
    sid = p.get("state")
    st = w.states().get(sid) if sid else None
    p["name"] = w.province_name(args.id)
    if st:
        p["owner"] = st["owner"]
        p["controller"] = st["controller"]
        p["victory_points"] = st["vps"].get(args.id)
        p["state_name"] = st["name"]
        p["buildings"] = st["province_buildings"].get(args.id, {})

    def text(r):
        out = [f"{r['id']} {r.get('name') or ''} {r['type']} {r['terrain']} state {r.get('state')} "
               f"{r.get('state_name', '')} owner {r.get('owner')} region {r.get('region')}"
               + (f" VP {r['victory_points']:g}" if r.get("victory_points") else "")
               + (" coastal" if r.get("coastal") in (True, "true") else "")
               + (f" hub {r['supply_hub']}" if r.get("supply_hub") else "")
               + (f" railway {r['railway']}" if r.get("railway") else "")
               + (f" naval_base {r['naval_base']}" if r.get("naval_base") else "")]
        for n in r["neighbours"]:
            out.append(f"  -> {n['id']} {n['type']} {n['terrain']} state {n['state']} {n['kind']}"
                       + (" RIVER" if n["river"] else "") + (" impassable" if n.get("impassable") else ""))
        return out

    return emit(args, p, text)


def cmd_route(args):
    w = world(args)
    g = w.graph()
    if g is None:
        return fail("no map graph")
    r = mapgraph.route(g, args.src, args.dst, avoid_rivers=args.avoid_rivers)
    if r is None:
        return fail(f"no land route {args.src} -> {args.dst}")
    r["provinces"] = [{"id": p, "terrain": g["provinces"][p]["terrain"], "state": g["provinces"][p].get("state"),
                       "owner": w.owner_of_province(p)} for p in r["path"]]

    def text(x):
        return [f"{x['steps']} steps, {x['rivers']} river crossings {x['river_crossings']}, cost {x['cost']}",
                " -> ".join(f"{p['id']}({p['terrain'][:4]},{p['owner']})" for p in x["provinces"])]

    return emit(args, r, text)


def cmd_vp(args):
    w = world(args)
    v = w.vp(args.TAG)
    rules = [o for o in scenariomod.outcomes(w, args.TAG) if o.get("requires_control")]
    for t in w.at_war_with(args.TAG):  # the other side's rules about this tag's states
        rules += [o for o in scenariomod.outcomes(w, t) if o.get("requires_control")]
    v["control_rules"] = rules

    def text(x):
        out = [f"{x['tag']} capital {x['capital']} total {x['total']:g} VP; base limit {x['base_limit']['v']} "
               f"[{x['base_limit']['src']}]"]
        for t in x["thresholds"]:
            out.append(f"  limit {t['limit']} -> {t['vp_needed']} VP ({t['with']})")
        for m in x["modifiers"]:
            out.append(f"  modifier {m['idea']} {m['value']} {'held' if m['held_at_start'] else 'possible'} [{m['src']}]")
        for r in x["provinces"]:
            out.append(f"  {r['province']} {r['name'] or ''} {r['vp']:g} state {r['state']} {r['state_name']} [{r['src']}]")
        for o in x["control_rules"]:
            out.append(f"  rule: {o['flag']} needs control of {o['requires_control']['states']} [{o['src']}]")
        return out

    return emit(args, v, text)


def cmd_oob(args):
    w = world(args)
    o = w.oob(args.TAG)
    enemy = w.mode == "fair" and w.tag and args.TAG != w.tag
    counts = {}
    for d in o["divisions"]:
        counts[d["template"]] = counts.get(d["template"], 0) + 1
    if enemy:  # FAIR: count and templates are aggregate, positions are private
        data = {"tag": args.TAG, "fair": "aggregate", "files": o["files"], "divisions": len(o["divisions"]),
                "templates": sorted(t for t in o["templates"] if t), "per_template": counts,
                "air_planes": sum(a["amount"] or 0 for a in o["air"]),
                "note": "FAIR: per-division positions are private; use --mode meta"}
        return emit(args, data, lambda x: [f"{x['tag']} (FAIR aggregate): {x['divisions']} divisions, planes "
                                           f"{x['air_planes']}", "templates: " + ", ".join(
                                               f"{k} x{v}" for k, v in x["per_template"].items()), x["note"]])
    data = dict(o, tag=args.TAG, per_template=counts, templates=list(o["templates"].values()))

    def text(x):
        out = ["files: " + ", ".join(f"{f['name']} ({f['kind']})" for f in x["files"])]
        for t in x["templates"]:
            out.append(f"template \"{t['name']}\" x{counts.get(t['name'], 0)}: "
                       + " ".join(f"{k}x{v}" for k, v in t["regiments"].items()) + f" | support {','.join(t['support'])}")
        per = {}
        for d in x["divisions"]:
            per.setdefault(d["state"], []).append(f"{d['template']}@{d['location']}")
        for s, ds in sorted(per.items(), key=lambda kv: kv[0] or 0):
            out.append(f"state {s} {w.state_name(s) if s else ''}: " + ", ".join(ds))
        for a in x["air"]:
            out.append(f"air state {a['state']}: {a['type']} {a['amount']} {a.get('version') or ''}")
        for f in x["fleets"]:
            out.append(f"fleet {f['name']} base {f['base']}: " + ", ".join(f"{k} {v}" for k, v in f["ships"].items()))
        for p in x["production"]:
            out.append(f"production {p['type']} {p.get('version') or ''} x{p['factories']}")
        return out

    return emit(args, data, text)


def cmd_characters(args):
    w = world(args, args.TAG)
    rows = w.characters(args.TAG, args.role)

    def text(rs):
        out = []
        for c in rs:
            roles = []
            for r in c["roles"]:
                bits = [r["role"]]
                for k in ("skill", "attack", "defense", "planning", "logistics", "slot", "cost", "ideology"):
                    if r.get(k) is not None:
                        bits.append(f"{k} {r[k]}")
                if r.get("traits"):
                    bits.append("traits " + ",".join(r["traits"]))
                roles.append(" ".join(bits))
            out.append(f"{'*' if c['recruited'] else ' '}{c['id']} \"{c['name']}\": " + "; ".join(roles) + f" [{c['src']}]")
        out.append("(* recruited at start)")
        return out

    return emit(args, rows, text)


def cmd_laws(args):
    w = world(args, args.TAG)
    laws = w.laws(args.TAG)

    def text(x):
        out = []
        for cat, row in sorted(x.items()):
            out.append(f"{cat} (now {row['current']})")
            for o in row["options"]:
                out.append(f"  {'*' if o['id'] == row['current'] else ' '}{o['id']} \"{o['name']}\" {o['cost']}PP"
                           + (f" | {o['modifier'][:120]}" if o.get("modifier") else "")
                           + (f" | needs {o['available'][:120]}" if o.get("available") else "") + f" [{o['src']}]")
        return out

    return emit(args, laws, text)


def cmd_idea(args):
    w = world(args)
    row = w.idea_row(args.id)
    if not row:
        return fail(f"unknown idea {args.id}")
    if w.tag:
        exits = dossiermod.idea_exits(w, w.tag).get(args.id)
        if exits:
            row["exits"] = exits
    return emit(args, row, lambda r: [f"{r['id']} \"{r['name']}\" category {r['category']} cost {r['cost']} "
                                      f"law {r['law']} [{r['src']}]"] +
                [f"{k}: {r[k]}" for k in ("allowed", "available", "modifier", "exits") if r.get(k) not in (None, True)])


def _decision_text(rs):
    out = []
    for x in rs:
        days = x.get("mission_days")
        if x.get("mission_days_value"):
            days = f"{days}={x['mission_days_value']['v']}"
        out.append(f"{x['id']} \"{x['name']}\" cost {x['cost']}" + (f" mission {days}d" if days is not None else "")
                   + (f" remove {x['days_remove']}d" if x.get("days_remove") else "")
                   + (" once" if x.get("fire_only_once") else "") + f" [{x['src']}]")
        for k in ("available", "visible", "activation", "complete_effect", "timeout_effect", "remove_effect", "controls"):
            if x.get(k):
                v = x[k]
                out.append(f"    {k}: {', '.join(v) if isinstance(v, list) else v}")
    return out


def cmd_decisions(args):
    w = world(args, args.TAG)
    rows = [x for x in w.decisions(args.TAG, missions=False) if x.get("specific") or args.all]
    return emit(args, rows, _decision_text)


def cmd_missions(args):
    w = world(args, args.TAG)
    rows = [x for x in w.decisions(args.TAG, missions=True) if x.get("specific") or args.all]
    deadlines = scenariomod.hooks(w, args.TAG)["deadlines"]
    if args.json:
        return emit(args, {"missions": rows, "deadlines": deadlines})
    out = _decision_text(rows)
    for m in deadlines:
        out.append(f"DEADLINE {m['mission']}: {m['earliest']}..{m['latest']} ({m['timeout']}d"
                   + (f" after {' -> '.join(m['via'])}" if m["via"] else "") + ")")
    print("\n".join(out))
    return 0


def cmd_hooks(args):
    w = world(args, args.TAG)
    h = scenariomod.hooks(w, args.TAG)

    def text(x):
        out = ["wars: " + "; ".join(f"{a['attacker']}->{a['target']} {a['type']} [{a['src']}]" for a in x["wars"])]
        for o in x["outcomes"]:
            out.append(f"outcome {o['flag']} in {o['set_in']} when {' AND '.join(o['conditions']) or 'always'} [{o['src']}]"
                       + (f" called from {', '.join(o['called_from'])}" if o.get("called_from") else ""))
        for m in x["deadlines"]:
            out.append(f"deadline {m['mission']} {m['earliest']}..{m['latest']}")
        for g in x["goals"]:
            out.append(f"goal {g['id']} controls {g['controls']['states']} {g['controls']['provinces']} [{g['src']}]")
        for e in x["escalation"]:
            out.append(f"threshold {e['threshold']} ({e['on_action']}) -> {', '.join(e['effects'])} [{e['src']}]")
        for t in x["timers"]:
            out.append(f"timer {t['kind']} {t['id']} {t['days']}d [{t['src']}]")
        for r in x["on_startup"] + x["on_actions"]:
            out.append(f"{r['on_action']}: " + (f"when {r['condition'][:160]}: " if r.get("condition") else "")
                       + ", ".join(r.get("effects") or []) + f" [{r['src']}]")
        return out

    return emit(args, h, text)


def cmd_event(args):
    w = world(args)
    e = scenariomod.event(w, args.id)
    if not e:
        return fail(f"unknown event {args.id}")

    def text(x):
        out = [f"{x['id']} ({x['type']}) \"{x['title']}\" [{x['src']}]" + (" hidden" if x["hidden"] else "")]
        for k in ("trigger", "mean_time_to_happen", "immediate"):
            if x.get(k):
                out.append(f"{k}: {x[k]}")
        for o in x["options"]:
            out.append(f"option {o['name']} \"{o['text']}\" -> {', '.join(o['effects'])} [{o['src']}]")
            for k in ("trigger", "ai_chance"):
                if o.get(k):
                    out.append(f"    {k}: {o[k]}")
        return out

    return emit(args, e, text)


def cmd_grep(args):
    w = world(args)
    rows = scenariomod.grep(w, args.token, limit=args.limit, folders=args.folder,
                            languages=None if args.all_languages else ("english",))
    return emit(args, rows, lambda rs: [f"{r['src']}{' (' + r['mod'] + ')' if r['mod'] else ''}: {r['text']}"
                                        for r in rs] or ["no match"])


def cmd_defines(args):
    w = world(args)
    hits = w.defines.find(args.KEY)
    rows = [{"key": k, "v": r["v"], "src": w.src(r["file"], r["line"]), "mod": w.content.source(r["file"])}
            for k, r in hits[:50]]
    return emit(args, rows, lambda rs: [f"{r['key']} = {r['v']} [{r['src']}]" for r in rs] or ["no define"])


# ---------------------------------------------------------------- parser

def _globals(p, suppress):
    d = argparse.SUPPRESS if suppress else None
    p.add_argument("--game", default=d, help="game folder (default HOI4_DIR or the Steam install)")
    p.add_argument("--userdir", default=d, help="HOI4 user folder (dlc_load.json, mods, logs)")
    p.add_argument("--mod", action="append", default=d, help="extra mod (.mod descriptor or folder)")
    p.add_argument("--date", default=d, help="start date (default: manifest, else 1936.1.1)")
    p.add_argument("--mode", choices=("fair", "meta"), default=d)
    p.add_argument("--json", action="store_true", default=d if suppress else False)
    p.add_argument("--tag", default=d, help="our country (default: the manifest's)")
    p.add_argument("--dlc", default=d, help="auto, or a comma-separated list of active DLC names")
    p.add_argument("--out", default=d, help="dossier folder")


def parser():
    ap = argparse.ArgumentParser(prog="facts.py", description=__doc__.split("\n")[0])
    _globals(ap, False)
    sub = ap.add_subparsers(dest="cmd", required=True)

    def add(name, fn, *pos, **kw):
        p = sub.add_parser(name)
        _globals(p, True)
        for a in pos:
            if isinstance(a, tuple):
                p.add_argument(a[0], **a[1])
            else:
                p.add_argument(a)
        p.set_defaults(fn=fn)
        return p

    p = add("dossier", cmd_dossier, ("action", {"choices": ("build", "show")}), ("section", {"nargs": "?"}))
    p.add_argument("--mods", choices=("auto", "none"), default="auto")
    p.add_argument("--enemy", action="append", help="enemy tag for sections 3, 4 and 9 (default: wars at start)")
    add("focus", cmd_focus, "id")
    p = add("focus-tree", cmd_focus_tree, "TAG")
    p.add_argument("--branch")
    p.add_argument("--depth", type=int)
    add("focus-timeline", cmd_focus_timeline, "TAG", ("ids", {"nargs": "+"}))
    add("tech", cmd_tech, "id")
    p = add("research-calendar", cmd_research_calendar, "TAG")
    p.add_argument("--years", help="e.g. 1936-1938")
    p = add("research-timeline", cmd_research_timeline, "plan")
    p.add_argument("--rule", choices=("all", "any"), default="all")
    add("state", cmd_state, ("id", {"type": int}))
    add("states", cmd_states, "TAG")
    add("province", cmd_province, ("id", {"type": int}))
    p = add("route", cmd_route, ("src", {"type": int}), ("dst", {"type": int}))
    p.add_argument("--avoid-rivers", action="store_true")
    add("vp", cmd_vp, "TAG")
    add("oob", cmd_oob, "TAG")
    p = add("characters", cmd_characters, "TAG")
    p.add_argument("--role", choices=("general", "marshal", "admiral", "advisor", "leader"))
    add("laws", cmd_laws, "TAG")
    add("idea", cmd_idea, "id")
    p = add("decisions", cmd_decisions, "TAG")
    p.add_argument("--all", action="store_true", help="also generic decisions that do not name the tag")
    p = add("missions", cmd_missions, "TAG")
    p.add_argument("--all", action="store_true")
    add("hooks", cmd_hooks, "TAG")
    add("event", cmd_event, "id")
    p = add("grep", cmd_grep, "token")
    p.add_argument("--limit", type=int, default=200)
    p.add_argument("--folder", action="append", help="limit to a content folder (repeat)")
    p.add_argument("--all-languages", action="store_true", help="also non-English localisation")
    add("defines", cmd_defines, "KEY")
    return ap


def main(argv=None):
    args = parser().parse_args(argv)
    for g in GLOBALS:
        if not hasattr(args, g):
            setattr(args, g, None)
    if args.json is None:
        args.json = False
    return args.fn(args)


if __name__ == "__main__":
    sys.exit(main())
