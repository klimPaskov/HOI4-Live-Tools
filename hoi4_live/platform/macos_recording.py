"""Owned-window RGB recording source. Never captures a display or desktop.

Capture reuses the Mac desktop backend's explicit process/window binding and
before/after frame validation. ffmpeg receives raw RGB via a bounded pipe write;
closing the pipe flushes its final Matroska segment without sending video bytes
as interactive commands. Nothing here requests a task port or changes focus.
"""
import os
import select
import time

import numpy as np

from ..errors import CapabilityUnavailable


class FrameSource:
    def __init__(self, identity, window_id, *, window=None, capturer=None):
        if type(window_id) is not int or window_id <= 0:
            raise ValueError("An explicit owned window ID is required")
        if window is None:
            from .macos_desktop import GameWindow
            window = GameWindow(expected_identity=identity, window_id=window_id)
        if capturer is None:
            from .macos_desktop import Capturer
            capturer = Capturer(window, timeout=2, allow_blank=True)
        self.window, self.capturer = window, capturer
        self._size = None

    def inspect(self):
        geometry = self.window.geometry()
        width, height = geometry.pixels
        if min(width, height) <= 0:
            raise RuntimeError("Owned window has no client pixels")
        self._size = width, height
        return int(round(geometry.x)), int(round(geometry.y)), width, height

    def frame(self, size):
        expected = self._size
        if expected is None:
            raise RuntimeError("Inspect the owned window before capturing")
        image = self.capturer.grab()
        if (not isinstance(image, np.ndarray) or image.dtype != np.uint8 or
                image.shape != (expected[1], expected[0], 3)):
            raise RuntimeError("Owned window frame has unexpected RGB geometry")
        width, height = size
        if min(size) <= 0 or width > expected[0] or height > expected[1]:
            raise ValueError("Recording crop extends outside the owned window")
        return np.ascontiguousarray(image[:height, :width]).tobytes()

    @staticmethod
    def write(stream, frame, timeout=2.0):
        """Bound encoder backpressure so a stuck ffmpeg cannot strand supervision."""
        fd = stream.fileno()
        os.set_blocking(fd, False)
        pending = memoryview(frame)
        deadline = time.monotonic() + timeout
        while pending:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([], [fd], [], remaining)[1]:
                raise CapabilityUnavailable("recording", "macos", "ffmpeg frame pipe stalled")
            try:
                written = os.write(fd, pending)
            except BlockingIOError:
                continue
            if written <= 0:
                raise BrokenPipeError("ffmpeg frame pipe closed")
            pending = pending[written:]
