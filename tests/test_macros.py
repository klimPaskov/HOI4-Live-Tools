"""Macro tools: construction plan (state choice from the telemetry's state scan)."""

import unittest
from unittest import mock

from helpers import FIXTURES  # noqa: F401

from hoi4_live.errors import ToolError
from hoi4_live.telemetry.parser import TelemetryStore
from hoi4_live.tools import economy_tools as eco

ROWS = [
    {"s": 5, "name": "Ermland-Masuren", "free": 4, "civ": 0, "mil": 0, "infra": 3, "ctrl": "GER"},
    {"s": 42, "name": "Moselland", "free": 7, "civ": 3, "mil": 0, "infra": 4, "ctrl": "GER"},
    {"s": 50, "name": "Wurttemberg", "free": 0, "civ": 1, "mil": 3, "infra": 3, "ctrl": "GER"},
    {"s": 9, "name": "Elsass", "free": 2, "civ": 1, "mil": 1, "infra": 5, "ctrl": "FRA"},
    {"s": 6, "name": "Hessen", "free": 3, "civ": 5, "mil": 2, "infra": 5, "ctrl": "GER"},
]


class PlanStatesTests(unittest.TestCase):
    def names(self, plan):
        return [(r["name"], n) for r, n in plan]

    def test_most_free_slots_first_per_state_cap_and_total(self):
        plan = eco.plan_states(ROWS, 6, 3, None, "free_slots", "GER")
        self.assertEqual(self.names(plan), [("Moselland", 3), ("Ermland-Masuren", 3)])

    def test_full_and_occupied_states_are_skipped(self):
        plan = eco.plan_states(ROWS, 20, 8, None, "free_slots", "GER")
        names = [r["name"] for r, n in plan]
        self.assertNotIn("Wurttemberg", names)       # no free slot
        self.assertNotIn("Elsass", names)            # controlled by France

    def test_total_is_respected_and_counts_are_limited_by_free_slots(self):
        plan = eco.plan_states(ROWS, 5, 8, None, "free_slots", "GER")
        self.assertEqual(sum(n for _, n in plan), 5)
        self.assertEqual(self.names(eco.plan_states(ROWS, 40, 8, None, "free_slots", "GER"))[-1], ("Hessen", 3))

    def test_other_orders(self):
        self.assertEqual(self.names(eco.plan_states(ROWS, 3, 3, None, "industry", "GER"))[0][0], "Hessen")
        self.assertEqual(self.names(eco.plan_states(ROWS, 3, 3, None, "infrastructure", "GER"))[0][0], "Hessen")

    def test_states_filter_by_id_or_name(self):
        plan = eco.plan_states(ROWS, 9, 3, ["5", "hessen"], "free_slots", "GER")
        self.assertEqual(sorted(r["name"] for r, n in plan), ["Ermland-Masuren", "Hessen"])


class FakeSession:
    def __init__(self, rows):
        self.store = TelemetryStore()
        for r in rows:
            self.store.add(dict(r, k="st", t="GER", n=1, _wall=1.0, _date="1936.1.4.1"))
        self.store.add({"k": "day", "t": "GER", "n": 1, "_date": "1936.1.4.1", "_wall": 1.0})

    @property
    def telemetry(self):
        return self.store

    def check_cancel(self):
        pass


class PlanToolTests(unittest.TestCase):
    ARGS = {"action": "plan", "levels": 5, "per_state": 3, "choose": "free_slots", "dry_run": True}

    def test_dry_run_lists_the_plan_without_building(self):
        res = eco.hoi4_construction(FakeSession(ROWS), dict(self.ARGS))
        self.assertIn("Moselland x3", res.summary)
        self.assertEqual(sum(x["levels"] for x in res.data["plan"]), 5)

    def test_without_a_state_scan_the_error_says_what_to_do(self):
        with self.assertRaises(ToolError) as cm:
            eco.hoi4_construction(FakeSession([]), dict(self.ARGS))
        self.assertIn("state scan", str(cm.exception))

    def test_building_calls_the_build_action_per_state_and_reports_failures(self):
        calls = []
        real = eco.hoi4_construction

        def fake(session, args):
            if args["action"] != "build":
                return real(session, args)
            calls.append((args["state"], args["count"]))
            if args["state"] == "5":                    # states are passed by id (Ermland-Masuren)
                raise ToolError("no slot")
            from hoi4_live.tools.registry import ToolResult
            return ToolResult(f"queued x{args['count']} in {args['state']}", {})
        args = dict(self.ARGS, dry_run=False)
        with mock.patch.object(eco, "hoi4_construction", fake), \
                mock.patch("hoi4_live.game.constructionview.read_queue", lambda s, img=None: {"rows": [1], "used": 3, "total": 9}):
            res = eco._construction_plan(FakeSession(ROWS), args)
        self.assertEqual(calls, [("42", 3), ("5", 2)])
        self.assertEqual(res.data["failed"][0]["state"], "Ermland-Masuren")
        self.assertEqual(len(res.data["queued"]), 1)
        self.assertFalse(res.is_error)


