# Windows layout recovered from static executable bytes

11 October 2026. Analysis used only the on-disk executable and read-only reference
research. No process was opened, queried, read, suspended, or modified; in particular
the user-owned PID 22000 was not accessed. No game functions were called.

## Build identity

| Field | Value |
|---|---|
| File | `C:/Program Files (x86)/Steam/steamapps/common/Hearts of Iron IV/hoi4.exe` |
| SHA-256 | `870ebf1f615262facca48a3b947775c41f0a5338f3068c40958209f5c41c65ad` |
| File size | 55,680,632 bytes |
| PE timestamp | `0x6a427fb9` |
| Existing build key | `6a427fb9_3519e78` |
| Preferred image base | `0x140000000` |

All addresses below are RVAs. Global-root addresses require the runtime module base.
Findings are specific to these bytes; the SHA-256 provides a stronger identity check
than timestamp and file length alone.

## Played country: `+0x520` is a tag index

The direct-country-index interpretation in the earlier handoff is contradicted by
the Windows code. The engine resolves this field through the tag mapping array:

```text
gs = pointer at module_base + 0x3316e90
tag = signed int32 at gs + 0x520
index = signed int32 at pointer(gs + 0x340) + 4*tag
country = pointer at pointer(gs + 0x310) + 8*index
```

Concrete chain, with intervening debug assertions omitted:

```asm
01372438: mov  rax, qword ptr [rip + 0x1fa4a51] ; global RVA 0x3316e90
0137243f: mov  edx, 0x520
01372444: mov  ecx, 0x524
01372449: cmp  dword ptr [rax + 0x520], 0
01372450: cmovg ecx, edx
01372453: add  rcx, rax
0137245a: jmp  0xba56a0

00ba5797: cmp  dword ptr [rbx], 0              ; rbx is incoming tag address
00ba579a: mov  rdi, qword ptr [rip + 0x27716ef] ; global RVA 0x3316e90
00ba57a1: jne  0xba57a7
00ba57a3: xor  eax, eax                        ; zero tag bypasses mapping
00ba57a5: jmp  0xba57af
00ba57a7: mov  rcx, rbx
00ba57aa: call 0xba6240

00ba62ee: mov  rcx, qword ptr [rip + 0x2770b9b] ; global RVA 0x3316e90
00ba62f5: movsxd rdx, dword ptr [rbx]
00ba62f8: mov  rax, qword ptr [rcx + 0x340]
00ba62ff: mov  eax, dword ptr [rax + rdx*4]
00ba6302: add  rsp, 0x30
00ba6306: pop  rbx
00ba6307: ret

00ba57b4: movsxd rcx, eax
00ba57b7: mov  rax, qword ptr [rdi + 0x310]
00ba57be: mov  rax, qword ptr [rax + rcx*8]
00ba57c2: add  rsp, 0x30
00ba57c6: pop  rdi
00ba57c7: ret
```

Independent corroboration: `0xba60a0` compares two tag objects by comparing their
mapped indexes, at `0xba61c1` through `0xba61d8`. `0xba5c20` indexes the game state's
tag-name array at `+0x358` by the input tag, using 32-byte elements.

The engine's general resolver accepts zero by returning `countries[0]`. This does
not establish zero as a playable country. A played-country reader should require
a positive in-bounds tag, an in-bounds mapped index, and a valid country object.
Equality of `tag` and `index` is not required and cannot validate the interpretation.

The caller shown above selects `+0x524` when `+0x520 <= 0`. The meaning of `+0x524`
was not recovered. It is not justified as a reader fallback. Dynamic/civil-war tags
must resolve through the mapping and object rather than a static tag-name list.

## Date: inline object `+0x460`, signed hour counter `+0x468`

The date is reachable directly from the same game-state root. Multiple telemetry
paths format `gs+0x460` and attach the result to the literal `in_game_date` at RVA
`0x270f188`. For example:

```asm
001ff406: mov  rcx, qword ptr [rip + 0x3117a83] ; global RVA 0x3316e90
001ff40d: add  rcx, 0x460
001ff414: mov  r8b, 0x2d                       ; separator '-'
001ff417: lea  rdx, [rsp + 0x58]               ; output string
001ff41c: call 0x526f20
001ff422: lea  rcx, [rip + 0x250fd5f]          ; literal in_game_date
...
001ff442: mov  r8, rax
001ff44e: call 0x2069a20                      ; attach string value
```

The formatter reads the inline date object's signed 32-bit value at `+8` and
subtracts a Windows epoch bias:

