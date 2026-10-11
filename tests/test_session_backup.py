"""Config-session durability and ownership checks; every target is temporary."""

import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest import mock

from hoi4_live.game import session_backup as backup


class ConfigSessionTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="hoi4_config_session_")
        self.addCleanup(self.temporary.cleanup)
        root = Path(self.temporary.name).resolve()
        self.user = root / "user"
        self.user.mkdir()
        self.backups = root / "backups"
        self.original = {
            "dlc_load.json": b'\xef\xbb\xbf{\r\n "enabled_mods": ["mod/original.mod"],\r\n "disabled_dlcs": ["dlc/no.dlc"], "other": true\r\n}\r\n',
            "settings.txt": b'\xff\xfeold\x00settings\r\n',
            "pdx_settings.txt": b'odd bytes: \x80\x00\xff\n',
        }
        for name, data in self.original.items():
            (self.user / name).write_bytes(data)

    def begin(self):
        return backup.ConfigSession.begin(self.user, self.backups)

    def recover(self, session):
        return backup.ConfigSession.recover(session.session_dir, self.user, self.backups)

    def assert_original(self):
        for name, data in self.original.items():
            self.assertEqual((self.user / name).read_bytes(), data)

    def test_durable_baseline_and_byte_exact_restore(self):
        session = self.begin()
        saved = json.loads((session.session_dir / "manifest.json").read_text())
        for name, data in self.original.items():
            self.assertEqual((session.session_dir / name).read_bytes(), data)
            self.assertEqual(saved["baseline"][name]["sha256"], hashlib.sha256(data).hexdigest())
            self.assertTrue(saved["baseline"][name]["exists"])
        session.write_telemetry_mod("mod/telemetry.mod")
        playset = json.loads((self.user / "dlc_load.json").read_bytes())
        self.assertEqual(playset["enabled_mods"], ["mod/telemetry.mod"])
        self.assertEqual(playset["disabled_dlcs"], ["dlc/no.dlc"])
        self.assertTrue(playset["other"])
        result = self.recover(session).restore()
        self.assertFalse(result["conflicts"])
        self.assert_original()
        self.assertFalse(session.lock_path.exists())
        self.assertFalse(self.recover(session).restore()["conflicts"])

    def test_live_and_unverified_launch_block_restore(self):
        session = self.begin()
        session.write_telemetry_mod("mod/telemetry.mod")
        session.mark_game_starting("pid-123-created-456")
        with self.assertRaisesRegex(RuntimeError, "verified exited"):
            self.recover(session).restore()
        session.mark_game_started("pid-123-created-456")
        with self.assertRaisesRegex(RuntimeError, "verified exited"):
            session.restore()
        with self.assertRaisesRegex(RuntimeError, "identity"):
            session.mark_game_exited("some-other-game")
        with self.assertRaisesRegex(RuntimeError, "before launch"):
            session.write_telemetry_mod("mod/other.mod")
        session.mark_game_exited("pid-123-created-456")
        session.restore()
        self.assert_original()

    def test_explicit_owned_game_rewrites_and_external_conflict(self):
        session = self.begin()
        session.write_telemetry_mod("mod/telemetry.mod")
        session.mark_game_starting("owned-run")
        session.mark_game_started("owned-run")
        (self.user / "settings.txt").write_bytes(b"known owned game settings rewrite")
        known = session.file_hashes()["settings.txt"]
        session.mark_game_exited("owned-run", {"settings.txt": known})
        # A later user/launcher edit must not be mistaken for that game rewrite.
        (self.user / "settings.txt").write_bytes(b"external edit after exit")
        result = self.recover(session).restore()
        self.assertIn("settings.txt", result["conflicts"])
        self.assertEqual((self.user / "settings.txt").read_bytes(), b"external edit after exit")
        self.assertTrue(session.lock_path.exists())
        (self.user / "settings.txt").write_bytes(b"known owned game settings rewrite")
        self.assertFalse(self.recover(session).restore()["conflicts"])
        self.assert_original()

    def test_external_changes_are_not_blindly_overwritten(self):
        session = self.begin()
        (self.user / "dlc_load.json").write_bytes(b'{"enabled_mods": ["external"]}')
        with self.assertRaisesRegex(RuntimeError, "outside this session"):
            session.write_telemetry_mod("mod/telemetry.mod")
        result = session.restore()
        self.assertIn("dlc_load.json", result["conflicts"])
        self.assertEqual((self.user / "dlc_load.json").read_bytes(), b'{"enabled_mods": ["external"]}')

    def test_missing_baseline_removes_only_manager_created_file(self):
        (self.user / "dlc_load.json").unlink()
        (self.user / "settings.txt").unlink()
        session = self.begin()
        session.write_telemetry_mod("mod/telemetry.mod")
        session.mark_game_starting("owned")
        session.mark_game_started("owned")
        (self.user / "settings.txt").write_bytes(b"game-created-settings")
        session.mark_game_exited("owned", {"settings.txt": session.file_hashes()["settings.txt"]})
        result = session.restore()
        self.assertFalse((self.user / "dlc_load.json").exists())
        self.assertIn("settings.txt", result["conflicts"])
        self.assertEqual((self.user / "settings.txt").read_bytes(), b"game-created-settings")

    def test_interruption_before_config_write_is_recoverable(self):
        session = self.begin()
        atomic = backup._atomic_write

        def interrupt(path, data):
            if path.parent == self.user:
                raise KeyboardInterrupt("before config replacement")
            return atomic(path, data)

        with mock.patch.object(backup, "_atomic_write", side_effect=interrupt):
            with self.assertRaises(KeyboardInterrupt):
                session.write_telemetry_mod("mod/telemetry.mod")
        recovered = self.recover(session)
        self.assertIn("dlc_load.json", recovered.manifest["writes"])
        self.assertFalse(recovered.restore()["conflicts"])
        self.assert_original()

    def test_interruption_after_config_write_is_recoverable(self):
        session = self.begin()
        atomic = backup._atomic_write

        def interrupt(path, data):
            atomic(path, data)
            if path.parent == self.user:
                raise KeyboardInterrupt("after config replacement")

        with mock.patch.object(backup, "_atomic_write", side_effect=interrupt):
            with self.assertRaises(KeyboardInterrupt):
                session.write_telemetry_mod("mod/telemetry.mod")
        self.assertNotEqual((self.user / "dlc_load.json").read_bytes(), self.original["dlc_load.json"])
        self.assertFalse(self.recover(session).restore()["conflicts"])
        self.assert_original()

    def test_interrupted_restore_is_idempotent(self):
        session = self.begin()
        session.write_telemetry_mod("mod/telemetry.mod")
        with mock.patch.object(session, "_save", side_effect=KeyboardInterrupt("after restored file")):
            with self.assertRaises(KeyboardInterrupt):
                session.restore()
        self.assert_original()
        self.assertFalse(self.recover(session).restore()["conflicts"])
        self.assertFalse(session.lock_path.exists())

    def test_interrupted_lock_release_is_recoverable(self):
        session = self.begin()
        session.write_telemetry_mod("mod/telemetry.mod")
        unlink = Path.unlink

        def interrupt(path, *args, **kwargs):
            if path == session.lock_path:
                raise KeyboardInterrupt("after completed journal")
            return unlink(path, *args, **kwargs)

        with mock.patch.object(Path, "unlink", interrupt):
            with self.assertRaises(KeyboardInterrupt):
                session.restore()
        self.assert_original()
        self.assertTrue(session.lock_path.exists())
        self.assertFalse(self.recover(session).restore()["conflicts"])
        self.assertFalse(session.lock_path.exists())

    def test_concurrent_session_and_wrong_recovery_target_refused(self):
        session = self.begin()
        with self.assertRaisesRegex(RuntimeError, "already owns"):
            self.begin()
        other = self.user.parent / "other"
        other.mkdir()
        with self.assertRaisesRegex(ValueError, "identity"):
            backup.ConfigSession.recover(session.session_dir, other, self.backups)
        session.lock_path.write_text("different-owner")
        with self.assertRaisesRegex(RuntimeError, "no longer owns"):
            session.restore()

    def test_corrupt_baseline_is_refused_before_restore(self):
        session = self.begin()
        session.write_telemetry_mod("mod/telemetry.mod")
        (session.session_dir / "dlc_load.json").write_bytes(b"damaged backup")
        with self.assertRaisesRegex(ValueError, "damaged"):
            self.recover(session)
        self.assertEqual(json.loads((self.user / "dlc_load.json").read_bytes())["enabled_mods"],
                         ["mod/telemetry.mod"])

    def test_changed_target_symlink_is_refused(self):
        session = self.begin()
        session.write_telemetry_mod("mod/telemetry.mod")
        outside = self.user.parent / "outside.json"
        outside.write_bytes(b"outside bytes")
        target = self.user / "dlc_load.json"
        target.unlink()
        try:
            target.symlink_to(outside)
        except OSError as error:
            self.skipTest("Symlinks unavailable: %s" % error)
        result = session.restore()
        self.assertIn("dlc_load.json", result["conflicts"])
        self.assertEqual(outside.read_bytes(), b"outside bytes")


if __name__ == "__main__":
    unittest.main()
