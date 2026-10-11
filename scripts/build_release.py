#!/usr/bin/env python3
"""Build the release archive: dist/hoi4-live-tools-<version>.zip and dist/SHA256SUMS.txt.

    python scripts/build_release.py [--out dist]

The archive holds exactly the files an install needs (install.PAYLOAD_*) under one top folder, stored without
compression with fixed timestamps and sorted names, so the same sources always give the same bytes. Before building
it checks that the version agrees in hoi4_live/__init__.py, pyproject.toml and the top CHANGELOG.md entry, and that
the working tree has no uncommitted change to a payload file (use --allow-dirty for a trial build). The printed
SHA-256 is what the Agentic HOI4 Modding bootstrap pins; publish exactly this file.
"""

import argparse
import hashlib
import os
import re
import subprocess
import sys
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, ROOT)
import install  # noqa: E402

FIXED_TIME = (2026, 1, 1, 0, 0, 0)


def declared_versions():
    found = {"package": install.read_version(ROOT)}
    with open(os.path.join(ROOT, "pyproject.toml"), encoding="utf-8") as f:
        found["pyproject"] = re.search(r'^version\s*=\s*"([^"]+)"', f.read(), re.M).group(1)
    with open(os.path.join(ROOT, "CHANGELOG.md"), encoding="utf-8") as f:
        match = re.search(r"^## \[?v?(\d+\.\d+\.\d+[^\]\s]*)\]?", f.read(), re.M)
    found["changelog"] = match.group(1) if match else None
    return found


def dirty_payload(files):
    try:
        out = subprocess.run(["git", "status", "--porcelain", "--"] + [name for name, _ in files], cwd=ROOT,
                             stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=True).stdout.decode()
    except (OSError, subprocess.CalledProcessError):
        return []
    return [line for line in out.splitlines() if line.strip()]


def build(out_dir, allow_dirty=False):
    versions = declared_versions()
    if len(set(versions.values())) != 1:
        raise SystemExit(f"version mismatch: {versions}")
    version = versions["package"]
    files = install.payload_files(ROOT)
    dirty = dirty_payload(files)
    if dirty and not allow_dirty:
        raise SystemExit("uncommitted payload changes (commit them or pass --allow-dirty):\n" + "\n".join(dirty))
    os.makedirs(out_dir, exist_ok=True)
    top = f"hoi4-live-tools-{version}"
    archive = os.path.join(out_dir, f"{top}.zip")
    with zipfile.ZipFile(archive, "w", zipfile.ZIP_STORED) as z:
        for name, path in files:
            with open(path, "rb") as f:
                data = f.read()
            if name.endswith((".cmd", ".ps1", ".py", ".md", ".json", ".toml", ".txt")) or name in ("LICENSE", "NOTICE"):
                data = data.replace(b"\r\n", b"\n")
                if name.endswith(".cmd"):
                    data = data.replace(b"\n", b"\r\n")
            info = zipfile.ZipInfo(f"{top}/{name}", FIXED_TIME)
            info.compress_type = zipfile.ZIP_STORED
            info.external_attr = 0o644 << 16
            z.writestr(info, data)
    with open(archive, "rb") as f:
        digest = hashlib.sha256(f.read()).hexdigest()
    with open(os.path.join(out_dir, "SHA256SUMS.txt"), "w", encoding="utf-8", newline="\n") as f:
        f.write(f"{digest}  {os.path.basename(archive)}\n")
    return archive, digest, len(files)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--out", default=os.path.join(ROOT, "dist"))
    parser.add_argument("--allow-dirty", action="store_true")
    args = parser.parse_args()
    archive, digest, count = build(args.out, args.allow_dirty)
    print(f"{archive}\n{count} files\nsha256 {digest}")


if __name__ == "__main__":
    main()
