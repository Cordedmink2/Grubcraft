# 16 · Art Sourcing: Vetted Free Assets

Candidate free assets for the Ember Clans and the Brood, ready for M1 step 8 (real art). Companion to [09 · Art and Audio](09-art-and-audio.md#where-art-comes-from-practical).

**How this was vetted.** Pages were read on 2026-10-12 (licence lines, formats, contents). The build container's network blocks the asset sites, so **nothing has been downloaded or opened yet**: formats, animation clip names and poly counts below are what the pages say, not what the files contain. Every item needs a quick check at download time (licence line still reads the same, files import into Blender). Keep a copy of each licence page alongside the files in `assets-src/`.

## Findings in one paragraph

Free CC0 art gets us **placeholder-grade fantasy soldiers, monsters, castles, terrain textures and sounds** right away. It does **not** get us a rigged samurai, Japanese buildings, banners, or a convincing hive: no CC0 3D versions were found. Those become the first "custom" models (see [14 · Decisions Needed](14-decisions-needed.md)), or we reskin CC0 bases through the palette system, which is already the plan (flat colour via palette rows makes a fantasy knight read as an armoured ashigaru with a new helmet and a banner).

## Licence rules we follow

- **CC0** (Kenney, KayKit, Poly Haven, ambientCG, OpenGameArt items marked CC0): no credit needed, usable commercially, can be modified and shipped inside the game.
- **Never accept** CC-BY, CC-BY-SA, "free for personal use", or pages with no licence line, even if the model is perfect. Several good samurai, spider and beetle models were rejected for this (list below).
- **Quaternius caveat.** Each Quaternius pack page says CC0, but quaternius.com/license.html now describes a "Quaternius Asset License v1.0" (updated 2026-08-28) that allows free commercial use without credit but **forbids redistributing the raw assets**. Newer packs (for example the Bestiary kit) are explicitly under it; older packs say CC0 on their own page. Either way, shipping them inside a game is fine. The only real difference is that we must not publish the raw files by themselves. **Rule: the repo keeps only the baked game pack and a manifest, never the raw Quaternius files.** This also holds for the Universal Animation Library (QAL, not CC0) and for Mixamo (see below).

## Ember Clans (samurai)

| Need | Candidate | Source / licence | Notes |
|---|---|---|---|
| Ashigaru, samurai bodies (rigged, animated) | **Ninja** | [Poly Pizza, Quaternius](https://poly.pizza/m/mCNoqcqpvC), CC0 | Closest to Japanese. Tagged animated; rig unconfirmed |
| | **Lowpoly Animated Knight** | [OpenGameArt, Quaternius](https://opengameart.org/content/lowpoly-animated-knight), CC0 | Confirmed rig with idle, walk, run, roll, death. Reskin via palette |
| | **LowPoly RPG Characters** (6) | [OpenGameArt](https://opengameart.org/content/lowpoly-rpg-characters) / [Quaternius](https://quaternius.com/packs/rpgcharacters.html), CC0 | FBX, OBJ, Blend, glTF |
| | **Ultimate Modular Men** (11 characters, swappable parts, 24 animations) | [Quaternius](https://quaternius.com/packs/ultimatemodularcharacters.html), CC0 | Best bet for many unit types from shared parts (helmets, armour, weapons). glTF |
| | **Ultimate Animated Characters** (50+) | [Quaternius](https://quaternius.com/packs/ultimatedanimatedcharacter.html), CC0 | FBX, OBJ, Blend |
| | **KayKit Adventurers** | [itch.io](https://kaylousberg.itch.io/kaykit-adventurers), CC0 | Free tier has 5 characters with accessories; 75 animations in the Source tier. FBX, glTF |
| | Warrior | [Poly Pizza, mastjie](https://poly.pizza/m/Z6ZUtm6kc1), CC0 | Rig unconfirmed |
| Monk (Onmyōji hero stand-in) | Monk | [Poly Pizza, Quaternius](https://poly.pizza/m/P5FzFYgG5z), CC0 | |
| Weapons (yari, katana, bow) | Modular Weapons Pack | [Quaternius](https://quaternius.com/packs/medievalweapons.html), pack page label unverified | Check licence line at download |
| Torii gate | Torii Gate | [Poly Pizza, Quaternius](https://poly.pizza/m/7SyXZ62xR5), CC0 | Static; the only Japanese 3D piece found |
| Castle, walls, towers, village | **Castle Kit** (75) | [Kenney](https://kenney.nl/assets/castle-kit), CC0 | Western shapes; layered roofs and banners must be modelled on top |
| | **Fantasy Town Kit** (160) | [Kenney](https://kenney.nl/assets/fantasy-town-kit), CC0 | Houses, walls, props |
| | Retro Medieval Kit | [Kenney](https://kenney.nl/assets/retro-medieval-kit), CC0 | Download link missing from page |
| | Medieval Village, Modular Medieval Buildings | Quaternius (`medievalvillage`, `modularmedievalbuildings`) | Licence line to confirm |
| Siege (ram, rocket cart) | Not found | | Build from props in the kits above |
| Banners (sashimono), kite | Not found | | Simple quads; model ourselves |

## The Brood (insects)

| Need | Candidate | Source / licence | Notes |
|---|---|---|---|
| Skitterlings, Chitin Guard bodies | **Ant (rigged, animated)** | [OpenGameArt, mujtabaio](https://opengameart.org/content/ant-3d-model-rigging-animated-low-poly-ish), CC0 | Only confirmed CC0 insect. `.blend` only, walk clip listed |
| Monsters, Colossus Maw, Reclaimer stand-ins | **Ultimate Monsters** (50, animated) | [Quaternius](https://quaternius.com/packs/ultimatemonsters.html), CC0 | Index tags mention bee and demon; contents to be checked |
| | Easy Enemy Pack (5) | [Quaternius](https://quaternius.com/packs/easyenemy.html), CC0 | Index tags mention bee, wasp, spider; unconfirmed |
| | Cute Animated Monsters (21) | [Quaternius](https://quaternius.com/packs/cutemonsters.html), CC0 | Tone is too cute for the Brood, maybe for Gnawers |
| | Animated Monster Pack (4) | [Quaternius](https://quaternius.com/packs/animatedmonster.html), CC0 | |
| Hive, Pustule, cocoon, veins | Not found | | Procedural: ovoid blobs plus the vein decals from [09](09-art-and-audio.md). Cheapest to build ourselves |
| Moths, Mantid Reapers, Shellbacks | Not found | | Model ourselves in Blender (insect bodies are easy in low poly) |

## Shared: terrain, sky, sound

| Need | Candidate | Licence | Notes |
|---|---|---|---|
| Soil, ground | ambientCG Ground 054; Poly Haven `forest_ground_04` | CC0 | 1K to 8K JPG/PNG. We ship 1K to 2K BC-compressed |
| Grass | ambientCG Grass 001 | CC0 | |
| Rock | ambientCG Rock 035; Poly Haven `aerial_grass_rock` | CC0 | |
| Hide, bone, bog, rice paddy | Not yet picked | CC0 | Search ambientCG for Leather, Bone, Peat/Mud, Clay and check each licence line |
| Sky lighting | Poly Haven HDRIs | CC0 | |
| Trees, rocks, foliage | Kenney Nature Kit (330) | CC0 | Quaternius Stylized Nature MegaKit as alternative |
| Impact, weapon, footstep sounds | Kenney Impact Sounds (130), RPG Audio (50) | CC0 | Pages show `.ogg` only; WAV unconfirmed |
| UI sounds | Kenney Interface Sounds (100) | CC0 | |
| Music | Not found | | Needs a separate pass (see [09](09-art-and-audio.md#audio-sourcing)) |

## Animation

- Clips come bundled with the character packs above (idle, walk, run, attack, death in the best cases). The pipeline needs idle, walk, run, 2 attacks, hit, 2 deaths, rout-run and cheer per unit type (see [09](09-art-and-audio.md#animation)); expect to fill gaps by retargeting.
- **Quaternius Universal Animation Library** is a good retarget source but is under the Quaternius licence, not CC0 (repo rule above applies).
- **Mixamo** (Adobe) allows royalty-free use in games, but not redistributing the raw files, and it is a download-only service with an Adobe login. Usable as a source for baked clips; keep raw FBX out of the repo.

## Rejected (not CC0)

| Asset | Why |
|---|---|
| [3D Lowpoly Samurai with Animations](https://opengameart.org/content/3d-lowpoly-samurai-with-animations-fbx-format) (null-painter-error) | CC-BY 4.0. Best samurai found; could be used with credit if Mario accepts a credits screen, or ask the author for a CC0 / paid licence |
| Beetle Golem [Animated] | CC-BY-SA 3.0 |
| Spider [Animated] (PantherOne) | CC-BY 3.0 |
| Pagoda (Poly by Google), Torii (Hattie Stroud), Shrine (Aidan K McLaughlin) | CC-BY 3.0 |
| Ant on itch.io (GameIdea Studio), Rigged Alien, Alien Hive | No licence named |
| Moth-ify Me! | Non-commercial, credit required |
| Poly Pizza / Sketchfab bulk searches | Licence varies per model; accept only after reading each model page |
| Synty, CGTrader, Fab, TurboSquid packs | Paid or proprietary |

## What to do next

1. **Download and verify** on a machine with open network access (Mario's Mac, or a cloud environment that allows `quaternius.com`, `kenney.nl`, `poly.pizza`, `opengameart.org`, `ambientcg.com`, `polyhaven.com`, `kaylousberg.itch.io`). Record file hashes and the licence line in an `assets-src/MANIFEST.md`.
2. **Import test:** one Quaternius character through `gc_bake` into a bone animation texture and onto screen, before choosing the whole set.
3. **Commission or model** the gaps: samurai kit parts (kabuto helmet, armour, sashimono banner), Japanese roofs, hive and cocoon pieces, siege carts.
4. **Decision for Mario:** accept a credits screen for CC-BY art (opens up the best samurai model), or stay strictly CC0.
