"""The DLC prepare step (decision R-6): copy launcher-v2.sqlite and dlc_signature, back up, restore. Temp folders only."""

import hashlib
import os
import tempfile
import unittest
from unittest import mock

import helpers  # noqa: F401  (puts the repository on sys.path)
from hoi4_live import config
from hoi4_live.errors import ModeRefused, ToolError
from hoi4_live.game import dlc_prepare
from hoi4_live.session import Session
from hoi4_live.tools import dispatch, session_tools

SQLITE = b"SQLite format 3\x00" + b"\x01" * 64
SIG = b"0123456789abcdef0123456789abcdef"


def sha(data):
    return hashlib.sha256(data).hexdigest()


class Folders(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="hoi4live_dlc_")
        self.addCleanup(self.tmp.cleanup)
        root = os.path.realpath(self.tmp.name)
        self.source = os.path.join(root, "normal")
        self.target = os.path.join(root, "scratch")
        self.backups = os.path.join(root, "backups")
        os.makedirs(self.source)
        os.makedirs(self.target)
        self.put(self.source, "launcher-v2.sqlite", SQLITE)
        self.put(self.source, "dlc_signature", SIG)

    def put(self, folder, name, data):
        with open(os.path.join(folder, name), "wb") as f:
            f.write(data)

    def get(self, folder, name):
        try:
            with open(os.path.join(folder, name), "rb") as f:
                return f.read()
        except FileNotFoundError:
            return None

    def apply(self):
        return dlc_prepare.apply(self.target, self.source, self.backups)

    def restore(self, **kw):
        return dlc_prepare.restore(self.target, self.backups, **kw)


class ApplyAndRestore(Folders):
    def test_copies_into_an_empty_folder_and_removes_the_copies_afterwards(self):
        rep = self.apply()
        self.assertEqual(sorted(rep["copied"]), sorted(dlc_prepare.DLC_FILES))
        self.assertEqual(self.get(self.target, "launcher-v2.sqlite"), SQLITE)
        self.assertEqual(self.get(self.target, "dlc_signature"), SIG)
        out = self.restore()
        self.assertEqual(sorted(out["removed"]), sorted(dlc_prepare.DLC_FILES))
        self.assertFalse(out["conflicts"])
        self.assertIsNone(self.get(self.target, "launcher-v2.sqlite"))
        self.assertIsNone(self.get(self.target, "dlc_signature"))

    def test_existing_files_are_backed_up_and_come_back_byte_for_byte(self):
        old_db, old_sig = SQLITE + b"older", b"\x00odd bytes\xff"
        self.put(self.target, "launcher-v2.sqlite", old_db)
        self.put(self.target, "dlc_signature", old_sig)
        self.apply()
        self.assertEqual(self.get(self.target, "launcher-v2.sqlite"), SQLITE)
        journal = dlc_prepare.journal_dir(self.backups, self.target)
        self.assertEqual(self.get(journal, "launcher-v2.sqlite"), old_db)      # the backup is on disk before use
        out = self.restore()
        self.assertEqual(sorted(out["restored"]), sorted(dlc_prepare.DLC_FILES))
        self.assertEqual(self.get(self.target, "launcher-v2.sqlite"), old_db)
        self.assertEqual(self.get(self.target, "dlc_signature"), old_sig)
        self.assertEqual(os.listdir(journal), ["manifest.json"])               # backups are dropped once restored

    def test_apply_twice_is_a_no_op_and_restore_still_returns_the_original(self):
        self.put(self.target, "dlc_signature", b"original")
        self.apply()
        again = self.apply()
        self.assertEqual(again["copied"], [])
        self.restore()
        self.assertEqual(self.get(self.target, "dlc_signature"), b"original")
        self.assertIsNone(self.get(self.target, "launcher-v2.sqlite"))

    def test_identical_files_need_no_backup_and_stay_after_restore(self):
        self.put(self.target, "launcher-v2.sqlite", SQLITE)
        rep = self.apply()
        self.assertEqual(rep["copied"], ["dlc_signature"])
        self.restore()
        self.assertEqual(self.get(self.target, "launcher-v2.sqlite"), SQLITE)
        self.assertIsNone(self.get(self.target, "dlc_signature"))

    def test_changed_source_while_active_is_refused(self):
        self.apply()
        self.put(self.source, "dlc_signature", b"another signature file")
        with self.assertRaises(ToolError) as cm:
            self.apply()
        self.assertEqual(cm.exception.details["error"], "dlc_prepare_active")
        self.restore()
        self.assertEqual(sorted(self.apply()["copied"]), sorted(dlc_prepare.DLC_FILES))

    def test_restore_without_a_prepare_does_nothing(self):
        self.assertTrue(self.restore()["nothing_to_restore"])

    def test_a_missing_target_folder_is_created(self):
        os.rmdir(self.target)
        rep = self.apply()
        self.assertTrue(rep["created_dir"])
        self.assertEqual(self.get(self.target, "dlc_signature"), SIG)


