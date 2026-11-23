"""Run observation: the state recorder, the situation report and the history.

A *snapshot* is one normalised JSON document built from the server's pure memory
reads (tools that take no input lease and open no panel) plus the executor's own
files in the run directory. The recorder writes one per game week and at every
war declaration, capitulation and operation phase change; the sitrep and the
history read them back. Nothing here sends input, opens a view or writes memory.

Modules:
  reads     which read tools the recorder calls (and the gaps it cannot read)
  snapshot  raw read bodies -> one normalised snapshot
  findings  what is wrong in a snapshot (value judgements with their reasons)
  sitrep    the plain-text situation report and the diff between two snapshots
  history   one quantity over time across the snapshots of a run
  recorder  the loop that decides when to snapshot and writes the files
"""
