"""hoi4-live play on the native runtime: run folder, game discovery, supervisor, proxy, start path, report.

No game, no input, no native runtime: the runtime is a fake (or a tiny Python child speaking JSON lines), the
clock is a counter and the process list is text.
"""

import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import threading
import unittest

import helpers

from hoi4_live.play import game, native, proxy, report, runs, start, supervisor

TEMPLATE = os.path.join(helpers.ROOT, "skills", "hoi4-campaign-playbook", "briefs", "player.txt")
PLAN_JSON = os.path.join(helpers.ROOT, "skills", "hoi4-campaign-playbook", "plans", "italy-ethiopia-1936.plan.json")
ITALY = {"id": "italy-ethiopia-1936", "tag": "ITA", "name": "Italy", "mode": "fair", "start": "1936.1.1",
         "stop": "1937.1.1", "path": PLAN_JSON, "guide": PLAN_JSON[:-len(".plan.json")] + ".md"}

TOOLS = [
    {"name": "hoi4_overview", "inputSchema": {"type": "object", "properties": {}}},
    {"name": "hoi4_observe", "inputSchema": {"type": "object", "properties": {"since": {}, "kinds": {}}}},
    {"name": "hoi4_war_stats", "inputSchema": {"type": "object", "properties": {}}},
    {"name": "hoi4_input_lease", "inputSchema": {"type": "object", "properties": {"op": {}, "lease_token": {}}}},
    {"name": "hoi4_plan_run", "inputSchema": {"type": "object", "properties": {"lease_token": {}}}},
    {"name": "hoi4_focus_available", "inputSchema": {"type": "object", "properties": {"focuses": {}}}},
    {"name": "hoi4_research_start", "inputSchema": {"type": "object", "properties": {
        "technology": {}, "request_id": {"type": "string"}, "lease_token": {"type": "string"}},
        "required": ["technology", "request_id", "lease_token"]}},
    {"name": "hoi4_time", "inputSchema": {"type": "object", "properties": {
        "paused": {}, "speed": {}, "request_id": {}, "lease_token": {}}, "required": ["lease_token"]}},
    {"name": "hoi4_autopilot", "inputSchema": {"type": "object", "properties": {
        "enabled": {}, "interval_ms": {}, "request_id": {}, "lease_token": {}}, "required": ["lease_token"]}},
    {"name": "hoi4_daily_governor", "inputSchema": {"type": "object", "oneOf": [
        {"properties": {"lease_token": {}, "op": {"enum": ["start"]}, "plan": {}}, "required": ["op", "lease_token"]},
        {"properties": {"op": {"enum": ["digest"]}}, "required": ["op"]}]}},
    {"name": "hoi4_diplomacy_incoming", "inputSchema": {"type": "object", "properties": {
        "answer": {}, "index": {}, "request_id": {}}}},
]


IDLE_LIMIT = supervisor.IDLE_PAUSE_MINUTES * 60


def template():
    with open(TEMPLATE, encoding="utf-8") as f:
        return f.read()


