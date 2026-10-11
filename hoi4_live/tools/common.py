"""Helpers shared by tool modules: screenshots for results, schema fragments, change detection."""

import numpy as np

from ..desktop import images
from .registry import ToolResult

REGION = {"type": "array", "items": {"type": "integer", "minimum": 0}, "minItems": 4, "maxItems": 4,
          "description": "[x, y, width, height] in game client pixels"}
MODE = {"type": "string", "enum": ["test", "campaign"],
        "description": "switch the session mode first (campaign refuses console cheats)"}


def attach(result, session, img, name=None, max_width=None, fmt=None, save=True, force=False):
    """Add a (downscaled) screenshot to a ToolResult and keep the full image as an artifact. With the session's
    image mode set to errors or none (hoi4_lock action=set_images) successful actions skip both: encoding and
    saving a full frame costs seconds on a busy machine. force=True always attaches (hoi4_screenshot)."""
    if not force and getattr(session, "image_mode", "all") != "all":
        return result
    cfg = session.cfg["screenshot"]
    if save:
        path = session.artifact(img, name or "shot")
        result.data.setdefault("screenshots", []).append(path)
    data, mime, size = images.encode(img, cfg["max_width"] if max_width is None else max_width,
                                     fmt or cfg["format"])
    result.add_image(data, mime)
    return result


def change_ratio(a, b, region=None):
    """Share of pixels that changed noticeably between two screenshots (optionally inside a region)."""
    if a is None or b is None or a.shape != b.shape:
        return 1.0
    if region:
        a = images.crop(a, region)
        b = images.crop(b, region)
    d = np.abs(a.astype(np.int16) - b.astype(np.int16)).max(2)
    return float((d > 24).mean())


def apply_mode(session, args):
    m = args.get("mode")
    if m:
        session.mode = m
    return session.mode


def result(summary="", **data):
    return ToolResult(summary, data)
