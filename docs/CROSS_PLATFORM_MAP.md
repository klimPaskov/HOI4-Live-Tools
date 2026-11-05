# macOS to Linux address map

`scripts/cross_platform_map.py` maps addresses and struct field offsets from
the macOS HOI4 binary (Mach-O x86_64, libc++) to the Linux binary (ELF x86_64,
libstdc++) of the same game version. It reads both binaries as data and never
runs them. The point is to port the macOS-only tools to Linux without
repeating the live mapping by hand.

## How it works

- **Vtables**: both builds keep Itanium RTTI. The tool reads the class name from
  the typeinfo before the macOS address point. It then takes the ELF vtable with
  the same name and the same address-point ordinal. ELF pointers are read
  through `R_X86_64_RELATIVE` relocations.
- **Functions**: the tool pairs functions in four steps:
  1. Functions in the same slot of the same class's vtable.
  2. Functions that share a string literal no other function uses.
  3. Callee propagation: the call lists of matched pairs are aligned, with known
     pairs as anchors. This runs for 12 rounds.
  4. Each pair must pass a size sanity check.

  On 1.19.3 this paired 67,284 of about 130,000 functions.
- **Globals**: the tool finds each macOS instruction that references the
  address, either directly or through its `__got` slot, inside a matched
  function. It aligns that instruction with the ELF twin by instruction shape
  (registers and numbers removed). Each function pair gives one vote for the
  ELF target.
- **Fields**: an item names the class by its macOS vtable. The tool collects
  candidate functions from:
  - the class's virtual functions,
  - constructors and destructors (code that stores the vtable),
  - optional anchor functions,
  - users of a "context" global that holds the object pointer.

  Only displacements on a register that holds the object vote, on both sides.
  The object register is tracked through copies, `lea`/load of the global and
  stack spills. No constant delta is used, because libc++ and libstdc++
  members differ in size.
- **Vote rule** (nothing is guessed):
  - **high**: at least 3 votes, at least twice the runner-up and at least 75%
    of all votes.
  - **medium**: at least 2 votes and at least twice the runner-up.
  - **low**: exactly one uncontested vote.
  - **unresolved**: anything else. The value is left empty and the tallies go
    in `notes`.

## Validation on 1.19.3

Inputs:
- macOS Mach-O `089EA519` and Linux ELF `a4971c9e`.
- The ground truth is `scripts/data/xmap-1.19.3-groundtruth.json`. It has 75
  pairs, taken from the `LINUX` and `MACOS` layouts in `native/src` at
  `2e61a70^` and paired by field name. The files are `game.rs`, `own_focus.rs`,
  `research.rs`, `army.rs`, `production_lines.rs` and `construction.rs`.
- The pairs are 29 vtables, 7 globals and 39 field offsets. None of the 39
  offsets is the same on both platforms.

| kind    | pairs | correct | wrong | unresolved |
|---------|------:|--------:|------:|-----------:|
| vtable  | 29    | 29      | 0     | 0          |
| global  | 7     | 7       | 0     | 0          |
| field   | 39    | 30      | 1     | 8          |
| **all** | 75    | **66**  | **1** | **8**      |

By confidence:

| confidence | correct | wrong |
|------------|--------:|------:|
| high       | 48      | 0     |
| medium     | 12      | 0     |
| low        | 6       | 1     |

So "high" and "medium" were always right (60 of 60). "Low" was right 6 times
out of 7.

Thirteen of the field items carry a `context` global: `gs_global` for
`CCurrentGameState`, `idler_global` for the idler, and the technology and
equipment database pointers. Without context and register tracking, the
`CCurrentGameState` fields all stayed unresolved. A first version that voted on
every aligned displacement got 3 fields wrong, so object-register tracking is
what keeps the error count at one.

Why the rest failed:

- **Wrong (1)**: `construction.template_key`, mac `+0x228`, expected ELF
  `+0x248`, the tool gave `+0x258` (low, one vote). This field is a
  `std::string`. Inlined string code touches a different sub-member on each
  ABI. On Linux, `+0x258` is the libstdc++ inline buffer of the string at
  `+0x248`.
- **Unresolved (8)**:
  - `playthrough` and `focus_name`: string fields, for the same reason.
  - `state_id`, `province_status`, `building_status`, `template_threshold`,
    `variant_type` and the army `province_id`: no matched function of the class
    uses the offset on the object register. These are read through other
    objects, for example `CProvince` from a division, or the offset only
    appears in unmatched functions. Giving an anchor function or a context
    global would let the tool try them.

## Items for 1.19.4.1

`scripts/data/macos-1.19.4.1-items.json` was made by
`cross_platform_map.py collect <1.19.4.1 Mach-O> native/src/launch/macos.rs native/src/launch/macos --root .`.
It lists every hex literal that falls inside the 1.19.4.1 Mach-O image (UUID
`DFA875A5`), classified against that binary.

Production code (not test fixtures) holds 152 image addresses:

