# 12 · Performance Budgets and Hill Climbing

Performance is a feature with tests. Every number here is measured automatically and a regression fails the build.

## Reference hardware

| Tier | Mac | Windows |
|---|---|---|
| **Low** (must hit targets) | MacBook Air M1, 8 GB | Laptop with Intel Iris Xe or AMD Radeon integrated graphics, 8 GB |
| **Mid** | MacBook Pro M2/M3 Pro | Desktop with GTX 1660 / RX 6600 class GPU |

If a friend's machine is older than the low tier, the Battery preset is the safety net.

## Headline targets

| Metric | Target | Hard ceiling | Stretch |
|---|---|---|---|
| Peak memory, 4-player Warfield, 60 min | **≤ 4 GB** | 4 GB | |
| Frame rate, low tier, 1080p, big battle | 60 fps | 45 fps | 60 fps at High preset |
| Frame rate, mid tier, 1440p | 144 fps | 90 fps | |
| Sim tick, 4-player late game, low tier (p99) | **≤ 2 ms** of the 50 ms tick | 5 ms | 1 ms |
| App launch to main menu | ≤ 1 s | 2 s | |
| Lobby start to first frame of match | ≤ 3 s | 5 s | ≤ 1.5 s |
| Executable size | ≤ 15 MB | 25 MB | |
| Install size (all assets) | ≤ 250 MB | 400 MB | ≤ 150 MB |
| Network per client (busy) | ≤ 3 KB/s | 10 KB/s | |
| Replay size per hour | ≤ 150 KB | 500 KB | |

**Memory budget decided 2026-10-12:** the original target was 160 MB. Measuring M0 on Mario's MacBook (Apple M4) gave about 190 MB, of which about 120 MB is what macOS and the GPU driver charge any Metal window at Retina resolution (measured with a bare Metal program that only clears its window) and about 50 MB is wgpu's own overhead. Mario was happy with that usage and set a 4 GB ceiling for the whole game instead.

"Memory" means the process's physical footprint as the OS reports it (Activity Monitor on Mac, working set on Windows), **including GPU allocations** on Apple Silicon's unified memory. GPU driver overhead that we cannot control is measured and reported separately so it doesn't hide our own regressions.

## Memory budget

The whole game stays under **4 GB** on the largest map with four players at maximum supply. There are no per-subsystem limits inside that ceiling; instead each subsystem is tracked so growth is visible:

| Subsystem (tag) | Main contents |
|---|---|
| Code and static data | Executable, static tables |
| Simulation (`sim`) | Entity pools, nav grid, flow-field cache, fog, snapshot buffer |
| Map, CPU side (`map`) | Heightmap, tile layers, nav sectors, decals |
| Textures, GPU (`tex`) | Terrain material arrays, palettes, splat, baked terrain lighting, particles, UI and font atlases |
| Meshes and animation (`mesh`) | Quantised meshes, bone animation textures (about 0.6 MB for the five M1 models) |
| Render targets (`rt`) | Colour, depth, shadow map, post buffers |
| Presentation (`present`) | Soldier puppets, instance buffers, interpolation |
| Audio (`audio`) | Effects, music stream buffers, mixer |
| UI (`ui`) | Layout scratch, text cache |
| Network and replay (`net`) | Rings, turn buffers, replay writer |
| AI (`ai`) | Host only: each AI's beliefs, influence maps, terrain analysis and scratch worlds for rehearsal (≤ 16 MB per AI player) |

Each tag is tracked live by the tagged allocator and the renderer's GPU allocation wrapper. The simulation keeps its own tight limit (under 8 MB, gated in `gc_bench`) because snapshot size drives rejoin and desync recovery, not because of the memory budget.

## Time budget

### Frame (low tier, 60 fps = 16.6 ms)
| Work | Budget |
|---|---|
| Input, UI build, camera | 0.5 ms |
| Presentation (puppets, interpolation) | 1.0 ms |
| Render encoding (CPU) | 1.5 ms |
| GPU: shadow pass | 1.5 ms |
| GPU: main pass | 7.0 ms |
| GPU: UI and post | 1.0 ms |
| Slack | 4.1 ms |

### Simulation tick (low tier, 4 players, late game)
| System | Budget |
|---|---|
| Commands, economy, production | 0.2 ms |
| Pathfinding (budgeted per tick) | 0.4 ms |
| Movement, formations, collisions | 0.5 ms |
| Combat and morale | 0.5 ms |
| Vision and fog | 0.2 ms |
| Everything else, hashing | 0.2 ms |
| **Total p99** | **≤ 2.0 ms** |

World-party rules (creep camps, hamlet garrisons, the relations ledger) sit in "everything else" and cost well under 0.01 ms.

