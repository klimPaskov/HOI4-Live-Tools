# Nation openers

What strong players do in the first years with each nation, as a starting point for a plan. Dates assume the 1936
start; focuses take 70 days unless noted (35 for Japan and Manchukuo). Read the tag's current tree, laws and
decisions before copying a step: DLCs change trees. FAIR showcases keep historical war timings (Germany: no general
European war before July 1939); steps marked META are allowed only in a META run. Tactic ids (PR08, PO19, ...)
are in [tactics.md](tactics.md).

Every opener starts the same way on day 0, still paused: put free military factories on existing lines (PR01),
cull start lines the plan does not use and settle the start naval queue (PR07, PR08), reset inherited trades to
what the lines need (EC16), queue construction in the best states, pick the first focus and research, and hire
the chief of army among the first three buys (PO07).

Start laws: Germany, Italy, Japan, Poland and Hungary start on Partial Mobilisation; the USSR, UK, France and
Romania on Civilian Economy; the USA on Undisturbed Isolation with the Great Depression and Free Trade. Research
slots at start: Germany, Italy, UK and USA 4; USSR, Japan, France, Poland, Hungary and Romania 3; China 2.
Balance of power exists only for BRA, CZE, DEN, ETH, FIN, ITA, PRC, SWE and SWI. Brazil plays the generic tree in
this install.

## Germany (GER)

Full FAIR plan with checkpoints: [germany-fair.md](germany-fair.md).
- **Start:** Limited Conscription, Limited Exports, Partial Mobilisation; MEFO bills (consumer goods +10%, rising about
  3% a month, price controls slow it).
- **Day 0:** remove the light-tank and old fighter lines (keep CAS if the plan uses it) and put those factories on
  rifle, support and artillery lines; keep the Type II subs, Z1934s and Deutschland, remove Hipper and both
  Scharnhorst unless the plan wants capitals; buy one rubber; research machine tools, construction, electronics.
- **1936:** Rhineland, Führerprinzip, Four Year Plan, Institute Price Controls, Anschluss (with real trained divisions in
  FAIR; one-battalion spam is META). Schacht (75 PP, -10% consumer goods), chief of army, the DNSAP event's 50 PP option,
  every price-control decision except Gömbös. Infrastructure only where no autobahn focus will max it; civilian
  factories until the 50-factory Heraeus gate, then military factories everywhere.
- **1937-38:** Sudetenland after the Sino-Japanese war starts, Fate of Czechoslovakia, Coal Liquefaction (refineries in
  high-infrastructure states from about April 1938; cancel rubber imports by mid-1938), Auftragstaktik, Buna; Hess
  regional decisions (50 PP); Todt then Speer; Extensive Conscription late 1938. Keep 100% research bonuses for
  Concentrated 2-3 or Tools 4. Free Trade is available until `GER_autarky_efforts`.
- **Army:** about 30 divisions in 1936, 60 by end 1937, 95+ by mid-1939; line 14-18 wide for mountains, 20 elsewhere;
  separate tank and motorised divisions; volunteers to Spain (armour, motorised, two wings) for general traits.
- **Navy and air:** 10-20 dockyards on torpedo destroyers from 1936 (30-40 by mid-1939), 1-2 convoy lines; from 1939
  improved-hull U-boats (60-100) on 8-12-boat groups in deep ocean; 300-600 naval bombers by mid-1940 for the Channel;
  Kriegsmarine chain `GER_strengthen_the_kriegsmarine`, `GER_trade_interdiction`, `GER_wolfpack_tactics` (needs war),
  `GER_u_boat_efforts`. Fighters from about 5 factories early to 15-25 by 1939.
- **War timing:** Danzig or War no earlier than July 1939 in FAIR; Poland without calling allies; then the Low Countries
  and France; seize gold after each capitulation; Molotov-Ribbentrop around Danzig so the USSR does not take Memel.
  Targets: about 220 military factories in 1940, 350 by end 1941, new ones in Poland away from bombers.

## Italy (ITA)

War reference and scripts: [italo-ethiopian-war.md](italo-ethiopian-war.md) §12.
- **Start:** Limited Conscription, Limited Exports, Partial Mobilisation; Il Duce (+10% PP, -15% law costs); balance of
  power 0.35 on the Mussolini side.
- **Day 0:** research machine tools, construction, engineering, range; infrastructure in Campania and Emilia-Romagna; a
  land facility near Rome; drop the interwar CAS line (one more factory on the basic CAS line), no tankettes; add
  artillery, AA and trains; new factories on infantry equipment up to 25-30; finish Montecuccoli, the Cavour rebuilds,
  Maestrale, Abruzzi and Calvi, remove the two Littorio in an Ethiopia-only plan.