```asm
00526f3d: mov  edi, dword ptr [rcx + 8]
00526f43: sub  edi, 0x29c55c0
00526f50: call 0x177140                       ; zero-based month
00526f58: lea  ebp, [rax + 1]
00526f5b: call 0x176db0                       ; zero-based day of month
00526f63: lea  r8d, [rax + 1]
```

`0x29c55c0 = 43,800,000 = 5000*365*24`. The formatter subsequently performs signed
division by 8760 through the `0xef66a241` reciprocal multiply and shift. Functions
`0x177140` and `0x176db0` independently read `date+8`, subtract the same bias,
divide by 24 and 365, and use a 12-entry month-length table at RVA `0x306d210`:
`31,28,31,30,31,30,31,31,30,31,30,31`. This is a 365-day calendar, including years
that would be Gregorian leap years.

For normal positive gameplay years the decode is:

```python
hours = read_i32(gs + 0x468) - 43_800_000
days, hour = divmod(hours, 24)
year, zero_based_day_of_year = divmod(days, 365)
# Resolve month/day through the non-leap month lengths; displayed day is +1.
```

Thus the expected raw value for `1936.1.1.0` is **60,759,360**, and one day advances
it by 24. Scanning for the Linux unshifted value around 16,959,360 would miss this
Windows field. The epoch is an encoding property, not a runtime pointer offset.

Additional corroboration: the date-mutating routine `0x527350` removes the epoch
at `0x527372` (`add edi, 0xfd63aa40`, the negative bias), computes day/month
arithmetic with factor `0x16d`, adds `0x29c55c0` at `0x527432`, then writes the
result to `date+8` at `0x527438`.

This establishes the offset and encoding statically. No live date comparison was
performed. Consumers should retain build checks, pointer validation, plausible
date bounds, and snapshot consistency checks before reporting a live result.

## Pause and speed

Paused is a byte at `idler+0x6c1`, where `idler` is the pointer at module base plus
`0x33172e0`. RTTI identifies the idler vtable at RVA `0x2952510` (offset zero).
The `CPauseGame` vtable's `+0x50` slot points to `0xdd3b50`; this routine loads
the idler and dispatches to its `+0x290` slot, which points to `0xdc6520`:

```asm
00dd3b73: mov  rcx, qword ptr [rip + 0x2543766] ; global RVA 0x33172e0
00dd3b7a: lea  r8, [rbx + 0x28]
00dd3b7e: movzx edx, byte ptr [rbx + 0x48]
00dd3b82: mov  rax, qword ptr [rcx]
00dd3b8a: jmp  qword ptr [rax + 0x290]
...
00dc6954: movzx ecx, byte ptr [rbx + 0x6c1]    ; rbx is idler
00dc695b: test cl, cl
00dc695d: sete al
00dc6960: mov  byte ptr [rbx + 0x6c1], al      ; toggle
```

The independent idler vtable `+0x2e8` getter is the leaf function `0xdc3db0`:
`movzx eax, byte ptr [rcx+0x6c1]; ret`. Branches within `0xdc6520` also explicitly
write 0 or 1 to this field. No Windows offset shift applies to this pause byte.
The command is subject to branches and multiplayer conditions before the toggle;
these bytes do not establish a safe command-calling ABI or authorize any calls.

Speed is a signed 32-bit **zero-based setting at `gs+0x4bc`**, range `0..4`, rather
than a field in the idler. RTTI identifies `CSetGameSpeedCommand`; its vtable
`+0x50` slot points to `0xef1f80`. The final fragment dispatches to a clamped setter:

```asm
00ef2081: mov  edx, dword ptr [rdi + 0x28]     ; command argument
00ef2084: mov  rcx, qword ptr [rip + 0x2424e05] ; global RVA 0x3316e90
00ef2090: jmp  0x1edad0

001edadb: test edx, edx
001edadd: jns  0x1edae3
001edadf: xor  edx, edx
001edae1: jmp  0x1edaed
001edae3: mov  eax, 4
001edae8: cmp  edx, eax
001edaea: cmovg edx, eax
001edaed: mov  dword ptr [rcx + 0x4bc], edx
```

`CIncreaseGameSpeedCommand` independently reads `gs+0x4bc` at `0xef1f50`, increments
it at `0xef1f56`, then tailcalls the same setter at `0xef1f6d`. A UI-facing speed
of 1 through 5 can represent this zero-based setting plus one; the UI label mapping
itself was not independently recovered here. Pause is a separate flag, so speed
zero should not be interpreted as paused.

## Research slots: structural corroboration

