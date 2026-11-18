"""strategize.py: the orchestrator runs every gate in order with a fake runner (no model, no game)."""
import json
import os
import sys
import tempfile
import unittest
from types import SimpleNamespace

sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "scripts"))

from strategist import strategize  # noqa: E402


class Fake(strategize.Runner):
    """Plays the roles: validate fails `validate_fails` times; the critic fails `critic_fails` rounds."""

    def __init__(self, run_dir, validate_fails=1, critic_fails=1):
        super().__init__(False)
        self.run_dir, self.validate_fails, self.critic_fails = run_dir, validate_fails, critic_fails
        self.critics = 0

    def run(self, argv, env=None, timeout=None):
        self.commands.append(argv)
        joined = " ".join(argv)
        if "facts.py" in joined:
            os.makedirs(os.path.join(self.run_dir, "dossier"), exist_ok=True)
            for name in ("dossier.md", "manifest.json"):
                with open(os.path.join(self.run_dir, "dossier", name), "w") as f:
                    f.write("{}")
            return 0, "built"
        if "pack.py" in joined:
            return 0, "{}"
        if "validate_plan.py" in joined:
            if self.validate_fails:
                self.validate_fails -= 1
                return 1, "1 error(s)"
            return 0, "0 error(s)"
        if "check_calls.py" in joined:
            return 0, "checked 0 calls; 0 error(s)"
        if "lint_plan.py" in joined:
            return 0, json.dumps({"errors": 0, "warnings": 2})
        if "--role" in argv:
            role = argv[argv.index("--role") + 1]
            prompt = os.path.basename(argv[argv.index("--prompt") + 1])
            if role == "researcher" and prompt == "decisions.md" and not getattr(self, "no_table", False):
                with open(os.path.join(self.run_dir, "decisions.md"), "w") as f:
                    f.write("| id | area |\n|---|---|\n| D-FOCUS-01 | FOCUS |\n| D-WAR-01 | WAR |\n")
            if role == "strategist" and prompt == "strategist.md":
                with open(os.path.join(self.run_dir, "plan.json"), "w") as f:
                    json.dump({"focus_path": []}, f)
            if role == "strategist" and not getattr(self, "leave_gaps", False):
                gaps = os.path.join(self.run_dir, "tool-gaps.md")
                if os.path.exists(gaps):
                    with open(gaps) as f:
                        text = f.read()
                    with open(gaps, "w") as f:
                        f.write(text.replace("| ? |", "| not used: no such war |"))
            if role == "critic" and getattr(self, "silent_critic", False):
                return 0, ""
            if role == "critic":
                self.critics += 1
                ok = self.critics > self.critic_fails
                critic = {"round": self.critics, "total": 25 if ok else 12, "max": 32, "pass": ok,
                          "scores": {"1": 2, "2": 2, "7": 2 if ok else 0, "13": 2},
                          "findings": [] if ok else [{"id": "F1", "severity": "blocker"}]}
                n = 1 + len([x for x in os.listdir(os.path.join(self.run_dir, "review")) if x.startswith("critic-")])
                with open(os.path.join(self.run_dir, "review", "critic-%d.json" % n), "w") as f:
                    json.dump(critic, f)
            return 0, ""
        raise AssertionError("unexpected command " + joined)


def args(run_dir, **kw):
    base = dict(run=run_dir, tag="ITA", date="1936.1.1", goal="Conquer Ethiopia", mode="fair", horizon=None,
                game=None, userdir=None, mod=None, provider=None, dry_run=False, from_step="dossier", library=None,
                no_reuse=False)
    base.update(kw)
    return SimpleNamespace(**base)


def roles(runner):
    out = []
    for argv in runner.commands:
        if "--role" in argv:
            out.append((argv[argv.index("--role") + 1], os.path.basename(argv[argv.index("--prompt") + 1])))
    return out


