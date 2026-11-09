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
            if role == "strategist" and prompt == "strategist.md":
                with open(os.path.join(self.run_dir, "plan.json"), "w") as f:
                    json.dump({"focus_path": []}, f)
            if role == "critic":
                self.critics += 1
                ok = self.critics > self.critic_fails
                critic = {"round": self.critics, "total": 18 if ok else 12, "pass": ok,
                          "scores": {"1": 2, "2": 2, "7": 2 if ok else 0},
                          "findings": [] if ok else [{"id": "F1", "severity": "blocker"}]}
                with open(os.path.join(self.run_dir, "review", "critic-%d.json" % self.critics), "w") as f:
                    json.dump(critic, f)
            return 0, ""
        raise AssertionError("unexpected command " + joined)


def args(run_dir, **kw):
    base = dict(run=run_dir, tag="ITA", date="1936.1.1", goal="Conquer Ethiopia", mode="fair", horizon=None,
                game=None, userdir=None, mod=None, provider=None, dry_run=False)
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
            self.assertEqual(roles(fake), [("strategist", "strategist.md"), ("strategist", "fix.md"),
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

    def test_review_gate_rules(self):
        ok, _ = strategize.review_passes({"total": 16, "scores": {"1": 1, "2": 1, "7": 1}, "findings": []})
        self.assertTrue(ok)
        self.assertFalse(strategize.review_passes({"total": 20, "scores": {"2": 0}, "findings": []})[0])
        self.assertFalse(strategize.review_passes(
            {"total": 20, "scores": {}, "findings": [{"id": "F", "severity": "blocker"}]})[0])
        self.assertFalse(strategize.review_passes({"total": 15, "scores": {}, "findings": []})[0])
        self.assertFalse(strategize.review_passes(None)[0])


if __name__ == "__main__":
    unittest.main()
