# Performance log

Before and after numbers from each milestone's performance pass ([12 · Performance](12-performance.md#the-loop)). Newest first.

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
