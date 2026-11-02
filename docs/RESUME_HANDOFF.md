# Resume handoff — 23 October 2026

The user has resumed work. The selection checkpoint was published as `acd6f6e`
with all nine CI jobs passing. A fresh owned Mac session now has stationary
Find/camera evidence and a typed placement-readback foundation. Read
[the current checkpoint](LINUX_RUST_HANDOFF.md) for subsequent changes. The historical pause snapshot below preserves the tested
uncommitted state and its evidence; its old process has since exited with default
settings unchanged. Never reconnect to that expired session or rerun its driver.

## Start here after the user resumes

Read root `AGENTS.md`, this file, `LINUX_RUST_HANDOFF.md` and the campaign playbook.
The user has authorized native macOS again; the old goal text saying to stop Mac
tests is superseded. Linux, Windows and macOS remain required. Final campaign
videos must come from native macOS at 1920×1080 or higher. Preserve Linux work,
the legacy Python package and earlier failed campaigns. Do not control the shared
Windows game.

The current branch is `codex/linux-native-rust`. HEAD and the last published
checkpoint are `416739b921b79c70be63240e798e99b55bb26936`. Its nine CI jobs passed.
The new construction-selection implementation described below is **uncommitted
and unpublished**. Do not reset or discard it. There are also 38 unrelated
Python/package/test work-in-progress files; their private hash baseline must
remain unchanged. Do not stage them incidentally.

## Latest result

One Luna player at medium reasoning passed the recorded native Mac selection
test. One ordinary cursor move and left click selected `arms_factory` on the
already-open construction panel. Typed same-attempt readback confirmed ordinary
selection mode 1 and the original template, with the complete construction queue
unchanged. `state_id: 64` bound the original own-state scope; the action did not
select that state or place construction.

The selection RPC took **240.087 ms**. The full 18-call driver took **1.503 s**,
including one owned-app activation. There were no screenshots, clock calls,
worker starts, observation pauses or input retries. The last gameplay readback
was Germany, Normal, historical AI, paused at `1936.01.01.12`, speed 0. Research,
Focus, production and the full queue remained unchanged.

The native recording finalized successfully: **1920×1108 H.264, 67.938333 s,
1,999 frames**, containing a verified native 1920×1080 game client. Video
metadata was checked; there was no additional visual review in this turn.
This is component evidence. **No clean two-year Luna run or full Germany,
Nationalist China or Poland campaign has been recorded.**

## Current implementation

- `native/src/construction/mac_ui/selection_state.rs` retains immutable original
  process, campaign, own scope, catalog, target and complete queue facts. Fresh
  selection readback does not revisit old UI controls or route caches. Mode 0
  avoids the inactive pointer; mode 1 must be the exact ordinary template.
  Unsupported modes and incomplete queue identity refuse explicitly.
- `native/src/launch/macos/construction_select.rs` adds the default-off
  `hoi4_construction_select` tool. A strict durable intent precedes all movement.
  The original five-second action deadline and lease are never renewed. A
  genuinely already-selected target uses no input. Otherwise one move/click
  revalidates the original callback, geometry and fresh routing before down.
  Possible-input failures remain uncertain and cannot be replayed or settled
  by later UI state. Selection does not grant placement authority.
- `construction/mac_ui/geometry.rs` keeps the original requested control and
  rectangle through hover changes. `construction/mac_panel/routing.rs` checks
  complete route membership, cursor correlation and exact target/gates without
  claiming atomic delivery. Success still requires canonical game readback.
- `platform/macos_input.rs` preallocates move/down/up events before posting.
  The final guard follows allocation; release does not need allocation after
  possible down. The new ordinary-click hook checks the OS cursor and route.
- Registration, default-off configuration and focused tests are in
  `launch/macos.rs`, `launch/macos/construction_select/tests.rs` and
  `tests/support/mac_construction_selection_state.rs`. The existing opener's
  bounded read-only recovery helper is shared with selection.

