# Rust foundation provenance

This port derives typed process reads, the own-country tag-index chain, focus and
research object semantics, and the 365-day calendar from smerss/Clausewitz-MCP
commit `97c85437081904d5ad07bae1623ec58d1c0a5b9e`, specifically
`calling/{mem,process/linux,api,game}.py`. Copyright (c) 2026 smerss.
The original MIT license is retained in `LICENSE.clausewitz`.

Rust modules `memory.rs`, `game.rs`, `calendar.rs` and `platform/linux.rs` carry
source comments identifying this derivation. Other modules are new project code.
Current Linux ELF layouts were recovered independently from the installed build;
upstream 1.19.2 offsets were not copied as current-build evidence.

This first port exposes observations only. Native command execution remains in
the disclosed private upstream validation adapter until its transaction and
recovery semantics have been ported and tested. No raw process-write, injection,
console or foreign-country interface is exposed by this Rust MCP.