class RequirementIconTests(unittest.TestCase):
    def img(self, rgb):
        import numpy as np
        a = np.zeros((60, 200, 3), np.uint8)
        a[10:28, 20:34] = rgb                    # the icon, 6 px left of a text box at x=40
        return a

    def test_red_cross_green_check_and_nothing(self):
        from hoi4_live.game.focusview import _mark_left_of
        box = [40, 12, 100, 14]
        self.assertIs(_mark_left_of(self.img((220, 40, 40)), box), False)
        self.assertIs(_mark_left_of(self.img((60, 200, 60)), box), True)
        self.assertIsNone(_mark_left_of(self.img((120, 120, 120)), box))


class TileOffsetTests(unittest.TestCase):
    def idx(self):
        gb = {"x": 750, "y": 40, "w": 70, "h": 70, "sw": None, "sw_pct": 0.8, "parent_w": None,
              "parent_fill": True, "format": "UPPER_LEFT"}
        return {"techs": {"t": {"folders": [{"name": "nsb_armour_folder", "x": 2, "y": 5},
                                              {"name": "artillery_folder", "x": 2, "y": 5}], "parents": []}},
                "layout": {"nsb_armour_folder": {"gridboxes": {"t_tree": dict(gb)}, "margin_w": 38},
                           "artillery_folder": {"gridboxes": {"t_tree": dict(gb)}, "margin_w": 38}}}

    def test_nsb_armour_columns_are_left_aligned_and_other_folders_are_centred(self):
        from hoi4_live.gamedata import tech
        idx = self.idx()
        left = tech.tile_offset(idx, "t", "nsb_armour_folder", 1920)
        centred = tech.tile_offset(idx, "t", "artillery_folder", 1920)
        self.assertEqual(left[0], 750 + 2 * 70 + 35)
        self.assertAlmostEqual(centred[0] - left[0], (0.8 * (1920 - 38) - 70) / 2, places=3)


class PlaceholderNameTests(unittest.TestCase):
    def test_static_part_and_title_pattern(self):
        from hoi4_live.game.focusview import placeholder_pattern
        q, rx = placeholder_pattern("Establish the [GetReichsWerkeName]")
        self.assertEqual(q, "Establish the")
        self.assertTrue(rx.search("Establish the Reichswerke Hermann Goering"))
        self.assertFalse(rx.search("Establish the"))
        self.assertFalse(rx.search("Expand the Reichswerke"))
        q, rx = placeholder_pattern("Anti-[SOV.GetAdjective] Pact")
        self.assertIn(q, ("Anti-", "Pact"))
        self.assertTrue(rx.search("Anti-Soviet Pact"))
        self.assertEqual(placeholder_pattern("Anschluss"), ("Anschluss", None))


class EffectLineTests(unittest.TestCase):
    def test_effect_lines_end_the_requirement_list(self):
        from hoi4_live.game.focusview import EFFECT_LINE_RX
        for s in ("Effect:", "Klaipeda Claim gained by German Reich.", "World Tension is increased by 3.0",
                  "This Focus will cancel if the requirements are not met.",
                  "The following will bypass the focus:"):
            self.assertTrue(EFFECT_LINE_RX.search(s), s)
        for s in ("Is fully independent", "At least 725.00K manpower in divisions in the field", "At peace"):
            self.assertFalse(EFFECT_LINE_RX.search(s), s)


