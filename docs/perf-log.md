# Performance log

Before and after numbers from each milestone's performance pass ([12 · Performance](12-performance.md#the-loop)). Newest first.

## Room for the whole block (2026-10-17)

Routes now keep each block's whole width clear of deep water and cliffs, cost more near them, are pulled tight, and blocks queue behind friends and narrow into columns in fords ([10](10-architecture.md#pathfinding)). This is more work for the simulation on purpose, so the instruction baseline (`tools/gc_bench/baseline.json`) was re-blessed in the same PR.

| Scenario (bench gate, instructions) | Before | After |
|---|---|---|
| `drill` (150 a player, 1,200 ticks) | 5.56 G | 7.08 G (+27%) |
| `pathing-storm` | 2.48 G | 3.01 G (+22%) |
| `field-8000pt-2v2` | 7.28 G | 8.42 G (+16%) |

| Wall clock, cloud container, p99 | Before | After |
|---|---|---|
| `drill` (300 a player, 12,000 ticks) | 1.28 to 1.31 ms | 1.76 to 1.97 ms |
| `pathing-storm` | 0.33 to 0.36 ms | 0.44 to 0.67 ms |
| `field-8000pt-2v2` | 0.42 to 0.47 ms | 0.66 to 0.68 ms |

**Where it goes** (drill, share of instructions): the sector search (`nav::Search::sector`) is now 23% (9% before), because a step near the banks costs more and the search looks wider for a roomier way; the friend-in-the-way scan and block overlaps another 17%. **Taken back already:** the wall cost comes from a table worked out once, the room across a block skips ahead by the clearance it finds, and the search finds each cell's weight once per cell instead of once per step; all three keep every hash. **Left for later:** a cheaper heuristic for the sector search near walls (the drill's p99 is within the 2 ms budget, CI's runners measure about two thirds of the container's time).

## The field remembers the fight (2026-10-16)

