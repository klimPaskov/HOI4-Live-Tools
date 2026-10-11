"""The PE/RTTI reader against the installed hoi4.exe (skipped when the game is not installed)."""

import os
import unittest

import tests.helpers  # noqa: F401

EXE = r"C:/Program Files (x86)/Steam/steamapps/common/Hearts of Iron IV/hoi4.exe"


@unittest.skipUnless(os.path.exists(EXE), "hoi4.exe not installed")
class PeRtti(unittest.TestCase):
    def test_classes_and_functions(self):
        from hoi4_live.memory import pe
        img, rt = pe.load_rtti(EXE, cache=False)
        for name in (".?AVCArmy@@", ".?AVCCountry@@", ".?AVCPauseGame@@", ".?AVCOrdersGroup@@"):
            self.assertIn(name, rt)
        commands = [n for n in rt if n.endswith("Command@@") and n.startswith(".?AVC")]
        self.assertGreater(len(commands), 300)
        starts, ends = img.function_table()
        self.assertGreater(len(starts), 100000)


if __name__ == "__main__":
    unittest.main()
