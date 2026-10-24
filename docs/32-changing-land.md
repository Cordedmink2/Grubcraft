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

Burning is a look only, reported on the event so the ground is drawn scorched ([35](35-ground-marks.md): char with ash drifts, and the stumps of burnt trees charred). It lives only in the event, so a scorched look that must survive a replay seek or a rejoin will need it kept on the terrain too. The style is hashed with each Conquest unit and stored in replays from `FORMAT` 30 (older files read the standard style). `SIM_VERSION` 31. The pictures of the four ways of clearing (workers' fire, charring trees, shattering crystal, gnawed trees) are built: [34 · Faction clearing pictures](34-ambient-life.md#faction-clearing-pictures); the tipping, lying and hauling described above is the Ember Clans' (and the standard) way.

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
- Computer players keep their battle lines out of mud (since the weather's stage 3a, below) but don't yet weigh slopes, and their routes and charges ignore both, so they will sometimes charge into a bog.
- Roads (+15%) and loose scree come with more footing kinds.

## Weather and the time of day (ideas 1 and 9)

*The simulation owns the sky's state and what it does in play; the cinematic lighting thread (T3, [30](30-cinematic-lighting.md)) owns every picture of it, the sun's direction included.*

**Built (2026-10-24), stage 1:** the sky's state, where it comes from, and its effect on sight, range and the march. `SIM_VERSION` 37, replay `FORMAT` 35 (stores the sky; older files read a fair afternoon whose clock stands still), net `PROTOCOL` 28 (the lobby's weather).

### The shared state

The sim keeps one small state (`gc_sim::sky`), deterministic and part of the state hash (under "clock"), and the game reads it each frame:

- `world.weather()` returns `Weather { kind, strength, cloud, wind }`. `kind` is `WeatherKind::{Clear, Rain, Snow, Fog}`, in the same order as `gc_render::Weather`. `strength` (0 to 255) is how hard it rains, snows or how thick the fog is; the renderer's amount is `strength / 255`. `cloud` (0 to 255) is cloud cover, so an overcast day is clear weather with heavy cloud. `wind` (two `i8`s, sixteenths of a tile a second) is for particles, smoke and banners.
- `world.daylight()` returns `Daylight { tick_of_day, day_ticks }`. `light()` (0 to 1000, smooth from tick to tick) is 0 through the night, rises through dawn (about 4:50 to 7:10), is 1000 through the day and falls through dusk (about 16:50 to 19:10). `rising()` says which half of the day it is (dawn or dusk), so the look can blend Night, Dusk, Golden hour and Noon.
- `world.air()` returns the `Air`: what the sky does in play now (below).

Both change smoothly: a new spell of weather first fades the old one's strength to nothing, then grows its own, one step every two ticks (`EASE_EVERY`, about 25 seconds end to end); cloud and wind ease the same way. The spell in force at tick 0 is there from the start.

### Where the sky comes from

`MatchSetup::sky` is a `Sky { start, day_ticks, runs, spells }`: the tick of the day at the first tick, the day's length (`DAY_TICKS`, eight hours of play, so an hour of the day takes twenty minutes), whether the clock moves, and a list of spells (`Spell { at, weather }`, each starting at a tick). `Sky::STILL` (a fair 17:00 whose clock stands still) is what tests, the sandbox and old replays have, and it hashes as nothing, so they play as before.

A lobby match builds its sky from the lobby (`gc_match::battle::sky_of`): the **Time of day** sets where the clock starts (Golden hour 17:00, Noon and Overcast 12:00, Dusk 18:00, and the new **Night** 23:00) and the day then passes, so a long dusk battle ends in the dark. The new **Weather** stepper, beside the time of day (on the map-code row, or beside it on a short window) (Fair, Changing, Rain, Snow, Fog), sets the spells: one for the whole match, or for Changing a new spell every three to six minutes drawn from the match seed (`sky::changing`; rain, fog or clear sky, a little over a third clear). Overcast is fair weather under heavy cloud, and keeps the cloud heavy under any weather. Every peer builds the same sky from the lobby settings, and replays store it.

### What it does in play (stage 1)

- **How far troops see and shoot.** The `Air`'s sight percent is the light's share times the weather's. Full light lasts until the light falls under half (so golden hour sees as far as noon), then sight falls to 45% in the dark of night (`NIGHT_SIGHT_PERCENT`). Fog takes up to 60% (`FOG_SIGHT_LOSS`), snow 30%, rain 20%, each scaled by strength. It moves in steps of 5%. Archers' and towers' range is cut by it everywhere: archers reaching 25 tiles reach about 21 in the lobby's rain, 11 in its fog and 11 at night, so a line can close in under fewer volleys. In fog of war, regiments' and buildings' sight is cut too.
- **Snow** slows marching by up to 15% (`SNOW_MARCH_LOSS`), on top of the ground.
- **Seeing it.** The top bar shows a chip whenever the sky cuts anything, e.g. "Fog: sight 40%", "Night: sight 45%" or "Snow: sight 70%, march 85%". The Ranged class in the Guide says fog, rain and night cut their reach.

Computer players plan for the shorter range and slower march (stage 3a, below).

### Rain soaks the ground (stage 2)

**Built (2026-10-24):** `SIM_VERSION` 39 (38 went to blueprints); no new replay data or protocol (the soaking follows from the sky and the map).

- **How wet the ground is.** The sky keeps one number, 0 (dry) to `WET_FULL` (1000). Once a second rain adds its strength / 64 (`RAIN_WETS`: about 3 in heavy rain, so some five minutes to soak it through), and anything else, drizzle under 64 included, takes 2 off (`DRIES`). Every match starts dry. It is hashed with the sky.
- **Which ground soaks.** When the match starts, `Terrain` lists the tiles rain can soak: firm, not wooded, nothing growing but scrub, at most half a tile above the water line (`SOAK_RISE`), no land neighbour more than half a tile higher or lower (`SOAK_SLOPE`), and at most 5 tiles from water (`SOAK_REACH`). Each gets the wetness it needs: 150, plus 150 a tile from the water, plus up to 300 for its height. Banks right by the water go first (about a minute and a half of heavy rain), ground further off later. Grown land (the lobby's default) has a few hundred to a thousand such tiles on a 192-tile map; the steeper Random and Classic maps have a few dozen or none.
- **Soaking and drying.** Each tick, when the wetness has passed a tile's need, the tile turns soft (`Terrain::soak`, which only moves a count: `Terrain::footing` reads a tile as soaked when it is no later in line than the last one soaked), the soonest first: slow going and no charges, as any soft ground. When the ground dries 100 (`SOAK_DRY_MARGIN`) below a soaked tile's need, it firms up again, the last soaked first, so tiles don't flicker as rain eases. The soaked tiles are a run of that list, so their count (`Terrain::soaked`, hashed under "ground" when above 0) is the whole state.
- **Seeing it.** The renderer draws soaked tiles with the soft ground's mud (the scar grid's spare byte, [35](35-ground-marks.md)); the top bar says "Mud by the water" (or adds ", mud" to the weather's words) while any is soaked, and the selection panel says "In mud" as on any soft ground. The Cavalry guide blurb mentions it. Computer players see the mud (stage 3a, below).

### Computer players read the sky (stage 3a)

**Built (2026-10-24):** no sim change (the minds sit outside it).

- **Range and march.** `Perception` reads the units under the world's `Air` (`UnitInfo::in_air`): archers' range is what fog, rain and night leave, and the march is what snow leaves. Everything that reads a unit's range (the estimator's who-outshoots-whom, when a line is under fire, how far archers stand off, the bank a line waits behind) now plans with the shorter reach, and closes in under fewer volleys. The table is rebuilt when the air changes (in steps of 5%), and the Conquest battle views the Marshal hands its Captains carry it too.
- **Mud.** The minds' own ground map (`gc_ai::Terrain`) reads which tiles are soft underfoot (bog, and mud the rain has soaked, `Terrain::soft`) at the first look and again whenever the soaked count changes, and `GroundLine::footing` counts mud as bad footing as it counts water, so the Captain doesn't put a battle line in a bog or on a soaked bank. Routes and charges still ignore it.
- **Towers.** The Spymaster's sums use Conquest towers' range under the same air, as the sim shoots them.

### Torches and night battles (stage 3b, idea 9)

**Built (2026-10-24):** SIM 41, replay FORMAT 38, net PROTOCOL 31. The plan was checked by an Opus review, whose fixes are folded in: marching troops are seen further, there are no double ambushes, torches guard against ambush, and troops hidden by the dark still count as on the field. At night the dark hides troops as woods do, and torches give them away. It runs through [20](20-ambushes.md)'s spotting (`gc_sim::vision`).

- **The dark.** `Air::dark`: the night's share of sight (before weather) is at most `DARK_PERCENT` (70%). That is from late dusk until early dawn. A Dusk 18:00 match gets dark some nine minutes in, and a Night 23:00 one is dark from the start. `Air::lit_percent` is sight as the weather leaves it, without the night's cut. Earlier in dusk only sight and range shrink, as before.
- **Hiding in the dark.** In the dark, a regiment in the open that has no torches, isn't fighting and hasn't shot lately is spotted only within `DARK_STILL` (6 tiles, edge to edge) standing, or `DARK_MOVING` (12) marching. In a wood or tall grass, the closer of the cover's distance and the dark's applies. A regiment placed on the field is in plain view as it is placed, so troops raised at night are no surprise.
- **Hidden by the dark is not hiding.** `Vision::hidden` stays cover's, so champions and the verdict count a regiment lost in the dark as on the field. Two armies that can't see each other in the dark still face each other (`Vision::faces`), so nobody holds the field until one is beaten. An army is heard in the dark: an attack order may name an enemy hidden only by the dark, and a chase after one goes on (`World::faces`). Archers ordered against one close in until they see it. In fog of war it is heard only within the side's sight. An archer hidden only by the dark doesn't hold its fire, as one hiding in a wood does.
- **Torches.** `Command::Torches { regiment, lit }` lights a regiment's torches or puts them out (`Regiments::torches`, hashed with the regiment). Any of the issuer's regiments may be ordered, at any time, deploying too. Torches burn only in the dark: by day the order is kept but does nothing. A regiment with lit torches in the dark:
  - is never hidden, by the dark or by cover;
  - lights the ground within `TORCH_LIGHT` (10 tiles, edge to edge). Any regiment there, friend or foe, is lit, and a lit one standing in the open is seen by everyone. So a torch-lit line sees 10 tiles where a dark one sees 6.
  - can't be ambushed out of the dark: it sees what comes at it. (A charge out of a wood still takes it by surprise.)
  Who is lit is worked out once a tick, from each torch-bearer (`Vision::light`). It is hashed, because the next tick's archers and towers read it.
- **Seen and shot from further.** Archers and towers shoot a lit regiment at the weather's range, without the night's cut (`Vision::reach_at`), and a dark one at the night's range, as before. Their search reaches the longer of the two. In fog of war a lit regiment is in sight as far as by day (`lit_sight` of `SIGHT`), from regiments and buildings alike.
- **Ambushes in the dark.** A charge or volley out of the dark (from a regiment hidden by the dark alone) springs an ambush as one out of a wood does, with two exceptions. The victim mustn't carry torches. And if the victim's side has only just seen the attacker too, both stumbled on each other and neither springs. So an ambush in the dark is for the patient: wait unseen until the enemy has been in view a while (`SURPRISE_TICKS`), then strike, or strike troops lit by a friend's torches.
- **Computer players.** Once a second in the dark, each mind lights the torches of its melee regiments that are fighting, and puts out every other's. The rest march dark. A mind sees only what its side sees, plus enemies hidden only by the dark: it hears those, and knows where they were when last heard, every 10 seconds (`HEARD_TICKS`). Rehearsal worlds carry torches (`Standing::torches`). **Not yet:** the minds' own sums of reach (`UnitInfo::range`) use the night's range even against lit targets, and they don't yet use the dark to lay ambushes of their own.
- **Seeing it.** The battle grid's Torches toggle (key X, the old Charge slot; charges need no order) lights the selection's torches, or puts them out once they all burn. The selection panel says "Torches lit: seen from afar" or "In the dark: unseen from afar", and the top bar says "Dark", or "Rain in the dark". The Ranged guide blurb mentions it. The lighting thread draws the torches and their light from `RegimentView::torches` and `World::air().dark`.
- **Golden.** `field-night-rain-1v1` is a night battle in heavy rain on the woods map: the armies close by sound, torches light up as the lines meet, and 16 ambushes are sprung.

### Later stages

- **Firelight (with burnable woods, idea 2).** Burning woods and buildings light the ground round them as torches do (stage 3b, above). The sim does not track fires yet: a damaged building's flames are only drawn.
- Snow on the Highland's changing weather comes with the lands' own skies.
- **Lightning (later).** `Event::Lightning { at, tick }` for a flash and thunder; a strike may set woods alight once woods burn (idea 2).

## The one ground change

All runtime changes to the ground go through one place in the simulation, `World::change_ground` (crate-private), given tiles and the growth they become. It:

1. writes the tile's growth and cover (a cleared tile's cover is open),
2. opens or shuts the walkable grid where thicket went or came (thicket stands on its cells like a building does, see `Nav::set_built_all`, so clearing it reuses the same incremental update),
3. folds the change into the ground's running digest, which is part of the state hash,
4. tells the game (`Event::Cleared`), which takes the trees off those tiles and redraws the ground.

Burning woods will become a second growth change (woods to burnt, then open), floods a change of water, and rubble a fallen building's footprint left as rough ground. Those that change how a cell is walked on (floods) add a ground-cost change beside this one (`Nav::set_ground_all`, planned with the nav doors). Rain soaking (stage 2 of the weather, above) is the one change outside it: it only turns footing soft and back, never growth, cover or the walkable grid, so it is `Terrain::soak`, kept as a count beside the digest.

## Rules that stay the same

Field Battles have no workers, so nothing is cleared in them; their ground changes only once woods can burn. Golden replays are blessed again with this change, as buildings no longer rise straight out of woods.
