#!/bin/zsh
# build.sh: build the stage-0 in-process module (x86_64: the macOS game runs as x86_64 under Rosetta on Apple
# silicon) and the offline scanner. Apple clang only; no Rust target or other install needed. Output: build/.
set -e
cd "${0:A:h}"
mkdir -p build
CF=(-arch x86_64 -mmacosx-version-min=11.0 -O2 -Wall -Wextra -std=c11)
xcrun clang $CF -o build/scan_file tools/scan_file.c core/scan.c core/caps.c
xcrun clang $CF -dynamiclib -fvisibility=hidden -install_name @rpath/libhoi4live_inproc.dylib \
  -o build/libhoi4live_inproc.dylib macos/module.c core/scan.c core/caps.c
xcrun clang $CF -o build/fake_host tools/fake_host.c
codesign --force --sign - build/libhoi4live_inproc.dylib 2>/dev/null
file build/libhoi4live_inproc.dylib build/scan_file
shasum -a 256 build/libhoi4live_inproc.dylib
