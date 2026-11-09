# Rules and modes

Profile revision: `campaign-house-2026-10-11`. This is a reproducible house
profile informed by multiplayer rules, not an official universal MP standard.
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

## META-only, banned and patched tricks

Listed so a plan, the lint and the player can refuse them. BAN: forbidden in FAIR (META may lift a house ban, never
the campaign boundary). META: only in a META run. Journal it: ordinary UI but uncertain; FAIR runs take the plain
alternative and record the choice.

| ID | Trick | What it is | Mode |
|---|---|---|---|
| MX01 | Hollow-design conversion | Produce a near-empty design to grow a line or skip resource costs, then convert to the real design (conversion has no minimum resource cost) | META |
| MX02 | Role-swap copy | Save a variant, switch the line, then make an identical copy with only the role changed (tank to TD or SPG) to dodge the switch loss | META |
| MX03 | Space marines | One line tank, SPG, TD or SPAA battalion in foot infantry or cavalry to lift armour above enemy piercing | BAN in FAIR, META allowed |
| MX04 | XP, mastery and cap laundering | Empty or zero-equipment templates converted for XP, mastery or special-forces cap; 2-wide "guns off" mastery grinders | BAN |
| MX05 | Deleting trapped units | Disbanding encircled divisions to recover 20% of their manpower | BAN in FAIR, META allowed |
| MX06 | Trait and XP farming | Halting and re-attacking long battles to restart trait progress; endless attack-cancel loops | BAN |
| MX07 | Fake wars and tension games | Justifications or faction churn to spike tension; baiting or working guarantees; declaring on several targets at once only to dodge a guarantor; justifying on minors just before a major falls | BAN |
| MX08 | Civ boosting | Unneeded-resource trades that pass civilian factories to an ally with better modifiers | BAN |
| MX09 | Puppet games | Release-and-annex spam for factories, manpower or score; lend-lease spam to crash a puppet's autonomy | BAN |
| MX10 | Invasion or empty-port cycling | Repeated landings or port grabs to reset AI behaviour | BAN |
| MX11 | Lend-lease and market loops | Cancel loops; fake template shortages to bait AI lend-lease | BAN |
| MX12 | Capital snipes | Landing or paradropping on the capital VP to force a capitulation without a supportable operation | META |
| MX13 | Planning without a front | Building planning on a garrison order, with unassigned divisions, or on a land front and then switching the army to a naval invasion | META |
| MX14 | Volunteer manpower laundering | Volunteers used to park or recover manpower | META |
| MX15 | Equipment-disable loopholes | Switching off support or regimental equipment to keep the bonus; training without the expensive equipment and filling it later | BAN in FAIR |
| MX16 | Cheap-ability armies | A one-battalion army to buy Force Attack or Last Stand cheaply, then stacking the attack on it | META |
| MX17 | AA reserve division | A line-AA division kept in reserve so it never loses its AA | META |
| MX18 | Reserve re-order | Cancelling and re-issuing orders so every waiting reserve joins at once: once to commit reserves is ordinary, repeated to farm is META | Journal it |
| MX19 | Allied spy-network stacking | Several allies' networks on one state to strip entrenchment and planning | FAIR in single-player |
| MX20 | MEFO cancelled on day 1 | Germany cancels MEFO right after price controls to keep the buffs without the consumer-goods growth | META |
| MX21 | Mission-gate spam | One-battalion divisions (Anschluss cavalry, Romanian focus gates) only to pass an army-size gate | META |
| MX22 | Contrived own civil war | Pre-splitting the army before a scripted civil war | META |
| MX23 | Faction-goal reveal | Dropping below a goal's threshold to make it selectable, then exceeding it again | META |
| MX24 | One-submarine task forces | Single-sub task forces to stay undetected, drain enemy fuel or act as decoys | META |
| MX25 | Dominance-only hulls | Empty carriers or battlecruisers built only for dominance per IC | META |
| MX26 | Flicker bombing | Switching bomber regions whenever fighters appear | META |
| MX27 | Neutral-territory invasion | Invading through a neutral's military access | META |
| MX28 | Merchant-raider decision | Germany's merchant-raider refit if its cost is not charged | META candidate |
| MX29 | Dominance tick massing | Massing at the daily recalculation to flip dominance | META candidate |
| MX30 | Holding while the enemy repairs | Never repairing while the enemy fleet sits in port indefinitely | Journal it |
| MX31 | Hour-zero invasion | Patrols and an invasion prepared in peace, launched the hour the war starts | Journal it |
| MX32 | Submarines pinning fleets | Subs on always-engage patrol only to hold enemy fleets | Journal it |
| MX33 | Stockpile flight before capitulation | Lend-leasing the whole stockpile away just before capitulating | Journal it; FAIR showcases skip it |
| MX34 | Cancelled relations for mastery | Cancelling improve-relations right after starting it | META |
| MX35 | Early war to end the Depression | USA provokes an early war to drop the Great Depression | META |
| MX36 | Doctrine stacking by expeditionaries | Expeditionary forces used to stack two doctrines | META |
| MX37 | No-garrison bleeding | Leaving non-core land ungarrisoned to wreck an invader's equipment | META |
| MX38 | Release to skip a gate | Poland releases Lithuania to bypass a compliance requirement | META |
| MX39 | Save copying | Copying ironman saves before a risky declaration | Unavailable (file manipulation) |
| MX40 | Patched tricks | Faction leave-and-rejoin manpower, army-leader cost stacking, design cost below -100%, task forces in two places, HQ disband trickleback, Distribute Taskforces force-retreat, halted-wing range, the Force Attack exploit, strike-force disengage spam, tech dragging for bonuses, peacetime mines, the Hungarian capitulation path, licensed ship modules | Patched or blocked: never plan on them |
