//! Particles: dust, smoke, fire, sparks, debris, ichor, crystal shards and
//! floating lights, one system for all of them.
//!
//! The CPU never moves a particle. [`crate::Renderer::spawn_particles`] gives
//! each one its start, velocity, forces and look once; they go into a ring
//! buffer ([`MAX_PARTICLES`] slots, the oldest overwritten first), and the
//! vertex shader (`vs_particle` in `shaders/particles.wgsl`) works out where
//! every particle is from its age: constant force and drag in closed form,
//! so a spawned burst costs the CPU nothing per frame. Each particle is a
//! camera-facing quad (or a streak stretched along its velocity) cut from one
//! small shared atlas ([`atlas`]), lit by the sun, sky and shadows at its
//! centre, fogged and hidden by the fog of war like everything else, and
//! blended premultiplied into the HDR image: alpha for dust and smoke,
//! added light for fire, sparks and motes.
//!
//! Nothing here touches the simulation: where particles come from is the
//! caller's business (`gc_game`'s effects), and [`crate::effects`] holds the
//! recipes. Particles are drawn after the sky in the world pass, tested
//! against the world's depth and writing none.

use std::collections::VecDeque;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

/// Slots in the ring buffer: the most particles alive at once.
pub const MAX_PARTICLES: usize = 32_768;

/// Behaviour flags of a [`Particle`]; the shader reads the same bits.
pub mod flags {
    /// Stretch along the velocity into a streak, head first.
    pub const STRETCH: u8 = 1;
    /// Add light instead of blending: fire, sparks, motes. The colour is
    /// then HDR-boosted by the shader and fog dims it.
    pub const ADDITIVE: u8 = 2;
    /// Come to rest on the ground instead of falling through it.
    pub const GROUND: u8 = 4;
    /// Fade out toward the ground under it, so a puff of dust does not
    /// show a hard line where it meets the hill.
    pub const SOFT: u8 = 8;
    /// Drift downwind.
    pub const WIND: u8 = 16;
    /// Flicker in brightness.
    pub const FLICKER: u8 = 32;
}

/// Which picture of the atlas a particle uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Shape {
    /// Four ragged round puffs, for dust and smoke.
    PuffA,
    PuffB,
    PuffC,
    PuffD,
    /// A soft round glow: fire cores, motes, embers.
    Glow,
    /// A tapering streak, bright at the head: sparks and droplets.
    Streak,
    /// A hard-edged crystal or chip: shards, debris.
    Shard,
    /// A thin ring: shock rings.
    Ring,
}

impl Shape {
    pub const PUFFS: [Shape; 4] = [Shape::PuffA, Shape::PuffB, Shape::PuffC, Shape::PuffD];
    /// Every picture, in atlas order.
    const ALL: [Shape; 8] = [
        Shape::PuffA,
        Shape::PuffB,
        Shape::PuffC,
        Shape::PuffD,
        Shape::Glow,
        Shape::Streak,
        Shape::Shard,
        Shape::Ring,
    ];
}

/// One particle to spawn. Lengths are tiles, times seconds; `y` is up.
///
/// It moves by `vel` and the forces below, with the drag: its velocity is
/// `(vel - a/drag) * exp(-drag * t) + a/drag` for the acceleration
/// `a = lift - gravity` (up), so a heavy chip falls and slides to rest and a
/// puff of smoke rises to a slow drift.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub pos: Vec3,
    pub vel: Vec3,
    /// How old it already is at spawn, for particles that began since the
    /// last frame.
    pub age: f32,
    pub life: f32,
    /// Radius (half the quad's width) at birth and at death.
    pub size: [f32; 2],
    /// sRGB colour and opacity at birth and at death, blended along its life.
    pub color: [u8; 4],
    pub color_end: [u8; 4],
    pub gravity: f32,
    pub drag: f32,
    pub lift: f32,
    pub shape: Shape,
    pub flags: u8,
    /// Spin in quarter radians a second (a signed byte).
    pub spin: i8,
    /// Varies the turn it starts in, its flicker and its wind.
    pub seed: u8,
}

impl Default for Particle {
    fn default() -> Particle {
        Particle {
            pos: Vec3::ZERO,
            vel: Vec3::ZERO,
            age: 0.0,
            life: 1.0,
            size: [0.1, 0.1],
            color: [255, 255, 255, 255],
            color_end: [255, 255, 255, 0],
            gravity: 0.0,
            drag: 0.0,
            lift: 0.0,
            shape: Shape::PuffA,
            flags: 0,
            spin: 0,
            seed: 0,
        }
    }
}

