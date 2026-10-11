"""Exclusive desktop lease with per-instance tokens and exact process identity.

A live owner's lease never expires by age. A dead or positively identified recycled
PID can be recovered; unreadable records require the stale grace period. Metadata
updates use a permanent OS file gate, preventing competing stale-file takeovers.
"""
import contextlib
import datetime
import json
import os
import threading
import time
import uuid

from .. import platform
from ..errors import LockBusy, ToolError
from ..platform.filelock import file_gate


class DesktopLock:
    def __init__(self, path, stale_seconds=600, wait_seconds=20, pid=None,
                 owner='hoi4-live-tools', backend=None):
        self.path = path
        self.stale_seconds = stale_seconds
        self.wait_seconds = wait_seconds
        self.pid = pid or os.getpid()
        self._creator_pid = os.getpid()
        self.owner = owner
        self.backend = backend if backend is not None else platform.current()
        self.token = uuid.uuid4().hex
        self.identity = self._identity(self.pid)
        self.depth = 0
        self.sticky = False
        self.purpose = None
        self._mutex = threading.RLock()

    def _identity(self, pid):
        try:
            identity = self.backend.capture_process_identity(pid)
            if (type(identity.creation_time) is not int or identity.creation_time <= 0
                    or not isinstance(identity.executable_path, str) or not identity.executable_path):
                return None
            return {'creation_time': identity.creation_time, 'executable_path': identity.executable_path}
        except (OSError, ToolError, AttributeError, NotImplementedError):
            return None

    def _ours(self, info):
        return (os.getpid() == self._creator_pid and info is not None and info.get('pid') == self.pid
                and info.get('token') == self.token
                and info.get('identity') == self.identity)

    def read(self):
        try:
            with open(self.path, encoding='utf-8') as handle:
                data = json.load(handle)
            if not isinstance(data, dict):
                raise ValueError
            return data
        except FileNotFoundError:
            return None
        except (ValueError, OSError):
            try:
                return {'pid': None, 'purpose': 'unreadable lock file', 'epoch': os.path.getmtime(self.path)}
            except OSError:
                return None

    def _payload(self, purpose):
        now = time.time()
        return {'pid': self.pid, 'token': self.token, 'identity': self.identity,
                'purpose': purpose, 'owner': self.owner, 'epoch': now,
                'time': datetime.datetime.fromtimestamp(now).isoformat(timespec='seconds')}

    def _rewrite(self, purpose):
        temporary = self.path + '.' + self.token + '.tmp'
        try:
            with open(temporary, 'w', encoding='utf-8') as handle:
                json.dump(self._payload(purpose), handle)
            os.replace(temporary, self.path)
        finally:
            try:
                os.remove(temporary)
            except FileNotFoundError:
                pass

    @staticmethod
    def _age(info):
        try:
            return time.time() - float(info.get('epoch') or 0)
        except (TypeError, ValueError):
            return 0.0  # malformed metadata cannot authorize immediate takeover

    def is_stale(self, info):
        if info is None:
            return True
        pid = info.get('pid')
        if type(pid) is not int or pid <= 0:
            return self._age(info) > self.stale_seconds
        try:
            if not self.backend.pid_alive(pid):
                return True
        except (OSError, ToolError):
            return False
        recorded = info.get('identity')
        if isinstance(recorded, dict) and recorded.get('creation_time') and recorded.get('executable_path'):
            current = self._identity(pid)
            if current is not None:
                return current != recorded
        return False

    def holder(self):
        info = self.read()
        if info is None:
            return None
        return dict(info, age_s=round(self._age(info), 1), stale=self.is_stale(info), ours=self._ours(info))

    def acquire(self, purpose, wait=None, sticky=False):
        if os.getpid() != self._creator_pid:
            raise LockBusy('an inherited desktop lease cannot be used in another process; create a new lock instance')
        wait = self.wait_seconds if wait is None else wait
        deadline = time.monotonic() + max(0, wait)
        with self._mutex:
            while True:
                with file_gate(self.path + '.gate'):
                    info = self.read()
                    if self._ours(info):
                        keep = self.purpose if self.sticky else (info.get('purpose') if self.depth else purpose)
                        self._rewrite(keep or purpose)
                        break
                    if info is None or self.is_stale(info):
                        self._rewrite(purpose)
                        break
                if time.monotonic() >= deadline:
                    raise LockBusy('the desktop lock is held by pid %s (%s); wait and retry' %
                                   (info.get('pid'), info.get('purpose')), {'lock': self.path, 'holder': info})
                time.sleep(min(0.1, max(0, deadline - time.monotonic())))
            if sticky:
                self.sticky = True
                self.purpose = purpose
            self.depth += 1
        return True

    def release(self, force_sticky=False):
        with self._mutex, file_gate(self.path + '.gate'):
            if self.depth > 0:
                self.depth -= 1
            if force_sticky:
                self.sticky = False
                self.depth = 0
            if self.depth == 0 and not self.sticky:
                if self._ours(self.read()):
                    os.remove(self.path)
                return True
        return False

    def refresh(self):
        with self._mutex, file_gate(self.path + '.gate'):
            info = self.read()
            if self._ours(info):
                self._rewrite(info.get('purpose') or 'refresh')

    @contextlib.contextmanager
    def hold(self, purpose, wait=None):
        self.acquire(purpose, wait)
        try:
            yield self
        finally:
            self.release()

    def held_by_us(self):
        return self._ours(self.read())

    def release_all(self):
        self.release(force_sticky=True)