class SourceChecks(Folders):
    def test_missing_or_empty_file_in_the_source_is_refused(self):
        os.remove(os.path.join(self.source, "dlc_signature"))
        with self.assertRaises(ToolError) as cm:
            self.apply()
        self.assertEqual(cm.exception.details["error"], "dlc_source_incomplete")
        self.put(self.source, "dlc_signature", b"")
        with self.assertRaises(ToolError):
            self.apply()
        self.assertIsNone(self.get(self.target, "launcher-v2.sqlite"))     # nothing half copied

    def test_a_file_that_is_not_sqlite_is_not_copied(self):
        self.put(self.source, "launcher-v2.sqlite", b"<html>not a database</html>")
        with self.assertRaises(ToolError) as cm:
            self.apply()
        self.assertEqual(cm.exception.details["error"], "dlc_source_damaged")

    def test_missing_source_folder(self):
        with self.assertRaises(ToolError) as cm:
            dlc_prepare.apply(self.target, os.path.join(self.tmp.name, "nope"), self.backups)
        self.assertEqual(cm.exception.details["error"], "dlc_source_missing")

    def test_the_normal_folder_needs_no_copy_and_no_restore(self):
        rep = dlc_prepare.apply(self.source, self.source, self.backups)
        self.assertTrue(rep["same_folder"])
        self.assertEqual(rep["copied"], [])
        self.assertTrue(dlc_prepare.restore(self.source, self.backups)["nothing_to_restore"])
        self.assertEqual(self.get(self.source, "launcher-v2.sqlite"), SQLITE)

    def test_an_unmerged_wal_file_is_reported(self):
        self.put(self.source, "launcher-v2.sqlite-wal", b"pending")
        self.assertTrue(self.apply()["warnings"])


class Conflicts(Folders):
    def test_a_file_changed_after_the_copy_is_left_alone(self):
        self.apply()
        self.put(self.target, "launcher-v2.sqlite", SQLITE + b"rewritten by the game")
        out = self.restore()
        self.assertIn("launcher-v2.sqlite", out["conflicts"])
        self.assertEqual(self.get(self.target, "launcher-v2.sqlite"), SQLITE + b"rewritten by the game")
        self.assertIsNone(self.get(self.target, "dlc_signature"))          # the untouched copy is still removed
        self.assertEqual(dlc_prepare.status(self.target, self.source, self.backups)["journal"]["state"], "applied")

    def test_force_restores_the_original_and_keeps_the_changed_file(self):
        self.put(self.target, "launcher-v2.sqlite", SQLITE + b"mine")
        self.apply()
        self.put(self.target, "launcher-v2.sqlite", SQLITE + b"rewritten by the game")
        out = self.restore(force=True)
        self.assertFalse(out["conflicts"])
        self.assertEqual(self.get(self.target, "launcher-v2.sqlite"), SQLITE + b"mine")
        kept = out["after_run_copies"]
        self.assertEqual(len(kept), 1)
        with open(kept[0], "rb") as f:
            self.assertEqual(f.read(), SQLITE + b"rewritten by the game")

    def test_a_file_that_was_already_identical_still_has_its_backup(self):
        # review finding: no backup was written when the target equalled the source, so a later change could never
        # be restored (even with force) and the journal wedged
        self.put(self.target, "launcher-v2.sqlite", SQLITE)
        self.apply()
        self.put(self.target, "launcher-v2.sqlite", SQLITE + b"rewritten by the game")
        out = self.restore()
        self.assertIn("launcher-v2.sqlite", out["conflicts"])
        out = self.restore(force=True)
        self.assertFalse(out["conflicts"])
        self.assertEqual(self.get(self.target, "launcher-v2.sqlite"), SQLITE)
        self.assertEqual(self.apply()["copied"], ["dlc_signature"])      # not wedged: a new prepare works again

    def test_an_identical_file_deleted_after_the_copy_comes_back(self):
        self.put(self.target, "dlc_signature", SIG)
        self.apply()
        os.remove(os.path.join(self.target, "dlc_signature"))
        self.assertFalse(self.restore()["conflicts"])
        self.assertEqual(self.get(self.target, "dlc_signature"), SIG)

    def test_a_file_deleted_after_the_copy_does_not_break_restore(self):
        self.put(self.target, "dlc_signature", b"original")
        self.apply()
        os.remove(os.path.join(self.target, "dlc_signature"))
        out = self.restore()
        self.assertFalse(out["conflicts"])
        self.assertEqual(self.get(self.target, "dlc_signature"), b"original")

    def test_a_damaged_backup_is_never_written_back(self):
        self.put(self.target, "dlc_signature", b"original")
        self.apply()
        journal = dlc_prepare.journal_dir(self.backups, self.target)
        self.put(journal, "dlc_signature", b"tampered")
        out = self.restore()
        self.assertIn("dlc_signature", out["conflicts"])
        self.assertEqual(self.get(self.target, "dlc_signature"), SIG)

    def test_a_linked_target_is_refused(self):
        real = os.path.join(self.tmp.name, "real.bin")
        self.put(self.tmp.name, "real.bin", b"x")
        try:
            os.symlink(real, os.path.join(self.target, "dlc_signature"))
        except (OSError, NotImplementedError):
            self.skipTest("symbolic links are not available here")
        with self.assertRaises(ToolError) as cm:
            self.apply()
        self.assertEqual(cm.exception.details["error"], "dlc_link")
        self.assertEqual(self.get(self.tmp.name, "real.bin"), b"x")


