#!/usr/bin/env python3
"""pack.py: the strategist's work pack, computed once by code before the first model call.

    python3 scripts/strategist/pack.py --run RUN [--game DIR] [--userdir DIR] [--mod PATH ...] [--workers N]

A strategist used to spend its first 15 to 20 minutes and 80 turns fetching the same material one command at
a time: the facts every plan needs, the shape of each plan key (read from the 200 KB schema with small
Python one-liners) and what each lint rule wants (read from the lint source). Every later turn re-reads all of
it. The pack writes that material to RUN/pack/ in one step, so the model reads the parts it needs in a few
large reads:

  index.md          what is here, the command that made each file, its size and first line
  schema/INDEX.md   every top-level plan key: owner (executor, player, ...), type, one line
  schema/<key>.md   the fields of one key with types, enums, required marks and the schema descriptions
  lint-rules.md     every lint rule id with its severity and message templates (what it checks)
  facts/<name>.txt  the facts and calc outputs every plan needs, for the tag and its opponents

Nothing here is country specific: the commands come from the procedure, the opponents from the dossier
(enemies at start, neighbours, scenario hooks) and the run spec (tags and country names in the goal).
Deterministic; a rebuild is skipped when the dossier hash and the repo inputs are unchanged. Stdlib only.
"""
import argparse
import ast
import concurrent.futures
import datetime
import hashlib
import json
import os
import re
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
SCHEMA = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plan.schema.json")
FACTS = os.path.join(ROOT, "scripts", "hoi4facts", "facts.py")
CALC = os.path.join(ROOT, "scripts", "hoi4facts", "calc.py")
LINT_SOURCES = [os.path.join(ROOT, "scripts", "hoi4facts", n) for n in
                ("lint_plan.py", "lint_fair.py", "lint_horizons.py", "lint_gaps.py", "check_calls.py",
                 os.path.join("..", "validate_plan.py"))]
VERSION = 1
DESC_CHARS = 240  # one schema description in a key file
LINE_CHARS = 110  # one line in the schema index


# ---------------------------------------------------------------------------------------------- schema

def _first_sentence(text, limit):
    text = " ".join(str(text or "").split())
    if not text:
        return ""
    cut = re.split(r"(?<=[.;])\s", text, maxsplit=1)[0]
    if len(cut) > limit:
        cut = cut[:limit - 3].rstrip() + "..."
    return cut


def _refname(node):
    ref = node.get("$ref") if isinstance(node, dict) else None
    return ref.split("/")[-1] if ref and ref.startswith("#/definitions/") else None


def _resolve(node, defs):
    """The node with a local $ref followed, and the definition name (or None)."""
    name = _refname(node)
    if name:
        merged = dict(defs.get(name, {}))
        merged.update({k: v for k, v in node.items() if k != "$ref"})
        return merged, name
    return node, None


def linked_defs(schema):
    """Definitions written once to their own file and linked from every field that uses them: the large or
    shared ones (predicate, war, overrides, ...). Dates stay inline as `date`."""
    defs = schema.get("definitions") or {}
    text = json.dumps(schema.get("properties") or {}) + json.dumps(defs)
    out = set()
    for name, node in defs.items():
        if name.startswith("date"):
            continue
        uses, size = text.count('"#/definitions/%s"' % name), len(json.dumps(node))
        if name == "predicate" or size > 1500 or (uses >= 2 and size > 400):
            out.add(name)
    return out


def _type(node, defs, linked=()):
    rnode, ref = _resolve(node, defs)
    if ref and ref.startswith("date"):
        return "date"
    if ref and ref in linked:
        return "%s (schema/def-%s.md)" % (ref, ref)
    t = rnode.get("type")
    if isinstance(t, list):
        t = "|".join(t)
    if not t:
        for alt in ("oneOf", "anyOf"):
            if rnode.get(alt):
                t = "|".join(sorted({_type(a, defs, linked) for a in rnode[alt]}))
                break
    if t == "array":
        return "list of " + _type(rnode.get("items") or {}, defs, linked)
    if rnode.get("enum"):
        vals = [json.dumps(v) for v in rnode["enum"]]
        return "%s (%s)" % (t or "enum", ", ".join(vals[:14]) + (", ..." if len(vals) > 14 else ""))
    return t or (("ref " + ref) if ref else "any")


