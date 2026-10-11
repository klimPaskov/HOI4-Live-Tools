"""Check upstream provenance, shipped license and the active portable adaptation."""
import ast
import hashlib
import json
from pathlib import Path
import struct
import unittest

from hoi4_live.engine.clausewitz.memory import ReadPrimitives
from hoi4_live.memory.winproc import ReadOnlyProcess as WindowsReader
from hoi4_live.platform.macos_memory import ReadOnlyProcess as MacReader
from hoi4_live.platform.linux_memory import ReadOnlyProcess as LinuxReader


ROOT = Path(__file__).resolve().parents[1]


class ClausewitzFoundationTests(unittest.TestCase):
    def test_pinned_sources_match_provenance_and_python39_syntax(self):
        source = ROOT / "third_party" / "clausewitz_mcp"
        manifest = json.loads((source / "UPSTREAM.json").read_text())
        self.assertEqual(manifest["commit"], "97c85437081904d5ad07bae1623ec58d1c0a5b9e")
        self.assertEqual(manifest["license"], "MIT")
        for name, entry in manifest["files"].items():
            with self.subTest(source=name):
                data = (source / name).read_bytes()
                self.assertEqual(hashlib.sha256(data).hexdigest(), entry["sha256"])
                self.assertFalse(entry["modified"])
                if name.endswith(".py"):
                    ast.parse(data, feature_version=(3, 9))
        self.assertFalse(list(source.rglob("*.pkl")))

    def test_license_is_preserved_for_packaged_derived_code(self):
        upstream = (ROOT / "third_party/clausewitz_mcp/LICENSE").read_bytes()
        derived = (ROOT / "hoi4_live/engine/clausewitz/LICENSE").read_bytes()
        self.assertEqual(derived, upstream)
        self.assertIn(b"Copyright (c) 2026 smerss", derived)
        self.assertIn("engine/clausewitz/LICENSE", (ROOT / "pyproject.toml").read_text())

    def test_all_platform_readers_use_same_upstream_primitives(self):
        for reader in (WindowsReader, MacReader, LinuxReader):
            self.assertTrue(issubclass(reader, ReadPrimitives))
            for name in ("u8", "u16", "u32", "i32", "u64", "i64", "f32", "f64"):
                self.assertIs(getattr(reader, name), getattr(ReadPrimitives, name))

    def test_typed_decoders_delegate_exact_reads_without_native_imports(self):
        class Fixture(ReadPrimitives):
            def read(self, address, size):
                self.request = address, size
                return self.data
        fixture = Fixture()
        for name, fmt, value in (("u8", "B", 255), ("u16", "H", 65535),
                                 ("u32", "I", 2 ** 32 - 1), ("i32", "i", -1234),
                                 ("u64", "Q", 2 ** 63 + 1), ("i64", "q", -2 ** 40),
                                 ("f32", "f", 1.25), ("f64", "d", -2.75)):
            fixture.data = struct.pack("<" + fmt, value)
            self.assertEqual(getattr(fixture, name)(0x123400), value)
            self.assertEqual(fixture.request, (0x123400, len(fixture.data)))
        self.assertFalse(hasattr(fixture, "write"))
        with self.assertRaises(NotImplementedError):
            ReadPrimitives().read(0x123400, 8)


if __name__ == "__main__":
    unittest.main()