RTTI identifies `CTechnologyStatus` at vtable RVA `0x295e378` and `CResearchSlot`
at `0x295e328`. The technology status contains an array of research-slot pointers
at `+0xa0` with its signed count at `+0xac`. Each slot has its owner status pointer
at `+0x10` and current technology pointer at `+0x18` (zero when empty).

The status load/member routine `0xecc9d0` iterates `status+0xa0` and `status+0xac`
at `0xecca2f` through `0xecca5f`. In the same routine, a technology object is
selected from `status+0x88` by its template's id (`template+0x3c`), then used to
construct a research-slot object:

```asm
00eccb3d: mov  ecx, 0x48
00eccb42: call 0x21b4b50                      ; allocate 0x48-byte slot
...
00eccb5f: lea  rax, [rip + 0x1a917c2]        ; CResearchSlot vtable 0x295e328
00eccb66: mov  qword ptr [rbx], rax
00eccb69: mov  qword ptr [rbx + 0x10], r15    ; owner CTechnologyStatus
00eccb6d: mov  qword ptr [rbx + 0x18], rsi    ; technology selected above
00eccb71: mov  qword ptr [rbx + 0x20], r14    ; r14 is zero
00eccb75: mov  qword ptr [rbx + 0x28], r14
00eccb79: mov  qword ptr [rbx + 0x30], r14
00eccb7d: mov  qword ptr [rbx + 0x38], 0x186a0
```

Independent corroboration: the RTTI-confirmed constructor `0xebdb60` initializes
`slot+0x18` to zero at `0xebdba6`; status serializer `0xed02a0` loops `+0xa0/+0xac`
and compares each `slot+0x18` against a candidate technology at `0xed03de` through
`0xed03f8`.

The slot's `+0x20`, `+0x28`, and `+0x38` qwords are serialized via fixed-point
writer `0x24ae590` in `0xecff80`, with token ids `0x31de`, `0x34bb`, and `0x3684`.
Their token names were not recovered here. The Linux reference labels `+0x20`
progress, but this report does **not** promote that label to a verified Windows
field. Technology names, research completion cost, and progress fractions need
additional evidence. The country-to-status pointer at `country+0xf60` remains
supported by the earlier handoff's live observation, rather than this static pass.

## Reproduction

`scripts/memory_spike/static_layout.py` imports the existing file-only PE reader
and Capstone. Its implementation contains no process-access API. Dependencies
used here: existing NumPy and Capstone 5.0.9, installed into the ignored isolated
environment `artifacts/static-layout/venv`.

```powershell
artifacts/static-layout/venv/Scripts/python.exe scripts/memory_spike/static_layout.py --xref 0x3316e90 --contains 0x520 --shortest-first --context 16 --limit 20
artifacts/static-layout/venv/Scripts/python.exe scripts/memory_spike/static_layout.py --function 0xba56a0 --length 0x128
artifacts/static-layout/venv/Scripts/python.exe scripts/memory_spike/static_layout.py --function 0xba6240 --length 0xc8
artifacts/static-layout/venv/Scripts/python.exe scripts/memory_spike/static_layout.py --function 0x1fef10 --contains 0x460 --context 12
artifacts/static-layout/venv/Scripts/python.exe scripts/memory_spike/static_layout.py --function 0x526f20
artifacts/static-layout/venv/Scripts/python.exe scripts/memory_spike/static_layout.py --function 0x177140 --length 0x61
artifacts/static-layout/venv/Scripts/python.exe scripts/memory_spike/static_layout.py --function 0xdc6520 --contains 0x6c1 --context 4
artifacts/static-layout/venv/Scripts/python.exe scripts/memory_spike/static_layout.py --function 0xdc3db0 --length 8
artifacts/static-layout/venv/Scripts/python.exe scripts/memory_spike/static_layout.py --function 0x1edad0 --length 0x23
artifacts/static-layout/venv/Scripts/python.exe scripts/memory_spike/static_layout.py --function 0xecc9d0 --contains 0xa0 --context 12
artifacts/static-layout/venv/Scripts/python.exe scripts/memory_spike/static_layout.py --function 0xebdb60
```

`.pdata` contains split unwind fragments, so a single table entry is not always a
whole source-level function. `--length` deliberately spans fragments; choose an
instruction-aligned start. The RIP xref search covers REX-prefixed `mov`, `lea`,
and store encodings only, validates candidates with Capstone and instruction
boundaries, and is not a complete cross-reference database. Function names above
describe observed behavior inferred from call sites; the executable has no debug
symbols. Raw disassembly logs are in ignored `artifacts/static-layout/`.
