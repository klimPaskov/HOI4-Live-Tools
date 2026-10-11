# Releasing

Nothing here is automated; a release is a short list of commands and one upload. Run them from the repository root
on Windows.

## Checklist

1. **Version.** Set the same version in `hoi4_live/__init__.py`, `pyproject.toml` and the top entry of
   `CHANGELOG.md` (`## 1.0.1 - date`). Semantic versioning: a new tool or argument is a minor release, a fix a patch,
   a removed or renamed tool a major one. Update the tool count in `README.md` and `docs/STATUS.md` if it changed
   (`python -m hoi4_live docs` regenerates `docs/tools.md`).
2. **Tests.** `cd tests` then `python -m unittest discover -s . -p "test_*.py"`. The top-bar and panel readers need
   the Windows OCR host and the game's files; run the installer tests alone with `python -m unittest test_installer`.
3. **Live check.** With the game closed or idle on the main menu, install the candidate into a scratch prefix and
   look at it:

   ```
   python scripts/build_release.py --allow-dirty --out %TEMP%\hoi4live_candidate
   mkdir %TEMP%\hoi4live_try && tar -xf %TEMP%\hoi4live_candidate\hoi4-live-tools-<version>.zip -C %TEMP%\hoi4live_try
   python %TEMP%\hoi4live_try\hoi4-live-tools-<version>\install.py --prefix %TEMP%\hoi4live_prefix --register none
   %TEMP%\hoi4live_prefix\app\hoi4-live-tools.cmd selftest
   python install.py uninstall --prefix %TEMP%\hoi4live_prefix
   ```

   The install must end with `server answers: N tools` and exit code 0. On a machine with a game, run one short
   scenario (`hoi4_status`, `hoi4_screen`) through the installed launcher.
4. **Licence.** `LICENSE` is Apache-2.0 and `NOTICE` lists the dependencies the installer fetches; re-check
   `NOTICE` when `requirements.txt` changes (no dependency may be added whose licence forbids this use).
5. **Commit** everything that goes into the archive. `build_release.py` refuses uncommitted changes to payload files.
6. **Build.** `python scripts/build_release.py`. It writes `dist/hoi4-live-tools-<version>.zip` and
   `dist/SHA256SUMS.txt` and prints the SHA-256. The archive is deterministic: the same commit gives the same
   bytes, so a rebuilt file can be compared with the published one.
7. **Pin it in the Agentic HOI4 Modding repository.** In `.tools/mcp/bootstrap_hoi4_live_tools.py` set `RELEASE_VERSION`,
   `RELEASE_URL`, `RELEASE_SHA256`, `RELEASE_SIZE`, and update the same values in the `mcp.hoi4_live.bootstrap`
   component of `hoi4-mod-setup.manifest.json` (the contract test `scripts/test_live_bootstrap_contract.py` fails
   until both agree). Run `python -m unittest discover -s scripts -p "test_*.py"` there.
8. **Publish.** Tag and push this repository, create the GitHub release `v<version>` with the changelog entry as its
   notes and attach exactly the built `hoi4-live-tools-<version>.zip` and `SHA256SUMS.txt`:

   ```
   git tag -a v<version> -m "HOI4 Live Tools <version>"
   git push origin master --tags
   gh release create v<version> dist/hoi4-live-tools-<version>.zip dist/SHA256SUMS.txt --title "HOI4 Live Tools <version>" --notes "<the changelog entry>"
   ```

   Download the asset from the release page and compare its SHA-256 with the pinned value before pushing the
   Agentic HOI4 Modding repository.
9. **Push the dependents** in this order: Agentic HOI4 Modding (CI refreshes its manifest), then HOI4 Mod Setup
   (its source manifest copy follows the published manifest).
10. **After release.** Record the version in `docs/STATUS.md` and, for a machine that runs agents, run
    `python install.py` from the new archive so the installed copy matches.

## What the archive contains

The files listed by `install.py` (`PAYLOAD_DIRS`, `PAYLOAD_DOCS`, `PAYLOAD_FILES`): the package, the campaign skill,
the installer, the launcher, the licence and notice, the readme, changelog and the main docs. Tests, fixtures and
the evidence images stay in the repository.

## Known limits to repeat in the release notes

Windows only; windowed or borderless at interface scale 1; the screen language of the game must match the OCR
language; the campaign tools were verified on the vanilla interface with one large mod; the phase 4 exit check was
not repeated end to end on the final code and the two-year campaign runs of the plan were not made (see
`docs/STATUS.md`).
