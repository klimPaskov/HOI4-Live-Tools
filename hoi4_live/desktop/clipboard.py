"""Portable clipboard facade. Native access occurs only when requested."""
from ..platform import current


def get_text():
    return current().clipboard().get_text()


def set_text(text):
    return current().clipboard().set_text(text)


class Preserved:
    """Preserve every clipboard representation where the backend supports snapshots."""
    def __enter__(self):
        self.backend = current().clipboard()
        self.snapshot = getattr(self.backend, "snapshot", None)
        self.saved = self.snapshot() if self.snapshot else self.backend.get_text()
        return self

    def __exit__(self, *exc):
        if self.snapshot:
            self.backend.restore(self.saved)
        elif self.saved is not None:
            self.backend.set_text(self.saved)
