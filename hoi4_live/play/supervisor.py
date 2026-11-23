"""The supervisor of one native campaign: python -m hoi4_live.play.supervisor --prefix INSTALL --run RUN --native BIN

`hoi4-live play` starts it in the background after attaching to the user's game. It owns the run until the game
closes or the run is stopped:

    start       start the native runtime on the run's session, retrying while the user is still in the menus;
                take the input lease, check the country (and, for a continued campaign, that the loaded game is
                not older than where the last segment ended), reconcile the plan's names with the live game
                (play/reconcile.py: templates, generals, wings, production, first research; reads only), bring the
                game to the front once, switch the executor on
    serve       answer the `hoi4-live-play` proxy on a private endpoint (authenticated, this user only)
    each call   add the lease token and a request id where the tool takes them; refuse an identical action that
                succeeded within DUPLICATE_WINDOW seconds with its earlier result (`already_done`); bring the
                game to the front once when input found another window in front; log the call without tokens
    idle guard  a real campaign never pauses itself: the game keeps running between player calls. Only after a
                long silence (no player call for the run's idle_pause_minutes, default 15; 0 switches it off) is the
                player taken for dead: at half that time a wake_player change is raised and recorded, at the full
                time the executor goes off and a running game is paused, with the time and reason recorded for the
                report. The next player call lifts that pause (executor on, the game running again); any player
                call resets the timer. A pause the player or the user asks for is theirs and never lifted here.
    health      the game or the runtime gone: stop
    end         the plan's goal reached (the executor's goal_reached change) or its goal.deadline passed in the
                game: stop with that reason (HOI4_LIVE_END=off keeps the run going)
    limits      the machine's swap and load noted at the start; the run ends with a plain message when the game's
                memory passes HOI4_LIVE_GAME_MEMORY_GB (default 10; 0 off: a segment boundary, saved, continued with
                `hoi4-live play --continue`) or its date stands still while it runs for HOI4_LIVE_STALL_MINUTES
                (default 5; 0 off)
    watch       every SAMPLE_EVERY seconds one line in watch.jsonl: game date, war stats, each army's supply and
                organisation (reads only), for the report and for looking back at how the war went
    silence     the player taken for gone: no player call for the run's player_silent_minutes (default 45; 0
                off), for example after the model's usage limit: the run ends as player_silent, with the game paused
                and saved, instead of the executor playing on alone for hours
    until       the game date the user gave (`hoi4-live play ... --until 1938.01.01`, request.json "until"):
                the run ends as end_date
    dialog      a system prompt on screen (a permission or password dialog; the runtime refuses input while it is
                there): the player and the status are told at once; still there after DIALOG_MINUTES, the run ends
                as system_dialog (nothing touches the dialog)
    feed        every change of the runtime's feed in feed.jsonl, and every PRODUCTION_EVERY seconds the
                production lines, stockpile and economy in watch.jsonl ("prod"), for `hoi4-live report`
    stop        the state for the gate (gate/<date>.json: factories, armies, equipment, templates, air, navy,
                alerts); at the supervisor's own ends (goal, deadline, end date, silence, memory limit) the game is
                paused; the game's own save (hoi4_save) so the campaign continues in a new segment
                (`hoi4-live play --continue`); executor off, lease released, runtime closed, report.md written, the
                active-run pointer removed; the game itself is never closed

Every decision is in the Supervisor class, which takes its runtime, clock and helpers as arguments for tests.
"""

import argparse
import copy
import json
import os
import re
import secrets
import subprocess
import sys
import tempfile
import threading
import time

from . import clockfile, game, reconcile, report, runs
from .native import Native

IDLE_PAUSE_MINUTES = runs.IDLE_PAUSE_MINUTES
DUPLICATE_WINDOW = 45
RENEW_EVERY = 300
LEASE_SECONDS = 900
CAMPAIGN_WAIT = 1800
# A continued campaign may load a save written a little before the boundary (the game's own autosave).
RESUME_SLACK_DAYS = 45
RETRY_EVERY = 5
TICK = 5
HEALTH_EVERY = 15
SAMPLE_EVERY = 60
PRODUCTION_EVERY = 300
GAME_MEMORY_GB = 10.0
STALL_MINUTES = 5.0
PLAYER_SILENT_MINUTES = runs.PLAYER_SILENT_MINUTES
DIALOG_MINUTES = 5.0
FEED_PAGE, FEED_PAGES = 200, 10
# The supervisor's own ends: nobody plays after them, so the game is paused before the save.
PAUSE_AT_END = {"goal_reached", "deadline", "end_date", "player_silent", "game_memory"}
# Ends where input no longer works or the game is gone: no save is tried.
NO_SAVE = {"game_closed", "game_stalled", "system_dialog", "supervisor_exit"}
# What the gate reads (reads only), kept at the end of each segment and on demand for `hoi4-live report`.
GATE_READS = (("hoi4_observe", {"since": 999999}), ("hoi4_economy", {}), ("hoi4_armies", {"threat": False}),
              ("hoi4_alerts", {}), ("hoi4_country_status", {}), ("hoi4_supply", {"summary": False}),
              ("hoi4_stockpile", {}), ("hoi4_template_get", {}), ("hoi4_air", {}), ("hoi4_fleets", {}),
              ("hoi4_air_regions", {}), ("hoi4_resources", {}))
HOLDER = "hoi4-live-play"
# Actions whose repeat is meaningful (a clock change, a plan step) or that the supervisor owns.
NOT_DEDUPED = {"hoi4_time", "hoi4_army_plan", "hoi4_plan_run", "hoi4_autopilot", "hoi4_input_lease"}
# The supervisor's own tools: the player never holds the lease or runs the executor's step itself.
HIDDEN = {"hoi4_input_lease", "hoi4_plan_run"}
# Tools that take the lease only with one argument (a read without it): the answer to a diplomatic proposal.
LEASED_WITH = {"hoi4_diplomacy_incoming": "answer"}
GONE = {"process_exited", "process_absent", "process_lost", "ownership_changed"}
FATAL_START = {"unsupported_build", "task_access_denied", "input_permission_denied", "ownership_changed",
               "process_absent", "platform_unavailable", "invalid_session", "permission_denied"}
