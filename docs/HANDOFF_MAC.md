# Mac handoff: start here

11 October 2026. Public source: <https://github.com/klimPaskov/HOI4-Live-Tools>.
This is the single entry point for continuing with Codex on a Mac. Read the root
[AGENTS.md](../AGENTS.md), then continue on `wip/governor-memory` or a feature
branch based on it. `master` is the sanitized Windows baseline plus this handoff.
Python 3.9+ and stdio MCP remain the contract. Windows and macOS are both required
targets; macOS live control is **not implemented end to end yet**.

## User requirements and operating rules

The builder is **gpt-6-astra**. Delegate the actual small-model player acceptance
to **gpt-6-luna**, initially at medium reasoning. The MCP must let a small model
play in real time without problems. Windows HOI4 is reserved for separate mod
testing; all new development and campaign runs for this continuation happen on
the Mac. Work autonomously, never ask questions, and preserve unrelated work.

- A background governor executes the model's durable standing plan every game
  day at high speed, including while the model thinks. It should refill research,
  focus, construction and production, manage shortages, and coordinate armies.
- Return a compact digest (target 1–2 KB) with freshness/source information,
  severity, stable IDs and concrete one-call fixes. Unknown is not zero.
- Wait in the server for relevant changes, deadlines or real decisions. Do not
  repeatedly poll with model turns or pause every two days. Handle routine
  events by declared policy; deduplicate alerts. Target about one strategic
  model turn per peaceful month, plus genuine decisions.
- Use IDs for states, provinces, countries, technologies, equipment, armies,
  divisions, fleets, air wings, bases and sea regions. Verify actions by state
  readback; do not burn a plan entry on a transient UI failure.
- War automation must assign/split/reinforce armies, form fronts and offensives,
  handle supply/equipment/resources, and support naval invasions and Sea Lion:
  transport capacity, ports, planning, naval supremacy, escort/air cover, launch,
  landfall, reinforcement and victory checks.
- Showcase rules: Normal difficulty, vanilla rules, historical AI. The only
  enabled mod is observational telemetry. No console, debug launch, eval,
  fire_event, save editing, hidden enemy intelligence or reload-to-undo. Saves
  may record progress or resume an interrupted run; never reverse an outcome.
  Any native command adapter must issue only normal, validated player actions;
  forced commands and arbitrary injected effects are excluded.
- Pause only for a necessary blocking decision or bounded action that needs it;
  restore the prior running state. Routine observations never pause. One agent
  owns game input; helpers must not compete for the same desktop.

### Live speed (user requirement)

The constant pausing seen in the Windows runs is not acceptable. Play must be
live and very fast: the game keeps running at high speed while the model plans
and while tools act; reads come from telemetry/memory, never by pausing; actions
are posted while the game runs; the model is woken only by events, deadlines or
real decisions. Measure and report pause count, pause seconds and real time per
game year, and treat every avoidable pause as a defect.

### Two play modes (user requirement)

The MCP has two explicit modes, chosen per session and recorded in every log:

1. **Fair mode (default): common HOI4 multiplayer game rules.** No exploits or
   cheese that multiplayer house rules ban, for example no space marines (no
   marine/para spam used to exploit combat width or terrain), no naval-invasion
   or paratrooper exploit spam, no 1-width or other exploit templates, no
   border-gore/peace-deal exploits, no AI-baiting tricks, no exploiting known
   engine bugs. It must still play strongly and cleverly: encirclements and
   pockets, concentrated breakthroughs, air superiority before offensives,
   defensive lines and fallbacks, supply-aware planning, combined arms, sound
   production and research priorities. Keep the banned-tactics list in one
   reviewed file (e.g. `hoi4_live/policy/fair_mode.json`) and enforce it in the
   tools (refuse or warn on banned orders/templates) so a small model cannot
   drift into exploits.
2. **Meta mode: metas and exploits allowed, still no cheating.** Competitive
   metas and known exploits that a human player can perform through the normal
   UI are allowed (space marines, exploit templates, invasion spam, AI
   manipulation), but cheating stays forbidden in both modes: no console, debug,
   eval/fire_event, save editing, hidden enemy intelligence, memory writes or
   reload-to-undo.

The showcase campaigns run in fair mode unless the user asks otherwise.