- **Jan-May 1936:** everything to Ethiopia (SC01-SC03); win before the exile focus or no conference opens; accept the
  Tigray ultimatum; peace: small puppets, reparations, resource rights, release Eritrea and Somaliland.
- **Trade:** oil 4-5 civilian factories (2 after Ethiopia), coal from Germany, rubber from Siam, steel only until the
  ships finish.
- **1936-37:** Italian Highways first; 2 + 2 civilian factories, then military (or civilian until the Industrialisation
  mission's target plus one); Badoglio, Silent Workhorse; after the Ethiopian triumph take over the Ministry of
  Corporations; War Economy at about 125 PP; keep the balance of power at 0.6+ on the Mussolini side.
- **1938-40:** Albania; the strengthen-the-military mission needs 750k men in the field within a year (real divisions);
  enter the war on a French claim (never expires) or with Germany at Danzig; Romanian oil through the Balkans ends
  fuel trouble.
- **Navy:** torpedo destroyers plus the existing battleships under Fleet in Being; subs in groups of 10 in deep
  Mediterranean regions; one escort group on the Red Sea route while East Africa is supplied.

## Japan (JAP)

- **Start:** non-aligned; Limited Conscription, Limited Exports, Partial Mobilisation; -20% justify time; 3 slots;
  Army, Navy, Zaibatsu and Civic factions.
- **Day 0:** research production, construction, engineering; agency; cancel tank lines; cut each ship line to its
  current hull, delete early cruisers, destroyers under about 50% and subs; 2 convoy lines, 2 fighter, 3 support, the
  rest infantry equipment; steel from Manchukuo, rubber from Siam.
- **1936:** crush the Kodoha on 26 February (historical path); abandon the naval treaties and take Naval Reform;
  Showa Aircraft and Kawagoe Arsenal (+4 military factories), Revisit the 21 Demands, Concentrated Industry; zaibatsu
  Mitsui, Sumitomo, Mitsubishi with Guide the Zaibatsus; meet army demands (750k men in the field in about a year,
  300 artillery in a year) with real divisions; about 25 civilian factories, then military and dockyards; naval
  exercises to about 100 navy XP only.
- **1937 China war:** cheapest CAS lines; invade at Shanghai and north of it, Jinan is the key hub; collaboration on
  China before it capitulates; switch to Closed Economy and National Defence State (war with a major or war support
  over 75%, enemy industry over half of yours) before China capitulates; harsh quotas during the war, civilian
  oversight after; puppet Chinese states for research sharing.
- **1938-41:** Indochina, Siam, Kido Butai; Strike South brings in the USA. Navy: Base Strike, 8 carriers in two stacks,
  a quarter to a third of carrier planes fighters, heavy fighters for island bases; commerce raiding is pointless.

## Soviet Union (SOV)

- **Start:** Volunteer Only, Export Focus, Civilian Economy; Trotskyite Plot (-15% PP, -20% stability).
- **Day 0:** delete light-tank, naval and air lines; guns, AA, support, artillery, trucks, trains; finish ship lines
  at one hull; rubber from the Dutch East Indies; research construction, electronics, tools then concentrated.
- **1936:** convert the 90-95 divisions to one template, 4 armies, exercises; infrastructure and civilian factories in
  Stalino, Kharkov, Moscow and Stalingrad; agency; paranoia: inspect navy and air force, never army or
  administration, refuse Yezhov; first 150 PP on Kalinin (+15% stability); attaché to Spain to reach 50% war support,
  then straight to War Economy in 1936.
- **1937-38:** High-Yield Agitprop (15/30/60/120 PP), Spanish gold (25 PP); purge focuses through the Bloc of Rights and
  Trotskyites to end paranoia; 100-101 factories for the Five-Year-Plan focus, then military factories east of Moscow.
- **1939-41:** Molotov-Ribbentrop, Baltic ultimatums, Winter War, Desperate Measures when war starts; defensive
  fighters; about 20% more military factories than Germany; hold Barbarossa on rivers with fallback lines.

## United Kingdom (ENG)

- **Start:** Volunteer Only, Export Focus, Civilian Economy; 1000 convoys.
- **Day 0:** Local Autonomy occupation law; civilian factories in London and Yorkshire; finish only ships about 90%
  done; 2 convoy lines; a naval airframe line for MIO funds; sell old planes, tanks, spare convoys and trains on the
  market at the high price (no tool yet).
- **1936:** Limited Rearmament, Reinforce the Empire; Naval Reform at 35 navy XP; improve relations with Republican
  Spain, then an attaché for war support (without it Partial Mobilisation comes far too late); Silent Workhorse.
- **1937-38:** Free Trade, Shadow Scheme, War Industrialist before Baldwin retires, the research-slot focus; military
  factories in full-infrastructure states from about 1937; land facility in the West Midlands; infrastructure in
  aluminium and rubber colonies; Prepare for the Inevitable and General Rearmament make Partial Mobilisation work like
  War Economy for military factories (the UK cannot leave Partial before war, so never convert civilian factories);
  100% research bonuses on Tools 4 and Construction 4.
- **1939:** War Economy and Blitz Spirit at war, trade law back to Export Focus; Imperial Conference (dominions at
  opinion 80+); hold Suez, Gibraltar and Malta; sink Italy's fleet first. Navy: ASW destroyers on escort (1 per 12
  convoys per region) plus hunter patrols; carriers 2-4 per strike force; fighters at home with radar.

