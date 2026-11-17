"""hoi4-live play: the run's own ends and segments, without a game.

The supervisor ends a run when the player falls silent, at the end date the user gave, when a system prompt stays
on screen; at its own ends it pauses and saves the game with the game's own save. `--continue` starts the next
segment from that save with the plan in force and the executor's memory carried. The runtime is a fake.
"""

import datetime
import io
import json
import os
import shutil
import tempfile
import unittest

import helpers

from hoi4_live import firstrun
from hoi4_live.play import game, runs, start, supervisor
from test_play_flow import ITALY, TOOLS, Clock, FakeNative, Folder, template

SAVE_TOOLS = TOOLS + [
    {"name": "hoi4_save", "inputSchema": {"type": "object", "properties": {
        "request_id": {"type": "string"}, "lease_token": {"type": "string"}}, "required": ["request_id", "lease_token"]}},
]


class SavingNative(FakeNative):
    """A runtime that has hoi4_save; the save is named like the game names it."""

    def __init__(self, **options):
        super().__init__(**options)
        self.handlers["hoi4_save"] = lambda a: (False, {"saved": "Italy_1936_01_01_12.hoi4", "directory": "x"})

    def tools(self):
        return SAVE_TOOLS


class Base(Folder):
    def supervisor(self, fake=None, request=None, **options):
        self.run_dir = self.make_run()
        if request:
            data = runs.read_json(os.path.join(self.run_dir, "request.json"))
            data.update(request)
            runs.write_json(os.path.join(self.run_dir, "request.json"), data)
        runs.record_user_files(self.user_dir, self.run_dir)
        self.fake = fake or SavingNative()
        self.clock = Clock()
        options.setdefault("swap", lambda: None)
        options.setdefault("game_memory", lambda: None)
        s = supervisor.Supervisor(self.run_dir, self.prefix, lambda: self.fake, clock=self.clock,
                                  sleep=self.clock.sleep, **options)
        self.addCleanup(s.close_records)
        return s

    def ready(self, **options):
        s = self.supervisor(**options)
        self.assertTrue(s.start())
        return s

    def read(self, name):
        with open(os.path.join(self.run_dir, name), encoding="utf-8") as f:
            return f.read()


