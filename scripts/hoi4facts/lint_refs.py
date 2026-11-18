"""Playbook references (rule playbook.refs), run by lint_plan.py (Lint.run) on every plan.

A plan's force, economy, air and political decisions must follow the project's research, the owner's guides and
the playbook built from them, or say why they depart. So each decision entry names its source in its `why` (or
`note`):

  [P <reference file>#<row id>]   a playbook reference row (skills/hoi4-campaign-playbook/references/<file>);
                                  the file must exist and the row id must appear in it
  [R <report> <row id>]           a research row (the private research reports; the format is checked)
  [G <guide> <section>]           a section of one of the owner's guides (the format is checked)
  [NONE: <reason>]                a deliberate departure from the playbook, with its reason

  playbook.refs  an entry with no source (error), or a [P ...] reference to a file or row that does not exist
                 (error). The rubric scores a decision that contradicts or ignores the playbook without a reason.
"""
import os
import re

REF = re.compile(r"\[(P|R|G)\s+([^\]\s#]+)(?:#|\s+)([^\]]+)\]|\[NONE:\s*[^\]]{3,}\]")
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


def problems(text):
    """None when the text carries a valid source; else why not."""
    text = str(text or "")
    found = list(REF.finditer(text))
    if not found:
        return "no source"
    for m in found:
        if m.group(1) == "P":
            ref = _reference_text(m.group(2))
            if ref is None:
                return "the playbook has no reference %s" % m.group(2)
            row = m.group(3).strip()
            if row not in ref:
                return "%s has no row %s" % (m.group(2), row)
    return None


def check(lint):
    """Adds playbook.refs findings to `lint` (a lint_plan.Lint)."""
    missing = []
    for label, e in entries(lint.plan):
        why = " ".join(str(e.get(k) or "") for k in ("why", "note", "source"))
        p = problems(why)
        if p == "no source":
            missing.append(label)
        elif p:
            lint.add("error", "playbook.refs", label, "%s: %s" % (label, p))
    if missing:
        lint.add("error", "playbook.refs", missing[0],
                 "%d decision entries name no source (%s%s): each force, economy, air and political decision "
                 "names the playbook row it follows ([P file#row]), a research row ([R report row]) or a guide "
                 "section ([G guide section]), or says why it departs ([NONE: reason])" % (
                     len(missing), ", ".join(missing[:8]), "..." if len(missing) > 8 else ""))
