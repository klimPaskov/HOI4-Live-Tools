#!/usr/bin/env python3
"""strategize.py: produce a reviewed campaign plan for one run, end to end.

Usage:
    strategize.py --run RUN [--tag ITA --date 1936.1.1 --goal "..." --mode fair --horizon 1937.1.1]
                  [--game DIR] [--userdir DIR] [--mod PATH ...] [--provider NAME] [--from gates|critic]
                  [--dry-run]

RUN/run_spec.json is written from the options when it does not exist yet (an existing spec wins).
Steps, each gated; the script owns the order so no step can be skipped or self-reviewed:

  1. dossier    facts.py dossier build -> RUN/dossier/ (deterministic, cached by its manifest), then
                pack.py -> RUN/pack/ (the facts every plan needs, the schema digest, the lint rules)
  2. strategist run.py --role strategist, prompt skills/hoi4-strategist/prompts/strategist.md
                (procedure steps 1-7; it stops before the critic)
  3. gates      validate_plan.py --strict, lint_plan.py --json -> RUN/lint.json, and check_calls.py
                (every player tool call valid against the tool catalogue); on errors the
                strategist gets prompts/fix.md with RUN/gates-<n>.txt, at most FIX_ROUNDS times
  4. critic     run.py --role critic (fresh context) -> RUN/review/critic-<n>.json
  5. answers    on a failed review the strategist gets prompts/answer.md, then gates and critic
                again; at most REVIEW_ROUNDS critic rounds
  6. freeze     plan.json _meta (version, dossier hash, models, created, lint summary, review)
                and RUN/strategize-report.json; on failure RUN/strategist-blocked.md

Reuse: when a reviewed plan in the library (`--library DIR`, HOI4_LIVE_PLAN_LIBRARY, then
skills/hoi4-campaign-playbook/plans/reviewed/) was made for the same request (tag, start date, mode, goal,
clarifications, horizon, difficulty, AI, and the same resolved DLCs and mods), it is copied into RUN and
the run starts at the gates: when they pass unchanged the plan ships with its stored review (no model call);
when they fail, the fix and review steps run as usual. `--no-reuse` always plans afresh; `--publish DIR`
copies a passed run into a library folder. A library entry is a folder with run_spec.json,
dossier-manifest.json, plan.json, plan.md, plan.brief.md, tool-gaps.md and review/.

Exit status 0 only when the plan passed every gate and the review. Models and effort come from
config.json `agents` (run.py); nothing here is provider-specific. Stdlib only, Python 3.9+.
"""
import argparse
import datetime
import glob
import hashlib
import json
import os
import re
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
SKILL = os.path.join(ROOT, "skills", "hoi4-strategist")
PROMPTS = os.path.join(SKILL, "prompts")
FIX_ROUNDS = 2
REVIEW_ROUNDS = 2
# Critic rubric (skills/hoi4-strategist/references/rubric.md): 16 rows scored 0-2, pass at 22 of 32 with no 0
# in a critical row. A critic file that states another `max` is scored against the same share of it.
PASS_TOTAL = 22
MAX_TOTAL = 32
CRITICAL_ROWS = ("1", "2", "7", "13")


def log(msg):
    print("[strategize %s] %s" % (time.strftime("%H:%M:%S"), msg), file=sys.stderr, flush=True)


def write_json(path, obj):
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(obj, f, indent=1, ensure_ascii=False)
        f.write("\n")
    os.replace(tmp, path)


