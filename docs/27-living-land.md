# 27 · Living Land: organic world generation

*Plan, 2026-10-22. Stage 1, grown land, is built (2026-10-23, see [Stage 1 as built](#stage-1-as-built)), and stage 2, ways through (2026-10-24, see [Stage 2 as built](#stage-2-as-built)); stages 3 to 5 are not yet. The fairness model was decided by Mario on 2026-10-22 (see [Fairness](#fairness)).*

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

### Clearing land for bases (idea 22)

Mario (2026-10-23): clearing trees and scrub should be part of growing a base. On grown maps the woods are denser and more ragged, so room to build near a start often has to be cut. This is planned and built as its own piece of work, **idea 22** (the mechanics, workers felling and grubbing, animations, stumps and cleared ground), not as a Living Land stage. What Living Land owes it:

- Woods, scrub and thicket stay a **tile cover layer** the game can change at runtime, never baked into the ground mesh or the scenery placement, so a cleared tile can drop back to open ground and the dressing on it can be removed.
- Each tile keeps its **habitat** after clearing, so a felled old-growth tile becomes a glade-like clearing with stumps and loam, and cut scrub becomes rough grass.
- In Measured fairness (stage 3), the **equal clearing** each start gets is the open room before any cutting. If clearing is cheap, `gc_mapcheck` should also count the cuttable woods near each start, so no player starts boxed in by old growth while another has light scrub.

## Stages

Each stage is playable on its own.

1. **Grown land** (`gc_map`). Port shape, rock, weather, water, biomes, habitats and canopy, all in whole-number maths, replacing the Wilds' line and disc stamps. Keep `Rot2` for now, so fairness is unchanged. Plains, Mixed and Wild become dials on the same pipeline: how much mountain country, crag height and forest density. The old Wilds stays as "Classic", so old seeds and replays still load. Target: a 768 map builds in under 3 seconds, measured before merge.
2. **Ways through and walkable mountains.** The pass and ford carver, ramps from soft strata, and shallows on wide rivers. This absorbs "Natural mountains, stage 2" from the [roadmap](13-roadmap.md). Golden replays are re-recorded if shipped maps change.
3. **Measured fairness.** The start search, resource fields at equal walks, equal clearings, team-aware balance, Conquest shares placed by walking distance, fairness limits and reroll in `gc_mapcheck`, the lobby's Fairness option, and the generator version stored with the map choice in replays and the protocol.
4. **Biome look and sites by reason.** Ground materials per habitat, riverside dressing, trails and the few new scenery models. Sites placed by the rules above, with Conquest lairs guarding the rich fields.
5. **Optional: water that runs downhill.** The game has one water line for the whole map, so rivers sit in valleys cut down to it (see [07 · Water](07-world-and-maps.md#water-later)). Highland lakes and falling rivers need a water level per tile. That touches the simulation's ground and the water drawing, so it is its own decision.

## Stage 1 as built

*Built 2026-10-23.* The code is `crates/gc_map/src/grown/`: `field.rs` (fixed-point helpers, value noise, fractal noise, warping, ridged noise, blurs and distances), `shape.rs` (shape and rock), `erode.rs` (weather), `water.rs` and `biome.rs` (biomes, habitats and canopy), with `mod.rs` running them in order. Every value is a whole number (16.16 fixed point), and the crate's float lint covers it.

**How the lobby offers it.** A Field Battle lists **Land: Random**, **Land: Plains**, **Land: Mixed** and **Land: Wild**, the grown maps, and a new lobby starts on Land: Random. The old Wilds are still there as **Classic: Random**, **Classic: Plains**, **Classic: Mixed** and **Classic: Wild**, built exactly as before, so old seeds and replays load. Conquest's big six-player maps are still the Classic Wilds in stage 1 and are named Classic in its list; they move to grown land once fairness is measured (stage 3), because Conquest's shares and lairs are placed by the Wilds' layout. The network protocol went from 20 to 21 because the map choices changed.

**How the map is stored.** A grown map's description (`gc_map::grown::describe`) holds the seed, the wildness, the starts, the sites and set pieces; `MapDef.grown` tells the loader to grow the heights, cover, thicket, land and habitat from the seed (cached, so the lobby picture and the battle share one build) and then lay any stamps on top. So a grown map is the same bytes on every machine, and the fingerprints of one map of each wildness are pinned in a test.

**The pipeline, in order.**

1. Shape at half resolution, drawn up to full: broad land, rolling hills and mountain country (ridged crests to the power 1.5), kept off the starts.
2. Strata benches (from 2.7 tiles above the water line, 1.6 tiles high), cliff or ramp by a sharpness field, then crags on the highest cores of mountain country. A "lean" field sets each crag wall's width from 0.35 to 3 tiles of height, so some walls are sheer and some climbable. Heights ease off below the top the map can hold, so peaks don't flatten.
3. Weather: about 70,000 raindrops on a 512 map (scaled by area), run one after another so the result is the same everywhere, then a light crumbling pass.
4. Each start gets a flat clearing, and the mountains keep off a ragged ring round it, not a circle.
5. Symmetry: the land is grown whole, then the south half is made the turned copy of the north along a wandering seam, blended over a band so there is no straight join. The water and biome layers are made symmetric the same way: every noise field the biomes read is blended across the seam, and the water is worked out on symmetric ground, with the map's middle rows as outlets, so rivers that reach the middle end in lobed ponds instead of stopping dead.
6. Water: priority flood and flow, river lines that meander (more on flat land), lakes with a size cap, valleys easing down to the rivers, and shallows across every long river about every 140 tiles of its length (fords).
7. Biomes, the 19 habitats of [Biome depth](#biome-depth) and canopy: woods, tall grass and thicket are chosen per habitat. Thicket stands only deep in old growth, in small knots (never a wall more than 11 tiles long) and never near a start.
8. Ways through ([stage 2](#stage-2-as-built)): a walkable pocket no start can reach (say a mesa ringed by cliffs) gets a ramp, or is broken into rock when small, and a pass is cut where two starts' way round is far longer than the straight line.

**The dials.** Plains, Mixed and Wild set the share of mountain country (11%, 30%, 44%), crest height, how sharp bench edges are, where crags begin and how tall they are, forest density and how much flow makes a river.

**Cover for idea 22.** Woods, tall grass and thicket stay the tile cover layer the sim already reads, written into the `Map` at load, never baked into the ground. Scrub (scrub edge and the like) is its own habitat with lighter cover: tall grass, or woods where the canopy allows, so it can be cleared differently from old growth. Every tile keeps its habitat (`Map.habitat`), ready for a cleared tile to become a glade or rough grass. The ground now reads it ([36](36-habitat-ground.md)); the scenery does not yet.

**Sites and set pieces.** The starts are the seeded map's plateaus. The sites (village, company camp, old fort, lair, standing stones) are placed in the north half on standable ground and turned for the south; placing them by reason is stage 4. The set pieces are found from the built land: "The heights" and "The far heights" where ground stands well above its surroundings, and "The ford" where enough ford tiles gather. A map with fewer than two gets a knoll at each side.

**Measured (2026-10-23, release build).** A 768 map builds in about 2.1 to 2.4 seconds (`grown_load_time`, run by hand, on a machine busy with other builds), inside the 3-second target. `every_grown_map_passes_the_checker` builds each wildness at 256, 512 and 768 for four seeds (36 maps) and runs them through `gc_mapcheck`'s checks. All pass, and none needed a reroll; the test allows a reroll for one map in five, as the lobby rerolls a map whose starts aren't joined. `cargo run -p gc_mapcheck -- gen Land-Wild 512 SEED OUT.ron` (or `Land-Plains`, `Land-Mixed`) writes a grown map. Screenshots are in the project files under `screenshots/living-land/`.

**Not in stage 1.** Ways through (the pass and ford carver) and the AI's route check are stage 2 (built, below); fairness is still the turned copy until stage 3; grown maps use today's ground colours by land, not the habitat materials, until stage 4 (the habitat ground is built, [36](36-habitat-ground.md); scenery by habitat is not).

## Stage 2 as built

*Built 2026-10-24.* The code is `crates/gc_map/src/grown/ways.rs`, run as the last step of growing the land, after the biomes. It works on what a regiment can stand on as the game's own nav rules see it (the same half-tile walkable grid, with ground costs), and every change goes to a tile and its turned copy alike, so the map stays its own turned copy.

- **Ramps into cut-off ground.** A piece of standable ground no start can reach (a plateau ringed by cliffs, a valley shut in by crags, in principle a start cut off by a river) gets a ramp from the reachable land at its cheapest crossing (a step costs ten, plus four a tenth of a tile climbed, plus thirty into deep water). The ramp is a straight slope six tiles wide at full height, fading into the land two tiles further out, long enough to climb no more than 0.3 of a tile per tile (under half what the nav rules call steep). It is cut into the high side and built out on the low side, from the land's own height at one end to its height at the other. Where it meets deep water it becomes a ford, 0.2 of a tile under the line. Thicket on it is gone, and bare rock on it becomes scree. Pieces under 40 tiles, where a ramp would be more scar than land, are broken into rock as in stage 1, now with every other tile pushed up or down so no one can stand there.
- **Passes.** Where the walk between two starts (as `gc_mapcheck` walks it, ground costs and all) is more than 1.7 times the straight line, the cheapest way through what is in between is found. A step off standable ground costs three open steps plus the climb, and more into deep water. Each stretch of cliff, river or thicket on that way gets a ramp, or a ford across a river.
- **Rounds.** Ramps and passes run up to four rounds, each checking the land again. Each round cuts a pass only for the worst-stretched pair, since one pass often mends several pairs. Whatever is still cut off at the end is broken, except a start's own ground, which is never broken (and never the reachable land, when a pocket's turned copy is part of it).
- **Narrow mesas and the middle.** On a pocket narrower than the ramp is long, the ramp runs between the two end tiles' own heights, so it climbs onto the mesa instead of cutting a trench across it. Where a ramp meets its own turned copy (near the middle of the map) the stronger hand wins on both sides, so nothing is blended twice.
- **Fords on wide rivers.** A river at least six tiles wide somewhere gets a ford about every 93 tiles of its length instead of 140.
- **Ramps from soft strata** were already in stage 1: a bench edge is a cliff only where the sharpness field says so, and a ramp elsewhere.
- **Floods.** The fords and river ramps are tiles under the water line a regiment can walk, which is what the flooding rivers ([32](32-changing-land.md#fords-that-flood-idea-6)) look for, so a long rain shuts them like any other ford.

**Measured (2026-10-24, release build, 12 maps at 512: four seeds of each wildness).**

- On Mixed and Wild maps, the share of highland an army can reach went from 35–48% to 54–64%. Cut-off plateaus that stage 1 broke into rubble are now high ground with a way up.
- The longest walk between any two starts, against the straight line, went from up to 2.2 times to at most 1.46. The walk to the nearest enemy is unchanged (about 1.0 to 1.1).
- A 768 map still builds in about 2.1 to 2.5 seconds. The loader's height sampling got faster to pay for the extra checks.
- All 36 maps of `every_grown_map_passes_the_checker` pass with no reroll.
- `ways::tests` check that no start or plateau is left cut off and no start pair winds past 1.8 at 256.
- **The computer players' routes.** `gc_match`'s `grown_land_battles_meet` (run by hand) plays two Veteran computer players on a 256 map of each wildness, two seeds each. In all six their armies met within 23 to 77 seconds, and the battle was decided in under four and a half minutes. The minds read the grown land's chokes and ways without changes.

## Stage 3 as built

*Built 2026-10-24, in part.* Measured fairness is in the lobby as four new choices, **Measured: Random, Plains, Mixed and Wild** (`gc_map::MEASURED`). The mirrored grown land stays **Land: ...** and is still the default, because Measured is not yet as even as the doc's limits for four to six players (below). The code is `grown/starts.rs` and the `Fairness` switch on `Grown`.

- **Unique land.** No seam: the land, water and woods grow once over the whole map.
- **Starts by search.** Each start keeps to a window round its plateau, so north, south, corners and middles mean what they always did for handing out starts (`gc_match::battle_starts`). A coarse-grid search moves the starts one at a time to make the walk to the nearest enemy as equal as it can. It scores every way the lobby hands starts out: corners and all six, north against south, everyone for themselves, and column against column. It runs twice. The first pass runs on the land's shape before the water, so the rivers, woods and open ground grow round the chosen starts. The second runs on the finished land as the nav rules see it, a dozen tiles at most, onto dry, standable ground.
- **Equal clearings.** Every start gets the same clearing shape, so its building room is equal by construction.
- **Passes cut sooner.** Stage 2's passes fire at 1.3 times the straight line instead of 1.7, for up to eight rounds, because under Measured the walks are what keeps the starts equal.
- **Paired sites.** Each site in the north (village, camp, fort, lair, stones, knolls, heights, ford) gets a partner of its own in the south, as near the turned spot as the land has room. Nothing is mirrored.
- **Deployment and the lobby preview** read the map's own starts (`gc_match::terrain_view::field_starts`), not the plateaus. PROTOCOL 35.

**Measured (2026-10-24, 12 maps at 512, true walks via `gc_mapcheck`'s search, spread = (most - least) / mean of each start's walk to its nearest enemy).**

- **Two players** are equal by construction: one pair, one walk.
- **Four players (2v2, free-for-all):** 0 to 5% on most seeds, but 10 to 20% on some Mixed and Wild seeds.
- **Six players:** 3v3 2 to 26%. Free-for-all and two-against-two-against-two 3 to 39%. The mirrored land is 1 to 30% on the same measures, since twins are equal but corners and middles are not.
- **The checker:** all 72 maps (36 mirrored, 36 measured) of `every_grown_map_passes_the_checker` pass with no reroll.
- **Build time:** a 768 Measured map builds in about 4.5 seconds (mirrored 2.2), over the 3-second target. The second search and the extra pass rounds are the cost.

**Still to do for stage 3:**

- A finer search, or seeds rerolled against fairness limits in `gc_mapcheck`, to bring four to six players under about 5%.
- Then make Measured the default.
- Count cuttable woods near starts (idea 22).
- Team-aware limits.
- Store the generator version with the map choice in replays.
- Move Conquest big maps onto grown land.

## Costs and risks

- **Load time.** The Python prototype takes 20 to 40 seconds a map. The game runs erosion, drainage and the start search on coarse grids. Estimated 1 to 3 seconds for a 768 map; measured in stage 1 at about 2 seconds.
- **Same map everywhere.** Online play needs identical maps on every machine. Erosion and river tracing are the new parts to pin with hash tests, the way `Map::generate` is pinned today.
- **The computer players.** The battle and Conquest minds read chokes and routes from the map, and organic maps have more varied chokes. Their route reading gets a check in stage 2.
- **Replays and seeds.** Maps are rebuilt from their seed, so the generator's version is saved with the map choice. Old replays keep playing on Classic.
