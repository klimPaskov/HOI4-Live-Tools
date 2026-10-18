# Changelog

## Unreleased native preview

- Campaign actions on macOS (native MCP, Steam 1.19.3 x86_64 under Rosetta,
  Mach-O 089EA519), each driven by ordinary CGEvent input to the owned,
  front-most window, targeted from the live GUI tree and verified by fresh
  memory readback, with an idempotent action journal: time, research (with
  tree scrolling), national focus (tree panning), production lines (add/set,
  offered designs listed on refusal), construction, laws, designers and
  advisors, decisions, division training with deployment locations, army
  creation, army groups and field marshals, front and offensive lines
  (found by comparing captures before and after front-line mode),
  battle-plan execute/stop, event answers (including newspaper-style news
  windows), diplomacy (list, Justify War goal, sent actions), and air wings
  and missions. Reads add armies by group, unassigned divisions by state,
  states by name or country tag, open events and an open peace conference.
  Army groups, field marshals and army splitting (Shift range selection in the
  division list needs a real modifier event). Not yet supported: the peace
  conference itself and naval and invasion actions.
- A country-agnostic campaign playbook (`skills/hoi4-campaign-playbook/
  references/generic-playbook.md`): army caps, army groups and field marshals,
  templates, build-up, planning and encirclement, the peace conference and
  common mistakes, from a cited research run checked against the 1.19.3 files.

- First native macOS read. `hoi4_overview` now selects a per-build layout:
  Linux ELF a4971c9e or macOS Mach-O 089EA519 (1.19.3.0 c01a under Rosetta).
  On the owned 1920x1080 game on 18 October 2026 it returned the player, date,
  speed, pause, difficulty, historical AI, Focus and research slots in about
  0.4 ms (99 reads). The Mac offsets were mapped live from RTTI roots and value
  changes across unpause, speed and player switches, and kept in a private
  evidence table. The image search now reads the declared Mach-O load commands
  in full. Other macOS tools and all macOS input remain unported.
- Start the native macOS adapter. `platform::macos` attaches read-only to one
  owned game process (the x86_64 Mach-O build 1.19.3.0 under Rosetta) through
  its task port. It verifies the executable SHA-256 and LC_UUID, the process
  start time, path and non-root uid on every check, and finds the image by its
  Mach-O header and UUID. The reader decodes libc++ strings for this build. The
  binary must be ad-hoc signed with `native/macos/debugger.entitlements`, and
  the user must hold Developer Tools task access. No game layout is wired yet,
  so macOS tools still refuse.
- Run092 (18 October 2026, `b0b9b35`, package v87, Haiku 5.5 player, 512x288)
  passed the map placement gate for a coastal building. The server selected a
  dockyard, used the Find view to centre the camera on West Prussia (state 63)
  and queued one dockyard with one map click. place verified in 1.7 s with
  `{in_state 1, elsewhere 0, complete true}`. The open, select and place replays
  returned identical historical receipts, and a second place ID was a zero-input
  no-op. The final queue held exactly one dockyard of amount 1 in state 63
  (location kind state). In this game data a dockyard is a coastal-only state
  building, so the client-centre click needed no province target. The state's
  Find anchor was predicted offline at map position (3059, 1599): the rounded
  bounding-box centre of its provinces, which also reproduces Brandenburg's live
  anchor. That point is inside coastal province 9361, 8 map pixels from the
  nearest outside pixel, which is sea. The live anchor matched exactly, and the
  private broker required that match before any Find key or map click. Lease
  release, exit 0, recording and instance absence passed. True provincial
  buildings (naval bases, bunkers) remain untested.
- New fixtures: placement summaries count dockyard lines by state and
  province-located lines by the state their province joins.
- Run091 (18 October 2026, `7f0728c`, Haiku 5.5 player, 512x288) passed the
  map placement gate. ensure_open, select_building(arms_factory), find_open,
  find_query(64), find_activate(64) and place(arms_factory, state 64, amount 1)
  each sent one server-confirmed ordinary input with canonical readback; place
  verified in 1.9 s with `{in_state 1, elsewhere 0, complete true}`. The open,
  select and place replays returned identical historical receipts, and a second
  place ID was a zero-input no-op. The final queue held exactly one arms_factory
  of amount 1 located in Brandenburg. Lease release, exit 0, recording and
  instance absence passed. Provincial placement is not yet live-tested.