class EndTests(Base):
    def test_a_silent_player_ends_the_run_with_the_game_paused_and_saved(self):
        s = self.ready()
        self.fake.paused = False
        self.clock.now += runs.PLAYER_SILENT_MINUTES * 60 + 1
        s.tick()
        self.assertEqual(s.end_reason, "player_silent")
        names = self.fake.names()
        stop = names.index("hoi4_save")
        self.assertIn(("hoi4_autopilot", False), [(n, a.get("enabled")) for n, a in self.fake.calls[:stop]])
        self.assertIn(("hoi4_time", True), [(n, a.get("paused")) for n, a in self.fake.calls[:stop]])
        self.assertIn("lease_token", self.fake.calls[stop][1])
        end = runs.read_json(os.path.join(self.run_dir, "end.json"))
        self.assertEqual((end["reason"], end["saved"]), ("player_silent", "Italy_1936_01_01_12.hoi4"))
        text = self.read("report.md")
        self.assertIn("player_silent: the player made no call", text)
        self.assertIn('Saved with the game\'s own save as "Italy_1936_01_01_12.hoi4"', text)
        self.assertIn("hoi4-live play --continue", text)

    def test_a_player_who_calls_keeps_the_run_and_the_silence_limit_can_be_switched_off(self):
        s = self.ready()
        self.clock.now += runs.PLAYER_SILENT_MINUTES * 60 - 5
        s.handle("hoi4_overview", {})
        self.clock.now += 60
        s.tick()
        self.assertFalse(s.stopped.is_set())
        settings = runs.read_json(os.path.join(self.run_dir, runs.SETTINGS))
        runs.write_json(os.path.join(self.run_dir, runs.SETTINGS), dict(settings, player_silent_minutes=0))
        self.clock.now += 10 * runs.PLAYER_SILENT_MINUTES * 60
        s.tick()
        self.assertFalse(s.stopped.is_set())

    def test_the_end_date_the_user_gave_ends_the_run_and_saves(self):
        s = self.ready(request={"until": "1936.01.01"})
        self.clock.now += supervisor.SAMPLE_EVERY + 1
        s.handle("hoi4_overview", {})
        s.tick()
        self.assertEqual(s.end_reason, "end_date")
        self.assertIn("hoi4_save", self.fake.names())

    def test_an_end_date_still_ahead_keeps_the_run(self):
        s = self.ready(request={"until": "1938.01.01"})
        self.clock.now += supervisor.SAMPLE_EVERY + 1
        s.handle("hoi4_overview", {})
        s.tick()
        self.assertFalse(s.stopped.is_set())

    def test_a_system_prompt_is_reported_at_once_and_ends_the_run_when_it_stays(self):
        shown = [["SecurityAgent"]]
        s = self.ready(dialogs=lambda: shown[0])
        s.tick()
        self.assertFalse(s.stopped.is_set())
        self.assertIn("SecurityAgent", runs.read_json(os.path.join(self.run_dir, "status.json"))["message"])
        notes = s.handle("hoi4_overview", {})["body"]["supervisor_notes"]
        self.assertIn("system prompt is on screen", notes[0])
        shown[0] = []
        s.tick()
        self.assertIsNone(s.dialog_since)
        shown[0] = ["UserNotificationCenter"]
        s.tick()
        self.clock.now += supervisor.DIALOG_MINUTES * 60 + 1
        s.handle("hoi4_overview", {})
        s.tick()
        self.assertEqual(s.end_reason, "system_dialog")
        self.assertNotIn("hoi4_save", self.fake.names())     # input is refused while the prompt is there
        self.assertEqual(s.counts["dialogs"], 2)

    def test_a_stop_by_the_user_saves_without_pausing_and_keeps_the_gate_state(self):
        s = self.ready()
        self.fake.paused = False
        s.stop("stopped_by_user")
        self.assertIn("hoi4_save", self.fake.names())
        self.assertNotIn(("hoi4_time", True), [(n, a.get("paused")) for n, a in self.fake.calls])
        gate = os.listdir(os.path.join(self.run_dir, "gate"))
        self.assertEqual(len(gate), 1)
        snap = runs.read_json(os.path.join(self.run_dir, "gate", gate[0]))
        self.assertEqual(sorted(snap["reads"]), sorted(t for t, _ in supervisor.GATE_READS))

    def test_a_failed_save_is_reported_with_what_to_do(self):
        fake = SavingNative()
        fake.handlers["hoi4_save"] = lambda a: (True, {"error": {"code": "input_readback_failed",
                                                                 "message": "No save file was written within 60 s"}})
        s = self.ready(fake=fake)
        s.stop("goal_reached")
        end = runs.read_json(os.path.join(self.run_dir, "end.json"))
        self.assertEqual(end["save_error"], "input_readback_failed")
        self.assertIn("The save at the end failed (input_readback_failed", self.read("report.md"))

    def test_the_game_is_brought_to_the_front_before_the_executor_starts(self):
        order = []
        fake = SavingNative()
        s = self.supervisor(fake=fake, front=lambda pid: order.append(("front", pid)) or True)
        fake.handlers["hoi4_autopilot"] = lambda a: order.append(("autopilot", a.get("enabled"))) or (False, {})
        self.assertTrue(s.start())
        self.assertEqual(order, [("front", 4242), ("autopilot", True)])


