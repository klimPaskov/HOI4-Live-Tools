"""Image helpers: numpy <-> PIL, downscaled encodes for MCP image content, artifact files."""

import datetime
import io
import os
import re

import numpy as np
from PIL import Image


def to_pil(img):
    return img if isinstance(img, Image.Image) else Image.fromarray(np.asarray(img, dtype=np.uint8))


def crop(img, region):
    """region = [x, y, w, h] in image pixels (clamped)."""
    if not region:
        return img
    x, y, w, h = [int(v) for v in region]
    H, W = img.shape[:2]
    x0, y0 = max(0, x), max(0, y)
    x1, y1 = min(W, x + w), min(H, y + h)
    if x1 <= x0 or y1 <= y0:
        raise ValueError(f"region {region} is outside the {W}x{H} image")
    return img[y0:y1, x0:x1]


def encode(img, max_width=960, fmt="png", quality=85):
    """Bytes and mime type, downscaled so the width is at most max_width (0 = full size)."""
    im = to_pil(img).convert("RGB")
    if max_width and im.width > max_width:
        im = im.resize((max_width, max(1, round(im.height * max_width / im.width))), Image.LANCZOS)
    buf = io.BytesIO()
    if fmt == "jpeg":
        im.save(buf, format="JPEG", quality=quality)
        return buf.getvalue(), "image/jpeg", im.size
    im.save(buf, format="PNG")
    return buf.getvalue(), "image/png", im.size


def safe_name(name):
    name = re.sub(r"[^A-Za-z0-9_.-]+", "_", name or "shot").strip("._")
    return name[:80] or "shot"


def save_artifact(img, directory, name, stamp=True):
    os.makedirs(directory, exist_ok=True)
    base = safe_name(name)
    if stamp:
        base = datetime.datetime.now().strftime("%Y%m%d_%H%M%S_") + base
    path = os.path.join(directory, base + ".png")
    to_pil(img).save(path)
    return path


def upscale(img, factor):
    im = to_pil(img)
    if factor == 1:
        return im
    return im.resize((max(1, int(im.width * factor)), max(1, int(im.height * factor))), Image.LANCZOS)
