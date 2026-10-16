//! Soldier models on the GPU: every model's vertices and indices in one
//! buffer each, the bone animation texture, the material texture array, and
//! one instanced draw per model and level of detail.
//!
//! Soldiers are skinned once a frame. A compute pass poses every vertex of
//! every soldier the frame draws (`cs_skin` in `shaders/world.wgsl`) and
//! keeps its world position, normal and tangent; the shadow map and the
//! world pass then read those instead of each skinning the model again.

use std::mem::offset_of;

use bytemuck::{Pod, Zeroable};
use gc_assets::model::{
    BLOCK_SIDE, BLOCKS_PER_LAYER, BONE_TEXELS_PER_ROW, Block, LAYERS_PER_MATERIAL, LODS,
    ModelVertex, Models, TEXTURE_MIPS, TEXTURE_SIZE,
};

/// The finest level of detail the shadow pass draws.
const SHADOW_MIN_LOD: usize = 2;

/// Fixed capacity of the soldier instance buffer; never reallocated.
pub const MAX_SOLDIER_INSTANCES: usize = 65_536;

/// Vertices the skinning pass can pose in a frame, all soldiers together:
/// 24 bytes each (position, normal and tangent), 96 MB in all, though the
/// buffers only grow as far as frames need. A frame that needs more draws the surplus at coarser levels of detail, then leaves it
/// out (and counts it in `RenderStats::dropped`).
pub const SKIN_VERTICES: usize = 4 << 20;
/// Vertices the skinned buffers start with; they grow to the next power of
/// two when a frame needs more (never shrinking), up to `SKIN_VERTICES`.
const SKIN_START: usize = 256 << 10;
/// Threads in a workgroup of `cs_skin`.
const SKIN_GROUP: u32 = 128;
/// Runs of soldiers one skinning pass can be told of (`Jobs` in world.wgsl).
pub(crate) const MAX_JOBS: usize = 256;
// One dispatch must cover every vertex, whatever the GPU.
const _: () = assert!(SKIN_VERTICES.div_ceil(SKIN_GROUP as usize) <= 65_535);

/// The model attributes the world pass still reads, skinning having done
/// the rest: the team weight and material, the last two bytes of the
/// tangent (the second is the bitangent's handedness), colour and texture
/// coordinate.
const MODEL_ATTRIBUTES: [wgpu::VertexAttribute; 4] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint8x2,
        offset: offset_of!(ModelVertex, team) as u64,
        shader_location: 0,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Snorm8x2,
        offset: offset_of!(ModelVertex, tangent) as u64 + 2,
        shader_location: 1,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Unorm8x4,
        offset: offset_of!(ModelVertex, color) as u64,
        shader_location: 2,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Unorm16x2,
        offset: offset_of!(ModelVertex, uv) as u64,
        shader_location: 3,
    },
];

/// The instance attributes the world pass reads: the flags (the highlight
/// is the second byte) and the team colour.
const INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 2] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Unorm8x4,
        offset: offset_of!(SoldierInstance, flags) as u64,
        shader_location: 4,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Unorm8x4,
        offset: offset_of!(SoldierInstance, color) as u64,
        shader_location: 5,
    },
];

/// One soldier to draw. 36 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod, Zeroable)]
pub struct SoldierInstance {
    /// Feet on the map, in tiles.
    pub pos: [f32; 2],
    /// Unit vector the soldier faces.
    pub facing: [f32; 2],
    /// Bone texture rows of the two frames to blend between.
    pub frames: [u16; 2],
    /// Blend toward the second frame, selection highlight, and whether the
    /// soldier lies fallen (for models without a death clip), as 0 to 255.
    pub flags: [u8; 4],
    /// Team colour (rgb).
    pub color: [u8; 4],
    /// Height above the ground, in tiles (negative sinks the fallen).
    pub lift: f32,
    /// Size relative to the model's baked height.
    pub scale: f32,
}

