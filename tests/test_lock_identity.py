"""Desktop lease safety, including same-process sessions and competing recovery."""
import json
import os
import tempfile
import threading
import time
import unittest
from types import SimpleNamespace
from unittest import mock

import helpers  # noqa: F401
from hoi4_live.desktop.lock import DesktopLock
from hoi4_live.errors import LockBusy
from hoi4_live.platform.identity import ProcessIdentity
from hoi4_live.platform.filelock import file_gate


class LeaseTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = os.path.join(self.directory.name, 'desktop.lock')
        self.backend = mock.Mock()
        self.backend.pid_alive.return_value = True
        self.backend.capture_process_identity.side_effect = lambda pid: ProcessIdentity(pid, 100, '/python')

    def lock(self):
        return DesktopLock(self.path, wait_seconds=0, backend=self.backend)

    def write(self, data):
        with open(self.path, 'w', encoding='utf8') as handle:
            json.dump(data, handle)

    def test_same_pid_different_instances_cannot_share_lease(self):
        first, second = self.lock(), self.lock()
        first.acquire('player', sticky=True)
        with self.assertRaises(LockBusy):
            second.acquire('competing session')
        self.assertFalse(second.held_by_us())
        second.refresh()
        second.release_all()
        self.assertTrue(first.held_by_us())
        first.release_all()

    def test_recycled_pid_lease_can_be_recovered(self):
        self.write({'pid': 77, 'identity': {'creation_time': 99, 'executable_path': '/python'},
                    'token': 'old', 'epoch': time.time()})
        owner = self.lock()
        owner.acquire('recovered')
        self.assertTrue(owner.held_by_us())
        owner.release_all()

    def test_inherited_instance_cannot_impersonate_its_parent(self):
        owner = self.lock()
        owner.acquire('parent', sticky=True)
        with mock.patch('hoi4_live.desktop.lock.os.getpid', return_value=owner._creator_pid + 1):
            self.assertFalse(owner.held_by_us())
            with self.assertRaises(LockBusy):
                owner.acquire('child')
            owner.release_all()
            self.assertTrue(os.path.exists(self.path))
        self.assertTrue(owner.held_by_us())
        owner.release_all()

    def test_denied_identity_cannot_authorize_takeover(self):
        self.write({'pid': 77, 'identity': {'creation_time': 99, 'executable_path': '/python'},
                    'epoch': 1, 'token': 'old'})
        self.backend.capture_process_identity.side_effect = PermissionError('denied')
        with self.assertRaises(LockBusy):
            self.lock().acquire('no')

    def test_unreadable_record_recovers_only_after_grace_period(self):
        with open(self.path, 'w') as handle:
            handle.write('incomplete')
        owner = self.lock()
        with self.assertRaises(LockBusy):
            owner.acquire('early')
        old = time.time() - 1000
        os.utime(self.path, (old, old))
        owner.acquire('recovered')
        self.assertTrue(owner.held_by_us())
        owner.release_all()

    def test_competing_stale_recovery_has_exactly_one_owner(self):
        self.write({'pid': 77, 'identity': {'creation_time': 99, 'executable_path': '/python'},
                    'token': 'old', 'epoch': time.time()})
        contenders = [self.lock() for _ in range(8)]
        barrier = threading.Barrier(len(contenders))
        won, errors = [], []
        def compete(lock):
            try:
                barrier.wait(timeout=5)
                lock.acquire('contender')
                won.append(lock)
            except LockBusy:
                pass
            except Exception as exc:
                errors.append(exc)
        threads = [threading.Thread(target=compete, args=(lock,)) for lock in contenders]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join(timeout=5)
        self.assertFalse(any(thread.is_alive() for thread in threads))
        self.assertEqual(errors, [])
        self.assertEqual(len(won), 1)
        self.assertEqual(sum(lock.held_by_us() for lock in contenders), 1)
        won[0].release_all()

    def test_os_gate_excludes_another_open_handle(self):
        gate = self.path + '.gate'
        with file_gate(gate):
            with self.assertRaises(LockBusy):
                with file_gate(gate, timeout=0):
                    self.fail('second handle acquired the gate')
        self.assertTrue(os.path.exists(gate), 'gate inode must never be unlinked')
        with file_gate(gate, timeout=0):
            pass

    def test_windows_empty_gate_never_writes_before_locking(self):
        from hoi4_live.platform import filelock
        native = SimpleNamespace(LK_NBLCK=2, LK_UNLCK=0, locking=mock.Mock())
        gate = self.path + '.empty-gate'
        with mock.patch.object(filelock.sys, 'platform', 'win32'), \
             mock.patch.dict('sys.modules', {'msvcrt': native}), \
             mock.patch.object(filelock.os, 'write', side_effect=PermissionError('byte held by contender')):
            with file_gate(gate, timeout=0):
                self.assertEqual(os.path.getsize(gate), 0)
        self.assertEqual([call.args[1:] for call in native.locking.call_args_list], [(2, 1), (0, 1)])


    def test_windows_generic_identity_accepts_python_and_closes_query_handle(self):
        from hoi4_live.platform.windows import WindowsPlatform
        from hoi4_live.memory import winproc
        api = mock.Mock()
        api.open_process.return_value = 123
        identity = winproc.ProcessIdentity(77, 100, r'C:\Python\python.exe')
        api.get_process_identity.return_value = identity
        with mock.patch.object(winproc, 'WindowsAPI', return_value=api):
            self.assertEqual(WindowsPlatform().capture_process_identity(77), identity)
        api.close.assert_called_once_with(123)

    def test_windows_expected_image_mismatch_closes_query_handle(self):
        from hoi4_live.platform.windows import WindowsPlatform
        from hoi4_live.memory import winproc
        api = mock.Mock()
        api.open_process.return_value = 123
        api.get_process_identity.return_value = winproc.ProcessIdentity(77, 100, r'C:\Other\game.exe')
        with mock.patch.object(winproc, 'WindowsAPI', return_value=api):
            with self.assertRaises(winproc.OwnershipError):
                WindowsPlatform().capture_process_identity(77, r'C:\Game\hoi4.exe')
        api.close.assert_called_once_with(123)


if __name__ == '__main__':
    unittest.main()
