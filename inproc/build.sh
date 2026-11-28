#!/bin/zsh
# build.sh: build the in-process module (stage 0 reads, stage 1 one command) (x86_64: the macOS game runs as x86_64 under Rosetta on Apple
# silicon) and the offline scanner. Apple clang only; no Rust target or other install needed. Output: build/.
set -e
cd "${0:A:h}"
mkdir -p build
CF=(-arch x86_64 -mmacosx-version-min=11.0 -O2 -Wall -Wextra -Wno-missing-field-initializers -std=c11) # spec rows leave trailing fields zero on purpose
xcrun clang $CF -o build/scan_file tools/scan_file.c core/scan.c core/caps.c core/cmd.c core/cmd2.c core/orders.c core/mio.c
xcrun clang $CF -dynamiclib -fvisibility=hidden -install_name @rpath/libhoi4live_inproc.dylib \
  -o build/libhoi4live_inproc.dylib macos/module.c core/scan.c core/caps.c core/cmd.c core/cmd2.c core/orders.c core/mio.c core/ipc.c
xcrun clang $CF -o build/orders_test tools/orders_test.c core/orders.c core/scan.c core/cmd.c
xcrun clang $CF -o build/mio_test tools/mio_test.c core/mio.c
xcrun clang $CF -o build/fake_host tools/fake_host.c
xcrun clang $CF -o build/ipc_selftest tools/ipc_selftest.c core/ipc.c
xcrun clang -arch x86_64 -mmacosx-version-min=11.0 -O2 -fobjc-arc -framework AppKit -o build/pump_host tools/pump_host.m
codesign --force --sign - build/libhoi4live_inproc.dylib 2>/dev/null
file build/libhoi4live_inproc.dylib build/scan_file
shasum -a 256 build/libhoi4live_inproc.dylib
