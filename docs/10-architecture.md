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
- **Built so far (M1):** one thread. The sim ticks inside the main thread's frame loop (at most eight ticks per frame, so 4× speed keeps up at 30 fps) and the renderer interpolates between the last two ticks; the sim and net threads arrive with networking in M2. Computer players think on the main thread just before each tick (about 0.003 ms a tick for two of them). The Warlord's rehearsals (AI-2) run on worker threads, all of the cores but one, while the game goes on, and a live game never waits for them, so the AI still needs no thread of its own.

## Workspace layout

```
grubcraft/
├─ crates/
│  ├─ gc_fixed/      fixed-point math, angles, trig tables, isqrt (no_std)
│  ├─ gc_sim/        deterministic simulation (no_std + alloc, no floats)
│  ├─ gc_ai/         AI players and world-party minds; reads sim views, sends commands
│  ├─ gc_campaign/   campaign mode: regions, seasons, hosts, the long-term ledger (C-1, see 19)
│  ├─ gc_data/       unit/faction/ability definitions, compiled tables
│  ├─ gc_net/        transport, lockstep turns, lobby protocol, discovery
│  ├─ gc_present/    interpolation, soldier puppets, effects triggers       (later)
│  ├─ gc_render/     wgpu renderer: terrain, instancing, VAT, fog, UI batch
│  ├─ gc_ui/         game UI (immediate mode; menus, lobby, settings, battle HUD minimap and army tray built)
│  ├─ gc_audio/      mixer, voices, ADPCM decoder, music layers, cpal output
│  ├─ gc_assets/     pack format, mmap loader, asset ids
│  ├─ gc_platform/   per-OS glue: memory stats, the log file and crash reports
│  └─ gc_game/       the application: states, input mapping, glue
├─ tools/
│  ├─ gc_bake/       data baker: data/*.ron → compiled tables
│  ├─ gc_artbake/    model baker: glTF → meshes, LODs, bone animation textures
│  ├─ gc_sfx/        generates the prototype sound set
│  ├─ gc_replay/     replay file, recorder, headless runner, golden checks
│  ├─ gc_bench/      performance benchmarks with budget gates
│  ├─ gc_mapcheck/   map validator and map builder: RON map to heights, cover, objects
│  └─ gc_arena/      headless AI-against-AI ladders and tactical drills      (later)
├─ data/             RON definitions (factions, units, abilities, maps)
├─ assets/           baked packs embedded in the game (models.pak, audio)
├─ assets-src/       model list for gc_artbake and small CC0 sources; big packs live in the shared project folder
└─ docs/
```

Crates marked *(later)* don't exist yet. Until `gc_present` is needed, interpolation and the soldier puppets live in `gc_game` (`src/puppets.rs`).

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

