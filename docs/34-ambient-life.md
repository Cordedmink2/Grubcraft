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
| 3 | **Busy bases** (idea 10; hearth smoke, lit windows and faction glows built as 3a, walkers between buildings as 3b, log piles beside halls as 3d, the rest not yet). Chimney and cook-fire smoke from finished buildings, lit windows and torches at dusk, sparks from smithies, workers walking between a base's buildings, log piles beside the halls, stacked supplies, carts and porters on the road from a seep to its granary. The faction decides what it is: Ember Clans' hearth smoke and lanterns, Concord bone carts and green fire, Choir resonance motes, Brood pulses and tending larvae. | Stage 1, building models' anchor points, the animation engine for a proper carrying walk |
| 4 | **Wildlife** (idea 3; deer built as 4a and doves as 4b, the rest not yet). Flocks that lift from woods when an army marches by and settle again, carrion birds over a battlefield's dead, moths round Grub seeps, butterflies in flower meadows, grazers that bolt from soldiers, fish rises and dragonflies over water. Each land has its own fauna. Creatures are a picture: soldiers walk through them. | Animal models and clips on the animation engine (T5) |
| 5 | **Villagers** (idea 3; the hamlets themselves built as 5a, the people as 5b: farmers, porters, children and dogs; carts and porters between hamlets built as stage 6b). Neutral hamlets and towns get people: porters, farmers, children, dogs, who go about their day, stop to look at an army, and shut their doors when a fight starts. Faction bases borrow a few of their worker models as non-selectable helpers. | Stage 4's creature system, worker models |

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

