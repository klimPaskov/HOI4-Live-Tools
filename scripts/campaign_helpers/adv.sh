#!/bin/bash
# usage: adv.sh DAYS [PP_HIGH] [extra stop_on json list]
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
python - "$@" <<'PY'
import json,sys
days=int(sys.argv[1]); pp=int(sys.argv[2]) if len(sys.argv)>2 else 160
rules=json.load(open("artifacts/dev/popup_rules.json"))
stop=json.loads(sys.argv[3]) if len(sys.argv)>3 else ["war_started","territory_lost","losing","low_stability","pp_high"]
json.dump({"days":days,"background":True,"stop_on":stop,"thresholds":{"pp_high":pp},"popup_rules":rules},open("artifacts/dev/adv_args.json","w"))
PY
python artifacts/dev/q.py hoi4_advance @artifacts/dev/adv_args.json | head -1
artifacts/dev/wait.sh ${WAITLINES:-14}
