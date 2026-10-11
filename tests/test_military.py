"""Phase 4: the save's military model (templates, divisions, armies and their orders, army groups, fleets, air
wings), the army card and battle plan readers on live screenshots, the fleet card strip, air counters, deployment
spots, mission names, the battle plan toolbar layout from the game's interface file, map mode calibration storage
and the Find view's name folding."""

import os
import tempfile
from types import SimpleNamespace
from unittest.mock import patch
import unittest

import numpy as np
from PIL import Image

from helpers import FIXTURES

from hoi4_live.state.savefile import SaveFile
from hoi4_live.state import military

MIL = os.path.join(FIXTURES, "saves", "military_text.txt")
SCREENS = os.path.join(FIXTURES, "screens")


def load(name):
    return np.array(Image.open(os.path.join(SCREENS, name)).convert("RGB"))


def on_canvas(crop, x, y, size=(1080, 1920)):
    canvas = np.zeros(size + (3,), np.uint8)
    canvas[y:y + crop.shape[0], x:x + crop.shape[1]] = crop
    return canvas


class SaveModel(unittest.TestCase):
    def setUp(self):
        self.sf = SaveFile(MIL, use_cache=False)

    def tearDown(self):
        self.sf.close()

    def test_templates_of_the_country_only(self):
        t = military.templates(self.sf, "USA")
        self.assertEqual([x["name"] for x in t], ["Infantry Division", "Rio Grande Infantry"])
        inf = t[0]
        self.assertEqual(inf["battalions"], 3)
        self.assertEqual([r["type"] for r in inf["support"]], ["artillery"])
        rio = t[1]
        self.assertEqual([(r["type"], r["x"]) for r in rio["regiments"]],
                         [("infantry", 0), ("infantry", 1), ("artillery_brigade", 2), ("infantry", 3)])

    def test_divisions(self):
        d = military.divisions(self.sf, "USA")
        self.assertEqual(len(d), 3)
        self.assertEqual(d[1]["name"], "Texas Rangers")
        self.assertEqual([x["executing"] for x in d], [True, True, False])
        self.assertEqual(d[2]["template_id"], 210)

    def test_armies_orders_leaders_groups(self):
        names = lambda keys: {"USA_george_s_patton": "George S. Patton", "USA_omar_bradley": "Omar Bradley",
                              "o_operation": "Operation", "o_dark": "Dark", "o_star": "Star"}
        a = military.armies(self.sf, "USA", names)
        self.assertEqual(len(a["armies"]), 1)
        arm = a["armies"][0]
        self.assertEqual(arm["name"], "Army of the Rio Grande")
        self.assertEqual(arm["leader"], "George S. Patton")
        self.assertEqual(arm["divisions"], 2)
        self.assertEqual(arm["executing_divisions"], 2)
        # the game's helper orders (virtual ones and the front segment it creates for an offensive) are skipped
        self.assertEqual([o["type"] for o in arm["orders"]], ["front_line", "offensive"])
        self.assertEqual(arm["orders"][0]["operation"], "Operation Dark Star")
        self.assertEqual(arm["orders"][1]["to"], 2060)
        self.assertEqual(a["unassigned_divisions"], 1)
        g = a["army_groups"][0]
        self.assertEqual((g["name"], g["leader"], g["army_names"]), ("Army Group 1", "Omar Bradley",
                                                                     ["Army of the Rio Grande"]))

    def test_fleets(self):
        f = military.fleets(self.sf, "USA")
        self.assertEqual(f[0]["name"], "Battle Force")
        self.assertEqual(f[0]["ships"], 4)
        tf = f[0]["task_forces"]
        self.assertEqual((tf[0]["mission"], tf[0]["regions"]), (1, [32]))
        self.assertEqual(tf[1]["regions"], [])
        # patrol is mission 0: a task force sent to a sea region is told by its accessible_regions (live save)
        self.assertEqual((tf[2]["mission"], tf[2]["regions"], tf[2]["hours_in_mission"]), (0, [89], 3))

    def test_air_wings(self):
        w = military.air_wings(self.sf, "USA")
        self.assertEqual([(x["name"], x["planes"], x["mission"], x["region"]) for x in w],
                         [("1. Medium Bombardment Group", 60, 1, 31), ("2. Medium Bombardment Group", 72, 0, None)])
        self.assertEqual(w[0]["type"], "tac_bomber")

    def test_missing_country_is_empty(self):
        self.assertEqual(military.templates(self.sf, "MEX"), [])
        self.assertEqual(military.armies(self.sf, "MEX")["armies"], [])
        self.assertEqual(military.fleets(self.sf, "MEX"), [])
        self.assertEqual(military.air_wings(self.sf, "MEX"), [])


