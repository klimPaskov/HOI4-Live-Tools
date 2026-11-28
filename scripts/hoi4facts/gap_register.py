#!/usr/bin/env python3
"""gap_register.py: the capability gap register. For every lever the project knows: can a plan ask for it, can a
tool carry it out in the game, and has that run live.

Usage:
    gap_register.py build [--out docs/GAP_REGISTER.md]    # write the register page from the data
    gap_register.py check                               # exit 1 with one line per problem (CI)
    gap_register.py blocked [--format md|json]          # the levers no tool carries yet (strategist, lint)

The data is skills/hoi4-campaign-playbook/gap-register.json: one row per capability with the playbook files
that teach it, the plan keys that ask for it, the tools that carry it out, how a tool refuses it (`blocked`),
where a live run is recorded (`verified`), the payoff for winning campaigns, what closing it takes and who owns
the open work. The status is computed, never written by hand, in this order:
  missing from playbook  no playbook file teaches it;
  not executable         no tool carries it, or the tool refuses this use (`blocked`);
  not plannable          no plan key with a reader (executor or player) asks for it, and the row does not say
                         why a plan never needs to (`plan_na`); a key whose executor step is not built yet
                         (x-executor-planned) or a text key does not count;
  unvalidated            no live run of this use is recorded (`verified` is null);
  ok                     everything above holds.
`check` fails when
  * the register page differs from what `build` writes (run `gap_register.py build`);
  * a row names a tool the catalogue (tools.schema.json, from `hoi4-live-native --list-tools`) does not list,
    or a plan key plan.schema.json does not list, or a playbook file that does not exist;
  * a playbook or strategist file names a tool the catalogue does not list;
  * a playbook line names a tool together with a use the register records as refused (`blocked_terms`) and does
    not say it is a gap (gap, blocked, refuses, not executable, unvalidated, not yet, register);
  * a tool's description says UNVALIDATED and no row with that tool is open, or a row records a live run while
    its tool still says UNVALIDATED (the descriptor or the row is stale);
  * a key whose executor step is not built (x-executor-planned) is in no open row (plan_keys.py check runs this);
  * an open row has no owner note, or a field holds a value outside its set.
Stdlib only, Python 3.9+.
"""
import argparse
import json
import os
import re
import sys
from collections import Counter, OrderedDict

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
if os.path.dirname(HERE) not in sys.path:
    sys.path.insert(0, os.path.dirname(HERE))

PLAYBOOK = os.path.join(ROOT, "skills", "hoi4-campaign-playbook")
DATA = os.path.join(PLAYBOOK, "gap-register.json")
CATALOGUE = os.path.join(PLAYBOOK, "tools.schema.json")
PAGE = os.path.join(ROOT, "docs", "GAP_REGISTER.md")
SKILLS = os.path.join(ROOT, "skills")
# Where a row's playbook file is looked up (a bare name, or a path from the repository root).
DOC_DIRS = (os.path.join(PLAYBOOK, "references"), PLAYBOOK, os.path.join(SKILLS, "hoi4-strategist", "references"),
            os.path.join(SKILLS, "hoi4-strategist"), SKILLS, ROOT)