/// A run of instances drawn with one model at one level of detail.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoldierBatch {
    pub model: u16,
    /// Level of detail: 0 is full, up to `LODS - 1` for the fewest
    /// triangles.
    pub lod: u8,
    pub first: u32,
    pub count: u32,
}

/// One model at one level of detail, as the GPU holds it: the vertices both
/// of its meshes need, and the two meshes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Level {
    /// First of the level's vertices in the shared vertex buffer, and how
    /// many. Every soldier drawn at this level has this many skinned.
    pub first: u32,
    pub len: u32,
    /// First index and index count of the mesh the world pass draws, and of
    /// the one the shadow map does (never finer than `SHADOW_MIN_LOD`).
    /// Indices count from the start of the shared vertex buffer.
    pub mesh: [u32; 2],
    pub shadow_mesh: [u32; 2],
}

/// Every model's levels laid out for the GPU.
struct Levels {
    vertices: Vec<ModelVertex>,
    indices: Vec<u32>,
    /// Per model, per level of detail.
    models: Vec<[Level; LODS]>,
}

/// Lay out each level of each model with only the vertices its two meshes
/// use. The levels of a model share vertices in the pack, and the coarser
/// meshes use few of them, so a soldier drawn far away is posed in few.
fn level_vertices(m: &Models<'_>) -> Levels {
    let mut out = Levels {
        vertices: Vec::new(),
        indices: Vec::new(),
        models: Vec::new(),
    };
    for r in m.models {
        let mesh = |lod: usize| {
            let [first, count] = r.lods[lod];
            &m.indices[first as usize..(first + count) as usize]
        };
        let mut levels = [Level::default(); LODS];
        for (lod, level) in levels.iter_mut().enumerate() {
            let shadow_lod = lod.clamp(SHADOW_MIN_LOD, LODS - 1);
            let mut used = vec![false; r.vertex_count as usize];
            for &i in mesh(lod).iter().chain(mesh(shadow_lod)) {
                used[i as usize] = true;
            }
            // Where each used vertex goes in the shared buffer.
            let first = out.vertices.len() as u32;
            let mut at = vec![0; used.len()];
            for (i, _) in used.iter().enumerate().filter(|(_, u)| **u) {
                at[i] = out.vertices.len() as u32;
                out.vertices.push(m.vertices[r.first_vertex as usize + i]);
            }
            let mut add = |lod: usize| {
                let start = out.indices.len() as u32;
                out.indices
                    .extend(mesh(lod).iter().map(|&i| at[i as usize]));
                [start, mesh(lod).len() as u32]
            };
            let main = add(lod);
            let shadow = if shadow_lod == lod {
                main
            } else {
                add(shadow_lod)
            };
            *level = Level {
                first,
                len: out.vertices.len() as u32 - first,
                mesh: main,
                shadow_mesh: shadow,
            };
        }
        out.models.push(levels);
    }
    out
}

/// A run of soldiers to draw: instances `first..end` of one model at the
/// level of detail the skinning budget left them.
#[derive(Clone, Copy)]
struct Draw {
    first: u32,
    end: u32,
    mesh: [u32; 2],
    shadow_mesh: [u32; 2],
    textured: bool,
}

pub(crate) struct ModelGpu {
    /// Each level's vertices, as stored in the pack; read by the skinning
    /// pass, and by the world pass for colour and texture coordinates.
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    /// Per instance, where its skinned vertices start (see `prepare`).
    pub origins: wgpu::Buffer,
    /// What the skinning pass poses (`Jobs` in world.wgsl).
    pub jobs: wgpu::Buffer,
    /// Where the skinning pass writes: 12 bytes of position, then 12 of
    /// normal and tangent, per vertex, `skin_capacity` vertices.
    skin_pos: wgpu::Buffer,
    skin_shade: wgpu::Buffer,
    skin_capacity: usize,
    /// What the bind groups hold besides buffers.
    bones: wgpu::TextureView,
    srgb: wgpu::TextureView,
    linear: wgpu::TextureView,
    sampler: wgpu::Sampler,
    /// For the shadow and world passes.
    pub bind_group: wgpu::BindGroup,
    /// For the skinning pass.
    pub skin_group: wgpu::BindGroup,
    /// Per model: its levels, and whether it is drawn textured.
    pub levels: Vec<([Level; LODS], bool)>,
    /// Everything but the skinned buffers.
    fixed_bytes: u64,
}

