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

*The source format.* A map is a RON file of a few dozen lines, not a picture. It names a size class (Duel 128, Skirmish 192 or Warfield 256 tiles), a symmetry (`Rot2` half a circle, `Rot4` a quarter circle, or `None`), a base ground height and a water line, then a list of **terrain stamps** (a disc or a thick line, a mode of Set, Raise or Lower to a height with a soft edge, all in sixteenths of a tile), **cover stamps** (woods and tall grass) and an **object list** (start, seeps, veins, hamlets, creep camps, Shrines, Heart, Old Mouth, Bloodseep sites, frontier slots) with the player each belongs to. With a symmetry set the author writes only the first player's share: stamps and objects are turned into copies and owners move on, so the result is exactly symmetric by construction (a test checks every tile). Objects marked `once` (the Heart and, on a Warfield map, the Old Mouth and the Bazaar) are placed once, a few tiles off a centre no tile sits on. Everything is integer arithmetic, so two machines build the same bytes; the loader's map hash is over the heights, water line and cover. The format and its loader now live in `crates/gc_map` (`def.rs`), which both `gc_mapcheck` and the game use. The game can open a described map with `--map NAME|FILE` (a shipped name or a file) to look round it in the sandbox; Field Battles and network matches still play on seeded maps, and choosing a map in the lobby comes with random maps step 1.

*What the validator checks.* Each is an error unless noted.
- **Counts:** one start per player, exactly one Heart, one Old Mouth and one Bazaar on a Warfield map, 4 to 8 frontier slots, at least 3 Bloodseep sites.
- **Placement:** nothing stands outside the map, on a cliff, in deep water (or in a ford, apart from creep camps, frontier slots and Bloodseep sites), or on top of anything else.
- **Each start:** 2 seeps and 1 vein in its base area (24 tiles), 2 more seeps for its natural expansion (24 to 60 tiles), and a hamlet within 80 tiles of walking.
- **Reachability:** every object reachable from every start by regiments (clear of cliffs by a tile, as the sim keeps them), and no stretch of 150 or more walkable tiles cut off from all starts.
- **Fairness:** for each player the walking distance to its 4 nearest seeps, nearest vein, the Heart, nearest hamlet, 2 nearest contested nodes, nearest Shrine and nearest enemy start, and a first-pass economy (Grub plus Marrow gathered by minutes 5, 10 and 15, taking nodes nearest-first with the rates of [04](04-economy.md)) must agree within 3% between players. The economy part is a stand-in and will be replaced by the real sim economy in M3.
- **Rushes:** each base has a gate at least 12 tiles wide, so a short wall can't seal it (counted as the walkable tiles whose walk from the start costs 26 to 27.5 tiles, a ring through every way out; a rough measure that suits bases on plateaus and in bowls, to be tightened if a map needs it), and the route to the nearest enemy still has a counter-route, no more than 2.2 times as long, after walling off 6 tiles either side of it.
- **Set pieces:** the author lists at least two Field Battle spots (a hill half a tile above its surroundings, or a ford with 12 ford tiles around it) and each must really be one.

Path costs count ten per open tile, 14 for steep ground and 20 for fords. `gc_mapcheck png FILE OUT.png` draws a map with its objects so an author can see what the numbers describe.

*The two maps.* **Ribcage Pass** (Duel): bases in opposite corners behind cliff walls with a south gate and an east gate; across the middle a wall of cliffs with three ways through, the ford (the Heart on an island in it) and a narrow pass either side, and the open flanks past the ends of the wall as the wide way round; 33 objects, a hill and the ford as set pieces. **The Four Teeth** (Warfield): four fang plateaus with cliff edges and two ramps each, rivers to wade, rock spines that split the ground into lanes, villages, towns and contested seeps and veins guarded by creeps, and the tongue bowl in the middle with the Heart, the Old Mouth and the Bazaar; 87 objects, two hills and a ford as set pieces. Both are first versions with plain terrain (smooth stamps, no road or rib models yet) and the game opens them only with `--map` in the sandbox (the M2 netcode generates battle maps from a seed instead, so Conquest loading comes with M3 thread A/C).


- **Seeded generator (stretch):** procedural 2-, 4- and 6-player maps from a seed, guaranteed fair by the validator. Good for variety among a small friend group who will learn fixed maps quickly. The campaign needs a smaller version early: a battle map generated from a campaign region's ground, route and feature ([19](19-campaign.md#battles-on-the-campaign)).

## Water (later)

Launch maps treat water as terrain: shallow fords are walkable and slow, deep water is impassable except to air. Naval play (ships, coastal raids, sea seeps) comes with the naval expansion. The map format reserves a water-depth layer from day one so old maps upgrade cleanly.

**Built so far (M1 step 6):** the placeholder map's walkable ground comes from its heights: ground rising more than 1.2 tiles per tile and water deeper than 0.35 tiles are blocked; slopes over half that and shallow fords are walkable but routes avoid them. Since the terrain rules landed, fords also slow the march to 60% and leave waders open to heavier blows, and high ground hits harder (see [05 · Combat](05-combat.md#terrain)); steep slopes don't slow anyone yet.

**Built so far (six-player lobbies, 2026-10-16):** the generated map has six flat start plateaus: the four corners (north-west, north-east, south-west and south-east, at the quarter points) plus north-centre and south-centre, in the middle of the map's north and south quarter lines. Generated maps changed as a result.