class FocusRequirementTests(unittest.TestCase):
    def req(self, *rows):
        return [{"text": t, "met": m} for t, m in rows]

    def test_one_of_group_is_met_when_any_line_is_green(self):
        from hoi4_live.game.focusview import unmet_requirements
        r = self.req(("Cannot be cancelled manually", False), ("Requires one of the following:", None),
                     ("- Heed von Neurath's Concerns", False), ("- Reorganize the Wehrmacht", True), ("At peace", None))
        self.assertEqual(unmet_requirements(r), [])

    def test_one_of_group_with_no_green_line_blocks(self):
        from hoi4_live.game.focusview import unmet_requirements
        r = self.req(("Requires one of the following:", None), ("- A", False), ("- B", False))
        self.assertEqual(unmet_requirements(r), ["one of: A; B"])

    def test_plain_red_lines_block_and_notes_do_not(self):
        from hoi4_live.game.focusview import unmet_requirements
        r = self.req(("Cannot be cancelled manually", False), ("Has completed Remilitarize", False), ("At peace", True))
        self.assertEqual(unmet_requirements(r), ["Has completed Remilitarize"])

    def test_a_red_line_after_the_group_is_not_swallowed_by_it(self):
        from hoi4_live.game.focusview import unmet_requirements
        r = self.req(("Requires one of the following:", None), ("- A", True), ("Not at war", False))
        self.assertEqual(unmet_requirements(r), ["Not at war"])


if __name__ == "__main__":
    unittest.main()


class ArmyPanelTests(unittest.TestCase):
    def test_panel_of_an_army_with_a_commander(self):
        import numpy as np
        from unittest import mock
        from hoi4_live.game import armyview as av
        from hoi4_live.ocr.winocr import OcrLine, OcrResult

        def ln(text, x, y):
            return OcrLine(text, [x, y, 120, 14], [{"text": text, "box": [x, y, 120, 14]}])
        lines = [ln("42 Heer Nord", 70, 98), ln("Generat", 70, 126), ln("Alfred Jodi", 70, 142),
                 ln("17. Infanterie-Division", 190, 250)]

        class S:
            ocr = mock.Mock()
            cfg = mock.Mock(game_dir=".")

            def grab(self, focus=None):
                return np.zeros((1080, 1920, 3), np.uint8)
        S.ocr.recognize = lambda img, region=None, scale=None, **k: OcrResult(lines if region and region[3] > 100 else [], region, 1)
        p = av.panel(S())
        self.assertEqual(p["kind"], "army")
        self.assertEqual(p["name"], "Heer Nord")
        self.assertEqual(p["commander"], "Alfred Jodi")


