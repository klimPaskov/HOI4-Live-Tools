"""The capability gap register (scripts/hoi4facts/gap_register.py) and the lint rule that reads it.

(a) the repository passes: the register page is current, every row is well formed, the playbook names only
    tools that exist, every UNVALIDATED tool is registered and every open row has an owner;
(b) the status follows the fields in order: missing from playbook, not executable, not plannable, unvalidated, ok;
(c) each kind of drift is reported: an unknown tool or plan key, an open row without an owner, an UNVALIDATED
    tool in no open row, a live run recorded while the descriptor still says UNVALIDATED, a playbook line naming a
    refused use without saying so, a stale page;
(d) the not-executable rows reach the strategist (blocked list) and the lint (gap.blocked, conquest.tool_gap);
(e) a key whose executor step is not built must be an open row (plan_keys.py check).
"""

import os
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4facts import gap_register, lint_gaps, plan_keys  # noqa: E402

CAT = {"hoi4_good": {"description": "does it"},
       "hoi4_new": {"description": "UNVALIDATED (no live test yet): does it"},
       "hoi4_ops": {"description": "operations"}}
RDR = {"war": "executor", "intelligence.operatives[].mission": "executor", "goal.capitulation.levers": "lint",
       "xp_budget": "planned", "air.note": "text"}


def row(**kw):
    base = {"id": "army.x", "domain": "orders", "capability": "attack", "playbook": ["tactics.md#Attack"],
            "plan": ["war"], "tools": ["hoi4_good"], "blocked": None, "verified": "live day log 14 Nov",
            "payoff": "high", "why": "wins wars", "closing": "none", "owner": ""}
    base.update(kw)
    return base


def problems(rows, files=(), cat=CAT):
    return gap_register.check(rows, cat, RDR, page=None, files=list(files))


class Status(unittest.TestCase):
    def test_order_of_the_statuses(self):
        st = lambda **kw: gap_register.status(row(**kw), RDR)  # noqa: E731
        self.assertEqual(st(), "ok")
        self.assertEqual(st(playbook=[], tools=[]), "missing from playbook")
        self.assertEqual(st(tools=[]), "not executable")
        self.assertEqual(st(blocked="refuses every operation"), "not executable")
        self.assertEqual(st(plan=[]), "not plannable")
        self.assertEqual(st(plan=["goal.capitulation.levers"]), "not plannable")  # lint only
        self.assertEqual(st(plan=["xp_budget"]), "not plannable")  # executor step not built
        self.assertEqual(st(plan=["air.note"]), "not plannable")  # text
        self.assertEqual(st(plan=[], plan_na="a read the player makes every tick"), "ok")
        self.assertEqual(st(verified=None), "unvalidated")


class Check(unittest.TestCase):
    def test_the_repository_passes(self):
        self.assertEqual(gap_register.check(), [])
        self.assertEqual(gap_register.main(["check"]), 0)

    def test_a_well_formed_register_passes(self):
        self.assertEqual(problems([row(), row(id="b", tools=["hoi4_new"], verified=None, closing="live",
                                               owner="live queue item 3")]), [])

    def test_unknown_tool_and_key_and_values(self):
        out = "\n".join(problems([row(tools=["hoi4_nope"], plan=["nope"], payoff="huge", domain="x")]))
        for want in ("hoi4_nope is not in the catalogue", "plan key nope", "payoff 'huge'", "domain 'x'"):
            self.assertIn(want, out)

    def test_an_open_row_needs_an_owner(self):
        out = problems([row(verified=None, closing="live")])
        self.assertTrue(any("no owner note" in p for p in out), out)

    def test_an_unvalidated_tool_must_be_in_an_open_row(self):
        out = problems([row()])
        self.assertTrue(any(p.startswith("hoi4_new: its description says UNVALIDATED") for p in out), out)

    def test_a_live_run_with_a_stale_descriptor(self):
        out = problems([row(tools=["hoi4_new"])])
        self.assertTrue(any("still says UNVALIDATED" in p for p in out), out)

    def test_playbook_lines(self):
        refused = row(id="esp.collab", domain="espionage", tools=["hoi4_ops"], blocked="refuses operations",
                      blocked_terms=["collaboration"], closing="live", owner="ops tool")
        with tempfile.TemporaryDirectory() as d:
            bad = os.path.join(d, "bad.md")
            with open(bad, "w", encoding="utf-8") as f:
                f.write("# Espionage\n\nRun collaboration with `hoi4_ops`.\nCall `hoi4_missing` first.\n")
            good = os.path.join(d, "good.md")
            with open(good, "w", encoding="utf-8") as f:
                f.write("# Espionage\n\nCollaboration with `hoi4_ops` is a gap: the tool refuses it.\n"
                        "## Gaps\n\n- collaboration via `hoi4_ops`\n- every `hoi4_x {json}` call\n")
            out = problems([row(), refused, row(id="n", tools=["hoi4_new"], verified=None, closing="live",
                                                owner="q")], files=[bad, good])
        self.assertEqual(len(out), 2, out)
        self.assertTrue(any("bad.md:3: names hoi4_ops for a use it refuses" in p for p in out), out)
        self.assertTrue(any("bad.md:4: names hoi4_missing" in p for p in out), out)

    def test_a_stale_page_fails(self):
        with tempfile.TemporaryDirectory() as d:
            page = os.path.join(d, "GAP.md")
            with open(page, "w", encoding="utf-8") as f:
                f.write("old\n")
            out = gap_register.check([row()], {"hoi4_good": CAT["hoi4_good"]}, RDR, page=page, files=[])
        self.assertEqual(len(out), 1)
        self.assertIn("is stale", out[0])