def _owner(node):
    return node.get("x-owner") or ("planned" if node.get("x-executor-planned") else "")


def _expandable(node, defs, linked):
    """The object whose properties a field lists inline (following arrays), or None for a leaf, a date or a
    linked definition."""
    for _ in range(3):
        name = _refname(node)
        if name and (name in linked or name.startswith("date")):
            return None
        node, _ = _resolve(node, defs)
        if node.get("properties"):
            return node
        if isinstance(node.get("items"), dict):
            node = node["items"]
            continue
        return None
    return None


def _fields(node, defs, linked, depth, prefix, out):
    obj = _expandable(node, defs, linked)
    if obj is None:
        return
    rnode, _ = _resolve(node, defs)
    if rnode.get("type") == "array" or ("items" in rnode and "properties" not in rnode):
        prefix += "[]"
    required = set(obj.get("required") or [])
    for name, sub in (obj.get("properties") or {}).items():
        rsub, _ = _resolve(sub, defs)
        path = (prefix + "." if prefix else "") + name
        out.append("- `%s` %s%s%s%s%s" % (path, _type(sub, defs, linked),
                                       " **required**" if name in required else "",
                                       " (deprecated)" if rsub.get("x-deprecated") else "",
                                       (" [%s]" % _owner(rsub)) if _owner(rsub) else "",
                                       (": " + _first_sentence(rsub.get("description"), DESC_CHARS))
                                       if rsub.get("description") else ""))
        if depth < 3:
            _fields(sub, defs, linked, depth + 1, path, out)


def _key_file(title, node, defs, linked, expand_self=True):
    rnode, _ = _resolve(node, defs)
    lines = ["# `%s`" % title, "", "Owner: %s. Type: %s." % (_owner(rnode) or "-", _type(node, defs, linked)), ""]
    if rnode.get("description"):
        lines += [" ".join(str(rnode["description"]).split()), ""]
    body = []
    if rnode.get("x-grammar") or rnode.get("x-atoms"):
        # the predicate language: its grammar and atom table say everything; its JSON shape is recursive
        grammar = rnode.get("x-grammar") or []
        body += ["Grammar:", "", "```"] + (grammar if isinstance(grammar, list) else [str(grammar)]) + ["```", ""]
        atoms = rnode.get("x-atoms") or {}
        if atoms:
            body += ["| atom | kind | arg | unit / values | read from | readable now |", "|---|---|---|---|---|---|"]
            for atom, a in atoms.items():
                a = a if isinstance(a, dict) else {}
                unit = a.get("unit") or ", ".join(str(v) for v in (a.get("values") or [])) or ""
                body.append("| `%s` | %s | %s | %s | %s | %s |" % (
                    atom, a.get("kind", ""), a.get("arg") or "", str(unit).replace("|", "/"),
                    str(a.get("read") or "").replace("|", "/"), "yes" if a.get("readable", True) else "no"))
        return "\n".join(lines + body).rstrip() + "\n"
    # a key that is only a reference to a linked definition is expanded here, in its own file
    target = rnode if expand_self else node
    _fields(target, defs, linked - {_refname(node)} if expand_self else linked, 0, "", body)
    return "\n".join(lines + body).rstrip() + "\n"


