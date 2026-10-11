# Public source audit — 11 October 2026

Publication scope: `master`, `wip/governor-memory` and the annotated `v1.1.0`
baseline tag. The public history is a sanitized root plus the stopped WIP commit.
Original branches and the original stopped-work commit remain local under
`private/*`; none of those refs are pushed. No release archive or live evidence
is uploaded in this publication.

Every reachable commit, tag and blob in the publication scope was inspected by
a local object-level scan for credential patterns, secret assignments, private
mod identifiers, personal absolute paths, unexpected binaries and oversized
objects. File names were reviewed for saves/dumps/credentials/archives. The
vanilla UI screenshot fixtures were visually inspected; the three parser
samples were reviewed as small text snippets, not campaign checkpoints. The
installer test uses a synthetic `C:\example` path, and public commit/tag metadata
uses the account's GitHub noreply address. Final publication requires zero
unresolved scan findings.

Private mod details in older status/plan/integration documents were removed;
the old status report is retained locally. Campaign and memory findings are
summarized without distributing private mod source, raw saves or process dumps.
Large screenshots, research binaries/environments and old release archives were
relocated to an external scratch evidence directory. A log held open by another
process was left alone; it is ignored and not part of the public history.

LICENSE is Apache-2.0. NOTICE credits smerss/Clausewitz-MCP (MIT) for consulted
design/research lessons. No upstream source was copied during publication.

Validation on Windows/Python 3.9: stopped WIP 457 tests, 15 skipped; sanitized
master 328 tests, 15 skipped; WIP compileall and tool listing passed. Skips cover
omitted/private screenshot fixtures. Mac live support, native command posting,
Luna two-year acceptance and successful showcase campaigns remain unfinished.
Read [HANDOFF_MAC.md](HANDOFF_MAC.md) for the continuation.
