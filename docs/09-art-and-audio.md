# 09 · Art and Audio Direction

The art direction has two jobs: look great from a bird's-eye camera, and cost almost nothing in memory. These turn out to be the same job.

## Visual style: "painted diorama"

Stylized low-poly with rich lighting, as if the battlefield were a hand-painted tabletop diorama lit by a late-afternoon sun.

- **Flat-shaded, chunky shapes** with strong silhouettes. Detail comes from *form and light*, not texture.
- **Palette texturing:** most models carry no unique texture at all. Each vertex points into a shared **palette texture** (a small gradient atlas, for example 256 × 16). A whole faction's units and buildings share one palette. This is the biggest single memory saving in the game: hundreds of models, a few kilobytes of colour.
- **Team colour** is a palette row swap, free at render time.
- **Lighting does the heavy lifting:** one sun with soft shadows, hemispheric sky/ground ambient, a rim light to pop units off the terrain, a subtle colour-graded LUT per map, and emissive Grub that glows.
- **Terrain** gets the only "real" textures: a small set of tiling materials (soil, hide, grass, rock, bone, bog, Grub-soaked earth) blended by a splat map, plus painted decals.
- **Readability over realism:** soldiers are slightly oversized relative to buildings (classic RTS proportion), heads and weapons slightly exaggerated so you can tell spears from swords from the default zoom.

### Faction looks
| Faction | Shapes | Palette | Signature visual |
|---|---|---|---|
| Ember Clans | Rectangular, layered roofs, banners | Lacquer red, black, gold, rice-paper cream | Banners (sashimono) on every regiment, ember-lit rockets |
| Brood | Round, segmented, organic | Bruise purple, chitin amber, Grub-glow green | Pulsing veins across the ground, cocoons |
| Choir | Triangular, faceted, floating parts | Pale glass, cyan and violet light | Light triangles over territory, refracting beams |
| Carrion Concord | Tall, narrow, ornate, hanging lanterns | Bone white, verdigris, ledger-ink blue, lantern orange | Long caravans, ghost-lights over battlefields |

### The world
- The Titan is everywhere at large scale: ridges are visibly ribs, lakes sit in old eye-sockets, forests grow like fur. At play scale it reads as natural terrain with an unsettling hint.
- Grub glows a warm amber-green and pulses slowly. Seeps are the brightest thing on the map, so economy is readable from far zoom.
- Lighting presets per map (dawn, noon, dusk, overcast, moonlit) rather than a day/night cycle.

