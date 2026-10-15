---
name: hoi4-campaign-playbook
description: Plan and operate Hearts of Iron IV campaigns with a reusable FAIR default or explicitly selected META rules profile, country strategy, daily standing plans, event-driven control, and verified victory evidence. Use for autonomous HOI4 campaign play and Germany, Nationalist China, or Poland showcases; excludes mod QA and cheat-based setup.
---

# HOI4 campaign playbook

Win through the installed game's normal player interface. FAIR is the default:
strong vanilla strategy under the explicit house profile below. META must be
selected by the user or the recorded campaign assignment. It permits powerful
normal-UI metas and game-mechanic exploits; it never permits privileged access.
Do not switch a losing FAIR run to META or relabel earlier actions.

For this repository read applicable `AGENTS.md` and the current
`docs/LINUX_RUST_HANDOFF.md` first. `docs/HANDOFF_MAC.md` preserves the original
campaign specifications; its old Mac test order is superseded.
This playbook supplies strategy, not evidence that the MCP implements it or that
live acceptance passed. Discover tool schemas and capabilities at run time.
The older `hoi4-live-campaign` skill's test-mode setup, save-per-briefing,
two-day pause loop and skip-on-failure examples are not showcase procedure.

## Load only what is needed

1. Read [rules and modes](references/rules-and-modes.md) at campaign setup.
2. Read [operating loop](references/operating-loop.md) to declare the standing
   plan and control the daily governor. Keep it available during play.
3. Read [strategy](references/strategy.md) for economic and military decisions.
4. Load one country card: [Germany FAIR](references/germany-fair.md),
   [Nationalist China FAIR](references/china-fair.md), or
   [Poland META](references/poland-meta.md).
5. Consult [sources and version checks](references/sources.md) when verifying
   patch-dependent mechanics, a rule's origin or a proposed META exploit.

## Non-negotiable campaign boundary

- Normal difficulty, vanilla rules, historical AI; observational telemetry is
  the only enabled mod. Player alternate-history choices are possible in META;
  historical AI does not guarantee every AI outcome.
- No console/debug/eval/fire_event, save editing, memory writes, command/effect
  injection, hidden enemy intelligence, or reload-to-undo, in either mode.
  Ordinary in-game intelligence and information actually visible to the player
  are permitted. Read-only observation does not authorize hidden information.
- Never enter test mode to prepare a campaign. A checkpoint may resume a real
  interruption; never pick an older checkpoint to reverse an outcome.
- One `gpt-6-luna` player at medium reasoning owns acceptance-run input.
  Builder and offline helpers do not control the same game. Verify Linux first;
  Windows and macOS need independent live acceptance. The shared Windows game
  remains reserved for separate mod testing.
- Verify session ownership using PID, creation identity and executable. A
  desktop lease alone is not process ownership. Do not take over unrelated games.
- Run-time policy can enforce only constraints supported by validated inputs.
  A FAIR flag does not prove absence of cheese; record decisions and evidence.

## Start once, then play continuously

Record build/checksum, DLC, mods, country ID, mode/profile revision, objectives,
model/reasoning, code revision, UTC start and artifact location before unpausing.
Resolve current focus/tech/unit/state/region IDs from the installed build.
Keep the public repository free of private paths and campaign artifacts.

The ordered showcase assignment is Germany FAIR, Nationalist China FAIR, then
Poland META, after the separate clean two-year Luna acceptance. Country cards
define milestones and evidence; a plan is not a result. Recording starts before
the campaign and ends after the victory proof. Five hours per campaign is a
target budget, not a substitute for a conclusive result.

The daily governor executes declared routine while the player thinks. Use a
fresh compact digest and server-side event waits, with roughly one strategic
turn per peaceful month. Observations do not pause. Pause only for a genuine
blocking choice or a bounded action that requires it, restoring the prior time
state afterward. When capability gaps prevent this contract, report the exact
gap instead of calling manual polling or offline tests live acceptance.
