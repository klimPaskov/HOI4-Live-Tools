# Japan FAIR (1936 start, Normal, historical AI)

Objective: conquer China (Nationalist China capitulated, every Chinese faction beaten), capture every Pacific and
East Indies island of the USA, Britain, the Netherlands, Australia and New Zealand, hold sea control of the Pacific,
and win against the USA and the Allies (USA, Australia, New Zealand capitulated; Malaya, Singapore, Borneo, Hong
Kong, Burma and the Raj taken). Rules: FAIR (see [rules-and-modes](rules-and-modes.md)): China from mid-1937, the
Allies from December 1941, no manual war-goal justification before the Danzig crisis, Japan and its co-prosperity
sphere never join the Axis, Papua New Guinea before Australia or New Zealand, Hawaii and the Philippines before
mainland North America (Alaska excepted), the Soviet Union only after all mainland Asia and only through the focus.
FAIR is not a handicap: every war is planned to be won fast and completely against the AI.

One focus point = 7 days; most Japanese focuses cost 5 (35 days), some 10 (70 days), a few 2-3.

**Start (1.19.4, ten DLCs incl. No Compromise No Surrender and Arms Against Tyranny)**: tree `japan_wtt_focus`;
3 research slots; 26 civilian / 16 military / 15 naval factories; 60 divisions (15 Hohei Shidan of 12 infantry,
30 Chuton-chi of 6 infantry, 9 cavalry, 3 mixed brigades, 2 motorised, 1 marine); command power 0; army, air and
navy experience 0; war support 60%, stability 20%; laws limited_exports, limited_conscription,
partial_economic_mobilisation; grand doctrines Grand Battleplan and Base Strike; `mtg_transport` known (invasions
of 6 divisions, 2 plans). Tanks and planes are designer equipment (`medium_tank_chassis_0` Type 89,
`small_plane_airframe_0`, `cv_small_plane_*`, `small_plane_naval_bomber_airframe_1`).

## Contents
1. Checkpoint table
2. Failure modes and standing countermeasures
3. Hard gates and traps
4. Dated focus plan
5. Do-not list
6. Laws, politics, research, production, construction, design companies, army
7. China: collaboration, capitulation arithmetic and the 1937 war
8. The warlords
9. Navy and air
10. The Pacific: order of operations
11. Completion arithmetic
12. Run record checklist

## 1. Checkpoint table

| Date | Must be true (else: stop, diagnose, fix before continuing) |
|---|---|
| 1936-01-01 | Agency founded; every military factory on a kept start line (no plane line); air and navy exercising; research slots full; Imperial Defense Plan running. |
| 1936-03 | Kodoha purged (historical path locked); chief of army and the Illusive Gentleman hired; Political Loyalty bought; network in China growing; the first Hohei Shidan (A) training. |
| 1936-08 | Chinese network at 50%; the first collaboration run started (three runs by 1937.5). |
| 1937-01 | 4th research slot (Showa Research Association) or a recorded reason; civilian construction never idle. |
| 1937-05 | Collaboration in China at 90% or more; East Hebei garrisons focus running to finish on 1937-07-07; Shanghai landing prepared (fleet on patrol and invasion support 50 days); army exercises stopped 30 days before the war. |
| 1937-07-07 | War with China: 110+ divisions (36+ Hohei Shidan (A), 15 Hohei Shidan, 6 Kido, 3 marines, the hold and garrison); Extensive Conscription taken; State General Mobilization queued. |
| 1937-12 | Shanghai, Nanjing and Shandong held; the North and Central groups linked at Xuzhou. |
| 1938-01 | Military factories and dockyards replace civilian construction (2 mil : 1 dock until about 40 dockyards). |
| 1938-03/04 | China capitulated (target 1938-03-31 at 51-56 VP); laws switched before the capitulation. |
| 1940-06 | The remaining warlords beaten (justified from 1939-09-01); about 40 dockyards; landing craft researched or due by 1941-06. |
| 1941-12-07 | 12 fleet carriers or the dated programme to them; 210 divisions; air bases filled in Indochina, Siam, Hainan, Taiwan; every landing route patrolled 50 days. Pearl Harbor port strike and five landings the same week. |
| 1942-06 | Philippines, Malaya, Singapore, Borneo, the Dutch East Indies and Hong Kong held; Burma falling; the US Pacific Fleet sunk or in port. |
| 1942-08 | Midway and Hawaii held; Papua New Guinea held; sea control Home Islands to Hawaii. |
| 1943 | Australia (1943-01 on) and New Zealand capitulated; the Raj capitulated or held; California landing from 1943-03. |
| Final | USA capitulated (330-377 VP); every island state of the goal controlled; no owned state occupied. |

