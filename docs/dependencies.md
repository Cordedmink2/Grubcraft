# Dependencies

Every third-party crate the game uses, why, and what it costs. Policy: see [10 · Architecture](10-architecture.md#dependency-policy). Adding a crate means adding a row here in the same pull request.

## Shipped in the game

| Crate | Used by | Why | Cost and notes |
|---|---|---|---|
| `wgpu` 30 | `gc_render` | One GPU API over Metal (Mac), DirectX 12 (Windows) and Vulkan | The largest dependency. Built with only the `metal`, `dx12`, `vulkan`, `wgsl` backends/frontends; GL and WebGPU are off |
| `winit` 0.30 | `gc_game` | Windows, input, Mac trackpad gestures | 0.31 is still in beta; move when it's stable |
| `glam` 0.34 | `gc_render`, `gc_game`, `gc_ai` | Vector/matrix maths for rendering, the camera and the computer players' tactics (floats, never in the sim) | Small, SIMD |
| `bytemuck` 1 | `gc_render` | Safe casting of instance data to GPU bytes | Tiny |
| `pollster` 1 | `gc_game` | Blocks on wgpu's async setup calls; avoids an async runtime | Tiny |
| `xxhash-rust` 0.8 (`xxh3` only) | `gc_sim`, `gc_assets` | Fast, well-distributed per-tick state hashes for desync detection; asset ids | Pure Rust, `no_std` |
| `cpal` 0.16 | `gc_audio` | Sound-card output: CoreAudio (Mac), WASAPI (Windows), ALSA (Linux) | Built with default features off, so no JACK or ASIO. On Linux the build needs the ALSA headers (`libasound2-dev`); CI installs them. Mixing, decoding and voice limits are ours |
| `ron` 0.10, `serde` 1 | `gc_ui`, `gc_ai` | Read and write the player's settings file; read the computer players' personalities (`data/ai/personalities.ron`, embedded in the executable and parsed once) | Small; also used at build time, below |
| `tracy-client` 0.18 | `gc_game` (`profile` feature only) | Frame and tick profiling with Tracy | Off by default; when on, builds Tracy's C++ client. Not in normal builds |

## Build time only (baker)

These run in `gc_bake` and in `gc_game`'s build script. `blake3` never reaches the game executable; `ron` and `serde` do, for the settings file and the AI personalities (above).

| Crate | Why | Cost and notes |
|---|---|---|
| `ron` 0.10 | Parses the human-edited game data in `data/*.ron` | Small |
| `serde` 1 (`derive`) | Maps RON onto typed definitions with unknown-field errors; also saves the player's settings (`gc_ui`) | Proc-macro build cost only |
| `blake3` 1 (`pure`) | Hash of all game data, compared by the lobby so every player runs identical data | `pure` feature: no C or assembly |

`tools/gc_artbake` (the model baker) also uses these; none of them reach the game either:

| Crate | Why | Cost and notes |
|---|---|---|
| `gltf` 1.4 (`utils`, `names`; default features off) | Reads the glTF and GLB model files | Pure Rust |
| `meshopt` 0.6 | Simplifies meshes for the distant level of detail and orders triangles for the GPU cache | Builds meshoptimizer's C++ with `cc`, so the tool needs a C++ compiler |
| `png` 0.17 | Decodes model textures so vertex colours can be sampled from them | Pure Rust |
| `base64` 0.22 | Decodes glTF buffers embedded as data URIs | Pure Rust |

## Development only

| Crate | Used by | Why |
|---|---|---|
| `naga` 30 (`wgsl-in`) | `gc_render` tests | Validates shaders in CI, where there is no GPU |
| `lz4_flex` 0.14 (`std`, `safe-encode`, `safe-decode`; no frame format) | `gc_replay` (the golden replays, `gc_bench`, `gc_game` tests) | Compresses replay files, as [11 · Networking](11-networking.md#save-load-and-replays) planned. Pure Rust, no `unsafe` with these features. Ships in the game once it records replays (M2) |

## Bundled assets

| Asset | Used by | Licence |
|---|---|---|
| Soldier models (`assets/models.pak`) | Soldiers on the field | CC0: Quaternius (Ninja, Ranger, Alien, Orc Skull) and CDmir (Alien Bug); listed in `CREDITS.md` and on the in-game credits screen |
| Sound effects and music loops (`assets/audio/sounds.gcsnd`) | Game audio | Synthesised by `tools/gc_sfx`; original work, no third-party licence |
| Glyphs rasterised from DejaVu Sans Mono (`crates/gc_render/src/debug_font.rs`) | Dev overlay text | Bitstream Vera / DejaVu licence: free to use, modify and redistribute, including embedded; the fonts' names may not be reused for modified versions. Copyright (c) 2003 Bitstream, Inc. (Bitstream Vera); DejaVu changes are in the public domain |

## Deliberately not used

- **Async runtimes** (`tokio`, `async-std`): lockstep networking needs one UDP socket and a thread.
- **Engines and ECS frameworks** (`bevy`, `hecs`, ...): see [ADR 0001](adr/0001-engine-and-language.md).
- **`rand`**: the sim uses its own PCG32 in `gc_fixed`, so the random stream can never change under us with a crate update.
- **`libc`, `windows-sys`, `mach2`** for memory stats: the three OS calls we need are declared directly in `gc_platform::memory`.