class FeedTests(Base):
    def test_the_whole_feed_is_kept_page_by_page(self):
        s = self.ready()
        pages = {1: {"seq": 9, "more": 2, "changes": [{"seq": 2, "kind": "a"}, {"seq": 3, "kind": "b"}]},
                 3: {"seq": 9, "more": 0, "changes": [{"seq": 8, "kind": "c"}, {"seq": 9, "kind": "goal_reached"}]}}
        self.fake.handlers["hoi4_observe"] = lambda a: (False, pages.get(a.get("since"), {"seq": 9, "changes": []}))
        self.clock.now += supervisor.HEALTH_EVERY + 1
        self.assertTrue(s.health(self.clock()))
        rows = [json.loads(line) for line in self.read("feed.jsonl").splitlines()]
        self.assertEqual([r["seq"] for r in rows], [2, 3, 8, 9])
        self.assertEqual(s.feed_seq, 9)
        self.assertIsNotNone(s.goal_reached)

    def test_production_is_sampled_for_the_report(self):
        s = self.ready()
        self.fake.handlers["hoi4_production_lines"] = lambda a: (False, {"lines": [
            {"entry_index": 0, "line_kind": "military", "equipment_id": "infantry_equipment_1",
             "factory_counters": {"requested_factories_raw_signed": 10, "active_factories_raw_signed": 9},
             "output": {"efficiency": 50.0}}]})
        self.fake.handlers["hoi4_stockpile"] = lambda a: (False, {"equipment": [
            {"archetype": "infantry_equipment", "stockpile": 100, "fielded_need": 50, "fielded_short": 0,
             "factories": 10, "produced_per_day": 5.0}]})
        self.fake.handlers["hoi4_economy"] = lambda a: (False, {"factory_trigger_count": {"military": 28},
                                                                 "political_power": {"raw_signed": 1500000}})
        self.clock.now += supervisor.SAMPLE_EVERY + 1
        s.sample(self.clock())
        line = json.loads(self.read("watch.jsonl").splitlines()[-1])
        self.assertEqual(line["prod"], {"date": "1936.01.01", "mil": 28, "pp": 15.0,
                                        "lines": [[0, "military", "infantry_equipment_1", 10, 9, 50.0]],
                                        "stock": [["infantry_equipment", 100, 50, 0, 10, 5.0]]})
        self.clock.now += supervisor.SAMPLE_EVERY + 1
        s.sample(self.clock())
        self.assertNotIn("prod", json.loads(self.read("watch.jsonl").splitlines()[-1]))


class ContinueTests(Base):
    def ended_run(self):
        s = self.ready()
        plan = runs.read_json(os.path.join(self.run_dir, "plan.json"))
        plan["stop_on_date"] = "1939.08.31"
        runs.write_json(os.path.join(self.run_dir, "plan.json"), plan)
        runs.write_json(os.path.join(self.run_dir, "plan_state.json"), {"done": ["x"]})
        s.stop("game_memory")
        return self.run_dir

    def test_a_new_segment_carries_the_plan_and_the_executors_memory(self):
        prev = self.ended_run()
        self.assertEqual(start.previous_run(self.prefix), prev)
        run = start.prepare_continue(self.prefix, prev, template(), now=datetime.datetime(2030, 1, 1))
        self.assertNotEqual(run, prev)
        self.assertEqual(runs.read_json(os.path.join(run, "plan_state.json")), {"done": ["x"]})
        plan = runs.read_json(os.path.join(run, "plan.json"))
        self.assertNotIn("stop_on_date", plan)
        self.assertIn("stop_on_date 1939.08.31 removed", plan["_meta"]["edits"][-1]["edit"])
        request = runs.read_json(os.path.join(run, "request.json"))
        self.assertEqual((request["continues"], request["segment"]), (prev, 2))
        self.assertEqual((request["resume_date"], request["resume_save"]),
                         ("1936.01.01.12", "Italy_1936_01_01_12.hoi4"))
        with open(os.path.join(run, "BRIEF.txt"), encoding="utf-8") as f:
            self.assertIn(run, f.read())
        self.assertIsNone(request["until"])

    def test_a_game_older_than_the_last_segment_is_refused(self):
        s = self.supervisor(request={"resume_date": "1937.06.01", "resume_save": "Italy_1937.hoi4"})
        self.assertFalse(s.start())
        self.assertEqual(s.end_reason, "older_save")
        self.assertIn("Italy_1937.hoi4", runs.read_json(os.path.join(self.run_dir, "status.json"))["message"])
        self.assertNotIn("hoi4_autopilot", [n for n, a in self.fake.calls if a.get("enabled")])

    def test_a_game_a_few_days_older_continues(self):
        s = self.supervisor(request={"resume_date": "1936.01.20"})
        self.assertTrue(s.start())

    def test_the_save_link_points_at_the_games_save_folder(self):
        run = self.make_run()
        self.assertTrue(runs.link_saves(self.user_dir, run))
        link = os.path.join(run, "userdata", "save games")
        self.assertEqual(os.path.realpath(link), os.path.realpath(os.path.join(self.user_dir, "save games")))
        self.assertTrue(runs.link_saves(self.user_dir, run))          # a second call keeps it
        self.assertFalse(runs.link_saves(os.path.join(self.prefix, "nowhere"), run))

    def test_a_new_plan_loses_its_stop_date_and_keeps_the_end_date(self):
        folder = tempfile.mkdtemp(prefix="hoi4live_plan_")
        self.addCleanup(shutil.rmtree, folder, True)
        with open(ITALY["path"], encoding="utf-8") as f:
            data = json.load(f)
        data["stop_on_date"] = "1936.06.01"
        path = os.path.join(folder, "p.plan.json")
        with open(path, "w", encoding="utf-8") as f:
            json.dump(data, f)
        run = start.prepare(self.prefix, dict(ITALY, path=path), "goal", template(), until="1938.01.01")
        self.assertNotIn("stop_on_date", runs.read_json(os.path.join(run, "plan.json")))
        self.assertEqual(runs.read_json(os.path.join(run, "request.json"))["until"], "1938.01.01")


