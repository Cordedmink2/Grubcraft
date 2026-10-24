# 34 · Ambient life and busy bases

*Plan, 2026-10-23, restart plan thread T11 ([29](29-restart-plan.md)). Stage 1, clearing land on screen, is built (see [Stage 1 as built](#stage-1-as-built)); the rest is not yet. Covers ideas 3 (wildlife and villagers) and 10 (busy living bases), idea 22 on screen ([32](32-changing-land.md)), and the living touches of [27](27-living-land.md#living-touches-later-polish-render-only).*

## What this is for

Mario wants a world that looks lived in: a base where people are doing things, land with creatures on it, and clearing woods that you can watch happen. None of it changes a match. It is all picture: it reads the simulation's world and events, and nothing it does feeds back into the simulation, replays or the network. That keeps the golden replays green and means each machine may draw its own random birds.

## Rules every stage keeps

- **Presentation only.** The only simulation change is read-only getters (`World::clearing_tile_of`). Anything that would change a match (animals that block movement, villagers that can be killed) is out of scope here and would be a sim stage of its own.
- **One module, small hooks.** The game side lives in `gc_game/src/ambient/` (one file per kind of life); `main.rs` gets one-line hooks. Particle recipes go in `gc_render/src/effects/ambient.rs`, not the shared `effects.rs`.
- **Unseen is not drawn.** Effects of what the viewer cannot see (fog of war) are not made, the same rule as the dust ([30](30-cinematic-lighting.md)).
- **Cheap.** Creatures are few and near the camera; a settled screen costs nothing. The Graphics preset's effects density thins every recipe. Budgets are measured, not guessed (see each stage).
- **No second animation system.** Anything that needs a new skeletal clip waits for the animation engine (T5, doc 33) and uses it. Until then, workers use the clips they have (the builders' swing).
- **Models by Sonnet 5.5 alone** (Mario, 2026-10-23); reviews on Opus.

## The stages

| Stage | What | Needs |
|---|---|---|
| 1 | **Clearing land on screen** (idea 22). Workers swing at the tile they clear, throwing chips or leaves. When the tile is cleared its trees tip over away from the cutter, dust and leaves fly where the crown lands, the trunks lie a while and are hauled off, and bushes are torn up. | Particles (done), scenery lean (this stage) |
| 2 | **Living touches without models** (built, except mist). Fireflies at dusk over fens and wet woods, pollen in meadows, dust devils in the waste, leaves falling in old woods. Mist pooling on low ground is left to the weather stage of the lighting thread. Particle recipes on a habitat sample of the ground round the camera, as the sound beds do ([09](09-art-and-audio.md)). | Particles, `Map.habitat` (done), lighting looks (done) |
| 3 | **Busy bases** (idea 10; hearth smoke, lit windows and faction glows built as 3a, walkers between buildings as 3b, the rest not yet). Chimney and cook-fire smoke from finished buildings, lit windows and torches at dusk, sparks from smithies, workers walking between a base's buildings, log piles and stacked supplies beside the halls, carts and porters on the road from a seep to its granary. The faction decides what it is: Ember Clans' hearth smoke and lanterns, Concord bone carts and green fire, Choir resonance motes, Brood pulses and tending larvae. | Stage 1, building models' anchor points, the animation engine for a proper carrying walk |
| 4 | **Wildlife** (idea 3). Flocks that lift from woods when an army marches by and settle again, carrion birds over a battlefield's dead, moths round Grub seeps, butterflies in flower meadows, grazers that bolt from soldiers, fish rises and dragonflies over water. Each land has its own fauna. Creatures are a picture: soldiers walk through them. | Animal models and clips on the animation engine (T5) |
| 5 | **Villagers** (idea 3). Neutral hamlets and towns get people: porters, farmers, children, dogs, who go about their day, stop to look at an army, and shut their doors when a fight starts. Faction bases borrow a few of their worker models as non-selectable helpers. | Stage 4's creature system, worker models |

Stages 2 and 3 can interleave; stage 4 waits for the animation engine's model format, so the animal models are baked once.

## Stage 1 as built

*2026-10-23. Render and game only; one read-only getter in `gc_sim`.*

### What a player sees

- A worker ordered to clear (or to build on overgrown ground) walks up to the tile and swings at it with the builders' swing, facing it, throwing a few chips of pale wood from a tree, or leaves and torn turf from scrub, about one throw every 0.6 seconds from each soldier at the tile.
- When the tile is cleared, each tree on it tips over, slowly and then fast, **away from the worker**, in about 1.7 seconds. Where the crown lands it raises a ring of dust and a shower of leaves, and a few splinters. Several trees on one tile go over a moment apart. The felled tree lies for 14 seconds, then is carried off over 2.5 seconds (it shrinks out of sight). Bushes are torn up in 0.6 seconds with a puff of leaves and clods.
- Nothing else changes: cleared ground, timber and orders work as in [32](32-changing-land.md).

### How it works

- **The simulation's getter.** `World::clearing_tile_of(handle)` gives the tile a worker has on its order and work has begun on (the tile is in the clearing's part-worked list), unless the worker is fighting; the game also checks the soldier is up at the tile. It reads `Clearing` and changes nothing; nothing is hashed or sent.
- **Scenery lean.** `SceneryInstance::kind` carries, above its surface-detail byte, how far the model is tipped over about its base (0 upright, 255 lying) and, in the next byte, the direction it falls as a turn from the way the model faces; the tree keeps its own facing and wind phase, so nothing snaps when it starts to go. The scenery shader (`scenery.wgsl`, `tip_of`, `tipped`) rotates the vertices and normals (the detail pass too), stops the wind moving a lying tree, and rests a crown on the ground under it rather than in it. The shadow pass uses the same placement, so the shadow falls with the tree. The detail textures still stick to the model's own space. `Extra` (the game's per-frame extra models) has a `lean`.
- **What stood there.** `Scenery::standing(tile)` returns the trees and bushes on a tile that are not yet felled and not under a building. `fell_scenery` asks it as the simulation's `Event::Cleared` arrives, hands the list to `ambient::Felling::clear` and only then marks the tile felled. The felled trees go on as extras of the same model, scale and tint they had, tipping, so nothing pops.
- **Which way it falls.** The soldier pass tells `Felling::cutting(tile, position)` where each working soldier stands; a tile with a recent cutter has its trees fall away from him, give or take a few degrees. With no one seen (a replay jumped into, or a worker lost), a tree falls the way it was already turned.
- **Particles.** `gc_render::effects::ambient` adds `leaves` and `cut_chips` recipes; `gc_game::effects::Field` adds `chop`, `tree_down` and `bush_up`. They follow the effects clock (still while paused, slow in slow motion) and thin with the Graphics preset.
- **Who sees it.** Trees come down only where the viewer can see (fog of war) and not while a rejoining player catches up on ticks long past; the tile is felled either way. A tree lying where a building has since been laid is dropped.
- **Limits.** At most 512 trees and bushes are down at once; a new one pushes out the oldest. The felled tree is drawn as an extra, so it counts against the extras limit of 4,096 that the gathering spots and morale banners share. On a slope a lying crown rests on the ground under it, but the dust where it lands is put at the height there too, which can differ a little from the crown.

### Not yet

- A log pile where a tree stood (log piles come with stage 3). The stump is built: the ground thread leaves one where each felled tree stood ([25](25-scenery-models.md#stumps)), and a hauled tree still shrinks away.
- A proper axe clip (the animation engine), and the thud and crack of a tree (the soundscape thread's hit sounds).
- Faction twists ([32](32-changing-land.md#faction-twists)): burning, shattering and eating are built by the changing-land thread; their pictures (fire on a burning tree, crystal shards, Gnawers chewing) join this stage once they exist.

## Stage 2 as built

*2026-10-23. Game and render only; nothing in the simulation.*

### What a player sees

- **Pollen** drifts up from flower meadows, grassland, glades and heather by day, fading out as the sun sinks.
- **Fireflies** come out as the light goes (the Dusk look): slow green-gold sparks that rise and fade over wet meadows, sedge, alder carr, bog, fern hollows and glades.
- **Leaves** fall from old growth, birch fringes (gold), tree lines and fern hollows, at any hour.
- **A whirlwind** now and then wanders the waste (cracked flats, bone drifts, dying grass) for about 14 seconds, throwing up a spreading column of dust, and dies out when it leaves the waste.
- Only near the camera (they fade out between 30 and 70 tiles of camera distance, where they would be under a pixel) and only on ground the viewer can see, so fog of war hides them too. All of it thins with the Graphics preset's effects density and stops with the pause.

### How it works

- `gc_game::ambient::Touches` is called once a frame from the effects block. It tries random points on the ground within a reach of the focus (14 tiles plus 0.9 per tile of camera distance, at most 48; the specks fade out with distance), looks the point's habitat up in `Map.habitat`, and spawns only where that habitat has the touch (`touches_of`). Rates are per tile of area, so a tidy screen costs a few dozen probes a second and keeps no per-creature state; only the whirlwind has any (one at a time). On a ride (the soldier's-eye camera) it follows the same ahead point the light motes do.
- Day and dusk are read from the look's sun elevation (`DUSK_AT` 0.30 over `DUSK_SPAN` 0.12), so a new look preset needs no change here.
- Recipes in `gc_render::effects::ambient` (`pollen`, `firefly`, `dust_devil`, beside `leaves` and `cut_chips`) and wrappers in `gc_game::effects::Field` (`pollen`, `firefly`, `leaf_fall`, `dust_devil`). Its own xorshift generator; the particle system's seed and the simulation's are untouched.

### Not yet

- Mist (weather stage), butterflies and moths (stage 4 creatures), and a habitat for the Brood's land beyond dust (its own touches come with idea 19's veins).

## Stage 3a as built: hearths and lights

*2026-10-24. Game and render only; nothing in the simulation.*

### What a player sees

- **Ember Clans** (and any timber or stone building): a finished hall or outpost smokes from two chimneys, a kitchen or forge from one thicker column, a house from one thin wisp. At dusk one to three lanterns glow warm orange on the outside of the walls, more on bigger buildings.
- **Carrion Concord**: green ghost-lights drift up over each finished building, with green lamps at dusk. **Shard Choir**: cyan resonance motes and glowing windows that shine a little even by day and brighten at dusk. **Brood**: pale spores breathing out of each, with pale lamps at dusk.
- Only finished, undamaged buildings that the viewer can see (a building under 70 % hit points smokes the siege way instead, [doc 26](26-fortifications.md)); not walls or towers; not in the menu or the unit viewer. The smoke shows out to 110 tiles of camera distance (past 60 only halls and outposts make any, so a crowded town cannot flood the particles); lamps fade out between 35 and 75, where they would be under a pixel.

### How it works

- `gc_game::ambient::bases::emit(look, dt, dusk, distance, fx)` is called each frame for each shown building from the effects block in `main.rs`, using the siege module's `Look` (the building as last drawn: footprint, ground, height, faction material), which now also carries the building's `role` and whether it is `finished`. Positions of chimneys and windows come from a hash of the building's handle, so they stay put frame to frame without any stored state.
- `gc_render::effects::ambient::lamp` is a steady additive glow that breathes; smoke is the existing `smoke` recipe made thin and pale. `dusk` is `touches::dusk_of(sun)`, the same dusk the fireflies use, from the look's sun elevation.
- Cost: a few dozen particles per visible building at most; thinned by the Graphics preset like all effects.

### Not yet

- Log piles and stacked supplies, carts on the road from a seep to its granary, porters with a proper carrying walk (animation engine stages 3 to 5), forge sparks, and window light that really is a model glow (the lights are particles hung just outside the walls for now, not in the windows' own frames).

## Stage 3b as built: walkers between buildings

*2026-10-24. Game only; nothing in the simulation, and no new model.*

### What a player sees

- Each finished, visible hall, outpost, workplace, barracks, tech building or heroes' building has one worker of its faction who now and then walks to another building of the same owner within 16 tiles, stands at its door for four to thirteen seconds, and walks home. They use the faction's own worker model with the ordinary walk and idle clips, in the owner's colour.
- They are a picture. They cannot be selected or hurt, cost no supply, block nobody, and are not in the replay or the network. A trip, out or home, is only taken along a straight way that stays on dry ground, off steep slopes, out of tiles with trees (rocks are not checked; they mostly stand on slopes), and a quarter tile clear of every building and wall; otherwise the worker stays put and tries again later. A building that goes up in a walker's way, or against his door, makes him vanish rather than walk through it (he comes back at his door once it is clear).
- Only buildings the viewer sees, on screen and within 60 tiles of the camera focus, have walkers. A walker whose home goes out of view or is destroyed is forgotten at once, so an unwatched base costs nothing; at most 160 walk at once. A door with a building against it gets no walker.

### How it works

- `gc_game::ambient::walkers::Walkers::advance(dt, homes, walkable, climb)` is called from the effects block each frame. `homes` is built by `Running::walker_homes` from the siege module's `Look` and the structure views: footprint, door (half a tile past the wall, the way the building faces), role, owner and the faction's worker model (`buildings::Looks::worker_of`). Walls, towers and buildings still going up are in the list as obstacles only. A walker's id is its home's handle, so a rebuilt building starts afresh.
- Straight-line paths only, checked every 0.4 tile when a trip is picked: no pathfinding, so nothing is searched and nothing can get stuck. `push_building_models` draws each walker as a `SoldierInstance`.
- Own xorshift generator, floats and frame time, as the rest of the ambient code.

### Not yet

- Porters with loads (they need the animation engine's carry), walkers that stop to talk or take a different route round a building, and wildlife and villagers (stages 4 and 5, which need models).

## Faction clearing pictures

*2026-10-24. Game and render only; the sim's twists are [32](32-changing-land.md).*

### What a player sees

Clearing looks like the faction that does it (`ambient::felling::Style`, from the worker regiment's faction; a tile the sim reports `burnt` always burns):

| Faction | At the tile while working | When the tile is cleared |
|---|---|---|
| Ember Clans (and standard) | chips and leaves, as before | trees tip over away from the cutters, lie, are hauled off |
| Carrion Concord | a low fire with smoke and embers on the tile | trees stay upright, char black in 1.2 s with flames, smoke and embers, then crumble to ash (3.6 s in all); bushes flare out in 1.2 s |
| Choir | glints of crystal and cold light motes | trees go pale and glassy for 0.9 s, then burst into shards and light (bushes in half that) |
| Brood | pale crumbs and spores thrown from the tool | trees shiver and sink as they are eaten, shedding spores, in 4.5 s (bushes 1.6 s); no trunk is left to haul |

### How it works

- `Felling::cutting(tile, from, style)` records the style of the regiment seen at a tile; `Felling::clear(tile, burnt, ...)` picks it up (a burnt tile is `Burn` whoever cleared it; with no cutter registered it is `Fell`). Every clearing regiment is registered each frame, in Commander View and out of the viewer's sight too, so the style is never lost; a regiment the viewer cannot see gives no direction, so trees fall as they stood and the picture gives nothing away. A cutter is kept 1.5 s after it was last registered. Each `Down` keeps its style; `pose` and `tint` give the lean, scale and colour by age, and `advance` makes the effects: `Field::blaze`, `ash_down`, `shatter`, `gnawed`, and the old `tree_down` and `bush_up` for the felled. `Field::chop` takes the style for what workers do at the tile.
- At most 48 burning trees get flames in a frame; the rest only char, so a burnt wood cannot flood the particles. Nothing is added to the sim, the replay or the network.
- `--fell-demo` now clears the four corners of the wood each faction's way (fell, burn, shatter, gnaw) so they can be looked at side by side.

### Not yet

- Burnt trunks lying instead of crumbling, and the Ember Clans' hauling carts. (The scorched ground and charred stumps a burnt tile leaves are the ground thread's, [35](35-ground-marks.md).)
