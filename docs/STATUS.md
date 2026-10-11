# Public status (11 October 2026)

Python 3.9+ stdio MCP. The Windows 1.1.0 baseline provides session control,
capture/OCR, popup handling, telemetry generation, state and briefings, economy,
diplomacy, army orders, navy/air missions, standing orders, a background clock,
campaign journal and tool-call logging. See [tools.md](tools.md).

Paths and process discovery have a platform interface with Windows and macOS
implementations. macOS desktop, OCR, launch, memory and recorder integration are
unfinished; the complete server cannot yet run on a Mac. This is a development
handoff, not a claim that macOS live play works.

The `wip/governor-memory` branch preserves the stopped implementation:
owned-process read-only Windows memory snapshots, durable governor plans,
research/popup routines, bounded condition waits, campaign visibility filters,
configuration backup/restore and segmented recording. Its tests passed on
Windows (457 tests, 15 skipped for absent fixtures). It has not passed a live
two-year small-model campaign. Unsupported plan sections fail explicitly.

Campaign attempts did not satisfy the showcase goals. Run 2 ended 24 March 1939
after the Austrian war, armies concentrated in Vienna, 14 home states lost and
74% surrender progress. A real crash produced a dump. Run 3 reached roughly
1 September 1936 before the user closed the game; it was not a verified crash.
The major problems were frequent clock stops, slow UI actions, unreliable unit
selection/order verification and shortages discovered too late.

Read [HANDOFF_MAC.md](HANDOFF_MAC.md) first. The historical reports are
[campaign run 3](handoff_2026-10-11_campaign_run3.md) and
[redesign spike](handoff_2026-10-11_redesign.md). Their original statements about
what was unfinished describe their own stop times; the WIP branch came later.
Raw saves, recordings, dumps, private mod content and original history remain
local and are not published. The fixture text files are minimal parser samples,
not loadable campaign checkpoints. Screenshot fixtures are cropped vanilla UI.