class Clock:
    def __init__(self, now=1000.0):
        self.now = now

    def __call__(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds


class FakeNative:
    """The native runtime: answers from handlers; records every call with its arguments."""

    def __init__(self, paused=True, player="ITA"):
        self.calls = []
        self.running = True
        self.paused = paused
        self.player = player
        self.handlers = {}

    def initialize(self):
        return {}

    def tools(self):
        return TOOLS

    def alive(self):
        return self.running

    def call(self, name, args):
        self.calls.append((name, dict(args)))
        if name in self.handlers:
            return self.handlers[name](args)
        if name == "hoi4_input_lease":
            return (False, {"lease_token": "secret-token"}) if args.get("op") == "acquire" else (False, {"ok": True})
        if name == "hoi4_overview":
            return False, {"player": self.player, "paused": self.paused, "date": {"text": "1936.01.01.12"}}
        if name == "hoi4_time":
            self.paused = args.get("paused", self.paused)
            return False, {"paused": self.paused}
        if name == "hoi4_observe":
            return False, {"seq": 1, "changes": []}
        return False, {"ok": True, "tool": name}

    def close(self):
        self.running = False
        return 0

    def names(self):
        return [name for name, _args in self.calls]


class FailingNative(FakeNative):
    def initialize(self):
        raise native.NativeError("process_exited", "exited")


class Folder(unittest.TestCase):
    def setUp(self):
        self.prefix = tempfile.mkdtemp(prefix="hoi4live_playflow_")
        self.addCleanup(shutil.rmtree, self.prefix, True)
        self.user_dir = os.path.join(self.prefix, "user")
        os.makedirs(os.path.join(self.user_dir, "save games"))
        for name in runs.USER_FILES:
            with open(os.path.join(self.user_dir, name), "w", encoding="utf-8") as f:
                f.write(name)

    def make_run(self):
        run = start.prepare(self.prefix, ITALY, "conquer Ethiopia", template())
        game_identity = {"pid": 4242, "start_ticks": "1.0", "exe": "/games/hoi4", "uid": 501}
        runs.write_json(os.path.join(run, "session.json"), {"game": game_identity, "attached": True})
        return run


# ---------------------------------------------------------------------------------------------------------------


class RunFolderTests(Folder):
    def test_the_run_folder_holds_the_plan_switched_on_the_binding_and_the_brief(self):
        run = self.make_run()
        with open(os.path.join(run, "plan.json"), encoding="utf-8") as f:
            self.assertTrue(json.load(f)["active"])
        self.assertEqual(runs.read_json(os.path.join(run, "input.json")),
                         {"controller": "hoi4-live-play", "player": "ITA"})
        self.assertEqual(runs.read_json(os.path.join(run, "request.json"))["runtime"], "native")
        with open(os.path.join(run, "BRIEF.txt"), encoding="utf-8") as f:
            brief = f.read()
        self.assertTrue(brief.startswith(runs.HOW_TO_CALL))
        for key in runs.PLACEHOLDERS:
            self.assertNotIn("{%s}" % key, brief)
        self.assertIn("    hoi4_observe '{\"since\":0}'", brief)     # the tool lines read as MCP calls
        self.assertIn("hoi4_batch '[[", brief)
        self.assertIn(ITALY["guide"], brief)                       # no brief.md: the plan notes are named
        self.assertIn("1937.1.1", brief)
        with open(os.path.join(run, "prompt.txt"), encoding="utf-8") as f:
            self.assertEqual(f.read().strip(), runs.instruction(run))
        with open(os.path.join(run, "plan.brief.md"), encoding="utf-8") as f:
            self.assertEqual(f.read().strip(), runs.plan_brief_text(ITALY))
        self.assertTrue(os.path.isfile(os.path.join(run, "plan.md")))
        if os.name != "nt":
            self.assertEqual(os.stat(run).st_mode & 0o777, 0o700)
            self.assertEqual(os.stat(os.path.join(run, "BRIEF.txt")).st_mode & 0o777, 0o600)

    def test_a_plan_brief_beside_the_plan_is_used_word_for_word(self):
        brief = os.path.join(self.prefix, "x.brief.md")
        with open(brief, "w", encoding="utf-8") as f:
            f.write("ONLY THESE ITEMS\n")
        self.assertEqual(runs.plan_brief_text(dict(ITALY, brief=brief)), "ONLY THESE ITEMS")
        self.assertIn("no player notes", runs.plan_brief_text(dict(ITALY, guide=None, path="/x/plan.json")))

    def test_the_active_pointer_names_one_run_and_is_cleared_only_by_it(self):
        run = self.make_run()
        self.assertIsNone(runs.active_run(self.prefix))
        runs.set_active(self.prefix, run, {"address": "a"}, 99)
        self.assertEqual(runs.active_run(self.prefix)["run"], run)
        runs.clear_active(self.prefix, os.path.join(self.prefix, "runs", "another"))
        self.assertIsNotNone(runs.active_run(self.prefix))
        runs.clear_active(self.prefix, run)
        self.assertIsNone(runs.active_run(self.prefix))

    def test_user_files_are_recorded_copied_and_compared_never_written(self):
        run = self.make_run()
        runs.record_user_files(self.user_dir, run)
        self.assertEqual(runs.compare_user_files(run), [])
        with open(os.path.join(self.user_dir, "pdx_settings.txt"), "w", encoding="utf-8") as f:
            f.write("changed in the game")
        open(os.path.join(self.user_dir, "save games", "autosave.hoi4"), "w").close()
        changed = dict(runs.compare_user_files(run))
        self.assertIn("pdx_settings.txt", changed)
        self.assertIn(os.path.join(run, "user-files", "pdx_settings.txt"), changed["pdx_settings.txt"])
        self.assertIn("autosave.hoi4", changed["save games"])
        with open(os.path.join(run, "user-files", "pdx_settings.txt"), encoding="utf-8") as f:
            self.assertEqual(f.read(), "pdx_settings.txt")       # the copy from the start


class GameTests(unittest.TestCase):
    def test_the_game_is_found_by_its_exact_executable_for_this_user(self):
        exe = "/Users/u/Library/Application Support/Steam/steamapps/common/Hearts of Iron IV/hoi4.app/Contents/MacOS/hoi4"
        text = ("    1     0 /sbin/launchd\n  812   501 %s\n  900   502 %s\n  901   501 %s.bak\n  77   501 /bin/zsh\n"
                % (exe, exe, exe))
        rows = game.parse_ps(text)
        self.assertIn((812, 501, exe), rows)
        self.assertEqual(game.find_games(exe, "macos", rows, uid=501), [812])
        self.assertEqual(game.find_games(exe, "macos", rows, uid=503), [])

    def test_windows_matches_the_image_name(self):
        rows = game.parse_tasklist('"hoi4.exe","5120","Console","1","1,200,000 K"\n"x.exe","1","Console","1","1 K"\n')
        self.assertEqual(game.find_games(r"C:\Steam\common\Hearts of Iron IV\hoi4.exe", "windows", rows), [5120])

    def test_steam_is_asked_with_its_own_url_only(self):
        seen = []
        args = game.launch_through_steam("macos", opener=lambda a, **_k: seen.append(a))
        self.assertEqual(seen, [["open", "steam://rungameid/394360"]])
        self.assertEqual(args, ["open", "steam://rungameid/394360"])
        game.launch_through_steam("linux", opener=lambda a, **_k: seen.append(a))
        self.assertEqual(seen[-1], ["xdg-open", "steam://rungameid/394360"])

    def test_display_steps_name_each_setting_that_differs(self):
        good = {"mode": "windowed", "resolution": (1920, 1080), "gui_scale": 1.0}
        self.assertEqual(game.display_steps(good, (1920, 1200), "macos"), [])
        steps = game.display_steps({"mode": "fullscreen", "resolution": (2560, 1440), "gui_scale": 1.5}, None,
                                   "macos")
        self.assertEqual(len(steps), 3)
        self.assertIn("Windowed", steps[0][1])
        small = game.display_steps(good, (1710, 1107), "macos")
        self.assertIn("More Space", small[0][1])
        self.assertEqual(game.display_steps(good, (1710, 1107), "linux"), [])
        self.assertEqual(game.display_steps({}, None, "macos"), [])


# ---------------------------------------------------------------------------------------------------------------


class SupervisorTests(Folder):
    def supervisor(self, fake=None, clock=None, front=None, **options):
        self.run_dir = self.make_run()
        runs.record_user_files(self.user_dir, self.run_dir)
        self.fake = fake or FakeNative()
        self.clock = clock or Clock()
        s = supervisor.Supervisor(self.run_dir, self.prefix, lambda: self.fake, clock=self.clock,
                                  sleep=self.clock.sleep, front=front, **options)
        self.addCleanup(s.close_records)
        return s

    def ready(self, **options):
        s = self.supervisor(**options)
        self.assertTrue(s.start())
        return s

    def test_start_takes_the_lease_checks_the_country_and_switches_the_executor_on(self):
        s = self.ready()
        self.assertEqual(s.token, "secret-token")
        self.assertEqual(s.state, "ready")
        self.assertEqual(s.start_date, "1936.01.01.12")
        name, args = self.fake.calls[-1]
        self.assertEqual(name, "hoi4_autopilot")
        self.assertEqual((args["enabled"], args["lease_token"]), (True, "secret-token"))
        self.assertIn("request_id", args)
        self.assertEqual(s.leased, {"hoi4_input_lease", "hoi4_plan_run", "hoi4_research_start", "hoi4_time",
                                    "hoi4_autopilot", "hoi4_daily_governor"})

    def test_the_rolling_planner_runs_beside_the_executor(self):
        s = self.ready()
        if not os.path.exists(os.path.join(self.run_dir, "plan.json")):
            supervisor.runs.write_json(os.path.join(self.run_dir, "plan.json"), {"research_path": []})
        self.assertIsNotNone(s.start_planner(load_facts=False))
        s.tick()
        self.assertTrue(os.path.isdir(os.path.join(self.run_dir, "planner")))
        out = s.run_tool({"op": "planner"})
        self.assertFalse(out["isError"])
        self.assertIn("budget", out["body"])
        self.assertTrue(s.run_tool({"op": "plan_patch", "update": 99, "patch": {"ops": []}})["isError"])
        leased = [n for n, a in self.fake.calls if "lease_token" in a and n.startswith(("hoi4_overview", "hoi4_states"))]
        self.assertEqual(leased, [])

    def test_start_waits_while_the_user_is_in_the_menus_then_binds(self):
        attempts = []

        def factory():
            attempts.append(1)
            if len(attempts) < 3:
                with open(os.path.join(self.run_dir, "native.stderr"), "a", encoding="utf-8") as f:
                    f.write('Error: Error { code: "campaign_unavailable", message: "Current in-game context" }\n')
                return FailingNative()
            return self.fake

        s = self.supervisor()
        s.start_native = factory
        self.assertTrue(s.start())
        self.assertEqual(len(attempts), 3)
        with open(os.path.join(self.run_dir, "supervisor.log"), encoding="utf-8") as f:
            events = [json.loads(line)["event"] for line in f]
        self.assertEqual(events.count("waiting"), 1)              # one line per new reason, not per attempt

    def test_a_game_behind_the_terminal_is_brought_forward_while_starting(self):
        fronted = []
        attempts = []

        def factory():
            attempts.append(1)
            if len(attempts) == 1:
                with open(os.path.join(self.run_dir, "native.stderr"), "a", encoding="utf-8") as f:
                    f.write("Error: Error { code: \"input_target_not_frontmost\", message: \"x\" }\n")
                return FailingNative()
            return self.fake

        s = self.supervisor(front=lambda pid: fronted.append(pid) or True)
        s.start_native = factory
        self.assertTrue(s.start())
        self.assertEqual(fronted, [4242])

    def test_a_reason_only_the_user_can_fix_ends_the_start_with_its_step(self):
        s = self.supervisor()
        with open(os.path.join(self.run_dir, "native.stderr"), "w", encoding="utf-8") as f:
            f.write("task_access_denied: the task port was refused\n")
        s.start_native = FailingNative
        self.assertFalse(s.start())
        status = runs.read_json(os.path.join(self.run_dir, "status.json"))
        self.assertEqual(status["state"], "failed")
        self.assertIn("hoi4-live doctor", status["message"])

    def test_waiting_ends_after_the_campaign_wait(self):
        s = self.supervisor(campaign_wait=30, retry_every=5)
        s.start_native = FailingNative
        self.assertFalse(s.start())
        self.assertEqual(s.end_reason, "timeout")

    def test_another_country_is_refused_and_the_lease_released(self):
        s = self.supervisor(fake=FakeNative(player="ETH"))
        self.assertFalse(s.start())
        status = runs.read_json(os.path.join(self.run_dir, "status.json"))
        self.assertIn("plays ETH", status["message"])
        self.assertIn(("hoi4_input_lease", {"op": "release", "lease_token": "secret-token"}), self.fake.calls)

    def test_calls_get_the_token_and_a_request_id_and_the_log_has_neither_token(self):
        s = self.ready()
        out = s.handle("hoi4_research_start", {"technology": "radio"})
        self.assertFalse(out["isError"])
        name, args = self.fake.calls[-1]
        self.assertEqual(args["lease_token"], "secret-token")
        self.assertTrue(args["request_id"])
        s.handle("hoi4_focus_available", {"focuses": ["a"]})
        self.assertEqual(self.fake.calls[-1][1], {"focuses": ["a"]})   # a strict read gets nothing added
        with open(os.path.join(self.run_dir, "calls.jsonl"), encoding="utf-8") as f:
            text = f.read()
        self.assertNotIn("secret-token", text)
        self.assertIn("hoi4_research_start", text)

    def test_a_lease_inside_one_alternative_of_a_schema_and_an_answer_to_a_proposal_get_the_token(self):
        s = self.ready()
        s.handle("hoi4_daily_governor", {"op": "start", "plan": {}})
        self.assertEqual(self.fake.calls[-1][1]["lease_token"], "secret-token")
        s.handle("hoi4_diplomacy_incoming", {})
        self.assertNotIn("lease_token", self.fake.calls[-1][1])          # without an answer it is a read
        s.handle("hoi4_diplomacy_incoming", {"answer": "refuse"})
        self.assertEqual(self.fake.calls[-1][1]["lease_token"], "secret-token")
        again = s.handle("hoi4_diplomacy_incoming", {"answer": "refuse"})
        self.assertIn("already_done", again["body"])
        shown = {t["name"]: t for t in supervisor.player_tools(TOOLS)}["hoi4_daily_governor"]["inputSchema"]
        self.assertNotIn("lease_token", json.dumps(shown))
        self.assertEqual(shown["oneOf"][0]["required"], ["op"])

    def test_an_identical_success_is_answered_from_memory_within_the_window(self):
        s = self.ready()
        first = s.handle("hoi4_research_start", {"technology": "radio"})
        sent = self.fake.names().count("hoi4_research_start")
        again = s.handle("hoi4_research_start", {"technology": "radio"})
        self.assertIn("already_done", again["body"])
        self.assertEqual(again["body"]["tool"], first["body"]["tool"])
        self.assertEqual(self.fake.names().count("hoi4_research_start"), sent)
        self.clock.now += supervisor.DUPLICATE_WINDOW + 1
        s.handle("hoi4_research_start", {"technology": "radio"})
        self.assertEqual(self.fake.names().count("hoi4_research_start"), sent + 1)
        # Clock changes always go through; reads are never held back.
        s.handle("hoi4_time", {"paused": False, "speed": 3})
        s.handle("hoi4_time", {"paused": False, "speed": 3})
        self.assertEqual(self.fake.names().count("hoi4_time"), 2)

    def test_a_failed_action_is_not_remembered(self):
        s = self.ready()
        self.fake.handlers["hoi4_research_start"] = lambda a: (True, {"error": {"code": "not_available"}})
        s.handle("hoi4_research_start", {"technology": "radio"})
        s.handle("hoi4_research_start", {"technology": "radio"})
        self.assertEqual(self.fake.names().count("hoi4_research_start"), 2)

    def test_the_supervisors_own_tools_are_not_the_players(self):
        s = self.ready()
        for name in ("hoi4_input_lease", "hoi4_plan_run", "hoi4_console"):
            self.assertEqual(s.handle(name, {})["body"]["error"]["code"], "tool_not_available")
        listed = {t["name"]: t for t in supervisor.player_tools(TOOLS)}
        self.assertNotIn("hoi4_input_lease", listed)
        self.assertIn("hoi4_batch", listed)
        schema = listed["hoi4_research_start"]["inputSchema"]
        self.assertNotIn("lease_token", schema["properties"])
        self.assertEqual(schema["required"], ["technology"])
        self.assertIn("lease_token", TOOLS[6]["inputSchema"]["properties"])   # the original is untouched

    def test_a_batch_runs_every_call_in_order(self):
        s = self.ready()
        out = s.handle("hoi4_batch", {"calls": [["hoi4_overview", {}], ["hoi4_research_start", {"technology": "x"}],
                                                ["hoi4_input_lease", {}], "bad"]})
        codes = [r["isError"] for r in out["body"]["results"]]
        self.assertEqual(codes, [False, False, True, True])
        self.assertFalse(out["isError"])
        self.assertIn("lease_token", self.fake.calls[-1][1])
        self.assertTrue(s.handle("hoi4_batch", {"calls": []})["isError"])

    def test_input_behind_another_window_brings_the_game_forward_once(self):
        fronted = []
        s = self.ready(front=lambda pid: fronted.append(pid) or True)
        replies = iter([(True, {"error": {"code": "input_target_not_frontmost"}}), (False, {"ok": True})])
        self.fake.handlers["hoi4_research_start"] = lambda a: next(replies)
        out = s.handle("hoi4_research_start", {"technology": "radio"})
        self.assertFalse(out["isError"])
        self.assertEqual(fronted, [4242])
        self.assertEqual(s.counts["front"], 1)

    def test_the_idle_pause_defaults_to_fifteen_minutes_from_the_runs_settings(self):
        s = self.ready()
        self.assertEqual(supervisor.IDLE_PAUSE_MINUTES, 15)
        self.assertEqual(runs.read_json(os.path.join(self.run_dir, "settings.json")), {"idle_pause_minutes": 15})
        self.assertEqual(s.idle_limit(), 900)

    def test_a_silence_shorter_than_half_the_threshold_does_nothing(self):
        s = self.ready()
        self.fake.paused = False
        self.clock.now += IDLE_LIMIT / 2 - 1
        before = len(self.fake.calls)
        self.assertFalse(s.watchdog(self.clock()))
        self.assertEqual(self.fake.calls[before:], [])
        self.assertEqual((s.counts["idle_wakes"], s.counts["idle_pauses"]), (0, 0))
        self.assertFalse(self.fake.paused)

    def test_a_wake_is_raised_once_at_half_the_threshold_and_recorded(self):
        s = self.ready()
        self.fake.paused = False
        self.clock.now += IDLE_LIMIT / 2
        before = len(self.fake.calls)
        self.assertFalse(s.watchdog(self.clock()))
        self.assertFalse(s.watchdog(self.clock() + 60))
        self.assertEqual(s.counts["idle_wakes"], 1)
        self.assertEqual(self.fake.calls[before:], [])                   # no input, no pause, executor on
        self.assertFalse(self.fake.paused)
        self.assertTrue(runs.read_json(os.path.join(self.run_dir, "status.json"))["idle_woken"])
        with open(os.path.join(self.run_dir, "supervisor.log"), encoding="utf-8") as f:
            self.assertIn('"idle_wake"', f.read())
        out = s.handle("hoi4_observe", {"since": 0})                      # the wake arrives as a feed change
        wake = out["body"]["changes"][-1]
        self.assertEqual((wake["kind"], wake["urgent"], wake["detail"]["reason"]), ("wake_player", True, "idle"))
        self.assertTrue(any("no call from you" in n for n in out["body"]["supervisor_notes"]))
        self.assertNotIn("supervisor_notes", s.handle("hoi4_observe", {"since": 1})["body"])   # once

    def test_the_game_is_paused_at_the_threshold_and_the_next_call_lifts_it(self):
        s = self.ready()
        self.fake.paused = False
        self.clock.now += IDLE_LIMIT - 1
        self.assertFalse(s.watchdog(self.clock()))
        self.assertFalse(self.fake.paused)
        self.clock.now += 1
        self.assertTrue(s.watchdog(self.clock()))
        self.assertTrue(self.fake.paused)
        self.assertIn(("hoi4_autopilot", False), [(n, a.get("enabled")) for n, a in self.fake.calls])
        self.assertTrue(runs.read_json(os.path.join(self.run_dir, "status.json"))["idle_paused"])
        self.assertFalse(s.watchdog(self.clock() + 1000))                 # once
        out = s.handle("hoi4_overview", {})
        self.assertFalse(s.idle_paused)
        self.assertFalse(self.fake.paused)                                # lifted: the game runs again
        self.assertIn(("hoi4_autopilot", True), [(n, a.get("enabled")) for n, a in self.fake.calls[-3:]])
        self.assertTrue(any("lifted the idle pause" in n for n in out["body"]["supervisor_notes"]))
        s.stop("stopped_by_player")
        with open(os.path.join(self.run_dir, "report.md"), encoding="utf-8") as f:
            text = f.read()
        self.assertIn("## Idle pauses", text)
        self.assertIn("no player call for 15 minutes", text)
        self.assertIn("lifted 0.0 minutes later by the next player call", text)

    def test_a_player_call_resets_the_timer(self):
        s = self.ready()
        self.fake.paused = False
        self.clock.now += IDLE_LIMIT - 10
        s.watchdog(self.clock())                                          # the wake
        s.handle("hoi4_wait_change", {"since": 0})
        self.clock.now += IDLE_LIMIT - 10
        self.assertFalse(s.watchdog(self.clock()))
        self.assertFalse(self.fake.paused)
        self.assertEqual((s.counts["idle_wakes"], s.counts["idle_pauses"]), (2, 0))

    def test_the_idle_pause_can_be_switched_off_or_changed_during_the_run(self):
        s = self.ready()
        self.fake.paused = False
        runs.write_json(os.path.join(self.run_dir, "settings.json"), {"idle_pause_minutes": 0})
        self.assertIsNone(s.idle_limit())
        self.clock.now += 24 * 3600
        self.assertFalse(s.watchdog(self.clock()))
        self.assertFalse(self.fake.paused)
        self.assertEqual((s.counts["idle_wakes"], s.counts["idle_pauses"]), (0, 0))
        runs.write_json(os.path.join(self.run_dir, "settings.json"), {"idle_pause_minutes": 45})
        self.assertEqual(s.idle_limit(), 2700)
        runs.write_json(os.path.join(self.run_dir, "settings.json"), {"idle_pause_minutes": None})
        self.assertIsNone(s.idle_limit())

    def test_an_explicit_pause_is_kept_and_never_lifted_by_the_supervisor(self):
        s = self.ready()
        self.fake.paused = False
        s.handle("hoi4_time", {"paused": True})
        self.assertTrue(self.fake.paused)
        self.assertEqual(s.counts["pause_requests"], 1)
        self.clock.now += IDLE_LIMIT + 1
        self.assertTrue(s.watchdog(self.clock()))                         # executor off, no clock change
        self.assertEqual([a["paused"] for n, a in self.fake.calls if n == "hoi4_time"], [True])
        s.handle("hoi4_overview", {})
        self.assertTrue(self.fake.paused)                                 # the player's pause stays
        self.assertEqual([a["paused"] for n, a in self.fake.calls if n == "hoi4_time"], [True])

    def test_a_pause_asked_for_while_idle_paused_is_kept(self):
        s = self.ready()
        self.fake.paused = False
        self.clock.now += IDLE_LIMIT
        s.watchdog(self.clock())
        s.handle("hoi4_batch", {"calls": [["hoi4_time", {"paused": True}]]})
        self.assertTrue(self.fake.paused)
        self.assertFalse(s.idle_paused)
        self.assertEqual(s.counts["pause_requests"], 1)

    def test_the_idle_guard_never_unpauses_or_pauses_a_paused_game(self):
        s = self.ready()
        self.clock.now += IDLE_LIMIT + 1
        s.watchdog(self.clock())
        s.handle("hoi4_overview", {})
        self.assertNotIn("hoi4_time", self.fake.names())

    def test_play_writes_the_idle_pause_given_for_the_run(self):
        run = start.prepare(self.prefix, ITALY, "conquer Ethiopia", template(), idle_pause_minutes=0)
        self.assertEqual(runs.read_json(os.path.join(run, "settings.json")), {"idle_pause_minutes": 0})
        self.assertIsNone(runs.idle_pause_seconds(run, 20))

    def test_a_closed_game_stops_the_run_with_a_report(self):
        s = self.ready()
        runs.set_active(self.prefix, self.run_dir, {"address": "x"}, os.getpid())
        s.handle("hoi4_research_start", {"technology": "radio"})
        self.fake.running = False
        s.tick()
        self.assertTrue(s.finished.is_set())
        self.assertEqual(s.end_reason, "game_closed")
        self.assertIsNone(runs.active_run(self.prefix))
        last = runs.read_json(os.path.join(self.prefix, "runs", "last.json"))
        self.assertEqual(last["reason"], "game_closed")
        with open(os.path.join(self.run_dir, "report.md"), encoding="utf-8") as f:
            text = f.read()
        self.assertIn("Ended: game_closed", text)
        self.assertIn("as they were at the start", text)
        self.assertEqual(runs.read_json(os.path.join(self.run_dir, "status.json"))["state"], "ended")
        self.assertEqual(s.handle("hoi4_overview", {})["body"]["error"]["code"], "run_not_ready")

    def test_a_stop_turns_the_executor_off_and_releases_the_lease_but_never_closes_the_game(self):
        s = self.ready()
        s.stop("stopped_by_player")
        self.assertIn(("hoi4_input_lease", {"op": "release", "lease_token": "secret-token"}), self.fake.calls)
        offs = [a for n, a in self.fake.calls if n == "hoi4_autopilot" and a.get("enabled") is False]
        self.assertEqual(len(offs), 1)
        self.assertIn("hoi4_war_stats", self.fake.names())
        s.stop("again")                                                   # once
        self.assertEqual(s.end_reason, "stopped_by_player")

    def test_hoi4_run_reports_status_and_stops(self):
        s = self.ready()
        body = s.handle("hoi4_run", {"op": "status"})["body"]
        self.assertEqual((body["tag"], body["date"], body["executor_on"]), ("ITA", "1936.01.01.12", True))
        self.assertTrue(s.handle("hoi4_run", {"op": "stop"})["body"]["stopping"])
        self.assertTrue(s.finished.wait(10))
        self.assertTrue(s.handle("hoi4_run", {"op": "nope"})["isError"])

    def test_an_executor_stop_in_the_feed_reaches_the_player(self):
        s = self.ready()
        self.fake.handlers["hoi4_observe"] = lambda a: (False, {"seq": 7, "changes": [
            {"kind": "autopilot_stopped", "detail": {"reason": "invalid_plan"}}]})
        self.clock.now += supervisor.HEALTH_EVERY + 1
        self.assertTrue(s.health(self.clock()))
        self.assertEqual(s.feed_seq, 7)
        notes = s.handle("hoi4_overview", {})["body"]["supervisor_notes"]
        self.assertIn("invalid_plan", notes[0])

    def test_the_lease_is_renewed_and_taken_again_when_lost(self):
        s = self.ready()
        self.fake.handlers["hoi4_input_lease"] = lambda a: (
            (True, {"error": {"code": "lease_expired"}}) if a["op"] == "renew" else (False, {"lease_token": "new"}))
        self.clock.now += supervisor.RENEW_EVERY + 1
        s.renew(self.clock())
        self.assertEqual(s.token, "new")

    def test_start_failures_are_read_from_either_error_form(self):
        self.assertEqual(supervisor.stderr_code('Error: Error { code: "invalid_input_binding", message: "x" }'),
                         "invalid_input_binding")
        self.assertEqual(supervisor.stderr_code("unsupported_build: your game is 1.20\n"), "unsupported_build")
        self.assertEqual(supervisor.stderr_code("panic at nothing"), "")


# ---------------------------------------------------------------------------------------------------------------


CHILD = r"""
import json, sys
held = None
for line in sys.stdin:
    m = json.loads(line)
    if "id" not in m:
        continue
    if m["method"] == "tools/call" and m["params"]["name"] == "slow":
        held = m
        continue
    if m["method"] == "tools/call" and m["params"]["name"] == "exit":
        sys.exit(3)
    body = {"tools": [{"name": "a"}]} if m["method"] == "tools/list" else {
        "content": [{"type": "text", "text": json.dumps({"echo": m["params"].get("name")})}]}
    print(json.dumps({"jsonrpc": "2.0", "id": m["id"], "result": body}), flush=True)
    if held:
        print(json.dumps({"jsonrpc": "2.0", "id": held["id"], "result": {"content": [
            {"type": "text", "text": "{\"late\": true}"}]}}), flush=True)
        held = None
"""


class NativeProcessTests(unittest.TestCase):
    def test_replies_reach_their_request_out_of_order_and_an_exit_is_an_answer(self):
        folder = tempfile.mkdtemp(prefix="hoi4live_native_")
        self.addCleanup(shutil.rmtree, folder, True)
        child = native.Native([sys.executable, "-c", CHILD], os.path.join(folder, "stderr"))
        self.addCleanup(child.close)
        child.initialize()
        self.assertEqual(child.tools(), [{"name": "a"}])
        late = {}
        worker = threading.Thread(target=lambda: late.update(r=child.call("slow", {})))
        worker.start()
        self.assertEqual(child.call("fast", {}), (False, {"echo": "fast"}))
        worker.join(10)
        self.assertEqual(late["r"], (False, {"late": True}))
        err, body = child.call("exit", {})
        self.assertTrue(err)
        self.assertEqual(body["error"]["code"], "process_exited")
        self.assertFalse(child.alive())


class ProxyTests(Folder):
    def test_without_a_run_the_catalogue_is_listed_and_calls_say_how_to_start(self):
        p = proxy.Proxy(self.prefix, connect=lambda _p: None, catalogue=lambda _p: supervisor.player_tools(TOOLS))
        init = p.handle({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "x"}})
        self.assertEqual(init["result"]["serverInfo"]["name"], "hoi4-live-play")
        self.assertEqual(init["result"]["protocolVersion"], "x")
        names = [t["name"] for t in p.handle({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})["result"]["tools"]]
        self.assertIn("hoi4_research_start", names)
        self.assertIn("hoi4_run", names)
        out = p.handle({"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                        "params": {"name": "hoi4_overview", "arguments": {}}})["result"]
        self.assertTrue(out["isError"])
        self.assertIn("hoi4-live play", out["content"][0]["text"])
        status = p.handle({"jsonrpc": "2.0", "id": 4, "method": "tools/call",
                           "params": {"name": "hoi4_run", "arguments": {"op": "status"}}})["result"]
        self.assertFalse(status["isError"])
        self.assertIsNone(p.handle({"jsonrpc": "2.0", "method": "notifications/initialized"}))
        self.assertEqual(p.handle({"jsonrpc": "2.0", "id": 5, "method": "nope"})["error"]["code"], -32601)

    def test_the_offline_catalogue_comes_from_the_runtime_itself(self):
        binary = proxy.native_binary(self.prefix)
        os.makedirs(os.path.dirname(binary))
        open(binary, "w").close()

        def run(args, **_kwargs):
            self.assertEqual(args, [binary, "--list-tools"])
            return subprocess.CompletedProcess(args, 0, json.dumps({"tools": TOOLS}).encode(), b"")
        names = [t["name"] for t in proxy.offline_tools(self.prefix, run=run)]
        self.assertIn("hoi4_time", names)
        self.assertNotIn("hoi4_input_lease", names)
        shutil.rmtree(os.path.dirname(binary))
        self.assertEqual([t["name"] for t in proxy.offline_tools(self.prefix)], ["hoi4_batch", "hoi4_run"])

    def test_a_call_travels_over_the_private_endpoint_and_a_wrong_key_is_refused(self):
        s = SupervisorTests.supervisor(self)
        self.assertTrue(s.start())
        address, family = supervisor.endpoint_address()
        key = os.urandom(32)
        listener = supervisor.serve(s, address, family, key)
        self.addCleanup(listener.close)
        runs.set_active(self.prefix, self.run_dir, {"address": address, "family": family, "authkey": key.hex()},
                        os.getpid())
        p = proxy.Proxy(self.prefix)
        names = [t["name"] for t in p.tools()]
        self.assertIn("hoi4_batch", names)
        out = p.call("hoi4_research_start", {"technology": "radio"})
        self.assertFalse(out["isError"])
        self.assertEqual(self.fake.calls[-1][1]["lease_token"], "secret-token")
        runs.set_active(self.prefix, self.run_dir, {"address": address, "family": family,
                                                    "authkey": os.urandom(32).hex()}, os.getpid())
        self.assertEqual(json.loads(proxy.Proxy(self.prefix).call("hoi4_overview", {})["content"][0]["text"])
                         ["error"]["code"], "no_active_run")

    def test_the_stdio_loop_answers_each_request(self):
        p = proxy.Proxy(self.prefix, connect=lambda _p: None, catalogue=lambda _p: [])
        lines = [{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {}},
                 {"jsonrpc": "2.0", "method": "notifications/initialized"},
                 {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "hoi4_overview"}}]
        stdin = io.BytesIO(b"".join(json.dumps(m).encode() + b"\n" for m in lines) + b"not json\n")
        stdout = io.BytesIO()
        proxy.serve(p, stdin, stdout)
        replies = [json.loads(line) for line in stdout.getvalue().splitlines()]
        self.assertEqual(sorted(r["id"] for r in replies), [1, 2])


# ---------------------------------------------------------------------------------------------------------------


class Out(io.StringIO):
    pass


class FakeProcess:
    def __init__(self, code=None):
        self.code = code
        self.terminated = False

    def poll(self):
        return self.code

    def terminate(self):
        self.terminated = True


class StartTests(Folder):
    def facts(self, **display):
        return {"system": "macos", "native": {"path": "/p/native/hoi4-live-native"}, "game": {"exe": "/g/hoi4"},
                "display": display or {"mode": "windowed", "resolution": (1920, 1080), "gui_scale": 1.0},
                "screen": (1920, 1200), "user_dir": self.user_dir}

    def start(self, finder, state="ready", attach_ok=True, launch=False, **display):
        clock = Clock()
        self.seen = {"attach": [], "spawn": [], "steam": 0}

        def attacher(binary, pid, run):
            self.seen["attach"].append((binary, pid, run))
            return attach_ok, "attached" if attach_ok else "task_access_denied: refused"

        def spawner(prefix, run, binary):
            self.seen["spawn"].append(run)
            runs.write_json(os.path.join(run, "status.json"), {"state": state, "message": "x", "date": "1936.01.01"})
            return FakeProcess()

        out = Out()
        code = start.start_native(self.prefix, ITALY, "conquer Ethiopia", self.facts(**display), out,
                                  template(), launch=launch, wait_minutes=1,
                                  finder=finder, attacher=attacher, spawner=spawner,
                                  steam=lambda: self.seen.__setitem__("steam", self.seen["steam"] + 1),
                                  sleep=clock.sleep, clock=clock)
        return code, out.getvalue()

    def test_a_running_game_is_attached_and_the_one_instruction_printed(self):
        code, text = self.start(lambda exe: [812])
        self.assertEqual(code, start.EXIT_OK, text)
        run = self.seen["spawn"][0]
        self.assertEqual(self.seen["attach"], [("/p/native/hoi4-live-native", 812, run)])
        self.assertIn(runs.instruction(run), text)
        self.assertIn("hoi4-live stop", text)
        self.assertTrue(os.path.isfile(os.path.join(run, "user-files.json")))   # recorded before the attach
        self.assertEqual(self.seen["steam"], 0)

    def test_no_game_waits_then_says_so_and_launch_asks_steam(self):
        code, text = self.start(lambda exe: [])
        self.assertEqual(code, start.EXIT_NO_GAME)
        self.assertIn("Start Hearts of Iron IV the way you always do (Steam)", text)
        self.assertIn("No game appeared", text)
        self.assertEqual(self.seen["attach"], [])
        found = iter([[], [], [812]])
        code, text = self.start(lambda exe: next(found), launch=True)
        self.assertEqual(code, start.EXIT_OK, text)
        self.assertEqual(self.seen["steam"], 1)

    def test_two_games_or_a_refused_attach_stop_before_anything_runs(self):
        code, text = self.start(lambda exe: [1, 2])
        self.assertEqual(code, start.EXIT_NO_GAME)
        self.assertIn("More than one", text)
        code, text = self.start(lambda exe: [812], attach_ok=False)
        self.assertEqual(code, start.EXIT_NO_GAME)
        self.assertIn("task_access_denied", text)
        self.assertEqual(self.seen["spawn"], [])

    def test_a_failed_start_is_reported_with_its_reason(self):
        code, text = self.start(lambda exe: [812], state="failed")
        self.assertEqual(code, start.EXIT_NO_GAME)
        self.assertIn("did not start", text)

    def test_display_settings_are_printed_before_the_game_starts(self):
        code, text = self.start(lambda exe: [812], mode="fullscreen", resolution=(2560, 1440), gui_scale=1.0)
        self.assertEqual(code, start.EXIT_OK)
        self.assertIn("Display mode: Windowed", text)
        self.assertIn("Resolution: 1920x1080", text)

    def test_a_running_campaign_is_not_started_twice_and_a_stale_pointer_is_dropped(self):
        other = os.path.join(self.prefix, "runs", "old")
        os.makedirs(other)
        runs.set_active(self.prefix, other, {}, os.getpid())
        code, text = self.start(lambda exe: [812])
        self.assertEqual(code, start.EXIT_BUSY)
        self.assertIn("hoi4-live stop", text)
        runs.set_active(self.prefix, other, {}, 2 ** 22 + 12345)       # a process that is not running
        code, _text = self.start(lambda exe: [812])
        self.assertEqual(code, start.EXIT_OK)


class ReportTests(Folder):
    def test_receipts_count_the_last_status_of_each_action(self):
        run = self.make_run()
        rows = [{"id": "a", "status": "Dispatching"}, {"id": "a", "status": "Verified"},
                {"id": "b", "status": "NotApplied"}, {"id": "c", "status": "Dispatching"},
                {"id": "c", "status": "Unconfirmed"}]
        with open(os.path.join(run, "game-actions.jsonl"), "w", encoding="utf-8") as f:
            f.write("".join(json.dumps(r) + "\n" for r in rows))
        with open(os.path.join(run, "native.stderr"), "w", encoding="utf-8") as f:
            f.write('[action] {"self_pause":true,"restored":true}\n[action] {"self_pause":false}\n')
        got = report.receipts(run)
        self.assertEqual((got["Verified"], got["NotApplied"], got["Unconfirmed"]), (1, 1, 1))
        self.assertEqual(report.self_pauses(run), (1, 1))
        text = report.build(run, "stopped_by_player", {"already_done": 2}, {"date": "1936.05.01"}, 0, 600)
        for word in ("Italy (ITA)", "conquer Ethiopia", "10.0 minutes", "1936.05.01", "already done: 2",
                     "Verified: 1", "Self-pauses recorded by the runtime: 1"):
            self.assertIn(word, text)

    def test_capitulation_progress_against_the_aim(self):
        plan = {"goal": {"predicates": ["capitulated:ETH", "capitulated:AFA"],
                         "capitulation": {"enemy": "ETH", "vp_needed": 22, "levers": []}}}
        self.assertEqual(report.capitulation_aims(plan), [("ETH", 22), ("AFA", None)])
        run = self.make_run()
        final = {"date": "1936.09.01", "capitulation": [{"tag": "ETH", "held": 14.0, "total": 32.0, "needed": 22},
                                                        {"tag": "AFA", "held": 0, "total": 1.0, "needed": None}]}
        text = report.build(run, "stopped_by_player", {}, final, 0, 60)
        self.assertIn("ETH: held 14 of the 22 VP needed to capitulate (8 to go; 32 VP in all)", text)
        self.assertIn("AFA: held 0 VP of 1 in all; the plan states no capitulation aim", text)


if __name__ == "__main__":
    unittest.main()