impl ModelGpu {
    pub fn gpu_bytes(&self) -> u64 {
        self.fixed_bytes + self.skin_pos.size() + self.skin_shade.size()
    }

    fn parts(&self) -> Parts<'_> {
        Parts {
            vertices: &self.vertices,
            origins: &self.origins,
            jobs: &self.jobs,
            skin_pos: &self.skin_pos,
            skin_shade: &self.skin_shade,
            bones: &self.bones,
            srgb: &self.srgb,
            linear: &self.linear,
            sampler: &self.sampler,
        }
    }

    /// Hold at least `vertices` skinned vertices, rounded up to a power of
    /// two (`SKIN_VERTICES` at most). The buffers are written afresh every
    /// frame, so nothing is copied.
    fn grow(&mut self, device: &wgpu::Device, soldiers: &SoldiersGpu, vertices: usize) {
        if vertices <= self.skin_capacity {
            return;
        }
        self.skin_capacity = vertices.next_power_of_two().min(SKIN_VERTICES);
        (self.skin_pos, self.skin_shade) = skin_buffers(device, self.skin_capacity);
        (self.bind_group, self.skin_group) = bind_groups(device, soldiers, &self.parts());
    }
}

/// The two buffers the skinning pass writes, for `capacity` vertices.
fn skin_buffers(device: &wgpu::Device, capacity: usize) -> (wgpu::Buffer, wgpu::Buffer) {
    let make = |label| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: (capacity * 12) as u64,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        })
    };
    (make("skinned positions"), make("skinned normals"))
}

/// A buffer, whole, as the resource of a binding.
fn entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
    wgpu::BindGroupEntry {
        binding,
        resource: buffer.as_entire_binding(),
    }
}

/// What the bind groups bind.
struct Parts<'a> {
    vertices: &'a wgpu::Buffer,
    origins: &'a wgpu::Buffer,
    jobs: &'a wgpu::Buffer,
    skin_pos: &'a wgpu::Buffer,
    skin_shade: &'a wgpu::Buffer,
    bones: &'a wgpu::TextureView,
    srgb: &'a wgpu::TextureView,
    linear: &'a wgpu::TextureView,
    sampler: &'a wgpu::Sampler,
}

/// The bind groups of the passes that draw and of the skinning pass.
fn bind_groups(
    device: &wgpu::Device,
    soldiers: &SoldiersGpu,
    m: &Parts<'_>,
) -> (wgpu::BindGroup, wgpu::BindGroup) {
    let draw = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("soldier models"),
        layout: &soldiers.models_layout,
        entries: &[
            entry(0, m.skin_pos),
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(m.srgb),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(m.linear),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(m.sampler),
            },
            entry(4, m.skin_shade),
            entry(5, m.origins),
        ],
    });
    let skin = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("soldier skinning"),
        layout: &soldiers.skin_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::TextureView(m.bones),
            },
            entry(9, m.vertices),
            entry(10, &soldiers.instances),
            entry(11, m.jobs),
            entry(12, m.skin_pos),
            entry(13, m.skin_shade),
        ],
    });
    (draw, skin)
}

pub(crate) struct SoldiersGpu {
    /// For models painted in vertex colours.
    pub pipeline: wgpu::RenderPipeline,
    pub textured_pipeline: wgpu::RenderPipeline,
    /// Depth only, for the sun's shadow map; any model.
    pub shadow_pipeline: wgpu::RenderPipeline,
    /// Poses every soldier's vertices before the passes that draw them.
    skin_pipeline: wgpu::ComputePipeline,
    pub models_layout: wgpu::BindGroupLayout,
    pub skin_layout: wgpu::BindGroupLayout,
    pub instances: wgpu::Buffer,
    pub models: Option<ModelGpu>,
    /// This frame's runs of soldiers, from `prepare`.
    draws: Vec<Draw>,
    /// Scratch for what `prepare` uploads.
    jobs: Vec<[u32; 4]>,
    origins: Vec<u32>,
    /// Vertices the skinning pass poses this frame.
    threads: u32,
}