START_STEPS = {
    "campaign_unavailable": "start or load the campaign in the game (it is still in the menus)",
    "invalid_input_binding": "the loaded campaign is another country: load or start the plan's country",
    "input_geometry_unverified": "set the game to Windowed, 1920x1080, UI scaling 1.0 (Options > Graphics)",
    "input_geometry_changed": "leave the game window as it is (size and screen) while the run starts",
    "input_target_not_frontmost": "click the game window so it is in front",
    "task_access_denied": "memory access is not allowed yet: run hoi4-live doctor and do its step",
    "unsupported_build": "this game build is not one the native runtime reads: run hoi4-live doctor",
    "input_permission_denied": "allow Accessibility for the app you ran hoi4-live play in (System Settings > "
                               "Privacy & Security > Accessibility), then run hoi4-live play again",
    "process_absent": "the game closed",
    "ownership_changed": "the game closed or was restarted: run hoi4-live play again",
}
EXTRA_TOOLS = [
    {"name": "hoi4_batch",
     "description": "Several tool calls in one turn, in order (at speed 3 every model turn costs game days). Each "
                    "item is [tool, {arguments}]; every call runs and each result is returned (bodies cut to about "
                    "1500 characters unless full is true). Lease tokens and request ids are added for you.",
     "inputSchema": {"type": "object", "properties": {
         "calls": {"type": "array", "minItems": 1, "maxItems": 20,
                   "items": {"type": "array", "minItems": 1, "maxItems": 2}},
         "full": {"type": "boolean"}}, "required": ["calls"], "additionalProperties": False}},
    {"name": "hoi4_run",
     "description": "The campaign run. {\"op\":\"status\"}: run folder, game date, executor, idle pause. "
                    "{\"op\":\"stop\"}: end the run once when you are done: executor off, input released, "
                    "report.md written in the run folder; the game keeps running. {\"op\":\"planner\"}: the rolling "
                    "planner's updates and model use. {\"op\":\"plan_patch\",\"update\":N,\"patch\":{...}}: answer a "
                    "plan update the supervisor asked you for (the gates check it before it is applied).",
     "inputSchema": {"type": "object", "properties": {
         "op": {"type": "string", "enum": ["status", "stop", "planner", "plan_patch"]},
         "update": {"type": "integer", "minimum": 1}, "patch": {"type": "object"}},
         "required": ["op"], "additionalProperties": False}},
]


def error(code, message):
    return {"isError": True, "body": {"error": {"code": code, "message": message}}}


def error_code(body):
    if isinstance(body, dict) and isinstance(body.get("error"), dict):
        return str(body["error"].get("code") or "")
    return ""


def stderr_code(text):
    """The error code a native runtime that failed to start wrote last ("code: message" or Error { code: ".." })."""
    codes = re.findall(r'code: "([a-z_]+)"', text or "") or re.findall(r"^([a-z][a-z_]+): ", text or "", re.M)
    return codes[-1] if codes else ""


def branches(schema):
    """The schema and each of its oneOf / anyOf alternatives (a tool may take different arguments per op)."""
    schema = schema or {}
    return [schema] + [b for key in ("oneOf", "anyOf") for b in (schema.get(key) or []) if isinstance(b, dict)]


def takes(tool, key):
    return any(key in (b.get("properties") or {}) for b in branches(tool.get("inputSchema")))


def player_tool(tool):
    """The tool as the player sees it: no lease token, request id optional, in every alternative of its schema."""
    shown = copy.deepcopy(tool)
    for schema in branches(shown.get("inputSchema")):
        (schema.get("properties") or {}).pop("lease_token", None)
        if "required" in schema:
            schema["required"] = [k for k in schema["required"] if k not in ("lease_token", "request_id")]
    return shown


def player_tools(tools):
    return [player_tool(t) for t in tools if t.get("name") not in HIDDEN] + copy.deepcopy(EXTRA_TOOLS)


