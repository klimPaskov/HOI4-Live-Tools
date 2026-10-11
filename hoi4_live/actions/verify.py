"""The verified-action primitive every campaign action builds on.

    with Verified(session, "set focus", region=panel_box) as v:
        session.input.click(x, y)
        v.expect(lambda after: some_reader(after) == wanted, "focus selected")
    v.report  -> {"action", "changed_ratio", "checks": [...], "screenshot"}

On exit it re-captures the screen, measures how much changed (overall and in the region), evaluates the
expectations and raises a ToolError with the after-screenshot attached when one fails.
"""

import time

from ..desktop import images
from ..errors import ToolError
from ..tools.common import change_ratio


class Verified:
    def __init__(self, session, action, region=None, settle=0.6):
        self.session = session
        self.action = action
        self.region = region
        self.settle = settle
        self.checks = []
        self.report = {"action": action}

    def __enter__(self):
        self.before = self.session.grab()
        return self

    def expect(self, predicate, description):
        self.checks.append((predicate, description))

    def __exit__(self, exc_type, exc, tb):
        if exc_type is not None:
            return False
        time.sleep(self.settle)
        after = self.session.grab()
        self.report["changed_ratio"] = round(change_ratio(self.before, after), 4)
        if self.region:
            self.report["changed_ratio_region"] = round(change_ratio(self.before, after, self.region), 4)
        results = []
        failed = None
        for pred, desc in self.checks:
            ok = bool(pred(after))
            results.append({"check": desc, "ok": ok})
            if not ok and failed is None:
                failed = desc
        self.report["checks"] = results
        if getattr(self.session, "image_mode", "all") == "all":
            self.report["screenshot"] = self.session.artifact(after, self.action.replace(" ", "_"))
        self.after = after
        if failed:
            png, _, _ = images.encode(after, 960)
            raise ToolError(f"{self.action}: check failed: {failed}", self.report, image=png)
        return False