pub(crate) fn make(
    device: &wgpu::Device,
    world_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
    format: wgpu::TextureFormat,
) -> SoldiersGpu {
    let materials = |binding| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    };
    let storage = |binding, visibility, read_only| wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let vertex = wgpu::ShaderStages::VERTEX;
    let compute = wgpu::ShaderStages::COMPUTE;
    let models_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("soldier models"),
        entries: &[
            // The skinned positions, then the material array seen as sRGB
            // (colour layers) and as linear (the normal layer), its
            // sampler, the skinned normals and tangents, and where each
            // instance's skinned vertices start.
            storage(0, vertex, true),
            materials(1),
            materials(2),
            wgpu::BindGroupLayoutEntry {
                binding: 3,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
            storage(4, vertex, true),
            storage(5, vertex, true),
        ],
    });
    let skin_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("soldier skinning"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 8,
                visibility: compute,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            storage(9, compute, true),
            storage(10, compute, true),
            wgpu::BindGroupLayoutEntry {
                binding: 11,
                visibility: compute,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            storage(12, compute, false),
            storage(13, compute, false),
        ],
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("soldiers"),
        bind_group_layouts: &[Some(world_layout), Some(&models_layout)],
        immediate_size: 0,
    });
    let skin_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("soldier skinning"),
        bind_group_layouts: &[Some(world_layout), Some(&skin_layout)],
        immediate_size: 0,
    });
    let skin_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("soldier skinning"),
        layout: Some(&skin_pipeline_layout),
        module: shader,
        entry_point: Some("cs_skin"),
        compilation_options: Default::default(),
        cache: None,
    });
    let pipeline = |label, vs, target: crate::Target<'_>, cull_mode, buffers: &[_]| {
        let (targets, fs) = target.color();
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some(label),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: shader,
                entry_point: Some(vs),
                compilation_options: Default::default(),
                buffers,
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode,
                ..Default::default()
            },
            depth_stencil: Some(target.depth_state()),
            multisample: Default::default(),
            fragment: fs.map(|fs| wgpu::FragmentState {
                module: shader,
                entry_point: Some(fs),
                compilation_options: Default::default(),
                targets: &targets,
            }),
            multiview_mask: None,
            cache: None,
        })
    };
    let buffers = [
        Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<ModelVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &MODEL_ATTRIBUTES,
        }),
        Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<SoldierInstance>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INSTANCE_ATTRIBUTES,
        }),
    ];
    // The low-poly packs model cloaks and blades as single faces; textured
    // models are built closed, so their back faces can be skipped.
    let textured_pipeline = pipeline(
        "textured soldiers",
        "vs_soldier_textured",
        crate::Target::Color(format, "fs_soldier_textured"),
        Some(wgpu::Face::Back),
        &buffers,
    );
    // The shadow map needs only the skinned positions, no vertex buffers.
    let shadow_pipeline = pipeline(
        "soldier shadows",
        "vs_soldier_depth",
        crate::Target::Shadow,
        None,
        &[],
    );
    let pipeline = pipeline(
        "soldiers",
        "vs_soldier",
        crate::Target::Color(format, "fs_soldier"),
        None,
        &buffers,
    );
    let instances = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("soldier instances"),
        size: (MAX_SOLDIER_INSTANCES * size_of::<SoldierInstance>()) as u64,
        usage: wgpu::BufferUsages::VERTEX
            | wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    SoldiersGpu {
        pipeline,
        textured_pipeline,
        shadow_pipeline,
        skin_pipeline,
        models_layout,
        skin_layout,
        instances,
        models: None,
        draws: Vec::new(),
        jobs: Vec::new(),
        origins: Vec::new(),
        threads: 0,
    }
}

