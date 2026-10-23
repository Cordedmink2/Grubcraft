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
| 2 | **Living touches without models.** Fireflies at dusk over fens and wet woods, pollen and drifting seeds in meadows, mist pooling on low ground, dust devils in the waste, leaves in the wind. Particle recipes on a habitat sample of the ground round the camera, as the sound beds do ([09](09-art-and-audio.md)). | Particles, `Map.habitat` (done), lighting looks (done) |
| 3 | **Busy bases** (idea 10). Chimney and cook-fire smoke from finished buildings, lit windows and torches at dusk, sparks from smithies, workers walking between a base's buildings, log piles and stacked supplies beside the halls, carts and porters on the road from a seep to its granary. The faction decides what it is: Ember Clans' hearth smoke and lanterns, Concord bone carts and green fire, Choir resonance motes, Brood pulses and tending larvae. | Stage 1, building models' anchor points, the animation engine for a proper carrying walk |
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

- A log pile and a stump where a tree stood (stumps are the ground thread's, T8, [32](32-changing-land.md#what-a-cleared-tile-becomes); log piles come with stage 3). Until then a hauled tree shrinks away.
- A proper axe clip (the animation engine), and the thud and crack of a tree (the soundscape thread's hit sounds).
- Faction twists ([32](32-changing-land.md#faction-twists)): burning, shattering and eating are built by the changing-land thread; their pictures (fire on a burning tree, crystal shards, Gnawers chewing) join this stage once they exist.
