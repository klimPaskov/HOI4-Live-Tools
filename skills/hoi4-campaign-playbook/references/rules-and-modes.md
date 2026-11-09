# Rules and modes

Profile revision: `fair-mp-2026-11-09`. Two modes exist. **FAIR** (the default) is the multiplayer ruleset
below plus the house rules. **META** allows exploits and must be chosen by the user or the campaign
assignment. A host's own agreement always wins in a real multiplayer game; this skill never authorizes
cheating against other players.

## Where the rules apply

* **FAIR is the default** for every campaign and for multiplayer games, unless the players agree that META
  exploit play is allowed. Do not switch a losing FAIR run to META.
* **House rules and general rules** apply in every FAIR game.
* **Axis, Allies and nation rules** apply whenever the campaign is in the historical WW2 setting with those
  nations (a historical bookmark, the historical tags). A rule for a side binds every nation on that side;
  a nation rule binds that nation.
* **Single player**: the agent's own nation follows the general rules, its side's rules and its own nation's
  rules. AI nations are not bound; never break a rule in reply to an AI that did.
* **Exceptions**: the rules name a few ("exceptions apply" for volunteers, host discretion for doubtful
  cases). A host-approved exception goes in the plan as `_fair.exceptions` {"fair.<rule>": "<reason>"} and
  in the journal; without one, take the plain alternative.
* **Checks**: `lint_plan.py` runs the `fair.*` rules (`scripts/hoi4facts/lint_fair.py`) on every plan whose
  mode is fair (`_validator.mode`, `_meta.mode` or `run_spec.mode`; a plan without a mode is fair). The executor refuses the rules that only
  the live game shows (the guard list at the top of `lint_fair.py`): in a FAIR run every action, the
  player's direct calls included, passes `native/src/fair_guard.rs`, which refuses a breach before any input
  with the error `fair_rule_refused` naming the rule (`fair.naval_mines: ...`); `_fair.exceptions` lets that
  rule through and META skips the guards. A rule no tool can check is still a
  rule: record the decision and the evidence.

## House rules (FAIR; META keeps them too)

* No console, debug, eval or event commands, no save editing, no memory writes or command injection.
* No hidden enemy intelligence: only what the player's own screen and ordinary in-game intelligence show.
* No reload to undo an outcome; a checkpoint may only resume a real interruption.
* Real games never pause: the agent never pauses itself once the campaign runs.

## General rules (every FAIR game)

**Conduct**
* No exploits and no skirting the rules. The host decides doubtful cases.
* Never switch the played country, even for a moment.

**Diplomacy**
* No military access, docking rights or airbase access, asked or offered. Only access for civil-war
  supplies is allowed (and the side exceptions below).
