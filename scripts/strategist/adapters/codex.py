"""Codex CLI adapter (checked against `codex exec --help`, codex-cli 0.167.0-alpha, the copy inside the
ChatGPT app; the npm `codex` on PATH on the maintainer's Mac fails to start, so the app copy is a fallback).

Non-interactive run:
    codex exec -m <model> -c model_reasoning_effort="<level>" --sandbox <workspace-write> -C <cwd>
               --json -o <RUN>/agents/<role>.last.txt [--add-dir DIR ..] [extra_args ..] -   < prompt file
There is no effort flag; the effort goes in through the `model_reasoning_effort` config override.
Which levels a model accepts depends on the model, so `effort_levels` in the provider config says
which to use (default low|medium|high: xhigh and max clamp to high). Codex has no allowed-tools list:
`tools` in a role is not enforced, and the sandbox mode (`sandbox`, default workspace-write) is the
limit; that is recorded in the manifest notes. Provider config keys: binary, effort_levels, effort_map,
sandbox, extra_args, env, unset_env.
"""

import json
import os

from .base import Adapter, Invocation


class Codex(Adapter):
    name = "codex"
    default_binary = "codex"
    levels = ("low", "medium", "high")
    fallback_binaries = ("/Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex",)

    def build(self, binary, role, effort, prompt_file, run_dir, cwd, add_dirs=()):
        conf = self.role(role)
        level, native = self.effort(effort)
        last = os.path.join(run_dir, "agents", "%s.last.txt" % role)
        argv = [binary, "exec", "-m", conf["model"], "-c", 'model_reasoning_effort="%s"' % native,
                "--sandbox", self.pconf.get("sandbox") or "workspace-write", "-C", cwd, "--json", "-o", last]
        for d in [run_dir] + list(add_dirs):
            argv += ["--add-dir", d]
        argv += list(self.pconf.get("extra_args") or [])
        argv.append("-")
        notes = []
        if level != effort:
            notes.append("effort %s clamped to %s (effort_levels %s)" % (effort, level, list(self.supported_levels())))
        if conf.get("tools"):
            notes.append("tools %s not enforced: codex has no allowed-tools list; sandbox %s applies"
                         % (conf["tools"], self.pconf.get("sandbox") or "workspace-write"))
        if conf.get("mcp_config"):
            notes.append("mcp_config ignored: configure MCP servers in ~/.codex/config.toml for codex")
        env = dict(self.pconf.get("env") or {})
        for name in self.pconf.get("unset_env") or []:
            env[name] = None
        return Invocation(argv, stdin_path=prompt_file, env=env, cwd=cwd, notes=notes, output_last=last)

    def parse_output(self, stdout_text, invocation):
        """Best effort over the JSONL events: the first `model` field seen and the thread id."""
        out = {}
        for line in stdout_text.splitlines():
            try:
                event = json.loads(line)
            except ValueError:
                continue
            if not isinstance(event, dict):
                continue
            found = _find(event, "model")
            if found and "resolved_model" not in out:
                out["resolved_model"] = found
            tid = event.get("thread_id") or _find(event, "thread_id")
            if tid and "session_id" not in out:
                out["session_id"] = tid
        return out


def _find(node, key):
    if isinstance(node, dict):
        if isinstance(node.get(key), str):
            return node[key]
        for v in node.values():
            hit = _find(v, key)
            if hit:
                return hit
    elif isinstance(node, list):
        for v in node:
            hit = _find(v, key)
            if hit:
                return hit
    return None
