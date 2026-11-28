"""Traceability lint: every plan entry says where it comes from, and the switches agree with the sections.

Rules (added to lint_plan.Lint by check()):
  trace.why        an entry of a plan section (a list of objects at the top level, or one level down:
                   intelligence.operations, navy.fleets, ...) without a `why`, or whose `why` names no
                   source. A source is a bracket tag ([G ...] guide, [R ...] research report, [V ...] game
                   file, [D ...] derived by a tool, [S ...] strategy review, [O ...] owner, [NONE ...] a
                   decision with no outside source, said so), a game file path (common/..., history/...,
                   map/...), a define name, or the words computed / owner / review / guide / engineer N.
                   One finding per section, listing the entries; a plan with a `goals` block (a plan
                   family generated from switches) gets warnings, any other plan info.
  trace.inert      an `_inert` row ({section, entry, needs, why}) without `needs` (what tool or engineer
                   the entry waits for) or without `why`; each well-formed row is listed as info so the
                   inert work stays visible.
  trace.goals      a `goals` row without a source, or a `constraints` switch without value or why.
  trace.airborne   `airborne.enabled` false while the plan still has paratrooper templates, drops or raids
                   outside `_inert`; enabled true in a FAIR plan without a roleplay goal or an owner switch
                   (owner, 28 Nov: "paratroopers yes, aren't considered for fair play").
  trace.volunteers a `volunteers` row without divisions (land) or planes (air), or an air row above 20% of
                   the planes the plan's air_plan fields (AIR_VOLUNTEER_PLANES_RATIO).

Facts: lint.facts["trace"] = {"entries": n, "with_source": n, "sections": {section: [missing ids]}}.

Stdlib only, Python 3.9+.
"""
import re

SOURCE = re.compile(
    r"\[(?:G|R|V|D|S|O|C|NONE)\b|(?i:\b(?:computed|owner|review|guide|engineer \d+|defines?)\b)"
    r"|(?:common|history|map|events|localisation)/|\b[A-Z][A-Z]+_[A-Z_]{3,}\b")
# Sections that hold free-form objects, not entries, or entries whose text lives elsewhere.
SKIP = {"_meta", "_validator", "_inert", "_fair", "goal", "goals", "constraints"}
# Lists of plain objects one level below a top-level object.
NESTED = {"intelligence": ("operations", "networks", "operatives", "upgrades"), "navy": ("fleets",),
          "airborne": ("drops", "raids")}
AIR_VOLUNTEER_PLANES_RATIO = 0.2


def has_source(text):
    return isinstance(text, str) and bool(SOURCE.search(text))


def _entries(plan):
    """[(section, label, entry)] of the entries the rule judges."""
    out = []
    for key, val in plan.items():
        if key in SKIP or key.startswith("_") and key != "_inert":
            continue
        if isinstance(val, list):
            for i, e in enumerate(val):
                if isinstance(e, dict):
                    out.append((key, str(e.get("id") or e.get("name") or e.get("idea") or i), e))
        elif isinstance(val, dict):
            for sub in NESTED.get(key, ()):
                rows = val.get(sub)
                for i, e in enumerate(rows if isinstance(rows, list) else []):
                    if isinstance(e, dict):
                        out.append(("%s.%s" % (key, sub), str(e.get("id") or e.get("fleet") or i), e))
    return out


def _paratroopers(plan):
    rows = []
    for t in plan.get("templates") or []:
        if not isinstance(t, dict):
            continue
        units = [u for col in t.get("regiments") or [] if isinstance(col, list) for u in col]
        if any(isinstance(u, str) and "paratrooper" in u for u in units):
            rows.append("templates %s" % (t.get("id") or t.get("name")))
    for r in plan.get("air_plan") or []:
        if isinstance(r, dict) and "paradrop" in str(r.get("mission") or ""):
            rows.append("air_plan %s" % r.get("id"))
    return rows


