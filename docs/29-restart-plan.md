# 29 · Restart plan

*Plan, 2026-10-23. Mario asked for an in-depth plan to restart development after the pause, at high quality and token efficient. Wave 1 starts at 01:00 NZDT on Saturday 24 October (2026-10-23 12:00 UTC). Nothing in this plan is built yet.* The illustrated version is the "Grubcraft Restart Plan" artifact (https://claude.ai/artifact/Qw4kBb6YPYZeq6ckdjJytB).

## What it covers

- **Living Land**, organic world generation ([27](27-living-land.md)), stages 1 to 4, with stage 5 as a later call. It absorbs natural mountains stage 2.
- **Cinematic lighting and effects** (the "Grubcraft Lighting Study" page), stages 1 to 5 and the seven small wins Mario approved on 2026-10-22.
- **Every soldier fights** ([28](28-soldier-combat.md)): option B first, option C later.
- **Fortifications and bases** ([26](26-fortifications.md)), all eight stages.
- **Ideas 1 to 19** from 2026-10-22. Ideas 16 (smarter formations, PR #178) and 18 (better guides, PR #177) are built. Idea 20 is soldier combat.
- **Idea 21, less-flat textures** (Mario, 2026-10-23, at minimal cost): (1) a small shared set of repeating detail textures (stone, wood, plaster, cloth, tiles) layered by material over every building and unit; (2) real grass and soil textures with normal maps blended across the ground, from the Grass001 and Ground054 sets in the project files; (3) lighting stage 1, already planned; (4) a stronger bake with crevice darkening, colour varied from block to block and stronger normals, then every building and unit re-baked.
- **Idea 22, clearing land to expand** (Mario, 2026-10-23): trees and scrub stand in the way of a growing base, with the full mechanics, animations and planning. See [Idea 22 in brief](#idea-22-in-brief).

Not in it: faction depth stages 7 to 9 (Tithe, Brood metamorphosis, heroes). They fit in any wave from 2 on without clashing with this plan if Mario wants them.

## Idea 22 in brief

Mario (2026-10-23): "Make it so that tree/scrub etc clearance is a thing for expanding bases", planned and built in full, with animations. The plan, with first-pass numbers that T9's design note settles:

- **Ground has to be cleared to build on.** Buildings can't go on scrub, woods or thicket. Workers get a **Clear** order (drag an area): scrub goes quickly, woods slower, thicket slowest. Laying a building on overgrown ground queues the clearing first, so a blueprint just takes longer.
- **Cleared ground changes the battlefield.** It becomes open ground with stumps: nobody hides there any more, troops no longer slow down in it, and cleared thicket stops blocking the way. Felled timber pays a little Grub.
- **Each faction clears its own way**, to confirm with Mario in the design note. For example, the Clans fell and haul timber, the Concord burns it (fast, leaving scorched ground), the Choir shatters it with resonance, and Brood Gnawers eat it.
- **Maps and computer players.** Living Land grows woods and scrub round every start, with equal open and clearable room for each player (T2's fairness work). Computer players clear ground as they expand.
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
| T8 | Living ground | Idea 21 part 2 (blended ground textures with normals, its first pull request); ideas 15 and 19; idea 2 trampled grass and scars (also on the Conquest map, idea 14); small win: trampled ground; doc 27 habitat ground materials; cleared ground and stumps (idea 22) | Ground surface in `world.wgsl`, new ground-mask module | Sonnet · high | Haiku: texture sourcing and baking, screenshots, docs | 2–3 | M |
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
| T3 Cinematic lighting | Stage 1, Light and Air, plus camera tilt, backlit foliage and film grain, starting from the prototype in `design/cinematic/` |
| T4 Every soldier fights | Stage 1, the scrum, plus per-soldier tint and animation phase variation |
| T6 Fortifications and bases | Stage 1, towers fire |
| T10 Battle soundscape | Positional sound and ambient beds per land |

Only T1 and T6 change the simulation in wave 1, in different files.

**Wave 2:** T1 builds nav doors (cells open to one team) and the hook that reopens cleared ground, then finishes. T9 starts in its place with idea 22: a short design note for Mario, then clearing. T2 stage 2, once T1's overhaul is in. T3 builds particles first (stage 3) because four threads wait on them, then clean edges (stage 2) and light motes. T4 stage 2, duels in the simulation. T6 stage 2, gates and turning, once the nav doors are in. New, both once T3 stage 1 is in: T5 starts with idea 21's surface detail (the stronger bake, detail textures by material, then the re-bakes), carried in the same model-format change as the animation engine; T8 starts with real ground textures (idea 21 part 2).

**Wave 3:** T2 stage 3 (measured fairness) and stage 4. T3 stage 4, camera craft and the director camera. T4 physical formations. T5 fight library, Ember Clans first. T6 stages 3 and 4. T8 habitat ground materials, once Living Land stage 1 is in. T9 terrain tactics, once duels are in. New: T7 and T11, once particles are in; T11 starts with idea 22 on screen.

**Wave 4:** T3 stage 5, the look of weather and night. T4 stage 5, matched kills and impacts, and the commander on the field. T5 the other three factions' moves. T6 stages 5 and 6 (with bridges). T7 sparks, blood, wear and aftermath, using T4's blow events. T9 weather, night and changing land. T10 hit, weather and habitat sounds.

**Wave 5:** T9 seasons in Conquest. T6 stage 8, walled towns and smarter sieges. Then Mario's calls: option C, Living Land stage 5 (water that runs downhill), prisoners.

## Shared files

Some files every thread touches. Rules for them:

- **Merge train.** `SIM_VERSION`, the replay `FORMAT`, the net `PROTOCOL`, the golden replays, the `PINNED` table and `tools/gc_bench/baseline.json` are taken in merge order. A thread rebases on main, takes the next free numbers, re-blesses goldens and the pinned table, and merges as soon as it is green. The others rebase after it.
- **Interfaces between neighbours.** T5 gives T4 a move picker; T4 keeps `puppets.rs`. T3 gives T7, T8 and T11 the particle system and ground-mask hooks; they add modules and only one-line hooks in `gc_render/lib.rs`. T6 touches only tower targeting in the volley code; T4 does not touch volleys. T9 reaches combat only through the terrain modifier.
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

- **Before wave 3:** download the Mixamo sword-and-shield and great-sword sets for the fight library (doc 28), unless a bought pack is preferred.
- **During wave 1:** tune the lighting looks on the Mac build after T3 stage 1.
- **Open calls with recommendations** (work follows the recommendation unless Mario says otherwise): stylised blood with full as a setting; the three calls in [26](26-fortifications.md#calls-for-mario).
- **Later calls:** option C, Living Land stage 5, prisoners after battles (idea 14 needs a design), faction depth stages 7 to 9.
