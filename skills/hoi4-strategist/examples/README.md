# Reference outputs

Hand-written plans in the playbook show the expected shape and depth of the strategist's output. They
are good but unproven (no run with them has won yet), so use them for structure and coverage, not as
facts: every fact in a generated plan still comes from the dossier or the facts CLI.

| Run spec (goal) | plan.md | plan.json |
|---|---|---|
| ITA, 1936.1.1, fair, "Conquer Ethiopia quickly" | [italy-ethiopia-1936.md](../../hoi4-campaign-playbook/plans/italy-ethiopia-1936.md) | [italy-ethiopia-1936.plan.json](../../hoi4-campaign-playbook/plans/italy-ethiopia-1936.plan.json) |
| ETH, 1936.1.1, fair, "Push Italy out of Africa" (East Africa reading) | [ethiopia-italy-1936.md](../../hoi4-campaign-playbook/plans/ethiopia-italy-1936.md) | [ethiopia-italy-1936.plan.json](../../hoi4-campaign-playbook/plans/ethiopia-italy-1936.plan.json) |

The scenario mechanics both plans cite are in
[italo-ethiopian-war.md](../../hoi4-campaign-playbook/references/italo-ethiopian-war.md); the dossier
builds the same kind of facts automatically.

What to compare (offline only: `python3 scripts/strategist/eval_plan.py GEN REF`): win predicates and
deadline, the first focuses, the first research block, VP order and threshold, construction triggers,
production split, event choices by option name, army guards and strike size, and the executor-owned
share. Divergences are not failures in themselves; each goes to a human diff review.

The older Germany, Nationalist China and Poland 1936-1938 plans in the same folder are weaker
references.
