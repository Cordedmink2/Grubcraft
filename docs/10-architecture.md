# 10 · Technical Architecture

This is the engineering plan for an RTS that is *absurdly* lightweight without looking or playing cheap. The engine choice and its rationale are in [ADR 0001](adr/0001-engine-and-language.md); budgets and measurement are in [12 · Performance](12-performance.md).

## Guiding principles

1. **Budget first.** Every subsystem has a memory and time budget before it has code. Exceeding a budget is a failing test, not a TODO.
2. **Simulate little, show a lot.** The deterministic simulation tracks hundreds of regiments; the screen shows thousands of soldiers. The expensive-looking part is presentation, which can be approximate.
3. **Send inputs, not state.** Lockstep networking means bandwidth and replays scale with *player actions*, not army size.
4. **Allocate once.** All gameplay memory is reserved at match load from fixed-capacity pools. Zero heap allocations per simulation tick, verified by tests.
5. **Data on disk is data in memory.** Assets are stored in GPU-ready, zero-copy layouts and memory-mapped, so loading is mostly the operating system paging bytes in.
6. **Own the stack, keep it small.** A small number of well-chosen libraries for windowing, GPU access and audio output; everything game-specific is ours and sized to fit.

## Stack

| Layer | Choice | Why |
|---|---|---|
| Language | **Rust** (stable) | Native speed, no garbage collector, memory safety, excellent cross-compilation, great for deterministic code |
| Windowing and input | `winit` | Mature cross-platform windows, input, trackpad gestures |
| GPU | `wgpu` | One API over **Metal** (Mac) and **DirectX 12** (Windows, Vulkan fallback); WGSL shaders |
| Audio output | `cpal` + our own mixer | Thin device access; mixing, decoding and voice limits are ours |
| Audio decoding | IMA ADPCM (ours) for effects, Ogg Vorbis (`lewton`, pure Rust) for streamed music | Tiny decoders, no C toolchain |
| Networking | `std::net::UdpSocket` + our own protocol | Lockstep needs very little; no async runtime |
| Data formats | RON source files compiled to binary tables | Human-editable balance data, zero-parse loading |
| Hashing | BLAKE3 for content/data hashes, xxHash3 for per-tick state hashes | Fast and well-distributed |
| Dev-only UI | `egui` behind a `dev` feature | Debug inspectors; never shipped in the game UI |
| Profiling | Tracy (via `tracy-client`) behind a feature flag | Frame and tick profiling with zero cost when off |

No general-purpose engine, no ECS framework, no async runtime, no scripting VM in the simulation.

## Process and threads

```
 ┌──────────── main thread ────────────┐   ┌──── sim thread ────┐
 │ OS events · input · UI · camera     │   │ fixed 20 Hz ticks  │
 │ presentation (soldier puppets)      │◀──│ writes snapshots   │
 │ render submission (wgpu)            │   │ to triple buffer   │
 └───────────────▲─────────────────────┘   └─────────▲──────────┘
                 │ local commands                    │ turn command sets
                 ▼                                   │
 ┌──────────── net thread ─────────────┐             │
 │ UDP socket · lockstep turns · acks  │─────────────┘
 └─────────────────────────────────────┘
 ┌──────────── audio thread (cpal) ────┐   lock-free queue from main thread
 └─────────────────────────────────────┘
```

- About five threads total. No thread pool during play; match loading uses scoped threads to decode in parallel, then they exit.
- The **main thread** must own the window on macOS. It runs input, UI, presentation and rendering.
- The **sim thread** is single-threaded and deterministic. At the target scale a tick costs ~1 to 2 ms out of 50 ms, so parallelising the sim is unnecessary and would only add determinism risk.
- Threads communicate through fixed-size lock-free ring buffers and a triple-buffered snapshot. No locks on hot paths.

## Workspace layout