### Animation
- Soldiers animate through **baked vertex animation textures** (VAT, see [10 · Architecture](10-architecture.md#rendering)). Each unit type has idle, walk, run, attack (2 variants), hit, die (2 variants), rout-run and cheer.
- Formation motion is *procedural*: soldiers step into gaps, jostle, and turn individually with small random delays so blocks look alive rather than rigid.
- **Soldier-on-soldier fighting** (asked for by Mario on 2026-10-13, inspired by Total War and Samurai Wars): every soldier in a fighting front picks an opponent and is seen to fight him, rather than a block of soldiers bobbing in place. It comes in three stages, all on screen only (the battle maths never changes, see [10 · Architecture](10-architecture.md#the-two-tier-simulation)):
  1. **Duel pairs and living formations.** Each front-rank soldier locks onto the nearest enemy across the contact line; the two face each other, swing at offset times and drift a step forward or back as they trade blows. When one falls, a neighbour shuffles in to take his place and his partner. Formations get the small movements that make a block look alive: soldiers speed up and slow down instead of hitting full speed at once, a moving block's front sets off a moment before its back, a stopping block overshoots slightly and settles, and a waiting regiment shifts its weight, sways and looks about. Uses the clips we already bake. **Built** (see [10 · Architecture](10-architecture.md#the-two-tier-simulation)).
  2. **Blocks, hits and kills: duels that read the numbers** (Mario's pick MB-2, 2026-10-15). Each paired soldier swings once a round, and every blow is decided with the simulation's own odds, so the stats show without a tooltip. A blow lands with the sim's chance to hit (50 + attack − the target's defence with its formation bonus, kept between 10% and 90%, and lower for a wavering regiment or big blades in close order); a man it lands on staggers back and loses his next swing. Otherwise he parries it with his weapon or, with a shield, takes it on his shield, unless the striker is heavy (a monster, cavalry, or a big swinging blade, which counts one class heavier): each class of mass he has over his man gives a 25% chance to **crush through** the guard and knock him down. A heavy regiment's **charge** knocks men off their feet (40% for each class of mass over the foe) or staggers them. A knocked-down soldier falls, lies a moment and gets up (the death clip played forward, then back; models without one lie flat). When the simulation takes a soldier from a regiment in melee, the one who falls is a man locked in a duel, and **his partner swings the killing blow**: he stands until it lands. **Built** (see [10 · Architecture](10-architecture.md#the-two-tier-simulation)). Today's stand-in models stagger with their hit clip and duck to block (the Quaternius "Duck" clip, baked as the new `Block` clip kind); a shield-raise clip comes with the faction art kits.
  3. **Matched kills.** Paired animations where killer and victim move together (as in Medieval II), kept for heroes, charges, cavalry and the Challenge duel, where the camera is likely to be close. These need custom animation work and come with the art pass.
- **Fidelity:** the CC0 models are a baseline. Mario wants high-fidelity, high-quality soldiers in the end. Nothing above depends on the model: pairing and motion move whatever skeleton they are given. Smooth skinning is built (engine upgrade step 1): each vertex follows up to four weighted bones, skeletons may have up to 63 bones (enough for a full humanoid rig with fingers), and vertices carry a texture coordinate and, when the source model has one, a tangent, ready for texture and normal maps. Textured soldiers are built too (step 2): a model may bring painted maps (albedo, team mask, normal, ambient occlusion, roughness, metalness, emitted light), which the baker shrinks to 512 × 512 with mips, and the soldier shader lights them with highlights that follow roughness. The first final-quality model is in: the Yari Ashigaru, made in Blender on Quaternius's Farmer body and rig (8,400 triangles, 1,024 maps baked from a 109,000-triangle sculpt; sources and scripts in the shared folder under `design/faction-looks/ashigaru-test/`). The whole Carrion Concord roster (14 models) is now built the same way: see [22 · Concord models](22-concord-models.md). Shadows and glow are built too (step 3): the sun casts soft shadows from soldiers, spears, banners, trees and hills, and the frame is drawn in HDR so the brightest light (sun glints on metal, emitted light such as Grub) glows. Four detail levels and BC7 texture compression are built too (engine upgrade step 4), so a textured material now costs about 1 MB in the pack rather than 4 MB. What is still to come is more final-quality models. None of it touches the fighting or motion code. The extra memory is well inside the 4 GB budget.
- Deaths leave bodies for a while (they are also a Brood and Concord resource), then sink into the Titan.

### Effects
- Particles are GPU-instanced billboards from one shared atlas. Strict budget per effect.
- Signature effects: Grub splash, ember rockets, acid arcs, light beams that visibly split, spectral raise-dead swirls.

## Where art comes from (practical)

This is a personal project, so the art pipeline must be realistic:

1. **Prototype on free CC0 low-poly kits** (for example Quaternius and Kenney packs: low-poly, animated, permissively licensed). Their style matches the palette approach and they are fine for non-commercial use (CC0 is fine for any use).
2. **Blender as the source of truth.** A Python export script bakes meshes, palette indices and VAT animations straight into our formats.
3. **Procedural where possible:** terrain, foliage scattering, Grub seeps, chord triangles, veins, fortifications built from modular pieces.
4. **Custom models over time** for faction-defining units and heroes.

The vetted shortlist, licences, gaps and download status are in [16 · Art Sourcing](16-art-sourcing.md); credits are in [`CREDITS.md`](../CREDITS.md).

See [14 · Decisions Needed](14-decisions-needed.md) for the open question of who makes custom art.

## Audio direction

Audio carries a surprising amount of RTS readability: you hear a battle start, a worker idle, a building finish.

### Music
- One theme per faction plus neutral map themes; **adaptive layers** that add percussion and brass as battle intensity rises (measured from combat in view and supply lost per minute).
- Clans: taiko, shakuhachi, shamisen. Brood: wet percussion, clicking rhythms, droning choir. Choir: glass harmonica, bowed metal, pure tones. Concord: funeral brass, bells, harpsichord.
- Streamed from disk (see memory budget), never fully decoded in RAM.

### Sound effects
- Layered battle ambience: the sound of a clash scales with the number of models fighting, using a few looping layers rather than hundreds of individual sounds.
- **Voice budget:** at most 32 simultaneous voices, prioritised by distance to camera and importance.
- Positional audio relative to the camera's ground point.

### Unit voices
- Warcraft-style acknowledgements ("Hai!", chitinous chitters, a bell-tone, a ledger stamp), with the classic easter-egg lines after repeated clicking. Short, characterful, compressed.

### Built so far
**Art (M1 step 8):** soldiers are real animated models. `tools/gc_artbake` reads the glTF files listed in `assets-src/models.ron` (from the downloaded packs in the shared project folder, or small CC0 files kept in the repo), scales each to its height on the field, picks out the team-colour parts, makes a reduced mesh for distance, samples each animation at 12 frames a second into a bone animation texture, and writes `assets/models.pak`, which is embedded in the game, with `assets/models.manifest` beside it (the baker and model schema versions, a hash of `models.ron`, of every source file and of the pack, with the sources that live only in the shared folder marked as not in the repo). CI runs `cargo run -p gc_artbake -- --check`, which fails if the committed pack, `models.ron` or a source the repo holds no longer matches the manifest; it cannot look at the shared sources, so it reports how many it skipped, and a change to one shows only when someone with the shared folder re-bakes. After changing `models.ron`, a source or the baker, re-bake and commit the pack and manifest together. A test bakes the repo's CC0 Alien Bug in memory and checks that bone weights sum to 255, every level of detail is within its budget and two bakes are byte-identical. Today the Yari Ashigaru is a final-quality model made for Grubcraft (see Fidelity above), and the Ember Clans roster is being built the same way (Blender sources and scripts in the shared folder's `design/faction-looks/ember-roster/`): the Yumi Ashigaru, Katana Samurai, Hands, The Ronin, The Onmyoji, the Mounted Samurai, the Yabusame Riders and The Daimyo are in the pack, and so is the whole Carrion Concord roster (14 models, made the same way; see [22 · Concord models](22-concord-models.md)). The Yumi Ashigaru's bow clips are Quaternius's Ranger clips moved onto the Farmer rig, and a 63rd bone draws its string and shows the nocked arrow only while it aims (the bone shrinks to nothing otherwise). The Onmyoji's casting clips are Quaternius's Wizard clips moved the same way, and his 63rd bone turns the wheel of talismans behind his shoulders (it burns out as he dies). The three Ember riders sit on Quaternius's CC0 Horse (Ultimate Animated Animals): the rider's upper body and the horse share one skeleton (58 bones, 59 for the Yabusame's bowstring), the rider's legs are posed into the stirrups, and each riding clip pairs a horse clip (walk, gallop, rear, death) with a rider clip, the rider leaning into the gallop and slumping forward as the horse falls. Models for units not yet in `data/units.ron` are only seen in the showroom (`--showroom`). Two models still stand in: Quaternius's Alien (Shardlings) and CDmir's Alien Bug (Skitterlings), both CC0. Each plays idle, walk, run, two attacks, shoot, a hit reaction, a block (a duck; the Yari Ashigaru guards with his sword-ready idle instead) and death (where the model has them; a cheer clip is baked for later), and the fallen stay on the field in their death pose. They are stand-ins until the faction kits in [16 · Art sourcing](16-art-sourcing.md) are chosen; changing a unit's model is one line in `models.ron` and a re-bake. Credits are on the in-game credits screen (`C`).

**Sound:** the first sound pass is in the game. Every sound is synthesised by `tools/gc_sfx` (see [assets/audio/README.md](../assets/audio/README.md) for the catalogue and how to regenerate): volleys, melee clashes, the war-drum charge, the wavering tone, the rout horn, the shatter crash, the rally horn, three UI clicks, and a calm and a battle music loop in the Ember Clans' style (flute, drone, taiko, plucked string). The two loops play in step and crossfade as regiments engage. Sounds follow the camera: quieter with distance, panned left or right. Keys: M mutes, N toggles the music; `--no-audio` skips the sound card. Faction themes, unit voices and positional battle-ambience layers are still to do.

### Audio sourcing
The free-sound sites were not reachable when the first pass was made, so the prototype sounds are generated instead, which also means they carry no licence obligations. The Kenney CC0 packs are now downloaded (`assets-src/kenney/`) and ready to audition. Recorded CC0 or royalty-free sounds can replace any of them later; record each in the table in `assets/audio/README.md`.

CC0 / Creative Commons effects libraries for prototyping, a couple of royalty-free music packs, then custom where it matters. Voice lines can be recorded by friends, which suits a personal game nicely.