/// A particle as the GPU reads it. 64 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub(crate) struct ParticleInstance {
    pos: [f32; 3],
    vel: [f32; 3],
    /// The renderer's effects clock at birth. Pending particles hold minus
    /// their age here until the frame adds the clock.
    start: f32,
    life: f32,
    size: [f32; 2],
    color: [u8; 4],
    color_end: [u8; 4],
    /// Gravity, drag and lift.
    motion: [f32; 3],
    /// Shape, flags, spin and seed.
    meta: [u8; 4],
}

impl From<&Particle> for ParticleInstance {
    fn from(p: &Particle) -> ParticleInstance {
        ParticleInstance {
            pos: p.pos.to_array(),
            vel: p.vel.to_array(),
            start: -p.age,
            life: p.life.clamp(0.01, MAX_LIFE),
            size: p.size,
            color: p.color,
            color_end: p.color_end,
            motion: [p.gravity, p.drag, p.lift],
            meta: [p.shape as u8, p.flags, p.spin as u8, p.seed],
        }
    }
}

/// The longest a particle lives, in seconds: longer lives are cut to it.
const MAX_LIFE: f32 = 8.0;

/// The ring of slots, and which of them can still hold a live particle.
#[derive(Clone, Debug, Default)]
struct Ring {
    head: usize,
    /// Batches written, oldest first: when the last of each is dead, and how
    /// many slots it took.
    batches: VecDeque<(f32, usize)>,
}

impl Ring {
    /// Make room for `n` (at most the capacity) more, all dead by `dies`: the
    /// (first slot, count) runs to write them to, in order (one run, or two
    /// when it wraps past the end of the ring).
    fn push(&mut self, n: usize, dies: f32) -> [(usize, usize); 2] {
        debug_assert!(n <= MAX_PARTICLES);
        let first = n.min(MAX_PARTICLES - self.head);
        let runs = [(self.head, first), (0, n - first)];
        self.head = (self.head + n) % MAX_PARTICLES;
        self.batches.push_back((dies, n));
        runs
    }

    /// Forget the batches that are dead by effects time `now`.
    fn expire(&mut self, now: f32) {
        while self.batches.front().is_some_and(|&(dies, _)| dies <= now) {
            self.batches.pop_front();
        }
    }

    /// The (first instance, end) runs of the slots that may be alive: those
    /// written most recently, up to the whole ring.
    fn live_runs(&self) -> [(u32, u32); 2] {
        let live = self
            .batches
            .iter()
            .map(|&(_, n)| n)
            .sum::<usize>()
            .min(MAX_PARTICLES);
        let start = (self.head + MAX_PARTICLES - live) % MAX_PARTICLES;
        if live == 0 {
            [(0, 0); 2]
        } else if start < self.head {
            [(start as u32, self.head as u32), (0, 0)]
        } else {
            [(start as u32, MAX_PARTICLES as u32), (0, self.head as u32)]
        }
    }
}

// ---------------------------------------------------------------- atlas

/// Texels on a side of one picture of the atlas, and how many pictures lie
/// across and down.
const CELL: usize = 128;
const CELLS: [usize; 2] = [4, 2];
/// Mip levels; the pictures fade to nothing before their edges, so the
/// coarser levels do not bleed into the neighbours.
const MIPS: usize = 4;

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x27d4_eb2d) ^ (y as u32).wrapping_mul(0x1656_67b1);
    h ^= seed.wrapping_mul(0x9e37_79b9);
    h = (h ^ (h >> 15)).wrapping_mul(0x2c1b_3c6d);
    h = (h ^ (h >> 12)).wrapping_mul(0x297a_2d39);
    (h ^ (h >> 15)) as f32 / u32::MAX as f32
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let (ix, iy) = (x.floor(), y.floor());
    let (fx, fy) = (x - ix, y - iy);
    let (ux, uy) = (smoothstep(0.0, 1.0, fx), smoothstep(0.0, 1.0, fy));
    let (ix, iy) = (ix as i32, iy as i32);
    let a = hash(ix, iy, seed);
    let b = hash(ix + 1, iy, seed);
    let c = hash(ix, iy + 1, seed);
    let d = hash(ix + 1, iy + 1, seed);
    (a + (b - a) * ux) + ((c + (d - c) * ux) - (a + (b - a) * ux)) * uy
}

fn fbm(x: f32, y: f32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut scale = 1.0;
    for octave in 0..4 {
        sum += amp * value_noise(x * scale, y * scale, seed + octave);
        amp *= 0.5;
        scale *= 2.03;
    }
    sum / 0.9375
}

