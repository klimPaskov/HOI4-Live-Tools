"""Gap-register checks (rule gap.blocked), run by lint_plan.py (Lint.run) on every plan.

A plan may need a lever no tool carries yet (the gap register's "not executable" rows,
skills/hoi4-campaign-playbook/gap-register.json). Dropping it hides the gap; planning it as if the executor
or the player could carry it out hides it too. So every plan key that relies on such a lever is reported:

  gap.blocked  (warning) the plan sets a key of a not-executable register row (and, when the row names the
               refused uses in blocked_terms, the value there names one of them): the step will be refused or
               skipped in the game. Keep it in the plan only with the gap written in RUN/tool-gaps.md and a
               workaround the tools do carry.

The capitulation levers (conquest.tool_gap) read the same register through gap_register.lever_tool_gaps().
"""
import json

try:
    from . import gap_register, plan_keys
except ImportError:  # run as a script from scripts/hoi4facts
    from hoi4facts import gap_register, plan_keys  # noqa: E402


def values_at(node, path):
    """The values at a key path such as "intelligence.operatives[].mission" (list items as key[])."""
    cur = [node]
    for part in path.split("."):
        name, many = (part[:-2], True) if part.endswith("[]") else (part, False)
        nxt = []
        for c in cur:
            if isinstance(c, dict) and name in c:
                v = c[name]
                if many and isinstance(v, list):
                    nxt.extend(v)
                elif not many:
                    nxt.append(v)
        cur = nxt
    return cur


def check(lint, rows=None):
    """Adds gap.blocked findings to `lint` (a lint_plan.Lint)."""
    try:
        keyed = gap_register.blocked_keys(rows, rdr=plan_keys.readers(lint.schema))
    except (OSError, ValueError, KeyError):
        return
    used = plan_keys.used_paths(lint.plan, lint.schema)
    for path in sorted(keyed):
        if path not in used:
            continue
        for row in keyed[path]:
            values = [v for v in values_at(lint.plan, path) if v not in (None, False, [], {}, "")]
            if not values:
                continue
            terms = [t.lower() for t in row.get("blocked_terms") or ()]
            if terms:
                text = json.dumps(values).lower()
                if not any(t in text for t in terms):
                    continue
            lint.add("warning", "gap.blocked", path,
                     f"relies on {row['capability'].rstrip('.')}, which no tool carries yet "
                     f"({row.get('blocked') or 'no tool'}; gap register {row['id']}): the step is refused or "
                     f"skipped in the game. Write it in RUN/tool-gaps.md and plan a workaround the tools carry")