```
grubcraft/
├─ crates/
│  ├─ gc_fixed/      fixed-point math, angles, trig tables, isqrt (no_std)
│  ├─ gc_sim/        deterministic simulation (no_std + alloc, no floats)
│  ├─ gc_data/       unit/faction/ability definitions, compiled tables
│  ├─ gc_net/        transport, lockstep turns, lobby protocol, discovery
│  ├─ gc_present/    interpolation, soldier puppets, effects triggers
│  ├─ gc_render/     wgpu renderer: terrain, instancing, VAT, fog, UI batch
│  ├─ gc_ui/         game UI (immediate mode, MSDF text)
│  ├─ gc_audio/      mixer, voices, decoders, music layers
│  ├─ gc_assets/     pack format, mmap loader, asset ids
│  ├─ gc_platform/   per-OS glue: paths, local-network permission, memory stats
│  └─ gc_game/       the application: states, input mapping, glue
├─ tools/
│  ├─ gc_bake/       asset baker: glTF/Blender exports → packed formats
│  ├─ gc_replay/     headless replay runner, determinism checker
│  ├─ gc_bench/      performance benchmarks with budget gates
│  └─ gc_mapcheck/   map validator
├─ data/             RON definitions (factions, units, abilities, maps)
├─ assets-src/       source art and audio (or a pointer to where they live)
└─ docs/
```

`gc_sim` depends on nothing platform-related. It cannot read the clock, open files, spawn threads or use floating point. This boundary is what makes lockstep, replays, rejoin, "Restage this battle" and future bots possible.

## The deterministic simulation

### Time
- Fixed **20 ticks per second** (50 ms). One lockstep turn equals one tick.
- Rendering runs at the display rate and interpolates between the two latest sim snapshots, so 20 Hz simulation still looks smooth at 144 Hz.

### Numbers
- **No floating point anywhere in `gc_sim`**, enforced by Clippy's `disallowed_types` lint and a CI grep. Floats can round differently across CPUs and compilers; integers cannot.
- Positions are **Q16.16 fixed-point** in tile units (`i32`), giving ±32,768 tiles of range at 1/65,536-tile precision.
- Angles are `u16` (65,536 steps per turn) with a 1,024-entry sine table. Square roots use integer Newton iteration on `u64`.
- Damage, HP and morale are plain integers.

### Randomness
- One **PCG32** generator per match seeded from the lobby seed, plus per-system substreams derived from it (combat, events, map) so adding a random draw in one system doesn't shift another.
- Never `rand::thread_rng`, never anything seeded from time.

### Entities and storage
- Data-oriented **structure-of-arrays** pools with **fixed capacity** reserved at match load, sized by map and player count:

| Pool | Capacity (Warfield) | Approx. bytes each |
|---|---|---|
| Regiments (incl. workers, heroes) | 1,536 | ~320 (incl. per-model HP) |
| Structures | 1,024 | ~160 |
| Volleys and projectiles | 2,048 | ~48 |
| Corpses | 4,096 | ~16 |
| Effects and statuses | 4,096 | ~32 |

- Entities are addressed by **generational handles** (`u32` index + `u16` generation), so stale references fail safely.
- Iteration is always in index order. No hash-map iteration in the sim; lookups use dense arrays or sorted vectors.
- Total simulation state for a 4-player late game is **under 4 MB**, which also makes snapshots (for rejoin and desync recovery) cheap.

### Tick pipeline (fixed order)
1. Apply this turn's commands (sorted by player, then sequence).
2. Economy: harvesting, vein flow, chord evaluation, caravans, upkeep.
3. Production, research, construction, larvae.
4. Abilities and statuses.
5. Pathfinding requests (budgeted per tick) and movement.
6. Formation solving and collisions.
7. Combat: melee contact, volleys, damage, deaths, corpses.
8. Morale and routing.
9. Vision and fog of war.
10. Titan events, hamlet and creep AI.
11. Victory checks.
12. State hash (every 10 ticks) and snapshot publish.

### Spatial partitioning
A uniform grid of 8 × 8-tile buckets with intrusive index lists. Rebuilt incrementally (entities move bucket only when they cross a boundary). Used for collision, targeting, vision and area effects.