/// How much of a picture shows at `r` from its centre (0 to 1 at the edge):
/// everything inside, none at the border.
fn window(r: f32) -> f32 {
    1.0 - smoothstep(0.8, 0.98, r)
}

/// The density of `shape` at (x, y), each from -1 to 1.
fn density(shape: Shape, x: f32, y: f32) -> f32 {
    let r = x.hypot(y);
    let value = match shape {
        Shape::PuffA | Shape::PuffB | Shape::PuffC | Shape::PuffD => {
            let seed = shape as u32 * 17 + 3;
            let warp = fbm(
                x * 1.7 + 3.1 * seed as f32,
                y * 1.7 - 1.3 * seed as f32,
                seed,
            );
            let d = r + (warp - 0.5) * 0.75;
            let body = 1.0 - smoothstep(0.1, 0.9, d);
            let grain = 0.65 + 0.35 * fbm(x * 3.3 + 9.0, y * 3.3 + seed as f32, seed + 40);
            body * grain
        }
        Shape::Glow => (-r * r * 5.0).exp(),
        Shape::Streak => {
            let u = (x * 0.5 + 0.5).clamp(0.0, 1.0);
            let width = 0.05 + 0.15 * u;
            u.powf(1.3) * (-(y / width) * (y / width)).exp()
        }
        Shape::Shard => {
            // A kite: narrow, long and a little crooked, with a bright
            // facet on one side.
            let q = x.abs() / 0.55 + (y - 0.12 * x).abs() / 0.9;
            smoothstep(1.0, 0.88, q) * if x > 0.0 { 1.0 } else { 0.72 }
        }
        Shape::Ring => {
            let d = (r - 0.72) / 0.11;
            let wobble = 0.6 + 0.4 * fbm(y.atan2(x) * 1.9 + 4.0, 2.0, 77);
            (-d * d).exp() * wobble
        }
    };
    let edge = if matches!(shape, Shape::Streak) {
        // A streak runs the cell's whole width, so its ends fade by x.
        1.0 - smoothstep(0.85, 0.99, x.abs())
    } else {
        window(r)
    };
    (value * edge).clamp(0.0, 1.0)
}

/// The atlas as mip levels of single-channel texels, finest first, with
/// each level's width and height.
pub(crate) fn atlas() -> Vec<(u32, u32, Vec<u8>)> {
    let (w, h) = (CELL * CELLS[0], CELL * CELLS[1]);
    let mut level = vec![0u8; w * h];
    for (i, &shape) in Shape::ALL.iter().enumerate() {
        let (ox, oy) = ((i % CELLS[0]) * CELL, (i / CELLS[0]) * CELL);
        for ty in 0..CELL {
            for tx in 0..CELL {
                let x = (tx as f32 + 0.5) / CELL as f32 * 2.0 - 1.0;
                let y = (ty as f32 + 0.5) / CELL as f32 * 2.0 - 1.0;
                level[(oy + ty) * w + ox + tx] = (density(shape, x, y) * 255.0 + 0.5) as u8;
            }
        }
    }
    let mut levels = vec![(w as u32, h as u32, level)];
    for _ in 1..MIPS {
        let (pw, ph, prev) = levels.last().expect("a first level");
        let (nw, nh) = (*pw as usize / 2, *ph as usize / 2);
        let mut next = vec![0u8; nw * nh];
        for y in 0..nh {
            for x in 0..nw {
                let at = |dx: usize, dy: usize| {
                    u32::from(prev[(2 * y + dy) * *pw as usize + 2 * x + dx])
                };
                next[y * nw + x] = ((at(0, 0) + at(1, 0) + at(0, 1) + at(1, 1) + 2) / 4) as u8;
            }
        }
        levels.push((nw as u32, nh as u32, next));
    }
    levels
}

// ---------------------------------------------------------------- GPU

/// The particle shader: the shared world part followed by the particles.
pub(crate) fn shader_source() -> String {
    format!(
        "{}\nconst FX_STRETCH: u32 = {}u;\nconst FX_ADDITIVE: u32 = {}u;\nconst FX_GROUND: u32 = {}u;\n\
         const FX_SOFT: u32 = {}u;\nconst FX_WIND: u32 = {}u;\nconst FX_FLICKER: u32 = {}u;\n\
         const FX_COLUMNS: u32 = {}u;\nconst FX_ROWS: u32 = {}u;\n{}",
        crate::rock::world_source(),
        flags::STRETCH,
        flags::ADDITIVE,
        flags::GROUND,
        flags::SOFT,
        flags::WIND,
        flags::FLICKER,
        CELLS[0],
        CELLS[1],
        include_str!("shaders/particles.wgsl")
    )
}