def read_json(path, default=None):
    try:
        with open(path, encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return default


def game_args(args):
    out = []
    if args.game:
        out += ["--game", args.game]
    if args.userdir:
        out += ["--userdir", args.userdir]
    for m in args.mod or []:
        out += ["--mod", m]
    return out


def lint_args(args):
    """lint_plan.py takes --game and --mod (no --userdir)."""
    out = ["--game", args.game] if args.game else []
    for m in args.mod or []:
        out += ["--mod", m]
    return out


class Runner:
    """Runs subprocesses; `dry` records the commands instead (tests and --dry-run)."""

    def __init__(self, dry=False):
        self.dry = dry
        self.commands = []

    def run(self, argv, env=None, timeout=None):
        self.commands.append(argv)
        if self.dry:
            return 0, ""
        e = dict(os.environ)
        e.update(env or {})
        p = subprocess.run(argv, cwd=ROOT, env=e, capture_output=True, text=True, timeout=timeout,
                           encoding="utf-8", errors="replace")
        return p.returncode, (p.stdout or "") + (p.stderr or "")


def spec_from(args, run_dir):
    path = os.path.join(run_dir, "run_spec.json")
    spec = read_json(path)
    if spec is None:
        missing = [k for k in ("tag", "date", "goal") if not getattr(args, k)]
        if missing:
            raise SystemExit("no RUN/run_spec.json and missing options: %s" % ", ".join(missing))
        template = read_json(os.path.join(SKILL, "templates", "run_spec.json"), {})
        spec = {k: v for k, v in template.items() if not k.startswith("_")}
        spec.update({"tag": args.tag, "start_date": args.date, "goal": args.goal,
                     "mode": args.mode, "horizon": args.horizon or spec.get("horizon")})
        write_json(path, spec)
    return spec


def agent(runner, role, run_dir, prompt, args, revision=None):
    argv = [sys.executable, os.path.join(HERE, "run.py"), "--role", role, "--run", run_dir,
            "--prompt", os.path.join(PROMPTS, prompt)]
    if args.provider:
        argv += ["--provider", args.provider]
    if revision:
        argv += ["--revision", revision]
    t0 = time.time()
    code, out = runner.run(argv)
    log("%s (%s) exit %s in %.0f s" % (role, prompt, code, time.time() - t0))
    return code, out, time.time() - t0


def gates(runner, run_dir, args):
    """(ok, text, lint) for validate_plan --strict and lint_plan on RUN/plan.json."""
    plan = os.path.join(run_dir, "plan.json")
    if not runner.dry and not os.path.exists(plan):
        return False, "RUN/plan.json missing", {}
    vcode, vout = runner.run([sys.executable, os.path.join(ROOT, "scripts", "validate_plan.py"),
                              plan, "--strict"] + game_args(args))
    lcode, lout = runner.run([sys.executable, os.path.join(ROOT, "scripts", "hoi4facts", "lint_plan.py"),
                              plan, "--json"] + lint_args(args))
    ccode, cout = runner.run([sys.executable, os.path.join(ROOT, "scripts", "hoi4facts", "check_calls.py"), run_dir])
    lint = {}
    unanswered = []
    if not runner.dry:
        try:
            lint = json.loads(lout[lout.index("{"):]) if "{" in lout else {}
        except ValueError:
            lint = {"unparsed": lout[-2000:]}
        write_json(os.path.join(run_dir, "lint.json"), lint)
        # the lint's gap findings reach tool-gaps.md before the critic reads it
        lint_tool_gaps(run_dir)
        unanswered = tool_gaps_unanswered(run_dir)
    write_timelines(runner, run_dir, args)
    text = ("## validate_plan --strict (exit %s)\n%s\n## lint_plan (exit %s)\n%s\n"
            "## check_calls: player tool calls against the tool catalogue (exit %s)\n%s\n"
            "## tool-gaps.md: every blocked lever answered for this plan (%d unanswered)\n%s\n"
            % (vcode, vout, lcode, lout, ccode, cout, len(unanswered),
               "\n".join("error tool-gaps.unanswered %s: answer it in the table (used: <key>, workaround; or not "
                         "used: <reason>)" % g for g in unanswered)))
    return vcode == 0 and lcode == 0 and ccode == 0 and not unanswered, text, lint


TIMELINES = "plan.timelines.md"


def write_timelines(runner, run_dir, args):
    """RUN/plan.timelines.md: the research and focus timelines of the current plan, computed by the facts CLI
    at every gate, so the strategist never copies dates by hand and the critic checks against the same
    output."""
    spec = read_json(os.path.join(run_dir, "run_spec.json"), {}) or {}
    plan_path = os.path.join(run_dir, "plan.json")
    plan = read_json(plan_path, {}) or {}
    tag, date, mode = spec.get("tag"), spec.get("start_date"), spec.get("mode", "fair")
    if not tag or not date:
        return None
    facts = [sys.executable, os.path.join(ROOT, "scripts", "hoi4facts", "facts.py")] + game_args(args) + [
        "--date", date, "--mode", mode, "--tag", tag]
    parts = ["# Timelines of the current plan (written by the gates; do not edit)", "",
             "Computed from RUN/plan.json by the facts CLI at every gate run. Cite these dates as "
             "[D research-timeline] and [D focus-timeline]; they replace hand-copied tables.", ""]
    rcode, rout = runner.run(facts + ["research-timeline", plan_path])
    parts += ["## Research (`facts.py research-timeline RUN/plan.json`, exit %s)" % rcode, "", "```",
              rout.rstrip(), "```", ""]
    focus = [f for f in plan.get("focus_path") or [] if isinstance(f, str)]
    if focus:
        fcode, fout = runner.run(facts + ["focus-timeline", tag, "--date", date] + focus)
        parts += ["## Focus (`facts.py focus-timeline %s --date %s <focus_path>`, exit %s)" % (tag, date, fcode),
                  "", "```", fout.rstrip(), "```", ""]
    if runner.dry:
        return None
    path = os.path.join(run_dir, TIMELINES)
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(parts))
    return path