### Pathfinding
- **Navigation grid** at half-tile resolution: a Warfield map is 512 × 512 cells, 2 bytes per cell (cost + flags) = 512 KB.
- **Hierarchical:** the grid is cut into 32 × 32-cell sectors with portals on shared edges. Long paths run A* over the small portal graph first.
- **Flow fields** per (sector, target portal) are computed on demand and cached in a fixed-size LRU (~1 KB each). Many regiments heading to the same place share fields, which is exactly what armies do.
- Regiments path **as a block**: the flow field moves the formation's anchor; members follow formation slots, with local avoidance between regiments (oriented-rectangle footprints).
- A per-tick path budget smooths spikes when 40 regiments are ordered at once.

### Formations and combat
- A regiment's formation is its anchor position, facing, frontage width and depth, plus a slot mask of which models are alive. The sim never tracks individual soldier positions.
- **Melee:** when two formations' fronts (or a front and a flank/rear) touch, a contact segment forms. Damage is resolved per file along the segment, using attacker and defender stats, facing multipliers, charge bonus, terrain and veterancy. Casualties remove models from the back ranks first (the "front rank steps up").
- **Ranged:** each volley is one entity. On impact it rolls hits against the target formation's footprint and density with the combat RNG substream, so a tight block suffers more than a loose screen.
- **Morale** is an integer per regiment updated from events (casualties, flank hits, nearby routs, heroes, terror) with decay toward a baseline.

### Fog of war
- Visibility per player on a tile-resolution grid of `u8` reference counts (256 × 256 = 64 KB per player). Units add and remove "vision stamps" only when they cross tiles, so cost is proportional to movement, not unit count.
- Explored-ever is a 1-bit-per-tile layer.
- The renderer receives the local player's fog as a small texture and blurs it in the shader.

### Abilities as data
Abilities are composed in data files from a fixed vocabulary of deterministic effect primitives: `Damage`, `Heal`, `ApplyStatus`, `ModifyMorale`, `Summon`, `Teleport`, `AreaPulse`, `SpawnVolley`, `Transform` (Brood metamorphosis), `RaiseCorpses`, and so on. Most of the 48 hero abilities and the faction-meter abilities need no new code. Faction-defining systems (veins, chords, caravans, tithe) are hand-written Rust inside `gc_sim`.

### State hashing and snapshots
- Every 10 ticks the sim hashes its full state (xxHash3 over the pools) for desync detection.
- A **snapshot** is the pools copied as-is into a buffer plus a header, then compressed with LZ4: typically 300 KB to 1 MB. Used for rejoin, desync recovery, save games and "Restage this battle".

## The two-tier simulation

```
   gc_sim (deterministic, 20 Hz)            gc_present (local, every frame)
   ─────────────────────────────            ───────────────────────────────
   regiment: anchor, facing, width,   ──▶   soldier puppets: position, velocity,
   slot alive mask, HP per model,           animation state, target slot,
   state (moving/fighting/routing)          small random personality offsets
   volley hit results, deaths        ──▶   arrows in flight, who falls, how
```

The **presentation layer** turns regiment state into believable soldiers:
- Each soldier is a ~32-byte puppet that steers toward its formation slot with a little lag and separation, so blocks ripple and jostle rather than sliding rigidly.
- When the sim says regiment R lost 3 models this tick, presentation picks three soldiers in contact (or in the back rank for ranged hits) and plays death animations, then shuffles survivors forward.
- Melee pairs, sword swings, arrows arcing to individual soldiers: all chosen by presentation to match the sim's numbers.
- Presentation never writes back to the sim, and it is allowed to be non-deterministic. Two players may see slightly different soldiers fall; they always see the same regiments with the same model counts.

Cost: ~10,000 puppets updated with SIMD-friendly arrays on the main thread in about 0.5 ms. If needed, puppets move to a GPU compute pass later.

## Rendering

Forward renderer on `wgpu`. Target **under 300 draw calls** per frame in the heaviest battle.

