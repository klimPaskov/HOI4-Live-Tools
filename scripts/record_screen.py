"""Record the HOI4 client area (found from the running game, on whichever monitor it is) with ffmpeg, in segments.

    python scripts/record_screen.py start <out_dir> [--fps 24] [--crf 27] [--size 1920x1080] [--segment 900]
    python scripts/record_screen.py stop  <out_dir>

Segments are Matroska files named seg_<YYYYmmdd_HHMMSS>.mkv (local wall clock of the segment start), so a
calls.jsonl timestamp maps to a position in a segment; they stay playable if ffmpeg is killed. `start` writes
recording.json (ffmpeg pid, wall-clock start, settings) and returns once ffmpeg runs; `stop` ends ffmpeg
gracefully (q on stdin is not available when detached, so it sends CTRL_BREAK-less taskkill after flushing).
"""

import argparse
import json
import os
import shutil
import subprocess
import sys
import time


def ffmpeg_path():
    return shutil.which("ffmpeg") or "ffmpeg"


def encoder_args(a):
    """AMD AMF (the GPU's video block, almost no CPU: the game's simulation keeps its cores) or x264."""
    if a.encoder == "amf":
        return ["-c:v", "h264_amf", "-usage", "transcoding", "-quality", "speed", "-rc", "cqp",
                "-qp_i", str(a.crf), "-qp_p", str(a.crf + 2), "-qp_b", str(a.crf + 2)]
    return ["-c:v", "libx264", "-preset", "veryfast", "-crf", str(a.crf), "-threads", "3"]


def client_rect():
    """(x, y, w, h) of the running game's client area on the virtual desktop."""
    sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    from hoi4_live.desktop.window import GameWindow
    return GameWindow().client()


def start(a):
    os.makedirs(a.out, exist_ok=True)
    x, y, w, h = client_rect()
    size = a.size if a.size != "auto" else f"{w}x{h}"
    cmd = [ffmpeg_path(), "-hide_banner", "-loglevel", "warning", "-y",
           "-f", "gdigrab", "-framerate", str(a.fps), "-draw_mouse", "1",
           "-offset_x", str(x), "-offset_y", str(y), "-video_size", size, "-i", "desktop",
           *encoder_args(a), "-pix_fmt", "yuv420p", "-g", str(a.fps * 2),
           "-f", "segment", "-segment_time", str(a.segment), "-reset_timestamps", "1", "-strftime", "1",
           "-segment_format", "matroska", os.path.join(a.out, "seg_%Y%m%d_%H%M%S.mkv")]
    log = open(os.path.join(a.out, "ffmpeg.log"), "ab")
    t0 = time.time()
    p = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=log, stderr=log,
                         creationflags=0x00000200 | 0x00000008)  # new process group, detached
    time.sleep(3)
    if p.poll() is not None:
        if a.encoder == "amf":                       # no usable GPU encoder: fall back to x264
            a.encoder = "x264"
            return start(a)
        sys.exit(f"ffmpeg exited with {p.returncode}; see ffmpeg.log")
    info = {"pid": p.pid, "started_epoch": t0, "started": time.strftime("%Y-%m-%dT%H:%M:%S", time.localtime(t0)),
            "fps": a.fps, "crf": a.crf, "encoder": a.encoder, "size": size, "offset": [x, y], "segment_s": a.segment, "cmd": cmd}
    json.dump(info, open(os.path.join(a.out, "recording.json"), "w"), indent=1)
    print(f"recording pid {p.pid}")


def stop(a):
    info = json.load(open(os.path.join(a.out, "recording.json")))
    pid = info["pid"]
    # graceful finish: ffmpeg writes the segment trailer on CTRL_BREAK; fall back to terminate
    subprocess.run(["taskkill", "/PID", str(pid)], capture_output=True)
    for _ in range(20):
        r = subprocess.run(["tasklist", "/FI", f"PID eq {pid}"], capture_output=True, text=True)
        if str(pid) not in r.stdout:
            break
        time.sleep(0.5)
    else:
        subprocess.run(["taskkill", "/F", "/PID", str(pid)], capture_output=True)
    info["stopped_epoch"] = time.time()
    json.dump(info, open(os.path.join(a.out, "recording.json"), "w"), indent=1)
    print("stopped")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("cmd", choices=["start", "stop"])
    ap.add_argument("out")
    ap.add_argument("--fps", type=int, default=24)
    ap.add_argument("--crf", type=int, default=32, help="AMF quantiser (x264: CRF 27 is about the same quality)")
    ap.add_argument("--size", default="auto", help="WxH, default the game client size")
    ap.add_argument("--segment", type=int, default=900)
    ap.add_argument("--encoder", choices=["amf", "x264"], default="amf")
    a = ap.parse_args()
    (start if a.cmd == "start" else stop)(a)


if __name__ == "__main__":
    main()
