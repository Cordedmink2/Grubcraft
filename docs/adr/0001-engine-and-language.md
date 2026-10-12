# ADR 0001 · Engine and language

- **Status:** Accepted (approved by Mario, 2026-10-12)
- **Date:** 2026-10-12

## Context

Grubcraft needs to:
- Run on macOS (Apple Silicon and Intel) and Windows, with Mac and Windows players in the same match.
- Be **absurdly lightweight**: target 160 MB peak memory for a 4-player match with thousands of soldiers ([12 · Performance](../12-performance.md)).
- Support deterministic lockstep networking, which requires bit-identical simulation across CPU architectures.
- Be built mostly by Claude sessions working in this repo, reviewed by Mario. Code that is easy to read, test and reason about matters more than visual editors.

## Options considered

| Option | Memory floor and control | Determinism | Cross-platform | Fit for this project |
|---|---|---|---|---|
| **Unreal Engine** | Very heavy runtime and install; little control over the floor | Floats throughout; lockstep RTS is an uphill fight | Excellent | Wrong tool: built for high-end 3D, editor-centric workflow |
| **Unity** | Heavy runtime and managed memory; garbage collector | Possible with effort (fixed-point libraries) | Excellent | Editor-centric; GC and engine overhead fight the memory goal |
| **Godot 4** | Lighter, but a general-purpose runtime we'd carry entirely | Possible with effort in GDExtension | Excellent | Good engine, but its floor and node-tree architecture work against "every byte justified"; large unused feature surface |
| **Bevy (Rust)** | Moderate; ECS framework, plugin stack | Possible (Rust, fixed-point) | Good | Closest alternative. Larger dependency tree, frequent breaking releases, ECS generality we don't need |
| **Custom engine, Rust + wgpu + winit** | **Lowest; every allocation is ours** | **Designed in: integer-only sim crate** | Good (Metal, DX12, Vulkan) | Strong fit: code-first, testable, small, deterministic |
| Custom engine, C or Zig + sokol | Lowest | Designed in | Good | Smaller binaries still, but less memory safety and weaker tooling for long-lived, AI-assisted development |

## Decision

Build a **small custom engine in Rust**, using `winit` (windowing and input), `wgpu` (Metal and DirectX 12), and `cpal` (audio output), with everything game-specific written by us. See [10 · Architecture](../10-architecture.md).

## Why

1. **The memory goal is only reachable if we own the floor.** A general-purpose engine brings a fixed cost before the game loads anything, plus subsystems we would never use. With a custom engine the budget table is literally the engine.
2. **Determinism is a design property, not a patch.** A `no_std`, integer-only simulation crate cannot accidentally read a clock, use a float or iterate a randomly seeded hash map. That makes Mac-versus-Windows lockstep reliable, and replays, rejoin and bots cheap.
3. **An RTS needs a narrow slice of an engine.** Fixed-pitch camera, instanced units, terrain, a 2D UI, positional audio, UDP. That is a well-understood, bounded amount of code.
4. **Rust suits AI-assisted development.** The compiler catches whole classes of bugs before review; tests and benchmarks run from the command line; no binary scene files to merge.
5. **`wgpu` gives native GPU APIs on both platforms** (Metal on Mac, DirectX 12 on Windows) without us maintaining two renderers.

## Consequences

- **More upfront work** before the first playable: windowing, rendering, UI and audio basics are ours to write (milestone M0). Mitigated by a deliberately small scope and by building the playable sandbox in M1.
- **No visual editor.** Maps are edited in an in-game edit mode; units and balance live in text data files. This suits a code-first workflow.
- **Art pipeline is ours:** a Blender export script and `gc_bake` instead of an engine importer.
- **Escape hatch:** if the custom path stalls badly in M0 or M1, the simulation crate (`gc_sim`) is engine-independent and could be hosted inside Bevy with a new renderer and UI. The most valuable code survives either way.

## Revisit if

- M0 takes more than about three times its expected effort.
- `wgpu` shows a blocking problem on a target machine (driver bugs, memory floor far above budget).
- Mario decides development speed matters more than the memory goal.