- Stacked supplies, carts on the road from a seep to its granary, porters with a proper carrying walk (animation engine stages 3 to 5), forge sparks, and window light that really is a model glow (the lights are particles hung just outside the walls for now, not in the windows' own frames).

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

## Stage 3d as built: log piles beside halls

*2026-10-24. Game and scenery pack only; nothing in the simulation. Two new scenery models ([25](25-scenery-models.md#log-piles)).*

### What a player sees

- A finished Ember Clans (timber) or stone-built main hall has a stack of logs against its back wall and a loose heap against a side wall; an outpost has a heap; about six in ten houses, workshops and farms of those factions have a stack or a heap (which one is the building's own). Bone, crystal and chitin buildings have none, and neither do walls, towers, tech and heroes' buildings or anything still going up.
- A pile stands about two thirds of a tile outside the wall, lying along it, off the wall's middle by up to a fifth of its half-length and a little off square. The stack is on the side away from the door (so the door's way stays clear for the walkers). Each building always shows the same piles: they come from a hash of its handle. A pile is left out where its middle or either end would be on wet ground, in the fog, or within half a tile of any building's walls, or where the ground under it changes by more than 0.15 of a tile (it stands level, so it would hang in the air).
- They are scenery, not things: no one carries logs to them and they do not grow with the wood a player holds.

### How it works

- `gc_game::ambient::piles::extras(look, facing, ground, free)` gives the one or two `scenery::Extra` of a building from its `siege::Look` and the way its door faces; `Running::wood_piles` calls it for the visible buildings within the bushes' reach of the camera (as stumps are) and chains the result onto the frame's extras. Not drawn in the Commander View or the unit viewer. Nothing is stored between frames.
- The extras limit (16,384) is shared with stumps, banners and falling trees; a base has at most two piles a building.

### Not yet

- Stacks that grow with the owner's wood, stacked supplies and sacks for other buildings, carts, and piles for the bone, crystal and chitin factions (which would want their own models).

## Stage 4a as built: deer

*2026-10-24. Game, creature pack and baker only; nothing in the simulation. The first creature, and the pipeline the rest follow.*

### What a player sees

- Small herds of two to five deer graze the open land near the camera, on meadow, forest, highland and valley ground (not fen or waste; a map with no land data counts as meadow). They graze head down, stand with ears up, and now and then the herd walks to new pasture a few tiles away. Deer differ a little in size and are out of step with each other.
- A herd bolts when a soldier the viewer sees comes within 15 tiles, runs until it is 30 tiles from every one (or nine seconds, whichever is first), and then stands alert; it stays alert while any soldier is within 30 tiles, only a soldier within 7 tiles startles it again, and after five quiet seconds it walks back to its pasture. Soldiers walk through deer; they are a picture, not in anyone's way.
- A herd is not made with its home within 14 tiles of a building's middle; its pasture can lie up to nine tiles from home, so deer can graze nearer a building than that, and a building put up beside a herd later does not move it. Grazing spots are dry, gently sloping and free of trees and buildings; a run or a bolt does not look at trees or buildings (only at water and steep ground).
- Herds are made in cells whose middle is within 36 tiles of the camera's focus plus 0.9 of its distance (75 at most), and forgotten again a little beyond that. Not in the Commander View, behind a menu, or in the unit viewer; while the game is paused they stand still.

### How it works

- `gc_game::ambient::wildlife::Wildlife` keeps herds by 20 by 20 tile cell of the map. A hash of the cell says whether it holds a herd (four in ten where the land suits) and where its home is, so coming back to a place finds the same herds in the same spots; cells away from the camera are forgotten and made again at home. Movement is floats on the effects clock with its own random generator; nothing is hashed in the simulation and nothing is saved or sent. The frame's update in `main.rs` passes the visible regiments' positions as threats and the town centres, and a closure says whether a spot is clear of buildings and trees.
- The model is `Deer` in `assets/buildings/creatures.pak`, baked from `assets-src/buildings/creatures.ron` by `gc_artbake` and joined into the model pack by `build.rs` like the building packs. It is made from code in Blender (`design/creatures/source/crit.py` is the small creature kit: a rig of bones with two-bone legs, painted vertex colours baked to an albedo map, clips written as keyframes; `deer.py` is the deer) with four levels of detail (3,000 to 220 triangles) and 1.1 MB in the pack. Clips: Idle, Walk, Run (a bound) and Cheer, which plays the graze (the picker's fall-backs for the fighting moves never reach Cheer, so a creature that only grazes cannot be asked to attack).
- Deer are drawn as `SoldierInstance`s in `push_building_models` with the pose from `art.clips.pose`, standing on the ground by `art.view.place`. The baker no longer insists on tangents for a model without a normal map (the deer has none); a model with one still must have them.

### Not yet

- Butterflies and moths, fish rises and dragonflies, the other lands' fauna, hunted deer leaving a carcass, and herds that react to fights rather than to soldiers alone.

## Stage 4b as built: doves

*2026-10-24. Game and creature pack only; nothing in the simulation. The second creature, made with the same kit as the deer.*

### What a player sees

- Flocks of four to nine doves feed on open ground near the camera (meadow, forest, highland and valley, as the deer do): they stand, turn their heads and strut a few steps. They are bigger than real doves so that a flock reads from the game's camera.
- When a soldier the viewer sees comes within 13 tiles of a flock it bursts into the air, every bird flapping and climbing away from the soldier on its own heading, with its shadow running over the ground. It keeps flying until every bird is out of the camera's reach (twenty seconds at most) and is then gone.
- A flock stays gone at least 20 to 35 seconds, and longer while any soldier is within 28 tiles of its feeding ground. Then it flies back in from beyond the camera's reach on a random side, high up, sinks as it nears the ground and lands where it fed. A soldier coming within 13 tiles of it as it comes in puts it into the air again, from where it is. A flock made while soldiers are within 28 tiles of its feeding ground starts out gone.
- No flock's feeding ground is within 14 tiles of a building's middle (birds stray a few tiles from it), and none feed on tiles with trees or buildings, or on wet ground. Not drawn in the Commander View, behind a menu or in the unit viewer, and a paused game freezes them.

### How it works

- `gc_game::ambient::birds::Birds` keeps flocks by the deer's 20 by 20 tile cell with its own hash salts (three in ten cells where the land suits), made and forgotten by the same reach measure (`wildlife::reach`), so a place keeps its flocks and a zoomed-out camera does not rebuild them. Each flock is in one mood: feeding, fleeing, gone or landing. Floats on the effects clock, own random generator, nothing hashed, saved or sent.
- The `Dove` model (`design/creatures/source/dove.py`, on the `crit.py` kit: a plump grey dove with a green-grey neck, 11 bones, wings of two bones each as one flat ribbon) joins `Deer` in `assets/buildings/creatures.pak` (now 2.1 MB). Clips: Idle, Walk (the strut), Run (not used yet), and Cheer, which is the flap. It is drawn as a soldier instance whose `lift` is the bird's height above the ground, with the flap played 1.7 times faster than keyed; the shadow pass draws it too.
- The threat list, the building filter and the clear-ground test are the deer's, computed once a frame in `main.rs` for both.

### Not yet

- Gliding, soaring in circles, perching in trees or on roofs, birds that wheel over a battle, and other lands' birds (a crow and a vulture on the same kit would also replace the aftermath's box crows).

## Stage 5a as built: Grubfolk hamlets

*2026-10-24. Game and scenery pack only; nothing in the simulation. Before this, a Village, Town or Bazaar was only a clearing in the scenery with a marker on the minimap: there was nothing to see, so villagers (5b) had nowhere to live.*

### What a player sees

- Every Village, Town and Bazaar on a map has its houses: a well in the middle, and round it huts and cottages with their doors toward the well, two haystacks and runs of wattle fence at the edge. A Village has four cottages (two huts and two houses) and a barn; a Town five cottages (two huts and three houses), a barn and three market stalls round its well; a Bazaar a ring of seven market stalls with five cottages outside it and no barn. Where the ground allows: any piece that does not fit is left out. Each place looks the same every time you see it (a hash of the site's tile).
- Things are left out where the ground is wet, is wood or thicket, or slopes by more than 0.45 of a tile under them, and no two of them overlap (trees and bushes at the edges of the clearing may still stand among the fences). A hamlet's models are hidden where a player has built on the ground under them. They are scenery: soldiers walk through them, and they are not in the Commander View.
- The Grubfolk's look is small, round and hard-working: daub and wattle walls, deep straw thatch, a lime-green stitch or lantern at the door ([25 · Grubfolk hamlets](25-scenery-models.md#grubfolk-hamlets)).

### How it works

- `gc_game::ambient::hamlets::Hamlets::new(map)` lays every site out once at the start of a match from `Map.objects` (kind Village, Town or Bazaar) and keeps the lists; `near(focus, reach, occupied)` hands the scenery pass the models of the hamlets near the camera each frame, chained onto the other extras, leaving out any whose footprint touches a tile `occupied` says has a building. The big pieces (the barn) are placed first in each ring so a neighbour cannot squeeze them out. The models are `Model::GrubfolkHut`, `GrubfolkHouse`, `GrubfolkBarn`, `GrubfolkWell`, `Haystack`, `WattleFence` and `MarketStall` in `assets/scenery.pak`.
- `--hamlet-demo` moves the camera to the first hamlet and lifts the fog, so the people show.

### Not yet

- Ruins when a hamlet is raided (the simulation has no raiding yet), a palisade round a Town, chimney smoke and lamp light in the evening, and hamlets that grow with trading.

## Stage 5b as built: the villagers

*2026-10-24. Game and the creature pack only; nothing in the simulation.*

### What a player sees

- Each Grubfolk hamlet has its people while the camera is near it: a Village a farmer, a porter, a child and a dog; a Town two farmers, two porters, two children and a dog; a Bazaar four porters, a farmer, two children and a dog. They are small and round, in patched dun tunics, with a straw hat (the farmer, who carries a hoe), a cap and a sack on the back (the porter) or a bright tunic (the child), and a scruffy tan dog.
- They go about their day between the doors of the cottages, the backs of the market stalls, the sides of the haystacks and, in a Village, the well. A farmer likes the haystacks (he hoes there for a while) and the doors, a porter the stalls (he mops his brow) and the doors, children run about and jump, the dog trots and sniffs. Now and then one goes in at a door and comes out again some seconds later.
- An army the viewer sees within 15 tiles of a villager makes them stop and turn to look at it (not the dog) until it has gone by.
- When a regiment the viewer sees is **fighting** within 40 tiles of a hamlet's middle, everybody out of doors runs to the nearest door and goes in. They stay in until it has been quiet for ten seconds, then come out a few at a time.
- Unseen is not drawn: in the fog of war a hamlet's people do not show. Nothing blocks soldiers and nothing can hurt a villager.

### How it works

- Models by Sonnet 5.5 from `design/creatures/source/grubfolk.py` (`FolkFarmer`, `FolkPorter`, `FolkChild`: 12 bones, about 1,500 to 1,800 triangles, sacks, hats and the hoe riding one bone each) and `dog.py` (`Dog`: 15 bones, 1,500), on the same kit as the deer and doves (`crit.py`), in `assets/buildings/creatures.pak` (now 6.7 MB). Clips: Idle, Walk, Run and Cheer, which is each one's work: the farmer's hoeing, the porter's brow-mopping, the child's jump and wave, the dog's sniffing.
- `gc_game::ambient::villagers::Villagers::advance(dt, focus, distance, hamlets, armies, fights)` makes a hamlet's people when the camera comes within `wildlife::reach` plus a hamlet's spread of its middle (from a hash of the site, so it has the same people each time) and forgets them 14 tiles farther out. `Hamlets::nodes(i)` gives the places to stand, by bearing from the middle (doors, stall backs, haystack sides, the village well); a villager hops from node to node round the ring the short way and a chord that would cut through the well or the ring of stalls bends round it, so there is no path-finding; a hop may still clip the corner of a cottage where the ground left a wide gap in the ring. `armies` are the regiments the viewer sees and `fights` those of them with `engaged` set, both gathered once in `main.rs` with the deer's threats. A hamlet's people are made without the cottages and stalls a player has built over, and a hamlet with no door gets nobody; people made while a fight is on start indoors behind a door.
- Drawn as `SoldierInstance`s in `push_building_models`, one model per kind (`Art.folk`).
- `--hamlet-demo` lifts the fog (`reveal_all`, offline only) so the people show without an army.

### Not yet

- Villagers carrying things round a hamlet (a load going from a haystack to the barn; the caravans between hamlets are [stage 6b](#stage-6b-as-built-caravans-between-hamlets)), other fauna, and villagers who react to a particular faction.

## Stage 6 as built: crows over the dead

*2026-10-24. Game and the creature pack only; nothing in the simulation. The crows themselves are battle-effects work (T7, `aftermath.rs`: when they come, how many, when soldiers flush them); this stage only gives them a body.*

- The `Crow` model (`design/creatures/source/crow.py`, the dove's rig and kit in blue-black with a thick beak, about 1,000 triangles) replaces the black boxes. Its Idle is a crow on the ground looking about and pecking twice, and Cheer is the flap, played slower than a dove's for a circling bird. It is drawn 25% bigger than the dove it is built from (`CROW_SCALE`).
- `Aftermath::birds` gives the crow model the same crows the old boxes were (same flocks, same times, same positions: a test holds them equal); `Aftermath::crows` still makes the boxes, which are drawn only if the pack lacks the model.
- Crows are drawn as soldier instances by `push_crows` (in every kind of match, not only Conquest), not in Commander View or the unit viewer. A crow plays its flap whenever it is off the ground at all, so a crow settling or climbing never glides stiff.

### Not yet

- Crows perched on corpses, and a crow that takes a bite (one now perches on a fallen standard: stage 9).

## Stage 6b as built: caravans between hamlets

*2026-10-24. Game and the creature pack only; nothing in the simulation, nothing hashed, saved or sent.*

### What a player sees

- Between neighbouring Grubfolk hamlets, an ox cart with a drover walking at the ox's flank, or one or two porters in file with sacks on their backs, goes along the open ground from one hamlet to the other, waits ten seconds or so to unload (the cart walks slowly round on the spot to face the way back, the drover walking round with it), and comes back. The cart is loaded with sacks and a barrel; its wheels turn as it goes.
- Routes serving a Town or the Bazaar usually have a cart, routes between villages one in four; the rest have porters. A hamlet has two routes at most. A route never passes through a third hamlet, over water, woods or a slope steeper than about one in six, so a land with few clear ways between its hamlets has few caravans (Four Teeth has four, two of them carts; Ribcage Pass none; the random lands between two and ten).
- An army the viewer sees within 14 tiles of a caravan stops it for six seconds to look (people and ox stand), and then it goes on, even with the army still there, so a camp beside the road does not hold it for good. A fight within 40 tiles holds it until six seconds after the fight is quiet. A building the player puts up across a route turns the caravan back. Caravans never run.
- Seen only where the viewer can see (fog), in Conquest only (the structures early return that deer, doves and villagers share), and not in Commander View or the unit viewer.

### How it works

- `gc_game::ambient::Caravans::new` (called with the hamlets at the start of a match) joins each hamlet to its nearest two others within 24 to 180 tiles, stopping each end 3.2 tiles beyond the hamlet's models (`Hamlets::reach`; the cart's middle, so the ox's nose and the turning circle stay outside) and checking the straight line with `hamlets::route_clear` (dry, no woods, at most 0.12 of climb every 0.75 tiles). The load of each route is decided from a hash of its index and length, so every machine, and a replay, draws the same ones.
- Every caravan is kept going all the time, wherever the camera is (a scalar along its route; a hundred of them cost nothing), so one seen later is where it should be, while the match is showing (they stand still behind the menu, in Commander View and in the unit viewer). Only those within the camera's reach are handed to the renderer. Armies and fights come from the same `Eyes` the villagers use.
- New model `Oxcart` (`design/creatures/source/oxcart.py`, about 5,600 triangles at full detail, 18 bones; creatures.pak is now about 9 MB): the ox's rig is the dog's gait scaled up, the cart rocks a little on its bone and each wheel is rigid on its own bone. It has only Idle and Walk clips, both two seconds long and exported slid to zero (`crit.py export(slide=True)`) so the baker's 12 frames a second land on whole keys. The Walk clip is two steps of the ox and exactly one turn of the wheels, and a wheel of radius 0.38 tiles rolls 2.39 tiles in a turn, which at `CART_PACE` (1.19 tiles a second, derived from the radius in `caravans.rs`) is two seconds, so the wheels roll without slipping (a test holds the two numbers together). The other creatures' Walk clips are 0.5 s over 6 baked frames (their keys sit a 24th of a second after zero, so the baker drops key 2 and the first step is short); the cart avoids that. The drover and porters are the existing `FolkFarmer` and `FolkPorter` with their Walk clip played at the rate that matches the cart's pace; a porter's Walk is already the laden walk (hands on his straps), so no new carry clip was needed.
- Drawn as soldier instances beside the villagers (in `push_creatures` since stage 7). (Stage 9 now pitches the cart to the slope; until then the instance had no pitch.)
- `--hamlet-demo` puts the first cart (or the first porters, where there is none) on its route and the camera ahead of it so it comes by.

### Not yet

- Carts and porters on the road from a faction base's seep to its granary (stage 3), a worn road where they go, a cart's wheels easing to rest when it stops (they snap to the Idle pose, at most 22 degrees), other lands' fauna (the Brood's, the Choir's), and crows perched on corpses.

## Stage 7 as built: butterflies, moths and dragonflies

*2026-10-24. Game and the creature pack only; nothing in the simulation, nothing hashed, saved or sent. It also moves the drawing of every ambient creature out of the Conquest-only building pass (below).*

### What a player sees

- **Butterflies** (orange, about a third of a metre across, drawn 1.6 times that) flit over flower meadows, glades, heather and grassland by day: they hop from place to place a few tiles from their home, bobbing as they go, and now and then alight on the ground for a second or so with their wings up, facing the way they will go (never on or beside water: a home near water has dragonflies and moths that do not alight). **Moths** (pale, brown-furred) do the same over wet meadows, sedge, carr, bog and fern hollows at dusk and in the dark, and **dragonflies** (blue, glassy wings, drawn 1.8 times the model) hover and dart over and beside water by day. A butterfly or dragonfly shrinks away as the light goes and a moth grows as it comes (the sun's height, as the fireflies and pollen use). A cell's kind never depends on the light, so at dusk the same butterfly fades out and the cell's moth fades in rather than one replacing the other (a glade has both; wet ground by water has moths and dragonflies).
- Each cell of eight tiles has none or one to two of them, the same ones every time, so a place keeps its insects. They are tiny and few on purpose; the Graphics preset's effects density thins them (zero shows none), and they are drawn only near the camera (12 tiles at no zoom to 40 tiles) and only where the viewer can see (fog). Insects are not drawn behind the menu, in the unit viewer or in Commander View; pausing freezes them where they hang, as the effects clock stops.
- A Conquest map has no habitats, so its lands stand in: Valley as flower meadow, Meadow as grassland, Fen as wet meadow, Highland as heather; forest and waste have none.

### How it works

- `gc_game::ambient::Insects` keeps no state. A hash of the cell says whether it has insects and where its home is; each insect goes from one waypoint (a hash of its seed and its hop number, within 3 to 5 tiles of home) to the next, as a function of the effects clock, so it is where the clock puts it however the camera has moved. A butterfly's or moth's hop is a smooth glide with a rest at the end of some of them (it comes down over the end of the hop and lifts off at the start of the next, with no jump); a dragonfly darts in the first 40% of a hop and hovers the rest. A resting hop glides for its first half and turns to face the next hop's way before it sits, so a resting insect never spins on the spot, and rests on the ground (height zero). Angles are turned the short way, and the bob has a phase of its own rather than the seed (a seed in the millions in a single-precision sine would jump). Tests hold, for real seeds: they stay within their roam, nobody moves faster than a dart, jumps in the air or snaps round, dragonflies keep to their pond, dusk swaps no cell's kind and fades it, the same clock gives the same picture, a map with no habitats uses its lands, and density and the camera's reach thin them.
- Models `Butterfly`, `Moth` and `Dragonfly` from `design/creatures/source/insects.py` (about 500 to 700 triangles each; creatures.pak is now about 12 MB with the three 512-pixel albedo maps). Each wing is one rigid piece on its own bone, so it never folds or crumples. Only Idle (wings up, trembling, two seconds) and Cheer (the wing beat, one second) clips, keyed 0 to 48 and 0 to 24 and exported slid to zero (see the cart); the beat is played 3.2, 4.5 and 9 times a second.
- **Drawing moved.** Deer, doves and the villagers were drawn inside `push_building_models`, which returns at once in a match with no structures (every Field Battle), so wildlife never showed there although it was being made. They (and the insects and the caravans, which `push_building_models` drew too) are now drawn by `push_creatures`, called next to `push_crows`; walkers and the building crews stay in `push_building_models`, so Field Battle land, which is where the habitats are, shows its deer, doves and insects. The hamlets, villagers and caravans still exist only on Conquest maps.

### Not yet

- Ripples under the dragonflies, fish that are seen (the rises are only their rings), insects that shy from armies and soldiers (they ignore everyone), butterflies on particular flowers, other lands' fauna (the Brood's, the Choir's), and crows on corpses.

## Stage 8 as built: fish rises

*2026-10-24. Game and the particle kit only; presentation, nothing in the simulation, nothing hashed, saved or sent.*

### What a player sees

- Now and then on the deeper water near the camera a fish takes a fly: two soft rings spread over a second or two (the wider one reaches about half a tile) and a bead or two of water jumps. They happen on any map with water (Conquest's too, which has no habitats), twice as often at dusk, only where the viewer can see (fog), and the Graphics preset's effects density thins them (zero shows none). Water shallower than 0.3 over the bed has none, so a ford or a puddle has no fish. They thin out as the camera rises (as pollen does) and are gone from high up.

### How it works

- `Touches::rises` (in `ambient/touches.rs`, called from the same block of `main.rs` as `Touches::frame`) tries random points round the camera, `RISE_RATE` (0.003) a second for each tile of reach, doubled at dusk; one that falls on water deeper than `RISE_DEPTH` and in view calls `Field::rise`, which makes the particles (`Effects::rise`: two `Ring` particles with different reach and life, and a little `spray`). Like the other touches it keeps no state. The rings are not staggered in time (the particle kit has no delay), so they open together at different speeds.
- Tests: rises come only on deep water, never out of view or at zero density, nor over dry land; `Effects::rise` makes two rings on the surface and beads.

## Stage 9 as built: crows on standards, wheel dust, carts on slopes

*2026-10-25. Game and renderer; presentation only, nothing in the simulation, nothing hashed, saved or sent.*

### What a player sees

- **A crow on every fallen standard.** The first crow of the flock that comes to a wiped-out regiment lands on the splintered stub of the standard the regiment left (not on the ground with the rest), sits there looking about, takes off with the flock when soldiers come near and comes back to the stub when they have gone. The standard stands for four minutes and the crows are gone in about three, so it is there for them; if it is dropped on a very crowded field the crow comes down to the ground.
- **Dust off the cart's wheels.** A cart on the way kicks a small puff behind each wheel in dry weather. A standing cart, one turning about at the end of its route, one on mud or snow (`Look::wet` or `Look::snow` above 0.35) or one the viewer cannot see raises none.
- **Carts that pitch.** The cart and its ox tilt with the slope along their way: the ox's nose rises on a climb and drops going down, instead of the whole cart staying level while the ground slopes under it. The people walking with it stay upright. The climb limit keeps routes under about nine degrees.

### How it works

- Crows: `aftermath::bird` takes a `perch` flag (the flock's centre still has a standard standing, `Aftermath::perch_standing`); crow 0 always has the standard's position as its spot, and `PERCH_HEIGHT` (0.9, the pole's height) as its height when it is not aloft and the standard stands, blending to the circle's height as it rises. If the standard is dropped (the 96-standard limit on a very big battle) the crow comes down to the ground at that spot. The box fallback is made from the same `bird`, so it perches too.
- Dust: `Caravans::dust` (called from the touches block of `main.rs`) puts `Field::footstep` at each wheel's contact point (0.62 behind and 0.8 each side of the cart's middle, `oxcart.py`'s hubs) for every cart the viewer sees that is rolling (on the way, not turning about: `Seen::rolling`), at a soldier's dust pace.
- Pitch: it rides on the tip the soldier instance already has. The third and fourth bytes of `SoldierInstance::flags` are how far a model is tipped over about its base (0 upright to 255 lying down) and the way it falls (a turn in 256ths from its front), which the fallen and the toppling buildings use (`tipped` in `world.wgsl`, which also turns the normals and tangents, so lighting and the shadow map follow, and which switches foot IK off). `gc_render::pitch_bytes(radians)` gives those two bytes for a pitch: the angle as the tip (kept under the 255 that means lying down) and the way 128 (onto its back: the front end rises) for a climb or 0 (onto its front) for a descent. The game's `caravans::tilt_of` is the slope of the drawn ground along the cart over 2.8 tiles (1.4 either side), the cart and its ox are tipped by it, and since the model tips about its origin an even slope keeps every contact point (wheels and hooves) on the ground, with a small error over a crest or a hollow.
- Tests: a crow sits on the stub at the standard's place and height, even on a hill, flies with the flock and returns, and has no stub once the standard is gone; the wheel points are behind and beside the cart; dust only from a cart on the way (not one standing or turning about) in the dry and in sight; the pitch is 0 level, positive up a hill, negative down it and 0 across it; its bytes are a tip toward the back for a climb and the front for a descent, never lying down.