## 2. Failure modes and standing countermeasures

| Failure | Standing countermeasure |
|---|---|
| China war planned for 1938 at 283 days with a third of the enemy's division count on the axes. | Three collaboration governments before the war day, the war on 1937-07-07 by the East Hebei focus, two army groups on two fronts with a sea landing; the capitulation needs 51-56 VP, not the west of China (section 7). |
| The start Chuton-chi edited into the main line: every fielded division drops strength and XP. | New templates train for the attack; the start ones hold unedited (JF-09). |
| Limited Conscription kept while the training queues need 200+ divisions. | Extensive Conscription the war week (JF-04). |
| Surface fleet with no sea control at the landings; carrier planes with no means to fly. | Carriers first in groups of four with their capitals and screens, patrol 50 days on each route before its date (JF-17, section 9). |
| US objectives dated before the force that takes them exists. | Date each landing from the force forecast; the second wave leaves China only when China and the Raj are done (section 10). |
| Support and train lines removed on day 0. | Support equipment and trains never go obsolete: keep a line (JF-07). |

## 3. Hard gates and traps

- **The war day is a focus.** `JAP_reinforce_the_beijing_garrison` (70 days) fires `china.1` when it completes.
  Otherwise the Kwantung Army mission fires it 725 days after the Kodoha purge (about 1938-02). Start East Hebei
  on about 1937-04-28 so it completes on 1937-07-07.
- **War Economy** needs war support above 50% and a fascist or communist government: Japan on the historical
  (neutrality) path cannot take it. Total Mobilisation needs a war and war support above 80%. Closed Economy needs a
  war and fascism or communism. **State General Mobilization** needs a war.
- **Extensive Conscription** needs war support above 20% and fascism/communism or a war against an enemy whose
  strength ratio is above 0.5: China opens it on the war day.
- **The London Naval Treaty** spirit caps carriers at 9,500 IC, cruisers at 5,300 and battleships at 10,000; the
  decision that breaks it needs a focus off the historical path. Design carriers under the cap.
- **Research slots**: Expand the Showa Research Association (+1, after the Central Economic Council) and Fund the
  RIKEN Institute (+1, needs one Approach focus and more than 34 civilian factories).
- **Special-forces cap**: max(24 battalions, 5% of the non-special-forces battalions). Marines and mountaineers
  share it: 24 in 1937, about 75 in 1939, about 100 at 200 divisions.
- **Develop the Chinese Resources** needs Excavation 1 and full control of the Chinese resource states; the three
  aluminium decisions need Excavation 3 and that focus.
- **Pressure Siam** needs Japan to lead a faction (the Co-Prosperity Sphere); Demand Tonkinese Bases and Occupy
  Indochina target French Indochina: after France capitulates.
- **Escalate the War in China** adds 2.5% US war support each time (a higher US surrender limit later).
- **FAIR justification**: no manual justification before 1939-09-01; the warlords not in China's war wait for it.
- **Invasion capacity**: 6 divisions and 2 plans with `mtg_transport`; 10 divisions and 5 plans with
  `mtg_landing_craft` (1940); marine subdoctrines add more. A landing needs sea control at launch (60% of the
  needed dominance; patrol builds 2% a day).

## 4. Dated focus plan