pub(crate) struct ParticlesGpu {
    pipeline: wgpu::RenderPipeline,
    instances: wgpu::Buffer,
    atlas_group: wgpu::BindGroup,
    atlas_bytes: u64,
    ring: Ring,
    /// Particles spawned since the last frame, waiting to be written.
    pending: Vec<ParticleInstance>,
}

pub(crate) fn make(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    world_layout: &wgpu::BindGroupLayout,
    hdr: crate::Hdr,
) -> ParticlesGpu {
    let levels = atlas();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("particle atlas"),
        size: wgpu::Extent3d {
            width: levels[0].0,
            height: levels[0].1,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut atlas_bytes = 0;
    for (mip, (w, h, texels)) in levels.iter().enumerate() {
        atlas_bytes += texels.len() as u64;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: mip as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            texels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(*w),
                rows_per_image: Some(*h),
            },
            wgpu::Extent3d {
                width: *w,
                height: *h,
                depth_or_array_layers: 1,
            },
        );
    }
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("particle atlas"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        ..Default::default()
    });
    let atlas_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("particle atlas"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });
    let atlas_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("particle atlas"),
        layout: &atlas_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(
                    &texture.create_view(&wgpu::TextureViewDescriptor::default()),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("particles"),
        bind_group_layouts: &[Some(world_layout), Some(&atlas_layout)],
        immediate_size: 0,
    });
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("particles.wgsl"),
        source: wgpu::ShaderSource::Wgsl(shader_source().into()),
    });
    let instance_layout = wgpu::VertexBufferLayout {
        array_stride: size_of::<ParticleInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32, 3 => Float32, 4 => Float32x2,
            5 => Unorm8x4, 6 => Unorm8x4, 7 => Float32x3, 8 => Uint8x4
        ],
    };
    // Premultiplied: alpha for dust and smoke, and a zero alpha adds light.
    let premultiplied = wgpu::BlendComponent {
        src_factor: wgpu::BlendFactor::One,
        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
        operation: wgpu::BlendOperation::Add,
    };
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("particles"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_particle"),
            compilation_options: Default::default(),
            buffers: &[Some(instance_layout)],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            cull_mode: None,
            ..Default::default()
        },
        // Tested against the world, writing nothing.
        depth_stencil: Some(wgpu::DepthStencilState {
            format: crate::DEPTH_FORMAT,
            depth_write_enabled: Some(false),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        }),
        multisample: hdr.multisample(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_particle"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: hdr.format,
                blend: Some(wgpu::BlendState {
                    color: premultiplied,
                    alpha: premultiplied,
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    });
    ParticlesGpu {
        pipeline,
        instances: device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("particles"),
            size: (MAX_PARTICLES * size_of::<ParticleInstance>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }),
        atlas_group,
        atlas_bytes,
        ring: Ring::default(),
        pending: Vec::new(),
    }
}

/// Trim a flood of queued particles to what the ring holds, the newest.
fn keep_newest(pending: &mut Vec<ParticleInstance>) {
    let excess = pending.len().saturating_sub(MAX_PARTICLES);
    pending.drain(..excess);
}

impl ParticlesGpu {
    /// The fixed ring buffer plus the atlas, for the static GPU memory total.
    pub fn gpu_bytes(&self) -> u64 {
        self.instances.size() + self.atlas_bytes
    }

    /// Queue particles to start at the next frame. A flood larger than the
    /// ring keeps its newest.
    pub fn spawn(&mut self, particles: &[Particle]) {
        self.pending
            .extend(particles.iter().map(ParticleInstance::from));
        keep_newest(&mut self.pending);
    }

    /// Forget every particle, queued and flying.
    pub fn clear(&mut self) {
        self.pending.clear();
        self.ring = Ring::default();
    }

    /// Write the queued particles into the ring, born at effects time `now`
    /// (less the age they came with).
    pub fn flush(&mut self, queue: &wgpu::Queue, now: f32) {
        self.ring.expire(now);
        if self.pending.is_empty() {
            return;
        }
        // The batch is certainly dead when its longest-lived particle is.
        let dies = now
            + self
                .pending
                .iter()
                .map(|p| p.life + p.start)
                .fold(0.0, f32::max);
        for p in &mut self.pending {
            p.start += now;
        }
        let mut done = 0;
        for (first, count) in self.ring.push(self.pending.len(), dies) {
            if count == 0 {
                continue;
            }
            queue.write_buffer(
                &self.instances,
                (first * size_of::<ParticleInstance>()) as u64,
                bytemuck::cast_slice(&self.pending[done..done + count]),
            );
            done += count;
        }
        self.pending.clear();
    }