def check(lint):
    plan = lint.plan
    family = isinstance(plan.get("goals"), list) and bool(plan.get("goals"))
    sev = "warning" if family else "info"
    total = sourced = 0
    missing = {}
    for section, label, e in _entries(plan):
        total += 1
        if has_source(e.get("why")):
            sourced += 1
        else:
            missing.setdefault(section, []).append(label)
    for section, labels in sorted(missing.items()):
        lint.add(sev, "trace.why", section, "%d entr%s without a sourced why: %s" % (
            len(labels), "y" if len(labels) == 1 else "ies", ", ".join(labels[:12]) + (" ..." if len(labels) > 12 else "")))
    lint.facts["trace"] = {"entries": total, "with_source": sourced, "sections": missing}

    inert = plan.get("_inert") if isinstance(plan.get("_inert"), list) else []
    inert_ids = set()
    for i, r in enumerate(inert):
        if not isinstance(r, dict):
            continue
        where = "_inert[%d] %s" % (i, r.get("entry") or r.get("id") or "")
        inert_ids.add(str(r.get("entry") or r.get("id")))
        if not r.get("needs") or not r.get("why"):
            lint.add("warning", "trace.inert", where, "an inert entry names what it needs (`needs`) and why")
        else:
            lint.add("info", "trace.inert", where, "needs: %s" % r["needs"])

    for i, g in enumerate(plan.get("goals") or []):
        if isinstance(g, dict) and not g.get("source"):
            lint.add("warning", "trace.goals", "goals[%d] %s" % (i, g.get("id", "")),
                     "a goal says who set it (roleplay outranks FAIR)")
    cons = plan.get("constraints") if isinstance(plan.get("constraints"), dict) else {}
    for k, c in sorted(cons.items()):
        if not isinstance(c, dict) or "value" not in c or not c.get("why"):
            lint.add("warning", "trace.goals", "constraints.%s" % k, "a switch carries a value and a why")

    air = plan.get("airborne") if isinstance(plan.get("airborne"), dict) else None
    if air is not None:
        live = [x for x in _paratroopers(plan) if x.split(" ", 1)[1] not in inert_ids]
        live += ["airborne.%s %s" % (k, r.get("id")) for k in ("drops", "raids") for r in air.get(k) or []
                 if isinstance(r, dict) and str(r.get("id")) not in inert_ids]
        if not air.get("enabled") and live:
            lint.add("warning", "trace.airborne", "airborne", "drops are switched off but the plan still has: "
                     + ", ".join(live[:8]) + " (move them to _inert)")
        fair = (lint.facts.get("fair") or {}).get("mode", "fair") == "fair"
        roleplay = any(isinstance(g, dict) and g.get("source") == "roleplay" for g in plan.get("goals") or [])
        switch = cons.get(str(air.get("switch") or "airborne")) or {}
        if air.get("enabled") and fair and not roleplay and switch.get("source") not in ("roleplay", "owner"):
            lint.add("warning", "trace.airborne", "airborne", "paratroop drops in FAIR need a roleplay goal or an "
                     "owner switch (owner, 28 Nov: \"paratroopers yes, aren't considered for fair play\")")

    planes = sum(int(r.get("count") or 0) for r in plan.get("air_plan") or [] if isinstance(r, dict))
    for i, v in enumerate(plan.get("volunteers") or []):
        if not isinstance(v, dict):
            continue
        where = "volunteers[%d] %s" % (i, v.get("id", ""))
        if v.get("kind") == "air":
            if not v.get("planes"):
                lint.add("warning", "trace.volunteers", where, "an air volunteer row names its planes")
            elif planes and v["planes"] > AIR_VOLUNTEER_PLANES_RATIO * planes:
                lint.add("warning", "trace.volunteers", where, "%d planes is above 20%% of the %d the air plan "
                         "fields (AIR_VOLUNTEER_PLANES_RATIO)" % (v["planes"], planes))
        elif not v.get("divisions"):
            lint.add("warning", "trace.volunteers", where, "a land volunteer row names its divisions")
    return lint
