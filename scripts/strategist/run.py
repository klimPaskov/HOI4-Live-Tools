"""Run one agent role (strategist, critic or player) through the configured launcher.

    python3 scripts/strategist/run.py --role strategist --run RUN --prompt FILE
        [--revision quick|full] [--provider P] [--model M] [--effort E] [--config config.json]
        [--timeout SECONDS] [--dry-run]
    python3 scripts/strategist/run.py --role player --print-settings

Settings come from the `agents` section of config.json (hoi4_live.config.load_agents: DEFAULTS, the file,
then HOI4_LIVE_AGENT_* environment overrides; --provider/--model/--effort override those). The prompt
file is fed on stdin. The child gets HOI4_LIVE_RUN, HOI4_LIVE_AGENT_ROLE, HOI4_LIVE_SKILL_DIR and
HOI4_LIVE_TOOLS_DIR. Output goes to RUN/agents/<role>-<stamp>.out/.err, and RUN/agents-manifest.json
records provider, launcher, model alias, resolved model id (when the CLI reports it), requested and
applied effort, the command line and the exit code. --dry-run prints the command and runs nothing.
"""

import argparse
import datetime
import json
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, ROOT)
sys.path.insert(0, os.path.dirname(HERE))

from hoi4_live import config  # noqa: E402
from strategist import adapters  # noqa: E402

PLAYBOOK_DIR = os.path.join(ROOT, "skills", "hoi4-campaign-playbook")
STRATEGIST_DIR = os.path.join(ROOT, "skills", "hoi4-strategist")
# The skill each role works from: planning roles use the strategist skill (which points into the
# playbook references); the player uses the playbook.
SKILL_FOR_ROLE = {"strategist": STRATEGIST_DIR, "critic": STRATEGIST_DIR, "player": PLAYBOOK_DIR}
SKILL_DIR = PLAYBOOK_DIR
TOOLS_DIR = os.path.join(ROOT, "scripts")
MANIFEST = "agents-manifest.json"


def settings_for(args, env=None):
    env = dict(os.environ if env is None else env)
    if args.provider:
        env["HOI4_LIVE_AGENT_PROVIDER"] = args.provider
    if args.model:
        env["HOI4_LIVE_AGENT_%s_MODEL" % args.role.upper()] = args.model
    if args.effort:
        env["HOI4_LIVE_AGENT_%s_EFFORT" % args.role.upper()] = args.effort
    settings = config.load_agents(args.config, env)
    effort = settings["roles"][args.role]["effort"]
    if getattr(args, "revision", None):
        if args.role != "strategist":
            raise SystemExit("--revision applies to the strategist role only")
        if not args.effort:
            effort = settings["revision"]["%s_effort" % args.revision]
    return settings, effort


def plan(args, env=None, probe=None):
    """(adapter, settings, effort, binary, version, invocation) for the arguments; runs nothing."""
    settings, effort = settings_for(args, env)
    adapter = adapters.for_settings(settings)
    adapter.role(args.role)  # refuses an unset model before touching the binary
    binary, version = adapter.resolve_binary(probe)
    run_dir = os.path.abspath(args.run)
    invocation = adapter.build(binary, args.role, effort, os.path.abspath(args.prompt), run_dir, ROOT,
                               add_dirs=[STRATEGIST_DIR, PLAYBOOK_DIR, TOOLS_DIR])
    skill = SKILL_FOR_ROLE.get(args.role, PLAYBOOK_DIR)
    invocation.env.update({"HOI4_LIVE_RUN": run_dir, "HOI4_LIVE_AGENT_ROLE": args.role,
                           "HOI4_LIVE_SKILL_DIR": skill, "HOI4_LIVE_TOOLS_DIR": TOOLS_DIR})
    return adapter, settings, effort, binary, version, invocation


