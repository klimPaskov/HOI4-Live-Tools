"""Version 1.2: state ids everywhere, the front read from a save, air missions by base state and wing type, naval
missions by strategic region id, the minimap calibration. Pure data and logic with synthetic game files and saves; the
screen parts are covered with a recording session double."""

import json
import os
import tempfile
import unittest

import numpy as np
from PIL import Image

from helpers import FIXTURES  # noqa: F401  (puts the repo root on sys.path)

from hoi4_live.gamedata import mapdata
from hoi4_live.state import front, military
from hoi4_live.state.savefile import SaveFile

# --------------------------------------------------------------------------------------------------- game files
COLOURS = {1: (255, 0, 0), 2: (0, 255, 0), 3: (0, 0, 255), 4: (255, 255, 0), 5: (0, 255, 255)}


def make_game(root):
    """A 40 x 20 world: provinces 1 and 2 are states 10 and 11 side by side (top left, top right), province 4 is
    state 12 below state 10, province 3 and 5 are sea (the strait between 10 and 11 and the south east)."""
    os.makedirs(os.path.join(root, "map", "strategicregions"))
    os.makedirs(os.path.join(root, "history", "states"))
    img = np.zeros((20, 40, 3), np.uint8)
    img[0:10, 0:15] = COLOURS[1]
    img[0:10, 15:20] = COLOURS[3]
    img[0:10, 20:40] = COLOURS[2]
    img[10:20, 0:15] = COLOURS[4]
    img[10:20, 15:40] = COLOURS[5]
    Image.fromarray(img).save(os.path.join(root, "map", "provinces.bmp"))
    with open(os.path.join(root, "map", "definition.csv"), "w") as f:
        f.write("0;0;0;0;land;false;unknown;0\n")
        for pid, (r, g, b) in COLOURS.items():
            kind = "sea" if pid in (3, 5) else "land"
            f.write(f"{pid};{r};{g};{b};{kind};false;plains;1\n")
    with open(os.path.join(root, "history", "states", "10-A.txt"), "w") as f:
        f.write("state={\n\tid=10\n\tname=\"STATE_10\"\n\tprovinces={\n\t\t1 \n\t}\n\thistory={\n\t\towner = GER\n"
                "\t\tvictory_points = { 1 5 }\n\t}\n}\n")
    with open(os.path.join(root, "history", "states", "11-B.txt"), "w") as f:
        f.write("state={\n\tid=11\n\tname=\"STATE_11\"\n\tprovinces={\n\t\t2 \n\t}\n\thistory={\n\t\towner = POL\n"
                "\t\tvictory_points = { 2 9 }\n\t}\n}\n")
    with open(os.path.join(root, "history", "states", "12-C.txt"), "w") as f:
        f.write("state={\n\tid=12\n\tname=\"STATE_12\"\n\tprovinces={\n\t\t4 \n\t}\n\thistory={\n\t\towner = POL\n\t}\n}\n")
    with open(os.path.join(root, "map", "strategicregions", "7-Strait.txt"), "w") as f:
        f.write("strategic_region={\n\tid=7\n\tname=\"STRATEGICREGION_7\"\n\tprovinces={\n\t\t3 5 # a comment\n\t}\n}\n")
    with open(os.path.join(root, "map", "buildings.txt"), "w") as f:
        f.write("10;air_base;5.00;10.00;12.00;0.0;0\n")


