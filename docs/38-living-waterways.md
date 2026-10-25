# 38 · Living waterways

*Design note, 2026-10-25, idea 23 (Mario: "water current, flash floods and season variability"; "I mean the rivers and waterways vary with seasons"). Plan only: Mario asked for it to be explored now and built later. It builds on the weather and changing land work ([32](32-changing-land.md)): fords that flood (stage 5) and seasons in Conquest (stage 6).*

## The idea

Rivers stop being fixed lines on the map. They run one way and push what wades in them. Through the year they rise and fall, so a crossing that is safe in summer is shut in spring. A storm upstream can send a wall of water down a river with only a few seconds' warning. A commander reads the water as they read the ground.

## What there is today

- **Maps** grow rivers by real drainage (`gc_map::grown::water`). Every tile knows how much water drains through it, and which neighbour it drains to. Fords are laid where a long river is narrowest. All of this stays in the map generator: the simulation only gets heights, one water line and the walkable grid, where deep water is blocked.
- **The simulation** has one water level for the whole map:
  - `Terrain::water_level` is the line, and `Terrain::rise` is how far a flood has lifted it.
  - `Terrain::wet` says whether a point is in water: wading is slow, troops in it fight worse and can't charge.
  - `gc_sim::flood` shuts the fords on the walkable grid when the rise is full, and sweeps regiments caught in them to the nearest bank.
  - Seasons give the year (`World::season`), and the winter freezes the water (`Terrain::frozen`).
- **Water has no direction.** A river and a lake behave alike. Every river on the map rises and falls together, and only long rain moves the level.

## What changes

### 1. Rivers that vary with the seasons

The water line moves through the year, as one level for the whole map (`Terrain::level`, signed, replacing `rise`):

| Season | The rivers | In play |
|---|---|---|
| Spring | **The melt.** The winter's snow comes down: the more snow fell (a snowpack counted from snow spells in the winter), the higher the water, up to a full flood. | Fords shut for the high weeks of spring, as in a rain flood. Banks are wading ground. |
| Summer | **Low water.** The line drops up to a fifth of a tile below the map's own. | Shallows at the edge of the deep channel are walkable (summer fords, see below), so a river that was a wall has new crossings. Mud dries. |
| Autumn | **The rains.** Back to the map's line; long rain floods as today. | As now. |
| Winter | **Ice**, as built: the shallows freeze unless a flood is up. Snow piles up for the spring melt. | As now. |

- **Summer fords.** When the water is low, cells that are shut only because the water is too deep, and whose bed lies above the low line, are found once at the start. The map hands these over as a depth layer, since the simulation never sees why a cell is shut, so cliffs, buildings and the map edge are never opened. The map's own wading rule (beds down to 0.35 of a tile) applies with the line lowered. Narrow rivers whose beds sit right at the threshold are rounded to stay shut, so low water opens real crossings, not a thin strip along every bank. They are opened on the walkable grid as ford ground: a new nav change beside `Change::Flooded` (`Change::Exposed`, its own per-cell layer, empty until used). When the water comes back up, anyone on them is swept off as in a flood. A message says when they open and shut, and the top bar shows "Low water".
- **Order rules.**
  - A rain flood during low water adds to the low level, so it has to lift the water further before the fords shut.
  - Summer fords open only while the water is low and not frozen.
  - A bridge (`Change::Bridged`) wins over both water layers: a cell under a bridge is open whatever the water does, and a fallen bridge leaves the cell to them.
  - A cell that is both exposed and flooded is shut.
- **The melt needs snow.** The first spring of a match has no winter behind it, and fixed weather never snows, so there is no melt in either case. The melt is as big as the last winter's snowpack.
- **No seasons, no change.** A Field Battle, or a Conquest match with seasons off, keeps the map's line except for rain floods. So nothing moves for matches without seasons, and the goldens stay put.

### 2. Water that flows: currents

- **A flow layer.** The map hands the simulation a flow for every water tile: which way the water runs (one of eight directions) and how fast (0 to 3: still, slow, steady, fast). It comes from the drainage the map already works out, with lakes still. Joined rivers share a river id, and each river's source tiles are marked. It is a new optional `Ground::flow` layer, saved in the replay. Maps that don't have one (old replays, hand-made maps) have still water everywhere, so nothing changes for them.
- **Drift.** A regiment whose centre is in flowing water (a ford, a flooded bank, a summer ford) is pushed downstream a little every tick:
  - a slow river pushes it a tenth of a tile a second;
  - a fast one pushes it four tenths;
  - a flood or the spring melt doubles the push.

  The push moves the whole block, as a shove does, and goes through the same blocked-cell checks as walking, so it never puts anyone in deep water. A regiment with no order drifts at most 3 tiles and then holds on the nearest bank, so it never creeps downstream for ever. A regiment marching upstream through a ford goes at three quarters of its wading pace, and one marching downstream gets a quarter more.