DOMAINS = OrderedDict([
    ("politics", "Politics and laws"), ("advisors", "Advisors"), ("decisions", "Decisions"),
    ("focuses", "National focuses"), ("events", "Events"), ("diplomacy", "Diplomacy"), ("factions", "Factions"),
    ("puppets", "Puppets and subjects"), ("lendlease", "Lend-lease and volunteers"), ("peace", "Peace conferences"),
    ("occupation", "Occupation and resistance"), ("espionage", "Espionage"), ("trade", "Trade and resources"),
    ("construction", "Construction"), ("production", "Production"), ("designers", "Equipment designers"),
    ("mio", "Military-industrial organisations"), ("research", "Research"), ("doctrines", "Doctrines"),
    ("special_projects", "Special projects"), ("templates", "Division templates"),
    ("training", "Training and deployment"), ("formations", "Formations and commanders"),
    ("orders", "Battle plans and orders"), ("supply", "Supply and logistics"), ("air", "Air"), ("navy", "Navy"),
])
STATUSES = ("missing from playbook", "not executable", "not plannable", "unvalidated", "ok")
OPEN = set(STATUSES) - {"ok"}
PAYOFFS = ("high", "medium", "low")
CLOSING = ("offline", "live", "both", "none")
REQUIRED = ("id", "domain", "capability", "playbook", "plan", "tools", "payoff", "why", "closing")
FIELDS = set(REQUIRED) | {"blocked", "blocked_terms", "verified", "owner", "plan_na", "lever_kind", "priority",
                          "evidence", "note"}
PLANNABLE_READERS = ("executor", "player")
GAP_WORDS = re.compile(r"\bgaps?\b|blocked|refuse|not executable|unvalidated|not yet|register|no tool",
                       re.I)
TOOL_TOKEN = re.compile(r"\bhoi4_[a-z0-9_]+\b")
# Names in the skills that are placeholders, not tools.
PLACEHOLDERS = {"hoi4_x", "hoi4_live"}  # a placeholder in a usage line; the Python package


def load(path=DATA):
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    return data["rows"]


def catalogue(path=CATALOGUE):
    with open(path, encoding="utf-8") as f:
        return json.load(f)["tools"]


def readers():
    from hoi4facts import plan_keys
    return plan_keys.readers()


def unvalidated_tools(cat):
    return {name for name, t in cat.items() if "UNVALIDATED" in (t.get("description") or "")}


def status(row, rdr=None):
    """The row's status (STATUSES), from its fields and the plan keys' readers."""
    rdr = readers() if rdr is None else rdr
    if not row.get("playbook"):
        return "missing from playbook"
    if not row.get("tools") or row.get("blocked"):
        return "not executable"
    if not row.get("plan_na") and not any(rdr.get(k) in PLANNABLE_READERS for k in row.get("plan") or ()):
        return "not plannable"
    if not row.get("verified"):
        return "unvalidated"
    return "ok"


def annotate(rows, rdr=None):
    rdr = readers() if rdr is None else rdr
    return [dict(r, status=status(r, rdr)) for r in rows]


def blocked(rows=None, rdr=None):
    """Rows no tool carries yet (status not executable), for the strategist and the lint."""
    rows = annotate(load() if rows is None else rows, rdr)
    return [r for r in rows if r["status"] == "not executable"]


def lever_tool_gaps(rows=None):
    """{lever kind: why no tool carries it} for the capitulation levers the lint lists (conquest.tool_gap)."""
    out = {}
    try:
        rows = load() if rows is None else rows
    except (OSError, ValueError, KeyError):
        return out
    for r in rows:
        kind = r.get("lever_kind")
        if kind and (r.get("blocked") or not r.get("tools")):
            out[kind] = "%s (gap register %s)" % (r.get("blocked") or "no tool carries it", r["id"])
    return out


def blocked_keys(rows=None, rdr=None):
    """{plan key path: [rows]} for every not-executable row with plan keys: a plan that sets one relies on a lever
    no tool carries (lint gap.blocked)."""
    out = {}
    for r in blocked(rows, rdr):
        for k in r.get("plan") or ():
            out.setdefault(k, []).append(r)
    return out


def _md(text):
    return str(text).replace("|", "\\|").replace("\n", " ")


def _plannable(row, rdr):
    if row.get("plan_na"):
        return "not needed: " + row["plan_na"]
    if not row.get("plan"):
        return "none"
    return ", ".join("`%s` (%s)" % (k, rdr.get(k, "unknown")) for k in row["plan"])


