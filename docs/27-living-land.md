# 27 · Living Land: organic world generation

*Plan, 2026-10-22. Nothing here is built yet: the project is paused, and building starts when Mario says go. The fairness model was decided by Mario on 2026-10-22 (see [Fairness](#fairness)).*

Mario asked for maps that are "truly random, truly chaotic, like nature", where no two things are alike. Today's maps tend to be lines, ovals and circles. This document says why, what replaces it, how maps stay fair, how the biomes get depth, and the order of the work. The concept page with prototype maps is the "Grubcraft Living Land" artifact (https://claude.ai/artifact/S9PTV2MgvKnoHBFZ16s6Wt). The Python prototype that made them is in the project files under `design/world-gen/proto/`.

## Why today's maps look drawn

The Wilds recipe (`crates/gc_map/src/wilds.rs`, see [07 · Field Battle maps](07-world-and-maps.md#field-battle-maps-and-recipes)) describes a map as a list of two shapes:

- **Lines.** Every mountain band, river and thicket belt is a `Shape::Line` stamp through points 8 tiles apart.
- **Discs.** Every copse, meadow, lake, hill and knoll is a `Shape::Disc`, and a biome patch is six discs on top of each other.

Then the loader turns the north half to make the south half (`Symmetry::Rot2`). The rolling ground underneath is noise averaged over that turn. Tuning the stamps can't make this look like nature, so Living Land grows the land from fields of numbers instead.

## How a map grows

Each layer reads the land the layers before it left. All of it is whole-number maths in the game (the prototype uses floats), so every machine builds the same map from the same seed.

1. **Shape.** Fractal noise, bent by more noise (domain warping), gives the broad lowlands and uplands. Inside a wandering "mountain country" outline, ridged multifractal noise draws sharp crests that branch like real ranges.
2. **Rock.** Strata: above a height, the ground climbs in benches. A noise field sets how sharp each bench edge is. Sharp edges are cliffs and soft edges are ramps, so cliff lines follow the land's contours and mountains can be climbed in places. The highest cores become crags, a sudden rock wall along a jagged contour, and the gaps between crags are natural passes.
3. **Weather.** Tens of thousands of simulated raindrops run downhill. They pick up soil on steep ground and drop it where the land flattens, so every slope gets gullies and every foot gets a fan. A light crumbling pass follows.
4. **Water.** A priority flood finds where every drop drains, and flow accumulation says how much water runs through each tile. Where enough gathers there is a river, as wide as its flow. It bends more where the land is flat, and rivers join into networks. Basins that can't drain fill into lakes with their own outlines. Valleys ease down to each river at a width that varies along it.
5. **Biomes.** Wetness (nearness to water plus noise) and height choose meadow, forest, fen, highland or bone waste. Borders are warped contours, never discs. See [Biome depth](#biome-depth) for what grows inside each one.
6. **Settlers.** Sites go where the land gives a reason, and each records its reason: villages on lake shores and river confluences, forts on high ground above a choke, lairs deep in forest or waste far from the starts, standing stones on the highest highland.
7. **Ways through.** There is a route between every pair of starts. Where the land blocks it, the cheapest crossing is cut: a pass at the lowest saddle, a ford at the narrows. In the prototype's test maps the land almost never needed one; passes between crags and shallows on wide rivers were enough.

## Fairness

Mirroring is how every map is fair today, and it is also what makes a map look made. Three ways were prototyped and measured on 512-tile maps. A spread is (most − least) ÷ average across players, so 0% is perfectly equal.

| Option | What it is | Result |
|---|---|---|
| Mirror | Organic land, south half still a turned copy | Twins are exact. The copy shows at map scale, and on six players corner and middle starts already differ (building room spread 34%). |
| Echo | Big shapes mirrored, detail unique | Dropped: put starts on lake shores, grew round lakes, worst spreads (nearest enemy 52%). |
| **Measured** | Unique land; starts placed by search, the Age of Empires II way | 2 players: 0% gap in walk to the nearest enemy on every seed, own resources 3 to 6%. 4 to 6 players: 3 to 16%. |

**Decided (Mario, 2026-10-22): Measured by default, with a lobby option "Fairness: Measured / Mirrored".** How Measured works:

- A search on a coarse grid places the starts so the walk to the nearest enemy (least-cost routes round cliffs, lakes and thicket) is as equal as possible.
- Each player's own resource fields stand the same walk away from their start (the prototype used 30, 55 and 85 tiles), in whatever direction the land allows.
- Every start gets the same flattened clearing, so building room is equal by construction. The prototype only roughed this in, which is why its room spreads on six players are high.
- Team games balance teams, not only players.
- `gc_mapcheck` reports the numbers and rerolls a map outside the limits. Today it holds Conquest maps to 3% on walking distances. Two-player maps already meet that. Four to six players need the Rust search to do better than the prototype, or per-team limits.

## Biome depth

Mario (2026-10-22): "really maturing and developing the depth and realism of the biomes within the land. Nothing crazy but nice and immersive."

Every patch of ground is decided by the same few things nature uses: **height**, **wetness** (nearness to water, how much drains through, hollows), **which way a slope faces** (sunny south-west sides warmer and drier, shaded sides cooler and mossier), **rock and slope**, and **age** (old growth against young regrowth). Each biome splits into habitats from those. Habitat edges blend over a few tiles, so woods thin out instead of stopping.

| Biome | Habitat | Where it forms | What you see |
|---|---|---|---|
| Meadow | Flower meadow | Sunny, drier flats | Wildflowers, open ground |
| | Grassland | Everywhere else | Grass, the odd veteran oak |
| | Scrub edge | Where meadow meets forest, rough slopes | Bushes and young trees |
| | Wet meadow | Hollows and stream sides | Tussocks, tall grass |
| Forest | Old growth | The heart of a forest | Tall dark trees, mossy boulders, glowcaps; thicket only in the deepest cores |
| | Birch fringe | Forest edges and young stands | Lighter, sparser trees |
| | Fern hollow | Damp dips | Ferns under trees |
| | Glade | Where old trees fell | Open grass, stumps and fallen logs |
| Fen | Sedge tussock | Open wet ground | Sedge and tall grass |
| | Alder carr | Wet woodland | Willows and alders standing in shallows |
| | Peat bog | Hollows | Dark pools and hummocks |
| Highland | Heather moor | Gentle tops | Purple heather, dry grass |
| | Scree | Aprons below cliffs | Fallen rock |
| | Tree line | A band that rises on sunny sides | Wind-bent pines thinning out |
| | Bare rock | Above the tree line, and cliffs | Crags and summits |
| Bone waste | Cracked flat | Flat waste | Crazed pale crust |
| | Bone drift | Lee sides | Wind-combed ripples of bone dust |
| | Rib field | Patches | Bone ribs and spires |
| | Dying grass | Where waste meets green land | Grass giving out |

**Rivers and shores** get the features real ones have: gravel and sand bars on the inside of bends, steep cut banks on the outside, the odd oxbow lake where a loop was cut off, bankside willows and alders in a strip whose width varies, reed beds in slack water, beaches on gently sloping lake shores, and springs and confluences marked as places.

**Worn trails** link villages, forts, stones and starts. Each new trail prefers ground earlier ones wore down, so they merge into a network the way real paths do. They avoid steep ground and cross rivers at shallows.

**Gameplay stays the same rules.** Habitats only choose among what the simulation already knows: open ground, tall grass, woods, thicket, slope and fords. That gives light, readable nudges: wet meadows and reed beds have tall grass to hide in (see [20 · Ambushes](20-ambushes.md)), scree is steep going, glades are open pockets inside forests, and gravel bars are natural fords.

**What it needs from art.** Most habitats are dressed with the existing scenery models ([25](25-scenery-models.md)). The new pieces are:

- a few models: stump, fallen log, alder, sedge tussock and a scree scatter;
- ground materials blended per habitat (grass, flowered grass, forest loam and moss, peat and mud, heather, scree and gravel, sand, bone dust, cracked crust) instead of one tint per land.

`Map.land` is already one byte per tile, so it can carry the habitat.

**Living touches (later polish, render only):** mist pooling on fen and valley floors, fireflies over fens at dusk, butterflies in flower meadows, dust devils in the waste, birds lifting from woods when an army passes, and an ambient sound bed per habitat.

## Stages

Each stage is playable on its own.

1. **Grown land** (`gc_map`). Port shape, rock, weather, water, biomes, habitats and canopy, all in whole-number maths, replacing the Wilds' line and disc stamps. Keep `Rot2` for now, so fairness is unchanged. Plains, Mixed and Wild become dials on the same pipeline: how much mountain country, crag height and forest density. The old Wilds stays as "Classic", so old seeds and replays still load. Target: a 768 map builds in under 3 seconds, measured before merge.
2. **Ways through and walkable mountains.** The pass and ford carver, ramps from soft strata, and shallows on wide rivers. This absorbs "Natural mountains, stage 2" from the [roadmap](13-roadmap.md). Golden replays are re-recorded if shipped maps change.
3. **Measured fairness.** The start search, resource fields at equal walks, equal clearings, team-aware balance, Conquest shares placed by walking distance, fairness limits and reroll in `gc_mapcheck`, the lobby's Fairness option, and the generator version stored with the map choice in replays and the protocol.
4. **Biome look and sites by reason.** Ground materials per habitat, riverside dressing, trails and the few new scenery models. Sites placed by the rules above, with Conquest lairs guarding the rich fields.
5. **Optional: water that runs downhill.** The game has one water line for the whole map, so rivers sit in valleys cut down to it (see [07 · Water](07-world-and-maps.md#water-later)). Highland lakes and falling rivers need a water level per tile. That touches the simulation's ground and the water drawing, so it is its own decision.

## Costs and risks

- **Load time.** The Python prototype takes 20 to 40 seconds a map. The game runs erosion, drainage and the start search on coarse grids. Estimate 1 to 3 seconds for a 768 map, to be measured in stage 1.
- **Same map everywhere.** Online play needs identical maps on every machine. Erosion and river tracing are the new parts to pin with hash tests, the way `Map::generate` is pinned today.
- **The computer players.** The battle and Conquest minds read chokes and routes from the map, and organic maps have more varied chokes. Their route reading gets a check in stage 2.
- **Replays and seeds.** Maps are rebuilt from their seed, so the generator's version is saved with the map choice. Old replays keep playing on Classic.
