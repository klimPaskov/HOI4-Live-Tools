"""The installer's client registrations, the payload list and the release archive."""

import hashlib
import json
import os
import shutil
import sys
import tempfile
import unittest
import zipfile

import helpers  # noqa: F401

sys.path.insert(0, os.path.join(helpers.ROOT, "scripts"))
import install  # noqa: E402
import build_release  # noqa: E402

CMD = r"C:\example\hoi4-live-tools\app\hoi4-live-tools.cmd"
CMD2 = r"D:\tools\app\hoi4-live-tools.cmd"


class RegistrationTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="hoi4live_inst_")
        self.addCleanup(shutil.rmtree, self.tmp, True)

    def path(self, name):
        return os.path.join(self.tmp, name)

    def read(self, name):
        with open(self.path(name), encoding="utf-8") as f:
            return f.read()

    def test_claude_new_file_and_other_servers_kept(self):
        with open(self.path("c.json"), "w", encoding="utf-8") as f:
            json.dump({"mcpServers": {"other": {"command": "x"}}, "projects": {"a": 1}}, f)
        r = install.register_json("claude", self.path("c.json"), CMD)
        self.assertEqual(r["status"], "registered")
        data = json.loads(self.read("c.json"))
        self.assertEqual(data["mcpServers"]["other"], {"command": "x"})
        self.assertEqual(data["projects"], {"a": 1})
        self.assertEqual(data["mcpServers"]["hoi4-live"]["command"], CMD)
        self.assertEqual(data["mcpServers"]["hoi4-live"]["type"], "stdio")
        self.assertTrue(os.path.exists(self.path("c.json.hoi4live.bak")))

    def test_claude_idempotent_then_update_to_new_path(self):
        install.register_json("claude", self.path("c.json"), CMD)
        self.assertEqual(install.register_json("claude", self.path("c.json"), CMD)["status"], "unchanged")
        self.assertEqual(install.register_json("claude", self.path("c.json"), CMD2)["status"], "updated")
        self.assertEqual(json.loads(self.read("c.json"))["mcpServers"]["hoi4-live"]["command"], CMD2)

    def test_foreign_entry_is_a_conflict_and_left_alone(self):
        with open(self.path("c.json"), "w", encoding="utf-8") as f:
            json.dump({"mcpServers": {"hoi4-live": {"command": "node", "args": ["x.js"]}}}, f)
        before = self.read("c.json")
        r = install.register_json("claude", self.path("c.json"), CMD)
        self.assertEqual(r["status"], "conflict")
        self.assertEqual(self.read("c.json"), before)
        self.assertEqual(install.unregister_json("claude", self.path("c.json"))["status"], "conflict")

    def test_unregister_removes_only_ours(self):
        install.register_json("cursor", self.path("m.json"), CMD)
        self.assertEqual(install.unregister_json("cursor", self.path("m.json"))["status"], "removed")
        self.assertEqual(json.loads(self.read("m.json")), {"mcpServers": {}})
        self.assertEqual(install.unregister_json("cursor", self.path("m.json"))["status"], "absent")
        self.assertEqual(install.unregister_json("cursor", self.path("missing.json"))["status"], "absent")

    def test_opencode_uses_its_own_shape(self):
        install.register_json("opencode", self.path("o.json"), CMD)
        entry = json.loads(self.read("o.json"))["mcp"]["hoi4-live"]
        self.assertEqual(entry, {"type": "local", "command": [CMD], "enabled": True})
        self.assertEqual(install.register_json("opencode", self.path("o.json"), CMD)["status"], "unchanged")

    def test_invalid_json_is_reported_not_overwritten(self):
        with open(self.path(".claude.json"), "w", encoding="utf-8") as f:
            f.write("{not json")
        results = install.register_clients(["claude"], self.tmp, home=self.tmp)
        self.assertEqual(results[0]["status"], "failed")
        self.assertEqual(self.read(".claude.json"), "{not json")

    def test_codex_block_appended_replaced_and_removed(self):
        original = 'model = "x"\n\n[mcp_servers.foo]\ncommand = "bar"\n'
        with open(self.path("config.toml"), "w", encoding="utf-8") as f:
            f.write(original)
        self.assertEqual(install.register_codex(self.path("config.toml"), CMD)["status"], "registered")
        text = self.read("config.toml")
        self.assertTrue(text.startswith(original))
        self.assertIn("[mcp_servers.hoi4-live]", text)
        self.assertIn("tool_timeout_sec = 1800", text)
        self.assertEqual(install.register_codex(self.path("config.toml"), CMD)["status"], "unchanged")
        self.assertEqual(install.register_codex(self.path("config.toml"), CMD2)["status"], "updated")
        self.assertEqual(self.read("config.toml").count("[mcp_servers.hoi4-live]"), 1)
        self.assertIn(CMD2, self.read("config.toml"))
        self.assertEqual(install.unregister_codex(self.path("config.toml"))["status"], "removed")
        self.assertEqual(self.read("config.toml").strip(), original.strip())

    def test_codex_manual_table_is_a_conflict(self):
        with open(self.path("config.toml"), "w", encoding="utf-8") as f:
            f.write("[mcp_servers.hoi4-live]\ncommand = 'old'\n")
        self.assertEqual(install.register_codex(self.path("config.toml"), CMD)["status"], "conflict")
        self.assertEqual(self.read("config.toml"), "[mcp_servers.hoi4-live]\ncommand = 'old'\n")

    def test_codex_creates_the_file(self):
        path = os.path.join(self.tmp, "sub", "config.toml")
        self.assertEqual(install.register_codex(path, CMD)["status"], "registered")
        self.assertTrue(self.read(os.path.join("sub", "config.toml")).startswith(install.MARK_BEGIN))

    def test_resolve_clients(self):
        self.assertEqual(install.resolve_clients("none"), [])
        self.assertEqual(install.resolve_clients("claude, codex"), ["claude", "codex"])
        self.assertEqual(install.resolve_clients("all"), list(install.CLIENTS))
        with self.assertRaises(install.InstallError):
            install.resolve_clients("vscode")

    def test_detect_clients_by_config_folders(self):
        os.makedirs(os.path.join(self.tmp, ".codex"))
        os.makedirs(os.path.join(self.tmp, ".config", "opencode"))
        found = install.detect_clients(self.tmp)
        self.assertIn("codex", found)
        self.assertIn("opencode", found)
        self.assertNotIn("cursor", found)

    def test_is_ours(self):
        self.assertTrue(install.is_ours(CMD))
        self.assertTrue(install.is_ours([CMD]))
        self.assertFalse(install.is_ours("node"))
        self.assertFalse(install.is_ours(None))


