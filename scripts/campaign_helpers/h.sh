#!/bin/bash
# usage: h.sh tool '{json}'
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
python scripts/bridge.py call "$@" 2>&1
