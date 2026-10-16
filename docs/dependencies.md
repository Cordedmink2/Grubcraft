# Dependencies

Every third-party crate the game uses, why, and what it costs. Policy: see [10 · Architecture](10-architecture.md#dependency-policy). Adding a crate means adding a row here in the same pull request. CI also runs `cargo deny` (`deny.toml`), which checks advisories, licences, bans and sources. A licence, ban or source problem fails a pull request; an advisory does not block one, but it fails the weekly run.

## Shipped in the game

| Crate | Used by | Why | Cost and notes |
|---|---|---|---|
| `wgpu` 30 | `gc_render` | One GPU API over Metal (Mac), DirectX 12 (Windows) and Vulkan | The largest dependency. Built with only the `metal`, `dx12`, `vulkan`, `wgsl` backends/frontends; GL and WebGPU are off |
| `winit` 0.30 | `gc_game` | Windows, input, Mac trackpad gestures | 0.31 is still in beta; move when it's stable |
| `glam` 0.34 | `gc_render`, `gc_game`, `gc_ai`, `gc_map` | Vector/matrix maths for rendering, the camera, the computer players' tactics and the map's drawing views (floats, never in the sim) | Small, SIMD |
| `bytemuck` 1 | `gc_render`, `gc_game`, `gc_assets`, `gc_data` | Safe casting of instance data to GPU bytes | Tiny |
| `pollster` 1 | `gc_game` | Blocks on wgpu's async setup calls; avoids an async runtime | Tiny |
| `xxhash-rust` 0.8 (`xxh3` only) | `gc_sim`, `gc_assets` | Fast, well-distributed per-tick state hashes for desync detection; asset ids | Pure Rust, `no_std` |
| `cpal` 0.16 | `gc_audio` | Sound-card output: CoreAudio (Mac), WASAPI (Windows), ALSA (Linux) | Built with default features off, so no JACK or ASIO. On Linux the build needs the ALSA headers (`libasound2-dev`); CI installs them. Mixing, decoding and voice limits are ours |
| `ron` 0.10, `serde` 1 | `gc_ui`, `gc_ai`, `gc_map` | Read and write the player's settings file; read the computer players' personalities (`data/ai/personalities.ron`, embedded in the executable and parsed once); read map descriptions (`data/maps/`, embedded the same way) | Small; also used at build time, below |
| `blake3` 1 (`pure`) | `gc_net`; `gc_bake` at build time | Keys for each link (`crypt.rs`), the check in join codes (`joincode.rs`), and a hash of all game data that the lobby compares so every player runs identical data | `pure` feature: no C or assembly |
| `chacha20poly1305` 0.10 (`alloc`) | `gc_net` | Encrypts and authenticates every packet on a link (`crypt.rs`), on LAN and over Tailscale | Adds 24 bytes to each packet (an 8-byte number and a 16-byte tag). The key exchange is not authenticated; see `crypt.rs` for what that does and does not stop |
| `x25519-dalek` 2 (`static_secrets`, `zeroize`) | `gc_net` | Key exchange when a player joins (`crypt.rs`) | A fresh key pair for each connection |
| `getrandom` 0.3 | `gc_net` | Random bytes for each new key pair (`crypt.rs`) | Reads the system's random source |
| `lz4_flex` 0.14 (`std`, `safe-encode`, `safe-decode`; no frame format) | `gc_replay`, used by `gc_game` | Compresses replay files, as [11 · Networking](11-networking.md#save-load-and-replays) planned. Pure Rust, no `unsafe` with these features. The game records and plays replays with it |
| `tracy-client` 0.18 | `gc_game` (`profile` feature only) | Frame and tick profiling with Tracy | Off by default; when on, builds Tracy's C++ client. Not in normal builds |

## Build time only (baker)

These run in `gc_bake` and in `gc_game`'s build script (and in the game itself only in a `--features dev` build, for the sandbox's data reload). `ron` and `serde` also ship, for the settings file, the AI personalities and the map descriptions (above). `blake3` ships too, through `gc_net` (above).

| Crate | Why | Cost and notes |
|---|---|---|
| `ron` 0.10 | Parses the human-edited game data in `data/*.ron` | Small |
| `serde` 1 (`derive`) | Maps RON onto typed definitions with unknown-field errors; also saves the player's settings (`gc_ui`) | Proc-macro build cost only |

`tools/gc_artbake` (the model baker) also uses `ron` and `serde`, and the crates below, none of which reach the game:

| Crate | Why | Cost and notes |
|---|---|---|
| `gltf` 1.4 (`utils`, `names`; default features off) | Reads the glTF and GLB model files | Pure Rust |
| `meshopt` 0.6 | Simplifies meshes for the four levels of detail and orders triangles for the GPU cache | Builds meshoptimizer's C++ with `cc`, so the tool needs a C++ compiler |
| `png` 0.17 | Decodes model textures so vertex colours can be sampled from them | Pure Rust |
| `intel_tex_2` 0.5 | BC7-compresses material textures | Prebuilt ISPC kernels for Linux, macOS and Windows on x86_64 and arm64, so no ISPC install is needed. MIT/Apache-2.0. Tool-only, never in the game |
| `base64` 0.22 | Decodes glTF buffers embedded as data URIs | Pure Rust |

`tools/gc_mapcheck` (the map check that CI runs) depends only on `gc_map` and `gc_sim`; `gc_map` reads the RON map format with `ron` and `serde`, listed above.

## Development only

| Crate | Used by | Why |
|---|---|---|
| `naga` 30 (`wgsl-in`) | `gc_render` tests | Validates shaders in CI, where there is no GPU |

## Bundled assets

| Asset | Used by | Licence |
|---|---|---|
| Soldier models (`assets/models.pak`) | Soldiers on the field | CC0: the Yari Ashigaru made for Grubcraft on Quaternius's Farmer, Quaternius (Ranger, Alien, Orc Skull) and CDmir (Alien Bug); listed in `CREDITS.md` and on the in-game credits screen |
| Sound effects and music loops (`assets/audio/sounds.gcsnd`) | Game audio | Synthesised by `tools/gc_sfx`; original work, no third-party licence |
| Glyphs rasterised from DejaVu Sans Mono (`crates/gc_render/src/debug_font.rs`) | Dev overlay text | Bitstream Vera / DejaVu licence: free to use, modify and redistribute, including embedded; the fonts' names may not be reused for modified versions. Copyright (c) 2003 Bitstream, Inc. (Bitstream Vera); DejaVu changes are in the public domain |

## Deliberately not used

- **Async runtimes** (`tokio`, `async-std`): lockstep networking needs one UDP socket and a thread.
- **Engines and ECS frameworks** (`bevy`, `hecs`, ...): see [ADR 0001](adr/0001-engine-and-language.md).
- **`rand`**: the sim uses its own PCG32 in `gc_fixed`, so the random stream can never change under us with a crate update.
- **`libc`, `windows-sys`, `mach2`** for memory stats: the three OS calls we need are declared directly in `gc_platform::memory`.
