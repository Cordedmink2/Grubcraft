# Grubcraft

A four-player LAN real-time strategy game where Warcraft III's base-building meets Shogun: Total War's regiment battles, fought over the corpse of a fallen god. Built on a lean custom engine for Mac and Windows, with a 4 GB memory budget for a full 4-player war.

- **Conquest** (main mode): gather Grub and Marrow, build a base, tech up, and command regiments with formations, flanks and morale. 2 to 4 players, 20 to 60 minutes.
- **Field Battle**: pre-built armies, one battlefield, 5 to 20 minutes.
- **Four factions** with four different economies: the Ember Clans, the Brood, the Choir and the Carrion Concord.

## Status

Milestone M0 (foundations) in progress; see the [roadmap](docs/13-roadmap.md). Start with the [design documents](docs/README.md).

## Building and running

You need [Rust](https://rustup.rs) (stable). Then, from this folder:

```sh
cargo run --release -p gc_game          # play the M0 build
cargo test --workspace                  # all tests, including determinism
cargo run --release -p gc_bench         # simulation benchmark with budget checks
./scripts/bundle-macos.sh               # macOS: build Grubcraft.app (universal) and zip it
```

**M0 controls:** you are red. Left-click a red block to select it, right-click the ground to move it. Pan with the arrow keys, screen edges or a two-finger trackpad scroll; zoom with the mouse wheel or a pinch; rotate with Alt + middle-drag or a two-finger twist; Home or a middle-click resets the rotation. **F11** prints memory by subsystem; **Esc** quits. The window title shows frame rate, simulation tick time and memory.

To measure memory on your machine: `cargo run --release -p gc_game -- --frames 600` runs for 600 frames and prints the numbers. Measurement flags: `--latency 1|2|3` (frames in flight), `--render-scale 0.25..1` (render fewer pixels than the window has) and `--draw all|terrain|blocks|none` (turn parts of the frame off).

## Layout

| Path | What |
|---|---|
| `crates/gc_fixed` | Deterministic fixed-point maths (no floats) |
| `crates/gc_sim` | The deterministic simulation; runs identically on every machine |
| `crates/gc_platform` | Memory accounting and OS process stats |
| `crates/gc_render` | wgpu renderer (Metal, DirectX 12, Vulkan) |
| `crates/gc_game` | The game application |
| `tools/gc_bench` | Headless benchmarks with budget gates |
| `docs/` | Design documents |
