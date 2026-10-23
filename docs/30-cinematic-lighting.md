# 30 · Cinematic lighting

*Plan from the "Grubcraft Lighting Study" (2026-10-22); stage 1 built 2026-10-23. The illustrated study is the "Grubcraft Lighting Study" page (https://claude.ai/artifact/MwkJpkNDwSN9XY9Kry8QVf), with the throwaway prototype and its screenshots in the project's shared folder under `design/cinematic/`.*

Players judge a scene by how the light behaves long before they count polygons. Grubcraft was lit like a flat noon in thick haze, so it read grey. This plan changes the light, the air and the colour, with the same models, and keeps everything render only: nothing here touches the simulation, so golden replays and multiplayer sync are unaffected. It is the "lighting presets per map, rim light, colour-graded look, shared particle atlas" that [09 · Art and Audio](09-art-and-audio.md) already promises.

## The stages

1. **Light and Air (built, 2026-10-23).** See [Built: stage 1](#built-stage-1).
2. **Clean edges.** Anti-aliasing, better soft shadows, contact shadows under soldiers, water that reflects the sky; hit flash, trampled ground and light motes.
3. **Dust and Smoke (built, 2026-10-23).** See [Built: stage 3](#built-stage-3). One GPU particle system with a shared atlas: dust trails, hit sparks, Brood ichor, Choir crystal shards, Concord ghost-light motes, smoke, fire, embers, debris. [26 · Fortifications](26-fortifications.md) stage 7 and several other threads use it, so it comes first in wave 2.
4. **Camera craft.** Switchable tilt-shift depth of field, sun shafts for the ride camera, charge shake, battle-start letterbox and slow motion, the director camera and replay films.
5. **Weather and night.** Rain, snow, river fog banks at dawn; night maps lit by torches and Grub.

Stages 4 and 5 are not built, and stage 2 is partly built (the camera tilt and backlit foliage came with stage 1; clean edges, hit flash, trampled ground and light motes follow). Not on the list on purpose: ray tracing, screen-space reflections, volumetric fog volumes, temporal anti-aliasing (it smears hundreds of thin spears) and real-time global illumination; each costs a lot on a Mac for little gain in a top-down game.

## Built: stage 1

The light is one `Look` (`gc_render/src/look.rs`): plain numbers for the sun's direction and colour, the sky and ground ambient light, the air (colour, haze toward the sun, density, how fast it thins with height), rim light, how much leaves glow, cloud shadows, ground occlusion, and the grade (exposure, contrast, saturation, vignette, film grain, a cool tint for the dark end and a warm one for the bright end), and the colours of the sky. `Renderer::set_look` takes one; nothing is read from files.

**Five prepared looks** (`LookPreset`), the game's default being Golden hour:

| Look | What it is |
|---|---|
| Noon | A high sun in clear air: short shadows, light haze |
| Golden hour | A low warm sun behind the field (about 21° up), blue shade, long shadows, warm haze toward the sun, slow cloud shadows, strong rim light |
| Overcast | Soft grey light, thick air, few shadows, desaturated |
| Storm | Steel light, fast dark cloud shadows, low colour, high contrast, heavy vignette |
| Dusk | The sun just above the hills, violet shade, an orange sky |

**What draws it:**

- **Sky.** One triangle at the far plane, drawn last in the world pass (so the HDR image keeps its 4 bytes a pixel on a Mac, and the depth test keeps it off every pixel the world covers): a gradient from the horizon colour to the colour overhead, a glow and a disc round the sun. At the usual bird's-eye tilt it hardly shows; it is there at the horizon in the ride camera, close zoom, and in the void past a map's edge.
- **Height fog.** Replaces the flat distance fog that greyed everything. The air is thick in low ground and thin on hills (density falls off with height, integrated along the view ray) and is warmer toward the sun.
- **Film curve and grade.** The composite pass applies a filmic curve (the ACES fit) to the HDR image, then split toning, saturation, contrast and a vignette. A little film grain (strongest in the mid tones) and a one-step triangular dither run every frame, which also removes colour banding in the sky.
- **Cloud shadows.** Soft patches drifting over the land (the `sunlight` function in `world.wgsl`).
- **Rim light** on soldiers and buildings, strongest where the sun is behind them.
- **Backlit foliage.** Leaves and blades (scenery that sways in the wind) glow yellow-green when the sun is behind them.
- **Ground occlusion.** Valleys and the foot of slopes get less light. It is baked once per map from the heights alone (`look::ground_occlusion`: 16 horizon samples a tile, in parallel, about 0.1 s on a 768-tile map) into a one-byte-per-tile texture, 0.6 MB on the biggest map.
- **Zoom-linked camera tilt** (`gc_game/src/camera.rs`). The bird's-eye camera looks down at 55° from the default zoom out to the widest, and eases to 30° at the closest zoom, so zooming in brings the scene toward eye level and the sky starts to show. The unit viewer and the showroom keep their own pitch.

**Using it:** `--look noon|golden-hour|overcast|storm|dusk` opens with a look; **F9** steps to the next one while playing (it is in the Marshal's Scroll under View). Colours that are drawn as they appear (fog, haze, sky) are written in sRGB in a `Look` and converted to HDR by `Look::hdr`, the inverse of the film curve, so they land near what was written.

**Cost.** No new full-screen pass: the sky is a triangle inside the world pass and everything else is folded into the world and composite shaders that already ran. The bloom is a little stronger than before to suit the new curve. Memory: the occlusion texture (up to 0.6 MB) and a 48-byte grade buffer.

## Built: stage 3

One particle system for every effect, in `gc_render` (`particles.rs`, `effects.rs`, `shaders/particles.wgsl`). Render only: nothing here touches the simulation, and the game never makes an effect for something the viewer cannot see.

**How it works.** The CPU never moves a particle. `Renderer::spawn_particles(&[Particle])` gives each one its start, velocity, forces (`gravity`, `drag`, `lift`), size and colour at birth and at death, shape and flags, once. They go into a ring buffer of `MAX_PARTICLES` (16,384) 64-byte slots, the oldest overwritten first (1 MB), and the vertex shader works out where each particle is from its age, in closed form (constant force with drag), so a burst costs the CPU nothing after it is spawned. Particles age by the **effects clock**, which follows the frame clock and stops when the game is paused or behind the menu and runs slow in slow motion (`Renderer::set_effects_speed`); `Particle::age` back-dates one that began between frames.

**What draws it.** One instanced draw (two when the ring wraps) of the slots that may still hold a live particle (the ring tracks when each batch is certainly dead: a life is capped at 8 seconds), four-vertex strips, in the world pass after the sky, tested against the world's depth and writing none (so there is no extra pass or render target). Each particle is a camera-facing quad (spinning, growing, fading in fast and out slowly), or with `STRETCH` a streak along its velocity with its head at the particle. Pictures come from one 512 × 256 single-channel atlas (128 KB with mips) made in code at start-up (`particles::atlas`): four ragged puffs, a soft glow, a streak, a crystal shard and a ring. The lighting is the world's: the sun with its shadows and the cloud shadows sampled at the particle's centre, sky light, a sphere-shaded face so puffs have a lit and a dark side, a glow when the particle is between the eye and the low sun, then the height fog, and a fade where the fog of war hides the ground. Blending is premultiplied: dust and smoke blend by alpha, and `ADDITIVE` particles (fire, sparks, motes, shards) add HDR light, which the glow then blooms. Flags: `STRETCH`, `ADDITIVE`, `GROUND` (come to rest on the ground), `SOFT` (fade out toward the ground under the particle, in place of a depth fade, which would need a second pass), `WIND` and `FLICKER`.

**Recipes** (`Effects`, which collects particles for the frame; `take()` hands them to the renderer): `footstep`, `dust_burst`, `smoke`, `flames`, `embers`, `sparks`, `debris`, `spray`, `shards`, `motes`, and `hit(pos, dir, HitStyle, armoured, power)` which picks by whose wound it is: **Flesh** (blood drops; Ember Clans and Iron Concord), **Ichor** (yellow-green drops and motes; the Brood), **Crystal** (glittering shards; the Choir), **Ghost** (pale motes; the Concord's summoned dead) and **Ember** (embers and sparks). Recipes thin themselves with `set_density` (the Graphics preset: High 1, Balanced 0.75, Battery 0.35) and draw their randomness from their own generator, never the simulation's.

**In the game** (`gc_game/src/effects.rs`): marching soldiers kick up dust (about a puff a second at a walk, more at a run, from every soldier on screen), arrows raise a puff and a few clods where a volley comes down, and a charge that hits home rolls a cloud and throws clods. Effects of regiments the viewer cannot see are not made. Building hits, fire, smoke and collapse use them since stage 7 part 1 of [26](26-fortifications.md) (`gc_game/src/siege.rs`, with the recipes `chips`, `trail` and `dust_cloud` added for it). Sparks and wounds of soldiers ([28](28-soldier-combat.md) stage 6) and the ambient motes are still to come. `--fx-demo` plays every effect in a row at the camera's focus at a quarter speed, and `--fx-still` freezes the same gallery at four ages for screenshots (`gc_game/src/fx_demo.rs`).

**Cost.** One draw call, 1 MB of ring buffer and 0.2 MB of atlas. Vertex work is 4 vertices per slot with a terrain height lookup and a few shadow taps; the fragment cost is overdraw, so smoke is made of few, large, faint puffs. About 1,000 puffs of dust in a big battle is the expected load; the ring and the density keep the worst case bounded. Measured on a Mac when Mario tries it.

**Limits.** No soft depth fade against buildings and scenery, only against the ground (a second pass would be needed). Dust colour is one tan; it will follow the ground tint of the land later.

## Next in stage 1's family

Not yet built, planned for the following pull request: the game choosing a look from a map's lands (dusty amber haze over the Waste, humid green mist in the Fen, cold clear air on the Highland, golden hour as the default) and a lobby "Time of day" option, a live tuning panel for tuning on the Mac, and a "Clarity" setting that turns off the vignette, grain and heavy grading for competitive play. Readability comes first: team colours, selection rings and fog of war are checked under every look.

## Overlaps with other plans

- **Living Land** ([27](27-living-land.md)): each land should bring its own light; the occlusion bake should run right after the generator, and grown valleys and erosion are what occlusion and height fog flatter.
- **Fortifications** ([26](26-fortifications.md)) stage 7 and the battle effects need stage 3's particles.
- **Performance** ([12](12-performance.md)): the whole of stage 1 is below 0.3 ms on an M-series Mac by estimate (not measured here: the cloud container has no real GPU); looks are tuned by eye on Mario's Mac.