### AI thread (host only)
AI players think on their own thread, never inside the tick, so they have their own budget ([18 · AI](18-ai.md#performance-and-memory)):

| Work | Budget |
|---|---|
| Each AI player | ≤ 2 ms average per 50 ms tick |
| Rehearsal (top difficulty), all AIs together | ≤ 15 ms average per tick; spikes allowed, the sim never waits |
| Each mercenary company or faction mind | ≤ 0.2 ms average per tick |

**Measured (AI-1 step 1, 2026-10-12):** two General-level battle AIs fighting each other on the generated map cost about 0.003 ms per tick between them on average and 0.11 ms in the worst tick (release build, Linux CI-class machine; the ignored `watch` test in `gc_game/src/battle.rs` prints it). That is so far under budget that they still run on the main thread just before each tick; they move to their own thread when rehearsal (AI-2) makes thinking expensive. Their memory is tracked under the `ai` tag in the dev overlay. **AI-1 step 2 (2026-10-13):** reading the ground raises the worst tick to 1 to 2 ms, the tick in which one AI weighs where its line should stand (every 3 s, each AI on its own beat); the average stays under 0.01 ms.

## The big ideas that make the numbers possible

Collected here so they're easy to find; detailed in [10 · Architecture](10-architecture.md).

1. **Regiments in the sim, soldiers in presentation.** The deterministic state is hundreds of formations, not thousands of soldiers. Simulation memory and CPU scale with regiments.
2. **Lockstep networking.** Bandwidth and replays scale with clicks, not units.
3. **Palette-textured low-poly art.** Units and buildings need almost no texture memory.
4. **Bone animation textures with four-bone skinning.** Kilobytes per unit type instead of megabytes.
5. **One instanced draw call per unit type**, 16 bytes per soldier.
6. **Commander View.** Zooming out makes rendering *cheaper*.
7. **Baked terrain lighting** for the fixed per-map sun; only dynamic objects cast real-time shadows.
8. **Memory-mapped, zero-copy, GPU-ready asset pack.** Upload and forget; nothing parsed.
9. **Fixed pools, zero allocations per tick**, verified by test.
10. **Incremental fog of war** with reference-counted vision stamps.
11. **Shared flow fields** for armies heading the same way.
12. **Compressed-in-memory audio** (ADPCM decoded during mixing; music streamed).

## Hill-climbing process

Performance work is continuous, measured and recorded.

### Instruments
- **Dev overlay (`` ` `` or `F11`):** frame time breakdown, sim tick time, memory per tag, GPU memory, draw calls, instance counts, network RTT/jitter/delay.
- **Tracy** profiling in instrumented builds.
- **Tagged allocator** and **GPU allocation wrapper** for exact memory attribution.
- **Allocation-in-tick tripwire:** a test and the `gc_bench` gate fail if the sim allocates during a tick.

### Benchmark suite (`gc_bench`)
Scenarios replayed headless (sim) and rendered with a scripted camera (frame):

| Scenario | What it stresses |
|---|---|
| `warfield-4p-60min` | Recorded 4-player match: peak memory, p99 tick, replay size |
| `field-8000pt-2v2` | Grand Battle with ~6,000 soldiers in contact: frame time, puppets, combat |
| `pathing-storm` | 120 regiments ordered across the map at once: pathfinding budget |
| `siege-and-fire` | Many structures, artillery, particles: GPU effects |
| `zoom-sweep` | Camera from closest to Commander View: LOD transitions |
| `cold-start` | Launch to menu, menu to match |

**Built so far:** three scenarios, gating tick p99 (≤ 2 ms), sim heap (≤ 8 MB) and allocations per tick (0). CI runs all three on every pull request. The first two run on a 256-tile ridged test map (`gc_sim::scenario::ribs_map`; `--open` runs them on an open plane instead).
- `drill`: four scripted players with 300 regiments each, ordering moves and group moves and fighting, for 12,000 ticks. With pathfinding (M1 step 6) it measured about 0.75 to 0.9 ms p99 (0.5 ms on the open plane); since friends push through each other slowly and fall into disorder (2026-10-15) it runs about 1.35 times slower a tick (about 0.75 ms p99 on CI's Linux runner, 1.4 ms in the noisier cloud build container), with 2.7 MB of sim heap (the nav grid and portal graph are most of it) and 0 allocations, in the cloud build container.
- `pathing-storm`: 120 regiments ordered across the map in the same tick. It also gates how long the last one waits for its route (≤ 40 ticks, 2 s); today about 17 ticks, at about 0.45 ms p99.

- `field-8000pt-2v2` (headless half): the recorded battle `replays/field-8000pt-2v2.gcr` played back. Four armies of about 1,500 soldiers (276 regiments, 6,052 soldiers; points arrive with the army builder, so "8,000 points" means 1,500 soldiers for now) on a generated 384-tile map with hills and rivers, every regiment ordered to attack the nearest enemy each second. Up to about 2,000 soldiers are in melee at once and half the field falls in the first minute. It measures about 0.05 ms p50, 0.3 ms p99 and 0.7 ms at worst a tick, 5.2 MB of sim heap and 0 allocations, in the cloud build container. The frame half (puppets and rendering of the same battle) needs in-game replay playback (M2 step 6) and a real GPU, so it is measured on Mario's machines.

The other scenarios need features that don't exist yet (a recorded hour-long 4-player match, rendering benchmarks).

### Gates in CI
- Every pull request runs the headless benchmarks on CI machines and compares against the stored baseline: **> 5% regression in any tracked metric fails**, unless the PR updates the baseline with a written reason. *(Not built yet: today CI checks the fixed budgets above, not a stored baseline. Due in M1, requirement PRF-7.)*
- Memory per tag is compared against the budget table above; exceeding a budget fails.
- Frame benchmarks run on real reference hardware (Mario's machines) before each release, since CI machines have no representative GPU.

### The loop
1. Each milestone ends with a **performance pass**: profile the benchmark suite, list the top three costs.
2. Optimise them, measure, and record before/after numbers in `docs/perf-log.md` (started at the first milestone performance pass; until then, numbers are in each pull request).
3. Tighten budgets when there is headroom. Budgets only ever go down.

### Optimisation backlog (ideas to try when the numbers ask for it)
- Move soldier puppets to a GPU compute pass.
- GPU-driven culling and indirect draws for structures and soldiers.
- Half-resolution particles and fog composited at full resolution.
- Variable-rate simulation for far-away or idle regiments (sim-safe only if deterministic).
- Delta-compressed snapshots against the previous keyframe for faster rejoin.
- Apple-specific: memoryless MSAA attachments if exposed through `wgpu`, letting 4× MSAA cost no memory on tile-based GPUs.
- Texture streaming by mip level for the terrain material array on the largest maps.