def schema_digest(schema):
    """{"INDEX.md": text, "<key>.md": text, "def-<name>.md": text, ...} from plan.schema.json."""
    defs = schema.get("definitions") or {}
    props = schema.get("properties") or {}
    linked = linked_defs(schema)
    files = {}
    index = ["# Plan keys (generated from skills/hoi4-campaign-playbook/plan.schema.json)", "",
             "Owner: executor = the executor runs it; player = a brief line with a call; planned = the executor",
             "step is not built yet (see the schema description). Read `schema/<key>.md` for the fields of the keys",
             "you write, several in one read (`cat RUN/pack/schema/{war,production}.md`). Shared shapes have their",
             "own file: %s. The schema file stays the contract: validate_plan.py checks against it."
             % ", ".join("`def-%s.md`" % n for n in sorted(linked)), "",
             "| key | owner | type | what it is |", "|---|---|---|---|"]
    for key, node in props.items():
        rnode, _ = _resolve(node, defs)
        index.append("| `%s` | %s | %s | %s |" % (key, _owner(rnode) or "-",
                                                  _type(node, defs, linked).split(" (")[0],
                                                  _first_sentence(rnode.get("description"), LINE_CHARS)
                                                  .replace("|", "/")))
        files[key + ".md"] = _key_file(key, node, defs, set(linked))
    for name in sorted(linked):
        files["def-%s.md" % name] = _key_file("definitions/" + name, defs[name], defs, set(linked) - {name})
    files["INDEX.md"] = "\n".join(index) + "\n"
    return files


# ------------------------------------------------------------------------------------------- lint rules

def _const_strings(tree):
    """Module and class level NAME = "string" assignments (rule ids kept in constants)."""
    out = {}
    for node in ast.walk(tree):
        if isinstance(node, ast.Assign) and isinstance(node.value, ast.Constant) and isinstance(node.value.value, str):
            for t in node.targets:
                if isinstance(t, ast.Name):
                    out[t.id] = node.value.value
    return out


def _text(node, consts):
    if isinstance(node, ast.Constant) and isinstance(node.value, str):
        return node.value
    if isinstance(node, ast.Name) and node.id in consts:
        return consts[node.id]
    if isinstance(node, ast.BinOp):  # "text %s" % (...) or "a " + "b"
        left = _text(node.left, consts)
        if isinstance(node.op, ast.Add):
            right = _text(node.right, consts)
            return (left or "") + (right or "...") if left else None
        return left
    if isinstance(node, ast.JoinedStr):
        parts = []
        for v in node.values:
            parts.append(v.value if isinstance(v, ast.Constant) else "{...}")
        return "".join(parts)
    if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute) and node.func.attr == "format":
        return _text(node.func.value, consts)
    return None


def lint_rules(paths):
    """{rule: {"severity": set, "messages": [..], "files": set}} from `.add(severity, rule, where, message)`
    calls in the lint sources."""
    rules = {}
    for path in paths:
        try:
            with open(path, encoding="utf-8") as f:
                tree = ast.parse(f.read())
        except (OSError, SyntaxError):
            continue
        consts = _const_strings(tree)
        for node in ast.walk(tree):
            if not (isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute)
                    and node.func.attr == "add" and len(node.args) >= 4):
                continue
            rule = _text(node.args[1], consts)
            if not rule or not re.match(r"^[a-z_]+(\.[a-z0-9_]+)+$", rule):
                continue
            sev = _text(node.args[0], consts) or "error|warning"
            msg = _text(node.args[3], consts)
            r = rules.setdefault(rule, {"severity": set(), "messages": [], "files": set()})
            r["severity"].add(sev)
            r["files"].add(os.path.basename(path))
            if msg:
                msg = " ".join(msg.split())
                if msg not in r["messages"]:
                    r["messages"].append(msg)
    return rules


def lint_rules_md(rules):
    lines = ["# Lint rules (generated from the lint sources)", "",
             "Each rule id with its severity and the message templates it prints (`%s` and `{...}` are filled in",
             "per finding). The source stays the contract; read it only when a message here does not say what the",
             "rule wants.", ""]
    for rule in sorted(rules):
        r = rules[rule]
        msgs = r["messages"][:4]
        more = len(r["messages"]) - len(msgs)
        lines.append("- `%s` (%s): %s%s" % (rule, "/".join(sorted(r["severity"])),
                                           " | ".join(m[:220] for m in msgs) or "-",
                                           " (+%d more)" % more if more > 0 else ""))
    return "\n".join(lines) + "\n"


# ------------------------------------------------------------------------------------------------ facts