- **Swept downstream.** A regiment caught in a shut ford is carried down before it reaches a bank: it loses men as now, and comes out downstream of where it went in.
- **Seeing it.** The flow layer is given to the game (`Terrain::flow`), so the water can be drawn moving with ripples, foam where it is fast, and floating debris in a flood. That drawing belongs to the lighting and ground threads. The selection panel says "Wading against the current".

### 3. Flash floods

- **What sets one off.** A storm: a spell of rain at strength 220 or more. Each storm spell rolls once (a hash of the match seed and the spell's start, so no random stream moves) for a flash flood on one river. The chance is flat with seasons off and higher in spring and autumn. Fixed-weather matches never get one, so the goldens are safe.
- **The wave.** The flood starts at the top of a river and runs down it at about 6 tiles a second. Each water tile knows how far it is downstream (worked out once from the flow layer). As the front passes, the water there jumps to a full flood for 20 seconds, then falls back over 30. Fords under the wave shut, and anyone in them is swept, as in a rain flood but faster and only on that river. Each river's fords are a group of their own with their own shut flag. The group shuts once when the front reaches its first ford and opens once when the water falls, so the walkable grid changes twice per wave, not every few ticks.
- **Warning.** About 8 seconds before the front reaches any ford, a rumble plays and a message says "A flash flood is coming down the river". An eye on the river sees the wave coming: a front of brown water and foam, drawn from the wave's position.
- **The cost.** A per-tile water level replaces the one map-wide level only while a wave is running: `Terrain::wet` adds the wave's height at that tile. The wave is a front position and a start tick, hashed with the ground while it runs.

### 4. Computer players read the water

- They keep off fords while the water is rising, a wave is coming, or the melt is up.
- They use summer fords when a summer route is shorter.
- They don't plan battles across a river that is about to shut behind them.
- Crossing the ice is the fortifications thread's (its stage 8). This work only gives it `Terrain::frozen` and the season.

## Built: stage 1, seasonal water

Built 2026-10-25. It is simpler than the plan in two ways. It needs no new map layer: the summer fords are found from the heights and the walkable grid the simulation already has. And `Terrain::rise` stays, with a separate `Terrain::low` for low water.

- **The water line.** `Terrain::water_line()` is the map's line, plus `rise` (a flood or the melt), minus `low` (a dry summer). `Terrain::wet` reads it, and so does the drawn water (`Renderer::set_water_level`).
- **Snowpack.**
  - Each second of a winter with snow falling adds up to 255 to `Flood::snowpack` (scaled by the snow's strength).
  - In the spring, 170 a second melts (`MELT_PER_SECOND`). While it melts, the rivers rise as they do in long rain, and shut the fords once at the top.
  - Less than a minute's melt (`MELT_MIN`) as the spring comes in melts away unseen, and what the spring leaves is gone by the summer. A winter of heavy snow is about a spring and a half of melt, so it shuts the fords for most of the spring.
- **Low water.** In a dry summer (the ground drier than `EBB_WET` and no flood up), the water drops 1 height unit a second, down to `LOW_DROP`, a fifth of a tile. Out of summer, or once rain wets the ground again, it comes back up at the same pace. A flood or melt first brings the water back to its line, then raises it.
- **Summer fords.** These are the tiles whose middle lies under the line but no deeper than `LOW_DROP` + `WADE_DEPTH` below it (0.55 of a tile; the map wades to 0.35), that have a cell the map gave as blocked, and that have no cliff's rise beside them. They are found when the match starts.
  - Only groups that make a crossing are kept: the land round the group's rim must fall in two or more pieces (a bar across a channel). A strip along one bank crosses nothing, so it stays deep.
  - They open at the full drop and shut again under half of it (`Nav::set_exposed_all`, a new `Change::Exposed` with its own per-cell layer, empty until used). Open cells cost as a ford (`nav::SHALLOWS`).
  - A bridge's deck wins over them, and a flood shuts them. Nothing can be built on them.
  - Once the water is back, any regiment still in their deep water with nowhere else to go (no order, or a chase, and not fighting, as in a shut ford) is sent each second to the nearest bank where its block fits. A route out of deep water, on a grid that has had low water, looks for open ground as far as a goal does.
- **Seeing it.** `Event::Shallows { open }` puts up a message ("Low water: the shallows at the rivers' edges can be waded", and "the summer fords are deep again"). The top bar's chip says "Low water: summer fords". The drawn water drops with the line. `World::flood().shallows()` and `exposed()` let the ground thread draw exposed mud banks.
- **Shared state.** Low water, whether the summer fords are open, and the snowpack are hashed with the ground, but only while any is non-zero. So matches without seasons hash exactly as before, and no golden changes. `SIM_VERSION` 53, because a Conquest match with seasons now plays differently.

## Built: stage 2, currents

Built 2026-10-25.

- **The flow layer.** Grown maps (docs/27) lay a current on every river tile as they lay the river: the way the nearest point of the river's line runs (lines run from source to mouth), in eight ways, and a speed from the river's width there: under 3 tiles is fast, under 6 steady, wider slow. Lakes, ponds and land are still. The turned half of the map runs the turned way, and the middle tile of an odd map is still. It is one `gc_sim::Flow` byte a tile in `Map::flow`, handed to the simulation as `Ground::flow`, kept in replays from format 46, and part of the map's hash. Maps that weren't grown, and older replays, have still water.
- **Drift.** A regiment marching through running water is carried a little across its way each tick (`Terrain::drift`), and steers back onto its way, so it comes out downstream of a straight line. The current runs `DRIFT_PERCENT` 10, 25 and 40 hundredths of a tile a second for slow, steady and fast water, doubled while the rivers are up. Only the part across the regiment's way carries it, and never more than half its step, so the slowest block still gets across. The drift is its own small move after the step, and is dropped if it would go onto blocked ground (`Nav::blocks`), so it never puts anyone in deep water or stops a march.
- **Up and downstream.** Wading along a river is `CURRENT_PACE_PERCENT` (8) slower against the current for each step of speed and as much faster with it, so a quarter slower up a fast river (`Terrain::going`). This is all the current does along the river; across it, it is the drift.
- **Changed from the plan.** A regiment standing still in a river holds its footing and is not carried, so a ford can still be held. With no drift for idle regiments there is no creep to cap, and no new state to hash. Troops swept in a shut ford still struggle to the nearest bank as before, not downstream. No new golden: the goldens' maps aren't grown, so they keep still water and stay put. The drift is whole-number maths like the rest of the simulation. The water is not yet drawn moving; that is a small piece for the lighting thread.
- `SIM_VERSION` 54 (grown maps play differently), replay `FORMAT` 46.

## Built: stage 3, flash floods

Built 2026-10-25 (`gc_sim::flash`).

- **The rivers.** These come from the flow layer, worked out when the match starts. Tiles of running water side by side are one river (joined rivers are one). Each tile's reach is how far up the river it lies from where the river ends: the map's edge, or a lake or pond it runs into. Each river's fords are a group of their own. Maps with still water have no rivers, so no flash floods.
- **What sets one off.** A storm: a spell of rain at strength 220 or more (`STORM_STRENGTH`) in changing weather. A sky of one spell is fixed weather and never gets one. Each storm rolls once, a hash of the match seed and the spell's start, so no random stream moves. The chance is 40%, or 60% in spring and autumn, 40% in summer and 15% in winter. A roll that hits picks one of the rivers with fords. Only one wave runs at a time, and none on frozen water.
- **The wave.** It sets off at the top of the river, or 96 tiles above its first ford if the river runs further up than that, and runs down at 6 tiles a second (`WAVE_TILES_PER_SECOND`). As the front passes a tile of that river, the water there is a full flood for 20 seconds, then falls over 30 (`Terrain::wet` adds it, from `WaveWater`). The river above where it set off stays as it was. The river's fords, and its summer fords, shut once as the front reaches the first of them, and open once when the water at the last has half fallen. Troops in them are swept to the bank as in a rain flood (`Flood::sweeps`), once the front has reached their ford. A rain flood that has shut every ford keeps them shut, and a wave's river stays shut if the rain's flood goes first. Winter ice doesn't form while a wave runs.
- **Warning.** 8 seconds before the front reaches the first ford, or at once if it starts closer, `Event::FlashFlood { stage: Coming }` puts up "A flash flood is coming down the river: get off the fords". `Here` and `Gone` follow. While a rain flood has every ford shut, the rain flood's own messages tell it; if the rain's flood goes first, `Here` comes again for the river still shut. The rumble and the drawn wave belong to the sound and lighting threads.
- **Shared state.** The wave (its river, age and two flags) is hashed with the ground only while one runs. Rehearsals (scratch worlds) set none off, and a running wave runs on in them but never changes the walkable grid they share. A storm at the very start of a match rolls on the first tick, and one at the start of spring rolls after the thaw. `SIM_VERSION` 55. No golden moves: none has changing weather on a map with running water.

## Built: stage 4, computer players read the water

Built 2026-10-25 (`gc_ai::terrain::Water`, `Terrain::read_water`).

- **What a mind reads.** Each tick, `Water::of(world)` says whether every ford is out (a flood or the melt has shut them, or the water is at least half way up to it, `FLOOD_RISE / 2`), whether the summer fords are open, and which river a flash flood is coming down or has shut (`Flash::threatened`, from the warning till its fords open). When that changes, the minds' ground changes with it: fords out of reach are blocked, open summer fords can be waded (as the simulation routes, `FORD`), cell by cell from the ground as the match began (a ford tile's deep cells stay deep, thicket still standing stays blocked), and the clearance round each tile that changed is measured again. That happens a few times a flood, not every tick. A rehearsal's minds keep the water as they found it, as its world keeps its fords.
- **What follows.** A mind's battle line looks for fords on that ground, so it neither plans to cross at a ford the water is shutting nor waits behind one for the enemy. It finds summer fords like any other. A crossing already called is called off if the water shuts its ford (or a summer ford goes deep) before anyone has gone in (`Terrain::ford_shut_near`), with the journal line "The river is rising over that ford: waiting again". Regiments already wading carry on, and the simulation sweeps any the wave catches.
- **Left out.** Crossing the ice stays the fortifications thread's. The minds don't time a crossing to a wave's passing, or lure an enemy into a ford ahead of one.
- **Shared state.** None: the minds sit outside the simulation, so no golden moves and `SIM_VERSION` stays 55.

## Stages (each its own pull request)

1. **Seasonal water.** Signed level (hashed exactly as `rise` is today while the level is at or above the line, so seasons-off matches hash the same), the map's depth layer for summer fords (replay format bump), the spring melt from a snowpack, summer low water with summer fords (`Change::Exposed`), messages and the top bar. The simulation only. Seasons-on matches only, so no golden moves. `SIM_VERSION` bump. A new golden: a Conquest year on the river map.
2. **The flow layer and currents.** `Ground::flow` from the map's drainage (replay format bump), drift, upstream and downstream pace, swept downstream. Replays carry their own ground, so the goldens recorded so far read no flow layer, get still water, and stay put. A new golden on a grown river map is recorded with currents. The water is drawn moving (a small piece for the lighting thread).
3. **Flash floods.** Storm rolls, the wave, the warning, the per-tile level while a wave runs. Changing weather only.
4. **Computer players.** Minds read rising water, waves, low water and summer fords.

## Decisions taken (Mario's rule: pick the recommended option, with an Opus check)

- **One map-wide level for the seasons**, not one per river: every river in a region lives through the same year, and a per-tile level costs memory and time all match for no visible gain. Per-tile water exists only while a flash flood runs.
- **Flash floods follow the weather, not a lobby switch.** Changing weather already says "the sky may surprise you". A separate toggle would be one more setting for little.
- **Currents only push wading troops.** Deep water stays impassable. Boats and swimming are out of scope.
- **Determinism.** Everything is fixed-point and derived from the seed, the tick and the map. No new random stream.

## Risks

- **Drift and formations.** A pushed regiment must keep its formation and its order. The push moves the whole block (as a shove does), never single soldiers.
- **Walkable-grid churn.** Summer fords change the walkable grid twice a year. That is as cheap as a flood, which already does the same.
- **River ids from the map.** The flow, river ids, sources and depths all come from the generator in one map change (stages 1 and 2 share it). Living Land maps are the Living Land thread's, so this is coordinated with it.
- **Old maps.** Maps with no flow layer get still water, so currents and flash floods need a grown map. The non-grown maps keep today's water.
