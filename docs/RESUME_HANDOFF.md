# Resume handoff — 5 November 2026

The current, complete starting point. Older handoffs (`LINUX_RUST_HANDOFF.md`,
`MAC_CONTINUATION_STATUS.md`, `HANDOFF_MAC.md`) are history.

## Goal

Recorded full HOI4 campaigns at 1920×1080 where an AI player reaches the goals:
FAIR Germany, FAIR Nationalist China, META Poland (rules in
`skills/hoi4-campaign-playbook/SKILL.md`). Native Rust MCP on macOS, Linux and
Windows. Then update the public repo and write the Windows handoff
(`WINDOWS_HANDOFF.md`).

Order agreed with the user:

1. Every tool built and verified live (done for most tools on macOS, see below).
2. Small war campaigns until the player plays real strategy and tactics
   (encirclements, concentrated offensives, reserves). First: **Italy vs Ethiopia**.
   It must succeed fully before moving on, then vary the training (defence,
   multi-front, rivers, counter-attacks, naval).
3. The full campaigns: Germany → China → Poland.

The player is one **Sonnet 5.5** agent (user decision, 2 November; Haiku proved too
weak). In the end Haiku should also manage full campaigns.

## Where things are

| | State |
|---|---|
| `master` | v1.2.0: macOS native runtime (~120 tools, HOI4 1.19.4.1), Windows Python runtime, Linux native preview (1.19.3 only), Steam guards, release workflow |
| Release v1.2.0 | Python zip, `hoi4-live-native` Linux x86_64 and macOS arm64, `SHA256SUMS.txt`, build attestations; kit pin in Agentic-HOI4-Modding moved to 1.2.0 |
| Linux port (PR #10) | Tools compile and run on Linux through `GameInput`/`X11Input` and `xlayout`; refused until a Linux 1.19.4.1 address table ships ([LINUX_PORT.md](LINUX_PORT.md)) |
| Tool verification | [LIVE_VERIFICATION_1_19_4.md](LIVE_VERIFICATION_1_19_4.md); index [TOOLS_MACOS.md](TOOLS_MACOS.md) |
| War doctrine | `skills/hoi4-campaign-playbook/references/war-operating-loop.md` (procedure plus principles learned in live wars) |

### Blocked: Linux 1.19.4.1

Needs the Linux executable and the `.so` files beside it (ZIN's WSL copy:
`/home/klim/games/hoi4/hoi4`, sha256 `36a69ea4…5da`, GNU build ID `2265e6e7…dc082`)
copied to `~/Projects/_scratch/hoi4-linux-1.19.4.1/`. Then:
`scripts/gen_linux_layout.py generate`, set `xlayout::LINUX_TABLE`, migrate the core
Linux layouts 1.19.3 → 1.19.4.1 (same-platform, like macOS did), live checks in the
Linux VM (game data can come from the macOS install).

### Open tool defects (macOS)

MIO list rows (CSmoothListbox items unmapped), naval invasion landing province not
read back (`landing_verified:false`), operatives, province buildings, automatic
diplomatic responses, `army_split` mixing states, `army_move` slow from home,
supply hubs (placement click hit a unit counter; low value in short wars).

## Ethiopia training so far

Sonnet as Italy vs Ethiopia (runs w2–w4): no capitulation. Causes found and fixed
or written into the playbook: fronts broken (fixed), supply 10–27% in the north,
home divisions and air never brought in, slow pace. Research for run w5: two big
armies (~24 + 22 divisions), aggressive dual offensive lines, all air over the front
(fighters and CAS; rebase wings to Africa), advance hub to hub (captured hubs supply
after about a week), Aussa flank after Tigray, Mussolini missions at ~150 and ~180
days. Supply hubs are not worth building in this war (user).

## Running a live game (macOS)

- Launch `hoi4.app/Contents/MacOS/hoi4 -userdir=…` directly, copy `dlc_signature` and
  `dlc_load.json` from `~/Documents/Paradox Interactive/Hearts of Iron IV` (10 DLCs),
  force 1920×1080 in `pdx_settings.txt`. Without Steam a fresh user folder has no DLCs.
- Memory access needs, once: Developer mode on (`sudo DevToolsSecurity -enable`, run
  by the user), the user in `_developer`, and the binary ad-hoc signed with
  `native/macos/debugger.entitlements`. Nothing has to be renewed: a signed binary is
  judged on `system.privilege.taskport.debug`, which has no password and no timeout.
  Only an unsigned binary still depends on the 10-hour `system.privilege.taskport`
  right (`security authorize -u system.privilege.taskport`).
- The game window must stay front-most during runs; other apps taking focus stop
  input (`input_target_not_frontmost`).
- Private harness (outside Git): `~/Projects/_scratch/hoi4-macos-native`
  (`newgate.py`, `go3.sh`, `redeploy.sh`, `macplay.py`, `plan_loop.py`, briefs).

## Rules that do not change

FAIR by default, META only for Poland; no console, save editing, memory writes,
hidden enemy intelligence or reload-to-undo. Normal difficulty, historical AI,
vanilla. Never touch the shared Windows game. Verify process ownership by
creation identity, executable and user. Keep recordings, journals, saves and
personal paths out of Git; audit full history before pushing. Commits carry no AI
attribution.