| # | Focus | Days | Date (earliest) | Why |
|---|---|---|---|---|
| JF-01a | Imperial Defense Plan | 21 | 1936-01-01 | root of the arsenal focuses |
| JF-01b | Establish Showa Aircraft Industry | 35 | 1936-01-22 | +2 military factories |
| JF-01c | Purge the Kodoha Faction | 0 | ~1936-02-26 (Aizawa flag) | locks the historical path; opens the Attache System |
| JF-01d | The Military Ministers' Attache System | 35 | 1936-02-26 | stability +15%; the Harakiri chain |
| JF-01e | Develop New Fighters | 70 | 1936-04-01 | 100% light-air bonus: the improved small airframe ahead of time |
| JF-01f | Harakiri Debate, Konoe's First Cabinet, National Spiritual Mobilization, Central Economic Council | 14+35+35+35 | 1936-06 to 1936-10 | the chain to the 4th research slot; Incite Tensions in the Diet shortens the Harakiri wait |
| JF-01g | Expand the Showa Research Association | 70 | ~1936-10 (done ~1936-12) | +1 research slot |
| JF-01h | Ministry of Commerce and Industry, Continue Takahashi's Policies | 35+35 | 1936-12 to 1937-02 | +1 civilian each |
| JF-01i | Revisit the Twenty-One Demands, Reinforce the East Hebei Garrisons | 21+70 | 1937-04-07 / 1937-04-28 | the war on 1937-07-07 |
| JF-01j | State General Mobilization (the war week), Approach Mitsui, Fund the RIKEN Institute | 35+70+70 | 1937-07 to 1937-12 | +5% factory output; +2 civilian and 50% production bonus (Tools 4); +1 research slot |
| JF-01k | Kawagoe and Hiratsuka arsenals, Nanshin-ron, Expand the Navy's Technical Department, Prepare the Southern Fronts, Focus on Aircraft Carriers | | 1938 | +2 military, +3 and +1 dockyards; naval and marine bonuses; carrier modules 100% |
| JF-01l | Religious Organizations Law, New Order Movement, Imperial Rule Assistance Association, Co-Prosperity Sphere | | 1938-1939 | political power and stability; faction for Pressure Siam |
| JF-01m | Approach Sumitomo/Mitsubishi, Great Japan Industrial Patriotic Association, Develop the Chinese Resources | | after the council idea expires | civilian factories, factory speed, resources |
| JF-01n | Soviet-Japanese Neutrality Pact, Demand Tonkinese Bases and Pressure Siam (after France falls), Unsinkable Aircraft Carriers, Senjinkun at the Allied war | | 1939-1941 | the southern bases; no war on Siam |

Fill every gap with a cheap focus; never let the focus slot idle.

## 5. Do-not list

- Do not edit a fielded start template; build new ones.
- Do not take Hokushin-ron (exclusive with Nanshin-ron) or Occupy Siam (Pressure Siam keeps Siam as a partner).
- Do not escalate the war in China.
- Do not build in Korea; no supply hub before the railway to it exists.
- Do not train 2-wide divisions to convert, and no disabled-equipment templates (FAIR).
- Do not use a CAS airframe as a fighter (role swap) or empty dominance-only carriers.
- Do not fight a fleet battle in a storm, without air cover, under enemy air superiority or on low supply.
- Do not refit a ship to a new engine; never swap main armament, engine or armour in a refit.
- Do not trade away civilian factories before China capitulates.

## 6. Laws, politics, research, production, construction, design companies, army

