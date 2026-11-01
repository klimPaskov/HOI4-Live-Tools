"""glyphs.read with only small glyphs on screen (no digit to fix the text line). Seen live on 1.19.4.1 in the
production view: the reader raised UnboundLocalError on `base`."""
import unittest
from types import SimpleNamespace

import numpy as np

from hoi4_live.perception import glyphs


class SmallGlyphsOnlyTests(unittest.TestCase):
    def test_two_dashes_and_no_digits(self):
        dash = np.zeros((3, 7), np.float32)
        dash[1, :] = 1.0
        dash[0, :] = dash[2, :] = 0.3
        fnt = SimpleNamespace(templates={"-": {"t": dash, "top": 0, "adv": 8}})
        crop = np.full((20, 40, 3), 20, np.uint8)
        for x0 in (5, 18):
            crop[9:12, x0:x0 + 7] = (dash * 230 + 20).astype(np.uint8)[..., None]
        self.assertEqual(glyphs.read(crop, fnt, charset="-"), "- -")


if __name__ == "__main__":
    unittest.main()
