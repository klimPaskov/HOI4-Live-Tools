# Reference review: smerss/Clausewitz-MCP

Reviewed 2026-10-11 from the repository page, its `research/` and `mcp/` listings and `research/FINDINGS.md` (MIT licence, Python, Linux only).
It lets an agent play Hearts of Iron IV by reading process memory and posting commands to the game's own command queue.
Nothing was copied; the table records which of its safety lessons this server already follows and which are open.

| Lesson from the project | Status here |
| --- | --- |
| Read back the result of every command before trusting it | Done. Every acting tool re-reads the screen or state and returns `verified_by` and what changed. |
| A toggle is sent only when the state differs from the wanted one | Done for `hoi4_time` (pause and run are checked against the clock) and for `hoi4_orders` execute and hold (the animation is read before and after, and `changed` says whether anything moved). |
| Waits always return by a limit | Done. `hoi4_advance` takes `days` and stops on the first decision point. |
| Refuse to act when the played country cannot be resolved | Done. The campaign tools take the tag from fresh telemetry and ignore stale telemetry. |
| Irreversible actions need an explicit confirmation | Done. `hoi4_diplomacy declare_war` needs `confirm_war`, `hoi4_shutdown` needs `confirm`, and test-mode tools are refused in campaign mode. |
| A `doctor` command that checks the installation | Done as `python -m hoi4_live selftest`. |
| The game's own error messages are hard to read | Added in this review: `hoi4_logs` attaches the documented fix (`known_mistake`) to the four event messages the Paradox wiki explains (a timer on a triggered-only event, an event that fires every day, an event without `add_namespace`, duplicate internal event ids). The list mirrors the HOI4 Agent Tools known-mistake rules and is closed; an entry needs a wiki or game-documentation source. |
| Do not re-issue a movement order every tick | Open (F1). `hoi4_orders front`, `offensive` and `spearhead` redraw a plan even when the army is already executing one. The project reports that re-targeting a marching division cancels its path. A fix needs a live check that a redraw really resets the plan: read `executing` from `hoi4_orders status`, and when it is true return `unchanged` unless `replace=true`. |
| Detect multiplayer and Ironman and narrow the actions | Open (F2). `hoi4_status` should report `ironman` and `multiplayer` (an Ironman save is binary, which `hoi4_save_query` already refuses) and the campaign tools should refuse to act in them. Needs a real Ironman launch to confirm the signals. |
| Process-memory reads, command-queue posting, attack and encirclement planning | Subsequent priority: own-country memory reads and daily automation are explored on `wip/governor-memory`. Linux transport is not portable as-is; command posting remains unproven. See [HANDOFF_MAC.md](HANDOFF_MAC.md). |

F1 and F2 were not implemented in this change because both change how acting tools behave and need the game; campaigns were running on the machine during this review.