| # | Area | Decision |
|---|---|---|
| JF-03 | Research bonuses | Develop New Fighters on the improved small airframe (and the carrier fighter airframe); Mitsui on Tools 4; Sumitomo on Excavation 2 and Computing Machines; Prepare the Southern Fronts on marines and landing craft; Focus on Aircraft Carriers on the carrier hull and deck. Time each bonus to land when its tech is next in its slot. |
| JF-02 | Research by slot | Three slots from day 1: (1) Tools 1, improved small airframe on the bonus, Concentrated 2, AA 1, Tools 4 on the Mitsui bonus, AA 2; (2) Computing, Concentrated 1, Tools 3, Construction 3, Excavation 3, improved air torpedoes, improved carrier hull, magnetic detonator; (3) Construction 1, Tools 2, Computing 2, infantry equipment 1, Excavation 1-2, AT 1-2, Concentrated 4. Fourth slot (~1936-12): heavy MG, Computing Machines, Radio 1, survivability, infantry equipment 2, gun 3. Fifth (~1937-12): Concentrated 3, mountaineers, support weapons, then marines 2, landing craft by 1941-06, improved destroyer hull, carrier airframes. |
| JF-04 | Politics and laws | Kan'in as chief of army (offensive, 50 PP) in 1936-02; Shun Akikusa (the Illusive Gentleman, +1 operative) the month the agency exists; Incite Tensions in the Diet; Extensive Conscription the war week; Total Mobilisation the day war support passes 80% at war; Luxury is the Enemy, Labour Patriot Corps and Bullet Stamps after Spiritual Mobilization; a war industrialist and the industrial concern; War Bonds; the aluminium decisions one a month after Excavation 3; Nagumo (carrier) in high command before 1941-06. Keep Limited Exports; never Free Trade. No War Economy (gate). |
| JF-05 | Construction | Civilian factories to 1937-12-31 in the best-infrastructure home states; from 1938-01-01 military factories and dockyards 2:1 until about 40 dockyards, then military factories. 1940-01 to 1941-09: naval bases (Kitakyushu or Pusan to 5, by Taipei, Hainan), level-2 railways Korea-Beijing-Hankou and towards Hanoi-Bangkok, air bases to the maximum in Indochina, Hainan, Taiwan and Siam, coastal forts on Okinawa with spare construction. Ports in northern Malaya once it is held. No refinery before the Indies' oil (1942) is out of reach. |
| JF-06 | Decisions | Incite Tensions in the Diet (1936-02); Luxury is the Enemy, Labour Patriot Corps, Bullet Stamps; annex Manchukuo by the integration decision after it finishes its settler focus; the three aluminium decisions (Shandong, Yunnan, Sichuan) one a month; never Escalate. |
| JF-07 | Production | Day 0 onto the existing lines: rifles 8, artillery 3, support 2, trucks 1, Type 89 medium 2 (the tankette line closes; its factory joins the medium line); no plane line. 1937-01: a train line; AT 1. 1937-10: fighters 10 on the improved small airframe, 25 by 1939-01, 40 by 1941-06. 1938-06: land naval bombers 10; carrier fighters and carrier naval bombers to fill 12 decks by 1942-06. 1940-01: CAS 8. Rifles 15 by 1938, 25 by 1940. A new line for each new gun; never move a mature line. |
| JF-08 | Design companies | Kokura Arsenal on rifles; Osaka Army Arsenal (artillery) on artillery, AT and AA; Mitsubishi Heavy Industries on the medium tank; Mitsubishi Aircraft on land and carrier fighters (cost, output and efficiency traits); Yokosuka Naval Air Arsenal on land and carrier naval bombers; Yokosuka Naval Arsenal on carriers and destroyers; Kawasaki Dockyards on the submarine refits (long-range raiding first). Production traits first. |
| JF-09 | Templates (vanilla widths: 2 a battalion, line artillery 3) | Hohei Shidan (A): 9 infantry + 1 line artillery, engineer, recon, artillery (width 21), the China attack. The start Hohei Shidan (12 infantry) attacks unedited; the start Chuton-chi (6 infantry) holds unedited. Rikusentai (L): 9 marines + engineer, artillery (18): 2 by 1937-06, 10 by 1941-09 (the cap). Kido: 4 medium + 3 motorised + engineer, recon, artillery (14): 6 by the war day. Hohei Shidan (P): (A) + support AA (21) from 1940-06. Sanchi Shidan: 9 mountaineers + engineer, artillery (18) from the special-forces headroom, for Burma and Yunnan. Kowan Shubitai: 12 infantry + artillery, AA, AT (24) on the island ports. Occupation: cavalry under Harsh Quotas. |
| JF-10 | Army size | 1937-07-07: 110+ divisions (about 60 attacking). 1938-03-31: 140. 1941-12-07: 210 (70 line (A)/(P), 15 Hohei Shidan, 6 Kido, 11 marines, 60 garrisons, 40 hold/occupation, 8 Sanchi). 1943-06: 260+ with the US expedition. |
| JF-11 | Officer corps | Political Loyalty first (army XP, 1936-03); Industrial Liaisons (air); State Serves the Military after the New Order Movement. Army XP then goes to the new templates and the Grand Battleplan subdoctrines towards Grand Assault. |
| JF-12 | Command power | Command power starts at 0: promote one field marshal per 40 as it accrues (Yamashita for the North China group, Hata for Central China, Terauchi for the southern group of 1941); traits Infantry Expert on line generals, Organizer or Logistics Wizard on marshals; Imamura (commando) leads the landings. |
| JF-13 | Exercises | Air and navy from 1936-01-01; army from 1936-04 to 30 days before the war; navy to 1941-10. |
| JF-18 | Doctrine | Keep Grand Battleplan (start) through the China war; Base Strike with Patrol Boats first, then Massed Carrier Fleet and Naval Gunfire Support; air: Operational Integrity with Medium-Range Escort and Precision Naval Bombing. |