class FrontDoorTests(unittest.TestCase):
    def setUp(self):
        self.prefix = tempfile.mkdtemp(prefix="hoi4live_cont_")
        self.addCleanup(shutil.rmtree, self.prefix, True)
        for part in ("plans", "briefs"):
            shutil.copytree(os.path.join(helpers.ROOT, "skills", "hoi4-campaign-playbook", part),
                            os.path.join(self.prefix, "app", "skills", "hoi4-campaign-playbook", part))

    def play(self, **options):
        from test_firstrun import facts
        out, started = io.StringIO(), []

        def start_native(prefix, plan, goal, facts_, out, template_, **kwargs):
            started.append((plan, goal, kwargs))
            return 0
        code = firstrun.play(self.prefix, options.pop("country", ""), options.pop("goal", ""), None,
                             gather=lambda _prefix: facts("macos"), out=out, start_native=start_native, **options)
        return code, out.getvalue(), started

    def test_continue_takes_the_last_run_and_its_goal(self):
        run = os.path.join(self.prefix, "runs", "r1")
        os.makedirs(run)
        runs.write_json(os.path.join(run, "request.json"), {"tag": "ITA", "country": "Italy", "goal": "win",
                                                            "mode": "fair", "plan": "p"})
        runs.write_json(os.path.join(run, "plan.json"), {"active": True})
        runs.write_json(os.path.join(self.prefix, "runs", "last.json"), {"run": run})
        code, text, started = self.play(continue_run="last", until="1939.09.01")
        self.assertEqual(code, 0, text)
        plan, goal, kwargs = started[0]
        self.assertEqual((plan["tag"], goal, kwargs["continue_from"], kwargs["until"]), ("ITA", "win", run,
                                                                                          "1939.09.01"))
        self.assertIn("the plan in force at the end of the last segment", text)

    def test_continue_without_an_earlier_run_says_how_to_start(self):
        code, text, started = self.play(continue_run="last")
        self.assertEqual(code, firstrun.EXIT_USAGE)
        self.assertIn("No earlier run to continue", text)

    def test_the_silence_limit_reaches_the_start_only_when_given(self):
        _code, _text, started = self.play(country="ITA", goal="x", player_silent_minutes=0)
        self.assertEqual(started[0][2]["player_silent_minutes"], 0)
        _code, _text, started = self.play(country="ITA", goal="x")
        self.assertNotIn("player_silent_minutes", started[0][2])

    def test_an_end_date_must_be_a_game_date(self):
        code, text, _ = self.play(country="ITA", goal="x", until="next year")
        self.assertEqual(code, firstrun.EXIT_USAGE)


class WindowTests(unittest.TestCase):
    ROWS = [{"owner": "Dock", "pid": 1, "id": 5, "layer": 20, "bounds": (0, 0, 10, 10)},
            {"owner": "SecurityAgent", "pid": 2, "id": 6, "layer": 0, "bounds": (0, 0, 400, 200)},
            {"owner": "hoi4", "pid": 4242, "id": 7, "layer": 0, "bounds": (0, 25, 1920, 1108)},
            {"owner": "hoi4", "pid": 4242, "id": 8, "layer": 0, "bounds": (0, 0, 40, 40)}]

    def test_system_prompts_and_the_game_window_are_found_by_owner(self):
        self.assertEqual(game.system_dialogs(self.ROWS), ["SecurityAgent"])
        self.assertEqual(game.system_dialogs(self.ROWS[:1]), [])
        self.assertEqual(game.game_window(4242, self.ROWS), {"id": 7, "bounds": (0, 25, 1920, 1108)})
        self.assertIsNone(game.game_window(99, self.ROWS))


if __name__ == "__main__":
    unittest.main()
