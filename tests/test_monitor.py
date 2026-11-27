"""The run monitor (hoi4_live/observe): issue detection, the response path, escalation packets,
lessons and costs, over fixtures cut from real runs (tests/fixtures/monitor/)."""
import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

from hoi4_live.observe import issue_rules as R
from hoi4_live.observe.escalation import MAX_PACKET_CHARS, Escalator, build_packet, gate_kind, validate_answer
from hoi4_live.observe.issues import Detector
from hoi4_live.observe.lessons import LessonStore, lessons_from_issue, walk
from hoi4_live.observe.monitor import Monitor
from hoi4_live.observe.runlogs import date_of_days, game_days

FIX = os.path.join(os.path.dirname(__file__), "fixtures", "monitor")
SEED = os.path.join(os.path.dirname(__file__), "..", "hoi4_live", "observe", "lessons_seed.json")


def fixture(name):
    with open(os.path.join(FIX, name), encoding="utf-8") as f:
        return json.load(f)


def snap(date, findings, at_war=True, **extra):
    return dict({"date": date, "utc": 0, "wars": {"at_war": at_war}, "findings": findings}, **extra)


def finding(code, text, subject=None, severity="high", **numbers):
    return {"code": code, "severity": severity, "text": text, "subject": subject, "numbers": numbers}


def events(out):
    with open(os.path.join(out, "issues.jsonl"), encoding="utf-8") as f:
        return [json.loads(line) for line in f]


def write_run(tmp_path, calls=(), actions=(), server=(), snaps=()):
    run = tmp_path / "run"
    (run / "state").mkdir(parents=True)
    with open(run / "calls.jsonl", "w") as f:
        for c in calls:
            f.write(json.dumps(c) + "\n")
    with open(run / "game-actions.jsonl", "w") as f:
        for a in actions:
            f.write(json.dumps(a) + "\n")
    with open(run / "mcp.stderr", "w") as f:
        for a in server:
            f.write("[action] " + json.dumps(a) + "\n")
    for i, s in enumerate(snaps):
        (run / "state" / f"{i:05d}.json").write_text(json.dumps(s))
    return str(run)


def overdue_issue(tmp_path, code="province_overstacked"):
    det = Detector(str(tmp_path))
    det.snapshot(snap("1940.05.10.00", [finding(code, "63 divisions on 4 provinces, supply 21-30%", "3230")]))
    det.snapshot(snap("1940.05.20.00", [finding(code, "63 divisions on 4 provinces, supply 21-30%", "3230")]))
    return det, det.issues[f"{code}:3230"]