impl SoldiersGpu {
    /// Plan the frame's soldiers: the first `count` uploaded instances by
    /// `batches`, each at its level of detail. Gives every soldier a place
    /// for its skinned vertices and uploads where they are. A frame that
    /// would pose more than [`SKIN_VERTICES`] first draws the finest levels
    /// (wherever they are) at the next coarser level, and only when all are
    /// at the coarsest cuts soldiers, from the last batch back. Returns how
    /// many were left out.
    pub fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        count: usize,
        batches: &[SoldierBatch],
    ) -> usize {
        self.draws.clear();
        self.jobs.clear();
        self.threads = 0;
        let Some(models) = self.models.as_ref().filter(|_| count > 0) else {
            return 0;
        };
        self.origins.clear();
        self.origins.resize(count, 0);
        // The runs to draw, in the batches' order (which keeps the pipeline
        // switches the caller chose).
        struct Run {
            first: u32,
            n: usize,
            lod: usize,
            levels: [Level; LODS],
            textured: bool,
        }
        let mut runs = Vec::with_capacity(batches.len());
        for b in batches {
            let Some(&(levels, textured)) = models.levels.get(b.model as usize) else {
                continue;
            };
            let end = (b.first + b.count).min(count as u32);
            // A level with no vertices is nothing to pose or draw.
            if b.first >= end || levels.iter().any(|l| l.len == 0) {
                continue;
            }
            runs.push(Run {
                first: b.first,
                n: (end - b.first) as usize,
                lod: (b.lod as usize).min(LODS - 1),
                levels,
                textured,
            });
        }
        // Each instance has one origin, so runs must not share instances.
        debug_assert!(
            {
                let mut spans: Vec<_> = runs.iter().map(|r| (r.first, r.n)).collect();
                spans.sort_unstable();
                spans
                    .windows(2)
                    .all(|w| w[0].0 as usize + w[0].1 <= w[1].0 as usize)
            },
            "soldier batches overlap"
        );
        let need = |r: &Run| r.levels[r.lod].len as usize * r.n;
        let mut total: usize = runs.iter().map(need).sum();
        for lod in 0..LODS - 1 {
            for r in runs.iter_mut().filter(|r| r.lod == lod) {
                if total <= SKIN_VERTICES {
                    break;
                }
                total -= need(r);
                r.lod += 1;
                total += need(r);
            }
        }
        let mut left_out = 0;
        for r in runs.iter_mut().rev() {
            if total <= SKIN_VERTICES {
                break;
            }
            let len = r.levels[r.lod].len as usize;
            let cut = (total - SKIN_VERTICES).div_ceil(len).min(r.n);
            r.n -= cut;
            total -= cut * len;
            left_out += cut;
        }
        // A job per run, as many as the pack's models and levels can make
        // (`set_models` checks that the shader's table holds them).
        debug_assert!(runs.len() <= MAX_JOBS, "more soldier batches than jobs");
        let (mut slots, mut top) = (0, 0);
        for (k, r) in runs.iter().enumerate() {
            if k >= MAX_JOBS {
                left_out += r.n;
                continue;
            }
            if r.n == 0 {
                continue;
            }
            let level = r.levels[r.lod];
            // Soldier `i` of the run has its vertices from `slots + i *
            // len`, in the order of the level's. The shaders find them by
            // adding the vertex index, which counts from the start of the
            // shared vertex buffer.
            for i in 0..r.n {
                self.origins[r.first as usize + i] =
                    (slots as u32 + i as u32 * level.len).wrapping_sub(level.first);
            }
            self.jobs
                .push([r.first, level.first, level.len, slots as u32]);
            self.draws.push(Draw {
                first: r.first,
                end: r.first + r.n as u32,
                mesh: level.mesh,
                shadow_mesh: level.shadow_mesh,
                textured: r.textured,
            });
            slots += r.n * level.len as usize;
            top = top.max(r.first as usize + r.n);
        }
        self.threads = slots as u32;
        if slots > 0 {
            // Room for this frame's vertices (the bind groups change with
            // the buffers, so the borrow is taken afresh).
            if let Some(mut models) = self.models.take() {
                models.grow(device, self, slots);
                self.models = Some(models);
            }
            let models = self.models.as_ref().expect("models were just put back");
            let header = [self.jobs.len() as u32, self.threads, 0, 0];
            queue.write_buffer(&models.jobs, 0, bytemuck::bytes_of(&header));
            queue.write_buffer(&models.jobs, 16, bytemuck::cast_slice(&self.jobs));
            queue.write_buffer(
                &models.origins,
                0,
                bytemuck::cast_slice(&self.origins[..top]),
            );
        }
        left_out
    }

    /// Pose the planned soldiers' vertices. `world` is the camera's bind
    /// group, which holds the terrain the soldiers stand on.
    pub fn skin(&self, encoder: &mut wgpu::CommandEncoder, world: &wgpu::BindGroup) {
        let Some(models) = self.models.as_ref().filter(|_| self.threads > 0) else {
            return;
        };
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("skin soldiers"),
            timestamp_writes: None,
        });
        pass.set_pipeline(&self.skin_pipeline);
        pass.set_bind_group(0, world, &[]);
        pass.set_bind_group(1, &models.skin_group, &[]);
        pass.dispatch_workgroups(self.threads.div_ceil(SKIN_GROUP), 1, 1);
    }

    /// Draw the planned soldiers into the frame or (with `shadow`) the
    /// shadow map. Returns draw calls, soldiers and triangles drawn.
    pub fn draw(&self, pass: &mut wgpu::RenderPass<'_>, shadow: bool) -> (u32, u32, u64) {
        let (mut calls, mut drawn, mut tris) = (0, 0, 0);
        let Some(models) = self.models.as_ref().filter(|_| !self.draws.is_empty()) else {
            return (calls, drawn, tris);
        };
        pass.set_bind_group(1, &models.bind_group, &[]);
        pass.set_index_buffer(models.indices.slice(..), wgpu::IndexFormat::Uint32);
        if shadow {
            pass.set_pipeline(&self.shadow_pipeline);
        } else {
            pass.set_vertex_buffer(0, models.vertices.slice(..));
            pass.set_vertex_buffer(1, self.instances.slice(..));
        }
        let mut textured_now = None;
        for d in &self.draws {
            if !shadow && textured_now != Some(d.textured) {
                pass.set_pipeline(if d.textured {
                    &self.textured_pipeline
                } else {
                    &self.pipeline
                });
                textured_now = Some(d.textured);
            }
            // The sun only needs a silhouette, so its pass never draws the
            // finer meshes, however close the soldier is to the camera.
            let [first, len] = if shadow { d.shadow_mesh } else { d.mesh };
            pass.draw_indexed(first..first + len, 0, d.first..d.end);
            calls += 1;
            drawn += d.end - d.first;
            tris += u64::from(len / 3) * u64::from(d.end - d.first);
        }
        (calls, drawn, tris)
    }
}