- Re-observe after a torn terminal witness. Run090 (v85) proved the placed
  arms_factory located in state 64 (`status_flags_raw` 6), but the confirmation
  read after input-idle tore in the panel's choice list and the stage recorded
  uncertainty, although the last whole snapshot had already reached the target.
  A torn confirmation read now restarts observation and idle confirmation within
  the same original deadline. A persistently torn witness never verifies.
- Prove construction locations of state buildings queued live. Run089 showed that
  one map click queued exactly one arms_factory line (amount 1, 15 factories), but
  its building status carried flag bits beyond the constructor's 2, so the location
  join refused it as `unsupported_location_status_flags` and the place stage
  stayed uncertain. The reader now takes province or state from bit 0, as the
  engine does. It still requires the exact embedded status, catalog ID and
  reciprocal building slot, and it reports `status_flags_raw`.
- Runs 087–089 (18 October 2026, Haiku 5.5 player, 512x288): ensure_open,
  select_building(arms_factory) and all three Find stages pass with the
  construction panel open. Run087 stopped on an over-strict broker check of Find
  receipts. In Runs 088 and 089 the place click was delivered and a new queue line
  appeared, but the verifier could not prove its location, as above.
- Add the opt-in `place` construction stage: with the canonical building
  selected, the construction panel shown, the game's Find view closed and the
  camera centred on an own state, one left click at the owned client centre must
  bring the queued amount of that building in that state to the requested
  total. The queue is read in the same snapshot, every line of the building
  must have a proven own location, and a click landing elsewhere never counts.
  Asking for an already-reached total is a no-op. Not yet live-accepted.