## 7. China: collaboration, capitulation arithmetic and the 1937 war

| # | Item | Decision and arithmetic |
|---|---|---|
| JF-14 | Agency and collaboration | Agency 1936-01-01; upgrades Form Department, Passive Defence x4, Army Department, decryption, training centres, Naval Department (no Suicide Pills); three operatives with the Illusive Gentleman. Network 50% in Nanjing/Shanghai by about 1936-07, then three collaboration runs (90 days, 2 operatives each): 1936-08, 1936-11, 1937-02, to 90% or more before the war day. Each run costs 200 support equipment, 1,000 rifles and 10 civilian factories for 60 days (rising on repeats). |
| JF-14a | Capitulation arithmetic | China: 92 VP, base surrender limit 0.8 (74 VP). War support 32% at the start lowers it 0.108; collaboration 0.9 lowers it 0.135 (0.15 per unit): limit 0.557 = 51 VP; 56 VP if Chinese Unity 1 lands. The lower Yangtze, Shandong, Henan and Wuhan hold 55-60: Nanjing 20, Wuhan 10, Shanghai 5, Qingdao 5, Suzhou 3, Henan (Zhengzhou, Kaifeng, Luoyang) 4, northern Jiangsu 4, Zhejiang 4, Nanchang 3, Anhui 2. Changde, Changsha and the west are not needed. |
| JF-15 | The war (1937-07-07) | Two army groups, one operation each. North China (Yamashita): (A), the start Hohei Shidan and the Kido along the Beijing-Tianjin-Jinan-Xuzhou railway; the start Chuton-chi hold Shanxi, Chahar and the Manchukuo border. Central China (Hata): the Shanghai landing on the war day + 3 (3 marines + 3 (A) on the two transport plans; the fleet on patrol and invasion support in the East China Sea and Yellow Sea from 1937-05-15), then 15 (A) through the port; a Qingdao landing in 1937-08 cuts Shandong. The groups link at Xuzhou (~1937-11) and pocket the Shandong and Jiangsu armies; Nanjing ~1937-12, the Zhejiang coast 1938-01, Henan 1938-02, Wuhan 1938-03: capitulation ~1938-03-31. Follow the railways, cross rivers early, attack from two provinces. Switch the war-only laws before the capitulation. |
| JF-15a | Air over China | No new planes: the start fighters on superiority over the North China and Shanghai regions, CAS over the axes, naval bombers on port strike against the river fleet; bases on Okinawa, Dairen and East Hebei. |

## 8. The warlords

| # | Decision |
|---|---|
| JF-16 | Warlords that join China's war fall with it or are mopped up in 1938-04 to 1938-09. The others: justify from 1939-09-01 (Militarism -20% justify time), declare from about 1939-10-15 two at a time (Guangxi and Yunnan first: they border Indochina and Burma; then Shanxi, the Ma clique, Xinjiang/Sichuan; the communists last), done by 1940-06. Harsh Quotas until annexed, Civilian Oversight after. |

## 9. Navy and air

