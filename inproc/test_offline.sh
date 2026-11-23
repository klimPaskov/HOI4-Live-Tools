#!/bin/zsh
# test_offline.sh [EXE_1_19_4_1] [EXE_1_19_3]: the module's scanner run over the game's executable FILES, checked
# against values found by hand in the disassembly and against the external runtime's tables (native/src/game.rs
# MACOS layout: idler_global 0x372afc0, gs_vtable 0x35044e8, idler_vtable 0x35089d8). Never touches a running game.
# Also the load test: the module injected into a stand-in x86_64 host (not the game) through DYLD_INSERT_LIBRARIES.
cd "${0:A:h}"
NEW=${1:-$HOME/Projects/_scratch/hoi4-versions/1.19.4.1-f584/hoi4.bin}
OLD=${2:-$HOME/Projects/_scratch/hoi4-versions/1.19.3-c01a/hoi4.bin}
fail=0
echo "== 1.19.4.1 f584 ($NEW)"
./build/scan_file $NEW command_post=0x10284d4a0 queue_root=0x10372afc0 queue_slot=0x90 \
  research_factory=0x10022bb70 research_vtable=0x1034b7fa8 research_type_id=0x2f70 \
  gamestate_vtable=0x1035044e8 idler_vtable=0x1035089d8 fields=1 | grep -E "^expect|resolved" || fail=1
[ ${pipestatus[1]} = 0 ] || fail=1
echo "== 1.19.3 c01a ($OLD): same scans, different addresses; no field table (reads off by design)"
./build/scan_file $OLD command_post=0x1026d57a0 queue_root=0x1035196c0 queue_slot=0x90 \
  research_factory=0x100218c50 research_vtable=0x1032ba448 research_type_id=0x2f70 \
  gamestate_vtable=0x103306260 idler_vtable=0x103308f90 fields=0 | grep -E "^expect|resolved" || fail=1
[ ${pipestatus[1]} = 0 ] || fail=1

echo "== 1.19.4.1 through the PROCESS view (segments laid out at a nonzero slide, as dyld maps them)"
SCAN_AS_PROCESS=1 ./build/scan_file $NEW command_post=0x10284d4a0 queue_root=0x10372afc0 queue_slot=0x90 \
  research_factory=0x10022bb70 research_vtable=0x1034b7fa8 research_type_id=0x2f70 \
  gamestate_vtable=0x1035044e8 idler_vtable=0x1035089d8 fields=1 2>&1 | grep -E "^expect|resolved|process view" || fail=1
[ ${pipestatus[1]} = 0 ] || fail=1

echo "== load test: stand-in host, not the game"
T=$(mktemp -d); mkdir -p $T/Fake.app/Contents/MacOS; cp build/fake_host $T/Fake.app/Contents/MacOS/hoi4
HOI4_INPROC_LOG=$T/inproc.log HOI4_INPROC_DELAY_S=1 DYLD_INSERT_LIBRARIES=$PWD/build/libhoi4live_inproc.dylib \
  $T/Fake.app/Contents/MacOS/hoi4 4 > $T/host.out; rc=$?
cat $T/host.out; cat $T/inproc.log
grep -q '"dyld_insert_libraries_after_load":null' $T/host.out || { echo "FAIL: environment not cleaned"; fail=1; }
grep -q '"event":"loaded"' $T/inproc.log || { echo "FAIL: module did not load"; fail=1; }
grep -q '"event":"inert"' $T/inproc.log || { echo "FAIL: module did not go inert on a non-game image"; fail=1; }
[ $rc = 0 ] || { echo "FAIL: host exit $rc"; fail=1; }
echo "== load test: another program name (module must stay silent)"
cp build/fake_host $T/other; HOI4_INPROC_LOG=$T/other.log DYLD_INSERT_LIBRARIES=$PWD/build/libhoi4live_inproc.dylib $T/other 0 >/dev/null
[ -e $T/other.log ] && { echo "FAIL: module acted in a non-game process"; fail=1; } || echo "silent: ok"
rm -rf $T
[ $fail = 0 ] && echo "ALL PASS" || echo "FAILED"
exit $fail
