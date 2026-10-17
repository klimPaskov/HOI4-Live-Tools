"""Portable game-window factory. Native APIs live in hoi4_live.platform."""
from ..platform import current


class GameWindow:
    def __new__(cls, process_name=None, title_prefix="Hearts of Iron IV", pin=True, backend=None, **kwargs):
        backend = backend or current()
        return backend.make_window(process_name=process_name or backend.game_process_name,
                                   title_prefix=title_prefix, pin=pin, **kwargs)
