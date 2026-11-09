"""Claude Code CLI adapter (checked against `claude --help`, Claude Code 2.1.319).

Non-interactive run:
    claude -p --model <alias> --effort <low|medium|high|xhigh|max> --output-format json
           --allowed-tools <A,B,..> [--add-dir DIR ..] [--permission-mode M] [--mcp-config FILE]
           [extra_args ..]   < prompt file
`-p` reads the prompt from stdin. `--effort` takes the same five levels as the normalised scale, so the
map is the identity; a provider config can narrow `effort_levels` (then the level is clamped) or rename
levels with `effort_map`. `--output-format json` returns one result object whose `modelUsage` keys are
the resolved model ids. Provider config keys: binary, effort_levels, effort_map, permission_mode,
extra_args (e.g. ["--dangerously-skip-permissions"] in a sandbox), env (e.g. {"CLAUDE_CONFIG_DIR": ...}),
unset_env (e.g. ["ANTHROPIC_API_KEY"] to use the subscription login). Role keys: model, effort, tools,
mcp_config.
"""

import json
import os

from .base import Adapter, Invocation


class ClaudeCode(Adapter):
    name = "claude-code"
    default_binary = "claude"
    fallback_binaries = (os.path.expanduser("~/.local/bin/claude"),)

    def build(self, binary, role, effort, prompt_file, run_dir, cwd, add_dirs=()):
        conf = self.role(role)
        level, native = self.effort(effort)
        argv = [binary, "-p", "--model", conf["model"], "--effort", native, "--output-format", "json"]
        tools = conf.get("tools")
        if tools:
            argv += ["--allowed-tools", ",".join(tools)]
        for d in [run_dir] + list(add_dirs):
            argv += ["--add-dir", d]
        if self.pconf.get("permission_mode"):
            argv += ["--permission-mode", self.pconf["permission_mode"]]
        if conf.get("mcp_config"):
            argv += ["--mcp-config", conf["mcp_config"]]
        argv += list(self.pconf.get("extra_args") or [])
        notes = []
        if level != effort:
            notes.append("effort %s clamped to %s (effort_levels %s)" % (effort, level, list(self.supported_levels())))
        env = dict(self.pconf.get("env") or {})
        for name in self.pconf.get("unset_env") or []:
            env[name] = None
        return Invocation(argv, stdin_path=prompt_file, env=env, cwd=cwd, notes=notes)

    def parse_output(self, stdout_text, invocation):
        """resolved_model (the model with the most output tokens), models_seen, session_id, cost."""
        try:
            result = json.loads(stdout_text.strip().splitlines()[-1]) if stdout_text.strip() else {}
        except ValueError:
            return {}
        if not isinstance(result, dict):
            return {}
        usage = result.get("modelUsage") or {}
        out = {"session_id": result.get("session_id"), "cost_usd": result.get("total_cost_usd"),
               "is_error": result.get("is_error")}
        if usage:
            ranked = sorted(usage.items(), key=lambda kv: -(kv[1] or {}).get("outputTokens", 0))
            out["resolved_model"] = ranked[0][0]
            out["models_seen"] = [k for k, _ in ranked]
        return out