def _executable(row):
    tools = ", ".join("`%s`" % t for t in row.get("tools") or ()) or "none"
    if row.get("blocked"):
        tools += "; refused: " + row["blocked"]
    return tools


def top_open(rows, n=10):
    """The open rows that matter most: an explicit priority first, then payoff, then status order."""
    rank = {p: i for i, p in enumerate(PAYOFFS)}
    st = {s: i for i, s in enumerate(STATUSES)}
    cands = [r for r in rows if r["status"] in OPEN]
    cands.sort(key=lambda r: (r.get("priority") or 999, rank[r["payoff"]], st[r["status"]], r["id"]))
    return cands[:n]


def render(rows=None, rdr=None):
    rdr = readers() if rdr is None else rdr
    rows = annotate(load() if rows is None else rows, rdr)
    by_status = Counter(r["status"] for r in rows)
    lines = [
        "# Gap register",
        "",
        "Generated by `python scripts/hoi4facts/gap_register.py build` from",
        "`skills/hoi4-campaign-playbook/gap-register.json`; edit the data, not this page. CI fails when the page is",
        "stale, when a playbook names a tool that does not exist or a use a tool refuses without saying so, when an",
        "UNVALIDATED tool is in no open row, and when an open row has no owner.",
        "",
        "For every lever the project knows: can a plan ask for it (plannable: the schema key and who reads it), can",
        "a tool carry it out in the game (executable), and has that run live (verified). A status is computed: the",
        "first of missing from playbook, not executable, not plannable, unvalidated that holds, else ok.",
        "",
        "## Summary",
        "",
        "%d capabilities: " % len(rows) + ", ".join("%s %d" % (s, by_status.get(s, 0)) for s in STATUSES) + ".",
        "",
        "| Domain | " + " | ".join(STATUSES) + " | total |",
        "|---|" + "---:|" * (len(STATUSES) + 1),
    ]
    for d, label in DOMAINS.items():
        drows = [r for r in rows if r["domain"] == d]
        if not drows:
            continue
        c = Counter(r["status"] for r in drows)
        lines.append("| %s | " % label + " | ".join(str(c.get(s, 0) or "") for s in STATUSES)
                     + " | %d |" % len(drows))
    lines += ["", "### The ten open gaps that matter most for winning campaigns", "",
              "| # | Capability | Status | Why it matters | Closing | Owner |", "|---|---|---|---|---|---|"]
    for i, r in enumerate(top_open(rows), 1):
        lines.append("| %d | %s (`%s`) | %s | %s | %s | %s |" % (
            i, _md(r["capability"]), r["id"], r["status"], _md(r["why"]), r["closing"], _md(r.get("owner") or "")))
    lines += ["", "## Register", ""]
    for d, label in DOMAINS.items():
        drows = [r for r in rows if r["domain"] == d]
        if not drows:
            continue
        lines += ["### " + label, "",
                  "| Capability | Playbook | Plannable | Executable | Verified live | Status | Payoff | Closing |",
                  "|---|---|---|---|---|---|---|---|"]
        for r in sorted(drows, key=lambda r: (STATUSES.index(r["status"]), PAYOFFS.index(r["payoff"]), r["id"])):
            owner = (" Owner: " + r["owner"]) if r["status"] in OPEN and r.get("owner") else ""
            lines.append("| %s (`%s`) | %s | %s | %s | %s | %s | %s: %s | %s.%s |" % (
                _md(r["capability"]), r["id"], _md(", ".join(r["playbook"]) or "none"), _md(_plannable(r, rdr)),
                _md(_executable(r)), _md(r.get("verified") or "no"), r["status"], r["payoff"], _md(r["why"]),
                r["closing"], _md(owner)))
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def skill_files():
    for dirpath, _, files in os.walk(SKILLS):
        for name in sorted(files):
            if name.endswith((".md", ".json", ".jsonl", ".txt", ".yaml")) and name != "tools.schema.json" \
                    and name != "gap-register.json":
                yield os.path.join(dirpath, name)


