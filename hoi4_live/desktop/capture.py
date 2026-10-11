"""Portable capture factory and image validation helpers."""
import time
from ..platform import current


class Capturer:
    def __new__(cls, window, ffmpeg="ffmpeg", method="auto", output_idx="auto", timeout=20, backend=None, **kwargs):
        return (backend or current()).make_capturer(window, ffmpeg=ffmpeg, method=method,
                                                   output_idx=output_idx, timeout=timeout, **kwargs)


def is_blank(img):
    """Detect near-black frames commonly returned by unavailable capture paths."""
    if img is None or img.size == 0:
        return True
    small = img[::16, ::16]
    return float(small.std()) < 2.0 and float(small.mean()) < 8.0


def timed(fn, *args, **kwargs):
    start = time.monotonic()
    result = fn(*args, **kwargs)
    return result, time.monotonic() - start
