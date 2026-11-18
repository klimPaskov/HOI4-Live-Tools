"""Playbook references (rule playbook.refs), run by lint_plan.py (Lint.run) on every plan.

A plan's force, economy, air and political decisions must follow the project's research, the owner's guides and
the playbook built from them, or say why they depart. So each decision entry names its source in its `why` (or
`note`):

  [P <reference file>#<row id>]   a playbook reference row (skills/hoi4-campaign-playbook/references/<file>);
                                  the file must exist and the row id must appear in it
  [R <report> <row id>]           a research row (the private research reports; the format is checked)
  [G <guide> <section>]           a section of one of the owner's guides (the format is checked)
  [D <row id>]                    a row of the run's sourced decision table (RUN/decisions.md, written by the
                                  pipeline's decisions step); the row must exist in that table
  [NONE: <reason>]                a deliberate departure from the playbook, with its reason

  playbook.refs       an entry with no source (error), a [P ...] reference to a file or row that does not exist,
                      or a [D ...] reference with no decision table or no such row (error). The rubric scores a
                      decision that contradicts or ignores the playbook without a reason.
  decisions.coverage  the restore-or-justify pass, when the plan has a decision table: every row of the table is
                      cited by the plan ([D row] anywhere in plan.json) or listed in _meta.decision_departures
                      with a reason (error otherwise); a departure whose reason is a lint ("a lint failed", "to pass
                      the lint") is an error: the reason must be a game fact or arithmetic; a departure for a row the
                      table does not have is an error.
"""
import os
import re

REF = re.compile(r"\[(P|R|G)\s+([^\]\s#]+)(?:#|\s+)([^\]]+)\]|\[(D)\s+(D-[A-Z]+-\d{2}[a-z]?)\s*\]"
                 r"|\[NONE:\s*[^\]]{3,}\]")
# A decision table row: | D-<AREA>-<nn> | ... (decisions.md, the strategist pipeline's decisions step)
DECISION_ROW = re.compile(r"(?m)^\|\s*(D-[A-Z]+-\d{2}[a-z]?)\s*\|")
DECISION_CITE = re.compile(r"\[D\s+(D-[A-Z]+-\d{2}[a-z]?)\s*\]")
# A departure reason that names a lint instead of a game fact or arithmetic.
LINT_REASON = re.compile(r"\blint(s|ed|ing)?\b|\bgates?\s+(failed|fail)\b|\bto pass\b", re.I)
REFS_DIR = os.path.join(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))),
                        "skills", "hoi4-campaign-playbook", "references")

# The decision keys: (path label, how to list the entries of the plan)
DECISION_KEYS = ("templates", "production", "politics", "officer_corps", "mio", "designs", "doctrines", "air_plan",
                 "research_path")


def entries(plan):
    """(label, entry) for every decision entry of the plan."""
    for key in DECISION_KEYS:
        v = plan.get(key)
        if isinstance(v, list):
            for i, e in enumerate(v):
                if isinstance(e, dict):
                    yield "%s[%d]" % (key, i), e
    auto = plan.get("politics_auto")
    if isinstance(auto, dict):
        for i, e in enumerate(auto.get("order") or []):
            if isinstance(e, dict):
                yield "politics_auto.order[%d]" % i, e


_cache = {}


def _reference_text(name):
    if name not in _cache:
        path = os.path.join(REFS_DIR, name if name.endswith(".md") else name + ".md")
        try:
            with open(path, encoding="utf-8") as f:
                _cache[name] = f.read()
        except OSError:
            _cache[name] = None
    return _cache[name]


def decision_rows(text):
    """The row ids of a decision table text (ordered, unique)."""
    return list(dict.fromkeys(DECISION_ROW.findall(text or "")))


def problems(text, table=None):
    """None when the text carries a valid source; else why not. table: the decision table's row ids (a set) or
    None when the plan has no table."""
    text = str(text or "")
    found = list(REF.finditer(text))
    if not found:
        return "no source"
    for m in found:
        if m.group(4) == "D":
            if table is None:
                return "[D %s] but the plan has no decision table (RUN/decisions.md)" % m.group(5)
            if m.group(5) not in table:
                return "the decision table has no row %s" % m.group(5)
            continue
        if m.group(1) == "P":
            ref = _reference_text(m.group(2))
            if ref is None:
                return "the playbook has no reference %s" % m.group(2)
            row = m.group(3).strip()
            if row not in ref:
                return "%s has no row %s" % (m.group(2), row)
    return None


def coverage(lint, rows):
    """decisions.coverage: every table row is cited or departed from with a fact or arithmetic reason."""
    import json
    plan = lint.plan
    meta = plan.get("_meta") if isinstance(plan.get("_meta"), dict) else {}
    departures = meta.get("decision_departures") or []
    cited = set(DECISION_CITE.findall(json.dumps({k: v for k, v in plan.items() if k != "_meta"})))
    departed = {}
    for i, d in enumerate(departures):
        if not isinstance(d, dict):
            continue
        row, reason = str(d.get("row") or ""), str(d.get("reason") or "")
        where = "_meta.decision_departures[%d]" % i
        if row not in rows:
            lint.add("error", "decisions.coverage", where, "%s: the decision table has no row %r" % (where, row))
            continue
        if LINT_REASON.search(reason):
            lint.add("error", "decisions.coverage", where,
                     "%s (%s): a lint is not a reason to drop a table decision; change the force, lines or dates "
                     "until it holds, or give the game fact or arithmetic that forbids it" % (where, row))
        elif len(reason.strip()) < 20:
            lint.add("error", "decisions.coverage", where,
                     "%s (%s): the reason must name the game fact or arithmetic that forbids the decision" % (where, row))
        departed[row] = reason
    left = [r for r in rows if r not in cited and r not in departed]
    if left:
        lint.add("error", "decisions.coverage", "decisions.md",
                 "%d of %d decision table rows are neither followed ([D row] in an entry's why) nor departed from "
                 "(_meta.decision_departures: row, reason): %s%s" % (
                     len(left), len(rows), ", ".join(left[:12]), "..." if len(left) > 12 else ""))
    facts = getattr(lint, "facts", None)
    if isinstance(facts, dict):
        facts["decisions"] = {"rows": len(rows), "cited": len(cited & set(rows)), "departed": len(departed)}


def check(lint):
    """Adds playbook.refs (and, with a decision table, decisions.coverage) findings to `lint` (a lint_plan.Lint)."""
    text = getattr(lint, "decisions", None)
    rows = decision_rows(text) if text is not None else None
    table = set(rows) if rows is not None else None
    if rows is not None:
        coverage(lint, rows)
    missing = []
    for label, e in entries(lint.plan):
        why = " ".join(str(e.get(k) or "") for k in ("why", "note", "source"))
        p = problems(why, table)
        if p == "no source":
            missing.append(label)
        elif p:
            lint.add("error", "playbook.refs", label, "%s: %s" % (label, p))
    if missing:
        lint.add("error", "playbook.refs", missing[0],
                 "%d decision entries name no source (%s%s): each force, economy, air and political decision "
                 "names the playbook row it follows ([P file#row]), a decision table row ([D row]), a research row "
                 "([R report row]) or a guide section ([G guide section]), or says why it departs ([NONE: reason])" % (
                     len(missing), ", ".join(missing[:8]), "..." if len(missing) > 8 else ""))