### Passes
1. **Shadow pass:** one shadow map (2048², 16-bit depth) fitted to the visible ground area. The fixed-pitch camera makes a single cascade enough. Terrain self-shadowing is **baked at map load** (the sun is fixed per map), so only units and structures render into the shadow map.
2. **Main pass:** terrain, structures, soldiers, decals (veins, chords, ranges, selection), particles, water. Fog of war is applied in these shaders by sampling the fog texture, with no separate pass.
3. **UI pass:** one batched draw for quads and MSDF text.
4. **Post:** one fullscreen pass for tone mapping, colour-grading LUT (32³) and FXAA. MSAA is offered as a quality option.

### Terrain
- Heightmap at 4 samples per tile (`R16`, 1024² on Warfield = 2 MB) sampled in the vertex shader over a shared grid mesh, drawn in 32 × 32-tile chunks with frustum culling and two LOD steps.
- Splat-map blending of up to 8 tiling materials from a texture array, plus painted decals.

### Units: instancing and vertex-animation textures
- Every soldier, of every regiment of a given unit type, is drawn in **one instanced draw call**.
- Per-instance data is packed into **16 bytes**: quantised position (2 × `u16`), facing, animation id and frame, team colour row, flags. 10,000 soldiers is 160 KB of upload per frame.
- Animation uses **bone animation textures**: each unit type's skeleton poses are baked per frame into a small texture, and vertices are rigidly bound to one bone (which suits the low-poly style). About 100 to 150 KB per unit type for all its animations, against several megabytes for per-vertex animation textures.
- Meshes use 16-bit quantised positions, octahedral-encoded normals and a palette index per vertex: around 8 bytes per vertex.

### Level of detail and Commander View
- Soldiers have two mesh LODs (full and ~40% triangles).
- Beyond a zoom threshold the renderer switches to **Commander View**: soldiers are not drawn at all; each regiment is a single banner-block. Rendering cost at full zoom-out is *lower* than mid-zoom, the opposite of most engines.

### Frame pacing and power
- Presets: **Battery** (30 or 60 fps cap, half-resolution shadows, no MSAA), **Balanced**, **High**.
- Frame limiter and V-sync; the game never spins a laptop's fans at the main menu.

## Memory architecture

### Rules
1. **Pools, not heaps, for gameplay.** All sim pools, flow-field caches, puppet arrays and per-frame instance buffers are allocated at match load. A debug assertion fails if anything allocates during a sim tick.
2. **Tagged accounting.** A thin global-allocator wrapper attributes every allocation to a subsystem tag (`sim`, `render`, `audio`, `ui`, `net`, `assets`). GPU buffers and textures are tracked by our renderer wrapper. The dev overlay shows a live table; the benchmark suite fails if any tag exceeds its budget.
3. **Upload and forget.** Texture and mesh bytes go from the memory-mapped pack straight into GPU staging buffers; no CPU copy is retained.
4. **Unified memory counts.** On Apple Silicon, GPU memory *is* system RAM, so GPU allocations are budgeted alongside CPU ones, not separately.
5. **Memory-mapped, read-only asset pack.** Pages the OS can evict and re-read don't count against the game's private working set, and nothing is read until touched.

