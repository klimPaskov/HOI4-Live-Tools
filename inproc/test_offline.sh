#!/bin/zsh
# test_offline.sh [EXE_1_19_4_1] [EXE_1_19_3] [EXE_1_20_0_0]: the module's scanner run over the game's executable FILES, checked
# against values found by hand in the disassembly and against the external runtime's tables (native/src/game.rs
# MACOS layout: idler_global 0x372afc0, gs_vtable 0x35044e8, idler_vtable 0x35089d8). Never touches a running game.
# Also the load test: the module injected into a stand-in x86_64 host (not the game) through DYLD_INSERT_LIBRARIES.
cd "${0:A:h}"
# The executables are the owner's game files and stay outside the repository: pass them as arguments, or set
# HOI4_VERSIONS (a folder with 1.19.4.1-f584/hoi4.bin and 1.19.3-c01a/hoi4.bin) and HOI4_V120 (the 1.20.0.0 macOS
# executable; optional).
NEW=${1:-$HOI4_VERSIONS/1.19.4.1-f584/hoi4.bin}
OLD=${2:-$HOI4_VERSIONS/1.19.3-c01a/hoi4.bin}
V120=${3:-$HOI4_V120}
[ -f "$NEW" ] && [ -f "$OLD" ] || { echo "usage: test_offline.sh EXE_1_19_4_1 EXE_1_19_3 [EXE_1_20_0_0] (or set HOI4_VERSIONS)"; exit 2; }
S1=(auth_slot=0x98 speed_type_id=0x2f42 speed_payload=0x24 speed_field=0x47c speed_range=5 speed_gs_global=0x10372b040 forbid_desc=0x103715808)
fail=0
echo "== 1.19.4.1 f584 ($NEW)"
./build/scan_file $NEW command_post=0x10284d4a0 queue_root=0x10372afc0 queue_slot=0x90 \
  research_factory=0x10022bb70 research_vtable=0x1034b7fa8 research_type_id=0x2f70 \
  gamestate_vtable=0x1035044e8 idler_vtable=0x1035089d8 fields=1 $S1 | grep -E "^expect|resolved" || fail=1
[ ${pipestatus[1]} = 0 ] || fail=1
echo "== 1.19.3 c01a ($OLD): same scans, different addresses; no field table (reads off by design)"
./build/scan_file $OLD command_post=0x1026d57a0 queue_root=0x1035196c0 queue_slot=0x90 \
  research_factory=0x100218c50 research_vtable=0x1032ba448 research_type_id=0x2f70 \
  gamestate_vtable=0x103306260 idler_vtable=0x103308f90 fields=0 auth_slot=0 speed_type_id=0 forbid_desc=0x103505990 \
  | grep -E "^expect|resolved" || fail=1
[ ${pipestatus[1]} = 0 ] || fail=1
echo "   (1.19.3's Post has no authorisation step: post_layout and speed_command stay OFF there, by design)"
if [ -f "$V120" ]; then
  echo "== 1.20.0.0 macOS ($V120): every scan incl. stage 1 resolves; no field table yet (reads: speed only)"
  ./build/scan_file $V120 command_post=0x1028505d0 queue_root=0x103730150 queue_slot=0x90 research_factory=0x10022c2b0 \
    research_vtable=0x1034bc180 research_type_id=0x2f70 gamestate_vtable=0x103508988 idler_vtable=0x10350cea0 fields=0 \
    auth_slot=0xa0 speed_type_id=0x2f42 speed_payload=0x24 speed_field=0x47c speed_range=5 speed_gs_global=0x1037301d0 \
    forbid_desc=0x10371a988 | grep -E "^expect|resolved" || fail=1
  [ ${pipestatus[1]} = 0 ] || fail=1
fi

echo "== 1.19.4.1 through the PROCESS view (segments laid out at a nonzero slide, as dyld maps them)"
SCAN_AS_PROCESS=1 ./build/scan_file $NEW command_post=0x10284d4a0 queue_root=0x10372afc0 queue_slot=0x90 \
  research_factory=0x10022bb70 research_vtable=0x1034b7fa8 research_type_id=0x2f70 \
  gamestate_vtable=0x1035044e8 idler_vtable=0x1035089d8 fields=1 $S1 2>&1 | grep -E "^expect|resolved|process view" || fail=1
[ ${pipestatus[1]} = 0 ] || fail=1

echo "== load test: stand-in host, not the game"
T=$(mktemp -d); mkdir -p $T/Fake.app/Contents/MacOS; cp build/fake_host $T/Fake.app/Contents/MacOS/hoi4
HOI4_INPROC_LOG=$T/inproc.log HOI4_INPROC_DELAY_S=1 HOI4_INPROC_POST=set_game_speed HOI4_INPROC_SOCK=$T/s.sock DYLD_INSERT_LIBRARIES=$PWD/build/libhoi4live_inproc.dylib \
  $T/Fake.app/Contents/MacOS/hoi4 4 > $T/host.out; rc=$?
cat $T/host.out; cat $T/inproc.log
grep -q '"dyld_insert_libraries_after_load":null' $T/host.out || { echo "FAIL: environment not cleaned"; fail=1; }
grep -q '"stage1_switches_after_load":null' $T/host.out || { echo "FAIL: stage-1 switches left in the environment"; fail=1; }
[ -e $T/s.sock ] && { echo "FAIL: channel opened in a non-game image"; fail=1; }
grep -q '"event":"loaded"' $T/inproc.log || { echo "FAIL: module did not load"; fail=1; }
grep -q '"event":"inert"' $T/inproc.log || { echo "FAIL: module did not go inert on a non-game image"; fail=1; }
[ $rc = 0 ] || { echo "FAIL: host exit $rc"; fail=1; }
echo "== load test: another program name (module must stay silent)"
cp build/fake_host $T/other; HOI4_INPROC_LOG=$T/other.log DYLD_INSERT_LIBRARIES=$PWD/build/libhoi4live_inproc.dylib $T/other 0 >/dev/null
[ -e $T/other.log ] && { echo "FAIL: module acted in a non-game process"; fail=1; } || echo "silent: ok"
rm -rf $T
echo "== the channel and the posting policy against a mock game (stage 1)"
python3 -I tools/ipc_test.py build | grep -E "FAIL|ALL PASS|FAILED" || fail=1
[ ${pipestatus[1]} = 0 ] || fail=1
echo "== main-thread queue inside an SDL-style Cocoa event pump (the route the module uses to reach the UI's thread)"
./build/pump_host || fail=1
[ $fail = 0 ] && echo "ALL PASS" || echo "FAILED"
exit $fail
