# 10 · Technical Architecture

This is the engineering plan for an RTS that is lightweight without looking or playing cheap. The engine choice and its rationale are in [ADR 0001](adr/0001-engine-and-language.md); budgets and measurement are in [12 · Performance](12-performance.md).

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

- About five threads total (six on a machine running AI players). No thread pool during play; match loading uses scoped threads to decode in parallel, then they exit.
- The **main thread** must own the window on macOS. It runs input, UI, presentation and rendering.
- The **sim thread** is single-threaded and deterministic. At the target scale a tick costs ~1 to 2 ms out of 50 ms, so parallelising the sim is unnecessary and would only add determinism risk.
- Threads communicate through fixed-size lock-free ring buffers and a triple-buffered snapshot. No locks on hot paths.
- An **AI thread** runs on the host (and in solo play) when the match has AI players or world parties with minds. It reads the latest snapshot and sends commands into the host's turn bundle exactly like a remote player, so it never slows the sim and cannot cause a desync; other machines don't run it. See [AI players and world parties](#ai-players-and-world-parties).
- **Built so far (M1):** one thread. The sim ticks inside the main thread's frame loop (at most eight ticks per frame, so 4× speed keeps up at 30 fps) and the renderer interpolates between the last two ticks; the sim and net threads arrive with networking in M2. Computer players think on the main thread just before each tick (about 0.003 ms a tick for two of them, so a thread of their own waits until rehearsal makes them expensive).

## Workspace layout

```
grubcraft/
├─ crates/
│  ├─ gc_fixed/      fixed-point math, angles, trig tables, isqrt (no_std)
│  ├─ gc_sim/        deterministic simulation (no_std + alloc, no floats)
│  ├─ gc_ai/         AI players and world-party minds; reads sim views, sends commands
│  ├─ gc_data/       unit/faction/ability definitions, compiled tables
│  ├─ gc_net/        transport, lockstep turns, lobby protocol, discovery   (M2)
│  ├─ gc_present/    interpolation, soldier puppets, effects triggers       (later)
│  ├─ gc_render/     wgpu renderer: terrain, instancing, VAT, fog, UI batch
│  ├─ gc_ui/         game UI (immediate mode; menus, lobby, settings built)
│  ├─ gc_audio/      mixer, voices, ADPCM decoder, music layers, cpal output
│  ├─ gc_assets/     pack format, mmap loader, asset ids
│  ├─ gc_platform/   per-OS glue: paths, local-network permission, memory stats
│  └─ gc_game/       the application: states, input mapping, glue
├─ tools/
│  ├─ gc_bake/       data baker: data/*.ron → compiled tables
│  ├─ gc_artbake/    model baker: glTF → meshes, LODs, bone animation textures
│  ├─ gc_sfx/        generates the prototype sound set
│  ├─ gc_replay/     headless replay runner, determinism checker            (later)
│  ├─ gc_bench/      performance benchmarks with budget gates
│  ├─ gc_arena/      headless AI-against-AI ladders and tactical drills      (later)
│  └─ gc_mapcheck/   map validator                                          (later)
├─ data/             RON definitions (factions, units, abilities, maps)
├─ assets/           baked packs embedded in the game (models.pak, audio)
├─ assets-src/       model list for gc_artbake and small CC0 sources; big packs live in the shared project folder
└─ docs/
```

Crates marked *(later)* or *(M2)* don't exist yet. Until `gc_present` is needed, interpolation and the soldier puppets live in `gc_game` (`src/puppets.rs`).

`gc_sim` depends on nothing platform-related. It cannot read the clock, open files, spawn threads or use floating point. This boundary is what makes lockstep, replays, rejoin, "Restage this battle" and AI players possible (they read the sim's public views and send commands; see [18 · AI](18-ai.md)).

## The deterministic simulation

### Time
- Fixed **20 ticks per second** (50 ms). One lockstep turn equals one tick.
- Rendering runs at the display rate and interpolates between the two latest sim snapshots, so 20 Hz simulation still looks smooth at 144 Hz.

### Numbers
- **No floating point anywhere in `gc_sim`**, enforced by a test (`gc_sim/tests/no_floats.rs`) that fails on any float type in the crate's source. Floats can round differently across CPUs and compilers; integers cannot.
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
- **Built so far:** the regiment (1,536) and volley (2,048) pools. Fallen soldiers are presentation-only for now (`gc_game` keeps the latest 8,192 on screen); a sim corpse pool comes with abilities that use corpses.
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
10. Titan events and world-party rules: creep camps, hamlet garrisons, relations and contracts. (Choices made by AI players and other minds are not here: they arrive as commands in step 1.)
11. Victory checks.
12. State hash (every 10 ticks) and snapshot publish.

**Built so far:** commands (including the sandbox's `RemoveRegiment` and `TuneUnit`), then combat's contact, charge and chase checks, then route planning (budgeted searches), then movement and turning along the routes, then melee blows and volleys, then morale. The state hash is computed on request (the determinism test checks it every 600 ticks); snapshots come with M2.

### Spatial partitioning
A uniform grid of 8 × 8-tile buckets with intrusive index lists. Rebuilt incrementally (entities move bucket only when they cross a boundary). Used for collision, targeting, vision and area effects.

**Built so far:** combat has its own spatial hash of 16-tile cells, rebuilt every tick (see [Formations and combat](#formations-and-combat)). Collision, vision and area effects will share or replace it when they arrive.

### Pathfinding
- **Navigation grid** at half-tile resolution: a Warfield map is 512 × 512 cells, 2 bytes per cell (cost + flags) = 512 KB.
- **Hierarchical:** the grid is cut into 32 × 32-cell sectors with portals on shared edges. Long paths run A* over the small portal graph first.
- **Flow fields** per (sector, target portal) are computed on demand and cached in a fixed-size LRU (~1 KB each). Many regiments heading to the same place share fields, which is exactly what armies do.
- Regiments path **as a block**: the flow field moves the formation's anchor; members follow formation slots, with local avoidance between regiments (oriented-rectangle footprints).
- A per-tick path budget smooths spikes when 40 regiments are ordered at once.

**Built so far (M1 step 6):** `gc_sim::nav`, with **waypoint routes instead of flow fields**. Regiments march as rigid blocks that wheel to face their direction of travel, so straight legs between a few waypoints suit them better than per-cell flow directions (which zigzag in 45° steps and would make blocks stop and turn constantly). Flow fields can still come later if huge armies sharing one goal make searches the bottleneck.
- **Grid:** one cost byte per half-tile cell (0 blocked, 1 open, higher is slow going that routes avoid; it does not slow marching yet). At load the sim measures each cell's clearance (steps to the nearest blocked cell or the map edge) and treats cells under 3 as closed to a regiment's centre, so blocks keep 1.25 tiles off cliffs. It also numbers the connected areas, so a search never starts for a goal that can't be reached.
- **Sectors and portals:** 32 × 32-cell sectors; a portal for every run of up to 16 open cells facing each other across a sector edge; each portal linked at load to every other portal it can reach in its sector, with the true walking cost. On the 256-tile ridged test map that is about 1,900 portal nodes, built in about 0.15 s.
- **A route request:** if the goal is in plain sight, walk straight there. Otherwise A* over the portal graph (start and goal linked to the portals they can see), fill in any leg between portals that isn't a straight line with a sector-local A*, then pull the chain tight into waypoints (legs of at most 24 tiles; 16 waypoints stored per regiment, and a longer route is finished with a fresh search when the 16th is reached). Searches weight their estimate by 1.25, trading routes up to a quarter longer than the shortest for much less work. A goal on blocked ground, or cut off from the start, becomes the nearest reachable cell within 32 tiles; a regiment standing too close to blocked ground first steps out to the nearest open cell.
- **Budget:** requests queue in order and each tick serves them until 32,000 work units are spent (4 per search step, 1 per cell checked along a line; at least one request per tick). A target that drifts less than 3 tiles from the one a route was found for (a chased enemy) keeps the route. Movement never steps onto a blocked cell; a regiment that would is sent to search again.
- **Not yet:** local avoidance between regiments (friendly blocks still pass through each other), blocks narrowing into columns for tight passes, and terrain costs slowing the march.

### Formations and combat
- A regiment's formation is its centre, facing and number of files (soldiers per rank); the ranks follow from how many soldiers are standing, and the block's width and depth from the unit's file and rank spacing. Hit points are kept per soldier by formation slot (rank 0 is the front). The sim never tracks individual soldier positions; `gc_sim::formation` gives each slot's place in the block for the renderer and for combat.
- Regiments turn on the spot before marching when the destination is more than 45° off their facing, and shuffle (step while turning toward the final facing) for short adjustments. Group orders are expanded by the ordering player's machine into one move per regiment (`formation::group_move`: side by side, at the slowest member's pace; a group wider than 64 tiles forms several lines, front to back, unless the player dragged the line's width), so the lockstep turn only ever carries single-regiment moves.
- **Melee:** two regiments are in contact when their blocks (oriented rectangles) come within half a tile of each other; a regiment in contact is locked, ignores move orders and only turns toward its foe at half speed. Each fighting regiment strikes once a second (staggered across regiments so the work spreads over ticks) with one blow per soldier who can reach the foe (the front rank when the foe is ahead, one per rank when it is to the side, half the front rank when it is behind): a blow lands with chance 50 + attack − defence percent (clamped 10 to 90), damage is multiplied by the side it lands on (front ×1.0, flank ×1.4, rear ×1.8) plus a charge bonus that fades over 3 s if contact began at speed. The expected damage is rolled ±25% on the combat RNG stream and carried in 1/256 hit points, so small blows add up exactly. Casualties remove models from the back slots first (the "front rank steps up"), and a block that loses whole ranks steps forward to stay in contact. Terrain and veterancy come later.
- **Ranged:** each volley is one entity in a fixed pool, flying one tile per tick from the archers to the aim point. On impact it rolls hits on the combat RNG stream, scaled by how densely packed the target is, so a tight block suffers more than a loose screen; a target that has moved away from the aim point is missed. Archers standing still fire at will at the nearest enemy in range, or at the regiment they were ordered to attack (walking into range first).
- **Finding foes:** a spatial hash of 16-tile cells (each regiment is listed in every cell its block covers, split by owner) keeps contact and target searches local; 1,200 regiments fighting cost about 0.4 ms per tick at the 99th percentile.
- **Morale** (`gc_sim::morale`) is an integer per regiment (tenths of a point), starting at the unit's `morale` stat. Each regiment checks it once a second, half a second out of step with its blows:
  - **Losses:** 0.9 points per 1% of full strength lost since the last check.
  - **Shock:** 3 points for flank hits that second, 6 for rear hits.
  - **In melee:** 2 points a second below half strength. Another 2 (or 4) when enemy soldiers within 12 tiles outnumber friendly ones 2 to 1 (or 3 to 1).
  - **Recovery:** out of the fight and losing nobody, it regains 3 points a second. The cap falls from the stat at full strength to half the stat with nobody left.
  - **States:** below 25 points a regiment wavers and deals 85% damage. At 0 it routs: it drops out of melee, rejects orders and every second aims 10 tiles away from the enemies within 20 tiles, at 90% of its speed, inside the map edge.
  - **Ripples:** a rout costs friends within 16 tiles 8 points and gives the enemies fighting the routers 5.
  - **Rally and shatter:** after 10 s with no enemy within 20 tiles, routers rally, wavering. Routing again within 120 s shatters a regiment: it runs, then stands and never takes orders again.
  - **Fearless:** units with a morale stat of 0 skip all of this.
  - Heroes, terror and veterancy come later.

### Fog of war
- Visibility per player on a tile-resolution grid of `u8` reference counts (256 × 256 = 64 KB per player). Units add and remove "vision stamps" only when they cross tiles, so cost is proportional to movement, not unit count.
- Explored-ever is a 1-bit-per-tile layer.
- The renderer receives the local player's fog as a small texture and blurs it in the shader.

### Abilities as data
Abilities are composed in data files from a fixed vocabulary of deterministic effect primitives: `Damage`, `Heal`, `ApplyStatus`, `ModifyMorale`, `Summon`, `Teleport`, `AreaPulse`, `SpawnVolley`, `Transform` (Brood metamorphosis), `RaiseCorpses`, and so on. Most of the 48 hero abilities and the faction-meter abilities need no new code. Faction-defining systems (veins, chords, caravans, tithe) are hand-written Rust inside `gc_sim`. Every ability in the game is drafted in `data/rosters/*.ron` with the primitives it needs; [17 · Unit Rosters](17-unit-rosters.md#schema-proposals) lists the extra primitives and record fields that implies.

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
- Each soldier is a ~32-byte puppet that steers toward its formation slot with a little lag and separation, so blocks ripple and jostle rather than sliding rigidly. The front two ranks of a fighting regiment lunge at the enemy. When the simulation removes soldiers from the back slots, the puppets show a front-rank soldier falling (anyone, for a block under arrows) and the soldier from the emptied slot stepping up into the gap; the fallen lie on the field for 20 s, then sink away.
- Melee pairs, sword swings, arrows arcing to individual soldiers: all chosen by presentation to match the sim's numbers.
- Presentation never writes back to the sim, and it is allowed to be non-deterministic. Two players may see slightly different soldiers fall; they always see the same regiments with the same model counts.

Cost: ~10,000 puppets updated with SIMD-friendly arrays on the main thread in about 0.5 ms. If needed, puppets move to a GPU compute pass later.

## AI players and world parties

Designed in [18 · AI](18-ai.md); summarised here for the engine's sake.

- **Rules in the sim, minds outside it.** Anything that must be identical everywhere (regiment reflexes, creep and hamlet behaviour, the relations ledger, contracts and the rule that answers an offer) is ordinary deterministic sim code. Anything that makes choices (AI players, mercenary companies, factions) is a *mind* in `gc_ai` on the AI thread, which acts only through commands.
- **Minds are players.** They use the public views (`RegimentView` and friends), filtered by their party's fog of war, and the same `Command`s as humans. Replays, saves and golden hashes never depend on AI code.
- **Parties.** `PlayerId` names a *party*: players 0 to 3 and up to 28 world parties (hamlets, creep camps, companies, factions). Whether two parties fight is one test, `World::hostile(a, b)`, which reads the stance table (`gc_sim::stances`) filled from the lobby's teams in `MatchSetup::teams`: teammates' regiments don't fight, can't be ordered to attack each other and count as friends for morale. The table is setup data like the map, not hashed state, and later grows into the relations ledger. The full list of engine hooks is in [18 · AI](18-ai.md#engine-hooks).
- **Built so far (AI-1 step 1):** `gc_ai` holds the battle layers: `perception` (a float copy of what is visible, taken once per tick for all minds), `estimator` (the quick fight model), `captain` (plans, roles and every regiment's job), `orders` (reaction delay and attention budget) and `army` (raising and deploying a Field Battle army), with difficulty knobs, personalities from `data/ai/personalities.ron` and a decision journal per mind. `gc_game` runs a `Minds` set for the match's AI slots and any side handed over with `I`, and appends their commands to the tick's command list beside the player's.
- **Lab mode.** In tests and the arena, the AI runs between ticks on the sim thread with fixed work budgets instead of time budgets, so AI-against-AI games are reproducible.

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
- **Built so far (M1 step 8):** soldiers are drawn from `assets/models.pak` (baked by `tools/gc_artbake`, format in `gc_assets::model`), with one instanced draw per model and level of detail. Vertices are 16 bytes (quantised position, bone index, team-colour flag, normal, colour sampled from the model's texture). Animation is a bone animation texture: one `Rgba16Float` row per frame holding a 3×4 matrix for each of up to 48 bones; the shader blends two frame rows. Instances are still 36 bytes (`SoldierInstance`: position, facing, two frame rows, blend, highlight, fallen flag, team colour, lift, scale) with up to 65,536 of them; packing them to 16 bytes is left until a profile shows upload cost matters. The five models plus all their clips are 612 KB. Selection plates, order previews, arrows and Commander View blocks are still `BoxInstance` boxes, and the game falls back to boxes for soldiers if the model pack fails to load.

### Level of detail and Commander View
- Soldiers have two mesh LODs (full and ~40% triangles).
- Beyond a zoom threshold the renderer switches to **Commander View**: soldiers are not drawn at all; each regiment is a single banner-block. Rendering cost at full zoom-out is *lower* than mid-zoom, the opposite of most engines.
- **Built so far (M1 step 8):** the reduced mesh is drawn for soldiers more than 70 tiles from the camera (`gc_artbake` targets about 1,400 and 500 triangles per model with `meshopt`). Commander View switches on beyond a camera distance of 175 and off below 160. The soldier puppets keep moving while it is on, so zooming back in shows them where they should be.

### Frame pacing and power
- Presets: **Battery** (30 or 60 fps cap, half-resolution shadows, no MSAA), **Balanced**, **High**.
- Frame limiter and V-sync; the game never spins a laptop's fans at the main menu.

## Memory architecture

### Rules
1. **Pools, not heaps, for gameplay.** All sim pools, flow-field caches, puppet arrays and per-frame instance buffers are allocated at match load. A test (`gc_sim/tests/no_alloc_in_tick.rs`) and the `gc_bench` gate fail if anything allocates during a sim tick.
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
- The game's build script bakes `data/` and embeds the resulting data pack in the executable, so a build can never run with stale or mismatched data. Art and audio (from M1) go in a separate pack that is memory-mapped, as above.
- The compiled data has a **BLAKE3 hash**. The lobby refuses to start unless every player has the same game build hash and data hash, which rules out the most common cause of desyncs.
- In solo sandbox, RON files hot-reload for balance tuning. **Built so far (M1 step 7):** `R` re-bakes `data/units.ron` with `gc_bake` (the same checks as the build) and sends a `TuneUnit` command for each type that changed; the stats panel sends the same command. Tuning goes through the command stream like any order, so it is deterministic and will replay. The unit table is part of the state hash. Adding or removing unit types still needs a restart.

## Maps
- Heightmap (`u16`), tile types (`u8`), water depth layer (reserved for naval), object list, Bloodseep sites and a small event-script block.
- A Warfield map is under 2 MB on disk. The baked terrain lighting and nav sector graph are generated at load (in well under a second) rather than stored.

## UI framework
- Our own **immediate-mode UI** in `gc_ui`: layout in Rust code, rendered as one batched draw, with an MSDF font atlas baked offline (crisp at any UI scale from one small atlas).

**Built so far (menus and lobby):** `gc_ui` has no GPU code: each frame `gc_game` feeds it the mouse and keys (`UiInput`), a `Menu` lays out and draws its current screen into a list of `Quad`s (a rectangle or a glyph, in UI pixels), and `gc_render` draws them in one instanced call (`Renderer::set_ui`, the same pipeline as the dev overlay, drawn under it). Text uses the dev overlay's 8 × 16 bitmap font at whole-number sizes; the MSDF font comes later. Screens: main menu, Play (host card and join card), lobby, settings; all are unit tested by clicking through them headlessly. The lobby model (`Slot`, `LobbySettings`, `MatchStart`) and the `Network` trait in `gc_ui::lobby` are the contract with the netcode: `gc_net` (M2) implements `Network`, and until then `LocalNetwork` hosts a practice lobby with AI players and answers a join with "networking arrives in milestone 2". Starting a lobby match builds a Field Battle from `MatchStart` (`gc_game/src/battle.rs`): each slot becomes a party with its team in the stance table, its colour and an army from its faction, and each AI slot gets a mind at its level. Settings are RON in the user config folder (`~/Library/Application Support/Grubcraft/` on a Mac, `%APPDATA%\Grubcraft\` on Windows), overridable with `GRUBCRAFT_SETTINGS`.
- Immediate mode suits a UI that is mostly views over game state; no retained widget tree to keep in sync.
- Localisation-ready from day one (strings by key, UTF-8, font fallback), English only at launch.

## Audio engine
- One mixer on the `cpal` audio thread; up to 32 voices with priority and distance culling.
- Effects stored as IMA ADPCM (4:1, decoded per sample while mixing, so they stay compressed in memory).
- Music streamed from the pack in small chunks, decoded on the fly; adaptive layers crossfade on battle intensity.
- The main thread sends play/stop/parameter events through a lock-free queue; the audio thread never allocates.
- **Built so far:** `gc_audio` has the ADPCM codec, the sound bank format, the lock-free queue (a ring of atomics holding one packed command per slot, no `unsafe`), the mixer (32 effect voices, at most 8 of any one sound, lowest priority then quietest voice stolen first; two music layers crossfaded by battle intensity; soft clipping) and `cpal` output (`f32` or `i16` devices, any channel count). The mixer has no device code and is unit-tested without a sound card. Music is held as ADPCM in the embedded bank and looped, not streamed from disk yet (two 19-second loops are about 300 KB); streamed Ogg comes with real recorded music. `gc_sim` knows nothing about audio: `gc_game/src/sounds.rs` reads the same public views the renderer reads (regiments, volleys in flight) after each tick and plays a sound for each change, positioned by distance and side relative to the camera. If there is no output device the game runs silently. Unit voices and per-faction music come later.

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
| `gc_replay` *(later)* | Run a replay headless at maximum speed; verify state hashes; dump state at a tick |
| `gc_bench` | Fixed benchmark scenarios; reports tick time, frame time, memory by tag; enforces budgets |
| `gc_mapcheck` *(later)* | Map fairness and validity checks |
| `gc_bake` | Data baker |
| `gc_artbake` | Model baker: `cargo run -p gc_artbake --release` re-bakes `assets/models.pak` from `assets-src/models.ron`; `--report` prints each model's colours to help pick team-colour parts |
| Dev overlay (`` ` `` or `F11`) | Frame and tick timings, memory by tag, draw calls, network stats |
| Sandbox (`--sandbox`) | In-game unit placement for any side, live stat tuning and `data/units.ron` reload, pause and 0.1× to 4× speed |
| `gc_arena` *(later)* | Headless AI-against-AI ladders and tactical drills ([18 · AI](18-ai.md#testing-tuning-and-explaining)) |
| AI lens (`L`) | Each computer player's battle plan drawn on the field (its front, every regiment's role, place and target, its point of attack) and its plan, odds, roles and latest decisions in the panel. Built in AI-1 step 3; intents, armies and influence maps join it as those layers arrive |

## Testing strategy
- **Unit tests** for fixed-point math, pathfinding, combat formulas, economy rules.
- **Golden replays:** a library of recorded matches with stored state hashes. Every change replays them; a hash change must be intentional (and the goldens re-recorded in the same PR).
- **Cross-platform determinism in CI:** the same golden replays run on macOS arm64 and Windows x64 runners and must produce identical hashes. This is the test that protects Mac-versus-Windows multiplayer.
- **Fuzzed soak test:** four scripted random-command players for 60 simulated minutes, headless, at maximum speed: no panics, memory plateaus, tick time within budget.
- **Network chaos test:** four local clients over a loopback shim that injects latency, jitter, loss and reordering.
- **Performance gates** (see [12 · Performance](12-performance.md#hill-climbing-process)).
- **AI drills and arena:** tactical puzzles with expected outcomes run as tests, and nightly AI-against-AI ladders in `gc_arena` that check each difficulty beats the one below ([18 · AI](18-ai.md#testing-tuning-and-explaining)).

**Built so far:** unit tests per crate (fixed-point maths, formations, group orders, combat, morale, pathfinding, data baking, puppets); a golden determinism test that runs a scripted 4-player drill (moves, group moves and fights) for 2,400 ticks, on an open plane and again on a ridged test map where most orders need routes, and compares state hashes on macOS, Windows and Linux CI; the no-float and no-allocation tests (the latter on the ridged map); and the `gc_bench` drill and pathing-storm benchmarks. Recorded golden replays arrive with replay recording in M2.

## Dependency policy
- Every third-party crate is listed in `docs/dependencies.md` with its purpose and its cost (binary size, transitive dependencies).
- No async runtime, no large frameworks, no C++ build dependencies without a written justification.
- `cargo deny` checks licences and duplicates; `cargo bloat` output is tracked so binary size regressions are visible.
