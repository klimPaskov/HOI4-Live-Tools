# Integration

How HOI4 Live Tools is installed, registered and used by the other two projects of the workflow.

| project | what it does with HOI4 Live Tools |
|---|---|
| [HOI4 Mod Setup](https://github.com/klimPaskov/HOI4-Mod-Setup) | installs the release as an optional component and registers it for the user's coding clients |
| [Agentic HOI4 Modding](https://github.com/klimPaskov/Agentic-HOI4-Modding) | ships the pinned bootstrap that the setup app runs, the `hoi4-live-playtest` skill and the `hoi4-debug-playtest` routing that make live play the normal way to test a mod |

## The installer contract

`install.py` in the release archive is the only thing the other projects run. It needs Python 3.9 or newer on
Windows and network access to PyPI for the first install.

```
python install.py [install] [--prefix DIR] [--register auto|all|none|claude,codex,cursor,opencode] [--quiet] [--json]
python install.py status | register | unregister | uninstall [--purge]
```

- **Layout** (`%LOCALAPPDATA%\hoi4-live-tools` unless `--prefix` or `HOI4_LIVE_PREFIX`): `app\` (the program;
  `app\hoi4-live-tools.cmd` is what clients start), `venv\` (numpy, Pillow, opencv-python-headless), `config.json`
  (optional user settings), `artifacts\` (screenshots, reports, journals, logs), `install.json` (version, paths,
  registrations). The path never contains the version, so client registrations survive upgrades.
- **Steps**: copy the payload to `app.new`, swap it in, create or reuse the environment, `pip install -r
  requirements.txt` (binary wheels only), start the installed launcher, run `initialize` and `tools/list` and
  require the installed version and `hoi4_status`, register the clients, run `selftest` (informational: it needs
  the game and Windows OCR), write `install.json`.
- **Registrations** (per user, atomic, one backup `<file>.hoi4live.bak`): Claude Code `~/.claude.json`
  (`$CLAUDE_CONFIG_DIR/.claude.json`) `mcpServers.hoi4-live`; Codex `~/.codex/config.toml` (`$CODEX_HOME`) as one
  marked block with `tool_timeout_sec = 1800`; Cursor `~/.cursor/mcp.json`; OpenCode
  `~/.config/opencode/opencode.json` `mcp.hoi4-live`. `auto` registers the clients whose configuration folder
  exists. An existing `hoi4-live` entry that points at another launcher of this program is updated; one that belongs
  to another program is a conflict and is left untouched.
- **Exit codes**: 0 done; 1 install failed; 2 usage or unsupported platform; 3 installed but the server did not
  verify; 4 installed and verified but a client registration failed or conflicts.
- **Never touched**: the game, `settings.txt`, `dlc_load.json` (only `hoi4_telemetry` edits the playset, with a
  backup), saves.
- **Uninstall** removes `app\`, `venv\`, `install.json` and the registrations it wrote; `--purge` also removes
  `config.json` and `artifacts\`.

## HOI4 Mod Setup

The setup app lists **HOI4 Live Tools** under the optional workflows. Selecting it adds two managed components of the
Agentic HOI4 Modding manifest:

- `mcp.hoi4_live.bootstrap`, the script `.tools/mcp/bootstrap_hoi4_live_tools.py` with one reviewed action
  (`--quiet`, current user, network access and expected writes declared in the manifest). It downloads the release
  archive named in the script, checks its SHA-256 and size against the pinned values, extracts it to a private
  temporary folder (no absolute paths, `..` or links), runs that folder's `install.py --quiet --register auto`
  with the same Python, and deletes the folder.
- `workflow.live_tools`, the `hoi4-live-playtest` skill and its setup reference, which the project instructions
  route to.

The app runs the action after the project files are in place and shows the outcome on the project readiness page:
**ready** (exit 0), or **incomplete** with the installer's message (no network, Python older than 3.9, a registration
conflict, the server not verifying). The project files stay either way. Removing the workflow removes the skill; the
program under `%LOCALAPPDATA%` and the client registrations are external tool state and are reported, not deleted
(`python install.py uninstall` removes them).

## Agentic HOI4 Modding and the playtest workflow

The `hoi4-live-playtest` skill is the generic operating guide for the tools: the checks before the first call, the
efficiency rules, a feature-test recipe (setup, trigger, advance, inspect, diff saves) and the failure rules.
`hoi4-debug-playtest` is the bounded QA workflow (launch, clean-start gate, repair loop, coverage ledger); when the
server is connected, it plays through these tools instead of controlling the desktop itself. A mod project that
wants its own playtest skill keeps the mod-specific scenarios there and defers the
mechanics to the generic skill.

Rules the skills enforce: one desktop lock held across a run and always released; reuse the running game and the
loaded save; repeat only the step that failed; save only when a deep read needs a file; compare saves with
`hoi4_save_query query=diff` for what a change did; leave the game paused on the map; live testing is requested by
the user, never started on an agent's own initiative.

## Updating

A new release changes `__version__`, `pyproject.toml` and the changelog together (`scripts/build_release.py` refuses
a mismatch), is built with `scripts/build_release.py`, and the bootstrap's pinned version, URL, SHA-256 and size and
the manifest's matching parameters are updated to it ([RELEASING.md](RELEASING.md)). Running the bootstrap again, or
`python install.py` from a newer archive, upgrades in place.