class PayloadAndReleaseTests(unittest.TestCase):
    def test_payload_contents(self):
        names = [n for n, _ in install.payload_files(helpers.ROOT)]
        for expected in ("install.py", "hoi4-live-tools.cmd", "requirements.txt", "LICENSE", "hoi4_live/__init__.py",
                         "hoi4_live/ocr/ocr_host.ps1", "skills/hoi4-live-campaign/SKILL.md"):
            self.assertIn(expected, names)
        self.assertFalse([n for n in names if "__pycache__" in n or n.endswith(".pyc") or n.startswith("tests/")
                          or n.startswith("docs/evidence") or n.startswith("artifacts")])
        self.assertEqual(names, sorted(names))

    def test_versions_agree(self):
        versions = build_release.declared_versions()
        self.assertEqual(len(set(versions.values())), 1, versions)

    def test_requirements_match_pyproject(self):
        with open(os.path.join(helpers.ROOT, "requirements.txt"), encoding="utf-8") as f:
            required = {line.strip() for line in f if line.strip()}
        with open(os.path.join(helpers.ROOT, "pyproject.toml"), encoding="utf-8") as f:
            text = f.read()
        for requirement in required:
            self.assertIn(f'"{requirement}"', text)

    def test_build_is_deterministic_and_complete(self):
        tmp = tempfile.mkdtemp(prefix="hoi4live_rel_")
        self.addCleanup(shutil.rmtree, tmp, True)
        a, digest_a, count = build_release.build(os.path.join(tmp, "a"), allow_dirty=True)
        b, digest_b, _ = build_release.build(os.path.join(tmp, "b"), allow_dirty=True)
        self.assertEqual(digest_a, digest_b)
        with open(a, "rb") as f:
            self.assertEqual(hashlib.sha256(f.read()).hexdigest(), digest_a)
        with open(os.path.join(tmp, "a", "SHA256SUMS.txt"), encoding="utf-8") as f:
            self.assertEqual(f.read(), f"{digest_a}  {os.path.basename(a)}\n")
        with zipfile.ZipFile(a) as z:
            names = z.namelist()
            top = names[0].split("/")[0]
            self.assertTrue(all(n.startswith(top + "/") for n in names))
            self.assertEqual(len(names), count)
            self.assertIn(f"{top}/install.py", names)
            self.assertNotIn(b"\r\n", z.read(f"{top}/install.py"))
            self.assertIn(b"\r\n", z.read(f"{top}/hoi4-live-tools.cmd"))
            self.assertTrue(all(i.date_time == build_release.FIXED_TIME for i in z.infolist()))


if __name__ == "__main__":
    unittest.main()
