"""Phase 3 screen readers on screenshots of the live game (HOI4 1.19.2, 1920x1080, gui_scale 1): text colours,
label matching, the tech tree calibration, the advisor list, the decisions panel and the diplomacy actions.
The OCR tests need the Windows OCR host; the calibration test needs the game files."""

import os
import sys
import unittest

import numpy as np
from PIL import Image

from helpers import FIXTURES, needs_fixture

from hoi4_live import config
from hoi4_live.game import (decisionsview, diplomacyview, focusview, panels, politicsview, productionview,
                            researchview)

SCREENS = os.path.join(FIXTURES, "screens")


def canvas(*crops):
    """A 1920x1080 client with crops of a live screenshot put back where they were taken: (file, x, y)..."""
    img = np.zeros((1080, 1920, 3), np.uint8)
    for name, x, y in crops:
        crop = np.asarray(Image.open(os.path.join(SCREENS, name)).convert("RGB"))
        img[y:y + crop.shape[0], x:x + crop.shape[1]] = crop
    return img


class FakeSession:
    """Just enough of a Session for the readers: OCR, the window size and a fixed screenshot."""

    def __init__(self, img, ocr):
        self.img = img
        self.ocr = ocr
        self.cfg = config.get()

        class W:
            @staticmethod
            def size():
                return (img.shape[1], img.shape[0])
        self.window = W()

    def grab(self, focus=None):
        return self.img


class ColourAndMatchTests(unittest.TestCase):
    def test_text_colour(self):
        img = np.zeros((40, 120, 3), np.uint8)
        img[5:15, 5:40] = (222, 86, 70)          # unaffordable cost red
        img[5:15, 45:80] = (90, 200, 90)         # met requirement green
        img[20:30, 5:40] = (230, 230, 230)
        self.assertEqual(panels.text_colour(img, [0, 0, 42, 18]), "red")
        self.assertEqual(panels.text_colour(img, [44, 0, 40, 18]), "green")
        self.assertEqual(panels.text_colour(img, [0, 18, 42, 14]), "white")
        self.assertEqual(panels.text_colour(img, [90, 20, 20, 15]), "grey")

    def test_similar_and_titles(self):
        self.assertEqual(panels.similar("M1 Garand", "MI Garand"), 1.0)          # OCR look-alikes
        self.assertGreater(panels.similar("Smalt Lobbying Effort", "Small Lobbying Effort"), 0.9)
        self.assertLess(panels.similar("War Plan Green", "War Plan Gray"), 0.95)
        self.assertEqual(focusview.title_score("WAR PLANS DIVISION", "War Plans Division"), 1.0)
        self.assertLess(focusview.title_score("AIR WAR PLANS DIVISION", "War Plans Division"), 0.85)

    def test_save_list_dates(self):
        from hoi4_live.game import menu
        rx = menu._date_rx("1936.1.1.12")
        self.assertTrue(rx.search("12:00, 1 January, 1936"))
        self.assertTrue(rx.search("12:00, 1 January,"))                 # the year wrapped to the next line
        self.assertFalse(rx.search("12:00, 11 January, 1936"))
        self.assertFalse(rx.search("20:00, 1 January, 1936"))
        self.assertTrue(menu.SAVE_DATE_RX.search("13:00, 2 January,"))

    def test_merge_page(self):
        doc = []
        a = [{"kind": "category", "name": "Congress"}, {"kind": "decision", "name": "A"},
             {"kind": "decision", "name": "B"}]
        b = [{"kind": "decision", "name": "B"}, {"kind": "category", "name": "Naval"},
             {"kind": "decision", "name": "C"}]
        decisionsview.merge_page(doc, a)
        decisionsview.merge_page(doc, b)
        self.assertEqual([r["name"] for r in doc], ["Congress", "A", "B", "Naval", "C"])
        decisionsview.merge_page(doc, [{"kind": "decision", "name": "Z"}])          # no overlap: appended
        self.assertEqual(doc[-1]["name"], "Z")

    def test_column_lines(self):
        class L:
            def __init__(self, words):
                self.words = words
        res = type("R", (), {})()
        res.lines = [L([{"text": "Harold", "box": [626, 208, 50, 11]}, {"text": "Ickes", "box": [680, 208, 40, 11]},
                        {"text": "Robert", "box": [858, 208, 45, 11]}, {"text": "Taft", "box": [906, 208, 30, 11]}])]
        out = politicsview._column_lines(res, [(555, 780), (785, 1010)])
        self.assertEqual([l.text for l in out], ["Harold Ickes", "Robert Taft"])


class CalibrationTests(unittest.TestCase):
    @needs_fixture("screens", "techtree_industry_1080.jpg")
    def test_industry_folder_offset(self):
        from hoi4_live.gamedata import tech
        from hoi4_live.telemetry.modgen import mod_paths
        cfg = config.get()
        if not cfg.game_dir or not os.path.isdir(os.path.join(cfg.game_dir, "common", "technologies")):
            self.skipTest("game files not found")
        idx = tech.index(cfg.game_dir, mod_paths(cfg.user_dir))
        if "industry_folder" not in idx.get("layout", {}):
            self.skipTest("no industry folder layout")
        img = np.asarray(Image.open(os.path.join(SCREENS, "techtree_industry_1080.jpg")).convert("RGB"))
        ox, oy, score, n = researchview.calibrate(img, idx, "industry_folder")
        # measured live: the industry folder's grid origin sits 41 px lower than the infantry folder's
        self.assertLessEqual(abs(ox - 12), 6)
        self.assertLessEqual(abs(oy - 143), 4)
        cx, cy = tech.tile_offset(idx, "basic_machine_tools", "industry_folder")
        self.assertLessEqual(abs(ox + cx - 270), 8)          # the Basic Machine Tools tile on screen
        self.assertLessEqual(abs(oy + cy - 350), 8)


