# Grubcraft

A four-player LAN real-time strategy game where Warcraft III's base-building meets Shogun: Total War's regiment battles, fought over the corpse of a fallen god. Built on a lean custom engine for Mac and Windows, with a 4 GB memory budget for a full 4-player war.

- **Conquest** (main mode): gather Grub and Marrow, build a base, tech up, and command regiments with formations, flanks and morale. 2 to 4 players, 20 to 60 minutes.
- **Field Battle**: pre-built armies, one battlefield, 5 to 20 minutes.
- **Four factions** with four different economies: the Ember Clans, the Brood, the Choir and the Carrion Concord.

## Status

M0 (foundations) is done apart from a test run on a Windows PC. M1 (regiments on a field) is in progress: formations, soldiers on screen, controls, melee and arrows, and morale are built; pathfinding, the sandbox and real art are next. See the [roadmap](docs/13-roadmap.md). Start with the [design documents](docs/README.md).

## Building and running

You need [Rust](https://rustup.rs) (stable). Then, from this folder:

```sh
cargo run --release -p gc_game          # play the current build
cargo test --workspace                  # all tests, including determinism
cargo run --release -p gc_bench         # simulation benchmark with budget checks
cargo run -p gc_bake                    # check data/*.ron and bake target/grubcraft.pak
cargo run --release -p gc_game --features profile   # build with Tracy profiling
./scripts/bundle-macos.sh               # macOS: build Grubcraft.app (universal) and zip it
```

**Controls:** you are red. Left-click a red regiment to select it, or left-drag a box around several (Shift adds or removes). Right-click the ground to march the selection there side by side, facing the way they travel; right-drag to draw the line they should form instead (its length sets how wide they stand, and they face away from you). A big selection forms several lines. Right-click an enemy regiment to attack it. Your back two rows are Yumi archers, who shoot at enemies in range on their own; blocks that touch fight, and flank, rear and charging hits hurt more. Regiments that lose heart waver (a blinking amber banner), then rout (a white banner): they break ranks and run, ignoring orders, and rally once the enemy is gone; a regiment that routs twice is shattered (a grey banner) for good. Ctrl (Cmd on a Mac) + a number key saves the selection as a control group; the number recalls it (Shift adds it), and pressing it twice centres the camera on it. Pan with the arrow keys, screen edges or a two-finger trackpad scroll; zoom with the mouse wheel or a pinch; rotate with Alt + middle-drag or a two-finger twist; Home or a middle-click resets the rotation. **`** (backquote) or **F11** shows the dev overlay with frame, tick and memory stats (and prints memory to the terminal); **Esc** quits. Start with `--overlay` to show it from the first frame.

To measure memory on your machine: `cargo run --release -p gc_game -- --frames 600` runs for 600 frames and prints the numbers. Measurement flags: `--latency 1|2|3` (frames in flight), `--render-scale 0.25..1` (render fewer pixels than the window has) and `--draw all|terrain|blocks|none` (turn parts of the frame off).

## Layout

| Path | What |
|---|---|
| `crates/gc_fixed` | Deterministic fixed-point maths (no floats) |
| `crates/gc_assets` | Asset pack format: writer and zero-copy reader |
| `crates/gc_data` | Game data tables (units, factions) as the game reads them |
| `crates/gc_sim` | The deterministic simulation; runs identically on every machine |
| `crates/gc_platform` | Memory accounting and OS process stats |
| `crates/gc_render` | wgpu renderer (Metal, DirectX 12, Vulkan) |
| `crates/gc_game` | The game application |
| `tools/gc_bake` | Baker: compiles `data/*.ron` (and later art and audio) into the asset pack |
| `tools/gc_bench` | Headless benchmarks with budget gates |
| `data/` | Game data in RON: edit these to change units (rebuilt automatically) |
| `docs/` | Design documents |