pub(crate) fn upload(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    soldiers: &SoldiersGpu,
    m: &Models<'_>,
) -> ModelGpu {
    let buffer = |label, bytes: &[u8], usage| {
        let b = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            // Buffers must be a multiple of 4 bytes.
            size: (bytes.len() as u64).next_multiple_of(4).max(4),
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut padded = bytes.to_vec();
        padded.resize(b.size() as usize, 0);
        queue.write_buffer(&b, 0, &padded);
        b
    };
    let empty = |label, size: usize, usage| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: size as u64,
            usage,
            mapped_at_creation: false,
        })
    };
    let levels = level_vertices(m);
    let vertices = buffer(
        "model vertices",
        bytemuck::cast_slice(&levels.vertices),
        wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
    );
    let indices = buffer(
        "model indices",
        bytemuck::cast_slice(&levels.indices),
        wgpu::BufferUsages::INDEX,
    );
    let origins = empty(
        "skin origins",
        MAX_SOLDIER_INSTANCES * 4,
        wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
    );
    let jobs = empty(
        "skin jobs",
        16 + MAX_JOBS * 16,
        wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    );
    let rows = m.bone_rows().max(1);
    let size = wgpu::Extent3d {
        width: BONE_TEXELS_PER_ROW,
        height: rows,
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("bone animation"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut texels: Vec<[u16; 4]> = m.bones.to_vec();
    texels.resize((BONE_TEXELS_PER_ROW * rows) as usize, [0; 4]);
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytemuck::cast_slice(&texels),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(BONE_TEXELS_PER_ROW * 8),
            rows_per_image: Some(rows),
        },
        size,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    // Without BC7 the textured models draw in their vertex colours.
    let textures = device
        .features()
        .contains(wgpu::Features::TEXTURE_COMPRESSION_BC)
        && m.materials() > 0;
    let materials = upload_materials(device, queue, m, textures);
    let array_view = |format: wgpu::TextureFormat| {
        materials.create_view(&wgpu::TextureViewDescriptor {
            format: Some(format),
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        })
    };
    let linear = materials.format();
    let srgb_view = array_view(linear.add_srgb_suffix());
    let linear_view = array_view(linear);
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("soldier materials"),
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: 4,
        ..Default::default()
    });
    let levels_of = levels.models.iter().zip(m.models).map(|(levels, r)| {
        let textured = textures
            && m.vertices
                .get(r.first_vertex as usize)
                .is_some_and(|v| v.material > 0);
        (*levels, textured)
    });
    let (skin_pos, skin_shade) = skin_buffers(device, SKIN_START);
    let (bind_group, skin_group) = bind_groups(
        device,
        soldiers,
        &Parts {
            vertices: &vertices,
            origins: &origins,
            jobs: &jobs,
            skin_pos: &skin_pos,
            skin_shade: &skin_shade,
            bones: &view,
            srgb: &srgb_view,
            linear: &linear_view,
            sampler: &sampler,
        },
    );
    ModelGpu {
        fixed_bytes: vertices.size()
            + indices.size()
            + origins.size()
            + jobs.size()
            + (texels.len() * 8) as u64
            + if textures { size_of_val(m.textures) } else { 4 } as u64,
        vertices,
        indices,
        origins,
        jobs,
        skin_pos,
        skin_shade,
        skin_capacity: SKIN_START,
        bones: view,
        srgb: srgb_view,
        linear: linear_view,
        sampler,
        bind_group,
        skin_group,
        levels: levels_of.collect(),
    }
}