class MapData(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="hoi4live_map_")
        self.old = os.environ.get("LOCALAPPDATA")
        os.environ["LOCALAPPDATA"] = self.tmp
        mapdata._MEM.clear()
        make_game(self.tmp + "/game")
        self.idx = mapdata.index(self.tmp + "/game", [])

    def tearDown(self):
        if self.old is None:
            os.environ.pop("LOCALAPPDATA", None)
        else:
            os.environ["LOCALAPPDATA"] = self.old
        mapdata._MEM.clear()

    def test_states_anchor_vp_and_neighbours(self):
        st = self.idx["states"]
        self.assertEqual(sorted(st), ["10", "11", "12"])
        self.assertEqual(st["10"]["anchor_province"], 1)
        self.assertEqual(sum(st["10"]["vp"].values()), 5)
        self.assertEqual(st["11"]["anchor_province"], 2)
        # the strait province (sea) separates 10 and 11; 10 touches 12 along the shared edge
        self.assertEqual(st["10"]["neighbours"], [12])
        self.assertEqual(st["12"]["neighbours"], [10])
        self.assertNotIn("neighbours", st["11"])

    def test_building_positions_use_the_bitmap_row(self):
        # buildings.txt z counts from the bottom edge: row = height - z
        self.assertEqual(mapdata.point_of(self.idx, 10, "air_base"), (5.0, 8.0))
        self.assertEqual(mapdata.point_of(self.idx, 11, "air_base"), tuple(self.idx["states"]["11"]["anchor"]))

    def test_strategic_region_aim_is_a_sea_province_and_comments_are_skipped(self):
        r = self.idx["regions"]["7"]
        self.assertEqual(r["provinces"], [3, 5])
        self.assertTrue(r["naval"] and r["all_sea"])
        self.assertIn(tuple(r["aim"]), {tuple(self.idx["provinces"]["3"]["xy"]), tuple(self.idx["provinces"]["5"]["xy"])})

    def test_index_is_cached_on_disk(self):
        files = [f for f in os.listdir(os.path.join(self.tmp, "hoi4_live", "script_index")) if f.startswith("mapdata_")]
        self.assertEqual(len(files), 1)

    def test_state_at_and_distance_helpers(self):
        self.assertEqual(mapdata.state_at(self.idx, self.idx["states"]["11"]["anchor"])[0], 11)
        self.assertEqual(mapdata.neighbours_by_distance(self.idx, 10, 1)[0][1], 12)


# --------------------------------------------------------------------------------------------------- the save
def save_text():
    """Germany (GER) with two divisions in state 10's province 1 and one in province 4; Poland (POL) with three
    divisions in province 2 (state 11) and one in 4; the controller of province 4 is POL although GER owns state 12."""
    def div(i, prov, strength, org, tpl=1):
        return (f"\t\t\tdivision={{\n\t\t\t\tid={{ id={i} type=51 }}\n\t\t\t\tlocation={prov}\n"
                f"\t\t\t\tdivision_template_id={{ id={tpl} type=52 }}\n\t\t\t\torganisation={org}\n"
                f"\t\t\t\tstrength={strength}\n\t\t\t}}\n")
    return ("HOI4txt\nplayer=\"GER\"\ndate=\"1939.9.5.12\"\nversion=\"x\"\nsave_version=33\n"
            "states={\r\n\t10={\r\n\t\towner=\"GER\"\r\n\t}\r\n\t11={\r\n\t\towner=\"POL\"\r\n\t}\r\n"
            "\t12={\r\n\t\towner=\"GER\"\r\n\t}\r\n}\n"
            "provinces={\r\n\t4={\r\n\t\tcontroller=\"POL\"\r\n\t}\r\n}\n"
            "strategic_air={\n\tair_base={\n\t\tid={ id=10 type=65 }\n\t\tstate=10\n\t}\n"
            "\tGER={\n\t\tair_wing_pool={\n\t\t\tdefinition=fighter\n\t\t\tair_base={ id=10 type=65 }\n"
            "\t\t\tair_wings={\n\t\t\t\tcount=100\n\t\t\t\tname=\"JG 1\"\n\t\t\t}\n\t\t}\n\t}\n}\n"
            "countries={\n\tGER={\n\t\tunits={\n" + div(1, 1, 200, 40) + div(2, 1, 180, 20) + div(3, 4, 100, 5)
            + "\t\t}\n\t\ttheatres={\n\t\t\ttheatre={\n\t\t\t\torders_group={\n\t\t\t\t\tid={ id=5 type=1 }\n"
              "\t\t\t\t\tname=\"Heer A\"\n\t\t\t\t\tmember={ unit={ id=1 type=51 } }\n"
              "\t\t\t\t\tmember={ unit={ id=2 type=51 } }\n\t\t\t\t}\n"
              "\t\t\t\torders_group={\n\t\t\t\t\tid={ id=6 type=1 }\n\t\t\t\t\tname=\"Heer B\"\n"
              "\t\t\t\t\tmember={ unit={ id=3 type=51 } }\n\t\t\t\t}\n\t\t\t}\n\t\t}\n\t}\n"
            "\tPOL={\n\t\tunits={\n" + div(10, 2, 300, 50) + div(11, 2, 300, 50) + div(12, 2, 280, 30)
            + div(13, 4, 250, 40) + "\t\t}\n\t}\n}\n")


