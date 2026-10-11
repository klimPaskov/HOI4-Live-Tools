# Mac continuation checkpoint — 11 October 2026

This development checkpoint continues `wip/governor-memory` on
`feat/macos-governor`. It is **not a release or campaign acceptance**. The original
WIP is preserved; the published version and downstream Windows release pins have
not moved.

Read this status before the older implementation-status paragraphs in
[HANDOFF_MAC.md](HANDOFF_MAC.md). Those paragraphs describe the starting point.
The remaining campaign requirements still apply.

## Implemented and checked offline

- Portable platform factories and lazy Windows imports. Native Windows window,
  capture, input, clipboard and OCR code remains behind `hoi4_live/platform/`.
  macOS supplies owned-window discovery, Retina coordinate transforms, capture,
  keyboard/mouse/clipboard and Vision OCR. Native permissions and locked desktops
  produce explicit capability errors.
- Exact macOS process identity from PID, creation timestamp and executable.
  Desktop leases use distinct session tokens and an OS file gate; live owners do
  not lose a lease merely because its heartbeat is old. Recycled PIDs and
  competing stale-lease recovery are tested.
- Python 3.9-compatible stdio startup, initialization, tool discovery and explicit
  campaign refusal. A Windows/macOS CI matrix is provided; local Mac tests do not
  establish Windows native regression success.
- Campaign mode and FAIR are the defaults. Campaign-to-QA escape, debug arguments,
  raw input escape routes and unfiltered enemy information are rejected.
  Direct background handlers also enforce normalized arguments, policy,
  own-country visibility and ownership. Ordinary prelaunch status remains usable.
- One versioned [policy file](../hoi4_live/game/play_policy.json) defines FAIR and
  META. Deterministic FAIR land-order review can bind a unique army name to its
  stable owned ID, require public state IDs and complete templates, reject
  leg-infantry/cavalry plus line-armor compositions, and bound target churn.
  This reviewer needs a trusted fresh live observation adapter, which is not yet
  wired into campaign execution. It does not claim to detect strategic intent or
  every engine exploit. Sensitive unsupported operations still fail closed.
- Routine fast-clock returns and declared popup choices leave the game running.
  Unresolved decisions and explicit stop can pause with a reason. Blind pause
  toggles on unknown clock state are refused. Standing-rule pause defaults to
  false; campaign front observations cannot create a save. The old chunked
  screen loop is unavailable for campaigns because it repeatedly pauses.
- Campaign telemetry generation excludes foreign force/economy expressions,
  inherited watches and private mod data. Configuration changes use durable
  backups. Bounded completed-day equipment/resource histories carry schema and
  generation identity and are invalidated on timeline changes.
- Governor digests include fresh, scoped equipment/resource shortages and explicit
  unknown sections, with daily observation coverage. Shortage alerts are
  observations and inspection suggestions, **not automatic repair**. Live plan
  execution still supports research and declared events only; focus,
  construction, production and army execution remain unfinished.
- Mac read-only task-access and Mach-O analysis infrastructure is bounded and
  permission aware. No Mac build layout is registered as live validated. No
  native command posting or memory-writing campaign adapter exists.
- Mac recording has owned-window capture, process identity checks, first-frame
  verification, bounded frame piping, segment/disk controls and graceful EOF
  shutdown. Synthetic ffmpeg recording was checked. Live game recording
  throughput has not been accepted.

## Playbook and campaign assignment

Use [hoi4-campaign-playbook](../skills/hoi4-campaign-playbook/SKILL.md). Its seven
references cover the explicit house rules, continuous operating loop, economy,
research, logistics, army planning, encirclements, air/naval support, invasion
readiness and country objectives. A dated source ledger distinguishes historical
exploit claims from mechanics verified on the installed build.

The requested sequence is:

1. Separate clean Germany acceptance, 1936-01-01 through 1938-01-01, one
   `gpt-6-luna` player at medium reasoning, FAIR, exclusive input ownership.
2. Full Germany FAIR showcase, including the original expansion, Sea Lion,
   Greater German Reich, Soviet and Allied defeat requirements.
3. Full Nationalist China FAIR showcase, including unification and actual
   Japanese capitulation, with measurable power and territorial predicates.
