# 25 · Scenery models

Scenery is the trees, bushes, grass and rocks that dress the field. This document covers its 28 3D models (26 placed by land, and the two gathering spots of Conquest): what they are, the lands they grow on, how they are made and packed, and what they do not do. The look itself is described in [09 · Art and Audio](09-art-and-audio.md).

## What it is

Each model is one mesh with vertex colours (the paint is on the mesh itself, with its ambient occlusion already in). There are no textures. Each model has four levels of detail: LOD0 is the closest and most detailed, LOD3 the farthest and simplest. Units are tiles, with 1 Blender metre to a tile, and a soldier is about 1.6 tall.

- Trees and rocks (not pebbles) cast shadows. Bushes and grass do not.
- The wind bends each model by height, so the tops move and the base stays put. The game computes the weights when the pack is baked, so the models need no rig.
- The game places them by the map's land, its cover (woods, tall grass and thicket) and its sites. It draws them in instanced batches, nearest first.
- **Decoration only.** Scenery never changes the simulation. The `gc_sim` crate never reads it, so replays are the same with or without it.

## The models

Lands are the land sets in `crates/gc_game/src/scenery.rs`. Height is the `height` in `facts.json`, in tiles. Triangles are per level of detail, from `facts.json`.

| Model | Family | Lands | Height | LOD0 | LOD1 | LOD2 | LOD3 |
|---|---|---|---|---|---|---|---|
| `oak-a` | tree | Meadow, Valley | 4.54 | 2,436 | 873 | 290 | 50 |
| `oak-b` | tree | Meadow, Forest | 4.49 | 2,297 | 873 | 291 | 50 |
| `pine-a` | tree | Forest | 5.50 | 804 | 492 | 232 | 58 |
| `pine-b` | tree | Forest, Highland | 6.00 | 692 | 420 | 192 | 58 |
| `thicket-fir` | tree | Thicket, on any land | 6.50 | 1,047 | 684 | 264 | 70 |
| `birch` | tree | Meadow, Valley | 4.47 | 2,161 | 873 | 291 | 64 |
| `willow` | tree | Fen | 3.81 | 2,156 | 872 | 291 | 44 |
| `snag` | tree | Fen, Waste | 3.04 | 1,610 | 872 | 290 | 50 |
| `crag-pine` | tree | Highland | 3.45 | 1,693 | 873 | 290 | 63 |
| `bone-spire` | tree | Waste | 3.81 | 2,028 | 872 | 291 | 72 |
| `leafy-bush` | bush | Meadow, Valley, Fen | 0.92 | 484 | 242 | 86 | 28 |
| `fern` | bush | Forest | 0.66 | 130 | 104 | 78 | 14 |
| `heather` | bush | Highland | 0.49 | 660 | 202 | 85 | 24 |
| `thorn-scrub` | bush | Waste | 0.79 | 678 | 231 | 84 | 25 |
| `glowcap` | bush | Forest, Fen | 0.60 | 550 | 242 | 88 | 24 |
| `grass-clump` | grass | Meadow, Valley, Forest | 0.38 | 130 | 55 | 21 | 7 |
| `tall-grass` | grass | Any land (on Fen tall-grass tiles, reeds are used instead) | 1.00 | 156 | 57 | 21 | 8 |
| `reeds` | grass | Fen, and the shores of Fen and Valley | 0.93 | 148 | 58 | 18 | 8 |
| `dry-grass` | grass | Highland, Waste | 0.37 | 125 | 50 | 21 | 8 |
| `wildflowers` | grass | Meadow, Valley | 0.34 | 140 | 60 | 22 | 8 |
| `pebbles` | rock | Every land | 0.25 | 252 | 116 | 38 | 12 |
| `boulder` | rock | Meadow, Highland | 0.90 | 400 | 338 | 116 | 40 |
| `mossy-boulder` | rock | Forest, Fen | 1.10 | 400 | 338 | 116 | 40 |
| `crag` | rock | Highland | 2.41 | 800 | 260 | 80 | 39 |
| `slab` | rock | Highland, Waste | 0.53 | 210 | 140 | 48 | 39 |
| `bone-ribs` | rock | Waste | 1.74 | 538 | 338 | 116 | 34 |
| `grub-seep` | rock | Conquest gathering spot, not placed by land | 0.36 | 942 | 339 | 80 | 39 |
| `marrow-vein` | rock | Conquest gathering spot, not placed by land | 1.10 | 755 | 301 | 90 | 40 |

Sway (how far the top moves in a gust, at scale 1) is 0.12 tiles for trees, 0.05 for bushes, 0.10 for grass and none for rocks.

## How each land is dressed

Woods are the cover tiles marked as woods. On about 55 in 100 of them a tree stands, and about one in four gets an undergrowth bush. Open ground gets grass, pebbles, the odd bush and rock, and sometimes a lone tree. Thicket tiles are different: each gets a dark fir, now and then (about 15 in 100) the land's own tree, and one or two undergrowth bushes.