def record(run_dir, entry):
    """Add `entry` to RUN/agents-manifest.json (latest per role plus the full history), atomically."""
    path = os.path.join(run_dir, MANIFEST)
    data = {"version": 1, "roles": {}, "invocations": []}
    if os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
    data.setdefault("roles", {})[entry["role"]] = entry
    data.setdefault("invocations", []).append(entry)
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2)
        f.write("\n")
    os.replace(tmp, path)
    return path


def entry_for(args, adapter, settings, effort, binary, version, invocation):
    applied, native = adapter.effort(effort)
    return {
        "role": args.role,
        "revision": getattr(args, "revision", None),
        "provider": settings["provider"],
        "launcher": settings["launcher"],
        "model_alias": settings["roles"][args.role]["model"],
        "resolved_model": None,
        "effort_requested": effort,
        "effort_applied": applied,
        "effort_native": native,
        "binary": binary,
        "binary_version": version,
        "argv": invocation.argv,
        "prompt_file": os.path.abspath(args.prompt),
        "notes": invocation.notes,
    }


def _now():
    return datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.strip().splitlines()[0])
    p.add_argument("--role", required=True, choices=config.AGENT_ROLES)
    p.add_argument("--run", help="run folder (manifest and outputs go here)")
    p.add_argument("--prompt", help="prompt file, fed on stdin")
    p.add_argument("--revision", choices=("quick", "full"), help="strategist revision: use revision.<mode>_effort")
    p.add_argument("--provider")
    p.add_argument("--model")
    p.add_argument("--effort", choices=config.EFFORT_SCALE)
    p.add_argument("--config", help="config file (default $HOI4_LIVE_CONFIG or the repo config.json)")
    p.add_argument("--timeout", type=float, default=None, help="seconds before the child is stopped")
    p.add_argument("--dry-run", action="store_true")
    p.add_argument("--print-settings", action="store_true", help="print the resolved role settings as JSON")
    args = p.parse_args(argv)
    if args.print_settings:
        settings, effort = settings_for(args)
        out = dict(settings["roles"][args.role], effort=effort, provider=settings["provider"],
                   launcher=settings["launcher"])
        print(json.dumps(out, indent=2))
        return 0
    if not args.run or not args.prompt:
        p.error("--run and --prompt are required")
    try:
        adapter, settings, effort, binary, version, inv = plan(args)
    except adapters.CapabilityError as e:
        print("capability error: %s" % e, file=sys.stderr)
        return 3
    entry = entry_for(args, adapter, settings, effort, binary, version, inv)
    if args.dry_run:
        print(json.dumps({"argv": inv.argv, "stdin": inv.stdin_path, "cwd": inv.cwd, "env": inv.env,
                          "notes": inv.notes}, indent=2))
        return 0
    run_dir = os.path.abspath(args.run)
    out_dir = os.path.join(run_dir, "agents")
    os.makedirs(out_dir, exist_ok=True)
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    out_path = os.path.join(out_dir, "%s-%s.out" % (args.role, stamp))
    err_path = os.path.join(out_dir, "%s-%s.err" % (args.role, stamp))
    env = dict(os.environ)
    for k, v in inv.env.items():
        if v is None:
            env.pop(k, None)
        else:
            env[k] = str(v)
    entry.update(started=_now(), output_file=out_path, error_file=err_path)
    try:
        with open(inv.stdin_path, "rb") as fin, open(out_path, "wb") as fout, open(err_path, "wb") as ferr:
            code = subprocess.run(inv.argv, stdin=fin, stdout=fout, stderr=ferr, cwd=inv.cwd, env=env,
                                  timeout=args.timeout).returncode
    except subprocess.TimeoutExpired:
        code = 124
        entry["notes"].append("stopped after %s s" % args.timeout)
    with open(out_path, encoding="utf-8", errors="replace") as f:
        entry.update(adapter.parse_output(f.read(), inv))
    entry.update(finished=_now(), exit_code=code)
    if inv.output_last:
        entry["last_message_file"] = inv.output_last
    print(record(run_dir, entry))
    return code


if __name__ == "__main__":
    sys.exit(main())