4. Full Poland META showcase. Interpret the follow-up's “third mode on Poland”
   as the third showcase using the second play mode, not a third policy mode.
   Record victory predicates and the consequential META tactic before play.

Both modes prohibit console/debug/eval/fire_event, save editing, memory writes,
hidden intelligence and reload-to-undo. META permits normal-UI metas and exploits;
it does not waive those prohibitions. The older `hoi4-live-campaign` skill's
QA setup and repeated pause/save examples are not the procedure for these runs.

## Actual local evidence and external blocker

The installed Mac binary is x86_64 **Case Green 1.18.0.0.858d (3bae)** on Apple
Silicon. It is not the 1.19.2 Windows/reference build. Independent on-disk Mach-O
research found candidate roots and a date representation, but none was validated
against live own-country telemetry. Never copy the historical offsets.

A builder smoke test launched one normal, owned Mac process with a backed-up,
telemetry-only playset. No campaign began. The first window action failed;
subsequent native diagnostics established a **locked macOS interactive session**.
Display wake restored an active display but did not unlock the session. The
agent did not attempt to unlock it or bypass that boundary. Window discovery and
prelaunch checks were improved from this incident.

Read-only task access to the exact owned process was also denied (`task_for_pid`,
Mach status 5); zero game-memory bytes were read. The permitted fallback is
telemetry/UI on an unlocked interactive desktop, without root, SIP changes or
re-signing the game.

The builder verified the owned process identity, terminated only that smoke-test
process and confirmed its exit. The original `dlc_load.json`, `pdx_settings.txt`
and `settings.txt` were restored byte-for-byte and verified by SHA-256. A small
settings rewrite by the game was preserved separately before restoring the
baseline. No Windows game was launched or controlled.

The MCP registration was added to the local Codex configuration with campaign,
FAIR and owned-only settings, and external artifact/call-log paths. Existing
configuration bytes were preserved with a private backup. The installed
Homebrew CLI shim was broken; the working app-bundled CLI verified registration.
The active chat may need a fresh server connection to expose newly registered
tools. Do not publish the private config backup or raw smoke artifacts.

Independent checkpoint validation passed **657 tests, 26 skipped**, plus
compile/import and stdio smoke checks, including the final preflight and lease-inheritance assertions.
Consult the latest test result recorded with the checkpoint for the final count.
Skips include platform-specific and unavailable private-fixture cases. Hosted Windows
runners do not have proprietary HOI4 font files: the complete glyph-assisted
fixture assertions require those files, while separate OCR-only tests verify
recognized values and explicit unknowns. Injected Mac identity tests use POSIX
path semantics even when the test host is Windows. Native
Vision recognized generated text; some Windows golden screenshots are not yet
accurately recognized by Vision. Offline and synthetic checks are not live
acceptance, recording evidence or proof of gameplay strength.

## Resume from here

1. The Mac desktop must be unlocked by the user before live work. Recheck native
   session/display/permission capabilities before launching or changing settings.
2. Start a fresh owned normal smoke run with the observational playset and
   recording checks. Validate client bounds, Retina scale, input modifiers,
   clipboard, OCR and real own-country telemetry on this installed build.
3. Finish fresh ID-based focus/construction/production/army observations and
   verified daily execution; connect the FAIR reviewer to trusted observations.
   Prove shortages are repaired rather than merely reported. Preserve pending
   plan entries through failed or ambiguous actions.
4. Implement and validate war/invasion verbs, including assignment, fronts,
   offensives, supplied staging, transport capacity, supremacy, escort/air
   support, launch, landing and reinforcement. Pixel change alone is insufficient
   action proof. Collect end-to-end pause/latency/coverage measurements; current
   fast-clock pause metrics cover only their run scope.
5. Only then give one Luna player exclusive input for the clean acceptance run.
   Keep recording, calls, manifest, journal and incidents outside Git. A rescued
   or failed run cannot be relabeled clean. Complete all three showcases after
   acceptance, then validate Windows regressions before release/pin changes.

An unlocked desktop is a concrete external prerequisite. The unfinished governor,
FAIR observation adapter and war automation are internal implementation gaps,
not claims of external blockage or completed work.