## Recorded showcase plan

After acceptance, record complete campaigns from 1 January 1936 to a conclusive
end, aiming for about five hours of wall time each. Treat that duration as a
budget target, not evidence of success. Record failures honestly.

1. **Germany:** Anschluss, Czechoslovakia, Poland, the Low Countries and France;
   successful Sea Lion with the UK capitulated; form the Greater German Reich;
   capitulate the USSR and defeat all Allies. Check the installed vanilla/DLC
   requirements for the formable before choosing the focus/territory plan.
   Record final wars, capitulations and the peace settlement as proof.
2. **Nationalist China:** unify China including warlords and communists; expel
   and capitulate Japan; recover lost territory; finish as a top-tier power.
   Define measurable end-state metrics in the journal before starting (unified
   territory, remaining wars, industry, army and major-power status).

Keep ffmpeg footage, `calls.jsonl`, the campaign journal, a run manifest (game
build, DLC, difficulty, historical AI, enabled mods, model/reasoning, commit,
start/end timestamps) and an incident log in a scratch directory outside Git.
Align UTC/wall timestamps and game dates; journal milestones for the demo video
and a short teaser. Start recording before the campaign and end after victory
proof. Use segments, disk-cap/headroom checks and graceful shutdown; verify with
ffprobe. Do not publish raw logs or footage without a privacy review.

## What exists and what failed

The 1.1.0 Windows baseline exposes 43 tools covering sessions, screen/OCR,
time/popups, generated telemetry, state/briefing/map, politics/economy/diplomacy,
army/orders/navy/air, front summaries, standing orders and a journal. The stdio
protocol uses the standard library; image processing uses NumPy, Pillow and
OpenCV. The Windows installer supports client registration and release archives.
The existing `hoi4_live/platform/` package implements paths and process basics
for Windows/macOS/POSIX. Desktop and OCR imports still prevent normal Mac server
startup. A generic POSIX launch path exists but has not been verified with Steam
or the Mac app bundle.

Historical reports (read them in order):

- [Campaign run 3](handoff_2026-10-11_campaign_run3.md): run 2 reached 24 March
  1939 after an Austrian war, armies piled into Vienna, 14 German states lost,
  74% surrender progress. It had a real crash with a dump. Run 3 reached about
  1 September 1936 before the user closed the game; that disappearance is not
  a proven crash. Neither attempt reached the required goals. UI actions often
  took 20–40 seconds; frequent stops, failed unit subset selection, false pixel
  verification and late shortage detection made small-model play impractical.
- [Redesign spike](handoff_2026-10-11_redesign.md): Windows PE/MSVC RTTI found
  14,040 classes and 370 command classes; read-only process memory worked.
  Germany's factories read as 35 civilian / 28 military / 10 dockyards, plus
  political power and division strength/organisation. Global roots were found.
  `CCommandQueue::Post` was identified at RVA `0x223bbc0` (591 callers), queue
  accessor at idler vtable `+0x88`. **No native command was executed.** These are
  build-specific observations, not portable offsets or proven vanilla runs.

The later stopped Codex WIP is preserved separately on `wip/governor-memory`:

- `game/governor.py`, `state/governor.py`, governor tools and `hoi4_wait_for`:
  durable plans, cached digests, daily scheduling and bounded waits. Start
  requires campaign mode, session-owned game, sticky lease and own telemetry.
  Only research plus explicit event policy are accepted; nonempty focus,
  construction, production and army plans are rejected as unfinished.
- `memory/{reader,winproc,layout}.py`, `hoi4_memory`: bounded read-only snapshots
  tied to owned PID/creation identity/executable and verified build; pointer/RTTI
  checks, structural witnesses and explicit unavailable fields. Date remains
  unavailable in the tool contract; no command posting surface exists.
- `game/session_backup.py`: configuration backup/restore; visibility filtering
  and changes to clock/guard/process/session/front/dispatch/telemetry tools;
  segmented owned-window recording and tests.