class Front(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="hoi4live_front_")
        self.old = os.environ.get("LOCALAPPDATA")
        os.environ["LOCALAPPDATA"] = self.tmp
        mapdata._MEM.clear()
        make_game(self.tmp + "/game")
        self.idx = mapdata.index(self.tmp + "/game", [])
        self.path = os.path.join(self.tmp, "s.hoi4")
        with open(self.path, "w", newline="") as f:
            f.write(save_text())
        self.sf = SaveFile(self.path, use_cache=False)

    def tearDown(self):
        self.sf.close()
        if self.old is None:
            os.environ.pop("LOCALAPPDATA", None)
        else:
            os.environ["LOCALAPPDATA"] = self.old
        mapdata._MEM.clear()

    def test_owners_and_controllers(self):
        self.assertEqual(front.owners(self.sf), {10: "GER", 11: "POL", 12: "GER"})
        self.assertEqual(front.controllers(self.sf), {4: "POL"})

    def test_state_control_follows_the_province_controller(self):
        c = front.state_control(self.idx, front.owners(self.sf), front.controllers(self.sf))
        self.assertEqual(c[12], {"owner": "GER", "controller": "POL", "share": 1.0})
        self.assertEqual(c[10]["controller"], "GER")

    def test_divisions_by_state(self):
        d = front.divisions_by_state(self.sf, "POL", self.idx)
        self.assertEqual({k: len(v) for k, v in d.items()}, {11: 3, 12: 1})

    def test_situation_armies_touch_and_enemy_divisions(self):
        sit = front.situation(self.sf, "GER", self.idx, ["POL"], {"STATE_12": "Southland"})
        by = {a["name"]: a for a in sit["armies"]}
        a = by["Heer A"]
        self.assertEqual((a["divisions"], a["main_state"]), (2, 10))
        # state 12 is held by Poland (controller) and borders state 10: the one enemy state beside Heer A
        self.assertEqual([(t["state"], t["name"], t["enemy_divisions"]) for t in a["touch"]], [(12, "Southland", 1)])
        # Heer B stands inside the enemy-held state 12 itself and touches state 10's side only through it
        self.assertEqual(by["Heer B"]["main_state"], 12)
        self.assertEqual(sit["enemy"]["POL"]["divisions"], 4)
        self.assertEqual(sit["enemy"]["POL"]["top_states"][0]["state"], 11)
        self.assertEqual([o["state"] for o in sit["objectives"]], [11])
        self.assertEqual(sit["fronts"]["POL"][0]["ours"], 10)

    def test_suggest_orders_gives_each_army_its_own_objective(self):
        sit = front.situation(self.sf, "GER", self.idx, ["POL"], {})
        sit["_idx"] = self.idx
        plan = front.suggest_orders(sit)
        self.assertEqual([o["army"] for o in plan["orders"]], ["Heer A"])
        self.assertEqual(plan["orders"][0]["front_target"], 12)
        self.assertEqual(plan["orders"][0]["target"], 11)
        self.assertIn("Heer B", plan["idle"])
        forced = front.suggest_orders(sit, objectives=["12"])
        self.assertEqual(forced["orders"][0]["target"], 12)

    def test_air_wings_carry_the_state_of_their_base(self):
        w = military.air_wings(self.sf, "GER")
        self.assertEqual(len(w), 1)
        self.assertEqual((w[0]["type"], w[0]["base"], w[0]["base_state"], w[0]["planes"]), ("fighter", 10, 10, 100))


# --------------------------------------------------------------------------------------------------- the screen
class FakeInput:
    def __init__(self):
        self.calls = []

    def click(self, x, y, button="left", clicks=1, **kw):
        self.calls.append(("click", x, y, button))

    def key_down(self, k):
        self.calls.append(("down", k))

    def key_up(self, k):
        self.calls.append(("up", k))


class FakeSession:
    def __init__(self):
        self.input = FakeInput()


class WingSelection(unittest.TestCase):
    def test_all_wings_by_the_header_some_by_rows_with_shift(self):
        from hoi4_live.tools import military_tools as mt
        s = FakeSession()
        mt._select_wings(s, [0, 1, 2], 3)
        self.assertEqual(s.input.calls, [("click", *mt.WING_HEADER, "left")])
        s = FakeSession()
        mt._select_wings(s, [0, 3], 4)
        ys = [c[2] for c in s.input.calls if c[0] == "click"]
        self.assertEqual(ys, [int(mt.WING_ROW_Y0), int(mt.WING_ROW_Y0 + 3 * mt.WING_ROW_PITCH)])
        self.assertEqual([c for c in s.input.calls if c[0] in ("down", "up")], [("down", "shift"), ("up", "shift")])
        # the shift is released before the next wing row is clicked and the first row is a plain click
        self.assertEqual(s.input.calls[0][0], "click")


