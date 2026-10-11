#!/bin/bash
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
python scripts/bridge.py stop >/dev/null 2>&1
sleep 2
(python scripts/bridge.py serve --port 8765 --call-log "${1:-artifacts/dev/calls_dev.jsonl}" --image-dir "${2:-artifacts/dev/images}" > artifacts/dev/bridge.log 2>&1 &)
sleep 4
cat artifacts/dev/bridge.log
python scripts/bridge.py call hoi4_lock '{"action":"acquire","purpose":"hoi4 campaign run + tool dev","wait_s":30}' | head -1; python scripts/bridge.py call hoi4_lock "{\"action\":\"set_mode\",\"mode\":\"campaign\"}" | head -1; python scripts/bridge.py call hoi4_lock "{\"action\":\"set_pace\",\"pace\":\"fast\"}" | head -0; python scripts/bridge.py call hoi4_lock "{\"action\":\"set_images\",\"images\":\"errors\"}" | head -0