**Built so far:** commands (including the sandbox's `RemoveRegiment` and `TuneUnit`, and `SetFormation`), then combat's contact, charge and chase checks, then lines that push (losers give ground, winners follow, the pinned are disordered), then route planning (budgeted searches), then movement and turning along the routes, then melee blows and volleys, then morale, then the champions' challenge, then vision (on maps with woods and tall grass: who sees each regiment, springing ambushes, ending chases after targets gone out of sight; see [Fog of war](#fog-of-war)). While a Field Battle's armies deploy (`gc_sim::deploy`), a tick applies its commands and does nothing else. The state hash is computed on request (the determinism test checks it every 600 ticks); snapshots come with M2.

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
- **Grid:** one cost byte per half-tile cell (0 blocked, 1 open, higher is slow going that routes avoid; the cost itself doesn't slow marching, but water does, see **Terrain** below). At load the sim measures each cell's clearance (steps to the nearest blocked cell or the map edge) and treats cells under 3 as closed to a regiment's centre, so blocks keep 1.25 tiles off cliffs. It also numbers the connected areas, so a search never starts for a goal that can't be reached.
- **Sectors and portals:** 32 × 32-cell sectors; a portal for every run of up to 16 open cells facing each other across a sector edge; each portal linked at load to every other portal it can reach in its sector, with the true walking cost. On the 256-tile ridged test map that is about 1,900 portal nodes, built in about 0.15 s.
- **A route request:** if the goal is in plain sight, walk straight there. Otherwise A* over the portal graph (start and goal linked to the portals they can see), fill in any leg between portals that isn't a straight line with a sector-local A*, then pull the chain tight into waypoints (legs of at most 24 tiles; 16 waypoints stored per regiment, and a longer route is finished with a fresh search when the 16th is reached). A waypoint at a portal is then slid along its sector edge (up to 8 tiles, while the legs either side stay on open, plain ground) toward where the straight line from the waypoint before to the next turn or the route's end crosses it (2026-10-15), so blocks marching side by side keep their spacing instead of funnelling through the same portal middle and pushing into each other. Searches weight their estimate by 1.25, trading routes up to a quarter longer than the shortest for much less work. A goal on blocked ground, or cut off from the start, becomes the nearest reachable cell within 32 tiles; a regiment standing too close to blocked ground first steps out to the nearest open cell.
- **Budget:** requests queue in order and each tick serves them until 32,000 work units are spent (4 per search step, 1 per cell checked along a line; at least one request per tick). A target that drifts less than 3 tiles from the one a route was found for (a chased enemy) keeps the route. Movement never steps onto a blocked cell; a regiment that would is sent to search again.
- **Crowding (2026-10-13):** in tight ground a marching regiment waits behind a friend holding its ground instead of walking into it, so a column wading out of a ford arrives one block at a time. A friend counts as holding when it is fighting, or (for up to 3 seconds, its *patience*) standing still or itself waiting; marching friends still pass through each other, and routers neither wait nor are waited for. It waits only if its next step would push more than a quarter tile deeper into the friend's block (oriented rectangles) and no walkable line leads to a spot level with the friend on either side; ground where the nav grid is clear for 10 tiles around counts as open and nobody waits there. After 3 seconds behind a friend that isn't fighting it walks through, so no order is stuck. The check uses the combat spatial hash plus a per-tick map of which buckets hold a holding regiment; a regiment with no holding friend within 2 tiles of reach skips the check for as many ticks as it would take to get there (at most 8), which keeps the 1,200-regiment drill bench at about 1 ms p99 a tick (0.76 ms before).
- **Pushing through (2026-10-15):** everywhere else friendly blocks may march through each other, slowly and at a cost ([05 · Combat](05-combat.md#pushing-through-friends)). Every 4 ticks (staggered) each regiment measures how deep it overlaps any friendly block (`Regiments::crowd`, the oriented-rectangle overlap, looking only in the hash cells under its own circle and stopping once the overlap is deep enough to change nothing). Past a quarter tile it slows, down to 40% at 1.25 tiles; past a tile it is disordered (`Regiments::disorder`, 3 s fading, in `RegimentView::disorder`), which scales its melee dealt and taken by up to ±20%. `crowd` and `disorder` enter the state hash only while non-zero. The check and the slower, denser fights it brings put the drill bench at about 1.35 times its old tick time.
- **Not yet:** the battle AI keeping its regiments from stacking (handed to the AI-2 work), blocks narrowing into columns for tight passes, and steep slopes slowing the march.

### Formations and combat
- A regiment's formation is its centre, facing and number of files (soldiers per rank); the ranks follow from how many soldiers are standing, and the block's width and depth from the unit's file and rank spacing. Hit points are kept per soldier by formation slot (rank 0 is the front). The sim never tracks individual soldier positions; `gc_sim::formation` gives each slot's place in the block for the renderer and for combat.
- Regiments turn on the spot before marching when the destination is more than 45° off their facing, and shuffle (step while turning toward the final facing) for short adjustments. Group orders are expanded by the ordering player's machine into one move per regiment (`formation::group_move`: side by side, at the slowest member's pace; a group wider than 64 tiles forms several lines, front to back, unless the player dragged the line's width), so the lockstep turn only ever carries single-regiment moves.
- **Melee:** two regiments are in contact when their blocks (oriented rectangles) come within half a tile of each other; a regiment in contact is locked, ignores move orders and only turns toward its foe at half speed (it moves only when lines push, below). Each fighting regiment strikes once a second (staggered across regiments so the work spreads over ticks) with one blow per soldier who can reach the foe (the front rank when the foe is ahead, plus half the second rank for spears and pikes, one per rank when it is to the side, half the front rank when it is behind): a blow lands with chance 50 + attack − defence percent (clamped 10 to 90), damage is multiplied by the side it lands on (front ×1.0, flank ×1.4, rear ×1.8) plus a charge bonus that fades over 3 s if contact began at speed. The expected damage is rolled ±25% on the combat RNG stream and carried in 1/256 hit points, so small blows add up exactly. Casualties remove models from the back slots first (the "front rank steps up"), and a block that loses whole ranks steps forward to stay in contact. Ground changes the blow too (see **Terrain** below). Veterancy comes later.
- **Weapon room and formations** (`Formation`, and `UnitStats::{mass, reach, swing_room, shield}`, unit schema 5): every regiment stands in `Line` (the default), `Tight`, `Loose` or `ShieldWall`, changed by `Command::SetFormation` from its owner only, and never while it is locked in melee (re-forming shrinks the block, which would break contact and re-arm a charge). Routers drop to Line and refuse it; Shield wall needs `shield` (a unit tuned to lose its shields falls back to Line). Formation scales the unit's file and rank spacing (Tight 80%, Loose 150%, Shield wall 70%), so it changes the block's size, its contact, and how densely it takes arrows. Melee defence gains 10 in Tight and, against blows on its front only, in Shield wall; Loose costs 10, except for `swing_room` units. Blow damage is multiplied by the weapon room: `swing_room` units deal 80% in Tight or Shield wall, `reach` or `shield` units 85% in Loose, everything else 100%. `reach` units facing the foe add `min(front, models − front)` second-rank soldiers at half weight, and on the tick a new contact begins with the foe in front of them they strike at once instead of on their turn; they then sit out their next turn (`Strike::Rest`), so lines that touch and part never earn extra blows. A Shield wall marches at no more than half its speed, never starts a charge, and volleys landing on its front deal 30%; a group order containing one marches at that pace (`GroupMember::of` takes each member's formation spacing and pace). These rules live on `Formation` and `UnitStats` (`space`, `pace_percent`, `can_charge`, `arrow_percent`, `melee_defence_in`, `weapon_room_percent`, `front_fighters_percent`), which combat, movement and the AI's estimator all call. `mass` (Light, Medium, Heavy) decides how hard a regiment shoves when lines push (below). Every new factor is 100 in Line with no flags, and the new per-regiment fields are hashed only when not default, so worlds without them keep their old hashes. Tests: `crates/gc_sim/tests/weapons.rs`.
- **Lines that push** (`Combat::push_lines`, 2026-10-15, [05](05-combat.md#lines-that-push)): every melee blow adds its expected damage (1/256 hit points, before the ±25% roll) times the striker's heft (`Mass::heft`: Light 1, Medium 2, Heavy 4) to the striker's `Regiments::pressing` and the target's `pressed`. Once a second, on each regiment's own turn and after its blow, both fade to 7/8 and its `giving` percent is read: with `losing_by = 100·(pressed − pressing)/(pressed + pressing)`, `giving = clamp((losing_by − 20)·100/40, 0, 100)`, and 0 until both tallies are non-zero or when it is not engaged. A blow that can't wound still counts 1 (times heft), so a regiment with no melee damage can still lose the exchange. All three reset to 0 whenever `before_move` finds a different foe (or none). Reading on its own turn means each side has struck exactly once since the last reading, so the side that struck last never looks ahead. Each tick after the contact checks and before movement, a regiment with `giving > 0` steps `GIVE_GROUND` (1/50 tile, 0.4 tiles a second) times `giving`% (slowed by wading) straight away from the side of its block its foe is on, but only when that foe's own foe is this regiment, the foe has `giving == 0`, and the foe's centre can take the same step without walking onto blocked ground; otherwise it holds. It is **pinned** instead when the middle or either corner of its rear edge, after the step, would be on a blocked nav cell (`Nav::blocked`), off the map, or in water while its centre is dry, or when its block would overlap an unbroken friend by more than `PUSHING` (1 tile) or any unbroken enemy, by more than it does now (one grid search under its own circle). A pinned regiment gets full disorder and the `SHOCK_PINNED` bit (3 morale points at its next check). Everyone engaged with a regiment that gives ground, and drawn away by the step, takes the same step (unless onto blocked ground); so does anyone engaged with a regiment that is following its own foe, and so on down the chain for up to four fights (`FOLLOW_HOPS`), so a fight several blocks deep moves together; nobody in a chain follows if any regiment in it would step onto blocked ground. When nobody gives ground in a tick, the follow pass is skipped. The steps are computed from start-of-tick positions, so the order regiments are visited in changes nothing. `pressing`, `pressed` and `giving` enter the state hash only when non-zero; `RegimentView::giving_ground` shows it. Tests: `crates/gc_sim/tests/lines_push.rs`.
- **Terrain** (`gc_sim::terrain`): `MatchSetup::ground` carries one height per tile (at the tile's centre, 256 units per tile of height) and the water line; the game takes them from the heightmap samples by an integer rule (`Heightmap::sim_ground`), the same tile-centre heights the minds read. A regiment's ground is the tile under its centre. Fighting from at least half a tile higher deals ×1.1 melee damage, and archers that far above their target reach ×1.15 as far. A regiment whose centre is under the water line wades at 60% pace, takes ×1.2 melee damage and can't start a charge. [`Ground::FLAT`] (no heights) turns all of it off, which is what tests and the open-plane arena use.
- **Ranged:** each volley is one entity in a fixed pool, flying one tile per tick from the archers to the aim point. On impact it rolls hits on the combat RNG stream, scaled by how densely packed the target is, so a tight block suffers more than a loose screen; a target that has moved away from the aim point is missed. Archers standing still fire at will at the nearest enemy in range, or at the regiment they were ordered to attack (walking into range first).
- **Finding foes:** a spatial hash of 16-tile cells (each regiment is listed in every cell its block covers, split by owner) keeps contact and target searches local; 1,200 regiments fighting cost about 0.4 ms per tick at the 99th percentile.
- **Morale** (`gc_sim::morale`) is an integer per regiment (tenths of a point), starting at the unit's `morale` stat. Each regiment checks it once a second, half a second out of step with its blows:
  - **Losses:** 0.9 points per 1% of full strength lost since the last check.
  - **Shock:** 3 points for flank hits that second, 6 for rear hits, and another 3 for a second spent pinned (lines that push).
  - **In melee:** 2 points a second below half strength. Another 2 (or 4) when enemy soldiers within 12 tiles outnumber friendly ones 2 to 1 (or 3 to 1).
  - **Recovery:** out of the fight and losing nobody, it regains 3 points a second. The cap falls from the stat at full strength to half the stat with nobody left.
  - **States:** below 25 points a regiment wavers and deals 85% damage. At 0 it routs: it drops out of melee, rejects orders and every second aims 10 tiles away from the enemies within 20 tiles, at 90% of its speed, inside the map edge.
  - **Ripples:** a rout costs friends within 16 tiles 8 points and gives the enemies fighting the routers 5.
  - **Rally and shatter:** after 10 s with no enemy within 20 tiles, routers rally, wavering. Routing again within 120 s shatters a regiment: it runs, then stands and never takes orders again.
  - **Fearless:** units with a morale stat of 0 skip all of this.
  - **Bonus:** a per-party `party_morale` added to the cap of each of the party's regiments (hashed only for the parties it is set for), which the champions' duel raises or lowers.
  - Heroes, terror and veterancy come later.
- **Champions** (`gc_sim::champions`, 2026-10-15, [05](05-combat.md#champions-before-battle)): only when `MatchSetup::champions` is set (the game sets it for a lobby battle with a computer player and the option on). Once a second, until it has, the sim looks for the closest hostile pair of unbroken regiments within 60 tiles that see each other (`Vision::sees`; regiments hidden in cover don't send champions either; only regiments within 60 tiles of an enemy party's ground, the box around its regiments, are paired up, so armies still far apart cost one pass over the pool); the first time it finds one, unless the battle has begun for either side (a regiment of it or its allies in melee, or with losses since its last morale check: then there will be no champions), and once each side has a regiment to send, it rolls the 50% chance on the events RNG stream and rolls the whole duel there and then, so a `Challenge` (the two regiments and their parties, the spot, the tick called and the tick they meet, the beats, a bitmask of the blows that landed, the winner) is fixed from the start and the presentation plays it out beat by beat (`World::challenge`). Each beat is 12 ticks and its blow lands 6 ticks in. The champions take between 2 and 12 seconds to meet. Nothing changes until the tick the last blow lands; then each party on the winner's side gets +60 tenths of `party_morale` and its regiments +60 tenths of morale, the loser's side −60 of both (morale never below 0.1, so the duel alone never routs anyone; `morale::shift`, the rule ambushes use too), and the loser's regiment loses its last soldier. The battle beginning for either side, or a champion's regiment broken or gone, before then abandons the challenge. The champions' state is hashed only when the match allows them, so worlds without champions keep their old hashes. The computer players see which parties' champions are fighting (`Perception::champions`); those on those sides hold while they do, and the time doesn't count against their patience. `Challenge::outcome` says how the duel stands after a number of ticks (fighting, cut short, a kill, or parted). Tests: `crates/gc_sim/tests/champions.rs`.

### Fog of war
- Visibility per player on a tile-resolution grid of `u8` reference counts (256 × 256 = 64 KB per player). Units add and remove "vision stamps" only when they cross tiles, so cost is proportional to movement, not unit count.
- Explored-ever is a 1-bit-per-tile layer.
- The renderer receives the local player's fog as a small texture and blurs it in the shader.

**Built so far (cover, [20 · Ambushes](20-ambushes.md#architecture)):** `gc_sim::vision` tracks, per regiment slot and per party (eight parties), how long the party has seen it, how long a lost sighting lingers, volley reveals and off-guard timers, about 26 bytes a regiment, all reserved when the match starts. Only regiments in cover search for spotters (with combat's spatial hash); on a map without cover it does nothing and adds nothing to the state hash. It is the first step of fog of war: sight ranges, line of sight and the explored layer come with M3.
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
- Each soldier is a ~36-byte puppet that steers toward its formation slot with a little lag and separation, so blocks ripple and jostle rather than sliding rigidly. The front two ranks of a fighting regiment lunge at the enemy. When the simulation removes soldiers from the back slots, the puppets show a front-rank soldier falling (anyone, for a block under arrows) and the soldier from the emptied slot stepping up into the gap; the fallen lie on the field for 20 s, then sink away.
- Melee pairs, sword swings, arrows arcing to individual soldiers: all chosen by presentation to match the sim's numbers.
- **Built so far (soldier fighting, stage 1):** puppets move like bodies, not cursors. Each is pulled to its slot by a slightly under-damped spring that matches most of the block's own pace, with acceleration capped and lower for each rank back, so soldiers gather speed, the front sets off before the rear, and a halting block overshoots a little and settles. At rest they sway and glance about (drawn only; the stored puppet stays put). Once a frame `Puppets::pair` matches each front-rank soldier of a regiment in melee to the nearest front-rank enemy within 2.6 tiles (from hostile regiments within 24 tiles, using the sim's `hostile` test), keeping him while he stays within 3.4 tiles. A paired soldier closes to his man on a 0.6-tile leash from his slot, faces him, and the pair step in and back half a beat apart, swinging out of step with each other. The rank behind keeps lunging, some of it waiting its turn.
- **Built so far (soldier fighting, stage 2, duels that read the numbers):** each block hands the puppets a `Fighter` (melee attack, its defence in its formation against blows on its front and on its flanks or rear from `UnitStats::melee_defence_in`, chosen per blow by the sim's rule for which side faces the striker, the sim's heart and weapon-room percentages, shield, mass, big blade) and its charge ticks left (the new `RegimentView::charge`; a rise is a charge hitting home). The chance a blow lands is the sim's own `combat::hit_percent`. A pair's rounds are 1.25 s, half a round out of step; a soldier's swing takes 0.7 s and its blow lands 45% of the way in. `Puppets::pair` resolves each blow that lands that frame with a stable hash of the pair and the blow's number: stagger (the sim's chance to hit), crush-through and knockdown (25% per class of mass a heavy striker has over his man), else parry or shield block. A stagger (0.6 s) or knockdown (1.8 s) costs any swing begun before it ends and shoves the man back; a parry doesn't. When a heavy regiment's charge hits home, its soldiers who pair off within 0.6 s knock their man down (40% per class) or stagger him (half the rest). When the sim removes a soldier from a regiment in melee, `fall` picks a front-rank man who has a partner (one whose partner is fighting him back, if any), the partner starts a killing swing, and the corpse is laid down with its fall 0.35 s ahead (it stands, in the first frame of its death clip, until the blow lands). The soldier reports a `Move` (guard, swing, parry, block, stagger, down) and `Clips::fight` turns it into a pose, playing one-shot clips at a chosen fraction (`Clips::at`). Still none of it touches the simulation. **Champions** (2026-10-15): `Puppets::champions` takes the sim's `Challenge` each frame; the middle soldier of each champion regiment's front rank leaves his slot, runs to his side of the duel spot (at 5.5 tiles a second, so he arrives about when the sim says they meet) and shows each beat as the sim rolled it: his own swing, a stagger when a blow lands on him, a parry or block when it misses. The loser falls to the winner's last blow (he is the soldier `fall` picks when the sim takes the man away), and the winner cheers for 2.5 s before walking back to his slot.
- Presentation never writes back to the sim, and it is allowed to be non-deterministic. Two players may see slightly different soldiers fall; they always see the same regiments with the same model counts.

Cost: ~10,000 puppets updated with SIMD-friendly arrays on the main thread in about 0.5 ms. If needed, puppets move to a GPU compute pass later.

## AI players and world parties

Designed in [18 · AI](18-ai.md); summarised here for the engine's sake.

- **Rules in the sim, minds outside it.** Anything that must be identical everywhere (regiment reflexes, creep and hamlet behaviour, the relations ledger, contracts and the rule that answers an offer) is ordinary deterministic sim code. Anything that makes choices (AI players, mercenary companies, factions) is a *mind* in `gc_ai` on the AI thread, which acts only through commands.
- **Minds are players.** They use the public views (`RegimentView` and friends), filtered by their party's fog of war, and the same `Command`s as humans. On maps with woods and tall grass each mind's `Perception` already leaves out the enemies its side can't see (`Perception::capture_for`), and the simulation refuses attack orders on them. Replays, saves and golden hashes never depend on AI code.
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

**Built so far (engine upgrade step 3):** the sun casts shadows and the frame is drawn in HDR with a glow. The shadow pass draws terrain, soldiers (at the detail level they are drawn at, position only) and boxes from the sun into a 2048² 16-bit depth map (8 MB), using the world pipelines' own vertex shaders with the sun's view swapped in. The map is a square fitted around the ground the camera sees, out to 400 tiles (past the far corners at the widest zoom); its orientation never changes and it moves in whole texels as the view pans, so shadow edges stay still, and its size changes in 16-tile steps as the view zooms. Every sunlit surface looks itself up in the map with four filtered taps (a soft edge about three texels wide), pushed out along its normal by 1.5 texels against self-shadowing, and the shadow fades out over the outer 5% of the map, which the fitted square keeps clear of the view. Terrain casts in real time for now (hills shadow the slopes behind them); baking terrain lighting at map load, as planned above, waits on a profile showing the shadow pass costs too much. The world pass renders into an HDR image (`Rg11b10Ufloat`, 4 bytes a pixel, where the GPU can draw into it, else `Rgba16Float`), fog included. The glow then keeps light above about 2.2 (sunlit white is about 1.7, so in practice sun glints on metal and emitted light), shrinks it down a chain of up to six half-size images with a 13-tap filter, and adds each level's tent-blurred image back onto the one above (Jimenez, Call of Duty: Advanced Warfare, 2014). The screen pass tone-maps the HDR image plus the glow with the same soft curve as before, then draws the menus and the dev overlay on top. `--no-shadows` and `--no-glow` turn each off; the colour-grading LUT and FXAA are not built yet.

### Terrain
- Heightmap at 4 samples per tile (`R16`, 1024² on Warfield = 2 MB) sampled in the vertex shader over a shared grid mesh, drawn in 32 × 32-tile chunks with frustum culling and two LOD steps.
- Splat-map blending of up to 8 tiling materials from a texture array, plus painted decals.

### Units: instancing and vertex-animation textures
- Every soldier, of every regiment of a given unit type, is drawn in **one instanced draw call**.
- Per-instance data is packed into **16 bytes**: quantised position (2 × `u16`), facing, animation id and frame, team colour row, flags. 10,000 soldiers is 160 KB of upload per frame.
- Animation uses **bone animation textures**: each unit type's skeleton poses are baked per frame into a small texture, and each vertex follows up to four weighted bones (smooth skinning, so high-detail models bend at the joints instead of creasing). About 100 to 150 KB per unit type for all its animations, against several megabytes for per-vertex animation textures.
- Meshes use 16-bit quantised positions, octahedral-encoded normals and a palette index per vertex: around 8 bytes per vertex.
- **Built so far (M1 step 8):** soldiers are drawn from `assets/models.pak` (baked by `tools/gc_artbake`, format in `gc_assets::model`), with one instanced draw per model and level of detail. Vertices are 32 bytes (model schema 3: quantised position, team-colour weight, material, up to four bones with byte weights summing to 255, normal, tangent, colour sampled from the model's texture, texture coordinate). Animation is a bone animation texture: one `Rgba16Float` row per frame holding a 3×4 matrix for each of up to 63 bones plus one that never moves; the shader blends two frame rows, then blends each vertex's bone matrices by weight. `gc_artbake` keeps a glTF vertex's four heaviest joints (from up to eight) and drops any under 5% of it (each bone costs the shader six texture reads); parts parented to a bone instead of skinned follow that bone fully. Meshes bound in a different pose from the skeleton's rest pose (some Quaternius models are) stay in their bind pose, and each bone's matrices absorb the difference, so the blend is exact skinning. The tangent (copied from the source when it has one) and texture coordinate serve textured models. **Textured soldiers (model schema 3):** a pack's `models/textures` table holds three RGBA8 layers per material, 512 × 512 with all mips: albedo with the team mask in alpha (sRGB), normal x and y with occlusion and roughness (linear), and emitted light with metalness in alpha. The renderer uploads them as one texture array viewed both as sRGB and as linear; each vertex names its material (0 for vertex colours only; all of a model's vertices agree), and textured models draw with their own shader pair, skipping back faces (they are built closed, unlike the stand-ins' single-sided cloaks; the baker reverses the winding of mirrored parts). Their reduced mesh keeps texture seams intact. `gc_artbake` reads a model's maps from PNGs listed under `textures` in `models.ron`, shrinks them with box filtering (normals averaged as vectors), and refuses a textured model without tangents. Repeating texture coordinates are still clamped to the texture, and tangents are not generated, so textured models must bring their own (Blender's glTF export does). Instances are still 36 bytes (`SoldierInstance`: position, facing, two frame rows, blend, highlight, fallen flag, team colour, lift, scale) with up to 65,536 of them; packing them to 16 bytes is left until a profile shows upload cost matters. The five models plus all their clips and the one material are 5.3 MB, 4.2 MB of it textures. Selection plates, order previews, arrows and Commander View blocks are still `BoxInstance` boxes, and the game falls back to boxes for soldiers if the model pack fails to load.

### Level of detail and Commander View
- Soldiers have two mesh LODs (full and ~40% triangles).
- Beyond a zoom threshold the renderer switches to **Commander View**: soldiers are not drawn at all; each regiment is a single banner-block. Rendering cost at full zoom-out is *lower* than mid-zoom, the opposite of most engines.
- **Built so far (M1 step 8):** the reduced mesh is drawn for soldiers more than 70 tiles from the camera (`gc_artbake` targets about 1,400 and 500 triangles per model with `meshopt`; the textured Yari Ashigaru uses 8,400 and 1,800 until the longer detail chain arrives). Commander View switches on beyond a camera distance of 175 and off below 160. The soldier puppets keep moving while it is on, so zooming back in shows them where they should be.

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
- In solo sandbox, RON files hot-reload for balance tuning. **Built so far (M1 step 7):** `Shift`+`R` re-bakes `data/units.ron` with `gc_bake` (the same checks as the build) and sends a `TuneUnit` command for each type that changed; the stats panel sends the same command. Tuning goes through the command stream like any order, so it is deterministic and will replay. The unit table is part of the state hash. Adding or removing unit types still needs a restart.

## Maps
- Heightmap (`u16`), tile types (`u8`), water depth layer (reserved for naval), object list, Bloodseep sites and a small event-script block.
- **Built so far (2026-10-15):** the source form is a RON description under `data/maps/`, read and built by `tools/gc_mapcheck` (see [07](07-world-and-maps.md#map-format-and-tooling)). `gc_mapcheck::Map` hands out exactly what `gc_sim::Ground` takes (one `u16` height per tile, the water line, a cover byte per tile) plus the placed objects, built with integer arithmetic only so every machine gets the same bytes, and a map hash. Nothing in the game reads it yet; Conquest loading comes with M3. (M2 step 4 generates Field Battle maps from a seed with integer maths instead of shipping baked files.)
- A Warfield map is under 2 MB on disk. The baked terrain lighting and nav sector graph are generated at load (in well under a second) rather than stored.

## UI framework
- Our own **immediate-mode UI** in `gc_ui`: layout in Rust code, rendered as one batched draw, with an MSDF font atlas baked offline (crisp at any UI scale from one small atlas).

**Built so far (battle HUD):** `gc_ui::hud` draws the bottom bar the same way, from plain data: the game gives it the ground colours, the dots, the camera's view and one `Card` per regiment (`HudData`), plus the mouse (`HudInput`), and gets back `HudEvent`s (jump, order, select, centre, command, alert) that `gc_game/src/hud.rs` turns into camera moves, selections and orders. `gc_ui::hero` draws the hero panel from plain data (name, level, hit points, ability ranks, items) and `gc_game` feeds the same abilities into the grid's A, S, D and F slots. `gc_ui::alerts` draws the edge badges and the idle-worker button: the game projects each remembered event onto the screen (`project`), and the glue notices your regiments losing men or breaking by comparing each frame with the last (no sim changes). It has no sim or GPU types, so it is unit tested headlessly.

**Built so far (menus and lobby):** `gc_ui` has no GPU code: each frame `gc_game` feeds it the mouse and keys (`UiInput`), a `Menu` lays out and draws its current screen into a list of `Quad`s (a rectangle or a glyph, in UI pixels), and `gc_render` draws them in one instanced call (`Renderer::set_ui`, the same pipeline as the dev overlay, drawn under it). Text uses the dev overlay's 8 × 16 bitmap font at whole-number sizes; the MSDF font comes later. Screens: main menu, Play (host card and join card), lobby, settings; all are unit tested by clicking through them headlessly. The lobby model (`Slot`, `LobbySettings`, `MatchStart`) and the `Network` trait in `gc_ui::lobby` are the contract with the netcode: `Menu<N: Network>` is generic over it, the game runs it on `gc_net::Lan` (LAN hosting and joining, see [11 · Networking](11-networking.md#transport)), and the tests run it on `LocalNetwork`, a practice lobby on one machine. A `MatchStart` with a `seed` is a network match. Starting a lobby match builds a Field Battle from `MatchStart` (`gc_game/src/battle.rs`): each slot becomes a party with its team in the stance table, its colour and an army from its faction, and each AI slot gets a mind at its level. Settings are RON in the user config folder (`~/Library/Application Support/Grubcraft/` on a Mac, `%APPDATA%\Grubcraft\` on Windows), overridable with `GRUBCRAFT_SETTINGS`.
- Immediate mode suits a UI that is mostly views over game state; no retained widget tree to keep in sync.
- Localisation-ready from day one (strings by key, UTF-8, font fallback), English only at launch.

## Audio engine
- One mixer on the `cpal` audio thread; up to 32 voices with priority and distance culling.
- Effects stored as IMA ADPCM (4:1, decoded per sample while mixing, so they stay compressed in memory).
- Music streamed from the pack in small chunks, decoded on the fly; adaptive layers crossfade on battle intensity.
- The main thread sends play/stop/parameter events through a lock-free queue; the audio thread never allocates.
- **Built so far:** `gc_audio` has the ADPCM codec, the sound bank format, the lock-free queue (a ring of atomics holding one packed command per slot, no `unsafe`), the mixer (32 effect voices, at most 8 of any one sound, lowest priority then quietest voice stolen first; two music layers crossfaded by battle intensity; soft clipping) and `cpal` output (`f32` or `i16` devices, any channel count). The mixer has no device code and is unit-tested without a sound card. Music is held as ADPCM in the embedded bank and looped, not streamed from disk yet (two 19-second loops are about 300 KB); streamed Ogg comes with real recorded music. `gc_sim` knows nothing about audio: `gc_game/src/sounds.rs` reads the same public views the renderer reads (regiments, volleys in flight) after each tick and plays a sound for each change, positioned by distance and side relative to the camera. If there is no output device the game runs silently; if the device is lost mid-game (headphones unplugged) it is reopened automatically (see Diagnostics). Unit voices and per-faction music come later.

## Diagnostics

A crash on a friend's machine has to leave something to read, because stderr is invisible from a Finder-launched app or a double-clicked exe. `gc_platform::diag` does three things, all in the per-user folder that holds `settings.ron`:
- **A log file.** `logln!` is `eprintln!` that also writes `logs/grubcraft.log`, each line stamped with seconds since start. The last four runs are kept (`grubcraft.1.log` is the run before). Startup failures, GPU and audio notes, desync reports and save errors all go through it.
- **A crash report.** `diag::init` installs a panic hook. A panic writes `crashes/crash-<unix time>.txt` with the message, location, thread, backtrace, the build (version and commit), the game data hash, the GPU, the operating system and the simulation tick; the newest 20 are kept. The release profile aborts on panic, but the hook runs first. `grubcraft --crash-test` panics once the game has started, to check the whole chain.
- **Readable backtraces.** Release builds keep the symbol table (about 4 MB), so the backtrace names the functions but gives no line numbers; debug info would make the binary 49 MB instead of 19. On Windows the names come from `grubcraft.pdb`, which the Windows zip carries beside the exe.
- **Safe saves.** `gc_platform::fsutil::write_atomic` writes a temporary file beside the target and renames it over the target, so a crash of the game or a full disk leaves the old file, not a torn one (it does not fsync, which would stall the frame loop, so power loss is not covered; `write_durable` does fsync and is used for settings). Settings, replays and desync reports use it. A `settings.ron` that does not parse is copied to `settings.ron.broken` and logged before the defaults are used, so a typo does not silently wipe a player's settings.
- **Lost devices.** The renderer records what the GPU driver reports from its own threads (`GpuHealth`): a lost device, and in release builds any error nothing was waiting for (debug builds keep wgpu's panic, so a validation error stays loud). The game logs these (a repeated error once, not every frame) from `about_to_wait`; if the device is gone it saves the battle's replay and quits, since nothing can be drawn. The audio output sets a flag when its stream dies (headphones unplugged); `Sounds` then reopens the default output (waiting 3 s after a failure, doubling up to a minute) and re-applies the player's mute, music and battle-layer state. Rebuilding the renderer in place after device loss is not built.
- **Limits.** Only Rust panics are covered. A fault inside a graphics driver or an abort from the allocator leaves just the log. The tick in a report is the last one simulated, so after a match ends it is stale.

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
| `gc_replay` | Run a replay headless at maximum speed and verify its state hashes: `cargo run --release -p gc_replay -- verify` (all of `replays/`), `bless FILE` (store the hashes the sim produces now), `info FILE` (with who played and which build recorded it), `compare A B` (where two machines' recordings of one match, such as desync reports, first disagree). Dumping state at a tick is still to come |
| `gc_bench` | Fixed benchmark scenarios; reports tick time, frame time, memory by tag; enforces budgets |
| `gc_mapcheck` | `cargo run -p gc_mapcheck -- check [FILE\|DIR]` (default `data/maps`) builds each map and reports errors (exit code 1) and the numbers behind them; `png FILE OUT.png` draws a map with its objects. Runs in CI. See [07 · Map format and tooling](07-world-and-maps.md#map-format-and-tooling) |
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
- **Lints:** CI runs Clippy with warnings denied, plus a chosen set of `clippy::pedantic` lints listed in the workspace `Cargo.toml` (lossless casts written with `From`, so every `as` left is one that can truncate or wrap; ignored tests give a reason). `gc_sim` and `gc_fixed` also deny `clippy::float_arithmetic` outside tests, beside the token check in `tests/no_floats.rs`.
- **Coverage** (`cargo llvm-cov`, measured in the M1 cleanup on 2026-10-15, by lines): `gc_sim` 98%, `gc_fixed` 98%, `gc_ai` 96%, `gc_net` 93%, `gc_ui` 91%, `gc_assets` 89%, `gc_replay` 71% (its command-line tool is run by CI rather than tests). The gaps are the GPU code in `gc_render` (14%) and the model baker `gc_artbake` (9%), which need a GPU or the source art to run; `gc_audio` and `gc_game` weren't measured (they need sound libraries).
- **AI drills and arena:** tactical puzzles with expected outcomes run as tests, and nightly AI-against-AI ladders in `gc_arena` that check each difficulty beats the one below ([18 · AI](18-ai.md#testing-tuning-and-explaining)).

**Built so far:** unit tests per crate (fixed-point maths, formations, group orders, combat, morale, pathfinding, data baking, puppets); a golden determinism test that runs a scripted 4-player drill (moves, group moves and fights) for 2,400 ticks, on an open plane and again on a ridged test map where most orders need routes, and compares state hashes on macOS, Windows and Linux CI; the no-float and no-allocation tests (the latter on the ridged map); and the `gc_bench` drill, pathing-storm and grand-battle benchmarks. **Golden replays (2026-10-15):** recorded battles in `replays/` (see `replays/README.md`): a 1v1 and a 2v2 Field Battle between computer players, a 1v1 in woods, a 1v1 with sandbox tuning and removal mid-fight, the 6,000-soldier `field-8000pt-2v2` brawl, `field-champions-1v1`, where champions duel to a kill before the armies close, and (2026-10-16) `field-deploy-1v1`, which opens with the deployment phase people play: zones, placing inside and outside them, a formation change, one side ready and the other running out the clock. A test (`the_goldens_cover_every_kind_of_command`) checks that the goldens together give every kind of command. Each stores its unit table and map, so retuning `data/units.ron` or the map generator never moves them; only simulation changes do. `cargo test` checks them (`gc_replay/tests/goldens.rs`) and CI plays them again in release mode on macOS, Windows and Linux. They were recorded headless by `gc_game`'s `goldens.rs` (an ignored test, run by hand); a PR that changes how battles play out re-blesses them with `gc_replay bless` in the same PR.

## Dependency policy
- Every third-party crate is listed in `docs/dependencies.md` with its purpose and its cost (binary size, transitive dependencies).
- No async runtime, no large frameworks, no C++ build dependencies without a written justification.
- `cargo deny` checks licences and duplicates; `cargo bloat` output is tracked so binary size regressions are visible.
