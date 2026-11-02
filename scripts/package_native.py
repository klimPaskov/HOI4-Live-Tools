"""Package a built hoi4-live-native binary as a reproducible release archive.

    python scripts/package_native.py --binary native/target/release/hoi4-live-native --platform linux-x86_64 --out dist

Writes dist/hoi4-live-native-<version>-<platform>.tar.gz holding hoi4-live-native-<version>-<platform>/ with the
binary, LICENSE, NOTICE, README.md, CHANGELOG.md and the native README. Entries have fixed times and owners and a
gzip header without a timestamp, so the same binary gives the same archive. The version is the native crate's.
"""

import argparse
import gzip
import io
import os
import re
import tarfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXED_MTIME = 1767225600  # 2026-01-01T00:00:00Z


def crate_version():
    with open(os.path.join(ROOT, "native", "Cargo.toml"), encoding="utf-8") as f:
        return re.search(r'^version\s*=\s*"([^"]+)"', f.read(), re.M).group(1)


def add(tar, name, data, mode):
    info = tarfile.TarInfo(name)
    info.size = len(data)
    info.mtime = FIXED_MTIME
    info.mode = mode
    info.uid = info.gid = 0
    info.uname = info.gname = ""
    tar.addfile(info, io.BytesIO(data))


def package(binary, platform, out_dir):
    version = crate_version()
    top = f"hoi4-live-native-{version}-{platform}"
    exe = "hoi4-live-native.exe" if platform.startswith("windows") else "hoi4-live-native"
    files = [(exe, binary, 0o755)]
    for name, rel in (("LICENSE", "LICENSE"), ("NOTICE", "NOTICE"), ("README.md", "README.md"),
                      ("CHANGELOG.md", "CHANGELOG.md"), ("NATIVE.md", os.path.join("native", "README.md"))):
        path = os.path.join(ROOT, rel)
        if os.path.exists(path):
            files.append((name, path, 0o644))
    if platform.startswith("macos"):
        files.append(("debugger.entitlements", os.path.join(ROOT, "native", "macos", "debugger.entitlements"), 0o644))
    os.makedirs(out_dir, exist_ok=True)
    raw = io.BytesIO()
    with tarfile.open(fileobj=raw, mode="w", format=tarfile.PAX_FORMAT) as tar:
        for name, path, mode in sorted(files):
            with open(path, "rb") as f:
                add(tar, f"{top}/{name}", f.read(), mode)
    archive = os.path.join(out_dir, f"{top}.tar.gz")
    with open(archive, "wb") as f:
        with gzip.GzipFile(filename="", mode="wb", fileobj=f, mtime=0, compresslevel=9) as gz:
            gz.write(raw.getvalue())
    return archive


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--binary", required=True)
    parser.add_argument("--platform", required=True, help="e.g. linux-x86_64, macos-arm64")
    parser.add_argument("--out", default=os.path.join(ROOT, "dist"))
    args = parser.parse_args()
    print(package(args.binary, args.platform, args.out))


if __name__ == "__main__":
    main()