GAPS_START = re.compile(r"^\W*(\w+ )?(tool )?gaps\b", re.I)


def _headings(lines):
    """The context of each line (0-based index -> text): the nearest heading above it, or the "Gaps:" line that opens
    the list it belongs to (a list runs to the next blank line or heading)."""
    out, cur, gaps, prev = [], "", "", ""
    for line in lines:
        if line.startswith("#"):
            cur, gaps = line, ""
        elif GAPS_START.match(line):
            gaps = line
        elif not line.strip() and prev is not gaps:
            gaps = ""  # the blank line right after "Gaps:" still opens its list
        out.append(gaps or cur)
        prev = line
    return out


def _rel(path):
    """The path relative to the repository, or as given when it is on another drive (Windows temp dirs)."""
    try:
        return os.path.relpath(path, ROOT)
    except ValueError:
        return path


def check(rows=None, cat=None, rdr=None, page=PAGE, files=None):
    """Problems as [str]; [] when the register is complete and current."""
    rows = load() if rows is None else rows
    cat = catalogue() if cat is None else cat
    rdr = readers() if rdr is None else rdr
    problems = []
    ids = Counter(r.get("id") for r in rows)
    problems += ["%s: the id is used %d times" % (i, n) for i, n in ids.items() if n > 1]
    for r in rows:
        rid = r.get("id", "?")
        missing = [k for k in REQUIRED if k not in r]
        if missing:
            problems.append("%s: missing %s" % (rid, ", ".join(missing)))
            continue
        unknown = set(r) - FIELDS
        if unknown:
            problems.append("%s: unknown fields %s" % (rid, ", ".join(sorted(unknown))))
        if r["domain"] not in DOMAINS:
            problems.append("%s: domain %r is not one of %s" % (rid, r["domain"], ", ".join(DOMAINS)))
        if r["payoff"] not in PAYOFFS:
            problems.append("%s: payoff %r is not one of %s" % (rid, r["payoff"], ", ".join(PAYOFFS)))
        if r["closing"] not in CLOSING:
            problems.append("%s: closing %r is not one of %s" % (rid, r["closing"], ", ".join(CLOSING)))
        for t in r["tools"]:
            if t not in cat:
                problems.append("%s: tool %s is not in the catalogue" % (rid, t))
        for k in r["plan"]:
            if k not in rdr:
                problems.append("%s: plan key %s is not in plan.schema.json" % (rid, k))
        for ref in r["playbook"]:
            path = ref.split("#", 1)[0]
            if not any(os.path.exists(os.path.join(d, path)) for d in DOC_DIRS):
                problems.append("%s: playbook file %s does not exist" % (rid, path))
        st = status(r, rdr)
        if st in OPEN:
            if not (r.get("owner") or "").strip():
                problems.append("%s: open (%s) with no owner note" % (rid, st))
            if r["closing"] == "none":
                problems.append("%s: open (%s) but closing is none" % (rid, st))
        stale = [t for t in r["tools"] if t in cat and "UNVALIDATED" in (cat[t].get("description") or "")]
        if r.get("verified") and stale and not r.get("blocked"):
            problems.append("%s: a live run is recorded (%s) but %s still says UNVALIDATED: update the descriptor "
                            "or the row" % (rid, r["verified"], ", ".join(stale)))
        if r.get("blocked_terms") and not r.get("blocked"):
            problems.append("%s: blocked_terms without blocked" % rid)
    open_tools = {t for r in rows if status(r, rdr) in OPEN for t in r.get("tools") or ()}
    for t in sorted(unvalidated_tools(cat) - open_tools):
        problems.append("%s: its description says UNVALIDATED but no open register row names it" % t)
    # The playbook and the strategist only name tools that exist, and never a refused use without saying so.
    refused = [(r, t, [w.lower() for w in r.get("blocked_terms") or ()]) for r in rows if r.get("blocked")
               for t in r["tools"]]
    for path in (skill_files() if files is None else files):
        with open(path, encoding="utf-8") as f:
            lines = f.read().splitlines()
        heads = _headings(lines)
        rel = _rel(path)
        for i, line in enumerate(lines):
            for name in set(TOOL_TOKEN.findall(line)):
                if name not in cat and name not in PLACEHOLDERS:
                    problems.append("%s:%d: names %s, which is not in the tool catalogue" % (rel, i + 1, name))
            low = line.lower()
            for r, t, terms in refused:
                if t in line and any(w in low for w in terms) and not GAP_WORDS.search(line) \
                        and not GAP_WORDS.search(heads[i]):
                    problems.append("%s:%d: names %s for a use it refuses (%s) without saying it is a gap "
                                    "(register %s)" % (rel, i + 1, t, r["blocked"][:80], r["id"]))
    if page is not None:
        want = render(rows, rdr)
        have = ""
        if os.path.exists(page):
            with open(page, encoding="utf-8") as fh:
                have = fh.read()
        if have != want:
            problems.append("%s is stale: run python scripts/hoi4facts/gap_register.py build"
                            % _rel(page))
    return problems


