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