def shrink(value, budget):
    """Valid JSON within about `budget` characters: long lists keep their first items, long strings are cut."""
    if len(json.dumps(value)) <= budget:
        return value
    if isinstance(value, dict):
        share = max(60, budget // max(1, len(value)))
        return {k: shrink(v, share) for k, v in value.items()}
    if isinstance(value, list):
        out, used = [], 2
        for i, item in enumerate(value):
            part = shrink(item, max(60, budget // 4))
            size = len(json.dumps(part)) + 1
            if used + size > budget and out:
                out.append({"_more": len(value) - i, "_hint": "full:true returns every item"})
                break
            out.append(part)
            used += size
        return out
    if isinstance(value, str) and len(value) > budget:
        return value[:max(20, budget - 3)] + "..."
    return value


class Supervisor:
    def __init__(self, run, prefix, start_native, clock=time.time, sleep=time.sleep, front=None,
                 idle_pause=None, duplicate_window=DUPLICATE_WINDOW, campaign_wait=CAMPAIGN_WAIT,
                 retry_every=RETRY_EVERY, swap=None, game_memory=None, dialogs=None):
        self.run, self.prefix = run, prefix
        self.start_native, self.clock, self.sleep, self.front = start_native, clock, sleep, front
        # Seconds; None reads the run's settings file (and follows a change made to it during the run).
        self.idle_pause_override, self.duplicate_window = idle_pause, duplicate_window
        self.campaign_wait, self.retry_every = campaign_wait, retry_every
        # The machine readers (tests pass their own): swap (used, total) GB and the game's memory in GB.
        self.swap = swap or machine_swap
        self.read_game_memory = game_memory or self.game_memory_gb
        # The system prompts on screen (owner names); tests pass their own; None where it cannot be read.
        self.dialogs = dialogs
        self.dialog_since = None
        self.last_production = None
        self.saved = None
        self.request = runs.read_json(os.path.join(run, "request.json")) or {}
        self.session = runs.read_json(os.path.join(run, "session.json")) or {}
        self.native = None
        self.tools, self.leased, self.takes_request_id = [], set(), set()
        self.token = None
        self.lock = threading.RLock()
        self.done = {}
        self.notes = []
        self.counter = 0
        self.started = clock()
        self.last_player = self.started
        self.last_renew = self.started
        self.last_health = self.started
        self.last_sample = self.started
        self.goal_reached = None
        self.last_date, self.date_since, self.last_paused = None, None, None
        self.idle_paused = False
        self.idle_paused_game = False
        self.idle_woken = False
        self.pending_wake = None
        self.counts = {"calls": 0, "already_done": 0, "idle_wakes": 0, "idle_pauses": 0, "pause_requests": 0,
                       "front": 0, "executor_stops": 0, "dialogs": 0}
        self.feed_seq = 0
        self.state = "starting"
        self.end_reason = None
        self.stopped = threading.Event()
        self.finished = threading.Event()
        self.start_date = None
        self.planner = None
        self.plan_lock = threading.RLock()
        self._log = open(os.path.join(run, "calls.jsonl"), "a", encoding="utf-8")
        self._events = open(os.path.join(run, "supervisor.log"), "a", encoding="utf-8")

    # ------------------------------------------------------------------------------------------ records
    def event(self, kind, **detail):
        with self.lock:
            # the run may have ended on another thread (hoi4_run stop, the game closing): no record after it
            if self._events.closed:
                return
            self._events.write(json.dumps({"t": round(self.clock(), 3), "event": kind, **detail}) + "\n")
            self._events.flush()

    def status(self, state=None, message=None, **extra):
        if state:
            self.state = state
        data = {"state": self.state, "message": message, "run": self.run, "pid": os.getpid(),
                "updated": self.clock(), "tag": self.request.get("tag"), "executor": self.state == "ready"
                and not self.idle_paused, "idle_paused": self.idle_paused, "idle_woken": self.idle_woken,
                "idle_pause_minutes": self.idle_minutes(),
                "end_reason": self.end_reason, **extra}
        runs.write_json(os.path.join(self.run, "status.json"), data)
        return data

    def note(self, text):
        with self.lock:
            self.notes.append(text)
        self.event("note", text=text)

    def _record_call(self, name, args, out, ms, source):
        shown = {k: v for k, v in (args or {}).items() if k != "lease_token"}
        with self.lock:
            # a call answered while the run ends (hoi4_run stop closes the records) is not recorded; it
            # must not turn its own answer into an error
            if self._log.closed:
                return
            self._log.write(json.dumps({"t": round(self.clock(), 3), "source": source, "tool": name, "args": shown,
                                        "ms": ms, "isError": out["isError"], "code": error_code(out["body"]),
                                        "already_done": bool(isinstance(out["body"], dict)
                                                             and out["body"].get("already_done"))}) + "\n")
            self._log.flush()

    # ------------------------------------------------------------------------------------------ start
    def is_action(self, name, args):
        return name in self.leased or (name in LEASED_WITH and LEASED_WITH[name] in args)

    def native_call(self, name, args):
        args = dict(args)
        if self.is_action(name, args) and self.token:
            args["lease_token"] = self.token
        if name in self.takes_request_id and "request_id" not in args:
            with self.lock:
                self.counter += 1
                args["request_id"] = "s%d-%d" % (int(self.started) % 100000, self.counter)
        return self.native.call(name, args)

    def _stderr_tail(self):
        try:
            with open(os.path.join(self.run, "native.stderr"), encoding="utf-8", errors="replace") as f:
                return f.read()[-4000:]
        except OSError:
            return ""

    def start(self):
        """True when the runtime is bound, the lease held and the executor on; False with status "failed"."""
        deadline = self.clock() + self.campaign_wait
        last = None
        while True:
            self.native = self.start_native()
            try:
                self.native.initialize()
                listed = self.native.tools()
            except Exception:  # noqa: BLE001 - a runtime that exited at start; its reason is in its stderr
                listed = None
            if listed is not None and self.native.alive():
                break
            code = stderr_code(self._stderr_tail()) or "native_start_failed"
            self.native.close()
            step = START_STEPS.get(code, "see native.stderr in the run folder")
            if code != last:
                self.event("waiting", code=code)
                last = code
            if code in FATAL_START or self.clock() >= deadline:
                why = code if code in FATAL_START else "timeout"
                self.end_reason = why
                self.status("failed", "%s: %s" % (code, step), code=code)
                return False
            self.status("waiting", "%s: %s" % (code, step), code=code)
            if code == "input_target_not_frontmost" and self.front:
                # The user is looking at the terminal: bring the game forward for the next try.
                self.front((self.session.get("game") or {}).get("pid"))
            self.sleep(self.retry_every)
        self.tools = listed
        for tool in listed:
            if takes(tool, "lease_token"):
                self.leased.add(tool["name"])
            if takes(tool, "request_id"):
                self.takes_request_id.add(tool["name"])
        err, lease = self.native.call("hoi4_input_lease", {"op": "acquire", "holder": HOLDER, "seconds": LEASE_SECONDS})
        if err or not lease.get("lease_token"):
            self.end_reason = "lease_refused"
            self.status("failed", "the input lease was refused: %s" % json.dumps(lease)[:300])
            self.native.close()
            return False
        self.token = lease["lease_token"]
        self.last_renew = self.clock()
        err, overview = self.native.call("hoi4_overview", {})
        tag = self.request.get("tag")
        if not err and tag and overview.get("player") and overview["player"] != tag:
            self.end_reason = "wrong_country"
            self.status("failed", "the loaded campaign plays %s, the plan is for %s: load %s and run hoi4-live play "
                                  "again" % (overview["player"], tag, tag))
            self.release()
            return False
        resume = self.request.get("resume_date")
        now_date = (overview.get("date") or {}).get("text") if not err and isinstance(overview, dict) else None
        if resume and now_date and report._day_number(now_date) is not None \
                and report._day_number(resume) is not None \
                and report._day_number(now_date) < report._day_number(resume) - RESUME_SLACK_DAYS:  # noqa: SLF001
            self.end_reason = "older_save"
            self.status("failed", "the loaded game is at %s, but the campaign's last segment ended at %s: load the "
                                  "save it wrote (%s) and run hoi4-live play --continue again"
                        % (now_date, resume, self.request.get("resume_save") or "the newest save"))
            self.release()
            return False
        self.reconcile_day_one()
        # The feed from here on is kept (feed.jsonl): 0 gives the current sequence and no changes.
        ferr, feed = self.native.call("hoi4_observe", {"since": 0})
        if not ferr and isinstance(feed, dict):
            self.feed_seq = feed.get("seq") or 0
        # The executor's first input needs the game in front (another window in front refuses every order).
        pid = (self.session.get("game") or {}).get("pid")
        if self.front and pid:
            self.event("front_at_start", ok=bool(self.front(pid)))
        err, body = self.native_call("hoi4_autopilot", {"enabled": True, "interval_ms": 1000})
        if err:
            self.note("the executor could not be switched on (%s); the plan's routine work is yours until it is"
                      % error_code(body))
        self.start_date = (overview.get("date") or {}).get("text") if isinstance(overview, dict) else None
        self.event("ready", date=self.start_date, executor=not err)
        self.note_machine()
        self.last_player = self.clock()
        # The status turns "ready" once the endpoint exists (main), so nobody is told to play before it can.
        self.state = "ready"
        self.start_clock_file()
        return True

    def start_clock_file(self):
        """Publish the game clock to a file while the run lasts, when the run's settings or the environment
        name one (clockfile.py; off by default). Reads only; the file is removed when the run stops."""
        if getattr(self, "clock_file", None) is not None:
            return
        user_dir = (runs.read_json(os.path.join(self.run, "user-files.json")) or {}).get("user_dir")
        self.clock_file = clockfile.ClockPublisher(
            self.run, lambda name, args: self.native.call(name, args), user_dir=user_dir,
            ready=lambda: self.state == "ready" and self.native is not None and self.native.alive())
        self.clock_file.start(self.stopped)

    def reconcile_day_one(self):
        """The plan's names against the live game, once per run, before the executor starts: a name that
        differs only in spelling is patched in plan.json; the rest reaches the player as a note. A failed read
        leaves the plan as written."""
        if not os.path.exists(os.path.join(self.run, "plan.json")) or \
                os.path.exists(os.path.join(self.run, "reconcile.json")):
            return None

        def read(tool, args):
            err, body = self.native.call(tool, args)
            if err:
                raise RuntimeError("%s: %s" % (tool, error_code(body) or json.dumps(body)[:200]))
            return body if isinstance(body, dict) else {}
        try:
            with self.plan_lock:
                rec = reconcile.run(self.run, read)
        except (RuntimeError, ValueError, OSError) as exc:
            self.event("reconcile_failed", error=str(exc)[:300])
            self.note("the plan could not be checked against the live game (%s); it runs as written" % str(exc)[:200])
            return None
        self.event("reconcile", patched=len(rec["patched"]), unresolved=len(rec["unresolved"]),
                   research_not_yet=len(rec["research"]["not_yet_available"]))
        if rec["patched"]:
            self.note("the plan's names were matched to the live game: %s" % "; ".join(
                "%s %s -> %s" % (p["kind"], p["from"], p["to"]) for p in rec["patched"][:8]))
        if rec["unresolved"]:
            self.note("the plan names what the live game does not have, so the executor cannot act on them; what "
                      "the plan asks of them is yours to do by hand (reconcile.json in the run folder): %s" % "; ".join(
                          "%s %r at %s (nearest: %s)" % (u["kind"], u["name"], u["path"],
                                                          ", ".join(map(str, u["candidates"][:3])) or "none")
                          for u in rec["unresolved"][:8]))
        late = rec["research"]["not_yet_available"]
        if late:
            self.note("first research block: %d of %d not researchable yet (%s); the executor takes the next "
                      "available ones" % (len(late), len(rec["research"]["first_block"]), ", ".join(late[:6])))
        return rec

    # ------------------------------------------------------------------------------------------ calls
    def handle(self, name, args, source="player"):
        if self.state != "ready":
            return error("run_not_ready", "the run is %s: %s" % (self.state, self.end_reason or "starting"))
        args = dict(args or {})
        began = self.clock()
        if source == "player":
            self.player_call(name, args, began)
        if name == "hoi4_batch":
            out = self.batch(args)
        elif name == "hoi4_run":
            out = self.run_tool(args)
        elif name in HIDDEN or name not in {t["name"] for t in self.tools}:
            out = error("tool_not_available", "%s is not a tool of this run" % name)
        else:
            out = self.forward(name, args)
        self.counts["calls"] += 1
        if source == "player":
            self.last_player = max(self.last_player, self.clock())      # a long call is not silence
        self._record_call(name, args, out, round((self.clock() - began) * 1000), source)
        with self.lock:
            notes, self.notes = self.notes, []
            wake, self.pending_wake = self.pending_wake, None
        if wake:
            out = self.deliver_wake(out, wake)
        if notes:
            body = out["body"] if isinstance(out["body"], dict) else {"result": out["body"]}
            out = {"isError": out["isError"], "body": {**body, "supervisor_notes": notes}}
        return out

    def signature(self, name, args):
        if not self.is_action(name, args) or name in NOT_DEDUPED:
            return None
        return name + json.dumps({k: v for k, v in args.items() if k not in ("request_id", "lease_token")},
                                 sort_keys=True)

    def forward(self, name, args):
        now = self.clock()
        sig = self.signature(name, args)
        if sig:
            with self.lock:
                prev = self.done.get(sig)
            if prev and now - prev[0] < self.duplicate_window:
                self.counts["already_done"] += 1
                body = prev[1] if isinstance(prev[1], dict) else {"result": prev[1]}
                return {"isError": False, "body": {**body, "already_done": (
                    "You did exactly this %d s ago and it succeeded. A successful action is done: read the state "
                    "to see its effect instead of repeating it; if the game shows it undone, change the target or "
                    "the arguments." % int(now - prev[0]))}}
        if name == "hoi4_plan":
            with self.plan_lock:                     # the planner swaps the plan file under the same lock
                err, body = self.native_call(name, args)
        else:
            err, body = self.native_call(name, args)
        if err and error_code(body) == "input_target_not_frontmost" and self.front and self.is_action(name, args):
            pid = (self.session.get("game") or {}).get("pid")
            if pid and self.front(pid):
                self.counts["front"] += 1
                self.event("front", tool=name)
                err, body = self.native_call(name, args)
        if not err and sig:
            with self.lock:
                self.done[sig] = (now, body)
        if err and error_code(body) in GONE:
            threading.Thread(target=self.stop, args=("game_closed",), daemon=True).start()
        return {"isError": err, "body": body}

    def batch(self, args):
        calls = args.get("calls")
        if not isinstance(calls, list) or not 1 <= len(calls) <= 20:
            return error("invalid_arguments", "calls must list 1 to 20 items, each [tool, {arguments}]")
        results = []
        for n, item in enumerate(calls):
            if not isinstance(item, list) or not item or not isinstance(item[0], str) \
                    or (len(item) > 1 and not isinstance(item[1], dict)):
                results.append({"n": n, "isError": True, "body": {"error": {"code": "invalid_arguments",
                                "message": "each item is [tool, {arguments}]"}}})
                continue
            name = item[0]
            if name in ("hoi4_batch", "hoi4_run") or name in HIDDEN or name not in {t["name"] for t in self.tools}:
                results.append({"n": n, "tool": name, "isError": True,
                                "body": {"error": {"code": "tool_not_available",
                                                   "message": "%s cannot be batched here" % name}}})
                continue
            out = self.forward(name, dict(item[1]) if len(item) > 1 else {})
            body = out["body"] if args.get("full") else shrink(out["body"], 1500)
            results.append({"n": n, "tool": name, "isError": out["isError"], "body": body})
        return {"isError": all(r["isError"] for r in results), "body": {"results": results}}

    def run_tool(self, args):
        op = args.get("op")
        if op == "status":
            return {"isError": False, "body": self.describe()}
        if op == "planner":
            if self.planner is None:
                return error("planner_off", "the rolling planner is not running in this run")
            return {"isError": False, "body": self.planner.summary()}
        if op == "plan_patch":
            if self.planner is None:
                return error("planner_off", "the rolling planner is not running in this run")
            ok, why = self.planner.submit(args.get("update") or 0, args.get("patch"))
            return {"isError": not ok, "body": {"received": ok, "message": why}}
        if op == "stop":
            threading.Thread(target=self.stop, args=("stopped_by_player",), daemon=True).start()
            return {"isError": False, "body": {"stopping": True, "report": os.path.join(self.run, "report.md")}}
        return error("invalid_arguments", "op is status, stop, planner or plan_patch")

    def describe(self):
        err, overview = self.native.call("hoi4_overview", {}) if self.native else (True, {})
        return {"run": self.run, "state": self.state, "tag": self.request.get("tag"),
                "goal": self.request.get("goal"), "date": None if err else (overview.get("date") or {}).get("text"),
                "paused": None if err else overview.get("paused"), "executor_on": not self.idle_paused,
                "idle_paused": self.idle_paused, "idle_pause_minutes": self.idle_minutes(),
                "minutes": round((self.clock() - self.started) / 60, 1),
                **self.counts}

    # ------------------------------------------------------------------------------------------ background
    def idle_minutes(self):
        """The idle pause in minutes, or None when it is switched off."""
        seconds = self.idle_limit()
        return None if seconds is None else round(seconds / 60.0, 2)

    def idle_limit(self):
        """Seconds without a player call before the idle pause, or None when it is switched off. The run's
        settings file is read each time, so the user can change it during a run."""
        if self.idle_pause_override is not None:
            return self.idle_pause_override if self.idle_pause_override > 0 else None
        return runs.idle_pause_seconds(self.run, IDLE_PAUSE_MINUTES)

    def player_call(self, name, args, now):
        """A player call: the idle timer starts again; an idle pause is lifted; an explicit pause is recorded."""
        self.last_player = now
        self.idle_woken = False
        calls = [(name, args)]
        if name == "hoi4_batch" and isinstance(args.get("calls"), list):
            calls = [(c[0], c[1] if len(c) > 1 and isinstance(c[1], dict) else {})
                     for c in args["calls"] if isinstance(c, list) and c]
        asks_pause = any(n == "hoi4_time" and isinstance(a, dict) and a.get("paused") is True for n, a in calls)
        if asks_pause:
            self.counts["pause_requests"] += 1
            self.event("pause_requested", source="player")
        if self.idle_paused:
            self.lift_idle_pause(keep_paused=asks_pause)

    def lift_idle_pause(self, keep_paused=False):
        err, body = self.native_call("hoi4_autopilot", {"enabled": True, "interval_ms": 1000})
        resumed = False
        if self.idle_paused_game and not keep_paused:
            perr, _ = self.native_call("hoi4_time", {"paused": False})
            resumed = not perr
        self.idle_paused = self.idle_paused_game = False
        self.event("idle_resume", executor=not err, game_resumed=resumed)
        self.note("your call lifted the idle pause: the executor is on again"
                  + ("" if not err else " (it refused: %s)" % error_code(body))
                  + (" and the game runs again" if resumed else ""))
        self.status()

    def deliver_wake(self, out, wake):
        """The idle wake as a feed change (wake_player, what the brief already reacts to) on a feed reply."""
        body = out["body"] if isinstance(out["body"], dict) else {"result": out["body"]}
        if isinstance(body.get("changes"), list):
            body = {**body, "changes": body["changes"] + [{"kind": "wake_player", "urgent": True, "detail": wake}]}
        notes = list(body.get("supervisor_notes") or []) + [wake["message"]]
        return {"isError": out["isError"], "body": {**body, "supervisor_notes": notes}}

    def watchdog(self, now):
        """The idle guard (see the module docstring). True when it paused on this tick."""
        limit = self.idle_limit()
        if limit is None or self.idle_paused:
            return False
        idle = now - self.last_player
        if not self.idle_woken and idle >= limit / 2.0:
            self.idle_woken = True
            self.counts["idle_wakes"] += 1
            wake = {"code": "wake_player", "rule": "supervisor", "reason": "idle",
                    "message": "no call from you for %d minutes: the game keeps running; the supervisor pauses it "
                               "after %d minutes without a call" % (idle // 60, round(limit / 60.0)),
                    "evidence": {"idle_seconds": round(idle), "idle_pause_seconds": round(limit)}}
            with self.lock:
                self.pending_wake = wake
            self.event("idle_wake", idle=round(idle), limit=round(limit))
            self.status()
        if idle < limit:
            return False
        self.native_call("hoi4_autopilot", {"enabled": False})
        err, overview = self.native.call("hoi4_overview", {})
        paused = False
        if not err and overview.get("paused") is False:
            perr, _ = self.native_call("hoi4_time", {"paused": True})
            paused = not perr
        date = None if err else (overview.get("date") or {}).get("text")
        self.idle_paused, self.idle_paused_game = True, paused
        self.counts["idle_pauses"] += 1
        reason = "no player call for %d minutes (idle pause at %d minutes)" % (idle // 60, round(limit / 60.0))
        self.event("idle_pause", idle=round(idle), limit=round(limit), game_paused=paused, date=date,
                   at=time.strftime("%Y-%m-%d %H:%M:%S", time.localtime(now)), reason=reason)
        self.note("%s: the supervisor turned the executor off%s; your next call lifts the pause"
                  % (reason, " and paused the game" if paused else ""))
        self.status()
        return True

    def renew(self, now):
        if now - self.last_renew < RENEW_EVERY:
            return
        self.last_renew = now
        err, body = self.native.call("hoi4_input_lease", {"op": "renew", "lease_token": self.token,
                                                          "seconds": LEASE_SECONDS})
        if err:
            err, lease = self.native.call("hoi4_input_lease", {"op": "acquire", "holder": HOLDER,
                                                               "seconds": LEASE_SECONDS})
            if not err and lease.get("lease_token"):
                self.token = lease["lease_token"]
            self.event("lease_reacquired", ok=not err)

    def health(self, now):
        """True when the game and the runtime are still there."""
        if not self.native.alive():
            return False
        if now - self.last_health < HEALTH_EVERY:
            return True
        self.last_health = now
        err, body = self.native.call("hoi4_overview", {})
        if err and error_code(body) in GONE:
            return False
        for change in self.read_feed():
            if change.get("kind") == "goal_reached":
                self.goal_reached = change.get("detail") or {}
                self.event("goal_reached", detail=change.get("detail"))
            if change.get("kind") == "autopilot_stopped" and not self.idle_paused:
                self.counts["executor_stops"] += 1
                self.event("executor_stopped", detail=change.get("detail"))
                self.note("the executor stopped (%s); the plan's routine work waits until it is on again "
                          "(hoi4_autopilot {\"enabled\":true})" % json.dumps(change.get("detail"))[:200])
        return True

    def read_feed(self):
        """The runtime's feed since the last read, every change written to feed.jsonl; the changes. Pages of
        FEED_PAGE follow the last row read (the reply's own seq jumps to the newest)."""
        out = []
        for _ in range(FEED_PAGES):
            err, feed = self.native.call("hoi4_observe", {"since": self.feed_seq, "limit": FEED_PAGE})
            if err or not isinstance(feed, dict):
                break
            rows = [c for c in feed.get("changes") or [] if isinstance(c, dict)]
            out += rows
            if rows and feed.get("more"):
                self.feed_seq = max(c.get("seq") or 0 for c in rows)
                continue
            self.feed_seq = feed.get("seq", self.feed_seq)
            break
        if out:
            with self.lock:
                with open(os.path.join(self.run, "feed.jsonl"), "a", encoding="utf-8") as f:
                    for change in out:
                        f.write(json.dumps(change, ensure_ascii=False) + "\n")
        return out

    def silent_limit(self):
        """Seconds without a player call before the run ends as player_silent, or None when it is off."""
        return runs.idle_pause_seconds(self.run, PLAYER_SILENT_MINUTES, key="player_silent_minutes")

    def silent(self, now):
        limit = self.silent_limit()
        if limit is None or now - self.last_player < limit:
            return False
        minutes = int((now - self.last_player) // 60)
        self.event("player_silent", minutes=minutes, limit=round(limit / 60.0))
        self.note("no call from the player for %d minutes (a usage limit or a closed client): the run ends with "
                  "the game paused and saved; continue it with hoi4-live play --continue" % minutes)
        return True

    def watch_dialogs(self, now):
        """A system prompt on screen: told at once, the run ends when it stays DIALOG_MINUTES. True to end."""
        if self.dialogs is None:
            return False
        try:
            owners = self.dialogs()
        except Exception:  # noqa: BLE001 - an unreadable window list is not a dialog
            owners = []
        if not owners:
            if self.dialog_since is not None:
                self.event("dialog_gone")
                self.note("the system prompt is gone: input works again")
                self.dialog_since = None
                self.status(message="playing")
            return False
        if self.dialog_since is None:
            self.dialog_since = now
            self.counts["dialogs"] += 1
            self.event("dialog", owners=owners)
            message = ("a system prompt is on screen (%s): the game takes no input until someone answers it. Answer "
                       "it yourself (nothing here touches it); the run ends if it is still there in %d minutes"
                       % (", ".join(owners), DIALOG_MINUTES))
            self.note(message)
            self.status(message=message)
            return False
        return now - self.dialog_since >= DIALOG_MINUTES * 60

    def past_until(self, date):
        """True once the game date is at or past the end date the user gave (request.json "until")."""
        day, until = report._day_number(date), report._day_number(self.request.get("until"))  # noqa: SLF001
        return day is not None and until is not None and int(day) >= int(until)

    def tick(self):
        now = self.clock()
        if self.stopped.is_set() or self.state != "ready":
            return
        if not self.health(now):
            self.stop("game_closed")
            return
        if self.goal_reached is not None and self.ends():
            self.stop("goal_reached")
            return
        if self.silent(now):
            self.stop("player_silent")
            return
        if self.watch_dialogs(now):
            self.stop("system_dialog")
            return
        self.renew(now)
        self.watchdog(now)
        if now - self.last_sample >= SAMPLE_EVERY:
            self.last_sample = now
            date = self.sample(now)
            if date and self.past_until(date):
                self.stop("end_date")
                return
            if date and self.ends() and self.past_deadline(date):
                self.stop("deadline")
                return
            limit = self.over_limit(now, date)
            if limit:
                self.event("limit", reason=limit[0], message=limit[1])
                self.note(limit[1])
                self.stop(limit[0])
                return
        if self.planner is not None:
            try:
                self.planner.tick()
            except Exception as exc:  # noqa: BLE001 - the plan in force stays; the next tick tries again
                self.event("planner_failed", error=str(exc)[:300])

    # ------------------------------------------------------------------------------------------ end and watch
    @staticmethod
    def ends():
        return os.environ.get("HOI4_LIVE_END") != "off"

    def past_deadline(self, date):
        """True once the game date (hoi4_overview text, YYYY.MM.DD[.HH]) is past the plan's goal.deadline."""
        plan = runs.read_json(os.path.join(self.run, "plan.json")) or {}
        goal = plan.get("goal") if isinstance(plan.get("goal"), dict) else {}
        day = report._day_number(date)  # noqa: SLF001 - the report's date reader, shared
        deadline = report._day_number(goal.get("deadline"))  # noqa: SLF001
        return day is not None and deadline is not None and int(day) > int(deadline)

    # ------------------------------------------------------------------------------------------ limits
    @staticmethod
    def env_number(name, default):
        try:
            return float(os.environ.get(name, default))
        except ValueError:
            return default

    def game_memory_gb(self):
        """The game's resident memory in GB (ps), None where it cannot be read."""
        pid = (self.session.get("game") or {}).get("pid")
        if not pid or os.name == "nt":
            return None
        try:
            out = subprocess.run(["ps", "-o", "rss=", "-p", str(int(pid))], capture_output=True, text=True,
                                 timeout=10).stdout.strip()
            return int(out.split()[0]) / (1024.0 * 1024.0) if out else None
        except (OSError, ValueError, IndexError, subprocess.SubprocessError):
            return None

    def note_machine(self):
        """Swap and load at the start: a machine short of memory plays at half its pace or less."""
        info = {}
        try:
            info["load"] = [round(x, 2) for x in os.getloadavg()]
        except (AttributeError, OSError):
            pass
        info["cpus"] = os.cpu_count()
        swap = self.swap()
        if swap:
            info["swap_used_gb"], info["swap_total_gb"] = swap
        self.event("machine", **info)
        used, total = swap or (0, 0)
        if total and used / total > 0.5:
            self.note("the machine is short of memory (%.1f of %.1f GB swap in use): the game runs slower and the "
                      "campaign falls behind its plan; close other programs" % (used, total))
        return info

    def over_limit(self, now, date):
        """(reason, message) when the game's memory passed the limit or its date stood still while it ran."""
        limit = self.env_number("HOI4_LIVE_GAME_MEMORY_GB", GAME_MEMORY_GB)
        if limit > 0:
            gb = self.read_game_memory()
            if gb is not None and gb > limit:
                return ("game_memory", "the game uses %.1f GB of memory, over the %.0f GB limit: the run ends here "
                        "as a segment boundary (the game is paused and saved) before the machine stalls; quit the "
                        "game, start it again, load that save and run hoi4-live play --continue" % (gb, limit))
        stall = self.env_number("HOI4_LIVE_STALL_MINUTES", STALL_MINUTES)
        if date is None or self.last_paused is not False:
            self.last_date, self.date_since = date, None
            return None
        if date != self.last_date or self.date_since is None:
            self.last_date, self.date_since = date, now
            return None
        if stall > 0 and now - self.date_since >= stall * 60:
            return ("game_stalled", "the game date has stood at %s for %d minutes while the game runs: the game has "
                    "hung; close it, start it again and run hoi4-live play" % (date, (now - self.date_since) // 60))
        return None

    def sample(self, now):
        """One watch.jsonl line (reads only); the game date, or None when it could not be read."""
        err, overview = self.native.call("hoi4_overview", {})
        if err or not isinstance(overview, dict):
            return None
        date = (overview.get("date") or {}).get("text")
        self.last_paused = overview.get("paused")
        line = {"t": round(now, 3), "date": date, "paused": overview.get("paused")}
        err, stats = self.native.call("hoi4_war_stats", {})
        if not err and isinstance(stats, dict):
            line["war"] = shrink(stats, 1500)
        err, supply = self.native.call("hoi4_supply", {"summary": True})
        if not err and isinstance(supply, dict):
            line["armies"] = [{"army": a.get("army"), "supply": a.get("supply_percent_mean"),
                               "org": a.get("organisation_mean"), "divisions": a.get("divisions")}
                              for a in (supply.get("armies") or []) if isinstance(a, dict)][:40]
        if date and (self.last_production is None or now - self.last_production >= PRODUCTION_EVERY):
            self.last_production = now
            prod = self.production(date)
            if prod:
                line["prod"] = prod
        with self.lock:
            with open(os.path.join(self.run, "watch.jsonl"), "a", encoding="utf-8") as f:
                f.write(json.dumps(line) + "\n")
        return date

    def production(self, date):
        """Production lines, stockpile and economy in one record (reads only), the shape `hoi4-live report`
        reads: lines [index, kind, equipment, requested, active, efficiency], stock [archetype, stockpile,
        fielded need, fielded short, factories, made per day]."""
        lerr, lines = self.native.call("hoi4_production_lines", {"limit": 128})
        serr, stock = self.native.call("hoi4_stockpile", {})
        eerr, economy = self.native.call("hoi4_economy", {})
        if lerr and serr and eerr:
            return None
        lines = lines if not lerr and isinstance(lines, dict) else {}
        stock = stock if not serr and isinstance(stock, dict) else {}
        economy = economy if not eerr and isinstance(economy, dict) else {}
        counters = lambda line: line.get("factory_counters") or {}  # noqa: E731
        return {"date": str(date)[:10], "mil": (economy.get("factory_trigger_count") or {}).get("military"),
                "pp": ((economy.get("political_power") or {}).get("raw_signed") or 0) / 1e5,
                "lines": [[x.get("entry_index"), x.get("line_kind"), x.get("equipment_id"),
                           counters(x).get("requested_factories_raw_signed"),
                           counters(x).get("active_factories_raw_signed"), (x.get("output") or {}).get("efficiency")]
                          for x in lines.get("lines") or [] if isinstance(x, dict)],
                "stock": [[x.get("archetype"), x.get("stockpile"), x.get("fielded_need"), x.get("fielded_short"),
                           x.get("factories"), x.get("produced_per_day")]
                          for x in stock.get("equipment") or [] if isinstance(x, dict)]}

    def snapshot(self):
        """What the gate reads, as {tool: body} with the game date; None without a runtime. Reads only."""
        if not (self.native and self.native.alive()):
            return None
        out = {"t": round(self.clock(), 3), "tag": self.request.get("tag"), "reads": {}}
        for tool, args in GATE_READS:
            err, body = self.native.call(tool, dict(args))
            out["reads"][tool] = {"error": error_code(body) or True} if err else body
        now = (out["reads"].get("hoi4_observe") or {}).get("now") or {}
        out["date"] = now.get("date") if isinstance(now, dict) else None
        return out

    def keep_snapshot(self, snap):
        """gate/<date>.json in the run folder; the path, or None."""
        if not snap:
            return None
        folder = runs.private_dir(os.path.join(self.run, "gate"))
        name = re.sub(r"[^0-9.]", "", str(snap.get("date") or "")) or "%d" % int(snap["t"])
        path = os.path.join(folder, name + ".json")
        runs.write_json(path, snap)
        return path

    def end_save(self, reason):
        """At the end of a segment: the game paused (the supervisor's own ends) and saved with its own save.
        {"saved": name} or {"error": ...}; None when no save is tried."""
        if reason in NO_SAVE or os.environ.get("HOI4_LIVE_SAVE_AT_END") == "off" or not self.token \
                or not (self.native and self.native.alive()) or "hoi4_save" not in {t["name"] for t in self.tools}:
            return None
        self.native_call("hoi4_autopilot", {"enabled": False})
        if reason in PAUSE_AT_END:
            err, overview = self.native.call("hoi4_overview", {})
            if not err and isinstance(overview, dict) and overview.get("paused") is False:
                self.native_call("hoi4_time", {"paused": True})
        err, body = self.native_call("hoi4_save", {})
        if err or not isinstance(body, dict) or not body.get("saved"):
            out = {"error": error_code(body) or "not_saved",
                   "message": str((body.get("error") or {}).get("message") if isinstance(body, dict)
                                  and isinstance(body.get("error"), dict) else body)[:300]}
        else:
            out = {"saved": body["saved"]}
        self.event("end_save", **out)
        return out

    # ------------------------------------------------------------------------------------------ planner
    def start_planner(self, load_facts=True):
        """The rolling planner (docs/ROLLING_PLAN.md) beside the executor; HOI4_LIVE_PLANNER=off leaves it out."""
        if os.environ.get("HOI4_LIVE_PLANNER") == "off" or not os.path.exists(os.path.join(self.run, "plan.json")):
            return None
        from ..planner import facts as pf, gates as pg, worker as pw
        tools_dir = pf.tools_dir()

        def ask_agent(folder):
            self.note("a plan update is waiting (%s): read %s and answer within 2 minutes with hoi4_run "
                      "{\"op\":\"plan_patch\",\"update\":%s,\"patch\":{...}}; without an answer the default "
                      "pick is applied" % (os.path.basename(folder), os.path.join(folder, "prompt.md"),
                                           os.path.basename(folder).split("-")[-1]))
        self.planner = pw.PlannerWorker(self.run, lambda name, args: self.native.call(name, args),
                                        lock=self.plan_lock, note=self.note,
                                        proposer=pw.proposer_for(tools_dir=tools_dir, notify=ask_agent))

        def bind():
            facts = pf.load()
            if facts is not None:
                tools = pg.ToolGates(tools_dir, pf.game_dir()) if tools_dir else None
                self.planner.attach(facts, tools)
            self.event("planner_ready", facts=facts is not None, tools=bool(tools_dir))
        if load_facts:
            threading.Thread(target=bind, daemon=True).start()
        return self.planner

    def loop(self):
        while not self.stopped.wait(TICK):
            try:
                self.tick()
            except Exception as exc:  # noqa: BLE001 - one failed tick is logged; the next one runs
                self.event("tick_failed", error=str(exc)[:300])

    # ------------------------------------------------------------------------------------------ stop
    def close_records(self):
        with self.lock:
            for record in (self._log, self._events):
                if not record.closed:
                    record.close()

    def release(self):
        if self.native and self.native.alive() and self.token:
            self.native_call("hoi4_autopilot", {"enabled": False})
            self.native.call("hoi4_input_lease", {"op": "release", "lease_token": self.token})
        if self.native:
            self.native.close()

    def stop(self, reason):
        with self.lock:
            if self.stopped.is_set():
                return
            self.stopped.set()
        self.end_reason = reason
        final = {}
        if self.native and self.native.alive():
            err, overview = self.native.call("hoi4_overview", {})
            if not err:
                final["date"] = (overview.get("date") or {}).get("text")
            err, stats = self.native.call("hoi4_war_stats", {})
            if not err:
                final["war_stats"] = stats
            # VP held against what the plan needs to capitulate each enemy (hoi4_states with a country tag)
            plan = runs.read_json(os.path.join(self.run, "plan.json")) or {}
            rows = []
            for tag, needed in report.capitulation_aims(plan):
                err, states = self.native.call("hoi4_states", {"query": tag})
                vp = (states or {}).get("victory_points") if not err and isinstance(states, dict) else None
                if vp:
                    rows.append({"tag": tag, "held": vp.get("held_by_others"), "total": vp.get("total"),
                                 "needed": needed})
            if rows:
                final["capitulation"] = rows
            try:
                final["gate"] = self.keep_snapshot(self.snapshot())
            except Exception as exc:  # noqa: BLE001 - the report is written without the gate state
                self.event("snapshot_failed", error=str(exc)[:300])
            self.saved = self.end_save(reason)
            if self.saved:
                final["save"] = self.saved
        if self.planner is not None:
            self.planner.write_summary()
        self.release()
        self.event("stopped", reason=reason)
        self.state = "ended"
        path = report.write(self.run, reason, self.counts, final, self.started, self.clock())
        self.status("ended", "run ended: %s; report: %s" % (reason, path), report=path)
        runs.clear_active(self.prefix, self.run)
        # what a continued segment starts from (`hoi4-live play --continue`)
        runs.write_json(os.path.join(self.run, "end.json"),
                        {"reason": reason, "date": final.get("date"), "saved": (self.saved or {}).get("saved"),
                         "save_error": (self.saved or {}).get("error"), "gate": final.get("gate")})
        runs.write_json(os.path.join(runs.runs_dir(self.prefix), "last.json"),
                        {"run": self.run, "reason": reason, "report": path, "date": final.get("date"),
                         "saved": (self.saved or {}).get("saved"), "tag": self.request.get("tag")})
        self.close_records()
        self.finished.set()


# ---------------------------------------------------------------------------------------------------------------
# endpoint


def machine_swap():
    """(used GB, total GB) of swap, None where it cannot be read (macOS sysctl, Linux /proc/meminfo)."""
    try:
        if sys.platform == "darwin":
            out = subprocess.run(["sysctl", "-n", "vm.swapusage"], capture_output=True, text=True,
                                 timeout=10).stdout
            total = re.search(r"total = ([\d.]+)M", out)
            used = re.search(r"used = ([\d.]+)M", out)
            if total and used:
                return float(used.group(1)) / 1024.0, float(total.group(1)) / 1024.0
        elif os.path.exists("/proc/meminfo"):
            with open("/proc/meminfo", encoding="utf-8") as f:
                kb = {k: int(v.split()[0]) for k, v in (line.split(":", 1) for line in f if ":" in line)
                      if v.split() and v.split()[0].isdigit()}
            if kb.get("SwapTotal"):
                return (kb["SwapTotal"] - kb.get("SwapFree", 0)) / 1048576.0, kb["SwapTotal"] / 1048576.0
    except (OSError, ValueError, subprocess.SubprocessError):
        return None
    return None


def endpoint_address(system=None):
    system = system or game.system_name()
    if system == "windows":
        return r"\\.\pipe\hoi4live-" + secrets.token_hex(8), "AF_PIPE"
    folder = tempfile.mkdtemp(prefix="hoi4live-")
    return os.path.join(folder, "play.sock"), "AF_UNIX"


def serve(supervisor, address, family, authkey):
    from multiprocessing.connection import Listener
    listener = Listener(address, family=family, authkey=authkey)

    def client(conn):
        with conn:
            try:
                while True:
                    message = conn.recv()
                    op = message.get("op")
                    if op == "tools":
                        conn.send({"tools": player_tools(supervisor.tools), "state": supervisor.state})
                    elif op == "call":
                        conn.send(supervisor.handle(message.get("name"), message.get("args")))
                    elif op == "stop":
                        threading.Thread(target=supervisor.stop, args=("stopped_by_user",), daemon=True).start()
                        conn.send({"stopping": True})
                    elif op == "snapshot":
                        # `hoi4-live report` on the running campaign: the gate's reads, not a player call
                        conn.send({"snapshot": supervisor.snapshot() if supervisor.state == "ready" else None})
                    else:
                        conn.send(error("invalid_request", "op is tools, call, stop or snapshot"))
            except (EOFError, OSError):
                return

    def accept():
        from multiprocessing import AuthenticationError
        while not supervisor.stopped.is_set():
            try:
                conn = listener.accept()
            except (AuthenticationError, EOFError):
                continue                                  # a caller without the key, or one that hung up
            except OSError:
                return                                    # the listener was closed
            threading.Thread(target=client, args=(conn,), daemon=True).start()

    threading.Thread(target=accept, daemon=True).start()
    return listener


def main(argv=None):
    parser = argparse.ArgumentParser(prog="hoi4-live-play supervisor", description=__doc__.split("\n\n")[0])
    parser.add_argument("--prefix", required=True)
    parser.add_argument("--run", required=True)
    parser.add_argument("--native", required=True)
    # Seconds, for live checks only; a run's own setting is idle_pause_minutes in its settings.json.
    override = os.environ.get("HOI4_LIVE_IDLE_PAUSE")
    parser.add_argument("--idle-pause", type=int, default=int(override) if override else None)
    args = parser.parse_args(argv)
    run = os.path.abspath(args.run)
    native_args = [args.native, "--session", os.path.join(run, "session.json"), "--mac-input", run]
    supervisor = Supervisor(run, os.path.abspath(args.prefix),
                            lambda: Native(native_args, os.path.join(run, "native.stderr")),
                            front=game.bring_to_front if game.system_name() == "macos" else None,
                            idle_pause=args.idle_pause,
                            dialogs=game.system_dialogs if game.system_name() == "macos" else None)
    supervisor.status("starting", "starting the native runtime")
    if not supervisor.start():
        return 1
    supervisor.start_planner()
    address, family = endpoint_address()
    authkey = secrets.token_bytes(32)
    listener = serve(supervisor, address, family, authkey)
    endpoint = {"address": address, "family": family, "authkey": authkey.hex()}
    runs.write_json(os.path.join(run, "endpoint.json"), endpoint)
    runs.set_active(supervisor.prefix, run, endpoint, os.getpid())
    supervisor.status("ready", "playing", date=supervisor.start_date)
    try:
        import signal
        signal.signal(signal.SIGTERM, lambda *_: threading.Thread(target=supervisor.stop, args=("stopped_by_user",),
                                                                  daemon=True).start())
    except (ValueError, OSError, AttributeError):
        pass
    try:
        supervisor.loop()
    finally:
        supervisor.stop(supervisor.end_reason or "supervisor_exit")
        supervisor.finished.wait(120)
        listener.close()
        if family == "AF_UNIX":
            for path in (address, os.path.dirname(address)):
                try:
                    (os.rmdir if os.path.isdir(path) else os.remove)(path)
                except OSError:
                    pass
    return 0


if __name__ == "__main__":
    sys.exit(main())
