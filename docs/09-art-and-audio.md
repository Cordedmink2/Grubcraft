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

The vetted shortlist, licences and gaps are in [16 · Art Sourcing](16-art-sourcing.md).

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

### Audio sourcing
CC0 / Creative Commons effects libraries for prototyping, a couple of royalty-free music packs, then custom where it matters. Voice lines can be recorded by friends, which suits a personal game nicely.