| class    | count | notes |
|----------|------:|-------|
| vtable   | 140   | RTTI name recorded for each |
| global   | 7     | 4 named `TGameItemDatabase<...>::_pInstance`, 3 unnamed pointer globals |
| function | 3     | slots of a window/video class read by `construction_place/point.rs` |
| objc     | 2     | `__objc_ivar` data, which has no Linux counterpart |

Test-only literals (fixtures) are listed too, with `tests_only: true`.

A further 182 small production literals are struct offsets or plain numbers.
They are listed under `fields_needing_owner` with their source lines. They can
only be mapped once someone attaches the owning class's vtable, an anchor
function or a context global.

As a dry run, the 1.19.4.1 macOS items were mapped against the 1.19.3 ELF. This
is cross-version, so the result is not a validation. Even so, 146 of the 152
resolved by structure:
- 139 vtables (12 at medium, because the slot counts changed between versions),
- all 7 globals.

The 6 that did not resolve:
- `CNationalFocusInfoWindow`, which is new in 1.19.4,
- the 3 window/video slot functions, which are probably platform-specific,
- the 2 Objective-C items.

## Expected manual work

- About 95% of the image addresses (vtables and globals) should map
  automatically, given 36 of 36 correct on 1.19.3. The 3 functions and 2
  Objective-C items need a person; they are probably macOS-only.
- Field offsets are the bulk of the manual work. The 182 bare offsets need an
  owning class attached first. Once they have one, expect about 80% to map
  (30 of 39 correct, 1 wrong on 1.19.3). The rest will be string members and
  fields only touched through other objects.
- Treat every **low** result as a candidate to check live. Treat std::string
  members as manual.

## Weak spots

- Function pairing has no independent check. The two builds share no
  function symbol names, and the ELF is stripped. Its accuracy shows only
  through the global and field results above.
- The register tracking ignores control flow. A function that reuses a
  register for another object on one branch can still cast a stray vote.
- Classes with platform-only virtual functions (the window, input and video
  layers) do not line up slot for slot. Their vtables still map by name, but
  their functions do not.

## Generating the Linux table for `xlayout`

`scripts/gen_linux_layout.py` builds the file that `native/src/xlayout.rs`
reads on Linux. It has three commands.

**`keys`** scans `native/src` and lists every key the tools use. It reads:

- class tags, including multi-line `Class::new(` calls, inline tags and
  registry structs such as `panelkit::Class`;
- every `img()` address;
- every `fld()` key it can resolve from source.

The scan follows these forms to find a concrete offset:

- literals and constant expressions;
- constants reached through a module path or a `use .. as` alias;
- `m.field`, `self.field` and `m.field[i]` read from the macOS struct-literal
  tables in the same file;
- closures and helper functions that pass their arguments on to `fld` or `img`;
- parent links: when code reads `let v = r.u64(x + fld(.., &P, OFF)?)?` and
  then uses `v + fld(.., &C, ..)`, the scan records that `C` objects are
  reached through field `P+OFF`.

Calls whose class or offset is only known at run time are listed as
unresolved, with a reason.

**`generate MACHO ELF [--hints H]`** maps those keys and writes two files:

- the table, `native/layouts/linux-<gnu-build-id>.json`, in the
  `Table::parse` format;
- a coverage report next to it.

Image keys are:

- the `img()` arguments,
- every class vtable,
- every image literal in files that use `xlayout`.

Fields are mapped class by class in a class-wide mode. Every offset that has
evidence gets an entry, so keys the static scan missed are covered too. A class
gets its evidence from one of these sources:

- its vtable (virtual functions and constructors);
- its RTTI name, which is tried automatically for vtable-0 tags that are real
  C++ class names;
- a hint (`anchors`, or `context` pointer globals);
- `via`: the parent object and the field that holds it. These links come from
  hints or from the parent links the scan found.

Context, anchor and `via` functions also pass their object to callees in `rdi`.
Those direct callees are inspected as well, one level deep.

Only high- and medium-confidence values are written. Add `--include-low` to
also write single-vote results. Negative displacements are dropped.
`scripts/data/xlayout-hints-1.19.4.1.json` is the starter hint set.

**`--check-table T [--keys K]`** checks a table against the `xlayout` parser
rules. With a keys file it also reports coverage.

The smoke run mapped the macOS 1.19.4.1 binary against the 1.19.3 ELF. It is
cross-version, so its values are not valid and its numbers only show that the
pipeline runs end to end. On a same-version pair, expect field coverage nearer
the 1.19.3 validation above.

## Running

Install capstone in a private venv, then:

```
python3 -m venv ~/Projects/_scratch/xmap-venv && ~/Projects/_scratch/xmap-venv/bin/pip install capstone
~/Projects/_scratch/xmap-venv/bin/python scripts/cross_platform_map.py MACHO ELF scripts/data/xmap-1.19.3-groundtruth.json --out result.json
```

The first run disassembles both binaries, about two minutes each, and pairs
functions, about ten minutes. Both results are cached in `~/.cache/hoi4-xmap`.
Later runs take seconds per item. The unit tests (`tests/test_cross_platform_map.py`)
need neither capstone nor the game.