## United States (USA)

- **Start:** Disarmed Nation, Free Trade; Undisturbed Isolation (50% consumer goods, -50% factory and dockyard
  construction, isolation raises the justify and guarantee gates); Great Depression (+1 PP a day cost, -50% recruitable).
- **1936:** Small Lobbying Effort spam, accept senators' build requests; infrastructure and facilities first (isolation
  slows only factories and dockyards); Continue the New Deal, AAA (Silent Workhorse, Financial Expert); fill the 230-day
  Fair Labor gate; Democrats in November; Morgenthau; improve worker conditions once; dockyards early.
- **1937-38:** Fair Labor, War Department; attaché to China when that war starts (+10% war support); Selective
  Training Act; The Giant Wakes; the Federal Housing Act ends the Depression; Partial Mobilisation; Panay "Demand
  Compensation"; Stimson.
- **1939-41:** War Plan Black; join the Allies but enter the war only with forces ready in the UK; late civilian-to-
  military conversion is nearly free (-50% focus, armament organiser, Total Mobilisation): about 60 civilian factories
  converted in mid-1941. Dispersed industry. Strategic bombers and heavy fighters only for a long war.
- **META:** an early war to drop the Depression; civ boosting to allies.

## France (FRA)

- **Start:** Limited Conscription, Export Focus, Civilian Economy; 8 military, 33 civilian factories; Disjointed
  Government (+0.8 PP daily cost, -10% stability, -50% surrender limit); Full Employment (going past Limited
  Conscription turns it into a -10% output Worker Shortage).
- **1936:** Local Autonomy; agency; civilian factories in Normandy, Loire, Champagne (infrastructure in Calais first);
  Revive the National Bloc, Laissez-Faire (bonus lands two years later); take the 1 May 1936 election event (free Early
  Mobilisation); Review Foreign Policy, Silent Workhorse; dockyards on convoys.
- **1937-38:** Begin Rearmament; the flame-tank project (cheap design now, converted later); attaché to China;
  Strengthen Government (PP-starved until 1938); Blum-Viollette, Encourage Immigration (removes Full Employment before
  Extensive Conscription); Defensive Stratagems a year after Strengthen Government (removes Disjointed Government);
  bank 300 PP for War Economy and Extensive Conscription on war day.
- **1939:** join the Allies; Colonial Industry (+12 factories); against the AI, counter-attack into the Rhineland with
  tanks or SP guns while Germany is in Poland.

## Poland (POL)

Monarchy route and META plan: [poland-meta.md](poland-meta.md).
- **Historical Sanation:** Four Year Plan, Railway Gaps, Agrarian Reform, industry focuses; Partial Mobilisation as soon
  as possible; National Defense Fund; Clamp Down on Danzig and the Gdańsk decisions (remove Embargoed Economy);
  diplomatic pressure on the USSR for a pact that holds to 1940; Plan West decisions; War Economy when a neighbour
  justifies.
- **Economy:** concentrated industry; military factories early (civilian until early 1938 at the latest); keep the 1936
  rifle and artillery lines and put new factories on 1939 models only for elite divisions; no planes; AA and forts.
- **Army:** about 30 divisions at the 1939 mobilisation; fight on rivers and forts, yield open plains, fallback lines,
  reserves behind hubs.

## Hungary (HUN)

- **Start:** Disarmed Nation, Export Focus, Civilian Economy; Trianon (-10% output, -50% military construction); Gömbös
  treaty (-15% PP).
- **1936-37:** railroad reintegration, Imrédy or Ganz, Secret Rearmament, then Partial Mobilisation at once; Horthy
  consolidation; bank rearmament manpower and train by exercises; renounce Trianon; Győr Programme (academy, mines,
  Manfréd Weiss steel, first military factories), Pécs oil; infrastructure and a facility once slots run out (~1940).
- **1938-41:** First Vienna Award, Carpathian Ruthenia, Second Vienna Award without war; Tripartite Pact only after
  France falls; Yugoslavia with Germany in 1941; joining the Allies cuts rubber, so build air first or refineries.

## Romania (ROM)