- **Meadow:** oaks and birches in the woods, leafy bushes as undergrowth. Open ground has grass clumps and wildflowers, the odd boulder and pebbles.
- **Forest:** pines and one oak in the woods, ferns and glowcaps as undergrowth. Open ground has grass clumps, pebbles and mossy boulders.
- **Fen:** willows and snags in the woods, leafy bushes and glowcaps as undergrowth. Open ground is wet: reeds, some tall grass, mossy boulders, and lone willows and snags. Reeds also grow on the shore.
- **Highland:** crag pines and pines in the woods, heather as undergrowth. Open ground is the rockiest land, with dry grass, boulders, slabs, crags, pebbles and lone crag pines.
- **Waste:** snags and bone-spires in the woods, thorn scrub as undergrowth. Open ground is bare: dry grass, slabs, bone-ribs and pebbles, with lone snags and bone-spires. It has the least grass.
- **Valley:** birches and oaks in the woods, leafy bushes as undergrowth. Open ground has grass clumps, wildflowers and reeds on the shore. It has no rocks.
- **Tall grass** cover (grass a unit can stand waist-deep in) gets tall grass on every land except Fen, where it gets reeds.

Bushes, rocks and lone trees keep off near sites. The radius depends on the site: 24 tiles round a start, 9 round the Heart, 8 round a village, town or bazaar, and 3 to 7 for the smaller sites. Grass and pebbles grow right up to them.

## Gathering spots

Conquest's gathering spots (the economy's nodes, see [04 · Economy](04-economy.md)) are drawn with two of these models, one per node, where the simulation says the node is:

- **`grub-seep`:** a Grub seep. A domed pool of honey-gold ooze in a dark earth crater, with bubbles, burst rings, drips over the lip and a few curled grubs on the rim.
- **`marrow-vein`:** a Marrow vein. Three broken bone shafts burst from a flat earth mound, with pinkish-cream marrow cores at the breaks, bone chips and marrow nodules round them.

A full spot is drawn at 1.2 times its baked size, about 2.5 tiles across (workers gather within 2 tiles of its centre). It shrinks to 40% of that as it runs down, is drawn a little brighter than baked while it holds, and is dark once only dregs are left. Each spot is turned by where it lies, so no two look the same way. They are culled and given levels of detail like big rocks, but they are always drawn first and at any distance. `gc_game::scenery::Scenery::set_nodes` takes them each frame. If the pack lacks either model, the game falls back to the old glowing boxes. Their Blender scripts are `seep.py` and `vein.py`, and their brief is `/mnt/project-files/design/scenery/NODES-BRIEF.md`.

## Pipeline

1. **Blender scripts.** The models are built in Blender from code, using Blender's Python module (`bpy`), in the shared folder `/mnt/project-files/design/scenery/source/`. `skit.py` is the shared kit. `trees.py`, `trees_more.py`, `bushes.py`, `grass.py` and `rocks.py` build the models of each family, and `seep.py` and `vein.py` the gathering spots. Each script takes `--only <name>` to rebuild a single model.
2. **The glb.** Each model is written to `/mnt/project-files/assets-src/grubcraft/scenery/<name>/<name>.glb`. The file holds four joined meshes, `<name>_LOD0` to `<name>_LOD3`, with the origin at the base and vertex colours only. Beside it sit `facts.json` (name, family, height, radius and triangles per level) and `review.png`, which shows the four levels at one scale.
3. **The model list.** `assets-src/scenery.ron` lists every model in pack order: its name, its glb (`shared:grubcraft/scenery/<name>/<name>.glb`), its family and its sway.
4. **The bake.** `cargo run --release -p gc_scenebake` reads the list and the glbs and writes `assets/scenery.pak` and `assets/scenery.manifest`. It refuses a level over its family's triangle budget (tree 2,500 / 900 / 300 / 80, bush 700 / 250 / 90 / 30, grass 160 / 60 / 24 / 8, rock 1,000 / 350 / 120 / 40, pebbles 300 / 120 / 40 / 12). It also refuses a level with more triangles than the one before it, and a model that reaches past 8 tiles from its origin. `--allow-missing` skips glbs that are not made yet, for development.
5. **The check.** CI runs `cargo run --release -p gc_scenebake -- --check`. It compares the committed pack with its manifest without baking anything.
6. **Commit.** Commit the new pack and manifest with the change to `scenery.ron` or a glb. Never merge the pack by hand: if two branches change it, merge, keep both sides' entries in `scenery.ron`, bake again and commit the new pack.

The game embeds `assets/scenery.pak` at build time. The pack is read by `gc_assets::scenery`, drawn by `gc_render`, and placed by `gc_game::scenery`. Its architecture is in [10 · Architecture](10-architecture.md#scenery).

## Known limits

- **No flying or draping.** A model stands at the terrain height under its base. It does not follow slopes.
- **Faces are double-sided.** Leaves and blades may be single planes, because the game draws both faces.
