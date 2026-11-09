"""Agent config (hoi4_live.config agents section), effort clamping and the launcher command lines.
No model is run: binaries are faked through the probe hook."""

import argparse
import json
import os
import sys
import tempfile
import unittest

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, ROOT)
sys.path.insert(0, os.path.join(ROOT, "scripts"))

from hoi4_live import config  # noqa: E402
from strategist import adapters, run  # noqa: E402
from strategist.adapters import CapabilityError, clamp_effort  # noqa: E402

OK_PROBE = lambda binary: (True, "fake 1.0")  # noqa: E731


def _write(path, data):
    with open(path, "w", encoding="utf-8") as f:
        json.dump(data, f)
    return path


class Defaults(unittest.TestCase):
    def test_user_decided_defaults(self):
        s = config.agents_settings(config.DEFAULTS["agents"], env={})
        self.assertEqual(s["provider"], "claude")
        self.assertEqual(s["launcher"], "claude-code")
        roles = s["roles"]
        self.assertEqual((roles["strategist"]["model"], roles["strategist"]["effort"]), ("sonnet", "xhigh"))
        self.assertEqual((roles["player"]["model"], roles["player"]["effort"]), ("haiku", "medium"))
        self.assertEqual((roles["critic"]["model"], roles["critic"]["effort"]), ("sonnet", "high"))
        self.assertEqual(s["revision"], {"quick_effort": "xhigh", "full_effort": "xhigh"})
        self.assertEqual(s["effort_scale"], ["low", "medium", "high", "xhigh", "max"])

    def test_repo_config_matches_defaults(self):
        with open(os.path.join(ROOT, "config.json"), encoding="utf-8") as f:
            self.assertEqual(json.load(f)["agents"], config.DEFAULTS["agents"])

    def test_openai_models_left_unset(self):
        s = config.agents_settings(config.DEFAULTS["agents"], env={"HOI4_LIVE_AGENT_PROVIDER": "openai"})
        self.assertEqual(s["launcher"], "codex")
        self.assertTrue(all(r["model"] is None for r in s["roles"].values()))
        # no revision block for openai: revisions use the strategist's effort
        self.assertEqual(s["revision"], {"quick_effort": "high", "full_effort": "high"})


class MergeAndEnv(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)

    def test_file_merges_over_defaults(self):
        path = _write(os.path.join(self.tmp.name, "c.json"), {"agents": {"providers": {
            "claude": {"critic": {"effort": "max"}, "player": {"model": "sonnet"}}}}})
        s = config.load_agents(path, env={})
        self.assertEqual(s["roles"]["critic"], {"model": "sonnet", "effort": "max",
                                                "tools": ["Read", "Glob", "Grep", "Bash"]})
        self.assertEqual(s["roles"]["player"]["model"], "sonnet")
        self.assertEqual(s["roles"]["player"]["effort"], "medium")  # untouched default
        self.assertEqual(s["roles"]["strategist"]["model"], "sonnet")
        self.assertIn("openai", config.DEFAULTS["agents"]["providers"])  # DEFAULTS never mutated
        self.assertEqual(config.DEFAULTS["agents"]["providers"]["claude"]["critic"]["effort"], "high")

    def test_missing_file_gives_defaults_and_env_config_path(self):
        self.assertEqual(config.load_agents(os.path.join(self.tmp.name, "none.json"), env={})["roles"]["player"]["model"],
                         "haiku")
        path = _write(os.path.join(self.tmp.name, "c.json"), {"agents": {"provider": "openai"}})
        self.assertEqual(config.load_agents(env={"HOI4_LIVE_CONFIG": path})["provider"], "openai")

    def test_env_overrides(self):
        env = {"HOI4_LIVE_AGENT_STRATEGIST_MODEL": "opus", "HOI4_LIVE_AGENT_PLAYER_EFFORT": "HIGH",
               "HOI4_LIVE_AGENT_REVISION_QUICK_EFFORT": "medium", "HOI4_LIVE_AGENT_BINARY": "/opt/claude"}
        s = config.agents_settings(config.DEFAULTS["agents"], env=env)
        self.assertEqual(s["roles"]["strategist"]["model"], "opus")
        self.assertEqual(s["roles"]["player"]["effort"], "high")
        self.assertEqual(s["revision"], {"quick_effort": "medium", "full_effort": "xhigh"})
        self.assertEqual(s["binary"], "/opt/claude")

    def test_bad_values_are_refused(self):
        with self.assertRaises(ValueError):
            config.agents_settings(config.DEFAULTS["agents"], env={"HOI4_LIVE_AGENT_CRITIC_EFFORT": "ultra"})
        with self.assertRaises(ValueError):
            config.agents_settings(config.DEFAULTS["agents"], env={"HOI4_LIVE_AGENT_PROVIDER": "nobody"})

    def test_config_object_exposes_agents_lazily(self):
        cfg = config.load(os.path.join(self.tmp.name, "none.json"))
        self.assertEqual(cfg.agents["roles"]["player"]["model"], "haiku")


