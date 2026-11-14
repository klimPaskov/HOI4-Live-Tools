#!/bin/sh
# HOI4 Live Tools: one-line installer for macOS and Linux.
#
#   curl -fsSL https://github.com/klimPaskov/HOI4-Live-Tools/releases/latest/download/install.sh | sh
#
# It finds Python 3.9 or newer, downloads install.py from the release, checks it against the release's
# SHA256SUMS.txt and runs it. install.py then downloads and checks the rest. Arguments go to install.py:
#
#   curl -fsSL .../install.sh | sh -s -- --register none
#
# HOI4_LIVE_VERSION picks a release other than the latest. HOI4_LIVE_RELEASE_BASE names another release
# location: an https address shaped like the GitHub releases page, or a local folder holding the release files.
set -eu

BASE=${HOI4_LIVE_RELEASE_BASE:-https://github.com/klimPaskov/HOI4-Live-Tools/releases}
BASE=${BASE%/}
VERSION=${HOI4_LIVE_VERSION:-}

say() { printf '%s\n' "$*" >&2; }

find_python() {
  for candidate in python3 python3.13 python3.12 python3.11 python3.10 python3.9 python; do
    found=$(command -v "$candidate" 2>/dev/null) || continue
    # The macOS /usr/bin/python3 placeholder opens an install dialog when the command line tools are absent.
    if [ "$(uname -s)" = Darwin ] && [ "$found" = /usr/bin/python3 ] && ! xcode-select -p >/dev/null 2>&1; then
      continue
    fi
    if "$found" -c 'import sys; sys.exit(0 if sys.version_info >= (3, 9) else 1)' >/dev/null 2>&1; then
      printf '%s\n' "$found"
      return 0
    fi
  done
  return 1
}

fetch() {
  # fetch NAME FILE
  case $BASE in
    https://*)
      if [ -n "$VERSION" ]; then url=$BASE/download/v$VERSION/$1; else url=$BASE/latest/download/$1; fi
      if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$url" -o "$2"
      elif command -v wget >/dev/null 2>&1; then
        wget -q "$url" -O "$2"
      else
        say "error: neither curl nor wget is installed; install one and run this again"
        return 1
      fi ;;
    *://*)
      say "error: HOI4_LIVE_RELEASE_BASE must be an https address or a local folder"
      return 1 ;;
    *)
      cp "$BASE/$1" "$2" ;;
  esac
}

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

PY=$(find_python) || {
  say "error: Python 3.9 or newer is needed and was not found."
  if [ "$(uname -s)" = Darwin ]; then
    say "Install it from https://www.python.org/downloads/ (or run: xcode-select --install), then run this again."
  else
    say "Install it with your package manager (for example: sudo apt install python3 python3-venv), then run this again."
  fi
  exit 2
}

TMP=$(mktemp -d "${TMPDIR:-/tmp}/hoi4live.XXXXXX")
trap 'rm -rf "$TMP"' EXIT INT TERM

fetch SHA256SUMS.txt "$TMP/SHA256SUMS.txt" || { say "error: could not download the release checksums"; exit 1; }
fetch install.py "$TMP/install.py" || { say "error: could not download install.py"; exit 1; }
EXPECTED=$(awk '$2 == "install.py" || $2 == "*install.py" {print $1}' "$TMP/SHA256SUMS.txt")
ACTUAL=$(sha256_of "$TMP/install.py")
if [ -z "$EXPECTED" ] || [ "$EXPECTED" != "$ACTUAL" ]; then
  say "error: install.py does not match its checksum in SHA256SUMS.txt; nothing was installed."
  exit 1
fi

if [ -n "$VERSION" ]; then
  "$PY" "$TMP/install.py" --version "$VERSION" "$@"
else
  "$PY" "$TMP/install.py" "$@"
fi
