# Dependencies

Every third-party crate the game uses, why, and what it costs. Policy: see [10 · Architecture](10-architecture.md#dependency-policy). Adding a crate means adding a row here in the same pull request.

## Shipped in the game

| Crate | Used by | Why | Cost and notes |
|---|---|---|---|
| `wgpu` 30 | `gc_render` | One GPU API over Metal (Mac), DirectX 12 (Windows) and Vulkan | The largest dependency. Built with only the `metal`, `dx12`, `vulkan`, `wgsl` backends/frontends; GL and WebGPU are off |
| `winit` 0.30 | `gc_game` | Windows, input, Mac trackpad gestures | 0.31 is still in beta; move when it's stable |
| `glam` 0.34 | `gc_render`, `gc_game` | Vector/matrix maths for rendering and camera (floats, never in the sim) | Small, SIMD |
| `bytemuck` 1 | `gc_render` | Safe casting of instance data to GPU bytes | Tiny |
| `pollster` 1 | `gc_game` | Blocks on wgpu's async setup calls; avoids an async runtime | Tiny |
| `xxhash-rust` 0.8 (`xxh3` only) | `gc_sim` | Fast, well-distributed per-tick state hashes for desync detection | Pure Rust, `no_std` |

## Development only

| Crate | Used by | Why |
|---|---|---|
| `naga` 30 (`wgsl-in`) | `gc_render` tests | Validates shaders in CI, where there is no GPU |

## Deliberately not used

- **Async runtimes** (`tokio`, `async-std`): lockstep networking needs one UDP socket and a thread.
- **Engines and ECS frameworks** (`bevy`, `hecs`, ...): see [ADR 0001](adr/0001-engine-and-language.md).
- **`rand`**: the sim uses its own PCG32 in `gc_fixed`, so the random stream can never change under us with a crate update.
- **`libc`, `windows-sys`, `mach2`** for memory stats: the three OS calls we need are declared directly in `gc_platform::memory`.