| # | Item | Decision |
|---|---|---|
| JF-17 | Fleet | Carriers first: 12 fleet carriers by 1942-06 in three groups of four, each with four capitals (the ten start battleships, no new battleships, no super-heavy project) and twelve screens (light-attack cruisers and torpedo destroyers; 36 + 6 spare). Submarines second: the 52 start submarines refitted (torpedoes only) and 24 new, task forces of 10 on raiding in the Coral and Tasman Seas and the Bay of Bengal from 1942-01. Twelve ASW destroyers (1936 hull, depth charges and sonar) on convoy escort in home waters from 1937. 300 more convoys by 1941-06. |
| JF-17a | Dockyards | 8 new carriers x ~10,000 + 42 screens x ~1,150 + 24 submarines x ~450 + 300 convoys x 70 = ~150,000 IC by 1942-06 at about 2.3 IC a dockyard-day: about 40 dockyards from 1939-01 (15 + 4 from focuses + ~21 built 1938-01 to 1939-06). |
| JF-17b | Missions by date | 1936 to 1937-04 training; 1937-05-15 to 1938-04 patrol and invasion support in the East China and Yellow Seas; 1938-05 to 1941-10 training to level 3, then reserve; 1941-11-20 the first carrier group to strike force for the Pearl Harbor port strike on 1941-12-07 (Base Strike: +40% port strike); the second group on strike force in the South China and Philippine Seas; patrol on every landing route 50 days before its date; 1942-02 to 1942-06 the Java Sea, the Bay of Bengal (Ceylon) and the Coral Sea; 1942-06 Midway and Hawaii with all twelve carriers. Fighters to escort before a fleet battle. |
| JF-19 | Air | Land fighters on the improved small airframe: 1,200 by 1941-12 over Indochina, Siam, Taiwan, the Philippines and Malaya on the first day of the Allied war. Carrier air for 12 carriers: one third carrier fighters, two thirds carrier naval bombers (torpedo), ~600 deck planes by 1942-06. Land naval bombers 600 by 1941-12 for port strikes and strikes near the bases (land-based planes joining a fleet battle are capped at 5% of the fleet's strength at first). CAS 400 by 1942 for Malaya, Burma and the Philippines. Heavy fighters for island bases later. |

## 10. The Pacific: order of operations

| # | Date | Operation |
|---|---|---|
| JF-20a | 1941-12-07 | Pearl Harbor port strike; the same week landings on five plans: Luzon, Malaya (from Indochina and Siam), Guam, Wake; Hong Kong by land from Guangzhou; Sabah and Kalimantan by 1941-12-20. Many landings at once exhaust the Allies' command power. |
| JF-20b | 1942-01 to 1942-05 | Singapore by land; Bataan and Corregidor; Java and Sumatra 1942-02/03 (then maximum infrastructure and garrisons); Burma by land from Siam (Rangoon, the Mandalay hub). |
| JF-20c | 1942-04 to 1942-08 | Ceylon and the Bay of Bengal; Midway, then Hawaii (before any landing in North America); Papua New Guinea (before Australia), the Solomons, Fiji, Samoa and the Gilberts to cut the US-Australia route. |
| JF-20d | 1942-06 to 1943 | The Raj from Burma and by landings in Bengal and Madras, fast. |
| JF-20e | 1943 | Australia from 1943-01 (Darwin, Queensland; Sydney, Melbourne, Brisbane, Canberra), New Zealand 1943-06. |
| JF-20f | 1943-03 to 1945 | The US west coast from Hawaii, then inland to the Midwest and the north-east until the surrender. |

## 11. Completion arithmetic

| # | Enemy | Victory points (total / needed) |
|---|---|---|
| JF-21a | China | 92 / 51-56 with three collaboration runs (section 7) |
| JF-21b | USA | 471 / 377 at limit 0.8 (330 with Wounded National Pride): New York, Washington and the industrial north-east are needed; the west coast alone is not enough |
| JF-21c | Raj | 224 / ~152 at its 10% start war support (limit 0.68) |
| JF-21d | Australia | 142 / ~92 at 0% war support (limit 0.65) |
| JF-21e | New Zealand | 28 / ~19 |
| JF-21f | Netherlands | its home is in Europe; the Indies are taken and held (the European capitulation is not Japan's) |

Landings: about 21 (the first-week five, Borneo, Java, Sumatra, Ceylon, the Andamans, Midway, Hawaii, Papua, the
Solomons, Fiji, the Gilberts, Queensland, Auckland, California) plus the follow-up waves through captured ports.

## 12. Run record checklist

- The war day, the collaboration level on it and the capitulation date with the VP held.
- Divisions by template on 1937-07-07, 1938-03-31 and 1941-12-07; factories and dockyards on the same dates.
- Carriers, screens and submarines by date; planes by type on 1941-12-07.
- Each landing: date, divisions, plans, the sea regions patrolled and the dominance at launch.
- Every departure from this card with its reason.
