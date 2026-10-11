"""Top-bar values (OCR plus the game's font glyphs) and tooltip boxes, on a screenshot of the live game."""

import os
import sys
import unittest

import numpy as np
from PIL import Image

from helpers import FIXTURES

from hoi4_live import config
from hoi4_live.perception import glyphs, tooltip, topbar

SCREENS = os.path.join(FIXTURES, "screens")


def usa_1936():
    """1920x1080 client with the live top bar of a 1936 USA game (11 Jan 1936): PP 13, stability 94%, war
    support 5%, manpower 261.62K, factories 160, fuel 29 d, supply 100%, convoys 641, command power 20,
    experience 0/0/0, world tension 0%."""
    img = np.zeros((1080, 1920, 3), np.uint8)
    img[:100, :1000] = np.asarray(Image.open(os.path.join(SCREENS, "topleft_usa_1936.png")).convert("RGB"))
    img[:100, 1920 - 460:] = np.asarray(Image.open(os.path.join(SCREENS, "topright_usa_1936.png")).convert("RGB"))
    return img


class ParseTests(unittest.TestCase):
    def test_parse_and_normalise(self):
        self.assertEqual(topbar.parse_value("261.62K", "amount"), 261620)
        self.assertEqual(topbar.parse_value("94%", "percent"), 0.94)
        self.assertEqual(topbar.normalise("940/0", "percent"), "94%")
        self.assertEqual(topbar.normalise("00/0", "percent"), "0%")
        self.assertEqual(topbar.normalise("298", "fuel"), "29 d")
        self.assertEqual(topbar.parse_value("1.2M", "amount"), 1200000)
        self.assertEqual(topbar.parse_value("-12", "number"), -12)
        self.assertIsNone(topbar.parse_value("", "number"))


class GlyphTests(unittest.TestCase):
    def test_lone_digits_in_the_game_font(self):
        gd = config.get().game_dir
        fnt = glyphs.font(gd) if gd else None
        if fnt is None:
            self.skipTest("game fonts not installed")
        img = usa_1936()
        boxes = topbar.cell_boxes(1920, 1080)
        for cell in ("army_experience", "navy_experience", "air_experience"):
            x, y, w, h = boxes[cell]
            self.assertEqual(glyphs.read(img[y:y + h, x:x + w], fnt, charset=topbar.GLYPH_SETS["number"]), "0", cell)


@unittest.skipUnless(sys.platform == "win32", "expected values calibrated against Windows OCR")
class BarTests(unittest.TestCase):
    def test_read_bar(self):
        from hoi4_live.ocr.winocr import OcrHost
        ocr = OcrHost()
        try:
            r = topbar.read_bar(usa_1936(), ocr, game_dir=config.get().game_dir)
        finally:
            ocr.close()
        want = {"political_power": 13, "stability": 0.94, "war_support": 0.05, "manpower": 261620,
                "factories": 160, "fuel": 29.0, "supply": 1.0, "convoys": 641, "command_power": 20,
                "army_experience": 0, "navy_experience": 0, "air_experience": 0, "world_tension": 0.0}
        got = {k: r[k]["value"] for k in want}
        self.assertEqual(got, want)
        self.assertEqual(r["_missing"], [])


class TooltipBoxTests(unittest.TestCase):
    def test_box_near_cursor_and_animation_ignored(self):
        rng = np.random.default_rng(1)
        before = rng.integers(0, 60, (1080, 1920, 3)).astype(np.uint8)
        after = before.copy()
        after[38:553, 558:903] = 20                              # the tooltip
        after[600:700, 1000:1300][::7, ::7] = 255                # scattered map animation
        after[10:30, 1629:1761] = 200                            # the paused date box flashing
        self.assertEqual(tooltip.find_box(before, after, (535, 14)), [558, 38, 345, 515])
        self.assertIsNone(tooltip.find_box(before, before, (535, 14)))
        self.assertIsNone(tooltip.find_box(before, after, (1800, 1000), max_dist=300))

    def test_parse_lines(self):
        p = tooltip.parse_lines(["Fuel", "Current Fuel: 72.4K", "Daily Gain: 3.7K", "Army"])
        self.assertEqual(p["title"], "Fuel")
        self.assertEqual(p["values"], {"Current Fuel": "72.4K", "Daily Gain": "3.7K"})


if __name__ == "__main__":
    unittest.main()
