# Upstream foundation

This directory pins the Clausewitz-MCP runtime source at the commit recorded in
`UPSTREAM.json`. Each imported file is unchanged and has a recorded SHA-256.
`LICENSE` retains the upstream MIT copyright and permission notice.

The source is the foundation for the ongoing engine migration, not an additional
MCP server automatically started by HOI4 Live Tools. Do not import the upstream
entry point into campaign dispatch: it includes debug tools, unvalidated native
calls, build-specific Linux offsets and observations outside campaign visibility.
Binary caches and generated game tables are omitted. They must be regenerated
and validated for the installed build rather than trusted across platforms.

Active ported components live in `hoi4_live/engine/clausewitz/`. The first is the
typed memory API shared by all three platform readers. Platform adapters retain
process ownership, exact reads and explicit capability errors. The next stages
are the structured game model, verified ordinary command adapter, and the
upstream event/army loop under durable plans and the FAIR/META policy. Until
those stages pass live checks, telemetry/UI remains the working fallback.