class Minimap(unittest.TestCase):
    def test_camera_rectangle_centre(self):
        from hoi4_live.game import mapview
        img = np.zeros((1080, 1920, 3), np.uint8)
        img[1000:1011, 1700] = (230, 20, 20)
        img[1000:1011, 1730] = (230, 20, 20)
        img[1000, 1700:1731] = (230, 20, 20)
        img[1010, 1700:1731] = (230, 20, 20)

        class S:
            def grab(self):
                return img
        cx, cy = mapview.camera_on_minimap(S(), img)
        self.assertAlmostEqual(cx, 1715, delta=1)
        self.assertAlmostEqual(cy, 1005, delta=1)
        self.assertIsNone(mapview.camera_on_minimap(S(), np.zeros((1080, 1920, 3), np.uint8)))

    def test_calibration_fits_two_axes_and_go_to_point_clicks_there(self):
        from hoi4_live.game import mapview
        idx = {"states": {"1": {"anchor": [1000, 500]}, "2": {"anchor": [4000, 1500]}, "3": {"anchor": [2500, 1000]}}}
        pos = {1: (1000, 500), 2: (4000, 1500), 3: (2500, 1000)}
        cam = {}

        class S:
            cfg = type("C", (), {"artifacts_dir": tempfile.mkdtemp()})()
            window = type("W", (), {"size": staticmethod(lambda: (1920, 1080))})()
            input = FakeInput()

            def grab(self):
                return None

            def park_cursor(self):
                pass
        s = S()
        orig_goto, orig_cam = mapview.go_to_id, mapview.camera_on_minimap
        try:
            mapview.go_to_id = lambda session, sid, expect=None: cam.update(p=pos[sid])
            mapview.camera_on_minimap = lambda session, img=None: (1580 + 0.046 * cam["p"][0], 980 + 0.047 * cam["p"][1])
            c = mapview.calibrate_minimap(s, idx, probes=(1, 2, 3))
            self.assertAlmostEqual(c["ax"], 0.046, places=4)
            self.assertAlmostEqual(c["by"], 980, delta=0.05)
            mapview.go_to_point(s, idx, 3000, 800, calibration=c)
        finally:
            mapview.go_to_id, mapview.camera_on_minimap = orig_goto, orig_cam
        self.assertEqual(s.input.calls[-1][:3], ("click", round(1580 + 0.046 * 3000), round(980 + 0.047 * 800)))


class ToolSchemas(unittest.TestCase):
    def test_new_arguments_are_in_the_schemas(self):
        import hoi4_live.tools  # noqa: F401
        from hoi4_live.tools import registry
        air = registry.get("hoi4_navy_air").schema["properties"]
        for k in ("plan", "types", "region_id", "region_at"):
            self.assertIn(k, air)
        self.assertEqual(registry.get("hoi4_front").schema["properties"]["action"]["enum"],
                         ["situation", "plan", "order"])
        self.assertIn("modifiers", registry.get("hoi4_click").schema["properties"])
        self.assertIn("minimap", registry.get("hoi4_map").schema["properties"]["action"]["enum"])


if __name__ == "__main__":
    unittest.main()


class SmallHelpers(unittest.TestCase):
    def test_stroke_changed_sees_an_arrow_of_any_colour(self):
        from hoi4_live.tools import military_tools as mt
        a = np.zeros((1080, 1920, 3), np.uint8)
        b = a.copy()
        b[500:520, 900:1100] = (230, 200, 40)          # a yellow arrow, no red in it
        self.assertTrue(mt._stroke_changed(a, b, (900, 510), (1100, 510)))
        self.assertFalse(mt._stroke_changed(a, a, (900, 510), (1100, 510)))

    def test_front_point_owner_names_match_loosely(self):
        from hoi4_live.game import armyview as av
        self.assertTrue(av._name_match("The Federal State of Austria", "Federal State of Austria"))
        self.assertFalse(av._name_match("Czechoslovak Republic", "Federal State of Austria"))

    def test_standing_defers_an_entry_when_an_event_window_is_open(self):
        import inspect
        from hoi4_live.state import standing
        src = inspect.getsource(standing.StandingOrders._fire)
        self.assertIn("event window", src)
        self.assertIn("no idle research slot", src)
