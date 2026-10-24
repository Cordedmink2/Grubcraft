# 32 · Changing land

*Design note, 2026-10-24, restart plan thread T9 ([29](29-restart-plan.md)). Covers idea 22 (clearing land to expand bases) first, and the one way the simulation changes the ground mid-match, which burning woods (idea 2), floods (idea 6) and rubble reuse later. Weather, night, terrain tactics and seasons (ideas 1, 9, 13, 7) get their own sections here as they are built.*

## Clearing land (idea 22)

Mario (2026-10-23): "Make it so that tree/scrub etc clearance is a thing for expanding bases."

### What grows on a tile

Every tile has a **growth**, kept by the simulation and changed only by it:

| Growth | Where it comes from | Building on it | Clearing time (one worker regiment) | Felled timber |
|---|---|---|---|---|
| Open | Bare ground, meadow, tall grass in a meadow | Yes | — | — |
| Scrub | Scrub habitats on grown maps (scrub edge, bramble and the like), whatever their cover | Cleared first | 3 s | none |
| Woods | Woods cover | Cleared first | 8 s | 3 Grub |
| Thicket | Thicket (woods no one marches through) | Cleared first | 15 s | 5 Grub |

Tall grass in an open meadow stays buildable: it is trampled flat, not grubbed out. Classic maps have no habitats, so their growth is only woods and thicket. Clearing times follow the match's tempo, like build times. Timber pays less than gathering Grub (a woods tile is about a third of a Grub trip's worth for most of its time), so clearing is never a better income than a seep.

### Orders

- **Clear** (workers): drag an area, at most 16 by 16 tiles. The worker clears its overgrown tiles one at a time, the nearest first, and stops when none is left. Several workers sent to the same area share it.
- **Build on overgrown ground**: allowed. The worker pays when ordered, clears the footprint, then lays the foundation and builds as usual. Until the foundation is laid, any other order calls it off with everything refunded. If the ground is taken meanwhile (a building or a regiment there), or part of it can't be reached, the price comes back and the worker stops. Ground paid for and still being cleared is spoken for: nobody else may build on it.
- The placing preview shows overgrown tiles amber (they will be cleared first) and blocked tiles red.

### What a cleared tile becomes

Open ground. Nobody hides there any more, troops no longer slow down in it, arrows are no longer caught by its trees, and cleared thicket can be walked through. The tile keeps its habitat (`Map.habitat`), so the ground thread (T8) draws a felled old-growth tile as a glade with stumps and loam and cut scrub as rough grass. The loam ([35](35-ground-marks.md)) and the stumps ([25](25-scenery-models.md#stumps)) are built. Ambient life (T11, [34](34-ambient-life.md)) draws the workers felling and grubbing: workers swing at the tile, its trees tip over away from them and are hauled off, and bushes are torn up.

### Faction twists

Mario chose all four as proposed (2026-10-24). **Built (2026-10-24):** each Conquest unit carries a `ClearStyle` (`gc_sim::clearing`): a pace for scrub, woods and thicket (100 is the standard time, 50 twice as fast), the Grub each pays, and whether it burns. The game sets it from the faction (`gc_game::conquest::clear_style`); the sim knows no factions. Workers of different styles can share one area; each works at its own pace and the tile pays what the worker who finishes it pays.

| Faction | How they clear | Pace (scrub, woods, thicket) | Grub (scrub, woods, thicket) |
|---|---|---|---|
| Standard | fell and grub out | 100, 100, 100 | 0, 3, 5 |
| **Ember Clans** | fell and haul: more timber | 100, 100, 100 | 0, 5, 8 |
| **Carrion Concord** | burn it: twice as fast, no timber, leaves scorched ground (`Event::Cleared { burnt: true }`) that is open like any other | 50, 50, 50 | 0, 0, 0 |
| **Choir** | shatter it with resonance: thicket as fast as woods, no timber | 100, 100, 53 | 0, 0, 0 |
| **Brood** | Gnawers eat it: slower, but even scrub feeds them | 150, 150, 150 | 1, 3, 5 |

Burning is a look only, reported on the event so the ground is drawn scorched ([35](35-ground-marks.md): char with ash drifts, and the stumps of burnt trees charred). It lives only in the event, so a scorched look that must survive a replay seek or a rejoin will need it kept on the terrain too. The style is hashed with each Conquest unit and stored in replays from `FORMAT` 30 (older files read the standard style). `SIM_VERSION` 31.

### Computer players

A computer player prefers open spots for a new building; when only overgrown ground fits, it lays there anyway and its worker clears first. Its picture of the ground (`gc_ai::Terrain`) follows the simulation's: whenever the ground changes, cleared thicket stops being a wall to its armies and cleared woods stop being cover for its ambushes (`Terrain::follow`, checked every tick against `gc_sim::Terrain::changes`, and re-measuring only round the tiles cleared). Preferring open spots costs nothing worth noting: a search for a building spot, measured on a test base, takes about 4 µs on open ground and about 50 µs where nothing fits at all, the most any search can take.

## The one ground change

All runtime changes to the ground go through one place in the simulation, `World::change_ground` (crate-private), given tiles and the growth they become. It:

1. writes the tile's growth and cover (a cleared tile's cover is open),
2. opens or shuts the walkable grid where thicket went or came (thicket stands on its cells like a building does, see `Nav::set_built_all`, so clearing it reuses the same incremental update),
3. folds the change into the ground's running digest, which is part of the state hash,
4. tells the game (`Event::Cleared`), which takes the trees off those tiles and redraws the ground.

Burning woods will become a second growth change (woods to burnt, then open), floods a change of water, and rubble a fallen building's footprint left as rough ground. Those that change how a cell is walked on (floods) add a ground-cost change beside this one (`Nav::set_ground_all`, planned with the nav doors).

## Rules that stay the same

Field Battles have no workers, so nothing is cleared in them; their ground changes only once woods can burn. Golden replays are blessed again with this change, as buildings no longer rise straight out of woods.