class Consumers(unittest.TestCase):
    rows = [row(id="esp.collab", domain="espionage", plan=["intelligence.operatives[].mission"],
                tools=["hoi4_ops"], blocked="refuses operations", blocked_terms=["collaboration"],
                lever_kind="operation", closing="live", owner="ops tool")]

    def test_blocked_list_and_lever_gaps(self):
        self.assertEqual([r["id"] for r in gap_register.blocked(self.rows, RDR)], ["esp.collab"])
        self.assertIn("esp.collab", gap_register.blocked_markdown(self.rows, RDR))
        self.assertIn("refuses operations", gap_register.lever_tool_gaps(self.rows)["operation"])

    def test_lint_flags_a_plan_that_relies_on_a_blocked_lever(self):
        class Lint:
            schema = plan_keys.load_schema()

            def __init__(self, plan):
                self.plan, self.findings = plan, []

            def add(self, severity, rule, where, message):
                self.findings.append((severity, rule, where, message))

        relies = Lint({"intelligence": {"operatives": [{"mission": "operation_collaboration_government"}]}})
        lint_gaps.check(relies, self.rows)
        self.assertEqual([f[:3] for f in relies.findings],
                         [("warning", "gap.blocked", "intelligence.operatives[].mission")])
        fine = Lint({"intelligence": {"operatives": [{"mission": "build_intel_network"}]}})
        lint_gaps.check(fine, self.rows)
        self.assertEqual(fine.findings, [])

    def test_values_at(self):
        plan = {"a": [{"b": {"c": 1}}, {"b": {"c": 2}}, {"x": 3}]}
        self.assertEqual(lint_gaps.values_at(plan, "a[].b.c"), [1, 2])


class Strategist(unittest.TestCase):
    def test_the_run_gets_the_blocked_levers(self):
        from strategist import strategize
        with tempfile.TemporaryDirectory() as d:
            path = strategize.write_blocked_levers(d)
            with open(path, encoding="utf-8") as f:
                text = f.read()
        self.assertIn("espionage.coup", text)
        self.assertIn("RUN/tool-gaps.md", text)

    def test_gap_findings_reach_tool_gaps(self):
        import json
        from strategist import strategize
        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "lint.json"), "w", encoding="utf-8") as f:
                json.dump({"findings": [{"rule": "gap.blocked", "where": "occupation.per_state",
                                         "message": "relies on per-state laws"}]}, f)
            self.assertEqual(len(strategize.lint_tool_gaps(d)), 1)
            with open(os.path.join(d, "tool-gaps.md"), encoding="utf-8") as f:
                self.assertIn("occupation.per_state", f.read())


class PlannedKeys(unittest.TestCase):
    def test_a_planned_key_must_be_an_open_row(self):
        rdr = dict(RDR)
        out = gap_register.planned_problems(["xp_budget"], rows=[row(plan=["xp_budget"], verified=None,
                                                                    closing="offline", owner="W1-14")], rdr=rdr)
        self.assertEqual(out, [])
        out = gap_register.planned_problems(["xp_budget"], rows=[], rdr=rdr)
        self.assertEqual(out[0][0], "xp_budget")

    def test_the_schema_reconcile_runs_it(self):
        planned = [p for p, r in plan_keys.readers().items() if r == "planned"]
        self.assertTrue(planned)
        self.assertEqual(gap_register.planned_problems(planned), [])


if __name__ == "__main__":
    unittest.main()
