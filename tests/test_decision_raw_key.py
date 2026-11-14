"""A decision without localisation shows its raw key, which OCR misreads (live 1.19.4.2: livetest_missing_loc was read
as tivetest_missing_toc). find_row matches such a name with a lower threshold and keeps the normal one for names."""

import unittest
from unittest import mock

import helpers  # noqa: F401
from hoi4_live.game import decisionsview as dv


def rows(*names):
    return [{"kind": "decision", "name": n, "y": 100 + 40 * i} for i, n in enumerate(names)]


class RawKeyTests(unittest.TestCase):
    def find(self, name, screen_names):
        with mock.patch.object(dv, "read_visible", return_value=rows(*screen_names)), \
                mock.patch.object(dv, "scroll"):
            return dv.find_row(mock.Mock(), name)

    def test_a_misread_raw_key_is_found(self):
        row = self.find("livetest_missing_loc", ["Live test decision", "tivetest_missing_toc"])
        self.assertEqual(row["name"], "tivetest_missing_toc")

    def test_a_badly_misread_raw_key_is_still_found(self):
        row = self.find("livetest_missing_loc", ["Blocked live test decision", "tivetest_rnissing_toc"])
        self.assertEqual(row["name"], "tivetest_rnissing_toc")

    def test_an_unrelated_raw_key_is_not(self):
        self.assertIsNone(self.find("livetest_missing_loc", ["DEBUG_activate_missiolinis_system", "Live test decision"]))

    def test_a_localised_name_keeps_the_normal_threshold(self):
        self.assertIsNone(self.find("Abandon the Naval Treaties", ["Second London Naval Conference"]))


if __name__ == "__main__":
    unittest.main()
