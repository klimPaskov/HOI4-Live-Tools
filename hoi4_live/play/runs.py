"""The run folder of a native campaign and the install's pointer to the active one.

    <prefix>/runs/<stamp>-<TAG>/      private (0700): everything of one run
        plan.json  input.json  request.json  BRIEF.txt  prompt.txt
        session.json                      written by `hoi4-live-native --attach`
        status.json  endpoint.json        written by the supervisor
        calls.jsonl  supervisor.log  native.stderr  report.md
        user-files/                       private copies of the game's settings files at the start
    <prefix>/runs/active.json         the run the `hoi4-live-play` proxy forwards to
"""

import datetime
import hashlib
import json
import os
import shutil

ACTIVE = "active.json"
USER_FILES = ("settings.txt", "pdx_settings.txt", "dlc_load.json")
SAVES = "save games"


def runs_dir(prefix):
    return os.path.join(prefix, "runs")


def private_dir(path):
    os.makedirs(path, mode=0o700, exist_ok=True)
    if os.name != "nt":
        os.chmod(path, 0o700)
    return path


def write_private(path, text):
    """Write text readable by this user only, atomically (a temporary file in the same folder, then a rename)."""
    temporary = "%s.%d.tmp" % (path, os.getpid())
    flags = os.O_WRONLY | os.O_CREAT | os.O_TRUNC | getattr(os, "O_NOFOLLOW", 0)
    fd = os.open(temporary, flags, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as f:
        f.write(text)
    os.replace(temporary, path)
    return path


def write_json(path, data):
    return write_private(path, json.dumps(data, indent=2, ensure_ascii=False) + "\n")


def read_json(path):
    try:
        with open(path, encoding="utf-8") as f:
            data = json.load(f)
        return data if isinstance(data, dict) else None
    except (OSError, ValueError):
        return None


def new_run(prefix, tag, now=None):
    stamp = (now or datetime.datetime.now()).strftime("%Y%m%d-%H%M%S")
    private_dir(runs_dir(prefix))
    base = os.path.join(runs_dir(prefix), "%s-%s" % (stamp, tag))
    for n in range(1, 100):
        path = base if n == 1 else "%s-%d" % (base, n)
        try:
            os.mkdir(path, 0o700)
            return path
        except FileExistsError:
            continue
    raise FileExistsError(base)


def active_path(prefix):
    return os.path.join(runs_dir(prefix), ACTIVE)


def active_run(prefix):
    """The active run's pointer ({"run", "endpoint", "pid"}) or None."""
    data = read_json(active_path(prefix))
    if not data or not data.get("run") or not os.path.isdir(data["run"]):
        return None
    return data


def set_active(prefix, run, endpoint, pid):
    private_dir(runs_dir(prefix))
    return write_json(active_path(prefix), {"run": run, "endpoint": endpoint, "pid": pid})


def clear_active(prefix, run):
    """Remove the pointer only when it still names `run` (a newer run keeps its own)."""
    data = read_json(active_path(prefix))
    if data and os.path.normcase(os.path.abspath(data.get("run") or "")) == os.path.normcase(os.path.abspath(run)):
        try:
            os.remove(active_path(prefix))
        except OSError:
            pass


# ---------------------------------------------------------------------------------------------------------------
# the user's files: recorded at the start, compared at the end, never written


def _sha256(path):
    try:
        digest = hashlib.sha256()
        with open(path, "rb") as f:
            for block in iter(lambda: f.read(1 << 20), b""):
                digest.update(block)
        return digest.hexdigest()
    except OSError:
        return None


def user_files_state(user_dir):
    state = {name: _sha256(os.path.join(user_dir, name)) for name in USER_FILES}
    try:
        state[SAVES] = sorted(os.listdir(os.path.join(user_dir, SAVES)))
    except OSError:
        state[SAVES] = None
    return state


def record_user_files(user_dir, run):
    """Hash the game's settings files and list the saves; keep a private copy of each settings file."""
    copies = private_dir(os.path.join(run, "user-files"))
    for name in USER_FILES:
        source = os.path.join(user_dir, name)
        if os.path.isfile(source):
            shutil.copyfile(source, os.path.join(copies, name))
            if os.name != "nt":
                os.chmod(os.path.join(copies, name), 0o600)
    state = {"user_dir": user_dir, "files": user_files_state(user_dir)}
    write_json(os.path.join(run, "user-files.json"), state)
    return state


def compare_user_files(run):
    """[(name, what changed)] since the start of the run; [] when nothing changed or nothing was recorded."""
    before = read_json(os.path.join(run, "user-files.json"))
    if not before:
        return []
    now = user_files_state(before["user_dir"])
    changed = []
    for name in USER_FILES:
        if before["files"].get(name) != now.get(name):
            changed.append((name, "changed during the run; the copy from the start is in %s"
                            % os.path.join(run, "user-files", name)))
    old, new = before["files"].get(SAVES) or [], now.get(SAVES) or []
    added = sorted(set(new) - set(old))
    if added:
        changed.append((SAVES, "new saves: " + ", ".join(added[:10])))
    removed = sorted(set(old) - set(new))
    if removed:
        changed.append((SAVES, "saves no longer there: " + ", ".join(removed[:10])))
    return changed


# ---------------------------------------------------------------------------------------------------------------
# the brief

HOW_TO_CALL = (
    "HOW TO CALL: your game tools are the hoi4-live-play MCP tools. A line below of the form\n"
    "    TOOL '{json}'      means: call TOOL with that JSON object as its arguments;\n"
    "    hoi4_batch '[[\"tool\",{...}],...]'  means: call hoi4_batch with {\"calls\": [[\"tool\",{...}],...]}\n"
    "Lease tokens and request ids are added for you. hoi4_run {\"op\":\"status\"} shows the run; when you stop,\n"
    "call hoi4_run {\"op\":\"stop\"} once (it writes the run report and leaves the game running).\n\n")
PLACEHOLDERS = ("TITLE", "RUN", "SPEED", "CALL", "BATCH", "PLAN_BRIEF", "SITREP", "LOG", "STOP_DATE", "MINUTES")


def plan_brief_text(plan):
    """The plan's player section: a `<plan>.brief.md` beside a shipped plan, else the plan notes to read."""
    brief = plan.get("brief")
    if not brief and plan["path"].endswith(".plan.json"):
        brief = plan["path"][:-len(".plan.json")] + ".brief.md"
    if brief and os.path.isfile(brief):
        with open(brief, encoding="utf-8") as f:
            return f.read().strip()
    guide = plan.get("guide")
    if guide:
        return ("Read the plan notes once before you unpause: %s (checkpoints, the opening while paused, the war\n"
                "phases, contingencies, the do-not list). They are the strategist's player items for this plan."
                % guide)
    return "The plan has no player notes: watch the feed and do only what wake_player hands you."


def render_brief(template, plan, goal, run, minutes=180, speed=3):
    values = {
        "TITLE": "%s: %s" % (plan["tag"], goal),
        "RUN": run,
        "SPEED": str(speed),
        "CALL": "",
        "BATCH": "hoi4_batch",
        "PLAN_BRIEF": plan_brief_text(plan),
        "SITREP": "hoi4_observe '{\"since\":0}'",
        "LOG": os.path.join(run, "log.md"),
        "STOP_DATE": str(plan.get("stop") or "the plan horizon"),
        "MINUTES": str(minutes),
    }
    text = template
    for key in PLACEHOLDERS:
        text = text.replace("{%s}" % key, values[key])
    # A line that began with {CALL} keeps one leading space; tidy it.
    text = "\n".join(line.replace("     hoi4_", "    hoi4_", 1) if line.startswith("     hoi4_") else line
                     for line in text.split("\n"))
    return HOW_TO_CALL + text


def instruction(run):
    return "Read %s and follow it word for word." % os.path.join(run, "BRIEF.txt")