def opponent_tags(spec, dossier, country_names=None, limit=12):
    """Tags the plan will likely fight or deal with: enemies at start, tags in scenario hook wars, tags and
    country names named in the goal, then land neighbours. Never the player's own tag."""
    own = spec.get("tag")
    out = []

    def add(t):
        if isinstance(t, str) and re.match(r"^[A-Z][A-Z0-9]{2}$", t) and t != own and t not in out:
            out.append(t)

    for row in (dossier.get("enemies") or []):
        add(row.get("tag") if isinstance(row, dict) else row)
    text = " ".join([spec.get("goal") or ""] + list(spec.get("goal_clarifications") or []))
    for t in re.findall(r"\b[A-Z][A-Z0-9]{2}\b", text):
        if country_names is None or t in country_names:
            add(t)
    lowered = text.lower()
    own_name = ((country_names or {}).get(own) or (dossier.get("country") or {}).get("name") or "").lower()
    for tag, name in sorted((country_names or {}).items()):
        if name and len(name) > 3 and name.lower() != own_name and re.search(r"\b%s\b" % re.escape(name.lower()), lowered):
            add(tag)
    own_hooks = ((dossier.get("hooks") or {}).get("own") or {})
    for war in own_hooks.get("wars") or []:
        if isinstance(war, dict):
            for k in ("enemy", "attacker", "defender", "tag"):
                add(war.get(k))
    for row in ((dossier.get("neighbours") or {}).get("rows") or []):
        add(row.get("tag"))
    return out[:limit]


def neighbour_names(dossier):
    return {r.get("tag"): r.get("name") for r in ((dossier.get("neighbours") or {}).get("rows") or [])
            if r.get("tag")}


def country_names(game_args):
    """{tag: English name} of every country tag in the game (and mods), or {} when the game cannot be read.
    The goal names countries in words ("Poland", "the Soviet Union"); this maps them to tags."""
    sys.path.insert(0, os.path.join(ROOT, "scripts"))
    try:
        from hoi4facts import derive, facts
        ns = argparse.Namespace(game=None, userdir=None, mod=[], dlc=None, tag=None, date=None, mode=None, out=None)
        it = iter(game_args)
        for a in it:
            if a == "--mod":
                ns.mod.append(next(it, None))
            elif a in ("--game", "--userdir"):
                setattr(ns, a[2:], next(it, None))
        game_dir, mods = facts.resolve(ns)[:2]
        game = facts.Game(game_dir, mods)
        table = derive.localisation(game)
        out = {}
        for tag in game.country_tags():
            name = table.get(tag) or table.get(tag + "_DEF")
            if name and "$" not in name:
                out[tag] = name.replace("the ", "").strip()
        return out
    except Exception:  # noqa: BLE001 - the pack still works from the dossier's neighbours
        return {}
    finally:
        sys.path.pop(0)


def _year(date):
    try:
        return int(str(date).split(".")[0])
    except ValueError:
        return 1936


def _add_years(date, years):
    parts = str(date).split(".")
    try:
        return "%d.%s.%s" % (int(parts[0]) + years, parts[1], parts[2])
    except (IndexError, ValueError):
        return date