class OrdersAttackTests(unittest.TestCase):
    def run_attack(self, args, fail_on=None):
        from hoi4_live.tools import military_tools as mt
        calls = []

        def fake(session, a):
            calls.append((a["action"], a.get("target"), a.get("army")))
            if fail_on and a["action"] == fail_on:
                raise ToolError(f"{fail_on} failed")
            from hoi4_live.tools.registry import ToolResult
            return ToolResult("ok", {})
        with mock.patch.object(mt, "hoi4_orders", fake):
            res = mt._orders_attack(None, args)
        return mt, calls, res

    def test_attack_deletes_draws_front_and_offensive_then_executes(self):
        mt, calls, res = self.run_attack({"army": "Heer Nord", "front_target": "Poznan", "target": "Lodz",
                                          "replace": True})
        self.assertEqual([c[0] for c in calls], ["delete", "front", "offensive", "execute"])
        self.assertEqual(calls[1][1], "Poznan")
        self.assertEqual(calls[2][1], "Lodz")
        self.assertIn("executing", res.summary)

    def test_missing_old_orders_do_not_stop_the_attack(self):
        mt, calls, _ = self.run_attack({"army": "A", "front_target": "X", "target": "Y", "replace": True},
                                       fail_on="delete")
        self.assertEqual([c[0] for c in calls], ["delete", "front", "offensive", "execute"])

    def test_batch_lists_failures_and_continues(self):
        from hoi4_live.tools import military_tools as mt
        seen = []

        def fake_attack(session, a):
            seen.append(a.get("army"))
            if a.get("army") == "B":
                raise ToolError("no army named B")
            from hoi4_live.tools.registry import ToolResult
            return ToolResult(f"{a['army']}: executing", {})
        session = mock.Mock()
        with mock.patch.object(mt, "_orders_attack", fake_attack):
            res = mt._orders_batch(session, {"orders": [{"army": "A", "front_target": "x", "target": "y"},
                                                        {"army": "B", "front_target": "x", "target": "y"},
                                                        {"army": "C", "front_target": "x", "target": "y"}]})
        self.assertEqual(seen, ["A", "B", "C"])
        self.assertEqual(len(res.data["ok"]), 2)
        self.assertEqual(res.data["failed"][0]["army"], "B")

    def test_attack_always_redraws_its_lines_even_over_an_executing_plan(self):
        from hoi4_live.tools import military_tools as mt
        seen = []

        def fake(session, a):
            seen.append((a["action"], a.get("replace")))
            from hoi4_live.tools.registry import ToolResult
            return ToolResult("ok", {})
        with mock.patch.object(mt, "hoi4_orders", fake):
            mt._orders_attack(None, {"army": "A", "front_target": "X", "target": "Y"})
        self.assertEqual(seen[0][0], "delete")
        self.assertEqual([r for a, r in seen if a in ("front", "offensive")], [True, True])

    def test_attack_needs_both_targets(self):
        from hoi4_live.tools import military_tools as mt
        with self.assertRaises(ToolError):
            mt._orders_attack(None, {"army": "A", "target": "Y"})


class StartButtonTests(unittest.TestCase):
    def test_green_is_enabled_and_grey_is_disabled(self):
        import numpy as np
        from hoi4_live.game.focusview import _start_enabled
        g = np.zeros((60, 200, 3), np.uint8)
        g[:] = (60, 68, 54)
        self.assertTrue(_start_enabled(g, [60, 20, 40, 12]))
        g[:] = (65, 65, 65)
        self.assertFalse(_start_enabled(g, [60, 20, 40, 12]))


class GameModeTests(unittest.TestCase):
    def header(self, text):
        import os
        import tempfile
        d = tempfile.mkdtemp()
        p = os.path.join(d, "s.hoi4")
        open(p, "wb").write(text if isinstance(text, bytes) else text.encode("utf-8"))
        from hoi4_live.game import gamemode
        return gamemode.read_header(p), p

    def test_single_player_text_save(self):
        h, _ = self.header('HOI4txt\nplayer="GER"\ndate="1940.5.1.2"\ndifficulty="normal"\n'
                           'player_countries={\n\tGER={\n\t\tuser="UNKNOWN"\n\t}\n}\n'
                           'mods={\n\t"HOI4 Live Telemetry"\n}\n\tironman=0\n')
        self.assertFalse(h["ironman"])
        self.assertFalse(h["multiplayer"])
        self.assertEqual((h["difficulty"], h["player"], h["mods"]), ("normal", "GER", ["HOI4 Live Telemetry"]))

    def test_binary_save_is_ironman_and_two_users_is_multiplayer(self):
        h, _ = self.header(b"HOI4bin" + bytes([0, 1]))
        self.assertTrue(h["ironman"])
        h, _ = self.header('HOI4txt\nplayer_countries={\n\tGER={\n\t\tuser="A"\n\t}\n\tITA={\n\t\tuser="B"\n\t}\n}\n')
        self.assertTrue(h["multiplayer"])

    def test_check_refuses_only_fresh_ironman_saves(self):
        import os
        import time
        from hoi4_live.game import gamemode
        h, p = self.header('HOI4txt\nplayer="GER"\n\tironman=1\n')
        session = mock.Mock()
        session.cfg.saves_dir = os.path.dirname(p)
        session._mode_cache = None
        with self.assertRaises(ToolError) as cm:
            gamemode.check(session, "hoi4_console")
        self.assertEqual(cm.exception.details["error"], "ironman")
        old = time.time() - 7 * 3600
        os.utime(p, (old, old))
        session._mode_cache = None
        self.assertTrue(gamemode.check(session, "hoi4_console")["ironman"])      # an old save says nothing