/// The material texture array, every layer with its mips (BC7); a single
/// blank texel without `textures`, since the binding must exist.
fn upload_materials(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    m: &Models<'_>,
    textures: bool,
) -> wgpu::Texture {
    let (format, side, mips, layers) = if textures {
        let layers = (m.materials() * LAYERS_PER_MATERIAL) as u32;
        let format = wgpu::TextureFormat::Bc7RgbaUnorm;
        (format, TEXTURE_SIZE, TEXTURE_MIPS, layers)
    } else {
        (wgpu::TextureFormat::Rgba8Unorm, 1, 1, 1)
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("soldier materials"),
        size: wgpu::Extent3d {
            width: side,
            height: side,
            depth_or_array_layers: layers,
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[format.add_srgb_suffix()],
    });
    if !textures {
        return texture;
    }
    for (layer, blocks) in m
        .textures
        .as_chunks::<BLOCKS_PER_LAYER>()
        .0
        .iter()
        .enumerate()
    {
        let mut offset = 0;
        for mip in 0..TEXTURE_MIPS {
            let side = TEXTURE_SIZE >> mip;
            let across = side / BLOCK_SIDE;
            let count = (across * across) as usize;
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: mip,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: layer as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&blocks[offset..offset + count]),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(across * size_of::<Block>() as u32),
                    rows_per_image: Some(across),
                },
                wgpu::Extent3d {
                    width: side,
                    height: side,
                    depth_or_array_layers: 1,
                },
            );
            offset += count;
        }
    }
    texture
}