class MonitorTest(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.tmp, True)

    # --- the calendar ----------------------------------------------------------------------------------

    def test_game_days_round_trip_and_order(self):
        d = game_days("1940.05.10.21")
        assert date_of_days(d) == "1940.05.10"
        assert game_days("1941.01.01") - game_days("1940.12.31") == 1
        assert game_days("1940.03.01") - game_days("1940.02.28") == 1  # the game has no leap days


    # --- detection: state issues (snapshot findings) -----------------------------------------------------

    def test_air_force_left_over_a_pacified_region_is_a_gameplay_issue_with_a_war_deadline(self):
        tmp_path = self.tmp
        # g17n, January 1940: 11 fighter wings over Western Poland three months after its capitulation
        det = Detector(str(tmp_path))
        det.snapshot(snap("1940.01.02.12", [finding(
            "air_away_from_front", "11 wings (1100 planes) fly air_superiority over Western Poland, no enemy there",
            subject="Western Poland", wings=11)]))
        it = det.issues["air_away_from_front:Western Poland"]
        assert (it["class"], it["lane"], it["severity"]) == ("gameplay", "air_plan", "high")
        assert it["deadline"] == "1940.01.05"  # 3 days at war
        assert "air_plan" in det.render()


    def test_planning_at_the_declaration_counts_only_attacking_armies(self):
        # s7n (24 Nov 2026): the Westwall hold (area defence, a front with no offensive) was
        # counted as an attacker with 0 planning
        det = Detector(str(self.tmp))
        armies = [
            {"army": "Hold", "divisions": 24, "planning": {"mean": 0.0},
             "orders": [{"kind": "area_defense", "offensives": []}]},
            {"army": "West", "divisions": 16, "planning": {"mean": 0.0},
             "orders": [{"kind": "front", "offensives": []}]},
            {"army": "Nord", "divisions": 23, "planning": {"mean": 0.36},
             "orders": [{"kind": "front", "offensives": [{"id": 1}]}]},
        ]
        det.snapshot(snap("1939.09.01.01", [], trigger="war_declared:POL", land={"armies": armies}))
        assert not [k for k in det.issues if k.startswith("planning_low_at_declaration")]
        armies[2]["planning"]["mean"] = 0.02
        det.snapshot(snap("1939.09.01.02", [], trigger="war_declared:POL", land={"armies": armies}))
        assert [k for k in det.issues if k.startswith("planning_low_at_declaration")]

    def test_transport_wings_standing_by_are_not_idle(self):
        from hoi4_live.observe import findings as fm
        a = {"wings": [{"wing": "LG Greifswald", "airframe": "transport_plane_equipment_1", "missions": []},
                       {"wing": "KG 1", "airframe": "medium_plane_airframe_1", "missions": []}],
             "wings_without_mission": ["LG Greifswald", "KG 1"]}
        out = fm.air({"air": a}, None)
        hit = [f for f in out if f["code"] == "air_wings_without_mission"]
        assert len(hit) == 1 and hit[0]["numbers"]["wings"] == 1 and "KG 1" in hit[0]["text"]

    def test_group_without_field_marshal_is_idling_owned_by_the_command_lane(self):
        tmp_path = self.tmp
        det = Detector(str(tmp_path))
        det.snapshot(snap("1939.10.01.00", [finding("army_group_without_field_marshal",
                                                    "Heeresgruppe Nord: 3 armies, no field marshal", "Heeresgruppe Nord")]))
        it = det.issues["army_group_without_field_marshal:Heeresgruppe Nord"]
        assert (it["class"], it["lane"]) == ("idling", "command")


    def test_divisions_waiting_in_reserve_open_a_battle_issue_and_resolve_when_gone(self):
        tmp_path = self.tmp
        # West bed, May 1940: 23 divisions attack one forest province, 4 engaged, 19 in reserve
        det = Detector(str(tmp_path))
        f = finding("battle_reserve_waiting", "Armee Pommern: 23 divisions attack Neufchateau, 4 engaged, 19 in reserve",
                    "Neufchateau", divisions=23, reserve=19)
        det.snapshot(snap("1940.05.14.06", [f]))
        det.snapshot(snap("1940.05.15.06", [f]))
        it = det.issues["battle_reserve_waiting:Neufchateau"]
        assert it["state"] == "open" and it["seen"] == 2 and it["lane"] == "war"
        det.snapshot(snap("1940.05.16.06", []))
        assert it["state"] == "resolved"
        det.flush()
        kinds = [e["event"] for e in events(str(tmp_path))]
        assert kinds == ["open", "resolved"]


    def test_idle_dockyards_from_a_real_snapshot_go_overdue_after_a_week_at_peace(self):
        tmp_path = self.tmp
        snaps = fixture("w53a_snapshots.json")
        det = Detector(str(tmp_path))
        first = dict(snaps[0], wars={"at_war": False})
        det.snapshot(first)
        it = det.issues["dockyards_unassigned"]
        assert it["lane"] == "dockyards" and it["class"] == "idling"
        det.snapshot(dict(first, date="1940.05.20.00"))
        assert it["overdue"] and it["state"] == "open"
        assert any(e["event"] == "overdue" and e["key"] == "dockyards_unassigned" for e in det.pending)


    def test_real_snapshot_overhead_is_measured_and_small(self):
        tmp_path = self.tmp
        det = Detector(str(tmp_path))
        for s in fixture("w53a_snapshots.json"):
            det.snapshot(s)
        assert len(det.overhead_ms) == 3
        assert max(det.overhead_ms) < 100  # milliseconds a snapshot; the real runs measured 1-5 ms


    def test_game_alert_frames_are_low_and_never_escalated(self):
        tmp_path = self.tmp
        det = Detector(str(tmp_path))
        det.snapshot(snap("1940.05.08.22", [finding("frame_4", "unlabelled alert")]))
        it = det.issues["frame_4"]
        assert it["severity"] == "low" and it["escalate"] is False


    # --- detection: logs (calls, executor orders, refusals) ------------------------------------------------

    def test_saves_failing_from_real_calls(self):
        tmp_path = self.tmp
        calls = [c for c in fixture("g17n_calls.json") if c["tool"] == "hoi4_save"]
        det = Detector(str(tmp_path))
        for c in calls:
            det.call(c)
        it = det.issues["save_failing"]
        failed = sum(1 for c in calls if c["isError"])
        assert failed >= 5 and it["lane"] == "saves"
        assert it["numbers"]["failed"] == failed


    def test_player_argument_faults_from_real_calls_are_tool_use(self):
        tmp_path = self.tmp
        calls = [c for c in fixture("g17n_calls.json") if c["tool"] in ("hoi4_event_choose", "hoi4_politics_options")]
        det = Detector(str(tmp_path))
        for c in calls:
            det.call(c)
        keys = [k for k, i in det.issues.items() if i["class"] == "tool_use"]
        assert any(k.startswith("invalid_arguments:hoi4_event_choose") or
                   k.startswith("refusal_retried_unchanged:player") for k in keys)


    def test_executor_refusal_retried_six_times_from_real_logs(self):
        tmp_path = self.tmp
        # g17n: one air rebase sent again and again to a full base
        acts = [a for a in fixture("executor_actions.json") if a["run"] == "g17n"]
        srv = [a for a in fixture("executor_server.json") if a["run"] == "g17n"]
        det = Detector(str(tmp_path))
        rows = sorted([(a["unix_ms"], -1 if a["status"] == "Dispatching" else 1, "a", a) for a in acts] + [(a["unix_ms"] or 0, 0, "s", a) for a in srv],
                      key=lambda r: r[:2])
        for _, _, kind, rec in rows:
            (det.action if kind == "a" else det.server_action)(rec)
        worst = max((i for i in det.issues.values() if i["code"] == "refusal_retried_unchanged"),
                    key=lambda i: i["numbers"]["times"])
        assert worst["numbers"]["times"] >= 6
        assert worst["actor"] == "executor" and worst["escalate"] is False  # the ledger's, not a model's
        assert worst["numbers"]["code"] in ("air_base_full", "rebase_refused")


    def test_convoy_line_over_the_cap_refused_unchanged(self):
        tmp_path = self.tmp
        det = Detector(str(tmp_path))
        det.clock("1939.11.01.00", False, 0)
        order = {"kind": "production_set", "line": "convoy_1", "dockyards": 18}
        for n in range(3):
            rid = f"plan-conv-{n}"
            det.action({"id": rid, "action": order, "status": "Dispatching"})
            det.server_action({"executor": True, "tool": "hoi4_production_set", "ok": False,
                               "error": "dockyards_over_cap", "request_id": rid, "ms": 900})
            det.action({"id": rid, "action": order, "status": "NotApplied"})
        it = det.issues[next(k for k in det.issues if k.startswith("refusal_retried_unchanged:executor"))]
        assert it["numbers"] == {"times": 3, "code": "dockyards_over_cap"}
        assert it["lane"] in ("production", "dockyards", "convoys")


    def test_duplicate_verified_orders_within_a_week(self):
        tmp_path = self.tmp
        det = Detector(str(tmp_path))
        det.clock("1940.05.12.00", False, 0)
        for n in range(3):
            det.action({"id": f"p{n}", "action": {"kind": "army_plan", "army": "Army Group 1", "execute": True},
                        "status": "Verified"})
        it = next(i for i in det.issues.values() if i["code"] == "duplicate_action")
        assert it["class"] == "duplicate" and it["numbers"]["times"] == 3


    def test_game_not_advancing_while_unpaused(self):
        tmp_path = self.tmp
        det = Detector(str(tmp_path))
        det.clock("1940.05.12.00", False, 1000.0)
        det.clock("1940.05.12.00", False, 1200.0)
        assert det.issues["game_not_advancing"]["class"] == "technical"


    # --- the response path ---------------------------------------------------------------------------

    def test_every_rule_lane_is_an_executor_lane_or_a_reporting_lane(self):
        reporting = {"gate", "strategy", "plan", "alerts", "ledger", "executor", "saves", "player", "server"}
        for code, (cls, lane, *_rest) in R.RULES.items():
            assert cls in R.CLASSES, code
            # None: the tool's lane decides (R.lane_of)
        assert lane is None or lane in R.LANE_KINDS or lane in reporting, (code, lane)


    def test_an_executor_order_of_the_owning_lane_marks_the_issue_acted(self):
        tmp_path = self.tmp
        det = Detector(str(tmp_path))
        det.snapshot(snap("1939.10.01.00", [finding("dockyards_unassigned", "6 dockyards idle", severity="medium")],
                          at_war=False))
        det.action({"id": "p1", "action": {"kind": "production_add", "equipment": "convoy_1"}, "status": "Verified"})
        it = det.issues["dockyards_unassigned"]
        assert it["acts"] == 1 and it["state"] == "acted"
        det.flush()
        open_rows = json.load(open(tmp_path / "issues_open.json", encoding="utf-8"))["issues"]
        assert open_rows[0]["lane"] == "dockyards"


    # --- escalation (Tier 1) ---------------------------------------------------------------------------

    def test_one_escalation_packet_content_and_size(self):
        tmp_path = self.tmp
        det, it = overdue_issue(tmp_path)
        esc = Escalator(str(tmp_path), lessons=LessonStore(str(tmp_path / "store")))
        esc.lessons.seed(SEED)
        rec = esc.consider("overdue", it, det.date, True)
        assert rec and rec["why"] == "a judgement between named alternatives"
        packet = open(next((tmp_path / "escalations").glob(f"{it['id']}-*.md")), encoding="utf-8").read()
        assert len(packet) <= MAX_PACKET_CHARS and rec["packet_tokens_est"] < 1300
        for part in ("ESCALATION", "WHAT:", "SINCE:", "ALLOWED RESPONSES", "NAMED ALTERNATIVES", "LESSON short_front"):
            assert part in packet, part
        # a second sibling of the same kind within two weeks is covered by the first answer
        _, it2 = overdue_issue(tmp_path / "b")
        assert esc.consider("overdue", it2, "1940.05.21.00", True) is None


    def test_escalation_budget_per_game_year(self):
        tmp_path = self.tmp
        esc = Escalator(str(tmp_path), per_year=2)
        n = 0
        for i, code in enumerate(["province_overstacked", "losing_exchange", "victory_points_flat"]):
            _, it = overdue_issue(tmp_path / str(i), code)
            n += bool(esc.consider("overdue", it, "1940.05.20.00", True))
        assert n == 2
        assert esc.log[-1]["skipped"] == "budget for the game year used"


    def test_executor_refusals_and_gate_codes_never_go_to_tier_1(self):
        tmp_path = self.tmp
        esc = Escalator(str(tmp_path))
        det = Detector(str(tmp_path))
        det.snapshot(snap("1940.05.10.00", [finding("executor_repeated_refusal", "air_mission refused 13 times")]))
        it = det.issues["executor_repeated_refusal"]
        it["severity"] = "high"
        assert esc.trigger("stale", it) is None
        it2 = det.raise_issue("checkpoint_missed:cp", "checkpoint_missed", "cp missed", "high")
        assert it2["lane"] == "gate" and esc.trigger("overdue", it2) is None


    def test_answers_are_constrained_to_allowed_actions(self):
        tmp_path = self.tmp
        _, it = overdue_issue(tmp_path)
        ok, why = validate_answer('{"kind":"plan_edit","path":"operations[0].front","value":[1],"why":"longer"}', it)
        assert ok and why == "ok"
        bad, why = validate_answer('{"kind":"action","tool":"hoi4_save","args":{},"why":"x"}', it)
        assert bad is None and "not allowed" in why
        assert validate_answer("no idea", it)[0] is None


    def test_answer_command_is_logged_with_tokens_and_seconds(self):
        tmp_path = self.tmp
        _, it = overdue_issue(tmp_path)
        env = {"result": '{"kind":"leave","why":"the attack ends tomorrow"}',
               "usage": {"input_tokens": 900, "output_tokens": 30}, "total_cost_usd": 0.0002}
        script = tmp_path / "answer.py"
        script.write_text("import sys\nsys.stdin.read()\nprint(%r)\n" % json.dumps(env))
        cmd = f'"{sys.executable}" "{script}"'
        esc = Escalator(str(tmp_path), answer_cmd=cmd)
        rec = esc.consider("overdue", it, "1940.05.20.00", True)
        assert rec["answered"] and rec["answer"]["kind"] == "leave"
        assert rec["tokens"]["input"] == 900 and rec["seconds"] >= 0


    def test_gate_kinds(self):
        assert gate_kind("1939.12.30.00", {"date": "1940.01.01.00"}) == "new_year"
        assert gate_kind("1940.05.09", {"date": "1940.05.10", "trigger": "war_declared:HOL"}) == "war_declared"
        assert gate_kind(None, {"date": "1940.05.10"}, failure="crash") == "failure"


    # --- lessons -----------------------------------------------------------------------------------------

    def test_seeded_lessons_are_valid_and_general(self):
        rows = json.load(open(SEED, encoding="utf-8"))
        assert len(rows) >= 30
        ids = [r["id"] for r in rows]
        assert len(ids) == len(set(ids))
        for r in rows:
            assert r["statement"] and r["evidence"] and r["situation"] and r["checked"], r["id"]
            assert "/Users/" not in json.dumps(r)
            ex = r.get("executor")
            if ex:
                assert ex["treat_as"] in ("transient", "permanent", "wait") and r["situation"].get("cause")


    def test_a_lesson_written_from_a_run_is_applied_on_the_next_occurrence(self):
        tmp_path = self.tmp
        store = LessonStore(str(tmp_path / "store"))
        det = Detector(str(tmp_path))
        det.lessons = store
        det.clock("1939.12.01.00", False, 0)
        order = {"kind": "air_mission", "wing": "JG 1", "mission": "air_superiority", "region": 39}

        def refuse(n, cause="events_open"):
            rid = f"plan-air-{n}"
            det.action({"id": rid, "action": order, "status": "Dispatching"})
            det.server_action({"executor": True, "tool": "hoi4_air_mission", "ok": False, "error": cause,
                               "request_id": rid, "ms": 800})
            det.action({"id": rid, "action": order, "status": "NotApplied"})

        refuse(1)
        refuse(2)
        it = det.issues[next(k for k in det.issues if k.startswith("refusal_retried_unchanged"))]
        assert "lesson" not in it  # nothing known yet
        det.action({"id": "plan-air-3", "action": order, "status": "Verified"})
        assert it["state"] == "resolved" and it["passed_after"] == 2
        le = lessons_from_issue(store, it, "resolved", "run-a")
        assert le["executor"]["treat_as"] == "transient" and le["scope"] == "any"
        # the executor's lookup (mirrored in Python) applies it to the same cause from now on
        assert store.executor_advice("hoi4_air_mission", "events_open")["id"] == le["id"]
        # next occurrence: the issue carries the lesson for the executor and the player
        det.clock("1940.02.01.00", False, 10)
        refuse(4)
        refuse(5)
        it2 = det.issues[it["key"]]
        assert it2["lesson"]["id"] == le["id"] and it2["lesson"]["treat_as"] == "transient"
        det.flush()
        row = json.load(open(tmp_path / "issues_open.json", encoding="utf-8"))["issues"][0]
        assert row["lesson"]["treat_as"] == "transient" and row["cause"] == "events_open"
        # confirmed again: its confidence rises
        det.action({"id": "plan-air-6", "action": order, "status": "Verified"})
        le2 = lessons_from_issue(store, det.issues[it["key"]], "resolved", "run-b")
        assert le2["confidence"] > le["confidence"] and le2["confirmations"] == 1


    def test_technical_causes_write_no_executor_lesson(self):
        tmp_path = self.tmp
        store = LessonStore(str(tmp_path))
        it = {"code": "refusal_retried_unchanged", "class": "tool_use", "tool": "hoi4_air_mission", "passed_after": 3,
              "numbers": {"code": "input_readback_failed", "times": 3}, "text": "x"}
        assert lessons_from_issue(store, it, "resolved", "r") is None


    def test_contradicted_lessons_lose_confidence_and_retire(self):
        tmp_path = self.tmp
        store = LessonStore(str(tmp_path))
        sit = {"code": "dockyards_unassigned"}
        store.learn(sit, "convoys cleared it", "worked", "e1", "any", confidence=0.4)
        for i in range(3):
            store.learn(sit, "it came back", "failed", f"e{i}", "any")
        worked = store.get("dockyards_unassigned_worked")
        assert worked["status"] == "retired" and worked["contradictions"] == 3


    def test_lessons_from_another_game_version_are_not_used(self):
        tmp_path = self.tmp
        old = LessonStore(str(tmp_path), game_version="1.18.0")
        old.learn({"tool": "hoi4_save", "cause": "events_open"}, "s", "worked", "e", "any",
                  executor={"treat_as": "transient"}, confidence=0.9)
        assert LessonStore(str(tmp_path), game_version="1.19.4.1").executor_advice("hoi4_save", "events_open") is None
        assert LessonStore(str(tmp_path), game_version="1.18.0").executor_advice("hoi4_save", "events_open")


    def test_plan_lint_warns_on_a_row_a_lesson_contradicts(self):
        tmp_path = self.tmp
        store = LessonStore(str(tmp_path))
        store.seed(SEED)
        plan = {"operations": [{"formations": [{"offensive": {"target_line": [6]}},
                                               {"offensive": {"target_line": [6, 7]}}]}]}
        warns = store.lint_plan(plan)
        assert [w["where"] for w in warns if w["lesson"] == "one_state_spear_line_stops"] == \
            ["operations[0].formations[0].offensive.target_line"]
        assert list(walk({"a": [{"b": 1}, {"b": 2}]}, "a[*].b")) == [("a[0].b", 1), ("a[1].b", 2)]


    # --- the whole monitor over a run folder -----------------------------------------------------------

    def test_monitor_replay_writes_every_report(self):
        tmp_path = self.tmp
        snaps = [dict(s, utc=1700000000 + i * 600) for i, s in enumerate(fixture("w53a_snapshots.json"))]
        calls = [dict(c, utc=1700000000 + i) for i, c in enumerate(fixture("g17n_calls.json"))]
        run = write_run(tmp_path, calls=calls, snaps=snaps)
        out = str(tmp_path / "out")
        mon = Monitor(run, out, replay=True, store=str(tmp_path / "store"))
        evs = mon.replay()
        assert any(e["event"] == "open" for e in evs)
        for name in ("issues.jsonl", "issues_open.json", "issues_counts.json", "ISSUES.md", "COSTS.md", "COVERAGE.md",
                     "GATE.md", "monitor_stats.json"):
            assert os.path.exists(os.path.join(out, name)), name
        counts = json.load(open(os.path.join(out, "issues_counts.json"), encoding="utf-8"))
        assert "1940" in counts
        gate = open(os.path.join(out, "GATE.md"), encoding="utf-8").read()
        assert len(gate) < 8000
        cov = open(os.path.join(out, "COVERAGE.md"), encoding="utf-8").read()
        assert "hoi4_save" in cov


    def test_monitor_survives_bad_lines(self):
        tmp_path = self.tmp
        for i, bad in enumerate(["", "{not json", '{"date": null}']):
            base = tmp_path / str(i)
            run = write_run(base)
            with open(os.path.join(run, "calls.jsonl"), "a") as f:
                f.write(bad + "\n")
            Monitor(run, str(base / "out"), replay=True, store=str(base / "s")).replay()


if __name__ == "__main__":
    unittest.main()
