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
| **Terrain lanes** | Ridges (ribs), forests, fords, roads, bogs that shape where armies meet |

### Map sizes

| Size | Tiles | Players | Typical length (Standard) |
|---|---|---|---|
| Duel | 128 × 128 | 2 | 20 to 30 min |
| Skirmish | 192 × 192 | 2 to 3 | 30 to 45 min |
| Warfield | 256 × 256 | 4 | 40 to 60 min |

One tile is about the width of a two-storey building (structures occupy 2×2 to 5×5 tiles; an infantry regiment in line is about 6 tiles wide).

### Design rules for maps
- **Symmetry or fairness by construction.** 2-player maps are rotationally symmetric; 4-player maps are 4-way symmetric or validated by the map tool (equal travel times from each start to resources and to the Heart).
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
Each hamlet tracks a reputation per player: **Friendly**, **Wary** or **Hostile**.
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

## Map format and tooling

- Maps are data, not code: a heightmap, a tile-type layer, an object list (seeps, veins, hamlets, creeps, Shrines, start positions, Bloodseep sites) and a small script block for map-specific events.
- Stored as a compact binary in the asset pack (a Warfield map is under 2 MB on disk; see [10 · Architecture](10-architecture.md#maps)).
- Built in an **internal map editor** that is just the game running in an edit mode (same renderer, extra tools), so there is no separate editor engine to maintain. Exposed to players later.
- **Map validator** (headless tool): checks start fairness (equal path lengths to resources and the Heart), unreachable areas, hamlet counts, and runs a few bot-free scripted economy simulations to compare income curves per start.
- **Seeded generator (stretch):** procedural 2- and 4-player maps from a seed, guaranteed fair by the validator. Good for variety among a small friend group who will learn fixed maps quickly.

## Water (later)

Launch maps treat water as terrain: shallow fords are walkable and slow, deep water is impassable except to air. Naval play (ships, coastal raids, sea seeps) comes with the naval expansion. The map format reserves a water-depth layer from day one so old maps upgrade cleanly.

**Built so far (M1 step 6):** the placeholder map's walkable ground comes from its heights: ground rising more than 1.2 tiles per tile and water deeper than 0.35 tiles are blocked; slopes over half that and shallow fords are walkable but routes avoid them (they don't slow the march yet).