The budget table lives in [12 · Performance](12-performance.md#memory-budget).

## Assets

### Pipeline
```
Blender / glTF / WAV / PNG / RON
        │  gc_bake (offline, deterministic, cached by content hash)
        ▼
grubcraft.pak  ── memory-mapped at runtime ──▶ GPU / mixer / sim tables
```

### Pack format
- A single file with a sorted table of contents: 64-bit asset id (hash of the source path) → offset, size, type, flags.
- Every entry is aligned and stored in its final in-memory layout: no parsing, just pointer casts over validated headers.
- Textures are stored **BC-compressed** (BC7 colour, BC4/BC5 masks and normals) with mipmaps, uploaded as-is. BC formats are supported by the Metal GPUs in every Mac we target and by every Windows GPU.
- Small data tables may be LZ4-compressed when it pays off.
- Dev builds can load loose files and hot-reload them; release builds read only the pack.

### Game data
- Factions, units, abilities, upgrades and maps are defined in RON under `data/`, compiled by `gc_bake` into binary tables.
- The compiled data has a **BLAKE3 hash**. The lobby refuses to start unless every player has the same game build hash and data hash, which rules out the most common cause of desyncs.
- In solo sandbox, RON files hot-reload for balance tuning.

## Maps
- Heightmap (`u16`), tile types (`u8`), water depth layer (reserved for naval), object list, Bloodseep sites and a small event-script block.
- A Warfield map is under 2 MB on disk. The baked terrain lighting and nav sector graph are generated at load (in well under a second) rather than stored.

## UI framework
- Our own **immediate-mode UI** in `gc_ui`: layout in Rust code, rendered as one batched draw, with an MSDF font atlas baked offline (crisp at any UI scale from one small atlas).
- Immediate mode suits a UI that is mostly views over game state; no retained widget tree to keep in sync.
- Localisation-ready from day one (strings by key, UTF-8, font fallback), English only at launch.

## Audio engine
- One mixer on the `cpal` audio thread; up to 32 voices with priority and distance culling.
- Effects stored as IMA ADPCM (4:1, decoded per sample while mixing, so they stay compressed in memory).
- Music streamed from the pack in small chunks, decoded on the fly; adaptive layers crossfade on battle intensity.
- The main thread sends play/stop/parameter events through a lock-free queue; the audio thread never allocates.

## Platform specifics

### macOS (12 Monterey and later, Apple Silicon and Intel)
- Metal backend. Shipped as a **universal binary** (arm64 + x86_64) in a `.app` bundle.
- `Info.plist` declares `NSLocalNetworkUsageDescription` and the Bonjour service type, so macOS asks once for local-network permission (required for LAN discovery on recent macOS).
- For personal distribution we use ad-hoc code signing; friends approve the first launch once (right-click → Open). Notarisation would need a paid Apple developer account and is optional.
- Memory reported from `task_info` physical footprint (what Activity Monitor shows).

### Windows (10 and 11, x64)
- DirectX 12 backend, Vulkan fallback.
- Single `.exe` plus `grubcraft.pak`. Windows Firewall asks once when hosting; we document "allow on private networks".
- Unsigned builds show a SmartScreen warning on first run ("More info → Run anyway").
- Memory reported from the process working set and commit size.

### Linux
Not a target, but the stack is cross-platform; it will very likely work via Vulkan for free.

## Tooling
| Tool | Purpose |
|---|---|
| `gc_replay` | Run a replay headless at maximum speed; verify state hashes; dump state at a tick |
| `gc_bench` | Fixed benchmark scenarios; reports tick time, frame time, memory by tag; enforces budgets |
| `gc_mapcheck` | Map fairness and validity checks |
| `gc_bake` | Asset and data baker |
| Dev overlay (`F11`) | Frame and tick timings, memory by tag, draw calls, network stats |
| Sandbox | In-game unit placement, balance hot-reload, slow motion |

## Testing strategy
- **Unit tests** for fixed-point math, pathfinding, combat formulas, economy rules.
- **Golden replays:** a library of recorded matches with stored state hashes. Every change replays them; a hash change must be intentional (and the goldens re-recorded in the same PR).
- **Cross-platform determinism in CI:** the same golden replays run on macOS arm64 and Windows x64 runners and must produce identical hashes. This is the test that protects Mac-versus-Windows multiplayer.
- **Fuzzed soak test:** four scripted random-command players for 60 simulated minutes, headless, at maximum speed: no panics, memory plateaus, tick time within budget.
- **Network chaos test:** four local clients over a loopback shim that injects latency, jitter, loss and reordering.
- **Performance gates** (see [12 · Performance](12-performance.md#hill-climbing-process)).

## Dependency policy
- Every third-party crate is listed in `docs/dependencies.md` with its purpose and its cost (binary size, transitive dependencies).
- No async runtime, no large frameworks, no C++ build dependencies without a written justification.
- `cargo deny` checks licences and duplicates; `cargo bloat` output is tracked so binary size regressions are visible.
