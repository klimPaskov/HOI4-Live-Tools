# Windows native port

The released Windows Python package and the Rust preview are separate. See
[Windows usage](LEGACY_WINDOWS_USAGE.md) for the release. The Rust server still
reports `platform_unavailable` for Windows game attachment. No Windows HOI4
campaign or native input acceptance is claimed.

## Current foundation

[`platform/windows`](../native/src/platform/windows.rs) is an internal read-only
foundation with injected tests and a Win32 implementation. It implements neither
`platform::Process` nor `memory::ReadMemory`, registers no tool or CLI option, and
changes no existing session format. The production build allowlist is empty:
opening a production reader refuses with `unsupported_build` before requesting
VM-read rights or reading remote headers or payload.

The reader retains the original game and supervisor handles. Ownership includes
exact creation FILETIME, complete binary SID, executable path, volume and 128-bit
file ID, direct parent relationship, and executable module mapping. It never
adopts replacement processes. Native handles request query/synchronize rights,
with read rights available only after an exact test-only profile is approved.
There is no write, injection, privilege adjustment, input or process-control API.

Reads are bounded and require committed readable regions and exact byte counts.
A batch checks full ownership before and after its transfers; cheap original
handle checks and an optional caller deadline/cancellation check surround each
transfer. Results are all-or-error, and lifetime failures quarantine the reader.
These checks do not make mutable game data atomic. A future snapshot adapter
must use batches, retain complete structural witnesses and preserve its original
deadline. No Windows read latency has been qualified.

The supported subset is native x64 Windows, unelevated processes, local paths,
regular non-reparse executable files and bounded PE32+ headers. Other cases
refuse. See [module policies and provenance](../native/src/platform/windows/README.md)
for exact limits and the test contract.

## Verification

Portable fixtures exercise ownership changes, cleanup, PE/SHA identity, read
bounds, guard pages, short reads, whole-batch cancellation and sticky failures.
A 64-field fixture requires exactly two full inspections per process for the
whole batch; it is a call-count check, not a native timing measurement.

The Windows test suite includes ABI-size checks and a parent fixture that launches
only its own test executable, reads a known synthetic buffer, then refuses after
that child exits. Its synthetic PE/SHA approval exists only under `cfg(test)`;
production arguments and manifests cannot enable it. On an elevated runner,
the fixture tests pre-open elevation refusal and explicitly reports the positive
transfer as pending. Cross-compilation and injected tests do not establish
native Windows execution or HOI4 compatibility.

Run the normal checks from the repository root:

```sh
cargo fmt --manifest-path native/Cargo.toml --check
cargo clippy --manifest-path native/Cargo.toml --all-targets --locked -- -D warnings
cargo test --manifest-path native/Cargo.toml --locked
```

Three-platform CI runs shared checks on Windows, Linux and macOS. Inspect the
Windows fixture output before describing its positive read path as verified.

## Remaining work

1. Validate the native reader on an unelevated owned Windows test child, including
   cancellation, cleanup and measured batching latency.
2. Recover and review Windows build identities, MSVC strings/RTTI and layouts
   independently. Add exact reviewed profiles only after bounded own-country
   reads agree with ordinary visible game information. Linux/Mac offsets and
   string layouts are not Windows profiles.
3. Add an owned launcher and session contract for Windows identities. Keep the
   released Python package compatible; do not translate FILETIME or SID into
   Unix timestamp/UID fields. Back up settings and restore them only after the
   exact owned game exits.
4. Integrate the reader through explicit build/ABI selection. Unknown builds,
   replacement ownership, stale reads and torn snapshots must refuse.
5. Implement ordinary input with owned-window/foreground checks, client geometry,
   DPI awareness, keyboard layout, held-input handling and point occlusion.
   Preserve once-only journals, unresolved-input blocking and finite leases.
6. Qualify recording, event waits, standing plans, wider economy and warfare
   actions independently at native 1920×1080 or higher.

Never control the shared Windows game reserved for mod testing. Use a separately
owned instance for future live validation. Campaigns use Normal difficulty,
vanilla rules, historical AI and FAIR by default. No console/eval, memory writes,
engine injection, hidden enemy information, save editing or reload-to-undo is
permitted. All final showcase videos must come from native macOS; Windows
validation does not replace that requirement.

Acceptance remains a clean two-year run by one Luna player at medium reasoning,
followed by the campaigns in [the current handoff](LINUX_RUST_HANDOFF.md). No
complete campaign has been accepted. Preserve failures and distinguish offline,
synthetic native and game evidence. Keep private artifacts outside Git; release
tags and downstream pins move only after release acceptance.