    /// Draw the slots that may still hold a live particle (the shader skips
    /// the dead among them) into the world pass. Returns the draw calls.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>) -> u32 {
        let runs = self.ring.live_runs();
        if runs[0].1 == 0 {
            return 0;
        }
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(1, &self.atlas_group, &[]);
        pass.set_vertex_buffer(0, self.instances.slice(..));
        let mut calls = 0;
        for (first, end) in runs.into_iter().filter(|&(first, end)| end > first) {
            pass.draw(0..4, first..end);
            calls += 1;
        }
        calls
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_instance_layout_is_stable() {
        // The vertex attributes in `make` and the shader's inputs depend on it.
        assert_eq!(size_of::<ParticleInstance>(), 64);
    }

    #[test]
    fn the_ring_wraps_and_overwrites_the_oldest() {
        let mut ring = Ring::default();
        assert_eq!(ring.push(10, 1.0), [(0, 10), (0, 0)]);
        assert_eq!(
            ring.push(MAX_PARTICLES - 4, 2.0),
            [(10, MAX_PARTICLES - 10), (0, 6)]
        );
        assert_eq!(ring.head, 6);
        assert_eq!(ring.push(2, 3.0), [(6, 2), (0, 0)]);
    }

    #[test]
    fn the_draw_covers_only_slots_that_may_be_alive() {
        let mut ring = Ring::default();
        assert_eq!(ring.live_runs()[0], (0, 0));
        ring.push(10, 1.0);
        ring.push(5, 2.0);
        assert_eq!(ring.live_runs(), [(0, 15), (0, 0)]);
        ring.expire(1.0);
        assert_eq!(ring.live_runs(), [(10, 15), (0, 0)]);
        // Wrapping: the live slots end at the head, past the ring's end.
        ring.push(MAX_PARTICLES - 20, 3.0);
        ring.push(8, 4.0);
        assert_eq!(ring.head, 3);
        ring.expire(2.0);
        // The 15 slots written first are dead; 3 are at the start.
        assert_eq!(ring.live_runs(), [(15, MAX_PARTICLES as u32), (0, 3)]);
        ring.expire(10.0);
        assert_eq!(ring.live_runs()[0], (0, 0));
    }

    #[test]
    fn a_pending_flood_keeps_the_newest() {
        let mut pending: Vec<ParticleInstance> = (0..MAX_PARTICLES + 5)
            .map(|i| ParticleInstance {
                life: i as f32,
                ..Default::default()
            })
            .collect();
        keep_newest(&mut pending);
        assert_eq!(pending.len(), MAX_PARTICLES);
        assert_eq!(pending[0].life, 5.0);
    }

    #[test]
    fn every_picture_fades_to_nothing_before_its_border() {
        let (w, h, texels) = &atlas()[0];
        let (w, h) = (*w as usize, *h as usize);
        assert_eq!((w, h), (CELL * CELLS[0], CELL * CELLS[1]));
        for cell in 0..CELLS[0] * CELLS[1] {
            let (ox, oy) = ((cell % CELLS[0]) * CELL, (cell / CELLS[0]) * CELL);
            let mut edge = 0;
            let mut centre = 0;
            for i in 0..CELL {
                for (x, y) in [(i, 0), (i, CELL - 1), (0, i), (CELL - 1, i)] {
                    edge = edge.max(texels[(oy + y) * w + ox + x]);
                }
            }
            for y in CELL / 4..CELL * 3 / 4 {
                for x in CELL / 4..CELL * 3 / 4 {
                    centre = centre.max(texels[(oy + y) * w + ox + x]);
                }
            }
            assert_eq!(edge, 0, "picture {cell} reaches its border");
            assert!(centre > 100, "picture {cell} is empty at its centre");
        }
    }

    #[test]
    fn the_atlas_has_its_mips() {
        let levels = atlas();
        assert_eq!(levels.len(), MIPS);
        for pair in levels.windows(2) {
            assert_eq!((pair[1].0, pair[1].1), (pair[0].0 / 2, pair[0].1 / 2));
        }
        let bytes: usize = levels.iter().map(|l| l.2.len()).sum();
        assert!(bytes < 200_000, "the atlas is {bytes} bytes");
    }

    #[test]
    fn the_particle_shader_has_its_entry_points() {
        let source = shader_source();
        let module = naga::front::wgsl::parse_str(&source)
            .unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        let names: Vec<&str> = module
            .entry_points
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        for want in ["vs_particle", "fs_particle"] {
            assert!(names.contains(&want), "no entry point {want}");
        }
    }
}