class Status(Folders):
    def test_reports_the_need_and_the_journal(self):
        st = dlc_prepare.status(self.target, self.source, self.backups)
        self.assertTrue(st["needs_prepare"])
        self.assertIsNone(st["journal"])
        self.apply()
        st = dlc_prepare.status(self.target, self.source, self.backups)
        self.assertFalse(st["needs_prepare"])
        self.assertEqual(st["journal"]["state"], "applied")
        self.restore()
        self.assertEqual(dlc_prepare.status(self.target, self.source, self.backups)["journal"]["state"], "restored")

    def test_the_normal_folder_without_the_files_says_what_to_do(self):
        os.remove(os.path.join(self.source, "dlc_signature"))
        st = dlc_prepare.status(self.source, self.source, self.backups)
        self.assertTrue(st["needs_prepare"])
        self.assertIn("launcher", st["reason"])


class ToolLevel(Folders):
    """hoi4_prepare, hoi4_launch prepare_dlc and hoi4_shutdown on a real Session with temp folders."""

    def make_session(self, mode="test"):
        cfg = config.load(overrides={"mode": mode, "user_dir": self.target, "dlc_source_dir": self.source,
                                     "lock": {"path": os.path.join(self.tmp.name, "t.lock"), "wait_seconds": 1},
                                     "artifacts_dir": os.path.join(self.tmp.name, "artifacts")})
        session = Session(cfg)
        self.addCleanup(session.shutdown)
        return session

    def patches(self, steam=(), game=()):
        return (mock.patch.object(session_tools.process, "steam_processes", return_value=list(steam)),
                mock.patch.object(session_tools.process, "running_pids", return_value=list(game)))

    def call(self, session, **args):
        out = dispatch.call(session, "hoi4_prepare", args)
        return out

    def test_apply_status_restore_through_the_tool(self):
        session = self.make_session()
        p1, p2 = self.patches()
        with p1, p2:
            out = self.call(session)
            self.assertFalse(out["isError"], out)
            self.assertTrue(session.dlc_prepared)
            self.assertEqual(self.get(self.target, "dlc_signature"), SIG)
            st = self.call(session, action="status")["structuredContent"]
            self.assertFalse(st["needs_prepare"])
            out = self.call(session, action="restore")
            self.assertFalse(out["isError"], out)
            self.assertFalse(session.dlc_prepared)
            self.assertIsNone(self.get(self.target, "dlc_signature"))

    def test_steam_running_stops_the_prepare_when_opted_in(self):
        session = self.make_session()
        p1, p2 = self.patches(steam=[("steam.exe", 1)])
        with p1, p2, mock.patch.dict(os.environ, {"HOI4_LIVE_REQUIRE_STEAM_CLOSED": "1"}):
            out = self.call(session)
        self.assertTrue(out["isError"])
        self.assertIn("Steam is running", out["content"][0]["text"])
        self.assertIsNone(self.get(self.target, "dlc_signature"))

    def test_steam_running_does_not_stop_the_prepare_by_default(self):
        session = self.make_session()
        p1, p2 = self.patches(steam=[("steam.exe", 1)])
        env = {k: v for k, v in os.environ.items() if k != "HOI4_LIVE_REQUIRE_STEAM_CLOSED"}
        with p1, p2, mock.patch.dict(os.environ, env, clear=True):
            out = self.call(session)
        self.assertFalse(out["isError"], out)
        self.assertEqual(self.get(self.target, "dlc_signature"), SIG)

    def test_a_running_game_stops_both_apply_and_restore(self):
        session = self.make_session()
        with self.patches()[0], mock.patch.object(session_tools.process, "running_pids", return_value=[99]):
            self.assertTrue(self.call(session)["isError"])
        p1, p2 = self.patches()
        with p1, p2:
            self.call(session)
        with self.patches()[0], mock.patch.object(session_tools.process, "running_pids", return_value=[99]):
            self.assertTrue(self.call(session, action="restore")["isError"])
            self.assertFalse(self.call(session, action="restore", force=True)["isError"])
        self.assertIsNone(self.get(self.target, "dlc_signature"))

    def test_campaign_mode_refuses_the_helper(self):
        session = self.make_session(mode="campaign")
        with self.assertRaises(ModeRefused):
            session_tools._apply_dlc(session)

    def test_a_failed_launch_puts_the_folder_back(self):
        session = self.make_session()
        session.window = mock.Mock()
        session.window.find.return_value = False
        session.platform = mock.Mock()
        p1, p2 = self.patches()
        args = {"restart": False, "debug": False, "wait_menu": False, "timeout_s": 60, "prepare_dlc": True}
        with p1, p2, mock.patch.object(session_tools.process, "crash_dirs", return_value=set()), \
                mock.patch.object(session_tools.process, "launch", side_effect=ToolError("no exe")):
            with self.assertRaises(ToolError):
                session_tools.hoi4_launch(session, args)
        self.assertIsNone(self.get(self.target, "dlc_signature"))
        self.assertFalse(session.dlc_prepared)

    def test_launch_prepares_before_the_process_starts(self):
        session = self.make_session()
        session.window = mock.Mock()
        session.window.find.return_value = False
        session.platform = mock.Mock()
        seen = {}

        def fake_launch(game_dir, launch_args):
            seen["db"] = self.get(self.target, "launcher-v2.sqlite")
            raise ToolError("stop here")
        p1, p2 = self.patches()
        args = {"restart": False, "debug": True, "wait_menu": False, "timeout_s": 60, "prepare_dlc": True}
        with p1, p2, mock.patch.object(session_tools.process, "crash_dirs", return_value=set()), \
                mock.patch.object(session_tools.process, "launch", side_effect=fake_launch):
            with self.assertRaises(ToolError):
                session_tools.hoi4_launch(session, args)
        self.assertEqual(seen["db"], SQLITE)

    def test_shutdown_restores_after_the_exit(self):
        session = self.make_session()
        p1, p2 = self.patches()
        with p1, p2:
            self.call(session)
            out = session.restore_dlc_after_exit()
        self.assertEqual(sorted(out["removed"]), sorted(dlc_prepare.DLC_FILES))
        self.assertIsNone(session.restore_dlc_after_exit())      # nothing prepared any more

    def test_a_copy_that_fails_half_way_is_still_undone_by_the_restore(self):
        session = self.make_session()
        real = dlc_prepare._atomic_write
        calls = []

        def flaky(path, data):
            calls.append(path)
            # the journal and the first file are written, the second file fails (disk full)
            if os.path.basename(str(path)) == "dlc_signature" and "scratch" in str(path):
                raise OSError("disk full")
            return real(path, data)
        p1, p2 = self.patches()
        with p1, p2, mock.patch.object(dlc_prepare, "_atomic_write", side_effect=flaky):
            out = self.call(session)
        self.assertTrue(out["isError"])
        self.assertTrue(session.dlc_prepared)
        self.assertEqual(self.get(self.target, "launcher-v2.sqlite"), SQLITE)        # half copied
        with p1, p2:
            res = session.restore_dlc_after_exit()
        self.assertFalse(res["conflicts"])
        self.assertIsNone(self.get(self.target, "launcher-v2.sqlite"))
        self.assertFalse(session.dlc_prepared)

    def test_a_restore_that_raises_an_oserror_is_reported_not_raised(self):
        session = self.make_session()
        p1, p2 = self.patches()
        with p1, p2:
            self.call(session)
            with mock.patch.object(dlc_prepare, "restore", side_effect=PermissionError("in use")):
                out = session.restore_dlc_after_exit()
        self.assertIn("in use", out["error"])
        self.assertTrue(session.dlc_prepared)

    def test_restore_after_exit_waits_for_the_game_and_reports_conflicts(self):
        session = self.make_session()
        p1, p2 = self.patches()
        with p1, p2:
            self.call(session)
        with mock.patch.object(session_tools.process, "running_pids", return_value=[5]):
            self.assertIn("still running", session.restore_dlc_after_exit()["error"])
        self.assertTrue(session.dlc_prepared)
        self.put(self.target, "launcher-v2.sqlite", b"rewritten")
        with mock.patch.object(session_tools.process, "running_pids", return_value=[]):
            out = session.restore_dlc_after_exit()
        self.assertIn("launcher-v2.sqlite", out["conflicts"])
        self.assertTrue(session.dlc_prepared)


if __name__ == "__main__":
    unittest.main()
