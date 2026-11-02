# Native Mac input geometry

The Mac adapter reads the current game's own GUI, graphics and SDL Cocoa window
state before ordinary keyboard or mouse input. It accepts one fixed profile:
1920×1080 render and logical dimensions, GUI scale 1, and a shown, focused,
windowed game. Unsupported geometry produces a capability error before input.
No screenshot, pointer movement, pause, game call or memory write establishes
this binding.

This layout is independently recovered for Mach-O UUID
`089EA519-6780-353B-8E39-4E9309B444D4`, executable SHA-256
`09419b087f8b4656c00df337bf838ff34c9b1d816b76cef5383ae6a4ac15b1fb`.
The process adapter verifies that exact executable and the owned game/supervisor
identities. Other builds require separate recovery; these are not Linux offsets.

The bounded reader checks the current in-game idler, CGameGui and CGameGraphics
primary types, the GUI's graphics owner, physical/logical dimensions and scale.
Three constructor-proved callbacks identify the Cocoa video backend. Its window
list is bounded to eight entries, rejects cycles and requires exactly one window
matching the render dimensions. The selected window must have reciprocal links
to its video backend, Cocoa driver data, SDLView and Cocoa listener.

The SDLView and listener subclass ivar offsets are read from their runtime ObjC
offset globals. Their on-disk values are not safe substitutes: the observed
runtime values were 536 and 32 bytes, while both on-disk values were 8.
Every dependency is re-read at the end of the snapshot; a change returns a torn
read error instead of geometry.

SDL's Cocoa creation, move and resize paths obtain the native content rectangle
using `contentRectForFrameRect:` and cache its x/y/width/height. The OS adapter
corroborates that rectangle against exactly one normal on-screen CG window
belonging to the owned process. The exact game client must be frontmost; a same-owner window wholly inside its
title decoration may precede it (macOS adds a sharing indicator during recording).
A foreign window, overlapping child or modal elsewhere still refuses, and the
SDL window must independently retain input focus. The client
origin is taken from the live SDL rectangle, not calculated from a guessed title
bar. A bounded decoration difference is only a consistency check.

Binding pins the native objects, window ID, OS frame and content rectangle.
Each input rechecks the binding. Mouse presses recheck after cursor settlement;
keyboard presses recheck after any modifier recovery. Key/button releases still
complete after a successful press. A changed window, scale, position, backend or
owner requires a new explicit binding.

The read-only `hoi4_input_geometry` result remains a diagnostic. It reports the
memory-derived rectangle and dimensions but does not claim OS association or
control hit routing. OS corroboration belongs to the input adapter, and individual
actions still need their fresh own-state and GUI readback checks.

On the native Mac qualification game, bounded reads found render/logical
1920×1080, GUI scale 1, cached content rectangle `(0,81,1920,1080)` and owned CG
frame `(0,53,1920,1108)`. These observations corroborate one Retina display
arrangement; they do not qualify other scales, multiple displays, builds or a
campaign. Private disassembly, heap addresses, captures and session identities
remain outside Git. See [live validation](CLAUSEWITZ_VALIDATION.md) for recorded
action outcomes and their limits.
