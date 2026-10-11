"""Shared test helpers: a Session wired to a temporary lock file so tests never touch the real desktop lock."""

import os
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if ROOT not in sys.path:
    sys.path.insert(0, ROOT)

FIXTURES = os.path.join(ROOT, "tests", "fixtures")
EXTERNAL_POPUPS = os.environ.get("HOI4_LIVE_POPUP_FIXTURES", "")


def needs_fixture(*parts):
    """Skip a test whose screenshot fixture is not shipped (some captures come from a private mod and are not in the
    public repository)."""
    import unittest
    path = os.path.join(FIXTURES, *parts)
    return unittest.skipUnless(os.path.exists(path), "fixture %s is not in this checkout" % "/".join(parts))


def make_session(tmpdir=None):
    from hoi4_live import config
    from hoi4_live.session import Session
    tmpdir = tmpdir or tempfile.mkdtemp(prefix="hoi4live_test_")
    cfg = config.load(overrides={"mode": "test", "lock": {"path": os.path.join(tmpdir, "test.lock"), "wait_seconds": 1},
                                 "artifacts_dir": os.path.join(tmpdir, "artifacts")})
    return Session(cfg)
