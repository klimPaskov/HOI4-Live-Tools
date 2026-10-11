"""Entry point.

    python -m hoi4_live                 run the MCP stdio server (what MCP clients launch)
    python -m hoi4_live list            list the tools
    python -m hoi4_live call TOOL JSON  run one tool from the command line (JSON or @file.json)
    python -m hoi4_live docs            regenerate docs/tools.md from the registry
    python -m hoi4_live selftest        offline environment check (paths, ffmpeg, OCR host)
"""

import json
import logging
import logging.handlers
import os
import sys


def setup_logging(cfg, to_stderr=True):
    root = logging.getLogger("hoi4_live")
    root.setLevel(logging.INFO)
    fmt = logging.Formatter("%(asctime)s %(levelname)s %(name)s: %(message)s")
    if to_stderr:
        h = logging.StreamHandler(sys.stderr)
        h.setFormatter(fmt)
        h.setLevel(logging.WARNING)
        root.addHandler(h)
    try:
        d = os.path.join(cfg.artifacts_dir, "logs")
        os.makedirs(d, exist_ok=True)
        fh = logging.handlers.RotatingFileHandler(os.path.join(d, "hoi4_live.log"), maxBytes=2 << 20, backupCount=3,
                                                  encoding="utf-8")
        fh.setFormatter(fmt)
        root.addHandler(fh)
    except OSError:
        pass


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    from . import config
    cfg = config.get()
    cmd = argv[0] if argv else "serve"
    setup_logging(cfg, to_stderr=True)
    from . import tools  # noqa: F401  (registers tools)
    from .tools import registry
    if cmd == "serve":
        from .protocol import server
        from .session import Session
        server.run(Session(cfg))
        return 0
    if cmd == "list":
        for t in sorted(registry.all_tools(), key=lambda t: (t.family, t.name)):
            print(f"{t.name:22s} [{t.family}, lock={t.lock}{', test-only' if t.test_only else ''}] "
                  f"{t.description.split('.')[0]}")
        return 0
    if cmd == "call":
        if len(argv) < 2:
            print("usage: python -m hoi4_live call TOOL [JSON]", file=sys.stderr)
            return 2
        from .session import Session
        from .tools import dispatch
        session = Session(cfg)
        raw = argv[2] if len(argv) > 2 else "{}"
        if raw.startswith("@"):                      # @file.json avoids shell quoting of regexes
            with open(raw[1:], encoding="utf-8") as f:
                raw = f.read()
        args = json.loads(raw)
        try:
            out = dispatch.call(session, argv[1], args, progress=lambda m, *a: print("..", m, file=sys.stderr))
        finally:
            session.shutdown()
        for c in out["content"]:
            if c["type"] == "text":
                print(c["text"])
            else:
                print(f"[image {c['mimeType']}, {len(c['data']) * 3 // 4} bytes]")
        return 1 if out.get("isError") else 0
    if cmd == "docs":
        from .docs import write_tools_md
        path = write_tools_md()
        print("wrote", path)
        return 0
    if cmd == "selftest":
        from .selftest import run
        return run(cfg)
    print(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
