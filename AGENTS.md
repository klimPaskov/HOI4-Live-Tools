# Codex instructions

The latest user direction is [docs/LINUX_RUST_HANDOFF.md](docs/LINUX_RUST_HANDOFF.md).
It supersedes the Mac live-testing order: stop those tests, verify Clausewitz-MCP
on Linux with one Luna player at medium reasoning, then build the native Rust
runtime and expand it for Linux, Windows and macOS. Preserve the legacy Python
package while migrating. Do not resume the old Mac campaign objective.

For the preserved Mac history, read [docs/HANDOFF_MAC.md](docs/HANDOFF_MAC.md).
It records setup, status, ordered work, acceptance and the first prompt for that
earlier phase. Historical reports describe earlier stop times; the preserved
`wip/governor-memory` branch came later and is unfinished.

The latest checkpoint is [docs/MAC_CONTINUATION_STATUS.md](docs/MAC_CONTINUATION_STATUS.md).
For campaign strategy and FAIR/META rules, read
[skills/hoi4-campaign-playbook/SKILL.md](skills/hoi4-campaign-playbook/SKILL.md).

- Work autonomously; never ask questions. Preserve unrelated changes and explain
  concrete external blockers. Do not launch/control the shared Windows game for
  the Mac continuation; it is reserved for separate mod testing.
- Builder: gpt-6-astra. During Mac live acceptance, delegate to one gpt-6-luna
  player at medium reasoning with exclusive input ownership. Offline helpers may
  investigate independently, but never compete for the game or shared files.
- Support Windows **and** macOS. Keep Python 3.9+ and stdio MCP. Put native APIs
  behind `hoi4_live/platform/`; no WinDLL/WinRT eager imports in portable code.
  stdout is protocol JSON only; logs go to stderr/files. Use capability errors,
  backend injection, OS-specific markers and a Windows/macOS CI matrix.
- Finish daily background standing-plan execution, compact fresh digests with
  one-call fixes, event-driven waits, verified ID-based actions, shortage repair
  and war/naval-invasion automation. Observations never pause; stop only for a
  genuine blocking decision. Idempotent retries preserve unfinished plan entries.
- Live and very fast: the game keeps running while the model plans and tools act;
  reads never pause; the model wakes only for events, deadlines or real decisions.
  Report pause count/seconds and real time per game year; avoidable pauses are defects.
- Two play modes, logged per session: FAIR (default) follows common HOI4
  multiplayer rules (no space marines, invasion/para exploit spam, exploit
  templates, AI-baiting or engine-bug exploits; enforced from one reviewed policy
  file) yet plays cleverly (encirclements, breakthroughs, air superiority,
  defensive lines); META allows metas and exploits a human can do via the UI.
  Cheating is forbidden in both. Showcases run in FAIR mode.
- Reference: https://github.com/smerss/Clausewitz-MCP (MIT): memory reads,
  command-queue posting, wait_for, micro engine, encirclement search.
- Campaigns use Normal difficulty, vanilla rules, historical AI and observational
  telemetry only. No console/debug/eval/fire_event, save editing, hidden enemy
  intelligence or reload-to-undo. Test-mode features are for separate mod QA and
  must remain inaccessible to campaign automation.
- Check PID creation/executable ownership, not PID alone. Never take over or kill
  a user's unrelated game. Back up settings/playsets and restore only after the
  owned game exits. Task access denial must allow telemetry/UI fallback; do not
  disable SIP, re-sign the game or run the whole agent as root.
- Do not copy build offsets across platforms. Validate bounded read-only memory
  against own-country telemetry/screen; unknown builds and stale/torn reads fail
  explicitly. Native command posting is unproven; only verified ordinary player
  actions may ever enter a campaign command adapter.
- Run `python -m unittest discover -s tests -v` after meaningful changes, plus
  targeted protocol/import/backend checks. Offline tests are not live evidence.
  Acceptance is a clean two-year Luna run, then the full recorded Germany and
  Nationalist China campaigns specified in the handoff. Log actual failures.
- Keep recordings, calls.jsonl, journals, dumps, saves, credentials, private mod
  data and personal paths outside Git. Audit **full reachable history** of every
  pushed branch/tag. Never push `private/*`, `--all`, `--mirror` or arbitrary tags.
- Attribute consulted ideas to smerss/Clausewitz-MCP in NOTICE. Retain upstream
  MIT copyright/license and file provenance if code is later copied.
- Preserve `wip/governor-memory`; release only tested changes. Version/changelog,
  audited archives and release pins for HOI4 Mod Setup/Agentic HOI4 Modding must
  agree. Windows downstream pins move only after the release is accepted.
- Never add Co-Authored-By, "Generated with" or any AI attribution line to commits or PRs: the user is the sole author.