#[cfg(test)]
mod tests {
    use super::*;
    use gc_assets::model::ModelRecord;

    /// One model of six vertices (a strip), with a different mesh at each
    /// level of detail.
    fn strip() -> (Vec<ModelRecord>, Vec<ModelVertex>, Vec<u16>) {
        let mut record = ModelRecord::zeroed();
        record.vertex_count = 6;
        // Level 0 uses all six vertices, 1 uses 0 to 3, 2 uses 0, 2 and 4,
        // and 3 uses 0, 2 and 5.
        let meshes: [&[u16]; LODS] = [
            &[0, 1, 2, 1, 3, 2, 2, 3, 4, 3, 5, 4],
            &[0, 1, 2, 1, 3, 2],
            &[0, 2, 4],
            &[0, 2, 5],
        ];
        let mut indices = Vec::new();
        for (lod, mesh) in meshes.iter().enumerate() {
            record.lods[lod] = [indices.len() as u32, mesh.len() as u32];
            indices.extend_from_slice(mesh);
        }
        let vertices = (0..6)
            .map(|i| {
                let mut v = ModelVertex::zeroed();
                v.pos = [i, 0, 0];
                v
            })
            .collect();
        (vec![record], vertices, indices)
    }

    fn models<'a>(
        records: &'a [ModelRecord],
        vertices: &'a [ModelVertex],
        indices: &'a [u16],
    ) -> Models<'a> {
        Models {
            models: records,
            vertices,
            indices,
            clips: &[],
            bones: &[],
            textures: &[],
        }
    }

    #[test]
    fn a_level_holds_only_the_vertices_its_meshes_use() {
        let (records, vertices, indices) = strip();
        let m = models(&records, &vertices, &indices);
        let out = level_vertices(&m);
        let [l0, l1, l2, l3] = out.models[0];
        // The finest level poses everything. The others pose what their own
        // mesh and the shadow mesh (level 2 at the finest) use together.
        assert_eq!([l0.len, l1.len, l2.len, l3.len], [6, 5, 3, 3]);
        assert_eq!(out.vertices.len(), 17);
        // Levels follow each other in the buffer.
        assert_eq!(l1.first, 6);
        assert_eq!(l2.first, 11);
        assert_eq!(l3.first, 14);
    }

    #[test]
    fn indices_still_name_the_same_vertices() {
        let (records, vertices, indices) = strip();
        let m = models(&records, &vertices, &indices);
        let out = level_vertices(&m);
        for (lod, level) in out.models[0].iter().enumerate() {
            let shadow_lod = lod.max(SHADOW_MIN_LOD);
            for (mesh, source) in [(level.mesh, lod), (level.shadow_mesh, shadow_lod)] {
                let [first, len] = mesh;
                let [from, count] = records[0].lods[source];
                assert_eq!(len, count);
                for k in 0..len as usize {
                    let new = out.indices[first as usize + k] as usize;
                    // Within the level's own vertices, and the same one.
                    assert!(
                        (level.first as usize..(level.first + level.len) as usize).contains(&new)
                    );
                    let old = indices[from as usize + k] as usize;
                    assert_eq!(out.vertices[new].pos, vertices[old].pos);
                }
            }
        }
    }

    #[test]
    fn the_shadow_mesh_is_never_finer_than_the_limit() {
        let (records, vertices, indices) = strip();
        let m = models(&records, &vertices, &indices);
        let out = level_vertices(&m);
        let [l0, l1, l2, l3] = out.models[0];
        assert_eq!(l0.shadow_mesh[1], records[0].lods[SHADOW_MIN_LOD][1]);
        assert_eq!(l1.shadow_mesh[1], records[0].lods[SHADOW_MIN_LOD][1]);
        // From there on the sun draws what the camera does.
        assert_eq!(l2.shadow_mesh, l2.mesh);
        assert_eq!(l3.shadow_mesh, l3.mesh);
    }
}