class Effort(unittest.TestCase):
    def test_normalize(self):
        self.assertEqual(config.normalize_effort(" XHigh "), "xhigh")
        with self.assertRaises(ValueError):
            config.normalize_effort("minimal")

    def test_clamp(self):
        three = ("low", "medium", "high")
        self.assertEqual(clamp_effort("xhigh", three), "high")
        self.assertEqual(clamp_effort("max", three), "high")
        self.assertEqual(clamp_effort("medium", three), "medium")
        self.assertEqual(clamp_effort("low", ("high", "max")), "high")  # nothing below: lowest supported
        self.assertEqual(clamp_effort("xhigh", ("low", "max")), "low")
        self.assertEqual(clamp_effort("max", config.EFFORT_SCALE), "max")
        with self.assertRaises(ValueError):
            clamp_effort("ultra", three)
        with self.assertRaises(CapabilityError):
            clamp_effort("low", ())

    def test_adapter_maps_and_clamps(self):
        claude = adapters.for_settings(config.agents_settings(config.DEFAULTS["agents"], env={}))
        self.assertEqual(claude.effort("xhigh"), ("xhigh", "xhigh"))
        self.assertEqual(claude.effort("max"), ("max", "max"))
        agents = json.loads(json.dumps(config.DEFAULTS["agents"]))
        agents["providers"]["openai"]["effort_levels"] = ["low", "medium", "high", "xhigh"]
        agents["providers"]["openai"]["effort_map"] = {"low": "minimal"}
        codex = adapters.for_settings(config.agents_settings(agents, env={"HOI4_LIVE_AGENT_PROVIDER": "openai"}))
        self.assertEqual(codex.effort("max"), ("xhigh", "xhigh"))
        self.assertEqual(codex.effort("low"), ("low", "minimal"))
        default_codex = adapters.for_settings(config.agents_settings(config.DEFAULTS["agents"],
                                                                     env={"HOI4_LIVE_AGENT_PROVIDER": "openai"}))
        self.assertEqual(default_codex.effort("xhigh"), ("high", "high"))


