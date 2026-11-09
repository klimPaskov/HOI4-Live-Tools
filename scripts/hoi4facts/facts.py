#!/usr/bin/env python3
"""Facts CLI for the strategist: deterministic answers from the effective game files, with sources.

    facts.py dossier build --tag T --date D [--dlc auto|"A,B"] [--mods auto|none] --out DIR
    facts.py dossier show <section 0-10 or name> [--out DIR]
    facts.py focus <id>
    facts.py focus-tree <TAG> [--branch id] [--depth N]
    facts.py focus-timeline <TAG> --date D <ids...>
    facts.py tech <id>
    facts.py research-calendar <TAG> --date D --years 1936-1938
    facts.py research-timeline PLAN.json [--rule all|any]
    facts.py state <id>          facts.py states <TAG> [--energy]   (coal per state, energy estimate)
    facts.py province <id>       facts.py route <from> <to> [--avoid-rivers]
    facts.py vp <TAG>            facts.py oob <TAG>
    facts.py characters <TAG> [--role general|marshal|admiral|advisor|leader]
    facts.py laws <TAG>          facts.py idea <id>
    facts.py decisions <TAG>     facts.py missions <TAG>
    facts.py hooks <TAG>         facts.py event <id>   (options, and who fires it: caller, scope, delay)
    facts.py effect <name>       scripted effect (or trigger) body with source, and its callers
    facts.py resources <TAG>     per-resource balance at the start: owned states, trade-law export, start lines
    facts.py grep <token> [-C N] facts.py defines <KEY>   (FAIR: grep leaves out common/ai_*)
    facts.py terrain-width <TAG | state ids> [--route A B]   terrain mix, combat widths, width target
    facts.py units [--category C] [--unlocked-by D --tag T]  battalions and companies: width, needs, rows
    facts.py weather <state | --region ID | R<id> | name>     monthly mud, snow, cold, heat
    facts.py bop <TAG>           balance of power: ranges, start value, focuses/decisions that move it
    facts.py occupation-laws     occupation laws, resistance/compliance thresholds, own states at start
    facts.py ports <TAG> [--enemy E]   naval bases, overseas theatres, sea routes, coastal port candidates
    facts.py designs <TAG> [--kind tank|plane|ship] [--date D]   chassis/hulls/airframes and modules the date's
                                 techs unlock, module XP costs, start variants
    facts.py mio <TAG> [--id ORG]   military-industrial organisations, trait trees, policies
    facts.py projects <TAG>      special projects, facilities, scientists
    facts.py diplomacy <TAG>     world tension, ideology gates (justify, guarantee, volunteers, lend-lease, faction), war goals
    facts.py factions <TAG>      faction templates, manifests, goals, rules, member upgrades
    facts.py peace <TAG> [--enemy E]   peace-conference cost modifiers and defines
    facts.py command <TAG>       military spirits, leader abilities, assignable traits, command-power costs
    facts.py agency              intelligence agency upgrades and operations

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
import re
import sys

if __package__ in (None, ""):
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    __package__ = "hoi4facts"  # noqa: A001

from hoi4facts import dossier as dossiermod  # noqa: E402
from hoi4facts import focus as focusmod  # noqa: E402
from hoi4facts import mapgraph  # noqa: E402
from hoi4facts import research as researchmod  # noqa: E402
from hoi4facts import scenario as scenariomod  # noqa: E402
from hoi4facts import theatre as theatremod  # noqa: E402
from hoi4facts.content import default_userdir, mod_from_arg, mods_from_userdir  # noqa: E402
from hoi4facts.game import Game  # noqa: E402
from hoi4facts.world import World, gate_text  # noqa: E402

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
        return fail(f"unknown section {args.section!r} (0-10 or " +
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
    occ = w.occupation_start(st["owner"]).get(args.id) if st["owner"] else None
    row["occupation"] = occ or {}

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
                "occupation at start: " + (", ".join(f"{k} {r['occupation'][k]}" for k in
                                                     ("resistance_active", "resistance", "compliance", "garrison")
                                                     if k in r["occupation"]) + f" [{', '.join(r['occupation']['srcs'])}]"
                                           if r["occupation"] else "none set"),
                "neighbours: " + "; ".join(f"{n['state']} {n['name']} ({n['owner']})" for n in r["neighbours"])]

    return emit(args, row, text)


energy_line = dossiermod.energy_line


def cmd_states(args):
    w = world(args, args.TAG)
    if args.energy:
        e = w.energy(args.TAG)

        def etext(x):
            out = ["id name coal factories infra local-consumption energy-buildings [src]"]
            for s in x["states"]:
                if s["coal"] or s["factories"] or s["local_consumption"]:
                    out.append(f"{s['id']} {s['name']} {s['coal']:g} {s['factories']} {s['infrastructure']} "
                               f"{s['local_consumption']:+g} {','.join(f'{b} {n}' for b, n in s['buildings'].items())} "
                               f"[{s['src']}]")
            out.append(energy_line(x))
            out.append("method: " + x["d"])
            return out

        return emit(args, e, etext)
    rows = dossiermod.state_rows(w, args.TAG)

    def text(rs):
        out = ["rank id name cat/slots free infra civ/mil/dock coal VP enemy/border-steps borders [src]"]
        for r in rs:
            out.append(f"{r['rank']} {r['id']} {r['name']}{'' if r['core'] else '*'} {r['category']}/{r['slots']} "
                       f"{r['free_slots']} {r['infrastructure']} {r['civ']}/{r['mil']}/{r['dock']} {r['coal']:g} {r['vp']:g} "
                       f"{r['enemy_steps']}/{r['border_steps']} {' '.join(r['borders'])} [{r['src']}]")
        out.append("(* non-core) " + dossiermod.STATES_METHOD)
        out.append(energy_line(w.energy(args.TAG)) + f"; per state: `facts states {args.TAG} --energy`")
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
                g = gate_text(o.get("gates") or [])
                out.append(f"  {'*' if o['id'] == row['current'] else ' '}{o['id']} \"{o['name']}\" {o['cost']}PP"
                           + (f" | GATES {g}" if g else "")
                           + (f" | {o['modifier'][:120]}" if o.get("modifier") else "")
                           + (f" | needs {o['available'][:120]}" if o.get("available") else "") + f" [{o['src']}]")
        out.append("GATES: numeric and named conditions of `available` with the static parts decided (has_dlc, tag, "
                   "if-branches); 'any of' = an OR block")
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
                   + (" once" if x.get("fire_only_once") else "")
                   + (f" NEVER-TAKE ({'; '.join(x['never_take'][:3])})" if x.get("never_take") else "")
                   + (" starts mission " + ", ".join(f"{m['id']} ({m['days']}d)" for m in x["starts_missions"])
                      if x.get("starts_missions") else "") + f" [{x['src']}]")
        for k in ("available", "visible", "activation", "complete_effect", "timeout_effect", "remove_effect", "controls"):
            if x.get(k):
                v = x[k]
                out.append(f"    {k}: {', '.join(v) if isinstance(v, list) else v}")
    return out


def flag_decisions(w, tag, rows):
    """Add never_take (scenariomod.never_take) and the missions a decision starts (with their timers)."""
    never = {x["id"]: x for x in scenariomod.never_take(w, tag)}
    timers = {m["id"]: m.get("mission_days") for m in w.decisions(tag, missions=True)}
    for x in rows:
        if x["id"] in never:
            x["never_take"] = never[x["id"]]["reasons"]
        starts = [e.split(":", 1)[1] for e in x.get("complete_effect", []) if e.startswith("activate_mission:")]
        if starts:
            x["starts_missions"] = [{"id": m, "days": timers.get(m)} for m in starts]
    return rows


def cmd_decisions(args):
    w = world(args, args.TAG)
    rows = flag_decisions(w, args.TAG, [x for x in w.decisions(args.TAG, missions=False)
                                        if x.get("specific") or args.all])
    if args.never:
        rows = [x for x in rows if x.get("never_take")]
    return emit(args, rows, lambda rs: _decision_text(rs) + [
        "NEVER-TAKE: " + scenariomod.NEVER_METHOD])


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
            out.append(f"{r['on_action']}: " + (f"when {r['condition']}: " if r.get("condition") else "")
                       + ", ".join(r.get("effects") or []) + f" [{r['src']}]")
        return out

    return emit(args, h, text)


def cmd_event(args):
    w = world(args)
    e = scenariomod.event(w, args.id)
    if not e:
        return fail(f"unknown event {args.id}")
    e["called_from"] = scenariomod.event_callers(w, args.id)

    def text(x):
        out = [f"{x['id']} ({x['type']}) \"{x['title']}\" [{x['src']}]" + (" hidden" if x["hidden"] else "")
               + (" triggered_only" if x.get("triggered_only") else "")]
        for k in ("trigger", "mean_time_to_happen", "immediate"):
            if x.get(k):
                out.append(f"{k}: {x[k]}")
        for c in x["called_from"]:
            out.append(f"fired by {c['src']}: {c['via'] or 'top level'} -> {c['key']}; receiver scope {c['scope']}"
                       + (f"; delay {' '.join(f'{k}={v}' for k, v in c['delay'].items())}" if c.get("delay") else ""))
            for cond in c["conditions"]:
                out.append(f"    when {cond}")
            if c.get("effect_called_from"):
                out.append(f"    effect {c['effect']} called from: " + "; ".join(c["effect_called_from"]))
        if not x["called_from"] and x.get("triggered_only"):
            out.append("fired by: no caller found in events, common or history (code, console or a mod)")
        for o in x["options"]:
            out.append(f"option {o['name']} \"{o['text']}\" -> {', '.join(o['effects'])} [{o['src']}]")
            for k in ("trigger", "ai_chance"):
                if o.get(k):
                    out.append(f"    {k}: {o[k]}")
        return out

    return emit(args, e, text)


def cmd_resources(args):
    w = world(args, args.TAG)
    r = w.resources(args.TAG)

    def text(x):
        out = [f"{x['tag']} trade law {x['trade_law']} (min_export {x['min_export']})",
               "resource produced exported available needed balance"]
        for k, b in x["balance"].items():
            out.append(f"{k} {b['produced']:g} {b['exported']:g} {b['available']:g} {b['needed']:g} {b['balance']:+g}")
        for s in x["states"]:
            out.append(f"state {s['id']} {s['name']}: " + " ".join(f"{k} {v:g}" for k, v in s["resources"].items())
                       + f" [{s['src']}]")
        for ln in x["lines"]:
            out.append(f"line {ln['equipment']} {ln['version'] or ''} x{ln['factories']}: "
                       + (" ".join(f"{k} {v:g}" for k, v in ln["per_factory"].items()) or "no resources")
                       + " per factory" + (f" ({ln['note']})" if ln.get("note") else "") + f" [{ln['src']}]")
        out.append("method: " + x["d"])
        return out

    return emit(args, r, text)


def cmd_effect(args):
    w = world(args)
    e = scenariomod.effect_body(w, args.name)
    if not e:
        return fail(f"unknown scripted effect or trigger {args.name}")
    e["called_from"] = scenariomod.effect_callers(w, args.name) if e["kind"] == "scripted_effect" else []

    def text(x):
        out = [f"{x['kind']} {x['name']} [{x['src']}]" + (f" ({x['mod']})" if x.get("mod") else ""), x["text"]]
        for c in x["called_from"]:
            out.append(f"called from {c['src']}: {c['via'] or 'top level'}; scope {c['scope']}")
        return out

    return emit(args, e, text)


def cmd_grep(args):
    w = world(args)
    rows = scenariomod.grep(w, args.token, limit=args.limit, folders=args.folder,
                            languages=None if args.all_languages else ("english",), context=args.context or 0)

    def text(rs):
        out = []
        for r in rs:
            if "hidden_ai_only" in r:
                out.append(f"({r['hidden_ai_only']} file(s) in AI-only folders hidden: {r['note']})")
                continue
            out.append(f"{r['src']}{' (' + r['mod'] + ')' if r['mod'] else ''}: {r['text']}"
                       + (" [substring]" if r.get("substring") else ""))
            if r.get("context"):
                out.extend("    " + c for c in r["context"])
        return out or ["no match"]

    return emit(args, rows, text)


def cmd_terrain_width(args):
    w = world(args)
    g = w.graph()
    if g is None:
        return fail("no map graph")
    subsets = {}
    if args.route:
        r = mapgraph.route(g, args.route[0], args.route[1])
        if r is None:
            return fail(f"no land route {args.route[0]} -> {args.route[1]}")
        subsets["route"] = r["path"]
    targets = [x for t in (args.target or []) for x in str(t).replace(",", " ").split()]
    ids = [int(x) for x in targets if x.isdigit()]
    tags = [x for x in targets if re.match(r"^[A-Z][A-Z0-9]{2}$", x)]
    if ids:
        st = w.states()
        missing = [s for s in ids if s not in st]
        if missing:
            return fail(f"unknown states {missing}")
        subsets["states " + ",".join(map(str, ids))] = [p for s in ids for p in st[s]["provinces"]]
    for t in tags:
        subsets[f"{t} territory"] = w.provinces_of([t])
        ours = w.tag
        pairs = [(ours, t)] if ours and ours != t else [(e, t) for e in (args.enemy or w.at_war_with(t))]
        for attacker, defender in pairs:
            th = theatremod.theatre(w, attacker, defender)
            front = theatremod.front_provinces(w, attacker, defender) | theatremod.front_provinces(w, defender, attacker)
            subsets[f"{attacker}-{defender} border"] = sorted(front)
            subsets[f"{attacker}->{defender} routes"] = sorted({p for r in th["routes"] for p in r.get("path", [])})
    if not subsets:
        return fail("give a TAG, state ids or --route A B")
    rows = []
    for name, provs in subsets.items():
        tw = theatremod.terrain_width(w, provs)
        rows.append({"subset": name, "mix": tw["mix"], "width": tw["width"]})
    fight = [p for name, provs in subsets.items() if not name.endswith("territory") for p in provs] or \
        [p for provs in subsets.values() for p in provs]
    total = theatremod.terrain_width(w, fight)
    data = {"subsets": rows, "fighting": total, "terrains": {t: {k: r.get(k) for k in ("combat_width",
            "combat_support_width", "movement_cost", "attack", "width_src")} for t, r in w.terrains().items()
            if r.get("combat_width")}, "d": theatremod.WIDTH_METHOD}

    def mixtext(m):
        return ", ".join(f"{t} {round(s * 100)}%" for t, s in m["shares"].items()) + f" ({m['provinces']} provinces)"

    def wtext(x):
        if not x or x.get("target") is None:
            return "no width target"
        d = x["detail"]
        fit = "; ".join(f"{t} " + "/".join(f"{v['divisions']}x{v['fill']:.2f}" for v in f.values())
                        for t, f in d["fit"].items())
        return (f"target width {x['target']} (dominant {x['dominant']} {round(x['dominant_share'] * 100)}%, battle widths "
                f"{x['battle_widths'][x['dominant']]}; score {d['dominant_score']}, mix {d['mix_score']}); best for the mix "
                + ", ".join(f"{b['width']} ({b['mix_score']})" for b in x["best_for_mix"])
                + (f"; best <= 25: {x['best_small']['width']} ({x['best_small']['mix_score']})" if x.get("best_small") else "")
                + f" | divisions x fill per 1/2 directions: {fit}")

    def text(x):
        out = ["terrain widths: " + "; ".join(f"{t} {r['combat_width']:g}+{r['combat_support_width']:g} "
                                             f"[{r['width_src']}]" for t, r in x["terrains"].items())]
        for r in x["subsets"]:
            out.append(f"{r['subset']}: {mixtext(r['mix'])}")
        out.append(f"fighting terrain: {mixtext(x['fighting']['mix'])}")
        out.append(wtext(x["fighting"]["width"]))
        out.append("method: " + x["d"])
        return out

    return emit(args, data, text)


def cmd_units(args):
    if args.unlocked_by:
        args.date = args.unlocked_by
    w = world(args)
    units = w.sub_units()
    done = set(w.country(w.tag)["techs"]) if w.tag else set()
    year = int(w.date.split(".")[0])
    ready = set()
    if w.tag:  # equipment archetypes with a variant enabled by the start techs
        from hoi4facts import derive
        from hoi4facts.world import _equipment_archetypes
        arche = _equipment_archetypes(w.game)
        for e, techs in derive.equipment_unlocks(w.game).items():
            if techs & done:
                ready |= {e, arche.get(e, e)}
    rows = []
    for uid, u in sorted(units.items(), key=lambda kv: (kv[1]["row"], kv[0])):
        if args.category:
            c = args.category.lower()
            hay = [u["row"], u["group"] or "", uid] + u["categories"] + u["type"]
            if not any(c in (h or "").lower() for h in hay):
                continue
        elif u["row"] in ("air", "naval") and not args.all:
            continue
        row = {k: u[k] for k in ("id", "name", "row", "group", "type", "combat_width", "manpower", "training_time",
                                  "need", "special_forces", "allowed_battalion_groups", "categories", "stats",
                                  "active", "src")}
        row["unlocked_by"] = u["unlocked_by"]
        if w.tag:
            have = [x["tech"] for x in u["unlocked_by"] if x["tech"] in done]
            row["unlocked"] = bool(u["active"] or have)
            row["unlocked_via"] = have or (["active = yes"] if u["active"] else [])
            row["by_year"] = row["unlocked"] or any((x["year"] or 0) <= year for x in u["unlocked_by"])
            row["equipment_missing"] = [e for e in u["need"] if e not in ready]
        rows.append(row)
    if args.unlocked_by and w.tag:
        rows = [r for r in rows if r["unlocked"]]
    data = {"tag": w.tag or None, "date": w.date, "rows": rows,
            "d": "common/units sub_units; row from group / divisional = no / regimental = no; unlocked = active = yes or "
                 "a technology with enable_subunits the tag has at the date (history set_technology); by_year = "
                 "an enabling technology's start_year <= the date's year (researchable without the ahead penalty); "
                 "equipment_missing = needed archetypes with no variant enabled by those techs (a stockpile or "
                 "licence may still hold some)"}

    def text(x):
        out = []
        for r in x["rows"]:
            need = ",".join(f"{k} {v:g}" for k, v in r["need"].items())
            st = r["stats"]
            bits = [f"{r['id']} \"{r['name']}\" {r['row']}", f"group {r['group']}", f"width {r['combat_width']:g}",
                    f"mp {r['manpower']:g}"]
            if need:
                bits.append("needs " + need)
            for k in ("max_organisation", "soft_attack", "hard_attack", "defense", "breakthrough", "maximum_speed",
                      "suppression"):
                if k in st:
                    bits.append(f"{k} {st[k]:g}")
            if r["special_forces"]:
                bits.append("SPECIAL FORCES")
            if r["allowed_battalion_groups"]:
                bits.append("with " + "/".join(r["allowed_battalion_groups"]))
            if "unlocked" in r:
                bits.append(("UNLOCKED by " + ",".join(r["unlocked_via"])) if r["unlocked"] else
                            ("locked; techs " + ",".join(f"{t['tech']}({t['year']})" for t in r["unlocked_by"][:3])))
                if r["equipment_missing"]:
                    bits.append("no equipment tech yet: " + ",".join(r["equipment_missing"]))
            elif r["unlocked_by"]:
                bits.append("techs " + ",".join(f"{t['tech']}({t['year']})" for t in r["unlocked_by"][:3]))
            out.append(" | ".join(bits) + f" [{r['src']}]")
        out.append(x["d"])
        return out

    return emit(args, data, text)


def _region_arg(w, arg, force_region=False):
    """[(region id, provinces of the target in it)] for a state id, a region id (--region, R102, region:102)
    or a region name."""
    regs = w.strategic_regions()
    a = str(arg).strip()
    m = re.match(r"^(?:r|region:)(\d+)$", a, re.I)
    if m or (force_region and a.isdigit()):
        rid = int(m.group(1) if m else a)
        return [(rid, len(regs[rid]["provinces"]))] if rid in regs else []
    if a.isdigit():
        st = w.states().get(int(a))
        if not st:
            return []
        pr = theatremod.province_regions(w)
        counts = {}
        for p in st["provinces"]:
            if p in pr:
                counts[pr[p]] = counts.get(pr[p], 0) + 1
        return sorted(counts.items(), key=lambda kv: -kv[1])
    hits = [rid for rid, r in regs.items() if a.lower() in (r["name"] or "").lower()]
    return [(rid, len(regs[rid]["provinces"])) for rid in sorted(hits)]


def cmd_weather(args):
    w = world(args)
    regs = _region_arg(w, args.target, args.region)
    if not regs:
        return fail(f"no strategic region for {args.target!r} (a state id, --region ID, R<id> or a region name)")
    data = {"target": args.target, "regions": [], "settings_src": w.weather_settings()["src"],
            "terrain_modifiers": w.weather_settings()["terrain_modifiers"], "d": theatremod.WEATHER_METHOD}
    for rid, n in regs:
        r = w.strategic_regions()[rid]
        data["regions"].append({"region": rid, "name": r["name"], "provinces": n, "src": r["src"],
                                "months": theatremod.weather_months(w, rid),
                                "summary": theatremod.weather_summary(w, rid)})

    def text(x):
        out = []
        for r in x["regions"]:
            s = r["summary"]
            out.append(f"region {r['region']} {r['name']} ({r['provinces']} provinces) [{r['src']}]: mud {s['mud']} "
                       f"(heavy {s['heavy_mud']}); ground snow {s['snow']}; extreme cold {s['cold']}; extreme heat {s['heat']}")
            for m in r["months"]:
                t = m["temperature"]
                out.append(f"  {m['name']} {t[0]:g}..{t[1]:g}C rain {round(m['rain'] * 100)}% (heavy "
                           f"{round(m['heavy_rain'] * 100)}%) snow {round(m['snowfall'] * 100)}% mud {m['mud']} "
                           f"(wet {m['wet']:g} x{m['mud_factor']:g}) ground snow {m['ground_snow']}"
                           + (f" cold {m['cold']}" if m["cold"] else "") + (f" heat {m['heat']}" if m["heat"] else "")
                           + (f" sandstorm {round(m['sandstorm'] * 100)}%" if m["sandstorm"] else ""))
        out.append("method: " + x["d"])
        return out

    return emit(args, data, text)


def _bop_text(rows, w=None):
    out = []
    for r in rows:
        d = r["definition"] or {}
        st = r["start"]
        out.append(f"{r['id']} [{r['src']}]: sides {d.get('left_side')} (-1) / {d.get('right_side')} (+1); "
                   f"initial {d.get('initial_value')}; decisions {d.get('decision_category')}")
        out.append("  at start: " + (f"ACTIVE value {st['value']} range {st.get('range')} [{st.get('src')}]"
                                     if st.get("active") else "not active"))
        for g in r["gained_by"][:4]:
            out.append(f"  gained by {g['kind']} {g['id']} ({g['effect']} "
                       f"{g['value'] if g['value'] is not None else 'to the initial value'})"
                       f" [{g['src']}]")
        for rg in d.get("ranges", []):
            mods = ", ".join(f"{k} {v}" for k, v in rg["modifier"].items() if v not in (0, 0.0))
            out.append(f"  range {rg['id']} {rg['min']}..{rg['max']}" + (f": {mods}" if mods else "")
                       + (f"; on enter {', '.join(rg['on_activate'][:3])}" if rg["on_activate"] else ""))
        out.append(f"  moved by: " + ", ".join(f"{k} {n}" for k, n in r["mover_counts"].items()))
        for m in r["movers"][:40]:
            amount = m["value"] if m["value"] is not None else (f"{m['modifier']} {m['daily']}/day" if m["modifier"] else "")
            out.append(f"    {m['kind']} {m['id']}: {m['effect']} {amount}" + (f" (via {m['via']})" if m["via"] else "")
                       + f" [{m['src']}]")
        if len(r["movers"]) > 40:
            out.append(f"    ... {len(r['movers']) - 40} more (--json)")
    if not rows:
        out.append("no balance of power for this tag (common/bop, history, focuses, decisions)")
    return out


def cmd_bop(args):
    w = world(args, args.TAG)
    rows = scenariomod.bop(w, args.TAG)
    return emit(args, {"tag": args.TAG, "balances": rows, "d": scenariomod.BOP_METHOD},
                lambda x: _bop_text(x["balances"]) + ["method: " + x["d"]])


def cmd_occupation_laws(args):
    w = world(args)
    data = {"laws": w.occupation_laws(), "modifiers": w.resistance_modifiers(),
            "activities": w.resistance_activities(), "fair": "public"}
    if w.tag:
        data["own_states"] = {str(k): v for k, v in w.occupation_start(w.tag).items()}

    def text(x):
        out = ["Occupation laws (state_modifier; * default, ^ starting, ! main fallback):"]
        for r in x["laws"]:
            flags = ("*" if r.get("default") else "") + ("^" if r.get("starting") else "") + \
                ("!" if r.get("main_fallback") else "")
            mods = ", ".join(f"{k} {v}" for k, v in r["state_modifier"].items())
            out.append(f"- {flags}{r['id']} \"{r['name']}\": {mods}" + (f"; visible {r['visible'][:90]}" if r.get("visible") else "")
                       + (f"; available {r['available'][:90]}" if r.get("available") else "") + f" [{r['src']}]")
        out.append("Resistance / compliance thresholds:")
        for m in x["modifiers"]:
            mods = ", ".join(f"{k} {v}" for k, v in m["state_modifier"].items())
            out.append(f"- {m['id']} ({m['type']}) at {m['threshold']}: {mods or '-'}"
                       + (f"; enabled {m['enabled'][:80]}" if m.get("enabled") else "") + f" [{m['src']}]")
        out.append("Resistance activities: " + ", ".join(f"{a['id']}" for a in x["activities"]))
        if "own_states" in x:
            out.append(f"{w.tag} states with occupation settings at start (resistance on / resistance / compliance / garrison):")
            for sid, r in x["own_states"].items():
                out.append(f"- {sid} {r['name']}{'' if r['core'] else ' (non-core)'}: {r.get('resistance_active', '-')} / "
                           f"{r.get('resistance', '-')} / {r.get('compliance', '-')} / {r.get('garrison', '-')} "
                           f"[{', '.join(r['srcs'])}]")
        return out

    return emit(args, data, text)


def cmd_ports(args):
    w = world(args, args.TAG)
    data = theatremod.ports(w, args.TAG, args.enemy, steps=args.steps, below=args.below)

    def text(x):
        out = [f"{x['tag']} naval bases (province name state level capacity sea zones):"]
        for p in x["ports"]:
            out.append(f"- {p['province']} {p['name'] or ''} st {p['state']} {p['state_name']} lvl {p['level']} cap "
                       f"{p['capacity']:g} sea {p['sea'][:3]} region {p['region']}")
        out.append(f"capacity: {x['d']} [{', '.join(x['capacity_sources'])}]")
        for t in x["theatres"]:
            out.append("")
            out.append(f"vs {t['enemy']}: " + ("OVERSEAS (no land link from the capital)" if t.get("overseas") else
                                               ("land-linked to the capital" if t.get("overseas") is False else "?")))
            r = t.get("route")
            if r:
                out.append(f"  sea route port {r['from_port']} -> {r['to_port']}: {r['steps']} sea zones, {r['pixels']} px; "
                           f"rules {', '.join(x['rule'] for x in r['rules']) or 'none'}")
                for rr in t.get("rules", []):
                    out.append(f"  rule {rr['rule']}: navy closed when {'/'.join(rr['navy_closed_when']) or 'never'}; "
                               f"disabled if {rr.get('is_disabled') or '-'}; required provinces "
                               + ", ".join(f"{p} ({o})" for p, o in rr["required_provinces"].items()) + f" [{rr['src']}]")
                for alt in t.get("without", []):
                    out.append(f"  without {alt['avoid']}: {alt['steps']} sea zones, {alt['pixels']} px, rules "
                               f"{', '.join(alt['rules'] or []) or 'none'}" if alt["steps"] else
                               f"  without {alt['avoid']}: no sea route")
            elif t.get("note"):
                out.append("  " + t["note"])
            c = t.get("candidates") or {}
            if c.get("rows"):
                out.append(f"  coastal provinces within {c['steps']} steps of the front, naval base below {c['below_level']} "
                           f"({c['count']}):")
                for row in c["rows"]:
                    out.append(f"  - {row['province']} {row['name'] or ''} st {row['state']} {row['state_name']}: "
                               f"{row['steps']} steps, port lvl {row['level']}, infra {row['infrastructure']}, next level "
                               f"{row['next_level_cost']:g} CP, done {row['finish']} ({row['days']}d)"
                               + (f", hub {row['supply_hub']}" if row.get("supply_hub") else ""))
            elif c.get("note"):
                out.append("  " + c["note"])
        s = x["supply"]
        out.append("")
        out.append("supply: " + "; ".join(f"{k} {v['v']} [{v['src']}]" for k, v in s["defines"].items()))
        out.append("costs: " + "; ".join(f"{b} {c['base_cost']:g}" + (f" +{c['per_level_extra_cost']:g}/level"
                                                                      if c.get("per_level_extra_cost") else "")
                                         + f" [{c['src']}]" for b, c in s["costs"].items()))
        out.append("reach after 1-4 provinces: " + "; ".join(f"{k} {v}" for k, v in s["reach_1_to_4"].items() if v))
        out.append("method: " + s["d"])
        return out

    return emit(args, data, text)


# ---------------------------------------------------------------- wave 2: rules and catalogues (F-12 .. F-20)

def _nums(d, limit=6):
    """"k +v, k2 x-v" text of a {stat: number} dict (first `limit` items)."""
    return ", ".join(f"{k} {v:+g}" if isinstance(v, (int, float)) else f"{k} {v}" for k, v in list(d.items())[:limit])


def _gate(v, n=90):
    return "yes" if v is True else ("no" if v is False else str(v)[:n])


def _defs(d):
    return "; ".join(f"{k} {r['v']}" for k, r in d.items()) + (f" [{next(iter(d.values()))['src']}]" if d else "")


def fair_foreign(args, w):
    """True in FAIR mode when TAG is not our country (--tag or the manifest's): its private start state stays hidden."""
    ours = resolve(args)[3]
    return w.mode == "fair" and bool(ours) and args.TAG != ours


FAIR_DROPPED = "hidden in FAIR mode: private start state of another country (use --mode meta)"


def cmd_designs(args):
    w = world(args, args.TAG)
    if fair_foreign(args, w):
        return fail(f"designs of {args.TAG} follow its research and variants: {FAIR_DROPPED}")
    data = w.designs(args.TAG, args.kind)

    def text(x):
        c = x["costs"]
        out = [f"{x['tag']} designers at {x['date']} (start technologies at that date). XP per change: "
               + ", ".join(f"{k.replace('EQUIPMENT_MODULE_', '').replace('_XP_COST', '').lower()} {r['v']:g}"
                           for k, r in c.items() if k.startswith("EQUIPMENT_MODULE_"))
               + (f" [{c['EQUIPMENT_MODULE_ADD_XP_COST']['src']}]" if "EQUIPMENT_MODULE_ADD_XP_COST" in c else "")
               + "; a module's own xp_cost replaces add/replace [?]"]
        for k, kd in sorted(x["kinds"].items()):
            xp = kd.get("xp") or {}
            out.append("")
            out.append(f"== {k} designer: modules {'loaded' if kd['loaded'] else 'NOT loaded (DLC limit false)'}; "
                       f"{XP_NAME.get(k, k)} XP at start {xp.get('v')}")
            fam = {}
            for b in kd["bases"]:
                fam.setdefault(b["family"], []).append(b)
            for a, bs in sorted(fam.items()):
                own = [b for b in bs if b["archetype"] == a] or bs
                top = own[-1]
                slots = " ".join(f"{s['slot']}{'*' if s['required'] else ''}({'/'.join(s['categories'][:2])}"
                                 f"{'...' if len(s['categories']) > 2 else ''})" for s in top.get("slots") or [])
                ups = ", ".join(f"{u['id']} max {u['max_level']}" + (f" {u['cost_by_level']:g}xp/level" if u.get("cost_by_level")
                                                                    else "") for u in top["upgrades"])
                dup = sorted({b["archetype"] for b in bs} - {a})
                out.append(f"- {a}: {', '.join(b['id'] for b in own)} (newest {top['year']}, by {','.join(top['techs'][:2])})"
                           + (f"; same slots as {', '.join(dup)}" if dup else "") + f"; slots {slots}"
                           + (f"; upgrades {ups}" if ups else "") + f" [{top['src']}]")
            if kd["next_bases"]:
                out.append("  next bases: " + ", ".join(f"{b['id']} ({b['techs'][0]} {b['year']})" for b in kd["next_bases"][:10]))
            cats = {}
            for m in kd["modules"]:
                cats.setdefault(m["category"], []).append(m)
            out.append(f"modules unlocked ({len(kd['modules'])}; xp / dismantle IC / stats / resources):")
            for cat, ms in sorted(cats.items()):
                out.append(f"- {cat}: " + "; ".join(
                    f"{m['id']} {m['xp_cost']:g}xp" if m["xp_cost"] is not None else f"{m['id']} default-xp"
                    for m in ms) + " | " + "; ".join(
                    f"{m['id']}: {_nums(m['add_stats'], 4)}" + (f" x({_nums(m['multiply_stats'], 3)})" if m["multiply_stats"] else "")
                    + (f" res {_nums(m['build_cost_resources'], 3)}" if m["build_cost_resources"] else "")
                    for m in ms[:4]) + (f" [{ms[0]['src']}]"))
            if kd["next_modules"]:
                out.append("  next modules: " + ", ".join(f"{m['id']} ({m['techs'][0]} {m['year']})"
                                                         for m in kd["next_modules"][:12]))
            for v in kd["variants"]:
                out.append(f"  start variant \"{v['v']}\" {v['type']}"
                           + (": " + ", ".join(f"{s}={m}" for s, m in v["modules"].items()) if v["modules"] else "")
                           + (": " + ", ".join(f"{u} {n}" for u, n in v["upgrades"].items()) if v["upgrades"] else "")
                           + (" (obsolete)" if v["obsolete"] else "") + f" [{v['src']}]")
        out.append("method: " + x["d"])
        return out

    return emit(args, data, text)


XP_NAME = {"tank": "army", "plane": "air", "ship": "navy"}


def cmd_mio(args):
    w = world(args, args.TAG)
    data = w.mio(args.TAG)
    if args.id:
        data["organisations"] = [o for o in data["organisations"] if o["id"] == args.id]

    def trait_text(t):
        bonus = "; ".join(_nums(t[k], 4) for k in ("equipment_bonus", "production_bonus", "organization_modifier") if t.get(k))
        par = t["any_parent"] or t["all_parents"]
        return f"{t['token']}" + (f" <- {'/'.join(par)}" if par else "") + (f" x {','.join(t['mutually_exclusive'])}"
                                                                         if t["mutually_exclusive"] else "") + \
            (f": {bonus}" if bonus else "") + (f" (needs {t['available'][:60]})" if t.get("available") else "")

    def text(x):
        out = [f"{x['tag']} military-industrial organisations ({len(x['organisations'])}; * foreign MIO):"]
        for o in x["organisations"]:
            it = o.get("initial_trait") or {}
            ib = "; ".join(_nums(it[k], 4) for k in ("equipment_bonus", "production_bonus", "organization_modifier")
                           if it.get(k))
            out.append(f"- {'*' if o['foreign'] else ''}{o['id']} \"{o['name']}\": equipment {','.join(o['equipment_type'] or [])}; "
                       f"research {','.join(o['research_categories'] or [])}; initial {ib or '-'}; visible {_gate(o['visible'])}; "
                       f"available {_gate(o['available'])} [{o['src']}]")
            if args.id:
                for t in o["traits"]:
                    out.append(f"    {trait_text(t)}")
            else:
                out.append(f"    traits ({len(o['traits'])}): " + "; ".join(trait_text(t) for t in o["traits"][:6])
                           + (f"; ... (--id {o['id']})" if len(o["traits"]) > 6 else ""))
        out.append("policies (attach cost PP / cooldown days): " + "; ".join(
            f"{p['id']} {p['cost']}/{p['cooldown']}: " + "; ".join(_nums(b, 3) for b in p["bonuses"].values())
            for p in x["policies"]))
        out.append("defines: " + _defs(x["defines"]))
        out.append("method: " + x["d"])
        return out

    return emit(args, data, text)


def cmd_projects(args):
    w = world(args, args.TAG)
    data = w.projects(args.TAG)
    if fair_foreign(args, w):
        data["scientists"] = []
        data["scientists_note"] = FAIR_DROPPED

    def text(x):
        out = [f"{x['tag']} special projects ({len(x['projects'])} not statically excluded); facilities at start: "
               + (", ".join(f"{f['building']} st {f['state']} {f['name']}" for f in x["facilities_at_start"]) or "none")]
        for p in x["projects"]:
            cx = p["complexity"] or {}
            out.append(f"- {p['id']} \"{p['name']}\" ({(p['specialization'] or '').replace('specialization_', '')}"
                       f"{', ' + ','.join(p['tags']) if p['tags'] else ''}): prototype {p['prototype_days']}d, complexity "
                       f"{cx.get('min')}-{cx.get('max')}, breakthrough {_nums(p['breakthrough_cost'], 3) or '-'}"
                       + (f", resources {_nums(p['resources'], 4)}" if p["resources"] else "")
                       + (f", after {'+'.join(p['parent'])}" if p["parent"] else "")
                       + "; gives " + ("; ".join(f"{k} {','.join(v[:4])}" for k, v in p["outputs"].items() if v) or "effects")
                       + f"; available {_gate(p['available'], 100)} [{p['src']}]")
        out.append("scientists: " + ("; ".join(f"{s['id']} {_nums(s['skills'], 3)} {','.join(s['traits'])}"
                                              + (f" (visible {s['visible'][:50]})" if s.get("visible") else "")
                                              for s in x["scientists"]) or x.get("scientists_note")
                                     or "none in the tag's character files"))
        out.append("facility buildings: " + "; ".join(f"{b} {','.join(r['specialization'])}{' coastal' if r['coastal'] else ''} "
                                                      f"{r['cost']:g} +{r['per_controlled_cost'] or 0:g}/owned [{r['src']}]"
                                                      for b, r in x["facility_buildings"].items()))
        out.append("scientist traits: " + "; ".join(f"{t} ({_nums(r['modifier'], 3)})" for t, r in list(x["scientist_traits"].items())[:12])
                   + (f"; ... {len(x['scientist_traits']) - 12} more (--json)" if len(x["scientist_traits"]) > 12 else ""))
        out.append("defines: " + _defs(x["defines"]))
        out.append("method: " + x["d"])
        return out

    return emit(args, data, text)


def cmd_diplomacy(args):
    w = world(args, args.TAG)
    data = w.diplomacy(args.TAG)

    def text(x):
        wt = x["world_tension"]
        out = [f"{x['tag']} at {w.date}: ideology {x['ideology']} [{x['ideology_src']}]; rules "
               + (", ".join(f"{k} {v}" for k, v in x["rules"].items()) or "none set")
               + f"; war impact on world tension x{x['war_impact_on_world_tension']}",
               f"World tension at start: {wt['total']:g}% from history threats "
               + (", ".join(f"{t['tag']} {t['name']} {t['v']:g} [{t['src']}]" for t in wt["named"]) or "(none)")
               + "; wars at start: " + (", ".join(f"{r['attacker']}->{r['target']} {r['type']} (threat {r['threat']}) "
                                                  f"[{r['src']}]" for r in wt["wars"]) or "none"),
               "Gates (world tension needed = ideology + start spirits):"]
        for k, g in x["gates"].items():
            parts = ([f"{x['ideology']} {g['ideology']:g}"] if g["ideology"] is not None else []) + \
                [f"{s['idea']} {s['value']:+g} [{s['src']}]" for s in g["spirits"]]
            out.append(f"- {g['text']}: {'any' if g['need'] <= 0 else str(round(g['need'] * 100)) + '%'} "
                       f"({', '.join(parts) or 'no modifier: 0'})")
        js = [s for s in x["spirits"] if s["key"].startswith("justify")]
        if js:
            out.append("- justification time: " + ", ".join(f"{s['idea']} {s['key']} {s['value']:+g}" for s in js))
        out.append("War goals (justify cost base + per state; take states; expire; threat):")
        for g in x["wargoals"]:
            out.append(f"- {g['id']}: {g['generate_base_cost']} + {g['generate_per_state_cost']}/state"
                       + (f"; take up to {g['take_states_limit']} ({g['take_states_cost']:+g} each)" if g["take_states_limit"] else "")
                       + f"; expire {g['expire']}; threat {g['threat']}; allowed {_gate(g['allowed'])}"
                       + (f"; available {_gate(g['available'])}" if g["available"] is not True else "") + f" [{g['src']}]")
        out.append("defines: " + _defs(x["defines"]))
        out.append("method: " + x["d"] + "; " + wt["d"])
        return out

    return emit(args, data, text)


def cmd_factions(args):
    w = world(args, args.TAG)
    data = scenariomod.factions(w, args.TAG)

    def goal_text(g):
        r = g.get("ratio") or {}
        secs = "; ".join(f"{k} {v['min']}-{v['max'] if v['max'] is not None else ''}: {_nums(v['modifier'], 3)}"
                         for k, v in (g.get("sections") or {}).items())
        return f"{g['id']} \"{g.get('name')}\"" + (f" ({g.get('category')})" if g.get("category") else "") + \
            (f": {', '.join(f'{k} {v}' for k, v in r.items())}" if r else (f": {g.get('completed')[:90]}" if g.get("completed") else "")) + \
            (f"; {secs}" if secs else "")

    def text(x):
        out = [f"{x['tag']} factions: at start " + (", ".join(f"{f['kind']} {f['v']}" for f in x["faction_at_start"]) or "none")]
        for t in x["templates"]:
            if not t["created_by"] and t["selectable"] is not True:
                continue
            by = ", ".join(f"{c['kind']} {c['id']}" for c in t["created_by"][:3]) or "selectable when creating a faction"
            out.append(f"- {t['id']} \"{t['name']}\" ({by}) [{t['src']}]")
            out.append(f"    manifest {goal_text(t['manifest'])}")
            if t["goals"]:
                out.append("    goals " + "; ".join(goal_text(g) for g in t["goals"]))
            out.append("    rules " + "; ".join(f"{r['id']} ({r.get('type')})" + (f": {_nums(r.get('modifier') or {}, 3)}"
                                                                                 if r.get("modifier") else "")
                                             for r in t["default_rules"])
                       + (f"; available {t['available'][:80]}" if t.get("available") else ""))
        other = [t["id"] for t in x["templates"] if not t["created_by"] and t["selectable"] is None]
        if other:
            out.append("templates whose visibility needs the running game: " + ", ".join(other))
        if x["joins"]:
            out.append("join effects: " + "; ".join(f"{j['effect']} {j['target']} by {j['kind']} {j['id']}" for j in x["joins"][:8]))
        out.append("rule groups: " + "; ".join(f"{g['id']} (default {g['default']}): {','.join(g['rules'][:5])}"
                                              for g in x["rule_groups"]))
        mu = x["member_upgrades"]
        out.append("member upgrades: " + "; ".join(f"{g['id']} {g['type']}: " + ",".join(g["upgrades"]) for g in mu["groups"])
                   + " (" + ", ".join(f"{u['id']} {u['bonus']}" for u in mu["upgrades"]) + ")")
        out.append("defines: " + _defs(x["defines"]))
        out.append("method: " + x["d"])
        return out

    return emit(args, data, text)


def cmd_peace(args):
    w = world(args, args.TAG)
    data = w.peace(args.TAG, args.enemy or None)

    def text(x):
        out = [f"{x['tag']} peace-conference cost modifiers (enemies {','.join(x['enemies']) or 'none'}; multipliers "
               "multiply; < 1 cheaper):"]
        for rel in ("own", "enemy", "generic"):
            rows = [m for m in x["modifiers"] if m["relation"] == rel]
            if not rows:
                continue
            out.append(f"{rel} ({len(rows)}):")
            for m in rows[: (60 if rel != "generic" else 30)]:
                out.append(f"- {m['id']} {'/'.join(m['types'])} x{m['cost_multiplier']} ({m['category']}): "
                           f"{m['enable'][:140]} [{m['src']}]")
            if rel == "generic" and len(rows) > 30:
                out.append(f"- ... {len(rows) - 30} more (--json)")
        out.append("defines: " + _defs(x["defines"]))
        out.append("method: " + x["d"])
        return out

    return emit(args, data, text)


def cmd_command(args):
    w = world(args, args.TAG)
    data = w.command(args.TAG)
    if fair_foreign(args, w):
        data["command_power_at_start"] = None
        data["command_power_note"] = FAIR_DROPPED

    def text(x):
        cp = x["command_power_at_start"]
        cpt = cp["v"] if cp else (x.get("command_power_note") or "not set by the history (STARTING_COMMAND_POWER)")
        out = [f"{x['tag']} command: command power at start {cpt}; "
               "defines: " + _defs(x["defines"])]
        slots = {}
        for s in x["spirits"]:
            slots.setdefault((s["branch"], s["slot"], s["cost"]), []).append(s)
        out.append(f"Military spirits ({len(x['spirits'])}; cost in that branch's XP):")
        for (branch, slot, cost), ss in sorted(slots.items()):
            out.append(f"- {branch} {slot} {cost} XP: " + ", ".join(
                s["id"] + (f" (visible {_gate(s['visible'], 40)})" if s["visible"] not in (True, None) else "") for s in ss))
        out.append(f"Leader abilities ({len(x['abilities'])}; cost CP as in the file, duration h [?]):")
        for a in x["abilities"]:
            mods = "; ".join(_nums(m, 4) for m in a["modifiers"].values())
            out.append(f"- {a['id']} ({a['type']}) cost {a['cost']}, duration {a['duration']}"
                       + (f", cooldown {a['cooldown']}" if a["cooldown"] else "") + (f": {mods}" if mods else "")
                       + (f"; allowed {_gate(a['allowed'], 70)}" if a["allowed"] is not True else "") + f" [{a['src']}]")
        out.append(f"Assignable leader traits ({len(x['traits'])}; type row/col cost [?] parents x exclusive: modifiers):")
        for t in x["traits"]:
            par = t["any_parent"] or t["all_parents"] or t["parent"]
            mods = "; ".join(_nums(m, 3) for m in t["modifiers"].values())
            out.append(f"- {t['id']} {t['type']} {t['gui_row']}/{t['gui_column']} {t['cost']}"
                       + (f" <- {'/'.join(par)}" if par else "") + (f" x {t['mutually_exclusive']}" if t["mutually_exclusive"] else "")
                       + (f": {mods}" if mods else "") + (f"; ability {t['enable_ability']}" if t["enable_ability"] else ""))
        sk = {}
        for s in x["skills"]:
            sk.setdefault(s["type"], []).append(f"{s['level']}:{s['cost']}")
        out.append("leader skill level costs (XP): " + "; ".join(f"{k} {' '.join(v)}" for k, v in sk.items()))
        out.append("method: " + x["d"])
        return out

    return emit(args, data, text)


def cmd_agency(args):
    w = world(args)
    data = w.agency(w.tag or None)

    def text(x):
        out = [f"Intelligence agency: La Resistance {'active' if x['dlc_la_resistance'] else 'NOT active'}; "
               + (f"{x['tag']} agency at start: {x['agency_at_start']['v'] if x['agency_at_start'] else 'none'}; " if x["tag"] else "")
               + "defines: " + _defs(x["defines"])]
        branch = None
        for u in x["upgrades"]:
            if u["branch"] != branch:
                branch = u["branch"]
                out.append(f"{branch}:")
            out.append(f"- {u['id']} ({len(u['levels'])} levels: " + " | ".join(_nums(lv, 3) for lv in u["levels"][:4])
                       + (f"; while upgrading {_nums(u['during_progress'], 2)}" if u["during_progress"] else "")
                       + (f"; available {u['available'][:80]}" if u["available"] else "") + f") [{u['src']}]")
        out.append(f"Operations ({len(x['operations'])}; days, network %, operatives, risk, equipment, phase groups):")
        for o in x["operations"]:
            out.append(f"- {o['id']}: {o['days']}d, net {o['network_strength']}, {o['operatives']} op, risk {o['risk_chance']}"
                       + (f", equipment {_nums(o['equipment'], 3)}" if o["equipment"] else "")
                       + f", phases {len(o['phases'])}" + (f"; needs {o['requirements'][:80]}" if o["requirements"] else "")
                       + f" [{o['src']}]")
        out.append("method: " + x["d"])
        return out

    return emit(args, data, text)


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
    p = add("states", cmd_states, "TAG")
    p.add_argument("--energy", action="store_true", help="energy per state: coal, factories, energy buildings")
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
    p.add_argument("--never", action="store_true", help="only never-take decisions")
    p = add("missions", cmd_missions, "TAG")
    p.add_argument("--all", action="store_true")
    add("hooks", cmd_hooks, "TAG")
    add("event", cmd_event, "id")
    add("effect", cmd_effect, "name")
    add("resources", cmd_resources, "TAG")
    p = add("grep", cmd_grep, "token")
    p.add_argument("--limit", type=int, default=200)
    p.add_argument("--folder", action="append", help="limit to a content folder (repeat)")
    p.add_argument("--all-languages", action="store_true", help="also non-English localisation")
    p.add_argument("--context", "-C", type=int, default=0, help="lines of context before and after each hit")
    add("defines", cmd_defines, "KEY")
    p = add("terrain-width", cmd_terrain_width, ("target", {"nargs": "*", "help": "a TAG or state ids"}))
    p.add_argument("--route", nargs=2, type=int, metavar=("FROM", "TO"))
    p.add_argument("--enemy", action="append", help="attacker tag when TAG is ours (default: wars at start)")
    p = add("units", cmd_units)
    p.add_argument("--category", help="row, group, id or category substring (regimental, support, armor, ...)")
    p.add_argument("--unlocked-by", help="date: only sub-units the --tag has by then (start techs at that date)")
    p.add_argument("--all", action="store_true", help="also air and naval sub-units")
    p = add("weather", cmd_weather, "target")
    p.add_argument("--region", action="store_true", help="the number is a strategic region id")
    add("bop", cmd_bop, "TAG")
    add("occupation-laws", cmd_occupation_laws)
    p = add("ports", cmd_ports, "TAG")
    p.add_argument("--enemy", action="append")
    p.add_argument("--steps", type=int, default=3, help="land steps from the front for coastal candidates")
    p.add_argument("--below", type=int, default=3, help="list naval bases below this level as candidates")
    p = add("designs", cmd_designs, "TAG")
    p.add_argument("--kind", choices=("tank", "plane", "ship"))
    p = add("mio", cmd_mio, "TAG")
    p.add_argument("--id", help="one organisation with its whole trait tree")
    add("projects", cmd_projects, "TAG")
    add("diplomacy", cmd_diplomacy, "TAG")
    add("factions", cmd_factions, "TAG")
    p = add("peace", cmd_peace, "TAG")
    p.add_argument("--enemy", action="append", help="enemy tag (default: wars at start)")
    add("command", cmd_command, "TAG")
    add("agency", cmd_agency)
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