- Military factories from the start in Bucovina, Moldova and Muntenia (full infrastructure); Royal Dictatorship,
  industry focuses first, Limited Conscription, Balkans Dominance; meet the 400k/500k/750k focus gates with real
  divisions (cavalry spam is META); puppet Bulgaria, align Hungary, split Czechoslovakia, then Greece and Yugoslavia by
  mid-1939.

## Nationalist China (CHI)

Full FAIR plan: [china-fair.md](china-fair.md).
- **Start:** 8 military, 11 civilian factories, 2 slots; corruption (economy laws +65% cost); armed-forces corruption
  (-78% recruitable); the Nine Power Treaty locks China on Free Trade (no Export Focus, Limited Exports or Closed
  Economy while it holds); push consumer goods down with focuses and a construction minister.
- **1936:** Extend Compulsory Education first (shrinks Neglected Countryside every 180 days); the industry timer focus;
  production-efficiency tech; inland military factories; sell the air force; Falkenhausen; Liangguang event top option;
  Xi'an incident, United Front.
- **July 1937:** refuse Japan's ultimatum; abandon the northern deserts; hold the Taiyuan mountains and river lines by
  the hubs; War Economy, not Total Mobilisation; let naval invasions land away from ports and cut them off.
- **1937-39:** subjugate warlords, Request Forces (Sichuan, 35 divisions); Demonstrate Resolve missions (25 PP, hold 90
  days); watch United Front influence: spending it can hand the faction lead to a warlord.

## Sweden (SWE)

- **Start:** Volunteer Only, Export Focus, Civilian Economy; Hungersköld (strikes below 68% stability, ends at 45
  civilian factories or 14 built, +120 PP on removal); Folkhemmet (+20% civilian and infrastructure speed, -20%
  military and dockyard); Riksdag balance of power (centre +10% stability, right side speeds military factories).
- **Monarchist line:** Defense Act (Bagge), urbanisation, radio decisions, Government Weapons Contract, Old Enemy Stirs,
  25% war support, Partial Mobilisation; Meet the King at 65% non-aligned about January 1938, Counter-Revolution;
  Narvik crisis (+40% war support), puppet Norway; Bornholm, puppet Denmark; integrate both before German interests in
  Scandinavia; Winter War, Åland, puppet Finland; infrastructure in the three northern resource states, then Gävleborg
  (dam).

## Ethiopia (ETH)

War reference and scripts: [italo-ethiopian-war.md](italo-ethiopian-war.md) §12.
- **Start:** 2 military, 2 civilian factories; Chitet law; Heroes of Adwa (+10% attack against Italy); League embargo;
  balance of power -0.25 (Mesafint side).
- **Day 0:** convert to mountaineers; 5 divisions hold Gondar; abandon Tigray to three hill tiles; force-retreat from
  the first attacks; factories on infantry equipment; trade with France over land.
- **War:** Rally Around the Emperor, Military College, Scavenging, Renewed Offensives, Northern Thrust (+25% attack,
  +50% breakthrough for 90 days), Mercenaries, The Emperor Stays, Learning from the Enemy; the 10 PP recall decision
  for instant units; walk into Aussa once Italy holds Tigray; rotate, small southern pockets, War Economy, refuse Italy's
  peace; take Massawa; the Fait Accompli focus (`ETH_fait_accompli`) cedes held Eritrea, Somaliland and Jubaland (SC04-SC06).
- **After:** infrastructure focuses before the Federal Empire; reintegrate puppets at 75 PP.

## Manchukuo (MAN)

- **Start:** 3 military, 2 civilian factories; Kwantung Veto (+25% law and advisor costs); Low Legitimacy (-50%
  stability and war support).
- **Order:** Pacify the Countryside, the subject-law change (Japan cannot call you into the China war), Local Arms
  Procurement (+4 military factories), Purge the General Affairs Council (removes the veto), then Seize Japanese Arms
  Depots repeatedly (about 10k guns the first time); independence war when Japan is about half way into China.

## Yugoslavia (YUG)

- Tall line: Western Focus, Friendship Treaty with Italy (keep the pact), Attract Allied Capital, devolve into ten
  subjects (each runs the generic tree and builds), then Reunite the Kingdoms about May-June 1940 for about 40
  military and 60 civilian factories; forts on Axis borders.

## Brazil (BRA) and Mexico (MEX)

- **Brazil:** generic tree here: generic industry focuses and the generic political branch; openers written for the
  Brazilian tree do not apply.
- **Mexico (fascist line):** Improve Worker Conditions, Spain volunteers, Social Reformer, arrest Cedillo, prosecute the
  clergy, fascist government (+150 PP), Partial Mobilisation; Central American wars while keeping your own generated
  tension under about 10% are META. Communist line: Red Shirts, invite Trotsky, revolution at 40% communism.
