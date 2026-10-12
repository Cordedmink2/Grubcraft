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
| Meshes and animation (`mesh`) | Quantised meshes, bone animation textures |
| Render targets (`rt`) | Colour, depth, shadow map, post buffers |
| Presentation (`present`) | Soldier puppets, instance buffers, interpolation |
| Audio (`audio`) | Effects, music stream buffers, mixer |
| UI (`ui`) | Layout scratch, text cache |
| Network and replay (`net`) | Rings, turn buffers, replay writer |

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

## The big ideas that make the numbers possible

Collected here so they're easy to find; detailed in [10 · Architecture](10-architecture.md).

1. **Regiments in the sim, soldiers in presentation.** The deterministic state is hundreds of formations, not thousands of soldiers. Simulation memory and CPU scale with regiments.
2. **Lockstep networking.** Bandwidth and replays scale with clicks, not units.
3. **Palette-textured low-poly art.** Units and buildings need almost no texture memory.
4. **Bone animation textures with rigid binding.** Kilobytes per unit type instead of megabytes.
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

**Built so far:** one scenario, `drill`: four scripted players with 300 regiments each, ordering moves and group moves and fighting, for 12,000 ticks. It gates tick p99 (≤ 2 ms), sim heap (≤ 8 MB) and allocations per tick (0); CI runs it on every pull request. With morale (M1 step 5) it measures about 0.5 ms p99, 0.52 MB and 0 allocations (in the cloud build container). The other scenarios need features that don't exist yet (replays, rendering benchmarks, pathfinding).

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