The existing Sol reviewer found the mouse allocation issue; it was corrected
before final tests. The subsequent read-only review found no actionable defects.
No helper competed for live input. All source files were frozen through the gate.

## Validation and evidence

The current source passed all **1,611 Rust tests**, **956 Python tests with 37
skips**, formatting, strict Clippy on host macOS plus Windows GNU and Linux
x86_64/ARM64 musl targets, release build and the 28-tool offline MCP smoke.
These are local checks; the new source has not been committed or tested in CI.
Do not repeat broad checks without a new change or unresolved concern.

Private evidence belongs outside Git. The existing private Mac evidence root
contains `qualification-20261023-12` (called Q12 below). The local private
`Q12/RESUME_HANDOFF.md` records absolute paths and process identities.

- `Q12/construction-selection008/`: validation logs, candidate binary,
  typed-selection helper handoff and offline protocol output.
- `Q12/construction-select001/`: frozen driver/manifest, closed grant,
  original six-record journal prefix, saved prior config, calls, result and
  completed recording. Never rerun this one-shot driver.
- `Q12/construction-choice-geometry007/`: previous published geometry test,
  reviewed history audit and 38-file preserved-work baseline.
- `Q12/NEXT.json`: private continuation pointer.

The candidate's SHA-256 is
`3ce35f50ff624a0945d7bd0d685a8cd4f2d07440b51461bbf9f60d563d54b519`.
The frozen live manifest pins 270 source/binary/helper files. The tested input
configuration added only `construction_selection: true`; the previous config
was saved privately. That permission alone grants no input without a new lease.

## Stop state and next work

Luna returned exclusive input ownership. The grant is closed, lease release
verified, MCP exited on EOF with code 0, and the recorder exited successfully.
No worker is active. The retained journal now has eight rows: all original six
remain byte-exact, followed by the selection intent and Verified receipt.

The exact task-owned game and supervisor still existed at handoff. The game was
left paused; no new save, reload, quit, relaunch or settings restoration was
performed. The original six-hour supervisor deadline remains **23 October 2026,
13:33:28.834612 UTC**. Never renew it. After a later resume, inspect actual process
creation/executable/parent identity before trusting any live state. If it has
expired, inspect exit/restoration evidence before considering a new owned run.
Do not adopt a replacement PID or restore settings while the owned game is live.

After an explicit resume:

1. Revalidate the worktree, private manifest and ownership. The selection result
   includes complete player readback; a separate root-only fresh memory read
   after Luna was **not** run because the user stopped work. Read only if the
   original session remains valid; do not renew its lifetime or replay input.
2. Finish public documentation and NOTICE for the selection adapter. Audit the
   complete reachable history, stage only owned changes and publish the explicit
   branch. Inspect CI for that exact new commit. No release/tag/downstream pin
   update is authorized by this component result.
3. Implement ordinary state targeting and construction placement with original
   typed scope, complete queue-delta verification and durable receipts, then
   integrate construction into mixed standing plans. Do not equate the existing
   legacy construction action with verified new placement.
4. Finish shortage repair, army/combat/naval-invasion automation, continuous
   waits/digests and the remaining FAIR policy bindings. Complete the clean
   two-year Luna gate, then full filmed Germany FAIR, China FAIR and Poland META
   showcases. These remain part of the goal; small tests do not replace them.

Use one Luna-medium player for live input and at most a small number of Sol 6.1
or Luna helpers when useful. Keep reads fast and stationary, preserve failures,
and avoid screenshots or cursor movement unless the actual action needs them.
No cheats, console, memory writes, hidden intelligence or reload-to-undo.

Suggested continuation prompt:

> Resume the paused HOI4 Live Tools goal. Read docs/RESUME_HANDOFF.md and the
> private Q12 handoff first. Preserve the uncommitted tested selection adapter
> and all unrelated work. Revalidate current state, finish its publication, then
> continue typed construction placement and mixed standing plans. Use one Luna
> medium player, fast live reads and native macOS 1080p or higher for recordings.
