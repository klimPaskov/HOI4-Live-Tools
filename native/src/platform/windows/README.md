# Dormant Windows read-only foundation

This module implements neither `platform::Process` nor `memory::ReadMemory`.
It registers no server, CLI, tool, input, worker, layout or string ABI. The existing
Windows server attachment still reports `platform_unavailable`.

`OwnedReader::open(api, session)` accepts typed, exact caller-supplied launch
expectations: unrounded creation FILETIME, complete binary SID, executable path,
volume plus 128-bit file ID, and a distinct original supervisor. It checks both
live processes, ordinary-user ownership, the direct parent relationship, module
identity and retained file handles. This is not a launcher or proof that an
arbitrary caller-created session came from an authorized launch.

The production build allowlist is empty. Opening a production reader ends with
`unsupported_build` before requesting VM-read rights or reading loaded headers or
game payload. The generated PE/SHA profile branch exists only under `cfg(test)`;
no production argument, environment variable or manifest can enable it. A future
Windows game adapter still needs explicit build profiles, MSVC RTTI/string
handling and independently reviewed Windows layouts.

The injected `Api` separates query/read operations from ownership policy. Native
process handles request only QUERY_INFORMATION, SYNCHRONIZE and, after a test-only
profile is accepted, VM_READ. No write, injection, termination, debug privilege,
process launch or input interface is present in the native adapter. Process,
token, snapshot and executable handles use RAII. Executable files are opened for
read with FILE_SHARE_READ and kept open; no write/delete sharing is requested.

Reads validate every range before API access. A batch performs exactly one full
ownership/mapping check before and after its transfers; cheap retained-handle
PID/creation/liveness checks run around each transfer, together with an optional
original caller cancellation/deadline checkpoint. Scalars use a one-item batch.
Transfers require exact byte counts and reject guard/uncommitted/unreadable or
cross-region spans. Changed region metadata discards the result. Lifetime/read
failures are sticky. Batches return all results in order or an error; they expose
no partial result. These checks do not make mutable game fields atomic: future
callers must retain their own complete structural witnesses and deadline.

Conservative observer policies: native x64 Windows only; local drive-letter
paths with exact API spelling (no UNC/device/path normalization fallback),
non-reparse regular executable, unelevated caller and processes, nonzero file
IDs, at most 512 MiB executable, 64 KiB PE headers, 96 ordered sections,
1 GiB virtual image, single readable region per read, 1 MiB per transfer,
32 MiB / 65,536 requests per batch, 4,096 modules and 65,536 process metadata
entries. Full process/module censuses are O(1) per batch, not per field. Per-scalar
calls still pay the full bracket and no Windows timing is qualified; a future
snapshot adapter must use real batches and supply its original deadline. These are supported-subset limits, not Windows or engine maxima.
`IsWow64Process2` is resolved by its fixed name from resident kernel32; missing
support refuses rather than adding a new mandatory loader import.

Portable injected tests cover ownership/races, cleanup, PE/SHA rejection,
bounds, guard pages, partial reads and sticky failures. On Windows, a normal
parent test launches only its own test executable and an explicitly selected
ignored child endpoint, approving only that exact synthetic PE/SHA under
`cfg(test)`. The child publishes a known test buffer and exits after stdin EOF;
parent cleanup owns only that child. An elevated Windows runner exercises the
pre-open elevation refusal and explicitly leaves positive transfer pending.
None of these tests has been executed on Windows during Mac development. They
provide no HOI4 build, input, campaign or native Windows gameplay evidence.

Port provenance: existing project `hoi4_live/memory/winproc.py`, `pe.py`,
`reader.py`, `layout.py`, and `tests/test_memory_reader.py`. The Rust implementation
is new; it does not copy Linux/Mac offsets or use the legacy timestamp/filesize
build key as authorization. Win32 contracts were checked against Microsoft
Learn: process access rights, GetProcessTimes, GetTokenInformation, FILE_ID_INFO,
Toolhelp process/module structures, VirtualQueryEx/MEMORY_BASIC_INFORMATION,
ReadProcessMemory, PE Format and optional GetProcAddress. Full source URLs and
verification records are retained with the private review package.
