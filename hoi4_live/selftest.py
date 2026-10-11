"""Offline environment check: python -m hoi4_live selftest. Does not touch the game."""

import os
import shutil
import sys
import time


def run(cfg):
    ok = True

    def line(name, good, detail=""):
        nonlocal ok
        ok = ok and good
        print(f"{'ok  ' if good else 'FAIL'} {name}: {detail}")

    line("python", sys.version_info >= (3, 9), sys.version.split()[0])
    for mod in ("numpy", "PIL", "cv2"):
        try:
            __import__(mod)
            line(mod, True, "importable")
        except ImportError as e:
            line(mod, False, str(e))
    line("game_dir", bool(cfg.game_dir and os.path.exists(cfg.game_exe or "")), str(cfg.game_dir))
    line("user_dir", os.path.isdir(cfg.user_dir), cfg.user_dir)
    line("saves_dir", os.path.isdir(cfg.saves_dir), cfg.saves_dir)
    line("logs_dir", os.path.isdir(cfg.logs_dir), cfg.logs_dir)
    ff = shutil.which(cfg["capture"]["ffmpeg"]) or (cfg["capture"]["ffmpeg"] if os.path.exists(cfg["capture"]["ffmpeg"]) else None)
    line("ffmpeg", bool(ff), ff or "not found (needed for ddagrab capture)")
    try:
        settings = open(cfg.settings_path, encoding="utf-8", errors="replace").read()
        binary = "save_as_binary=yes" in settings
        line("text saves", not binary, "save_as_binary=no" if not binary else "save_as_binary=yes: save queries need text saves")
    except OSError as e:
        line("settings.txt", False, str(e))
    try:
        from .ocr.winocr import OcrHost
        import numpy as np
        h = OcrHost(cfg["ocr"]["language"])
        t = time.time()
        img = np.full((60, 400, 3), 255, np.uint8)
        from PIL import Image, ImageDraw
        im = Image.fromarray(img)
        ImageDraw.Draw(im).text((10, 20), "HOI4 LIVE TOOLS 1936", fill=(0, 0, 0))
        res = h.recognize(np.asarray(im), scale=3)
        h.close()
        line("windows ocr", True, f"{round(time.time() - t, 1)} s, read {res.text!r}")
    except Exception as e:
        line("windows ocr", False, f"{type(e).__name__}: {e}")
    from .tools import registry
    line("tools", len(registry.all_tools()) > 0, f"{len(registry.all_tools())} registered")
    return 0 if ok else 1