class StrategizeTests(unittest.TestCase):
    def test_fix_round_then_review_round_then_freeze(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Fake(d, validate_fails=1, critic_fails=1)
            report = strategize.strategize(args(d), fake)
            self.assertTrue(report["ok"], report.get("failure"))
            self.assertEqual(roles(fake), [("researcher", "decisions.md"), ("strategist", "strategist.md"),
                                           ("strategist", "fix.md"),
                                           ("critic", "critic.md"), ("strategist", "answer.md"),
                                           ("critic", "critic.md")])
            plan = json.load(open(os.path.join(d, "plan.json")))
            self.assertTrue(plan["_meta"]["review"]["pass"])
            self.assertEqual(plan["_meta"]["lint_summary"], {"errors": 0, "warnings": 2})
            self.assertTrue(plan["_meta"]["dossier_hash"])
            spec = json.load(open(os.path.join(d, "run_spec.json")))
            self.assertEqual((spec["tag"], spec["goal"]), ("ITA", "Conquer Ethiopia"))
            self.assertFalse(os.path.exists(os.path.join(d, "strategist-blocked.md")))

    def test_review_that_never_passes_blocks_without_a_third_round(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Fake(d, validate_fails=0, critic_fails=5)
            report = strategize.strategize(args(d), fake)
            self.assertFalse(report["ok"])
            self.assertEqual(sum(1 for r in roles(fake) if r[0] == "critic"), strategize.REVIEW_ROUNDS)
            self.assertTrue(os.path.exists(os.path.join(d, "strategist-blocked.md")))
            self.assertNotIn("_meta", json.load(open(os.path.join(d, "plan.json"))))

    def test_gates_that_never_pass_stop_before_the_critic(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Fake(d, validate_fails=99, critic_fails=0)
            report = strategize.strategize(args(d), fake)
            self.assertFalse(report["ok"])
            self.assertNotIn("critic", [r[0] for r in roles(fake)])
            self.assertEqual(sum(1 for r in roles(fake) if r[1] == "fix.md"), strategize.FIX_ROUNDS)

    def test_critic_without_a_review_file_is_retried_once_then_blocks(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Fake(d, validate_fails=0, critic_fails=0)
            fake.silent_critic = True
            report = strategize.strategize(args(d), fake)
            self.assertFalse(report["ok"])
            self.assertIn("no RUN/review/critic", report["failure"])
            self.assertEqual(sum(1 for r in roles(fake) if r[0] == "critic"), 2)
            self.assertNotIn("answer.md", [r[1] for r in roles(fake)])

    def test_resume_from_the_critic_skips_strategist_and_gates(self):
        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "plan.json"), "w") as f:
                json.dump({"focus_path": []}, f)
            os.makedirs(os.path.join(d, "dossier"))
            fake = Fake(d, validate_fails=0, critic_fails=0)
            report = strategize.strategize(args(d, from_step="critic"), fake)
            self.assertTrue(report["ok"], report.get("failure"))
            self.assertEqual(roles(fake), [("critic", "critic.md")])
            self.assertFalse(any("validate_plan.py" in " ".join(c) for c in fake.commands))

    def test_review_gate_rules(self):
        self.assertEqual((strategize.PASS_TOTAL, strategize.MAX_TOTAL), (24, 34))
        self.assertEqual(strategize.CRITICAL_ROWS, ("1", "2", "7", "13", "17"))
        ok, detail = strategize.review_passes({"total": 24, "scores": {"1": 1, "2": 1, "7": 1, "13": 1, "17": 1},
                                               "findings": []})
        self.assertTrue(ok, detail)
        self.assertIn("24/34", detail)
        self.assertFalse(strategize.review_passes({"total": 23, "scores": {}, "findings": []})[0])
        self.assertFalse(strategize.review_passes({"total": 16, "scores": {"1": 1, "2": 1, "7": 1}, "findings": []})[0])
        self.assertFalse(strategize.review_passes({"total": 28, "scores": {"2": 0}, "findings": []})[0])
        self.assertFalse(strategize.review_passes({"total": 28, "scores": {"13": 0}, "findings": []})[0])
        self.assertTrue(strategize.review_passes({"total": 28, "scores": {"12": 0}, "findings": []})[0])
        self.assertFalse(strategize.review_passes(
            {"total": 28, "scores": {}, "findings": [{"id": "F", "severity": "blocker"}]})[0])
        self.assertTrue(strategize.review_passes(
            {"total": 28, "scores": {}, "findings": [{"id": "F", "severity": "blocker", "status": "fixed"}]})[0])
        self.assertFalse(strategize.review_passes(None)[0])
        # an open major finding sends the plan back while rounds remain; the last round ships it reported
        major = {"total": 28, "scores": {}, "findings": [{"id": "F1", "severity": "major"}]}
        ok, detail = strategize.review_passes(major)
        self.assertFalse(ok)
        self.assertIn("open majors ['F1']", detail)
        self.assertTrue(strategize.review_passes(major, allow_majors=True)[0])
        self.assertTrue(strategize.review_passes(
            {"total": 28, "scores": {}, "findings": [{"id": "F1", "severity": "major", "status": "fixed"}]})[0])

    def test_max_from_the_critic_file(self):
        rows11 = {str(i): 1 for i in range(1, 12)}
        ok, detail = strategize.review_passes({"total": 16, "max": 22, "scores": rows11, "findings": []})
        self.assertTrue(ok, detail)
        self.assertIn("16/22", detail)
        self.assertFalse(strategize.review_passes({"total": 15, "max": 22, "scores": rows11, "findings": []})[0])
        # A declared max below 2 points per scored row does not lower the pass mark.
        rows16 = {str(i): 1 for i in range(1, 17)}
        ok, detail = strategize.review_passes({"total": 16, "max": 16, "scores": rows16, "findings": []})
        self.assertFalse(ok)
        self.assertIn("16/32", detail)
        for bad in (None, 0, -4, "32", True):
            self.assertEqual(strategize.max_total({"max": bad}, {}), 34, bad)
        self.assertEqual([strategize.pass_mark(m) for m in (34, 22, 64)], [24, 16, 46])



class LintToolGaps(unittest.TestCase):
    def test_tool_gap_findings_reach_tool_gaps_md(self):
        with tempfile.TemporaryDirectory() as d:
            row = {"rule": "conquest.tool_gap", "severity": "warning",
                   "where": "goal.capitulation.levers[1] operation_collaboration_government",
                   "message": "the plan needs it but hoi4_operative_assign refuses operations"}
            with open(os.path.join(d, "lint.json"), "w", encoding="utf-8") as f:
                json.dump({"findings": [row, {"rule": "conquest.levers", "where": "x", "message": "y"}]}, f)
            self.assertEqual(len(strategize.lint_tool_gaps(d)), 1)
            self.assertEqual(strategize.lint_tool_gaps(d), [])  # once
            with open(os.path.join(d, "tool-gaps.md"), encoding="utf-8") as f:
                text = f.read()
            self.assertIn("operation_collaboration_government", text)
            self.assertNotIn("- x:", text)

class DecisionTableStepTests(unittest.TestCase):
    """1c: the sourced decision table before the strategist (any country, private sources optional)."""

    def test_decisions_only_writes_the_table_and_stops(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Fake(d, validate_fails=0, critic_fails=0)
            report = strategize.strategize(args(d, decisions_only=True), fake)
            self.assertTrue(report["ok"], report.get("failure"))
            self.assertEqual(roles(fake), [("researcher", "decisions.md")])
            step = [s for s in report["steps"] if s["step"] == "decisions"][0]
            self.assertEqual(step["rows"], 2)
            self.assertFalse(os.path.exists(os.path.join(d, "plan.json")))

    def test_an_existing_table_is_kept(self):
        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "decisions.md"), "w") as f:
                f.write("| D-GOAL-01 | GOAL |\n")
            fake = Fake(d, validate_fails=0, critic_fails=0)
            report = strategize.strategize(args(d), fake)
            self.assertTrue(report["ok"], report.get("failure"))
            self.assertEqual(roles(fake)[0], ("strategist", "strategist.md"))
            self.assertIn({"step": "decisions", "kept": True}, report["steps"])

    def test_no_table_blocks_before_the_strategist(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Fake(d, validate_fails=0, critic_fails=0)
            fake.no_table = True
            report = strategize.strategize(args(d), fake)
            self.assertFalse(report["ok"])
            self.assertIn("without RUN/decisions.md", report["failure"])
            self.assertEqual(roles(fake), [("researcher", "decisions.md")])

    def test_private_sources_are_copied_text_only_and_never_huge(self):
        with tempfile.TemporaryDirectory() as d, tempfile.TemporaryDirectory() as src:
            for name, size in (("NOTES.md", 100), ("land.md", 200), ("guide.pdf", 50),
                               ("big_guide.md", strategize.SOURCE_MAX_BYTES + 1)):
                with open(os.path.join(src, name), "w") as f:
                    f.write("x" * size)
            fake = Fake(d, validate_fails=0, critic_fails=0)
            report = strategize.strategize(args(d, decisions_only=True, source=[src]), fake)
            self.assertTrue(report["ok"], report.get("failure"))
            self.assertEqual(sorted(os.listdir(os.path.join(d, "sources"))), ["NOTES.md", "land.md"])
            self.assertEqual([s for s in report["steps"] if s["step"] == "decisions"][0]["sources"], 2)

    def test_resume_from_decisions_keeps_the_dossier(self):
        with tempfile.TemporaryDirectory() as d:
            os.makedirs(os.path.join(d, "dossier"))
            for name in ("dossier.md", "manifest.json"):
                with open(os.path.join(d, "dossier", name), "w") as f:
                    f.write("{}")
            fake = Fake(d, validate_fails=0, critic_fails=0)
            report = strategize.strategize(args(d, from_step="decisions"), fake)
            self.assertTrue(report["ok"], report.get("failure"))
            self.assertFalse(any("dossier" in c and "build" in c for c in fake.commands))
            self.assertEqual(roles(fake)[:2], [("researcher", "decisions.md"), ("strategist", "strategist.md")])


if __name__ == "__main__":
    unittest.main()


class GapTableAndTimelines(unittest.TestCase):
    def test_seeded_table_must_be_answered_before_the_critic(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Fake(d, validate_fails=0, critic_fails=0)
            fake.leave_gaps = True
            report = strategize.strategize(args(d), fake)
            self.assertFalse(report["ok"])
            self.assertNotIn("critic", [r[0] for r in roles(fake)])
            with open(os.path.join(d, "gates-1-1.txt")) as f:
                self.assertIn("error tool-gaps.unanswered", f.read())

    def test_seed_and_check(self):
        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "tool-gaps.md"), "w") as f:
                f.write("# My gaps\n\n- one gap found while planning\n")
            strategize.seed_tool_gaps(d)
            with open(os.path.join(d, "tool-gaps.md")) as f:
                text = f.read()
            self.assertIn("- one gap found while planning", text)  # the strategist's own notes stay
            self.assertIn(strategize.GAP_TABLE_HEAD, text)
            left = strategize.tool_gaps_unanswered(d)
            self.assertTrue(left)
            first = left[0]
            text = text.replace("| `%s` |" % first, "| `%s` |" % first, 1)
            lines = [ln.replace("| ? |", "| used: war.x; workaround: none |") if "`%s`" % first in ln else ln
                     for ln in text.splitlines()]
            with open(os.path.join(d, "tool-gaps.md"), "w") as f:
                f.write("\n".join(lines))
            self.assertNotIn(first, strategize.tool_gaps_unanswered(d))
            self.assertEqual(len(strategize.tool_gaps_unanswered(d)), len(left) - 1)
            # seeding again keeps the answers
            strategize.seed_tool_gaps(d)
            self.assertNotIn(first, strategize.tool_gaps_unanswered(d))
            # a deleted row counts as unanswered
            with open(os.path.join(d, "tool-gaps.md"), "w") as f:
                f.write("\n".join(ln for ln in lines if "`%s`" % first not in ln))
            self.assertIn(first, strategize.tool_gaps_unanswered(d))

    def test_no_table_no_check(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertEqual(strategize.tool_gaps_unanswered(d), [])

    def test_gates_write_the_timelines(self):
        with tempfile.TemporaryDirectory() as d:
            fake = Fake(d, validate_fails=0, critic_fails=0)
            report = strategize.strategize(args(d), fake)
            self.assertTrue(report["ok"], report.get("failure"))
            self.assertTrue(any("research-timeline" in c for c in fake.commands))
            with open(os.path.join(d, strategize.TIMELINES)) as f:
                self.assertIn("## Research", f.read())


class Reuse(unittest.TestCase):
    def library_entry(self, lib, goal="Conquer Ethiopia", passed=True, dlcs=("By Blood Alone",)):
        entry = os.path.join(lib, "ita-eth")
        os.makedirs(os.path.join(entry, "review"))
        spec = {"tag": "ITA", "start_date": "1936.1.1", "goal": goal, "mode": "fair", "horizon": "1937.1.1"}
        with open(os.path.join(entry, "run_spec.json"), "w") as f:
            json.dump(spec, f)
        with open(os.path.join(entry, "dossier-manifest.json"), "w") as f:
            json.dump({"dlcs": list(dlcs), "mods": []}, f)
        with open(os.path.join(entry, "plan.json"), "w") as f:
            json.dump({"focus_path": [], "_meta": {"models": {"strategist": {"model": "m"}},
                                                    "review": {"pass": passed, "detail": "total 28/32"}}}, f)
        for name in ("plan.md", "plan.brief.md"):
            with open(os.path.join(entry, name), "w") as f:
                f.write("x")
        with open(os.path.join(entry, "review", "critic-1.json"), "w") as f:
            json.dump({"total": 28}, f)
        strategize.seed_tool_gaps(entry)
        with open(os.path.join(entry, "tool-gaps.md")) as f:
            text = f.read()
        with open(os.path.join(entry, "tool-gaps.md"), "w") as f:
            f.write(text.replace("| ? |", "| not used: no such war |"))
        return entry

    def manifest(self, d, dlcs=("By Blood Alone",)):
        os.makedirs(os.path.join(d, "dossier"), exist_ok=True)
        for name in ("dossier.md", "manifest.json"):
            with open(os.path.join(d, "dossier", name), "w") as f:
                json.dump({"dlcs": list(dlcs), "mods": []}, f)

    def test_matching_reviewed_plan_ships_without_a_model_call(self):
        with tempfile.TemporaryDirectory() as lib, tempfile.TemporaryDirectory() as d:
            self.library_entry(lib)
            self.manifest(d)
            fake = Fake(d, validate_fails=0, critic_fails=0)
            report = strategize.strategize(args(d, goal="conquer  ethiopia", library=[lib]), fake)
            self.assertTrue(report["ok"], report.get("failure"))
            self.assertEqual(roles(fake), [])
            self.assertIn({"step": "reuse", "from": "ita-eth"}, report["steps"])
            with open(os.path.join(d, "plan.json")) as f:
                plan = json.load(f)
            self.assertEqual(plan["_meta"]["reused_from"], "ita-eth")
            self.assertEqual(plan["_meta"]["models"], {"strategist": {"model": "m"}})
            self.assertTrue(plan["_meta"]["review"]["pass"])
            self.assertTrue(os.path.exists(os.path.join(d, "review", "critic-1.json")))

    def test_reused_plan_that_fails_the_gates_is_fixed_and_reviewed(self):
        with tempfile.TemporaryDirectory() as lib, tempfile.TemporaryDirectory() as d:
            self.library_entry(lib)
            self.manifest(d)
            fake = Fake(d, validate_fails=1, critic_fails=0)
            report = strategize.strategize(args(d, library=[lib]), fake)
            self.assertTrue(report["ok"], report.get("failure"))
            self.assertEqual(roles(fake), [("strategist", "fix.md"), ("critic", "critic.md")])

    def test_no_match_plans_afresh(self):
        for kw in ({"goal": "Conquer Greece"}, {"passed": False}, {"dlcs": ("Other DLC",)}):
            with tempfile.TemporaryDirectory() as lib, tempfile.TemporaryDirectory() as d:
                self.library_entry(lib, **kw)
                self.manifest(d)
                fake = Fake(d, validate_fails=0, critic_fails=0)
                strategize.strategize(args(d, library=[lib]), fake)
                self.assertEqual(roles(fake)[:2], [("researcher", "decisions.md"), ("strategist", "strategist.md")],
                                 kw)
        with tempfile.TemporaryDirectory() as lib, tempfile.TemporaryDirectory() as d:
            self.library_entry(lib)
            self.manifest(d)
            fake = Fake(d, validate_fails=0, critic_fails=0)
            strategize.strategize(args(d, library=[lib], no_reuse=True), fake)
            self.assertEqual(roles(fake)[1], ("strategist", "strategist.md"))

    def test_publish_then_reuse(self):
        with tempfile.TemporaryDirectory() as lib, tempfile.TemporaryDirectory() as d, \
                tempfile.TemporaryDirectory() as d2:
            fake = Fake(d, validate_fails=0, critic_fails=0)
            self.assertTrue(strategize.strategize(args(d, no_reuse=True), fake)["ok"])
            with open(os.path.join(d, "dossier", "manifest.json"), "w") as f:
                json.dump({"dlcs": ["A"], "mods": []}, f)
            strategize.publish(d, os.path.join(lib, "first"))
            self.manifest(d2, dlcs=("A",))
            fake2 = Fake(d2, validate_fails=0, critic_fails=0)
            report = strategize.strategize(args(d2, library=[lib]), fake2)
            self.assertTrue(report["ok"], report.get("failure"))
            self.assertEqual(roles(fake2), [])

