# Rules and modes

Profile revision: `campaign-house-2026-10-11`. This is a reproducible house
profile informed by multiplayer rules, not an official universal MP standard.
The [source ledger](sources.md) separates old examples from current evidence.
Real multiplayer must additionally follow that host's rules; this skill does
not authorize cheating against other players or overriding their agreement.

## FAIR default

Play to win decisively. Encirclements, overruns, pinning, retreating to rivers,
concentrated tanks, breakthrough exploitation, air superiority, CAS, logistics
strikes, naval blockades, convoy raiding and well-supported invasions are FAIR.
So are efficient construction, bonuses used as intended, sensible ahead-of-time
research, designer/MIO optimization, ordinary trade, collaboration/intelligence
operations available in-game, and lawful focus/diplomatic expansion. Winning
quickly or causing high enemy casualties does not by itself make a tactic unfair.

The following are house bans, including cases where a normal UI permits them:

| Area | FAIR policy | Ordinary alternative |
|---|---|---|
| Bugs and caps | No equipment duplication, free-cost/research-bonus loops, special-forces-cap bypass, template-conversion cap/XP/mastery laundering, or stale-stat exploit | Train, design and research within displayed costs and limits |
| Division armor | No line tank/SPG/TD/SPAA mixed with leg infantry or cavalry solely to transfer armor: the broad space-marine composition is excluded | Separate foot defenders and tank + motorized/mechanized assault formations; normal support companies remain allowed |
| Diplomacy | No military-access order-66 betrayal, fake wars to manipulate guarantees/tension, faction hopping to bypass peace/war constraints, or contrived civil wars to strip the opponent's army | Focus claims, justified wars, normal alliances and negotiations |
| Economy | No unneeded-resource trades to transfer factories, lend-lease/market duplication or cancellation loops, puppet-release spam for artificial factories/manpower/score | Buy useful resources/equipment and administer a coherent postwar settlement |
| Combat farming | No deliberately endless wars, perpetual attack-cancel farming or empty-template conversions to manufacture XP/traits/mastery | Train within resources and gain experience in purposeful combat |
| Defeat avoidance | No deleting or converting trapped units to rescue their resources through a loophole | Withdraw before encirclement, relieve the pocket, or accept the loss |
| AI manipulation | No repeated empty-port/invasion cycling solely to reset AI behavior, no surrender/VP drop cheese without a supportable operational objective | Feints, flanks and supported airborne operations are allowed |
| Equipment exploitation | No deliberate equipment-disable/stat loopholes, invulnerability, or known pathological damage/visibility stacking | Optimize actual cost, supply, reliability, attack and survivability |

This profile does not ban all min-maxing, mass submarines, strong armor, forts,
paratroopers or collaboration operations merely because some hosts do. Use them
within intended mechanics and an operational plan. Do not import old limits
such as a fixed combat width, fort cap, aircraft cap or specific war date from a
2016 rulebook and claim they are current balance law.

For the historical FAIR showcases, avoid prewar opportunistic world conquest.
Germany uses its historical diplomatic expansion and begins the general
European war no earlier than July 1939; focus on Poland before the Low Countries
and France. China may pursue national unification through its ordinary content
and defend against Japan; no rule requires China to lose or accept defeat.
There is no forced historical defeat, no mandatory delay of an otherwise won
war, and no automatic rule that delays Sea Lion or Soviet operations to a date.

When a novel tactic has uncertain classification, choose the strong ordinary
alternative and journal the ambiguity. This does not require a pause or a user
question. Do not invent a generic detector for intent, OP strength or fairness.

## META opt-in

META lifts the FAIR house bans on normal-UI exploits and unusually powerful
metas for the authorized single-player campaign. The common campaign boundary
in SKILL.md remains absolute. An exploit requiring console, script invocation,
file manipulation, a memory write or privileged command posting is unavailable.

Use the following ladder, stopping once the strategic objective is covered:

1. Strong intended mechanics: snowballing focus paths and cores, efficient
   designs, concentrated force, aggressive timings and favorable diplomacy.
2. Normal-UI cheese: armor-supported infantry, opportunistic early wars,
   airborne victory-point/port seizure, puppet equipment/manpower mechanics,
   and deliberate AI baiting when legal in the installed UI.
3. Reproducible UI-only exploits: only after identifying the exact build/DLC,
   visible prerequisites, costs, steps and postcondition. Record the exploit
   actually used and its effect. A name from an old guide is not a recipe.

Never promise a current working bug from historical anecdotes. One bounded
attempt may validate a candidate during a real META run if its cost fits the
declared plan; accept the consequences and continue if it fails. Do not restart,
roll back, or repeat blind clicks. Prefer a separate non-showcase practice run
for discovery; practice still cannot falsify campaign evidence.

Track META techniques as `candidate`, `verified on build`, `failed on build`,
or `unavailable`. A candidate record contains source date, build/DLC, normal-UI
steps, expected visible result, maximum cost/attempts, fallback and run outcome.
Do not label an ordinary useful tactic an exploit merely for publicity.

## Policy enforcement versus evidence

Machines may validate the requested mode, telemetry-only mod list, selected
country, explicit forbidden tool surfaces, verified IDs, user-visible action
type, resource bounds and state readback when those fields exist. They cannot
infer all strategic intent from a button click, prove no exploit exists, or
verify the entire rules profile from a `fair=true` field. Unsupported checks
must be reported as unknown. Preserve mode and profile in the run manifest and
review action/journal evidence before calling a showcase FAIR.