def planned_problems(keys, rows=None, rdr=None):
    """For plan_keys.py check: every key whose executor step is not built (x-executor-planned) is in an open row."""
    try:
        rows = load() if rows is None else rows
    except (OSError, ValueError, KeyError):
        return [(k, "not in the gap register (skills/hoi4-campaign-playbook/gap-register.json is unreadable)")
                for k in keys]
    rdr = readers() if rdr is None else rdr
    covered = {k for r in rows if status(r, rdr) in OPEN for k in r.get("plan") or ()}
    return [(k, "its executor step is not built (x-executor-planned) and no open gap-register row names it")
            for k in sorted(keys) if k not in covered]


def blocked_markdown(rows=None, rdr=None):
    rows = blocked(rows, rdr)
    out = ["# Levers no tool carries yet (gap register: not executable)", "",
           "Generated from skills/hoi4-campaign-playbook/gap-register.json. Never drop one of these from the plan",
           "because no tool carries it: when the goal would use it, plan it (the key named, `tool_gap` on a",
           "capitulation lever), write it in RUN/tool-gaps.md with its payoff for this goal, and plan the best",
           "workaround the tools do carry. The lint warns (gap.blocked) on a plan key that relies on one.", ""]
    for r in sorted(rows, key=lambda r: (PAYOFFS.index(r["payoff"]), r["domain"], r["id"])):
        keys = ", ".join(r.get("plan") or ()) or "no plan key"
        out.append("- `%s` (%s, payoff %s): %s. Plan key: %s. Why blocked: %s. Owner: %s." % (
            r["id"], DOMAINS[r["domain"]], r["payoff"], r["capability"], keys,
            r.get("blocked") or "no tool carries it", r.get("owner") or "none"))
    return "\n".join(out) + "\n"


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("cmd", choices=("build", "check", "blocked"))
    ap.add_argument("--out", default=PAGE)
    ap.add_argument("--format", choices=("md", "json"), default="md")
    a = ap.parse_args(argv)
    if a.cmd == "build":
        text = render()
        with open(a.out, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
        print("wrote %s" % _rel(a.out))
        return 0
    if a.cmd == "blocked":
        if a.format == "json":
            print(json.dumps([{k: r.get(k) for k in ("id", "domain", "capability", "plan", "tools", "blocked",
                                                      "payoff", "owner")} for r in blocked()], indent=1))
        else:
            sys.stdout.write(blocked_markdown())
        return 0
    problems = check()
    for p in problems:
        print(p)
    rows = annotate(load())
    print("gap register: %d capabilities, %d open, %d problems" % (
        len(rows), sum(r["status"] in OPEN for r in rows), len(problems)))
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
