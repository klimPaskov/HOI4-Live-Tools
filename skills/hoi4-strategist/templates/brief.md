# Brief: <Country> — <goal> (plan v<N>)

<On a revision, first line: "Revised v<N>: <one-line summary of what changed>".>

**Win.** <predicates in one line, with ids>, before <deadline>.
Check: `<exact read call, e.g. hoi4_states {"query":"<ENEMY>"}>` — <what the answer must show>.

**Next checkpoints.**
1. <date>: <must be true> — if not: <action>.
2. <date>: <must be true> — if not: <action>.
3. <date>: <must be true> — if not: <action>.

**The executor already does** (do not repeat): research, focus, construction, production, laws in the
plan, decisions in the plan, armies and layout, war moves on `vp_targets`, guards, doctrines, events in
`event_options`. <add or remove per plan>

**Your items** (one batch per turn; exact calls):
* <when>: `<tool> <args JSON>` — <why, one clause>.
* <when>: `<tool> <args JSON>` — <why>.

**Events that need judgement.**
* `<event id>`: pick `<option_name>` if `<predicate>`, else `<option_name>` — <why>.

**Do not.**
* <rule>.
* Never pause the game. Never reassign a general who already has an army.

**On `plan_revised`.** Re-read this brief once (`RUN/plan.brief.md`); drop items the new brief no longer
lists; keep playing at speed 3.

**If stuck.** `hoi4_plan {"op":"request_revision","reason":"<one line>"}` and continue the current plan.