def jobs(spec, opponents, game_args):
    """[(file name, argv, what it answers)] for the facts every plan needs."""
    tag, date, mode = spec["tag"], spec.get("start_date", "1936.1.1"), spec.get("mode", "fair")
    y = _year(date)
    f = [sys.executable, FACTS] + game_args + ["--date", date, "--mode", mode, "--tag", tag]
    c = [sys.executable, CALC]
    out = [
        ("decisions", f + ["decisions", tag], "decisions: cost, gates, timers (procedure items 1, 26)"),
        ("decisions_never", f + ["decisions", tag, "--never"], "never-take decisions"),
        ("missions", f + ["missions", tag], "missions with timers (checkpoints, item 27)"),
        ("hooks", f + ["hooks", tag], "scenario hooks: scripted wars, events, flags (items 1, 2, 26)"),
        ("laws", f + ["laws", tag], "laws with costs and gates (item 13)"),
        ("politics_slots", f + ["politics-slots"], "politics slot index for politics[].slot (items 13, 15)"),
        ("characters_advisor", f + ["characters", tag, "--role", "advisor"], "advisors, gates (item 15)"),
        ("characters_general", f + ["characters", tag, "--role", "general"], "generals (item 16)"),
        ("characters_marshal", f + ["characters", tag, "--role", "marshal"], "field marshals (item 16)"),
        ("resources", f + ["resources", tag], "resource balance at the start (items 8, 10)"),
        ("states", f + ["states", tag], "own states: slots, infrastructure, buildings (item 10, 19)"),
        ("oob", f + ["oob", tag], "own order of battle: templates, divisions, wings, fleets (items 6, 16, 20, 21)"),
        ("vp", f + ["vp", tag], "own VPs and surrender limit (defence, item 18.8)"),
        ("bop", f + ["bop", tag], "balance of power (item 12)"),
        ("mio", f + ["mio", tag], "military-industrial organisations (item 9)"),
        ("designs", f + ["designs", tag], "designers and design options (item 7)"),
        ("projects", f + ["projects", tag], "special projects (item 11)"),
        ("command", f + ["command", tag], "command power and high command (item 15, 16)"),
        ("agency", f + ["agency"], "intelligence agency upgrades and operations (item 24)"),
        ("diplomacy", f + ["diplomacy", tag], "diplomatic rules and gates (item 22)"),
        ("factions", f + ["factions", tag], "factions (item 22)"),
        ("peace", f + ["peace", tag], "peace conference costs (item 25)"),
        ("ports", f + ["ports", tag], "ports and sea routes (items 19, 21)"),
        ("occupation_laws", f + ["occupation-laws"], "occupation laws (item 23)"),
        ("units", f + ["units"], "land battalions and support companies (item 6)"),
        ("research_calendar", f + ["research-calendar", tag, "--years", "%d-%d" % (y, y + 2)],
         "startable techs by year (item 11)"),
        ("pp", c + ["pp", "--tag", tag, "--date", date, "--mode", mode, "--until", _add_years(date, 1)],
         "political power timeline, first year (items 13-15)"),
        ("xp_army", c + ["xp", "--tag", tag, "--date", date, "--mode", mode, "--branch", "army",
                         "--until", _add_years(date, 1)], "army XP forecast, first year (item 5)"),
    ]
    for t in opponents:
        out.append(("vp_%s" % t, f + ["vp", t], "%s VPs, surrender limit, capital (items 1, 18, 25)" % t))
        out.append(("oob_%s" % t, f + ["oob", t], "%s order of battle (FAIR: counts and templates)" % t))
    return out


def _run(job, timeout):
    name, argv, _ = job
    try:
        p = subprocess.run(argv, cwd=ROOT, capture_output=True, text=True, timeout=timeout,
                           encoding="utf-8", errors="replace")
        return name, p.returncode, (p.stdout or "") + (("\n[stderr]\n" + p.stderr) if p.returncode and p.stderr else "")
    except subprocess.TimeoutExpired:
        return name, 124, "timed out after %s s" % timeout


def _display(argv):
    out = []
    for a in argv:
        if a == sys.executable:
            out.append("python3")
        elif os.path.isabs(a) and a.startswith(ROOT):
            out.append(os.path.relpath(a, ROOT).replace(os.sep, "/"))
        else:
            out.append(a)
    return " ".join(out)


# ------------------------------------------------------------------------------------------------ build

def _sha(paths, extra=""):
    h = hashlib.sha256(extra.encode())
    for p in paths:
        try:
            with open(p, "rb") as f:
                h.update(f.read())
        except OSError:
            h.update(b"missing:" + p.encode())
    return h.hexdigest()[:20]


def read_json(path, default=None):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return default


