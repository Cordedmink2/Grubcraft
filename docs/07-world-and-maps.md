# 07 · World and Maps

## Map anatomy

Every Conquest map is built from the same parts, so players learn to read any new map in seconds.

| Feature | Purpose |
|---|---|
| **Start position** | 2 Grub seeps, 1 Marrow vein, room for a base, one natural chokepoint |
| **Natural expansion** | 2 seeps close to the start; the first contested ground |
| **Contested expansions** | Seeps and Marrow veins in exposed spots, often creep-guarded |
| **Grubfolk hamlets** | Neutral trade/raid targets, at least one per player within reach of each start |
| **Creep camps** | Guard Marrow, Shrines and treasure; hero experience and items |
| **Shrines** | Capturable; give vision and a small aura (Clans earn Honor from them) |
| **The Titan's Heart** | Centre objective for the Heart victory and Heartbeat events |
| **Bloodseep sites** | Pre-placed hidden locations where Tremors can open new seeps |
| **Frontier sites** | 4 to 8 slots hidden in the fog (ruins, lost caravans, relic barrows and more), filled from the match seed; they reward whoever scouts them first ([19](19-campaign.md#part-1--exploring-inside-a-match)) |
| **Terrain lanes** | Ridges (ribs), forests, fords, roads, bogs that shape where armies meet |

### Map sizes

| Size | Tiles | Players | Typical length (Standard) |
|---|---|---|---|
| Duel | 128 × 128 | 2 | 20 to 30 min |
| Skirmish | 192 × 192 | 2 to 3 | 30 to 45 min |
| Warfield | 256 × 256 | 4 | 40 to 60 min |

One tile is about the width of a two-storey building (structures occupy 2×2 to 5×5 tiles and walls are one tile thick, see [21 · Buildings](21-buildings.md); an infantry regiment in line is about 6 tiles wide).

### Design rules for maps
- **Symmetry or fairness by construction.** 2-player maps are rotationally symmetric; 4-player maps are 4-way symmetric, 6-player maps are 6-way (or 3-way for 3v3) symmetric, or validated by the map tool (equal travel times from each start to resources and to the Heart).
- **Every rush route has a counter-route.** No base with a single entrance that can be walled off completely.
- **Readable from the minimap.** Ridges and rivers are visible and distinctive; seeps glow on the minimap.
- **Elevation creates battlefields.** Each map has at least two "set-piece" spots where a defensive line on high ground or at a ford makes a great Field Battle.

## Launch maps

| Map | Size | Concept |
|---|---|---|
| **Ribcage Pass** | Duel | Two bases separated by the Titan's ribs; three passes between them (narrow, middle ford, wide flank). The tutorial map's big brother. |
| **Hollow Lung** | Skirmish (2v2 or 3 FFA) | A vast cavern-lung opening to the sky in the centre; Heart in a bowl surrounded by Shrines on the rim. |
| **The Four Teeth** | Warfield (4 FFA or 2v2) | Four starts on four great fangs; the jawline between them is a ring road with hamlets; the Heart sits on the tongue. |
| **Marrowfen** | Skirmish | Bog-heavy lowlands rich in Marrow, poor in Grub; built for the Choir and Concord. |

Field Battle maps (smaller, 64 × 64 to 96 × 96, terrain-focused): **Ashen Ford** (river crossing), **Kite Hill** (a single dominant hill), **Shrine Steps** (terraced paddies, three victory points), **Ribcage Pass (battle cut)**.

## Grubfolk hamlets

Grubfolk are small, round, hard-working neutral farmers who live by seeps and have survived every war on the corpse by trading with all sides.

### Hamlet sizes
| Size | Market | Mercenaries | Garrison |
|---|---|---|---|
| Village | Grub↔Marrow | Grubfolk Militia (spears) | Light |
| Town | Grub↔Marrow, items | Militia, Slingers, a hired hero-for-a-while | Medium, palisade |
| Bazaar (1 per map, centre-ish) | Best prices, rare items | Exotic mercenaries (titan-spawn tamers) | Heavy, walls |

### Reputation
Each hamlet tracks a reputation per player: **Friendly**, **Neutral**, **Wary** or **Hostile** (Neutral is where every hamlet starts). Reputation is the hamlet's entry in the relations ledger that every party shares, built from fading memories of what you did ([18 · AI](18-ai.md#relations)), so the hamlet's banner tooltip can always say why it feels the way it does.
- Trading regularly nudges a hamlet towards Friendly (better prices).
- Attacking a hamlet's units, raiding it, or razing it makes it Hostile to you and every other hamlet Wary of you for 10 minutes.
- Hostile hamlets shoot at your units in range and will not trade.
- The Concord's Magnate can buy a hamlet outright (*Hostile Takeover*), making it an exclusive trade post.

### Raiding
Destroy the palisade (or the town hall for villages) and you loot its store: roughly 400 Grub and 100 Marrow for a village, more for towns. The hamlet becomes a ruin for 5 minutes, then rebuilds hostile to the raider. A razed Bazaar never rebuilds. Raiding is a strong tempo play that costs you every future deal.

## Creep camps (Titan-spawn)

| Creep | Habitat | Notes |
|---|---|---|
| Gutworms | Seeps, bogs | Burrow and ambush, weak alone |
| Bile Toads | Fords, bogs | Spit acid, slow |
| Marrow Golems | Marrow veins | Heavy, slow, drop Marrow on death |
| Rib Harpies | Ridges, cliffs | Flying creeps, harass |
| The Old Mouth (boss) | One per Warfield map | Huge; drops a powerful item and reveals the map briefly |

Creeps sleep at night on maps with a night lighting preset (purely as a scouting cue, no day/night cycle).

Creep behaviour is a handful of simulation rules, not an AI: a camp wakes when an enemy comes within its aggro radius, chases up to a leash distance, then walks home and heals. Titan-spawn are hostile to everyone but territorial, so they never go hunting ([18 · AI](18-ai.md#the-living-world-encounters)).

## Mercenary companies (planned, M4)

Besides the militia hamlets hire out, a few **mercenary companies** roam each map: a Grubfolk free company, a band of Titan-spawn tamers, a crew of disgraced ronin. Each has a captain, a personality, a small roster and a price. They camp at a hamlet or wander between them; you meet them, see their price and their opinion of you, and hire them for a fee plus upkeep. They fight for you while paid, may sell out to a higher bidder if their honour is low, and tell other companies who pays and who cheats. Their minds and contracts are described in [18 · AI](18-ai.md#deals-and-contracts).

## Frontier sites (planned, M4)

Places hidden in the fog that only reward whoever goes looking: a ruined waystation's cache, a lost caravan to rescue or rob, a hermit who sells the next Tremor's location, a Titan scar that reveals the land around it, a company camp, a relic barrow. The map marks slots; the match seed picks what appears in each, so a map the group knows still surprises. The first sighting of a site, hamlet or company is a ledger event (*first contact*). Full list and rules in [19 · Campaign and Exploration](19-campaign.md#frontier-sites).

## Map format and tooling

- Maps are data, not code: a heightmap, a tile-type layer, an object list (seeps, veins, hamlets, creeps, Shrines, start positions, Bloodseep sites, frontier site slots) and a small script block for map-specific events.
- Stored as a compact binary in the asset pack (a Warfield map is under 2 MB on disk; see [10 · Architecture](10-architecture.md#maps)).
- Built in an **internal map editor** that is just the game running in an edit mode (same renderer, extra tools), so there is no separate editor engine to maintain. Exposed to players later.
- **Map validator** (headless tool, `tools/gc_mapcheck`, built): checks start fairness (equal path lengths to resources and the Heart), unreachable areas, hamlet counts, and runs a few bot-free scripted economy simulations to compare income curves per start.
**Built so far (2026-10-15):** the validator is `tools/gc_mapcheck`, and the first two launch maps, Ribcage Pass and The Four Teeth, are in `data/maps/`. Run `cargo run -p gc_mapcheck -- check`; CI runs it on every pull request.

*The source format.* A map is a RON file of a few dozen lines, not a picture. It names a size class (Duel 128, Skirmish 192, Warfield 256 or, for five or six players, Grand 320 tiles), a symmetry (`Rot2` half a circle, `Rot4` a quarter circle, or `None`), a base ground height and a water line, then a list of **terrain stamps** (a disc or a thick line, a mode of Set, Raise or Lower to a height with a soft edge, all in sixteenths of a tile), **cover stamps** (woods, tall grass, and thicket: woods too dense to march through, which block like a cliff and are woods to the sim's eyes), optional **land paints** (the kind of land each tile is, painted in order: meadow, forest, fen, highland, waste or river valley; the renderer and the minimap colour the ground by it, and the rules don't read it) and an **object list** (start, seeps, veins, hamlets, creep camps, Shrines, Heart, Old Mouth, Bloodseep sites, frontier slots) with the player each belongs to. With a symmetry set the author writes only the first copy's share: stamps and objects are turned into copies and owners move on by the players over the copies (on a six-player `Rot2` map player 0's turned start belongs to player 3), so the result is exactly symmetric by construction (a test checks every tile). Objects marked `once` (the Heart and, on a Warfield map, the Old Mouth and the Bazaar) are placed once, a few tiles off a centre no tile sits on. Everything is integer arithmetic, so two machines build the same bytes; the loader's map hash is over the heights, water line and cover. The format and its loader now live in `crates/gc_map` (`def.rs`), which both `gc_mapcheck` and the game use. The game can open a described map with `--map NAME|FILE` (a shipped name or a file) to look round it in the sandbox; Field Battles and network matches play on the map the lobby chose ([Field Battle maps and recipes](#field-battle-maps-and-recipes) below).

*What the validator checks.* Each is an error unless noted.
- **Counts:** one start per player, exactly one Heart, one Old Mouth and one Bazaar on a Warfield map, 4 to 8 frontier slots, at least 3 Bloodseep sites.
- **Placement:** nothing stands outside the map, on a cliff, in deep water (or in a ford, apart from creep camps, frontier slots and Bloodseep sites), or on top of anything else.
- **Each start:** 2 seeps and 1 vein in its base area (24 tiles), 2 more seeps for its natural expansion (24 to 60 tiles), and a hamlet within 80 tiles of walking.
- **Reachability:** every object reachable from every start by regiments (clear of cliffs by a tile, as the sim keeps them), and no stretch of 150 or more walkable tiles cut off from all starts.
- **Fairness:** for each player the walking distance to its 4 nearest seeps, nearest vein, the Heart, nearest hamlet, 2 nearest contested nodes, nearest Shrine and nearest enemy start, and a first-pass economy (Grub plus Marrow gathered by minutes 5, 10 and 15, taking nodes nearest-first with the rates of [04](04-economy.md)) must agree within 3% between players. The economy part is a stand-in and will be replaced by the real sim economy in M3. Where a start stands (the Heart and the nearest enemy start) is compared only between starts the symmetry makes alike when there are more starts than turned copies: on a six-player `Rot2` map, each start with the one its turned copy stands on (player 0 with 3, 1 with 4 and 2 with 5 on the big Conquest maps), found by where they stand rather than by their numbers (*2026-10-17, for the big Conquest maps*). Everything else is compared across all of them.
- **Which checks:** a Field Battle map (Field size) is checked for placement, reachability and set pieces only; a Field-size map with a Heart is a big Conquest map and gets every check.
- **Rushes:** each base has a gate at least 12 tiles wide, so a short wall can't seal it (counted as the walkable tiles whose walk from the start costs 26 to 27.5 tiles, a ring through every way out; a rough measure that suits bases on plateaus and in bowls, to be tightened if a map needs it), and the route to the nearest enemy still has a counter-route, no more than 2.2 times as long, after walling off 6 tiles either side of it.
- **Set pieces:** the author lists at least two Field Battle spots (a hill half a tile above its surroundings, or a ford with 12 ford tiles around it) and each must really be one.

Path costs count ten per open tile, 14 for steep ground and 20 for fords. `gc_mapcheck png FILE OUT.png` draws a map with its objects so an author can see what the numbers describe.

*The two maps.* **Ribcage Pass** (Duel): bases in opposite corners behind cliff walls with a south gate and an east gate; across the middle a wall of cliffs with three ways through, the ford (the Heart on an island in it) and a narrow pass either side, and the open flanks past the ends of the wall as the wide way round; 33 objects, a hill and the ford as set pieces. **The Four Teeth** (Warfield): four fang plateaus with cliff edges and two ramps each, rivers to wade, rock spines that split the ground into lanes, villages, towns and contested seeps and veins guarded by creeps, and the tongue bowl in the middle with the Heart, the Old Mouth and the Bazaar; 87 objects, two hills and a ford as set pieces. Both are first versions with plain terrain (smooth stamps, no road or rib models yet) and the game opens them only with `--map` in the sandbox (the M2 netcode generates battle maps from a seed instead, so Conquest loading comes with M3 thread A/C).


- **Seeded generator (stretch):** procedural 2-, 4- and 6-player maps from a seed, guaranteed fair by the validator. Good for variety among a small friend group who will learn fixed maps quickly. The Field Battle recipes below are a first step, and the Conquest recipes are built too (two and four players, and six on the big Wilds maps, see [Conquest maps and recipes](#conquest-maps-and-recipes)). The campaign needs a smaller version early: a battle map generated from a campaign region's ground, route and feature ([19](19-campaign.md#battles-on-the-campaign)).

## Field Battle maps and recipes

*Built (random maps step 1, 2026-10-16).* Every Field Battle lobby has a map and a **map seed**, a 64-bit number. The map is made from the choice, the seed and the battle's size, so machines that agree on the three build the same map. These random maps are 256 to 896 tiles a side (see Map size below), not the 64 to 96 tile maps named under launch maps above.

**Choosing a map.** The host picks one of ten maps: **Proving Ground**, the seeded map every battle was played on before maps could be chosen; **Random: Pass**, **Random: Ford**, **Random: Hill** and **Random: Open**, one from each recipe below; **Random: Any**, which takes one of the four recipes from the seed; and the Wilds (below): **Land: Random**, as wild as the seed picks, which a new lobby starts on, and **Land: Plains**, **Land: Mixed** and **Land: Wild**. **New map** draws a fresh seed. The **map code**, `XXXX-XXXX-XXXX-XXXX`, is the seed in hex, which is how players share one: a code typed in (dashes, spaces and capitals don't matter) sets the seed. Everyone sees the code and, when the window has room, a small picture of the map with its six starts marked. Only the host can change the map, the seed or the code. In the lobby the host's map button shows just the map's name, which fits its column. Switching the mode to Conquest and back starts each mode on its first choice (Land: Random, Ribcage Pass).

**Map size.** *Built 2026-10-17 (big maps, step 1).* Mario asked for maps that feel like a world, several minutes' march across, so the host also picks a **Size** (in the lobby's header, beside the title), and the player count sets the side:

| Size | 2 players | 3 or 4 | 5 or 6 | Foot, edge to edge |
|---|---|---|---|---|
| Small (the old maps) | 256 | 256 | 320 | about 1.5 minutes |
| **Standard** (the default) | 512 | 640 | 768 | about 3 to 4 minutes |
| Large | 640 | 768 | 896 | about 3.5 to 5 minutes |

Foot marches about 3 tiles a second. On maps of 512 tiles or more the starts stand a sixth of the map in from the edges instead of a quarter, so the bases are far apart too: on a Standard map, two players start about 2 minutes 40 seconds' march apart. A map code makes a different map at each size and player count. On the big sizes (512 tiles and up, `BIG_MAP_TILES`) woods and grass grow with the map's area, so a big map is as dressed as a small one; the Small sizes keep the counts their maps always had, so old map codes still make the same maps. Most of making a big map is its rolling ground, which is worked out on every core at once (the same heights however many cores there are). What a big map costs is in the plan, /mnt/project-files/design/random-maps/big-world-maps-plan.md: about 100 MB at most on a 768 map, against the 6 GB limit. The terrain is drawn with less detail far from the camera, so a big map doesn't draw millions of triangles.

**Making a random map.** A random map's side comes from its size and its player count (above). A recipe writes the north half from the seed, and the loader turns it half a circle for the south (`Rot2`), so both sides get the same ground. Every recipe puts the six starts on the plateaus of the seeded map: the four corners, and the middles of the north and south. If the starts of a random map can't all walk to each other, the next seed is tried, so nobody is shown a broken map.

**The recipes.** Each one starts from rolling ground and adds its own shape:
- **Pass:** a cliff wall along the middle, with two to four gaps. One gap is a ford, and a rise behind another gap holds it.
- **Ford:** a river along the middle, with one to three fords and a rise on the bank above the first one.
- **Hill:** one big hill in the middle, with a knoll on either side of it, one to the north-west and one to the south-east.
- **Open:** no walls. Two rises stand in the north half (and their turned copies in the south), and there are more woods than on the other recipes.

Every recipe also scatters clumps of woods and tall grass over the north half, kept clear of the starts so armies deploy in the open, and names its set pieces: the hills and fords a line can hold.

**The Wilds.** *Built 2026-10-17 (big maps, step 2).* Mario asked for big maps that feel like a world, where you can't march straight from one side to the other: the way should wind through the land, round mountains, over rivers and past forest too dense to rush through, with plenty of open ground and walkable woods between. The Wilds recipe (`crates/gc_map/src/wilds.rs`) lays out the north half like this, and the loader turns it for the south:
- a **band** across the map between the north starts and the middle, with ways through it;
- on bigger maps (512 tiles and more), often a **middle band** along the middle line, whose turned copy carries it across the east half;
- **spurs** from the north edge down between the north starts, stopping short of the band, so neighbours go round their ends;
- **patches of land** over everything else, each about a quarter of the map across (128 tiles on a 512 map; a jittered grid, painted in a random order as lumps of overlapping discs, so the borders are ragged): **meadow** (tall grass, a few copses), **forest** (many walkable copses and a few thicket groves), **fen** (low wet ground, wadeable pools, a mere or two too deep to cross), **highland** (a raised table with cliffs and two or three ramps, and a rise on top named as a set piece; a broad hill where there's no room) and **bone waste** (open ground, rock spurs no one climbs, a low ridge);
- two knolls named as set pieces, and woods and tall grass everywhere;
- **sites** out in the land (*built 2026-10-17, big maps step 3*): Grubfolk villages, outposts (a company camp or an old fort), creep lairs and standing stones, one or two of each in the north half of a 512 map and more on bigger ones, each with ground round it that nothing blocks, floods or grows over, clear of the starts, the ways through and its own turned copy. A site goes where random draws find room, or on a crowded map anywhere clear on an eight-tile grid.

Each band or spur is one of three barriers: a **mountain range** (foothills all along, a sharp ridge with peaks too steep to climb, and passes through), a **river** (too deep to wade, with fords at the ways through; the first is named "The ford") or a **belt of thicket** (walkable woods either side and paths through). Nothing that blocks or floods goes on a way through, a start or a barrier, and no copse grows in water.

How wild a map is decides the rest:

| | Band's ways through | Middle band | Spurs | Mostly |
|---|---|---|---|---|
| **Plains** | 3 (4 on 640 and up), spread out | no | no | meadow and forest; barriers mostly rivers |
| **Mixed** | 2 between the starts (and one in the middle on 640 and up) | half the time | each half the time, sometimes with a pass | every kind of land |
| **Wild** | 2, between the starts | always, one way over below each corner start | both | forest and highland |

On a wild map every start turns aside to reach a way through, and the middle band's way over is back below the corner start, so a march from corner to corner zigzags. Measured over 10 seeds, the walk from a corner start to the opposite corner is about 1.4 times a straight line on a wild map (1.15 mixed, 1.1 plains), to the corner straight across about 1.45 times, and to the neighbouring start about 1.25 times. A small (256) map has no room for the middle band or the spurs, and the Wilds are not made smaller than 256 tiles (`wilds::MIN_TILES`).

In the game, each thicket tile carries a big dark tree and undergrowth, so a belt reads as a wall, and the minimap and the lobby's picture colour open ground by its land. In a Field Battle the sites are landmarks: the minimap and the lobby's picture mark each one, villages amber, outposts and lairs dark red, standing stones pale blue. They aren't drawn on the field yet (that waits for building art), and nothing in a Field Battle uses them yet; they are where neutral peoples, outposts and capture points will hook in.

**Tools and tests.** `cargo run -p gc_mapcheck -- gen RECIPE TILES SEED OUT.ron` writes the map a recipe makes from a seed, so a good one can become a hand-tuned map. `tools/gc_mapcheck/tests/recipes.rs` runs every recipe at both sizes through the validator for a dozen seeds, the Wilds at each wildness at 256, 512 and 768 for five seeds (a sweep of 30 seeds at five sizes passed), checks that a wild map's ways wind (`a_wild_maps_ways_wind`), and that every lobby map at the Standard and Large sizes joins its starts. For the Wilds, `gen` takes `Plains`, `Mixed` or `Wild` as the recipe, at 256 tiles or more, and `Conquest-Plains`, `Conquest-Mixed` or `Conquest-Wild` for the big Conquest maps below, at 512 tiles or more. The `gc_map` tests check that a seed always writes the same description, and pin the fingerprints of the random maps.

**Where the map is used.** A lobby battle is set out on the lobby's map, and its world is seeded with the map seed. The lobby settings carry the choice and the seed to every player ([11 · Networking](11-networking.md)). A replay stores the choice (its seed is the map seed), so it is drawn on the map it was played on ([11](11-networking.md#save-load-and-replays)).

## Conquest maps and recipes

*Built (random maps step 2, 2026-10-16).* Three recipes, **Teeth**, **Bowl** and **Fen**, write a whole Conquest map from a seed, at **Duel** (128 tiles, two players), **Skirmish** (192 tiles, two players) and **Warfield** (256 tiles, four players). Grand and Field sizes are not made. The lobby does not offer these maps yet, and Conquest is not playable yet; the Conquest mode thread wires them in later.

**Making a Conquest map.** A recipe writes the first player's share from the seed: the start, two seeps and a Marrow vein in the base, a natural expansion of two seeps, a Village, a contested seep and a contested vein each with a creep camp, a Shrine, Bloodseep sites and frontier slots. The loader turns that share round for the other players (`Rot2` for two, `Rot4` for four), so every start gets the same ground and the same economy. The Heart is placed once in the middle, and on a Warfield the Old Mouth and the Bazaar with it. Whether a map is fair is the checker's call: `gc_mapcheck` tries the seed, then the next, up to 16 seeds, and keeps the first map the checker passes.

**The recipes.** Each one adds its own shape to rolling ground:
- **Teeth:** each start sits on a high fang with cliff sides and two ramps down. Rock spines split the open ground into lanes, and the players meet at a wading place: a pool round the Heart's island on two-player maps, a stream between neighbours on a Warfield. A hill stands in the lanes.
- **Bowl:** a wide, shallow bowl in the middle, with the Heart on a raised tongue in it. The starts sit on the rim, with a hill on the rim either side of each.
- **Fen:** low wet ground, with wadeable pools, channels between them, and a deep mere or two that no one wades. The starts sit on dry islands, with a mound and plenty of reeds (tall grass) to hide in.

**Big Conquest maps.** *Built 2026-10-17 (big maps step 3).* For six-player Conquest on a big map, `gc_map::conquest::describe_wilds(tiles, seed, wildness)` makes a Wilds map (above) for Conquest, 512 tiles a side or more (`None` below that: six bases need the room), as wild as the wildness asks. It has the same land as a Field Battle Wilds map, with these differences:
- **Players.** Six: players 0 to 2 are the north starts west to east (corner, middle, corner), and 3 to 5 their turned copies.
- **Each start's share.** Every start gets the same share round it, at the same places, in its open clearing (out to about an eighth of the map's side, where the land's dressing doesn't reach):
  - the start;
  - two seeps and a Marrow vein within a dozen tiles;
  - a natural expansion of two seeps about 33 tiles out;
  - a Village, a Shrine, a Bloodseep site and a frontier slot;
  - three fields guarded by creep camps, a seep and a vein then another seep, so the first five expansions a start takes are its own.
- **The middle.** The Heart stands on flat open ground in the middle. When there is a middle band, it always has a way over there.
- **Sites.** The sites stand as on a Field Battle map. Each lair also guards a rich field: a seep and a vein beside it.

How fair it is:
- **Three against three.** North against south is fair, because the south is the north turned. (The Heart stands on the middle tile, half a tile off the centre the map turns about, so twins differ by about half a percent there.)
- **Every start's economy** is the same, near and far: the checker finds the seeps, the vein, the hamlet, the contested fields, the Shrine and the income within 0% to 2% of each other.
- **A game for all.** Where a start stands is not the same for all six. The middle two are nearer the Heart than the corners: about 12% nearer on wild and mixed maps, up to about 35% on plains. As on most six-player maps, a middle start has two neighbours near and a corner start one.

The game doesn't reroll these maps: every seed passes the checker. They are named "Conquest Wilds" and the seed. A sweep of 10 seeds of each wildness at 512, 640, 768 and 896 tiles (120 maps) passed, building and checking a 768 map in about 4 seconds. The Conquest mode thread wires these maps into the Conquest lobby.

**Tools and tests.** `cargo run -p gc_mapcheck -- gen RECIPE SIZE SEED OUT.ron` writes a Conquest map (Teeth, Bowl or Fen, at Duel, Skirmish or Warfield), and says so if it rerolled the seed; `gen Conquest-Wild 768 SEED OUT.ron` (or `Conquest-Plains`, `Conquest-Mixed`) writes a big one. `every_conquest_wilds_map_passes_the_checker` checks each wildness at 512 (two seeds) and 768 (one). `tools/gc_mapcheck/tests/recipes.rs` checks 50 seeds of each recipe at Duel, 25 at Skirmish and 12 at Warfield, and needs 80% of them to pass at each size. The `gc_map` tests check that a seed always writes the same description.

## Campaign battle maps

*Built (random maps step 3, 2026-10-16).* `gc_map::battle::battle_map` writes the battlefield of a campaign battle from a region: its ground, the route the attacker came along, at most one feature and maybe a Fort. The campaign ([19](19-campaign.md#battles-on-the-campaign)) will call it; the lobby does not offer these maps, and the campaign itself is not built yet.

**Making a campaign battle map.** A map is 192 to 320 tiles a side, near the size of a random Field Battle map (256 or 320), so a battle has room to manoeuvre: foot takes about one to two minutes to march across. Road and Burrow maps are 192 or 224, Pass and Ford maps 224 or 256 (the seed picks), and a Seat, a Fort or the Lungs add 64, up to 320. (The campaign doc first asked for 64 to 96; Mario chose Field Battle size on 2026-10-17.) The region's area, route, feature and Fort (yes or no) and a seed give the land, so the same region and seed always give the same map. Unlike a Conquest map, it is not rerolled: every kind of map passes the checker as made.

The map is not fair and not turned on purpose. The defender (player 0) holds the north, with its feature on its hill: a settlement's village on the slope facing the middle, a Great Place on the top. The attacker (player 1) deploys in the south, on the side it came from. With a Fort, the defender's start is on a hill inside an eight-sided palisade with a gate facing south. For now the palisade is a steep earth bank; real walls come with M3 buildings. Every map has at least two named high-ground set pieces.

Features become objects: a Seat is the defender's town, a Settlement a village, a CompanyCamp or a Lair a creep camp, a Site a frontier slot, and a GreatPlace a Shrine.

**The routes.** The route shapes the middle of the map:
- **Road:** a road four tiles wide through both starts, kept clear of woods.
- **Pass:** a cliff wall across the middle, with one or two gaps.
- **Ford:** a river across the middle, with one or two fords.
- **Burrow:** no road, and the attacker starts closer, as it comes up from a Brood burrow.

**The areas.** The area is the part of the Titan's body the region lies in, and it dresses the ground: **Crown** (higher ground and a crater lake), **Ribs** (rock ridges), **Lungs** (cavern walls down the east and west edges, and rock pillars), **Gut** (low boggy ground with wadeable pools and tall grass), **Spine** (high broken ground and steep rocks), **Hands** (deep forest), **Tail** (flat waste) and **Heart** (raised veins).

**Tools and tests.** `cargo run -p gc_mapcheck -- battle AREA ROUTE FEATURE fort|open SEED OUT.ron` writes one map, and `check` and `png` work on it as on any other. The test `every_campaign_battle_map_passes_the_checker` in `tools/gc_mapcheck/tests/recipes.rs` builds one seed of each of the 448 combinations (eight areas, four routes, seven features, with or without a Fort) and requires all of them to pass. A sweep of 20 seeds of each combination (8,960 maps) also passed when the maps were made Field Battle size, with no road on blocked ground. `a_forts_palisade_is_shut_but_for_its_gate` checks that no one walks out of a Fort except through its gate.

## Water (later)

Launch maps treat water as terrain: shallow fords are walkable and slow, deep water is impassable except to air. Naval play (ships, coastal raids, sea seeps) comes with the naval expansion. The map format reserves a water-depth layer from day one so old maps upgrade cleanly.

**Built so far (M1 step 6):** the placeholder map's walkable ground comes from its heights: ground rising more than 1.2 tiles per tile and water deeper than 0.35 tiles are blocked; slopes over half that and shallow fords are walkable but routes avoid them. Since the terrain rules landed, fords also slow the march to 60% and leave waders open to heavier blows, and high ground hits harder (see [05 · Combat](05-combat.md#terrain)); steep slopes don't slow anyone yet.

**Built so far (six-player lobbies, 2026-10-16):** the generated map has six flat start plateaus: the four corners (north-west, north-east, south-west and south-east, at the quarter points) plus north-centre and south-centre, in the middle of the map's north and south quarter lines. Generated maps changed as a result.
