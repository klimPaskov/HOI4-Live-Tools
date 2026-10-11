"""Short cross-process file gate; keep its inode permanently at the same path."""
import contextlib
import errno
import os
import sys
import time

from ..errors import LockBusy


@contextlib.contextmanager
def file_gate(path, timeout=2.0):
    # Never unlink this gate: another process could still hold the old inode.
    fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o600)
    locked = False
    deadline = time.monotonic() + timeout
    try:
        if sys.platform == 'win32':
            import msvcrt
            if os.fstat(fd).st_size == 0:
                os.write(fd, b'\0')
            def take():
                os.lseek(fd, 0, os.SEEK_SET)
                msvcrt.locking(fd, msvcrt.LK_NBLCK, 1)
            def drop():
                os.lseek(fd, 0, os.SEEK_SET)
                msvcrt.locking(fd, msvcrt.LK_UNLCK, 1)
        else:
            import fcntl
            def take():
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            def drop():
                fcntl.flock(fd, fcntl.LOCK_UN)
        while not locked:
            try:
                take()
                locked = True
            except OSError as exc:
                if exc.errno not in (errno.EACCES, errno.EAGAIN, errno.EDEADLK):
                    raise
                if time.monotonic() >= deadline:
                    raise LockBusy('desktop lease metadata is busy; retry', {'lock': path}) from exc
                time.sleep(0.02)
        yield
    finally:
        if locked:
            drop()
        os.close(fd)