* No manual war-goal justification before the Danzig crisis (Germany's war on Poland), except the listed
  exceptions.
* No disbanding factions.

**Army**
* Deployed divisions at war are at least 10 combat width, counted before doctrine modifiers. Starting
  divisions are exempt; new and reorganised ones are not.
* No space marines: no infantry or cavalry battalions with tank battalions (line tanks, SPGs, tank
  destroyers, SPAA) in one division. Tank support companies are fine.
* Paratrooper divisions are at least 20 width; a nation has at most 18 paratrooper divisions.
* No intentional bypass of the special forces division limit (for example converting fielded divisions
  into special forces).
* No ghost divisions without equipment.
* No deleting, editing or recalling divisions that are encircled or about to be.
* No tanks on ports unless a battle is on or next to the port.
* No grinding mastery with divisions whose main equipment type is switched off.

**Navy**
* No naval blockade in peacetime.
* No naval mining.
* Submarines: no always engage, at least 5 per submarine fleet, no special submarine modules (AIP,
  coating), and nothing above Submarine II (no research, design or production of later hulls).

**Air**
* Strategic bombing may not target refineries specifically.
* No flicker bombing.
* Logistics strikes are allowed.
* No changing the type or role of licensed aircraft; no way around licence penalties.

**Economy**
* No deleting stockpiles, factories or research facilities. Dockyards may go only when the state is out of
  slots.

## Axis rules

* No collaboration governments in the Soviet Union.
* Military and docking access between the European Axis and Japan only once the Iran corridor is secured.
* Sea Lion (invading Great Britain): the European Axis first holds 2 of Gibraltar, Malta and Suez; 1 is
  enough once Barbarossa has started.
* Win condition: the Soviet Union capitulated and no Allied foothold in Europe.
* Japan and its co-prosperity sphere never join the Axis; no European Axis nation joins the co-prosperity
  sphere.

**Germany**
* Barbarossa from 1941, on the focus war goal (no manual justification), in 1942 at the latest.

**Italy and the Balkan Axis**
* Italy joins the war by 1 January 1941 at the latest.
* The Italian surface fleet is docked in core Italian ports when Italy joins the war.
* May justify on and attack the Balkans before the world war.
* Bulgaria takes no land in northern Europe unless it has a collaboration government prepared.

**Spain**
* Joins the Axis only after France falls.
* Gives the Axis no military access unless it joins the war.

**Finland**
* Does not join the Axis during the Winter War.

**Japan, Manchukuo and Siam**
* War with the Allies from 1941.
* Attack the Soviet Union only after securing all of mainland Asia, and only through the focus.
* Take Papua New Guinea before invading Australia or New Zealand.
* Take Hawaii and the Philippines before invading mainland North America (Alaska is the exception).

## Allies rules

* No Allied troops from outside mainland Europe in mainland Europe until France capitulates.
* No volunteers outside Spain (exceptions below).
* Take Iwo Jima, Taiwan and Okinawa before attacking mainland Japan.
* Secure North Africa before invading the core Italian mainland. Islands are fine; attacks over land are
  exempt.
* Military access with the Soviet Union only once all of mainland Asia is secured.
* The Soviet Union receives at most 5,000 lend-lease planes from the Allies.

**France**
* No building or upgrading ports or air bases in France before capitulation.
* All troops stay in mainland Europe before capitulation.
* No lend-lease before capitulation; Republican Spain is the exception.

**Soviet Union**
* Attacks the Axis from 1942, or earlier once the Axis has invaded the mainland USA or UK.
* May justify on Iran by hand after the Molotov-Ribbentrop Pact.
* Winter War: no naval invasions; accept the historical peace deal (reparations are allowed if Finland
  capitulates).
* Volunteers to China: at most 6 divisions, no tanks and no air.
* Only communist nations in the Comintern.

**USA**
* Joins the Allies only when Japan attacks it, from 1942, when the UK is invaded, when any land in North
  or South America is invaded, or when the Soviet Union loses 2 of its 3 major cities (Moscow, Leningrad,
  Stalingrad).
* No civilian-factory boosting of the UK or France before Danzig (an embargo enforces it in game).
* May send volunteers to Spain, China (at most 6 divisions and 1 air wing), the Allies and the Soviet Union.

**United Kingdom**
* Accepts Bulgarian rearmament.

**Netherlands and Belgium**
* Join the Allies only when Germany declares war on them.

## Strong ordinary play stays FAIR

Play to win decisively. Encirclements, overruns, pinning, retreating to rivers, concentrated tanks,
breakthrough exploitation, air superiority, CAS, logistics strikes, wartime convoy raiding and blockades,
and well-supported invasions are FAIR. So are efficient construction, bonuses used as intended, sensible
ahead-of-time research, designer and MIO optimisation, ordinary trade, intelligence operations the rules
allow, and focus and diplomatic expansion within the rules. Winning quickly or causing high enemy
casualties does not by itself make a tactic unfair.

House bans beyond the ruleset, including cases where the normal UI permits them:

| Area | FAIR policy | Ordinary alternative |
|---|---|---|
| Bugs and caps | No equipment duplication, free-cost or research-bonus loops, special-forces-cap bypass, template-conversion cap/XP/mastery laundering, or stale-stat exploits | Train, design and research within displayed costs and limits |
| Division armour | Space marines are banned (general rules) | Separate foot defenders and tank plus motorised or mechanised assault divisions; normal support companies stay allowed |
| Diplomacy | No access of any kind (general rules), no fake wars to manipulate guarantees or tension, no faction hopping to bypass peace or war constraints, no contrived civil wars to strip an opponent's army | Focus claims, justifications after Danzig, normal alliances and negotiations |
| Economy | No unneeded-resource trades to transfer factories (civ boosting), lend-lease or market duplication or cancellation loops, puppet-release spam for factories, manpower or score | Buy useful resources and equipment; run a coherent postwar settlement |
| Combat farming | No deliberately endless wars, perpetual attack-cancel farming or empty-template conversions for XP, traits or mastery | Train within resources and gain experience in purposeful combat |
| Defeat avoidance | No deleting, editing or recalling trapped units | Withdraw before encirclement, relieve the pocket, or accept the loss |
| AI manipulation | No repeated empty-port or invasion cycling to reset AI behaviour, no surrender or VP-drop cheese without a supportable operational objective | Feints, flanks and supported airborne operations are allowed |
| Equipment exploitation | No equipment-disable or stat loopholes, invulnerability, or known pathological damage or visibility stacking | Optimise actual cost, supply, reliability, attack and survivability |

For the historical FAIR showcases, avoid prewar opportunistic world conquest. Germany uses its historical
diplomatic expansion and begins the general European war no earlier than July 1939; Poland before the Low
Countries and France. China may pursue national unification through its ordinary content and defend
against Japan; no rule requires China to lose. There is no forced historical defeat and no delay of an
otherwise won war beyond the dated rules above (Barbarossa from 1941, Sea Lion after its prerequisites,
Japan against the Allies from 1941, the Soviet Union against the Axis from 1942).

When a novel tactic has an uncertain classification, take the strong ordinary alternative and journal the
ambiguity. This needs no pause and no user question.

## META opt-in

META lifts the FAIR general, side and nation rules and the house bans on normal-UI exploits for the
authorized campaign. The house rules (console, saves, hidden intelligence, reloads, pausing) stay absolute:
an exploit that needs console, script invocation, file manipulation, a memory write or privileged command
posting is unavailable. Everything the FAIR rules ban is META-only. The exploit playbook is
[exploits.md](exploits.md): META-only, never used in a FAIR run.

Use the following ladder, stopping once the strategic objective is covered:

1. Strong intended mechanics: snowballing focus paths and cores, efficient designs, concentrated force,
   aggressive timings and favourable diplomacy.
2. Normal-UI cheese: armour-supported infantry, opportunistic early wars, airborne victory-point or port
   seizure, puppet equipment and manpower mechanics, and deliberate AI baiting when legal in the installed UI.
3. Reproducible UI-only exploits: only after identifying the exact build and DLC, visible prerequisites,
   costs, steps and postcondition. Record the exploit actually used and its effect. A name from an old guide
   is not a recipe.

Never promise a current working bug from historical anecdotes. One bounded attempt may validate a candidate
during a real META run if its cost fits the declared plan; accept the consequences and continue if it fails.
Do not restart, roll back, or repeat blind clicks. Prefer a separate non-showcase practice run for discovery;
practice still cannot falsify campaign evidence.

Track META techniques as `candidate`, `verified on build`, `failed on build`, or `unavailable`. A candidate
record contains source date, build and DLC, normal-UI steps, expected visible result, maximum cost and
attempts, fallback and run outcome. Do not label an ordinary useful tactic an exploit merely for publicity.

## Policy enforcement versus evidence

Machines validate what they can read: the mode, the mod list, the country, the plan against the `fair.*`
lint, verified IDs, the action type at the executor guard, resource bounds and state readback. They cannot
infer all strategic intent from a button click or prove no exploit exists. Unsupported checks are reported
as unknown. Preserve the mode and profile revision in the run manifest and review action and journal
evidence before calling a showcase FAIR.

Where another playbook file marks a tactic FAIR that these rules ban (for example always-engage
submarines, improved or advanced submarine hulls, anechoic tiles, naval mines, military access), the rules
here win and the tactic is META-only.

## META-only, banned and patched tricks

Listed so a plan, the lint and the player can refuse them. BAN: forbidden in FAIR (META may lift a house
ban, never the house rules). META: only in a META run. Journal it: ordinary UI but uncertain; FAIR runs
take the plain alternative and record the choice. Every trick the FAIR rules above ban is META-only even
when it has no row here.

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
| MX16 | Cheap-ability armies | A one-battalion army to buy Force Attack or Last Stand cheaply, then stacking the attack on it; since 1.19.0 abilities cost per battalion and apply only to the general's own army, so a small army using its own ability is ordinary play | Patched (1.19.0) |
| MX17 | AA reserve division | A line-AA division kept in reserve so it never loses its AA | META |
| MX18 | Reserve re-order | Cancelling and re-issuing orders so every waiting reserve joins at once: once to commit reserves is ordinary, repeated to farm is META | Journal it |
| MX19 | Allied spy-network stacking | Several allies' networks on one state to strip entrenchment and planning | FAIR in single-player |
| MX20 | MEFO cancelled on day 1 | Germany cancels MEFO right after price controls to keep the buffs without the consumer-goods growth | META |
| MX21 | Mission-gate spam | One-battalion divisions (Anschluss cavalry, Romanian focus gates) only to pass an army-size gate; below 10 width they are also banned at war | META |
| MX22 | Contrived own civil war | Pre-splitting the army before a scripted civil war | META |
| MX23 | Faction-goal reveal | Dropping below a goal's threshold to make it selectable, then exceeding it again | META |
| MX24 | Small submarine task forces | Task forces of 1-4 submarines spread over many zones to stay undetected, drain enemy fuel or act as decoys (FAIR needs 5 per fleet) | META |
| MX25 | Dominance-only hulls | Empty carriers or battlecruisers built only for dominance per IC | META |
| MX26 | Flicker bombing | Switching bomber regions whenever fighters appear | BAN in FAIR, META allowed |
| MX27 | Neutral-territory invasion | Invading through a neutral's military access | META |
| MX28 | Merchant-raider decision | Germany's merchant-raider refit if its cost is not charged | META candidate |
| MX29 | Dominance tick massing | Massing at the daily recalculation to flip dominance | META candidate |
| MX30 | Holding while the enemy repairs | Never repairing while the enemy fleet sits in port indefinitely | Journal it |
| MX31 | Hour-zero invasion | Patrols and an invasion prepared in peace, launched the hour the war starts | Journal it |
| MX32 | Submarines pinning fleets | Subs on always-engage patrol only to hold enemy fleets (always engage is banned for submarines) | BAN in FAIR, META allowed |
| MX33 | Stockpile flight before capitulation | Lend-leasing the whole stockpile away just before capitulating | Journal it; FAIR showcases skip it |
| MX34 | Cancelled relations for mastery | Cancelling improve-relations right after starting it | META |
| MX35 | Early war to end the Depression | USA provokes an early war to drop the Great Depression | META |
| MX36 | Doctrine stacking by expeditionaries | Expeditionary forces used to stack two doctrines | META |
| MX37 | No-garrison bleeding | Leaving non-core land ungarrisoned to wreck an invader's equipment | META |
| MX38 | Release to skip a gate | Poland releases Lithuania to bypass a compliance requirement | META |
| MX39 | Save copying | Copying ironman saves before a risky declaration | Unavailable (file manipulation) |
| MX40 | Patched tricks | Faction leave-and-rejoin manpower, army-leader cost stacking, design cost below -100%, task forces in two places, HQ disband trickleback, Distribute Taskforces force-retreat, halted-wing range, the Force Attack exploit, strike-force disengage spam, tech dragging for bonuses, peacetime mines, the Hungarian capitulation path, licensed ship modules, re-completing faction goals by dismantling and recreating the faction, subjects blockading their overlord | Patched or blocked: never plan on them |
| MX41 | Volunteer-recall duplicate HQ | Recalling volunteers while their general is deployed as an Army HQ leaves a second, permanent HQ unit (1.19.4 bug); withdraw the HQ first | BAN |
| MX42 | State-transfer shelter | Transferring or selling states to a third country to keep them from an invader or to make artificial buffer borders, then taking them back | BAN |
| MX43 | Paradrop retreat trick | Dropping one tile behind the front only to force a retreat glitch, or converting paratroopers to another template right after landing; supported drops on hubs, rail junctions, ports and victory points stay FAIR | BAN |
| MX44 | Parked-mission efficiency | Holding air wings on a mission they cannot fly (wrong region or mission type) to build mission efficiency, then switching to the real mission | META |
