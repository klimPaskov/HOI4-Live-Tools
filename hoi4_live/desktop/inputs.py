"""Portable input factory; every backend receives a game-window guard."""
import math
from ..platform import current


class Input:
    def __new__(cls, window, key_delay=0.04, type_delay=0.035, click_hold=0.06, settle=0.25, backend=None, **kwargs):
        return (backend or current()).make_input(window, key_delay=key_delay, type_delay=type_delay,
                                                click_hold=click_hold, settle=settle, **kwargs)


def distance(a, b):
    return math.hypot(a[0] - b[0], a[1] - b[1])
