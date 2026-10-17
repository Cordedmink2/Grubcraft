# 12 · Performance Budgets and Hill Climbing

Performance is a feature with tests. The numbers that CI can measure are checked on every build, and a regression fails the build. The other numbers here are targets; see [Gates in CI](#gates-in-ci) for what is checked.

## Reference hardware

| Tier | Mac | Windows |
|---|---|---|
| **Low** (must hit targets) | MacBook Air M1, 8 GB | Laptop with Intel Iris Xe or AMD Radeon integrated graphics, 8 GB |
| **Mid** | MacBook Pro M2/M3 Pro | Desktop with GTX 1660 / RX 6600 class GPU |

If a friend's machine is older than the low tier, the Battery preset is the safety net.

## Headline targets

| Metric | Target | Hard ceiling | Stretch |
|---|---|---|---|
| Peak memory, 6-player Warfield, 60 min | **≤ 6 GB** | 6 GB | |
| Frame rate, low tier, 1080p, big battle | 60 fps | 45 fps | 60 fps at High preset |
| Frame rate, mid tier, 1440p | 144 fps | 90 fps | |
| Sim tick, 6-player late game, low tier (p99) | **≤ 2 ms** of the 50 ms tick | 5 ms | 1 ms |
| App launch to main menu | ≤ 1 s | 2 s | |
| Lobby start to first frame of match | ≤ 3 s | 5 s | ≤ 1.5 s |
| Executable size | ≤ 15 MB | 25 MB | |
| Install size (all assets) | ≤ 250 MB | 400 MB | ≤ 150 MB |
| Network per client (busy) | ≤ 3 KB/s | 10 KB/s | |
| Replay size per hour | ≤ 150 KB | 500 KB | |

**Memory budget decided 2026-10-12:** the original target was 160 MB. Measuring M0 on Mario's MacBook (Apple M4) gave about 190 MB, of which about 120 MB is what macOS and the GPU driver charge any Metal window at Retina resolution (measured with a bare Metal program that only clears its window) and about 50 MB is wgpu's own overhead. Mario was happy with that usage and set a 4 GB ceiling for the whole game instead. **Raised 2026-10-16:** Mario raised the ceiling to **6 GB** and the largest match to six players.

"Memory" means the process's physical footprint as the OS reports it (Activity Monitor on Mac, working set on Windows), **including GPU allocations** on Apple Silicon's unified memory. GPU driver overhead that we cannot control is measured and reported separately so it doesn't hide our own regressions.

## Memory budget

The target is that the whole game stays under **6 GB** on the largest map with six players at maximum supply. Apart from the simulation's heap gate and the AI target below, there are no per-subsystem limits inside that ceiling; instead each subsystem is tracked so growth is visible:

| Subsystem (tag) | Main contents |
|---|---|
| Code and static data | Executable, static tables |
| Simulation (`sim`) | Entity pools, nav grid, route plans, vision (fog of war) state |
| Map, CPU side (`map`) | Heightmap, tile layers, nav sectors, decals |
| Renderer, CPU side (`render`) | Renderer setup and scenery. GPU memory is not tagged: terrain material arrays, meshes, bone animation textures (about 0.6 MB for the five M1 models), the skinned soldier vertices (6 MB at the start, growing with what frames need to at most 96 MB), colour, depth, shadow and post buffers are counted by `Renderer::gpu_bytes` instead |
| Presentation (`present`) | Soldier puppets, instance buffers, interpolation, the fallen (up to 768 KB) and stuck arrows (up to 576 KB) |
| Audio (`audio`) | Effects, music stream buffers, mixer |
| UI (`ui`) | Layout scratch, text cache |
| Network and replay (`net`) | Rings, turn buffers, replay writer |
| AI (`ai`) | Host only: each AI's beliefs, influence maps, terrain analysis and scratch worlds for rehearsal (target: ≤ 16 MB per AI player, not gated yet) |

Each tag is tracked live by the tagged allocator. GPU memory is not: there is no allocation wrapper on the graphics device, so the renderer adds up the sizes of the buffers and textures it creates (`Renderer::gpu_bytes`). That total is for display only (the dev overlay and the window title), and it leaves out what the driver adds for itself and a few small buffers. The simulation keeps its own tight limit (under 8 MB, gated in `gc_bench`) because snapshot size will drive rejoin and desync recovery once snapshots exist, not because of the memory budget.

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

### Simulation tick (low tier, 6 players, late game)
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

**Measured (AI-1 step 1, 2026-10-12):** two General-level battle AIs fighting each other on the generated map cost about 0.003 ms per tick between them on average and 0.11 ms in the worst tick (release build, Linux CI-class machine; the ignored `watch` test in `gc_match/src/battle.rs` prints it). That is so far under budget that they still run on the main thread just before each tick; they move to their own thread when rehearsal (AI-2) makes thinking expensive. Their memory is tracked under the `ai` tag in the dev overlay. **AI-1 step 2 (2026-10-13):** reading the ground raises the worst tick to 1 to 2 ms, the tick in which one AI weighs where its line should stand (every 3 s, each AI on its own beat); the average stays under 0.01 ms. **AI-2 (2026-10-15):** a Warlord costs the main thread about 0.03 ms a tick on average and 5 to 9 ms at worst; its rehearsals run on worker threads, about 28 ms of the other cores' time per tick on a 4-core machine (over the 15 ms rehearsal budget), and a live game never waits for them ([18 · AI](18-ai.md#rehearsal-thinking-ahead-with-the-real-simulation)).

## The big ideas that make the numbers possible

Collected here so they're easy to find; detailed in [10 · Architecture](10-architecture.md).

1. **Regiments in the sim, soldiers in presentation.** The deterministic state is hundreds of formations, not thousands of soldiers. Simulation memory and CPU scale with regiments.
2. **Lockstep networking.** Bandwidth and replays scale with clicks, not units.
3. **Small shared textures.** Stand-in units and buildings are palette-coloured; textured soldiers share one 512 × 512 texture array, BC7-compressed to about 1 MB per material (4 MB as RGBA8).
4. **Bone animation textures with four-bone skinning.** Kilobytes per unit type instead of megabytes.
5. **One instanced draw per model and level of detail**, 36 bytes per soldier (16 bytes is a target).
6. **Commander View.** Zooming out makes rendering *cheaper*. The terrain follows the same rule: each 32 × 32-tile chunk is drawn at one of four levels of detail by its distance from the eye (8,192, 2,048, 512 or 128 triangles), so the cost of the ground stops growing with the map's size once most of it is far away. The sun's shadow pass draws the same levels. `RenderStats` has the terrain triangles of both passes and the chunk count at each level.
7. **Target: baked terrain lighting** for the fixed per-map sun, so only dynamic objects cast real-time shadows. Today the terrain also casts into the shadow map in real time; baking waits on a profile showing the shadow pass costs too much.
8. **Zero-copy, GPU-ready asset packs,** compiled into the executable with `include_bytes!` and read in place.
9. **Fixed pools, zero allocations per tick**, verified by test.
10. **Incremental fog of war.** The first step is built (vision timers per party, for regiments in cover); sight ranges and the explored layer are a target for M3.
11. **Target: shared flow fields** for armies heading the same way. Pathing uses waypoint routes for now.
12. **Compressed-in-memory audio** (ADPCM decoded during mixing; music is looped from the embedded bank today, streaming is a target).

## Hill-climbing process

Performance work is continuous, measured and recorded.

### Instruments
- **Dev overlay (`` ` `` or `F11`):** frame time, sim tick time, memory per tag, GPU memory, draw calls, instance counts. Network RTT, jitter and delay are a target.
- **Tracy** profiling in instrumented builds.
- **Tagged allocator** for exact memory attribution on the CPU side. GPU memory is a sum the renderer keeps by hand (see the memory budget above); a real GPU allocation wrapper is a target for the renderer's next pass.
- **Allocation-in-tick tripwire:** a test and the `gc_bench` gate fail if the sim allocates during a tick.

### Benchmark suite (`gc_bench`)
Target suite, replayed headless (sim) and rendered with a scripted camera (frame). Only `drill`, `pathing-storm` and the headless half of `field-8000pt-2v2` exist so far:

| Scenario | What it stresses |
|---|---|
| `warfield-6p-60min` | Recorded 6-player match: peak memory, p99 tick, replay size |
| `field-8000pt-2v2` | Grand Battle with ~6,000 soldiers in contact: frame time, puppets, combat |
| `pathing-storm` | 120 regiments ordered across the map at once: pathfinding budget |
| `siege-and-fire` | Many structures, artillery, particles: GPU effects |
| `zoom-sweep` | Camera from closest to Commander View: LOD transitions |
| `cold-start` | Launch to menu, menu to match |

**Built so far:** four scenarios, gating tick p99 (≤ 2 ms), sim heap (≤ 8 MB) and allocations per tick (0), plus an instruction-count baseline (below); `conquest-6p` has big-map budgets of its own. CI runs all four on every pull request. The first two run on a 256-tile ridged test map (`gc_sim::scenario::ribs_map`; `--open` runs them on an open plane instead).
- `drill`: four scripted players with 300 regiments each, ordering moves and group moves and fighting, for 12,000 ticks. With pathfinding (M1 step 6) it measured about 0.75 to 0.9 ms p99 (0.5 ms on the open plane); since friends push through each other slowly and fall into disorder (2026-10-15) it runs about 1.35 times slower a tick (about 0.75 ms p99 on CI's Linux runner, 1.4 ms in the noisier cloud build container); the M1 performance pass took about 12% back (about 1.2 ms in the cloud container, see [the performance log](perf-log.md)), with 2.7 MB of sim heap (the nav grid and portal graph are most of it) and 0 allocations, in the cloud build container. Routes that keep the whole block clear of the banks, and blocks that queue and narrow into columns in fords (2026-10-17), made a drill tick about 1.3 to 1.4 times dearer again (about 1.75 to 1.95 ms p99 in the cloud container, 3.7 MB of sim heap, still 0 allocations; see [the performance log](perf-log.md)). Checking for friends in the way at most every other tick, and a cheaper scan for them, took about 13% back off the slowest ticks (about 1.45 to 1.5 ms p99 in the cloud container; see [the performance log](perf-log.md)). CI's macOS runner is the noisiest: it has timed the same code anywhere from 1.2 to 2.8 ms p99, against 0.95 to 1.5 ms on Linux.
- `pathing-storm`: 120 regiments ordered across the map in the same tick. It also gates how long the last one waits for its route (≤ 40 ticks, 2 s); today about 25 ticks, at about 0.45 to 0.65 ms p99 (routes that keep a berth from the banks search more of the ground).

- `field-8000pt-2v2` (headless half): the recorded battle `replays/field-8000pt-2v2.gcr` played back. Four armies of about 1,500 soldiers (276 regiments, 6,052 soldiers; points arrive with the army builder, so "8,000 points" means 1,500 soldiers for now) on a generated 384-tile map with hills and rivers, every regiment ordered to attack the nearest enemy each second. Up to about 2,000 soldiers are in melee at once and half the field falls in the first minute. It measures about 0.05 ms p50, 0.3 ms p99 and 0.7 ms at worst a tick, 5.2 MB of sim heap and 0 allocations, in the cloud build container. The frame half (puppets and rendering of the same battle) needs a real GPU and the game to show a 384-tile map (it plays replays since M2 step 6a, on its own 256-tile map), so it is measured on Mario's machines later.

- `conquest-6p` (2026-10-17, Conquest step 6, run only when named): six players in a Conquest match played in fog of war on a 768-tile ridged map, 100 regiments each drilling about the map and 40 buildings each ordered one a tick round their corners (about 230 of the 240 go up; the rest fall on ridges or regiments), for 6,000 ticks; `--no-fog` plays it without fog. **Big-map budgets (step 6b):** a 768-tile walking grid alone is over the 8 MB heap budget, so this scenario has its own: tick p99 ≤ 5 ms and 0 allocations over the ticks no building goes up in, sim heap ≤ 48 MB, and every tick a building goes up in ≤ 40 ms (those may allocate, as the walking grid changes). In the cloud container it measures about 2.7 to 3.7 ms p99, 22.4 MB, and 9 ms typical and 18 ms worst to lay a building. CI runs it as a step of its own on every platform. Numbers in the [perf log](perf-log.md).

**Conquest soak (step 6b, 2026-10-17):** `cargo test --release -p gc_game conquest_soak -- --ignored --nocapture` plays an hour of game time on a 768-tile Land: Mixed Conquest map, set up as the lobby sets up Conquest, between six scripted sides (the Clans, the Concord and the Choir twice each; the Brood's larvae aren't built yet). Each gathers with up to 14 workers, keeps laying houses and up to three barracks, trains soldiers, upgrades its main building from minute 10 and from minute 8 sends a wave every four minutes at the next side still in, in fog of war with Annihilation on, so bases fall and sides go out. It prints each minute's tick times, regiments, buildings, sim heap, all heap and process memory, and fails if the process goes over 6 GB or the sim heap over the big-map 48 MB. CI runs it on Linux. Last run: up to about 150 regiments and 70 buildings, worst tick 30 ms (a razed base), sim heap 26 MB, process 57 MB (79 MB at its peak by the OS's count). The process is the simulation, the map and the game data with no window or GPU; the renderer's share of the 6 GB is measured in the game. `SOAK_MINUTES=N` plays a shorter one.

The other scenarios need features that don't exist yet (a recorded hour-long 6-player match, rendering benchmarks).

### Gates in CI
- Every pull request runs the headless benchmarks on CI machines and compares them against a stored baseline: **> 5% regression in any tracked metric fails**, unless the PR updates the baseline with a written reason (PRF-7). The tracked metric is the number of instructions each scenario executes under valgrind's callgrind (`scripts/bench-gate.py`, on the Linux runner, against `tools/gc_bench/baseline.json`). Instruction counts do not move with how busy a shared machine is: repeat runs agree to better than 0.001%, where wall-clock times on a CI runner vary far more than 5%. The fixed budgets above (p99 tick, heap, allocations) stay as an outer net. The scenarios are smaller than the timing runs so each takes about half a minute under callgrind: `drill` with 150 regiments a player for 1,200 ticks, `pathing-storm`, and the whole `field-8000pt-2v2` replay. A PR that changes how much work the simulation does on purpose runs `scripts/bench-gate.py bless` in the same PR and says why; the gate prints a note when a PR is more than 5% faster, so the gain can be locked in the same way. The counts also go up as the `bench-instructions` artifact of each run. *(Not covered yet: the computer players' cost. `gc_bench` does not depend on `gc_ai`, and the Warlord's rehearsals are known to run over their budget, see [18 · AI](18-ai.md).)*
- The simulation heap (8 MB) is gated in `gc_bench`. Per-tag memory is reported but not gated. `scripts/smoke-run.py` fails the build if the game's process memory goes over 6 GB during its 300-frame battle, on Linux with a software GPU and on the Windows build's zip. That is a short battle; the hour-long six-player match in the headline table is the Conquest soak above (simulation, map and data, without the renderer), which CI runs on Linux.
- Frame benchmarks are a target: `gc_bench` has none yet. They will run on real reference hardware (Mario's machines), since CI machines have no representative GPU.

### The loop
1. Each milestone ends with a **performance pass**: profile the benchmark suite, list the top three costs.
2. Optimise them, measure, and record before/after numbers in [the performance log](perf-log.md) (started with the M1 pass on 2026-10-15).
3. Tighten budgets when there is headroom. Budgets only ever go down.

### Optimisation backlog (ideas to try when the numbers ask for it)
- Move soldier puppets to a GPU compute pass.
- GPU-driven culling and indirect draws for structures and soldiers.
- Half-resolution particles and fog composited at full resolution.
- Variable-rate simulation for far-away or idle regiments (sim-safe only if deterministic).
- Delta-compressed snapshots against the previous keyframe for faster rejoin.
- Apple-specific: memoryless MSAA attachments if exposed through `wgpu`, letting 4× MSAA cost no memory on tile-based GPUs.
- Texture streaming by mip level for the terrain material array on the largest maps.