@unittest.skipUnless(sys.platform == "win32", "screenshot text and word boxes calibrated against Windows OCR")
class OcrViewTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        from hoi4_live.ocr.winocr import OcrHost
        cls.ocr = OcrHost()

    @classmethod
    def tearDownClass(cls):
        cls.ocr.close()

    @needs_fixture("screens", "techtree_industry_1080.jpg")
    def test_tree_open_on_the_tech_tree(self):
        for name, want in (("techtree_industry_1080.jpg", True),):
            img = np.asarray(Image.open(os.path.join(SCREENS, name)).convert("RGB"))
            self.assertEqual(researchview.tree_open(FakeSession(img, self.ocr)), want, name)

    def test_advisor_cards(self):
        s = FakeSession(canvas(("advisor_list_540_80.png", 540, 80)), self.ocr)
        self.assertRegex(politicsview.list_title(s), r"(?i)political\s*advisor")
        cards = {c["name"]: c for c in politicsview.read_cards(s)}
        names = " ".join(cards)
        for n in ("Cordell Hull", "Henry Stimson", "Earl Browder", "Joseph Patterson"):
            self.assertIn(n, names)
        ickes = next(c for n, c in cards.items() if "Ickes" in n)
        self.assertEqual(ickes["cost"], 150)
        self.assertTrue(ickes["cannot_afford"])              # 150 in red with 1 political power
        self.assertFalse(ickes["condition_unmet"])
        self.assertTrue(cards["Earl Browder"]["condition_unmet"])     # red X
        self.assertTrue(cards["Joseph Patterson"]["condition_unmet"])
        self.assertGreater(cards["Cordell Hull"]["center"][0], 780)   # right column

    def test_production_lines(self):
        # Mexico's list after adding a line: the lines short of equipment carry a warning triangle after the number,
        # which OCR reads as "4a Mondragón M94" / "2 a Towed Artillery"
        s = FakeSession(canvas(("production_lines_mex_0_362.png", 0, 362)), self.ocr)
        lines = productionview.read_lines(s)
        self.assertEqual([l["number"] for l in lines], [1, 2, 3, 4])
        self.assertEqual([l["factories"] for l in lines], [1, 1, 2, 1])
        self.assertGreaterEqual(panels.similar(lines[1]["name"], "Towed Artillery"), 0.95)
        self.assertGreaterEqual(panels.similar(lines[3]["name"], "Mondragón M94"), 0.9)
        self.assertEqual(lines[3]["type"], "Basic Infantry Equipment")

    def test_decision_rows(self):
        s = FakeSession(canvas(("decisions_panel_crop.jpg", 0, 80)), self.ocr)
        rows = decisionsview.read_visible(s, categories=["Congress", "Naval Treaties", "Propaganda Efforts",
                                                         "Political Actions"])
        cats = [r["name"] for r in rows if r["kind"] == "category"]
        self.assertIn("Naval Treaties", cats)
        dec = {panels.norm(r["name"]): r for r in rows if r["kind"] == "decision"}
        sm = dec[panels.norm("Special Measures")] if panels.norm("Special Measures") in dec else \
            next(r for k, r in dec.items() if panels.norm("measures") in k)
        self.assertEqual((sm["cost"], sm["tint"], sm["cost_colour"]), (50, "green", "red"))
        lob = next(r for k, r in dec.items() if panels.norm("lobbying") in k)
        self.assertEqual(lob["tint"], "dark")
        conf = next(r for k, r in dec.items() if panels.norm("london") in k)
        self.assertEqual(conf["days"], 115)

    def test_notice_window(self):
        s = FakeSession(canvas(("notice_focus_completed_730_375.png", 730, 375)), self.ocr)
        n = panels.find_notice(s)
        self.assertIsNotNone(n)
        self.assertIn("COMPLETED", n["title"].upper())
        self.assertLess(abs(n["ok"][0] - 1127), 8)
        self.assertLess(abs(n["ok"][1] - 596), 8)
        s2 = FakeSession(canvas(("advisor_list_540_80.png", 540, 80)), self.ocr)
        self.assertIsNone(panels.find_notice(s2))        # a list window is not a notice

    def test_diplomacy_actions(self):
        s = FakeSession(canvas(("diplomacy_header_230_120.png", 230, 120), ("diplomacy_actions_270_500.png", 270, 500)), self.ocr)
        hd = diplomacyview.read_header(s)
        self.assertIn("Canad", hd["name"])
        acts = {panels.norm(a["name"]): a for a in diplomacyview.read_actions(s)}
        imp = next(a for k, a in acts.items() if panels.norm("improve relations") in k)
        self.assertIn("10", imp["cost"] or "")
        mil = next(a for k, a in acts.items() if panels.norm("military access") in k and k.startswith("ask"))
        self.assertEqual(mil["mark"], "blocked")
        lic = next(a for k, a in acts.items() if panels.norm("licenses") in k)
        self.assertEqual(lic["mark"], "ok")


if __name__ == "__main__":
    unittest.main()