class Screens(unittest.TestCase):
    def test_army_card_boxes(self):
        from hoi4_live.game import armyview
        # live 1080p: [army group +][Army 1 4/24][Army 2 4/24][new army +], cropped from x = 480
        img = load("army_cards_480_bottom140.png")
        self.assertEqual(armyview.card_boxes(img), [307, 475, 556, 639])

    def test_fleet_card_strip(self):
        from hoi4_live.game import navalairview
        img = load("navy_cards_480_bottom140.png")
        xs = navalairview.card_strip(None, img)
        self.assertEqual(len(xs), 8)                     # seven fleets and the naval theatre card
        self.assertEqual(xs[:3], [190, 270, 350])

    def test_fleet_cards_a_highlight_hid_are_filled_in_from_the_grid(self):
        from hoi4_live.game import navalairview
        whole = [(552, 75), (643, 54), (723, 54), (803, 54), (883, 54), (963, 54), (1043, 54), (1123, 54), (1226, 55)]
        broken = [r for r in whole if r[0] not in (643, 723)]       # seen live: the first two cards vanished
        self.assertEqual(navalairview.cards_from_runs([broken]), navalairview.cards_from_runs([whole]))
        self.assertEqual(navalairview.cards_from_runs([whole])[:3], [670, 750, 830])
        self.assertEqual(navalairview.cards_from_runs([whole])[-1], 1253)       # the theatre card stays off the grid
        # another row of the same strip sees what the first lost
        self.assertEqual(navalairview.cards_from_runs([[(552, 75), (803, 54)], [(643, 54), (723, 54)]])[:4],
                         [670, 750, 830])
        self.assertEqual(navalairview.cards_from_runs([[]]), [])

    def test_empty_map_points_avoid_counters(self):
        from hoi4_live.game import armyview
        img = np.full((1080, 1920, 3), (30, 50, 90), np.uint8)
        first = armyview.empty_map_points(img, 1)[0]
        self.assertEqual(first, (int(1920 * 0.40), int(1080 * 0.30)))
        x, y = first
        img[y - 8:y + 8, x - 20:x + 20] = (40, 120, 50)                  # a green division counter on the first spot
        second = armyview.empty_map_points(img, 1)[0]
        self.assertNotEqual(second, first)
        self.assertGreater(len(armyview.empty_map_points(img, 3)), 1)
        img[:] = (255, 255, 255)                                         # nothing usable: a centre fallback
        self.assertEqual(armyview.empty_map_points(img, 1), [(960, 378)])

    def test_the_offensive_arrow_under_the_night_tint_is_a_plan_line(self):
        from hoi4_live.game import armyview
        img = on_canvas(load("offensive_arrow_night_700x420.png"), 610, 330)
        # (133, 34, 79) to (158, 66, 127): the arrow of a save loaded at 12:00; it was missed with the daylight red
        self.assertGreater(armyview.red_pixels_near(img, (960, 540)), 3000)
        self.assertGreaterEqual(len(armyview.plan_lines(img)), 2)

    def test_plan_lines_on_the_map(self):
        from hoi4_live.game import armyview
        img = on_canvas(load("plan_lines_750_300.jpg"), 750, 300)
        lines = armyview.plan_lines(img)
        self.assertGreaterEqual(len(lines), 3)           # front line pieces and the offensive arrow
        self.assertGreater(armyview.red_pixels_near(img, (960, 540)), 3000)
        p = armyview.nearest_line_point(img, (960, 540))
        self.assertTrue(p and abs(p[0] - 960) < 120 and abs(p[1] - 540) < 120, p)
        self.assertEqual(armyview.red_pixels_near(np.zeros_like(img), (960, 540)), 0)

    def test_air_counters(self):
        from hoi4_live.tools import military_tools
        img = on_canvas(load("air_counters_960_700.png"), 960, 700)
        self.assertEqual(military_tools._air_counters(img), [(1084, 785), (1084, 808)])

    def test_green_deployment_spot(self):
        from hoi4_live.tools import military_tools
        img = np.full((1080, 1920, 3), 60, np.uint8)
        for y in range(300, 700, 12):                     # green stripes right of the centre
            img[y:y + 5, 1100:1400] = (20, 230, 30)
        x, y = military_tools._green_spot(img, (960, 540))
        self.assertTrue(1100 <= x <= 1130 and 400 <= y <= 700, (x, y))
        self.assertIsNone(military_tools._green_spot(np.full((1080, 1920, 3), 60, np.uint8), (960, 540)))

    def test_word_groups(self):
        from hoi4_live.game import armyview
        words = [{"text": "4", "box": [936, 1060, 8, 10]}, {"text": "24", "box": [950, 1060, 16, 10]},
                 {"text": "12/24", "box": [1100, 1060, 30, 10]}]
        self.assertEqual([t for t, _ in armyview._word_groups(words)], ["4 24", "12/24"])