class CommandLines(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.prompt = os.path.join(self.tmp.name, "prompt.txt")
        with open(self.prompt, "w") as f:
            f.write("plan Italy 1936")
        self.run_dir = os.path.join(self.tmp.name, "RUN")

    def args(self, role, **kw):
        base = dict(role=role, run=self.run_dir, prompt=self.prompt, provider=None, model=None, effort=None,
                    config=os.path.join(self.tmp.name, "none.json"), revision=None)
        base.update(kw)
        return argparse.Namespace(**base)

    def test_claude_strategist(self):
        env = {"HOI4_LIVE_AGENT_BINARY": "/bin/claude"}
        adapter, settings, effort, binary, version, inv = run.plan(self.args("strategist"), env, OK_PROBE)
        self.assertEqual(inv.argv[:9], ["/bin/claude", "-p", "--model", "sonnet", "--effort", "xhigh",
                                        "--output-format", "json", "--allowed-tools"])
        self.assertEqual(inv.argv[9], "Read,Write,Edit,Glob,Grep,Bash")
        self.assertEqual(inv.argv[10:12], ["--add-dir", self.run_dir])
        self.assertIn(run.SKILL_DIR, inv.argv)
        self.assertEqual(inv.stdin_path, self.prompt)
        self.assertEqual(inv.cwd, run.ROOT)
        self.assertEqual(inv.env["HOI4_LIVE_AGENT_ROLE"], "strategist")
        self.assertEqual(inv.env["HOI4_LIVE_RUN"], self.run_dir)

    def test_claude_player_and_provider_options(self):
        path = _write(os.path.join(self.tmp.name, "c.json"), {"agents": {"providers": {"claude": {
            "binary": "/bin/claude", "permission_mode": "dontAsk", "extra_args": ["--bare"],
            "env": {"CLAUDE_CONFIG_DIR": "/cfg"}, "unset_env": ["ANTHROPIC_API_KEY"],
            "player": {"mcp_config": "/mcp.json"}}}}})
        inv = run.plan(self.args("player", config=path), {}, OK_PROBE)[-1]
        a = inv.argv
        self.assertEqual(a[a.index("--model") + 1], "haiku")
        self.assertEqual(a[a.index("--effort") + 1], "medium")
        self.assertEqual(a[a.index("--allowed-tools") + 1], "Read,Bash,mcp__hoi4-live")
        self.assertEqual(a[a.index("--permission-mode") + 1], "dontAsk")
        self.assertEqual(a[a.index("--mcp-config") + 1], "/mcp.json")
        self.assertEqual(a[-1], "--bare")
        self.assertEqual(inv.env["CLAUDE_CONFIG_DIR"], "/cfg")
        self.assertIsNone(inv.env["ANTHROPIC_API_KEY"])

    def test_revision_and_cli_overrides(self):
        env = {"HOI4_LIVE_AGENT_BINARY": "/bin/claude", "HOI4_LIVE_AGENT_REVISION_QUICK_EFFORT": "high"}
        a = run.plan(self.args("strategist", revision="quick"), env, OK_PROBE)[-1].argv
        self.assertEqual(a[a.index("--effort") + 1], "high")
        a = run.plan(self.args("critic", model="opus", effort="low"), env, OK_PROBE)[-1].argv
        self.assertEqual((a[a.index("--model") + 1], a[a.index("--effort") + 1]), ("opus", "low"))

    def test_codex_command(self):
        env = {"HOI4_LIVE_AGENT_PROVIDER": "openai", "HOI4_LIVE_AGENT_BINARY": "/bin/codex",
               "HOI4_LIVE_AGENT_STRATEGIST_MODEL": "gpt-5"}
        inv = run.plan(self.args("strategist", effort="xhigh"), env, OK_PROBE)[-1]
        self.assertEqual(inv.argv[:10], ["/bin/codex", "exec", "-m", "gpt-5", "-c", 'model_reasoning_effort="high"',
                                         "--sandbox", "workspace-write", "-C", run.ROOT])
        self.assertEqual(inv.argv[10:13], ["--json", "-o", os.path.join(self.run_dir, "agents", "strategist.last.txt")])
        self.assertEqual(inv.argv[-1], "-")
        self.assertTrue(any("clamped" in n for n in inv.notes))

    def test_codex_without_model_or_binary_is_a_capability_error(self):
        with self.assertRaises(CapabilityError) as e:
            run.plan(self.args("player"), {"HOI4_LIVE_AGENT_PROVIDER": "openai"}, OK_PROBE)
        self.assertIn("model is not set", str(e.exception))
        env = {"HOI4_LIVE_AGENT_PROVIDER": "openai", "HOI4_LIVE_AGENT_PLAYER_MODEL": "m",
               "HOI4_LIVE_AGENT_BINARY": "/bin/codex"}
        with self.assertRaises(CapabilityError) as e:
            run.plan(self.args("player"), env, lambda b: (False, "ENOENT"))
        self.assertIn("not runnable", str(e.exception))


class Manifest(unittest.TestCase):
    def test_record_and_parse(self):
        with tempfile.TemporaryDirectory() as tmp:
            prompt = _write(os.path.join(tmp, "p.txt"), {})
            args = argparse.Namespace(role="strategist", run=tmp, prompt=prompt, provider=None, model=None,
                                      effort=None, config=os.path.join(tmp, "none.json"), revision=None)
            adapter, settings, effort, binary, version, inv = run.plan(args, {"HOI4_LIVE_AGENT_BINARY": "/bin/claude"},
                                                                       OK_PROBE)
            entry = run.entry_for(args, adapter, settings, effort, binary, version, inv)
            stdout = json.dumps({"type": "result", "session_id": "s1", "total_cost_usd": 0.5, "modelUsage": {
                "claude-haiku-x": {"outputTokens": 10}, "claude-sonnet-5-5": {"outputTokens": 900}}})
            entry.update(adapter.parse_output(stdout, inv), exit_code=0)
            path = run.record(tmp, entry)
            run.record(tmp, dict(entry, role="critic"))
            with open(path) as f:
                data = json.load(f)
        self.assertEqual(sorted(data["roles"]), ["critic", "strategist"])
        self.assertEqual(len(data["invocations"]), 2)
        s = data["roles"]["strategist"]
        self.assertEqual((s["provider"], s["launcher"], s["model_alias"]), ("claude", "claude-code", "sonnet"))
        self.assertEqual(s["resolved_model"], "claude-sonnet-5-5")
        self.assertEqual((s["effort_requested"], s["effort_applied"], s["effort_native"]), ("xhigh",) * 3)
        self.assertEqual(s["binary_version"], "fake 1.0")

    def test_codex_parse_output(self):
        settings = config.agents_settings(config.DEFAULTS["agents"], env={"HOI4_LIVE_AGENT_PROVIDER": "openai"})
        codex = adapters.for_settings(settings)
        lines = "\n".join([json.dumps({"type": "thread.started", "thread_id": "t1"}), "not json",
                           json.dumps({"type": "turn.started", "info": {"model": "gpt-5.1"}})])
        self.assertEqual(codex.parse_output(lines, None), {"session_id": "t1", "resolved_model": "gpt-5.1"})


if __name__ == "__main__":
    unittest.main()