def build(run_dir, game_args=(), workers=3, timeout=300, runner=None):
    """Write RUN/pack/. Returns the pack manifest. `runner(job, timeout)` replaces the subprocess (tests)."""
    run_dir = os.path.abspath(run_dir)
    spec = read_json(os.path.join(run_dir, "run_spec.json"), {})
    if not spec.get("tag"):
        raise SystemExit("pack: RUN/run_spec.json has no tag")
    dossier = read_json(os.path.join(run_dir, "dossier", "dossier.json"), {})
    dmanifest = read_json(os.path.join(run_dir, "dossier", "manifest.json"), {})
    pack = os.path.join(run_dir, "pack")
    key = _sha([SCHEMA, os.path.abspath(__file__)] + LINT_SOURCES,
               json.dumps([VERSION, spec, dmanifest.get("content_hash"), list(game_args)], sort_keys=True))
    old = read_json(os.path.join(pack, "manifest.json"), {})
    if old.get("key") == key and runner is None:
        return old
    os.makedirs(os.path.join(pack, "schema"), exist_ok=True)
    os.makedirs(os.path.join(pack, "facts"), exist_ok=True)

    schema = read_json(SCHEMA, {})
    for name, text in schema_digest(schema).items():
        _write(os.path.join(pack, "schema", name), text)
    _write(os.path.join(pack, "lint-rules.md"), lint_rules_md(lint_rules(LINT_SOURCES)))

    names = dict(neighbour_names(dossier))
    if runner is None:
        names.update(country_names(list(game_args)))
    opponents = opponent_tags(spec, dossier, names or None)
    todo = jobs(spec, opponents, list(game_args))
    run_one = runner or _run
    results = {}
    with concurrent.futures.ThreadPoolExecutor(max_workers=max(1, workers)) as ex:
        for name, code, text in ex.map(lambda j: run_one(j, timeout), todo):
            results[name] = (code, text)
    rows = []
    for name, argv, what in todo:
        code, text = results[name]
        path = os.path.join(pack, "facts", name + ".txt")
        _write(path, text)
        first = next((ln.strip() for ln in text.splitlines() if ln.strip()), "")
        rows.append({"file": "facts/%s.txt" % name, "command": _display(argv), "exit": code, "bytes": len(text.encode()),
                     "lines": text.count("\n"), "what": what, "first": first[:120]})
    manifest = {"version": VERSION, "key": key, "tag": spec["tag"], "opponents": opponents,
                "dossier_hash": dmanifest.get("content_hash"),
                "created": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
                "facts": rows}
    _write(os.path.join(pack, "index.md"), index_md(manifest))
    _write(os.path.join(pack, "manifest.json"), json.dumps(manifest, indent=1) + "\n")
    return manifest


def index_md(manifest):
    lines = ["# Work pack for %s (generated by scripts/strategist/pack.py)" % manifest["tag"], "",
             "Computed once by code before the strategist starts. Read from here instead of re-running these",
             "commands; run `facts.py` / `calc.py` only for what the pack does not hold (one state, a route, a",
             "focus or event, a timeline of your own plan). Cite a fact from a pack file with the command shown",
             "(the file is its output), for example `[V facts.py laws %s]`." % manifest["tag"], "",
             "* `schema/INDEX.md`: every plan key with its owner and a one-line meaning; `schema/<key>.md`: its fields.",
             "* `lint-rules.md`: every lint rule id and what it checks.",
             "* Opponents covered (enemies at start, scenario wars, the goal's countries, land neighbours): %s."
             % (", ".join(manifest.get("opponents") or []) or "none"), "",
             "| file | command | lines | what it answers | first line |", "|---|---|---|---|---|"]
    for r in manifest["facts"]:
        status = "" if r["exit"] == 0 else " (exit %s)" % r["exit"]
        lines.append("| `%s`%s | `%s` | %d | %s | %s |" % (r["file"], status, r["command"], r["lines"], r["what"],
                                                       r["first"].replace("|", "/")))
    return "\n".join(lines) + "\n"


def _write(path, text):
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(text)
    os.replace(tmp, path)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--run", required=True)
    ap.add_argument("--game")
    ap.add_argument("--userdir")
    ap.add_argument("--mod", action="append")
    ap.add_argument("--workers", type=int, default=3)
    args = ap.parse_args(argv)
    game_args = []
    if args.game:
        game_args += ["--game", args.game]
    if args.userdir:
        game_args += ["--userdir", args.userdir]
    for m in args.mod or []:
        game_args += ["--mod", m]
    m = build(args.run, game_args, workers=args.workers)
    bad = [r["file"] for r in m["facts"] if r["exit"] != 0]
    print(json.dumps({"pack": os.path.join(os.path.abspath(args.run), "pack"), "files": len(m["facts"]),
                      "opponents": m["opponents"], "failed": bad}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