def max_total(critic, scores):
    """The rubric maximum: the critic file's `max` when it is a positive integer, never below 2 points per
    scored row; MAX_TOTAL otherwise."""
    declared = critic.get("max")
    if isinstance(declared, bool) or not isinstance(declared, int) or declared <= 0:
        return MAX_TOTAL
    return max(declared, 2 * len(scores))


def pass_mark(maximum):
    """PASS_TOTAL of MAX_TOTAL, scaled to another maximum and rounded up (16 of 22 for the 11-row rubric)."""
    if maximum == MAX_TOTAL:
        return PASS_TOTAL
    return -(-PASS_TOTAL * maximum // MAX_TOTAL)


def review_passes(critic, allow_majors=False):
    if not critic:
        return False, "no critic output"
    scores = {str(k): v for k, v in (critic.get("scores") or {}).items()}
    total = critic.get("total", sum(v for v in scores.values() if isinstance(v, int)))
    maximum = max_total(critic, scores)
    mark = pass_mark(maximum)
    zeros = [r for r in CRITICAL_ROWS if scores.get(r) == 0]
    def open_ids(severity):
        return [f.get("id") for f in critic.get("findings") or [] if f.get("severity") == severity
                and f.get("status", "open") == "open"]
    blockers, majors = open_ids("blocker"), open_ids("major")
    # A score above the mark does not excuse an open major finding: the strategist answers it while rounds
    # remain (allow_majors is set only for the last round, where the plan ships with them reported).
    ok = total >= mark and not zeros and not blockers and (allow_majors or not majors)
    return ok, "total %s/%s (pass %s), zero rows %s, open blockers %s, open majors %s" % (
        total, maximum, mark, zeros or "none", blockers or "none", majors or "none")


def latest(run_dir, pattern):
    files = sorted(glob.glob(os.path.join(run_dir, "review", pattern)),
                   key=lambda p: int("".join(c for c in os.path.basename(p) if c.isdigit()) or 0))
    return files[-1] if files else None


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        h.update(f.read())
    return h.hexdigest()


def freeze(run_dir, report):
    plan_path = os.path.join(run_dir, "plan.json")
    plan = read_json(plan_path, {})
    manifest = os.path.join(run_dir, "dossier", "manifest.json")
    agents = read_json(os.path.join(run_dir, "agents-manifest.json"), {}).get("roles", {})
    meta = dict(plan.get("_meta") or {})
    reused = next((st.get("from") for st in report.get("steps") or [] if st.get("step") == "reuse"), None)
    if reused:
        meta["reused_from"] = reused
    meta.update({
        "version": meta.get("version", 1),
        "parent": meta.get("parent"),
        "dossier_hash": sha256(manifest) if os.path.exists(manifest) else None,
        "models": {r: {"provider": a.get("provider"),
                       "model": a.get("resolved_model") or a.get("model_alias") or a.get("model"),
                       "effort": a.get("effort_applied") or a.get("effort")} for r, a in agents.items()}
        or meta.get("models"),
        "created": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "lint_summary": report.get("lint_summary"),
        "review": report.get("review"),
    })
    plan["_meta"] = meta
    write_json(plan_path, plan)


LIBRARY = os.path.join(ROOT, "skills", "hoi4-campaign-playbook", "plans", "reviewed")
PLAN_FILES = ("plan.json", "plan.md", "plan.brief.md", "tool-gaps.md", "plan.timelines.md")


def _norm(text):
    return " ".join(str(text or "").split()).casefold()


def request_key(spec):
    """What makes two requests the same plan: the user's words and the scenario settings."""
    return (spec.get("tag"), _norm(spec.get("start_date")).replace("-", "."), _norm(spec.get("mode") or "fair"),
            _norm(spec.get("goal")), tuple(_norm(c) for c in spec.get("goal_clarifications") or []),
            _norm(spec.get("horizon")), _norm(spec.get("difficulty") or "normal"),
            _norm(spec.get("ai") or "historical"))


def _content(manifest):
    """(DLCs, mods) a dossier manifest resolved, as sets; None for a part it does not record."""
    manifest = manifest or {}
    dlcs = manifest.get("dlcs")
    mods = manifest.get("mods")
    return (frozenset(dlcs) if isinstance(dlcs, list) else None,
            frozenset(m if isinstance(m, str) else json.dumps(m, sort_keys=True) for m in mods)
            if isinstance(mods, list) else None)


def libraries(args):
    out = list(getattr(args, "library", None) or [])
    out += [p for p in (os.environ.get("HOI4_LIVE_PLAN_LIBRARY") or "").split(os.pathsep) if p]
    out.append(LIBRARY)
    return out


def find_reusable(spec, manifest, dirs):
    """The first library entry made for this request with a passed review and the same DLCs and mods, or None."""
    want, (dlcs, mods) = request_key(spec), _content(manifest)
    for lib in dirs:
        if not os.path.isdir(lib):
            continue
        for name in sorted(os.listdir(lib)):
            entry = os.path.join(lib, name)
            plan = read_json(os.path.join(entry, "plan.json"))
            espec = read_json(os.path.join(entry, "run_spec.json"))
            if not isinstance(plan, dict) or not isinstance(espec, dict) or request_key(espec) != want:
                continue
            if not ((plan.get("_meta") or {}).get("review") or {}).get("pass"):
                continue
            edlcs, emods = _content(read_json(os.path.join(entry, "dossier-manifest.json")))
            if (dlcs is not None and edlcs is not None and dlcs != edlcs) or \
                    (mods is not None and emods is not None and mods != emods):
                continue
            return entry
    return None


def copy_entry(entry, run_dir):
    import shutil
    for name in PLAN_FILES:
        if os.path.exists(os.path.join(entry, name)):
            shutil.copyfile(os.path.join(entry, name), os.path.join(run_dir, name))
    review = os.path.join(entry, "review")
    if os.path.isdir(review):
        for name in os.listdir(review):
            if name.endswith(".json"):
                shutil.copyfile(os.path.join(review, name), os.path.join(run_dir, "review", name))


def publish(run_dir, dest):
    """Copy a passed run (plan files, run spec, dossier manifest, review) into a library folder."""
    import shutil
    run_dir = os.path.abspath(run_dir)
    plan = read_json(os.path.join(run_dir, "plan.json"), {})
    if not ((plan.get("_meta") or {}).get("review") or {}).get("pass"):
        raise SystemExit("publish: RUN/plan.json has no passed review in _meta (run strategize to the end first)")
    os.makedirs(os.path.join(dest, "review"), exist_ok=True)
    for name in PLAN_FILES + ("run_spec.json",):
        if os.path.exists(os.path.join(run_dir, name)):
            shutil.copyfile(os.path.join(run_dir, name), os.path.join(dest, name))
    if os.path.exists(os.path.join(run_dir, "dossier", "manifest.json")):
        shutil.copyfile(os.path.join(run_dir, "dossier", "manifest.json"), os.path.join(dest, "dossier-manifest.json"))
    for name in os.listdir(os.path.join(run_dir, "review")):
        if name.endswith(".json"):
            shutil.copyfile(os.path.join(run_dir, "review", name), os.path.join(dest, "review", name))
    return dest


def strategize(args, runner=None):
    runner = runner or Runner(args.dry_run)
    run_dir = os.path.abspath(args.run)
    os.makedirs(os.path.join(run_dir, "review"), exist_ok=True)
    spec = spec_from(args, run_dir)
    report = {"run": run_dir, "spec": spec, "steps": [], "ok": False}
    started = time.time()

    start_at = getattr(args, "from_step", None) or "dossier"
    # 1. dossier (deterministic; the strategist re-uses it)
    dossier = os.path.join(run_dir, "dossier")
    if start_at == "dossier" and (runner.dry or not os.path.exists(os.path.join(dossier, "dossier.md"))):
        code, out = runner.run([sys.executable, os.path.join(ROOT, "scripts", "hoi4facts", "facts.py")]
                               + game_args(args) + ["--mode", spec.get("mode", "fair"),
                               "dossier", "build", "--tag", spec["tag"], "--date", spec["start_date"],
                               "--out", dossier])
        report["steps"].append({"step": "dossier", "exit": code})
        if code != 0:
            return finish(run_dir, report, "dossier build failed:\n" + out[-3000:], started)

    # The levers no tool carries yet, from the gap register: the strategist plans and reports them, never drops them.
    write_blocked_levers(run_dir)

    # 1b. work pack (deterministic, cached): the facts every plan needs, the schema digest and the lint rules,
    # computed once by code so no model call spends turns fetching them (scripts/strategist/pack.py)
    t0 = time.time()
    code, out = runner.run([sys.executable, os.path.join(HERE, "pack.py"), "--run", run_dir] + game_args(args))
    report["steps"].append({"step": "pack", "exit": code, "seconds": round(time.time() - t0)})
    if code != 0:
        log("work pack failed (the models fetch the facts themselves): %s" % out[-500:])

    # Reuse a reviewed plan made for the same request: the gates decide whether it still holds on this install.
    reused = None
    if start_at == "dossier" and not getattr(args, "no_reuse", False) and not runner.dry \
            and not os.path.exists(os.path.join(run_dir, "plan.json")):
        reused = find_reusable(spec, read_json(os.path.join(dossier, "manifest.json")), libraries(args))
        if reused:
            copy_entry(reused, run_dir)
            log("reusing the reviewed plan %s" % reused)
            report["steps"].append({"step": "reuse", "from": os.path.basename(reused)})
            start_at = "gates"

    if not runner.dry:
        seed_tool_gaps(run_dir)  # after a reuse: a reviewed plan brings its answered table

    # 2. strategist (skipped when resuming from the gates or the critic, or reusing a plan)
    if start_at == "dossier":
        code, _, secs = agent(runner, "strategist", run_dir, "strategist.md", args)
        report["steps"].append({"step": "strategist", "exit": code, "seconds": round(secs)})
        if code != 0:
            return finish(run_dir, report, "strategist exited %s" % code, started)
    elif not reused:
        report["steps"].append({"step": "resume", "from": start_at})

    # 3-5. gates, critic, answers
    changed = False
    for review_round in range(1, REVIEW_ROUNDS + 1):
        for fix in range(0 if (start_at == "critic" and review_round == 1) else FIX_ROUNDS + 1):
            ok, text, lint = gates(runner, run_dir, args)
            report["lint_summary"] = {k: lint.get(k) for k in ("errors", "warnings") if k in lint} or None
            report["steps"].append({"step": "gates", "round": review_round, "fix": fix, "ok": ok})
            if ok or runner.dry:
                break
            if fix == FIX_ROUNDS:
                return finish(run_dir, report, "gates still fail after %d fixes:\n%s" % (FIX_ROUNDS, text[-4000:]),
                              started)
            gate_file = os.path.join(run_dir, "gates-%d-%d.txt" % (review_round, fix + 1))
            with open(gate_file, "w", encoding="utf-8") as f:
                f.write(text)
            code, _, secs = agent(runner, "strategist", run_dir, "fix.md", args)
            changed = True
            report["steps"].append({"step": "fix", "file": os.path.basename(gate_file), "exit": code,
                                    "seconds": round(secs)})
        if reused and not changed:
            # the reviewed plan passes the gates on this install unchanged: its stored review stands
            stored = (read_json(os.path.join(run_dir, "plan.json"), {}).get("_meta") or {}).get("review") or {}
            report["review"] = dict(stored, reused_from=os.path.basename(reused))
            break
        before = latest(run_dir, "critic-*.json")
        code, _, secs = agent(runner, "critic", run_dir, "critic.md", args)
        critic_path = latest(run_dir, "critic-*.json")
        if not runner.dry and critic_path == before:
            # No new review file: a tool or permission problem, not a failed review. Run once more.
            log("critic wrote no review file; running it once more")
            code, _, more = agent(runner, "critic", run_dir, "critic.md", args)
            secs += more
            critic_path = latest(run_dir, "critic-*.json")
            if critic_path == before:
                report["steps"].append({"step": "critic", "round": review_round, "exit": code,
                                        "seconds": round(secs), "detail": "no review file"})
                return finish(run_dir, report, "the critic wrote no RUN/review/critic-<n>.json twice "
                              "(check its tools and RUN/review/*.message.md)", started)
        critic = read_json(critic_path) if critic_path else None
        passed, why = review_passes(critic, allow_majors=review_round == REVIEW_ROUNDS)
        report["review"] = {"round": review_round, "pass": passed, "detail": why,
                            "file": os.path.basename(critic_path) if critic_path else None}
        report["steps"].append({"step": "critic", "round": review_round, "exit": code, "pass": passed,
                                "detail": why, "seconds": round(secs)})
        if passed or runner.dry:
            break
        if review_round == REVIEW_ROUNDS:
            return finish(run_dir, report, "review failed after %d rounds: %s" % (REVIEW_ROUNDS, why), started)
        code, _, secs = agent(runner, "strategist", run_dir, "answer.md", args)
        report["steps"].append({"step": "answer", "round": review_round, "exit": code, "seconds": round(secs)})

    # 6. freeze
    report["ok"] = True
    if not runner.dry:
        freeze(run_dir, report)
    return finish(run_dir, report, None, started)


def write_blocked_levers(run_dir):
    """RUN/blocked-levers.md: the gap register's not-executable rows (scripts/hoi4facts/gap_register.py
    blocked). Returns the path, or None when the register cannot be read."""
    sys.path.insert(0, os.path.join(ROOT, "scripts"))
    try:
        from hoi4facts import gap_register
        text = gap_register.blocked_markdown()
    except Exception as e:  # noqa: BLE001 - the run goes on without the list; the lint still warns
        log("gap register unreadable (%s): RUN/blocked-levers.md not written" % e)
        return None
    finally:
        sys.path.pop(0)
    path = os.path.join(run_dir, "blocked-levers.md")
    with open(path, "w", encoding="utf-8") as f:
        f.write(text)
    return path


GAP_RULES = ("conquest.tool_gap", "gap.blocked")
GAP_TABLE = "## Blocked levers: the answer for this plan"
GAP_TABLE_HEAD = "| lever | area | payoff | this plan |"


def seed_tool_gaps(run_dir):
    """Start RUN/tool-gaps.md with one row per blocked lever of the gap register and `?` in the answer column.
    The strategist answers every row (used, with the key and the workaround, or not used, with the reason)
    and the gates fail while one is open: in both 16 Nov runs the critic's round 1 raised a major finding for
    blocked levers neither planned nor answered, and each such finding cost a 17 to 25 minute answer round."""
    sys.path.insert(0, os.path.join(ROOT, "scripts"))
    try:
        from hoi4facts import gap_register
        rows = gap_register.blocked()
        domains = gap_register.DOMAINS
    except Exception as e:  # noqa: BLE001 - without the register there is nothing to answer
        log("gap register unreadable (%s): no blocked-lever table in tool-gaps.md" % e)
        return None
    finally:
        sys.path.pop(0)
    path = os.path.join(run_dir, "tool-gaps.md")
    old = ""
    if os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            old = f.read()
    if GAP_TABLE in old:
        return path
    table = [GAP_TABLE, "",
             "Seeded by strategize.py from RUN/blocked-levers.md (the descriptions are there). Replace each `?` with",
             "`used: <plan key>; <payoff for this goal>; workaround: <what the tools do carry>` or `not used: <why this",
             "goal does not need it>`. Edit the rows in place; the gates fail while a row is `?` or missing.", "",
             GAP_TABLE_HEAD, "|---|---|---|---|"]
    order = getattr(gap_register, "PAYOFFS", ())
    for r in sorted(rows, key=lambda r: (order.index(r["payoff"]) if r.get("payoff") in order else 9,
                                         r.get("domain", ""), r["id"])):
        table.append("| `%s` | %s | %s | ? |" % (r["id"], domains.get(r.get("domain"), r.get("domain")),
                                                r.get("payoff")))
    with open(path, "w", encoding="utf-8") as f:
        f.write((old.rstrip() + "\n\n" if old.strip() else "# Tool gaps\n\n") + "\n".join(table) + "\n")
    return path


def tool_gaps_unanswered(run_dir):
    """Lever ids of the seeded table still `?` (or empty), plus seeded ids whose row is gone. [] when the table
    was never seeded."""
    sys.path.insert(0, os.path.join(ROOT, "scripts"))
    try:
        from hoi4facts import gap_register
        ids = [r["id"] for r in gap_register.blocked()]
    except Exception:  # noqa: BLE001
        ids = []
    finally:
        sys.path.pop(0)
    try:
        with open(os.path.join(run_dir, "tool-gaps.md"), encoding="utf-8") as f:
            text = f.read()
    except OSError:
        return []
    if GAP_TABLE not in text:
        return []
    answers = {}
    for line in text[text.index(GAP_TABLE):].splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")] if line.startswith("|") else []
        if len(cells) >= 2:
            for lever in re.findall(r"`([a-z_]+\.[a-z0-9_.]+)`", cells[0]):
                answers[lever] = cells[-1]
    return [i for i in ids if answers.get(i, "").strip("` ") in ("", "?")]


def lint_tool_gaps(run_dir):
    """Append the last lint's conquest.tool_gap and gap.blocked findings (a lever the plan needs that no tool
    carries) to tool-gaps.md, once each, so the gap reaches the operator even when the strategist did not write
    it."""
    try:
        with open(os.path.join(run_dir, "lint.json"), encoding="utf-8") as f:
            findings = json.load(f).get("findings") or []
    except (OSError, ValueError, AttributeError):
        return []
    path = os.path.join(run_dir, "tool-gaps.md")
    old = ""
    if os.path.exists(path):
        with open(path, encoding="utf-8") as f:
            old = f.read()
    new = ["- %s: %s" % (f.get("where"), f.get("message")) for f in findings
           if f.get("rule") in GAP_RULES and "- %s:" % f.get("where") not in old]
    if new:
        with open(path, "a", encoding="utf-8") as f:
            f.write(("\n" if old else "") + "## Levers the plan needs that no tool carries (lint conquest.tool_gap, "
                    "gap.blocked)\n\n"
                    + "\n".join(new) + "\n")
    return new


def finish(run_dir, report, failure, started):
    report["seconds"] = round(time.time() - started)
    lint_tool_gaps(run_dir)
    gaps = os.path.join(run_dir, "tool-gaps.md")
    if os.path.exists(gaps):
        # The models may not change the tools; they report gaps here for the operator to fix.
        report["tool_gaps"] = gaps
        log("tool gaps reported: " + gaps)
    if failure:
        report["failure"] = failure
        with open(os.path.join(run_dir, "strategist-blocked.md"), "w", encoding="utf-8") as f:
            f.write("# Strategist blocked\n\n%s\n" % failure)
        log("BLOCKED: " + failure.splitlines()[0])
    write_json(os.path.join(run_dir, "strategize-report.json"), report)
    return report


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--run", required=True)
    ap.add_argument("--tag")
    ap.add_argument("--date")
    ap.add_argument("--goal")
    ap.add_argument("--mode", default="fair", choices=["fair", "meta"])
    ap.add_argument("--horizon")
    ap.add_argument("--game")
    ap.add_argument("--userdir")
    ap.add_argument("--mod", action="append")
    ap.add_argument("--provider")
    ap.add_argument("--from", dest="from_step", choices=["dossier", "gates", "critic"], default="dossier",
                    help="resume an existing run: gates (validate, lint, calls, then review) or critic")
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--library", action="append", help="folder of reviewed plans to reuse (repeat; see Reuse)")
    ap.add_argument("--no-reuse", action="store_true", help="always plan afresh")
    ap.add_argument("--publish", metavar="DIR", help="copy the passed run in --run into a library folder and exit")
    ap.add_argument("--gaps-check", action="store_true",
                    help="only list the blocked levers RUN/tool-gaps.md leaves unanswered (exit 1 when any)")
    args = ap.parse_args(argv)
    if args.publish:
        print(publish(args.run, args.publish))
        return 0
    if args.gaps_check:
        left = tool_gaps_unanswered(os.path.abspath(args.run))
        print("\n".join(left) if left else "every blocked lever answered")
        return 1 if left else 0
    report = strategize(args)
    if args.dry_run:
        print(json.dumps(report.get("steps"), indent=1))
    print(json.dumps({"ok": report["ok"], "seconds": report["seconds"], "review": report.get("review"),
                      "failure": (report.get("failure") or "").splitlines()[:1]}))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
