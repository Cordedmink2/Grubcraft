# 32 · Changing land

*Design note, 2026-10-24, restart plan thread T9 ([29](29-restart-plan.md)). Covers idea 22 (clearing land to expand bases) first, and the one way the simulation changes the ground mid-match, which burning woods (idea 2), floods (idea 6) and rubble reuse later. Terrain tactics (idea 13) is built below; weather, night and seasons (ideas 1, 9, 7) get their own sections here as they are built.*

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

## Reading the ground (idea 13)

**Built (2026-10-24), stage 1.** The ground now matters in more ways, and each one can be seen.

- **Climbing.** A regiment marching uphill loses half its pace for each tile of height it climbs per tile of ground (`CLIMB_SLOW_PERCENT` 50), at most 40% (`CLIMB_SLOW_MAX`). It looks one tile ahead along its march's main axis (`Terrain::climb_percent`, so diagonal climbs count as straight ones do), so a column slows as it starts up a slope. Going down is no faster, which keeps the crowd check's longest step true.
- **Charges and slopes.** A charge that hits home from high ground (half a tile or more above its target, as for fighting downhill) lands with half its charge bonus again (`CHARGE_DOWNHILL_PERCENT` 150). One that hits home up onto high ground keeps half (`CHARGE_UPHILL_PERCENT` 50) and never ploughs into the ranks. The same scaling applies to the weight braced spears turn back on a charger. Holding a hill against cavalry now pays.
- **Mud.** Each tile is firm or soft (`gc_sim::Footing`, the new `Ground::footing` layer). Soft ground is peat bog and alder carr (wet woodland) on a grown map, about a tenth of it, and fen on a painted one (`Map::settle_footing`). Troops in it march at 75% pace (heavy troops 60%), on top of any woods. Nobody starts a charge in mud or water, and nobody charges a regiment standing in either (`Terrain::bears_charge`): horses bog down. A run-up is only built on ground that bears a charge, so cavalry leaving a bog needs a fresh run-up. Charging into a ford was allowed before; it no longer is.
- **Seeing it.** The selection panel says what the ground under the regiment does: "In a ford: slow, no charges", "In mud: slow, no charges", "In woods: slower" or "On a slope: slower uphill" (shown for a group only when every regiment in it shares it). On the ground, bog and carr are fen (reeds and pools); sedge is fen too but firm, so the ground thread (T8) is asked to draw soft tiles visibly muddier than the rest of the fen. The Quick Start's Command step and the Cavalry class say to charge downhill and keep out of mud.

The footing layer is stored in replays from `FORMAT` 33 (older files read firm ground) and is part of the online map check. `SIM_VERSION` 34.

**Later:**
- Rain makes ground soft (idea 1). The footing layer is where it will write.
- Computer players don't yet weigh mud or slopes. Their routes and charges ignore both, so they will sometimes charge into a bog.
- Roads (+15%) and loose scree come with more footing kinds.

## Weather and the time of day (ideas 1 and 9)

*Plan, 2026-10-24. The simulation owns the sky's state and what it does in play; the cinematic lighting thread (T3, [30](30-cinematic-lighting.md)) owns every picture of it, the sun's direction included.*

### The shared state

The sim keeps one small state, deterministic and part of the state hash, and the game reads it each frame:

- `world.weather()` returns `Weather { kind, strength, cloud, wind }`. `kind` is `WeatherKind::{Clear, Rain, Snow, Fog}`, in the same order as `gc_render::Weather`. `strength` (0 to 255) is how hard it rains, snows or how thick the fog is; the renderer's amount is `strength / 255`. `cloud` (0 to 255) is cloud cover, so an overcast day is clear weather with heavy cloud. `wind` (two `i8`s, tiles per second times 16) is for particles, smoke and banners.
- `world.daylight()` returns `Daylight { tick_of_day, day_ticks }`. `light()` (0 to 1000, smooth from tick to tick) is 0 through the night, rises through dawn, is 1000 through the day and falls through dusk. `rising()` says which half of the day it is (dawn or dusk), so the look can blend Night, Dusk, Golden hour and Noon. `day_ticks` 0 means the clock stands still.

Both change smoothly: a new spell of weather fades the old one's strength to nothing, then grows its own, one step every two ticks (about 25 seconds end to end), and cloud eases the same way.

### Where the sky comes from

`MatchSetup::sky` is a `Sky { start, day_ticks, spells }`: the time of day at the first tick, the day's length, and a list of spells (`Spell { at, kind, strength, cloud, wind }`, each starting at a tick). The game builds it from the lobby's **Time of day** (where the clock starts; the day then passes slowly, one game day in 40 minutes) and a new **Weather** option (Fair, Changing, Rain, Snow, Fog). Changing draws a spell every three to six minutes from the match seed. Every peer builds the same sky from the lobby settings, replays store it, and a Conquest match uses fair, changing weather.

### What it does in play (stage 1)

- **How far troops see.** A visibility percent comes from the light (100 by day, down to 45 in the dark of night) times the weather (fog takes up to 60%, snow 30%, rain 20%, each scaled by strength). In fog of war, regiments' and buildings' sight is cut by it, and archers' range is cut by it everywhere: rain and fog let a line close in under fewer volleys.
- **Snow** slows marching by up to 15%, like light mud on all ground.
- The selection panel and the top bar say what the sky is doing ("Fog: archers half range").

### Later stages

- **Rain soaks the ground (stage 2).** Long, heavy rain turns low, flat, open tiles beside water soft (it writes the footing layer through `World::change_ground`), and a dry spell firms them up again, so a bog spreads in a wet battle. The ground thread draws them muddy.
- **Night battles, sight tied to light (stage 3, idea 9).** Regiments can carry torches: they see further at night, and are seen from much further. Burning buildings and fires light the ground around them. Ambushes are easier in the dark. The lighting thread draws torches and firelight from the same state.
- **Lightning (later).** `Event::Lightning { at, tick }` for a flash and thunder; a strike may set woods alight once woods burn (idea 2).

## The one ground change

All runtime changes to the ground go through one place in the simulation, `World::change_ground` (crate-private), given tiles and the growth they become. It:

1. writes the tile's growth and cover (a cleared tile's cover is open),
2. opens or shuts the walkable grid where thicket went or came (thicket stands on its cells like a building does, see `Nav::set_built_all`, so clearing it reuses the same incremental update),
3. folds the change into the ground's running digest, which is part of the state hash,
4. tells the game (`Event::Cleared`), which takes the trees off those tiles and redraws the ground.

Burning woods will become a second growth change (woods to burnt, then open), floods a change of water, and rubble a fallen building's footprint left as rough ground. Those that change how a cell is walked on (floods) add a ground-cost change beside this one (`Nav::set_ground_all`, planned with the nav doors).

## Rules that stay the same

Field Battles have no workers, so nothing is cleared in them; their ground changes only once woods can burn. Golden replays are blessed again with this change, as buildings no longer rise straight out of woods.
