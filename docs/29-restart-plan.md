# 29 · Restart plan

*Plan, 2026-10-23; progress updated 2026-10-25 08:30 UTC. Mario asked for an in-depth plan to restart development after the pause, at high quality and token efficient. Wave 1 started at 01:00 NZDT on Saturday 24 October (2026-10-23 12:00 UTC). About 120 pull requests (#184 to #304) were built in the two days after; see [Progress](#progress). Work pauses for the week from 2026-10-25.* The illustrated version is the "Grubcraft Restart Plan" artifact (https://claude.ai/artifact/Qw4kBb6YPYZeq6ckdjJytB).

## Progress

*As of 2026-10-25 08:30 UTC, counting two pieces still finishing as done: hidden traps (#304) and the computer players' half of fortifications stage 8 (walls, bridges, stakes, traps and crossing ice; plan in `design/fortifications/stage8-plan.md`). The waves below are the original plan; threads ran ahead of them wherever nothing blocked them, so waves 1 to 5 are built except for the items under Left.*

| # | Thread | Built (pull requests) | Left |
|---|---|---|---|
| T1 | Pathfinding overhaul | Overhaul #193; attacks on buildings you can see #200 | Nothing. Nav doors (#203) and the reopen hook (#199) were built by T6 and T9 |
| T2 | Living Land maps | Stage 1 grown land #189; stage 2 ways through (passes, fords, walkable mountains) #292; stage 3 Measured maps #293; stage 4 sites by reason and five habitat scenery models #295 (habitat ground by T8, #221) | Stage 5, water that runs downhill, is a later call for Mario |
| T3 | Cinematic lighting | Stage 1 #188; particles #201; clean edges and motes #209; looks per land and F10 panel #217; camera craft #222 #228 (director camera, replay films) #233; weather and night look #235 #239 #245 #247 #250 #254 #255 #257 #261 #262 #265; F10 knobs #268 | Tuning on Mario's Mac with F10 |
| T4 | Every soldier fights | Scrum #187; duels #191 #192 #195; charges that plough (idea 11) #212; commander on the field (idea 12) #226; matched kills #243 #263; tumbling #266; finishers #270 #280; motion warping and champion kills #272; head turns #275; general as hero #277 | A duel spacing retune with T5 (below); option C is a later call |
| T5 | Materials, animation engine, fight library | Surface detail #198; engine stages 1 to 4 #213 #224 #229; fight library #238 #241 #260 #267 #274 #279; kills and finishers allow for where models are drawn #282; tipping models #285; blows, reels and kills for the Brood, riders and other rigs #291 #299 #301 | Building re-bake (waits on Mario); fixing the bake itself so models are drawn where they stand, with T4's duel spacing retune; the rest of the fight library ([33](33-animation-engine.md) stage 5): more blows and kills per weapon, kanabo and sword-and-shield moves, kills for the other factions' weapons |
| T6 | Fortifications and bases | Towers #190; turning #196; gates #203; drawn walls #215 #230 #236; ring tool and blueprints #244; wall levels #251; manning #264; ladders #273; faction wall twists #278; gatehouses #290; bridges #302; stakes and barricades #303; hidden traps #304; computer players fortify, assault walls and cross ice (stage 8, AI side) | Stage 5, the eight new towers (Bolt and Blast for each faction) and tower levels; stage 8's walled-town bonus and town life |
| T7 | Battle effects and aftermath | Sieges on particles #207; sparks and wounds #211; standards, crows and grime #219; Blood setting #286; toppling walls and towers #294; rubble as slow ground #296; trophies #298 | Prisoners, a later call (idea 14 needs a design) |
| T8 | Living ground | Ground textures #194; scenery detail #197 #202; Brood veins (19) #205; faction lands (15) #208; ground marks (2) #216; habitat ground #221; stumps #225; scorched ground #232; mud #237; scenery re-bake and Conquest map marks #289 | Nothing |
| T9 | Weather and changing land | Clearing land (22) #199 #206 #210; reading the ground (13) #227; weather, mud and night rules #242 #249 #252 #258; woods that burn #271; fords that flood (6) #283; seasons in Conquest (7) #288 | Small later stages in [32](32-changing-land.md): lightning as a game event that can set woods alight, burning buildings lighting the ground, Highland snow |
| T10 | Battle soundscape | Positional sound and beds #184; blows, weather and habitat sounds #287 (thunder came with T3, #262) | Tuning by ear on Mario's Mac |
| T11 | Ambient life and busy bases | Clearing on screen #214 #234; living touches #218; busy bases #220; walkers #231; log piles #240; deer #246; doves #248; houses in Grubfolk villages #253; villagers #256; crow model #259; caravans #269; insects #276; fish rises #281; crows on standards, wheel dust, carts on slopes #300 | Polish from [34](34-ambient-life.md)'s "not yet" lists: supply carts in bases, forge sparks, other lands' fauna, crows on corpses, ruined hamlets |

**Ideas 1 to 22:** all are built: 1 weather and time of day, 2 a world that remembers, 3 wildlife and villagers, 4 sound, 5 director camera and replays, 6 usable rivers (floods and bridges), 7 seasons, 8 wear, trophies and torn banners, 9 night battles, 10 busy bases, 11 physical formations, 12 commander on the field, 13 terrain tactics, 14 aftermath (prisoners are a later call), 15 faction lands, 16 smarter formations, 17 pathfinding, 18 better guides, 19 Brood veins, 20 option B, 21 less-flat textures (the building re-bake waits on Mario) and 22 clearing land.

Outside the eleven threads: the Hand over button to give your army to an ally (#186). CI and docs housekeeping: #185 #204 #223 #284.

**What remains, in one list:**
1. Fortifications stage 5: the eight new towers (Bolt and Blast for each faction) and tower levels ([26](26-fortifications.md)).
2. Fortifications stage 8, the town half: the walled-town bonus and town life.
3. The building re-bake with the stronger bake, once Mario says "OK, edit the building configs".
4. The bake fix so models are drawn where they stand, together with a duel spacing retune (T5 with T4); the rest of the fight library (more kills per weapon, kanabo, sword and shield, the other factions' weapons).
5. Smaller extras: lightning as a game event, burning buildings lighting the ground, Highland snow, and ambient polish (supply carts, forge sparks, more fauna).
6. Tuning on Mario's Mac: lighting with F10, and sound by ear.
7. Mario's later calls: option C (every soldier a body in the simulation), Living Land stage 5, prisoners after battles, faction depth stages 7 to 9.

## What it covers

- **Living Land**, organic world generation ([27](27-living-land.md)), stages 1 to 4, with stage 5 as a later call. It absorbs natural mountains stage 2.
- **Cinematic lighting and effects** (the "Grubcraft Lighting Study" page), stages 1 to 5 and the seven small wins Mario approved on 2026-10-22.
- **Every soldier fights** ([28](28-soldier-combat.md)): option B first, option C later.
- **Fortifications and bases** ([26](26-fortifications.md)), all eight stages.
- **Ideas 1 to 19** from 2026-10-22. Ideas 16 (smarter formations, PR #178) and 18 (better guides, PR #177) are built. Idea 20 is soldier combat.
- **Idea 21, less-flat textures** (Mario, 2026-10-23, at minimal cost): (1) a small shared set of repeating detail textures (stone, wood, plaster, cloth, tiles) layered by material over every building and unit; (2) real grass and soil textures with normal maps blended across the ground, from the Grass001 and Ground054 sets in the project files (built 2026-10-23, see [10](10-architecture.md)); (3) lighting stage 1, already planned; (4) a stronger bake with crevice darkening, colour varied from block to block and stronger normals, then every building and unit re-baked; (5) the same surface detail on scenery (Mario, 2026-10-23): trees, bushes, grass, rocks, stumps and the other scenery models get bark, leaf, stone and moss detail textures with surface relief, rocks share the Rock035 cliff textures so they match the mountains, tree and rock bases blend into the ground, and the whole scenery pack is re-baked with the stronger bake. (Built 2026-10-23, render only: see [25](25-scenery-models.md#surface-detail); the scenery pack was re-baked with the stronger bake's colour variation on 2026-10-24.)
- **Idea 22, clearing land to expand** (Mario, 2026-10-23): trees and scrub stand in the way of a growing base, with the full mechanics, animations and planning. See [Idea 22 in brief](#idea-22-in-brief).

- **Idea 23, living waterways** (Mario, 2026-10-25): rivers that vary with the seasons, currents and flash floods. Planned in [38](38-living-waterways.md), to be built later.

Not in it: faction depth stages 7 to 9 (Tithe, Brood metamorphosis, heroes). They fit in any wave from 2 on without clashing with this plan if Mario wants them.

## Idea 22 in brief

Mario (2026-10-23): "Make it so that tree/scrub etc clearance is a thing for expanding bases", planned and built in full, with animations. The plan, with first-pass numbers that T9's design note settles:

- **Ground has to be cleared to build on.** Buildings can't go on scrub, woods or thicket. Workers get a **Clear** order (drag an area): scrub goes quickly, woods slower, thicket slowest. Laying a building on overgrown ground queues the clearing first, so a blueprint just takes longer.
- **Cleared ground changes the battlefield.** It becomes open ground with stumps: nobody hides there any more, troops no longer slow down in it, and cleared thicket stops blocking the way. Felled timber pays a little Grub.
- **Each faction clears its own way**, to confirm with Mario in the design note. For example, the Clans fell and haul timber, the Concord burns it (fast, leaving scorched ground), the Choir shatters it with resonance, and Brood Gnawers eat it.
- **Maps and computer players.** Living Land grows woods and scrub round every start, with equal open and clearable room for each player (T2's fairness work). Computer players clear ground as they expand.
- **Settled (2026-10-24, T9's design note, [32](32-changing-land.md)):** scrub 3 s, woods 8 s, thicket 15 s a tile for one worker; woods pay 3 Grub, thicket 5. Shared rules built first; Mario chose all four faction twists as proposed, built next.
- **On screen.** Workers chop with their own moves, trees shake and fall, logs pile up, and stumps and bare earth stay behind. The moves and models are Sonnet 3D work.
- **How it works.** One runtime ground-change command updates a tile's cover and, for thicket, the walking grid through T1's reopen hook. It goes through the lockstep like every other order, and burning woods, flooded fords and siege rubble reuse it later.

## How the work is split

Work is grouped into **eleven threads, one per area of code**. A thread keeps its area for all of its stages, so it never has to relearn the code, and two threads never edit the same files at once. Since M3 the project has read [15](15-staffing-plan.md#hard-rules) rule 3 ("one writer per crate") as one writer per module; this plan makes that explicit for `gc_sim`, `gc_render` and `gc_game`.

| # | Thread | Covers | Owns | Lead · effort | Helpers | Waves | Cost |
|---|---|---|---|---|---|---|---|
| T1 | Pathfinding overhaul | Idea 17; nav doors for gates; the hook that reopens cleared ground (idea 22) | `gc_sim` `nav.rs`, `nav_reblock.rs` | Opus · high | Haiku: benches, ford test, goldens, docs. Sonnet: nav debug overlay | 1–2 | L |
| T2 | Living Land maps | Doc 27 stages 1–4; scrub, woods and thicket as separate cover, and the clearable room near each start counted in fairness (idea 22) | `gc_map`, `gc_mapcheck`, map dressing in `gc_game/scenery.rs` | Opus · medium | Sonnet: port each layer behind hash tests, and the five new scenery models. Haiku: mapcheck sweeps, load times, screenshots, docs | 1–4 | L |
| T3 | Cinematic lighting | Lighting stages 1–5 (stage 1 is idea 21 part 3); small wins: camera tilt, backlit foliage, film grain, light motes; idea 5 director camera and replays; look of ideas 1 and 9 | `gc_render` frame, post, shadows, sky, particles; `world.wgsl` lighting; `gc_game` `camera.rs`, `replays.rs` | Sonnet · high for stage 1, then medium | Haiku: screenshots, bench runs, docs | 1–4 | L |
| T4 | Every soldier fights | Doc 28 stages 1, 2, 5; small win: soldier variation; hit flash; idea 11 physical formations; idea 12 commander on the field | `gc_game/puppets.rs`; `gc_sim` melee in `combat.rs`, `morale.rs`, `regiments.rs`, `champions.rs` | Opus · medium for stage 1, high from stage 2 | Sonnet: scrum and matched-kill screen code. Haiku: balance tables, benches, goldens, docs | 1–4 (C after) | XL |
| T5 | Materials, animation engine and fight library | Idea 21 parts 1 and 4 (detail textures by material, the stronger bake, re-baking buildings and units); doc 28 stages 3–4 | `gc_render/soldiers.rs` (it draws buildings too) and skinning, `gc_assets` model format, `gc_artbake`, the building and unit bake scripts | Sonnet · high for the engine, medium for bakes and moves | Sonnet: re-bakes, retargeting and tagging moves in Blender, one faction at a time. Haiku: screenshots, docs | 2–4 | XL |
| T6 | Fortifications and bases | Doc 26 stages 1–6 and 8; bridges that are built and burned (idea 6) | `gc_sim/structures.rs`, tower volleys, new wall modules; placing tool; fortification AI | Opus · medium | Sonnet: placing tool and wall interface. Sonnet: wall and tower models. Haiku: goldens, docs | 1–5 | XL |
| T7 | Battle effects and aftermath | Doc 26 stage 7; doc 28 stage 6; idea 8 wear, trophies, torn banners; idea 14 crows and lingering dead | Effect modules in `gc_game` and the particle users in `gc_render` | Sonnet · medium | Sonnet: collapse chunks, banner and trophy models. Haiku: sound list, screenshots, docs | 3–4 | L |
| T8 | Living ground | Idea 21 part 2 (blended ground textures with normals, its first pull request); ideas 15 and 19; idea 2 trampled grass and scars (also on the Conquest map, idea 14); small win: trampled ground; doc 27 habitat ground materials; cleared ground and stumps (idea 22); idea 21 part 5, surface detail on scenery (its second pull request) | Ground surface in `world.wgsl`, new ground-mask module; scenery drawing (`gc_render/scenery.rs`, `scenery.wgsl`), the scenery model format (`gc_assets::scenery`) and `gc_scenebake` | Sonnet · high | Sonnet: the scenery re-bake in Blender. Haiku: texture sourcing and baking, screenshots, docs | 2–3 | L |
| T9 | Weather and changing land | Idea 22 first: its design note, then clearing trees and scrub to expand (orders, rules, computer players); idea 13 terrain tactics; ideas 1 and 9 rules (rain, mud, night vision by light); idea 2 woods that burn and are cut; idea 6 fords that flood; idea 7 seasons in Conquest | `gc_sim/terrain.rs`, `vision.rs`, new `weather.rs`, a runtime ground-change command | Opus · medium | Haiku: balance tables, goldens, docs | 2–5 | L |
| T10 | Battle soundscape | Idea 4, later hit, weather and habitat sounds | `gc_audio`, `gc_game/sounds.rs` | Sonnet · medium | Haiku: sound sourcing and licence checks, credits | 1–2, 4 | M |
| T11 | Ambient life and busy bases | Ideas 3 and 10; idea 22 on screen (workers chopping, trees falling, log piles); doc 27 living touches (mist, fireflies, birds) | New `gc_game` ambient module | Sonnet · medium | Sonnet: animal and villager models. Haiku: screenshots, docs | 3–4 | L |

**Cost tiers** are relative, for planning: **M** is two or three pull requests, mostly Sonnet. **L** is four to six pull requests, or fewer with an Opus lead on hard code. **XL** is seven or more pull requests, long-lived.

**Models and effort** follow the project rules: Opus leads anything that changes the simulation, netcode or replays; Sonnet leads render, audio and tools. **3D modelling runs on Sonnet 5.5 alone** (Mario, 2026-10-23): models, rigs, moves, bakes and re-bakes, with no Opus supervisor and no Haiku builders. Low effort is never used. Extra-high or max needs Mario's go first; option C may be worth suggesting it.

## Waves

A wave starts when the work it waits on has merged, not on a date. At most seven threads run at once.

**Wave 1 (Saturday 24 October, 01:00 NZDT): six threads.**

| Thread | First pull request |
|---|---|
| T1 Pathfinding overhaul | Routes that match building footprints and terrain exactly, find clear gaps between buildings and replan fast; the open nav follow-ups (sector heuristic near walls, `Grid::number_areas`, local portal updates). It must also win back route-search headroom under the 2 ms p99 budget, because gates (T6 stage 2) spend it |
| T2 Living Land maps | Stage 1, grown land, with the old Wilds kept as Classic and a 768 map built in under 3 seconds |
| T3 Cinematic lighting | Stage 1, Light and Air, plus camera tilt, backlit foliage and film grain, starting from the prototype in `design/cinematic/` (built 2026-10-23, see [30](30-cinematic-lighting.md); the looks per land, lobby Time of day, Clarity and tuning panel followed on 2026-10-24) |
| T4 Every soldier fights | Stage 1, the scrum, plus per-soldier tint and animation phase variation |
| T6 Fortifications and bases | Stage 1, towers fire |
| T10 Battle soundscape | Positional sound and ambient beds per land |

Only T1 and T6 change the simulation in wave 1, in different files.

**Wave 2:** T1 builds nav doors (cells open to one team) and the hook that reopens cleared ground, then finishes. T9 starts in its place with idea 22: a short design note for Mario, then clearing. T2 stage 2, once T1's overhaul is in. T3 builds particles first (stage 3, built 2026-10-23: see [30](30-cinematic-lighting.md#built-stage-3)) because four threads wait on them, then clean edges (stage 2, built 2026-10-24: see [30](30-cinematic-lighting.md#built-stage-2)) and light motes, with hit flash left to T4 and trampled ground to T8. T4 stage 2, duels in the simulation. T6 stage 2, gates and turning, once the nav doors are in. New, both once T3 stage 1 is in: T5 starts with idea 21's surface detail (the stronger bake, detail textures by material, then the re-bakes), carried in the same model-format change as the animation engine; T8 starts with real ground textures (idea 21 part 2), then puts the same detail on scenery (idea 21 part 5), using T5's shared detail textures and stronger bake, and re-bakes the scenery pack.

**Wave 3:** T2 stage 3 (measured fairness) and stage 4. T3 stage 4, camera craft and the director camera. T4 physical formations. T5 fight library, Ember Clans first. T6 stages 3 and 4. T8 habitat ground materials, once Living Land stage 1 is in. T9 terrain tactics, once duels are in. New: T7 and T11, once particles are in; T11 starts with idea 22 on screen.

**Wave 4:** T3 stage 5, the look of weather and night. T4 stage 5, matched kills and impacts, and the commander on the field. T5 the other three factions' moves. T6 stages 5 and 6 (with bridges). T7 sparks, blood, wear and aftermath, using T4's blow events. T9 weather, night and changing land. T10 hit, weather and habitat sounds.

**Wave 5:** T9 seasons in Conquest. T6 stage 8, walled towns and smarter sieges. Then Mario's calls: option C, Living Land stage 5 (water that runs downhill), prisoners.

## Shared files

Some files every thread touches. Rules for them:

- **Merge train.** `SIM_VERSION`, the replay `FORMAT`, the net `PROTOCOL`, the golden replays, the `PINNED` table and `tools/gc_bench/baseline.json` are taken in merge order. A thread rebases on main, takes the next free numbers, re-blesses goldens and the pinned table, and merges as soon as it is green. The others rebase after it.
- **Interfaces between neighbours.** T5's detail textures and stronger bake are described in [31](31-surface-detail.md): T8 reads `assets/detail.pak` (`gc_render/src/detail.rs` uploads it) and the same bake method for scenery. T5 gives T4 a move picker; T4 keeps `puppets.rs`. T3 gives T7, T8 and T11 the particle system and ground-mask hooks; they add modules and only one-line hooks in `gc_render/lib.rs`. T5 makes the shared detail textures and the stronger-bake method; T8 uses both for scenery, and T2's new scenery models are made to T8's format (any made earlier are re-baked with the rest). T6 touches only tower targeting in the volley code; T4 does not touch volleys. T9 reaches combat only through the terrain modifier.
- **The AI keeps up.** Each simulation thread updates the computer players for its own feature in the same pull request, and runs the AI-only Conquest games before it pushes.
- **Guides, docs, status.** Every pull request updates the guides it affects (Guide menu, Quick Start, building tips, tooltips), the docs it affects, and the status page after merging, and the merge reply ends with the Mac build link.
- **Quality.** `/simplify` then `/code-review` on every code pull request, at high effort for the simulation, netcode, replays and AI, medium elsewhere.

## Saving tokens without losing quality

- Start **fresh build threads** briefed by the merged docs (26, 27, 28 and this one). The planning threads carry large prototype contexts that every later turn would read again; they stay as references.
- **Long-lived threads**, one per area of code, carry what they learn through all their stages.
- **Opus writes a short spec and reviews**; Sonnet helpers write render, interface and tool code; Haiku helpers re-bless goldens, run benches, balance tables and map sweeps, take screenshots, write docs and guide text, and source textures and sounds.
- **Medium effort by default.** High only where a slip is costly: route-search speed, duels and balance, the gates in route search, and the first look pass of the lighting and the ground.
- **Unblock first.** The pathfinding overhaul lands before gates and grown mountains, and particles land before every effect, so nobody builds twice.
- **Reuse what exists:** the lighting prototype and its presets, the Python map generator, the fight toy, the Mount & Blade research.

## Mario's to-dos

- **When you're back:** say "OK, edit the building configs" when you want all 46 buildings re-baked with the stronger bake.
- **When you play next:** tune the lighting on the Mac build with F10 (wet, snow, stars, cloud banks, thunder, torches, Brood glow) and copy the Enter log to the lighting thread. Tune the sound beds, blows and weather sounds by ear.
- **Decide what's next:** Bolt and Blast towers and walled towns are ready to build; option C, Living Land stage 5, prisoners and faction depth stages 7 to 9 wait on your call.
- **Open calls with recommendations** (work follows the recommendation unless Mario says otherwise): the three calls in [26](26-fortifications.md#calls-for-mario). Blood is settled: stylised by default, Full as a setting (#286).
