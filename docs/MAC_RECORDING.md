# Native Mac window recording preview

`hoi4-live-native --record OWNED_SESSION RECORDING_CONFIG` records one explicitly
owned game window on macOS 15 or later. Rust validates the request, creates private
files and supervises a bundled ScreenCaptureKit bridge. Building the bridge needs
Xcode command line tools and a macOS 15+ SDK. Linux and Windows return a capability
error; their builds do not compile or link Apple frameworks.

The recorder uses continuous H.264 MP4 capture without screenshots, game input,
cursor movement or pause commands. It records the window including decoration,
with no audio. It requires existing Screen Recording permission and does not
request elevated process access. A denied game task port does not prevent recording;
the owned process pair and exact inspected executable still have to match.

Create a private JSON configuration using the freshly verified OS window ID and
rectangle. The following paths and geometry are examples, not discovery defaults:

```json
{
  "output": "/absolute/private/captures/run-001",
  "window_id": 1234,
  "frame": {"x": 0, "y": 53, "width": 1920, "height": 1108},
  "fps": 30,
  "max_seconds": 3600,
  "max_bytes": 20000000000,
  "min_free_bytes": 4000000000
}
```

The output directory must not already exist; its parent must exist. A new
directory is created with mode 0700. It contains the recording, configuration,
owned bridge executable, diagnostic log and continuously updated `result.json`.
Keep the entire directory outside Git. Output is one encoded pixel per logical
window point. Width and height must be even and at least 1920×1080; this check
alone does **not** prove a 1080p game client inside the titlebar. Independently
qualify the game graphics and input geometry before filming a campaign.
`client_bounds_verified` remains false in recorder metadata because that evidence
belongs to the separate input binding.

Wait for `recording_started: true` and complete frames before beginning a test.
To request a clean stop, create `stop.request` in that exact output directory.
The foreground Rust command waits for finalization and prints one JSON result
on success. Readiness is available in the status file; no MCP server or input
lease is required. A final result is successful only when the child exits cleanly,
the recording has started, complete frames exist and its recorded duration is
positive. Inspect the resulting MP4 for resolution, cadence and readable UI before
accepting a campaign video.

Every 250 ms the bridge checks exact game and supervisor identities, their parent
relationship, its Rust owner's identity, the unlocked owning desktop session and
the pinned visible window rectangle. Changed geometry or ownership stops capture
and retains the exact failed witness. It does not bind a replacement window.
Blank, suspended or unknown active frame statuses fail; unchanged-display idle
frames are counted separately. Startup and finalization each have a ten-second
deadline, with an outer Rust deadline as a final bound.

Duration is bounded to six hours. Storage thresholds are checked every 250 ms;
they are stop thresholds, not byte-exact file caps, since encoding and finalization
may add buffered data. Reaching a storage threshold is a failure with partial
evidence retained. Owner loss also finalizes the partial video as a failure.
The recorder never kills or closes the game. An existing output is never replaced.

This is a recording preview. Short lifecycle tests do not establish long campaign
reliability, and recording success does not establish gameplay acceptance.

The `frames_complete` screen-output callback counter and the MP4's encoded frame
count are independent observations; do not assume exact equality. Preserve the
final result and an independent media probe. The Find-navigation component, for
example, recorded 2,012 encoded frames and 2,011 complete callbacks with a clean
explicit stop. Short component recordings do not establish campaign-duration
reliability.