- [Static Windows layout evidence on WIP](https://github.com/klimPaskov/HOI4-Live-Tools/blob/wip/governor-memory/docs/evidence/memory/static_layout.md)
  corrects an early interpretation: `game_state+0x520` is a tag index resolved
  through the tag mapping, not directly a country-array index. Static evidence
  also identifies date/speed/pause/research offsets, but live date decoding and
  Mac equivalents still require validation. Do not rely on the older report's
  more optimistic layout assumptions.

Before preservation, Python 3.9 `python -m unittest discover -s tests -v`
passed **457 tests, 15 skipped** for unavailable private/omitted fixtures.
This proves the offline Windows tests, not live acceptance or Mac compatibility.
The baseline's older reports mention 324 tests at their earlier stop times.
The sanitized master parser-fixture changes also passed 328 tests with 15 skips.
No game was launched or controlled during the publication handoff.

## Mac setup from zero

Install Steam and your owned copy of HOI4; launch once to create its user folder,
then close it. Check the actual executable architecture with `file`; an Intel
Mac build on Apple Silicon may require Rosetta. Keep Python/helpers native where
possible; target process ABI/architecture determines memory decoding.

Install Homebrew using its [official setup](https://docs.brew.sh/Installation):

```sh
xcode-select --install
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

Follow the installer's PATH instructions for your architecture. Then:

```sh
brew install git python ffmpeg gh
brew install --cask codex
mkdir -p ~/Projects ~/Projects/_scratch
cd ~/Projects
git clone https://github.com/klimPaskov/HOI4-Live-Tools.git
cd HOI4-Live-Tools
git switch --track origin/wip/governor-memory
python3 -m venv .venv
source .venv/bin/activate
python -m pip install --upgrade pip
python -m pip install -e .
python --version
codex --version
codex --model gpt-6-astra
```

The HTTPS clone is public and needs no credentials. Push access later requires
`gh auth login` and `gh auth setup-git`. Python 3.9 is the compatibility floor;
a current Homebrew Python is suitable for the Mac environment. `install.py` is
Windows-only, so use the venv installation above. The first Codex launch offers
Sign in with ChatGPT; use the account with the requested models. Keep exact
model names; report unavailable account access instead of silently substituting.
See [Codex CLI](https://learn.chatgpt.com/docs/codex/cli) and the
[Homebrew Codex cask](https://formulae.brew.sh/cask/codex).

To make spawned players use Luna, merge these settings into
`~/.codex/config.toml` without replacing existing configuration:

```toml
model = "gpt-6-astra"

[agents]
enabled = true
default_subagent_model = "gpt-6-luna"
default_subagent_reasoning_effort = "medium"
max_concurrent_threads_per_session = 2
```

Explicitly request a Luna player when starting acceptance. Defaults are for
spawned agents, not a switch of the primary builder. See
[Codex subagents](https://learn.chatgpt.com/docs/agent-configuration/subagents).

Register the MCP by adding the following table to `~/.codex/config.toml`.
Replace `<you>` with your account name; TOML does not expand `~` inside `command`.
This is the intended Mac command; startup will remain blocked until the desktop
imports/backends below are ported. Do not interpret that as a working Mac release.

```toml
[mcp_servers.hoi4-live]
command = "/Users/<you>/Projects/HOI4-Live-Tools/.venv/bin/python"
args = ["-m", "hoi4_live"]
cwd = "/Users/<you>/Projects/HOI4-Live-Tools"
startup_timeout_sec = 30
tool_timeout_sec = 1800

[mcp_servers.hoi4-live.env]
HOI4_LIVE_MODE = "campaign"
HOI4_LIVE_OWNED_ONLY = "1"
HOI4_LIVE_GAME_DIR = "/Users/<you>/Library/Application Support/Steam/steamapps/common/Hearts of Iron IV"
HOI4_LIVE_USER_DIR = "/Users/<you>/Documents/Paradox Interactive/Hearts of Iron IV"
HOI4_LIVE_ARTIFACTS = "/Users/<you>/Projects/_scratch/hoi4-live-tools-evidence/mac"
HOI4_LIVE_CALL_LOG = "/Users/<you>/Projects/_scratch/hoi4-live-tools-evidence/mac/calls.jsonl"
```

Create the artifact directory first. Restart Codex, inspect `codex mcp list` and
the available tools, then run selftest/protocol smoke once imports are portable.
The 1800-second tool timeout accommodates existing slow load/time operations;
governor waits themselves remain bounded. See
[official MCP configuration](https://learn.chatgpt.com/docs/extend/mcp?surface=cli).

HOI4 locations:

- Install: `~/Library/Application Support/Steam/steamapps/common/Hearts of Iron IV/`
  (or a custom Steam library); currently probes `hoi4.app/Contents/MacOS/hoi4`
  and `hoi4`. Inspect the real bundle rather than assuming its name.
- User data: `~/Documents/Paradox Interactive/Hearts of Iron IV/`, including
  `mod/`, `logs/`, `save games/`, `settings.txt`, `pdx_settings.txt`,
  `dlc_load.json` and launcher playset state.

Back up settings/playset bytes before modifying them. Generate the telemetry mod
without importing Windows desktop tools while the game is closed:

```sh
python - <<'PY'
from hoi4_live.config import get
from hoi4_live.telemetry.modgen import generate
cfg = get()
assert cfg.game_dir, "Set HOI4_LIVE_GAME_DIR to the installed game folder"
manifest = generate(cfg.user_dir, cfg.game_dir)
print(manifest)
PY
```

The environment above is for the MCP subprocess; export the same game/user paths
in the shell if auto-discovery fails. Enable `hoi4live_telemetry` in a dedicated
vanilla launcher playset, with all other mods disabled, and launch normally.
Once tools work, `hoi4_telemetry action=enable` generates and updates
`dlc_load.json` with a backup; the launcher may overwrite that file, so verify the
actual selected playset. Mod changes load only on restart. Confirm daily
`HOI4LIVE` records in `logs/game.log`, fresh own-country telemetry and no generated
script errors. Never enable arbitrary variable watches during showcase play.

In System Settings → Privacy & Security, grant **Accessibility** to the app
responsible for input (Terminal or Codex/helper) and **Screen Recording** (on
some versions Screen & System Audio Recording) to the recording/capture host.
Restart the affected app after granting access. Test coordinate conversion on
Retina and a second monitor; screen pixels and logical points differ.

Memory access is separate from those permissions. `task_for_pid` generally
requires elevated privilege or a correctly signed debugger with the debugger
entitlement; hardened target restrictions also apply. The target's signing,
`get-task-allow`, SIP and architecture can still block access even as root.
Options: start with telemetry/UI (no task port); build a narrowly scoped local
read-only helper with explicit PID/creation/executable validation; or evaluate a
properly signed debugger helper. Do not run the whole agent as root, disable SIP,
or alter/re-sign the Steam game as a routine setup step. Root expands compromise
impact, signing changes can invalidate updates/integrity, and task access can
crash a process if later used for unverified calls. Permission denial must return
an actionable capability status and fall back to telemetry. See Apple's
[debugger entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.security.cs.debugger).

## Recording on macOS

Install ffmpeg as above. Enumerate AVFoundation devices and choose the **screen**
entry (not the webcam); indices vary by machine. Use a dedicated display with
notifications hidden and no private apps visible. Example shell session:

```sh
mkdir -p ~/Projects/_scratch/hoi4-live-tools-evidence/mac/GER
ffmpeg -f avfoundation -list_devices true -i ""
SCREEN_INDEX=1 # replace with the screen index printed above
ffmpeg -f avfoundation -framerate 30 -capture_cursor 1 -i "${SCREEN_INDEX}:none" \
  -vf "scale=trunc(iw/2)*2:trunc(ih/2)*2" -c:v libx264 -preset veryfast -crf 23 \
  -pix_fmt yuv420p -f segment -segment_time 600 -reset_timestamps 1 \
  ~/Projects/_scratch/hoi4-live-tools-evidence/mac/GER/segment_%04d.mkv
```

Use `q` in the ffmpeg terminal for a clean stop. This records screen video
without audio; add an explicitly chosen game-audio capture source only after
testing it. Crop to the HOI4 client for shared footage. Verify a short test with
ffprobe and a playback check, then record the whole run. These native ffmpeg
commands work independently of the still-Windows `scripts/record_screen.py`.
Port its ownership/supervisor/disk checks rather than discarding them. See
[FFmpeg AVFoundation](https://ffmpeg.org/ffmpeg-devices.html#avfoundation).

## Reference research and attribution

Clone [smerss/Clausewitz-MCP](https://github.com/smerss/Clausewitz-MCP) into scratch:

```sh
git clone https://github.com/smerss/Clausewitz-MCP.git ~/Projects/_scratch/Clausewitz-MCP
```

It is MIT licensed, targets Linux/ELF/ptrace and uses the Itanium C++ ABI. Study
`research/FINDINGS.md`, `calling/{fieldmap,commands,turnloop,ragemicro,combat}.py`
and its offset/constructor tables. The macOS C++ build also uses the Itanium
ABI, so RTTI and serializer-offset discovery should map more directly than
Windows MSVC RTTI. That is an architectural inference, not binary compatibility:
Mach-O sections, dyld/ASLR, libc++ string/container layout, x86_64/arm64 and
version-specific fields/calling conventions still need independent discovery.
The reference's native calls are not verified on Mac. Preserve its MIT notice
for any copied code; record file-level provenance. [NOTICE](../NOTICE) attributes
the design/safety lessons already consulted; no upstream source was copied in
this publication work. Keep reference code out of the release unless explicitly
ported with its license intact.

## Cross-platform architecture and complete dependency inventory

Keep orchestration, schemas, game IDs, parsing, governor rules and visibility
policy OS neutral. Use `hoi4_live/platform/` as the boundary with Windows and
macOS backends; keep existing facades while migrating callers. Return typed
capability/permission errors, never import WinDLL/WinRT on macOS, and avoid
subprocess shell strings. Process ownership must use more than a recyclable PID.

| Interface | Windows backend | macOS backend to finish |
| --- | --- | --- |
| paths | known folders, registry, Steam VDF | Documents, Application Support, Steam VDF and actual app bundle |
| process | Toolhelp, image path, creation time, liveness | libproc/ps, executable identity/start time, liveness |
| window | HWND, client rect, DPI, focus | CGWindowList + Accessibility, PID-owned window, point/pixel transforms |
| capture | PrintWindow/GDI, ddagrab | ScreenCaptureKit/Quartz; AVFoundation fallback; RGBA client crop |
| input + clipboard | SendInput, Windows clipboard | CGEvent/Accessibility and NSPasteboard; verified modifiers and text |
| OCR | persistent Windows.Media.Ocr PowerShell host | Vision OCR with the same text/box contract |
| memory | VM_READ, PE/MSVC RTTI, build layout | Mach task read helper, Mach-O/dyld, Itanium RTTI, build/arch layout |
| launcher | detached exe outside client job | Steam/open/app-bundle path with verified owned process identity |
| recording | owned-window gdigrab supervisor | AVFoundation/ScreenCaptureKit with equivalent supervisor/disk checks |

Windows-native files: `desktop/{win32,window,capture,inputs,clipboard,lock}.py`,
`ocr/{winocr.py,ocr_host.ps1}`, `platform/windows.py`, `memory/{pe,winproc,layout,reader}.py`
(the last three on WIP), `scripts/memory_spike/*.py` (Windows research scripts),
`scripts/record_screen.py`, `hoi4-live-tools.cmd` and the Windows installer paths
in `install.py`. `pe.py` is pure file analysis but only understands PE/MSVC.

Direct callers/import surfaces to migrate: `session.py`, `selftest.py`,
`game/{process,session_backup,console,popups,armyview,deployview,focusview,mapview}.py`,
`tools/{lowlevel,session_tools,common,campaign_common,memory_tools}.py`,
`actions/verify.py`, plus eager registration in `tools/__init__.py` and startup
in `__main__.py`/`protocol/server.py`. `desktop/images.py` is portable and should
stay so. Audit the remaining views' key assumptions, tooltips and calibration;
`config.py` defaults must select OS process names/temp paths, and game-data cache
paths must use the platform backend. Windows-specific launch flags and owned
memory identity must not leak into shared session backup or recording APIs.

Testing plan: use `unittest.skipUnless(sys.platform == 'win32', ...)` and
`skipUnless(sys.platform == 'darwin', ...)` as explicit OS markers; separately
gate tests requiring a live game, permissions or omitted screenshot fixtures.
Shared tests must run on both systems with injected fake backends. Do not skip
the whole suite to conceal portability failures. Add GitHub Actions with a
`windows-latest` / `macos-latest` matrix on Python 3.9 and a current supported
Python (e.g. 3.13), pip install, unittest discovery, compile/import and MCP stdio
smoke. Hosted CI cannot prove GUI permissions, HOI4 memory offsets or campaigns;
those need local recorded acceptance. Linux pure-parser checks are optional.

## Ordered continuation and acceptance gates

1. Inspect branches/status; create `feat/macos-governor` from WIP. Read both
   historical reports and static-layout correction. Re-run Windows-compatible
   offline tests on the Mac; classify genuine OS dependencies, not blanket skips.
2. Remove eager native imports; build the platform factory/session boundary,
   paths/process identity/lock and a capability-aware selftest. Get clean stdio
   initialize + tools/list + unsupported-feature responses on macOS. Keep
   diagnostics on stderr and protocol-only JSON on stdout.
3. Implement window/capture/input/clipboard and Vision OCR. Check client bounds,
   Retina scaling, modifiers, popups and normal launch/load/new-game; preserve
   user files and reject an unrelated already-running game. Restore configuration
   only after the owned process exits. Add the cross-platform CI matrix.
4. Enable fresh own-country telemetry. Port read-only memory with version/arch
   identity and bounded pointer/RTTI checks; validate fields against screen and
   telemetry, including date, focus/research, economy, supply and armies. Unknown
   builds and denied task ports must fall back cleanly, with no hidden intel.
5. Finish the daily governor, compact digest and one-call fixes across all routine
   plan sections. Idempotent retries, alert deduplication, event waits,
   cancellation/restart recovery and no unnecessary pauses are required. Prove
   war verbs and an actual naval invasion. Native command posting stays a bounded
   separate experiment until constructors/signatures and readback are proven.
6. **Acceptance:** one gpt-6-luna player at medium reasoning plays Germany from
   1 January 1936 through 1 January 1938 cleanly in real time under the rules.
   No builder/human intervention, crash, ownership violation, repeated idle
   queues, unhandled routine shortage, cheat or needless stop loop. Collect
   recording + calls + journal, action success/error rates, per-call latency,
   daily-governor coverage, pause time/reasons and strategic turn count. Failure
   produces a reproducible issue; fix tooling and restart a new acceptance run.
7. Run both full showcases above, then make the demo video and teaser. Publish a
   release only after Windows regression tests and Mac local acceptance pass;
   label results accurately and never claim a simulated test as live evidence.

## Sync back to Windows

Commit focused work and push to the public repository; preserve the WIP branch.
Every new commit ends with a blank line and
`Co-Authored-By: Codex (gpt-6.1-sol) <noreply@openai.com>` per the user's request.
Do not push `private/*`, mirror the repository, or publish all local refs.
Keep logs/dumps/saves/recordings, credentials, personal paths and private mod
material outside Git. Re-audit every reachable historical object and tag before
pushing new history or artifacts.

After acceptance, update version/changelog together, run the release builder,
audit the archive, and tag/publish a tested release. Follow
[RELEASING.md](RELEASING.md). The Windows side then pins that exact release URL,
version, SHA-256 and size in the [HOI4 Mod Setup](https://github.com/klimPaskov/HOI4-Mod-Setup)
app and [Agentic HOI4 Modding](https://github.com/klimPaskov/Agentic-HOI4-Modding)
repository; update bundled skills/config names, run clean install/upgrade smoke
and verify the advertised tools. Do not pin unfinished WIP as a stable release.
This handoff publication did not change those downstream pins.

## First prompt to paste into Codex on the Mac

```text
Work autonomously; never ask questions. You are the gpt-6-astra builder for the public HOI4 Live Tools repo https://github.com/klimPaskov/HOI4-Live-Tools. Read AGENTS.md and docs/HANDOFF_MAC.md first (especially "Live speed" and "Two play modes"), then docs/handoff_2026-10-11_campaign_run3.md, docs/handoff_2026-10-11_redesign.md and the WIP static-layout evidence. Inspect git status, branches and history; preserve unrelated work. Create feat/macos-governor from origin/wip/governor-memory. The Windows game is reserved for other mod testing: develop and test on this Mac only.

Study the reference MCP https://github.com/smerss/Clausewitz-MCP closely (MIT; clone it into a scratch folder): how it reads live game state from process memory without pausing (RTTI walk, serializer field maps, offset tables), how it posts ordinary player commands through the game's own command queue (CCommandQueue::Post, same legality checks as clicks, batched per attach, read-only guard), its hoi4_wait_for event-driven waiting, per-day micro engine, combat planner and encirclement search. The macOS build uses the Itanium C++ ABI like that project's Linux target, so its discovery approach maps closely to the Mac; still discover Mac Mach-O layouts independently and never assume Linux or Windows offsets. Reuse ideas or code with MIT attribution in NOTICE.

Keep Python 3.9+ and stdio MCP, and support BOTH Windows and macOS. Finish the platform boundary for paths, process identity, window, capture, input/clipboard, OCR, launcher, read-only memory, command posting and recording. Remove eager Windows imports, add explicit capability/permission errors, OS-specific test markers and a Windows/macOS CI matrix. Set up the venv, MCP registration, observational telemetry and ffmpeg recording using the handoff. Do not disable SIP, re-sign the game or run the agent as root; use telemetry/UI if task access is denied.

Everything must be LIVE and VERY FAST: the constant pausing of the earlier runs is unacceptable. The game keeps running at high speed while the model plans and while tools act; reads come from telemetry/memory without pausing; actions are posted while the game runs; the model is woken only by events, deadlines or real decisions. Measure pause count, pause seconds and real time per game year, and treat every avoidable pause as a defect.

Complete the background governor so it executes the model's durable standing plan daily at high game speed while the model thinks. Provide a compact fresh digest, concrete one-call fixes, event-driven bounded waits, id-based verified actions, shortage repair and war automation: army assignment and splitting, fronts, offensives, encirclements and pockets, fallback lines, supply, air superiority, naval superiority, naval invasions and a successful Sea Lion. Fix failed selection and unreliable pixel verification. Keep retries idempotent, unknown data explicit, hidden enemy state excluded, owned-process boundaries enforced, and settings/playsets backed up and restored. Validate unfinished WIP honestly.

Implement two play modes, selected per session and logged: (1) FAIR mode, the default: common HOI4 multiplayer game rules, so no exploits or banned cheese (no space marines, no naval-invasion or paratrooper exploit spam, no exploit templates, no AI-baiting or engine-bug exploits), enforced by the tools from one reviewed policy file, while still playing strongly and cleverly (encirclements, concentrated breakthroughs, air superiority before offensives, defensive lines, combined arms, sound economy); (2) META mode: competitive metas and exploits a human can do through the normal UI are allowed. In BOTH modes cheating is forbidden: no console/debug/eval/fire_event, no save editing, no memory writes, no hidden enemy intelligence, no reload-to-undo. Showcases run in fair mode.

Delegate live player acceptance to one gpt-6-luna subagent at medium reasoning, with exclusive input ownership; you remain the builder. First require a clean real-time Germany run in fair mode from 1936-01-01 through 1938-01-01 without builder/human rescue. Then record full fair-mode campaigns from 1936 to a conclusive end, about five hours each: Germany must achieve Anschluss, Czechoslovakia, Poland, Low Countries, France, successful Sea Lion capitulating the UK, Greater German Reich, USSR capitulated and all Allies defeated; Nationalist China must unify China including warlords/communists, expel and capitulate Japan, recover lost territory and become a top-tier power. Normal difficulty, vanilla rules, historical AI, observational telemetry only. Never touch an unrelated game.

Keep full ffmpeg recordings, calls.jsonl, journal, manifest and incident log outside Git for the demo video and teaser. Measure latency, pauses, governor coverage and action failures; turn failures into tooling fixes and a fresh acceptance run. Commit focused changes and never add Co-Authored-By or any AI attribution line (the user is the sole author). Audit all pushed history and artifacts for secrets, personal paths, dumps, saves and private mod material; push only public development branches and tested tags. After both OS checks and acceptance pass, publish a release and document the exact version/URL/SHA-256/size for the Windows HOI4 Mod Setup app and Agentic HOI4 Modding pins. Continue until the acceptance and showcases are achieved or a concrete external prerequisite prevents progress; report evidence and remaining gaps honestly.
```