class Names(unittest.TestCase):
    def test_mission_names(self):
        from hoi4_live.game import navalairview as nv
        from hoi4_live.tools.military_tools import _mission_key
        self.assertEqual(_mission_key(nv.NAVAL_MISSIONS, "patrol"), "patrol")
        self.assertEqual(_mission_key(nv.NAVAL_MISSIONS, "Convoy Escort"), "convoy_escort")
        self.assertEqual(_mission_key(nv.AIR_MISSIONS, "Close Air Support"), "cas")
        self.assertEqual(_mission_key(nv.AIR_MISSIONS, "air superiority"), "air_superiority")
        with self.assertRaises(Exception):
            _mission_key(nv.NAVAL_MISSIONS, "bombard the moon")

    def test_find_view_folding(self):
        from hoi4_live.game.mapview import _fold
        self.assertEqual(_fold("Nuevo León"), "nuevoleon")
        self.assertEqual(_fold("Coahu ila"), "coahuila")

    def test_words_in(self):
        from hoi4_live.tools.military_tools import _words_in
        self.assertTrue(_words_in("United States", "The United States"))
        self.assertFalse(_words_in("United States", "w Mexico"))

    def test_battle_plan_toolbar_layout(self):
        from hoi4_live.game import armyview
        with tempfile.TemporaryDirectory(prefix="hoi4live_toolbar_") as root:
            interface = os.path.join(root, "interface")
            os.makedirs(interface)
            path = os.path.join(interface, "battleplantools.gui")
            # A small public fixture from the layout contract; no installed game,
            # user playset, permissions or native desktop Session is needed.
            with open(path, "w", encoding="utf-8") as stream:
                stream.write('guiTypes = { containerWindowType = {\n'
                             'name = "battleplantools_window" position = { x = -270 y = -245 }\n'
                             'show_position = { x = -270 y = -245 }\n'
                             'buttonType = { name = "btn_order_draw_defence" position = { x = 239 y = 33 } }\n'
                             'buttonType = { name = "btn_order_draw_offence" position = { x = 276 y = 33 } }\n'
                             '} }\n')
            s = SimpleNamespace(cfg=SimpleNamespace(game_dir=root, user_dir=root))
            with patch.dict(armyview._BPT_CACHE, {}, clear=True):
                lay = armyview._bpt_layout(s)
            self.assertEqual(lay["source"], path)
            self.assertEqual(lay["buttons"]["front_line"], (239, 33))
            self.assertEqual(lay["buttons"]["offensive"], (276, 33))
            # at 1920x1080 the toolbar sits at the bottom centre: front line at x 944, y 883 (checked live)
            w, h = 1920, 1080
            wx, wy = lay["window"]
            self.assertEqual((w // 2 + wx + 239 + 15, h + wy + 33 + 15), (944, 883))


if __name__ == "__main__":
    unittest.main()
