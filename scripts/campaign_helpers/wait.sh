#!/bin/bash
# join the background clock until it finishes; print the summary
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
for i in $(seq 1 40); do
  out=$(python artifacts/dev/q.py hoi4_clock '{"action":"join"}' 2>&1)
  if ! echo "$out" | head -1 | grep -q "still running"; then echo "$out" | head -${1:-30}; exit 0; fi
done
echo "$out" | head -3