- Run086 on `3f3d731` passed the keyboard Find gate live at 512x288 with the
  Haiku 5.5 player: `find_open` (382 ms), `find_query` for state 64 (437 ms;
  first of 21 results was the state) and `find_activate` (303 ms) were each one
  server-confirmed ordinary input with canonical readback. Their identical
  replays were historical, a separate activate request was a zero-input no-op,
  and the camera look-at moved to Brandenburg's template position `[3008,
  1569]` with the view closed. Map placement and queueing are still pending.
- Run085 on `679f9e0` passed `find_open` live again; `find_query`'s readback
  refused because typing matched more than an arbitrary 256 results. Only the
  first entry is read, so the bound is now the reader's array limit, and a
  first-row/entry disagreement during refresh is reported as a torn read that
  post-input readback waits out.
- Run084 on `236e3bc` passed `find_open` live: one server-confirmed `f` opened
  the game's Find view with its edit box focused, and the identical replay was
  historical. `find_query` typed the digits but its readback refused, because
  recycled result rows keep their object only in a kind-specific field. The
  observer now reads rows by kind; `find_query` verifies the typed query and a
  refreshed first row, and `find_activate` refuses before input unless the first
  row is the requested state (a province can also match a numeric ID).
- Run083 on `10907e9` sent the first Find stage: the `f` shortcut was
  server-confirmed and the game's Find view opened, but readback compared the
  GUI text receiver with the edit box itself. In this build the receiver is the
  box's embedded receiver (as already proven for the Focus search box), so the
  stage ended durably uncertain and no further input followed. Focus is now
  read as keyboard focus on the box plus its embedded text receiver.
- Add three opt-in keyboard Find stages to `hoi4_construction_attempt`:
  `find_open` presses the game's own `f` shortcut for its map Find view,
  `find_query` types an own state ID's digits into the focused empty query, and
  `find_activate` presses Return to activate the verified first state row. Each
  is one durable ordinary input with same-snapshot readback: view visible with a
  focused text box, exact query and first state row, then a closed view with the
  camera look-at on the state's template position. Existing receipts and the
  public controls output are unchanged. Not yet live-accepted.
- Run082 on `62bcbab` passed the read-only Find view gate at 512x288 with the
  Haiku 5.5 player: all calls succeeded in about 32 ms with no input. The start
  camera's look-at `[3010, 9.5, 1568]` sits about two map units from
  Brandenburg's state-template position `[3008, 1569]`, the unopened Find view
  reported hidden with no results, and a foreign state's anchor stayed unread.
  Opening, typing, result rows and placement remain unexercised.
- Run081 on `350a633` passed preflight (no torn snapshot) and the Find view
  observer's first live call named the real type at the state's anchor field:
  a state template (`CStateTemplate`), not a province. The observer now reads
  that template's map position, which the game's camera centring uses. No input
  occurred; recording, clock gates, release and closure passed.
- Run080 stopped at preflight: the pregrant read-only `hoi4_construction_controls`
  read returned the recurring opener torn read (`read=171; size=4;
  changed_byte=1`). No grant or player call happened. Every such refusal so far
  followed a root click on the construction opener, with the pointer left over
  that button; its hover state is the likely changing field (unproven). The
  read-only construction controls and Find view tools now use the same bounded
  whole-snapshot recovery as action pre-input reads and report discarded
  snapshots.
- Run079 on `e58058b` tested the read-only Find view observer at 512x288 with the
  Haiku 5.5 player. Its first call refused with a shared type-check message, so
  no Find view, camera or anchor values were obtained. Recording, four clock-skew
  gates (all under 0.04 s), lease release, normal exit and exact-process closure
  passed. The observer now names the object and its image-relative type on any
  type refusal.
- Add read-only `hoi4_construction_find_view`: the game's Find view state, first
  result row joined to the state catalog, camera look-at and an optional own
  state's anchor province, as groundwork for map placement. No input; live
  agreement pending. Record the 1080p calibration finding: an owned 1920x1080
  game stayed on Loading GUI for over 30 minutes on the QEMU TCG VM, so live
  runs stay at 512x288 there until a KVM or native host exists.
- Run077 on `5e61fec` passed the bounded Linux construction action gate: one
  ordinary click opened the construction panel (1.59 s) and one selected the
  dockyard (0.85 s), each with server-confirmed delivery and canonical readback.
  Both identical replays returned the historical receipt without input, and both
  no-op requests verified with no delivery. Each action discarded exactly one torn
  `opener` snapshot after intent and before input, then used a fresh coherent
  read. Lease release, normal game exit and exact-process closure passed. The
  recording failed: a guest clock step made ffmpeg spend its run filling
  duplicate frames, so the grant window is not on video. This is a construction
  action gate, not campaign acceptance. The player was Claude Haiku 5.5 at medium
  reasoning.
- Record Run076's repeated pre-input construction refusal: controls passed in
  72 ms, then opening refused in 114 ms with `section=opener; read=171; size=4;
  changed_byte=1`. Empty action journals prove no input. Canonical release,
  finalized recording, normal game exit and exact-process closure passed.
  The request-relative ordinal alone does not identify the semantic field.
- Bound construction pre-input torn-snapshot recovery to four whole producer
  reads, one second after the first rejection, 25 ms intervals and the original
  lease/call deadline. Explicit initial, candidate, before-intent, after-intent
  and final dispatch-check phases preserve rejected-read diagnostics. Context
  acceptance, predicates, scope/point checks, journals and input never retry;
  post-input readback is unchanged. Live action acceptance remains pending.
- Record Run075's first construction-action refusal: the initial controls read
  passed in 44 ms, but `ensure_open` returned `torn_read` before durable intent
  or input. Luna stopped, released control and made no further game observations.
  Four empty action journals, finalized recording and normal closure were audited.
- Add opt-in construction witness diagnostics identifying the producer section,
  first-read ordinal, read size and changed-byte position without addresses or
  payload. Witness checks still refuse unchanged; no automatic retry is added.
  Run075's exact changing field remains unknown.

- Add an explicitly opted-in Linux construction action adapter for ensuring the
  panel is open and selecting one canonical ordinary building type. Pin private
  witnessed identities and the original point before one ordinary click; verify
  canonical readback and persist no-ops, historical receipts and uncertainty.
  Construction uncertainty blocks all shared input adapters and background plans.
  Live action acceptance and map placement remain pending.

- Record Run074's single combined construction read: five successful player RPCs,
  correct lease/capture ordering, 81 ms controls response and zero player input.
  Preserve both false local dispatch gates and the recording-manifest cleanup
  race; finalized recording, canonical release and normal process closure passed.
- Observe Standard's optional parent gate only after joining the exact named own
  window's typed position object. Unknown pointers/types remain unavailable.
  Add durable EnsureOpen/SelectBuilding stage foundations with pinned identity,
  no-op handling, once-only dispatch and persisted uncertainty/refusal causes.
  The separately opted-in adapter above uses this foundation; live action
  acceptance is pending.

- Record Run073's failed combined controls read: correct lease ordering, one
  capture, zero player input and `Unique construction control names required`.
  Correct the overstrict registry policy to require only the requested name to
  be unique, including location text. Preserve full bounded validation and final
  witnesses; the exact failing collection/name remains unknown. Run074 later passed the corrected lookup.

- Add read-only `hoi4_construction_controls` for one canonical building ID.
  One witnessed snapshot joins the nested topbar opener, catalog-backed choice,
  exact callback bindings, local blockers and optional typed geometry. Unknown
  controls remain explicit; no construction input or routing authority is enabled.

- Add optional canonical construction-selection readback under `include_ui`,
  with fresh catalog/ordinal checks and explicit unsupported modes. Static and
  generated evidence only; no construction input is enabled.

- Record Run072's paused dockyard state 56/Weser-Ems label comparison, with
  35–37 ms UI reads and zero player inputs. Preserve the late lease acquisition
  as a protocol deviation rather than a clean pass. Retain the setup menu
  misclick/close and early campaign-unavailable response. Audit canonical release,
  empty action journals, finalized recording and normal owned process closure.

- Add optional `include_ui` construction observations. Within the same witnessed
  snapshot, join exact own panel rows and both full handle copies to located own
  building lines before reading the named location text. Preserve default-off
  behavior and queue-index pagination. Stored UI text and local flags do not
  establish freshness, name agreement, eligibility or input authority.

- Add optional `include_location` to construction observations, default false.
  Selected state/province IDs require own ownership and control, current catalog
  joins, exact embedded status and reciprocal building/template membership.
  Preserve unsupported-location reasons and validate all witnesses before paging.
  Static/generated evidence only; live location comparison remains pending.

- Add read-only `hoi4_construction_queue` with full own-queue validation before
  paging, exact building/railway types, snapshot-scoped handles and canonical
  building keys. Independently distinguish cost from produced work, retain
  conversion/repair/allied flags, and reject stale or foreign-owned payloads.
  Run071 verifies one dockyard across advancing days with stable identity and
  increased produced work: 14 Luna RPCs, zero errors and 31–39 ms queue reads.
  Preserve manual setup's refused inland placement and startup reader error.
  Precise UI comparison, railway payloads, locations, allocation and construction
  input remain pending.

- Pass Linux run070 with one Luna medium player: the worker completes Rhineland,
  searches and starts the Four Year Plan, and preserves receipts/budget through
  restart without duplicate input. Independently audit 12 verified Focus input
  pairs and two assignment reservations; retain one read-only busy retry and
  recovered torn reads. This is bounded automation evidence, not a full campaign.

- Preserve Run069's interrupted initial segment and failed worker continuation.
  Correct the search reader's null country-override handling using the exact
  Linux binary's positive-player fallback, with own-country validation and
  witnesses retained. Add compact search predicate diagnostics; the corrected
  worker route passed run070 without identifying run069's missing predicate.

- Verify full manual Focus search-to-View in Linux run068, including three Tab
  steps and exact historical replays. Add optional explicit `focus_search` hints
  to daily Focus plans with durable ordered-chain validation and unchanged
  serialization for existing entries. Worker live validation remains pending;
  failed/uncertain chains and existing query text are never replayed or replaced.

- Verify Focus search text and two ordinary Tab selection steps in Linux run067.
  Preserve the third step's torn-read refusal before intent, with no pending
  action. Extend bounded snapshot recovery to the three pre-intent search read
  phases, preserving their diagnostic causes and the original action deadline.
  Ownership, state predicates, journaling and input are never retried.

- Preserve run055's passing paused geometry and run056's failed advancing
  second-Focus View, including its exact capacity boundary and actual Rhineland
  membership. Increase only the opt-in Focus position allocation cap to 16,384;
  keep total pointer reads, ordinary/root bounds, ownership and input checks.
  Live successive-Focus verification of the correction remains pending.

- Add optional base equipment keys to authenticated own production lines, with
  fresh reviewed catalog membership, exact type/intern joins and full snapshot
  witnesses before paging. Explicit unavailability covers unsupported variants
  and types. Custom variant identity, allocation and live UI validation remain
  pending; generated Linux fixtures do not establish live behavior.

- Keep portable geometry tests independent of Unix-only journal helpers. The
  persistence check stays in the Unix journal suite; the runtime is unchanged.

- Preserve run054's successful read-only recovery and failed successive-Focus
  gate. Three Four Year Plan View refusals occurred before input; their exact
  hierarchy limits were truncated and remain unknown. Canonical release,
  718.333-second recording and normal exit passed. Compress future policy
  diagnostics to retain all boundary quantities within the durable message bound,
  with no traversal, ownership or retry-policy change.

- Discard and retry torn Focus readiness snapshots before navigation or assignment,
  sharing the existing four-read/one-second bound. Recheck authority and day for
  each read; preserve rejected-read diagnostics in the worker's historical report.
  Invalid state, ownership loss and possible input never gain retry authority.
- Preserve run053's failed successive-Focus gate: five read-only refusals across
  game days, one Rhineland assignment, no Four Year Plan attempt, canonical release,
  622-second recording and normal exit. Run054 later exercised read-only recovery.

- Verify actual Rhineland Focus completion in run052, followed by same-plan
  restart/STOP without duplicate input. Preserve historical background read
  diagnostics and distinguish STOP's last-attempt-day budget from current-day
  digests. Recording and normal exit passed; full campaigns remain unaccepted.
- Describe shared stability errors as snapshot changes instead of research UI
  changes. Error codes, reads and retry policy remain unchanged.

- Scope Focus Start and selected-detail reads to the selected window and its
  Start button. View retains tree-node checks; ownership, canonical identity,
  Start callback/geometry and final torn-read checks remain required.

- Retain each new core NotQueued reason in its durable journal after later
  completion, and expose the refusal count/latest record in assignment history.
  Older missing reasons remain unknown; updates and restarts preserve budgets.

- Verify run050 actual research completion and same-slot queued refill over
  advancing game time, with one assignment reservation per day and unchanged
  restart receipts. Preserve both known-no-input navigation failures, an extra
  refused core reservation whose exact reason was not retained, and read-only
  busy/torn/deadline refusals. The 1,982-second recording and normal exit passed.
- Add the read-only `hoi4_focus_completed` preview. Its bounded own canonical
  membership reader follows the current engine predicate independently of GUI
  state; run052 later verified actual Rhineland completion.

- Verify run049 worker research-to-Focus-to-research assignment and both explicit
  target-wait cases. Preserve Luna's initial stop over an omitted summary receipt;
  independent saved journals proved the earlier transition, and a separate
  restart/STOP supplement preserved all completions and budget 3/3/0 without replay.
  The 561-second recording and normal owned exit passed. Completion over game time
  and full campaigns remain unaccepted.
- Mark the bounded reverse route as live-validated and clarify that Focus details
  contain the latest preparation summary, not the full navigation ledger.
- Separate current continuation instructions from preserved historical checkpoints
  and refresh preview/setup documentation around the current verified behavior.

- Add bounded fresh read-only Focus navigation readiness before any intent or
  reservation: at most four reads, a one-second retry window and the unchanged
  absolute deadline. Positive preflight remains mandatory; reader/context errors
  propagate and uncertain actions are never retried. Run049 passed the combined
  route; no evidence establishes whether the readiness retry branch fired.
- Preserve run048's pre-input Focus navigation refusal, two completed research
  assignments and budget2/3/1. Recording and normal exit passed; reverse navigation
  and successful assignment-target waits remain unaccepted.
- Retain bounded observed mode, roots, blocker counts and selection-state facts
  in future Focus navigation refusals. No new reads or input/retry changes.
- Add a separately typed research-to-Politics preview with positive own panel/tree
  facts, both domain permissions, preserved active Focus and durable nonreplay.
  Existing open-Politics semantics and old receipt serialization remain unchanged.
  Recorded reverse-transition validation passed run049.
- Verify run047 worker-only Focus-to-research assignment and adjacent same-plan
  restart/STOP, retaining both completion IDs and budget2/2. Preserve its one
  known pre-input SelectSlot retry; recording and normal exit passed.
- Add an explicit `until_assignments` target to the read-only server wait. It
  matches the first accepted snapshot and requires all requested active Focus
  and research-slot IDs, avoiding a missed-change timeout after work finishes.
  Successful and already-matching cases passed run049; run048 preserves
  historical-timeout evidence.
- Correct two uncertainty tests to inspect persisted bytes after releasing the
  journal lock. All nine Windows/Linux/macOS CI jobs passed the test-only repair;
  production locks and no-replay assertions are unchanged.

- Preserve run046's unresolved OpenFocus intent and worker quarantine. No Start,
  handoff, research or restart was reached; recording and normal exit passed.
- Retain bounded original causes for Focus/research readback, context, idle,
  authority and commit failures after possible input. They remain
  `action_uncertain`; journals, guards, deadlines and nonreplay are unchanged.

- Verify run045 public Rust ClosePolitics, identical historical-receipt lookup
  without new input, and research assignment while retaining active Rhineland.
  Recording and normal exit passed; no focus completion over game time is claimed.
- Add an opt-in automatic Focus-to-research worker handoff using fresh own-state
  checks and the shared guarded close executor. Retain its durable preparation
  history across restart without spending or resetting assignment reservations.
  Run047 passed combined-plan validation; the reverse direction remains pending.

- Preserve run044's terminal nonqueued Focus Start and consumed budget. No retry,
  close stage or research input occurred; recording and normal owned exit passed.
- Label Focus torn-read refusals with bounded static sections and known relative
  fields, without process addresses, memory payloads, new reads or weaker checks.

- Expose the guarded ordinary `close_politics` Focus stage after run043 verified
  the Q transition and subsequent Rust research assignment. Retain active-focus,
  local-blocker, lease, deadline and durable uncertainty checks; automatic worker
  handoff and the new Rust close-stage live gate remain pending.
- Preserve run043's initial stale pre-input refusal. A distinct guest-local
  diagnostic passed with the unchanged freshness limit, followed by five verified
  research stages (four input stages and one no-input category result). Recording,
  independent lease readback and normal owned exit passed.
- Record run042: read-only Focus UI before lease acquisition and after worker
  Start; immediate restart/STOP retained completion and durable budget 1/1.
  A duplicate update returned nonqueued `input_busy` and was not retried.
  Recording integrity, independent lease proof and normal owned exit passed.
- Derive STOP's historical budget from checked durable state, including after an
  immediate worker restart. Preserve unknown days and all consumed reservations;
  return explicit unavailability if storage cannot be verified.
- Add compact read-only `hoi4_focus_ui` for the owned canonical focus, managed
  mode, local roots and complete own blocker counts. It needs no input lease and
  exposes no command authority; the bounded run042 observation gate passed.
- Record run041: worker-driven Focus Start, active-focus waiting, idempotent plan
  update and two restarts without replay or budget reset. Five digests stayed
  below 2 KB, with eight exact historical detail lookups. Recording and normal
  owned exit passed; full campaign acceptance remains open.
- Keep overflowing daily histories in an exact read-only detail snapshot while
  returning a bounded summary with fresh counts, budget, completion counts,
  quarantine and historical failure codes. Stale detail IDs fail explicitly.
- Preserve Focus transport refusal causes without changing terminal NotQueued
  classification, reservation budgets or replay rules.
- Record run040's nonqueued Start, pending assignment and digest overflow.
  Lease release was verified; host restart interrupted recorder/close cleanup.

- Compact matching completed Focus Start by reference and omit only its successful
  preparation, retaining error history. Run039 verified one original-reservation
  pre-input retry and completion, then stopped on digest overflow before update/restart.
- Report durable assignment completion separately from fresh observations and
  batch receipts in digest/STOP. Preserve latest completion and counts when
  optional IDs truncate; reserve essential counts before filling the 2KB bound.
- Add a runtime-disabled typed ClosePolitics foundation with exact active-focus,
  settled mode/root and blocker guards. Live shortcut and panel handoff acceptance
  remain pending.
- Record run038 worker-driven Rhineland Start and durable assignment completion
  with one consumed attempt. Preserve the player reporting failure: digest
  historical IDs obscured completion, so update/restart were not attempted.
- Bound proven pre-input Focus Start torn-read retries inside the same in-flight
  reservation/request, preserving the original cause, history and daily budget.
  Surface core refusals beside preparation in digest/stop output.
- Return explicitly historical timeout evidence when a later wait sample misses
  its deadline. Discard late payloads; initial and real scope/ownership errors
  retain strict refusal behavior.
- Record run037 verified worker preparation through View, followed by a known
  nonqueued Start `torn_read` that exhausted its single reservation. Preserve the
  wait-deadline refusal and missing core-cause summary; worker acceptance is open.
- Wire a default-off daily Focus preview with separate owner/request permission,
  ordered heads, active-focus waiting and unchanged shared assignment budgets.
  Retain actual verified evidence when late returns require durable quarantine.
  Worker live validation and cross-domain UI handoff remain pending.
- Add bounded connected components and multi-source breadth-first search over
  supplied public/generated topology, with explicit truncation and deterministic
  source ties. The helper has no game-state producer or campaign action adapter.
- Pass recorded run036 exact Rhineland selection and one ordinary Start with
  canonical active-focus readback. Keep full campaign acceptance open.
- Add disabled daily Focus foundations: borrowed executor, strict Start receipt
  conversion, ordered head and separate durable preparation ledger. Worker
  enablement and cross-domain UI handoff remain separate gates.
- Add a compact exact selected-focus window observation after a fields-present
  parsing failure in run035. Keep all original witnesses and action guards;
  label selected-only listener provenance without claiming an absent NOOP.
- Add read-only own equipment-line pages with complete collection validation,
  lossless snapshot-scoped handles and separate signed factory counters. Unknown
  layouts expose bounded metadata; equipment joins and allocation remain unavailable.
- Record worker-only four-slot research assignment, idempotent plan updates and
  restart without resetting the consumed daily budget. Ordered queues now retain
  pending slot heads; refill after a technology finishes still needs live validation.
- Record exact focus selection with witnessed hierarchy/cache closure. Earlier
  runs stopped before Start on unavailable bindings. Bounded diagnostics identified
  a selected-only listener; its narrow
  event binding and the prior two-listener binding now require selector zero.
  Run036 subsequently verified one ordinary focus Start, as recorded above.
- Prefilter occupied research slots using a complete witnessed canonical probe.
  Fixture reads fall while occupied and increase while idle; live latency is unverified.

- Add witnessed own-focus command context, local UI facts and private Sprite
  point preparation. Keep public defaults unchanged; focus dispatch remains
  unavailable. Record one successful industry-category switch/no-op and a later
  electronics verification timeout, with durable quarantine and no retry.
- Combine category verification's canonical research and UI evidence under one
  witness set. Reduce generated logical reads by 58.5% while retaining validation,
  deadlines and replay policy; live latency verification remains pending.

- Add bounded own research UI diagnostics with exact widget/slot joins, primary
  button/render metadata and repeated structural witnesses. No click points or
  input authorization are exposed; installed-game validation is pending.

- Add a fresh own research model with completed membership, canonical joins and
  signed active counters; progress units and eligibility stay unavailable.
  Correct the GUI diagnostic's derived `CGameGui` primary-type check after a
  real installed-game refusal. Own completed membership and GUI dimensions now
  pass installed-game checks; active progress units and click mapping stay unverified.
- Retry only transient torn observations within server waits, bind their own
  campaign context and refuse post-deadline retries or changed context. Record
  the optimized Luna clock endpoint and wait failure, then a fresh passed wait
  and verified pause. Campaign acceptance remains open.
- Start a separate Rust stdio runtime with owned Linux process reads, bounded
  focus/research/clock snapshots, explicit capability errors and server waits.
- Verify selected upstream ordinary actions with one Luna medium player; record
  the final pause timeout and quarantine. No clean campaign acceptance is claimed.
- Measure Rust and guarded Python over the same persistent SSH path: 7.5 ms
  versus 294.9 ms median for 20 live overview samples each.
- Retain Clausewitz-MCP MIT provenance; add Rust tests and a three-OS CI matrix.
- Add durable action receipts with restart/replay quarantine and a Linux X11
  keyboard fallback building block. Recovery and isolated Xvfb transport fixtures
  pass; live command tools and governor integration remain disabled.
- Add a persistent ordinary-user Linux launch supervisor, private authenticated
  reconnectable stdio clients, pinned-child shutdown and mutable mapping checks.
  Generated-child fixtures cover reconnect failures, hung termination, fresh
  allocations, permission changes, partial reads, identity changes and exit.
  Actual owned-game frontend reads and two client reconnects passed under the
  original Yama scope 1. Add a private boot/AT_RANDOM witness to refuse same-file
  exec at snapshot boundaries. Refuse failed worker creation without unwinding
  the owner or reaping during an active read; bounded socket connections avoid
  indefinite client hangs. Wait for the configured executable before publishing
  a child identity, fixing a pre-exec race caught by hosted Linux CI. Campaign
  commands remain unavailable.
- Add opt-in owned Linux clock control, an exclusive expiring controller lease,
  explicit Xauthority and display-peer verification, and fresh typed state
  readback. Refuse queued input while busy; synchronize shutdown with key pairs.
  Regression checks cover automatic pause, changing speed, stale no-op receipts,
  expired authority and malformed final pause bytes. Same-campaign load generation
  and full campaign acceptance remain unproved.
- Pass a recorded Luna medium Linux Rust clock test through 3 January 1936,
  including desired speed, unpause, server wait and verified pause. No screenshot,
  cursor movement or kernel-policy change occurred. Publish per-operation lease
  schemas after a rejected release argument; remove nested clock snapshot brackets
  while retaining structural and final ownership/exec checks.
- Port a bounded loaded technology-ID catalog with filtering/pagination and
  complete list/definition readbacks. Add own GUI pointer/dimension/scale
  diagnostics with unverified click mapping explicit. Neither invokes eligibility,
  exposes raw addresses or sends input; the installed catalog and GUI scale pass
  live observations without proving input conversion.
- Simplify README and preserve the existing Windows reference in a separate doc.
  Strategic commands, governor and Windows/macOS native adapters remain unfinished.

The Python release version and downstream release pins remain unchanged.

## 1.1.0 - 2026-10-11

Speed and long campaigns. 43 tools (new: `hoi4_clock`, `hoi4_standing`, `hoi4_peace`).

- Fast clock: `hoi4_advance` follows the telemetry's day records instead of pausing every two days and reading the date
  by OCR (a game day in 0.7 s instead of 3.3 s); `background=true` keeps the game running while the agent acts and
  `hoi4_clock` joins or stops the run. The clock stops by itself for the peace conference and answers notices.
- Standing orders (`hoi4_standing`): research, focus, construction and production queues the clock executes when their
  situation comes up (`research_idle`, `focus_done`, `focus_idle`, `construction_done`, `production_idle` ...).
- Telemetry schema 3: known and researched technologies, per-state factory levels and free building slots.
- Macros: `hoi4_construction plan`, `hoi4_production fill`, `hoi4_orders attack` and `batch`, `hoi4_peace`,
  `hoi4_diplomacy` `states` and `via`, `hoi4_focus` `if_idle` and `search`.
- Window guard before every campaign action; the clock state is restored after an action that left the game running.
- `hoi4_lock`: `record` (one JSON line per tool call with a time breakdown), `set_pace`, `set_images`.
- `hoi4_orders front/offensive/spearhead` leave an executing army alone unless `replace=true` (F1);
  `hoi4_status.game_mode` and refusal of console tools and manual saves in Ironman and multiplayer games (F2).
- Fixes: focus names with script placeholders, requirement groups and icons, the disabled Start button, NSB armour
  tiles, split panel titles, new army panels, the division designer's battalion lists, Find view OCR slips.
- Scripts: `scripts/bridge.py` (a persistent server for shell agents), `scripts/record_screen.py` (segmented screen
  recording with the GPU encoder).

## 1.0.0 - 2026-10-10

First release. 40 tools, live-verified on Hearts of Iron IV 1.19.2 with a large mod.

- Session and screen: launch, load, save and quit through the game's own screens; screenshots, OCR, tooltips,
  popups, input inside the game window under a shared desktop lock.
- Test mode: console, `eval_effect`, events, error and game log deltas, plain-text save queries and diffs,
  scenario reports, optional telemetry companion mod.
  Error-log lines the Paradox wiki explains (event timers, missing namespaces, duplicate event ids) carry
  `known_mistake` with the documented fix.
- Understanding: one state model of a country (telemetry, newest save, top bar) and a one-page briefing.
- Campaign mode: focus, research, production, construction, laws and advisors, decisions, diplomacy and trade,
  armies, battle plans, fleets and air wings, all verified on screen after acting.
- Campaign loop: `hoi4_advance` (run time until something needs a decision, answer events by policy) and
  `hoi4_journal`.
- Installer (`install.py`): private Python environment, launcher that needs no hard-coded Python, per-user
  registration for Claude Code, Codex, Cursor and OpenCode, upgrade in place, clean uninstall.
- Release archive builder and checklist (`scripts/build_release.py`, `docs/RELEASING.md`).

Known limits: see `docs/STATUS.md` (the phase 4 exit check was not repeated end to end on the final code; the
two-year campaign runs of the plan were not made).