Fallen soldiers now stay for the whole battle and spent arrows stay where they land ([09](09-art-and-audio.md#animation)). This is presentation only, so the simulation's tick time and every golden replay are unchanged.

**Memory:** the fallen are 24 bytes each, up to 32,768 (768 KB, over five times the grand battle's 6,052 soldiers). Stuck arrows are 24 bytes each, up to 24,576 (576 KB). Both live in the `present` tag, about 1.3 MB in all.

**Draw cost:** only what the camera or the sun can see is drawn. A body lying down uses one level of detail coarser than a standing man at the same spot, and a plain 10-triangle box past the coarsest model. An arrow is two boxes. The living have first claim on the renderer's room for soldiers, and the arrows come last, into whatever box room is left.

**Measured** with a temporary timer around the two push functions (since removed), in the cloud container, release build, on a sandbox scene with the camera over the dead. The container has no GPU, so only the CPU side is measured here; the frame rate there comes from software rendering and says nothing about a real graphics card.

| Case | Corpses pushed, per frame | Arrows pushed, per frame |
|---|---|---|
| Empty field (before) | 0.8 us | 0.2 us |
| 6,052 fallen and 12,000 arrows, all around the camera | 301 us | 262 us |
| Worst case: 32,768 fallen and 24,576 arrows, all around the camera | 1.65 ms | 0.6 ms |

The worst case is a pile far thicker than any real battle makes, and its 2.3 ms together is over the 1.0 ms presentation line (though well inside the frame's slack). A real field is mostly out of view, and the cost falls with it. To be checked on real hardware: if the GPU side of 30,000 lying bodies in one view proves too dear, lower `MAX_CORPSES` or raise `CORPSE_LOD_BIAS` in `gc_game`.

## M1 pass (2026-10-15)

**Where it was measured:** the `drill` benchmark (four players with 300 regiments each on the ridged map), 3,000 ticks under Valgrind's callgrind, counting instructions. That count is exact and repeatable, unlike wall-clock time in the shared cloud container. Wall-clock numbers below are the median of several runs and move by ±15% between runs.

**How tick time moved through the day** (drill p99, cloud container, each merge on main):

| Main after | Drill p99 |
|---|---|
| LAN step 3 (#34) | 0.99 ms |
| Weapon room (#35) | 1.04 ms |
| Duels (#37) | 0.97 ms |
| Ambushes (#38) | 1.11 ms |
| AI room to fight (#39) | 0.96 ms |
| Push-through (#31) | 1.37 ms |

Push-through added about 40%, close to the 1.35× its PR measured. The budget is 2 ms.

**Top three costs of a drill tick** (share of all instructions, including world creation):

| # | Cost | Share | What it is |
|---|---|---|---|
| 1 | `Combat::friend_in_the_way` neighbour scan | 18.6% | Each marching regiment in tight ground looks for a friend holding its ground in the way (the ford and pass queue) |
| 2 | `Combat::crowding` neighbour scan | 9.6% | Each regiment finds the friends its block overlaps (push-through and disorder) |
| 3 | `nav::Search::sector` | 9.2% | Route searches across the portal graph |

**Fixed in this pass:** number 1 searched every grid cell within its reach plus the radius of the largest regiment on the map. It now searches only the cells a friend it could count can be filed in: its reach, plus one tile, plus the look-ahead, plus the furthest any regiment can step this tick (the grid is filled before anyone moves; a debug assertion checks the step). The same friends are found, so the drill and every golden replay keep their hashes.

| | Before | After |
|---|---|---|
| `friend_in_the_way` | 3.84 G instructions | 2.43 G (−37%) |
| Whole tick | 17.71 G | 15.63 G (−12%) |
| Drill p99, wall clock | 1.37 to 1.45 ms | 1.19 to 1.25 ms |

**Left for later:** number 2 (crowding) already looks only under each regiment's own circle; making it cheaper means caching overlaps between ticks. Number 3 belongs with the pathfinding work in M3, when Conquest maps add buildings to route around.

## Six players on a 768-tile map (2026-10-17, Conquest step 6)

**Where it was measured:** the new `conquest-6p` bench (`gc_bench --scenario conquest-6p`): six players with 100 regiments each on a 768-tile ridged map in a Conquest match played in fog of war, 230 buildings laid one a tick at the start, 6,000 ticks. Cloud container, wall clock; instruction counts under callgrind for the breakdowns.

**Two costs that only show on big maps, found and cut:**

| | Before | After |
|---|---|---|
| A building going up (typical tick) | 21 ms | about 5.5 ms |
| A building going up (worst tick) | 220 ms | 10 to 17 ms |
| Tick p50 in fog of war | 2.5 ms | 0.87 ms |
| Tick p50 without fog | 0.42 ms | 0.44 ms |

- **Laying a building** renumbered the walking grid's areas with a flood fill over the whole map (about 650 M instructions on a 768-tile map). Areas are now renumbered near the building when that can be told from a box a sector round it (comparing the box's pieces before and after, so an area that only meets itself beyond the box is no reason to look further), and in two quick passes over the map when it can't (two areas joined, or a wall cutting a piece in two that both run on out of the box); links are made again only in sectors where a cell may have opened or shut. What a building costs now is mostly the searches in the sectors round it, and placing portals across the whole map (about a tenth).
- **Fog of war** had every regiment test every building each tick (600 × 230), and search 32 tiles round it every tick. Buildings are now sorted into 32-tile cells each tick and only nearby cells are tested, and each regiment and building looks round it once in four ticks (five times a second, each on its own tick; a sighting lingers 3 seconds anyway).

**Left for later:** the sim heap is 22.4 MB on a 768-tile map with room for 1,536 regiments (the walking grid and its copies), over the 8 MB budget set for 256-tile maps; budgets for big maps, and a CI run of this bench, come with the six-player soak. Placing portals only near the change, and keeping the next free area number instead of finding the highest when a pocket is cut off, are both in `nav.rs` and left for its owner.

