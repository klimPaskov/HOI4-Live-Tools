import datetime
import os
import sys
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))
from scripts.hoi4facts.volunteers import window  # noqa: E402

DEFS = {"RECALL_VOLUNTEERS_AFTER_ARRIVAL_COOLDOWN": 180, "VOLUNTEERS_TRANSFER_SPEED": 14,
        "VOLUNTEERS_RECALL_DAYS_PER_UNIT": 2, "VOLUNTEERS_PER_TARGET_PROVINCE": 0.05,
        "VOLUNTEERS_PER_COUNTRY_ARMY": 0.05, "VOLUNTEERS_DIVISIONS_REQUIRED": 30, "SEND_VOLUNTEERS_AGAIN_COOLDOWN": 180}


class VolunteerWindow(unittest.TestCase):
    def test_window_and_cap_from_the_defines(self):
        w = window(DEFS, datetime.date(1939, 9, 1), 4, own_divisions=40, receiver_provinces=300)
        # recall 8 days + one transfer margin before the war; send before the lock, both transfers and the recall
        self.assertEqual(w["recall_from"], "1939.8.10")
        self.assertEqual(w["send_until"], "1939.1.28")
        self.assertEqual(w["cap"], 17)
        self.assertTrue(w["may_send"])

    def test_a_missing_define_is_an_error(self):
        d = dict(DEFS)
        del d["VOLUNTEERS_TRANSFER_SPEED"]
        with self.assertRaises(ValueError):
            window(d, datetime.date(1939, 9, 1), 4)


if __name__ == "__main__":
    unittest.main()
