# Durable Mac controller identity

The native Mac controller now keeps its campaign identity across stdio server
restarts. This is a prerequisite for the durable daily worker; the existing Mac
standing-plan runner is still synchronous and does not gain background execution
from this change.

Run the preview with an exact owned session and a private input directory:

```sh
hoi4-live-native --session /private/run/session.json --mac-input /private/run/input
```

The directory must be private to its owner (mode `0700`). Its `input.json` names
the configured controller and own country, for example:

```json
{"controller":"player","player":"GER"}
```

Binding creates `controller-epoch.json` only for a directory without previous
nonempty action or daily-worker history. It records the original epoch, exact
process creation timestamp, own player, playthrough, context roots, game setup
and highest observed date. Reopening retains the original nonce; it does not
restore an input lease. Competing writers, replaced state files, different
campaigns and date regression refuse. A failed identity/storage check remains
blocked for that controller instance, even if a later check looks valid.

A same-date observation validates the retained state without rewriting or
syncing it. An advancing date is persisted. This avoids idle disk writes while
preserving restart chronology. State files and all action history are private
runtime data and must stay outside Git.

Older input directories without this identity record refuse automatic migration
if they contain history. Preserve those directories and their unresolved actions;
there is no automatic reset or migration that establishes old work as safe.

## Action limits and recovery

Research and production calls carry one five-second action deadline, narrowed by
the existing input lease. Nested reads and geometry checks retain that deadline;
UI stage timers cannot extend it. Nested guards retain the original checks. A
bounded read that finishes after expiry is discarded. Drag motion rechecks before
each step, and a cancellation or motion failure still attempts button release at
the last posted point. No observation pauses the game.

The Mac clock and gameplay journals share an input quarantine. An unresolved
request blocks a different input domain, a new request ID and UI queries that
would send input. Only the recorded request can use its journal's existing
read-only replay or settlement path. A failed journal write remains blocked for
the instance. Truncated or oversized journal records refuse on reopen.

The server and controller retain the same opened process handle through shared
ownership. This does not create a worker or grant access to another process.

## Qualification limits

Generated tests cover restart identity, storage replacement, writer exclusion,
chronology, cross-journal quarantine, deadlines, cancellation, nested guards,
drag cleanup and shared process lifetime. The live qualification record is in
[CLAUSEWITZ_VALIDATION.md](CLAUSEWITZ_VALIDATION.md). Durable Mac worker input,
completed mixed-plan restart, the two-year Luna run and full recorded campaigns
remain required.
